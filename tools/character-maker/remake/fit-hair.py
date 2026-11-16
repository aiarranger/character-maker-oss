# Tripo の「緑のマネキン＋髪」から髪だけを取り出し、今の頭へ合わせる。
# blender -b -P fit-hair.py -- <今のモデル.glb> <tripo.glb> <出力の接頭辞> [前後の追加の回転(度)]
import bpy, bmesh, sys, math, json
import numpy as np
from mathutils import Vector, Matrix
from mathutils.bvhtree import BVHTree
a = sys.argv[sys.argv.index('--') + 1:]
base, tripo, out = a[0], a[1], a[2]
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
# 合わせ先: 頭・顔の肌・体（首）の、今の姿の面
tv, tf = [], []
for o in sc.objects:
    if o.type != 'MESH' or not ('OriginalModularHead' in o.name or 'FaceSkin' in o.name or 'tripo' in o.name): continue
    ev = o.evaluated_get(dg); me = ev.to_mesh(); me.calc_loop_triangles(); n = len(tv)
    tv += [ev.matrix_world @ v.co for v in me.vertices]; tf += [tuple(n + i for i in t.vertices) for t in me.loop_triangles]
target = BVHTree.FromPolygons(tv, tf)
head_obj = next(o for o in sc.objects if o.type == 'MESH' and 'OriginalModularHead' in o.name)
hv = [head_obj.evaluated_get(dg).matrix_world @ v.co for v in head_obj.evaluated_get(dg).to_mesh().vertices]
hlo = Vector([min(p[i] for p in hv) for i in range(3)]); hhi = Vector([max(p[i] for p in hv) for i in range(3)])
print('HEAD', [round(x, 4) for x in hlo], [round(x, 4) for x in hhi])
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
# Tripo の正面は Blender の -Y。今のモデルの正面（+X）へ回す
me.transform(Matrix.Rotation(math.radians(90 + (float(a[3]) if len(a) > 3 else 0)), 4, 'Z'))
img = next(n.image for n in me.materials[0].node_tree.nodes if n.type == 'TEX_IMAGE' and n.image)
W, H = img.size; px = np.array(img.pixels[:], dtype=np.float32).reshape(H, W, 4)
uv = me.uv_layers.active.data
def is_green(u, v):
    c = px[int(min(max(v % 1.0, 0), 0.9999) * H), int(min(max(u % 1.0, 0), 0.9999) * W)]
    # 明るい緑だけでなく、目のくぼみなどの暗い緑も緑（髪は彩度の低いチャコール）
    return c[1] > 0.06 and c[1] > 1.25 * c[0] and c[1] > 1.25 * c[2]
green_face = []
for p in me.polygons:
    us = [uv[l].uv for l in p.loop_indices]
    cu = sum(u[0] for u in us) / len(us); cv = sum(u[1] for u in us) / len(us)
    votes = sum(is_green(u[0], u[1]) for u in us) + 2 * is_green(cu, cv)
    green_face.append(votes >= 3)
gv = set()
for p, g in zip(me.polygons, green_face):
    if g: gv.update(p.vertices)
hair_v = set()
for p, g in zip(me.polygons, green_face):
    if not g: hair_v.update(p.vertices)
print('FACES', len(me.polygons), 'green', sum(green_face))
G = np.array([me.vertices[i].co[:] for i in gv if i not in hair_v])
zmin, zmax = G[:, 2].min(), G[:, 2].max()
G = G[G[:, 2] > zmin + 0.30 * (zmax - zmin)]  # 台座と首の下は合わせに使わない
# はじめの合わせ: 耳から耳の幅と、緑の重心
s = (hhi.y - hlo.y) / (G[:, 1].max() - G[:, 1].min())
c0 = np.array([(hlo.x + hhi.x) / 2, 0, hlo.z + (hhi.z - hlo.z) * 0.45])
t = c0 - s * np.array([(G[:, 0].min() + G[:, 0].max()) / 2, (G[:, 1].min() + G[:, 1].max()) / 2, G[:, 2].mean()])
for it in range(60):
    P = G * s + t
    Q = np.array([target.find_nearest(Vector(p))[0][:] for p in P])
    d = np.linalg.norm(P - Q, axis=1); keep = d <= np.quantile(d, 0.8)
    p, q = G[keep], Q[keep]; pm, qm = p.mean(0), q.mean(0)
    s = float(((p - pm) * (q - qm)).sum() / ((p - pm) ** 2).sum()); t = qm - s * pm
    t[1] = 0 - s * 0 if abs(t[1]) < 0.002 else t[1]
