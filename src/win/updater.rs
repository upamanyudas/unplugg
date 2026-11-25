use std::fs;
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::updater;

pub enum Event {
    Release(Option<Value>, bool),
    Download(Result<PathBuf, String>),
}

thread_local! {
    static CHANNEL: (Sender<Event>, Receiver<Event>) = mpsc::channel();
}

fn agent(timeout: u64) -> Result<ureq::Agent, String> {
    let tls = native_tls::TlsConnector::new().map_err(|e| e.to_string())?;
    Ok(ureq::AgentBuilder::new()
        .tls_connector(Arc::new(tls))
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(timeout))
        .build())
}

pub fn check(manual: bool) {
    let sender = CHANNEL.with(|(sender, _)| sender.clone());
    std::thread::spawn(move || {
        let release = agent(15)
            .ok()
            .and_then(|agent| updater::latest_release(&agent));
        let _ = sender.send(Event::Release(release, manual));
    });
}

pub fn manager() -> Option<&'static str> {
    let path = match std::env::current_exe() {
        Ok(path) => path,
        Err(_) => return Some("Download it from GitHub."),
    };
    if path.components().any(|part| {
        part.as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case("scoop")
    }) {
        return Some("Run: scoop update unplugg");
    }
    match path.parent() {
        Some(folder) if folder.join("unins000.exe").exists() => None,
        _ => Some("Download it from GitHub."),
    }
}

pub fn download(release: &Value) -> bool {
    let name = if cfg!(target_arch = "aarch64") {
        "unplugg-windows-arm64-setup.exe"
    } else {
        "unplugg-windows-x64-setup.exe"
    };
    let assets = release["assets"].as_array();
    let url = |name: &str| {
        assets
            .and_then(|items| items.iter().find(|item| item["name"] == name))
            .and_then(|item| item["browser_download_url"].as_str())
            .map(str::to_owned)
    };
    let (Some(installer), Some(sha)) = (url(name), url(&format!("{name}.sha256"))) else {
        return false;
    };
    let sender = CHANNEL.with(|(sender, _)| sender.clone());
    let name = name.to_owned();
    std::thread::spawn(move || {
        let _ = sender.send(Event::Download(fetch(&installer, &sha, &name)));
    });
    true
}

fn fetch(url: &str, sha: &str, name: &str) -> Result<PathBuf, String> {
    let read = |url: &str, timeout| -> Result<Vec<u8>, String> {
        let response = agent(timeout)?.get(url).call().map_err(|e| e.to_string())?;
        let mut data = Vec::new();
        response
            .into_reader()
            .read_to_end(&mut data)
            .map_err(|e| e.to_string())?;
        Ok(data)
    };
    let data = read(url, 300)?;
    let checksum = String::from_utf8(read(sha, 15)?).map_err(|e| e.to_string())?;
    updater::verify_sha256(&data, &checksum).map_err(str::to_owned)?;
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let folder =
        std::env::temp_dir().join(format!("unplugg-update-{}-{unique}", std::process::id()));
    fs::create_dir(&folder).map_err(|e| e.to_string())?;
    let path = folder.join(name);
    if let Err(error) = fs::write(&path, data) {
        let _ = fs::remove_dir_all(folder);
        return Err(error.to_string());
    }
    Ok(path)
}

pub fn launch(path: &PathBuf) -> Result<(), String> {
    Command::new(path)
        .arg("/S")
        .creation_flags(0x00000008)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn alert(hwnd: windows_sys::Win32::Foundation::HWND, title: &str, text: &str) {
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
            hwnd,
            wide(text).as_ptr(),
            wide(title).as_ptr(),
            0,
        );
    }
}

unsafe extern "system" fn offer_proc(
    hwnd: windows_sys::Win32::Foundation::HWND,
    message: u32,
    w: usize,
    _: isize,
) -> isize {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    match message {
        WM_INITDIALOG => 1,
        WM_COMMAND if matches!(w & 0xffff, 101..=103 | 2) => {
            EndDialog(hwnd, (w & 0xffff) as isize);
            1
        }
        WM_CLOSE => {
            EndDialog(hwnd, 2);
            1
        }
        _ => 0,
    }
}

