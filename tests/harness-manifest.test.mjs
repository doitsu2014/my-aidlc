import test from 'node:test';
import assert from 'node:assert/strict';
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { loadHarnessManifest } from '../core/tools/lib.mjs';

const repo = resolve(import.meta.dirname, '..');

test('shipped manifests declare modelAliases', () => {
  assert.deepEqual(loadHarnessManifest('claude').modelAliases, ['opus', 'sonnet', 'haiku', 'fable']);
  assert.deepEqual(loadHarnessManifest('deepseek').modelAliases, []);
});

test('a manifest with an unknown or missing model alias is rejected', (t) => {
  const home = mkdtempSync(join(tmpdir(), 'aidlc-manifest-'));
  t.after(() => { delete process.env.DTECH_AIDLC_HOME; rmSync(home, { recursive: true, force: true }); });
  cpSync(join(repo, 'harness/claude'), join(home, 'harness/claude'), { recursive: true });
  const path = join(home, 'harness/claude/manifest.json');
  const manifest = JSON.parse(readFileSync(path, 'utf8'));
  process.env.DTECH_AIDLC_HOME = home;
  writeFileSync(path, JSON.stringify({ ...manifest, modelAliases: ['opus', 'gpt'] }));
  assert.throws(() => loadHarnessManifest('claude'), /modelAliases/);
  const { modelAliases, ...without } = manifest;
  writeFileSync(path, JSON.stringify(without));
  assert.throws(() => loadHarnessManifest('claude'), /modelAliases/);
});
