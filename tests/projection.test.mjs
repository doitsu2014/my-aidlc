import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { repo, cli, tempProject, run, initProject, json } from './helpers.mjs';
import { compareVersions, hashText, projectionNotices } from '../core/tools/projection.mjs';
import { loadHarnessManifest, listHarnesses } from '../core/tools/lib.mjs';

const manifest = loadHarnessManifest('claude');
const source = (file) => readFileSync(join(repo, 'harness/claude', file));

test('JSON manifests validate and harness folder is the only projected source', () => {
  assert.deepEqual(listHarnesses(), ['claude', 'deepseek']);
  for (const name of listHarnesses()) {
    const loaded = loadHarnessManifest(name);
    assert.equal(loaded.schemaVersion, 1);
    assert.equal(existsSync(join(repo, 'harness', name, 'manifest.mjs')), false);
  }
});

test('init stamps projected files, preserves them on repeat, and reports current status', (t) => {
  const project = tempProject(t), output = initProject(project);
  assert.equal(output.projections[0].state, 'current');
  const stampPath = join(project, manifest.skillDir, '.projection.json'), stat = lstatSync(stampPath), bytes = readFileSync(stampPath);
  assert.equal(stat.mode & 0o777, 0o644);
  const stamp = JSON.parse(bytes), version = spawnSync(process.execPath, [cli, 'version'], { encoding: 'utf8' }).stdout.trim().replace('dtech-aidlc ', '');
  assert.equal(stamp.engineVersion, version);
  assert.equal(stamp.files['SKILL.md'].sha256, createHash('sha256').update(source('skills/aidlc/SKILL.md').toString()).digest('hex'));
  run(project, 'init', '--harness', 'claude', '--scope', 'solo');
  assert.deepEqual(readFileSync(stampPath), bytes);
  assert.equal(json(project, 'harness', 'status').harnesses[0].state, 'current');
  writeFileSync(join(project, manifest.skillDir, 'SKILL.md'), `${source('skills/aidlc/SKILL.md')}\nlocal edit`);
  assert.equal(json(project, 'harness', 'status').harnesses[0].files[0].state, 'modified');
  rmSync(join(project, manifest.skillDir, 'SKILL.md'));
  assert.equal(json(project, 'harness', 'status').harnesses[0].files[0].state, 'missing');
  rmSync(stampPath);
  assert.equal(run(project, 'onboard', '--harness', 'claude', '--scope', 'solo').status, 0);
  assert.deepEqual(readFileSync(join(project, manifest.skillDir, 'SKILL.md')), source('skills/aidlc/SKILL.md'));
  assert.ok(existsSync(stampPath));
});

test('init rejects symlinked projection parents before creating .aidlc', (t) => {
  const project = tempProject(t), outside = tempProject(t);
  symlinkSync(outside, join(project, '.claude'));
  const result = run(project, 'init', '--harness', 'claude', '--scope', 'solo');
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Refusing to write through symlink/);
  assert.equal(existsSync(join(project, '.aidlc')), false);
  assert.deepEqual(readdirSync(outside), []);
});

test('refresh keeps or force-backs up edits and refuses unsafe downgrades', (t) => {
  const project = tempProject(t); initProject(project);
  const skill = join(project, manifest.skillDir, 'SKILL.md');
  writeFileSync(skill, `${readFileSync(skill, 'utf8')}\nuser`);
  const before = readFileSync(skill), keep = json(project, 'harness', 'refresh');
  assert.ok(keep.needsAttention);
  assert.equal(keep.harnesses[0].actions[0].action, 'kept-modified');
  assert.deepEqual(readFileSync(skill), before);
  assert.equal(json(project, 'harness', 'refresh', '--force').harnesses[0].actions[0].action, 'replaced-with-backup');
  assert.match(readFileSync(`${skill}.local-backup`, 'utf8'), /user/);
  assert.deepEqual(readFileSync(skill), source('skills/aidlc/SKILL.md'));
  const path = join(project, manifest.skillDir, '.projection.json'), stamp = JSON.parse(readFileSync(path));
  stamp.engineVersion = '99.0.0'; writeFileSync(path, JSON.stringify(stamp, null, 2) + '\n');
  writeFileSync(skill, 'different');
  assert.equal(json(project, 'harness', 'status').harnesses[0].state, 'ahead');
  assert.equal(run(project, 'harness', 'refresh').status, 1);
  assert.equal(run(project, 'harness', 'refresh', '--force').status, 0);
});

