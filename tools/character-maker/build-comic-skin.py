#!/usr/bin/env python3
"""アメコミの肌の絵（maker-skin）の、細かいむらと濃い陰をならした版を作る。docs/game-motions-spec.md
  python3 tools/character-maker/build-comic-skin.py
元の GLB（parts/assets/v1/{feminine,masculine}.glb）は変えない。出力: parts/assets/comic-skin-v1/{base}-soft-skin.png と manifest.json。
public/maker-assets/comic-{base}-soft-skin.png へは pnpm assets:sync が写す（parts/assets/studio-runtime-v1/manifest.json の一覧も、この道具が書き直す）。
- 細かいむら（毛穴・無精ひげ・筋の線）: まわりとの差が小さい所だけ、ぼかした色へ寄せる。唇・鼻の穴・目のふちのような大きい差は残す
- 広い陰（目のくぼみ・鼻の横・頬）: まわりより暗い所だけ、明るさを半分戻す
Pillow と numpy を使う。"""
import hashlib, json, struct, sys
from io import BytesIO
from pathlib import Path
import numpy as np
from PIL import Image, ImageFilter

ROOT = Path(__file__).resolve().parents[2]
SETTINGS = {'fineRadius': 3.5, 'fineKeep': [0.03, 0.075], 'wideRadius': 16, 'wideDark': 0.2, 'wideAmount': 0.5}
LUMA = np.array([.299, .587, .114], dtype=np.float32)

def skin_image(glb: bytes) -> bytes:
    length = struct.unpack_from('<I', glb, 12)[0]
    doc = json.loads(glb[20:20 + length]); binary = glb[28 + length:]
    material = next(m for m in doc['materials'] if m['name'] == 'maker-skin')
    image = doc['images'][doc['textures'][material['pbrMetallicRoughness']['baseColorTexture']['index']]['source']]
    view = doc['bufferViews'][image['bufferView']]
    return binary[view.get('byteOffset', 0):view.get('byteOffset', 0) + view['byteLength']]

def clean(source: bytes) -> Image.Image:
    image = Image.open(BytesIO(source)).convert('RGBA'); alpha = image.split()[3]; rgb = image.convert('RGB')
    blur = lambda radius: np.asarray(rgb.filter(ImageFilter.GaussianBlur(radius))).astype(np.float32) / 255
    x = np.asarray(rgb).astype(np.float32) / 255
    fine, wide = blur(SETTINGS['fineRadius']), blur(SETTINGS['wideRadius'])
    lo, hi = SETTINGS['fineKeep']
    keep = np.clip((np.abs(x - fine).max(axis=2, keepdims=True) - lo) / (hi - lo), 0, 1); keep = keep * keep * (3 - 2 * keep)
    y = fine + (x - fine) * keep
    dark = np.clip(((wide @ LUMA) - (y @ LUMA)) / SETTINGS['wideDark'], 0, 1)[..., None] * (1 - keep)
    y = y + (wide - y) * dark * SETTINGS['wideAmount']
    return Image.merge('RGBA', (*Image.fromarray((np.clip(y, 0, 1) * 255 + .5).astype(np.uint8)).split(), alpha))

records = []
for base in ('feminine', 'masculine'):
    source = (ROOT / f'parts/assets/v1/{base}.glb').read_bytes()
    out = ROOT / f'parts/assets/comic-skin-v1/{base}-soft-skin.png'
    out.parent.mkdir(parents=True, exist_ok=True)
    clean(skin_image(source)).save(out, optimize=True)
    records.append({'base': base, 'input': {'path': f'parts/assets/v1/{base}.glb', 'sha256': hashlib.sha256(source).hexdigest()},
                    'output': {'path': str(out.relative_to(ROOT)), 'bytes': out.stat().st_size, 'sha256': hashlib.sha256(out.read_bytes()).hexdigest()}})
(ROOT / 'parts/assets/comic-skin-v1/manifest.json').write_text(json.dumps({'version': 'comic-skin-v1', 'settings': SETTINGS, 'license': 'CC0 (derived from Quaternius Universal Base Characters)', 'assets': records}, indent=2) + '\n')
# 実行時素材の一覧（public/ へ写す元）を書き直す
runtime_path = ROOT / 'parts/assets/studio-runtime-v1/manifest.json'
runtime = json.loads(runtime_path.read_text())
for record in records:
    public = f"public/maker-assets/comic-{record['base']}-soft-skin.png"
    entry = {'public': public, 'source': record['output']['path'], 'sha256': record['output']['sha256'], 'bytes': record['output']['bytes'], 'kind': 'maker-asset'}
    runtime['entries'] = [item for item in runtime['entries'] if item['public'] != public] + [entry]
runtime_path.write_text(json.dumps(runtime, indent=2, ensure_ascii=False) + '\n')
json.dump(records, sys.stdout, indent=1)
