use bevy::prelude::*;
use rand::random_range;

use crate::domain::dice::DiceRoll;
use crate::domain::piece::{PieceState, PieceStatus};
use crate::domain::rules::can_launch;
use crate::domain::tile::TileKind;
use crate::gameplay::turn_flow::TurnState;
use crate::plugins::game_plugin::{BoardLayout, PlayerRoster};
use crate::plugins::piece_plugin::{HangarSlot, PieceId};
use crate::states::{AppState, GamePhase};

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
    mut piece_query: Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) {
    if !matches!(game_phase.get(), GamePhase::AwaitDice) {
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

    let initial_progress = apply_action(&action, &board_layout, &mut piece_query);
    let mut notes = Vec::new();
    let final_progress =
        apply_tile_effects(&action, initial_progress, &board_layout, &mut piece_query, &mut notes);
    resolve_collision(&action, final_progress, &mut piece_query, &mut notes);
    turn_state.last_action = Some(describe_action(&action, roll_value, &notes));

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
    progress: u8,
}

fn choose_action(
    current_player: u8,
    roll: DiceRoll,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
    piece_query: &Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> Option<PlannedAction> {
    let route_len = board_layout.route_len() as u8;
    if route_len == 0 {
        return None;
    }

    let snapshots = piece_query
        .iter()
        .map(|(piece_id, _, piece_state, _)| PieceSnapshot {
            piece_id: piece_id.0,
            owner_player_id: piece_state.owner_player_id,
            team_id: piece_state.team_id,
            status: piece_state.status,
            progress: piece_state.progress,
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
                player_profile.launch_tile_index,
            ) {
                return Some(PlannedAction::Launch {
                    piece_id: piece.piece_id,
                    target_progress: player_profile.launch_tile_index,
                });
            }

            if can_launch(
                &PieceState {
                    owner_player_id: piece.owner_player_id,
                    team_id: piece.team_id,
                    status: piece.status,
                    progress: piece.progress,
                    shield: 0,
                },
                roll,
            ) {
                return Some(PlannedAction::Launch {
                    piece_id: piece.piece_id,
                    target_progress: player_profile.launch_tile_index,
                });
            }
        }
    }

    for piece in snapshots.iter().filter(|piece| piece.owner_player_id == current_player) {
        if piece.status != PieceStatus::Active {
            continue;
        }

        let target_progress = (piece.progress + roll.0) % route_len;
        if is_enemy_on_progress(
            &snapshots,
            current_player,
            piece.team_id,
            target_progress,
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
                    target_progress: player_profile.launch_tile_index,
                });
            }
        }
    }

    for piece in snapshots.iter().filter(|piece| piece.owner_player_id == current_player) {
        if piece.status == PieceStatus::Active {
            return Some(PlannedAction::Move {
                piece_id: piece.piece_id,
                target_progress: (piece.progress + roll.0) % route_len,
            });
        }
    }

    None
}

fn is_enemy_on_progress(
    snapshots: &[PieceSnapshot],
    current_player: u8,
    current_team: u8,
    target_progress: u8,
) -> bool {
    snapshots.iter().any(|piece| {
        piece.owner_player_id != current_player
            && piece.team_id != current_team
            && piece.status == PieceStatus::Active
            && piece.progress == target_progress
    })
}

fn apply_action(
    action: &PlannedAction,
    board_layout: &BoardLayout,
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
            } => (piece_id, target_progress, PieceStatus::Active),
        };

        if piece_id.0 != target_piece_id {
            continue;
        }

        piece_state.status = next_status;
        piece_state.progress = target_progress;
        if let Some(world_pos) = board_layout.world_pos_for_route_index(target_progress) {
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
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
    notes: &mut Vec<String>,
) -> u8 {
    let route_len = board_layout.route_len() as u8;
    if route_len == 0 {
        return final_progress;
    }

    let Some(tile_kind) = board_layout.tile_kind_for_route_index(final_progress) else {
        return final_progress;
    };

    match tile_kind {
        TileKind::Jump => {
            final_progress = (final_progress + 4) % route_len;
            update_piece_progress(action, final_progress, board_layout, piece_query);
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
    target_progress: u8,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
    notes: &mut Vec<String>,
) {
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
            || piece_state.progress != target_progress
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
        transform.translation.x = hangar_slot.0.x;
        transform.translation.y = hangar_slot.0.y;
        notes.push(format!("sent piece #{} back to hangar", piece_id.0));
    }
}

fn update_piece_progress(
    action: &PlannedAction,
    target_progress: u8,
    board_layout: &BoardLayout,
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
        if let Some(world_pos) = board_layout.world_pos_for_route_index(target_progress) {
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
