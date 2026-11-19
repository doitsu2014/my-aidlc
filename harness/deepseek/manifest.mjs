// DeepSeek Harness payload metadata. The source tree is copied into
// .dsh/skills/aidlc and the host discovers SKILL.md as /aidlc.
const manifest = {
  name: "deepseek",
  productName: "DeepSeek Harness",
  harnessDir: ".dsh",
  invoke: "/aidlc",
  configNextStep: "Open DeepSeek Harness in this project and run /aidlc --doctor.",
  orchestratorSkillPath: ".dsh/skills/aidlc/SKILL.md",
};

export default manifest;
