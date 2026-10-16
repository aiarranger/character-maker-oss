// Eight fantasy presets, responsive selection and height persistence/export.
// node lib/character-maker/ui-presets.e2e.mjs [output-directory]
import assert from 'node:assert/strict';
import { mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { getPresets, HEIGHT_RANGE, makeDocument } from './config.ts';
import { importPlaywright, MAKER_URL } from '../../tools/lib/paths.mjs';

const output = process.argv[2] ?? await mkdtemp(join(tmpdir(), 'maker-presets-'));
await mkdir(output, { recursive: true });
const { chromium } = await importPlaywright();
const browser = await chromium.launch({ channel: 'chrome', headless: true });
const context = await browser.newContext({ viewport: { width: 1440, height: 960 } });
const page = await context.newPage();
page.setDefaultTimeout(30000);
const errors = [], steps = [], exports = [];
page.on('pageerror', error => errors.push(error.message));
const presetSection = page.getByRole('region', { name: 'スタイルから選ぶ', exact: true });
const ready = () => page.waitForFunction(() => [...document.querySelectorAll('button')].some(b => b.textContent === 'GLBを書き出す' && !b.disabled), null, { timeout: 120000 });
const body = () => page.getByRole('navigation', { name: '変更するパーツ' }).getByRole('button', { name: /^体型/ }).click();
async function capture(name, fullPage = false) {
  await page.screenshot({ path: join(output, `${name}.jpg`), type: 'jpeg', quality: 93, fullPage });
}
async function download(label, filename) {
  const pending = page.waitForEvent('download', { timeout: 120000 });
  await page.getByRole('button', { name: label, exact: true }).click();
  const item = await pending;
  const file = join(output, filename);
  await item.saveAs(file); return file;
}
async function selectPreset(preset) {
  await presetSection.getByRole('button', { name: preset.name, exact: true }).click();
  await ready();
  // 選んだ時点で現行の形へ直る（剣の既定 weaponId、引退した男性のサイドポニーは sideSweep へ）ので、その結果と比べる。
  await page.waitForFunction(expected => {
    const draft = JSON.parse(sessionStorage.getItem('characters.character-maker.v1.draft') ?? 'null');
    if (!draft?.document) return false;
    const actual = { expression: 'neutral', expressionStrength: 1, style: 'comic', ...draft.document.config };
    return Object.entries(expected).every(([key, value]) => actual[key] === value);
  }, makeDocument(preset.name, preset.config).config);
  assert.equal(await presetSection.getByRole('button', { name: preset.name, exact: true }).getAttribute('aria-pressed'), 'true');
}
try {
  await page.goto(`${MAKER_URL}/character-maker/classic`, { waitUntil: 'domcontentloaded' });
  await ready();
  const presets = getPresets('fantasy');
  assert.equal(await presetSection.getByRole('button').count(), 8);
  await page.waitForFunction(() => [...document.querySelectorAll('section[aria-labelledby="maker-presets"] img')].every(img => img.complete && img.naturalWidth > 0));
  const loaded = await presetSection.locator('img').evaluateAll(imgs => imgs.map(img => ({ src: img.getAttribute('src'), width: img.naturalWidth })));
  assert.equal(loaded.length, 8);
  for (const preset of presets) {
    assert.ok(loaded.some(img => img.src.endsWith(`thumb-fantasy-${preset.config.base}-preset-${preset.id}.jpg`) && img.width > 0));
    await selectPreset(preset);
    await page.evaluate(() => window.scrollTo(0, 0));
    await page.waitForTimeout(250);
    if (['elegant', 'composed', 'scholar', 'bold'].includes(preset.id)) await capture(`desktop-${preset.id}`);
  }
  steps.push('Eight rendered portraits load; all selections apply the exact preset appearance and base');

  // 「ロゼ（elegant）」は顔が oval に変わったので、顔が「大人びた（mature）」のプリセットで確かめる。
  await selectPreset(presets.find(preset => preset.config.face === 'mature'));
  await page.getByRole('button', { name: /^アメコミ/ }).click(); await ready();
  assert.equal(await page.getByRole('button', { name: '大人びた', exact: true }).count(), 0);
  await page.getByRole('button', { name: /^ファンタジー/ }).click(); await ready();
  assert.equal(await page.getByRole('button', { name: '大人びた', exact: true }).getAttribute('aria-pressed'), 'true');
  await page.getByRole('navigation', { name: '変更するパーツ' }).getByRole('button', { name: /^目/ }).click();
  await page.getByRole('button', { name: 'まるい目', exact: true }).click();
  await page.getByRole('navigation', { name: '変更するパーツ' }).getByRole('button', { name: /^顔の形/ }).click();
  assert.equal(await page.getByRole('button', { name: '大人びた', exact: true }).getAttribute('aria-pressed'), 'true');
  steps.push('Mature face survives style round-trip and remains independent of the eye selection');

  for (const [label, height, key] of [['short', HEIGHT_RANGE.min, 'Home'], ['tall', HEIGHT_RANGE.max, 'End']]) {
    await body();
    await page.getByRole('slider', { name: '身長', exact: true }).press(key);
    const name = `身長確認-${label}`;
    await page.getByRole('textbox', { name: 'キャラクターの名前', exact: true }).fill(name);
    await page.getByRole('button', { name: '保存', exact: true }).click();
    await page.getByRole('status').filter({ hasText: `「${name}」を保存しました。` }).waitFor();
    await page.reload({ waitUntil: 'domcontentloaded' }); await ready(); await body();
    assert.equal(await page.getByRole('slider', { name: '身長', exact: true }).inputValue(), String(height));
    const document = JSON.parse(await readFile(await download('JSONを書き出す', `${label}.character.json`), 'utf8'));
    assert.equal(document.config.height, height);
    const bytes = await readFile(await download('GLBを書き出す', `${label}.glb`));
    assert.equal(bytes.readUInt32LE(0), 0x46546c67);
    assert.equal(bytes.readUInt32LE(8), bytes.length);
    const gltf = JSON.parse(bytes.subarray(20, 20 + bytes.readUInt32LE(12)).toString('utf8'));
    const root = gltf.nodes.find(node => node.name === 'CharacterMaker');
    assert.deepEqual(root.scale, [height, height, height]);
    assert.deepEqual(root.extras.characterMaker.config, document.config);
    const head = gltf.nodes.find(node => node.name === 'J_Bip_C_Head');
    // 大人びた顔の頭の比率は体のベースごとに決めた値へ変わった（assembly.ts の mature 分岐）。
    assert.equal(document.config.face, 'mature');
    const matureHead = document.config.base === 'masculine' ? [1.035, 1, 1.025] : [0.94, 1.01, 0.98];
    head.scale.forEach((value, i) => assert.ok(Math.abs(value - matureHead[i]) < 1e-6, `head scale ${head.scale} ≠ ${matureHead}`));
    assert.deepEqual(gltf.animations.map(animation => animation.name).sort(), ['face_hurt', 'hit', 'idle', 'run', 'sword_attack', 'sword_idle', 'walk']);
    exports.push({ label, height, bytes: bytes.length, scale: root.scale });
  }
  steps.push('Both height endpoints survive save/reload and match downloaded JSON plus GLB root scale and embedded config');

  await page.setViewportSize({ width: 390, height: 844 });
  for (const preset of presets) await selectPreset(preset);
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth), 390);
  await capture('mobile-presets'); await capture('mobile-page', true);
  steps.push('All eight presets can be selected at 390px; no horizontal overflow');
  await page.setViewportSize({ width: 1440, height: 960 });
  await page.getByRole('button', { name: /^アメコミ/ }).click(); await ready();
  assert.equal(await presetSection.getByRole('button').count(), 4);
  for (const preset of getPresets('comic')) await selectPreset(preset);
  steps.push('All four original comic presets still select correctly');
  assert.deepEqual(errors, []);
  await writeFile(join(output, 'result.json'), JSON.stringify({ success: true, steps, exports, errors }, null, 2));
  console.log(JSON.stringify({ success: true, output, steps }));
} catch (error) {
  await capture('failure', true).catch(() => {});
  await writeFile(join(output, 'failure.json'), JSON.stringify({ error: String(error), steps, errors }, null, 2));
  console.error(error); process.exitCode = 1;
} finally { await context.close(); await browser.close(); }
