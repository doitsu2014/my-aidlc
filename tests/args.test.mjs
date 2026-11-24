import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync, statSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { parseArgs } from '../core/tools/lib.mjs';
import { COMMAND_FLAGS, TWO_WORD_COMMANDS, commandKey, optionTypes } from '../core/tools/cli-spec.mjs';
import { cli, initProject, json, repo, run, runRaw, tempProject } from './helpers.mjs';

const types = { project: 'string', help: 'boolean', version: 'boolean', intent: 'string', tag: 'list' };

function snapshot(dir) {
  const out = {};
  if (!existsSync(dir)) return out;
  const walk = (path, rel = '') => {
    for (const name of readdirSync(path)) {
      const next = join(path, name);
      const key = rel ? `${rel}/${name}` : name;
      const info = statSync(next);
      if (info.isDirectory()) walk(next, key);
      else out[key] = readFileSync(next);
    }
  };
  walk(dir);
  return out;
}

test('parseArgs covers typed flags, unknowns, and the terminator', () => {
  assert.deepEqual(parseArgs(['--intent=hello'], types).flags.intent, 'hello');
  assert.equal(parseArgs(['--intent', 'hello'], types).flags.intent, 'hello');
  assert.equal(parseArgs(['--intent', '--fix-login'], types).flags.intent, '--fix-login');
  assert.throws(() => parseArgs(['--intent'], types), /Option --intent requires a non-empty value/);
  assert.throws(() => parseArgs(['--intent='], types), /Option --intent requires a non-empty value/);
  assert.throws(() => parseArgs(['--intent', '--'], types), /Option --intent requires a non-empty value/);
  assert.throws(() => parseArgs(['--intent', '--project'], types), /Option --intent requires a non-empty value/);
  assert.throws(() => parseArgs(['--intent', 'a', '--intent', 'b'], types), /Option --intent was given more than once/);
  assert.deepEqual(parseArgs(['--tag', 'a', '--tag', 'b'], types).flags.tag, ['a', 'b']);
  const helpFlag = parseArgs(['--help', 'status'], types);
  assert.deepEqual(helpFlag.positional, ['status']);
  assert.equal(helpFlag.flags.help, true);
  assert.deepEqual(helpFlag.unknown, []);
  assert.throws(() => parseArgs(['--help=true'], types), /Option --help does not take a value/);
  assert.deepEqual(parseArgs(['--bogus', 'x'], types), { positional: ['x'], flags: Object.create(null), unknown: ['bogus'] });
  assert.deepEqual(parseArgs(['status', '--', '--help'], types).positional, ['status', '--help']);
});

test('unknown options fail before writing and list allowed flags', (t) => {
  const project = tempProject(t, 'aidlc-args-unknown-');
  for (const flag of ['--bogus']) {
    const result = runRaw(project, 'onboard', flag, 'x', '--project', project);
    assert.equal(result.status, 1);
    assert.match(result.stderr, /Unknown option --bogus for 'onboard'/);
    assert.match(result.stderr, /Allowed options: --harness, --scope, --docs, --no-detect, --rescan, --project/);
    assert.equal(existsSync(join(project, '.aidlc')), false);
  }
});

test('missing option values fail with a clear message', (t) => {
  const project = tempProject(t, 'aidlc-args-value-');
  for (const args of [['orchestrate', 'next', '--intent'], ['orchestrate', 'next', '--intent', ''], ['orchestrate', 'next', '--intent', '--project', project]]) {
    const result = runRaw(project, cli, ...args);
    assert.equal(result.status, 1);
    assert.match(result.stderr, /Option --intent requires a non-empty value/);
    assert.equal(result.stderr.includes('toLowerCase'), false);
  }
  const missingProject = runRaw(project, cli, 'status', '--project');
  assert.equal(missingProject.status, 1);
  assert.match(missingProject.stderr, /Option --project requires a non-empty value/);
});

test('intent text may start with dashes after a project is initialized', (t) => {
  const project = tempProject(t, 'aidlc-args-dash-');
  initProject(project);
  const directive = json(project, 'orchestrate', 'next', '--intent', '--fix the login bug');
  assert.equal(directive.intent, '--fix the login bug');
  assert.equal(directive.intentId, 'fix-the-login-bug');
});

