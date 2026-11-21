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
