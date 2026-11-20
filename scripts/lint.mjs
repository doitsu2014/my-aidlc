#!/usr/bin/env node
import { access, readFile, readdir } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const errors = [];
const exists = async (relative) => {
  try { await access(path.join(root, relative)); return true; } catch { return false; }
};

let pkg;
try {
  pkg = JSON.parse(await readFile(path.join(root, 'package.json'), 'utf8'));
} catch (error) {
  errors.push(`package.json is missing or invalid JSON: ${error.message}`);
}

if (pkg) {
  if (!pkg.engines?.node || Number.parseInt(pkg.engines.node.match(/\d+/)?.[0] ?? '0', 10) < 20) {
    errors.push('package.json must require Node.js 20 or newer');
  }
  if (pkg.type !== 'module') errors.push('package.json must declare type=module');
  if (typeof pkg.bin?.['dtech-aidlc'] !== 'string' || !(await exists(pkg.bin['dtech-aidlc']))) {
    errors.push(`CLI entry point does not exist: ${pkg.bin?.['dtech-aidlc'] ?? '(not configured)'}`);
  }
}

// The methodology README must agree with the graph it explains.
async function lintMethodologyReadme(graph) {
  const file = 'core/methodology/README.md';
  if (await exists(file)) {
    const text = (await readFile(path.join(root, file), 'utf8')).replace(/\r\n/g, '\n');
    const phases = graph.phases ?? [];
    const phaseName = (id) => phases.find((phase) => phase.id === id)?.name ?? id;
    const code = (id) => '`' + id + '`';
    const expected = {
      scopes: Object.entries(graph.scopes ?? {}).map(([scope, ids]) => `- **${scope}:** ${ids.map(phaseName).join(', ')}.`).join('\n'),
      agents: `The graph contains ${(graph.agents ?? []).length} agents: ${phases.map((phase) => `${phase.name} (${(phase.agents ?? []).map(code).join(', ')})`).join('; ')}.`,
      depths: [
        '- **full:** every stage of the selected scope (the default).',
        ...Object.entries(graph.depths ?? {}).map(([name, depth]) => `- **${name}:** ${(depth.stages ?? []).join(', ')}${depth.produces ? ', with a reduced output list' : ''}.`),
      ].join('\n'),
    };
    for (const [name, want] of Object.entries(expected)) {
      const open = `<!-- generated:${name} -->`, close = `<!-- /generated:${name} -->`;
      const from = text.indexOf(open), to = text.indexOf(close);
      const match = from >= 0 && to > from ? [null, text.slice(from + open.length, to)] : null;
      if (!match) errors.push(`${file}: missing generated section ${name}`);
      else if (match[1].trim() !== want) errors.push(`${file}: section ${name} is out of date with phase-graph.json; expected: ${want}`);
    }
    const lines = text.split('\n');
    for (const phase of phases) {
      const start = lines.findIndex((line) => line === `### ${phase.name}`);
      let end = start + 1;
      while (start >= 0 && end < lines.length && !lines[end].startsWith('#')) end++;
      if (start < 0 || !lines.slice(start + 1, end).join('\n').includes(code(phase.gate))) errors.push(`${file}: phase ${phase.name} has no section naming gate ${phase.gate}`);
    }
    let inRegion = false;
    lines.forEach((line, index) => {
      if (/^<!-- generated:/.test(line)) inRegion = true;
      else if (/^<!-- \/generated:/.test(line)) inRegion = false;
      else if (!inRegion && /\b(\d+|one|two|three|four|five|six|seven|eight|nine|ten|eleven|twelve|thirteen|fourteen|fifteen)\s+(agents|phases|stages|scopes|gates)\b/i.test(line)) errors.push(`${file}:${index + 1}: put counts of agents, phases, stages, scopes or gates inside a generated section`);
    });
  }
  // An agent that calls itself a gate must serve only the last stage of its phase.
  for (const agent of graph.agents ?? []) {
    let body = '';
    try { body = await readFile(path.join(root, 'core', agent.file), 'utf8'); } catch { continue; }
    if (!/This stage is the .* gate/.test(body)) continue;
    for (const stage of (graph.stages ?? []).filter((item) => item.agent === agent.id)) {
      const phase = (graph.phases ?? []).find((item) => item.id === stage.phase);
      if (phase?.stages?.at(-1) !== stage.id) errors.push(`${agent.file}: calls itself a gate but serves non-final stage ${stage.id}`);
    }
  }
}

