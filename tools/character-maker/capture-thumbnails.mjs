#!/usr/bin/env node
/** Render option thumbnails through the same assembler as preview and GLB export.
 * Uses an owned, unauthenticated browser. No existing tabs or profiles are accessed.
 * node tools/character-maker/capture-thumbnails.mjs [output directory] [--presets-only|--expressions-only]
 */
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import ts from 'typescript';
import { importPlaywright, NODE_MODULES, MAKER_URL } from '../lib/paths.mjs';
import { fulfillMakerModule, routePublicJson } from '../lib/maker-modules.mjs';

const repo = fileURLToPath(new URL('../../', import.meta.url));
const args = process.argv.slice(2);
const output = path.resolve(args.find(arg => !arg.startsWith('--')) ?? path.join(repo, 'parts/assets/thumbnails-v2'));
await fs.mkdir(output, { recursive: true });
const { chromium } = await importPlaywright();
const browser = await chromium.launch({ channel: 'chrome', headless: true });
const page = await browser.newPage({ viewport: { width: 320, height: 320 }, deviceScaleFactor: 1 });
const errors = [];
page.on('pageerror', error => errors.push(String(error)));
const map = { imports: { three: '/maker-capture/three/build/three.module.js', 'three/examples/jsm/': '/maker-capture/three/examples/jsm/' } };
await page.route('**/maker-capture/**', async route => {
  const url = new URL(route.request().url());
  const rel = decodeURIComponent(url.pathname.slice('/maker-capture/'.length));
  if (!rel) return route.fulfill({ contentType: 'text/html', body: `<html><head><script type="importmap">${JSON.stringify(map)}</script><style>html,body{margin:0;width:100%;height:100%;overflow:hidden}canvas{display:block}</style></head><body></body></html>` });
  return fulfillMakerModule(route, rel);
});
await routePublicJson(page);
try {
  await page.goto(`${MAKER_URL}/maker-capture/`);
  const captures = await page.evaluate(async () => {
    const THREE = await import('three');
    const { assembleCharacter } = await import('/maker-capture/assembly');
    const configModule = await import('/maker-capture/config');
    const renderer = new THREE.WebGLRenderer({ antialias: true, preserveDrawingBuffer: true });
    renderer.setSize(320, 320); renderer.setPixelRatio(1);
    renderer.outputColorSpace = THREE.SRGBColorSpace; renderer.toneMapping = THREE.NoToneMapping;
    document.body.appendChild(renderer.domElement);
    const scene = new THREE.Scene(); scene.background = new THREE.Color('#e5edeb');
    const camera = new THREE.PerspectiveCamera(28, 1, 0.01, 50);
    const entries = [];
    for (const style of ['comic', 'fantasy']) for (const base of ['feminine', 'masculine']) {
      const defaults = { ...(style === 'comic' ? configModule.COMIC_DEFAULT_CONFIG : configModule.DEFAULT_CONFIG), base };
      const hairs = style === 'comic' ? ['parted','long','buns','crop','pixie','none'] : ['animeParted','sidePony','layered','none'];
      const options = { hair: hairs, face: configModule.getFaceShapes(style), eyes: ['almond','round','slender'], mouth: ['natural','smile','full'] };
      if (style === 'fantasy') options.expression = configModule.EXPRESSION_CHOICES.filter(item => item.group === 'emotion').map(item => item.id);
      for (const [kind, values] of Object.entries(options)) for (const value of values) {
        // Expression examples use each body's existing starter portrait.
        const portrait = kind === 'expression' ? configModule.getPresets(style).find(preset => preset.config.base === base).config : defaults;
        entries.push({ style, base, kind, value, config: { ...portrait, [kind]: value } });
      }
    }
    // Preset portraits must show the same base and appearance selected in the UI.
    for (const style of configModule.STYLES) for (const preset of configModule.getPresets(style)) {
      entries.push({ style, base: preset.config.base, kind: 'preset', value: preset.id, config: preset.config });
    }
    window.captureMakerThumbnail = async entry => {
      const instance = await assembleCharacter(entry.config);
      const mixer = new THREE.AnimationMixer(instance.root);
      scene.add(instance.root);
      const idle = instance.animations.find(clip => clip.name === 'idle');
      mixer.clipAction(idle).play(); mixer.update(idle.duration * 0.25);
      instance.root.updateMatrixWorld(true);
      const bounds = new THREE.Box3().setFromObject(instance.root);
      const height = bounds.max.y - bounds.min.y;
      const head = instance.root.getObjectByName(entry.style === 'fantasy' ? 'J_Bip_C_Head' : 'Head');
      const target = head.getWorldPosition(new THREE.Vector3()).add(new THREE.Vector3(0, height * 0.067, 0));
      const hairView = entry.kind === 'hair' || entry.kind === 'preset';
      const distance = height * (hairView ? 0.7 : 0.34);
      if (hairView) target.y -= height * 0.012;
      else target.y -= height * 0.052;
      camera.position.set(target.x + distance * (hairView ? 0.19 : 0.015), target.y + height * 0.01, target.z + distance);
      camera.lookAt(target); camera.updateProjectionMatrix();
      renderer.render(scene, camera);
      const result = { name: `thumb-${entry.style}-${entry.base}-${entry.kind}-${entry.value}.jpg`, data: renderer.domElement.toDataURL('image/jpeg', 0.93), triangles: renderer.info.render.triangles };
      mixer.stopAllAction(); mixer.uncacheRoot(instance.root); instance.dispose();
      return result;
    };
    return entries;
  });
  const selected = args.includes('--presets-only') ? captures.filter(entry => entry.kind === 'preset')
    : args.includes('--expressions-only') ? captures.filter(entry => entry.kind === 'expression') : captures;
  for (const entry of selected) {
    const result = await page.evaluate(entry => window.captureMakerThumbnail(entry), entry);
    if (!result.triangles) throw new Error(`Empty thumbnail ${result.name}`);
    await fs.writeFile(path.join(output, result.name), Buffer.from(result.data.split(',')[1], 'base64'));
  }
  if (errors.length) throw new Error(errors.join('\n'));
  console.log(JSON.stringify({ output, thumbnails: selected.length }));
} finally { await browser.close(); }
