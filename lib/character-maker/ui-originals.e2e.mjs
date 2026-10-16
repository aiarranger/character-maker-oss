// Actual original-character maker controls in a fresh unauthenticated Chrome.
// node lib/character-maker/ui-originals.e2e.mjs [output-directory] [--appearance]
// node lib/character-maker/ui-originals.e2e.mjs /tmp/output --export-only [--model=/tmp/probe.glb]
// オリジナルの編集画面は Character Studio（/character-maker）で開く（tools/lib/studio-e2e.mjs）。
// 元モデルと画面が読むモデル（人間は髪パーツ入りの modular、ロボ・ネコは appearance）は studio-catalog.json の /models/<sha>.glb。
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import { join, resolve, sep } from 'node:path';
import { ORIGINAL_FACE_CONTROLS, validateOriginalDocument } from './original-config.ts';
import { importPlaywright, NODE_MODULES, ROOT } from '../../tools/lib/paths.mjs';
import { catalogEntry, category as studioCategory, commonControls, current, displayedModel, exportFile, importJson, inspector, installReadTool, openStudio, ready as studioReady, save as studioSave, selectOriginal, waitConfig, waitDocument } from '../../tools/lib/studio-e2e.mjs';

const appearanceOnly = process.argv.includes('--appearance');
const exportOnly = process.argv.includes('--export-only');
const modelOverride = process.argv.find(value => value.startsWith('--model='))?.slice('--model='.length);
assert.ok(!modelOverride || exportOnly, 'Probe overrides are limited to the minimal export check');
const probeBytes = modelOverride ? await readFile(modelOverride) : null;
const probeSha = probeBytes ? createHash('sha256').update(probeBytes).digest('hex') : null;
const output = process.argv.slice(2).find(value => !value.startsWith('--')) ?? (appearanceOnly ? '/tmp/maker-originals-appearance-ui' : '/tmp/maker-originals-ui');
await mkdir(output, { recursive: true });
const ids = ['generic-human-a', 'generic-human-b', 'generic-robot', 'generic-cat'];
// characters の character.json と appearance の manifest の代わりに、画面と同じカタログ（studio-catalog.json）を使う。
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
const sources = await Promise.all(ids.map(async id => {
  const entry = catalogEntry(id);
  const path = join(ROOT, 'public', entry.modelUrl);
  const bytes = await readFile(path);
  assert.equal(sha256(bytes), entry.modelSha256, `${id}: source matches its catalog fingerprint`);
  const displayed = displayedModel(entry);
  assert.equal(sha256(await readFile(join(ROOT, 'public', displayed.url))), displayed.sha256, `${id}: the model the maker displays matches its catalog fingerprint`);
  const gltf = JSON.parse(bytes.subarray(20, 20 + bytes.readUInt32LE(12)).toString('utf8'));
  return { id, name: entry.name, sha: entry.modelSha256, path, viewer: entry.viewer, displayed, defaultHair: entry.modularModel?.hairstyles[0] ?? 'original',
    expressions: [...new Set(gltf.meshes.flatMap(mesh => mesh.extras?.targetNames ?? []))].filter(key => ORIGINAL_FACE_CONTROLS.some(([id]) => id === key)),
    clips: gltf.animations.map(clip => clip.name) };
}));
async function filesIn(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  return (await Promise.all(entries.map(entry => entry.isDirectory() ? filesIn(join(directory, entry.name)) : join(directory, entry.name)))).flat();
}
const protectedFiles = [...sources.flatMap(source => [source.path, join(ROOT, 'public', source.displayed.url)]), join(ROOT, 'lib/character-maker/studio-catalog.json'),
  ...await filesIn(join(ROOT, 'components/rig-actor/generic')), join(ROOT, 'lib/viewer-material/generic.mjs')];
