import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { existsSync, lstatSync, mkdirSync, readdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import { userInfo } from 'node:os';
import { dirname, join } from 'node:path';
import {
  assertNoSymlinkComponents, audit, configPath, engineRoot, loadConfig, orderedStages, readGraph, readJsonStrict, resolveDepth, stateDir, statePath, writeJsonAtomic,
} from './lib.mjs';
import { resolveAgentModel, validateModelConfig } from './agent-models.mjs';
import { ARTIFACT_VALIDATORS, extraRequiredArtifacts } from './context.mjs';
import { artifactPathFor, expectedArtifacts, verifyArtifacts } from './artifacts.mjs';
import { buildDirectiveContext } from './context.mjs';
import { knowledgeGateCheck, writeIndex } from './knowledge.mjs';

const ID = /^[a-z0-9][a-z0-9-]{0,63}$/;
const STATUSES = new Set(['in_progress', 'complete', 'blocked', 'skipped']);

export const intentDir = (root, id) => join(root, '.aidlc', 'intents', id);
export const intentStatePath = (root, id) => join(intentDir(root, id), 'state.json');

export function intentSlug(text) {
  const source = String(text);
  const folded = source.normalize('NFKD').replace(/[̀-ͯ]/g, '').replace(/[đĐ]/g, 'd').toLowerCase();
  const letters = (folded.match(/[\p{L}\p{N}]/gu) || []).length;
  const kept = (folded.match(/[a-z0-9]/g) || []).length;
  const base = folded.replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 48).replace(/-+$/, '');
  if (kept >= 3 && kept * 2 >= letters && base) return base;
  const day = new Date().toISOString().slice(0, 10).replaceAll('-', '');
  const hex = createHash('sha256').update(source.normalize('NFC')).digest('hex').slice(0, 6);
  return `intent-${day}-${hex}`;
}

function intentIdsOnDisk(root) {
  try {
    return readdirSync(join(stateDir(root), 'intents'), { withFileTypes: true })
      .filter((entry) => entry.isDirectory())
      .map((entry) => entry.name);
  } catch {
    return [];
  }
}

function corruptMessage(reason, root) {
  const ids = intentIdsOnDisk(root);
  return `.aidlc/state.json ${reason}. No changes were made. Intents on disk: ${ids.length ? ids.join(', ') : 'none'}. Run 'dtech-aidlc doctor' for details. To point the project at one of them run 'dtech-aidlc intent resume <id>'.`;
}

export function normalizeIntentText(value) {
  const text = String(value ?? '').replace(/^﻿/, '').replace(/\r\n/g, '\n').trim();
  if (!text) throw new Error('Intent text is empty');
  if (text.includes('\0')) throw new Error('Intent text contains a NUL character');
  return text;
}

export function sameIntentText(a, b) {
  const norm = (value) => String(value ?? '').normalize('NFC').trim().replace(/\s+/g, ' ');
  return norm(a) === norm(b);
}

export function deriveStatus(state, graph) {
  if (state.closed?.kind === 'abandoned') return 'abandoned';
  const stages = orderedStages(graph, state.scope, state.depth || 'full');
  if (stages.length && stages.every((stage) => ['complete', 'skipped'].includes(state.stages?.[stage.id]?.status))) return 'complete';
  return 'active';
}

function pathTaken(path) {
  try { lstatSync(path); return true; } catch (error) { if (error.code === 'ENOENT') return false; throw error; }
}

export function freeIntentId(root, base) {
  const taken = (id) => pathTaken(intentDir(root, id)) || pathTaken(join(root, '.aidlc', 'knowledge', `${id}.md`));
  if (!taken(base)) return base;
  let n = 2;
  while (taken(`${base}-${n}`)) n += 1;
  return `${base}-${n}`;
}

export function readPointer(root) {
  let raw;
  try { raw = readJsonStrict(statePath(root)); } catch { return { kind: 'corrupt', reason: 'cannot be parsed' }; }
  if (raw === undefined) return { kind: 'absent' };
  if (raw && typeof raw === 'object' && !Array.isArray(raw) && raw.version === 2
    && (raw.activeIntentId === null || (typeof raw.activeIntentId === 'string' && ID.test(raw.activeIntentId)))) {
    return { kind: 'pointer', activeIntentId: raw.activeIntentId };
  }
  if (raw && typeof raw === 'object' && !Array.isArray(raw) && raw.stages && typeof raw.stages === 'object' && !Array.isArray(raw.stages)) {
    return { kind: 'legacy', state: raw };
  }
  return { kind: 'corrupt', reason: 'is not a recognised state file' };
}

