#!/usr/bin/env node
/**
 * Build the maker's three derived clips from the unchanged CC0 Quaternius source.
 * The Peasant skeleton is the target: its rest TRS, inverse binds and bone lengths
 * are never rewritten. Bind-space rotations, rather than matching bone names,
 * define the conversion. No Blender, network, or new runtime dependency is used.
 */
import fs from 'node:fs/promises';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { NodeIO } from '@gltf-transform/core';
import { KHRMaterialsUnlit } from '@gltf-transform/extensions';
import {
  AnimationClip, Matrix4, Quaternion, QuaternionKeyframeTrack,
  Vector3, VectorKeyframeTrack,
} from 'three';

const repo = fileURLToPath(new URL('../../', import.meta.url));
const maker = path.join(repo, 'parts');
const sourceDirectory = path.join(maker, 'sources/animations');
const outputDirectory = path.join(maker, 'assets/v1');
// 動作原本は characters が持つ（再現記録が参照するため）。ここでは distribution/ に取り込んだ SHA 照合済みの写しを読む。
const sourceFile = path.join(repo, 'distribution/characters/_shared/character-maker/sources/animations/UAL1_Standard.glb');
const clipNames = { idle: 'Idle_Loop', walk: 'Walk_Loop', run: 'Jog_Fwd_Loop' };
const identity = new Matrix4();
const io = new NodeIO().registerExtensions([KHRMaterialsUnlit]);
const sha256 = (data) => createHash('sha256').update(data).digest('hex');
const number = (value) => Number(value.toFixed(8));
const vector = (values) => values.map(number);
const rotation = (matrix) => {
  const q = new Quaternion();
  matrix.decompose(new Vector3(), q, new Vector3());
  return q.normalize();
};
const matrixError = (a, b) => Math.max(...a.elements.map((x, i) => Math.abs(x - b.elements[i])));

async function readTargetGeometry(file) {
  // The preserved outfit glTF references unused PBR maps that the unlit maker
  // does not distribute. Motion inspection needs only geometry and skin buffers;
  // remove material references in memory, leaving the source files unchanged.
  const json = JSON.parse(await fs.readFile(file, 'utf8'));
  const resources = {}, buffers = [];
  for (const buffer of json.buffers ?? []) {
    const data = await fs.readFile(path.join(path.dirname(file), buffer.uri));
    resources[buffer.uri] = new Uint8Array(data);
    buffers.push({ uri: buffer.uri, bytes: data.length, sha256: sha256(data) });
  }
  for (const mesh of json.meshes ?? []) for (const primitive of mesh.primitives) delete primitive.material;
  for (const property of ['materials', 'images', 'textures', 'samplers']) delete json[property];
  return { document: await io.readJSON({ json, resources }), buffers };
}

function skeleton(document) {
  const skin = document.getRoot().listSkins()[0];
  if (!skin) throw new Error('A skinned target is required.');
  const joints = skin.listJoints();
  const byName = new Map(joints.map((node) => [node.getName(), node]));
  if (byName.size !== joints.length) throw new Error('Joint names must be unique.');
  const rest = new Map(joints.map((node) => {
    const world = new Matrix4().fromArray(node.getWorldMatrix());
    const parentWorld = new Matrix4().fromArray(node.getParentNode()?.getWorldMatrix() ?? identity.elements);
    return [node.getName(), {
      t: node.getTranslation(), q: node.getRotation(), s: node.getScale(),
      world, worldQ: rotation(world), parentWorld,
      parent: node.getParentNode()?.getName() ?? null,
    }];
  }));
  // glTF joint order is arbitrary. Evaluate parent joints before their children.
  const depth = (node) => byName.has(node.getParentNode()?.getName()) ? depth(node.getParentNode()) + 1 : 0;
  const ordered = [...joints].sort((a, b) => depth(a) - depth(b));
  const inverseBinds = skin.getInverseBindMatrices()?.getArray();
  if (!inverseBinds) throw new Error('Explicit inverse binds are required.');
  const residual = Math.max(...joints.map((node, i) => matrixError(
    rest.get(node.getName()).world.clone().multiply(new Matrix4().fromArray(inverseBinds, i * 16)), identity,
  )));
  if (residual > 1e-4) throw new Error(`Rest/inverse-bind mismatch: ${residual}`);
  const scaleError = Math.max(...joints.flatMap((node) => node.getScale().map((x) => Math.abs(x - 1))));
  if (scaleError > 1e-4) throw new Error(`This converter requires unscaled skeletons: ${scaleError}`);
  return { document, joints, byName, rest, ordered, inverseBinds, residual, scaleError };
}

