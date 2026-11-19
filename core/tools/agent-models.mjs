import { existsSync, lstatSync, mkdirSync, readFileSync, renameSync, unlinkSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { configPath, stateDir } from './lib.mjs';

export const MODEL_ALIASES = Object.freeze(['opus', 'sonnet', 'haiku', 'fable']);
function pathExists(path) { try { lstatSync(path); return true; } catch (error) { if (error.code === 'ENOENT') return false; throw error; } }

export function validateModelConfig(root, graph) {
  const dir = stateDir(root), path = configPath(root);
  if (pathExists(dir) && lstatSync(dir).isSymbolicLink()) throw new Error('Refusing to access symlink .aidlc directory');
  if (!pathExists(path)) return {};
  if (lstatSync(path).isSymbolicLink()) throw new Error('Refusing to access symlink config.json');
  let config;
  try { config = JSON.parse(readFileSync(path, 'utf8')); } catch { throw new Error('Invalid project configuration JSON'); }
  if (!config || typeof config !== 'object' || Array.isArray(config)) throw new Error('Invalid project configuration: expected an object');
  if (config.agentModels !== undefined) {
    if (!config.agentModels || typeof config.agentModels !== 'object' || Array.isArray(config.agentModels)) throw new Error('Invalid agentModels: expected an object mapping agent IDs to model aliases');
    const ids = new Set(graph.agents.map(agent => agent.id));
    for (const [id, model] of Object.entries(config.agentModels)) {
      if (!ids.has(id)) throw new Error(`Unknown agent in agentModels: ${id}`);
      if (!MODEL_ALIASES.includes(model)) throw new Error(`Invalid model alias for ${id}: ${String(model)}; choose opus, sonnet, haiku, or fable`);
    }
  }
  return config;
}

export function resolveAgentModel(agent, config) {
  const override = config.agentModels?.[agent.id];
  const defaultModel = agent.model ?? agent.modelRecommendation?.model;
  return { defaultModel, model: override ?? defaultModel, source: override === undefined ? 'default' : 'project' };
}

export function writeModelConfig(root, config) {
  const dir = stateDir(root), path = configPath(root);
  if (pathExists(dir) && lstatSync(dir).isSymbolicLink()) throw new Error('Refusing to write through symlink .aidlc directory');
  mkdirSync(dir, { recursive: true });
  if (lstatSync(dir).isSymbolicLink()) throw new Error('Refusing to write through symlink .aidlc directory');
  if (existsSync(path) && lstatSync(path).isSymbolicLink()) throw new Error('Refusing to write through symlink config.json');
  const temp = join(dir, `.config.json.${process.pid}.${Date.now()}.${Math.random().toString(16).slice(2)}.tmp`);
  try {
    writeFileSync(temp, `${JSON.stringify(config, null, 2)}\n`, { flag: 'wx', mode: 0o600 });
    renameSync(temp, path);
  } finally {
    try { if (existsSync(temp)) unlinkSync(temp); } catch {}
  }
}
