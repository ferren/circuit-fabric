"use strict";
/* CircuitFabric JLC EDA extension entry point. No JLCircuit-Agent code is used. */
var edaEsbuildExportName = (() => {
const OPERATIONS_PROTOCOL_VERSION = 1;
const EXTENSION_VERSION = "0.2.23";
const EXECUTOR_WS_ID = "circuitfabric-executor";
const DEFAULT_BRIDGE_HOST = "ws://127.0.0.1:49630";
const DEFAULT_EXECUTOR_PATH = "/eda-operations";
const DEFAULT_EXECUTOR_URL = DEFAULT_BRIDGE_HOST + DEFAULT_EXECUTOR_PATH;
const BRIDGE_BUS_TOPIC = "circuitfabric.bridge";

const executorState = {
  url: DEFAULT_EXECUTOR_URL,
  status: "idle", // idle | connecting | connected | unavailable
  lastError: null,
  helloAcked: false,
  operations: [],
  unsupportedOperations: [],
};
let executorRuntimeStarted = false;

function edaApi() {
  // JLC EDA supplies `eda` as a module-scoped global. Some versions additionally
  // expose it on globalThis, so retain that fallback for compatibility.
  return typeof eda !== "undefined" ? eda : globalThis.eda;
}

function notify(message) {
  try {
    const api = edaApi();
    if (api && api.sys_Message && api.sys_Message.showToastMessage) {
      api.sys_Message.showToastMessage(message, "info");
      return;
    }
    api?.sys_Dialog?.showInformationMessage?.(message, "CircuitFabric");
  } catch {
    // The original failure must not escape a menu callback and disappear silently.
  }
}

function addInconclusive(list, faces) {
  for (const face of faces) {
    if (!list.includes(face)) list.push(face);
  }
}

function namespaceOf(api, name) {
  const ns = api ? api[name] : undefined;
  return ns && typeof ns === "object" ? ns : null;
}

async function safeCall(fn, fallback) {
  if (arguments.length < 2) fallback = null;
  if (typeof fn !== "function") return fallback;
  try {
    return await fn();
  } catch {
    return fallback;
  }
}

function stateReader(object, name) {
  return typeof object?.[name] === "function" ? object[name].bind(object) : null;
}

function wirePoints(line) {
  if (!Array.isArray(line)) return null;
  if (line.length === 1 && Array.isArray(line[0])) return wirePoints(line[0]);
  if (line.every(value => typeof value === "number")) {
    if (line.length < 4 || line.length % 2 || !line.every(Number.isFinite)) return null;
    const points = [];
    for (let i = 0; i < line.length; i += 2) points.push([line[i], line[i + 1]]);
    return points;
  }
  return line.length >= 2 && line.every(point => Array.isArray(point) && point.length === 2 && point.every(Number.isFinite)) ? line : null;
}

function readableFootprint(value) {
  if (value == null) return null;
  if (typeof value === "string") return value;
  if (typeof value === "object") {
    return value.name || value.title || value.package || null;
  }
  return String(value);
}

// 官方 ESCH_PrimitiveComponentType：这些对象没有普通器件位号，保留为辅助符号。
// 未知类型、复用模块及真正缺位号的 part 仍必须报告不完整，不能按名称猜测后跳过。
function isSchematicSymbol(componentType) {
  return ["sheet", "netflag", "netport", "diffPairsFlag", "nonElectrical_symbol",
    "offPageConnector", "short_symbol"].includes(componentType);
}

async function readComponent(component, inconclusive) {
  const primitiveId = await safeCall(stateReader(component, "getState_PrimitiveId"));
  const componentType = await safeCall(stateReader(component, "getState_ComponentType"));
  const symbol = isSchematicSymbol(componentType);
  const item = {
    primitiveId,
    componentType,
    designator: await safeCall(stateReader(component, "getState_Designator")),
    name: await safeCall(stateReader(component, "getState_Name")),
    footprint: readableFootprint(await safeCall(stateReader(component, "getState_Footprint"))),
    uniqueId: await safeCall(stateReader(component, "getState_UniqueId")),
    libraryUuid: null,
    componentUuid: null,
    supplierId: null,
    x: null,
    y: null,
    rotation: null,
    mirror: null,
    properties: null,
    pins: [],
  };
  const library = await safeCall(stateReader(component, "getState_Component"));
  if (library && typeof library === "object") {
    item.libraryUuid = library.libraryUuid ?? null;
    item.componentUuid = library.uuid ?? null;
    if (!item.name) item.name = library.name ?? null;
  }
  // 立创编号（C 编号）：图元自身的 componentUuid 是工程内引用，lib_Device.get 查不到，
  // 回滚改用 lib_Device.getByLcscIds 核验重建。
  const supplierId = await safeCall(stateReader(component, "getState_SupplierId"));
  if (typeof supplierId === "string" && supplierId.trim()) item.supplierId = supplierId.trim();
  let placementFailed = false;
  const x = await safeCall(stateReader(component, "getState_X"), undefined);
  const y = await safeCall(stateReader(component, "getState_Y"), undefined);
  const rotation = await safeCall(stateReader(component, "getState_Rotation"), undefined);
  const mirror = await safeCall(stateReader(component, "getState_Mirror"), undefined);
  if (!Number.isFinite(x) || !Number.isFinite(y) || !Number.isFinite(rotation) || typeof mirror !== "boolean") {
    placementFailed = true;
  } else {
    item.x = x;
    item.y = y;
    item.rotation = rotation;
    item.mirror = mirror;
  }
  const properties = await safeCall(stateReader(component, "getState_OtherProperty"));
  if (properties && typeof properties === "object") item.properties = properties;
  if (typeof component?.getAllPins === "function") {
    const pins = await safeCall(component.getAllPins.bind(component), null);
    let pinsFailed = !Array.isArray(pins);
    for (const pin of Array.isArray(pins) ? pins : []) {
      const number = await safeCall(stateReader(pin, "getState_PinNumber"), undefined);
      const pinX = await safeCall(stateReader(pin, "getState_X"), undefined);
      const pinY = await safeCall(stateReader(pin, "getState_Y"), undefined);
      if (number == null || String(number) === "" || !Number.isFinite(pinX) || !Number.isFinite(pinY)) {
        pinsFailed = true;
        continue;
      }
      item.pins.push({
        number: String(number),
        name: await safeCall(stateReader(pin, "getState_PinName")),
        x: pinX,
        y: pinY,
        pinType: await safeCall(stateReader(pin, "getState_pinType")),
      });
    }
    if (pinsFailed) addInconclusive(inconclusive, ["pins"]);
  } else {
    addInconclusive(inconclusive, ["pins"]);
  }
  if (placementFailed) addInconclusive(inconclusive, ["physicalPlacement"]);
  if (!item.primitiveId || (!symbol && !item.designator) || item.name == null ||
      (componentType != null && componentType !== "part" && !symbol)) {
    addInconclusive(inconclusive, ["components"]);
  }
  return item;
}

// 四步链路第一步：只读观测。任何命名空间缺失或调用失败都记入 inconclusive，
// 其余观测面照常返回，绝不让「部分失败」冒充「完整成功」。
async function inspectSchematic() {
  const api = edaApi();
  const payload = { document: {}, components: [], symbols: [], wires: [], buses: [], nets: [] };
  const inconclusive = [];

  try {
    const projectNs = namespaceOf(api, "dmt_Project");
    const schematicNs = namespaceOf(api, "dmt_Schematic");
    if (projectNs?.getCurrentProjectInfo && schematicNs?.getCurrentSchematicPageInfo) {
      const project = await projectNs.getCurrentProjectInfo();
      const page = await schematicNs.getCurrentSchematicPageInfo();
      payload.document = {
        kind: "schematic",
        projectUuid: project?.uuid ?? null,
        projectName: project?.friendlyName || project?.name || null,
        pageUuid: page?.uuid ?? null,
        pageName: page?.name ?? null,
      };
      if (!payload.document.projectUuid || !payload.document.pageUuid) addInconclusive(inconclusive, ["document"]);
    } else {
      addInconclusive(inconclusive, ["document"]);
    }
  } catch {
    addInconclusive(inconclusive, ["document"]);
  }

  try {
    const componentNs = namespaceOf(api, "sch_PrimitiveComponent");
    if (typeof componentNs?.getAll === "function") {
      const components = await componentNs.getAll();
      if (!Array.isArray(components)) addInconclusive(inconclusive, ["components", "pins", "physicalPlacement"]);
      const parts = [];
      for (const component of Array.isArray(components) ? components : []) {
        const item = await readComponent(component, inconclusive);
        const symbol = isSchematicSymbol(item.componentType);
        (symbol ? payload.symbols : payload.components).push(item);
        if (!symbol) parts.push({ component, item });
      }
      // 诊断字段，不进入快照哈希；失败不影响观测结论。
      payload.libraryProbe = await probeLibraryIdentities(api, parts).catch(error => ({ error: describeError(error) }));
    } else {
      addInconclusive(inconclusive, ["components", "pins", "physicalPlacement"]);
    }
  } catch {
    addInconclusive(inconclusive, ["components", "pins", "physicalPlacement"]);
  }

  try {
    const wireNs = namespaceOf(api, "sch_PrimitiveWire");
    if (typeof wireNs?.getAll === "function") {
      const wires = await wireNs.getAll();
      let failed = !Array.isArray(wires);
      for (const wire of Array.isArray(wires) ? wires : []) {
        const primitiveId = await safeCall(stateReader(wire, "getState_PrimitiveId"), undefined);
        const line = await safeCall(stateReader(wire, "getState_Line"), undefined);
        const points = wirePoints(line);
        if (!primitiveId || !points) {
          failed = true;
          continue;
        }
        payload.wires.push({
          primitiveId,
          net: await safeCall(stateReader(wire, "getState_Net")),
          points,
        });
      }
      if (failed) addInconclusive(inconclusive, ["wires", "netMembership"]);
    } else {
      addInconclusive(inconclusive, ["wires", "netMembership"]);
    }
  } catch {
    addInconclusive(inconclusive, ["wires", "netMembership"]);
  }

  try {
    const busNs = namespaceOf(api, "sch_PrimitiveBus");
    if (typeof busNs?.getAll === "function") {
      const buses = await busNs.getAll();
      let failed = !Array.isArray(buses);
      for (const bus of Array.isArray(buses) ? buses : []) {
        const primitiveId = await safeCall(stateReader(bus, "getState_PrimitiveId"), undefined);
        if (!primitiveId) {
          failed = true;
          continue;
        }
        payload.buses.push({
          primitiveId,
          name: await safeCall(stateReader(bus, "getState_BusName")),
        });
      }
      if (failed) addInconclusive(inconclusive, ["buses"]);
    } else {
      addInconclusive(inconclusive, ["buses"]);
    }
  } catch {
    addInconclusive(inconclusive, ["buses"]);
  }

  try {
    const netNs = namespaceOf(api, "sch_Net");
    if (typeof netNs?.getAllNets === "function") {
      const nets = await netNs.getAllNets();
      let failed = !Array.isArray(nets);
      for (const entry of Array.isArray(nets) ? nets : []) {
        if (!entry || typeof entry !== "object" || entry.net == null) {
          failed = true;
          continue;
        }
        payload.nets.push({
          name: String(entry.net),
          wires: (Array.isArray(entry.wires) ? entry.wires : []).map((wire) => ({
            name: wire?.name ?? null,
            id: wire?.id ?? null,
            globalNetName: wire?.globalNetName ?? null,
            pageName: wire?.pageName ?? null,
            pageUuid: wire?.pageUuid ?? null,
          })),
        });
      }
      if (failed) addInconclusive(inconclusive, ["nets"]);
    } else {
      addInconclusive(inconclusive, ["nets"]);
    }
  } catch {
    addInconclusive(inconclusive, ["nets"]);
  }

  return { payload, inconclusive, capturedAt: new Date().toISOString() };
}

async function readContext() {
  const api = edaApi();
  const context = {
    projectName: null,
    pageName: null,
    documentKind: "schematic",
    environment: { client: null, web: null },
  };
  const inconclusive = [];

  const projectNs = namespaceOf(api, "dmt_Project");
  const project = await safeCall(projectNs?.getCurrentProjectInfo?.bind(projectNs));
  if (project && (project.friendlyName || project.name)) {
    context.projectName = project.friendlyName || project.name;
  } else {
    addInconclusive(inconclusive, ["document"]);
  }
  const schematicNs = namespaceOf(api, "dmt_Schematic");
  const page = await safeCall(schematicNs?.getCurrentSchematicPageInfo?.bind(schematicNs));
  if (page && page.name) {
    context.pageName = page.name;
  } else {
    addInconclusive(inconclusive, ["document"]);
  }

  const environment = namespaceOf(api, "sys_Environment");
  if (environment) {
    const client = await safeCall(environment.isClient?.bind(environment), undefined);
    const web = await safeCall(environment.isWeb?.bind(environment), undefined);
    if (typeof client === "boolean" && typeof web === "boolean") {
      context.environment = { client, web };
    } else {
      addInconclusive(inconclusive, ["environment"]);
    }
  } else {
    addInconclusive(inconclusive, ["environment"]);
  }

  return { context, inconclusive, capturedAt: new Date().toISOString() };
}

// API 调用成功 ≠ 设计验证成功：status 只由 errors 是否为空决定。
async function runDrc() {
  const api = edaApi();
  const drcNs = namespaceOf(api, "sch_Drc");
  if (typeof drcNs?.check !== "function") {
    return { status: "inconclusive", errors: [], inconclusive: ["drc"] };
  }
  let report;
  try {
    report = await drcNs.check(true, false, true);
  } catch (error) {
    return { status: "inconclusive", errors: [], inconclusive: ["drc"], error: String(error) };
  }
  if (!Array.isArray(report)) {
    return { status: "inconclusive", errors: [], inconclusive: ["drc"] };
  }
  const errors = [];
  let malformed = false;
  const pending = report.slice();
  while (pending.length) {
    const item = pending.shift();
    if (!item || typeof item !== "object") { malformed = true; continue; }
    if (Array.isArray(item.children)) { pending.push(...item.children); continue; }
    if (typeof item.type !== "string" || typeof item.rule !== "string") {
      malformed = true;
      continue;
    }
    errors.push({
      type: item.type ?? null,
      rule: item.rule ?? null,
      net: item.net ?? null,
      primitives: (Array.isArray(item.primitives) ? item.primitives : []).map((primitive) => ({
        designator: primitive?.designator ?? null,
        primitiveId: primitive?.primitiveId ?? null,
      })),
    });
  }
  return { status: errors.length ? "failed" : malformed ? "inconclusive" : "passed", errors,
    inconclusive: malformed ? ["drc"] : [] };
}

function arrayBufferToBase64(buffer) {
  const bytes = new Uint8Array(buffer);
  if (typeof Buffer !== "undefined") return Buffer.from(bytes).toString("base64");
  if (typeof btoa === "function") {
    let binary = "";
    for (let i = 0; i < bytes.length; i += 1) binary += String.fromCharCode(bytes[i]);
    return btoa(binary);
  }
  return null;
}

async function blobToBase64(blob) {
  if (!blob) return null;
  if (typeof FileReader !== "undefined") {
    try {
      return await new Promise((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(String(reader.result).split(",").pop() || null);
        reader.onerror = () => reject(reader.error);
        reader.readAsDataURL(blob);
      });
    } catch {
      // fall through to the arrayBuffer path
    }
  }
  if (typeof blob.arrayBuffer === "function") {
    try {
      return arrayBufferToBase64(await blob.arrayBuffer());
    } catch {
      return null;
    }
  }
  return null;
}

async function captureCanvas() {
  const api = edaApi();
  const controlNs = namespaceOf(api, "dmt_EditorControl");
  if (typeof controlNs?.getCurrentRenderedAreaImage !== "function") {
    return { mime: "image/png", base64: null, inconclusive: ["canvasImage"] };
  }
  let blob;
  try {
    blob = await controlNs.getCurrentRenderedAreaImage();
  } catch (error) {
    return { mime: "image/png", base64: null, inconclusive: ["canvasImage"], error: String(error) };
  }
  if (!blob) return { mime: "image/png", base64: null, inconclusive: ["canvasImage"] };
  const base64 = await blobToBase64(blob);
  if (!base64) return { mime: "image/png", base64: null, inconclusive: ["canvasImage"] };
  return { mime: "image/png", base64, inconclusive: [] };
}

async function collectEdaEnvironment() {
  const environment = namespaceOf(edaApi(), "sys_Environment");
  let edition = "unknown";
  if (environment) {
    if (await safeCall(environment.isJLCEDAProEdition?.bind(environment), false)) edition = "jlceda-pro";
    else if (await safeCall(environment.isEasyEDAProEdition?.bind(environment), false)) edition = "easyeda-pro";
  }
  return {
    edition,
    client: await safeCall(environment?.isClient?.bind(environment), null),
    web: await safeCall(environment?.isWeb?.bind(environment), null),
    apiTypesVersion: edaApi()?.apiTypesVersion ?? null,
  };
}

function sendExecutorMessage(message) {
  const api = edaApi();
  if (executorState.status !== "connected") return false;
  if (typeof api?.sys_WebSocket?.send !== "function") return false;
  try {
    api.sys_WebSocket.send(EXECUTOR_WS_ID, JSON.stringify(message));
    return true;
  } catch (error) {
    executorState.status = "unavailable";
    executorState.lastError = String(error);
    return false;
  }
}

async function sendExecutorHello() {
  const hello = {
    type: "executor_hello",
    protocolVersion: OPERATIONS_PROTOCOL_VERSION,
    extensionId: "circuitfabric-jlc-eda",
    extensionVersion: EXTENSION_VERSION,
    eda: await collectEdaEnvironment(),
  };
  sendExecutorMessage(hello);
}

function normalizeExecutorUrl(raw) {
  if (typeof raw !== "string" || !raw) return null;
  try {
    const url = new URL(raw);
    if (url.protocol !== "ws:" && url.protocol !== "wss:") return null;
    url.pathname = DEFAULT_EXECUTOR_PATH;
    url.search = "";
    url.hash = "";
    return url.toString();
  } catch {
    return null;
  }
}

let mutationBusy = false;
let activeMutation = null;

function beginMutation(request) {
  mutationBusy = true;
  activeMutation = { requestId: request.requestId, operation: request.operation,
    kind: request.params?.kind, startedAt: new Date().toISOString(), phase: "核对文档和图元" };
}

// EDA 的 RPC 可能以普通对象拒绝，String() 只会得到 [object Object]。
function describeError(error) {
  const text = String(error);
  if (!error || typeof error !== "object" || !text.includes("[object Object]")) return text;
  let json;
  try { json = JSON.stringify(error); } catch { json = undefined; }
  if (!json || json === "{}") return text;
  return error instanceof Error ? `${text} ${json}` : json;
}

// 只读诊断：如实记录 lib_Device.get 的返回形状，不据此做任何判断。
function summarizeDevice(device) {
  if (device === undefined || device === null) return { returned: String(device) };
  if (typeof device !== "object") return { returned: typeof device };
  return { uuid: device.uuid, libraryUuid: device.libraryUuid, libraryType: device.libraryType,
    libraryTypeOf: typeof device.libraryType, name: device.name, keys: Object.keys(device).slice(0, 20) };
}

async function probeLibraryIdentities(api, components) {
  const devices = namespaceOf(api, "lib_Device");
  const librariesList = namespaceOf(api, "lib_LibrariesList");
  const probe = { projectLibraryUuid: null, entries: [] };
  try {
    probe.projectLibraryUuid = await readWithTimeout(() => librariesList.getProjectLibraryUuid());
  } catch (error) {
    probe.projectLibraryUuid = { error: describeError(error) };
  }
  const lookup = (uuid, libraryUuid) => readWithTimeout(() => (libraryUuid === undefined
    ? devices.get(uuid) : devices.get(uuid, libraryUuid))).then(summarizeDevice, error => ({ error: describeError(error) }));
  probe.entries = await Promise.all(components.map(async ({ component, item }) => {
    const entry = { designator: item.designator, componentUuid: item.componentUuid, libraryUuid: item.libraryUuid };
    for (const field of ["Supplier", "SupplierId", "Manufacturer", "ManufacturerId"]) {
      entry[field[0].toLowerCase() + field.slice(1)] = await safeCall(stateReader(component, `getState_${field}`));
    }
    entry.lookups = {};
    if (typeof devices?.get === "function" && item.componentUuid) {
      const [declared, located] = await Promise.all([
        item.libraryUuid ? lookup(item.componentUuid, item.libraryUuid) : undefined,
        lookup(item.componentUuid, undefined)]);
      if (declared) entry.lookups.declared = declared;
      entry.lookups.located = located;
    }
    return entry;
  }));
  return probe;
}

function readWithTimeout(read) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("library_lookup_timeout")), 5000);
    Promise.resolve().then(read).then(
      value => { clearTimeout(timer); resolve(value); },
      error => { clearTimeout(timer); reject(error); });
  });
}

