const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const path=require('node:path');

function loadPage(){
  const elements=new Map();
  class Element {
    constructor(){this.value='';this.textContent='';this.style={};this.children=[];this.disabled=false;this.className='';this.onclick=null;this.onchange=null;this.onkeydown=null;this.onmousedown=null;this.parentNode=null;}
    appendChild(item){item.parentNode=this;this.children.push(item);return item}
    setAttribute(){}
    set innerHTML(value){this.children=[];this._innerHTML=value}
    get innerHTML(){return this._innerHTML||''}
  }
  const get=id=>{if(!elements.has(id))elements.set(id,new Element());return elements.get(id)};
  class Socket {
    static instances=[];
    constructor(){this.readyState=0;this.sent=[];Socket.instances.push(this)}
    send(raw){this.sent.push(JSON.parse(raw))}
    close(){this.readyState=3;if(this.onclose)this.onclose()}
    open(){this.readyState=1;this.onopen()}
    reply(message){this.onmessage({data:JSON.stringify(message)})}
  }
  const context=vm.createContext({
    document:{getElementById:get,createElement:tag=>new Element()},WebSocket:Socket,
    setTimeout:fn=>{fn();return 1},clearTimeout:()=>{},setInterval:()=>0,clearInterval:()=>{},Date,Math,
  });
  const source=fs.readFileSync(path.join(__dirname,'../iframe/compat.js'),'utf8');
  vm.runInContext(source,context);
  return {get,Socket};
}

function connectAndList(page,projects){
  const socket=page.Socket.instances.at(-1);socket.open();
  assert.deepEqual(socket.sent[0],{type:'hello',protocolVersion:1});
  socket.reply({type:'hello_ack'});
  assert.equal(socket.sent[1].type,'list_projects');
  socket.reply({type:'projects',projects});
  return socket;
}

function chooseProject(page,socket,id){
  const projectSelect=page.get('projectSelect');
  projectSelect.value=id;projectSelect.onchange();
  assert.deepEqual(socket.sent.at(-1),{type:'select_project',projectId:id});
  socket.reply({type:'project_selected',projectId:id});
}

test('project dropdown refreshes on open without losing the selection',()=>{
  const page=loadPage();const {get,Socket}=page;
  const projectSelect=get('projectSelect');
  assert.equal(projectSelect.disabled,true);
  assert.equal(projectSelect.children[0].textContent,'连接后选择项目');
  const socket=connectAndList(page,[]);
  assert.equal(get('send').disabled,true);
  assert.equal(projectSelect.children[0].textContent,'暂无已注册项目');
  // opening the dropdown asks the bridge for the current list, exactly once per click
  const before=socket.sent.length;
  projectSelect.onmousedown();
  assert.equal(socket.sent.length,before+1);
  assert.equal(socket.sent.at(-1).type,'list_projects');
  socket.reply({type:'projects',projects:[{id:'power-supply',name:'电源'}]});
  assert.equal(projectSelect.children.length,2);
  assert.equal(projectSelect.children[1].value,'power-supply');
  assert.equal(projectSelect.children[1].textContent,'电源 (power-supply)');
  chooseProject(page,socket,'power-supply');
  assert.equal(get('send').disabled,false);
  assert.equal(get('sessionSelect').disabled,false);
  // a later refresh keeps the chosen project selected
  projectSelect.onmousedown();
  assert.equal(socket.sent.at(-1).type,'list_projects');
  socket.reply({type:'projects',projects:[{id:'power-supply',name:'电源'},{id:'other',name:'其他'}]});
  assert.equal(projectSelect.value,'power-supply');
  // chat still flows over the one socket
  get('instruction').value='检查电源';get('send').onclick();
  assert.deepEqual(socket.sent.at(-1),{type:'chat',sessionId:socket.sent.at(-1).sessionId,text:'检查电源'});
  assert.equal(Socket.instances.length,1);
  // reconnect resets both dropdowns and asks for a fresh list
  get('refresh').onclick();const next=Socket.instances[1];next.open();
  assert.deepEqual(next.sent[0],{type:'hello',protocolVersion:1});
  assert.equal(get('projectSelect').children[0].textContent,'连接后选择项目');
  assert.equal(get('projectSelect').disabled,true);
});

