use bevy::prelude::*;

use crate::constants::{WINDOW_HEIGHT, WINDOW_WIDTH};
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
                resolution: (WINDOW_WIDTH, WINDOW_HEIGHT).into(),
                resizable: true,
                canvas: Some("#bevy".into()),
                fit_canvas_to_parent: true,
                ..default()
            }),
            ..default()
        }))
        .add_plugins(AeroplaneChessPlugins)
        .run();
}