export function validateIntentState(state, graph, folderId, { checkId = true } = {}) {
  const errors = [];
  if (!state || typeof state !== 'object' || Array.isArray(state)) return ['not an object'];
  if (typeof state.intent !== 'string' || !state.intent.trim()) errors.push('intent is not a non-empty string');
  if (!graph.scopes || !Object.hasOwn(graph.scopes, state.scope)) errors.push('scope is not a key of graph.scopes');
  if (!state.stages || typeof state.stages !== 'object' || Array.isArray(state.stages)) errors.push('stages is not an object');
  else {
    for (const [id, row] of Object.entries(state.stages)) {
      if (!row || typeof row !== 'object' || Array.isArray(row) || !STATUSES.has(row.status)) {
        errors.push(`stage ${id} has an invalid status`);
      }
    }
  }
  if (checkId && (!ID.test(state.intentId ?? '') || (folderId && state.intentId !== folderId))) {
    errors.push('intentId does not match folder');
  }
  if (state.currentStage !== undefined && state.currentStage !== null) {
    try {
      if (!orderedStages(graph, state.scope).some((stage) => stage.id === state.currentStage)) {
        errors.push('currentStage is not a stage of the scope');
      }
    } catch {
      errors.push('currentStage is not a stage of the scope');
    }
  }
  if (state.dependencies !== undefined) {
    const deps = state.dependencies;
    if (!deps || typeof deps !== 'object' || Array.isArray(deps)
      || Object.values(deps).some((value) => !Array.isArray(value) || value.some((item) => typeof item !== 'string'))) {
      errors.push('dependencies is not an object of string arrays');
    }
  }
  return errors;
}

function normalise(state) {
  const copy = { ...state, version: 2, dependencies: state.dependencies ?? {}, stages: { ...(state.stages ?? {}) } };
  delete copy.project;
  for (const [id, row] of Object.entries(copy.stages)) {
    copy.stages[id] = { ...row, detail: typeof row?.detail === 'string' ? row.detail : null };
  }
  return copy;
}

export function loadIntent(root, graph, id) {
  const raw = readJsonStrict(intentStatePath(root, id));
  if (raw === undefined) {
    throw new Error(`Active intent '${id}' has no state file at .aidlc/intents/${id}/state.json. No changes were made.`);
  }
  const state = normalise(raw);
  const problems = validateIntentState(state, graph, id);
  if (problems.length) throw new Error(`.aidlc/intents/${id}/state.json is invalid: ${problems[0]}. No changes were made.`);
  state.status = deriveStatus(state, graph);
  return state;
}

export function saveIntent(root, state, graph) {
  if (graph) state.status = deriveStatus(state, graph);
  state.updatedAt = new Date().toISOString();
  mkdirSync(intentDir(root, state.intentId), { recursive: true });
  assertNoSymlinkComponents(root, `.aidlc/intents/${state.intentId}/state.json`);
  writeJsonAtomic(intentStatePath(root, state.intentId), state);
  return state;
}

export function savePointer(root, activeIntentId) {
  mkdirSync(stateDir(root), { recursive: true });
  writeJsonAtomic(statePath(root), { version: 2, activeIntentId, updatedAt: new Date().toISOString() });
}

function writeIntentMarkdown(root, id, intent) {
  const path = join(intentDir(root, id), 'intent.md');
  if (!existsSync(path)) writeFileSync(path, `# ${id}\n\n${intent}\n`, { flag: 'wx', mode: 0o600 });
}

function migrateLegacy(root, graph, legacy, originalBytes) {
  const problems = validateIntentState(legacy, graph, legacy.intentId, { checkId: Boolean(legacy.intentId) });
  if (problems.length) throw new Error(`.aidlc/state.json is invalid: ${problems[0]}. No changes were made.`);
  let id = legacy.intentId;
  if (!id) {
    const base = intentSlug(legacy.intent);
    id = base;
    try {
      const existing = readJsonStrict(intentStatePath(root, id));
      if (!(existing && existing.intent === legacy.intent && existing.createdAt === legacy.createdAt)) {
        id = freeIntentId(root, base);
      }
    } catch {
      if (pathTaken(intentDir(root, id)) || pathTaken(join(root, '.aidlc', 'knowledge', `${id}.md`))) id = freeIntentId(root, base);
    }
    legacy.intentId = id;
  }
  mkdirSync(intentDir(root, id), { recursive: true });
  writeIntentMarkdown(root, id, legacy.intent);
  saveIntent(root, legacy);
  const backup = join(stateDir(root), 'state.v1.bak.json');
  if (!existsSync(backup)) writeFileSync(backup, originalBytes, { flag: 'wx', mode: 0o600 });
  savePointer(root, id);
  audit(root, 'state.migrated', { from: 1, intentId: id });
  return legacy;
}

export function loadWorkflow(root, graph, { migrate = false, missingActive = 'throw' } = {}) {
  const pointer = readPointer(root);
  if (pointer.kind === 'absent') return { activeIntentId: null, state: null, legacy: false };
  if (pointer.kind === 'corrupt') throw new Error(corruptMessage(pointer.reason, root));
  if (pointer.kind === 'pointer') {
    if (!pointer.activeIntentId) return { activeIntentId: null, state: null, legacy: false };
    try {
      const state = loadIntent(root, graph, pointer.activeIntentId);
      if (deriveStatus(state, graph) === 'abandoned') return { activeIntentId: null, state: null, legacy: false };
      return { activeIntentId: pointer.activeIntentId, state, legacy: false };
    } catch (error) {
      const missing = String(error.message).includes('has no state file');
      if (missing && missingActive === 'null') return { activeIntentId: pointer.activeIntentId, state: null, legacy: false, missing: true };
      if (missing) {
        throw new Error(`${error.message} Run 'dtech-aidlc intent list', then 'dtech-aidlc intent resume <id>' or 'dtech-aidlc intent pause'.`);
      }
      throw error;
    }
  }
  const legacy = normalise(pointer.state);
  const problems = validateIntentState(legacy, graph, legacy.intentId, { checkId: Boolean(legacy.intentId) });
  if (problems.length) throw new Error(`.aidlc/state.json is invalid: ${problems[0]}. No changes were made.`);
  if (!migrate) return { activeIntentId: legacy.intentId ?? null, state: legacy, legacy: true };
  const originalBytes = readFileSync(statePath(root));
  const migrated = migrateLegacy(root, graph, legacy, originalBytes);
  return { activeIntentId: migrated.intentId, state: migrated, legacy: false };
}

