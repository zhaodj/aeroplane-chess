use bevy::prelude::*;

use crate::data::game_mode::GameMode;
use crate::domain::piece::PieceState;
use crate::gameplay::match_flow::{MatchConfig, MatchResult, PlayerRoster};
use crate::gameplay::skill_flow::{
    arm_dash, arm_double_dice, build_skill_roster, can_use_skill_this_turn, current_player_type,
    dash_bonus, mark_skill_used, player_skill_state, spend_shield_charge, spend_snipe_charge,
    spend_swap_charge, sync_turn_skill_usage, SkillRoster,
};
use crate::gameplay::turn_flow::TurnState;
use crate::plugins::piece_plugin::{HangarSlot, PieceId};
use crate::states::{AppState, GamePhase};

pub struct SkillPlugin;

#[derive(Resource, Default)]
pub struct SkillTargetState {
    candidate_piece_ids: Vec<u8>,
    pub prompt: Option<String>,
    active: bool,
}

impl SkillTargetState {
    pub fn candidate_piece_ids(&self) -> &[u8] {
        &self.candidate_piece_ids
    }

    pub fn is_active(&self) -> bool {
        self.active
    }
}

impl Plugin for SkillPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), setup_skill_roster)
            .add_systems(
                Update,
                (
                    sync_skill_turn_state,
                    handle_human_skill_input,
                    handle_human_snipe_key_select,
                    handle_human_snipe_click,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(OnExit(AppState::InGame), cleanup_skill_roster);
    }
}

fn setup_skill_roster(mut commands: Commands, player_roster: Res<PlayerRoster>) {
    commands.insert_resource(build_skill_roster(&player_roster));
    commands.insert_resource(SkillTargetState::default());
}

fn cleanup_skill_roster(mut commands: Commands) {
    commands.remove_resource::<SkillRoster>();
    commands.remove_resource::<SkillTargetState>();
}

fn sync_skill_turn_state(
    turn_state: Res<TurnState>,
    mut skill_roster: ResMut<SkillRoster>,
    mut target_state: ResMut<SkillTargetState>,
) {
    sync_turn_skill_usage(&mut skill_roster, turn_state.current_player);
    if target_state.is_active() && skill_roster.active_turn_player != Some(turn_state.current_player) {
        clear_target_state(&mut target_state);
    }
}

fn handle_human_skill_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    match_config: Res<MatchConfig>,
    player_roster: Res<PlayerRoster>,
    match_result: Res<MatchResult>,
    game_phase: Res<State<GamePhase>>,
    turn_state: Res<TurnState>,
    mut skill_roster: ResMut<SkillRoster>,
    mut target_state: ResMut<SkillTargetState>,
    mut next_phase: ResMut<NextState<GamePhase>>,
    mut piece_query: Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
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
        let Some(target_piece_id) =
            preferred_shield_target_for_full_query(turn_state.current_player, &piece_query)
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

        if let Some(shield_value) = apply_shield_to_piece_for_full_query(target_piece_id, &mut piece_query) {
            mark_skill_used(&mut skill_roster, turn_state.current_player);
            skill_roster.last_skill_action = Some(format!(
                "P{} used Shield on piece #{} ({})",
                turn_state.current_player,
                target_piece_id,
                shield_value
            ));
        }
    } else if keyboard.just_pressed(KeyCode::KeyS) && matches!(game_phase.get(), GamePhase::AwaitDice) {
        let Some(current_player_profile) = player_roster
            .players
            .iter()
            .find(|player| player.state.player_id == turn_state.current_player)
        else {
            return;
        };
        let targets = collect_snipe_targets_for_full_query(
            turn_state.current_player,
            current_player_profile.state.team_id,
            &piece_query,
        );

        if targets.is_empty() {
            skill_roster.last_skill_action = Some(format!(
                "P{} found no Snipe target",
                turn_state.current_player
            ));
            return;
        }
        if !spend_snipe_charge(&mut skill_roster, turn_state.current_player) {
            skill_roster.last_skill_action = Some(format!(
                "P{} has no Snipe charges left",
                turn_state.current_player
            ));
            return;
        }

        if targets.len() == 1 {
            mark_skill_used(&mut skill_roster, turn_state.current_player);
            skill_roster.last_skill_action =
                Some(execute_snipe(targets[0], &mut piece_query));
            return;
        }

        mark_skill_used(&mut skill_roster, turn_state.current_player);
        target_state.candidate_piece_ids = targets;
        target_state.prompt = Some(format!(
            "Select a Snipe target with click or {}",
            (1..=target_state.candidate_piece_ids.len())
                .map(|index| index.to_string())
                .collect::<Vec<_>>()
                .join("/")
        ));
        target_state.active = true;
        next_phase.set(GamePhase::ResolveSkillEffect);
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
    } else if keyboard.just_pressed(KeyCode::KeyA) && matches!(game_phase.get(), GamePhase::AwaitDice) {
        if match_config.mode != GameMode::TwoVsTwo {
            skill_roster.last_skill_action = Some("Swap is only available in 2v2".to_string());
            return;
        }

        let Some(current_player_profile) = player_roster
            .players
            .iter()
            .find(|player| player.state.player_id == turn_state.current_player)
        else {
            return;
        };

        let Some(teammate_piece_id) = find_active_teammate_piece_for_swap(
            turn_state.current_player,
            current_player_profile.state.team_id,
            &piece_query,
        ) else {
            skill_roster.last_skill_action = Some(format!(
                "P{} found no teammate piece to Swap with",
                turn_state.current_player
            ));
            return;
        };
        if !current_player_has_active_piece(turn_state.current_player, &piece_query) {
            skill_roster.last_skill_action = Some(format!(
                "P{} needs an active piece to use Swap",
                turn_state.current_player
            ));
            return;
        }
        if !spend_swap_charge(&mut skill_roster, turn_state.current_player) {
            skill_roster.last_skill_action = Some(format!(
                "P{} has no Swap charges left",
                turn_state.current_player
            ));
            return;
        }

        mark_skill_used(&mut skill_roster, turn_state.current_player);
        skill_roster.last_skill_action = Some(execute_swap(
            turn_state.current_player,
            teammate_piece_id,
            &mut piece_query,
        ));
    }
}

