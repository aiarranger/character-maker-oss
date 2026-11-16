#!/usr/bin/env node
/** ゲーム用の動作（ジャンプ・回避・必殺技・パンチ・やられ・ダンス・話す・しゃがむ・調べる。UAL1 に無いバンザイ・手を振る・座る・回転キック・食べるは pose-motions.mjs の姿勢から）を作って足す道具。docs/game-motions-spec.md
 * node tools/character-maker/build-action-motions.mjs
 *
 * UAL1（CC0）のクリップを、build-hit-motions.mjs と同じ方法（世界回転の移し替え）でファンタジー・アメコミの各体の骨へ移す。
 * 1 つの動作を複数の原本クリップからつなぐこともできる（ジャンプ = Jump_Start → Jump_Loop の一部 → Jump_Land）。
 * 配布中の動作 JSON は作り直さない。新しいファイルだけを作る:
 *   public/maker-assets/{fantasy|comic}-{feminine|masculine}-action-motions.json  （実行時に読む）
 *   parts/assets/action-v1/{style}-{base}-action-validation.json                   （確認の記録）
 */
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { AnimationClip, Matrix4, Quaternion, QuaternionKeyframeTrack, Vector3, VectorKeyframeTrack } from 'three';
import { evaluate, fantasySources, readGeometry, retarget, sample, semanticMapping, skeleton, sourceFile, sourceSHA, floorY } from './build-extended-motions.mjs';
import { posedMotions } from './pose-motions.mjs';

const repo = fileURLToPath(new URL('../../', import.meta.url));
const maker = path.join(repo, 'parts');
/** 動作の名前 → つなぐ原本クリップ（[名前, 始まり, 終わり, 速さ]。始まり・終わりは秒で、省くと全部）。名前の正本は lib/character-maker/actions.ts */
export const ACTIONS = {
  jump: [['Jump_Start'], ['Jump_Loop', 0, .28], ['Jump_Land']],
  dodge: [['Roll']],
  skill: [['Spell_Simple_Enter'], ['Spell_Simple_Shoot'], ['Spell_Simple_Idle_Loop', 0, .5], ['Spell_Simple_Exit']],
  punch: [['Punch_Jab'], ['Punch_Cross']],
  down: [['Death01']],
  dance: [['Dance_Loop']],
  talk: [['Idle_Talking_Loop']],
  crouch: [['Crouch_Idle_Loop']],
  interact: [['Interact']],
};
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const round = x => Number(x.toFixed(5));
/** 原本のコマを何個おきに取るか */
const STEP = 2;
/** 値が変わらないトラックは最初の 1 キーだけにする */
const compact = (times, values, size) => { for (let i = size; i < values.length; i++) if (Math.abs(values[i] - values[i % size]) > 2e-5) return [times, values]; return [[times[0]], values.slice(0, size)]; };
const rotation = m => { const q = new Quaternion(); m.decompose(new Vector3(), q, new Vector3()); return q.normalize(); };
const xyz = m => new Vector3().setFromMatrixPosition(m);
const fileRecord = async file => { const b = await fs.readFile(file); return { path: path.relative(repo, file), bytes: b.length, sha256: hash(b) }; };

async function target(style, base) {
  if (style === 'comic') {
    const input = await readGeometry(path.join(maker, `assets/v1/${base}.glb`)), skel = skeleton(input.document);
    if (skel.joints.length !== 65) throw new Error('Unexpected comic skeleton.');
    return { input, target: skel, identity: true };
  }
  const definition = fantasySources[base];
  const input = await readGeometry(path.join(maker, 'sources/fantasy-v1', definition.file));
  if (input.input.sha256 !== definition.sha256) throw new Error(`Changed fantasy original ${base}`);
  const scene = input.document.getRoot().getDefaultScene(), roots = scene.listChildren();
  const facing = input.document.createNode('maker-fantasy-facing').setRotation([0, 1, 0, 0]);
  for (const root of roots) { scene.removeChild(root); facing.addChild(root); } scene.addChild(facing);
  return { input, target: skeleton(input.document), identity: false };
}

/** つなぐ原本クリップの時刻の列: [{ animation, sourceTime, time }]。継ぎ目の重なる 1 コマは後ろの物を捨てる */
function timeline(animations, segments) {
  const out = []; let offset = 0;
  for (const [name, from = 0, to = Infinity] of segments) {
    const animation = animations.find(a => a.getName() === name);
    if (!animation) throw new Error(`Missing source clip ${name}`);
    const all = [...new Set(animation.listSamplers().flatMap(s => Array.from(s.getInput().getArray())))].sort((a, b) => a - b);
    const times = all.filter(t => t >= from - 1e-6 && t <= to + 1e-6);
    // 原本は 30 コマ／秒。1 つおきに取り（15 コマ／秒）、区切りの最後のコマは必ず残す
    for (const [index, t] of times.entries()) { if (out.length && index === 0) continue; if (index % STEP && index !== times.length - 1) continue; out.push({ animation, sourceTime: t, time: round(offset + t - times[0]) }); }
    offset += times.at(-1) - times[0];
  }
  return out;
}