test('session dropdown lists titles, loads saved sessions, and starts local ones',()=>{
  const page=loadPage();const {get,Socket}=page;
  const socket=connectAndList(page,[{id:'power-supply',name:'电源'}]);
  chooseProject(page,socket,'power-supply');
  assert.deepEqual(socket.sent.at(-1),{type:'list_sessions'});
  socket.reply({type:'sessions',projectId:'power-supply',sessions:[
    {sessionId:'eda-1',startedAt:'2026-10-20T01:00:00Z',status:'completed',title:'检查电源输入风险'},
    {sessionId:'eda-2',startedAt:'2026-10-19T01:00:00Z',status:'failed',title:''},
  ]});
  const sessionSelect=get('sessionSelect');
  assert.equal(sessionSelect.children.length,3);
  assert.equal(sessionSelect.children[0].textContent,'新会话');
  assert.equal(sessionSelect.children[0].value,'');
  // the bridge-provided title leads the option label
  assert.ok(sessionSelect.children[1].textContent.indexOf('检查电源输入风险')===0);
  assert.equal(sessionSelect.children[1].value,'eda-1');
  // without a title the label falls back to startedAt · status
  assert.ok(sessionSelect.children[2].textContent.indexOf('2026-10-19T01:00:00Z · failed')===0);
  // choosing an existing session loads it over the same socket
  sessionSelect.value='eda-1';sessionSelect.onchange();
  assert.deepEqual(socket.sent.at(-1),{type:'load_session',sessionId:'eda-1'});
  assert.equal(Socket.instances.length,1);
  socket.reply({type:'session_loaded',projectId:'power-supply',sessionId:'eda-1',body:'## 轮次与工具调用',status:'completed'});
  assert.equal(sessionSelect.value,'eda-1');
  assert.ok(get('session').textContent.indexOf('eda-1')!==-1);
  // choosing 新会话 only starts a local session; no load_session is sent
  const sentBefore=socket.sent.length;
  sessionSelect.value='';sessionSelect.onchange();
  assert.equal(socket.sent.length,sentBefore);
  assert.equal(socket.sent.filter(message=>message.type==='load_session').length,1);
  assert.ok(get('session').textContent.indexOf('当前会话：compat-')===0);
});

test('the sessions refresh after chat completion keeps the current session selected',()=>{
  const page=loadPage();const {get}=page;
  const socket=connectAndList(page,[{id:'power-supply',name:'电源'}]);
  chooseProject(page,socket,'power-supply');
  socket.reply({type:'sessions',projectId:'power-supply',sessions:[]});
  get('instruction').value='检查电源';get('send').onclick();
  const chatId=socket.sent.at(-1).sessionId;
  socket.reply({type:'chat_started'});
  socket.reply({type:'chat_completed'});
  assert.equal(socket.sent.at(-1).type,'list_sessions');
  socket.reply({type:'sessions',projectId:'power-supply',sessions:[
    {sessionId:chatId,startedAt:'2026-10-22T01:00:00Z',status:'completed',title:'检查电源'},
  ]});
  assert.equal(get('sessionSelect').value,chatId);
});

test('a rejected project selection clears the choice and reports the reason',()=>{
  const page=loadPage();const {get}=page;
  const socket=connectAndList(page,[{id:'power-supply',name:'电源'}]);
  const projectSelect=get('projectSelect');
  projectSelect.value='power-supply';projectSelect.onchange();
  assert.equal(get('send').disabled,true);
  socket.reply({type:'error',requestType:'select_project',message:'项目 `power-supply` 未注册'});
  assert.equal(projectSelect.value,'');
  assert.equal(get('send').disabled,true);
  assert.equal(get('livePhase').textContent,'项目 `power-supply` 未注册');
});

function lastBubbleText(page){
  const conversation=page.get('conversation');
  const last=conversation.children[conversation.children.length-1];
  return last.children[0].textContent;
}

test('inspect explains concrete missing fields instead of only naming a face',()=>{
  const page=loadPage();
  const socket=connectAndList(page,[{id:'power-supply',name:'电源'}]);
  chooseProject(page,socket,'power-supply');
  page.get('inspectBtn').onclick();
  socket.reply({type:'operation_started',operation:'inspect',requestId:'diag-1'});
  socket.reply({type:'inspect_result',requestId:'diag-1',components:34,nets:13,
    inconclusive:['components'],diagnostics:[{face:'components',code:'missing_designator',
      count:45,message:'对象缺少位号',examples:['Drawing-Symbol_A4','GND']}]});
  assert.match(lastBubbleText(page),/对象缺少位号/);
  assert.match(lastBubbleText(page),/45 项/);
  assert.match(lastBubbleText(page),/Drawing-Symbol_A4、GND/);
  assert.equal(page.get('live').className,'live uncertain');
});

