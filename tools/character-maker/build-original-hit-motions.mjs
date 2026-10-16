#!/usr/bin/env node
/** のけぞり（hit）のクリップを、オリジナル4体（Tripo の41本の骨）へ移す。docs/main-character-spec.md
 * node tools/character-maker/build-original-hit-motions.mjs [--ids=generic-human-a,...] [--check]
 *
 * 剣の2動作を作った characters の tools/character-maker/build-original-motions.mjs と同じ方法
 * （UAL1 の世界の回転の差分を、解剖学的にそろえた基準の姿へ当てる）。素材は UAL1 Hit_Chest（CC0）。
 * Hit_Chest（約0.33秒）は最も反った姿から始まり待機へ戻るので、時間を2倍にして約0.66秒にする（HIT_DURATION）。
 * 出力は追加分だけ（parts/assets/original-hit-v1/<id>-hit.json）。剣の JSON・元のモデル・既存クリップは読むだけ。
 * 骨の休止の姿は、Studio が今読み込むモデル（studio-catalog.json の modularModel / appearanceModel）から取る。
 * --check は書き出さずに、保存済みの出力と同じになるかだけ確かめる。
 */
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import assert from 'node:assert/strict';
import { NodeIO } from '@gltf-transform/core';
import { ALL_EXTENSIONS } from '@gltf-transform/extensions';
import { MeshoptDecoder } from 'meshoptimizer';
import { AnimationClip, Matrix4, Quaternion, QuaternionKeyframeTrack, Vector3, VectorKeyframeTrack } from 'three';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const VERSION = 'original-hit-v1';
const OUTPUT = 'parts/assets/original-hit-v1';
const SOURCE = 'distribution/characters/_shared/character-maker/sources/animations/UAL1_Standard.glb';
const SOURCE_SHA = '69591853d817488edaa8fd9bf8fc1d821eaeaf789f8627b3cd23b41c4ed67997';
const SOURCE_CLIP = 'Hit_Chest';
const CLIP = 'hit'; // lib/character-maker/hurt.ts HIT_CLIP
const STRETCH = 2; // 0.33 秒 → 0.66 秒（hurt.ts HIT_DURATION）
const IDS = ['generic-human-a', 'generic-human-b', 'generic-robot', 'generic-cat'];
const ids = process.argv.find(value => value.startsWith('--ids='))?.slice(6).split(',') ?? IDS;
const check = process.argv.includes('--check');
const sha = data => createHash('sha256').update(data).digest('hex');
const round = values => values.map(value => Number(value.toFixed(7)));
const identity = new Matrix4();
const position = matrix => new Vector3().setFromMatrixPosition(matrix);
const rotation = matrix => { const q = new Quaternion(); matrix.decompose(new Vector3(), q, new Vector3()); return q.normalize(); };
const matrixError = (a, b) => Math.max(...a.elements.map((value, i) => Math.abs(value - b.elements[i])));
await MeshoptDecoder.ready;
const io = new NodeIO().registerExtensions(ALL_EXTENSIONS).registerDependencies({ 'meshopt.decoder': MeshoptDecoder });
const sourceBytes = await fs.readFile(path.join(ROOT, SOURCE));
assert.equal(sha(sourceBytes), SOURCE_SHA, 'The retained UAL source must not change');
const sourceDocument = await io.readBinary(sourceBytes);

const pairs = [['root', 'Root'], ['pelvis', 'Hip'], ['spine_01', 'Waist'], ['spine_02', 'Spine01'], ['spine_03', 'Spine02'], ['neck_01', 'NeckTwist01'], ['Head', 'Head']];
for (const [suffix, side] of [['l', 'L'], ['r', 'R']]) for (const [source, target] of Object.entries({ clavicle: 'Clavicle', upperarm: 'Upperarm', lowerarm: 'Forearm', hand: 'Hand', thigh: 'Thigh', calf: 'Calf', foot: 'Foot', ball: 'ToeBase' })) pairs.push([`${source}_${suffix}`, `${side}_${target}`]);
const byTarget = new Map(pairs.map(([source, target]) => [target, source]));
const tips = { Hip: 'Waist', Waist: 'Spine01', Spine01: 'Spine02', Spine02: 'NeckTwist01', NeckTwist01: 'Head' };
for (const side of ['L', 'R']) Object.assign(tips, { [`${side}_Clavicle`]: `${side}_Upperarm`, [`${side}_Upperarm`]: `${side}_Forearm`, [`${side}_Forearm`]: `${side}_Hand`, [`${side}_Thigh`]: `${side}_Calf`, [`${side}_Calf`]: `${side}_Foot`, [`${side}_Foot`]: `${side}_ToeBase` });

