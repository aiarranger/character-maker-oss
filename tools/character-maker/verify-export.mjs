#!/usr/bin/env node
/** Independently load a downloaded maker GLB, including its embedded images.
 * Usage: node tools/character-maker/verify-export.mjs <download.glb>
 *        [--config <download.character.json>] [--out /tmp/check-directory]
 *        [--capture-set hair]
 * The source GLB is read directly; it is never rewritten or copied to the repo.
 */
import fs from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import { createHash } from 'node:crypto';
import sharp from 'sharp';
import { importPlaywright, NODE_MODULES } from '../lib/paths.mjs';

const file = process.argv[2] && path.resolve(process.argv[2]);
if (!file) throw new Error('Pass the exported GLB path.');
const options = new Map();
for (let i = 3; i < process.argv.length; i += 2) options.set(process.argv[i], process.argv[i + 1]);
const configFile = path.resolve(options.get('--config') ?? file.replace(/\.glb$/i, '.character.json'));
const expectedDocument = JSON.parse(await fs.readFile(configFile, 'utf8'));
const bytes = await fs.readFile(file);
if (bytes.readUInt32LE(0) !== 0x46546c67 || bytes.readUInt32LE(4) !== 2 || bytes.readUInt32LE(8) !== bytes.length) {
  throw new Error('The input is not a complete glTF 2 GLB.');
}
const sourceHash = createHash('sha256').update(bytes).digest('hex');
const output = options.has('--out') ? path.resolve(options.get('--out')) : await fs.mkdtemp(path.join(os.tmpdir(), 'character-maker-export-'));
await fs.mkdir(output, { recursive: true });
const { chromium } = await importPlaywright();
const browser = await chromium.launch({ channel: 'chrome', headless: true });
const page = await browser.newPage({ viewport: { width: 900, height: 1040 }, deviceScaleFactor: 1 });
const consoleMessages = [], pageErrors = [], requestFailures = [];
page.on('console', (message) => consoleMessages.push({ type: message.type(), text: message.text() }));
page.on('pageerror', (error) => pageErrors.push(String(error)));
page.on('requestfailed', (request) => requestFailures.push({ url: request.url(), error: request.failure()?.errorText }));
const importMap = { imports: { three: '/three/build/three.module.js', 'three/examples/jsm/': '/three/examples/jsm/' } };
const html = `<!doctype html><html><head><meta charset="utf-8"><link rel="icon" href="data:,"><script type="importmap">${JSON.stringify(importMap)}</script><style>body{margin:0;background:#edf2ee;font:16px system-ui;color:#30493e}#caption{height:48px;display:flex;align-items:center;justify-content:center;letter-spacing:.06em}canvas{display:block}</style></head><body><div id="caption">Exported character</div></body></html>`;
const threeRoot = path.resolve(NODE_MODULES, 'three');
await page.route('**/*', async (route) => {
  const url = new URL(route.request().url());
  if (url.origin !== 'https://maker-verify.test') return route.abort();
  if (url.pathname === '/') return route.fulfill({ contentType: 'text/html', body: html });
  if (url.pathname === '/model.glb') return route.fulfill({ contentType: 'model/gltf-binary', body: bytes });
  if (url.pathname.startsWith('/three/')) {
    const requested = path.resolve(threeRoot, decodeURIComponent(url.pathname.slice('/three/'.length)));
    if (!requested.startsWith(`${threeRoot}${path.sep}`)) return route.fulfill({ status: 404 });
    try { return route.fulfill({ contentType: 'text/javascript', body: await fs.readFile(requested) }); } catch { return route.fulfill({ status: 404 }); }
  }
  return route.fulfill({ status: 404 });
});

