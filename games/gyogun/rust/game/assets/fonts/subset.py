#!/usr/bin/env python3
"""ゲームで使う字だけのフォントを作る（元のファイルは src/ に落として使う。リポジトリには入れない）。
字: かな・英数・記号の全部 + ソースと HTML に出てくる字 + 主役の名前（手元の配布 public/delivery の一覧から引く）+ よく使う漢字（JIS 第一水準）。"""
import glob, os, subprocess, sys, json
here = os.path.dirname(os.path.abspath(__file__))
root = os.path.abspath(os.path.join(here, '..', '..', '..', '..'))
chars = set(chr(c) for c in list(range(0x20, 0x7f)) + list(range(0xa0, 0x100)) + list(range(0x2010, 0x2060)) + list(range(0x2190, 0x2200)) + list(range(0x25a0, 0x2700)) + list(range(0x3000, 0x3100)) + list(range(0xff00, 0xfff0)))
for pat in ['rust/web/index.html', 'rust/game/src/**/*.rs', 'rust/sim/src/**/*.rs']:
    for f in glob.glob(os.path.join(root, pat), recursive=True):
        chars |= set(open(f, encoding='utf8').read())
# JIS 第一水準の漢字
for hi in range(0xb0, 0xd0):
    for lo in range(0xa1, 0xff):
        try: chars.add(bytes([hi, lo]).decode('euc_jp'))
        except Exception: pass
# 主役の名前（手元の配布の一覧から）
def get(rel):
    return json.load(open(os.path.join(root, 'public', 'delivery', rel), encoding='utf8'))
for c in get('channels/stable/v1/index.json')['characters'].values():
    chars |= set(get(c['releases'][c['current']]['path'])['name'])
chars = {c for c in chars if ord(c) >= 0x20}
txt = os.path.join(here, 'src', 'chars.txt')
open(txt, 'w', encoding='utf8').write(''.join(sorted(chars)))
print(len(chars), 'chars')
for f in sorted(glob.glob(os.path.join(here, 'src', '*.ttf'))):
    out = os.path.join(here, os.path.basename(f))
    subprocess.run([sys.executable, '-m', 'fontTools.subset', f, f'--text-file={txt}', f'--output-file={out}', '--layout-features=kern,palt,vert', '--no-hinting', '--desubroutinize'], check=True)
    print(os.path.basename(f), os.path.getsize(f) // 1024, '->', os.path.getsize(out) // 1024, 'KB')
