const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const path=require('node:path');

// Execute every script in page order so competing HTML listeners are covered.
function loadPage() {
  const elements=new Map();
  const timers=new Map();
  let nextTimer=0;
  class Element {
    constructor(){this.value='';this.textContent='';this.style={};this.children=[];this.disabled=false;this.className='';this.onclick=null;this.onchange=null;this.onkeydown=null;this.onmousedown=null;this.parentNode=null;this.listeners={};}
    appendChild(item){item.parentNode=this;this.children.push(item);return item}
    append(item){this.appendChild(item)}
    addEventListener(type,fn){(this.listeners[type]||= []).push(fn)}
    dispatch(type,event={}){for(const fn of this.listeners[type]||[])fn(event);this['on'+type]?.(event)}
    setAttribute(){}
    set innerHTML(value){this.children=[];this._innerHTML=value}
    get innerHTML(){return this._innerHTML||''}
  }
  const settings=new Element();
  const get=id=>{if(!elements.has(id)){const item=new Element();if(id==='bridge')item.parentNode=settings;elements.set(id,item)}return elements.get(id)};
  class Socket {
    static instances=[];
    static OPEN=1;
    constructor(){this.readyState=0;this.sent=[];Socket.instances.push(this)}
    send(raw){this.sent.push(JSON.parse(raw))}
    close(){this.readyState=3;if(this.onclose)this.onclose()}
    open(){this.readyState=1;this.onopen()}
    reply(message){this.onmessage({data:JSON.stringify(message)})}
  }
  const context=vm.createContext({
    document:{getElementById:get,createElement:()=>new Element()},WebSocket:Socket,
    setTimeout:fn=>{fn();return 1},clearTimeout:()=>{},
    setInterval:fn=>{timers.set(++nextTimer,fn);return nextTimer},clearInterval:id=>timers.delete(id),
    Date,Math,crypto:{randomUUID:()=> 'test-session'},performance:{now:()=>Date.now()},
  });
  const html=fs.readFileSync(path.join(__dirname,'../iframe/index.html'),'utf8');
  for(const match of html.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script>/g)) {
    const src=/src="([^"]+)"/.exec(match[1]);
    const source=src?fs.readFileSync(path.join(__dirname,'..',src[1]),'utf8'):match[2];
    vm.runInContext(source,context);
  }
  return {get,settings,Socket,timers};
}

function selectProject(page) {
  const {get,Socket}=page;
  const socket=Socket.instances.at(-1);socket.open();
  assert.deepEqual(socket.sent[0],{type:'hello',protocolVersion:1});
  socket.reply({type:'hello_ack'});
  socket.reply({type:'projects',projects:[{id:'power-supply',name:'电源'}]});
  const projectSelect=get('projectSelect');
  projectSelect.value='power-supply';projectSelect.onchange();
  assert.deepEqual(socket.sent.at(-1),{type:'select_project',projectId:'power-supply'});
  socket.reply({type:'project_selected',projectId:'power-supply'});
  assert.equal(get('send').disabled,false);
  return socket;
}

