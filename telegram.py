import requests
from typing import Optional, List
from pathlib import Path

def send_notification(event: str, photo_path: Optional[Path] = None, video_path: Optional[Path] = None, config: dict) -> None:
    """Send Telegram message with optional media."""
    destinations = config["telegram_destinations"]
    if not destinations:
        return

    message = f"Unplugg Alert: {event.upper()} detected at {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}!"

    for dest in destinations:
        token = dest["token"]
        chat_id = dest["chat_id"]

        # Send text first
        url = f"https://api.telegram.org/bot{token}/sendMessage"
        data = {"chat_id": chat_id, "text": message}
        requests.post(url, data=data)

        # Send photo if available
        if photo_path and photo_path.exists():
            url = f"https://api.telegram.org/bot{token}/sendPhoto"
            with open(photo_path, "rb") as f:
                files = {"photo": f}
                data = {"chat_id": chat_id, "caption": f"{event} photo"}
                requests.post(url, data=data, files=files)

        # Send video if available
        if video_path and video_path.exists():
            url = f"https://api.telegram.org/bot{token}/sendVideo"
            with open(video_path, "rb") as f:
                files = {"video": f}
                data = {"chat_id": chat_id, "caption": f"{event} 10s video"}
                requests.post(url, data=data, files=files)
