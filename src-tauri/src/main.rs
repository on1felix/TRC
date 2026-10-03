// TRC — переводчик EN<->RU: главное окно + панель поверх игр.
// Портативные настройки: <exe_dir>/data/settings.json (т.е. F:\TRC\data),
// fallback — %APPDATA%\com.trc.translator\settings.json
// Перевод — только онлайн без ключей (Яндекс, Bing, Google, MyMemory, Libre).

// Убирает консольное окно в релизе (в debug остаётся для логов).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::fs;
use std::os::windows::process::CommandExt;
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

mod mouse_hook;

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
    // Движок распознавания F9: system (Windows OCR, как в «Ножницах») или builtin (tesseract).
    #[serde(default = "d_ocr_engine")]
    ocr_engine: String,
}
fn d_true() -> bool {
    true
}
fn d_engine() -> String {
    "bing".into()
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
fn d_ocr_engine() -> String {
    "system".into()
}

    impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: true,
            engine: "bing".into(),
            source: "en".into(),
            target: "ru".into(),
            live: true,
            overlay_hotkey: "F8".into(),
            translate_hotkey: "Ctrl+Alt+T".into(),
            capture_hotkey: "F9".into(),
            libre_url: "https://libretranslate.de".into(),
            ocr_engine: "system".into(),
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
    // Основной файл → запасной settings.bak → дефолты.
    // Бэкап спасает, если файл побился (напр. убили процесс прямо во время записи).
    if let Ok(t) = fs::read_to_string(path) {
        if let Ok(s) = serde_json::from_str::<Settings>(&t) {
            return s;
        }
    }
    let bak = path.with_extension("bak");
    if let Ok(t) = fs::read_to_string(&bak) {
        if let Ok(s) = serde_json::from_str::<Settings>(&t) {
            return s;
        }
    }
    Settings::default()
}

