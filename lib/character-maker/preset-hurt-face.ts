// プリセット（ファンタジー・アメコミ）の「のけぞり」と「＞＜」。docs/main-character-spec.md
// - ＞＜ の部品は頭の骨の子のノード（HURT_FACE_NODE）。通常は scale 0、クリップの scale トラックで 1 にする。
// - アメコミ: 目のメッシュの形を写した板に ＞ の絵を貼る（左右の UV が反転しているので ＞＜ になる）。
// - ファンタジー: 顔のモーフ（女性は原本の Extra、男性は目を閉じて ＞＜ の板を前へ出す）。ノードは目印で、
//   書き出す hit・face_hurt には、そのモーフの重みのトラックも入れる。
import {
  AnimationClip, BufferAttribute, BufferGeometry, CanvasTexture, DoubleSide, Group, InterpolateDiscrete, Matrix4, Mesh, MeshBasicMaterial,
  NumberKeyframeTrack, SRGBColorSpace, SkinnedMesh, Vector3, VectorKeyframeTrack, type KeyframeTrack, type Object3D,
} from 'three';
import { HIT_CLIP, HURT_FACE_CLIP, HURT_FACE_NODE } from './hurt';

/** face_hurt の長さ（秒）。ループ再生か、他のクリップと重ねて使う */
const HURT_FACE_LOOP = 1;
/** 閉じた目の面を、元の目の面から顔の前（体の元の空間の +Z）へ出す量（m）。まぶた（約3mm前）より奥に収める */
const COMIC_LIFT = 0.0012;
/** 目の前の面とみなす奥行き（m） */
const COMIC_CAP = 0.004;
/** ＞＜ の板：目の前の面の幅に対する板の幅、縦横比、まぶたより前へ出す量（m） */
const COMIC_MARK = { width: 1.45, aspect: 0.75, lift: 0.0008 };

/** のけぞり（hit）と ＞＜ だけ（face_hurt）のクリップを、読み込んだ体の動きから作る。 */
export function presetHurtClips(clips: readonly AnimationClip[]): AnimationClip[] {
  const body = clips.find(clip => clip.name === HIT_CLIP);
  if (!body) throw new Error('のけぞりの動作データを読み込めませんでした。再読み込みしてください。');
  const hit = body.clone();
  hit.tracks = hit.tracks.filter(track => !track.name.startsWith(`${HURT_FACE_NODE}.`));
  // のけぞっている間は ＞＜。最後の時刻で消す（一回再生の最後の姿では、元の顔に戻る）。
  hit.tracks.push(new VectorKeyframeTrack(`${HURT_FACE_NODE}.scale`, [0, hit.duration], [1, 1, 1, 0, 0, 0], InterpolateDiscrete));
  const face = new AnimationClip(HURT_FACE_CLIP, HURT_FACE_LOOP, [
    new VectorKeyframeTrack(`${HURT_FACE_NODE}.scale`, [0, HURT_FACE_LOOP], [1, 1, 1, 1, 1, 1]),
  ]);
  return [...clips.filter(clip => clip.name !== HIT_CLIP), hit, face];
}

/** 書き出し用：hit と face_hurt に、顔のモーフの重みのトラックを足す（ファンタジーだけ）。
 * 重みは「今の見た目のまま ＞＜ にした値」を、各メッシュの全モーフについて持つ（glTF の weights は全モーフを動かすため）。 */
export function withHurtMorphTracks(clips: readonly AnimationClip[], meshes: readonly Mesh[], normal: readonly number[][], hurt: readonly number[][]): AnimationClip[] {
  const changed = meshes.map((mesh, index) => ({ mesh, normal: normal[index], hurt: hurt[index] }))
    .filter(item => item.hurt.some((value, k) => Math.abs(value - item.normal[k]) > 1e-6));
  if (!changed.length) return [...clips];
  return clips.map(source => {
    if (source.name !== HIT_CLIP && source.name !== HURT_FACE_CLIP) return source;
    const clip = source.clone();
    const tracks: KeyframeTrack[] = changed.map(item => source.name === HIT_CLIP
      ? new NumberKeyframeTrack(`${item.mesh.name}.morphTargetInfluences`, [0, clip.duration], [...item.hurt, ...item.normal], InterpolateDiscrete)
      : new NumberKeyframeTrack(`${item.mesh.name}.morphTargetInfluences`, [0, clip.duration], [...item.hurt, ...item.hurt]));
    clip.tracks.push(...tracks);
    return clip;
  });
}

