// 「魚群繚乱で遊ぶ」を、Studio（/character-maker）の全16体（オリジナル4・ファンタジー8・アメコミ4）で確かめる。docs/gyogun-spec.md
// node --import ./tests/ts-resolve.mjs lib/character-maker/ui-gyogun-handoff.e2e.mjs [output-directory] [key,key,…]
// - 魚群繚乱が配られていない（/gyogun/index.html が 404）ときは、飛ばずに知らせる。控えには何も置かない
// - ステージの左下のカード（動画つき。docs/gyogun-spec.md の「入口のカード」）を押すと、ブラウザの控え（Cache Storage の決まったキー）に
//   魚群繚乱用の .character.zip が入り、/gyogun/index.html?from=studio へ移る。入口はカードの 1 つだけ（バーのボタンは出ない）
// - ZIP の GLB に 待機・歩く・走る・構える・振る・のけぞり・＞＜・ジャンプ・回避・必殺技 が入り、武器の形は入らない。武器は character.json の weapon
// 魚群繚乱のページは空のページに差し替える（ここでは遊ばない。public/gyogun が無くても動く）。
// 出力先の ZIP は、魚群繚乱の主役の部品の試験が読む（games/gyogun/rust/game/tests/fixtures/studio/ へ写す。games/gyogun/README.md）。
// 2つ目の引数で体を絞れる（例: generic-cat,fantasy-knight,comic-rowan）。
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { unzipSync } from 'three/examples/jsm/libs/fflate.module.js';
import { ACTIVE_FANTASY_PRESET_IDS, getPresets, makeDocument, serializeDocument } from './config.ts';
import { STUDIO_GAMES } from './game-handoff.ts';
const { cacheName: GYOGUN_CACHE_NAME, characterKey: GYOGUN_CHARACTER_KEY, notBuilt: GYOGUN_NOT_BUILT, playUrl: GYOGUN_PLAY_URL } = STUDIO_GAMES.gyogun;
import { selectedMakerWeaponId } from './weapon-catalog.ts';
import { importPlaywright, MAKER_URL } from '../../tools/lib/paths.mjs';
import { catalog, current, importJson, installReadTool, openStudio, ready, selectOriginal } from '../../tools/lib/studio-e2e.mjs';

const output = process.argv[2] ?? '/tmp/maker-gyogun-handoff';
const only = process.argv[3] ? process.argv[3].split(',') : null;
await mkdir(output, { recursive: true });
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
const GAME_CLIPS = ['idle', 'walk', 'run', 'sword_idle', 'sword_attack', 'hit', 'face_hurt', 'jump', 'dodge', 'skill'];

function inspectPackage(bytes, expectedName, expectedWeapon) {
  const files = unzipSync(new Uint8Array(bytes));
  assert.deepEqual(Object.keys(files).sort(), ['character.json', 'model.glb', 'thumbnail.png']);
  const manifest = JSON.parse(new TextDecoder().decode(files['character.json']));
  assert.equal(manifest.name, expectedName);
  assert.equal(manifest.weapon, expectedWeapon);
  const model = Buffer.from(files['model.glb']);
  assert.deepEqual(manifest.model, { path: 'model.glb', sha256: sha256(model), bytes: model.length });
  assert.deepEqual([...files['thumbnail.png'].subarray(0, 4)], [137, 80, 78, 71]);
  const gltf = JSON.parse(model.subarray(20, 20 + model.readUInt32LE(12)).toString('utf8'));
  const names = gltf.animations.map(clip => clip.name);
  // 待機・歩く・走るは、読み込み側（contract.mjs inspectModel）と同じ探し方（':idle' で終わる名前 → 'idle'）。ほかは名前そのもの
  const pick = family => names.find(name => name.endsWith(`:${family}`)) ?? names.find(name => name === family);
  assert.deepEqual(manifest.motion.clips, { idle: pick('idle'), walk: pick('walk'), run: pick('run') });
  for (const clip of GAME_CLIPS.slice(3)) assert.equal(names.filter(name => name === clip).length, 1, `${clip} clip is in the package once`);
  assert.ok(gltf.nodes.some(node => node.name === 'maker-hurt-face'), '＞＜ node is in the package');
  const meshUnder = index => gltf.nodes[index].mesh !== undefined || (gltf.nodes[index].children ?? []).some(meshUnder);
  assert.deepEqual(gltf.nodes.flatMap((node, index) => /Sword|weapon/i.test(node.name ?? '') && meshUnder(index) ? [node.name] : []), [], 'no weapon mesh in the GLB');
  return { manifest, clips: names, modelBytes: model.length };
}

const { chromium } = await importPlaywright();
const browser = await chromium.launch({ channel: 'chrome', headless: true });
const context = await browser.newContext({ viewport: { width: 1440, height: 960 } });
await installReadTool(context);
const page = await context.newPage();
page.setDefaultTimeout(30000);
const errors = [], results = [];
page.on('pageerror', error => errors.push(error.message));
// 魚群繚乱のページ: built が false の間は 404、true なら空のページ
let built = false;
await page.route('**/gyogun/index.html*', route => built
  ? route.fulfill({ status: 200, contentType: 'text/html', body: '<!doctype html><title>gyogun stub</title>' })
  : route.fulfill({ status: 404, contentType: 'text/html', body: 'not found' }));

