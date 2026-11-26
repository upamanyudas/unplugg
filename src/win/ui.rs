use std::cell::RefCell;
use std::ptr::{null, null_mut};
use std::sync::{Arc, Mutex};

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::{
    CreateFontIndirectW, DeleteObject, GetSysColorBrush, MapWindowPoints, SetBkMode, SetTextColor,
    COLOR_BTNFACE, COLOR_WINDOW, HBRUSH, HDC, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::{
    InitCommonControlsEx, BST_CHECKED, BST_UNCHECKED, EM_SETCUEBANNER, ICC_STANDARD_CLASSES,
    ICC_TAB_CLASSES, INITCOMMONCONTROLSEX, NMHDR, TCIF_TEXT, TCITEMW, TCM_GETCURSEL,
    TCM_INSERTITEMW, TCM_SETCURSEL, TCN_SELCHANGE,
};
use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, SystemParametersInfoForDpi};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow;

use windows_sys::Win32::UI::WindowsAndMessaging::*;

use super::{app, devices, loginitem, permissions, sensors, utils};
use crate::telegram;

const CLASS: &str = "unplugg-settings";
const TELEGRAM_CLASS: &str = "unplugg-telegram";
const CHECKS: &[(&str, &str, usize)] = &[
    ("monitor_power", "Power unplugged", 0),
    ("monitor_lid", "Lid closed", 0),
    ("monitor_motion", "Moved (motion sensor)", 0),
    ("show_overlay", "Show overlay", 0),
    ("lock_device", "Lock when triggered", 0),
    ("launch_at_login", "Launch at login", 0),
    ("check_updates", "Check for updates automatically", 0),
    ("auto_update", "Install updates automatically", 0),
    ("play_sound", "Play siren at full volume", 1),
    ("capture_media", "Send photo && video", 1),
    ("flash_screen", "Flash screen for photos", 1),
    ("record_audio", "Record audio with video", 1),
    ("send_location", "Send location", 1),
    ("live_location", "Send live location", 1),
];
const PERMS: [(&str, &str, &str); 3] = [
    ("camera", "Camera", "Photos and video of a thief."),
    ("microphone", "Microphone", "Sound in that video."),
    ("location", "Location", "A map link to your laptop."),
];