// Some client SDKs return a project-local device UUID paired with its source
// library UUID. Never pass that unverified pair to create: the client can leave
// the write Promise pending forever when the device does not exist in that lib.
// API: prodocs.lceda.cn/cn/api/reference/pro-api.lib_librarieslist.getprojectlibraryuuid.html
// 工程库 UUID 与工程 UUID 不同，必须经 lib_LibrariesList 取得。
// 最后不指定库再查一次（libraryUuid 为可选参数），只接受 EDA 自己返回的同一器件及其所属库。
async function findExactDevice(api, spec, diagnostics) {
  const devices = namespaceOf(api, "lib_Device");
  if (!spec.componentUuid) return null;
  if (typeof devices?.get !== "function") {
    diagnostics.push("exact: lib_Device.get_unavailable");
    return null;
  }
  const librariesList = namespaceOf(api, "lib_LibrariesList");
  let projectLibraryUuid;
  try {
    if (typeof librariesList?.getProjectLibraryUuid !== "function") throw new Error("getProjectLibraryUuid_unavailable");
    projectLibraryUuid = await readWithTimeout(() => librariesList.getProjectLibraryUuid());
    if (!projectLibraryUuid) throw new Error("uuid_unavailable");
  } catch (error) {
    diagnostics.push(`project-library: ${describeError(error)}`);
  }
  const candidates = [...new Set([spec.libraryUuid, projectLibraryUuid].filter(Boolean)), undefined];
  for (const requested of candidates) {
    const label = requested ?? "located";
    let device;
    try {
      device = await readWithTimeout(() => (requested === undefined
        ? devices.get(spec.componentUuid) : devices.get(spec.componentUuid, requested)));
    } catch (error) {
      diagnostics.push(`${label}: ${describeError(error)}`);
      continue;
    }
    const libraryUuid = requested ?? device?.libraryUuid;
    if (!device || device.uuid !== spec.componentUuid || typeof libraryUuid !== "string" || !libraryUuid ||
        device.libraryUuid !== libraryUuid || (device.libraryType !== undefined && device.libraryType !== "3")) {
      diagnostics.push(`${label}: exact_device_not_found ${JSON.stringify(summarizeDevice(device))}`);
      continue;
    }
    const parts = Array.isArray(device.subPartNames) ? device.subPartNames : [];
    const subPartName = spec.subPartName ?? (parts.length === 1 ? parts[0] : undefined);
    if ((parts.length > 1 && subPartName === undefined) ||
        (subPartName !== undefined && parts.length > 0 && !parts.includes(subPartName))) {
      throw new Error("library_subpart_unresolved：缺少可核验的子部件，未调用 create");
    }
    return { component: { libraryUuid, uuid: device.uuid }, subPartName };
  }
  return null;
}

