#!/usr/bin/env node
/** のけぞり（hit）だけを作って足す道具。docs/main-character-spec.md
 * node tools/character-maker/build-hit-motions.mjs
 *
 * UAL1 の Hit_Chest（CC0）を、build-extended-motions.mjs と同じ方法（世界回転の移し替え）で
 * ファンタジー・アメコミの各体の骨へ移し、時間を2倍（約0.66秒）にして書き出す。
 * 配布中の動作 JSON は作り直さない（後から直した剣の動作が戻るため）。新しいファイルだけを作る:
 *   public/maker-assets/{fantasy|comic}-{feminine|masculine}-hit-motions.json  （実行時に読む）
 *   parts/assets/hit-v1/{style}-{base}-hit-validation.json                      （確認の記録）
 */
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { AnimationClip, Matrix4, Quaternion, QuaternionKeyframeTrack, Vector3, VectorKeyframeTrack } from 'three';
import { fantasySources, readGeometry, retarget, sample, semanticMapping, skeleton, sourceFile, sourceSHA, floorY, compareCanonical } from './build-extended-motions.mjs';

const repo = fileURLToPath(new URL('../../', import.meta.url));
const maker = path.join(repo, 'parts');
const HIT_SOURCE = 'Hit_Chest';
const HIT_CLIP = 'hit';
/** Hit_Chest（0.33秒）は速すぎて読み取れないので、2倍に伸ばして焼き込む（hurt.ts の HIT_DURATION） */
const STRETCH = 2;
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const round = x => Number(x.toFixed(8));
const rotation = m => { const q = new Quaternion(); m.decompose(new Vector3(), q, new Vector3()); return q.normalize(); };
const xyz = m => new Vector3().setFromMatrixPosition(m);
const fileRecord = async file => { const b = await fs.readFile(file); return { path: path.relative(repo, file), bytes: b.length, sha256: hash(b) }; };

async function target(style, base) {
  if (style === 'comic') {
    const canonical = path.join(maker, `assets/v1/${base}.glb`);
    const input = await readGeometry(canonical), skel = skeleton(input.document);
    if (skel.joints.length !== 65) throw new Error('Unexpected comic skeleton.');
    return { input, target: skel, canonical, identity: true };
  }
  const definition = fantasySources[base];
  const input = await readGeometry(path.join(maker, 'sources/fantasy-v1', definition.file));
  if (input.input.sha256 !== definition.sha256) throw new Error(`Changed fantasy original ${base}`);
  const scene = input.document.getRoot().getDefaultScene(), roots = scene.listChildren();
  const facing = input.document.createNode('maker-fantasy-facing').setRotation([0, 1, 0, 0]);
  for (const root of roots) { scene.removeChild(root); facing.addChild(root); } scene.addChild(facing);
  return { input, target: skeleton(input.document), canonical: path.join(maker, `assets/fantasy-v1/${base}.glb`), identity: false };
}

