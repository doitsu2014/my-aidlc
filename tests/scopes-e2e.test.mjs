import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { driveToCompletion, initProject, json, repo, run, tempProject, writeArtifacts } from './helpers.mjs';

const graph = JSON.parse(readFileSync(join(repo, 'core/data/phase-graph.json'), 'utf8'));

function stagesFor(scope) {
  const phases = new Set(graph.scopes[scope]);
  const order = new Map(graph.phases.map((phase, index) => [phase.id, phase.order ?? index]));
  return graph.stages
    .filter((stage) => phases.has(stage.phase))
    .sort((a, b) => (order.get(a.phase) - order.get(b.phase)) || (a.order - b.order));
}

for (const scope of ['solo', 'team', 'enterprise']) {
  test(`scope ${scope} walks every stage and records a gate per phase`, (t) => {
    const project = tempProject(t, `aidlc-scope-${scope}-`);
    initProject(project, scope);
    json(project, 'orchestrate', 'next', '--intent', `${scope} walk`);
    const visited = driveToCompletion(project);
    const expected = json(project, 'scope', scope).stages.map((row) => row.id);
    assert.deepEqual(visited, expected);
    const audit = readFileSync(join(project, '.aidlc/audit.jsonl'), 'utf8')
      .split(/\r?\n/).filter(Boolean).map((line) => JSON.parse(line));
    assert.equal(audit.filter((row) => row.event === 'gate.decided').length, graph.scopes[scope].length);
    const intents = json(project, 'intent', 'list');
    const id = intents[0].intentId;
    const state = JSON.parse(readFileSync(join(project, '.aidlc/intents', id, 'state.json'), 'utf8'));
    for (const row of Object.values(state.stages)) assert.ok(Array.isArray(row.artifacts));
    const done = json(project, 'orchestrate', 'next');
    assert.equal(done.complete, true);
    assert.equal(done.approvals.length, graph.scopes[scope].length);
  });
}

test('orchestrate report --stage accepts the current id and refuses another', (t) => {
  const project = tempProject(t);
  initProject(project);
  const next = json(project, 'orchestrate', 'next', '--intent', 'stage flag');
  writeArtifacts(project, next);
  const ok = run(project, 'orchestrate', 'report', '--stage', next.stage.id, '--result', 'complete');
  assert.equal(ok.status, 0, ok.stderr);
  const current = json(project, 'orchestrate', 'next');
  const other = stagesFor('solo').find((stage) => stage.id !== current.stage.id);
  const refused = run(project, 'orchestrate', 'report', '--stage', other.id, '--result', 'complete', '--approval', 'approved', '--approved-by', 'X');
  assert.equal(refused.status, 1);
  assert.match(refused.stderr, /Only the current stage/);
});
