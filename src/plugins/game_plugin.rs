use bevy::prelude::*;

use crate::gameplay::match_flow::{MatchConfig, MatchResult, MatchSetup, build_match_resources};
use crate::gameplay::turn_flow::TurnState;
use crate::plugins::boot_plugin::AutoplayMatch;
use crate::states::AppState;
use crate::states::GamePhase;

/// 对局生命周期插件：负责装配对局资源与结果页跳转。
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
    autoplay: Option<Res<AutoplayMatch>>,
    mut next_app_state: ResMut<NextState<AppState>>,
    mut next_game_phase: ResMut<NextState<GamePhase>>,
) {
    let (board_layout, player_roster, team_roster) = build_match_resources(&match_setup);

    commands.insert_resource(MatchConfig {
        mode: match_setup.mode,
        ai_difficulty: match_setup.ai_difficulty,
        fast_mode: match_setup.fast_mode,
        launch_rule: match_setup.launch_rule,
        player_seats: match_setup.normalized_player_seats(),
        pieces_per_player: match_setup.pieces_per_player,
        player_controls: match_setup.normalized_player_controls(),
    });
    commands.insert_resource(board_layout);
    commands.insert_resource(player_roster);
    commands.insert_resource(team_roster);
    commands.insert_resource(MatchResult::default());
    commands.insert_resource(TurnState::opening_turn());

    if autoplay.is_some() {
        set_autoplay_smoke_state("ingame", "");
    }
    next_game_phase.set(GamePhase::AwaitDice);
    next_app_state.set(AppState::InGame);
}

fn transition_to_result(
    game_phase: Res<State<GamePhase>>,
    match_result: Res<MatchResult>,
    autoplay: Option<Res<AutoplayMatch>>,
    mut next_app_state: ResMut<NextState<AppState>>,
) {
    if match_result.finished && matches!(game_phase.get(), GamePhase::CheckVictory) {
        if autoplay.is_some() {
            set_autoplay_smoke_state("result", &format_winner_players(&match_result));
        }
        next_app_state.set(AppState::Result);
    }
}

fn format_winner_players(match_result: &MatchResult) -> String {
    match_result
        .winner_player_ids
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(target_arch = "wasm32")]
fn set_autoplay_smoke_state(state: &str, winner_players: &str) {
    let Some(body) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.body())
    else {
        return;
    };
    let _ = body.set_attribute("data-ac-smoke-state", state);
    let _ = body.set_attribute("data-ac-smoke-winners", winner_players);
}

#[cfg(not(target_arch = "wasm32"))]
fn set_autoplay_smoke_state(_state: &str, _winner_players: &str) {}
