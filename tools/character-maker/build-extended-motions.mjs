#!/usr/bin/env node
/** Additive UAL motion adapter. The original builder and assets/v1 are read-only.
 * node tools/character-maker/build-extended-motions.mjs [--comic-only]
 * Fantasy targets are the retained VRM0 originals, with the canonical Y-pi wrapper.
 * Animations target raw bones, so ordinary GLTFLoader/AnimationMixer can play them.
 */
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { createHash } from 'node:crypto';
import { NodeIO } from '@gltf-transform/core';
import { AnimationClip, Matrix4, Quaternion, QuaternionKeyframeTrack, Vector3, VectorKeyframeTrack } from 'three';

const repo = fileURLToPath(new URL('../../', import.meta.url));
const maker = path.join(repo, 'parts');
// 動作原本は characters が持つ。distribution/ に取り込んだ SHA 照合済みの写しを読む。
export const sourceFile = path.join(repo, 'distribution/characters/_shared/character-maker/sources/animations/UAL1_Standard.glb');
export const sourceSHA = '69591853d817488edaa8fd9bf8fc1d821eaeaf789f8627b3cd23b41c4ed67997';
export const fantasySources = {
  feminine: { file: 'Victoria_Rubin.vrm', sha256: 'ad5750ce944b708155abb7ac6807e19d7f5e73a939f0ee32daf0539acfca57c0' },
  masculine: { file: 'Sakurada_Fumiriya.vrm', sha256: 'a36e91b81518c59f6da0e3f34a176b79090a8c68cc6bd5fe03c1560744b283f3' },
};
const clipNames = { idle: 'Idle_Loop', walk: 'Walk_Loop', run: 'Jog_Fwd_Loop', sword_idle: 'Sword_Idle', sword_attack: 'Sword_Attack' };
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const round = x => Number(x.toFixed(8));
const io = new NodeIO();
const identity = new Matrix4();
const rotation = m => { const q = new Quaternion(); m.decompose(new Vector3(), q, new Vector3()); return q.normalize(); };
const error = (a, b) => Math.max(...a.elements.map((v, i) => Math.abs(v - b.elements[i])));
const xyz = m => new Vector3().setFromMatrixPosition(m);
const fileRecord = async file => { const b = await fs.readFile(file); return { path: path.relative(repo, file), bytes: b.length, sha256: hash(b) }; };

// Material-free decoding is strictly in memory. Images and VRM metadata remain in
// the retained input. Read semantic names before discarding unsupported extensions.
export async function readGeometry(file) {
  const bytes = await fs.readFile(file);
  let json, resources;
  if (bytes.readUInt32LE(0) === 0x46546c67) {
    const length = bytes.readUInt32LE(12);
    json = JSON.parse(bytes.toString('utf8', 20, 20 + length));
    resources = { 'geometry.bin': new Uint8Array(bytes.subarray(28 + length)) };
    json.buffers[0].uri = 'geometry.bin';
  } else {
    json = JSON.parse(bytes.toString('utf8')); resources = {};
    for (const buffer of json.buffers) resources[buffer.uri] = new Uint8Array(await fs.readFile(path.join(path.dirname(file), buffer.uri)));
  }
  const humanBones = json.extensions?.VRM?.humanoid?.humanBones?.map(({ bone, node }) => ({ semantic: bone, name: json.nodes[node].name })) ?? [];
  for (const mesh of json.meshes ?? []) for (const primitive of mesh.primitives) delete primitive.material;
  for (const key of ['materials', 'textures', 'images', 'samplers', 'extensions', 'extensionsUsed', 'extensionsRequired']) delete json[key];
  return { document: await io.readJSON({ json, resources }), humanBones, input: { path: path.relative(repo, file), bytes: bytes.length, sha256: hash(bytes) } };
}

