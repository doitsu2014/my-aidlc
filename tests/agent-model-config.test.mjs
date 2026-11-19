import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, existsSync, symlinkSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const repo = resolve(import.meta.dirname, '..');
const cli = join(repo, 'core/tools/my-aidlc.mjs');
const graphPath = join(repo, 'core/data/phase-graph.json');
function temp() { return mkdtempSync(join(tmpdir(), 'aidlc-models-')); }
function run(project, ...args) { return spawnSync(process.execPath, [cli, ...args, '--project', project], { cwd: project, encoding: 'utf8' }); }
function json(project, ...args) { const result = run(project, ...args); assert.equal(result.status, 0, result.stderr || result.stdout); return JSON.parse(result.stdout); }
function init(project) { json(project, 'init'); }

test('config agents lists defaults, set/reset all aliases, and preserves config keys', t => {
  const project = temp(); t.after(() => rmSync(project, { recursive: true, force: true }));
  const graph = readFileSync(graphPath, 'utf8');
  let rows = json(project, 'config', 'agents');
  assert.equal(rows.length, 12);
  assert.ok(rows.every(row => row.model === row.defaultModel && row.source === 'default'));
  assert.equal(existsSync(join(project, '.aidlc')), false);
  init(project);
  const path = join(project, '.aidlc/config.json');
  const original = JSON.parse(readFileSync(path, 'utf8'));
  const aliases = ['opus', 'sonnet', 'haiku', 'fable'];
  for (let i = 0; i < aliases.length; i++) {
    const id = rows[i].id;
    json(project, 'config', 'agents', 'set', id, aliases[i]);
    const config = JSON.parse(readFileSync(path, 'utf8'));
    assert.equal(config.agentModels[id], aliases[i]);
    assert.equal(config.harness, original.harness);
    assert.equal(config.scope, original.scope);
    assert.equal(config.initializedAt, original.initializedAt);
  }
  rows = json(project, 'config', 'agents');
  assert.equal(rows[3].model, 'fable'); assert.equal(rows[3].source, 'project');
  json(project, 'config', 'agents', 'reset', rows[3].id);
  assert.equal(json(project, 'config', 'agents').find(row => row.id === rows[3].id).source, 'default');
  assert.equal(readFileSync(graphPath, 'utf8'), graph);
});

test('failed setters reject malformed/unknown inputs without changing config', t => {
  const project = temp(); t.after(() => rmSync(project, { recursive: true, force: true }));
  const noConfig = run(project, 'config', 'agents', 'set', 'brief-analyst', 'fable');
  assert.notEqual(noConfig.status, 0); assert.equal(existsSync(join(project, '.aidlc/config.json')), false);
  init(project);
  const path = join(project, '.aidlc/config.json');
  const original = readFileSync(path, 'utf8');
  for (const args of [['set', 'not-an-agent', 'opus'], ['set', 'brief-analyst', 'bad'], ['reset', 'unknown']]) {
    assert.notEqual(run(project, 'config', 'agents', ...args).status, 0);
    assert.equal(readFileSync(path, 'utf8'), original);
  }
  for (const invalid of ['{', '[]', '{"harness":"claude","scope":"solo","initializedAt":"x","agentModels":[]}', '{"harness":"claude","scope":"solo","initializedAt":"x","agentModels":{"brief-analyst":"bad"}}', '{"harness":"claude","scope":"solo","initializedAt":"x","agentModels":{"stale":"opus"}}']) {
    writeFileSync(path, invalid);
    const before = readFileSync(path, 'utf8');
    assert.notEqual(run(project, 'config', 'agents', 'set', 'brief-analyst', 'haiku').status, 0);
    assert.equal(readFileSync(path, 'utf8'), before);
  }
});

test('next directive and scope include resolved model and provenance; doctor detects invalid config', t => {
  const project = temp(); t.after(() => rmSync(project, { recursive: true, force: true }));
  init(project);
  let next = json(project, 'orchestrate', 'next', '--intent', 'Test agent routing');
  assert.equal(next.stage.model, 'sonnet'); assert.equal(next.stage.modelSource, 'default');
  assert.equal(next.stage.phaseFile, join(repo, 'core/methodology/phases/analyze.md'));
  assert.equal(next.stage.agentFile, join(repo, 'core/methodology/agents/brief-analyst.md'));
  const selected = next.stage.agent;
  json(project, 'config', 'agents', 'set', selected, 'fable');
  const scoped = json(project, 'scope', 'solo').stages.find(stage => stage.agent === selected);
  assert.equal(scoped.model, 'fable'); assert.equal(scoped.source, 'project');
  json(project, 'orchestrate', 'report', '--result', 'complete');
  next = json(project, 'orchestrate', 'next');
  assert.equal(next.stage.modelSource, 'default');
  writeFileSync(join(project, '.aidlc/config.json'), '{bad');
  const diagnosis = run(project, 'doctor');
  assert.notEqual(diagnosis.status, 0);
  assert.equal(JSON.parse(diagnosis.stdout).checks.some(check => check.status === 'fail' && /configuration/.test(check.message)), true);
  assert.notEqual(run(project, 'orchestrate', 'next').status, 0);
});

test('refuses symlinked state directory and config file', t => {
  const project = temp(); const outside = temp(); t.after(() => { rmSync(project, { recursive: true, force: true }); rmSync(outside, { recursive: true, force: true }); });
  symlinkSync(outside, join(project, '.aidlc'));
  assert.notEqual(run(project, 'config', 'agents', 'set', 'brief-analyst', 'fable').status, 0);
  rmSync(join(project, '.aidlc'));
  mkdirSync(join(project, '.aidlc'));
  writeFileSync(join(outside, 'config.json'), '{"harness":"claude","scope":"solo","initializedAt":"x"}');
  symlinkSync(join(outside, 'config.json'), join(project, '.aidlc/config.json'));
  assert.notEqual(run(project, 'config', 'agents').status, 0);
});
