use std::cell::RefCell;
use std::collections::HashMap;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const CLASS: &str = "unplugg-cover";
const SYNC: u32 = WM_APP + 30;
const INPUT: u32 = WM_APP + 31;

#[derive(Default)]
struct State {
    covers: Vec<HWND>,
    target: HWND,
    message: u32,
    armed_at: Option<Instant>,
    brightness: HashMap<String, u8>,
    alarm: bool,
    flash: bool,
}
thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn powershell(script: &str) -> Option<String> {
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .creation_flags(0x08000000)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn levels() -> HashMap<String, u8> {
    let Some(output) = powershell("Get-CimInstance -Namespace root/WMI -ClassName WmiMonitorBrightness | Select-Object InstanceName,CurrentBrightness | ConvertTo-Json -Compress") else { return HashMap::new() };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&output) else {
        return HashMap::new();
    };
    let rows: Vec<_> = match value {
        serde_json::Value::Array(v) => v,
        v => vec![v],
    };
    rows.into_iter()
        .filter_map(|row| {
            Some((
                row["InstanceName"].as_str()?.to_owned(),
                row["CurrentBrightness"].as_u64()?.try_into().ok()?,
            ))
        })
        .collect()
}

fn set_levels(levels: &HashMap<String, u8>) {
    if levels.is_empty() {
        return;
    }
    let Ok(json) = serde_json::to_string(levels) else {
        return;
    };
    let escaped = json.replace('\'', "''");
    let script = format!("$levels = ConvertFrom-Json '{escaped}'; Get-CimInstance -Namespace root/WMI -ClassName WmiMonitorBrightnessMethods | ForEach-Object {{ $level = $levels.PSObject.Properties[$_.InstanceName]; if ($null -ne $level) {{ Invoke-CimMethod -InputObject $_ -MethodName WmiSetBrightness -Arguments @{{Timeout=1; Brightness=[byte]$level.Value}} | Out-Null }} }}");
    let _ = powershell(&script);
}

pub fn show(target: HWND, message: u32, alarm: bool, flash: bool) {
    let first = STATE.with(|s| {
        let mut state = s.borrow_mut();
        let first = state.target.is_null();
        state.target = target;
        state.message = message;
        state.alarm = alarm;
        state.flash |= flash;
        first
    });
    if first {
        unsafe {
            register();
            sync();
        }
        listen();
    }
    if flash {
        let current = levels();
        STATE.with(|s| {
            let mut state = s.borrow_mut();
            for (name, level) in &current {
                state.brightness.entry(name.clone()).or_insert(*level);
            }
        });
        set_levels(&current.into_keys().map(|name| (name, 100)).collect());
    }
    STATE.with(|s| {
        for hwnd in &s.borrow().covers {
            unsafe {
                SetLayeredWindowAttributes(
                    *hwnd,
                    0,
                    if s.borrow().flash { 255 } else { 235 },
                    LWA_ALPHA,
                );
                InvalidateRect(*hwnd, null(), 1);
            }
        }
    });
}

pub fn listen() {
    STATE.with(|s| s.borrow_mut().armed_at = Some(Instant::now() + Duration::from_millis(1500)));
}

pub fn window() -> HWND {
    STATE.with(|s| s.borrow().covers.first().copied().unwrap_or(null_mut()))
}

pub fn hide() {
    let (covers, brightness) = STATE.with(|s| {
        let mut state = s.borrow_mut();
        let covers = std::mem::take(&mut state.covers);
        let brightness = std::mem::take(&mut state.brightness);
        *state = State::default();
        (covers, brightness)
    });
    for hwnd in covers {
        unsafe {
            DestroyWindow(hwnd);
        }
    }
    set_levels(&brightness);
}

unsafe fn register() {
    let class = wide(CLASS);
    let wc = WNDCLASSW {
        lpfnWndProc: Some(proc),
        hInstance: GetModuleHandleW(null()),
        lpszClassName: class.as_ptr(),
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        ..Default::default()
    };
    RegisterClassW(&wc);
}

unsafe extern "system" fn monitor(_: HMONITOR, _: HDC, rect: *mut RECT, data: LPARAM) -> i32 {
    (*(data as *mut Vec<RECT>)).push(*rect);
    1
}

