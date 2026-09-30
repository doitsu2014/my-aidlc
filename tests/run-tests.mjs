#!/usr/bin/env node
// tests/run-tests.mjs — smoke, unit, and integration tests for my-aidlc.

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
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
} from "../core/tools/lib/graph.mjs";
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

const tests = [];
const test = (name, fn) => tests.push({ name, fn });

function tempProject() {
  return mkdtempSync(join(tmpdir(), "my-aidlc-test-"));
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
  const m = loadMethodology(engineRoot());

  const expressRoot = tempProject();
  const classicRoot = tempProject();
  const offRoot = tempProject();
  try {
    // Scope-level budget (express sets min 0, max 3).
    const express = runNext(expressRoot, m, loadConfig(expressRoot), {
      text: "express change",
      scope: "express",
    });
    assert.equal(express.kind, "run-stage");
    assert.deepEqual(express.question_budget, { min: 0, max: 3, source: "scope" });
    assert.ok(express.protocol_modules.includes("question-flow"));

    // Project-level budget on a scope without its own.
    const classicConfig = loadConfig(classicRoot);
    classicConfig.questionBudget = { min: 2, max: 4 };
    const classic = runNext(classicRoot, m, classicConfig, { text: "build a feature" });
    assert.deepEqual(classic.question_budget, { min: 2, max: 4, source: "project" });

    // max 0 disables the question flow module.
    const offConfig = loadConfig(offRoot);
    offConfig.questionBudget = { min: 0, max: 0 };
    const off = runNext(offRoot, m, offConfig, { text: "build a feature" });
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
