// 走る人たちの名簿（docs/gyogyo-spec.md の「走る人たち」）。
// きゃしー: このゲームと同じ場所の配布（public/delivery。魚群繚乱と同じ 1 体の配布）から読む。
// つくったキャラ: Studio（/character-maker）が送った .character.zip（index.html が window.__gyogyoStudioZip に置く）。
// 魚 10 種（Fish Kit）・イカ 2 種（Squid Kit）: public/models/fish・public/squid。
use crate::view::runner::{Body, COMMON_SCALE, HERO_HEIGHT, HeroInfo};
use character_actor::actor::{CharacterActor, Gait};
use character_actor::delivery::{Catalog, HeroEntry};
use character_actor::package::{self, CharacterAsset};
use sim::hero_signals::hero_gait;
use std::sync::Arc;

/// 配布の一覧（public/ の下。ゲームを配っている同じ場所）
pub const CATALOG_PATH: &str = "delivery/channels/stable/v1/index.json";
/// つくったキャラの id（保存のキーに使う。送り直しても同じ）
pub const GUEST_ID: &str = "studio";

/// ページの URL から、同じ場所にある配布の一覧の URL を作る（引数と # は除く）
pub fn catalog_url_for_page(href: &str) -> String {
    let page = &href[..href.find(['?', '#']).unwrap_or(href.len())];
    let dir = &page[..page.rfind('/').map_or(page.len(), |i| i + 1)];
    format!("{dir}{CATALOG_PATH}")
}

pub fn open_browser_catalog() -> Result<Catalog, String> {
    let href = web_sys::window().and_then(|w| w.location().href().ok()).ok_or("ページの場所が分からない")?;
    Catalog::open_index_web(&catalog_url_for_page(&href), false)
}

/// 配布中の全主役（取れなかったキャラは外す）
pub async fn heroes(c: &Catalog) -> Result<Vec<HeroEntry>, String> { c.heroes_web().await }

/// 配布の主役を部品で読む（軽量版。読むファイルは全部 SHA を照合済み）
pub async fn hero(c: &Catalog, id: &str) -> Result<CharacterActor, String> {
    let (walk, run) = hero_gait(COMMON_SCALE);
    c.hero_web(id, Gait { walk, run }).await
}

/// Studio が送った .character.zip（無い・読めないときは None。理由は console へ）
pub fn studio_zip() -> Option<Vec<u8>> {
    let w = web_sys::window()?;
    let v = js_sys::Reflect::get(&w, &"__gyogyoStudioZip".into()).ok()?;
    if v.is_undefined() || v.is_null() { return None; }
    Some(js_sys::Uint8Array::new(&v).to_vec())
}

/// つくったキャラを名簿の 1 人にする。大きさは、背がきゃしー（配布の主役）と同じになる倍率
pub fn guest_hero(zip: &[u8]) -> Result<HeroInfo, String> {
    let asset = package::read_character_file(zip, "studio.character.zip")?;
    let m = &asset.manifest;
    let height = m["coordinates"]["heightMeters"].as_f64().unwrap_or(1.0).max(0.3);
    let scale = HERO_HEIGHT / height;
    Ok(HeroInfo {
        id: GUEST_ID.to_string(), name: m["name"].as_str().filter(|s| !s.is_empty()).unwrap_or("つくったキャラ").to_string(),
        thumb: String::new(), portrait: false, height: height as f32, select_file: None, scale,
        body: Body::Guest(Arc::new(asset)),
    })
}

/// つくったキャラを主役の部品にする（GPU は触らない）
pub fn guest_actor(asset: &CharacterAsset, scale: f64) -> Result<CharacterActor, String> {
    let mut actor = package::assemble_package(asset, None)?;
    let (walk, run) = hero_gait(scale);
    actor.set_gait(Gait { walk, run });
    Ok(actor)
}

/// 同じ場所のファイルを読む（魚とイカの選ぶ画面の絵）
pub async fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    let w = web_sys::window().ok_or("window が無い")?;
    let e = |e: wasm_bindgen::JsValue| format!("{url}: {e:?}");
    let resp: web_sys::Response = JsFuture::from(w.fetch_with_str(url)).await.map_err(e)?.dyn_into().map_err(e)?;
    if !resp.ok() { return Err(format!("{url}: {}", resp.status())); }
    let buf = JsFuture::from(resp.array_buffer().map_err(e)?).await.map_err(e)?;
    Ok(js_sys::Uint8Array::new(&buf).to_vec())
}
