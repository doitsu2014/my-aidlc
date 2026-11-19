#!/usr/bin/env node
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { audit, configPath, loadConfig, loadState, parseArgs, readGraph, readJson, saveState, safeId, stateDir, statePath, auditPath, copyTreePreserving, orderedStages, engineRoot, repoRoot } from './lib.mjs';
import { MODEL_ALIASES, resolveAgentModel, validateModelConfig, writeModelConfig } from './agent-models.mjs';
import { addDependency, importDependencies, listDependencies, projectDependenciesPath, pullDependencies, removeDependency, validateDependencies } from './dependencies.mjs';

const VERSION = readJson(join(repoRoot(), 'package.json'))?.version || readJson(join(engineRoot(), 'version.json'))?.version || '0.1.0';
const HELP = `dtech-aidlc ${VERSION}\n\nUsage: dtech-aidlc <command> [options]\n\nCommands:\n  help, --help            Show help\n  version, --version      Show version\n  init --harness NAME --scope NAME\n                          Initialize .aidlc and project harness/methodology\n  scope [NAME]            List scopes or show stages in a scope\n  status                  Show current intent and stage progress\n  doctor                  Validate graph, state, config, and projection\n  orchestrate next [--intent TEXT] [--scope NAME]\n  orchestrate report --result complete|blocked|skipped [--approval approved]\n  config agents            List effective agent models\n  config agents set <agent-id> <opus|sonnet|haiku|fable>\n  config agents reset <agent-id>\n  repos add <name> <url> [--ref REF]\n  repos list              List registered dependency repositories\n  repos pull [name]       Clone or fast-forward repositories from aidlc.deps.json\n  repos remove <name>     Unregister only; preserve checkout\n  completion zsh          Print the zsh completion function\n  deps add <id> <dependency-id>\n  deps list                List dependency edges\n  deps pull <id>           Show direct dependencies and their status\n\nScope values: solo, team, enterprise. Harness values: claude, deepseek.\nPhase gates require explicit --approval approved when reporting a phase's final stage.\nAll workflow state and audit records are local under .aidlc/.\n`;
function out(value) { process.stdout.write(typeof value === 'string' ? value : `${JSON.stringify(value, null, 2)}\n`); }
function fail(message) { process.stderr.write(`dtech-aidlc: ${message}\n`); process.exitCode = 1; }
function validateScope(scope) { if (!['solo', 'team', 'enterprise'].includes(scope)) throw new Error('scope must be solo, team, or enterprise'); return scope; }
function validateHarness(name) { if (!['claude', 'deepseek'].includes(name)) throw new Error('harness must be claude or deepseek'); return name; }
function stageStatus(state, stage) { return state?.stages?.[stage.id]?.status ?? 'pending'; }
function intentSlug(text) {
  const base = text.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 48);
  return base || 'intent';
}
function intentDir(root, id) { return join(root, '.aidlc', 'intents', id); }
function persistWorkflow(root, state) {
  state.updatedAt = new Date().toISOString();
  saveState(root, state);
  if (!state.intentId) return state;
  const dir = intentDir(root, state.intentId);
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, 'state.json'), `${JSON.stringify(state, null, 2)}\n`, { mode: 0o600 });
  return state;
}
function ensureState(root, scope, intent) {
  const state = loadState(root);
  if (state) {
    if (intent && intent !== state.intent) throw new Error('An active intent already exists; complete or otherwise resolve it before starting another intent.');
    if (scope !== state.scope) throw new Error(`Active intent uses scope ${state.scope}; refusing to change it to ${scope}.`);
    return state;
  }
  if (!intent) throw new Error('No active intent. Run `dtech-aidlc orchestrate next --intent "..."` first.');
  const id = intentSlug(intent);
  let intentId = id, n = 2;
  while (existsSync(intentDir(root, intentId))) intentId = `${id}-${n++}`;
  const created = { version: 1, intentId, project: root, scope, intent, status: 'active', currentStage: null, stages: {}, dependencies: {}, createdAt: new Date().toISOString() };
  const dir = intentDir(root, intentId);
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, 'intent.md'), `# ${intentId}\n\nProject: ${root}\n\n${intent}\n`, { flag: 'wx', mode: 0o600 });
  persistWorkflow(root, created);
  audit(root, 'intent.created', { intent, intentId, scope });
  return created;
}
function main() {
  const { positional: args, flags } = parseArgs(process.argv.slice(2));
  const cmd = args.shift() || 'help';
  if (cmd === 'help' || flags.help || cmd === '--help') return out(HELP);
  if (cmd === 'version' || flags.version || cmd === '--version') return out(`dtech-aidlc ${VERSION}\n`);
  const root = resolve(flags.project || process.cwd());
  if (cmd === 'init') {
    const harness = validateHarness(flags.harness || 'claude');
    const scope = validateScope(flags.scope || 'solo');
    const graph = readGraph();
    const sourceRoot = join(repoRoot(), 'harness', harness);
    if (!existsSync(sourceRoot)) throw new Error(`Harness source unavailable at ${sourceRoot}; projection requires installed source files.`);
    const existingConfig = loadConfig(root);
    if (existsSync(configPath(root)) && (existingConfig.harness !== harness || existingConfig.scope !== scope)) {
      throw new Error(`Project already initialized with harness ${existingConfig.harness} and scope ${existingConfig.scope}; refusing conflicting configuration`);
    }
    const project = existsSync(configPath(root)) ? existingConfig : { harness, scope, initializedAt: new Date().toISOString() };
    const projectionRoot = harness === 'claude' ? join(root, '.claude', 'skills', 'aidlc') : join(root, '.dsh', 'skills', 'aidlc');
    mkdirSync(stateDir(root), { recursive: true });
    if (!existsSync(configPath(root))) writeFileSync(configPath(root), `${JSON.stringify(project, null, 2)}\n`, { flag: 'wx', mode: 0o600 });
    const copied = copyTreePreserving(sourceRoot, projectionRoot);
    const methodologySrc = join(engineRoot(), 'methodology');
    let methodologyTarget = null;
    if (existsSync(methodologySrc)) { methodologyTarget = join(root, '.aidlc', 'methodology'); copied.push(...copyTreePreserving(methodologySrc, methodologyTarget)); }
    audit(root, 'project.initialized', { harness, scope, graphPhases: graph.phases.length, projection: projectionRoot });
    return out({ initialized: true, harness, scope, stateDir: stateDir(root), projection: projectionRoot, methodology: methodologyTarget, filesCreated: copied.length });
  }
  if (cmd === 'config') {
    const section = args.shift();
    if (section !== 'agents') throw new Error('Use config agents');
    const graph = readGraph(), config = validateModelConfig(root, graph), action = args.shift();
    if (!action) return out(graph.agents.map(agent => ({ id: agent.id, phase: agent.phase, ...resolveAgentModel(agent, config) })));
    if (action !== 'set' && action !== 'reset') throw new Error('Use config agents set or config agents reset');
    const id = args.shift(), agent = graph.agents.find(item => item.id === id);
    if (!agent) throw new Error(`Unknown agent: ${id ?? '(missing agent ID)'}`);
    if (!existsSync(configPath(root))) throw new Error('Initialize the project before changing agent models');
    const stateConfig = config;
    if (!['claude', 'deepseek'].includes(stateConfig.harness) || !['solo', 'team', 'enterprise'].includes(stateConfig.scope) || !stateConfig.initializedAt) throw new Error('Initialize the project before changing agent models');
    if (action === 'set') {
      const model = args.shift();
      if (!MODEL_ALIASES.includes(model)) throw new Error(`Invalid model alias: ${model ?? '(missing)'}; choose opus, sonnet, haiku, or fable`);
      stateConfig.agentModels ??= {};
      stateConfig.agentModels[id] = model;
    } else {
      if (stateConfig.agentModels) delete stateConfig.agentModels[id];
    }
    validateModelConfig(root, graph);
    writeModelConfig(root, stateConfig);
    return out({ id, ...resolveAgentModel(agent, stateConfig) });
  }
  if (cmd === 'scope') {
    const graph = readGraph();
    const config = validateModelConfig(root, graph);
    if (!args.length) return out(['solo', 'team', 'enterprise'].map(scope => ({ scope, stages: orderedStages(graph, scope).map(s => s.id) })));
    const scope = validateScope(args[0]);
    return out({ scope, stages: orderedStages(graph, scope).map(({ id, phase, agent, produces }) => ({ id, phase, agent: agent ?? null, ...(agent ? resolveAgentModel(graph.agents.find(item => item.id === agent), config) : {}), produces: produces ?? [] })) });
  }
  if (cmd === 'status') {
    const state = loadState(root); if (!state) return out({ initialized: existsSync(configPath(root)), activeIntent: null });
    return out({ intent: state.intent, intentId: state.intentId ?? null, intentDir: state.intentId ? intentDir(root, state.intentId) : null, scope: state.scope, status: state.status ?? 'active', currentStage: state.currentStage, stages: state.stages });
  }
  if (cmd === 'doctor') {
    const checks = [], check = (ok, message) => checks.push({ status: ok ? 'pass' : 'fail', message });
    let graph = null;
    try { graph = readGraph(); check(graph.phases.length > 0 && graph.stages.length > 0, `Graph: ${graph.phases.length} phases, ${graph.stages.length} stages, ${graph.agents.length} agents`); } catch (e) { check(false, e.message); }
    let config = {};
    try { config = validateModelConfig(root, graph ?? readGraph()); check(!existsSync(configPath(root)) || (['claude','deepseek'].includes(config.harness) && ['solo','team','enterprise'].includes(config.scope)), 'Project configuration and agent models'); }
    catch (e) { check(false, `Project configuration and agent models: ${e.message}`); }
    try { const repos = validateDependencies(root); check(true, `Dependency repositories: ${repos.length} registered`); }
    catch (e) { check(false, `Dependency repositories: ${e.message}`); }
    const state = loadState(root);
    check(!existsSync(statePath(root)) || Boolean(state), 'State JSON is readable');
    check(!existsSync(auditPath(root)) || readFileSync(auditPath(root), 'utf8').split(/\r?\n/).filter(Boolean).every(line => { try { JSON.parse(line); return true; } catch { return false; } }), 'Audit log contains valid JSON lines');
    if (graph) for (const stage of graph.stages) if (stage.agent && !graph.agents.some(a => a.id === stage.agent)) check(false, `Stage ${stage.id} references unknown agent ${stage.agent}`);
    out({ ok: checks.every(c => c.status === 'pass'), checks }); if (checks.some(c => c.status === 'fail')) process.exitCode = 1; return;
  }
  if (cmd === 'orchestrate') {
    const action = args.shift(), graph = readGraph();
    if (action === 'next') {
      const scope = validateScope(flags.scope || loadState(root)?.scope || loadConfig(root).scope || 'solo');
      const config = validateModelConfig(root, graph);
      const state = ensureState(root, scope, flags.intent || flags.description || null);
      const stages = orderedStages(graph, state.scope);
      const next = stages.find(stage => !['complete', 'skipped'].includes(stageStatus(state, stage)));
      if (!next) { state.currentStage = null; state.status = 'complete'; persistWorkflow(root, state); return out({ complete: true, intent: state.intent, intentId: state.intentId }); }
      state.currentStage = next.id; state.status = 'active'; state.stages[next.id] ??= { status: 'in_progress', startedAt: new Date().toISOString() }; persistWorkflow(root, state);
      audit(root, 'stage.selected', { intent: state.intent, stage: next.id });
      const model = next.agent ? resolveAgentModel(graph.agents.find(agent => agent.id === next.agent), config) : {};
      const phase = graph.phases.find(item => item.id === next.phase);
      const agent = next.agent ? graph.agents.find(item => item.id === next.agent) : null;
      return out({ intent: state.intent, intentId: state.intentId, intentDir: intentDir(root, state.intentId), scope: state.scope, stage: { ...next, phaseFile: phase ? join(engineRoot(), phase.file) : null, ...(agent ? { agentFile: join(engineRoot(), agent.file), model: model.model, modelSource: model.source } : {}) }, approvalRequiredAfter: next.id });
    }
    if (action === 'report') {
      const state = loadState(root); if (!state) throw new Error('No active workflow state');
      if (!state.currentStage) throw new Error('No current stage; run orchestrate next first');
      const stage = graph.stages.find(s => s.id === (flags.stage || state.currentStage)); if (!stage) throw new Error('No current stage; run orchestrate next first');
      if (stage.id !== state.currentStage) throw new Error(`Only the current stage (${state.currentStage}) can be reported`);
      const result = flags.result; if (!['complete','blocked','skipped'].includes(result)) throw new Error('--result must be complete, blocked, or skipped');
      const stages = orderedStages(graph, state.scope), index = stages.findIndex(s => s.id === stage.id);
      if (index < 0) throw new Error(`Stage ${stage.id} is not included in the active ${state.scope} scope`);
      const isGate = index === stages.length - 1 || stages[index + 1].phase !== stage.phase;
      if (isGate && result === 'complete' && flags.approval !== 'approved') throw new Error(`Phase gate after ${stage.id} requires explicit --approval approved`);
      state.stages[stage.id] = { ...(state.stages[stage.id] || {}), status: result, reportedAt: new Date().toISOString(), detail: flags.detail || null };
      state.currentStage = null;
      persistWorkflow(root, state); audit(root, 'stage.reported', { intent: state.intent, intentId: state.intentId, stage: stage.id, result, gateApproval: isGate && flags.approval === 'approved' ? 'approved' : null });
      return out({ recorded: true, stage: stage.id, result, phaseGate: isGate, approval: isGate ? 'approved' : null });
    }
    throw new Error('Use orchestrate next or orchestrate report');
  }
  if (cmd === 'completion') {
    if (args.length !== 1 || args[0] !== 'zsh') throw new Error('Usage: dtech-aidlc completion zsh');
    const script = join(dirname(fileURLToPath(import.meta.url)), 'completion.zsh');
    return out(readFileSync(script, 'utf8'));
  }
  if (cmd === 'repos') {
    const action = args.shift();
    const allowedFlags = action === 'add' ? ['project', 'ref'] : action === 'pull' ? ['project', 'file'] : ['project'];
    if (Object.keys(flags).some(flag => !allowedFlags.includes(flag))) throw new Error('Unknown option for repos command');
    if (action === 'add') {
      if (args.length !== 2 || (flags.ref !== undefined && typeof flags.ref !== 'string')) throw new Error('Usage: repos add <name> <url> [--ref REF]');
      return out(addDependency(root, args[0], args[1], flags.ref));
    }
    if (action === 'list' && args.length === 0) return out(listDependencies(root));
    if (action === 'pull' && args.length <= 1) {
      const source = flags.file || projectDependenciesPath(root);
      if (flags.file || existsSync(source)) importDependencies(root, source);
      const results = pullDependencies(root, args[0]);
      out(results);
      if (results.some(result => !result.ok)) process.exitCode = 1;
      return;
    }
    if (action === 'remove' && args.length === 1) return out(removeDependency(root, args[0]));
    throw new Error('Use repos add <name> <url> [--ref REF], repos list, repos pull [name], or repos remove <name>');
  }
  if (cmd === 'deps') {
    const action = args.shift(), state = loadState(root); if (!state) throw new Error('Initialize a workflow before managing dependencies');
    state.dependencies ??= {};
    if (action === 'add') {
      if (!args[0] || !args[1]) throw new Error('Usage: deps add <id> <dependency-id>');
      const id = safeId(args[0], 'item id'), dependency = safeId(args[1], 'dependency id'); if (id === dependency) throw new Error('An item cannot depend on itself');
      const edges = state.dependencies[id] ??= []; if (!edges.includes(dependency)) edges.push(dependency);
      saveState(root, state); audit(root, 'dependency.added', { id, dependency }); return out({ id, dependencies: edges });
    }
    if (action === 'list') return out(Object.entries(state.dependencies).flatMap(([id, dependencies]) => dependencies.map(dependency => ({ id, dependency }))));
    if (action === 'pull') { const id = safeId(args[0], 'item id'), deps = state.dependencies[id] || []; return out({ id, dependencies: deps.map(dependency => ({ id: dependency, status: state.stages?.[dependency]?.status ?? 'unknown' })) }); }
    throw new Error('Use deps add, deps list, or deps pull');
  }
  throw new Error(`Unknown command ${cmd}; use dtech-aidlc help`);
}
try { main(); } catch (error) { fail(error.message); }