export function createIntent(root, scope, intent, graph) {
  const intentId = freeIntentId(root, intentSlug(intent));
  const created = {
    version: 2, intentId, intent, scope, status: 'active', currentStage: null, stages: {}, dependencies: {},
    createdAt: new Date().toISOString(),
  };
  mkdirSync(intentDir(root, intentId), { recursive: true });
  writeIntentMarkdown(root, intentId, intent);
  saveIntent(root, created, graph);
  savePointer(root, intentId);
  audit(root, 'intent.created', { intent, intentId, scope });
  return created;
}

export function persistWorkflow(root, state, graph) {
  saveIntent(root, state, graph);
  savePointer(root, state.intentId);
  return state;
}

function progressOf(state, graph) {
  const stages = orderedStages(graph, state.scope, state.depth || 'full');
  const done = stages.filter((stage) => ['complete', 'skipped'].includes(state.stages?.[stage.id]?.status)).length;
  return { done, total: stages.length };
}

function titleOf(intent) {
  const line = String(intent ?? '').split(/\r?\n/)[0].trim();
  return line.slice(0, 80);
}

function listRow(state, graph, current) {
  const derived = deriveStatus(state, graph);
  const unfinished = derived === 'active';
  return {
    intentId: state.intentId,
    title: titleOf(state.intent),
    scope: state.scope,
    status: derived === 'complete' || derived === 'abandoned' ? derived : (current ? 'active' : 'paused'),
    current: Boolean(current) && unfinished,
    currentStage: state.currentStage ?? null,
    assignee: state.assignee ?? null,
    progress: progressOf(state, graph),
    units: Array.isArray(state.units) ? state.units : undefined,
    createdAt: state.createdAt ?? null,
    updatedAt: state.updatedAt ?? null,
  };
}

export function listIntents(root, graph) {
  const pointer = readPointer(root);
  const currentId = pointer.kind === 'pointer' ? pointer.activeIntentId : (pointer.kind === 'legacy' ? pointer.state.intentId ?? null : null);
  const rows = new Map();
  for (const id of intentIdsOnDisk(root).filter((name) => ID.test(name))) {
    try {
      const state = loadIntent(root, graph, id);
      rows.set(id, listRow(state, graph, pointer.kind !== 'corrupt' && id === currentId && deriveStatus(state, graph) !== 'abandoned'));
    } catch (error) {
      rows.set(id, { intentId: id, status: 'unreadable', error: error.message });
    }
  }
  if (pointer.kind === 'legacy') {
    const state = normalise(pointer.state);
    const id = state.intentId ?? 'legacy';
    rows.set(id, { ...listRow({ ...state, intentId: id }, graph, true), current: true });
  }
  if (pointer.kind === 'corrupt') {
    for (const row of rows.values()) row.current = false;
  }
  return [...rows.values()].sort((a, b) => String(a.createdAt ?? '').localeCompare(String(b.createdAt ?? '')) || String(a.intentId).localeCompare(String(b.intentId)));
}

