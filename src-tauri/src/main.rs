// TRC — переводчик EN<->RU: главное окно + панель поверх игр.
// Портативные настройки: <exe_dir>/data/settings.json (т.е. F:\TRC\data),
// fallback — %APPDATA%\com.trc.translator\settings.json
// Перевод — только онлайн без ключей (Яндекс, Bing, Google, MyMemory, Libre).

// Убирает консольное окно в релизе (в debug остаётся для логов).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex, OnceLock,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, WindowEvent,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    #[serde(default = "d_true")]
    enabled: bool,
    #[serde(default = "d_engine")]
    engine: String,
    #[serde(default = "d_src")]
    source: String,
    #[serde(default = "d_dst")]
    target: String,
    #[serde(default = "d_true")]
    live: bool,
    #[serde(default = "d_oh")]
    overlay_hotkey: String,
    #[serde(default = "d_th")]
    translate_hotkey: String,
    #[serde(default = "d_cap")]
    capture_hotkey: String,
    #[serde(default = "d_libre")]
    libre_url: String,
}
fn d_true() -> bool {
    true
}
fn d_engine() -> String {
    "yandex".into()
}
fn d_src() -> String {
    "en".into()
}
fn d_dst() -> String {
    "ru".into()
}
fn d_oh() -> String {
    "F8".into()
}
fn d_th() -> String {
    "Ctrl+Alt+T".into()
}
fn d_cap() -> String {
    "F9".into()
}
fn d_libre() -> String {
    "https://libretranslate.de".into()
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: true,
            engine: "yandex".into(),
            source: "en".into(),
            target: "ru".into(),
            live: true,
            overlay_hotkey: "F8".into(),
            translate_hotkey: "Ctrl+Alt+T".into(),
            capture_hotkey: "F9".into(),
            libre_url: "https://libretranslate.de".into(),
        }
    }
}

struct AppState {
    settings_path: PathBuf,
    settings: Mutex<Settings>,
    // Кэш параметров Bing (IG/key/token), живут ~30 минут
    bing: Mutex<Option<(Instant, String, String, String)>>,
    // Кадр для выбора области: снимаем ДО показа окна, чтобы в кадр
    // не попали наши же окна и чтобы игра не успела свернуться.
    pending: Mutex<Option<PendingCapture>>,
    // Время последнего падения движка → кулдаун, чтобы не долбить лежачий.
    cooldowns: Mutex<HashMap<String, Instant>>,
}

// Упавший движок отдыхает 10 минут, потом тихо перепроверяем.
const COOLDOWN_SECS: u64 = 600;

fn engine_in_cooldown(state: &AppState, e: &str) -> bool {
    if let Ok(g) = state.cooldowns.lock() {
        if let Some(t) = g.get(e) {
            if t.elapsed().as_secs() < COOLDOWN_SECS {
                return true;
            }
        }
    }
    false
}

fn mark_engine_ok(state: &AppState, e: &str) {
    if let Ok(mut g) = state.cooldowns.lock() {
        g.remove(e);
    }
}

fn mark_engine_fail(state: &AppState, e: &str) {
    if let Ok(mut g) = state.cooldowns.lock() {
        g.insert(e.to_string(), Instant::now());
    }
}

struct PendingCapture {
    png: Vec<u8>,
    w: u32,
    h: u32,
    main_visible: bool,
    panel_visible: bool,
}

fn win_visible(app: &tauri::AppHandle, label: &str) -> bool {
    app.get_webview_window(label)
        .map(|w| w.is_visible().unwrap_or(false))
        .unwrap_or(false)
}

fn portable_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let data = dir.join("data");
    let dir_str = dir.to_string_lossy().to_lowercase();
    if data.is_dir() || dir_str.ends_with("f:\\trc") || dir_str.ends_with("f:/trc") {
        let _ = fs::create_dir_all(&data);
        return Some(data.join("settings.json"));
    }
    None
}

fn settings_path(app: &tauri::AppHandle) -> PathBuf {
    if let Some(p) = portable_path() {
        return p;
    }
    if PathBuf::from("F:\\TRC").is_dir() {
        let _ = fs::create_dir_all("F:\\TRC\\data");
        return PathBuf::from("F:\\TRC\\data\\settings.json");
    }
    app.path()
        .app_data_dir()
        .map(|d| d.join("settings.json"))
        .unwrap_or_else(|_| PathBuf::from("settings.json"))
}

