#![allow(dead_code)]

mod app;
mod constants;
mod data;
mod domain;
mod gameplay;
mod plugins;
mod states;
mod ui;

fn main() {
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();

    app::run();
}