export function checkWorkflowState(root, graph) {
  if (!graph) return [];
  const rows = [];
  const pointer = readPointer(root);
  if (pointer.kind === 'corrupt') {
    rows.push({ status: 'fail', message: pointer.reason === 'cannot be parsed' ? 'State pointer: .aidlc/state.json cannot be parsed' : 'State pointer: .aidlc/state.json is not a recognised state file' });
  } else if (pointer.kind === 'pointer' && pointer.activeIntentId && !existsSync(intentStatePath(root, pointer.activeIntentId))) {
    rows.push({ status: 'fail', message: `State pointer: active intent '${pointer.activeIntentId}' has no state file` });
  } else if (pointer.kind === 'legacy') {
    rows.push({ status: 'warn', message: 'State: version 1 format; it is migrated automatically by the next orchestrate or intent command' });
  }
  const hasState = pointer.kind !== 'absent' || intentIdsOnDisk(root).length > 0;
  if (hasState && !existsSync(configPath(root))) {
    rows.push({ status: 'warn', message: 'Project has workflow state but no .aidlc/config.json; run dtech-aidlc onboard --harness <claude|deepseek> --scope <scope>' });
  }
  for (const id of intentIdsOnDisk(root).filter((name) => ID.test(name))) {
    let raw;
    try { raw = readJsonStrict(intentStatePath(root, id)); }
    catch { rows.push({ status: 'fail', message: `Intent ${id}: cannot be parsed` }); continue; }
    if (raw === undefined) continue;
    const problems = validateIntentState(normalise(raw), graph, id);
    for (const problem of problems) rows.push({ status: 'fail', message: `Intent ${id}: ${problem}` });
    if (problems.length) continue;
    const state = loadIntent(root, graph, id);
    const stages = orderedStages(graph, state.scope, state.depth || 'full');
    const inScope = new Set(stages.map((stage) => stage.id));
    const graphIds = new Set(graph.stages.map((stage) => stage.id));
    for (const stageId of Object.keys(state.stages)) {
      if (graphIds.has(stageId) && !inScope.has(stageId)) continue;
      if (!graphIds.has(stageId)) rows.push({ status: 'warn', message: `Intent ${id}: stage ${stageId} is not in the current graph` });
    }
    let unfinishedSeen = false;
    for (const stage of stages) {
      const status = state.stages[stage.id]?.status;
      const finished = status === 'complete' || status === 'skipped';
      if (finished && unfinishedSeen) rows.push({ status: 'fail', message: `Intent ${id}: stage ${stage.id} is recorded ${status} although the earlier stage is not finished` });
      if (!finished) unfinishedSeen = true;
    }
    const firstUnfinished = stages.find((stage) => !['complete', 'skipped'].includes(state.stages[stage.id]?.status));
    if (state.currentStage && firstUnfinished && state.currentStage !== firstUnfinished.id) {
      rows.push({ status: 'fail', message: `Intent ${id}: currentStage ${state.currentStage} is not the first unfinished stage` });
    }
    for (const stage of stages) {
      const info = gateInfo(graph, state.scope, stage, state.depth || 'full');
      if (stage.id !== info.finalStage) continue;
      const status = state.stages[stage.id]?.status;
      if (status !== 'complete' && status !== 'skipped') continue;
      const decision = state.gates?.[info.id]?.decision;
      if (Array.isArray(state.stages[stage.id]?.reports) && decision !== 'approved') {
        rows.push({ status: 'fail', message: `Intent ${id}: gate ${info.id} was passed without a recorded approval` });
      }
    }
  }
  if (!rows.length && (pointer.kind === 'pointer' || pointer.kind === 'legacy' || intentIdsOnDisk(root).length)) {
    rows.push({ status: 'pass', message: 'Workflow state' });
  }
  return rows;
}

export function unfinishedIds(root, graph) {
  return listIntents(root, graph).filter((row) => row.status === 'active' || row.status === 'paused').map((row) => row.intentId);
}

export function utcStamp(date = new Date()) {
  return date.toISOString().replace(/\.\d{3}Z$/, 'Z').replace(/[-:]/g, '').replace('T', 'T');
}

export function quarantineCorruptPointer(root) {
  const from = statePath(root);
  const stamp = new Date().toISOString().replace(/\.\d{3}Z$/, 'Z').replace(/[-:]/g, '');
  const to = join(stateDir(root), `state.json.corrupt-${stamp}`);
  renameSync(from, to);
  return to;
}

function stageStatus(state, stage) {
  return state?.stages?.[stage.id]?.status ?? 'pending';
}

function validateScope(scope) {
  if (!['solo', 'team', 'enterprise'].includes(scope)) throw new Error('scope must be solo, team, or enterprise');
  return scope;
}

function workUnitsOf(state) {
  return Array.isArray(state.units) && state.units.length ? state.units : [{ id: null, status: 'in_progress' }];
}

function applyWorkUnits(root, graph, state) {
  if (Array.isArray(state.units) && state.units.length) return;
  const design = expectedArtifacts(root, graph, { ...state, currentUnit: undefined, depth: state.depth || 'full' }, graph.stages.find((item) => item.id === 'implementation-design') || { produces: [] });
  const jsonPath = design.find((item) => item.output === 'work-units')?.path?.replace(/\.md$/, '.json');
  if (!jsonPath || !existsSync(jsonPath)) return;
  try {
    const data = JSON.parse(readFileSync(jsonPath, 'utf8'));
    const units = Array.isArray(data.units) ? data.units : data;
    if (!Array.isArray(units) || !units.length) return;
    state.units = units.map((row, index) => ({
      id: String(row.id || `U-${String(index + 1).padStart(2, '0')}`),
      status: 'in_progress',
      dependsOn: row.dependsOn || [],
    }));
  } catch { /* leave implicit unit */ }
}

function unitFinished(unit) {
  return ['complete', 'skipped'].includes(unit.status);
}

function lastRepeatOfPhase(graph, state, stage) {
  const stages = orderedStages(graph, state.scope, state.depth || 'full').filter((item) => item.phase === stage.phase && item.repeat === 'work-unit');
  return stages.at(-1)?.id === stage.id;
}

