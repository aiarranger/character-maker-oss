# 花札（猪・鹿・蝶）

猪鹿蝶（糸斬りの三連）で出す札の絵。2026-11-13 に手で描いた仮の絵（花札の「萩に猪」「紅葉に鹿」「牡丹に蝶」を意識した、このゲームのための絵）。

| ファイル | 中身 |
|---|---|
| `make.py` | SVG を書き出す（`python3 make.py`。この場所で流す） |
| `ino.svg` `shika.svg` `cho.svg` | 元の絵 |
| `ino.png` `shika.png` `cho.png` | ゲームが読む絵（320×520、48 色） |

絵を直したら、PNG を作り直す:

```bash
for n in ino shika cho; do rsvg-convert -w 320 -h 520 $n.svg | magick - -colors 48 -strip png8:$n.png; done
```

Gemini の画像生成でも試したが、今の鍵（無料枠）では画像のモデルが使えなかった（上限 0）。
