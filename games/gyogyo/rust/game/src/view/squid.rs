// 走者のイカ（アオリイカ aori・コウイカ kouika）。Squid Kit（CC0、three.js 用の squid-kit.js）の写し。GLB ではなく、写真と形の JSON から体を作り、
// 毎コマ全頂点を作り直す。形と動きの式はブラウザに頼らない squid_shape.rs（ネイティブで試験する）、ここは読み込みと island-kit への受け渡し。
// 体は「進む向き +Z（外套の先が前）・背が +Y・全長（外套の先から腕の先）が length_m・体の真ん中が原点」。
// 描く物は 2 つ：体（胴・頭・目・腕・触腕・漏斗。両面）と、ひれ（写真から決めた透け。表と裏の 2 枚）。素材の置き場は public/squid（README.md）。
use crate::squid_shape::{self, Rig};
use island_kit::geometry::Geometry;
use island_kit::material::Soft;
use island_kit::{GeoId, Kit, MatId, MatKind, Material, NodeId, Side, TexId};
use std::cell::Cell;
use std::sync::Arc;

pub use crate::squid_shape::SquidDrive;

/// 1 種類のイカの形の値と、3 枚の写真を部品ごとに焼いた 1 枚の模様（sRGB の RGBA）
pub struct SquidData {
    pub id: String,
    rig: Rig,
    atlas: Vec<u8>,
    /// 模様を GPU へ上げた物（同じ種類の体で 1 枚を使う）
    tex: Cell<Option<TexId>>,
}

impl SquidData {
    fn texture(&self, kit: &mut Kit) -> TexId {
        if let Some(t) = self.tex.get() { return t; }
        // 写真の色を濃くする（魚と同じ fish::vivid）
        let mut rgba = self.atlas.clone();
        super::fish::vivid(&mut rgba);
        let t = kit.data_texture(self.rig.atlas_w, self.rig.atlas_h, &rgba, true);
        self.tex.set(Some(t));
        t
    }
}

async fn fetch_text(url: &str) -> Result<String, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    let err = |e: wasm_bindgen::JsValue| format!("{url}: {e:?}");
    let win = web_sys::window().ok_or("window が無い")?;
    let r: web_sys::Response = JsFuture::from(win.fetch_with_str(url)).await.map_err(err)?.dyn_into().map_err(err)?;
    if !r.ok() { return Err(format!("{url}: HTTP {}", r.status())); }
    JsFuture::from(r.text().map_err(err)?).await.map_err(err)?.as_string().ok_or_else(|| format!("{url}: 文字ではない"))
}

/// 例: load_squid("aori").await。ページから相対の squid/<id>.json と squid/<id>_dorsal / _ventral / _side.webp を読む
pub async fn load_squid(id: &str) -> Result<Arc<SquidData>, String> {
    let sp = squid_shape::species(id).ok_or_else(|| format!("イカ {id} は無い（aori・kouika）"))?;
    let shape = squid_shape::parse_shape(&fetch_text(&format!("squid/{id}.json")).await?)?;
    let d = island_kit::texture::load_image(&format!("squid/{id}_dorsal.webp")).await?;
    let v = island_kit::texture::load_image(&format!("squid/{id}_ventral.webp")).await?;
    let s = island_kit::texture::load_image(&format!("squid/{id}_side.webp")).await?;
    let rig = Rig::new(sp, &shape, squid_shape::MID);
    let atlas = rig.bake_atlas(&shape, [(d.width, d.height, &d.rgba), (v.width, v.height, &v.rgba), (s.width, s.height, &s.rgba)]);
    #[allow(clippy::arc_with_non_send_sync)]
    Ok(Arc::new(SquidData { id: id.to_string(), rig, atlas, tex: Cell::new(None) }))
}

thread_local! {
    /// 体ごとに漏斗のゆれなどをずらす
    static SEED: Cell<u32> = const { Cell::new(0) };
}

pub struct SquidBody {
    rig: Rig,
    /// 体の入れ物（parent の子。体とひれはこの下）
    pub root: NodeId,
    pub body: NodeId,
    pub fins: NodeId,
    body_geo: GeoId,
    fin_geo: GeoId,
    body_mat: MatId,
    gb: Geometry,
    gf: Geometry,
    /// よろけの白さ（今の値）
    white: f32,
    pub length: f32,
}

impl SquidBody {
    /// parent の下に体を作る。体は「進む向き +Z・背が +Y・全長（外套の先から腕の先）が length_m・体の真ん中が原点」。
    /// イカは外套の先（とがった方）を前にして進む（ジェットで泳ぐときの向き）
    pub fn new(kit: &mut Kit, parent: NodeId, data: &Arc<SquidData>, length_m: f32) -> SquidBody {
        let tex = data.texture(kit);
        let mut rig = data.rig.clone();
        let n = SEED.get();
        SEED.set(n + 1);
        rig.fun_seed = n as f64 * 2.37;
        rig.set_length(length_m as f64);
        rig.build(0.0);
        let (gb, gf) = rig.new_geometries();
        let body_geo = kit.geometry(&gb);
        let fin_geo = kit.geometry(&gf);
        // 写真の色を自分で少し光らせる（fish.rs の creature_material。夕暮れ・夜のコースで暗く沈まない）
        let body_mat = kit.material(Material { side: Side::Double, ..super::fish::creature_material(tex) });
        // ひれは 1 枚の板の表（背中の写真）と裏（お腹の写真）を別の 2 枚にする（表だけ描く）。透けは模様のアルファ
        let fin_mat = kit.material(Material { transparent: true, ..super::fish::creature_material(tex) });
        let root = kit.scene.group(Some(parent));
        let body = kit.scene.mesh(Some(root), body_geo, body_mat);
        let fins = kit.scene.mesh(Some(root), fin_geo, fin_mat);
        for m in [body, fins] {
            kit.scene[m].cast_shadow = true;
            kit.scene[m].receive_shadow = true;
        }
        SquidBody { rig, root, body, fins, body_geo, fin_geo, body_mat, gb, gf, white: 0.0, length: length_m }
    }

    /// 体の厚み（背から腹まで、parent の空間）
    pub fn height(&self) -> f32 { self.rig.height() as f32 }

    /// 毎コマ。頂点を動かして kit.update_geometry で上げる
    pub fn update(&mut self, kit: &mut Kit, dt: f64, now: f64, d: &SquidDrive) {
        self.rig.update(dt, now, d);
        self.rig.fill(&mut self.gb, &mut self.gf);
        kit.update_geometry(self.body_geo, &self.gb);
        kit.update_geometry(self.fin_geo, &self.gf);
        // よろけ：驚いて白く抜ける（写真の自発光を強める。段を付けて、変わったときだけ材質を書き換える）
        let w = ((d.hurt.clamp(0.0, 1.0) * 4.0).round() / 4.0 * 0.35) as f32;
        if w != self.white {
            self.white = w;
            if let MatKind::Soft(s) = &kit.mat(self.body_mat).kind {
                let s = Soft { emissive_intensity: super::fish::CREATURE_GLOW + w * 2.0, ..*s };
                kit.mat_mut(self.body_mat).kind = MatKind::Soft(s);
            }
        }
    }

    /// 縁取りの層（自機だけ縁取る）
    pub fn set_outline(&self, kit: &mut Kit, on: bool) {
        for m in [self.body, self.fins] {
            if on { kit.scene[m].layers |= 2 } else { kit.scene[m].layers &= !2 }
        }
    }

    pub fn set_visible(&self, kit: &mut Kit, on: bool) { kit.scene[self.root].visible = on; }
}
