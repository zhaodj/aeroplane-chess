use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::data::game_mode::GameMode;
use crate::domain::dice::DiceRoll;
use crate::domain::piece::PieceState;
use crate::domain::player::PlayerControl;
use crate::gameplay::ai::AiDifficulty;
use crate::gameplay::match_flow::{
    BoardLayout, MatchConfig, MatchResult, PlayerRoster, TeamRoster,
};
use crate::gameplay::skill_flow::{
    MAX_PIECE_SHIELD, SkillRoster, arm_dash, arm_double_dice, can_use_skill_this_turn,
    clear_dash_arm, dash_bonus, is_active_teammate_piece, is_legal_shield_target,
    is_legal_snipe_target, mark_skill_used, player_skill_state, resolve_roll_value,
    spend_shield_charge, spend_snipe_charge, spend_swap_charge,
};
use crate::gameplay::turn_flow::{
    ActionResources, ActionState, PlannedAction, TurnInputState, TurnState, choose_action,
    collect_actions, current_player_control, execute_action, find_pending_action_by_piece_id,
    finish_turn_without_action, get_pending_action, pressed_selection_key, set_pending_actions,
    set_roll,
};
use crate::plugins::piece_plugin::{HangarSlot, PieceId};
use crate::states::{AppState, GamePhase};

/// 回合驱动插件：整合 AI/人类输入并推进回合结算流程。
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
/// AI 自动执行节拍器（用于模拟思考间隔）。
struct TurnAutomation {
    timer: Timer,
}

#[derive(Resource, Default)]
/// HUD/鼠标入口写入的回合动作请求。
pub struct TurnUiRequest {
    roll_requested: bool,
}

impl TurnUiRequest {
    pub fn queue_roll(&mut self) {
        self.roll_requested = true;
    }

    fn take_roll(&mut self) -> bool {
        let requested = self.roll_requested;
        self.roll_requested = false;
        requested
    }
}

#[derive(SystemParam)]
struct TurnActionParams<'w, 's> {
    input_state: ResMut<'w, TurnInputState>,
    turn_ui_request: ResMut<'w, TurnUiRequest>,
    turn_state: ResMut<'w, TurnState>,
    next_phase: ResMut<'w, NextState<GamePhase>>,
    board_layout: Res<'w, BoardLayout>,
    match_config: Res<'w, MatchConfig>,
    player_roster: Res<'w, PlayerRoster>,
    team_roster: Res<'w, TeamRoster>,
    skill_roster: ResMut<'w, SkillRoster>,
    match_result: ResMut<'w, MatchResult>,
    piece_query: Query<
        'w,
        's,
        (
            &'static PieceId,
            &'static HangarSlot,
            &'static mut PieceState,
            &'static mut Transform,
        ),
    >,
}

fn execute_action_from_params(
    action: PlannedAction,
    roll_value: u8,
    params: &mut TurnActionParams,
) {
    execute_action(
        action,
        roll_value,
        ActionResources {
            player_roster: &params.player_roster,
            team_roster: &params.team_roster,
            match_config: &params.match_config,
            board_layout: &params.board_layout,
        },
        ActionState {
            skill_roster: &mut params.skill_roster,
            match_result: &mut params.match_result,
            turn_state: &mut params.turn_state,
            input_state: &mut params.input_state,
            next_phase: &mut params.next_phase,
        },
        &mut params.piece_query,
    );
}

fn setup_turn_automation(mut commands: Commands) {
    // AI 行为节拍与人类输入缓存在进入对局时统一初始化。
    commands.insert_resource(TurnAutomation {
        timer: Timer::from_seconds(0.9, TimerMode::Repeating),
    });
    commands.insert_resource(TurnInputState::default());
    commands.insert_resource(TurnUiRequest::default());
}

fn cleanup_turn_automation(mut commands: Commands) {
    // 离开对局时清理临时回合资源，避免下局残留。
    commands.remove_resource::<TurnAutomation>();
    commands.remove_resource::<TurnInputState>();
    commands.remove_resource::<TurnUiRequest>();
}

