// yukikau.jp 用（.character.zip）の書き出しを、Studio の全16体（オリジナル4・ファンタジー8・アメコミ4）で確かめる。docs/yukikau-route.md
// node --import ./tests/ts-resolve.mjs lib/character-maker/ui-yukikau-package.e2e.mjs [output-directory]
// - 書き出しメニューの「yukikau.jp 用（.character.zip）」でファイルを受け取り、通知の「yukikau.jp で読み込む」リンクを確かめる
// - ZIP の中身を yukikau.jp の読み込み（@aiarranger/character-runtime の contract.mjs）と同じ規則で調べる
// - Studio の一覧にないファンタジー4体は、プリセットの JSON を読み込んで書き出す
// 実際の読み込みコードでの確認は、出力先の ZIP をその読み込みに通す（作業記録の verify スクリプト）。
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { unzipSync } from 'three/examples/jsm/libs/fflate.module.js';
import { ACTIVE_FANTASY_PRESET_IDS, getPresets, makeDocument, serializeDocument } from './config.ts';
import { importPlaywright } from '../../tools/lib/paths.mjs';
import { catalog, importJson, installReadTool, openStudio, ready, selectOriginal } from '../../tools/lib/studio-e2e.mjs';

const output = process.argv[2] ?? '/tmp/maker-yukikau-package';
await mkdir(output, { recursive: true });
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
const ALLOWED_EXTENSIONS = ['EXT_meshopt_compression', 'KHR_materials_unlit', 'KHR_texture_transform'];
const MANIFEST_KEYS = ['schemaVersion', 'id', 'name', 'author', 'modelVersion', 'model', 'thumbnail', 'coordinates', 'motion', 'terms'];

/** contract.mjs の validateManifest / validatePackageFiles と同じ規則で調べる */
function inspectPackage(bytes, expectedName) {
  const files = unzipSync(new Uint8Array(bytes));
  assert.deepEqual(Object.keys(files).sort(), ['character.json', 'model.glb', 'thumbnail.png']);
  const manifest = JSON.parse(new TextDecoder().decode(files['character.json']));
  assert.deepEqual(Object.keys(manifest), MANIFEST_KEYS);
  assert.equal(manifest.schemaVersion, 1);
  assert.match(manifest.id, /^maker-[a-f0-9]{12}$/);
  assert.equal(manifest.name, expectedName);
  assert.equal(manifest.author, 'AI Arranger Character Studio');
  const model = Buffer.from(files['model.glb']);
  assert.deepEqual(manifest.model, { path: 'model.glb', sha256: sha256(model), bytes: model.length });
  assert.equal(manifest.id, `maker-${manifest.model.sha256.slice(0, 12)}`);
  assert.deepEqual(manifest.thumbnail, { path: 'thumbnail.png', sha256: sha256(files['thumbnail.png']) });
  assert.deepEqual([...files['thumbnail.png'].subarray(0, 4)], [137, 80, 78, 71]);
  assert.equal(manifest.coordinates.up, '+Y'); assert.equal(manifest.coordinates.forward, '+Z'); assert.equal(manifest.coordinates.ground, 'bounds-min-y');
  assert.ok(manifest.coordinates.heightMeters >= 0.2 && manifest.coordinates.heightMeters <= 3);
  assert.equal(manifest.motion.controller, 'morph-v1');
  assert.deepEqual(manifest.motion.expressions, []);
  assert.equal(manifest.terms.reference, 'https://github.com/aiarranger/character-maker-oss');
  const gltf = JSON.parse(model.subarray(20, 20 + model.readUInt32LE(12)).toString('utf8'));
  assert.equal(model.readUInt32LE(8), model.length);
  assert.deepEqual((gltf.extensionsRequired ?? []).filter(name => !ALLOWED_EXTENSIONS.includes(name)), [], 'no extension yukikau.jp rejects (KHR_mesh_quantization)');
  const names = gltf.animations.map(clip => clip.name);
  const pick = family => names.find(name => name.endsWith(`:${family}`)) ?? names.find(name => name === family);
  assert.deepEqual(manifest.motion.clips, { idle: pick('idle'), walk: pick('walk'), run: pick('run') });
  for (const family of ['hit', 'face_hurt']) assert.ok(names.includes(family), `${family} clip is in the package`);
  assert.ok(gltf.nodes.some(node => node.name === 'maker-hurt-face'), '＞＜ node is in the package');
  return { manifest, clips: names, modelBytes: model.length, thumbnailBytes: files['thumbnail.png'].length, model, thumbnail: Buffer.from(files['thumbnail.png']) };
}

