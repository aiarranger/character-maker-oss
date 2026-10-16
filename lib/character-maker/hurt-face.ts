/**
 * ＞＜ の顔（docs/main-character-spec.md）。characters の packages/character-motion/src/hurt-face.ts の createHurtFace を移植した。
 * まばたきのモーフで動く頂点（閉じた目のまぶた）から目の場所を求め、閉じた目を隠す肌色の楕円と ＞ の線を描いた板を、
 * 顔の面に沿わせて片目に1枚ずつ作り、頭の骨の下の1つのノード（HURT_FACE_NODE）にまとめる。どの頭の骨（Tripo の Head、
 * VRM の J_Bip_C_Head、Mixamo の mixamorigHead）でも使える。three とブラウザの canvas だけを使う。
 * 板は光なし（MeshBasicMaterial）。GLB では、ノードの scale を 0 にしておき、クリップの scale トラックで 1 にする。
 */
import * as THREE from 'three';
import { HURT_FACE_NODE } from './hurt';

/**
 * 1キャラぶんの ＞＜ の見た目。
 * - line: 線の色。skin: 閉じた目を隠す肌の色の予備（ふつうは顔のテクスチャから読む）。fixedSkin なら skin をそのまま使う（ルクスの画面）。
 * - width: 線の太さ（板の高さに対する割合）。size: 板の大きさ（閉じた目の幅に対する倍率）。open: ＞ の開き（度）。
 * - lift: 板を上へずらす量（板の高さに対する割合）。inset: ＞ の頂点を鼻の側へ寄せる量（板の幅に対する割合）。
 * - tall: 板の高さ（幅に対する割合。既定 0.75）。front: 板を乗せる面を、目より板の幅 × front 以内の手前に限る（既定 .25）。
 * - feather: 楕円の外側のこの割合で肌の色をなめらかに消す。glow: 線のまわりに同じ色のぼかしを足す（画面の顔の光る線）。
 * - sampled: 肌の色を板の点ごとに顔から読む（三毛のモカのように、目のまわりで毛の色が変わる顔）。楕円は白で描き、頂点の色を掛ける。
 * - mouth: ＞＜ の間に重ねる口のモーフの重み（そのキャラにある名前だけ効く）。
 * - grid: 板のマス目の数 [横, 縦]（既定 10×8）。
 */
export type HurtFaceStyle = {
  line: string; skin: string; width: number; size: number; open: number; lift: number; inset: number;
  mouth: Readonly<Record<string, number>>; tall?: number; fixedSkin?: boolean; grid?: readonly [number, number];
  front?: number; feather?: number; glow?: number; sampled?: boolean;
};

export type HurtFace = {
  /** 頭の骨の下の、＞＜ の板をまとめたノード（名前は HURT_FACE_NODE） */
  node: THREE.Group;
  /** 見せる強さ（0〜1）。プレビュー用（不透明度と表示を切り替える） */
  set(weight: number): void;
  /** GLB 用: 不透明度を 1 にして、ノードの scale で切り替える形にする（scale 0 で隠れる） */
  prepareForExport(): void;
  dispose(): void;
};

/** 片目の場所（世界の座標）。center は閉じた目の中心、normal は顔の外向き（頂点の法線の平均。無ければ null） */
export type EyeRegion = { center: THREE.Vector3; width: number; height: number; normal: THREE.Vector3 | null };

/** 白目・瞳・まつ毛・ハイライトのメッシュ。目の場所と肌の色は、まぶた・肌のメッシュから取る */
const NOT_LID = /white|iris|sclera|pupil|lash|highlight/i;
const PLATE = `${HURT_FACE_NODE}-eye`;