#[derive(Default)]
struct Ui {
    hwnd: HWND,
    controls: Vec<HWND>,
    choices: Vec<Vec<String>>,
    status: Vec<(HWND, COLORREF)>,
    page: usize,
}
#[derive(Default)]
struct TelegramUi {
    hwnd: HWND,
    result: Option<Arc<Mutex<Option<Result<String, String>>>>>,
}
thread_local! {
    static UI: RefCell<Ui> = RefCell::new(Ui::default());
    static TELEGRAM: RefCell<TelegramUi> = RefCell::new(TelegramUi::default());
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

pub(super) unsafe fn control(
    parent: HWND,
    class: &str,
    text: &str,
    style: u32,
    id: usize,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) -> HWND {
    let dpi = GetDpiForWindow(parent).max(96) as i32;
    let scale = |n: i32| n * dpi / 96;
    let child = CreateWindowExW(
        0,
        wide(class).as_ptr(),
        wide(text).as_ptr(),
        WS_CHILD | WS_VISIBLE | style,
        scale(x),
        scale(y),
        scale(w),
        scale(h),
        parent,
        id as HMENU,
        GetModuleHandleW(null()),
        null(),
    );
    let font = GetPropW(parent, wide("unplugg-font").as_ptr());
    if !font.is_null() {
        SendMessageW(child, WM_SETFONT, font as WPARAM, 1);
    }
    child
}

const TABS: [&str; 4] = ["General", "Alerts", "Devices", "Permissions"];
const WIDTH: i32 = 436; // group width inside the 460 px client area
const ROW: i32 = 26;
const ALLOWED: COLORREF = 0x0059c734; // #34c759
const BLOCKED: COLORREF = 0x00303bff; // #ff3b30

/// A titled frame for `rows` rows. Returns where its first row goes, and moves `y` past it.
unsafe fn group(hwnd: HWND, controls: &mut Vec<HWND>, y: &mut i32, title: &str, rows: i32, row: i32) -> i32 {
    let height = 20 + rows * row + 6;
    controls.push(control(hwnd, "BUTTON", title, BS_GROUPBOX as u32, 0, 12, *y, WIDTH, height));
    let first = *y + 20;
    *y += height + 8;
    first
}

unsafe fn check(hwnd: HWND, controls: &mut Vec<HWND>, config: &serde_json::Map<String, serde_json::Value>, key: &str, y: i32) {
    let i = CHECKS.iter().position(|(k, _, _)| *k == key).unwrap();
    let child = control(hwnd, "BUTTON", CHECKS[i].1, WS_TABSTOP | BS_AUTOCHECKBOX as u32, 100 + i, 24, y, WIDTH - 24, 24);
    let state = if config[key] == true { BST_CHECKED } else { BST_UNCHECKED };
    SendMessageW(child, BM_SETCHECK, state as usize, 0);
    controls.push(child);
}

unsafe fn page(hwnd: HWND, index: usize) {
    let old = UI.with(|ui| {
        let mut ui = ui.borrow_mut();
        ui.page = index;
        ui.choices.clear();
        ui.status.clear();
        std::mem::take(&mut ui.controls)
    });
    for child in old {
        DestroyWindow(child);
    }
    let mut controls = Vec::new();
    let mut choices = Vec::new();
    let mut status = Vec::new();
    let config = app::config();
    let mut y = 44;
    let checks = |keys: &[&str]| -> Vec<String> {
        keys.iter().filter(|k| **k != "monitor_motion" || sensors::acceleration().is_some()).map(|k| k.to_string()).collect()
    };
    let boxes = |controls: &mut Vec<HWND>, y: &mut i32, title: &str, keys: Vec<String>| {
        let mut row = group(hwnd, controls, y, title, keys.len() as i32, ROW);
        for key in keys {
            check(hwnd, controls, &config, &key, row);
            row += ROW;
        }
    };
    match index {
        0 => {
            boxes(&mut controls, &mut y, "Alarm when", checks(&["monitor_power", "monitor_lid", "monitor_motion"]));
            boxes(&mut controls, &mut y, "While armed", checks(&["show_overlay", "lock_device"]));
            boxes(&mut controls, &mut y, "", checks(&["launch_at_login"]));
            boxes(&mut controls, &mut y, "Updates", checks(&["check_updates", "auto_update"]));
        }
        1 => {
            let keys = ["play_sound", "capture_media", "flash_screen", "record_audio", "send_location", "live_location"];
            boxes(&mut controls, &mut y, "When triggered", checks(&keys));
            let row = group(hwnd, &mut controls, &mut y, "Telegram", 1, 30);
            controls.push(control(hwnd, "STATIC", "Destinations", 0, 0, 24, row + 5, 100, 20));
            let combo = control(hwnd, "COMBOBOX", "", WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWNLIST as u32, 400, 128, row, 150, 150);
            let names: Vec<&str> = config["telegram_destinations"].as_array().into_iter().flatten().filter_map(|d| d["name"].as_str()).collect();
            for name in if names.is_empty() { vec!["None yet"] } else { names.clone() } {
                SendMessageW(combo, CB_ADDSTRING, 0, wide(name).as_ptr() as LPARAM);
            }
            SendMessageW(combo, CB_SETCURSEL, 0, 0);
            controls.push(combo);
            for (id, title, x, width) in [(401, "+", 284, 28), (402, "−", 316, 28), (403, "Send Test", 348, 80)] {
                let button = control(hwnd, "BUTTON", title, WS_TABSTOP, id, x, row, width, 26);
                if id != 401 {
                    EnableWindow(button, !names.is_empty() as i32);
                }
                controls.push(button);
            }
            EnableWindow(combo, !names.is_empty() as i32);
            let row = group(hwnd, &mut controls, &mut y, "Saved media", 1, 30);
            let folder = config["media_dir"].as_str().unwrap_or("").replacen(&crate::config::home().to_string_lossy().to_string(), "~", 1);
            controls.push(control(hwnd, "STATIC", &folder, windows_sys::Win32::System::SystemServices::SS_PATHELLIPSIS, 0, 24, row + 5, 230, 20));
            controls.push(control(hwnd, "BUTTON", "Choose…", WS_TABSTOP, 404, 264, row, 80, 26));
            controls.push(control(hwnd, "BUTTON", "Open", WS_TABSTOP, 405, 348, row, 80, 26));
        }
        2 => {
            let mics = devices::microphones();
            let speakers = devices::speakers();
            let current = |key: &str| config[key].as_str().unwrap_or("").to_owned();
            // the default device is what's selected when none is chosen
            let mic = Some(current("mic")).filter(|m| mics.contains(m)).or_else(devices::default_microphone_name);
            let pickers = [
                ("Camera", devices::cameras(), devices::camera(&current("camera")), "Default camera"),
                ("Microphone", mics, mic, "Default microphone"),
                ("Speaker", [vec!["System default".to_owned()], speakers].concat(), Some(current("speaker_name")), ""),
            ];
            let mut row = 0;
            for (i, (label, mut items, selected, fallback)) in pickers.into_iter().enumerate() {
                if i == 0 {
                    row = group(hwnd, &mut controls, &mut y, "Capture", 2, 34);
                } else if i == 2 {
                    row = group(hwnd, &mut controls, &mut y, "Siren", 1, 34);
                }
                if items.is_empty() {
                    items.push(fallback.into());
                }
                controls.push(control(hwnd, "STATIC", label, 0, 0, 24, row + 5, 96, 20));
                let child = control(hwnd, "COMBOBOX", "", WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWNLIST as u32, 200 + i, 124, row, 300, 180);
                for item in &items {
                    SendMessageW(child, CB_ADDSTRING, 0, wide(item).as_ptr() as LPARAM);
                }
                let at = selected.and_then(|s| items.iter().position(|item| *item == s)).unwrap_or(0);
                SendMessageW(child, CB_SETCURSEL, at, 0);
                controls.push(child);
                choices.push(items);
                row += 34;
            }
        }
        _ => {
            let mut row = group(hwnd, &mut controls, &mut y, "All optional", 3, 40);
            for (i, (key, label, why)) in PERMS.iter().enumerate() {
                controls.push(control(hwnd, "STATIC", &format!("{label}\n{why}"), 0, 0, 24, row + 2, 230, 34));
                let granted = permissions::status(key) == "granted";
                let (text, color) = if granted { ("● Allowed", ALLOWED) } else { ("● Blocked", BLOCKED) };
                let label = control(hwnd, "STATIC", text, 0, 0, 258, row + 7, 80, 20);
                status.push((label, color));
                controls.push(label);
                if !granted {
                    controls.push(control(hwnd, "BUTTON", "Open Settings…", WS_TABSTOP, 300 + i, 340, row + 2, 90, 26));
                }
                row += 40;
            }
        }
    }
    UI.with(|ui| {
        let mut ui = ui.borrow_mut();
        ui.controls = controls;
        ui.choices = choices;
        ui.status = status;
    });
}

unsafe extern "system" fn window(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match message {
        WM_DPICHANGED => {
            dpi_changed(hwnd, w, l);
            0
        }
        WM_NOTIFY if (*(l as *const NMHDR)).code == TCN_SELCHANGE => {
            let tab = (*(l as *const NMHDR)).hwndFrom;
            page(hwnd, SendMessageW(tab, TCM_GETCURSEL, 0, 0) as usize);
            0
        }
        WM_CTLCOLORSTATIC => {
            let color = UI.with(|ui| ui.borrow().status.iter().find(|(label, _)| *label == l as HWND).map(|(_, c)| *c));
            if let Some(color) = color {
                SetTextColor(w as HDC, color);
            }
            SetBkMode(w as HDC, TRANSPARENT as i32);
            GetSysColorBrush(COLOR_BTNFACE) as LRESULT
        }
        WM_COMMAND => {
            let id = w & 0xffff;
            if (100..100 + CHECKS.len()).contains(&id) {
                let key = CHECKS[id - 100].0;
                let checked = SendMessageW(l as HWND, BM_GETCHECK, 0, 0) == BST_CHECKED as isize;
                app::update_config(key, checked.into());
                if key == "launch_at_login" {
                    let _ = if checked {
                        loginitem::enable()
                    } else {
                        loginitem::disable()
                    };
                }
            } else if (200..203).contains(&id) && (w >> 16) as u32 == CBN_SELCHANGE {
                let index = (id - 200) as usize;
                let selected = SendMessageW(l as HWND, CB_GETCURSEL, 0, 0) as usize;
                UI.with(|ui| {
                    if let Some(value) =
                        ui.borrow().choices.get(index).and_then(|v| v.get(selected))
                    {
                        let value = if index == 2 && selected == 0 {
                            ""
                        } else {
                            value
                        };
                        app::update_config(["camera", "mic", "speaker_name"][index], value.into());
                    }
                });
            } else if id == 401 {
                open_telegram(hwnd);
            } else if id == 402 || id == 403 {
                let selected = SendMessageW(GetDlgItem(hwnd, 400), CB_GETCURSEL, 0, 0) as usize;
                if id == 402 {
                    remove_destination(selected);
                    page(hwnd, 1);
                } else {
                    test_destination(hwnd, selected);
                }
            } else if id == 404 {
                if let Some(path) = choose_folder(hwnd) {
                    app::update_config("media_dir", path.into());
                    page(hwnd, 1);
                }
            } else if id == 405 {
                let config = app::config();
                let path = config["media_dir"].as_str().unwrap_or("");
                if !path.is_empty() {
                    // capture makes it lazily
                    let _ = std::fs::create_dir_all(path);
                    utils::open_url(path);
                }
            } else if (300..303).contains(&id) {
                permissions::open_settings(PERMS[id - 300].0);
            }
            0
        }
        // pick up changes made in Windows Settings
        WM_ACTIVATE if w as u16 != WA_INACTIVE as u16 => {
            let index = UI.with(|ui| ui.borrow().page);
            if index == 3 {
                page(hwnd, index);
            }
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            free_font(hwnd);
            UI.with(|ui| {
                let mut ui = ui.borrow_mut();
                app::update_config("settings_pane", ui.page.into());
                *ui = Ui::default();
            });
            0
        }
        _ => DefWindowProcW(hwnd, message, w, l),
    }
}

pub fn open_settings() {
    unsafe {
        let existing = UI.with(|ui| ui.borrow().hwnd);
        if !existing.is_null() {
            ShowWindow(existing, SW_SHOW);
            SetForegroundWindow(existing);
            return;
        }
        let tabs = INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_TAB_CLASSES | ICC_STANDARD_CLASSES,
        };
        InitCommonControlsEx(&tabs);
        let instance = GetModuleHandleW(null());
        let class = wide(CLASS);
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hIcon: LoadIconW(instance, 1 as _),
            // the dialog colour, so labels and frames blend in
            hbrBackground: (COLOR_BTNFACE + 1) as HBRUSH,
            ..Default::default()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            wide("unplugg Settings").as_ptr(),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            480,
            470,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() {
            return;
        }
        UI.with(|ui| ui.borrow_mut().hwnd = hwnd);
        initial_dpi(hwnd, 480, 470);
        let tab = control(hwnd, "SysTabControl32", "", WS_TABSTOP | WS_CLIPSIBLINGS, 10, 8, 8, 444, 28);
        for (i, title) in TABS.iter().enumerate() {
            let mut text = wide(title);
            let item = TCITEMW { mask: TCIF_TEXT, pszText: text.as_mut_ptr(), ..Default::default() };
            SendMessageW(tab, TCM_INSERTITEMW, i, &item as *const TCITEMW as LPARAM);
        }
        let pane = app::config()["settings_pane"].as_u64().unwrap_or(0).min(3) as usize;
        SendMessageW(tab, TCM_SETCURSEL, pane, 0);
        page(hwnd, pane);
        ShowWindow(hwnd, SW_SHOW);
    }
}

