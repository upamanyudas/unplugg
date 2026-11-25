use std::path::PathBuf;
use std::process::{Command, Stdio};

use gtk::gio::{self, prelude::*};
use gtk::prelude::*;

/// A bundled file: /usr/share/unplugg beside a package or AppImage's bin/, else the source tree.
pub fn asset_path(name: &str) -> PathBuf {
    let installed = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.parent()?.join("share/unplugg").join(name)))
        .filter(|path| path.exists());
    installed.unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets").join(name))
}

/// FAAC ships beside the binary, in lib/unplugg, so it can be swapped as the LGPL allows.
pub fn library_path(name: &str) -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.parent()?.join("lib/unplugg").join(name)))
        .unwrap_or_else(|| PathBuf::from(name))
}

pub fn app_icon() -> Option<gtk::gdk_pixbuf::Pixbuf> {
    gtk::gdk_pixbuf::Pixbuf::from_file(asset_path("logo.png")).ok()
}

/// Whether the panel is light, so the tray glyph is drawn black, else white.
pub fn light_bar() -> bool {
    // GNOME's top bar is dark in every theme
    if std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().contains("GNOME") {
        return false;
    }
    let Some(settings) = gtk::Settings::default() else { return true };
    let dark_theme = settings.gtk_theme_name().is_some_and(|name| name.to_lowercase().contains("dark"));
    !(settings.is_gtk_application_prefer_dark_theme() || dark_theme)
}

/// A web address, or a folder in the file manager.
pub fn open_url(url: &str) {
    let uri = if url.contains("://") { url.to_owned() } else { gio::File::for_path(url).uri().to_string() };
    let _ = gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>);
}

/// Lock now; the desktop's lock screen enforces the password.
pub fn lock_screen() {
    // logind tells the desktop's locker; xdg-screensaver covers sessions without systemd
    for command in [["loginctl", "lock-session"], ["xdg-screensaver", "lock"]] {
        let status = Command::new(command[0]).arg(command[1]).stdout(Stdio::null()).stderr(Stdio::null()).status();
        if status.is_ok_and(|status| status.success()) {
            return;
        }
    }
}
