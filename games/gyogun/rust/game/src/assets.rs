// 素材の読み込み（assets.ts の役）。public/ の下を相対パスで読む。GLB は一度読んだら持っておく。
// ブラウザではファイルを同期で読めないので、preload で先に取っておき、read はそこから返す。
use crate::glb::{self, Glb};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

pub const BASE: &str = "aisns/";
/// 主役の配布の一覧（public/ の下。ゲームを配っている同じ場所に置く。docs/gyogun-spec.md）
pub const CATALOG_PATH: &str = "delivery/channels/stable/v1/index.json";
/// Mac で直接動かす版が読む一覧（pnpm dev が配る場所）。環境変数 GYOGUN_CATALOG で替えられる
pub const CATALOG_URL_NATIVE: &str = "http://127.0.0.1:4284/gyogun/delivery/channels/stable/v1/index.json";

/// ページの URL から、同じ場所にある配布の一覧の URL を作る（引数と # は除く）
pub fn catalog_url_for_page(href: &str) -> String {
    let page = &href[..href.find(['?', '#']).unwrap_or(href.len())];
    let dir = &page[..page.rfind('/').map_or(page.len(), |i| i + 1)];
    format!("{dir}{CATALOG_PATH}")
}

#[cfg(not(target_arch = "wasm32"))]
fn root() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("GYOGUN_PUBLIC") { return std::path::PathBuf::from(p); }
    std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../public"))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn read(rel: &str) -> Result<Vec<u8>, String> {
    let p = root().join(rel);
    std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))
}

#[cfg(target_arch = "wasm32")]
static FILES: OnceLock<Mutex<HashMap<String, Arc<Vec<u8>>>>> = OnceLock::new();

#[cfg(target_arch = "wasm32")]
pub fn read(rel: &str) -> Result<Vec<u8>, String> {
    FILES.get_or_init(Default::default).lock().unwrap().get(rel).map(|b| b.as_ref().clone()).ok_or_else(|| format!("{rel}: 読めませんでした"))
}

/// まとめて先に取っておく（ブラウザだけ）。パスはページからの相対。読めなかった物は入れない（使うときに read が失敗を返す。今の版と同じく、使う側が決める）
#[cfg(target_arch = "wasm32")]
pub async fn preload(rels: &[String]) {
    let urls: Vec<String> = rels.iter().map(|r| format!("./{r}")).collect();
    // 取りに行くのは同時（共通部品 game-ui 0.3 の page::fetch_all。字体と素材を 1 本ずつ待たない）
    let got = game_ui::page::fetch_all(&urls).await;
    let mut f = FILES.get_or_init(Default::default).lock().unwrap();
    for (r, b) in rels.iter().zip(got) { if let Ok(b) = b { f.insert(r.clone(), Arc::new(b)); } }
}

/// 始める前に要るファイル（字体を除く）
pub fn startup_files() -> Vec<String> {
    let mut v = Vec::new();
    for w in crate::hero::hero::WEAPONS.iter() {
        v.push(format!("{BASE}models/wilds/weapons/{}.glb", w.id));
        v.push(format!("{BASE}models/wilds/weapons/{}.png", w.id));
    }
    for p in crate::view::world::PROP_NAMES { v.push(format!("{BASE}models/wilds/props/{p}.glb")); }
    for c in crate::view::candy::NAMES { v.push(crate::view::candy::path(c)); }
    v.extend(crate::view::fish::files());
    v
}

static CACHE: OnceLock<Mutex<HashMap<String, Arc<Glb>>>> = OnceLock::new();

