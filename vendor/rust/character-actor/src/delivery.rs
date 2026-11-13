// キャラは characters の共有配布だけから読む（2026-11-02 Owner 決定: 主役のみ・ゲームはキャラのデータを持たない）。
// 一覧は配布中の全主役、モデルは軽量版（lite）。取得・SHA の照合・キャッシュは characters の client（vendor/character-delivery/client.mjs）と src/character-delivery.ts の写し。
// 公開の GET だけで、認証情報は使わない。主役の読込み・動き・＞＜ は actor.rs（共通ルール。ゲームは合図だけを出す）。
use crate::actor::{self, CharacterActor, Gait, LoadOptions};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

pub const CAPABILITIES: [&str; 3] = ["character-package-v1", "gltf-meshopt-v1", "optional-weapon-v1"];
// 2026-11-03 Owner: 開発中は characters のローカル配布（packages/character-delivery/local.mjs、ポート 4259）が起動していればそれ、無ければ本番。本番のビルドは本番だけ
pub const PRODUCTION_INDEX_URL: &str = "https://characters.aiarranger.jp/channels/stable/v1/index.json?channel=stable";
pub const LOCAL_INDEX_URL: &str = "http://127.0.0.1:4259/channels/stable/v1/index.json";
pub const CATALOG_URL: &str = PRODUCTION_INDEX_URL;
const MAX_FILE: u64 = 128 * 1024 * 1024;
const TIMEOUT_MS: u64 = 6000;
const ASSET_TIMEOUT_MS: u64 = 30_000;

pub fn hash(bytes: &[u8]) -> String {
    let d = Sha256::digest(bytes);
    let mut s = String::with_capacity(64);
    for b in d { s.push_str(&format!("{b:02x}")); }
    s
}

fn is_sha(s: &str) -> bool { s.len() == 64 && s.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)) }

/// /^[a-z0-9][a-z0-9._-]{0,79}$/
pub fn identifier(v: &str) -> Result<&str, String> {
    let ok = !v.is_empty() && v.len() <= 80 && v.bytes().enumerate().all(|(i, c)| c.is_ascii_lowercase() || c.is_ascii_digit() || (i > 0 && matches!(c, b'.' | b'_' | b'-')));
    if ok { Ok(v) } else { Err("Invalid character/release identifier".into()) }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DeliveryFile {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

// モデル・画像など assets/ の取得だけ長く待つ。一覧と releases/ の manifest は従来どおり。
fn file_timeout_ms(file: &DeliveryFile) -> u64 {
    if file.path.starts_with("assets/") { ASSET_TIMEOUT_MS } else { TIMEOUT_MS }
}

pub fn checked_file(file: &Value) -> Result<DeliveryFile, String> {
    let bad = || "Invalid release file".to_string();
    let path = file.get("path").and_then(Value::as_str).ok_or_else(bad)?;
    let sha = file.get("sha256").and_then(Value::as_str).ok_or_else(bad)?;
    let bytes = file.get("bytes").and_then(Value::as_u64).ok_or_else(bad)?;
    let tail = path.strip_prefix("assets/").or_else(|| path.strip_prefix("releases/")).ok_or_else(bad)?;
    if tail.is_empty() || !path.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'/' | b'-')) { return Err(bad()); }
    if path.split('/').any(|s| s.is_empty() || s == ".." || s == ".") || !is_sha(sha) || !(1..=MAX_FILE).contains(&bytes) { return Err(bad()); }
    Ok(DeliveryFile { path: path.to_string(), sha256: sha.to_string(), bytes })
}

/// 一覧の URL から、配布の根（file.path の起点）を出す
pub fn release_root(index_url: &str) -> Result<String, String> {
    let bad = || "Invalid release index URL".to_string();
    let (scheme, rest) = index_url.split_once("://").ok_or_else(bad)?;
    if scheme != "https" && scheme != "http" { return Err(bad()); }
    let end = rest.find(['?', '#']).unwrap_or(rest.len());
    let rest = &rest[..end];
    let (host, path) = rest.split_once('/').ok_or_else(bad)?;
    if host.is_empty() || host.contains('@') { return Err(bad()); }
    let dir = format!("/{path}");
    let head = dir.strip_suffix("/channels/stable/v1/index.json").ok_or_else(bad)?;
    Ok(format!("{scheme}://{host}{head}/"))
}

pub fn file_url(root: &str, file: &DeliveryFile) -> String { format!("{root}{}?v={}", file.path, file.sha256) }

pub fn checked_index(index: Value) -> Result<Value, String> {
    let bad = || "Unsupported release index".to_string();
    if index.get("schemaVersion").and_then(Value::as_i64) != Some(1) { return Err(bad()); }
    let chars = index.get("characters").and_then(Value::as_object).ok_or_else(bad)?;
    for (id, entry) in chars {
        identifier(id)?;
        let current = entry.get("current").and_then(Value::as_str).ok_or("Invalid character/release identifier")?;
        identifier(current)?;
        let releases = entry.get("releases").and_then(Value::as_object).ok_or("Current release is missing")?;
        if !releases.contains_key(current) { return Err("Current release is missing".into()); }
        for (rev, file) in releases { identifier(rev)?; checked_file(file)?; }
    }
    Ok(index)
}

fn sget<'a>(v: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut c = v;
    for k in path { c = c.get(k)?; }
    Some(c)
}

