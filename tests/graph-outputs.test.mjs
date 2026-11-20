import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { repo } from './helpers.mjs';

const graph = JSON.parse(readFileSync(join(repo, 'core/data/phase-graph.json'), 'utf8'));

function section(text, heading) {
  const start = text.indexOf(heading);
  if (start < 0) return '';
  const rest = text.slice(start + heading.length);
  const next = rest.search(/\n## /);
  return (next < 0 ? rest : rest.slice(0, next)).toLowerCase().replaceAll('-', ' ');
}

test('every produced output is named in the agent Outputs section', () => {
  for (const stage of graph.stages) {
    const agent = graph.agents.find((item) => item.id === stage.agent);
    const body = readFileSync(join(repo, 'core', agent.file), 'utf8');
    const outputs = section(body, '## Outputs');
    for (const id of stage.produces) {
      const needle = id.replaceAll('-', ' ');
      assert.ok(outputs.includes(needle), `${stage.id} missing ${id} in ${agent.file}`);
    }
  }
});

test('builder-verifier names the trace table columns', () => {
  const text = readFileSync(join(repo, 'core/methodology/agents/builder-verifier.md'), 'utf8');
  assert.match(text, /trace table/i);
  for (const column of ['story identifier', 'criterion identifier', 'function identifier', 'test name', 'result']) {
    assert.match(text, new RegExp(column, 'i'));
  }
});
