// TS の部品（three + @aiarranger/character-actor + @pixiv/three-vrm の VRMSpringBoneLoaderPlugin）で、揺れ物の入ったモデル（full）の骨の値を書き出す。Rust の試験 tests/spring_parity.rs が同じ合図・同じ刻みで比べる。
//   node crates/character-actor/tools/spring-parity.mjs <出力のフォルダ> [id...]   （id を省くと配布中の全主役）
// 場面は tests/spring_parity.rs の scenes と同じ（どちらかを変えたら両方を変える）。入れ物を動かす（揺れ物は入れ物の動きにも反応する）。
// 入れ物は 3 段: holder（位置と向き）> stretch > twist。squash は伸び縮みするだけ（keiba の lean の形）、ball は回って伸び縮みする（pinball の玉の形）、shear は回った子を伸び縮みする親の下に置く（行列が回転と大きさに分けられなくなる）。
// テクスチャは読まない。lite・normal はモデルの中の揺れ物の数だけ書く（配布の lite・normal には入っていない）。
import { execFileSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, mkdtempSync, realpathSync, rmSync, symlinkSync, writeFileSync, readFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = fileURLToPath(import.meta.url);
if (!process.env.ACTOR_PARITY_INNER) {
  // 利用者の形のフォルダを作って、その中でこのファイルを動かす（actor-parity.mjs と同じ）
  const repo = path.resolve(path.dirname(here), '../../..');
  const release = JSON.parse(execFileSync(process.execPath, [path.join(path.dirname(here), 'actor-parity.mjs'), '--release-info'], { encoding: 'utf8' }));
  // three はこのリポジトリの 0.178（aisns と同じ。full を読むのは aisns だけ）。0.186 の updateWorldMatrix は matrixWorldNeedsUpdate を見るので、character-runtime の transport の行列が世界の行列に入らず、揺れ方が変わる
  const three = process.env.ACTOR_PARITY_THREE ?? realpathSync(path.join(repo, 'node_modules/three'));
  const dir = mkdtempSync(path.join(os.tmpdir(), 'spring-parity-')), nm = path.join(dir, 'node_modules');
  mkdirSync(path.join(nm, '@aiarranger'), { recursive: true });
  symlinkSync(three, path.join(nm, 'three'));
  cpSync(path.dirname(realpathSync(path.join(repo, 'node_modules/@pixiv/three-vrm'))), path.join(nm, '@pixiv'), { recursive: true, dereference: true });
  for (const { name, version } of release.packages) {
    const to = path.join(nm, '@aiarranger', name);
    mkdirSync(to, { recursive: true });
    execFileSync('tar', ['-xzf', path.join(repo, 'packages', name, 'releases', `aiarranger-${name}-${version}.tgz`), '--strip-components=1', '-C', to]);
  }
  cpSync(path.join(repo, 'packages/character-delivery/client.mjs'), path.join(dir, 'delivery-client.mjs'));
  cpSync(here, path.join(dir, 'run.mjs'));
  try {
    execFileSync(process.execPath, ['run.mjs', ...process.argv.slice(2).map((a, i) => (i === 0 ? path.resolve(a) : a))], { cwd: dir, stdio: 'inherit', env: { ...process.env, ACTOR_PARITY_INNER: '1', ACTOR_PARITY_RELEASE: JSON.stringify(release) } });
  } finally { rmSync(dir, { recursive: true, force: true }); }
  process.exit(0);
}

const release = JSON.parse(process.env.ACTOR_PARITY_RELEASE);

const THREE = await import('three');
const { GLTFLoader } = await import('three/examples/jsm/loaders/GLTFLoader.js');
const { MeshoptDecoder } = await import('three/examples/jsm/libs/meshopt_decoder.module.js');
const { VRMSpringBoneLoaderPlugin } = await import('@pixiv/three-vrm');
const { loadCharacterActor } = await import('@aiarranger/character-actor');
const { createDeliveryClient } = await import(path.join(process.cwd(), 'delivery-client.mjs'));

const noop = () => {};
const ctx = new Proxy({}, { get: (_, k) => (k === 'createRadialGradient' || k === 'createLinearGradient' ? () => ({ addColorStop: noop }) : k === 'getImageData' ? () => null : noop), set: () => true });
globalThis.document = { createElement: () => ({ getContext: () => ctx, width: 0, height: 0 }) };
await MeshoptDecoder.ready;
const noTexture = name => () => ({ name, loadTexture: () => Promise.resolve(null) });
/** 揺れ物を読む読み手。最初に読んだ物（モデル）の揺れ物の入れ物を manager に控える */
const makeParser = spring => {
  const seen = { manager: undefined };
  const l = new GLTFLoader().setMeshoptDecoder(MeshoptDecoder).register(noTexture('EXT_texture_webp')).register(noTexture('EXT_texture_avif')).register(noTexture('NO_TEXTURES_IN_NODE'));
  if (spring) l.register(p => new VRMSpringBoneLoaderPlugin(p));
  return { seen, parseAsync: async (data, p) => { const g = await l.parseAsync(data, p); if (seen.manager === undefined) seen.manager = g.userData.vrmSpringBoneManager ?? null; return g; } };
};

const [out, ...only] = process.argv.slice(2);
const INDEX_URL = process.env.CHARACTER_INDEX_URL ?? 'https://characters.aiarranger.jp/channels/stable/v1/index.json?channel=stable';
const dir = path.join(os.tmpdir(), 'character-actor-delivery-cache');
mkdirSync(dir, { recursive: true });
const key = url => path.join(dir, new URL(url).searchParams.get('v') ?? encodeURIComponent(url));
const client = createDeliveryClient({ indexUrl: INDEX_URL, timeoutMs: 120000, cache: {
  async get(url) { const f = key(url); return existsSync(f) ? new Response(readFileSync(f)) : undefined; },
  async put(url, bytes) { writeFileSync(key(url), bytes); },
} });
const index = await (await fetch(INDEX_URL, { cache: 'no-store' })).json();
const ids = only.length ? only : Object.keys(index.characters);
mkdirSync(out, { recursive: true });

// ---- 場面（tests/spring_parity.rs の scenes と同じ） ----
const DT = 1 / 60, RUN = 4, RATE = 1.3, EVERY = 3;
const HICCUP = [1 / 60, 1 / 60, 0.2, 0.001, 0, 1 / 30];
for (const id of ids) {
  const t0 = Date.now();
  const counts = {};
  for (const quality of ['lite', 'normal']) {
    const parser = makeParser(true);
    const h = await loadCharacterActor({ client, id, quality, gait: { walk: 0.15, run: 2.5 }, parser });
    counts[quality] = parser.seen.manager?.joints.size ?? 0;
    h.dispose();
  }
  const parser = makeParser(true);
  const hero = await loadCharacterActor({ client, id, quality: 'full', gait: { walk: 0.15, run: 2.5 }, parser });
  const manager = parser.seen.manager;
  counts.full = manager?.joints.size ?? 0;
  const obj = hero.object, holder = new THREE.Group(), stretch = new THREE.Group(), twist = new THREE.Group();
  holder.add(stretch); stretch.add(twist); twist.add(obj);
  const grip = hero.holdWeapon(new THREE.Group(), { length: 0.9 });
  const nodes = [], seen = new Set();
  obj.traverse(o => {
    if (!o.name || o.isMesh || seen.has(o.name) || o === obj || o.name === 'wilds-hurt-face' || o.name === 'weapon-grip' || o.name === 'Scene' || o.name === 'AuxScene') return;
    seen.add(o.name); nodes.push(o);
  });
  const sorted = manager ? (manager._sortJoints(), manager._sortedJoints.map(j => j.bone.name)) : [];
  const jointBones = manager ? manager._sortedJoints.map(j => [j.bone, j.bone.quaternion.clone()]) : [];
  const stride = nodes.length * 9;
  const samples = [], meta = [], angle = {};
  let x = 0, z = 0, heading = 0, clock = 0, f = 0, pose = null, poseAt = 0;
  // 入れ物の形（tests/spring_parity.rs の pose_matrix と同じ）。i は形を替えてからのコマ
  const setPose = () => {
    stretch.position.set(0, 0, 0); stretch.rotation.set(0, 0, 0); stretch.scale.set(1, 1, 1);
    twist.position.set(0, 0, 0); twist.rotation.set(0, 0, 0); twist.scale.set(1, 1, 1);
    if (!pose) return;
    const i = f - poseAt;
    if (pose === 'ball') {
      const tk = Math.min(1, i / 36), k = Math.max(0, 1 - i / 42), big = 1 + 0.45 * k * (2 - k), q = 0.12 * Math.sin(i * 0.4) * Math.exp(-i * 0.02), st = 0.08;
      stretch.rotation.set(0, Math.sin(i * DT * 0.8) * 0.25, -0.2 + (i < 36 ? Math.PI * 2 * (1 - (1 - tk) ** 3) : 0));
      stretch.scale.set(big * (1 - st * 0.6 - q * 0.5), big * (1 + st + q), big * (1 - st * 0.6 + q * 0.3));
      twist.position.set(0, -0.7, 0); twist.scale.setScalar(1.44);
    } else {
      const q = 0.2 * Math.sin(i * 0.25);
      stretch.scale.set(1 - q * 0.5, 1 + q, 1 - q * 0.5);
      if (pose === 'shear') twist.rotation.set(0, i * 0.1, 0.3 * Math.sin(i * 0.1));
    }
  };
  const step = (scene, dt, speed, armed, rate, turn) => {
    heading += turn * dt; x += Math.sin(heading) * speed * dt; z += Math.cos(heading) * speed * dt;
    holder.position.set(x, 0, z); holder.rotation.y = heading;
    setPose();
    hero.move({ speed, armed, rate });
    hero.update(dt, clock);
    clock += dt;
    let most = angle[scene] ?? 0;
    for (const [b, q] of jointBones) most = Math.max(most, b.quaternion.angleTo(q));
    angle[scene] = most;
    if (f++ % EVERY === 0) {
      holder.updateMatrixWorld(true);
      const a = new Float32Array(stride);
      let n = 0;
      for (const o of nodes) { const e = o.matrixWorld.elements; a.set([e[12], e[13], e[14], e[0], e[1], e[2], e[4], e[5], e[6]], n); n += 9; }
      samples.push(a); meta.push([scene, f - 1]);
    }
  };
  const has = k => hero.query(k).available;
  const run = (scene, n, turn = 0) => { for (let i = 0; i < n; i++) step(scene, DT, RUN, 0, RATE, turn); };
  const stop = (scene, n, armed = 0) => { for (let i = 0; i < n; i++) step(scene, DT, 0, armed, 1, 0); };
  stop('idle', 60);
  run('run', 90); run('run', 60, 1.5);
  stop('stop', 60);
  run('run-attack', 20); if (has('attack')) hero.start('attack'); run('run-attack', 70, 0.5);
  stop('stop-attack', 20, 1); if (has('attack')) hero.start('attack'); stop('stop-attack', 70, 1);
  stop('stop-attack', 20);
  run('run-hit', 20); if (has('hit')) hero.hit(); run('run-hit', 50, -0.5);
  stop('stop-hit', 20); if (has('hit')) hero.hit(); stop('stop-hit', 50);
  for (let i = 0; i < 30; i++) step('30hz', 1 / 30, RUN, 0, RATE, 0.7);
  for (let i = 0; i < 30; i++) step('30hz', 1 / 30, 0, 0, 1, 0);
  for (let i = 0; i < 144; i++) step('144hz', 1 / 144, RUN, 0, RATE, -0.7);
  for (let i = 0; i < 144; i++) step('144hz', 1 / 144, 0, 0, 1, 0);
  for (let i = 0; i < 60; i++) step('hiccup', HICCUP[i % 6], RUN, 0, RATE, 0);
  stop('hiccup', 30);
  // 伸び縮みする入れ物
  pose = 'squash'; poseAt = f;
  run('squash', 60, 0.5); stop('squash', 60);
  pose = 'ball'; poseAt = f;
  if (has('hit')) hero.hit();
  stop('ball', 90);
  pose = 'shear'; poseAt = f;
  run('shear', 60, 0.5); stop('shear', 60);
  pose = null;
  stop('shear', 20);
  // 遠くへ飛ぶ（広いゲームの座標。入れ物の移動は 1 コマの間に分けて揺らす）
  x += 150; z -= 80;
  run('far', 60, 0.4); stop('far', 60);

  const body = Buffer.alloc(samples.length * stride * 4);
  samples.forEach((a, i) => Buffer.from(a.buffer).copy(body, i * stride * 4));
  writeFileSync(path.join(out, `${id}.bin`), body);
  writeFileSync(path.join(out, `${id}.json`), JSON.stringify({ id, revision: hero.revision, three: THREE.REVISION, release, counts, colliders: manager?.colliders.length ?? 0, sorted, nodes: nodes.map(o => o.name), meta, stride, angle }));
  console.log(`${id}: ${samples.length} 点・節点 ${nodes.length}・揺れる骨 ${JSON.stringify(counts)}・${Date.now() - t0}ms（three ${THREE.REVISION}）`);
  hero.dispose();
}
