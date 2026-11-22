import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { initProject, json, readActiveState, run, tempProject, writeArtifacts } from './helpers.mjs';
import { projectFileWarnings } from '../core/tools/context.mjs';
import { citedIds, registryProblems, workUnitProblems } from '../core/tools/ids.mjs';

function abs(project, path) {
  return path.includes(':') || path.startsWith('/') ? path : join(project, path);
}

function artifact(project, directive, output) {
  const item = directive.artifacts.find((row) => row.output === output);
  assert.ok(item, `${output} is not expected by ${directive.stage.id}`);
  return abs(project, item.path);
}

function write(path, text) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, text);
}

function readAudit(project) {
  return readFileSync(join(project, '.aidlc/audit.jsonl'), 'utf8').trim().split('\n').map((line) => JSON.parse(line));
}

// Report every stage until the directive for `target` is selected, then return it.
function driveTo(project, intent, target, beforeReport = () => {}) {
  let next = json(project, 'orchestrate', 'next', '--intent', intent);
  for (let guard = 0; guard < 30; guard += 1) {
    if (next.stage?.id === target) return next;
    writeArtifacts(project, next);
    beforeReport(next);
    const extra = next.stage.phaseGate ? ['--approval', 'approved', '--approved-by', 'Owner'] : [];
    const report = run(project, 'orchestrate', 'report', '--result', 'complete', ...extra);
    assert.equal(report.status, 0, report.stderr);
    next = json(project, 'orchestrate', 'next');
  }
  assert.fail(`${target} was never selected`);
}

const writeCriteria = (project) => (directive) => {
  if (directive.stage.id === 'problem-framing') {
    write(artifact(project, directive, 'acceptance-criteria'), '# Acceptance criteria\n\n## AC-001 Restore\n\n## AC-002 Report\n');
  }
};

test('directives carry contractsFile, template paths, the suggested branch and the gate mode', (t) => {
  const project = tempProject(t);
  initProject(project);
  const first = json(project, 'orchestrate', 'next', '--intent', 'directive fields');
  assert.ok(existsSync(first.contractsFile), first.contractsFile);
  assert.match(first.contractsFile, /methodology[\\/]contracts\.md$/);
  assert.equal(first.suggestedBranch, `feature/${first.intentId}`);
  assert.equal(first.gateMode, 'human');
  const templated = first.artifacts.filter((item) => item.template);
  assert.deepEqual(templated.map((item) => item.output).sort(), ['acceptance-criteria', 'user-stories']);
  for (const item of templated) assert.ok(existsSync(item.template), item.template);
});