pub unsafe fn dialog_message(msg: &MSG) -> bool {
    let hwnd = UI.with(|ui| ui.borrow().hwnd);
    let telegram = TELEGRAM.with(|state| state.borrow().hwnd);
    (!telegram.is_null() && IsDialogMessageW(telegram, msg) != 0)
        || (!hwnd.is_null() && IsDialogMessageW(hwnd, msg) != 0)
}

unsafe fn alert(parent: HWND, title: &str, text: &str) {
    MessageBoxW(
        parent,
        wide(text).as_ptr(),
        wide(title).as_ptr(),
        MB_OK | MB_TOPMOST,
    );
}

unsafe fn field(hwnd: HWND, id: i32) -> String {
    let item = GetDlgItem(hwnd, id);
    let len = GetWindowTextLengthW(item).max(0) as usize;
    let mut buf = vec![0u16; len + 1];
    GetWindowTextW(item, buf.as_mut_ptr(), buf.len() as i32);
    String::from_utf16_lossy(&buf[..len]).trim().to_string()
}

unsafe extern "system" fn telegram_window(
    hwnd: HWND,
    message: u32,
    w: WPARAM,
    l: LPARAM,
) -> LRESULT {
    match message {
        WM_DPICHANGED => {
            dpi_changed(hwnd, w, l);
            0
        }
        WM_COMMAND => {
            match w & 0xffff {
                510 => utils::open_url(telegram::TOKEN_HELP.1),
                511 => {
                    let token = field(hwnd, 501);
                    if !token.is_empty() {
                        let result = Arc::new(Mutex::new(None));
                        TELEGRAM.with(|state| state.borrow_mut().result = Some(result.clone()));
                        refresh_telegram(hwnd);
                        SetWindowTextW(
                            GetDlgItem(hwnd, 506),
                            wide("Waiting… send your bot any message now.").as_ptr(),
                        );
                        std::thread::spawn(move || {
                            *result.lock().unwrap() = Some(telegram::find_chat_id(&token, 30));
                        });
                        SetTimer(hwnd, 1, 200, None);
                    }
                }
                512 => {
                    let result = telegram::send_test(&field(hwnd, 501), &field(hwnd, 502));
                    SetWindowTextW(
                        GetDlgItem(hwnd, 507),
                        wide(
                            &result
                                .err()
                                .unwrap_or_else(|| "Sent. Check Telegram.".into()),
                        )
                        .as_ptr(),
                    );
                }
                513 => alert(hwnd, "Make alerts loud on your phone", telegram::LOUD_HELP),
                514 => {
                    let name = field(hwnd, 500);
                    let token = field(hwnd, 501);
                    let chat = field(hwnd, 502);
                    if name.is_empty() || token.is_empty() || chat.is_empty() {
                        DestroyWindow(hwnd);
                        return 0;
                    }
                    match telegram::check(&token, &chat) {
                        Ok(()) => {
                            let mut destinations = app::config()["telegram_destinations"]
                                .as_array()
                                .cloned()
                                .unwrap_or_default();
                            destinations.push(
                                serde_json::json!({"name": name, "token": token, "chat_id": chat}),
                            );
                            app::update_config("telegram_destinations", destinations.into());
                            DestroyWindow(hwnd);
                            let settings = UI.with(|ui| ui.borrow().hwnd);
                            if !settings.is_null() {
                                page(settings, 1);
                            }
                        }
                        Err(err) => alert(hwnd, "Setup failed", &format!("Telegram says: {err}")),
                    }
                }
                515 => {
                    DestroyWindow(hwnd);
                }
                _ => (),
            }
            0
        }
        WM_TIMER => {
            let result = TELEGRAM.with(|state| {
                state
                    .borrow()
                    .result
                    .as_ref()
                    .and_then(|r| r.lock().unwrap().take())
            });
            if let Some(result) = result {
                KillTimer(hwnd, 1);
                let message = match result {
                    Ok(chat) => {
                        SetWindowTextW(GetDlgItem(hwnd, 502), wide(&chat).as_ptr());
                        "Found your chat.".into()
                    }
                    Err(err) => err,
                };
                SetWindowTextW(GetDlgItem(hwnd, 506), wide(&message).as_ptr());
                TELEGRAM.with(|state| state.borrow_mut().result = None);
                refresh_telegram(hwnd);
            }
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            free_font(hwnd);
            TELEGRAM.with(|state| *state.borrow_mut() = TelegramUi::default());
            0
        }
        _ => DefWindowProcW(hwnd, message, w, l),
    }
}

