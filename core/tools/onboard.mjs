import { createHash } from 'node:crypto';
import { existsSync, lstatSync, mkdirSync, readdirSync, readFileSync } from 'node:fs';
import { basename, extname, join, relative, resolve, sep } from 'node:path';
import { assertNoSymlinkComponents, configPath, engineRoot, readJson, writeFileAtomic } from './lib.mjs';

export const SKIP_DIRS = new Set([
  '.git', 'node_modules', 'dist', 'build', 'coverage', '.aidlc', '.claude', '.dsh',
  'vendor', 'target', 'out', 'obj', 'venv', 'virtualenv', '__pycache__', 'site-packages',
  'bower_components', 'Pods', 'third_party',
]);
export const LANGUAGE_BY_EXT = Object.freeze({
  '.js': 'javascript', '.jsx': 'javascript', '.mjs': 'javascript', '.cjs': 'javascript',
  '.ts': 'typescript', '.tsx': 'typescript', '.py': 'python', '.go': 'go', '.java': 'java',
  '.kt': 'kotlin', '.kts': 'kotlin', '.rs': 'rust', '.rb': 'ruby', '.php': 'php', '.cs': 'csharp',
  '.swift': 'swift', '.c': 'c', '.h': 'c', '.cpp': 'cpp', '.cc': 'cpp', '.hpp': 'cpp',
  '.dart': 'dart', '.vue': 'vue', '.svelte': 'svelte', '.scala': 'scala', '.sql': 'sql',
  '.sh': 'shell', '.bash': 'shell', '.ex': 'elixir', '.exs': 'elixir', '.tf': 'terraform',
});
const DOC_EXT = new Set(['.md', '.markdown', '.mdx', '.rst', '.adoc', '.txt', '.pdf', '.docx', '.doc', '.odt', '.html', '.htm', '.xlsx', '.pptx']);
const NEUTRAL_EXT = new Set(['.json', '.yaml', '.yml', '.toml', '.xml', '.ini', '.cfg', '.conf', '.lock', '.env', '.csv', '.tsv', '.svg', '.png', '.jpg', '.jpeg', '.gif', '.ico', '.webp', '.log']);
const MAX_DEPTH = 6;
const MAX_DOC_NOTE = 2 * 1024 * 1024;
const PIP_REQ = /^(requirements([-_].*)?|constraints)\.(txt|in)$/i;
const REQ_NAME = /(^|[^a-z0-9])(brd|prd|business[-_]?requirements?)([^a-z0-9]|$)/i;
const TEXT_EXT = new Set(['.md', '.markdown', '.rst', '.txt', '.adoc']);
const BINARY_EXT = new Set(['.pdf', '.doc', '.docx', '.xlsx', '.pptx']);

export function toPosix(path) {
  return String(path).split(sep).join('/');
}

function posixRel(root, abs) {
  return toPosix(relative(resolve(root), abs));
}

export function walk(dir, handlers = {}, depth = 0) {
  if (!existsSync(dir)) return;
  let entries = [];
  try { entries = readdirSync(dir, { withFileTypes: true }); }
  catch { handlers.skip?.(dir, basename(dir), 'unreadable'); return; }
  for (const entry of entries) {
    const absPath = join(dir, entry.name);
    if (entry.name.startsWith('.')) { handlers.skip?.(absPath, entry.name, 'hidden'); continue; }
    if (entry.isSymbolicLink()) { handlers.skip?.(absPath, entry.name, 'symlink'); continue; }
    const vendored = SKIP_DIRS.has(entry.name) || (entry.isDirectory() && existsSync(join(absPath, 'pyvenv.cfg')));
    if (vendored) { handlers.skip?.(absPath, entry.name, 'vendored-or-generated'); continue; }
    if (entry.isDirectory()) {
      if (depth + 1 > MAX_DEPTH) { handlers.skip?.(absPath, entry.name, 'depth-limit'); continue; }
      handlers.dir?.(absPath, entry.name, depth);
      walk(absPath, handlers, depth + 1);
    } else if (entry.isFile()) handlers.file?.(absPath, entry.name, depth);
  }
}

