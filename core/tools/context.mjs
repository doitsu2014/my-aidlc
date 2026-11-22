// buildDirectiveContext is synchronous, called once per orchestrate next that selects a stage, before any write.
// input: { root, graph, config, state, stage, intentDir, isNewIntent, expectedArtifacts, artifactPathFor }
import { resolve } from 'node:path';
import { existsSync, readFileSync } from 'node:fs';
import { inspectSources, loadKnowledgeManifest, readRecords, expectedRecordFrontmatter, recordPath } from './knowledge.mjs';
import { engineRoot } from './lib.mjs';
import { join } from 'node:path';
import { codebaseMapStatus, requiredProjectArtifacts, sourceStatus } from './onboard.mjs';
import { artifactPathFor } from './artifacts.mjs';
import { registryProblems, unitIdsFromFile, workUnitProblems } from './ids.mjs';

export function buildDirectiveContext(input) {
  try {
    const { root, state, stage } = input;
    const granted = stage.inputs?.context ?? ['request', 'project', 'sources', 'knowledge', 'profile', 'repos'];
    const result = { schemaVersion: 1, projectRoot: resolve(root), granted, warnings: [] };
    if (granted.includes('request') && state?.intentId) {
      result.request = { path: `.aidlc/intents/${state.intentId}/intent.md`, provenance: 'User-provided' };
    }
    if (granted.includes('project') || granted.includes('sources')) {
      try {
        const loaded = loadKnowledgeManifest(root);
        const inspected = inspectSources(root, loaded.manifest);
        if (granted.includes('project')) {
          result.project = inspected.project ? { ...inspected.project, provenance: 'User-provided' } : null;
          if (!inspected.project) result.warnings.push('No project source is declared in aidlc.knowledge.json');
          else if (inspected.project.path) {
            try { result.warnings.push(...projectFileWarnings(root, inspected.project.path, { developing: stage.phase === 'develop' })); }
            catch { /* advisory only */ }
          }
        }
        if (granted.includes('sources')) {
          result.sources = inspected.sources.filter((item) => item.kind !== 'project').map((item) => ({ ...item, provenance: 'Observed' }));
          try {
            const knowledge = loaded.exists ? { status: 'present', sources: loaded.manifest.sources } : { status: 'absent' };
            const status = sourceStatus(root, knowledge);
            result.sourceChanges = {
              scanned: status.scanned, scannedAt: status.scannedAt, missing: status.missing,
              changed: status.changed, added: status.added, removed: status.removed, more: status.more,
            };
            result.warnings.push(...status.warnings);
            if (status.warnings.some((row) => row.startsWith('No aidlc.knowledge.json was found'))) {
              result.warnings = result.warnings.filter((row) => !(row.startsWith('Project file ') && row.includes('knowledge init')));
            }
          } catch (error) {
            result.warnings.push(`source changes could not be listed: ${error.message}`);
          }
        }
        result.warnings.push(...inspected.warnings);
      } catch (error) {
        result.warnings.push(`sources could not be listed: ${error.message}`);
      }
    }
    if (granted.includes('profile')) {
      try { result.codebaseMap = codebaseMapStatus(root); }
      catch (error) { result.warnings.push(`codebase map status could not be listed: ${error.message}`); }
    }
    if (stage.repeat === 'work-unit' && state?.intentId) {
      // One baseline per intent: the first unit records the existing suite's result here and
      // later units reuse it instead of rediscovering the same failures.
      const path = `.aidlc/intents/${state.intentId}/${stage.phase}/baseline.md`;
      result.baseline = { path, exists: existsSync(resolve(root, path)) };
    }
    if (granted.includes('knowledge')) {
      try {
        const { records, invalid } = readRecords(root);
        result.knowledge = {
          dir: '.aidlc/knowledge',
          records: records.map((record) => ({ id: record.id, title: record.title, path: record.file, origin: record.origin })),
          invalid,
        };
        if (stage.id === 'knowledge-writeback' && state?.intentId) {
          const abs = recordPath(root, state.intentId);
          const template = existsSync(join(engineRoot(), 'methodology', 'templates', 'knowledge-record.md'))
            ? join(engineRoot(), 'methodology', 'templates', 'knowledge-record.md')
            : join(engineRoot(), 'methodology', 'templates', 'knowledge-record.md');
          result.knowledge.record = {
            path: `.aidlc/knowledge/${state.intentId}.md`,
            exists: existsSync(abs),
            template,
            frontmatter: expectedRecordFrontmatter(input.graph, state),
          };
        }
      } catch (error) {
        result.warnings.push(`knowledge could not be listed: ${error.message}`);
        result.knowledge = null;
      }
    }
    return result;
  } catch {
    return null;
  }
}

export const ARTIFACT_VALIDATORS = {
  'work-units-json': (absolutePath, text) => workUnitProblems(text),
  'id-registry': (absolutePath, text, { root, graph, intentId }) => {
    const unitIds = root && graph && intentId ? unitIdsFromFile(artifactPathFor(root, graph, intentId, 'work-units-json')) : null;
    return registryProblems(text, { unitIds });
  },
};

const PROJECT_SECTIONS = Object.freeze(['Environment prerequisites', 'Commands', 'Known baseline failures']);

function sectionBody(text, heading) {
  const lines = text.split(/\r?\n/);
  const start = lines.findIndex((line) => line.trim().toLowerCase() === `## ${heading}`.toLowerCase());
  if (start < 0) return null;
  const rest = lines.slice(start + 1);
  const end = rest.findIndex((line) => /^##?\s/.test(line));
  return (end < 0 ? rest : rest.slice(0, end)).join('\n');
}

// Lines the owner wrote, ignoring the scaffold's own headings and placeholder text.
function ownedLines(text, scaffoldLines) {
  return text.split(/\r?\n/).map((line) => line.trim()).filter((line) => line && !scaffoldLines.has(line));
}

export function projectFileWarnings(root, projectPath, { developing = false } = {}) {
  const scaffoldPath = join(engineRoot(), 'data', 'scaffold', 'PROJECT.md');
  let text;
  let scaffold;
  try { text = readFileSync(resolve(root, projectPath), 'utf8'); } catch { return []; }
  try { scaffold = readFileSync(scaffoldPath, 'utf8'); } catch { return []; }
  const scaffoldLines = new Set(scaffold.split(/\r?\n/).map((line) => line.trim()).filter(Boolean));
  if (ownedLines(text, scaffoldLines).length === 0) {
    return [`${projectPath} is still the unedited scaffold. Fill in the architecture, conventions, commands (build, test, lint, format, baseline), environment prerequisites and known baseline failures before Develop; every agent reads it first.`];
  }
  if (!developing) return [];
  const empty = PROJECT_SECTIONS.filter((heading) => {
    const body = sectionBody(text, heading);
    return body === null || ownedLines(body, scaffoldLines).length === 0;
  });
  return empty.length
    ? [`${projectPath} has no content under ${empty.map((heading) => `'## ${heading}'`).join(', ')}. Develop runs code: record the commands, the environment it needs and the failures already present, or the agents will rediscover them on every work unit.`]
    : [];
}
export function extraRequiredArtifacts({ root, stage }) {
  return requiredProjectArtifacts(root, stage.id);
}