async function build(source, style, base) {
  const { input, target: skel, canonical, identity } = await target(style, base);
  const mapping = identity
    ? { pairs: source.joints.map(n => ({ source: n.getName(), target: n.getName() })), excluded: [], byTarget: new Map(source.joints.map(n => [n.getName(), n.getName()])) }
    : semanticMapping(input.humanBones, skel);
  for (const p of mapping.pairs) if (!skel.byName.has(p.target)) throw new Error(`Missing bone ${p.target}`);
  const pelvis = mapping.pairs.find(p => p.source === 'pelvis').target;
  const ratio = xyz(skel.rest.get(pelvis).world).y / xyz(source.rest.get('pelvis').world).y;
  const animations = source.document.getRoot().listAnimations();
  const tPose = animations.find(a => a.getName() === 'A_TPose');
  const reference = retarget(source, skel, mapping, sample(source, tPose, 0), ratio);
  const tPoseError = Math.max(...[...skel.rest].map(([name, rest]) => Math.max(...rest.world.elements.map((v, i) => Math.abs(v - reference.worlds.get(name).elements[i])))));
  if (tPoseError > 1e-4) throw new Error(`T-pose recovery failed ${style}/${base}: ${tPoseError}`);
  const animation = animations.find(a => a.getName() === HIT_SOURCE);
  const idle = animations.find(a => a.getName() === 'Idle_Loop');
  if (!animation || !idle) throw new Error('Missing source clip.');
  const times = [...new Set(animation.listSamplers().flatMap(s => Array.from(s.getInput().getArray())))].sort((a, b) => a - b);
  const rotations = new Map(mapping.pairs.map(p => [p.target, []])), positions = [], snapshots = [];
  let rotationError = 0, last = null;
  for (const [index, time] of times.entries()) {
    const src = sample(source, animation, time), frame = retarget(source, skel, mapping, src, ratio);
    for (const p of mapping.pairs) {
      const values = rotations.get(p.target); let q = frame.locals.get(p.target).q;
      if (values.length && q.reduce((s, v, k) => s + v * values[values.length - 4 + k], 0) < 0) q = q.map(v => -v);
      values.push(...q);
      const wanted = rotation(src.worlds.get(p.source)).multiply(source.rest.get(p.source).worldQ.clone().invert());
      const got = rotation(frame.worlds.get(p.target)).multiply(skel.rest.get(p.target).worldQ.clone().invert());
      rotationError = Math.max(rotationError, wanted.angleTo(got));
    }
    positions.push(...frame.locals.get(pelvis).t);
    if ([0, Math.floor(times.length / 2), times.length - 1].includes(index)) {
      const head = mapping.pairs.find(p => p.source === 'Head').target;
      snapshots.push({ time: round(time * STRETCH), headZ: round(xyz(frame.worlds.get(head)).z), floorY: round(floorY(skel, frame.worlds)) });
    }
    last = frame;
  }
  if (rotationError > 1e-4) throw new Error(`World rotation transfer failed: ${style}/${base}`);
  // 最後の姿が待機の最初の姿に戻っていることを、骨ごとの回転の差で確かめる。
  const idleFrame = retarget(source, skel, mapping, sample(source, idle, 0), ratio);
  const endToIdleDegrees = Math.max(...mapping.pairs.map(p => new Quaternion().fromArray(last.locals.get(p.target).q).angleTo(new Quaternion().fromArray(idleFrame.locals.get(p.target).q)))) * 180 / Math.PI;
  const stretched = times.map(t => round(t * STRETCH));
  const tracks = [...rotations].map(([bone, values]) => new QuaternionKeyframeTrack(`${bone}.quaternion`, stretched, values));
  tracks.push(new VectorKeyframeTrack(`${pelvis}.position`, stretched, positions));
  const clip = new AnimationClip(HIT_CLIP, stretched.at(-1), tracks);
  const uuid = hash(`${sourceSHA}/${input.input.sha256}/${style}/${base}/${HIT_CLIP}/raw-world-rest-v1/x${STRETCH}`).slice(0, 32);
  clip.uuid = `${uuid.slice(0, 8)}-${uuid.slice(8, 12)}-${uuid.slice(12, 16)}-${uuid.slice(16, 20)}-${uuid.slice(20)}`;
  if (clip.tracks.some(t => !t.validate())) throw new Error(`Invalid clip ${style}/${base}`);
  const file = path.join(repo, 'public/maker-assets', `${style}-${base}-hit-motions.json`);
  await fs.writeFile(file, `${JSON.stringify([AnimationClip.toJSON(clip)])}\n`);
  const report = { input: input.input, sourceSHA256: sourceSHA, sourceClip: HIT_SOURCE, stretch: STRETCH, duration: clip.duration, samples: times.length,
    tracks: tracks.length, pelvisHeightRatio: ratio, tPoseError, worldRotationError: rotationError, endToIdleDegrees: round(endToIdleDegrees), snapshots,
    canonical: await compareCanonical(skel, canonical), output: await fileRecord(file) };
  const directory = path.join(maker, 'assets/hit-v1');
  await fs.mkdir(directory, { recursive: true });
  await fs.writeFile(path.join(directory, `${style}-${base}-hit-validation.json`), `${JSON.stringify(report, null, 2)}\n`);
  return { output: report.output, duration: report.duration, endToIdleDegrees: report.endToIdleDegrees, snapshots };
}

const sourceInput = await readGeometry(sourceFile);
if (sourceInput.input.sha256 !== sourceSHA) throw new Error('The preserved UAL source changed.');
const source = skeleton(sourceInput.document), results = {};
for (const style of ['comic', 'fantasy']) for (const base of ['feminine', 'masculine']) results[`${style}/${base}`] = await build(source, style, base);
console.log(JSON.stringify(results, null, 2));