// API: prodocs.lceda.cn/cn/api/reference/pro-api.lib_device.getbylcscids.html
// 默认查系统库；全离线客户端没有默认系统库，失败时显式传系统库 UUID 再试一次。
async function findByLcscId(api, spec, diagnostics) {
  const supplierId = typeof spec.supplierId === "string" ? spec.supplierId.trim() : "";
  if (!supplierId) {
    diagnostics.push("lcsc: supplierId_missing");
    return null;
  }
  const devices = namespaceOf(api, "lib_Device");
  if (typeof devices?.getByLcscIds !== "function") {
    diagnostics.push("lcsc: getByLcscIds_unavailable");
    return null;
  }
  let item;
  try {
    item = await readWithTimeout(() => devices.getByLcscIds(supplierId));
  } catch (error) {
    diagnostics.push(`lcsc ${supplierId}: ${describeError(error)}`);
    const librariesList = namespaceOf(api, "lib_LibrariesList");
    const systemLibrary = await readWithTimeout(() => librariesList.getSystemLibraryUuid()).catch(() => undefined);
    if (typeof systemLibrary === "string" && systemLibrary) {
      try {
        item = await readWithTimeout(() => devices.getByLcscIds(supplierId, systemLibrary));
      } catch (retryError) {
        diagnostics.push(`lcsc ${supplierId}@${systemLibrary}: ${describeError(retryError)}`);
      }
    }
  }
  if (Array.isArray(item)) item = item.length === 1 ? item[0] : undefined;
  const reported = item?.otherProperty?.["Supplier Part"];
  if (!item || typeof item.uuid !== "string" || !item.uuid || typeof item.libraryUuid !== "string" || !item.libraryUuid ||
      (reported != null && String(reported).trim().toUpperCase() !== supplierId.toUpperCase())) {
    diagnostics.push(`lcsc ${supplierId}: not_found ${JSON.stringify(summarizeDevice(item))}`);
    return null;
  }
  return { component: { libraryUuid: item.libraryUuid, uuid: item.uuid }, subPartName: spec.subPartName };
}

