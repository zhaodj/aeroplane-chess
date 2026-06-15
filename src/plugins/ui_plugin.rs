use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::constants::HUD_Z_LAYER;
use crate::domain::piece::PieceState;
use crate::domain::player::PlayerControl;
use crate::gameplay::match_flow::{MatchConfig, MatchResult, PlayerRoster};
use crate::gameplay::skill_flow::{
    SkillRoster, can_use_skill_this_turn, is_active_teammate_piece, is_current_player_active_piece,
    is_legal_shield_target, is_legal_snipe_target, player_skill_state,
};
use crate::gameplay::turn_flow::{TurnInputState, TurnState};
use crate::platform::{DeviceProfile, PointerInputState};
use crate::plugins::menu_plugin::SoundSettingsOverlayState;
use crate::plugins::piece_plugin::PieceId;
use crate::plugins::skill_plugin::{SkillTargetState, SkillUiAction, SkillUiRequest};
use crate::plugins::turn_plugin::TurnUiRequest;
use crate::states::{AppState, GamePhase};

/// UI 插件：负责 HUD 与结果页渲染及交互。
pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HudFoldState>()
            .init_resource::<HudEventLogState>()
            .add_systems(OnEnter(AppState::InGame), spawn_hud)
            .add_systems(
                Update,
                (update_hud, handle_hud_toggle, handle_skill_panel_click)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(OnEnter(AppState::Result), spawn_result_screen)
            .add_systems(
                Update,
                handle_result_input.run_if(in_state(AppState::Result)),
            )
            .add_systems(OnExit(AppState::InGame), cleanup_hud)
            .add_systems(OnExit(AppState::Result), cleanup_result);
    }
}

#[derive(Component)]
/// HUD 实体分组标记。
struct HudEntity;

#[derive(Component)]
/// HUD 可折叠区标记。
struct HudCollapsible;

#[derive(Component)]
/// 结果页实体分组标记。
struct ResultEntity;

#[derive(Component)]
/// HUD 主状态文本节点。
struct HudPrimaryText;

#[derive(Component)]
/// HUD 技能文本节点。
struct HudSkillsText;

#[derive(Component)]
/// HUD 提示文本节点。
struct HudPromptText;

#[derive(Component)]
/// HUD 最近事件文本节点。
struct HudEventsText;

#[derive(Component)]
/// HUD 技能按钮元数据。
struct HudSkillButton {
    action: SkillUiAction,
}

#[derive(Component)]
/// HUD 掷骰按钮背景。
struct HudRollButton;

#[derive(Component)]
/// HUD 掷骰按钮文本。
struct HudRollButtonText;

#[derive(Component)]
/// HUD 折叠提示文本节点。
struct HudToggleHintText;

#[derive(Resource, Default)]
/// HUD 折叠状态。
struct HudFoldState {
    collapsed: bool,
}

#[derive(Resource, Default)]
/// HUD 最近事件去重日志。
struct HudEventLogState {
    entries: Vec<String>,
}

impl HudEventLogState {
    fn record(&mut self, message: &str) {
        if message.trim().is_empty() || self.entries.last().is_some_and(|last| last == message) {
            return;
        }

        self.entries.push(message.to_string());
        const MAX_EVENTS: usize = 4;
        if self.entries.len() > MAX_EVENTS {
            self.entries.remove(0);
        }
    }

    fn format(&self) -> String {
        if self.entries.is_empty() {
            "Events\n-".to_string()
        } else {
            format!("Events\n{}", self.entries.join("\n"))
        }
    }
}

type HudPrimaryQuery<'w, 's> = Query<
    'w,
    's,
    &'static mut Text,
    (
        With<HudPrimaryText>,
        Without<HudSkillsText>,
        Without<HudPromptText>,
        Without<HudEventsText>,
        Without<HudToggleHintText>,
        Without<HudRollButtonText>,
    ),
