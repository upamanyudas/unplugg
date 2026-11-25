mod app;
#[allow(dead_code)]
mod auth;
#[allow(dead_code)]
mod capture;
#[allow(dead_code)]
mod devices;
mod dialogs;
mod guide;
mod lidsleep;
#[allow(dead_code)]
mod location;
#[allow(dead_code)]
mod loginitem;
mod overlay;
#[allow(dead_code)]
mod permissions;
mod prefs;
#[allow(dead_code)]
mod sensors;
#[allow(dead_code)]
mod siren;
mod updater;
#[allow(dead_code)]
mod utils;

pub fn run() {
    app::run();
}

