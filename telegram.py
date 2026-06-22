"""Send the alert (text + optional photo/video) to every configured Telegram
destination via the Bot API."""
from datetime import datetime
from pathlib import Path

import requests

EVENT_LABEL = {
    "power_unplugged": "Power adapter unplugged",
    "screen_moved": "Screen moved",
    "screen_closed": "Screen closed",
}


def send_alert(event: str, config: dict, photo=None, video=None, location_url=None):
    dests = config.get("telegram_destinations", [])
    if not dests:
        return

    when = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    lines = [f"🚨 unplugg alert: {EVENT_LABEL.get(event, event)}", f"🕒 {when}"]
    if location_url:
        lines.append(f"📍 {location_url}")
    text = "\n".join(lines)

    for d in dests:
        base = f"https://api.telegram.org/bot{d['token']}"
        chat = d["chat_id"]
        _post(f"{base}/sendMessage", {"chat_id": chat, "text": text})
        _send_file(f"{base}/sendPhoto", chat, "photo", photo)
        _send_file(f"{base}/sendVideo", chat, "video", video)


def _post(url, data, files=None):
    try:
        requests.post(url, data=data, files=files, timeout=30)
    except requests.RequestException as e:
        print(f"telegram send failed: {e}")  # don't crash the alarm on a flaky network


def _send_file(url, chat, field, path):
    if path and Path(path).exists():
        with open(path, "rb") as f:
            _post(url, {"chat_id": chat}, files={field: f})


def test_credentials(token: str, chat_id: str) -> bool:
    """Used by the setup panel to validate before saving."""
    try:
        r = requests.post(
            f"https://api.telegram.org/bot{token}/sendMessage",
            data={"chat_id": chat_id, "text": "✅ unplugg connected."},
            timeout=15,
        )
        return r.ok
    except requests.RequestException:
        return False
