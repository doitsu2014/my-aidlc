import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const repo = resolve(import.meta.dirname, '..');

function lint(root) { return spawnSync(process.execPath, [join(root, 'scripts/lint.mjs')], { encoding: 'utf8' }); }
// Lint resolves its root from its own location, so a mutated copy is linted in isolation.
function copy(t) {
  const root = mkdtempSync(join(tmpdir(), 'aidlc-lint-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  for (const name of ['core', 'scripts']) cpSync(join(repo, name), join(root, name), { recursive: true });
  cpSync(join(repo, 'package.json'), join(root, 'package.json'));
  return root;
}
function mutate(root, file, change) {
  const path = join(root, file), text = readFileSync(path, 'utf8').replace(/\r\n/g, '\n');
  const next = change(text);
  assert.notEqual(next, text, 'mutation must change the file');
  writeFileSync(path, next);
}
const README = 'core/methodology/README.md';

test('lint passes on the repository', () => {
  const result = lint(repo);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Lint passed\./);
});

test('a wrong agent count in the methodology README fails lint naming the agents section', (t) => {
  const root = copy(t);
  mutate(root, README, (text) => text.replace('12 agents', '10 agents'));
  const result = lint(root);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /section agents is out of date/);
});

test('a scope list without Archived fails lint naming the scopes section', (t) => {
  const root = copy(t);
  mutate(root, README, (text) => text.replace('- **solo:** Analyze, Ideate, Develop, Archived.', '- **solo:** Analyze, Ideate, Develop.'));
  const result = lint(root);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /section scopes is out of date/);
});

test('a phase without a README section naming its gate fails lint', (t) => {
  const root = copy(t);
  mutate(root, README, (text) => text.replace('### Archived', '### Closing'));
  const result = lint(root);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /phase Archived has no section naming gate/);
});

test('a count written outside a generated section fails lint', (t) => {
  const root = copy(t);
  mutate(root, README, (text) => `${text}\nThere are six phases.\n`);
  const result = lint(root);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /put counts/);
});

test('an agent that calls itself a gate while serving a non-final stage fails lint', (t) => {
  const root = copy(t);
  mutate(root, 'core/methodology/agents/builder-verifier.md', (text) => `${text}\nThis stage is the Develop gate.\n`);
  const result = lint(root);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /calls itself a gate but serves non-final stage red-tests/);
});

test('copied lint still passes', (t) => {
  const result = lint(copy(t));
  assert.equal(result.status, 0, result.stderr);
});

test('a syntax error in onboard.mjs is reported by lint', (t) => {
  const root = copy(t);
  mutate(root, 'core/tools/onboard.mjs', (text) => `${text}\n[[[`);
  const result = lint(root);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /syntax error in core\/tools\/onboard\.mjs/);
});

test('a syntax error in lib.mjs is reported by lint', (t) => {
  const root = copy(t);
  mutate(root, 'core/tools/lib.mjs', (text) => `${text}\n[[[`);
  const result = lint(root);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /syntax error in core\/tools\/lib\.mjs/);
});
