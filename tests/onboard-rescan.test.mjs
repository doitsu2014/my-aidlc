import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, utimesSync, writeFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { requiredProjectArtifacts } from '../core/tools/onboard.mjs';
import { completeStage } from './helpers.mjs';

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

test('required project artifacts enforce the codebase map only for scanned existing projects', (t) => {
  const root = docsFixture();
  t.after(() => rmSync(root, { recursive: true, force: true }));
  json(root, 'onboard', '--harness', 'claude', '--scope', 'solo');
  const expected = requiredProjectArtifacts(root, 'context-and-risk');
  assert.equal(expected.length, 1);
  assert.equal(expected[0].path, '.aidlc/knowledge/_codebase-map.md');
  assert.match(expected[0].reason, /kind brownfield/);
  assert.match(expected[0].reason, /write it from .*codebase-map\.md/);
  write(root, expected[0].path);
  assert.deepEqual(requiredProjectArtifacts(root, 'context-and-risk'), []);
  assert.deepEqual(requiredProjectArtifacts(root, 'red-tests'), []);

  const unknown = docsFixture();
  t.after(() => rmSync(unknown, { recursive: true, force: true }));
  json(unknown, 'onboard', '--harness', 'claude', '--scope', 'solo');
  const profilePath = join(unknown, '.aidlc/shared/project-profile.json');
  const profile = JSON.parse(readFileSync(profilePath, 'utf8'));
  profile.kind = 'unknown';
  writeFileSync(profilePath, JSON.stringify(profile));
  assert.match(requiredProjectArtifacts(unknown, 'context-and-risk')[0].reason, /kind unknown/);

  const green = mkdtempSync(join(tmpdir(), 'aidlc-rescan-green-'));
  t.after(() => rmSync(green, { recursive: true, force: true }));
  json(green, 'onboard', '--harness', 'claude', '--scope', 'solo');
  assert.deepEqual(requiredProjectArtifacts(green, 'context-and-risk'), []);
  const unseen = mkdtempSync(join(tmpdir(), 'aidlc-rescan-unseen-'));
  t.after(() => rmSync(unseen, { recursive: true, force: true }));
  assert.deepEqual(requiredProjectArtifacts(unseen, 'context-and-risk'), []);
});

test('context-and-risk refuses completion until its required codebase map exists', (t) => {
  const root = docsFixture();
  t.after(() => rmSync(root, { recursive: true, force: true }));
  json(root, 'onboard', '--harness', 'claude', '--scope', 'solo');
  json(root, 'orchestrate', 'next', '--intent', 'Map gate');
  completeStage(root);
  rmSync(join(root, '.aidlc/knowledge/_codebase-map.md'), { force: true });
  const directive = json(root, 'orchestrate', 'next');
  assert.equal(directive.stage.id, 'context-and-risk');
  for (const item of directive.artifacts) write(root, item.path, '# Expected artifact\n');
  writeFileSync(join(root, '.aidlc/shared/project-profile.json'), JSON.stringify({ schemaVersion: 2, kind: 'brownfield', languages: [] }));
  const before = JSON.stringify(requireAidlcFiles(root));
  const refusal = run(root, 'orchestrate', 'report', '--result', 'complete', '--approval', 'approved', '--approved-by', 'X');
  assert.equal(refusal.status, 1, refusal.stderr || refusal.stdout);
  assert.match(refusal.stderr, /cannot be reported complete/);
  assert.match(refusal.stderr, /missing: \.aidlc\/knowledge\/_codebase-map\.md/);
  assert.equal(JSON.stringify(requireAidlcFiles(root)), before);

  write(root, '.aidlc/knowledge/_codebase-map.md');
  writeFileSync(join(root, '.aidlc/shared/project-profile.json'), JSON.stringify({ schemaVersion: 2, kind: 'brownfield', languages: [] }));
  const passed = run(root, 'orchestrate', 'report', '--result', 'complete', '--approval', 'approved', '--approved-by', 'X');
  assert.equal(passed.status, 0, passed.stderr || passed.stdout);
});

test('blocked context-and-risk is not refused for a missing codebase map', (t) => {
  const root = docsFixture();
  t.after(() => rmSync(root, { recursive: true, force: true }));
  json(root, 'onboard', '--harness', 'claude', '--scope', 'solo');
  json(root, 'orchestrate', 'next', '--intent', 'Blocked map gate');
  completeStage(root);
  const directive = json(root, 'orchestrate', 'next');
  rmSync(join(root, '.aidlc/knowledge/_codebase-map.md'), { force: true });
  for (const item of directive.artifacts) write(root, item.path, '# Expected artifact\n');
  const blocked = run(root, 'orchestrate', 'report', '--result', 'blocked', '--detail', 'waiting');
  assert.equal(blocked.status, 0, blocked.stderr || blocked.stdout);
});

test('completeStage writes a required codebase map for brownfield fixtures', (t) => {
  const root = docsFixture();
  t.after(() => rmSync(root, { recursive: true, force: true }));
  json(root, 'onboard', '--harness', 'claude', '--scope', 'solo');
  json(root, 'orchestrate', 'next', '--intent', 'Helper map');
  completeStage(root);
  completeStage(root);
  assert.equal(readFileSync(join(root, '.aidlc/knowledge/_codebase-map.md'), 'utf8'), '# Codebase map\n');
});

function requireAidlcFiles(root) {
  const files = [];
  function visit(dir) {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) visit(path);
      else files.push([path.slice(root.length), readFileSync(path)]);
    }
  }
  visit(join(root, '.aidlc'));
  return files;
}

test('doctor reports a knowledge sources row', (t) => {
  const root = docsFixture();
  t.after(() => rmSync(root, { recursive: true, force: true }));
  json(root, 'onboard', '--harness', 'claude', '--scope', 'solo');
  const doctor = json(root, 'doctor');
  const row = doctor.checks.find((item) => String(item.message).startsWith('Knowledge sources:'));
  assert.ok(row);
  assert.equal(row.status, 'pass');
});
