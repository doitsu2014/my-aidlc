import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, mkdtemp, readFileSync, writeFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { promisify } from 'node:util';

const repo = resolve(import.meta.dirname, '..');
const cli = join(repo, 'core/tools/my-aidlc.mjs');
const installScript = join(repo, 'scripts/install.sh');
const mkdtempAsync = promisify(mkdtemp);

function temporaryDirectory(prefix) {
  return mkdtempSync(join(tmpdir(), prefix));
}

function runCli(project, ...args) {
  return spawnSync(process.execPath, [cli, ...args, '--project', project], {
    cwd: project,
    encoding: 'utf8',
  });
}

function cliJson(project, ...args) {
  const result = runCli(project, ...args);
  assert.equal(result.status, 0, result.stderr || result.stdout);
  return JSON.parse(result.stdout);
}

test('scope output contains only stages in the declared phase membership', (t) => {
  const project = temporaryDirectory('aidlc-scope-');
  t.after(() => rmSync(project, { recursive: true, force: true }));
  const byScope = cliJson(project, 'scope');
  const phaseGraph = JSON.parse(readFileSync(join(repo, 'core/data/phase-graph.json'), 'utf8'));
  const phaseMembership = Object.fromEntries(Object.entries(phaseGraph.scopes).map(([scope, phases]) => [scope, new Set(phaseGraph.stages.filter((stage) => phases.includes(stage.phase)).map((stage) => stage.id))]));
  for (const row of byScope) {
    const result = cliJson(project, 'scope', row.scope);
    assert.equal(result.scope, row.scope);
    assert.deepEqual(new Set(result.stages.map((stage) => stage.id)), phaseMembership[row.scope]);
    assert.deepEqual(new Set(row.stages), phaseMembership[row.scope]);
  }
  for (const phaseList of Object.values(phaseMembership)) {
    for (const stage of phaseGraph.stages) {
      assert.equal(phaseList.has(stage.id), phaseGraph.scopes[Object.keys(phaseMembership).find((scope) => phaseMembership[scope] === phaseList)].includes(stage.phase));
    }
  }
});

test('init projects Claude and DeepSeek harnesses and does not overwrite local files', (t) => {
  const project = temporaryDirectory('aidlc-init-');
  t.after(() => rmSync(project, { recursive: true, force: true }));
  for (const [harness, base] of [['claude', '.claude'], ['deepseek', '.dsh']]) {
    const targetProject = join(project, harness);
    mkdirSync(targetProject);
    const result = runCli(targetProject, 'init', '--harness', harness, '--scope', 'team');
    assert.equal(result.status, 0, result.stderr || result.stdout);
    const details = JSON.parse(result.stdout);
    assert.equal(details.initialized, true);
    assert.equal(details.harness, harness);
    assert.equal(details.scope, 'team');
    const projection = join(targetProject, base, 'skills', 'aidlc');
    assert.ok(existsSync(projection));
    assert.ok(existsSync(join(projection, 'manifest.mjs')));
    assert.ok(existsSync(join(targetProject, '.aidlc', 'methodology')));
    const config = JSON.parse(readFileSync(join(targetProject, '.aidlc', 'config.json'), 'utf8'));
    assert.equal(config.harness, harness);
    assert.equal(config.scope, 'team');
    const preservedPath = join(projection, 'local-note.txt');
    writeFileSync(preservedPath, 'keep this file');
    const repeat = runCli(targetProject, 'init', '--harness', harness, '--scope', 'team');
    assert.equal(repeat.status, 0, repeat.stderr || repeat.stdout);
    assert.equal(readFileSync(preservedPath, 'utf8'), 'keep this file');
    assert.deepEqual(JSON.parse(readFileSync(join(targetProject, '.aidlc', 'config.json'), 'utf8')), config);
  }
});

test('init rejects invalid scope and harness values without creating project state', (t) => {
  const project = temporaryDirectory('aidlc-invalid-init-');
  t.after(() => rmSync(project, { recursive: true, force: true }));
  for (const options of [['--harness', 'unknown'], ['--scope', 'unknown']]) {
    const result = runCli(project, 'init', ...options);
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /harness must be|scope must be/);
  }
  assert.equal(existsSync(join(project, '.aidlc')), false);
});

test('workflow keeps the original intent, advances in order, and records audit events', (t) => {
  const project = temporaryDirectory('aidlc-flow-');
  t.after(() => rmSync(project, { recursive: true, force: true }));
  const intent = 'Build an inventory API without changing existing auth behavior';
  let next = cliJson(project, 'orchestrate', 'next', '--scope', 'solo', '--intent', intent);
  assert.equal(next.intent, intent);
  assert.equal(next.scope, 'solo');
  assert.equal(next.stage.id, 'problem-framing');
  assert.equal(cliJson(project, 'status').intent, intent);

  const report = cliJson(project, 'orchestrate', 'report', '--result', 'complete');
  assert.equal(report.stage, 'problem-framing');
  next = cliJson(project, 'orchestrate', 'next');
  assert.equal(next.intent, intent);
  assert.equal(next.stage.id, 'context-and-risk');
  const state = JSON.parse(readFileSync(join(project, '.aidlc', 'state.json'), 'utf8'));
  assert.equal(state.intent, intent);
  assert.equal(state.stages['problem-framing'].status, 'complete');
  assert.equal(state.stages['context-and-risk'].status, 'in_progress');
  assert.equal(state.project, project);
  const tracked = JSON.parse(readFileSync(join(project, '.aidlc', 'intents', state.intentId, 'state.json'), 'utf8'));
  assert.equal(tracked.currentStage, 'context-and-risk');
  assert.match(readFileSync(join(project, '.aidlc', 'intents', state.intentId, 'intent.md'), 'utf8'), /inventory API/);

  const audit = readFileSync(join(project, '.aidlc', 'audit.jsonl'), 'utf8').trim().split('\n').map(JSON.parse);
  assert.ok(audit.some((event) => event.event === 'intent.created' && event.intent === intent));
  assert.ok(audit.some((event) => event.event === 'stage.selected' && event.stage === 'problem-framing'));
  assert.ok(audit.some((event) => event.event === 'stage.reported' && event.stage === 'problem-framing'));
});

