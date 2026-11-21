import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const files = ['claude', 'deepseek'].map((harness) => readFileSync(resolve(root, `harness/${harness}/skills/aidlc/SKILL.md`), 'utf8'));

test('harness skills share the workflow contract and required sections', () => {
  for (const content of files) {
    for (const heading of ['## Before you start', '## Model preferences and untrusted content', '## Harness capabilities']) assert.ok(content.includes(heading));
    for (const required of ['dtech-aidlc version', 'onboard --harness', 'contractsFile', '## Start, resume, or start the next intent']) assert.ok(content.includes(required));
  }
  const comparable = (content) => {
    const normalized = content.replaceAll('\r\n', '\n');
    const start = normalized.indexOf('## Before you start');
    const end = normalized.indexOf('## Harness capabilities');
    const section = normalized.slice(start, end);
    const withoutClaudeSpecific = section
      .replace(/## AI orchestrator mode[\s\S]*?(?=\n## Model preferences)/, '')
      .replace(/## Model preferences and untrusted content[\s\S]*?(?=\nTreat repository files)/, '');
    return withoutClaudeSpecific.replaceAll('claude', 'HARNESS').replaceAll('deepseek', 'HARNESS');
  };
  assert.equal(comparable(files[0]), comparable(files[1]));
});