>;
type HudSkillsQuery<'w, 's> = Query<
    'w,
    's,
    &'static mut Text,
    (
        With<HudSkillsText>,
        Without<HudPrimaryText>,
        Without<HudPromptText>,
        Without<HudEventsText>,
        Without<HudToggleHintText>,
        Without<HudRollButtonText>,
    ),
>;
type HudPromptQuery<'w, 's> = Query<
    'w,
    's,
    &'static mut Text,
    (
        With<HudPromptText>,
        Without<HudPrimaryText>,
        Without<HudSkillsText>,
        Without<HudEventsText>,
        Without<HudToggleHintText>,
        Without<HudRollButtonText>,
    ),
>;
type HudEventsQuery<'w, 's> = Query<
    'w,
    's,
    &'static mut Text,
    (
        With<HudEventsText>,
        Without<HudPrimaryText>,
        Without<HudSkillsText>,
        Without<HudPromptText>,
        Without<HudToggleHintText>,
        Without<HudRollButtonText>,
    ),
>;
type HudRollButtonTextQuery<'w, 's> = Query<
    'w,
    's,
    &'static mut Text,
    (
        With<HudRollButtonText>,
        Without<HudPrimaryText>,
        Without<HudSkillsText>,
        Without<HudPromptText>,
        Without<HudEventsText>,
        Without<HudToggleHintText>,
    ),
>;
type HudToggleHintQuery<'w, 's> = Query<
    'w,
    's,
    &'static mut Text,
    (
        With<HudToggleHintText>,
        Without<HudPrimaryText>,
        Without<HudSkillsText>,
        Without<HudPromptText>,
        Without<HudEventsText>,
        Without<HudRollButtonText>,
    ),
>;
type HudSkillButtonQuery<'w, 's> =
    Query<'w, 's, (&'static HudSkillButton, &'static mut BackgroundColor), Without<HudRollButton>>;
type HudRollButtonQuery<'w, 's> =
    Query<'w, 's, &'static mut BackgroundColor, (With<HudRollButton>, Without<HudSkillButton>)>;
type HudPieceQuery<'w, 's> = Query<'w, 's, (&'static PieceId, &'static PieceState)>;

#[derive(SystemParam)]
struct HudData<'w, 's> {
    match_config: Res<'w, MatchConfig>,
    player_roster: Res<'w, PlayerRoster>,
    match_result: Res<'w, MatchResult>,
    skill_roster: Res<'w, SkillRoster>,
    skill_target_state: Res<'w, SkillTargetState>,
    input_state: Res<'w, TurnInputState>,
    turn_state: Res<'w, TurnState>,
    game_phase: Res<'w, State<GamePhase>>,
    hud_fold_state: Res<'w, HudFoldState>,
    event_log: ResMut<'w, HudEventLogState>,
    piece_query: HudPieceQuery<'w, 's>,
}

#[derive(SystemParam)]
struct HudTextQueries<'w, 's> {
    primary_query: HudPrimaryQuery<'w, 's>,
    skills_query: HudSkillsQuery<'w, 's>,
    prompt_query: HudPromptQuery<'w, 's>,
    events_query: HudEventsQuery<'w, 's>,
    roll_button_text_query: HudRollButtonTextQuery<'w, 's>,
    toggle_hint_query: HudToggleHintQuery<'w, 's>,
}

#[derive(SystemParam)]
struct SkillPanelClickParams<'w> {
    game_phase: Res<'w, State<GamePhase>>,
    match_result: Res<'w, MatchResult>,
    player_roster: Res<'w, PlayerRoster>,
    turn_state: Res<'w, TurnState>,
    hud_fold_state: Res<'w, HudFoldState>,
    skill_ui_request: ResMut<'w, SkillUiRequest>,
    turn_ui_request: ResMut<'w, TurnUiRequest>,
}

