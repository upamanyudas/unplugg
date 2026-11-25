//! Launch at login: an XDG autostart entry.

use std::fs;
use std::io;
use std::path::PathBuf;

fn entry() -> PathBuf {
    let config = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| crate::config::home().join(".config"));
    config.join("autostart/unplugg.desktop")
}

pub fn enable() -> io::Result<()> {
    // an AppImage runs from $APPIMAGE, not its mount point
    let command = std::env::var_os("APPIMAGE").map(PathBuf::from).map_or_else(std::env::current_exe, Ok)?;
    let entry = entry();
    fs::create_dir_all(entry.parent().unwrap())?;
    // desktop entries quote with ""
    fs::write(entry, format!("[Desktop Entry]\nType=Application\nName=unplugg\nExec=\"{}\"\nIcon=unplugg\n", command.display()))
}

pub fn disable() -> io::Result<()> {
    match fs::remove_file(entry()) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}
