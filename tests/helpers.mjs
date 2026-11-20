import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync, existsSync } from 'node:fs';
import { dirname } from 'node:path';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

export const repo = resolve(import.meta.dirname, '..');
export const cli = join(repo, 'core/tools/dtech-aidlc.mjs');

export function tempProject(t, prefix = 'aidlc-test-') {
  const project = mkdtempSync(join(tmpdir(), prefix));
  t.after(() => rmSync(project, { recursive: true, force: true }));
  return project;
}

export function run(project, ...args) {
  return spawnSync(process.execPath, [cli, ...args, '--project', project], { cwd: project, encoding: 'utf8' });
}

export function runRaw(cwd, ...args) {
  return spawnSync(process.execPath, [cli, ...args], { cwd, encoding: 'utf8' });
}

export function json(project, ...args) {
  const result = run(project, ...args);
  assert.equal(result.status, 0, result.stderr || result.stdout);
  return JSON.parse(result.stdout);
}

export function runWithStdin(project, text, ...args) {
  return spawnSync(process.execPath, [cli, ...args, '--project', project], { cwd: project, encoding: 'utf8', input: text });
}

export function initProject(project, scope = 'solo', harness = 'claude') {
  const result = run(project, 'init', '--harness', harness, '--scope', scope);
  assert.equal(result.status, 0, result.stderr || result.stdout);
  return JSON.parse(result.stdout);
}

export function writeArtifacts(project, directive) {
  for (const item of directive.artifacts ?? []) {
    const path = join(project, item.path);
    mkdirSync(dirname(path), { recursive: true });
    if (!existsSync(path)) writeFileSync(path, `# ${item.output}\n\nDraft.\n`);
  }
}

export function completeStage(project, extraReportArgs = []) {
  const next = json(project, 'orchestrate', 'next');
  if (next.complete) return next;
  writeArtifacts(project, next);
  let report = run(project, 'orchestrate', 'report', '--result', 'complete', ...extraReportArgs);
  if (report.status !== 0 && /requires explicit --approval approved/.test(report.stderr)) {
    report = run(project, 'orchestrate', 'report', '--result', 'complete', '--approval', 'approved', ...extraReportArgs);
  }
  assert.equal(report.status, 0, report.stderr);
  return next;
}

export function driveToCompletion(project) {
  const visited = [];
  for (;;) {
    const directive = completeStage(project);
    if (directive.complete) return visited;
    visited.push(directive.stage.id);
  }
}

export function readActiveState(project) {
  const pointer = JSON.parse(readFileSync(join(project, '.aidlc', 'state.json'), 'utf8'));
  return JSON.parse(readFileSync(join(project, '.aidlc', 'intents', pointer.activeIntentId, 'state.json'), 'utf8'));
}

export function helpText() {
  return runRaw(repo, 'help').stdout;
}

export function helpCommands(text = helpText()) {
  const lines = text.split(/\r?\n/);
  const start = lines.indexOf('Commands:');
  const slice = lines.slice(start + 1);
  const end = slice.findIndex((line) => line === '');
  return new Set(
    slice
      .slice(0, end < 0 ? undefined : end)
      .filter((line) => line.startsWith('  ') && line[2] && line[2] !== ' ')
      .map((line) => line.trim().split(/\s+/)[0].replace(/,$/, '')),
  );
}
