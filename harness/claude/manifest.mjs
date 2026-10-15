// harness/claude/manifest.mjs — the Claude Code distribution row.
//
// Projects core/ into dist/claude/.claude/ and ships the ambient method import
// at .claude/rules/aidlc.md. Claude discovers skills under .claude/skills/ and
// reads CLAUDE.md as its always-on context file.

const manifest = {
  name: "claude",
  productName: "Claude Code",
  harnessDir: ".claude",
  invoke: "/aidlc",
  configNextStep: "open Claude Code in this project and run `/aidlc --doctor`",
  orchestratorSkillPath: ".claude/skills/aidlc/SKILL.md",

  coreDirs: [
    { src: "tools", dst: "tools" },
    // The methodology: native AWS AI-DLC, fetched by `my-aidlc update`.
    { src: "native", dst: "native" },
    // Space-scope seeds copied into aidlc/spaces/<space>/scopes/ on config.
    { src: "templates/scopes", dst: "templates/scopes" },
    { src: "protocols", dst: "protocols" },
    { src: "knowledge", dst: "knowledge" },
    // Native skill directory: the orchestrator skill and the database skills.
    { src: "skills", dst: "skills" },
  ],

  coreFiles: [],

  harnessFiles: [
    { src: "rules-aidlc.md", dst: "rules/aidlc.md" },
    { src: "settings.json", dst: "settings.json" },
    { src: "settings.local.json.example", dst: "settings.local.json.example" },
  ],

  projectFiles: [{ src: "dot-gitignore", dst: ".gitignore" }],

  coreProjectFiles: [],

  onboarding: { src: "onboarding.md", dst: "CLAUDE.md", projectRoot: true },
};

export default manifest;
