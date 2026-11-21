// 凍結: character-actor 0.1.14（2026-11-12）の src/zip.rs の写し。NPC・汎用（character-runtime 0.2.5 の写しの決まり）だけが使う。主役の crate の直しをここへ持ち込まない（Owner 2026-11-12「種類ごとに分ける」）
// .character.zip を開く（three に入っている fflate の unzipSync と同じ読み方。std だけ）。
// 中央ディレクトリの順に読み、filter が通した物だけを展開する。無圧縮（0）と deflate（8）だけ。CRC は確かめない（fflate と同じ。中身は SHA で確かめる）。
// 失敗の文言は英語（読む側の normalize_error が、利用者向けの「壊れています」の文言に替える。TS も fflate の英語の文言を同じく替える）。

/// 中央ディレクトリの 1 件（fflate の UnzipFileInfo）
#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    /// 圧縮した大きさ
    pub size: u64,
    /// 展開した大きさ（中央ディレクトリの値）
    pub original_size: u64,
    pub compression: u16,
}

fn b2(d: &[u8], at: usize) -> Result<u16, String> { d.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).ok_or_else(|| "invalid zip data".to_string()) }
fn b4(d: &[u8], at: usize) -> Result<u32, String> { d.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok_or_else(|| "invalid zip data".to_string()) }
fn b8(d: &[u8], at: usize) -> Result<u64, String> { Ok(b4(d, at)? as u64 + b4(d, at + 4)? as u64 * 4294967296) }

/// unzipSync(data, { filter })。filter は展開の前に 1 件ずつ呼ばれ、Err を返すとそこで止まる。戻すのは (名前, 中身) を中央ディレクトリの順に（同じ名前は後の物）
pub fn unzip(data: &[u8], mut filter: impl FnMut(&Entry) -> Result<bool, String>) -> Result<Vec<(String, Vec<u8>)>, String> {
    if data.len() < 22 { return Err("invalid zip data".into()); }
    let mut e = data.len() - 22;
    while b4(data, e)? != 0x0605_4b50 {
        if e == 0 || data.len() - e > 65558 { return Err("invalid zip data".into()); }
        e -= 1;
    }
    let mut c = b2(data, e + 8)? as u64;
    if c == 0 { return Ok(Vec::new()); }
    let mut o = b4(data, e + 16)? as u64;
    let mut z = o == 4294967295 || c == 65535;
    if z {
        let ze = b4(data, e.checked_sub(12).ok_or("invalid zip data")?)? as usize;
        z = b4(data, ze)? == 0x0606_4b50;
        if z { c = b4(data, ze + 32)? as u64; o = b4(data, ze + 48)? as u64; }
    }
    let mut out: Vec<(String, Vec<u8>)> = Vec::new();
    let mut at = o as usize;
    for _ in 0..c {
        let f = at;
        let fnl = b2(data, f + 28)? as usize;
        let raw = data.get(f + 46..f + 46 + fnl).ok_or("invalid zip data")?;
        // 名前の文字（汎用フラグの 11 番が立っていれば UTF-8、無ければ latin1）
        let name: String = if b2(data, f + 8)? & 2048 != 0 { String::from_utf8_lossy(raw).into_owned() } else { raw.iter().map(|&b| b as char).collect() };
        let es = f + 46 + fnl;
        let bs = b4(data, f + 20)?;
        let (sc, su, off) = if z && bs == 4294967295 { z64e(data, es)? } else { (bs as u64, b4(data, f + 24)? as u64, b4(data, f + 42)? as u64) };
        let compression = b2(data, f + 10)?;
        at = es + b2(data, f + 30)? as usize + b2(data, f + 32)? as usize;
        let entry = Entry { name: name.clone(), size: sc, original_size: su, compression };
        if !filter(&entry)? { continue; }
        let off = off as usize;
        let b = off + 30 + b2(data, off + 26)? as usize + b2(data, off + 28)? as usize;
        let src = data.get(b..b + sc as usize).ok_or("invalid zip data")?;
        let bytes = match compression {
            0 => src.to_vec(),
            8 => inflate(src, su as usize)?,
            c => return Err(format!("unknown compression type {c}")),
        };
        out.retain(|(n, _)| *n != name);
        out.push((name, bytes));
    }
    Ok(out)
}

