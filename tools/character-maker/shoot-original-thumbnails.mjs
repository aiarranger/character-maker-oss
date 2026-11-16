#!/usr/bin/env node
/** ハル・ナミの一覧の顔写真と、髪型・頭・目・口の選択のサムネイルを、今の Studio の画面から撮り直す。docs/original-remake-spec.md
 * node tools/character-maker/shoot-original-thumbnails.mjs [--ids=generic-human-a,generic-human-b]
 *
 * Studio（MAKER_URL。既定は http://127.0.0.1:4284）が動いていること。Chrome（WebGPU）と python3 の PIL を使う。
 * 出力: parts/assets/studio-runtime-v1/original-parts/<id>/*.jpg と portraits/*。台帳（studio-runtime-v1/manifest.json と original-parts の 2 つの manifest）の sha256 も書き換える。
 * このあと pnpm assets:sync で public/ へ写す。
 */
import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { importPlaywright, MAKER_URL } from '../lib/paths.mjs';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const CHARACTERS = {
  'generic-human-a': { name: 'ハル', portrait: 'portraits/haru-maker-quality-20261005.png',
    hair: [['haru-short-v2', 'ショート'], ['haru-crop-v2', '刈り上げショート'], ['haru-center-v2', 'センターパート'], ['haru-swept-v2', 'オールバック'], ['haru-sidepart-v3', 'サイドパート'], ['haru-buzz-v3', 'ベリーショート']] },
  'generic-human-b': { name: 'ナミ', portrait: 'portraits/nagi-selector-quality-v1.jpg',
    hair: [['nagi-bob-v2', 'ボブ'], ['nagi-pixie-v2', 'ピクシー'], ['nagi-long-v2', 'ロングウェーブ', ['hair-nagi-long-v2-front-3ebd4576e87ca87e.jpg']], ['nagi-pony-v2', 'ハイポニー']] },
};
const PARTS = [['head', '顔の形', [['original', 'いつもの顔'], ['rounded', 'まるみ'], ['slim', 'すっきり']]], ['eyes', '目', [['original', 'いつもの目'], ['large', '大きめ'], ['narrow', '横長']]], ['mouth', '口', [['original', 'いつもの口'], ['small', '小さめ'], ['wide', '横に広め']]]];
const ids = process.argv.find(v => v.startsWith('--ids='))?.slice(6).split(',') ?? Object.keys(CHARACTERS);
const work = path.join(ROOT, '.snaps/thumbs'); fs.mkdirSync(work, { recursive: true });
const { chromium } = await importPlaywright();
const browser = await chromium.launch({ channel: 'chrome', headless: true, args: ['--enable-unsafe-webgpu'] });
const jobs = []; // [生の画面写真, 出力（studio-runtime-v1 から見た道）, 一辺, 切り出しの割合]
for (const id of ids) {
  const character = CHARACTERS[id], page = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 2 });
  page.on('pageerror', e => console.error('pageerror:', e.message));
  await page.goto(`${MAKER_URL}/character-maker`, { waitUntil: 'networkidle' });
  await page.getByRole('button', { name: `${character.name}を選ぶ`, exact: true }).click();
  await page.waitForTimeout(9000);
  await page.getByRole('button', { name: '持たない', exact: true }).click().catch(() => {});
  await page.getByRole('button', { name: /細かくカスタマイズ/ }).click().catch(() => {});
  const tab = async label => { await page.getByRole('button', { name: label, exact: true }).first().click(); await page.waitForTimeout(500); };
  // 「通常」を押すと自動まばたきが止まる
  await tab('表情'); await page.getByRole('button', { name: '通常', exact: true }).first().click(); await page.waitForTimeout(500);
  await page.getByRole('button', { name: '待機', exact: true }).click().catch(() => {});
  await page.getByRole('button', { name: '顔', exact: true }).click(); await page.getByRole('button', { name: '正面', exact: true }).click();
  await page.waitForTimeout(2500);
  const stage = page.locator('[data-stage-canvas]').first();
  const shoot = async (file, out, size, crop) => {
    // 名前や切り替えのボタンは写さない（舞台の絵だけ）
    const style = await page.addStyleTag({ content: 'body *{visibility:hidden!important}[data-stage-canvas],[data-stage-canvas] *{visibility:visible!important}html,body{background:#e8edf2!important}' });
    await page.waitForTimeout(250);
    fs.writeFileSync(path.join(work, file), await stage.screenshot());
    await style.evaluate(node => node.remove());
    for (const target of [].concat(out)) jobs.push([path.join(work, file), target, size, crop]);
  };
  await tab('見た目'); await tab('髪');
  for (const [hairstyle, label, extra = []] of character.hair) {
    await page.getByRole('button', { name: new RegExp(`^${label}`) }).first().click(); await page.waitForTimeout(3500);
    await shoot(`${id}-hair-${hairstyle}.png`, [`original-parts/${id}/hair-${hairstyle}.jpg`, ...extra.map(name => `original-parts/${id}/${name}`)], 512, 0.8);
  }
  await page.getByRole('button', { name: new RegExp(`^${character.hair[0][1]}`) }).first().click(); await page.waitForTimeout(3500);
  await shoot(`${id}-portrait.png`, character.portrait, 512, 0.92);
  for (const [category, label, list] of PARTS) {
    await tab(label);
    for (const [choice, name] of list) {
      await page.getByRole('button', { name: new RegExp(name) }).first().click(); await page.waitForTimeout(1800);
      await shoot(`${id}-${category}-${choice}.png`, `original-parts/${id}/${category}-${choice}.jpg`, 320, 0.56);
    }
    await page.getByRole('button', { name: new RegExp(list[0][1]) }).first().click(); await page.waitForTimeout(800);
  }
  await page.close();
}
await browser.close();