print('FIT scale', round(s, 5), 'offset', [round(float(x), 5) for x in t], 'residual(mean of kept)', round(float(d[keep].mean()), 5), 'p80', round(float(np.quantile(d, 0.8)), 5))
me.transform(Matrix.Translation(Vector(t)) @ Matrix.Scale(s, 4))
# 緑の面を消して、髪だけにする
bm = bmesh.new(); bm.from_mesh(me); bm.faces.ensure_lookup_table()
bmesh.ops.delete(bm, geom=[f for f, g in zip(bm.faces, green_face) if g], context='FACES')
# 小さな離れ島（緑の縁の取り残し）を消す
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
drop = [v for i in islands if len(i) < 0.02 * big for v in i]
bmesh.ops.delete(bm, geom=drop, context='VERTS')
# 緑を面ごとに消した跡の縁はギザギザなので、縁の頂点だけ均す
bm.edges.ensure_lookup_table()
rim = list({v for e in bm.edges if len(e.link_faces) == 1 for v in e.verts})
for _ in range(4): bmesh.ops.smooth_vert(bm, verts=rim, factor=0.5, use_axis_x=True, use_axis_y=True, use_axis_z=True)
bm.to_mesh(me); bm.free(); me.update()
T.name = 'NewHair'
lo = Vector([min(v.co[i] for v in me.vertices) for i in range(3)]); hi = Vector([max(v.co[i] for v in me.vertices) for i in range(3)])
print('HAIR', len(me.vertices), 'verts', len(me.polygons), 'faces', [round(x, 4) for x in lo], [round(x, 4) for x in hi], 'islands', len(islands), 'dropped', len(drop))
# 髪の土台: 頭の面のうち、外へ向けて髪に当たる所だけを写し、少し外へ出す
dg = bpy.context.evaluated_depsgraph_get()
hair_bvh = BVHTree.FromObject(T, dg)
hm = head_obj.evaluated_get(dg).to_mesh(); mw = head_obj.evaluated_get(dg).matrix_world
centre = Vector(((hlo.x + hhi.x) / 2 - 0.01, 0, hlo.z + (hhi.z - hlo.z) * 0.55))
cover = []
for v in hm.vertices:
    p = mw @ v.co; d = (p - centre).normalized()
    hit = hair_bvh.ray_cast(p + d * 0.0005, d, 0.06)
    cover.append(hit[0] is not None)
cap = bpy.data.meshes.new('NewHairCap'); verts = []; idx = {}; faces = []
for poly in hm.polygons:
    if all(cover[i] for i in poly.vertices):
        f = []
        for i in poly.vertices:
            if i not in idx:
                p = mw @ hm.vertices[i].co; idx[i] = len(verts); verts.append(p + (p - centre).normalized() * 0.0012)
            f.append(idx[i])
        faces.append(f)
cap.from_pydata(verts, [], faces); cap.update()
# 土台の縁も面ごとのギザギザなので均す（生え際・耳のまわりに暗いのこぎり歯が出ないように）
_cb = bmesh.new(); _cb.from_mesh(cap); _cb.edges.ensure_lookup_table()
_rim = list({v for e in _cb.edges if len(e.link_faces) == 1 for v in e.verts})
for _ in range(6): bmesh.ops.smooth_vert(_cb, verts=_rim, factor=0.5, use_axis_x=True, use_axis_y=True, use_axis_z=True)
_cb.to_mesh(cap); _cb.free(); cap.update()
for p in cap.polygons: p.use_smooth = True
C = bpy.data.objects.new('NewHairCap', cap); sc.collection.objects.link(C)
mat = bpy.data.materials.new('NewHairCapMat'); mat.use_nodes = True
# 土台の色: 髪のテクスチャの暗いほうの平均
hp = px[::8, ::8, :3].reshape(-1, 3); hp = hp[~((hp[:, 1] > 0.3) & (hp[:, 1] > 1.6 * hp[:, 0]))]; hp = hp[hp.sum(1) < 2.4]
col = np.quantile(hp, 0.35, axis=0)
mat.node_tree.nodes['Principled BSDF'].inputs['Base Color'].default_value = (*[float(x) for x in col], 1)
cap.materials.append(mat)
print('CAP', len(verts), 'verts', len(faces), 'faces', 'color', [round(float(x), 4) for x in col])
# 軽くする（このあと絵を焼くので、形を先に確定する）
for o, ratio in ((T, min(1.0, 26000 / max(1, len(me.polygons)))), (C, min(1.0, 5000 / max(1, len(cap.polygons))))):
    m = o.modifiers.new('dec', 'DECIMATE'); m.ratio = ratio
    bpy.context.view_layer.objects.active = o
    for x in sc.objects: x.select_set(x is o)
    bpy.ops.object.modifier_apply(modifier='dec')
# 髪の絵を作り直す: Tripo の絵は陰を消してあるので束が読めず、色も原画からずれる。
# 決まった髪色（argv の #rrggbb）に、くぼみの陰（AO を焼く）と、元の絵の明暗（髪留めなどの濃淡）を掛ける。色替えは絵を灰色にして色を掛けるので、明暗だけが効く
tone = np.array([int(a[4][k:k + 2], 16) / 255 for k in (1, 3, 5)]) if len(a) > 4 and a[4].startswith('#') else np.array([float(x) for x in col])
SIZE = 1024
ao = bpy.data.images.new('NewHairAO', SIZE, SIZE)
hmat = me.materials[0]; nodes = hmat.node_tree.nodes
bake_node = nodes.new('ShaderNodeTexImage'); bake_node.image = ao; nodes.active = bake_node
sc.render.engine = 'CYCLES'; sc.cycles.samples = 48; sc.cycles.device = 'CPU'
bpy.context.view_layer.objects.active = T
for x in sc.objects: x.select_set(x is T)
for x in sc.objects:
    if x.type == 'MESH' and x is not T: x.hide_render = True