test('approved plans require a separate apply click and missing readback stays uncertain with rollback and cancel controls',()=>{
  const page=loadPage();const socket=selectProject(page);const {get}=page;
  socket.reply({type:'preview_result',planHash:'plan',baselineSnapshotHash:'base',approvalAllowed:true,plan:{steps:[],diff:[],risks:[],rollbackable:true}});
  assert.equal(get('applyPlan').disabled,true);
  get('approvePlan').onclick();socket.reply({type:'approval_result',planHash:'plan',decision:'approved'});
  assert.equal(socket.sent.filter(m=>m.type==='apply').length,0);assert.equal(get('applyPlan').disabled,false);
  get('applyPlan').onclick();assert.equal(socket.sent.at(-1).type,'apply');assert.equal(socket.sent.at(-1).planHash,'plan');
  socket.reply({type:'operation_started',operation:'apply',requestId:'op-1'});
  assert.equal(get('cancelPlan').disabled,false);get('cancelPlan').onclick();assert.equal(socket.sent.at(-1).type,'cancel_operation');
  assert.equal(socket.sent.at(-1).requestId,'op-1');
  socket.reply({type:'apply_result',operation:'apply',requestId:'op-1',planHash:'plan',ok:false,rollbackHandle:'handle',execution:{write:{status:'passed'},readback:{status:'inconclusive'},verification:{status:'inconclusive'}}});
  assert.equal(get('live').className,'live uncertain');assert.equal(get('applyPlan').disabled,true);assert.equal(get('rollbackPlan').disabled,false);
  assert.match(get('planDetails').textContent,/write=passed.*readback=inconclusive.*verification=inconclusive/);
  get('rollbackPlan').onclick();assert.equal(socket.sent.at(-1).type,'rollback');
  socket.reply({type:'operation_started',operation:'rollback',requestId:'op-2'});
  socket.reply({type:'rollback_result',operation:'rollback',requestId:'op-2',planHash:'plan',ok:true,execution:{write:{status:'passed'},readback:{status:'passed'},verification:{status:'passed'}}});
  assert.equal(get('live').className,'live done');
});

test('rollback warnings are shown prominently and approval stays available',()=>{
  const page=loadPage();const socket=selectProject(page);const {get}=page;
  socket.reply({type:'preview_result',planHash:'plan',baselineSnapshotHash:'base',approvalAllowed:true,
    plan:{steps:[],diff:[],risks:[],rollbackable:false,rollbackWarnings:['删除 U9 后无法自动回滚']}});
  assert.equal(get('rollbackWarnings').style.display,'');
  assert.match(get('rollbackWarnings').textContent,/回滚提醒[\s\S]*U9/);
  assert.match(get('approvePlan').textContent,/已知晓回滚风险/);
  assert.equal(get('approvePlan').disabled,false);
  assert.match(get('planDetails').textContent,/具备补偿计划：否（见回滚提醒）/);
  socket.reply({type:'preview_result',planHash:'plan2',baselineSnapshotHash:'base',approvalAllowed:true,
    plan:{steps:[],diff:[],risks:[],rollbackable:true}});
  assert.equal(get('rollbackWarnings').style.display,'none');
  assert.equal(get('approvePlan').textContent,'批准计划（不执行）');
});

test('Enter sends exactly one chat on the selected project connection',()=>{
  const page=loadPage();
  const socket=selectProject(page);
  const {get,Socket}=page;
  get('instruction').value='检查电源';
  let prevented=0;
  get('instruction').dispatch('keydown',{key:'Enter',shiftKey:false,preventDefault:()=>prevented++});
  assert.equal(Socket.instances.filter(item=>item.readyState!==3).length,1);
  assert.equal(get('connection').textContent,'Bridge 已连接');
  const chats=socket.sent.filter(message=>message.type==='chat');
  assert.equal(chats.length,1);
  assert.equal(chats[0].text,'检查电源');
  assert.ok(chats[0].sessionId);
  assert.equal(prevented,1);
  get('instruction').value='重复发送';get('send').onclick();
  assert.equal(socket.sent.filter(message=>message.type==='chat').length,1);
  socket.reply({type:'chat_started'});
  socket.reply({type:'chat_delta',delta:'检查结果'});
  socket.reply({type:'chat_completed'});
  assert.equal(get('send').disabled,false);
  assert.equal(get('liveState').textContent,'当前状态：执行完成');
  assert.equal(page.timers.size,0);
});