// 依次：精确库查找 → 立创编号（EDA 核验）→ 原库身份直接重建（未经核验的兜底，
// 身份不对时 create 可能不返回，写入状态会变为未知）。调用 create 前再次核对文档。
async function resolveLibraryComponent(api, spec, document) {
  const diagnostics = [];
  let method = "exact";
  let resolved = await findExactDevice(api, spec, diagnostics);
  if (!resolved) {
    method = "lcsc";
    resolved = await findByLcscId(api, spec, diagnostics);
  }
  if (!resolved && spec.libraryUuid && spec.componentUuid) {
    method = "original_unverified";
    resolved = { component: { libraryUuid: spec.libraryUuid, uuid: spec.componentUuid }, subPartName: spec.subPartName };
  }
  if (!resolved) {
    throw new Error(`library_identity_unresolved：器件 ${spec.componentUuid ?? spec.supplierId} 既无可核验库身份也无原库身份；未调用 create；${diagnostics.join("; ")}`);
  }
  const project = await api.dmt_Project.getCurrentProjectInfo();
  const page = await api.dmt_Schematic.getCurrentSchematicPageInfo();
  if (document.projectUuid !== project?.uuid || document.pageUuid !== page?.uuid) {
    throw new Error("library_document_changed：核验器件库期间切换了文档，未调用 create");
  }
  return { ...resolved, method, diagnostics };
}

