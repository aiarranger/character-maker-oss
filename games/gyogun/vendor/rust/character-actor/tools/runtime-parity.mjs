// 配布を通らない主役（.character.zip・manifest なしの持ち込みの .glb）の読み方と動きを、TS の配布済みの部品で書き出す。Rust の試験 tests/runtime_parity.rs が同じ入力・同じ刻みで比べる。
//   node crates/character-actor/tools/runtime-parity.mjs <出力のフォルダ>
// 部品（SHA を照合した tgz）: packages/character-runtime（0.2.7）・character-actor・character-motion の配布。NPC・汎用（0.2.5 の写し）は character-actor-npc の tools/npc-parity.mjs。
// 入力（aisns のフォルダ。RUNTIME_PARITY_AISNS で替えられる）: public/character-packages の catalog.json の主役の zip と、持ち込みの見本の .glb。
//   汎用の zip と NPC の素の .glb は、主役の読み手で読んだ結果（持ち込みとして読める・読めない）だけ比べる。
// 読み方: readCharacterFile の結果（manifest・stats）と、壊れた入力の文言。
// 主役の zip で登録表（character-motion）に id と SHA が合い、stillBones に level がある主役は、PackageActor の KeepHeldLevel と同じく update のあとに levelBone を当てる。
// 動き: createCharacterInstance の update（PackageActor・NPC の CharacterActor と同じ入力: moving・running・speech・faceControl・photoPose・springReset・isNpc・nowSeconds）を
//   tools/runtime-scenes.json の場面で回し、節点の位置と向き・モーフを書き出す。three はこのリポジトリの 0.178（aisns と同じ）。テクスチャは読まない。
// 主役の部品（@aiarranger/character-actor の配布）の写真・話す口・npc: aisns の同梱の主役と同じ形の版（lib/wilds/hero-actor-source.ts。登録表なし・共通ののけぞりと休止は
//   aisns の public/animations）を tools/runtime-actor-scenes.json の場面で動かし、節点・モーフ・持ち手・合図を書き出す（npc あり・なしの 2 回）。
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = fileURLToPath(import.meta.url);
const repo = path.resolve(path.dirname(here), '../../..');
const aisns = process.env.RUNTIME_PARITY_AISNS ?? path.resolve(repo, '../aisns');
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
/** tgz を SHA256SUMS と照合する */
const checked = (dir, file) => {
  const tarball = path.join(dir, file), listed = readFileSync(path.join(dir, 'SHA256SUMS'), 'utf8').split('\n').find(l => l.endsWith(`  ${file}`))?.slice(0, 64);
  const got = sha(readFileSync(tarball));
  if (listed !== got) throw Error(`${tarball}: SHA256SUMS と合いません`);
  return { tarball, sha256: got };
};

