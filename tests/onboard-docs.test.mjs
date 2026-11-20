import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, writeFileSync, readFileSync, rmSync, existsSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { detectDocumentation } from '../core/tools/onboard.mjs';
import { SOURCE_KINDS } from '../core/tools/knowledge.mjs';

const cli = resolve(import.meta.dirname, '../core/tools/dtech-aidlc.mjs');

function fixture(prefix, setup) {
  const root = mkdtempSync(join(tmpdir(), prefix));
  setup(root);
  return root;
}

function run(root, ...args) {
  return spawnSync(process.execPath, [cli, ...args, '--project', root], { encoding: 'utf8', cwd: root });
}

function write(root, rel, content = 'x\n') {
  const path = join(root, rel);
  mkdirSync(join(path, '..'), { recursive: true });
  writeFileSync(path, content);
}

test('detectDocumentation classifies the owner docs layout', () => {
  const root = fixture('aidlc-docs-detect-', (dir) => {
    write(dir, 'README.md', '# hi\n');
    write(dir, 'docs/architecture.md');
    write(dir, 'docs/adr/0001-use-postgres.md');
    write(dir, 'docs/api/openapi.yaml', 'openapi: 3.0.0\n');
    write(dir, 'docs/runbook.md');
    write(dir, 'docs/BRD.md', '# BRD\n');
  });
  try {
    const { candidates } = detectDocumentation(root);
    assert.ok(candidates.every((row) => SOURCE_KINDS.includes(row.kind)));
    assert.deepEqual(candidates.map((row) => [row.path, row.kind]), [
      ['README.md', 'readme'],
      ['docs', 'docs'],
      ['docs/BRD.md', 'requirements'],
      ['docs/adr', 'adr'],
      ['docs/api/openapi.yaml', 'api-spec'],
      ['docs/architecture.md', 'architecture'],
      ['docs/runbook.md', 'runbook'],
    ]);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('python requirements files are not documentation', () => {
  const root = fixture('aidlc-docs-pip-', (dir) => {
    write(dir, 'requirements.txt', 'flask==3.0\n');
    write(dir, 'requirements-dev.txt');
    write(dir, 'constraints.txt');
    write(dir, 'requirements.in');
    write(dir, 'venv/lib/requirements.txt', 'x\n');
    write(dir, 'venv/pyvenv.cfg', 'x\n');
  });
  try {
    const { candidates } = detectDocumentation(root);
    assert.equal(candidates.some((row) => /requirements|constraints/.test(row.path)), false);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('onboard registers docs by reference and does not copy them', (t) => {
  const root = fixture('aidlc-docs-onboard-', (dir) => {
    write(dir, 'package.json', '{"name":"x"}\n');
    write(dir, 'src/a.ts', 'export {}\n');
    write(dir, 'README.md', '# x\n');
    write(dir, 'docs/architecture.md', 'ZZ-MARKER-ZZ\n');
    write(dir, 'docs/adr/0001-use-postgres.md');
    write(dir, 'docs/api/openapi.yaml', 'openapi: 3.0.0\n');
    write(dir, 'docs/runbook.md');
    write(dir, 'docs/BRD.md', '# BRD\n');
  });
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const result = run(root, 'onboard', '--harness', 'claude', '--scope', 'solo');
  assert.equal(result.status, 0, result.stderr);
  const output = JSON.parse(result.stdout);
  assert.equal(output.knowledge.manifestStatus, 'created');
  assert.equal(output.knowledge.project.path, 'PROJECT.md');
  assert.equal(existsSync(join(root, '.aidlc/shared/brd')), false);
  const manifest = JSON.parse(readFileSync(join(root, 'aidlc.knowledge.json'), 'utf8'));
  const pairs = manifest.sources.map((row) => [row.path, row.kind]);
  assert.deepEqual(pairs, [
    ['PROJECT.md', 'project'],
    ['README.md', 'readme'],
    ['docs', 'docs'],
    ['docs/BRD.md', 'requirements'],
    ['docs/adr', 'adr'],
    ['docs/api/openapi.yaml', 'api-spec'],
    ['docs/architecture.md', 'architecture'],
    ['docs/runbook.md', 'runbook'],
  ]);
  const blob = `${readFileSync(join(root, 'aidlc.knowledge.json'), 'utf8')}${readFileSync(join(root, '.aidlc/shared/project-profile.json'), 'utf8')}`;
  assert.equal(blob.includes('ZZ-MARKER-ZZ'), false);
  assert.equal(blob.includes(root), false);
});

test('onboard --docs missing path writes nothing', () => {
  const root = fixture('aidlc-docs-missing-', () => {});
  try {
    const result = run(root, 'onboard', '--harness', 'claude', '--docs', 'nope');
    assert.equal(result.status, 1);
    assert.match(result.stderr, /--docs path does not exist: nope/);
    assert.equal(existsSync(join(root, '.aidlc')), false);
    assert.equal(existsSync(join(root, 'PROJECT.md')), false);
    assert.equal(existsSync(join(root, 'aidlc.knowledge.json')), false);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('empty project onboard creates no knowledge manifest', () => {
  const root = fixture('aidlc-docs-empty-', () => {});
  try {
    const result = run(root, 'onboard', '--harness', 'claude', '--scope', 'solo');
    assert.equal(result.status, 0, result.stderr);
    const output = JSON.parse(result.stdout);
    assert.equal(output.knowledge.manifestStatus, 'absent');
    assert.equal(existsSync(join(root, 'aidlc.knowledge.json')), false);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
