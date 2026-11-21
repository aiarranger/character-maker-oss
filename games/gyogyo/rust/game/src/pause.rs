// 一時停止: Esc・P・ゲームパッドの Start・画面の Ⅱ で止め、操作の説明を出す（全ゲーム共通）。
// 説明は controls.rs の割り当て表から、最後に触った機器の表記で作る（キーを変えても説明が古くならない）。
// 項目は上下左右（キー・十字・左スティック）で移り、Enter・Space・パッドの A で押す。選んでいる項目の輪は、キーかパッドで触っている間だけ。
// ボタンの中身は app が持つ（文字は app の pause_labels、押したときの処理は pause_run。番号は pause_run の番号）。描くのは ui.rs の Screens から。
use crate::controls::{Keys, help_rows};
use crate::ui::*;
use egui::{Rect, pos2, vec2};
use island_kit::app::input::Device;
use serde_json::{Value, json};
use game_ui::menu::step;
use sim::menu::{keys_entry, pause_ids};

pub struct Pause {
    pub now: f64,
    /// 出している項目（pause_run の番号, 文字）
    items: Vec<(usize, String)>,
    help: Vec<(String, String)>,
    /// 選んでいる項目（items の順。キー設定の札が出ている間は、その後ろに札が続く）
    pub focus: usize,
    /// 選んでいる項目の輪を出すか（ui.rs が毎コマ渡す）
    pub ring: bool,
    device: Device,
    /// 「キー設定」を出すか（指の間は出さない。sim::menu::keys_entry）
    keys_entry: bool,
    /// キー設定の札（.keys.show）と、その中身（文字, 押すのを待っている）
    pub keys_shown: bool,
    pub key_labels: Vec<(String, bool)>,
    /// 入りきらないとき（とても小さい窓）の送り
    pub scroll: Scroller,
    card_k: Tween,
}

/// 大きさの決め（表の字・行の上下・左右、ボタンの間・字・内側・「つづける」の字）
#[derive(Clone, Copy)]
struct Sizes { hfs: f32, tdv: f32, tdh: f32, c1: f32, c2max: f32, mgap: f32, mfs: f32, mpad: f32, pfs: f32, short: bool }
const COLS: usize = 2;

impl Pause {
    pub fn new() -> Pause {
        Pause { now: 0.0, items: vec![], help: vec![], focus: 0, ring: false, device: Device::Keys, keys_entry: true, keys_shown: false, key_labels: vec![], scroll: Scroller::new(false), card_k: Tween::new(0.0, 0.45, 0.0, bezier_spring) }
    }
    fn render_help(&mut self, keys: &Keys) { self.help = help_rows(self.device, keys); }
    /// sync（labels は pause_run の番号順の今の文字）
    pub fn sync(&mut self, labels: &[String]) {
        self.items = pause_ids(self.keys_entry || self.keys_shown).into_iter().filter_map(|i| labels.get(i).map(|l| (i, l.clone()))).collect();
        self.focus = step(self.focus, self.count(), false, false);
    }
    pub fn show(&mut self, on: bool, labels: &[String], keys: &Keys) {
        self.card_k.set(if on { 1.0 } else { 0.0 }, self.now);
        if on { self.focus = 0; self.scroll.reset(); self.keys_entry = keys_entry(false, self.device == Device::Touch); self.render_help(keys); self.sync(labels); }
    }
    pub fn set_device(&mut self, d: Device, labels: &[String], keys: &Keys) {
        self.device = d;
        self.keys_entry = keys_entry(self.keys_entry, d == Device::Touch);
        self.render_help(keys);
        self.sync(labels);
    }
    /// 選べる項目の数（ボタンと、出ていればキー設定の札）
    fn count(&self) -> usize { self.items.len() + if self.keys_shown { self.key_labels.len() } else { 0 } }
    /// キー・ゲームパッドで項目を移る。決定したら押した物を返す
    pub fn nav(&mut self, prev: bool, next: bool, confirm: bool) -> Option<Act> {
        self.focus = step(self.focus, self.count(), prev, next);
        if !confirm { return None; }
        match self.items.get(self.focus) { Some((id, _)) => Some(Act::PauseBtn(*id)), None => Some(Act::Key(self.focus - self.items.len())) }
    }
    /// 確かめ用: 今の版の #pause の中身（選んでいる項目に *。輪が出ている間だけ）
    pub fn dump(&self) -> Value {
        let n = self.items.len();
        json!({
            "help": self.help.iter().map(|(k, v)| [k.clone(), v.clone()]).collect::<Vec<_>>(),
            "menu": self.items.iter().enumerate().map(|(i, (_, l))| format!("{l}{}", if i == self.focus && self.ring { "*" } else { "" })).collect::<Vec<_>>(),
            "ids": self.items.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            "focus": self.focus,
            "keys": if self.keys_shown { json!(self.key_labels.iter().enumerate().map(|(j, (s, w))| format!("{s}{}{}", if *w { "*" } else { "" }, if n + j == self.focus && self.ring { "<" } else { "" })).collect::<Vec<_>>()) } else { Value::Null },
        })
    }

