import { copyFileSync, existsSync, lstatSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, extname, join, relative, resolve, sep } from 'node:path';

const SKIP = new Set(['.git', 'node_modules', 'dist', 'build', 'coverage', '.aidlc', '.claude', '.dsh']);
const SOURCE = new Set(['.js', '.jsx', '.ts', '.tsx', '.mjs', '.cjs', '.py', '.go', '.java', '.kt', '.rs', '.rb', '.php', '.cs', '.cpp', '.c', '.swift']);
const BRD_NAME = /(^|[^a-z])(brd|prd|business[-_ ]requirements?|requirements?)([^a-z]|$)/i;
const BRD_EXT = new Set(['.md', '.txt', '.pdf', '.docx', '.doc']);
const MAX_BRD = 2 * 1024 * 1024;

function walk(dir, visit, depth = 0) {
  if (depth > 6 || !existsSync(dir)) return;
  let entries = [];
  try { entries = readdirSync(dir, { withFileTypes: true }); } catch { return; }
  for (const entry of entries) {
    if (entry.name.startsWith('.') && entry.name !== '.aidlc') continue;
    if (SKIP.has(entry.name) || entry.isSymbolicLink()) continue;
    const path = join(dir, entry.name);
    if (entry.isDirectory()) walk(path, visit, depth + 1);
    else if (entry.isFile()) visit(path, entry.name);
  }
}

function add(map, key, evidence) {
  const row = map.get(key) ?? { name: key, evidence: [] };
  if (row.evidence.length < 8) row.evidence.push(evidence);
  map.set(key, row);
}

export function scanProject(root) {
  const project = resolve(root);
  const languages = new Map();
  const architecture = new Map();
  const brd = [];
  let sourceFiles = 0;
  const manifests = [
    ['package.json', 'javascript'],
    ['tsconfig.json', 'typescript'],
    ['go.mod', 'go'],
    ['pyproject.toml', 'python'],
    ['requirements.txt', 'python'],
    ['pom.xml', 'java'],
    ['build.gradle', 'java'],
    ['Cargo.toml', 'rust'],
    ['composer.json', 'php'],
    ['Gemfile', 'ruby'],
  ];
  for (const [file, language] of manifests) if (existsSync(join(project, file))) add(languages, language, file);
  walk(project, (path, name) => {
    const ext = extname(name).toLowerCase();
    const rel = relative(project, path);
    if (SOURCE.has(ext)) sourceFiles += 1;
    const byExt = { '.js': 'javascript', '.jsx': 'javascript', '.mjs': 'javascript', '.cjs': 'javascript', '.ts': 'typescript', '.tsx': 'typescript', '.py': 'python', '.go': 'go', '.java': 'java', '.kt': 'java', '.rs': 'rust', '.rb': 'ruby', '.php': 'php', '.cs': 'csharp', '.swift': 'swift' };
    if (byExt[ext]) add(languages, byExt[ext], rel);
    if (name.endsWith('.csproj')) add(languages, 'csharp', rel);
    if (name === 'Dockerfile' || name === 'docker-compose.yml' || name === 'compose.yaml') add(architecture, 'containers', rel);
    if (name === 'pnpm-workspace.yaml' || name === 'turbo.json') add(architecture, 'monorepo', rel);
    if (BRD_EXT.has(ext) && BRD_NAME.test(name) && !rel.split(sep).includes('..')) {
      let size = MAX_BRD + 1;
      try { size = lstatSync(path).size; } catch { /* ignore */ }
      brd.push({ source: rel, bytes: size, copy: size <= MAX_BRD });
    }
  });
  if (existsSync(join(project, 'apps')) && existsSync(join(project, 'packages'))) add(architecture, 'monorepo', 'apps/ and packages/');
  return {
    kind: sourceFiles > 0 || languages.size > 0 ? 'brownfield' : 'greenfield',
    sourceFiles,
    languages: [...languages.values()],
    architecture: [...architecture.values()],
    brd,
  };
}

export function writeProjectProfile(root, scan) {
  const shared = join(resolve(root), '.aidlc', 'shared');
  const brdDir = join(shared, 'brd');
  mkdirSync(brdDir, { recursive: true });
  const copied = [];
  for (const item of scan.brd.filter(row => row.copy)) {
    const from = join(resolve(root), item.source);
    if (!existsSync(from) || lstatSync(from).isSymbolicLink()) continue;
    const target = join(brdDir, basename(item.source));
    if (!existsSync(target)) copyFileSync(from, target);
    copied.push({ source: item.source, path: relative(resolve(root), target) });
  }
  const profile = { ...scan, brdCopied: copied, scannedAt: new Date().toISOString() };
  writeFileSync(join(shared, 'project-profile.json'), `${JSON.stringify(profile, null, 2)}\n`, { mode: 0o600 });
  const lines = [
    `# Project profile`,
    ``,
    `Kind: ${profile.kind}`,
    `Source files: ${profile.sourceFiles}`,
    ``,
    `## Languages`,
    ...(profile.languages.length ? profile.languages.map(row => `- ${row.name}: ${row.evidence.join(', ')}`) : ['- none detected']),
    ``,
    `## Architecture`,
    ...(profile.architecture.length ? profile.architecture.map(row => `- ${row.name}: ${row.evidence.join(', ')}`) : ['- none detected']),
    ``,
    `## BRD`,
    ...(copied.length ? copied.map(row => `- ${row.path} from ${row.source}`) : ['- none detected']),
    ``,
  ];
  writeFileSync(join(shared, 'project-profile.md'), lines.join('\n'), { mode: 0o600 });
  return { shared, profile };
}
