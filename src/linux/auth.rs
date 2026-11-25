//! Requires the owner before disarming or quitting while armed: polkit's dialog
//! for unplugg's action, which the .deb and .rpm install; without it (AppImage,
//! brew), a password box checked by PAM. The password is never kept.

use std::cell::{Cell, RefCell};
use std::ffi::{c_char, c_int, c_void, CString};
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::glib;
use gtk::prelude::*;

const ACTION: &str = "com.unplugg.app.disarm"; // packaging/linux/com.unplugg.app.policy

thread_local! {
    // one dialog at a time
    static ASKING: Cell<bool> = const { Cell::new(false) };
}

/// Waits for the owner: true = proceed. `parent` is a window the dialog should sit above.
pub fn require(reason: &str, parent: Option<&gtk::Window>) -> bool {
    if ASKING.with(|asking| asking.replace(true)) {
        return false;
    }
    let allowed = polkit(reason, parent);
    ASKING.with(|asking| asking.set(false));
    allowed
}

/// Runs the main loop until done() or the timeout, so the siren keeps going.
fn wait(done: impl FnMut() -> bool + 'static, timeout: Duration) -> bool {
    let main = glib::MainLoop::new(None, false);
    let done = Rc::new(RefCell::new(done));
    let finished = Rc::new(Cell::new(false));
    let deadline = Instant::now() + timeout;
    let (check, result, stop) = (done.clone(), finished.clone(), main.clone());
    glib::timeout_add_local(Duration::from_millis(100), move || {
        let over = (check.borrow_mut())();
        result.set(over);
        if over || Instant::now() > deadline {
            stop.quit();
            return glib::ControlFlow::Break;
        }
        glib::ControlFlow::Continue
    });
    main.run();
    finished.get()
}

/// polkit's own dialog: the desktop's agent shows it, fingerprint included.
fn polkit(reason: &str, parent: Option<&gtk::Window>) -> bool {
    // field 22, the start time: the pid alone is racy
    let start = std::fs::read_to_string("/proc/self/stat")
        .ok()
        .and_then(|stat| stat.rsplit_once(')').and_then(|(_, rest)| rest.split_whitespace().nth(19).map(str::to_owned)))
        .unwrap_or_default();
    let process = format!("{},{start},{}", std::process::id(), unsafe { getuid() });
    let check = Command::new("pkcheck")
        .args(["--action-id", ACTION, "--allow-user-interaction", "--process", &process])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    let Ok(check) = check else { return pam(reason, parent) };
    let check = Rc::new(RefCell::new(check));
    let status = Rc::new(Cell::new(None));
    let (child, code) = (check.clone(), status.clone());
    let done = wait(
        move || {
            if let Ok(Some(exit)) = child.borrow_mut().try_wait() {
                code.set(exit.code());
            }
            code.get().is_some()
        },
        Duration::from_secs(120),
    );
    if !done {
        let _ = check.borrow_mut().kill();
        return false;
    }
    match status.get() {
        Some(0) => true,
        // no agent, or no unplugg policy (AppImage, brew)
        Some(2) | Some(127) => pam(reason, parent),
        _ => false,
    }
}

/// Our own password box, checked by PAM's login stack.
fn pam(reason: &str, parent: Option<&gtk::Window>) -> bool {
    let mut prompt = format!("unplugg wants to {reason}. Enter your password.");
    for _ in 0..3 {
        let Some(password) = ask_password(&prompt, parent) else { return false };
        if authenticate(&user(), &password) {
            return true;
        }
        prompt = "That password didn't work. Try again.".into();
    }
    false
}

fn user() -> String {
    std::env::var("USER").or_else(|_| std::env::var("LOGNAME")).unwrap_or_default()
}

fn ask_password(prompt: &str, parent: Option<&gtk::Window>) -> Option<String> {
    let dialog = gtk::Dialog::with_buttons(Some("unplugg"), parent, gtk::DialogFlags::MODAL, &[("Cancel", gtk::ResponseType::Cancel), ("OK", gtk::ResponseType::Ok)]);
    dialog.set_keep_above(true);
    dialog.set_default_response(gtk::ResponseType::Ok);
    let column = gtk::Box::new(gtk::Orientation::Vertical, 8);
    column.set_border_width(12);
    let label = gtk::Label::new(Some(prompt));
    label.set_xalign(0.0);
    let entry = gtk::Entry::new();
    entry.set_visibility(false);
    entry.set_activates_default(true);
    column.pack_start(&label, false, false, 0);
    column.pack_start(&entry, false, false, 0);
    dialog.content_area().pack_start(&column, true, true, 0);
    dialog.show_all();
    let ok = dialog.run() == gtk::ResponseType::Ok;
    let password = entry.text().to_string();
    unsafe { dialog.destroy() };
    ok.then_some(password)
}

// ---- PAM, loaded at runtime: the app still starts where libpam is missing ----

const PAM_PROMPT_ECHO_OFF: c_int = 1;
const PAM_PROMPT_ECHO_ON: c_int = 2;

#[repr(C)]
struct Message {
    style: c_int,
    text: *const c_char,
}

#[repr(C)]
struct Response {
    text: *mut c_char,
    code: c_int,
}

#[repr(C)]
struct Conversation {
    function: unsafe extern "C" fn(c_int, *mut *const Message, *mut *mut Response, *mut c_void) -> c_int,
    data: *mut c_void,
}

unsafe extern "C" {
    fn getuid() -> u32;
    fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn calloc(count: usize, size: usize) -> *mut c_void;
    fn strdup(text: *const c_char) -> *mut c_char;
}

/// Answers PAM's password prompts with the password in `data`.
unsafe extern "C" fn converse(count: c_int, messages: *mut *const Message, responses: *mut *mut Response, data: *mut c_void) -> c_int {
    let answers = calloc(count as usize, size_of::<Response>()) as *mut Response;
    if answers.is_null() {
        return 5; // PAM_BUF_ERR
    }
    for i in 0..count as usize {
        let style = (**messages.add(i)).style;
        if style == PAM_PROMPT_ECHO_OFF || style == PAM_PROMPT_ECHO_ON {
            (*answers.add(i)).text = strdup(data as *const c_char);
        }
    }
    *responses = answers;
    0
}

fn authenticate(user: &str, password: &str) -> bool {
    type Start = unsafe extern "C" fn(*const c_char, *const c_char, *const Conversation, *mut *mut c_void) -> c_int;
    type Call = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;
    let (Ok(user), Ok(password)) = (CString::new(user), CString::new(password)) else { return false };
    unsafe {
        let library = dlopen(c"libpam.so.0".as_ptr(), 2);
        if library.is_null() {
            return false;
        }
        let symbol = |name: &std::ffi::CStr| dlsym(library, name.as_ptr());
        let (start, auth, end) = (symbol(c"pam_start"), symbol(c"pam_authenticate"), symbol(c"pam_end"));
        if start.is_null() || auth.is_null() || end.is_null() {
            return false;
        }
        let (start, auth, end): (Start, Call, Call) = (std::mem::transmute(start), std::mem::transmute(auth), std::mem::transmute(end));
        let conversation = Conversation { function: converse, data: password.as_ptr() as *mut c_void };
        let mut handle = std::ptr::null_mut();
        if start(c"login".as_ptr(), user.as_ptr(), &conversation, &mut handle) != 0 {
            return false;
        }
        let status = auth(handle, 0);
        end(handle, status);
        status == 0
    }
}
