import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { completeStage, initProject, json, run, tempProject, writeArtifacts } from './helpers.mjs';

test('an unlisted approver is refused and an allowed name is recorded', (t) => {
  const project = tempProject(t);
  initProject(project);
  const configPath = join(project, '.aidlc/config.json');
  const config = JSON.parse(readFileSync(configPath, 'utf8'));
  config.roles = { owner: ['Ana'], approvers: { 'analysis-baseline': ['Ana', 'Binh'] } };
  writeFileSync(configPath, `${JSON.stringify(config, null, 2)}\n`);
  json(project, 'orchestrate', 'next', '--intent', 'roles');
  completeStage(project);
  const next = json(project, 'orchestrate', 'next');
  writeArtifacts(project, next);
  const refused = run(project, 'orchestrate', 'report', '--result', 'complete', '--approval', 'approved', '--approved-by', 'Cara');
  assert.equal(refused.status, 1);
  assert.match(refused.stderr, /Allowed: Ana, Binh/);
  const ok = run(project, 'orchestrate', 'report', '--result', 'complete', '--approval', 'approved', '--approved-by', 'Ana');
  assert.equal(ok.status, 0, ok.stderr);
  const audit = readFileSync(join(project, '.aidlc/audit.jsonl'), 'utf8');
  assert.match(audit, /"event":"gate.decided"/);
  assert.match(audit, /"by":"Ana"/);
});

test('intent assign is shown on intent list', (t) => {
  const project = tempProject(t);
  initProject(project);
  json(project, 'orchestrate', 'next', '--intent', 'assign me');
  json(project, 'intent', 'assign', 'Binh');
  const listed = json(project, 'intent', 'list');
  assert.equal(listed[0].assignee, 'Binh');
});

test('doctor warns when the default scope has no allowed approver', (t) => {
  const project = tempProject(t);
  initProject(project);
  const doctor = json(project, 'doctor');
  assert.equal(doctor.ok, true);
  assert.ok(doctor.checks.some((row) => row.status === 'warn' && /No allowed approver/.test(row.message)));
});
