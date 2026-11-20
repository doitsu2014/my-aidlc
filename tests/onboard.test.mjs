import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, writeFileSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const cli = resolve(import.meta.dirname, '../core/tools/dtech-aidlc.mjs');

test('onboard detects a brownfield project and copies the BRD into shared', t => {
  const root = mkdtempSync(join(tmpdir(), 'aidlc-onboard-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(join(root, 'src'));
  writeFileSync(join(root, 'package.json'), '{"name":"sample"}\n');
  writeFileSync(join(root, 'src/app.ts'), 'export const value = 1;\n');
  mkdirSync(join(root, 'docs'));
  writeFileSync(join(root, 'docs/product-brd.md'), '# BRD\nBuild inventory.\n');
  const result = spawnSync(process.execPath, [cli, 'onboard', '--harness', 'claude', '--scope', 'solo', '--project', root], { encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  const output = JSON.parse(result.stdout);
  assert.equal(output.kind, 'brownfield');
  const profile = JSON.parse(readFileSync(join(root, '.aidlc/shared/project-profile.json'), 'utf8'));
  assert.ok(profile.languages.some(row => row.name === 'javascript'));
  assert.ok(profile.languages.some(row => row.name === 'typescript'));
  assert.match(readFileSync(join(root, '.aidlc/shared/brd/product-brd.md'), 'utf8'), /inventory/);
  const skillPath = join(root, '.claude/skills/dtech-aidlc/SKILL.md');
  assert.ok(existsSync(skillPath), 'Claude Code /dtech-aidlc skill entry point must be projected at the manifest path');
  assert.match(readFileSync(skillPath, 'utf8'), /^---\r?\nname: dtech-aidlc\r?\n/m);
  assert.equal(existsSync(join(root, '.claude/skills/dtech-aidlc/skills/aidlc/SKILL.md')), false);
});

test('onboard reports a project with no source as greenfield', t => {
  const root = mkdtempSync(join(tmpdir(), 'aidlc-green-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const result = spawnSync(process.execPath, [cli, 'onboard', '--project', root], { encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(JSON.parse(result.stdout).kind, 'greenfield');
});
