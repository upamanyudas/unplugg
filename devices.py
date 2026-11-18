"""Enumerate capture/output devices for the Preferences pickers.

Cameras + microphones come from AVFoundation and are saved by name: names
survive an iPhone or AirPods showing up, list indices shift (index 0 was the
iPhone mic). Speakers come from CoreAudio, also by name, and the siren's pick
becomes the default output while it sounds.
"""
import struct

import objc
from AVFoundation import AVCaptureDevice, AVMediaTypeAudio, AVMediaTypeVideo
from CoreAudio import (
    AudioObjectGetPropertyData, AudioObjectGetPropertyDataSize, AudioObjectPropertyAddress,
    AudioObjectSetPropertyData, kAudioDevicePropertyStreams,
    kAudioHardwarePropertyDefaultOutputDevice, kAudioHardwarePropertyDevices,
    kAudioObjectPropertyElementMain, kAudioObjectPropertyName, kAudioObjectPropertyScopeGlobal,
    kAudioObjectPropertyScopeOutput, kAudioObjectSystemObject,
)

BUILT_IN = int.from_bytes(b"bltn", "big")  # kIOAudioDeviceTransportTypeBuiltIn


def _capture(media):
    return list(AVCaptureDevice.devicesWithMediaType_(media))


def cameras():
    return [d.localizedName() for d in _capture(AVMediaTypeVideo)]


def microphones():
    return [d.localizedName() for d in _capture(AVMediaTypeAudio)]


def camera(name):
    return _pick(_capture(AVMediaTypeVideo), name)


def microphone(name):
    return _pick(_capture(AVMediaTypeAudio), name)


def _pick(devs, name):
    """`name` while it's connected, else this Mac's built-in device ("" = built-in),
    so a thief pulling the dock cable can't leave the alarm without a camera."""
    if name in [d.localizedName() for d in devs]:
        return name
    built_in = [d for d in devs if d.transportType() == BUILT_IN] or devs
    return built_in[0].localizedName() if built_in else None


def device(media, name):
    """The connected capture device called `name`, or None."""
    return next((d for d in _capture(media) if d.localizedName() == name), None)


def speakers():
    return list(_outputs())


def current_speaker():
    default = struct.unpack("I", _get(kAudioObjectSystemObject, kAudioHardwarePropertyDefaultOutputDevice))[0]
    return next((name for name, i in _outputs().items() if i == default), None)


def set_speaker(name):
    device_id = _outputs().get(name)
    if device_id is not None:
        AudioObjectSetPropertyData(kAudioObjectSystemObject, _address(
            kAudioHardwarePropertyDefaultOutputDevice), 0, b"", 4, struct.pack("I", device_id))


def _outputs():
    """{name: CoreAudio id} of every device that can play sound."""
    raw = _get(kAudioObjectSystemObject, kAudioHardwarePropertyDevices)
    ids = struct.unpack(f"{len(raw) // 4}I", raw)
    return {_name(i): i for i in ids
            if _get(i, kAudioDevicePropertyStreams, kAudioObjectPropertyScopeOutput)}


def _name(device_id):
    pointer = struct.unpack("P", _get(device_id, kAudioObjectPropertyName))[0]
    return str(objc.objc_object(c_void_p=pointer))  # the bytes hold a CFStringRef


def _address(selector, scope=kAudioObjectPropertyScopeGlobal):
    return AudioObjectPropertyAddress(selector, scope, kAudioObjectPropertyElementMain)


def _get(obj, selector, scope=kAudioObjectPropertyScopeGlobal):
    """A CoreAudio property's raw bytes; b"" when it has none."""
    address = _address(selector, scope)
    err, size = AudioObjectGetPropertyDataSize(obj, address, 0, b"", None)
    if err or not size:
        return b""
    err, _, data = AudioObjectGetPropertyData(obj, address, 0, b"", size, None)
    return b"" if err else bytes(data)
