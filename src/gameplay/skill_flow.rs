use bevy::prelude::*;
use rand::random_range;

use crate::gameplay::match_flow::PlayerRoster;

#[derive(Clone, Debug, Default, Resource)]
pub struct SkillRoster {
    pub players: Vec<PlayerSkillState>,
    pub last_skill_action: Option<String>,
}

#[derive(Clone, Debug)]
pub struct PlayerSkillState {
    pub player_id: u8,
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
                shield_charges: 1,
                double_dice_charges: 1,
                double_dice_armed: false,
            })
            .collect(),
        last_skill_action: None,
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
    use crate::domain::player::{PlayerControl, PlayerState};
    use crate::gameplay::match_flow::{PlayerProfile, PlayerRoster};

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
        assert_eq!(skill_roster.players[0].shield_charges, 1);
        assert_eq!(skill_roster.players[0].double_dice_charges, 1);
        assert!(!skill_roster.players[0].double_dice_armed);
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
}
