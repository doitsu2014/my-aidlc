import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const repo = resolve(import.meta.dirname, '..');
const cli = join(repo, 'core/tools/dtech-aidlc.mjs');
const base = JSON.parse(readFileSync(join(repo, 'core/data/phase-graph.json'), 'utf8'));
function fixture() {
  const root = mkdtempSync(join(tmpdir(), 'aidlc-graph-'));
  mkdirSync(join(root, 'core/data'), { recursive: true });
  mkdirSync(join(root, 'core/methodology/phases'), { recursive: true });
  mkdirSync(join(root, 'core/methodology/agents'), { recursive: true });
  writeFileSync(join(root, 'core/data/phase-graph.json'), JSON.stringify(base));
  for (const phase of base.phases) writeFileSync(join(root, 'core', phase.file), readFileSync(join(repo, 'core', phase.file)));
  for (const agent of base.agents) writeFileSync(join(root, 'core', agent.file), readFileSync(join(repo, 'core', agent.file)));
  return root;
}
function check(root) {
  const result = spawnSync(process.execPath, [cli, 'scope', '--project', root], { encoding: 'utf8', env: { ...process.env, DTECH_AIDLC_HOME: join(root, 'core') } });
  return result;
}

test('graph references existing methodology and next directive includes absolute paths', () => {
  const root = fixture();
  try {
    spawnSync(process.execPath, [cli, 'init', '--harness', 'claude', '--scope', 'solo', '--project', root], { encoding: 'utf8' });
    const result = spawnSync(process.execPath, [cli, 'orchestrate', 'next', '--intent', 'fixture', '--project', root], { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    const stage = JSON.parse(result.stdout).stage;
    assert.equal(stage.phaseFile, join(repo, 'core', base.phases[0].file));
    assert.equal(stage.agentFile, join(repo, 'core', base.agents[0].file));
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('harness onboarding references the actual methodology projection and CLI graph', () => {
  for (const [harness, dir] of [['claude', '.claude'], ['deepseek', '.dsh']]) {
    const onboarding = readFileSync(join(repo, 'harness', harness, 'onboarding.md'), 'utf8');
    assert.ok(onboarding.includes('`.aidlc/methodology/`'), `${harness} onboarding must reference the methodology projection`);
    assert.ok(onboarding.includes('`core/data/phase-graph.json`'), `${harness} onboarding must identify the CLI graph`);
    assert.ok(!onboarding.includes(`\`${dir}/data/phase-graph.json\``), `${harness} onboarding must not point at a nonexistent projected graph`);
  }
});

test('readGraph rejects invalid fixture schema, paths, models, and duplicate identifiers', () => {
  const mutations = [
    graph => { graph.schemaVersion = '1.0.0'; },
    graph => { graph.phases[0].file = '../../outside.md'; },
    graph => { graph.phases[0].file = 'methodology/phases/missing.md'; },
    graph => { graph.agents[0].model = 'unknown'; },
    graph => { graph.modelAliases = ['opus', 'other']; },
    graph => { graph.phases[1].id = graph.phases[0].id; },
    graph => { graph.agents[1].id = graph.agents[0].id; },
    graph => { graph.stages[1].id = graph.stages[0].id; },
    graph => { graph.phases[0].agents.reverse(); },
  ];
  for (const mutate of mutations) {
    const root = fixture();
    try {
      const graphPath = join(root, 'core/data/phase-graph.json');
      const graph = JSON.parse(readFileSync(graphPath, 'utf8'));
      mutate(graph);
      writeFileSync(graphPath, JSON.stringify(graph));
      const result = check(root);
      assert.notEqual(result.status, 0, result.stdout);
      assert.match(result.stderr, /Invalid phase-graph\.json/);
    } finally { rmSync(root, { recursive: true, force: true }); }
  }
});