function skeleton(document) {
  const joints = [...new Set(document.getRoot().listSkins().flatMap(skin => skin.listJoints()))];
  const byName = new Map(joints.map(node => [node.getName(), node]));
  assert.equal(byName.size, joints.length, 'Joint names must be unique');
  const rest = new Map(joints.map(node => { const world = new Matrix4().fromArray(node.getWorldMatrix()); return [node.getName(), {
    t: node.getTranslation(), q: node.getRotation(), s: node.getScale(), world, worldQ: rotation(world),
    parent: node.getParentNode()?.getName(), parentWorld: new Matrix4().fromArray(node.getParentNode()?.getWorldMatrix() ?? identity.elements),
  }]; }));
  const depth = node => byName.has(node.getParentNode()?.getName()) ? depth(node.getParentNode()) + 1 : 0;
  const ordered = joints.toSorted((a, b) => depth(a) - depth(b));
  const meshes = document.getRoot().listNodes().filter(node => node.getSkin() && node.getMesh());
  let bindResidual = 0;
  for (const mesh of meshes) { const skin = mesh.getSkin(), ibm = skin.getInverseBindMatrices().getArray(), inverseMesh = new Matrix4().fromArray(mesh.getWorldMatrix()).invert();
    for (const [i, joint] of skin.listJoints().entries()) bindResidual = Math.max(bindResidual, matrixError(inverseMesh.clone().multiply(rest.get(joint.getName()).world).multiply(new Matrix4().fromArray(ibm, i * 16)), identity));
  }
  assert(bindResidual < .002, `Invalid bind space: ${bindResidual}`);
  return { document, joints, byName, rest, ordered, meshes, bindResidual };
}
function evaluate(skel, locals) {
  const worlds = new Map();
  for (const node of skel.ordered) { const name = node.getName(), rest = skel.rest.get(name), local = locals.get(name);
    worlds.set(name, (worlds.get(rest.parent) ?? rest.parentWorld).clone().multiply(new Matrix4().compose(new Vector3().fromArray(local.t), new Quaternion().fromArray(local.q), new Vector3().fromArray(local.s))));
  }
  return worlds;
}
function sample(skel, animation, time) {
  const locals = new Map([...skel.rest].map(([name, rest]) => [name, { t: [...rest.t], q: [...rest.q], s: [...rest.s] }]));
  for (const channel of animation.listChannels()) {
    const sampler = channel.getSampler(), times = sampler.getInput().getArray(), values = sampler.getOutput().getArray();
    assert.equal(sampler.getInterpolation(), 'LINEAR');
    const property = channel.getTargetPath(), stride = property === 'rotation' ? 4 : 3;
    let i = 0; while (i + 1 < times.length && times[i + 1] <= time) i++;
    const next = Math.min(i + 1, times.length - 1), f = next === i ? 0 : (time - times[i]) / (times[next] - times[i]);
    const a = Array.from(values.slice(i * stride, (i + 1) * stride)), b = Array.from(values.slice(next * stride, (next + 1) * stride));
    const value = property === 'rotation' ? new Quaternion().fromArray(a).normalize().slerp(new Quaternion().fromArray(b).normalize(), f).toArray() : a.map((value, k) => value + (b[k] - value) * f);
    const local = locals.get(channel.getTargetNode().getName());
    if (local) local[property === 'rotation' ? 'q' : property === 'translation' ? 't' : 's'] = value;
  }
  return { locals, worlds: evaluate(skel, locals) };
}
// Only the reference frame changes; the target rest/bind, vertices and source channels stay as they are.
function anatomicalReference(source, target) {
  const locals = new Map(), worlds = new Map();
  for (const node of target.ordered) {
    const name = node.getName(), rest = target.rest.get(name), parent = worlds.get(rest.parent) ?? rest.parentWorld;
    const local = { t: [...rest.t], q: [...rest.q], s: [...rest.s] };
    let world = parent.clone().multiply(new Matrix4().compose(new Vector3().fromArray(local.t), new Quaternion().fromArray(local.q), new Vector3().fromArray(local.s)));
    if (tips[name]) {
      const tip = tips[name], sourceName = byTarget.get(name), sourceTip = byTarget.get(tip);
      const currentQ = rotation(world), delta = currentQ.clone().multiply(rest.worldQ.clone().invert());
      const currentDirection = position(target.rest.get(tip).world).sub(position(rest.world)).applyQuaternion(delta).normalize();
      const desiredDirection = position(source.rest.get(sourceTip).world).sub(position(source.rest.get(sourceName).world)).normalize();
      const corrected = new Quaternion().setFromUnitVectors(currentDirection, desiredDirection).multiply(currentQ).normalize();
      local.q = rotation(parent).invert().multiply(corrected).normalize().toArray();
      world = parent.clone().multiply(new Matrix4().compose(new Vector3().fromArray(local.t), new Quaternion().fromArray(local.q), new Vector3().fromArray(local.s)));
    }
    locals.set(name, local); worlds.set(name, world);
  }
  return { locals, worlds };
}
function retarget(source, target, reference, frame, ratio) {
  const locals = new Map(), worlds = new Map();
  for (const node of target.ordered) {
    const name = node.getName(), rest = target.rest.get(name), sourceName = byTarget.get(name), parent = worlds.get(rest.parent) ?? rest.parentWorld;
    const local = { t: [...rest.t], q: [...reference.locals.get(name).q], s: [...rest.s] };
    if (sourceName) {
      const desired = rotation(frame.worlds.get(sourceName)).multiply(source.rest.get(sourceName).worldQ.clone().invert()).multiply(rotation(reference.worlds.get(name))).normalize();
      local.q = rotation(parent).invert().multiply(desired).normalize().toArray();
      if (sourceName === 'pelvis') {
        const delta = position(frame.worlds.get(sourceName)).sub(position(source.rest.get(sourceName).world)).multiplyScalar(ratio);
        local.t = new Vector3().fromArray(rest.t).add(delta.applyQuaternion(rotation(parent).invert())).toArray();
      }
    }
    locals.set(name, local); worlds.set(name, parent.clone().multiply(new Matrix4().compose(new Vector3().fromArray(local.t), new Quaternion().fromArray(local.q), new Vector3().fromArray(local.s))));
  }
  return { locals, worlds };
}
function floorY(target, worlds) {
  let floor = Infinity;
  for (const mesh of target.meshes) {
    const skin = mesh.getSkin(), ibm = skin.getInverseBindMatrices().getArray();
    const matrices = skin.listJoints().map((joint, i) => worlds.get(joint.getName()).clone().multiply(new Matrix4().fromArray(ibm, i * 16)));
    for (const primitive of mesh.getMesh().listPrimitives()) {
      const p = primitive.getAttribute('POSITION'), j = primitive.getAttribute('JOINTS_0'), w = primitive.getAttribute('WEIGHTS_0'); if (!j || !w) continue;
      const pp = p.getArray(), jj = j.getArray(), ww = w.getArray();
      for (let i = 0; i < p.getCount(); i += 3) { const v = new Vector3(), point = new Vector3().fromArray(pp, i * 3);
        for (let k = 0; k < 4; k++) if (ww[i * 4 + k]) v.addScaledVector(point.clone().applyMatrix4(matrices[jj[i * 4 + k]]), ww[i * 4 + k]);
        floor = Math.min(floor, v.y);
      }
    }
  }
  return floor;
}

