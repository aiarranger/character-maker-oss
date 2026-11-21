// three の形を JSON に書き出す（Rust の tests/geometry.rs が同じ引数で作って比べる）。
// 実行: node crates/island-kit/tests/fixtures/gen-geometry.mjs（three は pengo の node_modules）
import { writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, '../../../../../pengo/node_modules/three');
const THREE = await import(join(root, 'build/three.module.js'));
const { RoundedBoxGeometry } = await import(join(root, 'examples/jsm/geometries/RoundedBoxGeometry.js'));
const { mergeGeometries } = await import(join(root, 'examples/jsm/utils/BufferGeometryUtils.js'));

const PI = Math.PI;
const V2 = (x, y) => new THREE.Vector2(x, y);
const V3 = (x, y, z) => new THREE.Vector3(x, y, z);
const out = {};
const r = a => Array.from(a, v => Number(v.toFixed(6)));
function geo(name, g) {
  g.computeBoundingSphere();
  const s = g.boundingSphere;
  out[name] = {
    position: r(g.attributes.position.array),
    normal: g.attributes.normal ? r(g.attributes.normal.array) : [],
    uv: g.attributes.uv ? r(g.attributes.uv.array) : [],
    index: g.index ? Array.from(g.index.array) : null,
    groups: g.groups.map(x => [x.start, x.count, x.materialIndex]),
    sphere: r([s.center.x, s.center.y, s.center.z, s.radius]),
  };
}
const pts = (name, list) => { out[name] = { points: r(list.flatMap(p => (p.z === undefined ? [p.x, p.y] : [p.x, p.y, p.z]))) }; };

// ---- 基本の形 ----
geo('plane', new THREE.PlaneGeometry(2, 3));
geo('plane_seg', new THREE.PlaneGeometry(2, 3, 2, 3));
geo('box', new THREE.BoxGeometry(1, 1, 1));
geo('box_seg', new THREE.BoxGeometry(2, 3, 4, 1, 2, 3));
geo('sphere', new THREE.SphereGeometry(1, 8, 6));
geo('sphere_part', new THREE.SphereGeometry(0.5, 6, 4, 0.3, 2, 0.4, 1.2));
geo('sphere_top', new THREE.SphereGeometry(1, 8, 4, 0, PI * 2, 0, PI / 2));
geo('sphere_bottom', new THREE.SphereGeometry(1, 8, 4, 0, PI * 2, PI / 2, PI / 2));
geo('cylinder', new THREE.CylinderGeometry(1, 1, 2, 8));
geo('cylinder_seg', new THREE.CylinderGeometry(0.5, 1, 2, 6, 2));
geo('cylinder_open', new THREE.CylinderGeometry(1, 0.5, 2, 6, 1, true));
geo('cylinder_part', new THREE.CylinderGeometry(1, 1, 1, 6, 1, false, 0.5, 3));
geo('cylinder_tip', new THREE.CylinderGeometry(0, 1, 2, 5, 2));
geo('cone', new THREE.ConeGeometry(1, 2, 6));
geo('cone_full', new THREE.ConeGeometry(0.5, 1, 8, 2, true, 0.2, 4));
geo('circle', new THREE.CircleGeometry(1, 8));
geo('circle_part', new THREE.CircleGeometry(2, 5, 0.5, 2));
geo('ring', new THREE.RingGeometry(0.5, 1, 8));
geo('ring_full', new THREE.RingGeometry(0.3, 1.2, 6, 3, 0.4, 3));
geo('torus', new THREE.TorusGeometry(1, 0.4, 6, 8));
geo('torus_arc', new THREE.TorusGeometry(1, 0.2, 4, 6, 3));
const lathePts = [V2(0, -1), V2(1, -0.5), V2(0.8, 0.5), V2(0, 1)];
geo('lathe', new THREE.LatheGeometry(lathePts, 8));
geo('lathe_part', new THREE.LatheGeometry(lathePts, 5, 0.5, 2));
geo('capsule', new THREE.CapsuleGeometry(0.5, 1, 3, 6));
geo('capsule_full', new THREE.CapsuleGeometry(1, 2, 2, 5, 3));
geo('icosa', new THREE.IcosahedronGeometry(1, 0));
geo('icosa_1', new THREE.IcosahedronGeometry(2, 1));
geo('icosa_2', new THREE.IcosahedronGeometry(1, 2));
geo('octa', new THREE.OctahedronGeometry(1, 0));
geo('octa_2', new THREE.OctahedronGeometry(1.5, 2));
geo('tetra', new THREE.TetrahedronGeometry(1, 0));
geo('tetra_1', new THREE.TetrahedronGeometry(1, 1));
geo('dodeca', new THREE.DodecahedronGeometry(1, 0));
geo('dodeca_1', new THREE.DodecahedronGeometry(1, 1));
geo('rounded', new RoundedBoxGeometry(1, 1, 1, 2, 0.1));
geo('rounded_wide', new RoundedBoxGeometry(2, 1, 0.5, 3, 0.2));
geo('rounded_big_r', new RoundedBoxGeometry(1, 1, 1, 1, 5));
geo('rounded_0', new RoundedBoxGeometry(1, 2, 3, 0, 0.1));