async function fingerprints() {
  return Object.fromEntries(await Promise.all(protectedFiles.map(async file => [file, createHash('sha256').update(await readFile(file)).digest('hex')])));
}
const before = await fingerprints();
const haru = sources.find(source => source.id === 'generic-human-a');
const catalogDisplayedSha = haru.displayed.sha256;
if (probeSha) haru.displayed = { ...haru.displayed, sha256: probeSha };
const { chromium } = await importPlaywright();
const browser = await chromium.launch({ channel: 'chrome', headless: true });
const context = await browser.newContext({ viewport: { width: 1440, height: 960 } });
await installReadTool(context);
const page = await context.newPage();
page.setDefaultTimeout(30000);
const errors = [], steps = [], screenshots = [], exports = [], sizeChecks = [];
const assetResponses = [];
page.on('pageerror', error => errors.push(error.message));
if (probeBytes) {
  // Fixture the descriptor and its bytes together in this isolated context.
  // The production exporter still performs its real SHA check; canonical source
  // identity and all source files remain unchanged.
  // カタログは画面の JS に入っているので、その中のハルのモデルの SHA を試作モデルの SHA に差し替える。
  await page.route(url => url.pathname.startsWith('/_next/static/') && url.pathname.endsWith('.js'), async route => {
    const response = await route.fetch();
    const body = await response.text();
    await route.fulfill({ response, body: body.includes(catalogDisplayedSha) ? body.replaceAll(catalogDisplayedSha, probeSha) : body });
  });
}
await page.route('**/models/**', async route => {
  const path = new URL(route.request().url()).pathname;
  const source = sources.find(item => item.displayed.url === path || `/models/${item.sha}.glb` === path);
  if (probeBytes && source === haru && path === haru.displayed.url) {
    assetResponses.push({ id: source.id, path, status: 200, sha: probeSha, fixture: modelOverride });
    return route.fulfill({ status: 200, contentType: 'model/gltf-binary', body: probeBytes });
  }
  const response = await route.fetch(), bytes = await response.body();
  assetResponses.push({ id: source?.id, path, status: response.status(), sha: sha256(bytes) });
  await route.fulfill({ response, body: bytes });
});
page.on('console', message => {
  if (message.type() === 'error' && message.text().includes('Encountered two children with the same key')) errors.push(message.text());
});
const ready = () => studioReady(page);
// 保存前の編集中の内容は、画面の読み取りツール（read_character_edit）で読む。
const expectDraft = expected => waitDocument(page, expected);
const currentDocument = () => current(page);
const expectConfig = partial => waitConfig(page, partial);
async function observation(minPixels = 1000) {
  // 390px 幅では Studio の 3D プレビューが操作部の下にあり、画面外のあいだは描かれないので、見える位置まで送る。
  await page.locator('[aria-label="キャラクターの3Dプレビュー"] canvas').scrollIntoViewIfNeeded();
  const handle = await page.waitForFunction(minPixels => {
    const canvas = document.querySelector('[aria-label="キャラクターの3Dプレビュー"] canvas'); if (!canvas) return false;
    const copy = document.createElement('canvas'); copy.width = canvas.width; copy.height = canvas.height;
    const context = copy.getContext('2d'); context.drawImage(canvas, 0, 0);
    const pixels = context.getImageData(0, 0, copy.width, copy.height).data;
    let opaquePixels = 0, x0 = copy.width, y0 = copy.height, x1 = -1, y1 = -1;
    for (let y = 0; y < copy.height; y++) for (let x = 0; x < copy.width; x++) {
      if (pixels[(y * copy.width + x) * 4 + 3] <= 250) continue;
      opaquePixels++; x0 = Math.min(x0, x); x1 = Math.max(x1, x); y0 = Math.min(y0, y); y1 = Math.max(y1, y);
    }
    return opaquePixels > minPixels && { opaquePixels, width: copy.width, height: copy.height, bounds: { x0, y0, x1, y1, width: x1 - x0 + 1, height: y1 - y0 + 1 } };
  }, minPixels, { timeout: 15000 });
  const sample = await handle.jsonValue(); await handle.dispose(); return sample;
}
async function capture(name) {
  const sample = await observation();
  await page.evaluate(async () => {
    const visible = [...document.querySelectorAll('img')].filter(image => {
      const r = image.getBoundingClientRect(); return r.bottom > 0 && r.top < innerHeight && r.width > 0;
    });
    await Promise.all(visible.map(image => image.decode().catch(() => {})));
    await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  });
  await page.screenshot({ path: join(output, `${name}.jpg`), type: 'jpeg', quality: 93 });
  screenshots.push({ name, ...sample }); return sample;
}
const download = (label, filename) => exportFile(page, { 'GLBを書き出す': '3Dモデル（GLB）', 'JSONを書き出す': '編集設定（JSON）' }[label], join(output, filename));

