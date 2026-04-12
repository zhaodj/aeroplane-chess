use bevy::prelude::*;

use crate::gameplay::match_flow::{
    build_match_resources, MatchConfig, MatchResult, MatchSetup,
};
use crate::gameplay::turn_flow::TurnState;
use crate::states::AppState;
use crate::states::GamePhase;

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::LoadingGame), prepare_match)
            .add_systems(
                Update,
                transition_to_result.run_if(in_state(AppState::InGame)),
            );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GameSet {
    Flow,
}

fn prepare_match(
    mut commands: Commands,
    match_setup: Res<MatchSetup>,
    mut next_app_state: ResMut<NextState<AppState>>,
    mut next_game_phase: ResMut<NextState<GamePhase>>,
) {
    let (board_layout, player_roster, team_roster) = build_match_resources(&match_setup);

    commands.insert_resource(MatchConfig {
        mode: match_setup.mode,
        ai_difficulty: match_setup.ai_difficulty,
        fast_mode: match_setup.fast_mode,
        human_color: match_setup.human_color,
        pieces_per_player: match_setup.pieces_per_player,
    });
    commands.insert_resource(board_layout);
    commands.insert_resource(player_roster);
    commands.insert_resource(team_roster);
    commands.insert_resource(MatchResult::default());
    commands.insert_resource(TurnState::opening_turn());

    next_game_phase.set(GamePhase::AwaitDice);
    next_app_state.set(AppState::InGame);
}

fn transition_to_result(
    game_phase: Res<State<GamePhase>>,
    match_result: Res<MatchResult>,
    mut next_app_state: ResMut<NextState<AppState>>,
) {
    if match_result.finished && matches!(game_phase.get(), GamePhase::CheckVictory) {
        next_app_state.set(AppState::Result);
    }
}
