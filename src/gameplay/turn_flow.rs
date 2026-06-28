use bevy::prelude::*;
use rand::random_range;

use crate::data::game_mode::GameMode;
use crate::domain::dice::DiceRoll;
use crate::domain::event::TileEventKind;
use crate::domain::piece::{PieceState, PieceStatus};
use crate::domain::player::PlayerControl;
use crate::domain::rules::{LaunchRule, can_launch};
use crate::domain::tile::TileKind;
use crate::gameplay::match_flow::{
    BoardLayout, MatchConfig, MatchResult, PlayerProfile, PlayerRoster, PlayerSeat, TeamRoster,
    evaluate_match_result, turn_marker_position_for_seat,
};
use crate::gameplay::skill_flow::{
    SkillRoster, disable_next_skill_turn, grant_random_skill_charge,
};
use crate::plugins::piece_plugin::{HangarSlot, PieceId};
use crate::states::GamePhase;

pub const BOARD_ROUTE_TILES: u8 = 48;
pub const MAIN_ROUTE_STEPS: u8 = 52;
pub const HOME_LANE_STEPS: u8 = 6;
pub const HOME_ENTRY_PROGRESS: u8 = MAIN_ROUTE_STEPS - 3;
pub const FINISH_DISTANCE: u8 = HOME_ENTRY_PROGRESS + HOME_LANE_STEPS;
pub const MAX_CHAIN_EXTRA_ROLLS: u8 = 3;
pub const MAX_PIECE_SHIELD: u8 = 2;

#[derive(Clone, Debug, Default, Eq, PartialEq, Resource)]
/// 回合状态资源：当前玩家、掷骰状态与最近一次动作日志。
pub struct TurnState {
    pub current_player: u8,
    pub extra_rolls_remaining: u8,
    pub consecutive_sixes: u8,
    pub turn_index: u32,
    pub roll_serial: u32,
    pub current_roll: Option<u8>,
    pub current_roll_faces: Option<[u8; 2]>,
    pub last_roll: Option<u8>,
    pub last_roll_faces: Option<[u8; 2]>,
    pub last_roll_player: Option<u8>,
    pub hold_last_roll_display: bool,
    pub roll_display_animation_started: bool,
    pub player_last_rolls: [Option<u8>; 4],
    pub player_last_roll_faces: [Option<[u8; 2]>; 4],
    pub pending_roll_display: Option<PendingRollDisplay>,
    pub last_piece_effect: Option<PieceEffectNotice>,
    pub last_action: Option<String>,
    pub last_action_player_id: Option<u8>,
    pub last_action_turn_index: u32,
    pub last_action_serial: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PendingRollDisplay {
    pub roll_serial: u32,
    pub player_id: u8,
    pub roll: u8,
    pub faces: [u8; 2],
}

impl TurnState {
    /// 创建对局首回合状态：默认从 P1 开始，无历史掷骰与动作。
    pub fn opening_turn() -> Self {
        Self {
            current_player: 1,
            extra_rolls_remaining: 0,
            consecutive_sixes: 0,
            turn_index: 1,
            roll_serial: 0,
            current_roll: None,
            current_roll_faces: None,
            last_roll: None,
            last_roll_faces: None,
            last_roll_player: None,
            hold_last_roll_display: false,
            roll_display_animation_started: false,
            player_last_rolls: [None; 4],
            player_last_roll_faces: [None; 4],
            pending_roll_display: None,
            last_piece_effect: None,
            last_action: None,
            last_action_player_id: None,
            last_action_turn_index: 0,
            last_action_serial: 0,
        }
    }

    /// 查询指定玩家最近一次掷骰结果，用于棋盘停机坪骰面显示。
    pub fn player_last_roll(&self, player_id: u8) -> Option<u8> {
        self.player_last_rolls
            .get(player_id.saturating_sub(1) as usize)
            .copied()
            .flatten()
    }