fn load_settings(path: &PathBuf) -> Settings {
    fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn persist(state: &AppState) {
    if let Some(parent) = state.settings_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(s) = state.settings.lock() {
        let _ = fs::write(
            &state.settings_path,
            serde_json::to_string_pretty(&*s).unwrap_or_default(),
        );
    }
}

// ---------- перевод (только онлайн, без ключей) ----------

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";
const YANDEX_UA: &str = "ru.yandex.translate/3.20.2024";

static HTTP: OnceLock<reqwest::Client> = OnceLock::new();
fn http() -> &'static reqwest::Client {
    HTTP.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(8))
            .user_agent(UA)
            .build()
            .expect("http client")
    })
}

// --- Яндекс (бесплатный endpoint мобильного API, без ключа) ---
static YX_CTR: AtomicU64 = AtomicU64::new(0);
fn yandex_sid() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let c = YX_CTR.fetch_add(1, Ordering::Relaxed) as u128;
    let pid = std::process::id() as u128;
    let v = nanos ^ (c << 64) ^ (pid << 96);
    format!("{v:032x}-0-0")
}

async fn tr_yandex_once(text: &str, src: &str, dst: &str) -> Result<String, String> {
    let sid = yandex_sid();
    let url =
        format!("https://translate.yandex.net/api/v1/tr.json/translate?id={sid}&srv=android");
    let r: serde_json::Value = http()
        .post(&url)
        .header("User-Agent", YANDEX_UA)
        .form(&[
            ("source_lang", src),
            ("target_lang", dst),
            ("text", text),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let code = r.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
    if code != 200 {
        return Err(format!("yandex {code}"));
    }
    r.pointer("/text/0")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "bad yandex response".to_string())
}

async fn tr_yandex(text: &str, src: &str, dst: &str) -> Result<String, String> {
    // 405 = протухшая сессия: одна повторная попытка с новой сессией чинит.
    match tr_yandex_once(text, src, dst).await {
        Err(e) if e.starts_with("yandex 405") => tr_yandex_once(text, src, dst).await,
        r => r,
    }
}

// --- Microsoft Bing (неофициальный web-flow, без ключа) ---
fn parse_bing_params(html: &str) -> Option<(String, String, String)> {
    let ig_key = "IG:\"";
    let i = html.find(ig_key)?;
    let rest = html.get(i + ig_key.len()..)?;
    let ig = rest.get(..32)?.to_string();
    if !ig.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let pk = "params_AbusePreventionHelper = [";
    let j = html.find(pk)?;
    let rest = html.get(j + pk.len()..)?;
    let comma = rest.find(',')?;
    let key = rest.get(..comma)?.trim().to_string();
    if key.is_empty() || !key.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let after = rest.get(comma + 1..)?;
    let q1 = after.find('"')?;
    let after = after.get(q1 + 1..)?;
    let q2 = after.find('"')?;
    let token = after.get(..q2)?.to_string();
    if token.is_empty() {
        return None;
    }
    Some((ig, key, token))
}

async fn bing_params(state: &AppState) -> Result<(String, String, String), String> {
    if let Ok(g) = state.bing.lock() {
        if let Some((at, ig, key, token)) = g.as_ref() {
            if at.elapsed().as_secs() < 1800 {
                return Ok((ig.clone(), key.clone(), token.clone()));
            }
        }
    }
    let html = http()
        .get("https://www.bing.com/translator")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    let (ig, key, token) = parse_bing_params(&html).ok_or("bing: no params on page")?;
    if let Ok(mut g) = state.bing.lock() {
        *g = Some((Instant::now(), ig.clone(), key.clone(), token.clone()));
    }
    Ok((ig, key, token))
}

async fn tr_bing(state: &AppState, text: &str, src: &str, dst: &str) -> Result<String, String> {
    let (ig, key, token) = bing_params(state).await?;
    let url = format!("https://www.bing.com/ttranslatev3?isVertical=1&IG={ig}&IID=translator.5027");
    let r: serde_json::Value = http()
        .post(&url)
        .form(&[
            ("fromLang", src),
            ("to", dst),
            ("text", text),
            ("token", token.as_str()),
            ("key", key.as_str()),
            ("tryFetchingGenderDebiasedTranslations", "true"),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    if r.get("statusCode").is_some() {
        // токен протух — сбрасываем кэш, следующий вызов возьмёт свежий
        if let Ok(mut g) = state.bing.lock() {
            *g = None;
        }
        return Err("bing: bad token".into());
    }
    r.pointer("/0/translations/0/text")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "bad bing response".to_string())
}

// --- Google (free endpoint, без ключа; иногда упирается в лимит IP) ---
async fn tr_google(text: &str, src: &str, dst: &str) -> Result<String, String> {
    let url = "https://translate.googleapis.com/translate_a/single";
    let r: serde_json::Value = http()
        .get(url)
        .query(&[
            ("client", "gtx"),
            ("sl", src),
            ("tl", dst),
            ("dt", "t"),
            ("q", text),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let mut s = String::new();
    if let Some(arr) = r.get(0).and_then(|v| v.as_array()) {
        for seg in arr {
            if let Some(t) = seg.get(0).and_then(|v| v.as_str()) {
                s.push_str(t);
            }
        }
    }
    if s.is_empty() {
        Err("bad google response".into())
    } else {
        Ok(s)
    }
}

// --- MyMemory (бесплатно с лимитом) ---
async fn tr_mymemory(text: &str, src: &str, dst: &str) -> Result<String, String> {
    let pair = format!("{src}|{dst}");
    let r: serde_json::Value = http()
        .get("https://api.mymemory.translated.net/get")
        .query(&[("q", text), ("langpair", &pair)])
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    // при исчерпанной квоте отвечает INVALID EMAIL / MYMEMORY WARNING — считаем ошибкой
    let status = r
        .pointer("/responseStatus")
        .and_then(|v| v.as_i64())
        .unwrap_or(200);
    if status == 429 || status == 403 {
        return Err(format!("mymemory quota ({status})"));
    }
    let t = r
        .pointer("/responseData/translatedText")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "bad mymemory response".to_string())?;
    if t.contains("MYMEMORY WARNING") || t.contains("INVALID EMAIL") {
        return Err("mymemory quota".into());
    }
    Ok(t)
}

// --- LibreTranslate (свой/публичный сервер) ---
async fn tr_libre(text: &str, libre_url: &str, src: &str, dst: &str) -> Result<String, String> {
    let base = libre_url.trim_end_matches('/');
    if base.is_empty() {
        return Err("libre: empty url".into());
    }
    let r: serde_json::Value = http()
        .post(format!("{base}/translate"))
        .json(&serde_json::json!({"q": text, "source": src, "target": dst, "format": "text"}))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    r.pointer("/translatedText")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "bad libre response".to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn translate_text(
    state: tauri::State<'_, AppState>,
    text: String,
    engine: String,
    libre_url: String,
    source: String,
    target: String,
) -> Result<String, String> {
    let t = text.trim().to_string();
    if t.is_empty() {
        return Ok(String::new());
    }
    if source == target {
        return Ok(t);
    }
    // Выбранный движок первым, дальше — остальные как страховка.
    // Поэтому «не работает» одного сервиса больше не ломает перевод.
    let mut order: Vec<&str> = vec![engine.as_str()];
    for e in ["yandex", "bing", "google", "mymemory", "libre"] {
        if !order.contains(&e) {
            order.push(e);
        }
    }
    let mut last_err = String::from("no engines");
    for e in order {
        let r = match e {
            "yandex" => tr_yandex(&t, &source, &target).await,
            "bing" => tr_bing(&state, &t, &source, &target).await,
            "google" => tr_google(&t, &source, &target).await,
            "mymemory" => tr_mymemory(&t, &source, &target).await,
            "libre" => tr_libre(&t, &libre_url, &source, &target).await,
            _ => tr_yandex(&t, &source, &target).await,
        };
        match r {
            Ok(s) if !s.trim().is_empty() => return Ok(s),
            Ok(_) => last_err = format!("{e}: empty"),
            Err(err) => last_err = format!("{e}: {err}"),
        }
    }
    Err(last_err)
}

// ---------- настройки/окна ----------

#[tauri::command]
fn get_settings(state: tauri::State<'_, AppState>) -> Settings {
    state.settings.lock().map(|s| s.clone()).unwrap_or_default()
}

#[tauri::command]
fn save_settings(state: tauri::State<'_, AppState>, app: tauri::AppHandle, settings: Settings) {
    if let Ok(mut s) = state.settings.lock() {
        *s = settings.clone();
    }
    persist(&state);
    let _ = app.emit("trc:settings-updated", &settings);
}

fn emit_settings(app: &tauri::AppHandle) {
    let cur = app
        .state::<AppState>()
        .settings
        .lock()
        .map(|s| s.clone())
        .unwrap_or_default();
    let _ = app.emit("trc:settings-updated", &cur);
}

#[tauri::command]
async fn prepare_selection(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    // 1. Запоминаем и прячем свои окна — иначе они попадут в кадр.
    let main_vis = win_visible(&app, "main");
    let panel_vis = win_visible(&app, "panel");
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
    if let Some(w) = app.get_webview_window("panel") {
        let _ = w.hide();
    }
    // 2. Пауза, чтобы композитор успел убрать окна с экрана.
    tokio::time::sleep(std::time::Duration::from_millis(220)).await;
    // 3. Снимаем чистый экран (игра ещё на месте).
    use xcap::Monitor;
    let mons = Monitor::all().map_err(|e| e.to_string())?;
    let mon = mons.into_iter().next().ok_or("no monitor")?;
    let img = mon.capture_image().map_err(|e| e.to_string())?;
    let (w, h) = (img.width(), img.height());
    let mut buf = Vec::new();
    {
        let mut cur = std::io::Cursor::new(&mut buf);
        img.write_to(&mut cur, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
    }
    if let Ok(mut p) = state.pending.lock() {
        *p = Some(PendingCapture {
            png: buf,
            w,
            h,
            main_visible: main_vis,
            panel_visible: panel_vis,
        });
    }
    // 4. Только теперь показываем окно выбора с готовым кадром.
    if let Some(win) = app.get_webview_window("select") {
        let _ = win.show();
        let _ = win.set_focus();
        let _ = win.set_always_on_top(true);
    }
    // Явный сигнал фронту — надёжнее, чем полагаться на событие фокуса.
    let _ = app.emit("trc:select-opened", ());
    Ok(serde_json::json!({ "w": w, "h": h }))
}

#[tauri::command]
fn take_selection_image(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let g = state.pending.lock().map_err(|e| e.to_string())?;
    let p = g.as_ref().ok_or("no pending capture")?;
    Ok(serde_json::json!({
        "pngBase64": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &p.png),
        "w": p.w,
        "h": p.h,
    }))
}

#[tauri::command]
fn close_selector(app: tauri::AppHandle, state: tauri::State<'_, AppState>) {
    if let Some(w) = app.get_webview_window("select") {
        let _ = w.hide();
    }
    // Возвращаем окна как было до захвата.
    if let Ok(mut p) = state.pending.lock() {
        if let Some(c) = p.take() {
            if c.main_visible {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                }
            }
            if c.panel_visible {
                if let Some(w) = app.get_webview_window("panel") {
                    let _ = w.show();
                }
            }
        }
    }
}

#[tauri::command]
fn toggle_panel(app: tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("panel") {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
        } else {
            let _ = w.show();
            let _ = w.set_focus();
            emit_settings(&app);
        }
    }
}

#[tauri::command]
fn show_panel(app: tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("panel") {
        let _ = w.show();
        let _ = w.set_focus();
    }
    emit_settings(&app);
}

#[tauri::command]
fn show_main(app: tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
        let _ = w.unminimize();
    }
}

#[tauri::command]
fn hide_to_tray(app: tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn set_hotkeys(state: tauri::State<'_, AppState>, overlay: String, translate: String, capture: String) {
    // Хоткеи регистрирует фронтенд (JS-плагин), здесь только сохраняем.
    if let Ok(mut s) = state.settings.lock() {
        s.overlay_hotkey = overlay;
        s.translate_hotkey = translate;
        s.capture_hotkey = capture;
    }
    persist(&state);
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let path = settings_path(&app.handle());
            let mut settings = load_settings(&path);
            // миграция со старых версий (был offline)
            if !["yandex", "bing", "google", "mymemory", "libre"].contains(&settings.engine.as_str()) {
                settings.engine = "yandex".into();
            }
            app.manage(AppState {
                settings_path: path,
                settings: Mutex::new(settings),
                bing: Mutex::new(None),
                pending: Mutex::new(None),
                cooldowns: Mutex::new(HashMap::new()),
            });

            // --- tray: работа в свёрнутом режиме (яркая иконка видна на любой панели) ---
            let quit = MenuItemBuilder::with_id("quit", "Выйти полностью").build(app)?;
            let show = MenuItemBuilder::with_id("show", "Показать TRC").build(app)?;
            let now = MenuItemBuilder::with_id("now", "Перевести сейчас").build(app)?;
            let menu = MenuBuilder::new(app).items(&[&show, &now, &quit]).build()?;
            let tray_icon = {
                let png = include_bytes!("../icons/tray-icon.png");
                let dyn_img = image::load_from_memory(png).unwrap_or_else(|_| {
                    image::DynamicImage::new_rgba8(32, 32)
                });
                let rgba = dyn_img.to_rgba8();
                let (w, h) = (rgba.width(), rgba.height());
                tauri::image::Image::new_owned(rgba.into_raw(), w, h)
            };
            TrayIconBuilder::with_id("main")
                .icon(tray_icon)
                .tooltip("TRC — живой перевод (работает в трее)")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, e| match e.id().as_ref() {
                    "quit" => app.exit(0),
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "now" => {
                        let _ = app.emit("trc:translate-now", ());
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, e| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = e
                    {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            if w.is_visible().unwrap_or(false) {
                                let _ = w.hide();
                            } else {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                    }
                })
                .build(app)?;

            // Крестик главного окна = свернуть в трей, перевод продолжает работать
            if let Some(main) = app.get_webview_window("main") {
                let h = main.clone();
                main.on_window_event(move |e| {
                    if let WindowEvent::CloseRequested { api, .. } = e {
                        api.prevent_close();
                        let _ = h.hide();
                    }
                });
                let h2 = main.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    let _ = h2.show();
                });
            }
            if let Some(d) = app.get_webview_window("panel") {
                let _ = d.set_always_on_top(true);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            translate_text,
            prepare_selection,
            take_selection_image,
            close_selector,
            toggle_panel,
            show_panel,
            set_hotkeys,
            show_main,
            hide_to_tray,
            quit_app
        ])
        .run(tauri::generate_context!())
        .expect("TRC failed to run");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_serialize_camel_case() {
        // Фронт ждёт camelCase: overlayHotkey, libreUrl и т.д.
        let s = Settings::default();
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v.get("engine").and_then(|v| v.as_str()), Some("yandex"));
        assert!(v.get("overlayHotkey").is_some(), "no overlayHotkey: {v}");
        assert!(v.get("translateHotkey").is_some(), "no translateHotkey: {v}");
        assert!(v.get("libreUrl").is_some(), "no libreUrl: {v}");
        assert!(v.get("overlay_hotkey").is_none(), "snake leaked: {v}");
    }

    #[test]
    fn settings_parse_frontend_payload() {
        // То, что шлёт фронт в save_settings
        let s: Settings = serde_json::from_str(
            r#"{"enabled":true,"engine":"google","source":"en","target":"ru","live":true,"overlayHotkey":"F8","translateHotkey":"Ctrl+Alt+T","libreUrl":"https://libretranslate.de"}"#,
        )
        .unwrap();
        assert_eq!(s.engine, "google");
        assert_eq!(s.overlay_hotkey, "F8");
        assert_eq!(s.libre_url, "https://libretranslate.de");
    }

    fn test_state() -> AppState {
        AppState {
            settings_path: PathBuf::from("test-settings.json"),
            settings: Mutex::new(Settings::default()),
            bing: Mutex::new(None),
            pending: Mutex::new(None),
            cooldowns: Mutex::new(HashMap::new()),
        }
    }

    #[test]
    fn engine_cooldown_marks_and_clears() {
        let st = test_state();
        assert!(!engine_in_cooldown(&st, "yandex"));
        mark_engine_fail(&st, "yandex");
        assert!(engine_in_cooldown(&st, "yandex"));
        assert!(!engine_in_cooldown(&st, "bing"));
        mark_engine_ok(&st, "yandex");
        assert!(!engine_in_cooldown(&st, "yandex"));
    }
}
