import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { json, repo, run, runRaw, tempProject } from './helpers.mjs';

function write(project, path, content) {
  const target = join(project, path);
  mkdirSync(join(target, '..'), { recursive: true });
  writeFileSync(target, content);
}

function manifestPath(project) {
  return join(project, 'aidlc.knowledge.json');
}

test('existing documentation reaches every directive and can be corrected', (t) => {
  const project = tempProject(t, 'aidlc-brownfield-e2e-');
  const files = {
    'package.json': '{"name":"stock-service"}\n',
    'src/app.ts': 'export const app = true;\n',
    'README.md': '# Stock service\n',
    'docs/architecture.md': '# Architecture\n',
    'docs/adr/0001-use-postgres.md': '# ADR 0001\n',
    'docs/api/openapi.yaml': 'openapi: 3.0.0\n',
    'docs/runbook.md': '# Runbook\n',
    'docs/BRD.md': '# Requirements\n',
    'requirements.txt': 'flask==3.0\n',
    'vendor/lib/x.go': 'package lib\n',
    'PROJECT.md': '# Project\n\n## Conventions\n\nUse TypeScript strict mode.\n',
  };
  for (const [path, content] of Object.entries(files)) write(project, path, content);

  const onboardResult = run(project, 'onboard', '--harness', 'claude', '--scope', 'solo');
  assert.equal(onboardResult.status, 0, onboardResult.stderr || onboardResult.stdout);
  const onboard = JSON.parse(onboardResult.stdout);
  assert.equal(onboard.kind, 'brownfield');
  assert.equal(onboard.knowledge.manifestStatus, 'created');
  assert.equal(onboard.knowledge.project.created, false);
  const manifestBytes = readFileSync(manifestPath(project));
  const manifest = JSON.parse(manifestBytes);
  assert.deepEqual(manifest.sources.map(({ path, kind }) => [path, kind]), [
    ['PROJECT.md', 'project'], ['README.md', 'readme'], ['docs', 'docs'],
    ['docs/BRD.md', 'requirements'], ['docs/adr', 'adr'],
    ['docs/api/openapi.yaml', 'api-spec'], ['docs/architecture.md', 'architecture'],
    ['docs/runbook.md', 'runbook'],
  ]);
  assert.deepEqual(readFileSync(join(project, 'PROJECT.md')), Buffer.from(files['PROJECT.md']));
  assert.equal(existsSync(join(project, '.aidlc/shared/brd')), false);
  assert.equal(manifest.sources.some(({ path }) => path.includes('requirements.txt')), false);
  assert.equal(JSON.stringify(JSON.parse(readFileSync(join(project, '.aidlc/shared/project-profile.json'), 'utf8')).languages).includes('vendor/'), false);
  assert.equal(existsSync(join(project, '.aidlc/shared/sources-snapshot.json')), true);

  const next = json(project, 'orchestrate', 'next', '--intent', 'Add stock reservation endpoint');
  assert.equal(next.context.project.path, 'PROJECT.md');
  assert.equal(next.context.project.status, 'ok');
  assert.deepEqual(next.context.sources.map(({ path, kind }) => [path, kind]), manifest.sources.slice(1).map(({ path, kind }) => [path, kind]));
  assert.equal(next.context.codebaseMap.required, true);
  assert.equal(next.context.codebaseMap.exists, false);
  assert.equal(next.context.codebaseMap.path, '.aidlc/knowledge/_codebase-map.md');
  assert.equal(next.context.sourceChanges.scanned, true);
  for (const key of ['changed', 'added', 'removed', 'missing']) assert.deepEqual(next.context.sourceChanges[key], []);
  assert.equal(next.context.warnings.some((warning) => /changed since the last scan|aidlc\.knowledge\.json/.test(warning)), false);

  manifest.sources = manifest.sources.filter(({ path }) => path !== 'docs/runbook.md');
  manifest.sources.find(({ path }) => path === 'docs/architecture.md').kind = 'docs';
  writeFileSync(manifestPath(project), `${JSON.stringify(manifest, null, 2)}\n`);
  const corrected = json(project, 'orchestrate', 'next');
  assert.equal(corrected.context.sources.some(({ path }) => path === 'docs/runbook.md'), false);
  assert.equal(corrected.context.sources.find(({ path }) => path === 'docs/architecture.md').kind, 'docs');

  write(project, 'docs/architecture.md', `${files['docs/architecture.md']}Updated.\n`);
  const manifestBeforeRescan = readFileSync(manifestPath(project));
  const stale = json(project, 'orchestrate', 'next');
  assert.ok(stale.context.sourceChanges.changed.includes('docs/architecture.md'));
  assert.ok(stale.context.warnings.some((warning) => warning.includes('docs/architecture.md')));
  const rescan = json(project, 'onboard', '--rescan');
  assert.ok(rescan.changesSinceLastScan.changed.includes('docs/architecture.md'));
  const scanned = json(project, 'orchestrate', 'next');
  assert.equal(scanned.context.warnings.some((warning) => warning.includes('changed since the last scan')), false);
  assert.deepEqual(readFileSync(manifestPath(project)), manifestBeforeRescan);

  write(project, '.aidlc/knowledge/_codebase-map.md', '# Map\n');
  const mapped = json(project, 'orchestrate', 'next');
  assert.equal(mapped.context.codebaseMap.exists, true);
  assert.equal(json(project, 'knowledge', 'list').records.some(({ path }) => path.includes('_codebase-map')), false);

  const beforeOnboard = readFileSync(manifestPath(project));
  const dry = json(project, 'onboard');
  assert.equal(dry.knowledge.manifestStatus, 'unchanged');
  assert.ok(dry.knowledge.wouldAdd.some(({ path }) => path === 'docs/runbook.md'));
  assert.equal(runRaw(project, 'version').stdout.trim(), `dtech-aidlc ${JSON.parse(readFileSync(join(repo, 'package.json'), 'utf8')).version}`);
  assert.deepEqual(readFileSync(manifestPath(project)), beforeOnboard);
  const repeated = json(project, 'onboard', '--harness', 'claude', '--scope', 'solo');
  assert.equal(repeated.knowledge.manifestStatus, 'unchanged');
  assert.ok(repeated.knowledge.wouldAdd.some(({ path }) => path === 'docs/runbook.md'));
  assert.deepEqual(readFileSync(manifestPath(project)), beforeOnboard);
});

