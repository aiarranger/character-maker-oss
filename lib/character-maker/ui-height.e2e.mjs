// Height must change the visible character, while leaving user camera work intact.
// Uses a fresh, unauthenticated native Chrome; no existing browser/profile is read.
// node lib/character-maker/ui-height.e2e.mjs [output-directory]
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { importPlaywright, MAKER_URL } from '../../tools/lib/paths.mjs';

const output = process.argv[2] ?? await mkdtemp(join(tmpdir(), 'maker-height-'));
await mkdir(output, { recursive: true });
const { chromium } = await importPlaywright();
const browser = await chromium.launch({ channel: 'chrome', headless: true });
const page = await browser.newPage({ viewport: { width: 1257, height: 960 }, deviceScaleFactor: 1 });
page.setDefaultTimeout(30000);
const errors = [], results = [];
page.on('pageerror', error => errors.push(error.message));

// Observe the matrices actually sent to WebGL, without React internals or an
// application test API. A fixed floor's model-view matrix measures all camera
// motion. Skinned meshes also expose portrait distance along the front axis.
await page.addInitScript(() => {
  const contexts = new WeakMap();
  window.__makerCameraProbe = canvas => contexts.get(canvas)?.snapshot ?? null;
  for (const Type of [window.WebGLRenderingContext, window.WebGL2RenderingContext]) {
    if (!Type) continue;
    const proto = Type.prototype;
    const locations = new WeakMap();
    const state = gl => {
      let value = contexts.get(gl.canvas);
      if (!value) { value = { programs: new Map(), current: null, snapshot: { draws: 0 } }; contexts.set(gl.canvas, value); }
      return value;
    };
    const wrap = (name, after) => {
      const original = proto[name];
      proto[name] = function(...args) { const result = original.apply(this, args); after(this, args, result); return result; };
    };
    wrap('getUniformLocation', (gl, [program, name], location) => {
      if (!location) return;
      const s = state(gl);
      if (!s.programs.has(program)) s.programs.set(program, { names: new Set() });
      s.programs.get(program).names.add(name);
      locations.set(location, { program, name });
    });
    wrap('useProgram', (gl, [program]) => { state(gl).current = program; });
    for (const method of ['uniform1f', 'uniformMatrix4fv']) wrap(method, (gl, [location, value, matrix]) => {
      const uniform = locations.get(location); if (!uniform) return;
      const p = state(gl).programs.get(uniform.program); if (!p) return;
      if (uniform.name === 'opacity') p.opacity = value;
      if (uniform.name === 'modelViewMatrix') p.matrix = Array.from(matrix);
    });
    const draw = gl => {
      const s = state(gl), p = s.programs.get(s.current); if (!p?.matrix) return;
      s.snapshot.draws++;
      if (Math.abs(p.opacity - 0.24) < 1e-5) s.snapshot.floor = { matrix: p.matrix, draw: s.snapshot.draws };
      if (p.names.has('boneTexture')) s.snapshot.skin = { matrix: p.matrix, draw: s.snapshot.draws };
    };
    wrap('drawElements', draw); wrap('drawArrays', draw);
  }
});

