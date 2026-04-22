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
    BoardLayout, MatchConfig, MatchResult, PlayerProfile, PlayerRoster, TeamRoster,
    evaluate_match_result,
};
use crate::gameplay::skill_flow::{
    SkillRoster, disable_next_skill_turn, grant_random_skill_charge,
};
use crate::plugins::piece_plugin::{HangarSlot, PieceId};
use crate::states::GamePhase;

pub const MAIN_ROUTE_STEPS: u8 = 48;
pub const HOME_LANE_STEPS: u8 = 6;
pub const FINISH_DISTANCE: u8 = MAIN_ROUTE_STEPS + HOME_LANE_STEPS;
pub const MAX_CHAIN_EXTRA_ROLLS: u8 = 3;
pub const MAX_PIECE_SHIELD: u8 = 2;

#[derive(Clone, Debug, Default, Eq, PartialEq, Resource)]
/// 回合状态资源：当前玩家、掷骰状态与最近一次动作日志。
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
    /// 创建对局首回合状态：默认从 P1 开始，无历史掷骰与动作。
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
/// 回合输入缓存：候选动作、可点击棋子和提示文案。
pub struct TurnInputState {
    pending_actions: Vec<PlannedAction>,
    candidate_piece_ids: Vec<u8>,
    pub prompt: Option<String>,
}

impl TurnInputState {
    /// 返回当前阶段可供玩家选择的棋子列表（用于 UI 高亮与点击判定）。
    pub fn candidate_piece_ids(&self) -> &[u8] {
        &self.candidate_piece_ids
    }
}

#[derive(Clone, Copy, Debug)]
/// 执行动作描述：起飞或移动。
pub enum PlannedAction {
    Launch { piece_id: u8, target_progress: u8 },
    Move { piece_id: u8, target_progress: u8 },
}

impl PlannedAction {
    /// 获取动作对应的棋子 ID，便于统一处理 Launch/Move 两类动作。
    pub fn piece_id(&self) -> u8 {
        match *self {
            Self::Launch { piece_id, .. } | Self::Move { piece_id, .. } => piece_id,
        }
    }

