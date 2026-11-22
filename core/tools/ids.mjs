// Machine-readable identifier registry (ids.json) and work-unit data (work-units.json).
// implementation-design mints FN/IF/TS/WU/CFG identifiers once; every later Develop and
// Launch artifact may cite them but not mint new ones. AC/US identifiers belong to Analyze.
import { readdirSync, readFileSync } from 'node:fs';
import { join, relative, sep } from 'node:path';
import { artifactPathFor } from './artifacts.mjs';

export const REGISTRY_PREFIXES = Object.freeze(['FN', 'IF', 'TS', 'WU', 'CFG']);
export const ANALYZE_PREFIXES = Object.freeze(['AC', 'US']);
// Phases whose artifacts are checked against the registry.
export const CITATION_PHASES = Object.freeze(['develop', 'launch']);
const CITATION = /\b(FN|IF|TS|WU|U|CFG|AC|US)-(\d{1,5})\b/g;
const UNIT_ID = /^(?:WU|U)-\d{1,5}$/;
const ANY_ID = /^(?:FN|IF|TS|WU|U|CFG|AC|US)-\d{1,5}$/;

const rel = (root, path) => relative(root, path).split(sep).join('/');

// Legacy work units were named U-01; they are the same identifiers as WU-01.
export function normalizeId(id) {
  return /^U-\d+$/.test(id) ? `W${id}` : id;
}

export function prefixOf(id) {
  return normalizeId(id).split('-')[0];
}

export function citedIds(text) {
  const found = new Set();
  for (const match of String(text).matchAll(CITATION)) found.add(normalizeId(`${match[1]}-${match[2]}`));
  return found;
}

function notApplicable(data) {
  return data && typeof data === 'object' && !Array.isArray(data) && data.status === 'not-applicable';
}

export function unitsFromData(data) {
  if (notApplicable(data)) return [];
  const units = Array.isArray(data?.units) ? data.units : data;
  return Array.isArray(units) ? units : [];
}

// Problems in a work-units.json text. An empty array means the file is valid.
export function workUnitProblems(text) {
  let data;
  try { data = JSON.parse(text); } catch { return ['work-units.json is not valid JSON']; }
  if (notApplicable(data)) {
    return typeof data.reason === 'string' && data.reason.trim() ? [] : ["a not-applicable work-units.json needs a non-empty 'reason'"];
  }
  const units = Array.isArray(data?.units) ? data.units : data;
  if (!Array.isArray(units) || units.length === 0) return ['work-units.json must list at least one unit under "units"'];
  const problems = [];
  const ids = new Map();
  for (const [index, row] of units.entries()) {
    if (!row || typeof row !== 'object' || Array.isArray(row)) { problems.push(`work-units.json unit ${index + 1} must be an object`); continue; }
    if (typeof row.id !== 'string' || !UNIT_ID.test(row.id)) { problems.push(`work-units.json unit ${index + 1} needs an id like WU-01 (got ${JSON.stringify(row.id ?? null)})`); continue; }
    const id = normalizeId(row.id);
    if (ids.has(id)) problems.push(`work-units.json lists ${row.id} more than once`);
    ids.set(id, row);
    if (row.dependsOn !== undefined && (!Array.isArray(row.dependsOn) || row.dependsOn.some((dep) => typeof dep !== 'string'))) {
      problems.push(`work-units.json ${row.id} dependsOn must be an array of unit ids`);
    }
    if (row.criteria !== undefined && (!Array.isArray(row.criteria) || row.criteria.some((item) => typeof item !== 'string' || !/^AC-\d+$/.test(item)))) {
      problems.push(`work-units.json ${row.id} criteria must be an array of AC- identifiers`);
    }
  }
  if (problems.length) return problems;
  for (const [id, row] of ids) {
    for (const dep of row.dependsOn ?? []) {
      const target = normalizeId(dep);
      if (target === id) problems.push(`work-units.json ${row.id} depends on itself`);
      else if (!ids.has(target)) problems.push(`work-units.json ${row.id} depends on unknown unit ${dep}`);
    }
  }
  if (problems.length) return problems;
  const state = new Map();
  const visit = (id, trail) => {
    if (state.get(id) === 'done') return null;
    if (state.get(id) === 'active') return [...trail, id];
    state.set(id, 'active');
    for (const dep of ids.get(id).dependsOn ?? []) {
      const cycle = visit(normalizeId(dep), [...trail, id]);
      if (cycle) return cycle;
    }
    state.set(id, 'done');
    return null;
  };
  for (const id of ids.keys()) {
    const cycle = visit(id, []);
    if (cycle) return [`work-units.json dependsOn forms a cycle: ${cycle.slice(cycle.indexOf(cycle.at(-1))).join(' -> ')}`];
  }
  return [];
}