const canvas = page.locator('[aria-label="キャラクターの3Dプレビュー"] canvas');
const slider = page.getByRole('slider', { name: '身長', exact: true });
async function ready() {
  await page.waitForFunction(() => [...document.querySelectorAll('button')].some(button => button.textContent === 'GLBを書き出す' && !button.disabled), null, { timeout: 90000 });
}
async function height(key, expected) {
  await slider.press(key);
  await page.waitForFunction(expected => JSON.parse(sessionStorage.getItem('characters.character-maker.v1.draft') ?? 'null')?.document.config.height === expected, expected);
  assert.equal(Number(await slider.inputValue()), expected);
}
async function observation(label) {
  const data = await canvas.evaluate(async canvas => {
    // A button click can resolve before React's Canvas layout effect paints.
    await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const copy = document.createElement('canvas'); copy.width = canvas.width; copy.height = canvas.height;
    const ctx = copy.getContext('2d'); ctx.drawImage(canvas, 0, 0);
    const pixels = ctx.getImageData(0, 0, copy.width, copy.height).data;
    let x0 = copy.width, y0 = copy.height, x1 = -1, y1 = -1;
    for (let y = 0; y < copy.height; y++) for (let x = 0; x < copy.width; x++) {
      if (pixels[(y * copy.width + x) * 4 + 3] <= 250) continue;
      x0 = Math.min(x0, x); x1 = Math.max(x1, x); y0 = Math.min(y0, y); y1 = Math.max(y1, y);
    }
    return { width: copy.width, height: copy.height, bounds: { x0, y0, x1, y1, width: x1 - x0 + 1, height: y1 - y0 + 1 }, gpu: window.__makerCameraProbe(canvas), png: canvas.toDataURL() };
  });
  if (label) await writeFile(join(output, `${label}.png`), Buffer.from(data.png.split(',')[1], 'base64'));
  delete data.png;
  assert.ok(data.bounds.height > 100, 'An opaque character must actually render.');
  return data;
}
function matrixClose(a, b, message, tolerance = 0.001) {
  assert.equal(a.length, 16); assert.equal(b.length, 16);
  const delta = Math.max(...a.map((value, i) => Math.abs(value - b[i])));
  assert.ok(delta < tolerance, `${message}: maximum matrix delta ${delta}`);
  return delta;
}
function inside(data, message) {
  const b = data.bounds;
  assert.ok(b.x0 > 1 && b.y0 > 1 && b.x1 < data.width - 2 && b.y1 < data.height - 2, `${message}: ${JSON.stringify(b)}`);
}
async function settleCamera() {
  let before = await observation();
  for (let i = 0; i < 20; i++) {
    await page.waitForTimeout(100);
    const after = await observation();
    if (before.gpu?.floor && after.gpu?.floor && Math.max(...before.gpu.floor.matrix.map((v, j) => Math.abs(v - after.gpu.floor.matrix[j]))) < 1e-5) return after;
    before = after;
  }
  throw new Error('The camera did not settle after the real orbit/wheel input.');
}
try {
  await page.goto(`${MAKER_URL}/character-maker/classic`, { waitUntil: 'domcontentloaded', timeout: 120000 });
  await ready();
  for (const [style, label] of [['fantasy', /^ファンタジー/], ['comic', /^アメコミ/]]) {
    await page.getByRole('button', { name: label }).click(); await ready();
    await page.getByRole('navigation', { name: '変更するパーツ' }).getByRole('button', { name: /^体型/ }).click();
    await page.getByRole('button', { name: '正面', exact: true }).click();
    await height('Home', 0.85); const min = await observation(`${style}-min`);
    await height('End', 1.12); const max = await observation(`${style}-max`);
    const ratio = max.bounds.height / min.bounds.height;
    console.log(`${style}: minimum ${min.bounds.height}px, maximum ${max.bounds.height}px`);
    assert.ok(ratio > 1.25 && ratio < 1.39, `Height must visibly grow in ${style}; actual ratio ${ratio}`);
    inside(min, `${style} minimum height`); inside(max, `${style} maximum height`);
    matrixClose(min.gpu.floor.matrix, max.gpu.floor.matrix, `${style} height must not move the full camera`);
    await page.screenshot({ path: join(output, `${style}-max-ui.jpg`), type: 'jpeg', quality: 90 });

    // Drag and wheel are genuine pointer input; the floor matrix proves both
    // the altered view and its retention when the model's height changes.
    const box = await canvas.boundingBox();
    await page.mouse.move(box.x + box.width * 0.55, box.y + box.height * 0.5);
    await page.mouse.down(); await page.mouse.move(box.x + box.width * 0.7, box.y + box.height * 0.6, { steps: 12 }); await page.mouse.up();
    await page.mouse.wheel(0, 100);
    const orbited = await settleCamera();
    assert.ok(Math.max(...orbited.gpu.floor.matrix.map((v, i) => Math.abs(v - max.gpu.floor.matrix[i]))) > 0.1, 'Real orbit and wheel must change the camera.');
    await height('Home', 0.85); const retained = await observation(`${style}-orbit-retained`);
    const orbitDelta = matrixClose(orbited.gpu.floor.matrix, retained.gpu.floor.matrix, `${style} user orbit/zoom must survive height edits`);

    // The body controls stay open when selecting the portrait framing.
    await page.getByRole('button', { name: '正面', exact: true }).click();
    await page.getByRole('button', { name: '顔', exact: true }).click();
    const portraitMin = await observation(`${style}-portrait-min`);
    await height('End', 1.12); const portraitMax = await observation(`${style}-portrait-max`);
    assert.ok(portraitMax.gpu.skin.draw > portraitMin.gpu.skin.draw, 'Portrait measurements must come from new rendered frames.');
    const portraitDepthDelta = Math.abs(portraitMin.gpu.skin.matrix[14] - portraitMax.gpu.skin.matrix[14]);
    assert.ok(portraitDepthDelta < 0.003, `${style} portrait distance changed: ${portraitDepthDelta}`);
    assert.ok(Math.abs(portraitMin.gpu.skin.matrix[13] - portraitMax.gpu.skin.matrix[13]) > 0.25, `${style} portrait must follow the actual head vertically.`);
    assert.ok(portraitMin.bounds.y0 > 2 && portraitMax.bounds.y0 > 2, `${style} portrait hair must remain in frame.`);

    // Sample one complete attack at maximum height, including its furthest
    // silhouettes. Alpha excludes the translucent ground disk and CSS stage.
    await page.getByRole('button', { name: '剣を振る', exact: true }).click();
    const attack = [];
    for (let frame = 0; frame < 34; frame++) {
      await page.waitForTimeout(50);
      const sample = await observation(); inside(sample, `${style} maximum-height attack frame ${frame}`); attack.push(sample);
    }
    await page.getByRole('button', { name: '剣を振る', exact: true }).click();
    await page.waitForTimeout(350);
    await observation(`${style}-attack`);
    await page.screenshot({ path: join(output, `${style}-attack-ui.jpg`), type: 'jpeg', quality: 90 });
    results.push({ style, min, max, ratio, orbitDelta, portraitMin, portraitMax, portraitDepthDelta, attack });
    console.log(`${style}: visible height ratio ${ratio.toFixed(3)}, orbit retained, portrait distance retained, 34 attack frames inside canvas`);
  }
  assert.deepEqual(errors, []);
  await writeFile(join(output, 'result.json'), JSON.stringify({ success: true, output, results, errors }, null, 2));
  console.log(JSON.stringify({ success: true, output, errors }));
} catch (error) {
  await page.screenshot({ path: join(output, 'failure.jpg'), type: 'jpeg', quality: 90 }).catch(() => {});
  await writeFile(join(output, 'failure.json'), JSON.stringify({ success: false, message: error.message, stack: error.stack, results, errors }, null, 2));
  console.error(error); process.exitCode = 1;
} finally { await browser.close(); }
