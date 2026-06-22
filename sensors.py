"""Hardware state: AC power, lid angle (2019+ Macs), clamshell open/closed.

Lid angle comes from an undocumented HID sensor that only exists on 2019+ Macs.
Where it's missing we fall back to the clamshell (closed/open) boolean, which is
available on every Mac laptop. So the degree threshold is used when we can, and
"lid closed" detection everywhere else.

ponytail: shells out to pmset/ioreg instead of binding IOKit power APIs — two
process spawns per poll is nothing at a 1s interval, and the strings are stable.
"""
import subprocess

try:
    from pybooklid import read_lid_angle, is_sensor_available
    _HAS_ANGLE = is_sensor_available()
except Exception:  # library missing or import failed
    _HAS_ANGLE = False
    def read_lid_angle():
        return None


def has_angle_sensor() -> bool:
    return _HAS_ANGLE


def is_plugged_in() -> bool:
    """True if running on AC power. `pmset -g batt` prints "Now drawing from
    'AC Power'" when plugged — `pmset -g ac` does NOT contain that string, which
    was the false "plug in first" bug."""
    out = subprocess.run(["pmset", "-g", "batt"], capture_output=True, text=True).stdout
    return "AC Power" in out


def lid_angle():
    """Current lid angle in degrees, or None if no sensor."""
    return read_lid_angle() if _HAS_ANGLE else None


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
    print(f"plugged={is_plugged_in()} closed={is_lid_closed()} "
          f"angle={a} angle_sensor={has_angle_sensor()}")


if __name__ == "__main__":
    demo()
