use bevy::prelude::*;
#[cfg(any(target_os = "android", target_os = "ios"))]
use bevy::winit::WinitSettings;

use crate::platform::{self, PlatformPlugin};
use crate::plugins::AeroplaneChessPlugins;
use crate::states::{AppState, GamePhase};

pub fn run() {
    let default_plugins = DefaultPlugins.set(WindowPlugin {
        primary_window: Some(platform::primary_window()),
        ..default()
    });

    let mut app = App::new();
    app.insert_resource(ClearColor(Color::srgb(0.92, 0.95, 1.0)))
        .add_plugins(default_plugins)
        .add_plugins(PlatformPlugin)
        .init_state::<AppState>()
        .init_state::<GamePhase>()
        .add_plugins(AeroplaneChessPlugins);

    #[cfg(any(target_os = "android", target_os = "ios"))]
    app.insert_resource(WinitSettings::mobile());

    app.run();
}
