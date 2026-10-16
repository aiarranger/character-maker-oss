#!/usr/bin/env node
/**
 * Populate public/* Character Studio runtime files from canonical sources
 * listed in parts/assets/studio-runtime-v1/manifest.json.
 *
 * Usage: node tools/character-maker/sync-studio-runtime-assets.mjs [--check]
 * --check  verify only (exit 1 on missing/mismatch); do not write.
 */
import { createHash } from 'node:crypto';
import { copyFileSync, existsSync, mkdirSync, readFileSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const MANIFEST = path.join(ROOT, 'parts/assets/studio-runtime-v1/manifest.json');
const checkOnly = process.argv.includes('--check');

function sha256(file) {
  return createHash('sha256').update(readFileSync(file)).digest('hex');
}

const manifest = JSON.parse(readFileSync(MANIFEST, 'utf8'));
if (manifest.version !== 'studio-runtime-v1' || !Array.isArray(manifest.entries)) {
  throw new Error('studio-runtime-v1 manifest is invalid.');
}

let copied = 0;
let same = 0;
const problems = [];

for (const entry of manifest.entries) {
  const source = path.join(ROOT, entry.source);
  const dest = path.join(ROOT, entry.public);
  if (!existsSync(source)) {
    problems.push(`missing source ${entry.source}`);
    continue;
  }
  const sourceSha = sha256(source);
  if (sourceSha !== entry.sha256) {
    problems.push(`source sha mismatch ${entry.source} (manifest ${entry.sha256.slice(0, 12)}… actual ${sourceSha.slice(0, 12)}…)`);
    continue;
  }
  if (existsSync(dest)) {
    const destSha = sha256(dest);
    if (destSha === entry.sha256 && statSync(dest).size === entry.bytes) {
      same += 1;
      continue;
    }
    if (checkOnly) {
      problems.push(`public mismatch ${entry.public}`);
      continue;
    }
  } else if (checkOnly) {
    problems.push(`missing public ${entry.public}`);
    continue;
  }
  if (!checkOnly) {
    mkdirSync(path.dirname(dest), { recursive: true });
    copyFileSync(source, dest);
    if (sha256(dest) !== entry.sha256) problems.push(`copy failed ${entry.public}`);
    else copied += 1;
  }
}

if (problems.length) {
  console.error(problems.slice(0, 40).join('\n'));
  if (problems.length > 40) console.error(`… and ${problems.length - 40} more`);
  process.exit(1);
}

console.log(JSON.stringify({
  ok: true,
  mode: checkOnly ? 'check' : 'sync',
  entries: manifest.entries.length,
  copied,
  same,
}, null, 2));