unsafe fn sync() {
    let old = STATE.with(|s| std::mem::take(&mut s.borrow_mut().covers));
    for hwnd in old {
        DestroyWindow(hwnd);
    }
    let mut displays: Vec<RECT> = Vec::new();
    EnumDisplayMonitors(
        null_mut(),
        null(),
        Some(monitor),
        &mut displays as *mut _ as LPARAM,
    );
    let mut covers = Vec::new();
    for rect in displays {
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
            wide(CLASS).as_ptr(),
            null(),
            WS_POPUP,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            null_mut(),
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
        if hwnd.is_null() {
            continue;
        }
        SetLayeredWindowAttributes(
            hwnd,
            0,
            if STATE.with(|s| s.borrow().flash) {
                255
            } else {
                235
            },
            LWA_ALPHA,
        );
        let mut animate = 0i32;
        if SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            (&mut animate as *mut i32).cast(),
            0,
        ) != 0
            && animate != 0
        {
            SetTimer(hwnd, 1, 100, None);
        }
        ShowWindow(hwnd, SW_SHOW);
        covers.push(hwnd);
    }
    if let Some(hwnd) = covers.first() {
        SetForegroundWindow(*hwnd);
    }
    STATE.with(|s| s.borrow_mut().covers = covers);
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_DISPLAYCHANGE => {
            PostMessageW(hwnd, SYNC, 0, 0);
            0
        }
        SYNC => {
            if !window().is_null() {
                sync();
            }
            0
        }
        WM_KEYDOWN | WM_SYSKEYDOWN | WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN
        | WM_MOUSEMOVE | WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
            let ready = STATE.with(|s| {
                let mut state = s.borrow_mut();
                if state.armed_at.is_some_and(|at| Instant::now() >= at) {
                    state.armed_at = None;
                    true
                } else {
                    false
                }
            });
            if ready {
                PostMessageW(hwnd, INPUT, 0, 0);
            }
            0
        }
        INPUT => {
            let (target, message) = STATE.with(|s| {
                let state = s.borrow();
                (state.target, state.message)
            });
            if !target.is_null() {
                PostMessageW(target, message, 0, 0);
            }
            0
        }
        WM_TIMER => {
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_PAINT => {
            paint(hwnd);
            0
        }
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}

unsafe fn paint(hwnd: HWND) {
    let (alarm, flash) = STATE.with(|s| {
        let state = s.borrow();
        (state.alarm, state.flash)
    });
    let mut ps = PAINTSTRUCT::default();
    let dc = BeginPaint(hwnd, &mut ps);
    let mut rect = RECT::default();
    GetClientRect(hwnd, &mut rect);
    let brush = CreateSolidBrush(if flash { 0x00ffffff } else { 0x00161212 });
    FillRect(dc, &rect, brush);
    DeleteObject(brush);
    if !flash {
        SetBkMode(dc, TRANSPARENT as i32);
        let center = (rect.bottom - rect.top) / 2;
        let icon_data = include_bytes!("../../assets/win/cover.png");
        let icon = CreateIconFromResourceEx(
            icon_data.as_ptr(),
            icon_data.len() as u32,
            1,
            0x00030000,
            160,
            160,
            0,
        );
        if !icon.is_null() {
            DrawIconEx(
                dc,
                (rect.right - 160) / 2,
                center - 205,
                icon,
                160,
                160,
                0,
                null_mut(),
                DI_NORMAL,
            );
            DestroyIcon(icon);
        }
        let font = CreateFontW(
            -40,
            0,
            0,
            0,
            600,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            0,
            0,
            0,
            0,
            wide("Segoe UI").as_ptr(),
        );
        let old = SelectObject(dc, font);
        SetTextColor(dc, 0x00ffffff);
        let mut title = wide("Protected by unplugg");
        let mut line = RECT {
            left: 0,
            top: center - 40,
            right: rect.right,
            bottom: center + 12,
        };
        DrawTextW(
            dc,
            title.as_mut_ptr(),
            -1,
            &mut line,
            DT_CENTER | DT_SINGLELINE | DT_VCENTER,
        );
        SelectObject(dc, old);
        DeleteObject(font);
        let font = CreateFontW(
            -19,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            0,
            0,
            0,
            0,
            wide("Segoe UI").as_ptr(),
        );
        let old = SelectObject(dc, font);
        SetTextColor(dc, 0x00bbbbbb);
        let mut hint = wide("Press any key or move the mouse to unlock.");
        line.top = center + 18;
        line.bottom = center + 48;
        DrawTextW(
            dc,
            hint.as_mut_ptr(),
            -1,
            &mut line,
            DT_CENTER | DT_SINGLELINE | DT_VCENTER,
        );
        let mut badge = wide(if alarm {
            "●  Alarm triggered"
        } else {
            "●  Armed"
        });
        let elapsed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            % 2000;
        let phase = if elapsed < 1000 {
            elapsed
        } else {
            2000 - elapsed
        };
        let intensity = 255 - (phase * 3 / 16) as u32;
        let color = if alarm { 0x00303bffu32 } else { 0x0059c734u32 };
        let fade = |v: u32| (v * intensity / 255) & 255;
        SetTextColor(
            dc,
            fade(color & 255) | (fade((color >> 8) & 255) << 8) | (fade((color >> 16) & 255) << 16),
        );
        line.top = center + 52;
        line.bottom = center + 82;
        DrawTextW(
            dc,
            badge.as_mut_ptr(),
            -1,
            &mut line,
            DT_CENTER | DT_SINGLELINE | DT_VCENTER,
        );
        SelectObject(dc, old);
        DeleteObject(font);
    }
    EndPaint(hwnd, &ps);
}
