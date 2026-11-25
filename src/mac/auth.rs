use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2::{msg_send, ClassType};
use objc2_foundation::{NSDate, NSError, NSRunLoop, NSString, NSDefaultRunLoopMode};
use objc2_local_authentication::{LAContext, LAPolicy};

static ASKING: AtomicBool = AtomicBool::new(false);

pub fn require(reason: &str) -> bool {
    let context: Retained<LAContext> = unsafe { msg_send![LAContext::class(), new] };
    let policy = LAPolicy::DeviceOwnerAuthentication;
    if unsafe { context.canEvaluatePolicy_error(policy) }.is_err() {
        return true;
    }
    if ASKING.swap(true, Ordering::SeqCst) {
        return false;
    }

    let result = Arc::new(AtomicU8::new(0));
    let reply_result = Arc::clone(&result);
    let reply = RcBlock::new(move |ok: Bool, _: *mut NSError| {
        reply_result.store(if ok.as_bool() { 1 } else { 2 }, Ordering::SeqCst);
    });
    unsafe {
        context.evaluatePolicy_localizedReason_reply(policy, &NSString::from_str(reason), &reply);
    }

    let deadline = Instant::now() + Duration::from_secs(120);
    while result.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
        unsafe {
            NSRunLoop::currentRunLoop().runMode_beforeDate(
                NSDefaultRunLoopMode,
                &NSDate::dateWithTimeIntervalSinceNow(0.1),
            );
        }
    }
    let outcome = result.load(Ordering::SeqCst);
    if outcome == 0 {
        unsafe { context.invalidate() };
    }
    ASKING.store(false, Ordering::SeqCst);
    outcome == 1
}
