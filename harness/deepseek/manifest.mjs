// harness/deepseek/manifest.mjs — the DeepSeek Harness distribution row.
//
// Projects core/ into dist/deepseek/.dsh/. DSH discovers a project skill at
// <project>/.dsh/skills/<name>/SKILL.md — the `project-dsh` root, rank 100, the
// highest-priority project root — and reads AGENTS.md as its always-on context
// file. A user-invocable skill is reachable as `/<name>` from the Web GUI, the
// TUI, and ACP alike, so the orchestrator answers to `/aidlc` with no separate
// prompt template.
//
// DSH ships no project-scoped native config file: plugins and config patches
// compose from $DSH_HOME/profiles/<name>/cordis.patch.yml, which belongs to the
// user's installation rather than to this project. The harness therefore
// declares no harnessFiles.

const manifest = {
  name: "deepseek",
  productName: "DeepSeek Harness",
  harnessDir: ".dsh",
  invoke: "/aidlc",
  configNextStep: "open DeepSeek Harness in this project and run `/aidlc --doctor`",
  orchestratorSkillPath: ".dsh/skills/aidlc/SKILL.md",

  // core/<src> -> <harnessDir>/<dst>
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

  // core/<src> -> <harnessDir>/<dst> for individual files (none today)
  coreFiles: [],

  // harness/deepseek/<src> -> <harnessDir>/<dst> (none today: DSH config is
  // home-scoped, so there is no per-project native config to ship)
  harnessFiles: [],

  // harness/deepseek/<src> -> <projectRoot>/<dst>
  projectFiles: [{ src: "dot-gitignore", dst: ".gitignore" }],

  // core/<src> -> <projectRoot>/<dst>
  coreProjectFiles: [],

  onboarding: { src: "onboarding.md", dst: "AGENTS.md", projectRoot: true },
};

export default manifest;
