mod app;
#[allow(dead_code)]
mod auth;
#[allow(dead_code)]
mod capture;
#[allow(dead_code)]
mod devices;
#[allow(dead_code)]
mod location;
#[allow(dead_code)]
mod loginitem;
mod overlay;
#[allow(dead_code)]
mod permissions;
#[allow(dead_code)]
mod sensors;
#[allow(dead_code)]
mod siren;
#[allow(dead_code)]
mod utils;

pub fn run() {
    app::run();
}