function selectNextStage(graph, state, stages) {
  const units = workUnitsOf(state);
  const repeating = stages.filter((stage) => stage.repeat === 'work-unit');
  const firstRepeat = repeating[0];
  const lastRepeat = repeating.at(-1);
  const before = firstRepeat ? stages.slice(0, stages.indexOf(firstRepeat)) : stages;
  const after = lastRepeat ? stages.slice(stages.indexOf(lastRepeat) + 1) : [];
  const unfinishedBefore = before.find((stage) => !['complete', 'skipped'].includes(stageStatus(state, stage)));
  if (unfinishedBefore) {
    state.currentUnit = undefined;
    return unfinishedBefore;
  }
  if (firstRepeat) {
    const pending = units.find((unit) => !unitFinished(unit));
    if (pending) {
      state.currentUnit = pending.id || undefined;
      const unfinishedRepeat = repeating.find((stage) => {
        const row = state.stages[stage.id];
        const key = pending.id || '_';
        const done = row?.units?.[key]?.status;
        return !['complete', 'skipped'].includes(done || (pending.id ? undefined : row?.status));
      });
      if (unfinishedRepeat) return unfinishedRepeat;
    }
  }
  state.currentUnit = undefined;
  return after.find((stage) => !['complete', 'skipped'].includes(stageStatus(state, stage)));
}

function allowedApprovers(config, gateId) {
  const roles = config?.roles;
  if (!roles || typeof roles !== 'object') return null;
  const named = roles.approvers?.[gateId];
  if (Array.isArray(named) && named.length) return named;
  if (Array.isArray(roles.owner) && roles.owner.length) return roles.owner;
  return null;
}

export function changeIntentDepth(root, name, options = {}) {
  const graph = readGraph();
  const depth = resolveDepth(graph, name);
  const loaded = loadWorkflow(root, graph, { migrate: true });
  if (!loaded.state) throw new Error('No active intent');
  if (!options.approvedBy) throw new Error('intent depth requires --approved-by NAME');
  if (!options.detail) throw new Error('intent depth requires --detail TEXT');
  const from = loaded.state.depth || 'full';
  loaded.state.depth = depth;
  persistWorkflow(root, loaded.state, graph);
  const who = resolveApprover(root, options.approvedBy);
  audit(root, 'depth.changed', { intentId: loaded.state.intentId, from, to: depth, by: who.by, bySource: who.bySource, detail: options.detail });
  return { intentId: loaded.state.intentId, from, to: depth };
}

export function assignIntent(root, name) {
  const graph = readGraph();
  const loaded = loadWorkflow(root, graph, { migrate: true });
  if (!loaded.state) throw new Error('No active intent');
  const assignee = String(name || '').trim();
  if (!assignee) throw new Error('intent assign requires a name');
  loaded.state.assignee = assignee;
  persistWorkflow(root, loaded.state, graph);
  return { intentId: loaded.state.intentId, assignee };
}

export function gateInfo(graph, scope, stage, depth = 'full') {
  const phase = graph.phases.find((item) => item.id === stage.phase);
  const stages = orderedStages(graph, scope, depth).filter((item) => item.phase === stage.phase);
  const finalStage = stages.at(-1)?.id;
  return { id: phase?.gate ?? null, phase: stage.phase, finalStage };
}

function nearestConfigParent(root) {
  let current = dirname(root);
  const stop = dirname(current);
  while (true) {
    if (existsSync(configPath(current))) return current;
    const parent = dirname(current);
    if (parent === current) return null;
    current = parent;
    if (current === stop && stop === dirname(stop)) return null;
  }
}

function completePayload(state, graph, extra = {}) {
  const skippedStages = orderedStages(graph, state.scope).filter((stage) => state.stages?.[stage.id]?.status === 'skipped').map((stage) => stage.id);
  const approvals = Object.entries(state.gates ?? {}).map(([gate, row]) => ({ gate, by: row.by, at: row.at }));
  return { complete: true, intent: state.intent, intentId: state.intentId, skippedStages, approvals, ...extra };
}

function requireInitialized(root) {
  if (existsSync(configPath(root))) return;
  const parent = nearestConfigParent(root);
  throw new Error(`Project is not initialized at ${root}. Run 'dtech-aidlc onboard --harness <claude|deepseek> --scope <solo|team|enterprise>' there first.${parent ? ` An initialized project exists at ${parent}; run the command there or pass --project ${parent}.` : ''}`);
}

function draftIntent(root, scope, intent, graph, scopeSource, projectScope, depth = 'full') {
  const intentId = freeIntentId(root, intentSlug(intent));
  return {
    version: 2, intentId, intent, scope, scopeSource, depth, status: 'active', currentStage: null, stages: {}, dependencies: {},
    createdAt: new Date().toISOString(),
  };
}

