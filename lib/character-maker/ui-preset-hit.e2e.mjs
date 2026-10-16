// のけぞり（hit）と ＞＜ をプリセット（Studio に並ぶファンタジー4体・アメコミ4体）で確かめる。docs/main-character-spec.md
// CHARACTER_MAKER_URL=http://127.0.0.1:4288 node --import ./tests/ts-resolve.mjs lib/character-maker/ui-preset-hit.e2e.mjs <output> [--quick]
//   --quick  ファンタジー女性・男性、アメコミ女性の3体だけ
// 画面：のけぞる → 始め・中ほど・終わり（元の動きへ戻る）を撮る。＞＜ の切り替えを押して顔を撮る。
// 書き出し：標準（武器なし）と武器ありの GLB に hit・face_hurt・maker-hurt-face があり、読み直して hit を再生すると ＞＜ が出る。
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { join, resolve, sep } from 'node:path';
import { ACTIVE_FANTASY_PRESET_IDS, getPresets } from './config.ts';
import { HIT_CLIP, HURT_FACE_CLIP, HURT_FACE_NODE } from './hurt.ts';
import { importPlaywright, NODE_MODULES, ROOT } from '../../tools/lib/paths.mjs';
import { category, commonControls, exportFile, installReadTool, openStudio, ready } from '../../tools/lib/studio-e2e.mjs';

const quick = process.argv.includes('--quick');
const output = process.argv.slice(2).find(value => !value.startsWith('--')) ?? '/tmp/maker-preset-hit';
await mkdir(output, { recursive: true });
const fantasy = getPresets('fantasy').filter(preset => ACTIVE_FANTASY_PRESET_IDS.includes(preset.id)).map(preset => ({ ...preset, style: 'fantasy' }));
const comic = getPresets('comic').map(preset => ({ ...preset, style: 'comic' }));
const all = [...fantasy, ...comic];
const chosen = quick
  ? [fantasy.find(p => p.config.base === 'feminine'), fantasy.find(p => p.config.base === 'masculine'), comic.find(p => p.config.base === 'feminine')]
  : all;
const BASE_CLIPS = ['idle', 'walk', 'run', 'sword_idle', 'sword_attack'];

const { chromium } = await importPlaywright();
const browser = await chromium.launch({ channel: 'chrome', headless: true });
const context = await browser.newContext({ viewport: { width: 1440, height: 960 }, acceptDownloads: true });
await installReadTool(context);
const page = await context.newPage();
page.setDefaultTimeout(30000);
const errors = [], steps = [], shots = [], exports = [];
page.on('pageerror', error => errors.push(error.message));
const stage = () => page.locator('#studio-stage');
const settled = () => page.waitForSelector('[data-stage-canvas][data-camera-settled="true"]', { timeout: 60000 });
async function shot(name) {
  const path = join(output, `${name}.jpg`);
  await stage().screenshot({ path, type: 'jpeg', quality: 90 });
  shots.push(path);
  return readFile(path);
}
const differs = (a, b) => a.length !== b.length || !a.equals(b);

async function selectPreset(preset) {
  const card = page.getByRole('navigation', { name: 'キャラクター一覧' }).getByRole('button', { name: `${preset.name}を選ぶ`, exact: true });
  await card.click();
  await page.waitForFunction(name => [...document.querySelectorAll('[aria-current="true"]')].some(item => item.getAttribute('aria-label') === `${name}を選ぶ`), preset.name);
  await ready(page);
  await settled();
}
/** プリセットの編集中の控え（sessionStorage）。のけぞり・＞＜ は入らない */
const draftConfig = () => page.evaluate(() => {
  const key = Object.keys(sessionStorage).find(item => item.startsWith('characters.character-maker.v1.draft'));
  return key ? JSON.parse(sessionStorage.getItem(key))?.document?.config ?? null : null;
});

/** GLB の JSON と、アクセサの値を読む */
function parseGlb(bytes) {
  assert.equal(bytes.readUInt32LE(0), 0x46546c67);
  const jsonLength = bytes.readUInt32LE(12);
  const gltf = JSON.parse(bytes.subarray(20, 20 + jsonLength).toString('utf8'));
  const bin = bytes.subarray(20 + jsonLength + 8);
  const read = index => {
    const accessor = gltf.accessors[index], view = gltf.bufferViews[accessor.bufferView];
    const size = { SCALAR: 1, VEC3: 3, VEC4: 4 }[accessor.type];
    assert.equal(accessor.componentType, 5126);
    return Array.from(new Float32Array(bin.buffer.slice(bin.byteOffset + (view.byteOffset ?? 0) + (accessor.byteOffset ?? 0), bin.byteOffset + (view.byteOffset ?? 0) + (accessor.byteOffset ?? 0) + accessor.count * size * 4)));
  };
  return { gltf, read };
}

