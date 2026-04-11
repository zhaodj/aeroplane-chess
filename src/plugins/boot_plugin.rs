use bevy::prelude::*;

use crate::data::game_mode::GameMode;
use crate::gameplay::ai::AiDifficulty;
use crate::gameplay::match_flow::MatchSetup;
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
    commands.insert_resource(MatchSetup {
        mode: GameMode::TwoVsTwo,
        ai_difficulty: AiDifficulty::Normal,
        fast_mode: false,
    });
}

fn advance_to_main_menu(mut next_state: ResMut<NextState<AppState>>) {
    next_state.set(AppState::MainMenu);
}