export function buildDirective({ root, graph, config, state, stage, warnings = [], context, rework }) {
  const model = stage.agent ? resolveAgentModel(graph.agents.find((agent) => agent.id === stage.agent), config) : {};
  const phase = graph.phases.find((item) => item.id === stage.phase);
  const gate = gateInfo(graph, state.scope, stage, state.depth || 'full');
  const phaseGate = stage.id === gate.finalStage;
  const expected = expectedArtifacts(root, graph, state, stage);
  const directive = {
    intent: state.intent,
    intentId: state.intentId,
    intentDir: intentDir(root, state.intentId),
    scope: state.scope,
    depth: state.depth || 'full',
    scopeSource: state.scopeSource ?? 'config',
    ...(state.currentUnit ? { currentUnit: state.currentUnit } : {}),
    ...(state.assignee ? { assignee: state.assignee } : {}),
    warnings,
    gate,
    approvalRequiredAfter: gate.finalStage,
    artifacts: expected.map(({ output, kind, relPath }) => ({ output, kind, path: relPath })),
    stage: {
      ...stage,
      phaseGate,
      phaseFile: phase ? join(engineRoot(), phase.file) : null,
      ...(stage.agent ? { agentFile: join(engineRoot(), graph.agents.find((item) => item.id === stage.agent).file), ...(model.source === 'unsupported' ? {} : { model: model.model, modelSource: model.source }) } : {}),
      ...(rework ? { rework } : {}),
    },
  };
  if (context !== null && context !== undefined) directive.context = context;
  return directive;
}

export function orchestrateNext(root, options = {}, hooks = {}) {
  const graph = readGraph();
  const config = validateModelConfig(root, graph);
  const loaded = loadWorkflow(root, graph, { migrate: true });
  const requested = options.intent || null;
  const contextProvider = hooks.contextProvider ?? buildDirectiveContext;
  let state = loaded.state;
  const finished = state && ['complete', 'abandoned'].includes(deriveStatus(state, graph));
  let isNewIntent = false;
  let warnings = [];
  let scopeSource = state?.scopeSource ?? 'config';
  const projectScope = ['solo', 'team', 'enterprise'].includes(config.scope) ? config.scope : null;

  if (!requested) {
    if (!state || finished) {
      if (state && deriveStatus(state, graph) === 'complete') return completePayload(state, graph);
      const paused = unfinishedIds(root, graph);
      throw new Error(`No active intent. Start one with: dtech-aidlc orchestrate next --intent "..."${paused.length ? ` Paused intents: ${paused.join(', ')}. Continue one with: dtech-aidlc intent resume <id>.` : ''}`);
    }
  } else if (!state || finished) {
    if (state && deriveStatus(state, graph) === 'complete' && sameIntentText(requested, state.intent)) {
      return completePayload(state, graph, { hint: 'This intent is already complete. Use a different --intent text to start new work.' });
    }
    requireInitialized(root);
    const scope = validateScope(options.scope || config.scope || 'solo');
    scopeSource = options.scope ? 'flag' : 'config';
    const depth = resolveDepth(graph, options.depth);
    if (options.depth && options.depth !== 'full' && !options.approvedBy) {
      throw new Error('Creating an intent with --depth requires --approved-by NAME');
    }
    state = draftIntent(root, scope, requested, graph, scopeSource, projectScope, depth);
    isNewIntent = true;
    if (options.scope && projectScope && options.scope !== projectScope) {
      warnings.push(`Intent scope '${scope}' overrides the project scope '${projectScope}' from .aidlc/config.json (given by --scope).`);
    }
  } else if (options.depth && resolveDepth(graph, options.depth) !== (state.depth || 'full')) {
    throw new Error(`Intent '${state.intentId}' uses depth ${state.depth || 'full'}; the depth cannot change after an intent starts. Use 'dtech-aidlc intent depth'.`);
  } else if (!sameIntentText(requested, state.intent)) {
    const stages = orderedStages(graph, state.scope, state.depth || 'full');
    const nextUnfinished = stages.find((stage) => !['complete', 'skipped'].includes(stageStatus(state, stage)));
    const { done, total } = progressOf(state, graph);
    throw new Error(`Intent '${state.intentId}' is still in progress (stage ${state.currentStage || nextUnfinished?.id}, ${done} of ${total} stages finished). Finish it, set it aside with 'dtech-aidlc intent pause', or close it with 'dtech-aidlc intent abandon --detail "<reason>"', then start the new intent.`);
  } else if (options.scope && options.scope !== state.scope) {
    throw new Error(`Intent '${state.intentId}' uses scope ${state.scope}; the scope cannot change after an intent starts. Omit --scope to continue it.`);
  }

  if (!isNewIntent && !existsSync(configPath(root))) {
    warnings = [`Project has an active intent but no .aidlc/config.json. Run dtech-aidlc onboard --harness <claude|deepseek> --scope ${state.scope} to complete setup.`];
  }

  const stages = orderedStages(graph, state.scope, state.depth || 'full');
  applyWorkUnits(root, graph, state);
  const next = selectNextStage(graph, state, stages);
  if (!next) {
    if (!isNewIntent) persistWorkflow(root, { ...state, currentStage: null, currentUnit: undefined }, graph);
    return completePayload(state, graph);
  }

  const row = state.stages[next.id] ?? {};
  const reports = Array.isArray(row.reports) ? row.reports : [];
  const lastReport = reports.at(-1);
  const wasBlocked = row.status === 'blocked' || lastReport?.result === 'blocked';
  let rework;
  if (wasBlocked) {
    state.stages[next.id] = { ...row, status: 'in_progress', startedAt: row.startedAt, resumedAt: row.resumedAt ?? new Date().toISOString() };
    rework = {
      blockedAt: lastReport?.at ?? row.reportedAt ?? null,
      detail: lastReport?.detail ?? row.detail ?? null,
      attempt: (reports.length || 1) + 1,
    };
  } else {
    state.stages[next.id] ??= { status: 'in_progress', startedAt: new Date().toISOString() };
  }
  state.currentStage = next.id;
  const gate = gateInfo(graph, state.scope, next, state.depth || 'full');
  const stageForContext = { ...next, phaseGate: next.id === gate.finalStage };
  const expected = expectedArtifacts(root, graph, state, next);
  const contextInput = {
    root, graph, config, state, stage: stageForContext, intentDir: intentDir(root, state.intentId),
    isNewIntent, expectedArtifacts: expected, artifactPathFor: (outputId) => artifactPathFor(root, graph, state.intentId, outputId),
  };
  const context = contextProvider(contextInput);
  const directive = buildDirective({ root, graph, config, state, stage: next, warnings, context, rework });

  if (isNewIntent) {
    mkdirSync(intentDir(root, state.intentId), { recursive: true });
    writeIntentMarkdown(root, state.intentId, state.intent);
    saveIntent(root, state, graph);
    savePointer(root, state.intentId);
    audit(root, 'intent.created', { intent: state.intent, intentId: state.intentId, scope: state.scope, scopeSource, projectScope });
    if ((state.depth || 'full') !== 'full') {
      const who = resolveApprover(root, options.approvedBy);
      audit(root, 'depth.chosen', { intentId: state.intentId, depth: state.depth, by: who.by, bySource: who.bySource, detail: options.detail ?? null });
    }
  } else {
    persistWorkflow(root, state, graph);
  }
  audit(root, 'stage.selected', { intent: state.intent, intentId: state.intentId, stage: next.id, rework: Boolean(rework) });
  return directive;
}