/** A separate GLTFLoader reopens the downloaded file, without importing maker code. */
async function inspectExport(file, expected, renderName = null) {
  const bytes = await readFile(file);
  // 体型の変化は、画面が組み立てに使ったモデル（人間は髪パーツ入りの modular）と比べる。元モデルとはメッシュの分け方が違う。
  const sourceBytes = expected.config.appearance?.build ? await readFile(join(ROOT, 'public', sources.find(source => source.id === expected.characterId).displayed.url)) : null;
  assert.equal(bytes.readUInt32LE(0), 0x46546c67); assert.equal(bytes.readUInt32LE(4), 2); assert.equal(bytes.readUInt32LE(8), bytes.length);
  const gltf = JSON.parse(bytes.subarray(20, 20 + bytes.readUInt32LE(12)).toString('utf8'));
  // のけぞり（hit）と ＞＜（face_hurt）は、待機・歩く・走るの後ろに足す（docs/main-character-spec.md）
  assert.deepEqual(gltf.animations.map(clip => clip.name.split(':').at(-1)), ['idle', 'walk', 'run', 'hit', 'face_hurt'].filter(name => gltf.animations.some(clip => clip.name.split(':').at(-1) === name)));
  assert.deepEqual(gltf.animations.map(clip => clip.name.split(':').at(-1)).sort(), ['face_hurt', 'hit', 'idle', 'run', 'walk']);
  const hurtNode = gltf.nodes.findIndex(node => node.name === 'maker-hurt-face');
  assert.ok(hurtNode >= 0, 'The ＞＜ node is exported'); assert.deepEqual(gltf.nodes[hurtNode].scale, [0, 0, 0], 'The ＞＜ node is hidden by scale 0');
  assert.equal(gltf.nodes.find(node => node.children?.includes(hurtNode))?.name, 'Head', 'The ＞＜ node follows the head bone');
  for (const name of ['hit', 'face_hurt']) assert.ok(gltf.animations.find(clip => clip.name === name).channels.some(channel => channel.target.node === hurtNode && channel.target.path === 'scale'), `${name} switches the ＞＜ node by scale`);
  assert.ok(!gltf.animations.find(clip => clip.name === 'face_hurt').channels.some(channel => ['rotation', 'translation'].includes(channel.target.path)), 'face_hurt does not move bones');
  assert.ok(gltf.materials.length > 0 && gltf.materials.every(material => material.extensions?.KHR_materials_unlit), 'All exported materials preserve the unlit presentation');
  const inspect = await context.newPage();
  const threeRoot = resolve(NODE_MODULES, 'three');
  await inspect.route('https://original-verify.test/**', async route => {
    const url = new URL(route.request().url());
    if (url.pathname === '/') return route.fulfill({ contentType: 'text/html', body: '<!doctype html><link rel="icon" href="data:,"><script type="importmap">{"imports":{"three":"/three/build/three.module.js","three/examples/jsm/":"/three/examples/jsm/"}}</script>' });
    if (url.pathname === '/model.glb') return route.fulfill({ contentType: 'model/gltf-binary', body: bytes });
    if (url.pathname === '/source.glb' && sourceBytes) return route.fulfill({ contentType: 'model/gltf-binary', body: sourceBytes });
    const requested = resolve(threeRoot, url.pathname.slice('/three/'.length));
    if (!url.pathname.startsWith('/three/') || !requested.startsWith(threeRoot + sep)) return route.fulfill({ status: 404 });
    return route.fulfill({ contentType: 'text/javascript', body: await readFile(requested) });
  });
  try {
    await inspect.goto('https://original-verify.test/');
    const result = await inspect.evaluate(async ({ expected, render }) => {
      const THREE = await import('three');
      const { GLTFLoader } = await import('three/examples/jsm/loaders/GLTFLoader.js');
      const gltf = await new GLTFLoader().loadAsync('/model.glb');
      const all = []; gltf.scene.traverse(object => all.push(object));
      const meshes = all.filter(object => object.isMesh), bones = all.filter(object => object.isBone);
      if (!meshes.length || !bones.length) throw new Error('Export lost meshes or skeleton');
      for (const mesh of meshes) for (const material of Array.isArray(mesh.material) ? mesh.material : [mesh.material]) {
        if (!material.isMeshBasicMaterial) throw new Error('Export is no longer unlit');
        const image = material.map?.image;
        if (image && !(image.width > 0 && image.height > 0)) throw new Error('Embedded image did not decode');
      }
      const root = all.find(object => object.name === 'CharacterMakerOriginal' && object.userData.characterMaker?.sourceKind === 'original');
      if (!root) throw new Error('Missing original maker wrapper metadata');
      if (root.scale.toArray().some(value => Math.abs(value - expected.config.size) > 1e-6)) throw new Error('The selected size wrapper was lost');
      const saved = root.userData.characterMaker;
      if (saved.characterId !== expected.characterId || saved.modelSha256 !== expected.modelSha256) throw new Error('Export lost original source identity');
      if (JSON.stringify(saved.config) !== JSON.stringify(expected.config)) throw new Error('Export lost selected edit settings');
      const hairParts = all.filter(object => object.userData.originalMakerPart).map(object => object.userData.originalMakerPart);
      // 人間の髪パーツ（modular）は hairstyleId で区別し、選ばなかった髪は書き出しから外れる（original-hair.ts）。
      const modularHair = [...new Set(all.filter(object => object.userData.originalModularPart === 'hair').map(object => object.userData.hairstyleId))];
      const hairColors = [...new Set(meshes.flatMap(mesh => (Array.isArray(mesh.material) ? mesh.material : [mesh.material])
        .filter(material => material.userData.originalMakerHair || (modularHair.length && material.userData.originalMakerHair !== false && material.name.startsWith('maker-original-hair-')))
        .map(material => `#${material.color.getHexString()}`)))];
      const appearance = expected.config.appearance;
      if (modularHair.length) {
        if (JSON.stringify(modularHair) !== JSON.stringify([appearance.hairstyle])) throw new Error(`The exported hair parts ${modularHair} are not exactly the selected ${appearance.hairstyle}`);
        if (appearance.hairColor !== 'source' && (!hairColors.length || hairColors.some(color => color !== appearance.hairColor))) throw new Error('The selected hair color was lost');
      } else if (appearance && appearance.hairstyle !== 'original') {
        if (!hairParts.includes(appearance.hairstyle)) throw new Error('The selected replacement hair was lost');
        if (hairParts.includes('source-hair') || hairParts.includes(appearance.hairstyle === 'short' ? 'ponytail' : 'short')) throw new Error('An unselected hair style became visible in the exported GLB');
        if (appearance.hairColor !== 'source' && (!hairColors.length || hairColors.some(color => color !== appearance.hairColor))) throw new Error('The selected hair color was lost');
      }
      const bodyChanges = [];
      if (appearance?.build) {
        const { MeshoptDecoder } = await import('three/examples/jsm/libs/meshopt_decoder.module.js');
        const original = await new GLTFLoader().setMeshoptDecoder(MeshoptDecoder).loadAsync('/source.glb');
        original.scene.traverse(mesh => {
          if (!mesh.isSkinnedMesh) return;
          const position = mesh.geometry.getAttribute('position');
          const exported = meshes.find(candidate => candidate.name === mesh.name && !candidate.userData.originalMakerPart && candidate.geometry.getAttribute('position')?.count === position.count);
          if (!exported) return;
          const changed = exported.geometry.getAttribute('position');
          let moved = 0, maxDelta = 0;
          for (let index = 0; index < position.count; index++) {
            const delta = Math.hypot(changed.getX(index) - position.getX(index), changed.getY(index) - position.getY(index), changed.getZ(index) - position.getZ(index));
            maxDelta = Math.max(maxDelta, delta); if (delta > 1e-5) moved++;
          }
          bodyChanges.push({ mesh: mesh.name, vertices: position.count, moved, maxDelta });
        });
        if (!bodyChanges.some(change => change.moved > 100 && change.maxDelta > 1e-4)) throw new Error('The body setting did not change the independently parsed exported geometry');
      }
      const faceWeights = {};
      for (const mesh of meshes) for (const [key, value] of Object.entries(expected.config.face)) {
        const index = mesh.morphTargetDictionary?.[key]; if (index === undefined) continue;
        const actual = mesh.morphTargetInfluences[index];
        if (Math.abs(actual - value) > 1e-6) throw new Error(`Export lost selected face baseline ${key}`);
        faceWeights[key] = actual;
      }
      const mixer = new THREE.AnimationMixer(gltf.scene), clips = [];
      // ＞＜: 休止では scale 0、hit の途中と face_hurt では 1
      const hurtFace = gltf.scene.getObjectByName('maker-hurt-face');
      if (!hurtFace || hurtFace.scale.x !== 0) throw new Error('The ＞＜ node is missing or visible at rest');
      const hurtScale = {};
      for (const [name, at] of [['hit', .4], ['face_hurt', .5]]) {
        const clip = gltf.animations.find(item => item.name === name);
        mixer.stopAllAction(); mixer.clipAction(clip).reset().play(); mixer.setTime(clip.duration * at);
        hurtScale[name] = hurtFace.scale.x;
        if (hurtFace.scale.x !== 1) throw new Error(`${name} does not show the ＞＜ node`);
      }
      mixer.stopAllAction();
      for (const clip of gltf.animations) {
        if (clip.name === 'face_hurt') continue;
        mixer.stopAllAction(); const action = mixer.clipAction(clip).reset().play(); mixer.setTime(0); gltf.scene.updateMatrixWorld(true);
        const first = bones.map(bone => bone.matrixWorld.toArray());
        mixer.setTime(Math.min(clip.duration * 0.37, clip.duration - 0.001)); gltf.scene.updateMatrixWorld(true);
        const delta = Math.max(...bones.flatMap((bone, i) => bone.matrixWorld.elements.map((value, j) => Math.abs(value - first[i][j]))));
        if (!(delta > 1e-6)) throw new Error(`Exported clip does not move: ${clip.name}`);
        clips.push({ name: clip.name, duration: clip.duration, delta }); action.stop();
      }
      if (render) {
        mixer.stopAllAction();
        const walk = gltf.animations.find(clip => clip.name.endsWith(':walk'));
        mixer.clipAction(walk).reset().play(); mixer.setTime(walk.duration * .37); gltf.scene.updateMatrixWorld(true);
        const bounds = new THREE.Box3().setFromObject(gltf.scene, true), span = bounds.getSize(new THREE.Vector3()), center = bounds.getCenter(new THREE.Vector3());
        const scene = new THREE.Scene(); scene.background = new THREE.Color('#eaf0e9'); scene.add(gltf.scene);
        const camera = new THREE.OrthographicCamera(-1, 1, 1, -1, .01, 100);
        camera.position.copy(center).add(new THREE.Vector3(0, 0, 8)); camera.lookAt(center);
        camera.zoom = 1.75 / Math.max(span.x, span.y); camera.updateProjectionMatrix();
        const renderer = new THREE.WebGLRenderer({ antialias: true, preserveDrawingBuffer: true });
        renderer.setSize(720, 720); renderer.outputColorSpace = THREE.SRGBColorSpace;
        document.body.style.margin = '0'; document.body.appendChild(renderer.domElement);
        renderer.render(scene, camera);
      }
      return { meshes: meshes.length, bones: bones.length, size: root.scale.toArray(), faceWeights, hairParts, modularHair, hairColors, bodyChanges, clips, hurtScale };
    }, { expected, render: !!renderName });
    if (renderName) {
      await inspect.setViewportSize({ width: 720, height: 720 });
      await inspect.locator('canvas').screenshot({ path: join(output, `${renderName}.jpg`), type: 'jpeg', quality: 93 });
    }
    return { characterId: expected.characterId, bytes: bytes.length, ...result };
  } finally { await inspect.close(); }
}

