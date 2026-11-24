import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { listIntents } from '../core/tools/workflow.mjs';
import { readGraph } from '../core/tools/lib.mjs';
import { driveToCompletion, initProject, json, readActiveState, run, tempProject, writeArtifacts } from './helpers.mjs';

test('a finished intent can be followed by another, listed, paused, resumed, and abandoned', (t) => {
  const project = tempProject(t, 'aidlc-life-');
  initProject(project);
  json(project, 'orchestrate', 'next', '--intent', 'First task');
  driveToCompletion(project);
  const afterLast = json(project, 'status');
  assert.equal(afterLast.status, 'complete');
  const firstStatePath = join(project, '.aidlc', 'intents', afterLast.intentId, 'state.json');
  const firstBytes = readFileSync(firstStatePath);
  const foldersBefore = readdirSync(join(project, '.aidlc', 'intents'));
  const hint = json(project, 'orchestrate', 'next', '--intent', 'First task');
  assert.equal(hint.complete, true);
  assert.match(hint.hint, /already complete/);
  assert.deepEqual(readdirSync(join(project, '.aidlc', 'intents')), foldersBefore);
  const second = json(project, 'orchestrate', 'next', '--intent', 'Second task');
  assert.equal(second.intentId, 'second-task');
  assert.equal(second.stage.id, 'problem-framing');
  assert.deepEqual(readFileSync(firstStatePath), firstBytes);
  assert.equal(JSON.parse(readFileSync(join(project, '.aidlc', 'state.json'), 'utf8')).activeIntentId, 'second-task');

  const refused = run(project, 'orchestrate', 'next', '--intent', 'Third task');
  assert.equal(refused.status, 1);
  assert.match(refused.stderr, /second-task/);
  assert.match(refused.stderr, /intent pause/);
  assert.match(refused.stderr, /intent abandon/);
  const continued = json(project, 'orchestrate', 'next', '--intent', 'Second  task ');
  assert.equal(continued.intentId, 'second-task');

  const listed = json(project, 'intent', 'list');
  assert.equal(listed.length, 2);
  assert.equal(listed[0].status, 'complete');
  assert.equal(listed[1].status, 'active');
  assert.equal(listed[1].current, true);
  const imported = listIntents(project, readGraph());
  assert.deepEqual(imported, listed);
  assert.equal(JSON.parse(readFileSync(join(project, '.aidlc', 'state.json'), 'utf8')).activeIntentId, 'second-task');

  json(project, 'intent', 'pause');
  const pausedStatus = json(project, 'status');
  assert.equal(pausedStatus.activeIntent, null);
  assert.deepEqual(pausedStatus.pausedIntents, ['second-task']);
  assert.equal(json(project, 'intent', 'list').find((row) => row.intentId === 'second-task').status, 'paused');
  const noActive = run(project, 'orchestrate', 'next');
  assert.equal(noActive.status, 1);
  assert.match(noActive.stderr, /No active intent/);
  assert.match(noActive.stderr, /intent resume/);
  const resumed = json(project, 'intent', 'resume', 'second-task');
  assert.equal(resumed.resumed, true);
  assert.equal(json(project, 'orchestrate', 'next').stage.id, continued.stage.id);

  json(project, 'intent', 'pause');
  const other = json(project, 'orchestrate', 'next', '--intent', 'Other unfinished');
  assert.equal(run(project, 'intent', 'resume', 'second-task').status, 1);
  assert.match(run(project, 'intent', 'resume', 'second-task').stderr, new RegExp(other.intentId));
  json(project, 'intent', 'pause');
  json(project, 'intent', 'resume', 'second-task');

  assert.equal(run(project, 'intent', 'abandon').status, 1);
  const abandoned = json(project, 'intent', 'abandon', '--detail', 'superseded');
  assert.equal(abandoned.abandoned, true);
  assert.equal(json(project, 'intent', 'list').find((row) => row.intentId === 'second-task').status, 'abandoned');
  assert.equal(JSON.parse(readFileSync(join(project, '.aidlc', 'state.json'), 'utf8')).activeIntentId, null);
  const audit = readFileSync(join(project, '.aidlc', 'audit.jsonl'), 'utf8');
  assert.match(audit, /intent.abandoned/);
  assert.match(audit, /superseded/);
  assert.match(run(project, 'intent', 'resume', 'second-task').stderr, /was abandoned/);
  assert.match(run(project, 'intent', 'abandon', 'second-task', '--detail', 'x').stderr, /already abandoned/);

  const firstId = listed[0].intentId;
  const shown = json(project, 'status', '--intent', firstId);
  assert.ok(shown.stages['knowledge-writeback']);
  assert.match(run(project, 'status', '--intent', 'nope').stderr, /Unknown intent 'nope'/);
});

