// 下見の模型 1 体の口（プリセットは maker.rs の Model、オリジナル 4 体は original.rs）。Stage（lib.rs）はこの口だけを呼ぶ。
use island_kit::Kit;
use serde_json::Value;

pub trait Actor {
    /// assemble の返り値（立った姿の箱・顔の表示の箱など。TS の studio-camera が読む）
    fn info(&self, kit: &Kit) -> Value;
    /// 作り直さずに設定を当てる
    fn apply(&mut self, kit: &mut Kit, config: &Value);
    /// 合図（play の JSON: motion・playing・speed・hurtFace・hit・weaponVisible・ほか模型ごとの値）
    fn signal(&mut self, kit: &mut Kit, v: &Value);
    /// 1 コマ進める
    fn step(&mut self, kit: &mut Kit, dt: f64);
    /// 測り（{ q: 'armed' | 'bone' | ... }）
    fn query(&self, kit: &Kit, v: &Value) -> Value;
    /// TS へ知らせる出来事（motionComplete・hitComplete など）
    fn take_events(&mut self) -> Vec<Value>;
    /// 開発用の読み出し
    fn debug(&self, kit: &Kit) -> Value;
    /// GLB の書き出し（段 R6）
    fn export(&mut self, _kit: &mut Kit, _v: &Value) -> Result<Vec<u8>, String> { Err("書き出しはまだありません。".into()) }
    /// 場面から外す
    fn remove(self: Box<Self>, kit: &mut Kit);
}
