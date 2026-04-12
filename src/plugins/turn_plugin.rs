use bevy::prelude::*;

use crate::data::game_mode::GameMode;
use crate::domain::dice::DiceRoll;
use crate::domain::piece::PieceState;
use crate::domain::player::PlayerControl;
use crate::gameplay::match_flow::{
    BoardLayout, MatchConfig, MatchResult, PlayerRoster, TeamRoster,
};
use crate::gameplay::skill_flow::{
    arm_dash, arm_double_dice, can_use_skill_this_turn, clear_dash_arm, dash_bonus,
    mark_skill_used, player_skill_state, resolve_roll_value, spend_shield_charge,
    spend_snipe_charge, spend_swap_charge, SkillRoster,
};
use crate::gameplay::turn_flow::{
    choose_action, collect_actions, current_player_control, execute_action,
    find_pending_action_by_piece_id, finish_turn_without_action, get_pending_action,
    pressed_selection_key, set_pending_actions, set_roll, PlannedAction, TurnInputState,
    TurnState,
};
use crate::plugins::piece_plugin::{HangarSlot, PieceId};
use crate::states::{AppState, GamePhase};

pub struct TurnPlugin;

impl Plugin for TurnPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), setup_turn_automation)
            .add_systems(
                Update,
                (
                    drive_ai_turn_loop,
                    handle_human_roll_input,
                    handle_human_action_input,
                    handle_human_action_click,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(OnExit(AppState::InGame), cleanup_turn_automation);
    }
}

#[derive(Resource)]
struct TurnAutomation {
    timer: Timer,
}

fn setup_turn_automation(mut commands: Commands) {
    commands.insert_resource(TurnAutomation {
        timer: Timer::from_seconds(0.9, TimerMode::Repeating),
    });
    commands.insert_resource(TurnInputState::default());
}

fn cleanup_turn_automation(mut commands: Commands) {
    commands.remove_resource::<TurnAutomation>();
    commands.remove_resource::<TurnInputState>();
}

fn drive_ai_turn_loop(
    time: Res<Time>,
    mut automation: ResMut<TurnAutomation>,
    mut input_state: ResMut<TurnInputState>,
    mut turn_state: ResMut<TurnState>,
    mut next_phase: ResMut<NextState<GamePhase>>,
    game_phase: Res<State<GamePhase>>,
    board_layout: Res<BoardLayout>,
    match_config: Res<MatchConfig>,
    player_roster: Res<PlayerRoster>,
    team_roster: Res<TeamRoster>,
    mut skill_roster: ResMut<SkillRoster>,
    mut match_result: ResMut<MatchResult>,
    mut piece_query: Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) {
    if !matches!(game_phase.get(), GamePhase::AwaitDice) || match_result.finished {
        return;
    }

    if current_player_control(turn_state.current_player, &player_roster) != Some(PlayerControl::Ai)
    {
        return;
    }

    if !automation.timer.tick(time.delta()).just_finished() {
        return;
    }

    maybe_use_ai_skills(
        turn_state.current_player,
        &match_config,
        &mut skill_roster,
        &mut piece_query,
    );

    let roll_resolution = resolve_roll_value(&mut skill_roster, turn_state.current_player);
    let roll_value = roll_resolution.value;
    let roll = DiceRoll(roll_value);
    set_roll(&mut turn_state, roll_value);
    if roll_resolution.used_double_dice {
        skill_roster.last_skill_action = Some(format!(
            "P{} resolved DoubleDice into {}",
            turn_state.current_player, roll_value
        ));
    }

    let current_player = turn_state.current_player;
    let Some(action) =
        choose_action(
            current_player,
            roll,
            dash_bonus(&skill_roster, current_player),
            &board_layout,
            &player_roster,
            &piece_query,
        )
    else {
        turn_state.last_action =
            Some(format!("P{current_player} rolled {roll_value} but had no legal action"));
        finish_turn_without_action(
            &mut turn_state,
            &mut input_state,
            &player_roster,
            &mut next_phase,
        );
        return;
    };

    execute_action(
        action,
        roll_value,
        &player_roster,
        &team_roster,
        &match_config,
        &board_layout,
        &mut piece_query,
        &mut skill_roster,
        &mut match_result,
        &mut turn_state,
        &mut input_state,
        &mut next_phase,
    );
    clear_dash_arm(&mut skill_roster, current_player);
}