async function build(source, style, base) {
  const { input, target: skel, identity } = await target(style, base);
  const mapping = identity
    ? { pairs: source.joints.map(n => ({ source: n.getName(), target: n.getName() })), byTarget: new Map(source.joints.map(n => [n.getName(), n.getName()])) }
    : semanticMapping(input.humanBones, skel);
  for (const p of mapping.pairs) if (!skel.byName.has(p.target)) throw new Error(`Missing bone ${p.target}`);
  const pelvis = mapping.pairs.find(p => p.source === 'pelvis').target;
  const ratio = xyz(skel.rest.get(pelvis).world).y / xyz(source.rest.get('pelvis').world).y;
  const animations = source.document.getRoot().listAnimations();
  const clips = [], report = { input: input.input, sourceSHA256: sourceSHA, pelvisHeightRatio: ratio, clips: {} };
  // UAL1 に無い動作は、待機の最初の姿から姿勢で作る（pose-motions.mjs）
  const posed = posedMotions(source, evaluate, sample(source, animations.find(a => a.getName() === 'Idle_Loop'), 0).locals);
  const ground = floorY(skel, new Map([...skel.rest].map(([name, rest]) => [name, rest.world])));
  const all = [...Object.entries(ACTIONS).map(([name, segments]) => [name, { segments, frames: timeline(animations, segments).map(key => ({ time: key.time, frame: sample(source, key.animation, key.sourceTime) })) }]),
    ...Object.entries(posed).map(([name, def]) => [name, { segments: 'posed', floor: def.floor, frames: def.frames }])];
  for (const [name, { segments, floor, frames }] of all) {
    const rotations = new Map(mapping.pairs.map(p => [p.target, []])), positions = [];
    let rotationError = 0, floorMin = Infinity, floorMax = -Infinity;
    for (const [index, { frame: src }] of frames.entries()) {
      const frame = retarget(source, skel, mapping, src, ratio);
      for (const p of mapping.pairs) {
        const values = rotations.get(p.target); let q = frame.locals.get(p.target).q;
        if (values.length && q.reduce((s, v, k) => s + v * values[values.length - 4 + k], 0) < 0) q = q.map(v => -v);
        values.push(...q.map(round));
        const wanted = rotation(src.worlds.get(p.source)).multiply(source.rest.get(p.source).worldQ.clone().invert());
        const got = rotation(frame.worlds.get(p.target)).multiply(skel.rest.get(p.target).worldQ.clone().invert());
        rotationError = Math.max(rotationError, wanted.angleTo(got));
      }
      const hip = new Vector3().fromArray(frame.locals.get(pelvis).t);
      if (floor === 'snap') {
        // 座る動作: 毎コマ、いちばん低い所を立った姿の床の高さへ合わせる（脚の長さが体ごとに違っても、浮かない・沈まない）
        const parent = frame.worlds.get(skel.rest.get(pelvis).parent) ?? skel.rest.get(pelvis).parentWorld;
        hip.add(new Vector3(0, ground - floorY(skel, frame.worlds), 0).applyQuaternion(rotation(parent).invert()));
      }
      positions.push(...hip.toArray().map(round));
      if (index % 8 === 0 && floor !== 'snap') { const y = floorY(skel, frame.worlds); floorMin = Math.min(floorMin, y); floorMax = Math.max(floorMax, y); }
    }
    if (rotationError > 1e-4) throw new Error(`World rotation transfer failed: ${style}/${base}/${name}`);
    const times = frames.map(f => f.time);
    const tracks = [...rotations].map(([bone, values]) => new QuaternionKeyframeTrack(`${bone}.quaternion`, ...compact(times, values, 4)));
    tracks.push(new VectorKeyframeTrack(`${pelvis}.position`, times, positions));
    const clip = new AnimationClip(name, times.at(-1), tracks);
    const uuid = hash(`${sourceSHA}/${input.input.sha256}/${style}/${base}/${name}/${JSON.stringify(segments)}/action-v1`).slice(0, 32);
    clip.uuid = `${uuid.slice(0, 8)}-${uuid.slice(8, 12)}-${uuid.slice(12, 16)}-${uuid.slice(16, 20)}-${uuid.slice(20)}`;
    if (clip.tracks.some(t => !t.validate())) throw new Error(`Invalid clip ${style}/${base}/${name}`);
    clips.push(AnimationClip.toJSON(clip));
    report.clips[name] = { segments, duration: round(clip.duration), samples: times.length, tracks: tracks.length, worldRotationError: rotationError, floorY: floor === 'snap' ? 'snap' : [round(floorMin), round(floorMax)] };
  }
  const file = path.join(repo, 'public/maker-assets', `${style}-${base}-action-motions.json`);
  await fs.writeFile(file, `${JSON.stringify(clips)}\n`);
  report.output = await fileRecord(file);
  const directory = path.join(maker, 'assets/action-v1');
  await fs.mkdir(directory, { recursive: true });
  await fs.writeFile(path.join(directory, `${style}-${base}-action-validation.json`), `${JSON.stringify(report, null, 2)}\n`);
  return { bytes: report.output.bytes, clips: Object.keys(report.clips).join(' ') };
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const sourceInput = await readGeometry(sourceFile);
  if (sourceInput.input.sha256 !== sourceSHA) throw new Error('The preserved UAL source changed.');
  const source = skeleton(sourceInput.document), results = {};
  for (const style of ['comic', 'fantasy']) for (const base of ['feminine', 'masculine']) results[`${style}/${base}`] = await build(source, style, base);
  console.log(JSON.stringify(results, null, 2));
}
