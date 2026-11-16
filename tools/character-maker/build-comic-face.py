#!/usr/bin/env python3
"""アメコミの顔の形を直した体の GLB を作る（眉とまつ毛の位置と太さ）。docs/game-motions-spec.md
  python3 tools/character-maker/build-comic-face.py
元の GLB（parts/assets/v1/{feminine,masculine}.glb）は変えない。頂点の数・UV・骨の重み・元のモーフの差分はそのままで、位置を動かし、表情のモーフ（happy・angry・sad・relaxed・surprised・blinkLeft・blinkRight）を足した写しを作る。
出力: parts/assets/comic-face-v1/{base}.glb と manifest.json（public/maker-assets/comic-soft-{base}.glb へは pnpm assets:sync が写す）。

元の形では、眉（maker-brow の大きい部品）の下の端が目玉の上 1/4 にかぶさり、まつ毛の線（同じメッシュの小さい部品）が
目玉の中央の少し上を横切る。これで「眉をひそめた半目」に見える。
- 眉: 上へ動かし、太さ（上下）を細くし、眉頭を上げて「ハの字のひそめ」をゆるめる
- 鼻先と唇: 前への張り出しを少し引く（鼻の付け根・鼻の穴のふち・口角は動かさない）
- まぶた（maker-head の目のふち）とまつ毛の線: 上まぶたのふちを上げ、下まぶたを少し下げて、目の開きを広げる（元は目玉 30mm に対して開き 10mm）
numpy を使う。"""
import hashlib, json, struct, sys
from pathlib import Path
import numpy as np

ROOT = Path(__file__).resolve().parents[2]
# base ごとの値（m）。brow_up: 眉を上げる量、brow_thin: 眉の太さの倍率（男性の太い眉は個性として太めに残す）、brow_level: 眉頭を上げる回転（度）、
# nose_back: 鼻先を奥へ引く量（まわり 14mm でなだらかに 0）、lips_back: 唇の張り出しを奥へ引く量、
# lid_up・lid_down: 上まぶた・下まぶたのふちを上・下へ動かす量（目の開きを広げる。まつ毛の線も同じだけ動く）
SETTINGS = {
    'masculine': {'brow_up': 0.0070, 'brow_thin': 0.78, 'brow_level': 4.0, 'lid_up': 0.0036, 'lid_down': 0.0010, 'nose_back': 0.0040, 'lips_back': 0.0015, 'blink': 0.0122},
    'feminine': {'brow_up': 0.0065, 'brow_thin': 0.65, 'brow_level': 3.5, 'lid_up': 0.0036, 'lid_down': 0.0010, 'nose_back': 0.0035, 'lips_back': 0.0015, 'blink': 0.0138},
}
COMPONENT = {5126: 'f4', 5123: 'u2', 5125: 'u4', 5121: 'u1'}
COUNT = {'SCALAR': 1, 'VEC2': 2, 'VEC3': 3, 'VEC4': 4, 'MAT4': 16}

