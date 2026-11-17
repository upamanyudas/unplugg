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


# ---- setup panel helpers ---------------------------------------------------
def _call(token, method, **data):
    """One Bot API call -> (result, None) or (None, Telegram's reason)."""
    try:
        r = requests.post(f"https://api.telegram.org/bot{token}/{method}", data=data,
                          timeout=15).json()
    except (requests.RequestException, ValueError):
        return None, "Couldn't reach Telegram. Check your internet connection."
    return (r["result"], None) if r.get("ok") else (None, r.get("description", "Unknown error"))


def find_chat_id(token):
    """Chat id of the newest message sent to the bot, or None (Uptime Kuma's Auto Get)."""
    updates, _ = _call(token, "getUpdates")
    for u in reversed(updates or []):
        post = u.get("message") or u.get("channel_post")
        if post:
            return str(post["chat"]["id"])
    return None


def check(token, chat_id):
    """Can this bot reach this chat? None if yes, else why not. Sends nothing."""
    return _call(token, "getChat", chat_id=chat_id)[1]


def send_test(token, chat_id):
    """None once the test message landed, else why it didn't."""
    return _call(token, "sendMessage", chat_id=chat_id,
                 text="✅ unplugg test: alerts will arrive here.")[1]
