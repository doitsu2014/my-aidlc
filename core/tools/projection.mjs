import { createHash } from 'node:crypto';
import { existsSync, lstatSync, readFileSync, readdirSync, unlinkSync, rmdirSync, mkdirSync, copyFileSync, statSync } from 'node:fs';
import { basename, delimiter, dirname, join, relative } from 'node:path';
import { audit, assertNoSymlinkComponents, configuredHarnesses, engineRoot, loadHarnessManifest, repoRoot, writeFileAtomic } from './lib.mjs';

export function hashText(text) { return createHash('sha256').update(String(text).replaceAll('\r\n', '\n')).digest('hex'); }
export function stampPath(root, manifest) { return join(root, manifest.skillDir, '.projection.json'); }
export function readStamp(root, manifest) {
  const path = stampPath(root, manifest);
  try {
    if (lstatSync(path).isSymbolicLink()) return null;
    const stamp = JSON.parse(readFileSync(path, 'utf8'));
    return stamp?.schemaVersion === 1 ? stamp : null;
  } catch { return null; }
}
export function writeStamp(root, manifest, stamp) {
  const path = stampPath(root, manifest), text = `${JSON.stringify(stamp, null, 2)}\n`;
  try { if (readFileSync(path, 'utf8') === text) return; } catch {}
  writeFileAtomic(path, text, { mode: 0o644 });
}
export function compareVersions(a, b) {
  const parse = (value) => String(value ?? '').match(/^(\d+)\.(\d+)\.(\d+)/)?.slice(1).map(Number) ?? [0, 0, 0];
  const left = parse(a), right = parse(b);
  for (let i = 0; i < 3; i++) if (left[i] !== right[i]) return left[i] - right[i];
  return 0;
}
function sourceHash(manifest, file) { return hashText(readFileSync(join(repoRoot(), 'harness', manifest.name, file.source))); }
export function projectionStatus(root, manifest, engineVersion) {
  const stamp = readStamp(root, manifest), history = projectionHistory().harness?.[manifest.name] ?? {};
  const files = manifest.files.map((file) => {
    const path = join(root, manifest.skillDir, file.target);
    assertNoSymlinkComponents(root, join(manifest.skillDir, dirname(file.target)));
    if (!existsSync(path)) return { path: file.target, state: 'missing' };
    let hash;
    try { if (!lstatSync(path).isFile()) return { path: file.target, state: 'modified' }; hash = hashText(readFileSync(path)); }
    catch { return { path: file.target, state: 'missing' }; }
    if (hash === sourceHash(manifest, file)) return { path: file.target, state: 'current' };
    return { path: file.target, state: hash === stamp?.files?.[file.target]?.sha256 || history[basename(file.target)]?.includes(hash) ? 'behind' : 'modified' };
  });
  const obsolete = [];
  const candidates = [...(manifest.obsoleteFiles ?? []).map((file) => ({ dir: manifest.skillDir, file })), ...(manifest.legacy ?? []).flatMap((entry) => entry.files.map((file) => ({ dir: entry.dir, file })))];
  for (const { dir, file } of candidates) {
    const path = join(root, dir, file);
    assertNoSymlinkComponents(root, dir);
    if (!existsSync(path)) continue;
    let state = 'modified';
    try {
      if (lstatSync(path).isFile()) {
        const hash = hashText(readFileSync(path));
        const prior = stamp?.files?.[file]?.sha256 ?? history[basename(file)]?.find((known) => known === hash);
        if (prior === hash) state = 'pristine';
      }
    } catch {}
    obsolete.push({ path: relative(root, path).split(/\\/).join('/'), state });
  }
  const stampedEngineVersion = stamp?.engineVersion ?? null;
  const ahead = compareVersions(stampedEngineVersion, engineVersion) > 0 && files.some((file) => file.state !== 'current');
  const state = ahead ? 'ahead' : files.some((file) => file.state === 'missing') ? 'missing' : [...files, ...obsolete].some((file) => file.state === 'modified') ? 'modified' : files.some((file) => file.state === 'behind') || obsolete.some((file) => file.state === 'pristine') ? 'behind' : 'current';
  return { harness: manifest.name, invoke: manifest.invoke, skillDir: manifest.skillDir, state, stampedEngineVersion, files, obsolete };
}
export function updateProjectionStamp(root, manifest, engineVersion) {
  const existing = readStamp(root, manifest) ?? { schemaVersion: 1, harness: manifest.name, engineVersion: null, files: {} };
  const files = { ...existing.files };
  let current = true;
  for (const file of manifest.files) {
    const path = join(root, manifest.skillDir, file.target), hash = sourceHash(manifest, file);
    try {
      if (lstatSync(path).isFile() && hashText(readFileSync(path)) === hash) files[file.target] = { sha256: hash };
      else current = false;
    } catch { current = false; }
  }
  const stamp = { schemaVersion: 1, harness: manifest.name, engineVersion: current && compareVersions(existing.engineVersion, engineVersion) <= 0 ? engineVersion : existing.engineVersion ?? null, files };
  writeStamp(root, manifest, stamp);
  return stamp;
}
export function refreshProjection(root, manifest, engineVersion, { dryRun = false, force = false } = {}) {
  assertNoSymlinkComponents(root, manifest.skillDir);
  for (const legacy of manifest.legacy ?? []) assertNoSymlinkComponents(root, legacy.dir);
  const before = projectionStatus(root, manifest, engineVersion), prior = readStamp(root, manifest), actions = [];
  if (before.state === 'ahead' && !force) actions.push({ path: manifest.skillDir, action: 'refused-ahead' });
  else {
    const history = projectionHistory().harness?.[manifest.name] ?? {};
    for (const file of manifest.files) {
      const full = join(root, manifest.skillDir, file.target), state = before.files.find((row) => row.path === file.target).state;
      if (state === 'current') continue;
      if (state === 'missing') { actions.push({ path: relative(root, full).split(/\\/).join('/'), action: 'created' }); if (!dryRun) { mkdirSync(dirname(full), { recursive: true }); writeFileAtomic(full, readFileSync(join(repoRoot(), 'harness', manifest.name, file.source)), { mode: 0o644 }); } continue; }
      if (state === 'modified') {
        let regular = false; try { regular = lstatSync(full).isFile(); } catch {}
        if (!regular) { actions.push({ path: relative(root, full).split(/\\/).join('/'), action: 'refused-not-a-regular-file' }); continue; }
        const backup = `${full}.local-backup`; let backupPath = backup, n = 2; while (existsSync(backupPath)) backupPath = `${backup}.${n++}`;
        if (!force) actions.push({ path: relative(root, full).split(/\\/).join('/'), action: 'kept-modified' });
        else { actions.push({ path: relative(root, full).split(/\\/).join('/'), action: 'replaced-with-backup', backup: relative(root, backupPath).split(/\\/).join('/') }); if (!dryRun) { writeFileAtomic(backupPath, readFileSync(full), { mode: 0o644 }); writeFileAtomic(full, readFileSync(join(repoRoot(), 'harness', manifest.name, file.source)), { mode: 0o644 }); } }
        continue;
      }
      if (state === 'behind') { actions.push({ path: relative(root, full).split(/\\/).join('/'), action: 'updated' }); if (!dryRun) { mkdirSync(dirname(full), { recursive: true }); writeFileAtomic(full, readFileSync(join(repoRoot(), 'harness', manifest.name, file.source)), { mode: 0o644 }); } }
    }
    const obsolete = before.obsolete;
    for (const file of obsolete) {
      if (file.state === 'modified') { actions.push({ path: file.path, action: 'kept-modified-obsolete' }); continue; }
      actions.push({ path: file.path, action: 'removed' });
      if (!dryRun) {
        const full = join(root, file.path); unlinkSync(full);
        let dir = dirname(full);
        while (dir !== join(root, manifest.skillDir) && dir.startsWith(root)) { try { rmdirSync(dir); actions.push({ path: relative(root, dir).split(/\\/).join('/'), action: 'removed-directory' }); } catch { break; } if (manifest.legacy?.some((entry) => join(root, entry.dir) === dir)) break; dir = dirname(dir); }
      }
    }
    if (!dryRun) {
      const retained = Object.fromEntries(Object.entries(prior?.files ?? {}).filter(([path]) => manifest.files.some((file) => file.target === path)));
      const next = { schemaVersion: 1, harness: manifest.name, engineVersion, files: retained };
      for (const file of manifest.files) {
        const path = join(root, manifest.skillDir, file.target);
        try { if (lstatSync(path).isFile() && hashText(readFileSync(path)) === sourceHash(manifest, file)) next.files[file.target] = { sha256: sourceHash(manifest, file) }; } catch {}
      }
      writeStamp(root, manifest, next);
      if (actions.some((row) => ['created', 'updated', 'replaced-with-backup', 'removed'].includes(row.action))) {
        const selected = actions.filter((row) => ['created', 'updated', 'replaced-with-backup', 'removed'].includes(row.action));
        audit(root, 'projection.refreshed', { harness: manifest.name, from: prior?.engineVersion ?? null, to: engineVersion, created: selected.filter((row) => row.action === 'created').map((row) => row.path), updated: selected.filter((row) => ['updated', 'replaced-with-backup'].includes(row.action)).map((row) => row.path), removed: selected.filter((row) => row.action === 'removed').map((row) => row.path), keptModified: actions.filter((row) => ['kept-modified', 'kept-modified-obsolete'].includes(row.action)).map((row) => row.path) });
      }
    }
  }
  const after = dryRun ? before : projectionStatus(root, manifest, engineVersion);
  const state = actions.some((row) => row.action === 'refused-ahead') ? 'ahead' : actions.some((row) => row.action === 'refused-not-a-regular-file') ? 'modified' : after.state;
  return { harness: manifest.name, from: prior?.engineVersion ?? null, to: engineVersion, stateBefore: before.state, state, actions };
}
export function legacyMethodologyStatus(root) {
  const base = join(root, '.aidlc', 'methodology');
  if (!existsSync(base)) return null;
  const history = projectionHistory().methodology ?? {}, pristine = [], modified = [];
  const walk = (dir, prefix = '') => {
    for (const name of readdirSync(dir)) {
      const path = join(dir, name), rel = prefix ? `${prefix}/${name}` : name;
      let stat;
      try { stat = lstatSync(path); } catch { modified.push(rel); continue; }
      if (stat.isSymbolicLink()) { modified.push(rel); continue; }
      if (stat.isDirectory()) walk(path, rel);
      else if (stat.isFile() && history[rel]?.includes(hashText(readFileSync(path)))) pristine.push(rel);
      else modified.push(rel);
    }
  };
  try { walk(base); } catch { modified.push('.'); }
  return { pristine, modified };
}
export function projectionDoctorRows(root, config, engineVersion) {
  const harnesses = configuredHarnesses(config);
  if (!harnesses.length) return [];
  const rows = [];
  for (const name of harnesses) {
    const manifest = loadHarnessManifest(name);
    let status;
    try { status = projectionStatus(root, manifest, engineVersion); }
    catch (error) {
      rows.push({ status: 'fail', message: `Projection ${name}: ${error.message}` });
      continue;
    }
    for (const file of status.files.filter((item) => item.state === 'missing')) {
      rows.push({ status: 'fail', message: `Projection ${name}: ${join(manifest.skillDir, file.path).split(/\\\\/g).join('/')} is missing; run: dtech-aidlc harness refresh` });
    }
    const paths = [...status.files.filter((item) => item.state === 'modified').map((item) => join(manifest.skillDir, item.path).split('\\\\').join('/')), ...status.obsolete.filter((item) => item.state === 'modified').map((item) => item.path)];
    if (status.state === 'behind') rows.push({ status: 'warn', message: `Projection ${name}: behind engine ${engineVersion} (projected by ${status.stampedEngineVersion ?? 'an earlier version without a stamp'}); run: dtech-aidlc harness refresh` });
    else if (status.state === 'modified') rows.push({ status: 'warn', message: `Projection ${name}: ${paths.join(', ')} edited locally or of unknown origin; keep them, or for skill files run: dtech-aidlc harness refresh --force; obsolete files must be deleted by hand` });
    else if (status.state === 'ahead') rows.push({ status: 'warn', message: `Projection ${name}: projected by engine ${status.stampedEngineVersion}, newer than the installed ${engineVersion}; upgrade dtech-aidlc` });
    else if (status.state === 'current') rows.push({ status: 'pass', message: `Projection ${name}: current (${manifest.skillDir}, engine ${engineVersion})` });
  }
  if (!commandOnPath('dtech-aidlc')) rows.push({ status: 'warn', message: 'dtech-aidlc is not on PATH; the projected skill runs the bare command dtech-aidlc' });
  return rows;
}
export function commandOnPath(command) {
  const extensions = process.platform === 'win32' ? (process.env.PATHEXT || '.COM;.EXE;.BAT;.CMD').split(';') : [''];
  return (process.env.PATH || '').split(delimiter).some((directory) => extensions.some((extension) => {
    try { return statSync(join(directory, `${command}${extension}`)).isFile(); } catch { return false; }
  }));
}
export function projectionHistory() {
  try { return JSON.parse(readFileSync(join(engineRoot(), 'data', 'projection-history.json'), 'utf8')); } catch { return { harness: {}, methodology: {} }; }
}
export function projectionNotices(root, engineVersion) {
  try {
    const config = JSON.parse(readFileSync(join(root, '.aidlc', 'config.json'), 'utf8'));
    if (!config || typeof config !== 'object' || Array.isArray(config)) return [];
    return configuredHarnesses(config).flatMap((name) => {
      const manifest = loadHarnessManifest(name), status = projectionStatus(root, manifest, engineVersion);
      if (status.state === 'current') return [];
      if (status.state === 'ahead') return [`The projected ${manifest.invoke} skill for ${name} was written by engine ${status.stampedEngineVersion}, newer than the installed ${engineVersion}. Ask the user to upgrade dtech-aidlc.`];
      return [`The projected ${manifest.invoke} skill for ${name} is ${status.state} relative to engine ${engineVersion}. Ask the user to run: dtech-aidlc harness refresh`];
    });
  } catch { return []; }
}
