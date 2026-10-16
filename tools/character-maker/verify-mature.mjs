#!/usr/bin/env node
/** Render the production assembler's reversible mature face and reload its export.
 * node tools/character-maker/verify-mature.mjs [/tmp/output-directory]
 * Uses an owned native Chrome; originals and canonical assets are never written.
 */
import fs from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import { fileURLToPath } from 'node:url';
import ts from 'typescript';
import sharp from 'sharp';
import { importPlaywright, NODE_MODULES, MAKER_URL } from '../lib/paths.mjs';
import { fulfillMakerModule, routePublicJson } from '../lib/maker-modules.mjs';

const repo = fileURLToPath(new URL('../../', import.meta.url));
const output = process.argv[2] ? path.resolve(process.argv[2]) : await fs.mkdtemp(path.join(os.tmpdir(), 'maker-mature-'));
await fs.mkdir(output, { recursive: true });
const { chromium } = await importPlaywright();
const browser = await chromium.launch({ channel: 'chrome', headless: true });
const page = await browser.newPage({ viewport: { width: 640, height: 800 }, deviceScaleFactor: 1 });
const errors = [], report = { output, bases: {}, images: [], errors };
page.on('pageerror', error => errors.push(String(error)));
const imports = { imports: { three: '/maker-mature/three/build/three.module.js', 'three/examples/jsm/': '/maker-mature/three/examples/jsm/' } };
await page.route('**/maker-mature/**', async route => {
  const rel = decodeURIComponent(new URL(route.request().url()).pathname.slice('/maker-mature/'.length));
  if (!rel) return route.fulfill({ contentType: 'text/html', body: `<html><head><link rel="icon" href="data:,"><script type="importmap">${JSON.stringify(imports)}</script><style>html,body{margin:0;overflow:hidden}canvas{display:block}</style></head><body></body></html>` });
  return fulfillMakerModule(route, rel);
});
await routePublicJson(page);