pub fn checked_manifest(manifest: Value, id: &str, revision: &str) -> Result<Value, String> {
    let m = &manifest;
    let requires = m.get("requires").and_then(Value::as_array);
    let ok = m.get("schemaVersion").and_then(Value::as_i64) == Some(1) && m.get("id").and_then(Value::as_str) == Some(id) && m.get("revision").and_then(Value::as_str) == Some(revision)
        && m.get("kind").and_then(Value::as_str) == Some("hero")
        && requires.is_some_and(|r| r.iter().all(|c| c.as_str().is_some_and(|s| CAPABILITIES.contains(&s))));
    if !ok { return Err("Unsupported character release".into()); }
    let full_sha = sget(m, &["files", "full", "sha256"]).and_then(Value::as_str);
    if m.get("name").and_then(Value::as_str).is_none() || sget(m, &["packageManifest", "id"]).and_then(Value::as_str) != Some(id)
        || sget(m, &["packageManifest", "model", "sha256"]).and_then(Value::as_str).is_none()
        || sget(m, &["packageManifest", "model", "sha256"]).and_then(Value::as_str) != full_sha {
        return Err("Character package and release disagree".into());
    }
    let files = m.get("files").and_then(Value::as_object).ok_or("Invalid release file")?;
    for f in files.values() { checked_file(f)?; }
    let optional = requires.unwrap().iter().any(|c| c.as_str() == Some("optional-weapon-v1"));
    if !files.contains_key("full") || !files.contains_key("package") || !files.contains_key("normal") || !files.contains_key("lite") || (!files.contains_key("weapon") && !optional) {
        return Err("Incomplete character release".into());
    }
    let full_sha = full_sha.unwrap_or("");
    let registry = m.get("registry");
    if registry.and_then(|r| r.get("id")).and_then(Value::as_str) != Some(id) || registry.and_then(|r| sget(r, &["delivered", "modelSha256"])).and_then(Value::as_str) != Some(full_sha) {
        return Err("Motion registry and release disagree".into());
    }
    let registry = registry.unwrap();
    let profile = registry.get("profile");
    let has_assets = ["stand", "standSmall", "bust"].iter().any(|k| files.contains_key(&format!("portrait:{k}")));
    if registry.get("profileAssetsVersion").is_some() || has_assets {
        if registry.get("profileAssetsVersion").and_then(Value::as_i64) != Some(1) || profile.is_none() { return Err("Unsupported character profile assets".into()); }
        let profile = profile.unwrap();
        if sget(profile, &["portraits", "madeFor"]).and_then(Value::as_str) != Some(&full_sha[..12.min(full_sha.len())]) { return Err("Character profile and release disagree".into()); }
        for key in ["stand", "standSmall", "bust"] {
            let portrait = sget(profile, &["portraits", "files", key]);
            let file = files.get(&format!("portrait:{key}"));
            let same = portrait.is_some_and(|p| p.get("file").is_some()) && file.is_some() && portrait.and_then(|p| p.get("sha256")) == file.and_then(|f| f.get("sha256"));
            if !same { return Err("Character profile portrait and release disagree".into()); }
        }
    }
    Ok(manifest)
}

fn manifest_file(m: &Value, key: &str) -> Option<DeliveryFile> { m.get("files")?.get(key).and_then(|f| checked_file(f).ok()) }

/// 2026-11-04 Owner: どのゲームでもキャラ選びに出す絵。portrait は頭と肩で全身ではない
pub fn select_picture(manifest: &Value) -> (Option<DeliveryFile>, bool) {
    match manifest_file(manifest, "portrait:select").or_else(|| manifest_file(manifest, "portrait:bust")) {
        Some(f) => (Some(f), true),
        None => (manifest_file(manifest, "thumbnail"), false),
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum IndexSource { Local, Production, Override }

#[derive(Clone, Debug)]
pub struct IndexChoice { pub url: String, pub source: IndexSource }

#[cfg(not(target_arch = "wasm32"))]
fn http_get(url: &str, timeout_ms: u64) -> Result<Vec<u8>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_millis(timeout_ms))).build().into();
    let mut res = agent.get(url).call().map_err(|e| format!("Character release unavailable: {e}"))?;
    res.body_mut().with_config().limit(MAX_FILE + 1024).read_to_vec().map_err(|e| format!("Character release unavailable: {e}"))
}

/// 読む一覧を決める（characters の chooseIndexUrl）。override があればそれ、開発中はローカル配布が起動していればそれ、無ければ本番
#[cfg(not(target_arch = "wasm32"))]
pub fn choose_index_url(development: bool, override_url: Option<String>) -> IndexChoice {
    if let Some(url) = override_url.filter(|u| !u.is_empty()) { return IndexChoice { url, source: IndexSource::Override }; }
    if development && http_get(LOCAL_INDEX_URL, 800).is_ok() { return IndexChoice { url: LOCAL_INDEX_URL.into(), source: IndexSource::Local }; }
    IndexChoice { url: PRODUCTION_INDEX_URL.into(), source: IndexSource::Production }
}

pub fn describe_index_source(c: &IndexChoice) -> String {
    let s = match c.source { IndexSource::Local => "キャラ: ローカル（characters）", IndexSource::Production => "キャラ: 本番の配布", IndexSource::Override => "キャラ: 指定の一覧" };
    format!("{s} {}", c.url)
}

#[cfg(not(target_arch = "wasm32"))]
fn write_atomic(path: &std::path::Path, bytes: &[u8]) {
    // 残せない環境でも、その場では読める
    if let Some(dir) = path.parent() { let _ = std::fs::create_dir_all(dir); }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    if std::fs::write(&tmp, bytes).is_ok() && std::fs::rename(&tmp, path).is_err() { let _ = std::fs::remove_file(&tmp); }
}

/// SHA を確かめた 1 組（characters の prepare の結果）
pub struct Prepared {
    pub manifest: Arc<Value>,
    pub data: HashMap<String, Arc<Vec<u8>>>,
    pub root: String,
}

#[derive(Default)]
struct State {
    index: Option<Arc<Value>>,
    prepared: HashMap<String, Arc<Prepared>>,
    previous: HashMap<String, Arc<Prepared>>,
    /// 同期の読み込みへ渡す、取り終えた 1 人分（ブラウザだけ）
    #[cfg(target_arch = "wasm32")]
    loaded: HashMap<String, Arc<Prepared>>,
}