// Problems in an ids.json text. unitIds, when given, are the normalized ids from work-units.json.
export function registryProblems(text, { unitIds = null } = {}) {
  let data;
  try { data = JSON.parse(text); } catch { return ['ids.json is not valid JSON']; }
  if (notApplicable(data)) {
    return typeof data.reason === 'string' && data.reason.trim() ? [] : ["a not-applicable ids.json needs a non-empty 'reason'"];
  }
  if (!data || typeof data !== 'object' || Array.isArray(data)) return ['ids.json must be an object'];
  if (data.schemaVersion !== 1) return ['ids.json needs "schemaVersion": 1'];
  if (!data.ids || typeof data.ids !== 'object' || Array.isArray(data.ids)) return ['ids.json needs an "ids" object keyed by prefix'];
  const problems = [];
  const seen = new Set();
  const traces = [];
  for (const [prefix, entries] of Object.entries(data.ids)) {
    if (!REGISTRY_PREFIXES.includes(prefix)) { problems.push(`ids.json has unknown prefix ${prefix}; use ${REGISTRY_PREFIXES.join(', ')}`); continue; }
    if (!Array.isArray(entries)) { problems.push(`ids.json ids.${prefix} must be an array`); continue; }
    const pattern = prefix === 'WU' ? UNIT_ID : new RegExp(`^${prefix}-\\d{1,5}$`);
    for (const entry of entries) {
      if (!entry || typeof entry !== 'object' || Array.isArray(entry) || typeof entry.id !== 'string' || !pattern.test(entry.id)) {
        problems.push(`ids.json ids.${prefix} has an entry without a valid ${prefix}- id`);
        continue;
      }
      const id = normalizeId(entry.id);
      if (seen.has(id)) problems.push(`ids.json registers ${entry.id} more than once`);
      seen.add(id);
      if (entry.title !== undefined && typeof entry.title !== 'string') problems.push(`ids.json ${entry.id} title must be a string`);
      if (entry.traces !== undefined) {
        if (!Array.isArray(entry.traces) || entry.traces.some((item) => typeof item !== 'string' || !ANY_ID.test(item))) {
          problems.push(`ids.json ${entry.id} traces must be an array of identifiers`);
        } else {
          for (const target of entry.traces) traces.push([entry.id, normalizeId(target)]);
        }
      }
    }
  }
  for (const [from, target] of traces) {
    if (REGISTRY_PREFIXES.includes(prefixOf(target)) && !seen.has(target) && !(prefixOf(target) === 'WU' && unitIds?.has(target))) {
      problems.push(`ids.json ${from} traces to unregistered ${target}`);
    }
  }
  const registeredUnits = [...seen].filter((id) => prefixOf(id) === 'WU');
  if (unitIds && registeredUnits.length) {
    const missing = [...unitIds].filter((id) => !seen.has(id));
    const extra = registeredUnits.filter((id) => !unitIds.has(id));
    if (missing.length || extra.length) {
      problems.push(`ids.json WU list must match work-units.json${missing.length ? `; missing ${missing.join(', ')}` : ''}${extra.length ? `; not in work-units.json ${extra.join(', ')}` : ''}`);
    }
  }
  return problems;
}

function readText(path) {
  try { return readFileSync(path, 'utf8'); } catch { return null; }
}

export function unitIdsFromFile(path) {
  const text = path ? readText(path) : null;
  if (text === null) return null;
  try {
    return new Set(unitsFromData(JSON.parse(text)).filter((row) => typeof row?.id === 'string').map((row) => normalizeId(row.id)));
  } catch {
    return null;
  }
}

