#!/usr/bin/env node
// tests/run-tests.mjs — smoke, unit, and integration tests for my-aidlc.

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";

import { parseFrontmatter } from "../core/tools/lib/frontmatter.mjs";
import {
  compileGraph,
  detectScope,
  loadMethodology,
  workflowForScope,
  scopeByName,
  readNativeLock,
} from "../core/tools/lib/graph.mjs";
import {
  isUpstreamPath,
  projectSnapshotFiles,
  readLock,
  readPin,
  sha256,
  verifySnapshot,
  writeSnapshot,
} from "../core/tools/lib/native.mjs";
import { buildHarness, listHarnesses } from "../core/tools/lib/packager.mjs";
import { runDoctor } from "../core/tools/lib/doctor.mjs";
import { runNext, runReport, statusReport, parkDirective } from "../core/tools/lib/orchestrate.mjs";
import {
  loadConfig,
  normalizeBudget,
  normalizeMode,
  normalizeReviewRequired,
  resolveMode,
  resolveQuestionBudget,
  resolveReviewRequired,
  saveConfig,
} from "../core/tools/lib/config.mjs";
import { loadState, readAudit } from "../core/tools/lib/state.mjs";
import {
  SUPPORTED_SHELLS,
  buildCompletionModel,
  completionScript,
  detectShell,
  installCompletion,
  uninstallCompletion,
} from "../core/tools/lib/completion.mjs";
import { engineRoot, repoRoot, readVersion } from "../core/tools/lib/version.mjs";
import {
  HARNESS_DIRS,
  HARNESS_INVOKE,
  detectHarness,
  harnessDir,
  spaceScopesDir,
} from "../core/tools/lib/paths.mjs";

const tests = [];
const test = (name, fn) => tests.push({ name, fn });

function tempProject() {
  return mkdtempSync(join(tmpdir(), "my-aidlc-test-"));
}

/** Copy the seven space-scope seeds into a project, as `config` does. */
function seedSpaceScopes(root) {
  const source = join(engineRoot(), "templates", "scopes");
  const target = spaceScopesDir(root);
  mkdirSync(target, { recursive: true });
  for (const file of readdirSync(source)) {
    if (file.endsWith(".md")) copyFileSync(join(source, file), join(target, file));
  }
}

// A two-phase workflow where `analyze` requires a phase review and `develop` does not.
function phaseReviewMethodology() {
  const stage = (overrides) => ({
    slug: "stage",
    name: "Stage",
    phase: "develop",
    execution: "ALWAYS",
    condition: "",
    leadAgent: null,
    supportAgents: [],
    mode: "inline",
    reviewer: null,
    review: "auto",
    forEach: null,
    workspaceRequires: false,
    questionBudget: null,
    produces: [],
    consumes: [],
    requiresStage: [],
    scopes: [],
    inputs: "",
    outputs: "",
    file: "",
    body: "",
    ...overrides,
  });
  return {
    phases: [
      { slug: "analyze", name: "Analyze", order: 1, review: "required" },
      { slug: "develop", name: "Develop", order: 2, review: "auto" },
    ],
    stages: [
      stage({ slug: "a1", name: "A1", phase: "analyze", produces: ["a1-art"] }),
      stage({ slug: "a2", name: "A2", phase: "analyze", requiresStage: ["a1"], produces: ["a2-art"] }),
      stage({ slug: "d1", name: "D1", phase: "develop", requiresStage: ["a2"], produces: ["d1-art"] }),
    ],
    scopes: [
      {
        name: "classic",
        depth: "Standard",
        keywords: [],
        description: "",
        skeleton: "off",
        reviewCap: "advisory",
        guardPolicy: "relaxed",
        sensors: "on",
        learnings: "on",
        summaryConfirmation: "off",
        mode: null,
        questionBudget: null,
        phases: ["analyze", "develop"],
        include: [],
        skip: [],
        file: "",
        body: "",
      },
    ],
    agents: [],
  };
}