    /// r は画面、safe は端の安全域（札はその内側）
    #[allow(clippy::too_many_arguments)]
    pub fn draw(&mut self, pen: &Pen, hits: &mut Hits, tws: &mut Tw, r: Rect, safe: [f32; 4], on: bool, notrans: bool) {
        pen.rect(r, rgba(8, 20, 16, 0.5));
        if on { hits.block(r); }
        let a = Rect::from_min_max(pos2(r.left() + safe[3], r.top() + safe[0]), pos2(r.right() - safe[1], r.bottom() - safe[2]));
        let (w, h) = (a.width(), a.height());
        let (short, xs) = (r.height() <= 520.0, r.width() <= 480.0);
        let (pt, ph, pb) = if short { (10.0, 16.0, 10.0) } else if xs { (16.0, 14.0, 18.0) } else { (20.0, 26.0, 22.0) };
        // 表の文字（狭い・低い画面で小さく）
        let (hfs, tdv, tdh) = if short { (12.0, 3.0, 6.0) } else if xs { (13.0, 3.0, 6.0) } else { (14.0, 4.0, 10.0) };
        let kbds: Vec<Kbd> = self.help.iter().map(|(k, _)| Kbd::help(pen, k, hfs)).collect();
        let c1 = kbds.iter().map(|k| k.w).fold(0.0, f32::max) + tdh * 2.0;
        let c2max = self.help.iter().map(|(_, v)| tw_(pen, v, hfs)).fold(0.0, f32::max) + tdh * 2.0;
        // ボタン（低い画面でも指で押せる高さ 40px 以上）
        let (mgap, mfs, mpad, pfs) = if short { (6.0, 12.0, 11.0, 14.0) } else if xs { (8.0, 13.0, 11.0, 16.0) } else { (8.0, 14.0, 10.0, 16.0) };
        let z = Sizes { hfs, tdv, tdh, c1, c2max, mgap, mfs, mpad, pfs, short };
        let btn_max = self.items.iter().skip(1).map(|(_, l)| tw(pen, l, JP7, mfs, 0.0) + mpad * 2.0 + 4.0).fold(0.0, f32::max);
        let prim_max = self.items.first().map_or(0.0, |(_, l)| tw(pen, l, JP9, pfs, pfs * 0.06) + mpad * 2.0);
        let menu_max = (btn_max * COLS as f32 + mgap * (COLS as f32 - 1.0)).max(prim_max);
        let keys_max = if self.keys_shown { self.key_labels.iter().map(|(l, _)| tw(pen, l, JP7, 13.0, 0.0) + 20.0).fold(0.0, f32::max) * 2.0 + 6.0 } else { 0.0 };
        let left_max = (c1 + c2max).max(keys_max).max(tw(pen, "ひと休み", DISP, 28.0, 0.0));
        // 低い画面（横持ちのスマホ）は、表を左・ボタンを右に並べる（送らなくても全部見える）。並べる幅が無ければ縦に積む
        let colgap = 20.0;
        let two = short && w * 0.96 - ph * 2.0 - colgap >= 520.0;
        let (cw, left_w, right_w) = if two {
            let avail = w * 0.96 - ph * 2.0 - colgap;
            let right_w = menu_max.min(avail * 0.56);
            let left_w = left_max.min(avail - right_w);
            (left_w + colgap + right_w + ph * 2.0, left_w, right_w)
        } else {
            let cw = (left_max.max(menu_max) + ph * 2.0).clamp(430.0_f32.min(w * 0.92), w * 0.92);
            (cw, cw - ph * 2.0, cw - ph * 2.0)
        };
        // 高さ（測るだけ）
        let lh = self.head(pen, false, &kbds, 0.0, 0.0, left_w, z);
        let bh = self.buttons(pen, None, 0.0, 0.0, right_w, z);
        let kh = self.keys(pen, None, 0.0, 0.0, left_w);
        let content = if two { (lh + kh).max(bh) } else { lh + if short { 6.0 } else { 12.0 } + bh + kh };
        let full = content + pt + pb;
        let ch = full.min(h * if short { 0.96 } else { 0.94 });
        let card = Rect::from_center_size(a.center(), vec2(cw, ch));
        let k = if notrans { if on { 1.0 } else { 0.0 } } else { self.card_k.get(self.now) };
        let sc = lerp(0.98, 1.0, k);
        let q = pen.sub(Xf::about(card.center().x, card.center().y, Xf::tr(0.0, 14.0 * (1.0 - k)).then(Xf::scale(sc, sc))), 1.0);
        q.box_shadow(card, 22.0, 0.0, 30.0, 70.0, rgba(0, 0, 0, 0.5));
        q.rrect(card, 22.0, PAPER);
        let at = self.scroll.place(card, full - ch);
        let qc = q.clip(card);
        let old = hits_clip(hits, Some(card));
        let mark = hits.mark();
        let (x, top) = (card.left() + ph, card.top() + pt - at);
        if two {
            let ly = top + (content - (lh + kh)) * 0.5;
            self.head(&qc, true, &kbds, x, ly, left_w, z);
            self.keys(&qc, Some(&mut *hits), x, ly + lh, left_w);
            self.buttons(&qc, Some((&mut *hits, &mut *tws)), x + left_w + colgap, top + (content - bh) * 0.5, right_w, z);
        } else {
            self.head(&qc, true, &kbds, x, top, left_w, z);
            let by = top + lh + if short { 6.0 } else { 12.0 };
            self.buttons(&qc, Some((&mut *hits, &mut *tws)), x, by, right_w, z);
            self.keys(&qc, Some(&mut *hits), x, by + bh, left_w);
        }
        if !on { hits.truncate(mark); }
        hits_clip(hits, old);
        more_cue(&q, &self.scroll, PAPER, GREEN);
    }