const category = label => studioCategory(page, label);
const frame = label => page.getByRole('group', { name: '表示範囲', exact: true }).getByRole('button', { name: label, exact: true }).click();
// 大きさ・体型のスライダーは上の「共通のカスタマイズ」と右の「体型」の両方にある。動作は右の「動き」タブで選ぶ。
const size = () => commonControls(page).getByRole('slider', { name: '大きさ', exact: true });
const motionGroup = () => inspector(page).getByRole('group', { name: '動作', exact: true });
const hairGroup = () => inspector(page).getByRole('group', { name: '髪型', exact: true });
async function pause() {
  const button = page.getByRole('button', { name: '動作を一時停止', exact: true });
  if (await button.count()) await button.click();
  await expectConfig({ playing: false });
}
const save = (name, asNew = false) => studioSave(page, name, asNew);
// 画面左の一覧が4体のオリジナルと（8+4体の）プリセットを並べる。
const originalButtons = () => page.getByRole('navigation', { name: 'キャラクター一覧', exact: true }).getByRole('button')
  .filter({ hasText: new RegExp(`^(${sources.map(source => source.name).join('|')})$`) });

/** 舞台の絵（preserveDrawingBuffer の canvas）の指紋。＞＜ が描かれたかを、前後の絵の違いで確かめる */
const stageImage = () => page.evaluate(() => document.querySelector('[data-stage-canvas] canvas')?.toDataURL('image/png') ?? '');
/** のけぞる（一回再生して元の動作へ戻る・連打で最初から）と ＞＜ の切り替え。どちらも保存データを変えない */
async function hitAndHurtFace(source) {
  const before = await currentDocument();
  const hit = commonControls(page).getByRole('button', { name: 'のけぞる', exact: true });
  await hit.click();
  await page.waitForSelector('[data-hit-playing]', { timeout: 5000 });
  await page.waitForSelector('[data-hit-playing]', { state: 'detached', timeout: 10000 });
  await hit.click(); await page.waitForTimeout(80); await hit.click();
  await page.waitForSelector('[data-hit-playing]', { state: 'detached', timeout: 10000 });
  assert.deepEqual(await currentDocument(), before, `${source.name}: the hit is playback only and leaves the document unchanged`);
  await category('表情'); await frame('顔'); await pause();
  await page.waitForTimeout(1200);
  const toggle = page.getByRole('button', { name: '＞＜の顔', exact: true });
  assert.equal(await toggle.getAttribute('aria-pressed'), 'false');
  const plain = await stageImage();
  await toggle.click(); assert.equal(await toggle.getAttribute('aria-pressed'), 'true');
  await page.waitForTimeout(400);
  const hurt = await stageImage();
  assert.notEqual(hurt, plain, `${source.name}: ＞＜ changes the drawn face`);
  await page.evaluate(() => window.scrollTo(0, 0)); await capture(`${source.id}-hurt-face`);
  await toggle.click(); assert.equal(await toggle.getAttribute('aria-pressed'), 'false');
  const { playing, ...rest } = (await currentDocument()).config, { playing: _, ...expected } = before.config;
  assert.deepEqual(rest, expected, `${source.name}: ＞＜ is view state only`);
  await page.getByRole('button', { name: '動作を再生', exact: true }).click(); await expectConfig({ playing: true });
  await frame('全身');
}

