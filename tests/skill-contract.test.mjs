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
    for (const required of ['--intent-file .aidlc/request.md', '`warnings`', '`context.warnings`', 'the directive\'s `artifacts` list', '`template`', '`stage.phaseGate`', '`gateMode`', '--needs <prerequisite>', 'dtech-aidlc ids check', 'never mints one', '`stage.rework`', '`suggestedBranch`', 'exits with status 3', 'Do not pass `--force-resume`', 'Check the environment on turn one', 'Recommend `standard` for a pilot', '`gate.id`', '--approved-by', 'dtech-aidlc intent abandon', '`complete: true`', 'a `hint`', 'AskUserQuestion', '(Recommended)', '`Other`']) assert.ok(skill.includes(required), `missing ${required}`);
  }
});

test('Claude skill uses model preferences safely for stage delegation', () => {
  const claude = skills[0];
  for (const required of ['`stage.model`', 'Claude Code Agent tool', 'passing `stage.model` explicitly as the requested model', '`stage.model` takes precedence over any `model` recommendation in `stage.agentFile`', 'directive carries the effective configured preference', 'verify that the Agent tool call actually includes the directive\'s `stage.model`', 'requested model', 'expected artifacts', 'do the stage directly and disclose that the preference could not be honored', 'must not run orchestration/report commands', 'must not', 'human-mode gates still require explicit human approval', 'AI-mode gates still require the conductor']) {
    assert.ok(claude.includes(required), `missing ${required}`);
  }
});

test('Claude AI mode runs one dynamic workflow per stage directive in included lifecycle phases', () => {
  const claude = skills[0];
  for (const required of ['In AI mode, use Claude Code\'s dynamic `Workflow` tool', 'Analyze, Ideate, Develop, Launch, and Curate', 'one workflow for each stage directive', 'do not run workflows for phases omitted by scope or depth', 'must not pre-create artifacts', 'In human mode, do not require dynamic workflows', 'do the stage work directly and disclose the fallback']) {
    assert.ok(claude.includes(required), `missing ${required}`);
  }
  assert.equal(claude.includes('one workflow for each such phase'), false);
});

test('Claude delegation briefs from the directive and templates and hands writers a fixed registry', () => {
  const claude = skills[0];
  for (const required of ['Brief the delegate from the CLI and the templates, not from your summary of them', 'directive JSON in the prompt verbatim', 'every `template` path', 'never substituted for it', 'never minted']) {
    assert.ok(claude.includes(required), `missing ${required}`);
  }
});

test('AI-mode gate approval in both skills requires classified open items', () => {
  for (const skill of skills) {
    for (const required of ['## Open items', '`[written-approval]`', '`[follow-up]`', '`- None`', 'A recommendation that defers a decision is not an approval', 'When `gateMode` is `human`, present the gate to the owner']) {
      assert.ok(skill.includes(required), `missing ${required}`);
    }
  }
});
