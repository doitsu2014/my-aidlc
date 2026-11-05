# Ultracode stage execution

This is the Claude Code recipe for the **Ultracode** section of `SKILL.md`.
It applies to every `run-stage` directive with substantive work. It changes
only how the stage is executed; the engine still owns routing and the directive
still sets the human loop.

## Sequence

1. **Scout the inputs before authoring the script.** Read `lead_agent_file`,
   `stage_file`, each existing `consumes[].path`, the active scope,
   `support_agent_files`, and `reviewer_file` when the directive names one.
   Collect the exact paths and the stage's questions — workflow agents should
   not have to discover them.
2. **Run the question flow in this loop.** Branch on the directive: with
   `answer_policy: "human"`, ask the stage's questions, write them to
   `<record>/<phase>/<stage>/<stage>-questions.md`, and wait for the answers.
   With `answer_policy: "recommended"` (YOLO), auto-pick the recommended answer
   and record it in that file. Either way the file is the workflow's input and
   the source of truth.
3. **Author one workflow for the stage and run it.** Decompose, produce,
   verify adversarially, and account for completeness, as below.
4. **Synthesize at `produce_paths`.** Apply the support agents' changes and the
   reviewer's blocking findings; if the reviewer blocks, run a focused fix pass
   and re-review before you call the stage done.
5. **Report and gate yourself.** Call
   `{{INVOKE}} orchestrate report --stage <slug> --result <outcome>`. When
   `auto_approve` is false, present the approval gate yourself. When it is
   true, skip the gate, report `awaiting-approval`, and let the engine record
   `STAGE_AUTO_APPROVED`. A `review_required` stage always presents the gate.
   Workflow agents do neither.

## Output mapping: no two writers on one file

The engine supplies one path per produced artifact (`produce_paths`), while a
stage may have many independent units — requirements, stories, components,
units of work. Do not point one producer per unit at the same canonical path;
they would collide or write outside it.

- Each producer writes only its own fragment under
  `<directive.record_dir>/<phase>/<stage>/work/<item-id>/`.
- One synthesis agent reads the fragments and writes the canonical
  `produce_paths`. It is the only writer of those paths.
- The reviewer and the completeness critic read both the fragments and the
  canonical artifacts.
- No two concurrent agents write the same file.

Stages that declare `for_each` (for example `code-generation` over
`unit-of-work`) still have one canonical artifact path per artifact: fragments
per unit, one synthesis into each canonical path.

## What the script must do

- **Decompose from the stage, not from a template.** Items are the stage's
  independent units: one per requirement, user story, component, contract,
  unit of work, review dimension, or question the stage body defines. Derive
  the list by reading the stage and its inputs.
- **Pipeline by default.** Run each item through produce → verify with
  `pipeline()`, so one item's verification starts as soon as its production
  ends. Use a barrier only when stage N genuinely needs every result from stage
  N-1, such as the support and synthesis passes.
- **Use the stage's own agents.** The lead agent's persona is the producer's
  voice. Dispatch each `support_agent_files` entry as an independent participant
  over the fragments, and dispatch `reviewer_file` as an independent reviewer
  over the canonical artifacts. Do not adopt all their voices yourself.
- **Verify adversarially.** Give each fragment several distinct lenses (for
  example traceability, testability, contract consistency, and
  security/compliance where relevant), each prompted to refute. A finding
  survives only when it is confirmed; a stage does not ship on an unverified
  artifact.
- **Add a completeness critic.** One final agent reads everything produced and
  answers: what is missing, unread, unverified, or contradictory? Its findings
  become the next round of work, not a footnote.
- **Keep failures visible.** `agent()` returns `null` when an agent is skipped
  or dies. Keep that result with its item and lens, log it, and pass it to the
  critic and the gate; never `filter(Boolean)` it away.
- **Return structured data.** Give every agent a `schema` so the conductor gets
  verdicts and paths, not prose to parse.

