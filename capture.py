"""Webcam capture: a short video (with mic audio) + a few stills pulled from it.

Primary path is ffmpeg's avfoundation input — it records the chosen camera and
microphone into one mp4 with audio, which OpenCV alone cannot do. Stills are
extracted from that recording so the camera is opened once. If ffmpeg is missing
or errors, we fall back to OpenCV for a silent, default-camera video.
"""
import subprocess
from datetime import datetime
from pathlib import Path

import devices

_DEFAULT_DIR = Path.home() / "Pictures" / "unplugg"


def _media_dir(config):
    d = Path(config.get("media_dir") or _DEFAULT_DIR).expanduser()
    d.mkdir(parents=True, exist_ok=True)
    return d


def capture(config, photos: int = 3, video_seconds: int = 10):
    """Record, then return (photo_path, video_path); either may be None."""
    media = _media_dir(config)
    stamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    video = media / f"{stamp}.mp4"

    if devices.FFMPEG and _ffmpeg_record(config, video, video_seconds):
        photo = _ffmpeg_stills(media, video, stamp, photos)
        return photo, video
    return _opencv_fallback(media, stamp, video, photos, video_seconds)


def _ffmpeg_record(config, video, seconds) -> bool:
    vid = config.get("camera_index", 0)
    aud = config.get("mic_index", 0) if config.get("record_audio", True) else "none"
    cmd = [devices.FFMPEG, "-y", "-f", "avfoundation", "-framerate", "30",
           "-i", f"{vid}:{aud}", "-t", str(seconds),
           "-pix_fmt", "yuv420p", "-movflags", "+faststart", str(video)]
    try:
        r = subprocess.run(cmd, capture_output=True, timeout=seconds + 25)
        return r.returncode == 0 and video.exists()
    except (subprocess.TimeoutExpired, OSError):
        return False


def _ffmpeg_stills(media, video, stamp, photos):
    pattern = str(media / f"{stamp}_%d.jpg")
    subprocess.run([devices.FFMPEG, "-y", "-i", str(video), "-vf", "fps=1",
                    "-frames:v", str(photos), pattern],
                   capture_output=True, timeout=30)
    shots = sorted(media.glob(f"{stamp}_*.jpg"))
    return shots[-1] if shots else None


def _opencv_fallback(media, stamp, video, photos, seconds):
    # ponytail: video-only fallback; audio needs ffmpeg, which we'd have used.
    import time
    import cv2
    cam = cv2.VideoCapture(0)
    if not cam.isOpened():
        return None, None
    photo = None
    for i in range(photos):
        ok, frame = cam.read()
        if ok:
            photo = media / f"{stamp}_{i}.jpg"
            cv2.imwrite(str(photo), frame)
    w = int(cam.get(cv2.CAP_PROP_FRAME_WIDTH)) or 640
    h = int(cam.get(cv2.CAP_PROP_FRAME_HEIGHT)) or 480
    out = cv2.VideoWriter(str(video), cv2.VideoWriter_fourcc(*"mp4v"), 20.0, (w, h))
    end = time.time() + seconds
    wrote = False
    while time.time() < end:
        ok, frame = cam.read()
        if ok:
            out.write(frame); wrote = True
    out.release(); cam.release()
    return photo, (video if wrote else None)
