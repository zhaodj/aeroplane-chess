use bevy::prelude::*;
use rand::random_range;

use crate::data::game_mode::GameMode;
use crate::domain::dice::DiceRoll;
use crate::domain::event::TileEventKind;
use crate::domain::piece::{PieceState, PieceStatus};
use crate::domain::player::PlayerControl;
use crate::domain::rules::can_launch;
use crate::domain::tile::TileKind;
use crate::gameplay::match_flow::{
    evaluate_match_result, BoardLayout, MatchConfig, MatchResult, PlayerProfile, PlayerRoster,
    TeamRoster,
};
use crate::gameplay::skill_flow::{
    disable_next_skill_turn, grant_random_skill_charge, SkillRoster,
};
use crate::plugins::piece_plugin::{HangarSlot, PieceId};
use crate::states::GamePhase;

pub const MAIN_ROUTE_STEPS: u8 = 32;
pub const HOME_LANE_STEPS: u8 = 4;
pub const FINISH_DISTANCE: u8 = MAIN_ROUTE_STEPS + HOME_LANE_STEPS;
pub const MAX_CHAIN_EXTRA_ROLLS: u8 = 3;
pub const MAX_PIECE_SHIELD: u8 = 2;

#[derive(Clone, Debug, Default, Eq, PartialEq, Resource)]
pub struct TurnState {
    pub current_player: u8,
    pub extra_rolls_remaining: u8,
    pub consecutive_sixes: u8,
    pub turn_index: u32,
    pub current_roll: Option<u8>,
    pub last_roll: Option<u8>,
    pub last_action: Option<String>,
}

impl TurnState {
    pub fn opening_turn() -> Self {
        Self {
            current_player: 1,
            extra_rolls_remaining: 0,
            consecutive_sixes: 0,
            turn_index: 1,
            current_roll: None,
            last_roll: None,
            last_action: None,
        }
    }
}

#[derive(Resource, Default)]
pub struct TurnInputState {
    pending_actions: Vec<PlannedAction>,
    candidate_piece_ids: Vec<u8>,
    pub prompt: Option<String>,
}

impl TurnInputState {
    pub fn candidate_piece_ids(&self) -> &[u8] {
        &self.candidate_piece_ids
    }
}

#[derive(Clone, Copy, Debug)]
pub enum PlannedAction {
    Launch { piece_id: u8, target_progress: u8 },
    Move { piece_id: u8, target_progress: u8 },
}

impl PlannedAction {
    pub fn piece_id(&self) -> u8 {
        match *self {
            Self::Launch { piece_id, .. } | Self::Move { piece_id, .. } => piece_id,
        }
    }