fn handle_human_snipe_key_select(
    keyboard: Res<ButtonInput<KeyCode>>,
    game_phase: Res<State<GamePhase>>,
    mut target_state: ResMut<SkillTargetState>,
    mut skill_roster: ResMut<SkillRoster>,
    mut next_phase: ResMut<NextState<GamePhase>>,
    mut piece_query: Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) {
    if !matches!(game_phase.get(), GamePhase::ResolveSkillEffect) || !target_state.is_active() {
        return;
    }

    if keyboard.just_pressed(KeyCode::Escape) {
        skill_roster.last_skill_action = Some("Snipe selection cancelled".to_string());
        clear_target_state(&mut target_state);
        next_phase.set(GamePhase::AwaitDice);
        return;
    }

    let keys = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4];
    let Some(selection) = keys
        .iter()
        .enumerate()
        .find_map(|(index, key)| {
            (index < target_state.candidate_piece_ids.len() && keyboard.just_pressed(*key))
                .then_some(index)
        })
    else {
        return;
    };

    let target_piece_id = target_state.candidate_piece_ids[selection];
    skill_roster.last_skill_action = Some(execute_snipe(target_piece_id, &mut piece_query));
    clear_target_state(&mut target_state);
    next_phase.set(GamePhase::AwaitDice);
}

fn handle_human_snipe_click(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform)>,
    game_phase: Res<State<GamePhase>>,
    mut target_state: ResMut<SkillTargetState>,
    mut skill_roster: ResMut<SkillRoster>,
    mut next_phase: ResMut<NextState<GamePhase>>,
    mut piece_query: Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) {
    if !matches!(game_phase.get(), GamePhase::ResolveSkillEffect) || !target_state.is_active() {
        return;
    }
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor_position) = window.cursor_position() else {
        return;
    };
    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };
    let Ok(cursor_world) = camera.viewport_to_world_2d(camera_transform, cursor_position) else {
        return;
    };

    let mut selected_piece_id = None;
    let mut best_distance_sq = f32::MAX;
    for (piece_id, _, _, transform) in &mut piece_query {
        if !target_state.candidate_piece_ids.contains(&piece_id.0) {
            continue;
        }

        let distance_sq = transform.translation.truncate().distance_squared(cursor_world);
        if distance_sq <= 28.0 * 28.0 && distance_sq < best_distance_sq {
            best_distance_sq = distance_sq;
            selected_piece_id = Some(piece_id.0);
        }
    }

    let Some(target_piece_id) = selected_piece_id else {
        return;
    };
    skill_roster.last_skill_action = Some(execute_snipe(target_piece_id, &mut piece_query));
    clear_target_state(&mut target_state);
    next_phase.set(GamePhase::AwaitDice);
}

fn clear_target_state(target_state: &mut SkillTargetState) {
    target_state.candidate_piece_ids.clear();
    target_state.prompt = None;
    target_state.active = false;
}

