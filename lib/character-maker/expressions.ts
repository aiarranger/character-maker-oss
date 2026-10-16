import { BufferAttribute, BufferGeometry, Euler, Mesh, type Object3D } from 'three';
import type { GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js';
import type { CharacterConfig } from './config';

// VRM 0.x names, resolved through each source's complete blendShapeGroup.
// Target indices differ between the two bodies; they are never shared.
const SOURCE_PRESETS: Record<string, string> = {
  happy: 'joy', angry: 'angry', sad: 'sorrow', relaxed: 'fun',
  aa: 'a', ih: 'i', ou: 'u', ee: 'e', oh: 'o',
  blink: 'blink', blinkLeft: 'blink_l', blinkRight: 'blink_r',
};
type SourceGroup = {
  name: string; presetName: string; isBinary?: boolean;
  binds: { mesh: number; index: number; weight: number }[];
  materialValues?: unknown[];
};
type Binding = { weights: number[]; index: number; value: number; name: string };
/** 男性の ＞＜ の板を動かすモーフ。女性の EyeExtra_On の実測（板の中心から見た移動・広がり）を、男性の目の大きさに合わせた値 */
const HURT_BOARD = { name: 'makerHurtBoard', shift: [0.0077, 0.0035, -0.037], scale: [2.25, 1.85, 1] } as const;

/** Adapt the already-authored fantasy expressions without editing the template. */
export function createMakerExpressions(source: GLTF, body: Object3D) {
  const groups = source.userData.sourceVrmExpressions?.blendShapeGroups as SourceGroup[] | undefined;
  const sourceMeshNames = source.userData.sourceMeshNames as string[] | undefined;
  if (!groups || !sourceMeshNames) throw new Error('表情データを読み込めませんでした。');
  const meshes: Mesh[] = [];
  body.traverse(object => { if (object instanceof Mesh && object.morphTargetInfluences) meshes.push(object); });
  const geometries: BufferGeometry[] = [];
  const corrections: { mesh: Mesh; name: string; eyes: string; side: 'L' | 'R' }[] = [];
  const eyeOcclusions: { mesh: Mesh; name: string; side: 'L' | 'R' }[] = [];
  const closeMaleEyeSurface = source.userData.characterMaker?.base === 'masculine';
  // ＞＜：女性は原本の表情 Extra（Fcl_EYE_Extra＋EyeExtra_On）。男性は目を閉じて ＞＜ の板を前へ出す。
  const extraGroup = groups.find(item => item.name === 'Extra');
  const hurtBoards: Mesh[] = [];
  type Attribute = BufferGeometry['attributes'][string];
  // Face primitives share their source vertex and morph accessors. Keep that
  // sharing inside this instance instead of copying every accessor ten times.
  // Names are part of the key because updateMorphTargets reads POSITION names.
  const privateAttributes = new Map<Attribute, Map<string, BufferAttribute>>();
  const cloneAttribute = (attribute: Attribute, name = attribute.name) => {
    let named = privateAttributes.get(attribute);
    if (!named) { named = new Map(); privateAttributes.set(attribute, named); }
    let cloned = named.get(name);
    if (!cloned) { cloned = attribute.clone(); cloned.name = name; named.set(name, cloned); }
    return cloned;
  };
  const privateCorrections = new Map<Attribute, Map<Attribute, Map<string, BufferAttribute>>>();
  const zeroNormals = new Map<number, BufferAttribute>();
  for (const mesh of meshes) {
    if (mesh.morphTargetDictionary?.eyesRound === undefined) continue;
    // Open-eye appearance deltas would otherwise keep the lower lid and iris
    // open beneath an authored closed lid. Correct only the closing side's
    // aperture, keeping the chosen eye shape and the other eye unchanged.
    const previousWeights = [...mesh.morphTargetInfluences!];
    const sourceGeometry = mesh.geometry;
    const targetNames = Object.fromEntries(Object.entries(mesh.morphTargetDictionary!).map(([name, index]) => [index, name]));
    const geometry = new BufferGeometry();
    geometry.name = sourceGeometry.name;
    geometry.index = sourceGeometry.index?.clone() ?? null;
    for (const [name, attribute] of Object.entries(sourceGeometry.attributes)) geometry.setAttribute(name, cloneAttribute(attribute));
    geometry.morphAttributes = Object.fromEntries(Object.entries(sourceGeometry.morphAttributes).map(([semantic, attributes]) => [semantic,
      attributes.map((attribute, index) => cloneAttribute(attribute, semantic === 'position' ? targetNames[index] ?? attribute.name : attribute.name)),
    ]));
    geometry.morphTargetsRelative = sourceGeometry.morphTargetsRelative;
    geometry.groups = sourceGeometry.groups.map(group => ({ ...group }));
    geometry.boundingBox = sourceGeometry.boundingBox?.clone() ?? null;
    geometry.boundingSphere = sourceGeometry.boundingSphere?.clone() ?? null;
    geometry.setDrawRange(sourceGeometry.drawRange.start, sourceGeometry.drawRange.count);
    geometry.userData = { ...sourceGeometry.userData };
    geometries.push(geometry); mesh.geometry = geometry;
    const positionTargets = geometry.morphAttributes.position!;
    for (const eyes of ['round', 'slender']) for (const side of ['L', 'R'] as const) {
      const key = eyes === 'round' ? 'eyesRound' : 'eyesSlender';
      const delta = sourceGeometry.morphAttributes.position![mesh.morphTargetDictionary![key]];
      const position = sourceGeometry.attributes.position;
      const name = `makerClosed${key}${side}`;
      let deltas = privateCorrections.get(position);
      if (!deltas) { deltas = new Map(); privateCorrections.set(position, deltas); }
      let named = deltas.get(delta);
      if (!named) { named = new Map(); deltas.set(delta, named); }
      let attribute = named.get(name);
      if (!attribute) {
        const values = new Float32Array(position.count * 3);
        for (let i = 0; i < position.count; i++) {
          const t = Math.max(0, Math.min(1, (position.getX(i) + 0.006) / 0.012));
          const right = t * t * (3 - 2 * t), mask = side === 'L' ? 1 - right : right;
          values[i * 3] = -delta.getX(i) * mask;
          values[i * 3 + 1] = -delta.getY(i) * mask;
          values[i * 3 + 2] = -delta.getZ(i) * mask;
        }
        attribute = new BufferAttribute(values, 3); attribute.name = name; named.set(name, attribute);
      }
      positionTargets.push(attribute);
      for (const semantic of ['normal', 'color'] as const) {
        const targets = geometry.morphAttributes[semantic];
        if (targets?.length) {
          let zero = zeroNormals.get(position.count);
          if (!zero) { zero = new BufferAttribute(new Float32Array(position.count * 3), 3); zeroNormals.set(position.count, zero); }
          targets.push(zero);
        }
      }
      corrections.push({ mesh, name, eyes, side });
    }
    const materials = Array.isArray(mesh.material) ? mesh.material : [mesh.material];
    const eyeSurface = materials.some(material => /_(EyeIris|EyeHighlight|EyeWhite|EyeExtra)_\d+_EYE$/.test(material.name));
    if (closeMaleEyeSurface && eyeSurface) for (const side of ['L', 'R'] as const) {
      // The source male eye sheets leave a sub-pixel iris/sclera glint under
      // fully closed lids. Recess only that eye's internal surface during the
      // final quarter of closure; lid/lash geometry remains authored and intact.
      const position = geometry.attributes.position;
      const values = new Float32Array(position.count * 3);
      const used = new Set<number>();
      if (geometry.index) for (let i = 0; i < geometry.index.count; i++) used.add(geometry.index.getX(i));
      else for (let i = 0; i < position.count; i++) used.add(i);
      for (const i of used) {
        const t = Math.max(0, Math.min(1, (position.getX(i) + 0.006) / 0.012));
        const right = t * t * (3 - 2 * t), mask = side === 'L' ? 1 - right : right;
        values[i * 3 + 1] = 0.035 * mask;
        values[i * 3 + 2] = 0.025 * mask;
      }
      const name = `makerClosedEyeSurface${side}`;
      const attribute = new BufferAttribute(values, 3); attribute.name = name;
      positionTargets.push(attribute);
      for (const semantic of ['normal', 'color'] as const) {
        const targets = geometry.morphAttributes[semantic];
        if (targets?.length) {
          let zero = zeroNormals.get(position.count);
          if (!zero) { zero = new BufferAttribute(new Float32Array(position.count * 3), 3); zeroNormals.set(position.count, zero); }
          targets.push(zero);
        }
      }
      eyeOcclusions.push({ mesh, name, side });
    }
    if (!extraGroup && materials.some(material => /_EyeExtra_\d+_EYE$/.test(material.name))) {
      // ＞＜：男性の原本には Extra の表情が無い。原本の ＞＜ の板（瞳の奥に隠れている）を、
      // 女性の EyeExtra_On の実測（片目ごとに中心で広げ、まぶたの前へ出す）に合わせて動かすモーフを作る。
      const position = geometry.attributes.position;
      const used = new Set<number>();
      if (geometry.index) for (let i = 0; i < geometry.index.count; i++) used.add(geometry.index.getX(i));
      else for (let i = 0; i < position.count; i++) used.add(i);
      let mid = 0; for (const i of used) mid += position.getX(i); mid /= Math.max(used.size, 1);
      const values = new Float32Array(position.count * 3);
      for (const side of [1, -1]) {
        const ids = [...used].filter(i => Math.sign(position.getX(i) - mid) === side);
        const min = [Infinity, Infinity, Infinity], max = [-Infinity, -Infinity, -Infinity];
        for (const i of ids) for (let k = 0; k < 3; k++) { const v = position.getComponent(i, k); min[k] = Math.min(min[k], v); max[k] = Math.max(max[k], v); }
        const centre = min.map((v, k) => (v + max[k]) / 2);
        const goal = [centre[0] + side * HURT_BOARD.shift[0], centre[1] + HURT_BOARD.shift[1], centre[2] + HURT_BOARD.shift[2]];
        for (const i of ids) for (let k = 0; k < 3; k++) {
          const v = position.getComponent(i, k);
          values[i * 3 + k] = goal[k] + (v - centre[k]) * HURT_BOARD.scale[k] - v;
        }
      }
      const attribute = new BufferAttribute(values, 3); attribute.name = HURT_BOARD.name;
      positionTargets.push(attribute);
      for (const semantic of ['normal', 'color'] as const) {
        const targets = geometry.morphAttributes[semantic];
        if (targets?.length) {
          let zero = zeroNormals.get(position.count);
          if (!zero) { zero = new BufferAttribute(new Float32Array(position.count * 3), 3); zeroNormals.set(position.count, zero); }
          targets.push(zero);
        }
      }
      hurtBoards.push(mesh);
    }
    mesh.updateMorphTargets();
    previousWeights.forEach((value, index) => { mesh.morphTargetInfluences![index] = value; });
  }
  const expressions = new Map<string, { bindings: Binding[]; binary: boolean }>();
  const owned = new Map<number[], Set<number>>();
  const profile: Record<string, Record<string, number>> = {};
  for (const key of [...Object.keys(SOURCE_PRESETS), 'surprised']) {
    const group = groups.find(item => key === 'surprised'
      ? item.name === 'Surprised' : item.presetName === SOURCE_PRESETS[key]);
    if (!group || group.materialValues?.length) throw new Error(`対応する表情を読み込めませんでした: ${key}`);
    const bindings: Binding[] = [];
    profile[key] = {};
    for (const bind of group.binds) {
      const name = sourceMeshNames[bind.mesh];
      const meshDefinition = source.parser.json.meshes?.find((item: { name?: string }) => item.name === name);
      const targetName = meshDefinition?.extras?.targetNames?.[bind.index] as string | undefined;
      if (!targetName) throw new Error(`表情の対応先がありません: ${key}`);
      let found = false;
      for (const mesh of meshes) {
        const index = mesh.morphTargetDictionary?.[targetName];
        if (index === undefined || !mesh.morphTargetInfluences) continue;
        found = true;
        const weights = mesh.morphTargetInfluences;
        bindings.push({ weights, index, value: bind.weight / 100, name: targetName });
        const indices = owned.get(weights) ?? new Set<number>();
        indices.add(index); owned.set(weights, indices);
      }
      if (!found) throw new Error(`表情の描画先がありません: ${key}`);
      profile[key][targetName] = bind.weight / 100;
    }
    expressions.set(key, { bindings, binary: group.isBinary === true });
  }
  {
    // ＞＜ は保存しない一時の顔（docs/main-character-spec.md）。EXPRESSIONS には入れない。
    const bindings: Binding[] = [];
    profile.hurt = {};
    if (extraGroup) {
      for (const bind of extraGroup.binds) {
        const meshDefinition = source.parser.json.meshes?.find((item: { name?: string }) => item.name === sourceMeshNames[bind.mesh]);
        const targetName = meshDefinition?.extras?.targetNames?.[bind.index] as string | undefined;
        if (!targetName) throw new Error('表情の対応先がありません: hurt');
        for (const mesh of meshes) {
          const index = mesh.morphTargetDictionary?.[targetName];
          if (index === undefined || !mesh.morphTargetInfluences) continue;
          bindings.push({ weights: mesh.morphTargetInfluences, index, value: bind.weight / 100, name: targetName });
          const indices = owned.get(mesh.morphTargetInfluences) ?? new Set<number>();
          indices.add(index); owned.set(mesh.morphTargetInfluences, indices);
        }
        profile.hurt[targetName] = bind.weight / 100;
      }
    } else {
      bindings.push(...expressions.get('blink')!.bindings);
      Object.assign(profile.hurt, profile.blink);
      for (const mesh of hurtBoards) {
        const index = mesh.morphTargetDictionary![HURT_BOARD.name];
        bindings.push({ weights: mesh.morphTargetInfluences!, index, value: 1, name: HURT_BOARD.name });
        const indices = owned.get(mesh.morphTargetInfluences!) ?? new Set<number>();
        indices.add(index); owned.set(mesh.morphTargetInfluences!, indices);
      }
      if (hurtBoards.length) profile.hurt[HURT_BOARD.name] = 1;
    }
    if (!bindings.length) throw new Error('＞＜ の表情を読み込めませんでした。');
    expressions.set('hurt', { bindings, binary: false });
  }

  // Both original VRM0 models face -Z beneath the maker's Y-pi wrapper.
  // Their source lookAt ranges are inner 8 / outer 12 / up & down 10 degrees.
  // Positive raw yaw is the character's left; positive raw pitch is up.
  const eyes = (['L', 'R'] as const).map(side => {
    const bone = body.getObjectByName(`J_Adj_${side}_FaceEye`);
    if (!bone) throw new Error('視線の接続点がありません。');
    return { side, bone, rest: bone.quaternion.clone(), target: bone.quaternion.clone() };
  });
  const rotation = new Euler(0, 0, 0, 'YXZ');
  const applyEyeGaze = () => { eyes.forEach(({ bone, target }) => bone.quaternion.copy(target)); };
  const apply = (config: CharacterConfig, hurt = false) => {
    // Only authored expression targets belong to this writer. Appearance,
    // hair fitting and any unrelated animation weights keep their values.
    owned.forEach((indices, weights) => { indices.forEach(index => { weights[index] = 0; }); });
    // ＞＜ の間は、選んだ表情の代わりに ＞＜ を出す（保存している表情は変えない）。
    const key: string = hurt ? 'hurt' : config.expression ?? 'neutral', strength = hurt ? 1 : config.expressionStrength ?? 1;
    const selected = expressions.get(key);
    if (selected) {
      const value = selected.binary ? Number(strength >= 0.5) : strength;
      selected.bindings.forEach(binding => { binding.weights[binding.index] += value * binding.value; });
    }
    for (const correction of corrections) {
      const closes = key === 'blink' || key === 'happy' || key === 'hurt'
        || key === (correction.side === 'L' ? 'blinkLeft' : 'blinkRight');
      correction.mesh.morphTargetInfluences![correction.mesh.morphTargetDictionary![correction.name]] =
        closes && config.eyes === correction.eyes ? strength : 0;
    }
    for (const occlusion of eyeOcclusions) {
      // ＞＜ の板は目の奥へ下げない（板そのものを前へ出す）。
      const closes = key === 'blink' || key === 'happy' || (key === 'hurt' && !hurtBoards.includes(occlusion.mesh)) || key === (occlusion.side === 'L' ? 'blinkLeft' : 'blinkRight');
      const t = closes ? Math.max(0, Math.min(1, (strength - 0.75) / 0.25)) : 0;
      occlusion.mesh.morphTargetInfluences![occlusion.mesh.morphTargetDictionary![occlusion.name]] = t * t * (3 - 2 * t);
    }
    // Full-strength exported expression definitions include the same per-eye
    // appearance and surface corrections as the live assembly.
    for (const expression of ['blink', 'blinkLeft', 'blinkRight', 'happy', 'hurt']) {
      for (const correction of corrections) {
        const activeSide = expression === 'blink' || expression === 'happy' || expression === 'hurt' || expression === (correction.side === 'L' ? 'blinkLeft' : 'blinkRight');
        if (activeSide && config.eyes === correction.eyes) profile[expression][correction.name] = 1;
        else delete profile[expression][correction.name];
      }
      for (const occlusion of eyeOcclusions) {
        const activeSide = expression === 'blink' || expression === 'happy' || (expression === 'hurt' && !hurtBoards.includes(occlusion.mesh)) || expression === (occlusion.side === 'L' ? 'blinkLeft' : 'blinkRight');
        if (activeSide) profile[expression][occlusion.name] = 1;
        else if (!(expression === 'hurt' && hurtBoards.includes(occlusion.mesh))) delete profile[expression][occlusion.name];
      }
    }
    for (const eye of eyes) {
      const yaw = key === 'lookLeft' ? (eye.side === 'L' ? 12 : 8)
        : key === 'lookRight' ? (eye.side === 'L' ? -8 : -12) : 0;
      const pitch = key === 'lookUp' ? 10 : key === 'lookDown' ? -10 : 0;
      rotation.set(pitch * strength * Math.PI / 180, yaw * strength * Math.PI / 180, 0);
      eye.target.setFromEuler(rotation).premultiply(eye.rest);
    }
    applyEyeGaze();
  };
  return { apply, applyEyeGaze, profile, meshes, dispose: () => geometries.forEach(geometry => geometry.dispose()) };
}
