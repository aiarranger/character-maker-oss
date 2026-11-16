# Tripo の「緑の頭＋体」から体だけを取り出し、今の体へ合わせる（骨は今のまま使うので、位置と大きさを今の体に重ねる）。
# blender -b -P fit-body.py -- <今のモデル.glb> <tripo.glb> <出力の接頭辞> [三角形の数]
import bpy, bmesh, sys, math, json
import numpy as np
from mathutils import Vector, Matrix
from mathutils.bvhtree import BVHTree
a = sys.argv[sys.argv.index('--') + 1:]
base, tripo, out = a[0], a[1], a[2]
tris = int(a[3]) if len(a) > 3 else 30000
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=base)
sc = bpy.context.scene
for o in sc.objects:
    if o.animation_data: o.animation_data_clear()
    if o.type == 'ARMATURE':
        for b in o.pose.bones: b.location = (0, 0, 0); b.rotation_quaternion = (1, 0, 0, 0); b.scale = (1, 1, 1)
for o in list(sc.objects):
    if o.type == 'MESH' and ('Hair' in o.name or 'lob' in o.name): bpy.data.objects.remove(o, do_unlink=True)
bpy.context.view_layer.update()
dg = bpy.context.evaluated_depsgraph_get()
body = next(o for o in sc.objects if o.type == 'MESH' and o.name.startswith('tripo'))
ev = body.evaluated_get(dg); bm_ = ev.to_mesh(); bm_.calc_loop_triangles()
tv = [ev.matrix_world @ v.co for v in bm_.vertices]; tf = [tuple(t.vertices) for t in bm_.loop_triangles]
target = BVHTree.FromPolygons(tv, tf)
B = np.array([p[:] for p in tv]); blo, bhi = B.min(0), B.max(0)
print('BODY', blo.round(4), bhi.round(4))
before = set(sc.objects)
bpy.ops.import_scene.gltf(filepath=tripo)
T = next(o for o in sc.objects if o not in before and o.type == 'MESH')
bpy.context.view_layer.objects.active = T
for o in sc.objects: o.select_set(o is T)
T.parent = None
bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
me = T.data
# glTF は絵の継ぎ目で頂点が分かれている。同じ位置の頂点をつないでから、島や縁を調べる（UV は面の角ごとなので変わらない）
_bm = bmesh.new(); _bm.from_mesh(me); bmesh.ops.remove_doubles(_bm, verts=_bm.verts, dist=1e-6); _bm.to_mesh(me); _bm.free(); me.update()
me.transform(Matrix.Rotation(math.radians(90), 4, 'Z'))
img = next(n.image for n in me.materials[0].node_tree.nodes if n.type == 'TEX_IMAGE' and n.image)
W, H = img.size; px = np.array(img.pixels[:], dtype=np.float32).reshape(H, W, 4)
uv = me.uv_layers.active.data
def is_green(u, v):
    c = px[int(min(max(v % 1.0, 0), 0.9999) * H), int(min(max(u % 1.0, 0), 0.9999) * W)]
    return c[1] > 0.06 and c[1] > 1.25 * c[0] and c[1] > 1.25 * c[2]
green_face = []
for p in me.polygons:
    us = [uv[l].uv for l in p.loop_indices]
    cu = sum(u[0] for u in us) / len(us); cv = sum(u[1] for u in us) / len(us)
    green_face.append(sum(is_green(u[0], u[1]) for u in us) + 2 * is_green(cu, cv) >= 3)
keepv = set()
for p, g in zip(me.polygons, green_face):
    if not g: keepv.update(p.vertices)
P0 = np.array([me.vertices[i].co[:] for i in sorted(keepv)])
print('FACES', len(me.polygons), 'green', sum(green_face), 'tripo body', P0.min(0).round(4), P0.max(0).round(4))
# はじめの合わせ: 足もとと、体（緑を除く）の高さ。左右の中心は 0
s = (bhi[2] - blo[2]) / (P0[:, 2].max() - P0[:, 2].min())
t = np.array([(blo[0] + bhi[0]) / 2 - s * (P0[:, 0].min() + P0[:, 0].max()) / 2, -s * (P0[:, 1].min() + P0[:, 1].max()) / 2, blo[2] - s * P0[:, 2].min()])
G = P0[::3]
for it in range(40):
    P = G * s + t
    Q = np.array([target.find_nearest(Vector(p))[0][:] for p in P])
    d = np.linalg.norm(P - Q, axis=1); keep = d <= np.quantile(d, 0.85)
    p, q = G[keep], Q[keep]; pm, qm = p.mean(0), q.mean(0)
    s = float(((p - pm) * (q - qm)).sum() / ((p - pm) ** 2).sum()); t = qm - s * pm