export function listFiles(root, relPath) {
  const abs = join(resolve(root), ...String(relPath).split('/').filter(Boolean));
  if (!existsSync(abs)) return [];
  const out = [];
  const visitFile = (path) => {
    try {
      const stat = lstatSync(path);
      if (!stat.isFile()) return;
      out.push({ path: posixRel(root, path), bytes: stat.size, mtimeMs: stat.mtimeMs });
    } catch {}
  };
  try {
    const stat = lstatSync(abs);
    if (stat.isFile()) visitFile(abs);
    else if (stat.isDirectory()) walk(abs, { file: (path) => visitFile(path) });
  } catch { return []; }
  return out.sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
}

function addLanguage(map, key, evidence) {
  const row = map.get(key) ?? { name: key, evidence: [] };
  const posix = toPosix(evidence);
  if (row.evidence.length < 8 && !row.evidence.includes(posix)) row.evidence.push(posix);
  map.set(key, row);
}

export function scanProject(root) {
  const project = resolve(root);
  const languages = new Map();
  const architecture = new Map();
  const skipped = { counts: { hidden: 0, 'vendored-or-generated': 0, symlink: 0, 'depth-limit': 0, unreadable: 0 }, examples: [] };
  let sourceFiles = 0;
  const manifests = [
    ['package.json', 'javascript'], ['tsconfig.json', 'typescript'], ['go.mod', 'go'],
    ['pyproject.toml', 'python'], ['requirements.txt', 'python'], ['pom.xml', 'java'],
    ['build.gradle', 'java'], ['Cargo.toml', 'rust'], ['composer.json', 'php'], ['Gemfile', 'ruby'],
    ['pubspec.yaml', 'dart'], ['mix.exs', 'elixir'], ['build.sbt', 'scala'],
  ];
  const unclassified = [];
  let unclassifiedFiles = 0;
  for (const [file, language] of manifests) if (existsSync(join(project, file))) addLanguage(languages, language, file);
  walk(project, {
    file(path, name) {
      const ext = extname(name).toLowerCase();
      const rel = posixRel(project, path);
      if (LANGUAGE_BY_EXT[ext]) {
        sourceFiles += 1;
        addLanguage(languages, LANGUAGE_BY_EXT[ext], rel);
      }
      if (name.endsWith('.csproj')) addLanguage(languages, 'csharp', rel);
      if (name === 'Dockerfile' || name === 'docker-compose.yml' || name === 'compose.yaml') addLanguage(architecture, 'containers', rel);
      if (name === 'pnpm-workspace.yaml' || name === 'turbo.json') addLanguage(architecture, 'monorepo', rel);
      if (ext && !LANGUAGE_BY_EXT[ext] && !name.endsWith('.csproj') && !DOC_EXT.has(ext) && !NEUTRAL_EXT.has(ext)) {
        unclassifiedFiles += 1;
        if (unclassified.length < 8) unclassified.push(rel);
      }
    },
    skip(absPath, name, reason) {
      skipped.counts[reason] = (skipped.counts[reason] ?? 0) + 1;
      if (reason !== 'hidden' && skipped.examples.length < 20) skipped.examples.push({ path: posixRel(project, absPath) || name, reason });
    },
  });
  if (existsSync(join(project, 'apps')) && existsSync(join(project, 'packages'))) addLanguage(architecture, 'monorepo', 'apps/ and packages/');
  const kind = sourceFiles > 0 || languages.size > 0 ? 'brownfield' : unclassifiedFiles > 0 ? 'unknown' : 'greenfield';
  return {
    kind,
    sourceFiles,
    unclassifiedFiles,
    unclassified,
    languages: [...languages.values()],
    architecture: [...architecture.values()],
    skipped,
  };
}

function stem(name) {
  const ext = extname(name);
  return ext ? name.slice(0, -ext.length) : name;
}

