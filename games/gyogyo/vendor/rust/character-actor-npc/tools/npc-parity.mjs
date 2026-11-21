// NPC・汎用キャラ（凍結: character-runtime 0.2.5 の写し）の読み方と動きを、aisns の vendor の配布済みの部品で書き出す。Rust の試験 tests/npc_parity.rs が同じ入力・同じ刻みで比べる。
//   node crates/character-actor-npc/tools/npc-parity.mjs <出力のフォルダ>
// 部品（SHA を照合した tgz）: NPC = aisns の vendor/character-runtime-npc 0.2.5、汎用 = vendor/character-runtime-generic 0.2.5（中身は同じ写し）。主役の部品は読まない。
// 入力（aisns のフォルダ。RUNTIME_PARITY_AISNS で替えられる）: lib/welcome-npcs.json と public/models/welcome の NPC 16 人、public/character-packages の汎用の zip。
//   主役の zip と持ち込みの見本の .glb は、汎用の読み手で読んだ結果（読めるか・文言）だけ比べる。
// 動き: createCharacterInstance の update（NPC の CharacterActor・汎用の PackageActor と同じ入力）を tools/runtime-scenes.json の場面で回す。three はこのリポジトリの 0.178。テクスチャは読まない。
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

if (!process.env.NPC_PARITY_INNER) {
  const packages = [
    { name: 'character-runtime-npc', ...checked(path.join(aisns, 'vendor/character-runtime-npc'), 'aiarranger-character-runtime-npc-0.2.5.tgz') },
    { name: 'character-runtime-generic', ...checked(path.join(aisns, 'vendor/character-runtime-generic'), 'aiarranger-character-runtime-generic-0.2.5.tgz') },
  ];
  const scenes = readFileSync(path.join(path.dirname(here), 'runtime-scenes.json'));
  const release = { packages: packages.map(({ name, sha256 }) => ({ name, sha256 })), tool: sha(readFileSync(here)), scenes: sha(scenes) };
  release.key = sha(JSON.stringify(release));
  if (process.argv[2] === '--release-info') { console.log(JSON.stringify(release)); process.exit(0); }
  const three = process.env.RUNTIME_PARITY_THREE ?? realpathSync(path.join(repo, 'node_modules/three'));
  const dir = mkdtempSync(path.join(os.tmpdir(), 'npc-parity-')), nm = path.join(dir, 'node_modules');
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
  try {
    execFileSync(process.execPath, ['run.mjs', path.resolve(process.argv[2])], { cwd: dir, stdio: 'inherit', env: { ...process.env, NPC_PARITY_INNER: '1', NPC_PARITY_RELEASE: JSON.stringify(release), RUNTIME_PARITY_AISNS: aisns, RUNTIME_PARITY_REPO: repo } });
  } finally { rmSync(dir, { recursive: true, force: true }); }
  process.exit(0);
}

const release = JSON.parse(process.env.NPC_PARITY_RELEASE), repoDir = process.env.RUNTIME_PARITY_REPO;
const THREE = await import('three');
const { GLTFLoader } = await import('three/examples/jsm/loaders/GLTFLoader.js');
const { MeshoptDecoder } = await import('three/examples/jsm/libs/meshopt_decoder.module.js');
const { VRMSpringBoneLoaderPlugin } = await import('@pixiv/three-vrm');
const KINDS = {
  npc: { pkg: await import('@aiarranger/character-runtime-npc/package'), rt: await import('@aiarranger/character-runtime-npc/runtime') },
  generic: { pkg: await import('@aiarranger/character-runtime-generic/package'), rt: await import('@aiarranger/character-runtime-generic/runtime') },
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
for (const npc of npcCatalog) items.push({ name: `npc-${npc.id}`, kind: 'npc', file: path.join(aisnsDir(), 'public', npc.model), filename: npc.model, override: { id: npc.id, name: npc.name, heightMeters: npc.heightMeters, neutralMouthClose: npc.neutralMouthClose } });
const packages = JSON.parse(readFileSync(path.join(aisnsDir(), 'public/character-packages/catalog.json'), 'utf8'));
for (const row of packages.characters) {
  // 汎用は動きまで、主役の zip は汎用の読み手で読んだ結果だけ
  items.push({ name: `pkg-${row.manifest.id}`, kind: 'generic', readOnly: row.kind !== 'generic', file: path.join(aisnsDir(), 'public/character-packages', row.package), filename: row.package });
}
// manifest なしの .glb（汎用の読み手で読んだ結果だけ）
for (const glb of ['kiyogon-cbf0be289a1f.glb', 'generic-cat-4392298cc474.glb']) items.push({ name: `import-${glb.replace(/\.glb$/, '')}`, kind: 'generic', readOnly: true, file: path.join(aisnsDir(), 'public/character-packages', glb), filename: `C:\\Users\\me\\Downloads\\${glb.toUpperCase()}` });

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
for (const [name, bytes, filename] of brokenInputs()) for (const kind of ['generic']) errors.push({ name, kind, filename, ...(await tryRead(kind, bytes, filename)) });
writeFileSync(path.join(out, 'errors.json'), JSON.stringify({ release, errors }));

// ---- 動き ----
for (const item of items) {
  const t0 = Date.now();
  const bytes = new Uint8Array(readFileSync(item.file));
  const read = await tryRead(item.kind, bytes, item.filename);
  const doc = { release, name: item.name, kind: item.kind, file: path.relative(aisnsDir(), item.file), filename: item.filename, read, readOnly: !!item.readOnly, override: item.override ?? null };
  if (!read.ok || item.readOnly) { writeFileSync(path.join(out, `${item.name}.json`), JSON.stringify(doc)); console.log(`${item.name}: ${read.ok ? '読んだだけ' : '読めない ' + read.error}`); continue; }
  const asset = await KINDS[item.kind].pkg.readCharacterFile(bytes, item.filename);
  if (item.override) {
    const o = item.override;
    asset.manifest.id = o.id; asset.manifest.name = o.name; asset.manifest.coordinates.heightMeters = o.heightMeters; asset.manifest.motion.neutralMouthClose = o.neutralMouthClose;
  }
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
  };
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
writeFileSync(path.join(out, 'items.json'), JSON.stringify({ release, items: items.map(i => i.name) }));
