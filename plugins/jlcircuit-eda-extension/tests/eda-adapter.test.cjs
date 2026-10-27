const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const source = fs.readFileSync(path.join(__dirname, '../src/index.js'), 'utf8');

function makePin(number, name, x, y, pinType) {
  return {
    getState_PinNumber: async () => number,
    getState_PinName: async () => name,
    getState_X: async () => x,
    getState_Y: async () => y,
    getState_pinType: async () => pinType,
  };
}

function makeComponent(designator, name, pins) {
  return {
    getState_PrimitiveId: async () => 'gge-' + designator,
    getState_Designator: async () => designator,
    getState_Name: async () => name,
    getState_Footprint: async () => '0603',
    getState_UniqueId: async () => 'uid-' + designator,
    getState_Component: async () => ({ libraryUuid: 'lib-1', uuid: 'cmp-' + designator, name }),
    getState_X: async () => 10,
    getState_Y: async () => 20,
    getState_Rotation: async () => 0,
    getState_Mirror: async () => false,
    getState_OtherProperty: async () => ({ Manufacturer: 'Yageo' }),
    getAllPins: async () => pins,
  };
}

function makeFullEda(overrides = {}) {
  const eda = {
    lib_Device: { get: async (uuid, libraryUuid) => ({ uuid, libraryUuid, libraryType: '3', subPartNames: [] }) },
    lib_LibrariesList: { getProjectLibraryUuid: async () => 'proj-lib-uuid' },
    dmt_Project: { getCurrentProjectInfo: async () => ({ uuid: 'proj-uuid', friendlyName: '电源板', name: 'power' }) },
    dmt_Schematic: { getCurrentSchematicPageInfo: async () => ({ uuid: 'page-uuid', name: '第 1 页', parentSchematicUuid: 'sch-uuid' }) },
    sch_PrimitiveComponent: {
      getAll: async () => [
        makeComponent('R1', '10k', [makePin('1', 'A', 0, 0, 'passive'), makePin('2', 'B', 10, 0, 'passive')]),
        makeComponent('U1', 'MCU', [makePin('1', 'VCC', 5, 5, 'power')]),
      ],
    },
    sch_PrimitiveWire: {
      getAll: async () => [{
        getState_PrimitiveId: async () => 'wire-1',
        getState_Net: async () => 'VCC',
        getState_Line: async () => [[0, 0], [10, 0]],
      }],
    },
    sch_PrimitiveBus: {
      getAll: async () => [{
        getState_PrimitiveId: async () => 'bus-1',
        getState_BusName: async () => 'DBUS',
      }],
    },
    sch_Net: {
      getAllNets: async () => [{
        net: 'VCC',
        wires: [{ name: 'VCC', id: 'wire-1', globalNetName: 'VCC', pageName: '第 1 页', pageUuid: 'page-uuid' }],
      }],
    },
    sch_Drc: { check: async () => [] },
    dmt_EditorControl: { getCurrentRenderedAreaImage: async () => ({ arrayBuffer: async () => new Uint8Array([137, 80, 78, 71]).buffer }) },
    sys_Environment: {
      isClient: async () => true,
      isWeb: async () => false,
      isJLCEDAProEdition: async () => true,
      isEasyEDAProEdition: async () => false,
    },
    sys_WebSocket: {
      register: async () => {},
      send: () => {},
      close: () => {},
    },
    sys_MessageBus: { pull: () => {}, push: () => {} },
  };
  return Object.assign(eda, overrides);
}

function load(eda, timers = {}) {
  const context = vm.createContext({
    eda,
    URL,
    Buffer,
    console,
    setTimeout,
    clearTimeout,
    ...timers,
  });
  vm.runInContext(source, context);
  return context.edaEsbuildExportName._test;
}

// Values built inside the vm context carry its intrinsics' prototypes; the
// round trip normalizes them so deepStrictEqual compares structure only.
function plain(value) {
  return JSON.parse(JSON.stringify(value));
}

test('property and wire mutations use fresh CAS reads, separate acknowledgements and readback', async () => {
  let componentName = '10k', wireNet = 'VCC', writes = 0;
  const component = makeComponent('R1', componentName, []);
  component.getState_Name = () => componentName;
  const wire = { getState_PrimitiveId: () => 'wire-1', getState_Net: () => wireNet, getState_Line: () => [[0,0],[10,0]] };
  const eda = makeFullEda({
    sch_PrimitiveComponent: {getAll: async()=>[component], get: async()=>component,
      modify: async(id, p)=>{assert.equal(id,'gge-R1');componentName=p.name;writes++;return component;}},
    sch_PrimitiveWire: {getAll: async()=>[wire], get: async()=>wire,
      modify: async(id,p)=>{assert.equal(id,'wire-1');wireNet=p.net;writes++;return wire;}},
  });
  const hooks = load(eda);
  const request = (kind,id,expected,value,operation='apply') => ({type:'eda_operation',protocolVersion:1,requestId:'write-1',operation,
    planHash:'plan',baselineSnapshotHash:'base',scope:['project.write'],document:{kind:'schematic',projectUuid:'proj-uuid',pageUuid:'page-uuid'},
    params:{kind,primitiveId:id,expected,value}});
  let result=await hooks.handleOperationMessage(request('component_value','gge-R1','10k','47k'));
  assert.equal(result.ok,true);assert.equal(result.payload.written,true);assert.equal(result.payload.snapshot,undefined);
  result=await hooks.handleOperationMessage(request('wire_net','wire-1','VCC','POWER'));
  assert.equal(result.ok,true);
  result=await hooks.handleOperationMessage({type:'eda_operation',operation:'readback',requestId:'read-1'});
  assert.equal(result.payload.components[0].name,'47k');assert.equal(result.payload.wires[0].net,'POWER');
  result=await hooks.handleOperationMessage(request('component_value','gge-R1','stale','10k','rollback'));
  assert.equal(result.ok,false);assert.equal(writes,2);
  result=await hooks.handleOperationMessage(request('component_value','gge-R1','47k','10k','rollback'));
  assert.equal(result.ok,true);assert.equal(componentName,'10k');
  const wrongDoc=request('wire_net','wire-1','POWER','VCC');wrongDoc.document.pageUuid='other-page';
  result=await hooks.handleOperationMessage(wrongDoc);assert.equal(result.ok,false);assert.equal(writes,3);
  const noScope=request('wire_net','wire-1','POWER','VCC');noScope.scope=[];
  result=await hooks.handleOperationMessage(noScope);assert.equal(result.ok,false);assert.equal(writes,3);
});

