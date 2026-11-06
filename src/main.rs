#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod i18n;
mod model;
mod platform;
mod storage;
mod ui;
mod worker;

fn main() {
    if let Err(e) = i18n::init().and_then(|()| ui::run()) {
        ui::error_box(&e);
    }
}
