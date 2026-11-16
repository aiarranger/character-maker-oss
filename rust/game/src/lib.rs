// キャラメイクの下見の Stage（Rust・WebGPU）。React の部品（components/character-maker/rust/）が canvas を渡し、素材と設定を渡して描かせる。
// 計画は docs/rust-stage-plan.md。ブラウザだけで動く（wasm32）。
#[cfg(target_arch = "wasm32")]
mod actor;
#[cfg(target_arch = "wasm32")]
mod export;
#[cfg(target_arch = "wasm32")]
mod original;
#[cfg(target_arch = "wasm32")]
mod original_emotion;
#[cfg(target_arch = "wasm32")]
mod expressions;
#[cfg(target_arch = "wasm32")]
mod head;
#[cfg(target_arch = "wasm32")]
mod hurt_face;
#[cfg(target_arch = "wasm32")]
mod weapon;
#[cfg(target_arch = "wasm32")]
mod floor;
#[cfg(target_arch = "wasm32")]
mod maker;
#[cfg(target_arch = "wasm32")]
mod look;

#[cfg(target_arch = "wasm32")]
mod api {
    use crate::floor::Floor;
    use crate::actor::Actor;
    use crate::maker::{Assets, Model};
    use glam::Vec3;
    use island_kit::{Color, FINAL_DIRECT, Gpu, Kit, Output, RenderOpts};
    use serde_json::Value;
    use std::cell::RefCell;
    use wasm_bindgen::prelude::*;

    fn err(e: impl std::fmt::Display) -> JsValue { JsValue::from_str(&e.to_string()) }
    fn parse(json: &str) -> Result<Value, JsValue> { serde_json::from_str(json).map_err(err) }

    /// 下見の canvas 1 つ（今の版の R3F の Canvas 1 つに当たる）
    #[wasm_bindgen]
    pub struct Stage { kit: Kit, assets: Assets, model: Option<Box<dyn Actor>>, floor: Option<Floor>, look: crate::look::Look,
        /// 陰影を付けて描くか（init の look が 'studio'。旧画面 classic と書き出し用の Stage は付けない）
        shaded: bool }

    #[wasm_bindgen]
    impl Stage {
        /// 素材が届いているか（同じ素材を 2 回 fetch しない）
        pub fn has(&self, name: &str) -> bool { self.assets.has(name) }
        /// 素材（GLB・動作の JSON・PNG）を名前で渡す
        pub fn asset(&mut self, name: &str, bytes: &[u8]) -> Result<(), JsValue> { self.assets.put(name, bytes).map_err(err) }
        /// 模型を作り直す（今の版の assembleCharacter）。返り値は立った姿の箱（JSON）
        pub fn assemble(&mut self, json: &str) -> Result<String, JsValue> {
            let spec = parse(json)?;
            if let Some(old) = self.model.take() { old.remove(&mut self.kit); }
            self.look.reset();
            let model: Box<dyn Actor> = if spec["kind"].as_str() == Some("original") {
                crate::original::assemble(&mut self.kit, &mut self.assets, &spec).map_err(err)?
            } else {
                Box::new(Model::assemble(&mut self.kit, &mut self.assets, &spec).map_err(err)?)
            };
            let info = model.info(&self.kit).to_string();
            self.model = Some(model);
            Ok(info)
        }
        /// 作り直さずに設定を当てる（今の版の instance.apply）
        pub fn apply(&mut self, json: &str) -> Result<(), JsValue> {
            let c = parse(json)?;
            if let Some(m) = self.model.as_mut() { m.apply(&mut self.kit, &c); }
            self.look.measure = true;
            Ok(())
        }
        /// 動作の合図: { motion?, playing?, speed? }
        pub fn play(&mut self, json: &str) -> Result<(), JsValue> {
            let v = parse(json)?;
            if let Some(m) = self.model.as_mut() { m.signal(&mut self.kit, &v); }
            Ok(())
        }
        /// 動きを 1 コマ進める（描く前。カメラは TS がこのあとの頭の位置で決める）
        pub fn step(&mut self, dt: f64) { if let Some(m) = self.model.as_mut() { m.step(&mut self.kit, dt); } }
        /// 描く。cam: 位置 xyz・回転 xyzw・fov・near・far・aspect（three の PerspectiveCamera の値）
        pub fn draw(&mut self, cam: &[f64]) {
            if cam.len() >= 11 {
                let c = &mut self.kit.camera;
                c.position = Vec3::new(cam[0] as f32, cam[1] as f32, cam[2] as f32);
                c.quaternion = glam::Quat::from_xyzw(cam[3] as f32, cam[4] as f32, cam[5] as f32, cam[6] as f32);
                c.fov = cam[7] as f32; c.near = cam[8] as f32; c.far = cam[9] as f32; c.aspect = cam[10] as f32;
            }
            if let (Some(m), true) = (self.model.as_ref(), self.shaded) {
                let subject = m.look(&self.kit);
                if self.look.measure { self.look.set_head(subject.head, m.head_box(&self.kit)); }
                self.look.refresh(&mut self.kit, &subject);
            }
            self.kit.render(&RenderOpts { outline: false, hidden: false, clear: Color { r: 0.0, g: 0.0, b: 0.0 } });
            drop(self.kit.take_shader_errors());
        }
        /// 測り: { q: 'armed', names } → 構えの包み、{ q: 'bone', name: 'head'|'hips' } → 骨の位置
        pub fn query(&self, json: &str) -> Result<String, JsValue> {
            let v = parse(json)?;
            Ok(self.model.as_ref().map_or(Value::Null, |m| m.query(&self.kit, &v)).to_string())
        }
        /// GLB を書き出す（段 R6）。v は模型ごとの値（variant など）
        pub fn export(&mut self, json: &str) -> Result<Vec<u8>, JsValue> {
            let v = parse(json)?;
            match self.model.as_mut() { Some(m) => m.export(&mut self.kit, &v).map_err(err), None => Err(err("模型がありません。")) }
        }
        /// classic の床を置く（1 回）
        pub fn classic_floor(&mut self) { crate::floor::classic(&mut self.kit); }
        /// GLB の高さ（yukikau-package.ts の measureGlbHeight と同じ測り方）
        pub fn glb_height(&self, bytes: &[u8]) -> Result<f64, JsValue> { crate::export::glb_height(bytes).map(|h| h as f64).map_err(err) }
        /// 床（StageFloor）。vals: 影の x・z・直径・濃さ、輪の半径・大きさ・見せるか。color: 輪の色 '#RRGGBB'（空なら替えない）。影の絵は image('floor-shadow') で先に渡す
        pub fn floor(&mut self, vals: &[f64], color: &str) {
            if self.floor.is_none() {
                let Some(im) = self.assets.image_opt("floor-shadow") else { return };
                let t = self.kit.data_texture(im.width, im.height, &im.rgba, false);
                self.floor = Some(Floor::new(&mut self.kit, t));
            }
            let c = color.strip_prefix('#').and_then(|h| u32::from_str_radix(h, 16).ok()).map(Color::hex);
            if let Some(f) = self.floor.as_mut() { f.set(&mut self.kit, vals, c); }
        }
        /// 出来事（JSON の配列: motionComplete・hitComplete）を取り出す
        pub fn events(&mut self) -> String { self.model.as_mut().map_or("[]".into(), |m| Value::Array(m.take_events()).to_string()) }
        /// canvas で作った絵（RGBA。上の行が先）を名前で渡す
        pub fn image(&mut self, name: &str, w: u32, h: u32, rgba: &[u8]) { self.assets.put_image(name, w, h, rgba.to_vec()); }
        /// 開発用の読み出し
        pub fn debug(&self) -> String { self.model.as_ref().map_or("null".into(), |m| m.debug(&self.kit).to_string()) }
        /// dpr を替える
        pub fn set_dpr(&mut self, dpr: f32) { self.kit.set_max_pixel(dpr); self.kit.resize(); }
    }

