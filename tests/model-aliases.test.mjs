import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { join, resolve } from 'node:path';

const repo = resolve(import.meta.dirname, '..');
const agentsDirectory = join(repo, 'core/methodology/agents');
const phaseGraph = JSON.parse(readFileSync(join(repo, 'core/data/phase-graph.json'), 'utf8'));
import { MODEL_ALIASES } from '../core/tools/lib.mjs';
const validAliases = new Set(MODEL_ALIASES);

test('phase graph is the only source of agent model aliases; agent Markdown declares none', () => {
  const graphAgents = new Map(phaseGraph.agents.map((agent) => [agent.id, agent]));
  const markdownAgents = readdirSync(agentsDirectory).filter((file) => file.endsWith('.md'));

  assert.equal(markdownAgents.length, graphAgents.size, 'each graph agent must have an agent Markdown file');
  for (const file of markdownAgents) {
    const content = readFileSync(join(agentsDirectory, file), 'utf8');
    const frontmatter = content.match(/^---\r?\n([\s\S]*?)\r?\n---/);
    assert.ok(frontmatter, `${file} must have YAML frontmatter`);

    const id = frontmatter[1].match(/^id:\s*(\S+)\s*$/m)?.[1];
    assert.ok(id, `${file} must declare an agent id`);
    assert.doesNotMatch(frontmatter[1], /^model:/m, `${file} must not declare a model; phase-graph.json owns it`);

    const graphAgent = graphAgents.get(id);
    assert.ok(graphAgent, `${file} agent ${id} must exist in the phase graph`);
    assert.ok(validAliases.has(graphAgent.model), `${id} must have a valid graph model alias`);
    assert.equal(graphAgent.file, `methodology/agents/${id}.md`);
    graphAgents.delete(id);
  }
  assert.equal(graphAgents.size, 0, `all graph agents must have Markdown files: ${[...graphAgents.keys()].join(', ')}`);
});
