use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;

const LABEL: &str = "com.unplugg.launch";

fn plist() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
        .join("Library/LaunchAgents")
        .join(format!("{LABEL}.plist"))
}

pub fn enable() -> io::Result<()> {
    let executable = std::env::current_exe()?;
    let bundle = executable
        .ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app"));
    let args = if let Some(bundle) = bundle {
        vec![
            "/usr/bin/open".into(),
            bundle.to_string_lossy().into_owned(),
        ]
    } else {
        vec![executable.to_string_lossy().into_owned()]
    };
    let items: String = args
        .iter()
        .map(|arg| format!("<string>{arg}</string>"))
        .collect();
    let path = plist();
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(
        &path,
        format!(
            concat!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
                "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" ",
                "\"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">",
                "<plist version=\"1.0\"><dict>",
                "<key>Label</key><string>{LABEL}</string>",
                "<key>ProgramArguments</key><array>{items}</array>",
                "<key>RunAtLoad</key><true/></dict></plist>"
            ),
            LABEL = LABEL,
            items = items
        ),
    )?;
    let _ = Command::new("launchctl").arg("load").arg(path).output();
    Ok(())
}

pub fn disable() -> io::Result<()> {
    let path = plist();
    let _ = Command::new("launchctl").arg("unload").arg(&path).output();
    match fs::remove_file(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}
