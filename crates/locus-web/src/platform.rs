//! Browser capabilities behind `locus_effects::Platform`, plus the clock and
//! the origin the client talks to.

use locus_client::Client;
use locus_core::Now;
use locus_effects::{BlobHint, PickedFile, Platform};
use url::Url;
use wasm_bindgen_futures::JsFuture;

/// Zero-sized: copied into each effect future.
#[derive(Debug, Clone, Copy)]
pub struct WebPlatform;

impl Platform for WebPlatform {
    async fn sleep(&self, ms: u64) {
        sleep_ms(ms.min(u64::from(u32::MAX)) as u32).await;
    }

    async fn pick_files(&self) -> Vec<PickedFile> {
        let Some(handles) = rfd::AsyncFileDialog::new()
            .set_title("Upload files")
            .pick_files()
            .await
        else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity(handles.len());
        for handle in handles {
            let name = handle.file_name();
            let bytes = handle.read().await;
            let mime = mime_for(&name).to_string();
            out.push(PickedFile { name, mime, bytes });
        }
        out
    }

    async fn open_blob(&self, url: Url, _hint: Option<BlobHint>) -> Result<(), String> {
        let window = web_sys::window().ok_or("no browser window")?;
        window
            .open_with_url_and_target(url.as_str(), "_blank")
            .map(|_| ())
            .map_err(|e| e.as_string().unwrap_or_else(|| "the browser blocked the tab".into()))
    }
}

/// The page's route: `/app/<section>` locks the page to that section, and
/// `#<target>` (e.g. `#note:12`) is a cross-link to open.
pub fn route() -> (Option<locus_core::Section>, Option<locus_core::Target>) {
    let Some(location) = web_sys::window().map(|w| w.location()) else {
        return (None, None);
    };
    let path = location.pathname().unwrap_or_default();
    let section = path
        .trim_end_matches('/')
        .rsplit_once("/app/")
        .and_then(|(_, rest)| locus_core::Section::from_slug(rest));
    (section, hash_target())
}

/// The cross-link in the URL hash, if any.
pub fn hash_target() -> Option<locus_core::Target> {
    let hash = web_sys::window()?.location().hash().ok()?;
    locus_core::Target::parse(hash.trim_start_matches('#'))
}

/// Drop the hash once its target is handled, so the same link sent again
/// is a change the page hears about.
pub fn clear_hash() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let path = window.location().pathname().unwrap_or_default();
    if let Ok(history) = window.history() {
        let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&path));
    }
}

/// The tab name a section's page goes by, so `window.open` finds it again.
fn tab_name(section: locus_core::Section) -> String {
    format!("locus-{}", section.slug())
}

/// Name this tab after its section and set the page title.
pub fn claim_tab(section: Option<locus_core::Section>, title: &str) {
    let Some(window) = web_sys::window() else {
        return;
    };
    // An all-sections page drops a name left by a section page in this tab.
    let _ = window.set_name(&section.map(tab_name).unwrap_or_default());
    if let Some(doc) = window.document() {
        doc.set_title(title);
    }
}

/// Open or focus the tab for `section` (`/app/<section>#<target>`). The
/// named target reuses the section's tab when the browser can find it.
pub fn open_section(section: locus_core::Section, target: Option<locus_core::Target>) {
    let Some(window) = web_sys::window() else {
        return;
    };
    // index.html sets <base href="/app/">; section pages live right below it.
    let base = window
        .document()
        .and_then(|d| d.base_uri().ok().flatten())
        .unwrap_or_else(|| "/app/".into());
    let mut url = format!("{base}{}", section.slug());
    if let Some(target) = target {
        url.push('#');
        url.push_str(&target.to_string());
    }
    match window.open_with_url_and_target(&url, &tab_name(section)) {
        Ok(Some(tab)) => {
            let _ = tab.focus();
        }
        _ => web_sys::console::warn_1(&"locus: the browser blocked opening a tab".into()),
    }
}

