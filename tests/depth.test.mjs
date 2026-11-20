import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { driveToCompletion, initProject, json, run, tempProject } from './helpers.mjs';

test('scope solo --depth fast prints the reduced list', (t) => {
  const project = tempProject(t);
  initProject(project);
  const listed = json(project, 'scope', 'solo', '--depth', 'fast');
  assert.deepEqual(listed.stages.map((row) => row.id), ['problem-framing', 'implementation-design', 'red-tests', 'build-and-verify', 'knowledge-writeback']);
});

test('an intent at depth fast completes with reduced stages and a depth.chosen row', (t) => {
  const project = tempProject(t);
  initProject(project);
  json(project, 'orchestrate', 'next', '--intent', 'fast change', '--depth', 'fast', '--approved-by', 'Owner');
  const visited = driveToCompletion(project);
  assert.deepEqual(visited, ['problem-framing', 'implementation-design', 'red-tests', 'build-and-verify', 'knowledge-writeback']);
  const pointer = JSON.parse(readFileSync(join(project, '.aidlc/state.json'), 'utf8'));
  const state = JSON.parse(readFileSync(join(project, '.aidlc/intents', pointer.activeIntentId, 'state.json'), 'utf8'));
  assert.equal(state.depth, 'fast');
  assert.equal(state.stages['context-and-risk'], undefined);
  for (const row of Object.values(state.stages)) assert.ok(['in_progress', 'complete', 'blocked', 'skipped'].includes(row.status));
  const doctor = json(project, 'doctor');
  assert.equal(doctor.ok, true);
  assert.equal(doctor.checks.some((row) => row.status === 'fail' && String(row.message).startsWith('Intent ')), false);
  const audit = readFileSync(join(project, '.aidlc/audit.jsonl'), 'utf8');
  assert.match(audit, /"event":"depth.chosen"/);
  assert.match(audit, /"depth":"fast"/);
  assert.match(audit, /Owner/);
});

test('intent without depth behaves as full', (t) => {
  const project = tempProject(t);
  initProject(project);
  const next = json(project, 'orchestrate', 'next', '--intent', 'default depth');
  assert.equal(next.depth, 'full');
});

test('unknown depth is refused', (t) => {
  const project = tempProject(t);
  initProject(project);
  const result = run(project, 'scope', 'solo', '--depth', 'nonsense');
  assert.equal(result.status, 1);
  assert.match(result.stderr, /full, standard, fast/);
});

test('intent depth without --approved-by is refused', (t) => {
  const project = tempProject(t);
  initProject(project);
  json(project, 'orchestrate', 'next', '--intent', 'change depth later');
  const refused = run(project, 'intent', 'depth', 'fast', '--detail', 'smaller');
  assert.equal(refused.status, 1);
  const ok = json(project, 'intent', 'depth', 'fast', '--approved-by', 'Owner', '--detail', 'smaller');
  assert.equal(ok.to, 'fast');
  const audit = readFileSync(join(project, '.aidlc/audit.jsonl'), 'utf8');
  assert.match(audit, /"event":"depth.changed"/);
});
