import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, mkdirSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { run, tempProject } from './helpers.mjs';
import { codebaseMapStatus } from '../core/tools/onboard.mjs';

const methodology = new URL('../core/methodology/', import.meta.url);
const read = (path) => readFileSync(new URL(path, methodology), 'utf8');

test('brownfield methodology describes the one-time codebase map', () => {
  const template = read('templates/codebase-map.md');
  for (let section = 1; section <= 10; section++) assert.match(template, new RegExp(`## ${section}\\.`));
  assert.match(template, /relative to the project root/);
  const analyze = read('phases/analyze.md');
  for (const text of ['## Existing projects (brownfield)', 'context.codebaseMap', '_codebase-map.md', '(../templates/codebase-map.md)']) assert.ok(analyze.includes(text));
  const risk = read('agents/context-risk-analyst.md');
  assert.match(risk, /Codebase map.*\[the codebase-map template\]\(\.\.\/templates\/codebase-map\.md\)/);
  assert.match(risk, /Re-map a codebase that already has a map/);
  assert.match(risk.split('\n').find((line) => line.startsWith('- Project context')), /codebase map/);
  assert.match(read('agents/brief-analyst.md').split('\n').find((line) => line.startsWith('- Available project context')), /codebase map/);
  const readme = read('README.md');
  assert.ok(readme.indexOf('## Kickoff: new project') < readme.indexOf('## Kickoff: existing project (brownfield)'));
  assert.equal(run(process.cwd(), 'scope').status, 0);
});

test('registered source guidance and decision continuity are documented', () => {
  const analyze = read('phases/analyze.md');
  const brief = read('agents/brief-analyst.md');
  const risk = read('agents/context-risk-analyst.md');
  const ideate = read('phases/ideate.md');
  const explorer = read('agents/solution-explorer.md');
  const facilitator = read('agents/decision-facilitator.md');
  const architecture = read('templates/architecture-design.md');
  assert.match(analyze, /Do not pick one silently/);
  assert.match(brief, /- Available project context.*`requirements`.*`readme`/);
  assert.match(brief, /reported as not read/);
  assert.match(risk, /one row in the evidence log/);
  assert.match(risk, /Constraints found in decision records/);
  assert.match(ideate, /would supersede/);
  for (const agent of [explorer, facilitator]) {
    assert.match(agent, /`adr`.*`architecture`.*`api-spec`/);
    if (agent === explorer) assert.match(agent, /would supersede/);
  }
  assert.match(facilitator, /Never restart at ADR-001/);
  assert.match(facilitator, /does not restate it/);
  assert.equal((architecture.match(/does not restate it/g) ?? []).length, 1);
  assert.equal((facilitator.match(/does not restate it/g) ?? []).length, 1);
  assert.match(architecture, /next free number/);
  assert.match(architecture, /see decision-record\.md/);
  assert.doesNotMatch(architecture, /- Consequences:|Record each significant decision as an ADR/);
  assert.equal((architecture.match(/ADR-001/g) ?? []).length, 2);
});

test('codebase map status reports required state and safely detects regular files', (t) => {
  const root = tempProject(t);
  assert.equal(codebaseMapStatus(root).required, null);
  const green = tempProject(t);
  assert.equal(run(green, 'onboard', '--harness', 'claude', '--scope', 'solo').status, 0);
  assert.equal(codebaseMapStatus(green).required, false);
  mkdirSync(join(root, 'src'), { recursive: true });
  writeFileSync(join(root, 'src/a.ts'), 'export {}');
  assert.equal(run(root, 'onboard', '--harness', 'claude', '--scope', 'solo').status, 0);
  const status = codebaseMapStatus(root);
  assert.equal(status.required, true);
  assert.equal(status.exists, false);
  assert.equal(status.path, '.aidlc/knowledge/_codebase-map.md');
  assert.ok(status.template.startsWith('/'));
  assert.ok(existsSync(status.template));
  mkdirSync(join(root, '.aidlc/knowledge'), { recursive: true });
  writeFileSync(join(root, status.path), 'map');
  assert.equal(codebaseMapStatus(root).exists, true);
  writeFileSync(join(root, status.path), '');
  assert.equal(codebaseMapStatus(root).exists, false);
  rmSync(join(root, status.path));
  symlinkSync(join(root, 'src/a.ts'), join(root, status.path));
  assert.equal(codebaseMapStatus(root).exists, false);
  rmSync(join(root, status.path));
  const directive = JSON.parse(run(root, 'orchestrate', 'next', '--intent', 'x').stdout);
  assert.equal(directive.context.codebaseMap.exists, false);
  assert.equal(directive.context.codebaseMap.required, true);
  writeFileSync(join(root, status.path), 'map');
  const after = JSON.parse(run(root, 'orchestrate', 'next').stdout);
  assert.equal(after.context.codebaseMap.exists, true);
  assert.equal(JSON.parse(run(root, 'knowledge', 'list').stdout).records.some((record) => record.id === '_codebase-map'), false);
});