fn persist(state: &AppState) {
    if let Some(parent) = state.settings_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let text = match state.settings.lock() {
        Ok(s) => serde_json::to_string_pretty(&*s).unwrap_or_default(),
        Err(_) => return,
    };
    if text.is_empty() {
        return;
    }
    // 1. Текущий хороший файл — в бэкап (если он валидный).
    let bak = state.settings_path.with_extension("bak");
    if let Ok(cur) = fs::read_to_string(&state.settings_path) {
        if serde_json::from_str::<Settings>(&cur).is_ok() {
            let _ = fs::write(&bak, cur);
        }
    }
    // 2. Атомарная запись: tmp + rename — файл никогда не бывает полупустым.
    let tmp = state.settings_path.with_extension("tmp");
    if fs::write(&tmp, text).is_ok() {
        let _ = fs::rename(&tmp, &state.settings_path);
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
    // Кодовые токены прячем: мобильная модель их уродует (GeminiAIChatCog → близнецы).
    let (shielded, map) = shield_tokens(text);
    let r: serde_json::Value = http()
        .post(&url)
        .header("User-Agent", YANDEX_UA)
        .form(&[
            ("source_lang", src),
            ("target_lang", dst),
            ("text", shielded.as_str()),
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
        .map(|s| unshield(s, &map))
        .ok_or_else(|| "bad yandex response".to_string())
}

async fn tr_yandex_line(text: &str, src: &str, dst: &str) -> Result<String, String> {
    // 405 = протухшая сессия: одна повторная попытка с новой сессией чинит.
    match tr_yandex_once(text, src, dst).await {
        Err(e) if e.starts_with("yandex 405") => tr_yandex_once(text, src, dst).await,
        r => r,
    }
}

/// Токены, которые движки без NER уродуют (мобильный яндекс, gtx):
/// CamelCase (GeminiAIChatCog), snake_case (ai_info), dotted (Paiza.IO),
/// bot-команды (n!code), версии/числа с точками (32.000.000).
/// Обычные слова (Translator, AI, FREE, end.) — не трогаем, они переводятся.
fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '!'
}

fn is_code_token(tok: &str) -> bool {
    if !tok.chars().any(|c| c.is_ascii_alphanumeric()) {
        return false;
    }
    // Точка внутри слова: Paiza.IO, 32.000 (но не «end.» — там пустой хвост).
    if tok.contains('.') && tok.split('.').filter(|p| !p.is_empty()).count() >= 2 {
        return true;
    }
    if tok.contains('_') {
        return true;
    }
    // «!» между словом: n!code (а не «Hello!»).
    if tok.contains('!') && tok.split('!').filter(|p| !p.is_empty()).count() >= 2 {
        return true;
    }
    // CamelCase: есть строчные И (2+ заглавных ИЛИ переход lower→Upper).
    if tok.chars().any(|c| c.is_ascii_lowercase()) {
        if tok.chars().filter(|c| c.is_ascii_uppercase()).count() >= 2 {
            return true;
        }
        let mut prev_lower = false;
        for c in tok.chars() {
            if c.is_ascii_uppercase() && prev_lower {
                return true;
            }
            prev_lower = c.is_ascii_lowercase();
        }
    }
    false
}

/// Прячем кодовые токены за плейсхолдеры вида zz_trc_0 (строчные —
/// переживают оба движка дословно, а капс гугл транслитерирует — проверено).
/// Счётчик глобальный: один и тот же плейсхолдер не должен значить разное
/// в разных запросах (движки кэшируют).
/// Возвращаем текст и карту для обратной замены.
fn shield_tokens(text: &str) -> (String, Vec<(String, String)>) {
    // Сам плейсхолдер в тексте — не shielding, чтобы не подменять чужое.
    if text.contains("zz_trc_") {
        return (text.to_string(), Vec::new());
    }
    static SHIELD_CTR: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let mut out = String::with_capacity(text.len());
    let mut map: Vec<(String, String)> = Vec::new();
    let mut buf = String::new();
    let flush = |buf: &mut String, out: &mut String, map: &mut Vec<(String, String)>| {
        if buf.is_empty() {
            return;
        }
        // Висячая пунктуация — не часть токена: «Paiza.IO.» → «Paiza.IO» + «.».
        let mut tail = String::new();
        while let Some(c) = buf.chars().last() {
            if ".,!?:;".contains(c) {
                tail.insert(0, c);
                buf.pop();
            } else {
                break;
            }
        }
        if !buf.is_empty() && is_code_token(buf) {
            let n = SHIELD_CTR.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let ph = format!("zz_trc_{n}");
            out.push_str(&ph);
            map.push((ph, buf.clone()));
        } else {
            out.push_str(buf);
        }
        out.push_str(&tail);
        buf.clear();
    };
    for c in text.chars() {
        if is_token_char(c) {
            buf.push(c);
        } else {
            flush(&mut buf, &mut out, &mut map);
            out.push(c);
        }
    }
    flush(&mut buf, &mut out, &mut map);
    (out, map)
}

/// Возвращаем токены на место. Совпадение — точное, запасное — без учёта регистра
/// (движок иногда меняет кейс плейсхолдера).
fn unshield(text: &str, map: &[(String, String)]) -> String {
    let mut s = text.to_string();
    // Длинные плейсхолдеры первыми: zz_trc_1 — префикс zz_trc_12.
    let mut order: Vec<usize> = (0..map.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(map[i].0.len()));
    for &i in &order {
        let (ph, orig) = &map[i];
        if s.contains(ph) {
            s = s.replace(ph, orig);
            continue;
        }
        let slow = s.to_lowercase();
        let needle = ph.to_lowercase();
        let mut res = String::with_capacity(s.len());
        let mut start = 0;
        let mut found = false;
        while let Some(rel) = slow[start..].find(needle.as_str()) {
            let i = start + rel;
            let j = i + needle.len();
            match (s.get(start..i), s.get(j..)) {
                (Some(head), _) => {
                    res.push_str(head);
                    res.push_str(orig);
                    start = j;
                    found = true;
                }
                _ => break,
            }
        }
        if found {
            if let Some(tail) = s.get(start..) {
                res.push_str(tail);
                s = res;
            }
        }
    }
    // Движок мог разорвать dotted-токен пробелами («Paiza. IO», «Paiza.  IO»).
    // Склеиваем только дословные варианты НАШИХ токенов — чужое не трогаем.
    for (_, orig) in map.iter().filter(|(_, o)| o.contains('.')) {
        s = join_spaced_token(&s, orig);
    }
    s
}

/// Склейка «Paiza.␣IO» → «Paiza.IO»: части исходного токена через '.' + пробелы.
/// Без пробелов (чистый токен) — не трогаем, чужое — не трогаем.
fn join_spaced_token(s: &str, orig: &str) -> String {
    let parts: Vec<&str> = orig.split('.').collect();
    if parts.len() < 2 || parts.iter().any(|p| p.is_empty()) {
        return s.to_string();
    }
    let mut out = String::new();
    let mut rest = s;
    loop {
        let Some(i) = rest.find(parts[0]) else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..i]);
        let mut j = i + parts[0].len();
        let mut ok = true;
        for k in 1..parts.len() {
            if !rest.get(j..).map(|r| r.starts_with('.')).unwrap_or(false) {
                ok = false;
                break;
            }
            j += 1;
            let mut saw_ws = false;
            // Любой пробел, включая неразрывный (гугл любит U+00A0).
            while let Some(c) = rest[j..].chars().next() {
                if c.is_whitespace() {
                    j += c.len_utf8();
                    saw_ws = true;
                } else {
                    break;
                }
            }
            if !saw_ws {
                ok = false;
                break;
            }
            if !rest.get(j..).map(|r| r.starts_with(parts[k])).unwrap_or(false) {
                ok = false;
                break;
            }
            j += parts[k].len();
        }
        if ok {
            out.push_str(orig);
            rest = &rest[j..];
        } else if let Some(c) = rest[i..].chars().next() {
            let n = i + c.len_utf8();
            out.push_str(&rest[i..n]);
            rest = &rest[n..];
        } else {
            break;
        }
    }
    out
}

/// Списки/колонки vs проза: короткие строки разной длины — пункты,
/// ровные длинные — завёрнутая проза (её рвать нельзя: обрывки переводятся хуже).
fn looks_like_list(text: &str) -> bool {
    let lines: Vec<&str> = text
        .split('\n')
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let n = lines.len();
    if n < 4 {
        return false;
    }
    let mut lens: Vec<usize> = lines.iter().map(|l| l.chars().count()).collect();
    let avg = lens.iter().sum::<usize>() as f64 / n as f64;
    lens.sort_unstable();
    // Разброс середин (IQR): у списков строки разной длины, у завёрнутой прозы
    // все полные строки ровные — короткий хвост не в счёт (spread по краям врёт).
    let iqr = lens[3 * n / 4] - lens[n / 4];
    (avg <= 80.0 && iqr > 12) || avg <= 25.0
}