// Validate methodology metadata where the phase graph is present.
const graphPath = path.join(root, 'core/data/phase-graph.json');
if (await exists('core/data/phase-graph.json')) {
  try {
    const graph = JSON.parse(await readFile(graphPath, 'utf8'));
    if (!graph || typeof graph !== 'object' || Array.isArray(graph)) errors.push('phase-graph.json must contain a JSON object');
    else {
      for (const key of ['phases', 'stages', 'agents']) {
        if (!Array.isArray(graph[key])) errors.push(`phase-graph.json must contain a ${key} array`);
      }
      const phases = new Set((graph.phases ?? []).map((phase) => phase?.id).filter(Boolean));
      const agentIds = new Set((graph.agents ?? []).map((agent) => agent?.id).filter(Boolean));
      if (graph.schemaVersion !== '1.4.0') errors.push('phase-graph schemaVersion must be 1.4.0');
      const stageIds = (graph.stages ?? []).map((stage) => stage?.id).filter(Boolean);
      for (const phase of graph.phases ?? []) if (!phase?.id || !phase?.name) errors.push('each phase must define id and name');
      for (const stage of graph.stages ?? []) {
        if (!stage?.id || !phases.has(stage.phase)) errors.push(`stage ${stage?.id ?? '(missing id)'} references unknown phase ${stage?.phase ?? '(missing phase)'}`);
        if (stage.agent && !agentIds.has(stage.agent)) errors.push(`stage ${stage.id} references unknown agent ${stage.agent}`);
      }
      for (const agent of graph.agents ?? []) {
        if (!agent?.id || !phases.has(agent.phase)) errors.push(`agent ${agent?.id ?? '(missing id)'} references unknown phase ${agent?.phase ?? '(missing phase)'}`);
      }
      if (graph.scopes && (typeof graph.scopes !== 'object' || Array.isArray(graph.scopes))) errors.push('phase-graph scopes must be an object');
      for (const [scope, included] of Object.entries(graph.scopes ?? {})) {
        if (!Array.isArray(included)) errors.push(`scope ${scope} must list phases`);
        else for (const phase of included) if (!phases.has(phase)) errors.push(`scope ${scope} references unknown phase ${phase}`);
      }
      if (new Set(stageIds).size !== stageIds.length) errors.push('stage IDs must be unique');
      await lintMethodologyReadme(graph);
    }
  } catch (error) {
    errors.push(`phase-graph.json is invalid: ${error.message}`);
  }
}

for (const directory of ['scripts']) {
  try {
    const files = await readdir(path.join(root, directory));
    if (files.length === 0) errors.push(`${directory}/ must not be empty`);
  } catch { errors.push(`required directory is missing: ${directory}/`); }
}

for (const directory of ['core/tools', 'scripts', 'tests']) {
  const dir = path.join(root, directory);
  let files = [];
  try { files = await readdir(dir); } catch { continue; }
  for (const name of files) {
    if (!name.endsWith('.mjs')) continue;
    const relative = path.join(directory, name).replaceAll('\\', '/');
    const checked = spawnSync(process.execPath, ['--check', path.join(dir, name)], { encoding: 'utf8' });
    if (checked.status !== 0) errors.push(`syntax error in ${relative}`);
  }
}

try {
  const { MODEL_ALIASES } = await import('../core/tools/lib.mjs');
  const graph = JSON.parse(await readFile(graphPath, 'utf8'));
  if (!Array.isArray(graph.modelAliases) || graph.modelAliases.join(',') !== MODEL_ALIASES.join(',')) {
    errors.push(`phase-graph modelAliases must be ${MODEL_ALIASES.join(', ')}`);
  }
} catch (error) {
  errors.push(`cannot load core/tools/lib.mjs: ${error.message}`);
}

if (errors.length) {
  for (const error of errors) console.error(`lint: ${error}`);
  process.exitCode = 1;
} else {
  console.log('Lint passed.');
}
