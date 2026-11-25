use std::cell::RefCell;
use std::ptr::{null, null_mut};
use std::sync::{Arc, RwLock};

use serde_json::{Map, Value};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Power::{
    RegisterPowerSettingNotification, UnregisterPowerSettingNotification, POWERBROADCAST_SETTING,
};
use windows_sys::Win32::System::Shutdown::ShutdownBlockReasonCreate;
use windows_sys::Win32::System::SystemServices::GUID_LIDSWITCH_STATE_CHANGE;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use super::sensors::WinSensors;
use crate::monitor::{Monitor, MonitorConfig, Sensors};

const TRAY: u32 = WM_APP + 1;
const ALARM: u32 = WM_APP + 2;
const SETTINGS: u32 = WM_APP + 3;
const CLASS: &str = "unplugg-tray-window";

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

struct State {
    config: Map<String, Value>,
    alarm_config: Arc<RwLock<Map<String, Value>>>,
    monitor: Monitor<WinSensors>,
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
            let event = Box::from_raw(l as *mut (String, Map<String, Value>));
            STATE.with(|s| {
                if let Some(state) = s.borrow_mut().as_mut() {
                    if state.config["activated"] == true {
                        set_icon(state, "alarm");
                    }
                    alarm(&event.0, &event.1);
                }
            });
            0
        }
        WM_COMMAND => {
            match w & 0xffff {
                1 => toggle(hwnd),
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
        WM_POWERBROADCAST if w as u32 == PBT_POWERSETTINGCHANGE && l != 0 => {
            let setting = &*(l as *const POWERBROADCAST_SETTING);
            if setting.PowerSetting.data1 == GUID_LIDSWITCH_STATE_CHANGE.data1
                && setting.PowerSetting.data2 == GUID_LIDSWITCH_STATE_CHANGE.data2
                && setting.PowerSetting.data3 == GUID_LIDSWITCH_STATE_CHANGE.data3
                && setting.PowerSetting.data4 == GUID_LIDSWITCH_STATE_CHANGE.data4
                && setting.DataLength >= 4
            {
                let value = std::ptr::read_unaligned(setting.Data.as_ptr().cast::<u32>());
                super::sensors::set_lid_closed(value == 0);
            }
            1
        }
        WM_POWERBROADCAST if w as u32 == PBT_APMSUSPEND => {
            STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    state.monitor.trip("shutdown");
                }
            });
            1
        }
        WM_QUERYENDSESSION => {
            let armed = STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    if state.config["activated"] == true {
                        state.monitor.trip("shutdown");
                        return true;
                    }
                    false
                } else {
                    false
                }
            });
            if armed {
                ShutdownBlockReasonCreate(hwnd, wide("unplugg is armed").as_ptr());
                0
            } else {
                1
            }
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}

unsafe fn toggle(hwnd: HWND) {
    let armed = STATE.with(|s| {
        s.borrow()
            .as_ref()
            .is_some_and(|s| s.config["activated"] == true)
    });
    if armed {
        if !super::auth::require("turn off the alarm", windows::Win32::Foundation::HWND(hwnd)) {
            return;
        }
        STATE.with(|s| {
            if let Some(state) = s.borrow_mut().as_mut() {
                state.config["activated"] = false.into();
                state.monitor.stop();
                super::power::release(&mut state.config);
                set_icon(state, "idle");
                let _ = crate::config::save_config(&state.config);
            }
        });
        return;
    }
    if !super::sensors::is_laptop() {
        MessageBoxW(hwnd, wide("It guards the power cable and the lid. This computer has no battery, so the alarm can't be activated here.").as_ptr(), wide("unplugg is made for laptops").as_ptr(), MB_OK);
        return;
    }
    let monitor_power = STATE.with(|s| {
        s.borrow()
            .as_ref()
            .is_some_and(|s| s.config["monitor_power"] == true)
    });
    if monitor_power && !WinSensors.is_plugged_in() {
        MessageBoxW(hwnd, wide("Your laptop should be plugged in to power before you can activate the alarm. Alternatively, turn off \"Power unplugged\" in Settings.").as_ptr(), wide("Plug in first").as_ptr(), MB_OK);
        return;
    }
    STATE.with(|s| {
        if let Some(state) = s.borrow_mut().as_mut() {
            state.config["activated"] = true.into();
            super::power::hold(&mut state.config);
            *state.alarm_config.write().unwrap() = state.config.clone();
            state.monitor.start();
            set_icon(state, "armed");
            if state.config["show_overlay"] == true {
                show_overlay();
            }
            let _ = crate::config::save_config(&state.config);
        }
    });
}
unsafe fn quit(hwnd: HWND) {
    let armed = STATE.with(|s| {
        s.borrow()
            .as_ref()
            .is_some_and(|s| s.config["activated"] == true)
    });
    if armed && !super::auth::require("quit while armed", windows::Win32::Foundation::HWND(hwnd)) {
        return;
    }
    STATE.with(|s| {
        if let Some(state) = s.borrow_mut().as_mut() {
            state.monitor.stop();
            super::power::release(&mut state.config);
            hide_overlay();
            stop_alarm();
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
fn alarm(_reason: &str, _config: &Map<String, Value>) {}
fn stop_alarm() {}
fn show_overlay() {}
fn hide_overlay() {}

pub fn run() {
    unsafe {
        let _ = windows::Win32::System::WinRT::RoInitialize(
            windows::Win32::System::WinRT::RO_INIT_SINGLETHREADED,
        );
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
        super::power::release(&mut config);
        let monitor_config = Arc::new(RwLock::new(MonitorConfig {
            monitor_power: config["monitor_power"] == true,
            monitor_lid: config["monitor_lid"] == true,
            monitor_motion: config["monitor_motion"] == true,
            angle_threshold: config["angle_threshold"].as_f64().unwrap_or(5.0),
        }));
        let target = hwnd as isize;
        let alarm_config = Arc::new(RwLock::new(config.clone()));
        let snapshot = Arc::clone(&alarm_config);
        let monitor = Monitor::new(
            move |reason| {
                let event = Box::into_raw(Box::new((
                    reason.to_owned(),
                    snapshot.read().unwrap().clone(),
                )));
                if PostMessageW(target as HWND, ALARM, 0, event as LPARAM) == 0 {
                    drop(Box::from_raw(event));
                }
            },
            monitor_config,
            WinSensors,
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
                alarm_config,
                monitor,
                icon: tray,
            })
        });
        let power_hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        let notification = RegisterPowerSettingNotification(
            power_hwnd,
            &GUID_LIDSWITCH_STATE_CHANGE,
            DEVICE_NOTIFY_WINDOW_HANDLE,
        );
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        if notification != 0 {
            UnregisterPowerSettingNotification(notification);
        }
        CloseHandle(mutex);
    }
}