// The identifiers an intent may cite: registry entries, work units, and Analyze definitions.
export function loadRegistry(root, graph, intentId) {
  const path = artifactPathFor(root, graph, intentId, 'id-registry');
  const unitsPath = artifactPathFor(root, graph, intentId, 'work-units-json');
  const result = { path: path ? rel(root, path) : null, exists: false, notApplicable: false, ids: new Set(), byPrefix: {}, error: null };
  const text = path ? readText(path) : null;
  if (text === null) return result;
  result.exists = true;
  let data;
  try { data = JSON.parse(text); } catch { result.error = 'ids.json is not valid JSON'; return result; }
  if (notApplicable(data)) { result.notApplicable = true; return result; }
  for (const [prefix, entries] of Object.entries(data?.ids ?? {})) {
    if (!Array.isArray(entries)) continue;
    for (const entry of entries) {
      if (typeof entry?.id !== 'string') continue;
      const id = normalizeId(entry.id);
      result.ids.add(id);
      (result.byPrefix[prefix] ??= []).push(id);
    }
  }
  for (const id of unitIdsFromFile(unitsPath) ?? []) result.ids.add(id);
  return result;
}

// AC/US identifiers defined by Analyze. null for a prefix means its source artifact is absent,
// so citations of that prefix are not checked.
export function analyzeDefinitions(root, graph, intentId) {
  const fromArtifact = (output, prefix) => {
    const path = artifactPathFor(root, graph, intentId, output);
    const text = path ? readText(path) : null;
    if (text === null) return null;
    return new Set([...citedIds(text)].filter((id) => prefixOf(id) === prefix));
  };
  return { AC: fromArtifact('acceptance-criteria', 'AC'), US: fromArtifact('user-stories', 'US') };
}

// Unregistered identifiers cited by the given Markdown files.
export function citationReport(root, graph, intentId, files) {
  const registry = loadRegistry(root, graph, intentId);
  if (!registry.exists || registry.notApplicable || registry.error) {
    return { applies: false, registry: registry.path, reason: registry.error ?? (registry.notApplicable ? 'ids.json is marked not-applicable' : 'this intent has no ids.json'), checked: [], unregistered: [] };
  }
  const defined = analyzeDefinitions(root, graph, intentId);
  const unregistered = [];
  const checked = [];
  for (const path of files) {
    if (!path.endsWith('.md')) continue;
    const text = readText(path);
    if (text === null) continue;
    checked.push(rel(root, path));
    for (const id of citedIds(text)) {
      const prefix = prefixOf(id);
      if (REGISTRY_PREFIXES.includes(prefix)) {
        if (!registry.ids.has(id)) unregistered.push({ id, file: rel(root, path), source: 'ids.json' });
      } else if (defined[prefix] && !defined[prefix].has(id)) {
        unregistered.push({ id, file: rel(root, path), source: prefix === 'AC' ? 'acceptance-criteria' : 'user-stories' });
      }
    }
  }
  return { applies: true, registry: registry.path, checked, unregistered };
}

export function formatUnregistered(unregistered, limit = 10) {
  const lines = unregistered.slice(0, limit).map((row) => `  ${row.id} in ${row.file} (not in ${row.source})`);
  if (unregistered.length > limit) lines.push(`  ... and ${unregistered.length - limit} more`);
  return lines.join('\n');
}

export function markdownFilesUnder(dir) {
  const files = [];
  const walk = (current) => {
    let entries;
    try { entries = readdirSync(current, { withFileTypes: true }); } catch { return; }
    for (const entry of entries) {
      const path = join(current, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.isFile() && entry.name.endsWith('.md')) files.push(path);
    }
  };
  walk(dir);
  return files.sort();
}

// Registered functions that no green verification report cites. Advisory: the trace table
// should have a row for every function the design promised.
export function uncoveredFunctions(root, graph, intentId, intentDirectory) {
  const registry = loadRegistry(root, graph, intentId);
  if (!registry.exists || registry.notApplicable || registry.error) return [];
  const functions = registry.byPrefix.FN ?? [];
  if (!functions.length) return [];
  const reports = markdownFilesUnder(join(intentDirectory, 'develop', 'build-and-verify'))
    .filter((path) => path.endsWith(`${sep}green-verification-report.md`) || path.endsWith('/green-verification-report.md'));
  const cited = new Set();
  for (const path of reports) for (const id of citedIds(readText(path) ?? '')) cited.add(id);
  return functions.filter((id) => !cited.has(id));
}