// A minimal two-stage workflow: `reviewed` requires a human gate, `auto` does not.
function reviewMethodology() {
  const stage = (overrides) => ({
    slug: "stage",
    name: "Stage",
    phase: "develop",
    execution: "ALWAYS",
    condition: "",
    leadAgent: null,
    supportAgents: [],
    mode: "inline",
    reviewer: null,
    review: "auto",
    forEach: null,
    workspaceRequires: false,
    questionBudget: null,
    produces: [],
    consumes: [],
    requiresStage: [],
    scopes: [],
    inputs: "",
    outputs: "",
    file: "",
    body: "",
    ...overrides,
  });
  return {
    phases: [{ slug: "develop", name: "Develop", order: 1 }],
    stages: [
      stage({ slug: "reviewed", name: "Reviewed Stage", review: "required", produces: ["reviewed-artifact"] }),
      stage({ slug: "auto", name: "Auto Stage", requiresStage: ["reviewed"], produces: ["auto-artifact"] }),
    ],
    scopes: [
      {
        name: "classic",
        depth: "Standard",
        keywords: [],
        description: "",
        skeleton: "off",
        reviewCap: "advisory",
        guardPolicy: "relaxed",
        sensors: "on",
        learnings: "on",
        summaryConfirmation: "off",
        mode: null,
        questionBudget: null,
        phases: ["develop"],
        include: [],
        skip: [],
        file: "",
        body: "",
      },
    ],
    agents: [],
  };
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

test("methodology loads the five native phases, 33 stages, scopes, and agents", () => {
  const m = loadMethodology(engineRoot());
  assert.equal(m.phases.length, 5);
  assert.deepEqual(
    m.phases.map((p) => p.slug),
    ["initialization", "ideation", "inception", "construction", "operation"],
  );
  assert.equal(m.stages.length, 33);
  assert.equal(m.agents.length, 14);
  // The 11 native baseline scopes ship in the engine; the team's 7 live in a space.
  assert.equal(m.scopes.length, 11);
  assert.ok(m.scopes.every((scope) => scope.source === "native"));
});

test("the native snapshot matches the pin it was fetched from", () => {
  const pin = readPin(repoRoot());
  const lock = readNativeLock(engineRoot());
  assert.ok(lock, "core/native/lock.json is missing");
  assert.equal(lock.repository, pin.repository);
  assert.equal(lock.version, pin.version);
  assert.equal(lock.tag, pin.tag);
  assert.equal(lock.stages, 33);
  assert.equal(lock.agents, 14);
  assert.equal(lock.scopes, 11);
  const verified = verifySnapshot(repoRoot(), pin);
  assert.ok(verified.ok, `snapshot drifted: ${JSON.stringify(verified.drift.slice(0, 3))}`);
});

test("optional natives produces surface in the run directive", () => {
  // `functional-design` declares `optional_produces: [frontend-components]`;
  // the directive must name it so the conductor knows it may write it.
  const root = tempProject();
  try {
    const m = loadMethodology(engineRoot());
    const config = loadConfig(root);
    config.mode = "yolo";
    let directive = runNext(root, m, config, { text: "build an inventory API", scope: "classic" });
    let guard = 0;
    while (directive.stage !== "functional-design" && guard < 60) {
      guard += 1;
      directive = runReport(root, m, config, { stage: directive.stage, result: "awaiting-approval" });
      if (directive.kind !== "run-stage") break;
    }
    assert.equal(directive.stage, "functional-design");
    assert.deepEqual(directive.optional_produces, ["frontend-components"]);
    assert.equal(directive.optional_produce_paths.length, 1);
    assert.ok(directive.optional_produce_paths[0].endsWith("/functional-design/frontend-components.md"));
    assert.ok(directive.produce_paths.some((path) => path.endsWith("/functional-spec.md")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the native grids reproduce the published AI-DLC scope sizes", () => {
  const m = loadMethodology(engineRoot());
  // Sizes documented in awslabs/aidlc-workflows docs/guide/05-scopes-and-depth.md.
  const published = {
    enterprise: 33,
    feature: 33,
    mvp: 23,
    poc: 8,
    bugfix: 9,
    refactor: 10,
    infra: 13,
    "security-patch": 10,
    classic: 18,
  };
  for (const [name, size] of Object.entries(published)) {
    const scope = scopeByName(m, name);
    assert.ok(scope, `missing baseline scope ${name}`);
    const workflow = workflowForScope(m, scope);
    assert.equal(workflow.length, size, `scope ${name} resolved to ${workflow.length} stages`);
  }
});

test("the committed snapshot is exactly what the generator produces", () => {
  // Regression guard: `my-aidlc update` and scripts/fetch-native.mjs must both
  // emit the projection layer (phases/, README.md, ADAPTATION.md), not just the
  // upstream methodology files. This reproduces the committed tree from its own
  // upstream half, with no network access.
  const pin = readPin(repoRoot());
  const dir = join(repoRoot(), pin.snapshotDir);
  const lock = readLock(repoRoot(), pin);
  assert.ok(lock, "core/native/lock.json is missing");

  const methodology = new Map();
  for (const entry of lock.files) {
    if (!isUpstreamPath(entry.path)) continue;
    methodology.set(entry.path, readFileSync(join(dir, entry.path)));
  }
  assert.equal(methodology.size, 33 + 14 + 11 + 9);

  const { files, counts } = projectSnapshotFiles(methodology, pin);
  assert.deepEqual(counts, { stages: 33, agents: 14, scopes: 11, protocols: 9 });
  assert.equal(files.size, lock.files.length, "generator file count differs from the lock");
  for (const entry of lock.files) {
    const generated = files.get(entry.path);
    assert.ok(generated, `generator omitted ${entry.path}`);
    assert.equal(sha256(generated), entry.sha256, `generator differs from the lock for ${entry.path}`);
  }
});

test("a missing native tree fails loudly instead of yielding an empty workflow", () => {
  const root = tempProject();
  try {
    assert.throws(
      () => loadMethodology(root),
      /No native AI-DLC methodology/,
      "loadMethodology should name the missing tree",
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the snapshot writer is deterministic and prunes stale files", () => {
  const root = tempProject();
  try {
    const pin = { ...readPin(repoRoot()), snapshotDir: ".native-snapshot-test" };
    const outDir = join(root, pin.snapshotDir);
    const files = new Map([
      ["stages/initialization/workspace-scaffold.md", Buffer.from("stage\n", "utf8")],
      ["phases/initialization.md", Buffer.from("phase\n", "utf8")],
      ["README.md", Buffer.from("readme\n", "utf8")],
      ["stale/old-stage.md", Buffer.from("gone soon\n", "utf8")],
    ]);
    writeSnapshot(outDir, files, pin, { archiveVerified: true });
    assert.ok(existsSync(join(outDir, "stale/old-stage.md")));

    // A refresh that no longer ships the stage must drop it and its directory
    // rather than leaving a stale file or crashing on rmdir.
    const next = new Map([...files].filter(([path]) => path !== "stale/old-stage.md"));
    const lock = writeSnapshot(outDir, next, pin, { archiveVerified: true });
    assert.ok(!existsSync(join(outDir, "stale/old-stage.md")));
    assert.ok(!existsSync(join(outDir, "stale")), "empty directory should be pruned");
    assert.equal(lock.files.length, 3);

    // Writing the same content twice changes nothing.
    const again = writeSnapshot(outDir, next, pin, { archiveVerified: true });
    assert.deepEqual(again, lock);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("every stage references known agents", () => {
  const m = loadMethodology(engineRoot());
  const known = new Set(m.agents.map((a) => a.slug));
  // Initialization stages are led by the engine's own conductor.
  const reserved = new Set(["orchestrator"]);
  for (const stage of m.stages) {
    const refs = [stage.leadAgent, ...stage.supportAgents, stage.reviewer].filter(Boolean);
    for (const ref of refs) {
      if (reserved.has(ref)) continue;
      assert.ok(known.has(ref), `stage ${stage.slug} references unknown agent ${ref}`);
    }
  }
});

test("a review phase gates at the phase boundary under yolo", () => {
  const root = tempProject();
  try {
    const m = phaseReviewMethodology();
    const config = loadConfig(root);
    config.mode = "yolo";

    const first = runNext(root, m, config, { text: "build a widget" });
    assert.equal(first.stage, "a1");
    assert.equal(first.auto_approve, true);
    const second = runReport(root, m, config, { stage: "a1", result: "awaiting-approval" });
    assert.equal(second.stage, "a2");
    assert.equal(second.auto_approve, true);

    // a2 closes Analyze: the next directive is a phase review, not Develop.
    const gate = runReport(root, m, config, { stage: "a2", result: "awaiting-approval" });
    assert.equal(gate.kind, "ask");
    assert.equal(gate.ask_type, "phase-review");
    assert.equal(gate.phase, "analyze");
    assert.deepEqual(gate.stages, ["a1", "a2"]);
    assert.equal(gate.produce_paths.length, 2);
    assert.ok(readAudit(root).some((row) => row.event === "PHASE_AWAITING_APPROVAL"));

    // Approving the phase continues into Develop.
    const next = runReport(root, m, config, { phase: "analyze", result: "approved" });
    assert.equal(next.kind, "run-stage");
    assert.equal(next.stage, "d1");
    assert.equal(next.auto_approve, true);

    const done = runReport(root, m, config, { stage: "d1", result: "awaiting-approval" });
    assert.equal(done.kind, "done");
    const audit = readAudit(root);
    assert.ok(audit.some((row) => row.event === "PHASE_APPROVED"));
    assert.ok(audit.some((row) => row.event === "WORKFLOW_COMPLETED"));
    assert.ok(!audit.some((row) => row.event === "PHASE_REJECTED"));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("requesting changes on a phase review re-opens the phase", () => {
  const root = tempProject();
  try {
    const m = phaseReviewMethodology();
    const config = loadConfig(root);
    config.mode = "yolo";
    runNext(root, m, config, { text: "build a widget" });
    runReport(root, m, config, { stage: "a1", result: "awaiting-approval" });
    const gate = runReport(root, m, config, { stage: "a2", result: "awaiting-approval" });
    assert.equal(gate.ask_type, "phase-review");

    const reopened = runReport(root, m, config, {
      phase: "analyze",
      result: "rejected",
      reason: "requirements are vague",
    });
    assert.equal(reopened.kind, "run-stage");
    assert.equal(reopened.stage, "a1");
    const state = loadState(root);
    assert.equal(state.stages.a1.status, "active");
    assert.equal(state.stages.a2.status, "pending");
    assert.equal(state.stages.d1, undefined);
    const rejected = readAudit(root).find((row) => row.event === "PHASE_REJECTED");
    assert.equal(rejected.reason, "requirements are vague");

    // Re-running the phase reaches the gate again.
    runReport(root, m, config, { stage: "a1", result: "awaiting-approval" });
    const again = runReport(root, m, config, { stage: "a2", result: "awaiting-approval" });
    assert.equal(again.ask_type, "phase-review");
    assert.equal(again.phase, "analyze");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("phase review also gates in normal mode", () => {
  const root = tempProject();
  try {
    const m = phaseReviewMethodology();
    const config = loadConfig(root);
    const first = runNext(root, m, config, { text: "build a widget" });
    assert.equal(first.execution_mode, "normal");
    const stageGate = runReport(root, m, config, {
      stage: "a1",
      result: "awaiting-approval",
    });
    assert.equal(stageGate.ask_type, "stage-approval");
    const second = runReport(root, m, config, { stage: "a1", result: "approved" });
    assert.equal(second.stage, "a2");
    const stageGate2 = runReport(root, m, config, {
      stage: "a2",
      result: "awaiting-approval",
    });
    assert.equal(stageGate2.ask_type, "stage-approval");
    const phaseGate = runReport(root, m, config, { stage: "a2", result: "approved" });
    assert.equal(phaseGate.ask_type, "phase-review");
    assert.equal(phaseGate.phase, "analyze");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("a project-wide review toggle gates every phase", () => {
  const root = tempProject();
  try {
    const m = phaseReviewMethodology();
    m.phases.forEach((phase) => {
      phase.review = "auto";
    });
    const config = loadConfig(root);
    config.mode = "yolo";
    config.reviewRequired = true;

    const first = runNext(root, m, config, { text: "build a widget" });
    assert.equal(first.stage, "a1");
    const second = runReport(root, m, config, { stage: "a1", result: "awaiting-approval" });
    assert.equal(second.stage, "a2");
    const gate = runReport(root, m, config, { stage: "a2", result: "awaiting-approval" });
    assert.equal(gate.ask_type, "phase-review");
    assert.equal(gate.phase, "analyze");

    // The toggle also gates Develop, the phase with no frontmatter flag.
    const next = runReport(root, m, config, { phase: "analyze", result: "approved" });
    assert.equal(next.stage, "d1");
    const doneGate = runReport(root, m, config, { stage: "d1", result: "awaiting-approval" });
    assert.equal(doneGate.ask_type, "phase-review");
    assert.equal(doneGate.phase, "develop");
    const done = runReport(root, m, config, { phase: "develop", result: "approved" });
    assert.equal(done.kind, "done");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("every stage resolves a review policy", () => {
  const m = loadMethodology(engineRoot());
  for (const stage of m.stages) {
    assert.ok(
      ["auto", "required"].includes(stage.review),
      `stage ${stage.slug} has review=${stage.review}`,
    );
  }
});

test("every phase resolves a review policy", () => {
  const m = loadMethodology(engineRoot());
  for (const phase of m.phases) {
    assert.ok(
      ["auto", "required"].includes(phase.review),
      `phase ${phase.slug} has review=${phase.review}`,
    );
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

test("space scopes layer over the native baseline and state their own workflow", () => {
  const root = tempProject();
  try {
    seedSpaceScopes(root);
    const m = loadMethodology(engineRoot(), { spaceScopesDir: spaceScopesDir(root) });

    // The team's seven replace the native baseline of the same name.
    const classic = scopeByName(m, "classic");
    assert.equal(classic.source, "space");
    assert.equal(workflowForScope(m, classic).length, 33);

    const bugfix = scopeByName(m, "bugfix");
    assert.equal(bugfix.source, "space");
    assert.equal(workflowForScope(m, bugfix).length, 10);

    // Native-only scopes stay available underneath.
    const enterprise = scopeByName(m, "enterprise");
    assert.equal(enterprise.source, "native");
    assert.equal(workflowForScope(m, enterprise).length, 33);

    // `include` is authoritative: a stage the native grid runs for a scope is
    // still skipped when a space scope leaves it out.
    const poc = workflowForScope(m, scopeByName(m, "poc")).map((s) => s.slug);
    assert.deepEqual(poc, [
      "workspace-scaffold",
      "workspace-detection",
      "state-init",
      "intent-capture",
      "feasibility",
      "requirements-analysis",
      "code-generation",
    ]);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("every space-scope seed accounts for the whole native inventory", () => {
  const m = loadMethodology(engineRoot());
  const all = new Set(m.stages.map((stage) => stage.slug));
  const dir = join(engineRoot(), "templates", "scopes");
  const files = readdirSync(dir).filter((file) => file.endsWith(".md"));
  assert.equal(files.length, 7, "expected the seven original workflow profiles");
  for (const file of files) {
    const { data } = parseFrontmatter(readFileSync(join(dir, file), "utf8"));
    const include = data.include || [];
    const skip = data.skip || [];
    const covered = new Set([...include, ...skip]);
    assert.equal(covered.size, all.size, `${file} covers ${covered.size} of ${all.size} stages`);
    assert.equal(include.filter((s) => skip.includes(s)).length, 0, `${file} overlaps include and skip`);
    for (const slug of covered) assert.ok(all.has(slug), `${file} names unknown stage ${slug}`);
  }
});

test("classic scope runs all 33 native stages in phase order", () => {
  const m = loadMethodology(engineRoot());
  const workflow = workflowForScope(m, scopeByName(m, "classic"));
  assert.equal(workflow.length, 18); // the native baseline grid
  const phaseOrder = { initialization: 1, ideation: 2, inception: 3, construction: 4, operation: 5 };
  let last = 0;
  for (const stage of workflow) {
    const order = phaseOrder[stage.phase];
    assert.ok(order >= last, `phase order regressed at ${stage.slug}`);
    last = order;
  }
});

test("express scope is lighter than classic and skips the design pass", () => {
  const m = loadMethodology(engineRoot());
  const express = workflowForScope(m, scopeByName(m, "express")).map((s) => s.slug);
  assert.ok(express.length < 18);
  assert.ok(!express.includes("domain-design"));
  assert.ok(!express.includes("user-stories"));
  assert.ok(express.includes("code-generation"));
  assert.ok(express.includes("deployment-execution"));
});

test("scope detection matches keywords and falls back to classic", () => {
  const m = loadMethodology(engineRoot());
  assert.equal(detectScope(m, "fix a bug in checkout"), "bugfix");
  assert.equal(detectScope(m, "quick express change"), "express");
  assert.equal(detectScope(m, "build something nobody has words for"), "classic");
});

test("compiled graph records the native pin and a scope grid", () => {
  const graph = compileGraph(engineRoot());
  assert.equal(graph.phases.length, 5);
  assert.equal(graph.stages.length, 33);
  assert.ok(graph.native, "compiled graph should record the native release");
  assert.equal(graph.native.stages, 33);
  assert.equal(graph.scopeGrid.classic.length, 18);
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
    // Native AI-DLC starts every scope at the Initialization phase.
    assert.equal(first.stage, "workspace-scaffold");
    assert.equal(first.phase, "initialization");
    assert.equal(first.gate, true);
    assert.equal(first.stage_file, ".pi/native/stages/initialization/workspace-scaffold.md");
    assert.ok(first.record_dir.startsWith("aidlc/spaces/default/intents/"));

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
    // A conditional stage that does not apply reports `skipped` with a reason;
    // reporting it without one is refused.
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
      // The methodology is the fetched native snapshot, not hand-authored files.
      assert.ok(existsSync(join(tree, "native/stages/ideation/intent-capture.md")));
      assert.ok(existsSync(join(tree, "native/agents/aidlc-product-agent.md")));
      assert.ok(existsSync(join(tree, "native/scopes/classic.md")));
      assert.ok(existsSync(join(tree, "native/lock.json")));
      // The team's workflow scopes ship as seeds for a project's space.
      assert.ok(existsSync(join(tree, "templates/scopes/classic.md")));
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

test("the deepseek harness targets DSH discovery paths", async () => {
  assert.ok(listHarnesses(repoRoot()).includes("deepseek"), "deepseek harness is not listed");
  assert.equal(HARNESS_DIRS.deepseek, ".dsh");
  assert.equal(HARNESS_INVOKE.deepseek, "/aidlc");
  assert.equal(harnessDir(tempProject(), "deepseek"), ".dsh");

  const outDir = join(tempProject(), "dist-deepseek");
  try {
    const { manifest } = await buildHarness({
      repoRoot: repoRoot(),
      harnessName: "deepseek",
      outDir,
    });
    assert.equal(manifest.name, "deepseek");
    assert.equal(manifest.harnessDir, ".dsh");
    assert.equal(manifest.invoke, "/aidlc");
    assert.equal(manifest.orchestratorSkillPath, ".dsh/skills/aidlc/SKILL.md");

    // DSH config is home-scoped, so the harness ships no native config file.
    assert.deepEqual(manifest.harnessFiles, []);

    // The orchestrator is a top-level bundle under the rank-100 project root.
    const skill = readFileSync(join(outDir, ".dsh/skills/aidlc/SKILL.md"), "utf8");
    assert.ok(!skill.includes("{{"), "deepseek skill still contains tokens");
    assert.match(skill, /^---\nname: aidlc\n/);
    assert.match(skill, /\/aidlc orchestrate next/);

    assert.ok(existsSync(join(outDir, ".dsh/native/stages/ideation/intent-capture.md")));
    assert.ok(existsSync(join(outDir, ".dsh/templates/scopes/classic.md")));
    assert.ok(existsSync(join(outDir, "AGENTS.md")), "deepseek onboarding is missing");
    assert.ok(existsSync(join(outDir, ".dsh/version.json")));
  } finally {
    rmSync(outDir, { recursive: true, force: true });
  }
});

test("applying deepseek writes .dsh, merges AGENTS.md, and drives .dsh paths", async () => {
  const root = tempProject();
  try {
    const outDir = join(root, "dist");
    const { manifest } = await buildHarness({
      repoRoot: repoRoot(),
      harnessName: "deepseek",
      outDir,
    });
    // Pre-existing instructions must survive the managed-block merge.
    writeFileSync(join(root, "AGENTS.md"), "# Project notes\n\nKeep me.\n", "utf8");

    const { applyDistribution } = await import("../core/tools/lib/packager.mjs");
    applyDistribution({ distributionDir: outDir, projectRoot: root, manifest });

    assert.ok(existsSync(join(root, ".dsh/skills/aidlc/SKILL.md")));
    assert.ok(existsSync(join(root, ".dsh/tools/my-aidlc.mjs")));
    const agents = readFileSync(join(root, "AGENTS.md"), "utf8");
    assert.match(agents, /Keep me\./);
    assert.match(agents, /# BEGIN my-aidlc:deepseek/);
    assert.match(agents, /# END my-aidlc:deepseek/);

    saveConfig(root, { harness: "deepseek" });
    assert.equal(detectHarness(root), "deepseek");

    // The run directive resolves stage and agent files under .dsh/, not .pi/.
    const m = loadMethodology(engineRoot());
    const directive = runNext(root, m, loadConfig(root), { text: "build a widget" });
    assert.equal(directive.kind, "run-stage");
    assert.ok(directive.stage_file.startsWith(".dsh/native/stages/"), directive.stage_file);
    assert.ok(
      directive.lead_agent_file === null || directive.lead_agent_file.startsWith(".dsh/native/agents/"),
      directive.lead_agent_file,
    );
    for (const file of directive.support_agent_files) {
      assert.ok(file.startsWith(".dsh/native/agents/"), file);
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("detectHarness honours a configured harness and falls back by directory", () => {
  const root = tempProject();
  try {
    // Nothing installed: the default projection is PI.
    assert.equal(detectHarness(root), "pi");

    mkdirSync(join(root, HARNESS_DIRS.deepseek), { recursive: true });
    assert.equal(detectHarness(root), "deepseek");

    // An explicit config entry outranks directory presence.
    saveConfig(root, { harness: "claude" });
    assert.equal(detectHarness(root), "claude");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("question budget resolves with stage > scope > project > default precedence", () => {
  assert.deepEqual(normalizeBudget(undefined), { min: 0, max: 5 });
  assert.deepEqual(normalizeBudget(3), { min: 0, max: 3 });
  assert.deepEqual(normalizeBudget("off"), { min: 0, max: 0 });
  assert.deepEqual(normalizeBudget({ min: 4, max: 2 }), { min: 4, max: 4 });
  assert.deepEqual(normalizeBudget({ max: 4 }), { min: 0, max: 4 });

  const scope = { min: 0, max: 3 };
  const config = { min: 1, max: 9 };
  assert.equal(resolveQuestionBudget({}).source, "default");
  assert.equal(resolveQuestionBudget({ config }).source, "project");
  assert.equal(resolveQuestionBudget({ scope, config }).source, "scope");
  assert.equal(
    resolveQuestionBudget({ stage: { max: 2 }, scope, config }).source,
    "stage",
  );
  assert.deepEqual(resolveQuestionBudget({ stage: { max: 2 }, scope, config }), {
    min: 0,
    max: 2,
    source: "stage",
  });
});

test("run-stage directives carry the effective question budget", () => {
  const expressRoot = tempProject();
  const classicRoot = tempProject();
  const offRoot = tempProject();
  try {
    // Scope-level budget: the space `express` profile pins min 0, max 3.
    seedSpaceScopes(expressRoot);
    const expressMethodology = loadMethodology(engineRoot(), {
      spaceScopesDir: spaceScopesDir(expressRoot),
    });
    const express = runNext(expressRoot, expressMethodology, loadConfig(expressRoot), {
      text: "express change",
      scope: "express",
    });
    assert.equal(express.kind, "run-stage");
    assert.deepEqual(express.question_budget, { min: 0, max: 3, source: "scope" });
    assert.ok(express.protocol_modules.includes("question-flow"));

    // Project-level budget on a scope without its own.
    const classicConfig = loadConfig(classicRoot);
    classicConfig.questionBudget = { min: 2, max: 4 };
    const classic = runNext(classicRoot, loadMethodology(engineRoot()), classicConfig, {
      text: "build a feature",
    });
    assert.deepEqual(classic.question_budget, { min: 2, max: 4, source: "project" });

    // max 0 disables the question flow module.
    const offConfig = loadConfig(offRoot);
    offConfig.questionBudget = { min: 0, max: 0 };
    const off = runNext(offRoot, loadMethodology(engineRoot()), offConfig, { text: "build a feature" });
    assert.deepEqual(off.question_budget, { min: 0, max: 0, source: "project" });
    assert.ok(!off.protocol_modules.includes("question-flow"));
  } finally {
    for (const root of [expressRoot, classicRoot, offRoot]) {
      rmSync(root, { recursive: true, force: true });
    }
  }
});

test("project config persists the question budget", () => {
  const root = tempProject();
  try {
    // Mirrors `my-aidlc config --questions-min 1 --questions-max 3`.
    saveConfig(root, { questionBudget: { min: 1, max: 3 } });
    const config = loadConfig(root);
    assert.deepEqual(config.questionBudget, { min: 1, max: 3 });
    // A later update is clamped and normalised by loadConfig.
    saveConfig(root, { questionBudget: { min: 5, max: 1 } });
    assert.deepEqual(loadConfig(root).questionBudget, { min: 5, max: 5 });
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("execution mode resolves scope > project > default", () => {
  assert.equal(normalizeMode("YOLO"), "yolo");
  assert.equal(normalizeMode("nonsense"), "normal");
  assert.deepEqual(resolveMode({}), { mode: "normal", source: "default" });
  assert.deepEqual(resolveMode({ config: "yolo" }), { mode: "yolo", source: "project" });
  assert.deepEqual(resolveMode({ scope: "normal", config: "yolo" }), {
    mode: "normal",
    source: "scope",
  });
});

test("the phase-review toggle resolves scope > project > default", () => {
  assert.equal(normalizeReviewRequired(undefined), false);
  assert.equal(normalizeReviewRequired("on"), true);
  assert.equal(normalizeReviewRequired("off"), false);
  assert.equal(normalizeReviewRequired(true), true);
  assert.equal(normalizeReviewRequired("banana"), false);
  assert.deepEqual(resolveReviewRequired({}), { required: false, source: "default" });
  assert.deepEqual(resolveReviewRequired({ config: true }), {
    required: true,
    source: "project",
  });
  assert.deepEqual(resolveReviewRequired({ scope: false, config: true }), {
    required: false,
    source: "scope",
  });
});

test("project config persists the phase-review toggle", () => {
  const root = tempProject();
  try {
    assert.equal(loadConfig(root).reviewRequired, false);
    saveConfig(root, { reviewRequired: true });
    assert.equal(loadConfig(root).reviewRequired, true);
    saveConfig(root, { reviewRequired: "off" });
    assert.equal(loadConfig(root).reviewRequired, false);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("yolo mode skips questions and auto-approves gates, audibly", () => {
  const root = tempProject();
  try {
    const m = loadMethodology(engineRoot());
    const config = loadConfig(root);
    config.mode = "yolo";

    const first = runNext(root, m, config, { text: "build an inventory API" });
    assert.equal(first.kind, "run-stage");
    assert.equal(first.execution_mode, "yolo");
    assert.equal(first.mode_source, "project");
    assert.equal(first.auto_approve, true);
    assert.equal(first.answer_policy, "recommended");
    assert.deepEqual(first.question_budget, { min: 0, max: 0, source: "mode" });
    assert.ok(!first.protocol_modules.includes("question-flow"));

    let directive = first;
    let guard = 0;
    while (directive.kind !== "done" && guard < 100) {
      guard += 1;
      assert.notEqual(directive.kind, "ask", "yolo mode must never present a gate");
      assert.equal(directive.kind, "run-stage");
      // A conductor reports artifacts ready; the engine auto-approves and advances.
      directive = runReport(root, m, config, {
        stage: directive.stage,
        result: "awaiting-approval",
      });
    }
    assert.equal(directive.kind, "done");
    assert.ok(guard < 100);

    const audit = readAudit(root);
    const auto = audit.filter((row) => row.event === "STAGE_AUTO_APPROVED");
    assert.ok(auto.length > 0, "expected STAGE_AUTO_APPROVED audit rows");
    assert.ok(!audit.some((row) => row.event === "STAGE_APPROVED"));
    const state = loadState(root);
    assert.equal(Object.values(state.stages).every((r) => r.status === "complete"), true);
    assert.equal(state.currentStage, null);

    // status labels auto-approved stages as auto-completed.
    const report = statusReport(root, m, config);
    assert.equal(report.mode, "yolo");
    assert.equal(report.stages.every((stage) => stage.autoApproved), true);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("a review stage still gates under yolo while other stages auto-approve", () => {
  const root = tempProject();
  try {
    const m = reviewMethodology();
    const config = loadConfig(root);
    config.mode = "yolo";

    // The review stage keeps its gate even though the project is in yolo mode.
    const first = runNext(root, m, config, { text: "build a widget" });
    assert.equal(first.kind, "run-stage");
    assert.equal(first.stage, "reviewed");
    assert.equal(first.execution_mode, "yolo");
    assert.equal(first.review_required, true);
    assert.equal(first.auto_approve, false);

    const gate = runReport(root, m, config, {
      stage: first.stage,
      result: "awaiting-approval",
    });
    assert.equal(gate.kind, "ask");
    assert.equal(gate.ask_type, "stage-approval");
    assert.equal(gate.review_required, true);
    assert.match(gate.question, /Review and approve/);
    assert.deepEqual(gate.produce_paths, [
      `${first.record_dir}/develop/reviewed/reviewed-artifact.md`,
    ]);
    assert.ok(
      !readAudit(root).some((row) => row.event === "STAGE_AUTO_APPROVED"),
      "the review stage must not be auto-approved",
    );

    // Approving advances to the next stage, which yolo auto-approves as usual.
    const second = runReport(root, m, config, { stage: first.stage, result: "approved" });
    assert.equal(second.kind, "run-stage");
    assert.equal(second.stage, "auto");
    assert.equal(second.review_required, false);
    assert.equal(second.auto_approve, true);
    const done = runReport(root, m, config, { stage: second.stage, result: "awaiting-approval" });
    assert.equal(done.kind, "done");

    const audit = readAudit(root);
    assert.ok(audit.some((row) => row.event === "STAGE_APPROVED"));
    assert.ok(audit.some((row) => row.event === "STAGE_AUTO_APPROVED"));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("normal mode still presents a gate after awaiting-approval", () => {
  const root = tempProject();
  try {
    const m = loadMethodology(engineRoot());
    const config = loadConfig(root);
    const first = runNext(root, m, config, { text: "build a feature" });
    assert.equal(first.execution_mode, "normal");
    assert.equal(first.auto_approve, false);
    const gate = runReport(root, m, config, {
      stage: first.stage,
      result: "awaiting-approval",
    });
    assert.equal(gate.kind, "ask");
    assert.equal(gate.ask_type, "stage-approval");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("help renders and documents modes, questions, and completion", () => {
  const cli = join(repoRoot(), "core/tools/my-aidlc.mjs");
  const out = execFileSync(process.execPath, [cli, "help"], { encoding: "utf8" });
  assert.match(out, /COMMANDS/);
  assert.match(out, /MODES/);
  assert.match(out, /yolo/);
  assert.match(out, /STAGE_AUTO_APPROVED/);
  assert.match(out, /--mode/);
  assert.match(out, /--questions-max/);
  assert.match(out, /completion/);
});

test("top-level alias flags resolve instead of falling through to help", () => {
  const cli = join(repoRoot(), "core/tools/my-aidlc.mjs");
  const run = (args) => execFileSync(process.execPath, [cli, ...args], { encoding: "utf8" });

  // A bare invocation still prints help.
  assert.match(run([]), /USAGE/);

  // Each alias must reach its command, not the help fallback. `status` and
  // `config` print their own heading; `doctor` exits non-zero on a repo root
  // with no workspace, so accept either outcome and check the heading only.
  assert.doesNotMatch(run(["--status"]), /USAGE/);
  assert.doesNotMatch(run(["--config"]), /USAGE/);
  const doctor = spawnSync(process.execPath, [cli, "--doctor"], { encoding: "utf8" });
  assert.match(`${doctor.stdout}${doctor.stderr}`, /doctor/);
  assert.doesNotMatch(`${doctor.stdout}`, /USAGE/);

  // A real unknown command still fails loudly.
  assert.throws(() => run(["definitely-not-a-command"]), /Command failed|Unknown command/);
});

test("completion output includes the mode flag and its values", () => {
  const cli = join(repoRoot(), "core/tools/my-aidlc.mjs");
  for (const shell of ["bash", "zsh", "fish", "powershell"]) {
    const out = execFileSync(process.execPath, [cli, "completion", shell], { encoding: "utf8" });
    assert.ok(out.length > 0, `${shell} completion is empty`);
    assert.match(out, /mode/, `${shell} completion omits the mode flag`);
    assert.match(out, /yolo/, `${shell} completion omits the yolo value`);
  }
});

test("completion status detects a missing, current, or stale install", () => {
  const home = tempProject();
  try {
    return import("../core/tools/lib/completion.mjs").then((completion) => {
      const model = completion.buildCompletionModel(
        loadMethodology(engineRoot()),
        listHarnesses(repoRoot()),
        "my-aidlc",
      );
      assert.ok(model.groups.completion.includes("status"));

      assert.equal(completion.completionStatus({ shell: "zsh", model, home }).installed, false);

      completion.installCompletion({ shell: "zsh", model, home });
      const fresh = completion.completionStatus({ shell: "zsh", model, home });
      assert.equal(fresh.installed, true);
      assert.equal(fresh.upToDate, true);
      assert.equal(fresh.stale, false);

      writeFileSync(fresh.scriptPath, "# stale\n", "utf8");
      assert.equal(completion.completionStatus({ shell: "zsh", model, home }).stale, true);
    });
  } finally {
    rmSync(home, { recursive: true, force: true });
  }
});

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
// Unit: shell completion
// ---------------------------------------------------------------------------

test("completion model mirrors the live methodology", () => {
  const m = loadMethodology(engineRoot());
  const model = buildCompletionModel(m, ["pi", "claude", "codex"]);
  assert.deepEqual(model.values["--scope"].slice().sort(), m.scopes.map((s) => s.name).sort());
  assert.deepEqual(model.values["--stage"].slice().sort(), m.stages.map((s) => s.slug).sort());
  assert.deepEqual(model.values["--harness"], ["pi", "claude", "codex"]);
  assert.ok(model.commands.includes("orchestrate"));
  assert.ok(model.groups.orchestrate.includes("next"));
  assert.equal(detectShell({ SHELL: "/bin/zsh" }), "zsh");
  assert.equal(detectShell({ SHELL: "/usr/bin/fish" }), "fish");
  assert.equal(detectShell({ SHELL: "/bin/sh" }), null);
});

test("completion scripts are generated for every supported shell", () => {
  const m = loadMethodology(engineRoot());
  const model = buildCompletionModel(m, ["pi", "claude", "codex"]);
  for (const shell of SUPPORTED_SHELLS) {
    const script = completionScript(shell, model);
    assert.ok(script.includes("my-aidlc"), `${shell} script names the command`);
    assert.ok(script.includes("orchestrate"), `${shell} script lists orchestrate`);
    assert.ok(script.includes(model.list.stages[0]), `${shell} script lists a stage`);
    // Every config flag must reach every shell (fish renders long flags as `-l name`).
    for (const flag of model.options.config) {
      const rendered = shell === "fish" ? `-l ${flag.slice(2)}` : flag;
      assert.ok(script.includes(rendered), `${shell} script omits ${flag}`);
    }
  }
  assert.throws(() => completionScript("tcsh", model), /Unsupported shell/);
});

test("completion install is idempotent and uninstall reverses it", () => {
  const m = loadMethodology(engineRoot());
  const model = buildCompletionModel(m, ["pi", "claude", "codex"]);
  const home = tempProject();
  try {
    const first = installCompletion({ shell: "zsh", model, home });
    assert.ok(existsSync(first.scriptPath));
    const second = installCompletion({ shell: "zsh", model, home });
    assert.equal(second.rcUpdated, false, "second install does not duplicate the rc block");
    const rc = readFileSync(join(home, ".zshrc"), "utf8");
    assert.equal(rc.split("# >>> my-aidlc completion >>>").length - 1, 1);
    const removed = uninstallCompletion({ shell: "zsh", home });
    assert.equal(removed.removedScript, true);
    assert.equal(existsSync(first.scriptPath), false);
    assert.equal(readFileSync(join(home, ".zshrc"), "utf8").includes("my-aidlc completion"), false);
  } finally {
    rmSync(home, { recursive: true, force: true });
  }
});

test("generated bash and zsh scripts pass a syntax check", () => {
  const m = loadMethodology(engineRoot());
  const model = buildCompletionModel(m, ["pi", "claude", "codex"]);
  for (const [shell, binary] of [
    ["bash", "bash"],
    ["zsh", "zsh"],
  ]) {
    const probe = spawnSync(binary, ["--version"], { stdio: "ignore" });
    if (probe.error) continue;
    const dir = tempProject();
    try {
      const file = join(dir, shell === "bash" ? "my-aidlc.bash" : "_my-aidlc");
      writeFileSync(file, completionScript(shell, model));
      const result = spawnSync(binary, ["-n", file]);
      assert.equal(result.status, 0, `${shell} syntax: ${result.stderr}`);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  }
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