const HUD_PANEL_WIDTH: f32 = 276.0;
const HUD_PANEL_HEIGHT: f32 = 430.0;
const HUD_PANEL_MARGIN: f32 = 16.0;
const HUD_CONTENT_INSET: f32 = 12.0;
const HUD_TEXT_WIDTH: f32 = HUD_PANEL_WIDTH - HUD_CONTENT_INSET * 2.0;
const HUD_SKILL_ROW_TOPS: [f32; 5] = [158.0, 182.0, 206.0, 230.0, 254.0];
const HUD_SKILL_ROW_HEIGHT: f32 = 24.0;
const HUD_ROLL_BUTTON_TOP: f32 = 286.0;
const HUD_ROLL_BUTTON_HEIGHT: f32 = 30.0;

#[derive(Clone, Copy, Default)]
struct SkillBoardAvailability {
    shield_target: bool,
    snipe_target: bool,
    active_self: bool,
    active_teammate: bool,
}

impl SkillBoardAvailability {
    fn from_query(
        current_player: u8,
        current_team: u8,
        piece_query: &HudPieceQuery<'_, '_>,
    ) -> Self {
        let mut availability = Self::default();
        for (_, piece_state) in piece_query.iter() {
            availability.shield_target |= is_legal_shield_target(current_player, piece_state);
            availability.snipe_target |=
                is_legal_snipe_target(current_player, current_team, piece_state);
            availability.active_self |= is_current_player_active_piece(current_player, piece_state);
            availability.active_teammate |=
                is_active_teammate_piece(current_player, current_team, piece_state);
        }
        availability
    }
}