test('refresh dry-run is read-only; option order and symlink target protections hold', (t) => {
  const project = tempProject(t); initProject(project);
  const skill = join(project, manifest.skillDir, 'SKILL.md'); writeFileSync(skill, 'modified');
  const original = readFileSync(skill);
  for (const args of [['--dry-run', 'claude'], ['claude', '--dry-run']]) {
    assert.equal(json(project, 'harness', 'refresh', ...args).dryRun, true);
    assert.deepEqual(readFileSync(skill), original);
  }
  assert.match(run(project, 'harness', 'refresh', '--dry-run=yes').stderr, /Option --dry-run does not take a value/);
  rmSync(skill); symlinkSync(join(project, 'elsewhere'), skill); writeFileSync(join(project, 'elsewhere'), 'target');
  assert.equal(run(project, 'harness', 'refresh').status, 1);
  assert.equal(run(project, 'harness', 'refresh', '--force').status, 1);
  assert.equal(readFileSync(join(project, 'elsewhere'), 'utf8'), 'target');
});

test('unstamped historic projections and obsolete files refresh only known content', (t) => {
  const project = tempProject(t), oldPath = join(project, '.claude/skills/aidlc');
  mkdirSync(oldPath, { recursive: true });
  writeFileSync(join(oldPath, 'SKILL.md'), source('skills/aidlc/SKILL.md'));
  writeFileSync(join(oldPath, 'local-note.txt'), 'keep');
  initProject(project);
  const refreshed = json(project, 'harness', 'refresh');
  assert.ok(refreshed.harnesses[0].actions.some(row => row.path === '.claude/skills/aidlc/SKILL.md' && row.action === 'removed'));
  assert.equal(existsSync(join(oldPath, 'local-note.txt')), true);
  assert.equal(existsSync(join(oldPath, 'SKILL.md')), false);
  assert.equal(existsSync(join(project, manifest.skillDir, 'manifest.mjs')), false);
});

test('projection notices describe configured projection states and tolerate unreadable files', (t) => {
  const project = tempProject(t);
  assert.deepEqual(projectionNotices(project, '1.0.0'), []);
  initProject(project);
  const notices = projectionNotices(project, '1.0.0');
  assert.deepEqual(notices, []);
  const skill = join(project, manifest.skillDir, 'SKILL.md'), stampPath = join(project, manifest.skillDir, '.projection.json');
  const stamp = JSON.parse(readFileSync(stampPath, 'utf8'));
  stamp.engineVersion = '1.0.0';
  stamp.files['SKILL.md'].sha256 = hashText('prior projected text');
  writeFileSync(stampPath, JSON.stringify(stamp));
  writeFileSync(skill, 'prior projected text');
  assert.match(projectionNotices(project, '1.1.0')[0], /is behind relative to engine 1.1.0/);
  writeFileSync(skill, 'local modification');
  assert.match(projectionNotices(project, '1.1.0')[0], /is modified relative to engine 1.1.0/);
  stamp.engineVersion = '2.0.0';
  writeFileSync(stampPath, JSON.stringify(stamp));
  assert.match(projectionNotices(project, '1.1.0')[0], /was written by engine 2.0.0/);
  writeFileSync(skill, 'current text');
  writeFileSync(skill, readFileSync(join(repo, 'harness/claude/skills/aidlc/SKILL.md')));
  assert.deepEqual(projectionNotices(project, '1.1.0'), []);
  stamp.engineVersion = '1.0.0';
  writeFileSync(stampPath, JSON.stringify(stamp));
  rmSync(skill);
  rmSync(join(project, manifest.skillDir), { recursive: true });
  symlinkSync(tempProject(t), join(project, manifest.skillDir));
  assert.deepEqual(projectionNotices(project, '1.1.0'), []);
});

