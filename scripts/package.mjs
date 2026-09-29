#!/usr/bin/env node
// scripts/package.mjs — generate every harness distribution into dist/<name>/.
//
// Usage:
//   node scripts/package.mjs              # build all harnesses
//   node scripts/package.mjs pi           # build one harness
//   node scripts/package.mjs --check      # determinism guard (rebuild and diff)
//   node scripts/package.mjs --check pi
//
// Edit core/ or harness/<name>/, never dist*.

import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { dirname } from "node:path";
import { buildHarness, diffTrees, listHarnesses } from "../core/tools/lib/packager.mjs";
import { compileGraph } from "../core/tools/lib/graph.mjs";
import { engineRoot } from "../core/tools/lib/version.mjs";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const distRoot = join(repoRoot, "dist");

function writeCompiledGraph() {
  const graph = compileGraph(engineRoot());
  const target = join(engineRoot(), "data", "stage-graph.json");
  mkdirSync(dirname(target), { recursive: true });
  writeFileSync(target, `${JSON.stringify(graph, null, 2)}\n`, "utf8");
  return target;
}

async function main() {
  const args = process.argv.slice(2);
  const check = args.includes("--check");
  const named = args.filter((arg) => !arg.startsWith("--"));
  const harnesses = named.length ? named : listHarnesses(repoRoot);

  const graphPath = writeCompiledGraph();
  console.log(`compiled ${graphPath}`);

  let failures = 0;
  for (const name of harnesses) {
    const outDir = join(distRoot, name);
    if (check) {
      const temp = mkdtempSync(join(tmpdir(), `my-aidlc-check-${name}-`));
      try {
        await buildHarness({ repoRoot, harnessName: name, outDir: temp });
        if (!existsSync(outDir)) {
          console.error(`FAIL ${name}: dist/${name} does not exist; run package first.`);
          failures += 1;
          continue;
        }
        const differences = diffTrees(temp, outDir);
        if (differences.length) {
          console.error(`FAIL ${name}: generated tree differs from dist/${name}:`);
          for (const diff of differences.slice(0, 20)) console.error(`  ${diff}`);
          failures += 1;
        } else {
          console.log(`PASS ${name}: dist/${name} is up to date`);
        }
      } finally {
        rmSync(temp, { recursive: true, force: true });
      }
    } else {
      await buildHarness({ repoRoot, harnessName: name, outDir });
      console.log(`built dist/${name}`);
    }
  }

  if (failures > 0) {
    console.error(`${failures} harness(es) failed.`);
    process.exit(1);
  }
}

main().catch((error) => {
  console.error(error?.stack || error?.message || error);
  process.exit(1);
});