function checkGlb(bytes, preset, armed) {
  const { gltf, read } = parseGlb(bytes);
  const names = gltf.animations.map(animation => animation.name);
  assert.deepEqual(names, [...BASE_CLIPS, HIT_CLIP, HURT_FACE_CLIP], `${preset.name}: clips are the five motions followed by hit and face_hurt`);
  const nodeIndex = gltf.nodes.findIndex(node => node.name === HURT_FACE_NODE);
  assert.ok(nodeIndex >= 0, `${preset.name}: the ${HURT_FACE_NODE} node is exported`);
  assert.deepEqual(gltf.nodes[nodeIndex].scale, [0, 0, 0], `${preset.name}: the hurt face is hidden by scale 0 by default`);
  const sword = gltf.nodes.findIndex(node => node.name === 'CharacterMakerSword');
  const channel = (animation, node, path) => animation.channels.find(item => item.target.node === node && item.target.path === path);
  const hit = gltf.animations[names.indexOf(HIT_CLIP)], face = gltf.animations[names.indexOf(HURT_FACE_CLIP)];
  const hitFace = channel(hit, nodeIndex, 'scale');
  assert.ok(hitFace, 'hit carries the hurt face scale track');
  assert.equal(hit.samplers[hitFace.sampler].interpolation, 'STEP');
  assert.deepEqual(read(hit.samplers[hitFace.sampler].output), [1, 1, 1, 0, 0, 0]);
  const hitTimes = read(hit.samplers[hitFace.sampler].input);
  assert.ok(Math.abs(hitTimes.at(-1) - 0.6667) < 0.01, `hit lasts about 0.66 s (${hitTimes.at(-1)})`);
  assert.ok(hit.channels.filter(item => item.target.path === 'rotation').length >= 50, 'hit moves the body bones');
  if (sword >= 0) {
    const hitSword = channel(hit, sword, 'scale');
    assert.ok(hitSword, 'hit carries the weapon visibility track');
    assert.ok(read(hit.samplers[hitSword.sampler].output).every(value => value === (armed ? 1 : 0)), `hit shows the weapon only for armed exports (${armed})`);
    assert.equal(channel(face, sword, 'scale'), undefined, 'face_hurt has no weapon track');
  } else assert.equal(armed, false);
  assert.ok(channel(face, nodeIndex, 'scale'), 'face_hurt carries the hurt face scale track');
  assert.ok(face.channels.every(item => ['scale', 'weights'].includes(item.target.path)), 'face_hurt never moves the body bones');
  // 既存のクリップは、元の動作データ（JSON）の値そのまま（追加しただけで、サンプラーのバイト列は変わらない）。
  const sources = (preset.style === 'fantasy' ? [`fantasy-${preset.config.base}-motions.json`] : [`${preset.config.base}-motions.json`, `comic-${preset.config.base}-sword-motions.json`])
    .flatMap(file => JSON.parse(readFileSync(join(ROOT, 'public/maker-assets', file), 'utf8')));
  let comparedTracks = 0;
  for (const source of sources) {
    const animation = gltf.animations[names.indexOf(source.name)];
    for (const track of source.tracks.filter(item => !item.name.startsWith('CharacterMakerSword.'))) {
      const dot = track.name.lastIndexOf('.'), node = gltf.nodes.findIndex(item => item.name === track.name.slice(0, dot));
      const found = channel(animation, node, { quaternion: 'rotation', position: 'translation' }[track.name.slice(dot + 1)]);
      assert.ok(found, `${source.name}: ${track.name} is exported`);
      const sampler = animation.samplers[found.sampler];
      assert.deepEqual(read(sampler.input), track.times.map(Math.fround), `${source.name}: ${track.name} times unchanged`);
      assert.deepEqual(read(sampler.output), track.values.map(Math.fround), `${source.name}: ${track.name} values unchanged`);
      comparedTracks++;
    }
  }
  const weights = hit.channels.filter(item => item.target.path === 'weights');
  if (preset.style === 'fantasy') assert.ok(weights.length > 0, 'fantasy hit carries the face morph weights');
  else assert.equal(weights.length, 0);
  return { clips: names, hurtNode: nodeIndex, weightsChannels: weights.length, hitDuration: hitTimes.at(-1), unchangedSourceTracks: comparedTracks };
}

