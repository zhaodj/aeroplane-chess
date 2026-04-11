use bevy::prelude::*;

use crate::plugins::AeroplaneChessPlugins;
use crate::states::{AppState, GamePhase};

pub fn run() {
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.92, 0.95, 1.0)))
        .init_state::<AppState>()
        .init_state::<GamePhase>()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Aeroplane Chess".into(),
                resolution: (1280, 720).into(),
                resizable: true,
                ..default()
            }),
            ..default()
        }))
        .add_plugins(AeroplaneChessPlugins)
        .run();
}
