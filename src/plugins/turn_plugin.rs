use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::data::game_mode::GameMode;
use crate::domain::dice::DiceRoll;
use crate::domain::piece::{PieceProgress, PieceState};
use crate::domain::player::PlayerControl;
use crate::domain::rules::LaunchRule;
use crate::gameplay::ai::AiDifficulty;
use crate::gameplay::match_flow::{
    BoardLayout, MatchConfig, MatchResult, PlayerRoster, TeamRoster,
};
use crate::gameplay::skill_flow::{
    MAX_PIECE_SHIELD, RollResolution, SkillRoster, arm_dash, arm_double_dice,
    can_use_skill_this_turn, clear_dash_arm, dash_bonus, is_current_player_swap_piece,
    is_legal_shield_target, is_legal_snipe_target, is_legal_swap_target, mark_skill_used,
    player_skill_state, record_skill_action, resolve_roll_value, spend_shield_charge,
    spend_snipe_charge, spend_swap_charge,
};
use crate::gameplay::swap_flow::execute_swap;
use crate::gameplay::turn_flow::{
    ActionResources, ActionState, PlannedAction, TurnInputState, TurnState, choose_action,
    choose_pending_double_dice, clear_pending_input, collect_actions, current_player_control,
    execute_action, find_pending_action_by_piece_id, finish_turn_without_action,
    get_pending_action, human_roll_is_ready, player_has_finished_all_pieces, pressed_selection_key,
    record_turn_action, roll_die, set_pending_actions, set_pending_double_dice_choice,
    set_roll_with_faces, skip_current_player_turn,
};
use crate::i18n::{Language, LanguageSettings};
use crate::platform::{DeviceProfile, PointerInputState};
use crate::plugins::animation_plugin::MovingPieceQuery;
use crate::plugins::boot_plugin::AutoplayMatch;
use crate::plugins::effects_plugin::{
    EffectRevealDelays, PieceMotionEffects, TARGETED_MISSILE_REVEAL_DURATION, VisualEffectQueue,
};
use crate::plugins::menu_plugin::{SoundSettingsOverlayState, sound_settings_overlay_blocks_input};
use crate::plugins::piece_plugin::{
    HangarSlot, MoveTargetGuidePiece, PieceId, move_target_guide_infos, pick_move_target_guide,
};
use crate::plugins::skill_plugin::{SkillTargetState, SkillUiAction};
use crate::plugins::ui_plugin::{PlayerHudState, player_hud_point_is_interactive};
use crate::states::{AppState, GamePhase};

/// 回合驱动插件：整合 AI/人类输入并推进回合结算流程。
pub struct TurnPlugin;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TurnSystems;

impl Plugin for TurnPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), setup_turn_automation)
            .add_systems(
                Update,
                (
                    skip_completed_player_turn,
                    drive_ai_turn_loop,
                    handle_human_roll_input,
                    handle_human_action_input,
                    handle_human_action_click,
                )
                    .chain()
                    .in_set(TurnSystems)
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

const DOUBLE_DICE_CHOICE_OFFSET_X: f32 = 38.0;
const DOUBLE_DICE_CHOICE_MIN_PICK_RADIUS: f32 = 40.0;

fn double_dice_resolution_note(player_id: u8, dice: [u8; 2], chosen_roll: u8) -> String {
    format!(
        "P{} resolved DoubleDice: rolled {}/{} and chose {}",
        player_id, dice[0], dice[1], chosen_roll
    )
}

fn double_dice_choice_prompt(faces: [u8; 2], language: Language) -> String {
    match language {
        Language::SimplifiedChinese => format!(
            "双骰掷出 {}/{}。按 1/2 或点击一个骰子选择点数。",
            faces[0], faces[1]
        ),
        Language::English => format!(
            "DoubleDice rolled {}/{}. Press 1/2 or click one die to choose.",
            faces[0], faces[1]
        ),
    }
}

fn resolve_roll_for_rule_set(
    skill_roster: &mut SkillRoster,
    player_id: u8,
    skills_enabled: bool,
) -> RollResolution {
    if skills_enabled {
        resolve_roll_value(skill_roster, player_id)
    } else {
        let value = roll_die();
        RollResolution {
            value,
            dice: [value, 0],
            used_double_dice: false,
        }
    }
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

    pub(crate) fn take_roll(&mut self) -> bool {
        let requested = self.roll_requested;
        self.roll_requested = false;
        requested
    }
}

type TurnPieceQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static PieceId,
        &'static HangarSlot,
        &'static mut PieceState,
        &'static mut Transform,
    ),
>;

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
    language_settings: Res<'w, LanguageSettings>,
    skill_roster: ResMut<'w, SkillRoster>,
    effect_queue: ResMut<'w, VisualEffectQueue>,
    reveal_delays: ResMut<'w, EffectRevealDelays>,
    motion_effects: ResMut<'w, PieceMotionEffects>,
    match_result: ResMut<'w, MatchResult>,
    piece_query: TurnPieceQuery<'w, 's>,
    moving_pieces: MovingPieceQuery<'w, 's>,
}

