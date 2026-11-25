//! Closing the lid sleeps a Mac even while armed, which silences the siren and
//! stops live tracking. `pmset disablesleep 1` prevents that but needs root, so a
//! sudoers rule lets this user run exactly those two pmset commands, nothing else.

use std::fs;
use std::process::{Command, Stdio};

use super::dialogs::alert;

const RULE: &str = "/etc/sudoers.d/unplugg";
const PMSET: &str = "/usr/bin/pmset";

/// Turns the setting on or off. Explains first and asks for the admin password
/// only after "Continue". False if the user said "Not now" or it failed.
pub fn set(on: bool, armed: bool) -> bool {
    let (title, text) = if on {
        (
            "Stay awake with the lid closed?",
            "Closing the lid puts your Mac to sleep, even while armed. That silences the siren and stops live tracking.\n\n\
             To keep it awake, unplugg needs your admin password once. It only lets unplugg turn sleep off while armed, and back on when you disarm.",
        )
    } else {
        (
            "Let the lid sleep your Mac again?",
            "unplugg needs your admin password once more, to remove its permission to keep your Mac awake.",
        )
    };
    if alert(title, text, &["Continue", "Not now"]) != 0 {
        return false;
    }
    if !on {
        pmset(false);
        return admin(&format!("/bin/rm -f {RULE}"), "unplugg wants to stop keeping your Mac awake with the lid closed.");
    }
    if install() {
        if armed {
            pmset(true);
        }
        return true;
    }
    false
}

fn install() -> bool {
    let user = Command::new("/usr/bin/id").arg("-un").output().ok().map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned());
    let Some(user) = user.filter(|user| !user.is_empty() && user.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))) else {
        alert("Couldn't set this up", "unplugg couldn't read your user name.", &[]);
        return false;
    };
    let staged = std::env::temp_dir().join(format!("unplugg-sudoers-{}", std::process::id()));
    let rule = format!("{user} ALL=(root) NOPASSWD: {PMSET} -a disablesleep 0, {PMSET} -a disablesleep 1\n");
    if fs::write(&staged, rule).is_err() {
        return false;
    }
    let staged_path = staged.to_string_lossy();
    // visudo checks the rule before it goes live: a broken sudoers file locks out sudo
    let installed = admin(
        &format!("/usr/sbin/visudo -cf '{staged_path}' && /bin/mkdir -p /etc/sudoers.d && /usr/bin/install -m 0440 -o root -g wheel '{staged_path}' {RULE}"),
        "unplugg wants to keep your Mac awake with the lid closed while armed.",
    );
    let _ = fs::remove_file(&staged);
    if installed && pmset(false) {
        return true;
    }
    if installed {
        alert("Couldn't set this up", "macOS didn't accept the permission, so the lid will still sleep your Mac.", &[]);
    }
    false
}

/// Runs a shell command as root after macOS's own password prompt.
fn admin(command: &str, prompt: &str) -> bool {
    let script = format!("do shell script \"{command}\" with prompt \"{prompt}\" with administrator privileges");
    Command::new("/usr/bin/osascript")
        .args(["-e", &script])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Blocks or restores lid sleep. Never asks for a password.
pub fn pmset(block: bool) -> bool {
    Command::new("/usr/bin/sudo")
        .args(["-n", PMSET, "-a", "disablesleep", if block { "1" } else { "0" }])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}
