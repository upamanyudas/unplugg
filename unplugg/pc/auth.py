"""Require the owner before disarming or quitting while armed, so a thief can't
just click Deactivate to kill the alarm.

Windows: Windows Hello (PIN, face, fingerprint), else the account password in
Windows' own credential dialog, checked by LogonUser. Linux: polkit's dialog
for unplugg's action, which the .deb/.rpm install; without it (AppImage, brew),
a password box checked by PAM. Either way we never keep the password.

ponytail: as on macOS, an account with no password at all is let through
rather than lock the owner out of their own app.
"""
import getpass
import os
import subprocess
import time

from PySide6.QtCore import QEventLoop, QTimer

from .utils import WINDOWS

ACTION = "com.unplugg.app.disarm"  # packaging/com.unplugg.app.policy
_asking = False  # one dialog at a time


def require(reason="confirm it's you", parent=None) -> bool:
    """Wait for the owner. True = proceed, False = denied. `parent` is a window
    the dialog should sit above, such as the overlay."""
    global _asking
    if _asking:
        return False
    _asking = True
    try:
        if WINDOWS:
            return _hello(reason) if _hello_ready() else _windows_password(reason, parent)
        return _polkit(reason, parent)
    finally:
        _asking = False


def _wait(done, timeout=120) -> bool:
    """Run the event loop until done() or the timeout, so the siren keeps going."""
    loop, timer, deadline = QEventLoop(), QTimer(), time.monotonic() + timeout
    timer.timeout.connect(lambda: (done() or time.monotonic() > deadline) and loop.quit())
    timer.start(100)
    loop.exec()
    timer.stop()
    return done()