// API: prodocs.lceda.cn/cn/api/reference/pro-api.sch_primitivecomponent.modify.html
//      prodocs.lceda.cn/cn/api/reference/pro-api.sch_primitivewire.modify.html
async function mutateProperty(request) {
  const params = request.params || {};
  const api = edaApi();
  const fail = (message, status) => ({ ok: false, payload: { written: false, writeStatus: status || "failed", message }, inconclusive: [] });
  if (mutationBusy) return fail("另一个写入仍在处理中。");
  if (request.protocolVersion !== 1 || !request.planHash || !request.baselineSnapshotHash ||
      !Array.isArray(request.scope) || !request.scope.includes("project.write") ||
      typeof params.expected !== "string" || typeof params.value !== "string" || typeof params.primitiveId !== "string") {
    return fail("写入缺少协议、计划、权限或原值绑定。");
  }
  beginMutation(request);
  let invoked = false;
  try {
    const project = await api.dmt_Project.getCurrentProjectInfo();
    const page = await api.dmt_Schematic.getCurrentSchematicPageInfo();
    if (request.document?.kind !== "schematic" || request.document.projectUuid !== project?.uuid || request.document.pageUuid !== page?.uuid) {
      return fail("当前文档已切换，拒绝写入。");
    }
    const component = params.kind === "component_value";
    if (!component && params.kind !== "wire_net") return fail("不支持的写入类型。");
    const ns = component ? api.sch_PrimitiveComponent : api.sch_PrimitiveWire;
    if (typeof ns?.get !== "function" || typeof ns?.modify !== "function") return fail("当前 EDA 不提供所需读写 API。");
    // Always fetch a fresh primitive: modify's return object may contain stale getters.
    const primitive = await ns.get(params.primitiveId);
    const id = await primitive?.getState_PrimitiveId?.();
    const current = component ? await primitive?.getState_Name?.() : await primitive?.getState_Net?.();
    if (id !== params.primitiveId || current !== params.expected) return fail("目标身份或当前属性已漂移，拒绝覆盖。");
    invoked = true;
    const result = await ns.modify(params.primitiveId, component ? { name: params.value } : { net: params.value });
    if (!result) return fail("API 未确认写入；结果未知。", "inconclusive");
    return { ok: true, payload: { written: true, writeStatus: "passed" }, inconclusive: [] };
  } catch (error) {
    return fail(String(error), invoked ? "inconclusive" : "failed");
  } finally { mutationBusy = false; activeMutation = null; }
}

