// Independent, unauthenticated local-browser check. Does not connect to an existing browser or profile.
// node lib/character-maker/ui.e2e.mjs [output-directory]
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { importPlaywright, MAKER_URL } from '../../tools/lib/paths.mjs';

const output = process.argv[2] ?? await mkdtemp(join(tmpdir(), 'character-maker-ui-'));
await mkdir(output, { recursive: true });
const { chromium } = await importPlaywright();
const browser = await chromium.launch({ channel: 'chrome', headless: true, args: ['--enable-unsafe-swiftshader', '--use-angle=swiftshader'] });
const context = await browser.newContext({ viewport: { width: 1257, height: 712 } });
const page = await context.newPage();
page.setDefaultTimeout(15000);
const errors = [], nativeDialogs = [], steps = [];
page.on('pageerror', error => errors.push(error.message));
page.on('dialog', async dialog => { nativeDialogs.push(dialog.message()); await dialog.dismiss(); });
const check = label => { steps.push(label); console.log(label); };
async function value(locator, expected) { assert.equal(await locator.inputValue(), expected); }
async function waitReady() { await page.getByRole('button', { name: 'GLBを書き出す', exact: true }).waitFor(); await page.waitForFunction(() => [...document.querySelectorAll('button')].some(button => button.textContent === 'GLBを書き出す' && !button.disabled), null, { timeout: 45000 }); }
async function noSwitchDialog() { assert.equal(await page.getByRole('alertdialog').count(), 0); }
try {
  // characters のキャラ一覧（「＋ キャラクターメイク」リンク）は分離でこのリポジトリに無いので、旧画面を直接開く。
  await page.goto(`${MAKER_URL}/character-maker/classic`, { waitUntil: 'domcontentloaded' });
  await waitReady();
  check('classic character-maker opens');
  // This regression flow covers the preserved comic editor; new installs now
  // start in fantasy, so choose the original style through its visible control.
  await page.getByRole('button', { name: /^アメコミ/ }).click();
  await waitReady();

  const name = page.getByRole('textbox', { name: 'キャラクターの名前', exact: true });
  await name.fill('E2E アオイ');
  await page.getByRole('button', { name: '保存', exact: true }).click();
  await page.getByRole('status').filter({ hasText: '「E2E アオイ」を保存しました。' }).waitFor();
  await name.fill('E2E リナ');
  await page.getByRole('button', { name: '別のキャラとして保存', exact: true }).click();
  await page.getByRole('status').filter({ hasText: '「E2E リナ」を別のキャラとして保存しました。' }).waitFor();
  await page.getByRole('button', { name: 'E2E アオイ', exact: true }).waitFor();
  await page.getByRole('button', { name: 'E2E リナ', exact: true }).waitFor();
  check('two distinct saved characters created through UI');
  const librarySnapshot = await page.evaluate(() => Object.keys(localStorage).filter(key => key.startsWith('characters.character-maker.v1.record.')).sort().map(key => [key, localStorage.getItem(key)]));

  await name.fill('E2E 編集を保持');
  await page.getByRole('button', { name: 'E2E アオイ', exact: true }).click();
  await value(name, 'E2E アオイ');
  await noSwitchDialog();
  await page.reload({ waitUntil: 'domcontentloaded' });
  await waitReady();
  await value(name, 'E2E アオイ');
  assert.equal(await page.getByRole('button', { name: 'E2E アオイ', exact: true }).count(), 1);
  assert.equal(await page.getByRole('button', { name: 'E2E リナ', exact: true }).count(), 1);
  check('dirty → saved switches directly without alert; immediate reload restores target and both saved records');

  const imported = { format: 'characters.character-maker', schemaVersion: 1, assetVersion: 'human-maker-v1', name: 'E2E JSON', config: { base: 'masculine', height: 1.04, build: 0.15, hair: 'crop', hairColor: '#292B32', face: 'angular', eyes: 'almond', mouth: 'natural' } };
  await name.fill('E2E JSON読込前');
  await page.getByLabel('キャラクター設定JSON', { exact: true }).setInputFiles({ name: 'test.character.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(imported)) });
  await noSwitchDialog();
  await value(name, 'E2E JSON');
  await page.reload({ waitUntil: 'domcontentloaded' });
  await waitReady();
  await value(name, 'E2E JSON');
  assert.deepEqual(await page.evaluate(() => Object.keys(localStorage).filter(key => key.startsWith('characters.character-maker.v1.record.')).sort().map(key => [key, localStorage.getItem(key)])), librarySnapshot);
  check('dirty → JSON switches directly; reload retains imported document and leaves saved records unchanged');

  // Return directly to a saved state, then capture both viewport sizes.
  await page.getByRole('button', { name: 'E2E アオイ', exact: true }).click();
  await noSwitchDialog();
  await waitReady();
  await page.getByRole('button', { name: '全身', exact: true }).click();
  await page.getByRole('button', { name: '正面', exact: true }).click();
  await page.evaluate(() => window.scrollTo(0, 0));
  await page.screenshot({ path: join(output, 'desktop-1257x712.png') });
  await page.screenshot({ path: join(output, 'desktop-full.png'), fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({ path: join(output, 'mobile-390x844.png') });
  await page.screenshot({ path: join(output, 'mobile-full.png'), fullPage: true });
  const mobile = await page.evaluate(() => ({ width: innerWidth, clientWidth: document.documentElement.clientWidth, scrollWidth: document.documentElement.scrollWidth }));
  assert.equal(mobile.width, 390); assert.equal(mobile.scrollWidth, 390);
  await page.getByRole('button', { name: /^髪型/ }).click();
  await page.getByRole('button', { name: 'おだんご', exact: true }).click();
  await waitReady();
  await page.screenshot({ path: join(output, 'mobile-hair.png') });
  check('390×844 shows 3D and controls, no horizontal overflow, hair selection works');

  assert.equal(nativeDialogs.length, 0);
  assert.equal(errors.length, 0);
  const result = { success: true, output, steps, errors, nativeDialogs, mobile };
  await writeFile(join(output, 'result.json'), JSON.stringify(result, null, 2));
  console.log(JSON.stringify(result));
} catch (error) {
  await page.screenshot({ path: join(output, 'failure.png'), fullPage: true }).catch(() => {});
  await writeFile(join(output, 'failure.json'), JSON.stringify({ message: error.message, steps, errors, nativeDialogs, output }, null, 2));
  console.error(JSON.stringify({ message: error.message, steps, errors, nativeDialogs, output }));
  process.exitCode = 1;
} finally {
  await context.close();
  await browser.close();
}