print('FIT scale', round(s, 5), 'offset', t.round(5), 'residual mean', round(float(d.mean()), 5), 'p50', round(float(np.quantile(d, .5)), 5), 'p95', round(float(np.quantile(d, .95)), 5))
me.transform(Matrix.Translation(Vector(t)) @ Matrix.Scale(s, 4))
bm = bmesh.new(); bm.from_mesh(me); bm.faces.ensure_lookup_table()
bmesh.ops.delete(bm, geom=[f for f, g in zip(bm.faces, green_face) if g], context='FACES')
bm.verts.ensure_lookup_table(); seen = set(); islands = []
for v in bm.verts:
    if v in seen: continue
    stack = [v]; isl = []; seen.add(v)
    while stack:
        x = stack.pop(); isl.append(x)
        for e in x.link_edges:
            y = e.other_vert(x)
            if y not in seen: seen.add(y); stack.append(y)
    islands.append(isl)
big = max(len(i) for i in islands)
bmesh.ops.delete(bm, geom=[v for i in islands if len(i) < 0.01 * big for v in i], context='VERTS')
bm.to_mesh(me); bm.free(); me.update()
T.name = 'NewBody'
# 絵: 緑のにじみを近くの服の色へ寄せるのは難しいので、暗い灰色にしておく（頭と襟の内側で隠れる）
# 顔の肌色（今の頭の絵の、明るい画素の中央値）
himg = next(n.image for mm in bpy.data.materials if mm.name.startswith('Modular head skin') for n in mm.node_tree.nodes if n.type == 'TEX_IMAGE' and n.image)
hp = np.array(himg.pixels[:], dtype=np.float32).reshape(-1, 4)[::16, :3]; hp = hp[(hp[:, 0] > 0.75) & (hp[:, 0] > hp[:, 2] + 0.08)]
face_skin = np.median(hp, axis=0)
# 手の肌（服より明るく、赤みのある画素。差し色の黄土は彩度が高いので外す）を、顔の肌色へ寄せる
r, g, b = px[:, :, 0], px[:, :, 1], px[:, :, 2]
skin = (r > 0.62) & (r > g) & (g > b) & (r - b > 0.12) & (r - b < 0.52) & (g - b > 0.03) & (g > 0.42)
if skin.sum() > 500:
    mu = np.median(px[skin][:, :3], axis=0)
    px[skin, :3] = np.clip(px[skin][:, :3] * (face_skin / mu), 0, 1)
    print('SKIN pixels', int(skin.sum()), 'from', mu.round(3), 'to', face_skin.round(3))
gm = (g > 0.06) & (g > 1.25 * r) & (g > 1.25 * b)
px[gm, :3] = face_skin; px[:, :, 3] = 1
# 襟の内側（首の軸を向いた面）は Tripo の絵が乱れているので、上着の暗い色で塗る
blue = px[(b > r + 0.06) & (b > 0.2)][:, :3]
inner = np.median(blue, axis=0) * 0.72 if len(blue) else np.array([0.2, 0.26, 0.33])
if len(a) > 4 and a[4].startswith('#'): inner = np.array([int(a[4][k:k + 2], 16) / 255 for k in (1, 3, 5)])  # 襟の内側の色の指定
uv = me.uv_layers.active.data  # 面を消したあとは取り直す
Vn = np.array([v.co[:] for v in me.vertices]); zs = Vn[:, 2]
top = zs.max(); axis = Vn[zs > top - 0.03][:, :2].mean(0)
painted = 0
for p in me.polygons:
    c = np.array(p.center[:]); n = np.array(p.normal[:])
    to_axis = np.array([axis[0] - c[0], axis[1] - c[1], 0.0]); dist = np.linalg.norm(to_axis)
    if c[2] < top - 0.11 or dist > 0.11 or dist < 1e-5: continue
    if np.dot(n, to_axis / dist) < 0.15: continue
    uvs = np.array([[uv[l].uv[0] * W, uv[l].uv[1] * H] for l in p.loop_indices])
    x0, y0 = np.floor(uvs.min(0)).astype(int) - 1; x1, y1 = np.ceil(uvs.max(0)).astype(int) + 1
    x0, y0, x1, y1 = max(x0, 0), max(y0, 0), min(x1, W - 1), min(y1, H - 1)
    if x1 <= x0 or y1 <= y0 or (x1 - x0) * (y1 - y0) > 400000: continue
    xs, ys = np.meshgrid(np.arange(x0, x1 + 1) + 0.5, np.arange(y0, y1 + 1) + 0.5)
    a0, b0, c0 = uvs[0], uvs[1], uvs[2]
    den = (b0[1] - c0[1]) * (a0[0] - c0[0]) + (c0[0] - b0[0]) * (a0[1] - c0[1])
    if abs(den) < 1e-9: continue
    w0 = ((b0[1] - c0[1]) * (xs - c0[0]) + (c0[0] - b0[0]) * (ys - c0[1])) / den
    w1 = ((c0[1] - a0[1]) * (xs - c0[0]) + (a0[0] - c0[0]) * (ys - c0[1])) / den
    inside = (w0 >= -0.15) & (w1 >= -0.15) & (w0 + w1 <= 1.15)
    px[y0:y1 + 1, x0:x1 + 1][inside, :3] = inner; painted += 1