// ---- 平らな形（Shape） ----
function chevron() { const s = new THREE.Shape(); s.moveTo(-0.26, 0); s.lineTo(0, 0.2); s.lineTo(0.26, 0); s.lineTo(0.26, 0.09); s.lineTo(0, 0.29); s.lineTo(-0.26, 0.09); return s; }
function star(R, r0) { // pengo の starShape
  const s = new THREE.Shape();
  const pt = (i, rr) => { const a = PI / 2 + i * PI / 5; return V2(Math.cos(a) * rr, Math.sin(a) * rr); };
  for (let i = 0; i < 10; i++) {
    const cur = pt(i, i % 2 ? r0 : R), nxt = pt(i + 1, (i + 1) % 2 ? r0 : R);
    const k = i % 2 ? 0.0 : 0.22;
    const a = cur.clone().lerp(nxt, k), b = cur.clone().lerp(nxt, 1 - ((i + 1) % 2 ? 0.0 : 0.22));
    if (i === 0) s.moveTo(a.x, a.y); else s.quadraticCurveTo(cur.x, cur.y, a.x, a.y);
    s.lineTo(b.x, b.y);
  }
  const p0 = pt(0, R), a0 = p0.clone().lerp(pt(1, r0), 0.22);
  s.quadraticCurveTo(p0.x, p0.y, a0.x, a0.y);
  return s;
}
function bird() { const s = new THREE.Shape(); s.moveTo(0, 0.18); s.bezierCurveTo(0.8, 0.5, 1.7, 0.45, 2.3, 0.15); s.lineTo(2.1, -0.05); s.bezierCurveTo(1.0, -0.35, 0.4, -0.3, 0, -0.18); s.closePath(); return s; }
function arch() { const s = new THREE.Shape(); s.moveTo(-0.45, 0); s.lineTo(-0.45, 1.1); s.absarc(0, 1.1, 0.45, PI, 0, true); s.lineTo(0.45, 0); s.closePath(); return s; }
function holed() { const s = new THREE.Shape(); s.moveTo(-1, -1); s.lineTo(1, -1); s.lineTo(1, 1); s.lineTo(-1, 1); s.closePath(); const h = new THREE.Path(); h.absarc(0.2, 0.1, 0.4, 0, PI * 2, false); s.holes.push(h); const h2 = new THREE.Path(); h2.moveTo(-0.8, -0.8); h2.lineTo(-0.5, -0.8); h2.lineTo(-0.5, -0.5); h2.lineTo(-0.8, -0.5); h2.closePath(); s.holes.push(h2); return s; }
function disc(rad) { const s = new THREE.Shape(); s.absarc(0, 0, rad, 0, PI * 2, false); return s; }
function discHoled() { const s = disc(2); const h = new THREE.Path(); h.absarc(0.5, 0, 0.6, 0, PI * 2, true); s.holes.push(h); const h2 = new THREE.Path(); h2.absarc(-0.9, 0.2, 0.5, 0, PI * 2, false); s.holes.push(h2); return s; }
function bowtie() { return new THREE.Shape([V2(0, 0), V2(2, 2), V2(2, 0), V2(0, 2), V2(-1, 1)]); }
function selfCross() { return new THREE.Shape([V2(0, 0), V2(4, 0), V2(1, 3), V2(2, -1), V2(3, 3), V2(0, 1), V2(4, 1.5), V2(-1, 2)]); }
function cw() { return new THREE.Shape([V2(0, 0), V2(0, 1), V2(1, 1), V2(1, 0)]); }
function splined() { const s = new THREE.Shape(); s.moveTo(0, 0); s.splineThru([V2(1, 0.5), V2(2, 0), V2(1.5, 1.5)]); s.lineTo(0, 1); s.ellipse(0, -0.5, 0.5, 0.3, PI / 2, PI * 1.5, false, 0.3); return s; }
function rr(ww, hh, rad) { const sh = new THREE.Shape(); const x = -ww / 2, y = 0; sh.moveTo(x + rad, y); sh.lineTo(x + ww - rad, y); sh.quadraticCurveTo(x + ww, y, x + ww, y + rad); sh.lineTo(x + ww, y + hh - rad); sh.quadraticCurveTo(x + ww, y + hh, x + ww - rad, y + hh); sh.lineTo(x + rad, y + hh); sh.quadraticCurveTo(x, y + hh, x, y + hh - rad); sh.lineTo(x, y + rad); sh.quadraticCurveTo(x, y, x + rad, y); return sh; }
function frame() { const outer = rr(2, 3, 0.2); const inner = rr(1.6, 2.6, 0.15); outer.holes.push(new THREE.Path(inner.getPoints(8).map(p => V2(p.x, p.y + 0.13)))); return outer; }