fn maybe_use_ai_skills(
    current_player: u8,
    match_config: &MatchConfig,
    skill_roster: &mut SkillRoster,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) {
    if can_use_skill_this_turn(skill_roster, current_player)
        && let Some(target_piece_id) = preferred_ai_snipe_target(current_player, piece_query)
    {
        let can_use_snipe = player_skill_state(skill_roster, current_player)
            .map(|skills| skills.snipe_charges > 0)
            .unwrap_or(false);

        if can_use_snipe && spend_snipe_charge(skill_roster, current_player) {
            mark_skill_used(skill_roster, current_player);
            skill_roster.last_skill_action =
                Some(execute_snipe_on_turn_query(target_piece_id, piece_query, true));
            return;
        }
    }

    if let Some(target_piece_id) = preferred_ai_shield_target(current_player, piece_query) {
        let can_use_shield = player_skill_state(skill_roster, current_player)
            .map(|skills| skills.shield_charges > 0)
            .unwrap_or(false);

        if can_use_shield
            && spend_shield_charge(skill_roster, current_player)
            && let Some(shield_value) =
                apply_shield_to_piece_to_turn_query(target_piece_id, piece_query)
        {
            mark_skill_used(skill_roster, current_player);
            skill_roster.last_skill_action = Some(format!(
                "P{} (AI) used Shield on piece #{} ({})",
                current_player, target_piece_id, shield_value
            ));
            return;
        }
    }

    if match_config.mode == GameMode::TwoVsTwo
        && can_use_skill_this_turn(skill_roster, current_player)
        && let Some(teammate_piece_id) = preferred_ai_swap_target(current_player, piece_query)
    {
        let can_use_swap = player_skill_state(skill_roster, current_player)
            .map(|skills| skills.swap_charges > 0)
            .unwrap_or(false);
        if can_use_swap && spend_swap_charge(skill_roster, current_player) {
            mark_skill_used(skill_roster, current_player);
            skill_roster.last_skill_action =
                Some(execute_swap_on_turn_query(current_player, teammate_piece_id, piece_query));
            return;
        }
    }

    if arm_dash_for_ai(current_player, skill_roster, piece_query) {
        return;
    }

    if should_ai_arm_double_dice(current_player, skill_roster, piece_query)
        && arm_double_dice(skill_roster, current_player)
    {
        mark_skill_used(skill_roster, current_player);
        skill_roster.last_skill_action = Some(format!(
            "P{} (AI) armed DoubleDice for launch pressure",
            current_player
        ));
    }
}

