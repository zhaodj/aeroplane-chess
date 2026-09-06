use bevy::prelude::*;

use crate::data::game_mode::GameMode;
use crate::domain::piece::{PieceState, PieceStatus, SWAP_MOTION_SERIAL_DELTA};
use crate::gameplay::match_flow::{BoardLayout, PlayerRoster};
use crate::gameplay::skill_flow::{
    is_current_player_swap_piece, is_legal_swap_target, is_swap_main_route_piece,
};
use crate::gameplay::turn_flow::{
    HOME_ENTRY_PROGRESS, MIN_ROUTE_PROGRESS, board_position_for_distance, world_position_for_piece,
};
use crate::plugins::animation_plugin::{MovingPieceQuery, swap_pair_is_moving};
use crate::plugins::piece_plugin::{HangarSlot, PieceId};

pub(crate) const SWAP_MOTION_PENDING: &str = "Swap waiting: aircraft still moving";

/// 换的是公共棋盘位置，不是两名玩家各自起点下的进度数值。
pub(crate) fn swapped_piece_states(
    first: PieceState,
    second: PieceState,
    roster: &PlayerRoster,
) -> Option<(PieceState, PieceState)> {
    if !is_swap_main_route_piece(&first) || !is_swap_main_route_piece(&second) {
        return None;
    }
    let rebase = |owner: u8, source: PieceState| {
        let source_player = roster
            .players
            .iter()
            .find(|p| p.state.player_id == source.owner_player_id)?;
        let target_player = roster.players.iter().find(|p| p.state.player_id == owner)?;
        let position = board_position_for_distance(source_player, source.progress, source.status)?;
        (MIN_ROUTE_PROGRESS..=HOME_ENTRY_PROGRESS).find(|&progress| {
            board_position_for_distance(target_player, progress, PieceStatus::Active)
                == Some(position)
        })
    };
    let swapped_first = PieceState {
        progress: rebase(first.owner_player_id, second)?,
        shield: second.shield,
        stack_shield: second.stack_shield,
        motion_serial: first.motion_serial.wrapping_add(SWAP_MOTION_SERIAL_DELTA),
        ..first
    };
    let swapped_second = PieceState {
        progress: rebase(second.owner_player_id, first)?,
        shield: first.shield,
        stack_shield: first.stack_shield,
        motion_serial: second.motion_serial.wrapping_add(SWAP_MOTION_SERIAL_DELTA),
        ..second
    };
    Some((swapped_first, swapped_second))
}

/// 真人和 AI 共用换位入口；坐标从逻辑位置生成，不采样动画中途的 Transform。
pub(crate) fn execute_swap(
    current_player: u8,
    mode: GameMode,
    target_piece_id: u8,
    board: &BoardLayout,
    roster: &PlayerRoster,
    pieces: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
    moving: &MovingPieceQuery,
) -> String {
    let Some(source_id) = pieces
        .iter()
        .filter(|(_, _, piece, _)| is_current_player_swap_piece(current_player, piece))
        .map(|(id, _, _, _)| id.0)
        .min()
    else {
        return "Swap failed: current player's active piece not found".into();
    };
    execute_selected_swap(
        current_player,
        mode,
        source_id,
        target_piece_id,
        board,
        roster,
        pieces,
        moving,
    )
    .unwrap_or_else(str::to_string)
}

