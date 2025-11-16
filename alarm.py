import os
from datetime import datetime
from playsound import playsound
from alarm import Alarm

class AlarmHandler:
    def __init__(self, config: dict):
        self.config = config
        self.media_dir = Path.home() / "Library" / "Application Support" / "unplugg" / "media"
        self.media_dir.mkdir(exist_ok=True)

    def trigger(self, event: str):
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        photo_path = None
        video_path = None

        if self.config["play_sound"]:
            try:
                playsound("assets/siren.wav")  # Loops if needed; add threading for background
            except Exception as e:
                print(f"Sound failed: {e}")

        if self.config["capture_media"]:
            from cv2 import VideoCapture, imwrite, VideoWriter_fourcc
            cap = VideoCapture(0)  # Webcam index 0
            if not cap.isOpened():
                print("Webcam access denied")
                return

            # Capture series of photos (e.g., 3 quick shots)
            for i in range(3):
                ret, frame = cap.read()
                if ret:
                    photo_path = self.media_dir / f"{event}_{timestamp}_{i}.jpg"
                    imwrite(str(photo_path), frame)

            # Capture 10s video
            fourcc = VideoWriter_fourcc(*'mp4v')
            video_path = self.media_dir / f"{event}_{timestamp}.mp4"
            out = cv2.VideoWriter(str(video_path), fourcc, 20.0, (640, 480))
            start = time.time()
            while (time.time() - start) < 10:
                ret, frame = cap.read()
                if ret:
                    out.write(frame)
            out.release()
            cap.release()

        # Always send Telegram (even if no media)
        from telegram import send_notification  # Import here to avoid circular
        send_notification(event, photo_path, video_path, self.config)

        # Overlay/Lock
        if self.config["show_overlay"]:
            from overlay import show_overlay
            show_overlay()
        if self.config["lock_device"]:
            from utils import lock_screen
            lock_screen()