test('phase-final completion requires explicit approval; blocked and skipped reports do not', (t) => {
  const project = temporaryDirectory('aidlc-approval-');
  t.after(() => rmSync(project, { recursive: true, force: true }));
  cliJson(project, 'orchestrate', 'next', '--scope', 'solo', '--intent', 'Review approval boundaries');
  const ordered = ['problem-framing', 'context-and-risk', 'option-generation', 'direction-selection', 'implementation-design', 'build-and-verify'];
  const phaseFinals = new Set(['context-and-risk', 'direction-selection', 'build-and-verify']);
  for (const stage of ordered) {
    const current = cliJson(project, 'orchestrate', 'next');
    assert.equal(current.stage.id, stage);
    if (phaseFinals.has(stage)) {
      const report = runCli(project, 'orchestrate', 'report', '--result', 'complete');
      assert.notEqual(report.status, 0);
      assert.match(report.stderr, /requires explicit --approval approved/);
      const state = JSON.parse(readFileSync(join(project, '.aidlc', 'state.json'), 'utf8'));
      assert.notEqual(state.stages[stage]?.status, 'complete');
      const approved = cliJson(project, 'orchestrate', 'report', '--result', 'complete', '--approval', 'approved');
      assert.equal(approved.phaseGate, true);
      assert.equal(approved.approval, 'approved');
    } else {
      const completed = cliJson(project, 'orchestrate', 'report', '--result', 'complete');
      assert.equal(completed.phaseGate, false);
    }
  }
  const final = runCli(project, 'orchestrate', 'next');
  assert.equal(final.status, 0, final.stderr);
  assert.equal(JSON.parse(final.stdout).complete, true);
});

test('dependency commands validate identifiers, deduplicate edges, and expose dependency status', (t) => {
  const project = temporaryDirectory('aidlc-deps-');
  t.after(() => rmSync(project, { recursive: true, force: true }));
  cliJson(project, 'orchestrate', 'next', '--intent', 'Track prerequisites');
  cliJson(project, 'orchestrate', 'report', '--result', 'complete');
  cliJson(project, 'orchestrate', 'next');
  cliJson(project, 'orchestrate', 'report', '--result', 'complete', '--approval', 'approved');
  cliJson(project, 'deps', 'add', 'build-task', 'problem-framing');
  cliJson(project, 'deps', 'add', 'build-task', 'problem-framing');
  const listed = cliJson(project, 'deps', 'list');
  assert.deepEqual(listed, [{ id: 'build-task', dependency: 'problem-framing' }]);
  const pull = cliJson(project, 'deps', 'pull', 'build-task');
  assert.equal(pull.id, 'build-task');
  assert.deepEqual(pull.dependencies, [{ id: 'problem-framing', status: 'complete' }]);
  for (const args of [['deps', 'add', 'bad/id', 'other'], ['deps', 'add', 'same', 'same']]) {
    const result = runCli(project, ...args);
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /Invalid|cannot depend on itself/);
  }
});

test('local installer creates a working isolated launcher and refuses replacement', async (t) => {
  const scratch = await mkdtempAsync(join(tmpdir(), 'aidlc-installer-'));
  const prefix = join(scratch, 'prefix');
  t.after(() => rmSync(scratch, { recursive: true, force: true }));
  const installed = spawnSync('sh', [installScript, '--from', repo, '--prefix', prefix], { cwd: scratch, encoding: 'utf8' });
  assert.equal(installed.status, 0, installed.stderr || installed.stdout);
  const launcher = join(prefix, 'bin', 'my-aidlc');
  const help = spawnSync('sh', [launcher, '--help'], { encoding: 'utf8' });
  assert.equal(help.status, 0, help.stderr || help.stdout);
  assert.equal(existsSync(join(prefix, 'lib', 'my-aidlc', '.git')), false);

  const rerun = spawnSync('sh', [installScript, '--from', repo, '--prefix', prefix], { cwd: scratch, encoding: 'utf8' });
  assert.notEqual(rerun.status, 0);
  assert.match(rerun.stderr, /already exists/);
  assert.ok(existsSync(launcher));
  const completion = join(prefix, 'share/zsh/site-functions/_my-aidlc');
  assert.equal(readFileSync(completion, 'utf8').startsWith('#compdef my-aidlc\n'), true);
  assert.match(installed.stdout, /fpath=/);
});

test('doctor validates clean initialization and status remains local to project', (t) => {
  const project = temporaryDirectory('aidlc-doctor-');
  t.after(() => rmSync(project, { recursive: true, force: true }));
  cliJson(project, 'init', '--harness', 'deepseek', '--scope', 'enterprise');
  assert.equal(cliJson(project, 'doctor').ok, true);
  assert.deepEqual(cliJson(project, 'status'), { initialized: true, activeIntent: null });
});