test('implementation-design lists work-units.json and ids.json and refuses to complete without them', (t) => {
  const project = tempProject(t);
  initProject(project);
  const design = driveTo(project, 'work unit data', 'implementation-design');
  const outputs = design.artifacts.map((item) => item.output);
  assert.ok(outputs.includes('work-units-json'));
  assert.ok(outputs.includes('id-registry'));
  assert.equal(design.artifacts.find((item) => item.output === 'work-units-json').kind, 'data');
  assert.match(design.artifacts.find((item) => item.output === 'id-registry').path, /implementation-design\/ids\.json$/);

  writeArtifacts(project, design);
  const unitsPath = artifact(project, design, 'work-units-json');
  rmSync(unitsPath);
  let report = run(project, 'orchestrate', 'report', '--result', 'complete');
  assert.equal(report.status, 1);
  assert.match(report.stderr, /work-units-json is missing/);

  write(unitsPath, JSON.stringify({ units: [{ id: 'WU-01', dependsOn: ['WU-02'] }, { id: 'WU-02', dependsOn: ['WU-01'] }] }));
  report = run(project, 'orchestrate', 'report', '--result', 'complete');
  assert.match(report.stderr, /forms a cycle/);

  write(unitsPath, JSON.stringify({ units: [{ id: 'WU-01', dependsOn: ['WU-09'] }] }));
  assert.match(run(project, 'orchestrate', 'report', '--result', 'complete').stderr, /unknown unit WU-09/);

  write(unitsPath, JSON.stringify({ units: [{ id: 'WU-01' }, { id: 'WU-02', dependsOn: ['WU-01'] }] }));
  write(artifact(project, design, 'id-registry'), '{ not json');
  assert.match(run(project, 'orchestrate', 'report', '--result', 'complete').stderr, /ids\.json is not valid JSON/);

  write(artifact(project, design, 'id-registry'), JSON.stringify({ schemaVersion: 1, ids: { WU: [{ id: 'WU-01' }] } }));
  assert.match(run(project, 'orchestrate', 'report', '--result', 'complete').stderr, /WU list must match work-units\.json; missing WU-02/);

  write(artifact(project, design, 'id-registry'), JSON.stringify({ schemaVersion: 1, ids: { WU: [{ id: 'WU-01' }, { id: 'WU-02' }] } }));
  report = run(project, 'orchestrate', 'report', '--result', 'complete');
  assert.equal(report.status, 0, report.stderr);
  assert.equal(json(project, 'orchestrate', 'next').currentUnit, 'WU-01');
  assert.deepEqual(readActiveState(project).units.map((unit) => unit.id), ['WU-01', 'WU-02']);
});

test('stages that cite unregistered identifiers cannot be reported complete; ids check lists them', (t) => {
  const project = tempProject(t);
  initProject(project);
  const design = driveTo(project, 'id registry', 'implementation-design', writeCriteria(project));
  writeArtifacts(project, design);
  write(artifact(project, design, 'id-registry'), JSON.stringify({
    schemaVersion: 1,
    ids: { FN: [{ id: 'FN-001', traces: ['AC-001'] }], TS: [{ id: 'TS-001', traces: ['FN-001'] }] },
  }));
  write(artifact(project, design, 'functional-design'), '# Functional design\n\n### FN-001 Restore\n- Traces to: AC-001\n\n### FN-002 Undo\n- Traces to: AC-007\n');

  let report = run(project, 'orchestrate', 'report', '--result', 'complete');
  assert.equal(report.status, 1);
  assert.match(report.stderr, /2 cited identifier\(s\) are not registered/);
  assert.match(report.stderr, /FN-002 in .*functional-design\.md \(not in ids\.json\)/);
  assert.match(report.stderr, /AC-007 in .*functional-design\.md \(not in acceptance-criteria\)/);

  const check = run(project, 'ids', 'check', '--stage', 'implementation-design');
  assert.equal(check.status, 1);
  const listed = JSON.parse(check.stdout);
  assert.equal(listed.applies, true);
  assert.deepEqual(listed.stages[0].unregistered.map((row) => row.id).sort(), ['AC-007', 'FN-002']);

  write(artifact(project, design, 'functional-design'), '# Functional design\n\n### FN-001 Restore\n- Traces to: AC-001\n- Unit: U-01\n');
  report = run(project, 'orchestrate', 'report', '--result', 'complete');
  assert.equal(report.status, 0, report.stderr);
  const clean = json(project, 'ids', 'check');
  assert.equal(clean.ok, true);
  assert.deepEqual(clean.uncoveredFunctions, ['FN-001']);

  const red = json(project, 'orchestrate', 'next');
  assert.equal(red.stage.id, 'red-tests');
  writeArtifacts(project, red);
  write(artifact(project, red, 'red-evidence'), '# Red evidence\n\nTS-001 fails for FN-001 and the new CFG-004 flag.\n');
  report = run(project, 'orchestrate', 'report', '--result', 'complete');
  assert.match(report.stderr, /CFG-004 in .*red-evidence\.md/);
  assert.match(report.stderr, /rework the design/);
});