/// Колонки: «  code␣␣␣Run it» → (отступ, [(«code», зазор), («Run it», «»)]).
/// Зазор — 2+ пробелов/табов; одиночные пробелы остаются внутри кусков.
/// Зазоры храним как есть, чтобы вернуть дословно.
fn split_columns(line: &str) -> (String, Vec<(String, String)>) {
    let indent_len = line.len() - line.trim_start_matches([' ', '\t']).len();
    let indent = line[..indent_len].to_string();
    let rest = &line[indent_len..];
    let mut segs: Vec<(String, String)> = Vec::new();
    let mut cur = String::new();
    let mut gap = String::new();
    let mut in_gap = false;
    for c in rest.chars() {
        if c == ' ' || c == '\t' {
            in_gap = true;
            gap.push(c);
        } else {
            if in_gap {
                if gap.chars().count() >= 2 {
                    segs.push((cur.clone(), gap.clone()));
                    cur.clear();
                } else {
                    cur.push_str(&gap);
                }
                gap.clear();
                in_gap = false;
            }
            cur.push(c);
        }
    }
    if in_gap {
        if gap.chars().count() >= 2 && !cur.trim().is_empty() {
            segs.push((cur.clone(), gap.clone()));
            cur.clear();
        } else {
            cur.push_str(&gap);
        }
    }
    if !cur.trim().is_empty() || segs.is_empty() {
        segs.push((cur, String::new()));
    }
    (indent, segs)
}

async fn tr_yandex(text: &str, src: &str, dst: &str) -> Result<String, String> {
    // Мобильная модель крошит структуру целого куска (клеит строки, теряет слова),
    // а короткие независимые строки переводит прилично — списки гоним построчно.
    // Куски колонок — отдельно, зазоры возвращаем дословно (движок их схлопывает).
    if looks_like_list(text) {
        let mut out: Vec<String> = Vec::new();
        for ln in text.split('\n') {
            if ln.trim().is_empty() {
                out.push(String::new());
                continue;
            }
            let (indent, segs) = split_columns(ln);
            if segs.len() < 2 {
                let one = tr_yandex_line(ln, src, dst).await?;
                if yandex_hallucinated_brand(ln, &one) {
                    return Err("yandex: brand hallucination".to_string());
                }
                out.push(one);
                continue;
            }
            let mut line = indent;
            for (seg, gap) in segs {
                if seg.trim().is_empty() {
                    continue;
                }
                let t = tr_yandex_line(&seg, src, dst).await?;
                line.push_str(if t.trim().is_empty() { &seg } else { &t });
                line.push_str(&gap);
            }
            let line = line.trim_end().to_string();
            if yandex_hallucinated_brand(ln, &line) {
                return Err("yandex: brand hallucination".to_string());
            }
            out.push(line);
        }
        return Ok(out.join("\n"));
    }
    tr_yandex_line(text, src, dst).await
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
    // Как у яндекса: списки — построчно с дословными зазорами (гугл их схлопывает).
    if looks_like_list(text) {
        let mut out: Vec<String> = Vec::new();
        for ln in text.split('\n') {
            if ln.trim().is_empty() {
                out.push(String::new());
                continue;
            }
            let (indent, segs) = split_columns(ln);
            if segs.len() < 2 {
                out.push(tr_google_once(ln, src, dst).await?);
                continue;
            }
            let mut line = indent;
            for (seg, gap) in segs {
                if seg.trim().is_empty() {
                    continue;
                }
                let t = tr_google_once(&seg, src, dst).await?;
                line.push_str(if t.trim().is_empty() { &seg } else { &t });
                line.push_str(&gap);
            }
            out.push(line.trim_end().to_string());
        }
        return Ok(out.join("\n"));
    }
    tr_google_once(text, src, dst).await
}