/// 显式交换所选的两架飞机。所有验证都在写入之前，失败时不改变棋子。
pub(crate) fn execute_selected_swap(
    current_player: u8,
    mode: GameMode,
    source_id: u8,
    target_piece_id: u8,
    board: &BoardLayout,
    roster: &PlayerRoster,
    pieces: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
    moving: &MovingPieceQuery,
) -> Result<String, &'static str> {
    if swap_pair_is_moving((source_id, target_piece_id), moving) {
        return Err(SWAP_MOTION_PENDING);
    }
    let Some(source) = pieces.iter().find_map(|(id, _, piece, _)| {
        (id.0 == source_id && is_current_player_swap_piece(current_player, piece)).then_some(*piece)
    }) else {
        return Err("Swap failed: current player's active piece not found");
    };
    let Some(target) = pieces.iter().find_map(|(id, _, piece, _)| {
        (id.0 == target_piece_id
            && is_legal_swap_target(current_player, source.team_id, mode, piece))
        .then_some(*piece)
    }) else {
        return Err("Swap failed: target piece not found on main route");
    };
    let Some((new_source, new_target)) = swapped_piece_states(source, target, roster) else {
        return Err("Swap failed: board position could not be resolved");
    };
    let position = |piece: PieceState| {
        world_position_for_piece(
            piece.owner_player_id,
            piece.progress,
            piece.status,
            board,
            roster,
        )
    };
    let (Some(source_position), Some(target_position)) =
        (position(new_source), position(new_target))
    else {
        return Err("Swap failed: board position could not be resolved");
    };
    for (id, _, mut piece, mut transform) in pieces.iter_mut() {
        if id.0 == source_id {
            *piece = new_source;
            transform.translation = source_position.extend(transform.translation.z);
        } else if id.0 == target_piece_id {
            *piece = new_target;
            transform.translation = target_position.extend(transform.translation.z);
        }
    }
    Ok(format!(
        "Swap exchanged piece #{source_id} with piece #{target_piece_id}"
    ))
}

/// 换位的只读选择会话；单一候选只跳过选择，不跳过确认。
#[derive(Clone, Debug)]
pub(crate) struct SwapSelection {
    pub player_id: u8,
    pub turn_index: u32,
    pub sources: Vec<u8>,
    pub targets: Vec<u8>,
    pub source: Option<u8>,
    pub target: Option<u8>,
}

impl SwapSelection {
    pub fn new(player_id: u8, turn_index: u32, sources: Vec<u8>, targets: Vec<u8>) -> Self {
        let mut selection = Self {
            player_id,
            turn_index,
            sources,
            targets,
            source: None,
            target: None,
        };
        if selection.sources.len() == 1 {
            selection.select(selection.sources[0]);
        }
        selection
    }

    pub fn candidates(&self) -> &[u8] {
        if self.source.is_none() {
            &self.sources
        } else {
            &self.targets
        }
    }

    pub fn pair(&self) -> Option<(u8, u8)> {
        Some((self.source?, self.target?))
    }

    pub fn select(&mut self, id: u8) {
        if self.source.is_none() && self.sources.contains(&id) {
            self.source = Some(id);
            if self.targets.len() == 1 {
                self.target = Some(self.targets[0]);
            }
        } else if self.source.is_some() && self.targets.contains(&id) {
            self.target = Some(id);
        }
    }