test('structural mutations place, wire, delete and restore with fresh reads and rollback handles', async () => {
  const calls = [];
  const placedComponent = { getState_PrimitiveId: async () => 'gge-placed' };
  const createdWire = { getState_PrimitiveId: async () => 'wire-created' };
  let deletedR1 = false;
  const r1 = makeComponent('R1', '10k', [makePin('1', 'A', 0, 0, 'passive'), makePin('2', 'B', 10, 0, 'passive')]);
  const wireNs = {
    create: async (line, net) => { calls.push(['wire.create', line, net]); return createdWire; },
    get: async id => (id === 'wire-created' ? createdWire : undefined),
    delete: async id => { calls.push(['wire.delete', id]); return true; },
  };
  const componentNs = {
    getAll: async () => deletedR1 ? [] : [r1],
    get: async id => (id === 'gge-R1' ? r1 : id === 'gge-placed' ? placedComponent : undefined),
    create: async (item, x, y, sub, rotation, mirror) => {
      calls.push(['component.create', item, x, y, rotation, mirror]);
      return placedComponent;
    },
    modify: async (id, props) => { calls.push(['component.modify', id, props]); return placedComponent; },
    delete: async id => { if (id === 'gge-R1') deletedR1 = true; calls.push(['component.delete', id]); return true; },
  };
  const hooks = load(makeFullEda({ sch_PrimitiveComponent: componentNs, sch_PrimitiveWire: wireNs }));
  const request = (params, operation = 'apply') => ({ type: 'eda_operation', protocolVersion: 1, requestId: 'w', operation,
    planHash: 'plan', baselineSnapshotHash: 'base', scope: ['project.write'],
    document: { kind: 'schematic', projectUuid: 'proj-uuid', pageUuid: 'page-uuid' }, params });

  // 连线：引脚坐标现场解析，坐标锚点直连，网络名随 create 传入。
  let result = await hooks.handleOperationMessage(request({ kind: 'wire_create', primitiveId: 'net:BS#0',
    spec: { net: 'BS', connection: { pin: { designator: 'R1', pin: '1' }, anchor: { x: 50, y: 50 } } } }));
  assert.equal(result.ok, true);
  assert.equal(result.payload.createdId, 'wire-created');
  assert.deepEqual(plain(calls.at(-1)), ['wire.create', [0, 0, 50, 50], 'BS']);

  // 无锚点 → 单脚短桩（+100, 0）；符号引脚锚点按位号解析。
  await hooks.handleOperationMessage(request({ kind: 'wire_create', primitiveId: 'net:BS#1',
    spec: { net: 'BS', connection: { pin: { designator: 'R1', pin: '2' } } } }));
  assert.deepEqual(plain(calls.at(-1)), ['wire.create', [10, 0, 110, 0], 'BS']);
  await hooks.handleOperationMessage(request({ kind: 'wire_create', primitiveId: 'net:GND#0',
    spec: { net: 'GND', connection: { pin: { designator: 'R1', pin: '2' }, anchor: { pin: { designator: 'R1', pin: '1' } } } } }));
  assert.deepEqual(plain(calls.at(-1)), ['wire.create', [10, 0, 0, 0], 'GND']);

  // 放置：库身份 + 坐标创建，随后设置位号与值；必须带回新建 ID。
  result = await hooks.handleOperationMessage(request({ kind: 'component_place', primitiveId: 'component:J1',
    spec: { libraryUuid: 'lib-1', componentUuid: 'cmp-8p', x: 200, y: 300, rotation: 90, mirror: true,
            designator: 'J1', name: 'HOST_SPI_8P' } }));
  assert.equal(result.ok, true);
  assert.equal(result.payload.createdId, 'gge-placed');
  assert.deepEqual(plain(calls.at(-2)), ['component.create', { libraryUuid: 'lib-1', uuid: 'cmp-8p' }, 200, 300, 90, true]);
  assert.deepEqual(plain(calls.at(-1)), ['component.modify', 'gge-placed', { designator: 'J1', name: 'HOST_SPI_8P' }]);

  // 删除：新鲜读取核对位号；漂移时拒绝且不调用 API。
  result = await hooks.handleOperationMessage(request({ kind: 'component_delete', primitiveId: 'gge-R1',
    spec: { designator: 'R2', restore: { libraryUuid: 'lib-1', componentUuid: 'cmp-r1', x: 1, y: 2 } } }));
  assert.equal(result.ok, false);
  assert.equal(calls.at(-1)[0], 'component.modify', '位号漂移不得触发删除');
  result = await hooks.handleOperationMessage(request({ kind: 'component_delete', primitiveId: 'gge-R1',
    spec: { designator: 'R1', restore: { libraryUuid: 'lib-1', componentUuid: 'cmp-r1', x: 1, y: 2 } } }));
  assert.equal(result.ok, true);
  assert.deepEqual(plain(calls.at(-1)), ['component.delete', 'gge-R1']);

  // 回滚：新建按句柄幂等删除（已不存在视为已恢复）；删除按补偿数据重建。
  result = await hooks.handleOperationMessage(request({ kind: 'wire_create', primitiveId: 'wire-gone', spec: { net: 'BS' } }, 'rollback'));
  assert.equal(result.ok, true, '图元已不存在 → 幂等成功');
  result = await hooks.handleOperationMessage(request({ kind: 'component_place', primitiveId: 'gge-placed', spec: {} }, 'rollback'));
  assert.equal(result.ok, true);
  assert.deepEqual(plain(calls.at(-1)), ['component.delete', 'gge-placed']);
  result = await hooks.handleOperationMessage(request({ kind: 'component_delete', primitiveId: 'gge-R1',
    spec: { designator: 'R1', restore: { libraryUuid: 'lib-1', componentUuid: 'cmp-r1', x: 1, y: 2, rotation: 90, mirror: false,
            designator: 'R1', name: '10k', uniqueId: 'uid-R1', properties: { Manufacturer: 'Yageo' } } } }, 'rollback'));
  assert.equal(result.ok, true);
  assert.equal(result.payload.createdId, 'gge-placed');
  assert.deepEqual(plain(calls.at(-2)), ['component.create', { libraryUuid: 'lib-1', uuid: 'cmp-r1' }, 1, 2, 90, false]);
  assert.deepEqual(plain(calls.at(-1)), ['component.modify', 'gge-placed', { designator: 'R1', name: '10k', uniqueId: 'uid-R1', otherProperty: { Manufacturer: 'Yageo' } }]);

  // 补偿数据缺失与权限缺失均明确拒绝。
  result = await hooks.handleOperationMessage(request({ kind: 'component_delete', primitiveId: 'gge-R1',
    spec: { designator: 'R1' } }, 'rollback'));
  assert.equal(result.ok, false);
  const noScope = request({ kind: 'component_place', primitiveId: 'component:J1',
    spec: { libraryUuid: 'lib-1', componentUuid: 'cmp-8p', x: 0, y: 0 } });
  noScope.scope = [];
  result = await hooks.handleOperationMessage(noScope);
  assert.equal(result.ok, false);
});