function isRootTextName(name, base) {
  const lower = name.toLowerCase();
  const wanted = base.toLowerCase();
  if (lower === wanted) return true;
  const ext = extname(lower);
  return TEXT_EXT.has(ext) && stem(lower) === wanted;
}

function openApiKind(name) {
  const lower = name.toLowerCase();
  const ext = extname(lower);
  if (!['.yaml', '.yml', '.json'].includes(ext)) return false;
  const base = stem(lower);
  return base === 'openapi' || base === 'swagger'
    || base.startsWith('openapi.') || base.startsWith('openapi-')
    || base.startsWith('swagger.') || base.startsWith('swagger-')
    || base.endsWith('.openapi') || base.endsWith('.swagger');
}

function requirementsKind(name) {
  if (PIP_REQ.test(name)) return false;
  return REQ_NAME.test(name);
}

function runbookKind(name) {
  return /runbook/i.test(name);
}

function architectureFile(name) {
  return isRootTextName(name, 'architecture') || /^architecture\b/i.test(stem(name));
}

function notesFor(path, bytes) {
  const notes = [];
  const ext = extname(path).toLowerCase();
  if (BINARY_EXT.has(ext)) notes.push(`binary format (${ext})`);
  if (bytes > MAX_DOC_NOTE) notes.push(`large file (${(bytes / (1024 * 1024)).toFixed(1)} MB)`);
  return notes;
}

function candidate(root, rel, kind, origin) {
  const files = listFiles(root, rel);
  const bytes = files.reduce((sum, file) => sum + file.bytes, 0);
  const type = existsSync(join(resolve(root), ...rel.split('/'))) && (() => {
    try { return lstatSync(join(resolve(root), ...rel.split('/'))).isDirectory() ? 'directory' : 'file'; } catch { return 'file'; }
  })();
  return { path: rel, type, kind, origin, files: files.length, bytes, notes: files.length === 1 ? notesFor(rel, bytes) : [] };
}

