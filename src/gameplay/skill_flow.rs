use bevy::prelude::*;
use rand::random_range;

use crate::domain::piece::{PieceState, PieceStatus};
use crate::domain::player::PlayerControl;
use crate::gameplay::match_flow::PlayerRoster;
use crate::plugins::piece_plugin::PieceId;

#[derive(Clone, Debug, Default, Resource)]
pub struct SkillRoster {
    pub players: Vec<PlayerSkillState>,
    pub last_skill_action: Option<String>,
    pub active_turn_player: Option<u8>,
    pub skill_used_this_turn: bool,
}

#[derive(Clone, Debug)]
pub struct PlayerSkillState {
    pub player_id: u8,
    pub dash_charges: u8,
    pub dash_armed: bool,
    pub snipe_charges: u8,
    pub shield_charges: u8,
    pub double_dice_charges: u8,
    pub double_dice_armed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RollResolution {
    pub value: u8,
    pub used_double_dice: bool,
}

pub fn build_skill_roster(player_roster: &PlayerRoster) -> SkillRoster {
    SkillRoster {
        players: player_roster
            .players
            .iter()
            .map(|player| PlayerSkillState {
                player_id: player.state.player_id,
                dash_charges: 1,
                dash_armed: false,
                snipe_charges: 1,
                shield_charges: 1,
                double_dice_charges: 1,
                double_dice_armed: false,
            })
            .collect(),
        last_skill_action: None,
        active_turn_player: None,
        skill_used_this_turn: false,
    }
}

pub fn player_skill_state(
    skill_roster: &SkillRoster,
    player_id: u8,
) -> Option<&PlayerSkillState> {
    skill_roster
        .players
        .iter()
        .find(|player| player.player_id == player_id)
}

pub fn sync_turn_skill_usage(skill_roster: &mut SkillRoster, current_player: u8) {
    if skill_roster.active_turn_player != Some(current_player) {
        skill_roster.active_turn_player = Some(current_player);
        skill_roster.skill_used_this_turn = false;
    }
}

pub fn can_use_skill_this_turn(skill_roster: &SkillRoster, current_player: u8) -> bool {
    skill_roster.active_turn_player == Some(current_player) && !skill_roster.skill_used_this_turn
}

pub fn mark_skill_used(skill_roster: &mut SkillRoster, current_player: u8) {
    skill_roster.active_turn_player = Some(current_player);
    skill_roster.skill_used_this_turn = true;
}

pub fn arm_dash(skill_roster: &mut SkillRoster, player_id: u8) -> bool {
    let Some(player_state) = skill_roster
        .players
        .iter_mut()
        .find(|player| player.player_id == player_id)
    else {
        return false;
    };

    if player_state.dash_charges == 0 || player_state.dash_armed {
        return false;
    }

    player_state.dash_charges -= 1;
    player_state.dash_armed = true;
    true
}

pub fn dash_bonus(skill_roster: &SkillRoster, player_id: u8) -> u8 {
    player_skill_state(skill_roster, player_id)
        .map(|player| if player.dash_armed { 3 } else { 0 })
        .unwrap_or(0)
}

pub fn clear_dash_arm(skill_roster: &mut SkillRoster, player_id: u8) {
    if let Some(player_state) = skill_roster
        .players
        .iter_mut()
        .find(|player| player.player_id == player_id)
    {
        player_state.dash_armed = false;
    }
}

pub fn spend_snipe_charge(skill_roster: &mut SkillRoster, player_id: u8) -> bool {
    let Some(player_state) = skill_roster
        .players
        .iter_mut()
        .find(|player| player.player_id == player_id)
    else {
        return false;
    };

    if player_state.snipe_charges == 0 {
        return false;
    }

    player_state.snipe_charges -= 1;
    true
}

pub fn collect_snipe_targets(
    current_player: u8,
    current_team: u8,
    piece_query: &Query<(&PieceId, &mut PieceState)>,
) -> Vec<u8> {
    let mut unshielded = Vec::new();
    let mut shielded = Vec::new();

    for (piece_id, piece_state) in piece_query.iter() {
        if piece_state.owner_player_id == current_player
            || piece_state.team_id == current_team
            || piece_state.status != PieceStatus::Active
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

pub fn preferred_shield_target(
    player_id: u8,
    piece_query: &Query<(&PieceId, &mut PieceState)>,
) -> Option<u8> {
    piece_query
        .iter()
        .filter(|(_, piece_state)| {
            piece_state.owner_player_id == player_id
                && piece_state.status == PieceStatus::Active
                && piece_state.shield == 0
        })
        .map(|(piece_id, _)| piece_id.0)
        .min()
        .or_else(|| {
            piece_query
                .iter()
                .filter(|(_, piece_state)| {
                    piece_state.owner_player_id == player_id
                        && piece_state.status == PieceStatus::Active
                })
                .map(|(piece_id, _)| piece_id.0)
                .min()
        })
        .or_else(|| {
            piece_query
                .iter()
                .filter(|(_, piece_state)| piece_state.owner_player_id == player_id)
                .map(|(piece_id, _)| piece_id.0)
                .min()
        })
}

pub fn should_ai_use_shield(
    player_id: u8,
    skill_roster: &SkillRoster,
    piece_query: &Query<(&PieceId, &mut PieceState)>,
) -> bool {
    let Some(skill_state) = player_skill_state(skill_roster, player_id) else {
        return false;
    };
    if skill_state.shield_charges == 0 {
        return false;
    }

    piece_query.iter().any(|(_, piece_state)| {
        piece_state.owner_player_id == player_id
            && piece_state.status == PieceStatus::Active
            && piece_state.shield == 0
    })
}

pub fn should_ai_arm_double_dice(
    player_id: u8,
    skill_roster: &SkillRoster,
    piece_query: &Query<(&PieceId, &mut PieceState)>,
) -> bool {
    let Some(skill_state) = player_skill_state(skill_roster, player_id) else {
        return false;
    };
    if skill_state.double_dice_charges == 0 || skill_state.double_dice_armed {
        return false;
    }

    let has_active_piece = piece_query.iter().any(|(_, piece_state)| {
        piece_state.owner_player_id == player_id && piece_state.status == PieceStatus::Active
    });
    let has_hangar_piece = piece_query.iter().any(|(_, piece_state)| {
        piece_state.owner_player_id == player_id && piece_state.status == PieceStatus::InHangar
    });

    !has_active_piece && has_hangar_piece
}

pub fn apply_shield_to_piece(
    piece_id: u8,
    piece_query: &mut Query<(&PieceId, &mut PieceState)>,
) -> Option<u8> {
    for (query_piece_id, mut piece_state) in piece_query.iter_mut() {
        if query_piece_id.0 != piece_id {
            continue;
        }

        piece_state.shield = piece_state.shield.saturating_add(1);
        return Some(piece_state.shield);
    }

    None
}

pub fn current_player_type(player_roster: &PlayerRoster, player_id: u8) -> Option<PlayerControl> {
    player_roster
        .players
        .iter()
        .find(|player| player.state.player_id == player_id)
        .map(|player| player.state.control)
}

pub fn spend_shield_charge(skill_roster: &mut SkillRoster, player_id: u8) -> bool {
    let Some(player_state) = skill_roster
        .players
        .iter_mut()
        .find(|player| player.player_id == player_id)
    else {
        return false;
    };

    if player_state.shield_charges == 0 {
        return false;
    }

    player_state.shield_charges -= 1;
    true
}

pub fn arm_double_dice(skill_roster: &mut SkillRoster, player_id: u8) -> bool {
    let Some(player_state) = skill_roster
        .players
        .iter_mut()
        .find(|player| player.player_id == player_id)
    else {
        return false;
    };

    if player_state.double_dice_charges == 0 || player_state.double_dice_armed {
        return false;
    }

    player_state.double_dice_charges -= 1;
    player_state.double_dice_armed = true;
    true
}

pub fn resolve_roll_value(skill_roster: &mut SkillRoster, player_id: u8) -> RollResolution {
    let Some(player_state) = skill_roster
        .players
        .iter_mut()
        .find(|player| player.player_id == player_id)
    else {
        return RollResolution {
            value: random_range(1..=6),
            used_double_dice: false,
        };
    };

    if player_state.double_dice_armed {
        player_state.double_dice_armed = false;
        return resolve_roll_from_values(random_range(1..=6), random_range(1..=6), true);
    }

    resolve_roll_from_values(random_range(1..=6), 0, false)
}

pub fn resolve_roll_from_values(first: u8, second: u8, use_double_dice: bool) -> RollResolution {
    if use_double_dice {
        RollResolution {
            value: first.max(second),
            used_double_dice: true,
        }
    } else {
        RollResolution {
            value: first,
            used_double_dice: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::player::PlayerState;
    use crate::gameplay::match_flow::{PlayerProfile, PlayerRoster};
    use bevy::ecs::system::SystemState;

    fn sample_roster() -> PlayerRoster {
        PlayerRoster {
            players: vec![
                PlayerProfile {
                    state: PlayerState {
                        player_id: 1,
                        team_id: 1,
                        control: PlayerControl::Human,
                    },
                    color: Color::WHITE,
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
                    color: Color::BLACK,
                    hangar_slots: vec![],
                    launch_tile_index: 0,
                    home_lane_positions: vec![],
                    goal_position: Vec2::ZERO,
                },
            ],
        }
    }

    #[test]
    fn build_skill_roster_initializes_default_charges() {
        let skill_roster = build_skill_roster(&sample_roster());

        assert_eq!(skill_roster.players.len(), 2);
        assert_eq!(skill_roster.players[0].dash_charges, 1);
        assert_eq!(skill_roster.players[0].snipe_charges, 1);
        assert_eq!(skill_roster.players[0].shield_charges, 1);
        assert_eq!(skill_roster.players[0].double_dice_charges, 1);
        assert!(!skill_roster.players[0].dash_armed);
        assert!(!skill_roster.players[0].double_dice_armed);
    }

    #[test]
    fn arm_dash_consumes_charge_and_sets_flag() {
        let mut skill_roster = build_skill_roster(&sample_roster());

        assert!(arm_dash(&mut skill_roster, 1));
        assert_eq!(dash_bonus(&skill_roster, 1), 3);
        assert_eq!(player_skill_state(&skill_roster, 1).unwrap().dash_charges, 0);

        clear_dash_arm(&mut skill_roster, 1);
        assert_eq!(dash_bonus(&skill_roster, 1), 0);
    }

    #[test]
    fn turn_skill_usage_resets_when_player_changes() {
        let mut skill_roster = build_skill_roster(&sample_roster());

        sync_turn_skill_usage(&mut skill_roster, 1);
        assert!(can_use_skill_this_turn(&skill_roster, 1));
        mark_skill_used(&mut skill_roster, 1);
        assert!(!can_use_skill_this_turn(&skill_roster, 1));

        sync_turn_skill_usage(&mut skill_roster, 2);
        assert!(can_use_skill_this_turn(&skill_roster, 2));
    }

    #[test]
    fn spend_snipe_charge_only_succeeds_when_charge_exists() {
        let mut skill_roster = build_skill_roster(&sample_roster());

        assert!(spend_snipe_charge(&mut skill_roster, 1));
        assert!(!spend_snipe_charge(&mut skill_roster, 1));
    }

    #[test]
    fn spend_shield_charge_only_succeeds_when_charge_exists() {
        let mut skill_roster = build_skill_roster(&sample_roster());

        assert!(spend_shield_charge(&mut skill_roster, 1));
        assert!(!spend_shield_charge(&mut skill_roster, 1));
    }

    #[test]
    fn arm_double_dice_consumes_charge_and_sets_flag() {
        let mut skill_roster = build_skill_roster(&sample_roster());

        assert!(arm_double_dice(&mut skill_roster, 1));
        assert!(player_skill_state(&skill_roster, 1).unwrap().double_dice_armed);
        assert_eq!(player_skill_state(&skill_roster, 1).unwrap().double_dice_charges, 0);
        assert!(!arm_double_dice(&mut skill_roster, 1));
    }

    #[test]
    fn resolve_roll_from_values_uses_higher_die_for_double_dice() {
        assert_eq!(
            resolve_roll_from_values(2, 5, true),
            RollResolution {
                value: 5,
                used_double_dice: true,
            }
        );
        assert_eq!(
            resolve_roll_from_values(4, 0, false),
            RollResolution {
                value: 4,
                used_double_dice: false,
            }
        );
    }

    #[test]
    fn ai_arms_double_dice_when_all_pieces_are_in_hangar() {
        let mut world = World::new();
        world.spawn((
            PieceId(1),
            PieceState {
                owner_player_id: 2,
                team_id: 2,
                status: PieceStatus::InHangar,
                progress: 0,
                shield: 0,
                stack_shield: 0,
            },
        ));

        let mut system_state: SystemState<Query<(&PieceId, &mut PieceState)>> =
            SystemState::new(&mut world);
        let query = system_state.get(&world);
        let skill_roster = build_skill_roster(&sample_roster());

        assert!(should_ai_arm_double_dice(2, &skill_roster, &query));
    }

    #[test]
    fn ai_uses_shield_when_active_piece_has_no_shield() {
        let mut world = World::new();
        world.spawn((
            PieceId(1),
            PieceState {
                owner_player_id: 2,
                team_id: 2,
                status: PieceStatus::Active,
                progress: 4,
                shield: 0,
                stack_shield: 0,
            },
        ));

        let mut system_state: SystemState<Query<(&PieceId, &mut PieceState)>> =
            SystemState::new(&mut world);
        let query = system_state.get(&world);
        let skill_roster = build_skill_roster(&sample_roster());

        assert!(should_ai_use_shield(2, &skill_roster, &query));
        assert_eq!(preferred_shield_target(2, &query), Some(1));
    }

    #[test]
    fn collect_snipe_targets_prioritizes_unshielded_enemies() {
        let mut world = World::new();
        world.spawn((
            PieceId(1),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: PieceStatus::Active,
                progress: 0,
                shield: 0,
                stack_shield: 0,
            },
        ));
        world.spawn((
            PieceId(2),
            PieceState {
                owner_player_id: 3,
                team_id: 2,
                status: PieceStatus::Active,
                progress: 0,
                shield: 1,
                stack_shield: 0,
            },
        ));
        world.spawn((
            PieceId(3),
            PieceState {
                owner_player_id: 4,
                team_id: 2,
                status: PieceStatus::Active,
                progress: 0,
                shield: 0,
                stack_shield: 0,
            },
        ));

        let mut system_state: SystemState<Query<(&PieceId, &mut PieceState)>> =
            SystemState::new(&mut world);
        let query = system_state.get(&world);

        assert_eq!(collect_snipe_targets(1, 1, &query), vec![3, 2]);
    }
}
