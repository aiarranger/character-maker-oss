#!/usr/bin/env node
/**
 * Character Studio の受入計測（docs/studio-quality-spec.md §10.1）。開発サーバーを起動した状態で実行する。
 *
 *   node tools/character-maker/measure-studio.mjs <outdir> [--viewports=1440x900,390x844] [--character=ナミ]
 *
 * 幅ごとに: canvas と枠の大きさ、キャラ（不透明画素）の高さの割合と端からの距離、横スクロール、
 * canvas が最初の画面に入るか、コンソールエラー。武器「持たない」と「構える」の両方を測る（1440x900 と 1280x720 のみ）。
 * 結果は <outdir>/measure.json とスクリーンショット。合否の判定は §10.1 の数値と照らして人が読む。
 */
import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import sharp from 'sharp';
import { importPlaywright, MAKER_URL } from '../lib/paths.mjs';

const args = process.argv.slice(2);
const out = path.resolve(args.find(arg => !arg.startsWith('--')) ?? '/tmp/measure-studio');
const option = name => args.find(arg => arg.startsWith(`--${name}=`))?.slice(name.length + 3);
const viewports = (option('viewports') ?? '1440x900,1280x720,1024x768,820x1180,768x1024,390x844,360x740,844x390,320x640')
  .split(',').map(item => item.split('x').map(Number));
const characterName = option('character') ?? 'ナミ';
await mkdir(out, { recursive: true });

const { chromium } = await importPlaywright();
const browser = await chromium.launch({ channel: 'chrome', headless: true });

/** canvas の画素から、不透明（alpha>250）な部分の上下左右を求める */
async function occupancy(page, label) {
  const shot = await page.evaluate(() => {
    const canvas = document.querySelector('[data-stage-canvas] canvas') ?? document.querySelector('section[aria-label="キャラクターの3Dプレビュー"] canvas');
    if (!canvas) return null;
    const rect = canvas.getBoundingClientRect();
    return { data: canvas.toDataURL('image/png'), rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height } };
  });
  if (!shot) return null;
  const png = Buffer.from(shot.data.split(',')[1], 'base64');
  await writeFile(path.join(out, `${label}-canvas.png`), png);
  const { data, info } = await sharp(png).ensureAlpha().raw().toBuffer({ resolveWithObject: true });
  let top = info.height, bottom = -1, left = info.width, right = -1;
  for (let y = 0; y < info.height; y += 1) for (let x = 0; x < info.width; x += 1) {
    if (data[(y * info.width + x) * 4 + 3] > 250) { if (y < top) top = y; if (y > bottom) bottom = y; if (x < left) left = x; if (x > right) right = x; }
  }
  if (bottom < 0) return { rect: shot.rect, empty: true };
  const scale = info.height / shot.rect.height;
  return {
    rect: shot.rect,
    characterPx: Math.round((bottom - top + 1) / scale),
    occupancy: Number(((bottom - top + 1) / info.height).toFixed(3)),
    margins: { top: Math.round(top / scale), bottom: Math.round((info.height - 1 - bottom) / scale), left: Math.round(left / scale), right: Math.round((info.width - 1 - right) / scale) },
  };
}

async function settle(page) {
  await page.waitForFunction(() => {
    const canvas = document.querySelector('[data-stage-canvas] canvas') ?? document.querySelector('section[aria-label="キャラクターの3Dプレビュー"] canvas');
    const root = document.querySelector('[data-stage-canvas]');
    return canvas && canvas.width > 0 && !document.querySelector('[role=alert]') && (!root || root.getAttribute('data-camera-settled') !== 'false');
  }, null, { timeout: 90000 }).catch(() => {});
  await page.waitForTimeout(1800);
}

const results = [];
for (const [width, height] of viewports) {
  const mobile = width < 600;
  const context = await browser.newContext({ viewport: { width, height }, deviceScaleFactor: mobile ? 2 : 1, isMobile: mobile, hasTouch: mobile });
  const page = await context.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
  page.on('response', response => { if (response.status() >= 400) errors.push(`${response.status()} ${response.url()}`); });
  await page.goto(`${MAKER_URL}/character-maker`, { waitUntil: 'domcontentloaded' });
  const pick = page.getByRole('button', { name: `${characterName}を選ぶ` });
  if (await pick.count()) await pick.first().click().catch(() => {});
  await settle(page);
  const label = `${width}x${height}`;
  const layout = await page.evaluate(() => {
    const canvas = document.querySelector('[data-stage-canvas] canvas') ?? document.querySelector('section[aria-label="キャラクターの3Dプレビュー"] canvas');
    const slot = canvas?.closest('[data-stage-canvas]')?.parentElement ?? canvas?.parentElement;
    const r = element => { if (!element) return null; const b = element.getBoundingClientRect(); return { x: Math.round(b.x), y: Math.round(b.y), width: Math.round(b.width), height: Math.round(b.height) }; };
    const scrollers = [...document.querySelectorAll('body *')].filter(element => { const s = getComputedStyle(element); return /(auto|scroll)/.test(s.overflowY) && element.scrollHeight > element.clientHeight + 2; }).length;
    return { canvas: r(canvas), slot: r(slot), scrollWidth: document.documentElement.scrollWidth, innerWidth, innerHeight, documentScrolls: document.documentElement.scrollHeight > innerHeight + 2, innerScrollers: scrollers };
  });
  const entry = { viewport: label, errors, ...layout, canvasMatchesSlot: layout.canvas && layout.slot ? Math.abs(layout.canvas.height - layout.slot.height) <= 2 : null,
    horizontalOverflow: layout.scrollWidth > layout.innerWidth, canvasInFirstScreen: layout.canvas ? layout.canvas.y >= 0 && layout.canvas.y + layout.canvas.height <= layout.innerHeight + 1 : false };
  await page.screenshot({ path: path.join(out, `${label}.png`) });
  if (width >= 1280) {
    const tray = page.getByRole('region', { name: '持たせる武器' }).or(page.locator('section[aria-label="持たせる武器"]'));
    const none = tray.getByRole('button', { name: /持たない/ }).first();
    if (await none.count()) { await none.click().catch(() => {}); await settle(page); entry.unarmed = await occupancy(page, `${label}-unarmed`); }
    const cake = tray.getByRole('button', { name: /ケーキの剣/ }).first();
    if (await cake.count()) { await cake.click().catch(() => {}); await settle(page); entry.armed = await occupancy(page, `${label}-armed`); }
  } else entry.current = await occupancy(page, label);
  results.push(entry);
  console.log(JSON.stringify({ viewport: label, canvas: entry.canvas, unarmed: entry.unarmed?.occupancy, armed: entry.armed?.occupancy, current: entry.current?.characterPx, overflow: entry.horizontalOverflow, firstScreen: entry.canvasInFirstScreen, errors: errors.length }));
  await context.close();
}
await browser.close();
await writeFile(path.join(out, 'measure.json'), `${JSON.stringify(results, null, 2)}\n`);