bpy.ops.object.bake(type='AO', margin=12)
for x in sc.objects: x.hide_render = False
nodes.remove(bake_node)
aop = np.array(ao.pixels[:], dtype=np.float32).reshape(SIZE, SIZE, 4)[:, :, 0]
img.scale(SIZE, SIZE)
px = np.array(img.pixels[:], dtype=np.float32).reshape(SIZE, SIZE, 4)
lum = 0.2126 * px[:, :, 0] + 0.7152 * px[:, :, 1] + 0.0722 * px[:, :, 2]
green = (px[:, :, 1] > 0.06) & (px[:, :, 1] > 1.25 * px[:, :, 0]) & (px[:, :, 1] > 1.25 * px[:, :, 2])
med = float(np.median(lum[~green])) or 1.0
rel = np.clip(np.where(green, 1.0, lum / med), 0.45, 1.25)           # 元の絵の明暗（中央値を 1 に）
occ = np.clip(aop / max(1e-3, float(np.quantile(aop[aop > 0.02], 0.8))), 0.0, 1.08) ** 1.5   # くぼみほど暗く
shade = np.clip((0.42 + 0.66 * occ) * (0.55 + 0.45 * rel), 0.3, 1.2)
out_px = np.ones((SIZE, SIZE, 4), dtype=np.float32)
out_px[:, :, :3] = np.clip(tone[None, None, :] * shade[:, :, None] * 1.12, 0, 1)
img.pixels[:] = out_px.reshape(-1); img.file_format = 'PNG'; img.pack()
col = tone * 0.62
mat.node_tree.nodes['Principled BSDF'].inputs['Base Color'].default_value = (*[float(x) for x in col], 1)
ci = bpy.data.images.new('NewHairCapColor', 8, 8); ci.pixels[:] = [float(col[0]), float(col[1]), float(col[2]), 1.0] * 64; ci.file_format = 'PNG'; ci.pack()
tn = mat.node_tree.nodes.new('ShaderNodeTexImage'); tn.image = ci
mat.node_tree.links.new(tn.outputs['Color'], mat.node_tree.nodes['Principled BSDF'].inputs['Base Color'])
cap.uv_layers.new(name='UVMap')
print('TEXTURE tone', tone.round(3), 'ao q80', round(float(np.quantile(aop[aop > 0.02], 0.8)), 3), 'shade', np.quantile(shade, [0.05, 0.5, 0.95]).round(3))
json.dump({'scale': s, 'offset': [float(x) for x in t], 'capColorLinear': [float(x) for x in col]}, open(out + '-fit.json', 'w'))
for o in sc.objects: o.select_set(o in (T, C))
bpy.ops.export_scene.gltf(filepath=out + '.glb', use_selection=True, export_format='GLB', export_apply=True, export_yup=True, export_animations=False, export_skins=False, export_image_format='AUTO')
# 確かめの絵（陰なし）
for m in bpy.data.materials:
    if not m.use_nodes: continue
    ns = m.node_tree.nodes; bs = next((n for n in ns if n.type == 'BSDF_PRINCIPLED'), None)
    if not bs: continue
    c = bs.inputs['Base Color']; e = ns.new('ShaderNodeEmission')
    if c.is_linked: m.node_tree.links.new(c.links[0].from_socket, e.inputs['Color'])
    else: e.inputs['Color'].default_value = c.default_value
    m.node_tree.links.new(e.outputs[0], next(n for n in ns if n.type == 'OUTPUT_MATERIAL').inputs['Surface'])
cam = bpy.data.objects.new('cam', bpy.data.cameras.new('cam')); sc.collection.objects.link(cam); sc.camera = cam
cam.data.type = 'ORTHO'; cam.data.ortho_scale = 0.34
sc.render.resolution_x = sc.render.resolution_y = 640; sc.render.engine = 'BLENDER_EEVEE'
sc.world = bpy.data.worlds.new('w'); sc.world.use_nodes = True; sc.world.node_tree.nodes['Background'].inputs[0].default_value = (0.8, 0.8, 0.8, 1)
sc.view_settings.view_transform = 'Standard'
cc = Vector(((hlo.x + hhi.x) / 2, 0, (hlo.z + hhi.z) / 2 + 0.01))
for name, d in {'front': Vector((1, 0, 0)), 'q': Vector((1, -1, 0.15)).normalized(), 'side': Vector((0, -1, 0)), 'back': Vector((-1, 0, 0)), 'top': Vector((0.3, 0, 1)).normalized()}.items():
    cam.location = cc + d * 3; cam.rotation_euler = (-d).to_track_quat('-Z', 'Y').to_euler()
    sc.render.filepath = f'{out}-{name}.png'; bpy.ops.render.render(write_still=True)
