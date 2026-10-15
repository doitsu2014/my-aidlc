#!/usr/bin/env node
// scripts/fetch-native.mjs — refresh core/native/ from the pinned AWS AI-DLC
// release, or verify that the committed snapshot still matches the pin.
//
// Usage:
//   node scripts/fetch-native.mjs                 fetch, verify, and rewrite core/native/
//   node scripts/fetch-native.mjs --check         verify the snapshot against the pin (no network)
//   node scripts/fetch-native.mjs --from <path>   use a local archive instead of downloading
//   node scripts/fetch-native.mjs --dry-run       report what would be written, write nothing
//
// `my-aidlc update` is the same operation through the CLI; both call
// `updateSnapshot` in core/tools/lib/native.mjs, so they cannot drift.

import { readPin, updateSnapshot, verifySnapshot } from "../core/tools/lib/native.mjs";
import { repoRoot } from "../core/tools/lib/version.mjs";

async function main() {
  const args = process.argv.slice(2);
  const check = args.includes("--check");
  const dryRun = args.includes("--dry-run");
  const fromIndex = args.indexOf("--from");
  const from = fromIndex === -1 ? null : args[fromIndex + 1];
  const root = repoRoot();
  const pin = readPin(root);
  const log = (message) => process.stdout.write(`${message}\n`);

  if (check) {
    const result = verifySnapshot(root, pin);
    if (!result.ok) {
      for (const entry of result.drift) {
        process.stderr.write(`FAIL ${entry.kind} ${entry.path}\n`);
      }
      process.stderr.write(
        result.reason === "missing"
          ? `FAIL no snapshot at ${pin.snapshotDir}/. Run: node scripts/fetch-native.mjs\n`
          : result.reason === "pin"
            ? `FAIL snapshot is ${result.lock.version} from ${result.lock.repository} but the pin is ${pin.version} from ${pin.repository}.\n`
            : `FAIL ${result.drift.length} file(s) drifted from the snapshot lock. Run: node scripts/fetch-native.mjs\n`,
      );
      process.exit(1);
    }
    const lock = result.lock;
    process.stdout.write(
      `PASS native snapshot: AI-DLC ${lock.version}, ${lock.stages} stages, ${lock.agents} agents, ` +
        `${lock.scopes} scopes, ${lock.protocols} protocols ` +
        `(${lock.archiveVerified ? "archive sha256 verified" : "content verified"})\n`,
    );
    return;
  }

  await updateSnapshot(root, pin, { log, from, dryRun });
}

await main();
