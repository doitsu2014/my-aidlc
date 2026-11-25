import test from 'node:test';
import assert from 'node:assert/strict';
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const root = resolve(import.meta.dirname, '../..');
const scripts = import.meta.dirname;
const isWindows = process.platform === 'win32';
const hasPwsh = !isWindows || !spawnSync('pwsh', ['-NoProfile', '-Command', '$PSVersionTable.PSVersion.ToString()']).error;
const bashSkip = isWindows ? 'POSIX shell unavailable on Windows' : false;
const pwshSkip = !hasPwsh ? 'pwsh unavailable' : false;

function fixture(t) {
  const dir = mkdtempSync(join(tmpdir(), 'aidlc-rust-inst-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const source = join(dir, 'source');
  const bin = join(source, 'target/release');
  mkdirSync(bin, { recursive: true });
  mkdirSync(join(source, 'assets'));
  writeFileSync(join(source, 'assets/phase-graph.json'), '{"version":1}\n');
  const executable = join(bin, 'dtech-aidlc');
  writeFileSync(executable, '#!/bin/sh\n[ -f "$DTECH_AIDLC_ASSETS/phase-graph.json" ] || exit 9\nprintf "asset-ok\\n"\n');
  chmodSync(executable, 0o755);
  return { dir, source, prefix: join(dir, 'prefix') };
}
function shInstall(f, ...args) {
  return spawnSync('sh', [join(scripts, 'install.sh'), '--from', f.source, '--prefix', f.prefix, ...args], { encoding: 'utf8' });
}
function psInstall(f, ...args) {
  return spawnSync('pwsh', ['-NoProfile', '-File', join(scripts, 'install.ps1'), '-From', f.source, '-Prefix', f.prefix, ...args], { encoding: 'utf8' });
}

test('POSIX prototype installs assets and executable, upgrades recognized Rust install and uninstalls', { skip: bashSkip }, (t) => {
  const f = fixture(t);
  let result = shInstall(f);
  assert.equal(result.status, 0, result.stderr);
  const exe = join(f.prefix, 'lib/dtech-aidlc/dtech-aidlc');
  const run = spawnSync(join(f.prefix, 'bin/dtech-aidlc'), [], { encoding: 'utf8' });
  assert.equal(run.status, 0, run.stderr);
  assert.equal(run.stdout, 'asset-ok\n');
  assert.ok(existsSync(join(f.prefix, 'lib/dtech-aidlc/assets/phase-graph.json')));
  result = shInstall(f, '--upgrade');
  assert.equal(result.status, 0, result.stderr);
  assert.ok(existsSync(exe));
  result = shInstall(f, '--uninstall');
  assert.equal(result.status, 0, result.stderr);
  assert.equal(existsSync(join(f.prefix, 'lib/dtech-aidlc')), false);
});

test('POSIX prototype safely upgrades a recognized Node layout and does not remove it on Rust uninstall', { skip: bashSkip }, (t) => {
  const f = fixture(t);
  mkdirSync(join(f.prefix, 'lib/dtech-aidlc/core/tools'), { recursive: true });
  writeFileSync(join(f.prefix, 'lib/dtech-aidlc/package.json'), '{"name":"dtech-aidlc"}');
  writeFileSync(join(f.prefix, 'lib/dtech-aidlc/core/tools/dtech-aidlc.mjs'), '');
  writeFileSync(join(f.prefix, 'bin/dtech-aidlc'), '#!/bin/sh\\nexec node "$0"\\n');
  let result = shInstall(f, '--upgrade');
  assert.equal(result.status, 0, result.stderr);
  assert.equal(existsSync(join(f.prefix, 'lib/dtech-aidlc/package.json')), false);
  assert.equal(existsSync(join(f.prefix, 'lib/dtech-aidlc/dtech-aidlc')), true);
  result = shInstall(f, '--uninstall');
  assert.equal(result.status, 0, result.stderr);
  assert.equal(existsSync(join(f.prefix, 'lib/dtech-aidlc')), false);
  // A legacy Node launcher outside the canonical install is never targeted.
  writeFileSync(join(f.prefix, 'lib/dtech-aidlc/package.json'), '{"name":"dtech-aidlc"}');
  writeFileSync(join(f.prefix, 'lib/dtech-aidlc/core/tools/dtech-aidlc.mjs'), '');
  assert.equal(shInstall(f, '--uninstall').status, 0);
  assert.equal(existsSync(join(f.prefix, 'lib/dtech-aidlc/package.json')), true);
});

test('PowerShell prototype installs assets and round-trips upgrade/uninstall', { skip: pwshSkip }, (t) => {
  const f = fixture(t);
  const winExe = join(f.source, 'target/release/dtech-aidlc.exe');
  writeFileSync(winExe, 'test executable placeholder');
  let result = psInstall(f);
  assert.equal(result.status, 0, result.stderr + result.stdout);
  assert.ok(existsSync(join(f.prefix, 'lib/dtech-aidlc/assets/phase-graph.json')));
  assert.ok(existsSync(join(f.prefix, 'lib/dtech-aidlc/dtech-aidlc.exe')));
  result = psInstall(f, '-Upgrade');
  assert.equal(result.status, 0, result.stderr + result.stdout);
  result = psInstall(f, '-Uninstall');
  assert.equal(result.status, 0, result.stderr + result.stdout);
  assert.equal(existsSync(join(f.prefix, 'lib/dtech-aidlc')), false);
});

test('PowerShell prototype upgrades recognized Node layout and leaves Node-only install untouched on Rust uninstall', { skip: pwshSkip }, (t) => {
  const f = fixture(t);
  mkdirSync(join(f.prefix, 'lib/dtech-aidlc/core/tools'), { recursive: true });
  writeFileSync(join(f.prefix, 'lib/dtech-aidlc/package.json'), '{"name":"dtech-aidlc"}');
  writeFileSync(join(f.prefix, 'lib/dtech-aidlc/core/tools/dtech-aidlc.mjs'), '');
  let result = psInstall(f, '-Upgrade');
  assert.equal(result.status, 0, result.stderr + result.stdout);
  assert.equal(existsSync(join(f.prefix, 'lib/dtech-aidlc/package.json')), false);
  assert.equal(existsSync(join(f.prefix, 'lib/dtech-aidlc/dtech-aidlc.exe')), true);
  result = psInstall(f, '-Uninstall');
  assert.equal(result.status, 0, result.stderr + result.stdout);
  mkdirSync(join(f.prefix, 'lib/dtech-aidlc/core/tools'), { recursive: true });
  writeFileSync(join(f.prefix, 'lib/dtech-aidlc/package.json'), '{"name":"dtech-aidlc"}');
  writeFileSync(join(f.prefix, 'lib/dtech-aidlc/core/tools/dtech-aidlc.mjs'), '');
  result = psInstall(f, '-Uninstall');
  assert.equal(result.status, 0, result.stderr + result.stdout);
  assert.equal(existsSync(join(f.prefix, 'lib/dtech-aidlc/package.json')), true);
});
