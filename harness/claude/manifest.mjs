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
    { src: "phases", dst: "phases" },
    { src: "agents", dst: "agents" },
    { src: "scopes", dst: "scopes" },
    { src: "protocols", dst: "protocols" },
    { src: "knowledge", dst: "knowledge" },
  ],

  coreFiles: [
    { src: "skills/aidlc/SKILL.md", dst: "skills/aidlc/SKILL.md" },
    { src: "skills/aidlc/question-rendering.md", dst: "skills/aidlc/question-rendering.md" },
  ],

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
