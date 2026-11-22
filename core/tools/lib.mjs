import { existsSync, mkdirSync, readFileSync, readdirSync, copyFileSync, lstatSync, realpathSync, appendFileSync, writeFileSync, renameSync, unlinkSync } from 'node:fs';
import { dirname, join, resolve, relative, isAbsolute, sep, win32 } from 'node:path';
import { fileURLToPath } from 'node:url';

export function engineRoot() {
  const here = dirname(fileURLToPath(import.meta.url));
  const home = process.env.DTECH_AIDLC_HOME;
  if (home) {
    const candidates = [resolve(home, 'core'), resolve(home)];
    for (const candidate of candidates) {
      if (existsSync(join(candidate, 'data', 'phase-graph.json')) || existsSync(join(candidate, 'methodology'))) return candidate;
    }
  }
  return resolve(here, '..');
}
export function repoRoot() {
  const home = process.env.DTECH_AIDLC_HOME;
  if (home && existsSync(resolve(home))) return resolve(home);
  return resolve(engineRoot(), '..');
}
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
  if (!graph || typeof graph !== 'object' || Array.isArray(graph) || graph.schemaVersion !== '1.4.0') invalid('schemaVersion must be 1.4.0 (depths and work-unit repeat are required)');
  for (const key of ['phases', 'stages', 'agents']) if (!Array.isArray(graph[key])) invalid(`${key} must be an array`);
  if (!Array.isArray(graph.modelAliases) || graph.modelAliases.length === 0 || graph.modelAliases.some(x => !MODEL_ALIASES.includes(x)) || new Set(graph.modelAliases).size !== graph.modelAliases.length) invalid('modelAliases must be a unique allowlisted alias array');
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
    if (!stage.inputs || typeof stage.inputs !== 'object' || Array.isArray(stage.inputs) || !Array.isArray(stage.inputs.artifacts) || !Array.isArray(stage.inputs.context)) {
      invalid(`stage ${stage.id} must declare inputs.artifacts and inputs.context`);
    }
    const contextParts = new Set(['request', 'project', 'sources', 'knowledge', 'profile', 'repos']);
    for (const part of stage.inputs.context) if (!contextParts.has(part)) invalid(`stage ${stage.id} has unknown context part ${part}`);
    for (const id of stage.inputs.artifacts) {
      if (id !== '*' && (typeof id !== 'string' || !/^[a-z0-9][a-z0-9-]*$/.test(id))) invalid(`stage ${stage.id} inputs.artifacts has invalid id`);
    }
    for (const output of stage.produces) {
      if (typeof output !== 'string' || !/^[a-z0-9][a-z0-9-]*$/.test(output)) invalid(`stage ${stage.id} produces an invalid output id`);
    }
  }
  const produced = graph.stages.flatMap((stage) => stage.produces.map((output) => [output, stage.id]));
  const seen = new Map();
  for (const [output, stageId] of produced) {
    if (seen.has(output)) invalid(`output ${output} is produced by more than one stage`);
    seen.set(output, stageId);
  }
  if (graph.outputs !== undefined) {
    if (!graph.outputs || typeof graph.outputs !== 'object' || Array.isArray(graph.outputs)) invalid('outputs must be an object');
    for (const [id, entry] of Object.entries(graph.outputs)) {
      if (!seen.has(id)) invalid(`outputs.${id} is not produced by any stage`);
      if (!entry || typeof entry !== 'object' || Array.isArray(entry)) invalid(`outputs.${id} must be an object`);
      const keys = Object.keys(entry);
      if (keys.some((key) => key !== 'kind' && key !== 'path')) invalid(`outputs.${id} has unknown keys`);
      if (entry.kind !== undefined && entry.kind !== 'document' && entry.kind !== 'code-manifest') invalid(`outputs.${id} kind must be document or code-manifest`);
      if (entry.path !== undefined) {
        if (typeof entry.path !== 'string' || isAbsolute(entry.path) || entry.path.includes('\\') || entry.path.split('/').includes('..') || !entry.path.endsWith('.md')) {
          invalid(`outputs.${id} path is invalid`);
        }
        if (/\{(?!intentId\}|phase\}|stage\}|output\}|unitId\})/.test(entry.path.replaceAll('{intentId}', '').replaceAll('{phase}', '').replaceAll('{stage}', '').replaceAll('{output}', '').replaceAll('{unitId}', ''))) {
          invalid(`outputs.${id} path has invalid placeholders`);
        }
      }
    }
  }
  for (const agent of graph.agents) {
    if (!phaseIds.has(agent.phase) || typeof agent.model !== 'string' || !graph.modelAliases.includes(agent.model)) invalid(`invalid phase or model for agent ${agent.id}`);
    const md = frontmatter(filePath(agent.file, `agent ${agent.id}`), `agent ${agent.id}`);
    // The graph is the only source of an agent's model; Markdown carries no model field.
    if (md.fields.id !== agent.id || md.fields.phase !== agent.phase) invalid(`agent ${agent.id} id or phase disagrees with Markdown`);
    if (md.fields.model !== undefined) invalid(`agent ${agent.id} Markdown must not declare a model; set it in phase-graph.json`);
    if (!phaseById.get(agent.phase).agents.includes(agent.id)) invalid(`agent ${agent.id} missing from phase membership`);
  }
  for (const phase of graph.phases) {
    const members = graph.stages.filter(stage => stage.phase === phase.id).sort((a, b) => a.order - b.order).map(stage => stage.id);
    if (JSON.stringify(members) !== JSON.stringify(phase.stages)) invalid(`phase ${phase.id} stage membership/order is inconsistent`);
  }
  if (!graph.scopes || typeof graph.scopes !== 'object' || Array.isArray(graph.scopes)) invalid('scopes must be an object');
  for (const [scope, phases] of Object.entries(graph.scopes)) if (!Array.isArray(phases) || phases.some(id => !phaseIds.has(id))) invalid(`invalid phase references in scope ${scope}`);
  if (graph.depths === undefined || typeof graph.depths !== 'object' || Array.isArray(graph.depths)) invalid('depths must be an object');
  if (Object.hasOwn(graph.depths, 'full')) invalid('full must not be declared');
  const rcStage = graph.phases.find((phase) => phase.gate === 'release-candidate')?.stages?.at(-1);
  const archivedStage = graph.phases.find((phase) => phase.id === 'archived')?.stages?.at(-1);
  for (const [name, depth] of Object.entries(graph.depths)) {
    if (!depth || typeof depth !== 'object' || Array.isArray(depth) || !Array.isArray(depth.stages)) invalid(`depth ${name} must declare a stages array`);
    for (const id of depth.stages) if (!stageIds.has(id)) invalid(`depth ${name} names unknown stage ${id}`);
    if (rcStage && !depth.stages.includes(rcStage)) invalid(`depth ${name} removes the release-candidate gate stage`);
    if (archivedStage && !depth.stages.includes(archivedStage)) invalid(`depth ${name} removes the Archived gate stage`);
    if (depth.produces) {
      if (typeof depth.produces !== 'object' || Array.isArray(depth.produces)) invalid(`depth ${name} produces must be an object`);
      for (const [stageId, outputs] of Object.entries(depth.produces)) {
        const stage = stageById.get(stageId);
        if (!stage) invalid(`depth ${name} produces names unknown stage ${stageId}`);
        if (!Array.isArray(outputs) || outputs.some((id) => !stage.produces.includes(id))) invalid(`depth ${name} produces subset for ${stageId} is not a subset of that stage's produces`);
      }
    }
  }
  for (const stage of graph.stages) {
    if (stage.repeat !== undefined && stage.repeat !== 'work-unit') invalid(`stage ${stage.id} has invalid repeat`);
  }
  return graph;
}
export const MODEL_ALIASES = Object.freeze(['opus', 'sonnet', 'haiku', 'fable']);
export const ORCHESTRATION_MODES = Object.freeze(['human', 'ai']);
export function resolveMode(config, flag = null) {
  const mode = flag ?? config?.mode ?? 'human';
  if (!ORCHESTRATION_MODES.includes(mode)) {
    throw new Error(`Unknown mode '${mode}'. Known modes: ${ORCHESTRATION_MODES.join(', ')}`);
  }
  return mode;
}
export function loadConfig(root) { return readJson(configPath(root), {}); }
export function listHarnesses() {
  const dir = join(repoRoot(), 'harness');
  if (!existsSync(dir)) return [];
  return readdirSync(dir, { withFileTypes: true }).filter((entry) => entry.isDirectory() && existsSync(join(dir, entry.name, 'manifest.json'))).map((entry) => entry.name).sort();
}
export function configuredHarnesses(config) {
  const names = listHarnesses();
  if (Array.isArray(config?.harnesses) && config.harnesses.length && config.harnesses.every((name) => names.includes(name))) return config.harnesses;
  return names.includes(config?.harness) ? [config.harness] : [];
}
export function loadHarnessManifest(name) {
  const invalid = (reason) => { throw new Error(`Invalid harness manifest for ${name}: ${reason}`); };
  const dir = join(repoRoot(), 'harness', name), path = join(dir, 'manifest.json');
  let manifest;
  try { manifest = JSON.parse(readFileSync(path, 'utf8')); } catch { invalid('cannot read JSON'); }
  const safe = (value) => typeof value === 'string' && value.length > 0 && !isAbsolute(value) && !win32.isAbsolute(value) && !value.split(/[\\/]/).includes('..');
  if (manifest?.schemaVersion !== 1) invalid('schemaVersion must be 1');
  if (manifest.name !== name) invalid('name must match its folder');
  if (!safe(manifest.skillDir)) invalid('skillDir must be a safe relative path');
  if (!Array.isArray(manifest.files) || !manifest.files.length) invalid('files must be a non-empty array');
  const targets = new Set();
  for (const file of manifest.files) {
    if (!safe(file?.source)) invalid('source must be a safe relative path');
    const source = join(dir, file.source);
    try { if (lstatSync(source).isSymbolicLink() || !lstatSync(source).isFile()) invalid(`source is not a regular file: ${file.source}`); }
    catch (error) { if (error.message.startsWith(`Invalid harness manifest for ${name}:`)) throw error; invalid(`source is missing: ${file.source}`); }
    if (!safe(file?.target)) invalid('target must be a safe relative path');
    if (targets.has(file.target)) invalid(`target is repeated: ${file.target}`);
    targets.add(file.target);
  }
  for (const field of ['obsoleteFiles']) if (manifest[field] !== undefined && (!Array.isArray(manifest[field]) || manifest[field].some((path) => !safe(path)))) invalid(`${field} must contain safe relative paths`);
  if (manifest.legacy !== undefined && (!Array.isArray(manifest.legacy) || manifest.legacy.some((entry) => !safe(entry?.dir) || entry.dir === manifest.skillDir || !Array.isArray(entry.files) || entry.files.some((path) => !safe(path))))) invalid('legacy paths must be safe relative paths');
  if (!Array.isArray(manifest.modelAliases) || manifest.modelAliases.some((alias) => !MODEL_ALIASES.includes(alias))) invalid('modelAliases must be an array of known model aliases');
  return manifest;
}
export function projectHarnessFiles(manifest, root) {
  assertNoSymlinkComponents(root, manifest.skillDir);
  const sourceDir = join(repoRoot(), 'harness', manifest.name), targetDir = join(root, manifest.skillDir);
  const created = [];
  for (const file of manifest.files) created.push(...copyTreePreserving(join(sourceDir, file.source), join(targetDir, file.target)));
  return created;
}
function pathExists(path) {
  try { lstatSync(path); return true; } catch (error) { if (error.code === 'ENOENT') return false; throw error; }
}
export function writeFileAtomic(path, text, { mode = 0o600 } = {}) {
  if (pathExists(path) && lstatSync(path).isSymbolicLink()) throw new Error(`Refusing to write through symlink: ${path}`);
  const temp = `${path}.${process.pid}.${Math.random().toString(16).slice(2)}.tmp`;
  try {
    writeFileSync(temp, text, { flag: 'wx', mode });
    renameSync(temp, path);
  } finally {
    try { if (existsSync(temp)) unlinkSync(temp); } catch {}
  }
}
export function writeJsonAtomic(path, value) {
  writeFileAtomic(path, `${JSON.stringify(value, null, 2)}\n`);
}
export function readJsonStrict(path) {
  if (!existsSync(path)) return undefined;
  try { return JSON.parse(readFileSync(path, 'utf8')); }
  catch { throw new Error(`${path} cannot be parsed as JSON`); }
}
export function assertNoSymlinkComponents(root, relativePath) {
  let current = resolve(root);
  for (const part of relativePath.split(/[\\/]/).filter(Boolean)) {
    current = join(current, part);
    if (!pathExists(current)) break;
    if (lstatSync(current).isSymbolicLink()) {
      throw new Error(`Refusing to write through symlink: ${relative(root, current).split(sep).join('/')}`);
    }
  }
}
export function assertSafeStateDir(root) {
  const dir = stateDir(root);
  if (pathExists(dir) && lstatSync(dir).isSymbolicLink()) throw new Error('Refusing to access symlink .aidlc directory');
  if (pathExists(dir) && !lstatSync(dir).isDirectory()) throw new Error('Refusing unsafe .aidlc path');
  assertNoSymlinkComponents(root, '.aidlc/intents');
  const intents = join(dir, 'intents');
  if (pathExists(intents) && !lstatSync(intents).isDirectory()) throw new Error('Refusing unsafe .aidlc/intents path');
}
export function audit(root, event, details = {}) {
  assertSafeStateDir(root);
  const path = auditPath(root);
  if (pathExists(path) && lstatSync(path).isSymbolicLink()) throw new Error('Refusing to write through symlink: .aidlc/audit.jsonl');
  mkdirSync(stateDir(root), { recursive: true });
  const row = { at: new Date().toISOString(), event, ...details };
  appendFileSync(path, `${JSON.stringify(row)}\n`, { mode: 0o600 });
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
export function parseArgs(args, optionTypes = {}) {
  const positional = [], flags = Object.create(null), unknown = [];
  const known = (token) => Object.keys(optionTypes).some((name) => token === `--${name}` || token.startsWith(`--${name}=`));
  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (arg === '--') { positional.push(...args.slice(i + 1)); break; }
    if (!arg.startsWith('--')) { positional.push(arg); continue; }
    const eq = arg.indexOf('=');
    const key = arg.slice(2, eq < 0 ? undefined : eq);
    const type = optionTypes[key];
    if (!type) { unknown.push(key); continue; }
    if (type === 'boolean') {
      if (eq >= 0) throw new Error(`Option --${key} does not take a value`);
      flags[key] = true;
      continue;
    }
    const value = eq >= 0 ? arg.slice(eq + 1) : args[++i];
    if (value === undefined || value === '' || value === '--' || known(value)) {
      throw new Error(`Option --${key} requires a non-empty value`);
    }
    if (type === 'string' && Object.hasOwn(flags, key)) throw new Error(`Option --${key} was given more than once`);
    if (type === 'list') (flags[key] ??= []).push(value);
    else flags[key] = value;
  }
  return { positional, flags, unknown };
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
export function copyHarnessProjection(source, target) {
  const copied = [];
  for (const entry of readdirSync(source, { withFileTypes: true })) {
    if (entry.name === 'skills' || entry.isSymbolicLink()) continue;
    copied.push(...copyTreePreserving(join(source, entry.name), join(target, entry.name)));
  }
  const skillSource = join(source, 'skills', 'aidlc');
  if (existsSync(skillSource)) copied.push(...copyTreePreserving(skillSource, target));
  return copied;
}
export function knownDepths(graph) {
  return ['full', ...Object.keys(graph.depths ?? {})];
}
export function resolveDepth(graph, depth) {
  const name = depth || 'full';
  if (name === 'full') return 'full';
  if (!graph.depths || !Object.hasOwn(graph.depths, name)) {
    throw new Error(`Unknown depth '${name}'. Known depths: ${knownDepths(graph).join(', ')}`);
  }
  return name;
}
export function orderedStages(graph, scope, depth = 'full') {
  const includedPhases = graph.scopes?.[scope];
  if (!Array.isArray(includedPhases)) return [];
  const included = new Set(includedPhases);
  const phaseOrder = new Map(graph.phases.map((p, i) => [p.id, Number.isFinite(p.order) ? p.order : i]));
  let stages = graph.stages
    .map((stage, index) => ({ stage, index }))
    .filter(({ stage }) => included.has(stage.phase))
    .sort((a, b) => phaseOrder.get(a.stage.phase) - phaseOrder.get(b.stage.phase) || (Number.isFinite(a.stage.order) ? a.stage.order : a.index) - (Number.isFinite(b.stage.order) ? b.stage.order : b.index))
    .map(({ stage }) => stage);
  if (depth && depth !== 'full') {
    const allowed = new Set(graph.depths?.[depth]?.stages ?? []);
    stages = stages.filter((stage) => allowed.has(stage.id));
  }
  return stages;
}
export function stageProduces(graph, stage, depth = 'full') {
  const subset = depth && depth !== 'full' ? graph.depths?.[depth]?.produces?.[stage.id] : undefined;
  return subset ?? stage.produces ?? [];
}
