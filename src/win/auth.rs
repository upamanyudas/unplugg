use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows::core::{w, HSTRING, PCWSTR, PWSTR};
use windows::Security::Credentials::UI::{
    UserConsentVerificationResult, UserConsentVerifier, UserConsentVerifierAvailability,
};
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ACCOUNT_RESTRICTION, ERROR_LOGON_FAILURE, HANDLE, HWND,
};
use windows::Win32::Security::Credentials::{
    CredUIPromptForWindowsCredentialsW, CredUnPackAuthenticationBufferW,
    CREDUIWIN_ENUMERATE_CURRENT_USER, CREDUI_INFOW, CRED_PACK_PROTECTED_CREDENTIALS,
};
use windows::Win32::Security::{LogonUserW, LOGON32_LOGON_INTERACTIVE, LOGON32_PROVIDER_DEFAULT};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, FindWindowW, PeekMessageW, PostQuitMessage, SetForegroundWindow,
    SetWindowPos, TranslateMessage, HWND_TOPMOST, MSG, PM_REMOVE, SWP_NOMOVE, SWP_NOSIZE, WM_QUIT,
};

static ASKING: AtomicBool = AtomicBool::new(false);

pub fn require(reason: &str, parent: HWND) -> bool {
    if ASKING.swap(true, Ordering::SeqCst) {
        return false;
    }
    let prompt = format!("unplugg wants to {reason}.");
    let result = if hello_ready() {
        hello(&prompt)
    } else {
        password(&prompt, parent)
    };
    ASKING.store(false, Ordering::SeqCst);
    result
}

fn wait(mut done: impl FnMut() -> bool, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while !done() && Instant::now() < deadline {
        unsafe {
            let mut message = MSG::default();
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                if message.message == WM_QUIT {
                    PostQuitMessage(message.wParam.0 as i32);
                    return false;
                }
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    done()
}

fn hello_ready() -> bool {
    let Ok(op) = UserConsentVerifier::CheckAvailabilityAsync() else {
        return false;
    };
    wait(
        || op.Status().map_or(true, |status| status.0 != 0),
        Duration::from_secs(5),
    ) && op.Status().is_ok_and(|status| status.0 == 1)
        && op.GetResults() == Ok(UserConsentVerifierAvailability::Available)
}

fn hello(prompt: &str) -> bool {
    let Ok(op) = UserConsentVerifier::RequestVerificationAsync(&HSTRING::from(prompt)) else {
        return false;
    };
    let mut raised = false;
    let finished = wait(
        || {
            if !raised {
                unsafe {
                    if let Ok(dialog) =
                        FindWindowW(w!("Credential Dialog Xaml Host"), PCWSTR::null())
                    {
                        let _ = SetWindowPos(
                            dialog,
                            Some(HWND_TOPMOST),
                            0,
                            0,
                            0,
                            0,
                            SWP_NOMOVE | SWP_NOSIZE,
                        );
                        let _ = SetForegroundWindow(dialog);
                        raised = true;
                    }
                }
            }
            op.Status().map_or(true, |status| status.0 != 0)
        },
        Duration::from_secs(120),
    );
    if !finished {
        let _ = op.Cancel();
    }
    finished
        && op.Status().is_ok_and(|status| status.0 == 1)
        && op.GetResults() == Ok(UserConsentVerificationResult::Verified)
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn clear<T: Default>(buffer: &mut [T]) {
    for value in buffer {
        unsafe { std::ptr::write_volatile(value, T::default()) };
    }
}

fn password(prompt: &str, parent: HWND) -> bool {
    let message = wide(prompt);
    let info = CREDUI_INFOW {
        cbSize: std::mem::size_of::<CREDUI_INFOW>() as u32,
        hwndParent: parent,
        pszMessageText: PCWSTR(message.as_ptr()),
        pszCaptionText: w!("unplugg"),
        ..Default::default()
    };
    let mut error = 0;
    for _ in 0..3 {
        let (mut package, mut blob, mut size) = (0, std::ptr::null_mut(), 0);
        unsafe {
            if CredUIPromptForWindowsCredentialsW(
                Some(&info),
                error,
                &mut package,
                None,
                0,
                &mut blob,
                &mut size,
                None,
                CREDUIWIN_ENUMERATE_CURRENT_USER,
            ) != 0
            {
                return false;
            }
            let (mut user, mut domain, mut secret) = ([0u16; 512], [0u16; 512], [0u16; 512]);
            let (mut user_len, mut domain_len, mut secret_len) = (512, 512, 512);
            let unpacked = CredUnPackAuthenticationBufferW(
                CRED_PACK_PROTECTED_CREDENTIALS,
                blob,
                size,
                Some(PWSTR(user.as_mut_ptr())),
                &mut user_len,
                Some(PWSTR(domain.as_mut_ptr())),
                Some(&mut domain_len),
                Some(PWSTR(secret.as_mut_ptr())),
                &mut secret_len,
            )
            .is_ok();
            if !blob.is_null() {
                clear(std::slice::from_raw_parts_mut(
                    blob.cast::<u8>(),
                    size as usize,
                ));
                CoTaskMemFree(Some(blob));
            }
            if !unpacked {
                clear(&mut secret);
                error = ERROR_LOGON_FAILURE.0;
                continue;
            }
            let end = user.iter().position(|&c| c == 0).unwrap_or(user.len());
            let slash = user[..end].iter().position(|&c| c == b'\\' as u16);
            let (name, where_) = if let Some(i) = slash {
                user[i] = 0;
                (PCWSTR(user.as_ptr().add(i + 1)), PCWSTR(user.as_ptr()))
            } else if domain[0] != 0 {
                (PCWSTR(user.as_ptr()), PCWSTR(domain.as_ptr()))
            } else if user[..end].contains(&(b'@' as u16)) {
                (PCWSTR(user.as_ptr()), PCWSTR::null())
            } else {
                (PCWSTR(user.as_ptr()), w!("."))
            };
            let mut token = HANDLE::default();
            let blank = secret[0] == 0;
            let ok = LogonUserW(
                name,
                where_,
                PCWSTR(secret.as_ptr()),
                LOGON32_LOGON_INTERACTIVE,
                LOGON32_PROVIDER_DEFAULT,
                &mut token,
            )
            .is_ok();
            let last_error = GetLastError();
            clear(&mut secret);
            if ok {
                let _ = CloseHandle(token);
                return true;
            }
            if last_error == ERROR_ACCOUNT_RESTRICTION && blank {
                return true;
            }
            error = ERROR_LOGON_FAILURE.0;
        }
    }
    false
}
