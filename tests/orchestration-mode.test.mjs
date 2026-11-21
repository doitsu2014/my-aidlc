import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { completeStage, initProject, json, readActiveState, run, tempProject, writeArtifacts } from './helpers.mjs';

function readAudit(project) {
  return readFileSync(join(project, '.aidlc/audit.jsonl'), 'utf8').trim().split('\n').map((line) => JSON.parse(line));
}

test('config mode defaults to human and accepts ai', (t) => {
  const project = tempProject(t);
  initProject(project);
  assert.deepEqual(json(project, 'config', 'mode'), { mode: 'human', configured: true });
  assert.deepEqual(json(project, 'config', 'mode', 'ai'), { mode: 'ai', appliesTo: 'new intents' });
  assert.deepEqual(json(project, 'config', 'mode'), { mode: 'ai', configured: true });
  assert.equal(run(project, 'config', 'mode', 'robot').status, 1);
});

test('orchestrate next emits orchestrationMode from config and flag', (t) => {
  const project = tempProject(t);
  initProject(project);
  const human = json(project, 'orchestrate', 'next', '--intent', 'human intent');
  assert.equal(human.mode, 'human');
  assert.equal(human.orchestrationMode, 'human');
  assert.equal(readActiveState(project).mode, 'human');

  const other = tempProject(t);
  initProject(other);
  json(other, 'config', 'mode', 'ai');
  const flagged = json(other, 'orchestrate', 'next', '--intent', 'ai intent', '--mode', 'ai');
  assert.equal(flagged.mode, 'ai');
  assert.equal(flagged.orchestrationMode, 'ai');

  const overridden = tempProject(t);
  initProject(overridden);
  json(overridden, 'config', 'mode', 'ai');
  const back = json(overridden, 'orchestrate', 'next', '--intent', 'back to human', '--mode', 'human');
  assert.equal(back.mode, 'human');
  assert.equal(back.orchestrationMode, 'human');
  assert.equal(run(overridden, 'orchestrate', 'next', '--intent', 'x', '--mode', 'robot').status, 1);
});

test('mode cannot change after an intent starts', (t) => {
  const project = tempProject(t);
  initProject(project);
  json(project, 'orchestrate', 'next', '--intent', 'locked mode');
  const refused = run(project, 'orchestrate', 'next', '--intent', 'locked mode', '--mode', 'ai');
  assert.equal(refused.status, 1);
  assert.match(refused.stderr, /mode cannot change/);
});

test('human mode rejects AI Orchestrator as approver', (t) => {
  const project = tempProject(t);
  initProject(project);
  const next = json(project, 'orchestrate', 'next', '--intent', 'human gate');
  writeArtifacts(project, next);
  json(project, 'orchestrate', 'report', '--result', 'complete');
  const gate = json(project, 'orchestrate', 'next');
  writeArtifacts(project, gate);
  const refused = run(project, 'orchestrate', 'report', '--result', 'complete', '--approval', 'approved', '--approved-by', 'AI Orchestrator');
  assert.equal(refused.status, 1);
  assert.match(refused.stderr, /human mode/);
});

test('AI mode requires gate-recommendation and records AI approval', (t) => {
  const project = tempProject(t);
  initProject(project);
  json(project, 'config', 'mode', 'ai');
  const first = json(project, 'orchestrate', 'next', '--intent', 'AI approved work');
  assert.equal(first.orchestrationMode, 'ai');
  writeArtifacts(project, first);
  json(project, 'orchestrate', 'report', '--result', 'complete');

  // Gate stage: approving without a recommendation is refused.
  const gate = json(project, 'orchestrate', 'next');
  assert.equal(gate.stage.phaseGate, true);
  writeArtifacts(project, gate);
  const refused = run(project, 'orchestrate', 'report', '--result', 'complete', '--approval', 'approved', '--approved-by', 'AI Orchestrator');
  assert.equal(refused.status, 1);
  assert.match(refused.stderr, /gate-recommendation/);

  // A human name is refused in AI mode.
  const humanName = run(project, 'orchestrate', 'report', '--result', 'complete', '--approval', 'approved', '--approved-by', 'Reviewer');
  assert.equal(humanName.status, 1);
  assert.match(humanName.stderr, /AI orchestrator mode/);

  // With the recommendation file, AI approval succeeds and is audited.
  const rec = join(gate.intentDir, gate.stage.phase, gate.stage.id, 'gate-recommendation.md');
  mkdirSync(join(gate.intentDir, gate.stage.phase, gate.stage.id), { recursive: true });
  writeFileSync(rec, '# Gate recommendation\n\n## Options\n\nA vs B.\n\n## Evidence\n\nObserved.\n\n## Decision\n\nA.\n');
  const approved = json(project, 'orchestrate', 'report', '--result', 'complete', '--approval', 'approved', '--approved-by', 'AI Orchestrator');
  assert.equal(approved.approval, 'approved');
  assert.equal(approved.approvedBy, 'AI Orchestrator');
  const decided = readAudit(project).find((row) => row.event === 'gate.decided');
  assert.ok(decided);
  assert.equal(decided.mode, 'ai');
  assert.equal(decided.by, 'AI Orchestrator');
  assert.equal(readActiveState(project).gates[decided.gate].mode, 'ai');
});

test('AI mode intent lists and status show the mode', (t) => {
  const project = tempProject(t);
  initProject(project);
  json(project, 'config', 'mode', 'ai');
  json(project, 'orchestrate', 'next', '--intent', 'visible mode');
  const listed = json(project, 'intent', 'list');
  assert.equal(listed[0].mode, 'ai');
  const status = json(project, 'status');
  assert.equal(status.mode, 'ai');
  assert.deepEqual(json(project, 'intent', 'list').map((row) => row.mode), ['ai']);
});

test('AI mode drive to completion uses AI approvals', (t) => {
  const project = tempProject(t);
  initProject(project);
  json(project, 'config', 'mode', 'ai');
  const first = json(project, 'orchestrate', 'next', '--intent', 'full AI run');
  writeArtifacts(project, first);
  json(project, 'orchestrate', 'report', '--result', 'complete');
  for (;;) {
    const next = json(project, 'orchestrate', 'next');
    if (next.complete) break;
    writeArtifacts(project, next);
    if (next.stage.phaseGate) {
      const dir = join(next.intentDir, next.stage.phase, next.stage.id);
      mkdirSync(dir, { recursive: true });
      writeFileSync(join(dir, 'gate-recommendation.md'), '# Gate recommendation\n\nResearched; approved.\n');
      json(project, 'orchestrate', 'report', '--result', 'complete', '--approval', 'approved', '--approved-by', 'AI Orchestrator');
    } else {
      json(project, 'orchestrate', 'report', '--result', 'complete');
    }
  }
  const decided = readAudit(project).filter((row) => row.event === 'gate.decided');
  assert.ok(decided.length > 0);
  for (const row of decided) {
    assert.equal(row.mode, 'ai');
    assert.equal(row.by, 'AI Orchestrator');
  }
});