if (!process.env.RUNTIME_PARITY_INNER) {
  const heroVersion = JSON.parse(readFileSync(path.join(repo, 'packages/character-runtime/package.json'), 'utf8')).version;
  const version = name => JSON.parse(readFileSync(path.join(repo, 'packages', name, 'package.json'), 'utf8')).version;
  const packages = [
    { name: 'character-runtime', ...checked(path.join(repo, 'packages/character-runtime/releases'), `aiarranger-character-runtime-${heroVersion}.tgz`) },
    { name: 'character-actor', ...checked(path.join(repo, 'packages/character-actor/releases'), `aiarranger-character-actor-${version('character-actor')}.tgz`) },
    { name: 'character-motion', ...checked(path.join(repo, 'packages/character-motion/releases'), `aiarranger-character-motion-${version('character-motion')}.tgz`) },
  ];
  const scenes = readFileSync(path.join(path.dirname(here), 'runtime-scenes.json'));
  const release = { packages: packages.map(({ name, sha256 }) => ({ name, sha256 })), tool: sha(readFileSync(here)), scenes: sha(scenes), actorScenes: sha(readFileSync(path.join(path.dirname(here), 'runtime-actor-scenes.json'))) };
  release.key = sha(JSON.stringify(release));
  if (process.argv[2] === '--release-info') { console.log(JSON.stringify(release)); process.exit(0); }
  const three = process.env.RUNTIME_PARITY_THREE ?? realpathSync(path.join(repo, 'node_modules/three'));
  const dir = mkdtempSync(path.join(os.tmpdir(), 'runtime-parity-')), nm = path.join(dir, 'node_modules');
  mkdirSync(path.join(nm, '@aiarranger'), { recursive: true });
  symlinkSync(three, path.join(nm, 'three'));
  cpSync(path.dirname(realpathSync(path.join(repo, 'node_modules/@pixiv/three-vrm'))), path.join(nm, '@pixiv'), { recursive: true, dereference: true });
  for (const p of packages) {
    const to = path.join(nm, '@aiarranger', p.name);
    mkdirSync(to, { recursive: true });
    execFileSync('tar', ['-xzf', p.tarball, '--strip-components=1', '-C', to]);
  }
  cpSync(here, path.join(dir, 'run.mjs'));
  cpSync(path.join(path.dirname(here), 'runtime-scenes.json'), path.join(dir, 'runtime-scenes.json'));
  cpSync(path.join(path.dirname(here), 'runtime-actor-scenes.json'), path.join(dir, 'runtime-actor-scenes.json'));
  try {
    execFileSync(process.execPath, ['run.mjs', path.resolve(process.argv[2])], { cwd: dir, stdio: 'inherit', env: { ...process.env, RUNTIME_PARITY_INNER: '1', RUNTIME_PARITY_RELEASE: JSON.stringify(release), RUNTIME_PARITY_AISNS: aisns, RUNTIME_PARITY_REPO: repo } });
  } finally { rmSync(dir, { recursive: true, force: true }); }
  process.exit(0);
}

const release = JSON.parse(process.env.RUNTIME_PARITY_RELEASE), repoDir = process.env.RUNTIME_PARITY_REPO;
const THREE = await import('three');
const { GLTFLoader } = await import('three/examples/jsm/loaders/GLTFLoader.js');
const { MeshoptDecoder } = await import('three/examples/jsm/libs/meshopt_decoder.module.js');
const { VRMSpringBoneLoaderPlugin } = await import('@pixiv/three-vrm');
const { levelBone, levelFor, stillBones, registeredCharacter } = await import('@aiarranger/character-motion');
const KINDS = {
  hero: { pkg: await import('@aiarranger/character-runtime/package'), rt: await import('@aiarranger/character-runtime/runtime') },
};
const noop = () => {};
const ctx = new Proxy({}, { get: (_, k) => (k === 'createRadialGradient' || k === 'createLinearGradient' ? () => ({ addColorStop: noop }) : k === 'getImageData' ? () => null : noop), set: () => true });
globalThis.document = { createElement: () => ({ getContext: () => ctx, width: 0, height: 0 }) };
await MeshoptDecoder.ready;
const noTexture = name => () => ({ name, loadTexture: () => Promise.resolve(null) });
// character-runtime の createGlbCharacterLoader({ spring: true }) と同じ（テクスチャだけ読まない）
const parse = bytes => new GLTFLoader().setMeshoptDecoder(MeshoptDecoder).register(noTexture('EXT_texture_webp')).register(noTexture('EXT_texture_avif')).register(noTexture('NO_TEXTURES_IN_NODE'))
  .register(p => new VRMSpringBoneLoaderPlugin(p)).parseAsync(bytes.slice().buffer, '');

const out = process.argv[2];
mkdirSync(out, { recursive: true });
const SCENES = JSON.parse(readFileSync(path.join(process.cwd(), 'runtime-scenes.json'), 'utf8'));
const DT = 1 / 60, EVERY = 2;
const DT_PATTERNS = { 60: [1 / 60], 144: [1 / 144], uneven: [1 / 60, 1 / 60, 0.2, 0.001, 0, 1 / 30, 1 / 144, 1 / 24, 0.05, 0.051] };
const FACES = {
  happy: { emotion: 'happy', emotionWeight: 0.8, mouth: 'aa', mouthWeight: 0.5, look: 'lookUp', lookWeight: 0.6 },
  sad: { emotion: 'sad', emotionWeight: 1, mouth: 'neutral', mouthWeight: 1, look: 'neutral', lookWeight: 1 },
  surprised: { emotion: 'surprised', emotionWeight: 0.9, mouth: 'oh', mouthWeight: 0.7, look: 'lookLeft', lookWeight: 0.5 },
};

