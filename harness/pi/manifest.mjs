// harness/pi/manifest.mjs — the PI Agent distribution row.
//
// Projects core/ into dist/pi/.pi/: the native skill (skills/aidlc/SKILL.md),
// a prompt template (/aidlc), agents, phases, scopes, protocols, and engine
// tools. PI discovers project skills under .pi/skills/ and project prompt
// templates under .pi/prompts/ after project trust is granted.

const manifest = {
  name: "pi",
  productName: "PI Agent",
  harnessDir: ".pi",
  invoke: "/aidlc",
  configNextStep: "run `pi`, then `/aidlc --doctor`",
  orchestratorSkillPath: ".pi/skills/aidlc/SKILL.md",

  // core/<src> -> <harnessDir>/<dst>
  coreDirs: [
    { src: "tools", dst: "tools" },
    { src: "phases", dst: "phases" },
    { src: "agents", dst: "agents" },
    { src: "scopes", dst: "scopes" },
    { src: "protocols", dst: "protocols" },
    { src: "knowledge", dst: "knowledge" },
  ],

  // core/<src> -> <harnessDir>/<dst> for the native skill directory
  coreFiles: [
    { src: "skills/aidlc/SKILL.md", dst: "skills/aidlc/SKILL.md" },
    { src: "skills/aidlc/question-rendering.md", dst: "skills/aidlc/question-rendering.md" },
  ],

  // harness/pi/<src> -> <harnessDir>/<dst>
  harnessFiles: [
    { src: "prompts/aidlc.md", dst: "prompts/aidlc.md" },
    { src: "settings.json", dst: "settings.json" },
  ],

  // harness/pi/<src> -> <projectRoot>/<dst>
  projectFiles: [{ src: "dot-gitignore", dst: ".gitignore" }],

  // core/<src> -> <projectRoot>/<dst>
  coreProjectFiles: [],

  onboarding: { src: "onboarding.md", dst: "AGENTS.md", projectRoot: true },
};

export default manifest;
