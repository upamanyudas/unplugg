use std::{env, fs, path::PathBuf};

fn main() {
    for path in [
        "packaging/windows/unplugg.rc",
        "packaging/windows/unplugg.manifest",
        "assets/logo.ico",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    if env::var("CARGO_CFG_TARGET_OS").unwrap() != "windows" {
        return;
    }
    if !cfg!(windows) {
        println!("cargo:warning=Windows resources require a Windows build host");
        return;
    }
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let numbers = format!(
        "{},{},{},0",
        env::var("CARGO_PKG_VERSION_MAJOR").unwrap(),
        env::var("CARGO_PKG_VERSION_MINOR").unwrap(),
        env::var("CARGO_PKG_VERSION_PATCH").unwrap()
    );
    let path = |name: &str| root.join(name).to_string_lossy().replace('\\', "/");
    let rc = fs::read_to_string(root.join("packaging/windows/unplugg.rc"))
        .unwrap()
        .replace("@ICON@", &path("assets/logo.ico"))
        .replace("@MANIFEST@", &path("packaging/windows/unplugg.manifest"))
        .replace("@VERSION_NUMBERS@", &numbers)
        .replace("@VERSION@", &version);
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("unplugg.rc");
    fs::write(&output, rc).unwrap();
    embed_resource::compile(output, embed_resource::NONE)
        .manifest_required()
        .unwrap();
}