// ---- 入力 ----
const items = [];
const npcCatalog = JSON.parse(readFileSync(path.join(aisnsDir(), 'lib/welcome-npcs.json'), 'utf8'));
function aisnsDir() { return process.env.RUNTIME_PARITY_AISNS; }
for (const npc of npcCatalog) items.push({ name: `npc-${npc.id}`, kind: 'hero', readOnly: true, file: path.join(aisnsDir(), 'public', npc.model), filename: npc.model });
const packages = JSON.parse(readFileSync(path.join(aisnsDir(), 'public/character-packages/catalog.json'), 'utf8'));
for (const row of packages.characters) {
  items.push({ name: `pkg-${row.manifest.id}`, kind: 'hero', readOnly: row.kind !== 'hero', file: path.join(aisnsDir(), 'public/character-packages', row.package), filename: row.package });
}
// 持ち込みの .glb（manifest なし。主役用の読み手）
for (const glb of ['kiyogon-cbf0be289a1f.glb', 'generic-cat-4392298cc474.glb']) items.push({ name: `import-${glb.replace(/\.glb$/, '')}`, kind: 'hero', file: path.join(aisnsDir(), 'public/character-packages', glb), filename: `C:\\Users\\me\\Downloads\\${glb.toUpperCase()}` });

/** 壊れた入力と、読めない形（どの種類の読み手で読むか）。Rust の試験は同じ物を同じ作り方で作る */
const brokenInputs = () => {
  const zip = readFileSync(path.join(aisnsDir(), 'public/character-packages/generic-human-b-d8b26fa368e7.character.zip'));
  const glb = readFileSync(path.join(aisnsDir(), 'public/models/welcome/socrates-3a8d975e7fea.glb'));
  const layout = readFileSync(path.join(repoDir, 'public/hero-weapons/kiyogon/kiyogon-ba45496e1d0c.character.zip'));
  const flip = (b, at) => { const c = Uint8Array.from(b); c[at] ^= 0xff; return c; };
  return [
    ['zip-half', zip.subarray(0, zip.length >> 1), 'a.character.zip'],
    ['zip-eocd', flip(zip, zip.length - 22), 'a.character.zip'],
    ['zip-data', flip(zip, 2000), 'a.character.zip'],
    ['zip-layout', layout, 'kiyogon.character.zip'],
    ['glb-short', glb.subarray(0, 1000), 'socrates.glb'],
    ['glb-magic', flip(glb, 0), 'socrates.glb'],
    ['glb-json', flip(glb, 40), 'socrates.glb'],
    ['glb-bin', glb.subarray(0, glb.length - 4), 'socrates.glb'],
    ['empty', new Uint8Array(0), 'x.character.zip'],
  ];
};
const tryRead = async (kind, bytes, filename) => {
  try { const a = await KINDS[kind].pkg.readCharacterFile(bytes, filename); return { ok: true, manifest: a.manifest, source: a.source, stats: a.stats ?? null, thumbnail: a.thumbnail ? sha(a.thumbnail) : null }; }
  catch (e) { return { ok: false, error: e.message }; }
};
const errors = [];
for (const [name, bytes, filename] of brokenInputs()) for (const kind of ['hero']) errors.push({ name, kind, filename, ...(await tryRead(kind, bytes, filename)) });
writeFileSync(path.join(out, 'errors.json'), JSON.stringify({ release, errors }));

// ---- 透過の値（KHR_materials_transmission・volume・ior。地域の敵 8 体と拡張マップの 2 つ）。three の GLTFLoader が材質に書く値 ----
const TRANSMISSION = ['models/map-expansion/enemies/mob-popcorn-asteroids-1.glb', 'models/map-expansion/enemies/mob-ice-floe-field-5.glb', 'models/map-expansion/enemies/mob-ramune-stream-4.glb',
  'models/map-expansion/enemies/mob-ramune-stream-1.glb', 'models/map-expansion/enemies/mob-croissant-sky-isles-2.glb', 'models/map-expansion/enemies/mob-sunken-restaurant-4.glb',
  'models/map-expansion/enemies/mob-biscuit-canal-city-5.glb', 'models/map-expansion/enemies/mob-soda-marsh-3.glb', 'fields/expansion-models/jelly-island-edge/model.glb', 'fields/expansion-models/ramune-marble/model.glb',
  'fields/expansion-models/amber-arch/model.glb'];