class Glb:
    def __init__(self, data: bytes):
        length = struct.unpack_from('<I', data, 12)[0]
        self.json = json.loads(data[20:20 + length]); self.bin = bytearray(data[28 + length:])
    def _loc(self, index):
        a = self.json['accessors'][index]; v = self.json['bufferViews'][a['bufferView']]
        dtype = COMPONENT[a['componentType']]; n = COUNT[a['type']]
        return a, dtype, n, v.get('byteOffset', 0) + a.get('byteOffset', 0), v.get('byteStride') or np.dtype(dtype).itemsize * n
    def read(self, index):
        a = self.json['accessors'][index]
        if 'sparse' in a:
            # 疎な値（元のモーフ）: 0 の上に、番号と値を置く
            n = COUNT[a['type']]; out = np.zeros((a['count'], n), dtype=COMPONENT[a['componentType']]); sp = a['sparse']
            def raw(view_index, byte_offset, dtype, count, width):
                v = self.json['bufferViews'][view_index]; start = v.get('byteOffset', 0) + byte_offset
                return np.frombuffer(self.bin, dtype=dtype, count=count * width, offset=start).reshape(count, width).copy()
            at = raw(sp['indices']['bufferView'], sp['indices'].get('byteOffset', 0), COMPONENT[sp['indices']['componentType']], sp['count'], 1)[:, 0]
            out[at] = raw(sp['values']['bufferView'], sp['values'].get('byteOffset', 0), COMPONENT[a['componentType']], sp['count'], n)
            return out
        a, dtype, n, offset, stride = self._loc(index)
        raw = np.frombuffer(self.bin, dtype='u1')
        at = offset + np.arange(a['count'])[:, None] * stride + np.arange(np.dtype(dtype).itemsize * n)[None, :]
        return raw[at].copy().view(dtype).reshape(a['count'], n)
    def write(self, index, values):
        a, dtype, n, offset, stride = self._loc(index)
        rows = np.ascontiguousarray(values, dtype=dtype).view('u1').reshape(a['count'], -1)
        for k in range(a['count']): self.bin[offset + k * stride:offset + k * stride + rows.shape[1]] = rows[k].tobytes()
        if 'min' in a: a['min'] = [float(x) for x in np.asarray(values, dtype=dtype).min(0)]; a['max'] = [float(x) for x in np.asarray(values, dtype=dtype).max(0)]
    def add_target(self, mesh, name, delta):
        """モーフを 1 つ足す（位置の差分。頂点の数は形と同じ）"""
        m = next(item for item in self.json['meshes'] if item['name'] == mesh)
        data = np.ascontiguousarray(delta, dtype='f4')
        while len(self.bin) % 4: self.bin.append(0)
        self.json['bufferViews'].append({'buffer': 0, 'byteOffset': len(self.bin), 'byteLength': data.nbytes})
        self.bin.extend(data.tobytes())
        self.json['accessors'].append({'name': f'{mesh}-{name}', 'bufferView': len(self.json['bufferViews']) - 1, 'componentType': 5126, 'count': len(data), 'type': 'VEC3',
                                       'min': [float(x) for x in data.min(0)], 'max': [float(x) for x in data.max(0)]})
        m['primitives'][0].setdefault('targets', []).append({'POSITION': len(self.json['accessors']) - 1})
        m.setdefault('weights', []).append(0); m.setdefault('extras', {}).setdefault('targetNames', []).append(name)
    def _append(self, data, target=None):
        while len(self.bin) % 4: self.bin.append(0)
        view = {'buffer': 0, 'byteOffset': len(self.bin), 'byteLength': data.nbytes}
        if target: view['target'] = target
        self.json['bufferViews'].append(view); self.bin.extend(data.tobytes())
        return len(self.json['bufferViews']) - 1
    def add_accessor(self, values, component, kind, target=None, bounds=False):
        data = np.ascontiguousarray(values, dtype=COMPONENT[component])
        accessor = {'bufferView': self._append(data, target), 'componentType': component, 'count': len(data), 'type': kind}
        if bounds: accessor['min'] = [float(x) for x in data.reshape(len(data), -1).min(0)]; accessor['max'] = [float(x) for x in data.reshape(len(data), -1).max(0)]
        self.json['accessors'].append(accessor)
        return len(self.json['accessors']) - 1
    def target(self, mesh, name):
        m = next(item for item in self.json['meshes'] if item['name'] == mesh)
        return self.read(m['primitives'][0]['targets'][m['extras']['targetNames'].index(name)]['POSITION']).astype(np.float64)
    def primitive(self, mesh): return next(m for m in self.json['meshes'] if m['name'] == mesh)['primitives'][0]
    def bytes(self):
        binary = bytes(self.bin); binary += b'\0' * ((4 - len(binary) % 4) % 4)
        self.json['buffers'][0]['byteLength'] = len(binary)
        text = json.dumps(self.json, separators=(',', ':'), ensure_ascii=False).encode(); text += b' ' * ((4 - len(text) % 4) % 4)
        return struct.pack('<III', 0x46546C67, 2, 28 + len(text) + len(binary)) + struct.pack('<II', len(text), 0x4E4F534A) + text + struct.pack('<II', len(binary), 0x004E4942) + binary

