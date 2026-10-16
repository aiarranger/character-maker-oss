// Focused draft recovery check in a fresh, unauthenticated local browser.
// node lib/character-maker/ui-draft.e2e.mjs [output-directory]
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { importPlaywright, MAKER_URL } from '../../tools/lib/paths.mjs';

const output = process.argv[2] ?? await mkdtemp(join(tmpdir(), 'character-maker-tab-drafts-'));
await mkdir(output, { recursive: true });
const { chromium } = await importPlaywright();
// Let Chrome use its native renderer; two animated canvases can starve the
// software renderer's animation-frame and screenshot work on this machine.
const browser = await chromium.launch({ channel: 'chrome', headless: true });
const context = await browser.newContext({ viewport: { width: 1257, height: 712 } });
const errors = [], steps = [];
const draftKey = 'characters.character-maker.v1.draft';
const prefix = 'characters.character-maker.v1.record.';
let preparation = 'launch';
const name = page => page.getByRole('textbox', { name: 'キャラクターの名前', exact: true });
async function hydrated(page) {
  preparation = 'wait for hydrated save control';
  await page.waitForFunction(() => [...document.querySelectorAll('button')].some(button => button.textContent === '保存' && !button.disabled));
  preparation = 'wait for assembled model';
  await page.waitForFunction(() => [...document.querySelectorAll('button')].some(button => button.textContent === 'GLBを書き出す' && !button.disabled), null, { timeout: 45000 });
}
async function activate(page) {
  preparation = 'foreground tab';
  await page.bringToFront();
  await page.waitForFunction(() => document.visibilityState === 'visible');
  // Let the newly foregrounded canvas and layout paint before real input.
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
}
async function bodyControls(page) {
  await activate(page);
  await hydrated(page);
  const category = page.getByRole('navigation', { name: '変更するパーツ' }).getByRole('button', { name: /^体型/ });
  preparation = 'open body controls';
  await category.focus();
  await page.keyboard.press('Enter');
  await page.getByRole('slider', { name: '身長', exact: true }).waitFor({ state: 'visible' });
  preparation = 'ready for assertions';
}
async function open() {
  const page = await context.newPage();
  page.setDefaultTimeout(15000);
  page.on('pageerror', error => errors.push(error.message));
  await page.goto(`${MAKER_URL}/character-maker/classic`, { waitUntil: 'domcontentloaded' });
  await bodyControls(page);
  return page;
}
async function draftWritten(page, expected) {
  await page.waitForFunction(({ key, expected }) => {
    const session = JSON.parse(sessionStorage.getItem(key) ?? 'null');
    const backup = JSON.parse(localStorage.getItem(key) ?? 'null');
    return session?.document.name === expected && backup?.document.name === expected;
  }, { key: draftKey, expected });
}
async function reload(page) {
  await activate(page);
  await page.reload({ waitUntil: 'domcontentloaded' });
  await bodyControls(page);
}
async function records(page) {
  return page.evaluate(prefix => Object.keys(localStorage).filter(key => key.startsWith(prefix)).map(key => JSON.parse(localStorage.getItem(key))), prefix);
}
const check = text => { steps.push(text); console.log(text); };
let a, b, c;
try {
  a = await open();
  await name(a).fill('共通の保存作品');
  await a.getByRole('button', { name: '保存', exact: true }).click();
  await a.getByRole('status').filter({ hasText: '「共通の保存作品」を保存しました。' }).waitFor();
  const original = (await records(a))[0];
  b = await open();
  assert.equal(await name(b).inputValue(), '共通の保存作品');

  await activate(a);
  await name(a).fill('タブAの未保存編集');
  await a.getByRole('slider', { name: '身長', exact: true }).press('End');
  await draftWritten(a, 'タブAの未保存編集');
  await activate(b);
  await name(b).fill('タブBの未保存編集');
  await b.getByRole('slider', { name: '身長', exact: true }).press('Home');
  await draftWritten(b, 'タブBの未保存編集');
  await reload(a);
  assert.equal(await name(a).inputValue(), 'タブAの未保存編集');
  assert.equal(await a.getByRole('slider', { name: '身長', exact: true }).inputValue(), '1.12');
  // 読み込んだだけでは控えを書かない（draft-writer.test.mjs「initial hydration ... do not write」）。
  // 共通の控え（localStorage）は最後に編集したタブBのまま、タブAは自分の sessionStorage から戻る。
  await a.waitForFunction(({ key, a, b }) => JSON.parse(sessionStorage.getItem(key) ?? 'null')?.document.name === a
    && JSON.parse(localStorage.getItem(key) ?? 'null')?.document.name === b, { key: draftKey, a: 'タブAの未保存編集', b: 'タブBの未保存編集' });
  await reload(b);
  assert.equal(await name(b).inputValue(), 'タブBの未保存編集');
  assert.equal(await b.getByRole('slider', { name: '身長', exact: true }).inputValue(), '0.85');
  check('two tabs restore their own unsaved name and height after reload');

  await name(b).fill('タブBで保存した最新版');
  await b.getByRole('button', { name: '保存', exact: true }).click();
  await b.getByRole('status').filter({ hasText: '「タブBで保存した最新版」を保存しました。' }).waitFor();
  await reload(a);
  assert.equal(await name(a).inputValue(), 'タブAの未保存編集');
  await a.getByRole('alert').filter({ hasText: '別の画面で更新されています' }).waitFor();
  await a.getByRole('button', { name: '保存', exact: true }).click();
  await a.getByRole('status').filter({ hasText: '「タブAの未保存編集」を保存しました。' }).waitFor();
  const afterConflict = await records(a);
  assert.equal(afterConflict.length, 2);
  assert.equal(afterConflict.find(item => item.id === original.id).name, 'タブBで保存した最新版');
  assert.equal(afterConflict.find(item => item.id !== original.id).name, 'タブAの未保存編集');
  check('stale revision is retained as a new record and does not overwrite the latest saved character');

  await activate(b);
  await name(b).fill('閉じたタブの最後の編集');
  await draftWritten(b, '閉じたタブの最後の編集');
  await b.close();
  c = await open();
  assert.equal(await name(c).inputValue(), '閉じたタブの最後の編集');
  check('a fresh tab recovers the last persistent draft after its original tab closes');

  // Old drafts lack revision metadata; import the fixture as a browser from the previous version.
  await c.evaluate(({ draftKey, original }) => {
    sessionStorage.removeItem(draftKey);
    localStorage.setItem(draftKey, JSON.stringify({ document: { ...original, name: '旧形式の編集中データ' }, savedId: original.id }));
  }, { draftKey, original });
  await reload(c);
  assert.equal(await name(c).inputValue(), '旧形式の編集中データ');
  await c.getByRole('alert').filter({ hasText: '元の保存内容を確認できないため' }).waitFor();
  await c.getByRole('button', { name: '別のキャラとして保存', exact: true }).click();
  await c.getByRole('status').filter({ hasText: '「旧形式の編集中データ」を保存しました。' }).waitFor();
  const afterLegacy = await records(c);
  assert.equal(afterLegacy.length, 3);
  assert.equal(afterLegacy.find(item => item.id === original.id).name, 'タブBで保存した最新版');
  check('legacy draft recovery keeps its content and preserves the latest saved record');

  assert.deepEqual(errors, []);
  const result = { success: true, output, steps, errors, afterConflict, afterLegacy };
  await writeFile(join(output, 'result.json'), JSON.stringify(result, null, 2));
  console.log(JSON.stringify(result));
} catch (error) {
  const screenshots = [];
  for (const [index, page] of context.pages().entries()) {
    if (!page || page.isClosed()) continue;
    const path = join(output, `failure-tab-${index + 1}.png`);
    await page.bringToFront().catch(() => {});
    await page.screenshot({ path, fullPage: true }).then(() => screenshots.push(path)).catch(() => {});
  }
  const result = { success: false, output, preparation, message: error.message, stack: error.stack, steps, errors, screenshots };
  await writeFile(join(output, 'result.json'), JSON.stringify(result, null, 2));
  console.error(JSON.stringify(result));
  process.exitCode = 1;
} finally {
  await context.close();
  await browser.close();
}
