use std::cell::RefCell;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::{COLOR_WINDOW, HBRUSH};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Controls::{BST_CHECKED, BST_UNCHECKED};
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use super::{app, loginitem, permissions, ui};

const CLASS: &str = "unplugg-guide";
const STEPS: [(&str, &str); 4] = [
    ("Welcome to unplugg", "Your laptop, guarded while you step away."),
    ("Permissions", "Each one turns on one feature. Skip any you don't need."),
    ("Get alerts on your phone", "When your laptop is disturbed, a Telegram bot sends you photos, video and where it is. Click + to add one."),
    ("You're all set", "Before you step away, click unplugg in the tray and choose Activate."),
];

#[derive(Default)]
struct Guide {
    hwnd: HWND,
    page: usize,
    controls: Vec<HWND>,
}
thread_local! { static GUIDE: RefCell<Guide> = RefCell::new(Guide::default()); }

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

unsafe fn control(
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
    let dpi = GetDpiForWindow(parent).max(1) as i32;
    let scale = |v| (v * dpi + 48) / 96;
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
    GUIDE.with(|state| state.borrow_mut().controls.push(child));
    child
}

unsafe fn page(hwnd: HWND, index: usize) {
    let old = GUIDE.with(|state| {
        let mut state = state.borrow_mut();
        state.page = index;
        std::mem::take(&mut state.controls)
    });
    for child in old {
        DestroyWindow(child);
    }
    control(hwnd, "STATIC", STEPS[index].0, 0, 0, 20, 18, 460, 24);
    control(hwnd, "STATIC", STEPS[index].1, 0, 0, 20, 48, 460, 48);
    match index {
        0 => {
            for (i, (title, text)) in [
                ("Plug in", "unplugg watches the power cable and the lid."),
                (
                    "Activate",
                    "Click unplugg in the taskbar's tray before you step away.",
                ),
                (
                    "If it's unplugged or moved",
                    "A siren sounds. Photos, video and location go to your phone.",
                ),
                (
                    "Only you can stop it",
                    "Turning it off asks for Windows Hello or your password.",
                ),
            ]
            .iter()
            .enumerate()
            {
                control(
                    hwnd,
                    "STATIC",
                    &format!("{title}\n{text}"),
                    0,
                    0,
                    30,
                    105 + i as i32 * 55,
                    430,
                    48,
                );
            }
        }
        1 => {
            control(hwnd, "STATIC", "All optional", 0, 0, 20, 100, 430, 20);
            for (i, (key, title, why)) in [
                ("camera", "Camera", "Photos and video of a thief."),
                ("microphone", "Microphone", "Sound in that video."),
                ("location", "Location", "A map link to your laptop."),
            ]
            .iter()
            .enumerate()
            {
                let y = 125 + i as i32 * 65;
                let allowed = permissions::status(key) == "granted";
                control(
                    hwnd,
                    "STATIC",
                    &format!(
                        "{title} — {}\n{why}",
                        if allowed {
                            "● Allowed"
                        } else {
                            "● Blocked"
                        }
                    ),
                    0,
                    0,
                    30,
                    y,
                    270,
                    48,
                );
                if !allowed {
                    control(
                        hwnd,
                        "BUTTON",
                        "Open Settings…",
                        WS_TABSTOP,
                        300 + i,
                        310,
                        y,
                        130,
                        28,
                    );
                }
            }
        }
        2 => {
            control(hwnd, "STATIC", "Telegram", 0, 0, 20, 108, 430, 22);
            control(hwnd, "STATIC", "Destinations", 0, 0, 20, 147, 90, 22);
            let choices = control(
                hwnd,
                "COMBOBOX",
                "",
                WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWNLIST as u32,
                200,
                112,
                142,
                145,
                150,
            );
            let config = app::config();
            let destinations = config["telegram_destinations"].as_array();
            if let Some(destinations) = destinations.filter(|d| !d.is_empty()) {
                for destination in destinations {
                    SendMessageW(
                        choices,
                        CB_ADDSTRING,
                        0,
                        wide(destination["name"].as_str().unwrap_or("")).as_ptr() as LPARAM,
                    );
                }
            } else {
                SendMessageW(
                    choices,
                    CB_ADDSTRING,
                    0,
                    wide("None yet").as_ptr() as LPARAM,
                );
                EnableWindow(choices, 0);
            }
            SendMessageW(choices, CB_SETCURSEL, 0, 0);
            for (id, title, x, width) in [
                (201, "+", 264, 32),
                (202, "−", 300, 32),
                (203, "Send Test", 337, 100),
            ] {
                let button = control(hwnd, "BUTTON", title, WS_TABSTOP, id, x, 142, width, 27);
                if id != 201 && destinations.is_none_or(|d| d.is_empty()) {
                    EnableWindow(button, 0);
                }
            }
        }
        _ => {
            let check = control(
                hwnd,
                "BUTTON",
                "Launch at login",
                WS_TABSTOP | BS_AUTOCHECKBOX as u32,
                400,
                25,
                110,
                400,
                26,
            );
            SendMessageW(
                check,
                BM_SETCHECK,
                if app::config()["launch_at_login"] == true {
                    BST_CHECKED as usize
                } else {
                    BST_UNCHECKED as usize
                },
                0,
            );
            control(hwnd, "STATIC", "Windows hides new tray icons under the ^ arrow. Drag unplugg onto the taskbar to keep it in sight.", 0, 0, 25, 155, 430, 60);
        }
    }
    if index > 0 {
        control(hwnd, "BUTTON", "Back", WS_TABSTOP, 100, 290, 350, 85, 30);
    }
    control(
        hwnd,
        "BUTTON",
        if index == 3 { "Finish" } else { "Next" },
        WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
        101,
        380,
        350,
        85,
        30,
    );
}