/// characters の createDeliveryClient。store は読んだ一覧と版を残す所、cache はファイルを SHA の名前で残す所（無ければ残さない）
#[cfg(not(target_arch = "wasm32"))]
pub struct DeliveryClient {
    pub index_url: String,
    pub root: String,
    pub store: Option<PathBuf>,
    pub cache: Option<PathBuf>,
    state: Mutex<State>,
}

#[cfg(not(target_arch = "wasm32"))]
impl DeliveryClient {
    pub fn new(index_url: &str, store: Option<PathBuf>, cache: Option<PathBuf>) -> Result<DeliveryClient, String> {
        Ok(DeliveryClient { root: release_root(index_url)?, index_url: index_url.to_string(), store, cache, state: Mutex::default() })
    }

    fn get_stored(&self, name: &str) -> Option<Value> {
        let p = self.store.as_ref()?.join(name);
        serde_json::from_slice(&std::fs::read(p).ok()?).ok()
    }
    fn set_stored(&self, name: &str, v: &Value) {
        if let Some(d) = &self.store { write_atomic(&d.join(name), v.to_string().as_bytes()); }
    }

    /// 1 ファイルを読む。中身の SHA で決まるファイルは変わらないので、控えがあれば確かめて通信せずに使う
    pub fn read_file(&self, file: &DeliveryFile) -> Result<Vec<u8>, String> {
        let ok = |b: &[u8]| b.len() as u64 == file.bytes && hash(b) == file.sha256;
        let cached = self.cache.as_ref().map(|d| d.join(&file.sha256));
        if let Some(p) = &cached && let Ok(b) = std::fs::read(p) && ok(&b) { return Ok(b); }
        match http_get(&file_url(&self.root, file), file_timeout_ms(file)) {
            Ok(b) => {
                if !ok(&b) { return Err("Character release integrity mismatch".into()); }
                if let Some(p) = &cached { write_atomic(p, &b); }
                Ok(b)
            }
            Err(e) => Err(e),
        }
    }

    pub fn refresh(&self) -> Result<Arc<Value>, String> {
        let got = http_get(&self.index_url, TIMEOUT_MS).and_then(|b| {
            if b.len() > 1024 * 1024 { return Err("Release index is too large".to_string()); }
            let v: Value = serde_json::from_slice(&b).map_err(|e| e.to_string())?;
            checked_index(v)
        });
        let mut st = self.state.lock().unwrap();
        match got {
            Ok(v) => {
                self.set_stored("index.json", &v);
                st.index = Some(Arc::new(v));
            }
            Err(e) => {
                if st.index.is_none() {
                    match self.get_stored("index.json") {
                        Some(v) => st.index = Some(Arc::new(checked_index(v)?)),
                        None => return Err(e),
                    }
                }
            }
        }
        Ok(st.index.clone().unwrap())
    }

    pub fn manifest(&self, id: &str, revision: Option<&str>) -> Result<Option<Arc<Value>>, String> {
        identifier(id)?;
        let have = self.state.lock().unwrap().index.clone();
        let index = match have { Some(i) => i, None => self.refresh()? };
        let Some(entry) = sget(&index, &["characters", id]) else { return Ok(None) };
        let Some(selected) = revision.or_else(|| entry.get("current").and_then(Value::as_str)) else { return Ok(None) };
        let Some(file) = sget(entry, &["releases", selected]) else { return Ok(None) };
        let bytes = self.read_file(&checked_file(file)?)?;
        let v: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        Ok(Some(Arc::new(checked_manifest(v, id, selected)?)))
    }

    /// 1 人分を全部そろえてから返す（全部の SHA を確かめてから使う）。取れなければ前に完全に取れた版
    pub fn prepare(&self, id: &str, revision: Option<&str>, files: &[String], fallback: bool) -> Result<Option<Arc<Prepared>>, String> {
        match self.prepare_inner(id, revision, files) {
            Ok(v) => Ok(v),
            Err(error) => {
                if !fallback || revision.is_some() { return Err(error); }
                if let Some(before) = self.state.lock().unwrap().previous.get(id).cloned() && files.iter().all(|k| before.data.contains_key(k)) { return Ok(Some(before)); }
                if let Some(stored) = self.get_stored(&format!("complete-{id}.json")) {
                    let rev = sget(&stored, &["manifest", "revision"]).and_then(Value::as_str).unwrap_or("").to_string();
                    let old = Arc::new(checked_manifest(stored["manifest"].clone(), id, &rev)?);
                    let mut data = HashMap::new();
                    for name in files {
                        let f = manifest_file(&old, name).ok_or_else(|| format!("Required character file is missing: {name}"))?;
                        data.insert(name.clone(), Arc::new(self.read_file(&f)?));
                    }
                    let value = Arc::new(Prepared { manifest: old, data, root: self.root.clone() });
                    self.state.lock().unwrap().previous.insert(id.to_string(), value.clone());
                    return Ok(Some(value));
                }
                Err(error)
            }
        }
    }

    fn prepare_inner(&self, id: &str, revision: Option<&str>, files: &[String]) -> Result<Option<Arc<Prepared>>, String> {
        let Some(selected) = self.manifest(id, revision)? else { return Ok(None) };
        let rev = selected.get("revision").and_then(Value::as_str).unwrap_or("").to_string();
        let mut wanted: Vec<String> = Vec::new();
        for f in files { if !wanted.contains(f) { wanted.push(f.clone()); } }
        let mut sorted = wanted.clone();
        sorted.sort();
        let key = format!("{id}:{rev}:{}", sorted.join(","));
        if let Some(v) = self.state.lock().unwrap().prepared.get(&key).cloned() { return Ok(Some(v)); }
        let mut data = HashMap::new();
        for name in &wanted {
            let f = manifest_file(&selected, name).ok_or_else(|| format!("Required character file is missing: {name}"))?;
            data.insert(name.clone(), Arc::new(self.read_file(&f)?));
        }
        let value = Arc::new(Prepared { manifest: selected.clone(), data, root: self.root.clone() });
        let mut st = self.state.lock().unwrap();
        st.prepared.insert(key, value.clone());
        st.previous.insert(id.to_string(), value.clone());
        drop(st);
        self.set_stored(&format!("complete-{id}.json"), &json!({ "manifest": *selected, "files": wanted }));
        Ok(Some(value))
    }
}

