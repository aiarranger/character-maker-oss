// The generic-character material rule of the 4255 viewer: every material except Accessory_ is drawn without lights
// (MeshBasicMaterial), as RigActor did before b2d7f820 and as aisns draws the choosable characters. With lights,
// Haru's and Nagi's untextured eye-fold skin showed as a box around the eyes and Lux's screen went dark (2026-10-01).
// Each kind keeps its own copy of its rule and never imports another kind's file (Owner, 2026-10-01).
// Used by RigActor (components/rig-actor/generic/RigActor.tsx, the generic copy of the 4255 viewer) and
// tools/character-checks/viewer_shot.mjs, which serves this file to a browser page as it is: plain JavaScript, and
// 'three' is its only import.
// Frozen with the generic viewer copy (components/rig-actor/kind-copies.test.mjs pins its SHA-256).
import * as THREE from 'three';

/**
 * @param {import('three').MeshStandardMaterial} m a material of the loaded GLB
 * @returns {import('three').Material} the material the viewer draws
 */
export default function genericViewerMaterial(m) {
  return m.name.startsWith('Accessory_') ? m.clone() : new THREE.MeshBasicMaterial({ map: m.map, color: m.color, side: m.side, vertexColors: m.vertexColors, transparent: m.transparent, opacity: m.opacity, alphaTest: m.alphaTest, depthWrite: m.depthWrite, toneMapped: false });
}