// 入口はステージの左下のカードだけ（バーのボタンと「書き出し」メニューの項目は無い）
const button = () => page.getByRole('region', { name: 'キャラクターの3Dプレビュー', exact: true }).getByRole('button', { name: '魚群繚乱で遊ぶ', exact: true });
const waitName = name => page.waitForFunction(name => document.querySelector('input[aria-label="キャラクターの名前"]')?.value === name, name, { timeout: 120000 });
const cached = () => page.evaluate(async ([name, key]) => {
  const hit = await (await caches.open(name)).match(key);
  if (!hit) return null;
  const blob = await hit.blob();
  return new Promise(resolve => { const reader = new FileReader(); reader.onload = () => resolve(String(reader.result).split(',')[1]); reader.readAsDataURL(blob); });
}, [GYOGUN_CACHE_NAME, GYOGUN_CHARACTER_KEY]);

async function send(key, expectedName, expectedWeapon) {
  await button().click();
  await page.waitForURL(`${MAKER_URL}${GYOGUN_PLAY_URL}`, { timeout: 180000 });
  const base64 = await cached();
  assert.ok(base64, 'the character zip is in Cache Storage');
  const bytes = Buffer.from(base64, 'base64');
  const file = join(output, `${key}.character.zip`);
  await writeFile(file, bytes);
  const inspected = inspectPackage(bytes, expectedName, expectedWeapon);
  results.push({ key, file, zipBytes: bytes.length, manifest: inspected.manifest, clips: inspected.clips, modelBytes: inspected.modelBytes });
  console.log(key, inspected.manifest.name, inspected.manifest.weapon, inspected.manifest.coordinates.heightMeters, `zip ${(bytes.length / 1048576).toFixed(2)}MiB`, inspected.clips.length, 'clips');
  await openStudio(page);
}
const wanted = key => !only || only.includes(key);

try {
  await openStudio(page);
  await page.evaluate(name => caches.delete(name), GYOGUN_CACHE_NAME);
  // 入口は 1 つだけ: カードに動画（動きを減らす設定なら止め絵）があり、同じ名前のボタンはほかに見えない
  assert.equal(await page.getByRole('button', { name: '魚群繚乱で遊ぶ', exact: true }).count(), 1);
  assert.equal(await button().locator('video, img').count(), 1);
  // 魚群繚乱がまだ作られていない: 飛ばずに知らせる
  await button().click();
  await page.getByRole('alert').filter({ hasText: GYOGUN_NOT_BUILT }).or(page.getByRole('status').filter({ hasText: GYOGUN_NOT_BUILT })).first().waitFor({ timeout: 60000 });
  assert.equal(new URL(page.url()).pathname, '/character-maker');
  assert.equal(await cached(), null, 'nothing is stored when the game is not built');
  built = true;

  for (const id of ['generic-human-a', 'generic-human-b', 'generic-robot', 'generic-cat']) {
    if (!wanted(id)) continue;
    await selectOriginal(page, id);
    const name = (await page.getByRole('textbox', { name: 'キャラクターの名前', exact: true }).inputValue()) || catalog.find(item => item.id === id).name;
    await send(id, name, selectedMakerWeaponId((await current(page)).config));
  }
  const cast = page.getByRole('navigation', { name: 'キャラクター一覧' });
  const activeFantasy = getPresets('fantasy').filter(preset => ACTIVE_FANTASY_PRESET_IDS.includes(preset.id));
  for (const [style, preset] of [...activeFantasy.map(preset => ['fantasy', preset]), ...getPresets('comic').map(preset => ['comic', preset])]) {
    if (!wanted(`${style}-${preset.id}`)) continue;
    await cast.getByRole('button', { name: `${preset.name}を選ぶ`, exact: true }).click();
    await waitName(preset.name);
    await ready(page);
    await send(`${style}-${preset.id}`, preset.name, selectedMakerWeaponId(preset.config));
  }
  // 一覧にないファンタジー4体は、プリセットの JSON を読み込んで送る（読み込む先はプリセットの編集画面）
  for (const preset of getPresets('fantasy').filter(preset => !ACTIVE_FANTASY_PRESET_IDS.includes(preset.id))) {
    if (!wanted(`fantasy-${preset.id}`)) continue;
    await cast.getByRole('button', { name: `${activeFantasy[0].name}を選ぶ`, exact: true }).click();
    await ready(page);
    const json = join(output, `fantasy-${preset.id}.json`);
    await writeFile(json, JSON.stringify(serializeDocument(makeDocument(preset.name, preset.config)), null, 2));
    await importJson(page, json);
    await waitName(preset.name);
    await ready(page);
    await send(`fantasy-${preset.id}`, preset.name, selectedMakerWeaponId(preset.config));
  }
  if (!only) assert.equal(results.length, 16);
  assert.ok(results.length > 0);
  assert.deepEqual(errors, []);
  console.log('PASS ui-gyogun-handoff', results.length);
} finally {
  await writeFile(join(output, 'gyogun-handoff-results.json'), JSON.stringify({ errors, results }, null, 2) + '\n');
  await browser.close();
}
