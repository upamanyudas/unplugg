use std::cell::RefCell;
use std::ptr::{null, null_mut};
use std::sync::{Arc, RwLock};
use std::time::Duration;

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
const UNLOCK: u32 = WM_APP + 4;
// timers: the first update check once launch has settled, then daily; the updater's answers
const UPDATES: usize = 1;
const UPDATER: usize = 2;
const DAY_MS: u32 = 24 * 60 * 60 * 1000;
const CLASS: &str = "unplugg-tray-window";

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

struct State {
    config: Map<String, Value>,
    alarm_config: Arc<RwLock<Map<String, Value>>>,
    monitor: Monitor<WinSensors>,
    monitor_config: Arc<RwLock<MonitorConfig>>,
    icon: NOTIFYICONDATAW,
    menu_owner: HWND,
    siren: Option<super::siren::Siren>,
    tracking: Option<(super::location::Source, crate::live::Tracker)>,
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
            let owner = STATE.with(|s| s.borrow().as_ref().map(|state| state.menu_owner));
            if let Some(owner) = owner {
                menu(owner);
            }
            0
        }
        UNLOCK => {
            unlock(hwnd);
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
                        if event.1["live_location"] == true && state.tracking.is_none() {
                            let tracker =
                                crate::live::Tracker::start(&Value::Object(event.1.clone()));
                            let updates = tracker.clone();
                            let source =
                                super::location::Source::start(move |fix| updates.update(fix));
                            state.tracking = Some((source, tracker));
                        }
                        if event.1["play_sound"] == true && state.siren.is_none() {
                            state.siren = Some(super::siren::Siren::start(
                                event.1["speaker_name"].as_str().unwrap_or(""),
                            ));
                        }
                        if event.1["show_overlay"] == true || event.1["flash_screen"] == true {
                            super::overlay::show(
                                hwnd,
                                UNLOCK,
                                true,
                                event.1["flash_screen"] == true,
                            );
                        }
                    }
                }
            });
            0
        }
        WM_TIMER if w == UPDATES => {
            if config()["check_updates"] == true {
                super::updater::check(false);
            }
            SetTimer(hwnd, UPDATES, DAY_MS, None);
            0
        }
        WM_TIMER if w == UPDATER => {
            // dialogs get no owner: this window is message-only
            if super::updater::poll(null_mut()) {
                quit(hwnd);
            }
            0
        }
        WM_COMMAND => {
            match w & 0xffff {
                1 => toggle(hwnd),
                2 => preferences(),
                3 => super::guide::about(),
                4 => updates(),
                5 => guide(),
                6 => super::guide::help(),
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
                let cover = super::overlay::window();
                if !cover.is_null() {
                    ShutdownBlockReasonCreate(cover, wide("unplugg is armed").as_ptr());
                }
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
        deactivate();
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
    if STATE.with(|s| {
        s.borrow().as_ref().is_some_and(|state| {
            state.config["send_location"] == true || state.config["live_location"] == true
        })
    }) {
        let _ = windows::Devices::Geolocation::Geolocator::RequestAccessAsync();
    }
    STATE.with(|s| {
        if let Some(state) = s.borrow_mut().as_mut() {
            state.config["activated"] = true.into();
            super::power::hold(&mut state.config);
            *state.alarm_config.write().unwrap() = state.config.clone();
            state.monitor.start();
            set_icon(state, "armed");
            if state.config["show_overlay"] == true {
                super::overlay::show(hwnd, UNLOCK, false, false);
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
            state.siren = None;
            stop_tracking(state, "unplugg quit", Duration::from_secs(5));
            super::power::release(&mut state.config);
            super::overlay::hide();
            Shell_NotifyIconW(NIM_DELETE, &state.icon);
            DestroyIcon(state.icon.hIcon);
        }
    });
    DestroyWindow(hwnd);
}
fn preferences() {
    super::ui::open_settings();
}
fn guide() { super::guide::open(); }
fn updates() {
    super::updater::check(true);
}
fn alarm(reason: String, config: Map<String, Value>) {
    let config = Value::Object(config);
    if config["lock_device"] == true {
        super::utils::lock_screen();
    }
    let media = if config["capture_media"] == true {
        super::capture::capture_full(&config)
    } else {
        super::capture::Captured {
            photo: None,
            video: None,
            document: None,
        }
    };
    let link = if config["send_location"] == true {
        super::location::maps_link(Duration::from_secs(8))
    } else {
        None
    };
    crate::telegram::send_alert_with_document(
        &reason,
        &config,
        media.photo.as_deref(),
        media.video.as_deref(),
        link.as_deref(),
        media.document.as_deref(),
    );
}

fn stop_tracking(state: &mut State, reason: &'static str, wait: Duration) {
    if let Some((source, tracker)) = state.tracking.take() {
        source.stop();
        tracker.stop(reason, wait);
    }
}
unsafe fn deactivate() {
    STATE.with(|s| {
        if let Some(state) = s.borrow_mut().as_mut() {
            state.config["activated"] = false.into();
            state.monitor.stop();
            stop_tracking(state, "the alarm was turned off", Duration::ZERO);
            state.siren = None;
            super::power::release(&mut state.config);
            super::overlay::hide();
            set_icon(state, "idle");
            let _ = crate::config::save_config(&state.config);
        }
    });
}

unsafe fn unlock(hwnd: HWND) {
    if !super::auth::require(
        "unlock the screen",
        windows::Win32::Foundation::HWND(super::overlay::window()),
    ) {
        super::overlay::listen();
        return;
    }
    super::overlay::hide();
    STATE.with(|s| {
        if let Some(state) = s.borrow_mut().as_mut() {
            state.siren = None;
        }
    });
    if super::overlay::confirm(
        hwnd,
        "Deactivate the alarm?",
        "Keep Armed hides this cover; the alarm keeps watching.",
        "Deactivate",
        "Keep Armed",
    ) {
        deactivate();
    }
}

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
        if !super::sensors::is_laptop()
            && super::overlay::confirm(
                null_mut(),
                "unplugg is made for laptops",
                "It guards the power cable and the lid. This computer has no battery, so the alarm can't be activated here.",
                "Quit",
                "Open Anyway",
            )
        {
            DestroyWindow(hwnd);
            CloseHandle(mutex);
            return;
        }
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
                let config = snapshot.read().unwrap().clone();
                let event_reason = reason.to_owned();
                let alert_config = config.clone();
                let alert_reason = event_reason.clone();
                std::thread::spawn(move || alarm(alert_reason, alert_config));
                let event = Box::into_raw(Box::new((event_reason, config)));
                if PostMessageW(target as HWND, ALARM, 0, event as LPARAM) == 0 {
                    drop(Box::from_raw(event));
                }
            },
            Arc::clone(&monitor_config),
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
        STATE.with(|s| {
            *s.borrow_mut() = Some(State {
                config,
                alarm_config,
                monitor,
                monitor_config,
                icon: tray,
                menu_owner: power_hwnd,
                siren: None,
                tracking: None,
            })
        });
        if self::config()["setup_done"] != true {
            guide();
        }
        SetTimer(hwnd, UPDATES, 10_000, None);
        SetTimer(hwnd, UPDATER, 1_000, None);
        let notification = RegisterPowerSettingNotification(
            power_hwnd,
            &GUID_LIDSWITCH_STATE_CHANGE,
            DEVICE_NOTIFY_WINDOW_HANDLE,
        );
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            if !super::guide::dialog_message(&msg) && !super::ui::dialog_message(&msg) {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        if notification != 0 {
            UnregisterPowerSettingNotification(notification);
        }
        CloseHandle(mutex);
    }
}

pub fn config() -> Map<String, Value> {
    STATE.with(|s| {
        s.borrow()
            .as_ref()
            .map(|state| state.config.clone())
            .unwrap_or_default()
    })
}

pub fn update_config(key: &str, value: Value) {
    STATE.with(|s| {
        if let Some(state) = s.borrow_mut().as_mut() {
            state.config.insert(key.into(), value);
            *state.alarm_config.write().unwrap() = state.config.clone();
            *state.monitor_config.write().unwrap() = MonitorConfig {
                monitor_power: state.config["monitor_power"] == true,
                monitor_lid: state.config["monitor_lid"] == true,
                monitor_motion: state.config["monitor_motion"] == true,
                angle_threshold: state.config["angle_threshold"].as_f64().unwrap_or(5.0),
            };
            let _ = crate::config::save_config(&state.config);
        }
    });
}