fn spawn_hud(
    mut commands: Commands,
    mut hud_fold_state: ResMut<HudFoldState>,
    mut event_log: ResMut<HudEventLogState>,
    device_profile: Res<DeviceProfile>,
) {
    event_log.entries.clear();
    hud_fold_state.collapsed = device_profile.should_start_hud_collapsed();
    let panel_visibility = if hud_fold_state.collapsed {
        Visibility::Hidden
    } else {
        Visibility::Visible
    };

    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(HUD_PANEL_MARGIN),
            right: Val::Px(HUD_PANEL_MARGIN),
            width: Val::Px(HUD_PANEL_WIDTH),
            height: Val::Px(HUD_PANEL_HEIGHT),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.98, 0.99, 1.0, 0.97)),
        BorderColor::all(Color::srgba(0.56, 0.64, 0.76, 0.36)),
        ZIndex(0),
        panel_visibility,
        Name::new("HudPanelBackdrop"),
        HudCollapsible,
        HudEntity,
    ));
    commands.spawn((
        Text::new("Loading HUD..."),
        TextFont {
            font_size: 16.0,
            ..default()
        },
        TextColor(Color::srgb(0.10, 0.16, 0.24)),
        TextLayout::new_with_linebreak(LineBreak::WordOrCharacter),
        ZIndex(1),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(28.0),
            right: Val::Px(HUD_PANEL_MARGIN + HUD_CONTENT_INSET),
            width: Val::Px(HUD_TEXT_WIDTH),
            ..default()
        },
        panel_visibility,
        Name::new("HudPrimaryText"),
        HudPrimaryText,
        HudCollapsible,
        HudEntity,
    ));
    for (row_index, action) in [
        SkillUiAction::Dash,
        SkillUiAction::Snipe,
        SkillUiAction::Swap,
        SkillUiAction::Shield,
        SkillUiAction::DoubleDice,
    ]
    .iter()
    .enumerate()
    {
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(HUD_PANEL_MARGIN + HUD_CONTENT_INSET),
                top: Val::Px(HUD_SKILL_ROW_TOPS[row_index]),
                width: Val::Px(HUD_TEXT_WIDTH),
                height: Val::Px(HUD_SKILL_ROW_HEIGHT),
                ..default()
            },
            BackgroundColor(skill_button_color(false, false)),
            ZIndex(1),
            panel_visibility,
            Name::new(format!("HudSkillButton{:?}", action)),
            HudSkillButton { action: *action },
            HudCollapsible,
            HudEntity,
        ));
    }
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(HUD_PANEL_MARGIN + HUD_CONTENT_INSET),
            top: Val::Px(HUD_ROLL_BUTTON_TOP),
            width: Val::Px(HUD_TEXT_WIDTH),
            height: Val::Px(HUD_ROLL_BUTTON_HEIGHT),
            ..default()
        },
        BackgroundColor(skill_button_color(false, false)),
        ZIndex(1),
        panel_visibility,
        Name::new("HudRollButton"),
        HudRollButton,
        HudCollapsible,
        HudEntity,
    ));
    commands.spawn((
        Text::new("Roll [Space]"),
        TextFont {
            font_size: 15.0,
            ..default()
        },
        TextColor(Color::srgb(0.09, 0.16, 0.24)),
        TextLayout::new_with_linebreak(LineBreak::WordOrCharacter),
        ZIndex(2),
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(HUD_PANEL_MARGIN + HUD_CONTENT_INSET + 10.0),
            top: Val::Px(HUD_ROLL_BUTTON_TOP + 5.0),
            width: Val::Px(HUD_TEXT_WIDTH - 20.0),
            ..default()
        },
        panel_visibility,
        Name::new("HudRollButtonText"),
        HudRollButtonText,
        HudCollapsible,
        HudEntity,
    ));
    commands.spawn((
        Text::new(""),
        TextFont {
            font_size: 16.0,
            ..default()
        },
        TextColor(Color::srgb(0.20, 0.28, 0.40)),
        TextLayout::new_with_linebreak(LineBreak::WordOrCharacter),
        ZIndex(1),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(132.0),
            right: Val::Px(HUD_PANEL_MARGIN + HUD_CONTENT_INSET),
            width: Val::Px(HUD_TEXT_WIDTH),
            ..default()
        },
        panel_visibility,
        Name::new("HudSkillsText"),
        HudSkillsText,
        HudCollapsible,
        HudEntity,
    ));
    commands.spawn((
        Text::new(""),
        TextFont {
            font_size: 16.0,
            ..default()
        },
        TextColor(Color::srgb(0.28, 0.35, 0.46)),
        TextLayout::new_with_linebreak(LineBreak::WordOrCharacter),
        ZIndex(1),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(326.0),
            right: Val::Px(HUD_PANEL_MARGIN + HUD_CONTENT_INSET),
            width: Val::Px(HUD_TEXT_WIDTH),
            ..default()
        },
        panel_visibility,
        Name::new("HudPromptText"),
        HudPromptText,
        HudCollapsible,
        HudEntity,
    ));
    commands.spawn((
        Text::new("Events\n-"),
        TextFont {
            font_size: 13.0,
            ..default()
        },
        TextColor(Color::srgb(0.18, 0.25, 0.34)),
        TextLayout::new_with_linebreak(LineBreak::WordOrCharacter),
        ZIndex(1),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(374.0),
            right: Val::Px(HUD_PANEL_MARGIN + HUD_CONTENT_INSET),
            width: Val::Px(HUD_TEXT_WIDTH),
            ..default()
        },
        panel_visibility,
        Name::new("HudEventsText"),
        HudEventsText,
        HudCollapsible,
        HudEntity,
    ));
    commands.spawn((
        Text::new("HUD: Expanded [Tab]"),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        TextColor(Color::srgb(0.18, 0.26, 0.38)),
        ZIndex(2),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(12.0),
            right: Val::Px(16.0),
            ..default()
        },
        Name::new("HudToggleHintText"),
        HudToggleHintText,
        HudEntity,
    ));
}