/// 一覧の 1 人（src/character-delivery.ts の HeroEntry）
#[derive(Clone)]
pub struct HeroEntry {
    pub id: String,
    pub name: String,
    /// 身長（メートル）
    pub height: f32,
    /// 動作・表情の設定（character-runtime に渡す台帳）
    pub manifest: Arc<Value>,
    /// キャラ選びの絵（肖像。無ければサムネイル）。thumb は HUD とカットイン
    pub thumb: String,
    pub thumb_file: Option<DeliveryFile>,
    /// キャラ選びの絵（2026-11-04 Owner）
    pub select: String,
    pub select_file: Option<DeliveryFile>,
    /// select が肖像（頭と肩）か（TS の selectPicture(m).portrait）
    pub select_portrait: bool,
    /// thumb が肖像（頭と肩）か。サムネイル（全身）なら表示の側で顔のあたりを拡大する
    pub portrait: bool,
}

/// 版の台帳から一覧の 1 行を作る
fn hero_entry(m: &Value, root: &str) -> Option<HeroEntry> {
    let (select_file, select_portrait) = select_picture(m);
    let thumb_file = manifest_file(m, "portrait:bust").or_else(|| manifest_file(m, "thumbnail"));
    let url = |f: &Option<DeliveryFile>| f.as_ref().map(|f| file_url(root, f)).unwrap_or_default();
    Some(HeroEntry {
        id: m.get("id")?.as_str()?.to_string(),
        name: m.get("name")?.as_str()?.to_string(),
        height: sget(m, &["actor", "heightMeters"])?.as_f64()? as f32,
        manifest: Arc::new(m.get("packageManifest")?.clone()),
        thumb: url(&thumb_file),
        select: url(&select_file),
        portrait: m.get("files")?.get("portrait:bust").is_some(),
        thumb_file,
        select_file,
        select_portrait,
    })
}

/// src/character-delivery.ts の openCatalog
#[derive(Clone)]
pub struct Catalog { pub client: Arc<DeliveryClient>, pub choice: IndexChoice }

#[cfg(not(target_arch = "wasm32"))]
impl Catalog {
    /// 指定の一覧を、控えを残さずに開く
    pub fn open(index_url: &str, store: Option<PathBuf>, cache: Option<PathBuf>) -> Result<Catalog, String> {
        Ok(Catalog { client: Arc::new(DeliveryClient::new(index_url, store, cache)?), choice: IndexChoice { url: index_url.to_string(), source: IndexSource::Override } })
    }

    /// 読む一覧は characters の client が決める。本番の一覧だけ、読んだ一覧とファイルを残して通信できないときに使う（ローカル・指定の一覧の控えと混ぜない）。
    /// dir はディスクの控えの場所、override_url は指定の一覧（どちらもゲームが決める）。開発のビルドはローカル配布が起動していればそれ
    pub fn open_default(dir: PathBuf, override_url: Option<String>) -> Result<Catalog, String> {
        let choice = choose_index_url(cfg!(debug_assertions), override_url);
        if cfg!(debug_assertions) { eprintln!("{}", describe_index_source(&choice)); }
        let prod = choice.source == IndexSource::Production;
        let client = DeliveryClient::new(&choice.url, prod.then(|| dir.join("delivery")), prod.then(|| dir.join("files")))?;
        Ok(Catalog { client: Arc::new(client), choice })
    }

    /// 配布中の全主役（取れなかったキャラは外す）
    pub fn heroes(&self) -> Result<Vec<HeroEntry>, String> {
        let index = self.client.refresh()?;
        let ids: Vec<&String> = index["characters"].as_object().map(|o| o.keys().collect()).unwrap_or_default();
        let client = &self.client;
        let rows: Vec<Option<HeroEntry>> = std::thread::scope(|s| {
            let hs: Vec<_> = ids.iter().map(|id| s.spawn(move || {
                let m = client.manifest(id, None).ok()??;
                hero_entry(&m, &client.root)
            })).collect();
            hs.into_iter().map(|h| h.join().ok().flatten()).collect()
        });
        Ok(rows.into_iter().flatten().collect())
    }

    /// キャラ選び・HUD の絵のバイト（png / webp。SHA を照合済み）
    pub fn read_picture(&self, file: &DeliveryFile) -> Result<Vec<u8>, String> { self.client.read_file(file) }

    /// 主役を部品で読む（軽量版・武器の動作・のけぞり・必殺技・回避・ジャンプ。読むファイルは全部 SHA を照合済み）。
    /// gait は歩き・走りに切り替わる速さ（部品の空間）。GPU は触らない（別のスレッドで呼べる）
    pub fn hero(&self, id: &str, gait: Gait) -> Result<CharacterActor, String> {
        actor::load_character_actor(&self.client, LoadOptions { id: id.to_string(), revision: None, quality: actor::Quality::Lite, gait })
    }
}

// ---- ブラウザの版（vendor/character-delivery/client.mjs の createDeliveryClient と browserFileCache、src/character-delivery.ts の openBrowserCatalog の写し） ----
// 通信は async。取り終えたファイルはメモリの表（State）に持ち、主役の組み立ては Mac と同じ同期の load_character_actor がそこから読む。
#[cfg(target_arch = "wasm32")]
mod net {
    use js_sys::{Function, Promise, Reflect, Uint8Array};
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::JsFuture;

