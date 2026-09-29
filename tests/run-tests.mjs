#!/usr/bin/env node
// tests/run-tests.mjs — smoke, unit, and integration tests for my-aidlc.

import assert from "node:assert/strict";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { parseFrontmatter } from "../core/tools/lib/frontmatter.mjs";
import {
  compileGraph,
  detectScope,
  loadMethodology,
  workflowForScope,
  scopeByName,
} from "../core/tools/lib/graph.mjs";
import { buildHarness, listHarnesses } from "../core/tools/lib/packager.mjs";
import { runDoctor } from "../core/tools/lib/doctor.mjs";
import { runNext, runReport, statusReport, parkDirective } from "../core/tools/lib/orchestrate.mjs";
import { loadConfig } from "../core/tools/lib/config.mjs";
import { loadState, readAudit } from "../core/tools/lib/state.mjs";
import { engineRoot, repoRoot, readVersion } from "../core/tools/lib/version.mjs";

const tests = [];
const test = (name, fn) => tests.push({ name, fn });

function tempProject() {
  return mkdtempSync(join(tmpdir(), "my-aidlc-test-"));
}

// ---------------------------------------------------------------------------
// Unit: frontmatter
// ---------------------------------------------------------------------------

test("frontmatter parses scalars, lists, and list-of-maps", () => {
  const text = [
    "---",
    "slug: demo",
    "order: 3",
    "flag: true",
    "tags: [a, b]",
    "support_agents:",
    "  - product-agent",
    "consumes:",
    "  - artifact: intent-statement",
    "    required: true",
    "  - artifact: requirements",
    "    required: false",
    "---",
    "",
    "# Body",
  ].join("\n");
  const { data, body, hasFrontmatter } = parseFrontmatter(text);
  assert.equal(hasFrontmatter, true);
  assert.equal(data.slug, "demo");
  assert.equal(data.order, 3);
  assert.equal(data.flag, true);
  assert.deepEqual(data.tags, ["a", "b"]);
  assert.deepEqual(data.support_agents, ["product-agent"]);
  assert.equal(data.consumes.length, 2);
  assert.equal(data.consumes[0].artifact, "intent-statement");
  assert.equal(data.consumes[0].required, true);
  assert.equal(data.consumes[1].required, false);
  assert.match(body, /# Body/);
});

test("frontmatter handles folded block scalars", () => {
  const text = "---\ndescription: >\n  one\n  two\ntier: judgment\n---\n";
  const { data } = parseFrontmatter(text);
  assert.equal(data.description, "one two");
  assert.equal(data.tier, "judgment");
});

// ---------------------------------------------------------------------------
// Unit: methodology
// ---------------------------------------------------------------------------

test("methodology loads five phases, 21 stages, scopes, and agents", () => {
  const m = loadMethodology(engineRoot());
  assert.equal(m.phases.length, 5);
  assert.deepEqual(
    m.phases.map((p) => p.slug),
    ["analyze", "ideate", "develop", "launch", "curate"],
  );
  assert.equal(m.stages.length, 21);
  assert.ok(m.scopes.length >= 6);
  assert.equal(m.agents.length, 15);
});

test("every stage references known agents", () => {
  const m = loadMethodology(engineRoot());
  const known = new Set(m.agents.map((a) => a.slug));
  for (const stage of m.stages) {
    const refs = [stage.leadAgent, ...stage.supportAgents, stage.reviewer].filter(Boolean);
    for (const ref of refs) {
      assert.ok(known.has(ref), `stage ${stage.slug} references unknown agent ${ref}`);
    }
  }
});

test("every consumed artifact is produced by some stage", () => {
  const m = loadMethodology(engineRoot());
  const produced = new Set(m.stages.flatMap((s) => s.produces));
  for (const stage of m.stages) {
    for (const consume of stage.consumes) {
      assert.ok(produced.has(consume.artifact), `${stage.slug} consumes unproduced ${consume.artifact}`);
    }
  }
});

test("classic scope runs all 21 stages in phase order", () => {
  const m = loadMethodology(engineRoot());
  const workflow = workflowForScope(m, scopeByName(m, "classic"));
  assert.equal(workflow.length, 21);
  const phaseOrder = { analyze: 1, ideate: 2, develop: 3, launch: 4, curate: 5 };
  let last = 0;
  for (const stage of workflow) {
    const order = phaseOrder[stage.phase];
    assert.ok(order >= last, `phase order regressed at ${stage.slug}`);
    last = order;
  }
});

test("express scope is lighter than classic and skips design", () => {
  const m = loadMethodology(engineRoot());
  const express = workflowForScope(m, scopeByName(m, "express")).map((s) => s.slug);
  assert.ok(express.length < 21);
  assert.ok(!express.includes("architecture-design"));
  assert.ok(express.includes("code-generation"));
});

test("scope detection matches keywords and falls back to classic", () => {
  const m = loadMethodology(engineRoot());
  assert.equal(detectScope(m, "fix a bug in checkout"), "bugfix");
  assert.equal(detectScope(m, "quick express change"), "express");
  assert.equal(detectScope(m, "build something nobody has words for"), "classic");
});

test("compiled graph includes a scope grid", () => {
  const graph = compileGraph(engineRoot());
  assert.equal(graph.phases.length, 5);
  assert.ok(graph.scopeGrid.classic.length === 21);
  assert.ok(Array.isArray(graph.scopeGrid.express));
});

// ---------------------------------------------------------------------------
// Integration: orchestration
// ---------------------------------------------------------------------------

test("orchestration drives a workflow from start to done", () => {
  const root = tempProject();
  try {
    const m = loadMethodology(engineRoot());
    const config = loadConfig(root);

    const first = runNext(root, m, config, { text: "build an inventory API" });
    assert.equal(first.kind, "run-stage");
    assert.equal(first.stage, "intent-capture");
    assert.equal(first.gate, true);
    assert.ok(first.produce_paths[0].startsWith("aidlc/spaces/default/intents/"));

    let directive = first;
    let guard = 0;
    while (directive.kind !== "done" && guard < 100) {
      guard += 1;
      if (directive.kind === "ask") {
        directive = runReport(root, m, config, {
          stage: directive.stage,
          result: "approved",
          userInput: "Approve",
        });
      } else if (directive.kind === "run-stage") {
        runReport(root, m, config, { stage: directive.stage, result: "awaiting-approval" });
        directive = runReport(root, m, config, {
          stage: directive.stage,
          result: "approved",
          userInput: "Approve",
        });
      } else {
        break;
      }
    }
    assert.equal(directive.kind, "done");
    assert.ok(guard < 100);

    const state = loadState(root);
    const config2 = loadConfig(root);
    const report = statusReport(root, m, config2);
    assert.equal(report.stages.every((s) => s.status === "complete"), true);
    assert.equal(state.currentStage, null);

    const audit = readAudit(root);
    assert.ok(audit.some((row) => row.event === "INTENT_CREATED"));
    assert.ok(audit.some((row) => row.event === "WORKFLOW_COMPLETED"));
    assert.ok(existsSync(join(root, "aidlc/state.json")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("a skipped conditional stage requires a reason and advances", () => {
  const root = tempProject();
  try {
    const m = loadMethodology(engineRoot());
    const config = loadConfig(root);
    let directive = runNext(root, m, config, { text: "add a feature" });
    // Move to a conditional stage by approving until research-synthesis appears,
    // or skip the current stage directly.
    const noReason = runReport(root, m, config, { stage: directive.stage, result: "skipped" });
    assert.equal(noReason.kind, "error");
    const skipped = runReport(root, m, config, {
      stage: directive.stage,
      result: "skipped",
      reason: "not applicable",
    });
    assert.notEqual(skipped.kind, "error");
    const state = loadState(root);
    assert.equal(state.stages[directive.stage].status, "skipped");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("rejected then revised keeps the stage active until approved", () => {
  const root = tempProject();
  try {
    const m = loadMethodology(engineRoot());
    const config = loadConfig(root);
    const first = runNext(root, m, config, { text: "build a thing" });
    runReport(root, m, config, { stage: first.stage, result: "awaiting-approval" });
    const rejected = runReport(root, m, config, {
      stage: first.stage,
      result: "rejected",
      userInput: "Request Changes",
      reason: "needs more detail",
    });
    assert.notEqual(rejected.kind, "error");
    let state = loadState(root);
    assert.equal(state.stages[first.stage].status, "active");
    assert.equal(state.stages[first.stage].feedback.length, 1);
    const revised = runReport(root, m, config, { stage: first.stage, result: "revised" });
    assert.equal(revised.kind, "ask");
    state = loadState(root);
    assert.equal(state.stages[first.stage].status, "awaiting-approval");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("parking and resuming is recorded", () => {
  const root = tempProject();
  try {
    const m = loadMethodology(engineRoot());
    const config = loadConfig(root);
    runNext(root, m, config, { text: "build a thing" });
    const parked = parkDirective(root, loadState(root));
    assert.equal(parked.kind, "parked");
    assert.equal(loadState(root).parked, true);
    // A plain next stays parked; --resume continues.
    assert.equal(runNext(root, m, config, {}).kind, "parked");
    const resumed = runNext(root, m, config, { resume: true });
    assert.equal(resumed.kind, "run-stage");
    assert.equal(loadState(root).parked, false);
    assert.ok(readAudit(root).some((row) => row.event === "WORKFLOW_PARKED"));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

// ---------------------------------------------------------------------------
// Integration: packaging
// ---------------------------------------------------------------------------

test("packaging builds every harness with tokens substituted", async () => {
  for (const name of listHarnesses(repoRoot())) {
    const outDir = join(tempProject(), `dist-${name}`);
    try {
      const { manifest } = await buildHarness({ repoRoot: repoRoot(), harnessName: name, outDir });
      const tree = join(outDir, manifest.harnessDir);
      assert.ok(existsSync(tree), `${name} harness dir missing`);
      assert.ok(existsSync(join(tree, "phases/analyze/stages/intent-capture.md")));
      assert.ok(existsSync(join(tree, "agents/product-agent.md")));
      assert.ok(existsSync(join(tree, "version.json")));
      const skillPath = join(outDir, manifest.orchestratorSkillPath);
      assert.ok(existsSync(skillPath), `${name} orchestrator skill missing`);
      const skill = readFileSync(skillPath, "utf8");
      assert.ok(!skill.includes("{{"), `${name} skill still contains tokens`);
      assert.ok(skill.includes(`${manifest.invoke} orchestrate next`) || skill.includes("orchestrate next"));
    } finally {
      rmSync(outDir, { recursive: true, force: true });
    }
  }
});

test("every harness ships the four database skills with valid frontmatter", async () => {
  const expected = ["db-postgres", "db-mysql", "db-mssql", "db-mongodb"];
  for (const name of listHarnesses(repoRoot())) {
    const outDir = join(tempProject(), `dist-${name}`);
    try {
      const { manifest } = await buildHarness({ repoRoot: repoRoot(), harnessName: name, outDir });
      const skillsDir = skillsDirFor(outDir, manifest);
      for (const skill of expected) {
        const path = join(skillsDir, skill, "SKILL.md");
        assert.ok(existsSync(path), `${name} is missing ${skill} at ${path}`);
        const { data } = parseFrontmatter(readFileSync(path, "utf8"));
        assert.equal(data.name, skill, `${name}/${skill} frontmatter name mismatch`);
        assert.ok(
          typeof data.description === "string" && data.description.length > 20,
          `${name}/${skill} needs a routing description`,
        );
        assert.ok(data.description.length <= 1024, `${name}/${skill} description exceeds 1024 chars`);
      }
    } finally {
      rmSync(outDir, { recursive: true, force: true });
    }
  }
});

function skillsDirFor(outDir, manifest) {
  const projectFile = (manifest.coreProjectFiles || []).find((file) => file.src === "skills");
  if (projectFile) return join(outDir, projectFile.dst);
  const dir = (manifest.coreDirs || []).find((entry) => entry.src === "skills");
  if (dir) return join(outDir, manifest.harnessDir, dir.dst);
  throw new Error(`harness ${manifest.name} ships no skills directory`);
}

test("doctor passes on a configured project", async () => {
  const root = tempProject();
  try {
    const { manifest } = await buildHarness({
      repoRoot: repoRoot(),
      harnessName: "pi",
      outDir: join(root, "dist"),
    });
    const { applyDistribution } = await import("../core/tools/lib/packager.mjs");
    applyDistribution({ distributionDir: join(root, "dist"), projectRoot: root, manifest });
    // Scaffold memory + config like `config` does.
    const { mkdirSync, copyFileSync, readdirSync } = await import("node:fs");
    const memoryTarget = join(root, "aidlc/spaces/default/memory");
    mkdirSync(memoryTarget, { recursive: true });
    for (const file of readdirSync(join(engineRoot(), "memory"))) {
      if (file.endsWith(".md")) copyFileSync(join(engineRoot(), "memory", file), join(memoryTarget, file));
    }
    writeFileSync(join(root, "aidlc/config.json"), JSON.stringify({ harness: "pi" }));

    const report = runDoctor(root);
    const failures = report.checks.filter((check) => check.status === "fail");
    assert.deepEqual(failures, [], `doctor failures: ${JSON.stringify(failures)}`);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("version is a semver string", () => {
  assert.match(readVersion(), /^\d+\.\d+\.\d+/);
});

// ---------------------------------------------------------------------------
// Runner
// ---------------------------------------------------------------------------

async function main() {
  let passed = 0;
  let failed = 0;
  for (const { name, fn } of tests) {
    try {
      await fn();
      passed += 1;
      process.stdout.write(`PASS ${name}\n`);
    } catch (error) {
      failed += 1;
      process.stdout.write(`FAIL ${name}\n     ${error?.message || error}\n`);
    }
  }
  process.stdout.write(`\n${passed} passed, ${failed} failed\n`);
  process.exit(failed === 0 ? 0 : 1);
}

main();
