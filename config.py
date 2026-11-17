"""Settings persistence. Plain JSON in Application Support, file mode 0600.

ponytail: tokens stored in plaintext (chmod 600), not encrypted. base64 is not
encryption and pretends to be. Add keyring only if a threat model demands it.
"""
import json
import os
from pathlib import Path

CONFIG_DIR = Path.home() / "Library" / "Application Support" / "unplugg"
CONFIG_FILE = CONFIG_DIR / "config.json"

DEFAULTS = {
    "activated": False,
    "monitor_power": True,       # alarm on power unplugged
    "monitor_lid": True,         # alarm on screen closed / moved
    "angle_threshold": 5,        # degrees of lid movement that counts as unsafe
    "play_sound": True,
    "send_location": True,       # attach a maps link (CoreLocation)
    "capture_media": True,       # photos + 10s video
    "record_audio": True,        # include mic audio in the video
    "camera": "",                # capture device names; "" = this Mac's built-in
    "mic": "",
    "speaker_name": "",          # output device for the siren ("" = system default)
    "media_dir": str(Path.home() / "Pictures" / "unplugg"),  # where photos/video are saved
    "telegram_destinations": [],  # [{"name", "token", "chat_id"}]
    "show_overlay": True,
    "lock_device": False,
    "launch_at_login": False,
    "settings_pane": 0,          # reopen Settings on the last pane used
}


def load_config() -> dict:
    if CONFIG_FILE.exists():
        with open(CONFIG_FILE) as f:
            return {**DEFAULTS, **json.load(f)}
    return dict(DEFAULTS)


def save_config(config: dict) -> None:
    CONFIG_DIR.mkdir(parents=True, exist_ok=True)
    with open(CONFIG_FILE, "w") as f:
        json.dump(config, f, indent=2)
    os.chmod(CONFIG_FILE, 0o600)
