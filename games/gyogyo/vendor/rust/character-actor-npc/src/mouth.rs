// 凍結: character-actor 0.1.14（2026-11-12）の src/mouth.rs の写し。NPC・汎用（character-runtime 0.2.5 の写しの決まり）だけが使う。主役の crate の直しをここへ持ち込まない（Owner 2026-11-12「種類ごとに分ける」）
// 話す口（character-runtime の mouth.js の写し。0.2.5・0.2.7 で同じ）。音を聞いて動かすのではなく、決まった順の口の形を、話している長さのあいだ流す。
// 時刻は倍精度（TS の number と同じ）。モーフへ書くときに f32 にする。

/// 話している間（秒。nowSeconds と同じ時計）。character-runtime の CharacterSpeech
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterSpeech { pub started_at: f64, pub ends_at: f64 }

impl CharacterSpeech {
    /// createCharacterSpeech。長さは文の長さから（characterSpeechDuration）
    pub fn new(text: &str, now: f64) -> CharacterSpeech { CharacterSpeech { started_at: now, ends_at: now + speech_duration(text) } }
}

/// characterSpeechDuration（秒）。長さは JS の text.length（UTF-16 の数）
pub fn speech_duration(text: &str) -> f64 { (1.6 + text.encode_utf16().count() as f64 * 0.11).max(2.6).min(7.0) }

const SEQUENCE: [(Shape, f64); 11] = [
    (Shape::Close, 0.16), (Shape::Aa, 0.25), (Shape::Ee, 0.21), (Shape::Ou, 0.23),
    (Shape::Aa, 0.22), (Shape::Close, 0.27), (Shape::Ee, 0.22), (Shape::Aa, 0.30),
    (Shape::Ou, 0.27), (Shape::Ee, 0.18), (Shape::Close, 0.48),
];
#[derive(Clone, Copy, PartialEq)]
enum Shape { Close, Aa, Ee, Ou }

fn sequence_duration() -> f64 { SEQUENCE.iter().fold(0.0, |s, x| s + x.1) }
fn smooth(t: f64) -> f64 { t * t * (3.0 - 2.0 * t) }

/// 3 つの口（mouthClose・aa・ee・ou）。台帳の表情の無い主役・NPC・汎用
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct ThreeMouth { pub mouth_close: f64, pub aa: f64, pub ee: f64, pub ou: f64 }
impl ThreeMouth {
    fn add(&mut self, s: Shape, v: f64) { match s { Shape::Close => self.mouth_close += v, Shape::Aa => self.aa += v, Shape::Ee => self.ee += v, Shape::Ou => self.ou += v } }
    /// モーフの名前の値（mouthClose・aa・ee・ou のほかは None）
    pub fn get(&self, name: &str) -> Option<f64> { match name { "mouthClose" => Some(self.mouth_close), "aa" => Some(self.aa), "ee" => Some(self.ee), "ou" => Some(self.ou), _ => None } }
}

/// speakingMouth
pub fn speaking_mouth(time: f64) -> ThreeMouth {
    let d = sequence_duration();
    let mut phase = ((time % d) + d) % d;
    for i in 0..SEQUENCE.len() {
        let (shape, hold) = SEQUENCE[i];
        if phase >= hold { phase -= hold; continue; }
        let previous = SEQUENCE[(i + SEQUENCE.len() - 1) % SEQUENCE.len()].0;
        let mix = smooth((phase / 0.095).min(1.0));
        let mut r = ThreeMouth::default();
        r.add(previous, 1.0 - mix);
        r.add(shape, mix);
        return r;
    }
    ThreeMouth { mouth_close: 1.0, ..Default::default() }
}

/// sampleCharacterSpeech。話していない間は None
pub fn sample_character_speech(speech: Option<&CharacterSpeech>, now: f64) -> Option<ThreeMouth> {
    let s = speech?;
    if now < s.started_at || now >= s.ends_at { return None; }
    let w = speaking_mouth(now - s.started_at);
    let fade = smooth(((s.ends_at - now) / 0.095).min(1.0));
    Some(ThreeMouth { mouth_close: 1.0 - (1.0 - w.mouth_close) * fade, aa: w.aa * fade, ee: w.ee * fade, ou: w.ou * fade })
}