test('typos and missing detail do not change workflow files', (t) => {
  const project = tempProject(t, 'aidlc-args-report-');
  initProject(project);
  json(project, 'orchestrate', 'next', '--intent', 'keep files');
  const before = snapshot(join(project, '.aidlc'));
  const missing = run(project, 'orchestrate', 'report', '--result', 'skipped', '--detail');
  const typo = run(project, 'orchestrate', 'report', '--result', 'complete', '--detial', 'x');
  assert.equal(missing.status, 1);
  assert.equal(typo.status, 1);
  const after = snapshot(join(project, '.aidlc'));
  assert.deepEqual(Object.keys(after).sort(), Object.keys(before).sort());
  for (const key of Object.keys(before)) assert.deepEqual(after[key], before[key]);
});

test('--version prints only the package version line', () => {
  const version = JSON.parse(readFileSync(join(repo, 'package.json'), 'utf8')).version;
  const result = runRaw(repo, '--version');
  assert.equal(result.status, 0);
  assert.equal(result.stdout, `dtech-aidlc ${version}\n`);
});

test('prototype property names are valid dependency ids', (t) => {
  const project = tempProject(t, 'aidlc-args-proto-');
  initProject(project);
  json(project, 'orchestrate', 'next', '--intent', 'deps proto');
  const added = json(project, 'deps', 'add', 'constructor', 'x');
  assert.deepEqual(added, { id: 'constructor', dependencies: ['x'] });
  const pulled = json(project, 'deps', 'pull', 'toString');
  assert.equal(pulled.id, 'toString');
  assert.deepEqual(pulled.dependencies, []);
});

test('unexpected extra arguments are rejected', (t) => {
  const project = tempProject(t, 'aidlc-args-extra-');
  const result = run(project, 'status', 'extra');
  assert.equal(result.status, 1);
  assert.match(result.stderr, /Unexpected argument 'extra' for 'status'/);
});

test('help lists global and previously undocumented options', () => {
  const result = runRaw(repo, '--help');
  assert.equal(result.status, 0);
  assert.match(result.stdout, /Global option: --project DIR  Use DIR as the project root instead of the current directory\./);
  assert.match(result.stdout, /--description TEXT/);
  assert.match(result.stdout, /--detail TEXT/);
  assert.match(result.stdout, /--stage ID/);
  assert.match(result.stdout, /--file FILE/);
  for (const line of [
    '  help, --help            Show help',
    '  version, --version      Show version',
    '  onboard [--harness NAME] [--scope NAME] [--docs PATH] [--no-detect]',
    '  init [--harness NAME] [--scope NAME]    Alias of onboard (--harness is required the first time)',
    '  scope [NAME] [--depth NAME]  List scopes or show stages in a scope',
    '  status [--intent ID]    Show current intent and stage progress',
    '  intent rework --detail TEXT  Return blocked Develop work to implementation-design',
    '  doctor                  Validate graph, state, config, and projection',
    '  config agents            List effective agent models',
    '  completion zsh          Print the zsh completion function',
    'Scope values: solo, team, enterprise. Harness values: claude, deepseek.',
  ]) assert.equal(result.stdout.includes(line), true, line);
});

test('source lines in the CLI stay short and the changelog exists', () => {
  const long = readFileSync(join(repo, 'core/tools/dtech-aidlc.mjs'), 'utf8').split(/\n/).filter((line) => line.length > 400);
  assert.deepEqual(long, []);
  assert.match(readFileSync(join(repo, 'CHANGELOG.md'), 'utf8'), /^## 0\.1\.3$/m);
});

test('command table covers every key and two-word prefix', (t) => {
  assert.equal(commandKey(['knowledge', 'sources', 'add']), 'knowledge sources');
  assert.equal(commandKey(['status', 'x']), 'status');
  optionTypes();
  const project = tempProject(t, 'aidlc-args-table-');
  for (const key of Object.keys(COMMAND_FLAGS)) {
    const result = runRaw(project, ...key.split(' '), '--bogus');
    assert.equal(result.status, 1, key);
    assert.equal(existsSync(join(project, '.aidlc')), false, key);
  }
  for (const word of TWO_WORD_COMMANDS) assert.equal(commandKey([word, 'x']).split(' ').length, 2);
});