def components(positions, indices):
    """位置が同じ頂点をまとめて、つながった部品ごとの頂点の番号を返す"""
    first = {}; rep = np.array([first.setdefault(tuple(v), i) for i, v in enumerate(np.round(positions, 5))])
    parent = list(range(len(positions)))
    def find(x):
        while parent[x] != x: parent[x] = parent[parent[x]]; x = parent[x]
        return x
    for a, b, c in indices.reshape(-1, 3): ra = find(rep[a]); parent[find(rep[b])] = ra; parent[find(rep[c])] = ra
    groups = {}
    for i in range(len(positions)): groups.setdefault(find(rep[i]), []).append(i)
    return [np.array(g) for g in groups.values()]

def smooth(a, b, x):
    t = np.clip((x - a) / (b - a), 0, 1); return t * t * (3 - 2 * t)

def lid_shift(points, eye, s, sides=(-1.0, 1.0)):
    """目のふちを上下へ広げる量（y の差）。目の開きの中心からの高さで決め、ふちで最大、眉の高さと頬へ向けて 0 に戻す。横は目の幅の外で 0"""
    out = np.zeros(len(points))
    for side in sides:
        half = eye[np.sign(eye[:, 0]) == side]
        cx = (half[:, 0].min() + half[:, 0].max()) / 2; cy = (half[:, 1].min() + half[:, 1].max()) / 2 - 0.0015; front = half[:, 2].max()
        u = points[:, 1] - cy
        across = 1 - smooth(0.013, 0.023, np.abs(points[:, 0] - cx))
        depth = smooth(front - 0.045, front - 0.030, points[:, 2])
        up = smooth(0.0, 0.0045, u) * (1 - smooth(0.0045, 0.019, u))
        down = smooth(0.0, 0.0050, -u) * (1 - smooth(0.0050, 0.014, -u))
        out += (s['lid_up'] * up - s['lid_down'] * down) * across * depth
    return out