test('restore refuses duplicate designators before creating, including after a lost acknowledgement', async () => {
  let creates = 0;
  const hooks = load(makeFullEda({ sch_PrimitiveComponent: {
    get: async () => undefined, delete: async () => true, modify: async () => ({}),
    getAll: async () => [makeComponent('U1', 'restored earlier', [])],
    create: async () => { creates++; return {}; },
  }}));
  const result = await hooks.handleOperationMessage({type:'eda_operation', protocolVersion:1, operation:'rollback',
    requestId:'dup',planHash:'p',baselineSnapshotHash:'b',scope:['project.write'],
    document:{kind:'schematic',projectUuid:'proj-uuid',pageUuid:'page-uuid'},
    params:{kind:'component_delete',primitiveId:'old-u1',spec:{restore:{libraryUuid:'lib',componentUuid:'dev',x:1,y:2,designator:'U1'}}}});
  assert.equal(result.ok, false);
  assert.match(result.payload.message, /重复重建/);
  assert.equal(creates, 0);
});

test('recovery readback confirms idle and refuses to observe through an in-flight create', async () => {
  let release;
  const api = makeFullEda();
  api.sch_PrimitiveComponent.getAll = async () => [];
  api.sch_PrimitiveComponent.get = async () => undefined;
  api.sch_PrimitiveComponent.delete = async () => true;
  api.sch_PrimitiveComponent.modify = async () => ({});
  api.sch_PrimitiveComponent.create = async () => {
    await new Promise(resolve => { release = resolve; });
    return {getState_PrimitiveId: () => 'new-u1'};
  };
  const hooks = load(api);
  const readback = {type:'eda_operation',operation:'readback',requestId:'read',params:{requireIdle:true}};
  assert.equal((await hooks.handleOperationMessage(readback)).mutationIdle, true);
  const restoring = hooks.handleOperationMessage({type:'eda_operation',protocolVersion:1,operation:'rollback',
    requestId:'restore',planHash:'p',baselineSnapshotHash:'b',scope:['project.write'],
    document:{kind:'schematic',projectUuid:'proj-uuid',pageUuid:'page-uuid'},
    params:{kind:'component_delete',primitiveId:'old-u1',spec:{restore:{libraryUuid:'lib',componentUuid:'dev',x:1,y:2}}}});
  while (!release) await new Promise(resolve => setImmediate(resolve));
  const busy = await hooks.handleOperationMessage(readback);
  assert.equal(busy.ok, false);
  assert.equal(busy.mutationIdle, false);
  const status = { ...readback, params: {requireIdle:true, statusOnly:true} };
  // 探针不能依赖任何可能被在途写入堵住的 EDA 读取。
  const getAll = api.sch_PrimitiveComponent.getAll;
  api.sch_PrimitiveComponent.getAll = () => { throw new Error('status probe touched EDA'); };
  const pending = await hooks.handleOperationMessage(status);
  assert.equal(pending.payload.executorStatus, true);
  assert.equal(pending.mutationIdle, false);
  assert.equal(pending.activeMutation.requestId, 'restore');
  assert.match(pending.activeMutation.phase, /create/);
  assert.equal(pending.extensionVersion, '0.2.23');
  release();
  assert.equal((await restoring).ok, true);
  const idle = await hooks.handleOperationMessage(status);
  assert.equal(idle.mutationIdle, true);
  assert.equal(idle.activeMutation, null);
  api.sch_PrimitiveComponent.getAll = getAll;
  assert.equal((await hooks.handleOperationMessage(readback)).mutationIdle, true);
});