// API: prodocs.easyeda.com/cn/api/reference/pro-api.sch_primitivecomponent.create.html
//      prodocs.easyeda.com/cn/api/reference/pro-api.sch_primitivecomponent.delete.html
//      prodocs.easyeda.com/cn/api/reference/pro-api.sch_primitivewire.create.html
//      prodocs.easyeda.com/cn/api/reference/pro-api.sch_primitivewire.delete.html
// 以上 create/delete 均为官方 BETA 预览 API。结构类写入（删除/放置/连线）同样
// 要求文档绑定与新鲜读取；新建必须带回图元 ID 作为 Bridge 的回滚句柄，
// 回滚删除按幂等处理（图元已不存在视为已恢复）。
async function pinCoordinates(api, designator, pinNumber) {
  if (typeof designator !== "string" || !designator) return null;
  const componentNs = namespaceOf(api, "sch_PrimitiveComponent");
  if (typeof componentNs?.getAll !== "function") return null;
  const components = await componentNs.getAll();
  for (const component of Array.isArray(components) ? components : []) {
    if ((await safeCall(stateReader(component, "getState_Designator"))) !== designator) continue;
    if (typeof component?.getAllPins !== "function") return null;
    const pins = await safeCall(component.getAllPins.bind(component), null);
    for (const pin of Array.isArray(pins) ? pins : []) {
      const number = await safeCall(stateReader(pin, "getState_PinNumber"));
      if (String(number ?? "") !== String(pinNumber ?? "")) continue;
      const x = await safeCall(stateReader(pin, "getState_X"), undefined);
      const y = await safeCall(stateReader(pin, "getState_Y"), undefined);
      return Number.isFinite(x) && Number.isFinite(y) ? { x, y } : null;
    }
  }
  return null;
}

