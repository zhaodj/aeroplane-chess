use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::data::game_mode::GameMode;
use crate::domain::piece::PieceState;
use crate::gameplay::match_flow::{BoardLayout, MatchConfig, MatchResult, PlayerRoster};
use crate::gameplay::skill_flow::{
    MAX_PIECE_SHIELD, SkillRoster, arm_dash, arm_double_dice, build_skill_roster,
    can_use_skill_this_turn, current_player_type, dash_bonus, is_current_player_dash_move_piece,
    is_current_player_swap_piece, is_legal_shield_target, is_legal_snipe_target,
    is_legal_swap_target, mark_skill_used, player_skill_state, record_skill_action,
    spend_shield_charge, spend_snipe_charge, spend_swap_charge, sync_turn_skill_usage,
};
#[cfg(test)]
use crate::gameplay::swap_flow::execute_swap;
use crate::gameplay::swap_flow::{SWAP_MOTION_PENDING, SwapSelection, execute_selected_swap};
use crate::gameplay::turn_flow::{TurnState, human_roll_is_ready};
use crate::i18n::{Language, LanguageSettings};
use crate::platform::{DeviceProfile, PointerInputState};
use crate::plugins::animation_plugin::{MovingPieceQuery, swap_pair_is_moving};
use crate::plugins::effects_plugin::{
    EffectRevealDelays, PieceMotionEffects, TARGETED_MISSILE_REVEAL_DURATION, VisualEffectQueue,
};
use crate::plugins::menu_plugin::{SoundSettingsOverlayState, sound_settings_overlay_blocks_input};
use crate::plugins::piece_plugin::{HangarSlot, PieceId};
use crate::plugins::ui_plugin::{PlayerHudState, player_hud_point_is_interactive};
use crate::states::{AppState, GamePhase};
use crate::ui::game_layout::{GameLayout, swap_piece_picker_rects};

/// 技能插件：处理技能输入、目标选择与技能效果执行。
pub struct SkillPlugin;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct SkillSystems;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SkillUiAction {
    Shield,
    Snipe,
    Swap,
    DoubleDice,
    Dash,
}

#[derive(Resource, Default)]
/// 技能 UI 动作请求队列（键盘与点击入口共用）。
pub struct SkillUiRequest {
    pending: Option<SkillUiAction>,
    cancel_target_requested: bool,
    confirm_target_requested: bool,
}

impl SkillUiRequest {
    /// 向技能请求队列写入一次动作（同一时刻只保留一个待处理请求）。
    pub fn queue(&mut self, action: SkillUiAction) {
        if self.pending.is_none() {
            self.pending = Some(action);
        }
    }

    /// 请求取消当前技能目标选择。
    pub fn queue_cancel_target(&mut self) {
        self.cancel_target_requested = true;
    }

    pub fn queue_confirm_target(&mut self) {
        self.confirm_target_requested = true;
    }

    fn take_confirm_target(&mut self) -> bool {
        std::mem::take(&mut self.confirm_target_requested)
    }

    /// 取出并清空待处理技能请求。
    fn take(&mut self) -> Option<SkillUiAction> {
        self.pending.take()
    }

    /// 取出并清空技能目标取消请求。
    fn take_cancel_target(&mut self) -> bool {
        let requested = self.cancel_target_requested;
        self.cancel_target_requested = false;
        requested
    }
}

#[derive(Resource, Default)]
/// 技能目标选择状态（如 Snipe 多目标时的暂存）。
pub struct SkillTargetState {
    candidate_piece_ids: Vec<u8>,
    pub prompt: Option<String>,
    action: Option<SkillUiAction>,
    active: bool,
    pub(crate) swap: Option<SwapSelection>,
    swap_snapshot: Option<(PieceState, PieceState)>,
    swap_motion_pending: bool,
    input_consumed: bool,
    pub(crate) overlap_choices: Vec<u8>,
}

impl SkillTargetState {
    pub(crate) fn with_swap(swap: SwapSelection) -> Self {
        Self {
            candidate_piece_ids: swap.candidates().to_vec(),
            action: Some(SkillUiAction::Swap),
            active: true,
            swap: Some(swap),
            ..default()
        }
    }
    /// 当前技能（如 Snipe）可选目标列表。
    pub fn candidate_piece_ids(&self) -> &[u8] {
        &self.candidate_piece_ids
    }

    /// 是否处于“等待技能目标选择”状态。
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// 当前等待确认目标的技能类型。
    pub fn action(&self) -> Option<SkillUiAction> {
        self.action
    }

    pub fn is_swap_preview(&self) -> bool {
        self.overlap_choices.is_empty()
            && self.swap.as_ref().and_then(SwapSelection::pair).is_some()
    }

    pub fn swap_pair(&self) -> Option<(u8, u8)> {
        self.swap.as_ref()?.pair()
    }

    pub fn can_confirm_swap(&self) -> bool {
        self.is_swap_preview() && !self.swap_motion_pending
    }

    pub fn swap_source(&self) -> Option<u8> {
        self.swap.as_ref()?.source
    }
}

type SkillPieceQuery<'w, 's> = Query<
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
struct HumanSkillInputParams<'w, 's> {
    match_config: Res<'w, MatchConfig>,
    player_roster: Res<'w, PlayerRoster>,
    match_result: Res<'w, MatchResult>,
    game_phase: Res<'w, State<GamePhase>>,
    turn_state: Res<'w, TurnState>,
    language_settings: Res<'w, LanguageSettings>,
    skill_roster: ResMut<'w, SkillRoster>,
    skill_ui_request: ResMut<'w, SkillUiRequest>,
    target_state: ResMut<'w, SkillTargetState>,
    effect_queue: ResMut<'w, VisualEffectQueue>,
    reveal_delays: ResMut<'w, EffectRevealDelays>,
    motion_effects: ResMut<'w, PieceMotionEffects>,
    next_phase: ResMut<'w, NextState<GamePhase>>,
    piece_query: SkillPieceQuery<'w, 's>,
    moving_pieces: MovingPieceQuery<'w, 's>,
}

#[derive(SystemParam)]
struct SkillTargetParams<'w, 's> {
    board_layout: Res<'w, BoardLayout>,
    player_roster: Res<'w, PlayerRoster>,
    language_settings: Res<'w, LanguageSettings>,
    match_result: Res<'w, MatchResult>,
    match_config: Res<'w, MatchConfig>,
    game_phase: Res<'w, State<GamePhase>>,
    turn_state: Res<'w, TurnState>,
    target_state: ResMut<'w, SkillTargetState>,
    skill_roster: ResMut<'w, SkillRoster>,
    skill_ui_request: ResMut<'w, SkillUiRequest>,
    effect_queue: ResMut<'w, VisualEffectQueue>,
    reveal_delays: ResMut<'w, EffectRevealDelays>,
    motion_effects: ResMut<'w, PieceMotionEffects>,
    next_phase: ResMut<'w, NextState<GamePhase>>,
    piece_query: SkillPieceQuery<'w, 's>,
    moving_pieces: MovingPieceQuery<'w, 's>,
}

impl Plugin for SkillPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), setup_skill_roster)
            .add_systems(
                Update,
                (
                    sync_skill_turn_state,
                    handle_human_skill_input,
                    update_swap_motion_readiness,
                    handle_human_skill_target_key_select,
                    handle_human_skill_target_click,
                    update_skill_smoke_state,
                )
                    .chain()
                    .in_set(SkillSystems)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(OnExit(AppState::InGame), cleanup_skill_roster);
    }
}

fn setup_skill_roster(mut commands: Commands, player_roster: Res<PlayerRoster>) {
    // 进入对局时初始化技能资源与技能目标状态。
    commands.insert_resource(build_skill_roster(&player_roster));
    commands.insert_resource(SkillTargetState::default());
    commands.insert_resource(SkillUiRequest::default());
}

fn cleanup_skill_roster(mut commands: Commands) {
    // 离开对局时回收技能相关资源。
    commands.remove_resource::<SkillRoster>();
    commands.remove_resource::<SkillTargetState>();
    commands.remove_resource::<SkillUiRequest>();
}

fn sync_skill_turn_state(
    turn_state: Res<TurnState>,
    mut skill_roster: ResMut<SkillRoster>,
    mut target_state: ResMut<SkillTargetState>,
) {
    // 同步当前回合玩家技能状态；若玩家切换则清除旧目标选择。
    sync_turn_skill_usage(&mut skill_roster, turn_state.current_player);
    if target_state.is_active()
        && skill_roster.active_turn_player != Some(turn_state.current_player)
    {
        clear_target_state(&mut target_state);
    }
}

