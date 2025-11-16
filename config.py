import json
import os
import base64  # Simple encryption demo
from pathlib import Path

CONFIG_DIR = Path.home() / "Library" / "Application Support" / "unplugg"
CONFIG_DIR.mkdir(exist_ok=True)
CONFIG_FILE = CONFIG_DIR / "config.json"

DEFAULTS = {
    "activated": False,
    "monitor_power": True,
    "monitor_lid": True,
    "play_sound": True,
    "capture_media": True,
    "telegram_destinations": [],  # List of {"name": str, "token": str (encrypted), "chat_id": str}
    "show_overlay": True,
    "lock_device": False,
    "launch_at_login": False,
}

def encrypt_token(token: str) -> str:
    return base64.b64encode(token.encode()).decode()  # Demo; use keyring in prod

def decrypt_token(encrypted: str) -> str:
    return base64.b64decode(encrypted.encode()).decode()

def load_config() -> dict:
    if CONFIG_FILE.exists():
        with open(CONFIG_FILE, "r") as f:
            config = json.load(f)
            # Decrypt tokens
            for dest in config.get("telegram_destinations", []):
                if "token" in dest:
                    dest["token"] = decrypt_token(dest["token"])
            return {**DEFAULTS, **config}
    return DEFAULTS

def save_config(config: dict) -> None:
    # Encrypt tokens before save
    save_config = config.copy()
    for dest in save_config.get("telegram_destinations", []):
        if "token" in dest:
            dest["token"] = encrypt_token(dest["token"])
    with open(CONFIG_FILE, "w") as f:
        json.dump(save_config, f, indent=2)