/** まばたきのモーフ（blinkLeft・blinkRight）で動く頂点の、閉じた姿の位置から左右の目の場所を出す（bind の姿のまま、アニメの前に呼ぶ） */
export function eyeRegionsFromMorphs(root: THREE.Object3D, forward = new THREE.Vector3(0, 0, 1)): { left: EyeRegion; right: EyeRegion } | null {
  return eyeRegionsOf(root, forward, true) ?? eyeRegionsOf(root, forward, false);
}
function eyeRegionsOf(root: THREE.Object3D, forward: THREE.Vector3, lidsOnly: boolean): { left: EyeRegion; right: EyeRegion } | null {
  root.updateMatrixWorld(true);
  const front = { blinkLeft: -Infinity, blinkRight: -Infinity };
  const boxes = { blinkLeft: new THREE.Box3(), blinkRight: new THREE.Box3() };
  const normals = { blinkLeft: new THREE.Vector3(), blinkRight: new THREE.Vector3() };
  const p = new THREE.Vector3(), d = new THREE.Vector3(), n = new THREE.Vector3(), normalMatrix = new THREE.Matrix3();
  const both: { p: THREE.Vector3; n: THREE.Vector3 | null }[] = [];
  root.traverse(object => {
    const mesh = object as THREE.Mesh;
    const dict = mesh.morphTargetDictionary, geometry = mesh.geometry as THREE.BufferGeometry | undefined;
    if (!mesh.isMesh || !dict || !geometry || !mesh.visible || (lidsOnly && NOT_LID.test(mesh.name))) return;
    const position = geometry.getAttribute('position'), normal = geometry.getAttribute('normal');
    const targets = geometry.morphAttributes.position;
    if (!position || !targets) return;
    normalMatrix.getNormalMatrix(mesh.matrixWorld);
    const keys = dict.blinkLeft !== undefined || dict.blinkRight !== undefined ? (['blinkLeft', 'blinkRight'] as const) : (['blink'] as const);
    for (const key of keys) {
      const index = dict[key];
      const target = index === undefined ? undefined : targets[index];
      if (!target) continue;
      for (let i = 0; i < position.count; i++) {
        p.fromBufferAttribute(position, i); d.fromBufferAttribute(target, i);
        if (!geometry.morphTargetsRelative) d.sub(p);
        if (d.lengthSq() < 1e-12) continue;
        p.add(d);
        if ((mesh as THREE.SkinnedMesh).isSkinnedMesh && (mesh as THREE.SkinnedMesh).skeleton) (mesh as THREE.SkinnedMesh).applyBoneTransform(i, p);
        p.applyMatrix4(mesh.matrixWorld);
        const vn = normal && !(mesh as THREE.SkinnedMesh).isSkinnedMesh ? n.fromBufferAttribute(normal, i).applyMatrix3(normalMatrix).normalize() : null;
        if (key === 'blink') { both.push({ p: p.clone(), n: vn?.clone() ?? null }); continue; }
        boxes[key].expandByPoint(p);
        front[key] = Math.max(front[key], p.dot(forward));
        if (vn) normals[key].add(vn);
      }
    }
  });
  if (both.length) {
    const side = new THREE.Vector3(0, 1, 0).cross(forward).normalize(), s = both.map(b => b.p.dot(side));
    const mid = (Math.min(...s) + Math.max(...s)) / 2;
    both.forEach((b, i) => {
      const key = s[i] > mid ? 'blinkLeft' : 'blinkRight';
      boxes[key].expandByPoint(b.p);
      front[key] = Math.max(front[key], b.p.dot(forward));
      if (b.n) normals[key].add(b.n);
    });
  }
  const region = (key: 'blinkLeft' | 'blinkRight'): EyeRegion | null => {
    const box = boxes[key];
    if (box.isEmpty()) return null;
    const size = box.getSize(new THREE.Vector3());
    const center = box.getCenter(new THREE.Vector3());
    center.addScaledVector(forward, front[key] - center.dot(forward));
    // 幅は顔の横の向き（forward と上に直交する向き）の広がり
    const side = new THREE.Vector3(0, 1, 0).cross(forward).normalize();
    const width = Math.abs(size.x * side.x) + Math.abs(size.z * side.z);
    return { center, width: Math.max(width, 1e-4), height: size.y, normal: normals[key].lengthSq() > 1e-12 ? normals[key].clone().normalize() : null };
  };
  const left = region('blinkLeft'), right = region('blinkRight');
  return left && right ? { left, right } : null;
}

/** root の正面（root の座標の +X など、模型の向き）を世界の向きにする */
export function worldForward(root: THREE.Object3D, local: THREE.Vector3): THREE.Vector3 {
  root.updateWorldMatrix(true, false);
  return local.clone().transformDirection(root.matrixWorld);
}

