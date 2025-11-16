import time
import cv2
from datetime import datetime
from pathlib import Path
from playsound import playsound
from telegram import send_notification
from utils import lock_screen

class AlarmHandler:
    def __init__(self, config):
        self.config = config
        self.media_dir = Path.home() / "Library" / "Application Support" / "unplugg" / "media"
        self.media_dir.mkdir(exist_ok=True)

    def trigger(self, event: str):
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        photo_path = None
        video_path = None

        if self.config["play_sound"]:
            try:
                playsound("assets/siren.wav", block=False)  # Non-blocking
            except Exception as e:
                print(f"Sound play failed: {e}")

        if self.config["capture_media"]:
            cap = cv2.VideoCapture(0)
            if cap.isOpened():
                # Photos
                for i in range(3):
                    ret, frame = cap.read()
                    if ret:
                        p_path = self.media_dir / f"{event}_{timestamp}_{i}.jpg"
                        cv2.imwrite(str(p_path), frame)
                        photo_path = p_path  # Last one

                # Video
                fourcc = cv2.VideoWriter_fourcc(*'mp4v')
                v_path = self.media_dir / f"{event}_{timestamp}.mp4"
                out = cv2.VideoWriter(str(v_path), fourcc, 20.0, (640, 480))
                start_time = time.time()
                while (time.time() - start_time) < 10:
                    ret, frame = cap.read()
                    if ret:
                        out.write(frame)
                out.release()
                video_path = v_path
                cap.release()

        send_notification(event, photo_path, video_path, self.config)

        if self.config["show_overlay"]:
            from overlay import show_overlay
            show_overlay()

        if self.config["lock_device"]:
            lock_screen()