async fn tr_google_once(text: &str, src: &str, dst: &str) -> Result<String, String> {
    let url = "https://translate.googleapis.com/translate_a/single";
    // Как у яндекса: прячем кодовые токены (н! код, БлизнецыAIChatCog).
    let (shielded, map) = shield_tokens(text);
    let r: serde_json::Value = http()
        .get(url)
        .query(&[
            ("client", "gtx"),
            ("sl", src),
            ("tl", dst),
            ("dt", "t"),
            ("q", shielded.as_str()),
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
        Ok(unshield(&s, &map))
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

/// Мобильный яндекс любит галлюцинировать бренд («маркете» → «Yandex. Market»),
/// хотя в исходнике его нет. Перевод с выдуманным именем собственным считаем
/// сбоем движка — цепочка-страховка идёт дальше (там обычно Bing, он точнее).
fn yandex_hallucinated_brand(source: &str, translated: &str) -> bool {
    const BRANDS: [&str; 2] = ["yandex", "яндекс"];
    let slow = source.to_lowercase();
    let tlow = translated.to_lowercase();
    // Бренд в переводе при полном отсутствии в исходнике (в любом написании).
    // «яндекс» → «Yandex» — честная транслитерация, не триггерит.
    let in_src = BRANDS.iter().any(|b| slow.contains(b));
    let in_tr = BRANDS.iter().any(|b| tlow.contains(b));
    in_tr && !in_src
}

/// Точечные правки по подсказке исходника (для любого движка):
/// «есть торг» — это negotiable, а не «there is a bargain» (выгодная покупка).
fn fix_lexicon(source: &str, translated: &str) -> String {
    let slow = source.to_lowercase();
    let mut s = translated.to_string();
    if slow.contains("торг") {
        s = s
            .replace("There is a bargain", "Negotiable")
            .replace("there is a bargain", "negotiable");
    }
    s
}

/// Яндекс на многострочном тексте любит уронить запятую/точку
/// в начало следующей строки («…управления\n, и я…»).
/// Приклеиваем такую строку к предыдущей без пробела перед знаком.
fn fix_punct_lines(s: &str) -> String {
    const CLOSING: [char; 12] = [
        ',', '.', '!', '?', ':', ';', '%', ')', ']', '}', '»', '…',
    ];
    let mut lines: Vec<String> = Vec::new();
    for raw in s.split('\n') {
        let line = raw.trim();
        match line.chars().next() {
            Some(c) if CLOSING.contains(&c) => match lines.last_mut() {
                Some(prev) if !prev.is_empty() => prev.push_str(line),
                _ => lines.push(line.to_string()),
            },
            _ => lines.push(raw.trim_end().to_string()),
        }
    }
    lines.join("\n")
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
            Ok(s) if !s.trim().is_empty() => {
                if e == "yandex" && yandex_hallucinated_brand(&t, &s) {
                    last_err = format!("{e}: brand hallucination");
                    continue;
                }
                return Ok(fix_punct_lines(&fix_lexicon(&t, &s)));
            }
            Ok(_) => last_err = format!("{e}: empty"),
            Err(err) => last_err = format!("{e}: {err}"),
        }
    }
    Err(last_err)
}

// ---------- автообновление через GitHub Releases ----------
const GITHUB_API: &str = "https://api.github.com/repos/on1felix/TRC/releases/latest";

#[derive(Serialize)]
struct UpdateInfo {
    current: String,
    latest: String,
    download_url: String,
    size: u64,
}

#[derive(Clone, Serialize)]
struct UpdateProgress {
    downloaded: u64,
    total: u64,
    percent: f64,
    speed_mbps: f64,
}

fn parse_version(s: &str) -> Vec<u64> {
    s.split('.').filter_map(|p| p.parse::<u64>().ok()).collect()
}

fn is_newer_version(latest: &str, current: &str) -> bool {
    let mut lat = parse_version(latest);
    let mut cur = parse_version(current);
    let len = lat.len().max(cur.len());
    lat.resize(len, 0);
    cur.resize(len, 0);
    lat > cur
}

fn strip_tag(tag: &str) -> String {
    tag.replace("TRC-", "")
        .replace("TRC_", "")
        .trim_start_matches('v')
        .to_string()
}

/// Проверка последнего релиза на GitHub
#[tauri::command]
async fn check_update(app: tauri::AppHandle) -> Result<Option<UpdateInfo>, String> {
    let json: serde_json::Value = http()
        .get(GITHUB_API)
        .header("User-Agent", "TRC-Updater")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let tag = json["tag_name"].as_str().unwrap_or("");
    let latest = strip_tag(tag);
    if latest.is_empty() {
        return Ok(None);
    }

    let current = app.package_info().version.to_string();
    if !is_newer_version(&latest, &current) {
        return Ok(None);
    }

    // Предпочитаем portable TRC.exe (замена на месте), иначе первый .exe.
    let mut download_url: Option<String> = None;
    let mut fallback: Option<(String, u64)> = None;
    let mut size = 0u64;
    if let Some(assets) = json["assets"].as_array() {
        for a in assets {
            let name = a["name"].as_str().unwrap_or("");
            if !name.ends_with(".exe") {
                continue;
            }
            let url = match a["browser_download_url"].as_str() {
                Some(u) => u.to_string(),
                None => continue,
            };
            let sz = a["size"].as_u64().unwrap_or(0);
            if name.eq_ignore_ascii_case("TRC.exe") {
                download_url = Some(url);
                size = sz;
                break;
            }
            if fallback.is_none() {
                fallback = Some((url, sz));
            }
        }
    }
    if download_url.is_none() {
        if let Some((url, sz)) = fallback {
            download_url = Some(url);
            size = sz;
        }
    }
    match download_url {
        Some(url) => Ok(Some(UpdateInfo { current, latest, download_url: url, size })),
        None => Ok(None),
    }
}

/// Скачивание новой версии во временную папку с прогрессом
#[tauri::command]
async fn download_update(app: tauri::AppHandle, url: String) -> Result<String, String> {
    use std::io::Write;

    let mut res = http()
        .get(&url)
        .header("User-Agent", "TRC-Updater")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let total = res.content_length().unwrap_or(0);

    let tmp = std::env::temp_dir().join("TRC_update.exe");
    let mut file = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;

    let mut downloaded: u64 = 0;
    let mut last_time = std::time::Instant::now();
    let mut last_bytes: u64 = 0;
    let mut speed = 0.0f64;

    while let Some(chunk) = res.chunk().await.map_err(|e| e.to_string())? {
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;

        let elapsed = last_time.elapsed().as_secs_f64();
        if elapsed >= 0.25 {
            let instant = (downloaded - last_bytes) as f64 / elapsed / (1024.0 * 1024.0);
            speed = if speed == 0.0 { instant } else { speed * 0.7 + instant * 0.3 };
            last_time = std::time::Instant::now();
            last_bytes = downloaded;
            let _ = app.emit(
                "update-progress",
                UpdateProgress {
                    downloaded,
                    total,
                    percent: if total > 0 { downloaded as f64 / total as f64 * 100.0 } else { 0.0 },
                    speed_mbps: speed,
                },
            );
        }
    }
    file.flush().ok();
    drop(file);

    let _ = app.emit(
        "update-progress",
        UpdateProgress { downloaded, total, percent: 100.0, speed_mbps: speed },
    );
    Ok(tmp.to_string_lossy().into_owned())
}

/// Замена текущего exe на скачанный и перезапуск.
/// Скрытый bat-скрипт: дожидается закрытия приложения → удаляет старую версию →
/// перемещает скачанный файл на её место → запускает новую версию → удаляет себя.
#[tauri::command]
fn apply_update() -> Result<(), String> {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let new_exe = std::env::temp_dir().join("TRC_update.exe");
    if !new_exe.exists() {
        return Err("Файл обновления не найден".into());
    }
    let cur = std::env::current_exe().map_err(|e| format!("Не удалось определить путь: {}", e))?;
    let pid = std::process::id();

    let bat = std::env::temp_dir().join("trc_update.bat");
    let script = String::from(
        "@echo off\r\n\
         setlocal\r\n\
         set /a TRIES=0\r\n\
         :wait_loop\r\n\
         tasklist /FI \"PID eq %1\" 2>NUL | find \"%1\" >NUL\r\n\
         if errorlevel 1 goto proc_dead\r\n\
         set /a TRIES+=1\r\n\
         if %TRIES% GEQ 30 goto force_kill\r\n\
         ping -n 2 127.0.0.1 >NUL\r\n\
         goto wait_loop\r\n\
         :force_kill\r\n\
         taskkill /F /PID %1 >NUL 2>&1\r\n\
         ping -n 3 127.0.0.1 >NUL\r\n\
         :proc_dead\r\n\
         ping -n 4 127.0.0.1 >NUL\r\n\
         del /f /q \"%2\"\r\n\
         if exist \"%2\" (\r\n\
           ping -n 4 127.0.0.1 >NUL\r\n\
           del /f /q \"%2\"\r\n\
         )\r\n\
         move /y \"%3\" \"%2\"\r\n\
         start \"\" \"%2\"\r\n\
         endlocal\r\n\
         (goto) 2>NUL & del /f /q \"%~f0\"\r\n",
    );
    std::fs::write(&bat, script).map_err(|e| format!("Не удалось создать скрипт обновления: {}", e))?;

    std::process::Command::new("cmd")
        .args([
            "/C",
            &bat.to_string_lossy(),
            &pid.to_string(),
            &cur.to_string_lossy(),
            &new_exe.to_string_lossy(),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("Не удалось запустить обновление: {}", e))?;
    std::process::exit(0);
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
    mouse_hook::configure(&settings.overlay_hotkey, &settings.capture_hotkey);
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
    // Тоггл: окно выбора уже открыто — закрываем его вместо второго слоя.
    if win_visible(&app, "select") {
        close_selector_inner(&app, &state, true);
        return Ok(serde_json::json!({ "toggled": false, "w": 0, "h": 0 }));
    }    // 1. Запоминаем и прячем свои окна — иначе они попадут в кадр.
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
    Ok(serde_json::json!({ "toggled": true, "w": w, "h": h }))
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
fn close_selector(app: tauri::AppHandle, state: tauri::State<'_, AppState>, restore: bool) {
    close_selector_inner(&app, &state, restore);
}

// ---------- системный OCR (тот же движок, что в «Ножницах»: Windows.Media.Ocr) ----------
// Первичный путь распознавания F9: офлайн, быстро, I не путает с |.
// Tesseract.js на фронте остаётся запасным на случай недоступности WinRT.
fn make_ocr_engine(lang: &str) -> Result<windows::Media::Ocr::OcrEngine, String> {
    use windows::Globalization::Language;
    use windows::Media::Ocr::OcrEngine;
    // Сначала язык исходника, потом второй наш, потом что есть у пользователя.
    let mut prefs: Vec<&str> = vec![lang];
    for l in ["en", "ru"] {
        if !prefs.contains(&l) {
            prefs.push(l);
        }
    }
    for l in prefs {
        let tag = windows::core::HSTRING::from(l);
        if let Ok(language) = Language::CreateLanguage(&tag) {
            if let Ok(eng) = OcrEngine::TryCreateFromLanguage(&language) {
                return Ok(eng);
            }
        }
    }
    OcrEngine::TryCreateFromUserProfileLanguages()
        .map_err(|e| format!("Windows OCR недоступен: {e}"))
}

fn ocr_png_to_bitmap(png: &[u8]) -> Result<windows::Graphics::Imaging::SoftwareBitmap, String> {
    use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
    use windows::Storage::Streams::DataWriter;
    let img = image::load_from_memory(png).map_err(|e| format!("не картинка: {e}"))?;
    let mut rgba = img.to_rgba8();
    // OcrEngine берёт ограниченный размер — большое жмём заранее.
    const MAX_SIDE: u32 = 3000;
    let (w0, h0) = (rgba.width(), rgba.height());
    if w0.max(h0) > MAX_SIDE {
        let (nw, nh) = if w0 >= h0 {
            (MAX_SIDE, h0 * MAX_SIDE / w0)
        } else {
            (w0 * MAX_SIDE / h0, MAX_SIDE)
        };
        rgba = image::imageops::resize(
            &rgba,
            nw.max(1),
            nh.max(1),
            image::imageops::FilterType::Triangle,
        );
    }
    let (w, h) = (rgba.width() as i32, rgba.height() as i32);
    // RGBA → Bgra8: меняем R и B местами.
    let mut bgra = rgba.into_raw();
    for px in bgra.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    let writer = DataWriter::new().map_err(|e| format!("winrt buffer: {e}"))?;
    writer
        .WriteBytes(&bgra)
        .map_err(|e| format!("winrt buffer: {e}"))?;
    let buf = writer
        .DetachBuffer()
        .map_err(|e| format!("winrt buffer: {e}"))?;
    SoftwareBitmap::CreateCopyFromBuffer(&buf, BitmapPixelFormat::Bgra8, w, h)
        .map_err(|e| format!("bitmap: {e}"))
}

struct OcrWordBox {
    text: String,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

/// Собираем строки сами по координатам слов, а не по порядку движка:
/// движок складывает колонки отдельными блоками (все команды — потом все описания),
/// а зазоры между словами превращаем в пробелы — колонки сохраняются.
fn rebuild_lines(words: &mut Vec<OcrWordBox>) -> String {
    words.retain(|w| !w.text.trim().is_empty());
    if words.is_empty() {
        return String::new();
    }
    let mut hs: Vec<f32> = words.iter().map(|w| (w.y1 - w.y0).max(1.0)).collect();
    hs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let med_h = hs[hs.len() / 2].max(4.0);
    let char_w = (med_h * 0.55).max(1.0);
    // Строки: сортировка по вертикальному центру, разрыв при скачке > 0.6 высоты.
    words.sort_by(|a, b| {
        ((a.y0 + a.y1) / 2.0)
            .partial_cmp(&((b.y0 + b.y1) / 2.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut rows: Vec<Vec<&OcrWordBox>> = Vec::new();
    let mut last_yc = f32::MIN;
    for w in words.iter() {
        let yc = (w.y0 + w.y1) / 2.0;
        if rows.is_empty() || yc - last_yc > med_h * 0.6 {
            rows.push(Vec::new());
        }
        last_yc = yc;
        if let Some(row) = rows.last_mut() {
            row.push(w);
        }
    }
    let mut out: Vec<String> = Vec::new();
    // Сначала сортируем слова в строках слева направо — зазоры считаем уже по порядку.
    for row in rows.iter_mut() {
        row.sort_by(|a, b| {
            a.x0
                .partial_cmp(&b.x0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    // Единица пробела — медиана мелких зазоров (обычные пробелы <
    // высоты строки, колоночные — в разы больше). Падаем на char_w, если нечего мерить.
    let mut small: Vec<f32> = Vec::new();
    for row in rows.iter() {
        let mut px = f32::MIN;
        let mut first = true;
        for w in row.iter() {
            if !first {
                let gap = w.x0 - px;
                if gap > 0.0 && gap < med_h {
                    small.push(gap);
                }
            }
            first = false;
            px = w.x1;
        }
    }
    small.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let space_unit = small
        .get(small.len() / 2)
        .copied()
        .unwrap_or(char_w)
        .max(1.0);
    for row in rows.iter_mut() {
        let mut line = String::new();
        let mut prev_x1 = f32::MIN;
        for w in row.iter() {
            if !line.is_empty() {
                let gap = w.x0 - prev_x1;
                // Обычный пробел — один; колонки — по ширине зазора.
                let n = ((gap / space_unit).round() as usize).clamp(1, 32);
                line.push_str(&" ".repeat(n));
            }
            line.push_str(w.text.trim());
            prev_x1 = w.x1;
        }
        let line = line.trim_end().to_string();
        if !line.is_empty() {
            out.push(line);
        }
    }
    out.join("\n").trim().to_string()
}

fn recognize_png_blocking(png: &[u8], lang: &str) -> Result<String, String> {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
    // COM-квартира для WinRT на этом потоке (повторный вызов безвреден).
    // Блокирующее .get(): движок считает ~0.5–2с на синхронном пуле Tauri.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let sb = ocr_png_to_bitmap(png)?;
    let engine = make_ocr_engine(lang)?;
    let op = engine
        .RecognizeAsync(&sb)
        .map_err(|e| format!("ocr start: {e}"))?;
    let res = op.get().map_err(|e| format!("ocr: {e}"))?;
    let mut words: Vec<OcrWordBox> = Vec::new();
    let lines = res.Lines().map_err(|e| format!("ocr lines: {e}"))?;
    for line in lines {
        let ws = match line.Words() {
            Ok(ws) => ws,
            Err(_) => continue,
        };
        for w in ws {
            let (t, r) = match (w.Text(), w.BoundingRect()) {
                (Ok(t), Ok(r)) => (t, r),
                _ => continue,
            };
            let s = t.to_string_lossy();
            if s.trim().is_empty() {
                continue;
            }
            words.push(OcrWordBox {
                text: s,
                x0: r.X,
                y0: r.Y,
                x1: r.X + r.Width,
                y1: r.Y + r.Height,
            });
        }
    }
    Ok(rebuild_lines(&mut words))
}

#[tauri::command(rename_all = "camelCase")]
fn ocr_image(image_base64: String, lang: String) -> Result<String, String> {
    let raw = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        image_base64.trim(),
    )
    .map_err(|e| format!("bad base64: {e}"))?;
    if raw.len() > 25 * 1024 * 1024 {
        return Err("картинка больше 25МБ".into());
    }
    recognize_png_blocking(&raw, lang.trim())
}

// Быстрая проверка для статуса в главном окне (без распознавания).
// Ok — «OCR готов», Err с причиной — показываем пользователю как есть.
#[tauri::command]
fn ocr_available() -> Result<(), String> {
    make_ocr_engine("en")
        .map(|_| ())
        .map_err(|e| format!("{e} (нужен языковой пакет распознавания текста EN/RU)"))
}

fn close_selector_inner(app: &tauri::AppHandle, state: &AppState, restore: bool) {
    if let Some(w) = app.get_webview_window("select") {
        let _ = w.hide();
    }
    // Возвращаем окна как было до захвата (при успехе — нет: откроется только панель).
    if !restore {
        if let Ok(mut p) = state.pending.lock() {
            p.take();
        }
        return;
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
    // Хоткеи регистрирует фронтенд (JS-плагин) + нативный хук мыши, здесь только сохраняем.
    if let Ok(mut s) = state.settings.lock() {
        s.overlay_hotkey = overlay.clone();
        s.translate_hotkey = translate;
        s.capture_hotkey = capture.clone();
    }
    mouse_hook::configure(&overlay, &capture);
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
                settings: Mutex::new(settings.clone()),
                bing: Mutex::new(None),
                pending: Mutex::new(None),
                cooldowns: Mutex::new(HashMap::new()),
            });
            mouse_hook::configure(&settings.overlay_hotkey, &settings.capture_hotkey);
            mouse_hook::install(app.handle().clone());

            // --- tray: работа в свёрнутом режиме (яркая иконка видна на любой панели) ---
            let quit = MenuItemBuilder::with_id("quit", "Выйти полностью").build(app)?;
            let show = MenuItemBuilder::with_id("show", "Показать TRC").build(app)?;
            let menu = MenuBuilder::new(app).items(&[&show, &quit]).build()?;
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
            ocr_image,
            ocr_available,
            close_selector,
            toggle_panel,
            show_panel,
            set_hotkeys,
            show_main,
            hide_to_tray,
            quit_app,
            check_update,
            download_update,
            apply_update
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
        assert_eq!(v.get("engine").and_then(|v| v.as_str()), Some("bing"));
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
        assert_eq!(s.ocr_engine, "system"); // старого файла без поля хватает
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

    #[test]
    fn yandex_brand_guard_and_bargain_fix() {
        // «Стоит на маркете» + выдуманный Yandex в переводе = галлюцинация.
        assert!(yandex_hallucinated_brand(
            "Стоит на маркете",
            "Available on Yandex. Market"
        ));
        // Бренд из исходника — не галлюцинация.
        assert!(!yandex_hallucinated_brand(
            "купи на яндекс маркете",
            "buy on Yandex Market"
        ));
        assert!(!yandex_hallucinated_brand("hello world", "привет мир"));
        // «есть торг» — negotiable, не bargain.
        assert_eq!(
            fix_lexicon("есть торг", "There is a bargain."),
            "Negotiable."
        );
        // Без торга в исходнике — не трогаем.
        assert_eq!(
            fix_lexicon("a good deal", "There is a bargain."),
            "There is a bargain."
        );
    }

    #[test]
    fn punct_lines_glue_to_prev() {        // Кейс со скрина: запятая и точка упали на новые строки.
        let inp = "размер элемента управления\n, и я использую режим\nцвета discord по умолчанию\n. О, мне, наверное";
        let out = fix_punct_lines(inp);
        assert_eq!(
            out,
            "размер элемента управления, и я использую режим\nцвета discord по умолчанию. О, мне, наверное"
        );
        // Тире прямой речи не трогаем, обычные строки тоже.
        let dlg = "он сказал\n— иди сюда";
        assert_eq!(fix_punct_lines(dlg), dlg);
        assert_eq!(fix_punct_lines("a\nb"), "a\nb");
    }

    #[test]
    fn winocr_reads_sample() {
        // Настоящий прогон Windows.Media.Ocr по эталону вида Discord.
        // Фикстура извне: TRC_OCR_TEST_PNG=C:\...\discord-sample.png
        let Ok(path) = std::env::var("TRC_OCR_TEST_PNG") else {
            eprintln!("SKIP winocr_reads_sample: TRC_OCR_TEST_PNG not set");
            return;
        };
        let png = std::fs::read(&path).unwrap_or_else(|_| panic!("no fixture: {path}"));
        let text = recognize_png_blocking(&png, "en").expect("win ocr failed");
        eprintln!("OCR>> {text}");
        for w in ["toggle", "midnight", "theme", "control"] {
            assert!(
                text.to_lowercase().contains(w),
                "нет '{w}' в:\n{text}"
            );
        }
        assert!(text.contains('I'), "потерялась I в:\n{text}");
    }

    #[test]
    fn ocr_engine_creates() {
        // Системный движок доступен (иначе и статус, и F9 не заведутся).
        assert!(make_ocr_engine("en").is_ok());
    }

    #[test]
    fn columns_split() {
        let (ind, segs) = split_columns("  code              Run it");
        assert_eq!(ind, "  ");
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].0, "code");
        assert_eq!(segs[0].1.chars().count(), 14);
        assert_eq!(segs[1].0, "Run it");
        assert_eq!(segs[1].1, "");
        // Без зазора — один кусок.
        let (_, one) = split_columns("hello world");
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].0, "hello world");
        // Одиночный пробел — не зазор.
        let (_, two) = split_columns("a b");
        assert_eq!(two.len(), 1);
        // Три колонки.
        let (_, three) = split_columns("pk  a  b");
        assert_eq!(three.len(), 3);
    }

    #[test]
    fn shield_roundtrip() {
        let (s, map) = shield_tokens("Run ai_info Paiza.IO. n!code! end. AI Translator CodeCompiler viewthreadchannel");
        // Спрятаны кодовые, обычные на месте.
        assert!(s.contains("zz_trc_"), "ничего не спрятано: {s}");
        assert!(s.contains("Run"), "обычное слово тронуто: {s}");
        assert!(s.contains("end."), "точка в конце тронута: {s}");
        assert!(s.contains(" AI "), "аббревиатура тронута: {s}");
        assert!(s.contains("Translator"), "обычное слово тронуто: {s}");
        assert!(s.contains("viewthreadchannel"), "длинное слово тронуто: {s}");
        assert_eq!(map.len(), 4, "карта: {map:?}");
        let paiza = map.iter().find(|(_, o)| o == "Paiza.IO").expect("no Paiza map");
        let ncode = map.iter().find(|(_, o)| o == "n!code").expect("no ncode map");
        assert!(s.contains(&format!("{}.", paiza.0)), "точка потеряна: {s}");
        assert!(s.contains(&format!("{}!", ncode.0)), "восклицание потеряно: {s}");
        // Обратная замена дословная.
        let back = unshield(&s, &map);
        assert_eq!(
            back,
            "Run ai_info Paiza.IO. n!code! end. AI Translator CodeCompiler viewthreadchannel"
        );
        // Запасной путь: движок сменил регистр плейсхолдера.
        let mangled = s.replace(&paiza.0, &paiza.0.to_uppercase());
        assert!(unshield(&mangled, &map).contains("Paiza.IO"));
        // Склейка разорванного движком dotted-токена (только своих).
        let map2 = vec![("zz_trc_9".to_string(), "Paiza.IO".to_string())];
        assert_eq!(
            unshield("using Paiza. IO daily", &map2),
            "using Paiza.IO daily"
        );
        assert_eq!(
            unshield("using Paiza.  IO daily", &map2),
            "using Paiza.IO daily"
        );
        assert_eq!(
            unshield("end. Start of it", &map2),
            "end. Start of it",
            "чужое не трогаем"
        );
    }

    #[test]
    fn list_detector() {
        // Колонки Heroes-команд: короткие строки разной длины.
        assert!(looks_like_list(
            "CodeCompiler:\ncode Run it\nlanguages List all\nContentBuilder:\nview It\nTranslator:\ntranslate It to lang\nNo Category:\nhelp Shows this"
        ));
        // Проза в три строки — не список.
        assert!(!looks_like_list(
            "well I read the theme file and only saw the part mentioning the toggle on it\nand I'm using the colors off mode to maintain default discord colors\noh I should probably have mentioned that I'm asking about the midnight theme"
        ));
        // Мало строк — не список.
        assert!(!looks_like_list("one\ntwo"));
        // Ровные длинные строки (завёрнутая проза) — не список.
        assert!(!looks_like_list(
            "lorem ipsum dolor sit amet consectetur adipiscing elit sed\nlorem ipsum dolor sit amet consectetur adipiscing elit sed\nlorem ipsum dolor sit amet consectetur adipiscing elit sed\nshort tail"
        ));
    }

    #[test]
    fn winocr_keeps_columns() {
        // Двухколоночный текст: команды и описания должны остаться в одних строках,
        // а не складываться блоками (все команды — потом все описания).
        let Ok(sample) = std::env::var("TRC_OCR_TEST_PNG") else {
            eprintln!("SKIP winocr_keeps_columns: TRC_OCR_TEST_PNG not set");
            return;
        };
        let cols = std::path::Path::new(&sample)
            .parent()
            .unwrap()
            .join("ab7")
            .join("columns.png");
        if !cols.exists() {
            eprintln!("SKIP winocr_keeps_columns: no columns.png next to fixture");
            return;
        }
        let png = std::fs::read(&cols).expect("read columns fixture");
        let text = recognize_png_blocking(&png, "en").expect("win ocr failed");
        eprintln!("COLS>>\n{text}");
        let row_with = |needle: &str| {
            text.lines()
                .find(|l| l.contains(needle))
                .unwrap_or("")
                .to_string()
        };
        let code_row = row_with("Run code");
        assert!(
            code_row.trim_start().starts_with("code"),
            "колонки схлопнулись:\n{text}"
        );
        let ai_row = row_with("Show info");
        assert!(ai_row.contains("ai"), "описание оторвалось:\n{text}");
        let tr_row = row_with("Translates text");
        assert!(
            tr_row.trim_start().starts_with("translate"),
            "колонки схлопнулись:\n{text}"
        );
    }

    #[test]
    fn update_tag_and_version_compare() {
        assert_eq!(strip_tag("TRC-v1.0.0"), "1.0.0");
        assert_eq!(strip_tag("TRC_v2.3"), "2.3");
        assert_eq!(strip_tag("v1.2.3"), "1.2.3");
        assert!(is_newer_version("1.0.1", "1.0.0"));
        assert!(is_newer_version("1.1.0", "1.0.9"));
        assert!(!is_newer_version("1.0.0", "1.0.0"));
        assert!(!is_newer_version("0.9.9", "1.0.0"));
    }
}
