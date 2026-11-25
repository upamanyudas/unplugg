mod app;
mod guide;
mod auth;
#[allow(dead_code)]
mod capture;
#[allow(dead_code)]
mod devices;
#[allow(dead_code)]
mod location;
#[allow(dead_code)]
mod loginitem;
#[allow(dead_code)]
mod media;
mod overlay;
#[allow(dead_code)]
mod permissions;
mod power;
#[allow(dead_code)]
mod sensors;
#[allow(dead_code)]
mod siren;
mod ui;
mod updater;
#[allow(dead_code)]
mod utils;

pub fn run() {
    app::run();
}