fn execute_action_from_params(
    action: PlannedAction,
    roll_value: u8,
    params: &mut TurnActionParams,
) {
    let movement_roll_value = roll_value.saturating_add(dash_bonus(
        &params.skill_roster,
        params.turn_state.current_player,
    ));
    execute_action(
        action,
        roll_value,
        movement_roll_value,
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

fn setup_turn_automation(
    mut commands: Commands,
    match_config: Option<Res<MatchConfig>>,
    autoplay: Option<Res<AutoplayMatch>>,
) {
    // AI 行为节拍与人类输入缓存在进入对局时统一初始化。
    let interval = if autoplay.is_some() {
        0.02
    } else if match_config
        .as_deref()
        .is_some_and(|match_config| match_config.fast_mode)
    {
        0.12
    } else {
        0.9
    };
    commands.insert_resource(TurnAutomation {
        timer: Timer::from_seconds(interval, TimerMode::Repeating),
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

fn skip_completed_player_turn(game_phase: Res<State<GamePhase>>, mut params: TurnActionParams) {
    if !matches!(game_phase.get(), GamePhase::AwaitDice)
        || !current_player_should_skip_turn(
            &params.match_config,
            &params.match_result,
            &params.turn_state,
            &params.piece_query,
        )
    {
        return;
    }

    let current_player = params.turn_state.current_player;
    clear_pending_input(&mut params.input_state);
    params.turn_ui_request.roll_requested = false;
    clear_dash_arm(&mut params.skill_roster, current_player);
    record_turn_action(
        &mut params.turn_state,
        format!("P{current_player} completed all pieces; skipped turn"),
    );
    skip_current_player_turn(
        &mut params.turn_state,
        params.player_roster.players.len() as u8,
    );
    params.next_phase.set(GamePhase::AwaitDice);
}

fn current_player_should_skip_turn(
    match_config: &MatchConfig,
    match_result: &MatchResult,
    turn_state: &TurnState,
    piece_query: &TurnPieceQuery,
) -> bool {
    !match_result.finished
        && match_config.mode == GameMode::TwoVsTwo
        && turn_state.current_roll.is_none()
        && player_has_finished_all_pieces(
            turn_state.current_player,
            piece_query.iter().map(|(_, _, piece_state, _)| piece_state),
        )
}

/// AI 回合驱动主循环：定时触发掷骰、选动作并执行完整结算。
fn drive_ai_turn_loop(
    time: Res<Time>,
    mut automation: ResMut<TurnAutomation>,
    game_phase: Res<State<GamePhase>>,
    autoplay: Option<Res<AutoplayMatch>>,
    mut params: TurnActionParams,
) {
    if !matches!(game_phase.get(), GamePhase::AwaitDice) || params.match_result.finished {
        return;
    }

    let should_autoplay = autoplay.is_some()
        || current_player_control(params.turn_state.current_player, &params.player_roster)
            == Some(PlayerControl::Ai);
    if !should_autoplay {
        return;
    }

    if roll_result_waiting_for_display(&params.turn_state) {
        return;
    }

    if let Some(roll_value) = params.turn_state.current_roll {
        finish_ai_roll_after_display(roll_value, &mut params);
        return;
    }

    if !automation.timer.tick(time.delta()).just_finished() {
        return;
    }

    maybe_use_ai_skills(
        params.turn_state.current_player,
        params.turn_state.turn_index,
        &params.match_config,
        &params.board_layout,
        &params.player_roster,
        &mut params.skill_roster,
        &mut params.effect_queue,
        &mut params.reveal_delays,
        &mut params.motion_effects,
        &mut params.piece_query,
        &params.moving_pieces,
    );

    let roll_resolution = resolve_roll_for_rule_set(
        &mut params.skill_roster,
        params.turn_state.current_player,
        params.match_config.rule_set.skills_enabled(),
    );
    let roll_value = roll_resolution.value;
    set_roll_with_faces(&mut params.turn_state, roll_value, roll_resolution.dice);
    if roll_resolution.used_double_dice {
        record_skill_action(
            &mut params.skill_roster,
            params.turn_state.turn_index,
            params.turn_state.current_player,
            double_dice_resolution_note(
                params.turn_state.current_player,
                roll_resolution.dice,
                roll_value,
            ),
        );
    }
}

fn roll_result_waiting_for_display(turn_state: &TurnState) -> bool {
    turn_state.pending_roll_display.is_some()
        && (turn_state.current_roll.is_some() || turn_state.pending_double_dice_choice.is_some())
}

fn finish_ai_roll_after_display(roll_value: u8, params: &mut TurnActionParams) {
    let current_player = params.turn_state.current_player;
    let roll = DiceRoll(roll_value);
    maybe_arm_dash_for_ai_after_roll(
        current_player,
        params.turn_state.turn_index,
        AiDashEvaluation {
            ai_difficulty: params.match_config.ai_difficulty,
            skills_enabled: params.match_config.rule_set.skills_enabled(),
            roll,
            launch_rule: params.match_config.launch_rule,
            board_layout: &params.board_layout,
            player_roster: &params.player_roster,
        },
        &mut params.skill_roster,
        &mut params.piece_query,
    );
    let Some(action) = choose_action(
        current_player,
        roll,
        dash_bonus(&params.skill_roster, current_player),
        params.match_config.launch_rule,
        &params.board_layout,
        &params.player_roster,
        &params.piece_query,
    ) else {
        record_turn_action(
            &mut params.turn_state,
            format!("P{current_player} rolled {roll_value} but had no legal action"),
        );
        finish_turn_without_action(
            &mut params.turn_state,
            &mut params.input_state,
            &params.player_roster,
            &mut params.next_phase,
        );
        return;
    };

    execute_action_from_params(action, roll_value, params);
    clear_dash_arm(&mut params.skill_roster, current_player);
}

const HARD_AI_SNIPE_MIN_PROGRESS: PieceProgress = 8;

/// AI 技能策略：Easy 不主动用技能，Normal 偏防御/起飞，Hard 才主动攻击与换位。
fn maybe_use_ai_skills(
    current_player: u8,
    turn_index: u32,
    match_config: &MatchConfig,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
    skill_roster: &mut SkillRoster,
    effect_queue: &mut VisualEffectQueue,
    reveal_delays: &mut EffectRevealDelays,
    motion_effects: &mut PieceMotionEffects,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
    moving_pieces: &MovingPieceQuery,
) {
    if !match_config.rule_set.skills_enabled() {
        return;
    }

    if !can_use_skill_this_turn(skill_roster, current_player) {
        return;
    }

    match match_config.ai_difficulty {
        AiDifficulty::Easy => {}
        AiDifficulty::Normal => {
            let _ = try_ai_shield(
                current_player,
                turn_index,
                skill_roster,
                effect_queue,
                piece_query,
            ) || try_ai_double_dice(current_player, turn_index, skill_roster, piece_query);
        }
        AiDifficulty::Hard => {
            let _ = try_ai_snipe(
                current_player,
                turn_index,
                HARD_AI_SNIPE_MIN_PROGRESS,
                skill_roster,
                effect_queue,
                reveal_delays,
                motion_effects,
                piece_query,
            ) || try_ai_shield(
                current_player,
                turn_index,
                skill_roster,
                effect_queue,
                piece_query,
            ) || try_ai_swap(
                current_player,
                turn_index,
                match_config.mode,
                board_layout,
                player_roster,
                skill_roster,
                piece_query,
                moving_pieces,
            ) || try_ai_double_dice(current_player, turn_index, skill_roster, piece_query);
        }
    }
}

fn try_ai_snipe(
    current_player: u8,
    turn_index: u32,
    minimum_target_progress: PieceProgress,
    skill_roster: &mut SkillRoster,
    effect_queue: &mut VisualEffectQueue,
    reveal_delays: &mut EffectRevealDelays,
    motion_effects: &mut PieceMotionEffects,
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
    if let Some(target_world) = piece_world_position_for_turn_query(target_piece_id, piece_query) {
        effect_queue.hud_skill_missile(SkillUiAction::Snipe, target_world);
    }
    if piece_personal_shield_for_turn_query(target_piece_id, piece_query).unwrap_or_default() > 0 {
        reveal_delays.delay_shield_loss(target_piece_id, TARGETED_MISSILE_REVEAL_DURATION);
    }
    if snipe_will_send_to_hangar_for_turn_query(target_piece_id, piece_query) {
        motion_effects.delay_piece_motion(target_piece_id, TARGETED_MISSILE_REVEAL_DURATION);
    }
    let message = execute_snipe_on_turn_query(target_piece_id, piece_query, true);
    record_skill_action(skill_roster, turn_index, current_player, message);
    true
}

fn try_ai_shield(
    current_player: u8,
    turn_index: u32,
    skill_roster: &mut SkillRoster,
    effect_queue: &mut VisualEffectQueue,
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
        && let Some(target_world) =
            piece_world_position_for_turn_query(target_piece_id, piece_query)
        && let Some(shield_value) =
            apply_shield_to_piece_to_turn_query(target_piece_id, piece_query)
    {
        mark_skill_used(skill_roster, current_player);
        effect_queue.shield_flash(target_world);
        record_skill_action(
            skill_roster,
            turn_index,
            current_player,
            format!(
                "P{} (AI) used Shield on piece #{} ({})",
                current_player, target_piece_id, shield_value
            ),
        );
        return true;
    }

    false
}

fn try_ai_swap(
    current_player: u8,
    turn_index: u32,
    mode: GameMode,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
    skill_roster: &mut SkillRoster,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
    moving_pieces: &MovingPieceQuery,
) -> bool {
    let can_use_swap = player_skill_state(skill_roster, current_player)
        .map(|skills| skills.swap_charges > 0)
        .unwrap_or(false);
    if !can_use_swap {
        return false;
    }

    let Some(target_piece_id) = preferred_ai_swap_target(current_player, mode, piece_query) else {
        return false;
    };

    let result = execute_swap(
        current_player,
        mode,
        target_piece_id,
        board_layout,
        player_roster,
        piece_query,
        moving_pieces,
    );
    if !result.starts_with("Swap exchanged piece #") {
        return false;
    }
    spend_swap_charge(skill_roster, current_player);
    mark_skill_used(skill_roster, current_player);
    let message = format!("AI {result}");
    record_skill_action(skill_roster, turn_index, current_player, message);
    true
}

fn try_ai_double_dice(
    current_player: u8,
    turn_index: u32,
    skill_roster: &mut SkillRoster,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> bool {
    if should_ai_arm_double_dice(current_player, skill_roster, piece_query)
        && arm_double_dice(skill_roster, current_player)
    {
        mark_skill_used(skill_roster, current_player);
        record_skill_action(
            skill_roster,
            turn_index,
            current_player,
            format!(
                "P{} (AI) armed DoubleDice for launch pressure",
                current_player
            ),
        );
        return true;
    }

    false
}

/// AI 在掷骰后评估是否需要临时预备 Dash（仅当存在可移动动作）。
struct AiDashEvaluation<'a> {
    ai_difficulty: AiDifficulty,
    skills_enabled: bool,
    roll: DiceRoll,
    launch_rule: LaunchRule,
    board_layout: &'a BoardLayout,
    player_roster: &'a PlayerRoster,
}

fn maybe_arm_dash_for_ai_after_roll(
    current_player: u8,
    turn_index: u32,
    evaluation: AiDashEvaluation,
    skill_roster: &mut SkillRoster,
    piece_query: &mut TurnPieceQuery,
) {
    if evaluation.ai_difficulty == AiDifficulty::Easy {
        return;
    }

    if !evaluation.skills_enabled {
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

    let has_movable_action = collect_actions(
        current_player,
        evaluation.roll,
        0,
        evaluation.launch_rule,
        evaluation.board_layout,
        evaluation.player_roster,
        piece_query,
    )
    .iter()
    .any(PlannedAction::is_move);
    if !has_movable_action {
        return;
    }

    if arm_dash(skill_roster, current_player) {
        mark_skill_used(skill_roster, current_player);
        record_skill_action(
            skill_roster,
            turn_index,
            current_player,
            format!(
                "P{} (AI) armed Dash after roll for +3 movement",
                current_player
            ),
        );
    }
}

/// 选择 AI 的 Snipe 目标：优先无盾、再有盾，且不攻击队友。
fn preferred_ai_snipe_target(
    current_player: u8,
    minimum_target_progress: PieceProgress,
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

/// 选择 AI 的 Swap 目标：2v2 找更靠前的队友，其他模式找更靠前的敌机。
fn preferred_ai_swap_target(
    current_player: u8,
    mode: GameMode,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> Option<u8> {
    let mut own_progress = None;
    let mut own_team = None;

    for (_, _, piece_state, _) in piece_query.iter_mut() {
        if is_current_player_swap_piece(current_player, &piece_state) {
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
            is_legal_swap_target(current_player, own_team, mode, piece_state)
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

/// 读取 turn_query 中棋子的世界坐标（供 AI 特效路径调用）。
fn piece_world_position_for_turn_query(
    piece_id: u8,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> Option<Vec2> {
    piece_query
        .iter_mut()
        .find(|(query_piece_id, _, _, _)| query_piece_id.0 == piece_id)
        .map(|(_, _, _, transform)| transform.translation.truncate())
}

fn piece_personal_shield_for_turn_query(
    piece_id: u8,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> Option<u8> {
    piece_query
        .iter_mut()
        .find(|(query_piece_id, _, _, _)| query_piece_id.0 == piece_id)
        .map(|(_, _, piece_state, _)| piece_state.shield)
}

fn snipe_will_send_to_hangar_for_turn_query(
    piece_id: u8,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> bool {
    piece_query
        .iter_mut()
        .find(|(query_piece_id, _, _, _)| query_piece_id.0 == piece_id)
        .is_some_and(|(_, _, piece_state, _)| {
            piece_state.shield == 0 && piece_state.stack_shield == 0
        })
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

/// 人类玩家掷骰输入（中心骰子、HUD 按钮或 Space）。
fn handle_human_roll_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    pointer: Res<PointerInputState>,
    device_profile: Res<DeviceProfile>,
    windows: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform)>,
    game_phase: Res<State<GamePhase>>,
    overlay_state: Res<SoundSettingsOverlayState>,
    hud_state: Res<PlayerHudState>,
    skill_target: Res<SkillTargetState>,
    mut params: TurnActionParams,
) {
    // Drain even while blocked or when Space is also pressed; no stale click may
    // survive an animation/choice and unexpectedly roll in a later turn.
    let ui_roll_requested = params.turn_ui_request.take_roll();
    if sound_settings_overlay_blocks_input(&overlay_state)
        || skill_target.is_active()
        || !matches!(game_phase.get(), GamePhase::AwaitDice)
        || params.match_result.finished
    {
        return;
    }

    if current_player_control(params.turn_state.current_player, &params.player_roster)
        != Some(PlayerControl::Human)
    {
        return;
    }

    if let Some(choice) = params.turn_state.pending_double_dice_choice {
        if roll_result_waiting_for_display(&params.turn_state) {
            return;
        }

        params.input_state.prompt = Some(double_dice_choice_prompt(
            choice.faces,
            params.language_settings.language,
        ));
        let keyboard_selection = pressed_double_dice_choice_key(&keyboard);
        let pointer_selection = keyboard_selection.is_none().then(|| {
            clicked_double_dice_choice(
                &pointer,
                *device_profile,
                &windows,
                &camera_query,
                &hud_state,
                &params.player_roster,
                params.match_config.rule_set.skills_enabled(),
                choice.faces,
            )
        });
        let Some(selection) = keyboard_selection.or(pointer_selection.flatten()) else {
            return;
        };
        let Some(roll_value) = choose_pending_double_dice(&mut params.turn_state, selection) else {
            return;
        };
        params.input_state.prompt = None;
        record_skill_action(
            &mut params.skill_roster,
            params.turn_state.turn_index,
            params.turn_state.current_player,
            double_dice_resolution_note(params.turn_state.current_player, choice.faces, roll_value),
        );
        finish_human_roll_after_display(roll_value, &mut params);
        return;
    }

    if roll_result_waiting_for_display(&params.turn_state) {
        return;
    }

    params.input_state.prompt = None;

    if let Some(roll_value) = params.turn_state.current_roll {
        finish_human_roll_after_display(roll_value, &mut params);
        return;
    }

    if !human_roll_is_ready(
        &params.turn_state,
        game_phase.get(),
        &params.player_roster,
        &params.match_result,
    ) || (!keyboard.just_pressed(KeyCode::Space) && !ui_roll_requested)
    {
        return;
    }

    let roll_resolution = resolve_roll_for_rule_set(
        &mut params.skill_roster,
        params.turn_state.current_player,
        params.match_config.rule_set.skills_enabled(),
    );
    let roll_value = roll_resolution.value;
    if roll_resolution.used_double_dice {
        set_pending_double_dice_choice(&mut params.turn_state, roll_resolution.dice);
        params.input_state.prompt = Some(double_dice_choice_prompt(
            roll_resolution.dice,
            params.language_settings.language,
        ));
        return;
    }

    set_roll_with_faces(&mut params.turn_state, roll_value, roll_resolution.dice);
}

fn pressed_double_dice_choice_key(keyboard: &ButtonInput<KeyCode>) -> Option<usize> {
    if keyboard.just_pressed(KeyCode::Digit1) {
        return Some(0);
    }
    if keyboard.just_pressed(KeyCode::Digit2) {
        return Some(1);
    }
    None
}

fn clicked_double_dice_choice(
    pointer: &PointerInputState,
    device_profile: DeviceProfile,
    windows: &Query<&Window>,
    camera_query: &Query<(&Camera, &GlobalTransform)>,
    hud_state: &PlayerHudState,
    player_roster: &PlayerRoster,
    skills_enabled: bool,
    faces: [u8; 2],
) -> Option<usize> {
    if !pointer.just_pressed() {
        return None;
    }
    let pointer_position = pointer.just_pressed_position()?;
    let window = windows.single().ok()?;
    if player_hud_point_is_interactive(
        pointer_position,
        window,
        device_profile,
        player_roster,
        hud_state,
        skills_enabled,
    ) {
        return None;
    }
    let (camera, camera_transform) = camera_query.single().ok()?;
    let cursor_world = camera
        .viewport_to_world_2d(camera_transform, pointer_position)
        .ok()?;
    double_dice_choice_at_world(
        cursor_world,
        faces,
        device_profile
            .piece_pick_radius_world()
            .max(DOUBLE_DICE_CHOICE_MIN_PICK_RADIUS),
    )
}

fn double_dice_choice_at_world(
    cursor_world: Vec2,
    faces: [u8; 2],
    pick_radius: f32,
) -> Option<usize> {
    let has_second_die = (1..=6).contains(&faces[1]);
    let mut best_index = None;
    let mut best_distance_sq = f32::MAX;
    let pick_radius_sq = pick_radius * pick_radius;

    for (index, face) in faces.iter().enumerate() {
        if !(1..=6).contains(face) || (!has_second_die && index > 0) {
            continue;
        }

        let center = if has_second_die {
            Vec2::new(
                if index == 0 {
                    -DOUBLE_DICE_CHOICE_OFFSET_X
                } else {
                    DOUBLE_DICE_CHOICE_OFFSET_X
                },
                0.0,
            )
        } else {
            Vec2::ZERO
        };
        let distance_sq = center.distance_squared(cursor_world);
        if distance_sq <= pick_radius_sq && distance_sq < best_distance_sq {
            best_distance_sq = distance_sq;
            best_index = Some(index);
        }
    }

    best_index
}

fn finish_human_roll_after_display(roll_value: u8, params: &mut TurnActionParams) {
    let current_player = params.turn_state.current_player;
    let roll = DiceRoll(roll_value);
    let actions = collect_actions(
        current_player,
        roll,
        0,
        params.match_config.launch_rule,
        &params.board_layout,
        &params.player_roster,
        &params.piece_query,
    );

    if actions.is_empty() {
        let current_player = params.turn_state.current_player;
        record_turn_action(
            &mut params.turn_state,
            format!(
                "P{} rolled {} but had no legal action",
                current_player, roll_value
            ),
        );
        finish_turn_without_action(
            &mut params.turn_state,
            &mut params.input_state,
            &params.player_roster,
            &mut params.next_phase,
        );
        return;
    }

    let can_offer_dash = params.match_config.rule_set.skills_enabled()
        && can_use_skill_this_turn(&params.skill_roster, current_player)
        && dash_bonus(&params.skill_roster, current_player) == 0
        && player_skill_state(&params.skill_roster, current_player)
            .map(|skills| skills.dash_charges > 0)
            .unwrap_or(false)
        && actions.iter().any(PlannedAction::is_move);

    if should_auto_execute_human_action(&actions, can_offer_dash) {
        execute_action_from_params(actions[0], roll_value, params);
        return;
    }

    set_pending_actions(
        &mut params.input_state,
        roll_value,
        actions,
        &mut params.next_phase,
    );
    params.input_state.prompt = Some(pending_action_prompt(
        roll_value,
        can_offer_dash,
        params.language_settings.language,
    ));
}

fn should_auto_execute_human_action(_actions: &[PlannedAction], _can_offer_dash: bool) -> bool {
    false
}

fn pending_action_prompt(roll_value: u8, can_offer_dash: bool, language: Language) -> String {
    match (language, can_offer_dash) {
        (Language::SimplifiedChinese, true) => {
            format!("掷出 {}。可点冲刺 +3，或点击高亮飞机。", roll_value)
        }
        (Language::SimplifiedChinese, false) => {
            format!("掷出 {}。点击高亮飞机移动。", roll_value)
        }
        (Language::English, true) => {
            format!("Rolled {roll_value}. Tap Dash +3 or a highlighted aircraft.")
        }
        (Language::English, false) => {
            format!("Rolled {roll_value}. Tap a highlighted aircraft to move.")
        }
    }
}

fn dash_pending_action_prompt(move_bonus: u8, language: Language) -> String {
    match language {
        Language::SimplifiedChinese => format!("冲刺已启用（+{}）。点击高亮飞机。", move_bonus),
        Language::English => {
            format!("Dash armed (+{move_bonus}). Tap a highlighted aircraft.")
        }
    }
}

/// 人类玩家“选棋阶段”键盘输入处理（1~4 选择动作）。
fn handle_human_action_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    game_phase: Res<State<GamePhase>>,
    overlay_state: Res<SoundSettingsOverlayState>,
    mut params: TurnActionParams,
) {
    if overlay_state.open
        || !matches!(game_phase.get(), GamePhase::AwaitPieceSelect)
        || params.match_result.finished
    {
        return;
    }

    refresh_pending_actions_for_dash(
        &mut params.input_state,
        DashRefreshContext {
            turn_state: &params.turn_state,
            board_layout: &params.board_layout,
            player_roster: &params.player_roster,
            skill_roster: &params.skill_roster,
            launch_rule: params.match_config.launch_rule,
            language: params.language_settings.language,
        },
        &mut params.piece_query,
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
    pointer: Res<PointerInputState>,
    device_profile: Res<DeviceProfile>,
    windows: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform)>,
    game_phase: Res<State<GamePhase>>,
    overlay_state: Res<SoundSettingsOverlayState>,
    hud_state: Res<PlayerHudState>,
    mut params: TurnActionParams,
) {
    if !matches!(game_phase.get(), GamePhase::AwaitPieceSelect) || params.match_result.finished {
        return;
    }

    if sound_settings_overlay_blocks_input(&overlay_state) || !pointer.just_pressed() {
        return;
    };
    let Some(pointer_position) = pointer.just_pressed_position() else {
        return;
    };
    let Ok(window) = windows.single() else {
        return;
    };
    if player_hud_point_is_interactive(
        pointer_position,
        window,
        *device_profile,
        &params.player_roster,
        &hud_state,
        params.match_config.rule_set.skills_enabled(),
    ) {
        return;
    }
    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };
    let Ok(cursor_world) = camera.viewport_to_world_2d(camera_transform, pointer_position) else {
        return;
    };

    refresh_pending_actions_for_dash(
        &mut params.input_state,
        DashRefreshContext {
            turn_state: &params.turn_state,
            board_layout: &params.board_layout,
            player_roster: &params.player_roster,
            skill_roster: &params.skill_roster,
            launch_rule: params.match_config.launch_rule,
            language: params.language_settings.language,
        },
        &mut params.piece_query,
        &mut params.next_phase,
    );

    let pick_radius = device_profile.piece_pick_radius_world();
    let guide_pieces = params
        .piece_query
        .iter()
        .map(
            |(piece_id, _, piece_state, transform)| MoveTargetGuidePiece {
                piece_id: piece_id.0,
                piece_state: *piece_state,
                origin: transform.translation.truncate(),
                connector_origin: transform.translation.truncate(),
            },
        )
        .collect::<Vec<_>>();
    let target_guides = move_target_guide_infos(
        params.input_state.pending_actions(),
        &guide_pieces,
        &params.board_layout,
        &params.player_roster,
    );
    if let Some(selected_piece_id) =
        pick_move_target_guide(&target_guides, cursor_world, pick_radius)
    {
        let Some(action) = find_pending_action_by_piece_id(&params.input_state, selected_piece_id)
        else {
            return;
        };

        let roll_value = params.turn_state.last_roll.unwrap_or_default();
        let current_player = params.turn_state.current_player;
        execute_action_from_params(action, roll_value, &mut params);
        clear_dash_arm(&mut params.skill_roster, current_player);
        return;
    }

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
        if distance_sq <= pick_radius * pick_radius && distance_sq < best_distance_sq {
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
struct DashRefreshContext<'a> {
    turn_state: &'a TurnState,
    board_layout: &'a BoardLayout,
    player_roster: &'a PlayerRoster,
    skill_roster: &'a SkillRoster,
    launch_rule: LaunchRule,
    language: Language,
}

fn refresh_pending_actions_for_dash(
    input_state: &mut TurnInputState,
    context: DashRefreshContext,
    piece_query: &mut TurnPieceQuery,
    next_phase: &mut ResMut<NextState<GamePhase>>,
) {
    let move_bonus = dash_bonus(context.skill_roster, context.turn_state.current_player);
    if move_bonus == 0 || input_state.candidate_piece_ids().is_empty() {
        return;
    }

    let refreshed_actions = collect_actions(
        context.turn_state.current_player,
        DiceRoll(context.turn_state.last_roll.unwrap_or_default()),
        move_bonus,
        context.launch_rule,
        context.board_layout,
        context.player_roster,
        piece_query,
    );
    if refreshed_actions.is_empty() {
        return;
    }

    set_pending_actions(
        input_state,
        context.turn_state.last_roll.unwrap_or_default(),
        refreshed_actions,
        next_phase,
    );
    input_state.prompt = Some(dash_pending_action_prompt(move_bonus, context.language));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::piece::PieceStatus;
    use crate::domain::player::PlayerControl;
    use crate::gameplay::ai::AiDifficulty;
    use crate::gameplay::match_flow::{
        MatchConfig, MatchSetup, PlayerRoster, PlayerSeat, build_match_rosters,
    };
    use crate::gameplay::skill_flow::{
        build_skill_roster, player_skill_state, sync_turn_skill_usage,
    };
    use crate::gameplay::turn_flow::HOME_ENTRY_PROGRESS;
    use crate::plugins::effects_plugin::PieceMotionEffect;
    use bevy::ecs::system::SystemState;

    fn setup(mode: GameMode) -> MatchSetup {
        MatchSetup {
            mode,
            rule_set: crate::data::rule_set::RuleSet::Creative,
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

    fn match_config(mode: GameMode, ai_difficulty: AiDifficulty) -> MatchConfig {
        MatchConfig {
            mode,
            rule_set: crate::data::rule_set::RuleSet::Creative,
            ai_difficulty,
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

    fn human_roll_test_app() -> App {
        let mut app = App::new();
        let (players, teams) = build_match_rosters(&setup(GameMode::TwoVsTwo));
        let players = PlayerRoster::from_players(players);
        let skills = build_skill_roster(&players);
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<PointerInputState>()
            .init_resource::<DeviceProfile>()
            .insert_resource(State::new(GamePhase::AwaitDice))
            .init_resource::<NextState<GamePhase>>()
            .init_resource::<SoundSettingsOverlayState>()
            .init_resource::<PlayerHudState>()
            .init_resource::<TurnInputState>()
            .init_resource::<TurnUiRequest>()
            .insert_resource(TurnState::opening_turn())
            .insert_resource(BoardLayout::default())
            .insert_resource(match_config(GameMode::TwoVsTwo, AiDifficulty::Normal))
            .insert_resource(players)
            .insert_resource(TeamRoster { teams })
            .init_resource::<LanguageSettings>()
            .insert_resource(skills)
            .init_resource::<VisualEffectQueue>()
            .init_resource::<EffectRevealDelays>()
            .init_resource::<PieceMotionEffects>()
            .init_resource::<MatchResult>()
            .init_resource::<SkillTargetState>()
            .add_systems(Update, handle_human_roll_input);
        app
    }

    #[test]
    fn targeting_blocks_same_frame_roll_before_phase_transition_and_drains_request() {
        let mut app = human_roll_test_app();
        app.insert_resource(SkillTargetState::with_swap(
            crate::gameplay::swap_flow::SwapSelection::new(1, 1, vec![1, 2], vec![5, 6]),
        ));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Space);
        app.world_mut().resource_mut::<TurnUiRequest>().queue_roll();
        app.update();
        assert_eq!(app.world().resource::<TurnState>().roll_serial, 0);
        assert!(!app.world().resource::<TurnUiRequest>().roll_requested);
        app.insert_resource(SkillTargetState::default());
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.update();
        assert_eq!(app.world().resource::<TurnState>().roll_serial, 0);
    }

    #[test]
    fn simultaneous_keyboard_and_pointer_roll_is_consumed_once() {
        let mut app = human_roll_test_app();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Space);
        app.world_mut().resource_mut::<TurnUiRequest>().queue_roll();
        app.update();
        assert_eq!(app.world().resource::<TurnState>().roll_serial, 1);
        assert!(
            app.world()
                .resource::<TurnState>()
                .pending_roll_display
                .is_some()
        );
        assert!(!app.world().resource::<TurnUiRequest>().roll_requested);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        // Simulate a later eligible turn; no queued click should survive into it.
        app.insert_resource(TurnState::opening_turn());
        app.update();
        assert_eq!(app.world().resource::<TurnState>().roll_serial, 0);
    }

    #[test]
    fn blocked_roll_requests_never_leak_into_a_later_turn() {
        use crate::gameplay::turn_flow::commit_pending_roll_display;
        for blocked in 0..8 {
            let mut app = human_roll_test_app();
            match blocked {
                0 => {
                    app.world_mut()
                        .resource_mut::<SoundSettingsOverlayState>()
                        .open = true
                }
                1 => {
                    app.world_mut()
                        .resource_mut::<SoundSettingsOverlayState>()
                        .input_captured = true
                }
                2 => {
                    app.insert_resource(State::new(GamePhase::ResolveSkillEffect));
                }
                3 => app.world_mut().resource_mut::<MatchResult>().finished = true,
                4 => app.world_mut().resource_mut::<TurnState>().current_player = 2,
                5 => {
                    set_roll_with_faces(&mut app.world_mut().resource_mut::<TurnState>(), 4, [4, 0])
                }
                _ => {
                    let mut turn = app.world_mut().resource_mut::<TurnState>();
                    set_pending_double_dice_choice(&mut turn, [2, 6]);
                    if blocked == 7 {
                        commit_pending_roll_display(&mut turn, 1);
                    }
                }
            }
            let serial = app.world().resource::<TurnState>().roll_serial;
            app.world_mut().resource_mut::<TurnUiRequest>().queue_roll();
            app.update();
            assert_eq!(
                app.world().resource::<TurnState>().roll_serial,
                serial,
                "blocked={blocked}"
            );
            assert!(!app.world().resource::<TurnUiRequest>().roll_requested);
            app.insert_resource(SoundSettingsOverlayState::default())
                .insert_resource(State::new(GamePhase::AwaitDice))
                .insert_resource(MatchResult::default())
                .insert_resource(TurnState::opening_turn());
            app.update();
            assert_eq!(app.world().resource::<TurnState>().roll_serial, 0);
        }
    }

    fn spawn_test_piece(
        world: &mut World,
        piece_id: u8,
        owner_player_id: u8,
        team_id: u8,
        progress: PieceProgress,
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
                motion_serial: 0,
            },
            Transform::default(),
        ));
    }

    fn spawn_test_piece_with_status(
        world: &mut World,
        piece_id: u8,
        owner_player_id: u8,
        team_id: u8,
        status: PieceStatus,
        progress: PieceProgress,
    ) {
        world.spawn((
            PieceId(piece_id),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id,
                team_id,
                status,
                progress,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::default(),
        ));
    }

    #[test]
    fn preferred_ai_swap_target_ignores_home_lane_source_and_teammate() {
        let mut world = World::new();
        spawn_test_piece(&mut world, 1, 2, 2, 4, 0);
        spawn_test_piece(&mut world, 3, 4, 2, HOME_ENTRY_PROGRESS + 1, 0);
        spawn_test_piece(&mut world, 4, 4, 2, 12, 0);
        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world).unwrap();

        assert_eq!(
            preferred_ai_swap_target(2, GameMode::TwoVsTwo, &mut query),
            Some(4)
        );

        let mut home_lane_source_world = World::new();
        spawn_test_piece(
            &mut home_lane_source_world,
            1,
            2,
            2,
            HOME_ENTRY_PROGRESS + 1,
            0,
        );
        spawn_test_piece(&mut home_lane_source_world, 4, 4, 2, 12, 0);
        let mut source_system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut home_lane_source_world);
        let mut source_query = source_system_state
            .get_mut(&mut home_lane_source_world)
            .unwrap();

        assert_eq!(
            preferred_ai_swap_target(2, GameMode::TwoVsTwo, &mut source_query),
            None
        );
    }

    #[test]
    fn preferred_ai_swap_target_can_pick_enemy_outside_two_vs_two() {
        let mut world = World::new();
        spawn_test_piece(&mut world, 1, 2, 2, 4, 0);
        spawn_test_piece(&mut world, 3, 1, 1, 12, 0);
        spawn_test_piece(&mut world, 4, 3, 3, HOME_ENTRY_PROGRESS + 1, 0);
        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world).unwrap();

        assert_eq!(
            preferred_ai_swap_target(2, GameMode::OneVsOne, &mut query),
            Some(3)
        );
    }

    #[test]
    fn ai_swap_rebases_progress_from_the_same_public_positions_as_human_swap() {
        let (players, _) = build_match_rosters(&setup(GameMode::TwoVsTwo));
        let roster = PlayerRoster::from_players(players);
        let board = BoardLayout::default();
        let mut skills = build_skill_roster(&roster);
        sync_turn_skill_usage(&mut skills, 1);
        set_ai_skill_charges(&mut skills, 1, 0, 0, 1, 0, 0);
        let mut world = World::new();
        spawn_test_piece(&mut world, 1, 1, 1, 3, 0);
        spawn_test_piece(&mut world, 2, 3, 1, 18, 0);
        let mut system: SystemState<(TurnPieceQuery, MovingPieceQuery)> =
            SystemState::new(&mut world);
        let (mut pieces, moving_pieces) = system.get_mut(&mut world).unwrap();
        assert!(try_ai_swap(
            1,
            1,
            GameMode::TwoVsTwo,
            &board,
            &roster,
            &mut skills,
            &mut pieces,
            &moving_pieces,
        ));
        for (id, _, state, transform) in pieces.iter() {
            let (old_owner, old_progress, expected_progress) =
                if id.0 == 1 { (3, 18, 5) } else { (1, 3, 16) };
            let expected = crate::gameplay::turn_flow::world_position_for_piece(
                old_owner,
                old_progress,
                state.status,
                &board,
                &roster,
            )
            .unwrap();
            assert_eq!(state.progress, expected_progress);
            assert_eq!(transform.translation.truncate(), expected);
        }
        assert_eq!(player_skill_state(&skills, 1).unwrap().swap_charges, 0);
    }

    #[test]
    fn ai_swap_does_not_spend_charge_or_turn_opportunity_while_either_piece_moves() {
        use crate::plugins::animation_plugin::PieceMoveAnimation;
        for moving_id in [1, 2] {
            let (players, _) = build_match_rosters(&setup(GameMode::TwoVsTwo));
            let roster = PlayerRoster::from_players(players);
            let board = BoardLayout::default();
            let mut skills = build_skill_roster(&roster);
            sync_turn_skill_usage(&mut skills, 1);
            set_ai_skill_charges(&mut skills, 1, 0, 0, 1, 0, 0);
            let mut world = World::new();
            spawn_test_piece(&mut world, 1, 1, 1, 3, 0);
            spawn_test_piece(&mut world, 2, 3, 1, 18, 0);
            let entity = world
                .query::<(Entity, &PieceId)>()
                .iter(&world)
                .find(|(_, id)| id.0 == moving_id)
                .unwrap()
                .0;
            world
                .entity_mut(entity)
                .insert(PieceMoveAnimation::test_pending());
            let mut system: SystemState<(TurnPieceQuery, MovingPieceQuery)> =
                SystemState::new(&mut world);
            {
                let (mut pieces, moving) = system.get_mut(&mut world).unwrap();
                let before = pieces
                    .iter()
                    .map(|(id, _, state, transform)| (id.0, *state, transform.translation))
                    .collect::<Vec<_>>();
                assert!(!try_ai_swap(
                    1,
                    1,
                    GameMode::TwoVsTwo,
                    &board,
                    &roster,
                    &mut skills,
                    &mut pieces,
                    &moving
                ));
                assert_eq!(
                    before,
                    pieces
                        .iter()
                        .map(|(id, _, state, transform)| (id.0, *state, transform.translation))
                        .collect::<Vec<_>>()
                );
                assert_eq!(player_skill_state(&skills, 1).unwrap().swap_charges, 1);
                assert!(!skills.skill_used_this_turn);
            }
            world.entity_mut(entity).remove::<PieceMoveAnimation>();
            let (mut pieces, moving) = system.get_mut(&mut world).unwrap();
            assert!(try_ai_swap(
                1,
                1,
                GameMode::TwoVsTwo,
                &board,
                &roster,
                &mut skills,
                &mut pieces,
                &moving
            ));
            assert_eq!(player_skill_state(&skills, 1).unwrap().swap_charges, 0);
            assert!(skills.skill_used_this_turn);
        }
    }

    #[test]
    fn completed_player_is_skipped_only_in_two_vs_two_waiting_turns() {
        let mut turn_state = TurnState::opening_turn();
        let mut world = World::new();
        spawn_test_piece_with_status(&mut world, 1, 1, 1, PieceStatus::Finished, 0);
        spawn_test_piece_with_status(&mut world, 2, 1, 1, PieceStatus::Finished, 0);
        spawn_test_piece_with_status(&mut world, 3, 3, 1, PieceStatus::Active, 4);
        spawn_test_piece_with_status(&mut world, 4, 2, 2, PieceStatus::Active, 4);
        let mut system_state: SystemState<TurnPieceQuery> = SystemState::new(&mut world);
        let query = system_state.get_mut(&mut world).unwrap();
        let mut config = match_config(GameMode::TwoVsTwo, AiDifficulty::Normal);
        let result = MatchResult::default();

        assert!(current_player_should_skip_turn(
            &config,
            &result,
            &turn_state,
            &query
        ));

        config.mode = GameMode::FreeForAll;
        assert!(!current_player_should_skip_turn(
            &config,
            &result,
            &turn_state,
            &query
        ));

        config.mode = GameMode::TwoVsTwo;
        turn_state.current_roll = Some(3);
        assert!(!current_player_should_skip_turn(
            &config,
            &result,
            &turn_state,
            &query
        ));
    }

    #[test]
    fn maybe_use_ai_skills_does_not_arm_dash_before_roll() {
        let match_config = match_config(GameMode::OneVsOne, AiDifficulty::Normal);
        let (players, _) = build_match_rosters(&setup(GameMode::OneVsOne));
        let player_roster = PlayerRoster::from_players(players);
        let mut skill_roster = build_skill_roster(&player_roster);
        sync_turn_skill_usage(&mut skill_roster, 2);
        set_ai_skill_charges(&mut skill_roster, 2, 0, 0, 0, 0, 1);

        let mut world = World::new();
        spawn_test_piece(&mut world, 1, 2, 2, 1, 0);
        let mut system_state: SystemState<(
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
            MovingPieceQuery,
        )> = SystemState::new(&mut world);
        let (mut query, moving_query) = system_state.get_mut(&mut world).unwrap();
        let mut effect_queue = VisualEffectQueue::default();
        let mut reveal_delays = EffectRevealDelays::default();
        let mut motion_effects = PieceMotionEffects::default();

        maybe_use_ai_skills(
            2,
            1,
            &match_config,
            &BoardLayout::default(),
            &player_roster,
            &mut skill_roster,
            &mut effect_queue,
            &mut reveal_delays,
            &mut motion_effects,
            &mut query,
            &moving_query,
        );
        let state = player_skill_state(&skill_roster, 2).unwrap();
        assert!(!state.dash_armed);
        assert!(!skill_roster.skill_used_this_turn);
        assert_eq!(effect_queue.pending_count(), 0);
    }

    #[test]
    fn traditional_rule_set_prevents_ai_skill_use() {
        let mut match_config = match_config(GameMode::OneVsOne, AiDifficulty::Hard);
        match_config.rule_set = crate::data::rule_set::RuleSet::Traditional;
        let (players, _) = build_match_rosters(&setup(GameMode::OneVsOne));
        let player_roster = PlayerRoster::from_players(players);
        let mut skill_roster = build_skill_roster(&player_roster);
        sync_turn_skill_usage(&mut skill_roster, 2);
        set_ai_skill_charges(&mut skill_roster, 2, 1, 1, 0, 1, 1);

        let mut world = World::new();
        spawn_test_piece(&mut world, 1, 2, 2, 1, 0);
        spawn_test_piece(&mut world, 2, 1, 1, HARD_AI_SNIPE_MIN_PROGRESS, 0);
        let mut system_state: SystemState<(
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
            MovingPieceQuery,
        )> = SystemState::new(&mut world);
        let (mut query, moving_query) = system_state.get_mut(&mut world).unwrap();
        let mut effect_queue = VisualEffectQueue::default();
        let mut reveal_delays = EffectRevealDelays::default();
        let mut motion_effects = PieceMotionEffects::default();

        maybe_use_ai_skills(
            2,
            1,
            &match_config,
            &BoardLayout::default(),
            &player_roster,
            &mut skill_roster,
            &mut effect_queue,
            &mut reveal_delays,
            &mut motion_effects,
            &mut query,
            &moving_query,
        );

        let state = player_skill_state(&skill_roster, 2).unwrap();
        assert_eq!(state.snipe_charges, 1);
        assert_eq!(state.shield_charges, 1);
        assert_eq!(state.double_dice_charges, 1);
        assert!(!skill_roster.skill_used_this_turn);
        assert_eq!(effect_queue.pending_count(), 0);
    }

    #[test]
    fn human_roll_never_auto_executes_single_action() {
        let actions = [PlannedAction::Launch {
            piece_id: 1,
            target_progress: 0,
        }];

        assert!(!should_auto_execute_human_action(&actions, false));
        assert!(!should_auto_execute_human_action(&actions, true));
    }

    #[test]
    fn rolled_turn_waits_until_center_dice_display_commits() {
        let mut turn_state = TurnState::opening_turn();

        assert!(!roll_result_waiting_for_display(&turn_state));

        set_roll_with_faces(&mut turn_state, 3, [3, 0]);
        assert!(roll_result_waiting_for_display(&turn_state));

        assert!(crate::gameplay::turn_flow::commit_pending_roll_display(
            &mut turn_state,
            1
        ));
        assert!(!roll_result_waiting_for_display(&turn_state));
        assert_eq!(turn_state.current_roll, Some(3));

        let mut choice_state = TurnState::opening_turn();
        crate::gameplay::turn_flow::set_pending_double_dice_choice(&mut choice_state, [2, 6]);
        assert!(roll_result_waiting_for_display(&choice_state));
        assert!(crate::gameplay::turn_flow::commit_pending_roll_display(
            &mut choice_state,
            1
        ));
        assert!(!roll_result_waiting_for_display(&choice_state));
        assert_eq!(choice_state.current_roll, None);
        assert!(choice_state.pending_double_dice_choice.is_some());
    }

    #[test]
    fn double_dice_choice_hit_testing_picks_nearest_visible_die() {
        assert_eq!(
            double_dice_choice_at_world(Vec2::new(-DOUBLE_DICE_CHOICE_OFFSET_X, 0.0), [2, 6], 40.0),
            Some(0)
        );
        assert_eq!(
            double_dice_choice_at_world(Vec2::new(DOUBLE_DICE_CHOICE_OFFSET_X, 0.0), [2, 6], 40.0),
            Some(1)
        );
        assert_eq!(
            double_dice_choice_at_world(Vec2::new(0.0, 0.0), [4, 0], 40.0),
            Some(0)
        );
        assert_eq!(
            double_dice_choice_at_world(Vec2::new(140.0, 0.0), [2, 6], 40.0),
            None
        );
    }

    #[test]
    fn easy_ai_does_not_use_skills_even_with_targets() {
        let match_config = match_config(GameMode::OneVsOne, AiDifficulty::Easy);
        let (players, _) = build_match_rosters(&setup(GameMode::OneVsOne));
        let player_roster = PlayerRoster::from_players(players);
        let mut skill_roster = build_skill_roster(&player_roster);
        sync_turn_skill_usage(&mut skill_roster, 2);
        set_ai_skill_charges(&mut skill_roster, 2, 1, 1, 0, 0, 0);

        let mut world = World::new();
        spawn_test_piece(&mut world, 1, 2, 2, 4, 0);
        spawn_test_piece(&mut world, 2, 1, 1, 12, 0);
        let mut system_state: SystemState<(
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
            MovingPieceQuery,
        )> = SystemState::new(&mut world);
        let (mut query, moving_query) = system_state.get_mut(&mut world).unwrap();
        let mut effect_queue = VisualEffectQueue::default();
        let mut reveal_delays = EffectRevealDelays::default();
        let mut motion_effects = PieceMotionEffects::default();

        maybe_use_ai_skills(
            2,
            1,
            &match_config,
            &BoardLayout::default(),
            &player_roster,
            &mut skill_roster,
            &mut effect_queue,
            &mut reveal_delays,
            &mut motion_effects,
            &mut query,
            &moving_query,
        );

        let state = player_skill_state(&skill_roster, 2).unwrap();
        assert_eq!(state.snipe_charges, 1);
        assert_eq!(state.shield_charges, 1);
        assert!(!skill_roster.skill_used_this_turn);
        assert_eq!(effect_queue.pending_count(), 0);
    }

    #[test]
    fn normal_ai_prefers_shield_over_opening_snipe() {
        let match_config = match_config(GameMode::OneVsOne, AiDifficulty::Normal);
        let (players, _) = build_match_rosters(&setup(GameMode::OneVsOne));
        let player_roster = PlayerRoster::from_players(players);
        let mut skill_roster = build_skill_roster(&player_roster);
        sync_turn_skill_usage(&mut skill_roster, 2);
        set_ai_skill_charges(&mut skill_roster, 2, 1, 1, 0, 0, 0);

        let mut world = World::new();
        spawn_test_piece(&mut world, 1, 2, 2, 4, 0);
        spawn_test_piece(&mut world, 2, 1, 1, 12, 0);
        let mut system_state: SystemState<(
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
            MovingPieceQuery,
        )> = SystemState::new(&mut world);
        let (mut query, moving_query) = system_state.get_mut(&mut world).unwrap();
        let mut effect_queue = VisualEffectQueue::default();
        let mut reveal_delays = EffectRevealDelays::default();
        let mut motion_effects = PieceMotionEffects::default();

        maybe_use_ai_skills(
            2,
            1,
            &match_config,
            &BoardLayout::default(),
            &player_roster,
            &mut skill_roster,
            &mut effect_queue,
            &mut reveal_delays,
            &mut motion_effects,
            &mut query,
            &moving_query,
        );

        let state = player_skill_state(&skill_roster, 2).unwrap();
        assert_eq!(state.snipe_charges, 1);
        assert_eq!(state.shield_charges, 0);
        assert_eq!(effect_queue.pending_count(), 1);
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
        let player_roster = PlayerRoster::from_players(players);
        let mut skill_roster = build_skill_roster(&player_roster);
        sync_turn_skill_usage(&mut skill_roster, 2);
        set_ai_skill_charges(&mut skill_roster, 2, 1, 0, 0, 0, 0);

        let mut world = World::new();
        spawn_test_piece(&mut world, 1, 2, 2, 4, 0);
        spawn_test_piece(&mut world, 2, 1, 1, HARD_AI_SNIPE_MIN_PROGRESS - 1, 0);
        let mut system_state: SystemState<(
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
            MovingPieceQuery,
        )> = SystemState::new(&mut world);
        let (mut query, moving_query) = system_state.get_mut(&mut world).unwrap();
        let mut effect_queue = VisualEffectQueue::default();
        let mut reveal_delays = EffectRevealDelays::default();
        let mut motion_effects = PieceMotionEffects::default();

        maybe_use_ai_skills(
            2,
            1,
            &match_config,
            &BoardLayout::default(),
            &player_roster,
            &mut skill_roster,
            &mut effect_queue,
            &mut reveal_delays,
            &mut motion_effects,
            &mut query,
            &moving_query,
        );

        let state = player_skill_state(&skill_roster, 2).unwrap();
        assert_eq!(state.snipe_charges, 1);
        assert!(!skill_roster.skill_used_this_turn);
        assert_eq!(effect_queue.pending_count(), 0);
    }

    #[test]
    fn hard_ai_uses_snipe_on_advanced_target() {
        let match_config = match_config(GameMode::OneVsOne, AiDifficulty::Hard);
        let (players, _) = build_match_rosters(&setup(GameMode::OneVsOne));
        let player_roster = PlayerRoster::from_players(players);
        let mut skill_roster = build_skill_roster(&player_roster);
        sync_turn_skill_usage(&mut skill_roster, 2);
        set_ai_skill_charges(&mut skill_roster, 2, 1, 0, 0, 0, 0);

        let mut world = World::new();
        spawn_test_piece(&mut world, 1, 2, 2, 4, 0);
        spawn_test_piece(&mut world, 2, 1, 1, HARD_AI_SNIPE_MIN_PROGRESS, 1);
        let mut system_state: SystemState<(
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
            MovingPieceQuery,
        )> = SystemState::new(&mut world);
        let (mut query, moving_query) = system_state.get_mut(&mut world).unwrap();
        let mut effect_queue = VisualEffectQueue::default();
        let mut reveal_delays = EffectRevealDelays::default();
        let mut motion_effects = PieceMotionEffects::default();

        maybe_use_ai_skills(
            2,
            1,
            &match_config,
            &BoardLayout::default(),
            &player_roster,
            &mut skill_roster,
            &mut effect_queue,
            &mut reveal_delays,
            &mut motion_effects,
            &mut query,
            &moving_query,
        );

        let state = player_skill_state(&skill_roster, 2).unwrap();
        assert_eq!(state.snipe_charges, 0);
        assert!(skill_roster.skill_used_this_turn);
        assert_eq!(effect_queue.pending_count(), 1);
        assert_eq!(reveal_delays.visible_shield(2, 0), 1);
        assert_eq!(
            motion_effects.take_for_piece(2),
            PieceMotionEffect::default()
        );
    }

    #[test]
    fn hard_ai_snipe_delays_unshielded_target_return_motion() {
        let match_config = match_config(GameMode::OneVsOne, AiDifficulty::Hard);
        let (players, _) = build_match_rosters(&setup(GameMode::OneVsOne));
        let player_roster = PlayerRoster::from_players(players);
        let mut skill_roster = build_skill_roster(&player_roster);
        sync_turn_skill_usage(&mut skill_roster, 2);
        set_ai_skill_charges(&mut skill_roster, 2, 1, 0, 0, 0, 0);

        let mut world = World::new();
        spawn_test_piece(&mut world, 1, 2, 2, 4, 0);
        spawn_test_piece(&mut world, 2, 1, 1, HARD_AI_SNIPE_MIN_PROGRESS, 0);
        let mut system_state: SystemState<(
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
            MovingPieceQuery,
        )> = SystemState::new(&mut world);
        let (mut query, moving_query) = system_state.get_mut(&mut world).unwrap();
        let mut effect_queue = VisualEffectQueue::default();
        let mut reveal_delays = EffectRevealDelays::default();
        let mut motion_effects = PieceMotionEffects::default();

        maybe_use_ai_skills(
            2,
            1,
            &match_config,
            &BoardLayout::default(),
            &player_roster,
            &mut skill_roster,
            &mut effect_queue,
            &mut reveal_delays,
            &mut motion_effects,
            &mut query,
            &moving_query,
        );

        let effect = motion_effects.take_for_piece(2);
        assert!(effect.start_delay_secs >= TARGETED_MISSILE_REVEAL_DURATION);
        assert!(effect.advance_two.is_none());
    }

    #[test]
    fn maybe_arm_dash_for_ai_after_roll_arms_dash_when_move_exists() {
        let (players, _) = build_match_rosters(&setup(GameMode::OneVsOne));
        let board_layout = BoardLayout::default();
        let player_roster = PlayerRoster::from_players(players);
        let mut skill_roster = build_skill_roster(&player_roster);
        sync_turn_skill_usage(&mut skill_roster, 2);
        set_ai_skill_charges(&mut skill_roster, 2, 0, 0, 0, 0, 1);

        let mut world = World::new();
        spawn_test_piece(&mut world, 1, 2, 2, 1, 0);
        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world).unwrap();

        maybe_arm_dash_for_ai_after_roll(
            2,
            1,
            AiDashEvaluation {
                ai_difficulty: AiDifficulty::Normal,
                skills_enabled: true,
                roll: DiceRoll(2),
                launch_rule: LaunchRule::SixOnly,
                board_layout: &board_layout,
                player_roster: &player_roster,
            },
            &mut skill_roster,
            &mut query,
        );
        let state = player_skill_state(&skill_roster, 2).unwrap();
        assert!(state.dash_armed);
        assert!(skill_roster.skill_used_this_turn);
    }
}
