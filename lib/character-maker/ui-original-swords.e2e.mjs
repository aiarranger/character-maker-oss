// Actual original-maker controls, isolated Chrome/storage, no user browser access.
// オリジナルの編集画面は Character Studio（/character-maker）で開く（tools/lib/studio-e2e.mjs）。
// node lib/character-maker/ui-original-swords.e2e.mjs /tmp/output [--ids=generic-human-a,...]
import assert from 'node:assert/strict';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { createHash } from 'node:crypto';
import { importPlaywright, ROOT } from '../../tools/lib/paths.mjs';
import { catalogEntry, category as studioCategory, current, exportFile, importJson, installReadTool, inspector, openStudio, ready as studioReady, save, selectOriginal, waitConfig } from '../../tools/lib/studio-e2e.mjs';
const output = process.argv.slice(2).find(value => !value.startsWith('--')) ?? '/tmp/maker-original-sword-ui';
const selected = process.argv.find(value => value.startsWith('--ids='))?.slice(6).split(',');
const ids = ['generic-human-a', 'generic-human-b', 'generic-robot', 'generic-cat'].filter(id => !selected || selected.includes(id));
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
await mkdir(output, { recursive: true });
// characters の character.json の代わりに、画面と同じカタログ（studio-catalog.json）と public/models の元モデルを使う。
const sources = await Promise.all(ids.map(async id => {
  const entry = catalogEntry(id);
  const glb = join(ROOT, 'public', entry.modelUrl);
  assert.equal(hash(await readFile(glb)), entry.modelSha256);
  return { id, name: entry.name, glb, sha256: entry.modelSha256 };
}));
const { chromium } = await importPlaywright();
const browser = await chromium.launch({ channel: 'chrome', headless: true });
const context = await browser.newContext({ viewport: { width: 1440, height: 960 } });
await installReadTool(context);
const page = await context.newPage(); page.setDefaultTimeout(30000);
const errors = [], results = [];
page.on('pageerror', error => errors.push(error.message));
page.on('console', message => { if (message.type() === 'error' && /Encountered two children|THREE.PropertyBinding/.test(message.text())) errors.push(message.text()); });
const category = name => studioCategory(page, name);
const ready = () => studioReady(page);
const draft = () => current(page);
const expectConfig = (values, options) => waitConfig(page, values, options);
// 動作は右の「動き」タブで選ぶ。武器が選べるようになり「剣を構える・剣を振る」は「武器を構える・武器を振る」になった。
const motionGroup = () => inspector(page).getByRole('group', { name: '動作', exact: true });
async function painted(name) {
  const result = await page.waitForFunction(() => {
    const canvas = document.querySelector('[aria-label="キャラクターの3Dプレビュー"] canvas'); if (!canvas) return false;
    const copy = document.createElement('canvas'); copy.width = canvas.width; copy.height = canvas.height;
    const ctx = copy.getContext('2d'); ctx.drawImage(canvas, 0, 0); const pixels = ctx.getImageData(0, 0, copy.width, copy.height).data;
    let count = 0, x0 = copy.width, y0 = copy.height, x1 = 0, y1 = 0;
    for (let y = 0; y < copy.height; y++) for (let x = 0; x < copy.width; x++) if (pixels[(y * copy.width + x) * 4 + 3] > 250) { count++; x0 = Math.min(x0, x); y0 = Math.min(y0, y); x1 = Math.max(x1, x); y1 = Math.max(y1, y); }
    return count > 1000 && { count, width: copy.width, height: copy.height, x0, y0, x1, y1 };
  }, null, { timeout: 15000 });
  const sample = await result.jsonValue(); await result.dispose();
  await page.evaluate(async () => { window.scrollTo(0, 0);
    const visible = [...document.images].filter(image => { const box = image.getBoundingClientRect(); return box.bottom > 0 && box.top < innerHeight && image.complete && image.naturalWidth > 0; });
    await Promise.all(visible.map(image => image.decode().catch(() => {})));
    await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  });
  await page.screenshot({ path: join(output, `${name}.jpg`), type: 'jpeg', quality: 95 }); return sample;
}
async function pause() { const button = page.getByRole('button', { name: '動作を一時停止', exact: true }); if (await button.count()) await button.click(); await expectConfig({ playing: false }); }
const download = (label, filename) => exportFile(page, { 'GLBを書き出す': '3Dモデル（GLB）', 'JSONを書き出す': '編集設定（JSON）' }[label], join(output, filename));
function inspectGlb(bytes, document, variant) {
  assert.equal(bytes.readUInt32LE(0), 0x46546c67); assert.equal(bytes.readUInt32LE(8), bytes.length);
  const gltf = JSON.parse(bytes.subarray(20, 20 + bytes.readUInt32LE(12)).toString());
  const root = gltf.nodes.find(node => node.name === 'CharacterMakerOriginal'); assert.deepEqual(root.extras.characterMaker.config, document.config);
  assert.equal(root.extras.characterMaker.modelSha256, document.modelSha256);
  const exportInfo = root.extras.characterMaker.export; assert.equal(exportInfo.variant, variant);
  const clips = gltf.animations.map(clip => clip.name);
  const families = clips.map(name => name.split(':').at(-1)).sort();
  assert.deepEqual(families, variant === 'armed' ? ['face_hurt', 'hit', 'sword_attack', 'sword_idle'] : ['face_hurt', 'hit', 'idle', 'run', 'walk']);
  // のけぞり（hit）と ＞＜（face_hurt）は最後に足す。＞＜ のノードは頭の骨の下で scale 0（docs/main-character-spec.md）
  assert.deepEqual(clips.slice(-2), ['hit', 'face_hurt']);
  const hurtNode = gltf.nodes.findIndex(node => node.name === 'maker-hurt-face'); assert(hurtNode >= 0);
  assert.deepEqual(gltf.nodes[hurtNode].scale, [0, 0, 0]);
  assert.equal(gltf.nodes.find(node => node.children?.includes(hurtNode))?.name, 'Head');
  for (const name of ['hit', 'face_hurt']) assert(gltf.animations.find(clip => clip.name === name).channels.some(channel => channel.target.node === hurtNode && channel.target.path === 'scale'), `${name} switches the ＞＜ node`);
  assert(gltf.animations.find(clip => clip.name === 'hit').channels.some(channel => gltf.nodes[channel.target.node].name === 'Head' && channel.target.path === 'rotation'), 'hit moves the body');
  const names = new Set(gltf.nodes.map(node => node.name));
  const visibilityNames = new Set(['OriginalNativeRightHand', 'OriginalSwordRightHand', 'CharacterMakerSword']);
  for (const clip of gltf.animations) for (const channel of clip.channels) {
    assert(channel.target.node >= 0 && channel.target.node < gltf.nodes.length, `${clip.name} animation target resolves`);
    const node = gltf.nodes[channel.target.node];
    assert(!(visibilityNames.has(node.name) && channel.target.path === 'scale'), `${clip.name} excludes visibility track ${node.name}.scale`);
  }
  let attachment = null;
  if (variant === 'standard') {
    for (const name of visibilityNames) assert(!names.has(name), `standard GLB excludes ${name}`);
    assert.equal(exportInfo.weapon.included, false);
  } else {
    const swordIndex = gltf.nodes.findIndex(node => node.name === 'CharacterMakerSword'); assert(swordIndex >= 0);
    const parent = gltf.nodes.find(node => node.children?.includes(swordIndex)); assert.equal(parent.name, 'R_Hand');
    assert.deepEqual(gltf.nodes[swordIndex].scale ?? [1, 1, 1], [1, 1, 1]);
    attachment = parent.name;
    if (document.characterId.startsWith('generic-human')) {
      assert(!names.has('OriginalNativeRightHand'), 'armed human GLB physically removes the open-hand split mesh');
      const closedIndex = gltf.nodes.findIndex(node => node.name === 'OriginalSwordRightHand'); assert(closedIndex >= 0);
      const closedParent = gltf.nodes.find(node => node.children?.includes(closedIndex)); assert.equal(closedParent.name, 'R_Hand');
      assert.deepEqual(gltf.nodes[closedIndex].scale ?? [1, 1, 1], [1, 1, 1]);
    }
    assert.deepEqual(exportInfo.weapon.playback.sword_attack, { loop: false, next: 'sword_idle' });
  }
  return { bytes: bytes.length, variant, clips, attachment, sha256: hash(bytes) };
}
try {
  await openStudio(page);
  for (const source of sources) {
    await selectOriginal(page, source.id); await category('動き');
    assert.equal(await motionGroup().getByRole('button').count(), 5);
    await motionGroup().getByRole('button', { name: '待機', exact: true }).click(); await expectConfig({ motion: 'idle', playing: true });
    const standardDocument = await draft();
    const standardExport = inspectGlb(await readFile(await download('GLBを書き出す', `${source.id}-standard.glb`)), standardDocument, 'standard');
    await motionGroup().getByRole('button', { name: '武器を構える', exact: true }).click(); await expectConfig({ motion: 'sword_idle', playing: true });
    await category('動き'); await page.getByRole('slider', { name: '再生速度', exact: true }).press('Home'); await expectConfig({ speed: .25 });
    await pause(); const guard = await painted(`${source.id}-guard`);
    await motionGroup().getByRole('button', { name: '武器を振る', exact: true }).click(); await expectConfig({ motion: 'sword_attack', playing: true });
    await page.waitForTimeout(650);
    const progress = page.getByRole('slider', { name: '再生位置', exact: true }); const beforeReplay = Number(await progress.inputValue()); assert(beforeReplay > .04);
    await motionGroup().getByRole('button', { name: '武器を振る', exact: true }).click();
    await page.waitForFunction(before => Number(document.querySelector('input[aria-label="再生位置"]').value) < before / 2, beforeReplay);
    await expectConfig({ motion: 'sword_idle' }, { timeout: 10000 });
    await motionGroup().getByRole('button', { name: '武器を振る', exact: true }).click(); await pause();
    const rangeBox = await progress.boundingBox();
    await progress.click({ position: { x: rangeBox.width / 2, y: rangeBox.height / 2 } });
    await expectConfig({ motion: 'sword_attack', playing: false });
    await page.waitForFunction(() => { const time = Number(document.querySelector('input[aria-label="再生位置"]').value); return time > .3 && time < .7; });
    const pausedTime = Number(await progress.inputValue());
    const attack = await painted(`${source.id}-attack-middle`);
    if (source.id.startsWith('generic-human')) {
      // 人間の髪は髪パーツ（modular）になったので、今と違う髪型を1つ選ぶ。
      await category('髪'); await inspector(page).getByRole('group', { name: '髪型', exact: true }).locator('button[aria-pressed="false"]').first().click(); await page.getByRole('button', { name: 'ローズ', exact: true }).click(); await ready();
      await category('動き'); const afterHair = Number(await progress.inputValue()); assert(Math.abs(afterHair - pausedTime) < .002, 'Paused attack position survives hair/style recoloring');
    }
    await category('体型'); await page.getByRole('button', { name: 'ふっくら', exact: true }).click();
    await category('動き'); assert(Math.abs(Number(await progress.inputValue()) - pausedTime) < .002, 'Paused attack position survives body editing');
    const name = `${source.name}・剣の確認`;
    await save(page, name);
    const saved = await draft(); assert.equal(saved.modelSha256, source.sha256); assert.equal(saved.config.motion, 'sword_attack'); assert(!('attackRevision' in saved.config));
    const json = await download('JSONを書き出す', `${source.id}.json`); assert.deepEqual(JSON.parse(await readFile(json, 'utf8')), saved);
    const exported = inspectGlb(await readFile(await download('GLBを書き出す', `${source.id}-armed.glb`)), saved, 'armed');
    await page.reload({ waitUntil: 'domcontentloaded', timeout: 120000 }); await ready(); assert.deepEqual(await draft(), saved); await painted(`${source.id}-reloaded`);
    await category('動き'); await motionGroup().getByRole('button', { name: '待機', exact: true }).click();
    await importJson(page, json);
    assert.equal(await page.getByRole('alertdialog').count(), 0, 'JSON switches without a confirmation'); await ready(); await expectConfig(saved.config); await painted(`${source.id}-reimported`);
    await category('動き'); await progress.press('End'); await expectConfig({ motion: 'sword_attack', playing: false });
    await page.getByRole('button', { name: '動作を再生', exact: true }).click(); await expectConfig({ motion: 'sword_idle' }, { timeout: 10000 });
    results.push({ id: source.id, guard, attack, beforeReplay, pausedTime, standardExport, exported, sameAttackRestarts: true, completedReturnsToGuard: true, pausedEditsPreserveTime: true, savedReloadedReimported: true });
    console.log(JSON.stringify(results.at(-1)));
  }
  await page.setViewportSize({ width: 390, height: 844 }); await category('動き'); assert.equal(await page.evaluate(() => document.documentElement.scrollWidth), 390);
  await motionGroup().getByRole('button', { name: '武器を振る', exact: true }).click(); await pause(); await painted('mobile-sword-390');
  for (const source of sources) assert.equal(hash(await readFile(source.glb)), source.sha256);
  assert.deepEqual(errors, []); await writeFile(join(output, 'result.json'), JSON.stringify({ success: true, results, errors }, null, 2));
} catch (error) { await page.screenshot({ path: join(output, 'failure.jpg'), type: 'jpeg', quality: 95, fullPage: true }).catch(() => {}); await writeFile(join(output, 'failure.json'), JSON.stringify({ error: String(error), results, errors }, null, 2)); throw error; }
finally { await context.close(); await browser.close(); }
