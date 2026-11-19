#!/usr/bin/env node
import { access, readFile, readdir } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

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
      if (graph.schemaVersion !== '1.1.0') errors.push('phase-graph schemaVersion must be 1.1.0');
      if (!Array.isArray(graph.modelAliases) || graph.modelAliases.join(',') !== 'opus,sonnet,haiku,fable') errors.push('phase-graph modelAliases must be opus, sonnet, haiku, fable');
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

if (errors.length) {
  for (const error of errors) console.error(`lint: ${error}`);
  process.exitCode = 1;
} else {
  console.log('Lint passed.');
}