function checkHierarchy(source, target) {
  if (source.joints.length !== target.joints.length) throw new Error('Different joint counts.');
  for (const [name, rest] of source.rest) {
    const sourceParent = source.byName.has(rest.parent) ? rest.parent : null;
    const targetParentName = target.rest.get(name)?.parent;
    const targetParent = target.byName.has(targetParentName) ? targetParentName : null;
    if (!target.rest.has(name) || targetParent !== sourceParent) {
      throw new Error(`Missing joint or changed hierarchy: ${name}`);
    }
  }
}

function sourceFrame(source, animation, time) {
  const locals = new Map([...source.rest].map(([name, r]) => [name, { t: [...r.t], q: [...r.q], s: [...r.s] }]));
  for (const channel of animation.listChannels()) {
    const sampler = channel.getSampler();
    if (sampler.getInterpolation() !== 'LINEAR') throw new Error('Expected LINEAR source animation.');
    const input = sampler.getInput().getArray();
    const values = sampler.getOutput().getArray();
    const property = channel.getTargetPath();
    const stride = property === 'rotation' ? 4 : 3;
    let index = 0;
    while (index + 1 < input.length && input[index + 1] <= time) index++;
    const next = Math.min(index + 1, input.length - 1);
    const factor = next === index ? 0 : Math.max(0, Math.min(1, (time - input[index]) / (input[next] - input[index])));
    const a = Array.from(values.slice(index * stride, (index + 1) * stride));
    const b = Array.from(values.slice(next * stride, (next + 1) * stride));
    const value = property === 'rotation'
      ? new Quaternion().fromArray(a).normalize().slerp(new Quaternion().fromArray(b).normalize(), factor).toArray()
      : a.map((v, i) => v + (b[i] - v) * factor);
    locals.get(channel.getTargetNode().getName())[property === 'rotation' ? 'q' : property === 'translation' ? 't' : 's'] = value;
  }
  const worlds = evaluate(source, locals);
  return { locals, worlds };
}

function evaluate(skel, locals) {
  const worlds = new Map();
  for (const node of skel.ordered) {
    const name = node.getName(), rest = skel.rest.get(name), local = locals.get(name);
    const parent = worlds.get(rest.parent) ?? rest.parentWorld;
    worlds.set(name, parent.clone().multiply(new Matrix4().compose(
      new Vector3().fromArray(local.t), new Quaternion().fromArray(local.q).normalize(), new Vector3().fromArray(local.s),
    )));
  }
  return worlds;
}

