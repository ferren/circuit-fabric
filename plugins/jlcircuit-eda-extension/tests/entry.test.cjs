const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const source = fs.readFileSync(path.join(__dirname, '../src/index.js'), 'utf8');
function load(frame) {
  const messages = [];
  const context = vm.createContext({ eda: {
    sys_IFrame: frame,
    sys_Message: { showToastMessage: message => messages.push(message) },
  }});
  vm.runInContext(source, context);
  return { open: context.edaEsbuildExportName.openCircuitFabricAssistant, messages };
}
test('opens iframe without a bridge', async () => {
  let calls = 0;
  const entry = load({ showIFrame: async () => false, openIFrame: async (file, w, h, id) => {
    calls++;
    assert.equal(file, '/iframe/index.html');
    assert.equal(id, 'circuitfabric-assistant');
    return true;
  }});
  await entry.open();
  assert.equal(calls, 1);
  assert.deepEqual(entry.messages, []);
});
test('reports host rejection', async () => {
  const entry = load({ openIFrame: async () => false });
  await entry.open();
  assert.match(entry.messages[0], /缺少 iframe/);
});
test('displays the original host exception and allows retry', async () => {
  const entry = load({ openIFrame: async () => { throw new Error('host failure'); } });
  await entry.open();
  await entry.open();
  assert.equal(entry.messages.length, 2);
  assert.match(entry.messages[0], /host failure/);
});
