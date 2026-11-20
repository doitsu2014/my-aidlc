// buildDirectiveContext is synchronous, called once per orchestrate next that selects a stage, before any write.
// input: { root, graph, config, state, stage, intentDir, isNewIntent, expectedArtifacts, artifactPathFor }
import { resolve } from 'node:path';
import { inspectSources, loadKnowledgeManifest, readRecords } from './knowledge.mjs';
import { sourceStatus } from './onboard.mjs';

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
    if (granted.includes('knowledge')) {
      try {
        const { records, invalid } = readRecords(root);
        result.knowledge = {
          dir: '.aidlc/knowledge',
          records: records.map((record) => ({ id: record.id, title: record.title, path: record.file, origin: record.origin })),
          invalid,
        };
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

export const ARTIFACT_VALIDATORS = {};
export const extraRequiredArtifacts = () => [];
