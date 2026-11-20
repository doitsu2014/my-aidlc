export const GLOBAL_FLAGS = Object.freeze({ project: 'string', help: 'boolean', version: 'boolean' });
export const TWO_WORD_COMMANDS = Object.freeze([
  'orchestrate', 'config', 'repos', 'deps', 'completion', 'intent', 'knowledge', 'harness',
]);
export const COMMAND_FLAGS = Object.freeze({
  init: { harness: 'string', scope: 'string' },
  onboard: { harness: 'string', scope: 'string' },
  'orchestrate next': { intent: 'string', description: 'string', scope: 'string' },
  'orchestrate report': { result: 'string', approval: 'string', detail: 'string', stage: 'string', 'approved-by': 'string', 'approval-note': 'string' },
  scope: {},
  status: { intent: 'string' },
  doctor: {},
  'intent list': {},
  'intent pause': { detail: 'string' },
  'intent resume': {},
  'intent abandon': { detail: 'string' },
  'config agents': {},
  'repos add': { ref: 'string' },
  'repos pull': { file: 'string' },
  'repos list': {},
  'repos remove': {},
  'deps add': {},
  'deps list': {},
  'deps pull': {},
  'completion zsh': {},
  'knowledge init': {},
  'knowledge sources': { kind: 'string', description: 'string' },
  'knowledge list': {},
  'knowledge show': {},
  'knowledge check': {},
  'knowledge index': {},
});
export const COMMAND_MAX_ARGS = Object.freeze({
  init: 0,
  onboard: 0,
  status: 0,
  doctor: 0,
  'orchestrate next': 0,
  'orchestrate report': 0,
  'repos list': 0,
  'deps list': 0,
  'completion zsh': 0,
  scope: 1,
  'repos pull': 1,
  'repos remove': 1,
  'deps pull': 1,
  'repos add': 2,
  'deps add': 2,
  'config agents': 3,
  'intent list': 0,
  'intent pause': 0,
  'intent resume': 1,
  'intent abandon': 1,
  'knowledge init': 0,
  'knowledge sources': 3,
  'knowledge list': 0,
  'knowledge show': 1,
  'knowledge check': 0,
  'knowledge index': 0,
});
export function commandKey(positionals) {
  return TWO_WORD_COMMANDS.includes(positionals[0]) && positionals[1]
    ? `${positionals[0]} ${positionals[1]}`
    : positionals[0];
}
export function optionTypes() {
  const result = Object.assign(Object.create(null), GLOBAL_FLAGS);
  for (const row of Object.values(COMMAND_FLAGS)) {
    for (const [name, type] of Object.entries(row)) {
      if (result[name] && result[name] !== type) throw new Error(`Option --${name} has conflicting types`);
      result[name] = type;
    }
  }
  return result;
}