export function skeleton(document) {
  const skins = document.getRoot().listSkins();
  const joints = [...new Set(skins.flatMap(skin => skin.listJoints()))];
  const byName = new Map(joints.map(n => [n.getName(), n]));
  if (byName.size !== joints.length) throw new Error('Duplicate raw joint names.');
  const rest = new Map(joints.map(n => {
    const world = new Matrix4().fromArray(n.getWorldMatrix());
    return [n.getName(), { t: n.getTranslation(), q: n.getRotation(), s: n.getScale(), world, worldQ: rotation(world),
      parent: n.getParentNode()?.getName() ?? null, parentWorld: new Matrix4().fromArray(n.getParentNode()?.getWorldMatrix() ?? identity.elements) }];
  }));
  const depth = n => byName.has(n.getParentNode()?.getName()) ? depth(n.getParentNode()) + 1 : 0;
  const ordered = [...joints].sort((a, b) => depth(a) - depth(b));
  let bindResidual = 0;
  const meshes = document.getRoot().listNodes().filter(n => n.getSkin() && n.getMesh());
  for (const mesh of meshes) {
    const skin = mesh.getSkin(), ibm = skin.getInverseBindMatrices()?.getArray();
    if (!ibm) throw new Error('Missing inverse bind matrices.');
    const inverseMesh = new Matrix4().fromArray(mesh.getWorldMatrix()).invert();
    for (const [index, joint] of skin.listJoints().entries()) bindResidual = Math.max(bindResidual, error(
      inverseMesh.clone().multiply(rest.get(joint.getName()).world).multiply(new Matrix4().fromArray(ibm, index * 16)), identity,
    ));
  }
  if (bindResidual > 1e-4) throw new Error(`Bind-space mismatch: ${bindResidual}`);
  if (joints.some(n => n.getScale().some(s => Math.abs(s - 1) > 1e-4))) throw new Error('Unexpected scaled joint.');
  return { document, skins, joints, byName, rest, ordered, meshes, bindResidual };
}

export function semanticMapping(humanBones, target) {
  const semantics = new Map(humanBones.map(h => [h.semantic, h.name]));
  if (semantics.size !== 54 || new Set(semantics.values()).size !== 54) throw new Error('Expected 54 unique humanoid definitions.');
  const map = { pelvis: 'hips', spine_01: 'spine', spine_02: 'chest', spine_03: 'upperChest', neck_01: 'neck', Head: 'head' };
  for (const [suffix, side] of [['l', 'left'], ['r', 'right']]) {
    for (const [ual, vrm] of Object.entries({ clavicle: 'Shoulder', upperarm: 'UpperArm', lowerarm: 'LowerArm', hand: 'Hand', thigh: 'UpperLeg', calf: 'LowerLeg', foot: 'Foot', ball: 'Toes' })) map[`${ual}_${suffix}`] = `${side}${vrm}`;
    for (const [ual, vrm] of Object.entries({ index: 'Index', middle: 'Middle', ring: 'Ring', pinky: 'Little', thumb: 'Thumb' })) {
      for (const [index, part] of [['01', 'Proximal'], ['02', 'Intermediate'], ['03', 'Distal']]) map[`${ual}_${index}_${suffix}`] = `${side}${vrm}${part}`;
    }
  }
  const pairs = Object.entries(map).map(([source, semantic]) => ({ source, semantic, target: semantics.get(semantic) }));
  if (pairs.length !== 52 || new Set(pairs.map(p => p.target)).size !== 52 || pairs.some(p => !p.target || !target.byName.has(p.target))) throw new Error('Invalid or duplicate 52-bone mapping.');
  const excluded = humanBones.filter(h => !pairs.some(p => p.target === h.name));
  if (excluded.map(h => h.semantic).sort().join(',') !== 'leftEye,rightEye') throw new Error('Only the two eyes may be excluded.');
  return { pairs, excluded, byTarget: new Map(pairs.map(p => [p.target, p.source])) };
}

export function evaluate(skel, locals) {
  const worlds = new Map();
  for (const n of skel.ordered) {
    const name = n.getName(), rest = skel.rest.get(name), local = locals.get(name);
    worlds.set(name, (worlds.get(rest.parent) ?? rest.parentWorld).clone().multiply(new Matrix4().compose(
      new Vector3().fromArray(local.t), new Quaternion().fromArray(local.q).normalize(), new Vector3().fromArray(local.s),
    )));
  }
  return worlds;
}