const materials = [];
for (const file of TRANSMISSION) {
  const g = await parse(new Uint8Array(readFileSync(path.join(aisnsDir(), 'public', file))));
  const list = await g.parser.getDependencies('material');
  materials.push({ file, materials: list.map(m => ({ name: m.name, type: m.type, ...(m.isMeshPhysicalMaterial ? { transmission: m.transmission, thickness: m.thickness, ior: m.ior, attenuationDistance: Number.isFinite(m.attenuationDistance) ? m.attenuationDistance : 'inf', attenuationColor: m.attenuationColor.toArray() } : {}) })) });
}
writeFileSync(path.join(out, 'materials.json'), JSON.stringify({ release, materials }));

// ---- 動き ----
for (const item of items) {
  const t0 = Date.now();
  const bytes = new Uint8Array(readFileSync(item.file));
  const read = await tryRead(item.kind, bytes, item.filename);
  // 登録表の行（aisns の registeredCharacter(id, sha)）と、PackageActor の KeepHeldLevel が当てる持つ物の骨
  const registry = read.ok ? registeredCharacter(read.manifest.id, read.manifest.model.sha256) : null;
  const level = read.ok ? levelFor(stillBones(read.manifest.id, read.manifest.model.sha256), 'idle') : null;
  const doc = { release, name: item.name, kind: item.kind, file: path.relative(aisnsDir(), item.file), filename: item.filename, read, readOnly: !!item.readOnly, registry, level };
  if (!read.ok || item.readOnly) { writeFileSync(path.join(out, `${item.name}.json`), JSON.stringify(doc)); console.log(`${item.name}: ${read.ok ? '読んだだけ' : '読めない ' + read.error}`); continue; }
  const asset = await KINDS[item.kind].pkg.readCharacterFile(bytes, item.filename);
  const rt = KINDS[item.kind].rt;
  const gltf = await parse(asset.model);
  const inst = rt.createCharacterInstance(gltf, asset);
  const holder = new THREE.Group();
  holder.add(inst.root);
  const nodes = [], seen = new Set();
  inst.root.traverse(o => { if (!o.name || o.isMesh || seen.has(o.name) || o.name === 'Scene' || o.name === 'AuxScene') return; seen.add(o.name); nodes.push(o); });
  const morphs = [];
  inst.root.traverse(o => { if (o.isMesh && o.morphTargetDictionary) for (const [k, i] of Object.entries(o.morphTargetDictionary)) morphs.push([o, k, i]); });
  const stride = nodes.length * 9 + morphs.length;
  const samples = [], meta = [];
  let x = 0, z = 0, heading = 0, clock = 0, resets = 0;
  let moving = false, running = false, speech = null, face = undefined, photo = undefined, npc = false;
  const frame = dt => {
    const speed = moving ? (running ? 3.5 : 1) : 0;
    heading += (moving ? 0.6 : 0) * dt; x += Math.sin(heading) * speed * dt; z += Math.cos(heading) * speed * dt;
    holder.position.set(x, 0, z); holder.rotation.set(0, heading, 0);
    clock += dt;
    inst.update(dt, { moving, running, speech, faceControl: face, photoPose: photo, springReset: resets, isNpc: npc, nowSeconds: clock });
    // PackageActor の KeepHeldLevel（CharacterActor の update（優先度 -1）のあと、優先度 0 で毎コマ）
    if (level) { if (!held?.parent) held = inst.root.getObjectByName(level.bone) ?? null; holder.updateMatrixWorld(true); levelBone(held, level.localUp); }
  };
  let held = null;
  for (const sc of SCENES) for (const dtKind of sc.dts ?? ['60']) {
    moving = false; running = false; speech = null; face = undefined; photo = undefined; npc = !!sc.npc;
    for (let i = 0; i < 30; i++) frame(DT);
    const pat = DT_PATTERNS[dtKind], tag = `${sc.name}@${dtKind}`, todo = sc.do.map(c => ({ ...c, done: false }));
    for (let t = 0, k = 0; t < sc.seconds - 1e-9; k++) {
      const dt = pat[k % pat.length];
      for (const c of todo) if (!c.done && t + 1e-9 >= c.at) {
        c.done = true;
        if (c.moving !== undefined) moving = c.moving;
        if (c.running !== undefined) running = c.running;
        if (c.speech !== undefined) speech = c.speech === null ? null : rt.createCharacterSpeech(c.speech, clock);
        if (c.face !== undefined) face = c.face === null ? undefined : FACES[c.face];
        if (c.photo !== undefined) photo = c.photo ?? undefined;
        if (c.teleport) { x = c.teleport[0]; z = c.teleport[1]; }
        if (c.springReset) resets++;
      }
      frame(dt);
      t += dt;
      if (k % EVERY === 0) {
        holder.updateMatrixWorld(true);
        const a = new Float32Array(stride);
        let n = 0;
        for (const o of nodes) { const e = o.matrixWorld.elements; a.set([e[12], e[13], e[14], e[0], e[1], e[2], e[4], e[5], e[6]], n); n += 9; }
        for (const [o, , i] of morphs) a[n++] = o.morphTargetInfluences[i];
        samples.push(a); meta.push([tag, k]);
      }
    }
  }
  const body = Buffer.alloc(samples.length * stride * 4);
  samples.forEach((a, i) => Buffer.from(a.buffer).copy(body, i * stride * 4));
  writeFileSync(path.join(out, `${item.name}.bin`), body);
  writeFileSync(path.join(out, `${item.name}.json`), JSON.stringify({ ...doc, three: THREE.REVISION, nodes: nodes.map(o => o.name), morphs: morphs.map(([o, k]) => [o.name, k]), meta, stride, spring: !!gltf.userData.vrmSpringBoneManager?.joints.size }));
  inst.dispose();
  console.log(`${item.name}: ${samples.length} 点・節点 ${nodes.length}・モーフ ${morphs.length}（${Date.now() - t0}ms）`);
}
// ---- 主役の部品（character-actor）: 写真・話す口・npc を、動作・のけぞり・休む・構えと一緒に ----
const { loadCharacterActor } = await import('@aiarranger/character-actor');
const ACTOR_SCENES = JSON.parse(readFileSync(path.join(process.cwd(), 'runtime-actor-scenes.json'), 'utf8'));
const SHARED = ['biped-hit.glb', 'biped-rest-down.glb', 'biped-rest-loop.glb', 'biped-rest-up.glb'];
const actors = [];
for (const id of ['kai', 'aichan']) {
  const row = packages.characters.find(c => c.manifest.id === id);
  const asset = await KINDS.hero.pkg.readCharacterFile(new Uint8Array(readFileSync(path.join(aisnsDir(), 'public/character-packages', row.package))), row.package);
  // aisns の heroActorSource の同梱の形（登録表なし）
  const files = { full: { path: '', sha256: asset.manifest.model.sha256, bytes: asset.model.byteLength } };
  const data = { full: asset.model };
  for (const f of SHARED) { files['clip:' + f] = { path: '/animations/' + f, sha256: '', bytes: 0 }; data['clip:' + f] = new Uint8Array(readFileSync(path.join(aisnsDir(), 'public/animations', f))); }
  const manifest = { id, revision: 'bundled', name: asset.manifest.name, actor: { heightMeters: asset.manifest.coordinates.heightMeters }, files, packageManifest: asset.manifest };
  const client = { async manifest() { return manifest; }, async prepare(_id, o) { return { manifest, data: Object.fromEntries(o.files.filter(k => data[k]).map(k => [k, data[k]])) }; } };
  const parser = { parseAsync: (buf, p) => parse(new Uint8Array(buf)) };
  for (const npc of [false, true]) {
    const hero = await loadCharacterActor({ client, id, quality: 'full', gait: { walk: 0.15, run: 2.5 }, parser, npc });
    const holder = new THREE.Group();
    holder.add(hero.object);
    const grip = hero.holdWeapon(new THREE.Group(), { length: 0.9 });
    const nodes = [], seen = new Set();
    hero.object.traverse(o => { if (!o.name || o.isMesh || seen.has(o.name) || o === hero.object || o.name === 'wilds-hurt-face' || o.name === 'weapon-grip' || o.name === 'Scene' || o.name === 'AuxScene') return; seen.add(o.name); nodes.push(o); });
    const morphs = [];
    hero.object.traverse(o => { if (o.isMesh && o.morphTargetDictionary && o.name !== 'wilds-hurt-face') for (const [k, i] of Object.entries(o.morphTargetDictionary)) morphs.push([o, k, i]); });
    const stride = nodes.length * 9 + morphs.length + 16;
    const samples = [], meta = [], events = [];
    let f = 0, clock = 0, face = null, speech = null, photo = null;
    const update = (dt, speed, armed, rate) => {
      hero.move({ speed, grounded: true, armed, rate, photo });
      hero.express({ face, speech });
      clock += dt;
      for (const e of hero.update(dt, clock)) events.push([f, e.type, e.action, e.cue ?? null]);
      f++;
    };
    for (const sc of ACTOR_SCENES) for (const dtKind of sc.dts ?? ['60']) {
      face = null; speech = null; photo = null;
      if (hero.current) hero.stop(hero.current);
      for (let i = 0, n = 0; (i < 30 || hero.current) && n < 900; n++) { update(DT, 0, 0, 1); if (!hero.current) i++; }
      const pat = DT_PATTERNS[dtKind], every = sc.every ?? 1, tag = `${sc.name}@${dtKind}`, todo = sc.do.map(c => ({ ...c, done: false }));
      let speed = sc.speed ?? 0, rate = sc.rate ?? 1, armed = sc.armed ?? 0;
      for (let t = 0, k = 0; t < sc.seconds - 1e-9; k++) {
        const dt = pat[k % pat.length];
        for (const c of todo) if (!c.done && t + 1e-9 >= c.at) {
          c.done = true;
          if (c.stop) hero.stop(c.stop);
          if (c.start) hero.start(c.start, c.opts ?? {});
          if (c.hit) hero.hit(c.hit);
          if (c.speech !== undefined) speech = c.speech === null ? null : KINDS.hero.rt.createCharacterSpeech(c.speech, clock);
          if (c.face !== undefined) face = c.face === null ? null : FACES[c.face];
          if (c.photo !== undefined) photo = c.photo;
        }
        update(dt, speed, armed, rate);
        t += dt;
        if (k % every === 0) {
          holder.updateMatrixWorld(true);
          const a = new Float32Array(stride);
          let n = 0;
          for (const o of nodes) { const e = o.matrixWorld.elements; a.set([e[12], e[13], e[14], e[0], e[1], e[2], e[4], e[5], e[6]], n); n += 9; }
          for (const [o, , i] of morphs) a[n++] = o.morphTargetInfluences[i];
          if (grip) a.set(grip.matrixWorld.elements, n); else a.fill(NaN, n, n + 16);
          samples.push(a); meta.push([tag, k]);
        }
      }
    }
    const name = `actor-${id}${npc ? '-npc' : ''}`;
    const body = Buffer.alloc(samples.length * stride * 4);
    samples.forEach((a, i) => Buffer.from(a.buffer).copy(body, i * stride * 4));
    writeFileSync(path.join(out, `${name}.bin`), body);
    const motions = Object.fromEntries(['attack', 'skill', 'hit', 'rest', 'ready'].map(k => [k, hero.query(k).available]));
    writeFileSync(path.join(out, `${name}.json`), JSON.stringify({ release, id, npc, package: row.package, shared: SHARED, manifest, nodes: nodes.map(o => o.name), morphs: morphs.map(([o, k]) => [o.name, k]), meta, events, stride, grip: !!grip, motions }));
    hero.dispose();
    actors.push(name);
    console.log(`${name}: ${samples.length} 点・節点 ${nodes.length}・モーフ ${morphs.length}・合図 ${events.length}・${JSON.stringify(motions)}`);
  }
}
writeFileSync(path.join(out, 'items.json'), JSON.stringify({ release, items: items.map(i => i.name), actors }));
