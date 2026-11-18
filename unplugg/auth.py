"""Require the device owner (Touch ID or login password) before disarming or
quitting while armed — so a thief can't just click Deactivate to kill the alarm.

ponytail: LocalAuthentication is the native gate; we never see or handle the
password. If the Mac has no passcode/biometrics set up we allow the action
rather than lock the owner out of their own app.
"""
import time

from Foundation import NSDate, NSDefaultRunLoopMode, NSRunLoop

try:
    from LocalAuthentication import LAContext, LAPolicyDeviceOwnerAuthentication
except Exception:  # framework not installed
    LAContext = None

_asking = False  # one dialog at a time


def require(reason="confirm it's you") -> bool:
    """Wait for the owner to authenticate. True = proceed, False = denied.
    `reason` ends macOS's own sentence: "unplugg is trying to <reason>."

    Spins the run loop while the dialog is up instead of blocking the main
    thread, so the siren still starts if the alarm fires meanwhile."""
    global _asking
    if LAContext is None:
        return True
    ctx = LAContext.alloc().init()
    can, _ = ctx.canEvaluatePolicy_error_(LAPolicyDeviceOwnerAuthentication, None)
    if not can:
        return True  # no auth configured on this Mac
    if _asking:
        return False

    _asking = True
    result = {}
    ctx.evaluatePolicy_localizedReason_reply_(
        LAPolicyDeviceOwnerAuthentication, reason, lambda ok, err: result.update(ok=bool(ok)))
    try:
        deadline = time.monotonic() + 120
        while "ok" not in result and time.monotonic() < deadline:
            NSRunLoop.currentRunLoop().runMode_beforeDate_(
                NSDefaultRunLoopMode, NSDate.dateWithTimeIntervalSinceNow_(0.1))
        if "ok" not in result:
            ctx.invalidate()  # close the dialog nobody answered
    finally:
        _asking = False
    return result.get("ok", False)
