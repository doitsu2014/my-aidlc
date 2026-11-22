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
  writeFileSync(rec, '# Gate recommendation\n\n## Options\n\nA vs B.\n\n## Evidence\n\nObserved.\n\n## Decision\n\nA.\n\n## Open items\n\n- [follow-up] Tune the retry count after launch.\n');
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
      writeFileSync(join(dir, 'gate-recommendation.md'), '# Gate recommendation\n\nResearched; approved.\n\n## Open items\n\n- None\n');
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

function writeRecommendation(directive, text) {
  const dir = join(directive.intentDir, directive.stage.phase, directive.stage.id);
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, 'gate-recommendation.md'), text);
}

test('AI gate approval counts deferrals in the Open items section', (t) => {
  const project = tempProject(t);
  initProject(project);
  json(project, 'config', 'mode', 'ai');
  const first = json(project, 'orchestrate', 'next', '--intent', 'deferral counting');
  writeArtifacts(project, first);
  json(project, 'orchestrate', 'report', '--result', 'complete');
  const gate = json(project, 'orchestrate', 'next');
  writeArtifacts(project, gate);
  const approve = () => run(project, 'orchestrate', 'report', '--result', 'complete', '--approval', 'approved', '--approved-by', 'AI Orchestrator');

  writeRecommendation(gate, '# Gate recommendation\n\n## Decision\n\nA.\n');
  let refused = approve();
  assert.equal(refused.status, 1);
  assert.match(refused.stderr, /needs an '## Open items' section/);

  writeRecommendation(gate, '# Gate recommendation\n\n## Open items\n\n');
  assert.match(approve().stderr, /empty '## Open items' section/);

  writeRecommendation(gate, '# Gate recommendation\n\n## Open items\n\n- Confirm the retention period.\n');
  assert.match(approve().stderr, /without a \[written-approval\] or \[follow-up\] tag/);

  writeRecommendation(gate, '# Gate recommendation\n\n## Open items\n\n- [written-approval] Owner confirms the retention period.\n- [follow-up] Tune alerts.\n');
  refused = approve();
  assert.equal(refused.status, 1);
  assert.match(refused.stderr, /leaves 1 item\(s\) that need written approval open/);
  assert.match(refused.stderr, /config gate-mode analysis-baseline human/);

  const blocked = run(project, 'orchestrate', 'report', '--result', 'blocked', '--detail', 'Owner must confirm the retention period');
  assert.equal(blocked.status, 0, blocked.stderr);
  const again = json(project, 'orchestrate', 'next');
  assert.equal(again.stage.id, gate.stage.id);
  writeRecommendation(again, '# Gate recommendation\n\n## Open items\n\n- [follow-up] Tune alerts.\n');
  const approved = approve();
  assert.equal(approved.status, 0, approved.stderr);
  const decided = readAudit(project).find((row) => row.event === 'gate.decided');
  assert.deepEqual(decided.openItems, { writtenApproval: 0, followUp: 1 });
});

test('config gate-mode pins a gate to human approval inside an AI-mode intent', (t) => {
  const project = tempProject(t);
  initProject(project);
  json(project, 'config', 'mode', 'ai');
  assert.deepEqual(json(project, 'config', 'gate-mode', 'analysis-baseline', 'human'), { gate: 'analysis-baseline', mode: 'human', appliesTo: 'every intent, from its next report of this gate' });
  const toAi = run(project, 'config', 'gate-mode', 'analysis-baseline', 'ai');
  assert.equal(toAi.status, 1);
  assert.match(toAi.stderr, /never to AI approval/);
  assert.equal(run(project, 'config', 'gate-mode', 'no-such-gate', 'human').status, 1);
  assert.deepEqual(json(project, 'config', 'gate-mode').gateModes, { 'analysis-baseline': 'human' });

  const first = json(project, 'orchestrate', 'next', '--intent', 'pinned gate');
  assert.equal(first.orchestrationMode, 'ai');
  assert.equal(first.gateMode, 'human');
  writeArtifacts(project, first);
  json(project, 'orchestrate', 'report', '--result', 'complete');
  const gate = json(project, 'orchestrate', 'next');
  writeArtifacts(project, gate);
  writeRecommendation(gate, '# Gate recommendation\n\n## Open items\n\n- None\n');
  const ai = run(project, 'orchestrate', 'report', '--result', 'complete', '--approval', 'approved', '--approved-by', 'AI Orchestrator');
  assert.equal(ai.status, 1);
  assert.match(ai.stderr, /pinned to human mode/);
  json(project, 'orchestrate', 'report', '--result', 'complete', '--approval', 'approved', '--approved-by', 'Owner');
  const state = readActiveState(project);
  assert.equal(state.gates['analysis-baseline'].mode, 'human');
  assert.equal(state.gates['analysis-baseline'].pinned, true);

  const ideate = json(project, 'orchestrate', 'next');
  assert.equal(ideate.gateMode, 'ai');
  assert.deepEqual(json(project, 'config', 'gate-mode', 'analysis-baseline', 'default'), { gate: 'analysis-baseline', mode: 'default', appliesTo: 'every intent, from its next report of this gate' });
  assert.equal(json(project, 'config', 'gate-mode').gateModes['analysis-baseline'], undefined);
});
