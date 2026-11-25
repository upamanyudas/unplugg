use serde_json::{json, Map, Value};
use std::error::Error;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

type Config = Map<String, Value>;
type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn home() -> PathBuf {
    #[cfg(windows)]
    let key = "USERPROFILE";
    #[cfg(not(windows))]
    let key = "HOME";
    #[allow(deprecated)]
    std::env::var_os(key)
        .map(PathBuf::from)
        .or_else(std::env::home_dir)
        .unwrap_or_default()
}

pub fn config_file() -> PathBuf {
    #[cfg(target_os = "macos")]
    let dir = home().join("Library/Application Support");
    #[cfg(target_os = "windows")]
    let dir = PathBuf::from(std::env::var_os("APPDATA").expect("APPDATA is not set"));
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let dir = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config"));
    dir.join("unplugg/config.json")
}

pub fn defaults() -> Config {
    json!({
        "activated": false, "monitor_power": true, "monitor_lid": true,
        "monitor_motion": true, "angle_threshold": 5, "play_sound": true,
        "send_location": true, "capture_media": true, "flash_screen": true,
        "record_audio": true, "camera": "", "mic": "", "speaker_name": "",
        "media_dir": home().join("Pictures/unplugg").to_string_lossy(),
        "telegram_destinations": [], "show_overlay": true, "lock_device": false,
        "launch_at_login": false, "show_in_dock": false, "settings_pane": 0,
        "setup_done": false, "check_updates": true, "auto_update": false,
        "skipped_version": "", "power_restore": {}
    })
    .as_object()
    .unwrap()
    .clone()
}

pub fn load_config() -> Result<Config> {
    load_from(&config_file())
}

fn load_from(path: &Path) -> Result<Config> {
    let mut config = defaults();
    match File::open(path) {
        Ok(file) => config.extend(serde_json::from_reader::<_, Config>(file)?),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => (),
        Err(err) => return Err(err.into()),
    }
    Ok(config)
}

pub fn save_config(config: &Config) -> Result<()> {
    save_to(&config_file(), config)
}

fn save_to(path: &Path, config: &Config) -> Result<()> {
    fs::create_dir_all(path.parent().ok_or("missing config directory")?)?;
    let file = File::create(path)?;
    serde_json::to_writer_pretty(&file, config)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_and_invalid_files() {
        let dir =
            std::env::temp_dir().join(format!("unplugg-invalid-config-{}", std::process::id()));
        let path = dir.join("config.json");
        assert_eq!(load_from(&path).unwrap(), defaults());
        fs::create_dir_all(&dir).unwrap();
        fs::write(&path, "{").unwrap();
        assert!(load_from(&path).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn merge_and_save_extra_keys() {
        let dir = std::env::temp_dir().join(format!("unplugg-config-{}", std::process::id()));
        let path = dir.join("config.json");
        fs::create_dir_all(&dir).unwrap();
        fs::write(&path, r#"{"monitor_power":false,"extra":{"a":1}}"#).unwrap();
        let config = load_from(&path).unwrap();
        assert_eq!(config["monitor_power"], false);
        assert_eq!(config["monitor_lid"], true);
        save_to(&path, &config).unwrap();
        assert_eq!(load_from(&path).unwrap()["extra"], json!({"a": 1}));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        fs::remove_dir_all(dir).unwrap();
    }
}