export function sample(source, animation, time) {
  const locals = new Map([...source.rest].map(([name, r]) => [name, { t: [...r.t], q: [...r.q], s: [...r.s] }]));
  for (const channel of animation.listChannels()) {
    const sampler = channel.getSampler(), times = sampler.getInput().getArray(), values = sampler.getOutput().getArray();
    if (sampler.getInterpolation() !== 'LINEAR') throw new Error('Unexpected source interpolation.');
    const property = channel.getTargetPath(), stride = property === 'rotation' ? 4 : 3;
    let i = 0; while (i + 1 < times.length && times[i + 1] <= time) i++;
    const next = Math.min(i + 1, times.length - 1), f = next === i ? 0 : (time - times[i]) / (times[next] - times[i]);
    const a = Array.from(values.slice(i * stride, i * stride + stride)), b = Array.from(values.slice(next * stride, next * stride + stride));
    const value = property === 'rotation' ? new Quaternion().fromArray(a).normalize().slerp(new Quaternion().fromArray(b).normalize(), f).toArray() : a.map((x, k) => x + (b[k] - x) * f);
    locals.get(channel.getTargetNode().getName())[property === 'rotation' ? 'q' : property === 'translation' ? 't' : 's'] = value;
  }
  for (const [name, local] of locals) {
    const r = source.rest.get(name);
    if (name !== 'pelvis' && new Vector3().fromArray(local.t).distanceTo(new Vector3().fromArray(r.t)) > 2e-5) throw new Error(`Unexpected source translation: ${name}`);
    if (local.s.some((v, i) => Math.abs(v - r.s[i]) > 1e-5)) throw new Error(`Unexpected source scale: ${name}`);
  }
  const worlds = evaluate(source, locals);
  if (rotation(worlds.get('root')).angleTo(source.rest.get('root').worldQ) > 1e-5) throw new Error('Root motion is not supported.');
  return { locals, worlds };
}

export function retarget(source, target, mapping, frame, ratio) {
  const locals = new Map(), worlds = new Map();
  for (const node of target.ordered) {
    const name = node.getName(), r = target.rest.get(name), sourceName = mapping.byTarget.get(name);
    const parent = worlds.get(r.parent) ?? r.parentWorld;
    const local = { t: [...r.t], q: [...r.q], s: [...r.s] };
    if (sourceName) {
      const desired = rotation(frame.worlds.get(sourceName)).multiply(source.rest.get(sourceName).worldQ.clone().invert()).multiply(r.worldQ).normalize();
      local.q = rotation(parent).invert().multiply(desired).normalize().toArray();
      if (sourceName === 'pelvis') {
        const delta = xyz(frame.worlds.get(sourceName)).sub(xyz(source.rest.get(sourceName).world));
        // Preserve the established maker behavior: only vertical hip displacement
        // scales with leg length; the original lateral in-place sway is retained.
        delta.y *= ratio;
        local.t = new Vector3().fromArray(r.t).add(delta.applyQuaternion(rotation(parent).invert())).toArray();
      }
    }
    locals.set(name, local);
    worlds.set(name, parent.clone().multiply(new Matrix4().compose(new Vector3().fromArray(local.t), new Quaternion().fromArray(local.q), new Vector3().fromArray(local.s))));
  }
  return { locals, worlds };
}

export function floorY(target, worlds) {
  let min = Infinity;
  for (const mesh of target.meshes) {
    const skin = mesh.getSkin(), ibm = skin.getInverseBindMatrices().getArray();
    const matrices = skin.listJoints().map((n, i) => worlds.get(n.getName()).clone().multiply(new Matrix4().fromArray(ibm, i * 16)));
    const visited = new Set();
    for (const primitive of mesh.getMesh().listPrimitives()) {
      const position = primitive.getAttribute('POSITION');
      if (visited.has(position)) continue; visited.add(position);
      const p = position.getArray(), js = primitive.getAttribute('JOINTS_0').getArray(), ws = primitive.getAttribute('WEIGHTS_0').getArray();
      for (let i = 0; i < position.getCount(); i++) {
        const input = new Vector3().fromArray(p, i * 3), v = new Vector3();
        for (let k = 0; k < 4; k++) if (ws[i * 4 + k]) v.addScaledVector(input.clone().applyMatrix4(matrices[js[i * 4 + k]]), ws[i * 4 + k]);
        min = Math.min(min, v.y);
      }
    }
  }
  return min;
}

export function directions(mapping, worlds) {
  const targetName = source => mapping.pairs.find(p => p.source === source).target;
  return Object.fromEntries(Object.entries({ torso: ['pelvis', 'neck_01'], rightArm: ['upperarm_r', 'hand_r'], leftArm: ['upperarm_l', 'hand_l'], rightLeg: ['thigh_r', 'foot_r'], leftLeg: ['thigh_l', 'foot_l'] }).map(([key, [a, b]]) => [key, xyz(worlds.get(targetName(b))).sub(xyz(worlds.get(targetName(a)))).normalize().toArray().map(round)]));
}

function gripLandmarks(target, mapping, frame) {
  const targetName = source => mapping.pairs.find(p => p.source === source).target;
  const inverse = frame.worlds.get(targetName('hand_r')).clone().invert();
  const values = Object.fromEntries(['index_01_r', 'pinky_01_r', 'middle_01_r', 'middle_02_r', 'middle_03_r', 'thumb_03_r'].map(n => [n, xyz(frame.worlds.get(targetName(n))).applyMatrix4(inverse).toArray().map(round)]));
  return { attachmentBone: targetName('hand_r'), handLocalAtSwordIdle: values };
}

