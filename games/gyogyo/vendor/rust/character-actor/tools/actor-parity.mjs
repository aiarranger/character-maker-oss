// TS の配布済み部品（three + SHA 照合済みの release tgz）で主役の骨の値を書き出す。Rust の試験 tests/ts_parity.rs が同じ合図・同じ刻みで比べる。
//   [ACTOR_PARITY_THREE=<three のフォルダ>] node crates/character-actor/tools/actor-parity.mjs <出力のフォルダ> [id...]   （id を省くと配布中の全主役）
// 場面: 動作の表と HIT_SCENES は tests/ts_parity.rs と同じ（どちらかを変えたら両方を変える）。ゲームの入り方の場面は tools/game-scenes.json を両方が読み、入れ物の式（ballFrame・friendFrame）は両方に書く。
// three の版は試験が ACTOR_PARITY_THREE で渡す（ゲームが使う 0.178 と 0.186 の両方）。省くとこのリポジトリの 0.178。
// テクスチャは読まない（node に無い。骨・クリップ・モーフは同じ）。揺れ物は読まない（normal・lite のモデルに入っていない。full の揺れ物は spring-parity.mjs が比べる）。
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { cpSync, existsSync, mkdirSync, mkdtempSync, realpathSync, rmSync, symlinkSync, writeFileSync, readFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = fileURLToPath(import.meta.url);
if (!process.env.ACTOR_PARITY_INNER) {
  // 利用者の形のフォルダを作って、その中でこのファイルを動かす（packages/character-actor/scripts/test.mjs と同じ。部品は利用側の three を読む）
  const repo = path.resolve(path.dirname(here), '../../..');
  // package.json は配布版を選ぶためだけに読む。実行する package.json / dist は両方 tgz から取り出す。
  const releases = ['character-actor', 'character-runtime', 'character-motion'].map(name => {
    const from = path.join(repo, 'packages', name);
    const version = JSON.parse(readFileSync(path.join(from, 'package.json'), 'utf8')).version;
    const file = `aiarranger-${name}-${version}.tgz`, tarball = path.join(from, 'releases', file);
    const sha256 = createHash('sha256').update(readFileSync(tarball)).digest('hex');
    const sums = readFileSync(path.join(from, 'releases/SHA256SUMS'), 'utf8');
    const listed = sums.split('\n').find(line => line.endsWith(`  ${file}`))?.slice(0, 64);
    if (listed !== sha256) throw Error(`${tarball}: SHA256SUMS does not match the released tgz`);
    const packed = JSON.parse(execFileSync('tar', ['-xOf', tarball, 'package/package.json'], { encoding: 'utf8' }));
    if (packed.name !== `@aiarranger/${name}` || packed.version !== version) throw Error(`${tarball}: packaged name/version does not match`);
    return { name, version, sha256 };
  });
  const release = { packages: releases, key: createHash('sha256').update(JSON.stringify(releases)).digest('hex') };
  // Rust のキャッシュ判定と spring-parity も、同じ配布 SHA を入口にする（素材の通信や値の生成はしない）。
  if (process.argv[2] === '--release-info') { console.log(JSON.stringify(release)); process.exit(0); }
  const three = process.env.ACTOR_PARITY_THREE ?? realpathSync(path.join(repo, 'node_modules/three'));
  const dir = mkdtempSync(path.join(os.tmpdir(), 'actor-parity-')), nm = path.join(dir, 'node_modules');
  mkdirSync(path.join(nm, '@aiarranger'), { recursive: true });
  symlinkSync(three, path.join(nm, 'three'));
  cpSync(path.dirname(realpathSync(path.join(repo, 'node_modules/@pixiv/three-vrm'))), path.join(nm, '@pixiv'), { recursive: true, dereference: true });
  for (const { name, version } of releases) {
    const to = path.join(nm, '@aiarranger', name);
    mkdirSync(to, { recursive: true });
    execFileSync('tar', ['-xzf', path.join(repo, 'packages', name, 'releases', `aiarranger-${name}-${version}.tgz`), '--strip-components=1', '-C', to]);
  }
  cpSync(path.join(repo, 'packages/character-delivery/client.mjs'), path.join(dir, 'delivery-client.mjs'));
  cpSync(here, path.join(dir, 'run.mjs'));
  cpSync(path.join(path.dirname(here), 'game-scenes.json'), path.join(dir, 'game-scenes.json'));
  try {
    execFileSync(process.execPath, ['run.mjs', ...process.argv.slice(2).map((a, i) => (i === 0 ? path.resolve(a) : a))], { cwd: dir, stdio: 'inherit', env: { ...process.env, ACTOR_PARITY_INNER: '1', ACTOR_PARITY_RELEASE: JSON.stringify(release) } });
  } finally { rmSync(dir, { recursive: true, force: true }); }
  process.exit(0);
}

const release = JSON.parse(process.env.ACTOR_PARITY_RELEASE);

const THREE = await import('three');
const { GLTFLoader } = await import('three/examples/jsm/loaders/GLTFLoader.js');
const { MeshoptDecoder } = await import('three/examples/jsm/libs/meshopt_decoder.module.js');
const { loadCharacterActor } = await import('@aiarranger/character-actor');
const { createDeliveryClient } = await import(path.join(process.cwd(), 'delivery-client.mjs'));

const noop = () => {};
const ctx = new Proxy({}, { get: (_, k) => (k === 'createRadialGradient' || k === 'createLinearGradient' ? () => ({ addColorStop: noop }) : k === 'getImageData' ? () => null : noop), set: () => true });
globalThis.document = { createElement: () => ({ getContext: () => ctx, width: 0, height: 0 }) };
await MeshoptDecoder.ready;
const noTexture = name => () => ({ name, loadTexture: () => Promise.resolve(null) });
const parser = new GLTFLoader().setMeshoptDecoder(MeshoptDecoder).register(noTexture('EXT_texture_webp')).register(noTexture('EXT_texture_avif')).register(noTexture('NO_TEXTURES_IN_NODE'));

const [out, ...only] = process.argv.slice(2);
const INDEX_URL = process.env.CHARACTER_INDEX_URL ?? 'https://characters.aiarranger.jp/channels/stable/v1/index.json?channel=stable';
const dir = path.join(os.tmpdir(), 'character-actor-delivery-cache');
mkdirSync(dir, { recursive: true });
const key = url => path.join(dir, new URL(url).searchParams.get('v') ?? encodeURIComponent(url));
const client = createDeliveryClient({ indexUrl: INDEX_URL, timeoutMs: 60000, cache: {
  async get(url) { const f = key(url); return existsSync(f) ? new Response(readFileSync(f)) : undefined; },
  async put(url, bytes) { writeFileSync(key(url), bytes); },
} });
const index = await (await fetch(INDEX_URL, { cache: 'no-store' })).json();
const ids = only.length ? only : Object.keys(index.characters);
mkdirSync(out, { recursive: true });

// ---- 場面（tests/ts_parity.rs の SCENES と同じ） ----
const DT = 1 / 60, RUN = 4, RATE = 1.3, LEAD = 40, TAIL = 40, EVERY = 3;
const CONDITIONS = ['idle', 'run', 'stop'];
// swing は Engimeow の壁を壊す振り（攻撃を SWING_SECONDS で）
const ACTIONS = ['attack', 'swing', 'hit', 'dodge', 'skill', 'banzai', 'greet'], SWING_SECONDS = 0.75;
// のけぞりの入り方（tests/ts_parity.rs の HIT_SCENES と同じ）。どのコマも比べる。
// before: 先に始める動き。lead: のけぞるまでのコマ。grounded: false は宙（のけぞって 30 コマで地面へ戻る）。holder: 入れ物の動き。seconds・since: hit の指定（0 は省く）。again: もう一度のけぞるまでのコマ（0 は無し）
const hitScene = (name, before, lead, speed, grounded, armed, holder, seconds = 0, since = 0, again = 0) => ({ name, before, lead, speed, grounded, armed, holder, seconds, since, again });
const HIT_SCENES = [
  hitScene('ready', null, 40, 0, true, 1, 'still'),
  hitScene('swing', 'swing', 20, 0, true, 1, 'still'),
  hitScene('swing-run', 'swing', 20, RUN, true, 0, 'still'),
  hitScene('jump', 'jump', 15, 0, false, 0, 'still'),
  hitScene('air', null, 30, 0, false, 0, 'still'),
  hitScene('dodge', 'dodge', 10, 0, true, 0, 'still'),
  hitScene('skill', 'skill', 20, 0, true, 1, 'still'),
  hitScene('banzai', 'banzai', 20, 0, true, 0, 'still'),
  hitScene('rest', 'rest', 60, 0, true, 0, 'still'),
  hitScene('short', null, 20, RUN, true, 0, 'still', 0.5),
  hitScene('since', null, 20, 0, true, 0, 'still', 0, 0.2),
  hitScene('again', null, 20, 0, true, 0, 'still', 0, 0, 12),
  hitScene('turn', null, 20, 0, false, 0, 'turn'),
  hitScene('squash', null, 20, 0, false, 0, 'squash'),
  hitScene('tumble', null, 20, 0, false, 0, 'tumble'),
  hitScene('tumble-stand', null, 20, 0, true, 0, 'tumble'),
  hitScene('tumble-swing', 'swing', 20, 0, true, 1, 'tumble'),
];
// 入れ物の動き（pinball の玉の入れ物 cast.ts の形）: 回るだけ（turn）・伸び縮みするだけ（squash）・両方（tumble）。i は場面の始めからのコマ、hit はのけぞったコマ（まだなら -1）
const TUMBLE = 36;
const holderPose = (kind, i, hit) => {
  if (kind === 'still') return { p: [0, 0, 0], ry: 0, rz: 0, s: [1, 1, 1] };
  const j = hit < 0 ? 0 : i - hit, tk = Math.min(1, j / TUMBLE), p = [0.5, 1.25, -0.75];
  const rz = -0.2 + (hit >= 0 && j < TUMBLE ? Math.PI * 2 * (1 - (1 - tk) ** 3) : 0), ry = Math.sin(i * DT * 0.8) * 0.25;
  const q = 0.12 * Math.sin(i * 0.4) * Math.exp(-i * 0.02), st = 0.08, k = hit < 0 ? 0 : Math.max(0, 1 - j / 42), big = 1 + 0.45 * k * (2 - k);
  const s = [big * (1 - st * 0.6 - q * 0.5), big * (1 + st + q), big * (1 - st * 0.6 + q * 0.3)];
  return kind === 'turn' ? { p, ry, rz, s: [1, 1, 1] } : kind === 'squash' ? { p, ry: 0, rz: 0, s } : { p, ry, rz, s };
};
/** 動作の長さ（フレーム）。部品の既定の長さ。無い動きは試さない */
const framesOf = (hero, action) => {
  const q = hero.query(action === 'swing' ? 'attack' : action);
  if (!q.available) return null;
  return Math.ceil((action === 'swing' ? SWING_SECONDS : q.seconds ?? 0.6) / DT);
};

// ---- ゲームの入り方の場面（tools/game-scenes.json） ----
// 秒で書く。at の時刻を過ぎた最初のコマの update の前に合図を出す（stopAfter は update の後。muso・ひな形の「止める」）。
// 入れ物は pinball の玉（holder > inner > charBox。cast.ts の createBallView）と仲間（holder > lean。createFriend）の式
const GAME_SCENES = JSON.parse(readFileSync(path.join(process.cwd(), 'game-scenes.json'), 'utf8'));
const DT_PATTERNS = { 60: [1 / 60], 120: [1 / 120], 144: [1 / 144], uneven: [1 / 60, 1 / 60, 0.2, 0.001, 0, 1 / 30, 1 / 144, 1 / 24, 0.05, 0.051] };
const DT_EVERY = { 60: 1, 120: 2, 144: 3, uneven: 1 };
const face = (emotion, w, mouth = 'neutral', look = 'neutral') => ({ emotion, emotionWeight: w, mouth, mouthWeight: mouth === 'neutral' ? 0 : w * 0.8, look, lookWeight: look === 'neutral' ? 0 : 0.8 });
const FACES = { up: face('neutral', 0, 'neutral', 'lookUp'), surprised: face('surprised', 0.9, 'oh'), cheer: face('happy', 0.85, 'ih'), sad: face('sad', 0.8, 'oh') };
const SCALE = 1.8, IN_BALL = 0.8, TUMBLE_S = 0.6, FRIEND_SCALE = SCALE * 1.3, WIND = 0.3;
const clamp = (v, a, b) => Math.max(a, Math.min(b, v));
const newBall = standing => ({ x: 0, y: 1, tilt: 0, squash: 0, squashV: 0, tumbleT: 0, tumbleDir: 1, slapK: 0, standing, faceT: 0, faceNow: FACES.up });
const newFriend = () => ({ pos: [-3.2, 0.5, 0], from: [0, 0, 0], target: [0, 0, 0], jumping: 0, dur: 1, hop: 0, hopV: 0, sq: 0, sqV: 0, leanY: 0, leanZ: 0, swingT: 9, faceT: 0, faceNow: null });
const ballFrame = (st, dt, t, g) => {
  const vx = 6 * Math.sin(t * 2), vy = 8 * Math.cos(t * 1.5);
  st.x += vx * dt; st.y += vy * dt;
  g.holder.position.set(st.x, st.y, 0); g.holder.rotation.set(0, 0, 0); g.holder.scale.set(1, 1, 1);
  st.tilt += (clamp(-vx * 0.025 - WIND * 0.25, -0.6, 0.6) - st.tilt) * Math.min(1, dt * 8);
  st.tumbleT = Math.max(0, st.tumbleT - dt);
  const tk = 1 - st.tumbleT / TUMBLE_S;
  g.mid.rotation.set(0, Math.sin(t * 0.8) * 0.25, st.tilt + (st.tumbleT > 0 ? st.tumbleDir * Math.PI * 2 * (1 - (1 - tk) ** 3) : 0));
  st.squashV += (-st.squash * 260 - st.squashV * 16) * dt;
  st.squash += st.squashV * dt;
  const stretch = Math.min(0.14, Math.hypot(vx, vy) * 0.004), wob = Math.sin(t * 19) * 0.07 * WIND;
  st.slapK = Math.max(0, st.slapK - dt / 0.7);
  const big = 1 + 0.45 * st.slapK * (2 - st.slapK);
  g.mid.position.set(0, 0, 0);
  g.mid.scale.set(big * (1 - stretch * 0.6 - st.squash * 0.5 + wob), big * (1 + stretch + st.squash - wob), big * (1 - stretch * 0.6 + st.squash * 0.3));
  g.box.scale.setScalar(SCALE * IN_BALL);
  g.box.position.set(0, -g.height * SCALE * IN_BALL * 0.5, 0);
  st.faceT -= dt;
  return { grounded: st.standing, face: st.faceT > 0 ? st.faceNow : FACES.up };
};
const friendFrame = (st, dt, t, g) => {
  st.swingT += dt;
  let grounded = true;
  if (st.jumping > 0) {
    st.jumping = Math.max(0, st.jumping - dt);
    const k = 1 - st.jumping / st.dur, e = k * k * (3 - 2 * k);
    st.pos = st.from.map((f, i) => f + (st.target[i] - f) * e);
    st.pos[1] += Math.sin(k * Math.PI) * 3.5;
    grounded = false;
  }
  st.hopV -= 30 * dt; st.hop += st.hopV * dt;
  if (st.hop < 0) { st.hop = 0; st.hopV = 0; }
  st.sqV += (-st.sq * 240 - st.sqV * 14) * dt; st.sq += st.sqV * dt;
  st.leanZ *= Math.max(0, 1 - dt * 6);
  st.leanY += (clamp((2.5 * Math.sin(t) - st.pos[0]) * 0.12, -0.6, 0.6) - st.leanY) * Math.min(1, dt * 4);
  g.holder.position.set(st.pos[0], st.pos[1], st.pos[2]); g.holder.rotation.set(0, 0.4, 0); g.holder.scale.setScalar(FRIEND_SCALE);
  g.mid.position.set(0, st.hop / SCALE, 0); g.mid.rotation.set(0, st.leanY, st.leanZ); g.mid.scale.set(1 - st.sq * 0.3, 1 + st.sq * 0.5, 1 - st.sq * 0.3);
  g.box.position.set(0, 0, 0); g.box.scale.set(1, 1, 1);
  st.faceT -= dt;
  return { grounded, face: st.faceT > 0 ? st.faceNow : null };
};

/** 主役を比べる形に置く（入れ物は 3 段。使わない段はそのまま）。書き出すのは節点の位置と向き・モーフ・持ち手 */
const rigOf = hero => {
  const obj = hero.object, holder = new THREE.Group(), mid = new THREE.Group(), box = new THREE.Group();
  holder.add(mid); mid.add(box); box.add(obj);
  const grip = hero.holdWeapon(new THREE.Group(), { length: 0.9 });
  const nodes = [], seen = new Set();
  obj.traverse(o => {
    if (!o.name || o.isMesh || seen.has(o.name) || o === obj || o.name === 'wilds-hurt-face' || o.name === 'weapon-grip' || o.name === 'Scene' || o.name === 'AuxScene') return;
    seen.add(o.name); nodes.push(o);
  });
  const morphs = [];
  obj.traverse(o => { if (o.isMesh && o.morphTargetDictionary && o.name !== 'wilds-hurt-face') for (const [k, i] of Object.entries(o.morphTargetDictionary)) morphs.push([o, k, i]); });
  const motions = {};
  for (const k of ['attack', 'skill', 'dodge', 'jump', 'hit', 'rest', 'banzai', 'greet', 'ready', 'run']) { const q = hero.query(k); motions[k] = { available: q.available, source: q.source, seconds: q.seconds, travel: q.travel ?? null, height: q.height ?? null, face: q.face ?? null }; }
  const stride = nodes.length * 9 + morphs.length + 16;
  const samples = [], events = [], meta = [];
  const sample = (tag, frame) => {
    holder.updateMatrixWorld(true);
    const a = new Float32Array(stride);
    let n = 0;
    for (const o of nodes) { const e = o.matrixWorld.elements; a.set([e[12], e[13], e[14], e[0], e[1], e[2], e[4], e[5], e[6]], n); n += 9; }
    for (const [o, , i] of morphs) a[n++] = o.morphTargetInfluences[i];
    if (grip) a.set(grip.matrixWorld.elements, n); else a.fill(NaN, n, n + 16);
    samples.push(a); meta.push([...tag, frame]);
  };
  const save = (name, id) => {
    const body = Buffer.alloc(samples.length * stride * 4);
    samples.forEach((a, i) => Buffer.from(a.buffer).copy(body, i * stride * 4));
    writeFileSync(path.join(out, `${name}.bin`), body);
    writeFileSync(path.join(out, `${name}.json`), JSON.stringify({ id, revision: hero.revision, quality: hero.quality, three: THREE.REVISION, release, nodes: nodes.map(o => o.name), morphs: morphs.map(([o, k]) => [o.name, k]), motions, meta, events, stride, grip: !!grip }));
    console.log(`${name}: ${samples.length} 点・節点 ${nodes.length}・モーフ ${morphs.length}（three ${THREE.REVISION}）`);
  };
  return { holder, mid, box, height: hero.heightMeters, sample, save, events };
};

/** ゲームの入り方の場面を全部（lite。ゲームが読む品質） */
const runGameScenes = (hero, g) => {
  let f = 0;
  const update = (dt, now, speed, grounded, armed, rate) => {
    hero.move({ speed, grounded, armed, rate });
    for (const e of hero.update(dt, now)) g.events.push([f, e.type, e.action, e.cue ?? null]);
    f++;
  };
  for (const sc of GAME_SCENES) for (const dtKind of sc.dts ?? [60]) {
    // 前の場面を片付ける（休むは立ち終わるまで）
    for (const o of [g.holder, g.mid, g.box]) { o.position.set(0, 0, 0); o.rotation.set(0, 0, 0); o.scale.set(1, 1, 1); }
    hero.express({ face: null });
    if (hero.current) hero.stop(hero.current);
    for (let i = 0, n = 0; (i < 30 || hero.current) && n < 900; n++) { update(DT, f * DT, 0, true, 0, 1); if (!hero.current) i++; }
    const pat = DT_PATTERNS[dtKind], every = DT_EVERY[dtKind] * (sc.every ?? 1), tag = `game:${sc.name}@${dtKind}`;
    const ball = sc.holder === 'ball' ? newBall(sc.grounded ?? true) : null, friend = sc.holder === 'friend' ? newFriend() : null;
    let speed = sc.speed ?? 0, rate = sc.rate ?? 1, armed = sc.armed ?? 0, grounded = sc.grounded ?? true;
    const todo = sc.do.map(c => ({ ...c, done: false }));
    const stopCurrent = () => { if (hero.current) hero.stop(hero.current); };
    const signal = c => {
      if (c.stopCurrent) stopCurrent();
      if (c.stop) hero.stop(c.stop);
      if (c.start) hero.start(c.start, c.opts ?? {});
      if (c.hit) hero.hit(c.hit);
      if (c.speed !== undefined) speed = c.speed;
      if (c.rate !== undefined) rate = c.rate;
      if (c.armed !== undefined) armed = c.armed;
      if (c.grounded !== undefined) grounded = c.grounded;
      if (ball) {
        if (c.slap) { hero.hit(); ball.tumbleT = TUMBLE_S; ball.tumbleDir = c.slap; ball.slapK = 1; }
        if (c.ouch) hero.hit();
        if (c.kick) ball.squashV += c.kick;
        if (c.face) { ball.faceNow = FACES[c.face]; ball.faceT = c.faceSeconds; }
      }
      if (friend) {
        if (c.swing) { if (friend.swingT > 0.12) { hero.start('attack', { seconds: 0.34 }); friend.swingT = 0; } friend.hopV += 3.2; friend.sqV -= 6; friend.leanZ = -c.swing * 0.18; }
        if (c.catch) { friend.sqV += 8; friend.faceNow = FACES.surprised; friend.faceT = 0.3; }
        if (c.cheer) { friend.faceNow = FACES.cheer; friend.faceT = c.cheer; friend.hopV += 4.5; }
        if (c.nod) friend.sqV += 3;
        if (c.worry) { friend.faceNow = FACES.sad; friend.faceT = c.worry; }
        if (c.jumpTo) { friend.from = [...friend.pos]; friend.target = c.jumpTo.slice(0, 3); friend.jumping = c.jumpTo[3]; friend.dur = c.jumpTo[3]; }
      }
    };
    for (let t = 0, k = 0; t < sc.seconds - 1e-9; k++) {
      const dt = pat[k % pat.length];
      for (const c of todo) if (!c.done && !c.stopAfter && t + 1e-9 >= c.at) { c.done = true; signal(c); }
      const look = ball ? ballFrame(ball, dt, t, g) : friend ? friendFrame(friend, dt, t, g) : null;
      if (look) { grounded = look.grounded; hero.express({ face: look.face }); }
      update(dt, t, speed, grounded, armed, rate);
      for (const c of todo) if (!c.done && c.stopAfter && t + 1e-9 >= c.at) { c.done = true; stopCurrent(); }
      t += dt;
      if (k % every === 0) g.sample([tag, 'game'], k);
    }
  }
};

for (const id of ids) {
  const t0 = Date.now();
  const hero = await loadCharacterActor({ client, id, quality: 'normal', gait: { walk: 0.15, run: 2.5 }, parser });
  const g = rigOf(hero), holder = g.holder;
  const { sample, events } = g;
  let f = 0, grounded = true;
  const step = (speed, armed, rate) => {
    hero.move({ speed, grounded, armed, rate });
    const ev = hero.update(DT, f * DT);
    for (const e of ev) events.push([f, e.type, e.action, e.cue ?? null]);
    f++;
  };
  for (const cond of CONDITIONS) for (const action of ACTIONS) {
    const len = framesOf(hero, action);
    if (len === null) continue;
    // 前の動きが残らないように、立って止まった姿まで戻す
    for (let i = 0; i < 30; i++) step(0, 0, 1);
    for (let i = 0; i < LEAD; i++) { if (cond === 'idle') step(0, 0, 1); else step(i < 10 ? 1 : RUN, 0, i < 10 ? 1 : RATE); }
    if (action === 'hit') hero.hit(); else if (action === 'swing') hero.start('attack', { seconds: SWING_SECONDS }); else hero.start(action);
    for (let i = 0; i < len + TAIL; i++) {
      if (cond === 'run') step(RUN, 0, RATE); else if (cond === 'stop') step(0, 1, 1); else step(0, 0, 1);
      if (i % EVERY === 0) sample([cond, action], i);
    }
  }
  const pose = (kind, i, at) => { const p = holderPose(kind, i, at); holder.position.set(p.p[0], p.p[1], p.p[2]); holder.rotation.set(0, p.ry, p.rz); holder.scale.set(p.s[0], p.s[1], p.s[2]); };
  if (hero.query('hit').available) for (const sc of HIT_SCENES) {
    if (sc.before && !hero.query(sc.before === 'swing' ? 'attack' : sc.before).available) continue;
    const len = Math.ceil(hero.query('hit').seconds / DT), rate = sc.speed > 0 ? RATE : 1;
    pose('still', 0, -1);
    grounded = true;
    for (let i = 0; i < 30; i++) step(0, 0, 1);
    grounded = sc.grounded;
    if (sc.before === 'swing') hero.start('attack', { seconds: SWING_SECONDS }); else if (sc.before) hero.start(sc.before);
    for (let i = 0; i < sc.lead; i++) { pose(sc.holder, i, -1); step(sc.speed, sc.armed, rate); }
    const hit = () => hero.hit({ ...(sc.seconds > 0 ? { seconds: sc.seconds } : {}), ...(sc.since > 0 ? { since: sc.since } : {}) });
    hit();
    for (let i = 0; i < len + TAIL; i++) {
      if (sc.again > 0 && i === sc.again) hit();
      if (!sc.grounded && i === 30) grounded = true;
      pose(sc.holder, sc.lead + i, sc.lead);
      step(sc.speed, sc.armed, rate);
      sample(['hit:' + sc.name, 'hit'], i);
    }
    if (sc.before === 'rest') hero.stop('rest');
  }
  g.save(id, id);
  hero.dispose();
  const lite = await loadCharacterActor({ client, id, quality: 'lite', gait: { walk: 0.15, run: 2.5 }, parser });
  const gl = rigOf(lite);
  runGameScenes(lite, gl);
  gl.save(`${id}.lite`, id);
  lite.dispose();
  console.log(`${id}: ${Date.now() - t0}ms`);
}