/** 閉じた目：元の目の絵の白目と瞳を、ぼかした肌の色で覆う（まぶたの間から見える所が肌になる）。 */
function comicCoverTexture(source: MeshBasicMaterial): CanvasTexture {
  const size = 256;
  const canvas = document.createElement('canvas');
  canvas.width = size; canvas.height = size;
  const context = canvas.getContext('2d', { willReadFrequently: true });
  if (!context) throw new Error('＞＜ の絵を作れませんでした。');
  const image = source.map?.image as CanvasImageSource | undefined;
  let skin = 'rgb(214, 140, 132)';
  if (image) {
    context.drawImage(image, 0, 0, size, size);
    // 白目の外（上下左右）の肌の色を拾う。
    const pixels = [[128, 22], [128, 232], [24, 128], [232, 128]].map(([x, y]) => context.getImageData(x, y, 1, 1).data);
    skin = `rgb(${[0, 1, 2].map(k => Math.round(pixels.reduce((sum, pixel) => sum + pixel[k], 0) / pixels.length)).join(',')})`;
  } else {
    context.fillStyle = skin; context.fillRect(0, 0, size, size);
  }
  context.save();
  context.filter = 'blur(6px)';
  context.fillStyle = skin;
  context.beginPath(); context.ellipse(130, 120, 92, 70, 0, 0, Math.PI * 2); context.fill();
  context.restore();
  context.fillStyle = skin;
  context.beginPath(); context.ellipse(130, 120, 80, 60, 0, 0, Math.PI * 2); context.fill();
  return canvasTexture(canvas, 'maker-hurt-face-cover');
}

/** ＞ の線（透明の板に描く）。左右の目で U を反転して ＞＜ にする。 */
function comicMarkTexture(): CanvasTexture {
  const canvas = document.createElement('canvas');
  canvas.width = 256; canvas.height = 192;
  const context = canvas.getContext('2d');
  if (!context) throw new Error('＞＜ の絵を作れませんでした。');
  context.strokeStyle = '#2b1a17';
  context.lineWidth = 26; context.lineCap = 'round'; context.lineJoin = 'round';
  context.beginPath();
  context.moveTo(44, 26); context.lineTo(212, 96); context.lineTo(44, 166);
  context.stroke();
  return canvasTexture(canvas, 'maker-hurt-face-marks');
}

function canvasTexture(canvas: HTMLCanvasElement, name: string) {
  const texture = new CanvasTexture(canvas);
  texture.flipY = false; // glTF の UV（左上が原点）に合わせる
  texture.colorSpace = SRGBColorSpace;
  texture.name = name;
  texture.needsUpdate = true;
  return texture;
}

export type PresetHurtFace = {
  node: Group;
  /** 目の形（モーフ）が変わったら、板を作り直す */
  update: () => void;
  dispose: () => void;
};