    impl Drop for Stage { fn drop(&mut self) { self.kit.release_gpu(); } }

    thread_local! {
        /// ページに 1 つの Device（下見を開き直すたびに作らない。aisns の rust/game/src/lib.rs と同じ）
        static GPU: RefCell<Option<Gpu>> = const { RefCell::new(None) };
    }
    async fn gpu() -> Result<Gpu, String> {
        if let Some(g) = GPU.with(|c| c.borrow().clone()) { return Ok(g); }
        let made = Gpu::new().await?;
        if let Some(g) = GPU.with(|c| c.borrow().clone()) { made.device.destroy(); return Ok(g); }
        made.device.set_device_lost_callback(|reason, msg| {
            web_sys::console::error_1(&format!("GPU の Device を失いました（{reason:?}）: {msg}").into());
            GPU.with(|c| *c.borrow_mut() = None);
            // 今の版（three）へ戻す（components/character-maker/rust/stage.ts が聞く）
            if let Some(w) = web_sys::window() && let Ok(ev) = web_sys::Event::new("character-maker-gpu-lost") { let _ = w.dispatch_event(&ev); }
        });
        GPU.with(|c| *c.borrow_mut() = Some(made.clone()));
        Ok(made)
    }

    /// init: { dpr, look }。look が 'studio' なら陰影を付けて描く（look.rs）。色の出口は今の版の Canvas（composer なし・ACES・透ける canvas。材質は toneMapped なし）
    #[wasm_bindgen]
    pub async fn create_stage(canvas: web_sys::HtmlCanvasElement, init: String) -> Result<Stage, JsValue> {
        console_error_panic_hook::set_once();
        let v = parse(&init)?;
        let gpu = gpu().await.map_err(err)?;
        let mut kit = Kit::with_gpu(canvas, &gpu).map_err(err)?;
        kit.set_pixel_budget(1.0e12);
        kit.set_max_pixel(v["dpr"].as_f64().unwrap_or(1.0) as f32);
        kit.set_size_from_canvas(true);
        kit.output = Output::DirectAces;
        kit.bloom_enabled = false;
        kit.set_final(FINAL_DIRECT, 16);
        kit.set_final_uniform(&[0u8; 16]);
        kit.set_canvas_alpha(true);
        kit.clear_alpha = 0.0;
        Ok(Stage { kit, assets: Assets::default(), model: None, floor: None, look: crate::look::Look::default(), shaded: v["look"].as_str() == Some("studio") })
    }
}
