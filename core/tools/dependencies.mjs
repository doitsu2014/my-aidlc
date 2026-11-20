import { spawnSync } from 'node:child_process';
import {
  closeSync, existsSync, lstatSync, mkdirSync, openSync, readFileSync,
  realpathSync, renameSync, rmSync, writeFileSync,
} from 'node:fs';
import { dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { writeFileAtomic } from './lib.mjs';

const MANIFEST = 'deps.json';
const LOCK = 'deps.lock';
const LOCKFILE = 'deps-lock.json';
const REPOS = 'repos';
const GIT_TIMEOUT = 120_000;
const SAFE_NAME = /^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$/;

function fail(message) { throw new Error(message); }
function checkName(name) {
  if (typeof name !== 'string' || !SAFE_NAME.test(name) || name === '.' || name === '..') fail(`Invalid dependency name: ${String(name)}`);
  return name;
}
function validateUrl(url) {
  if (typeof url !== 'string' || !url || url.startsWith('-') || /[\s\0]/.test(url)) fail('Invalid repository URL');
  let ok = false;
  if (/^https:\/\//i.test(url)) {
    try {
      const parsed = new URL(url);
      ok = parsed.protocol === 'https:' && Boolean(parsed.hostname) && !parsed.username && !parsed.password;
    } catch { /* invalid */ }
  } else if (/^ssh:\/\//i.test(url)) {
    try {
      const parsed = new URL(url);
      ok = parsed.protocol === 'ssh:' && Boolean(parsed.hostname) && !parsed.password;
    } catch { /* invalid */ }
  } else if (/^git@[^/:\s]+:.+/.test(url)) {
    ok = !/[?#]/.test(url) && !url.slice(url.indexOf(':') + 1).split('/').includes('..');
  } else if (isAbsolute(url)) {
    ok = !url.split(sep).includes('..') && !url.startsWith('//');
  }
  if (!ok || /^(?:file|ext)::/i.test(url)) fail('Invalid repository URL: use https://, ssh://, git@host:path, or an absolute local path without embedded passwords or tokens');
  return url;
}
function checkRef(ref) {
  if (ref === undefined) return undefined;
  if (typeof ref !== 'string' || !ref || ref.startsWith('-') || /[\s\0~^:?*\[\\]/.test(ref) || ref.includes('..') || ref.includes('@{') || ref.endsWith('/') || ref.endsWith('.') || ref.includes('//')) fail('Invalid dependency ref');
  return ref;
}
function safeRoot(root) {
  if (typeof root !== 'string' || !isAbsolute(resolve(root))) fail('Project root must be an absolute path');
  const abs = resolve(root);
  if (existsSync(abs) && lstatSync(abs).isSymbolicLink()) fail(`Refusing to use symlink path: ${abs}`);
  if (existsSync(abs) && !lstatSync(abs).isDirectory()) fail('Project root must be a directory');
  // Canonicalize normal OS aliases such as macOS /var -> /private/var, while
  // still rejecting a project root that itself is a symlink.
  return existsSync(abs) ? realpathSync(abs) : abs;
}
function statePaths(root, create = false) {
  const project = safeRoot(root);
  const aidlc = join(project, '.aidlc');
  if (existsSync(aidlc) && (!lstatSync(aidlc).isDirectory() || lstatSync(aidlc).isSymbolicLink())) fail('Refusing unsafe .aidlc path');
  if (create) mkdirSync(aidlc, { recursive: true, mode: 0o700 });
  const repos = join(aidlc, REPOS);
  if (existsSync(repos) && (!lstatSync(repos).isDirectory() || lstatSync(repos).isSymbolicLink())) fail('Refusing unsafe dependency repository directory');
  return { project, aidlc, repos, manifest: join(aidlc, MANIFEST), lock: join(aidlc, LOCK), lockfile: join(aidlc, LOCKFILE) };
}
function validateManifest(data) {
  if (!data || typeof data !== 'object' || Array.isArray(data) || data.schemaVersion !== 1 || Object.keys(data).some(k => !['schemaVersion', 'repos'].includes(k)) || !data.repos || typeof data.repos !== 'object' || Array.isArray(data.repos)) fail('Invalid dependency manifest: expected schemaVersion 1 and repos object');
  for (const [name, entry] of Object.entries(data.repos)) {
    checkName(name);
    if (!entry || typeof entry !== 'object' || Array.isArray(entry) || Object.keys(entry).some(k => !['url', 'ref'].includes(k))) fail(`Invalid dependency manifest entry: ${name}`);
    validateUrl(entry.url);
    checkRef(entry.ref);
  }
  return data;
}
function readManifest(paths) {
  if (!existsSync(paths.manifest)) return { schemaVersion: 1, repos: {} };
  if (lstatSync(paths.manifest).isSymbolicLink() || !lstatSync(paths.manifest).isFile()) fail('Refusing unsafe dependency manifest path');
  let data;
  try { data = JSON.parse(readFileSync(paths.manifest, 'utf8')); } catch { fail('Invalid dependency manifest: malformed JSON'); }
  return validateManifest(data);
}
export function projectDependenciesPath(root) {
  return join(safeRoot(root), 'aidlc.deps.json');
}
function readDeclaredManifest(file) {
  const source = resolve(file);
  if (!existsSync(source) || lstatSync(source).isSymbolicLink() || !lstatSync(source).isFile()) fail(`Dependency file is missing or unsafe: ${source}`);
  let data;
  try { data = JSON.parse(readFileSync(source, 'utf8')); } catch { fail('Invalid dependency manifest: malformed JSON'); }
  return validateManifest(data);
}
export function importDependencies(root, file) {
  const source = resolve(file);
  const data = readDeclaredManifest(source);
  return withLock(root, paths => {
    const runtime = readManifest(paths);
    const dropped = Object.keys(runtime.repos).filter((name) => !Object.hasOwn(data.repos, name)).sort();
    atomicWrite(paths.manifest, `${JSON.stringify(data, null, 2)}\n`);
    return { file: source, repos: Object.keys(data.repos).sort(), dropped };
  });
}
function atomicWrite(file, contents) {
  const temp = `${file}.${process.pid}.${Math.random().toString(16).slice(2)}.tmp`;
  try { writeFileSync(temp, contents, { flag: 'wx', mode: 0o600 }); renameSync(temp, file); }
  catch (error) { try { rmSync(temp, { force: true }); } catch {} throw error; }
}
function pidRunning(pid) {
  try { process.kill(pid, 0); return true; }
  catch (error) { return error.code !== 'ESRCH'; }
}
function lockIsStale(lockPath) {
  let parsed;
  try { parsed = JSON.parse(readFileSync(lockPath, 'utf8')); }
  catch {
    try { return Date.now() - lstatSync(lockPath).mtimeMs > 60_000; }
    catch { return false; }
  }
  return parsed && typeof parsed.pid === 'number' && !pidRunning(parsed.pid);
}
function lockHeldMessage(lockPath) {
  try {
    const data = JSON.parse(readFileSync(lockPath, 'utf8'));
    if (data && typeof data.pid === 'number') {
      return `Dependency manager is locked by process ${data.pid} since ${data.at}. If no dtech-aidlc command is running, delete .aidlc/deps.lock.`;
    }
  } catch { /* unreadable */ }
  return 'Dependency manager is locked by another process';
}
function withLock(root, action) {
  const paths = statePaths(root, true);
  const open = () => openSync(paths.lock, 'wx', 0o600);
  let fd;
  try { fd = open(); }
  catch (error) {
    if (error.code !== 'EEXIST') throw error;
    if (lockIsStale(paths.lock)) {
      rmSync(paths.lock, { force: true });
      try { fd = open(); }
      catch (retry) {
        if (retry.code === 'EEXIST') fail(lockHeldMessage(paths.lock));
        throw retry;
      }
    } else fail(lockHeldMessage(paths.lock));
  }
  try {
    writeFileSync(fd, `${JSON.stringify({ pid: process.pid, at: new Date().toISOString() })}\n`);
    return action(paths);
  } finally {
    closeSync(fd);
    rmSync(paths.lock, { force: true });
  }
}
function scrub(text, url) {
  let value = String(text ?? '');
  if (url) value = value.split(url).join('[repository URL]');
  return value.replace(/(https:\/\/)[^\s/@]+:[^\s/@]+@/gi, '$1[credentials]@');
}
function runGit(paths, args, { url } = {}) {
  const template = join(paths.aidlc, `.git-template-${process.pid}-${Math.random().toString(16).slice(2)}`);
  mkdirSync(template, { mode: 0o700 });
  const env = {
    ...process.env,
    GIT_CONFIG_NOSYSTEM: '1', GIT_CONFIG_GLOBAL: process.platform === 'win32' ? 'NUL' : '/dev/null',
    GIT_TEMPLATE_DIR: template, GIT_TERMINAL_PROMPT: '0', GIT_OPTIONAL_LOCKS: '0',
  };
  delete env.GIT_DIR; delete env.GIT_WORK_TREE; delete env.GIT_CONFIG_COUNT; delete env.GIT_CONFIG_PARAMETERS;
  for (const key of Object.keys(env)) if (/^GIT_CONFIG_(?:KEY|VALUE)_/.test(key)) delete env[key];
  const gitArgs = ['-c', 'core.hooksPath=/dev/null', '-c', 'core.fsmonitor=', '-c', 'core.sshCommand=ssh', '-c', 'core.askPass=', '-c', 'credential.helper=', '-c', 'diff.external=', '-c', 'protocol.ext.allow=never', '-c', 'submodule.recurse=false', '-c', 'core.autocrlf=false', ...args];
  const result = spawnSync('git', gitArgs, { cwd: paths.project, encoding: 'utf8', env, timeout: GIT_TIMEOUT, maxBuffer: 8 * 1024 * 1024, windowsHide: true });
  rmSync(template, { recursive: true, force: true });
  if (result.error || result.status !== 0) {
    const message = result.error?.code === 'ETIMEDOUT' ? 'git command timed out' : scrub(result.stderr || result.error?.message || `git exited ${result.status}`, url);
    throw new Error(message.trim());
  }
  return (result.stdout ?? '').trim();
}
function requireSafeCheckout(paths, name, expectedUrl) {
  const target = join(paths.repos, name);
  if (!existsSync(target)) return { target, exists: false };
  if (lstatSync(target).isSymbolicLink() || !lstatSync(target).isDirectory()) fail(`Refusing unmanaged dependency path: ${name}`);
  const gitDir = join(target, '.git');
  if (!existsSync(gitDir) || lstatSync(gitDir).isSymbolicLink() || !lstatSync(gitDir).isDirectory()) fail(`Refusing unmanaged dependency checkout: ${name}`);
  const actual = realpathSync(target), repoBase = realpathSync(paths.repos);
  const rel = relative(repoBase, actual);
  if (!rel || rel === '..' || rel.startsWith(`..${sep}`) || isAbsolute(rel)) fail(`Dependency checkout escapes managed directory: ${name}`);
  const output = runGit(paths, ['-C', target, 'config', '--local', '--get', 'remote.origin.url'], { url: expectedUrl });
  if (output !== expectedUrl) fail(`Dependency checkout origin does not match manifest: ${name}`);
  const insideGit = runGit(paths, ['-C', target, 'rev-parse', '--show-toplevel']);
  if (realpathSync(insideGit) !== actual) fail(`Dependency checkout has an unexpected Git worktree: ${name}`);
  return { target, exists: true };
}
function checkoutStatus(paths, target) {
  const porcelain = runGit(paths, ['-C', target, 'status', '--porcelain', '--untracked-files=all']);
  if (porcelain) fail('Checkout has dirty or untracked changes; refusing to update');
  return runGit(paths, ['-C', target, 'rev-parse', 'HEAD']);
}
function desiredCommit(paths, target, ref, url) {
  if (ref) {
    const isCommit = /^[0-9a-f]{40,64}$/i.test(ref);
    const isTag = ref.startsWith('refs/tags/');
    if (isCommit || isTag) {
      try { return runGit(paths, ['-C', target, 'rev-parse', '--verify', `${ref}^{commit}`], { url }); }
      catch {
        runGit(paths, ['-C', target, 'fetch', '--no-recurse-submodules', '--no-tags', 'origin', ref], { url });
        return runGit(paths, ['-C', target, 'rev-parse', '--verify', 'FETCH_HEAD^{commit}'], { url });
      }
    }
    // Branch refs are intentionally refreshed; the caller still requires a fast-forward.
    runGit(paths, ['-C', target, 'fetch', '--no-recurse-submodules', '--no-tags', 'origin', ref], { url });
    return runGit(paths, ['-C', target, 'rev-parse', '--verify', 'FETCH_HEAD^{commit}'], { url });
  }
  runGit(paths, ['-C', target, 'fetch', '--no-recurse-submodules', '--no-tags', 'origin', 'HEAD'], { url });
  return runGit(paths, ['-C', target, 'rev-parse', '--verify', 'FETCH_HEAD^{commit}'], { url });
}
function installCheckout(paths, name, url, ref) {
  const { target, exists } = requireSafeCheckout(paths, name, url);
  mkdirSync(paths.repos, { recursive: true, mode: 0o700 });
  if (!exists) {
    if (existsSync(target)) fail(`Refusing to overwrite dependency path: ${name}`);
    const temp = join(paths.repos, `.tmp-${name}-${process.pid}-${Math.random().toString(16).slice(2)}`);
    mkdirSync(temp, { mode: 0o700 });
    try {
      runGit(paths, ['clone', '--no-checkout', '--no-recurse-submodules', '--origin', 'origin', '--', url, temp], { url });
      // A clean, isolated local config means remote attributes cannot activate custom filters.
      const refToResolve = ref ?? 'HEAD';
      let commit;
      if (ref) {
        try { commit = runGit(paths, ['-C', temp, 'rev-parse', '--verify', `${ref}^{commit}`], { url }); }
        catch {
          runGit(paths, ['-C', temp, 'fetch', '--no-recurse-submodules', '--no-tags', 'origin', ref], { url });
          commit = runGit(paths, ['-C', temp, 'rev-parse', '--verify', 'FETCH_HEAD^{commit}'], { url });
        }
      } else commit = runGit(paths, ['-C', temp, 'rev-parse', '--verify', `${refToResolve}^{commit}`], { url });
      runGit(paths, ['-C', temp, 'checkout', '--detach', '--no-recurse-submodules', commit], { url });
      renameSync(temp, target);
      return commit;
    } catch (error) { rmSync(temp, { recursive: true, force: true }); throw error; }
  }
  const current = checkoutStatus(paths, target);
  const desired = desiredCommit(paths, target, ref, url);
  if (desired === current) return current;
  const ancestor = (() => {
    try { runGit(paths, ['-C', target, 'merge-base', '--is-ancestor', current, desired], { url }); return true; }
    catch { return false; }
  })();
  if (!ancestor) fail('Update is not a fast-forward; refusing to change checkout');
  // Immutable object IDs and tags are pins; branch refs may advance, but only via fast-forward.
  if (ref && (/^[0-9a-f]{40,64}$/i.test(ref) || ref.startsWith('refs/tags/'))) return current;
  runGit(paths, ['-C', target, 'checkout', '--detach', '--no-recurse-submodules', desired], { url });
  return desired;
}
function recordFrom(paths, name, entry, commit, status) {
  return { name, url: entry.url, ...(entry.ref === undefined ? {} : { ref: entry.ref }), path: join(paths.repos, name), ...(status === undefined ? {} : { status }), ...(commit === undefined ? {} : { commit }) };
}

function applyToProjectFile(root, mutate) {
  const declared = projectDependenciesPath(root);
  if (!existsSync(declared)) return;
  const data = readDeclaredManifest(declared);
  mutate(data);
  writeFileAtomic(declared, `${JSON.stringify(data, null, 2)}\n`, { mode: 0o644 });
  return data;
}
export function addDependency(root, name, url, ref) {
  checkName(name); validateUrl(url); checkRef(ref);
  return withLock(root, paths => {
    const declared = applyToProjectFile(root, (data) => {
      const current = data.repos[name];
      if (current) {
        if (current.url === url && current.ref === ref) return;
        fail(`Dependency ${name} already exists with a different configuration`);
      }
      data.repos[name] = { url, ...(ref === undefined ? {} : { ref }) };
    });
    const manifest = declared ?? readManifest(paths);
    if (!declared) {
      const current = Object.hasOwn(manifest.repos, name) ? manifest.repos[name] : undefined;
      if (current) {
        if (current.url === url && current.ref === ref) return recordFrom(paths, name, current);
        fail(`Dependency ${name} already exists with a different configuration`);
      }
      manifest.repos[name] = { url, ...(ref === undefined ? {} : { ref }) };
    }
    atomicWrite(paths.manifest, `${JSON.stringify(manifest, null, 2)}\n`);
    return recordFrom(paths, name, manifest.repos[name]);
  });
}
export function listDependencies(root) {
  const paths = statePaths(root), manifest = readManifest(paths);
  const lock = paths.lockfile && existsSync(paths.lockfile) ? (() => { try { return JSON.parse(readFileSync(paths.lockfile, 'utf8')); } catch { return {}; } })() : {};
  return Object.entries(manifest.repos).map(([name, entry]) => {
    const target = join(paths.repos, name);
    let status = 'not-cloned', commit;
    if (existsSync(target)) {
      try {
        if (lstatSync(target).isSymbolicLink() || lstatSync(join(target, '.git')).isSymbolicLink()) status = 'unsafe';
        else { commit = runGit(paths, ['-C', target, 'rev-parse', 'HEAD'], { url: entry.url }); status = 'cloned'; }
      } catch { status = 'invalid'; }
    }
    return recordFrom(paths, name, entry, lock?.repos?.[name]?.commit ?? commit, status);
  });
}
export function pullDependencies(root, name) {
  if (name !== undefined) checkName(name);
  return withLock(root, paths => {
    const manifest = readManifest(paths);
    const names = name === undefined ? Object.keys(manifest.repos).sort() : [name];
    if (name !== undefined && !manifest.repos[name]) fail(`Unknown dependency: ${name}`);
    // Validate every registered path before making network or filesystem changes.
    for (const item of names) requireSafeCheckout(paths, item, manifest.repos[item].url);
    const lock = existsSync(paths.lockfile) ? (() => { try { return JSON.parse(readFileSync(paths.lockfile, 'utf8')); } catch { fail('Invalid dependency lock file'); } })() : { schemaVersion: 1, repos: {} };
    if (!lock || lock.schemaVersion !== 1 || !lock.repos || typeof lock.repos !== 'object') fail('Invalid dependency lock file');
    const results = [];
    for (const item of names) {
      const entry = manifest.repos[item];
      try {
        const commit = installCheckout(paths, item, entry.url, entry.ref);
        lock.repos[item] = { commit };
        results.push({ name: item, ok: true, commit, path: join(paths.repos, item) });
      } catch (error) { results.push({ name: item, ok: false, error: scrub(error.message, entry.url) }); }
    }
    if (results.some(result => result.ok)) atomicWrite(paths.lockfile, `${JSON.stringify(lock, null, 2)}\n`);
    return results;
  });
}
export function removeDependency(root, name) {
  checkName(name);
  return withLock(root, paths => {
    const declaredPath = projectDependenciesPath(root);
    const inDeclared = existsSync(declaredPath) && Object.hasOwn(readDeclaredManifest(declaredPath).repos, name);
    const manifest = readManifest(paths);
    const inRuntime = Object.hasOwn(manifest.repos, name);
    if (!inDeclared && !inRuntime) fail(`Unknown dependency: ${name}`);
    if (inDeclared) applyToProjectFile(root, (data) => { delete data.repos[name]; });
    if (inRuntime) delete manifest.repos[name];
    atomicWrite(paths.manifest, `${JSON.stringify(manifest, null, 2)}\n`);
    if (existsSync(paths.lockfile)) {
      try {
        const lock = JSON.parse(readFileSync(paths.lockfile, 'utf8'));
        if (lock?.schemaVersion === 1 && lock.repos && typeof lock.repos === 'object') {
          delete lock.repos[name];
          atomicWrite(paths.lockfile, `${JSON.stringify(lock, null, 2)}\n`);
        }
      } catch { /* leave an unparseable lock file alone */ }
    }
    return { name, removed: true };
  });
}
export function validateDependencies(root) {
  const paths = statePaths(root), manifest = readManifest(paths), issues = [];
  for (const [name, entry] of Object.entries(manifest.repos)) {
    try { requireSafeCheckout(paths, name, entry.url); }
    catch (error) { issues.push({ name, error: scrub(error.message, entry.url) }); }
  }
  if (issues.length) fail(`Dependency validation failed: ${issues.map(issue => `${issue.name}: ${issue.error}`).join('; ')}`);
  return Object.entries(manifest.repos).map(([name, entry]) => recordFrom(paths, name, entry, undefined, existsSync(join(paths.repos, name)) ? 'cloned' : 'not-cloned'));
}