# 前の開きの縁などに残る黄緑のにじみ（マネキンの緑が服の色と混ざった所）も、同じ暗い色へ
r, g, b = px[:, :, 0], px[:, :, 1], px[:, :, 2]
yg = (g >= 0.97 * r) & (g > 1.22 * b) & (g > 0.25)
px[yg, :3] = inner
img.pixels[:] = px.reshape(-1); img.pack()
print('YELLOWGREEN pixels', int(yg.sum()))
print('COLLAR inner faces painted', painted, 'colour', inner.round(3), 'axis', axis.round(4), 'top', round(float(top), 4))
img.pixels[:] = px.reshape(-1); img.pack()
m = T.modifiers.new('dec', 'DECIMATE'); m.ratio = min(1.0, tris / max(1, len(me.polygons)))
V = np.array([v.co[:] for v in me.vertices])
print('NEWBODY', len(me.vertices), 'verts', len(me.polygons), 'faces', V.min(0).round(4), V.max(0).round(4), 'islands', len(islands))
for o in sc.objects: o.select_set(o is T)
bpy.ops.export_scene.gltf(filepath=out + '.glb', use_selection=True, export_format='GLB', export_apply=True, export_yup=True, export_animations=False, export_skins=False)
# 確かめの絵: 新しい体（今の頭つき）と、今の体を半透明の代わりに並べずに重ねて描く
body.hide_render = True
for mm in bpy.data.materials:
    if not mm.use_nodes: continue
    ns = mm.node_tree.nodes; bs = next((n for n in ns if n.type == 'BSDF_PRINCIPLED'), None)
    if not bs: continue
    c = bs.inputs['Base Color']; e = ns.new('ShaderNodeEmission')
    if c.is_linked: mm.node_tree.links.new(c.links[0].from_socket, e.inputs['Color'])
    else: e.inputs['Color'].default_value = c.default_value
    mm.node_tree.links.new(e.outputs[0], next(n for n in ns if n.type == 'OUTPUT_MATERIAL').inputs['Surface'])
cam = bpy.data.objects.new('cam', bpy.data.cameras.new('cam')); sc.collection.objects.link(cam); sc.camera = cam
cam.data.type = 'ORTHO'; cam.data.ortho_scale = 1.12
sc.render.resolution_x = sc.render.resolution_y = 900; sc.render.engine = 'BLENDER_EEVEE'
sc.world = bpy.data.worlds.new('w'); sc.world.use_nodes = True; sc.world.node_tree.nodes['Background'].inputs[0].default_value = (0.8, 0.8, 0.8, 1)
sc.view_settings.view_transform = 'Standard'
cc = Vector((0, 0, 0.5))
for name, d in {'front': Vector((1, 0, 0)), 'side': Vector((0, -1, 0)), 'back': Vector((-1, 0, 0))}.items():
    cam.location = cc + d * 4; cam.rotation_euler = (-d).to_track_quat('-Z', 'Y').to_euler()
    sc.render.filepath = f'{out}-{name}.png'; bpy.ops.render.render(write_still=True)
body.hide_render = False; T.hide_render = True
cam.location = cc + Vector((1, 0, 0)) * 4; cam.rotation_euler = Vector((-1, 0, 0)).to_track_quat('-Z', 'Y').to_euler()
sc.render.filepath = f'{out}-oldfront.png'; bpy.ops.render.render(write_still=True)
