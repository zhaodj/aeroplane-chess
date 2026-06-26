use bevy::prelude::*;
use rand::random_range;

use crate::domain::piece::{PieceState, PieceStatus};
use crate::domain::player::PlayerControl;
use crate::gameplay::match_flow::PlayerRoster;
use crate::gameplay::turn_flow::HOME_ENTRY_PROGRESS;
use crate::plugins::piece_plugin::PieceId;

pub const MAX_PIECE_SHIELD: u8 = 2;
pub const STARTING_SKILL_CHARGE_POINTS: u8 = 1;

#[derive(Clone, Debug, Default, Resource)]
/// 全体玩家技能资源与本回合技能使用状态。
pub struct SkillRoster {
    pub players: Vec<PlayerSkillState>,
    pub last_skill_action: Option<String>,
    pub last_skill_action_player_id: Option<u8>,
    pub last_skill_action_turn_index: u32,
    pub last_skill_action_serial: u64,
    pub active_turn_player: Option<u8>,
    pub skill_used_this_turn: bool,
}

#[derive(Clone, Debug)]
/// 单个玩家的技能充能与技能标志位。
pub struct PlayerSkillState {
    pub player_id: u8,
    pub dash_charges: u8,
    pub dash_armed: bool,
    pub snipe_charges: u8,
    pub swap_charges: u8,
    pub shield_charges: u8,
    pub double_dice_charges: u8,
    pub double_dice_armed: bool,
    pub skip_next_skill_turn: bool,
    pub skill_blocked_this_turn: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// 掷骰解析结果（是否消耗了 DoubleDice）。
pub struct RollResolution {
    pub value: u8,
    pub dice: [u8; 2],
    pub used_double_dice: bool,
}

/// 根据玩家名单初始化技能资源（每名玩家开局总计 1 点基础充能）。
pub fn build_skill_roster(player_roster: &PlayerRoster) -> SkillRoster {
    SkillRoster {
        players: player_roster
            .players
            .iter()
            .map(|player| PlayerSkillState {
                player_id: player.state.player_id,
                dash_charges: STARTING_SKILL_CHARGE_POINTS,
                dash_armed: false,
                snipe_charges: 0,
                swap_charges: 0,
                shield_charges: 0,
                double_dice_charges: 0,
                double_dice_armed: false,
                skip_next_skill_turn: false,
                skill_blocked_this_turn: false,
            })
            .collect(),
        last_skill_action: None,
        last_skill_action_player_id: None,
        last_skill_action_turn_index: 0,
        last_skill_action_serial: 0,
        active_turn_player: None,
        skill_used_this_turn: false,
    }
}

/// 记录一次技能日志事件，并分配稳定序号避免 UI 按帧重复消费旧消息。
pub fn record_skill_action(
    skill_roster: &mut SkillRoster,
    turn_index: u32,
    player_id: u8,
    action: impl Into<String>,
) {
    skill_roster.last_skill_action = Some(action.into());
    skill_roster.last_skill_action_player_id = Some(player_id);
    skill_roster.last_skill_action_turn_index = turn_index;
    skill_roster.last_skill_action_serial = skill_roster.last_skill_action_serial.saturating_add(1);
}

/// 读取指定玩家的技能状态快照。
pub fn player_skill_state(skill_roster: &SkillRoster, player_id: u8) -> Option<&PlayerSkillState> {
    skill_roster
        .players
        .iter()
        .find(|player| player.player_id == player_id)
}

/// 每次回合切换时同步“本回合技能是否已使用”与“跳过技能回合”标志。
pub fn sync_turn_skill_usage(skill_roster: &mut SkillRoster, current_player: u8) {
    if skill_roster.active_turn_player != Some(current_player) {
        skill_roster.active_turn_player = Some(current_player);
        skill_roster.skill_used_this_turn = false;
        if let Some(player_state) = skill_roster
            .players
            .iter_mut()
            .find(|player| player.player_id == current_player)
        {
            if player_state.skip_next_skill_turn {
                player_state.skip_next_skill_turn = false;
                player_state.skill_blocked_this_turn = true;
            } else {
                player_state.skill_blocked_this_turn = false;
            }
        }
    }
}

/// 判断当前玩家本回合是否仍可使用技能。
pub fn can_use_skill_this_turn(skill_roster: &SkillRoster, current_player: u8) -> bool {
    skill_roster.active_turn_player == Some(current_player)
        && !skill_roster.skill_used_this_turn
        && !player_skill_state(skill_roster, current_player)
            .map(|state| state.skill_blocked_this_turn)
            .unwrap_or(false)
}

/// 标记当前玩家已在本回合消耗技能次数。
pub fn mark_skill_used(skill_roster: &mut SkillRoster, current_player: u8) {
    skill_roster.active_turn_player = Some(current_player);
    skill_roster.skill_used_this_turn = true;
}

/// 消耗 1 次 Dash 充能并进入预备态。
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

/// 读取 Dash 的额外步数加成（仅预备态返回 +3）。
pub fn dash_bonus(skill_roster: &SkillRoster, player_id: u8) -> u8 {
    player_skill_state(skill_roster, player_id)
        .map(|player| if player.dash_armed { 3 } else { 0 })
        .unwrap_or(0)
}

/// 清除 Dash 预备态（一般在动作执行后调用）。
pub fn clear_dash_arm(skill_roster: &mut SkillRoster, player_id: u8) {
    if let Some(player_state) = skill_roster
        .players
        .iter_mut()
        .find(|player| player.player_id == player_id)
    {
        player_state.dash_armed = false;
    }
}

/// 消耗 1 次 Snipe 充能。
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

/// 消耗 1 次 Swap 充能。
pub fn spend_swap_charge(skill_roster: &mut SkillRoster, player_id: u8) -> bool {
    let Some(player_state) = skill_roster
        .players
        .iter_mut()
        .find(|player| player.player_id == player_id)
    else {
        return false;
    };

    if player_state.swap_charges == 0 {
        return false;
    }

    player_state.swap_charges -= 1;
    true
}

/// 收集可被 Snipe 的目标：优先无护盾敌人，再是有护盾敌人。
pub fn collect_snipe_targets(
    current_player: u8,
    current_team: u8,
    piece_query: &Query<(&PieceId, &mut PieceState)>,
) -> Vec<u8> {
    let mut unshielded = Vec::new();
    let mut shielded = Vec::new();

    for (piece_id, piece_state) in piece_query.iter() {
        if !is_legal_snipe_target(current_player, current_team, piece_state) {
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

/// 选择 Shield 的默认目标：优先无盾的己方 Active 棋子。
pub fn preferred_shield_target(
    player_id: u8,
    piece_query: &Query<(&PieceId, &mut PieceState)>,
) -> Option<u8> {
    piece_query
        .iter()
        .filter(|(_, piece_state)| is_legal_shield_target(player_id, piece_state))
        .map(|(piece_id, _)| piece_id.0)
        .min()
}

/// AI 是否应当在当前回合使用 Shield。
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

    piece_query
        .iter()
        .any(|(_, piece_state)| is_legal_shield_target(player_id, piece_state))
}

/// 判断棋子是否可作为 Shield 目标。
pub fn is_legal_shield_target(player_id: u8, piece_state: &PieceState) -> bool {
    piece_state.owner_player_id == player_id
        && piece_state.status == PieceStatus::Active
        && piece_state.shield < MAX_PIECE_SHIELD
}

/// 判断棋子是否可作为 Snipe 目标。
pub fn is_legal_snipe_target(
    current_player: u8,
    current_team: u8,
    piece_state: &PieceState,
) -> bool {
    piece_state.owner_player_id != current_player
        && piece_state.team_id != current_team
        && piece_state.status == PieceStatus::Active
        && piece_state.progress <= HOME_ENTRY_PROGRESS
}

/// 判断棋子是否为当前玩家可操作的 Active 棋子。
pub fn is_current_player_active_piece(current_player: u8, piece_state: &PieceState) -> bool {
    piece_state.owner_player_id == current_player && piece_state.status == PieceStatus::Active
}

/// 判断棋子是否为当前玩家可通过 Dash 增幅的移动棋子。
pub fn is_current_player_dash_move_piece(current_player: u8, piece_state: &PieceState) -> bool {
    piece_state.owner_player_id == current_player
        && matches!(
            piece_state.status,
            PieceStatus::AtLaunch | PieceStatus::Active
        )
}

/// 判断棋子是否为当前玩家同队队友的 Active 棋子。
pub fn is_active_teammate_piece(
    current_player: u8,
    current_team: u8,
    piece_state: &PieceState,
) -> bool {
    piece_state.owner_player_id != current_player
        && piece_state.team_id == current_team
        && piece_state.status == PieceStatus::Active
}

/// AI 是否应当预备 DoubleDice（用于开局起飞机会）。
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

/// 为目标棋子增加 1 层护盾（带上限）。
pub fn apply_shield_to_piece(
    piece_id: u8,
    piece_query: &mut Query<(&PieceId, &mut PieceState)>,
) -> Option<u8> {
    for (query_piece_id, mut piece_state) in piece_query.iter_mut() {
        if query_piece_id.0 != piece_id {
            continue;
        }

        piece_state.shield = piece_state.shield.saturating_add(1).min(MAX_PIECE_SHIELD);
        return Some(piece_state.shield);
    }

    None
}

/// 查询玩家的人机控制类型。
pub fn current_player_type(player_roster: &PlayerRoster, player_id: u8) -> Option<PlayerControl> {
    player_roster
        .players
        .iter()
        .find(|player| player.state.player_id == player_id)
        .map(|player| player.state.control)
}

/// 消耗 1 次 Shield 充能。
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

/// 预备 DoubleDice：消耗充能并设置 armed 标记。
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

/// 根据当前技能状态解析最终掷骰值（包含 DoubleDice 逻辑）。
pub fn resolve_roll_value(skill_roster: &mut SkillRoster, player_id: u8) -> RollResolution {
    let Some(player_state) = skill_roster
        .players
        .iter_mut()
        .find(|player| player.player_id == player_id)
    else {
        let value = random_range(1..=6);
        return RollResolution {
            value,
            dice: [value, 0],
            used_double_dice: false,
        };
    };

    if player_state.double_dice_armed {
        player_state.double_dice_armed = false;
        return resolve_roll_from_values(random_range(1..=6), random_range(1..=6), true);
    }

    resolve_roll_from_values(random_range(1..=6), 0, false)
}

/// 在给定骰面下计算最终点数（测试与回放友好）。
pub fn resolve_roll_from_values(first: u8, second: u8, use_double_dice: bool) -> RollResolution {
    if use_double_dice {
        RollResolution {
            value: first.max(second),
            dice: [first, second],
            used_double_dice: true,
        }
    } else {
        RollResolution {
            value: first,
            dice: [first, 0],
            used_double_dice: false,
        }
    }
}

/// 随机给玩家补充 1 次技能充能；1v1 可禁用 Swap 池。
pub fn grant_random_skill_charge(
    skill_roster: &mut SkillRoster,
    player_id: u8,
    allow_swap: bool,
) -> Option<&'static str> {
    let player_state = skill_roster
        .players
        .iter_mut()
        .find(|player| player.player_id == player_id)?;

    if allow_swap {
        match random_range(0..=4) {
            0 => {
                player_state.dash_charges = player_state.dash_charges.saturating_add(1);
                Some("Dash")
            }
            1 => {
                player_state.snipe_charges = player_state.snipe_charges.saturating_add(1);
                Some("Snipe")
            }
            2 => {
                player_state.swap_charges = player_state.swap_charges.saturating_add(1);
                Some("Swap")
            }
            3 => {
                player_state.shield_charges = player_state.shield_charges.saturating_add(1);
                Some("Shield")
            }
            _ => {
                player_state.double_dice_charges =
                    player_state.double_dice_charges.saturating_add(1);
                Some("DoubleDice")
            }
        }
    } else {
        match random_range(0..=3) {
            0 => {
                player_state.dash_charges = player_state.dash_charges.saturating_add(1);
                Some("Dash")
            }
            1 => {
                player_state.snipe_charges = player_state.snipe_charges.saturating_add(1);
                Some("Snipe")
            }
            2 => {
                player_state.shield_charges = player_state.shield_charges.saturating_add(1);
                Some("Shield")
            }
            _ => {
                player_state.double_dice_charges =
                    player_state.double_dice_charges.saturating_add(1);
                Some("DoubleDice")
            }
        }
    }
}

/// 给目标玩家打上“下回合禁止用技能”的标记。
pub fn disable_next_skill_turn(skill_roster: &mut SkillRoster, player_id: u8) -> bool {
    let Some(player_state) = skill_roster
        .players
        .iter_mut()
        .find(|player| player.player_id == player_id)
    else {
        return false;
    };
    player_state.skip_next_skill_turn = true;
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::player::PlayerState;
    use crate::gameplay::match_flow::{PlayerProfile, PlayerRoster, PlayerSeat};
    use bevy::ecs::system::SystemState;

    fn sample_roster() -> PlayerRoster {
        PlayerRoster::from_players(vec![
            PlayerProfile {
                state: PlayerState {
                    player_id: 1,
                    team_id: 1,
                    control: PlayerControl::Human,
                },
                seat: PlayerSeat::Blue,
                color: Color::WHITE,
                hangar_slots: vec![],
                launch_position: Vec2::ZERO,
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
                seat: PlayerSeat::Red,
                color: Color::BLACK,
                hangar_slots: vec![],
                launch_position: Vec2::ZERO,
                launch_tile_index: 0,
                home_lane_positions: vec![],
                goal_position: Vec2::ZERO,
            },
        ])
    }

    fn total_charges(skills: &PlayerSkillState) -> u8 {
        skills.dash_charges
            + skills.snipe_charges
            + skills.swap_charges
            + skills.shield_charges
            + skills.double_dice_charges
    }

    fn set_charges(
        skill_roster: &mut SkillRoster,
        player_id: u8,
        dash: u8,
        snipe: u8,
        swap: u8,
        shield: u8,
        double_dice: u8,
    ) {
        let player = skill_roster
            .players
            .iter_mut()
            .find(|player| player.player_id == player_id)
            .expect("player should exist");
        player.dash_charges = dash;
        player.snipe_charges = snipe;
        player.swap_charges = swap;
        player.shield_charges = shield;
        player.double_dice_charges = double_dice;
    }

    fn piece_state(owner_player_id: u8, status: PieceStatus) -> PieceState {
        PieceState {
            owner_player_id,
            team_id: owner_player_id,
            status,
            progress: 0,
            shield: 0,
            stack_shield: 0,
            motion_serial: 0,
        }
    }

    #[test]
    fn build_skill_roster_initializes_one_starting_charge_point() {
        let skill_roster = build_skill_roster(&sample_roster());

        assert_eq!(skill_roster.players.len(), 2);
        for player in &skill_roster.players {
            assert_eq!(total_charges(player), STARTING_SKILL_CHARGE_POINTS);
            assert_eq!(player.dash_charges, STARTING_SKILL_CHARGE_POINTS);
            assert_eq!(player.snipe_charges, 0);
            assert_eq!(player.swap_charges, 0);
            assert_eq!(player.shield_charges, 0);
            assert_eq!(player.double_dice_charges, 0);
            assert!(!player.dash_armed);
            assert!(!player.double_dice_armed);
        }
    }

    #[test]
    fn arm_dash_consumes_charge_and_sets_flag() {
        let mut skill_roster = build_skill_roster(&sample_roster());

        assert!(arm_dash(&mut skill_roster, 1));
        assert_eq!(dash_bonus(&skill_roster, 1), 3);
        assert_eq!(
            player_skill_state(&skill_roster, 1).unwrap().dash_charges,
            0
        );

        clear_dash_arm(&mut skill_roster, 1);
        assert_eq!(dash_bonus(&skill_roster, 1), 0);
    }

    #[test]
    fn dash_move_piece_includes_launch_and_active_only_for_current_player() {
        assert!(is_current_player_dash_move_piece(
            1,
            &piece_state(1, PieceStatus::AtLaunch)
        ));
        assert!(is_current_player_dash_move_piece(
            1,
            &piece_state(1, PieceStatus::Active)
        ));
        assert!(!is_current_player_dash_move_piece(
            1,
            &piece_state(1, PieceStatus::InHangar)
        ));
        assert!(!is_current_player_dash_move_piece(
            1,
            &piece_state(2, PieceStatus::AtLaunch)
        ));
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
    fn disable_next_skill_turn_blocks_only_the_next_turn() {
        let mut skill_roster = build_skill_roster(&sample_roster());

        assert!(disable_next_skill_turn(&mut skill_roster, 1));
        sync_turn_skill_usage(&mut skill_roster, 1);
        assert!(!can_use_skill_this_turn(&skill_roster, 1));

        sync_turn_skill_usage(&mut skill_roster, 2);
        assert!(can_use_skill_this_turn(&skill_roster, 2));

        sync_turn_skill_usage(&mut skill_roster, 1);
        assert!(can_use_skill_this_turn(&skill_roster, 1));
    }

    #[test]
    fn spend_snipe_charge_only_succeeds_when_charge_exists() {
        let mut skill_roster = build_skill_roster(&sample_roster());
        set_charges(&mut skill_roster, 1, 0, 1, 0, 0, 0);

        assert!(spend_snipe_charge(&mut skill_roster, 1));
        assert!(!spend_snipe_charge(&mut skill_roster, 1));
    }

    #[test]
    fn spend_swap_charge_only_succeeds_when_charge_exists() {
        let mut skill_roster = build_skill_roster(&sample_roster());
        set_charges(&mut skill_roster, 1, 0, 0, 1, 0, 0);

        assert!(spend_swap_charge(&mut skill_roster, 1));
        assert!(!spend_swap_charge(&mut skill_roster, 1));
    }

    #[test]
    fn spend_shield_charge_only_succeeds_when_charge_exists() {
        let mut skill_roster = build_skill_roster(&sample_roster());
        set_charges(&mut skill_roster, 1, 0, 0, 0, 1, 0);

        assert!(spend_shield_charge(&mut skill_roster, 1));
        assert!(!spend_shield_charge(&mut skill_roster, 1));
    }

    #[test]
    fn arm_double_dice_consumes_charge_and_sets_flag() {
        let mut skill_roster = build_skill_roster(&sample_roster());
        set_charges(&mut skill_roster, 1, 0, 0, 0, 0, 1);

        assert!(arm_double_dice(&mut skill_roster, 1));
        assert!(
            player_skill_state(&skill_roster, 1)
                .unwrap()
                .double_dice_armed
        );
        assert_eq!(
            player_skill_state(&skill_roster, 1)
                .unwrap()
                .double_dice_charges,
            0
        );
        assert!(!arm_double_dice(&mut skill_roster, 1));
    }

    #[test]
    fn resolve_roll_from_values_uses_higher_die_for_double_dice() {
        assert_eq!(
            resolve_roll_from_values(2, 5, true),
            RollResolution {
                value: 5,
                dice: [2, 5],
                used_double_dice: true,
            }
        );
        assert_eq!(
            resolve_roll_from_values(4, 0, false),
            RollResolution {
                value: 4,
                dice: [4, 0],
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
                motion_serial: 0,
            },
        ));

        let mut system_state: SystemState<Query<(&PieceId, &mut PieceState)>> =
            SystemState::new(&mut world);
        let query = system_state.get_mut(&mut world).unwrap();
        let mut skill_roster = build_skill_roster(&sample_roster());
        set_charges(&mut skill_roster, 2, 0, 0, 0, 0, 1);

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
                motion_serial: 0,
            },
        ));