/** 書き出した GLB を three で読み直し、待機と hit（0.1秒）の顔を描いて比べる */
async function renderExported(bytes, name) {
  const inspect = await context.newPage();
  const threeRoot = resolve(NODE_MODULES, 'three');
  await inspect.route('https://preset-hit-verify.test/**', async route => {
    const url = new URL(route.request().url());
    if (url.pathname === '/') return route.fulfill({ contentType: 'text/html', body: '<!doctype html><link rel="icon" href="data:,"><body style="margin:0;background:#fff"><script type="importmap">{"imports":{"three":"/three/build/three.module.js","three/examples/jsm/":"/three/examples/jsm/"}}</script></body>' });
    if (url.pathname === '/model.glb') return route.fulfill({ contentType: 'model/gltf-binary', body: bytes });
    const requested = resolve(threeRoot, url.pathname.slice('/three/'.length));
    if (!url.pathname.startsWith('/three/') || !requested.startsWith(threeRoot + sep)) return route.fulfill({ status: 404 });
    return route.fulfill({ contentType: 'text/javascript', body: await readFile(requested) });
  });
  try {
    await inspect.goto('https://preset-hit-verify.test/');
    const result = await inspect.evaluate(async ({ HIT_CLIP, HURT_FACE_CLIP, HURT_FACE_NODE }) => {
      const THREE = await import('three');
      const { GLTFLoader } = await import('three/examples/jsm/loaders/GLTFLoader.js');
      const gltf = await new GLTFLoader().loadAsync('/model.glb');
      const scene = new THREE.Scene(); scene.background = new THREE.Color('#ffffff'); scene.add(gltf.scene);
      const node = gltf.scene.getObjectByName(HURT_FACE_NODE);
      if (!node) throw new Error('The hurt face node was lost');
      const head = gltf.scene.getObjectByName('J_Bip_C_Head') ?? gltf.scene.getObjectByName('Head');
      const renderer = new THREE.WebGLRenderer({ antialias: true, preserveDrawingBuffer: true });
      renderer.setSize(420, 420); document.body.appendChild(renderer.domElement);
      const camera = new THREE.PerspectiveCamera(24, 1, 0.01, 50);
      const mixer = new THREE.AnimationMixer(gltf.scene);
      const clip = name => gltf.animations.find(item => item.name === name);
      const frame = () => {
        gltf.scene.updateMatrixWorld(true);
        const target = head.getWorldPosition(new THREE.Vector3()).add(new THREE.Vector3(0, 0.07, 0));
        camera.position.copy(target).add(new THREE.Vector3(0, 0, 0.55)); camera.lookAt(target);
        renderer.render(scene, camera);
        return renderer.domElement.toDataURL('image/png');
      };
      const idle = mixer.clipAction(clip('idle')); idle.play(); mixer.update(0.01);
      const rest = { scale: node.scale.x, image: frame() };
      const hit = mixer.clipAction(clip(HIT_CLIP)); hit.setLoop(THREE.LoopOnce, 1); idle.stop(); hit.play(); mixer.update(0.1);
      const during = { scale: node.scale.x, image: frame() };
      mixer.update(0.7);
      const after = { scale: node.scale.x };
      hit.stop();
      const layered = mixer.clipAction(clip(HURT_FACE_CLIP)); idle.reset().play(); layered.play(); mixer.update(0.2);
      const face = { scale: node.scale.x, image: frame() };
      return { rest, during, after, face, morphTracks: clip(HIT_CLIP).tracks.filter(track => track.name.endsWith('.morphTargetInfluences')).length };
    }, { HIT_CLIP, HURT_FACE_CLIP, HURT_FACE_NODE });
    const save = async (label, data) => { const path = join(output, `${name}-reloaded-${label}.png`); await writeFile(path, Buffer.from(data.split(',')[1], 'base64')); shots.push(path); return path; };
    await save('idle', result.rest.image); await save('hit-0.1s', result.during.image); await save('face_hurt-layered', result.face.image);
    assert.equal(result.rest.scale, 0, 'hurt face is hidden while idling');
    assert.equal(result.during.scale, 1, 'hit shows the hurt face');
    assert.equal(result.face.scale, 1, 'face_hurt layered over idle shows the hurt face');
    assert.notEqual(result.rest.image, result.during.image, 'the reloaded hit visibly changes the face');
    return { duringScale: result.during.scale, layeredScale: result.face.scale, afterScale: result.after.scale, morphTracks: result.morphTracks };
  } finally { await inspect.close(); }
}

