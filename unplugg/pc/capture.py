"""Webcam capture: a short video with mic audio, plus stills taken while it records.

Qt Multimedia records the camera and microphone straight to H.264 + AAC MP4,
which Telegram plays inline. Stills come from the same camera feed at 1, 2 and
3 s, so the camera is opened once.

Qt's capture objects live on the main thread; capture() is called from the
alarm's background thread and waits for them.
"""
import threading
from datetime import datetime
from pathlib import Path

from PySide6.QtCore import QTimer, QUrl
from PySide6.QtMultimedia import (
    QAudioInput, QCamera, QMediaCaptureSession, QMediaFormat, QMediaRecorder, QVideoSink,
)

from . import devices
from .utils import call_after

_DEFAULT_DIR = Path.home() / "Pictures" / "unplugg"
BITRATE = 4_000_000  # ~4 MB a clip: quick to upload mid-theft
_recordings = []     # keep Qt's objects alive while they record


def _media_dir(config):
    d = Path(config.get("media_dir") or _DEFAULT_DIR).expanduser()
    d.mkdir(parents=True, exist_ok=True)
    return d


def capture(config, photos: int = 3, video_seconds: int = 10):
    """Record, then return (photo_path, video_path); either may be None."""
    media = _media_dir(config)
    stamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    done = threading.Event()
    result = {}
    call_after(_record, config, media, stamp, photos, video_seconds, result, done)
    done.wait(video_seconds + 15)
    return result.get("photo"), result.get("video")


def _record(config, media, stamp, photos, seconds, result, done):
    session = QMediaCaptureSession()
    camera = QCamera(devices.camera(config.get("camera", "")))
    session.setCamera(camera)
    mic = None
    if config.get("record_audio", True):
        mic = QAudioInput(devices.microphone(config.get("mic", "")))
        session.setAudioInput(mic)
    frames = QVideoSink()
    session.setVideoSink(frames)
    recorder = QMediaRecorder()
    session.setRecorder(recorder)
    video_format = QMediaFormat(QMediaFormat.FileFormat.MPEG4)
    video_format.setVideoCodec(QMediaFormat.VideoCodec.H264)
    video_format.setAudioCodec(QMediaFormat.AudioCodec.AAC)
    recorder.setMediaFormat(video_format)
    recorder.setVideoBitRate(BITRATE)
    video = media / f"{stamp}.mp4"
    recorder.setOutputLocation(QUrl.fromLocalFile(str(video)))
    keep = (session, camera, mic, frames, recorder)
    _recordings.append(keep)

    def still(i):
        """The live frame at i s; the last one is sent, once exposure has settled."""
        image = frames.videoFrame().toImage()
        if not image.isNull() and image.save(str(media / f"{stamp}_{i}.jpg"), quality=90):
            result["photo"] = media / f"{stamp}_{i}.jpg"

    def finished():
        if keep not in _recordings:  # an error can follow the stop
            return
        camera.stop()
        if video.exists():
            result["video"] = video
        _recordings.remove(keep)
        done.set()

    recorder.recorderStateChanged.connect(
        lambda state: state == QMediaRecorder.RecorderState.StoppedState and finished())
    camera.errorOccurred.connect(lambda *_: recorder.stop() if recorder.recorderState()
                                 != QMediaRecorder.RecorderState.StoppedState else finished())
    camera.start()
    recorder.record()
    for i in range(1, photos + 1):
        QTimer.singleShot(i * 1000, lambda i=i: still(i))
    QTimer.singleShot(seconds * 1000, recorder.stop)