/// AI 回合驱动主循环：定时触发掷骰、选动作并执行完整结算。
fn drive_ai_turn_loop(
    time: Res<Time>,
    mut automation: ResMut<TurnAutomation>,
    game_phase: Res<State<GamePhase>>,
    mut params: TurnActionParams,
) {
    if !matches!(game_phase.get(), GamePhase::AwaitDice) || params.match_result.finished {
        return;
    }

    if current_player_control(params.turn_state.current_player, &params.player_roster)
        != Some(PlayerControl::Ai)
    {
        return;
    }

    if !automation.timer.tick(time.delta()).just_finished() {
        return;
    }

    maybe_use_ai_skills(
        params.turn_state.current_player,
        &params.match_config,
        &mut params.skill_roster,
        &mut params.piece_query,
    );

    let roll_resolution =
        resolve_roll_value(&mut params.skill_roster, params.turn_state.current_player);
    let roll_value = roll_resolution.value;
    let roll = DiceRoll(roll_value);
    set_roll(&mut params.turn_state, roll_value);
    if roll_resolution.used_double_dice {
        params.skill_roster.last_skill_action = Some(format!(
            "P{} resolved DoubleDice into {}",
            params.turn_state.current_player, roll_value
        ));
    }

    let current_player = params.turn_state.current_player;
    maybe_arm_dash_for_ai_after_roll(
        current_player,
        params.match_config.ai_difficulty,
        roll,
        &params.player_roster,
        &mut params.skill_roster,
        &mut params.piece_query,
    );
    let Some(action) = choose_action(
        current_player,
        roll,
        dash_bonus(&params.skill_roster, current_player),
        &params.board_layout,
        &params.player_roster,
        &params.piece_query,
    ) else {
        params.turn_state.last_action = Some(format!(
            "P{current_player} rolled {roll_value} but had no legal action"
        ));
        finish_turn_without_action(
            &mut params.turn_state,
            &mut params.input_state,
            &params.player_roster,
            &mut params.next_phase,
        );
        return;
    };

    execute_action_from_params(action, roll_value, &mut params);
    clear_dash_arm(&mut params.skill_roster, current_player);
}

const HARD_AI_SNIPE_MIN_PROGRESS: u8 = 8;

/// AI 技能策略：Easy 不主动用技能，Normal 偏防御/起飞，Hard 才主动攻击与换位。
fn maybe_use_ai_skills(
    current_player: u8,
    match_config: &MatchConfig,
    skill_roster: &mut SkillRoster,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) {
    if !can_use_skill_this_turn(skill_roster, current_player) {
        return;
    }

    match match_config.ai_difficulty {
        AiDifficulty::Easy => {}
        AiDifficulty::Normal => {
            let _ = try_ai_shield(current_player, skill_roster, piece_query)
                || try_ai_double_dice(current_player, skill_roster, piece_query);
        }
        AiDifficulty::Hard => {
            let _ = try_ai_snipe(
                current_player,
                HARD_AI_SNIPE_MIN_PROGRESS,
                skill_roster,
                piece_query,
            ) || try_ai_shield(current_player, skill_roster, piece_query)
                || try_ai_swap(current_player, match_config.mode, skill_roster, piece_query)
                || try_ai_double_dice(current_player, skill_roster, piece_query);
        }
    }
}

