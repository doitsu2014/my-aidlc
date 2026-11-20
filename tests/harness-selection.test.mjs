import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, mkdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { tempProject, run, json, initProject } from './helpers.mjs';

const configPath = (project) => join(project, '.aidlc', 'config.json');
const auditRows = (project) => readFileSync(join(project, '.aidlc', 'audit.jsonl'), 'utf8').trim().split(/\r?\n/).map(JSON.parse);

test('bare onboard and init require an explicit harness before creating state', (t) => {
  const project = tempProject(t);
  for (const command of ['onboard', 'init']) {
    const result = run(project, command);
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /--harness is required for a new project/);
    assert.equal(existsSync(join(project, '.aidlc')), false);
  }
});

test('initialized team project safely re-runs without rewriting config', (t) => {
  const project = tempProject(t);
  initProject(project, 'team');
  const before = readFileSync(configPath(project));
  for (const args of [['onboard', '--harness', 'claude'], ['onboard']]) {
    const result = json(project, ...args);
    assert.equal(result.scope, 'team');
    assert.deepEqual(readFileSync(configPath(project)), before);
  }
  const wrongHarness = run(project, 'onboard', '--harness', 'deepseek');
  assert.match(wrongHarness.stderr, /dtech-aidlc harness add deepseek/);
  const wrongScope = run(project, 'onboard', '--scope', 'solo');
  assert.match(wrongScope.stderr, /dtech-aidlc config scope solo/);
});

test('harness add projects another harness and preserves config and audit', (t) => {
  const project = tempProject(t);
  initProject(project);
  const config = JSON.parse(readFileSync(configPath(project), 'utf8'));
  config.custom = 1;
  writeFileSync(configPath(project), `${JSON.stringify(config, null, 2)}\n`);
  const result = json(project, 'harness', 'add', 'deepseek');
  assert.deepEqual(result.harnesses, ['claude', 'deepseek']);
  assert.equal(existsSync(join(project, '.dsh/skills/aidlc/SKILL.md')), true);
  assert.equal(existsSync(join(project, '.dsh/skills/aidlc/.projection.json')), true);
  const saved = JSON.parse(readFileSync(configPath(project), 'utf8'));
  assert.equal(saved.harness, 'claude');
  assert.deepEqual(saved.harnesses, ['claude', 'deepseek']);
  assert.equal(saved.custom, 1);
  assert.equal(statSync(configPath(project)).mode & 0o777, 0o600);
  assert.equal(json(project, 'harness', 'status').harnesses.length, 2);
  const doctor = json(project, 'doctor');
  assert.ok(doctor.checks.some((row) => row.message.includes('Projection claude:')));
  assert.ok(doctor.checks.some((row) => row.message.includes('Projection deepseek:')));
  assert.ok(auditRows(project).some((row) => row.event === 'harness.added' && row.harness === 'deepseek'));
  const rows = auditRows(project).filter((row) => row.event === 'harness.added').length;
  json(project, 'harness', 'add', 'deepseek');
  assert.equal(auditRows(project).filter((row) => row.event === 'harness.added').length, rows);
});

test('config scope changes only the default and preserves extra config fields', (t) => {
  const project = tempProject(t);
  initProject(project, 'team');
  let config = JSON.parse(readFileSync(configPath(project), 'utf8'));
  config.custom = 1;
  writeFileSync(configPath(project), `${JSON.stringify(config, null, 2)}\n`);
  const existing = json(project, 'orchestrate', 'next', '--scope', 'team', '--intent', 'existing team task');
  assert.equal(existing.scope, 'team');
  assert.equal(json(project, 'config', 'scope').scope, 'team');
  const changed = json(project, 'config', 'scope', 'solo');
  assert.equal(changed.appliesTo, 'new intents');
  config = JSON.parse(readFileSync(configPath(project), 'utf8'));
  assert.equal(config.scope, 'solo');
  assert.equal(config.custom, 1);
  assert.ok(auditRows(project).some((row) => row.event === 'config.scope.changed' && row.from === 'team' && row.to === 'solo'));
  assert.equal(json(project, 'status').scope, 'team');
  const nextProject = tempProject(t);
  initProject(nextProject, 'team');
  json(nextProject, 'config', 'scope', 'solo');
  const next = json(nextProject, 'orchestrate', 'next', '--intent', 'new task');
  assert.equal(next.scope, 'solo');
});

test('old config without harnesses remains unchanged while refresh repairs projection', (t) => {
  const project = tempProject(t);
  const dir = join(project, '.aidlc');
  mkdirSync(dir, { recursive: true });
  writeFileSync(configPath(project), '{"harness":"deepseek","scope":"solo","initializedAt":"x"}\n');
  const original = readFileSync(configPath(project));
  assert.equal(json(project, 'harness', 'status').harnesses[0].state, 'missing');
  const doctor = run(project, 'doctor');
  assert.notEqual(doctor.status, 0);
  assert.match(doctor.stdout, /Project configuration and agent models/);
  assert.equal(readFileSync(configPath(project)).equals(original), true);
  assert.equal(run(project, 'harness', 'refresh').status, 0);
  assert.equal(existsSync(join(project, '.dsh/skills/aidlc/SKILL.md')), true);
  assert.equal(json(project, 'doctor').ok, true);
  assert.equal(readFileSync(configPath(project)).equals(original), true);
});

test('invalid configs, missing arguments, and extra arguments fail clearly', (t) => {
  const project = tempProject(t);
  mkdirSync(join(project, '.aidlc'));
  writeFileSync(configPath(project), '{}\n');
  const invalid = run(project, 'init', '--harness', 'claude', '--scope', 'solo');
  assert.match(invalid.stderr, /Invalid \.aidlc\/config\.json/);
  const empty = tempProject(t);
  assert.notEqual(run(empty, 'harness', 'add').status, 0);
  initProject(empty);
  assert.match(run(empty, 'config', 'scope', 'solo', 'extra').stderr, /Unexpected argument 'extra'/);
});