    const CACHE_NAME: &str = "aiarranger-character-delivery-v1";

    pub fn js_err(e: JsValue) -> String { e.as_string().unwrap_or_else(|| format!("{e:?}")) }

    /// AbortSignal.timeout（無いブラウザでは時間を切らない）
    fn timeout_signal(ms: u64) -> Option<JsValue> {
        let class = Reflect::get(&js_sys::global(), &"AbortSignal".into()).ok()?;
        let timeout: Function = Reflect::get(&class, &"timeout".into()).ok()?.dyn_into().ok()?;
        timeout.call1(&class, &JsValue::from_f64(ms as f64)).ok()
    }

    async fn bytes_of_raw(r: &web_sys::Response) -> Result<Vec<u8>, JsValue> {
        let ab = JsFuture::from(r.array_buffer()?).await?;
        Ok(Uint8Array::new(&ab).to_vec())
    }

    async fn bytes_of(r: &web_sys::Response) -> Result<Vec<u8>, String> { bytes_of_raw(r).await.map_err(js_err) }

    fn aborted(signal: Option<&JsValue>) -> bool {
        signal.is_some_and(|s| Reflect::get(s, &"aborted".into()).ok().and_then(|v| v.as_bool()) == Some(true))
    }

    fn retryable_network_error(error: &JsValue, signal: Option<&JsValue>) -> bool {
        if aborted(signal) { return false; }
        // fetch/本文受信の通信エラーは TypeError。AbortError・TimeoutError は再試行しない。
        // ブラウザは CORS なども同じ TypeError に隠すので、切断の詳細理由は区別できない。
        Reflect::get(error, &"name".into()).ok().and_then(|v| v.as_string()).as_deref() == Some("TypeError")
    }

    async fn wait_to_retry(win: &web_sys::Window, signal: Option<&JsValue>) -> Result<(), JsValue> {
        let pause = Promise::new(&mut |resolve, reject| {
            if let Err(e) = win.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 250) { let _ = reject.call1(&JsValue::NULL, &e); }
        });
        JsFuture::from(pause).await?;
        if aborted(signal) { return Err(Reflect::get(signal.unwrap(), &"reason".into()).unwrap_or_else(|_| "AbortError".into())); }
        Ok(())
    }

    /// characters の request（cache: 'no-store'、時間切れあり）。通信エラー（TypeError）は 250 ms 待って 1 回だけ取り直す
    pub async fn http_get(url: &str, timeout_ms: u64) -> Result<Vec<u8>, String> { http_get_with(url, timeout_ms, true).await }

    /// 取り直さない 1 回（ローカル配布があるかを探すとき。断られたらすぐ本番へ移る。取り直すと約 260 ms 遅れる）
    pub async fn http_get_once(url: &str, timeout_ms: u64) -> Result<Vec<u8>, String> { http_get_with(url, timeout_ms, false).await }

    async fn http_get_with(url: &str, timeout_ms: u64, retry: bool) -> Result<Vec<u8>, String> {
        let unavailable = |e: String| format!("Character release unavailable: {e}");
        let win = web_sys::window().ok_or("window が無い")?;
        let init = web_sys::RequestInit::new();
        Reflect::set(&init, &"cache".into(), &"no-store".into()).map_err(js_err)?;
        // 2 回で同じ signal を共有し、再試行で期限を延ばさない。中止済みの signal も再利用しない。
        let signal = timeout_signal(timeout_ms);
        if let Some(signal) = &signal { Reflect::set(&init, &"signal".into(), signal).map_err(js_err)?; }
        let mut retry = retry;
        loop {
            let got = match JsFuture::from(win.fetch_with_str_and_init(url, &init)).await {
                Ok(got) => got,
                Err(e) if retry && retryable_network_error(&e, signal.as_ref()) => {
                    retry = false;
                    wait_to_retry(&win, signal.as_ref()).await.map_err(|e| unavailable(js_err(e)))?;
                    continue;
                }
                Err(e) => return Err(unavailable(js_err(e))),
            };
            let resp: web_sys::Response = got.dyn_into().map_err(js_err)?;
            if !resp.ok() { return Err(unavailable(format!("HTTP {}", resp.status()))); }
            // 読む前の状態を保持。自分の arrayBuffer() が bodyUsed を立てた後では切断と区別できない。
            let readable = !resp.body_used() && Reflect::get(&resp, &"body".into()).ok().is_some_and(|body|
                body.is_null() || Reflect::get(&body, &"locked".into()).ok().and_then(|v| v.as_bool()) == Some(false));
            match bytes_of_raw(&resp).await {
                Err(e) if retry && readable && retryable_network_error(&e, signal.as_ref()) => {
                    retry = false;
                    wait_to_retry(&win, signal.as_ref()).await.map_err(|e| unavailable(js_err(e)))?;
                }
                result => return result.map_err(|e| unavailable(js_err(e))),
            }
        }
    }

    /// Cache API を開く。使えない環境（http の別マシンなど）では None で、控えなしで動く
    async fn open_cache() -> Option<web_sys::Cache> {
        let win = web_sys::window()?;
        let caches = Reflect::get(&win, &"caches".into()).ok()?;
        if caches.is_undefined() { return None; }
        let caches: web_sys::CacheStorage = caches.unchecked_into();
        JsFuture::from(caches.open(CACHE_NAME)).await.ok()?.dyn_into().ok()
    }
    pub async fn cache_get(url: &str) -> Option<Vec<u8>> {
        let hit = JsFuture::from(open_cache().await?.match_with_str(url)).await.ok()?;
        let resp: web_sys::Response = hit.dyn_into().ok()?;
        bytes_of(&resp).await.ok()
    }
    pub async fn cache_put(url: &str, bytes: &[u8]) {
        let Some(cache) = open_cache().await else { return };
        let Ok(resp) = web_sys::Response::new_with_opt_buffer_source(Some(&Uint8Array::from(bytes))) else { return };
        let _ = JsFuture::from(cache.put_with_str(url, &resp)).await;
    }
}

