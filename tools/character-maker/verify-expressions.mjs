#!/usr/bin/env node
/** Exercise the production expression assembler in owned, unauthenticated Chrome.
 * node tools/character-maker/verify-expressions.mjs [/tmp/output-directory]
 * Screenshots and transient GLB reloads stay outside the repository.
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
const output = process.argv[2] ? path.resolve(process.argv[2]) : await fs.mkdtemp(path.join(os.tmpdir(), 'maker-expressions-'));
await fs.mkdir(output, { recursive: true });
const { chromium } = await importPlaywright();
const browser = await chromium.launch({ channel: 'chrome', headless: true });
const page = await browser.newPage({ viewport: { width: 360, height: 360 }, deviceScaleFactor: 1 });
const errors = [], report = { output, bases: {}, sheets: [], errors, visualAssertions: [] };
page.on('pageerror', error => errors.push(String(error)));
const imports = { imports: { three: '/maker-expressions/three/build/three.module.js', 'three/examples/jsm/': '/maker-expressions/three/examples/jsm/' } };
await page.route('**/maker-expressions/**', async route => {
  const rel = decodeURIComponent(new URL(route.request().url()).pathname.slice('/maker-expressions/'.length));
  if (!rel) return route.fulfill({ contentType: 'text/html', body: `<html><head><link rel="icon" href="data:,"><script type="importmap">${JSON.stringify(imports)}</script><style>html,body{margin:0;overflow:hidden}canvas{display:block}</style></head><body></body></html>` });
  return fulfillMakerModule(route, rel);
});
await routePublicJson(page);

async function sheet(name, captures, columns = 6) {
  const rows = Math.ceil(captures.length / columns), cells = [];
  for (let i = 0; i < captures.length; i++) {
    const left = (i % columns) * 360, top = Math.floor(i / columns) * 388;
    cells.push({ input: Buffer.from(captures[i].data.split(',')[1], 'base64'), left, top: top + 28 });
    const label = captures[i].label.replace(/[<>&"]/g, '');
    cells.push({ input: Buffer.from(`<svg width="360" height="28"><rect width="360" height="28" fill="#233c35"/><text x="10" y="20" font-family="Arial" font-size="15" fill="white">${label}</text></svg>`), left, top });
  }
  const file = path.join(output, `${name}.jpg`);
  await sharp({ create: { width: columns * 360, height: rows * 388, channels: 3, background: '#edf2ee' } }).composite(cells).jpeg({ quality: 93 }).toFile(file);
  report.sheets.push(file); console.log(JSON.stringify({ sheet: file }));
}

// These source irises are cyan/blue; the measured portrait's eye band contains
// no other blue feature. Count residual iris pixels in the closed-eye region.
async function closedIrisPixels(shot, expression) {
  const left = expression === 'blinkLeft' ? 185 : 45;
  const width = expression === 'blinkLeft' || expression === 'blinkRight' ? 130 : 270;
  const { data, info } = await sharp(Buffer.from(shot.data.split(',')[1], 'base64')).extract({left,top:130,width,height:125}).raw().toBuffer({resolveWithObject:true});
  let count = 0;
  for (let i = 0; i < data.length; i += info.channels) {
    const [r,g,b] = data.subarray(i,i+3);
    if (b-r > 25 && Math.max(g,b)-r > 35) count++;
  }
  return count;
}

async function openEyeDifference(neutral, wink, expression) {
  const region = {left:expression === 'blinkLeft' ? 60 : 205,top:150,width:95,height:85};
  const buffers = await Promise.all([neutral,wink].map(shot => sharp(Buffer.from(shot.data.split(',')[1],'base64')).extract(region).raw().toBuffer()));
  let maxPixelDelta = 0;
  for (let i = 0; i < buffers[0].length; i++) maxPixelDelta = Math.max(maxPixelDelta,Math.abs(buffers[0][i] - buffers[1][i]));
  return maxPixelDelta;
}

try {
  await page.goto(`${MAKER_URL}/maker-expressions/`);
  await page.evaluate(async () => {
    const T = await import('three');
    const { GLTFLoader } = await import('three/examples/jsm/loaders/GLTFLoader.js');
    const { assembleCharacter, exportCharacterGlb } = await import('/maker-expressions/assembly');
    const { DEFAULT_CONFIG } = await import('/maker-expressions/config');
    const expectedExpressions = ['neutral','happy','angry','sad','relaxed','surprised','blink','blinkLeft','blinkRight','aa','ih','ou','ee','oh','lookLeft','lookRight','lookUp','lookDown'];
    const suffix = { happy:'Fcl_ALL_Joy', angry:'Fcl_ALL_Angry', sad:'Fcl_ALL_Sorrow', relaxed:'Fcl_ALL_Fun', surprised:'Fcl_ALL_Surprised', blink:'Fcl_EYE_Close', blinkLeft:'Fcl_EYE_Close_L', blinkRight:'Fcl_EYE_Close_R', aa:'Fcl_MTH_A', ih:'Fcl_MTH_I', ou:'Fcl_MTH_U', ee:'Fcl_MTH_E', oh:'Fcl_MTH_O' };
    const renderer = new T.WebGLRenderer({ antialias: true, preserveDrawingBuffer: true });
    renderer.setSize(360, 360); renderer.outputColorSpace = T.SRGBColorSpace; renderer.toneMapping = T.NoToneMapping;
    document.body.append(renderer.domElement);
    const scene = new T.Scene(); scene.background = new T.Color('#edf2ee');
    const camera = new T.PerspectiveCamera(28, 1, .01, 50);
    const near = (a, b, label) => { if (a.length !== b.length || a.some((v,i) => Math.abs(v - b[i]) > 1e-6)) throw new Error(label); };
    const eyes = root => ['J_Adj_L_FaceEye','J_Adj_R_FaceEye'].map(name => root.getObjectByName(name));
    const weights = root => {
      const values = {};
      root.traverse(o => { if (!o.morphTargetDictionary) return; for (const [name,index] of Object.entries(o.morphTargetDictionary)) values[`${o.name}/${name}`] = o.morphTargetInfluences[index]; });
      return values;
    };
    const assertAppearance = (root, config) => {
      const expected = { faceRound:Number(config.face === 'round'), faceAngular:Number(config.face === 'angular') /* mature は専用形状（mature-v2）で angular morph を使わない */, eyesRound:Number(config.eyes === 'round'), eyesSlender:Number(config.eyes === 'slender'), mouthSmile:Number(config.mouth === 'smile'), mouthFull:Number(config.mouth === 'full'), bodySlim:Math.max(0,-config.build), bodyBroad:Math.max(0,config.build) };
      root.traverse(o => { if (!o.morphTargetDictionary) return; for (const [name,value] of Object.entries(expected)) { const index = o.morphTargetDictionary[name]; if (index !== undefined && Math.abs(o.morphTargetInfluences[index] - value) > 1e-6) throw new Error(`Appearance changed: ${name}`); } });
    };
    const assertExpression = (root, config) => {
      const entries = Object.entries(weights(root));
      for (const [expression, ending] of Object.entries(suffix)) {
        const matches = entries.filter(([key]) => key.endsWith(ending));
        if (!matches.length) throw new Error(`Missing authored target: ${expression}`);
        const expected = config.expression === expression ? config.expressionStrength : 0;
        if (matches.some(([,weight]) => Math.abs(weight - expected) > 1e-6)) throw new Error(`Wrong expression: ${expression} for ${config.expression}`);
      }
      if (config.expression.startsWith('look') && config.expressionStrength > 0 && eyes(root).every(o => o.quaternion.angleTo(new T.Quaternion()) < .005)) throw new Error('Gaze did not move eyes');
    };
    let state;
    window.prepareExpressions = async base => {
      if (state) { state.mixer.stopAllAction(); state.mixer.uncacheRoot(state.instance.root); state.instance.dispose(); }
      const config = { ...DEFAULT_CONFIG, base, expression:'neutral', expressionStrength:1, hair:'animeParted', hairColor:'#514036' };
      const instance = await assembleCharacter(config);
      if (typeof instance.applyEyeGaze !== 'function') throw new Error('Missing post-animation applyEyeGaze');
      scene.add(instance.root);
      state = { config, instance, mixer:new T.AnimationMixer(instance.root), height:instance.restBounds.height };
      return { expressions:expectedExpressions, clips:instance.animations.map(c => c.name) };
    };
    function frame(root, mixer, clips, height, clipName = 'idle', full = false) {
      mixer.stopAllAction();
      const clip = clips.find(c => c.name === clipName); mixer.clipAction(clip).reset().setLoop(T.LoopOnce,1).play(); mixer.setTime(clip.duration * .25);
      if (root === state.instance.root) state.instance.applyEyeGaze();
      root.updateMatrixWorld(true);
      const target = root.getObjectByName('J_Bip_C_Head').getWorldPosition(new T.Vector3()).add(new T.Vector3(0,height * .025,0));
      const distance = height * (full ? 2.1 : .34);
      if (full) target.y = height * .55;
      camera.position.set(target.x,target.y + height * .005,target.z + distance); camera.lookAt(target); camera.updateProjectionMatrix();
      renderer.render(scene,camera);
      if (!renderer.info.render.triangles) throw new Error('Empty render');
      return renderer.domElement.toDataURL('image/png');
    }
    window.captureExpression = ({ expression, strength = 1, appearance = {}, clip = 'idle', full = false }) => {
      const { instance,config,mixer,height } = state, current = { ...config,...appearance,expression,expressionStrength:strength };
      instance.apply(current);
      const data = frame(instance.root,mixer,instance.animations,height,clip,full);
      assertAppearance(instance.root,current); assertExpression(instance.root,current);
      return { data, label:expression };
    };
    window.verifyExpressionState = async () => {
      const { instance,config,mixer } = state;
      mixer.stopAllAction(); instance.apply(config); instance.applyEyeGaze();
      const original = weights(instance.root), originalEyes = eyes(instance.root).map(o => o.quaternion.toArray());
      let strengthChecks = 0;
      for (const expression of expectedExpressions) for (const expressionStrength of [0,.5,1]) {
        const current = {...config,expression,expressionStrength}; instance.apply(current); instance.applyEyeGaze(); assertExpression(instance.root,current); assertAppearance(instance.root,current); strengthChecks++;
      }
      instance.apply({...config,expression:'happy'});
      const fresh = await assembleCharacter(config); assertExpression(fresh.root,config); assertAppearance(fresh.root,config);
      const freshWeights = weights(fresh.root);
      if (Object.keys(freshWeights).some(key => Math.abs(freshWeights[key] - original[key]) > 1e-6)) throw new Error('Cached template or new instance inherited an expression');
      fresh.dispose(); instance.apply(config); instance.applyEyeGaze();
      const restored = weights(instance.root);
      if (Object.keys(original).some(key => Math.abs(restored[key] - original[key]) > 1e-6)) throw new Error('Neutral did not restore original weights');
      eyes(instance.root).forEach((eye,i) => near(eye.quaternion.toArray(),originalEyes[i],'Neutral did not restore gaze'));
      return { strengthChecks, neutralRestored:true, appearancePreserved:true, cachedTemplateIndependent:true };
    };
    window.verifyExpressionExport = async ({expression,appearance = {}}) => {
      const { instance,config,height } = state, current = {...config,...appearance,expression,expressionStrength:1};
      const bytes = await exportCharacterGlb(current), loaded = await new GLTFLoader().parseAsync(bytes,'');
      const meta = loaded.scene.getObjectByName('CharacterMaker')?.userData.characterMaker;
      if (meta?.config?.expression !== expression || meta.config.expressionStrength !== 1) throw new Error('Export lost selected expression metadata');
      assertExpression(loaded.scene,current); assertAppearance(loaded.scene,current);
      const clipNames = loaded.animations.map(c => c.name);
      if (clipNames.length !== 7 || state.instance.animations.some(c => !clipNames.includes(c.name))) throw new Error('Export lost motion clips');
      scene.remove(instance.root); scene.add(loaded.scene);
      const mixer = new T.AnimationMixer(loaded.scene);
      let data;
      // のけぞり（hit）と ＞＜（face_hurt）は、選んだ表情の代わりに ＞＜ を出すクリップ（docs/main-character-spec.md）なので、表情の確認から外す。
      for (const clip of loaded.animations) { data = frame(loaded.scene,mixer,loaded.animations,height,clip.name); if (!['hit','face_hurt'].includes(clip.name)) assertExpression(loaded.scene,current); }
      data = frame(loaded.scene,mixer,loaded.animations,height);
      mixer.stopAllAction(); mixer.uncacheRoot(loaded.scene); scene.remove(loaded.scene); scene.add(instance.root);
      const geometries = new Set(), materials = new Set(), maps = new Set();
      loaded.scene.traverse(o => { if (!o.isMesh) return; geometries.add(o.geometry); for (const m of Array.isArray(o.material)?o.material:[o.material]) { materials.add(m); if (m.map) maps.add(m.map); } });
      geometries.forEach(g=>g.dispose()); materials.forEach(m=>m.dispose()); maps.forEach(m=>m.dispose());
      return { expression,appearance,bytes:bytes.byteLength,clips:clipNames,selectedStateRestored:true,data,label:`Reloaded ${expression}${appearance.eyes ? ' / '+appearance.eyes : ''}` };
    };
  });

  // Produce the two main sheets first, so visual review need not wait for exports.
  for (const base of ['feminine','masculine']) {
    report.bases[base] = await page.evaluate(base => window.prepareExpressions(base), base);
    const captures = [];
    for (const expression of report.bases[base].expressions) captures.push(await page.evaluate(expression => window.captureExpression({expression}), expression));
    report.bases[base].nativeClosedIrisPixels = {};
    for (const shot of captures.filter(item => ['blink','blinkLeft','blinkRight','happy'].includes(item.label))) {
      report.bases[base].nativeClosedIrisPixels[shot.label] = await closedIrisPixels(shot,shot.label);
    }
    await sheet(`${base}-all-expressions`,captures);
  }
  for (const base of ['feminine','masculine']) {
    await page.evaluate(base => window.prepareExpressions(base),base);
    const appearance = [];
    for (const eyes of ['round','slender']) for (const expression of ['neutral','blink','blinkLeft','blinkRight','happy','surprised']) {
      const shot = await page.evaluate(value => window.captureExpression(value),{expression,appearance:{eyes}}); shot.label = `${eyes} / ${expression}`; appearance.push(shot);
      if (['blink','blinkLeft','blinkRight','happy'].includes(expression)) report.visualAssertions.push({base,eyes,expression,residualIrisPixels:await closedIrisPixels(shot,expression)});
    }
    await sheet(`${base}-eye-combinations`,appearance);
    const intermediate = [];
    for (const eyes of ['round','slender']) for (const expression of ['blink','blinkLeft','blinkRight','happy']) {
      const shot = await page.evaluate(value => window.captureExpression(value),{expression,strength:.5,appearance:{eyes}}); shot.label = `${eyes} / ${expression} 50%`; intermediate.push(shot);
      if (expression === 'blinkLeft' || expression === 'blinkRight') {
        const neutral = appearance.find(item => item.label === `${eyes} / neutral`);
        report.visualAssertions.push({base,eyes,expression,strength:.5,openEyeMaxPixelDelta:await openEyeDifference(neutral,shot,expression)});
      }
    }
    await sheet(`${base}-eye-half-strength`,intermediate,4);
    const mouth = [];
    for (const mouthStyle of ['natural','smile','full']) for (const expression of ['neutral','aa','ih','ou','ee','oh']) {
      const shot = await page.evaluate(value => window.captureExpression(value),{expression,appearance:{mouth:mouthStyle}}); shot.label = `${mouthStyle} / ${expression}`; mouth.push(shot);
    }
    await sheet(`${base}-mouth-combinations`,mouth);
    const mature = [];
    for (const expression of ['neutral','happy','angry','sad','relaxed','surprised']) mature.push(await page.evaluate(value => window.captureExpression(value),{expression,appearance:{face:'mature',eyes:'slender'}}));
    await sheet(`${base}-mature-expressions`,mature);
    let motionChecks = 0;
    for (const expression of ['happy','lookLeft']) for (const clip of report.bases[base].clips) { await page.evaluate(value => window.captureExpression(value),{expression,clip,full:true}); motionChecks++; }
    Object.assign(report.bases[base],await page.evaluate(() => window.verifyExpressionState()),{appearanceCaptures:appearance.length + mouth.length + mature.length,motionChecks,exports:[]});
    const reloads = [];
    for (const request of [{expression:'happy'},{expression:'lookLeft'},{expression:'blink',appearance:{eyes:'round'}}]) {
      const result = await page.evaluate(request => window.verifyExpressionExport(request),request);
      const { data,label,...summary } = result; report.bases[base].exports.push(summary); reloads.push({data,label});
      if (request.expression === 'blink') report.visualAssertions.push({base,eyes:'round',expression:'blink',source:'GLB reload',residualIrisPixels:await closedIrisPixels({data},'blink')});
    }
    await sheet(`${base}-export-reload`,reloads,3);
  }
  if (errors.length) throw new Error(errors.join('\n'));
  // Compare lossless pixels with the authored closed eye at the same angle.
  // Its antialiased edge can contain a handful of blue pixels even at 100%.
  for (const check of report.visualAssertions) if (check.residualIrisPixels !== undefined) check.authoredBaseline = report.bases[check.base].nativeClosedIrisPixels[check.expression];
  const leaks = report.visualAssertions.filter(check => check.residualIrisPixels > check.authoredBaseline + 2);
  if (leaks.length) throw new Error(`Closed eyes retain visible iris: ${JSON.stringify(leaks)}`);
  const changedOpenEyes = report.visualAssertions.filter(check => check.openEyeMaxPixelDelta > 1);
  if (changedOpenEyes.length) throw new Error(`Wink changed the opposite eye: ${JSON.stringify(changedOpenEyes)}`);
  report.status = 'passed';
} catch (error) { report.status = 'failed'; report.error = String(error); process.exitCode = 1; }
finally { await fs.writeFile(path.join(output,'report.json'),JSON.stringify(report,null,2)+'\n'); await browser.close(); }
console.log(JSON.stringify({status:report.status,error:report.error,output,bases:report.bases}));
