import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { join } from 'node:path';
import { COMMAND_FLAGS } from '../core/tools/cli-spec.mjs';
import { MODEL_ALIASES } from '../core/tools/lib.mjs';
import { helpCommands, helpText, repo } from './helpers.mjs';

test('help, completion, and the command table stay in step', () => {
  const completion = readFileSync(join(repo, 'core/tools/completion.zsh'), 'utf8');
  const names = [...completion.matchAll(/^\s+'([a-z-]+):/gm)].map((match) => match[1]);
  assert.deepEqual(helpCommands(), new Set(names));
  const table = new Set(Object.keys(COMMAND_FLAGS).map((key) => key.split(' ')[0]));
  table.delete('help');
  table.delete('version');
  const fromHelp = helpCommands();
  fromHelp.delete('help');
  fromHelp.delete('version');
  assert.deepEqual(fromHelp, table);
  const help = helpText();
  for (const [command, options] of Object.entries(COMMAND_FLAGS)) {
    for (const name of Object.keys(options)) {
      assert.match(help, new RegExp(`--${name}\\b`), `${command} option --${name}`);
    }
  }
  const graph = JSON.parse(readFileSync(join(repo, 'core/data/phase-graph.json'), 'utf8'));
  const words = new Set([
    ...graph.agents.map((agent) => agent.id),
    ...Object.keys(graph.scopes),
    ...readdirSync(join(repo, 'harness')).filter((name) => !name.startsWith('.')),
    ...MODEL_ALIASES,
  ]);
  for (const word of words) {
    assert.match(completion, new RegExp(`(?:^|[^A-Za-z0-9_-])${word}(?:$|[^A-Za-z0-9_-])`), word);
  }
  const zsh = spawnSync('zsh', ['-n', join(repo, 'core/tools/completion.zsh')], { encoding: 'utf8' });
  if (zsh.error && zsh.error.code === 'ENOENT') return;
  assert.equal(zsh.status, 0, zsh.stderr);
});

test('completion zsh includes onboard', () => {
  const result = spawnSync(process.execPath, [join(repo, 'core/tools/dtech-aidlc.mjs'), 'completion', 'zsh'], { encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /onboard:/);
});

assert.equal(existsSync(join(repo, 'tests/helpers')), false);
