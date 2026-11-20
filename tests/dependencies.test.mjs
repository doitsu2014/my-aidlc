import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { addDependency, listDependencies, pullDependencies, removeDependency, validateDependencies } from '../core/tools/dependencies.mjs';
import { utimesSync } from 'node:fs';

function temp(t) {
  const dir = mkdtempSync(join(tmpdir(), 'aidlc-deps-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  return dir;
}
function git(cwd, ...args) {
  const result = spawnSync('git', args, { cwd, encoding: 'utf8', env: { ...process.env, GIT_CONFIG_NOSYSTEM: '1', GIT_CONFIG_GLOBAL: '/dev/null' } });
  assert.equal(result.status, 0, result.stderr);
  return result.stdout.trim();
}
function fixture(base, name = 'source') {
  const bare = join(base, `${name}.git`), work = join(base, `${name}-work`);
  mkdirSync(work); git(work, 'init', '-q', '-b', 'main');
  git(work, 'config', 'user.email', 'test@example.invalid'); git(work, 'config', 'user.name', 'Test');
  writeFileSync(join(work, 'content.txt'), 'one\n'); git(work, 'add', '.'); git(work, 'commit', '-qm', 'initial');
  const first = git(work, 'rev-parse', 'HEAD');
  git(base, 'clone', '-q', '--bare', work, bare);
  return { bare, work, first };
}
function commit(work, content) {
  writeFileSync(join(work, 'content.txt'), `${content}\n`); git(work, 'add', '.'); git(work, 'commit', '-qm', content); return git(work, 'rev-parse', 'HEAD');
}

test('registers multiple repositories idempotently and pulls each without workflow state', (t) => {
  const root = temp(t), fixtureA = fixture(root, 'a'), fixtureB = fixture(root, 'b');
  const a = addDependency(root, 'alpha', fixtureA.bare);
  assert.equal(a.name, 'alpha');
  addDependency(root, 'alpha', fixtureA.bare);
  addDependency(root, 'beta', fixtureB.bare, 'main');
  assert.equal(listDependencies(root).length, 2);
  const result = pullDependencies(root);
  assert.deepEqual(result.map(x => x.ok), [true, true]);
  assert.equal(readFileSync(join(root, '.aidlc/repos/alpha/content.txt'), 'utf8'), 'one\n');
  assert.equal(validateDependencies(root).length, 2);
  assert.equal(JSON.parse(readFileSync(join(root, '.aidlc/deps-lock.json'))).repos.alpha.commit, fixtureA.first);
});

test('updates default branch fast-forward and pinned refs do not move checkout', (t) => {
  const root = temp(t), repo = fixture(root);
  addDependency(root, 'repo', repo.bare);
  assert.equal(pullDependencies(root, 'repo')[0].ok, true);
  const second = commit(repo.work, 'two'); git(repo.work, 'push', '-q', repo.bare, 'main');
  assert.equal(pullDependencies(root, 'repo')[0].commit, second);
  const pinnedRoot = temp(t);
  addDependency(pinnedRoot, 'repo', repo.bare, repo.first);
  assert.equal(pullDependencies(pinnedRoot, 'repo')[0].commit, repo.first);
  commit(repo.work, 'three'); git(repo.work, 'push', '-q', repo.bare, 'main');
  assert.equal(pullDependencies(pinnedRoot, 'repo')[0].commit, repo.first);
});

test('dirty checkouts, divergent updates, and clone errors preserve content and report failures', (t) => {
  const root = temp(t), repo = fixture(root), badRoot = temp(t);
  addDependency(root, 'repo', repo.bare); pullDependencies(root, 'repo');
  const checkout = join(root, '.aidlc/repos/repo');
  writeFileSync(join(checkout, 'content.txt'), 'local edit\n');
  const dirty = pullDependencies(root, 'repo');
  assert.equal(dirty[0].ok, false); assert.match(dirty[0].error, /dirty/);
  assert.equal(readFileSync(join(checkout, 'content.txt'), 'utf8'), 'local edit\n');
  writeFileSync(join(checkout, 'content.txt'), 'one\n');
  git(checkout, 'config', 'user.email', 'test@example.invalid'); git(checkout, 'config', 'user.name', 'Test');
  const localCommit = commit(checkout, 'local');
  commit(repo.work, 'remote'); git(repo.work, 'push', '-q', repo.bare, 'main');
  const divergence = pullDependencies(root, 'repo');
  assert.equal(divergence[0].ok, false); assert.match(divergence[0].error, /fast-forward/);
  assert.equal(git(checkout, 'rev-parse', 'HEAD'), localCommit);
  addDependency(badRoot, 'broken', join(badRoot, 'missing.git'));
  const failures = pullDependencies(badRoot);
  assert.equal(failures.length, 1); assert.equal(failures[0].ok, false);
});

test('removal retains the checkout and malformed manifests are rejected without overwrite', (t) => {
  const root = temp(t), repo = fixture(root);
  addDependency(root, 'repo', repo.bare); pullDependencies(root, 'repo');
  const checkout = join(root, '.aidlc/repos/repo');
  assert.deepEqual(removeDependency(root, 'repo'), { name: 'repo', removed: true });
  assert.equal(existsSync(checkout), true);
  writeFileSync(join(root, '.aidlc/deps.json'), '{broken');
  assert.throws(() => addDependency(root, 'other', repo.bare), /malformed JSON/);
  assert.equal(readFileSync(join(root, '.aidlc/deps.json'), 'utf8'), '{broken');
});

test('rejects unsafe names, URLs, refs, and symlink-managed paths', (t) => {
  const root = temp(t), repo = fixture(root);
  for (const name of ['../escape', 'bad/name', '-option']) assert.throws(() => addDependency(root, name, repo.bare), /Invalid dependency name/);
  for (const url of ['relative.git', 'https://user:secret@example.invalid/repo', 'file:///etc/passwd', 'ext::sh -c nope']) assert.throws(() => addDependency(root, 'repo', url), /Invalid repository URL/);
  assert.throws(() => addDependency(root, 'repo', repo.bare, 'refs/heads/../main'), /Invalid dependency ref/);
  const outside = join(root, 'outside'); mkdirSync(outside);
  const linked = join(root, 'linked');
  try { symlinkSync(outside, linked); }
  catch (error) { if (error.code === 'EPERM' && process.platform === 'win32') return; throw error; }
  assert.throws(() => addDependency(linked, 'repo', repo.bare), /symlink/);
  const stateRoot = temp(t), externalRepos = join(stateRoot, 'outside-repos'); mkdirSync(externalRepos);
  mkdirSync(join(stateRoot, '.aidlc')); symlinkSync(externalRepos, join(stateRoot, '.aidlc/repos'));
  assert.throws(() => pullDependencies(stateRoot), /unsafe dependency repository directory/);
});

test('git hooks and global init templates are not executed during checkout', (t) => {
  const root = temp(t), repo = fixture(root), template = join(root, 'template'), marker = join(root, 'ran');
  mkdirSync(join(template, 'hooks'), { recursive: true });
  writeFileSync(join(template, 'hooks', 'post-checkout'), `#!/bin/sh\ntouch '${marker}'\n`, { mode: 0o755 });
  const result = spawnSync(process.execPath, ['--input-type=module', '-e', `import {addDependency,pullDependencies} from ${JSON.stringify(new URL('../core/tools/dependencies.mjs', import.meta.url).href)}; const r=${JSON.stringify(root)}; addDependency(r,'repo',${JSON.stringify(repo.bare)}); pullDependencies(r);`], {
    cwd: root, encoding: 'utf8', env: { ...process.env, GIT_TEMPLATE_DIR: template },
  });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(existsSync(marker), false);
  assert.equal(lstatSync(join(root, '.aidlc/repos/repo/.git')).isDirectory(), true);
});

test('checkout config and inherited Git config cannot execute commands', (t) => {
  const root = temp(t), repo = fixture(root), marker = join(root, 'executed');
  const script = join(root, 'marker.sh');
  writeFileSync(script, `#!/bin/sh\ntouch '${marker}'\n`, { mode: 0o755 });
  addDependency(root, 'repo', repo.bare);
  assert.equal(pullDependencies(root, 'repo')[0].ok, true);
  git(join(root, '.aidlc/repos/repo'), 'config', 'core.fsmonitor', script);
  assert.equal(pullDependencies(root, 'repo')[0].ok, true);
  assert.equal(existsSync(marker), false);
  const inherited = spawnSync(process.execPath, ['--input-type=module', '-e', `import {addDependency,pullDependencies} from ${JSON.stringify(new URL('../core/tools/dependencies.mjs', import.meta.url).href)}; const r=${JSON.stringify(root)}; addDependency(r,'inherited',${JSON.stringify(repo.bare)}); if (!pullDependencies(r,'inherited')[0].ok) process.exit(2);`], {
    encoding: 'utf8', env: { ...process.env, GIT_CONFIG_PARAMETERS: `'core.fsmonitor=${script}'` },
  });
  assert.equal(inherited.status, 0, inherited.stderr);
  assert.equal(existsSync(marker), false);
});

test('removeDependency drops the lock-file entry', (t) => {
  const root = temp(t), repo = fixture(root);
  addDependency(root, 'repo', repo.bare);
  assert.equal(pullDependencies(root, 'repo')[0].ok, true);
  removeDependency(root, 'repo');
  const lock = JSON.parse(readFileSync(join(root, '.aidlc/deps-lock.json'), 'utf8'));
  assert.equal(lock.repos.repo, undefined);
});

test('a stale lock with a dead pid is recovered', (t) => {
  const root = temp(t), repo = fixture(root);
  const child = spawnSync(process.execPath, ['-e', 'process.exit(0)'], { encoding: 'utf8' });
  mkdirSync(join(root, '.aidlc'), { recursive: true });
  writeFileSync(join(root, '.aidlc/deps.lock'), `${JSON.stringify({ pid: child.pid || 1, at: new Date().toISOString() })}\n`);
  addDependency(root, 'repo', repo.bare);
  assert.equal(existsSync(join(root, '.aidlc/deps.lock')), false);
});

test('an unreadable old lock is recovered', (t) => {
  const root = temp(t), repo = fixture(root);
  mkdirSync(join(root, '.aidlc'), { recursive: true });
  const lock = join(root, '.aidlc/deps.lock');
  writeFileSync(lock, 'not-json');
  const old = new Date(Date.now() - 120_000);
  utimesSync(lock, old, old);
  addDependency(root, 'repo', repo.bare);
  assert.equal(existsSync(lock), false);
});

test('a live lock is refused', (t) => {
  const root = temp(t), repo = fixture(root);
  mkdirSync(join(root, '.aidlc'), { recursive: true });
  writeFileSync(join(root, '.aidlc/deps.lock'), `${JSON.stringify({ pid: process.pid, at: new Date().toISOString() })}\n`);
  assert.throws(() => addDependency(root, 'repo', repo.bare), /locked by process/);
  assert.equal(existsSync(join(root, '.aidlc/deps.lock')), true);
});

test('prototype property names are valid dependency names', (t) => {
  const root = temp(t), repo = fixture(root);
  assert.equal(addDependency(root, 'constructor', repo.bare).name, 'constructor');
  assert.equal(pullDependencies(root, 'constructor')[0].ok, true);
  assert.equal(readFileSync(join(root, '.aidlc/repos/constructor/content.txt'), 'utf8'), 'one\n');
});

test('ref-less dependency stays on the remote default branch when other branches exist', (t) => {
  const root = temp(t), repo = fixture(root);
  git(repo.work, 'checkout', '-q', '-b', 'a-feature');
  commit(repo.work, 'feature'); git(repo.work, 'push', '-q', repo.bare, 'a-feature');
  git(repo.work, 'checkout', '-q', '--orphan', 'aa-orphan');
  commit(repo.work, 'orphan'); git(repo.work, 'push', '-q', repo.bare, 'aa-orphan');
  git(repo.work, 'checkout', '-q', 'main');
  addDependency(root, 'repo', repo.bare);
  const first = pullDependencies(root, 'repo')[0];
  assert.equal(first.commit, repo.first);
  const second = pullDependencies(root, 'repo')[0];
  assert.equal(second.ok, true);
  assert.equal(second.commit, repo.first);
  assert.equal(readFileSync(join(root, '.aidlc/repos/repo/content.txt'), 'utf8'), 'one\n');
  const next = commit(repo.work, 'two'); git(repo.work, 'push', '-q', repo.bare, 'main');
  assert.equal(pullDependencies(root, 'repo')[0].commit, next);
});