if WINDOWS:
    import ctypes
    from ctypes import wintypes

    from winrt.windows.foundation import AsyncStatus
    from winrt.windows.security.credentials.ui import (
        UserConsentVerificationResult, UserConsentVerifier, UserConsentVerifierAvailability,
    )

    credui, user32 = ctypes.windll.credui, ctypes.windll.user32
    advapi32 = ctypes.WinDLL("advapi32", use_last_error=True)
    user32.FindWindowW.restype = wintypes.HWND
    user32.SetForegroundWindow.argtypes = [wintypes.HWND]
    user32.SetWindowPos.argtypes = [wintypes.HWND, wintypes.HWND] + [ctypes.c_int] * 4 + [wintypes.UINT]
    CREDUIWIN_ENUMERATE_CURRENT_USER = 0x200
    ERROR_CANCELLED = 1223
    ERROR_LOGON_FAILURE = 1326
    ERROR_ACCOUNT_RESTRICTION = 1327  # the right password, but it's blank
    LOGON32_LOGON_INTERACTIVE, LOGON32_PROVIDER_DEFAULT = 2, 0
    HWND_TOPMOST, SWP_NOMOVE, SWP_NOSIZE = wintypes.HWND(-1), 0x2, 0x1

    class CREDUI_INFO(ctypes.Structure):
        _fields_ = [("cbSize", wintypes.DWORD), ("hwndParent", wintypes.HWND),
                    ("pszMessageText", wintypes.LPCWSTR), ("pszCaptionText", wintypes.LPCWSTR),
                    ("hbmBanner", wintypes.HBITMAP)]

    def _hello_ready():
        op = UserConsentVerifier.check_availability_async()
        _wait(lambda: op.status != AsyncStatus.STARTED, 5)
        return op.status == AsyncStatus.COMPLETED and \
            op.get_results() == UserConsentVerifierAvailability.AVAILABLE

    def _raise_hello():
        """A desktop app's Hello prompt can open behind; bring it up over the overlay.
        (RequestVerificationForWindowAsync, the owned prompt, needs Windows 11.)"""
        dialog = user32.FindWindowW("Credential Dialog Xaml Host", None)
        if dialog:
            user32.SetWindowPos(dialog, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE)
            user32.SetForegroundWindow(dialog)
        return dialog

    def _hello(reason):
        op = UserConsentVerifier.request_verification_async(f"unplugg wants to {reason}.")
        raised = False

        def done():
            nonlocal raised
            raised = raised or bool(_raise_hello())
            return op.status != AsyncStatus.STARTED
        if not _wait(done):
            op.cancel()
            return False
        return op.status == AsyncStatus.COMPLETED and \
            op.get_results() == UserConsentVerificationResult.VERIFIED

    def _windows_password(reason, parent):
        """Windows' credential dialog for this account, then LogonUser checks it.
        A wrong password asks again, with the error, three times."""
        info = CREDUI_INFO(ctypes.sizeof(CREDUI_INFO), parent, f"unplugg wants to {reason}.", "unplugg", None)
        error = 0
        for _ in range(3):
            package, blob, size = wintypes.ULONG(0), ctypes.c_void_p(), wintypes.ULONG(0)
            if credui.CredUIPromptForWindowsCredentialsW(
                    ctypes.byref(info), error, ctypes.byref(package), None, 0, ctypes.byref(blob),
                    ctypes.byref(size), None, CREDUIWIN_ENUMERATE_CURRENT_USER) != 0:
                return False  # cancelled
            user, domain, password = (ctypes.create_unicode_buffer(512) for _ in range(3))
            lengths = [wintypes.DWORD(512) for _ in range(3)]
            credui.CredUnPackAuthenticationBufferW(0, blob, size, user, ctypes.byref(lengths[0]),
                                                   domain, ctypes.byref(lengths[1]),
                                                   password, ctypes.byref(lengths[2]))
            ctypes.memset(blob, 0, size.value)
            ctypes.windll.ole32.CoTaskMemFree(blob)
            name, where = user.value, domain.value or None
            if "\\" in name:  # DOMAIN\user or MicrosoftAccount\email
                where, name = name.split("\\", 1)
            elif "@" not in name and not where:
                where = "."  # this PC's own accounts
            token = wintypes.HANDLE()
            ok = advapi32.LogonUserW(name, where, password.value, LOGON32_LOGON_INTERACTIVE,
                                     LOGON32_PROVIDER_DEFAULT, ctypes.byref(token))
            error, blank = ctypes.get_last_error(), not password.value
            ctypes.memset(password, 0, ctypes.sizeof(password))
            if ok:
                ctypes.windll.kernel32.CloseHandle(token)
                return True
            if error == ERROR_ACCOUNT_RESTRICTION and blank:
                return True  # no password on this account
            error = ERROR_LOGON_FAILURE
        return False
else:
    def _polkit(reason, parent):
        """polkit's own dialog: the desktop's agent shows it, fingerprint included."""
        with open("/proc/self/stat") as f:
            start = f.read().rsplit(")", 1)[1].split()[19]  # field 22, start time: pid alone is racy
        try:
            check = subprocess.Popen(["pkcheck", "--action-id", ACTION, "--allow-user-interaction",
                                      "--process", f"{os.getpid()},{start},{os.getuid()}"],
                                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        except FileNotFoundError:
            return _pam(reason, parent)
        if not _wait(lambda: check.poll() is not None):
            check.kill()
            return False
        if check.returncode in (2, 127):  # no agent, or no unplugg policy (AppImage, brew)
            return _pam(reason, parent)
        return check.returncode == 0

    def _pam(reason, parent):
        """Our own password box, checked by PAM's login stack."""
        import pam
        from PySide6.QtWidgets import QInputDialog, QLineEdit
        prompt = f"unplugg wants to {reason}. Enter your password."
        for _ in range(3):
            password, ok = QInputDialog.getText(parent, "unplugg", prompt, QLineEdit.EchoMode.Password)
            if not ok:
                return False
            if pam.authenticate(getpass.getuser(), password, service="login", resetcreds=False):
                return True
            prompt = "That password didn't work. Try again."
        return False