fn update_hud(
    mut data: HudData,
    mut text_queries: HudTextQueries,
    mut skill_button_query: HudSkillButtonQuery,
    mut roll_button_query: HudRollButtonQuery,
) {
    let Ok(mut primary_text) = text_queries.primary_query.single_mut() else {
        return;
    };
    let Ok(mut skills_text_node) = text_queries.skills_query.single_mut() else {
        return;
    };
    let Ok(mut prompt_text_node) = text_queries.prompt_query.single_mut() else {
        return;
    };
    let Ok(mut events_text_node) = text_queries.events_query.single_mut() else {
        return;
    };
    let Ok(mut roll_button_text) = text_queries.roll_button_text_query.single_mut() else {
        return;
    };
    let Ok(mut toggle_hint_text) = text_queries.toggle_hint_query.single_mut() else {
        return;
    };

    let roll_text = match data.turn_state.last_roll {
        Some(value) => value.to_string(),
        None => "-".to_string(),
    };
    let current_profile = data
        .player_roster
        .players
        .iter()
        .find(|player| player.state.player_id == data.turn_state.current_player);
    let current_control = current_profile
        .map(|player| match player.state.control {
            PlayerControl::Human => "Human",
            PlayerControl::Ai => "AI",
        })
        .unwrap_or("-");
    let board_availability = current_profile
        .map(|player| {
            SkillBoardAvailability::from_query(
                data.turn_state.current_player,
                player.state.team_id,
                &data.piece_query,
            )
        })
        .unwrap_or_default();
    let phase_label = match data.game_phase.get() {
        GamePhase::AwaitDice => "Roll",
        GamePhase::AwaitPieceSelect => "Choose Piece",
        GamePhase::CheckVictory => "Victory Check",
        _ => "Resolving",
    };
    let result_text = if data.match_result.finished {
        format!(
            "Result: Team {} wins",
            data.match_result.winner_team_id.unwrap_or_default()
        )
    } else {
        "Result: in progress".to_string()
    };
    let is_human_turn = matches!(current_control, "Human");
    let can_use_skill = can_use_skill_this_turn(&data.skill_roster, data.turn_state.current_player)
        && is_human_turn;
    let current_skills = player_skill_state(&data.skill_roster, data.turn_state.current_player);
    let roll_button_ready = is_human_turn
        && matches!(data.game_phase.get(), GamePhase::AwaitDice)
        && !data.match_result.finished;
    if let Ok(mut roll_button_background) = roll_button_query.single_mut() {
        *roll_button_background =
            BackgroundColor(skill_button_color(roll_button_ready, is_human_turn));
    }
    *roll_button_text = Text::new(if roll_button_ready {
        "Roll [Space]"
    } else {
        "Roll locked"
    });

    for (button, mut background) in &mut skill_button_query {
        let ready = current_skills
            .map(|skills| {
                is_skill_button_ready(
                    button.action,
                    skills,
                    can_use_skill,
                    data.game_phase.get(),
                    data.match_config.mode,
                    board_availability,
                )
            })
            .unwrap_or(false);
        *background = BackgroundColor(skill_button_color(ready, can_use_skill));
    }
    let stacked_hint = if matches!(data.game_phase.get(), GamePhase::AwaitPieceSelect) {
        "Highlighted teammate stacks share one shield.".to_string()
    } else {
        String::new()
    };
    let prompt_text = data
        .skill_target_state
        .prompt
        .as_deref()
        .or(data.input_state.prompt.as_deref())
        .unwrap_or("Space roll | Q Shield | S Snipe | A Swap | W Double | E Dash");
    let candidate_hint = candidate_piece_hint(&data.input_state, &data.skill_target_state);
    let skill_text = current_skills
        .map(|skills| {
            format_skill_panel(
                skills,
                is_human_turn,
                can_use_skill,
                data.match_config.mode,
                data.game_phase.get(),
            )
        })
        .unwrap_or_else(|| {
            "Skills\nDash [E]: -\nSnipe [S]: -\nSwap [A]: -\nShield [Q]: -\nDoubleDice [W]: -"
                .to_string()
        });

    if let Some(skill_note) = data.skill_roster.last_skill_action.as_deref() {
        data.event_log.record(skill_note);
    }
    if let Some(action_note) = data.turn_state.last_action.as_deref() {
        data.event_log.record(action_note);
    }
    if data.match_result.finished {
        data.event_log.record(&result_text);
    }

    *primary_text = Text::new(format!(
        "Mode: {:?} | AI: {:?}\nP{} ({}) | Round {}\nPhase: {} | Roll {}\n{}",
        data.match_config.mode,
        data.match_config.ai_difficulty,
        data.turn_state.current_player,
        current_control,
        data.turn_state.turn_index,
        phase_label,
        roll_text,
        result_text,
    ));
    *skills_text_node = Text::new(skill_text);
    *prompt_text_node = Text::new(if stacked_hint.is_empty() {
        format_prompt_text(prompt_text, &candidate_hint)
    } else {
        format!(
            "{}\n{}",
            format_prompt_text(prompt_text, &candidate_hint),
            stacked_hint
        )
    });
    *events_text_node = Text::new(data.event_log.format());
    *toggle_hint_text = Text::new(if data.hud_fold_state.collapsed {
        format!(
            "P{} {} | Roll {} | HUD [Tab]",
            data.turn_state.current_player, current_control, roll_text
        )
    } else {
        format!(
            "P{} {} | Roll {} | Hide [Tab]",
            data.turn_state.current_player, current_control, roll_text
        )
    });
}