/** ＞＜ の部品のノードを頭の骨の下に作る。アメコミは目のメッシュを写した板、ファンタジーは目印だけ。 */
export function createPresetHurtFace(style: 'comic' | 'fantasy', body: Object3D, head: Object3D): PresetHurtFace {
  const node = new Group();
  node.name = HURT_FACE_NODE;
  node.scale.setScalar(0);
  node.userData.makerHurtFace = style === 'comic' ? 'overlay' : 'morph';
  head.add(node);
  if (style !== 'comic') return { node, update: () => {}, dispose: () => node.removeFromParent() };

  const eyes = body.getObjectByName('maker-eyes'), face = body.getObjectByName('maker-head'), lids = body.getObjectByName('maker-brow');
  if (!(eyes instanceof SkinnedMesh) || !(face instanceof SkinnedMesh) || !(lids instanceof SkinnedMesh)) throw new Error('目のメッシュが見つかりません。');
  const headIndex = eyes.skeleton.bones.findIndex(bone => bone.name === head.name);
  if (headIndex < 0) throw new Error('目のメッシュが頭の骨に付いていません。');
  const sourceMaterial = (Array.isArray(eyes.material) ? eyes.material[0] : eyes.material) as MeshBasicMaterial;
  // 閉じた目（目のメッシュの形、白目と瞳を肌で覆う）と、その前に浮かべる ＞＜ の板。
  const coverTexture = comicCoverTexture(sourceMaterial), markTexture = comicMarkTexture();
  const coverMaterial = new MeshBasicMaterial({ map: coverTexture, side: sourceMaterial.side, toneMapped: false, polygonOffset: true, polygonOffsetFactor: -2, polygonOffsetUnits: -2 });
  coverMaterial.name = 'maker-hurt-face-cover';
  const markMaterial = new MeshBasicMaterial({ map: markTexture, transparent: true, depthWrite: false, side: DoubleSide, toneMapped: false });
  markMaterial.name = 'maker-hurt-face-marks';
  const coverGeometry = new BufferGeometry(), markGeometry = new BufferGeometry();
  const cover = new Mesh(coverGeometry, coverMaterial), marks = new Mesh(markGeometry, markMaterial);
  cover.name = `${HURT_FACE_NODE}-cover`; marks.name = `${HURT_FACE_NODE}-marks`;
  cover.frustumCulled = false; marks.frustumCulled = false; marks.renderOrder = 2;
  node.add(cover, marks);
  // 目のメッシュは頭の骨だけに付いている（全頂点）。頭の骨の空間へ移す行列は姿勢によらない。
  const toHead = new Matrix4().multiplyMatrices(eyes.skeleton.boneInverses[headIndex], eyes.bindMatrix);
  const source = eyes.geometry;
  const skinIndex = source.getAttribute('skinIndex'), skinWeight = source.getAttribute('skinWeight');
  for (let i = 0; i < skinIndex.count; i++) for (let k = 0; k < 4; k++) {
    if (skinWeight.getComponent(i, k) > 1e-4 && skinIndex.getComponent(i, k) !== headIndex) throw new Error('目のメッシュが頭以外の骨に付いています。');
  }
  let key = '';
  const point = new Vector3();
  const update = () => {
    const next = [...(eyes.morphTargetInfluences ?? []), ...(face.morphTargetInfluences ?? []), ...(lids.morphTargetInfluences ?? [])].map(value => value.toFixed(4)).join(',');
    if (next === key) return;
    key = next;
    const bind = morphedPositions(eyes), surfaces = [morphedPositions(face), morphedPositions(lids)];
    // 目のメッシュは法線を持たない。顔の前（+Z）へ少しだけ出す（GLB では polygonOffset が無くても前に出る）。
    const count = bind.length / 3, positions = new Float32Array(count * 3);
    for (let i = 0; i < count; i++) {
      point.fromArray(bind, i * 3); point.z += COMIC_LIFT;
      point.applyMatrix4(toHead).toArray(positions, i * 3);
    }
    coverGeometry.setAttribute('position', new BufferAttribute(positions, 3));
    const uv = source.getAttribute('uv');
    if (uv && !coverGeometry.getAttribute('uv')) coverGeometry.setAttribute('uv', uv.clone());
    if (source.index && !coverGeometry.index) coverGeometry.setIndex(source.index.clone());
    coverGeometry.computeBoundingBox(); coverGeometry.computeBoundingSphere();

    // ＞＜ の板：片目ずつ、目の前の面（いちばん前の 4mm）の中心に、まぶたより少し前へ置く（体の元の空間＝顔は +Z 向き）。
    const markPositions: number[] = [], markUvs: number[] = [], markIndex: number[] = [];
    for (const side of [-1, 1]) {
      let front = -Infinity;
      for (let i = 0; i < count; i++) if (Math.sign(bind[i * 3]) === side) front = Math.max(front, bind[i * 3 + 2]);
      let x0 = Infinity, x1 = -Infinity, y0 = Infinity, y1 = -Infinity;
      for (let i = 0; i < count; i++) {
        if (Math.sign(bind[i * 3]) !== side || bind[i * 3 + 2] < front - COMIC_CAP) continue;
        x0 = Math.min(x0, bind[i * 3]); x1 = Math.max(x1, bind[i * 3]); y0 = Math.min(y0, bind[i * 3 + 1]); y1 = Math.max(y1, bind[i * 3 + 1]);
      }
      const cx = (x0 + x1) / 2, cy = (y0 + y1) / 2, width = (x1 - x0) * COMIC_MARK.width, height = width * COMIC_MARK.aspect;
      // 板の範囲の顔（まぶた・頬）とまぶたの線（maker-brow）のいちばん前より、少し前。
      let z = front;
      for (const surface of surfaces) for (let i = 0; i < surface.length / 3; i++) {
        if (Math.abs(surface[i * 3] - cx) < width * 0.42 && Math.abs(surface[i * 3 + 1] - cy) < height * 0.42) z = Math.max(z, surface[i * 3 + 2]);
      }
      z += COMIC_MARK.lift;
      const base = markPositions.length / 3;
      for (const [x, y] of [[cx - width / 2, cy - height / 2], [cx + width / 2, cy - height / 2], [cx + width / 2, cy + height / 2], [cx - width / 2, cy + height / 2]]) {
        point.set(x, y, z).applyMatrix4(toHead);
        markPositions.push(point.x, point.y, point.z);
        // ＞ の頂点は鼻の側（x=0 の側）。左（−x）の目は U をそのまま、右（+x）の目は反転して ＜。
        const u = side < 0 ? (x - (cx - width / 2)) / width : ((cx + width / 2) - x) / width;
        markUvs.push(u, ((cy + height / 2) - y) / height);
      }
      markIndex.push(base, base + 1, base + 2, base, base + 2, base + 3);
    }
    markGeometry.setAttribute('position', new BufferAttribute(new Float32Array(markPositions), 3));
    markGeometry.setAttribute('uv', new BufferAttribute(new Float32Array(markUvs), 2));
    markGeometry.setIndex(markIndex);
    markGeometry.computeVertexNormals(); markGeometry.computeBoundingBox(); markGeometry.computeBoundingSphere();
  };
  update();
  return {
    node,
    update,
    dispose: () => {
      coverGeometry.dispose(); markGeometry.dispose(); coverMaterial.dispose(); markMaterial.dispose(); coverTexture.dispose(); markTexture.dispose();
      node.removeFromParent();
    },
  };
}

/** スキンの元の空間の頂点位置に、今のモーフの重みを足した値 */
function morphedPositions(mesh: SkinnedMesh): Float32Array {
  const geometry = mesh.geometry, position = geometry.getAttribute('position');
  const targets = geometry.morphAttributes.position ?? [], influences = mesh.morphTargetInfluences ?? [];
  const values = new Float32Array(position.count * 3);
  for (let i = 0; i < position.count; i++) {
    let x = position.getX(i), y = position.getY(i), z = position.getZ(i);
    targets.forEach((target: BufferGeometry['attributes'][string], k: number) => {
      const weight = influences[k] ?? 0;
      if (!weight) return;
      if (geometry.morphTargetsRelative) { x += target.getX(i) * weight; y += target.getY(i) * weight; z += target.getZ(i) * weight; }
      else { x += (target.getX(i) - x) * weight; y += (target.getY(i) - y) * weight; z += (target.getZ(i) - z) * weight; }
    });
    values[i * 3] = x; values[i * 3 + 1] = y; values[i * 3 + 2] = z;
  }
  return values;
}
