#![allow(dead_code)]

pub mod app;
mod constants;
mod data;
mod domain;
mod gameplay;
mod platform;
mod plugins;
mod states;
mod ui;

pub use app::run;

#[bevy::prelude::bevy_main]
pub fn main() {
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();

    run();
}
