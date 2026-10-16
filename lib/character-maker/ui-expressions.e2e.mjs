// Run the real maker controls in a fresh Chrome context; never touches the user's browser storage.
// node lib/character-maker/ui-expressions.e2e.mjs [output-directory]
import assert from 'node:assert/strict';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { EXPRESSION_CHOICES } from './config.ts';
import { importPlaywright, MAKER_URL } from '../../tools/lib/paths.mjs';

const output = process.argv[2] ?? '/tmp/maker-expression-ui';
await mkdir(output, { recursive: true });
const { chromium } = await importPlaywright();
const browser = await chromium.launch({ channel: 'chrome', headless: true });
const context = await browser.newContext({ viewport: { width: 1440, height: 960 } });
const page = await context.newPage();
page.setDefaultTimeout(30000);
const errors = [], steps = [], exports = [], selections = [], renderedCaptures = [];
page.on('pageerror', error => errors.push(error.message));
page.on('console', message => {
  if (message.type() === 'error' && message.text().includes('Encountered two children with the same key')) errors.push(message.text());
});
const navigation = page.getByRole('navigation', { name: '変更するパーツ' });
const editor = page.getByRole('region', { name: '表情', exact: true });
const ready = () => page.getByRole('button', { name: 'GLBを書き出す', exact: true }).waitFor({ state: 'visible', timeout: 120000 })
  .then(() => page.waitForFunction(() => [...document.querySelectorAll('button')].some(button => button.textContent === 'GLBを書き出す' && !button.disabled), null, { timeout: 120000 }));
const category = name => navigation.getByRole('button', { name: new RegExp(`^${name}`) }).click();
async function painted() {
  // The enabled export button means assembly is ready, not that Canvas has
  // painted it. Check the actual opaque model pixels, excluding the faint floor.
  const result = await page.waitForFunction(() => {
    const canvas = document.querySelector('[aria-label="キャラクターの3Dプレビュー"] canvas');
    if (!canvas) return false;
    const copy = document.createElement('canvas'); copy.width = canvas.width; copy.height = canvas.height;
    const context = copy.getContext('2d'); context.drawImage(canvas, 0, 0);
    const pixels = context.getImageData(0, 0, copy.width, copy.height).data;
    let opaquePixels = 0;
    for (let index = 3; index < pixels.length; index += 4) if (pixels[index] > 250) opaquePixels++;
    return opaquePixels > 1000 && { opaquePixels, width: canvas.width, height: canvas.height };
  }, null, { timeout: 15000 });
  const sample = await result.jsonValue(); await result.dispose(); return sample;
}
async function capture(name, fullPage = false) {
  renderedCaptures.push({ name, ...await painted() });
  await page.waitForFunction(() => {
    const images = [...document.querySelectorAll('[aria-label="気持ちを選ぶ"] img')];
    return images.length === 6 && images.every(image => image.complete && image.naturalWidth > 0);
  });
  await page.locator('[aria-label="気持ちを選ぶ"] img').evaluateAll(images => Promise.all(images.map(image => image.decode())));
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  await page.screenshot({ path: join(output, `${name}.jpg`), type: 'jpeg', quality: 93, fullPage });
}
async function expectDraft(expected) {
  await page.waitForFunction(values => {
    const document = JSON.parse(sessionStorage.getItem('characters.character-maker.v1.draft') ?? 'null')?.document;
    if (!document) return false;
    const config = { style: 'comic', expression: 'neutral', expressionStrength: 1, ...document.config };
    return Object.entries(values).every(([key, value]) => config[key] === value);
  }, expected);
}
async function selectExpression(id) {
  const choice = EXPRESSION_CHOICES.find(item => item.id === id);
  if (choice.group !== 'emotion' && !await editor.locator('details').getAttribute('open').then(value => value !== null)) {
    await editor.getByText('目・口・視線も試す', { exact: true }).click();
  }
  const button = editor.getByRole('button', { name: choice.label, exact: true });
  await button.click();
  assert.equal(await button.getAttribute('aria-pressed'), 'true');
  await expectDraft({ expression: id });
}
async function download(label, filename) {
  const pending = page.waitForEvent('download', { timeout: 120000 });
  await page.getByRole('button', { name: label, exact: true }).click();
  const file = join(output, filename);
  await (await pending).saveAs(file);
  return file;
}
function inspectGlb(bytes, document) {
  assert.equal(bytes.readUInt32LE(0), 0x46546c67);
  assert.equal(bytes.readUInt32LE(4), 2);
  assert.equal(bytes.readUInt32LE(8), bytes.length);
  const gltf = JSON.parse(bytes.subarray(20, 20 + bytes.readUInt32LE(12)).toString('utf8'));
  const root = gltf.nodes.find(node => node.name === 'CharacterMaker');
  assert.deepEqual(root.extras.characterMaker.config, document.config);
  assert.deepEqual(gltf.animations.map(animation => animation.name).sort(), ['face_hurt', 'hit', 'idle', 'run', 'sword_attack', 'sword_idle', 'walk']);
  const profile = root.extras.characterMaker.expressionMorphs;
  const expected = profile[document.config.expression];
  assert.ok(expected && Object.keys(expected).length, 'The downloaded expression has authored morph bindings');
  const found = new Set();
  for (const node of gltf.nodes) {
    const mesh = gltf.meshes?.[node.mesh];
    if (!mesh?.extras?.targetNames) continue;
    const weights = node.weights ?? mesh.weights;
    for (const [name, authoredWeight] of Object.entries(expected)) {
      const index = mesh.extras.targetNames.indexOf(name);
      if (index < 0) continue;
      // タイプBの瞳を閉じ切るときだけ奥へ下げる補正（makerClosedEyeSurface*、expressions.ts）は、強さ 0.75〜1 の間だけ滑らかに効く。
      const strength = document.config.expressionStrength;
      const surface = Math.max(0, Math.min(1, (strength - 0.75) / 0.25));
      const want = name.startsWith('makerClosedEyeSurface') ? authoredWeight * surface * surface * (3 - 2 * surface) : authoredWeight * strength;
      assert.ok(Math.abs(weights[index] - want) < 1e-6, `Saved expression weight ${name}`);
      found.add(name);
    }
  }
  assert.deepEqual([...found].sort(), Object.keys(expected).sort());
  return { bytes: bytes.length, schemaVersion: document.schemaVersion, config: document.config, clips: gltf.animations.map(animation => animation.name), expressionTargets: [...found] };
}

