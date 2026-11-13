#!/usr/bin/env node
/**
 * 下見の Rust の Stage を配る形にする: 最適化つきで作り（rust/web/build.sh）、distribution/maker-stage/ へ写して SHA-256 を出す。
 * このあと distribution/maker-stage/ を commit し、公開リポジトリ（tools/export-oss.mjs）へ push した固定コミットを
 * server/stage-files.mjs の STAGE_FILES_COMMIT に書いてから pnpm pack:vendor する。docs/rust-stage-plan.md
 *
 *   node tools/character-maker/release-stage.mjs
 */
import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { stageFileManifest } from '../../server/stage-files.mjs';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
execFileSync('bash', ['rust/web/build.sh'], { cwd: ROOT, stdio: 'inherit' });
const out = path.join(ROOT, 'distribution/maker-stage');
mkdirSync(out, { recursive: true });
for (const name of ['character_maker_stage.js', 'character_maker_stage_bg.wasm']) copyFileSync(path.join(ROOT, 'public/maker-stage', name), path.join(out, name));
console.log(JSON.stringify(stageFileManifest('<固定コミット>').files.map(({ public: p, sha256, bytes }) => ({ public: p, sha256, bytes })), null, 2));
