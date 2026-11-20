import test from 'node:test';
import assert from 'node:assert/strict';
import { chmodSync, existsSync, lstatSync, mkdirSync, readFileSync, readdirSync, symlinkSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { mkdtempSync } from 'node:fs';
import { writeFileAtomic, assertNoSymlinkComponents } from '../core/tools/lib.mjs';
import { initProject, json, readActiveState, run, tempProject } from './helpers.mjs';

const LEGACY = {
  version: 1,
  scope: 'solo',
  intent: 'test',
  currentStage: 'problem-framing',
  stages: { 'problem-framing': { status: 'in_progress', startedAt: '2026-11-19T10:51:28.417Z' } },
  dependencies: {},
  createdAt: '2026-11-19T10:51:28.409Z',
};

function writeLegacy(project, extra = {}) {
  mkdirSync(join(project, '.aidlc'), { recursive: true });
  const body = { ...LEGACY, ...extra };
  const text = `${JSON.stringify(body)}\n`;
  writeFileSync(join(project, '.aidlc', 'state.json'), text);
  return text;
}

function walkTmp(dir) {
  const found = [];
  const visit = (path) => {
    for (const name of readdirSync(path, { withFileTypes: true })) {
      const next = join(path, name.name);
      if (name.isDirectory()) visit(next);
      else if (name.name.endsWith('.tmp')) found.push(next);
    }
  };
  if (existsSync(dir)) visit(dir);
  return found;
}

test('legacy state migrates on orchestrate next and is idempotent', (t) => {
  const project = tempProject(t, 'aidlc-mig-legacy-');
  const original = writeLegacy(project);
  const first = json(project, 'orchestrate', 'next');
  assert.equal(first.intentId, 'test');
  assert.equal(first.stage.id, 'problem-framing');
  const pointer = JSON.parse(readFileSync(join(project, '.aidlc', 'state.json'), 'utf8'));
  assert.equal(pointer.version, 2);
  assert.equal(pointer.activeIntentId, 'test');
  assert.ok(existsSync(join(project, '.aidlc', 'intents', 'test', 'state.json')));
  assert.ok(existsSync(join(project, '.aidlc', 'intents', 'test', 'intent.md')));
  assert.equal(readFileSync(join(project, '.aidlc', 'state.v1.bak.json'), 'utf8'), original);
  const audit = readFileSync(join(project, '.aidlc', 'audit.jsonl'), 'utf8').trim().split('\n').map(JSON.parse);
  assert.equal(audit.filter((row) => row.event === 'state.migrated').length, 1);
  json(project, 'orchestrate', 'next');
  assert.equal(existsSync(join(project, '.aidlc', 'intents', 'test-2')), false);
});

test('interrupted migration reuses the matching intent folder', (t) => {
  const project = tempProject(t, 'aidlc-mig-interrupt-');
  writeLegacy(project);
  mkdirSync(join(project, '.aidlc', 'intents', 'test'), { recursive: true });
  writeFileSync(join(project, '.aidlc', 'intents', 'test', 'state.json'), `${JSON.stringify({
    version: 1, intentId: 'test', intent: 'test', createdAt: LEGACY.createdAt, scope: 'solo', stages: {},
  }, null, 2)}\n`);
  const next = json(project, 'orchestrate', 'next');
  assert.equal(next.intentId, 'test');
  assert.equal(existsSync(join(project, '.aidlc', 'intents', 'test-2')), false);
});

test('root content wins over a stale intent mirror', (t) => {
  const project = tempProject(t, 'aidlc-mig-mirror-');
  writeLegacy(project, { intentId: 'keep-edge', dependencies: { a: ['b'] } });
  mkdirSync(join(project, '.aidlc', 'intents', 'keep-edge'), { recursive: true });
  writeFileSync(join(project, '.aidlc', 'intents', 'keep-edge', 'state.json'), `${JSON.stringify({
    version: 1, intentId: 'keep-edge', intent: 'test', scope: 'solo', stages: {}, dependencies: {},
  }, null, 2)}\n`);
  json(project, 'orchestrate', 'next');
  const state = JSON.parse(readFileSync(join(project, '.aidlc', 'intents', 'keep-edge', 'state.json'), 'utf8'));
  assert.deepEqual(state.dependencies, { a: ['b'] });
});

test('truncated pointer is refused without writes', (t) => {
  const project = tempProject(t, 'aidlc-mig-trunc-');
  mkdirSync(join(project, '.aidlc', 'intents'), { recursive: true });
  const path = join(project, '.aidlc', 'state.json');
  writeFileSync(path, '{');
  const before = readFileSync(path);
  const status = run(project, 'status');
  const next = run(project, 'orchestrate', 'next', '--intent', 'Other');
  assert.equal(status.status, 1);
  assert.equal(next.status, 1);
  assert.match(status.stderr, /cannot be parsed/);
  assert.match(next.stderr, /No changes were made/);
  assert.deepEqual(readFileSync(path), before);
  assert.deepEqual(readdirSync(join(project, '.aidlc', 'intents')), []);
});

test('unrecognised root state is refused', (t) => {
  const project = tempProject(t, 'aidlc-mig-unrec-');
  mkdirSync(join(project, '.aidlc'), { recursive: true });
  writeFileSync(join(project, '.aidlc', 'state.json'), `${JSON.stringify({ hello: 1 })}\n`);
  const result = run(project, 'orchestrate', 'next', '--intent', 'Other');
  assert.equal(result.status, 1);
  assert.match(result.stderr, /is not a recognised state file/);
});

test('invalid legacy scope and invalid per-intent state fail without writes', (t) => {
  const invalidLegacy = tempProject(t, 'aidlc-mig-galaxy-');
  writeLegacy(invalidLegacy, { scope: 'galaxy' });
  const legacyResult = run(invalidLegacy, 'orchestrate', 'next');
  assert.equal(legacyResult.status, 1);
  assert.match(legacyResult.stderr, /is invalid/);
  assert.equal(existsSync(join(invalidLegacy, '.aidlc', 'state.v1.bak.json')), false);
  assert.equal(existsSync(join(invalidLegacy, '.aidlc', 'intents')), false);

  const project = tempProject(t, 'aidlc-mig-bad-intent-');
  initProject(project);
  json(project, 'orchestrate', 'next', '--intent', 'broken scope');
  const state = readActiveState(project);
  const before = snapshotAidlc(join(project, '.aidlc'));
  writeFileSync(join(project, '.aidlc', 'intents', state.intentId, 'state.json'), `${JSON.stringify({ ...state, scope: 'galaxy' }, null, 2)}\n`);
  const result = run(project, 'orchestrate', 'next');
  assert.equal(result.status, 1);
  assert.match(result.stderr, /scope/);
  assert.match(result.stderr, new RegExp(state.intentId));
});

test('symlinked .aidlc and intents paths are refused', (t) => {
  const project = tempProject(t, 'aidlc-mig-link-');
  const target = mkdtempSync(join(tmpdir(), 'aidlc-link-target-'));
  t.after(() => { try { require('node:fs').rmSync(target, { recursive: true, force: true }); } catch {} });
  symlinkSync(target, join(project, '.aidlc'));
  const init = run(project, 'init', '--harness', 'claude', '--scope', 'solo');
  assert.equal(init.status, 1);
  assert.match(init.stderr, /Refusing to access symlink \.aidlc directory/);
  assert.deepEqual(readdirSync(target), []);

  const other = tempProject(t, 'aidlc-mig-intents-link-');
  mkdirSync(join(other, '.aidlc'));
  const intentsTarget = mkdtempSync(join(tmpdir(), 'aidlc-intents-target-'));
  t.after(() => { try { require('node:fs').rmSync(intentsTarget, { recursive: true, force: true }); } catch {} });
  symlinkSync(intentsTarget, join(other, '.aidlc', 'intents'));
  const status = run(other, 'status');
  assert.equal(status.status, 1);
  assert.match(status.stderr, /Refusing to write through symlink: \.aidlc\/intents/);
});

test('writeFileAtomic and assertNoSymlinkComponents refuse links', (t) => {
  const dir = tempProject(t, 'aidlc-mig-atomic-');
  const target = join(dir, 'target.txt');
  writeFileSync(target, 'keep');
  const link = join(dir, 'link.txt');
  symlinkSync(target, link);
  assert.throws(() => writeFileAtomic(link, 'nope'), /Refusing to write through symlink:/);
  assert.equal(readFileSync(target, 'utf8'), 'keep');
  mkdirSync(join(dir, 'a'));
  symlinkSync(join(dir, 'a'), join(dir, 'a-link'));
  mkdirSync(join(dir, 'a-link-real'));
  symlinkSync(join(dir, 'a'), join(dir, 'nested'));
  assert.throws(() => {
    mkdirSync(join(dir, 'x'));
    symlinkSync(join(dir, 'x'), join(dir, 'a', 'b'));
    assertNoSymlinkComponents(dir, 'a/b/c.txt');
  }, /a\/b/);
  assert.doesNotThrow(() => assertNoSymlinkComponents(dir, 'missing/b/c.txt'));
});

test('successful commands leave no tmp files and use mode 600', (t) => {
  const project = tempProject(t, 'aidlc-mig-mode-');
  initProject(project);
  json(project, 'orchestrate', 'next', '--intent', 'Add CSV export');
  assert.deepEqual(walkTmp(join(project, '.aidlc')), []);
  if (process.platform === 'win32') return;
  const pointerMode = lstatSync(join(project, '.aidlc', 'state.json')).mode & 0o777;
  const stateMode = lstatSync(join(project, '.aidlc', 'intents', 'add-csv-export', 'state.json')).mode & 0o777;
  assert.equal(pointerMode, 0o600);
  assert.equal(stateMode, 0o600);
});

test('status on legacy state is read-only and reports pending migration', (t) => {
  const project = tempProject(t, 'aidlc-mig-status-');
  const original = writeLegacy(project);
  const before = snapshotAidlc(join(project, '.aidlc'));
  const status = json(project, 'status');
  assert.equal(status.migration, 'pending');
  assert.equal(status.intentId, null);
  assert.deepEqual(snapshotAidlc(join(project, '.aidlc')), before);
  assert.equal(readFileSync(join(project, '.aidlc', 'state.json'), 'utf8'), original);
});

test('deps add writes the per-intent state and leaves a three-key pointer', (t) => {
  const project = tempProject(t, 'aidlc-mig-deps-');
  initProject(project);
  json(project, 'orchestrate', 'next', '--intent', 'deps edge');
  json(project, 'deps', 'add', 'a', 'b');
  const state = readActiveState(project);
  assert.deepEqual(state.dependencies.a, ['b']);
  const pointer = JSON.parse(readFileSync(join(project, '.aidlc', 'state.json'), 'utf8'));
  assert.deepEqual(Object.keys(pointer).sort(), ['activeIntentId', 'updatedAt', 'version']);
});

test('fresh intent uses a pointer, v2 state, and no Project line', (t) => {
  const project = tempProject(t, 'aidlc-mig-fresh-');
  initProject(project);
  json(project, 'orchestrate', 'next', '--intent', 'Add CSV export');
  const pointer = JSON.parse(readFileSync(join(project, '.aidlc', 'state.json'), 'utf8'));
  assert.deepEqual(Object.keys(pointer).sort(), ['activeIntentId', 'updatedAt', 'version']);
  assert.equal(pointer.version, 2);
  assert.equal(pointer.activeIntentId, 'add-csv-export');
  const state = readActiveState(project);
  assert.equal(state.version, 2);
  assert.equal(Object.hasOwn(state, 'project'), false);
  const md = readFileSync(join(project, '.aidlc', 'intents', 'add-csv-export', 'intent.md'), 'utf8');
  assert.equal(/^Project:/m.test(md), false);
  const blob = `${readFileSync(join(project, '.aidlc', 'intents', 'add-csv-export', 'state.json'), 'utf8')}${readFileSync(join(project, '.aidlc', 'audit.jsonl'), 'utf8')}`;
  assert.equal(blob.includes(project), false);
});

function snapshotAidlc(dir) {
  const out = {};
  const visit = (path, rel = '') => {
    for (const name of readdirSync(path)) {
      const next = join(path, name);
      const key = rel ? `${rel}/${name}` : name;
      const stat = lstatSync(next);
      if (stat.isDirectory()) visit(next, key);
      else if (stat.isSymbolicLink()) out[key] = 'symlink';
      else out[key] = readFileSync(next);
    }
  };
  if (existsSync(dir)) visit(dir);
  return out;
}
