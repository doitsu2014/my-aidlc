import { existsSync, lstatSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { assertNoSymlinkComponents, assertSafeStateDir, engineRoot, orderedStages, safeId, writeFileAtomic } from './lib.mjs';

export const KNOWLEDGE_MANIFEST = 'aidlc.knowledge.json';
export const SOURCE_KINDS = Object.freeze(['project', 'docs', 'requirements', 'architecture', 'adr', 'api-spec', 'runbook', 'conventions', 'readme']);
export const knowledgeManifestPath = (root) => join(resolve(root), KNOWLEDGE_MANIFEST);
export const knowledgeDir = (root) => join(resolve(root), '.aidlc', 'knowledge');
export const recordPath = (root, id) => join(knowledgeDir(root), `${safeId(id, 'record id')}.md`);

export function findProjectFile(root) {
  let entries;
  try { entries = readdirSync(root, { withFileTypes: true }); } catch { return null; }
  const names = entries.filter((entry) => entry.isFile() && entry.name.toLowerCase() === 'project.md').map((entry) => entry.name).sort();
  return names.includes('PROJECT.md') ? 'PROJECT.md' : names[0] ?? null;
}

function isPlainObject(value) {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

export function validateKnowledgeManifest(data) {
  const fail = (reason) => { throw new Error(`Invalid knowledge manifest: ${reason}`); };
  if (!isPlainObject(data) || data.schemaVersion !== 1 || !Array.isArray(data.sources)) fail('expected schemaVersion 1 and a sources array');
  const extra = Object.keys(data).filter((key) => key !== 'schemaVersion' && key !== 'sources');
  if (extra.length) fail(`unknown key ${extra[0]}`);
  if (data.sources.length > 50) fail('at most 50 sources are allowed');
  const ids = new Set();
  let projectCount = 0;
  for (const source of data.sources) {
    if (!isPlainObject(source)) fail('each source must be an object');
    const id = source.id;
    for (const key of Object.keys(source)) {
      if (!['id', 'path', 'kind', 'description'].includes(key)) fail(`source ${id}: unknown key ${key}`);
    }
    try { safeId(source.id, 'source id'); } catch { fail('source id is invalid: use letters, digits, dot, underscore, or hyphen'); }
    if (ids.has(source.id)) fail(`source ${id}: duplicate id`);
    ids.add(source.id);
    if (!SOURCE_KINDS.includes(source.kind)) fail(`source ${id}: kind must be one of ${SOURCE_KINDS.join(', ')}`);
    if (source.kind === 'project') projectCount += 1;
    if (source.description !== undefined && (typeof source.description !== 'string' || /[\r\n]/.test(source.description) || source.description.length > 300)) {
      fail(`source ${id}: description must be one line of at most 300 characters`);
    }
    const path = source.path;
    if (typeof path !== 'string' || path.length < 1 || path.length > 400 || path.includes('\0') || path.includes('\\') || path.startsWith('/') || /^[A-Za-z]:/.test(path)) {
      fail(`source ${id}: path must be a relative path inside the project root using forward slashes`);
    }
    const parts = path.split('/');
    if (parts.some((part) => !part || part === '.' || part === '..')) fail(`source ${id}: path must be a relative path inside the project root using forward slashes`);
    if (parts[0] === '.git' || parts[0] === '.aidlc') fail(`source ${id}: path must not point into .git or .aidlc`);
  }
  if (projectCount > 1) fail('only one source may have kind project');
  return data;
}

export function defaultKnowledgeManifest(root) {
  const path = findProjectFile(root) ?? 'PROJECT.md';
  return { schemaVersion: 1, sources: [{ id: 'project', path, kind: 'project' }] };
}

export function loadKnowledgeManifest(root) {
  const path = knowledgeManifestPath(root);
  if (!existsSync(path)) return { exists: false, manifest: defaultKnowledgeManifest(root) };
  let data;
  try { data = JSON.parse(readFileSync(path, 'utf8')); } catch { throw new Error('Invalid knowledge manifest: cannot be parsed as JSON'); }
  return { exists: true, manifest: validateKnowledgeManifest(data) };
}

export function writeKnowledgeManifest(root, manifest) {
  validateKnowledgeManifest(manifest);
  writeFileAtomic(knowledgeManifestPath(root), `${JSON.stringify(manifest, null, 2)}\n`, { mode: 0o644 });
}

export function inspectSources(root, manifest) {
  const warnings = [];
  const sources = (manifest.sources ?? []).map((source) => {
    const abs = join(resolve(root), ...source.path.split('/'));
    let status = 'missing';
    let size = null;
    try {
      const stat = lstatSync(abs);
      if (stat.isSymbolicLink()) { status = 'symlink'; warnings.push(`Source ${source.id} is a symbolic link`); }
      else if (stat.isDirectory()) { status = 'ok'; size = null; }
      else if (stat.isFile()) { status = 'ok'; size = stat.size; }
      else status = 'unsafe';
    } catch { warnings.push(`Source ${source.id} path ${source.path} does not exist`); }
    return { ...source, status, size };
  });
  return { sources, project: sources.find((item) => item.kind === 'project') ?? null, warnings };
}

export function addSource(root, { id, path, kind, description }) {
  const loaded = loadKnowledgeManifest(root);
  const manifest = loaded.exists ? loaded.manifest : defaultKnowledgeManifest(root);
  if (manifest.sources.some((source) => source.id === id)) throw new Error(`Source '${id}' already exists`);
  const source = { id, path, kind, ...(description ? { description } : {}) };
  manifest.sources.push(source);
  writeKnowledgeManifest(root, manifest);
  return source;
}

export function removeSource(root, id) {
  const loaded = loadKnowledgeManifest(root);
  if (!loaded.exists) throw new Error('No aidlc.knowledge.json');
  const before = loaded.manifest.sources.length;
  loaded.manifest.sources = loaded.manifest.sources.filter((source) => source.id !== id);
  if (loaded.manifest.sources.length === before) throw new Error(`Unknown source '${id}'`);
  writeKnowledgeManifest(root, loaded.manifest);
  return { removed: id };
}

export function knowledgePreflight(root) {
  assertSafeStateDir(root);
  const path = knowledgeManifestPath(root);
  if (existsSync(path)) {
    try { if (lstatSync(path).isSymbolicLink()) throw new Error('Refusing to write through symlink: aidlc.knowledge.json'); } catch (error) {
      if (error.message.startsWith('Refusing')) throw error;
    }
  }
  return loadKnowledgeManifest(root);
}

function templatePath() {
  return fileURLToPath(new URL('../data/scaffold/PROJECT.md', import.meta.url));
}

export function initKnowledge(root, { createManifest = false } = {}) {
  const loaded = knowledgePreflight(root);
  const warnings = [];
  let target = loaded.exists
    ? loaded.manifest.sources.find((source) => source.kind === 'project')?.path ?? null
    : findProjectFile(root) ?? 'PROJECT.md';
  let project;
  if (loaded.exists && !target) project = { path: null, created: false, reason: 'no project source declared' };
  else {
    const abs = join(resolve(root), target);
    try {
      lstatSync(abs);
      project = { path: target, created: false, reason: 'already exists' };
    } catch {
      const parent = dirname(abs);
      if (!existsSync(parent)) project = { path: target, created: false, reason: 'parent directory does not exist' };
      else {
        const scaffold = templatePath();
        if (!existsSync(scaffold)) throw new Error(`Missing scaffold file: ${scaffold}`);
        try {
          writeFileSync(abs, readFileSync(scaffold, 'utf8'), { flag: 'wx', mode: 0o644 });
          project = { path: target, created: true };
        } catch (error) {
          if (error.code === 'EEXIST') project = { path: target, created: false, reason: 'already exists' };
          else throw error;
        }
      }
    }
  }
  let manifestStatus = 'absent';
  if (loaded.exists) manifestStatus = 'unchanged';
  else if (createManifest) {
    writeKnowledgeManifest(root, { schemaVersion: 1, sources: [{ id: 'project', path: target, kind: 'project' }] });
    manifestStatus = 'created';
  }
  assertNoSymlinkComponents(root, '.aidlc/knowledge');
  mkdirSync(knowledgeDir(root), { recursive: true });
  return { project, manifest: KNOWLEDGE_MANIFEST, manifestStatus, knowledgeDir: '.aidlc/knowledge', warnings };
}

export function ensureKnowledgeDir(root) {
  assertSafeStateDir(root);
  assertNoSymlinkComponents(root, '.aidlc/knowledge');
  mkdirSync(knowledgeDir(root), { recursive: true });
}

export function parseRecord(text) {
  const match = text.match(/^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)/);
  if (!match) return { error: 'frontmatter is missing' };
  const frontmatter = {};
  for (const line of match[1].split(/\r?\n/)) {
    const scalar = line.match(/^([\w-]+):\s*(.*?)\s*$/);
    if (scalar) frontmatter[scalar[1]] = scalar[2].replace(/^['"]|['"]$/g, '');
  }
  const splitList = (key) => String(frontmatter[key] ?? '').replace(/^\[|\]$/g, '').split(',').map((item) => item.trim()).filter(Boolean);
  const body = text.slice(match[0].length);
  const titleLine = body.split(/\r?\n/).find((line) => line.startsWith('# '));
  const headings = new Map();
  const lines = body.split(/\r?\n/);
  let current = null, buf = [];
  const flush = () => { if (current) headings.set(current, buf.join('\n').trim()); };
  for (const line of lines) {
    if (line.startsWith('## ')) { flush(); current = line.slice(3).trim(); buf = []; }
    else if (line.startsWith('# ')) { flush(); current = null; buf = []; }
    else if (current) buf.push(line);
  }
  flush();
  return { frontmatter, lists: { 'phases-run': splitList('phases-run'), supersedes: splitList('supersedes'), tags: splitList('tags') }, body, title: titleLine ? titleLine.slice(2).trim() : '', headings };
}

export function readRecords(root) {
  const dir = knowledgeDir(root);
  const records = [], invalid = [];
  let entries;
  try {
    if (lstatSync(dir).isSymbolicLink()) return { records, invalid };
    entries = readdirSync(dir, { withFileTypes: true });
  } catch { return { records, invalid }; }
  for (const entry of entries) {
    if (!entry.name.endsWith('.md') || entry.name.startsWith('_') || entry.name.startsWith('.')) continue;
    const file = join(dir, entry.name);
    const rel = `.aidlc/knowledge/${entry.name}`;
    if (entry.isSymbolicLink() || !entry.isFile()) { invalid.push({ file: rel, errors: ['is a symbolic link or not a regular file'] }); continue; }
    const stem = entry.name.slice(0, -3);
    try { safeId(stem, 'record id'); } catch { invalid.push({ file: rel, errors: ['file name is not a valid record id'] }); continue; }
    const text = readFileSync(file, 'utf8');
    const parsed = parseRecord(text);
    if (parsed.error) {
      records.push({ id: stem, title: stem, closed: null, origin: 'legacy', scope: null, tags: [], supersedes: [], warnings: ['no frontmatter; add it using the knowledge-record template'], file: rel });
      continue;
    }
    records.push({
      id: parsed.frontmatter.id || stem,
      title: parsed.frontmatter.title || parsed.title || stem,
      origin: parsed.frontmatter.origin || 'archived',
      scope: parsed.frontmatter.scope || null,
      closed: parsed.frontmatter.closed || null,
      tags: parsed.lists.tags,
      supersedes: parsed.lists.supersedes,
      file: rel,
      parsed,
    });
  }
  return { records, invalid };
}

export function writeIndex(root) {
  ensureKnowledgeDir(root);
  const { records, invalid } = readRecords(root);
  const lines = ['# Knowledge index', '', `Records: ${records.length}. Invalid: ${invalid.length}.`, ''];
  for (const record of records) lines.push(`- ${record.id} — ${record.title} (${record.origin}${record.closed ? `, closed ${record.closed}` : ''})`);
  for (const item of invalid) lines.push(`- INVALID ${item.file}: ${item.errors.join('; ')}`);
  writeFileAtomic(join(knowledgeDir(root), '_index.md'), `${lines.join('\n')}\n`, { mode: 0o644 });
  return { records, invalid, index: '.aidlc/knowledge/_index.md' };
}

export function expectedRecordFrontmatter(graph, state) {
  const phases = graph.scopes[state.scope] ?? [];
  const run = phases.filter((phase) => {
    const stages = (graph.stages ?? []).filter((stage) => stage.phase === phase);
    return stages.some((stage) => ['complete', 'skipped'].includes(state.stages?.[stage.id]?.status) || stage.id === 'knowledge-writeback');
  });
  return {
    id: state.intentId,
    origin: 'intent',
    closed: new Date().toISOString().replace(/\.\d{3}Z$/, 'Z'),
    scope: state.scope,
    'phases-run': run.join(', '),
  };
}

function knowledgeTemplatePath() {
  const engine = join(engineRoot(), 'methodology', 'templates', 'knowledge-record.md');
  if (existsSync(engine)) return engine;
  return fileURLToPath(new URL('../methodology/templates/knowledge-record.md', import.meta.url));
}

export function knowledgeGateCheck(root, graph, state) {
  const rel = `.aidlc/knowledge/${state.intentId}.md`;
  const path = recordPath(root, state.intentId);
  const template = knowledgeTemplatePath();
  const errors = [];
  if (!existsSync(path)) return { ok: false, path: rel, template, errors: ['missing file'] };
  try {
    const stat = lstatSync(path);
    if (stat.isSymbolicLink() || !stat.isFile()) return { ok: false, path: rel, template, errors: ['is a symbolic link or not a regular file'] };
  } catch {
    return { ok: false, path: rel, template, errors: ['missing file'] };
  }
  const parsed = parseRecord(readFileSync(path, 'utf8'));
  if (parsed.error) return { ok: false, path: rel, template, errors: [parsed.error] };
  if (parsed.frontmatter.id !== state.intentId) errors.push(`id must be ${state.intentId}`);
  if (parsed.frontmatter.origin !== 'intent') errors.push('origin must be intent');
  if (!parsed.frontmatter.title || parsed.frontmatter.title.startsWith('<')) errors.push('title is empty');
  if (parsed.title && parsed.title.startsWith('<')) errors.push('title is empty');
  if (!parsed.frontmatter.closed) errors.push('closed must be a date such as 2026-11-20 or 2026-11-20T10:15:00Z');
  if (parsed.frontmatter.scope && parsed.frontmatter.scope !== state.scope) errors.push(`scope must be ${state.scope}`);
  for (const heading of ['Outcome', 'Decisions', 'Limitations', 'Follow-ups', 'Links']) {
    if (!parsed.headings.has(heading)) errors.push(`missing heading ${heading}`);
  }
  return { ok: errors.length === 0, path: rel, template, errors };
}

const IGNORE_BLOCK = [
  '# >>> dtech-aidlc managed block v1',
  '/state.json',
  '/state.v1.bak.json',
  '/audit.jsonl',
  '/config.json',
  '/shared/',
  '/repos/',
  '*.tmp',
  '# <<< dtech-aidlc managed block',
].join('\n');

export function ensureIgnoreFile(root) {
  assertSafeStateDir(root);
  assertNoSymlinkComponents(root, '.aidlc/.gitignore');
  mkdirSync(join(root, '.aidlc'), { recursive: true });
  const path = join(root, '.aidlc', '.gitignore');
  if (!existsSync(path)) {
    writeFileAtomic(path, `${IGNORE_BLOCK}\n`, { mode: 0o600 });
    return { path: '.aidlc/.gitignore', result: 'created' };
  }
  const text = readFileSync(path, 'utf8');
  const lines = text.split(/\r?\n/);
  const start = lines.findIndex((line) => line.startsWith('# >>> dtech-aidlc managed block'));
  const end = lines.findIndex((line) => line.startsWith('# <<< dtech-aidlc managed block'));
  let next;
  if (start >= 0 && end > start && lines.filter((line) => line.startsWith('# >>> dtech-aidlc managed block')).length === 1) {
    next = [...lines.slice(0, start), ...IGNORE_BLOCK.split('\n'), ...lines.slice(end + 1)].join('\n');
    if (!next.endsWith('\n')) next += '\n';
  } else {
    const rest = lines.filter((line) => !line.startsWith('# >>> dtech-aidlc managed block') && !line.startsWith('# <<< dtech-aidlc managed block'));
    next = `${IGNORE_BLOCK}\n\n${rest.join('\n').replace(/^\n+/, '')}`;
    if (!next.endsWith('\n')) next += '\n';
  }
  if (next === (text.endsWith('\n') ? text : `${text}\n`) || next === text) return { path: '.aidlc/.gitignore', result: 'unchanged' };
  writeFileAtomic(path, next, { mode: 0o600 });
  return { path: '.aidlc/.gitignore', result: 'updated' };
}

export function checkRecords(root) {
  const { records, invalid } = readRecords(root);
  const problems = invalid.map((item) => `${item.file}: ${item.errors.join('; ')}`);
  for (const record of records) {
    if (record.origin === 'archived' && record.parsed) {
      for (const heading of ['Outcome', 'Decisions', 'Limitations', 'Follow-ups', 'Links']) {
        if (!record.parsed.headings.has(heading)) problems.push(`${record.id}: missing heading ${heading}`);
      }
    }
  }
  return { ok: problems.length === 0, problems, records: records.length };
}
