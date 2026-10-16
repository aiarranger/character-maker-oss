// Actual UI round-trip for both styles, legacy JSON, sword replay and downloads.
import assert from 'node:assert/strict';
import { mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { importPlaywright, MAKER_URL } from '../../tools/lib/paths.mjs';
const output = process.argv[2] ?? await mkdtemp(join(tmpdir(), 'maker-fantasy-ui-'));
await mkdir(output, { recursive: true });
const { chromium } = await importPlaywright();
const browser = await chromium.launch({ channel: 'chrome', headless: true });
const context = await browser.newContext({ viewport: { width: 1440, height: 960 } });
const page = await context.newPage();
page.setDefaultTimeout(30000);
const errors = [], steps = [];
page.on('pageerror', error => errors.push(error.message));
async function ready() { await page.waitForFunction(() => [...document.querySelectorAll('button')].some(b => b.textContent === 'GLBを書き出す' && !b.disabled), null, { timeout: 120000 }); }
async function download(label, filename) {
  const promise = page.waitForEvent('download', { timeout: 120000 });
  await page.getByRole('button', { name: label, exact: true }).click();
  const item = await promise;
  const destination = join(output, filename);
  await item.saveAs(destination); return destination;
}
async function exportPair(stem) {
  const json = await download('JSONを書き出す', `${stem}.character.json`);
  await download('GLBを書き出す', `${stem}.glb`);
  return JSON.parse(await readFile(json, 'utf8'));
}
async function capture(name, fullPage = false) { await page.screenshot({ path: join(output, `${name}.jpg`), type: 'jpeg', quality: 94, fullPage }); }
async function category(name) { await page.getByRole('navigation', { name: '変更するパーツ' }).getByRole('button', { name: new RegExp(`^${name}`) }).click(); }
try {
  // characters のキャラ一覧（「＋ キャラクターメイク」リンク）は分離でこのリポジトリに無いので、旧画面を直接開く。
  await page.goto(`${MAKER_URL}/character-maker/classic`, { waitUntil: 'domcontentloaded', timeout: 120000 });
  await ready();
  assert.equal(await page.locator('main[data-style]').getAttribute('data-style'), 'fantasy');
  await page.getByRole('textbox', { name: 'キャラクターの名前', exact: true }).fill('ルミナ');
  await page.getByRole('button', { name: '丸顔', exact: true }).click();
  await category('目'); await page.getByRole('button', { name: 'まるい目', exact: true }).click();
  await category('口'); await page.getByRole('button', { name: '口角を上げる', exact: true }).click();
  await category('髪色'); await page.getByRole('button', { name: '銀', exact: true }).click();
  await category('体型'); await page.getByRole('slider', { name: '身長', exact: true }).press('End');
  await page.getByRole('slider', { name: '身長', exact: true }).press('ArrowLeft');
  await page.getByRole('button', { name: '保存', exact: true }).click();
  await page.reload({ waitUntil: 'domcontentloaded' }); await ready();
  assert.equal(await page.getByRole('textbox', { name: 'キャラクターの名前', exact: true }).inputValue(), 'ルミナ');
  await category('髪型'); await page.getByRole('button', { name: 'サイドポニー', exact: true }).click(); await ready();
  await page.getByRole('button', { name: '顔', exact: true }).click();
  await page.evaluate(() => window.scrollTo(0, 0));
  await page.waitForTimeout(500); await capture('desktop-fantasy');
  const fantasy = await exportPair('fantasy-feminine');
  // 武器（既定 cake_sword）の追加で汎用キャラの形式は schema 5 になった。
  assert.equal(fantasy.schemaVersion, 5); assert.equal(fantasy.config.style, 'fantasy');
  assert.equal(fantasy.config.face, 'round'); assert.equal(fantasy.config.eyes, 'round');
  assert.equal(fantasy.config.mouth, 'smile'); assert.equal(fantasy.config.hairColor, '#C4C7CD');
  assert.equal(fantasy.config.height, 1.11);
  steps.push('fantasy: all six controls → save → reload → JSON and animated GLB');

  await page.getByRole('button', { name: '剣を振る', exact: true }).click();
  await page.waitForTimeout(600); await capture('desktop-sword');
  await page.waitForFunction(() => [...document.querySelectorAll('button')].some(b => b.textContent === '剣を構える' && b.getAttribute('aria-pressed') === 'true'));
  await page.getByRole('button', { name: '剣を振る', exact: true }).click();
  await page.waitForTimeout(100);
  await page.getByRole('button', { name: '剣を振る', exact: true }).click();
  await page.waitForFunction(() => [...document.querySelectorAll('button')].some(b => b.textContent === '剣を構える' && b.getAttribute('aria-pressed') === 'true'));
  steps.push('sword: one-shot returns to stance; repeated and rapid presses replay');
  await page.getByRole('button', { name: '待機', exact: true }).click();
  await category('体型'); await page.getByRole('button', { name: 'タイプB', exact: true }).click(); await ready();
  await page.getByRole('button', { name: '顔', exact: true }).click(); await capture('fantasy-masculine');
  await exportPair('fantasy-masculine');

  const legacy = { format: 'characters.character-maker', schemaVersion: 1, assetVersion: 'human-maker-v1', name: 'カイ', config: { base: 'masculine', height: 1.04, build: 0.15, hair: 'crop', hairColor: '#292B32', face: 'angular', eyes: 'almond', mouth: 'natural' } };
  await page.getByLabel('キャラクター設定JSON', { exact: true }).setInputFiles({ name: 'legacy.character.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(legacy)) });
  if (await page.getByRole('alertdialog').count()) await page.getByRole('button', { name: '保存せず切り替える', exact: true }).click();
  await ready();
  assert.equal(await page.locator('main[data-style]').getAttribute('data-style'), 'comic');
  const comic = await exportPair('comic-masculine');
  assert.deepEqual(comic, legacy);
  await page.getByRole('button', { name: /^ファンタジー/ }).click(); await ready();
  await page.getByRole('button', { name: /^アメコミ/ }).click(); await ready();
  const returnComic = JSON.parse(await readFile(await download('JSONを書き出す', 'comic-return.character.json'), 'utf8'));
  assert.deepEqual(returnComic, legacy);
  await capture('desktop-comic');
  steps.push('legacy JSON imports unchanged; style switch returns exact comic settings and schema1');
  await category('体型'); await page.getByRole('button', { name: 'タイプA', exact: true }).click(); await ready();
  await exportPair('comic-feminine');
  await page.getByRole('button', { name: /^ファンタジー/ }).click(); await ready();
  await category('体型'); await page.getByRole('button', { name: 'タイプA', exact: true }).click(); await ready();
  await category('髪型');
  await page.getByRole('button', { name: '顔', exact: true }).click();
  await page.setViewportSize({ width: 390, height: 844 });
  await page.evaluate(() => window.scrollTo(0, 0)); await page.waitForTimeout(300);
  await capture('mobile-fantasy'); await capture('mobile-full', true);
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth), 390);
  await page.getByRole('button', { name: 'エアリーショート', exact: true }).click(); await ready();
  await page.getByRole('button', { name: '剣を振る', exact: true }).click();
  await page.waitForFunction(() => [...document.querySelectorAll('button')].some(b => b.textContent === '剣を構える' && b.getAttribute('aria-pressed') === 'true'));
  steps.push('mobile 390px: no horizontal overflow, hair edit and sword replay work');
  assert.deepEqual(errors, []);
  await writeFile(join(output, 'result.json'), JSON.stringify({ success: true, output, steps, errors }, null, 2));
  console.log(JSON.stringify({ success: true, output, steps }));
} catch (error) {
  await capture('failure', true).catch(() => {});
  await writeFile(join(output, 'failure.json'), JSON.stringify({ error: String(error), steps, errors }, null, 2));
  console.error(error); process.exitCode = 1;
} finally { await context.close(); await browser.close(); }