async function mutateStructure(request) {
  const params = request.params || {};
  const api = edaApi();
  const fail = (message, status) => ({ ok: false, payload: { written: false, writeStatus: status || "failed", message }, inconclusive: [] });
  if (mutationBusy) return fail("另一个写入仍在处理中。");
  if (request.protocolVersion !== 1 || !request.planHash || !request.baselineSnapshotHash ||
      !Array.isArray(request.scope) || !request.scope.includes("project.write") ||
      typeof params.primitiveId !== "string" || !params.spec || typeof params.spec !== "object") {
    return fail("写入缺少协议、计划、权限或规格绑定。");
  }
  beginMutation(request);
  let invoked = false;
  try {
    const project = await api.dmt_Project.getCurrentProjectInfo();
    const page = await api.dmt_Schematic.getCurrentSchematicPageInfo();
    if (request.document?.kind !== "schematic" || request.document.projectUuid !== project?.uuid || request.document.pageUuid !== page?.uuid) {
      return fail("当前文档已切换，拒绝写入。");
    }
    const rollback = request.operation === "rollback";
    const spec = params.spec;

    if (params.kind === "component_delete") {
      const ns = namespaceOf(api, "sch_PrimitiveComponent");
      if (typeof ns?.get !== "function" || typeof ns?.delete !== "function" ||
          typeof ns?.create !== "function" || typeof ns?.modify !== "function") {
        return fail("当前 EDA 不提供所需器件读写 API。");
      }
      if (rollback) {
        const restore = spec.restore || {};
        if ((!(restore.libraryUuid && restore.componentUuid) && !restore.supplierId) ||
            !Number.isFinite(restore.x) || !Number.isFinite(restore.y)) {
          return fail("补偿数据缺少库身份/立创编号或放置，拒绝重建。");
        }
        // 原图元或同位号对象仍在时不得盲目重建，避免超时重试造成重复器件。
        if (typeof ns.getAll !== "function") return fail("无法核对当前器件集合，拒绝重建。");
        const current = await ns.getAll();
        if (!Array.isArray(current)) return fail("无法完整读取当前器件集合，拒绝重建。");
        for (const item of current) {
          const id = await item?.getState_PrimitiveId?.();
          const designator = await item?.getState_Designator?.();
          const type = await safeCall(stateReader(item, "getState_ComponentType"));
          if (!id || (designator == null && !isSchematicSymbol(type))) return fail("器件身份读取不完整，拒绝重建。");
          if (id === params.primitiveId || (restore.designator && designator === restore.designator)) {
            return fail("原图元或同位号器件已经存在；拒绝重复重建，请回读核实。");
          }
        }
        activeMutation.phase = "只读核验器件库身份（库 UUID / 立创编号）";
        sendExecutorMessage({ type: "operation_progress", requestId: request.requestId, operation: request.operation,
          phase: activeMutation.phase, completed: 0, total: 3 });
        const resolved = await resolveLibraryComponent(api, restore, request.document);
        activeMutation.phase = `等待 create 返回（${resolved.method}）；libraryUuid=${resolved.component.libraryUuid}; uuid=${resolved.component.uuid}`;
        sendExecutorMessage({ type: "operation_progress", requestId: request.requestId, operation: request.operation,
          phase: activeMutation.phase, completed: 1, total: 3 });
        invoked = true;
        const created = await ns.create(
          resolved.component,
          restore.x, restore.y, resolved.subPartName, restore.rotation ?? 0, restore.mirror ?? false);
        if (!created) return fail("重建 API 未确认；结果未知。", "inconclusive");
        const createdId = await safeCall(stateReader(created, "getState_PrimitiveId"));
        if (!createdId) return fail("重建器件未返回 ID；结果未知。", "inconclusive");
        activeMutation.phase = "恢复位号和属性";
        sendExecutorMessage({ type: "operation_progress", requestId: request.requestId, operation: request.operation,
          phase: "器件已创建，正在恢复位号和属性", completed: 2, total: 3 });
        if (restore.designator || restore.name != null) {
          const modified = await ns.modify(createdId, {
            ...(restore.designator ? { designator: restore.designator } : {}),
            ...(restore.name != null ? { name: restore.name } : {}),
            ...(restore.uniqueId ? { uniqueId: restore.uniqueId } : {}),
            ...(restore.properties && typeof restore.properties === "object" ? { otherProperty: restore.properties } : {}),
          });
          if (!modified) return fail("重建后属性未确认；结果未知。", "inconclusive");
        }
        return { ok: true, payload: { written: true, writeStatus: "passed", createdId,
          restoreMethod: resolved.method, restoreDiagnostics: resolved.diagnostics }, inconclusive: [] };
      }
      // 删除前新鲜读取并核对位号，防止误删漂移目标。
      const primitive = await ns.get(params.primitiveId);
      const id = await primitive?.getState_PrimitiveId?.();
      const designator = await primitive?.getState_Designator?.();
      if (id !== params.primitiveId || designator !== spec.designator) {
        return fail("目标身份或位号已漂移，拒绝删除。");
      }
      invoked = true;
      const deleted = await ns.delete(params.primitiveId);
      if (deleted !== true) return fail("删除 API 未确认；结果未知。", "inconclusive");
      return { ok: true, payload: { written: true, writeStatus: "passed" }, inconclusive: [] };
    }

    if (params.kind === "component_place") {
      const ns = namespaceOf(api, "sch_PrimitiveComponent");
      if (typeof ns?.get !== "function" || typeof ns?.create !== "function" ||
          typeof ns?.modify !== "function" || typeof ns?.delete !== "function") {
        return fail("当前 EDA 不提供所需器件读写 API。");
      }
      if (rollback) {
        // 幂等删除新建器件：已不存在视为已恢复。
        const primitive = await ns.get(params.primitiveId);
        const id = await primitive?.getState_PrimitiveId?.();
        if (id == null) return { ok: true, payload: { written: true, writeStatus: "passed" }, inconclusive: [] };
        invoked = true;
        const deleted = await ns.delete(params.primitiveId);
        if (deleted !== true) return fail("删除 API 未确认；结果未知。", "inconclusive");
        return { ok: true, payload: { written: true, writeStatus: "passed" }, inconclusive: [] };
      }
      if ((!(spec.libraryUuid && spec.componentUuid) && !spec.supplierId) || !Number.isFinite(spec.x) || !Number.isFinite(spec.y)) {
        return fail("放置规格缺少库身份/立创编号或坐标。");
      }
      activeMutation.phase = "只读核验器件库身份（库 UUID / 立创编号）";
      const resolved = await resolveLibraryComponent(api, spec, request.document);
      activeMutation.phase = `等待 create 返回（${resolved.method}）；libraryUuid=${resolved.component.libraryUuid}; uuid=${resolved.component.uuid}`;
      invoked = true;
      const created = await ns.create(
        resolved.component,
        spec.x, spec.y, resolved.subPartName, spec.rotation ?? 0, spec.mirror ?? false);
      if (!created) return fail("放置 API 未确认；结果未知。", "inconclusive");
      const createdId = await safeCall(stateReader(created, "getState_PrimitiveId"));
      if (!createdId) return fail("新建器件未返回 ID；结果未知。", "inconclusive");
      if (spec.designator || spec.name != null) {
        const modified = await ns.modify(createdId, {
          ...(spec.designator ? { designator: spec.designator } : {}),
          ...(spec.name != null ? { name: spec.name } : {}),
        });
        if (!modified) return fail("放置后属性未确认；结果未知。", "inconclusive");
      }
      return { ok: true, payload: { written: true, writeStatus: "passed", createdId,
        restoreMethod: resolved.method, restoreDiagnostics: resolved.diagnostics }, inconclusive: [] };
    }

    if (params.kind === "wire_create") {
      const wireNs = namespaceOf(api, "sch_PrimitiveWire");
      const componentNs = namespaceOf(api, "sch_PrimitiveComponent");
      if (typeof wireNs?.create !== "function" || typeof wireNs?.get !== "function" ||
          typeof wireNs?.delete !== "function" || typeof componentNs?.getAll !== "function") {
        return fail("当前 EDA 不提供所需导线读写 API。");
      }
      if (rollback) {
        const primitive = await wireNs.get(params.primitiveId);
        const id = await primitive?.getState_PrimitiveId?.();
        if (id == null) return { ok: true, payload: { written: true, writeStatus: "passed" }, inconclusive: [] };
        invoked = true;
        const deleted = await wireNs.delete(params.primitiveId);
        if (deleted !== true) return fail("删除 API 未确认；结果未知。", "inconclusive");
        return { ok: true, payload: { written: true, writeStatus: "passed" }, inconclusive: [] };
      }
      const connection = spec.connection || {};
      const from = await pinCoordinates(api, connection.pin?.designator, connection.pin?.pin);
      if (!from) return fail("引脚坐标不可解析（位号或引脚号不存在）。");
      const anchor = connection.anchor || {};
      let to = null;
      if (Number.isFinite(anchor.x) && Number.isFinite(anchor.y)) {
        to = { x: anchor.x, y: anchor.y };
      } else if (anchor.pin) {
        to = await pinCoordinates(api, anchor.pin.designator, anchor.pin.pin);
        if (!to) return fail("锚点引脚坐标不可解析。");
      } else {
        to = { x: from.x + 100, y: from.y }; // 单脚短桩
      }
      invoked = true;
      const wire = await wireNs.create([from.x, from.y, to.x, to.y], spec.net);
      if (!wire) return fail("导线 API 未确认创建；结果未知。", "inconclusive");
      const createdId = await safeCall(stateReader(wire, "getState_PrimitiveId"));
      if (!createdId) return fail("新建导线未返回 ID；结果未知。", "inconclusive");
      return { ok: true, payload: { written: true, writeStatus: "passed", createdId }, inconclusive: [] };
    }

    return fail("不支持的结构写入类型。");
  } catch (error) {
    return fail(String(error), invoked ? "inconclusive" : "failed");
  } finally { mutationBusy = false; activeMutation = null; }
}

function isStructuralMutation(params) {
  return ["component_delete", "component_place", "wire_create"].includes(params?.kind);
}