async function inspectInBrowser(expected) {
  const THREE = await import('three');
  const { GLTFLoader } = await import('three/examples/jsm/loaders/GLTFLoader.js');
  const gltf = await new GLTFLoader().loadAsync('/model.glb');
  const character = gltf.scene;
  const namedRoot = character.getObjectByName('CharacterMaker');
  const fail = (message) => { throw new Error(message); };
  if (!namedRoot) fail('Missing CharacterMaker root.');
  if (namedRoot.scale.toArray().some((value) => Math.abs(value - expected.config.height) > 1e-7)) fail('Exported height differs from the selected height.');
  const savedConfig = namedRoot.userData.characterMaker?.config;
  if (!savedConfig || Object.entries(expected.config).some(([key, value]) => savedConfig[key] !== value)) fail('Embedded configuration differs from downloaded JSON.');
  const names = gltf.animations.map((clip) => clip.name);
  if (names.length !== 7 || ['idle', 'walk', 'run', 'sword_idle', 'sword_attack', 'hit', 'face_hurt'].some((name) => !names.includes(name))) fail('Missing locomotion, sword, hit or face_hurt clips.');
  const fantasy = expected.config.style === 'fantasy';
  const headBoneName = fantasy ? 'J_Bip_C_Head' : 'Head';
  if (fantasy && expected.config.face === 'mature' && character.getObjectByName(headBoneName).scale.toArray().some(value => Math.abs(value - 0.91) > 1e-7)) fail('Exported mature head proportion was lost.');
  const handBoneName = fantasy ? 'J_Bip_R_Hand' : 'hand_r';
  const sword = character.getObjectByName('CharacterMakerSword');
  if (!sword || sword.parent?.name !== handBoneName) fail('Exported sword or hand attachment missing.');
  const expectedWeights = {
    bodySlim: Math.max(0, -expected.config.build), bodyBroad: Math.max(0, expected.config.build),
    faceRound: expected.config.face === 'round' ? 1 : 0, faceAngular: ['angular', 'mature'].includes(expected.config.face) ? 1 : 0,
    eyesRound: expected.config.eyes === 'round' ? 1 : 0, eyesSlender: expected.config.eyes === 'slender' ? 1 : 0,
    mouthSmile: expected.config.mouth === 'smile' ? 1 : 0, mouthFull: expected.config.mouth === 'full' ? 1 : 0,
  };
  const materials = [], meshes = [], skeletons = new Set();
  character.traverse((object) => {
    if (!object.isMesh) return;
    meshes.push(object);
    if (object.isSkinnedMesh) skeletons.add(object.skeleton);
    for (const [name, value] of Object.entries(expectedWeights)) {
      const index = object.morphTargetDictionary?.[name];
      if (index !== undefined && Math.abs(object.morphTargetInfluences[index] - value) > 1e-7) fail(`Wrong saved morph ${object.name}/${name}.`);
    }
    for (const material of Array.isArray(object.material) ? object.material : [object.material]) {
      const image = material.map?.source?.data;
      materials.push({ mesh: object.name, name: material.name, color: material.color?.getHexString(), texture: image ? {
        width: image.width ?? image.naturalWidth, height: image.height ?? image.naturalHeight,
        embeddedImageLoaded: (image.width ?? image.naturalWidth ?? 0) > 0 && (image.height ?? image.naturalHeight ?? 0) > 0,
      } : null });
      if (material.name === 'maker-hair' && material.color.getHexString().toLowerCase() !== expected.config.hairColor.slice(1).toLowerCase()) fail('Exported hair colour differs from the selection.');
    }
  });
  const head = character.getObjectByName('maker-head') ?? meshes.find(mesh => mesh.morphTargetDictionary?.faceRound !== undefined);
  if (!head || Object.keys(expectedWeights).some(name => !meshes.some(mesh => mesh.morphTargetDictionary?.[name] !== undefined))) fail('The editable face or body morphs were lost.');
  for (const role of [...(fantasy ? [] : ['maker-skin', 'maker-outfit']), ...(expected.config.hair === 'none' ? [] : ['maker-hair'])]) {
    const matches = materials.filter((material) => material.name === role);
    if (!matches.length || matches.some((material) => !material.texture?.embeddedImageLoaded)) fail(`Missing embedded texture: ${role}`);
  }
  const hair = character.getObjectByName('HairStyle');
  if (expected.config.hair !== 'none' && (!hair || hair.parent?.name !== headBoneName || (!fantasy && !hair.getObjectByName(`hair-${expected.config.hair}`)))) fail('The selected hair or its head attachment was lost.');
  if (expected.config.hair === 'none' && hair) fail('Hair is present although none was selected.');
  const hairFits = [];
  hair?.traverse((object) => {
    if (!object.isMesh || fantasy) return;
    for (const [target, base] of [['hairFitMasculine', 'masculine'], ['hairFitFeminine', 'feminine']]) {
      const index = object.morphTargetDictionary?.[target];
      if (index === undefined) fail(`Missing hair-fit target: ${target}`);
      const weight = object.morphTargetInfluences[index];
      if (Math.abs(weight - Number(expected.config.base === base)) > 1e-7) fail(`Wrong saved hair fit: ${target}`);
      hairFits.push({ mesh: object.name, target, weight });
    }
  });
  if (!skeletons.size || [...skeletons].some(skeleton => skeleton.bones.length < 52)) fail('The humanoid skeleton was lost.');
  if (!fantasy && [...skeletons].some(skeleton => skeleton.bones.length !== 65)) fail('Comic skeleton joint count changed.');

  const renderer = new THREE.WebGLRenderer({ antialias: true, preserveDrawingBuffer: true });
  renderer.setPixelRatio(1); renderer.setSize(900, 992);
  renderer.outputColorSpace = THREE.SRGBColorSpace; renderer.toneMapping = THREE.NoToneMapping;
  document.body.appendChild(renderer.domElement);
  const scene = new THREE.Scene(); scene.background = new THREE.Color('#edf2ee'); scene.add(character);
  const camera = new THREE.PerspectiveCamera(30, 900 / 992, 0.01, 30);
  character.updateMatrixWorld(true);
  const restBounds = new THREE.Box3().setFromObject(character);
  const height = restBounds.max.y - restBounds.min.y;
  const centre = restBounds.getCenter(new THREE.Vector3());
  const floor = new THREE.Mesh(new THREE.CircleGeometry(0.78 * expected.config.height, 80), new THREE.MeshBasicMaterial({ color: '#d3ded6', transparent: true, opacity: 0.6 }));
  floor.rotation.x = -Math.PI / 2; floor.position.y = -0.035 * expected.config.height; scene.add(floor);
  const mixer = new THREE.AnimationMixer(character);
  function setPose(clipName, fraction) {
    mixer.stopAllAction();
    if (clipName !== 'rest') {
      const clip = gltf.animations.find((item) => item.name === clipName);
      const action = mixer.clipAction(clip).reset().setLoop(THREE.LoopOnce, 1); action.clampWhenFinished = true; action.play();
      mixer.setTime(clip.duration * fraction);
    }
    character.updateMatrixWorld(true); skeletons.forEach((skeleton) => skeleton.update());
  }
  function poseMetrics() {
    let minimumY = Infinity;
    for (const mesh of meshes) {
      const vertex = new THREE.Vector3();
      for (let i = 0; i < mesh.geometry.attributes.position.count; i++) {
        mesh.getVertexPosition(i, vertex).applyMatrix4(mesh.matrixWorld); minimumY = Math.min(minimumY, vertex.y);
      }
    }
    if (!Number.isFinite(minimumY)) fail('Non-finite exported vertex position.');
    const bones = fantasy ? ['J_Bip_C_Head', 'J_Bip_L_Hand', 'J_Bip_R_Hand', 'J_Bip_L_Foot', 'J_Bip_R_Foot'] : ['Head', 'hand_l', 'hand_r', 'foot_l', 'foot_r'];
    return { floorY: minimumY, swordScale: sword.scale.toArray(), joints: Object.fromEntries(bones.map((name) => [name, character.getObjectByName(name).getWorldPosition(new THREE.Vector3()).toArray()])) };
  }
  const clips = [];
  for (const clip of gltf.animations) {
    const samples = [];
    for (const fraction of [0, 0.25, 0.5, 0.75]) {
      setPose(clip.name, fraction);
      const metrics = poseMetrics();
      // のけぞり（hit）は武器ありの書き出しで武器を出す。＞＜ だけ（face_hurt）は武器のトラックを持たない（待機のまま 0）。
      const expectedSwordScale = clip.name.startsWith('sword_') || clip.name === 'hit' ? 1 : 0;
      if (metrics.swordScale.some(value => Math.abs(value - expectedSwordScale) > 1e-6)) fail(`Wrong sword visibility in ${clip.name}.`);
      samples.push({ fraction, ...metrics });
    }
    const maxJointMovement = Math.max(...samples.slice(1).flatMap((sample) => Object.entries(sample.joints).map(([name, position]) =>
      new THREE.Vector3().fromArray(position).distanceTo(new THREE.Vector3().fromArray(samples[0].joints[name])))));
    if (clip.name === 'face_hurt') {
      // ＞＜ だけのクリップは体を動かさず、＞＜ の部品（maker-hurt-face）を scale 1 にする。
      setPose(clip.name, 0.5);
      if (maxJointMovement > 1e-6 || character.getObjectByName('maker-hurt-face')?.scale.x !== 1) fail('face_hurt must only show the hurt face.');
    } else if (maxJointMovement < 1e-5) fail(`The exported ${clip.name} clip does not move the model.`);
    clips.push({ name: clip.name, duration: clip.duration, tracks: clip.tracks.length, maxJointMovement, samples });
  }
  window.captureExport = ({ clip, fraction, view }) => {
    setPose(clip, fraction);
    const face = view.startsWith('face');
    const target = face ? character.getObjectByName(headBoneName).getWorldPosition(new THREE.Vector3()).add(new THREE.Vector3(0, height * 0.065, 0)) : centre.clone();
    const distance = height * (face ? 0.69 : 2.14);
    const angle = view === 'left' ? Math.PI / 2 : ['back', 'face-back', 'face-back-above'].includes(view) ? Math.PI : 0;
    const elevation = view === 'face-back-above' ? height * 0.32 : face ? 0 : height * 0.025;
    camera.position.set(target.x + Math.sin(angle) * distance, target.y + elevation, target.z + Math.cos(angle) * distance);
    camera.lookAt(target); camera.updateProjectionMatrix(); renderer.render(scene, camera);
    document.getElementById('caption').textContent = `${expected.name} / ${clip} / ${view} / ${Math.round(fraction * 100)}%`;
    return { drawCalls: renderer.info.render.calls, triangles: renderer.info.render.triangles, ...poseMetrics() };
  };
  return { configMatches: true, heightMatches: true, selectedHair: expected.config.hair, selectedHairColor: expected.config.hairColor,
    selectedMorphs: Object.fromEntries(Object.entries(head.morphTargetDictionary).map(([name, index]) => [name, head.morphTargetInfluences[index]])),
    skeletons: skeletons.size, bonesPerSkeleton: [...skeletons].map((skeleton) => skeleton.bones.length),
    textures: materials, hairFits, clips, restBounds: { min: restBounds.min.toArray(), max: restBounds.max.toArray() } };
}