/// Every new URL hash, as it changes (another page sent a cross-link).
pub fn hash_changes() -> impl iced::futures::Stream<Item = String> {
    iced::stream::channel(8, async |sender: iced::futures::channel::mpsc::Sender<String>| {
        use wasm_bindgen::JsCast;
        use wasm_bindgen::closure::Closure;
        let Some(window) = web_sys::window() else {
            return;
        };
        let mut sender = sender;
        let listener = Closure::<dyn FnMut()>::new(move || {
            let hash = web_sys::window()
                .and_then(|w| w.location().hash().ok())
                .unwrap_or_default();
            let _ = sender.try_send(hash);
        });
        let _ = window
            .add_event_listener_with_callback("hashchange", listener.as_ref().unchecked_ref());
        // Lives as long as the page.
        listener.forget();
        std::future::pending::<()>().await;
    })
}

/// One-shot timer on the JS event loop.
async fn sleep_ms(ms: u32) {
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        if let Some(window) = web_sys::window() {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                &resolve,
                ms.min(i32::MAX as u32) as i32,
            );
        }
    });
    let _ = JsFuture::from(promise).await;
}

/// The clock for every `update`, from the browser: unix seconds and the
/// local offset (`local = utc + offset`).
pub fn clock() -> Now {
    let unix = (js_sys::Date::now() / 1000.0).floor() as i64;
    // getTimezoneOffset is minutes west of UTC, so the sign flips.
    let west_min = js_sys::Date::new_0().get_timezone_offset();
    let offset = -(west_min as i32) * 60;
    locus_effects::now(unix, offset)
}

/// The page's origin. The app is served by the gateway at `/app/`, which
/// proxies the ship API on the same origin, so the session cookie applies.
pub fn origin() -> Url {
    web_sys::window()
        .and_then(|w| w.location().origin().ok())
        .and_then(|s| Url::parse(&s).ok())
        .unwrap_or_else(|| Url::parse("http://127.0.0.1:8080").expect("static URL"))
}

/// `true` when the browser asks for a light colour scheme.
pub fn prefers_light() -> bool {
    web_sys::window()
        .and_then(|w| w.match_media("(prefers-color-scheme: light)").ok().flatten())
        .map(|mql| mql.matches())
        .unwrap_or(false)
}

pub fn client() -> Client {
    Client::new(origin()).expect("page origin is a valid base URL")
}

/// Mime type from a file name's extension. Unknown types are octet-stream.
pub fn mime_for(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "heic" => "image/heic",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "pdf" => "application/pdf",
        "txt" | "log" => "text/plain",
        "md" => "text/markdown",
        "csv" => "text/csv",
        "json" => "application/json",
        "html" | "htm" => "text/html",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
}


thread_local! {
    /// Modifier keys held during the latest DOM keydown.
    static KEY_MODS: std::cell::Cell<Option<locus_core::Mods>> = const { std::cell::Cell::new(None) };
}

/// Record the modifier state of every keydown, before winit sees it.
///
/// winit's web backend reports a key press before the modifier change that
/// came with it, so iced's modifiers lag by one event whenever they changed
/// without a key event of their own (Alt released while the tab was in the
/// background, synthetic input). The DOM event itself is always right.
pub fn track_key_modifiers() {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;
    let Some(window) = web_sys::window() else {
        return;
    };
    let listener = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(|e: web_sys::KeyboardEvent| {
        KEY_MODS.with(|m| {
            m.set(Some(locus_core::Mods {
                ctrl: e.ctrl_key(),
                alt: e.alt_key(),
                shift: e.shift_key(),
                logo: e.meta_key(),
            }))
        });
    });
    let options = web_sys::AddEventListenerOptions::new();
    options.set_capture(true);
    let _ = window.add_event_listener_with_callback_and_add_event_listener_options(
        "keydown",
        listener.as_ref().unchecked_ref(),
        &options,
    );
    // Lives as long as the page.
    listener.forget();
}

/// Modifiers of the latest keydown, if one has been seen.
pub fn key_modifiers() -> Option<locus_core::Mods> {
    KEY_MODS.with(std::cell::Cell::get)
}
