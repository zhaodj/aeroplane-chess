use bevy::prelude::*;
use rand::random_range;

use crate::domain::dice::DiceRoll;
use crate::domain::piece::{PieceState, PieceStatus};
use crate::domain::rules::can_launch;
use crate::domain::tile::TileKind;
use crate::gameplay::turn_flow::TurnState;
use crate::plugins::game_plugin::{
    evaluate_match_result, BoardLayout, MatchResult, PlayerProfile, PlayerRoster, TeamRoster,
};
use crate::plugins::piece_plugin::{HangarSlot, PieceId};
use crate::states::{AppState, GamePhase};

const MAIN_ROUTE_STEPS: u8 = 32;
const HOME_LANE_STEPS: u8 = 4;
const FINISH_DISTANCE: u8 = MAIN_ROUTE_STEPS + HOME_LANE_STEPS;

pub struct TurnPlugin;

impl Plugin for TurnPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), setup_turn_automation)
            .add_systems(
                Update,
                drive_turn_loop.run_if(in_state(AppState::InGame)),
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
}

fn cleanup_turn_automation(mut commands: Commands) {
    commands.remove_resource::<TurnAutomation>();
}

fn drive_turn_loop(
    time: Res<Time>,
    mut automation: ResMut<TurnAutomation>,
    mut turn_state: ResMut<TurnState>,
    mut next_phase: ResMut<NextState<GamePhase>>,
    game_phase: Res<State<GamePhase>>,
    board_layout: Res<BoardLayout>,
    player_roster: Res<PlayerRoster>,
    team_roster: Res<TeamRoster>,
    mut match_result: ResMut<MatchResult>,
    mut piece_query: Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) {
    if !matches!(game_phase.get(), GamePhase::AwaitDice) || match_result.finished {
        return;
    }

    if !automation.timer.tick(time.delta()).just_finished() {
        return;
    }

    let roll_value = random_range(1..=6);
    let roll = DiceRoll(roll_value);

    turn_state.current_roll = Some(roll_value);
    turn_state.last_roll = Some(roll_value);

    if roll_value == 6 {
        turn_state.extra_rolls_remaining = turn_state.extra_rolls_remaining.saturating_add(1);
    }

    let current_player = turn_state.current_player;

    let Some(action) = choose_action(current_player, roll, &board_layout, &player_roster, &piece_query) else {
        turn_state.last_action = Some(format!("P{current_player} rolled {roll_value} but had no legal action"));
        advance_turn(&mut turn_state, player_roster.players.len() as u8);
        next_phase.set(GamePhase::AwaitDice);
        return;
    };

    let initial_progress = apply_action(&action, &board_layout, &player_roster, &mut piece_query);
    let mut notes = Vec::new();
    let final_progress = apply_tile_effects(
        &action,
        initial_progress,
        &board_layout,
        &player_roster,
        &mut piece_query,
        &mut notes,
    );
    resolve_collision(
        &action,
        final_progress,
        &player_roster,
        &mut piece_query,
        &mut notes,
    );

    let finished_player_ids = piece_query
        .iter()
        .filter_map(|(_, _, piece_state, _)| {
            (piece_state.status == PieceStatus::Finished).then_some(piece_state.owner_player_id)
        })
        .collect::<Vec<_>>();

    let evaluated_result = evaluate_match_result(&team_roster, &finished_player_ids);
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

    if match_result.finished {
        next_phase.set(GamePhase::CheckVictory);
        return;
    }

    advance_turn(&mut turn_state, player_roster.players.len() as u8);
    next_phase.set(GamePhase::AwaitDice);
}

#[derive(Clone, Copy, Debug)]
enum PlannedAction {
    Launch { piece_id: u8, target_progress: u8 },
    Move { piece_id: u8, target_progress: u8 },
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

fn choose_action(
    current_player: u8,
    roll: DiceRoll,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
    piece_query: &Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> Option<PlannedAction> {
    if board_layout.route_len() == 0 {
        return None;
    }

    let snapshots = piece_query
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
        .collect::<Vec<_>>();

    let Some(player_profile) = player_roster
        .players
        .iter()
        .find(|player| player.state.player_id == current_player) else {
        return None;
    };

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

        let Some(target_progress) = compute_target_distance(piece.distance, roll.0) else {
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
            let Some(target_progress) = compute_target_distance(piece.distance, roll.0) else {
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
) -> u8 {
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
        return target_progress;
    }

    0
}

fn apply_tile_effects(
    action: &PlannedAction,
    mut final_progress: u8,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
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
        TileKind::Defense => {
            if let Some(shield) = modify_piece_shield(action, piece_query, 1) {
                notes.push(format!("gained shield ({shield})"));
            }
        }
        TileKind::Attack | TileKind::Event | TileKind::Goal | TileKind::Normal => {}
    }

    final_progress
}

fn resolve_collision(
    action: &PlannedAction,
    _target_progress: u8,
    player_roster: &PlayerRoster,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
    notes: &mut Vec<String>,
) {
    let Some(attacker_board_position) = attacker_position(action, player_roster, piece_query) else {
        return;
    };
    let BoardPosition::Main(target_tile_index) = attacker_board_position else {
        return;
    };

    let attacker_piece_id = match *action {
        PlannedAction::Launch {
            piece_id,
            ..
        } => piece_id,
        PlannedAction::Move {
            piece_id,
            ..
        } => piece_id,
    };

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
            notes.push(format!("piece #{} blocked collision with shield", piece_id.0));
            continue;
        }

        piece_state.status = PieceStatus::InHangar;
        piece_state.progress = 0;
        piece_state.shield = 0;
        transform.translation.x = hangar_slot.0.x;
        transform.translation.y = hangar_slot.0.y;
        notes.push(format!("sent piece #{} back to hangar", piece_id.0));
    }
}

fn update_piece_progress(
    action: &PlannedAction,
    target_progress: u8,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) {
    let target_piece_id = match *action {
        PlannedAction::Launch { piece_id, .. } | PlannedAction::Move { piece_id, .. } => piece_id,
    };

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
    let target_piece_id = match *action {
        PlannedAction::Launch { piece_id, .. } | PlannedAction::Move { piece_id, .. } => piece_id,
    };

    for (piece_id, _, mut piece_state, _) in piece_query.iter_mut() {
        if piece_id.0 != target_piece_id {
            continue;
        }

        piece_state.shield = piece_state.shield.saturating_add(delta);
        return Some(piece_state.shield);
    }

    None
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BoardPosition {
    Main(u8),
    Home(u8),
    Goal,
}

fn compute_target_distance(current_distance: u8, roll_value: u8) -> Option<u8> {
    current_distance
        .checked_add(roll_value)
        .filter(|next_distance| *next_distance <= FINISH_DISTANCE)
}

fn board_position_for_distance(
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
    let attacker_piece_id = match *action {
        PlannedAction::Launch { piece_id, .. } | PlannedAction::Move { piece_id, .. } => piece_id,
    };

    for (piece_id, _, piece_state, _) in piece_query.iter_mut() {
        if piece_id.0 == attacker_piece_id {
            return piece_board_position(*piece_state, player_roster);
        }
    }

    None
}

fn world_position_for_piece(
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

fn advance_turn(turn_state: &mut TurnState, player_count: u8) {
    if turn_state.extra_rolls_remaining > 0 {
        turn_state.extra_rolls_remaining -= 1;
    } else {
        turn_state.current_player = if turn_state.current_player >= player_count {
            1
        } else {
            turn_state.current_player + 1
        };
        turn_state.turn_index = turn_state.turn_index.saturating_add(1);
    }

    turn_state.current_roll = None;
}
