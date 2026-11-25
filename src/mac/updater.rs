use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSApplication, NSAutoresizingMaskOptions, NSBorderType, NSFont, NSScrollView, NSTextView};
use objc2_foundation::{NSBundle, NSPoint, NSRect, NSSize, NSString};
use serde_json::Value;

use super::app::App;
use super::dialogs::{alert, alert_with};
use crate::updater;

pub const DAY: f64 = 24.0 * 60.0 * 60.0;

enum Event {
    Release(Option<Value>, bool),
    Download(Result<PathBuf, String>),
}

thread_local! {
    static CHANNEL: (Sender<Event>, Receiver<Event>) = mpsc::channel();
}


fn agent(read: u64) -> Result<ureq::Agent, String> {
    let tls = native_tls::TlsConnector::new().map_err(|e| e.to_string())?;
    Ok(ureq::AgentBuilder::new()
        .tls_connector(Arc::new(tls))
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(read))
        .build())
}

/// Asks GitHub off the main thread; poll() answers on it.
pub fn check(manual: bool) {
    let sender = CHANNEL.with(|(sender, _)| sender.clone());
    std::thread::spawn(move || {
        let release = agent(15).ok().and_then(|agent| updater::latest_release(&agent));
        let _ = sender.send(Event::Release(release, manual));
    });
}

pub fn poll(app: &App) {
    while let Some(event) = CHANNEL.with(|(_, receiver)| receiver.try_recv().ok()) {
        match event {
            Event::Release(release, manual) => offer(app, release, manual),
            Event::Download(Ok(path)) if app.get("activated") != true => {
                if let Err(error) = swap(&path) {
                    alert("Couldn't update", &error, &[]);
                }
            }
            Event::Download(Ok(path)) => {
                let _ = fs::remove_dir_all(path.parent().unwrap());
            }
            Event::Download(Err(error)) => {
                alert("Couldn't update", &error, &[]);
            }
        }
    }
}

fn offer(app: &App, release: Option<Value>, manual: bool) {
    let Some((_, current)) = installed() else {
        if manual {
            alert("Updates need the released app", "This copy runs from source. Download unplugg from GitHub to get updates.", &[]);
        }
        return;
    };
    let Some(release) = release else {
        if manual {
            alert("Couldn't check for updates", "GitHub didn't answer. Try again later.", &[]);
        }
        return;
    };
    let tag = release["tag_name"].as_str().unwrap_or("");
    let version = tag.trim_start_matches('v');
    let skipped = app.get("skipped_version");
    if !updater::should_offer(tag, &current, skipped.as_str().unwrap_or(""), manual) {
        if manual {
            alert("unplugg is up to date", &format!("Version {current} is the newest."), &[]);
        }
        return;
    }
    if app.get("auto_update") == true && !manual {
        if app.get("activated") != true {
            install(app, &release);
        }
        return;
    }
    let body = release["body"].as_str().filter(|s| !s.is_empty()).unwrap_or("No release notes.");
    let choice = alert_with(
        &format!("unplugg {version} is available"),
        &format!("You have {current}. What's new:"),
        &["Install Update", "Skip This Version", "Remind Me Later"],
        &notes(&updater::notes(body)),
    );
    match choice {
        0 => install(app, &release),
        1 => app.set("skipped_version", version.into()),
        _ => (),
    }
}

fn install(app: &App, release: &Value) {
    if app.get("activated") == true {
        alert("Turn off the alarm first", "unplugg can't update while it's armed.", &[]);
        return;
    }
    let assets = release["assets"].as_array();
    let url = |name: &str| {
        assets
            .and_then(|items| items.iter().find(|item| item["name"] == name))
            .and_then(|item| item["browser_download_url"].as_str())
            .map(str::to_owned)
    };
    let (Some(dmg), Some(sha)) = (url("unplugg.dmg"), url("unplugg.dmg.sha256")) else {
        alert("Couldn't update", "This release has no download for this Mac.", &[]);
        return;
    };
    let sender = CHANNEL.with(|(sender, _)| sender.clone());
    std::thread::spawn(move || {
        let _ = sender.send(Event::Download(download(&dmg, &sha)));
    });
}

