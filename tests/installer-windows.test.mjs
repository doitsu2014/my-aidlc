import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync, cpSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const repo = resolve(import.meta.dirname, '..');
const script = join(repo, 'scripts/install.ps1');
const pwsh = spawnSync('pwsh', ['-NoProfile', '-Command', '$PSVersionTable.PSVersion.ToString()'], { encoding: 'utf8' });
const skip = pwsh.error || pwsh.status !== 0 ? 'pwsh not on PATH' : false;

function scratch(t) {
  const dir = mkdtempSync(join(tmpdir(), 'aidlc-wininst-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  return { dir, prefix: join(dir, 'prefix') };
}
function install(prefix, extra = []) {
  return spawnSync('pwsh', ['-NoProfile', '-File', script, '-From', repo, '-Prefix', prefix, ...extra], { encoding: 'utf8' });
}

test('pwsh install.ps1 creates the new layout and both launchers', { skip }, (t) => {
  const { prefix } = scratch(t);
  const result = install(prefix);
  assert.equal(result.status, 0, result.stderr + result.stdout);
  assert.equal(existsSync(join(prefix, 'lib/dtech-aidlc/core/tools/dtech-aidlc.mjs')), true);
  assert.equal(existsSync(join(prefix, 'lib/dtech-aidlc/harness')), true);
  assert.equal(existsSync(join(prefix, 'bin/dtech-aidlc.cmd')), true);
  assert.equal(existsSync(join(prefix, 'bin/dtech-aidlc')), true);
});

test('the sh launcher has no CR or BOM and prints the version', { skip }, (t) => {
  const { prefix } = scratch(t);
  assert.equal(install(prefix).status, 0);
  const sh = join(prefix, 'bin/dtech-aidlc');
  const buf = readFileSync(sh);
  assert.notEqual(buf[0], 0xEF);
  assert.equal(buf.includes(0x0d), false);
  const version = spawnSync('sh', [sh, 'version'], { encoding: 'utf8' });
  if (!(version.error && version.error.code === 'ENOENT')) {
    assert.equal(version.status, 0, version.stderr);
    assert.match(version.stdout, /^dtech-aidlc /);
  }
  if (process.platform === 'win32') {
    const cmd = spawnSync('cmd', ['/c', join(prefix, 'bin/dtech-aidlc.cmd'), 'version'], { encoding: 'utf8' });
    assert.equal(cmd.status, 0, cmd.stderr);
  }
});

test('a second plain run names -Upgrade; -Upgrade succeeds; -Uninstall removes files', { skip }, (t) => {
  const { prefix } = scratch(t);
  assert.equal(install(prefix).status, 0);
  const second = install(prefix);
  assert.notEqual(second.status, 0);
  assert.match(second.stderr + second.stdout, /-Upgrade/);
  const upgraded = install(prefix, ['-Upgrade']);
  assert.equal(upgraded.status, 0, upgraded.stderr);
  const gone = install(prefix, ['-Uninstall']);
  assert.equal(gone.status, 0, gone.stderr);
  assert.equal(existsSync(join(prefix, 'lib/dtech-aidlc')), false);
  assert.equal(existsSync(join(prefix, 'bin/dtech-aidlc.cmd')), false);
});

test('-Upgrade converts the old lib/core layout', { skip }, (t) => {
  const { prefix } = scratch(t);
  mkdirSync(join(prefix, 'bin'), { recursive: true });
  mkdirSync(join(prefix, 'lib'), { recursive: true });
  cpSync(join(repo, 'core'), join(prefix, 'lib/core'), { recursive: true });
  cpSync(join(repo, 'harness'), join(prefix, 'lib/harness'), { recursive: true });
  cpSync(join(repo, 'package.json'), join(prefix, 'lib/package.json'));
  writeFileSync(join(prefix, 'bin/dtech-aidlc.cmd'), '@echo off\nnode "%~dp0..\\lib\\core\\tools\\dtech-aidlc.mjs" %*\n');
  const upgraded = install(prefix, ['-Upgrade']);
  assert.equal(upgraded.status, 0, upgraded.stderr);
  assert.equal(existsSync(join(prefix, 'lib/core')), false);
  assert.equal(existsSync(join(prefix, 'lib/dtech-aidlc/core')), true);
});

test('DTECH_AIDLC_PREFIX is used when -Prefix is omitted', { skip }, (t) => {
  const { dir, prefix } = scratch(t);
  const result = spawnSync('pwsh', ['-NoProfile', '-File', script, '-From', repo], {
    encoding: 'utf8', env: { ...process.env, DTECH_AIDLC_PREFIX: prefix },
  });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(existsSync(join(prefix, 'lib/dtech-aidlc/package.json')), true);
  void dir;
});

test('a source without harness/ is rejected', { skip }, (t) => {
  const { dir, prefix } = scratch(t);
  const source = join(dir, 'src');
  mkdirSync(join(source, 'core/tools'), { recursive: true });
  mkdirSync(join(source, 'core/data'), { recursive: true });
  writeFileSync(join(source, 'package.json'), '{"name":"dtech-aidlc"}\n');
  writeFileSync(join(source, 'core/tools/dtech-aidlc.mjs'), '');
  writeFileSync(join(source, 'core/tools/lib.mjs'), '');
  writeFileSync(join(source, 'core/data/phase-graph.json'), '{}');
  const result = spawnSync('pwsh', ['-NoProfile', '-File', script, '-From', source, '-Prefix', prefix], { encoding: 'utf8' });
  assert.notEqual(result.status, 0);
  assert.equal(existsSync(join(prefix, 'lib/dtech-aidlc')), false);
});
