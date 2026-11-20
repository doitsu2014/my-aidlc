import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { chmodSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const repo = resolve(import.meta.dirname, '..');
const script = join(repo, 'scripts/install.sh');
const skip = process.platform === 'win32' && !process.env.RUN_INSTALLER_TESTS ? 'POSIX installer' : false;

function scratch(t) {
  const dir = mkdtempSync(join(tmpdir(), 'aidlc-inst-'));
  t.after(() => { try { chmodSync(join(dir, 'prefix/bin'), 0o755); } catch {} rmSync(dir, { recursive: true, force: true }); });
  return { dir, prefix: join(dir, 'prefix') };
}
function install(prefix, ...args) { return spawnSync('sh', [script, '--from', repo, '--prefix', prefix, ...args], { encoding: 'utf8' }); }
function launcherVersion(launcher) { return spawnSync('sh', [launcher, 'version'], { encoding: 'utf8' }); }
const paths = (prefix) => [join(prefix, 'lib/dtech-aidlc'), join(prefix, 'bin/dtech-aidlc'), join(prefix, 'share/zsh/site-functions/_dtech-aidlc')];
// A copy of the runtime tree whose package.json carries a higher version.
function newerSource(dir) {
  const source = join(dir, 'newer');
  mkdirSync(source);
  for (const name of ['core', 'harness']) cpSync(join(repo, name), join(source, name), { recursive: true });
  const pkg = JSON.parse(readFileSync(join(repo, 'package.json'), 'utf8'));
  writeFileSync(join(source, 'package.json'), JSON.stringify({ ...pkg, version: '9.9.9' }));
  return source;
}

test('plain install lists the installed files and a second one names every path', { skip }, (t) => {
  const { prefix } = scratch(t);
  const first = install(prefix);
  assert.equal(first.status, 0, first.stderr);
  assert.match(first.stdout, /Installed files:/);
  for (const path of paths(prefix)) assert.ok(first.stdout.includes(path));
  const second = install(prefix);
  assert.notEqual(second.status, 0);
  assert.equal(second.stderr.trim().split('\n').length, 1);
  assert.match(second.stderr, /already exists/);
  assert.match(second.stderr, /--upgrade/);
  for (const path of paths(prefix)) assert.ok(second.stderr.includes(path), path);
});

test('--upgrade replaces the installation and prints the migration instruction', { skip }, (t) => {
  const { dir, prefix } = scratch(t);
  assert.equal(install(prefix).status, 0);
  const source = newerSource(dir);
  const upgraded = spawnSync('sh', [script, '--from', source, '--prefix', prefix, '--upgrade'], { encoding: 'utf8' });
  assert.equal(upgraded.status, 0, upgraded.stderr);
  assert.match(upgraded.stdout, /Upgraded dtech-aidlc \S+ -> 9\.9\.9/);
  assert.match(upgraded.stdout, /dtech-aidlc harness refresh/);
  assert.equal(launcherVersion(join(prefix, 'bin/dtech-aidlc')).stdout.trim(), 'dtech-aidlc 9.9.9');
});

test('a launcher without its library fails in one line and --upgrade repairs it', { skip }, (t) => {
  const { prefix } = scratch(t);
  assert.equal(install(prefix).status, 0);
  rmSync(join(prefix, 'lib/dtech-aidlc'), { recursive: true, force: true });
  const broken = launcherVersion(join(prefix, 'bin/dtech-aidlc'));
  assert.equal(broken.status, 1);
  assert.match(broken.stderr, /installation is incomplete/);
  assert.doesNotMatch(broken.stderr, /Cannot find module/);
  assert.equal(broken.stderr.trim().split('\n').length, 1);
  assert.equal(install(prefix, '--upgrade').status, 0);
  assert.equal(launcherVersion(join(prefix, 'bin/dtech-aidlc')).status, 0);
});

test('the launcher works through a symbolic link', { skip }, (t) => {
  const { dir, prefix } = scratch(t);
  assert.equal(install(prefix).status, 0);
  const link = join(dir, 'elsewhere');
  mkdirSync(link);
  symlinkSync(join(prefix, 'bin/dtech-aidlc'), join(link, 'dtech-aidlc'));
  const result = launcherVersion(join(link, 'dtech-aidlc'));
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout.trim(), /^dtech-aidlc \d+\.\d+\.\d+/);
});

test('--uninstall removes only the three paths and works without a source tree', { skip }, (t) => {
  const { prefix } = scratch(t);
  assert.equal(install(prefix).status, 0);
  writeFileSync(join(prefix, 'bin/other'), 'keep');
  const removed = spawnSync('sh', [script, '--from', '/nonexistent', '--prefix', prefix, '--uninstall'], { encoding: 'utf8' });
  assert.equal(removed.status, 0, removed.stderr);
  for (const path of paths(prefix)) assert.equal(existsSync(path), false);
  assert.equal(readFileSync(join(prefix, 'bin/other'), 'utf8'), 'keep');
  const again = install(prefix, '--uninstall');
  assert.equal(again.status, 0);
  assert.match(again.stdout, /nothing to uninstall/);
});

test('unrecognised files are never touched by --upgrade or --uninstall', { skip }, (t) => {
  const { prefix } = scratch(t);
  assert.equal(install(prefix).status, 0);
  const launcher = join(prefix, 'bin/dtech-aidlc');
  writeFileSync(launcher, '#!/bin/sh\necho mine\n');
  const library = readFileSync(join(prefix, 'lib/dtech-aidlc/package.json'), 'utf8');
  for (const option of ['--upgrade', '--uninstall']) {
    const result = install(prefix, option);
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /not a dtech-aidlc file/);
    assert.equal(readFileSync(launcher, 'utf8'), '#!/bin/sh\necho mine\n');
    assert.equal(readFileSync(join(prefix, 'lib/dtech-aidlc/package.json'), 'utf8'), library);
  }
});

test('--upgrade and --uninstall cannot be combined', { skip }, (t) => {
  const { prefix } = scratch(t);
  const result = install(prefix, '--upgrade', '--uninstall');
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /cannot be combined/);
});

test('a failed upgrade keeps the old library', { skip: skip || (process.getuid?.() === 0 ? 'root ignores permissions' : false) }, (t) => {
  const { dir, prefix } = scratch(t);
  assert.equal(install(prefix).status, 0);
  const before = launcherVersion(join(prefix, 'bin/dtech-aidlc')).stdout.trim();
  chmodSync(join(prefix, 'bin'), 0o555);
  const failed = spawnSync('sh', [script, '--from', newerSource(dir), '--prefix', prefix, '--upgrade'], { encoding: 'utf8' });
  assert.notEqual(failed.status, 0);
  chmodSync(join(prefix, 'bin'), 0o755);
  assert.equal(launcherVersion(join(prefix, 'bin/dtech-aidlc')).stdout.trim(), before);
});
