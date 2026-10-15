#!/usr/bin/env node
// scripts/lint.mjs — dependency-free structural checks for the methodology
// and engine. Fails on unknown agents, unproduced consumes, invalid scopes,
// missing phase/stage files, and drifts in the fetched native snapshot.

import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { loadMethodology, nativeRoot, readNativeLock } from "../core/tools/lib/graph.mjs";
import { readPin, verifySnapshot } from "../core/tools/lib/native.mjs";
import { parseFrontmatter } from "../core/tools/lib/frontmatter.mjs";
import { engineRoot, repoRoot } from "../core/tools/lib/version.mjs";

const problems = [];
const note = (message) => problems.push(message);

const coreRoot = engineRoot();
const native = nativeRoot(coreRoot);

// The methodology is fetched, not authored: fail if the committed snapshot has
// drifted from the pinned release, or if the pin itself cannot be read.
let pin = null;
try {
  pin = readPin(repoRoot());
  const verified = verifySnapshot(repoRoot(), pin);
  if (!verified.ok) {
    note(
      verified.reason === "missing"
        ? `no native snapshot at ${pin.snapshotDir}/; run: node scripts/fetch-native.mjs`
        : `native snapshot drifted from the pin (${verified.drift.length} file(s)); run: node scripts/fetch-native.mjs`,
    );
  }
} catch (error) {
  note(`aidlc-version.json could not be read: ${error.message}`);
}

const methodology = loadMethodology(coreRoot);
const agentSlugs = new Set(methodology.agents.map((agent) => agent.slug));
const stageSlugs = new Set(methodology.stages.map((stage) => stage.slug));
const produced = new Set(
  methodology.stages.flatMap((stage) => [...stage.produces, ...(stage.optionalProduces || [])]),
);
const phaseSlugs = new Set(methodology.phases.map((phase) => phase.slug));
const scopeNames = new Set(methodology.scopes.map((scope) => scope.name));

const nativeLock = readNativeLock(coreRoot);
if (!nativeLock) note(`no readable native lock at ${join(native, "lock.json")}`);

for (const phase of methodology.phases) {
  if (!existsSync(join(native, "phases", `${phase.slug}.md`))) {
    note(`phase ${phase.slug} has no native phase metadata file`);
  }
  if (!["auto", "required"].includes(phase.review)) {
    note(`phase ${phase.slug} has unknown review value "${phase.review}" (use auto or required)`);
  }
}

for (const slug of phaseSlugs) {
  if (!existsSync(join(native, "stages", slug))) note(`no native stage directory for phase ${slug}`);
}

// The three Initialization stages are owned by the engine's own conductor
// (`lead_agent: orchestrator`), not by an agent persona, and they write the
// record dir rather than a named artifact. Both facts are native.
const RESERVED_LEAD_AGENTS = new Set(["orchestrator"]);
const ENGINE_OWNED_PHASES = new Set(["initialization"]);

for (const stage of methodology.stages) {
  if (!phaseSlugs.has(stage.phase)) note(`stage ${stage.slug} has unknown phase ${stage.phase}`);
  if (stage.leadAgent && !agentSlugs.has(stage.leadAgent) && !RESERVED_LEAD_AGENTS.has(stage.leadAgent)) {
    note(`stage ${stage.slug} has unknown lead agent ${stage.leadAgent}`);
  }
  for (const agent of stage.supportAgents) {
    if (!agentSlugs.has(agent)) note(`stage ${stage.slug} has unknown support agent ${agent}`);
  }
  if (stage.reviewer && !agentSlugs.has(stage.reviewer)) {
    note(`stage ${stage.slug} has unknown reviewer ${stage.reviewer}`);
  }
  if (!["auto", "required"].includes(stage.review)) {
    note(`stage ${stage.slug} has unknown review value "${stage.review}" (use auto or required)`);
  }
  for (const consume of stage.consumes) {
    if (!produced.has(consume.artifact)) {
      note(`stage ${stage.slug} consumes unproduced artifact ${consume.artifact}`);
    }
  }
  for (const required of stage.requiresStage) {
    if (!stageSlugs.has(required)) note(`stage ${stage.slug} requires unknown stage ${required}`);
  }
  if (stage.produces.length === 0 && stage.execution === "ALWAYS") {
    if (!ENGINE_OWNED_PHASES.has(stage.phase) && !RESERVED_LEAD_AGENTS.has(stage.leadAgent)) {
      note(`always-on stage ${stage.slug} produces no artifacts`);
    }
  }
  if (stage.execution === "CONDITIONAL" && !stage.condition) {
    note(`conditional stage ${stage.slug} has no condition`);
  }
}