fn try_ai_snipe(
    current_player: u8,
    minimum_target_progress: u8,
    skill_roster: &mut SkillRoster,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> bool {
    let can_use_snipe = player_skill_state(skill_roster, current_player)
        .map(|skills| skills.snipe_charges > 0)
        .unwrap_or(false);
    if !can_use_snipe {
        return false;
    }

    let Some(target_piece_id) =
        preferred_ai_snipe_target(current_player, minimum_target_progress, piece_query)
    else {
        return false;
    };

    if !spend_snipe_charge(skill_roster, current_player) {
        return false;
    }

    mark_skill_used(skill_roster, current_player);
    skill_roster.last_skill_action = Some(execute_snipe_on_turn_query(
        target_piece_id,
        piece_query,
        true,
    ));
    true
}

fn try_ai_shield(
    current_player: u8,
    skill_roster: &mut SkillRoster,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> bool {
    let can_use_shield = player_skill_state(skill_roster, current_player)
        .map(|skills| skills.shield_charges > 0)
        .unwrap_or(false);
    if !can_use_shield {
        return false;
    }

    let Some(target_piece_id) = preferred_ai_shield_target(current_player, piece_query) else {
        return false;
    };

    if spend_shield_charge(skill_roster, current_player)
        && let Some(shield_value) =
            apply_shield_to_piece_to_turn_query(target_piece_id, piece_query)
    {
        mark_skill_used(skill_roster, current_player);
        skill_roster.last_skill_action = Some(format!(
            "P{} (AI) used Shield on piece #{} ({})",
            current_player, target_piece_id, shield_value
        ));
        return true;
    }

    false
}

fn try_ai_swap(
    current_player: u8,
    mode: GameMode,
    skill_roster: &mut SkillRoster,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> bool {
    if mode != GameMode::TwoVsTwo {
        return false;
    }

    let can_use_swap = player_skill_state(skill_roster, current_player)
        .map(|skills| skills.swap_charges > 0)
        .unwrap_or(false);
    if !can_use_swap {
        return false;
    }

    let Some(teammate_piece_id) = preferred_ai_swap_target(current_player, piece_query) else {
        return false;
    };

    if !spend_swap_charge(skill_roster, current_player) {
        return false;
    }

    mark_skill_used(skill_roster, current_player);
    skill_roster.last_skill_action = Some(execute_swap_on_turn_query(
        current_player,
        teammate_piece_id,
        piece_query,
    ));
    true
}

fn try_ai_double_dice(
    current_player: u8,
    skill_roster: &mut SkillRoster,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> bool {
    if should_ai_arm_double_dice(current_player, skill_roster, piece_query)
        && arm_double_dice(skill_roster, current_player)
    {
        mark_skill_used(skill_roster, current_player);
        skill_roster.last_skill_action = Some(format!(
            "P{} (AI) armed DoubleDice for launch pressure",
            current_player
        ));
        return true;
    }

    false
}

/// AI 在掷骰后评估是否需要临时预备 Dash（仅当存在可移动动作）。
fn maybe_arm_dash_for_ai_after_roll(
    current_player: u8,
    ai_difficulty: AiDifficulty,
    roll: DiceRoll,
    player_roster: &PlayerRoster,
    skill_roster: &mut SkillRoster,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) {
    if ai_difficulty == AiDifficulty::Easy {
        return;
    }

    if !can_use_skill_this_turn(skill_roster, current_player) {
        return;
    }
    let can_arm = player_skill_state(skill_roster, current_player)
        .map(|skills| skills.dash_charges > 0 && !skills.dash_armed)
        .unwrap_or(false);
    if !can_arm {
        return;
    }

    let has_movable_action = collect_actions(current_player, roll, 0, player_roster, piece_query)
        .iter()
        .any(PlannedAction::is_move);
    if !has_movable_action {
        return;
    }

    if arm_dash(skill_roster, current_player) {
        mark_skill_used(skill_roster, current_player);
        skill_roster.last_skill_action = Some(format!(
            "P{} (AI) armed Dash after roll for +3 movement",
            current_player
        ));
    }
}

/// 选择 AI 的 Snipe 目标：优先无盾、再有盾，且不攻击队友。
fn preferred_ai_snipe_target(
    current_player: u8,
    minimum_target_progress: u8,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> Option<u8> {
    let mut attacker_team = None;
    for (_, _, piece_state, _) in piece_query.iter_mut() {
        if piece_state.owner_player_id == current_player {
            attacker_team = Some(piece_state.team_id);
            break;
        }
    }
    let attacker_team = attacker_team?;

    let mut unshielded = Vec::new();
    let mut shielded = Vec::new();
    for (piece_id, _, piece_state, _) in piece_query.iter_mut() {
        if !is_legal_snipe_target(current_player, attacker_team, &piece_state)
            || piece_state.progress < minimum_target_progress
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
    unshielded
        .into_iter()
        .next()
        .or_else(|| shielded.into_iter().next())
}

/// 选择 AI 的 Shield 目标：优先己方无盾 Active 棋子。
fn preferred_ai_shield_target(
    current_player: u8,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> Option<u8> {
    piece_query
        .iter_mut()
        .filter(|(_, _, piece_state, _)| is_legal_shield_target(current_player, piece_state))
        .map(|(piece_id, _, _, _)| piece_id.0)
        .min()
}

/// 选择 AI 的 Swap 目标：优先与“更靠前的队友”换位。
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
            is_active_teammate_piece(current_player, own_team, piece_state)
                && piece_state.progress >= own_progress.saturating_add(6)
        })
        .map(|(piece_id, _, piece_state, _)| (piece_id.0, piece_state.progress))
        .collect::<Vec<_>>();
    candidates.sort_by_key(|(_, progress)| std::cmp::Reverse(*progress));
    candidates.into_iter().map(|(piece_id, _)| piece_id).next()
}

/// 判断 AI 是否应预备 DoubleDice（典型场景：全员在机库等待起飞）。
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

/// 在 turn_query 上直接给目标棋子加盾（供 AI 路径调用）。
fn apply_shield_to_piece_to_turn_query(
    piece_id: u8,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> Option<u8> {
    for (query_piece_id, _, mut piece_state, _) in piece_query.iter_mut() {
        if query_piece_id.0 != piece_id {
            continue;
        }

        piece_state.shield = piece_state.shield.saturating_add(1).min(MAX_PIECE_SHIELD);
        return Some(piece_state.shield);
    }

    None
}

/// 在 turn_query 上执行 Snipe 的完整效果（扣盾或送回机库）。
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

/// 在 turn_query 上执行 Swap：交换两枚棋子的状态与位置。
fn execute_swap_on_turn_query(
    current_player: u8,
    teammate_piece_id: u8,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> String {
    let Some((current_piece_id, current_state, current_translation)) =
        piece_query
            .iter()
            .find_map(|(piece_id, _, piece_state, transform)| {
                (piece_state.owner_player_id == current_player
                    && piece_state.status == crate::domain::piece::PieceStatus::Active)
                    .then_some((piece_id.0, *piece_state, transform.translation))
            })
    else {
        return "AI Swap failed: current active piece not found".to_string();
    };

    let Some((teammate_state, teammate_translation)) =
        piece_query
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

/// 人类玩家“掷骰阶段”输入处理（Space 掷骰）。
fn handle_human_roll_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    game_phase: Res<State<GamePhase>>,
    mut params: TurnActionParams,
) {
    if !matches!(game_phase.get(), GamePhase::AwaitDice) || params.match_result.finished {
        return;
    }

    if current_player_control(params.turn_state.current_player, &params.player_roster)
        != Some(PlayerControl::Human)
    {
        return;
    }

    params.input_state.prompt = Some("Press Space to roll".to_string());

    if !keyboard.just_pressed(KeyCode::Space) && !params.turn_ui_request.take_roll() {
        return;
    }

    let roll_resolution =
        resolve_roll_value(&mut params.skill_roster, params.turn_state.current_player);
    let roll_value = roll_resolution.value;
    let roll = DiceRoll(roll_value);
    set_roll(&mut params.turn_state, roll_value);
    if roll_resolution.used_double_dice {
        params.skill_roster.last_skill_action = Some(format!(
            "P{} resolved DoubleDice into {}",
            params.turn_state.current_player, roll_value
        ));
    }

    let current_player = params.turn_state.current_player;
    let actions = collect_actions(
        current_player,
        roll,
        0,
        &params.player_roster,
        &params.piece_query,
    );

    if actions.is_empty() {
        params.turn_state.last_action = Some(format!(
            "P{} rolled {} but had no legal action",
            params.turn_state.current_player, roll_value
        ));
        finish_turn_without_action(
            &mut params.turn_state,
            &mut params.input_state,
            &params.player_roster,
            &mut params.next_phase,
        );
        return;
    }

    let can_offer_dash = can_use_skill_this_turn(&params.skill_roster, current_player)
        && dash_bonus(&params.skill_roster, current_player) == 0
        && player_skill_state(&params.skill_roster, current_player)
            .map(|skills| skills.dash_charges > 0)
            .unwrap_or(false)
        && actions.iter().any(PlannedAction::is_move);

    if actions.len() == 1 && !can_offer_dash {
        execute_action_from_params(actions[0], roll_value, &mut params);
        return;
    }

    set_pending_actions(
        &mut params.input_state,
        roll_value,
        actions,
        &mut params.next_phase,
    );
    if can_offer_dash {
        params.input_state.prompt = Some(format!(
            "Rolled {}. Press E for Dash (+3), then click a highlighted piece or press {}",
            roll_value,
            (1..=params.input_state.candidate_piece_ids().len())
                .map(|index| index.to_string())
                .collect::<Vec<_>>()
                .join("/")
        ));
    }
}

/// 人类玩家“选棋阶段”键盘输入处理（1~4 选择动作）。
fn handle_human_action_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    game_phase: Res<State<GamePhase>>,
    mut params: TurnActionParams,
) {
    if !matches!(game_phase.get(), GamePhase::AwaitPieceSelect) || params.match_result.finished {
        return;
    }

    refresh_pending_actions_for_dash(
        &mut params.input_state,
        &params.turn_state,
        &params.player_roster,
        &mut params.piece_query,
        &params.skill_roster,
        &mut params.next_phase,
    );

    let Some(selection) =
        pressed_selection_key(&keyboard, params.input_state.candidate_piece_ids().len())
    else {
        return;
    };
    let Some(action) = get_pending_action(&params.input_state, selection) else {
        return;
    };

    let roll_value = params.turn_state.last_roll.unwrap_or_default();
    let current_player = params.turn_state.current_player;
    execute_action_from_params(action, roll_value, &mut params);
    clear_dash_arm(&mut params.skill_roster, current_player);
}

/// 人类玩家“选棋阶段”鼠标点击处理（点击高亮棋子）。
fn handle_human_action_click(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform)>,
    game_phase: Res<State<GamePhase>>,
    mut params: TurnActionParams,
) {
    if !matches!(game_phase.get(), GamePhase::AwaitPieceSelect) || params.match_result.finished {
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
        &mut params.input_state,
        &params.turn_state,
        &params.player_roster,
        &mut params.piece_query,
        &params.skill_roster,
        &mut params.next_phase,
    );

    let mut selected_piece_id = None;
    let mut best_distance_sq = f32::MAX;
    for (piece_id, _, _, transform) in &mut params.piece_query {
        if !params
            .input_state
            .candidate_piece_ids()
            .contains(&piece_id.0)
        {
            continue;
        }

        let distance_sq = transform
            .translation
            .truncate()
            .distance_squared(cursor_world);
        if distance_sq <= 28.0 * 28.0 && distance_sq < best_distance_sq {
            best_distance_sq = distance_sq;
            selected_piece_id = Some(piece_id.0);
        }
    }

    let Some(selected_piece_id) = selected_piece_id else {
        return;
    };
    let Some(action) = find_pending_action_by_piece_id(&params.input_state, selected_piece_id)
    else {
        return;
    };

    let roll_value = params.turn_state.last_roll.unwrap_or_default();
    let current_player = params.turn_state.current_player;
    execute_action_from_params(action, roll_value, &mut params);
    clear_dash_arm(&mut params.skill_roster, current_player);
}

/// Dash 预备后刷新候选动作，确保 UI 与可选列表同步。
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::piece::PieceStatus;
    use crate::domain::player::PlayerControl;
    use crate::gameplay::ai::AiDifficulty;
    use crate::gameplay::match_flow::{
        MatchConfig, MatchSetup, PlayerColorChoice, PlayerRoster, build_match_rosters,
    };
    use crate::gameplay::skill_flow::{
        build_skill_roster, player_skill_state, sync_turn_skill_usage,
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

    fn match_config(mode: GameMode, ai_difficulty: AiDifficulty) -> MatchConfig {
        MatchConfig {
            mode,
            ai_difficulty,
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

    fn set_ai_skill_charges(
        skill_roster: &mut SkillRoster,
        player_id: u8,
        snipe: u8,
        shield: u8,
        swap: u8,
        double_dice: u8,
        dash: u8,
    ) {
        if let Some(state) = skill_roster
            .players
            .iter_mut()
            .find(|state| state.player_id == player_id)
        {
            state.snipe_charges = snipe;
            state.shield_charges = shield;
            state.swap_charges = swap;
            state.double_dice_charges = double_dice;
            state.double_dice_armed = false;
            state.dash_charges = dash;
            state.dash_armed = false;
        }
    }

    fn spawn_test_piece(
        world: &mut World,
        piece_id: u8,
        owner_player_id: u8,
        team_id: u8,
        progress: u8,
        shield: u8,
    ) {
        world.spawn((
            PieceId(piece_id),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id,
                team_id,
                status: PieceStatus::Active,
                progress,
                shield,
                stack_shield: 0,
            },
            Transform::default(),
        ));
    }

    #[test]
    fn maybe_use_ai_skills_does_not_arm_dash_before_roll() {
        let match_config = match_config(GameMode::OneVsOne, AiDifficulty::Normal);
        let (players, _) = build_match_rosters(&setup(GameMode::OneVsOne));
        let player_roster = PlayerRoster { players };
        let mut skill_roster = build_skill_roster(&player_roster);
        sync_turn_skill_usage(&mut skill_roster, 2);
        set_ai_skill_charges(&mut skill_roster, 2, 0, 0, 0, 0, 1);

        let mut world = World::new();
        spawn_test_piece(&mut world, 1, 2, 2, 1, 0);
        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world);

        maybe_use_ai_skills(2, &match_config, &mut skill_roster, &mut query);
        let state = player_skill_state(&skill_roster, 2).unwrap();
        assert!(!state.dash_armed);
        assert!(!skill_roster.skill_used_this_turn);
    }

    #[test]
    fn easy_ai_does_not_use_skills_even_with_targets() {
        let match_config = match_config(GameMode::OneVsOne, AiDifficulty::Easy);
        let (players, _) = build_match_rosters(&setup(GameMode::OneVsOne));
        let player_roster = PlayerRoster { players };
        let mut skill_roster = build_skill_roster(&player_roster);
        sync_turn_skill_usage(&mut skill_roster, 2);

        let mut world = World::new();
        spawn_test_piece(&mut world, 1, 2, 2, 4, 0);
        spawn_test_piece(&mut world, 2, 1, 1, 12, 0);
        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world);

        maybe_use_ai_skills(2, &match_config, &mut skill_roster, &mut query);

        let state = player_skill_state(&skill_roster, 2).unwrap();
        assert_eq!(state.snipe_charges, 1);
        assert_eq!(state.shield_charges, 1);
        assert!(!skill_roster.skill_used_this_turn);
    }

    #[test]
    fn normal_ai_prefers_shield_over_opening_snipe() {
        let match_config = match_config(GameMode::OneVsOne, AiDifficulty::Normal);
        let (players, _) = build_match_rosters(&setup(GameMode::OneVsOne));
        let player_roster = PlayerRoster { players };
        let mut skill_roster = build_skill_roster(&player_roster);
        sync_turn_skill_usage(&mut skill_roster, 2);

        let mut world = World::new();
        spawn_test_piece(&mut world, 1, 2, 2, 4, 0);
        spawn_test_piece(&mut world, 2, 1, 1, 12, 0);
        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world);

        maybe_use_ai_skills(2, &match_config, &mut skill_roster, &mut query);

        let state = player_skill_state(&skill_roster, 2).unwrap();
        assert_eq!(state.snipe_charges, 1);
        assert_eq!(state.shield_charges, 0);
        assert!(
            skill_roster
                .last_skill_action
                .as_deref()
                .is_some_and(|note| note.contains("Shield"))
        );
    }

    #[test]
    fn hard_ai_snipe_requires_target_progress_threshold() {
        let match_config = match_config(GameMode::OneVsOne, AiDifficulty::Hard);
        let (players, _) = build_match_rosters(&setup(GameMode::OneVsOne));
        let player_roster = PlayerRoster { players };
        let mut skill_roster = build_skill_roster(&player_roster);
        sync_turn_skill_usage(&mut skill_roster, 2);
        set_ai_skill_charges(&mut skill_roster, 2, 1, 0, 0, 0, 0);

        let mut world = World::new();
        spawn_test_piece(&mut world, 1, 2, 2, 4, 0);
        spawn_test_piece(&mut world, 2, 1, 1, HARD_AI_SNIPE_MIN_PROGRESS - 1, 0);
        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world);

        maybe_use_ai_skills(2, &match_config, &mut skill_roster, &mut query);

        let state = player_skill_state(&skill_roster, 2).unwrap();
        assert_eq!(state.snipe_charges, 1);
        assert!(!skill_roster.skill_used_this_turn);
    }

    #[test]
    fn hard_ai_uses_snipe_on_advanced_target() {
        let match_config = match_config(GameMode::OneVsOne, AiDifficulty::Hard);
        let (players, _) = build_match_rosters(&setup(GameMode::OneVsOne));
        let player_roster = PlayerRoster { players };
        let mut skill_roster = build_skill_roster(&player_roster);
        sync_turn_skill_usage(&mut skill_roster, 2);
        set_ai_skill_charges(&mut skill_roster, 2, 1, 0, 0, 0, 0);

        let mut world = World::new();
        spawn_test_piece(&mut world, 1, 2, 2, 4, 0);
        spawn_test_piece(&mut world, 2, 1, 1, HARD_AI_SNIPE_MIN_PROGRESS, 0);
        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world);

        maybe_use_ai_skills(2, &match_config, &mut skill_roster, &mut query);

        let state = player_skill_state(&skill_roster, 2).unwrap();
        assert_eq!(state.snipe_charges, 0);
        assert!(skill_roster.skill_used_this_turn);
    }

    #[test]
    fn maybe_arm_dash_for_ai_after_roll_arms_dash_when_move_exists() {
        let (players, _) = build_match_rosters(&setup(GameMode::OneVsOne));
        let player_roster = PlayerRoster { players };
        let mut skill_roster = build_skill_roster(&player_roster);
        sync_turn_skill_usage(&mut skill_roster, 2);
        set_ai_skill_charges(&mut skill_roster, 2, 0, 0, 0, 0, 1);

        let mut world = World::new();
        spawn_test_piece(&mut world, 1, 2, 2, 1, 0);
        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world);

        maybe_arm_dash_for_ai_after_roll(
            2,
            AiDifficulty::Normal,
            DiceRoll(2),
            &player_roster,
            &mut skill_roster,
            &mut query,
        );
        let state = player_skill_state(&skill_roster, 2).unwrap();
        assert!(state.dash_armed);
        assert!(skill_roster.skill_used_this_turn);
    }
}