test('intent rework returns a blocked Develop stage to implementation-design', (t) => {
  const project = tempProject(t, 'aidlc-life-rework-');
  initProject(project);
  const design = json(project, 'orchestrate', 'next', '--intent', 'Split the export intent');
  assert.equal(design.stage.id, 'problem-framing');

  let current = design;
  for (let guard = 0; guard < 30 && current.stage.id !== 'red-tests'; guard += 1) {
    writeArtifacts(project, current);
    const extra = current.stage.phaseGate ? ['--approval', 'approved', '--approved-by', 'Owner'] : [];
    assert.equal(run(project, 'orchestrate', 'report', '--result', 'complete', ...extra).status, 0);
    current = json(project, 'orchestrate', 'next');
  }
  assert.equal(current.stage.id, 'red-tests');
  writeArtifacts(project, current);
  const blocked = run(project, 'orchestrate', 'report', '--result', 'blocked', '--detail', 'The approved work-unit split needs revision');
  assert.equal(blocked.status, 0, blocked.stderr);
  const beforeInvalid = readActiveState(project);
  assert.equal(beforeInvalid.stages['red-tests'].reports.at(-1).result, 'blocked');
  const reselected = json(project, 'orchestrate', 'next');
  assert.equal(reselected.stage.id, 'red-tests');
  assert.equal(reselected.stage.rework.attempt, 2);

  const missingReason = run(project, 'intent', 'rework');
  assert.equal(missingReason.status, 1);
  assert.match(missingReason.stderr, /Usage: intent rework --detail/);
  const stillBlocked = readActiveState(project);
  assert.equal(stillBlocked.stages['red-tests'].reports.at(-1).result, 'blocked');
  assert.equal(stillBlocked.currentStage, 'red-tests');

  const requested = json(project, 'intent', 'rework', '--detail', 'Owner authorized splitting WU-06');
  assert.deepEqual(requested, {
    reworkRequested: true,
    intentId: 'split-the-export-intent',
    fromStage: 'red-tests',
    nextStage: 'implementation-design',
  });
  const next = json(project, 'orchestrate', 'next');
  assert.equal(next.stage.id, 'implementation-design');
  assert.deepEqual(next.stage.rework, {
    requestedAt: next.stage.rework.requestedAt,
    fromStage: 'red-tests',
    detail: 'Owner authorized splitting WU-06',
  });
  assert.equal(next.stage.rework.requestedAt.length > 0, true);
  const state = readActiveState(project);
  assert.equal(state.stages['implementation-design'].status, 'in_progress');
  assert.equal(state.stages['red-tests'].status, 'in_progress');
  assert.equal(state.units, undefined);
  assert.equal(readFileSync(join(project, '.aidlc', 'audit.jsonl'), 'utf8').includes('design.rework.requested'), true);
});

test('knowledge filename occupies an intent id', (t) => {
  const project = tempProject(t, 'aidlc-life-know-');
  initProject(project);
  mkdirSync(join(project, '.aidlc', 'knowledge'), { recursive: true });
  const knowledge = join(project, '.aidlc', 'knowledge', 'add-csv-export.md');
  writeFileSync(knowledge, '# prior\n');
  const before = readFileSync(knowledge);
  const next = json(project, 'orchestrate', 'next', '--intent', 'Add CSV export');
  assert.equal(next.intentId, 'add-csv-export-2');
  assert.deepEqual(readFileSync(knowledge), before);
});

test('intent resume quarantines a corrupt pointer', (t) => {
  const project = tempProject(t, 'aidlc-life-corrupt-');
  initProject(project);
  json(project, 'orchestrate', 'next', '--intent', 'Recover me');
  const id = readActiveState(project).intentId;
  writeFileSync(join(project, '.aidlc', 'state.json'), '{broken');
  const resume = json(project, 'intent', 'resume', id);
  assert.equal(resume.resumed, true);
  const corrupt = readdirSync(join(project, '.aidlc')).filter((name) => name.startsWith('state.json.corrupt-'));
  assert.equal(corrupt.length, 1);
  assert.equal(readFileSync(join(project, '.aidlc', corrupt[0]), 'utf8'), '{broken');
  assert.equal(json(project, 'status').intentId, id);
});

test('help describes intent list pause semantics', () => {
  const help = run(process.cwd(), 'help');
  assert.match(help.stdout, /intent list/);
  assert.match(help.stdout, /paused means: not the active intent of this checkout/);
});