// 星の頂点の sin・cos（倍精度のまま）。Mac の libm は V8 と 1 ulp 違うことがあり、ほぼ重なる点の数が変わるので、比べるときは同じ値から作る
out.star_input = { cossin: Array.from({ length: 11 }, (_, i) => { const a = PI / 2 + i * PI / 5; return [Math.cos(a), Math.sin(a)]; }) };
pts('path_points_arch', arch().getPoints(8));
pts('path_points_bird', bird().getPoints(6));
pts('path_points_spline', splined().getPoints(4));
geo('shape_chevron', new THREE.ShapeGeometry(chevron()));
geo('shape_star', new THREE.ShapeGeometry(star(0.62, 0.32), 6));
geo('shape_bird', new THREE.ShapeGeometry(bird(), 8));
geo('shape_arch', new THREE.ShapeGeometry(arch(), 8));
geo('shape_holed', new THREE.ShapeGeometry(holed(), 10));
geo('shape_disc_big', new THREE.ShapeGeometry(disc(1.5), 48));
geo('shape_disc_holed_big', new THREE.ShapeGeometry(discHoled(), 32));
geo('shape_bowtie', new THREE.ShapeGeometry(bowtie()));
geo('shape_cross', new THREE.ShapeGeometry(selfCross()));
geo('shape_cw', new THREE.ShapeGeometry(cw()));
geo('shape_spline', new THREE.ShapeGeometry(splined(), 6));
geo('shape_array', new THREE.ShapeGeometry([chevron(), arch()], 4));

// ---- 押し出し ----
geo('extrude_default', new THREE.ExtrudeGeometry(chevron()));
geo('extrude_star', new THREE.ExtrudeGeometry(star(0.62, 0.32), { depth: 0.2, bevelEnabled: true, bevelThickness: 0.12, bevelSize: 0.1, bevelSegments: 3, curveSegments: 4 }));
geo('extrude_flat', new THREE.ExtrudeGeometry(arch(), { depth: 0.5, bevelEnabled: false, curveSegments: 6 }));
geo('extrude_steps', new THREE.ExtrudeGeometry(chevron(), { depth: 1, steps: 3, bevelEnabled: false }));
geo('extrude_steps_bevel', new THREE.ExtrudeGeometry(chevron(), { depth: 1, steps: 2, bevelEnabled: true, bevelThickness: 0.05, bevelSize: 0.04, bevelOffset: 0.01, bevelSegments: 2 }));
geo('extrude_holed', new THREE.ExtrudeGeometry(holed(), { depth: 0.3, bevelEnabled: true, bevelThickness: 0.04, bevelSize: 0.03, bevelSegments: 2, curveSegments: 8 }));
geo('extrude_frame', new THREE.ExtrudeGeometry(frame(), { depth: 0.16, bevelEnabled: true, bevelThickness: 0.04, bevelSize: 0.04, bevelSegments: 2, curveSegments: 6 }));
geo('extrude_big', new THREE.ExtrudeGeometry(disc(1), { depth: 0.1, bevelEnabled: false, curveSegments: 48 }));
geo('extrude_cw', new THREE.ExtrudeGeometry(cw(), { depth: 1, bevelEnabled: true, bevelThickness: 0.1, bevelSize: 0.05, bevelSegments: 1 }));
geo('extrude_size_default', new THREE.ExtrudeGeometry(cw(), { depth: 1, bevelThickness: 0.3, bevelSegments: 2 }));
geo('extrude_array', new THREE.ExtrudeGeometry([chevron(), cw()], { depth: 0.5, bevelEnabled: false }));