const catalog = JSON.parse(await fs.readFile(path.join(ROOT, 'lib/character-maker/studio-catalog.json'), 'utf8'));
const runtime = JSON.parse(await fs.readFile(path.join(ROOT, 'parts/assets/studio-runtime-v1/manifest.json'), 'utf8'));
const source = skeleton(sourceDocument);
const animation = sourceDocument.getRoot().listAnimations().find(item => item.getName() === SOURCE_CLIP);
assert(animation, `${SOURCE_CLIP} is missing`);
const results = [];
let mismatched = 0;
for (const id of ids) {
  assert(IDS.includes(id), `Unknown character ${id}`);
  const entry = catalog.find(item => item.id === id);
  // Studio が読み込むモデル（人型は頭と髪の部品のモデル、ルクス・モカは見た目のモデル）。剣の動作の v3 の合わせ先と同じ。
  const current = entry.modularModel ?? entry.appearanceModel ?? { sha256: entry.modelSha256 };
  const file = runtime.entries.find(item => item.public === `public/models/${current.sha256}.glb`);
  assert(file, `${id}: runtime source for ${current.sha256} is missing`);
  const inputBytes = await fs.readFile(path.join(ROOT, file.source));
  assert.equal(sha(inputBytes), current.sha256);
  const document = await io.readBinary(inputBytes), scene = document.getRoot().getDefaultScene(), facing = document.createNode('maker-original-reference-facing').setRotation([0, -Math.SQRT1_2, 0, Math.SQRT1_2]);
  for (const node of scene.listChildren()) { scene.removeChild(node); facing.addChild(node); } scene.addChild(facing);
  const target = skeleton(document); assert.equal(target.joints.length, 41);
  for (const [s, t] of pairs) assert(source.byName.has(s) && target.byName.has(t), `${s}/${t}`);
  const reference = anatomicalReference(source, target), ratio = position(target.rest.get('Hip').world).y / position(source.rest.get('pelvis').world).y;
  const ground = floorY(target, new Map([...target.rest].map(([name, rest]) => [name, rest.world])));
  const sourceTimes = [...new Set(animation.listSamplers().flatMap(sampler => [...sampler.getInput().getArray()]))].sort((a, b) => a - b);
  const frames = sourceTimes.map(time => retarget(source, target, reference, sample(source, animation, time), ratio));
  // A constant vertical offset seats the clip on the rest floor (as the sword clips do).
  const floorSamples = [...new Set([0, Math.floor(sourceTimes.length / 4), Math.floor(sourceTimes.length / 2), Math.floor(sourceTimes.length * 3 / 4), sourceTimes.length - 1])];
  const floorOffset = ground - Math.min(...floorSamples.map(index => floorY(target, frames[index].worlds)));
  const times = sourceTimes.map(time => Number((time * STRETCH).toFixed(6)));
  const rotations = new Map(target.joints.map(node => [node.getName(), []])), positions = [];
  for (const frame of frames) {
    for (const [bone, values] of rotations) { let q = frame.locals.get(bone).q; if (values.length && q.reduce((sum, v, k) => sum + v * values[values.length - 4 + k], 0) < 0) q = q.map(v => -v); values.push(...round(q)); }
    const hip = new Vector3().fromArray(frame.locals.get('Hip').t), parent = frame.worlds.get(target.rest.get('Hip').parent) ?? target.rest.get('Hip').parentWorld;
    hip.add(new Vector3(0, floorOffset, 0).applyQuaternion(rotation(parent).invert())); positions.push(...round(hip.toArray()));
  }
  const tracks = [...rotations].map(([bone, values]) => new QuaternionKeyframeTrack(`${bone}.quaternion`, times, values));
  tracks.push(new VectorKeyframeTrack('Hip.position', times, positions));
  const clip = new AnimationClip(CLIP, times.at(-1), tracks), uuid = sha(`${VERSION}/${current.sha256}/${CLIP}`).slice(0, 32);
  clip.uuid = `${uuid.slice(0, 8)}-${uuid.slice(8, 12)}-${uuid.slice(12, 16)}-${uuid.slice(16, 20)}-${uuid.slice(20)}`;
  assert(clip.tracks.every(track => track.validate()));
  // 最初は最も反った姿（頭が後ろ）、最後は待機の姿へ戻る、を数値で確かめる
  const headBack = frame => { const head = position(frame.worlds.get('Head')), hip = position(frame.worlds.get('Hip')); return head.z - hip.z; };
  const lean = { start: headBack(frames[0]), end: headBack(frames.at(-1)) };
  const bytes = `${JSON.stringify([AnimationClip.toJSON(clip)])}\n`;
  const outputPath = path.join(ROOT, OUTPUT, `${id}-hit.json`);
  if (check) {
    const saved = await fs.readFile(outputPath, 'utf8').catch(() => '');
    if (saved !== bytes) { mismatched++; console.error(`${id}: ${path.relative(ROOT, outputPath)} differs from the builder output`); }
  } else {
    await fs.mkdir(path.dirname(outputPath), { recursive: true });
    await fs.writeFile(outputPath, bytes);
  }
  results.push({ id, model: { path: file.source, sha256: current.sha256 }, output: { path: path.relative(ROOT, outputPath), sha256: sha(bytes) }, clip: CLIP, duration: clip.duration, samples: times.length, tracks: tracks.length, floorOffset, bindResidual: target.bindResidual, headOffsetAlongForward: lean });
}
if (!check && ids.length === IDS.length) {
  const builder = fileURLToPath(import.meta.url);
  const manifest = { version: VERSION, sourceAnimation: { path: SOURCE, sha256: SOURCE_SHA, clip: SOURCE_CLIP, license: 'CC0 (parts/sources/animations/License.txt)' }, timeScale: STRETCH,
    builder: { path: path.relative(ROOT, builder), sha256: sha(await fs.readFile(builder)) },
    method: 'world-quaternion-delta-over-anatomically-aligned-reference (as the original sword clips); additive output only; source models, sword clips and preset clips untouched',
    mapping: pairs, characters: results };
  await fs.writeFile(path.join(ROOT, OUTPUT, 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`);
}
for (const result of results) console.log(JSON.stringify({ id: result.id, duration: result.duration, samples: result.samples, tracks: result.tracks, floorOffset: Number(result.floorOffset.toFixed(4)), lean: result.headOffsetAlongForward, sha256: result.output.sha256.slice(0, 12) }));
if (mismatched) process.exit(1);
