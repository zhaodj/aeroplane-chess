use bevy::prelude::*;

use crate::states::AppState;

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::LoadingGame), prepare_match)
            .add_systems(Update, keep_in_game_state.in_set(GameSet::Flow));
    }
}

#[derive(SystemSet, Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GameSet {
    Flow,
}

fn prepare_match() {}

fn keep_in_game_state(state: Res<State<AppState>>, mut next_state: ResMut<NextState<AppState>>) {
    if matches!(state.get(), AppState::LoadingGame) {
        next_state.set(AppState::InGame);
    }
}