async function appearanceJourney() {
  const source = haru;
  await selectOriginal(page, source.id);
  await ready(); await pause(); await frame('全身');
  await category('体型');
  const original = await currentDocument();
  // 人間の髪は髪パーツ（modular）になり、既定は髪パーツの先頭（ハルは haru-short-v2）。
  assert.deepEqual(original.config.appearance, { version: 'original-appearance-v2', parts: { head: 'original', ears: 'original', eyes: 'original', mouth: 'original', coatColor: 'source', accentColor: 'source', eyeColor: 'source' }, build: 0, hairstyle: source.defaultHair, hairColor: 'source' });
  await page.evaluate(() => window.scrollTo(0, 0)); await capture('haru-original-body');
  await inspector(page).getByRole('slider', { name: '体型', exact: true }).press('End');
  await expectConfig({ appearance: { ...original.config.appearance, build: 1 } });
  await page.evaluate(() => window.scrollTo(0, 0)); await capture('haru-build-1');
  await category('髪');
  // 旧「ショート」の代わりに、今の髪パーツから「刈り上げショート」（haru-crop-v2）を選ぶ。
  await hairGroup().getByRole('button', { name: '刈り上げショート', exact: true }).click();
  await ready(); await expectConfig({ appearance: { ...original.config.appearance, build: 1, hairstyle: 'haru-crop-v2' } });
  await page.getByRole('button', { name: 'ローズ', exact: true }).click();
  const appearance = { ...original.config.appearance, build: 1, hairstyle: 'haru-crop-v2', hairColor: '#b97991' };
  await expectConfig({ appearance }); await ready();
  await frame('顔'); await page.evaluate(() => window.scrollTo(0, 0)); await capture('haru-short-rose-face');
  await frame('全身');
  await commonControls(page).getByRole('button', { name: '歩く', exact: true }).click();
  await expectConfig({ motion: 'walk', playing: true });
  await save('ハル・体型とショート');
  const document = await currentDocument();
  assert.equal(document.characterId, source.id); assert.equal(document.modelSha256, source.sha);
  const jsonFile = await download('JSONを書き出す', 'haru-appearance.character.json');
  assert.deepEqual(validateOriginalDocument(JSON.parse(await readFile(jsonFile, 'utf8'))), document);
  await page.reload({ waitUntil: 'domcontentloaded', timeout: 120000 }); await ready();
  await expectDraft(document); await observation();
  await category('髪'); await page.evaluate(() => window.scrollTo(0, 0)); await capture('haru-appearance-reloaded');
  exports.push(await inspectExport(await download('GLBを書き出す', 'haru-appearance.glb'), document, 'haru-appearance-glb-walk'));
  await page.setViewportSize({ width: 390, height: 844 });
  await category('髪'); await page.evaluate(() => window.scrollTo(0, 0));
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth), 390);
  await expectDraft(document); await capture('haru-appearance-mobile-390');
  steps.push('Haru: build +1, crop hair part, rose color, walking, named save/JSON export/reload, source identity, independent GLB render with three preserved clips, and 390px mobile controls without horizontal overflow');
}