export async function compareCanonical(target, file) {
  try { await fs.access(file); } catch { return { status: 'not-built-yet', path: path.relative(repo, file) }; }
  const actual = skeleton((await readGeometry(file)).document);
  let localError = 0, worldError = 0, inverseBindError = 0;
  for (const [name, a] of target.rest) {
    const b = actual.rest.get(name); if (!b) throw new Error(`Canonical omitted original bone ${name}`);
    for (const key of ['t', 'q', 's']) localError = Math.max(localError, ...a[key].map((v, i) => Math.abs(v - b[key][i])));
    worldError = Math.max(worldError, error(a.world, b.world));
  }
  const skin = target.skins[0], actualSkin = actual.skins[0], ib = skin.getInverseBindMatrices().getArray(), aib = actualSkin.getInverseBindMatrices().getArray();
  for (const [i, joint] of skin.listJoints().entries()) {
    const k = actualSkin.listJoints().findIndex(n => n.getName() === joint.getName());
    if (k < 0) throw new Error(`Canonical skin omitted ${joint.getName()}`);
    for (let x = 0; x < 16; x++) inverseBindError = Math.max(inverseBindError, Math.abs(ib[i * 16 + x] - aib[k * 16 + x]));
  }
  if (Math.max(localError, worldError, inverseBindError) > 1e-5) throw new Error(`Canonical rest mismatch: ${localError}/${worldError}/${inverseBindError}`);
  return { status: 'passed', ...(await fileRecord(file)), localError, worldError, inverseBindError };
}