for (const scope of methodology.scopes) {
  for (const phase of scope.phases) {
    if (!phaseSlugs.has(phase)) note(`scope ${scope.name} names unknown phase ${phase}`);
  }
  for (const slug of [...scope.include, ...scope.skip]) {
    if (!stageSlugs.has(slug)) note(`scope ${scope.name} names unknown stage ${slug}`);
  }
  const overlap = scope.include.filter((slug) => scope.skip.includes(slug));
  if (overlap.length) note(`scope ${scope.name} both includes and skips: ${overlap.join(", ")}`);
}

// Every declared scope must resolve to at least one stage.
for (const scope of methodology.scopes) {
  const applicable = methodology.stages.some((stage) => {
    if (scope.include.length) return scope.include.includes(stage.slug);
    if (scope.skip.includes(stage.slug)) return false;
    if (stage.scopes.length) return stage.scopes.includes(scope.name);
    return scope.phases.includes(stage.phase);
  });
  if (!applicable) note(`scope ${scope.name} resolves to zero stages`);
}

// The space-scope seeds are the team's own workflows. Each must name a real,
// complete partition of the native stage inventory: nothing unknown, nothing
// both included and skipped, and nothing left unstated — a partial list would
// silently fall through to the stage-declared baseline.
const templateDir = join(coreRoot, "templates", "scopes");
const templates = existsSync(templateDir)
  ? readdirSync(templateDir).filter((file) => file.endsWith(".md"))
  : [];
if (templates.length === 0) note("no space-scope seeds in core/templates/scopes");
for (const file of templates) {
  const { data } = parseFrontmatter(readFileSync(join(templateDir, file), "utf8"));
  const name = data.name || file.replace(/\.md$/, "");
  const include = Array.isArray(data.include) ? data.include : [];
  const skip = Array.isArray(data.skip) ? data.skip : [];
  if (include.length === 0) {
    note(`space scope ${name} has no include list; it would fall through to the native baseline`);
    continue;
  }
  const unknown = [...include, ...skip].filter((slug) => !stageSlugs.has(slug));
  if (unknown.length) note(`space scope ${name} names unknown stage(s): ${unknown.join(", ")}`);
  const overlap = include.filter((slug) => skip.includes(slug));
  if (overlap.length) note(`space scope ${name} both includes and skips: ${overlap.join(", ")}`);
  const covered = new Set([...include, ...skip]);
  const missing = [...stageSlugs].filter((slug) => !covered.has(slug));
  if (missing.length) {
    note(`space scope ${name} does not account for ${missing.length} stage(s): ${missing.join(", ")}`);
  }
  if (include.length !== covered.size - skip.length) {
    note(`space scope ${name} duplicates a stage in include`);
  }
}

// Every agent should be reachable from at least one stage, except agents that
// are part of the conductor's own cross-cutting surface (the adaptive
// composer runs outside any single stage).
const CROSS_CUTTING_AGENTS = new Set(["aidlc-composer-agent", "composer-agent"]);;
for (const agent of methodology.agents) {
  if (CROSS_CUTTING_AGENTS.has(agent.slug)) continue;
  const referenced = methodology.stages.some(
    (stage) =>
      stage.leadAgent === agent.slug ||
      stage.supportAgents.includes(agent.slug) ||
      stage.reviewer === agent.slug,
  );
  if (!referenced) note(`agent ${agent.slug} is not referenced by any stage`);
}

// Harness manifests must exist and export a name + harnessDir.
const harnessRoot = join(repoRoot(), "harness");
for (const entry of readdirSync(harnessRoot, { withFileTypes: true })) {
  if (!entry.isDirectory()) continue;
  const manifestPath = join(harnessRoot, entry.name, "manifest.mjs");
  if (!existsSync(manifestPath)) {
    note(`harness ${entry.name} has no manifest.mjs`);
    continue;
  }
  const mod = await import(manifestPath);
  const manifest = mod.default;
  if (!manifest?.name || !manifest?.harnessDir) note(`harness ${entry.name} manifest is incomplete`);
  if (manifest?.name !== entry.name) note(`harness ${entry.name} manifest name is ${manifest?.name}`);
}

if (problems.length) {
  for (const problem of problems) process.stderr.write(`FAIL ${problem}\n`);
  process.stderr.write(`\n${problems.length} problem(s) found.\n`);
  process.exit(1);
}
process.stdout.write(`PASS lint: ${methodology.stages.length} stages, ${methodology.scopes.length} scopes, ${methodology.agents.length} agents\n`);