test('a project initialized by an earlier version keeps working and is told the exact command to run', (t) => {
  const project = tempProject(t, 'aidlc-brownfield-legacy-');
  write(project, 'docs/a.md', '# Existing docs\n');
  write(project, 'PROJECT.md', '# Project\n\nExisting project conventions.\n');
  json(project, 'init', '--harness', 'deepseek', '--scope', 'team');
  rmSync(manifestPath(project));
  rmSync(join(project, '.aidlc/shared'), { recursive: true, force: true });

  const legacy = json(project, 'orchestrate', 'next', '--intent', 'x');
  const warning = legacy.context.warnings.filter((item) => item.includes('aidlc.knowledge.json'));
  assert.equal(warning.length, 1);
  assert.ok(warning[0].includes('dtech-aidlc onboard --harness deepseek --scope team'));
  assert.equal(json(project, 'doctor').ok, true);
  const onboardResult = run(project, 'onboard', '--harness', 'deepseek', '--scope', 'team');
  assert.equal(onboardResult.status, 0, onboardResult.stderr || onboardResult.stdout);
  const onboard = JSON.parse(onboardResult.stdout);
  assert.equal(onboard.knowledge.manifestStatus, 'created');
  assert.ok(JSON.parse(readFileSync(manifestPath(project), 'utf8')).sources.some(({ path, kind }) => path === 'docs' && kind === 'docs'));
  const current = json(project, 'orchestrate', 'next');
  assert.equal(current.context.warnings.some((item) => /aidlc\.knowledge\.json|not been scanned/.test(item)), false);
});