pub(super) unsafe fn open_telegram(owner: HWND) {
    let existing = TELEGRAM.with(|state| state.borrow().hwnd);
    if !existing.is_null() {
        SetForegroundWindow(existing);
        return;
    }
    let instance = GetModuleHandleW(null());
    let class = wide(TELEGRAM_CLASS);
    let wc = WNDCLASSW {
        lpfnWndProc: Some(telegram_window),
        hInstance: instance,
        lpszClassName: class.as_ptr(),
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        hbrBackground: (COLOR_WINDOW + 1) as HBRUSH,
        ..Default::default()
    };
    RegisterClassW(&wc);
    let hwnd = CreateWindowExW(
        0,
        class.as_ptr(),
        wide("Add a Telegram destination").as_ptr(),
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        500,
        380,
        owner,
        null_mut(),
        instance,
        null(),
    );
    if hwnd.is_null() {
        return;
    }
    TELEGRAM.with(|state| state.borrow_mut().hwnd = hwnd);
    initial_dpi(hwnd, 500, 380);
    control(
        hwnd,
        "STATIC",
        "Alerts (with photos and video) are sent here.",
        0,
        0,
        20,
        15,
        440,
        22,
    );
    for (id, label, y, style) in [
        (500, "Destination name", 45, 0),
        (501, "Telegram bot token", 80, ES_PASSWORD as u32),
        (502, "Chat id", 150, 0),
    ] {
        control(hwnd, "STATIC", label, 0, 0, 20, y + 3, 145, 22);
        control(
            hwnd,
            "EDIT",
            "",
            WS_TABSTOP | WS_BORDER | ES_AUTOHSCROLL as u32 | style,
            id,
            170,
            y,
            210,
            24,
        );
    }
    for (id, hint) in [
        (500, "e.g. My phone"),
        (501, "123456:ABC-DEF..."),
        (502, "e.g. 987654321"),
    ] {
        SendMessageW(
            GetDlgItem(hwnd, id),
            EM_SETCUEBANNER,
            0,
            wide(hint).as_ptr() as LPARAM,
        );
    }
    control(
        hwnd,
        "BUTTON",
        "How to get a bot token",
        WS_TABSTOP,
        510,
        170,
        112,
        180,
        25,
    );
    control(
        hwnd,
        "BUTTON",
        "Auto-detect",
        WS_TABSTOP,
        511,
        385,
        150,
        90,
        25,
    );
    control(
        hwnd,
        "STATIC",
        "Click Auto-detect, then send your bot any message.",
        0,
        506,
        170,
        180,
        300,
        22,
    );
    control(
        hwnd,
        "BUTTON",
        "Send Test Message",
        WS_TABSTOP,
        512,
        20,
        210,
        145,
        25,
    );
    control(hwnd, "STATIC", "", 0, 507, 170, 215, 300, 22);
    control(
        hwnd,
        "BUTTON",
        "Make alerts ring on silent or Do Not Disturb…",
        WS_TABSTOP,
        513,
        20,
        245,
        360,
        25,
    );
    control(
        hwnd,
        "BUTTON",
        "Save",
        WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
        514,
        290,
        290,
        80,
        28,
    );
    control(hwnd, "BUTTON", "Cancel", WS_TABSTOP, 515, 380, 290, 80, 28);
    refresh_telegram(hwnd);
    ShowWindow(hwnd, SW_SHOW);
}

