use bevy::prelude::*;

use crate::domain::piece::{PieceState, PieceStatus};
use crate::domain::player::PlayerControl;
use crate::gameplay::match_flow::{MatchResult, PlayerRoster};
use crate::gameplay::skill_flow::{
    arm_double_dice, build_skill_roster, player_skill_state, spend_shield_charge, SkillRoster,
};
use crate::gameplay::turn_flow::TurnState;
use crate::plugins::piece_plugin::PieceId;
use crate::states::{AppState, GamePhase};

pub struct SkillPlugin;

impl Plugin for SkillPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), setup_skill_roster)
            .add_systems(
                Update,
                handle_human_skill_input.run_if(in_state(AppState::InGame)),
            )
            .add_systems(OnExit(AppState::InGame), cleanup_skill_roster);
    }
}

fn setup_skill_roster(mut commands: Commands, player_roster: Res<PlayerRoster>) {
    commands.insert_resource(build_skill_roster(&player_roster));
}

fn cleanup_skill_roster(mut commands: Commands) {
    commands.remove_resource::<SkillRoster>();
}

fn handle_human_skill_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    player_roster: Res<PlayerRoster>,
    match_result: Res<MatchResult>,
    game_phase: Res<State<GamePhase>>,
    turn_state: Res<TurnState>,
    mut skill_roster: ResMut<SkillRoster>,
    mut piece_query: Query<(&PieceId, &mut PieceState)>,
) {
    if match_result.finished || !matches!(game_phase.get(), GamePhase::AwaitDice) {
        return;
    }

    let Some(current_player) = player_roster
        .players
        .iter()
        .find(|player| player.state.player_id == turn_state.current_player)
    else {
        return;
    };

    if current_player.state.control != PlayerControl::Human {
        return;
    }

    if keyboard.just_pressed(KeyCode::KeyQ) {
        let Some(target_piece_id) = preferred_shield_target(current_player.state.player_id, &piece_query)
        else {
            skill_roster.last_skill_action = Some(format!(
                "P{} could not find a piece for Shield",
                current_player.state.player_id
            ));
            return;
        };

        if !spend_shield_charge(&mut skill_roster, current_player.state.player_id) {
            skill_roster.last_skill_action = Some(format!(
                "P{} has no Shield charges left",
                current_player.state.player_id
            ));
            return;
        }

        for (piece_id, mut piece_state) in &mut piece_query {
            if piece_id.0 != target_piece_id {
                continue;
            }

            piece_state.shield = piece_state.shield.saturating_add(1);
            skill_roster.last_skill_action = Some(format!(
                "P{} used Shield on piece #{} ({})",
                current_player.state.player_id,
                target_piece_id,
                piece_state.shield
            ));
            break;
        }
    } else if keyboard.just_pressed(KeyCode::KeyW) {
        if arm_double_dice(&mut skill_roster, current_player.state.player_id) {
            skill_roster.last_skill_action = Some(format!(
                "P{} armed DoubleDice for the next roll",
                current_player.state.player_id
            ));
        } else {
            let armed = player_skill_state(&skill_roster, current_player.state.player_id)
                .map(|state| state.double_dice_armed)
                .unwrap_or(false);
            let message = if armed {
                format!(
                    "P{} already has DoubleDice armed",
                    current_player.state.player_id
                )
            } else {
                format!(
                    "P{} has no DoubleDice charges left",
                    current_player.state.player_id
                )
            };
            skill_roster.last_skill_action = Some(message);
        }
    }
}

fn preferred_shield_target(
    player_id: u8,
    piece_query: &Query<(&PieceId, &mut PieceState)>,
) -> Option<u8> {
    piece_query
        .iter()
        .filter(|(_, piece_state)| {
            piece_state.owner_player_id == player_id && piece_state.status == PieceStatus::Active
        })
        .map(|(piece_id, _)| piece_id.0)
        .min()
        .or_else(|| {
            piece_query
                .iter()
                .filter(|(_, piece_state)| piece_state.owner_player_id == player_id)
                .map(|(piece_id, _)| piece_id.0)
                .min()
        })
}