/// zip64 の追加の欄（fflate の z64e）
fn z64e(d: &[u8], mut b: usize) -> Result<(u64, u64, u64), String> {
    while b2(d, b)? != 1 { b += 4 + b2(d, b + 2)? as usize; }
    Ok((b8(d, b + 12)?, b8(d, b + 4)?, b8(d, b + 20)?))
}

// ---- deflate（RFC 1951）を解く ----

const LEN_BASE: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEN_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASE: [u16; 30] = [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const DIST_EXTRA: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];
const CL_ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

struct Bits<'a> { src: &'a [u8], pos: usize, buf: u64, cnt: u32, over: u32 }
impl Bits<'_> {
    #[inline]
    fn fill(&mut self) {
        while self.cnt <= 56 {
            let b = if self.pos < self.src.len() { let b = self.src[self.pos]; self.pos += 1; b } else { self.over += 8; 0 };
            self.buf |= (b as u64) << self.cnt;
            self.cnt += 8;
        }
    }
    #[inline]
    fn peek(&mut self, n: u32) -> u32 { if self.cnt < n { self.fill(); } (self.buf & ((1u64 << n) - 1)) as u32 }
    #[inline]
    fn drop_bits(&mut self, n: u32) { self.buf >>= n; self.cnt -= n; }
    #[inline]
    fn get(&mut self, n: u32) -> u32 { if n == 0 { return 0; } let v = self.peek(n); self.drop_bits(n); v }
    /// 読みすぎ（入力の終わりの先）か
    fn overrun(&self) -> bool { self.over > self.cnt }
}

const FAST: u32 = 10;

/// 標準形の符号（数えた長さと並べた記号）と、短い符号を 1 回で引く表
struct Huff { counts: [u16; 16], symbols: Vec<u16>, fast: Vec<u16> }
impl Huff {
    fn new(lengths: &[u8]) -> Result<Huff, String> {
        let mut counts = [0u16; 16];
        for &l in lengths { counts[l as usize] += 1; }
        counts[0] = 0;
        let mut left: i32 = 1;
        for &c in &counts[1..] { left = left * 2 - c as i32; if left < 0 { return Err("invalid length/literal".into()); } }
        let mut offs = [0u16; 16];
        for l in 1..15 { offs[l + 1] = offs[l] + counts[l]; }
        let mut symbols = vec![0u16; lengths.len()];
        for (s, &l) in lengths.iter().enumerate() { if l != 0 { symbols[offs[l as usize] as usize] = s as u16; offs[l as usize] += 1; } }
        // 短い符号の表（読む向きは下の桁から。符号は上の桁から並ぶので、ひっくり返して置く）
        let mut fast = vec![0u16; 1 << FAST];
        let (mut code, mut index) = (0u32, 0usize);
        for len in 1..=15u32 {
            for _ in 0..counts[len as usize] {
                if len <= FAST {
                    let rev = code.reverse_bits() >> (32 - len);
                    let mut k = rev as usize;
                    while k < fast.len() { fast[k] = symbols[index] | ((len as u16) << 9); k += 1 << len; }
                }
                code += 1;
                index += 1;
            }
            code <<= 1;
        }
        Ok(Huff { counts, symbols, fast })
    }

    #[inline]
    fn decode(&self, b: &mut Bits) -> Result<u16, String> {
        let e = self.fast[b.peek(FAST) as usize];
        if e != 0 { b.drop_bits((e >> 9) as u32); return Ok(e & 511); }
        // 長い符号は 1 桁ずつ（puff の decode）
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= b.get(1) as i32;
            let count = self.counts[len] as i32;
            if code - count < first { return Ok(self.symbols[(index + (code - first)) as usize]); }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        Err("invalid length/literal".into())
    }
}

