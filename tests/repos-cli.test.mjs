import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, existsSync, rmSync, lstatSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const cli = resolve(import.meta.dirname, '../core/tools/dtech-aidlc.mjs');
function git(cwd, ...args) {
  const result = spawnSync('git', args, { cwd, encoding: 'utf8', env: { ...process.env, GIT_CONFIG_NOSYSTEM: '1', GIT_CONFIG_GLOBAL: '/dev/null' } });
  assert.equal(result.status, 0, result.stderr);
}
function invoke(root, ...args) {
  return spawnSync(process.execPath, [cli, ...args, '--project', root], { encoding: 'utf8' });
}
function json(root, ...args) {
  const result = invoke(root, ...args);
  assert.equal(result.status, 0, result.stderr);
  return JSON.parse(result.stdout);
}

test('repos CLI manages multiple checkouts separately from workflow dependencies', t => {
  const base = mkdtempSync(join(tmpdir(), 'aidlc-repos-cli-'));
  t.after(() => rmSync(base, { recursive: true, force: true }));
  const project = join(base, 'project'); mkdirSync(project);
  for (const name of ['alpha', 'beta']) {
    const source = join(base, name); mkdirSync(source);
    git(source, 'init', '-q', '-b', 'main');
    git(source, 'config', 'user.name', 'Test');
    git(source, 'config', 'user.email', 'test@example.invalid');
    writeFileSync(join(source, 'README.txt'), name);
    git(source, 'add', '.'); git(source, 'commit', '-qm', 'initial');
    assert.equal(json(project, 'repos', 'add', name, source, '--ref', 'main').name, name);
  }
  assert.equal(existsSync(join(project, '.aidlc/state.json')), false);
  assert.equal(json(project, 'repos', 'list').length, 2);
  assert.equal(json(project, 'repos', 'pull').every(row => row.ok), true);
  const checkout = join(project, '.aidlc/repos/alpha');
  assert.equal(readFileSync(join(checkout, 'README.txt'), 'utf8'), 'alpha');
  assert.equal(json(project, 'doctor').ok, true);
  writeFileSync(join(checkout, 'README.txt'), 'local change');
  const dirty = invoke(project, 'repos', 'pull', 'alpha');
  assert.equal(dirty.status, 1);
  assert.equal(JSON.parse(dirty.stdout)[0].ok, false);
  assert.equal(readFileSync(join(checkout, 'README.txt'), 'utf8'), 'local change');
  assert.equal(json(project, 'repos', 'remove', 'alpha').removed, true);
  assert.equal(existsSync(checkout), true);
  const declared = join(base, 'declared'); mkdirSync(declared);
  git(declared, 'init', '-q', '-b', 'main');
  git(declared, 'config', 'user.name', 'Test');
  git(declared, 'config', 'user.email', 'test@example.invalid');
  writeFileSync(join(declared, 'README.txt'), 'from-file');
  git(declared, 'add', '.'); git(declared, 'commit', '-qm', 'initial');
  writeFileSync(join(project, 'aidlc.deps.json'), JSON.stringify({ schemaVersion: 1, repos: { fromfile: { url: declared, ref: 'main' } } }));
  const imported = invoke(project, 'repos', 'pull');
  assert.equal(imported.status, 0, imported.stderr);
  assert.match(imported.stderr, /not declared in aidlc\.deps\.json/);
  assert.match(imported.stderr, /beta/);
  assert.equal(JSON.parse(imported.stdout).find(row => row.name === 'fromfile').ok, true);
  assert.equal(readFileSync(join(project, '.aidlc/repos/fromfile/README.txt'), 'utf8'), 'from-file');
  assert.deepEqual(json(project, 'repos', 'list').map(row => row.name), ['fromfile']);
  const bad = invoke(project, 'repos', 'add', '../escape', join(base, 'alpha'));
  assert.equal(bad.status, 1);
  assert.equal(invoke(project, 'repos', 'pull', 'unknown').status, 1);
  assert.equal(invoke(project, 'repos', 'pull', '--bogus').status, 1);
});

test('repos add and remove edit aidlc.deps.json and write audit rows', (t) => {
  const base = mkdtempSync(join(tmpdir(), 'aidlc-repos-edit-'));
  t.after(() => rmSync(base, { recursive: true, force: true }));
  const project = join(base, 'project'); mkdirSync(project);
  const source = join(base, 'manual'); mkdirSync(source);
  git(source, 'init', '-q', '-b', 'main');
  git(source, 'config', 'user.name', 'Test');
  git(source, 'config', 'user.email', 'test@example.invalid');
  writeFileSync(join(source, 'README.txt'), 'manual');
  git(source, 'add', '.'); git(source, 'commit', '-qm', 'initial');
  writeFileSync(join(project, 'aidlc.deps.json'), JSON.stringify({ schemaVersion: 1, repos: {} }, null, 2) + '\n');
  json(project, 'init', '--harness', 'claude', '--scope', 'solo');
  json(project, 'repos', 'add', 'manual', source, '--ref', 'main');
  const declared = JSON.parse(readFileSync(join(project, 'aidlc.deps.json'), 'utf8'));
  assert.ok(declared.repos.manual);
  if (process.platform !== 'win32') {
    assert.equal(lstatSync(join(project, 'aidlc.deps.json')).mode & 0o777, 0o644);
    assert.equal(lstatSync(join(project, '.aidlc/deps.json')).mode & 0o777, 0o600);
  }
  json(project, 'repos', 'pull');
  json(project, 'repos', 'remove', 'manual');
  assert.equal(JSON.parse(readFileSync(join(project, 'aidlc.deps.json'), 'utf8')).repos.manual, undefined);
  const events = readFileSync(join(project, '.aidlc/audit.jsonl'), 'utf8').split(/\r?\n/).filter(Boolean).map((line) => JSON.parse(line)).map((row) => row.event);
  assert.ok(events.includes('repos.added'));
  assert.ok(events.includes('repos.pulled'));
  assert.ok(events.includes('repos.removed'));
  const text = readFileSync(join(project, '.aidlc/audit.jsonl'), 'utf8');
  assert.equal(text.includes(source), false);
});