        let mut system_state: SystemState<Query<(&PieceId, &mut PieceState)>> =
            SystemState::new(&mut world);
        let query = system_state.get_mut(&mut world).unwrap();
        let mut skill_roster = build_skill_roster(&sample_roster());
        set_charges(&mut skill_roster, 2, 0, 0, 0, 1, 0);

        assert!(should_ai_use_shield(2, &skill_roster, &query));
        assert_eq!(preferred_shield_target(2, &query), Some(1));
    }

    #[test]
    fn shield_target_requires_active_piece_below_max_shield() {
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
                motion_serial: 0,
            },
        ));
        world.spawn((
            PieceId(2),
            PieceState {
                owner_player_id: 2,
                team_id: 2,
                status: PieceStatus::Active,
                progress: 4,
                shield: MAX_PIECE_SHIELD,
                stack_shield: 0,
                motion_serial: 0,
            },
        ));

        let mut system_state: SystemState<Query<(&PieceId, &mut PieceState)>> =
            SystemState::new(&mut world);
        let query = system_state.get_mut(&mut world).unwrap();
        let mut skill_roster = build_skill_roster(&sample_roster());
        set_charges(&mut skill_roster, 2, 0, 0, 0, 1, 0);

        assert!(!should_ai_use_shield(2, &skill_roster, &query));
        assert_eq!(preferred_shield_target(2, &query), None);
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
                motion_serial: 0,
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
                motion_serial: 0,
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
                motion_serial: 0,
            },
        ));

        let mut system_state: SystemState<Query<(&PieceId, &mut PieceState)>> =
            SystemState::new(&mut world);
        let query = system_state.get_mut(&mut world).unwrap();

        assert_eq!(collect_snipe_targets(1, 1, &query), vec![3, 2]);
    }

    #[test]
    fn collect_snipe_targets_excludes_home_lane_enemies() {
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
                motion_serial: 0,
            },
        ));
        world.spawn((
            PieceId(2),
            PieceState {
                owner_player_id: 3,
                team_id: 2,
                status: PieceStatus::Active,
                progress: HOME_ENTRY_PROGRESS + 1,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
        ));
        world.spawn((
            PieceId(3),
            PieceState {
                owner_player_id: 4,
                team_id: 2,
                status: PieceStatus::Active,
                progress: 6,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
        ));

        let mut system_state: SystemState<Query<(&PieceId, &mut PieceState)>> =
            SystemState::new(&mut world);
        let query = system_state.get_mut(&mut world).unwrap();

        assert_eq!(collect_snipe_targets(1, 1, &query), vec![3]);
    }
}
