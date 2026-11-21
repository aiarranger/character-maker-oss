// DOM の小さな補助。Closure は forget（ページが生きている間ずっと使う）。
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{AddEventListenerOptions, Document, Element, Event, EventTarget, HtmlElement, Window};

pub fn window() -> Window { web_sys::window().expect("window") }
pub fn document() -> Document { window().document().expect("document") }
pub fn body() -> HtmlElement { document().body().expect("body") }

/// TS の `$(id)`。無ければ panic（index.html と食い違っている）
pub fn el(id: &str) -> HtmlElement { try_el(id).unwrap_or_else(|| panic!("#{id} が無い")) }
pub fn try_el(id: &str) -> Option<HtmlElement> { document().get_element_by_id(id)?.dyn_into().ok() }

/// 同じなら触らない（文字を書き換えるだけで再レイアウトになるので）
pub fn set_text(e: &HtmlElement, s: &str) { if e.text_content().as_deref() != Some(s) { e.set_text_content(Some(s)); } }
pub fn set_html(e: &HtmlElement, s: &str) { if e.inner_html() != s { e.set_inner_html(s); } }
pub fn class_toggle(e: &HtmlElement, name: &str, on: bool) { let _ = e.class_list().toggle_with_force(name, on); }
pub fn class_has(e: &HtmlElement, name: &str) -> bool { e.class_list().contains(name) }
/// style の値。`--k` のような変数もこれで入れる
pub fn set_style(e: &HtmlElement, prop: &str, value: &str) { let _ = e.style().set_property(prop, value); }
pub fn attr(e: &Element, name: &str) -> Option<String> { e.get_attribute(name) }
pub fn set_attr(e: &Element, name: &str, value: &str) { let _ = e.set_attribute(name, value); }
pub fn remove_attr(e: &Element, name: &str) { let _ = e.remove_attribute(name); }

/// document.createElement（class は空なら付けない）
pub fn create(tag: &str, class: &str) -> HtmlElement {
    let e: HtmlElement = document().create_element(tag).expect("createElement").unchecked_into();
    if !class.is_empty() { e.set_class_name(class); }
    e
}
pub fn append(parent: &HtmlElement, child: &HtmlElement) { let _ = parent.append_child(child); }
pub fn remove(e: &HtmlElement) { e.remove(); }

fn to_html(e: Element) -> Option<HtmlElement> { e.dyn_into().ok() }
pub fn query(selector: &str) -> Option<HtmlElement> { to_html(document().query_selector(selector).ok()??) }
pub fn query_in(root: &HtmlElement, selector: &str) -> Option<HtmlElement> { to_html(root.query_selector(selector).ok()??) }
fn nodes(list: Result<web_sys::NodeList, wasm_bindgen::JsValue>) -> Vec<HtmlElement> {
    let Ok(list) = list else { return Vec::new() };
    (0..list.length()).filter_map(|i| list.item(i)?.dyn_into().ok()).collect()
}
pub fn query_all(selector: &str) -> Vec<HtmlElement> { nodes(document().query_selector_all(selector)) }
pub fn query_all_in(root: &HtmlElement, selector: &str) -> Vec<HtmlElement> { nodes(root.query_selector_all(selector)) }
pub fn closest(e: &Element, selector: &str) -> Option<HtmlElement> { to_html(e.closest(selector).ok()??) }

pub fn scroll_into_view(e: &HtmlElement) { e.scroll_into_view(); }
pub fn focus(e: &HtmlElement) { let _ = e.focus(); }

/// addEventListener（E は KeyboardEvent など）。TS の第 3 引数は passive・once
pub fn listen<E: JsCast + 'static>(target: &EventTarget, ty: &str, passive: Option<bool>, once: bool, mut f: impl FnMut(E) + 'static) {
    let c = Closure::<dyn FnMut(Event)>::new(move |e: Event| f(e.unchecked_into()));
    let opts = AddEventListenerOptions::new();
    if let Some(p) = passive { opts.set_passive(p); }
    if once { opts.set_once(true); }
    let _ = target.add_event_listener_with_callback_and_add_event_listener_options(ty, c.as_ref().unchecked_ref(), &opts);
    c.forget();
}

/// `$(id).addEventListener(ty, f)`
pub fn on(id: &str, ty: &str, f: impl FnMut(Event) + 'static) { listen::<Event>(&el(id), ty, None, false, f); }
/// 要素へ直接
pub fn on_el(e: &HtmlElement, ty: &str, f: impl FnMut(Event) + 'static) { listen::<Event>(e, ty, None, false, f); }