/** 頭の骨（Tripo の Head、VRM の J_Bip_C_Head、Mixamo の mixamorigHead）。無ければ null */
export function findHeadBone(root: THREE.Object3D): THREE.Object3D | null {
  return root.getObjectByName('J_Bip_C_Head') ?? root.getObjectByName('Head') ?? root.getObjectByName('mixamorigHead') ?? null;
}

/** 片目の板の絵: 肌の色の楕円で閉じた目を隠し、鼻の側へとがる ＞ を描く（板の +x が目の外側） */
function drawEye(style: HurtFaceStyle): THREE.CanvasTexture {
  const W = 256, H = Math.round(W * (style.tall ?? .75)), canvas = document.createElement('canvas');
  canvas.width = W; canvas.height = H;
  const g = canvas.getContext('2d')!;
  const f = Math.min(1, Math.max(0, style.feather ?? .38)), steps = 8;
  const fill = g.createRadialGradient(W / 2, H / 2, 0, W / 2, H / 2, H * .5);
  fill.addColorStop(0, style.skin);
  for (let i = 0; i <= steps; i++) {
    const t = i / steps, a = Math.round(255 * (1 - t * t * (3 - 2 * t))), at = 1 - f + f * t;
    if (at > 0) fill.addColorStop(at, a === 255 ? style.skin : `${style.skin}${a.toString(16).padStart(2, '0')}`);
  }
  g.save(); g.translate(W / 2, H / 2); g.scale(W / H, 1); g.translate(-W / 2, -H / 2);
  g.fillStyle = fill; g.beginPath(); g.arc(W / 2, H / 2, H / 2, 0, Math.PI * 2); g.fill(); g.restore();
  const half = (style.open / 2) * Math.PI / 180, arm = W * .3, tipX = W * (.32 - style.inset), cy = H / 2;
  const stroke = () => { g.beginPath(); g.moveTo(tipX + arm * Math.cos(half), cy - arm * Math.sin(half)); g.lineTo(tipX, cy); g.lineTo(tipX + arm * Math.cos(half), cy + arm * Math.sin(half)); g.stroke(); };
  g.lineCap = 'round'; g.lineJoin = 'round';
  if (style.glow) { g.save(); g.strokeStyle = style.line; g.globalAlpha = .35; g.lineWidth = H * style.width * (1 + style.glow); g.filter = `blur(${Math.round(H * .03)}px)`; stroke(); g.restore(); }
  g.strokeStyle = style.line; g.lineWidth = H * style.width; stroke();
  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  return texture;
}