/// characters の createDeliveryClient（ブラウザ）。persist は読んだ一覧・版・ファイルを残すか（本番の一覧だけ）
#[cfg(target_arch = "wasm32")]
pub struct DeliveryClient {
    pub index_url: String,
    pub root: String,
    persist: bool,
    state: Mutex<State>,
}

#[cfg(target_arch = "wasm32")]
impl DeliveryClient {
    pub fn new(index_url: &str, persist: bool) -> Result<DeliveryClient, String> {
        Ok(DeliveryClient { root: release_root(index_url)?, index_url: index_url.to_string(), persist, state: Mutex::default() })
    }

    fn storage(&self) -> Option<web_sys::Storage> {
        if !self.persist { return None; }
        web_sys::window()?.local_storage().ok()?
    }
    fn stored_key(&self, suffix: &str) -> String { format!("character-delivery.v1:{}{suffix}", self.index_url) }
    fn get_stored(&self, suffix: &str) -> Option<Value> {
        serde_json::from_str(&self.storage()?.get_item(&self.stored_key(suffix)).ok()??).ok()
    }
    fn set_stored(&self, suffix: &str, v: &Value) {
        // 残せない環境でも、その場では読める
        if let Some(s) = self.storage() { let _ = s.set_item(&self.stored_key(suffix), &v.to_string()); }
    }

    /// 1 ファイルを読む。中身の SHA で決まるファイルは変わらないので、控えがあれば確かめて通信せずに使う
    pub async fn read_file_web(&self, file: &DeliveryFile) -> Result<Vec<u8>, String> {
        let ok = |b: &[u8]| b.len() as u64 == file.bytes && hash(b) == file.sha256;
        let url = file_url(&self.root, file);
        if self.persist && let Some(b) = net::cache_get(&url).await && ok(&b) { return Ok(b); }
        let b = net::http_get(&url, file_timeout_ms(file)).await?;
        if !ok(&b) { return Err("Character release integrity mismatch".into()); }
        if self.persist { net::cache_put(&url, &b).await; }
        Ok(b)
    }

    pub async fn refresh_web(&self) -> Result<Arc<Value>, String> {
        let got = net::http_get(&self.index_url, TIMEOUT_MS).await.and_then(|b| {
            if b.len() > 1024 * 1024 { return Err("Release index is too large".to_string()); }
            let v: Value = serde_json::from_slice(&b).map_err(|e| e.to_string())?;
            checked_index(v)
        });
        let mut st = self.state.lock().unwrap();
        match got {
            Ok(v) => {
                self.set_stored(":index", &v);
                st.index = Some(Arc::new(v));
            }
            Err(e) => {
                if st.index.is_none() {
                    match self.get_stored(":index") {
                        Some(v) => st.index = Some(Arc::new(checked_index(v)?)),
                        None => return Err(e),
                    }
                }
            }
        }
        Ok(st.index.clone().unwrap())
    }

    pub async fn manifest_web(&self, id: &str, revision: Option<&str>) -> Result<Option<Arc<Value>>, String> {
        identifier(id)?;
        let have = self.state.lock().unwrap().index.clone();
        let index = match have { Some(i) => i, None => self.refresh_web().await? };
        let Some(entry) = sget(&index, &["characters", id]) else { return Ok(None) };
        let Some(selected) = revision.or_else(|| entry.get("current").and_then(Value::as_str)) else { return Ok(None) };
        let Some(file) = sget(entry, &["releases", selected]) else { return Ok(None) };
        let bytes = self.read_file_web(&checked_file(file)?).await?;
        let v: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        Ok(Some(Arc::new(checked_manifest(v, id, selected)?)))
    }

    /// 1 人分を全部そろえてから返す（全部の SHA を確かめてから使う）。取れなければ前に完全に取れた版
    pub async fn prepare_web(&self, id: &str, revision: Option<&str>, files: &[String], fallback: bool) -> Result<Option<Arc<Prepared>>, String> {
        match self.prepare_inner_web(id, revision, files).await {
            Ok(v) => Ok(v),
            Err(error) => {
                if !fallback || revision.is_some() { return Err(error); }
                let before = self.state.lock().unwrap().previous.get(id).cloned();
                if let Some(before) = before && files.iter().all(|k| before.data.contains_key(k)) { return Ok(Some(before)); }
                if let Some(stored) = self.get_stored(&format!(":complete:{id}")) {
                    let rev = sget(&stored, &["manifest", "revision"]).and_then(Value::as_str).unwrap_or("").to_string();
                    let old = Arc::new(checked_manifest(stored["manifest"].clone(), id, &rev)?);
                    let mut data = HashMap::new();
                    for name in files {
                        let f = manifest_file(&old, name).ok_or_else(|| format!("Required character file is missing: {name}"))?;
                        data.insert(name.clone(), Arc::new(self.read_file_web(&f).await?));
                    }
                    let value = Arc::new(Prepared { manifest: old, data, root: self.root.clone() });
                    self.state.lock().unwrap().previous.insert(id.to_string(), value.clone());
                    return Ok(Some(value));
                }
                Err(error)
            }
        }
    }

