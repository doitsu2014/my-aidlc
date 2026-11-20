import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { intentSlug } from '../core/tools/workflow.mjs';
import { readGraph } from '../core/tools/lib.mjs';
import { scanProject } from '../core/tools/onboard.mjs';
import { initProject, json, tempProject } from './helpers.mjs';

test('phase graph 1.2.0 declares inputs on every stage', () => {
  const graph = readGraph();
  assert.equal(graph.schemaVersion, '1.2.0');
  for (const stage of graph.stages) {
    assert.ok(Array.isArray(stage.inputs?.artifacts), stage.id);
    assert.ok(Array.isArray(stage.inputs?.context), stage.id);
  }
});

test('Vietnamese intent text becomes a readable id', (t) => {
  const project = tempProject(t, 'aidlc-m23-slug-');
  initProject(project);
  const next = json(project, 'orchestrate', 'next', '--intent', 'Thêm chức năng xuất kho');
  assert.equal(next.intentId, 'them-chuc-nang-xuat-kho');
  assert.equal(intentSlug('Sửa lỗi đăng nhập Đà Nẵng'), 'sua-loi-dang-nhap-da-nang');
});

test('orchestrate next writes .aidlc/.gitignore', (t) => {
  const project = tempProject(t, 'aidlc-m23-ignore-');
  initProject(project);
  json(project, 'orchestrate', 'next', '--intent', 'ignore file');
  assert.equal(existsSync(join(project, '.aidlc', '.gitignore')), true);
});

test('dart sources classify as brownfield', (t) => {
  const project = tempProject(t, 'aidlc-m23-dart-');
  writeFileSync(join(project, 'pubspec.yaml'), 'name: x\n');
  mkdirSync(join(project, 'lib'));
  writeFileSync(join(project, 'lib', 'main.dart'), 'void main() {}\n');
  const scan = scanProject(project);
  assert.equal(scan.kind, 'brownfield');
  assert.ok(scan.languages.some((row) => row.name === 'dart'));
});