type ProxyFace = { mesh: THREE.Mesh; material: THREE.Material | undefined };
/** 目のまわりの顔の面（目を閉じた姿・骨の変形を通した世界の座標）を小さなメッシュに写す。白目・瞳と板は入れない */
function faceProxy(root: THREE.Object3D, box: THREE.Box3, head: THREE.Object3D): { proxy: THREE.Mesh; faces: ProxyFace[] } {
  root.updateMatrixWorld(true);
  const meshes: THREE.Mesh[] = [];
  root.traverse(o => { const m = o as THREE.Mesh; if (m.isMesh && m.visible && !m.name.startsWith(HURT_FACE_NODE) && !/white|iris|sclera|pupil|highlight/i.test(m.name) && m.geometry?.getAttribute('position')) meshes.push(m); });
  const saved: [THREE.Mesh, number, number][] = [];
  for (const m of meshes) for (const key of ['blinkLeft', 'blinkRight', 'blink']) {
    const i = m.morphTargetDictionary?.[key];
    if (i !== undefined && m.morphTargetInfluences) { saved.push([m, i, m.morphTargetInfluences[i]]); m.morphTargetInfluences[i] = 1; }
  }
  const positions: number[] = [], uvs: number[] = [], faces: ProxyFace[] = [];
  const v = new THREE.Vector3(), c = new THREE.Vector3(), sphere = new THREE.Sphere(), local = new THREE.Box3();
  try {
    for (const m of meshes) {
      const geometry = m.geometry as THREE.BufferGeometry, position = geometry.getAttribute('position'), uv = geometry.getAttribute('uv');
      // 骨で変形しないメッシュは、境界球が目のまわりに届かなければ飛ばす（頂点を全部写すと重い）
      if (!(m as THREE.SkinnedMesh).isSkinnedMesh) {
        if (!geometry.boundingSphere) geometry.computeBoundingSphere();
        sphere.copy(geometry.boundingSphere!).applyMatrix4(m.matrixWorld);
        if (!box.intersectsSphere(sphere)) continue;
      }
      const world = new Float32Array(position.count * 3), inside = new Uint8Array(position.count), computed = new Uint8Array(position.count);
      local.copy(box).expandByScalar(box.getSize(v).length());
      // 骨で変形するメッシュは、頭（とその下の骨）に付いた頂点だけを写す（体の頂点まで骨の変形を通すと重い）
      const skinned = m as THREE.SkinnedMesh, skinIndex = geometry.getAttribute('skinIndex'), skinWeight = geometry.getAttribute('skinWeight');
      let headBones: Set<number> | null = null;
      if (skinned.isSkinnedMesh && skinned.skeleton && skinIndex && skinWeight) {
        headBones = new Set();
        skinned.skeleton.bones.forEach((bone, index) => { for (let o: THREE.Object3D | null = bone; o; o = o.parent) if (o === head) { headBones!.add(index); break; } });
        if (!headBones.size) continue;
      }
      for (let i = 0; i < position.count; i++) {
        if (headBones) {
          let weight = 0;
          for (let k = 0; k < 4; k++) if (headBones.has(skinIndex.getComponent(i, k))) weight += skinWeight.getComponent(i, k);
          if (weight <= .01) continue;
        }
        m.getVertexPosition(i, v); v.applyMatrix4(m.matrixWorld); v.toArray(world, i * 3); inside[i] = local.containsPoint(v) ? 1 : 0; computed[i] = 1;
      }
      const index = geometry.index, count = index ? index.count : position.count;
      const materials = Array.isArray(m.material) ? m.material : null;
      for (let t = 0; t + 2 < count; t += 3) {
        const a = index ? index.getX(t) : t, b = index ? index.getX(t + 1) : t + 1, d = index ? index.getX(t + 2) : t + 2;
        if (!computed[a] || !computed[b] || !computed[d] || (!inside[a] && !inside[b] && !inside[d])) continue;
        c.set((world[a * 3] + world[b * 3] + world[d * 3]) / 3, (world[a * 3 + 1] + world[b * 3 + 1] + world[d * 3 + 1]) / 3, (world[a * 3 + 2] + world[b * 3 + 2] + world[d * 3 + 2]) / 3);
        if (!box.containsPoint(c)) continue;
        for (const k of [a, b, d]) { positions.push(world[k * 3], world[k * 3 + 1], world[k * 3 + 2]); uvs.push(uv ? uv.getX(k) : 0, uv ? uv.getY(k) : 0); }
        const group = materials ? geometry.groups.find(g => t >= g.start && t < g.start + g.count) : null;
        faces.push({ mesh: m, material: materials ? materials[group?.materialIndex ?? 0] : m.material as THREE.Material });
      }
    }
  } finally {
    for (const [m, i, w] of saved) m.morphTargetInfluences![i] = w;
  }
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute('position', new THREE.Float32BufferAttribute(positions, 3));
  geometry.setAttribute('uv', new THREE.Float32BufferAttribute(uvs, 2));
  return { proxy: new THREE.Mesh(geometry, new THREE.MeshBasicMaterial({ side: THREE.DoubleSide })), faces };
}

/** 顔の面の、光線が当たった所のテクスチャの色（読めなければ null） */
function makeTextureReader() {
  const canvas = document.createElement('canvas'), size = 256;
  canvas.width = canvas.height = size;
  const g = canvas.getContext('2d', { willReadFrequently: true });
  const drawn = new Map<unknown, ImageData | null>();
  return (material: THREE.Material | undefined, uv: THREE.Vector2): number[] | null => {
    const basic = material as THREE.MeshBasicMaterial | undefined, map = basic?.map, image = map?.image as CanvasImageSource | undefined;
    const tint = basic?.color?.clone().convertLinearToSRGB() ?? new THREE.Color(1, 1, 1);
    if (!map || !image) return basic?.color ? [tint.r * 255, tint.g * 255, tint.b * 255] : null;
    if (!g) return null;
    if (!drawn.has(image)) { try { g.clearRect(0, 0, size, size); g.drawImage(image, 0, 0, size, size); drawn.set(image, g.getImageData(0, 0, size, size)); } catch { drawn.set(image, null); } }
    const data = drawn.get(image);
    if (!data) return null;
    const u = ((uv.x % 1) + 1) % 1, w = ((uv.y % 1) + 1) % 1;
    const x = Math.min(size - 1, Math.floor(u * size)), y = Math.min(size - 1, Math.floor((map.flipY ? 1 - w : w) * size)), k = (y * size + x) * 4;
    if (data.data[k + 3] <= 200) return null;
    return [data.data[k] * tint.r, data.data[k + 1] * tint.g, data.data[k + 2] * tint.b];
  };
}

