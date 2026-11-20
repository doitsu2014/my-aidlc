#!/usr/bin/env node
import { existsSync, lstatSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { audit, configPath, loadConfig, parseArgs, readGraph, readJson, safeId, stateDir, statePath, auditPath, copyTreePreserving, copyHarnessProjection, orderedStages, engineRoot, repoRoot, assertSafeStateDir, ensureWithin } from './lib.mjs';
import { GLOBAL_FLAGS, COMMAND_FLAGS, COMMAND_MAX_ARGS, commandKey, optionTypes } from './cli-spec.mjs';
import {
  checkWorkflowState, deriveStatus, intentDir, listIntents, loadIntent, loadWorkflow, orchestrateNext, orchestrateReport,
  persistWorkflow, quarantineCorruptPointer, readPointer, saveIntent, savePointer, unfinishedIds,
} from './workflow.mjs';
import { MODEL_ALIASES, resolveAgentModel, validateModelConfig, writeModelConfig } from './agent-models.mjs';
import { addDependency, importDependencies, listDependencies, projectDependenciesPath, pullDependencies, removeDependency, validateDependencies } from './dependencies.mjs';
import { detectDocumentation, onboardCommands, planRegistration, scanProject, sourceStatus, writeProjectProfile, writeSourcesSnapshot } from './onboard.mjs';
import {
  addSource, checkRecords, defaultKnowledgeManifest, ensureIgnoreFile, ensureKnowledgeDir, initKnowledge, inspectSources, knowledgePreflight,
  loadKnowledgeManifest, readRecords, removeSource, writeIndex, writeKnowledgeManifest,
} from './knowledge.mjs';

const VERSION = readJson(join(repoRoot(), 'package.json'))?.version || readJson(join(engineRoot(), 'version.json'))?.version || '0.1.0';
const HELP = [
  `dtech-aidlc ${VERSION}`,
  '',
  'Usage: dtech-aidlc <command> [options]',
  '',
  'Commands:',
  '  help, --help            Show help',
  '  version, --version      Show version',
  '  onboard --harness NAME --scope NAME',
  '                          Initialize and detect the project in one command',
  '  onboard --rescan [--docs PATH]...',
  '                          Re-scan an initialized project and report documentation changes',
  '  init [same options]     Alias of onboard',
  '  scope [NAME]            List scopes or show stages in a scope',
  '  status [--intent ID]    Show current intent and stage progress',
  '  doctor                  Validate graph, state, config, and projection',
  '  orchestrate next [--intent TEXT] [--description TEXT] [--scope NAME]',
  '  orchestrate report --result complete|blocked|skipped [--approval approved] [--detail TEXT] [--stage ID]',
  '  config agents            List effective agent models',
  '  config agents set <agent-id> <opus|sonnet|haiku|fable>',
  '  config agents reset <agent-id>',
  '  repos add <name> <url> [--ref REF]',
  '  repos list              List registered dependency repositories',
  '  repos pull [name] [--file FILE]       Clone or fast-forward repositories from aidlc.deps.json',
  '  repos remove <name>     Unregister only; preserve checkout',
  '  completion zsh          Print the zsh completion function',
  '  deps add <id> <dependency-id>',
  '  deps list                List dependency edges',
  '  deps pull <id>           Show direct dependencies and their status',
  '  intent list             List every intent with its status; paused means: not the active intent of this checkout',
  '  intent pause [--detail TEXT]',
  '  intent resume <id>',
  '  intent abandon [<id>] --detail TEXT',
  '  knowledge init          Create PROJECT.md and .aidlc/knowledge/ if missing',
  '  knowledge sources       List, add, or remove knowledge sources',
  '  knowledge list          List knowledge records',
  '  knowledge show <id>     Show one knowledge record',
  '  knowledge check         Validate knowledge records',
  '  knowledge index         Write .aidlc/knowledge/_index.md',
  'Global option: --project DIR  Use DIR as the project root instead of the current directory.',
  '',
  'Scope values: solo, team, enterprise. Harness values: claude, deepseek.',
  'Phase gates require explicit --approval approved when reporting a phase\'s final stage.',
  'All workflow state and audit records are local under .aidlc/.',
].join('\n') + '\n';
function out(value) { process.stdout.write(typeof value === 'string' ? value : `${JSON.stringify(value, null, 2)}\n`); }
function fail(message) { process.stderr.write(`dtech-aidlc: ${message}\n`); process.exitCode = 1; }
function validateScope(scope) { if (!['solo', 'team', 'enterprise'].includes(scope)) throw new Error('scope must be solo, team, or enterprise'); return scope; }
function validateHarness(name) { if (!['claude', 'deepseek'].includes(name)) throw new Error('harness must be claude or deepseek'); return name; }
function knowledgeState(root) {
  try {
    const loaded = loadKnowledgeManifest(root);
    return loaded.exists ? { status: 'present', sources: loaded.manifest.sources } : { status: 'absent' };
  } catch (error) {
    return { status: 'invalid', error: error.message };
  }
}
function stageStatus(state, stage) { return state?.stages?.[stage.id]?.status ?? 'pending'; }
function main() {
  const parsed = parseArgs(process.argv.slice(2), optionTypes());
  const args = parsed.positional, flags = parsed.flags;
  if (flags.version || args[0] === 'version') return out(`dtech-aidlc ${VERSION}\n`);
  if (flags.help || args[0] === 'help' || args.length === 0) return out(HELP);
  const key = commandKey(args);
  const allowed = COMMAND_FLAGS[key];
  const invalid = parsed.unknown[0]
    ?? Object.keys(flags).find((name) => !Object.hasOwn(GLOBAL_FLAGS, name) && !Object.hasOwn(allowed ?? {}, name));
  if (invalid) {
    if (!allowed) throw new Error(`Unknown option --${invalid}. Run 'dtech-aidlc help'.`);
    const options = [...Object.keys(allowed), 'project'].map((name) => `--${name}`).join(', ');
    throw new Error(`Unknown option --${invalid} for '${key}'. Allowed options: ${options}.`);
  }
  const words = key.split(' ').length;
  const maxArgs = COMMAND_MAX_ARGS[key];
  if (maxArgs !== undefined && args.length - words > maxArgs) {
    throw new Error(`Unexpected argument '${args[words + maxArgs]}' for '${key}'.`);
  }
  const cmd = args.shift();
  const root = resolve(flags.project || process.cwd());
  if (cmd !== 'completion') assertSafeStateDir(root);
  if (cmd === 'onboard' || cmd === 'init') {
    const docs = [].concat(flags.docs ?? []);
    const detect = !flags['no-detect'];
    if (flags.rescan) {
      if (flags.harness || flags.scope || flags['no-detect']) {
        throw new Error('--rescan re-scans an initialized project and does not take --harness, --scope or --no-detect');
      }
      if (!existsSync(configPath(root))) {
        throw new Error('Project is not initialized; run dtech-aidlc onboard --harness <claude|deepseek> first');
      }
      const loaded = knowledgePreflight(root);
      const commands = onboardCommands(root);
      if (!loaded.exists && docs.length) {
        const quoted = docs.map((path) => (/[^A-Za-z0-9._/-]/.test(path) ? `"${path}"` : path)).map((path) => `--docs ${path}`).join(' ');
        throw new Error(`aidlc.knowledge.json does not exist; run \`${commands.create} ${quoted}\` instead`);
      }
      for (const item of docs) {
        const abs = resolve(root, item);
        if (!existsSync(abs)) throw new Error(`--docs path does not exist: ${item}`);
        try { ensureWithin(root, abs); }
        catch { throw new Error(`--docs path must be inside the project: ${item}`); }
      }
      const knowledgeState = loaded.exists ? { status: 'present', sources: loaded.manifest.sources } : { status: 'absent' };
      const before = sourceStatus(root, knowledgeState);
      const documentation = detectDocumentation(root, { explicit: docs, detect: true });
      let plan = { manifest: loaded.exists ? loaded.manifest : defaultKnowledgeManifest(root), status: 'absent', added: [], wouldAdd: [], alreadyRegistered: [] };
      if (loaded.exists) {
        plan = planRegistration(loaded.manifest, defaultKnowledgeManifest(root), documentation.candidates);
        if (plan.status === 'updated') {
          writeKnowledgeManifest(root, plan.manifest);
          const addedPaths = plan.manifest.sources.filter((source) => plan.added.includes(source.id)).map((source) => source.path);
          audit(root, 'knowledge.sources.registered', { manifestStatus: 'updated', added: addedPaths });
        }
      } else {
        plan.wouldAdd = documentation.candidates.map((row) => ({ path: row.path, kind: row.kind, command: commands.create }));
        if (plan.wouldAdd.length) plan.registerAllCommand = commands.create;
        plan.review = 'No documentation was detected';
      }
      const scan = scanProject(root);
      const detected = writeProjectProfile(root, scan, documentation);
      writeSourcesSnapshot(root, plan.manifest.sources);
      const afterKnowledge = loaded.exists && plan.status === 'updated'
        ? { status: 'present', sources: plan.manifest.sources }
        : knowledgeState;
      const after = sourceStatus(root, afterKnowledge);
      audit(root, 'project.rescanned', {
        kind: detected.profile.kind,
        changed: before.changed.length + before.more.changed,
        added: before.added.length + before.more.added,
        removed: before.removed.length + before.more.removed,
        missing: before.missing.length,
      });
      const knowledge = loaded.exists
        ? { manifest: 'aidlc.knowledge.json', manifestStatus: plan.status, registered: plan.added, added: plan.added, wouldAdd: plan.wouldAdd, review: plan.review, warnings: after.warnings }
        : { manifest: 'aidlc.knowledge.json', manifestStatus: 'absent', registered: [], added: [], wouldAdd: plan.wouldAdd, registerAllCommand: plan.registerAllCommand, review: plan.review, warnings: after.warnings };
      return out({
        rescanned: true,
        kind: detected.profile.kind,
        languages: detected.profile.languages,
        architecture: detected.profile.architecture,
        documentation: detected.profile.documentation,
        knowledge,
        changesSinceLastScan: {
          scannedAt: before.scannedAt,
          changed: before.changed,
          added: before.added,
          removed: before.removed,
          more: before.more,
        },
        missing: before.missing,
        ...(before.legacyBrdCopies ? { legacyBrdCopies: 1 } : {}),
      });
    }
    for (const item of docs) {
      const abs = resolve(root, item);
      if (!existsSync(abs)) throw new Error(`--docs path does not exist: ${item}`);
      try { ensureWithin(root, abs); }
      catch { throw new Error(`--docs path must be inside the project: ${item}`); }
    }
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
    const projectionRoot = harness === 'claude' ? join(root, '.claude', 'skills', 'dtech-aidlc') : join(root, '.dsh', 'skills', 'aidlc');
    mkdirSync(stateDir(root), { recursive: true });
    if (!existsSync(configPath(root))) writeFileSync(configPath(root), `${JSON.stringify(project, null, 2)}\n`, { flag: 'wx', mode: 0o600 });
    const copied = copyHarnessProjection(sourceRoot, projectionRoot);
    const methodologySrc = join(engineRoot(), 'methodology');
    let methodologyTarget = null;
    if (existsSync(methodologySrc)) { methodologyTarget = join(root, '.aidlc', 'methodology'); copied.push(...copyTreePreserving(methodologySrc, methodologyTarget)); }
    audit(root, 'project.initialized', { harness, scope, graphPhases: graph.phases.length, projection: relative(root, projectionRoot).split(sep).join('/') });
    const knowledge = initKnowledge(root, { createManifest: false });
    const scan = scanProject(root);
    const documentation = detectDocumentation(root, { explicit: docs, detect });
    const existing = loadKnowledgeManifest(root);
    const plan = planRegistration(existing.exists ? existing.manifest : null, defaultKnowledgeManifest(root), documentation.candidates);
    if (plan.status === 'created' || plan.status === 'updated') {
      writeKnowledgeManifest(root, plan.manifest);
      audit(root, 'knowledge.sources.registered', { added: plan.added, status: plan.status });
    }
    knowledge.manifestStatus = plan.status;
    knowledge.added = plan.added;
    knowledge.wouldAdd = plan.wouldAdd;
    knowledge.alreadyRegistered = plan.alreadyRegistered;
    knowledge.registered = plan.added;
    knowledge.review = plan.status === 'absent' ? 'No documentation was detected' : undefined;
    const detected = writeProjectProfile(root, scan, documentation);
    writeSourcesSnapshot(root, plan.manifest.sources);
    audit(root, 'project.onboarded', { kind: detected.profile.kind, languages: detected.profile.languages.map(row => row.name) });
    return out({
      ...{ initialized: true, harness, scope, stateDir: stateDir(root), projection: projectionRoot, methodology: methodologyTarget, filesCreated: copied.length, knowledge },
      kind: detected.profile.kind, shared: detected.shared, languages: detected.profile.languages,
      architecture: detected.profile.architecture, documentation: detected.profile.documentation,
      legacyBrdCopies: detected.profile.legacy?.brdCopies?.files,
    });
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
    const graph = readGraph();
    if (flags.intent) {
      try {
        const state = loadIntent(root, graph, flags.intent);
        return out({
          intent: state.intent, intentId: state.intentId, intentDir: intentDir(root, state.intentId),
          scope: state.scope, status: state.status, currentStage: state.currentStage, stages: state.stages,
        });
      } catch {
        throw new Error(`Unknown intent '${flags.intent}'. Run 'dtech-aidlc intent list'.`);
      }
    }
    const workflow = loadWorkflow(root, graph);
    const state = workflow.state;
    if (!state) {
      const paused = unfinishedIds(root, graph);
      return out({ initialized: existsSync(configPath(root)), activeIntent: null, ...(paused.length ? { pausedIntents: paused } : {}) });
    }
    return out({
      intent: state.intent, intentId: state.intentId ?? null, intentDir: state.intentId ? intentDir(root, state.intentId) : null,
      scope: state.scope, status: state.status ?? 'active', currentStage: state.currentStage, stages: state.stages,
      ...(workflow.legacy ? { migration: 'pending' } : {}),
    });
  }
  if (cmd === 'doctor') {
    // graph; configuration and agent models; projection (HRN-04); dependency repositories; knowledge (KNW-06); knowledge sources (BRN-05); workflow state (ENG-10); audit log; unused methodology copy (HRN-05)
    let graph = null, config = {};
    try { graph = readGraph(); } catch {}
    try { config = validateModelConfig(root, graph ?? { agents: [] }); } catch {}
    const ctx = { root, graph, config };
    const DOCTOR_CHECKS = [
      (ctx) => {
        try {
          const loaded = ctx.graph ?? readGraph();
          const rows = [{ status: 'pass', message: `Graph: ${loaded.phases.length} phases, ${loaded.stages.length} stages, ${loaded.agents.length} agents` }];
          for (const stage of loaded.stages) if (stage.agent && !loaded.agents.some((agent) => agent.id === stage.agent)) {
            rows.push({ status: 'fail', message: `Stage ${stage.id} references unknown agent ${stage.agent}` });
          }
          return rows;
        } catch (error) { return [{ status: 'fail', message: error.message }]; }
      },
      (ctx) => {
        try {
          const loaded = validateModelConfig(ctx.root, ctx.graph ?? readGraph());
          const ok = !existsSync(configPath(ctx.root)) || (['claude', 'deepseek'].includes(loaded.harness) && ['solo', 'team', 'enterprise'].includes(loaded.scope));
          return [{ status: ok ? 'pass' : 'fail', message: 'Project configuration and agent models' }];
        } catch (error) { return [{ status: 'fail', message: `Project configuration and agent models: ${error.message}` }]; }
      },
      (ctx) => {
        try { return [{ status: 'pass', message: `Dependency repositories: ${validateDependencies(ctx.root).length} registered` }]; }
        catch (error) { return [{ status: 'fail', message: `Dependency repositories: ${error.message}` }]; }
      },
      (ctx) => {
        const state = knowledgeState(ctx.root);
        if (state.status === 'invalid') return [{ status: 'fail', message: `Knowledge: ${state.error}` }];
        return [];
      },
      (ctx) => {
        const state = knowledgeState(ctx.root);
        if (state.status === 'invalid') return [];
        const status = sourceStatus(ctx.root, state);
        const commands = onboardCommands(ctx.root);
        if (status.manifest === 'absent') {
          const missing = status.warnings.some((row) => row.startsWith('No aidlc.knowledge.json was found'));
          return [{ status: 'pass', message: missing
            ? `Knowledge sources: no aidlc.knowledge.json; run ${commands.create}`
            : 'Knowledge sources: no aidlc.knowledge.json; no documentation detected at the last scan' }];
        }
        const missing = status.missing.length;
        const changed = status.changed.length + status.added.length + status.removed.length + status.more.changed + status.more.added + status.more.removed;
        const rowStatus = missing > 0 ? 'warn' : 'pass';
        if (!status.scanned) {
          return [{ status: rowStatus, message: `Knowledge sources: ${status.sources} registered, ${missing} missing; not scanned on this machine` }];
        }
        return [{ status: rowStatus, message: `Knowledge sources: ${status.sources} registered, ${missing} missing, ${changed} changed since last scan` }];
      },
      (ctx) => checkWorkflowState(ctx.root, ctx.graph),
      (ctx) => [{ status: !existsSync(auditPath(ctx.root)) || readFileSync(auditPath(ctx.root), 'utf8').split(/\r?\n/).filter(Boolean).every((line) => { try { JSON.parse(line); return true; } catch { return false; } }) ? 'pass' : 'fail', message: 'Audit log contains valid JSON lines' }],
    ];
    const checks = [];
    for (const fn of DOCTOR_CHECKS) {
      try { checks.push(...fn(ctx)); }
      catch (error) { checks.push({ status: 'fail', message: error.message }); }
    }
    const ok = !checks.some((row) => row.status === 'fail');
    const warnings = checks.filter((row) => row.status === 'warn').length;
    out({ ok, warnings, checks });
    if (!ok) process.exitCode = 1;
    return;
  }
  if (cmd === 'orchestrate') {
    const action = args.shift(), graph = readGraph();
    if (action === 'next') {
      knowledgePreflight(root);
      const sources = [flags.intent, flags.description, flags['intent-file']].filter((value) => value !== undefined);
      if (sources.length > 1) throw new Error('Give the intent with only one of --intent, --description or --intent-file');
      let intentText = flags.intent || flags.description || null;
      if (flags['intent-file']) {
        const path = resolve(process.cwd(), flags['intent-file']);
        if (!existsSync(path)) throw new Error(`Intent file not found: ${path}`);
        const stat = lstatSync(path);
        if (stat.isSymbolicLink()) throw new Error(`Refusing to read symlink intent file: ${path}`);
        if (!stat.isFile()) throw new Error(`Intent file is not a regular file: ${path}`);
        const text = readFileSync(path, 'utf8');
        if (Buffer.byteLength(text) > 100_000) throw new Error('Intent file is too large (max 100000 bytes)');
        intentText = text;
      }
      const directive = orchestrateNext(root, { intent: intentText, scope: flags.scope || null });
      if (directive.stage) {
        ensureKnowledgeDir(root);
        ensureIgnoreFile(root);
      }
      return out(directive);
    }
    if (action === 'report') {
      const result = orchestrateReport(root, {
        stage: flags.stage, result: flags.result, approval: flags.approval, detail: flags.detail,
        approvedBy: flags['approved-by'], approvalNote: flags['approval-note'],
      });
      if (result.stage === 'knowledge-writeback' && result.result === 'complete') {
        writeIndex(root);
        result.knowledgeRecord = `.aidlc/knowledge/${result.intentId}.md`;
      }
      return out(result);
    }
    throw new Error('Use orchestrate next or orchestrate report');
  }
  if (cmd === 'intent') {
    const action = args.shift(), graph = readGraph();
    if (action === 'list') return out(listIntents(root, graph));
    if (action === 'pause') {
      const loaded = loadWorkflow(root, graph, { migrate: true, missingActive: 'null' });
      if (!loaded.state && !loaded.missing) throw new Error('No unfinished active intent to pause');
      const id = loaded.state?.intentId ?? loaded.activeIntentId;
      savePointer(root, null);
      audit(root, 'intent.paused', { intentId: id, detail: flags.detail ?? null });
      return out({ paused: true, intentId: id, resumeWith: `dtech-aidlc intent resume ${id}` });
    }
    if (action === 'resume') {
      const id = args[0];
      if (!id) throw new Error('Usage: intent resume <id>');
      let pointer = readPointer(root);
      if (pointer.kind === 'corrupt') {
        quarantineCorruptPointer(root);
        pointer = { kind: 'absent' };
      } else {
        loadWorkflow(root, graph, { migrate: true, missingActive: 'null' });
      }
      let target;
      try { target = loadIntent(root, graph, id); }
      catch { throw new Error(`Unknown intent '${id}'. Run 'dtech-aidlc intent list'.`); }
      const status = deriveStatus(target, graph);
      if (status === 'complete') throw new Error(`Intent '${id}' is complete and cannot be resumed`);
      if (status === 'abandoned') throw new Error(`Intent '${id}' was abandoned and cannot be resumed`);
      const loaded = loadWorkflow(root, graph, { missingActive: 'null' });
      if (loaded.state && loaded.state.intentId !== id && deriveStatus(loaded.state, graph) === 'active') {
        throw new Error(`Intent '${loaded.state.intentId}' is active; pause or finish it first`);
      }
      savePointer(root, id);
      audit(root, 'intent.resumed', { intentId: id, detail: null });
      return out({ resumed: true, intentId: id, currentStage: target.currentStage });
    }
    if (action === 'abandon') {
      if (!flags.detail) throw new Error('intent abandon requires --detail "<reason>"');
      loadWorkflow(root, graph, { migrate: true, missingActive: 'null' });
      const id = args[0];
      const loaded = loadWorkflow(root, graph, { missingActive: 'null' });
      const targetId = id || loaded.state?.intentId;
      if (!targetId) throw new Error('No active intent; name the intent to abandon');
      let target;
      try { target = loadIntent(root, graph, targetId); }
      catch { throw new Error(`Unknown intent '${targetId}'. Run 'dtech-aidlc intent list'.`); }
      const status = deriveStatus(target, graph);
      if (status === 'complete') throw new Error(`Intent '${targetId}' is already complete`);
      if (status === 'abandoned') throw new Error(`Intent '${targetId}' is already abandoned`);
      target.closed = { kind: 'abandoned', at: new Date().toISOString(), detail: flags.detail };
      target.currentStage = null;
      saveIntent(root, target, graph);
      if (loaded.activeIntentId === targetId) savePointer(root, null);
      audit(root, 'intent.abandoned', { intentId: targetId, detail: flags.detail });
      return out({ abandoned: true, intentId: targetId });
    }
    throw new Error('Use intent list, intent pause, intent resume, or intent abandon');
  }
  if (cmd === 'knowledge') {
    const action = args.shift();
    if (action === 'init') return out(initKnowledge(root, { createManifest: false }));
    if (action === 'sources') {
      const sub = args[0];
      if (!sub || sub === 'list') {
        const loaded = loadKnowledgeManifest(root);
        return out(inspectSources(root, loaded.manifest));
      }
      if (sub === 'add') {
        if (!args[1] || !args[2]) throw new Error('Usage: knowledge sources add <id> <path> --kind KIND');
        return out(addSource(root, { id: args[1], path: args[2], kind: flags.kind || 'docs', description: flags.description }));
      }
      if (sub === 'remove') {
        if (!args[1]) throw new Error('Usage: knowledge sources remove <id>');
        return out(removeSource(root, args[1]));
      }
      throw new Error('Use knowledge sources, knowledge sources add, or knowledge sources remove');
    }
    if (action === 'list') return out(readRecords(root));
    if (action === 'show') {
      const id = args[0];
      if (!id) throw new Error('Usage: knowledge show <id>');
      const { records } = readRecords(root);
      const record = records.find((item) => item.id === id);
      if (!record) throw new Error(`Unknown knowledge record '${id}'`);
      return out(record);
    }
    if (action === 'check') return out(checkRecords(root));
    if (action === 'index') return out(writeIndex(root));
    throw new Error('Use knowledge init, sources, list, show, check, or index');
  }
  if (cmd === 'completion') {
    if (args.length !== 1 || args[0] !== 'zsh') throw new Error('Usage: dtech-aidlc completion zsh');
    const script = join(dirname(fileURLToPath(import.meta.url)), 'completion.zsh');
    return out(readFileSync(script, 'utf8'));
  }
  if (cmd === 'repos') {
    const action = args.shift();
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
    const action = args.shift(), state = loadWorkflow(root, readGraph(), { migrate: action === 'add' }).state; if (!state) throw new Error('Initialize a workflow before managing dependencies');
    state.dependencies ??= {};
    if (action === 'add') {
      if (!args[0] || !args[1]) throw new Error('Usage: deps add <id> <dependency-id>');
      const id = safeId(args[0], 'item id'), dependency = safeId(args[1], 'dependency id'); if (id === dependency) throw new Error('An item cannot depend on itself');
      const edges = Object.hasOwn(state.dependencies, id) ? state.dependencies[id] : (state.dependencies[id] = []);
      if (!edges.includes(dependency)) edges.push(dependency);
      persistWorkflow(root, state, readGraph()); audit(root, 'dependency.added', { id, dependency }); return out({ id, dependencies: edges });
    }
    if (action === 'list') return out(Object.entries(state.dependencies).flatMap(([id, dependencies]) => dependencies.map(dependency => ({ id, dependency }))));
    if (action === 'pull') {
      const id = safeId(args[0], 'item id');
      const deps = Object.hasOwn(state.dependencies, id) ? state.dependencies[id] : [];
      const stages = state.stages ?? {};
      return out({ id, dependencies: deps.map((dependency) => ({
        id: dependency,
        status: Object.hasOwn(stages, dependency) ? stages[dependency].status : 'unknown',
      })) });
    }
    throw new Error('Use deps add, deps list, or deps pull');
  }
  throw new Error(`Unknown command ${cmd}; use dtech-aidlc help`);
}
try { main(); } catch (error) { fail(error.message); }