unsafe fn refresh_telegram(hwnd: HWND) {
    let token = !field(hwnd, 501).is_empty();
    let chat = !field(hwnd, 502).is_empty();
    let waiting = TELEGRAM.with(|state| state.borrow().result.is_some());
    EnableWindow(GetDlgItem(hwnd, 511), (token && !waiting) as i32);
    EnableWindow(GetDlgItem(hwnd, 512), (token && chat) as i32);
}

pub(super) fn remove_destination(index: usize) {
    let mut destinations = app::config()["telegram_destinations"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if index < destinations.len() {
        destinations.remove(index);
        app::update_config("telegram_destinations", destinations.into());
    }
}

pub(super) unsafe fn test_destination(hwnd: HWND, index: usize) {
    let config = app::config();
    let Some(dest) = config["telegram_destinations"]
        .as_array()
        .and_then(|v| v.get(index))
    else {
        return;
    };
    let result = telegram::send_test(
        dest["token"].as_str().unwrap_or(""),
        dest["chat_id"].as_str().unwrap_or(""),
    );
    let (title, text) = match result {
        Ok(()) => (
            "Test sent",
            format!(
                "Check Telegram for {}.",
                dest["name"].as_str().unwrap_or("")
            ),
        ),
        Err(err) => ("Test failed", format!("Telegram says: {err}")),
    };
    alert(hwnd, title, &text);
}

unsafe fn choose_folder(hwnd: HWND) -> Option<String> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_INPROC_SERVER};
    use windows::Win32::UI::Shell::{
        FileOpenDialog, IFileOpenDialog, IShellItem, SHCreateItemFromParsingName, FOS_PICKFOLDERS,
        SIGDN_FILESYSPATH,
    };

    let dialog: IFileOpenDialog =
        CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
    dialog
        .SetOptions(dialog.GetOptions().ok()? | FOS_PICKFOLDERS)
        .ok()?;
    let title = wide("Save photos and video in");
    let _ = dialog.SetTitle(PCWSTR(title.as_ptr()));
    let config = app::config();
    let path = wide(config["media_dir"].as_str().unwrap_or(""));
    if let Ok(folder) = SHCreateItemFromParsingName::<_, _, IShellItem>(PCWSTR(path.as_ptr()), None)
    {
        let _ = dialog.SetFolder(&folder);
    }
    dialog
        .Show(Some(windows::Win32::Foundation::HWND(hwnd)))
        .ok()?;
    let path = dialog
        .GetResult()
        .ok()?
        .GetDisplayName(SIGDN_FILESYSPATH)
        .ok()?;
    let text = path.to_string().ok();
    CoTaskMemFree(Some(path.0.cast()));
    text
}