    /// 見出しと操作の説明の表（表は中央。説明の列は狭ければ折り返す）。高さを返す
    #[allow(clippy::too_many_arguments)]
    fn head(&self, pen: &Pen, draw: bool, kbds: &[Kbd], x: f32, y0: f32, iw: f32, z: Sizes) -> f32 {
        let cx = x + iw * 0.5;
        let mut y = y0;
        if draw { let ew = eyebrow_w(pen, "PAUSE"); eyebrow(pen, "PAUSE", cx - ew * 0.5, y, hx(0xb0732e)); }
        y += nlh(NUM6, "P", 13.0) + 2.0;
        let (h2, mb) = if z.short { (18.0, 4.0) } else { (28.0, 10.0) };
        if draw { let tt = pen.t("ひと休み", DISP, h2, 0.0); pen.text(&tt, cx - tt.w * 0.5, base_n(DISP, "ひ", h2, y), GREEN, &[]); }
        y += h2 * 1.448 + mb;
        let table_w = (z.c1 + z.c2max).min(iw);
        let c2 = table_w - z.c1;
        let tx = cx - table_w * 0.5;
        for (i, (_, v)) in self.help.iter().enumerate() {
            // 列の幅は一番長い説明の幅そのもの（丸めで最後の字が折れないよう、少し余裕を持たせる）
            let rows = pen.lines(v, JP5, z.hfs, 0.0, c2 - z.tdh * 2.0 + 0.5);
            let lh = z.hfs * 1.448;
            let content = (rows.len() as f32 * lh).max(kbds[i].h).max(lh);
            let rh = content + z.tdv * 2.0 + 1.0;
            if draw {
                let cy = y + z.tdv + content * 0.5;
                kbds[i].draw(pen, tx + z.c1 - z.tdh - kbds[i].w, cy, GREEN);
                let top = cy - rows.len() as f32 * lh * 0.5;
                for (j, tt) in rows.iter().enumerate() { pen.text(tt, tx + z.c1 + z.tdh, base_n(JP5, v, z.hfs, top + j as f32 * lh), GREEN, &[]); }
                pen.rect(Rect::from_min_size(pos2(tx, y + rh - 1.0), vec2(table_w, 1.0)), rgba(15, 61, 46, 0.12));
            }
            y += rh;
        }
        y - y0
    }