test('operation buttons stay disabled until a project is ready and render the inspect result',()=>{
  const page=loadPage();const {get}=page;
  assert.equal(get('inspectBtn').disabled,true);
  assert.equal(get('drcBtn').disabled,true);
  assert.equal(get('captureBtn').disabled,true);
  const socket=connectAndList(page,[{id:'power-supply',name:'电源'}]);
  // connected but no project yet: still disabled
  assert.equal(get('inspectBtn').disabled,true);
  chooseProject(page,socket,'power-supply');
  assert.equal(get('inspectBtn').disabled,false);
  assert.equal(get('drcBtn').disabled,false);
  assert.equal(get('captureBtn').disabled,false);
  get('inspectBtn').onclick();
  const request=socket.sent.at(-1);
  assert.equal(request.type,'inspect');
  assert.ok(request.sessionId);
  assert.deepEqual(request.document,{kind:'schematic'});
  assert.equal(get('inspectBtn').disabled,true);
  socket.reply({type:'operation_started',requestId:'op-3',operation:'inspect'});
  assert.equal(get('livePhase').textContent,'观测当前设计：已开始');
  // a stale requestId must not finish the in-flight operation
  socket.reply({type:'inspect_result',requestId:'op-9',ok:true,components:1,nets:1});
  assert.equal(get('inspectBtn').disabled,true);
  socket.reply({type:'inspect_result',requestId:'op-3',ok:true,
    snapshotHash:'sha256:0123456789abcdefaaaa',logicalHash:'sha256:fedcba9876543210bbbb',physicalHash:'sha256:aaaabbbbccccdddd',
    components:12,nets:9,inconclusive:[]});
  assert.match(lastBubbleText(page),/观测完成：元件 12 个 · 网络 9 个/);
  assert.match(lastBubbleText(page),/0123456789abcdef/);
  assert.match(lastBubbleText(page),/不确定面：无/);
  assert.equal(get('liveState').textContent,'当前状态：执行完成');
  assert.equal(get('inspectBtn').disabled,false);
});

test('drc button shows an inconclusive verdict as uncertain and operation_error fails loudly',()=>{
  const page=loadPage();const {get}=page;
  const socket=connectAndList(page,[{id:'power-supply',name:'电源'}]);
  chooseProject(page,socket,'power-supply');
  get('drcBtn').onclick();
  assert.equal(socket.sent.at(-1).type,'drc');
  socket.reply({type:'operation_started',requestId:'op-4',operation:'drc'});
  socket.reply({type:'operation_progress',requestId:'op-4',operation:'drc',phase:'正在运行规则引擎',completed:1,total:2});
  assert.equal(get('livePhase').textContent,'DRC 检查：正在运行规则引擎');
  socket.reply({type:'drc_result',requestId:'op-4',ok:true,status:'inconclusive',errors:[],inconclusive:['drc']});
  assert.match(lastBubbleText(page),/DRC\/ERC 检查：不确定（无法判定）/);
  assert.match(lastBubbleText(page),/不确定面：drc/);
  assert.equal(get('live').className,'live uncertain');
  assert.equal(get('drcBtn').disabled,false);
  get('inspectBtn').onclick();
  socket.reply({type:'operation_started',requestId:'op-5',operation:'inspect'});
  socket.reply({type:'operation_error',requestId:'op-5',operation:'inspect',code:'eda_api_unavailable',message:'sys_WebSocket 不可用'});
  assert.equal(get('live').className,'live failed');
  assert.match(lastBubbleText(page),/EDA 操作失败：sys_WebSocket 不可用/);
  assert.equal(get('inspectBtn').disabled,false);
});

test('candidate plans require a human click and approval does not send apply',()=>{
  const page=loadPage();const {get}=page;
  const socket=connectAndList(page,[{id:'power-supply',name:'电源'}]);
  chooseProject(page,socket,'power-supply');
  get('instruction').value='标注电源用途';get('send').onclick();
  const sessionId=socket.sent.at(-1).sessionId;
  socket.reply({type:'preview_result',sessionId,planHash:'sha256:plan',baselineSnapshotHash:'sha256:base',approvalAllowed:true,
    plan:{diff:[{detail:'标注用途'}],risks:['预览风险'],rollbackable:true,steps:[]}});
  assert.equal(get('approvePlan').disabled,true,'cannot decide while the model is running');
  socket.reply({type:'chat_completed'});
  assert.equal(get('approvePlan').disabled,false);
  assert.match(get('planDetails').textContent,/预览风险/);
  assert.match(get('planDetails').textContent,/回读：未执行/);
  assert.equal(socket.sent.some(message=>message.type==='approve_plan'),false);
  get('approvePlan').onclick();
  assert.deepEqual(socket.sent.at(-1),{type:'approve_plan',sessionId,planHash:'sha256:plan',baselineSnapshotHash:'sha256:base',decision:'approved'});
  assert.equal(get('approvePlan').disabled,true);
  socket.reply({type:'approval_result',planHash:'sha256:plan',decision:'approved'});
  assert.equal(socket.sent.some(message=>message.type==='apply'),false);
  assert.match(lastBubbleText(page),/EDA 写入尚未执行/);
  get('clear').onclick();
  assert.equal(get('planReview').style.display,'none');
  assert.equal(get('approvePlan').disabled,true);
});