    /// 判断动作是否为“移动”而非“起飞”。
    pub fn is_move(&self) -> bool {
        matches!(self, Self::Move { .. })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// 棋盘逻辑位置：主环道、冲线道或终点。
pub enum BoardPosition {
    Main(u8),
    Home(u8),
    Goal,
}

#[derive(Clone, Copy, Debug)]
/// 供决策阶段使用的棋子快照。
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
/// 动作执行前状态快照（用于护盾反弹回退）。
struct ActionOrigin {
    status: PieceStatus,
    progress: u8,
    translation: Vec3,
    new_progress: u8,
}

/// 掷六面骰，结果范围固定为 1..=6。
pub fn roll_die() -> u8 {
    random_range(1..=6)
}

/// 查询当前回合玩家的人机控制类型。
pub fn current_player_control(
    current_player: u8,
    player_roster: &PlayerRoster,
) -> Option<PlayerControl> {
    player_roster
        .players
        .iter()
        .find(|player| player.state.player_id == current_player)
        .map(|player| player.state.control)
}

/// 读取 1~4 数字键，返回合法的动作序号。
pub fn pressed_selection_key(keyboard: &ButtonInput<KeyCode>, max_actions: usize) -> Option<usize> {
    let keys = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
    ];
    keys.iter().enumerate().find_map(|(index, key)| {
        (index < max_actions && keyboard.just_pressed(*key)).then_some(index)
    })
}

/// 写入本回合掷骰结果，并同步处理“连续 6 点额外回合”计数。
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

/// AI 自动决策动作：
/// 1) 优先尝试带收益的起飞/撞击；
/// 2) 再选择普通可行动作；
/// 3) 无动作时返回 None。
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
        for piece in snapshots
            .iter()
            .filter(|piece| piece.owner_player_id == current_player)
        {
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

    for piece in snapshots
        .iter()
        .filter(|piece| piece.owner_player_id == current_player)
    {
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
        for piece in snapshots
            .iter()
            .filter(|piece| piece.owner_player_id == current_player)
        {
            if piece.status == PieceStatus::InHangar {
                return Some(PlannedAction::Launch {
                    piece_id: piece.piece_id,
                    target_progress: 0,
                });
            }
        }
    }

    for piece in snapshots
        .iter()
        .filter(|piece| piece.owner_player_id == current_player)
    {
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

/// 收集当前玩家在本次掷骰下的全部合法动作，供人类选择或 AI 评估。
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
        for piece in snapshots
            .iter()
            .filter(|piece| piece.owner_player_id == current_player)
        {
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

    for piece in snapshots
        .iter()
        .filter(|piece| piece.owner_player_id == current_player)
    {
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

/// 执行一次完整动作结算流水：
/// 起飞/移动 -> 飞跃 -> 撞击 -> 落点效果 -> 叠加 -> 胜负检查 -> 回合推进。
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
    // 先清理出发格叠加态，避免“离开后仍共享护盾”的残留状态。
    clear_stack_from_origin(&action, player_roster, piece_query);
    let action_origin = apply_action(&action, board_layout, player_roster, piece_query);
    let mut notes = Vec::new();
    apply_jump_effect(
        &action,
        board_layout,
        player_roster,
        piece_query,
        &mut notes,
    );
    let attacker_landed = resolve_collision(
        &action,
        action_origin,
        board_layout,
        player_roster,
        match_config,
        piece_query,
        &mut notes,
    );
    // 只有攻击方最终留在落点，才继续结算格子效果与队友叠加。
    if attacker_landed {
        apply_post_collision_tile_effects(
            &action,
            board_layout,
            player_roster,
            piece_query,
            skill_roster,
            &mut notes,
        );
        apply_team_stack(
            &action,
            player_roster,
            match_config,
            piece_query,
            &mut notes,
        );
    }

    let player_completion = player_roster
        .players
        .iter()
        .map(|player| {
            let player_id = player.state.player_id;
            let all_finished = piece_query
                .iter()
                .filter(|(_, _, piece_state, _)| piece_state.owner_player_id == player_id)
                .all(|(_, _, piece_state, _)| piece_state.status == PieceStatus::Finished);
            (player_id, all_finished)
        })
        .collect::<Vec<_>>();

    let evaluated_result = evaluate_match_result(team_roster, &player_completion);
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

/// 当玩家无合法动作时，直接结束当前行动并切换到下一掷骰阶段。
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

/// 记录候选动作与提示文案，并切换到“等待选棋子”阶段。
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

/// 通过序号读取候选动作。
pub fn get_pending_action(input_state: &TurnInputState, selection: usize) -> Option<PlannedAction> {
    input_state.pending_actions.get(selection).copied()
}

/// 通过棋子 ID 反查其对应的候选动作（用于点击棋子时命中动作）。
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

/// 清理本轮输入缓存，避免跨回合污染。
pub fn clear_pending_input(input_state: &mut TurnInputState) {
    input_state.pending_actions.clear();
    input_state.candidate_piece_ids.clear();
    input_state.prompt = None;
}

/// 计算移动后的目标进度；若超终点则返回 None（精确到达规则）。
pub fn compute_target_distance(current_distance: u8, roll_value: u8) -> Option<u8> {
    current_distance
        .checked_add(roll_value)
        .filter(|next_distance| *next_distance <= FINISH_DISTANCE)
}

/// 将“逻辑进度”映射成棋盘位置（主环道/冲线道/终点）。
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

/// 将棋子状态映射到世界坐标，供渲染层更新 Transform。
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

/// 提取棋子快照，避免在决策阶段反复遍历可变查询。
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

/// 判断某目标位置上是否存在敌方棋子（用于起飞/移动优先级决策）。
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

/// 实际写入棋子状态与坐标，返回动作前状态快照（供“反弹恢复”使用）。
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

/// 飞跃结算：
/// 1) 落在与当前棋子同色的主环道格会触发同色飞跃；
/// 2) 若该格存在虚线快捷路径，则优先走快捷路径。
fn apply_jump_effect(
    action: &PlannedAction,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
    notes: &mut Vec<String>,
) {
    if board_layout.route_len() == 0 {
        return;
    }

    let Some(BoardPosition::Main(tile_index)) =
        attacker_position(action, player_roster, piece_query)
    else {
        return;
    };

    let Some(owner_player_id) = owner_player_id_for_action(action, piece_query) else {
        return;
    };

    if !is_same_color_route_tile(owner_player_id, tile_index) {
        return;
    }

    let current_progress = attacker_progress(action, piece_query).unwrap_or_default();

    let jump_delta = if let Some(shortcut_target_index) =
        board_layout.jump_shortcut_target_for_route_index(tile_index)
    {
        circular_forward_steps(tile_index, shortcut_target_index, MAIN_ROUTE_STEPS)
    } else {
        next_same_color_jump_steps(owner_player_id, tile_index)
    };

    if jump_delta == 0 {
        return;
    }

    let final_progress = current_progress
        .saturating_add(jump_delta)
        .min(FINISH_DISTANCE);
    update_piece_progress(
        action,
        final_progress,
        board_layout,
        player_roster,
        piece_query,
    );
    if board_layout
        .jump_shortcut_target_for_route_index(tile_index)
        .is_some()
    {
        notes.push(format!("took shortcut jump to tile {final_progress}"));
    } else {
        notes.push(format!("jumped to next same-color tile {final_progress}"));
    }
}

/// 将玩家 ID 映射为主环道颜色序号：
/// 0=Green, 1=Blue, 2=Red, 3=Yellow。
fn player_route_color_mod(player_id: u8) -> u8 {
    match player_id {
        1 => 1,
        2 => 2,
        3 => 0,
        4 => 3,
        _ => 0,
    }
}

/// 主环道当前格是否为该玩家的同色格。
fn is_same_color_route_tile(player_id: u8, tile_index: u8) -> bool {
    tile_index % 4 == player_route_color_mod(player_id)
}

/// 计算主环道上从 current 到 target 的顺时针前进步数。
fn circular_forward_steps(current: u8, target: u8, route_len: u8) -> u8 {
    if route_len == 0 {
        return 0;
    }
    if target >= current {
        target - current
    } else {
        route_len - (current - target)
    }
}

/// 同色飞跃默认跳到下一处同色格（主环道配色按 4 格循环）。
fn next_same_color_jump_steps(player_id: u8, tile_index: u8) -> u8 {
    let owner_mod = player_route_color_mod(player_id);
    for step in 1..MAIN_ROUTE_STEPS {
        let next_index = (tile_index + step) % MAIN_ROUTE_STEPS;
        if next_index % 4 == owner_mod {
            return step;
        }
    }
    0
}

/// 撞击结算后的落点效果（防御格、事件格等）。
fn apply_post_collision_tile_effects(
    action: &PlannedAction,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
    skill_roster: &mut SkillRoster,
    notes: &mut Vec<String>,
) {
    if board_layout.route_len() == 0 {
        return;
    }

    let Some(BoardPosition::Main(tile_index)) =
        attacker_position(action, player_roster, piece_query)
    else {
        return;
    };
    let Some(tile_kind) = board_layout.tile_kind_for_route_index(tile_index) else {
        return;
    };

    match tile_kind {
        TileKind::Defense => {
            if let Some(shield) = modify_piece_shield(action, piece_query, 1) {
                notes.push(format!("gained shield ({shield})"));
            }
        }
        TileKind::Event => {
            let mut final_progress = attacker_progress(action, piece_query).unwrap_or_default();
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
        TileKind::Attack | TileKind::Goal | TileKind::Jump | TileKind::Normal => {}
    }
}

/// 撞击主逻辑：
/// - 先判定共享护盾；
/// - 再判定单体护盾；
/// - 无护盾则送回机库。
fn resolve_collision(
    action: &PlannedAction,
    action_origin: Option<ActionOrigin>,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
    match_config: &MatchConfig,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
    notes: &mut Vec<String>,
) -> bool {
    let Some(attacker_board_position) = attacker_position(action, player_roster, piece_query)
    else {
        return true;
    };
    let BoardPosition::Main(target_tile_index) = attacker_board_position else {
        return true;
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
        return true;
    };

    let mut defenders_with_stack = Vec::new();
    for (piece_id, _, piece_state, _) in piece_query.iter_mut() {
        if piece_id.0 == attacker_piece_id {
            continue;
        }

        if piece_state.status != PieceStatus::Active
            || piece_state.team_id == attacker_team
            || piece_board_position(*piece_state, player_roster)
                != Some(BoardPosition::Main(target_tile_index))
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
        append_attack_tile_collision_note(board_layout, target_tile_index, notes);
        return false;
    }

    let mut collision_blocked = false;
    for (piece_id, hangar_slot, mut piece_state, mut transform) in piece_query.iter_mut() {
        if piece_id.0 == attacker_piece_id {
            continue;
        }

        if piece_state.status != PieceStatus::Active
            || piece_state.team_id == attacker_team
            || piece_board_position(*piece_state, player_roster)
                != Some(BoardPosition::Main(target_tile_index))
        {
            continue;
        }

        if piece_state.shield > 0 {
            piece_state.shield -= 1;
            collision_blocked = true;
            notes.push(format!(
                "piece #{} blocked collision with shield",
                piece_id.0
            ));
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
        append_attack_tile_collision_note(board_layout, target_tile_index, notes);
        return false;
    } else if notes
        .iter()
        .any(|note| note.contains("sent piece #") && note.contains("back to hangar"))
    {
        append_attack_tile_collision_note(board_layout, target_tile_index, notes);
    }
    true
}

/// 若撞击发生在攻击格，补充一条强化撞击说明。
fn append_attack_tile_collision_note(
    board_layout: &BoardLayout,
    target_tile_index: u8,
    notes: &mut Vec<String>,
) {
    if board_layout.tile_kind_for_route_index(target_tile_index) == Some(TileKind::Attack) {
        notes.push("enhanced collision on attack tile".to_string());
    }
}

/// 护盾阻挡撞击时，将攻击方回退到动作前状态。
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

/// 更新棋子进度与状态，并同步刷新棋子坐标。
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

/// 调整棋子护盾层数（带上限钳制）。
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

/// 棋子离开原位置时，清理原叠加体共享护盾。
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

/// 2v2 队友叠加判定：同格两枚及以上队友棋子获得共享护盾。
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

/// 消耗叠加体共享护盾。
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

/// 随机事件总入口：抽事件类型后交给具体效果函数执行。
fn apply_event_effect(
    action: &PlannedAction,
    final_progress: &mut u8,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
    skill_roster: &mut SkillRoster,
) -> Option<String> {
    apply_event_kind_effect(
        random_event_kind(),
        action,
        final_progress,
        board_layout,
        player_roster,
        piece_query,
        skill_roster,
    )
}

/// 随机选取一个事件类型。
fn random_event_kind() -> TileEventKind {
    match random_range(0..=4) {
        0 => TileEventKind::GainShield,
        1 => TileEventKind::GainSkillCharge,
        2 => TileEventKind::AdvanceTwo,
        3 => TileEventKind::DisableNextSkill,
        _ => TileEventKind::RemoveEnemyShield,
    }
}

/// 按事件类型执行对应效果，并返回可读日志。
fn apply_event_kind_effect(
    event_kind: TileEventKind,
    action: &PlannedAction,
    final_progress: &mut u8,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
    skill_roster: &mut SkillRoster,
) -> Option<String> {
    match event_kind {
        TileEventKind::GainShield => {
            let shield = modify_piece_shield(action, piece_query, 1)?;
            Some(format!(
                "event {:?}: gained shield ({shield})",
                TileEventKind::GainShield
            ))
        }
        TileEventKind::GainSkillCharge => {
            let owner_player_id = owner_player_id_for_action(action, piece_query)?;
            let allow_swap = player_roster.players.len() > 2;
            let charged = grant_random_skill_charge(skill_roster, owner_player_id, allow_swap)
                .unwrap_or("UnknownSkill");
            Some(format!(
                "event {:?}: gained 1 {charged} charge",
                TileEventKind::GainSkillCharge
            ))
        }
        TileEventKind::AdvanceTwo => {
            let next_progress = (*final_progress + 2).min(FINISH_DISTANCE);
            *final_progress = next_progress;
            update_piece_progress(
                action,
                next_progress,
                board_layout,
                player_roster,
                piece_query,
            );
            Some(format!(
                "event {:?}: advanced to tile {next_progress}",
                TileEventKind::AdvanceTwo
            ))
        }
        TileEventKind::DisableNextSkill => {
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
        TileEventKind::RemoveEnemyShield => {
            let target_piece_id = action.piece_id();
            let mut attacker_team = None;
            for (piece_id, _, piece_state, _) in piece_query.iter_mut() {
                if piece_id.0 == target_piece_id {
                    attacker_team = Some(piece_state.team_id);
                    break;
                }
            }

            let attacker_team = attacker_team?;
            let candidates = piece_query
                .iter_mut()
                .filter(|(piece_id, _, piece_state, _)| {
                    piece_id.0 != target_piece_id
                        && piece_state.team_id != attacker_team
                        && piece_state.shield > 0
                })
                .map(|(piece_id, _, _, _)| piece_id.0)
                .collect::<Vec<_>>();
            if candidates.is_empty() {
                return Some("event fizzled: no enemy shield to remove".to_string());
            }

            let picked_piece_id = candidates[random_range(0..candidates.len())];
            for (piece_id, _, mut piece_state, _) in piece_query.iter_mut() {
                if piece_id.0 != picked_piece_id {
                    continue;
                }
                piece_state.shield -= 1;
                return Some(format!(
                    "event {:?}: removed shield from piece #{}",
                    TileEventKind::RemoveEnemyShield,
                    piece_id.0
                ));
            }
            Some("event failed: selected enemy shield target disappeared".to_string())
        }
    }
}

/// 读取当前动作所属玩家 ID。
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

/// 将棋子状态换算为棋盘位置（内部工具函数）。
fn piece_board_position(
    piece_state: PieceState,
    player_roster: &PlayerRoster,
) -> Option<BoardPosition> {
    let player_profile = player_roster
        .players
        .iter()
        .find(|player| player.state.player_id == piece_state.owner_player_id)?;
    board_position_for_distance(player_profile, piece_state.progress, piece_state.status)
}

/// 获取攻击方当前位置（用于飞跃/撞击/格子效果结算）。
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

/// 获取攻击方当前逻辑进度。
fn attacker_progress(
    action: &PlannedAction,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> Option<u8> {
    let attacker_piece_id = action.piece_id();
    for (piece_id, _, piece_state, _) in piece_query.iter_mut() {
        if piece_id.0 == attacker_piece_id {
            return Some(piece_state.progress);
        }
    }
    None
}

/// 将动作与附加说明拼接成 HUD 可读日志。
fn describe_action(action: &PlannedAction, roll_value: u8, notes: &[String]) -> String {
    let base = match *action {
        PlannedAction::Launch { piece_id, .. } => {
            format!("rolled {roll_value}, launched piece #{piece_id}")
        }
        PlannedAction::Move {
            piece_id,
            target_progress,
        } => format!("rolled {roll_value}, moved piece #{piece_id} to tile {target_progress}"),
    };

    if notes.is_empty() {
        base
    } else {
        format!("{base}; {}", notes.join(", "))
    }
}

/// 推进回合：优先消耗额外掷骰，否则切换到下一位玩家。
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
    use crate::domain::player::{PlayerControl, PlayerState};
    use crate::gameplay::ai::AiDifficulty;
    use crate::gameplay::match_flow::{MatchSetup, PlayerColorChoice};
    use crate::gameplay::skill_flow::{
        build_skill_roster, can_use_skill_this_turn, sync_turn_skill_usage,
    };
    use bevy::ecs::system::SystemState;

    fn setup(mode: GameMode) -> MatchSetup {
        MatchSetup {
            mode,
            ai_difficulty: AiDifficulty::Normal,
            fast_mode: false,
            human_color: PlayerColorChoice::Crimson,
            pieces_per_player: 2,
            player_controls: [
                PlayerControl::Human,
                PlayerControl::Ai,
                PlayerControl::Human,
                PlayerControl::Ai,
            ],
        }
    }

    #[test]
    fn compute_target_distance_blocks_overshoot() {
        assert_eq!(
            compute_target_distance(FINISH_DISTANCE - 2, 2),
            Some(FINISH_DISTANCE)
        );
        assert_eq!(compute_target_distance(FINISH_DISTANCE - 2, 3), None);
    }

    #[test]
    fn board_position_uses_player_launch_offset_on_main_route() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::TwoVsTwo));
        let player_one = &players[0];
        let player_two = &players[1];

        assert_eq!(
            board_position_for_distance(player_one, 0, PieceStatus::Active),
            Some(BoardPosition::Main(40))
        );
        assert_eq!(
            board_position_for_distance(player_two, 0, PieceStatus::Active),
            Some(BoardPosition::Main(4))
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
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::TwoVsTwo));
        let player_roster = PlayerRoster { players };
        let board_layout = BoardLayout {
            tiles: crate::data::board_config::default_board_tiles(),
        };

        assert_eq!(
            world_position_for_piece(1, 0, PieceStatus::Active, &board_layout, &player_roster),
            board_layout.world_pos_for_route_index(40)
        );
        assert_eq!(
            world_position_for_piece(
                1,
                MAIN_ROUTE_STEPS,
                PieceStatus::Active,
                &board_layout,
                &player_roster
            ),
            Some(Vec2::new(-300.0, 0.0))
        );
        assert_eq!(
            world_position_for_piece(
                1,
                FINISH_DISTANCE,
                PieceStatus::Finished,
                &board_layout,
                &player_roster
            ),
            Some(Vec2::new(-36.0, 0.0))
        );
    }

    #[test]
    fn same_color_jump_advances_to_next_same_color_node() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::OneVsOne));
        let player_roster = PlayerRoster { players };
        let board_layout = BoardLayout::default();

        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: PieceStatus::Active,
                progress: 5,
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

        apply_jump_effect(
            &PlannedAction::Move {
                piece_id: 1,
                target_progress: 5,
            },
            &board_layout,
            &player_roster,
            &mut query,
            &mut notes,
        );

        let progress = query
            .iter_mut()
            .find(|(piece_id, _, _, _)| piece_id.0 == 1)
            .map(|(_, _, piece_state, _)| piece_state.progress)
            .unwrap_or_default();
        assert_eq!(progress, 9);
        assert!(
            notes
                .iter()
                .any(|note| note.contains("next same-color tile"))
        );
    }

    #[test]
    fn same_color_jump_uses_shortcut_when_defined() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::OneVsOne));
        let player_roster = PlayerRoster { players };
        let board_layout = BoardLayout::default();

        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: PieceStatus::Active,
                progress: 13,
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

        apply_jump_effect(
            &PlannedAction::Move {
                piece_id: 1,
                target_progress: 13,
            },
            &board_layout,
            &player_roster,
            &mut query,
            &mut notes,
        );

        let progress = query
            .iter_mut()
            .find(|(piece_id, _, _, _)| piece_id.0 == 1)
            .map(|(_, _, piece_state, _)| piece_state.progress)
            .unwrap_or_default();
        assert_eq!(progress, 25);
        assert!(notes.iter().any(|note| note.contains("shortcut jump")));
    }

    #[test]
    fn collect_actions_returns_launch_and_move_options_for_human_player() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::OneVsOne));
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
        assert!(
            actions
                .iter()
                .any(|action| matches!(action, PlannedAction::Launch { piece_id: 1, .. }))
        );
        assert!(actions.iter().any(|action| matches!(
            action,
            PlannedAction::Move {
                piece_id: 2,
                target_progress: 9
            }
        )));
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

        assert_eq!(
            current_player_control(1, &player_roster),
            Some(PlayerControl::Human)
        );
        assert_eq!(
            current_player_control(2, &player_roster),
            Some(PlayerControl::Ai)
        );
        assert_eq!(current_player_control(9, &player_roster), None);
    }

    #[test]
    fn apply_team_stack_grants_shared_shield_in_two_vs_two() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::TwoVsTwo));
        let player_roster = PlayerRoster { players };
        let match_config = MatchConfig {
            mode: GameMode::TwoVsTwo,
            ai_difficulty: AiDifficulty::Normal,
            fast_mode: false,
            human_color: PlayerColorChoice::Crimson,
            pieces_per_player: 2,
            player_controls: [
                PlayerControl::Human,
                PlayerControl::Ai,
                PlayerControl::Human,
                PlayerControl::Ai,
            ],
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
        assert!(
            notes
                .iter()
                .any(|note| note.contains("stacked with teammate"))
        );
    }

    #[test]
    fn clear_stack_from_origin_removes_shared_shield_from_remaining_stack() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::TwoVsTwo));
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
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::TwoVsTwo));
        let player_roster = PlayerRoster { players };
        let match_config = MatchConfig {
            mode: GameMode::TwoVsTwo,
            ai_difficulty: AiDifficulty::Normal,
            fast_mode: false,
            human_color: PlayerColorChoice::Crimson,
            pieces_per_player: 2,
            player_controls: [
                PlayerControl::Human,
                PlayerControl::Ai,
                PlayerControl::Human,
                PlayerControl::Ai,
            ],
        };
        let board_layout = BoardLayout::default();

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
            &board_layout,
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
        assert!(
            notes
                .iter()
                .any(|note| note.contains("shared stack shield blocked collision"))
        );
    }

    #[test]
    fn resolve_collision_with_shield_bounces_attacker_to_origin() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::TwoVsTwo));
        let player_roster = PlayerRoster { players };
        let match_config = MatchConfig {
            mode: GameMode::TwoVsTwo,
            ai_difficulty: AiDifficulty::Normal,
            fast_mode: false,
            human_color: PlayerColorChoice::Crimson,
            pieces_per_player: 2,
            player_controls: [
                PlayerControl::Human,
                PlayerControl::Ai,
                PlayerControl::Human,
                PlayerControl::Ai,
            ],
        };
        let board_layout = BoardLayout::default();

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
            &board_layout,
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
        assert_eq!(states, vec![(1, 1, 0, -50.0, -70.0), (2, 10, 0, 0.0, 0.0)]);
        assert!(notes.iter().any(|note| note.contains("bounced back")));
    }

    #[test]
    fn gain_skill_charge_event_adds_exactly_one_charge() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::OneVsOne));
        let player_roster = PlayerRoster { players };
        let mut skill_roster = build_skill_roster(&player_roster);
        let board_layout = BoardLayout::default();

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

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world);

        let before = skill_roster
            .players
            .iter()
            .find(|p| p.player_id == 1)
            .map(|p| {
                p.dash_charges
                    + p.snipe_charges
                    + p.swap_charges
                    + p.shield_charges
                    + p.double_dice_charges
            })
            .unwrap_or_default();
        let mut final_progress = 2;
        let note = apply_event_kind_effect(
            TileEventKind::GainSkillCharge,
            &PlannedAction::Move {
                piece_id: 1,
                target_progress: 2,
            },
            &mut final_progress,
            &board_layout,
            &player_roster,
            &mut query,
            &mut skill_roster,
        );
        let after = skill_roster
            .players
            .iter()
            .find(|p| p.player_id == 1)
            .map(|p| {
                p.dash_charges
                    + p.snipe_charges
                    + p.swap_charges
                    + p.shield_charges
                    + p.double_dice_charges
            })
            .unwrap_or_default();

        assert_eq!(after, before + 1);
        assert!(note.unwrap_or_default().contains("GainSkillCharge"));
    }

    #[test]
    fn disable_next_skill_event_blocks_next_turn_only() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::OneVsOne));
        let player_roster = PlayerRoster { players };
        let mut skill_roster = build_skill_roster(&player_roster);
        let board_layout = BoardLayout::default();

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

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world);
        let mut final_progress = 2;

        let note = apply_event_kind_effect(
            TileEventKind::DisableNextSkill,
            &PlannedAction::Move {
                piece_id: 1,
                target_progress: 2,
            },
            &mut final_progress,
            &board_layout,
            &player_roster,
            &mut query,
            &mut skill_roster,
        );
        assert!(note.unwrap_or_default().contains("DisableNextSkill"));

        sync_turn_skill_usage(&mut skill_roster, 1);
        assert!(!can_use_skill_this_turn(&skill_roster, 1));

        sync_turn_skill_usage(&mut skill_roster, 2);
        sync_turn_skill_usage(&mut skill_roster, 1);
        assert!(can_use_skill_this_turn(&skill_roster, 1));
    }

    #[test]
    fn remove_enemy_shield_event_hits_the_only_valid_enemy_target() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::OneVsOne));
        let player_roster = PlayerRoster { players };
        let mut skill_roster = build_skill_roster(&player_roster);
        let board_layout = BoardLayout::default();

        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::ZERO),
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
        world.spawn((
            PieceId(2),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 2,
                team_id: 2,
                status: PieceStatus::Active,
                progress: 4,
                shield: 1,
                stack_shield: 0,
            },
            Transform::default(),
        ));
        world.spawn((
            PieceId(3),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 2,
                team_id: 2,
                status: PieceStatus::Active,
                progress: 5,
                shield: 0,
                stack_shield: 0,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world);
        let mut final_progress = 3;

        let note = apply_event_kind_effect(
            TileEventKind::RemoveEnemyShield,
            &PlannedAction::Move {
                piece_id: 1,
                target_progress: 3,
            },
            &mut final_progress,
            &board_layout,
            &player_roster,
            &mut query,
            &mut skill_roster,
        )
        .unwrap_or_default();

        let shields = query
            .iter_mut()
            .map(|(piece_id, _, piece_state, _)| (piece_id.0, piece_state.shield))
            .collect::<Vec<_>>();
        assert_eq!(shields, vec![(1, 0), (2, 0), (3, 0)]);
        assert!(note.contains("removed shield from piece #2"));
    }
}
