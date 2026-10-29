const { test } = require('node:test');
const assert = require('node:assert/strict');
const { probeEda } = require('./eda_probe.cjs');
test('method presence never verifies lifecycle, missing versions stay null', async () => {
  let writes = 0;
  const result = await probeEda({
    dmt_Project: { getCurrentProjectInfo: () => ({ uuid: 'p' }), create: () => { writes++; } },
    dmt_Schematic: { getCurrentSchematicPageInfo: () => ({ uuid: 'a' }) },
  });
  assert.equal(writes, 0);
  assert.equal(result.methods.dmt_Project.create, true);
  assert.equal(result.lifecycle, 'unverified');
  assert.equal(result.sdkVersion, null);
  assert.equal(result.repeatedPageUuidEqual, true);
  assert.equal(result.restartIdentity, 'unverified');
});
test('switching pages, missing APIs and timeouts do not produce success', async () => {
  let page = 0;
  const switched = await probeEda({ dmt_Schematic: { getCurrentSchematicPageInfo: () => ({ uuid: String(page++) }) } });
  assert.equal(switched.repeatedPageUuidEqual, false);
  assert.equal(switched.calls[0].error, 'method_missing');
  const stalled = await probeEda({ dmt_Project: { getCurrentProjectInfo: () => new Promise(() => {}) } }, {}, 5);
  assert.match(stalled.calls[0].error, /timeout/);
  assert.equal(stalled.projectUuid, null);
});