try {
  await page.goto(`${MAKER_URL}/maker-mature/`);
  await page.evaluate(async () => {
    const T = await import('three');
    const { GLTFLoader } = await import('three/examples/jsm/loaders/GLTFLoader.js');
    const { assembleCharacter, exportCharacterGlb } = await import('/maker-mature/assembly');
    const { DEFAULT_CONFIG } = await import('/maker-mature/config');
    const renderer = new T.WebGLRenderer({ antialias: true, preserveDrawingBuffer: true });
    renderer.setSize(640, 800); renderer.outputColorSpace = T.SRGBColorSpace; renderer.toneMapping = T.NoToneMapping;
    document.body.append(renderer.domElement);
    const scene = new T.Scene(); scene.background = new T.Color('#edf2ee');
    const camera = new T.PerspectiveCamera(28, .8, .01, 50);
    const near = (actual, expected, label) => { if (actual.some((v, i) => Math.abs(v - expected[i]) > 1e-7)) throw new Error(label); };
    // 大人びた顔の頭の比率（assembly.ts と同じ。2026-10 に一律0.91から体のベースごとの値へ変わった）
    const MATURE_HEAD = { masculine: [1.035, 1, 1.025], feminine: [0.94, 1.01, 0.98] };
    const headFor = (originalScale, base, face) => face === 'mature' ? originalScale.map((v, i) => v * MATURE_HEAD[base][i]) : originalScale;
    let state;
    window.prepareMature = async base => {
      if (state) { state.mixer.stopAllAction(); state.mixer.uncacheRoot(state.instance.root); state.instance.dispose(); }
      const config = { ...DEFAULT_CONFIG, base, height: 1.07, build: -.15, hair: base === 'feminine' ? 'animeParted' : 'layered', hairColor: '#514036', face: 'angular', eyes: 'slender', mouth: 'natural' };
      const instance = await assembleCharacter(config), head = instance.root.getObjectByName('J_Bip_C_Head');
      scene.add(instance.root);
      const originalScale = head.scale.toArray(), bounds = new T.Box3().setFromObject(instance.root);
      const faceMaps = [];
      instance.root.traverse(o => { if (!o.isMesh) return; for (const material of Array.isArray(o.material) ? o.material : [o.material]) if (material.name.endsWith('Face_00_SKIN')) faceMaps.push({ material, original: material.map, source: material.map.source }); });
      state = { instance, head, originalScale, config, faceMaps, mixer: new T.AnimationMixer(instance.root), height: bounds.max.y - bounds.min.y, centre: bounds.getCenter(new T.Vector3()) };
      return { originalScale, clipNames: instance.animations.map(c => c.name), headScaleTracks: instance.animations.flatMap(c => c.tracks.filter(t => t.name === 'J_Bip_C_Head.scale').map(() => c.name)) };
    };
    window.captureMature = ({ face, clip = 'idle', view = 'front', full = false }) => {
      const { instance, head, originalScale, config, mixer, height, centre } = state;
      mixer.stopAllAction(); instance.apply({ ...config, face });
      const motion = instance.animations.find(c => c.name === clip);
      const action = mixer.clipAction(motion).reset().setLoop(T.LoopOnce, 1); action.play(); mixer.setTime(motion.duration * .25);
      instance.root.updateMatrixWorld(true);
      near(head.scale.toArray(), headFor(originalScale, config.base, face), 'Head scale changed during playback');
      const target = full ? centre : head.getWorldPosition(new T.Vector3()).add(new T.Vector3(0, height * .035, 0));
      const distance = height * (full ? 2.4 : .57), angle = view === 'side' ? Math.PI / 2 : view === 'back' ? Math.PI : 0;
      camera.position.set(target.x + Math.sin(angle) * distance, target.y, target.z + Math.cos(angle) * distance); camera.lookAt(target);
      renderer.render(scene, camera);
      if (!renderer.info.render.triangles) throw new Error('Empty render');
      return { headScale: head.scale.toArray(), triangles: renderer.info.render.triangles, data: renderer.domElement.toDataURL('image/jpeg', .93) };
    };
    window.verifyMature = async () => {
      const { instance, head, originalScale, config, faceMaps, mixer } = state;
      mixer.stopAllAction();
      for (const face of ['mature', 'oval', 'round', 'angular', 'mature']) {
        instance.apply({ ...config, face }); near(head.scale.toArray(), headFor(originalScale, config.base, face), `Restore failed: ${face}`);
        for (const { material, original, source } of faceMaps) {
          if (original.source !== source) throw new Error('Original texture Source was replaced');
          if (face !== 'mature' && material.map !== original) throw new Error(`Original map was not restored: ${face}`);
          if (face === 'mature') {
            if (material.map === original || material.map.source === source || material.map.image === original.image) throw new Error('Mature map shares the original image or Source');
            for (const key of ['colorSpace','flipY','wrapS','wrapT','minFilter','magFilter','anisotropy','channel','generateMipmaps','premultiplyAlpha','unpackAlignment']) if (material.map[key] !== original[key]) throw new Error(`Mature sampler differs: ${key}`);
          }
        }
      }
      const fresh = await assembleCharacter(config);
      near(fresh.root.getObjectByName('J_Bip_C_Head').scale.toArray(), originalScale, 'Cached template was changed'); fresh.dispose();
      for (const eyes of ['almond', 'round', 'slender']) for (const mouth of ['natural', 'smile', 'full']) {
        instance.apply({ ...config, face: 'mature', eyes, mouth });
        instance.root.traverse(o => { if (!o.morphTargetDictionary) return; for (const [name, value] of Object.entries({ faceAngular: 0, faceRound: 0, eyesRound: Number(eyes === 'round'), eyesSlender: Number(eyes === 'slender'), mouthSmile: Number(mouth === 'smile'), mouthFull: Number(mouth === 'full') })) {
          const i = o.morphTargetDictionary[name]; if (i !== undefined && o.morphTargetInfluences[i] !== value) throw new Error(`Independent morph changed: ${name}`);
        } });
      }
      const bytes = await exportCharacterGlb({ ...config, face: 'mature' });
      const gltf = await new GLTFLoader().parseAsync(bytes, '');
      const exportedHead = gltf.scene.getObjectByName('J_Bip_C_Head');
      near(exportedHead.scale.toArray(), headFor(originalScale, config.base, 'mature'), 'Export lost mature scale');
      const clipNames = gltf.animations.map(c => c.name);
      // のけぞり（hit）と ＞＜（face_hurt）を足して7本（docs/main-character-spec.md）
      if (clipNames.length !== 7 || ['idle','walk','run','sword_idle','sword_attack','hit','face_hurt'].some(n => !clipNames.includes(n))) throw new Error('Export lost motion');
      const exportedMixer = new T.AnimationMixer(gltf.scene);
      for (const clip of gltf.animations) { exportedMixer.stopAllAction(); exportedMixer.clipAction(clip).play(); exportedMixer.setTime(clip.duration * .25); near(exportedHead.scale.toArray(), headFor(originalScale, config.base, 'mature'), `Export motion lost scale: ${clip.name}`); }
      exportedMixer.stopAllAction(); exportedMixer.uncacheRoot(gltf.scene);
      return { reversible: true, cachedTemplateUnchanged: true, mapRestoreAndSeparateSource: true, originalSamplerPreserved: true, eyesMouthIndependent: true, exportedBytes: bytes.byteLength, exportedHeadScale: exportedHead.scale.toArray(), exportedClipNames: clipNames };
    };
  });
  for (const base of ['feminine', 'masculine']) {
    report.bases[base] = await page.evaluate(base => window.prepareMature(base), base);
    for (const face of ['angular', 'mature']) for (const full of [false, true]) {
      const result = await page.evaluate(value => window.captureMature(value), { face, full });
      const file = path.join(output, `${base}-${face}-${full ? 'full' : 'face'}.jpg`);
      await fs.writeFile(file, Buffer.from(result.data.split(',')[1], 'base64')); report.images.push(file);
    }
    for (const crop of ['face', 'full']) {
      const cells = await Promise.all(['angular','mature'].map(async (face, i) => ({ input: await fs.readFile(path.join(output, `${base}-${face}-${crop}.jpg`)), left: i * 640, top: 32 })));
      cells.push({ input: Buffer.from('<svg width="1280" height="32"><rect width="1280" height="32" fill="#172125"/><text x="20" y="23" font-family="Arial" font-size="20" fill="white">Angular</text><text x="660" y="23" font-family="Arial" font-size="20" fill="white">Mature: Head 0.91</text></svg>'), left: 0, top: 0 });
      await sharp({ create: { width: 1280, height: 832, channels: 3, background: '#edf2ee' } }).composite(cells).jpeg({ quality: 92 }).toFile(path.join(output, `compare-${base}-${crop}.jpg`));
    }
    console.log(JSON.stringify({ base, comparison: path.join(output, `compare-${base}-face.jpg`) }));
    for (const clip of ['idle','walk','run','sword_idle','sword_attack']) for (const view of ['front','side','back']) {
      const result = await page.evaluate(value => window.captureMature(value), { face: 'mature', clip, view });
      const file = path.join(output, `${base}-${clip}-${view}.jpg`); await fs.writeFile(file, Buffer.from(result.data.split(',')[1], 'base64')); report.images.push(file);
    }
    Object.assign(report.bases[base], await page.evaluate(() => window.verifyMature()));
  }
  if (errors.length) throw new Error(errors.join('\n'));
  report.status = 'passed';
} catch (error) { report.status = 'failed'; report.error = String(error); process.exitCode = 1; }
finally { await fs.writeFile(path.join(output, 'report.json'), JSON.stringify(report, null, 2) + '\n'); await browser.close(); }
console.log(JSON.stringify({ status: report.status, error: report.error, output, bases: report.bases }));