unsafe fn font(hwnd: HWND, dpi: u32) -> HANDLE {
    let mut metrics = NONCLIENTMETRICSW {
        cbSize: std::mem::size_of::<NONCLIENTMETRICSW>() as u32,
        ..Default::default()
    };
    if SystemParametersInfoForDpi(
        SPI_GETNONCLIENTMETRICS,
        metrics.cbSize,
        (&mut metrics as *mut NONCLIENTMETRICSW).cast(),
        0,
        dpi,
    ) == 0
    {
        return null_mut();
    }
    let result = CreateFontIndirectW(&metrics.lfMessageFont);
    SetPropW(hwnd, wide("unplugg-font").as_ptr(), result);
    result
}

pub(super) unsafe fn free_font(hwnd: HWND) {
    let old = RemovePropW(hwnd, wide("unplugg-font").as_ptr());
    if !old.is_null() {
        DeleteObject(old);
    }
}

pub(super) unsafe fn initial_dpi(hwnd: HWND, width: i32, height: i32) {
    let dpi = GetDpiForWindow(hwnd).max(96);
    SetPropW(hwnd, wide("unplugg-dpi").as_ptr(), dpi as HANDLE);
    font(hwnd, dpi);
    SetWindowPos(
        hwnd,
        null_mut(),
        0,
        0,
        width * dpi as i32 / 96,
        height * dpi as i32 / 96,
        SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
    );
}