test('a stage blocked with --needs is not re-selected until the prerequisite check passes', (t) => {
  const project = tempProject(t);
  initProject(project);
  assert.equal(run(project, 'config', 'prerequisite', 'set', 'docker').status, 1);
  assert.match(run(project, 'config', 'prerequisite', 'set', 'docker', '--check', 'exit 0', '--applies-to', 'nowhere').stderr, /unknown stage or phase: nowhere/);
  assert.deepEqual(json(project, 'config', 'prerequisite', 'set', 'docker', '--check', 'exit 1', '--hint', 'Start Docker Desktop'), { name: 'docker', check: 'exit 1', hint: 'Start Docker Desktop' });
  assert.equal(json(project, 'config', 'prerequisite')[0].name, 'docker');

  const first = json(project, 'orchestrate', 'next', '--intent', 'needs docker');
  const unknown = run(project, 'orchestrate', 'report', '--result', 'blocked', '--needs', 'kafka', '--detail', 'Kafka is down');
  assert.equal(unknown.status, 1);
  assert.match(unknown.stderr, /Unknown prerequisite kafka/);
  assert.match(run(project, 'orchestrate', 'report', '--result', 'complete', '--needs', 'docker').stderr, /only accepted with --result blocked/);
  json(project, 'orchestrate', 'report', '--result', 'blocked', '--needs', 'docker', '--detail', 'Docker is not running');

  const blocked = run(project, 'orchestrate', 'next');
  assert.equal(blocked.status, 3);
  const payload = JSON.parse(blocked.stdout);
  assert.equal(payload.blocked, true);
  assert.equal(payload.stage, first.stage.id);
  assert.equal(payload.needs[0].ok, false);
  assert.match(payload.hint, /Start Docker Desktop/);
  assert.equal(readActiveState(project).currentStage, null);

  assert.match(run(project, 'orchestrate', 'next', '--force-resume').stderr, /--force-resume requires --detail/);
  json(project, 'config', 'prerequisite', 'set', 'docker', '--check', 'exit 0');
  const resumed = json(project, 'orchestrate', 'next');
  assert.equal(resumed.stage.id, first.stage.id);
  assert.deepEqual(resumed.stage.rework.needs, ['docker']);
  assert.equal(resumed.prerequisites[0].ok, true);
  const checked = readAudit(project).filter((row) => row.event === 'prerequisite.checked');
  assert.deepEqual(checked.map((row) => row.results[0].ok), [false, true]);
});

test('a prerequisite attached to a phase is a preflight that only the owner can override', (t) => {
  const project = tempProject(t);
  initProject(project);
  json(project, 'config', 'prerequisite', 'set', 'toolchain', '--check', 'exit 1', '--hint', 'Install the toolchain', '--applies-to', 'develop');
  const doctor = JSON.parse(run(project, 'doctor').stdout);
  assert.ok(doctor.checks.some((row) => row.status === 'warn' && row.message.startsWith('Prerequisite toolchain: check failed')));
  const all = run(project, 'config', 'prerequisite', 'check');
  assert.equal(all.status, 1);
  assert.equal(JSON.parse(all.stdout).ok, false);

  let next = json(project, 'orchestrate', 'next', '--intent', 'preflight');
  let blocked = null;
  for (let guard = 0; guard < 10 && !blocked; guard += 1) {
    writeArtifacts(project, next);
    const extra = next.stage.phaseGate ? ['--approval', 'approved', '--approved-by', 'Owner'] : [];
    json(project, 'orchestrate', 'report', '--result', 'complete', ...extra);
    const result = run(project, 'orchestrate', 'next');
    if (result.status === 3) blocked = JSON.parse(result.stdout);
    else next = JSON.parse(result.stdout);
  }
  assert.ok(blocked, 'develop was never preflighted');
  assert.equal(blocked.stage, 'implementation-design');
  const forced = json(project, 'orchestrate', 'next', '--force-resume', '--detail', 'Owner accepts building without the toolchain');
  assert.equal(forced.stage.id, 'implementation-design');
  assert.ok(forced.warnings.some((row) => row.includes('overridden with --force-resume: toolchain')));
  const overridden = readAudit(project).find((row) => row.event === 'prerequisite.overridden');
  assert.deepEqual(overridden.names, ['toolchain']);
  assert.equal(overridden.detail, 'Owner accepts building without the toolchain');

  json(project, 'config', 'prerequisite', 'remove', 'toolchain');
  assert.deepEqual(json(project, 'config', 'prerequisite'), []);
});

