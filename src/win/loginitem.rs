use std::io;
use std::os::windows::ffi::OsStrExt;

use windows::core::PCWSTR;
use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_SET_VALUE, REG_SZ,
};

use super::utils::wide;

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

fn update(command: Option<&[u16]>) -> io::Result<()> {
    let path = wide(RUN);
    let name = wide("unplugg");
    unsafe {
        let mut key = HKEY::default();
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(path.as_ptr()),
            None,
            KEY_SET_VALUE,
            &mut key,
        )
        .ok()
        .map_err(io::Error::other)?;
        let result = if let Some(command) = command {
            let bytes =
                std::slice::from_raw_parts(command.as_ptr().cast::<u8>(), command.len() * 2);
            RegSetValueExW(key, PCWSTR(name.as_ptr()), None, REG_SZ, Some(bytes))
        } else {
            RegDeleteValueW(key, PCWSTR(name.as_ptr()))
        };
        let _ = RegCloseKey(key);
        if result == ERROR_FILE_NOT_FOUND && command.is_none() {
            return Ok(());
        }
        result.ok().map_err(io::Error::other)
    }
}

pub fn enable() -> io::Result<()> {
    let path = std::env::current_exe()?;
    let mut command = vec![b'"' as u16];
    command.extend(path.as_os_str().encode_wide());
    command.extend([b'"' as u16, 0]);
    update(Some(&command))
}

pub fn disable() -> io::Result<()> {
    update(None)
}