/// 5 つの母音と閉じた口（台帳の表情 VRM1 の口）
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct FiveMouth { pub closed: f64, pub aa: f64, pub ih: f64, pub ou: f64, pub ee: f64, pub oh: f64 }
#[derive(Clone, Copy)]
enum Five { Closed, Aa, Ih, Ou, Ee, Oh }
const FIVE_SHAPES: [Five; 11] = [Five::Closed, Five::Aa, Five::Ih, Five::Ou, Five::Ee, Five::Closed, Five::Oh, Five::Aa, Five::Ou, Five::Ih, Five::Closed];
impl FiveMouth {
    fn add(&mut self, s: Five, v: f64) { match s { Five::Closed => self.closed += v, Five::Aa => self.aa += v, Five::Ih => self.ih += v, Five::Ou => self.ou += v, Five::Ee => self.ee += v, Five::Oh => self.oh += v } }
    /// 母音の値（aa・ih・ou・ee・oh の順。characterFaceInput が書く順）
    pub fn vowels(&self) -> [(&'static str, f64); 5] { [("aa", self.aa), ("ih", self.ih), ("ou", self.ou), ("ee", self.ee), ("oh", self.oh)] }
}

/// speakingFiveMouth
pub fn speaking_five_mouth(time: f64) -> FiveMouth {
    let d = sequence_duration();
    let mut phase = ((time % d) + d) % d;
    for i in 0..SEQUENCE.len() {
        let hold = SEQUENCE[i].1;
        if phase >= hold { phase -= hold; continue; }
        let mix = smooth((phase / 0.095).min(1.0));
        let mut w = FiveMouth::default();
        w.add(FIVE_SHAPES[(i + FIVE_SHAPES.len() - 1) % FIVE_SHAPES.len()], 1.0 - mix);
        w.add(FIVE_SHAPES[i], mix);
        return w;
    }
    FiveMouth { closed: 1.0, ..Default::default() }
}

/// sampleFiveMouthSpeech
pub fn sample_five_mouth_speech(speech: Option<&CharacterSpeech>, now: f64) -> Option<FiveMouth> {
    let s = speech?;
    if now < s.started_at || now >= s.ends_at { return None; }
    let w = speaking_five_mouth(now - s.started_at);
    let fade = smooth(((s.ends_at - now) / 0.095).min(1.0));
    Some(FiveMouth { closed: 1.0 - (1.0 - w.closed) * fade, aa: w.aa * fade, ih: w.ih * fade, ou: w.ou * fade, ee: w.ee * fade, oh: w.oh * fade })
}

#[cfg(test)]
mod tests {
    use super::*;
    /// 防ぐ故障: 文の長さの数え方（JS の UTF-16 の数）と上下の丸めが TS と変わる
    #[test]
    fn speech_duration_follows_js_length() {
        assert_eq!(speech_duration(""), 2.6);
        assert!((speech_duration("こんにちは、今日はいい天気ですね") - (1.6 + 16.0 * 0.11)).abs() < 1e-12);
        assert_eq!(speech_duration(&"あ".repeat(100)), 7.0);
        // サロゲートの組は 2 つに数える
        assert!((speech_duration("😀😀😀😀😀😀😀😀😀😀😀") - (1.6 + 22.0 * 0.11)).abs() < 1e-12);
    }
    /// 防ぐ故障: 話し終わりと始まりの前に口が残る
    #[test]
    fn silent_outside_speech() {
        let s = CharacterSpeech { started_at: 1.0, ends_at: 2.0 };
        assert!(sample_character_speech(Some(&s), 0.99).is_none());
        assert!(sample_character_speech(Some(&s), 2.0).is_none());
        assert!(sample_five_mouth_speech(Some(&s), 1.5).is_some());
        // 始まりは閉じた口
        let m = sample_character_speech(Some(&s), 1.0).unwrap();
        assert_eq!((m.mouth_close, m.aa), (1.0, 0.0));
    }
}
