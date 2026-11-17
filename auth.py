"""Require the device owner (Touch ID or login password) before disarming or
quitting while armed — so a thief can't just click Deactivate to kill the alarm.

ponytail: LocalAuthentication is the native gate; we never see or handle the
password. If the Mac has no passcode/biometrics set up we allow the action
rather than lock the owner out of their own app.
"""
import threading

try:
    from LocalAuthentication import LAContext, LAPolicyDeviceOwnerAuthentication
except Exception:  # framework not installed
    LAContext = None


def require(reason="confirm it's you") -> bool:
    """Block until the owner authenticates. True = proceed, False = denied.
    `reason` ends macOS's own sentence: "unplugg is trying to <reason>." """
    if LAContext is None:
        return True
    ctx = LAContext.alloc().init()
    can, _ = ctx.canEvaluatePolicy_error_(LAPolicyDeviceOwnerAuthentication, None)
    if not can:
        return True  # no auth configured on this Mac

    result = {}
    done = threading.Event()

    def reply(success, error):
        result["ok"] = bool(success)
        done.set()

    ctx.evaluatePolicy_localizedReason_reply_(
        LAPolicyDeviceOwnerAuthentication, reason, reply)
    if not done.wait(120):
        ctx.invalidate()  # close the dialog nobody answered
    return result.get("ok", False)