    async fn prepare_inner_web(&self, id: &str, revision: Option<&str>, files: &[String]) -> Result<Option<Arc<Prepared>>, String> {
        let Some(selected) = self.manifest_web(id, revision).await? else { return Ok(None) };
        let rev = selected.get("revision").and_then(Value::as_str).unwrap_or("").to_string();
        let mut wanted: Vec<String> = Vec::new();
        for f in files { if !wanted.contains(f) { wanted.push(f.clone()); } }
        let mut sorted = wanted.clone();
        sorted.sort();
        let key = format!("{id}:{rev}:{}", sorted.join(","));
        if let Some(v) = self.state.lock().unwrap().prepared.get(&key).cloned() { return Ok(Some(v)); }
        let mut data = HashMap::new();
        for name in &wanted {
            let f = manifest_file(&selected, name).ok_or_else(|| format!("Required character file is missing: {name}"))?;
            data.insert(name.clone(), Arc::new(self.read_file_web(&f).await?));
        }
        let value = Arc::new(Prepared { manifest: selected.clone(), data, root: self.root.clone() });
        let mut st = self.state.lock().unwrap();
        st.prepared.insert(key, value.clone());
        st.previous.insert(id.to_string(), value.clone());
        drop(st);
        self.set_stored(&format!(":complete:{id}"), &json!({ "manifest": *selected, "files": wanted }));
        Ok(Some(value))
    }

    /// 同期の load_character_actor 用: 取り終えた 1 人分の版
    pub fn manifest(&self, id: &str, _revision: Option<&str>) -> Result<Option<Arc<Value>>, String> {
        Ok(self.state.lock().unwrap().loaded.get(id).map(|p| p.manifest.clone()))
    }
    /// 同期の load_character_actor 用: 取り終えた 1 人分のファイル
    pub fn prepare(&self, id: &str, _revision: Option<&str>, files: &[String], _fallback: bool) -> Result<Option<Arc<Prepared>>, String> {
        let Some(p) = self.state.lock().unwrap().loaded.get(id).cloned() else { return Ok(None) };
        match files.iter().find(|f| !p.data.contains_key(*f)) {
            Some(f) => Err(format!("Required character file is missing: {f}")),
            None => Ok(Some(p)),
        }
    }
}

/// characters の showIndexSource: 読んでいる一覧を隅に小さく出す（開発のときだけ）
#[cfg(target_arch = "wasm32")]
fn show_index_source(choice: &IndexChoice) {
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else { return };
    let Some(body) = doc.query_selector("body").ok().flatten() else { return };
    let label = match doc.get_element_by_id("character-index-source") {
        Some(e) => e,
        None => {
            let Ok(e) = doc.create_element("div") else { return };
            if body.append_child(&e).is_err() { return; }
            e
        }
    };
    label.set_id("character-index-source");
    label.set_text_content(Some(&describe_index_source(choice)));
    let color = match choice.source { IndexSource::Local => "#2a7d4f", IndexSource::Production => "#8a5a00", IndexSource::Override => "#555" };
    let _ = label.set_attribute("style", &format!("position:fixed;left:6px;bottom:6px;z-index:2147483647;font:11px/1.4 system-ui,sans-serif;padding:2px 6px;border-radius:4px;pointer-events:none;color:#fff;background:{color}"));
}

/// 読む一覧を決める（characters の chooseIndexUrl）
#[cfg(target_arch = "wasm32")]
pub async fn choose_index_url_web(development: bool, override_url: Option<String>) -> IndexChoice {
    if let Some(url) = override_url.filter(|u| !u.is_empty()) { return IndexChoice { url, source: IndexSource::Override }; }
    // 探す 1 回は取り直さない（ローカル配布が起動していないと断られる。取り直すと 250 ms 待つ）
    if development && net::http_get_once(LOCAL_INDEX_URL, 800).await.is_ok() { return IndexChoice { url: LOCAL_INDEX_URL.into(), source: IndexSource::Local }; }
    IndexChoice { url: PRODUCTION_INDEX_URL.into(), source: IndexSource::Production }
}

/// ページの URL の引数（?auto&seed=1 など）
#[cfg(target_arch = "wasm32")]
pub fn page_query() -> Vec<(String, String)> {
    let s = web_sys::window().and_then(|w| w.location().search().ok()).unwrap_or_default();
    s.trim_start_matches('?').split('&').filter(|p| !p.is_empty()).map(|p| match p.split_once('=') {
        Some((k, v)) => (k.to_string(), v.to_string()),
        None => (p.to_string(), String::new()),
    }).collect()
}

#[cfg(target_arch = "wasm32")]
impl Catalog {
    /// src/character-delivery.ts の openBrowserCatalog。本番のビルドは本番の一覧だけ。開発のビルドは ?catalog=URL、ローカル配布、本番の順。
    /// 本番の一覧だけ、読んだ一覧とファイルを残して通信できないときに使う（ローカル・指定の一覧の控えと混ぜない）
    pub async fn open_default_web() -> Result<Catalog, String> { Catalog::open_web(cfg!(debug_assertions)).await }

    /// open_default_web の開発かどうかをゲームが決める形（ゲームは release で配信して確かめるので、配信の決まりで判定する）
    pub async fn open_web(development: bool) -> Result<Catalog, String> {
        let over = if development {
            page_query().into_iter().find(|(k, _)| k == "catalog").and_then(|(_, v)| js_sys::decode_uri_component(&v).ok()).map(String::from)
        } else { None };
        let choice = choose_index_url_web(development, over).await;
        if development { show_index_source(&choice); }
        let client = DeliveryClient::new(&choice.url, choice.source == IndexSource::Production)?;
        Ok(Catalog { client: Arc::new(client), choice })
    }

    /// 一覧の URL をゲームが決める形（サーバーが一覧の URL を決める Engimeow・予備の一式）。persist は読んだ一覧とファイルを残すか
    pub fn open_index_web(url: &str, persist: bool) -> Result<Catalog, String> {
        let client = DeliveryClient::new(url, persist)?;
        Ok(Catalog { client: Arc::new(client), choice: IndexChoice { url: url.to_string(), source: if persist { IndexSource::Production } else { IndexSource::Local } } })
    }