export function detectDocumentation(root, { explicit = [], detect = true } = {}) {
  const project = resolve(root);
  const skipped = [];
  const byPath = new Map();
  const add = (rel, kind, origin) => {
    if (!rel) return;
    const existing = byPath.get(rel);
    if (existing) {
      if (origin === 'explicit') existing.origin = 'explicit';
      return;
    }
    byPath.set(rel, candidate(project, rel, kind, origin));
  };
  if (detect) {
    let entries = [];
    try { entries = readdirSync(project, { withFileTypes: true }); } catch { entries = []; }
    for (const entry of entries) {
      const abs = join(project, entry.name);
      if (entry.name.startsWith('.')) { skipped.push({ path: entry.name, reason: 'hidden' }); continue; }
      if (entry.isSymbolicLink()) { skipped.push({ path: entry.name, reason: 'symlink' }); continue; }
      if (entry.isDirectory()) {
        const lower = entry.name.toLowerCase();
        if (['docs', 'doc', 'documentation'].includes(lower)) add(entry.name, 'docs', 'detected');
        if (['adr', 'adrs'].includes(lower)) add(entry.name, 'adr', 'detected');
      } else if (entry.isFile()) {
        if (isRootTextName(entry.name, 'README')) add(entry.name, 'readme', 'detected');
        if (isRootTextName(entry.name, 'CONTRIBUTING')) add(entry.name, 'conventions', 'detected');
        if (isRootTextName(entry.name, 'ARCHITECTURE')) add(entry.name, 'architecture', 'detected');
      }
    }
    walk(project, {
      file(path, name) {
        const rel = posixRel(project, path);
        if (openApiKind(name)) add(rel, 'api-spec', 'detected');
        if (requirementsKind(name)) add(rel, 'requirements', 'detected');
        if (runbookKind(name)) add(rel, 'runbook', 'detected');
        if (architectureFile(name) && rel.includes('/')) add(rel, 'architecture', 'detected');
      },
      dir(path, name) {
        const rel = posixRel(project, path);
        if (['adr', 'adrs'].includes(name.toLowerCase()) && rel.includes('/')) add(rel, 'adr', 'detected');
      },
      skip(absPath, name, reason) {
        skipped.push({ path: posixRel(project, absPath) || name, reason });
      },
    });
  }
  for (const item of explicit) {
    const rel = toPosix(item).replace(/^\.\//, '').replace(/\/$/, '');
    add(rel, 'docs', 'explicit');
  }
  const candidates = [...byPath.values()].sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
  return { candidates, skipped, truncated: false };
}

function writeGenerated(root, name, contents) {
  assertNoSymlinkComponents(root, `.aidlc/shared/${name}`);
  mkdirSync(join(resolve(root), '.aidlc', 'shared'), { recursive: true, mode: 0o700 });
  writeFileAtomic(join(resolve(root), '.aidlc', 'shared', name), contents, { mode: 0o600 });
}

const HASH_LIMIT = 20 * 1024 * 1024;
const SNAPSHOT_CAP = 2000;

function hashFile(root, rel, bytes) {
  if (bytes > HASH_LIMIT) return null;
  return createHash('sha256').update(readFileSync(join(resolve(root), ...rel.split('/')))).digest('hex');
}

export function enumerateSourceFiles(root, sources) {
  const byPath = new Map();
  const longestFirst = [...(sources ?? [])].sort((a, b) => String(b.path).length - String(a.path).length);
  for (const source of longestFirst) {
    for (const file of listFiles(root, source.path)) {
      if (!byPath.has(file.path)) byPath.set(file.path, { ...file, source: source.id });
    }
  }
  const files = [...byPath.values()].sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
  return { files: files.slice(0, SNAPSHOT_CAP), truncated: files.length > SNAPSHOT_CAP };
}

export function writeSourcesSnapshot(root, sources) {
  const { files, truncated } = enumerateSourceFiles(root, sources);
  const snapshot = {
    schemaVersion: 1,
    generated: true,
    scannedAt: new Date().toISOString(),
    truncated,
    files: files.map((file) => ({
      source: file.source,
      path: file.path,
      bytes: file.bytes,
      mtimeMs: file.mtimeMs,
      sha256: hashFile(root, file.path, file.bytes),
    })),
  };
  writeGenerated(root, 'sources-snapshot.json', `${JSON.stringify(snapshot, null, 2)}\n`);
  return snapshot;
}

export function onboardCommands(root) {
  const config = readJson(configPath(root), {}) || {};
  const harnessOk = typeof config.harness === 'string' && /^[a-z0-9-]+$/.test(config.harness);
  const scopeOk = ['solo', 'team', 'enterprise'].includes(config.scope);
  const create = harnessOk && scopeOk
    ? `dtech-aidlc onboard --harness ${config.harness} --scope ${config.scope}`
    : 'dtech-aidlc onboard --harness <claude|deepseek>';
  const scan = existsSync(configPath(root)) ? 'dtech-aidlc onboard --rescan' : 'dtech-aidlc onboard --harness <claude|deepseek>';
  return { create, scan };
}

function readSnapshot(root) {
  const path = join(resolve(root), '.aidlc', 'shared', 'sources-snapshot.json');
  try {
    if (!existsSync(path) || lstatSync(path).isSymbolicLink()) return null;
    const data = JSON.parse(readFileSync(path, 'utf8'));
    if (!data || data.schemaVersion !== 1 || !Array.isArray(data.files)) return null;
    return data;
  } catch { return null; }
}

function capList(paths) {
  const sorted = [...paths].sort();
  return { list: sorted.slice(0, 20), more: Math.max(0, sorted.length - 20) };
}

function hasLegacyBrd(root) {
  const dir = join(resolve(root), '.aidlc', 'shared', 'brd');
  try { return existsSync(dir) && lstatSync(dir).isDirectory() && readdirSync(dir).length > 0; } catch { return false; }
}

export function sourceStatus(root, knowledge) {
  const commands = onboardCommands(root);
  const warnings = [];
  const profile = readProjectProfile(root);
  const snapshot = readSnapshot(root);
  const scanned = Boolean(profile && !profile.outdated && snapshot);
  const result = {
    manifest: knowledge?.status ?? 'absent',
    scanned,
    scannedAt: snapshot?.scannedAt ?? null,
    sources: knowledge?.status === 'present' ? knowledge.sources.length : 0,
    missing: [],
    changed: [],
    added: [],
    removed: [],
    more: { changed: 0, added: 0, removed: 0 },
    legacyBrdCopies: hasLegacyBrd(root),
    warnings,
  };
  if (knowledge?.status === 'invalid') {
    warnings.push(`aidlc.knowledge.json is invalid: ${knowledge.error}`);
    return result;
  }
  if (knowledge?.status !== 'present') {
    let detectedDocs = [];
    try {
      detectedDocs = JSON.parse(readFileSync(join(resolve(root), '.aidlc', 'shared', 'project-profile.json'), 'utf8'))?.documentation?.detected ?? [];
    } catch { detectedDocs = []; }
    if (!scanned || detectedDocs.length > 0) {
      warnings.push(`No aidlc.knowledge.json was found, so no project documentation is registered for agents. Run \`${commands.create}\`.`);
    }
    return result;
  }
  if (!scanned) {
    warnings.push(`This project has not been scanned on this machine by this version of dtech-aidlc, so the project profile and the document change check are unavailable. Run \`${commands.scan}\`.`);
  }
  for (const source of knowledge.sources) {
    const abs = join(resolve(root), ...String(source.path).split('/'));
    if (!existsSync(abs)) {
      result.missing.push({ id: source.id, path: source.path });
      warnings.push(`Registered knowledge source ${source.id} (${source.path}) does not exist. Fix the path in aidlc.knowledge.json.`);
    }
  }
  if (scanned && snapshot) {
    const current = enumerateSourceFiles(root, knowledge.sources);
    const prev = new Map(snapshot.files.map((file) => [file.path, file]));
    const now = new Map(current.files.map((file) => [file.path, file]));
    const changed = [];
    const added = [];
    const removed = [];
    for (const [path, file] of now) {
      const before = prev.get(path);
      if (!before) {
        if (!snapshot.truncated) added.push(path);
        continue;
      }
      const sizeOrTime = before.bytes !== file.bytes || before.mtimeMs !== file.mtimeMs;
      if (!sizeOrTime) continue;
      if (before.sha256 == null) changed.push(path);
      else {
        const digest = hashFile(root, path, file.bytes);
        if (digest !== before.sha256) changed.push(path);
      }
    }
    if (!snapshot.truncated) {
      for (const path of prev.keys()) if (!now.has(path)) removed.push(path);
    }
    const changedCap = capList(changed);
    const addedCap = capList(added);
    const removedCap = capList(removed);
    result.changed = changedCap.list;
    result.added = addedCap.list;
    result.removed = removedCap.list;
    result.more = { changed: changedCap.more, added: addedCap.more, removed: removedCap.more };
    const total = changed.length + added.length + removed.length;
    if (total > 0) {
      const sample = [...changed, ...added, ...removed].sort().slice(0, 5).join(', ');
      const day = String(snapshot.scannedAt).slice(0, 10);
      warnings.push(`${total} registered documents changed since the last scan on ${day} (${sample}). Re-read them before relying on earlier conclusions. Run \`dtech-aidlc onboard --rescan\` to acknowledge.`);
    }
  }
  return result;
}

export function writeProjectProfile(root, scan, documentation = { candidates: [], skipped: [], truncated: false }) {
  const shared = join(resolve(root), '.aidlc', 'shared');
  const brdDir = join(shared, 'brd');
  let legacy;
  try {
    if (existsSync(brdDir) && lstatSync(brdDir).isDirectory()) {
      const files = readdirSync(brdDir).length;
      if (files) legacy = { brdCopies: { path: '.aidlc/shared/brd', files } };
    }
  } catch {}
  const profile = {
    schemaVersion: 2,
    generated: true,
    generatedBy: 'dtech-aidlc onboard',
    scannedAt: new Date().toISOString(),
    kind: scan.kind,
    sourceFiles: scan.sourceFiles,
    unclassifiedFiles: scan.unclassifiedFiles ?? 0,
    unclassified: scan.unclassified ?? [],
    languages: scan.languages,
    architecture: scan.architecture,
    documentation: {
      manifest: 'aidlc.knowledge.json',
      detected: documentation.candidates,
      skipped: documentation.skipped,
      truncated: documentation.truncated,
    },
    skipped: scan.skipped,
    ...(legacy ? { legacy } : {}),
  };
  writeGenerated(root, 'project-profile.json', `${JSON.stringify(profile, null, 2)}\n`);
  const docLines = profile.documentation.detected.length
    ? profile.documentation.detected.map((row) => `- ${row.path} (${row.kind}, ${row.files} files, ${row.bytes} bytes)${row.notes?.length ? `; ${row.notes.join('; ')}` : ''}`)
    : ['- none detected'];
  const skipLines = (documentation.skipped || []).map((row) => `- ${row.path}: ${row.reason}`);
  const counts = scan.skipped?.counts ?? {};
  const lines = [
    '# Project profile',
    '',
    '> Generated by `dtech-aidlc onboard`. Do not edit: this file is rewritten on every scan. Put project knowledge in PROJECT.md and documentation sources in aidlc.knowledge.json.',
    '',
    `Kind: ${profile.kind}${profile.kind === 'unknown' ? ` (${profile.unclassifiedFiles} files of unrecognized types, for example ${(profile.unclassified ?? []).slice(0, 3).join(', ')}). Treated as an existing project.` : ''}`,
    `Source files: ${profile.sourceFiles}`,
    '',
    '## Languages',
    ...(profile.languages.length ? profile.languages.map((row) => `- ${row.name}: ${row.evidence.join(', ')}`) : ['- none detected']),
    '',
    '## Architecture',
    ...(profile.architecture.length ? profile.architecture.map((row) => `- ${row.name}: ${row.evidence.join(', ')}`) : ['- none detected']),
    '',
    '## Documentation',
    ...docLines,
    '',
    '## Skipped',
    ...skipLines,
    `Counts: hidden ${counts.hidden ?? 0}, vendored-or-generated ${counts['vendored-or-generated'] ?? 0}, symlink ${counts.symlink ?? 0}, depth-limit ${counts['depth-limit'] ?? 0}, unreadable ${counts.unreadable ?? 0}`,
    '',
  ];
  if (legacy) {
    lines.push('## Legacy', `BRD copies from an earlier dtech-aidlc version are in .aidlc/shared/brd (${legacy.brdCopies.files} files). They are no longer updated or read; documents are now read in place. You may delete the folder.`, '');
  }
  writeGenerated(root, 'project-profile.md', lines.join('\n'));
  return { shared, profile };
}

export function requiredProjectArtifacts(root, stageId) {
  if (stageId !== 'context-and-risk') return [];
  try {
    const status = codebaseMapStatus(root);
    return status.required === true && !status.exists
      ? [{ path: status.path, reason: `the codebase map is required for an existing project (kind ${readProjectProfile(root)?.kind}); write it from ${status.template}` }]
      : [];
  } catch {
    return [];
  }
}

export function codebaseMapStatus(root) {
  const path = '.aidlc/knowledge/_codebase-map.md';
  const abs = join(resolve(root), path);
  const template = join(engineRoot(), 'methodology', 'templates', 'codebase-map.md');
  let profile = null;
  try { profile = readProjectProfile(root); } catch {}
  let stat;
  try { stat = lstatSync(abs); } catch {}
  const exists = Boolean(stat?.isFile() && !stat.isSymbolicLink() && stat.size > 0);
  const required = !profile || profile.outdated ? null : ['brownfield', 'unknown'].includes(profile.kind);
  return {
    path, template, exists, required,
    reason: required === null ? 'project not scanned' : required ? `project kind is ${profile.kind}` : 'project kind is greenfield',
    bytes: exists ? stat.size : 0,
    modifiedAt: exists ? stat.mtime.toISOString() : null,
  };
}

export function readProjectProfile(root) {
  const jsonPath = join(resolve(root), '.aidlc', 'shared', 'project-profile.json');
  try {
    if (!existsSync(jsonPath) || lstatSync(jsonPath).isSymbolicLink()) return null;
    const data = JSON.parse(readFileSync(jsonPath, 'utf8'));
    if (!data || typeof data !== 'object') return null;
    const schemaVersion = data.schemaVersion ?? 1;
    return {
      path: '.aidlc/shared/project-profile.md',
      jsonPath: '.aidlc/shared/project-profile.json',
      generated: true,
      schemaVersion,
      outdated: schemaVersion < 2,
      kind: data.kind,
      scannedAt: data.scannedAt,
      languages: (data.languages ?? []).map((row) => row.name ?? row),
    };
  } catch { return null; }
}

export function normalizeSourcePath(path) {
  return toPosix(path).replace(/^\.\//, '').replace(/\/$/, '');
}

export function registerCommand(paths) {
  let cmd = 'dtech-aidlc onboard --rescan';
  for (const path of paths) {
    cmd += /[^A-Za-z0-9._/-]/.test(path) ? ` --docs "${path}"` : ` --docs ${path}`;
  }
  return cmd;
}

export function planRegistration(existingManifest, defaultManifest, candidates) {
  const normalize = normalizeSourcePath;
  const registered = new Set((existingManifest ?? defaultManifest).sources.map((source) => normalize(source.path)));
  const added = [];
  const wouldAdd = [];
  const alreadyRegistered = [];
  const base = existingManifest
    ? { schemaVersion: 1, sources: [...existingManifest.sources] }
    : { schemaVersion: 1, sources: [...defaultManifest.sources] };
  const eligible = [];
  for (const candidate of candidates) {
    const path = normalize(candidate.path);
    if (registered.has(path) || base.sources.some((source) => normalize(source.path) === path)) {
      alreadyRegistered.push(path);
      continue;
    }
    if (existingManifest && candidate.origin !== 'explicit') {
      wouldAdd.push({ path, kind: candidate.kind, command: registerCommand([path]) });
      continue;
    }
    eligible.push(candidate);
  }
  const capacity = Math.max(0, 50 - base.sources.length);
  const explicit = eligible.filter((item) => item.origin === 'explicit');
  const detected = eligible.filter((item) => item.origin !== 'explicit');
  const keep = [];
  for (const item of [...explicit, ...detected]) {
    if (keep.length >= capacity) {
      wouldAdd.push({ path: normalize(item.path), kind: item.kind, command: registerCommand([item.path]), truncated: true });
    } else keep.push(item);
  }
  keep.sort((a, b) => candidates.indexOf(a) - candidates.indexOf(b));
  for (const item of keep) {
    const path = normalize(item.path);
    const id = path.replace(/[^A-Za-z0-9._-]+/g, '-').replace(/^-+|-+$/g, '') || 'docs';
    let unique = id, n = 2;
    while (base.sources.some((source) => source.id === unique)) unique = `${id}-${n++}`;
    const description = item.origin === 'explicit' ? 'Added with --docs.' : `Detected ${item.kind} source.`;
    base.sources.push({ id: unique, path, kind: item.kind, description });
    added.push(unique);
    registered.add(path);
  }
  let status;
  if (!existingManifest) status = added.length ? 'created' : 'absent';
  else status = added.length ? 'updated' : 'unchanged';
  return { manifest: base, status, added, wouldAdd, alreadyRegistered };
}
