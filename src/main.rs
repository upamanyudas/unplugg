#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "windows")]
mod win;

fn main() {
    #[cfg(target_os = "linux")]
    linux::run();
    #[cfg(target_os = "macos")]
    mac::run();
    #[cfg(target_os = "windows")]
    win::run();
}