fn installed() -> Option<(PathBuf, String)> {
    let executable = std::env::current_exe().ok()?;
    let app = executable.parent()?.parent()?.parent()?;
    if app.extension()? != "app" {
        return None;
    }
    let bundle = NSBundle::mainBundle();
    let value =
        bundle.objectForInfoDictionaryKey(&NSString::from_str("CFBundleShortVersionString"))?;
    let version = value.downcast_ref::<NSString>()?.to_string();
    Some((app.to_owned(), version))
}

/// Scrollable release notes, read as plain text.
fn notes(text: &str) -> objc2::rc::Retained<NSScrollView> {
    let mtm = MainThreadMarker::new().unwrap();
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(420.0, 180.0));
    let scroll = NSScrollView::initWithFrame(NSScrollView::alloc(mtm), frame);
    scroll.setHasVerticalScroller(true);
    scroll.setBorderType(NSBorderType::BezelBorder);
    let view = NSTextView::initWithFrame(NSTextView::alloc(mtm), scroll.contentView().bounds());
    view.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable);
    view.setEditable(false);
    view.setFont(Some(&NSFont::systemFontOfSize(12.0)));
    view.setTextContainerInset(NSSize::new(6.0, 6.0));
    view.setString(&NSString::from_str(text));
    scroll.setDocumentView(Some(&view));
    scroll
}

fn download(dmg: &str, sha: &str) -> Result<PathBuf, String> {
    let fetch = |url: &str, timeout| -> Result<Vec<u8>, String> {
        let response = agent(timeout)?.get(url).call().map_err(|e| e.to_string())?;
        let mut data = Vec::new();
        response
            .into_reader()
            .read_to_end(&mut data)
            .map_err(|e| e.to_string())?;
        Ok(data)
    };
    let data = fetch(dmg, 120)?;
    let checksum = String::from_utf8(fetch(sha, 15)?).map_err(|e| e.to_string())?;
    updater::verify_sha256(&data, &checksum).map_err(str::to_owned)?;
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let work = std::env::temp_dir().join(format!("unplugg-update-{}-{unique}", std::process::id()));
    fs::create_dir(&work).map_err(|e| e.to_string())?;
    let result = (|| {
        let image = work.join("update.dmg");
        let mount = work.join("mount");
        fs::write(&image, data).map_err(|e| e.to_string())?;
        run(Command::new("/usr/bin/hdiutil")
            .args(["attach", "-nobrowse", "-readonly", "-mountpoint"])
            .arg(&mount)
            .arg(&image))?;
        let copied = run(Command::new("/usr/bin/ditto")
            .arg(mount.join("unplugg.app"))
            .arg(work.join("unplugg.app")));
        let detached = run(Command::new("/usr/bin/hdiutil").arg("detach").arg(&mount));
        copied?;
        detached?;
        Ok(work.join("unplugg.app"))
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(work);
    }
    result
}

fn run(command: &mut Command) -> Result<(), String> {
    let output = command.output().map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

fn swap(new_app: &Path) -> Result<(), String> {
    let (here, _) = installed().ok_or("This copy runs from source.")?;
    let parent = here.parent().ok_or("Missing application directory.")?;
    let staged = parent.join(format!(".unplugg-update-{}.app", std::process::id()));
    let old = parent.join(format!(".unplugg-old-{}.app", std::process::id()));
    run(Command::new("/usr/bin/ditto").arg(new_app).arg(&staged))?;
    fs::rename(&here, &old).map_err(|e| e.to_string())?;
    if let Err(error) = fs::rename(&staged, &here) {
        if fs::rename(&old, &here).is_ok() {
            let _ = fs::remove_dir_all(&staged);
        }
        return Err(format!(
            "macOS wouldn't replace {}: {error}.",
            here.display()
        ));
    }
    if let Err(error) = Command::new("/bin/sh")
        .args([
            "-c",
            "sleep 1; /usr/bin/open \"$1\"; /bin/rm -rf \"$2\" \"$3\"",
            "unplugg-update",
        ])
        .arg(&here)
        .arg(&old)
        .arg(new_app.parent().unwrap())
        .spawn()
    {
        if fs::rename(&here, &staged).is_ok() && fs::rename(&old, &here).is_ok() {
            let _ = fs::remove_dir_all(&staged);
        }
        return Err(error.to_string());
    }
    NSApplication::sharedApplication(MainThreadMarker::new().unwrap()).terminate(None);
    Ok(())
}