    pub fn player_last_roll_faces(&self, player_id: u8) -> Option<[u8; 2]> {
        self.player_last_roll_faces
            .get(player_id.saturating_sub(1) as usize)
            .copied()
            .flatten()
    }
}

/// 记录一次行动日志事件，并保留其发生时的回合号。
pub fn record_turn_action(turn_state: &mut TurnState, action: impl Into<String>) {
    turn_state.last_action = Some(action.into());
    turn_state.last_action_player_id = Some(turn_state.current_player);
    turn_state.last_action_turn_index = turn_state.turn_index;
    turn_state.last_action_serial = turn_state.last_action_serial.saturating_add(1);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// 最近一次特殊格结算产生的棋子附加效果。
pub struct PieceEffectNotice {
    pub piece_id: u8,
    pub kind: PieceEffectKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PieceEffectKind {
    Attack,
    Defense,
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

    pub fn pending_actions(&self) -> &[PlannedAction] {
        &self.pending_actions
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
    Launch,
    Main(u8),
    TurnMarker(PlayerSeat),
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
    owner_player_id: u8,
    status: PieceStatus,
    progress: u8,
    translation: Vec3,
    new_progress: u8,
}

/// Immutable resources needed to resolve one player action.
pub struct ActionResources<'a> {
    pub player_roster: &'a PlayerRoster,
    pub team_roster: &'a TeamRoster,
    pub match_config: &'a MatchConfig,
    pub board_layout: &'a BoardLayout,
}

/// Mutable match state touched while resolving one player action.
pub struct ActionState<'a> {
    pub skill_roster: &'a mut SkillRoster,
    pub match_result: &'a mut MatchResult,
    pub turn_state: &'a mut TurnState,
    pub input_state: &'a mut TurnInputState,
    pub next_phase: &'a mut NextState<GamePhase>,
}

#[derive(Clone, Copy)]
struct LandingResources<'a> {
    player_roster: &'a PlayerRoster,
    match_config: &'a MatchConfig,
    board_layout: &'a BoardLayout,
    jump_source_event_tile: Option<u8>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct MoveRouteEffects {
    shortcut_used: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TileEventOutcome {
    kind: TileEventKind,
    note: String,
    attacker_still_landed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct JumpResolution {
    target_progress: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MovementStepKind {
    Normal,
    Shortcut,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MovementStep {
    pub progress: u8,
    pub kind: MovementStepKind,
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
    set_roll_with_faces(turn_state, roll_value, [roll_value, 0]);
}

pub fn set_roll_with_faces(turn_state: &mut TurnState, roll_value: u8, roll_faces: [u8; 2]) {
    let roll_faces = normalized_roll_faces(roll_value, roll_faces);
    turn_state.roll_serial = turn_state.roll_serial.wrapping_add(1);
    turn_state.current_roll = Some(roll_value);
    turn_state.current_roll_faces = Some(roll_faces);
    turn_state.last_roll = Some(roll_value);
    turn_state.last_roll_faces = Some(roll_faces);
    turn_state.last_roll_player = Some(turn_state.current_player);
    turn_state.hold_last_roll_display = false;
    turn_state.roll_display_animation_started = false;
    turn_state.last_piece_effect = None;
    turn_state.pending_roll_display = Some(PendingRollDisplay {
        roll_serial: turn_state.roll_serial,
        player_id: turn_state.current_player,
        roll: roll_value,
        faces: roll_faces,
    });

    if roll_value == 6 {
        if turn_state.consecutive_sixes < MAX_CHAIN_EXTRA_ROLLS {
            turn_state.extra_rolls_remaining = turn_state.extra_rolls_remaining.saturating_add(1);
        }
        turn_state.consecutive_sixes = turn_state.consecutive_sixes.saturating_add(1);
    } else {
        turn_state.consecutive_sixes = 0;
    }
}

pub fn commit_pending_roll_display(turn_state: &mut TurnState, roll_serial: u32) -> bool {
    let Some(pending) = turn_state.pending_roll_display else {
        return false;
    };
    if pending.roll_serial != roll_serial {
        return false;
    }

    if let Some(player_roll) = turn_state
        .player_last_rolls
        .get_mut(pending.player_id.saturating_sub(1) as usize)
    {
        *player_roll = Some(pending.roll);
    }
    if let Some(player_roll_faces) = turn_state
        .player_last_roll_faces
        .get_mut(pending.player_id.saturating_sub(1) as usize)
    {
        *player_roll_faces = Some(pending.faces);
    }
    turn_state.pending_roll_display = None;
    true
}

fn normalized_roll_faces(roll_value: u8, roll_faces: [u8; 2]) -> [u8; 2] {
    if roll_faces[0] == 0 {
        [roll_value, 0]
    } else {
        roll_faces
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
    launch_rule: LaunchRule,
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

    if launch_rule.allows(roll) {
        for piece in snapshots
            .iter()
            .filter(|piece| piece.owner_player_id == current_player)
        {
            if piece.status != PieceStatus::InHangar {
                continue;
            }

            if can_launch(
                &PieceState {
                    owner_player_id: piece.owner_player_id,
                    team_id: piece.team_id,
                    status: piece.status,
                    progress: piece.distance,
                    shield: piece.shield,
                    stack_shield: 0,
                    motion_serial: 0,
                },
                roll,
                launch_rule,
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
        if !matches!(piece.status, PieceStatus::AtLaunch | PieceStatus::Active) {
            continue;
        }

        let Some(target_progress) = compute_move_target_distance_on_board(
            piece.owner_player_id,
            piece.status,
            piece.distance,
            roll.0.saturating_add(move_bonus),
            board_layout,
            player_roster,
        ) else {
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

    if launch_rule.allows(roll) {
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
        if matches!(piece.status, PieceStatus::AtLaunch | PieceStatus::Active) {
            let Some(target_progress) = compute_move_target_distance_on_board(
                piece.owner_player_id,
                piece.status,
                piece.distance,
                roll.0.saturating_add(move_bonus),
                board_layout,
                player_roster,
            ) else {
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
    launch_rule: LaunchRule,
    board_layout: &BoardLayout,
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

    if launch_rule.allows(roll) {
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
                        motion_serial: 0,
                    },
                    roll,
                    launch_rule,
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
        if !matches!(piece.status, PieceStatus::AtLaunch | PieceStatus::Active) {
            continue;
        }

        if let Some(target_progress) = compute_move_target_distance_on_board(
            piece.owner_player_id,
            piece.status,
            piece.distance,
            roll.0.saturating_add(move_bonus),
            board_layout,
            player_roster,
        ) {
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
    movement_roll_value: u8,
    resources: ActionResources<'_>,
    state: ActionState<'_>,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) {
    // 先清理出发格叠加态，避免“离开后仍共享护盾”的残留状态。
    if action.is_move() {
        clear_stack_from_origin(&action, resources.player_roster, piece_query);
    }
    let action_origin = apply_action(
        &action,
        resources.board_layout,
        resources.player_roster,
        piece_query,
    );
    let mut notes = Vec::new();
    let mut piece_effect_notice = None;
    let mut jump_source_event_tile_for_action = None;
    let route_effects = move_route_effects_for_action(
        &action,
        action_origin.as_ref(),
        movement_roll_value,
        resources.board_layout,
        resources.player_roster,
    );
    let attacker_landed = if action.is_move() {
        let pre_jump_position = attacker_position(&action, resources.player_roster, piece_query);
        apply_jump_effect(
            &action,
            route_effects,
            resources.board_layout,
            resources.player_roster,
            piece_query,
            &mut notes,
        );
        let post_jump_position = attacker_position(&action, resources.player_roster, piece_query);
        jump_source_event_tile_for_action = jump_source_event_tile(
            pre_jump_position,
            post_jump_position,
            resources.board_layout,
        );
        let notes_before_collision = notes.len();
        let attacker_landed = resolve_collision(
            &action,
            action_origin,
            resources.board_layout,
            resources.player_roster,
            resources.match_config,
            piece_query,
            &mut notes,
        );
        if notes[notes_before_collision..]
            .iter()
            .any(|note| note.contains("enhanced collision on attack tile"))
        {
            piece_effect_notice = Some(PieceEffectNotice {
                piece_id: action.piece_id(),
                kind: PieceEffectKind::Attack,
            });
        }
        attacker_landed
    } else {
        true
    };

    // 只有移动动作的攻击方最终留在落点，才继续结算格子效果与队友叠加。
    if attacker_landed && action.is_move() {
        let attacker_still_landed = apply_post_collision_tile_effects(
            &action,
            LandingResources {
                player_roster: resources.player_roster,
                match_config: resources.match_config,
                board_layout: resources.board_layout,
                jump_source_event_tile: jump_source_event_tile_for_action,
            },
            piece_query,
            state.skill_roster,
            &mut notes,
            &mut piece_effect_notice,
        );
        if attacker_still_landed {
            apply_team_stack(
                &action,
                resources.player_roster,
                resources.match_config,
                piece_query,
                &mut notes,
            );
        }
    }

    let player_completion = resources
        .player_roster
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

    let evaluated_result = evaluate_match_result(resources.team_roster, &player_completion);
    if evaluated_result.finished {
        state.match_result.winner_team_id = evaluated_result.winner_team_id;
        state.match_result.winner_player_ids = evaluated_result.winner_player_ids.clone();
        state.match_result.finished = true;
        notes.push(format!(
            "team {} wins",
            evaluated_result.winner_team_id.unwrap_or_default()
        ));
    }

    record_turn_action(
        state.turn_state,
        describe_action(&action, roll_value, &notes),
    );
    state.turn_state.last_piece_effect = piece_effect_notice;
    clear_pending_input(state.input_state);

    if state.match_result.finished {
        state.next_phase.set(GamePhase::CheckVictory);
        return;
    }

    state.turn_state.hold_last_roll_display = true;
    state.turn_state.roll_display_animation_started = false;
    advance_turn(
        state.turn_state,
        resources.player_roster.players.len() as u8,
    );
    state.next_phase.set(GamePhase::AwaitDice);
}

/// 当玩家无合法动作时，直接结束当前行动并切换到下一掷骰阶段。
pub fn finish_turn_without_action(
    turn_state: &mut TurnState,
    input_state: &mut TurnInputState,
    player_roster: &PlayerRoster,
    next_phase: &mut ResMut<NextState<GamePhase>>,
) {
    clear_pending_input(input_state);
    turn_state.hold_last_roll_display = false;
    turn_state.roll_display_animation_started = false;
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
        "Rolled {}. Tap a highlighted piece to move.",
        roll_value
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
    let mut current_progress = current_distance;
    let mut direction = 1;
    for _ in 0..roll_value {
        current_progress = next_progress_with_finish_bounce(current_progress, &mut direction)?;
    }
    Some(current_progress)
}

/// 计算棋子移动后的目标进度；起飞点视为主环道前一格。
pub fn compute_move_target_distance(
    status: PieceStatus,
    current_distance: u8,
    roll_value: u8,
) -> Option<u8> {
    match status {
        PieceStatus::AtLaunch => roll_value.checked_sub(1),
        PieceStatus::Active => compute_target_distance(current_distance, roll_value),
        _ => None,
    }
}

/// 计算棋子按真实棋盘路径移动后的目标进度；若最后一步落在虚线飞跃起点，
/// 则飞跃起点和终点共同消耗这 1 个骰点。
pub fn compute_move_target_distance_on_board(
    owner_player_id: u8,
    status: PieceStatus,
    current_distance: u8,
    roll_value: u8,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
) -> Option<u8> {
    movement_steps_for_roll(
        owner_player_id,
        status,
        current_distance,
        roll_value,
        board_layout,
        player_roster,
    )
    .and_then(|steps| steps.last().copied().map(|step| step.progress))
}

/// 按骰点展开本次移动路径，供规则结算与动画层保持一致。
pub fn movement_steps_for_roll(
    owner_player_id: u8,
    status: PieceStatus,
    current_distance: u8,
    roll_value: u8,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
) -> Option<Vec<MovementStep>> {
    if roll_value == 0 {
        return None;
    }

    let mut steps = Vec::new();
    let mut remaining_steps = roll_value;
    let mut current_progress = match status {
        PieceStatus::AtLaunch => {
            steps.push(MovementStep {
                progress: 0,
                kind: MovementStepKind::Normal,
            });
            remaining_steps = remaining_steps.checked_sub(1)?;
            0
        }
        PieceStatus::Active if current_distance <= FINISH_DISTANCE => current_distance,
        _ => return None,
    };
    let mut shortcut_used = false;
    let mut direction = 1;

    while remaining_steps > 0 {
        current_progress = next_progress_with_finish_bounce(current_progress, &mut direction)?;
        steps.push(MovementStep {
            progress: current_progress,
            kind: MovementStepKind::Normal,
        });
        remaining_steps = remaining_steps.saturating_sub(1);

        if remaining_steps > 0 || shortcut_used || direction < 0 {
            continue;
        }

        if let Some(shortcut_progress) = shortcut_progress_after_landing(
            owner_player_id,
            current_progress,
            board_layout,
            player_roster,
        ) {
            current_progress = shortcut_progress;
            steps.push(MovementStep {
                progress: current_progress,
                kind: MovementStepKind::Shortcut,
            });
            shortcut_used = true;
        }
    }

    Some(steps)
}

/// 从前后逻辑状态反推视觉移动路径。若最终目标来自虚线飞跃，直接连向虚线目标。
pub fn movement_steps_between_progresses(
    owner_player_id: u8,
    previous_status: PieceStatus,
    previous_progress: u8,
    current_status: PieceStatus,
    current_progress: u8,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
) -> Option<Vec<MovementStep>> {
    if !matches!(current_status, PieceStatus::Active | PieceStatus::Finished) {
        return None;
    }

    let target_progress = current_progress.min(FINISH_DISTANCE);
    if previous_status == PieceStatus::Active
        && previous_progress >= HOME_ENTRY_PROGRESS
        && target_progress <= previous_progress
    {
        return movement_steps_between_home_lane_bounce(previous_progress, target_progress);
    }

    let mut next_progress = match previous_status {
        PieceStatus::AtLaunch => 0,
        PieceStatus::Active if previous_progress < target_progress => {
            previous_progress.saturating_add(1)
        }
        _ => return None,
    };

    let mut steps = Vec::new();
    let mut shortcut_used = false;
    while next_progress <= target_progress {
        steps.push(MovementStep {
            progress: next_progress,
            kind: MovementStepKind::Normal,
        });
        if next_progress == target_progress {
            break;
        }

        if !shortcut_used
            && let Some(shortcut_progress) = shortcut_progress_after_landing(
                owner_player_id,
                next_progress,
                board_layout,
                player_roster,
            )
            && shortcut_progress <= target_progress
        {
            steps.push(MovementStep {
                progress: shortcut_progress,
                kind: MovementStepKind::Shortcut,
            });
            shortcut_used = true;
            if shortcut_progress == target_progress {
                break;
            }
            next_progress = shortcut_progress.saturating_add(1);
            continue;
        }

        next_progress = next_progress.saturating_add(1);
    }

    Some(steps)
}

fn movement_steps_between_home_lane_bounce(
    previous_progress: u8,
    target_progress: u8,
) -> Option<Vec<MovementStep>> {
    let mut steps = Vec::new();
    let mut current_progress = previous_progress;
    let mut direction = 1;
    let mut bounced_from_finish = false;

    for _ in 0..=(HOME_LANE_STEPS * 2) {
        current_progress = next_progress_with_finish_bounce(current_progress, &mut direction)?;
        if current_progress == FINISH_DISTANCE {
            bounced_from_finish = true;
        }
        steps.push(MovementStep {
            progress: current_progress,
            kind: MovementStepKind::Normal,
        });
        if bounced_from_finish && current_progress == target_progress {
            return Some(steps);
        }
    }

    None
}

fn next_progress_with_finish_bounce(current_progress: u8, direction: &mut i8) -> Option<u8> {
    if current_progress == FINISH_DISTANCE && *direction > 0 {
        *direction = -1;
    } else if current_progress == HOME_ENTRY_PROGRESS && *direction < 0 {
        *direction = 1;
    }

    if *direction > 0 {
        current_progress
            .checked_add(1)
            .filter(|next_progress| *next_progress <= FINISH_DISTANCE)
    } else {
        current_progress
            .checked_sub(1)
            .filter(|next_progress| *next_progress >= HOME_ENTRY_PROGRESS)
    }
}

/// 将“逻辑进度”映射成棋盘位置（主环道/冲线道/终点）。
pub fn board_position_for_distance(
    player_profile: &PlayerProfile,
    distance: u8,
    status: PieceStatus,
) -> Option<BoardPosition> {
    match status {
        PieceStatus::InHangar => None,
        PieceStatus::AtLaunch => Some(BoardPosition::Launch),
        PieceStatus::Finished => Some(BoardPosition::Goal),
        PieceStatus::Active if distance < HOME_ENTRY_PROGRESS => {
            let public_index = (public_index_for_route_index(player_profile.launch_tile_index)?
                + distance)
                % MAIN_ROUTE_STEPS;
            board_position_for_public_index(public_index)
        }
        PieceStatus::Active if distance == HOME_ENTRY_PROGRESS => {
            Some(BoardPosition::TurnMarker(player_profile.seat))
        }
        PieceStatus::Active if distance < FINISH_DISTANCE => {
            Some(BoardPosition::Home(distance - HOME_ENTRY_PROGRESS))
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
        BoardPosition::Launch => Some(player_profile.launch_position),
        BoardPosition::Main(tile_index) => board_layout.world_pos_for_route_index(tile_index),
        BoardPosition::TurnMarker(seat) => Some(turn_marker_position_for_seat(seat)),
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
            } => (piece_id, target_progress, PieceStatus::AtLaunch),
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
        piece_state.motion_serial = piece_state.motion_serial.wrapping_add(1);
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
            owner_player_id: piece_state.owner_player_id,
            status: previous_status,
            progress: previous_progress,
            translation: previous_translation,
            new_progress: target_progress,
        });
    }

    None
}

/// 飞跃结算：落在与当前棋子同色的普通主环道格时，触发免费同色飞跃。
/// 虚线快捷飞跃属于移动路径的一步，必须消耗骰点，不能在落地后免费触发。
fn apply_jump_effect(
    action: &PlannedAction,
    route_effects: MoveRouteEffects,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
    notes: &mut Vec<String>,
) {
    if board_layout.route_len() == 0 {
        return;
    }

    if route_effects.shortcut_used {
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

    if !is_same_color_route_tile(owner_player_id, tile_index, board_layout, player_roster) {
        return;
    }

    if attacker_has_enemy_on_current_tile(action, player_roster, piece_query) {
        return;
    }

    let Some(jump) = jump_resolution_for_same_color_landing(
        owner_player_id,
        tile_index,
        board_layout,
        player_roster,
    ) else {
        return;
    };

    update_piece_progress(
        action,
        jump.target_progress,
        board_layout,
        player_roster,
        piece_query,
    );
    notes.push(format!(
        "jumped to next same-color tile {}",
        jump.target_progress
    ));
}

fn attacker_has_enemy_on_current_tile(
    action: &PlannedAction,
    player_roster: &PlayerRoster,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> bool {
    let Some(target_position) = attacker_position(action, player_roster, piece_query) else {
        return false;
    };

    let attacker_piece_id = action.piece_id();
    let Some(attacker_team) = piece_query
        .iter()
        .find(|(piece_id, _, _, _)| piece_id.0 == attacker_piece_id)
        .map(|(_, _, piece_state, _)| piece_state.team_id)
    else {
        return false;
    };

    piece_query.iter().any(|(piece_id, _, piece_state, _)| {
        piece_id.0 != attacker_piece_id
            && piece_state.status == PieceStatus::Active
            && piece_state.team_id != attacker_team
            && piece_board_position(*piece_state, player_roster) == Some(target_position)
    })
}

fn move_route_effects_for_action(
    action: &PlannedAction,
    action_origin: Option<&ActionOrigin>,
    roll_value: u8,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
) -> MoveRouteEffects {
    let PlannedAction::Move {
        target_progress, ..
    } = *action
    else {
        return MoveRouteEffects::default();
    };
    let Some(origin) = action_origin else {
        return MoveRouteEffects::default();
    };

    let shortcut_used = movement_steps_for_roll(
        origin.owner_player_id,
        origin.status,
        origin.progress,
        roll_value,
        board_layout,
        player_roster,
    )
    .is_some_and(|steps| {
        steps
            .iter()
            .any(|step| step.kind == MovementStepKind::Shortcut)
            && steps
                .last()
                .is_some_and(|step| step.progress == target_progress)
    });

    MoveRouteEffects { shortcut_used }
}

fn jump_resolution_for_same_color_landing(
    player_id: u8,
    tile_index: u8,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
) -> Option<JumpResolution> {
    if !is_same_color_route_tile(player_id, tile_index, board_layout, player_roster) {
        return None;
    }

    let current_progress = progress_for_main_route_index(player_roster, player_id, tile_index)?;

    if shortcut_progress_after_landing(player_id, current_progress, board_layout, player_roster)
        .is_some()
    {
        return None;
    }

    next_same_color_jump_progress(player_id, current_progress, board_layout, player_roster)
        .map(|target_progress| JumpResolution { target_progress })
}

fn shortcut_progress_after_landing(
    player_id: u8,
    current_progress: u8,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
) -> Option<u8> {
    if current_progress >= HOME_ENTRY_PROGRESS {
        return None;
    }

    let tile_index = route_index_for_progress(player_roster, player_id, current_progress)?;
    if !is_same_color_route_tile(player_id, tile_index, board_layout, player_roster) {
        return None;
    }

    let shortcut_target_index = board_layout.jump_shortcut_target_for_route_index(tile_index)?;
    let shortcut_progress =
        progress_for_main_route_index(player_roster, player_id, shortcut_target_index)?;
    (shortcut_progress > current_progress && shortcut_progress < HOME_ENTRY_PROGRESS)
        .then_some(shortcut_progress)
}

/// 主环道当前格是否为该玩家的同色格。
fn is_same_color_route_tile(
    player_id: u8,
    tile_index: u8,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
) -> bool {
    let Some(player_color_slot) = player_seat_slot(player_id, player_roster) else {
        return false;
    };

    board_layout.player_color_slot_for_route_index(tile_index) == Some(player_color_slot)
}

fn player_seat_slot(player_id: u8, player_roster: &PlayerRoster) -> Option<usize> {
    player_roster
        .players
        .iter()
        .find(|player| player.state.player_id == player_id)
        .map(|player| player.seat.slot_index())
}

fn progress_for_main_route_index(
    player_roster: &PlayerRoster,
    player_id: u8,
    route_index: u8,
) -> Option<u8> {
    let player_profile = player_roster
        .players
        .iter()
        .find(|player| player.state.player_id == player_id)?;
    let launch_index = public_index_for_route_index(player_profile.launch_tile_index)?;
    let route_index = public_index_for_route_index(route_index)?;
    let progress = if route_index >= launch_index {
        route_index - launch_index
    } else {
        MAIN_ROUTE_STEPS - (launch_index - route_index)
    };
    (progress < HOME_ENTRY_PROGRESS).then_some(progress)
}

fn route_index_for_progress(
    player_roster: &PlayerRoster,
    player_id: u8,
    progress: u8,
) -> Option<u8> {
    let player_profile = player_roster
        .players
        .iter()
        .find(|player| player.state.player_id == player_id)?;
    if progress >= HOME_ENTRY_PROGRESS {
        return None;
    }
    let public_index = (public_index_for_route_index(player_profile.launch_tile_index)? + progress)
        % MAIN_ROUTE_STEPS;
    route_index_for_public_index(public_index)
}

fn board_position_for_public_index(public_index: u8) -> Option<BoardPosition> {
    match public_node_for_index(public_index)? {
        PublicRouteNode::Route(route_index) => Some(BoardPosition::Main(route_index)),
        PublicRouteNode::TurnMarker(seat) => Some(BoardPosition::TurnMarker(seat)),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PublicRouteNode {
    Route(u8),
    TurnMarker(PlayerSeat),
}

fn public_node_for_index(public_index: u8) -> Option<PublicRouteNode> {
    match public_index {
        0 => Some(PublicRouteNode::Route(0)),
        1 => Some(PublicRouteNode::TurnMarker(PlayerSeat::Red)),
        2..=13 => Some(PublicRouteNode::Route(public_index - 1)),
        14 => Some(PublicRouteNode::TurnMarker(PlayerSeat::Yellow)),
        15..=26 => Some(PublicRouteNode::Route(public_index - 2)),
        27 => Some(PublicRouteNode::TurnMarker(PlayerSeat::Green)),
        28..=39 => Some(PublicRouteNode::Route(public_index - 3)),
        40 => Some(PublicRouteNode::TurnMarker(PlayerSeat::Blue)),
        41..=51 => Some(PublicRouteNode::Route(public_index - 4)),
        _ => None,
    }
}

fn public_index_for_route_index(route_index: u8) -> Option<u8> {
    (route_index < BOARD_ROUTE_TILES).then(|| match route_index {
        0 => 0,
        1..=12 => route_index + 1,
        13..=24 => route_index + 2,
        25..=36 => route_index + 3,
        37..=47 => route_index + 4,
        _ => unreachable!(),
    })
}

fn route_index_for_public_index(public_index: u8) -> Option<u8> {
    match public_node_for_index(public_index)? {
        PublicRouteNode::Route(route_index) => Some(route_index),
        PublicRouteNode::TurnMarker(_) => None,
    }
}

/// 同色飞跃默认跳到玩家前进路径上的下一处同色节点。
/// 本方支路第 1 格是主环道后的同色拐点，也可以作为飞跃终点。
fn next_same_color_jump_progress(
    player_id: u8,
    current_progress: u8,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
) -> Option<u8> {
    for progress in current_progress.saturating_add(1)..HOME_ENTRY_PROGRESS {
        let Some(next_index) = route_index_for_progress(player_roster, player_id, progress) else {
            continue;
        };
        if is_same_color_route_tile(player_id, next_index, board_layout, player_roster) {
            return Some(progress);
        }
    }
    player_roster
        .players
        .iter()
        .find(|player| player.state.player_id == player_id)
        .filter(|player| {
            current_progress < HOME_ENTRY_PROGRESS && !player.home_lane_positions.is_empty()
        })
        .map(|_| HOME_ENTRY_PROGRESS)
}

/// 撞击结算后的落点效果（防御格、事件格等）。
fn apply_post_collision_tile_effects(
    action: &PlannedAction,
    resources: LandingResources<'_>,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
    skill_roster: &mut SkillRoster,
    notes: &mut Vec<String>,
    piece_effect_notice: &mut Option<PieceEffectNotice>,
) -> bool {
    if resources.board_layout.route_len() == 0 {
        return true;
    }

    let Some(BoardPosition::Main(tile_index)) =
        attacker_position(action, resources.player_roster, piece_query)
    else {
        return true;
    };
    let Some(tile_kind) = resources.board_layout.tile_kind_for_route_index(tile_index) else {
        return true;
    };

    let mut attacker_still_landed = match tile_kind {
        TileKind::Defense => {
            if let Some(shield) = modify_piece_shield(action, piece_query, 1) {
                notes.push(format!("gained shield ({shield})"));
                *piece_effect_notice = Some(PieceEffectNotice {
                    piece_id: action.piece_id(),
                    kind: PieceEffectKind::Defense,
                });
            }
            true
        }
        TileKind::Event => {
            let mut final_progress = attacker_progress(action, piece_query).unwrap_or_default();
            let Some(event_outcome) = apply_event_effect(
                action,
                &mut final_progress,
                resources,
                piece_query,
                skill_roster,
                notes,
            ) else {
                return true;
            };
            notes.push(event_outcome.note);
            event_outcome.attacker_still_landed
        }
        TileKind::Attack | TileKind::Goal | TileKind::Jump | TileKind::Normal => true,
    };

    if attacker_still_landed
        && let Some(source_tile_index) = resources.jump_source_event_tile
        && source_tile_index != tile_index
    {
        let mut final_progress = attacker_progress(action, piece_query).unwrap_or_default();
        if let Some(event_outcome) = apply_event_effect(
            action,
            &mut final_progress,
            resources,
            piece_query,
            skill_roster,
            notes,
        ) {
            notes.push(format!(
                "pre-jump event tile {source_tile_index}: {}",
                event_outcome.note
            ));
            attacker_still_landed = event_outcome.attacker_still_landed;
        }
    }

    attacker_still_landed
}

fn jump_source_event_tile(
    pre_jump_position: Option<BoardPosition>,
    post_jump_position: Option<BoardPosition>,
    board_layout: &BoardLayout,
) -> Option<u8> {
    let Some(BoardPosition::Main(source_tile_index)) = pre_jump_position else {
        return None;
    };

    if post_jump_position == Some(BoardPosition::Main(source_tile_index)) {
        return None;
    }

    (board_layout.tile_kind_for_route_index(source_tile_index) == Some(TileKind::Event))
        .then_some(source_tile_index)
}

/// 撞击主逻辑：
/// - 攻击格清场优先，穿透共享护盾与单体护盾；
/// - 再判定普通格叠防；
/// - 再判定共享护盾；
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
    let target_tile_index = match attacker_board_position {
        BoardPosition::Main(tile_index) => Some(tile_index),
        BoardPosition::TurnMarker(_) => None,
        BoardPosition::Launch | BoardPosition::Home(_) | BoardPosition::Goal => return true,
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

    let defender_piece_ids = piece_query
        .iter()
        .filter_map(|(piece_id, _, piece_state, _)| {
            (piece_id.0 != attacker_piece_id
                && piece_state.status == PieceStatus::Active
                && piece_state.team_id != attacker_team
                && piece_board_position(*piece_state, player_roster)
                    == Some(attacker_board_position))
            .then_some(piece_id.0)
        })
        .collect::<Vec<_>>();

    if target_tile_index.is_some_and(|tile_index| is_attack_route_tile(board_layout, tile_index))
        && !defender_piece_ids.is_empty()
    {
        let target_tile_index = target_tile_index.expect("attack tile has a route index");
        clear_attack_tile_defenders(
            attacker_piece_id,
            attacker_team,
            target_tile_index,
            player_roster,
            piece_query,
            notes,
        );
        append_attack_tile_collision_note(board_layout, target_tile_index, notes);
        return true;
    }

    if is_plain_board_position(board_layout, attacker_board_position)
        && defender_piece_ids.len() >= 2
    {
        send_attacker_to_hangar(action, piece_query);
        notes.push("stacked defenders bounced attacker back to hangar".to_string());
        return false;
    }

    let mut defenders_with_stack = Vec::new();
    for (piece_id, _, piece_state, _) in piece_query.iter_mut() {
        if piece_id.0 == attacker_piece_id {
            continue;
        }

        if piece_state.status != PieceStatus::Active
            || piece_state.team_id == attacker_team
            || piece_board_position(*piece_state, player_roster) != Some(attacker_board_position)
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
        if let Some(target_tile_index) = target_tile_index {
            append_attack_tile_collision_note(board_layout, target_tile_index, notes);
        }
        return false;
    }

    let mut collision_blocked = false;
    for (piece_id, hangar_slot, mut piece_state, mut transform) in piece_query.iter_mut() {
        if piece_id.0 == attacker_piece_id {
            continue;
        }

        if piece_state.status != PieceStatus::Active
            || piece_state.team_id == attacker_team
            || piece_board_position(*piece_state, player_roster) != Some(attacker_board_position)
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
        if let Some(target_tile_index) = target_tile_index {
            append_attack_tile_collision_note(board_layout, target_tile_index, notes);
        }
        return false;
    } else if notes
        .iter()
        .any(|note| note.contains("sent piece #") && note.contains("back to hangar"))
    {
        if let Some(target_tile_index) = target_tile_index {
            append_attack_tile_collision_note(board_layout, target_tile_index, notes);
        }
    }
    true
}

fn is_plain_board_position(board_layout: &BoardLayout, target_position: BoardPosition) -> bool {
    match target_position {
        BoardPosition::Main(tile_index) => is_plain_route_tile(board_layout, tile_index),
        BoardPosition::TurnMarker(_) => true,
        BoardPosition::Launch | BoardPosition::Home(_) | BoardPosition::Goal => false,
    }
}

/// 普通主环道格：不含攻击、防御、随机、飞跃等特殊效果。
fn is_plain_route_tile(board_layout: &BoardLayout, target_tile_index: u8) -> bool {
    board_layout.tile_kind_for_route_index(target_tile_index) == Some(TileKind::Normal)
}

fn is_attack_route_tile(board_layout: &BoardLayout, target_tile_index: u8) -> bool {
    board_layout.tile_kind_for_route_index(target_tile_index) == Some(TileKind::Attack)
}

/// 攻击格撞击会穿透护盾，将目标格上的所有敌方棋子送回机库。
fn clear_attack_tile_defenders(
    attacker_piece_id: u8,
    attacker_team: u8,
    target_tile_index: u8,
    player_roster: &PlayerRoster,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
    notes: &mut Vec<String>,
) {
    for (piece_id, hangar_slot, mut piece_state, mut transform) in piece_query.iter_mut() {
        if piece_id.0 == attacker_piece_id
            || piece_state.status != PieceStatus::Active
            || piece_state.team_id == attacker_team
            || piece_board_position(*piece_state, player_roster)
                != Some(BoardPosition::Main(target_tile_index))
        {
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

/// 普通格撞到重叠防御时，攻击方直接回到自身机库。
fn send_attacker_to_hangar(
    action: &PlannedAction,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) {
    for (piece_id, hangar_slot, mut piece_state, mut transform) in piece_query.iter_mut() {
        if piece_id.0 != action.piece_id() {
            continue;
        }

        piece_state.status = PieceStatus::InHangar;
        piece_state.progress = 0;
        piece_state.shield = 0;
        piece_state.stack_shield = 0;
        transform.translation.x = hangar_slot.0.x;
        transform.translation.y = hangar_slot.0.y;
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
    resources: LandingResources<'_>,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
    skill_roster: &mut SkillRoster,
    notes: &mut Vec<String>,
) -> Option<TileEventOutcome> {
    apply_event_kind_effect(
        random_event_kind(),
        action,
        final_progress,
        resources,
        piece_query,
        skill_roster,
        notes,
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
    resources: LandingResources<'_>,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
    skill_roster: &mut SkillRoster,
    notes: &mut Vec<String>,
) -> Option<TileEventOutcome> {
    match event_kind {
        TileEventKind::GainShield => {
            let shield = modify_piece_shield(action, piece_query, 1)?;
            Some(TileEventOutcome {
                kind: event_kind,
                note: format!("event {event_kind:?}: gained shield ({shield})"),
                attacker_still_landed: true,
            })
        }
        TileEventKind::GainSkillCharge => {
            let owner_player_id = owner_player_id_for_action(action, piece_query)?;
            let allow_swap = resources.player_roster.players.len() > 2;
            let charged = grant_random_skill_charge(skill_roster, owner_player_id, allow_swap)
                .unwrap_or("UnknownSkill");
            Some(TileEventOutcome {
                kind: event_kind,
                note: format!("event {event_kind:?}: gained 1 {charged} charge"),
                attacker_still_landed: true,
            })
        }
        TileEventKind::AdvanceTwo => {
            let next_progress = (*final_progress + 2).min(FINISH_DISTANCE);
            let action_origin =
                snapshot_action_origin(action, piece_query).map(|origin| ActionOrigin {
                    new_progress: next_progress,
                    ..origin
                });
            *final_progress = next_progress;
            update_piece_progress(
                action,
                next_progress,
                resources.board_layout,
                resources.player_roster,
                piece_query,
            );
            let attacker_still_landed = resolve_collision(
                action,
                action_origin,
                resources.board_layout,
                resources.player_roster,
                resources.match_config,
                piece_query,
                notes,
            );
            Some(TileEventOutcome {
                kind: event_kind,
                note: "event advance +2".to_string(),
                attacker_still_landed,
            })
        }
        TileEventKind::DisableNextSkill => {
            let owner_player_id = owner_player_id_for_action(action, piece_query)?;
            if disable_next_skill_turn(skill_roster, owner_player_id) {
                Some(TileEventOutcome {
                    kind: event_kind,
                    note: format!(
                        "event {event_kind:?}: next skill turn disabled for P{owner_player_id}"
                    ),
                    attacker_still_landed: true,
                })
            } else {
                Some(TileEventOutcome {
                    kind: event_kind,
                    note: "event fizzled: could not disable next skill turn".to_string(),
                    attacker_still_landed: true,
                })
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
                return Some(TileEventOutcome {
                    kind: event_kind,
                    note: "event fizzled: no enemy shield to remove".to_string(),
                    attacker_still_landed: true,
                });
            }

            let picked_piece_id = candidates[random_range(0..candidates.len())];
            for (piece_id, _, mut piece_state, _) in piece_query.iter_mut() {
                if piece_id.0 != picked_piece_id {
                    continue;
                }
                piece_state.shield -= 1;
                return Some(TileEventOutcome {
                    kind: event_kind,
                    note: format!(
                        "event {event_kind:?}: removed shield from piece #{}",
                        piece_id.0
                    ),
                    attacker_still_landed: true,
                });
            }
            Some(TileEventOutcome {
                kind: event_kind,
                note: "event failed: selected enemy shield target disappeared".to_string(),
                attacker_still_landed: true,
            })
        }
    }
}

/// Capture the moving piece before a secondary movement, so shield blocks can
/// return it to the pre-secondary-movement position.
fn snapshot_action_origin(
    action: &PlannedAction,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> Option<ActionOrigin> {
    for (piece_id, _, piece_state, transform) in piece_query.iter_mut() {
        if piece_id.0 != action.piece_id() {
            continue;
        }
        return Some(ActionOrigin {
            owner_player_id: piece_state.owner_player_id,
            status: piece_state.status,
            progress: piece_state.progress,
            translation: transform.translation,
            new_progress: piece_state.progress,
        });
    }
    None
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
    turn_state.current_roll_faces = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::player::{PlayerControl, PlayerState};
    use crate::gameplay::ai::AiDifficulty;
    use crate::gameplay::match_flow::{MatchSetup, PlayerSeat, build_match_rosters};
    use crate::gameplay::skill_flow::{
        build_skill_roster, can_use_skill_this_turn, sync_turn_skill_usage,
    };
    use bevy::ecs::system::SystemState;

    fn setup(mode: GameMode) -> MatchSetup {
        MatchSetup {
            mode,
            ai_difficulty: AiDifficulty::Normal,
            fast_mode: false,
            launch_rule: LaunchRule::SixOnly,
            player_seats: [
                PlayerSeat::Blue,
                PlayerSeat::Red,
                PlayerSeat::Green,
                PlayerSeat::Yellow,
            ],
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
    fn turn_input_state_exposes_and_clears_pending_actions() {
        let mut input_state = TurnInputState {
            pending_actions: vec![PlannedAction::Move {
                piece_id: 1,
                target_progress: 4,
            }],
            candidate_piece_ids: vec![1],
            prompt: Some("choose".to_string()),
        };

        assert_eq!(
            input_state.pending_actions(),
            &[PlannedAction::Move {
                piece_id: 1,
                target_progress: 4
            }]
        );

        clear_pending_input(&mut input_state);

        assert!(input_state.pending_actions().is_empty());
        assert!(input_state.candidate_piece_ids().is_empty());
        assert_eq!(input_state.prompt, None);
    }

    fn match_config(mode: GameMode) -> MatchConfig {
        MatchConfig {
            mode,
            ai_difficulty: AiDifficulty::Normal,
            fast_mode: false,
            launch_rule: LaunchRule::SixOnly,
            player_seats: [
                PlayerSeat::Blue,
                PlayerSeat::Red,
                PlayerSeat::Green,
                PlayerSeat::Yellow,
            ],
            pieces_per_player: 2,
            player_controls: [
                PlayerControl::Human,
                PlayerControl::Ai,
                PlayerControl::Human,
                PlayerControl::Ai,
            ],
        }
    }

    fn match_config_from_setup(setup: &MatchSetup) -> MatchConfig {
        MatchConfig {
            mode: setup.mode,
            ai_difficulty: setup.ai_difficulty,
            fast_mode: setup.fast_mode,
            launch_rule: setup.launch_rule,
            player_seats: setup.normalized_player_seats(),
            pieces_per_player: setup.pieces_per_player,
            player_controls: setup.normalized_player_controls(),
        }
    }

    fn spawn_test_pieces(world: &mut World, player_roster: &PlayerRoster) {
        let mut piece_id = 1;
        for player in &player_roster.players {
            for &hangar_slot in &player.hangar_slots {
                world.spawn((
                    PieceId(piece_id),
                    HangarSlot(hangar_slot),
                    PieceState {
                        owner_player_id: player.state.player_id,
                        team_id: player.state.team_id,
                        status: PieceStatus::InHangar,
                        progress: 0,
                        shield: 0,
                        stack_shield: 0,
                        motion_serial: 0,
                    },
                    Transform::from_xyz(hangar_slot.x, hangar_slot.y, 0.0),
                ));
                piece_id += 1;
            }
        }
    }

    fn simulated_action_score(
        action: PlannedAction,
        piece_query: &Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
    ) -> i32 {
        match action {
            PlannedAction::Launch { .. } => 0,
            PlannedAction::Move {
                piece_id,
                target_progress,
            } => {
                let current_progress = piece_query
                    .iter()
                    .find(|(id, _, _, _)| id.0 == piece_id)
                    .map(|(_, _, piece_state, _)| piece_state.progress)
                    .unwrap_or_default();
                let finish_bonus = if target_progress == FINISH_DISTANCE {
                    10_000
                } else {
                    0
                };
                let forward_bonus = i32::from(target_progress.saturating_sub(current_progress));
                finish_bonus + 1_000 + i32::from(target_progress) * 10 + forward_bonus
            }
        }
    }

    fn choose_simulated_turn_action(
        turn_state: &TurnState,
        match_config: &MatchConfig,
        board_layout: &BoardLayout,
        player_roster: &PlayerRoster,
        piece_query: &Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
    ) -> Option<(u8, PlannedAction)> {
        let mut best: Option<(i32, u8, PlannedAction)> = None;

        for roll_value in 1..=6 {
            let actions = collect_actions(
                turn_state.current_player,
                DiceRoll(roll_value),
                0,
                match_config.launch_rule,
                board_layout,
                player_roster,
                piece_query,
            );

            for action in actions {
                let score = simulated_action_score(action, piece_query);
                if best
                    .as_ref()
                    .is_none_or(|(best_score, _, _)| score > *best_score)
                {
                    best = Some((score, roll_value, action));
                }
            }
        }

        best.map(|(_, roll_value, action)| (roll_value, action))
    }

    fn snapshot_pieces(
        piece_query: &Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
    ) -> Vec<(u8, u8, PieceStatus, u8)> {
        let mut pieces = piece_query
            .iter()
            .map(|(piece_id, _, piece_state, _)| {
                (
                    piece_id.0,
                    piece_state.owner_player_id,
                    piece_state.status,
                    piece_state.progress,
                )
            })
            .collect::<Vec<_>>();
        pieces.sort_by_key(|(piece_id, _, _, _)| *piece_id);
        pieces
    }

    fn simulate_match_to_result(setup: MatchSetup, max_turns: usize) -> MatchResult {
        let match_config = match_config_from_setup(&setup);
        let board_layout = BoardLayout::default();
        let (players, teams) = build_match_rosters(&setup);
        let player_roster = PlayerRoster::from_players(players);
        let team_roster = TeamRoster { teams };
        let mut skill_roster = build_skill_roster(&player_roster);
        let mut match_result = MatchResult::default();
        let mut turn_state = TurnState::opening_turn();
        let mut input_state = TurnInputState::default();
        let mut next_phase = NextState::<GamePhase>::default();
        let mut world = World::new();
        spawn_test_pieces(&mut world, &player_roster);
        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut turns_taken = 0;

        while !match_result.finished && turns_taken < max_turns {
            turns_taken += 1;
            let mut query = system_state.get_mut(&mut world).unwrap();

            if let Some((roll_value, action)) = choose_simulated_turn_action(
                &turn_state,
                &match_config,
                &board_layout,
                &player_roster,
                &query,
            ) {
                set_roll(&mut turn_state, roll_value);
                execute_action(
                    action,
                    roll_value,
                    roll_value,
                    ActionResources {
                        player_roster: &player_roster,
                        team_roster: &team_roster,
                        match_config: &match_config,
                        board_layout: &board_layout,
                    },
                    ActionState {
                        skill_roster: &mut skill_roster,
                        match_result: &mut match_result,
                        turn_state: &mut turn_state,
                        input_state: &mut input_state,
                        next_phase: &mut next_phase,
                    },
                    &mut query,
                );
            } else {
                clear_pending_input(&mut input_state);
                advance_turn(&mut turn_state, player_roster.players.len() as u8);
                next_phase.set(GamePhase::AwaitDice);
            }
        }

        if !match_result.finished {
            let query = system_state.get_mut(&mut world).unwrap();
            panic!(
                "simulated match did not finish in {max_turns} turns; mode={:?}, launch_rule={:?}, pieces={}, seats={:?}, current_player={}, last_action={:?}, pieces={:?}",
                setup.mode,
                setup.launch_rule,
                setup.pieces_per_player,
                setup.player_seats,
                turn_state.current_player,
                turn_state.last_action,
                snapshot_pieces(&query)
            );
        }

        match_result
    }

    fn progress_for_main_tile(player_roster: &PlayerRoster, player_id: u8, route_index: u8) -> u8 {
        progress_for_main_route_index(player_roster, player_id, route_index)
            .expect("route tile is reachable before this player's home lane")
    }

    fn seat_layouts_for_simulation() -> Vec<[PlayerSeat; 4]> {
        vec![
            [
                PlayerSeat::Blue,
                PlayerSeat::Red,
                PlayerSeat::Green,
                PlayerSeat::Yellow,
            ],
            [
                PlayerSeat::Red,
                PlayerSeat::Blue,
                PlayerSeat::Green,
                PlayerSeat::Yellow,
            ],
            [
                PlayerSeat::Yellow,
                PlayerSeat::Green,
                PlayerSeat::Blue,
                PlayerSeat::Red,
            ],
            [
                PlayerSeat::Green,
                PlayerSeat::Yellow,
                PlayerSeat::Red,
                PlayerSeat::Blue,
            ],
        ]
    }

    fn control_layouts_for_simulation() -> Vec<[PlayerControl; 4]> {
        vec![
            [
                PlayerControl::Human,
                PlayerControl::Ai,
                PlayerControl::Human,
                PlayerControl::Ai,
            ],
            [
                PlayerControl::Ai,
                PlayerControl::Human,
                PlayerControl::Ai,
                PlayerControl::Human,
            ],
        ]
    }

    #[test]
    fn simulated_matches_finish_for_varied_play_configs() {
        let mut scenario_count = 0;

        for mode in GameMode::ALL {
            for launch_rule in LaunchRule::ALL {
                for pieces_per_player in 1..=4 {
                    for player_seats in seat_layouts_for_simulation() {
                        for player_controls in control_layouts_for_simulation() {
                            let mut setup = setup(mode);
                            setup.launch_rule = launch_rule;
                            setup.pieces_per_player = pieces_per_player;
                            setup.player_seats = player_seats;
                            setup.player_controls = player_controls;
                            scenario_count += 1;

                            let result = simulate_match_to_result(setup.clone(), 5_000);
                            assert!(result.finished);
                            assert!(result.winner_team_id.is_some());
                            assert!(
                                !result.winner_player_ids.is_empty(),
                                "winner players should be reported for mode={:?}, launch_rule={:?}, pieces={}, seats={:?}, controls={:?}",
                                setup.mode,
                                setup.launch_rule,
                                setup.pieces_per_player,
                                setup.player_seats,
                                setup.player_controls,
                            );
                        }
                    }
                }
            }
        }

        assert_eq!(
            scenario_count,
            GameMode::ALL.len()
                * LaunchRule::ALL.len()
                * 4
                * seat_layouts_for_simulation().len()
                * control_layouts_for_simulation().len()
        );
    }

    #[test]
    fn simulated_matches_cover_representative_edge_configs() {
        let mut edge_configs = Vec::new();
        let mut one_vs_one_both_human = setup(GameMode::OneVsOne);
        one_vs_one_both_human.launch_rule = LaunchRule::Even;
        one_vs_one_both_human.pieces_per_player = 4;
        one_vs_one_both_human.player_seats = [
            PlayerSeat::Yellow,
            PlayerSeat::Green,
            PlayerSeat::Blue,
            PlayerSeat::Red,
        ];
        one_vs_one_both_human.player_controls = [
            PlayerControl::Human,
            PlayerControl::Human,
            PlayerControl::Ai,
            PlayerControl::Ai,
        ];
        edge_configs.push(one_vs_one_both_human);

        let mut two_vs_two_fast_mode = setup(GameMode::TwoVsTwo);
        two_vs_two_fast_mode.launch_rule = LaunchRule::Even;
        two_vs_two_fast_mode.pieces_per_player = 4;
        two_vs_two_fast_mode.fast_mode = true;
        two_vs_two_fast_mode.player_controls = [
            PlayerControl::Human,
            PlayerControl::Ai,
            PlayerControl::Ai,
            PlayerControl::Human,
        ];
        edge_configs.push(two_vs_two_fast_mode);

        let mut free_for_all_single_piece = setup(GameMode::FreeForAll);
        free_for_all_single_piece.launch_rule = LaunchRule::SixOnly;
        free_for_all_single_piece.pieces_per_player = 1;
        free_for_all_single_piece.player_seats = [
            PlayerSeat::Green,
            PlayerSeat::Blue,
            PlayerSeat::Yellow,
            PlayerSeat::Red,
        ];
        edge_configs.push(free_for_all_single_piece);

        for setup in edge_configs {
            let result = simulate_match_to_result(setup.clone(), 5_000);
            assert!(result.finished);
            assert!(result.winner_team_id.is_some());
            assert!(
                !result.winner_player_ids.is_empty(),
                "winner players should be reported for {:?}",
                setup.mode
            );
        }
    }

    #[test]
    fn compute_target_distance_bounces_from_finish_on_overshoot() {
        assert_eq!(
            compute_target_distance(FINISH_DISTANCE - 2, 2),
            Some(FINISH_DISTANCE)
        );
        assert_eq!(
            compute_target_distance(FINISH_DISTANCE - 2, 3),
            Some(FINISH_DISTANCE - 1)
        );
        assert_eq!(
            compute_target_distance(FINISH_DISTANCE - 1, 2),
            Some(FINISH_DISTANCE - 1)
        );
    }

    #[test]
    fn launch_point_moves_to_first_main_tile_on_one() {
        assert_eq!(
            compute_move_target_distance(PieceStatus::AtLaunch, 0, 1),
            Some(0)
        );
        assert_eq!(
            compute_move_target_distance(PieceStatus::AtLaunch, 0, 6),
            Some(5)
        );
    }

    #[test]
    fn home_lane_overshoot_remains_legal_and_bounces_back() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::OneVsOne));
        let board_layout = BoardLayout::default();
        let player_roster = PlayerRoster::from_players(players);
        let start_progress = FINISH_DISTANCE - 2;

        assert_eq!(
            movement_steps_for_roll(
                1,
                PieceStatus::Active,
                start_progress,
                3,
                &board_layout,
                &player_roster
            ),
            Some(vec![
                MovementStep {
                    progress: FINISH_DISTANCE - 1,
                    kind: MovementStepKind::Normal,
                },
                MovementStep {
                    progress: FINISH_DISTANCE,
                    kind: MovementStepKind::Normal,
                },
                MovementStep {
                    progress: FINISH_DISTANCE - 1,
                    kind: MovementStepKind::Normal,
                },
            ])
        );
        assert_eq!(
            compute_move_target_distance_on_board(
                1,
                PieceStatus::Active,
                start_progress,
                3,
                &board_layout,
                &player_roster
            ),
            Some(FINISH_DISTANCE - 1)
        );
    }

    #[test]
    fn board_position_uses_player_launch_offset_on_main_route() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::TwoVsTwo));
        let player_one = &players[0];
        let player_two = &players[1];

        assert_eq!(
            board_position_for_distance(player_one, 0, PieceStatus::AtLaunch),
            Some(BoardPosition::Launch)
        );
        assert_eq!(
            board_position_for_distance(player_one, 0, PieceStatus::Active),
            Some(BoardPosition::Main(39))
        );
        assert_eq!(
            board_position_for_distance(player_two, 0, PieceStatus::Active),
            Some(BoardPosition::Main(3))
        );
        assert_eq!(
            board_position_for_distance(player_one, HOME_ENTRY_PROGRESS - 1, PieceStatus::Active),
            Some(BoardPosition::Main(36))
        );
        assert_eq!(
            board_position_for_distance(player_one, HOME_ENTRY_PROGRESS, PieceStatus::Active),
            Some(BoardPosition::TurnMarker(PlayerSeat::Blue))
        );
        assert_eq!(
            board_position_for_distance(player_one, HOME_ENTRY_PROGRESS + 1, PieceStatus::Active),
            Some(BoardPosition::Home(1))
        );
        assert_eq!(
            board_position_for_distance(player_one, FINISH_DISTANCE, PieceStatus::Finished),
            Some(BoardPosition::Goal)
        );
    }

    #[test]
    fn home_lane_entry_starts_at_turn_marker_for_each_player() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::TwoVsTwo));
        let board_layout = BoardLayout::default();

        for (player, expected_last_main, expected_home) in [
            (&players[0], 36, Vec2::new(-300.104, -0.104)),
            (&players[1], 0, Vec2::new(-0.104, 300.104)),
            (&players[2], 24, Vec2::new(0.104, -300.104)),
            (&players[3], 12, Vec2::new(300.317, 0.104)),
        ] {
            assert_eq!(
                board_position_for_distance(player, HOME_ENTRY_PROGRESS - 1, PieceStatus::Active),
                Some(BoardPosition::Main(expected_last_main))
            );
            assert_eq!(
                board_position_for_distance(player, HOME_ENTRY_PROGRESS, PieceStatus::Active),
                Some(BoardPosition::TurnMarker(player.seat))
            );
            assert_eq!(
                player
                    .home_lane_positions
                    .first()
                    .copied()
                    .expect("home lane entry exists"),
                expected_home
            );
            assert_eq!(
                board_layout.world_pos_for_route_index(expected_last_main),
                match expected_last_main {
                    36 => Some(Vec2::new(-300.104, -40.104)),
                    0 => Some(Vec2::new(-40.104, 300.104)),
                    24 => Some(Vec2::new(40.104, -300.104)),
                    12 => Some(Vec2::new(300.317, 40.104)),
                    _ => None,
                }
            );
        }
    }

    #[test]
    fn advance_turn_consumes_extra_roll_before_switching_player() {
        let mut turn_state = TurnState {
            current_player: 1,
            extra_rolls_remaining: 1,
            consecutive_sixes: 1,
            turn_index: 3,
            roll_serial: 1,
            current_roll: Some(6),
            current_roll_faces: Some([6, 0]),
            last_roll: Some(6),
            last_roll_faces: Some([6, 0]),
            last_roll_player: Some(1),
            hold_last_roll_display: false,
            roll_display_animation_started: false,
            player_last_rolls: [Some(6), None, None, None],
            player_last_roll_faces: [Some([6, 0]), None, None, None],
            pending_roll_display: None,
            last_piece_effect: None,
            last_action: None,
            last_action_player_id: None,
            last_action_turn_index: 0,
            last_action_serial: 0,
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
        turn_state.last_piece_effect = Some(PieceEffectNotice {
            piece_id: 1,
            kind: PieceEffectKind::Defense,
        });

        set_roll(&mut turn_state, 6);
        assert_eq!(turn_state.extra_rolls_remaining, 1);
        assert_eq!(turn_state.consecutive_sixes, 1);
        assert_eq!(turn_state.roll_serial, 1);
        assert_eq!(turn_state.current_roll_faces, Some([6, 0]));
        assert_eq!(turn_state.player_last_roll(1), None);
        assert_eq!(
            turn_state.pending_roll_display,
            Some(PendingRollDisplay {
                roll_serial: 1,
                player_id: 1,
                roll: 6,
                faces: [6, 0],
            })
        );
        assert!(commit_pending_roll_display(&mut turn_state, 1));
        assert_eq!(turn_state.player_last_roll(1), Some(6));
        assert_eq!(turn_state.player_last_roll_faces(1), Some([6, 0]));
        assert_eq!(turn_state.last_piece_effect, None);

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
    fn world_position_for_piece_uses_first_route_home_lane_and_goal_positions() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::TwoVsTwo));
        let player_roster = PlayerRoster::from_players(players);
        let board_layout = BoardLayout {
            tiles: crate::data::board_config::default_board_tiles(),
        };

        assert_eq!(
            world_position_for_piece(1, 0, PieceStatus::AtLaunch, &board_layout, &player_roster),
            Some(Vec2::new(-316.104, 156.104))
        );
        assert_eq!(
            world_position_for_piece(1, 0, PieceStatus::Active, &board_layout, &player_roster),
            board_layout.world_pos_for_route_index(39)
        );
        assert_eq!(
            world_position_for_piece(2, 0, PieceStatus::Active, &board_layout, &player_roster),
            board_layout.world_pos_for_route_index(3)
        );
        assert_eq!(
            world_position_for_piece(3, 0, PieceStatus::Active, &board_layout, &player_roster),
            board_layout.world_pos_for_route_index(27)
        );
        assert_eq!(
            world_position_for_piece(4, 0, PieceStatus::Active, &board_layout, &player_roster),
            board_layout.world_pos_for_route_index(15)
        );
        assert_eq!(
            world_position_for_piece(
                1,
                HOME_ENTRY_PROGRESS,
                PieceStatus::Active,
                &board_layout,
                &player_roster
            ),
            Some(Vec2::new(-300.104, -0.104))
        );
        assert_eq!(
            world_position_for_piece(
                1,
                FINISH_DISTANCE,
                PieceStatus::Finished,
                &board_layout,
                &player_roster
            ),
            Some(Vec2::new(-35.958, 0.0))
        );
    }

    #[test]
    fn launch_action_places_piece_on_launch_point_not_main_route() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::OneVsOne));
        let board_layout = BoardLayout::default();
        let player_roster = PlayerRoster::from_players(players);

        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: PieceStatus::InHangar,
                progress: 0,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world).unwrap();

        apply_action(
            &PlannedAction::Launch {
                piece_id: 1,
                target_progress: 0,
            },
            &board_layout,
            &player_roster,
            &mut query,
        );

        let (_, _, piece_state, transform) = query.iter_mut().next().unwrap();
        assert_eq!(piece_state.status, PieceStatus::AtLaunch);
        assert_eq!(piece_state.progress, 0);
        assert_eq!(
            transform.translation.truncate(),
            Vec2::new(-316.104, 156.104)
        );
    }

    #[test]
    fn same_color_jump_advances_to_next_same_color_node() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::OneVsOne));
        let board_layout = BoardLayout::default();
        let player_roster = PlayerRoster::from_players(players);
        let start_progress = progress_for_main_tile(&player_roster, 1, 40);
        let expected_progress = progress_for_main_tile(&player_roster, 1, 44);

        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: PieceStatus::Active,
                progress: start_progress,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world).unwrap();
        let mut notes = Vec::new();

        apply_jump_effect(
            &PlannedAction::Move {
                piece_id: 1,
                target_progress: start_progress,
            },
            MoveRouteEffects::default(),
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
        assert_eq!(progress, expected_progress);
        assert!(
            notes
                .iter()
                .any(|note| note.contains("next same-color tile"))
        );
    }

    #[test]
    fn same_color_jump_is_blocked_by_enemy_on_landing_tile() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::OneVsOne));
        let board_layout = BoardLayout::default();
        let player_roster = PlayerRoster::from_players(players);
        let match_config = match_config(GameMode::OneVsOne);
        let landing_tile = 40;
        let p1_landing_progress = progress_for_main_tile(&player_roster, 1, landing_tile);
        let p2_landing_progress = progress_for_main_tile(&player_roster, 2, landing_tile);

        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: PieceStatus::Active,
                progress: p1_landing_progress,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::default(),
        ));
        world.spawn((
            PieceId(2),
            HangarSlot(Vec2::new(30.0, -30.0)),
            PieceState {
                owner_player_id: 2,
                team_id: 2,
                status: PieceStatus::Active,
                progress: p2_landing_progress,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world).unwrap();
        let mut notes = Vec::new();
        let action = PlannedAction::Move {
            piece_id: 1,
            target_progress: p1_landing_progress,
        };

        apply_jump_effect(
            &action,
            MoveRouteEffects::default(),
            &board_layout,
            &player_roster,
            &mut query,
            &mut notes,
        );

        let p1_progress = query
            .iter_mut()
            .find(|(piece_id, _, _, _)| piece_id.0 == 1)
            .map(|(_, _, piece_state, _)| piece_state.progress)
            .unwrap_or_default();
        assert_eq!(p1_progress, p1_landing_progress);
        assert!(
            !notes
                .iter()
                .any(|note| note.contains("next same-color tile"))
        );

        assert!(resolve_collision(
            &action,
            None,
            &board_layout,
            &player_roster,
            &match_config,
            &mut query,
            &mut notes,
        ));

        let p2_status = query
            .iter_mut()
            .find(|(piece_id, _, _, _)| piece_id.0 == 2)
            .map(|(_, _, piece_state, _)| piece_state.status)
            .unwrap();
        assert_eq!(p2_status, PieceStatus::InHangar);
        assert!(
            notes
                .iter()
                .any(|note| note.contains("sent piece #2 back to hangar"))
        );
    }

    #[test]
    fn same_color_jump_can_land_on_home_lane_turn_marker() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::OneVsOne));
        let board_layout = BoardLayout::default();
        let player_roster = PlayerRoster::from_players(players);
        let start_progress = progress_for_main_tile(&player_roster, 1, 33);

        assert_eq!(
            next_same_color_jump_progress(1, start_progress, &board_layout, &player_roster),
            Some(HOME_ENTRY_PROGRESS)
        );

        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: PieceStatus::Active,
                progress: start_progress,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world).unwrap();
        let mut notes = Vec::new();

        apply_jump_effect(
            &PlannedAction::Move {
                piece_id: 1,
                target_progress: start_progress,
            },
            MoveRouteEffects::default(),
            &board_layout,
            &player_roster,
            &mut query,
            &mut notes,
        );

        let piece_state = query
            .iter_mut()
            .find(|(piece_id, _, _, _)| piece_id.0 == 1)
            .map(|(_, _, piece_state, _)| *piece_state)
            .expect("piece exists");
        assert_eq!(piece_state.progress, HOME_ENTRY_PROGRESS);
        assert_eq!(
            piece_board_position(piece_state, &player_roster),
            Some(BoardPosition::TurnMarker(PlayerSeat::Blue))
        );
        assert!(
            notes
                .iter()
                .any(|note| note.contains("next same-color tile"))
        );
    }

    #[test]
    fn same_color_event_tile_is_kept_as_pre_jump_event_source() {
        let board_layout = BoardLayout::default();

        assert_eq!(
            jump_source_event_tile(
                Some(BoardPosition::Main(0)),
                Some(BoardPosition::Main(3)),
                &board_layout,
            ),
            Some(0)
        );
        assert_eq!(
            jump_source_event_tile(
                Some(BoardPosition::Main(0)),
                Some(BoardPosition::Main(0)),
                &board_layout,
            ),
            None
        );
        assert_eq!(
            jump_source_event_tile(
                Some(BoardPosition::Main(44)),
                Some(BoardPosition::Main(0)),
                &board_layout,
            ),
            None
        );
    }

    #[test]
    fn same_color_jump_does_not_chain_from_landing_tile() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::OneVsOne));
        let player_roster = PlayerRoster::from_players(players);
        let board_layout = BoardLayout::default();
        let start_progress = progress_for_main_tile(&player_roster, 1, 40);
        let expected_progress = progress_for_main_tile(&player_roster, 1, 44);
        let chained_progress = progress_for_main_tile(&player_roster, 1, 0);

        assert!(is_same_color_route_tile(
            1,
            44,
            &board_layout,
            &player_roster
        ));

        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: PieceStatus::Active,
                progress: start_progress,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world).unwrap();
        let mut notes = Vec::new();

        apply_jump_effect(
            &PlannedAction::Move {
                piece_id: 1,
                target_progress: start_progress,
            },
            MoveRouteEffects::default(),
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
        assert_eq!(progress, expected_progress);
        assert_ne!(progress, chained_progress);
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn same_color_route_tile_uses_selected_player_seat() {
        let mut one_vs_one_setup = setup(GameMode::OneVsOne);
        one_vs_one_setup.set_player_seat(0, PlayerSeat::Red);
        let (players, _) = crate::gameplay::match_flow::build_match_rosters(&one_vs_one_setup);
        let player_roster = PlayerRoster::from_players(players);
        let board_layout = BoardLayout::default();

        assert!(is_same_color_route_tile(
            1,
            45,
            &board_layout,
            &player_roster
        ));
        assert!(!is_same_color_route_tile(
            1,
            40,
            &board_layout,
            &player_roster
        ));
    }

    #[test]
    fn shortcut_flight_consumes_one_roll_step() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::TwoVsTwo));
        let player_roster = PlayerRoster::from_players(players);
        let board_layout = BoardLayout::default();

        for (player_id, source_tile, target_tile) in
            [(1, 7, 18), (2, 19, 30), (3, 43, 6), (4, 31, 42)]
        {
            let source_progress = progress_for_main_tile(&player_roster, player_id, source_tile);
            let start_progress = source_progress.saturating_sub(1);
            let expected_progress = progress_for_main_tile(&player_roster, player_id, target_tile);
            let progress_after_source = source_progress.saturating_add(1);
            let same_color_progress = next_same_color_jump_progress(
                player_id,
                source_progress,
                &board_layout,
                &player_roster,
            )
            .expect("nearer same-color tile exists");

            assert_eq!(
                board_layout.tile_kind_for_route_index(source_tile),
                Some(TileKind::Jump)
            );
            assert!(
                same_color_progress < expected_progress,
                "shortcut should outrank a nearer same-color jump for P{player_id}"
            );
            assert_eq!(
                movement_steps_for_roll(
                    player_id,
                    PieceStatus::Active,
                    start_progress,
                    1,
                    &board_layout,
                    &player_roster
                ),
                Some(vec![
                    MovementStep {
                        progress: source_progress,
                        kind: MovementStepKind::Normal,
                    },
                    MovementStep {
                        progress: expected_progress,
                        kind: MovementStepKind::Shortcut,
                    }
                ])
            );
            assert_eq!(
                compute_move_target_distance_on_board(
                    player_id,
                    PieceStatus::Active,
                    start_progress,
                    1,
                    &board_layout,
                    &player_roster
                ),
                Some(expected_progress)
            );
            assert_eq!(
                movement_steps_for_roll(
                    player_id,
                    PieceStatus::Active,
                    start_progress,
                    2,
                    &board_layout,
                    &player_roster
                ),
                Some(vec![
                    MovementStep {
                        progress: source_progress,
                        kind: MovementStepKind::Normal,
                    },
                    MovementStep {
                        progress: progress_after_source,
                        kind: MovementStepKind::Normal,
                    },
                ])
            );
            assert_eq!(
                compute_move_target_distance_on_board(
                    player_id,
                    PieceStatus::Active,
                    start_progress,
                    2,
                    &board_layout,
                    &player_roster
                ),
                Some(progress_after_source)
            );
        }
    }

    #[test]
    fn move_route_effects_only_marks_landed_shortcut_as_used() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::TwoVsTwo));
        let player_roster = PlayerRoster::from_players(players);
        let board_layout = BoardLayout::default();
        let source_progress = progress_for_main_tile(&player_roster, 1, 7);
        let start_progress = source_progress.saturating_sub(1);
        let shortcut_target = progress_for_main_tile(&player_roster, 1, 18);
        let origin = ActionOrigin {
            owner_player_id: 1,
            status: PieceStatus::Active,
            progress: start_progress,
            translation: Vec3::ZERO,
            new_progress: shortcut_target,
        };

        assert_eq!(
            move_route_effects_for_action(
                &PlannedAction::Move {
                    piece_id: 1,
                    target_progress: shortcut_target,
                },
                Some(&origin),
                1,
                &board_layout,
                &player_roster,
            ),
            MoveRouteEffects {
                shortcut_used: true,
            }
        );

        let pass_through_target = source_progress.saturating_add(1);
        let pass_through_origin = ActionOrigin {
            new_progress: pass_through_target,
            ..origin
        };
        assert_eq!(
            move_route_effects_for_action(
                &PlannedAction::Move {
                    piece_id: 1,
                    target_progress: pass_through_target,
                },
                Some(&pass_through_origin),
                2,
                &board_layout,
                &player_roster,
            ),
            MoveRouteEffects::default()
        );
    }

    #[test]
    fn shortcut_flight_execute_action_stops_on_shortcut_target() {
        let (players, teams) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::TwoVsTwo));
        let player_roster = PlayerRoster::from_players(players);
        let team_roster = TeamRoster { teams };
        let board_layout = BoardLayout::default();
        let match_config = match_config(GameMode::TwoVsTwo);
        let source_progress = progress_for_main_tile(&player_roster, 1, 7);
        let start_progress = source_progress.saturating_sub(1);
        let expected_progress = progress_for_main_tile(&player_roster, 1, 18);

        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: PieceStatus::Active,
                progress: start_progress,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::default(),
        ));

        let mut skill_roster = build_skill_roster(&player_roster);
        let mut match_result = MatchResult::default();
        let mut turn_state = TurnState::opening_turn();
        let mut input_state = TurnInputState::default();
        let mut next_phase = NextState::<GamePhase>::default();
        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world).unwrap();

        execute_action(
            PlannedAction::Move {
                piece_id: 1,
                target_progress: expected_progress,
            },
            1,
            1,
            ActionResources {
                player_roster: &player_roster,
                team_roster: &team_roster,
                match_config: &match_config,
                board_layout: &board_layout,
            },
            ActionState {
                skill_roster: &mut skill_roster,
                match_result: &mut match_result,
                turn_state: &mut turn_state,
                input_state: &mut input_state,
                next_phase: &mut next_phase,
            },
            &mut query,
        );

        let progress = query
            .iter_mut()
            .find(|(piece_id, _, _, _)| piece_id.0 == 1)
            .map(|(_, _, piece_state, _)| piece_state.progress)
            .unwrap_or_default();

        assert_eq!(progress, expected_progress);
        assert_ne!(progress, expected_progress.saturating_add(1));
    }

    #[test]
    fn shortcut_source_does_not_trigger_free_jump_after_landing() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::TwoVsTwo));
        let player_roster = PlayerRoster::from_players(players);
        let board_layout = BoardLayout::default();
        let source_progress = progress_for_main_tile(&player_roster, 1, 7);

        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: PieceStatus::Active,
                progress: source_progress,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world).unwrap();
        let mut notes = Vec::new();

        apply_jump_effect(
            &PlannedAction::Move {
                piece_id: 1,
                target_progress: source_progress,
            },
            MoveRouteEffects::default(),
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
        assert_eq!(progress, source_progress);
        assert!(notes.is_empty());
    }

    #[test]
    fn different_color_route_tile_does_not_trigger_same_color_jump() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::OneVsOne));
        let player_roster = PlayerRoster::from_players(players);
        let board_layout = BoardLayout::default();
        let start_progress = progress_for_main_tile(&player_roster, 1, 45);

        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: PieceStatus::Active,
                progress: start_progress,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world).unwrap();
        let mut notes = Vec::new();

        apply_jump_effect(
            &PlannedAction::Move {
                piece_id: 1,
                target_progress: start_progress,
            },
            MoveRouteEffects::default(),
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
        assert_eq!(progress, start_progress);
        assert!(notes.is_empty());
    }

    #[test]
    fn collect_actions_returns_launch_and_move_options_for_human_player() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::OneVsOne));
        let board_layout = BoardLayout::default();
        let player_roster = PlayerRoster::from_players(players);

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
                motion_serial: 0,
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
                motion_serial: 0,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let query = system_state.get_mut(&mut world).unwrap();

        let actions = collect_actions(
            1,
            DiceRoll(6),
            0,
            LaunchRule::SixOnly,
            &board_layout,
            &player_roster,
            &query,
        );
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
    fn collect_actions_allows_home_lane_finish_bounce() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::OneVsOne));
        let board_layout = BoardLayout::default();
        let player_roster = PlayerRoster::from_players(players);

        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: PieceStatus::Active,
                progress: FINISH_DISTANCE - 2,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let query = system_state.get_mut(&mut world).unwrap();

        assert_eq!(
            collect_actions(
                1,
                DiceRoll(3),
                0,
                LaunchRule::SixOnly,
                &board_layout,
                &player_roster,
                &query,
            ),
            vec![PlannedAction::Move {
                piece_id: 1,
                target_progress: FINISH_DISTANCE - 1,
            }]
        );
    }

    #[test]
    fn collect_actions_uses_configured_launch_rule() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::OneVsOne));
        let board_layout = BoardLayout::default();
        let player_roster = PlayerRoster::from_players(players);

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
                motion_serial: 0,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let query = system_state.get_mut(&mut world).unwrap();

        let six_only_actions = collect_actions(
            1,
            DiceRoll(4),
            0,
            LaunchRule::SixOnly,
            &board_layout,
            &player_roster,
            &query,
        );
        assert!(six_only_actions.is_empty());

        let even_actions = collect_actions(
            1,
            DiceRoll(4),
            0,
            LaunchRule::Even,
            &board_layout,
            &player_roster,
            &query,
        );
        assert_eq!(
            even_actions,
            vec![PlannedAction::Launch {
                piece_id: 1,
                target_progress: 0,
            }]
        );
    }

    #[test]
    fn current_player_control_reads_player_roster() {
        let player_roster = PlayerRoster::from_players(vec![
            PlayerProfile {
                state: PlayerState {
                    player_id: 1,
                    team_id: 1,
                    control: PlayerControl::Human,
                },
                seat: PlayerSeat::Blue,
                color: Color::srgb(1.0, 0.0, 0.0),
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
                color: Color::srgb(0.0, 0.0, 1.0),
                hangar_slots: vec![],
                launch_position: Vec2::ZERO,
                launch_tile_index: 0,
                home_lane_positions: vec![],
                goal_position: Vec2::ZERO,
            },
        ]);

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
        let player_roster = PlayerRoster::from_players(players);
        let match_config = match_config(GameMode::TwoVsTwo);
        let stack_tile = 42;
        let p1_progress = progress_for_main_tile(&player_roster, 1, stack_tile);
        let p3_progress = progress_for_main_tile(&player_roster, 3, stack_tile);

        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: PieceStatus::Active,
                progress: p1_progress,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
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
                progress: p3_progress,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world).unwrap();
        let mut notes = Vec::new();

        apply_team_stack(
            &PlannedAction::Move {
                piece_id: 1,
                target_progress: p1_progress,
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
        let positions = query
            .iter_mut()
            .map(|(piece_id, _, piece_state, _)| {
                (
                    piece_id.0,
                    piece_board_position(*piece_state, &player_roster),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            positions,
            vec![
                (1, Some(BoardPosition::Main(stack_tile))),
                (2, Some(BoardPosition::Main(stack_tile))),
            ]
        );
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
        let player_roster = PlayerRoster::from_players(players);
        let stack_tile = 42;
        let p1_progress = progress_for_main_tile(&player_roster, 1, stack_tile);
        let p3_progress = progress_for_main_tile(&player_roster, 3, stack_tile);

        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: PieceStatus::Active,
                progress: p1_progress,
                shield: 0,
                stack_shield: 1,
                motion_serial: 0,
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
                progress: p3_progress,
                shield: 0,
                stack_shield: 1,
                motion_serial: 0,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world).unwrap();

        clear_stack_from_origin(
            &PlannedAction::Move {
                piece_id: 1,
                target_progress: p1_progress + 2,
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
    fn normal_tile_stacked_defenders_send_attacker_to_hangar_without_damage() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::TwoVsTwo));
        let player_roster = PlayerRoster::from_players(players);
        let match_config = match_config(GameMode::TwoVsTwo);
        let board_layout = BoardLayout::default();
        let stack_tile = 42;
        let attacker_hangar = Vec2::new(320.0, 280.0);
        let p1_progress = progress_for_main_tile(&player_roster, 1, stack_tile);
        let p2_progress = progress_for_main_tile(&player_roster, 2, stack_tile);
        let p3_progress = progress_for_main_tile(&player_roster, 3, stack_tile);

        assert_eq!(
            board_layout.tile_kind_for_route_index(stack_tile),
            Some(TileKind::Normal)
        );

        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(attacker_hangar),
            PieceState {
                owner_player_id: 2,
                team_id: 2,
                status: PieceStatus::Active,
                progress: p2_progress,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::from_xyz(12.0, 34.0, 0.0),
        ));
        world.spawn((
            PieceId(2),
            HangarSlot(Vec2::new(-320.0, 280.0)),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: PieceStatus::Active,
                progress: p1_progress,
                shield: 0,
                stack_shield: 1,
                motion_serial: 0,
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
                progress: p3_progress,
                shield: 0,
                stack_shield: 1,
                motion_serial: 0,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world).unwrap();
        let mut notes = Vec::new();

        let attacker_landed = resolve_collision(
            &PlannedAction::Move {
                piece_id: 1,
                target_progress: p2_progress,
            },
            None,
            &board_layout,
            &player_roster,
            &match_config,
            &mut query,
            &mut notes,
        );

        assert!(!attacker_landed);
        let states = query
            .iter_mut()
            .map(|(piece_id, _, piece_state, transform)| {
                (
                    piece_id.0,
                    piece_state.status,
                    piece_state.progress,
                    piece_state.stack_shield,
                    transform.translation.x,
                    transform.translation.y,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            states,
            vec![
                (
                    1,
                    PieceStatus::InHangar,
                    0,
                    0,
                    attacker_hangar.x,
                    attacker_hangar.y
                ),
                (2, PieceStatus::Active, p1_progress, 1, 0.0, 0.0),
                (3, PieceStatus::Active, p3_progress, 1, 0.0, 0.0),
            ]
        );
        assert!(
            notes
                .iter()
                .any(|note| note.contains("stacked defenders bounced attacker back to hangar"))
        );
    }

    #[test]
    fn attack_tile_collision_clears_all_defenders_through_shields() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::TwoVsTwo));
        let player_roster = PlayerRoster::from_players(players);
        let match_config = match_config(GameMode::TwoVsTwo);
        let board_layout = BoardLayout::default();
        let attack_tile = 14;
        let attacker_hangar = Vec2::new(320.0, 280.0);
        let p1_hangar = Vec2::new(-320.0, 280.0);
        let p3_hangar = Vec2::new(-320.0, -280.0);
        let p1_progress = progress_for_main_tile(&player_roster, 1, attack_tile);
        let p2_progress = progress_for_main_tile(&player_roster, 2, attack_tile);
        let p3_progress = progress_for_main_tile(&player_roster, 3, attack_tile);

        assert_eq!(
            board_layout.tile_kind_for_route_index(attack_tile),
            Some(TileKind::Attack)
        );

        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(attacker_hangar),
            PieceState {
                owner_player_id: 2,
                team_id: 2,
                status: PieceStatus::Active,
                progress: p2_progress,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::from_xyz(12.0, 34.0, 0.0),
        ));
        world.spawn((
            PieceId(2),
            HangarSlot(p1_hangar),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: PieceStatus::Active,
                progress: p1_progress,
                shield: 2,
                stack_shield: 1,
                motion_serial: 0,
            },
            Transform::default(),
        ));
        world.spawn((
            PieceId(3),
            HangarSlot(p3_hangar),
            PieceState {
                owner_player_id: 3,
                team_id: 1,
                status: PieceStatus::Active,
                progress: p3_progress,
                shield: 1,
                stack_shield: 1,
                motion_serial: 0,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world).unwrap();
        let mut notes = Vec::new();

        let attacker_landed = resolve_collision(
            &PlannedAction::Move {
                piece_id: 1,
                target_progress: p2_progress,
            },
            Some(ActionOrigin {
                owner_player_id: 2,
                status: PieceStatus::Active,
                progress: p2_progress.saturating_sub(1),
                translation: Vec3::new(-50.0, -70.0, 0.0),
                new_progress: p2_progress,
            }),
            &board_layout,
            &player_roster,
            &match_config,
            &mut query,
            &mut notes,
        );

        assert!(attacker_landed);
        let states = query
            .iter_mut()
            .map(|(piece_id, _, piece_state, transform)| {
                (
                    piece_id.0,
                    piece_state.status,
                    piece_state.progress,
                    piece_state.shield,
                    piece_state.stack_shield,
                    transform.translation.x,
                    transform.translation.y,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            states,
            vec![
                (1, PieceStatus::Active, p2_progress, 0, 0, 12.0, 34.0),
                (2, PieceStatus::InHangar, 0, 0, 0, p1_hangar.x, p1_hangar.y),
                (3, PieceStatus::InHangar, 0, 0, 0, p3_hangar.x, p3_hangar.y),
            ]
        );
        assert!(
            notes
                .iter()
                .any(|note| note == "sent piece #2 back to hangar")
        );
        assert!(
            notes
                .iter()
                .any(|note| note == "sent piece #3 back to hangar")
        );
        assert!(
            notes
                .iter()
                .any(|note| note == "enhanced collision on attack tile")
        );
        assert!(
            !notes
                .iter()
                .any(|note| note.contains("blocked") || note.contains("bounced"))
        );
    }

    #[test]
    fn free_for_all_treats_two_vs_two_teammates_as_enemies_on_collision() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::FreeForAll));
        let player_roster = PlayerRoster::from_players(players);
        let match_config = match_config(GameMode::FreeForAll);
        let board_layout = BoardLayout::default();
        let target_tile = 42;
        let defender_hangar = Vec2::new(-320.0, -280.0);
        let p1_progress = progress_for_main_tile(&player_roster, 1, target_tile);
        let p3_progress = progress_for_main_tile(&player_roster, 3, target_tile);

        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::new(-320.0, 280.0)),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: PieceStatus::Active,
                progress: p1_progress,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::default(),
        ));
        world.spawn((
            PieceId(3),
            HangarSlot(defender_hangar),
            PieceState {
                owner_player_id: 3,
                team_id: 3,
                status: PieceStatus::Active,
                progress: p3_progress,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world).unwrap();
        let mut notes = Vec::new();

        let attacker_landed = resolve_collision(
            &PlannedAction::Move {
                piece_id: 1,
                target_progress: p1_progress,
            },
            None,
            &board_layout,
            &player_roster,
            &match_config,
            &mut query,
            &mut notes,
        );

        assert!(attacker_landed);
        let defender = query
            .iter_mut()
            .find_map(|(piece_id, _, piece_state, transform)| {
                (piece_id.0 == 3).then_some((
                    piece_state.status,
                    transform.translation.x,
                    transform.translation.y,
                ))
            })
            .expect("defender exists");
        assert_eq!(
            defender,
            (PieceStatus::InHangar, defender_hangar.x, defender_hangar.y)
        );
        assert!(
            notes
                .iter()
                .any(|note| note == "sent piece #3 back to hangar")
        );
    }

    #[test]
    fn special_tile_collision_consumes_shared_stack_shield() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::TwoVsTwo));
        let player_roster = PlayerRoster::from_players(players);
        let match_config = match_config(GameMode::TwoVsTwo);
        let board_layout = BoardLayout::default();
        let stack_tile = 44;
        let p1_progress = progress_for_main_tile(&player_roster, 1, stack_tile);
        let p2_progress = progress_for_main_tile(&player_roster, 2, stack_tile);
        let p3_progress = progress_for_main_tile(&player_roster, 3, stack_tile);

        assert_eq!(
            board_layout.tile_kind_for_route_index(stack_tile),
            Some(TileKind::Defense)
        );

        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::new(-320.0, 280.0)),
            PieceState {
                owner_player_id: 2,
                team_id: 2,
                status: PieceStatus::Active,
                progress: p2_progress,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
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
                progress: p1_progress,
                shield: 0,
                stack_shield: 1,
                motion_serial: 0,
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
                progress: p3_progress,
                shield: 0,
                stack_shield: 1,
                motion_serial: 0,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world).unwrap();
        let mut notes = Vec::new();

        resolve_collision(
            &PlannedAction::Move {
                piece_id: 1,
                target_progress: p2_progress,
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
        let player_roster = PlayerRoster::from_players(players);
        let match_config = match_config(GameMode::TwoVsTwo);
        let board_layout = BoardLayout::default();
        let shield_tile = 20;
        let p1_progress = progress_for_main_tile(&player_roster, 1, shield_tile);
        let p2_target_progress = progress_for_main_tile(&player_roster, 2, shield_tile);
        let p2_origin_progress = p2_target_progress - 1;

        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::new(-320.0, 280.0)),
            PieceState {
                owner_player_id: 2,
                team_id: 2,
                status: PieceStatus::Active,
                progress: p2_target_progress,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
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
                progress: p1_progress,
                shield: 1,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world).unwrap();
        let mut notes = Vec::new();

        resolve_collision(
            &PlannedAction::Move {
                piece_id: 1,
                target_progress: p2_target_progress,
            },
            Some(ActionOrigin {
                owner_player_id: 2,
                status: PieceStatus::Active,
                progress: p2_origin_progress,
                translation: Vec3::new(-50.0, -70.0, 0.0),
                new_progress: p2_target_progress,
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
        assert_eq!(
            states,
            vec![
                (1, p2_origin_progress, 0, -50.0, -70.0),
                (2, p1_progress, 0, 0.0, 0.0)
            ]
        );
        assert!(notes.iter().any(|note| note.contains("bounced back")));
    }

    #[test]
    fn gain_skill_charge_event_adds_exactly_one_charge() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::OneVsOne));
        let player_roster = PlayerRoster::from_players(players);
        let mut skill_roster = build_skill_roster(&player_roster);
        let match_config = match_config(GameMode::OneVsOne);
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
                motion_serial: 0,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world).unwrap();

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
            LandingResources {
                player_roster: &player_roster,
                match_config: &match_config,
                board_layout: &board_layout,
                jump_source_event_tile: None,
            },
            &mut query,
            &mut skill_roster,
            &mut Vec::new(),
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
        let outcome = note.expect("event should resolve");
        assert_eq!(outcome.kind, TileEventKind::GainSkillCharge);
        assert!(outcome.note.contains("GainSkillCharge"));
        assert!(outcome.attacker_still_landed);
    }

    #[test]
    fn gain_shield_event_updates_piece_buff_state() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::OneVsOne));
        let player_roster = PlayerRoster::from_players(players);
        let mut skill_roster = build_skill_roster(&player_roster);
        let match_config = match_config(GameMode::OneVsOne);
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
                motion_serial: 0,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world).unwrap();
        let mut final_progress = 2;

        let outcome = apply_event_kind_effect(
            TileEventKind::GainShield,
            &PlannedAction::Move {
                piece_id: 1,
                target_progress: 2,
            },
            &mut final_progress,
            LandingResources {
                player_roster: &player_roster,
                match_config: &match_config,
                board_layout: &board_layout,
                jump_source_event_tile: None,
            },
            &mut query,
            &mut skill_roster,
            &mut Vec::new(),
        )
        .expect("event should resolve");

        let shield = query
            .iter_mut()
            .find(|(piece_id, _, _, _)| piece_id.0 == 1)
            .map(|(_, _, piece_state, _)| piece_state.shield)
            .unwrap_or_default();
        assert_eq!(outcome.kind, TileEventKind::GainShield);
        assert!(outcome.note.contains("gained shield"));
        assert!(outcome.attacker_still_landed);
        assert_eq!(shield, 1);
    }

    #[test]
    fn defense_tile_records_piece_effect_notice() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::OneVsOne));
        let player_roster = PlayerRoster::from_players(players);
        let mut skill_roster = build_skill_roster(&player_roster);
        let match_config = match_config(GameMode::OneVsOne);
        let board_layout = BoardLayout::default();
        let target_tile = 44;
        let target_progress = progress_for_main_tile(&player_roster, 1, target_tile);

        assert_eq!(
            board_layout.tile_kind_for_route_index(target_tile),
            Some(TileKind::Defense)
        );

        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: PieceStatus::Active,
                progress: target_progress,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world).unwrap();
        let mut notes = Vec::new();
        let mut piece_effect_notice = None;

        let attacker_still_landed = apply_post_collision_tile_effects(
            &PlannedAction::Move {
                piece_id: 1,
                target_progress,
            },
            LandingResources {
                player_roster: &player_roster,
                match_config: &match_config,
                board_layout: &board_layout,
                jump_source_event_tile: None,
            },
            &mut query,
            &mut skill_roster,
            &mut notes,
            &mut piece_effect_notice,
        );

        assert!(attacker_still_landed);
        assert_eq!(
            piece_effect_notice,
            Some(PieceEffectNotice {
                piece_id: 1,
                kind: PieceEffectKind::Defense,
            })
        );
        assert!(notes.iter().any(|note| note.contains("gained shield")));
    }

    #[test]
    fn passing_over_special_tiles_does_not_trigger_tile_effects() {
        let (players, teams) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::OneVsOne));
        let player_roster = PlayerRoster::from_players(players);
        let team_roster = TeamRoster { teams };
        let mut skill_roster = build_skill_roster(&player_roster);
        let match_config = match_config(GameMode::OneVsOne);
        let board_layout = BoardLayout::default();
        let start_progress = progress_for_main_tile(&player_roster, 1, 7);
        let target_progress = progress_for_main_tile(&player_roster, 1, 9);

        assert_eq!(
            board_layout.tile_kind_for_route_index(8),
            Some(TileKind::Defense)
        );
        assert_eq!(
            movement_steps_for_roll(
                1,
                PieceStatus::Active,
                start_progress,
                2,
                &board_layout,
                &player_roster,
            ),
            Some(vec![
                MovementStep {
                    progress: progress_for_main_tile(&player_roster, 1, 8),
                    kind: MovementStepKind::Normal,
                },
                MovementStep {
                    progress: target_progress,
                    kind: MovementStepKind::Normal,
                }
            ])
        );

        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: PieceStatus::Active,
                progress: start_progress,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::default(),
        ));

        let mut match_result = MatchResult::default();
        let mut turn_state = TurnState::opening_turn();
        let mut input_state = TurnInputState::default();
        let mut next_phase = NextState::<GamePhase>::default();
        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world).unwrap();

        execute_action(
            PlannedAction::Move {
                piece_id: 1,
                target_progress,
            },
            2,
            2,
            ActionResources {
                player_roster: &player_roster,
                team_roster: &team_roster,
                match_config: &match_config,
                board_layout: &board_layout,
            },
            ActionState {
                skill_roster: &mut skill_roster,
                match_result: &mut match_result,
                turn_state: &mut turn_state,
                input_state: &mut input_state,
                next_phase: &mut next_phase,
            },
            &mut query,
        );

        let shield = query
            .iter_mut()
            .find(|(piece_id, _, _, _)| piece_id.0 == 1)
            .map(|(_, _, piece_state, _)| piece_state.shield)
            .unwrap_or_default();
        assert_eq!(shield, 0);
        assert_eq!(turn_state.last_piece_effect, None);
        assert!(
            !turn_state
                .last_action
                .as_deref()
                .unwrap_or_default()
                .contains("gained shield")
        );
    }

    #[test]
    fn post_collision_effects_resolve_event_tile_that_triggered_same_color_jump() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::OneVsOne));
        let player_roster = PlayerRoster::from_players(players);
        let mut skill_roster = build_skill_roster(&player_roster);
        let match_config = match_config(GameMode::OneVsOne);
        let board_layout = BoardLayout::default();
        let target_progress = progress_for_main_tile(&player_roster, 1, 3);

        assert_eq!(
            board_layout.tile_kind_for_route_index(0),
            Some(TileKind::Event)
        );
        assert_eq!(
            jump_source_event_tile(
                Some(BoardPosition::Main(0)),
                Some(BoardPosition::Main(3)),
                &board_layout,
            ),
            Some(0)
        );

        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: PieceStatus::Active,
                progress: target_progress,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world).unwrap();
        let mut notes = Vec::new();
        let mut piece_effect_notice = None;

        let attacker_still_landed = apply_post_collision_tile_effects(
            &PlannedAction::Move {
                piece_id: 1,
                target_progress,
            },
            LandingResources {
                player_roster: &player_roster,
                match_config: &match_config,
                board_layout: &board_layout,
                jump_source_event_tile: Some(0),
            },
            &mut query,
            &mut skill_roster,
            &mut notes,
            &mut piece_effect_notice,
        );

        assert!(attacker_still_landed);
        assert_eq!(piece_effect_notice, None);
        assert!(
            notes
                .iter()
                .any(|note| note.contains("pre-jump event tile 0: event"))
        );
    }

    #[test]
    fn disable_next_skill_event_blocks_next_turn_only() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::OneVsOne));
        let player_roster = PlayerRoster::from_players(players);
        let mut skill_roster = build_skill_roster(&player_roster);
        let match_config = match_config(GameMode::OneVsOne);
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
                motion_serial: 0,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world).unwrap();
        let mut final_progress = 2;

        let note = apply_event_kind_effect(
            TileEventKind::DisableNextSkill,
            &PlannedAction::Move {
                piece_id: 1,
                target_progress: 2,
            },
            &mut final_progress,
            LandingResources {
                player_roster: &player_roster,
                match_config: &match_config,
                board_layout: &board_layout,
                jump_source_event_tile: None,
            },
            &mut query,
            &mut skill_roster,
            &mut Vec::new(),
        );
        let outcome = note.expect("event should resolve");
        assert_eq!(outcome.kind, TileEventKind::DisableNextSkill);
        assert!(outcome.note.contains("DisableNextSkill"));
        assert!(outcome.attacker_still_landed);

        sync_turn_skill_usage(&mut skill_roster, 1);
        assert!(!can_use_skill_this_turn(&skill_roster, 1));

        sync_turn_skill_usage(&mut skill_roster, 2);
        sync_turn_skill_usage(&mut skill_roster, 1);
        assert!(can_use_skill_this_turn(&skill_roster, 1));
    }

    #[test]
    fn advance_two_event_advances_exactly_two_progress() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::OneVsOne));
        let player_roster = PlayerRoster::from_players(players);
        let mut skill_roster = build_skill_roster(&player_roster);
        let match_config = match_config(GameMode::OneVsOne);
        let board_layout = BoardLayout::default();
        let start_progress = 10;

        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: PieceStatus::Active,
                progress: start_progress,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world).unwrap();
        let mut final_progress = start_progress;
        let mut notes = Vec::new();

        let event_result = apply_event_kind_effect(
            TileEventKind::AdvanceTwo,
            &PlannedAction::Move {
                piece_id: 1,
                target_progress: start_progress,
            },
            &mut final_progress,
            LandingResources {
                player_roster: &player_roster,
                match_config: &match_config,
                board_layout: &board_layout,
                jump_source_event_tile: None,
            },
            &mut query,
            &mut skill_roster,
            &mut notes,
        );

        assert_eq!(
            event_result,
            Some(TileEventOutcome {
                kind: TileEventKind::AdvanceTwo,
                note: "event advance +2".to_string(),
                attacker_still_landed: true,
            })
        );
        assert_eq!(final_progress, start_progress + 2);

        let progress = query
            .iter_mut()
            .find(|(piece_id, _, _, _)| piece_id.0 == 1)
            .map(|(_, _, piece_state, _)| piece_state.progress)
            .unwrap_or_default();
        assert_eq!(progress, start_progress + 2);
        assert!(notes.is_empty());
    }

    #[test]
    fn advance_two_event_can_send_enemy_back_to_hangar() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::OneVsOne));
        let player_roster = PlayerRoster::from_players(players);
        let mut skill_roster = build_skill_roster(&player_roster);
        let match_config = match_config(GameMode::OneVsOne);
        let board_layout = BoardLayout::default();
        let target_tile = 42;
        let p1_start_progress = progress_for_main_tile(&player_roster, 1, 40);
        let p2_target_progress = progress_for_main_tile(&player_roster, 2, target_tile);
        let p2_hangar = Vec2::new(320.0, 280.0);

        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: PieceStatus::Active,
                progress: p1_start_progress,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::default(),
        ));
        world.spawn((
            PieceId(2),
            HangarSlot(p2_hangar),
            PieceState {
                owner_player_id: 2,
                team_id: 2,
                status: PieceStatus::Active,
                progress: p2_target_progress,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::from_xyz(10.0, 20.0, 0.0),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world).unwrap();
        let mut final_progress = p1_start_progress;
        let mut notes = Vec::new();

        let event_result = apply_event_kind_effect(
            TileEventKind::AdvanceTwo,
            &PlannedAction::Move {
                piece_id: 1,
                target_progress: p1_start_progress,
            },
            &mut final_progress,
            LandingResources {
                player_roster: &player_roster,
                match_config: &match_config,
                board_layout: &board_layout,
                jump_source_event_tile: None,
            },
            &mut query,
            &mut skill_roster,
            &mut notes,
        );

        assert_eq!(
            event_result,
            Some(TileEventOutcome {
                kind: TileEventKind::AdvanceTwo,
                note: "event advance +2".to_string(),
                attacker_still_landed: true,
            })
        );
        let states = query
            .iter_mut()
            .map(|(piece_id, _, piece_state, transform)| {
                (
                    piece_id.0,
                    piece_state.status,
                    piece_state.progress,
                    transform.translation.x,
                    transform.translation.y,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            states,
            vec![
                (
                    1,
                    PieceStatus::Active,
                    p1_start_progress + 2,
                    board_layout
                        .world_pos_for_route_index(target_tile)
                        .unwrap()
                        .x,
                    board_layout
                        .world_pos_for_route_index(target_tile)
                        .unwrap()
                        .y,
                ),
                (2, PieceStatus::InHangar, 0, p2_hangar.x, p2_hangar.y),
            ]
        );
        assert!(
            notes
                .iter()
                .any(|note| note.contains("sent piece #2 back to hangar"))
        );
    }

    #[test]
    fn advance_two_event_collision_respects_enemy_shield() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::OneVsOne));
        let player_roster = PlayerRoster::from_players(players);
        let mut skill_roster = build_skill_roster(&player_roster);
        let match_config = match_config(GameMode::OneVsOne);
        let board_layout = BoardLayout::default();
        let target_tile = 42;
        let p1_start_progress = progress_for_main_tile(&player_roster, 1, 40);
        let p2_target_progress = progress_for_main_tile(&player_roster, 2, target_tile);
        let origin_translation = Vec3::new(-12.0, -34.0, 0.0);

        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: PieceStatus::Active,
                progress: p1_start_progress,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::from_translation(origin_translation),
        ));
        world.spawn((
            PieceId(2),
            HangarSlot(Vec2::new(320.0, 280.0)),
            PieceState {
                owner_player_id: 2,
                team_id: 2,
                status: PieceStatus::Active,
                progress: p2_target_progress,
                shield: 1,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world).unwrap();
        let mut final_progress = p1_start_progress;
        let mut notes = Vec::new();

        let event_result = apply_event_kind_effect(
            TileEventKind::AdvanceTwo,
            &PlannedAction::Move {
                piece_id: 1,
                target_progress: p1_start_progress,
            },
            &mut final_progress,
            LandingResources {
                player_roster: &player_roster,
                match_config: &match_config,
                board_layout: &board_layout,
                jump_source_event_tile: None,
            },
            &mut query,
            &mut skill_roster,
            &mut notes,
        );

        assert_eq!(
            event_result,
            Some(TileEventOutcome {
                kind: TileEventKind::AdvanceTwo,
                note: "event advance +2".to_string(),
                attacker_still_landed: false,
            })
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
            vec![
                (
                    1,
                    p1_start_progress,
                    0,
                    origin_translation.x,
                    origin_translation.y
                ),
                (2, p2_target_progress, 0, 0.0, 0.0),
            ]
        );
        assert!(notes.iter().any(|note| note.contains("bounced back")));
    }

    #[test]
    fn remove_enemy_shield_event_hits_the_only_valid_enemy_target() {
        let (players, _) =
            crate::gameplay::match_flow::build_match_rosters(&setup(GameMode::OneVsOne));
        let player_roster = PlayerRoster::from_players(players);
        let mut skill_roster = build_skill_roster(&player_roster);
        let match_config = match_config(GameMode::OneVsOne);
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
                motion_serial: 0,
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
                motion_serial: 0,
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
                motion_serial: 0,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world).unwrap();
        let mut final_progress = 3;

        let note = apply_event_kind_effect(
            TileEventKind::RemoveEnemyShield,
            &PlannedAction::Move {
                piece_id: 1,
                target_progress: 3,
            },
            &mut final_progress,
            LandingResources {
                player_roster: &player_roster,
                match_config: &match_config,
                board_layout: &board_layout,
                jump_source_event_tile: None,
            },
            &mut query,
            &mut skill_roster,
            &mut Vec::new(),
        )
        .expect("event should resolve");

        let shields = query
            .iter_mut()
            .map(|(piece_id, _, piece_state, _)| (piece_id.0, piece_state.shield))
            .collect::<Vec<_>>();
        assert_eq!(shields, vec![(1, 0), (2, 0), (3, 0)]);
        assert_eq!(note.kind, TileEventKind::RemoveEnemyShield);
        assert!(note.note.contains("removed shield from piece #2"));
    }
}
