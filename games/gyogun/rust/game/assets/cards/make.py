import math, random
INK='#17110f'; PAPER='#f4e8cc'; RED='#c8321e'; GREEN='#2f6b3a'; YEL='#e2ab2f'; BROWN='#6e4a33'
def head(): return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 416" width="256" height="416">\n<rect width="256" height="416" rx="15" fill="{INK}"/>\n<rect x="9" y="9" width="238" height="398" rx="8" fill="{PAPER}"/>\n<clipPath id="in"><rect x="9" y="9" width="238" height="398" rx="8"/></clipPath>\n<g clip-path="url(#in)" stroke-linejoin="round" stroke-linecap="round">\n'
def tail(): return '</g>\n<rect x="9" y="9" width="238" height="398" rx="8" fill="none" stroke="#17110f" stroke-width="3"/>\n</svg>\n'
def bez(p0,p1,p2,p3,t):
    u=1-t
    return (u**3*p0[0]+3*u*u*t*p1[0]+3*u*t*t*p2[0]+t**3*p3[0], u**3*p0[1]+3*u*u*t*p1[1]+3*u*t*t*p2[1]+t**3*p3[1])
def E(cx,cy,rx,ry,fill,rot=0,sw=3,stroke=INK): return f'<ellipse cx="{cx:.1f}" cy="{cy:.1f}" rx="{rx:.1f}" ry="{ry:.1f}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}" transform="rotate({rot:.1f} {cx:.1f} {cy:.1f})"/>\n'
def P(d,fill='none',sw=3,stroke=INK): return f'<path d="{d}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}"/>\n'

# ---- 猪（萩に猪） ----
random.seed(3)
s=head()
s+=P('M 0 352 C 60 330, 190 338, 256 350 L 256 416 L 0 416 Z',YEL)
stems=[((228,352),(236,240),(214,110),(150,40)),((206,350),(196,230),(140,150),(40,128)),((240,352),(256,250),(252,150),(214,84)),((176,350),(150,250),(96,205),(28,214))]
for st in stems:
    s+=P(f'M {st[0][0]} {st[0][1]} C {st[1][0]} {st[1][1]}, {st[2][0]} {st[2][1]}, {st[3][0]} {st[3][1]}',sw=4,stroke=GREEN)
for st in stems:
    for k in range(3,13):
        t=k/12; x,y=bez(*st,t); x2,y2=bez(*st,min(1,t+0.02)); a=math.degrees(math.atan2(y2-y,x2-x))
        side=1 if k%2 else -1
        ox,oy=-math.sin(math.radians(a))*11*side, math.cos(math.radians(a))*11*side
        if k%3==0: s+=E(x+ox,y+oy,10,5.5,GREEN,a+35*side,2)
        else: s+=E(x+ox,y+oy,6.5,6.5,'#b9306a' if k%2 else RED,0,2)
# 体
s+=P('M 72 262 C 84 214, 168 204, 206 232 C 228 250, 226 296, 204 308 C 166 322, 108 322, 82 308 C 60 300, 58 282, 72 262 Z',BROWN,4)
# たてがみ
s+=P('M 88 232 l 9 -15 l 7 13 l 9 -16 l 7 14 l 10 -16 l 7 14 l 10 -15 l 7 14 l 10 -13 l 7 14 l 9 -10 l 6 14 Z','#2c1d15',3)
# 脚
for x in (92,112,170,190): s+=f'<rect x="{x}" y="298" width="13" height="44" rx="3" fill="#4d3526" stroke="{INK}" stroke-width="3"/>\n<rect x="{x}" y="334" width="13" height="10" rx="2" fill="{INK}"/>\n'
# しっぽ
s+=P('M 212 250 q 22 -8 16 -30',sw=6,stroke='#4d3526')
# 頭
s+=P('M 96 250 C 70 244, 40 262, 30 286 C 26 298, 36 310, 52 306 C 74 302, 92 296, 100 280 Z','#7d583f',4)
s+=P('M 88 250 L 96 226 L 108 250 Z','#4d3526',3)
s+=E(34,292,8,10,'#d49a7c',-20,3)
s+=E(66,274,3.4,3.4,INK,0,0)
s+=P('M 50 303 q 2 -14 12 -13',sw=5,stroke='#fff8e6')
s+=P('M 50 303 q 2 -14 12 -13',sw=1.5)
# 体の縞
for x in (128,150,172): s+=P(f'M {x} 232 q -6 30 2 62',sw=3,stroke='#4d3526')
open('ino.svg','w').write(s+tail())

# ---- 鹿（紅葉に鹿） ----
random.seed(7)
def maple(cx,cy,r,rot,fill):
    pts=[]
    lob=[1.0,0.42,0.86,0.4,0.72,0.34,0.44]  # 半分の輪郭
    n=7
    radii=[1.0,0.45,0.9,0.42,0.78,0.36,0.5,0.2,0.5,0.36,0.78,0.42,0.9,0.45]
    for i,rr in enumerate(radii):
        a=math.radians(rot-90+i*360/len(radii))
        pts.append(f'{cx+math.cos(a)*r*rr:.1f},{cy+math.sin(a)*r*rr:.1f}')
    a=math.radians(rot+90)
    return f'<polygon points="{" ".join(pts)}" fill="{fill}" stroke="{INK}" stroke-width="2.5"/>\n<line x1="{cx:.1f}" y1="{cy:.1f}" x2="{cx+math.cos(a)*r*0.75:.1f}" y2="{cy+math.sin(a)*r*0.75:.1f}" stroke="{INK}" stroke-width="2.5"/>\n'
s=head()
s+=P('M 0 356 C 70 340, 180 346, 256 356 L 256 416 L 0 416 Z','#3f7a45')
s+=P('M 256 40 C 190 52, 120 44, 40 86',sw=7,stroke='#3a281d')
s+=P('M 150 50 C 140 80, 150 110, 190 130',sw=5,stroke='#3a281d')
leaves=[(52,92,34,-20,RED),(104,66,30,15,'#e0861e'),(160,82,36,-8,RED),(214,62,32,25,'#e0861e'),(196,138,34,-30,RED),(232,118,26,10,YEL),(78,140,26,40,'#e0861e'),(128,120,24,-15,YEL),(30,150,22,5,RED)]
for l in leaves: s+=maple(*l)
# 体（右向き、首を左へ振り返る）
s+=P('M 70 268 C 84 236, 170 232, 200 252 C 214 264, 212 296, 196 304 C 160 314, 104 314, 82 304 C 64 298, 62 282, 70 268 Z','#c98f52',4)
for x,y in ((108,258),(128,250),(150,254),(170,262),(120,276),(144,274),(164,282)): s+=E(x,y,4.5,4.5,'#fff3d6',0,0)
for x in (84,102,172,190): s+=f'<rect x="{x}" y="298" width="10" height="50" rx="3" fill="#b07a44" stroke="{INK}" stroke-width="3"/>\n<rect x="{x}" y="340" width="10" height="9" rx="2" fill="{INK}"/>\n'
s+=E(206,258,9,7,'#fff3d6',-30,3)
# 首と頭（振り返る）
s+=P('M 84 262 C 80 230, 96 206, 116 196 L 134 212 C 118 222, 110 244, 112 268 Z','#c98f52',4)
s+=P('M 110 192 C 124 180, 150 184, 162 198 C 166 206, 158 214, 146 212 C 134 216, 118 214, 110 204 Z','#d49c5e',4)
s+=E(160,204,5,4,INK,0,0)
s+=E(134,196,3.2,3.2,INK,0,0)
s+=P('M 112 190 L 100 174 L 120 182 Z','#b07a44',3)
# 角
s+=P('M 118 186 C 110 164, 112 150, 124 138 M 114 162 L 100 152 M 118 150 L 132 150',sw=5,stroke='#5a3a22')
s+=P('M 128 186 C 128 166, 138 154, 152 148 M 132 166 L 146 168',sw=5,stroke='#5a3a22')
open('shika.svg','w').write(s+tail())

# ---- 蝶（牡丹に蝶） ----
def peony(cx,cy,r,fill1,fill2):
    o=''
    for i in range(8):
        a=math.radians(i*45+10); o+=E(cx+math.cos(a)*r*0.62,cy+math.sin(a)*r*0.62,r*0.5,r*0.5,fill1,0,3)
    for i in range(6):
        a=math.radians(i*60+30); o+=E(cx+math.cos(a)*r*0.32,cy+math.sin(a)*r*0.32,r*0.38,r*0.38,fill2,0,3)
    o+=E(cx,cy,r*0.24,r*0.24,YEL,0,3)
    for i in range(6):
        a=math.radians(i*60); o+=E(cx+math.cos(a)*r*0.12,cy+math.sin(a)*r*0.12,1.8,1.8,INK,0,0)
    return o
def leaf(cx,cy,l,w,rot): return f'<path d="M {-l} 0 Q 0 {-w} {l} 0 Q 0 {w} {-l} 0 Z M {-l} 0 L {l} 0" fill="{GREEN}" stroke="{INK}" stroke-width="3" transform="translate({cx} {cy}) rotate({rot})"/>\n'
def fly(cx,cy,sc,rot,c1,c2):
    o=f'<g transform="translate({cx} {cy}) rotate({rot}) scale({sc})">\n'
    for sx in (-1,1):
        o+=f'<g transform="scale({sx} 1)">\n'
        o+=P('M 2 -4 C 14 -40, 52 -44, 56 -18 C 58 -2, 30 4, 2 2 Z',c1,3)
        o+=P('M 2 2 C 22 4, 44 14, 40 32 C 34 46, 12 36, 2 8 Z',c2,3)
        o+=E(38,-22,7,7,'#fff3d6',0,2.5)+E(26,22,4.5,4.5,INK,0,0)
        o+=P('M 2 -6 C 8 -20, 12 -28, 20 -34',sw=2)
        o+='</g>\n'
    o+=E(0,0,4,16,INK,0,0)
    return o+'</g>\n'
s=head()
s+=P('M 60 416 C 70 360, 80 330, 96 300 M 196 416 C 190 370, 180 330, 164 290',sw=6,stroke=GREEN)
for l in ((54,356,34,13,-35),(206,350,34,13,35),(36,292,30,12,20),(222,286,30,12,-20),(128,384,36,14,90),(92,392,28,11,-60),(166,392,28,11,60)): s+=leaf(*l)
s+=peony(128,300,64,RED,'#e2553f')
s+=peony(196,224,30,'#d9486a','#ef7d95')
s+=leaf(160,236,20,8,-40)
s+=fly(88,112,1.25,-18,YEL,RED)
s+=fly(182,128,0.85,24,'#fff3d6',YEL)
open('cho.svg','w').write(s+tail())
