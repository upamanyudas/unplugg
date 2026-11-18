"""Power policy while armed, and the power events that fire the alarm.

Armed, the laptop has to stay awake and ignore its lid and buttons, or it
would sleep before unplugg could react; a shutdown attempt fires the alarm.

Windows: SetThreadExecutionState holds off idle sleep, the lid, power and sleep
buttons are set to Do nothing (saved to the config first, so a crash while
armed is undone at the next launch), shutdown is refused with a reason, and a
sleep that starts anyway fires the alarm. Lid state comes as a power-setting
notification.

Linux: systemd-logind inhibitor locks. Sleep, idle and the lid and power keys
are blocked; shutdown is delayed, and its PrepareForShutdown signal fires the
alarm. Lid state is ACPI's.
"""
import ctypes
import glob
import os
import subprocess
from uuid import UUID

from PySide6.QtCore import QAbstractNativeEventFilter, QObject, Slot
from PySide6.QtGui import QWindow

from ..config import save_config
from .utils import WINDOWS

_on_attempt = None  # fires the alarm
_armed = False
_keep = []          # Qt objects that must outlive watch()

if WINDOWS:
    from ctypes import wintypes

    kernel32, user32, powrprof = ctypes.windll.kernel32, ctypes.windll.user32, ctypes.windll.powrprof
    user32.RegisterPowerSettingNotification.argtypes = [wintypes.HANDLE, ctypes.c_char_p, wintypes.DWORD]
    user32.ShutdownBlockReasonCreate.argtypes = [wintypes.HWND, wintypes.LPCWSTR]
    kernel32.SetThreadExecutionState.argtypes = [wintypes.DWORD]
    kernel32.LocalFree.argtypes = [ctypes.c_void_p]

    WM_QUERYENDSESSION = 0x0011
    WM_POWERBROADCAST = 0x0218
    PBT_APMSUSPEND = 0x0004
    PBT_POWERSETTINGCHANGE = 0x8013
    ES_CONTINUOUS, ES_SYSTEM_REQUIRED = 0x80000000, 0x00000001
    LID_STATE = UUID("ba3e0f4d-b817-4094-a2d1-d56379e6a0f3").bytes_le  # GUID_LIDSWITCH_STATE_CHANGE
    BUTTONS = UUID("4f971e89-eebd-4455-a8de-9e59040e7347").bytes_le    # SUB_BUTTONS
    ACTIONS = ("5ca83367-6e45-459f-a27b-476b1d01c936",   # LIDACTION
               "7648efa3-dd9c-4e3e-b566-50f929386280",   # PBUTTONACTION
               "96996bc0-ad50-47ec-923b-6f41874dd9eb")   # SBUTTONACTION
    DO_NOTHING = 0
    _lid_closed = False

    class _Events(QAbstractNativeEventFilter):
        def nativeEventFilter(self, event_type, message):
            global _lid_closed
            msg = wintypes.MSG.from_address(int(message))
            if msg.message == WM_POWERBROADCAST and msg.wParam == PBT_POWERSETTINGCHANGE:
                if ctypes.string_at(msg.lParam, 16) == LID_STATE:
                    # POWERBROADCAST_SETTING: the GUID, a DWORD length, then the state
                    _lid_closed = ctypes.c_ulong.from_address(msg.lParam + 20).value == 0
            elif msg.message == WM_POWERBROADCAST and msg.wParam == PBT_APMSUSPEND and _armed:
                _on_attempt()  # can't be refused; ~2 s before it sleeps
            elif msg.message == WM_QUERYENDSESSION and _armed:
                # only a visible window can refuse, so this sticks while the overlay is up
                user32.ShutdownBlockReasonCreate(msg.hWnd, "unplugg is armed")
                _on_attempt()
                return True, 0  # FALSE: don't end the session
            return False, 0

    def _scheme():
        pointer = ctypes.c_void_p()
        powrprof.PowerGetActiveScheme(None, ctypes.byref(pointer))
        scheme = ctypes.string_at(pointer, 16)
        kernel32.LocalFree(pointer)
        return scheme

    def _action(scheme, setting, ac, value=None):
        """Read (value None) or write one button action of the active power plan."""
        guid = UUID(setting).bytes_le
        if value is None:
            out = wintypes.DWORD()
            read = powrprof.PowerReadACValueIndex if ac else powrprof.PowerReadDCValueIndex
            read(None, scheme, BUTTONS, guid, ctypes.byref(out))
            return out.value
        write = powrprof.PowerWriteACValueIndex if ac else powrprof.PowerWriteDCValueIndex
        write(None, scheme, BUTTONS, guid, value)
else:
    _LID = glob.glob("/proc/acpi/button/lid/*/state")
    _inhibitors = []
    LOGIND = ("org.freedesktop.login1", "/org/freedesktop/login1", "org.freedesktop.login1.Manager")

    class _Logind(QObject):
        @Slot(bool)
        def prepare(self, starting):
            if starting and _armed:
                _on_attempt()


def watch(app, on_attempt):
    """Listen for lid and shutdown events for as long as the app runs."""
    global _on_attempt
    _on_attempt = on_attempt
    if WINDOWS:
        events, window = _Events(), QWindow()
        app.installNativeEventFilter(events)
        # a hidden native window to receive the lid notifications; it sends the current state at once
        user32.RegisterPowerSettingNotification(int(window.winId()), LID_STATE, 0)
        _keep.extend([events, window])
    else:
        from PySide6.QtDBus import QDBusConnection
        logind = _Logind()
        QDBusConnection.systemBus().connect(*LOGIND, "PrepareForShutdown", logind, b"1prepare(bool)")
        _keep.append(logind)


def lid_closed() -> bool:
    if WINDOWS:
        return _lid_closed
    for path in _LID:
        with open(path) as f:
            return "closed" in f.read()
    return False


def hold(config):
    """Armed: stay awake, ignore the lid and buttons, refuse or delay shutdown."""
    global _armed
    _armed = True
    if WINDOWS:
        kernel32.SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED)
        scheme = _scheme()
        config["power_restore"] = {f"{s}/{ac}": _action(scheme, s, ac) for s in ACTIONS for ac in (1, 0)}
        save_config(config)
        for s in ACTIONS:
            for ac in (1, 0):
                _action(scheme, s, ac, DO_NOTHING)
        powrprof.PowerSetActiveScheme(None, scheme)
        return
    # tail exits with this process, so even a crash can't leave the laptop sleepless
    until_we_exit = ["tail", f"--pid={os.getpid()}", "-f", "/dev/null"]
    for what, mode in (("sleep:idle:handle-lid-switch:handle-power-key:handle-suspend-key:"
                        "handle-hibernate-key", "block"), ("shutdown", "delay")):
        try:
            _inhibitors.append(subprocess.Popen(
                ["systemd-inhibit", f"--what={what}", "--who=unplugg", "--why=The alarm is armed",
                 f"--mode={mode}", *until_we_exit]))
        except FileNotFoundError:  # no systemd
            return


def release(config):
    """Disarmed, or a crash while armed: put everything back."""
    global _armed
    _armed = False
    if WINDOWS:
        kernel32.SetThreadExecutionState(ES_CONTINUOUS)
        if config.get("power_restore"):
            scheme = _scheme()
            for key, value in config["power_restore"].items():
                setting, ac = key.split("/")
                _action(scheme, setting, ac == "1", value)
            powrprof.PowerSetActiveScheme(None, scheme)
            config["power_restore"] = {}
            save_config(config)
        return
    for inhibitor in _inhibitors:
        inhibitor.terminate()
    _inhibitors.clear()
