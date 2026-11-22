// Named prerequisites: environment conditions a stage needs (Docker running, a toolchain, a
// database). The owner declares each one with a check command in .aidlc/config.json; the
// command comes from the owner's configuration, never from an artifact or the request.
//
//   "prerequisites": {
//     "docker": { "check": "docker info", "hint": "Start Docker Desktop", "stages": ["develop"] }
//   }
//
// `stages` (stage or phase ids) makes the check a preflight: orchestrate next runs it before
// selecting a matching stage. A stage reported blocked with --needs NAME is not re-selected
// until NAME's check passes.
import { spawnSync } from 'node:child_process';

export const PREREQUISITE_NAME = /^[a-z0-9][a-z0-9-]{0,31}$/;
const DEFAULT_TIMEOUT_SECONDS = 60;
const OUTPUT_LIMIT = 600;

export function prerequisitesOf(config) {
  const value = config?.prerequisites;
  return value && typeof value === 'object' && !Array.isArray(value) ? value : {};
}

export function validatePrerequisite(name, definition, graph = null) {
  if (!PREREQUISITE_NAME.test(String(name))) throw new Error(`Prerequisite name '${name}' must be 1-32 lowercase letters, digits or hyphens`);
  if (!definition || typeof definition !== 'object' || Array.isArray(definition)) throw new Error(`Prerequisite ${name} must be an object`);
  if (typeof definition.check !== 'string' || !definition.check.trim() || definition.check.length > 1000 || /[\u0000-\u001f]/.test(definition.check)) {
    throw new Error(`Prerequisite ${name} needs a one-line --check command`);
  }
  if (definition.hint !== undefined && (typeof definition.hint !== 'string' || definition.hint.length > 500)) throw new Error(`Prerequisite ${name} hint must be text up to 500 characters`);
  if (definition.timeoutSeconds !== undefined && (!Number.isInteger(definition.timeoutSeconds) || definition.timeoutSeconds < 1 || definition.timeoutSeconds > 600)) {
    throw new Error(`Prerequisite ${name} timeoutSeconds must be an integer from 1 to 600`);
  }
  if (definition.stages !== undefined) {
    if (!Array.isArray(definition.stages) || definition.stages.some((id) => typeof id !== 'string' || !id)) throw new Error(`Prerequisite ${name} stages must be a list of stage or phase ids`);
    if (graph) {
      const known = new Set([...graph.stages.map((stage) => stage.id), ...graph.phases.map((phase) => phase.id)]);
      const unknown = definition.stages.filter((id) => !known.has(id));
      if (unknown.length) throw new Error(`Prerequisite ${name} names unknown stage or phase: ${unknown.join(', ')}`);
    }
  }
  return definition;
}

export function requireKnownPrerequisites(config, names) {
  const defined = prerequisitesOf(config);
  const unknown = names.filter((name) => !Object.hasOwn(defined, name));
  if (unknown.length) {
    const known = Object.keys(defined);
    throw new Error(`Unknown prerequisite ${unknown.join(', ')}. ${known.length ? `Declared: ${known.join(', ')}.` : 'None are declared.'} Declare one with: dtech-aidlc config prerequisite set NAME --check "COMMAND" [--hint TEXT]`);
  }
}

export function preflightNames(config, stage) {
  return Object.entries(prerequisitesOf(config))
    .filter(([, definition]) => Array.isArray(definition?.stages) && (definition.stages.includes(stage.id) || definition.stages.includes(stage.phase)))
    .map(([name]) => name);
}

function tail(text) {
  const value = String(text ?? '').trim();
  return value.length > OUTPUT_LIMIT ? `...${value.slice(-OUTPUT_LIMIT)}` : value;
}

export function runPrerequisite(root, name, definition) {
  const timeout = (definition.timeoutSeconds ?? DEFAULT_TIMEOUT_SECONDS) * 1000;
  const startedAt = new Date().toISOString();
  // shell: the check is an owner-written command line from config (for example `docker info`),
  // and needs PATH and .cmd resolution on Windows.
  const run = spawnSync(definition.check, { cwd: root, shell: true, encoding: 'utf8', timeout, windowsHide: true });
  const timedOut = run.error?.code === 'ETIMEDOUT' || run.signal === 'SIGTERM';
  const ok = !run.error && run.status === 0;
  return {
    name, ok, check: definition.check, hint: definition.hint ?? null, exitCode: run.status ?? null, startedAt,
    ...(timedOut ? { timedOut: true } : {}),
    ...(ok ? {} : { output: tail(`${run.stdout ?? ''}\n${run.stderr ?? ''}\n${run.error?.message ?? ''}`) }),
  };
}

export function runPrerequisites(root, config, names) {
  const defined = prerequisitesOf(config);
  return [...new Set(names)].map((name) => (Object.hasOwn(defined, name)
    ? runPrerequisite(root, name, defined[name])
    // The owner removed the declaration, so it no longer holds the stage back.
    : { name, ok: true, check: null, hint: null, exitCode: null, undeclared: true }));
}