// ---- 曲線と管 ----
const cr = [V3(0, 0, 0), V3(1, 2, 0.5), V3(2, 0.5, 1), V3(4, 1, -1), V3(3, -1, 0)];
const crv = (o = {}) => new THREE.CatmullRomCurve3(cr, o.closed ?? false, o.type ?? 'centripetal', o.tension ?? 0.5);
pts('curve_points', crv().getPoints(10));
pts('curve_points_at', [0, 0.1, 0.33, 0.5, 0.77, 1].map(u => crv().getPointAt(u)));
pts('curve_tangents_at', [0, 0.1, 0.33, 0.5, 0.77, 1].map(u => crv().getTangentAt(u)));
pts('curve_spaced', crv({ type: 'chordal' }).getSpacedPoints(7));
pts('curve_closed', crv({ closed: true }).getPoints(11));
pts('curve_catmullrom', crv({ type: 'catmullrom', tension: 0.1 }).getPoints(9));
pts('curve_two', new THREE.CatmullRomCurve3([V3(0, 0, 0), V3(1, 1, 0)]).getPoints(4));
out.curve_length = { points: r([crv().getLength(), crv({ closed: true }).getLength()]) };
geo('tube', new THREE.TubeGeometry(crv(), 8, 0.2, 4));
geo('tube_chordal', new THREE.TubeGeometry(crv({ type: 'chordal' }), 6, 0.1, 5));
geo('tube_catmullrom', new THREE.TubeGeometry(crv({ type: 'catmullrom', tension: 0.1 }), 10, 0.12, 5));
geo('tube_closed', new THREE.TubeGeometry(crv({ closed: true }), 12, 0.3, 3, true));
geo('tube_two', new THREE.TubeGeometry(new THREE.CatmullRomCurve3([V3(0, 0, 0), V3(1, 1, 0)]), 4, 0.1, 4));

// ---- 変形 ----
const boxS = () => new THREE.BoxGeometry(1, 2, 3, 1, 1, 1);
geo('rotate_x', boxS().rotateX(0.7));
geo('rotate_y', boxS().rotateY(-1.1));
geo('rotate_z', boxS().rotateZ(2.3));
geo('translate', boxS().translate(1, -2, 3));
geo('scale', new THREE.SphereGeometry(1, 6, 4).scale(2, 0.5, 3));
geo('center', new THREE.SphereGeometry(1, 6, 4, 0, PI, 0, PI / 2).translate(3, 4, 5).center());
geo('translate_lathe', new THREE.LatheGeometry(lathePts, 4).translate(0, 1, 0));
const mat = new THREE.Matrix4().compose(V3(1, 2, 3), new THREE.Quaternion().setFromEuler(new THREE.Euler(0.3, 0.5, 0.7)), V3(2, 0.5, 1.5));
out.matrix = { points: r(mat.elements) };
geo('apply_matrix4', new THREE.CylinderGeometry(1, 0.5, 2, 5).applyMatrix4(mat));
geo('normals_indexed', (() => { const g = new THREE.SphereGeometry(1, 6, 4); g.computeVertexNormals(); return g; })());
geo('normals_soup', (() => { const g = new THREE.SphereGeometry(1, 6, 4).toNonIndexed(); g.computeVertexNormals(); return g; })());
geo('normals_new', (() => { const g = new THREE.BufferGeometry(); g.setAttribute('position', new THREE.BufferAttribute(new Float32Array([0, 0, 0, 1, 0, 0, 0, 1, 0, 1, 1, 0.5]), 3)); g.setIndex([0, 1, 2, 2, 1, 3]); g.computeVertexNormals(); return g; })());
geo('non_indexed', new THREE.BoxGeometry(1, 1, 1, 2, 1, 1).toNonIndexed());
geo('merge', mergeGeometries([new THREE.BoxGeometry(1, 1, 1), new THREE.PlaneGeometry(2, 2).translate(0, 0, 2)]));
geo('merge_groups', mergeGeometries([new THREE.BoxGeometry(1, 1, 1), new THREE.PlaneGeometry(2, 2)], true));
geo('merge_soup', mergeGeometries([new THREE.IcosahedronGeometry(1, 0), new THREE.TetrahedronGeometry(1, 0)], true));
out.merge_bad = { isNull: mergeGeometries([new THREE.BoxGeometry(1, 1, 1), new THREE.IcosahedronGeometry(1, 0)]) === null };

writeFileSync(join(here, 'geometry.json'), JSON.stringify(out));
console.log(Object.keys(out).length, 'cases');
