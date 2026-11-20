import { createHash } from 'node:crypto';
import { existsSync, lstatSync, readFileSync, realpathSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { ensureWithin, stageProduces } from './lib.mjs';

const DEFAULT = '.aidlc/intents/{intentId}/{phase}/{stage}/{output}.md';

function fill(template, vars) {
  return template.replaceAll('{intentId}', vars.intentId).replaceAll('{phase}', vars.phase).replaceAll('{stage}', vars.stage).replaceAll('{output}', vars.output).replaceAll('{unitId}', vars.unitId ?? '');
}

export function expectedArtifacts(root, graph, state, stage) {
  return (stageProduces(graph, stage, state.depth || 'full')).map((output) => {
    const meta = graph.outputs?.[output] ?? {};
    const unitId = state.currentUnit || '';
    const template = (meta.path || DEFAULT).includes('{unitId}') || !unitId
      ? (meta.path || DEFAULT)
      : (unitId ? `.aidlc/intents/{intentId}/{phase}/{stage}/{unitId}/{output}.md` : (meta.path || DEFAULT));
    const relPath = fill(template.replace(/\/{unitId}/g, unitId ? `/${unitId}` : ''), { intentId: state.intentId, phase: stage.phase, stage: stage.id, output, unitId });
    const path = ensureWithin(root, join(root, ...relPath.split('/')));
    return { output, kind: meta.kind || 'document', path, relPath };
  });
}

export function artifactPathFor(root, graph, intentId, outputId) {
  const stage = graph.stages.find((item) => item.produces?.includes(outputId));
  if (!stage) return null;
  return expectedArtifacts(root, graph, { intentId }, stage).find((item) => item.output === outputId)?.path ?? null;
}

export function resolveInputPaths(root, graph, intentId, inputs) {
  const copy = { ...inputs, artifacts: (inputs.artifacts ?? []).map((entry) => {
    const path = artifactPathFor(root, graph, intentId, entry.id);
    const relPath = path ? relative(root, path).split(sep).join('/') : null;
    return { ...entry, path: relPath, exists: Boolean(path && existsSync(path)) };
  }) };
  return copy;
}

export function verifyArtifacts(root, expected, { validators = {}, graph, state, stage } = {}) {
  const artifacts = [];
  const problems = [];
  const rootReal = realpathSync(root);
  for (const item of expected) {
    const relPath = item.relPath;
    try {
      if (!existsSync(item.path)) {
        problems.push({ output: item.output, path: relPath, problem: 'missing' });
        continue;
      }
      const stat = lstatSync(item.path);
      if (stat.isSymbolicLink()) {
        problems.push({ output: item.output, path: relPath, problem: 'symbolic link' });
        continue;
      }
      if (!stat.isFile()) {
        problems.push({ output: item.output, path: relPath, problem: 'not a regular file' });
        continue;
      }
      const real = realpathSync(item.path);
      const rel = relative(rootReal, real);
      if (!rel || rel === '..' || rel.startsWith(`..${sep}`) || resolve(rel) === rel && rel.startsWith(sep)) {
        problems.push({ output: item.output, path: relPath, problem: 'outside the project' });
        continue;
      }
      const text = readFileSync(item.path, 'utf8');
      const head = text.slice(0, 2000);
      const na = /^\s*Status:\s*not-applicable\s*$/im.test(head);
      const reasonMatch = head.match(/^\s*Reason:\s*(\S.*)$/im);
      if (na && !reasonMatch) {
        problems.push({ output: item.output, path: relPath, problem: "not-applicable output needs a 'Reason:' line" });
        continue;
      }
      if (!na && !text.trim()) {
        problems.push({ output: item.output, path: relPath, problem: 'empty' });
        continue;
      }
      const extra = validators[item.output]?.(item.path, text, { root, graph, state, stage, intentId: state?.intentId, output: item.output, notApplicable: na }) ?? [];
      for (const problem of extra) problems.push({ output: item.output, path: relPath, problem });
      artifacts.push({
        output: item.output, path: relPath, bytes: stat.size,
        sha256: createHash('sha256').update(text).digest('hex'),
        ...(na ? { notApplicable: true, reason: reasonMatch[1].trim() } : {}),
      });
    } catch (error) {
      problems.push({ output: item.output, path: relPath, problem: error.message });
    }
  }
  return { ok: problems.length === 0, artifacts, problems };
}
