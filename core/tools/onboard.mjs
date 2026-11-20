import { existsSync, lstatSync, mkdirSync, readdirSync, readFileSync } from 'node:fs';
import { basename, extname, join, relative, resolve, sep } from 'node:path';
import { assertNoSymlinkComponents, writeFileAtomic } from './lib.mjs';

export const SKIP_DIRS = new Set([
  '.git', 'node_modules', 'dist', 'build', 'coverage', '.aidlc', '.claude', '.dsh',
  'vendor', 'target', 'out', 'obj', 'venv', 'virtualenv', '__pycache__', 'site-packages',
  'bower_components', 'Pods', 'third_party',
]);
const SOURCE = new Set(['.js', '.jsx', '.ts', '.tsx', '.mjs', '.cjs', '.py', '.go', '.java', '.kt', '.rs', '.rb', '.php', '.cs', '.cpp', '.c', '.swift']);
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
  ];
  for (const [file, language] of manifests) if (existsSync(join(project, file))) addLanguage(languages, language, file);
  walk(project, {
    file(path, name) {
      const ext = extname(name).toLowerCase();
      const rel = posixRel(project, path);
      if (SOURCE.has(ext)) sourceFiles += 1;
      const byExt = { '.js': 'javascript', '.jsx': 'javascript', '.mjs': 'javascript', '.cjs': 'javascript', '.ts': 'typescript', '.tsx': 'typescript', '.py': 'python', '.go': 'go', '.java': 'java', '.kt': 'java', '.rs': 'rust', '.rb': 'ruby', '.php': 'php', '.cs': 'csharp', '.swift': 'swift' };
      if (byExt[ext]) addLanguage(languages, byExt[ext], rel);
      if (name.endsWith('.csproj')) addLanguage(languages, 'csharp', rel);
      if (name === 'Dockerfile' || name === 'docker-compose.yml' || name === 'compose.yaml') addLanguage(architecture, 'containers', rel);
      if (name === 'pnpm-workspace.yaml' || name === 'turbo.json') addLanguage(architecture, 'monorepo', rel);
    },
    skip(absPath, name, reason) {
      skipped.counts[reason] = (skipped.counts[reason] ?? 0) + 1;
      if (reason !== 'hidden' && skipped.examples.length < 20) skipped.examples.push({ path: posixRel(project, absPath) || name, reason });
    },
  });
  if (existsSync(join(project, 'apps')) && existsSync(join(project, 'packages'))) addLanguage(architecture, 'monorepo', 'apps/ and packages/');
  return {
    kind: sourceFiles > 0 || languages.size > 0 ? 'brownfield' : 'greenfield',
    sourceFiles,
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
    if (!rel || byPath.has(rel)) return;
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
    `Kind: ${profile.kind}`,
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