test('doctor validates configured skill projections and warns about PATH', (t) => {
  const project = tempProject(t);
  const initialized = initProject(project);
  const doctor = () => run(project, 'doctor');
  let result = doctor(), report = JSON.parse(result.stdout);
  const projection = report.checks.findIndex((row) => row.message.startsWith('Projection claude:'));
  const config = report.checks.findIndex((row) => row.message === 'Project configuration and agent models');
  const dependencies = report.checks.findIndex((row) => row.message.startsWith('Dependency repositories:'));
  assert.ok(projection > config && projection < dependencies);
  assert.ok(report.checks.some((row) => row.status === 'pass' && row.message.startsWith('Projection claude: current')));
  assert.ok(report.checks.some((row) => row.status === 'warn' && row.message.includes('not on PATH')));
  assert.equal(report.ok, true);

  const skill = join(project, manifest.skillDir, 'SKILL.md');
  writeFileSync(skill, `${readFileSync(skill, 'utf8')}\\nlocal edit`);
  report = JSON.parse(doctor().stdout);
  assert.ok(report.checks.some((row) => row.status === 'warn' && row.message.includes('.claude/skills/dtech-aidlc/SKILL.md')));
  const stampPath = join(project, manifest.skillDir, '.projection.json'), stamp = JSON.parse(readFileSync(stampPath));
  stamp.files['SKILL.md'].sha256 = hashText(readFileSync(skill));
  stamp.engineVersion = '0.0.0'; writeFileSync(stampPath, JSON.stringify(stamp));
  report = JSON.parse(doctor().stdout);
  assert.ok(report.checks.some((row) => row.status === 'warn' && row.message.includes('behind engine') && row.message.includes('harness refresh')));
  stamp.engineVersion = '99.0.0'; writeFileSync(stampPath, JSON.stringify(stamp));
  report = JSON.parse(doctor().stdout);
  assert.ok(report.checks.some((row) => row.status === 'warn' && row.message.includes('newer than the installed')));

  rmSync(join(project, '.claude'), { recursive: true, force: true });
  result = doctor(); report = JSON.parse(result.stdout);
  assert.equal(result.status, 1);
  assert.equal(report.ok, false);
  assert.ok(report.checks.some((row) => row.status === 'fail' && row.message.includes('.claude/skills/dtech-aidlc/SKILL.md is missing')));
  assert.ok(report.checks.some((row) => row.status === 'fail' && row.message.includes('.claude/skills/dtech-aidlc/onboarding.md is missing')));

  const emptyPath = mkdtempSync(join(tmpdir(), 'aidlc-empty-path-'));
  t.after(() => rmSync(emptyPath, { recursive: true, force: true }));
  const noPath = spawnSync(process.execPath, [cli, 'doctor', '--project', project], { encoding: 'utf8', env: { ...process.env, PATH: emptyPath } });
  assert.ok(JSON.parse(noPath.stdout).checks.some((row) => row.status === 'warn' && row.message.includes('not on PATH')));
});

test('refresh removes only history-confirmed legacy files and keeps modified or unknown files', (t) => {
  const project = tempProject(t); initProject(project);
  const legacy = join(project, '.aidlc/methodology');
  mkdirSync(join(legacy, 'agents'), { recursive: true });
  const history = JSON.parse(readFileSync(join(repo, 'core/data/projection-history.json'), 'utf8')).methodology;
  const pristinePath = 'agents/brief-analyst.md';
  const pristineHash = history[pristinePath][0];
  const blob = '93ca303542e01c266ec30debb54e08f3abf4009d';
  const pristineBytes = spawnSync('git', ['cat-file', 'blob', blob], { cwd: repo }).stdout;
  assert.equal(createHash('sha256').update(pristineBytes.toString().replaceAll('\\r\\n', '\\n')).digest('hex'), pristineHash);
  writeFileSync(join(legacy, pristinePath), pristineBytes);
  writeFileSync(join(legacy, 'agents/edited.md'), 'edited');
  writeFileSync(join(legacy, 'agents/unknown.md'), 'unknown');
  const target = join(project, 'outside-note'); writeFileSync(target, 'outside');
  symlinkSync(target, join(legacy, 'agents/link.md'));
  const dryRun = json(project, 'harness', 'refresh', '--dry-run');
  assert.deepEqual(dryRun.legacyMethodology.removed, [pristinePath]);
  assert.deepEqual(dryRun.legacyMethodology.keptModified, ['agents/edited.md', 'agents/link.md', 'agents/unknown.md']);
  assert.equal(dryRun.needsAttention, true);
  assert.equal(existsSync(join(legacy, pristinePath)), true);
  assert.equal(readFileSync(join(project, '.aidlc/audit.jsonl'), 'utf8').includes('methodology.legacy.cleaned'), false);
  const result = json(project, 'harness', 'refresh');
  assert.deepEqual(result.legacyMethodology.removed, [pristinePath]);
  assert.deepEqual(result.legacyMethodology.keptModified, ['agents/edited.md', 'agents/link.md', 'agents/unknown.md']);
  assert.equal(result.needsAttention, true);
  assert.equal(existsSync(join(legacy, pristinePath)), false);
  assert.equal(readFileSync(join(legacy, 'agents/edited.md'), 'utf8'), 'edited');
  assert.equal(readFileSync(join(legacy, 'agents/unknown.md'), 'utf8'), 'unknown');
  assert.equal(lstatSync(join(legacy, 'agents/link.md')).isSymbolicLink(), true);
  assert.equal(readFileSync(target, 'utf8'), 'outside');
  const audit = JSON.parse(readFileSync(join(project, '.aidlc/audit.jsonl'), 'utf8').trim().split(/\r?\n/).at(-1));
  assert.equal(audit.event, 'methodology.legacy.cleaned');
  assert.deepEqual(audit.removed, [pristinePath]);
});

