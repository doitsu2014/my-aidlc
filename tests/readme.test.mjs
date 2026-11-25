import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { completeStage, helpCommands, initProject, json, repo, tempProject } from './helpers.mjs';

const readme = readFileSync(join(repo, 'README.md'), 'utf8').replace(/\r\n/g, '\n');

test('README has the required sections and mentions', () => {
  for (const heading of ['Onboard a project', 'Project knowledge', 'What to commit', 'Harnesses', 'Upgrade and uninstall', 'Environment variables']) {
    assert.match(readme, new RegExp(`^## ${heading}$`, 'm'), heading);
  }
  for (const text of ['PROJECT.md', 'aidlc.knowledge.json', 'DTECH_AIDLC_HOME', 'dtech-aidlc harness refresh', 'CHANGELOG.md']) assert.ok(readme.includes(text), text);
  assert.equal(readme.split('\n').filter((line) => line === 'sh scripts/install.sh').length, 1);
});

test('every dtech-aidlc command in a README sh block is listed by help', () => {
  const known = helpCommands();
  const blocks = [...readme.matchAll(/```sh\n([\s\S]*?)```/g)].map((match) => match[1]);
  const words = blocks.flatMap((block) => block.split('\n')).map((line) => line.match(/^dtech-aidlc\s+(\S+)/)?.[1]).filter(Boolean);
  assert.ok(words.length > 10);
  for (const word of words) assert.ok(known.has(word), `README uses unknown command ${word}`);
});

test('CHANGELOG has a section for each release', () => {
  const changelog = readFileSync(join(repo, 'CHANGELOG.md'), 'utf8');
  for (const version of ['0.1.3', '0.2.0', '0.3.0', '0.4.0', '0.5.0', '0.6.0', '0.6.1', '0.6.2', '0.6.3', '0.6.4', '0.7.0', '0.7.1', '0.7.2']) assert.match(changelog, new RegExp(`^## ${version.replaceAll('.', '\.')}$`, 'm'), version);
  assert.equal(JSON.parse(readFileSync(join(repo, 'package.json'), 'utf8')).version, '0.7.2');
});

test('the approval record fields in the methodology README match the gate.decided audit row', (t) => {
  const methodology = readFileSync(join(repo, 'core/methodology/README.md'), 'utf8').replace(/\r\n/g, '\n');
  const sentence = methodology.match(/record at minimum: ([^.]*)\./)?.[1];
  assert.ok(sentence, 'approval sentence not found');
  const fields = sentence.split(',').map((name) => name.trim());
  assert.deepEqual(fields, ['gate', 'phase', 'stage', 'decision', 'result', 'by', 'bySource', 'mode', 'at', 'note', 'detail', 'skippedStages', 'artifacts']);
  const project = tempProject(t);
  initProject(project);
  json(project, 'orchestrate', 'next', '--intent', 'Approval fields');
  completeStage(project);
  completeStage(project, ['--approval', 'approved', '--approved-by', 'Reviewer']);
  const rows = readFileSync(join(project, '.aidlc/audit.jsonl'), 'utf8').trim().split('\n').map((line) => JSON.parse(line));
  const row = rows.find((item) => item.event === 'gate.decided');
  assert.ok(row, 'gate.decided row');
  for (const name of fields) assert.ok(name in row, `gate.decided lacks ${name}`);
});