fn handle_hud_toggle(
    keyboard: Res<ButtonInput<KeyCode>>,
    pointer: Res<PointerInputState>,
    windows: Query<&Window>,
    overlay_state: Res<SoundSettingsOverlayState>,
    mut hud_fold_state: ResMut<HudFoldState>,
    mut collapsible_query: Query<&mut Visibility, With<HudCollapsible>>,
) {
    let pointer_toggled = pointer.just_pressed_position().is_some_and(|position| {
        windows
            .single()
            .map(|window| hud_toggle_rect(window.width()).contains(position))
            .unwrap_or(false)
    });

    if overlay_state.open || (!keyboard.just_pressed(KeyCode::Tab) && !pointer_toggled) {
        return;
    }

    hud_fold_state.collapsed = !hud_fold_state.collapsed;
    let next_visibility = if hud_fold_state.collapsed {
        Visibility::Hidden
    } else {
        Visibility::Visible
    };
    for mut visibility in &mut collapsible_query {
        *visibility = next_visibility;
    }
}

#[derive(Clone, Copy)]
struct ScreenRect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl ScreenRect {
    fn contains(self, point: Vec2) -> bool {
        point.x >= self.x
            && point.x <= self.x + self.w
            && point.y >= self.y
            && point.y <= self.y + self.h
    }
}

fn hud_toggle_rect(window_width: f32) -> ScreenRect {
    ScreenRect {
        x: (window_width - 220.0).max(8.0),
        y: 8.0,
        w: 212.0,
        h: 36.0,
    }
}

fn candidate_piece_hint(
    input_state: &TurnInputState,
    skill_target_state: &SkillTargetState,
) -> Option<String> {
    let candidates = if !skill_target_state.candidate_piece_ids().is_empty() {
        skill_target_state.candidate_piece_ids()
    } else {
        input_state.candidate_piece_ids()
    };

    if candidates.is_empty() {
        return None;
    }

    Some(format!(
        "Pieces: {}",
        candidates
            .iter()
            .map(|piece_id| format!("#{piece_id}"))
            .collect::<Vec<_>>()
            .join(" ")
    ))
}

fn format_prompt_text(prompt_text: &str, candidate_hint: &Option<String>) -> String {
    match candidate_hint {
        Some(hint) => format!("Prompt: {prompt_text}\n{hint}"),
        None => format!("Prompt: {prompt_text}"),
    }
}