const { chromium } = await importPlaywright();
const browser = await chromium.launch({ channel: 'chrome', headless: true });
const context = await browser.newContext({ viewport: { width: 1440, height: 960 }, acceptDownloads: true });
await installReadTool(context);
const page = await context.newPage();
page.setDefaultTimeout(30000);
const errors = [], results = [];
page.on('pageerror', error => errors.push(error.message));

// プリセットの編集画面には読み取りツールがないので、名前の欄で切り替わりを待つ
const waitName = name => page.waitForFunction(name => document.querySelector('input[aria-label="キャラクターの名前"]')?.value === name, name, { timeout: 120000 });
async function openExportMenu() {
  const menu = page.locator('details').filter({ has: page.locator('summary', { hasText: '書き出し' }) }).first();
  if (!await menu.evaluate(element => element.open)) await menu.locator('summary').click();
  return menu;
}
async function exportPackage(key, expectedName) {
  const menu = await openExportMenu();
  const pending = page.waitForEvent('download', { timeout: 180000 });
  await menu.getByRole('button', { name: 'yukikau.jp 用（.character.zip）', exact: true }).click();
  const download = await pending;
  assert.match(download.suggestedFilename(), /\.character\.zip$/);
  const file = join(output, `${key}.character.zip`);
  await download.saveAs(file);
  const link = page.getByRole('status').getByRole('link', { name: 'yukikau.jp で読み込む', exact: true });
  await link.waitFor({ timeout: 180000 });
  assert.equal(await link.getAttribute('href'), 'https://yukikau.jp/play?import=1');
  assert.equal(await link.getAttribute('target'), '_blank');
  assert.equal(await link.getAttribute('rel'), 'noopener');
  if (await menu.evaluate(element => element.open)) await menu.locator('summary').click();
  const inspected = inspectPackage(await readFile(file), expectedName);
  await writeFile(join(output, `${key}.thumbnail.png`), inspected.thumbnail);
  results.push({ key, file, suggested: download.suggestedFilename(), manifest: inspected.manifest, clips: inspected.clips, modelBytes: inspected.modelBytes, thumbnailBytes: inspected.thumbnailBytes });
  console.log(key, inspected.manifest.coordinates.heightMeters, JSON.stringify(inspected.manifest.motion.clips), inspected.modelBytes);
  await page.getByRole('button', { name: '通知を閉じる' }).first().click();
}

try {
  await openStudio(page);
  // オリジナル4体（剣を構えた状態でも、yukikau.jp 用は通常用の3動作で書き出す）
  for (const id of ['generic-human-a', 'generic-human-b', 'generic-robot', 'generic-cat']) {
    await selectOriginal(page, id);
    const name = (await page.getByRole('textbox', { name: 'キャラクターの名前', exact: true }).inputValue()) || catalog.find(item => item.id === id).name;
    await exportPackage(id, name);
  }
  // Studio の一覧のプリセット（ファンタジー4・アメコミ4）
  const cast = page.getByRole('navigation', { name: 'キャラクター一覧' });
  const activeFantasy = getPresets('fantasy').filter(preset => ACTIVE_FANTASY_PRESET_IDS.includes(preset.id));
  for (const [style, preset] of [...activeFantasy.map(preset => ['fantasy', preset]), ...getPresets('comic').map(preset => ['comic', preset])]) {
    await cast.getByRole('button', { name: `${preset.name}を選ぶ`, exact: true }).click();
    await waitName(preset.name);
    await ready(page);
    await exportPackage(`${style}-${preset.id}`, preset.name);
  }
  // 一覧にないファンタジー4体は、プリセットの JSON を読み込んで書き出す
  for (const preset of getPresets('fantasy').filter(preset => !ACTIVE_FANTASY_PRESET_IDS.includes(preset.id))) {
    const json = join(output, `fantasy-${preset.id}.json`);
    await writeFile(json, JSON.stringify(serializeDocument(makeDocument(preset.name, preset.config)), null, 2));
    await importJson(page, json);
    await waitName(preset.name);
    await ready(page);
    await exportPackage(`fantasy-${preset.id}`, preset.name);
  }
  assert.equal(results.length, 16);
  assert.deepEqual(errors, []);
  console.log('PASS ui-yukikau-package', results.length);
} finally {
  await writeFile(join(output, 'yukikau-package-results.json'), JSON.stringify({ errors, results }, null, 2) + '\n');
  await browser.close();
}
