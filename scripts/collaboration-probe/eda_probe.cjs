// Run in the extension entry context, never an iframe or desktop mock.
// This probe performs reads only. Existence of a method is not verification.
async function probeEda(api, versions = {}, timeoutMs = 3000) {
  const calls = [];
  const read = async (namespace, method) => {
    const record = { namespace, method, status: 'unverified' };
    calls.push(record);
    if (typeof api?.[namespace]?.[method] !== 'function') {
      record.error = 'method_missing';
      return null;
    }
    let timer;
    try {
      record.value = await Promise.race([
        Promise.resolve().then(() => api[namespace][method]()),
        new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('timeout')), timeoutMs); }),
      ]);
      record.status = 'observed';
      return record.value;
    } catch (error) { record.error = String(error); return null; }
    finally { clearTimeout(timer); }
  };
  const project = await read('dmt_Project', 'getCurrentProjectInfo');
  const page = await read('dmt_Schematic', 'getCurrentSchematicPageInfo');
  const again = await read('dmt_Schematic', 'getCurrentSchematicPageInfo');
  const methods = {};
  for (const ns of ['dmt_Project', 'dmt_Schematic', 'dmt_EditorControl', 'sch_Document']) {
    methods[ns] = {};
    for (const method of ['create', 'open', 'rename', 'save', 'saveAll', 'delete', 'getCurrentProjectInfo', 'getCurrentSchematicPageInfo']) {
      methods[ns][method] = typeof api?.[ns]?.[method] === 'function';
    }
  }
  return { schemaVersion: 1, probeVersion: '1.0.0', collectedAt: new Date().toISOString(),
    runtimeVersion: versions.runtimeVersion ?? null, sdkVersion: versions.sdkVersion ?? null,
    scope: 'readonly current project/page, same-context repeated identity only',
    projectUuid: project?.uuid ?? null, pageUuid: page?.uuid ?? null,
    repeatedPageUuidEqual: !!page?.uuid && page.uuid === again?.uuid,
    lifecycle: 'unverified', restartIdentity: 'unverified', multiInstance: 'unverified',
    methods, calls };
}
module.exports = { probeEda };
