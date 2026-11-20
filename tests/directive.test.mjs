import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { orchestrateNext } from '../core/tools/workflow.mjs';
import { initProject, json, run, tempProject, writeArtifacts } from './helpers.mjs';

test('stage directives include gate fields and an empty warnings array', (t) => {
  const project = tempProject(t, 'aidlc-dir-');
  initProject(project);
  const framing = json(project, 'orchestrate', 'next', '--intent', 'Gate fields');
  assert.equal(framing.stage.phaseGate, false);
  assert.equal(framing.gate.id, 'analysis-baseline');
  assert.equal(framing.gate.finalStage, 'context-and-risk');
  assert.equal(framing.approvalRequiredAfter, 'context-and-risk');
  assert.deepEqual(framing.warnings, []);
  assert.equal(Object.hasOwn(framing, 'notices'), false);
  assert.equal(typeof framing.context, 'object');
  writeArtifacts(project, framing);
  json(project, 'orchestrate', 'report', '--result', 'complete');
  const risk = json(project, 'orchestrate', 'next');
  assert.equal(risk.stage.id, 'context-and-risk');
  assert.equal(risk.stage.phaseGate, true);
});

test('team develop gate is release-candidate on build-and-verify only', (t) => {
  const project = tempProject(t, 'aidlc-dir-team-');
  initProject(project, 'team');
  json(project, 'orchestrate', 'next', '--intent', 'Team gates');
  const stages = [];
  for (;;) {
    const next = json(project, 'orchestrate', 'next');
    if (next.complete) break;
    stages.push(next);
    writeArtifacts(project, next);
    const extra = next.stage.phaseGate ? ['--approval', 'approved'] : [];
    json(project, 'orchestrate', 'report', '--result', 'complete', ...extra);
    if (next.stage.id === 'build-and-verify') break;
  }
  const red = stages.find((row) => row.stage.id === 'red-tests');
  const build = stages.find((row) => row.stage.id === 'build-and-verify');
  assert.equal(red.stage.phaseGate, false);
  assert.equal(build.stage.phaseGate, true);
  assert.equal(build.gate.id, 'release-candidate');
});

test('contextProvider throwing creates no files', (t) => {
  const project = tempProject(t, 'aidlc-dir-throw-');
  initProject(project);
  assert.throws(() => orchestrateNext(project, { intent: 'boom' }, { contextProvider: () => { throw new Error('boom'); } }), /boom/);
  assert.equal(existsSync(join(project, '.aidlc', 'intents')), false);
  assert.equal(existsSync(join(project, '.aidlc', 'state.json')), false);
});

test('contextProvider return value is copied onto the directive', (t) => {
  const project = tempProject(t, 'aidlc-dir-ctx-');
  initProject(project);
  let seen;
  const directive = orchestrateNext(project, { intent: 'with context' }, {
    contextProvider: (input) => {
      seen = Object.keys(input).sort();
      return { a: 1 };
    },
  });
  assert.deepEqual(directive.context, { a: 1 });
  assert.deepEqual(seen, ['artifactPathFor', 'config', 'expectedArtifacts', 'graph', 'intentDir', 'isNewIntent', 'root', 'stage', 'state']);
});

test('new intents require initialization and record scopeSource', (t) => {
  const empty = tempProject(t, 'aidlc-dir-empty-');
  const missing = run(empty, 'orchestrate', 'next', '--intent', 'x');
  assert.equal(missing.status, 1);
  assert.match(missing.stderr, /Project is not initialized/);
  assert.match(missing.stderr, /dtech-aidlc onboard --harness/);
  assert.equal(existsSync(join(empty, '.aidlc')), false);

  const project = tempProject(t, 'aidlc-dir-scope-');
  initProject(project);
  mkdirSync(join(project, 'src'));
  const nested = run(join(project, 'src'), 'orchestrate', 'next', '--intent', 'x');
  assert.equal(nested.status, 1);
  assert.match(nested.stderr, new RegExp(project));
  assert.match(nested.stderr, /--project/);
  assert.equal(existsSync(join(project, 'src', '.aidlc')), false);

  const big = json(project, 'orchestrate', 'next', '--scope', 'enterprise', '--intent', 'big');
  assert.equal(big.scope, 'enterprise');
  assert.equal(big.scopeSource, 'flag');
  assert.match(big.warnings.join('\n'), /overrides the project scope/);
});

test('legacy next without config.json warns and continues', (t) => {
  const project = tempProject(t, 'aidlc-dir-legacy-');
  mkdirSync(join(project, '.aidlc'), { recursive: true });
  writeFileSync(join(project, '.aidlc', 'state.json'), `${JSON.stringify({
    version: 1, scope: 'solo', intent: 'test', currentStage: 'problem-framing',
    stages: { 'problem-framing': { status: 'in_progress', startedAt: '2026-11-19T10:51:28.417Z' } },
    dependencies: {}, createdAt: '2026-11-19T10:51:28.409Z',
  })}\n`);
  const next = json(project, 'orchestrate', 'next');
  assert.equal(next.intentId, 'test');
  assert.equal(next.warnings.length, 1);
  assert.match(next.warnings[0], /Run dtech-aidlc onboard --harness <claude\|deepseek> --scope solo/);
});

test('blocked reports keep detail, rework, and do not record false approval', (t) => {
  const project = tempProject(t, 'aidlc-dir-block-');
  initProject(project);
  const first = json(project, 'orchestrate', 'next', '--intent', 'truthful report');
  writeArtifacts(project, first);
  json(project, 'orchestrate', 'report', '--result', 'complete');
  json(project, 'orchestrate', 'next');
  const blocked = json(project, 'orchestrate', 'report', '--result', 'blocked', '--detail', 'owner rejected: framing wrong');
  assert.equal(blocked.approval, null);
  assert.equal(blocked.gate, 'analysis-baseline');
  const mixed = run(project, 'orchestrate', 'report', '--result', 'blocked', '--approval', 'approved', '--detail', 'x');
  assert.equal(mixed.status, 1);
  const rework = json(project, 'orchestrate', 'next');
  assert.equal(rework.stage.id, 'context-and-risk');
  assert.equal(rework.stage.rework.detail, 'owner rejected: framing wrong');
  assert.equal(rework.stage.rework.attempt, 2);
  assert.equal(run(project, 'orchestrate', 'report', '--result', 'skipped').status, 1);
  assert.equal(run(project, 'orchestrate', 'report', '--stage', 'nope', '--result', 'complete').status, 1);
});