function restoreRequest(overrides = {}) {
  return { type: 'eda_operation', protocolVersion: 1, operation: 'rollback', requestId: 'restore-library',
    planHash: 'p', baselineSnapshotHash: 'b', scope: ['project.write'],
    document: { kind: 'schematic', projectUuid: 'proj-uuid', pageUuid: 'page-uuid' },
    params: { kind: 'component_delete', primitiveId: 'old-u1', spec: { restore: {
      libraryUuid: 'source-lib', componentUuid: '007618a93e759d19', x: 315, y: 600,
      rotation: 90, mirror: true, designator: 'U1', uniqueId: 'gge562', name: 'ESP32',
      properties: { Manufacturer: 'Espressif' }, ...overrides,
    } } } };
}

function restorationEda(lookup, creates, lcsc) {
  return makeFullEda({ lib_Device: { get: lookup, ...(lcsc ? { getByLcscIds: lcsc } : {}) }, sch_PrimitiveComponent: {
    getAll: async () => [], get: async () => undefined, delete: async () => true,
    create: async (...args) => { creates.push(args); return { getState_PrimitiveId: () => 'restored-u1' }; },
    modify: async (id, props) => { creates.push([id, props]); return {}; },
  } });
}

test('restore resolves the same project-local UUID instead of combining it with its source library', async () => {
  const lookups = [], creates = [];
  const api = restorationEda(async (uuid, libraryUuid) => {
    lookups.push([uuid, libraryUuid]);
    if (libraryUuid === 'proj-uuid') throw new Error('[object Object]');
    return libraryUuid === 'proj-lib-uuid' ? { uuid, libraryUuid, libraryType: '3', subPartNames: ['A'] } : undefined;
  }, creates);
  const result = await load(api).handleOperationMessage(restoreRequest());
  assert.equal(result.ok, true);
  // 工程库 UUID 来自 lib_LibrariesList，而不是工程 UUID。
  assert.deepEqual(lookups, [['007618a93e759d19', 'source-lib'], ['007618a93e759d19', 'proj-lib-uuid']]);
  assert.deepEqual(plain(creates), [
    [{ uuid: '007618a93e759d19', libraryUuid: 'proj-lib-uuid' }, 315, 600, 'A', 90, true],
    ['restored-u1', { designator: 'U1', name: 'ESP32', uniqueId: 'gge562', otherProperty: { Manufacturer: 'Espressif' } }],
  ]);
});

test('restore falls back to an EDA-located device when the project library UUID is unavailable', async () => {
  const lookups = [], creates = [];
  const api = restorationEda(async (uuid, libraryUuid) => {
    lookups.push([uuid, libraryUuid ?? null]);
    return libraryUuid === undefined ? { uuid, libraryUuid: 'located-lib', libraryType: '3', subPartNames: [] } : undefined;
  }, creates);
  api.lib_LibrariesList = { getProjectLibraryUuid: async () => { throw { code: 500, msg: 'no project library' }; } };
  const result = await load(api).handleOperationMessage(restoreRequest());
  assert.equal(result.ok, true, result.payload.message);
  assert.deepEqual(lookups, [['007618a93e759d19', 'source-lib'], ['007618a93e759d19', null]]);
  assert.deepEqual(plain(creates[0]), [{ uuid: '007618a93e759d19', libraryUuid: 'located-lib' }, 315, 600, null, 90, true]);
});

test('non-Error rejections are described instead of collapsing to [object Object]', async () => {
  const creates = [];
  const api = restorationEda(async () => { throw { code: 404, msg: 'device missing' }; }, creates);
  api.lib_LibrariesList = { getProjectLibraryUuid: async () => { throw { code: 500, msg: 'no project library' }; } };
  const result = await load(api).handleOperationMessage(restoreRequest());
  const diagnostics = result.payload.restoreDiagnostics.join('; ');
  assert.match(diagnostics, /project-library: \{"code":500,"msg":"no project library"\}/);
  assert.match(diagnostics, /located: \{"code":404,"msg":"device missing"\}/);
  assert.doesNotMatch(diagnostics, /\[object Object\]/);
});