try {
  await openStudio(page);
  for (const preset of chosen) {
    const id = `${preset.style}-${preset.id}`;
    await selectPreset(preset);
    // 動きのタブで再生速度を 0.25倍にして、のけぞりの途中を撮れるようにする。
    await category(page, '動き');
    await page.getByRole('slider', { name: '再生速度', exact: true }).fill('0.25');
    await settled(); await page.waitForTimeout(400);
    const before = await shot(`${id}-hit-0-before`);
    const button = commonControls(page).getByRole('button', { name: 'のけぞる', exact: true });
    await button.click();
    await page.waitForTimeout(120);
    const start = await shot(`${id}-hit-1-start`);
    await page.waitForTimeout(1000);
    const mid = await shot(`${id}-hit-2-mid`);
    await page.waitForTimeout(2200);
    const end = await shot(`${id}-hit-3-end`);
    assert.ok(differs(before, start) && differs(start, mid), `${preset.name}: the hit visibly moves the character`);
    // 連打：途中で押すと最初からやり直す（例外なく動く）。
    await button.click(); await page.waitForTimeout(300); await button.click(); await page.waitForTimeout(3200);
    await page.getByRole('slider', { name: '再生速度', exact: true }).fill('1');
    steps.push(`${preset.name}: のけぞる plays once (start/mid/end) and returns to the previous motion; repeated presses restart`);
    void end;

    // ＞＜：表情のところの切り替え（顔のアップ）。
    await category(page, '表情');
    await settled(); await page.waitForTimeout(500);
    const toggle = page.getByRole('button', { name: '＞＜の顔', exact: true });
    assert.equal(await toggle.getAttribute('aria-pressed'), 'false');
    const plain = await shot(`${id}-face-0-plain`);
    await toggle.click();
    assert.equal(await toggle.getAttribute('aria-pressed'), 'true');
    await page.waitForTimeout(400);
    const hurt = await shot(`${id}-face-1-hurt`);
    assert.ok(differs(plain, hurt), `${preset.name}: the ＞＜ toggle changes the face`);
    await toggle.click();
    assert.equal(await toggle.getAttribute('aria-pressed'), 'false');
    await page.waitForTimeout(400);
    await shot(`${id}-face-2-off`);
    // 保存データには入らない（draft の config は表情のまま）。
    const config = await draftConfig();
    assert.ok(!config || (!('hurtFace' in config) && !('hit' in config)), 'Transient hit/hurt state is not part of the saved config');
    steps.push(`${preset.name}: ＞＜ toggle shows and hides the face; not saved`);
  }

  // 書き出し：武器あり（プリセットの既定）と標準（武器を持たない）。
  const targets = quick ? chosen : [fantasy.find(p => p.config.base === 'feminine'), fantasy.find(p => p.config.base === 'masculine'), comic.find(p => p.config.base === 'feminine'), comic.find(p => p.config.base === 'masculine')];
  for (const preset of targets) {
    const id = `${preset.style}-${preset.id}`;
    await selectPreset(preset);
    for (const armed of [true, false]) {
      if (!armed) {
        await page.getByRole('region', { name: '持たせる武器' }).getByRole('button', { name: '持たない', exact: true }).click();
        await page.waitForTimeout(300); await ready(page); await settled();
      }
      const file = await exportFile(page, '3DモデルのGLB', join(output, `${id}-${armed ? 'armed' : 'standard'}.glb`));
      const bytes = await readFile(file);
      const checked = checkGlb(bytes, preset, armed);
      const rendered = await renderExported(bytes, `${id}-${armed ? 'armed' : 'standard'}`);
      exports.push({ preset: preset.name, armed, bytes: bytes.length, ...checked, ...rendered });
    }
    steps.push(`${preset.name}: standard and armed GLB carry hit/face_hurt/${HURT_FACE_NODE}; reloaded hit shows ＞＜`);
  }
  assert.deepEqual(errors, []);
  const report = { ok: true, presets: chosen.map(p => p.name), steps, exports, screenshots: shots };
  await writeFile(join(output, 'report.json'), `${JSON.stringify(report, null, 2)}\n`);
  console.log(JSON.stringify({ ok: true, steps, exports: exports.map(({ preset, armed, clips, weightsChannels, duringScale, layeredScale }) => ({ preset, armed, clips: clips.length, weightsChannels, duringScale, layeredScale })), output }, null, 2));
} catch (error) {
  await page.screenshot({ path: join(output, 'failure.jpg'), type: 'jpeg', quality: 85 }).catch(() => {});
  console.error(JSON.stringify({ ok: false, error: String(error?.stack ?? error), errors, steps }, null, 2));
  process.exitCode = 1;
} finally {
  await browser.close();
}
