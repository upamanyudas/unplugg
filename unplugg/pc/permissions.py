"""Windows' privacy switches for the camera, microphone and location.

Desktop apps never get a prompt: each is a switch in Settings › Privacy, for
the whole device and for all desktop apps at once. Read from the registry,
where Settings keeps them. Linux has no such switches.
"""
from .utils import WINDOWS, open_url

STORE = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore"
NAMES = {"camera": "webcam", "microphone": "microphone", "location": "location"}


def status(key) -> str:
    """'granted' or 'denied'."""
    if not WINDOWS:
        return "granted"
    import winreg
    for root, path in ((winreg.HKEY_LOCAL_MACHINE, rf"{STORE}\{NAMES[key]}"),  # this device
                       (winreg.HKEY_CURRENT_USER, rf"{STORE}\{NAMES[key]}"),   # this account
                       (winreg.HKEY_CURRENT_USER, rf"{STORE}\{NAMES[key]}\NonPackaged")):  # desktop apps
        try:
            with winreg.OpenKey(root, path) as k:
                if winreg.QueryValueEx(k, "Value")[0] == "Deny":
                    return "denied"
        except OSError:
            continue
    return "granted"


def open_settings(key):
    open_url(f"ms-settings:privacy-{NAMES[key]}")
