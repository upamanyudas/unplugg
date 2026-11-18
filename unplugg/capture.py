"""Webcam capture: a short video (with mic audio) + a few stills pulled from it.

AVFoundation records the chosen camera and microphone into one movie, in-process:
nothing to install, and the grants are unplugg's own. Stills are taken from that
recording, so the camera is opened once.

ponytail: AVCaptureMovieFileOutput only writes QuickTime and Telegram only plays
MP4 inline, so the .mov is remuxed (passthrough, no re-encode).
"""
import threading
from datetime import datetime
from pathlib import Path

from AVFoundation import (
    AVAssetExportPresetPassthrough, AVAssetExportSession, AVAssetImageGenerator, AVCaptureDevice,
    AVCaptureDeviceInput, AVCaptureMovieFileOutput, AVCaptureSession, AVFileTypeMPEG4,
    AVMediaTypeAudio, AVMediaTypeVideo, AVURLAsset, AVVideoAverageBitRateKey, AVVideoCodecKey,
    AVVideoCodecTypeH264, AVVideoCompressionPropertiesKey,
)
from CoreMedia import CMTimeMakeWithSeconds
from Foundation import NSObject, NSURL
from Quartz import CGImageDestinationAddImage, CGImageDestinationCreateWithURL, CGImageDestinationFinalize

from . import devices

_DEFAULT_DIR = Path.home() / "Pictures" / "unplugg"
GRANTED = 3  # AVAuthorizationStatusAuthorized
# the camera's default is ~21 Mbit/s at 1080p, 27 MB a clip: slow to upload mid-theft
VIDEO = {AVVideoCodecKey: AVVideoCodecTypeH264,
         AVVideoCompressionPropertiesKey: {AVVideoAverageBitRateKey: 4_000_000}}


class _Recorded(NSObject):
    """AVFoundation calls this on any thread once the movie is fully written."""

    def captureOutput_didFinishRecordingToOutputFileAtURL_fromConnections_error_(self, *_):
        self.done.set()


def _media_dir(config):
    d = Path(config.get("media_dir") or _DEFAULT_DIR).expanduser()
    d.mkdir(parents=True, exist_ok=True)
    return d


def capture(config, photos: int = 3, video_seconds: int = 10):
    """Record, then return (photo_path, video_path); either may be None."""
    media = _media_dir(config)
    stamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    mov, video = media / f"{stamp}.mov", media / f"{stamp}.mp4"
    if not _record(config, mov, video_seconds):
        return None, None
    if _remux(mov, video):
        mov.unlink()
    else:
        video = mov  # still sent, as a file
    return _stills(video, media, stamp, photos), video


def _record(config, path, seconds) -> bool:
    cam = _input(AVMediaTypeVideo, devices.camera(config.get("camera", "")))
    if not cam:
        return False
    session = AVCaptureSession.alloc().init()
    session.addInput_(cam)
    mic = config.get("record_audio", True) and _input(
        AVMediaTypeAudio, devices.microphone(config.get("mic", "")))
    if mic:
        session.addInput_(mic)
    output = AVCaptureMovieFileOutput.alloc().init()
    output.setMaxRecordedDuration_(CMTimeMakeWithSeconds(seconds, 600))
    session.addOutput_(output)
    output.setOutputSettings_forConnection_(VIDEO, output.connectionWithMediaType_(AVMediaTypeVideo))
    session.startRunning()
    recorded = _Recorded.alloc().init()
    recorded.done = threading.Event()
    output.startRecordingToOutputFileURL_recordingDelegate_(_url(path), recorded)
    recorded.done.wait(seconds + 15)
    session.stopRunning()
    return path.exists()


def _input(media, name):
    """Capture input for a device that's already allowed, else None. Never asks:
    a permission prompt mid-alarm would only tip off the thief."""
    device = devices.device(media, name)
    if device is None or AVCaptureDevice.authorizationStatusForMediaType_(media) != GRANTED:
        return None
    return AVCaptureDeviceInput.deviceInputWithDevice_error_(device, None)[0]


def _remux(src, dst) -> bool:
    export = AVAssetExportSession.exportSessionWithAsset_presetName_(
        _asset(src), AVAssetExportPresetPassthrough)
    export.setOutputURL_(_url(dst))
    export.setOutputFileType_(AVFileTypeMPEG4)
    done = threading.Event()
    export.exportAsynchronouslyWithCompletionHandler_(done.set)
    done.wait(30)
    return dst.exists()


def _stills(video, media, stamp, count):
    """A frame at 1 s, 2 s, 3 s…; returns the last, once exposure has settled."""
    frames = AVAssetImageGenerator.assetImageGeneratorWithAsset_(_asset(video))
    photo = None
    for i in range(1, count + 1):
        image = frames.copyCGImageAtTime_actualTime_error_(CMTimeMakeWithSeconds(i, 600), None, None)[0]
        if image:
            photo = media / f"{stamp}_{i}.jpg"
            jpeg = CGImageDestinationCreateWithURL(_url(photo), "public.jpeg", 1, None)
            CGImageDestinationAddImage(jpeg, image, None)
            CGImageDestinationFinalize(jpeg)
    return photo


def _url(path):
    return NSURL.fileURLWithPath_(str(path))


def _asset(path):
    return AVURLAsset.URLAssetWithURL_options_(_url(path), None)
