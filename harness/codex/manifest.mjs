// harness/codex/manifest.mjs — the Codex CLI distribution row.
//
// Projects core/ into dist/codex/.codex/. Codex discovers skills at
// <project>/.agents/skills/, so the orchestrator skill lands there via
// coreProjectFiles rather than inside .codex/. AGENTS.md is the context file.

const manifest = {
  name: "codex",
  productName: "Codex CLI",
  harnessDir: ".codex",
  invoke: "$aidlc",
  configNextStep: "run `codex`, then `$aidlc --doctor`",
  orchestratorSkillPath: ".agents/skills/aidlc/SKILL.md",

  coreDirs: [
    { src: "tools", dst: "tools" },
    { src: "phases", dst: "phases" },
    { src: "agents", dst: "agents" },
    { src: "scopes", dst: "scopes" },
    { src: "protocols", dst: "protocols" },
    { src: "knowledge", dst: "knowledge" },
  ],

  coreFiles: [],

  harnessFiles: [{ src: "config.toml", dst: "config.toml" }],

  projectFiles: [{ src: "dot-gitignore", dst: ".gitignore" }],

  // Codex discovers skills at .agents/skills/ (outside .codex/).
  coreProjectFiles: [
    { src: "skills/aidlc/SKILL.md", dst: ".agents/skills/aidlc/SKILL.md" },
    { src: "skills/aidlc/question-rendering.md", dst: ".agents/skills/aidlc/question-rendering.md" },
  ],

  onboarding: { src: "onboarding.md", dst: "AGENTS.md", projectRoot: true },
};

export default manifest;
