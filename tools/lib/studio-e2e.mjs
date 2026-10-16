// オリジナルキャラの E2E が Character Studio（/character-maker）を操作するための共通部品。
// オリジナルの編集画面（OriginalCharacterMaker）は 2026-10-06 から Studio の左の一覧で選ぶ。
// 旧画面（/character-maker/classic）のオリジナルは先頭の1体しか開けないので、キャラごとの確認は Studio で行う。
import catalog from '../../lib/character-maker/studio-catalog.json' with { type: 'json' };
import { MAKER_URL } from './paths.mjs';

export { catalog };
export const STUDIO_URL = `${MAKER_URL}/character-maker`;
export const ORIGINAL_DRAFT_KEY = 'characters.character-maker.original.v1.draft';

export function catalogEntry(id) {
  const entry = catalog.find(item => item.id === id);
  if (!entry) throw new Error(`studio-catalog.json に ${id} がありません`);
  return entry;
}

/** 画面が読み込む 3D モデル（人間は髪パーツ入りの modular、ほかは appearance、無ければ元モデル）。 */
export function displayedModel(entry) {
  return entry.modularModel ?? entry.appearanceModel ?? { url: entry.modelUrl, sha256: entry.modelSha256 };
}

/** 画面が提供する読み取り専用ツール read_character_edit（useCharacterReadTool）を受け取れるようにする。
 *  保存前の編集中の内容（控えにまだ書かれていない状態も含む）を読むために使う。 */
export async function installReadTool(context) {
  await context.addInitScript(() => {
    const tools = {};
    Object.defineProperty(window, '__characterTools', { value: tools });
    document.modelContext = {
      registerTool(tool, options = {}) {
        tools[tool.name] = tool;
        options.signal?.addEventListener('abort', () => { if (tools[tool.name] === tool) delete tools[tool.name]; });
      },
    };
  });
}

export function current(page) {
  return page.evaluate(() => window.__characterTools.read_character_edit?.execute({}) ?? null);
}

/** 今の編集内容が、指定した項目（document の項目か config の項目）と一致するまで待つ。 */
export async function waitDocument(page, expected, options) {
  await page.waitForFunction(expected => {
    const document = window.__characterTools.read_character_edit?.execute({});
    return document && Object.entries(expected).every(([key, value]) => JSON.stringify(document[key]) === JSON.stringify(value));
  }, expected, options);
}
export async function waitConfig(page, partial, options) {
  await page.waitForFunction(partial => {
    const config = window.__characterTools.read_character_edit?.execute({})?.config;
    return config && Object.entries(partial).every(([key, value]) => JSON.stringify(config[key]) === JSON.stringify(value));
  }, partial, options);
}

/** モデルの組み立てが終わり、GLB を書き出せる状態（オリジナルは「3Dモデル（GLB）」、プリセットは「3DモデルのGLB」）。 */
export async function ready(page) {
  await page.waitForFunction(() => [...document.querySelectorAll('button')].some(button => ['3Dモデル（GLB）', '3DモデルのGLB'].includes(button.textContent) && !button.disabled), null, { timeout: 120000 });
}

export async function openStudio(page) {
  await page.goto(STUDIO_URL, { waitUntil: 'domcontentloaded', timeout: 120000 });
  await ready(page);
}

export async function selectOriginal(page, id) {
  const entry = catalogEntry(id);
  await page.getByRole('navigation', { name: 'キャラクター一覧' }).getByRole('button', { name: `${entry.name}を選ぶ`, exact: true }).click();
  await waitDocument(page, { characterId: id });
  await ready(page);
}

export const inspector = page => page.getByRole('complementary', { name: 'カスタマイズ項目', exact: true });
export const commonControls = page => page.getByRole('region', { name: '共通のカスタマイズ', exact: true });

/** 右の「細かくカスタマイズ」を開いて、見た目の項目（体型・髪…）か、表情・動きのタブへ移る。 */
export async function category(page, name) {
  const open = page.getByRole('button', { name: '細かくカスタマイズ', exact: true });
  if (await open.count()) await open.click();
  const sections = page.getByRole('navigation', { name: '編集する内容', exact: true });
  if (['見た目', '表情', '動き'].includes(name)) { await sections.getByRole('button', { name, exact: true }).click(); return; }
  await sections.getByRole('button', { name: '見た目', exact: true }).click();
  await page.getByRole('navigation', { name: '見た目の項目', exact: true }).getByRole('button', { name, exact: true }).click();
}

export async function pause(page) {
  const button = page.getByRole('button', { name: '動作を一時停止', exact: true });
  if (await button.count()) await button.click();
  await waitConfig(page, { playing: false });
}

async function openMenu(page, summary) {
  const menu = page.locator('details').filter({ has: page.locator('summary', { hasText: summary }) }).first();
  if (!await menu.evaluate(element => element.open)) await menu.locator('summary').click();
  return menu;
}
// メニュー（<details>）は使った後も開いたままなので、次の操作の邪魔にならないよう閉じる。
async function closeMenu(menu) {
  if (await menu.evaluate(element => element.open)) await menu.locator('summary').click();
}

/** 「書き出し」メニューの '3Dモデル（GLB）' か '編集設定（JSON）' でファイルを受け取る。 */
export async function exportFile(page, label, path) {
  const menu = await openMenu(page, '書き出し');
  const pending = page.waitForEvent('download', { timeout: 120000 });
  await menu.getByRole('button', { name: label, exact: true }).click();
  await (await pending).saveAs(path);
  await closeMenu(menu);
  return path;
}

/** 「•••」メニューの「JSONを読み込む」からファイルを選ぶ。 */
export async function importJson(page, path) {
  const menu = await openMenu(page, '•••');
  const chooser = page.waitForEvent('filechooser');
  await menu.getByRole('button', { name: 'JSONを読み込む', exact: true }).click();
  await (await chooser).setFiles(path);
  await closeMenu(menu);
}

export async function save(page, name, asNew = false) {
  await page.getByRole('textbox', { name: 'キャラクターの名前', exact: true }).fill(name);
  if (asNew) {
    const menu = await openMenu(page, '•••');
    await menu.getByRole('button', { name: '別のキャラクターとして保存', exact: true }).click();
    await closeMenu(menu);
  } else await page.getByRole('button', { name: '保存する', exact: true }).click();
  await page.getByRole('status').filter({ hasText: `「${name}」を保存しました。` }).waitFor();
}