const captures = options.get('--capture-set') === 'hair' ? [
  ['rest', 0, 'front'], ['idle', 0.25, 'face'], ['idle', 0.25, 'face-back'],
  ['idle', 0.25, 'face-back-above'], ['walk', 0.25, 'left'],
] : [
  ['rest', 0, 'front'], ['idle', 0.25, 'front'], ['idle', 0.25, 'face'],
  ['idle', 0.25, 'left'], ['idle', 0.25, 'back'], ['walk', 0.25, 'front'],
  ['walk', 0.25, 'left'], ['run', 0.25, 'front'], ['run', 0.25, 'left'],
  ['idle', 0.25, 'face-back'], ['run', 0.25, 'back'],
  ['sword_idle', 0.25, 'front'], ['sword_attack', 0.15, 'front'], ['sword_attack', 0.5, 'front'],
  ['sword_attack', 0.75, 'left'], ['sword_attack', 1, 'back'],
];
const report = { input: file, config: configFile, sourceBytes: bytes.length, sha256: sourceHash, output, images: [], consoleMessages, pageErrors, requestFailures };
try {
  await page.goto('https://maker-verify.test/');
  report.inspection = await page.evaluate(inspectInBrowser, expectedDocument);
  for (const [clip, fraction, view] of captures) {
    const capture = { clip, fraction, view };
    const metrics = await page.evaluate((value) => window.captureExport(value), capture);
    if (metrics.drawCalls === 0 || metrics.triangles === 0) throw new Error('The renderer produced no geometry.');
    const image = path.join(output, `${clip}-${view}-${Math.round(fraction * 100)}.jpg`);
    await page.screenshot({ path: image, type: 'jpeg', quality: 94 });
    report.images.push({ ...capture, path: image, metrics });
  }
  const cells = await Promise.all(report.images.map(async ({ path: filename }, index) => ({ input: await sharp(filename).resize(360, 416).jpeg({ quality: 91 }).toBuffer(), left: (index % 3) * 360, top: Math.floor(index / 3) * 416 })));
  report.sheet = path.join(output, 'sheet.jpg');
  await sharp({ create: { width: 1080, height: Math.ceil(report.images.length / 3) * 416, channels: 3, background: '#edf2ee' } }).composite(cells).jpeg({ quality: 92 }).toFile(report.sheet);
  const changedSource = createHash('sha256').update(await fs.readFile(file)).digest('hex') !== sourceHash;
  if (changedSource) throw new Error('The source GLB changed during verification.');
  if (pageErrors.length || requestFailures.length || consoleMessages.some((message) => message.type === 'error')) throw new Error('Browser load/render errors occurred; inspect report.json.');
  report.status = 'passed';
} catch (error) {
  report.status = 'failed'; report.error = String(error); process.exitCode = 1;
} finally {
  await fs.writeFile(path.join(output, 'report.json'), `${JSON.stringify(report, null, 2)}\n`);
  await browser.close();
}
console.log(JSON.stringify({ status: report.status, error: report.error, report: path.join(output, 'report.json'), sheet: report.sheet, images: report.images.map((image) => image.path), sha256: sourceHash, consoleErrors: consoleMessages.filter((message) => message.type === 'error'), pageErrors, requestFailures }, null, 2));
