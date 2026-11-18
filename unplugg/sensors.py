"""Hardware state: laptop or not, AC power, lid angle (2019+ Macs), clamshell open/closed.

Lid angle comes from an undocumented HID sensor that only exists on 2019+ Macs.
Where it's missing we fall back to the clamshell (closed/open) boolean, which is
available on every Mac laptop. So the degree threshold is used when we can, and
"lid closed" detection everywhere else.

Power is one IOKit call via ctypes (~0.1 ms), so the monitor can poll 5×/s.
ponytail: the clamshell fallback (older Macs) still shells out to ioreg — one
spawn per poll beats binding IORegistry property reads.
"""
import ctypes
import subprocess

try:
    from pybooklid import read_lid_angle, is_sensor_available
    _HAS_ANGLE = is_sensor_available()
except Exception:  # library missing or import failed
    _HAS_ANGLE = False
    def read_lid_angle():
        return None


_IOKIT = ctypes.CDLL("/System/Library/Frameworks/IOKit.framework/IOKit")
_IOKIT.IOPSGetTimeRemainingEstimate.restype = ctypes.c_double
UNLIMITED = -2.0  # kIOPSTimeRemainingUnlimited (IOPowerSources.h): on external power


def has_angle_sensor() -> bool:
    return _HAS_ANGLE


def is_laptop() -> bool:
    """Every MacBook lists an InternalBattery; desktops have none. hw.model can't
    tell: since M2, MacBooks and desktops are both "MacNN,N"."""
    out = subprocess.run(["pmset", "-g", "batt"], capture_output=True, text=True).stdout
    return "InternalBattery" in out


def is_plugged_in() -> bool:
    """True on AC power, i.e. no time limit on the power source."""
    return _IOKIT.IOPSGetTimeRemainingEstimate() == UNLIMITED


def lid_angle():
    """Current lid angle in degrees, or None if no sensor."""
    return read_lid_angle() if _HAS_ANGLE else None


def acceleration():
    """None: the M2+ accelerometer is undocumented HID that only root may read."""
    return None


def is_lid_closed() -> bool:
    """Clamshell closed. Universal fallback when there's no angle sensor."""
    out = subprocess.run(
        ["ioreg", "-r", "-k", "AppleClamshellState", "-d", "4"],
        capture_output=True, text=True,
    ).stdout
    return '"AppleClamshellState" = Yes' in out


def demo():
    # ponytail: runnable self-check — prove the readers return sane types.
    assert isinstance(is_plugged_in(), bool)
    assert isinstance(is_lid_closed(), bool)
    a = lid_angle()
    assert a is None or isinstance(a, (int, float))
    print(f"laptop={is_laptop()} plugged={is_plugged_in()} closed={is_lid_closed()} "
          f"angle={a} angle_sensor={has_angle_sensor()}")


if __name__ == "__main__":
    demo()