async function exportProbeJourney() {
  await selectOriginal(page, haru.id);
  await expectDraft({ characterId: 'generic-human-a', modelSha256: haru.sha });
  await pause();
  // 武器を持たない待機にして、待機・歩く・走るの3動作の GLB を書き出す。
  await commonControls(page).getByRole('button', { name: '待機', exact: true }).click(); await pause();
  await category('表情');
  await page.getByRole('button', { name: 'ふだんの表情に戻す', exact: true }).click();
  await category('髪');
  await hairGroup().getByRole('button', { name: '刈り上げショート', exact: true }).click();
  await page.getByRole('button', { name: 'ローズ', exact: true }).click();
  await expectConfig({ appearance: { ...(await currentDocument()).config.appearance, hairstyle: 'haru-crop-v2', hairColor: '#b97991' } });
  await ready();
  const document = validateOriginalDocument(await currentDocument());
  assert.equal(document.config.appearance.hairstyle, 'haru-crop-v2');
  assert.equal(document.config.appearance.hairColor, '#b97991');
  await page.evaluate(() => window.scrollTo(0, 0));
  await capture('haru-export-probe-face');
  exports.push(await inspectExport(await download('GLBを書き出す', 'haru-export-probe.glb'), document, 'haru-export-probe-independent'));
  steps.push('Haru: selected crop/rose probe exports with the real SHA check; independent GLTFLoader verifies source identity, static face, selected hair, unlit textures, skeleton and three moving clips');
}