function retargetFrame(source, target, frame, heightRatio) {
  const locals = new Map(), worldRotations = new Map();
  for (const node of target.ordered) {
    const name = node.getName(), sRest = source.rest.get(name), tRest = target.rest.get(name);
    const src = frame.locals.get(name);
    if (name !== 'pelvis' && new Vector3().fromArray(src.t).distanceTo(new Vector3().fromArray(sRest.t)) > 2e-5) {
      throw new Error(`Unexpected translation outside pelvis: ${name}`);
    }
    if (Math.max(...src.s.map((v, i) => Math.abs(v - sRest.s[i]))) > 1e-5) {
      throw new Error(`Unexpected animated scale: ${name}`);
    }
    // World-space rest correction: Qtarget(t) = Qsource(t) Qsource(rest)^-1 Qtarget(rest).
    const desiredWorldQ = rotation(frame.worlds.get(name))
      .multiply(sRest.worldQ.clone().invert()).multiply(tRest.worldQ).normalize();
    const parentQ = worldRotations.get(tRest.parent) ?? rotation(tRest.parentWorld);
    const localQ = parentQ.clone().invert().multiply(desiredWorldQ).normalize();
    const translation = new Vector3().fromArray(tRest.t);
    if (name === 'pelvis') {
      const delta = new Vector3().fromArray(src.t).sub(new Vector3().fromArray(sRest.t))
        .applyQuaternion(rotation(sRest.parentWorld));
      // Scale the vertical hip displacement, retaining the source's lateral sway.
      delta.y *= heightRatio;
      translation.add(delta.applyQuaternion(rotation(tRest.parentWorld).invert()));
    }
    if (name === 'root' && rotation(frame.worlds.get(name)).angleTo(sRest.worldQ) > 1e-5) {
      throw new Error('The selected source must not contain root motion.');
    }
    locals.set(name, { t: translation.toArray(), q: localQ.toArray(), s: [...tRest.s] });
    worldRotations.set(name, desiredWorldQ);
  }
  return { locals, worlds: evaluate(target, locals) };
}

function meshFloor(skel, worlds) {
  const jointMatrices = skel.joints.map((node, i) => worlds.get(node.getName()).clone()
    .multiply(new Matrix4().fromArray(skel.inverseBinds, i * 16)));
  let min = Infinity;
  for (const node of skel.document.getRoot().listNodes().filter((n) => n.getSkin() && n.getMesh())) {
    for (const primitive of node.getMesh().listPrimitives()) {
      const positions = primitive.getAttribute('POSITION').getArray();
      const joints = primitive.getAttribute('JOINTS_0').getArray();
      const weights = primitive.getAttribute('WEIGHTS_0').getArray();
      for (let i = 0; i < positions.length / 3; i++) {
        const input = new Vector3().fromArray(positions, i * 3), output = new Vector3();
        for (let j = 0; j < 4; j++) {
          const weight = weights[i * 4 + j];
          if (weight) output.addScaledVector(input.clone().applyMatrix4(jointMatrices[joints[i * 4 + j]]), weight);
        }
        min = Math.min(min, output.y);
      }
    }
  }
  return min;
}

function boneDirections(skel, worlds) {
  const pairs = { torso: ['pelvis', 'neck_01'], leftArm: ['upperarm_l', 'hand_l'], rightArm: ['upperarm_r', 'hand_r'],
    leftLeg: ['thigh_l', 'foot_l'], rightLeg: ['thigh_r', 'foot_r'] };
  return Object.fromEntries(Object.entries(pairs).map(([name, [start, end]]) => [name, vector(
    new Vector3().setFromMatrixPosition(worlds.get(end)).sub(new Vector3().setFromMatrixPosition(worlds.get(start))).normalize().toArray(),
  )]));
}

