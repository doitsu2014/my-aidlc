import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const skills = ['claude', 'deepseek'].map((harness) => readFileSync(resolve(root, `harness/${harness}/skills/aidlc/SKILL.md`), 'utf8'));

test('skills use file-based requests and current directive/report contracts', () => {
  for (const skill of skills) {
    assert.equal((skill.match(/\{\{/g) ?? []).length, 0);
    for (const forbidden of ['--intent "', 'in `<phase>/<stage>/`', 'notices', 'dtech-aidlc init', 'core/methodology/contracts.md']) assert.equal(skill.includes(forbidden), false, `unexpected ${forbidden}`);
    for (const required of ['--intent-file .aidlc/request.md', '`warnings`', '`context.warnings`', '`artifacts.expected`', '`stage.phaseGate`', '`gate.id`', '--approved-by', 'dtech-aidlc intent abandon', '`complete: true`', 'a `hint`', 'AskUserQuestion', '(Recommended)', '`Other`']) assert.ok(skill.includes(required), `missing ${required}`);
  }
});

test('Claude skill uses model preferences safely for stage delegation', () => {
  const claude = skills[0];
  for (const required of ['`stage.model`', 'Claude Code Agent tool', 'requested model', 'expected artifacts', 'continue the stage directly', 'must not run orchestration/report commands', 'must not', 'human-mode gates still require explicit human approval', 'AI-mode gates still require the conductor']) {
    assert.ok(claude.includes(required), `missing ${required}`);
  }
});

test('Claude AI mode runs one dynamic workflow per included lifecycle phase', () => {
  const claude = skills[0];
  for (const required of ['In AI mode, use Claude Code\'s dynamic `Workflow` tool', 'Analyze, Ideate, Develop, Launch, and Curate', 'one workflow for each such phase', 'do not run workflows for phases omitted by scope or depth', 'must not pre-create artifacts', 'In human mode, do not require dynamic workflows', 'do the phase work directly and disclose the fallback']) {
    assert.ok(claude.includes(required), `missing ${required}`);
  }
});