test('loading a historical execution restores rollback controls without restoring apply authorization', () => {
  const page = loadPage(); const socket = selectProject(page); const {get} = page;
  socket.reply({type:'session_loaded',projectId:'power-supply',sessionId:'original-session',body:'历史',
    review:{type:'preview_result',sessionId:'original-session',planHash:'original-plan',baselineSnapshotHash:'old-base',
      approvalAllowed:false,approved:false,started:true,rollbackHandle:'original-handle',plan:{steps:[],diff:[],risks:[],rollbackable:true}}});
  assert.equal(get('applyPlan').disabled, true);
  assert.equal(get('approvePlan').disabled, true);
  assert.equal(get('rollbackPlan').disabled, false);
  get('rollbackPlan').onclick();
  assert.equal(socket.sent.at(-1).sessionId, 'original-session');
  assert.equal(socket.sent.at(-1).planHash, 'original-plan');
  socket.reply({type:'operation_started',operation:'rollback',requestId:'r'});
  socket.reply({type:'rollback_result',operation:'rollback',requestId:'r',planHash:'original-plan',ok:false,
    failure:'operation_timeout：执行器结果未知',execution:{verification:{status:'inconclusive'}}});
  assert.match(get('planDetails').textContent, /operation_timeout/);
  assert.equal(get('live').className, 'live uncertain');
});

test('click reuses the socket; Shift+Enter and IME Enter do not send',()=>{
  const page=loadPage();const socket=selectProject(page);const {get}=page;
  get('instruction').value='检查电源';
  for(const event of [{key:'Enter',shiftKey:true},{key:'Enter',isComposing:true},{key:'Enter',keyCode:229}]) {
    get('instruction').dispatch('keydown',{...event,preventDefault:()=>assert.fail('must allow composition/newline')});
  }
  assert.equal(socket.sent.filter(message=>message.type==='chat').length,0);
  get('send').onclick();
  assert.equal(socket.sent.at(-1).type,'chat');
  assert.equal(socket.sent.at(-1).text,'检查电源');
  assert.equal(page.Socket.instances.length,1);
  socket.reply({type:'error',message:'Provider 不可用'});
  assert.equal(get('send').disabled,false);
  assert.equal(get('livePhase').textContent,'Provider 不可用');
  assert.equal(page.timers.size,0);
});

test('reconnect closes the old socket and requires project selection again',()=>{
  const page=loadPage();const old=selectProject(page);const {get,Socket}=page;
  get('instruction').value='检查电源';get('send').onclick();
  get('refresh').onclick();
  assert.equal(old.readyState,3);
  assert.equal(get('send').disabled,true);
  assert.equal(page.timers.size,0);
  const socket=selectProject(page);
  old.onclose();old.reply({type:'error',message:'旧连接错误'});
  assert.equal(get('send').disabled,false);
  assert.equal(get('connection').textContent,'Bridge 已连接');
  get('instruction').value='重连后发送';get('send').onclick();
  assert.equal(socket.sent.at(-1).text,'重连后发送');
  assert.equal(Socket.instances.length,2);
});

test('the composer exposes the observation, DRC and capture buttons as secondary actions',()=>{
  const html=fs.readFileSync(path.join(__dirname,'../iframe/index.html'),'utf8');
  assert.match(html,/id="inspectBtn"[^>]*class="secondary"[^>]*disabled/);
  assert.match(html,/id="drcBtn"[^>]*class="secondary"[^>]*disabled/);
  assert.match(html,/id="captureBtn"[^>]*class="secondary"[^>]*disabled/);
});

test('the capture button requests visual evidence and renders its content hash',()=>{
  const page=loadPage();const socket=selectProject(page);const {get}=page;
  assert.equal(get('captureBtn').disabled,false);
  get('captureBtn').onclick();
  const request=socket.sent.at(-1);
  assert.equal(request.type,'visual_capture');
  assert.ok(request.sessionId);
  assert.deepEqual(request.document,{kind:'schematic'});
  assert.equal(get('captureBtn').disabled,true);
  socket.reply({type:'operation_started',requestId:'op-5',operation:'visual_capture'});
  socket.reply({type:'visual_capture_result',requestId:'op-5',ok:true,
    evidence:{contentHash:'sha256:0011223344556677',mime:'image/png',bytes:12345},inconclusive:[]});
  assert.equal(get('liveState').textContent,'当前状态：执行完成');
  const conversation=get('conversation');
  const last=conversation.children[conversation.children.length-1];
  assert.match(last.children[0].textContent,/画布截图已采集/);
  assert.match(last.children[0].textContent,/0011223344556677/);
  assert.match(last.children[0].textContent,/12345 字节/);
  assert.equal(get('captureBtn').disabled,false);
});
