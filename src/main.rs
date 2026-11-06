#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod model;
mod platform;
mod storage;
mod ui;
mod worker;

fn main() {
    if let Err(e) = ui::run() {
        ui::error_box(&e);
    }
}