fn update_skill_smoke_state(skill_roster: Res<SkillRoster>) {
    if !skill_roster.is_changed() {
        return;
    }
    let Some(action) = skill_roster.last_skill_action.as_deref() else {
        return;
    };
    write_skill_smoke_state(skill_roster.last_skill_action_serial, action);
}

#[cfg(target_arch = "wasm32")]
fn write_skill_smoke_state(serial: u64, action: &str) {
    let Some(body) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.body())
    else {
        return;
    };
    if !body.has_attribute("data-ac-smoke-shell") && !body.has_attribute("data-ac-smoke-state") {
        return;
    }

    let _ = body.set_attribute("data-ac-smoke-skill-serial", &serial.to_string());
    let _ = body.set_attribute("data-ac-smoke-skill-action", action);
}

#[cfg(not(target_arch = "wasm32"))]
fn write_skill_smoke_state(_serial: u64, _action: &str) {}

/// 人类技能入口：统一处理按键与 HUD 点击触发的技能动作。
fn handle_human_skill_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    overlay_state: Res<SoundSettingsOverlayState>,
    mut params: HumanSkillInputParams,
) {
    params.target_state.input_consumed = false;
    if sound_settings_overlay_blocks_input(&overlay_state) {
        return;
    }

    if !params.match_config.rule_set.skills_enabled() {
        let _ = params.skill_ui_request.take();
        let _ = params.skill_ui_request.take_cancel_target();
        clear_target_state(&mut params.target_state);
        return;
    }

    let requested_action = if keyboard.just_pressed(KeyCode::KeyQ) {
        Some(SkillUiAction::Shield)
    } else if keyboard.just_pressed(KeyCode::KeyS) {
        Some(SkillUiAction::Snipe)
    } else if keyboard.just_pressed(KeyCode::KeyW) {
        Some(SkillUiAction::DoubleDice)
    } else if keyboard.just_pressed(KeyCode::KeyE) {
        Some(SkillUiAction::Dash)
    } else if keyboard.just_pressed(KeyCode::KeyA) {
        Some(SkillUiAction::Swap)
    } else {
        None
    }
    .or_else(|| params.skill_ui_request.take());

    let Some(action) = requested_action else {
        return;
    };

    if params.target_state.is_active() {
        return;
    }

    if params.match_result.finished {
        return;
    }

    if current_player_type(&params.player_roster, params.turn_state.current_player)
        != Some(crate::domain::player::PlayerControl::Human)
    {
        return;
    }

    if !can_use_skill_this_turn(&params.skill_roster, params.turn_state.current_player) {
        let blocked_by_event =
            player_skill_state(&params.skill_roster, params.turn_state.current_player)
                .map(|state| state.skill_blocked_this_turn)
                .unwrap_or(false);
        let message = if blocked_by_event {
            format!(
                "P{} cannot use skills this turn (event lock)",
                params.turn_state.current_player
            )
        } else {
            format!(
                "P{} already used a skill this turn",
                params.turn_state.current_player
            )
        };
        record_skill_action(
            &mut params.skill_roster,
            params.turn_state.turn_index,
            params.turn_state.current_player,
            message,
        );
        return;
    }

    match action {
        SkillUiAction::Shield if matches!(params.game_phase.get(), GamePhase::AwaitDice) => {
            let targets = collect_shield_targets_for_full_query(
                params.turn_state.current_player,
                &params.piece_query,
            );
            if targets.is_empty() {
                record_skill_action(
                    &mut params.skill_roster,
                    params.turn_state.turn_index,
                    params.turn_state.current_player,
                    format!(
                        "P{} could not find a piece for Shield",
                        params.turn_state.current_player
                    ),
                );
                return;
            }

            if targets.len() == 1 {
                resolve_shield_target(
                    targets[0],
                    params.turn_state.current_player,
                    params.turn_state.turn_index,
                    &mut params.skill_roster,
                    &mut params.effect_queue,
                    &mut params.piece_query,
                );
                return;
            }

            params.target_state.candidate_piece_ids = targets;
            params.target_state.action = Some(SkillUiAction::Shield);
            params.target_state.prompt =
                Some(shield_target_prompt(params.language_settings.language));
            params.target_state.active = true;
            params.next_phase.set(GamePhase::ResolveSkillEffect);
        }
        SkillUiAction::Snipe if matches!(params.game_phase.get(), GamePhase::AwaitDice) => {
            let Some(current_team_id) = params
                .player_roster
                .players
                .iter()
                .find(|player| player.state.player_id == params.turn_state.current_player)
                .map(|player| player.state.team_id)
            else {
                return;
            };
            let targets = collect_snipe_targets_for_full_query(
                params.turn_state.current_player,
                current_team_id,
                &params.piece_query,
            );

            if targets.is_empty() {
                record_skill_action(
                    &mut params.skill_roster,
                    params.turn_state.turn_index,
                    params.turn_state.current_player,
                    format!(
                        "P{} found no Snipe target",
                        params.turn_state.current_player
                    ),
                );
                return;
            }
            if !spend_snipe_charge(&mut params.skill_roster, params.turn_state.current_player) {
                record_skill_action(
                    &mut params.skill_roster,
                    params.turn_state.turn_index,
                    params.turn_state.current_player,
                    format!(
                        "P{} has no Snipe charges left",
                        params.turn_state.current_player
                    ),
                );
                return;
            }

            if targets.len() == 1 {
                mark_skill_used(&mut params.skill_roster, params.turn_state.current_player);
                if let Some(target_world) = piece_world_position(targets[0], &params.piece_query) {
                    params
                        .effect_queue
                        .hud_skill_missile(SkillUiAction::Snipe, target_world);
                }
                if piece_personal_shield(targets[0], &params.piece_query).unwrap_or_default() > 0 {
                    params
                        .reveal_delays
                        .delay_shield_loss(targets[0], TARGETED_MISSILE_REVEAL_DURATION);
                }
                if snipe_will_send_to_hangar(targets[0], &params.piece_query) {
                    params
                        .motion_effects
                        .delay_piece_motion(targets[0], TARGETED_MISSILE_REVEAL_DURATION);
                }
                let message = execute_snipe(targets[0], &mut params.piece_query);
                record_skill_action(
                    &mut params.skill_roster,
                    params.turn_state.turn_index,
                    params.turn_state.current_player,
                    message,
                );
                return;
            }

            mark_skill_used(&mut params.skill_roster, params.turn_state.current_player);
            params.target_state.candidate_piece_ids = targets;
            params.target_state.action = Some(SkillUiAction::Snipe);
            params.target_state.prompt =
                Some(snipe_target_prompt(params.language_settings.language));
            params.target_state.active = true;
            params.next_phase.set(GamePhase::ResolveSkillEffect);
        }
        SkillUiAction::DoubleDice if matches!(params.game_phase.get(), GamePhase::AwaitDice) => {
            if arm_double_dice(&mut params.skill_roster, params.turn_state.current_player) {
                mark_skill_used(&mut params.skill_roster, params.turn_state.current_player);
                record_skill_action(
                    &mut params.skill_roster,
                    params.turn_state.turn_index,
                    params.turn_state.current_player,
                    format!(
                        "P{} armed DoubleDice for the next roll",
                        params.turn_state.current_player
                    ),
                );
            } else {
                let armed =
                    player_skill_state(&params.skill_roster, params.turn_state.current_player)
                        .map(|state| state.double_dice_armed)
                        .unwrap_or(false);
                let message = if armed {
                    format!(
                        "P{} already has DoubleDice armed",
                        params.turn_state.current_player
                    )
                } else {
                    format!(
                        "P{} has no DoubleDice charges left",
                        params.turn_state.current_player
                    )
                };
                record_skill_action(
                    &mut params.skill_roster,
                    params.turn_state.turn_index,
                    params.turn_state.current_player,
                    message,
                );
            }
        }
        SkillUiAction::Dash
            if matches!(params.game_phase.get(), GamePhase::AwaitPieceSelect)
                && dash_bonus(&params.skill_roster, params.turn_state.current_player) == 0 =>
        {
            if !current_player_has_dash_move_piece(
                params.turn_state.current_player,
                &params.piece_query,
            ) {
                record_skill_action(
                    &mut params.skill_roster,
                    params.turn_state.turn_index,
                    params.turn_state.current_player,
                    format!(
                        "P{} needs a movable piece to use Dash",
                        params.turn_state.current_player
                    ),
                );
                return;
            }
            if arm_dash(&mut params.skill_roster, params.turn_state.current_player) {
                mark_skill_used(&mut params.skill_roster, params.turn_state.current_player);
                record_skill_action(
                    &mut params.skill_roster,
                    params.turn_state.turn_index,
                    params.turn_state.current_player,
                    format!(
                        "P{} armed Dash for +3 movement",
                        params.turn_state.current_player
                    ),
                );
            } else {
                record_skill_action(
                    &mut params.skill_roster,
                    params.turn_state.turn_index,
                    params.turn_state.current_player,
                    format!(
                        "P{} has no Dash charges left",
                        params.turn_state.current_player
                    ),
                );
            }
        }
        SkillUiAction::Swap if matches!(params.game_phase.get(), GamePhase::AwaitDice) => {
            if !human_roll_is_ready(
                &params.turn_state,
                params.game_phase.get(),
                &params.player_roster,
                &params.match_result,
            ) {
                return;
            }
            let Some(current_team_id) = params
                .player_roster
                .players
                .iter()
                .find(|player| player.state.player_id == params.turn_state.current_player)
                .map(|player| player.state.team_id)
            else {
                return;
            };

            let targets = collect_swap_targets(
                params.turn_state.current_player,
                current_team_id,
                params.match_config.mode,
                &params.piece_query,
            );
            if targets.is_empty() {
                record_skill_action(
                    &mut params.skill_roster,
                    params.turn_state.turn_index,
                    params.turn_state.current_player,
                    format!(
                        "P{} found no target piece to Swap with",
                        params.turn_state.current_player
                    ),
                );
                return;
            }
            let mut sources = params
                .piece_query
                .iter()
                .filter(|(_, _, piece, _)| {
                    is_current_player_swap_piece(params.turn_state.current_player, piece)
                })
                .map(|(id, _, _, _)| id.0)
                .collect::<Vec<_>>();
            sources.sort_unstable();
            if sources.is_empty() {
                record_skill_action(
                    &mut params.skill_roster,
                    params.turn_state.turn_index,
                    params.turn_state.current_player,
                    format!(
                        "P{} needs a main route piece to use Swap",
                        params.turn_state.current_player
                    ),
                );
                return;
            }
            if !player_skill_state(&params.skill_roster, params.turn_state.current_player)
                .is_some_and(|skills| skills.swap_charges > 0)
            {
                record_skill_action(
                    &mut params.skill_roster,
                    params.turn_state.turn_index,
                    params.turn_state.current_player,
                    format!(
                        "P{} has no Swap charges left",
                        params.turn_state.current_player
                    ),
                );
                return;
            }

            *params.target_state = SkillTargetState::with_swap(SwapSelection::new(
                params.turn_state.current_player,
                params.turn_state.turn_index,
                sources,
                targets,
            ));
            refresh_swap_target_state(
                &mut params.target_state,
                params.language_settings.language,
                &params.piece_query,
                &params.moving_pieces,
            );
            params.next_phase.set(GamePhase::ResolveSkillEffect);
        }
        _ => {
            record_skill_action(
                &mut params.skill_roster,
                params.turn_state.turn_index,
                params.turn_state.current_player,
                "Skill not available in current phase",
            );
        }
    }
}

