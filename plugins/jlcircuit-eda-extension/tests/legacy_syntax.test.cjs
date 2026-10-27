const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

test('the page loads a single connection and event controller', () => {
  const html = fs.readFileSync(path.join(__dirname, '../iframe/index.html'), 'utf8');
  assert.deepEqual(html.match(/<script\b[^>]*>/g), ['<script src="/iframe/compat.js">']);
  assert.equal(html.includes('default-project'), false);
  assert.match(html, /id="send"[^>]*disabled/);
  assert.match(html, /class="header-controls"[\s\S]*id="projectSelect"[\s\S]*id="sessionSelect"[\s\S]*id="bridge"[\s\S]*<\/header>/);
  assert.equal(html.includes('class="settings"'), false, 'the bridge address stays under the header selectors');
});

test('the page controller uses only conservative startup syntax', () => {
  const script = fs.readFileSync(path.join(__dirname, '../iframe/compat.js'), 'utf8');
  for (const unsupported of ['const ', 'let ', '=>', 'async ', 'await ', '?.', '??', '.finally(', '.replaceChildren(', 'new Option', 'selectedOptions', 'localStorage', 'Promise', 'crypto.randomUUID']) {
    assert.equal(script.includes(unsupported), false, `unexpected token: ${unsupported}`);
  }
});