test('A: restore uses the device EDA returns for the LCSC id when exact lookup fails', async () => {
  const creates = [], queries = [];
  const api = restorationEda(async () => undefined, creates, async (id, libraryUuid) => {
    queries.push([id, libraryUuid ?? null]);
    return { uuid: 'sys-esp32', libraryUuid: 'system-lib', name: 'ESP32-C3FH4', otherProperty: { 'Supplier Part': 'C2858491' } };
  });
  const result = await load(api).handleOperationMessage(restoreRequest({ supplierId: 'C2858491' }));
  assert.equal(result.ok, true, result.payload.message);
  assert.equal(result.payload.restoreMethod, 'lcsc');
  assert.deepEqual(queries, [['C2858491', null]]);
  assert.deepEqual(plain(creates[0]), [{ libraryUuid: 'system-lib', uuid: 'sys-esp32' }, 315, 600, null, 90, true]);
  assert.equal(creates[1][1].designator, 'U1');
});

test('A: offline clients retry the LCSC lookup with the explicit system library', async () => {
  const creates = [], queries = [];
  const api = restorationEda(async () => undefined, creates, async (id, libraryUuid) => {
    queries.push([id, libraryUuid ?? null]);
    if (!libraryUuid) throw new Error('no default system library');
    return { uuid: 'sys-esp32', libraryUuid };
  });
  api.lib_LibrariesList.getSystemLibraryUuid = async () => 'offline-system';
  const result = await load(api).handleOperationMessage(restoreRequest({ supplierId: 'C2858491' }));
  assert.equal(result.payload.restoreMethod, 'lcsc');
  assert.deepEqual(queries, [['C2858491', null], ['C2858491', 'offline-system']]);
  assert.deepEqual(plain(creates[0][0]), { libraryUuid: 'offline-system', uuid: 'sys-esp32' });
});

test('A: restore with only an LCSC id needs no original library identity', async () => {
  const creates = [];
  const api = restorationEda(async () => undefined, creates,
    async () => ({ uuid: 'sys-esp32', libraryUuid: 'system-lib' }));
  const result = await load(api).handleOperationMessage(
    restoreRequest({ libraryUuid: null, componentUuid: null, supplierId: 'C2858491' }));
  assert.equal(result.payload.restoreMethod, 'lcsc');
  assert.deepEqual(plain(creates[0][0]), { libraryUuid: 'system-lib', uuid: 'sys-esp32' });
});

test('B: unverifiable identities fall back to creating with the original pair', async () => {
  for (const [lookup, lcsc, supplierId] of [
    [async () => undefined, undefined, undefined],
    [async (uuid, libraryUuid) => ({ uuid: 'different-device', libraryUuid }), undefined, undefined],
    [async (uuid, libraryUuid) => ({ uuid, libraryUuid, libraryType: '2' }), undefined, undefined],
    [async () => { throw new Error('library is unavailable'); }, undefined, undefined],
    [async () => undefined, async () => undefined, 'C2858491'],
    [async () => undefined, async () => ({ uuid: 'other', libraryUuid: 'sys', otherProperty: { 'Supplier Part': 'C1' } }), 'C2858491'],
  ]) {
    const creates = [], hooks = load(restorationEda(lookup, creates, lcsc));
    const result = await hooks.handleOperationMessage(restoreRequest(supplierId ? { supplierId } : {}));
    assert.equal(result.ok, true, result.payload.message);
    assert.equal(result.payload.restoreMethod, 'original_unverified');
    assert.deepEqual(plain(creates[0][0]), { libraryUuid: 'source-lib', uuid: '007618a93e759d19' });
    const status = await hooks.handleOperationMessage({ type: 'eda_operation', operation: 'readback', params: {statusOnly: true} });
    assert.equal(status.mutationIdle, true);
  }
});

test('restore without any library identity or LCSC id is refused before create', async () => {
  const creates = [];
  const result = await load(restorationEda(async () => undefined, creates)).handleOperationMessage(
    restoreRequest({ libraryUuid: null, componentUuid: null }));
  assert.equal(result.payload.writeStatus, 'failed');
  assert.match(result.payload.message, /立创编号/);
  assert.equal(creates.length, 0);
});

test('inspect records library lookup shapes and supplier ids without changing component records', async () => {
  const u1 = makeComponent('U1', 'ESP32', []);
  Object.assign(u1, { getState_Component: () => ({ uuid: 'dev-u1', libraryUuid: 'lib-a' }),
    getState_SupplierId: () => 'C2858491', getState_Supplier: () => 'LCSC' });
  const hooks = load(makeFullEda({
    lib_Device: { get: async (uuid, libraryUuid) => (libraryUuid ? undefined : { uuid: 'sys-uuid', libraryUuid: 'sys-lib', libraryType: 3 }) },
    lib_LibrariesList: { getProjectLibraryUuid: async () => 'project' },
    sch_PrimitiveComponent: { getAll: async () => [u1] },
  }));
  const result = plain(await hooks.inspectSchematic());
  assert.equal(result.payload.components[0].componentUuid, 'dev-u1');
  assert.equal(result.payload.libraryProbe.projectLibraryUuid, 'project');
  const [entry] = result.payload.libraryProbe.entries;
  assert.equal(entry.supplierId, 'C2858491');
  assert.deepEqual(entry.lookups.declared, { returned: 'undefined' });
  assert.equal(entry.lookups.located.uuid, 'sys-uuid');
  assert.equal(entry.lookups.located.libraryTypeOf, 'number');
});