fn snipe_target_prompt(language: Language) -> String {
    match language {
        Language::SimplifiedChinese => "点击高亮狙击目标，或取消。".to_string(),
        Language::English => "Tap a highlighted Snipe target, or cancel.".to_string(),
    }
}

fn shield_target_prompt(language: Language) -> String {
    match language {
        Language::SimplifiedChinese => "点击高亮飞机加盾，或取消。".to_string(),
        Language::English => "Tap a highlighted aircraft to shield it, or cancel.".to_string(),
    }
}

fn skill_target_cancelled_message(action: SkillUiAction) -> &'static str {
    match action {
        SkillUiAction::Shield => "Shield selection cancelled",
        SkillUiAction::Snipe => "Snipe selection cancelled",
        SkillUiAction::Swap => "Swap selection cancelled",
        _ => "Skill target selection cancelled",
    }
}

/// 技能目标选择的键盘分支（数字键 + Esc 取消）。
fn handle_human_skill_target_key_select(
    keyboard: Res<ButtonInput<KeyCode>>,
    overlay_state: Res<SoundSettingsOverlayState>,
    mut params: SkillTargetParams,
) {
    let cancel_requested = params.skill_ui_request.take_cancel_target();
    let confirm_requested = params.skill_ui_request.take_confirm_target();
    if params.target_state.swap.as_ref().is_some_and(|swap| {
        swap.player_id != params.turn_state.current_player
            || swap.turn_index != params.turn_state.turn_index
            || params.match_result.finished
            || !matches!(
                params.game_phase.get(),
                GamePhase::AwaitDice | GamePhase::ResolveSkillEffect
            )
    }) {
        clear_target_state(&mut params.target_state);
        if matches!(params.game_phase.get(), GamePhase::ResolveSkillEffect) {
            params.next_phase.set(GamePhase::AwaitDice);
        }
        return;
    }
    if !params.match_config.rule_set.skills_enabled() {
        let _ = params.skill_ui_request.take_cancel_target();
        clear_target_state(&mut params.target_state);
        return;
    }

    if sound_settings_overlay_blocks_input(&overlay_state)
        || !matches!(params.game_phase.get(), GamePhase::ResolveSkillEffect)
        || !params.target_state.is_active()
    {
        return;
    }

    let Some(target_action) = params.target_state.action() else {
        return;
    };

    if keyboard.just_pressed(KeyCode::Escape) || cancel_requested {
        record_skill_action(
            &mut params.skill_roster,
            params.turn_state.turn_index,
            params.turn_state.current_player,
            skill_target_cancelled_message(target_action),
        );
        clear_target_state(&mut params.target_state);
        params.next_phase.set(GamePhase::AwaitDice);
        return;
    }

    if target_action == SkillUiAction::Swap {
        if keyboard.just_pressed(KeyCode::Backspace) {
            params.target_state.swap.as_mut().unwrap().reselect();
            refresh_swap_target_state(
                &mut params.target_state,
                params.language_settings.language,
                &params.piece_query,
                &params.moving_pieces,
            );
            params.target_state.input_consumed = true;
            return;
        }
        if (confirm_requested || keyboard.just_pressed(KeyCode::Enter))
            && params.target_state.is_swap_preview()
        {
            confirm_swap(&mut params);
            return;
        }
    }

    let keys = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
        KeyCode::Digit0,
        KeyCode::Minus,
        KeyCode::Equal,
    ];
    let candidates = if params.target_state.overlap_choices.is_empty() {
        &params.target_state.candidate_piece_ids
    } else {
        &params.target_state.overlap_choices
    };
    let Some(selection) = keys.iter().enumerate().find_map(|(index, key)| {
        (index < candidates.len() && keyboard.just_pressed(*key)).then_some(index)
    }) else {
        return;
    };

    let target_piece_id = candidates[selection];
    match target_action {
        SkillUiAction::Shield => resolve_shield_target(
            target_piece_id,
            params.turn_state.current_player,
            params.turn_state.turn_index,
            &mut params.skill_roster,
            &mut params.effect_queue,
            &mut params.piece_query,
        ),
        SkillUiAction::Snipe => resolve_snipe_target(
            target_piece_id,
            params.turn_state.current_player,
            params.turn_state.turn_index,
            &mut params.skill_roster,
            &mut params.effect_queue,
            &mut params.reveal_delays,
            &mut params.motion_effects,
            &mut params.piece_query,
        ),
        SkillUiAction::Swap => {
            select_swap_piece(target_piece_id, &mut params);
            return;
        }
        _ => return,
    }
    clear_target_state(&mut params.target_state);
    params.next_phase.set(GamePhase::AwaitDice);
}