    /// 配布中の全主役（取れなかったキャラは外す）
    pub async fn heroes_web(&self) -> Result<Vec<HeroEntry>, String> {
        use std::{cell::RefCell, rc::Rc};
        let index = self.client.refresh_web().await?;
        let ids: Vec<String> = index["characters"].as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default();
        let rows: Rc<RefCell<Vec<Option<HeroEntry>>>> = Rc::new(RefCell::new(vec![None; ids.len()]));
        // TS の Promise.all と同じく並べて取る
        let waits = js_sys::Array::new();
        for (i, id) in ids.into_iter().enumerate() {
            let (client, rows) = (self.client.clone(), rows.clone());
            waits.push(&wasm_bindgen_futures::future_to_promise(async move {
                let row = client.manifest_web(&id, None).await.ok().flatten().and_then(|m| hero_entry(&m, &client.root));
                rows.borrow_mut()[i] = row;
                Ok(wasm_bindgen::JsValue::UNDEFINED)
            }));
        }
        wasm_bindgen_futures::JsFuture::from(js_sys::Promise::all(&waits)).await.map_err(net::js_err)?;
        Ok(rows.take().into_iter().flatten().collect())
    }

    /// キャラ選び・HUD の絵のバイト（png / webp。SHA を照合済み）
    pub async fn read_picture_web(&self, file: &DeliveryFile) -> Result<Vec<u8>, String> { self.client.read_file_web(file).await }

    /// hero_web の版と品質をゲームが決める形（保存した周の版。読めなければ呼ぶ側が予備の一式へ）
    pub async fn hero_web_with(&self, o: LoadOptions) -> Result<CharacterActor, String> {
        let client = &self.client;
        let id = o.id.clone();
        let not = || format!("{id} は配布されていません");
        let listed = client.manifest_web(&id, o.revision.as_deref()).await?.ok_or_else(not)?;
        let plan = actor::release_plan(&listed, o.quality);
        let mut prepared = client.prepare_web(&id, o.revision.as_deref(), &plan.files, o.revision.is_none()).await?.ok_or_else(not)?;
        let plan = actor::release_plan(&prepared.manifest, o.quality);
        if plan.files.iter().any(|f| !prepared.data.contains_key(f)) {
            let rev = prepared.manifest["revision"].as_str().map(String::from);
            prepared = client.prepare_web(&id, rev.as_deref(), &plan.files, o.revision.is_none()).await?.ok_or_else(not)?;
        }
        client.state.lock().unwrap().loaded.insert(id.clone(), prepared);
        actor::load_character_actor(client, LoadOptions { revision: None, ..o })
    }

    /// 主役を部品で読む（軽量版・武器の動作・のけぞり・必殺技・回避・ジャンプ）。load_character_actor と同じ手順で要るファイルを決めて SHA を確かめながら取り、
    /// メモリの表に置いてから同期の読み込みで組み立てる。GPU は触らない
    pub async fn hero_web(&self, id: &str, gait: Gait) -> Result<CharacterActor, String> {
        let client = &self.client;
        let not = || format!("{id} は配布されていません");
        let listed = client.manifest_web(id, None).await?.ok_or_else(not)?;
        let plan = actor::release_plan(&listed, actor::Quality::Lite);
        let mut prepared = client.prepare_web(id, None, &plan.files, true).await?.ok_or_else(not)?;
        let plan = actor::release_plan(&prepared.manifest, actor::Quality::Lite);
        if plan.files.iter().any(|f| !prepared.data.contains_key(f)) {
            let rev = prepared.manifest["revision"].as_str().map(String::from);
            prepared = client.prepare_web(id, rev.as_deref(), &plan.files, true).await?.ok_or_else(not)?;
        }
        client.state.lock().unwrap().loaded.insert(id.to_string(), prepared);
        actor::load_character_actor(client, LoadOptions { id: id.to_string(), revision: None, quality: actor::Quality::Lite, gait })
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn root_and_urls() {
        assert_eq!(release_root(PRODUCTION_INDEX_URL).unwrap(), "https://characters.aiarranger.jp/");
        assert_eq!(release_root(LOCAL_INDEX_URL).unwrap(), "http://127.0.0.1:4259/");
        assert_eq!(release_root("https://a.b/x/channels/stable/v1/index.json").unwrap(), "https://a.b/x/");
        assert!(release_root("https://user:p@a.b/channels/stable/v1/index.json").is_err());
        assert!(release_root("ftp://a.b/channels/stable/v1/index.json").is_err());
        assert!(release_root("https://a.b/channels/stable/v2/index.json").is_err());
        let f = checked_file(&json!({ "path": "assets/aa.glb", "sha256": "0".repeat(64), "bytes": 3 })).unwrap();
        assert_eq!(file_url("https://a.b/", &f), format!("https://a.b/assets/aa.glb?v={}", "0".repeat(64)));
        assert!(checked_file(&json!({ "path": "assets/../aa.glb", "sha256": "0".repeat(64), "bytes": 3 })).is_err());
        assert!(checked_file(&json!({ "path": "other/aa.glb", "sha256": "0".repeat(64), "bytes": 3 })).is_err());
        assert!(identifier("Aichan").is_err());
        assert!(identifier("master-nagi").is_ok());
    }

    #[test]
    fn sha_matches_node() {
        // node -e "crypto.createHash('sha256').update('abc').digest('hex')"
        assert_eq!(hash(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn corrupt_cache_is_replaced_and_offline_uses_cache() {
        let dir = std::env::temp_dir().join(format!("character-delivery-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let body = b"hello".to_vec();
        let f = DeliveryFile { path: "assets/x.bin".into(), sha256: hash(&body), bytes: 5 };
        // 通信できない宛先 + 正しい控え: 控えで読める
        std::fs::write(dir.join(&f.sha256), &body).unwrap();
        let c = DeliveryClient::new("http://127.0.0.1:9/channels/stable/v1/index.json", None, Some(dir.clone())).unwrap();
        assert_eq!(c.read_file(&f).unwrap(), body);
        // 壊れた控えは使わない
        std::fs::write(dir.join(&f.sha256), b"xxxxx").unwrap();
        assert!(c.read_file(&f).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