const DEFAULT_GRID = { x: 10, y: 8 } as const;

/**
 * ＞＜ の板を2枚作り、頭の骨の下のノード（HURT_FACE_NODE）にまとめる（bind の姿のまま、アニメを当てる前に呼ぶ）。
 * forward は顔の正面（root の座標の向き）。板はマス目の点ごとに、目を閉じた顔の面へ前から光線を当てて、その面のすぐ上に置く。
 * 目や頭の骨が見つからなければ null。
 */
export function createHurtFace(root: THREE.Object3D, style: HurtFaceStyle, forward: THREE.Vector3): HurtFace | null {
  const eyes = eyeRegionsFromMorphs(root, forward), head = findHeadBone(root);
  if (!eyes || !head) return null;
  const GRID = style.grid ? { x: style.grid[0], y: style.grid[1] } : DEFAULT_GRID;
  const up = new THREE.Vector3(0, 1, 0), between = eyes.left.center.clone().add(eyes.right.center).multiplyScalar(.5);
  const plans = [eyes.left, eyes.right].map(eye => {
    const n = (eye.normal ?? forward).clone().lerp(forward, .5).normalize();
    const x = eye.center.clone().sub(between); x.addScaledVector(n, -x.dot(n));
    if (x.lengthSq() < 1e-12) x.crossVectors(up, n);
    x.normalize();
    const y = new THREE.Vector3().crossVectors(n, x).normalize();
    if (y.dot(up) < 0) y.negate();
    const w = eye.width * style.size, h = w * (style.tall ?? .75);
    return { eye, n, x, y, w, h, center: eye.center.clone().addScaledVector(y, h * style.lift) };
  });
  const box = new THREE.Box3();
  for (const p of plans) box.expandByPoint(p.center.clone().addScaledVector(p.x, p.w)).expandByPoint(p.center.clone().addScaledVector(p.x, -p.w)).expandByPoint(p.center.clone().addScaledVector(p.y, p.h)).expandByPoint(p.center.clone().addScaledVector(p.y, -p.h)).expandByPoint(p.center.clone().addScaledVector(p.n, p.w)).expandByPoint(p.center.clone().addScaledVector(p.n, -p.w));
  const { proxy, faces } = faceProxy(root, box, head);
  const raycaster = new THREE.Raycaster(), read = makeTextureReader(), skin: number[][] = [];
  const geometries: THREE.BufferGeometry[] = [], vertexSkin: (number[] | null)[][] = [];
  for (const p of plans) {
    const seen: (number[] | null)[] = [];
    vertexSkin.push(seen);
    const positions: number[] = [], uvs: number[] = [], index: number[] = [];
    const o = new THREE.Vector3(), q = new THREE.Vector3(), back = p.n.clone().negate();
    const depth: (number | null)[] = [];
    for (let j = 0; j <= GRID.y; j++) for (let i = 0; i <= GRID.x; i++) {
      const u = i / GRID.x, v = j / GRID.y;
      q.copy(p.center).addScaledVector(p.x, (u - .5) * p.w).addScaledVector(p.y, (v - .5) * p.h);
      o.copy(q).addScaledVector(p.n, p.w);
      raycaster.set(o, back); raycaster.far = p.w * 2;
      let best: THREE.Intersection | null = null;
      for (const hit of raycaster.intersectObject(proxy, false)) if (hit.distance > p.w * (1 - (style.front ?? .25)) && hit.distance < p.w * 1.6 && (!best || hit.distance < best.distance)) best = hit;
      depth.push(best ? best.distance : null);
      const c = best && best.uv && best.faceIndex != null ? read(faces[best.faceIndex]?.material, best.uv) : null;
      if (c) skin.push(c);
      seen.push(c);
    }
    const hitAlpha = depth.map(d => (d === null ? 0 : 1));
    const filled = depth.slice();
    for (let pass = 0; pass < GRID.x && filled.some(d => d === null); pass++) {
      const prev = filled.slice();
      prev.forEach((d, k) => { if (d !== null) return; const i = k % (GRID.x + 1), j = Math.floor(k / (GRID.x + 1)); let sum = 0, n = 0; for (const [a, b] of [[1, 0], [-1, 0], [0, 1], [0, -1]]) { const ii = i + a, jj = j + b; if (ii < 0 || jj < 0 || ii > GRID.x || jj > GRID.y) continue; const value = prev[jj * (GRID.x + 1) + ii]; if (value !== null) { sum += value; n++; } } if (n) filled[k] = sum / n; });
    }
    const known = filled.map(d => d ?? p.w);
    const at = (i: number, j: number) => known[Math.min(GRID.y, Math.max(0, j)) * (GRID.x + 1) + Math.min(GRID.x, Math.max(0, i))];
    const nearest = known.map((_, k) => { const i = k % (GRID.x + 1), j = Math.floor(k / (GRID.x + 1)); let m = Infinity; for (let b = -1; b <= 1; b++) for (let a = -1; a <= 1; a++) m = Math.min(m, at(i + a, j + b)); return m; });
    const smooth = nearest.map((_, k) => { const i = k % (GRID.x + 1), j = Math.floor(k / (GRID.x + 1)); let sum = 0, n = 0; for (let b = -1; b <= 1; b++) for (let a = -1; a <= 1; a++) { const ii = Math.min(GRID.x, Math.max(0, i + a)), jj = Math.min(GRID.y, Math.max(0, j + b)); sum += nearest[jj * (GRID.x + 1) + ii]; n++; } return Math.min(sum / n, known[k]); });
    const alpha = hitAlpha.map((_, k) => { const i = k % (GRID.x + 1), j = Math.floor(k / (GRID.x + 1)); let sum = 0, n = 0; for (let b = -1; b <= 1; b++) for (let a = -1; a <= 1; a++) { const ii = i + a, jj = j + b; if (ii < 0 || jj < 0 || ii > GRID.x || jj > GRID.y) continue; sum += hitAlpha[jj * (GRID.x + 1) + ii]; n++; } return hitAlpha[k] ? sum / n : 0; });
    const colors: number[] = [];
    // sampled: each point takes the median colour of the closed-eye face around it (3×3 points), filled in from neighbours where nothing was read
    const tint = style.sampled ? seen.map((_, k) => {
      const i = k % (GRID.x + 1), j = Math.floor(k / (GRID.x + 1)), near: number[][] = [];
      for (let r = 1; r <= GRID.x && near.length < 3; r++) { near.length = 0; for (let b = -r; b <= r; b++) for (let a = -r; a <= r; a++) { const ii = i + a, jj = j + b; if (ii < 0 || jj < 0 || ii > GRID.x || jj > GRID.y) continue; const c = seen[jj * (GRID.x + 1) + ii]; if (c) near.push(c); } }
      if (!near.length) return null;
      const median = (channel: number) => near.map(c => c[channel]).sort((x, y) => x - y)[Math.floor(near.length / 2)];
      return new THREE.Color().setRGB(median(0) / 255, median(1) / 255, median(2) / 255, THREE.SRGBColorSpace);
    }) : null;
    for (let j = 0; j <= GRID.y; j++) for (let i = 0; i <= GRID.x; i++) {
      const u = i / GRID.x, v = j / GRID.y, k = j * (GRID.x + 1) + i;
      q.copy(p.center).addScaledVector(p.x, (u - .5) * p.w).addScaledVector(p.y, (v - .5) * p.h).addScaledVector(p.n, p.w - smooth[k] + p.eye.width * .015);
      const c = tint?.[k];
      positions.push(q.x, q.y, q.z); uvs.push(u, v); colors.push(c?.r ?? 1, c?.g ?? 1, c?.b ?? 1, tint && !c ? 0 : alpha[k]);
    }
    for (let j = 0; j < GRID.y; j++) for (let i = 0; i < GRID.x; i++) {
      const a = j * (GRID.x + 1) + i, b = a + 1, c = a + GRID.x + 1, d = c + 1;
      index.push(a, b, d, a, d, c);
    }
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute('position', new THREE.Float32BufferAttribute(positions, 3));
    geometry.setAttribute('uv', new THREE.Float32BufferAttribute(uvs, 2));
    geometry.setAttribute('color', new THREE.Float32BufferAttribute(colors, 4));
    geometry.setIndex(index);
    geometries.push(geometry);
  }
  proxy.geometry.dispose(); (proxy.material as THREE.Material).dispose();
  // 肌の色: 暗い所（まつ毛・目の線）と鮮やかな所（ほほの赤み・髪・唇）を外した、くすんだ側の真ん中
  const hsl = { h: 0, s: 0, l: 0 }, tone = (c: number[]) => { new THREE.Color(c[0] / 255, c[1] / 255, c[2] / 255).getHSL(hsl); return { s: hsl.s, l: hsl.l }; };
  const bright = skin.filter(c => tone(c).l > .45).sort((a, b) => tone(a).s - tone(b).s);
  const calm = bright.slice(0, Math.max(1, Math.ceil(bright.length * .5))).sort((a, b) => (a[0] + a[1] + a[2]) - (b[0] + b[1] + b[2]));
  const mid = bright.length >= 4 ? calm[Math.floor(calm.length / 2)].map(v => Math.round(Math.min(255, v))) : null;
  const faceColor = mid ? `#${mid.map(c => c.toString(16).padStart(2, '0')).join('')}` : null;
  // 当たった面の色が肌から大きく離れている所（顔の縁にかかる髪など）は透明にする。目のまわりは覆いたいので除く
  if (mid && !style.fixedSkin && !style.sampled) geometries.forEach((geometry, e) => {
    const color = geometry.getAttribute('color') as THREE.BufferAttribute, seen = vertexSkin[e];
    for (let k = 0; k < color.count; k++) {
      const i = k % (GRID.x + 1), j = Math.floor(k / (GRID.x + 1)), c = seen[k];
      const u = i / GRID.x - .5, v = j / GRID.y - .5, near = (u / .4) ** 2 + (v / .38) ** 2 > 1;
      if (near && c && Math.abs(c[0] - mid[0]) + Math.abs(c[1] - mid[1]) + Math.abs(c[2] - mid[2]) > 90) color.setW(k, 0);
    }
    color.needsUpdate = true;
  });
  const texture = drawEye({ ...style, skin: style.sampled ? '#ffffff' : (style.fixedSkin ? null : faceColor) ?? style.skin });
  const material = new THREE.MeshBasicMaterial({ map: texture, vertexColors: true, transparent: true, depthWrite: true, alphaTest: .02, side: THREE.DoubleSide, toneMapped: false, opacity: 0, polygonOffset: true, polygonOffsetFactor: -2, polygonOffsetUnits: -2 });
  material.name = HURT_FACE_NODE;
  const node = new THREE.Group();
  node.name = HURT_FACE_NODE;
  head.add(node);
  node.updateMatrixWorld(true);
  const meshes = geometries.map((geometry, i) => {
    const mesh = new THREE.Mesh(geometry, material);
    mesh.name = `${PLATE}-${i ? 'right' : 'left'}`;
    mesh.renderOrder = -1;
    mesh.frustumCulled = false;
    mesh.visible = false;
    // 頂点は世界の座標（親なしの単位の変形）。世界の場所を保ったまま、親を頭の骨の下のノードにする
    mesh.updateMatrixWorld(true);
    node.attach(mesh);
    return mesh;
  });
  return {
    node,
    set(weight: number) { const v = Math.min(1, Math.max(0, weight)); material.opacity = v; for (const m of meshes) m.visible = v > .01; },
    prepareForExport() { material.opacity = 1; material.transparent = true; for (const m of meshes) m.visible = true; node.scale.setScalar(0); },
    // GPU の資源だけを捨てる（React StrictMode の再実行でも板の親子関係を保つ。捨てた後に描けば three が作り直す）
    dispose() { for (const m of meshes) m.geometry.dispose(); material.dispose(); texture.dispose(); },
  };
}