test('a missing library API falls back to B, but ambiguous subparts never start a write', async () => {
  const fallbackCreates = [], fallback = restorationEda(async () => undefined, fallbackCreates);
  delete fallback.lib_Device;
  const fallbackResult = await load(fallback).handleOperationMessage(restoreRequest());
  assert.equal(fallbackResult.payload.restoreMethod, 'original_unverified');
  assert.match(fallbackResult.payload.restoreDiagnostics.join(';'), /lib_Device\.get_unavailable/);

  const creates = [], api = restorationEda(async (uuid, libraryUuid) => ({ uuid, libraryUuid, subPartNames: ['A', 'B'] }), creates);
  const result = await load(api).handleOperationMessage(restoreRequest());
  assert.equal(result.payload.writeStatus, 'failed');
  assert.match(result.payload.message, /library_subpart_unresolved/);
  assert.equal(creates.length, 0);
});

test('changing the active document during library lookup prevents creation', async () => {
  const creates = [];
  const api = restorationEda(async (uuid, libraryUuid) => {
    api.dmt_Schematic.getCurrentSchematicPageInfo = async () => ({ uuid: 'other-page' });
    return { uuid, libraryUuid };
  }, creates);
  const result = await load(api).handleOperationMessage(restoreRequest());
  assert.equal(result.payload.writeStatus, 'failed');
  assert.match(result.payload.message, /library_document_changed/);
  assert.equal(creates.length, 0);
});

test('timed out read-only lookups fall back once and never cause a second late create', async () => {
  const releases = [], creates = [];
  const api = restorationEda(() => new Promise(resolve => releases.push(resolve)), creates);
  const hooks = load(api, { setTimeout: (callback) => setTimeout(callback, 5) });
  const result = await hooks.handleOperationMessage(restoreRequest());
  assert.equal(result.payload.restoreMethod, 'original_unverified');
  assert.match(result.payload.restoreDiagnostics.join(';'), /library_lookup_timeout/);
  const createCalls = creates.filter(args => typeof args[0] === 'object').length;
  assert.equal(createCalls, 1);
  for (const resolve of releases) resolve({ uuid: '007618a93e759d19', libraryUuid: 'proj-lib-uuid' });
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(creates.filter(args => typeof args[0] === 'object').length, 1);
});

test('exceptions after API invocation remain inconclusive, and concurrent mutations are refused', async()=>{
  let release;let calls=0;
  const primitive=makeComponent('R1','10k',[]);
  const hooks=load(makeFullEda({sch_PrimitiveComponent:{get:async()=>primitive,
    modify:async()=>{calls++;await new Promise(resolve=>{release=resolve});throw new Error('lost acknowledgement');}}}));
  const req={type:'eda_operation',protocolVersion:1,operation:'apply',requestId:'1',planHash:'p',baselineSnapshotHash:'b',scope:['project.write'],
    document:{kind:'schematic',projectUuid:'proj-uuid',pageUuid:'page-uuid'},params:{kind:'component_value',primitiveId:'gge-R1',expected:'10k',value:'47k'}};
  const first=hooks.handleOperationMessage(req);
  while(!release) await new Promise(resolve=>setImmediate(resolve));
  const busy=await hooks.handleOperationMessage({...req,requestId:'2'});assert.equal(busy.ok,false);assert.equal(calls,1);
  release();const unknown=await first;assert.equal(unknown.payload.writeStatus,'inconclusive');assert.equal(unknown.ok,false);
});

test('malformed or absent DRC output never means the design passed', async () => {
  for (const report of [undefined, null, true, false, {}, ['unknown'], [{unexpected:'shape'}]]) {
    const hooks = load(makeFullEda({sch_Drc:{check:async()=>report}}));
    const result = plain(await hooks.runDrc());
    assert.equal(result.status, 'inconclusive');
    assert.deepEqual(result.inconclusive, ['drc']);
  }
  const hooks = load(makeFullEda({sch_Drc:{check:async()=>[{children:[{type:'error',rule:'IN-IN'}]}]}}));
  assert.equal((await hooks.runDrc()).status, 'failed');
});

test('getters keep their primitive receiver and missing placement stays inconclusive', async () => {
  const primitive = makeComponent('R1','10k',[]);
  primitive.name = '47k';
  primitive.getState_Name = function () { return this.name; };
  delete primitive.getState_Mirror;
  primitive.getAllPins = async () => { throw new Error('pin API unavailable'); };
  const hooks = load(makeFullEda({sch_PrimitiveComponent:{getAll:async()=>[primitive]}}));
  const result = plain(await hooks.inspectSchematic());
  assert.equal(result.payload.components[0].name, '47k');
  assert.ok(result.inconclusive.includes('physicalPlacement'));
  assert.ok(result.inconclusive.includes('pins'));
});

test('undefined enumeration results cannot be mistaken for empty collections', async () => {
  const hooks = load(makeFullEda({
    dmt_Project:{getCurrentProjectInfo:async()=>undefined},
    sch_PrimitiveComponent:{getAll:async()=>undefined},
    sch_PrimitiveWire:{getAll:async()=>undefined},
    sch_PrimitiveBus:{getAll:async()=>undefined},
    sch_Net:{getAllNets:async()=>undefined},
  }));
  const result = plain(await hooks.inspectSchematic());
  for (const face of ['document','components','pins','physicalPlacement','wires','buses','nets']) {
    assert.ok(result.inconclusive.includes(face), face);
  }
});

