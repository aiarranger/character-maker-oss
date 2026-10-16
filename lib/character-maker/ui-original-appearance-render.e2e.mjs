// Independent owned browser contexts only. Never attach to the user's browser.
// node lib/character-maker/ui-original-appearance-render.e2e.mjs /tmp/output --compare-original
// node lib/character-maker/ui-original-appearance-render.e2e.mjs /tmp/output --contact-sheet [--character=generic-human-a] [--model=/tmp/probe.glb]
// node lib/character-maker/ui-original-appearance-render.e2e.mjs /tmp/output --face-keys --character=generic-human-a --model=/tmp/probe.glb
// オリジナルの編集画面は Character Studio（/character-maker）で開く（tools/lib/studio-e2e.mjs）。モデルは studio-catalog.json の /models/<sha>.glb。
// 元モデルを表示用モデルの代わりに読ませて見比べる --compare-original はロボだけが対象。人間は髪パーツ（modular）、
// ネコは耳などの編集用パーツ入りのモデルで組み立てるので、元モデルでは「編集用パーツを読み込めません」になる。
// ロボ・ネコは新しく作るとき髪を選べない（頭・耳・目・口・色のパーツを選ぶ）ので、髪を使う他の確認は人間だけが対象。
import assert from 'node:assert/strict';
import { readFile, mkdir, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { createHash } from 'node:crypto';
import sharp from 'sharp';
import { ORIGINAL_FACE_CONTROLS } from './original-config.ts';
import { importPlaywright, ROOT } from '../../tools/lib/paths.mjs';
import { catalogEntry, category as studioCategory, current, displayedModel, installReadTool, inspector, openStudio, pause, ready, selectOriginal } from '../../tools/lib/studio-e2e.mjs';

const args = process.argv.slice(2);
const output = args.find(value => !value.startsWith('--')) ?? '/tmp/maker-original-appearance-render';
const compare = args.includes('--compare-original');
const bodyAndColor = args.includes('--body-and-color');
const hairClose = args.includes('--hair-close');
const faceKeys = args.includes('--face-keys');
const closeSide = args.includes('--include-side');
const closeView = args.find(value => value.startsWith('--view='))?.slice('--view='.length);
const specifiedId = args.find(value => value.startsWith('--character='))?.slice('--character='.length);
const modelOverride = args.find(value => value.startsWith('--model='))?.slice('--model='.length);
assert.equal([compare, bodyAndColor, hairClose, faceKeys, args.includes('--contact-sheet')].filter(Boolean).length, 1, 'Choose exactly one operation');
assert.ok(!modelOverride || specifiedId, 'A probe model requires a specific character');
await mkdir(output, { recursive: true });
const human = id => !!catalogEntry(id).modularModel;
const applies = id => compare ? id === 'generic-robot' : human(id);
const ids = ['generic-human-a', 'generic-human-b', 'generic-robot', 'generic-cat'].filter(id => specifiedId ? specifiedId.split(',').includes(id) : applies(id));
assert.ok(ids.length);
assert.ok(!modelOverride || ids.length === 1, 'A probe model applies to one character');
assert.ok(ids.every(applies), compare ? '--compare-original: 元モデルで組み立てられるのはロボだけです' : 'ロボ・ネコは髪を選べないので、髪の確認は人間だけです');
const sources = await Promise.all(ids.map(async id => {
  const entry = catalogEntry(id);
  const bytes = await readFile(join(ROOT, 'public', entry.modelUrl));
  assert.equal(createHash('sha256').update(bytes).digest('hex'), entry.modelSha256);
  // 人間の既定の髪は髪パーツの先頭（makeOriginalDocument）。ロボ・ネコは元の髪型。
  return { id, name: entry.name, bytes, sha256: entry.modelSha256, displayed: displayedModel(entry), defaultHair: entry.modularModel?.hairstyles[0] ?? 'original' };
}));
const { chromium } = await importPlaywright();
const browser = await chromium.launch({ channel: 'chrome', headless: true });
const contexts = [], errors = [], results = [];
const sides = [['front', '正面'], ['side', '横'], ['back', '後ろ']];
const category = studioCategory;
const hairGroup = page => inspector(page).getByRole('group', { name: '髪型', exact: true });
// 髪型の選択肢はキャラごとに違う（人間は髪パーツ、ロボ・ネコは元の髪型・ショート・サイドポニー）。画面の選択肢をそのまま使う。
const hairLabels = page => hairGroup(page).locator('button strong').allInnerTexts();
const otherHair = page => hairGroup(page).locator('button[aria-pressed="false"]').first();
async function launch(source, variant) {
  console.log(JSON.stringify({ id: source.id, variant, phase: 'loading' }));
  const context = await browser.newContext({ viewport: { width: 1440, height: 960 }, deviceScaleFactor: 1 });
  await installReadTool(context);
  contexts.push(context);
  const page = await context.newPage(); await page.bringToFront(); page.setDefaultTimeout(30000);
  page.on('pageerror', error => errors.push({ id: source.id, variant, message: error.message }));
  const loaded = [];
  await page.route('**/models/**', async route => {
    const url = new URL(route.request().url());
    const isSelected = url.pathname === source.displayed.url;
    if (isSelected && (variant === 'source' || modelOverride)) {
      const bytes = variant === 'source' ? source.bytes : await readFile(modelOverride);
      loaded.push({ url: url.pathname, actualSha256: createHash('sha256').update(bytes).digest('hex') });
      return route.fulfill({ status: 200, contentType: 'model/gltf-binary', body: bytes });
    }
    const response = await route.fetch(), bytes = await response.body();
    loaded.push({ url: url.pathname, actualSha256: createHash('sha256').update(bytes).digest('hex') });
    await route.fulfill({ response, body: bytes });
  });
  await openStudio(page); await selectOriginal(page, source.id);
  await pause(page);
  await category(page, '表情'); await page.getByRole('button', { name: 'ふだんの表情に戻す', exact: true }).click();
  await category(page, '動き');
  const hairMotion = page.getByRole('checkbox', { name: '髪を揺らす', exact: true });
  if (await hairMotion.count()) await hairMotion.uncheck();
  // Comparing two independent source models requires the exact same clip time.
  // Fit/color contacts only need one stable paused pose; keep that existing pose.
  if (compare) {
    const progress = page.getByRole('slider', { name: '再生位置', exact: true });
    await progress.press('End');
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    await progress.press('Home');
    await page.waitForFunction(() => document.querySelector('input[aria-label="再生位置"]')?.value === '0').catch(async error => {
      throw new Error(`${source.id}/${variant}: seek zero failed; ${JSON.stringify({ value: await page.evaluate(() => document.querySelector('input[aria-label="再生位置"]')?.value), config: (await current(page))?.config })}`, { cause: error });
    });
  }
  await page.waitForFunction(hairstyle => {
    const c = window.__characterTools.read_character_edit?.execute({})?.config;
    return c && !c.playing && !c.autoBlink && !c.speaking && !c.hairMotion && c.appearance.build === 0 && c.appearance.hairstyle === hairstyle && c.appearance.hairColor === 'source';
  }, source.defaultHair);
  await page.getByRole('group', { name: '表示範囲', exact: true }).getByRole('button', { name: '顔', exact: true }).click();
  await category(page, human(source.id) ? '髪' : '体型');
  console.log(JSON.stringify({ id: source.id, variant, phase: 'ready' }));
  return { page, loaded };
}
async function pixels(page, path) {
  await page.bringToFront();
  await page.evaluate(() => window.scrollTo(0, 0));
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(() => requestAnimationFrame(resolve)))));
  const data = await page.evaluate(() => {
    const canvas = document.querySelector('[aria-label="キャラクターの3Dプレビュー"] canvas');
    const scratch = document.createElement('canvas'); scratch.width = canvas.width; scratch.height = canvas.height;
    const ctx = scratch.getContext('2d'); ctx.drawImage(canvas, 0, 0);
    const p = ctx.getImageData(0, 0, scratch.width, scratch.height).data;
    let opaque = 0; for (let i = 3; i < p.length; i += 4) if (p[i] > 250) opaque++;
    if (opaque < 1000) throw new Error('Canvas has no painted model');
    return { width: scratch.width, height: scratch.height, opaque, png: scratch.toDataURL('image/png').split(',')[1] };
  });
  const png = Buffer.from(data.png, 'base64'); await writeFile(path, png);
  const raw = await sharp(png).ensureAlpha().raw().toBuffer();
  return { width: data.width, height: data.height, opaque: data.opaque, png, raw };
}
function difference(a, b, firstRow = 0) {
  assert.equal(a.width, b.width); assert.equal(a.height, b.height);
  let changed = 0, changedOver2 = 0, alphaChanged = 0, sum = 0, max = 0;
  for (let i = firstRow * a.width * 4; i < a.raw.length; i += 4) {
    let pixelMax = 0;
    for (let c = 0; c < 4; c++) { const d = Math.abs(a.raw[i + c] - b.raw[i + c]); sum += d; max = Math.max(max, d); pixelMax = Math.max(pixelMax, d); }
    if (pixelMax) changed++; if (pixelMax > 2) changedOver2++; if (a.raw[i + 3] !== b.raw[i + 3]) alphaChanged++;
  }
  return { pixels: a.width * (a.height - firstRow), changed, changedOver2, alphaChanged, maxChannelDelta: max, meanChannelDelta: sum / (a.width * (a.height - firstRow) * 4) };
}
const labelImage = text => Buffer.from(`<svg width="300" height="30"><rect width="300" height="30" fill="#fff"/><text x="12" y="22" font-family="Arial,sans-serif" font-size="16" fill="#233b37">${text}</text></svg>`);
async function sheet(source, cells) {
  const items = [];
  for (let i = 0; i < cells.length; i++) {
    const left = (i % 3) * 300, top = Math.floor(i / 3) * 262;
    items.push({ input: labelImage(cells[i].label), left, top });
    items.push({ input: await sharp(cells[i].png).flatten({ background: '#edf2eb' }).resize(300, 232, { fit: 'contain', background: '#edf2eb' }).png().toBuffer(), left, top: top + 30 });
  }
  const path = join(output, `${source.id}-contact.jpg`);
  await sharp({ create: { width: 900, height: Math.ceil(cells.length / 3) * 262, channels: 3, background: '#fff' } }).composite(items).jpeg({ quality: 93 }).toFile(path);
  return path;
}
try {
  for (const source of sources) {
    if (compare) {
      const original = await launch(source, 'source'), derived = await launch(source, 'derived');
      const shots = [], comparisons = [];
      for (const [view, label] of sides) {
        await original.page.getByRole('group', { name: '見る方向', exact: true }).getByRole('button', { name: label, exact: true }).click();
        await derived.page.getByRole('group', { name: '見る方向', exact: true }).getByRole('button', { name: label, exact: true }).click();
        const a = await pixels(original.page, join(output, `${source.id}-source-${view}.png`));
        const b = await pixels(derived.page, join(output, `${source.id}-derived-${view}.png`));
        const delta = difference(a, b); comparisons.push({ view, ...delta });
        shots.push({ label: `${source.id} source ${view}`, png: a.png }, { label: `${source.id} derived ${view}`, png: b.png });
        console.log(JSON.stringify({ id: source.id, view, ...delta }));
      }
      await sheet(source, shots);
      results.push({ id: source.id, operation: 'compare-original', sourceSha256: source.sha256, loaded: { source: original.loaded, derived: derived.loaded }, comparisons });
      await original.page.context().close(); await derived.page.context().close();
    } else if (faceKeys) {
      const { page, loaded } = await launch(source, 'derived');
      const shots = [];
      const snap = async name => {
        const image = await pixels(page, join(output, `${source.id}-${name}.png`));
        shots.push({ label: name, png: image.png });
        return image;
      };
      const original = await snap('original-source');
      await page.getByRole('button', { name: 'ローズ', exact: true }).click();
      await snap('original-rose');
      await page.getByRole('button', { name: '元の髪色', exact: true }).click();
      const restored = await snap('original-restored');
      const sourceColorRestored = difference(original, restored);
      await otherHair(page).click();
      await page.getByRole('button', { name: 'ローズ', exact: true }).click();
      await category(page, '表情');
      await page.getByRole('button', { name: 'ふだんの表情に戻す', exact: true }).click();
      await page.getByText('目と口を細かく整える', { exact: true }).click();
      const neutral = await snap('short-rose-neutral');
      const controls = [];
      for (const [key, label] of ORIGINAL_FACE_CONTROLS) {
        const slider = page.getByRole('slider', { name: label, exact: true });
        if (!await slider.count()) continue; // そのキャラに無い表情の調整は出ない
        await slider.press('End');
        await page.waitForFunction(key => window.__characterTools.read_character_edit?.execute({})?.config.face[key] === 1, key);
        const active = await snap(`short-rose-${key}`);
        controls.push({ key, value: 1, visualDifference: difference(neutral, active) });
        await slider.press('Home');
        await page.waitForFunction(key => window.__characterTools.read_character_edit?.execute({})?.config.face[key] === 0, key);
      }
      const reset = await snap('short-rose-face-restored');
      await category(page, '髪');
      await page.getByRole('button', { name: '髪型と色を標準に戻す', exact: true }).click();
      const afterReplacement = await snap('original-after-replacement');
      const contactSheet = await sheet(source, shots);
      results.push({ id: source.id, operation: 'face-keys', sourceSha256: source.sha256, loaded,
        sourceColorRestored, controls, faceReset: difference(neutral, reset),
        originalAfterReplacement: difference(original, afterReplacement), contactSheet });
      console.log(JSON.stringify(results.at(-1)));
      await page.context().close();
    } else if (hairClose) {
      const { page, loaded } = await launch(source, 'derived');
      await otherHair(page).click();
      await page.getByRole('button', { name: 'ローズ', exact: true }).click();
      const shots = [];
      for (const [view, direction] of closeView ? sides.filter(([view]) => view === closeView) : sides.slice(0, closeSide ? 2 : 1)) {
        await page.getByRole('group', { name: '見る方向', exact: true }).getByRole('button', { name: direction, exact: true }).click();
        const path = join(output, `${source.id}-short-rose-${view}.png`);
        const image = await pixels(page, path);
        await page.screenshot({ path: join(output, `${source.id}-short-rose-${view}-ui.jpg`), type: 'jpeg', quality: 95 });
        shots.push({ view, path, opaque: image.opaque });
      }
      results.push({ id: source.id, operation: 'hair-close', loaded, shots });
      console.log(JSON.stringify(results.at(-1)));
      await page.context().close();
    } else if (bodyAndColor) {
      const { page, loaded } = await launch(source, 'derived');
      const base = await pixels(page, join(output, `${source.id}-color-source.png`));
      await page.getByRole('button', { name: 'ローズ', exact: true }).click();
      const rose = await pixels(page, join(output, `${source.id}-color-rose.png`));
      await page.getByRole('button', { name: '元の髪色', exact: true }).click();
      const restored = await pixels(page, join(output, `${source.id}-color-restored.png`));
      const resetDelta = difference(base, restored), lowerRegionDelta = difference(base, rose, Math.floor(base.height * .65));
      assert.equal(resetDelta.changed, 0, `${source.id}: restoring the source hair color restores every pixel`);
      // This region may include long original hair; preserve its measured
      // difference for visual review instead of misclassifying it as clothing.
      await page.getByRole('group', { name: '表示範囲', exact: true }).getByRole('button', { name: '全身', exact: true }).click();
      await category(page, '体型');
      const shots = [];
      for (const [key, build] of [['Home', -1], ['End', 1]]) {
        await inspector(page).getByRole('slider', { name: '体型', exact: true }).press(key);
        for (const [view, direction] of sides.slice(0, 2)) {
          await page.getByRole('group', { name: '見る方向', exact: true }).getByRole('button', { name: direction, exact: true }).click();
          const image = await pixels(page, join(output, `${source.id}-build-${build}-${view}.png`));
          shots.push({ label: `${source.id} build ${build} ${view}`, png: image.png });
        }
      }
      const contactSheet = await sheet(source, shots);
      await page.getByRole('region', { name: '共通のカスタマイズ', exact: true }).getByRole('button', { name: '歩く', exact: true }).click();
      await page.waitForTimeout(350); await pause(page);
      await category(page, '動き');
      const playbackBefore = Number(await page.getByRole('slider', { name: '再生位置', exact: true }).inputValue());
      await category(page, '髪');
      const beforeColor = await pixels(page, join(output, `${source.id}-paused-before-color-change.png`));
      await page.getByRole('button', { name: 'ローズ', exact: true }).click();
      const afterColor = await pixels(page, join(output, `${source.id}-paused-color-change.png`));
      await category(page, '動き');
      const playbackAfter = Number(await page.getByRole('slider', { name: '再生位置', exact: true }).inputValue());
      const pausedBodyDelta = difference(beforeColor, afterColor, Math.floor(beforeColor.height * .5));
      results.push({ id: source.id, operation: 'body-and-color', loaded, resetDelta, lowerRegionDelta, playbackBefore, playbackAfter,
        playbackPositionStable: Math.abs(playbackAfter - playbackBefore) < .001, pausedBodyDelta, contactSheet });
      await page.context().close();
      console.log(JSON.stringify(results.at(-1)));
    } else {
      const { page, loaded } = await launch(source, 'derived');
      const shots = [];
      for (const [index, label] of (await hairLabels(page)).entries()) {
        const style = `hair${index + 1}`;
        await hairGroup(page).locator('button').filter({ has: page.locator('strong').getByText(label, { exact: true }) }).click();
        for (const [view, direction] of sides) {
          await page.getByRole('group', { name: '見る方向', exact: true }).getByRole('button', { name: direction, exact: true }).click();
          const image = await pixels(page, join(output, `${source.id}-${style}-${view}.png`));
          shots.push({ label: `${source.id} ${style} ${view}`, png: image.png });
        }
      }
      const contactSheet = await sheet(source, shots);
      results.push({ id: source.id, operation: 'contact-sheet', sourceSha256: source.sha256, loaded, contactSheet });
      await page.context().close();
      console.log(JSON.stringify({ id: source.id, contactSheet }));
    }
  }
  assert.deepEqual(errors, []);
  await writeFile(join(output, 'result.json'), JSON.stringify({ complete: true, exactPixelMatch: compare ? results.every(result => result.comparisons.every(item => item.changed === 0)) : undefined, results, errors }, null, 2));
} catch (error) {
  console.error(error);
  await writeFile(join(output, 'failure.json'), JSON.stringify({ error: String(error), results, errors }, null, 2));
  throw error;
} finally { for (const context of contexts) await context.close().catch(() => {}); await browser.close(); }