def reshape(glb: Glb, s: dict):
    brow = glb.primitive('maker-brow'); eyes = glb.primitive('maker-eyes')
    p = glb.read(brow['attributes']['POSITION']).astype(np.float64)
    eye = glb.read(eyes['attributes']['POSITION'])
    eye_centre_y = float((eye[:, 1].min() + eye[:, 1].max()) / 2)
    report = []
    eye = eye.astype(np.float64)
    for part in components(p, glb.read(brow['indices'])):
        q = p[part]; centre = q.mean(0)
        side = 1.0 if centre[0] > 0 else -1.0
        if centre[1] < eye_centre_y + (eye[:, 1].max() - eye_centre_y) * 0.55 or q[:, 1].max() - q[:, 1].min() < 0.012 and centre[1] < eye[:, 1].max():
            # まつ毛の線: まぶたのふちと同じだけ動かす
            q = q + np.stack([np.zeros(len(q)), lid_shift(q, eye, s), np.zeros(len(q))], axis=1); kind = 'lash'
        else:
            # 眉: 太さを細くし、眉頭（顔の中心の側）を上げ、全体を上げる
            angle = -np.radians(s['brow_level']) * side  # 本人の左（x > 0）は時計回りで眉頭（x の小さい側）が上がる
            x, y = q[:, 0] - centre[0], (q[:, 1] - centre[1]) * s['brow_thin']
            q = np.stack([centre[0] + x * np.cos(angle) - y * np.sin(angle), centre[1] + x * np.sin(angle) + y * np.cos(angle) + s['brow_up'], q[:, 2]], axis=1); kind = 'brow'
        report.append({'kind': kind, 'side': 'L' if side > 0 else 'R', 'vertices': int(len(part)), 'y': [round(float(q[:, 1].min()), 4), round(float(q[:, 1].max()), 4)]})
        p[part] = q
    glb.write(brow['attributes']['POSITION'], p.astype('f4'))
    head = glb.primitive('maker-head'); h = glb.read(head['attributes']['POSITION']).astype(np.float64)
    shift = lid_shift(h, eye, s); h[:, 1] += shift
    # 鼻先: 目より下・顔の中心でいちばん前の頂点。まわりをなだらかに奥へ
    eye_y = (eye[:, 1].min() + eye[:, 1].max()) / 2
    band = (np.abs(h[:, 0]) < 0.012) & (h[:, 1] < eye_y - 0.012) & (h[:, 1] > eye_y - 0.065)
    tip = h[np.where(band)[0][np.argmax(h[band][:, 2])]].copy()
    tip[0] = 0.0  # 左右対称にする
    near = np.sqrt(((h[:, 0] - tip[0]) / 0.014) ** 2 + ((h[:, 1] - tip[1]) / 0.014) ** 2 + ((h[:, 2] - tip[2]) / 0.022) ** 2)
    nose = s['nose_back'] * (1 - smooth(0.15, 1.0, near))
    # 唇: 鼻先より下で、顔の中心でいちばん前の頂点のまわり（横に広く、縦に狭く）
    below = (np.abs(h[:, 0]) < 0.012) & (h[:, 1] < tip[1] - 0.016) & (h[:, 1] > tip[1] - 0.045)
    lip = h[np.where(below)[0][np.argmax(h[below][:, 2])]].copy()
    lip[0] = 0.0
    around = np.sqrt(((h[:, 0] - lip[0]) / 0.020) ** 2 + ((h[:, 1] - lip[1]) / 0.012) ** 2 + ((h[:, 2] - lip[2]) / 0.016) ** 2)
    lips = s['lips_back'] * (1 - smooth(0.2, 1.0, around))
    h[:, 2] -= nose + lips
    report.append({'kind': 'nose-lips', 'nose_tip': [round(float(v), 4) for v in tip], 'lips': [round(float(v), 4) for v in lip], 'nose_vertices': int((nose > 1e-5).sum()), 'lip_vertices': int((lips > 1e-5).sum())})
    glb.write(head['attributes']['POSITION'], h.astype('f4'))
    report.append({'kind': 'lids', 'vertices': int((np.abs(shift) > 1e-5).sum()), 'max_up': round(float(shift.max()), 4), 'max_down': round(float(-shift.min()), 4)})
    return {'eye': {'y': [round(float(eye[:, 1].min()), 4), round(float(eye[:, 1].max()), 4)]}, 'parts': report}

# 表情のモーフ（名前はゲーム側の表情の名前と同じ。lib/character-maker/config.ts の COMIC_EXPRESSIONS）。値は m・度。
# smile・full: 元の口のモーフ（mouthSmile・mouthFull）を足す量、brow_up・brow_inner: 眉を上げる量・眉頭を上げる回転、
# corner: 口角を上げる量（負で下げる）、lid_up・lid_down: 上まぶたを上げる量・下まぶたを下げる量（負で閉じる向き）
EXPRESSIONS = {
    'happy': {'smile': 1.6, 'corner': 0.0035, 'brow_up': 0.0012, 'lid_up': -0.0008, 'lid_down': -0.0018},
    'angry': {'smile': -0.5, 'corner': -0.0018, 'brow_up': -0.0036, 'brow_inner': -12.0, 'lid_up': -0.0020},
    'sad': {'smile': -1.0, 'corner': -0.0030, 'brow_up': 0.0008, 'brow_inner': 11.0, 'lid_up': -0.0016},
    'relaxed': {'smile': 0.6, 'corner': 0.0020, 'brow_up': 0.0006, 'lid_up': -0.0020},
    'surprised': {'full': 0.8, 'brow_up': 0.0055, 'lid_up': 0.0026, 'lid_down': 0.0010},
    # まばたき: 上まぶたを下げる量は体ごと（SETTINGS の blink。目の開きが違う）
    'blinkLeft': {'lid_up': 'blink', 'lid_down': -0.0022, 'sides': (1.0,)},
    'blinkRight': {'lid_up': 'blink', 'lid_down': -0.0022, 'sides': (-1.0,)},
}