/// deflate の流れを解く。limit は展開の大きさ（中央ディレクトリの値。超えたら止める）
pub fn inflate(src: &[u8], limit: usize) -> Result<Vec<u8>, String> {
    let mut out: Vec<u8> = Vec::with_capacity(limit);
    let mut b = Bits { src, pos: 0, buf: 0, cnt: 0, over: 0 };
    loop {
        let last = b.get(1);
        match b.get(2) {
            0 => {
                // 無圧縮の区切り: 次のバイトの境目から LEN・NLEN
                let skip = b.cnt % 8;
                b.drop_bits(skip);
                let len = b.get(16) as usize;
                let nlen = b.get(16) as usize;
                if len != (!nlen & 0xffff) { return Err("invalid block".into()); }
                if out.len() + len > limit { return Err("output too large".into()); }
                for _ in 0..len { let v = b.get(8) as u8; out.push(v); }
            }
            1 => {
                let mut l = [0u8; 288];
                for (i, x) in l.iter_mut().enumerate() { *x = if i < 144 { 8 } else if i < 256 { 9 } else if i < 280 { 7 } else { 8 }; }
                let lit = Huff::new(&l)?;
                let dist = Huff::new(&[5u8; 30])?;
                codes(&mut b, &mut out, &lit, &dist, limit)?;
            }
            2 => {
                let nlen = b.get(5) as usize + 257;
                let ndist = b.get(5) as usize + 1;
                let ncode = b.get(4) as usize + 4;
                if nlen > 286 || ndist > 30 { return Err("invalid block".into()); }
                let mut cl = [0u8; 19];
                for &i in CL_ORDER.iter().take(ncode) { cl[i] = b.get(3) as u8; }
                let clh = Huff::new(&cl)?;
                let mut lengths = vec![0u8; nlen + ndist];
                let mut i = 0;
                while i < nlen + ndist {
                    let sym = clh.decode(&mut b)?;
                    let (v, rep) = match sym {
                        0..=15 => { lengths[i] = sym as u8; i += 1; continue; }
                        16 => { if i == 0 { return Err("invalid block".into()); } (lengths[i - 1], 3 + b.get(2) as usize) }
                        17 => (0, 3 + b.get(3) as usize),
                        _ => (0, 11 + b.get(7) as usize),
                    };
                    if i + rep > nlen + ndist { return Err("invalid block".into()); }
                    for x in &mut lengths[i..i + rep] { *x = v; }
                    i += rep;
                }
                if lengths[256] == 0 { return Err("invalid block".into()); }
                let lit = Huff::new(&lengths[..nlen])?;
                let dist = Huff::new(&lengths[nlen..])?;
                codes(&mut b, &mut out, &lit, &dist, limit)?;
            }
            _ => return Err("invalid block type".into()),
        }
        if b.overrun() { return Err("unexpected EOF".into()); }
        if last == 1 { break; }
    }
    Ok(out)
}

fn codes(b: &mut Bits, out: &mut Vec<u8>, lit: &Huff, dist: &Huff, limit: usize) -> Result<(), String> {
    loop {
        let sym = lit.decode(b)? as usize;
        if sym < 256 {
            if out.len() >= limit { return Err("output too large".into()); }
            out.push(sym as u8);
            continue;
        }
        if sym == 256 { return Ok(()); }
        let s = sym - 257;
        if s >= 29 { return Err("invalid length/literal".into()); }
        let len = LEN_BASE[s] as usize + b.get(LEN_EXTRA[s] as u32) as usize;
        let ds = dist.decode(b)? as usize;
        if ds >= 30 { return Err("invalid distance".into()); }
        let d = DIST_BASE[ds] as usize + b.get(DIST_EXTRA[ds] as u32) as usize;
        if d > out.len() { return Err("invalid distance".into()); }
        if out.len() + len > limit { return Err("output too large".into()); }
        let from = out.len() - d;
        if d >= len { out.extend_from_within(from..from + len); } else { for k in 0..len { let v = out[from + k]; out.push(v); } }
        if b.overrun() { return Err("unexpected EOF".into()); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// 防ぐ故障: 無圧縮・固定の符号・選んだ符号の区切りのどれかを読み違える（python の zlib で作った流れ）
    #[test]
    fn inflates_known_streams() {
        // zlib.compress(b"hello hello hello hello", 9)[2:-4]（固定の符号・後ろを指す）
        let fixed = [0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x57, 0xc8, 0x40, 0x27, 0x01];
        assert_eq!(inflate(&fixed, 23).unwrap(), b"hello hello hello hello");
        // 無圧縮の区切り（BFINAL=1, BTYPE=00, LEN=3）
        let stored = [0x01, 0x03, 0x00, 0xfc, 0xff, b'a', b'b', b'c'];
        assert_eq!(inflate(&stored, 3).unwrap(), b"abc");
        // 大きさの上限を超えたら止める
        assert!(inflate(&fixed, 10).is_err());
        // 途中で切れた流れ
        assert!(inflate(&fixed[..5], 23).is_err());
    }
}