fn is_skill_button_ready(
    action: SkillUiAction,
    skills: &crate::gameplay::skill_flow::PlayerSkillState,
    can_use_skill: bool,
    phase: &GamePhase,
    mode: crate::data::game_mode::GameMode,
    board_availability: SkillBoardAvailability,
) -> bool {
    if !can_use_skill {
        return false;
    }
    match action {
        SkillUiAction::Dash => {
            matches!(phase, GamePhase::AwaitPieceSelect)
                && !skills.dash_armed
                && skills.dash_charges > 0
        }
        SkillUiAction::Snipe => {
            matches!(phase, GamePhase::AwaitDice)
                && skills.snipe_charges > 0
                && board_availability.snipe_target
        }
        SkillUiAction::Swap => {
            matches!(phase, GamePhase::AwaitDice)
                && mode == crate::data::game_mode::GameMode::TwoVsTwo
                && skills.swap_charges > 0
                && board_availability.active_self
                && board_availability.active_teammate
        }
        SkillUiAction::Shield => {
            matches!(phase, GamePhase::AwaitDice)
                && skills.shield_charges > 0
                && board_availability.shield_target
        }
        SkillUiAction::DoubleDice => {
            matches!(phase, GamePhase::AwaitDice)
                && !skills.double_dice_armed
                && skills.double_dice_charges > 0
        }
    }
}

fn skill_button_color(ready: bool, can_use_skill: bool) -> Color {
    if ready {
        Color::srgba(0.53, 0.77, 0.96, 0.42)
    } else if can_use_skill {
        Color::srgba(0.78, 0.82, 0.89, 0.28)
    } else {
        Color::srgba(0.70, 0.73, 0.79, 0.16)
    }
}

fn handle_skill_panel_click(
    pointer: Res<PointerInputState>,
    windows: Query<&Window>,
    overlay_state: Res<SoundSettingsOverlayState>,
    mut params: SkillPanelClickParams,
) {
    if params.hud_fold_state.collapsed
        || params.match_result.finished
        || overlay_state.input_captured
        || !matches!(
            params.game_phase.get(),
            GamePhase::AwaitDice | GamePhase::AwaitPieceSelect
        )
    {
        return;
    }
    let Some(cursor) = pointer.just_pressed_position() else {
        return;
    };

    let Some(current_player) = params
        .player_roster
        .players
        .iter()
        .find(|player| player.state.player_id == params.turn_state.current_player)
    else {
        return;
    };
    if current_player.state.control != PlayerControl::Human {
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };
    let (panel_left, panel_right) = hud_skill_panel_bounds(window.width());
    if cursor.x < panel_left || cursor.x > panel_right {
        return;
    }

    if cursor_in_vertical_band(cursor.y, HUD_ROLL_BUTTON_TOP, HUD_ROLL_BUTTON_HEIGHT) {
        if matches!(params.game_phase.get(), GamePhase::AwaitDice) {
            params.turn_ui_request.queue_roll();
        }
        return;
    }

    let Some(action) = skill_action_for_cursor_y(cursor.y) else {
        return;
    };
    params.skill_ui_request.queue(action);
}

fn hud_skill_panel_bounds(window_width: f32) -> (f32, f32) {
    let panel_left = (window_width - HUD_PANEL_MARGIN - HUD_PANEL_WIDTH).max(HUD_PANEL_MARGIN);
    let content_left = panel_left + HUD_CONTENT_INSET;
    (content_left, content_left + HUD_TEXT_WIDTH)
}

fn skill_action_for_cursor_y(cursor_y: f32) -> Option<SkillUiAction> {
    HUD_SKILL_ROW_TOPS
        .iter()
        .enumerate()
        .find_map(|(index, row_top)| {
            cursor_in_vertical_band(cursor_y, *row_top, HUD_SKILL_ROW_HEIGHT).then_some(index)
        })
        .and_then(|index| match index {
            0 => Some(SkillUiAction::Dash),
            1 => Some(SkillUiAction::Snipe),
            2 => Some(SkillUiAction::Swap),
            3 => Some(SkillUiAction::Shield),
            4 => Some(SkillUiAction::DoubleDice),
            _ => None,
        })
}