// 切り出して縮める（舞台の中央の正方形。crop は舞台の短い辺に対する割合）
const base = path.join(ROOT, 'parts/assets/studio-runtime-v1');
execFileSync('python3', ['-c', `
import sys, json
from PIL import Image
for src, out, size, crop in json.loads(sys.argv[1]):
    im = Image.open(src).convert('RGB'); w, h = im.size; s = int(min(w, h) * crop); cx, cy = w // 2, int(h * 0.47)
    box = (cx - s // 2, max(0, cy - s // 2), cx + s // 2, max(0, cy - s // 2) + s)
    im = im.crop(box).resize((size, size), Image.LANCZOS)
    im.save(out, quality=90) if out.endswith('.jpg') else im.save(out, optimize=True)
`, JSON.stringify(jobs.map(([src, out, size, crop]) => [src, path.join(base, out), size, crop]))], { stdio: 'inherit' });

// 台帳の sha256 を書き換える
const sha = file => createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const manifestPath = path.join(base, 'manifest.json'), manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
const catalog = JSON.parse(fs.readFileSync(path.join(ROOT, 'lib/character-maker/studio-catalog.json'), 'utf8'));
let updated = 0;
for (const [, out] of jobs) {
  const source = `parts/assets/studio-runtime-v1/${out}`, entry = manifest.entries.find(item => item.source === source);
  if (!entry) { console.error('not in the runtime manifest:', source); continue; }
  entry.sha256 = sha(path.join(ROOT, source)); entry.bytes = fs.statSync(path.join(ROOT, source)).size; updated++;
}
for (const name of ['manifest.json', 'hair-manifest.json']) {
  const file = path.join(base, 'original-parts', name), data = JSON.parse(fs.readFileSync(file, 'utf8'));
  for (const record of data.records) {
    const id = record.characterId ?? record.id, local = path.join(base, 'original-parts', id, path.basename(record.file));
    if (!ids.includes(id) || !jobs.some(([, out]) => path.join(base, out) === local)) continue;
    const model = catalog.find(item => item.id === id).modularModel.sha256;
    record.sha256 = sha(local);
    for (const key of ['modelSha256', 'modularModelSha256']) if (key in record) record[key] = model;
    for (const key of ['sourceImage', 'source']) if (key in record) record[key] = 'Studio screenshot (tools/character-maker/shoot-original-thumbnails.mjs)';
    delete record.sourceSha256;
  }
  fs.writeFileSync(file, `${JSON.stringify(data, null, 2)}\n`);
  // この 2 つの一覧も public へ写す素材なので、台帳の sha256 を合わせる
  const entry = manifest.entries.find(item => item.source === `parts/assets/studio-runtime-v1/original-parts/${name}`);
  if (entry) { entry.sha256 = sha(file); entry.bytes = fs.statSync(file).size; }
}
fs.writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
console.log(JSON.stringify({ shots: jobs.length, runtimeManifestEntries: updated }));