/// 技能目标选择的鼠标分支（点击高亮目标棋子）。
fn handle_human_skill_target_click(
    pointer: Res<PointerInputState>,
    device_profile: Res<DeviceProfile>,
    windows: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform)>,
    overlay_state: Res<SoundSettingsOverlayState>,
    player_roster: Res<PlayerRoster>,
    hud_state: Res<PlayerHudState>,
    mut params: SkillTargetParams,
) {
    if !params.match_config.rule_set.skills_enabled() {
        clear_target_state(&mut params.target_state);
        return;
    }

    let Some(target_action) = params.target_state.action() else {
        return;
    };

    if !matches!(params.game_phase.get(), GamePhase::ResolveSkillEffect)
        || !params.target_state.is_active()
        || params.target_state.input_consumed
    {
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
    if !params.target_state.overlap_choices.is_empty() {
        let (_, cells) = swap_piece_picker_rects(
            GameLayout::new(window.width(), window.height(), *device_profile),
            params.target_state.overlap_choices.len(),
        );
        if let Some(index) = cells
            .iter()
            .position(|rect| rect.contains(pointer_position))
        {
            let id = params.target_state.overlap_choices[index];
            select_swap_piece(id, &mut params);
        } else {
            params.target_state.overlap_choices.clear();
        }
        return;
    }
    if player_hud_point_is_interactive(
        pointer_position,
        window,
        *device_profile,
        &player_roster,
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

    let pick_radius = device_profile.piece_pick_radius_world();
    let mut selected_piece_id = None;
    let mut best_distance_sq = f32::MAX;
    let mut swap_hits = Vec::new();
    for (piece_id, _, _, transform) in &mut params.piece_query {
        if !params
            .target_state
            .candidate_piece_ids
            .contains(&piece_id.0)
            && params.target_state.swap_source() != Some(piece_id.0)
        {
            continue;
        }

        if params.target_state.is_swap_preview()
            && !params
                .target_state
                .swap_pair()
                .is_some_and(|pair| piece_id.0 == pair.0 || piece_id.0 == pair.1)
        {
            continue;
        }

        let distance_sq = transform
            .translation
            .truncate()
            .distance_squared(cursor_world);
        if target_action == SkillUiAction::Swap && distance_sq <= pick_radius * pick_radius {
            swap_hits.push((piece_id.0, distance_sq));
        }
        if distance_sq <= pick_radius * pick_radius && distance_sq < best_distance_sq {
            best_distance_sq = distance_sq;
            selected_piece_id = Some(piece_id.0);
        }
    }

    if swap_hits.len() > 1 {
        swap_hits.sort_by_key(|(id, _)| *id);
        params.target_state.overlap_choices = swap_hits.into_iter().map(|(id, _)| id).collect();
        params.target_state.input_consumed = true;
        return;
    }

    let Some(target_piece_id) = selected_piece_id else {
        return;
    };
    match target_action {
        SkillUiAction::Shield => resolve_shield_target(
            target_piece_id,
            params.turn_state.current_player,
            params.turn_state.turn_index,
            &mut params.skill_roster,
            &mut params.effect_queue,
            &mut params.piece_query,
        ),
        SkillUiAction::Snipe => resolve_snipe_target(
            target_piece_id,
            params.turn_state.current_player,
            params.turn_state.turn_index,
            &mut params.skill_roster,
            &mut params.effect_queue,
            &mut params.reveal_delays,
            &mut params.motion_effects,
            &mut params.piece_query,
        ),
        SkillUiAction::Swap => {
            select_swap_piece(target_piece_id, &mut params);
            return;
        }
        _ => return,
    }
    clear_target_state(&mut params.target_state);
    params.next_phase.set(GamePhase::AwaitDice);
}

/// 清理技能目标选择状态。
fn clear_target_state(target_state: &mut SkillTargetState) {
    *target_state = SkillTargetState::default();
}

fn swap_piece_snapshot(
    pair: (u8, u8),
    pieces: &SkillPieceQuery<'_, '_>,
) -> Option<(PieceState, PieceState)> {
    let get = |id| {
        pieces
            .iter()
            .find(|(piece_id, _, _, _)| piece_id.0 == id)
            .map(|(_, _, piece, _)| *piece)
    };
    Some((get(pair.0)?, get(pair.1)?))
}

fn refresh_swap_target_state(
    target: &mut SkillTargetState,
    language: Language,
    pieces: &SkillPieceQuery<'_, '_>,
    moving: &MovingPieceQuery,
) {
    target.overlap_choices.clear();
    let Some(swap) = &target.swap else {
        return;
    };
    target.active = true;
    target.action = Some(SkillUiAction::Swap);
    target.candidate_piece_ids = swap.candidates().to_vec();
    target.swap_snapshot = swap
        .pair()
        .and_then(|pair| swap_piece_snapshot(pair, pieces));
    target.swap_motion_pending = swap
        .pair()
        .is_some_and(|pair| swap_pair_is_moving(pair, moving));
    refresh_swap_prompt(target, language);
}

pub(crate) fn update_swap_motion_readiness(
    mut target: ResMut<SkillTargetState>,
    language: Res<LanguageSettings>,
    moving: MovingPieceQuery,
) {
    let pending = target
        .swap_pair()
        .is_some_and(|pair| swap_pair_is_moving(pair, &moving));
    if target.swap_motion_pending != pending {
        target.swap_motion_pending = pending;
        // Do not refresh the commit snapshot: stale selections must still fail validation.
        refresh_swap_prompt(&mut target, language.language);
    }
}

fn refresh_swap_prompt(target: &mut SkillTargetState, language: Language) {
    let Some(swap) = &target.swap else {
        return;
    };
    let zh = language == Language::SimplifiedChinese;
    target.prompt = Some(if let Some((source, other)) = swap.pair() {
        let owner = target
            .swap_snapshot
            .map(|(_, p)| p.owner_player_id)
            .unwrap_or_default();
        if target.swap_motion_pending && zh {
            format!(
                "P{} · #{} ↔ P{} · #{}\n等待双方落稳后确认；可取消，不消耗次数。",
                swap.player_id, source, owner, other
            )
        } else if target.swap_motion_pending {
            format!(
                "P{} · #{} ↔ P{} · #{}\nWait for both aircraft to land. Cancel is free.",
                swap.player_id, source, owner, other
            )
        } else if zh {
            format!(
                "P{} · #{} ↔ P{} · #{}\n确认后消耗 1 次；点击己方飞机可重选。",
                swap.player_id, source, owner, other
            )
        } else {
            format!(
                "P{} · #{} ↔ P{} · #{}\nCosts 1 charge on confirm. Tap your aircraft to reselect.",
                swap.player_id, source, owner, other
            )
        }
    } else if let Some(source) = swap.source {
        if zh {
            format!(
                "已选己方 #{}，请点击高亮目标。\n点击己方飞机可重选；取消不消耗次数。",
                source
            )
        } else {
            format!(
                "Selected #{}. Tap a highlighted target.\nTap your aircraft to reselect; cancel is free.",
                source
            )
        }
    } else if zh {
        "先点击一架高亮的己方飞机。\n取消不消耗次数。".into()
    } else {
        "First tap one of your highlighted aircraft.\nCancel does not spend a charge.".into()
    });
}

fn select_swap_piece(id: u8, params: &mut SkillTargetParams) {
    params.target_state.overlap_choices.clear();
    let Some(swap) = &mut params.target_state.swap else {
        return;
    };
    if swap.source == Some(id) {
        swap.reselect();
    } else {
        swap.select(id);
    }
    refresh_swap_target_state(
        &mut params.target_state,
        params.language_settings.language,
        &params.piece_query,
        &params.moving_pieces,
    );
    params.target_state.input_consumed = true;
}

fn confirm_swap(params: &mut SkillTargetParams) {
    let Some(pair) = params.target_state.swap_pair() else {
        return;
    };
    let player = params.turn_state.current_player;
    let legal_session = params.target_state.swap.as_ref().is_some_and(|swap| {
        swap.player_id == player && swap.turn_index == params.turn_state.turn_index
    });
    let result = if !legal_session
        || params.match_result.finished
        || current_player_type(&params.player_roster, player)
            != Some(crate::domain::player::PlayerControl::Human)
        || !can_use_skill_this_turn(&params.skill_roster, player)
        || params.target_state.swap_snapshot.is_none()
        || params.target_state.swap_snapshot != swap_piece_snapshot(pair, &params.piece_query)
    {
        Err("Swap failed: selection is no longer available")
    } else if !player_skill_state(&params.skill_roster, player).is_some_and(|s| s.swap_charges > 0)
    {
        Err("Swap failed: no charges left")
    } else {
        execute_selected_swap(
            player,
            params.match_config.mode,
            pair.0,
            pair.1,
            &params.board_layout,
            &params.player_roster,
            &mut params.piece_query,
            &params.moving_pieces,
        )
    };
    if result == Err(SWAP_MOTION_PENDING) {
        params.target_state.swap_motion_pending = true;
        params.target_state.input_consumed = true;
        refresh_swap_prompt(&mut params.target_state, params.language_settings.language);
        return;
    }
    if result.is_ok() {
        // Same exclusive system: the checked charge cannot change between validation and commit.
        spend_swap_charge(&mut params.skill_roster, player);
        mark_skill_used(&mut params.skill_roster, player);
    }
    record_skill_action(
        &mut params.skill_roster,
        params.turn_state.turn_index,
        player,
        result.unwrap_or_else(str::to_string),
    );
    clear_target_state(&mut params.target_state);
    params.next_phase.set(GamePhase::AwaitDice);
}

/// 收集 Shield 目标：仅允许己方 Active 且未满个人护盾的棋子。
fn collect_shield_targets_for_full_query(
    player_id: u8,
    piece_query: &Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> Vec<u8> {
    let mut targets = piece_query
        .iter()
        .filter(|(_, _, piece_state, _)| is_legal_shield_target(player_id, piece_state))
        .map(|(piece_id, _, _, _)| piece_id.0)
        .collect::<Vec<_>>();
    targets.sort_unstable();
    targets
}

enum ShieldTargetResult {
    Applied(u8),
    NoCharge,
    NoLongerLegal,
}

fn apply_shield_target(
    player_id: u8,
    target_piece_id: u8,
    skill_roster: &mut SkillRoster,
    effect_queue: &mut VisualEffectQueue,
    piece_query: &mut SkillPieceQuery<'_, '_>,
) -> ShieldTargetResult {
    let target_is_legal = piece_query.iter().any(|(piece_id, _, piece_state, _)| {
        piece_id.0 == target_piece_id && is_legal_shield_target(player_id, piece_state)
    });
    if !target_is_legal {
        return ShieldTargetResult::NoLongerLegal;
    }

    if !spend_shield_charge(skill_roster, player_id) {
        return ShieldTargetResult::NoCharge;
    }

    let target_world = piece_world_position(target_piece_id, piece_query);
    let Some(shield_value) = apply_shield_to_piece_for_full_query(target_piece_id, piece_query)
    else {
        return ShieldTargetResult::NoLongerLegal;
    };

    if let Some(target_world) = target_world {
        effect_queue.shield_flash(target_world);
    }
    mark_skill_used(skill_roster, player_id);
    ShieldTargetResult::Applied(shield_value)
}

fn resolve_shield_target(
    target_piece_id: u8,
    player_id: u8,
    turn_index: u32,
    skill_roster: &mut SkillRoster,
    effect_queue: &mut VisualEffectQueue,
    piece_query: &mut SkillPieceQuery<'_, '_>,
) {
    match apply_shield_target(
        player_id,
        target_piece_id,
        skill_roster,
        effect_queue,
        piece_query,
    ) {
        ShieldTargetResult::Applied(shield_value) => record_skill_action(
            skill_roster,
            turn_index,
            player_id,
            format!(
                "P{} used Shield on piece #{} ({})",
                player_id, target_piece_id, shield_value
            ),
        ),
        ShieldTargetResult::NoCharge => record_skill_action(
            skill_roster,
            turn_index,
            player_id,
            format!("P{} has no Shield charges left", player_id),
        ),
        ShieldTargetResult::NoLongerLegal => record_skill_action(
            skill_roster,
            turn_index,
            player_id,
            format!("P{} Shield target is no longer available", player_id),
        ),
    }
}

fn resolve_snipe_target(
    target_piece_id: u8,
    player_id: u8,
    turn_index: u32,
    skill_roster: &mut SkillRoster,
    effect_queue: &mut VisualEffectQueue,
    reveal_delays: &mut EffectRevealDelays,
    motion_effects: &mut PieceMotionEffects,
    piece_query: &mut SkillPieceQuery<'_, '_>,
) {
    if let Some(target_world) = piece_world_position(target_piece_id, piece_query) {
        effect_queue.hud_skill_missile(SkillUiAction::Snipe, target_world);
    }
    if piece_personal_shield(target_piece_id, piece_query).unwrap_or_default() > 0 {
        reveal_delays.delay_shield_loss(target_piece_id, TARGETED_MISSILE_REVEAL_DURATION);
    }
    if snipe_will_send_to_hangar(target_piece_id, piece_query) {
        motion_effects.delay_piece_motion(target_piece_id, TARGETED_MISSILE_REVEAL_DURATION);
    }
    let message = execute_snipe(target_piece_id, piece_query);
    record_skill_action(skill_roster, turn_index, player_id, message);
}

fn piece_world_position(
    piece_id: u8,
    piece_query: &Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> Option<Vec2> {
    piece_query
        .iter()
        .find(|(query_piece_id, _, _, _)| query_piece_id.0 == piece_id)
        .map(|(_, _, _, transform)| transform.translation.truncate())
}

fn piece_personal_shield(
    piece_id: u8,
    piece_query: &Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> Option<u8> {
    piece_query
        .iter()
        .find(|(query_piece_id, _, _, _)| query_piece_id.0 == piece_id)
        .map(|(_, _, piece_state, _)| piece_state.shield)
}

fn snipe_will_send_to_hangar(
    piece_id: u8,
    piece_query: &Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> bool {
    piece_query
        .iter()
        .find(|(query_piece_id, _, _, _)| query_piece_id.0 == piece_id)
        .is_some_and(|(_, _, piece_state, _)| {
            piece_state.shield == 0 && piece_state.stack_shield == 0
        })
}

/// 给目标棋子加盾（技能插件查询版本）。
fn apply_shield_to_piece_for_full_query(
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

/// 收集 Snipe 候选：同规则层一致，优先无盾敌人。
fn collect_snipe_targets_for_full_query(
    current_player: u8,
    current_team: u8,
    piece_query: &Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> Vec<u8> {
    let mut unshielded = Vec::new();
    let mut shielded = Vec::new();

    for (piece_id, _, piece_state, _) in piece_query.iter() {
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

/// 执行 Snipe：优先削盾，若无盾则送回机库。
fn execute_snipe(
    piece_id: u8,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> String {
    for (query_piece_id, hangar_slot, mut piece_state, mut transform) in piece_query.iter_mut() {
        if query_piece_id.0 != piece_id {
            continue;
        }

        if piece_state.shield > 0 {
            piece_state.shield -= 1;
            return format!("Snipe hit piece #{} and removed a shield", piece_id);
        }
        if piece_state.stack_shield > 0 {
            piece_state.stack_shield = 0;
            return format!("Snipe hit piece #{} and broke the shared shield", piece_id);
        }

        piece_state.status = crate::domain::piece::PieceStatus::InHangar;
        piece_state.progress = 0;
        piece_state.shield = 0;
        piece_state.stack_shield = 0;
        transform.translation.x = hangar_slot.0.x;
        transform.translation.y = hangar_slot.0.y;
        return format!("Snipe sent piece #{} back to hangar", piece_id);
    }

    "Snipe failed to resolve".to_string()
}

/// 判断当前玩家是否至少有一枚可作为 Swap 发起方的棋子。
#[cfg(test)]
fn current_player_has_swap_piece(
    current_player: u8,
    piece_query: &Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> bool {
    piece_query
        .iter()
        .any(|(_, _, piece_state, _)| is_current_player_swap_piece(current_player, &piece_state))
}

/// 判断当前玩家是否至少有一枚可通过 Dash 增幅的移动棋子。
fn current_player_has_dash_move_piece(
    current_player: u8,
    piece_query: &Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> bool {
    piece_query.iter().any(|(_, _, piece_state, _)| {
        is_current_player_dash_move_piece(current_player, piece_state)
    })
}

/// 查找可用于 Swap 的目标主环道棋子。
fn collect_swap_targets(
    current_player: u8,
    current_team: u8,
    mode: GameMode,
    piece_query: &Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> Vec<u8> {
    let mut candidates = piece_query
        .iter()
        .filter(|(_, _, piece_state, _)| {
            is_legal_swap_target(current_player, current_team, mode, piece_state)
        })
        .map(|(piece_id, _, _, _)| piece_id.0)
        .collect::<Vec<_>>();
    candidates.sort_unstable();
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gameplay::skill_flow::PlayerSkillState;
    use crate::gameplay::turn_flow::HOME_ENTRY_PROGRESS;
    use bevy::ecs::system::SystemState;

    fn swap_selection_app(single: bool) -> App {
        use crate::gameplay::match_flow::{MatchSetup, PlayerSeat, build_match_resources};
        let setup = MatchSetup {
            mode: GameMode::TwoVsTwo,
            rule_set: crate::data::rule_set::RuleSet::Creative,
            ai_difficulty: crate::gameplay::ai::AiDifficulty::Normal,
            fast_mode: false,
            launch_rule: crate::domain::rules::LaunchRule::SixOnly,
            player_seats: PlayerSeat::ALL,
            pieces_per_player: 2,
            player_controls: [crate::domain::player::PlayerControl::Human; 4],
        };
        let (board, roster, _) = build_match_resources(&setup);
        let config = MatchConfig {
            mode: setup.mode,
            rule_set: setup.rule_set,
            ai_difficulty: setup.ai_difficulty,
            fast_mode: setup.fast_mode,
            launch_rule: setup.launch_rule,
            player_seats: setup.player_seats,
            pieces_per_player: setup.pieces_per_player,
            player_controls: setup.player_controls,
        };
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .insert_state(GamePhase::AwaitDice)
            .insert_resource(config)
            .insert_resource(build_skill_roster(&roster))
            .insert_resource(roster)
            .insert_resource(TurnState::opening_turn())
            .insert_resource(board)
            .init_resource::<MatchResult>()
            .init_resource::<LanguageSettings>()
            .init_resource::<SoundSettingsOverlayState>()
            .init_resource::<SkillUiRequest>()
            .init_resource::<SkillTargetState>()
            .init_resource::<VisualEffectQueue>()
            .init_resource::<EffectRevealDelays>()
            .init_resource::<PieceMotionEffects>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_systems(
                Update,
                (
                    sync_skill_turn_state,
                    handle_human_skill_input,
                    update_swap_motion_readiness,
                    handle_human_skill_target_key_select,
                )
                    .chain(),
            );
        for (id, owner, progress) in [(1, 1, 3), (2, 1, 9), (5, 3, 18), (6, 3, 27)] {
            if single && (id == 2 || id == 6) {
                continue;
            }
            app.world_mut().spawn((
                PieceId(id),
                HangarSlot(Vec2::ZERO),
                PieceState {
                    owner_player_id: owner,
                    team_id: 1,
                    status: crate::domain::piece::PieceStatus::Active,
                    progress,
                    shield: 0,
                    stack_shield: 0,
                    motion_serial: 0,
                },
                Transform::default(),
            ));
        }
        app.world_mut()
            .resource_mut::<SkillUiRequest>()
            .queue(SkillUiAction::Swap);
        app.update();
        app.update();
        app
    }

    fn select_key(app: &mut App, key: KeyCode) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(key);
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
    }

    fn selection_pieces(app: &mut App) -> Vec<(u8, PieceState)> {
        let mut query = app.world_mut().query::<(&PieceId, &PieceState)>();
        let mut pieces = query
            .iter(app.world())
            .map(|(id, p)| (id.0, *p))
            .collect::<Vec<_>>();
        pieces.sort_by_key(|(id, _)| *id);
        pieces
    }

    #[test]
    fn overlap_picker_touch_selects_exact_number_and_cannot_confirm_in_same_press() {
        use bevy::input::touch::{TouchInput, TouchPhase};
        let mut app = swap_selection_app(false);
        app.init_resource::<ButtonInput<MouseButton>>()
            .add_message::<TouchInput>()
            .add_plugins(crate::platform::PlatformPlugin)
            .init_resource::<PlayerHudState>()
            .add_systems(
                Update,
                handle_human_skill_target_click.after(handle_human_skill_target_key_select),
            );
        let window = app
            .world_mut()
            .spawn(Window {
                resolution: (1280, 720).into(),
                ..default()
            })
            .id();
        let layout = GameLayout::new(
            1280.0,
            720.0,
            DeviceProfile::from_window_size(1280.0, 720.0),
        );
        for (choices, expected_source, expected_pair) in [
            (vec![1, 2], Some(2), None),
            (vec![5, 6], Some(2), Some((2, 6))),
        ] {
            app.world_mut()
                .resource_mut::<SkillTargetState>()
                .overlap_choices = choices;
            let (_, cells) = swap_piece_picker_rects(layout, 2);
            app.world_mut().write_message(TouchInput {
                phase: TouchPhase::Started,
                position: cells[1].center(),
                window,
                force: None,
                id: 1,
            });
            app.update();
            let state = app.world().resource::<SkillTargetState>();
            assert_eq!(state.swap_source(), expected_source);
            assert_eq!(state.swap_pair(), expected_pair);
            assert!(state.overlap_choices.is_empty());
            assert_eq!(
                app.world().resource::<SkillRoster>().players[0].swap_charges,
                1
            );
            app.world_mut().write_message(TouchInput {
                phase: TouchPhase::Ended,
                position: cells[1].center(),
                window,
                force: None,
                id: 1,
            });
            app.update();
        }
    }

    #[test]
    fn human_swap_selects_non_first_pair_previews_then_spends_once() {
        let mut app = swap_selection_app(false);
        let before = selection_pieces(&mut app);
        assert_eq!(
            app.world()
                .resource::<SkillTargetState>()
                .candidate_piece_ids(),
            &[1, 2]
        );
        select_key(&mut app, KeyCode::Digit2);
        assert_eq!(
            app.world()
                .resource::<SkillTargetState>()
                .candidate_piece_ids(),
            &[5, 6]
        );
        select_key(&mut app, KeyCode::Digit2);
        assert_eq!(
            app.world().resource::<SkillTargetState>().swap_pair(),
            Some((2, 6))
        );
        assert_eq!(selection_pieces(&mut app), before);
        let skills = app.world().resource::<SkillRoster>();
        assert_eq!(skills.players[0].swap_charges, 1);
        assert!(!skills.skill_used_this_turn);
        app.world_mut()
            .resource_mut::<SkillUiRequest>()
            .queue_confirm_target();
        select_key(&mut app, KeyCode::Enter);
        let after = selection_pieces(&mut app);
        assert_eq!(before[0], after[0]);
        assert_eq!(before[2], after[2]);
        assert_ne!(before[1], after[1]);
        assert_ne!(before[3], after[3]);
        let skills = app.world().resource::<SkillRoster>();
        assert_eq!(skills.players[0].swap_charges, 0);
        assert!(skills.skill_used_this_turn);
        assert_eq!(
            skills.last_skill_action.as_deref(),
            Some("Swap exchanged piece #2 with piece #6")
        );
        assert!(!app.world().resource::<SkillTargetState>().is_active());
        app.update();
        assert_eq!(
            *app.world().resource::<State<GamePhase>>().get(),
            GamePhase::AwaitDice
        );
        assert_eq!(selection_pieces(&mut app), after);
    }

    #[test]
    fn single_swap_candidates_still_require_confirmation_and_cancel_is_free() {
        for keyboard in [true, false] {
            let mut app = swap_selection_app(true);
            let before = selection_pieces(&mut app);
            assert_eq!(
                app.world().resource::<SkillTargetState>().swap_pair(),
                Some((1, 5))
            );
            if keyboard {
                select_key(&mut app, KeyCode::Escape);
            } else {
                app.world_mut()
                    .resource_mut::<SkillUiRequest>()
                    .queue_cancel_target();
                app.update();
            }
            assert_eq!(selection_pieces(&mut app), before);
            assert!(!app.world().resource::<SkillTargetState>().is_active());
            assert_eq!(
                app.world().resource::<SkillRoster>().players[0].swap_charges,
                1
            );
            assert!(!app.world().resource::<SkillRoster>().skill_used_this_turn);
        }
    }

    #[test]
    fn swap_waits_for_either_aircraft_and_requires_fresh_confirmation_without_spending() {
        use crate::plugins::animation_plugin::PieceMoveAnimation;
        for id in [1, 5] {
            for cancel in [false, true] {
                let mut app = swap_selection_app(true);
                let before = selection_pieces(&mut app);
                let entity = app
                    .world_mut()
                    .query::<(Entity, &PieceId)>()
                    .iter(app.world())
                    .find(|(_, piece)| piece.0 == id)
                    .unwrap()
                    .0;
                app.world_mut()
                    .entity_mut(entity)
                    .insert(PieceMoveAnimation::test_pending());
                app.world_mut()
                    .resource_mut::<SkillUiRequest>()
                    .queue_confirm_target();
                app.update();
                let target = app.world().resource::<SkillTargetState>();
                assert!(target.is_active() && target.is_swap_preview());
                assert!(!target.can_confirm_swap());
                assert!(target.prompt.as_deref().unwrap().contains("等待双方落稳"));
                assert_eq!(selection_pieces(&mut app), before);
                assert_eq!(
                    app.world().resource::<SkillRoster>().players[0].swap_charges,
                    1
                );
                assert!(!app.world().resource::<SkillRoster>().skill_used_this_turn);
                if cancel {
                    select_key(&mut app, KeyCode::Escape);
                    assert!(!app.world().resource::<SkillTargetState>().is_active());
                    assert_eq!(
                        app.world().resource::<SkillRoster>().players[0].swap_charges,
                        1
                    );
                    continue;
                }
                app.world_mut()
                    .entity_mut(entity)
                    .remove::<PieceMoveAnimation>();
                app.update();
                assert!(
                    app.world()
                        .resource::<SkillTargetState>()
                        .can_confirm_swap()
                );
                assert_eq!(selection_pieces(&mut app), before);
                assert_eq!(
                    app.world().resource::<SkillRoster>().players[0].swap_charges,
                    1
                );
                app.world_mut()
                    .resource_mut::<SkillUiRequest>()
                    .queue_confirm_target();
                app.update();
                assert!(!app.world().resource::<SkillTargetState>().is_active());
                assert_eq!(
                    app.world().resource::<SkillRoster>().players[0].swap_charges,
                    0
                );
                assert!(app.world().resource::<SkillRoster>().skill_used_this_turn);
            }
        }
    }

    #[test]
    fn waiting_for_motion_does_not_refresh_a_stale_swap_snapshot() {
        use crate::plugins::animation_plugin::PieceMoveAnimation;
        let mut app = swap_selection_app(true);
        let entity = app
            .world_mut()
            .query::<(Entity, &PieceId)>()
            .iter(app.world())
            .find(|(_, id)| id.0 == 5)
            .unwrap()
            .0;
        app.world_mut()
            .entity_mut(entity)
            .insert(PieceMoveAnimation::test_pending());
        app.update();
        app.world_mut()
            .get_mut::<PieceState>(entity)
            .unwrap()
            .progress += 1;
        app.world_mut()
            .entity_mut(entity)
            .remove::<PieceMoveAnimation>();
        app.update();
        let before = selection_pieces(&mut app);
        app.world_mut()
            .resource_mut::<SkillUiRequest>()
            .queue_confirm_target();
        app.update();
        assert_eq!(selection_pieces(&mut app), before);
        assert!(!app.world().resource::<SkillTargetState>().is_active());
        assert_eq!(
            app.world().resource::<SkillRoster>().players[0].swap_charges,
            1
        );
        assert!(!app.world().resource::<SkillRoster>().skill_used_this_turn);
    }

    #[test]
    fn swap_reselection_and_cancellation_work_at_every_stage() {
        for stage in 0..3 {
            let mut app = swap_selection_app(false);
            let before = selection_pieces(&mut app);
            if stage >= 1 {
                select_key(&mut app, KeyCode::Digit2);
            }
            if stage >= 2 {
                select_key(&mut app, KeyCode::Digit1);
            }
            select_key(&mut app, KeyCode::Backspace);
            assert_eq!(
                app.world()
                    .resource::<SkillTargetState>()
                    .candidate_piece_ids(),
                &[1, 2]
            );
            assert_eq!(
                app.world().resource::<SkillTargetState>().swap_source(),
                None
            );
            select_key(&mut app, KeyCode::Escape);
            assert_eq!(selection_pieces(&mut app), before);
            assert_eq!(
                app.world().resource::<SkillRoster>().players[0].swap_charges,
                1
            );
        }
    }

    #[test]
    fn swap_confirmation_rejects_stale_piece_turn_charge_or_permission_without_mutation() {
        for invalid in 0..7 {
            let mut app = swap_selection_app(true);
            match invalid {
                0 => {
                    app.world_mut().resource_mut::<TurnState>().turn_index += 1;
                }
                1 => {
                    app.world_mut().resource_mut::<TurnState>().current_player = 3;
                }
                2 => {
                    app.world_mut().resource_mut::<SkillRoster>().players[0].swap_charges = 0;
                }
                3 => {
                    app.world_mut()
                        .resource_mut::<SkillRoster>()
                        .skill_used_this_turn = true;
                }
                4 => {
                    app.world_mut().resource_mut::<MatchResult>().finished = true;
                }
                5 => {
                    let mut query = app.world_mut().query::<&mut PieceState>();
                    for mut piece in query.iter_mut(app.world_mut()) {
                        piece.progress += 1;
                    }
                }
                _ => {
                    app.world_mut().resource_mut::<SkillRoster>().players[0]
                        .skill_blocked_this_turn = true;
                }
            }
            let before = selection_pieces(&mut app);
            let charges = app.world().resource::<SkillRoster>().players[0].swap_charges;
            select_key(&mut app, KeyCode::Enter);
            assert_eq!(selection_pieces(&mut app), before, "case {invalid}");
            assert_eq!(
                app.world().resource::<SkillRoster>().players[0].swap_charges,
                charges
            );
            assert!(!app.world().resource::<SkillTargetState>().is_active());
        }
    }

    fn swap_test_roster() -> PlayerRoster {
        use crate::gameplay::match_flow::{MatchSetup, PlayerSeat, build_match_resources};
        let setup = MatchSetup {
            mode: GameMode::TwoVsTwo,
            rule_set: crate::data::rule_set::RuleSet::Creative,
            ai_difficulty: crate::gameplay::ai::AiDifficulty::Normal,
            fast_mode: false,
            launch_rule: crate::domain::rules::LaunchRule::SixOnly,
            player_seats: PlayerSeat::ALL,
            pieces_per_player: 2,
            player_controls: [crate::domain::player::PlayerControl::Human; 4],
        };
        build_match_resources(&setup).1
    }

    #[test]
    fn skill_ui_request_carries_touch_cancel_target() {
        let mut request = SkillUiRequest::default();

        assert!(!request.take_cancel_target());

        request.queue_cancel_target();

        assert!(request.take_cancel_target());
        assert!(!request.take_cancel_target());
    }

    #[test]
    fn shield_target_collection_returns_all_legal_targets_in_piece_order() {
        let mut world = World::new();
        for (piece_id, owner_player_id, status, shield) in [
            (7, 1, crate::domain::piece::PieceStatus::Active, 0),
            (2, 1, crate::domain::piece::PieceStatus::Active, 1),
            (
                4,
                1,
                crate::domain::piece::PieceStatus::Active,
                MAX_PIECE_SHIELD,
            ),
            (1, 1, crate::domain::piece::PieceStatus::InHangar, 0),
            (3, 2, crate::domain::piece::PieceStatus::Active, 0),
        ] {
            world.spawn((
                PieceId(piece_id),
                HangarSlot(Vec2::ZERO),
                PieceState {
                    owner_player_id,
                    team_id: owner_player_id,
                    status,
                    progress: 5,
                    shield,
                    stack_shield: 0,
                    motion_serial: 0,
                },
                Transform::default(),
            ));
        }

        let mut system_state: SystemState<SkillPieceQuery<'_, '_>> = SystemState::new(&mut world);
        let query = system_state.get_mut(&mut world).unwrap();

        assert_eq!(collect_shield_targets_for_full_query(1, &query), vec![2, 7]);
    }

    #[test]
    fn shield_target_applies_only_after_selected_target_is_confirmed() {
        let mut world = World::new();
        for piece_id in [1, 2] {
            world.spawn((
                PieceId(piece_id),
                HangarSlot(Vec2::ZERO),
                PieceState {
                    owner_player_id: 1,
                    team_id: 1,
                    status: crate::domain::piece::PieceStatus::Active,
                    progress: piece_id.into(),
                    shield: 0,
                    stack_shield: 0,
                    motion_serial: 0,
                },
                Transform::from_xyz(piece_id as f32, 0.0, 0.0),
            ));
        }

        let mut skill_roster = SkillRoster {
            players: vec![PlayerSkillState {
                player_id: 1,
                dash_charges: 0,
                dash_armed: false,
                snipe_charges: 0,
                swap_charges: 0,
                shield_charges: 1,
                double_dice_charges: 0,
                double_dice_armed: false,
                skip_next_skill_turn: false,
                skill_blocked_this_turn: false,
            }],
            active_turn_player: Some(1),
            ..default()
        };
        let mut effect_queue = VisualEffectQueue::default();
        let mut system_state: SystemState<SkillPieceQuery<'_, '_>> = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world).unwrap();

        assert!(matches!(
            apply_shield_target(1, 2, &mut skill_roster, &mut effect_queue, &mut query,),
            ShieldTargetResult::Applied(1)
        ));
        assert_eq!(skill_roster.players[0].shield_charges, 0);
        assert!(skill_roster.skill_used_this_turn);

        let states = query
            .iter_mut()
            .map(|(piece_id, _, piece_state, _)| (piece_id.0, piece_state.shield))
            .collect::<Vec<_>>();
        assert_eq!(states, vec![(1, 0), (2, 1)]);
        assert_eq!(effect_queue.pending_count(), 1);
    }

    #[test]
    fn execute_snipe_consumes_normal_shield_before_hangar() {
        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::new(-10.0, 20.0)),
            PieceState {
                owner_player_id: 2,
                team_id: 2,
                status: crate::domain::piece::PieceStatus::Active,
                progress: 7,
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

        let note = execute_snipe(1, &mut query);
        let states = query
            .iter_mut()
            .map(|(piece_id, _, piece_state, _)| {
                (piece_id.0, piece_state.status, piece_state.shield)
            })
            .collect::<Vec<_>>();

        assert!(note.contains("removed a shield"));
        assert_eq!(
            states,
            vec![(1, crate::domain::piece::PieceStatus::Active, 0)]
        );
    }

    #[test]
    fn execute_snipe_sends_unshielded_piece_to_hangar() {
        let hangar = Vec2::new(30.0, -40.0);
        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(hangar),
            PieceState {
                owner_player_id: 2,
                team_id: 2,
                status: crate::domain::piece::PieceStatus::Active,
                progress: 7,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::from_xyz(100.0, 200.0, 0.0),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world).unwrap();

        let note = execute_snipe(1, &mut query);
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

        assert!(note.contains("back to hangar"));
        assert_eq!(
            states,
            vec![(
                1,
                crate::domain::piece::PieceStatus::InHangar,
                0,
                hangar.x,
                hangar.y
            )]
        );
    }

    #[test]
    fn find_active_target_piece_for_swap_returns_smallest_valid_main_route_piece_id() {
        let mut world = World::new();
        world.spawn((
            PieceId(9),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: crate::domain::piece::PieceStatus::Active,
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
                owner_player_id: 3,
                team_id: 1,
                status: crate::domain::piece::PieceStatus::Active,
                progress: HOME_ENTRY_PROGRESS + 1,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::default(),
        ));
        world.spawn((
            PieceId(4),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 3,
                team_id: 1,
                status: crate::domain::piece::PieceStatus::Active,
                progress: 10,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::default(),
        ));
        world.spawn((
            PieceId(7),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 5,
                team_id: 1,
                status: crate::domain::piece::PieceStatus::Active,
                progress: 12,
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
            collect_swap_targets(1, 1, GameMode::TwoVsTwo, &query),
            vec![4, 7]
        );
        assert_eq!(
            collect_swap_targets(1, 1, GameMode::OneVsOne, &query),
            Vec::<u8>::new()
        );
    }

    #[test]
    fn execute_swap_rebases_progress_and_ignores_interpolated_positions() {
        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: crate::domain::piece::PieceStatus::Active,
                progress: 3,
                shield: 1,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::from_xyz(-100.0, 0.0, 0.0),
        ));
        world.spawn((
            PieceId(2),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 3,
                team_id: 1,
                status: crate::domain::piece::PieceStatus::Active,
                progress: 18,
                shield: 0,
                stack_shield: 1,
                motion_serial: 0,
            },
            Transform::from_xyz(100.0, 50.0, 0.0),
        ));

        let mut system_state: SystemState<(
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
            MovingPieceQuery,
        )> = SystemState::new(&mut world);
        let (mut query, moving_query) = system_state.get_mut(&mut world).unwrap();

        let note = execute_swap(
            1,
            GameMode::TwoVsTwo,
            2,
            &BoardLayout::default(),
            &swap_test_roster(),
            &mut query,
            &moving_query,
        );
        let states = query
            .iter_mut()
            .map(|(piece_id, _, piece_state, transform)| {
                (
                    piece_id.0,
                    piece_state.owner_player_id,
                    piece_state.progress,
                    piece_state.shield,
                    piece_state.stack_shield,
                    transform.translation.x,
                    transform.translation.y,
                )
            })
            .collect::<Vec<_>>();

        assert!(note.contains("exchanged"));
        assert_eq!(
            states,
            vec![
                (1, 1, 5, 0, 1, -140.104, 200.104),
                (2, 3, 16, 1, 0, -156.104, 124.104)
            ]
        );
    }

    #[test]
    fn execute_swap_allows_enemy_target_outside_two_vs_two() {
        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: crate::domain::piece::PieceStatus::Active,
                progress: 4,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::from_xyz(-20.0, 0.0, 0.0),
        ));
        world.spawn((
            PieceId(3),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 2,
                team_id: 2,
                status: crate::domain::piece::PieceStatus::Active,
                progress: 16,
                shield: 1,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::from_xyz(20.0, 0.0, 0.0),
        ));
        let mut system_state: SystemState<(
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
            MovingPieceQuery,
        )> = SystemState::new(&mut world);
        let (mut query, moving_query) = system_state.get_mut(&mut world).unwrap();

        let note = execute_swap(
            1,
            GameMode::OneVsOne,
            3,
            &BoardLayout::default(),
            &swap_test_roster(),
            &mut query,
            &moving_query,
        );
        let states = query
            .iter_mut()
            .map(|(piece_id, _, piece_state, transform)| {
                (
                    piece_id.0,
                    piece_state.owner_player_id,
                    piece_state.progress,
                    piece_state.shield,
                    transform.translation.x,
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(note, "Swap exchanged piece #1 with piece #3");
        assert_eq!(
            states,
            vec![(1, 1, 29, 1, 156.104), (3, 2, 43, 0, -124.104)]
        );
    }

    #[test]
    fn execute_swap_rejects_home_lane_source_or_target() {
        let mut target_home_lane_world = World::new();
        target_home_lane_world.spawn((
            PieceId(1),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: crate::domain::piece::PieceStatus::Active,
                progress: 3,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::from_xyz(-100.0, 0.0, 0.0),
        ));
        target_home_lane_world.spawn((
            PieceId(2),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 3,
                team_id: 1,
                status: crate::domain::piece::PieceStatus::Active,
                progress: HOME_ENTRY_PROGRESS + 1,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::from_xyz(100.0, 50.0, 0.0),
        ));

        let mut target_system_state: SystemState<(
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
            MovingPieceQuery,
        )> = SystemState::new(&mut target_home_lane_world);
        let (mut target_query, moving_target_query) = target_system_state
            .get_mut(&mut target_home_lane_world)
            .unwrap();
        let target_note = execute_swap(
            1,
            GameMode::TwoVsTwo,
            2,
            &BoardLayout::default(),
            &swap_test_roster(),
            &mut target_query,
            &moving_target_query,
        );
        assert!(target_note.contains("not found on main route"));

        let mut source_home_lane_world = World::new();
        source_home_lane_world.spawn((
            PieceId(1),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: crate::domain::piece::PieceStatus::Active,
                progress: HOME_ENTRY_PROGRESS + 1,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::from_xyz(-100.0, 0.0, 0.0),
        ));
        source_home_lane_world.spawn((
            PieceId(2),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 3,
                team_id: 1,
                status: crate::domain::piece::PieceStatus::Active,
                progress: 18,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::from_xyz(100.0, 50.0, 0.0),
        ));

        let mut source_system_state: SystemState<(
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
            MovingPieceQuery,
        )> = SystemState::new(&mut source_home_lane_world);
        let (mut source_query, moving_source_query) = source_system_state
            .get_mut(&mut source_home_lane_world)
            .unwrap();
        let source_note = execute_swap(
            1,
            GameMode::TwoVsTwo,
            2,
            &BoardLayout::default(),
            &swap_test_roster(),
            &mut source_query,
            &moving_source_query,
        );
        assert!(source_note.contains("active piece not found"));
    }

    #[test]
    fn collect_snipe_targets_for_full_query_includes_home_lane_enemy() {
        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: crate::domain::piece::PieceStatus::Active,
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
                status: crate::domain::piece::PieceStatus::Active,
                progress: HOME_ENTRY_PROGRESS + 1,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::default(),
        ));
        world.spawn((
            PieceId(3),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 3,
                team_id: 3,
                status: crate::domain::piece::PieceStatus::Active,
                progress: 6,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
            Transform::default(),
        ));
        world.spawn((
            PieceId(4),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 4,
                team_id: 1,
                status: crate::domain::piece::PieceStatus::Active,
                progress: HOME_ENTRY_PROGRESS + 2,
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
            collect_snipe_targets_for_full_query(1, 1, &query),
            vec![2, 3]
        );
    }
}
