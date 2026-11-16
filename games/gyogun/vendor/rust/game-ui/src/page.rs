// ブラウザのページから取る物（字体などのファイル・端の安全域）。0.3（2026-11-11）: 各ゲームが別々に書いていた物を共通にした。
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

fn text(e: JsValue) -> String { e.as_string().unwrap_or_else(|| format!("{e:?}")) }

/// ページからの相対の場所を 1 つ取る
pub async fn fetch(url: &str) -> Result<Vec<u8>, String> {
    let win = web_sys::window().ok_or("window がありません")?;
    let resp: web_sys::Response = JsFuture::from(win.fetch_with_str(url)).await.map_err(text)?.dyn_into().map_err(text)?;
    if !resp.ok() { return Err(format!("{url}: HTTP {}", resp.status())); }
    let buf = JsFuture::from(resp.array_buffer().map_err(text)?).await.map_err(text)?;
    Ok(js_sys::Uint8Array::new(&buf).to_vec())
}

/// まとめて取る（取りに行くのは同時。Promise.all と同じ）。返る並びは urls と同じ。
/// 字体を 1 本ずつ待つと、本数ぶんの往復が起動に足される（6 本で約 4.5MB）
pub async fn fetch_all(urls: &[String]) -> Vec<Result<Vec<u8>, String>> {
    let waits: Vec<js_sys::Promise> = urls.iter().map(|u| {
        let u = u.clone();
        wasm_bindgen_futures::future_to_promise(async move {
            fetch(&u).await.map(|b| js_sys::Uint8Array::from(&b[..]).into()).map_err(|e| JsValue::from_str(&e))
        })
    }).collect();
    let mut out = Vec::with_capacity(waits.len());
    for p in waits { out.push(JsFuture::from(p).await.map(|v| js_sys::Uint8Array::new(&v).to_vec()).map_err(text)); }
    out
}

/// 端の安全域（ノッチ・角の丸み・ホームバー）: [上, 右, 下, 左] の論理 px。0.5 から、URL に ?safe=上,右,下,左 があればその値を返す。ページの viewport に viewport-fit=cover が要る。
/// 端末の向きで変わるので、窓の大きさが変わったら呼び直す（毎コマは呼ばない。ページに物を 1 つ足して測る）
pub fn safe_area() -> [f32; 4] {
    // 確かめ用の ?safe=上,右,下,左 があれば、その値（実機が無くても端の寄せ方を見られる）
    if let Some(v) = web_sys::window().and_then(|w| w.location().search().ok()).and_then(|q| crate::menu::safe_from_query(&q)) { return v; }
    let read = || -> Option<[f32; 4]> {
        let win = web_sys::window()?;
        let doc = win.document()?;
        let probe = doc.create_element("div").ok()?;
        probe.set_attribute("style", "position:fixed;left:0;top:0;width:0;height:0;visibility:hidden;pointer-events:none;\
            padding:env(safe-area-inset-top) env(safe-area-inset-right) env(safe-area-inset-bottom) env(safe-area-inset-left)").ok()?;
        doc.body()?.append_child(&probe).ok()?;
        let st = win.get_computed_style(&probe).ok().flatten();
        let px = |k: &str| st.as_ref().and_then(|s| s.get_property_value(k).ok()).and_then(|v| v.trim().trim_end_matches("px").parse::<f32>().ok()).unwrap_or(0.0);
        let out = [px("padding-top"), px("padding-right"), px("padding-bottom"), px("padding-left")];
        probe.remove();
        Some(out)
    };
    read().unwrap_or([0.0; 4])
}

/// 端末の「動きを減らす」設定（prefers-reduced-motion: reduce）。紙吹雪・揺れ・点滅を控えめにする判断に使う
pub fn reduced_motion() -> bool {
    web_sys::window().and_then(|w| w.match_media("(prefers-reduced-motion: reduce)").ok().flatten()).is_some_and(|m| m.matches())
}