fn cursor_in_vertical_band(cursor_y: f32, top: f32, height: f32) -> bool {
    cursor_y >= top && cursor_y <= top + height
}

fn format_skill_panel(
    skills: &crate::gameplay::skill_flow::PlayerSkillState,
    is_human_turn: bool,
    can_use_skill: bool,
    mode: crate::data::game_mode::GameMode,
    phase: &GamePhase,
) -> String {
    let header = if is_human_turn {
        if skills.skill_blocked_this_turn {
            "Skills blocked by event"
        } else if can_use_skill {
            "Skills ready"
        } else {
            "Skill slot spent"
        }
    } else {
        "AI skills auto"
    };
    let dash_state = if skills.dash_armed {
        "armed +3 move"
    } else if skills.dash_charges == 0 {
        "cooldown"
    } else {
        "ready"
    };
    let snipe_state = if matches!(phase, GamePhase::ResolveSkillEffect) {
        "targeting"
    } else if skills.snipe_charges == 0 {
        "reloading"
    } else {
        "ready"
    };
    let swap_state = if mode == crate::data::game_mode::GameMode::TwoVsTwo {
        if skills.swap_charges == 0 {
            "empty"
        } else {
            "ready"
        }
    } else {
        "locked (2v2)"
    };
    let shield_state = if skills.shield_charges == 0 {
        "empty"
    } else {
        "ready"
    };
    let dice_state = if skills.double_dice_armed {
        "armed"
    } else if skills.double_dice_charges == 0 {
        "empty"
    } else {
        "ready"
    };

    format!(
        "{header}\nDash E: {dash_state} ({})\nSnipe S: {snipe_state} ({})\nSwap A: {swap_state} ({})\nShield Q: {shield_state} ({})\nDouble W: {dice_state} ({})",
        skills.dash_charges,
        skills.snipe_charges,
        skills.swap_charges,
        skills.shield_charges,
        skills.double_dice_charges,
    )
}

fn cleanup_hud(mut commands: Commands, query: Query<Entity, With<HudEntity>>) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
}

fn spawn_result_screen(mut commands: Commands, match_result: Res<MatchResult>) {
    let winner = match_result.winner_team_id.unwrap_or_default();
    commands.spawn((
        Sprite::from_color(Color::srgba(0.98, 0.99, 1.0, 0.94), Vec2::new(420.0, 220.0)),
        Transform::from_xyz(0.0, 40.0, HUD_Z_LAYER),
        Name::new("ResultBackdrop"),
        ResultEntity,
    ));
    commands.spawn((
        Text::new(format!(
            "Match Result\n\nTeam {} wins\n\nR: Restart Match\nEsc: Main Menu",
            winner
        )),
        TextFont {
            font_size: 40.0,
            ..default()
        },
        TextColor(Color::srgb(0.10, 0.16, 0.24)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Percent(28.0),
            left: Val::Percent(22.0),
            ..default()
        },
        Name::new("ResultText"),
        ResultEntity,
    ));
}

fn handle_result_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    overlay_state: Res<SoundSettingsOverlayState>,
    mut next_app_state: ResMut<NextState<AppState>>,
) {
    if overlay_state.open {
        return;
    }

    if keyboard.just_pressed(KeyCode::KeyR) {
        next_app_state.set(AppState::LoadingGame);
    } else if keyboard.just_pressed(KeyCode::Escape) {
        next_app_state.set(AppState::MainMenu);
    }
}

fn cleanup_result(mut commands: Commands, query: Query<Entity, With<ResultEntity>>) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
}