try {
  await page.goto(`${MAKER_URL}/character-maker/classic`, { waitUntil: 'domcontentloaded' });
  await ready();
  // 初期値がファンタジー・タイプA・ふつうになったので、最初の「タイプA」「ふつう」は変更にならず控えが書かれない。
  // 名前を変えて控え（sessionStorage の draft）を作ってから選ぶ。
  await page.getByRole('textbox', { name: 'キャラクターの名前', exact: true }).fill('表情確認');
  for (const [base, label] of [['feminine', 'タイプA'], ['masculine', 'タイプB']]) {
    await category('体型');
    await page.getByRole('button', { name: label, exact: true }).click();
    await ready(); await category('表情');
    const cards = editor.getByRole('group', { name: '気持ちを選ぶ', exact: true });
    assert.equal(await cards.getByRole('button').count(), 6);
    await page.waitForFunction(() => [...document.querySelectorAll('[aria-label="気持ちを選ぶ"] img')].every(image => image.complete && image.naturalWidth > 0));
    assert.equal(await cards.locator('img').count(), 6);
    for (const choice of EXPRESSION_CHOICES) {
      await selectExpression(choice.id);
      if (choice.group === 'emotion') {
        await page.evaluate(() => window.scrollTo(0, 0));
        await capture(`${base}-${choice.id}`);
      }
      selections.push({ base, expression: choice.id });
    }
    steps.push(`${base}: all 18 expressions select through the visible controls; six rendered emotion thumbnails load`);
    await selectExpression('happy');
    const strength = editor.getByRole('slider', { name: '表情の強さ', exact: true });
    await strength.press('Home');
    await expectDraft({ expression: 'happy', expressionStrength: 0 });
    await strength.press('End');
    for (let index = 0; index < 7; index++) await strength.press('ArrowLeft');
    assert.equal(await strength.inputValue(), '0.65');
    await expectDraft({ expression: 'happy', expressionStrength: 0.65 });

    // Appearance controls remain independent of the chosen expression.
    await category('目'); await page.getByRole('button', { name: 'まるい目', exact: true }).click();
    await category('口'); await page.getByRole('button', { name: 'ふっくら', exact: true }).click();
    await category('顔の形'); await page.getByRole('button', { name: '大人びた', exact: true }).click();
    await expectDraft({ base, expression: 'happy', expressionStrength: 0.65, eyes: 'round', mouth: 'full', face: 'mature' });
    await page.getByRole('button', { name: /^アメコミ/ }).click(); await ready();
    assert.equal(await navigation.getByRole('button', { name: /^表情/ }).count(), 0);
    assert.equal(await navigation.getByRole('button').count(), 6);
    await expectDraft({ style: 'comic', expression: 'neutral' });
    await page.getByRole('button', { name: /^ファンタジー/ }).click(); await ready(); await category('表情');
    assert.equal(await editor.getByRole('button', { name: '笑顔', exact: true }).getAttribute('aria-pressed'), 'true');
    assert.equal(await strength.inputValue(), '0.65');
    await expectDraft({ expression: 'happy', expressionStrength: 0.65, face: 'mature' });
    steps.push(`${base}: expression strength, eyes, mouth and mature face are independent; fantasy expression survives a comic style round-trip`);

    const name = `表情確認-${base}`;
    await page.getByRole('textbox', { name: 'キャラクターの名前', exact: true }).fill(name);
    await page.getByRole('button', { name: '保存', exact: true }).click();
    await page.getByRole('status').filter({ hasText: `「${name}」を保存しました。` }).waitFor();
    await page.reload({ waitUntil: 'domcontentloaded' }); await ready(); await category('表情');
    assert.equal(await editor.getByRole('button', { name: '笑顔', exact: true }).getAttribute('aria-pressed'), 'true');
    assert.equal(await strength.inputValue(), '0.65');
    const jsonFile = await download('JSONを書き出す', `${base}.character.json`);
    const document = JSON.parse(await readFile(jsonFile, 'utf8'));
    // 武器（既定 cake_sword）の追加で汎用キャラの形式は schema 5 になった。
    assert.equal(document.schemaVersion, 5);
    assert.equal(document.config.expression, 'happy');
    assert.equal(document.config.expressionStrength, 0.65);
    exports.push(inspectGlb(await readFile(await download('GLBを書き出す', `${base}.glb`)), document));
    const canvasBeforeImport = await page.locator('[aria-label="キャラクターの3Dプレビュー"] canvas').elementHandle();
    await selectExpression('angry');
    const chooser = page.waitForEvent('filechooser');
    await page.getByRole('button', { name: 'JSONを読み込む', exact: true }).click();
    await (await chooser).setFiles(jsonFile);
    assert.equal(await page.getByRole('alertdialog').count(), 0, 'JSON switches without a confirmation');
    await ready();
    await expectDraft(document.config);
    assert.equal(await editor.getByRole('button', { name: '笑顔', exact: true }).getAttribute('aria-pressed'), 'true');
    assert.equal(await strength.inputValue(), '0.65');
    await page.evaluate(() => window.scrollTo(0, 0)); await capture(`${base}-saved-expression`);
    assert.equal(await canvasBeforeImport.evaluate(canvas => canvas === document.querySelector('[aria-label="キャラクターの3Dプレビュー"] canvas')), true, 'Import preserves the live Canvas');
    await canvasBeforeImport.dispose();
    await page.waitForTimeout(1000);
    await capture(`${base}-saved-expression-after-1s`);
    steps.push(`${base}: save, reload, JSON download/reimport and GLB download retain expression, intensity, appearance and all five motion clips`);
    steps.push(`${base}: JSON reimport preserves the Canvas and renders the character immediately after paint and one second later`);
  }
  await page.getByRole('button', { name: '剣を振る', exact: true }).click();
  await page.getByRole('group', { name: '動作', exact: true }).getByRole('button', { name: '剣を構える', exact: true }).waitFor();
  await page.waitForFunction(() => [...document.querySelectorAll('[aria-label="動作"] button')].some(button => button.textContent === '剣を構える' && button.getAttribute('aria-pressed') === 'true'));
  await expectDraft({ expression: 'happy', expressionStrength: 0.65 });
  steps.push('Sword attack completes into the sword idle pose while the selected expression remains active');
  await page.getByRole('group', { name: '動作', exact: true }).getByRole('button', { name: '待機', exact: true }).click();
  await category('表情');
  await page.setViewportSize({ width: 390, height: 844 });
  await selectExpression('surprised');
  await selectExpression('blinkLeft');
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth), 390);
  await capture('mobile-expression');
  await editor.getByRole('button', { name: 'ふつうの表情に戻す', exact: true }).click();
  await expectDraft({ expression: 'neutral', expressionStrength: 1 });
  assert.equal(await editor.getByRole('slider', { name: '表情の強さ', exact: true }).isDisabled(), true);
  steps.push('At 390px both emotion cards and detail choices are selectable without overflow; reset restores neutral');
  assert.deepEqual(errors, []);
  await writeFile(join(output, 'result.json'), JSON.stringify({ success: true, steps, selections, exports, renderedCaptures, errors }, null, 2));
  console.log(JSON.stringify({ success: true, output, steps }));
} catch (error) {
  await page.screenshot({ path: join(output, 'failure.jpg'), type: 'jpeg', quality: 93, fullPage: true }).catch(() => {});
  await writeFile(join(output, 'failure.json'), JSON.stringify({ error: String(error), steps, selections, exports, renderedCaptures, errors }, null, 2));
  console.error(error); process.exitCode = 1;
} finally { await context.close(); await browser.close(); }