test('the unedited PROJECT.md scaffold and missing Develop sections are reported as warnings', (t) => {
  const project = tempProject(t);
  initProject(project);
  const first = json(project, 'orchestrate', 'next', '--intent', 'project file');
  assert.ok(first.context.warnings.some((row) => row.includes('PROJECT.md is still the unedited scaffold')), first.context.warnings.join('\n'));

  writeFileSync(join(project, 'PROJECT.md'), '# Project\n\n## Commands\n\n- Test: npm test\n');
  assert.deepEqual(projectFileWarnings(project, 'PROJECT.md'), []);
  const developing = projectFileWarnings(project, 'PROJECT.md', { developing: true });
  assert.equal(developing.length, 1);
  assert.match(developing[0], /'## Environment prerequisites', '## Known baseline failures'/);
  assert.doesNotMatch(developing[0], /'## Commands'/);
});

test('red-tests warns when the plan has more work units than the split threshold, and points at one baseline', (t) => {
  const project = tempProject(t);
  initProject(project);
  const config = JSON.parse(readFileSync(join(project, '.aidlc/config.json'), 'utf8'));
  writeFileSync(join(project, '.aidlc/config.json'), JSON.stringify({ ...config, unitSplitThreshold: 1 }, null, 2));
  const design = driveTo(project, 'split warning', 'implementation-design');
  writeArtifacts(project, design);
  write(artifact(project, design, 'work-units-json'), JSON.stringify({ units: [{ id: 'WU-01' }, { id: 'WU-02' }] }));
  json(project, 'orchestrate', 'report', '--result', 'complete');
  const red = json(project, 'orchestrate', 'next');
  assert.equal(red.currentUnit, 'WU-01');
  assert.ok(red.warnings.some((row) => row.includes('2 work units, more than the split threshold of 1')));
  assert.equal(red.context.baseline.path, `.aidlc/intents/${red.intentId}/develop/baseline.md`);
  assert.equal(red.context.baseline.exists, false);
});

test('ID helpers: legacy unit ids, citations and registry validation', () => {
  assert.deepEqual([...citedIds('U-01, WU-02, FN-001; AC-3 and xFN-9 or FN-')].sort(), ['AC-3', 'FN-001', 'WU-01', 'WU-02']);
  assert.deepEqual(workUnitProblems(JSON.stringify({ units: [{ id: 'U-01' }, { id: 'WU-01' }] })), ['work-units.json lists WU-01 more than once']);
  assert.deepEqual(workUnitProblems(JSON.stringify({ status: 'not-applicable', reason: 'One-line change' })), []);
  assert.match(workUnitProblems(JSON.stringify({ units: [{ id: 'task-1' }] }))[0], /needs an id like WU-01/);
  assert.deepEqual(registryProblems(JSON.stringify({ schemaVersion: 1, ids: { F: [] } })), ['ids.json has unknown prefix F; use FN, IF, TS, WU, CFG']);
  assert.deepEqual(registryProblems(JSON.stringify({ schemaVersion: 1, ids: { IF: [{ id: 'IF-001', traces: ['FN-009'] }] } })), ['ids.json IF-001 traces to unregistered FN-009']);
  assert.deepEqual(registryProblems(JSON.stringify({ schemaVersion: 1, ids: { FN: [{ id: 'FN-001' }, { id: 'FN-001' }] } })), ['ids.json registers FN-001 more than once']);
});