/// GLB を読む（同じファイルは 1 回だけ解く）
pub fn load_glb(rel: &str) -> Result<Arc<Glb>, String> {
    let c = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(g) = c.lock().unwrap().get(rel) { return Ok(g.clone()); }
    let g = Arc::new(glb::parse(&read(rel)?).map_err(|e| format!("{rel}: {e}"))?);
    c.lock().unwrap().insert(rel.to_string(), g.clone());
    Ok(g)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::delivery::{checked_file, checked_index, checked_manifest, hash, select_picture, DeliveryFile};
    use crate::hero::actor::{assemble, release_plan, Gait, LoadOptions, Quality};
    use serde_json::Value;

    // 防ぐ故障: 入れていない模型（スイーツ敵・角煮大王・お菓子の目印）を始めに読みに行って、読み込みが止まる
    #[test]
    fn 始めに読むファイルは浜辺の物だけで全部ある() {
        let files = startup_files();
        for f in &files {
            assert!(!f.contains("mobs/") && !f.contains("boss/") && !f.contains("candy_tower") && !f.contains("choco_rock"), "お菓子の面の模型が残っている: {f}");
            assert!(root().join(f).is_file(), "public に無い: {f}");
        }
        for want in ["models/fish/kihada.glb", "models/candy/anman.glb", "aisns/models/wilds/props/rock_large.glb", "aisns/models/wilds/weapons/gyoza_mace.glb"] {
            assert!(files.iter().any(|f| f == want), "一覧に無い: {want}");
        }
    }

    // 防ぐ故障: index.html や引数つきで開くと一覧の URL が壊れて、主役が読めない
    #[test]
    fn 配布の一覧はページと同じ場所から読む() {
        let want = "http://127.0.0.1:4284/gyogun/delivery/channels/stable/v1/index.json";
        for page in ["http://127.0.0.1:4284/gyogun/index.html", "http://127.0.0.1:4284/gyogun/", "http://127.0.0.1:4284/gyogun/index.html?auto&seed=1#x", "http://127.0.0.1:4284/gyogun/?hero=a/b"] {
            assert_eq!(catalog_url_for_page(page), want, "{page}");
        }
        assert_eq!(crate::delivery::release_root(want).unwrap(), "http://127.0.0.1:4284/gyogun/delivery/");
        assert_eq!(CATALOG_URL_NATIVE, want);
    }

    fn on_disk(file: &DeliveryFile) -> Vec<u8> {
        let b = read(&format!("delivery/{}", file.path)).unwrap();
        assert_eq!((b.len() as u64, hash(&b)), (file.bytes, file.sha256.clone()), "中身が台帳と違う: {}", file.path);
        b
    }

    // 防ぐ故障: 手元の配布に、きゃしー以外の主役・古い版が混ざる。台帳どおりのファイルが無くて主役が組み立てられない
    #[test]
    fn 配布はきゃしー1体で主役を組み立てられる() {
        let index = checked_index(serde_json::from_slice(&read(CATALOG_PATH).unwrap()).unwrap()).unwrap();
        let chars = index["characters"].as_object().unwrap();
        assert_eq!(chars.keys().collect::<Vec<_>>(), ["cassie"]);
        let entry = &chars["cassie"];
        let rev = entry["current"].as_str().unwrap();
        assert_eq!(entry["releases"].as_object().unwrap().keys().collect::<Vec<_>>(), [rev]);
        let release = checked_file(&entry["releases"][rev]).unwrap();
        let manifest: Value = checked_manifest(serde_json::from_slice(&on_disk(&release)).unwrap(), "cassie", rev).unwrap();
        // 主役の組み立て（軽量版）に要る物
        let plan = release_plan(&manifest, Quality::Lite);
        let mut data = std::collections::HashMap::new();
        for key in &plan.files {
            let f = checked_file(&manifest["files"][key]).unwrap_or_else(|e| panic!("{key}: {e}"));
            data.insert(key.clone(), Arc::new(on_disk(&f)));
        }
        let o = LoadOptions { id: "cassie".into(), revision: None, quality: Quality::Lite, gait: Gait { walk: 0.3, run: 2.8 } };
        if let Err(e) = assemble(&o, &plan, &manifest, &data) { panic!("主役を組み立てられない: {e}"); }
        // 選ぶ画面と HUD の絵（delivery.rs の hero_entry と同じ選び方）
        let select = select_picture(&manifest).0.expect("選ぶ画面の絵が無い");
        let thumb = checked_file(&manifest["files"]["portrait:bust"]).unwrap();
        on_disk(&select);
        on_disk(&thumb);
        // 読まない物（12MB の full など）は置かない
        let mut want: Vec<String> = plan.files.iter().map(|k| manifest["files"][k]["path"].as_str().unwrap().to_string()).chain([select.path, thumb.path]).collect();
        want.sort();
        let mut have: Vec<String> = std::fs::read_dir(root().join("delivery/assets")).unwrap().map(|e| format!("assets/{}", e.unwrap().file_name().to_string_lossy())).collect();
        have.sort();
        assert_eq!(have, want);
    }
}