struct Scale {
    parent: HWND,
    old: i32,
    new: i32,
    font: HANDLE,
}
unsafe extern "system" fn scale_child(hwnd: HWND, data: LPARAM) -> i32 {
    let scale = &*(data as *const Scale);
    if GetParent(hwnd) != scale.parent {
        return 1;
    }
    let mut rect = RECT::default();
    GetWindowRect(hwnd, &mut rect);
    MapWindowPoints(null_mut(), scale.parent, (&mut rect as *mut RECT).cast(), 2);
    SetWindowPos(
        hwnd,
        null_mut(),
        rect.left * scale.new / scale.old,
        rect.top * scale.new / scale.old,
        (rect.right - rect.left) * scale.new / scale.old,
        (rect.bottom - rect.top) * scale.new / scale.old,
        SWP_NOZORDER | SWP_NOACTIVATE,
    );
    if !scale.font.is_null() {
        SendMessageW(hwnd, WM_SETFONT, scale.font as WPARAM, 1);
    }
    1
}

pub(super) unsafe fn dpi_changed(hwnd: HWND, w: WPARAM, l: LPARAM) {
    let dpi = (w & 0xffff) as u32;
    let old = GetPropW(hwnd, wide("unplugg-dpi").as_ptr()) as usize as i32;
    if old == 0 || dpi == 0 {
        return;
    }
    let old_font = RemovePropW(hwnd, wide("unplugg-font").as_ptr());
    let new_font = font(hwnd, dpi);
    let data = Scale {
        parent: hwnd,
        old,
        new: dpi as i32,
        font: new_font,
    };
    EnumChildWindows(hwnd, Some(scale_child), &data as *const Scale as LPARAM);
    if !old_font.is_null() {
        DeleteObject(old_font);
    }
    SetPropW(hwnd, wide("unplugg-dpi").as_ptr(), dpi as HANDLE);
    let rect = &*(l as *const RECT);
    SetWindowPos(
        hwnd,
        null_mut(),
        rect.left,
        rect.top,
        rect.right - rect.left,
        rect.bottom - rect.top,
        SWP_NOZORDER | SWP_NOACTIVATE,
    );
}

