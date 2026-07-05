#![allow(dead_code)]

pub mod app;
mod constants;
mod data;
mod domain;
mod gameplay;
mod i18n;
mod platform;
mod plugins;
mod states;
mod ui;

pub use app::run;

#[cfg_attr(not(target_arch = "wasm32"), bevy::prelude::bevy_main)]
pub fn main() {
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();

    run();
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn wasm_start() {
    main();
}