fn preferred_shield_target_for_full_query(
    player_id: u8,
    piece_query: &Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> Option<u8> {
    let mut unshielded_active = Vec::new();
    let mut active = Vec::new();
    let mut any_owned = Vec::new();

    for (piece_id, _, piece_state, _) in piece_query.iter() {
        if piece_state.owner_player_id != player_id {
            continue;
        }

        any_owned.push(piece_id.0);
        if piece_state.status == crate::domain::piece::PieceStatus::Active {
            active.push(piece_id.0);
            if piece_state.shield == 0 {
                unshielded_active.push(piece_id.0);
            }
        }
    }

    unshielded_active.sort_unstable();
    active.sort_unstable();
    any_owned.sort_unstable();
    unshielded_active
        .into_iter()
        .next()
        .or_else(|| active.into_iter().next())
        .or_else(|| any_owned.into_iter().next())
}

fn apply_shield_to_piece_for_full_query(
    piece_id: u8,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> Option<u8> {
    for (query_piece_id, _, mut piece_state, _) in piece_query.iter_mut() {
        if query_piece_id.0 != piece_id {
            continue;
        }
        piece_state.shield = piece_state.shield.saturating_add(1);
        return Some(piece_state.shield);
    }
    None
}

fn collect_snipe_targets_for_full_query(
    current_player: u8,
    current_team: u8,
    piece_query: &Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> Vec<u8> {
    let mut unshielded = Vec::new();
    let mut shielded = Vec::new();

    for (piece_id, _, piece_state, _) in piece_query.iter() {
        if piece_state.owner_player_id == current_player
            || piece_state.team_id == current_team
            || piece_state.status != crate::domain::piece::PieceStatus::Active
        {
            continue;
        }

        if piece_state.shield == 0 && piece_state.stack_shield == 0 {
            unshielded.push(piece_id.0);
        } else {
            shielded.push(piece_id.0);
        }
    }

    unshielded.sort_unstable();
    shielded.sort_unstable();
    unshielded.extend(shielded);
    unshielded
}

fn execute_snipe(
    piece_id: u8,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> String {
    for (query_piece_id, hangar_slot, mut piece_state, mut transform) in piece_query.iter_mut() {
        if query_piece_id.0 != piece_id {
            continue;
        }

        if piece_state.shield > 0 {
            piece_state.shield -= 1;
            return format!("Snipe hit piece #{} and removed a shield", piece_id);
        }
        if piece_state.stack_shield > 0 {
            piece_state.stack_shield = 0;
            return format!("Snipe hit piece #{} and broke the shared shield", piece_id);
        }

        piece_state.status = crate::domain::piece::PieceStatus::InHangar;
        piece_state.progress = 0;
        piece_state.shield = 0;
        piece_state.stack_shield = 0;
        transform.translation.x = hangar_slot.0.x;
        transform.translation.y = hangar_slot.0.y;
        return format!("Snipe sent piece #{} back to hangar", piece_id);
    }

    "Snipe failed to resolve".to_string()
}

fn current_player_has_active_piece(
    current_player: u8,
    piece_query: &Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> bool {
    piece_query.iter().any(|(_, _, piece_state, _)| {
        piece_state.owner_player_id == current_player
            && piece_state.status == crate::domain::piece::PieceStatus::Active
    })
}

fn find_active_teammate_piece_for_swap(
    current_player: u8,
    current_team: u8,
    piece_query: &Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> Option<u8> {
    let mut candidates = piece_query
        .iter()
        .filter(|(_, _, piece_state, _)| {
            piece_state.owner_player_id != current_player
                && piece_state.team_id == current_team
                && piece_state.status == crate::domain::piece::PieceStatus::Active
        })
        .map(|(piece_id, _, _, _)| piece_id.0)
        .collect::<Vec<_>>();
    candidates.sort_unstable();
    candidates.into_iter().next()
}

fn execute_swap(
    current_player: u8,
    teammate_piece_id: u8,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> String {
    let Some((current_piece_id, current_state, current_translation)) = piece_query
        .iter()
        .find_map(|(piece_id, _, piece_state, transform)| {
            (piece_state.owner_player_id == current_player
                && piece_state.status == crate::domain::piece::PieceStatus::Active)
                .then_some((piece_id.0, *piece_state, transform.translation))
        })
    else {
        return "Swap failed: current player's active piece not found".to_string();
    };

    let Some((teammate_state, teammate_translation)) = piece_query
        .iter()
        .find_map(|(piece_id, _, piece_state, transform)| {
            (piece_id.0 == teammate_piece_id).then_some((*piece_state, transform.translation))
        })
    else {
        return "Swap failed: teammate piece not found".to_string();
    };

    for (piece_id, _, mut piece_state, mut transform) in piece_query.iter_mut() {
        if piece_id.0 == current_piece_id {
            piece_state.status = teammate_state.status;
            piece_state.progress = teammate_state.progress;
            piece_state.shield = teammate_state.shield;
            piece_state.stack_shield = teammate_state.stack_shield;
            transform.translation = teammate_translation;
        } else if piece_id.0 == teammate_piece_id {
            piece_state.status = current_state.status;
            piece_state.progress = current_state.progress;
            piece_state.shield = current_state.shield;
            piece_state.stack_shield = current_state.stack_shield;
            transform.translation = current_translation;
        }
    }

    format!(
        "Swap exchanged piece #{} with teammate piece #{}",
        current_piece_id, teammate_piece_id
    )
}
