#!/usr/bin/env python3
"""Rust 版の画面の文字（egui）に使う字だけの字体を作る（muso の subset.py と同じ作り）。
元のファイルは src/ に置く（リポジトリには入れない）:
  Zen Maru Gothic（Medium・Bold・Black）・Dela Gothic One は Google Fonts（github.com/google/fonts の ofl/）から、
  Fredoka は可変の字体 Fredoka[wdth,wght].ttf を src/Fredoka-VF.ttf に置き、太さ 500・600・700 の固定の字体へ切り出す（今の版の Google Fonts の読み込みと同じ太さ）。
字: かな・英数・記号の全部 + ソースと HTML に出てくる字 + 主役の名前（配布の一覧から）+ よく使う漢字（JIS 第一水準）。"""
import glob, os, subprocess, sys, json, urllib.request
here = os.path.dirname(os.path.abspath(__file__))
root = os.path.abspath(os.path.join(here, '..', '..', '..', '..'))
src = os.path.join(here, 'src')
for w in (500, 600, 700):
    out = os.path.join(src, f'Fredoka-{w}.ttf')
    if not os.path.exists(out):
        subprocess.run([sys.executable, '-m', 'fontTools.varLib.instancer', os.path.join(src, 'Fredoka-VF.ttf'), f'wght={w}', 'wdth=100', '-o', out, '-q'], check=True)
chars = set(chr(c) for c in list(range(0x20, 0x7f)) + list(range(0xa0, 0x100)) + list(range(0x2010, 0x2060)) + list(range(0x2190, 0x2200)) + list(range(0x2460, 0x2500)) + list(range(0x25a0, 0x2700)) + list(range(0x3000, 0x3100)) + list(range(0xff00, 0xfff0)))
for pat in ['index.html', 'src/**/*.ts', 'rust/game/src/**/*.rs', 'rust/sim/src/**/*.rs', 'docs/spec.md']:
    for f in glob.glob(os.path.join(root, pat), recursive=True):
        chars |= set(open(f, encoding='utf8').read())
for hi in range(0xb0, 0xd0):
    for lo in range(0xa1, 0xff):
        try: chars.add(bytes([hi, lo]).decode('euc_jp'))
        except Exception: pass
def get(u):
    return json.load(urllib.request.urlopen(urllib.request.Request(u, headers={'User-Agent': 'keiba-font-subset'})))
index = 'https://characters.aiarranger.jp/channels/stable/v1/index.json?channel=stable'
base = index.split('channels/')[0]
for c in get(index)['characters'].values():
    chars |= set(get(base + c['releases'][c['current']]['path'].lstrip('/'))['name'])
chars = {c for c in chars if ord(c) >= 0x20}
txt = os.path.join(src, 'chars.txt')
open(txt, 'w', encoding='utf8').write(''.join(sorted(chars)))
print(len(chars), 'chars')
for name in ['ZenMaruGothic-Medium.ttf', 'ZenMaruGothic-Bold.ttf', 'ZenMaruGothic-Black.ttf', 'DelaGothicOne-Regular.ttf', 'Fredoka-500.ttf', 'Fredoka-600.ttf', 'Fredoka-700.ttf']:
    f, out = os.path.join(src, name), os.path.join(here, name)
    subprocess.run([sys.executable, '-m', 'fontTools.subset', f, f'--text-file={txt}', f'--output-file={out}', '--layout-features=kern,palt,vert,tnum', '--no-hinting', '--desubroutinize'], check=True)
    print(name, os.path.getsize(f) // 1024, '->', os.path.getsize(out) // 1024, 'KB')
os.makedirs(os.path.join(here, 'licenses'), exist_ok=True)
for lic in glob.glob(os.path.join(src, 'OFL-*.txt')):
    open(os.path.join(here, 'licenses', os.path.basename(lic)), 'w').write(open(lic).read())
