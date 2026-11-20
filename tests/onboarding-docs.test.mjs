import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const repo = resolve(import.meta.dirname, '..');

test('onboarding docs match harness manifests and current project layout', (t) => {
  for (const harness of ['claude', 'deepseek']) {
    const manifest = JSON.parse(readFileSync(resolve(repo, `harness/${harness}/manifest.json`), 'utf8'));
    const doc = readFileSync(resolve(repo, `harness/${harness}/onboarding.md`), 'utf8');
    assert.doesNotMatch(doc, /tools\//);
    for (const required of ['dtech-aidlc version', 'dtech-aidlc onboard --harness', 'dtech-aidlc harness refresh', 'PROJECT.md', 'aidlc.knowledge.json', '.aidlc/knowledge/', '.aidlc/shared/']) assert.ok(doc.includes(required), `${harness} missing ${required}`);
    assert.ok(doc.includes(['-', `\`${manifest.skillDir}/SKILL.md\``].join(' ')));
    assert.ok(doc.includes(manifest.invoke));
    assert.ok(doc.includes(`--harness ${manifest.name}`));
    const root = mkdtempSync(join(tmpdir(), 'aidlc-onboarding-docs-'));
    t.after(() => rmSync(root, { recursive: true, force: true }));
    const cli = resolve(import.meta.dirname, '../core/tools/dtech-aidlc.mjs');
    const onboard = spawnSync(process.execPath, [cli, 'onboard', '--harness', manifest.name, '--scope', manifest.name === 'deepseek' ? 'team' : 'solo', '--project', root], { encoding: 'utf8' });
    assert.equal(onboard.status, 0, onboard.stderr);
    const projectedSkill = resolve(root, `${manifest.skillDir}/SKILL.md`);
    assert.equal(readFileSync(projectedSkill, 'utf8'), readFileSync(resolve(repo, `harness/${harness}/skills/aidlc/SKILL.md`), 'utf8'));
    const configPath = resolve(root, '.aidlc/config.json');
    const configBefore = readFileSync(configPath, 'utf8');
    const repeated = spawnSync(process.execPath, [cli, 'onboard', '--harness', manifest.name, '--scope', manifest.name === 'deepseek' ? 'team' : 'solo', '--project', root], { encoding: 'utf8' });
    assert.equal(repeated.status, 0, repeated.stderr);
    assert.equal(readFileSync(configPath, 'utf8'), configBefore);
    for (const line of doc.split('\n')) {
      if (!line.startsWith('- ')) continue;
      const match = line.match(/`(\.[^`]+)`/);
      if (match) assert.ok(existsSync(resolve(root, match[1])), `${harness} installed path missing: ${match[1]}`);
    }
  }
});
