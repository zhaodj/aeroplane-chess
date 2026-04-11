use bevy::prelude::*;

use crate::states::AppState;

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::MainMenu), enter_loading_game);
    }
}

fn enter_loading_game(mut next_state: ResMut<NextState<AppState>>) {
    next_state.set(AppState::LoadingGame);
}
