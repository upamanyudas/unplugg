mod app;
mod auth;
#[allow(dead_code)]
mod capture;
#[allow(dead_code)]
mod devices;
#[allow(dead_code)]
mod loginitem;
#[allow(dead_code)]
mod location;
#[allow(dead_code)]
mod media;
#[allow(dead_code)]
mod permissions;
mod overlay;
mod power;
#[allow(dead_code)]
mod sensors;
#[allow(dead_code)]
mod siren;
#[allow(dead_code)]
mod utils;

pub fn run() {
    app::run();
}
