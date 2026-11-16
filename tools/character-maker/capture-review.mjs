#!/usr/bin/env node
/**
 * 見た目のレビュー用の写真を撮る。開発サーバーを起動した状態で実行する。
 *   node tools/character-maker/capture-review.mjs <outdir> [--names=ナミ,ハル]
 * キャラごとに、全身（待機）と顔の 2 枚（ステージの部分）と、最初の 1 体だけ画面全体を撮り、一覧（sheet-body.jpg・sheet-face.jpg）にまとめる。
 */
import { mkdir } from 'node:fs/promises';
import path from 'node:path';
import { readFile } from 'node:fs/promises';
import { importPlaywright, MAKER_URL } from '../lib/paths.mjs';

const args = process.argv.slice(2);
const out = path.resolve(args.find(arg => !arg.startsWith('--')) ?? '.snaps/review');
const only = args.find(arg => arg.startsWith('--names='))?.slice(8).split(',');
await mkdir(out, { recursive: true });
const { chromium } = await importPlaywright();
const browser = await chromium.launch({ channel: 'chrome', headless: true, args: ['--enable-unsafe-webgpu', '--enable-features=Vulkan'] });
const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 });
page.on('console', message => { if (message.type() === 'error') console.error('console:', message.text()); });
await page.goto(`${MAKER_URL}/character-maker`, { waitUntil: 'networkidle' });
const settle = async () => {
  await page.waitForFunction(() => {
    const root = document.querySelector('[data-stage-canvas]');
    return root?.querySelector('canvas')?.width > 0 && root.getAttribute('data-camera-settled') !== 'false';
  }, null, { timeout: 90000 }).catch(() => {});
  await page.waitForTimeout(2500);
};
const names = await page.$$eval('button[aria-label$="を選ぶ"]', buttons => buttons.map(b => b.getAttribute('aria-label').replace('を選ぶ', '')).filter(n => !/シルエット|輪郭|目の形/.test(n)));
const cast = names.slice(0, 16).filter(name => !only || only.includes(name));
const stage = () => page.locator('[data-stage-canvas]').first();
const body = [], face = [];
for (const [index, name] of cast.entries()) {
  await page.getByRole('button', { name: `${name}を選ぶ`, exact: true }).click();
  await settle();
  await page.getByRole('button', { name: '持たない', exact: true }).click().catch(() => {});
  await page.getByRole('button', { name: '待機', exact: true }).click().catch(() => {});
  await page.getByRole('button', { name: '全身', exact: true }).click().catch(() => {});
  await settle();
  if (index === 0) await page.screenshot({ path: path.join(out, 'screen.png') });
  const b = path.join(out, `body-${String(index + 1).padStart(2, '0')}.png`);
  await stage().screenshot({ path: b }); body.push(b);
  await page.getByRole('button', { name: '顔', exact: true }).click();
  await settle();
  const f = path.join(out, `face-${String(index + 1).padStart(2, '0')}.png`);
  await stage().screenshot({ path: f }); face.push(f);
  console.log(name);
}
/** 一覧は、写真を並べたページを撮って作る（sharp を使わない） */
async function sheet(files, file, cell = 420) {
  const columns = Math.min(4, files.length), rows = Math.ceil(files.length / columns);
  const images = await Promise.all(files.map(async f => `data:image/png;base64,${(await readFile(f)).toString('base64')}`));
  const sheetPage = await browser.newPage({ viewport: { width: columns * cell, height: rows * cell } });
  await sheetPage.setContent(`<body style="margin:0;display:grid;grid-template-columns:repeat(${columns},${cell}px)">${images.map(src => `<img src="${src}" style="width:${cell}px;height:${cell}px;object-fit:cover">`).join('')}</body>`);
  await sheetPage.screenshot({ path: path.join(out, file), type: 'jpeg', quality: 86 });
  await sheetPage.close();
}
await sheet(body, 'sheet-body.jpg'); await sheet(face, 'sheet-face.jpg');
await browser.close();
