import test from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..', 'core/methodology');
function markdownFiles(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => entry.isDirectory()
    ? markdownFiles(join(dir, entry.name))
    : entry.isFile() && entry.name.endsWith('.md') ? [join(dir, entry.name)] : []);
}

test('relative Markdown links in methodology resolve to files outside fenced code', () => {
  const missing = [];
  for (const file of markdownFiles(root)) {
    const lines = readFileSync(file, 'utf8').split(/\r?\n/);
    let fence = null;
    for (const [index, line] of lines.entries()) {
      const marker = line.match(/^\s*(`{3,}|~{3,})/);
      if (marker) { fence = fence === null ? marker[1][0] : marker[1][0] === fence ? null : fence; continue; }
      if (fence) continue;
      for (const match of line.matchAll(/\]\(([^)]+)\)/g)) {
        const target = match[1].split(/[?#]/, 1)[0];
        if (!target || /^(https?:|#|mailto:)/i.test(target)) continue;
        const resolved = resolve(dirname(file), target);
        try {
          const stat = statSync(resolved);
          if (!stat.isFile() && !stat.isDirectory()) missing.push(`${file}:${index + 1} -> ${target}`);
        } catch { missing.push(`${file}:${index + 1} -> ${target}`); }
      }
    }
  }
  assert.deepEqual(missing, []);
});