def add_expressions(glb: Glb, original_head, original_brow, eye, lip, s):
    """直した形の上に、表情のモーフを足す。まぶたの動く範囲は、元の形の位置で決める（頂点の番号は同じ）"""
    brow = glb.primitive('maker-brow')
    now = glb.read(brow['attributes']['POSITION']).astype(np.float64)
    parts = components(original_brow, glb.read(brow['indices']))
    smile, full = glb.target('maker-head', 'mouthSmile'), glb.target('maker-head', 'mouthFull')
    eye_centre_y = float((eye[:, 1].min() + eye[:, 1].max()) / 2)
    for name, e in EXPRESSIONS.items():
        sides = e.get('sides', (-1.0, 1.0)); lids = {'lid_up': -s['blink'] if e.get('lid_up') == 'blink' else e.get('lid_up', 0.0), 'lid_down': e.get('lid_down', 0.0)}
        closing = max(0.0, -lids['lid_up'])
        def lid_delta(points):
            dy = lid_shift(points, eye, lids, sides)
            # 閉じるときは、まぶたが目玉の丸みに沿うように少し前へ出す
            dz = np.clip(-dy, 0, None) * (0.28 if closing > 0.005 else 0.0)
            return np.stack([np.zeros(len(points)), dy, dz], axis=1)
        head = lid_delta(original_head) + smile * e.get('smile', 0.0) + full * e.get('full', 0.0)
        # 口角: 唇の中心から左右 21mm の所を中心に、まわり 11mm でなだらかに
        for side in (-1.0, 1.0):
            d = np.sqrt(((original_head[:, 0] - side * 0.021) / 0.011) ** 2 + ((original_head[:, 1] - lip[1]) / 0.011) ** 2 + ((original_head[:, 2] - (lip[2] - 0.010)) / 0.020) ** 2)
            head[:, 1] += e.get('corner', 0.0) * (1 - smooth(0.15, 1.0, d))
        delta = np.zeros_like(now)
        for part in parts:
            q0 = original_brow[part]; centre0 = q0.mean(0); side = 1.0 if centre0[0] > 0 else -1.0
            if centre0[1] < eye_centre_y + (eye[:, 1].max() - eye_centre_y) * 0.55 or q0[:, 1].max() - q0[:, 1].min() < 0.012 and centre0[1] < eye[:, 1].max():
                delta[part] = lid_delta(q0)
            elif side in sides and 'sides' not in e:
                q = now[part]; centre = q.mean(0); angle = -np.radians(e.get('brow_inner', 0.0)) * side
                x, y = q[:, 0] - centre[0], q[:, 1] - centre[1]
                delta[part] = np.stack([centre[0] + x * np.cos(angle) - y * np.sin(angle) - q[:, 0], centre[1] + x * np.sin(angle) + y * np.cos(angle) + e.get('brow_up', 0.0) - q[:, 1], np.zeros(len(q))], axis=1)
        glb.add_target('maker-head', name, head)
        glb.add_target('maker-brow', name, delta)
        glb.add_target('maker-eyes', name, np.zeros((len(eye), 3)))
    # ---- 視線: 目玉（maker-eyes）を、それぞれの中心のまわりに回す（左右 12°・上下 10°。左右は本人から見た向き） ----
    eyes_mesh = glb.primitive('maker-eyes'); ep = glb.read(eyes_mesh['attributes']['POSITION']).astype(np.float64)
    for name, (yaw, pitch) in {'lookLeft': (12.0, 0.0), 'lookRight': (-12.0, 0.0), 'lookUp': (0.0, -10.0), 'lookDown': (0.0, 10.0)}.items():
        delta = np.zeros_like(ep)
        for side in (-1.0, 1.0):
            mask = np.sign(ep[:, 0]) == side; q = ep[mask]; centre = (q.min(0) + q.max(0)) / 2
            a, b = np.radians(yaw), np.radians(pitch)
            ry = np.array([[np.cos(a), 0, np.sin(a)], [0, 1, 0], [-np.sin(a), 0, np.cos(a)]]); rx = np.array([[1, 0, 0], [0, np.cos(b), -np.sin(b)], [0, np.sin(b), np.cos(b)]])
            delta[mask] = (q - centre) @ (ry @ rx).T + centre - q
        glb.add_target('maker-eyes', name, delta)
        for mesh, count in (('maker-head', len(original_head)), ('maker-brow', len(original_brow))): glb.add_target(mesh, name, np.zeros((count, 3)))
    # ---- 口の形（あ・い・う・え・お）: 口の中のメッシュは無いので、下唇とあごを下げ、口の幅を変えて形を出す ----
    def mouth(drop, wide, forward):
        d = np.zeros_like(original_head)
        u = (original_head[:, 0]) / 0.024; v = original_head[:, 1] - lip[1]
        # 下唇より下（あごまで）を下げる。唇の線のすぐ上は動かさない
        lower = smooth(0.0005, -0.004, v) * (1 - smooth(-0.028, -0.062, v)) * (1 - smooth(0.6, 1.5, np.abs(u))) * smooth(lip[2] - 0.05, lip[2] - 0.02, original_head[:, 2])
        d[:, 1] -= drop * lower
        # 口の幅: 唇のまわりを左右へ広げる・すぼめる
        near = (1 - smooth(0.0, 0.016, np.abs(v))) * (1 - smooth(0.9, 1.7, np.abs(u))) * smooth(lip[2] - 0.045, lip[2] - 0.015, original_head[:, 2])
        d[:, 0] += wide * 0.024 * u * near
        d[:, 2] += forward * near * (1 - np.clip(np.abs(u), 0, 1) * 0.6)
        return d
    for name, (drop, wide, forward) in {'aa': (0.0105, 0.02, 0.0), 'ih': (0.0024, 0.10, -0.0005), 'ou': (0.0036, -0.30, 0.0040), 'ee': (0.0050, 0.06, 0.0), 'oh': (0.0068, -0.30, 0.0030)}.items():
        glb.add_target('maker-head', name, mouth(drop, wide, forward))
        glb.add_target('maker-brow', name, np.zeros((len(original_brow), 3)))
        glb.add_target('maker-eyes', name, np.zeros((len(ep), 3)))
    add_mouth(glb, lip)
    return list(EXPRESSIONS) + ['lookLeft', 'lookRight', 'lookUp', 'lookDown', 'aa', 'ih', 'ou', 'ee', 'oh']

