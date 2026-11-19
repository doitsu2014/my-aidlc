import { existsSync, mkdirSync, readFileSync, readdirSync, copyFileSync, statSync, lstatSync, realpathSync, appendFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve, relative, isAbsolute, sep, parse as parsePath } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

export function engineRoot() {
  const here = dirname(fileURLToPath(import.meta.url));
  const home = process.env.MY_AIDLC_HOME;
  if (home) {
    const candidates = [resolve(home, 'core'), resolve(home)];
    for (const candidate of candidates) {
      if (existsSync(join(candidate, 'data', 'phase-graph.json')) || existsSync(join(candidate, 'methodology'))) return candidate;
    }
  }
  return resolve(here, '..');
}
export function repoRoot() {
  const home = process.env.MY_AIDLC_HOME;
  if (home && existsSync(resolve(home))) return resolve(home);
  return resolve(engineRoot(), '..');
}
export function projectRoot() { return resolve(process.cwd()); }
export const stateDir = (root) => join(root, '.aidlc');
export const statePath = (root) => join(stateDir(root), 'state.json');
export const auditPath = (root) => join(stateDir(root), 'audit.jsonl');
export const configPath = (root) => join(stateDir(root), 'config.json');
export function readJson(path, fallback = null) { try { return JSON.parse(readFileSync(path, 'utf8')); } catch { return fallback; } }
export function readGraph() {
  const root = engineRoot();
  const path = join(root, 'data', 'phase-graph.json');
  if (!existsSync(path)) throw new Error(`Missing methodology graph: ${path}`);
  let graph;
  try { graph = JSON.parse(readFileSync(path, 'utf8')); } catch { throw new Error(`Invalid phase-graph.json: malformed JSON (${path})`); }
  const invalid = message => { throw new Error(`Invalid phase-graph.json: ${message}`); };
  if (!graph || typeof graph !== 'object' || Array.isArray(graph) || graph.schemaVersion !== '1.1.0') invalid('schemaVersion must be 1.1.0');
  for (const key of ['phases', 'stages', 'agents']) if (!Array.isArray(graph[key])) invalid(`${key} must be an array`);
  if (!Array.isArray(graph.modelAliases) || graph.modelAliases.length === 0 || graph.modelAliases.some(x => !['opus', 'sonnet', 'haiku', 'fable'].includes(x)) || new Set(graph.modelAliases).size !== graph.modelAliases.length) invalid('modelAliases must be a unique allowlisted alias array');
  const unique = (rows, label) => {
    const ids = rows.map(row => row?.id);
    if (ids.some(id => typeof id !== 'string' || !id) || new Set(ids).size !== ids.length) invalid(`${label} IDs must be non-empty and unique`);
    return new Set(ids);
  };
  const phaseIds = unique(graph.phases, 'phase'), stageIds = unique(graph.stages, 'stage'), agentIds = unique(graph.agents, 'agent');
  const filePath = (file, label) => {
    if (typeof file !== 'string' || !file || isAbsolute(file) || file.split(/[\\/]/).includes('..')) invalid(`${label} file must be a relative path inside the engine root`);
    const resolved = resolve(root, file), rel = relative(resolve(root), resolved);
    if (!rel || rel === '..' || rel.startsWith(`..${sep}`) || isAbsolute(rel)) invalid(`${label} file must be inside the engine root`);
    if (!existsSync(resolved)) invalid(`${label} file does not exist: ${file}`);
    const actual = realpathSync(resolved), actualRel = relative(realpathSync(root), actual);
    if (!actualRel || actualRel === '..' || actualRel.startsWith(`..${sep}`) || isAbsolute(actualRel)) invalid(`${label} file resolves outside the engine root`);
    return actual;
  };
  const frontmatter = (file, label) => {
    const text = readFileSync(file, 'utf8');
    const match = text.match(/^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)/);
    if (!match) invalid(`${label} Markdown requires frontmatter: ${file}`);
    const fields = {};
    for (const line of match[1].split(/\r?\n/)) {
      const scalar = line.match(/^([\w-]+):\s*(.*?)\s*$/);
      if (scalar && !scalar[2].startsWith('- ')) fields[scalar[1]] = scalar[2].replace(/^['\"]|['\"]$/g, '');
    }
    return { fields, text: match[1] };
  };
  const phaseById = new Map(graph.phases.map(phase => [phase.id, phase]));
  const stageById = new Map(graph.stages.map(stage => [stage.id, stage]));
  const agentById = new Map(graph.agents.map(agent => [agent.id, agent]));
  for (const phase of graph.phases) {
    if (!Number.isInteger(phase.order) || typeof phase.name !== 'string' || typeof phase.gate !== 'string') invalid(`invalid phase metadata: ${phase.id}`);
    if (!Array.isArray(phase.agents) || !Array.isArray(phase.stages)) invalid(`phase ${phase.id} must declare agents and stages arrays`);
    const md = frontmatter(filePath(phase.file, `phase ${phase.id}`), `phase ${phase.id}`);
    if (md.fields.id !== phase.id) invalid(`phase ${phase.id} Markdown id mismatch`);
    const agentSection = md.text.match(/^agents:\s*\r?\n((?:  - .*\r?\n?)+)/m)?.[1] ?? '';
    const stageSection = md.text.match(/^stages:\s*\r?\n((?:  - .*\r?\n?)+)/m)?.[1] ?? '';
    const markdownAgents = [...agentSection.matchAll(/^  - ([^\r\n]+)\s*$/gm)].map(x => x[1]);
    const markdownStages = [...stageSection.matchAll(/^  - ([^\r\n]+)\s*$/gm)].map(x => x[1]);
    if (JSON.stringify(phase.agents) !== JSON.stringify(markdownAgents) || JSON.stringify(phase.stages) !== JSON.stringify(markdownStages)) invalid(`phase ${phase.id} membership/order disagrees with Markdown`);
    for (const id of phase.agents) if (!agentIds.has(id) || agentById.get(id).phase !== phase.id) invalid(`phase ${phase.id} references invalid agent ${id}`);
    for (const id of phase.stages) if (!stageIds.has(id) || stageById.get(id).phase !== phase.id) invalid(`phase ${phase.id} references invalid stage ${id}`);
    const gate = md.text.match(/^gate:\s*(\S+)\s*$/m)?.[1];
    if (gate !== phase.gate) invalid(`phase ${phase.id} gate disagrees with Markdown`);
  }
  for (const stage of graph.stages) {
    if (!phaseIds.has(stage.phase) || !agentIds.has(stage.agent)) invalid(`stage ${stage.id} references unknown phase or agent`);
    if (!Number.isInteger(stage.order) || !Array.isArray(stage.produces)) invalid(`invalid stage metadata: ${stage.id}`);
  }
  for (const agent of graph.agents) {
    if (!phaseIds.has(agent.phase) || typeof agent.model !== 'string' || !graph.modelAliases.includes(agent.model)) invalid(`invalid phase or model for agent ${agent.id}`);
    const md = frontmatter(filePath(agent.file, `agent ${agent.id}`), `agent ${agent.id}`);
    if (md.fields.id !== agent.id || md.fields.phase !== agent.phase || md.fields.model !== agent.model) invalid(`agent ${agent.id} id, phase, or model disagrees with Markdown`);
    if (!phaseById.get(agent.phase).agents.includes(agent.id)) invalid(`agent ${agent.id} missing from phase membership`);
  }
  for (const phase of graph.phases) {
    const members = graph.stages.filter(stage => stage.phase === phase.id).sort((a, b) => a.order - b.order).map(stage => stage.id);
    if (JSON.stringify(members) !== JSON.stringify(phase.stages)) invalid(`phase ${phase.id} stage membership/order is inconsistent`);
  }
  if (!graph.scopes || typeof graph.scopes !== 'object' || Array.isArray(graph.scopes)) invalid('scopes must be an object');
  for (const [scope, phases] of Object.entries(graph.scopes)) if (!Array.isArray(phases) || phases.some(id => !phaseIds.has(id))) invalid(`invalid phase references in scope ${scope}`);
  return graph;
}
export function loadConfig(root) { return readJson(configPath(root), {}); }
export function loadState(root) { return readJson(statePath(root), null); }
export function saveState(root, state) { mkdirSync(stateDir(root), { recursive: true }); writeFileSync(statePath(root), `${JSON.stringify(state, null, 2)}\n`, { flag: 'w', mode: 0o600 }); }
export function audit(root, event, details = {}) {
  mkdirSync(stateDir(root), { recursive: true });
  const row = { at: new Date().toISOString(), event, ...details };
  appendFileSync(auditPath(root), `${JSON.stringify(row)}\n`, { mode: 0o600 });
  return row;
}
export function safeId(value, label = 'name') {
  if (typeof value !== 'string' || !/^[a-zA-Z0-9][a-zA-Z0-9._-]{0,79}$/.test(value)) throw new Error(`Invalid ${label}: use letters, digits, dot, underscore, or hyphen`);
  return value;
}
export function ensureWithin(root, path) {
  const rel = relative(resolve(root), resolve(path));
  if (!rel || rel === '..' || rel.startsWith(`..${sep}`) || isAbsolute(rel)) throw new Error(`Path must be inside ${root}`);
  return resolve(path);
}
export function parseArgs(args) {
  const positional = [], flags = {};
  for (let i = 0; i < args.length; i++) {
    const a = args[i];
    if (!a.startsWith('--')) { positional.push(a); continue; }
    const [key, inline] = a.slice(2).split(/=(.*)/s, 2);
    if (inline !== undefined) flags[key] = inline;
    else if (args[i + 1] && !args[i + 1].startsWith('--')) flags[key] = args[++i];
    else flags[key] = true;
  }
  return { positional, flags };
}
export function sourceRoot() { return repoRoot(); }
export function projectHarness(root, name) {
  const known = ['claude', 'deepseek'];
  if (!known.includes(name)) throw new Error(`Unsupported harness "${name}". Choose claude or deepseek.`);
  const src = join(repoRoot(), 'harness', name);
  if (!existsSync(src)) throw new Error(`Harness source not found at ${src}; install my-aidlc with this harness projection available.`);
  return src;
}
export function copyTreePreserving(source, target) {
  const srcStats = lstatSync(source);
  if (srcStats.isSymbolicLink()) return [];
  const destExists = existsSync(target);
  if (destExists && lstatSync(target).isSymbolicLink()) {
    throw new Error(`Refusing to write through symlink: ${target}`);
  }
  if (srcStats.isFile()) {
    if (!destExists) { mkdirSync(dirname(target), { recursive: true }); copyFileSync(source, target); return [target]; }
    return [];
  }
  mkdirSync(target, { recursive: true });
  const copied = [];
  for (const entry of readdirSync(source, { withFileTypes: true })) {
    const from = join(source, entry.name), to = join(target, entry.name);
    if (entry.isSymbolicLink()) continue;
    copied.push(...copyTreePreserving(from, to));
  }
  return copied;
}
export function orderedStages(graph, scope) {
  const includedPhases = graph.scopes?.[scope];
  if (!Array.isArray(includedPhases)) return [];
  const included = new Set(includedPhases);
  const phaseOrder = new Map(graph.phases.map((p, i) => [p.id, Number.isFinite(p.order) ? p.order : i]));
  return graph.stages
    .map((stage, index) => ({ stage, index }))
    .filter(({ stage }) => included.has(stage.phase))
    .sort((a, b) => phaseOrder.get(a.stage.phase) - phaseOrder.get(b.stage.phase) || (Number.isFinite(a.stage.order) ? a.stage.order : a.index) - (Number.isFinite(b.stage.order) ? b.stage.order : b.index))
    .map(({ stage }) => stage);
}
export function withGraph(project = process.cwd()) { const root = resolve(project); return { root, graph: readGraph() }; }
export function urlToPath(path) { return pathToFileURL(path).href; }
