import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, utimesSync, writeFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const cli = resolve(import.meta.dirname, '../core/tools/dtech-aidlc.mjs');

function run(root, ...args) {
  return spawnSync(process.execPath, [cli, ...args, '--project', root], { encoding: 'utf8', cwd: root });
}

function json(root, ...args) {
  const result = run(root, ...args);
  assert.equal(result.status, 0, result.stderr || result.stdout);
  return JSON.parse(result.stdout);
}

function write(root, rel, content = 'x\n') {
  mkdirSync(join(root, rel, '..'), { recursive: true });
  writeFileSync(join(root, rel), content);
}

function docsFixture() {
  const root = mkdtempSync(join(tmpdir(), 'aidlc-rescan-'));
  write(root, 'package.json', '{"name":"x"}\n');
  write(root, 'src/a.ts', 'export {}\n');
  write(root, 'README.md', '# x\n');
  write(root, 'docs/architecture.md', 'arch\n');
  write(root, 'docs/adr/0001-use-postgres.md', 'adr\n');
  write(root, 'docs/api/openapi.yaml', 'openapi: 3.0.0\n');
  write(root, 'docs/runbook.md', 'run\n');
  write(root, 'docs/BRD.md', '# BRD\n');
  return root;
}

test('onboard writes a sources snapshot and rescan reports document changes', (t) => {
  const root = docsFixture();
  t.after(() => rmSync(root, { recursive: true, force: true }));
  json(root, 'onboard', '--harness', 'claude', '--scope', 'solo');
  const snapshotPath = join(root, '.aidlc/shared/sources-snapshot.json');
  assert.equal(existsSync(snapshotPath), true);
  const snapshot = JSON.parse(readFileSync(snapshotPath, 'utf8'));
  assert.equal(snapshot.schemaVersion, 1);
  assert.ok(snapshot.files.length > 0);
  assert.ok(snapshot.files.every((file) => file.sha256 === null || file.sha256.length === 64));
  assert.equal(JSON.stringify(snapshot).includes(root), false);
  if (process.platform !== 'win32') assert.equal(readFileSync(snapshotPath).length > 0, true);

  writeFileSync(join(root, 'docs/architecture.md'), 'arch changed\n');
  write(root, 'docs/adr/0007.md', 'new adr\n');
  rmSync(join(root, 'docs/runbook.md'));
  const first = json(root, 'onboard', '--rescan');
  assert.equal(first.rescanned, true);
  assert.ok(first.changesSinceLastScan.changed.includes('docs/architecture.md'));
  assert.ok(first.changesSinceLastScan.added.includes('docs/adr/0007.md'));
  assert.ok(first.missing.some((row) => row.path === 'docs/runbook.md'));
  const second = json(root, 'onboard', '--rescan');
  assert.deepEqual(second.changesSinceLastScan.changed, []);
  assert.deepEqual(second.changesSinceLastScan.added, []);
});

test('mtime-only changes are not reported as content changes', (t) => {
  const root = docsFixture();
  t.after(() => rmSync(root, { recursive: true, force: true }));
  json(root, 'onboard', '--harness', 'claude', '--scope', 'solo');
  const file = join(root, 'docs/architecture.md');
  const later = new Date(Date.now() + 10_000);
  utimesSync(file, later, later);
  const result = json(root, 'onboard', '--rescan');
  assert.equal(result.changesSinceLastScan.changed.includes('docs/architecture.md'), false);
});

test('rescan does not rewrite config or skill and can add --docs paths', (t) => {
  const root = docsFixture();
  t.after(() => rmSync(root, { recursive: true, force: true }));
  json(root, 'onboard', '--harness', 'deepseek', '--scope', 'team');
  const config = readFileSync(join(root, '.aidlc/config.json'));
  const skill = readFileSync(join(root, '.dsh/skills/aidlc/SKILL.md'));
  const manifest = readFileSync(join(root, 'aidlc.knowledge.json'));
  mkdirSync(join(root, 'adr'));
  writeFileSync(join(root, 'adr/0001.md'), 'later\n');
  const dry = json(root, 'onboard', '--rescan');
  assert.deepEqual(readFileSync(join(root, '.aidlc/config.json')), config);
  assert.deepEqual(readFileSync(join(root, '.dsh/skills/aidlc/SKILL.md')), skill);
  assert.deepEqual(readFileSync(join(root, 'aidlc.knowledge.json')), manifest);
  assert.ok(dry.knowledge.wouldAdd.some((row) => row.path === 'adr'));
  const added = json(root, 'onboard', '--rescan', '--docs', 'adr');
  assert.ok(added.knowledge.added.includes('adr') || added.knowledge.added.includes('adr-2') || added.knowledge.registered?.includes('adr'));
  const audit = readFileSync(join(root, '.aidlc/audit.jsonl'), 'utf8').trim().split('\n').map(JSON.parse);
  const registered = audit.filter((row) => row.event === 'knowledge.sources.registered');
  const rescanned = audit.filter((row) => row.event === 'project.rescanned');
  assert.ok(registered.at(-1).added.includes('adr'));
  assert.ok(rescanned.length >= 1);
});

test('rescan without a manifest does not create one', (t) => {
  const root = mkdtempSync(join(tmpdir(), 'aidlc-rescan-empty-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  json(root, 'init', '--harness', 'claude', '--scope', 'solo');
  mkdirSync(join(root, 'adr'));
  writeFileSync(join(root, 'adr/1.md'), 'x\n');
  const result = json(root, 'onboard', '--rescan');
  assert.equal(result.knowledge.manifestStatus, 'absent');
  assert.equal(existsSync(join(root, 'aidlc.knowledge.json')), false);
  assert.ok(result.knowledge.wouldAdd.some((row) => row.path === 'adr'));
  assert.equal(result.knowledge.registerAllCommand, 'dtech-aidlc onboard --harness claude --scope solo');
});

test('rescan rejects conflicting flags and uninitialized projects', (t) => {
  const root = mkdtempSync(join(tmpdir(), 'aidlc-rescan-err-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  assert.match(run(root, 'onboard', '--rescan').stderr, /Project is not initialized/);
  json(root, 'init', '--harness', 'claude', '--scope', 'solo');
  assert.match(run(root, 'onboard', '--rescan', '--harness', 'claude').stderr, /--rescan re-scans an initialized project/);
  assert.match(run(root, 'onboard', '--rescan=yes').stderr, /Option --rescan does not take a value/);
  assert.match(run(root, 'onboard', '--rescan', '--docs', 'docs').stderr, /aidlc.knowledge.json does not exist/);
});

test('doctor reports a knowledge sources row', (t) => {
  const root = docsFixture();
  t.after(() => rmSync(root, { recursive: true, force: true }));
  json(root, 'onboard', '--harness', 'claude', '--scope', 'solo');
  const doctor = json(root, 'doctor');
  const row = doctor.checks.find((item) => String(item.message).startsWith('Knowledge sources:'));
  assert.ok(row);
  assert.equal(row.status, 'pass');
});
