use bevy::prelude::*;

use crate::states::AppState;

pub struct BootPlugin;

impl Plugin for BootPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Boot), setup_camera)
            .add_systems(OnEnter(AppState::Boot), advance_to_main_menu);
    }
}

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

fn advance_to_main_menu(mut next_state: ResMut<NextState<AppState>>) {
    next_state.set(AppState::MainMenu);
}
