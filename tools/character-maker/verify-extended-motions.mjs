#!/usr/bin/env node
/** Owned, unauthenticated browser check for additive motions and sword export.
 * node tools/character-maker/verify-extended-motions.mjs comic|fantasy feminine|masculine [output-directory]
 * Screenshots/report only; does not change or duplicate original GLBs.
 */
import fs from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import { fileURLToPath } from 'node:url';
import ts from 'typescript';
import sharp from 'sharp';
import { importPlaywright, NODE_MODULES } from '../lib/paths.mjs';

const repo = fileURLToPath(new URL('../../', import.meta.url));
const style = process.argv[2], base = process.argv[3];
if (!['comic', 'fantasy'].includes(style) || !['feminine', 'masculine'].includes(base)) throw new Error('Pass comic|fantasy and feminine|masculine.');
const out = process.argv[4] ? path.resolve(process.argv[4]) : await fs.mkdtemp(path.join(os.tmpdir(), `maker-sword-${style}-${base}-`));
await fs.mkdir(out, { recursive: true });
const assetDirectory = path.join(repo, 'parts/assets');
const model = await fs.readFile(path.join(assetDirectory, style === 'comic' ? 'v1' : 'fantasy-v1', `${base}.glb`));
const clips = style === 'comic'
  ? [...JSON.parse(await fs.readFile(path.join(assetDirectory, `v1/${base}-motions.json`), 'utf8')), ...JSON.parse(await fs.readFile(path.join(assetDirectory, `extended-v1/${base}-sword-motions.json`), 'utf8'))]
  : JSON.parse(await fs.readFile(path.join(assetDirectory, `fantasy-v1/${base}-motions.json`), 'utf8'));
