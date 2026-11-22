import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { completeStage, initProject, json, run, tempProject, writeArtifacts } from './helpers.mjs';

function writeUnits(project, directive, units) {
  const item = directive.artifacts.find((row) => row.output === 'work-units');
  const md = item.path.includes(':') || item.path.startsWith('/') ? item.path : join(project, item.path);
  mkdirSync(dirname(md), { recursive: true });
  writeFileSync(md.replace(/\.md$/, '.json'), JSON.stringify({ units }));
}

test('three work units run red-tests and build-and-verify once per unit, in order, with one release-candidate gate', (t) => {
  const project = tempProject(t);
  initProject(project);
  json(project, 'orchestrate', 'next', '--intent', 'units');
  const sequence = [];
  const gates = [];
  for (let guard = 0; guard < 50; guard += 1) {
    const next = json(project, 'orchestrate', 'next');
    if (next.complete) break;
    writeArtifacts(project, next);
    if (next.stage.id === 'implementation-design') {
      writeUnits(project, next, [
        { id: 'U-01' },
        { id: 'U-02', dependsOn: ['U-01'] },
        { id: 'U-03', dependsOn: ['U-02'] },
      ]);
    }
    const label = next.currentUnit ? `${next.stage.id}@${next.currentUnit}` : next.stage.id;
    sequence.push(label);
    if (next.stage.phaseGate) gates.push(label);
    const extra = next.stage.phaseGate ? ['--approval', 'approved', '--approved-by', 'Owner'] : [];
    const report = run(project, 'orchestrate', 'report', '--result', 'complete', ...extra);
    assert.equal(report.status, 0, report.stderr);
  }
  const develop = sequence.filter((id) => /^(red-tests|build-and-verify)/.test(id));
  assert.deepEqual(develop, [
    'red-tests@U-01', 'build-and-verify@U-01',
    'red-tests@U-02', 'build-and-verify@U-02',
    'red-tests@U-03', 'build-and-verify@U-03',
  ]);
  assert.deepEqual(gates.filter((id) => id.startsWith('build-and-verify')), ['build-and-verify@U-03']);
  assert.equal(sequence.at(-1), 'knowledge-writeback');

  const listed = json(project, 'intent', 'list');
  assert.equal(listed[0].status, 'complete');
  assert.deepEqual(listed[0].units.map((unit) => [unit.id, unit.status]), [
    ['U-01', 'complete'], ['U-02', 'complete'], ['U-03', 'complete'],
  ]);
});

test('approval is refused on a build-and-verify report that is not the last work unit', (t) => {
  const project = tempProject(t);
  initProject(project);
  json(project, 'orchestrate', 'next', '--intent', 'unit gate');
  for (let guard = 0; guard < 20; guard += 1) {
    const next = json(project, 'orchestrate', 'next');
    writeArtifacts(project, next);
    if (next.stage.id === 'implementation-design') writeUnits(project, next, [{ id: 'U-01' }, { id: 'U-02' }]);
    if (next.stage.id === 'build-and-verify') {
      assert.equal(next.currentUnit, 'U-01');
      assert.equal(next.stage.phaseGate, false);
      const refused = run(project, 'orchestrate', 'report', '--result', 'complete', '--approval', 'approved', '--approved-by', 'Owner');
      assert.notEqual(refused.status, 0);
      assert.match(refused.stderr, /work unit U-01 is not the last/);
      return;
    }
    const extra = next.stage.phaseGate ? ['--approval', 'approved', '--approved-by', 'Owner'] : [];
    const report = run(project, 'orchestrate', 'report', '--result', 'complete', ...extra);
    assert.equal(report.status, 0, report.stderr);
  }
  assert.fail('build-and-verify was never selected');
});

test('an intent without units completes as one implicit unit', (t) => {
  const project = tempProject(t);
  initProject(project);
  json(project, 'orchestrate', 'next', '--intent', 'no units');
  const visited = [];
  for (;;) {
    const directive = completeStage(project);
    if (directive.complete) break;
    visited.push(directive.stage.id);
    if (visited.filter((id) => id === 'red-tests').length > 1) break;
  }
  assert.equal(visited.filter((id) => id === 'red-tests').length, 1);
});
