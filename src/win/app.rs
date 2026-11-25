use std::cell::RefCell;
use std::ptr::{null, null_mut};
use std::sync::{Arc, RwLock};

use serde_json::{Map, Value};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::monitor::{Monitor, MonitorConfig, Sensors};

const TRAY: u32 = WM_APP + 1;
const ALARM: u32 = WM_APP + 2;
const SETTINGS: u32 = WM_APP + 3;
const CLASS: &str = "unplugg-tray-window";

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

struct PendingSensors;
impl Sensors for PendingSensors {
    fn is_plugged_in(&self) -> bool {
        true
    }
    fn is_lid_closed(&self) -> bool {
        false
    }
    fn lid_angle(&self) -> Option<f64> {
        None
    }
    fn acceleration(&self) -> Option<[f64; 3]> {
        None
    }
}

struct State {
    config: Map<String, Value>,
    monitor: Monitor<PendingSensors>,
    icon: NOTIFYICONDATAW,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

unsafe fn icon(state: &str) -> HICON {
    let bytes: &[u8] = match (state, super::utils::light_taskbar()) {
        ("armed", true) => include_bytes!("../../assets/win/armed_light.png"),
        ("armed", false) => include_bytes!("../../assets/win/armed_dark.png"),
        ("alarm", _) => include_bytes!("../../assets/win/alarm_dark.png"),
        (_, true) => include_bytes!("../../assets/win/idle_light.png"),
        (_, false) => include_bytes!("../../assets/win/idle_dark.png"),
    };
    CreateIconFromResourceEx(bytes.as_ptr(), bytes.len() as u32, 1, 0x00030000, 16, 16, 0)
}

unsafe fn set_icon(state: &mut State, name: &str) {
    let old = state.icon.hIcon;
    state.icon.hIcon = icon(name);
    state.icon.uFlags = NIF_ICON;
    Shell_NotifyIconW(NIM_MODIFY, &state.icon);
    if !old.is_null() {
        DestroyIcon(old);
    }
}

unsafe fn menu(hwnd: HWND) {
    let menu = CreatePopupMenu();
    let armed = STATE.with(|s| {
        s.borrow()
            .as_ref()
            .is_some_and(|s| s.config["activated"] == true)
    });
    for (id, title) in [
        (1, if armed { "Deactivate" } else { "Activate" }),
        (0, ""),
        (2, "Settings…"),
        (0, ""),
        (3, "About…"),
        (4, "Check for Updates…"),
        (5, "Setup Guide…"),
        (6, "Help…"),
        (7, "Quit"),
    ] {
        if id == 0 {
            AppendMenuW(menu, MF_SEPARATOR, 0, null());
        } else {
            AppendMenuW(menu, MF_STRING, id, wide(title).as_ptr());
        }
    }
    let mut point = POINT::default();
    GetCursorPos(&mut point);
    SetForegroundWindow(hwnd);
    TrackPopupMenu(menu, TPM_RIGHTBUTTON, point.x, point.y, 0, hwnd, null());
    DestroyMenu(menu);
}

unsafe extern "system" fn window(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        TRAY if l as u32 == WM_LBUTTONUP || l as u32 == WM_RBUTTONUP => {
            menu(hwnd);
            0
        }
        SETTINGS => {
            preferences();
            0
        }
        ALARM => {
            STATE.with(|s| {
                if let Some(state) = s.borrow_mut().as_mut() {
                    set_icon(state, "alarm");
                }
            });
            alarm();
            0
        }
        WM_COMMAND => {
            match w & 0xffff {
                1 => toggle(),
                2 => preferences(),
                3 => dialogs(),
                4 => updates(),
                5 => guide(),
                6 => dialogs(),
                7 => quit(hwnd),
                _ => (),
            }
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}

unsafe fn toggle() {
    // Activation is enabled once the system sensors and owner check are connected.
    MessageBoxW(
        null_mut(),
        wide("Windows monitoring is not ready yet.").as_ptr(),
        wide("unplugg").as_ptr(),
        MB_OK,
    );
}
unsafe fn quit(hwnd: HWND) {
    STATE.with(|s| {
        if let Some(state) = s.borrow_mut().as_mut() {
            state.monitor.stop();
            Shell_NotifyIconW(NIM_DELETE, &state.icon);
            DestroyIcon(state.icon.hIcon);
        }
    });
    DestroyWindow(hwnd);
}
fn preferences() {}
fn guide() {}
fn dialogs() {}
fn updates() {}
fn alarm() {}

pub fn run() {
    unsafe {
        let mutex = CreateMutexW(null(), 0, wide("Local\\unplugg-rust-instance").as_ptr());
        if mutex.is_null() {
            return;
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let hwnd = FindWindowExW(HWND_MESSAGE, null_mut(), wide(CLASS).as_ptr(), null());
            if !hwnd.is_null() {
                PostMessageW(hwnd, SETTINGS, 0, 0);
            }
            CloseHandle(mutex);
            return;
        }
        let instance = GetModuleHandleW(null());
        let class = wide(CLASS);
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..Default::default()
        };
        if RegisterClassW(&wc) == 0 {
            CloseHandle(mutex);
            return;
        }
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() {
            CloseHandle(mutex);
            return;
        }
        let mut config = crate::config::load_config().unwrap_or_else(|_| crate::config::defaults());
        config["activated"] = false.into();
        let monitor_config = Arc::new(RwLock::new(MonitorConfig {
            monitor_power: true,
            monitor_lid: true,
            monitor_motion: true,
            angle_threshold: 5.0,
        }));
        let target = hwnd as isize;
        let monitor = Monitor::new(
            move |_| {
                PostMessageW(target as HWND, ALARM, 0, 0);
            },
            monitor_config,
            PendingSensors,
        );
        let mut tray = NOTIFYICONDATAW::default();
        tray.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        tray.hWnd = hwnd;
        tray.uID = 1;
        tray.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        tray.uCallbackMessage = TRAY;
        tray.hIcon = icon("idle");
        tray.szTip[..8].copy_from_slice(&wide("unplugg"));
        Shell_NotifyIconW(NIM_ADD, &tray);
        STATE.with(|s| {
            *s.borrow_mut() = Some(State {
                config,
                monitor,
                icon: tray,
            })
        });
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        CloseHandle(mutex);
    }
}
