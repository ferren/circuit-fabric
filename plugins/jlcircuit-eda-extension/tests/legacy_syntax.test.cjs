const { test } = require('node:test');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');

test('the v0.2.5 page stays byte-identical except for an optional external compatibility script', () => {
  const html = fs.readFileSync(path.join(__dirname, '../iframe/index.html'), 'utf8');
  const injected = '<script src="/iframe/compat.js"></script>';
  assert.equal(html.includes(injected), true);
  const baseline = html.replace(injected, '');
  assert.equal(crypto.createHash('sha256').update(baseline).digest('hex'), '60bf505e57e8b41e12024bb8752957d2281cae1ef4bc5f56373b94ce398d3abc');
});

test('the optional compatibility script uses only conservative startup syntax', () => {
  const script = fs.readFileSync(path.join(__dirname, '../iframe/compat.js'), 'utf8');
  for (const unsupported of ['const ', 'let ', '=>', 'async ', 'await ', '?.', '??', '.finally(', '.replaceChildren(', 'new Option', 'selectedOptions', 'localStorage', 'Promise', 'crypto.randomUUID']) {
    assert.equal(script.includes(unsupported), false, `unexpected token: ${unsupported}`);
  }
});