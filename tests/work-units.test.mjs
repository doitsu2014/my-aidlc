import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { completeStage, initProject, json, run, tempProject, writeArtifacts } from './helpers.mjs';

test('three work units return red-tests and build-and-verify three times each', (t) => {
  const project = tempProject(t);
  initProject(project);
  json(project, 'orchestrate', 'next', '--intent', 'units');
  while (true) {
    const next = json(project, 'orchestrate', 'next');
    if (next.complete) break;
    writeArtifacts(project, next);
    if (next.stage.id === 'implementation-design') {
      const item = (Array.isArray(next.artifacts) ? next.artifacts : next.artifacts.expected).find((row) => row.output === 'work-units');
      const md = item.path.includes(':') || item.path.startsWith('/') ? item.path : join(project, item.path);
      mkdirSync(dirname(md), { recursive: true });
      writeFileSync(md.replace(/\.md$/, '.json'), JSON.stringify({
        units: [
          { id: 'U-01' },
          { id: 'U-02', dependsOn: ['U-01'] },
          { id: 'U-03', dependsOn: ['U-02'] },
        ],
      }));
    }
    const extra = next.stage.phaseGate ? ['--approval', 'approved', '--approved-by', 'Owner'] : [];
    const report = run(project, 'orchestrate', 'report', '--result', 'complete', ...extra);
    assert.equal(report.status, 0, report.stderr);
    if (next.stage.id === 'knowledge-writeback') break;
  }
  const listed = json(project, 'intent', 'list');
  assert.ok(listed[0].units);
  assert.equal(listed[0].units.length, 3);
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