test('legacy methodology status reports unknown entries and refresh dry-run preserves them', (t) => {
  const project = tempProject(t); initProject(project);
  const legacy = join(project, '.aidlc/methodology');
  mkdirSync(join(legacy, 'agents'), { recursive: true });
  writeFileSync(join(legacy, 'agents/local.md'), 'project-owned');
  const status = json(project, 'harness', 'status').legacyMethodology;
  assert.deepEqual(status, { pristine: [], modified: ['agents/local.md'] });
  const refreshed = json(project, 'harness', 'refresh', '--dry-run');
  assert.deepEqual(refreshed.legacyMethodology, { removed: [], keptModified: ['agents/local.md'] });
  assert.equal(refreshed.needsAttention, true);
  assert.equal(readFileSync(join(legacy, 'agents/local.md'), 'utf8'), 'project-owned');
  const report = json(project, 'doctor');
  assert.equal(report.checks.at(-1).status, 'warn');
  assert.match(report.checks.at(-1).message, /Unused methodology copy at \.aidlc\/methodology/);
  assert.match(report.checks.at(-1).message, /1 modified or unknown file\(s\) will be kept/);
});

test('doctor skips projection and PATH rows without configured harnesses', (t) => {
  const project = tempProject(t), report = json(project, 'doctor');
  assert.equal(report.checks.some((row) => String(row.message).startsWith('Projection ')), false);
  assert.equal(report.checks.some((row) => String(row.message).includes('not on PATH')), false);
});

test('doctor reports a symlinked projection as a failed row without aborting checks', (t) => {
  const project = tempProject(t); initProject(project);
  const outside = tempProject(t), skill = join(project, manifest.skillDir, 'SKILL.md');
  rmSync(skill); symlinkSync(join(outside, 'missing'), skill);
  const result = run(project, 'doctor'), report = JSON.parse(result.stdout);
  assert.equal(result.status, 1);
  assert.ok(report.checks.some((row) => row.status === 'fail' && row.message.startsWith('Projection claude:')));
  assert.ok(report.checks.some((row) => row.message.startsWith('Dependency repositories:')));
});

test('history has the required hashes and hash/version helpers normalize correctly', () => {
  const history = JSON.parse(readFileSync(join(repo, 'core/data/projection-history.json'), 'utf8'));
  assert.ok(history.source.startsWith('git 91c9eea..'));
  const expected = ['1b6aebe53308e796d0ac355ae067c142e8cdd220037ffa6e0c4cfdbb8154f637', '89b889881b09d4c665bef9a0accdfa85e20e5ef81cb6bdfaed10780a16c95b77', 'cefb675bfd8665a0ad7be8e8ee36fb755b52e5b25f9673682d53fc870bf44199'];
  assert.ok(expected.every(hash => history.harness.claude['SKILL.md'].includes(hash)));
  assert.ok(Object.keys(history.methodology).every(path => !path.startsWith('/') && !path.split('/').includes('..')));
  assert.equal(hashText('a\r\nb'), hashText('a\nb'));
  assert.ok(compareVersions('0.2.0', '0.10.0') < 0);
  assert.equal(compareVersions('bad', '0.0.0'), 0);
});