```js
export const meta = {
  name: 'aidlc-<stage>',
  description: 'Execute <stage> in the <phase> phase in ultracode',
  phases: [{ title: 'Produce' }, { title: 'Verify' }, { title: 'Synthesize' }],
}

const LEAD = '<directive.lead_agent_file>'
const STAGE_FILE = '<directive.stage_file>'
const QUESTIONS = '<answered or auto-answered questions file>'
const WORK_DIR = '<directive.record_dir>/<phase>/<stage>/work'
// directive.consumes is [{ artifact, path, required }] — pass existing paths.
const CONSUMES = ['<existing consumes[].path>']
const SUPPORT = ['<directive.support_agent_files>']
// Set to null when directive.reviewer_file is null.
const REVIEWER = '<directive.reviewer_file>'
const PRODUCE_PATHS = ['<directive.produce_paths>']

const PRODUCE_SCHEMA = {
  type: 'object',
  properties: {
    fragment: { type: 'string' },
    tracesTo: { type: 'array', items: { type: 'string' } },
  },
  required: ['fragment', 'tracesTo'],
}
const VERDICT_SCHEMA = {
  type: 'object',
  properties: {
    refuted: { type: 'boolean' },
    finding: { type: 'string' },
    blocking: { type: 'boolean' },
  },
  required: ['refuted', 'finding', 'blocking'],
}
const FINDINGS_SCHEMA = {
  type: 'object',
  properties: {
    blocking: { type: 'array', items: { type: 'string' } },
    advisory: { type: 'array', items: { type: 'string' } },
  },
  required: ['blocking', 'advisory'],
}
const SYNTH_SCHEMA = {
  type: 'object',
  properties: {
    paths: { type: 'array', items: { type: 'string' } },
    unproduced: { type: 'array', items: { type: 'string' } },
  },
  required: ['paths', 'unproduced'],
}
const CRITIC_SCHEMA = {
  type: 'object',
  properties: {
    missing: { type: 'array', items: { type: 'string' } },
    followUps: { type: 'array', items: { type: 'string' } },
  },
  required: ['missing', 'followUps'],
}

const LENSES = ['traceability', 'testability', 'contract']
// Derive these from the stage body and its consumed artifacts.
const ITEMS = [{ id: '<item>', fragment: `${WORK_DIR}/<item-id>/fragment.md` }]

const produced = await pipeline(
  ITEMS,
  (item) =>
    agent(
      `You are the stage's lead agent for item ${item.id}. Read your persona ` +
        `at ${LEAD}, the stage body at ${STAGE_FILE}, the answers at ` +
        `${QUESTIONS}, and ${CONSUMES.join(', ')}. Write only ` +
        `${item.fragment}. Return the path and its traceability.`,
      { phase: 'Produce', label: `produce:${item.id}`, schema: PRODUCE_SCHEMA },
    ),
  (result, item) =>
    result
      ? parallel(
          LENSES.map((lens) => () =>
            agent(
              `Try to refute fragment ${item.fragment} through the ${lens} lens. ` +
                `Read ${STAGE_FILE} for what the stage requires. ` +
                `Default to refuted when uncertain.`,
              { phase: 'Verify', label: `verify:${lens}:${item.id}`, schema: VERDICT_SCHEMA },
            ),
          ),
        ).then((verdicts) => ({
          item,
          produced: result,
          verdicts: verdicts.map((verdict, index) => ({ lens: LENSES[index], verdict })),
        }))
      : { item, produced: null, verdicts: [], missing: true },
)

const missingItems = produced.filter((entry) => entry.missing).map((entry) => entry.item.id)
const unverified = produced.flatMap((entry) =>
  entry.verdicts.filter((entryVerdict) => !entryVerdict.verdict)
    .map((entryVerdict) => `${entry.item.id}:${entryVerdict.lens}`),
)
if (missingItems.length) log(`skipped or failed items: ${missingItems.join(', ')}`)
if (unverified.length) log(`unverified lenses: ${unverified.join(', ')}`)

const support = SUPPORT.length
  ? await parallel(
      SUPPORT.map((file, index) => () =>
        agent(
          `You are a support agent for this stage. Read your persona at ` +
            `${file}, the stage body at ${STAGE_FILE}, the fragments under ` +
            `${WORK_DIR}, and ${CONSUMES.join(', ')}. Report what the stage ` +
            `must add or change in your remit.`,
          { phase: 'Verify', label: `support:${index + 1}`, schema: FINDINGS_SCHEMA },
        ),
      ),
    )
  : []

const synthesis = await agent(
  `Synthesize the canonical artifacts at ${PRODUCE_PATHS.join(', ')} from the ` +
    `fragments under ${WORK_DIR}. You are the only writer of those paths. ` +
    `Resolve the verification verdicts and support findings below, and list ` +
    `anything you could not produce.\n\n` +
    `Verdicts: ${JSON.stringify(produced)}\nSupport: ${JSON.stringify(support)}`,
  { phase: 'Synthesize', label: 'synthesis', schema: SYNTH_SCHEMA },
)

const review = REVIEWER
  ? await agent(
      `You are the stage's reviewer. Read your persona at ${REVIEWER}, then ` +
        `the canonical artifacts at ${PRODUCE_PATHS.join(', ')} against ` +
        `${STAGE_FILE}. Report blocking findings and advisory notes.`,
      { phase: 'Verify', label: 'reviewer', schema: FINDINGS_SCHEMA },
    )
  : null

const critic = await agent(
  `Read the fragments and canonical artifacts. Compare them with ${STAGE_FILE}. ` +
    `What is missing, unread, unverified, or contradictory? ` +
    `Items with no produce result: ${JSON.stringify(missingItems)}. ` +
    `Unverified lenses: ${JSON.stringify(unverified)}. ` +
    `Reviewer and synthesis gaps: ${JSON.stringify(review)} ${JSON.stringify(synthesis)}.`,
  { phase: 'Verify', label: 'completeness', schema: CRITIC_SCHEMA },
)

return { stage: STAGE_FILE, produced, support, synthesis, review, critic, missingItems, unverified }
```

## Hard rules

- Agents may write only the canonical `produce_paths`, the per-item fragments
  under `<directive.record_dir>/<phase>/<stage>/work/`, and the questions file.
  Never `state.json`, `audit.log`, the scope files, or anything under
  `native/`.
- Never run `orchestrate` from inside a workflow. The conductor owns that call
  and reports each outcome exactly once.
- Never render a human question or approval gate from inside a workflow. Bring
  the result back and do it in the main loop.
- Workflow scripts are plain JavaScript with no filesystem access and no
  `Date.now()` or `Math.random()`; vary prompts by item index instead.
- Omit `model` and `effort` unless the stage's tier clearly demands otherwise.
  Agents inherit the session's model.
- If the fleet is bounded — top-N items, sampling, or a skipped stage section —
  `log()` what was dropped and repeat it in the stage summary. A null agent
  result is skipped coverage, not a success.
- A null synthesis or reviewer result is not a pass. Retry it, or stop and
  report the failure; never present the gate on artifacts that were not
  synthesized or reviewed.