const weapon = ts.transpileModule(await fs.readFile(path.join(repo, 'lib/character-maker/weapon.ts'), 'utf8'), { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ES2022 } }).outputText;
const { chromium } = await importPlaywright();
const browser = await chromium.launch({ channel: 'chrome', args: ['--enable-unsafe-swiftshader', '--use-angle=swiftshader'] });
const page = await browser.newPage({ viewport: { width: 760, height: 860 }, deviceScaleFactor: 1 });
const errors = [], warnings = [];
page.on('pageerror', e => errors.push(String(e)));
page.on('console', m => { if (m.type() === 'error') errors.push(m.text()); else if (m.type() === 'warning') warnings.push(m.text()); });
const threeRoot = path.resolve(NODE_MODULES, 'three');
await page.route('**/*', async route => {
  const u = new URL(route.request().url());
  if (u.origin !== 'https://maker-motion.test') return route.abort();
  if (u.pathname === '/') return route.fulfill({ contentType: 'text/html', body: '<!doctype html><meta charset="utf-8"><link rel="icon" href="data:,"><style>body{margin:0;background:#edf2ee;color:#30493e;font:16px system-ui}header{height:40px;text-align:center;line-height:40px}canvas{display:block}</style><script type="importmap">{"imports":{"three":"/three/build/three.module.js","three/examples/jsm/":"/three/examples/jsm/"}}</script><header id="caption"></header>' });
  if (u.pathname === '/model.glb') return route.fulfill({ contentType: 'model/gltf-binary', body: model });
  if (u.pathname === '/motions.json') return route.fulfill({ contentType: 'application/json', body: JSON.stringify(clips) });
  if (u.pathname === '/weapon.js') return route.fulfill({ contentType: 'text/javascript', body: weapon });
  if (u.pathname.startsWith('/three/')) {
    const file = path.resolve(threeRoot, u.pathname.slice(7));
    if (file.startsWith(`${threeRoot}${path.sep}`)) return route.fulfill({ contentType: 'text/javascript', body: await fs.readFile(file) });
  }
  return route.fulfill({ status: 404 });
});
const report = { style, base, out, errors, warnings, captures: [] };
try {
  await page.goto('https://maker-motion.test/');
  report.inspection = await page.evaluate(async ({ style, base }) => {
    const T = await import('three');
    const { GLTFLoader } = await import('three/examples/jsm/loaders/GLTFLoader.js');
    const { GLTFExporter } = await import('three/examples/jsm/exporters/GLTFExporter.js');
    const { createMakerSword, addWeaponTracks } = await import('/weapon.js');
    const body = await new GLTFLoader().loadAsync('/model.glb');
    const json = await (await fetch('/motions.json')).json(), original = json.map(value => T.AnimationClip.parse(value));
    const animations = addWeaponTracks(original), root = new T.Group(); root.name = 'CharacterMaker'; root.add(body.scene);
    if (JSON.stringify(original.map(value => T.AnimationClip.toJSON(value))) !== JSON.stringify(json)) throw new Error('Weapon track helper mutated the source clips.');
    const sword = createMakerSword(style, base), hand = body.scene.getObjectByName(sword.userData.attachmentBone);
    if (!hand) throw new Error('Attachment bone missing.'); hand.add(sword); root.updateMatrixWorld(true);
    // Export from the default hidden scale, without a viewer or native VRM plugin.
    const bytes = await new GLTFExporter().parseAsync(root, { binary: true, trs: true, onlyVisible: true, animations });
    const loaded = await new GLTFLoader().parseAsync(bytes, '');
    const character = loaded.scene, savedSword = character.getObjectByName('CharacterMakerSword');
    if (!savedSword || !savedSword.getObjectByName('MakerSwordBlade')) throw new Error('The default-hidden sword was dropped by export.');
    if (savedSword.parent.name !== hand.name || savedSword.scale.length() !== 0) throw new Error('Wrong exported attachment or rest visibility.');
    if (loaded.animations.length !== 5) throw new Error('Expected all five exported clips.');
    const mixer = new T.AnimationMixer(character), skins = [], meshes = [];
    character.traverse(o => { if (o.isSkinnedMesh) skins.push(o); if (o.isMesh) meshes.push(o); });
    function pose(clipName, fraction) {
      mixer.stopAllAction();
      const clip = loaded.animations.find(c => c.name === clipName), action = mixer.clipAction(clip).reset().setLoop(T.LoopOnce, 1);
      action.clampWhenFinished = true; action.play(); mixer.setTime(clip.duration * fraction); character.updateMatrixWorld(true);
      skins.forEach(m => m.skeleton.update());
      const shown = clipName.startsWith('sword_') ? 1 : 0;
      if (savedSword.scale.toArray().some(v => Math.abs(v - shown) > 1e-6)) throw new Error(`Wrong sword scale in ${clipName}`);
    }
    function metrics() {
      let floorY = Infinity;
      const visited = new Set();
      for (const mesh of skins) {
        const position = mesh.geometry.attributes.position;
        if (visited.has(position)) continue; visited.add(position);
        for (let i = 0; i < position.count; i++) {
          const v = mesh.getVertexPosition(i, new T.Vector3()).applyMatrix4(mesh.matrixWorld); floorY = Math.min(floorY, v.y);
        }
      }
      const grip = savedSword.getObjectByName('MakerSwordGrip');
      const tip = new T.Vector3(0, 0.755, 0).applyMatrix4(grip.matrixWorld);
      return { floorY, tip: tip.toArray(), hand: savedSword.parent.getWorldPosition(new T.Vector3()).toArray(), swordScale: savedSword.scale.toArray() };
    }
    const checks = [];
    for (const clip of loaded.animations) for (const fraction of [0, 0.25, 0.5, 0.75, 1]) { pose(clip.name, fraction); checks.push({ clip: clip.name, fraction, ...metrics() }); }
    const attacks = checks.filter(x => x.clip === 'sword_attack');
    const travel = Math.max(...attacks.map(x => new T.Vector3().fromArray(x.tip).distanceTo(new T.Vector3().fromArray(attacks[0].tip))));
    if (travel < 0.25) throw new Error('Sword attack does not move its visible tip sufficiently.');
    const scene = new T.Scene(); scene.background = new T.Color('#edf2ee'); scene.add(character);
    const renderer = new T.WebGLRenderer({ antialias: true, preserveDrawingBuffer: true }); renderer.setSize(760, 820);
    renderer.outputColorSpace = T.SRGBColorSpace; renderer.toneMapping = T.NoToneMapping; document.body.appendChild(renderer.domElement);
    const camera = new T.PerspectiveCamera(31, 760 / 820, 0.01, 30);
    const floor = new T.Mesh(new T.CircleGeometry(0.8, 64), new T.MeshBasicMaterial({ color: '#d3ded6' })); floor.rotation.x = -Math.PI / 2; floor.position.y = -0.035; scene.add(floor);
    window.captureMotion = ({ clip, fraction, view }) => {
      pose(clip, fraction);
      const close = view === 'hand', target = close ? savedSword.getObjectByName('MakerSwordGrip').getWorldPosition(new T.Vector3()) : new T.Vector3(0, style === 'fantasy' && base === 'masculine' ? 0.96 : 0.87, 0);
      const distance = close ? 0.44 : 4.7, angle = view === 'side' ? Math.PI / 2 : 0;
      camera.position.set(target.x + Math.sin(angle) * distance, target.y + (close ? 0.10 : 0.02), target.z + Math.cos(angle) * distance); camera.lookAt(target); renderer.render(scene, camera);
      document.getElementById('caption').textContent = `${style} ${base} / ${clip} / ${fraction} / ${view}`;
      return metrics();
    };
    return { exportedBytes: bytes.byteLength, sourceTracksUnchanged: true, clips: loaded.animations.map(c => ({ name: c.name, duration: c.duration, tracks: c.tracks.length })), bones: skins.map(m => m.skeleton.bones.length), swordAttachment: savedSword.parent.name, tipTravel: travel, checks };
  }, { style, base });
  for (const [clip, fraction, view] of [
    ['idle', 0, 'front'], ['sword_idle', 0, 'front'], ['sword_idle', 0, 'hand'],
    ...[0, 0.25, 0.5, 0.75, 1].map(f => ['sword_attack', f, 'front']),
    ...[0, 0.25, 0.5, 0.75, 1].map(f => ['sword_attack', f, 'side']),
    ['walk', 0.25, 'side'], ['run', 0.25, 'side'],
  ]) {
    const sample = { clip, fraction, view }, metrics = await page.evaluate(v => window.captureMotion(v), sample);
    const file = path.join(out, `${clip}-${fraction}-${view}.jpg`); await page.screenshot({ path: file, type: 'jpeg', quality: 94 });
    report.captures.push({ ...sample, file, metrics });
  }
  const cells = await Promise.all(report.captures.map(async ({ file }, i) => ({ input: await sharp(file).resize(304, 344).jpeg({ quality: 92 }).toBuffer(), left: i % 3 * 304, top: Math.floor(i / 3) * 344 })));
  report.sheet = path.join(out, 'sheet.jpg');
  await sharp({ create: { width: 912, height: Math.ceil(cells.length / 3) * 344, channels: 3, background: '#edf2ee' } }).composite(cells).jpeg({ quality: 94 }).toFile(report.sheet);
  if (errors.length) throw new Error('Browser errors occurred.');
  report.status = 'passed';
} catch (e) { report.status = 'failed'; report.error = String(e); process.exitCode = 1; }
finally { await fs.writeFile(path.join(out, 'report.json'), `${JSON.stringify(report, null, 2)}\n`); await browser.close(); }
console.log(JSON.stringify({ status: report.status, error: report.error, out, sheet: report.sheet, errors, warnings }, null, 2));