test('flat API wire coordinates normalize without inventing unsupported geometry', async () => {
  for (const line of [[0,0,10,0], [[0,0,10,0]]]) {
    const hooks = load(makeFullEda({sch_PrimitiveWire:{getAll:async()=>[{
      getState_PrimitiveId:()=> 'wire',getState_Net:()=> 'VCC',getState_Line:()=> line,
    }]}}));
    const result=plain(await hooks.inspectSchematic());
    assert.deepEqual(result.payload.wires[0].points,[[0,0],[10,0]]);
  }
  const hooks = load(makeFullEda({sch_PrimitiveWire:{getAll:async()=>[{
    getState_PrimitiveId:()=> 'wire',getState_Net:()=> 'VCC',getState_Line:()=> [[0,0,10,0],[20,0,30,0]],
  }]}}));
  assert.ok((await hooks.inspectSchematic()).inconclusive.includes('wires'));
});

test('inspect returns the full observation payload with an empty inconclusive list', async () => {
  const hooks = load(makeFullEda());
  const result = plain(await hooks.inspectSchematic());
  assert.deepEqual(result.inconclusive, []);
  assert.equal(typeof result.capturedAt, 'string');
  assert.ok(!Number.isNaN(Date.parse(result.capturedAt)));
  const payload = result.payload;
  assert.deepEqual(payload.document, {
    kind: 'schematic',
    projectUuid: 'proj-uuid',
    projectName: '电源板',
    pageUuid: 'page-uuid',
    pageName: '第 1 页',
  });
  assert.equal(payload.components.length, 2);
  const r1 = payload.components[0];
  assert.equal(r1.designator, 'R1');
  assert.equal(r1.primitiveId, 'gge-R1');
  assert.equal(r1.uniqueId, 'uid-R1');
  assert.equal(r1.libraryUuid, 'lib-1');
  assert.equal(r1.componentUuid, 'cmp-R1');
  assert.equal(r1.x, 10);
  assert.equal(r1.rotation, 0);
  assert.equal(r1.mirror, false);
  assert.deepEqual(r1.properties, { Manufacturer: 'Yageo' });
  assert.equal(r1.pins.length, 2);
  assert.deepEqual(r1.pins[0], { number: '1', name: 'A', x: 0, y: 0, pinType: 'passive' });
  assert.deepEqual(payload.wires, [{ primitiveId: 'wire-1', net: 'VCC', points: [[0, 0], [10, 0]] }]);
  assert.deepEqual(payload.buses, [{ primitiveId: 'bus-1', name: 'DBUS' }]);
  assert.equal(payload.nets.length, 1);
  assert.equal(payload.nets[0].name, 'VCC');
  assert.equal(payload.nets[0].wires[0].pageUuid, 'page-uuid');
});

test('documented symbol types stay observable without requiring a part designator', async () => {
  const part = makeComponent('U1', 'ESP32', []);
  part.getState_ComponentType = () => 'part';
  const symbols = ['sheet', 'netflag', 'netport', 'nonElectrical_symbol',
    'diffPairsFlag', 'offPageConnector', 'short_symbol'].map((type, index) => {
    const symbol = makeComponent('', index === 0 ? 'Drawing-Symbol_A4' : 'GND', []);
    symbol.getState_PrimitiveId = () => 'symbol-' + index;
    symbol.getState_ComponentType = () => type;
    return symbol;
  });
  const hooks = load(makeFullEda({ sch_PrimitiveComponent: {getAll: async () => [part, ...symbols]} }));
  const result = plain(await hooks.inspectSchematic());
  assert.deepEqual(result.inconclusive, []);
  assert.equal(result.payload.components.length, 1);
  assert.equal(result.payload.components[0].designator, 'U1');
  assert.deepEqual(result.payload.symbols.map(symbol => symbol.componentType),
    ['sheet', 'netflag', 'netport', 'nonElectrical_symbol', 'diffPairsFlag', 'offPageConnector', 'short_symbol']);
  assert.equal(result.payload.symbols[0].name, 'Drawing-Symbol_A4');
});

test('missing designators or unsupported types never pass by looking like GND or a drawing', async () => {
  for (const type of [null, 'part', 'block_symbol', 'future-symbol']) {
    const item = makeComponent('', 'GND', []);
    if (type !== null) item.getState_ComponentType = () => type;
    const result = plain(await load(makeFullEda({sch_PrimitiveComponent:{getAll:async()=>[item]}})).inspectSchematic());
    assert.ok(result.inconclusive.includes('components'), String(type));
    assert.equal(result.payload.components.length, 1);
    assert.equal(result.payload.symbols.length, 0);
  }
  const broken = makeComponent('', 'Drawing-Symbol_A4', []);
  broken.getState_ComponentType = () => 'sheet';
  broken.getState_PrimitiveId = () => undefined;
  const result = plain(await load(makeFullEda({sch_PrimitiveComponent:{getAll:async()=>[broken]}})).inspectSchematic());
  assert.ok(result.inconclusive.includes('components'), '辅助符号缺图元身份仍然不完整');
});