    /// ボタン（1 つ目は横いっぱい、あとは 2 列）。高さを返す（hits が None なら測るだけ）
    #[allow(clippy::too_many_arguments)]
    fn buttons(&self, pen: &Pen, mut hits: Option<(&mut Hits, &mut Tw)>, x: f32, y0: f32, iw: f32, z: Sizes) -> f32 {
        let (mgap, mfs, mpad, pfs) = (z.mgap, z.mfs, z.mpad, z.pfs);
        let colw = (iw - mgap * (COLS as f32 - 1.0)) / COLS as f32;
        let n = self.items.len();
        let mut y = y0;
        let mut i = 0;
        while i < n {
            let row: Vec<usize> = if i == 0 { vec![0] } else { (i..(i + COLS).min(n)).collect() };
            let rows_txt: Vec<Vec<T>> = row.iter().map(|&j| if j == 0 { vec![pen.t(&self.items[0].1, JP9, pfs, pfs * 0.06)] } else { pen.lines(&self.items[j].1, JP7, mfs, 0.0, colw - mpad * 2.0 - 4.0) }).collect();
            let rh = row.iter().zip(&rows_txt).map(|(&j, rt)| if j == 0 { pfs * 1.448 + mpad * 2.0 } else { rt.len() as f32 * mfs * 1.448 + mpad * 2.0 + 4.0 }).fold(0.0, f32::max);
            for (k, &j) in row.iter().enumerate() {
                let br = if j == 0 { Rect::from_min_size(pos2(x, y), vec2(iw, rh)) } else { Rect::from_min_size(pos2(x + k as f32 * (colw + mgap), y), vec2(colw, rh)) };
                let label = &self.items[j].1;
                if let Some((hits, tw)) = hits.as_mut() {
                    let act = Act::PauseBtn(self.items[j].0);
                    hits.add(act, br, Shape::Round(br.height() * 0.5));
                    let rad = br.height() * 0.5;
                    if j == 0 {
                        let hk = tw.go(act, if hits.pressed == Some(act) && hits.hover == Some(act) { 2.0 } else if hits.hover == Some(act) { 1.0 } else { 0.0 }, 0.15, bezier_spring);
                        let dy = if hk <= 1.0 { -2.0 * hk } else { lerp(-2.0, 4.0, hk - 1.0) };
                        let p = pen.sub(Xf::tr(0.0, dy), 1.0);
                        p.rrect(br.translate(vec2(0.0, 4.0)), rad, SHADOW_GREEN);
                        p.rrect(br, rad, GOLD);
                        let tt = &rows_txt[k][0];
                        p.text(tt, br.center().x - tt.w * 0.5, base_n(JP9, label, pfs, br.center().y - pfs * 1.448 * 0.5), GREEN, &[]);
                    } else {
                        pen.rrect(br, rad, WHITE);
                        pen.rframe(br, rad, 2.0, rgba(15, 61, 46, 0.25));
                        let lh = mfs * 1.448;
                        let top = br.center().y - rows_txt[k].len() as f32 * lh * 0.5;
                        for (q, tt) in rows_txt[k].iter().enumerate() { pen.text(tt, br.center().x - tt.w * 0.5, base_n(JP7, label, mfs, top + q as f32 * lh), GREEN, &[]); }
                    }
                    if j == self.focus && self.ring { pen.rframe(br.expand(5.0), rad + 5.0, 3.0, RED); }
                }
            }
            y += rh + mgap;
            i = if i == 0 { 1 } else { i + COLS };
        }
        y - mgap - y0
    }

    /// キー設定の札（2 列。出ていなければ高さ 0）
    fn keys(&self, pen: &Pen, mut hits: Option<&mut Hits>, x: f32, y0: f32, iw: f32) -> f32 {
        if !self.keys_shown || self.key_labels.is_empty() { return 0.0; }
        let y = y0 + 10.0;
        let kw = (iw - 6.0) / 2.0;
        let kh = 13.0 * 1.448 + 16.0 + 4.0;
        for (j, (l, wait)) in self.key_labels.iter().enumerate() {
            let br = Rect::from_min_size(pos2(x + (j % 2) as f32 * (kw + 6.0), y + (j / 2) as f32 * (kh + 6.0)), vec2(kw, kh));
            if let Some(hits) = hits.as_mut() {
                hits.add(Act::Key(j), br, Shape::Round(12.0));
                pen.rrect(br, 12.0, if *wait { GOLD } else { WHITE });
                pen.rframe(br, 12.0, 2.0, rgba(15, 61, 46, 0.25));
                let tt = pen.t(l, JP7, 13.0, 0.0);
                pen.text(&tt, br.center().x - tt.w * 0.5, base_n(JP7, l, 13.0, br.center().y - 13.0 * 1.448 * 0.5), GREEN, &[]);
                if self.items.len() + j == self.focus && self.ring { pen.rframe(br.expand(4.0), 16.0, 3.0, RED); }
            }
        }
        10.0 + self.key_labels.len().div_ceil(2) as f32 * (kh + 6.0) - 6.0
    }
}

fn tw_(pen: &Pen, s: &str, fs: f32) -> f32 { tw(pen, s, JP5, fs, 0.0) }