    pub fn reselect(&mut self) {
        self.source = None;
        self.target = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::piece::PieceProgress;
    use crate::domain::player::PlayerControl;
    use crate::gameplay::match_flow::{MatchSetup, PlayerSeat, build_match_resources};
    use crate::gameplay::turn_flow::{
        compute_move_target_distance_on_board, movement_steps_between_progresses,
    };

    fn roster(seats: [PlayerSeat; 4]) -> PlayerRoster {
        build_match_resources(&MatchSetup {
            mode: GameMode::TwoVsTwo,
            rule_set: crate::data::rule_set::RuleSet::Creative,
            ai_difficulty: crate::gameplay::ai::AiDifficulty::Normal,
            fast_mode: false,
            launch_rule: crate::domain::rules::LaunchRule::SixOnly,
            player_seats: seats,
            pieces_per_player: 2,
            player_controls: [PlayerControl::Human; 4],
        })
        .1
    }

    fn piece(owner: u8, progress: PieceProgress) -> PieceState {
        PieceState {
            owner_player_id: owner,
            team_id: (owner - 1) % 2 + 1,
            status: PieceStatus::Active,
            progress,
            shield: owner % 2,
            stack_shield: (owner + 1) % 2,
            motion_serial: u32::MAX - 1,
        }
    }

    #[test]
    fn swap_preserves_public_positions_for_every_seat_order_and_public_node() {
        let board = BoardLayout::default();
        for a in PlayerSeat::ALL {
            for b in PlayerSeat::ALL.into_iter().filter(|seat| *seat != a) {
                for c in PlayerSeat::ALL
                    .into_iter()
                    .filter(|seat| *seat != a && *seat != b)
                {
                    let d = PlayerSeat::ALL
                        .into_iter()
                        .find(|seat| ![a, b, c].contains(seat))
                        .unwrap();
                    let roster = roster([a, b, c, d]);
                    let position = |p: PieceState| {
                        world_position_for_piece(
                            p.owner_player_id,
                            p.progress,
                            p.status,
                            &board,
                            &roster,
                        )
                        .unwrap()
                    };
                    for first_owner in 1..=4 {
                        for second_owner in (1..=4).filter(|owner| *owner != first_owner) {
                            for progress in MIN_ROUTE_PROGRESS..=HOME_ENTRY_PROGRESS {
                                let first = piece(first_owner, progress);
                                let second = piece(
                                    second_owner,
                                    HOME_ENTRY_PROGRESS + MIN_ROUTE_PROGRESS - progress,
                                );
                                let (new_first, new_second) =
                                    swapped_piece_states(first, second, &roster).unwrap();
                                assert_eq!(position(new_first), position(second));
                                assert_eq!(position(new_second), position(first));
                                assert_eq!(
                                    (new_first.owner_player_id, new_first.team_id),
                                    (first.owner_player_id, first.team_id)
                                );
                                assert_eq!(
                                    (new_second.owner_player_id, new_second.team_id),
                                    (second.owner_player_id, second.team_id)
                                );
                                assert_eq!(
                                    (new_first.shield, new_first.stack_shield),
                                    (second.shield, second.stack_shield)
                                );
                                assert_eq!(new_first.motion_serial, 0);
                                let (back_first, back_second) =
                                    swapped_piece_states(new_first, new_second, &roster).unwrap();
                                assert_eq!(back_first.progress, first.progress);
                                assert_eq!(back_second.progress, second.progress);
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn swap_before_own_launch_remains_on_public_track_and_moves_across_zero() {
        let roster = roster(PlayerSeat::ALL);
        let board = BoardLayout::default();
        for owner in 1..=4 {
            let recipient = piece(owner, 10);
            for progress in [-2, -1] {
                let expected =
                    world_position_for_piece(owner, progress, PieceStatus::Active, &board, &roster)
                        .unwrap();
                let other_owner = owner % 4 + 1;
                let other_progress = (MIN_ROUTE_PROGRESS..=HOME_ENTRY_PROGRESS)
                    .find(|p| {
                        world_position_for_piece(
                            other_owner,
                            *p,
                            PieceStatus::Active,
                            &board,
                            &roster,
                        ) == Some(expected)
                    })
                    .unwrap();
                let (swapped, _) =
                    swapped_piece_states(recipient, piece(other_owner, other_progress), &roster)
                        .unwrap();
                assert_eq!(swapped.progress, progress);
                assert!(is_swap_main_route_piece(&swapped));
                let target = compute_move_target_distance_on_board(
                    owner,
                    swapped.status,
                    swapped.progress,
                    1,
                    &board,
                    &roster,
                )
                .unwrap();
                assert_eq!(target, progress + 1);
                let steps = movement_steps_between_progresses(
                    owner,
                    swapped.status,
                    progress,
                    swapped.status,
                    target,
                    &board,
                    &roster,
                )
                .unwrap();
                assert_eq!(steps.len(), 1);
                assert_eq!(steps[0].progress, target);
            }
        }
    }

    #[test]
    fn swap_rejects_non_public_states_and_unknown_players_atomically() {
        let roster = roster(PlayerSeat::ALL);
        for invalid in [
            PieceState {
                status: PieceStatus::InHangar,
                ..piece(2, 0)
            },
            piece(2, HOME_ENTRY_PROGRESS + 1),
            piece(2, MIN_ROUTE_PROGRESS - 1),
            piece(9, 4),
        ] {
            assert!(swapped_piece_states(piece(1, 3), invalid, &roster).is_none());
        }
    }
}