try {
  await openStudio(page);
  assert.equal(await originalButtons().count(), 4);

  if (exportOnly) await exportProbeJourney();
  else if (appearanceOnly) await appearanceJourney();
  else {
  for (const source of sources) {
    await selectOriginal(page, source.id);
    await expectDraft({ characterId: source.id, modelSha256: source.sha });
    await pause();
    await frame('全身');
    const measurements = [];
    for (const [key, expected] of [['Home', 0.65], ['End', 1.35]]) {
      await size().press(key); await expectConfig({ size: expected });
      await page.evaluate(() => window.scrollTo(0, 0));
      measurements.push(await capture(`${source.id}-size-${Math.round(expected * 100)}`));
    }
    const ratio = measurements[1].bounds.height / measurements[0].bounds.height;
    assert.ok(ratio > 1.9 && ratio < 2.2, `${source.name}: size must visibly change at fixed full-body framing (${ratio})`);
    sizeChecks.push({ id: source.id, small: measurements[0].bounds.height, large: measurements[1].bounds.height, ratio });
    // 大きさだけを戻すボタンは無くなり、体型の項目の「体型と大きさを元に戻す」になった。
    await category('体型'); await page.getByRole('button', { name: '体型と大きさを元に戻す', exact: true }).click(); await expectConfig({ size: 1 });
    await frame('顔'); await page.evaluate(() => window.scrollTo(0, 0)); await capture(`${source.id}-face`);

    await category('表情');
    await page.getByRole('button', { name: 'にっこり', exact: true }).click();
    await expectConfig({ autoBlink: false, speaking: false });
    await page.getByText('目と口を細かく整える', { exact: true }).click();
    for (const [key, label] of ORIGINAL_FACE_CONTROLS.filter(([key]) => source.expressions.includes(key))) {
      const slider = page.getByRole('slider', { name: label, exact: true });
      await slider.press('End');
      await page.waitForFunction(key => window.__characterTools.read_character_edit?.execute({})?.config.face[key] === 1, key);
      await slider.press('Home');
    }
    await page.getByRole('button', { name: 'ふだんの表情に戻す', exact: true }).click();
    await expectConfig({ face: { blinkLeft: 0, blinkRight: 0, mouthClose: source.viewer.neutralMouthClose ?? 1, aa: 0, ee: 0, ou: 0, happy: 0 } });
    await page.getByRole('button', { name: 'にっこり', exact: true }).click();
    await page.evaluate(() => window.scrollTo(0, 0)); await capture(`${source.id}-smile`);

    await category('動き');
    for (const [motion, label] of [['idle', '待機'], ['walk', '歩く'], ['run', '走る']]) {
      await motionGroup().getByRole('button', { name: label, exact: true }).click();
      await expectConfig({ motion, playing: true }); await observation();
    }
    const speed = page.getByRole('slider', { name: '再生速度', exact: true });
    await speed.press('End'); await expectConfig({ speed: Number(await speed.getAttribute('max')) });
    await pause();
    const progress = page.getByRole('slider', { name: '再生位置', exact: true });
    await page.waitForTimeout(250);
    const stopped = Number(await progress.inputValue()); await page.waitForTimeout(250);
    assert.ok(Math.abs(Number(await progress.inputValue()) - stopped) < 0.002, 'Paused playback stays at the same position');
    await page.getByRole('button', { name: '動作を再生', exact: true }).click(); await expectConfig({ playing: true });
    await hitAndHurtFace(source);
    await category('動き');
    await page.getByRole('checkbox', { name: '自動でまばたき', exact: true }).check();
    await page.getByRole('checkbox', { name: '会話の口の動き', exact: true }).check();
    await expectConfig({ autoBlink: true, speaking: true });
    const head = page.getByRole('slider', { name: '顔の向き', exact: true });
    if (await head.count()) { await head.press('End'); await expectConfig({ headTurn: source.viewer.headTurnDegrees }); }
    await size().press('Home'); await expectConfig({ size: 0.65 });
    const name = `${source.name}-表情保存`, copyName = `${source.name}-別名`;
    await save(name); await save(copyName, true);
    // 保存データはレール下端の「保存データ」に、そのキャラの分だけ並ぶ。
    const drawer = page.locator('details').filter({ has: page.locator('summary', { hasText: '保存データ' }) });
    if (!await drawer.evaluate(element => element.open)) await drawer.locator('summary').click();
    const library = page.getByRole('region', { name: `${source.name}の保存データ`, exact: true });
    assert.equal(await library.getByRole('button', { name: new RegExp(`^${name}`) }).count(), 1);
    assert.equal(await library.getByRole('button', { name: new RegExp(`^${copyName}`) }).count(), 1);
    await drawer.locator('summary').click();
    const jsonFile = await download('JSONを書き出す', `${source.id}.character.json`);
    const document = validateOriginalDocument(JSON.parse(await readFile(jsonFile, 'utf8')));
    assert.equal(document.characterId, source.id); assert.equal(document.modelSha256, source.sha);
    assert.equal(document.config.size, 0.65); assert.equal(document.config.autoBlink, true); assert.equal(document.config.speaking, true);
    exports.push(await inspectExport(await download('GLBを書き出す', `${source.id}.glb`), document));
    await size().press('End'); await expectConfig({ size: 1.35 });
    await importJson(page, jsonFile);
    assert.equal(await page.getByRole('alertdialog').count(), 0, 'JSON switches without a confirmation');
    await ready(); await expectDraft(document); await observation();
    await page.reload({ waitUntil: 'domcontentloaded', timeout: 120000 }); await ready();
    assert.equal(await page.locator('[data-style]').getAttribute('data-style'), 'original');
    await expectDraft(document); await observation();
    await save(copyName); // Leave a clean selection before choosing the next source.
    steps.push(`${source.name}: full/face, visible size, available face controls, all three motions, pause/speed, hit once/restart and ＞＜ toggle (not saved), save-as, JSON reimport/reload and independent GLB parse with hit/face_hurt`);
  }

  // 旧画面の「人間スタイル ⇄ オリジナル」の行き来は、Studio では一覧でプリセットとオリジナルを選び替える操作になった。
  const originalDraft = await currentDocument();
  const last = sources.at(-1);
  await page.getByRole('button', { name: 'セレネを選ぶ', exact: true }).click(); await ready();
  await page.getByRole('textbox', { name: 'キャラクターの名前', exact: true }).fill('人間スタイルの控え');
  await page.getByRole('button', { name: '保存する', exact: true }).click();
  await page.getByRole('status').filter({ hasText: '「人間スタイルの控え」を保存しました。' }).waitFor();
  await selectOriginal(page, last.id); await expectDraft(originalDraft);
  await page.getByRole('button', { name: 'セレネを選ぶ', exact: true }).click(); await ready();
  assert.equal(await page.getByRole('textbox', { name: 'キャラクターの名前', exact: true }).inputValue(), '人間スタイルの控え');
  await selectOriginal(page, last.id); await expectDraft(originalDraft);
  await page.reload({ waitUntil: 'domcontentloaded', timeout: 120000 }); await ready(); await expectDraft(originalDraft);
  steps.push('Original and preset drafts remain separate and survive selection round trips and reload');

  await page.setViewportSize({ width: 390, height: 844 });
  for (const source of sources) {
    await selectOriginal(page, source.id);
    // 390px の舞台は小さく、保存した大きさ 65% の姿は 1000px に満たないので、描かれていることだけを確かめる。
    await ready(); await expectDraft({ characterId: source.id }); await observation(200);
  }
  await category('表情'); await page.getByRole('button', { name: 'にっこり', exact: true }).click();
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth), 390);
  await capture('mobile-original-expression');
  steps.push('All four original characters and expression controls remain selectable at 390px without horizontal overflow');
  }

  const loadedAssets = await Promise.all(assetResponses);
  for (const source of appearanceOnly || exportOnly ? sources.filter(source => source.id === 'generic-human-a') : sources) {
    assert.ok(loadedAssets.some(asset => asset.path === source.displayed.url && asset.status === 200 && asset.sha === source.displayed.sha256), `${source.name}: browser loaded the exact catalog/derived GLB with verified original provenance`);
  }
  assert.deepEqual(await fingerprints(), before, 'Canonical model/catalog and frozen generic viewer files remain unchanged');
  assert.deepEqual(errors, []);
  await writeFile(join(output, 'result.json'), JSON.stringify({ success: true, steps, sizeChecks, screenshots, exports, loadedAssets, protectedFiles: before, errors }, null, 2));
  console.log(JSON.stringify({ success: true, output, steps }));
} catch (error) {
  await page.screenshot({ path: join(output, 'failure.jpg'), type: 'jpeg', quality: 93, fullPage: true }).catch(() => {});
  await writeFile(join(output, 'failure.json'), JSON.stringify({ error: String(error), steps, sizeChecks, screenshots, exports, errors }, null, 2));
  console.error(error); process.exitCode = 1;
} finally { await context.close(); await browser.close(); }