# 唇の線（上下の唇の境目）は、唇でいちばん前に出た点からこれだけ下（m）
MOUTH_SEAM = 0.0030
# 口の中（maker-mouth）: 口の形ごとの開き（横の半径・縦の半径。m）。唇の線のすぐ下に、暗い色の楕円を唇の前へ出す
MOUTH = {'aa': (0.0125, 0.0055), 'ih': (0.0165, 0.0013), 'ou': (0.0048, 0.0020), 'ee': (0.0150, 0.0026), 'oh': (0.0068, 0.0036), 'surprised': (0.0060, 0.0030)}

def add_mouth(glb: Glb, lip):
    """口の中のメッシュを足す。元の頭には口の中が無く、唇も開かないので、暗い色の楕円（ふだんは 1 点につぶれて見えない）を
    口の形のモーフで広げる。頭の骨だけに付ける"""
    j = glb.json; rim = 24
    head_node = next(i for i, n in enumerate(j['nodes']) if n.get('name') == 'maker-head')
    skin = j['nodes'][head_node]['skin']
    head_joint = next(k for k, node in enumerate(j['skins'][skin]['joints']) if j['nodes'][node].get('name') == 'Head')
    seam = [0.0, lip[1] - MOUTH_SEAM, lip[2] + 0.0009]
    count = rim + 1
    base = np.tile(np.array(seam, dtype='f4'), (count, 1))
    indices = np.array([[0, 1 + k, 1 + (k + 1) % rim] for k in range(rim)], dtype='u2').reshape(-1)
    joints = np.zeros((count, 4), dtype='u1'); joints[:, 0] = head_joint
    weights = np.zeros((count, 4), dtype='f4'); weights[:, 0] = 1
    j['materials'].append({'name': 'maker-mouth', 'pbrMetallicRoughness': {'baseColorFactor': [0.105, 0.028, 0.03, 1], 'metallicFactor': 0, 'roughnessFactor': 1}, 'extensions': {'KHR_materials_unlit': {}}, 'doubleSided': True})
    primitive = {'attributes': {'POSITION': glb.add_accessor(base, 5126, 'VEC3', 34962, True), 'TEXCOORD_0': glb.add_accessor(np.zeros((count, 2), dtype='f4'), 5126, 'VEC2', 34962),
                                'JOINTS_0': glb.add_accessor(joints, 5121, 'VEC4', 34962), 'WEIGHTS_0': glb.add_accessor(weights, 5126, 'VEC4', 34962)},
                 'indices': glb.add_accessor(indices, 5123, 'SCALAR', 34963), 'material': len(j['materials']) - 1, 'mode': 4}
    j['meshes'].append({'name': 'maker-mouth', 'primitives': [primitive], 'weights': [], 'extras': {'targetNames': []}})
    j['nodes'].append({'name': 'maker-mouth', 'mesh': len(j['meshes']) - 1, 'skin': skin, 'translation': [0, 0, 0], 'rotation': [0, 0, 0, 1], 'scale': [1, 1, 1]})
    parent = next(n for n in j['nodes'] if head_node in n.get('children', []))
    parent['children'].append(len(j['nodes']) - 1)
    angle = np.arange(rim) * 2 * np.pi / rim
    for name, (a, b) in MOUTH.items():
        delta = np.zeros((count, 3), dtype='f4')
        # 中心は唇の線から縦の半径ぶん下。ふちは唇の丸みに沿って少し奥へ
        delta[0] = [0, -b, 0]
        delta[1:, 0] = a * np.cos(angle); delta[1:, 1] = -b + b * np.sin(angle); delta[1:, 2] = -7.0 * (a * np.cos(angle)) ** 2
        glb.add_target('maker-mouth', name, delta)


