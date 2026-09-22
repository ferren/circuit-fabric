const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const path=require('node:path');

test('optional compatibility script discovers, selects, refreshes and sends without blocking the base page',()=>{
  const elements=new Map();
  class Element {
    constructor(){this.value='';this.textContent='';this.style={};this.children=[];this.disabled=false;this.className='';this.onclick=null;this.onchange=null;this.onkeydown=null;this.parentNode=null;}
    appendChild(item){item.parentNode=this;this.children.push(item);return item}
    setAttribute(){}
    set innerHTML(value){this.children=[];this._innerHTML=value}
    get innerHTML(){return this._innerHTML||''}
  }
  const settings=new Element();
  const get=id=>{if(!elements.has(id)){const item=new Element();if(id==='project'||id==='bridge')item.parentNode=settings;elements.set(id,item)}return elements.get(id)};
  class Socket {
    static instances=[];
    constructor(){this.readyState=0;this.sent=[];Socket.instances.push(this)}
    send(raw){this.sent.push(JSON.parse(raw))}
    close(){this.readyState=3;if(this.onclose)this.onclose()}
    open(){this.readyState=1;this.onopen()}
    reply(message){this.onmessage({data:JSON.stringify(message)})}
  }
  const context=vm.createContext({
    document:{getElementById:get,createElement:()=>new Element()},WebSocket:Socket,
    setTimeout:fn=>{fn();return 1},clearTimeout:()=>{},setInterval:()=>0,clearInterval:()=>{},Date,Math,
  });
  const source=fs.readFileSync(path.join(__dirname,'../iframe/compat.js'),'utf8');
  vm.runInContext(source,context);
  let socket=Socket.instances[0];socket.open();
  assert.deepEqual(socket.sent[0],{type:'hello',protocolVersion:1});
  socket.reply({type:'hello_ack'});
  assert.equal(socket.sent[1].type,'list_projects');
  socket.reply({type:'projects',projects:[]});
  assert.equal(get('send').disabled,true);
  const listButton=settings.children.find(item=>item.textContent==='刷新项目');
  listButton.onclick();
  socket.reply({type:'projects',projects:[{id:'power-supply',name:'电源'}]});
  const list=settings.children.find(item=>item.id==='circuitfabric-project-list');
  assert.equal(list.children.length,1);
  list.children[0].onclick();
  assert.deepEqual(socket.sent.at(-1),{type:'select_project',projectId:'power-supply'});
  socket.reply({type:'project_selected',projectId:'power-supply'});
  assert.equal(get('send').disabled,false);
  get('instruction').value='检查电源';get('send').onclick();
  assert.deepEqual(socket.sent.at(-1),{type:'chat',sessionId:socket.sent.at(-1).sessionId,text:'检查电源'});
  get('refresh').onclick();socket=Socket.instances[1];socket.open();
  assert.deepEqual(socket.sent[0],{type:'hello',protocolVersion:1});
});