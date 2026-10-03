//! Глобальный хук кнопок мыши (WH_MOUSE_LL) для биндов вида MouseX1/MouseX2/MouseMiddle.
//! Клавиатурные хоткеи обрабатывает tauri-plugin-global-shortcut, мышиные он не умеет.
//! Хук только СЛУШАЕТ (клики всегда пропускаем дальше через CallNextHookEx).

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

use tauri::{AppHandle, Emitter};
use windows_sys::Win32::Foundation::{LRESULT, WPARAM, LPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

// 0 — выкл, 1 — X1 (боковая назад), 2 — X2 (боковая вперёд), 3 — средняя.
static OVERLAY_BTN: AtomicU8 = AtomicU8::new(0);
static CAPTURE_BTN: AtomicU8 = AtomicU8::new(0);
static APP: OnceLock<AppHandle> = OnceLock::new();

fn parse_mouse_button(h: &str) -> u8 {
    match h.trim().to_lowercase().as_str() {
        "mousex1" => 1,
        "mousex2" => 2,
        "mousemiddle" | "mousemid" => 3,
        _ => 0,
    }
}

/// Вызывается из save_settings / set_hotkeys и при старте.
pub fn configure(overlay_hotkey: &str, capture_hotkey: &str) {
    OVERLAY_BTN.store(parse_mouse_button(overlay_hotkey), Ordering::Relaxed);
    CAPTURE_BTN.store(parse_mouse_button(capture_hotkey), Ordering::Relaxed);
}

unsafe extern "system" fn mouse_proc(ncode: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if ncode == HC_ACTION as i32 {
        // В WH_MOUSE_LL lParam — указатель на MSLLHOOKSTRUCT,
        // X-кнопка — в старшем слове mouseData (не в самом lParam!).
        #[repr(C)]
        struct MsllHook {
            _x: i32,
            _y: i32,
            mouse_data: u32,
            _flags: u32,
            _time: u32,
            _extra: usize,
        }
        let pressed: Option<u8> = match wparam as u32 {
            WM_XBUTTONDOWN => {
                let x = ((&*(lparam as *const MsllHook)).mouse_data >> 16) as u16;
                if x == XBUTTON1 as u16 {
                    Some(1)
                } else if x == XBUTTON2 as u16 {
                    Some(2)
                } else {
                    None
                }
            }
            WM_MBUTTONDOWN => Some(3),
            _ => None,
        };
        if let Some(b) = pressed {
            if let Some(app) = APP.get() {
                if OVERLAY_BTN.load(Ordering::Relaxed) == b {
                    let _ = app.emit("trc:mouse-hotkey", "overlay");
                }
                if CAPTURE_BTN.load(Ordering::Relaxed) == b {
                    let _ = app.emit("trc:mouse-hotkey", "capture");
                }
            }
        }
    }
    CallNextHookEx(std::ptr::null_mut(), ncode, wparam, lparam)
}

/// Ставится один раз при старте. Поток с message loop — требование WH_MOUSE_LL.
pub fn install(app: AppHandle) {
    let _ = APP.set(app);
    std::thread::spawn(|| unsafe {
        let hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), std::ptr::null_mut(), 0);
        if hook.is_null() {
            return;
        }
        let mut msg = std::mem::zeroed::<MSG>();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) != 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}