unsafe extern "system" fn window(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            let id = w & 0xffff;
            let current = GUIDE.with(|state| state.borrow().page);
            match id {
                100 if current > 0 => page(hwnd, current - 1),
                101 if current < 3 => page(hwnd, current + 1),
                101 => {
                    DestroyWindow(hwnd);
                }
                201 => ui::open_telegram(hwnd),
                202 | 203 => {
                    let selected = SendMessageW(GetDlgItem(hwnd, 200), CB_GETCURSEL, 0, 0);
                    if selected >= 0 {
                        if id == 202 {
                            ui::remove_destination(selected as usize);
                            page(hwnd, current);
                        } else {
                            ui::test_destination(hwnd, selected as usize);
                        }
                    }
                }
                300..=302 => {
                    permissions::open_settings(["camera", "microphone", "location"][id - 300])
                }
                400 => {
                    let checked =
                        SendMessageW(l as HWND, BM_GETCHECK, 0, 0) == BST_CHECKED as isize;
                    app::update_config("launch_at_login", checked.into());
                    let _ = if checked {
                        loginitem::enable()
                    } else {
                        loginitem::disable()
                    };
                }
                _ => (),
            }
            0
        }
        WM_ACTIVATE if w as u16 != WA_INACTIVE as u16 => {
            let current = GUIDE.with(|state| state.borrow().page);
            if current == 1 || current == 2 {
                page(hwnd, current);
            }
            0
        }
        WM_DPICHANGED => {
            ui::dpi_changed(hwnd, w, l);
            let current = GUIDE.with(|state| state.borrow().page);
            page(hwnd, current);
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            ui::free_font(hwnd);
            app::update_config("setup_done", true.into());
            GUIDE.with(|state| *state.borrow_mut() = Guide::default());
            0
        }
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}

pub fn open() {
    unsafe {
        let existing = GUIDE.with(|state| state.borrow().hwnd);
        if !existing.is_null() {
            ShowWindow(existing, SW_SHOW);
            SetForegroundWindow(existing);
            return;
        }
        let instance = GetModuleHandleW(null());
        let class = wide(CLASS);
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(window),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: (COLOR_WINDOW + 1) as HBRUSH,
            ..Default::default()
        });
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            wide("unplugg Setup Guide").as_ptr(),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            500,
            440,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() {
            return;
        }
        GUIDE.with(|state| state.borrow_mut().hwnd = hwnd);
        ui::initial_dpi(hwnd, 500, 440);
        page(hwnd, 0);
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
    }
}

pub fn is_open() -> bool {
    GUIDE.with(|state| !state.borrow().hwnd.is_null())
}

pub unsafe fn dialog_message(msg: &MSG) -> bool {
    let hwnd = GUIDE.with(|state| state.borrow().hwnd);
    !hwnd.is_null() && IsDialogMessageW(hwnd, msg) != 0
}

pub fn about() {
    unsafe {
        MessageBoxW(null_mut(), wide(&format!("unplugg {}\n\nYour laptop, guarded while you step away.\n\nCopyright © 2025 Upa Das. MIT License.", env!("CARGO_PKG_VERSION"))).as_ptr(), wide("unplugg").as_ptr(), MB_OK | MB_TOPMOST);
    }
}

unsafe extern "system" fn help_hook(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    if code == HCBT_ACTIVATE as i32 {
        SetWindowTextW(GetDlgItem(w as HWND, IDOK), wide("Done").as_ptr());
    }
    CallNextHookEx(null_mut(), code, w, l)
}

pub fn help() {
    unsafe {
        let hook = SetWindowsHookExW(WH_CBT, Some(help_hook), null_mut(), GetCurrentThreadId());
        MessageBoxW(null_mut(), wide("How unplugg works\n\n1. Plug in and add a Telegram destination.\n2. Choose what counts as unsafe: power unplugged, lid closed, or moved.\n3. Click Activate in unplugg's tray menu before you step away.\n\nWhen the laptop is disturbed: it sirens, captures photo + video, sends a location link, and alerts Telegram, then optionally locks.").as_ptr(), wide("unplugg").as_ptr(), MB_OK | MB_TOPMOST);
        if !hook.is_null() {
            UnhookWindowsHookEx(hook);
        }
    }
}