records = []
out_dir = ROOT / 'parts/assets/comic-face-v1'; out_dir.mkdir(parents=True, exist_ok=True)
for base, settings in SETTINGS.items():
    source = (ROOT / f'parts/assets/v1/{base}.glb').read_bytes()
    glb = Glb(source)
    original = {name: glb.read(glb.primitive(name)['attributes']['POSITION']).astype(np.float64) for name in ('maker-head', 'maker-brow', 'maker-eyes')}
    measured = reshape(glb, settings)
    lip = next(part['lips'] for part in measured['parts'] if part['kind'] == 'nose-lips')
    measured['expressions'] = add_expressions(glb, original['maker-head'], original['maker-brow'], original['maker-eyes'], lip, settings)
    data = glb.bytes(); out = out_dir / f'{base}.glb'; out.write_bytes(data)
    records.append({'base': base, 'settings': settings, 'measured': measured, 'input': {'path': f'parts/assets/v1/{base}.glb', 'sha256': hashlib.sha256(source).hexdigest()},
                    'output': {'path': str(out.relative_to(ROOT)), 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}})
(out_dir / 'manifest.json').write_text(json.dumps({'version': 'comic-face-v1', 'expressions': {name: {k: (list(v) if isinstance(v, tuple) else v) for k, v in e.items()} for name, e in EXPRESSIONS.items()}, 'license': 'CC0 (derived from Quaternius Universal Base Characters)', 'assets': records}, indent=2) + '\n')
runtime_path = ROOT / 'parts/assets/studio-runtime-v1/manifest.json'
runtime = json.loads(runtime_path.read_text())
for record in records:
    public = f"public/maker-assets/comic-soft-{record['base']}.glb"
    entry = {'public': public, 'source': record['output']['path'], 'sha256': record['output']['sha256'], 'bytes': record['output']['bytes'], 'kind': 'maker-asset'}
    runtime['entries'] = [item for item in runtime['entries'] if item['public'] != public] + [entry]
runtime_path.write_text(json.dumps(runtime, indent=2, ensure_ascii=False) + '\n')
json.dump([{'base': r['base'], 'bytes': r['output']['bytes'], 'expressions': r['measured']['expressions']} for r in records], sys.stdout, indent=1)
