#!/usr/bin/env node
// scripts/lint.mjs — dependency-free structural checks for the methodology
// and engine. Fails on unknown agents, unproduced consumes, invalid scopes,
// and missing phase/stage files.

import { existsSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { loadMethodology } from "../core/tools/lib/graph.mjs";
import { engineRoot, repoRoot } from "../core/tools/lib/version.mjs";

const problems = [];
const note = (message) => problems.push(message);

const coreRoot = engineRoot();
const methodology = loadMethodology(coreRoot);
const agentSlugs = new Set(methodology.agents.map((agent) => agent.slug));
const stageSlugs = new Set(methodology.stages.map((stage) => stage.slug));
const produced = new Set(methodology.stages.flatMap((stage) => stage.produces));
const phaseSlugs = new Set(methodology.phases.map((phase) => phase.slug));
const scopeNames = new Set(methodology.scopes.map((scope) => scope.name));

for (const phase of methodology.phases) {
  if (!existsSync(join(coreRoot, "phases", phase.slug, "phase.md"))) {
    note(`phase ${phase.slug} has no phase.md`);
  }
  if (!["auto", "required"].includes(phase.review)) {
    note(`phase ${phase.slug} has unknown review value "${phase.review}" (use auto or required)`);
  }
}

for (const stage of methodology.stages) {
  if (!phaseSlugs.has(stage.phase)) note(`stage ${stage.slug} has unknown phase ${stage.phase}`);
  if (stage.leadAgent && !agentSlugs.has(stage.leadAgent)) {
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
    note(`always-on stage ${stage.slug} produces no artifacts`);
  }
  if (stage.execution === "CONDITIONAL" && !stage.condition) {
    note(`conditional stage ${stage.slug} has no condition`);
  }
}

for (const scope of methodology.scopes) {
  if (!scope.phases.length) note(`scope ${scope.name} includes no phases`);
  for (const phase of scope.phases) {
    if (!phaseSlugs.has(phase)) note(`scope ${scope.name} names unknown phase ${phase}`);
  }
  for (const slug of [...scope.include, ...scope.skip]) {
    if (!stageSlugs.has(slug)) note(`scope ${scope.name} names unknown stage ${slug}`);
  }
}

// Every declared scope must appear in at least one stage's applicability.
for (const scope of methodology.scopes) {
  const anyApplicable = methodology.stages.some((stage) =>
    stage.scopes.length ? stage.scopes.includes(scope.name) : scope.phases.includes(stage.phase),
  );
  if (!anyApplicable) note(`scope ${scope.name} resolves to zero stages`);
}

// Every agent should be reachable from at least one stage, except agents that
// are part of the conductor's own cross-cutting surface (the adaptive
// composer runs outside any single stage).
const CROSS_CUTTING_AGENTS = new Set(["composer-agent"]);
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
