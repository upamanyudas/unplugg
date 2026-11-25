//! Updates from the latest GitHub release: the notes, then Install Update / Skip
//! This Version / Remind Me Later; installed automatically if allowed, never while
//! armed. Only the AppImage replaces itself; .deb, .rpm and brew belong to their
//! package manager, so the dialog says how to update instead.

use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{mpsc, Arc};
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;
use serde_json::Value;

use super::{app, dialogs, utils};
use crate::updater;

const DAY: u32 = 24 * 60 * 60;
const PAGE: &str = "https://github.com/upamanyudas/unplugg/releases/latest";

fn agent(read: u64) -> Option<ureq::Agent> {
    let tls = native_tls::TlsConnector::new().ok()?;
    Some(ureq::AgentBuilder::new().tls_connector(Arc::new(tls)).timeout_connect(Duration::from_secs(15)).timeout_read(Duration::from_secs(read)).build())
}

/// Runs `work` off the main thread, then `done` with its result on it.
fn background<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static, done: impl FnOnce(T) + 'static) {
    let (send, receive) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = send.send(work());
    });
    let mut done = Some(done);
    glib::timeout_add_local(Duration::from_millis(200), move || match receive.try_recv() {
        Ok(result) => {
            if let Some(done) = done.take() {
                done(result);
            }
            glib::ControlFlow::Break
        }
        Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
        Err(_) => glib::ControlFlow::Break,
    });
}

/// Checks now (if allowed) and again every day the app keeps running.
pub fn schedule() {
    if app::config()["check_updates"] == true {
        check(false);
    }
    glib::timeout_add_seconds_local_once(DAY, schedule);
}

pub fn check(manual: bool) {
    background(move || agent(15).and_then(|agent| updater::latest_release(&agent)), move |release| offer(release, manual));
}

/// None from source, where updating makes no sense.
fn installed_version() -> Option<&'static str> {
    let exe = std::env::current_exe().ok()?;
    let from_source = exe.components().any(|part| part.as_os_str() == "target");
    (!from_source).then_some(env!("CARGO_PKG_VERSION"))
}

/// None if this copy updates itself, else how you update it.
fn manager() -> Option<&'static str> {
    if std::env::var_os("APPIMAGE").is_none() {
        return Some("Download the new .deb or .rpm from GitHub.");
    }
    let brew = std::env::var_os("HOMEBREW_PREFIX").map_or_else(|| PathBuf::from("/home/linuxbrew/.linuxbrew"), PathBuf::from);
    brew.join("Caskroom/unplugg").exists().then_some("Run: brew upgrade unplugg")
}

fn offer(release: Option<Value>, manual: bool) {
    let Some(current) = installed_version() else {
        if manual {
            dialogs::alert("Updates need the released app", "This copy runs from source. Download unplugg from GitHub to get updates.", &[]);
        }
        return;
    };
    let Some(release) = release else {
        if manual {
            dialogs::alert("Couldn't check for updates", "GitHub didn't answer. Try again later.", &[]);
        }
        return;
    };
    let config = app::config();
    let tag = release["tag_name"].as_str().unwrap_or("");
    let version = tag.trim_start_matches('v').to_owned();
    if !updater::should_offer(tag, current, config["skipped_version"].as_str().unwrap_or(""), manual) {
        if manual {
            dialogs::alert("unplugg is up to date", &format!("Version {current} is the newest."), &[]);
        }
        return;
    }
    let how = manager();
    if config["auto_update"] == true && !manual && how.is_none() {
        if config["activated"] != true {
            install(&release);
        }
        return;
    }
    let notes = gtk::TextView::new();
    notes.set_editable(false);
    notes.set_wrap_mode(gtk::WrapMode::Word);
    notes.buffer().unwrap().set_text(&updater::notes(release["body"].as_str().filter(|b| !b.is_empty()).unwrap_or("No release notes.")));
    let scroll = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    scroll.set_size_request(420, 180);
    scroll.add(&notes);
    let text = format!("You have {current}.{} What's new:", how.map(|h| format!(" {h}")).unwrap_or_default());
    let first = match how {
        None => "Install Update",
        Some(h) if h.starts_with("Download") => "Open GitHub",
        Some(_) => "OK",
    };
    let choice = dialogs::alert_with(&format!("unplugg {version} is available"), &text, &[first, "Skip This Version", "Remind Me Later"], Some(scroll.upcast_ref()), None);
    match (choice, first) {
        (0, "Install Update") => install(&release),
        (0, "Open GitHub") => utils::open_url(PAGE),
        (1, _) => app::update_config("skipped_version", version.into()),
        _ => (),
    }
}

fn install(release: &Value) {
    if app::config()["activated"] == true {
        dialogs::alert("Turn off the alarm first", "unplugg can't update while it's armed.", &[]);
        return;
    }
    let name = if cfg!(target_arch = "aarch64") { "unplugg-linux-arm64.AppImage" } else { "unplugg-linux-x86_64.AppImage" };
    let url = |name: &str| release["assets"].as_array()?.iter().find(|a| a["name"] == name)?["browser_download_url"].as_str().map(str::to_owned);
    let (Some(image), Some(sha), Some(here)) = (url(name), url(&format!("{name}.sha256")), std::env::var_os("APPIMAGE").map(PathBuf::from)) else {
        dialogs::alert("Couldn't update", "This release has no download for this computer.", &[]);
        return;
    };
    // beside the AppImage, so the swap is a rename on one filesystem
    let new = here.with_file_name(format!(".{name}.new"));
    let target = new.clone();
    background(move || fetch(&image, &sha, &target), move |result| match result {
        Ok(()) => swap(&new, &here),
        Err(why) => {
            dialogs::alert("Couldn't update", &why, &[]);
        }
    });
}

fn fetch(url: &str, sha: &str, to: &PathBuf) -> Result<(), String> {
    let read = |url: &str, timeout| -> Result<Vec<u8>, String> {
        let mut data = Vec::new();
        agent(timeout).ok_or("No TLS.")?.get(url).call().map_err(|e| e.to_string())?.into_reader().read_to_end(&mut data).map_err(|e| e.to_string())?;
        Ok(data)
    };
    let data = read(url, 300)?;
    let checksum = String::from_utf8(read(sha, 15)?).map_err(|e| e.to_string())?;
    updater::verify_sha256(&data, &checksum).map_err(str::to_owned)?;
    std::fs::write(to, data).map_err(|e| e.to_string())
}

fn swap(new: &PathBuf, here: &PathBuf) {
    let replaced = std::fs::set_permissions(new, std::fs::Permissions::from_mode(0o755)).and_then(|_| std::fs::rename(new, here));
    if let Err(error) = replaced {
        let _ = std::fs::remove_file(new);
        dialogs::alert("Couldn't update", &format!("Linux wouldn't replace {}: {error}.", here.display()), &[]);
        return;
    }
    // reopen once this copy has quit, or the new one finds it still running
    let _ = std::process::Command::new("/bin/sh").args(["-c", "sleep 1; exec \"$0\""]).arg(here).spawn();
    app::quit();
}