fn preferred_ai_snipe_target(
    current_player: u8,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> Option<u8> {
    let mut attacker_team = None;
    for (_, _, piece_state, _) in piece_query.iter_mut() {
        if piece_state.owner_player_id == current_player {
            attacker_team = Some(piece_state.team_id);
            break;
        }
    }
    let Some(attacker_team) = attacker_team else {
        return None;
    };

    let mut unshielded = Vec::new();
    let mut shielded = Vec::new();
    for (piece_id, _, piece_state, _) in piece_query.iter_mut() {
        if piece_state.owner_player_id == current_player
            || piece_state.team_id == attacker_team
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
    unshielded.into_iter().next().or_else(|| shielded.into_iter().next())
}

fn preferred_ai_shield_target(
    current_player: u8,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> Option<u8> {
    piece_query
        .iter_mut()
        .filter(|(_, _, piece_state, _)| {
            piece_state.owner_player_id == current_player
                && piece_state.status == crate::domain::piece::PieceStatus::Active
                && piece_state.shield == 0
        })
        .map(|(piece_id, _, _, _)| piece_id.0)
        .min()
}

fn preferred_ai_swap_target(
    current_player: u8,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> Option<u8> {
    let mut own_progress = None;
    let mut own_team = None;

    for (_, _, piece_state, _) in piece_query.iter_mut() {
        if piece_state.owner_player_id == current_player
            && piece_state.status == crate::domain::piece::PieceStatus::Active
        {
            own_progress = Some(piece_state.progress);
            own_team = Some(piece_state.team_id);
            break;
        }
    }

    let (Some(own_progress), Some(own_team)) = (own_progress, own_team) else {
        return None;
    };

    let mut candidates = piece_query
        .iter_mut()
        .filter(|(_, _, piece_state, _)| {
            piece_state.owner_player_id != current_player
                && piece_state.team_id == own_team
                && piece_state.status == crate::domain::piece::PieceStatus::Active
                && piece_state.progress >= own_progress.saturating_add(6)
        })
        .map(|(piece_id, _, piece_state, _)| (piece_id.0, piece_state.progress))
        .collect::<Vec<_>>();
    candidates.sort_by_key(|(_, progress)| std::cmp::Reverse(*progress));
    candidates.into_iter().map(|(piece_id, _)| piece_id).next()
}

fn should_ai_arm_double_dice(
    current_player: u8,
    skill_roster: &SkillRoster,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> bool {
    let Some(skill_state) = player_skill_state(skill_roster, current_player) else {
        return false;
    };
    if skill_state.double_dice_charges == 0 || skill_state.double_dice_armed {
        return false;
    }

    let mut has_active_piece = false;
    let mut has_hangar_piece = false;
    for (_, _, piece_state, _) in piece_query.iter_mut() {
        if piece_state.owner_player_id != current_player {
            continue;
        }

        match piece_state.status {
            crate::domain::piece::PieceStatus::Active => has_active_piece = true,
            crate::domain::piece::PieceStatus::InHangar => has_hangar_piece = true,
            _ => {}
        }
    }

    !has_active_piece && has_hangar_piece
}

fn arm_dash_for_ai(
    current_player: u8,
    skill_roster: &mut SkillRoster,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> bool {
    if !can_use_skill_this_turn(skill_roster, current_player) {
        return false;
    }

    let has_movable_piece = piece_query.iter_mut().any(|(_, _, piece_state, _)| {
        piece_state.owner_player_id == current_player
            && piece_state.status == crate::domain::piece::PieceStatus::Active
    });
    if !has_movable_piece {
        return false;
    }

    if arm_dash(skill_roster, current_player) {
        mark_skill_used(skill_roster, current_player);
        skill_roster.last_skill_action = Some(format!(
            "P{} (AI) armed Dash for +3 movement",
            current_player
        ));
        return true;
    }

    false
}

fn apply_shield_to_piece_to_turn_query(
    piece_id: u8,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> Option<u8> {
    const MAX_PIECE_SHIELD: u8 = 2;
    for (query_piece_id, _, mut piece_state, _) in piece_query.iter_mut() {
        if query_piece_id.0 != piece_id {
            continue;
        }

        piece_state.shield = piece_state
            .shield
            .saturating_add(1)
            .min(MAX_PIECE_SHIELD);
        return Some(piece_state.shield);
    }

    None
}

fn execute_snipe_on_turn_query(
    piece_id: u8,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
    ai_actor: bool,
) -> String {
    for (query_piece_id, hangar_slot, mut piece_state, mut transform) in piece_query.iter_mut() {
        if query_piece_id.0 != piece_id {
            continue;
        }

        let prefix = if ai_actor { "AI Snipe" } else { "Snipe" };
        if piece_state.shield > 0 {
            piece_state.shield -= 1;
            return format!("{prefix} hit piece #{piece_id} and removed a shield");
        }
        if piece_state.stack_shield > 0 {
            piece_state.stack_shield = 0;
            return format!("{prefix} hit piece #{piece_id} and broke the shared shield");
        }

        piece_state.status = crate::domain::piece::PieceStatus::InHangar;
        piece_state.progress = 0;
        piece_state.shield = 0;
        piece_state.stack_shield = 0;
        transform.translation.x = hangar_slot.0.x;
        transform.translation.y = hangar_slot.0.y;
        return format!("{prefix} sent piece #{piece_id} back to hangar");
    }

    if ai_actor {
        "AI Snipe failed to resolve".to_string()
    } else {
        "Snipe failed to resolve".to_string()
    }
}

fn execute_swap_on_turn_query(
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
        return "AI Swap failed: current active piece not found".to_string();
    };

    let Some((teammate_state, teammate_translation)) = piece_query
        .iter()
        .find_map(|(piece_id, _, piece_state, transform)| {
            (piece_id.0 == teammate_piece_id).then_some((*piece_state, transform.translation))
        })
    else {
        return "AI Swap failed: teammate piece not found".to_string();
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
        "AI Swap exchanged piece #{} with teammate piece #{}",
        current_piece_id, teammate_piece_id
    )
}

fn handle_human_roll_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut input_state: ResMut<TurnInputState>,
    mut turn_state: ResMut<TurnState>,
    mut next_phase: ResMut<NextState<GamePhase>>,
    game_phase: Res<State<GamePhase>>,
    board_layout: Res<BoardLayout>,
    match_config: Res<MatchConfig>,
    player_roster: Res<PlayerRoster>,
    team_roster: Res<TeamRoster>,
    mut skill_roster: ResMut<SkillRoster>,
    mut match_result: ResMut<MatchResult>,
    mut piece_query: Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) {
    if !matches!(game_phase.get(), GamePhase::AwaitDice) || match_result.finished {
        return;
    }

    if current_player_control(turn_state.current_player, &player_roster)
        != Some(PlayerControl::Human)
    {
        return;
    }

    input_state.prompt = Some("Press Space to roll".to_string());

    if !keyboard.just_pressed(KeyCode::Space) {
        return;
    }

    let roll_resolution = resolve_roll_value(&mut skill_roster, turn_state.current_player);
    let roll_value = roll_resolution.value;
    let roll = DiceRoll(roll_value);
    set_roll(&mut turn_state, roll_value);
    if roll_resolution.used_double_dice {
        skill_roster.last_skill_action = Some(format!(
            "P{} resolved DoubleDice into {}",
            turn_state.current_player, roll_value
        ));
    }

    let current_player = turn_state.current_player;
    let actions = collect_actions(current_player, roll, 0, &player_roster, &piece_query);

    if actions.is_empty() {
        turn_state.last_action = Some(format!(
            "P{} rolled {} but had no legal action",
            turn_state.current_player, roll_value
        ));
        finish_turn_without_action(
            &mut turn_state,
            &mut input_state,
            &player_roster,
            &mut next_phase,
        );
        return;
    }

    let can_offer_dash = can_use_skill_this_turn(&skill_roster, current_player)
        && dash_bonus(&skill_roster, current_player) == 0
        && player_skill_state(&skill_roster, current_player)
            .map(|skills| skills.dash_charges > 0)
            .unwrap_or(false)
        && actions.iter().any(PlannedAction::is_move);

    if actions.len() == 1 && !can_offer_dash {
        execute_action(
            actions[0],
            roll_value,
            &player_roster,
            &team_roster,
            &match_config,
            &board_layout,
            &mut piece_query,
            &mut skill_roster,
            &mut match_result,
            &mut turn_state,
            &mut input_state,
            &mut next_phase,
        );
        return;
    }

    set_pending_actions(&mut input_state, roll_value, actions, &mut next_phase);
    if can_offer_dash {
        input_state.prompt = Some(format!(
            "Rolled {}. Press E for Dash (+3), then click a highlighted piece or press {}",
            roll_value,
            (1..=input_state.candidate_piece_ids().len())
                .map(|index| index.to_string())
                .collect::<Vec<_>>()
                .join("/")
        ));
    }
}

fn handle_human_action_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut input_state: ResMut<TurnInputState>,
    mut turn_state: ResMut<TurnState>,
    mut next_phase: ResMut<NextState<GamePhase>>,
    game_phase: Res<State<GamePhase>>,
    board_layout: Res<BoardLayout>,
    match_config: Res<MatchConfig>,
    player_roster: Res<PlayerRoster>,
    team_roster: Res<TeamRoster>,
    mut skill_roster: ResMut<SkillRoster>,
    mut match_result: ResMut<MatchResult>,
    mut piece_query: Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) {
    if !matches!(game_phase.get(), GamePhase::AwaitPieceSelect) || match_result.finished {
        return;
    }

    refresh_pending_actions_for_dash(
        &mut input_state,
        &turn_state,
        &player_roster,
        &mut piece_query,
        &skill_roster,
        &mut next_phase,
    );

    let Some(selection) = pressed_selection_key(&keyboard, input_state.candidate_piece_ids().len())
    else {
        return;
    };
    let Some(action) = get_pending_action(&input_state, selection) else {
        return;
    };

    let roll_value = turn_state.last_roll.unwrap_or_default();
    execute_action(
        action,
        roll_value,
        &player_roster,
        &team_roster,
        &match_config,
        &board_layout,
        &mut piece_query,
        &mut skill_roster,
        &mut match_result,
        &mut turn_state,
        &mut input_state,
        &mut next_phase,
    );
    clear_dash_arm(&mut skill_roster, turn_state.current_player);
}

fn handle_human_action_click(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform)>,
    mut input_state: ResMut<TurnInputState>,
    mut turn_state: ResMut<TurnState>,
    mut next_phase: ResMut<NextState<GamePhase>>,
    game_phase: Res<State<GamePhase>>,
    board_layout: Res<BoardLayout>,
    match_config: Res<MatchConfig>,
    player_roster: Res<PlayerRoster>,
    team_roster: Res<TeamRoster>,
    mut skill_roster: ResMut<SkillRoster>,
    mut match_result: ResMut<MatchResult>,
    mut piece_query: Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) {
    if !matches!(game_phase.get(), GamePhase::AwaitPieceSelect) || match_result.finished {
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

    refresh_pending_actions_for_dash(
        &mut input_state,
        &turn_state,
        &player_roster,
        &mut piece_query,
        &skill_roster,
        &mut next_phase,
    );

    let mut selected_piece_id = None;
    let mut best_distance_sq = f32::MAX;
    for (piece_id, _, _, transform) in &mut piece_query {
        if !input_state.candidate_piece_ids().contains(&piece_id.0) {
            continue;
        }

        let distance_sq = transform.translation.truncate().distance_squared(cursor_world);
        if distance_sq <= 28.0 * 28.0 && distance_sq < best_distance_sq {
            best_distance_sq = distance_sq;
            selected_piece_id = Some(piece_id.0);
        }
    }

    let Some(selected_piece_id) = selected_piece_id else {
        return;
    };
    let Some(action) = find_pending_action_by_piece_id(&input_state, selected_piece_id) else {
        return;
    };

    let roll_value = turn_state.last_roll.unwrap_or_default();
    execute_action(
        action,
        roll_value,
        &player_roster,
        &team_roster,
        &match_config,
        &board_layout,
        &mut piece_query,
        &mut skill_roster,
        &mut match_result,
        &mut turn_state,
        &mut input_state,
        &mut next_phase,
    );
    clear_dash_arm(&mut skill_roster, turn_state.current_player);
}

fn refresh_pending_actions_for_dash(
    input_state: &mut TurnInputState,
    turn_state: &TurnState,
    player_roster: &PlayerRoster,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
    skill_roster: &SkillRoster,
    next_phase: &mut ResMut<NextState<GamePhase>>,
) {
    let move_bonus = dash_bonus(skill_roster, turn_state.current_player);
    if move_bonus == 0 || input_state.candidate_piece_ids().is_empty() {
        return;
    }

    let refreshed_actions = collect_actions(
        turn_state.current_player,
        DiceRoll(turn_state.last_roll.unwrap_or_default()),
        move_bonus,
        player_roster,
        piece_query,
    );
    if refreshed_actions.is_empty() {
        return;
    }

    set_pending_actions(
        input_state,
        turn_state.last_roll.unwrap_or_default(),
        refreshed_actions,
        next_phase,
    );
    input_state.prompt = Some(format!(
        "Dash active (+{}). Click a highlighted piece or press {}",
        move_bonus,
        (1..=input_state.candidate_piece_ids().len())
            .map(|index| index.to_string())
            .collect::<Vec<_>>()
            .join("/")
    ));
}
