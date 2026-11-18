"""Hardware state on Windows and Linux: laptop or not, AC power, lid, motion.

Neither OS exposes a lid angle, so the lid counts as closed or open, like on
older Macs. Motion comes from the accelerometer most 2-in-1s carry; ordinary
laptops have none and the setting stays hidden.
"""
import glob
import os

import psutil

from . import power
from .utils import WINDOWS

G = 9.80665  # m/s² per g


def has_angle_sensor() -> bool:
    return False


def lid_angle():
    return None


def is_laptop() -> bool:
    """Laptops have a battery, desktops don't."""
    return psutil.sensors_battery() is not None


def is_plugged_in() -> bool:
    battery = psutil.sensors_battery()
    return battery is None or bool(battery.power_plugged)  # a desktop is always on power


def is_lid_closed() -> bool:
    return power.lid_closed()


if WINDOWS:
    try:
        from winrt.windows.devices.sensors import Accelerometer
        _accelerometer = Accelerometer.get_default()  # None without one
    except (ImportError, OSError):
        _accelerometer = None

    def acceleration():
        """(x, y, z) in g, or None without an accelerometer."""
        reading = _accelerometer and _accelerometer.get_current_reading()
        return (reading.acceleration_x, reading.acceleration_y, reading.acceleration_z) if reading else None
else:
    # the kernel's IIO sysfs files, readable by everyone; raw × scale = m/s²
    _IIO = next((os.path.dirname(p) for p in glob.glob("/sys/bus/iio/devices/iio:device*/in_accel_x_raw")), None)

    def _read(name, default=None):
        try:
            with open(os.path.join(_IIO, name)) as f:
                return float(f.read())
        except OSError:
            return default

    def acceleration():
        """(x, y, z) in g, or None without an accelerometer."""
        if not _IIO:
            return None
        scale = _read("in_accel_scale") or _read("in_accel_x_scale", 1.0)
        return tuple(_read(f"in_accel_{axis}_raw", 0) * scale / G for axis in "xyz")


def demo():
    # runnable self-check: prove the readers return sane types
    assert isinstance(is_plugged_in(), bool)
    assert isinstance(is_lid_closed(), bool)
    a = acceleration()
    assert a is None or len(a) == 3
    print(f"laptop={is_laptop()} plugged={is_plugged_in()} closed={is_lid_closed()} motion={a}")


if __name__ == "__main__":
    demo()