function deterministicUUID(text) {
  const hex = sha256(text).slice(0, 32);
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

async function build(source, targetFile, style, sourceHash, canonicalDirectory) {
  const targetInput = await readTargetGeometry(targetFile);
  const target = skeleton(targetInput.document);
  checkHierarchy(source, target);
  const sourceHeight = new Vector3().setFromMatrixPosition(source.rest.get('pelvis').world).y;
  const targetHeight = new Vector3().setFromMatrixPosition(target.rest.get('pelvis').world).y;
  const heightRatio = targetHeight / sourceHeight;
  const restAnimation = source.document.getRoot().listAnimations().find((a) => a.getName() === 'A_TPose');
  if (!restAnimation) throw new Error('The source must contain the A_TPose reference.');
  const restFrame = retargetFrame(source, target, sourceFrame(source, restAnimation, 0), heightRatio);
  const restError = Math.max(...[...target.rest].map(([name, rest]) => matrixError(rest.world, restFrame.worlds.get(name))));
  if (restError > 1e-4) throw new Error(`T-pose does not recover target rest: ${restError}`);
  const report = { target: path.relative(repo, targetFile), targetSha256: sha256(await fs.readFile(targetFile)), targetBuffers: targetInput.buffers,
    jointCount: target.joints.length, bindResidual: target.residual, restScaleError: target.scaleError,
    pelvisHeightRatio: heightRatio, tPoseMatrixError: restError,
    tPoseDirections: boneDirections(target, restFrame.worlds), clips: {} };
  const output = [];
  for (const [name, originalName] of Object.entries(clipNames)) {
    const animation = source.document.getRoot().listAnimations().find((a) => a.getName() === originalName);
    if (!animation) throw new Error(`Missing source clip ${originalName}`);
    const times = [...new Set(animation.listSamplers().flatMap((s) => Array.from(s.getInput().getArray())))].sort((a, b) => a - b);
    const quaternionValues = new Map(target.joints.map((n) => [n.getName(), []]));
    const pelvisValues = [], sourceFloors = [], targetFloors = [], snapshots = [];
    let maxWorldDeltaError = 0;
    for (const [i, time] of times.entries()) {
      const input = sourceFrame(source, animation, time), result = retargetFrame(source, target, input, heightRatio);
      for (const joint of target.joints) {
        const jointName = joint.getName(), values = quaternionValues.get(jointName);
        let q = result.locals.get(jointName).q;
        // Equivalent quaternion signs must not create a long interpolation arc.
        if (values.length && q.reduce((sum, v, axis) => sum + v * values[values.length - 4 + axis], 0) < 0) q = q.map((v) => -v);
        values.push(...q);
        const expected = rotation(input.worlds.get(jointName)).multiply(source.rest.get(jointName).worldQ.clone().invert());
        const actual = rotation(result.worlds.get(jointName)).multiply(target.rest.get(jointName).worldQ.clone().invert());
        maxWorldDeltaError = Math.max(maxWorldDeltaError, expected.angleTo(actual));
      }
      pelvisValues.push(...result.locals.get('pelvis').t);
      sourceFloors.push(meshFloor(source, input.worlds));
      targetFloors.push(meshFloor(target, result.worlds));
      if ([0, Math.floor((times.length - 1) / 4), Math.floor((times.length - 1) / 2), times.length - 1].includes(i)) {
        snapshots.push({ time: number(time), directions: boneDirections(target, result.worlds), floorY: number(targetFloors.at(-1)) });
      }
    }
    if (maxWorldDeltaError > 1e-4) throw new Error(`World rotation transfer failed: ${name} (${maxWorldDeltaError})`);
    const tracks = [...quaternionValues].map(([joint, values]) => new QuaternionKeyframeTrack(`${joint}.quaternion`, times, values));
    tracks.push(new VectorKeyframeTrack('pelvis.position', times, pelvisValues));
    const clip = new AnimationClip(name, times.at(-1), tracks);
    clip.uuid = deterministicUUID(`${sourceHash}/${style}/${name}/world-rest-retarget-v1`);
    const json = AnimationClip.toJSON(clip);
    const parsed = AnimationClip.parse(json);
    if (parsed.tracks.length !== 66 || parsed.tracks.some((track) => !track.validate())) throw new Error(`Invalid serialized clip ${name}`);
    const loopRotationGap = Math.max(...parsed.tracks.filter((track) => track.name.endsWith('.quaternion')).map((track) =>
      new Quaternion().fromArray(track.values).normalize().angleTo(new Quaternion().fromArray(track.values, track.values.length - 4).normalize())));
    const loopPelvisGap = new Vector3().fromArray(pelvisValues).distanceTo(new Vector3().fromArray(pelvisValues, pelvisValues.length - 3));
    output.push(json);
    report.clips[name] = { sourceClip: originalName, samples: times.length, duration: times.at(-1),
      worldDeltaErrorRadians: maxWorldDeltaError,
      sourceLoopEndpointDifferenceRetained: { rotationDegrees: number(loopRotationGap * 180 / Math.PI), pelvisDistance: number(loopPelvisGap) },
      sourceFloorY: vector([Math.min(...sourceFloors), Math.max(...sourceFloors)]),
      targetFloorY: vector([Math.min(...targetFloors), Math.max(...targetFloors)]), snapshots };
  }
  if (canonicalDirectory) {
    const canonical = skeleton(await io.read(path.join(canonicalDirectory, `${style}.glb`)));
    checkHierarchy(target, canonical);
    let trsError = 0, ibError = 0, parentFrameError = 0;
    for (const joint of target.joints) {
      const a = target.rest.get(joint.getName()), b = canonical.rest.get(joint.getName());
      for (const key of ['t', 'q', 's']) trsError = Math.max(trsError, ...a[key].map((v, i) => Math.abs(v - b[key][i])));
      if (!target.byName.has(a.parent)) parentFrameError = Math.max(parentFrameError, matrixError(a.parentWorld, b.parentWorld));
      const from = target.joints.indexOf(joint) * 16;
      const to = canonical.joints.findIndex((n) => n.getName() === joint.getName()) * 16;
      ibError = Math.max(ibError, ...Array.from(target.inverseBinds.slice(from, from + 16), (v, i) => Math.abs(v - canonical.inverseBinds[to + i])));
    }
    if (trsError > 1e-6 || ibError > 1e-6 || parentFrameError > 1e-6) throw new Error(`Canonical rest changed: TRS ${trsError}, IBM ${ibError}, parent ${parentFrameError}`);
    report.canonicalRestCheck = { trsError, inverseBindError: ibError, parentFrameError };
  }
  await fs.mkdir(outputDirectory, { recursive: true });
  const outputFile = path.join(outputDirectory, `${style}-motions.json`);
  const data = JSON.stringify(output);
  await fs.writeFile(outputFile, `${data}\n`);
  report.output = { path: path.relative(repo, outputFile), bytes: Buffer.byteLength(data) + 1, sha256: sha256(`${data}\n`) };
  return report;
}

const options = new Map();
for (let i = 2; i < process.argv.length; i += 2) options.set(process.argv[i], process.argv[i + 1]);
if (!options.get('--masculine') || !options.get('--feminine')) {
  throw new Error('Usage: node tools/character-maker/build-motions.mjs --masculine <Male_Peasant.gltf> --feminine <Female_Peasant.gltf> [--canonical-dir <canonical GLB directory>]');
}
const sourceHash = sha256(await fs.readFile(sourceFile));
if (sourceHash !== '69591853d817488edaa8fd9bf8fc1d821eaeaf789f8627b3cd23b41c4ed67997') {
  throw new Error('The preserved animation source hash changed.');
}
const source = skeleton(await io.read(sourceFile));
const report = { method: 'world-rest-retarget-v1', source: path.relative(repo, sourceFile), sourceSha256: sourceHash,
  sourceBindResidual: source.residual, sourceScaleError: source.scaleError,
  translation: 'Target rest translations retained; pelvis source delta transferred, world Y scaled by rest pelvis-height ratio; root fixed.',
  scale: 'Target rest scales retained; no scale tracks emitted.',
  floorNote: 'Read-only skinning at original sample times. The source walk already intersects Y=0; running includes a genuine flight phase. No per-frame floor clamp or change to the source is applied.',
  targets: {} };
for (const style of ['masculine', 'feminine']) {
  report.targets[style] = await build(source, path.resolve(options.get(`--${style}`)), style, sourceHash, options.get('--canonical-dir'));
}
await fs.writeFile(path.join(sourceDirectory, 'retarget-validation.json'), `${JSON.stringify(report, null, 2)}\n`);
console.log(JSON.stringify(Object.fromEntries(Object.entries(report.targets).map(([style, value]) => [style, {
  output: value.output, tPoseMatrixError: value.tPoseMatrixError, canonicalRestCheck: value.canonicalRestCheck,
  floors: Object.fromEntries(Object.entries(value.clips).map(([name, c]) => [name, c.targetFloorY])),
}])), null, 2));