fn offer(
    hwnd: windows_sys::Win32::Foundation::HWND,
    title: &str,
    text: &str,
    notes: &str,
    first: &str,
) -> i32 {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    fn dword(words: &mut Vec<u16>, value: u32) {
        words.extend([value as u16, (value >> 16) as u16]);
    }
    fn string(words: &mut Vec<u16>, value: &str) {
        words.extend(value.encode_utf16().chain(Some(0)));
    }
    fn item(words: &mut Vec<u16>, style: u32, pos: [u16; 4], id: u16, class: u16, text: &str) {
        if words.len() % 2 != 0 {
            words.push(0);
        }
        dword(words, style);
        dword(words, 0);
        words.extend(pos);
        words.extend([id, 0xffff, class]);
        string(words, text);
        words.push(0);
    }
    let mut words = Vec::new();
    dword(
        &mut words,
        WS_POPUP
            | WS_CAPTION
            | WS_SYSMENU
            | DS_MODALFRAME as u32
            | DS_SETFONT as u32
            | DS_CENTER as u32,
    );
    dword(&mut words, 0);
    words.extend([5, 0, 0, 330, 190, 0, 0]);
    string(&mut words, title);
    words.push(9);
    string(&mut words, "Segoe UI");
    let common = WS_CHILD | WS_VISIBLE;
    item(&mut words, common, [10, 10, 310, 24], 104, 0x82, text);
    item(
        &mut words,
        common
            | WS_TABSTOP
            | WS_BORDER
            | WS_VSCROLL
            | ES_MULTILINE as u32
            | ES_READONLY as u32
            | ES_AUTOVSCROLL as u32,
        [10, 36, 310, 120],
        105,
        0x81,
        &notes.replace('\n', "\r\n"),
    );
    item(
        &mut words,
        common | WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
        [10, 165, 100, 16],
        101,
        0x80,
        first,
    );
    item(
        &mut words,
        common | WS_TABSTOP,
        [115, 165, 100, 16],
        102,
        0x80,
        "Skip This Version",
    );
    item(
        &mut words,
        common | WS_TABSTOP,
        [220, 165, 100, 16],
        103,
        0x80,
        "Remind Me Later",
    );
    if words.len() % 2 != 0 {
        words.push(0);
    }
    let aligned: Vec<u32> = words
        .chunks_exact(2)
        .map(|pair| pair[0] as u32 | ((pair[1] as u32) << 16))
        .collect();
    let result = unsafe {
        DialogBoxIndirectParamW(
            GetModuleHandleW(std::ptr::null()),
            aligned.as_ptr().cast(),
            hwnd,
            Some(offer_proc),
            0,
        )
    };
    if result == -1 {
        alert(
            hwnd,
            "Couldn't check for updates",
            "The update window couldn't be opened.",
        );
        0
    } else {
        result as i32
    }
}

fn install(hwnd: windows_sys::Win32::Foundation::HWND, release: &Value) {
    if super::app::config()["activated"] == true {
        alert(
            hwnd,
            "Turn off the alarm first",
            "unplugg can't update while it's armed.",
        );
    } else if !download(release) {
        alert(
            hwnd,
            "Couldn't update",
            "This release has no download for this computer.",
        );
    }
}

fn handle_release(
    hwnd: windows_sys::Win32::Foundation::HWND,
    release: Option<Value>,
    manual: bool,
) {
    let Some(release) = release else {
        if manual {
            alert(
                hwnd,
                "Couldn't check for updates",
                "GitHub didn't answer. Try again later.",
            );
        }
        return;
    };
    let current = env!("CARGO_PKG_VERSION");
    let version = release["tag_name"]
        .as_str()
        .unwrap_or("")
        .trim_start_matches('v');
    let config = super::app::config();
    if !updater::should_offer(
        version,
        current,
        config["skipped_version"].as_str().unwrap_or(""),
        manual,
    ) {
        if manual {
            alert(
                hwnd,
                "unplugg is up to date",
                &format!("Version {current} is the newest."),
            );
        }
        return;
    }
    let how = manager();
    if config["auto_update"] == true && !manual && how.is_none() {
        if config["activated"] != true {
            install(hwnd, &release);
        }
        return;
    }
    let notes = updater::notes(
        release["body"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or("No release notes."),
    );
    let text = format!(
        "You have {current}.{} What's new:",
        how.map(|s| format!(" {s}")).unwrap_or_default()
    );
    let first = match how {
        None => "Install Update",
        Some(s) if s.starts_with("Download") => "Open GitHub",
        _ => "OK",
    };
    match offer(
        hwnd,
        &format!("unplugg {version} is available"),
        &text,
        &notes,
        first,
    ) {
        101 if how.is_none() => install(hwnd, &release),
        101 if first == "Open GitHub" => {
            super::utils::open_url("https://github.com/upamanyudas/unplugg/releases/latest")
        }
        102 => super::app::update_config("skipped_version", version.into()),
        _ => (),
    }
}

pub fn poll(hwnd: windows_sys::Win32::Foundation::HWND) -> bool {
    thread_local! { static BUSY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
    if BUSY.with(|busy| busy.replace(true)) {
        return false;
    }
    let event = CHANNEL.with(|(_, receiver)| receiver.try_recv().ok());
    let quit = match event {
        Some(Event::Release(release, manual)) => {
            handle_release(hwnd, release, manual);
            false
        }
        Some(Event::Download(Err(error))) => {
            alert(hwnd, "Couldn't update", &error);
            false
        }
        Some(Event::Download(Ok(path))) => {
            if super::app::config()["activated"] == true {
                if let Some(folder) = path.parent() {
                    let _ = fs::remove_dir_all(folder);
                }
                alert(
                    hwnd,
                    "Turn off the alarm first",
                    "unplugg can't update while it's armed.",
                );
                false
            } else if let Err(error) = launch(&path) {
                if let Some(folder) = path.parent() {
                    let _ = fs::remove_dir_all(folder);
                }
                alert(hwnd, "Couldn't update", &error);
                false
            } else {
                true
            }
        }
        None => false,
    };
    BUSY.with(|busy| busy.set(false));
    quit
}