export function resolveApprover(root, flagValue) {
  if (flagValue !== undefined && flagValue !== null && String(flagValue).length) {
    const by = String(flagValue).trim();
    if (by.length < 1 || by.length > 200 || /[\u0000-\u001f]/.test(by)) {
      throw new Error('--approved-by must be 1 to 200 characters without control characters');
    }
    return { by, bySource: 'flag' };
  }
  try {
    const git = spawnSync('git', ['config', '--get', 'user.name'], { cwd: root, encoding: 'utf8', timeout: 5000, windowsHide: true });
    if (git.status === 0 && git.stdout.trim()) return { by: git.stdout.trim(), bySource: 'git' };
  } catch {}
  try {
    const name = userInfo().username;
    if (name) return { by: name, bySource: 'os' };
  } catch {}
  return { by: 'unknown', bySource: 'none' };
}

export function orchestrateReport(root, options = {}) {
  const graph = readGraph();
  const state = loadWorkflow(root, graph, { migrate: true }).state;
  if (!state) throw new Error('No active workflow state');
  if (!state.currentStage) throw new Error('No current stage; run orchestrate next first');
  const requestedStage = options.stage || state.currentStage;
  if (options.stage && !graph.stages.some((item) => item.id === options.stage)) throw new Error(`Unknown stage '${options.stage}'`);
  const stage = graph.stages.find((item) => item.id === requestedStage);
  if (!stage) throw new Error('No current stage; run orchestrate next first');
  if (stage.id !== state.currentStage) throw new Error(`Only the current stage (${state.currentStage}) can be reported`);
  const result = options.result;
  if (!['complete', 'blocked', 'skipped'].includes(result)) throw new Error('--result must be complete, blocked, or skipped');
  if ((result === 'blocked' || result === 'skipped') && !options.detail) {
    throw new Error(`--result ${result} requires --detail "<reason>"`);
  }
  if (options.approval && !['approved', 'rejected'].includes(options.approval)) throw new Error('--approval must be approved or rejected');
  if (options.approval === 'approved' && result === 'blocked') {
    throw new Error('--approval approved cannot be combined with --result blocked; a blocked report never passes a gate');
  }
  const stages = orderedStages(graph, state.scope, state.depth || 'full');
  if (!stages.some((item) => item.id === stage.id)) throw new Error(`Stage ${stage.id} is not included in the active ${state.scope} scope`);
  const gate = gateInfo(graph, state.scope, stage, state.depth || 'full');
  const phaseGate = stage.id === gate.finalStage;
  if (!phaseGate && (options.approval || options.approvedBy || options.approvalNote)) {
    throw new Error(`Stage ${stage.id} is not a phase gate; --approval, --approved-by and --approval-note are only accepted on ${gate.finalStage}, the last stage of the ${gate.phase} phase`);
  }
  if ((options.approvedBy || options.approvalNote) && !options.approval) {
    throw new Error('--approved-by and --approval-note require --approval');
  }
  if (options.approval === 'rejected' && result !== 'blocked') {
    throw new Error('--approval rejected must be reported with --result blocked --detail "<the owner\'s feedback>"');
  }
  if (phaseGate && (result === 'complete' || result === 'skipped') && options.approval !== 'approved') {
    throw new Error(`Phase gate ${gate.id} after ${stage.id} requires explicit --approval approved from the project owner (name them with --approved-by NAME); this also applies when the stage is skipped.`);
  }
  let verified = { artifacts: [], problems: [] };
  if (result === 'complete') {
    const expected = expectedArtifacts(root, graph, state, stage);
    verified = verifyArtifacts(root, expected, { validators: ARTIFACT_VALIDATORS, graph, state, stage });
    if (!verified.ok) {
      const first = verified.problems[0];
      throw new Error(`Stage ${stage.id} cannot be completed: ${first.output} is ${first.problem} (${first.path})`);
    }
    const required = extraRequiredArtifacts({ root, graph, state, stage });
    const requiredCheck = verifyArtifacts(root, required.map((item) => ({
      output: item.path, path: join(root, item.path), relPath: item.path,
    })), { validators: ARTIFACT_VALIDATORS, graph, state, stage });
    if (!requiredCheck.ok) {
      const missing = new Map(requiredCheck.problems.map((item) => [item.path, item.problem]));
      const detail = required.filter((item) => missing.has(item.path))
        .map((item) => `  missing: ${item.path} (${item.reason})`).join('\n');
      throw new Error(`Stage ${stage.id} cannot be reported complete: ${missing.size} of ${required.length} expected artifacts are missing or invalid.\n${detail}`);
    }
    if (stage.id === 'knowledge-writeback') {
      const check = knowledgeGateCheck(root, graph, state);
      if (!check.ok) throw new Error(`Knowledge record is not ready: ${check.errors[0]} (${check.path})`);
    }
  }
  const now = new Date().toISOString();
  const previous = state.stages[stage.id] || {};
  let approvalRecord = null;
  let recordedApproval = null;
  let approver = { by: null, bySource: null };
  if (phaseGate && options.approval) {
    approver = resolveApprover(root, options.approvedBy);
    const allowed = allowedApprovers(loadConfig(root), gate.id);
    if (allowed && !allowed.includes(approver.by)) {
      throw new Error(`Approver '${approver.by}' is not allowed for gate ${gate.id}. Allowed: ${allowed.join(', ')}`);
    }
    recordedApproval = options.approval;
    approvalRecord = { decision: options.approval, by: approver.by, bySource: approver.bySource, note: options.approvalNote ?? null };
  }
  const reports = [...(previous.reports || []), { at: now, result, detail: options.detail ?? null, ...(approvalRecord ? { approval: approvalRecord } : {}) }];
  const unitKey = state.currentUnit || '_';
  const unitMap = { ...(previous.units || {}) };
  if (stage.repeat === 'work-unit') unitMap[unitKey] = { status: result, reportedAt: now };
  const units = workUnitsOf(state);
  const repeatingDone = stage.repeat === 'work-unit' && units.every((unit) => {
    const key = unit.id || '_';
    const status = (stage.id === state.currentStage && key === unitKey) ? result : previous.units?.[key]?.status;
    return ['complete', 'skipped'].includes(status);
  });
  const stageStatusValue = stage.repeat === 'work-unit' && !repeatingDone ? 'in_progress' : result;
  if (stage.repeat === 'work-unit' && repeatingDone && state.units) {
    const current = state.units.find((unit) => (unit.id || null) === (state.currentUnit || null));
    if (current && lastRepeatOfPhase(graph, state, stage)) current.status = result === 'skipped' ? 'skipped' : 'complete';
  }
  state.stages[stage.id] = {
    ...previous,
    status: stageStatusValue,
    reportedAt: now,
    detail: options.detail ?? null,
    reports,
    units: unitMap,
    ...(result === 'complete' ? { artifacts: [...(previous.artifacts || []), ...verified.artifacts] } : {}),
  };
  if (approvalRecord) {
    const phaseStages = stages.filter((item) => item.phase === stage.phase);
    const skippedStages = phaseStages.filter((item) => (item.id === stage.id ? result : state.stages[item.id]?.status) === 'skipped').map((item) => item.id);
    state.gates ??= {};
    state.gates[gate.id] = {
      phase: gate.phase, stage: stage.id, decision: options.approval, result, by: approver.by, bySource: approver.bySource,
      at: now, note: options.approvalNote ?? null, detail: options.detail ?? null, skippedStages, artifacts: [],
    };
  }
  state.currentStage = null;
  persistWorkflow(root, state, graph);
  audit(root, 'stage.reported', {
    intent: state.intent, intentId: state.intentId, stage: stage.id, phase: stage.phase, result,
    detail: options.detail ?? null, phaseGate, gate: phaseGate ? gate.id : null, gateApproval: recordedApproval,
    approvedBy: approver.by,
  });
  if (approvalRecord) {
    audit(root, 'gate.decided', {
      intentId: state.intentId, gate: gate.id, phase: gate.phase, stage: stage.id, decision: options.approval, result,
      by: approver.by, bySource: approver.bySource, note: options.approvalNote ?? null, detail: options.detail ?? null,
      skippedStages: state.gates[gate.id].skippedStages, artifacts: [],
    });
  }
  return {
    recorded: true, intentId: state.intentId, stage: stage.id, result, phaseGate, gate: gate.id,
    approval: recordedApproval, approvedBy: approver.by, intentComplete: deriveStatus(state, graph) === 'complete',
    ...(result === 'complete' ? { artifacts: verified.artifacts } : {}),
  };
}
