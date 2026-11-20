import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, mkdirSync, readFileSync, readdirSync, symlinkSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { intentSlug, sameIntentText } from '../core/tools/workflow.mjs';
import { initProject, json, readActiveState, run, runWithStdin, tempProject } from './helpers.mjs';

const fallback = /^intent-[0-9]{8}-[0-9a-f]{6}$/;

test('intent slugs are readable for Latin text and use a hashed fallback when needed', () => {
  assert.equal(intentSlug('Thêm chức năng xuất kho'), 'them-chuc-nang-xuat-kho');
  assert.equal(intentSlug('Sửa lỗi đăng nhập Đà Nẵng'), 'sua-loi-dang-nhap-da-nang');
  assert.equal(intentSlug('Add CSV export to inventory'), 'add-csv-export-to-inventory');
  assert.equal(intentSlug('Add CSV export of stock levels with $(whoami) and `date`'), 'add-csv-export-of-stock-levels-with-whoami-and-d');
  assert.match(intentSlug('日本語の機能'), fallback);
  assert.match(intentSlug('APIの認証を修正する'), fallback);
  assert.notEqual(intentSlug('日本語の機能'), intentSlug('APIの認証を修正する'));
  assert.equal(intentSlug('Fix 日本語 bug'), 'fix-bug');
  assert.match(intentSlug('QA'), fallback);
  assert.ok(intentSlug(`Add ${'many '.repeat(40)}features`).length <= 48);
  assert.ok(!intentSlug(`Add ${'many '.repeat(40)}features`).endsWith('-'));
});

test('same intent text compares NFC and collapsed whitespace', () => {
  assert.equal(sameIntentText('Café\nfeature', 'Café   feature'), true);
});

test('intent-file reads literal text and leaves its source unchanged', (t) => {
  const project = tempProject(t);
  initProject(project);
  const source = '.aidlc/request.md';
  const input = 'Add CSV export of stock levels with $(whoami) and `date`';
  writeFileSync(join(project, source), input);
  const result = run(project, 'orchestrate', 'next', '--intent-file', source);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(JSON.parse(result.stdout).intentId, 'add-csv-export-of-stock-levels-with-whoami-and-d');
  assert.equal(readFileSync(join(project, source), 'utf8'), input);
  assert.equal(readFileSync(join(project, '.aidlc/intents/add-csv-export-of-stock-levels-with-whoami-and-d/intent.md'), 'utf8').includes('$(whoami)'), true);
});

test('intent-file normalizes BOM and CRLF for persisted text', (t) => {
  const project = tempProject(t);
  initProject(project);
  writeFileSync(join(project, 'request.txt'), '﻿  First line\r\nSecond line  \r\n');
  const result = run(project, 'orchestrate', 'next', '--intent-file', 'request.txt');
  assert.equal(result.status, 0, result.stderr);
  const expected = 'First line\nSecond line';
  assert.equal(readActiveState(project).intent, expected);
  assert.equal(readFileSync(join(project, '.aidlc/intents/first-line-second-line/intent.md'), 'utf8').includes(expected), true);
});

test('intent text can be piped without shell interpretation', (t) => {
  const project = tempProject(t);
  initProject(project);
  const input = '  Fix `rm -rf` and $(whoami) bug  ';
  const result = runWithStdin(project, input, 'orchestrate', 'next', '--intent-file', '-');
  assert.equal(result.status, 0, result.stderr);
  assert.equal(readActiveState(project).intent, input.trim());
});

test('oversized intent files fail without creating intent directories', (t) => {
  const project = tempProject(t);
  initProject(project);
  writeFileSync(join(project, 'large.txt'), 'x'.repeat(100_001));
  const result = run(project, 'orchestrate', 'next', '--intent-file', 'large.txt');
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Intent file is larger than 100000 bytes/);
  assert.deepEqual(existsSync(join(project, '.aidlc/intents')) ? readdirSync(join(project, '.aidlc/intents')) : [], []);
});

test('invalid intent sources fail without creating intent directories', (t) => {
  const project = tempProject(t);
  initProject(project);
  const intentDir = join(project, '.aidlc/intents');
  const failures = [
    [...['--intent', 'a', '--intent-file', 'request.txt'], 'Give the intent with only one'],
  ];
  for (const [args, message] of failures) {
    const result = run(project, 'orchestrate', 'next', ...args);
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, new RegExp(message));
    assert.deepEqual(existsSync(intentDir) ? requireDirectory(intentDir) : [], []);
  }
  const cases = [
    ['missing.txt', 'Intent file not found'],
    ['empty.txt', 'Intent text is empty'],
    ['nul.txt', 'Intent text contains a NUL character'],
    ['large.txt', 'Intent file is larger than 100000 bytes'],
    ['link.txt', 'Refusing to read symlink intent file'],
    ['folder', 'Intent file is not a regular file'],
  ];
  writeFileSync(join(project, 'empty.txt'), ' \r\n\t');
  writeFileSync(join(project, 'nul.txt'), 'bad\0text');
  writeFileSync(join(project, 'large.txt'), 'x'.repeat(100_001));
  writeFileSync(join(project, 'target.txt'), 'intent');
  symlinkSync(join(project, 'target.txt'), join(project, 'link.txt'));
  mkdirSync(join(project, 'folder'));
  for (const [file, message] of cases) {
    const result = run(project, 'orchestrate', 'next', '--intent-file', file);
    assert.notEqual(result.status, 0, file);
    assert.ok(result.stderr.includes(message), `${file}: ${result.stderr}`);
    assert.deepEqual(existsSync(intentDir) ? requireDirectory(intentDir) : [], [], `${file} wrote intent data`);
  }
});

function requireDirectory(path) {
  return readdirSync(path);
}
