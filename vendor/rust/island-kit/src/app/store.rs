// localStorage の読み書き。保存できない環境でも遊びは止めない（失敗は無視。TS の try / catch と同じ）。
use serde_json::Value;

fn storage() -> Option<web_sys::Storage> { web_sys::window()?.local_storage().ok()? }

pub fn get(key: &str) -> Option<String> { storage()?.get_item(key).ok()? }
pub fn set(key: &str, value: &str) { if let Some(s) = storage() { let _ = s.set_item(key, value); } }
pub fn remove(key: &str) { if let Some(s) = storage() { let _ = s.remove_item(key); } }

/// 読めない・壊れているときは None
pub fn get_json(key: &str) -> Option<Value> { serde_json::from_str(&get(key)?).ok() }
pub fn set_json(key: &str, value: &Value) { set(key, &value.to_string()); }
