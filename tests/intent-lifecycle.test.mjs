import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { listIntents } from '../core/tools/workflow.mjs';
import { readGraph } from '../core/tools/lib.mjs';
import { driveToCompletion, initProject, json, readActiveState, run, tempProject } from './helpers.mjs';

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