    pub fn is_move(&self) -> bool {
        matches!(self, Self::Move { .. })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoardPosition {
    Main(u8),
    Home(u8),
    Goal,
}

#[derive(Clone, Copy, Debug)]
struct PieceSnapshot {
    piece_id: u8,
    owner_player_id: u8,
    team_id: u8,
    status: PieceStatus,
    distance: u8,
    shield: u8,
    board_position: Option<BoardPosition>,
}

#[derive(Clone, Copy, Debug)]
struct ActionOrigin {
    status: PieceStatus,
    progress: u8,
    translation: Vec3,
    new_progress: u8,
}

pub fn roll_die() -> u8 {
    random_range(1..=6)
}

pub fn current_player_control(current_player: u8, player_roster: &PlayerRoster) -> Option<PlayerControl> {
    player_roster
        .players
        .iter()
        .find(|player| player.state.player_id == current_player)
        .map(|player| player.state.control)
}

pub fn pressed_selection_key(
    keyboard: &ButtonInput<KeyCode>,
    max_actions: usize,
) -> Option<usize> {
    let keys = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4];
    keys.iter()
        .enumerate()
        .find_map(|(index, key)| {
            (index < max_actions && keyboard.just_pressed(*key)).then_some(index)
        })
}

pub fn set_roll(turn_state: &mut TurnState, roll_value: u8) {
    turn_state.current_roll = Some(roll_value);
    turn_state.last_roll = Some(roll_value);

    if roll_value == 6 {
        if turn_state.consecutive_sixes < MAX_CHAIN_EXTRA_ROLLS {
            turn_state.extra_rolls_remaining = turn_state.extra_rolls_remaining.saturating_add(1);
        }
        turn_state.consecutive_sixes = turn_state.consecutive_sixes.saturating_add(1);
    } else {
        turn_state.consecutive_sixes = 0;
    }
}

pub fn choose_action(
    current_player: u8,
    roll: DiceRoll,
    move_bonus: u8,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
    piece_query: &Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> Option<PlannedAction> {
    if board_layout.route_len() == 0 {
        return None;
    }

    let snapshots = collect_piece_snapshots(player_roster, piece_query);
    let player_profile = player_roster
        .players
        .iter()
        .find(|player| player.state.player_id == current_player)?;

    if roll.0 == 6 {
        for piece in snapshots.iter().filter(|piece| piece.owner_player_id == current_player) {
            if piece.status != PieceStatus::InHangar {
                continue;
            }

            if is_enemy_on_progress(
                &snapshots,
                current_player,
                player_profile.state.team_id,
                board_position_for_distance(player_profile, 0, PieceStatus::Active),
            ) {
                return Some(PlannedAction::Launch {
                    piece_id: piece.piece_id,
                    target_progress: 0,
                });
            }

            if can_launch(
                &PieceState {
                    owner_player_id: piece.owner_player_id,
                    team_id: piece.team_id,
                    status: piece.status,
                    progress: piece.distance,
                    shield: piece.shield,
                    stack_shield: 0,
                },
                roll,
            ) {
                return Some(PlannedAction::Launch {
                    piece_id: piece.piece_id,
                    target_progress: 0,
                });
            }
        }
    }

    for piece in snapshots.iter().filter(|piece| piece.owner_player_id == current_player) {
        if piece.status != PieceStatus::Active {
            continue;
        }

        let Some(target_progress) =
            compute_target_distance(piece.distance, roll.0.saturating_add(move_bonus))
        else {
            continue;
        };

        if is_enemy_on_progress(
            &snapshots,
            current_player,
            piece.team_id,
            board_position_for_distance(player_profile, target_progress, PieceStatus::Active),
        ) {
            return Some(PlannedAction::Move {
                piece_id: piece.piece_id,
                target_progress,
            });
        }
    }

    if roll.0 == 6 {
        for piece in snapshots.iter().filter(|piece| piece.owner_player_id == current_player) {
            if piece.status == PieceStatus::InHangar {
                return Some(PlannedAction::Launch {
                    piece_id: piece.piece_id,
                    target_progress: 0,
                });
            }
        }
    }

    for piece in snapshots.iter().filter(|piece| piece.owner_player_id == current_player) {
        if piece.status == PieceStatus::Active {
            let Some(target_progress) =
                compute_target_distance(piece.distance, roll.0.saturating_add(move_bonus))
            else {
                continue;
            };

            return Some(PlannedAction::Move {
                piece_id: piece.piece_id,
                target_progress,
            });
        }
    }

    None
}

pub fn collect_actions(
    current_player: u8,
    roll: DiceRoll,
    move_bonus: u8,
    player_roster: &PlayerRoster,
    piece_query: &Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> Vec<PlannedAction> {
    let snapshots = collect_piece_snapshots(player_roster, piece_query);
    if player_roster
        .players
        .iter()
        .all(|player| player.state.player_id != current_player)
    {
        return Vec::new();
    }

    let mut actions = Vec::new();

    if roll.0 == 6 {
        for piece in snapshots.iter().filter(|piece| piece.owner_player_id == current_player) {
            if piece.status == PieceStatus::InHangar
                && can_launch(
                    &PieceState {
                        owner_player_id: piece.owner_player_id,
                        team_id: piece.team_id,
                        status: piece.status,
                        progress: piece.distance,
                        shield: piece.shield,
                        stack_shield: 0,
                    },
                    roll,
                )
            {
                actions.push(PlannedAction::Launch {
                    piece_id: piece.piece_id,
                    target_progress: 0,
                });
            }
        }
    }

    for piece in snapshots.iter().filter(|piece| piece.owner_player_id == current_player) {
        if piece.status != PieceStatus::Active {
            continue;
        }

        if let Some(target_progress) =
            compute_target_distance(piece.distance, roll.0.saturating_add(move_bonus))
        {
            actions.push(PlannedAction::Move {
                piece_id: piece.piece_id,
                target_progress,
            });
        }
    }

    actions
}

pub fn execute_action(
    action: PlannedAction,
    roll_value: u8,
    player_roster: &PlayerRoster,
    team_roster: &TeamRoster,
    match_config: &MatchConfig,
    board_layout: &BoardLayout,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
    skill_roster: &mut SkillRoster,
    match_result: &mut MatchResult,
    turn_state: &mut TurnState,
    input_state: &mut TurnInputState,
    next_phase: &mut ResMut<NextState<GamePhase>>,
) {
    clear_stack_from_origin(&action, player_roster, piece_query);
    let action_origin = apply_action(&action, board_layout, player_roster, piece_query);
    let initial_progress = action_origin
        .map(|origin| origin.new_progress)
        .unwrap_or_default();
    let mut notes = Vec::new();
    apply_tile_effects(
        &action,
        initial_progress,
        board_layout,
        player_roster,
        piece_query,
        skill_roster,
        &mut notes,
    );
    apply_team_stack(&action, player_roster, match_config, piece_query, &mut notes);
    resolve_collision(
        &action,
        action_origin,
        player_roster,
        match_config,
        piece_query,
        &mut notes,
    );

    let finished_player_ids = piece_query
        .iter()
        .filter_map(|(_, _, piece_state, _)| {
            (piece_state.status == PieceStatus::Finished).then_some(piece_state.owner_player_id)
        })
        .collect::<Vec<_>>();

    let evaluated_result = evaluate_match_result(team_roster, &finished_player_ids);
    if evaluated_result.finished {
        match_result.winner_team_id = evaluated_result.winner_team_id;
        match_result.winner_player_ids = evaluated_result.winner_player_ids.clone();
        match_result.finished = true;
        notes.push(format!(
            "team {} wins",
            evaluated_result.winner_team_id.unwrap_or_default()
        ));
    }

    turn_state.last_action = Some(describe_action(&action, roll_value, &notes));
    clear_pending_input(input_state);

    if match_result.finished {
        next_phase.set(GamePhase::CheckVictory);
        return;
    }

    advance_turn(turn_state, player_roster.players.len() as u8);
    next_phase.set(GamePhase::AwaitDice);
}

pub fn finish_turn_without_action(
    turn_state: &mut TurnState,
    input_state: &mut TurnInputState,
    player_roster: &PlayerRoster,
    next_phase: &mut ResMut<NextState<GamePhase>>,
) {
    clear_pending_input(input_state);
    advance_turn(turn_state, player_roster.players.len() as u8);
    next_phase.set(GamePhase::AwaitDice);
}

pub fn set_pending_actions(
    input_state: &mut TurnInputState,
    roll_value: u8,
    actions: Vec<PlannedAction>,
    next_phase: &mut ResMut<NextState<GamePhase>>,
) {
    input_state.prompt = Some(format!(
        "Rolled {}. Click a highlighted piece or press {}",
        roll_value,
        (1..=actions.len())
            .map(|index| index.to_string())
            .collect::<Vec<_>>()
            .join("/")
    ));
    input_state.candidate_piece_ids = actions.iter().map(|action| action.piece_id()).collect();
    input_state.pending_actions = actions;
    next_phase.set(GamePhase::AwaitPieceSelect);
}

pub fn get_pending_action(input_state: &TurnInputState, selection: usize) -> Option<PlannedAction> {
    input_state.pending_actions.get(selection).copied()
}

pub fn find_pending_action_by_piece_id(
    input_state: &TurnInputState,
    piece_id: u8,
) -> Option<PlannedAction> {
    input_state
        .pending_actions
        .iter()
        .copied()
        .find(|action| action.piece_id() == piece_id)
}

pub fn clear_pending_input(input_state: &mut TurnInputState) {
    input_state.pending_actions.clear();
    input_state.candidate_piece_ids.clear();
    input_state.prompt = None;
}

pub fn compute_target_distance(current_distance: u8, roll_value: u8) -> Option<u8> {
    current_distance
        .checked_add(roll_value)
        .filter(|next_distance| *next_distance <= FINISH_DISTANCE)
}

pub fn board_position_for_distance(
    player_profile: &PlayerProfile,
    distance: u8,
    status: PieceStatus,
) -> Option<BoardPosition> {
    match status {
        PieceStatus::InHangar => None,
        PieceStatus::Finished => Some(BoardPosition::Goal),
        PieceStatus::Active if distance < MAIN_ROUTE_STEPS => Some(BoardPosition::Main(
            (player_profile.launch_tile_index + distance) % MAIN_ROUTE_STEPS,
        )),
        PieceStatus::Active if distance < FINISH_DISTANCE => {
            Some(BoardPosition::Home(distance - MAIN_ROUTE_STEPS))
        }
        PieceStatus::Active if distance == FINISH_DISTANCE => Some(BoardPosition::Goal),
        _ => None,
    }
}

pub fn world_position_for_piece(
    owner_player_id: u8,
    distance: u8,
    status: PieceStatus,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
) -> Option<Vec2> {
    let player_profile = player_roster
        .players
        .iter()
        .find(|player| player.state.player_id == owner_player_id)?;

    match board_position_for_distance(player_profile, distance, status)? {
        BoardPosition::Main(tile_index) => board_layout.world_pos_for_route_index(tile_index),
        BoardPosition::Home(home_index) => player_profile
            .home_lane_positions
            .get(home_index as usize)
            .copied(),
        BoardPosition::Goal => Some(player_profile.goal_position),
    }
}

fn collect_piece_snapshots(
    player_roster: &PlayerRoster,
    piece_query: &Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> Vec<PieceSnapshot> {
    piece_query
        .iter()
        .map(|(piece_id, _, piece_state, _)| {
            let player_profile = player_roster
                .players
                .iter()
                .find(|player| player.state.player_id == piece_state.owner_player_id);

            PieceSnapshot {
                piece_id: piece_id.0,
                owner_player_id: piece_state.owner_player_id,
                team_id: piece_state.team_id,
                status: piece_state.status,
                distance: piece_state.progress,
                shield: piece_state.shield,
                board_position: player_profile.and_then(|profile| {
                    board_position_for_distance(profile, piece_state.progress, piece_state.status)
                }),
            }
        })
        .collect()
}

fn is_enemy_on_progress(
    snapshots: &[PieceSnapshot],
    current_player: u8,
    current_team: u8,
    target_position: Option<BoardPosition>,
) -> bool {
    snapshots.iter().any(|piece| {
        piece.owner_player_id != current_player
            && piece.team_id != current_team
            && piece.status == PieceStatus::Active
            && target_position.is_some()
            && piece.board_position == target_position
    })
}

fn apply_action(
    action: &PlannedAction,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> Option<ActionOrigin> {
    for (piece_id, _, mut piece_state, mut transform) in piece_query.iter_mut() {
        let (target_piece_id, target_progress, next_status) = match *action {
            PlannedAction::Launch {
                piece_id,
                target_progress,
            } => (piece_id, target_progress, PieceStatus::Active),
            PlannedAction::Move {
                piece_id,
                target_progress,
            } => (
                piece_id,
                target_progress,
                if target_progress == FINISH_DISTANCE {
                    PieceStatus::Finished
                } else {
                    PieceStatus::Active
                },
            ),
        };

        if piece_id.0 != target_piece_id {
            continue;
        }

        let previous_status = piece_state.status;
        let previous_progress = piece_state.progress;
        let previous_translation = transform.translation;
        piece_state.status = next_status;
        piece_state.progress = target_progress;
        if let Some(world_pos) = world_position_for_piece(
            piece_state.owner_player_id,
            target_progress,
            next_status,
            board_layout,
            player_roster,
        ) {
            transform.translation.x = world_pos.x;
            transform.translation.y = world_pos.y;
        }
        return Some(ActionOrigin {
            status: previous_status,
            progress: previous_progress,
            translation: previous_translation,
            new_progress: target_progress,
        });
    }

    None
}

fn apply_tile_effects(
    action: &PlannedAction,
    mut final_progress: u8,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
    skill_roster: &mut SkillRoster,
    notes: &mut Vec<String>,
) -> u8 {
    if board_layout.route_len() == 0 || final_progress >= MAIN_ROUTE_STEPS {
        return final_progress;
    }

    let Some(BoardPosition::Main(tile_index)) =
        attacker_position(action, player_roster, piece_query) else {
        return final_progress;
    };

    let Some(tile_kind) = board_layout.tile_kind_for_route_index(tile_index) else {
        return final_progress;
    };

    match tile_kind {
        TileKind::Jump => {
            final_progress = (final_progress + 4).min(FINISH_DISTANCE);
            update_piece_progress(
                action,
                final_progress,
                board_layout,
                player_roster,
                piece_query,
            );
            notes.push(format!("jumped to tile {final_progress}"));
        }
        TileKind::Attack => {
            notes.push("attack tile primed".to_string());
        }
        TileKind::Defense => {
            if let Some(shield) = modify_piece_shield(action, piece_query, 1) {
                notes.push(format!("gained shield ({shield})"));
            }
        }
        TileKind::Event => {
            if let Some(event_note) = apply_event_effect(
                action,
                &mut final_progress,
                board_layout,
                player_roster,
                piece_query,
                skill_roster,
            ) {
                notes.push(event_note);
            }
        }
        TileKind::Goal | TileKind::Normal => {}
    }

    final_progress
}

fn resolve_collision(
    action: &PlannedAction,
    action_origin: Option<ActionOrigin>,
    player_roster: &PlayerRoster,
    match_config: &MatchConfig,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
    notes: &mut Vec<String>,
) {
    let Some(attacker_board_position) = attacker_position(action, player_roster, piece_query) else {
        return;
    };
    let BoardPosition::Main(target_tile_index) = attacker_board_position else {
        return;
    };

    let attacker_piece_id = action.piece_id();
    let mut attacker_team = None;
    for (piece_id, _, piece_state, _) in piece_query.iter() {
        if piece_id.0 == attacker_piece_id {
            attacker_team = Some(piece_state.team_id);
            break;
        }
    }

    let Some(attacker_team) = attacker_team else {
        return;
    };

    let mut defenders_with_stack = Vec::new();
    for (piece_id, _, piece_state, _) in piece_query.iter_mut() {
        if piece_id.0 == attacker_piece_id {
            continue;
        }

        if piece_state.status != PieceStatus::Active
            || piece_state.team_id == attacker_team
            || piece_board_position(*piece_state, player_roster) != Some(BoardPosition::Main(target_tile_index))
        {
            continue;
        }

        if match_config.mode == GameMode::TwoVsTwo && piece_state.stack_shield > 0 {
            defenders_with_stack.push(piece_id.0);
        }
    }

    if !defenders_with_stack.is_empty() {
        consume_stack_shield(&defenders_with_stack, piece_query);
        notes.push("shared stack shield blocked collision".to_string());
        restore_attacker_origin(action, action_origin, piece_query);
        return;
    }

    let mut collision_blocked = false;
    for (piece_id, hangar_slot, mut piece_state, mut transform) in piece_query.iter_mut() {
        if piece_id.0 == attacker_piece_id {
            continue;
        }

        if piece_state.status != PieceStatus::Active
            || piece_state.team_id == attacker_team
            || piece_board_position(*piece_state, player_roster) != Some(BoardPosition::Main(target_tile_index))
        {
            continue;
        }

        if piece_state.shield > 0 {
            piece_state.shield -= 1;
            collision_blocked = true;
            notes.push(format!("piece #{} blocked collision with shield", piece_id.0));
            continue;
        }

        piece_state.status = PieceStatus::InHangar;
        piece_state.progress = 0;
        piece_state.shield = 0;
        piece_state.stack_shield = 0;
        transform.translation.x = hangar_slot.0.x;
        transform.translation.y = hangar_slot.0.y;
        notes.push(format!("sent piece #{} back to hangar", piece_id.0));
    }

    if collision_blocked {
        restore_attacker_origin(action, action_origin, piece_query);
        notes.push("attacker bounced back after shield block".to_string());
    }
}

fn restore_attacker_origin(
    action: &PlannedAction,
    action_origin: Option<ActionOrigin>,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) {
    let Some(action_origin) = action_origin else {
        return;
    };

    for (piece_id, _, mut piece_state, mut transform) in piece_query.iter_mut() {
        if piece_id.0 != action.piece_id() {
            continue;
        }
        piece_state.status = action_origin.status;
        piece_state.progress = action_origin.progress;
        transform.translation = action_origin.translation;
        break;
    }
}

fn update_piece_progress(
    action: &PlannedAction,
    target_progress: u8,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) {
    let target_piece_id = action.piece_id();

    for (piece_id, _, mut piece_state, mut transform) in piece_query.iter_mut() {
        if piece_id.0 != target_piece_id {
            continue;
        }

        piece_state.progress = target_progress;
        piece_state.status = if target_progress == FINISH_DISTANCE {
            PieceStatus::Finished
        } else {
            PieceStatus::Active
        };
        if let Some(world_pos) = world_position_for_piece(
            piece_state.owner_player_id,
            target_progress,
            piece_state.status,
            board_layout,
            player_roster,
        ) {
            transform.translation.x = world_pos.x;
            transform.translation.y = world_pos.y;
        }
        break;
    }
}

fn modify_piece_shield(
    action: &PlannedAction,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
    delta: u8,
) -> Option<u8> {
    let target_piece_id = action.piece_id();

    for (piece_id, _, mut piece_state, _) in piece_query.iter_mut() {
        if piece_id.0 != target_piece_id {
            continue;
        }

        piece_state.shield = piece_state
            .shield
            .saturating_add(delta)
            .min(MAX_PIECE_SHIELD);
        return Some(piece_state.shield);
    }

    None
}

fn clear_stack_from_origin(
    action: &PlannedAction,
    player_roster: &PlayerRoster,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) {
    let moving_piece_id = action.piece_id();
    let mut origin_position = None;
    let mut moving_team = None;

    for (piece_id, _, piece_state, _) in piece_query.iter_mut() {
        if piece_id.0 == moving_piece_id {
            origin_position = piece_board_position(*piece_state, player_roster);
            moving_team = Some(piece_state.team_id);
            break;
        }
    }

    let Some(origin_position) = origin_position else {
        return;
    };
    let Some(moving_team) = moving_team else {
        return;
    };

    let mut same_tile_piece_ids = Vec::new();
    for (piece_id, _, piece_state, _) in piece_query.iter_mut() {
        if piece_id.0 == moving_piece_id {
            continue;
        }

        if piece_state.team_id == moving_team
            && piece_state.status == PieceStatus::Active
            && piece_board_position(*piece_state, player_roster) == Some(origin_position)
        {
            same_tile_piece_ids.push(piece_id.0);
        }
    }

    if same_tile_piece_ids.is_empty() {
        return;
    }

    for (piece_id, _, mut piece_state, _) in piece_query.iter_mut() {
        if piece_id.0 == moving_piece_id || same_tile_piece_ids.contains(&piece_id.0) {
            piece_state.stack_shield = 0;
        }
    }
}

fn apply_team_stack(
    action: &PlannedAction,
    player_roster: &PlayerRoster,
    match_config: &MatchConfig,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
    notes: &mut Vec<String>,
) {
    if match_config.mode != GameMode::TwoVsTwo {
        return;
    }

    let moving_piece_id = action.piece_id();
    let mut landing_position = None;
    let mut moving_team = None;

    for (piece_id, _, piece_state, _) in piece_query.iter_mut() {
        if piece_id.0 == moving_piece_id {
            landing_position = piece_board_position(*piece_state, player_roster);
            moving_team = Some(piece_state.team_id);
            break;
        }
    }

    let Some(landing_position) = landing_position else {
        return;
    };
    let Some(moving_team) = moving_team else {
        return;
    };

    let mut stack_piece_ids = vec![moving_piece_id];
    for (piece_id, _, piece_state, _) in piece_query.iter_mut() {
        if piece_id.0 == moving_piece_id {
            continue;
        }

        if piece_state.team_id == moving_team
            && piece_state.status == PieceStatus::Active
            && piece_board_position(*piece_state, player_roster) == Some(landing_position)
        {
            stack_piece_ids.push(piece_id.0);
        }
    }

    if stack_piece_ids.len() < 2 {
        return;
    }

    for (piece_id, _, mut piece_state, _) in piece_query.iter_mut() {
        if stack_piece_ids.contains(&piece_id.0) {
            piece_state.stack_shield = 1;
        }
    }

    notes.push("stacked with teammate (shared shield 1)".to_string());
}

fn consume_stack_shield(
    defender_piece_ids: &[u8],
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) {
    for (piece_id, _, mut piece_state, _) in piece_query.iter_mut() {
        if defender_piece_ids.contains(&piece_id.0) {
            piece_state.stack_shield = 0;
        }
    }
}

fn apply_event_effect(
    action: &PlannedAction,
    final_progress: &mut u8,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
    skill_roster: &mut SkillRoster,
) -> Option<String> {
    match random_range(0..=4) {
        0 => {
            let shield = modify_piece_shield(action, piece_query, 1)?;
            Some(format!(
                "event {:?}: gained shield ({shield})",
                TileEventKind::GainShield
            ))
        }
        1 => {
            let owner_player_id = owner_player_id_for_action(action, piece_query)?;
            let charged = grant_random_skill_charge(skill_roster, owner_player_id)
                .unwrap_or("UnknownSkill");
            Some(format!(
                "event {:?}: gained 1 {charged} charge",
                TileEventKind::GainSkillCharge
            ))
        }
        2 => {
            let next_progress = (*final_progress + 2).min(FINISH_DISTANCE);
            *final_progress = next_progress;
            update_piece_progress(action, next_progress, board_layout, player_roster, piece_query);
            Some(format!(
                "event {:?}: advanced to tile {next_progress}",
                TileEventKind::AdvanceTwo
            ))
        }
        3 => {
            let owner_player_id = owner_player_id_for_action(action, piece_query)?;
            if disable_next_skill_turn(skill_roster, owner_player_id) {
                Some(format!(
                    "event {:?}: next skill turn disabled for P{}",
                    TileEventKind::DisableNextSkill,
                    owner_player_id
                ))
            } else {
                Some("event fizzled: could not disable next skill turn".to_string())
            }
        }
        _ => {
            let target_piece_id = action.piece_id();
            let mut attacker_team = None;
            for (piece_id, _, piece_state, _) in piece_query.iter_mut() {
                if piece_id.0 == target_piece_id {
                    attacker_team = Some(piece_state.team_id);
                    break;
                }
            }

            let attacker_team = attacker_team?;
            for (piece_id, _, mut piece_state, _) in piece_query.iter_mut() {
                if piece_id.0 == target_piece_id
                    || piece_state.team_id == attacker_team
                    || piece_state.shield == 0
                {
                    continue;
                }

                piece_state.shield -= 1;
                return Some(format!(
                    "event {:?}: removed shield from piece #{}",
                    TileEventKind::RemoveEnemyShield,
                    piece_id.0
                ));
            }

            Some("event fizzled: no enemy shield to remove".to_string())
        }
    }
}

fn owner_player_id_for_action(
    action: &PlannedAction,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> Option<u8> {
    for (piece_id, _, piece_state, _) in piece_query.iter_mut() {
        if piece_id.0 == action.piece_id() {
            return Some(piece_state.owner_player_id);
        }
    }
    None
}

fn piece_board_position(piece_state: PieceState, player_roster: &PlayerRoster) -> Option<BoardPosition> {
    let player_profile = player_roster
        .players
        .iter()
        .find(|player| player.state.player_id == piece_state.owner_player_id)?;
    board_position_for_distance(player_profile, piece_state.progress, piece_state.status)
}

fn attacker_position(
    action: &PlannedAction,
    player_roster: &PlayerRoster,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> Option<BoardPosition> {
    let attacker_piece_id = action.piece_id();

    for (piece_id, _, piece_state, _) in piece_query.iter_mut() {
        if piece_id.0 == attacker_piece_id {
            return piece_board_position(*piece_state, player_roster);
        }
    }

    None
}

fn describe_action(action: &PlannedAction, roll_value: u8, notes: &[String]) -> String {
    let base = match *action {
        PlannedAction::Launch { piece_id, .. } => {
            format!("rolled {roll_value}, launched piece #{piece_id}")
        }
        PlannedAction::Move {
            piece_id,
            target_progress,
        } => format!(
            "rolled {roll_value}, moved piece #{piece_id} to tile {target_progress}"
        ),
    };

    if notes.is_empty() {
        base
    } else {
        format!("{base}; {}", notes.join(", "))
    }
}

pub fn advance_turn(turn_state: &mut TurnState, player_count: u8) {
    if turn_state.extra_rolls_remaining > 0 {
        turn_state.extra_rolls_remaining -= 1;
    } else {
        turn_state.current_player = if turn_state.current_player >= player_count {
            1
        } else {
            turn_state.current_player + 1
        };
        turn_state.consecutive_sixes = 0;
        turn_state.turn_index = turn_state.turn_index.saturating_add(1);
    }

    turn_state.current_roll = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::player::PlayerState;
    use bevy::ecs::system::SystemState;

    #[test]
    fn compute_target_distance_blocks_overshoot() {
        assert_eq!(compute_target_distance(FINISH_DISTANCE - 2, 2), Some(FINISH_DISTANCE));
        assert_eq!(compute_target_distance(FINISH_DISTANCE - 2, 3), None);
    }

    #[test]
    fn board_position_uses_player_launch_offset_on_main_route() {
        let (players, _) = crate::gameplay::match_flow::build_match_rosters(GameMode::TwoVsTwo);
        let player_one = &players[0];
        let player_two = &players[1];

        assert_eq!(
            board_position_for_distance(player_one, 0, PieceStatus::Active),
            Some(BoardPosition::Main(30))
        );
        assert_eq!(
            board_position_for_distance(player_two, 0, PieceStatus::Active),
            Some(BoardPosition::Main(6))
        );
        assert_eq!(
            board_position_for_distance(player_one, MAIN_ROUTE_STEPS, PieceStatus::Active),
            Some(BoardPosition::Home(0))
        );
        assert_eq!(
            board_position_for_distance(player_one, FINISH_DISTANCE, PieceStatus::Finished),
            Some(BoardPosition::Goal)
        );
    }

    #[test]
    fn advance_turn_consumes_extra_roll_before_switching_player() {
        let mut turn_state = TurnState {
            current_player: 1,
            extra_rolls_remaining: 1,
            consecutive_sixes: 1,
            turn_index: 3,
            current_roll: Some(6),
            last_roll: Some(6),
            last_action: None,
        };

        advance_turn(&mut turn_state, 4);
        assert_eq!(turn_state.current_player, 1);
        assert_eq!(turn_state.extra_rolls_remaining, 0);
        assert_eq!(turn_state.turn_index, 3);
        assert_eq!(turn_state.current_roll, None);

        advance_turn(&mut turn_state, 4);
        assert_eq!(turn_state.current_player, 2);
        assert_eq!(turn_state.consecutive_sixes, 0);
        assert_eq!(turn_state.turn_index, 4);
    }

    #[test]
    fn set_roll_caps_bonus_roll_chain_at_three() {
        let mut turn_state = TurnState::opening_turn();

        set_roll(&mut turn_state, 6);
        assert_eq!(turn_state.extra_rolls_remaining, 1);
        assert_eq!(turn_state.consecutive_sixes, 1);

        set_roll(&mut turn_state, 6);
        assert_eq!(turn_state.extra_rolls_remaining, 2);
        assert_eq!(turn_state.consecutive_sixes, 2);

        set_roll(&mut turn_state, 6);
        assert_eq!(turn_state.extra_rolls_remaining, 3);
        assert_eq!(turn_state.consecutive_sixes, 3);

        set_roll(&mut turn_state, 6);
        assert_eq!(turn_state.extra_rolls_remaining, 3);
        assert_eq!(turn_state.consecutive_sixes, 4);
    }

    #[test]
    fn world_position_for_piece_uses_home_lane_and_goal_positions() {
        let (players, _) = crate::gameplay::match_flow::build_match_rosters(GameMode::TwoVsTwo);
        let player_roster = PlayerRoster { players };
        let board_layout = BoardLayout {
            tiles: crate::data::board_config::default_board_tiles(),
        };

        assert_eq!(
            world_position_for_piece(1, 0, PieceStatus::Active, &board_layout, &player_roster),
            board_layout.world_pos_for_route_index(30)
        );
        assert_eq!(
            world_position_for_piece(
                1,
                MAIN_ROUTE_STEPS,
                PieceStatus::Active,
                &board_layout,
                &player_roster
            ),
            Some(Vec2::new(-128.0, 192.0))
        );
        assert_eq!(
            world_position_for_piece(
                1,
                FINISH_DISTANCE,
                PieceStatus::Finished,
                &board_layout,
                &player_roster
            ),
            Some(Vec2::new(-64.0, 0.0))
        );
    }

    #[test]
    fn collect_actions_returns_launch_and_move_options_for_human_player() {
        let (players, _) = crate::gameplay::match_flow::build_match_rosters(GameMode::OneVsOne);
        let player_roster = PlayerRoster { players };

        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::new(-320.0, 280.0)),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: PieceStatus::InHangar,
                progress: 0,
                shield: 0,
                stack_shield: 0,
            },
            Transform::default(),
        ));
        world.spawn((
            PieceId(2),
            HangarSlot(Vec2::new(-260.0, 280.0)),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: PieceStatus::Active,
                progress: 3,
                shield: 0,
                stack_shield: 0,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let query = system_state.get_mut(&mut world);

        let actions = collect_actions(1, DiceRoll(6), 0, &player_roster, &query);
        assert_eq!(actions.len(), 2);
        assert!(actions.iter().any(|action| matches!(action, PlannedAction::Launch { piece_id: 1, .. })));
        assert!(actions.iter().any(|action| matches!(action, PlannedAction::Move { piece_id: 2, target_progress: 9 })));
    }

    #[test]
    fn current_player_control_reads_player_roster() {
        let player_roster = PlayerRoster {
            players: vec![
                PlayerProfile {
                    state: PlayerState {
                        player_id: 1,
                        team_id: 1,
                        control: PlayerControl::Human,
                    },
                    color: Color::srgb(1.0, 0.0, 0.0),
                    hangar_slots: vec![],
                    launch_tile_index: 0,
                    home_lane_positions: vec![],
                    goal_position: Vec2::ZERO,
                },
                PlayerProfile {
                    state: PlayerState {
                        player_id: 2,
                        team_id: 2,
                        control: PlayerControl::Ai,
                    },
                    color: Color::srgb(0.0, 0.0, 1.0),
                    hangar_slots: vec![],
                    launch_tile_index: 0,
                    home_lane_positions: vec![],
                    goal_position: Vec2::ZERO,
                },
            ],
        };

        assert_eq!(current_player_control(1, &player_roster), Some(PlayerControl::Human));
        assert_eq!(current_player_control(2, &player_roster), Some(PlayerControl::Ai));
        assert_eq!(current_player_control(9, &player_roster), None);
    }

    #[test]
    fn apply_team_stack_grants_shared_shield_in_two_vs_two() {
        let (players, _) = crate::gameplay::match_flow::build_match_rosters(GameMode::TwoVsTwo);
        let player_roster = PlayerRoster { players };
        let match_config = MatchConfig {
            mode: GameMode::TwoVsTwo,
            ai_difficulty: crate::gameplay::ai::AiDifficulty::Normal,
            fast_mode: false,
        };

        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: PieceStatus::Active,
                progress: 2,
                shield: 0,
                stack_shield: 0,
            },
            Transform::default(),
        ));
        world.spawn((
            PieceId(2),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 3,
                team_id: 1,
                status: PieceStatus::Active,
                progress: 10,
                shield: 0,
                stack_shield: 0,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world);
        let mut notes = Vec::new();

        apply_team_stack(
            &PlannedAction::Move {
                piece_id: 1,
                target_progress: 2,
            },
            &player_roster,
            &match_config,
            &mut query,
            &mut notes,
        );

        let shields = query
            .iter_mut()
            .map(|(piece_id, _, piece_state, _)| (piece_id.0, piece_state.stack_shield))
            .collect::<Vec<_>>();
        assert_eq!(shields, vec![(1, 1), (2, 1)]);
        assert!(notes.iter().any(|note| note.contains("stacked with teammate")));
    }

    #[test]
    fn clear_stack_from_origin_removes_shared_shield_from_remaining_stack() {
        let (players, _) = crate::gameplay::match_flow::build_match_rosters(GameMode::TwoVsTwo);
        let player_roster = PlayerRoster { players };

        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: PieceStatus::Active,
                progress: 2,
                shield: 0,
                stack_shield: 1,
            },
            Transform::default(),
        ));
        world.spawn((
            PieceId(2),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 3,
                team_id: 1,
                status: PieceStatus::Active,
                progress: 10,
                shield: 0,
                stack_shield: 1,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world);

        clear_stack_from_origin(
            &PlannedAction::Move {
                piece_id: 1,
                target_progress: 4,
            },
            &player_roster,
            &mut query,
        );

        let shields = query
            .iter_mut()
            .map(|(piece_id, _, piece_state, _)| (piece_id.0, piece_state.stack_shield))
            .collect::<Vec<_>>();
        assert_eq!(shields, vec![(1, 0), (2, 0)]);
    }

    #[test]
    fn resolve_collision_consumes_shared_stack_shield_before_returning_to_hangar() {
        let (players, _) = crate::gameplay::match_flow::build_match_rosters(GameMode::TwoVsTwo);
        let player_roster = PlayerRoster { players };
        let match_config = MatchConfig {
            mode: GameMode::TwoVsTwo,
            ai_difficulty: crate::gameplay::ai::AiDifficulty::Normal,
            fast_mode: false,
        };

        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::new(-320.0, 280.0)),
            PieceState {
                owner_player_id: 2,
                team_id: 2,
                status: PieceStatus::Active,
                progress: 2,
                shield: 0,
                stack_shield: 0,
            },
            Transform::default(),
        ));
        world.spawn((
            PieceId(2),
            HangarSlot(Vec2::new(-320.0, 280.0)),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: PieceStatus::Active,
                progress: 10,
                shield: 0,
                stack_shield: 1,
            },
            Transform::default(),
        ));
        world.spawn((
            PieceId(3),
            HangarSlot(Vec2::new(-320.0, -280.0)),
            PieceState {
                owner_player_id: 3,
                team_id: 1,
                status: PieceStatus::Active,
                progress: 18,
                shield: 0,
                stack_shield: 1,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world);
        let mut notes = Vec::new();

        resolve_collision(
            &PlannedAction::Move {
                piece_id: 1,
                target_progress: 2,
            },
            None,
            &player_roster,
            &match_config,
            &mut query,
            &mut notes,
        );

        let states = query
            .iter_mut()
            .map(|(piece_id, _, piece_state, _)| {
                (piece_id.0, piece_state.status, piece_state.stack_shield)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            states,
            vec![
                (1, PieceStatus::Active, 0),
                (2, PieceStatus::Active, 0),
                (3, PieceStatus::Active, 0),
            ]
        );
        assert!(notes.iter().any(|note| note.contains("shared stack shield blocked collision")));
    }

    #[test]
    fn resolve_collision_with_shield_bounces_attacker_to_origin() {
        let (players, _) = crate::gameplay::match_flow::build_match_rosters(GameMode::TwoVsTwo);
        let player_roster = PlayerRoster { players };
        let match_config = MatchConfig {
            mode: GameMode::TwoVsTwo,
            ai_difficulty: crate::gameplay::ai::AiDifficulty::Normal,
            fast_mode: false,
        };

        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::new(-320.0, 280.0)),
            PieceState {
                owner_player_id: 2,
                team_id: 2,
                status: PieceStatus::Active,
                progress: 2,
                shield: 0,
                stack_shield: 0,
            },
            Transform::from_xyz(100.0, 100.0, 0.0),
        ));
        world.spawn((
            PieceId(2),
            HangarSlot(Vec2::new(-320.0, 280.0)),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: PieceStatus::Active,
                progress: 10,
                shield: 1,
                stack_shield: 0,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world);
        let mut notes = Vec::new();

        resolve_collision(
            &PlannedAction::Move {
                piece_id: 1,
                target_progress: 2,
            },
            Some(ActionOrigin {
                status: PieceStatus::Active,
                progress: 1,
                translation: Vec3::new(-50.0, -70.0, 0.0),
                new_progress: 2,
            }),
            &player_roster,
            &match_config,
            &mut query,
            &mut notes,
        );

        let states = query
            .iter_mut()
            .map(|(piece_id, _, piece_state, transform)| {
                (
                    piece_id.0,
                    piece_state.progress,
                    piece_state.shield,
                    transform.translation.x,
                    transform.translation.y,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            states,
            vec![(1, 1, 0, -50.0, -70.0), (2, 10, 0, 0.0, 0.0)]
        );
        assert!(notes.iter().any(|note| note.contains("bounced back")));
    }
}
