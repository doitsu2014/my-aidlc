import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { initProject, json, run, tempProject } from './helpers.mjs';

test('init creates PROJECT.md and a knowledge house without a default-only manifest', (t) => {
  const project = tempProject(t, 'aidlc-know-init-');
  const result = initProject(project);
  assert.equal(existsSync(join(project, 'PROJECT.md')), true);
  assert.equal(existsSync(join(project, '.aidlc', 'knowledge')), true);
  assert.equal(existsSync(join(project, 'aidlc.knowledge.json')), false);
  assert.equal(result.knowledge.project.created, true);
  assert.equal(result.knowledge.manifestStatus, 'absent');
});

test('knowledge sources add and list register docs by reference', (t) => {
  const project = tempProject(t, 'aidlc-know-src-');
  initProject(project);
  json(project, 'knowledge', 'sources', 'add', 'docs', 'docs', '--kind', 'docs');
  const listed = json(project, 'knowledge', 'sources');
  assert.equal(listed.sources.some((item) => item.id === 'docs' && item.kind === 'docs'), true);
  const manifest = JSON.parse(readFileSync(join(project, 'aidlc.knowledge.json'), 'utf8'));
  assert.equal(manifest.schemaVersion, 1);
  assert.equal(manifest.sources.some((item) => item.path === 'docs'), true);
});

test('orchestrate next context lists project knowledge', (t) => {
  const project = tempProject(t, 'aidlc-know-ctx-');
  initProject(project);
  json(project, 'knowledge', 'sources', 'add', 'docs', 'docs', '--kind', 'docs');
  const next = json(project, 'orchestrate', 'next', '--intent', 'Use project knowledge');
  assert.equal(next.context.project.path, 'PROJECT.md');
  assert.equal(next.context.knowledge.dir, '.aidlc/knowledge');
  assert.ok(Array.isArray(next.context.granted));
});

test('knowledge check and index work on an empty house', (t) => {
  const project = tempProject(t, 'aidlc-know-check-');
  initProject(project);
  const check = json(project, 'knowledge', 'check');
  assert.equal(check.ok, true);
  const index = json(project, 'knowledge', 'index');
  assert.equal(existsSync(join(project, '.aidlc', 'knowledge', '_index.md')), true);
  assert.equal(index.records.length, 0);
});