test('inspect marks missing namespaces as inconclusive instead of failing', async () => {
  const withoutNets = load(makeFullEda({ sch_Net: undefined }));
  const netsResult = plain(await withoutNets.inspectSchematic());
  assert.ok(netsResult.inconclusive.includes('nets'));
  assert.equal(netsResult.payload.nets.length, 0);

  const withoutComponents = load(makeFullEda({ sch_PrimitiveComponent: undefined }));
  const componentsResult = plain(await withoutComponents.inspectSchematic());
  assert.ok(componentsResult.inconclusive.includes('components'));
  assert.ok(componentsResult.inconclusive.includes('pins'));
  assert.ok(componentsResult.inconclusive.includes('physicalPlacement'));
  assert.equal(componentsResult.payload.components.length, 0);
  // other observation faces still arrive
  assert.equal(componentsResult.payload.wires.length, 1);
});

test('drc is inconclusive when the API throws, and passed only when no errors return', async () => {
  const throwing = load(makeFullEda({
    sch_Drc: { check: async () => { throw new Error('drc engine missing'); } },
  }));
  const failed = plain(await throwing.runDrc());
  assert.equal(failed.status, 'inconclusive');
  assert.deepEqual(failed.inconclusive, ['drc']);

  const withErrors = load(makeFullEda({
    sch_Drc: {
      check: async () => [{
        type: 'error', rule: 'IN-IN', net: 'VCC',
        primitives: [{ name: '', designator: 'R1', primitiveId: 'gge-R1', sheet: '第 1 页' }],
      }],
    },
  }));
  const drc = plain(await withErrors.runDrc());
  assert.equal(drc.status, 'failed');
  assert.deepEqual(drc.errors, [{
    type: 'error', rule: 'IN-IN', net: 'VCC',
    primitives: [{ designator: 'R1', primitiveId: 'gge-R1' }],
  }]);
  assert.deepEqual(drc.inconclusive, []);

  const clean = load(makeFullEda());
  assert.deepEqual(plain(await clean.runDrc()), { status: 'passed', errors: [], inconclusive: [] });

  const missingApi = load(makeFullEda({ sch_Drc: undefined }));
  const missing = plain(await missingApi.runDrc());
  assert.equal(missing.status, 'inconclusive');
  assert.deepEqual(missing.inconclusive, ['drc']);
});

test('captureCanvas reports an inconclusive canvasImage when the renderer returns nothing', async () => {
  const hooks = load(makeFullEda());
  const captured = plain(await hooks.captureCanvas());
  assert.deepEqual(captured.inconclusive, []);
  assert.equal(captured.mime, 'image/png');
  assert.equal(Buffer.from(captured.base64, 'base64').toString('hex'), '89504e47');

  const empty = load(makeFullEda({ dmt_EditorControl: { getCurrentRenderedAreaImage: async () => undefined } }));
  const missed = plain(await empty.captureCanvas());
  assert.equal(missed.base64, null);
  assert.deepEqual(missed.inconclusive, ['canvasImage']);
});

test('readContext returns project, page and guarded environment info', async () => {
  const hooks = load(makeFullEda());
  const result = plain(await hooks.readContext());
  assert.deepEqual(result.inconclusive, []);
  assert.deepEqual(result.context, {
    projectName: '电源板',
    pageName: '第 1 页',
    documentKind: 'schematic',
    environment: { client: true, web: false },
  });

  const bare = load(makeFullEda({ sys_Environment: undefined }));
  const bareResult = plain(await bare.readContext());
  assert.ok(bareResult.inconclusive.includes('environment'));
  assert.deepEqual(bareResult.context.environment, { client: null, web: null });
});

test('handleOperationMessage echoes requestId/operation and rejects unknown operations', async () => {
  const hooks = load(makeFullEda());
  const reply = plain(await hooks.handleOperationMessage({
    type: 'eda_operation',
    requestId: 'op-3',
    operation: 'inspect',
    protocolVersion: 1,
  }));
  assert.equal(reply.type, 'operation_result');
  assert.equal(reply.requestId, 'op-3');
  assert.equal(reply.operation, 'inspect');
  assert.equal(reply.ok, true);
  assert.equal(reply.payload.components.length, 2);
  assert.deepEqual(reply.inconclusive, []);

  const unsupported = plain(await hooks.handleOperationMessage({
    type: 'eda_operation',
    requestId: 'op-4',
    operation: 'unknown_operation',
  }));
  assert.equal(unsupported.type, 'operation_error');
  assert.equal(unsupported.code, 'unsupported_operation');
  assert.equal(unsupported.requestId, 'op-4');

  assert.equal(await hooks.handleOperationMessage({ type: 'ping' }), null);
});

test('loading and opening the menu stays silent when sys_WebSocket is absent', async () => {
  const messages = [];
  const eda = {
    sys_IFrame: { openIFrame: async () => true },
    sys_Message: { showToastMessage: message => messages.push(message) },
  };
  const context = vm.createContext({ eda, URL, Buffer, console });
  vm.runInContext(source, context);
  const exported = context.edaEsbuildExportName;
  assert.equal(typeof exported.openCircuitFabricAssistant, 'function');
  await exported.openCircuitFabricAssistant();
  assert.deepEqual(messages, []);
  assert.equal(exported._test.executorState.status, 'unavailable');
});
