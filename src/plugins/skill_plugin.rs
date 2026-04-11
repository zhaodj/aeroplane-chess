use bevy::prelude::*;

use crate::domain::piece::PieceState;
use crate::gameplay::match_flow::{MatchResult, PlayerRoster};
use crate::gameplay::skill_flow::{
    apply_shield_to_piece, arm_dash, arm_double_dice, build_skill_roster, can_use_skill_this_turn,
    current_player_type, dash_bonus, mark_skill_used, player_skill_state, preferred_shield_target,
    spend_shield_charge, sync_turn_skill_usage, SkillRoster,
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
                (sync_skill_turn_state, handle_human_skill_input).run_if(in_state(AppState::InGame)),
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

fn sync_skill_turn_state(turn_state: Res<TurnState>, mut skill_roster: ResMut<SkillRoster>) {
    sync_turn_skill_usage(&mut skill_roster, turn_state.current_player);
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
    if match_result.finished
        || !matches!(game_phase.get(), GamePhase::AwaitDice | GamePhase::AwaitPieceSelect)
    {
        return;
    }

    if current_player_type(&player_roster, turn_state.current_player)
        != Some(crate::domain::player::PlayerControl::Human)
    {
        return;
    }

    if !can_use_skill_this_turn(&skill_roster, turn_state.current_player) {
        return;
    }

    if keyboard.just_pressed(KeyCode::KeyQ) && matches!(game_phase.get(), GamePhase::AwaitDice) {
        let Some(target_piece_id) = preferred_shield_target(turn_state.current_player, &piece_query)
        else {
            skill_roster.last_skill_action = Some(format!(
                "P{} could not find a piece for Shield",
                turn_state.current_player
            ));
            return;
        };

        if !spend_shield_charge(&mut skill_roster, turn_state.current_player) {
            skill_roster.last_skill_action = Some(format!(
                "P{} has no Shield charges left",
                turn_state.current_player
            ));
            return;
        }

        if let Some(shield_value) = apply_shield_to_piece(target_piece_id, &mut piece_query) {
            mark_skill_used(&mut skill_roster, turn_state.current_player);
            skill_roster.last_skill_action = Some(format!(
                "P{} used Shield on piece #{} ({})",
                turn_state.current_player,
                target_piece_id,
                shield_value
            ));
        }
    } else if keyboard.just_pressed(KeyCode::KeyW) && matches!(game_phase.get(), GamePhase::AwaitDice) {
        if arm_double_dice(&mut skill_roster, turn_state.current_player) {
            mark_skill_used(&mut skill_roster, turn_state.current_player);
            skill_roster.last_skill_action = Some(format!(
                "P{} armed DoubleDice for the next roll",
                turn_state.current_player
            ));
        } else {
            let armed = player_skill_state(&skill_roster, turn_state.current_player)
                .map(|state| state.double_dice_armed)
                .unwrap_or(false);
            let message = if armed {
                format!(
                    "P{} already has DoubleDice armed",
                    turn_state.current_player
                )
            } else {
                format!(
                    "P{} has no DoubleDice charges left",
                    turn_state.current_player
                )
            };
            skill_roster.last_skill_action = Some(message);
        }
    } else if keyboard.just_pressed(KeyCode::KeyE)
        && matches!(game_phase.get(), GamePhase::AwaitPieceSelect)
        && dash_bonus(&skill_roster, turn_state.current_player) == 0
    {
        if arm_dash(&mut skill_roster, turn_state.current_player) {
            mark_skill_used(&mut skill_roster, turn_state.current_player);
            skill_roster.last_skill_action = Some(format!(
                "P{} armed Dash for +3 movement",
                turn_state.current_player
            ));
        } else {
            skill_roster.last_skill_action = Some(format!(
                "P{} has no Dash charges left",
                turn_state.current_player
            ));
        }
    }
}