async function dispatchOperation(request) {
  const { requestId, operation } = request;
  const progress = (phase, completed, total) => {
    sendExecutorMessage({ type: "operation_progress", requestId, operation, phase, completed, total });
  };
  if (!edaApi()) {
    return {
      type: "operation_error",
      requestId,
      operation,
      code: "eda_api_unavailable",
      message: "EDA 运行时全局对象不可用，无法执行操作。",
      inconclusive: [],
    };
  }
  try {
    if (operation === "apply" || operation === "rollback") {
      const result = await (isStructuralMutation(request.params) ? mutateStructure(request) : mutateProperty(request));
      return { type: "operation_result", requestId, operation, capturedAt: new Date().toISOString(), ...result };
    }
    if (operation === "inspect" || operation === "readback") {
      // 此探针不调用 EDA API；即使 create 尚未返回，也能报告在途操作。
      if (operation === "readback" && request.params?.statusOnly) {
        return { type: "operation_result", requestId, operation, ok: !mutationBusy,
          mutationIdle: !mutationBusy, activeMutation, payload: { executorStatus: true },
          inconclusive: mutationBusy ? ["executorBusy"] : [] };
      }
      if (request.params?.requireIdle && mutationBusy) {
        return { type: "operation_result", requestId, operation, ok: false, mutationIdle: false,
          message: "前次写入仍在执行，不能确认可安全重试。", inconclusive: ["executorBusy"] };
      }
      progress("读取文档与画布信息", 1, 4);
      const result = await inspectSchematic();
      progress("观测完成", 4, 4);
      return {
        type: "operation_result",
        requestId,
        operation,
        ok: !request.params?.requireIdle || !mutationBusy,
        mutationIdle: !mutationBusy,
        capturedAt: result.capturedAt,
        payload: result.payload,
        inconclusive: result.inconclusive,
      };
    }
    if (operation === "read_context") {
      const result = await readContext();
      return {
        type: "operation_result",
        requestId,
        operation,
        ok: true,
        capturedAt: result.capturedAt,
        payload: { context: result.context },
        inconclusive: result.inconclusive,
      };
    }
    if (operation === "drc") {
      progress("正在运行 DRC/ERC", 1, 1);
      const result = await runDrc();
      return {
        type: "operation_result",
        requestId,
        operation,
        ok: true,
        capturedAt: new Date().toISOString(),
        payload: { status: result.status, errors: result.errors },
        inconclusive: result.inconclusive,
      };
    }
    if (operation === "visual_capture") {
      const result = await captureCanvas();
      return {
        type: "operation_result",
        requestId,
        operation,
        ok: true,
        capturedAt: new Date().toISOString(),
        payload: { mime: result.mime, base64: result.base64 },
        inconclusive: result.inconclusive,
      };
    }
    return {
      type: "operation_error",
      requestId,
      operation,
      code: "unsupported_operation",
      message: `不支持的 EDA 操作：${String(operation)}。当前版本仅支持 inspect/read_context/drc/visual_capture。`,
      inconclusive: [],
    };
  } catch (error) {
    return {
      type: "operation_error",
      requestId,
      operation,
      code: "eda_error",
      message: String(error && error.message ? error.message : error),
      inconclusive: [],
    };
  }
}

async function handleOperationMessage(message) {
  if (!message || message.type !== "eda_operation") return null;
  const reply = await dispatchOperation(message);
  reply.extensionVersion = EXTENSION_VERSION;
  sendExecutorMessage(reply);
  return reply;
}

function handleExecutorEvent(event) {
  let raw = event;
  if (event && typeof event === "object" && "data" in event) raw = event.data;
  if (typeof raw !== "string" || !raw) return;
  let message;
  try {
    message = JSON.parse(raw);
  } catch {
    return;
  }
  if (message.type === "executor_hello_ack") {
    executorState.helloAcked = true;
    executorState.operations = Array.isArray(message.operations) ? message.operations : [];
    executorState.unsupportedOperations = Array.isArray(message.unsupportedOperations)
      ? message.unsupportedOperations
      : [];
    return;
  }
  if (message.type === "eda_operation") {
    void handleOperationMessage(message);
  }
}

async function connectExecutor() {
  const api = edaApi();
  if (!api) {
    executorState.status = "unavailable";
    executorState.lastError = "eda 全局对象不可用";
    return;
  }
  const sockets = api.sys_WebSocket;
  if (!sockets || typeof sockets.register !== "function" || typeof sockets.send !== "function") {
    executorState.status = "unavailable";
    executorState.lastError = "sys_WebSocket 不可用（缺少外部交互权限或版本过旧）";
    return;
  }
  executorState.status = "connecting";
  executorState.lastError = null;
  try {
    await sockets.register(
      EXECUTOR_WS_ID,
      executorState.url,
      (event) => handleExecutorEvent(event),
      () => {
        executorState.status = "connected";
        void sendExecutorHello();
      },
    );
  } catch (error) {
    executorState.status = "unavailable";
    executorState.lastError = String(error);
  }
}

function startBridgeBusListener() {
  const api = edaApi();
  const bus = api ? api.sys_MessageBus : undefined;
  if (!bus || typeof bus.pull !== "function") return;
  try {
    bus.pull(BRIDGE_BUS_TOPIC, (message) => {
      const url = normalizeExecutorUrl(message);
      if (!url || url === executorState.url) return;
      executorState.url = url;
      const sockets = edaApi()?.sys_WebSocket;
      if (sockets && typeof sockets.close === "function" && executorState.status === "connected") {
        try {
          sockets.close(EXECUTOR_WS_ID);
        } catch {
          // close is best-effort; register replaces an existing id on reconnect.
        }
      }
      executorState.status = "idle";
      void connectExecutor();
    });
  } catch {
    // MessageBus between iframe and entry is not officially guaranteed.
  }
}

function startExecutorRuntime() {
  if (executorRuntimeStarted) return;
  executorRuntimeStarted = true;
  startBridgeBusListener();
  void connectExecutor();
}

async function openCircuitFabricAssistant() {
  try {
    const api = edaApi();
    if (!api?.sys_IFrame?.openIFrame) {
      notify("当前嘉立创 EDA 运行时不提供 SYS_IFrame，无法打开 CircuitFabric 助手。");
      return;
    }
    startExecutorRuntime();
    const opened = await api.sys_IFrame.openIFrame(
      "/iframe/index.html",
      760,
      620,
      "circuitfabric-assistant",
      {
        title: "CircuitFabric 助手",
        maximizeButton: true,
        minimizeButton: true,
        minimizeStyle: "collapsed",
      },
    );
    if (opened === false) {
      notify("CircuitFabric 助手窗口打开失败：扩展包中缺少 iframe/index.html。");
    }
  } catch (error) {
    notify(`CircuitFabric 助手无法打开：${String(error)}`);
  }
}
return {
  openCircuitFabricAssistant,
  _test: {
    inspectSchematic,
    readContext,
    runDrc,
    captureCanvas,
    handleOperationMessage,
    startExecutorRuntime,
    executorState,
  },
};
})();