async function build(source, style, base) {
  let input, target, mapping, canonical;
  if (style === 'comic') {
    canonical = path.join(maker, `assets/v1/${base}.glb`);
    input = await readGeometry(canonical); target = skeleton(input.document);
    if (target.joints.length !== 65) throw new Error('Unexpected comic skeleton.');
    mapping = { pairs: source.joints.map(n => ({ source: n.getName(), target: n.getName() })), excluded: [], byTarget: new Map(source.joints.map(n => [n.getName(), n.getName()])) };
    for (const p of mapping.pairs) if (!target.byName.has(p.target)) throw new Error(`Missing comic bone ${p.target}`);
  } else {
    const definition = fantasySources[base];
    input = await readGeometry(path.join(maker, 'sources/fantasy-v1', definition.file));
    if (input.input.sha256 !== definition.sha256) throw new Error(`Changed fantasy original ${base}`);
    const scene = input.document.getRoot().getDefaultScene(), roots = scene.listChildren();
    const facing = input.document.createNode('maker-fantasy-facing').setRotation([0, 1, 0, 0]);
    for (const root of roots) { scene.removeChild(root); facing.addChild(root); } scene.addChild(facing);
    target = skeleton(input.document); mapping = semanticMapping(input.humanBones, target);
    canonical = path.join(maker, `assets/fantasy-v1/${base}.glb`);
  }
  const pelvis = mapping.pairs.find(p => p.source === 'pelvis').target;
  const ratio = xyz(target.rest.get(pelvis).world).y / xyz(source.rest.get('pelvis').world).y;
  const tPose = source.document.getRoot().listAnimations().find(a => a.getName() === 'A_TPose');
  const reference = retarget(source, target, mapping, sample(source, tPose, 0), ratio);
  const restError = Math.max(...[...target.rest].map(([name, rest]) => error(rest.world, reference.worlds.get(name))));
  if (restError > 1e-4) throw new Error(`T-pose recovery failed ${style}/${base}: ${restError}`);
  const report = { input: input.input, sourceSHA256: sourceSHA, jointCount: target.joints.length, mapping: mapping.pairs, excludedHumanoid: mapping.excluded,
    bindResidual: target.bindResidual, pelvisHeightRatio: ratio, tPoseError: restError, restDirections: directions(mapping, reference.worlds), clips: {} };
  const output = [];
  for (const [name, sourceName] of Object.entries(clipNames).filter(([name]) => style === 'fantasy' || name.startsWith('sword_'))) {
    const animation = source.document.getRoot().listAnimations().find(a => a.getName() === sourceName);
    if (!animation) throw new Error(`Missing source clip ${sourceName}`);
    const times = [...new Set(animation.listSamplers().flatMap(s => Array.from(s.getInput().getArray())))].sort((a, b) => a - b);
    const rotations = new Map(mapping.pairs.map(p => [p.target, []])), positions = [], snapshots = [];
    let rotationError = 0;
    for (const [index, time] of times.entries()) {
      const src = sample(source, animation, time), frame = retarget(source, target, mapping, src, ratio);
      for (const p of mapping.pairs) {
        const values = rotations.get(p.target); let q = frame.locals.get(p.target).q;
        if (values.length && q.reduce((s, v, k) => s + v * values[values.length - 4 + k], 0) < 0) q = q.map(v => -v);
        values.push(...q);
        const wanted = rotation(src.worlds.get(p.source)).multiply(source.rest.get(p.source).worldQ.clone().invert());
        const got = rotation(frame.worlds.get(p.target)).multiply(target.rest.get(p.target).worldQ.clone().invert());
        rotationError = Math.max(rotationError, wanted.angleTo(got));
      }
      positions.push(...frame.locals.get(pelvis).t);
      if ([0, Math.floor(times.length / 4), Math.floor(times.length / 2), Math.floor(times.length * 3 / 4), times.length - 1].includes(index)) {
        snapshots.push({ time: round(time), directions: directions(mapping, frame.worlds), floorY: round(floorY(target, frame.worlds)), sourceFloorY: round(floorY(source, src.worlds)) });
      }
      if (name === 'sword_idle' && index === 0) report.grip = gripLandmarks(target, mapping, frame);
    }
    if (rotationError > 1e-4) throw new Error(`World rotation transfer failed: ${name}`);
    const tracks = [...rotations].map(([bone, values]) => new QuaternionKeyframeTrack(`${bone}.quaternion`, times, values));
    tracks.push(new VectorKeyframeTrack(`${pelvis}.position`, times, positions));
    const shown = name.startsWith('sword_') ? 1 : 0;
    tracks.push(new VectorKeyframeTrack('CharacterMakerSword.scale', [0, times.at(-1)], Array(6).fill(shown)));
    const clip = new AnimationClip(name, times.at(-1), tracks), uuid = hash(`${sourceSHA}/${input.input.sha256}/${style}/${base}/${name}/raw-world-rest-v1`).slice(0, 32);
    clip.uuid = `${uuid.slice(0, 8)}-${uuid.slice(8, 12)}-${uuid.slice(12, 16)}-${uuid.slice(16, 20)}-${uuid.slice(20)}`;
    if (clip.tracks.some(t => !t.validate())) throw new Error(`Invalid clip ${name}`);
    output.push(AnimationClip.toJSON(clip));
    report.clips[name] = { sourceClip: sourceName, duration: clip.duration, samples: times.length, tracks: tracks.length, worldRotationError: rotationError, snapshots };
  }
  report.canonical = await compareCanonical(target, canonical);
  const directory = path.join(maker, style === 'comic' ? 'assets/extended-v1' : 'assets/fantasy-v1');
  await fs.mkdir(directory, { recursive: true });
  const file = path.join(directory, `${base}${style === 'comic' ? '-sword' : ''}-motions.json`);
  await fs.writeFile(file, `${JSON.stringify(output)}\n`); report.output = await fileRecord(file);
  await fs.writeFile(path.join(directory, `${base}-motion-validation.json`), `${JSON.stringify(report, null, 2)}\n`);
  return { output: report.output, canonical: report.canonical, tPoseError: restError, grip: report.grip };
}

// 道具として読み込まれたとき（build-hit-motions.mjs）は、既存の出力を作り直さない。
if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  const v1Files = (await fs.readdir(path.join(maker, 'assets/v1'))).sort();
  const before = await Promise.all(v1Files.map(file => fileRecord(path.join(maker, 'assets/v1', file))));
  const sourceInput = await readGeometry(sourceFile);
  if (sourceInput.input.sha256 !== sourceSHA) throw new Error('The preserved UAL source changed.');
  const source = skeleton(sourceInput.document), results = {};
  for (const style of process.argv.includes('--comic-only') ? ['comic'] : ['comic', 'fantasy']) {
    for (const base of ['feminine', 'masculine']) results[`${style}/${base}`] = await build(source, style, base);
  }
  const after = await Promise.all(v1Files.map(file => fileRecord(path.join(maker, 'assets/v1', file))));
  if (JSON.stringify(before) !== JSON.stringify(after)) throw new Error('Existing assets/v1 changed.');
  console.log(JSON.stringify({ legacyV1FilesUnchanged: before.length, results }, null, 2));
}
