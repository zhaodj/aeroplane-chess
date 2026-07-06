use bevy::ecs::system::SystemParam;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;

use crate::constants::{BOARD_WORLD_SIZE, gameplay_board_target_pixels};
use crate::data::game_mode::GameMode;
use crate::domain::piece::PieceState;
use crate::domain::player::PlayerControl;
use crate::domain::tile::TileKind;
use crate::gameplay::match_flow::{
    MatchConfig, MatchResult, PlayerProfile, PlayerRoster, PlayerSeat, hangar_center_for_seat,
};
use crate::gameplay::skill_flow::{
    FINISH_BOUNCES_PER_SKILL_REWARD, PlayerSkillState, SkillRoster, can_use_skill_this_turn,
    finish_bounce_count, is_current_player_dash_move_piece, is_current_player_swap_piece,
    is_legal_shield_target, is_legal_snipe_target, is_legal_swap_target, player_skill_state,
};
use crate::gameplay::turn_flow::{TurnState, player_has_finished_all_pieces};
use crate::i18n::{
    Language, LanguageSettings, LocalizedText, TextKey, skill_name as i18n_skill_name,
    skill_tip_body as i18n_skill_tip_body, skill_token as i18n_skill_token, text as i18n_text,
};
use crate::platform::{DeviceProfile, PointerInputState, PointerSource};
use crate::plugins::effects_plugin::EffectRevealDelays;
use crate::plugins::menu_plugin::{
    SoundSettingsOverlayState, global_settings_entry_screen_rect,
    sound_settings_overlay_blocks_input,
};
use crate::plugins::piece_plugin::PieceId;
use crate::plugins::skill_plugin::{SkillTargetState, SkillUiAction, SkillUiRequest};
use crate::plugins::turn_plugin::TurnUiRequest;
use crate::states::{AppState, GamePhase};

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerHudState>()
            .init_resource::<EventLogState>()
            .init_resource::<SkillTipState>()
            .add_systems(OnEnter(AppState::InGame), spawn_hud)
            .add_systems(
                Update,
                (
                    handle_player_hud_click,
                    handle_skill_tip_input,
                    handle_event_log_scroll,
                    update_player_hud_layout,
                    update_hud_content,
                    update_finish_bounce_charge_bar_layout,
                    update_skill_tip_content,
                    update_event_log_content,
                    update_event_notice_content,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(OnEnter(AppState::Result), spawn_result_screen)
            .add_systems(
                Update,
                (
                    handle_result_input.run_if(in_state(AppState::Result)),
                    handle_result_click.run_if(in_state(AppState::Result)),
                ),
            )
            .add_systems(OnExit(AppState::InGame), cleanup_hud)
            .add_systems(OnExit(AppState::Result), cleanup_result);
    }
}

#[derive(Component)]
struct HudEntity;

#[derive(Component)]
struct ResultEntity;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResultAction {
    RestartMatch,
    MainMenu,
}

#[derive(Component)]
struct PlayerHudEntry {
    player_id: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PlayerHudBadgeKind {
    Player,
    Team,
    Turn,
}

#[derive(Component)]
struct PlayerHudBadge {
    player_id: u8,
    kind: PlayerHudBadgeKind,
}

#[derive(Component)]
struct PlayerHudBadgeText {
    player_id: u8,
    kind: PlayerHudBadgeKind,
}

#[derive(Component)]
struct SharedSkillButton {
    action: SkillUiAction,
}

#[derive(Component)]
struct SharedSkillButtonIcon {
    action: SkillUiAction,
}

#[derive(Component)]
struct SharedSkillButtonBadge {
    action: SkillUiAction,
}

#[derive(Component)]
struct SharedSkillButtonText {
    action: SkillUiAction,
}

#[derive(Component)]
struct SharedSkillBlockMarker;

#[derive(Component)]
struct FinishBounceChargeBar;

#[derive(Component)]
struct FinishBounceChargeFill;

#[derive(Component)]
struct FinishBounceChargeText;

#[derive(Component)]
struct SkillTipPanel;

#[derive(Component)]
struct SkillTipTitle;

#[derive(Component)]
struct SkillTipBody;

#[derive(Component)]
struct SkillTipClose;

#[derive(Component)]
struct BoardRollButton;

#[derive(Component)]
struct BoardRollButtonText;

#[derive(Component)]
struct EventNoticePanel;

#[derive(Component)]
struct EventNoticeText;

#[derive(Component)]
struct EventLogToggle;

#[derive(Component)]
struct EventLogToggleText;

#[derive(Component)]
struct EventLogPanel;

#[derive(Component)]
struct EventLogScrollArea;

#[derive(Component)]
struct EventLogScrollbarThumb;

#[derive(Component)]
struct EventLogText;

#[derive(Resource, Default)]
pub struct PlayerHudState {
    event_log_expanded: bool,
    skill_tip_action: Option<SkillUiAction>,
    board_roll_button_visible: bool,
}

#[derive(Resource, Default)]
struct EventLogState {
    expanded: bool,
    scroll_to_bottom_requested: bool,
    entries: Vec<String>,
    last_turn_action_key: Option<u64>,
    last_skill_action_key: Option<u64>,
}

#[derive(Resource, Default)]
struct SkillTipState {
    visible_action: Option<SkillUiAction>,
    visible_from_hover: bool,
    pressed_action: Option<SkillUiAction>,
    pressed_source: Option<PointerSource>,
    pressed_started_at: f32,
    pressed_started_position: Vec2,
    hover_action: Option<SkillUiAction>,
    hover_started_at: f32,
    hover_left_at: Option<f32>,
}

type HudPieceQuery<'w, 's> = Query<'w, 's, (&'static PieceId, &'static PieceState)>;
type PlayerHudEntryLayoutQuery<'w, 's> =
    Query<'w, 's, (&'static PlayerHudEntry, &'static mut Node), Without<BoardRollButton>>;
type PlayerHudEntryStyleQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static PlayerHudEntry,
        &'static mut BackgroundColor,
        &'static mut BorderColor,
    ),
    (
        Without<PlayerHudBadge>,
        Without<SharedSkillButton>,
        Without<SharedSkillButtonBadge>,
        Without<BoardRollButton>,
    ),
>;
type PlayerHudBadgeStyleQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static PlayerHudBadge,
        &'static mut BackgroundColor,
        &'static mut BorderColor,
        &'static mut Visibility,
    ),
    (
        Without<PlayerHudEntry>,
        Without<SharedSkillButton>,
        Without<SharedSkillButtonBadge>,
        Without<BoardRollButton>,
    ),
>;
type PlayerHudBadgeTextQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static PlayerHudBadgeText,
        &'static mut Text,
        &'static mut TextColor,
    ),
    (Without<SharedSkillButtonText>, Without<BoardRollButtonText>),
>;
type SharedSkillButtonQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static SharedSkillButton,
        &'static mut BackgroundColor,
        &'static mut BorderColor,
    ),
    (
        Without<PlayerHudEntry>,
        Without<PlayerHudBadge>,
        Without<SharedSkillButtonBadge>,
        Without<BoardRollButton>,
    ),
>;
type SharedSkillButtonIconQuery<'w, 's> = Query<
    'w,
    's,
    (&'static SharedSkillButtonIcon, &'static mut ImageNode),
    (
        Without<PlayerHudEntry>,
        Without<PlayerHudBadge>,
        Without<SharedSkillButton>,
        Without<SharedSkillButtonBadge>,
    ),
>;
type SharedSkillButtonBadgeQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static SharedSkillButtonBadge,
        &'static mut BackgroundColor,
        &'static mut BorderColor,
    ),
    (
        Without<PlayerHudEntry>,
        Without<PlayerHudBadge>,
        Without<SharedSkillButton>,
        Without<BoardRollButton>,
    ),
>;
type SharedSkillButtonTextQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static SharedSkillButtonText,
        &'static mut Text,
        &'static mut TextColor,
    ),
    (Without<PlayerHudBadgeText>, Without<BoardRollButtonText>),
>;
type SharedSkillBlockMarkerQuery<'w, 's> =
    Query<'w, 's, &'static mut Visibility, With<SharedSkillBlockMarker>>;
type FinishBounceChargeFillQuery<'w, 's> = Query<
    'w,
    's,
    (&'static mut Node, &'static mut BackgroundColor),
    (
        With<FinishBounceChargeFill>,
        Without<FinishBounceChargeBar>,
        Without<PlayerHudEntry>,
        Without<PlayerHudBadge>,
        Without<SharedSkillButton>,
        Without<SharedSkillButtonBadge>,
        Without<BoardRollButton>,
    ),
>;
type FinishBounceChargeTextQuery<'w, 's> = Query<
    'w,
    's,
    (&'static mut Text, &'static mut TextColor),
    (
        With<FinishBounceChargeText>,
        Without<PlayerHudBadgeText>,
        Without<SharedSkillButtonText>,
        Without<BoardRollButtonText>,
    ),
>;
type BoardRollButtonQuery<'w, 's> = Query<
    'w,
    's,
    (&'static mut BackgroundColor, &'static mut Node),
    (
        With<BoardRollButton>,
        Without<PlayerHudEntry>,
        Without<PlayerHudBadge>,
        Without<SharedSkillButton>,
        Without<SharedSkillButtonBadge>,
    ),
>;
type BoardRollButtonTextQuery<'w, 's> = Query<
    'w,
    's,
    &'static mut Text,
    (
        With<BoardRollButtonText>,
        Without<PlayerHudBadgeText>,
        Without<SharedSkillButtonText>,
    ),
>;
type EventLogPanelVisibilityQuery<'w, 's> =
    Query<'w, 's, &'static mut Visibility, With<EventLogPanel>>;
type EventLogToggleTextQuery<'w, 's> =
    Query<'w, 's, &'static mut Text, (With<EventLogToggleText>, Without<EventLogText>)>;
type EventLogScrollAreaQuery<'w, 's> =
    Query<'w, 's, (&'static mut ScrollPosition, &'static ComputedNode), With<EventLogScrollArea>>;
type EventLogScrollbarThumbQuery<'w, 's> =
    Query<'w, 's, &'static mut Node, With<EventLogScrollbarThumb>>;
type EventLogTextQuery<'w, 's> =
    Query<'w, 's, &'static mut Text, (With<EventLogText>, Without<EventLogToggleText>)>;

#[derive(SystemParam)]
struct HudContentQueries<'w, 's> {
    entry_style_query: PlayerHudEntryStyleQuery<'w, 's>,
    badge_style_query: PlayerHudBadgeStyleQuery<'w, 's>,
    badge_text_query: PlayerHudBadgeTextQuery<'w, 's>,
    skill_button_query: SharedSkillButtonQuery<'w, 's>,
    skill_button_icon_query: SharedSkillButtonIconQuery<'w, 's>,
    skill_button_badge_query: SharedSkillButtonBadgeQuery<'w, 's>,
    skill_button_text_query: SharedSkillButtonTextQuery<'w, 's>,
    skill_block_marker_query: SharedSkillBlockMarkerQuery<'w, 's>,
    finish_bounce_charge_fill_query: FinishBounceChargeFillQuery<'w, 's>,
    finish_bounce_charge_text_query: FinishBounceChargeTextQuery<'w, 's>,
    board_roll_button_query: BoardRollButtonQuery<'w, 's>,
    board_roll_button_text_query: BoardRollButtonTextQuery<'w, 's>,
}

const HUD_EDGE_MARGIN: f32 = 10.0;
const HANGAR_BACKGROUND_WORLD_SIZE: f32 = 150.0;
const HUD_ENTRY_W: f32 = 146.0;
const HUD_ENTRY_H: f32 = 32.0;
const HUD_BADGE_H: f32 = 28.0;
const HUD_BADGE_GAP: f32 = 3.0;
const HUD_BADGE_PLAYER_W: f32 = 34.0;
const HUD_BADGE_TEAM_W: f32 = 28.0;
const HUD_BADGE_TURN_W: f32 = 28.0;
const HUD_TURN_INDICATOR_STEP_SECS: f32 = 0.34;
const SKILL_BUTTON_SIZE: f32 = 54.0;
const SKILL_ICON_SIZE: f32 = 42.0;
const SKILL_BADGE_W: f32 = 20.0;
const SKILL_BADGE_H: f32 = 20.0;
const SKILL_BLOCK_MARKER_W: f32 = 31.0;
const SKILL_BLOCK_MARKER_H: f32 = 14.0;
const SKILL_BUTTON_GAP: f32 = 8.0;
const FINISH_BOUNCE_CHARGE_BAR_H: f32 = 18.0;
const FINISH_BOUNCE_CHARGE_BAR_GAP: f32 = 6.0;
const FINISH_BOUNCE_CHARGE_BAR_TEXT_SIZE: f32 = 11.0;
const SKILL_TIP_W: f32 = 326.0;
const SKILL_TIP_H: f32 = 118.0;
const SKILL_TIP_GAP: f32 = 10.0;
const SKILL_TIP_LONG_PRESS_SECS: f32 = 0.52;
const SKILL_TIP_HOVER_SECS: f32 = 0.55;
const SKILL_TIP_HOVER_HIDE_SECS: f32 = 0.36;
const SKILL_TIP_PRESS_CANCEL_DISTANCE: f32 = 18.0;
const BOARD_ROLL_BUTTON_W: f32 = 64.0;
const BOARD_ROLL_BUTTON_H: f32 = 64.0;
const EVENT_LOG_TOGGLE_W: f32 = 92.0;
const EVENT_LOG_TOGGLE_H: f32 = 36.0;
const EVENT_LOG_PANEL_W: f32 = 360.0;
const EVENT_LOG_PANEL_H: f32 = 226.0;
const EVENT_LOG_PANEL_PADDING: f32 = 12.0;
const EVENT_LOG_GAP: f32 = 8.0;
const EVENT_LOG_SCROLLBAR_W: f32 = 8.0;
const EVENT_LOG_SCROLLBAR_GAP: f32 = 8.0;
const EVENT_LOG_SCROLLBAR_MIN_THUMB_H: f32 = 24.0;
const EVENT_LOG_SCROLL_STEP: f32 = 24.0;
const EVENT_LOG_MAX_ENTRIES: usize = 1000;
const EVENT_LOG_CONTENT_W: f32 = EVENT_LOG_PANEL_W
    - EVENT_LOG_PANEL_PADDING * 2.0
    - EVENT_LOG_SCROLLBAR_GAP
    - EVENT_LOG_SCROLLBAR_W;
const RESULT_PANEL_W: f32 = 460.0;
const RESULT_PANEL_H: f32 = 286.0;
const RESULT_BUTTON_W: f32 = 168.0;
const RESULT_BUTTON_H: f32 = 46.0;
const RESULT_BUTTON_GAP: f32 = 18.0;
const RESULT_BUTTON_TOP: f32 = 214.0;
const HUD_SKILL_ACTIONS: [SkillUiAction; 5] = [
    SkillUiAction::Dash,
    SkillUiAction::Snipe,
    SkillUiAction::Swap,
    SkillUiAction::Shield,
    SkillUiAction::DoubleDice,
];
const PLAYER_HUD_BADGES: [PlayerHudBadgeKind; 3] = [
    PlayerHudBadgeKind::Player,
    PlayerHudBadgeKind::Team,
    PlayerHudBadgeKind::Turn,
];

#[derive(Clone, Copy, Debug)]
pub(crate) struct ScreenRect {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) w: f32,
    pub(crate) h: f32,
}

impl ScreenRect {
    fn contains(self, point: Vec2) -> bool {
        point.x >= self.x
            && point.x <= self.x + self.w
            && point.y >= self.y
            && point.y <= self.y + self.h
    }

    fn overlaps(self, other: Self) -> bool {
        self.x < other.x + other.w
            && self.x + self.w > other.x
            && self.y < other.y + other.h
            && self.y + self.h > other.y
    }

    fn expanded(self, amount: f32) -> Self {
        Self {
            x: self.x - amount,
            y: self.y - amount,
            w: self.w + amount * 2.0,
            h: self.h + amount * 2.0,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct SkillBoardAvailability {
    shield_target: bool,
    snipe_target: bool,
    dash_move_target: bool,
    active_self: bool,
    swap_target: bool,
}

impl SkillBoardAvailability {
    fn from_query(
        current_player: u8,
        current_team: u8,
        mode: GameMode,
        piece_query: &HudPieceQuery<'_, '_>,
    ) -> Self {
        let mut availability = Self::default();
        for (_, piece_state) in piece_query.iter() {
            availability.shield_target |= is_legal_shield_target(current_player, piece_state);
            availability.snipe_target |=
                is_legal_snipe_target(current_player, current_team, piece_state);
            availability.dash_move_target |=
                is_current_player_dash_move_piece(current_player, piece_state);
            availability.active_self |= is_current_player_swap_piece(current_player, piece_state);
            availability.swap_target |=
                is_legal_swap_target(current_player, current_team, mode, piece_state);
        }
        availability
    }
}

fn spawn_hud(
    mut commands: Commands,
    mut hud_state: ResMut<PlayerHudState>,
    mut event_log: ResMut<EventLogState>,
    mut skill_tip: ResMut<SkillTipState>,
    player_roster: Res<PlayerRoster>,
    asset_server: Res<AssetServer>,
    language_settings: Res<LanguageSettings>,
) {
    let language = language_settings.language;
    hud_state.event_log_expanded = false;
    hud_state.skill_tip_action = None;
    *skill_tip = SkillTipState::default();
    event_log.expanded = false;
    event_log.scroll_to_bottom_requested = false;
    event_log.entries.clear();
    event_log.last_turn_action_key = None;
    event_log.last_skill_action_key = None;
    event_log
        .entries
        .push(i18n_text(language, TextKey::MatchStarted).to_string());

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                display: Display::None,
                width: Val::Px(BOARD_ROLL_BUTTON_W),
                height: Val::Px(BOARD_ROLL_BUTTON_H),
                border: UiRect::all(Val::Px(1.0)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                padding: UiRect::horizontal(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(skill_button_color(false, false)),
            BorderColor::all(Color::srgba(0.16, 0.22, 0.32, 0.28)),
            ZIndex(44),
            Name::new("BoardRollButton"),
            BoardRollButton,
            HudEntity,
        ))
        .with_children(|button| {
            button.spawn((
                Text::new(""),
                TextFont {
                    font_size: FontSize::Px(22.0),
                    ..default()
                },
                TextColor(Color::srgb(0.09, 0.16, 0.24)),
                TextLayout::justify(Justify::Center),
                Name::new("BoardRollButtonLabel"),
                BoardRollButtonText,
            ));
        });

    for action in HUD_SKILL_ACTIONS {
        commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(SKILL_BUTTON_SIZE),
                    height: Val::Px(SKILL_BUTTON_SIZE),
                    border: UiRect::all(Val::Px(1.0)),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    padding: UiRect::all(Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(skill_button_color(false, false)),
                BorderColor::all(Color::srgba(0.16, 0.22, 0.32, 0.20)),
                ZIndex(43),
                Name::new(format!(
                    "SharedSkillButton{}",
                    skill_action_name(action, language)
                )),
                SharedSkillButton { action },
                HudEntity,
            ))
            .with_children(|button| {
                button.spawn((
                    ImageNode {
                        color: skill_icon_color(false),
                        ..ImageNode::new(asset_server.load(skill_icon_asset_path(action)))
                    },
                    Node {
                        width: Val::Px(SKILL_ICON_SIZE),
                        height: Val::Px(SKILL_ICON_SIZE),
                        ..default()
                    },
                    Name::new(format!(
                        "SharedSkillButtonIcon{}",
                        skill_action_name(action, language)
                    )),
                    SharedSkillButtonIcon { action },
                ));
                button
                    .spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            top: Val::Px(0.0),
                            right: Val::Px(0.0),
                            width: Val::Px(SKILL_BADGE_W),
                            height: Val::Px(SKILL_BADGE_H),
                            border: UiRect::all(Val::Px(1.0)),
                            border_radius: BorderRadius::MAX,
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            padding: UiRect::horizontal(Val::Px(2.0)),
                            ..default()
                        },
                        BackgroundColor(skill_badge_color(false)),
                        BorderColor::all(skill_badge_border_color(false)),
                        ZIndex(1),
                        Name::new(format!(
                            "SharedSkillButtonBadge{}",
                            skill_action_name(action, language)
                        )),
                        SharedSkillButtonBadge { action },
                    ))
                    .with_children(|badge| {
                        badge.spawn((
                            Text::new(skill_badge_text(0)),
                            TextFont {
                                font_size: FontSize::Px(11.0),
                                ..default()
                            },
                            TextColor(skill_badge_text_color(false)),
                            TextLayout::justify(Justify::Center),
                            Name::new(format!(
                                "SharedSkillButtonBadgeText{}",
                                skill_action_name(action, language)
                            )),
                            SharedSkillButtonText { action },
                        ));
                    });
                button
                    .spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(3.0),
                            bottom: Val::Px(3.0),
                            width: Val::Px(SKILL_BLOCK_MARKER_W),
                            height: Val::Px(SKILL_BLOCK_MARKER_H),
                            border: UiRect::all(Val::Px(1.0)),
                            border_radius: BorderRadius::all(Val::Px(4.0)),
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            padding: UiRect::horizontal(Val::Px(2.0)),
                            ..default()
                        },
                        BackgroundColor(skill_block_marker_color()),
                        BorderColor::all(skill_block_marker_border_color()),
                        Visibility::Hidden,
                        ZIndex(3),
                        Name::new(format!(
                            "SharedSkillBlockMarker{}",
                            skill_action_name(action, language)
                        )),
                        SharedSkillBlockMarker,
                    ))
                    .with_children(|marker| {
                        marker.spawn((
                            Text::new(i18n_text(language, TextKey::SkillLocked)),
                            TextFont {
                                font_size: FontSize::Px(8.2),
                                ..default()
                            },
                            TextColor(Color::WHITE),
                            TextLayout::justify(Justify::Center),
                            Name::new(format!(
                                "SharedSkillBlockMarkerText{}",
                                skill_action_name(action, language)
                            )),
                            LocalizedText {
                                key: TextKey::SkillLocked,
                            },
                        ));
                    });
            });
    }

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(shared_skill_bar_width()),
                height: Val::Px(FINISH_BOUNCE_CHARGE_BAR_H),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                overflow: Overflow::clip(),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(finish_bounce_charge_bar_track_color()),
            BorderColor::all(finish_bounce_charge_bar_border_color()),
            Visibility::Hidden,
            ZIndex(42),
            Name::new("FinishBounceChargeBar"),
            FinishBounceChargeBar,
            HudEntity,
        ))
        .with_children(|bar| {
            bar.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    width: Val::Px(0.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(finish_bounce_charge_bar_fill_color(Color::WHITE)),
                Name::new("FinishBounceChargeFill"),
                FinishBounceChargeFill,
            ));
            bar.spawn((
                Text::new(""),
                TextFont {
                    font_size: FontSize::Px(FINISH_BOUNCE_CHARGE_BAR_TEXT_SIZE),
                    ..default()
                },
                TextColor(finish_bounce_charge_bar_text_color(false)),
                TextLayout::justify(Justify::Center),
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                ZIndex(1),
                Name::new("FinishBounceChargeText"),
                FinishBounceChargeText,
            ));
        });

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(SKILL_TIP_W),
                height: Val::Px(SKILL_TIP_H),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(8.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(5.0),
                padding: UiRect {
                    left: Val::Px(13.0),
                    right: Val::Px(34.0),
                    top: Val::Px(10.0),
                    bottom: Val::Px(10.0),
                },
                ..default()
            },
            BackgroundColor(skill_tip_panel_color()),
            BorderColor::all(skill_tip_border_color()),
            Visibility::Hidden,
            ZIndex(58),
            Name::new("SkillTipPanel"),
            SkillTipPanel,
            HudEntity,
        ))
        .with_children(|panel| {
            panel.spawn((
                Text::new(""),
                TextFont {
                    font_size: FontSize::Px(15.5),
                    ..default()
                },
                TextColor(Color::srgb(0.05, 0.08, 0.12)),
                TextLayout::justify(Justify::Left),
                Name::new("SkillTipTitle"),
                SkillTipTitle,
            ));
            panel.spawn((
                Text::new(""),
                TextFont {
                    font_size: FontSize::Px(12.5),
                    ..default()
                },
                TextColor(Color::srgb(0.15, 0.20, 0.28)),
                TextLayout::justify(Justify::Left),
                Name::new("SkillTipBody"),
                SkillTipBody,
            ));
            panel.spawn((
                Text::new(i18n_text(language, TextKey::SkillTipClose)),
                TextFont {
                    font_size: FontSize::Px(16.0),
                    ..default()
                },
                TextColor(Color::srgb(0.08, 0.12, 0.18)),
                Node {
                    position_type: PositionType::Absolute,
                    right: Val::Px(11.0),
                    top: Val::Px(8.0),
                    ..default()
                },
                Name::new("SkillTipClose"),
                SkillTipClose,
                LocalizedText {
                    key: TextKey::SkillTipClose,
                },
            ));
        });

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(shared_skill_bar_width()),
                height: Val::Px(SKILL_BUTTON_SIZE),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(8.0)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                padding: UiRect::horizontal(Val::Px(10.0)),
                ..default()
            },
            BackgroundColor(event_notice_panel_color(false)),
            BorderColor::all(event_notice_border_color(false)),
            Visibility::Hidden,
            ZIndex(43),
            Name::new("EventNoticePanel"),
            EventNoticePanel,
            HudEntity,
        ))
        .with_children(|panel| {
            panel.spawn((
                Text::new(""),
                TextFont {
                    font_size: FontSize::Px(13.0),
                    ..default()
                },
                TextColor(event_notice_text_color(false)),
                TextLayout::justify(Justify::Center),
                Node {
                    width: Val::Percent(100.0),
                    ..default()
                },
                Name::new("EventNoticeText"),
                EventNoticeText,
            ));
        });

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(EVENT_LOG_TOGGLE_W),
                height: Val::Px(EVENT_LOG_TOGGLE_H),
                border: UiRect::all(Val::Px(1.0)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                padding: UiRect::horizontal(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.84, 0.89, 0.96, 0.74)),
            BorderColor::all(Color::srgba(0.16, 0.22, 0.32, 0.30)),
            ZIndex(46),
            Name::new("EventLogToggle"),
            EventLogToggle,
            HudEntity,
        ))
        .with_children(|button| {
            button.spawn((
                Text::new(i18n_text(language, TextKey::EventLog)),
                TextFont {
                    font_size: FontSize::Px(16.0),
                    ..default()
                },
                TextColor(Color::srgb(0.10, 0.16, 0.24)),
                TextLayout::justify(Justify::Center),
                Name::new("EventLogToggleText"),
                EventLogToggleText,
                LocalizedText {
                    key: TextKey::EventLog,
                },
            ));
        });

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(EVENT_LOG_PANEL_W),
                height: Val::Px(EVENT_LOG_PANEL_H),
                border: UiRect::all(Val::Px(1.0)),
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(EVENT_LOG_SCROLLBAR_GAP),
                align_items: AlignItems::Stretch,
                padding: UiRect::all(Val::Px(EVENT_LOG_PANEL_PADDING)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.98, 0.99, 1.0, 0.96)),
            BorderColor::all(Color::srgba(0.16, 0.22, 0.32, 0.28)),
            Visibility::Hidden,
            ZIndex(45),
            Name::new("EventLogPanel"),
            EventLogPanel,
            HudEntity,
        ))
        .with_children(|panel| {
            panel
                .spawn((
                    Node {
                        width: Val::Px(EVENT_LOG_CONTENT_W),
                        height: Val::Percent(100.0),
                        flex_direction: FlexDirection::Column,
                        overflow: Overflow::scroll_y(),
                        ..default()
                    },
                    ScrollPosition(Vec2::ZERO),
                    Name::new("EventLogScrollArea"),
                    EventLogScrollArea,
                ))
                .with_children(|scroll_area| {
                    scroll_area.spawn((
                        Text::new(i18n_text(language, TextKey::MatchStarted)),
                        TextFont {
                            font_size: FontSize::Px(13.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.10, 0.16, 0.24)),
                        TextLayout::linebreak(LineBreak::WordOrCharacter),
                        Node {
                            width: Val::Percent(100.0),
                            ..default()
                        },
                        Name::new("EventLogText"),
                        EventLogText,
                    ));
                });
            panel
                .spawn((
                    Node {
                        width: Val::Px(EVENT_LOG_SCROLLBAR_W),
                        height: Val::Percent(100.0),
                        border_radius: BorderRadius::MAX,
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.55, 0.60, 0.68, 0.24)),
                    Name::new("EventLogScrollbarTrack"),
                ))
                .with_children(|track| {
                    track.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            top: Val::Px(0.0),
                            width: Val::Px(EVENT_LOG_SCROLLBAR_W),
                            height: Val::Px(EVENT_LOG_SCROLLBAR_MIN_THUMB_H),
                            border_radius: BorderRadius::MAX,
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.22, 0.28, 0.36, 0.62)),
                        Name::new("EventLogScrollbarThumb"),
                        EventLogScrollbarThumb,
                    ));
                });
        });

    for player in &player_roster.players {
        let player_id = player.state.player_id;
        commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(HUD_ENTRY_W),
                    height: Val::Px(HUD_ENTRY_H),
                    border: UiRect::all(Val::Px(0.0)),
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(HUD_BADGE_GAP),
                    align_items: AlignItems::Center,
                    justify_content: player_hud_badge_justify_content(player.seat),
                    padding: UiRect::all(Val::Px(0.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
                BorderColor::all(Color::srgba(0.0, 0.0, 0.0, 0.0)),
                ZIndex(32),
                Name::new(format!("PlayerHudEntryP{player_id}")),
                PlayerHudEntry { player_id },
                HudEntity,
            ))
            .with_children(|parent| {
                for kind in player_hud_badges_for_seat(player.seat) {
                    parent
                        .spawn((
                            Node {
                                width: Val::Px(player_hud_badge_width(kind)),
                                height: Val::Px(HUD_BADGE_H),
                                border: UiRect::all(Val::Px(1.0)),
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::Center,
                                padding: UiRect::horizontal(Val::Px(2.0)),
                                ..default()
                            },
                            BackgroundColor(player.color.mix(&Color::WHITE, 0.68).with_alpha(0.90)),
                            BorderColor::all(Color::srgba(0.10, 0.16, 0.24, 0.34)),
                            Name::new(format!(
                                "PlayerHudBadgeP{}{}",
                                player_id,
                                player_hud_badge_name(kind)
                            )),
                            PlayerHudBadge { player_id, kind },
                        ))
                        .with_children(|badge| {
                            badge.spawn((
                                Text::new(""),
                                TextFont {
                                    font_size: FontSize::Px(12.0),
                                    ..default()
                                },
                                TextColor(Color::srgb(0.08, 0.12, 0.18)),
                                TextLayout::justify(Justify::Center),
                                Name::new(format!(
                                    "PlayerHudBadgeTextP{}{}",
                                    player_id,
                                    player_hud_badge_name(kind)
                                )),
                                PlayerHudBadgeText { player_id, kind },
                            ));
                        });
                }
            });
    }
}

fn update_player_hud_layout(
    windows: Query<&Window>,
    device_profile: Res<DeviceProfile>,
    match_config: Res<MatchConfig>,
    player_roster: Res<PlayerRoster>,
    mut entry_query: PlayerHudEntryLayoutQuery,
    mut board_roll_button_query: Query<
        &mut Node,
        (
            With<BoardRollButton>,
            Without<PlayerHudEntry>,
            Without<SharedSkillButton>,
            Without<EventLogToggle>,
            Without<EventLogPanel>,
            Without<EventNoticePanel>,
        ),
    >,
    mut skill_button_query: Query<
        (&SharedSkillButton, &mut Node),
        (
            Without<PlayerHudEntry>,
            Without<BoardRollButton>,
            Without<EventLogToggle>,
            Without<EventLogPanel>,
            Without<EventNoticePanel>,
        ),
    >,
    mut event_notice_panel_query: Query<
        &mut Node,
        (
            With<EventNoticePanel>,
            Without<PlayerHudEntry>,
            Without<BoardRollButton>,
            Without<SharedSkillButton>,
            Without<EventLogToggle>,
            Without<EventLogPanel>,
        ),
    >,
    mut event_log_toggle_query: Query<
        &mut Node,
        (
            With<EventLogToggle>,
            Without<EventLogPanel>,
            Without<EventNoticePanel>,
            Without<PlayerHudEntry>,
            Without<BoardRollButton>,
            Without<SharedSkillButton>,
        ),
    >,
    mut event_log_panel_query: Query<
        &mut Node,
        (
            With<EventLogPanel>,
            Without<EventLogToggle>,
            Without<EventNoticePanel>,
            Without<PlayerHudEntry>,
            Without<BoardRollButton>,
            Without<SharedSkillButton>,
        ),
    >,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let window_width = window.width();
    let window_height = window.height();

    for mut node in &mut board_roll_button_query {
        apply_rect_to_node(
            &mut node,
            board_roll_button_rect(window_width, window_height, *device_profile),
        );
    }

    for (button, mut node) in &mut skill_button_query {
        let rect =
            shared_skill_button_rect(window_width, window_height, *device_profile, button.action);
        apply_rect_to_node(&mut node, rect);
        node.display = if match_config.rule_set.skills_enabled() {
            Display::Flex
        } else {
            Display::None
        };
    }

    for mut node in &mut event_notice_panel_query {
        apply_rect_to_node(
            &mut node,
            event_notice_panel_rect(window_width, window_height, *device_profile),
        );
        node.display =
            if match_config.rule_set.effective_tile_kind(TileKind::Event) == TileKind::Event {
                Display::Flex
            } else {
                Display::None
            };
    }

    for mut node in &mut event_log_toggle_query {
        apply_rect_to_node(
            &mut node,
            event_log_toggle_rect(window_width, window_height, *device_profile),
        );
    }

    for mut node in &mut event_log_panel_query {
        apply_rect_to_node(
            &mut node,
            event_log_panel_rect(window_width, window_height, *device_profile),
        );
    }

    for (entry, mut node) in &mut entry_query {
        let Some(player) = player_profile(&player_roster, entry.player_id) else {
            continue;
        };
        let rect = player_hud_entry_rect(window_width, window_height, *device_profile, player.seat);
        apply_rect_to_node(&mut node, rect);
        node.justify_content = player_hud_badge_justify_content(player.seat);
    }
}

fn update_finish_bounce_charge_bar_layout(
    windows: Query<&Window>,
    device_profile: Res<DeviceProfile>,
    match_config: Res<MatchConfig>,
    mut bar_query: Query<
        (&mut Node, &mut Visibility),
        (With<FinishBounceChargeBar>, Without<FinishBounceChargeFill>),
    >,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let rect = finish_bounce_charge_bar_rect(window.width(), window.height(), *device_profile);
    for (mut node, mut visibility) in &mut bar_query {
        apply_rect_to_node(&mut node, rect);
        *visibility = if match_config.rule_set.skills_enabled() {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

fn update_hud_content(
    match_config: Res<MatchConfig>,
    player_roster: Res<PlayerRoster>,
    skill_roster: Res<SkillRoster>,
    skill_target_state: Res<SkillTargetState>,
    turn_state: Res<TurnState>,
    game_phase: Res<State<GamePhase>>,
    match_result: Res<MatchResult>,
    reveal_delays: Res<EffectRevealDelays>,
    language_settings: Res<LanguageSettings>,
    time: Res<Time>,
    mut hud_state: ResMut<PlayerHudState>,
    piece_query: HudPieceQuery,
    mut queries: HudContentQueries,
) {
    let language = language_settings.language;
    let elapsed_secs = time.elapsed_secs();
    let turn_indicator_pulse = player_hud_turn_indicator_pulse(elapsed_secs);
    for (entry, mut background, mut border) in &mut queries.entry_style_query {
        let is_current = entry.player_id == turn_state.current_player;
        let color = player_roster
            .players
            .iter()
            .find(|player| player.state.player_id == entry.player_id)
            .map(|player| player.color)
            .unwrap_or(Color::srgb(0.78, 0.82, 0.89));
        *background = BackgroundColor(if is_current {
            color.mix(&Color::WHITE, 0.36).with_alpha(0.16)
        } else {
            Color::srgba(0.0, 0.0, 0.0, 0.0)
        });
        *border = BorderColor::all(if is_current {
            color.mix(&Color::WHITE, 0.24).with_alpha(0.38)
        } else {
            Color::srgba(0.10, 0.16, 0.24, 0.0)
        });
    }

    for (badge, mut background, mut border, mut visibility) in &mut queries.badge_style_query {
        let is_current = badge.player_id == turn_state.current_player;
        let color = player_roster
            .players
            .iter()
            .find(|player| player.state.player_id == badge.player_id)
            .map(|player| player.color)
            .unwrap_or(Color::srgb(0.78, 0.82, 0.89));
        *visibility = player_hud_badge_visibility(badge.kind, is_current);
        *background = BackgroundColor(player_hud_badge_color(
            badge.kind,
            color,
            is_current,
            turn_indicator_pulse,
        ));
        *border = BorderColor::all(player_hud_badge_border_color(
            badge.kind,
            is_current,
            turn_indicator_pulse,
        ));
    }

    for (badge_text, mut text, mut text_color) in &mut queries.badge_text_query {
        let Some(player) = player_roster
            .players
            .iter()
            .find(|player| player.state.player_id == badge_text.player_id)
        else {
            *text = Text::new("");
            continue;
        };
        let is_current = badge_text.player_id == turn_state.current_player;
        let player_finished = player_has_finished_all_pieces(
            badge_text.player_id,
            piece_query.iter().map(|(_, piece_state)| piece_state),
        );
        *text = Text::new(player_hud_badge_text(
            badge_text.kind,
            player,
            is_current,
            player_finished,
            language,
            elapsed_secs,
        ));
        *text_color = TextColor(player_hud_badge_text_color(
            badge_text.kind,
            is_current,
            turn_indicator_pulse,
        ));
    }

    let current_profile = player_roster
        .players
        .iter()
        .find(|player| player.state.player_id == turn_state.current_player);
    let current_human_turn = current_profile.is_some_and(|player| {
        player.state.control == PlayerControl::Human && !match_result.finished
    });
    let skills_enabled = match_config.rule_set.skills_enabled();
    let finish_bounce_progress = if skills_enabled {
        finish_bounce_count(&skill_roster, turn_state.current_player)
    } else {
        0
    };
    let current_player_color = current_profile
        .map(|player| player.color)
        .unwrap_or(Color::srgb(0.58, 0.64, 0.74));
    for (mut node, mut background) in &mut queries.finish_bounce_charge_fill_query {
        node.width = Val::Px(finish_bounce_charge_fill_width(
            finish_bounce_progress,
            shared_skill_bar_width(),
        ));
        *background = BackgroundColor(finish_bounce_charge_bar_fill_color(current_player_color));
    }
    for (mut text, mut text_color) in &mut queries.finish_bounce_charge_text_query {
        *text = Text::new(finish_bounce_charge_bar_text(
            finish_bounce_progress,
            skills_enabled,
            language,
        ));
        *text_color = TextColor(finish_bounce_charge_bar_text_color(skills_enabled));
    }

    let mut can_use_skill = false;
    let mut board_availability = SkillBoardAvailability::default();
    if skills_enabled && let Some(player) = current_profile {
        can_use_skill =
            current_human_turn && can_use_skill_this_turn(&skill_roster, player.state.player_id);
        board_availability = SkillBoardAvailability::from_query(
            player.state.player_id,
            player.state.team_id,
            match_config.mode,
            &piece_query,
        );
    }

    let current_skills = player_skill_state(&skill_roster, turn_state.current_player);
    let show_skill_block_marker =
        skills_enabled && skill_block_marker_visible(current_skills, match_result.finished);
    for (button, mut background, mut border) in &mut queries.skill_button_query {
        let ready = current_skills
            .map(|skills| {
                is_skill_button_ready(
                    button.action,
                    skills,
                    can_use_skill,
                    game_phase.get(),
                    match_config.mode,
                    board_availability,
                ) && reveal_delays
                    .visible_skill_charge(button.action, skill_charge(button.action, skills))
                    > 0
            })
            .unwrap_or(false);
        *background = BackgroundColor(skill_button_color(ready, can_use_skill));
        *border = BorderColor::all(if ready {
            Color::srgba(0.06, 0.10, 0.16, 0.72)
        } else {
            Color::srgba(0.16, 0.22, 0.32, 0.18)
        });
    }

    for (button_icon, mut image_node) in &mut queries.skill_button_icon_query {
        let ready = current_skills
            .map(|skills| {
                is_skill_button_ready(
                    button_icon.action,
                    skills,
                    can_use_skill,
                    game_phase.get(),
                    match_config.mode,
                    board_availability,
                ) && reveal_delays.visible_skill_charge(
                    button_icon.action,
                    skill_charge(button_icon.action, skills),
                ) > 0
            })
            .unwrap_or(false);
        image_node.color = skill_icon_color(ready);
    }

    for (button_badge, mut background, mut border) in &mut queries.skill_button_badge_query {
        let ready = current_skills
            .map(|skills| {
                is_skill_button_ready(
                    button_badge.action,
                    skills,
                    can_use_skill,
                    game_phase.get(),
                    match_config.mode,
                    board_availability,
                ) && reveal_delays.visible_skill_charge(
                    button_badge.action,
                    skill_charge(button_badge.action, skills),
                ) > 0
            })
            .unwrap_or(false);
        *background = BackgroundColor(skill_badge_color(ready));
        *border = BorderColor::all(skill_badge_border_color(ready));
    }

    for (button_text, mut text, mut text_color) in &mut queries.skill_button_text_query {
        let charges = current_skills
            .map(|skills| {
                reveal_delays.visible_skill_charge(
                    button_text.action,
                    skill_charge(button_text.action, skills),
                )
            })
            .unwrap_or_default();
        let ready = current_skills
            .map(|skills| {
                is_skill_button_ready(
                    button_text.action,
                    skills,
                    can_use_skill,
                    game_phase.get(),
                    match_config.mode,
                    board_availability,
                ) && charges > 0
            })
            .unwrap_or(false);
        *text = Text::new(skill_badge_text(charges));
        *text_color = TextColor(skill_badge_text_color(ready));
    }

    for mut visibility in &mut queries.skill_block_marker_query {
        *visibility = if show_skill_block_marker {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }

    let roll_ready = current_human_turn && matches!(game_phase.get(), GamePhase::AwaitDice);
    let cancel_target_ready = current_human_turn
        && matches!(game_phase.get(), GamePhase::ResolveSkillEffect)
        && skill_target_state.is_active();
    let board_roll_hit_target_active = roll_ready || cancel_target_ready;
    let board_roll_visual_visible = false;
    hud_state.board_roll_button_visible = board_roll_hit_target_active;

    for (mut background, mut node) in &mut queries.board_roll_button_query {
        node.display = if board_roll_visual_visible {
            Display::Flex
        } else {
            Display::None
        };
        *background = BackgroundColor(skill_button_color(
            board_roll_visual_visible,
            current_human_turn,
        ));
    }
    for mut text in &mut queries.board_roll_button_text_query {
        *text = Text::new(roll_button_text(cancel_target_ready, language));
    }
}

fn update_skill_tip_content(
    skill_tip: Res<SkillTipState>,
    language_settings: Res<LanguageSettings>,
    windows: Query<&Window>,
    device_profile: Res<DeviceProfile>,
    mut panel_query: Query<(&mut Node, &mut Visibility), With<SkillTipPanel>>,
    mut text_query: Query<
        (
            &mut Text,
            Option<&SkillTipTitle>,
            Option<&SkillTipBody>,
            Option<&SkillTipClose>,
        ),
        Or<(With<SkillTipTitle>, With<SkillTipBody>, With<SkillTipClose>)>,
    >,
) {
    let language = language_settings.language;
    let Ok(window) = windows.single() else {
        return;
    };
    let visible_action = skill_tip.visible_action;

    for (mut node, mut visibility) in &mut panel_query {
        if let Some(action) = visible_action {
            apply_rect_to_node(
                &mut node,
                skill_tip_rect(window.width(), window.height(), *device_profile, action),
            );
            *visibility = Visibility::Visible;
        } else {
            *visibility = Visibility::Hidden;
        }
    }

    for (mut text, title, body, close) in &mut text_query {
        if let Some(action) = visible_action {
            if title.is_some() {
                *text = Text::new(skill_action_name(action, language));
            } else if body.is_some() {
                *text = Text::new(skill_tip_body(action, language));
            } else if close.is_some() {
                *text = Text::new(i18n_text(language, TextKey::SkillTipClose));
            }
        } else {
            *text = Text::new("");
        }
    }
}

fn update_event_log_content(
    mut event_log: ResMut<EventLogState>,
    turn_state: Res<TurnState>,
    skill_roster: Res<SkillRoster>,
    language_settings: Res<LanguageSettings>,
    mut panel_visibility_query: EventLogPanelVisibilityQuery,
    mut toggle_text_query: EventLogToggleTextQuery,
    mut scroll_area_query: EventLogScrollAreaQuery,
    mut scrollbar_thumb_query: EventLogScrollbarThumbQuery,
    mut log_text_query: EventLogTextQuery,
) {
    let language = language_settings.language;
    sync_event_log(&mut event_log, &turn_state, &skill_roster, language);
    for mut visibility in &mut panel_visibility_query {
        *visibility = if event_log.expanded {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    for mut text in &mut toggle_text_query {
        *text = Text::new(i18n_text(
            language,
            if event_log.expanded {
                TextKey::EventLogOpen
            } else {
                TextKey::EventLogClosed
            },
        ));
    }
    let entries = format_event_log_entries(&event_log.entries, language);
    for mut text in &mut log_text_query {
        *text = Text::new(entries.clone());
    }
    for (mut scroll_position, computed) in &mut scroll_area_query {
        let max_scroll_y = event_log_scroll_max_y(computed);
        if event_log.scroll_to_bottom_requested {
            scroll_position.y = max_scroll_y;
        } else {
            scroll_position.y = scroll_position.y.clamp(0.0, max_scroll_y);
        }

        for mut thumb_node in &mut scrollbar_thumb_query {
            apply_event_log_scrollbar_thumb(&mut thumb_node, &scroll_position, computed);
        }
    }
    event_log.scroll_to_bottom_requested = false;
}

fn update_event_notice_content(
    turn_state: Res<TurnState>,
    language_settings: Res<LanguageSettings>,
    mut panel_query: Query<
        (&mut Visibility, &mut BackgroundColor, &mut BorderColor),
        With<EventNoticePanel>,
    >,
    mut text_query: Query<(&mut Text, &mut TextColor), With<EventNoticeText>>,
) {
    let notice = turn_state
        .last_action
        .as_deref()
        .and_then(|action| event_notice_text_from_action(action, language_settings.language));
    let active = notice.is_some();

    for (mut visibility, mut background, mut border) in &mut panel_query {
        *visibility = if active {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        *background = BackgroundColor(event_notice_panel_color(active));
        *border = BorderColor::all(event_notice_border_color(active));
    }

    for (mut text, mut text_color) in &mut text_query {
        *text = Text::new(notice.clone().unwrap_or_default());
        *text_color = TextColor(event_notice_text_color(active));
    }
}

fn handle_event_log_scroll(
    mut mouse_wheel_reader: MessageReader<MouseWheel>,
    windows: Query<&Window>,
    device_profile: Res<DeviceProfile>,
    event_log: Res<EventLogState>,
    mut scroll_area_query: Query<
        (&mut ScrollPosition, &Node, &ComputedNode),
        With<EventLogScrollArea>,
    >,
) {
    if !event_log.expanded {
        mouse_wheel_reader.clear();
        return;
    }
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    if !event_log_panel_rect(window.width(), window.height(), *device_profile).contains(cursor) {
        return;
    }
    let Ok((mut scroll_position, node, computed)) = scroll_area_query.single_mut() else {
        return;
    };
    let max_scroll_y = event_log_scroll_max_y(computed);
    if max_scroll_y <= 0.0 || node.overflow.y != OverflowAxis::Scroll {
        return;
    }
    for mouse_wheel in mouse_wheel_reader.read() {
        let dy = match mouse_wheel.unit {
            MouseScrollUnit::Line => mouse_wheel.y * EVENT_LOG_SCROLL_STEP,
            MouseScrollUnit::Pixel => mouse_wheel.y,
        };
        scroll_position.y = (scroll_position.y - dy).clamp(0.0, max_scroll_y);
    }
}

fn handle_skill_tip_input(
    pointer: Res<PointerInputState>,
    windows: Query<&Window>,
    device_profile: Res<DeviceProfile>,
    overlay_state: Res<SoundSettingsOverlayState>,
    match_config: Res<MatchConfig>,
    player_roster: Res<PlayerRoster>,
    skill_roster: Res<SkillRoster>,
    game_phase: Res<State<GamePhase>>,
    match_result: Res<MatchResult>,
    turn_state: Res<TurnState>,
    time: Res<Time>,
    piece_query: HudPieceQuery,
    mut hud_state: ResMut<PlayerHudState>,
    mut skill_tip: ResMut<SkillTipState>,
    mut skill_ui_request: ResMut<SkillUiRequest>,
) {
    if sound_settings_overlay_blocks_input(&overlay_state)
        || match_result.finished
        || !match_config.rule_set.skills_enabled()
    {
        clear_skill_tip_interaction(&mut skill_tip);
        hud_state.skill_tip_action = skill_tip.visible_action;
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };
    let now = time.elapsed_secs();
    let window_width = window.width();
    let window_height = window.height();
    let profile = *device_profile;

    if let Some(cursor) = pointer.just_pressed_position() {
        if let Some(action) = skill_tip.visible_action
            && skill_tip_rect(window_width, window_height, profile, action).contains(cursor)
        {
            hide_skill_tip(&mut skill_tip);
            hud_state.skill_tip_action = skill_tip.visible_action;
            return;
        }

        if let Some(action) = skill_action_at_point(cursor, window_width, window_height, profile) {
            skill_tip.pressed_action = Some(action);
            skill_tip.pressed_source = pointer.source();
            skill_tip.pressed_started_at = now;
            skill_tip.pressed_started_position = cursor;
            skill_tip.hover_left_at = None;
            if let Some(release_position) = pointer.just_released_position() {
                queue_skill_from_release_if_ready(
                    action,
                    release_position,
                    window_width,
                    window_height,
                    profile,
                    &skill_tip,
                    &match_config,
                    &player_roster,
                    &skill_roster,
                    game_phase.get(),
                    &match_result,
                    &turn_state,
                    &piece_query,
                    &mut skill_ui_request,
                );
                skill_tip.pressed_action = None;
                skill_tip.pressed_source = None;
            }
            hud_state.skill_tip_action = skill_tip.visible_action;
            return;
        }
    }

    if pointer.is_pressed() {
        update_pressed_skill_tip(
            &mut skill_tip,
            &pointer,
            window_width,
            window_height,
            profile,
            now,
        );
        hud_state.skill_tip_action = skill_tip.visible_action;
        return;
    }

    if let Some(release_position) = pointer.just_released_position() {
        if let Some(action) = skill_tip.pressed_action {
            queue_skill_from_release_if_ready(
                action,
                release_position,
                window_width,
                window_height,
                profile,
                &skill_tip,
                &match_config,
                &player_roster,
                &skill_roster,
                game_phase.get(),
                &match_result,
                &turn_state,
                &piece_query,
                &mut skill_ui_request,
            );
        }
        skill_tip.pressed_action = None;
        skill_tip.pressed_source = None;
        hud_state.skill_tip_action = skill_tip.visible_action;
        return;
    }

    update_hover_skill_tip(&mut skill_tip, &pointer, window, profile, now);
    hud_state.skill_tip_action = skill_tip.visible_action;
}

fn should_queue_skill_from_release(
    release_inside: bool,
    press_opened_tip: bool,
    skill_ready: bool,
) -> bool {
    release_inside && !press_opened_tip && skill_ready
}

fn queue_skill_from_release_if_ready(
    action: SkillUiAction,
    release_position: Vec2,
    window_width: f32,
    window_height: f32,
    device_profile: DeviceProfile,
    skill_tip: &SkillTipState,
    match_config: &MatchConfig,
    player_roster: &PlayerRoster,
    skill_roster: &SkillRoster,
    game_phase: &GamePhase,
    match_result: &MatchResult,
    turn_state: &TurnState,
    piece_query: &HudPieceQuery<'_, '_>,
    skill_ui_request: &mut SkillUiRequest,
) {
    let release_inside = skill_release_inside_button(
        action,
        release_position,
        window_width,
        window_height,
        device_profile,
    );
    let press_opened_tip = skill_press_opened_persistent_tip(skill_tip, action);
    let skill_ready = release_inside
        && skill_ready_for_current_context(
            action,
            match_config,
            player_roster,
            skill_roster,
            game_phase,
            match_result,
            turn_state,
            piece_query,
        );
    if should_queue_skill_from_release(release_inside, press_opened_tip, skill_ready) {
        skill_ui_request.queue(action);
    }
}

fn skill_release_inside_button(
    action: SkillUiAction,
    release_position: Vec2,
    window_width: f32,
    window_height: f32,
    device_profile: DeviceProfile,
) -> bool {
    shared_skill_button_rect(window_width, window_height, device_profile, action)
        .expanded(4.0)
        .contains(release_position)
}

fn skill_press_opened_persistent_tip(skill_tip: &SkillTipState, action: SkillUiAction) -> bool {
    skill_tip.visible_action == Some(action) && !skill_tip.visible_from_hover
}

fn handle_player_hud_click(
    pointer: Res<PointerInputState>,
    windows: Query<&Window>,
    device_profile: Res<DeviceProfile>,
    overlay_state: Res<SoundSettingsOverlayState>,
    match_config: Res<MatchConfig>,
    player_roster: Res<PlayerRoster>,
    game_phase: Res<State<GamePhase>>,
    match_result: Res<MatchResult>,
    turn_state: Res<TurnState>,
    skill_target_state: Res<SkillTargetState>,
    skill_tip: Res<SkillTipState>,
    mut hud_state: ResMut<PlayerHudState>,
    mut event_log: ResMut<EventLogState>,
    mut skill_ui_request: ResMut<SkillUiRequest>,
    mut turn_ui_request: ResMut<TurnUiRequest>,
) {
    if sound_settings_overlay_blocks_input(&overlay_state) || match_result.finished {
        return;
    }
    let Some(cursor) = pointer.just_pressed_position() else {
        return;
    };
    let Ok(window) = windows.single() else {
        return;
    };

    if let Some(action) = skill_tip.visible_action
        && skill_tip_rect(window.width(), window.height(), *device_profile, action).contains(cursor)
    {
        return;
    }

    let log_toggle = event_log_toggle_rect(window.width(), window.height(), *device_profile);
    if log_toggle.contains(cursor) {
        event_log.expanded = !event_log.expanded;
        if event_log.expanded {
            event_log.scroll_to_bottom_requested = true;
        }
        hud_state.event_log_expanded = event_log.expanded;
        return;
    }
    if event_log.expanded
        && event_log_panel_rect(window.width(), window.height(), *device_profile).contains(cursor)
    {
        return;
    }

    let board_roll_rect = board_roll_button_rect(window.width(), window.height(), *device_profile);
    if board_roll_rect.contains(cursor) {
        let current_human_turn = player_roster.players.iter().any(|player| {
            player.state.player_id == turn_state.current_player
                && player.state.control == PlayerControl::Human
        });
        let roll_ready = current_human_turn && matches!(game_phase.get(), GamePhase::AwaitDice);
        let cancel_target_ready = match_config.rule_set.skills_enabled()
            && current_human_turn
            && matches!(game_phase.get(), GamePhase::ResolveSkillEffect)
            && skill_target_state.is_active();
        if roll_ready {
            turn_ui_request.queue_roll();
            return;
        }
        if cancel_target_ready {
            skill_ui_request.queue_cancel_target();
            return;
        }
    }

    if match_config.rule_set.skills_enabled() {
        for action in HUD_SKILL_ACTIONS {
            let rect =
                shared_skill_button_rect(window.width(), window.height(), *device_profile, action);
            if rect.contains(cursor) {
                return;
            }
        }
    }

    for player in &player_roster.players {
        let rect = player_hud_entry_rect(
            window.width(),
            window.height(),
            *device_profile,
            player.seat,
        );
        if rect.contains(cursor) {
            return;
        }
    }
}

fn update_pressed_skill_tip(
    skill_tip: &mut SkillTipState,
    pointer: &PointerInputState,
    window_width: f32,
    window_height: f32,
    device_profile: DeviceProfile,
    now: f32,
) {
    let Some(action) = skill_tip.pressed_action else {
        return;
    };
    let Some(current_position) = pointer.current_position() else {
        return;
    };
    let button_rect =
        shared_skill_button_rect(window_width, window_height, device_profile, action).expanded(8.0);
    let moved_too_far = current_position.distance(skill_tip.pressed_started_position)
        > SKILL_TIP_PRESS_CANCEL_DISTANCE
        && !button_rect.contains(current_position);
    if moved_too_far {
        skill_tip.pressed_action = None;
        skill_tip.pressed_source = None;
        return;
    }

    let long_press_secs = match skill_tip.pressed_source {
        Some(PointerSource::Touch) | Some(PointerSource::Mouse) | None => SKILL_TIP_LONG_PRESS_SECS,
    };
    if now - skill_tip.pressed_started_at >= long_press_secs {
        show_skill_tip(skill_tip, action, false);
    }
}

fn update_hover_skill_tip(
    skill_tip: &mut SkillTipState,
    pointer: &PointerInputState,
    window: &Window,
    device_profile: DeviceProfile,
    now: f32,
) {
    if pointer.current_source() == Some(PointerSource::Touch) {
        return;
    }

    let Some(cursor) = pointer
        .current_position()
        .or_else(|| window.cursor_position())
    else {
        return;
    };
    let window_width = window.width();
    let window_height = window.height();

    if let Some(action) = skill_action_at_point(cursor, window_width, window_height, device_profile)
    {
        if skill_tip.hover_action != Some(action) {
            skill_tip.hover_action = Some(action);
            skill_tip.hover_started_at = now;
        }
        skill_tip.hover_left_at = None;
        if now - skill_tip.hover_started_at >= SKILL_TIP_HOVER_SECS {
            show_skill_tip(skill_tip, action, true);
        }
        return;
    }

    if skill_tip.visible_from_hover
        && let Some(action) = skill_tip.visible_action
        && skill_tip_rect(window_width, window_height, device_profile, action).contains(cursor)
    {
        skill_tip.hover_left_at = None;
        return;
    }

    skill_tip.hover_action = None;
    if skill_tip.visible_from_hover && skill_tip.visible_action.is_some() {
        let left_at = *skill_tip.hover_left_at.get_or_insert(now);
        if now - left_at >= SKILL_TIP_HOVER_HIDE_SECS {
            hide_skill_tip(skill_tip);
        }
    } else {
        skill_tip.hover_left_at = None;
    }
}

fn show_skill_tip(skill_tip: &mut SkillTipState, action: SkillUiAction, from_hover: bool) {
    skill_tip.visible_action = Some(action);
    skill_tip.visible_from_hover = from_hover;
}

fn hide_skill_tip(skill_tip: &mut SkillTipState) {
    skill_tip.visible_action = None;
    skill_tip.visible_from_hover = false;
    skill_tip.hover_action = None;
    skill_tip.hover_left_at = None;
}

fn clear_skill_tip_interaction(skill_tip: &mut SkillTipState) {
    hide_skill_tip(skill_tip);
    skill_tip.pressed_action = None;
    skill_tip.pressed_source = None;
}

fn skill_action_at_point(
    point: Vec2,
    window_width: f32,
    window_height: f32,
    device_profile: DeviceProfile,
) -> Option<SkillUiAction> {
    HUD_SKILL_ACTIONS.iter().copied().find(|action| {
        shared_skill_button_rect(window_width, window_height, device_profile, *action)
            .contains(point)
    })
}

fn skill_ready_for_current_context(
    action: SkillUiAction,
    match_config: &MatchConfig,
    player_roster: &PlayerRoster,
    skill_roster: &SkillRoster,
    game_phase: &GamePhase,
    match_result: &MatchResult,
    turn_state: &TurnState,
    piece_query: &HudPieceQuery<'_, '_>,
) -> bool {
    if !match_config.rule_set.skills_enabled() {
        return false;
    }

    let Some(current_profile) = player_profile(player_roster, turn_state.current_player) else {
        return false;
    };
    let current_human_turn =
        current_profile.state.control == PlayerControl::Human && !match_result.finished;
    let can_use_skill =
        current_human_turn && can_use_skill_this_turn(skill_roster, turn_state.current_player);
    let board_availability = SkillBoardAvailability::from_query(
        current_profile.state.player_id,
        current_profile.state.team_id,
        match_config.mode,
        piece_query,
    );
    player_skill_state(skill_roster, turn_state.current_player)
        .map(|skills| {
            is_skill_button_ready(
                action,
                skills,
                can_use_skill,
                game_phase,
                match_config.mode,
                board_availability,
            )
        })
        .unwrap_or(false)
}

pub fn player_hud_point_is_interactive(
    point: Vec2,
    window: &Window,
    device_profile: DeviceProfile,
    player_roster: &PlayerRoster,
    hud_state: &PlayerHudState,
    skills_enabled: bool,
) -> bool {
    if top_right_controls_rect(window.width()).contains(point)
        || event_log_toggle_rect(window.width(), window.height(), device_profile).contains(point)
    {
        return true;
    }

    if hud_state.board_roll_button_visible
        && board_roll_button_rect(window.width(), window.height(), device_profile).contains(point)
    {
        return true;
    }

    if hud_state.event_log_expanded
        && event_log_panel_rect(window.width(), window.height(), device_profile).contains(point)
    {
        return true;
    }

    if skills_enabled
        && let Some(action) = hud_state.skill_tip_action
        && skill_tip_rect(window.width(), window.height(), device_profile, action).contains(point)
    {
        return true;
    }

    if skills_enabled {
        for action in HUD_SKILL_ACTIONS {
            if shared_skill_button_rect(window.width(), window.height(), device_profile, action)
                .contains(point)
            {
                return true;
            }
        }
    }

    for player in &player_roster.players {
        let rect =
            player_hud_entry_rect(window.width(), window.height(), device_profile, player.seat);
        if rect.contains(point) {
            return true;
        }
    }

    false
}

fn player_profile(player_roster: &PlayerRoster, player_id: u8) -> Option<&PlayerProfile> {
    player_roster
        .players
        .iter()
        .find(|player| player.state.player_id == player_id)
}

fn gameplay_board_screen_rect(
    window_width: f32,
    window_height: f32,
    device_profile: DeviceProfile,
) -> ScreenRect {
    let size = gameplay_board_target_pixels(
        window_width,
        window_height,
        device_profile.board_screen_padding(),
    );
    ScreenRect {
        x: (window_width - size) * 0.5,
        y: (window_height - size) * 0.5,
        w: size,
        h: size,
    }
}

fn player_hud_entry_rect(
    window_width: f32,
    window_height: f32,
    device_profile: DeviceProfile,
    seat: PlayerSeat,
) -> ScreenRect {
    let board = gameplay_board_screen_rect(window_width, window_height, device_profile);
    let hangar = seat_hangar_screen_rect(board, seat);
    let mut rect = match seat {
        PlayerSeat::Blue => ScreenRect {
            x: hangar.x,
            y: hangar.y - HUD_ENTRY_H,
            w: HUD_ENTRY_W,
            h: HUD_ENTRY_H,
        },
        PlayerSeat::Red => ScreenRect {
            x: hangar.x + hangar.w - HUD_ENTRY_W,
            y: hangar.y - HUD_ENTRY_H,
            w: HUD_ENTRY_W,
            h: HUD_ENTRY_H,
        },
        PlayerSeat::Green => ScreenRect {
            x: hangar.x,
            y: hangar.y + hangar.h,
            w: HUD_ENTRY_W,
            h: HUD_ENTRY_H,
        },
        PlayerSeat::Yellow => ScreenRect {
            x: hangar.x + hangar.w - HUD_ENTRY_W,
            y: hangar.y + hangar.h,
            w: HUD_ENTRY_W,
            h: HUD_ENTRY_H,
        },
    };
    rect = clamp_rect_to_window(rect, window_width, window_height);

    let top_right_controls = top_right_controls_rect(window_width);
    if seat == PlayerSeat::Red && rect.overlaps(top_right_controls) {
        rect.y = (top_right_controls.y + top_right_controls.h + HUD_EDGE_MARGIN)
            .min((window_height - rect.h - HUD_EDGE_MARGIN).max(HUD_EDGE_MARGIN));
    }
    rect
}

fn seat_hangar_screen_rect(board: ScreenRect, seat: PlayerSeat) -> ScreenRect {
    let center = world_to_board_screen(board, hangar_center_for_seat(seat));
    let size = HANGAR_BACKGROUND_WORLD_SIZE * board.w / BOARD_WORLD_SIZE;
    ScreenRect {
        x: center.x - size * 0.5,
        y: center.y - size * 0.5,
        w: size,
        h: size,
    }
}

fn world_to_board_screen(board: ScreenRect, world_pos: Vec2) -> Vec2 {
    let half = BOARD_WORLD_SIZE * 0.5;
    Vec2::new(
        board.x + board.w * ((world_pos.x + half) / BOARD_WORLD_SIZE),
        board.y + board.h * ((half - world_pos.y) / BOARD_WORLD_SIZE),
    )
}

fn clamp_rect_to_window(rect: ScreenRect, window_width: f32, window_height: f32) -> ScreenRect {
    let max_x = (window_width - rect.w - HUD_EDGE_MARGIN).max(HUD_EDGE_MARGIN);
    let max_y = (window_height - rect.h - HUD_EDGE_MARGIN).max(HUD_EDGE_MARGIN);
    ScreenRect {
        x: rect.x.clamp(HUD_EDGE_MARGIN, max_x),
        y: rect.y.clamp(HUD_EDGE_MARGIN, max_y),
        ..rect
    }
}

fn top_right_controls_rect(window_width: f32) -> ScreenRect {
    let (x, y, w, h) = global_settings_entry_screen_rect(window_width);
    ScreenRect { x, y, w, h }
}

fn board_roll_button_rect(
    window_width: f32,
    window_height: f32,
    device_profile: DeviceProfile,
) -> ScreenRect {
    let board = gameplay_board_screen_rect(window_width, window_height, device_profile);
    roll_button_rect_at(board, 0.5, 0.5)
}

fn roll_button_rect_at(board: ScreenRect, x_ratio: f32, y_ratio: f32) -> ScreenRect {
    ScreenRect {
        x: board.x + board.w * x_ratio - BOARD_ROLL_BUTTON_W * 0.5,
        y: board.y + board.h * y_ratio - BOARD_ROLL_BUTTON_H * 0.5,
        w: BOARD_ROLL_BUTTON_W,
        h: BOARD_ROLL_BUTTON_H,
    }
}

fn shared_skill_bar_width() -> f32 {
    SKILL_BUTTON_SIZE * HUD_SKILL_ACTIONS.len() as f32
        + SKILL_BUTTON_GAP * HUD_SKILL_ACTIONS.len().saturating_sub(1) as f32
}

fn skill_action_index(action: SkillUiAction) -> usize {
    HUD_SKILL_ACTIONS
        .iter()
        .position(|candidate| *candidate == action)
        .unwrap_or_default()
}

fn shared_skill_bar_rect(
    window_width: f32,
    window_height: f32,
    device_profile: DeviceProfile,
) -> ScreenRect {
    let board = gameplay_board_screen_rect(window_width, window_height, device_profile);
    let total_width = shared_skill_bar_width();
    let total_height =
        SKILL_BUTTON_SIZE + FINISH_BOUNCE_CHARGE_BAR_GAP + FINISH_BOUNCE_CHARGE_BAR_H;
    let start_x = (board.x + (board.w - total_width) * 0.5).clamp(
        HUD_EDGE_MARGIN,
        (window_width - total_width - HUD_EDGE_MARGIN).max(HUD_EDGE_MARGIN),
    );
    let y = (board.y + board.h + HUD_EDGE_MARGIN).clamp(
        HUD_EDGE_MARGIN,
        (window_height - total_height - HUD_EDGE_MARGIN).max(HUD_EDGE_MARGIN),
    );
    ScreenRect {
        x: start_x,
        y,
        w: total_width,
        h: SKILL_BUTTON_SIZE,
    }
}

pub(crate) fn shared_skill_button_rect(
    window_width: f32,
    window_height: f32,
    device_profile: DeviceProfile,
    action: SkillUiAction,
) -> ScreenRect {
    let bar = shared_skill_bar_rect(window_width, window_height, device_profile);
    let index = skill_action_index(action) as f32;
    ScreenRect {
        x: bar.x + index * (SKILL_BUTTON_SIZE + SKILL_BUTTON_GAP),
        y: bar.y,
        w: SKILL_BUTTON_SIZE,
        h: SKILL_BUTTON_SIZE,
    }
}

fn finish_bounce_charge_bar_rect(
    window_width: f32,
    window_height: f32,
    device_profile: DeviceProfile,
) -> ScreenRect {
    let skill_bar = shared_skill_bar_rect(window_width, window_height, device_profile);
    ScreenRect {
        x: skill_bar.x,
        y: skill_bar.y + skill_bar.h + FINISH_BOUNCE_CHARGE_BAR_GAP,
        w: skill_bar.w,
        h: FINISH_BOUNCE_CHARGE_BAR_H,
    }
}

fn event_notice_panel_rect(
    window_width: f32,
    window_height: f32,
    device_profile: DeviceProfile,
) -> ScreenRect {
    let board = gameplay_board_screen_rect(window_width, window_height, device_profile);
    let width = shared_skill_bar_width();
    let x = (board.x + (board.w - width) * 0.5).clamp(
        HUD_EDGE_MARGIN,
        (window_width - width - HUD_EDGE_MARGIN).max(HUD_EDGE_MARGIN),
    );
    let y = (board.y - HUD_EDGE_MARGIN - SKILL_BUTTON_SIZE).clamp(
        HUD_EDGE_MARGIN,
        (window_height - SKILL_BUTTON_SIZE - HUD_EDGE_MARGIN).max(HUD_EDGE_MARGIN),
    );
    ScreenRect {
        x,
        y,
        w: width,
        h: SKILL_BUTTON_SIZE,
    }
}

fn skill_tip_rect(
    window_width: f32,
    window_height: f32,
    device_profile: DeviceProfile,
    action: SkillUiAction,
) -> ScreenRect {
    let button = shared_skill_button_rect(window_width, window_height, device_profile, action);
    let width = SKILL_TIP_W.min((window_width - HUD_EDGE_MARGIN * 2.0).max(0.0));
    let height = SKILL_TIP_H.min((window_height - HUD_EDGE_MARGIN * 2.0).max(0.0));
    let x = (button.x + button.w * 0.5 - width * 0.5).clamp(
        HUD_EDGE_MARGIN,
        (window_width - width - HUD_EDGE_MARGIN).max(HUD_EDGE_MARGIN),
    );
    let y_above = button.y - height - SKILL_TIP_GAP;
    let y = if y_above >= HUD_EDGE_MARGIN {
        y_above
    } else {
        button.y + button.h + SKILL_TIP_GAP
    }
    .clamp(
        HUD_EDGE_MARGIN,
        (window_height - height - HUD_EDGE_MARGIN).max(HUD_EDGE_MARGIN),
    );

    ScreenRect {
        x,
        y,
        w: width,
        h: height,
    }
}

fn event_log_toggle_rect(
    window_width: f32,
    window_height: f32,
    _device_profile: DeviceProfile,
) -> ScreenRect {
    clamp_rect_to_window(
        ScreenRect {
            x: HUD_EDGE_MARGIN,
            y: HUD_EDGE_MARGIN,
            w: EVENT_LOG_TOGGLE_W,
            h: EVENT_LOG_TOGGLE_H,
        },
        window_width,
        window_height,
    )
}

fn event_log_panel_rect(
    window_width: f32,
    window_height: f32,
    device_profile: DeviceProfile,
) -> ScreenRect {
    let toggle = event_log_toggle_rect(window_width, window_height, device_profile);
    clamp_rect_to_window(
        ScreenRect {
            x: toggle.x,
            y: toggle.y + toggle.h + EVENT_LOG_GAP,
            w: EVENT_LOG_PANEL_W,
            h: EVENT_LOG_PANEL_H,
        },
        window_width,
        window_height,
    )
}

fn apply_rect_to_node(node: &mut Node, rect: ScreenRect) {
    node.left = Val::Px(rect.x);
    node.top = Val::Px(rect.y);
    node.width = Val::Px(rect.w);
    node.height = Val::Px(rect.h);
}

fn skill_action_name(action: SkillUiAction, language: Language) -> &'static str {
    i18n_skill_name(language, action)
}

fn skill_icon_asset_path(action: SkillUiAction) -> &'static str {
    match action {
        SkillUiAction::Dash => "ui/skills/dash.png",
        SkillUiAction::Snipe => "ui/skills/snipe.png",
        SkillUiAction::Swap => "ui/skills/swap.png",
        SkillUiAction::Shield => "ui/skills/shield.png",
        SkillUiAction::DoubleDice => "ui/skills/double_dice.png",
    }
}

fn skill_tip_body(action: SkillUiAction, language: Language) -> &'static str {
    i18n_skill_tip_body(language, action)
}

fn skill_badge_text(charges: u8) -> String {
    if charges > 99 {
        "99+".to_string()
    } else {
        charges.to_string()
    }
}

fn skill_icon_color(ready: bool) -> Color {
    if ready {
        Color::WHITE
    } else {
        Color::srgba(0.64, 0.67, 0.72, 0.74)
    }
}

fn skill_badge_color(ready: bool) -> Color {
    if ready {
        Color::srgba(0.96, 0.98, 1.0, 0.96)
    } else {
        Color::srgba(0.78, 0.81, 0.86, 0.86)
    }
}

fn skill_badge_border_color(ready: bool) -> Color {
    if ready {
        Color::srgba(0.06, 0.10, 0.16, 0.76)
    } else {
        Color::srgba(0.18, 0.22, 0.30, 0.44)
    }
}

fn skill_badge_text_color(ready: bool) -> Color {
    if ready {
        Color::srgb(0.05, 0.08, 0.12)
    } else {
        Color::srgba(0.12, 0.16, 0.23, 0.76)
    }
}

fn skill_block_marker_visible(
    current_skills: Option<&PlayerSkillState>,
    match_finished: bool,
) -> bool {
    !match_finished
        && current_skills
            .map(|skills| skills.skill_blocked_this_turn)
            .unwrap_or(false)
}

fn skill_block_marker_color() -> Color {
    Color::srgba(0.76, 0.08, 0.10, 0.94)
}

fn skill_block_marker_border_color() -> Color {
    Color::srgba(0.98, 0.72, 0.72, 0.92)
}

fn finish_bounce_charge_bar_track_color() -> Color {
    Color::srgba(0.08, 0.12, 0.18, 0.30)
}

fn finish_bounce_charge_bar_border_color() -> Color {
    Color::srgba(0.96, 0.98, 1.0, 0.42)
}

fn finish_bounce_charge_bar_fill_color(player_color: Color) -> Color {
    player_color.mix(&Color::WHITE, 0.24).with_alpha(0.82)
}

fn finish_bounce_charge_bar_text_color(active: bool) -> Color {
    if active {
        Color::srgba(0.98, 0.99, 1.0, 0.96)
    } else {
        Color::srgba(0.76, 0.80, 0.88, 0.70)
    }
}

fn finish_bounce_charge_fill_width(count: u8, bar_width: f32) -> f32 {
    let threshold = FINISH_BOUNCES_PER_SKILL_REWARD.max(1);
    let progress = count.min(threshold) as f32 / threshold as f32;
    (bar_width * progress).clamp(0.0, bar_width)
}

fn finish_bounce_charge_bar_text(count: u8, active: bool, language: Language) -> String {
    if active {
        let label = match language {
            Language::SimplifiedChinese => "折返充能",
            Language::English => "Bounce Charge",
        };
        format!(
            "{label} {}/{}",
            count.min(FINISH_BOUNCES_PER_SKILL_REWARD),
            FINISH_BOUNCES_PER_SKILL_REWARD
        )
    } else {
        String::new()
    }
}

fn skill_charge(action: SkillUiAction, skills: &PlayerSkillState) -> u8 {
    match action {
        SkillUiAction::Dash => skills.dash_charges,
        SkillUiAction::Snipe => skills.snipe_charges,
        SkillUiAction::Swap => skills.swap_charges,
        SkillUiAction::Shield => skills.shield_charges,
        SkillUiAction::DoubleDice => skills.double_dice_charges,
    }
}

fn player_hud_badge_width(kind: PlayerHudBadgeKind) -> f32 {
    match kind {
        PlayerHudBadgeKind::Player => HUD_BADGE_PLAYER_W,
        PlayerHudBadgeKind::Team => HUD_BADGE_TEAM_W,
        PlayerHudBadgeKind::Turn => HUD_BADGE_TURN_W,
    }
}

fn player_hud_badges_for_seat(seat: PlayerSeat) -> [PlayerHudBadgeKind; 3] {
    match seat {
        PlayerSeat::Blue | PlayerSeat::Green => PLAYER_HUD_BADGES,
        PlayerSeat::Red | PlayerSeat::Yellow => [
            PlayerHudBadgeKind::Turn,
            PlayerHudBadgeKind::Team,
            PlayerHudBadgeKind::Player,
        ],
    }
}

fn player_hud_badge_justify_content(seat: PlayerSeat) -> JustifyContent {
    if player_hud_badges_align_to_left(seat) {
        JustifyContent::FlexStart
    } else {
        JustifyContent::FlexEnd
    }
}

fn player_hud_badges_align_to_left(seat: PlayerSeat) -> bool {
    matches!(seat, PlayerSeat::Blue | PlayerSeat::Green)
}

fn player_hud_badges_total_width() -> f32 {
    PLAYER_HUD_BADGES
        .iter()
        .map(|kind| player_hud_badge_width(*kind))
        .sum::<f32>()
        + HUD_BADGE_GAP * PLAYER_HUD_BADGES.len().saturating_sub(1) as f32
}

fn player_hud_badge_name(kind: PlayerHudBadgeKind) -> &'static str {
    match kind {
        PlayerHudBadgeKind::Player => "Player",
        PlayerHudBadgeKind::Team => "Team",
        PlayerHudBadgeKind::Turn => "Turn",
    }
}

fn player_hud_badge_text(
    kind: PlayerHudBadgeKind,
    player: &PlayerProfile,
    is_current: bool,
    player_finished: bool,
    language: Language,
    elapsed_secs: f32,
) -> String {
    match kind {
        PlayerHudBadgeKind::Player => {
            let label = match language {
                Language::SimplifiedChinese => player.state.player_id.to_string(),
                Language::English => format!("P{}", player.state.player_id),
            };
            if player_finished {
                format!("{label}✓")
            } else {
                label
            }
        }
        PlayerHudBadgeKind::Team => match language {
            Language::SimplifiedChinese => format!("队{}", player.state.team_id),
            Language::English => format!("T{}", player.state.team_id),
        },
        PlayerHudBadgeKind::Turn => player_hud_turn_indicator_text(is_current, elapsed_secs),
    }
}

fn player_hud_badge_visibility(kind: PlayerHudBadgeKind, is_current: bool) -> Visibility {
    if kind == PlayerHudBadgeKind::Turn && !is_current {
        Visibility::Hidden
    } else {
        Visibility::Visible
    }
}

fn player_hud_turn_indicator_text(is_current: bool, elapsed_secs: f32) -> String {
    if !is_current {
        return String::new();
    }
    match ((elapsed_secs / HUD_TURN_INDICATOR_STEP_SECS).floor() as usize) % 3 {
        1 => ">>".to_string(),
        _ => ">".to_string(),
    }
}

fn player_hud_turn_indicator_pulse(elapsed_secs: f32) -> f32 {
    ((elapsed_secs * std::f32::consts::TAU).sin() * 0.5 + 0.5).clamp(0.0, 1.0)
}

fn player_hud_badge_color(
    kind: PlayerHudBadgeKind,
    player_color: Color,
    is_current: bool,
    turn_indicator_pulse: f32,
) -> Color {
    match kind {
        PlayerHudBadgeKind::Player => player_color
            .mix(&Color::WHITE, if is_current { 0.16 } else { 0.62 })
            .with_alpha(if is_current { 1.0 } else { 0.94 }),
        PlayerHudBadgeKind::Team if is_current => Color::srgba(0.98, 0.99, 1.0, 0.98),
        PlayerHudBadgeKind::Team => Color::srgba(0.90, 0.94, 0.98, 0.82),
        PlayerHudBadgeKind::Turn if is_current => {
            Color::srgba(0.10, 0.16, 0.24, 0.82 + 0.16 * turn_indicator_pulse)
        }
        PlayerHudBadgeKind::Turn => Color::srgba(0.78, 0.82, 0.88, 0.0),
    }
}

fn player_hud_badge_border_color(
    kind: PlayerHudBadgeKind,
    is_current: bool,
    turn_indicator_pulse: f32,
) -> Color {
    match (kind, is_current) {
        (PlayerHudBadgeKind::Turn, true) => {
            Color::srgba(0.02, 0.05, 0.08, 0.78 + 0.20 * turn_indicator_pulse)
        }
        (PlayerHudBadgeKind::Turn, false) => Color::srgba(0.10, 0.16, 0.24, 0.0),
        (_, true) => Color::srgba(0.04, 0.09, 0.16, 0.86),
        _ => Color::srgba(0.10, 0.16, 0.24, 0.26),
    }
}

fn player_hud_badge_text_color(
    kind: PlayerHudBadgeKind,
    is_current: bool,
    turn_indicator_pulse: f32,
) -> Color {
    match (kind, is_current) {
        (PlayerHudBadgeKind::Turn, true) => {
            Color::srgba(1.0, 1.0, 1.0, 0.82 + 0.18 * turn_indicator_pulse)
        }
        (PlayerHudBadgeKind::Turn, false) => Color::srgba(0.10, 0.16, 0.24, 0.0),
        _ => Color::srgb(0.07, 0.11, 0.17),
    }
}

fn roll_button_text(cancel_target_ready: bool, language: Language) -> String {
    if cancel_target_ready {
        return i18n_text(language, TextKey::SkillTipClose).to_string();
    }
    String::new()
}

fn event_notice_text_from_action(action: &str, language: Language) -> Option<String> {
    let mut notice = None;
    for segment in action.split(';').flat_map(|part| part.split(", ")) {
        let segment = segment.trim();
        if let Some(finish_bounce_notice) = format_finish_bounce_notice(segment, language) {
            notice = Some(finish_bounce_notice);
        } else if let Some(event_note) = extract_event_note(segment) {
            notice = Some(format_event_notice(event_note, language));
        }
    }
    notice
}

fn format_finish_bounce_notice(segment: &str, language: Language) -> Option<String> {
    let detail = segment.strip_prefix("finish bounce ")?;
    if let Some((progress, skill_detail)) = detail.split_once(": gained 1 ")
        && let Some(skill) = skill_detail.strip_suffix(" charge")
    {
        return Some(match language {
            Language::SimplifiedChinese => {
                format!(
                    "终点折返 {progress}\n{} +1",
                    localized_skill_token(skill, language)
                )
            }
            Language::English => {
                format!(
                    "Finish bounce {progress}\n{} +1",
                    localized_skill_token(skill, language)
                )
            }
        });
    }

    Some(match language {
        Language::SimplifiedChinese => format!(
            "终点折返 {detail}\n累计到 {FINISH_BOUNCES_PER_SKILL_REWARD}/{} 奖励技能。",
            FINISH_BOUNCES_PER_SKILL_REWARD
        ),
        Language::English => format!(
            "Finish bounce {detail}\nReach {FINISH_BOUNCES_PER_SKILL_REWARD}/{} to gain a random skill.",
            FINISH_BOUNCES_PER_SKILL_REWARD
        ),
    })
}

fn extract_event_note(segment: &str) -> Option<&str> {
    if let Some(note) = segment.strip_prefix("pre-jump event tile ") {
        let (_, note) = note.split_once(": ")?;
        return note.starts_with("event ").then_some(note);
    }

    if segment.starts_with("event ") {
        return Some(segment);
    }

    segment
        .find(": event ")
        .map(|index| &segment[index + 2..])
        .filter(|note| note.starts_with("event "))
}

fn format_event_notice(event_note: &str, language: Language) -> String {
    match language {
        Language::SimplifiedChinese => format_event_notice_zh(event_note),
        Language::English => format_event_notice_en(event_note),
    }
}

fn format_event_notice_zh(event_note: &str) -> String {
    if event_note == "event advance +2" {
        return "事件：前进 +2\n额外前进 2 格。".to_string();
    }

    if let Some(detail) = event_note.strip_prefix("event GainShield: gained shield ") {
        let shield = detail.trim().trim_start_matches('(').trim_end_matches(')');
        return format!("事件：护盾 +1\n当前护盾：{shield}");
    }

    if let Some(skill) = event_note
        .strip_prefix("event GainSkillCharge: gained 1 ")
        .and_then(|detail| detail.strip_suffix(" charge"))
    {
        return format!(
            "事件：技能充能\n{} +1",
            localized_skill_token(skill, Language::SimplifiedChinese)
        );
    }

    if let Some(player) =
        event_note.strip_prefix("event DisableNextSkill: next skill turn disabled for ")
    {
        return format!(
            "事件：技能干扰\n{} 下回合不能使用技能。",
            localized_player_token(player, Language::SimplifiedChinese)
        );
    }

    if let Some(piece_id) =
        event_note.strip_prefix("event RemoveEnemyShield: removed shield from piece #")
    {
        return format!("事件：护盾破坏\n飞机 {piece_id} 失去 1 层护盾。");
    }

    match event_note {
        "event fizzled: could not disable next skill turn" => {
            "事件未生效\n没有可被干扰的技能回合。".to_string()
        }
        "event fizzled: no enemy shield to remove" => {
            "事件未生效\n没有敌方护盾可移除。".to_string()
        }
        "event failed: selected enemy shield target disappeared" => {
            "事件失败\n目标已经消失。".to_string()
        }
        _ => format!(
            "事件\n{}",
            event_note.strip_prefix("event ").unwrap_or(event_note)
        ),
    }
}

fn format_event_notice_en(event_note: &str) -> String {
    if event_note == "event advance +2" {
        return "Event: Advance +2\nMove forward 2 extra tiles.".to_string();
    }

    if let Some(detail) = event_note.strip_prefix("event GainShield: gained shield ") {
        let shield = detail.trim().trim_start_matches('(').trim_end_matches(')');
        return format!("Event: Shield +1\nCurrent shield: {shield}");
    }

    if let Some(skill) = event_note
        .strip_prefix("event GainSkillCharge: gained 1 ")
        .and_then(|detail| detail.strip_suffix(" charge"))
    {
        return format!(
            "Event: Skill Charge\n{} +1",
            localized_skill_token(skill, Language::English)
        );
    }

    if let Some(player) =
        event_note.strip_prefix("event DisableNextSkill: next skill turn disabled for ")
    {
        return format!(
            "Event: Skill Jam\n{} cannot use skills next turn.",
            localized_player_token(player, Language::English)
        );
    }

    if let Some(piece_id) =
        event_note.strip_prefix("event RemoveEnemyShield: removed shield from piece #")
    {
        return format!("Event: Shield Break\nPiece {piece_id} loses 1 shield.");
    }

    match event_note {
        "event fizzled: could not disable next skill turn" => {
            "Event Fizzled\nNo skill turn could be jammed.".to_string()
        }
        "event fizzled: no enemy shield to remove" => {
            "Event Fizzled\nNo enemy shield could be removed.".to_string()
        }
        "event failed: selected enemy shield target disappeared" => {
            "Event Failed\nThe target disappeared.".to_string()
        }
        _ => format!(
            "Event\n{}",
            event_note.strip_prefix("event ").unwrap_or(event_note)
        ),
    }
}

fn sync_event_log(
    event_log: &mut EventLogState,
    turn_state: &TurnState,
    skill_roster: &SkillRoster,
    language: Language,
) {
    if let Some(action) = turn_state.last_action.as_ref() {
        let key = turn_state.last_action_serial;
        if event_log.last_turn_action_key.as_ref() != Some(&key) {
            push_event_log_entry(
                event_log,
                format_event_log_entry(
                    turn_state.last_action_turn_index,
                    turn_state.last_action_player_id,
                    action,
                    language,
                ),
            );
            event_log.last_turn_action_key = Some(key);
        }
    }
    if let Some(action) = skill_roster.last_skill_action.as_ref() {
        let key = skill_roster.last_skill_action_serial;
        if event_log.last_skill_action_key.as_ref() != Some(&key) {
            push_event_log_entry(
                event_log,
                format_event_log_entry(
                    skill_roster.last_skill_action_turn_index,
                    skill_roster.last_skill_action_player_id,
                    action,
                    language,
                ),
            );
            event_log.last_skill_action_key = Some(key);
        }
    }
}

fn format_event_log_entry(
    turn_index: u32,
    player_id: Option<u8>,
    action: &str,
    language: Language,
) -> String {
    let localized_action = player_id
        .map(|player_id| strip_player_prefix(action, player_id))
        .unwrap_or(action);
    let localized_action = localize_action_text(localized_action, language);
    let Some(player_id) = player_id else {
        return match language {
            Language::SimplifiedChinese => format!("第{}回合：{}", turn_index, localized_action),
            Language::English => format!("Turn {turn_index}: {localized_action}"),
        };
    };
    match language {
        Language::SimplifiedChinese => {
            format!(
                "第{}回合 玩家{}：{}",
                turn_index, player_id, localized_action
            )
        }
        Language::English => format!("Turn {turn_index} P{player_id}: {localized_action}"),
    }
}

fn strip_player_prefix(action: &str, player_id: u8) -> &str {
    let prefix = format!("P{player_id} ");
    action.strip_prefix(&prefix).unwrap_or(action)
}

fn push_event_log_entry(event_log: &mut EventLogState, entry: String) {
    event_log.entries.push(entry);
    prune_event_log_entries(event_log);
}

fn prune_event_log_entries(event_log: &mut EventLogState) {
    let overflow = event_log
        .entries
        .len()
        .saturating_sub(EVENT_LOG_MAX_ENTRIES);
    if overflow > 0 {
        event_log.entries.drain(0..overflow);
    }
}

fn format_event_log_entries(entries: &[String], language: Language) -> String {
    if entries.is_empty() {
        match language {
            Language::SimplifiedChinese => "暂无事件",
            Language::English => "No events",
        }
        .to_string()
    } else {
        entries.join("\n")
    }
}

fn localize_action_text(action: &str, language: Language) -> String {
    action
        .split(';')
        .map(|part| {
            let part = part.trim();
            let localized_part = localize_action_segment(part, language);
            if localized_part != part {
                return localized_part;
            }
            part.split(", ")
                .map(|segment| localize_action_segment(segment.trim(), language))
                .collect::<Vec<_>>()
                .join(match language {
                    Language::SimplifiedChinese => "，",
                    Language::English => ", ",
                })
        })
        .collect::<Vec<_>>()
        .join(match language {
            Language::SimplifiedChinese => "；",
            Language::English => "; ",
        })
}

fn localize_action_segment(segment: &str, language: Language) -> String {
    if language == Language::English {
        return localize_action_segment_en(segment);
    }
    if let Some((roll, piece_id)) = segment
        .strip_prefix("rolled ")
        .and_then(|detail| detail.split_once(", launched piece #"))
    {
        return format!("掷出 {roll}，飞机 {piece_id} 起飞");
    }
    if let Some(roll) = segment.strip_prefix("rolled ") {
        if roll.chars().all(|character| character.is_ascii_digit()) {
            return format!("掷出 {roll}");
        }
    }
    if let Some((roll, detail)) = segment
        .strip_prefix("rolled ")
        .and_then(|detail| detail.split_once(", moved piece #"))
        && let Some((piece_id, target)) = detail.split_once(" to tile ")
    {
        return format!("掷出 {roll}，飞机 {piece_id} 移动到第 {target} 格");
    }
    if let Some((roll, _)) = segment
        .strip_prefix("rolled ")
        .and_then(|detail| detail.split_once(" but had no legal action"))
    {
        return format!("掷出 {roll}，没有可执行动作");
    }
    if let Some(piece_id) = segment
        .strip_prefix("sent piece #")
        .and_then(|detail| detail.strip_suffix(" back to hangar"))
    {
        return format!("飞机 {piece_id} 返回停机坪");
    }
    if let Some(shield) = segment
        .strip_prefix("gained shield ")
        .map(|detail| detail.trim().trim_start_matches('(').trim_end_matches(')'))
    {
        return format!("获得护盾（{shield}）");
    }
    if let Some(progress) = segment.strip_prefix("finish bounce ") {
        if let Some((progress, skill_detail)) = progress.split_once(": gained 1 ")
            && let Some(skill) = skill_detail.strip_suffix(" charge")
        {
            return format!(
                "终点折返 {progress}，{} +1",
                localized_skill_token(skill, language)
            );
        }
        return format!("终点折返 {progress}");
    }
    if let Some(event_note) = extract_event_note(segment) {
        return format_event_notice_single_line(event_note, language);
    }
    if let Some(piece_id) = segment
        .strip_prefix("Snipe hit piece #")
        .and_then(|detail| detail.strip_suffix(" and removed a shield"))
    {
        return format!("狙击命中飞机 {piece_id}，移除 1 层护盾");
    }
    if let Some(piece_id) = segment
        .strip_prefix("Snipe hit piece #")
        .and_then(|detail| detail.strip_suffix(" and broke the shared shield"))
    {
        return format!("狙击命中飞机 {piece_id}，击破共享护盾");
    }
    if let Some(piece_id) = segment
        .strip_prefix("Snipe sent piece #")
        .and_then(|detail| detail.strip_suffix(" back to hangar"))
    {
        return format!("狙击将飞机 {piece_id} 送回停机坪");
    }
    if let Some(piece_id) = segment
        .strip_prefix("AI Snipe hit piece #")
        .and_then(|detail| detail.strip_suffix(" and removed a shield"))
    {
        return format!("电脑狙击命中飞机 {piece_id}，移除 1 层护盾");
    }
    if let Some(piece_id) = segment
        .strip_prefix("AI Snipe hit piece #")
        .and_then(|detail| detail.strip_suffix(" and broke the shared shield"))
    {
        return format!("电脑狙击命中飞机 {piece_id}，击破共享护盾");
    }
    if let Some(piece_id) = segment
        .strip_prefix("AI Snipe sent piece #")
        .and_then(|detail| detail.strip_suffix(" back to hangar"))
    {
        return format!("电脑狙击将飞机 {piece_id} 送回停机坪");
    }
    if let Some((current_piece, teammate_piece)) = segment
        .strip_prefix("Swap exchanged piece #")
        .and_then(|detail| detail.split_once(" with teammate piece #"))
    {
        return format!("换位：飞机 {current_piece} 与队友飞机 {teammate_piece} 交换");
    }
    if let Some((current_piece, target_piece)) = segment
        .strip_prefix("Swap exchanged piece #")
        .and_then(|detail| detail.split_once(" with piece #"))
    {
        return format!("换位：飞机 {current_piece} 与飞机 {target_piece} 交换");
    }
    if let Some((current_piece, teammate_piece)) = segment
        .strip_prefix("AI Swap exchanged piece #")
        .and_then(|detail| detail.split_once(" with teammate piece #"))
    {
        return format!("电脑换位：飞机 {current_piece} 与队友飞机 {teammate_piece} 交换");
    }
    if let Some((current_piece, target_piece)) = segment
        .strip_prefix("AI Swap exchanged piece #")
        .and_then(|detail| detail.split_once(" with piece #"))
    {
        return format!("电脑换位：飞机 {current_piece} 与飞机 {target_piece} 交换");
    }
    if let Some((piece_id, shield)) = segment
        .strip_prefix("used Shield on piece #")
        .and_then(|detail| detail.split_once(" ("))
    {
        return format!(
            "护盾：飞机 {piece_id} 获得护盾（{}）",
            shield.trim_end_matches(')')
        );
    }
    if let Some(detail) = segment.strip_prefix("resolved DoubleDice: rolled ")
        && let Some((faces, chosen)) = detail.split_once(" and chose ")
    {
        return format!("双骰掷出 {faces}，选择 {chosen}");
    }
    if let Some(skill) = segment
        .strip_prefix("(AI) armed ")
        .and_then(|detail| detail.strip_suffix(" for launch pressure"))
    {
        return format!(
            "电脑启用{}，帮助起飞",
            localized_skill_token(skill, language)
        );
    }
    if let Some(skill) = segment
        .strip_prefix("(AI) armed ")
        .and_then(|detail| detail.strip_suffix(" after roll for +3 movement"))
    {
        return format!(
            "电脑启用{}，本回合移动 +3",
            localized_skill_token(skill, language)
        );
    }
    if let Some(skill) = segment
        .strip_prefix("armed ")
        .and_then(|detail| detail.strip_suffix(" for the next roll"))
    {
        return format!(
            "已启用{}，用于下一次掷骰",
            localized_skill_token(skill, language)
        );
    }
    if let Some(skill) = segment
        .strip_prefix("already has ")
        .and_then(|detail| detail.strip_suffix(" armed"))
    {
        return format!("{}已经启用", localized_skill_token(skill, language));
    }
    if let Some(skill) = segment
        .strip_prefix("has no ")
        .and_then(|detail| detail.strip_suffix(" charges left"))
    {
        return format!("{}充能不足", localized_skill_token(skill, language));
    }
    if let Some(skill) = segment
        .strip_prefix("armed ")
        .and_then(|detail| detail.strip_suffix(" for +3 movement"))
    {
        return format!(
            "已启用{}，本回合移动 +3",
            localized_skill_token(skill, language)
        );
    }
    if let Some(skill) = segment.strip_prefix("used ") {
        return format!("使用{}", localized_skill_token(skill, language));
    }
    match segment {
        "needs a movable piece to use Dash" => "需要有可移动飞机才能使用冲刺".to_string(),
        "cannot use skills this turn (event lock)" => "本回合被干扰，不能使用技能".to_string(),
        "already used a skill this turn" => "本回合已经使用过技能".to_string(),
        "could not find a piece for Shield" => "没有可加护盾的飞机".to_string(),
        "found no Snipe target" => "没有可狙击目标".to_string(),
        "Swap is only available in 2v2" => "换位只在 2v2 模式可用".to_string(),
        "found no teammate piece to Swap with" => "没有可换位的队友飞机".to_string(),
        "found no target piece to Swap with" => "没有可换位目标".to_string(),
        "needs a main route piece to use Swap" => "需要己方飞机在主航道上才能换位".to_string(),
        "Skill not available in current phase" => "当前阶段不能使用技能".to_string(),
        "Snipe selection cancelled" => "已取消狙击选择".to_string(),
        "completed all pieces" => "所有飞机已到达终点".to_string(),
        "skipped turn" => "跳过回合".to_string(),
        "Snipe failed to resolve" => "狙击结算失败".to_string(),
        "AI Snipe failed to resolve" => "电脑狙击结算失败".to_string(),
        "Swap failed: current player's active piece not found" => {
            "换位失败：未找到当前玩家的主航道飞机".to_string()
        }
        "Swap failed: teammate piece not found on main route" => {
            "换位失败：未找到主航道上的队友飞机".to_string()
        }
        "Swap failed: target piece not found on main route" => {
            "换位失败：未找到主航道上的目标飞机".to_string()
        }
        "AI Swap failed: current active piece not found" => {
            "电脑换位失败：未找到当前主航道飞机".to_string()
        }
        "AI Swap failed: teammate piece not found on main route" => {
            "电脑换位失败：未找到主航道上的队友飞机".to_string()
        }
        "AI Swap failed: target piece not found on main route" => {
            "电脑换位失败：未找到主航道上的目标飞机".to_string()
        }
        _ => segment.to_string(),
    }
}

fn localize_action_segment_en(segment: &str) -> String {
    if let Some((roll, piece_id)) = segment
        .strip_prefix("rolled ")
        .and_then(|detail| detail.split_once(", launched piece #"))
    {
        return format!("rolled {roll}, launched piece #{piece_id}");
    }
    if let Some(roll) = segment.strip_prefix("rolled ") {
        if roll.chars().all(|character| character.is_ascii_digit()) {
            return format!("rolled {roll}");
        }
    }
    if let Some((roll, detail)) = segment
        .strip_prefix("rolled ")
        .and_then(|detail| detail.split_once(", moved piece #"))
        && let Some((piece_id, target)) = detail.split_once(" to tile ")
    {
        return format!("rolled {roll}, moved piece #{piece_id} to tile {target}");
    }
    if let Some((roll, _)) = segment
        .strip_prefix("rolled ")
        .and_then(|detail| detail.split_once(" but had no legal action"))
    {
        return format!("rolled {roll}, no legal action");
    }
    if let Some(piece_id) = segment
        .strip_prefix("sent piece #")
        .and_then(|detail| detail.strip_suffix(" back to hangar"))
    {
        return format!("piece #{piece_id} returned to hangar");
    }
    if let Some(shield) = segment
        .strip_prefix("gained shield ")
        .map(|detail| detail.trim().trim_start_matches('(').trim_end_matches(')'))
    {
        return format!("gained shield ({shield})");
    }
    if let Some(progress) = segment.strip_prefix("finish bounce ") {
        if let Some((progress, skill_detail)) = progress.split_once(": gained 1 ")
            && let Some(skill) = skill_detail.strip_suffix(" charge")
        {
            return format!(
                "finish bounce {progress}, {} +1",
                localized_skill_token(skill, Language::English)
            );
        }
        return format!("finish bounce {progress}");
    }
    if let Some(event_note) = extract_event_note(segment) {
        return format_event_notice_single_line(event_note, Language::English);
    }
    if let Some(piece_id) = segment
        .strip_prefix("Snipe hit piece #")
        .and_then(|detail| detail.strip_suffix(" and removed a shield"))
    {
        return format!("Snipe hit piece #{piece_id}, removed 1 shield");
    }
    if let Some(piece_id) = segment
        .strip_prefix("Snipe hit piece #")
        .and_then(|detail| detail.strip_suffix(" and broke the shared shield"))
    {
        return format!("Snipe hit piece #{piece_id}, broke shared shield");
    }
    if let Some(piece_id) = segment
        .strip_prefix("Snipe sent piece #")
        .and_then(|detail| detail.strip_suffix(" back to hangar"))
    {
        return format!("Snipe sent piece #{piece_id} back to hangar");
    }
    if let Some(piece_id) = segment
        .strip_prefix("AI Snipe hit piece #")
        .and_then(|detail| detail.strip_suffix(" and removed a shield"))
    {
        return format!("AI Snipe hit piece #{piece_id}, removed 1 shield");
    }
    if let Some(piece_id) = segment
        .strip_prefix("AI Snipe hit piece #")
        .and_then(|detail| detail.strip_suffix(" and broke the shared shield"))
    {
        return format!("AI Snipe hit piece #{piece_id}, broke shared shield");
    }
    if let Some(piece_id) = segment
        .strip_prefix("AI Snipe sent piece #")
        .and_then(|detail| detail.strip_suffix(" back to hangar"))
    {
        return format!("AI Snipe sent piece #{piece_id} back to hangar");
    }
    if let Some((current_piece, teammate_piece)) = segment
        .strip_prefix("Swap exchanged piece #")
        .and_then(|detail| detail.split_once(" with teammate piece #"))
    {
        return format!(
            "Swap exchanged piece #{current_piece} with teammate piece #{teammate_piece}"
        );
    }
    if let Some((current_piece, target_piece)) = segment
        .strip_prefix("Swap exchanged piece #")
        .and_then(|detail| detail.split_once(" with piece #"))
    {
        return format!("Swap exchanged piece #{current_piece} with piece #{target_piece}");
    }
    if let Some((current_piece, teammate_piece)) = segment
        .strip_prefix("AI Swap exchanged piece #")
        .and_then(|detail| detail.split_once(" with teammate piece #"))
    {
        return format!(
            "AI Swap exchanged piece #{current_piece} with teammate piece #{teammate_piece}"
        );
    }
    if let Some((current_piece, target_piece)) = segment
        .strip_prefix("AI Swap exchanged piece #")
        .and_then(|detail| detail.split_once(" with piece #"))
    {
        return format!("AI Swap exchanged piece #{current_piece} with piece #{target_piece}");
    }
    if let Some((piece_id, shield)) = segment
        .strip_prefix("used Shield on piece #")
        .and_then(|detail| detail.split_once(" ("))
    {
        return format!(
            "Shield: piece #{piece_id} gained shield ({})",
            shield.trim_end_matches(')')
        );
    }
    if let Some(detail) = segment.strip_prefix("resolved DoubleDice: rolled ")
        && let Some((faces, chosen)) = detail.split_once(" and chose ")
    {
        return format!("DoubleDice rolled {faces}, chose {chosen}");
    }
    if let Some(skill) = segment
        .strip_prefix("(AI) armed ")
        .and_then(|detail| detail.strip_suffix(" for launch pressure"))
    {
        return format!(
            "AI armed {} for launch pressure",
            localized_skill_token(skill, Language::English)
        );
    }
    if let Some(skill) = segment
        .strip_prefix("(AI) armed ")
        .and_then(|detail| detail.strip_suffix(" after roll for +3 movement"))
    {
        return format!(
            "AI armed {} for +3 movement",
            localized_skill_token(skill, Language::English)
        );
    }
    if let Some(skill) = segment
        .strip_prefix("armed ")
        .and_then(|detail| detail.strip_suffix(" for the next roll"))
    {
        return format!(
            "armed {} for the next roll",
            localized_skill_token(skill, Language::English)
        );
    }
    if let Some(skill) = segment
        .strip_prefix("already has ")
        .and_then(|detail| detail.strip_suffix(" armed"))
    {
        return format!(
            "{} already armed",
            localized_skill_token(skill, Language::English)
        );
    }
    if let Some(skill) = segment
        .strip_prefix("has no ")
        .and_then(|detail| detail.strip_suffix(" charges left"))
    {
        return format!(
            "{} has no charges left",
            localized_skill_token(skill, Language::English)
        );
    }
    if let Some(skill) = segment
        .strip_prefix("armed ")
        .and_then(|detail| detail.strip_suffix(" for +3 movement"))
    {
        return format!(
            "armed {} for +3 movement",
            localized_skill_token(skill, Language::English)
        );
    }
    if let Some(skill) = segment.strip_prefix("used ") {
        return format!("used {}", localized_skill_token(skill, Language::English));
    }
    match segment {
        "completed all pieces" => "all pieces reached the goal".to_string(),
        "skipped turn" => "skipped turn".to_string(),
        _ => segment.to_string(),
    }
}

fn format_event_notice_single_line(event_note: &str, language: Language) -> String {
    format_event_notice(event_note, language).replace(
        '\n',
        match language {
            Language::SimplifiedChinese => "，",
            Language::English => ", ",
        },
    )
}

fn localized_skill_token(skill: &str, language: Language) -> &'static str {
    i18n_skill_token(language, skill)
}

fn localized_player_token(player: &str, language: Language) -> String {
    player
        .strip_prefix('P')
        .map(|id| match language {
            Language::SimplifiedChinese => format!("玩家{id}"),
            Language::English => format!("P{id}"),
        })
        .unwrap_or_else(|| player.to_string())
}

fn event_log_scroll_max_y(computed: &ComputedNode) -> f32 {
    let max_offset = (computed.content_size() - computed.size()) * computed.inverse_scale_factor();
    max_offset.y.max(0.0)
}

fn apply_event_log_scrollbar_thumb(
    thumb_node: &mut Node,
    scroll_position: &ScrollPosition,
    computed: &ComputedNode,
) {
    let visible_height = (computed.size().y * computed.inverse_scale_factor()).max(1.0);
    let content_height = (computed.content_size().y * computed.inverse_scale_factor()).max(1.0);
    let max_scroll_y = event_log_scroll_max_y(computed);
    let thumb_height = if max_scroll_y <= 0.0 {
        visible_height
    } else {
        (visible_height * (visible_height / content_height))
            .clamp(EVENT_LOG_SCROLLBAR_MIN_THUMB_H, visible_height)
    };
    let travel = (visible_height - thumb_height).max(0.0);
    let scroll_ratio = if max_scroll_y > 0.0 {
        (scroll_position.y / max_scroll_y).clamp(0.0, 1.0)
    } else {
        0.0
    };

    thumb_node.top = Val::Px(travel * scroll_ratio);
    thumb_node.height = Val::Px(thumb_height);
}

fn is_skill_button_ready(
    action: SkillUiAction,
    skills: &PlayerSkillState,
    can_use_skill: bool,
    phase: &GamePhase,
    _mode: GameMode,
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
                && board_availability.dash_move_target
        }
        SkillUiAction::Snipe => {
            matches!(phase, GamePhase::AwaitDice)
                && skills.snipe_charges > 0
                && board_availability.snipe_target
        }
        SkillUiAction::Swap => {
            matches!(phase, GamePhase::AwaitDice)
                && skills.swap_charges > 0
                && board_availability.active_self
                && board_availability.swap_target
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

fn skill_button_color(ready: bool, available_context: bool) -> Color {
    if ready {
        Color::srgba(0.45, 0.70, 0.92, 0.52)
    } else if available_context {
        Color::srgba(0.78, 0.84, 0.91, 0.46)
    } else {
        Color::srgba(0.73, 0.77, 0.84, 0.34)
    }
}

fn skill_tip_panel_color() -> Color {
    Color::srgba(0.97, 0.98, 1.0, 0.96)
}

fn skill_tip_border_color() -> Color {
    Color::srgba(0.08, 0.12, 0.18, 0.52)
}

fn event_notice_panel_color(active: bool) -> Color {
    if active {
        Color::srgba(0.96, 0.98, 1.0, 0.90)
    } else {
        Color::srgba(0.96, 0.98, 1.0, 0.0)
    }
}

fn event_notice_border_color(active: bool) -> Color {
    if active {
        Color::srgba(0.12, 0.18, 0.28, 0.44)
    } else {
        Color::srgba(0.12, 0.18, 0.28, 0.0)
    }
}

fn event_notice_text_color(active: bool) -> Color {
    if active {
        Color::srgb(0.08, 0.12, 0.18)
    } else {
        Color::srgba(0.08, 0.12, 0.18, 0.0)
    }
}

fn cleanup_hud(mut commands: Commands, query: Query<Entity, (With<HudEntity>, Without<ChildOf>)>) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
}

fn spawn_result_screen(
    mut commands: Commands,
    match_result: Res<MatchResult>,
    language_settings: Res<LanguageSettings>,
) {
    let language = language_settings.language;
    let winner = match_result.winner_team_id.unwrap_or_default();
    let winner_players = format_winner_players(&match_result, language);
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            ZIndex(60),
            Name::new("ResultRoot"),
            ResultEntity,
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: Val::Px(RESULT_PANEL_W),
                    height: Val::Px(RESULT_PANEL_H),
                    position_type: PositionType::Relative,
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.98, 0.99, 1.0, 0.96)),
                BorderColor::all(Color::srgba(0.34, 0.42, 0.55, 0.38)),
                Name::new("ResultPanel"),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new(i18n_text(language, TextKey::ResultTitle)),
                    TextFont {
                        font_size: FontSize::Px(34.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.10, 0.16, 0.24)),
                    TextLayout::justify(Justify::Center),
                    Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(28.0),
                        left: Val::Px(0.0),
                        width: Val::Px(RESULT_PANEL_W),
                        ..default()
                    },
                    Name::new("ResultTitle"),
                    LocalizedText {
                        key: TextKey::ResultTitle,
                    },
                ));
                panel.spawn((
                    Text::new(format_winner_summary(winner, &winner_players, language)),
                    TextFont {
                        font_size: FontSize::Px(24.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.14, 0.20, 0.30)),
                    TextLayout::justify(Justify::Center),
                    Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(88.0),
                        left: Val::Px(28.0),
                        width: Val::Px(RESULT_PANEL_W - 56.0),
                        ..default()
                    },
                    Name::new("ResultWinnerText"),
                ));
                spawn_result_button(
                    panel,
                    result_button_local_rect(ResultAction::RestartMatch),
                    i18n_text(language, TextKey::RestartMatch),
                    Some(TextKey::RestartMatch),
                    Color::srgba(0.42, 0.65, 0.88, 0.38),
                );
                spawn_result_button(
                    panel,
                    result_button_local_rect(ResultAction::MainMenu),
                    i18n_text(language, TextKey::MainMenu),
                    Some(TextKey::MainMenu),
                    Color::srgba(0.72, 0.54, 0.44, 0.30),
                );
            });
        });
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
        apply_result_action(ResultAction::RestartMatch, &mut next_app_state);
    } else if keyboard.just_pressed(KeyCode::Escape) {
        apply_result_action(ResultAction::MainMenu, &mut next_app_state);
    }
}

fn handle_result_click(
    pointer: Res<PointerInputState>,
    windows: Query<&Window>,
    overlay_state: Res<SoundSettingsOverlayState>,
    mut next_app_state: ResMut<NextState<AppState>>,
) {
    if sound_settings_overlay_blocks_input(&overlay_state) {
        return;
    }
    let Some(cursor) = pointer.just_pressed_position() else {
        return;
    };
    let Ok(window) = windows.single() else {
        return;
    };
    if let Some(action) = result_action_at(cursor, window.width(), window.height()) {
        apply_result_action(action, &mut next_app_state);
    }
}

fn spawn_result_button(
    panel: &mut ChildSpawnerCommands<'_>,
    rect: ScreenRect,
    label: &str,
    localized_key: Option<TextKey>,
    color: Color,
) {
    panel
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(rect.x),
                top: Val::Px(rect.y),
                width: Val::Px(rect.w),
                height: Val::Px(rect.h),
                border: UiRect::all(Val::Px(1.0)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                padding: UiRect::horizontal(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(color),
            BorderColor::all(Color::srgba(0.16, 0.22, 0.32, 0.28)),
            Name::new(format!("ResultButton{label}")),
        ))
        .with_children(|button| {
            let mut text_entity = button.spawn((
                Text::new(label),
                TextFont {
                    font_size: FontSize::Px(18.0),
                    ..default()
                },
                TextColor(Color::srgb(0.10, 0.16, 0.24)),
                TextLayout::justify(Justify::Center),
                Name::new(format!("ResultButtonLabel{label}")),
            ));
            if let Some(localized_key) = localized_key {
                text_entity.insert(LocalizedText { key: localized_key });
            }
        });
}

fn format_winner_summary(winner: u8, winner_players: &str, language: Language) -> String {
    match language {
        Language::SimplifiedChinese => format!("队伍 {winner} 获胜\n玩家：{winner_players}"),
        Language::English => format!("Team {winner} wins\nPlayers: {winner_players}"),
    }
}

fn format_winner_players(match_result: &MatchResult, language: Language) -> String {
    if match_result.winner_player_ids.is_empty() {
        return "-".to_string();
    }
    match_result
        .winner_player_ids
        .iter()
        .map(|player_id| match language {
            Language::SimplifiedChinese => format!("玩家{player_id}"),
            Language::English => format!("P{player_id}"),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn result_button_local_rect(action: ResultAction) -> ScreenRect {
    let total_width = RESULT_BUTTON_W * 2.0 + RESULT_BUTTON_GAP;
    let first_x = (RESULT_PANEL_W - total_width) * 0.5;
    let x = match action {
        ResultAction::RestartMatch => first_x,
        ResultAction::MainMenu => first_x + RESULT_BUTTON_W + RESULT_BUTTON_GAP,
    };
    ScreenRect {
        x,
        y: RESULT_BUTTON_TOP,
        w: RESULT_BUTTON_W,
        h: RESULT_BUTTON_H,
    }
}

fn result_panel_rect(window_width: f32, window_height: f32) -> ScreenRect {
    ScreenRect {
        x: (window_width - RESULT_PANEL_W) * 0.5,
        y: (window_height - RESULT_PANEL_H) * 0.5,
        w: RESULT_PANEL_W,
        h: RESULT_PANEL_H,
    }
}

fn result_button_screen_rect(
    action: ResultAction,
    window_width: f32,
    window_height: f32,
) -> ScreenRect {
    let panel = result_panel_rect(window_width, window_height);
    let button = result_button_local_rect(action);
    ScreenRect {
        x: panel.x + button.x,
        y: panel.y + button.y,
        ..button
    }
}

fn result_action_at(cursor: Vec2, window_width: f32, window_height: f32) -> Option<ResultAction> {
    [ResultAction::RestartMatch, ResultAction::MainMenu]
        .into_iter()
        .find(|action| {
            result_button_screen_rect(*action, window_width, window_height).contains(cursor)
        })
}

fn apply_result_action(action: ResultAction, next_app_state: &mut NextState<AppState>) {
    match action {
        ResultAction::RestartMatch => next_app_state.set(AppState::LoadingGame),
        ResultAction::MainMenu => next_app_state.set(AppState::MainMenu),
    }
}

fn cleanup_result(
    mut commands: Commands,
    query: Query<Entity, (With<ResultEntity>, Without<ChildOf>)>,
) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::piece::PieceStatus;
    use crate::domain::rules::LaunchRule;
    use crate::gameplay::ai::AiDifficulty;
    use crate::gameplay::match_flow::{MatchSetup, build_match_rosters};
    use crate::gameplay::skill_flow::record_skill_action;
    use crate::gameplay::turn_flow::{HOME_ENTRY_PROGRESS, record_turn_action};
    use bevy::ecs::system::SystemState;

    fn test_profile(width: f32, height: f32) -> DeviceProfile {
        DeviceProfile::from_window_size(width, height)
    }

    fn test_roster() -> PlayerRoster {
        let setup = MatchSetup {
            mode: GameMode::TwoVsTwo,
            rule_set: crate::data::rule_set::RuleSet::Creative,
            ai_difficulty: AiDifficulty::Normal,
            fast_mode: false,
            launch_rule: LaunchRule::SixOnly,
            player_seats: PlayerSeat::ALL,
            pieces_per_player: 2,
            player_controls: [
                PlayerControl::Human,
                PlayerControl::Ai,
                PlayerControl::Human,
                PlayerControl::Ai,
            ],
        };
        let (players, _) = build_match_rosters(&setup);
        PlayerRoster::from_players(players)
    }

    fn player_hud_badge_cluster_rect(entry: ScreenRect, seat: PlayerSeat) -> ScreenRect {
        let w = player_hud_badges_total_width();
        let x = if player_hud_badges_align_to_left(seat) {
            entry.x
        } else {
            entry.x + entry.w - w
        };
        ScreenRect {
            x,
            y: entry.y + (entry.h - HUD_BADGE_H) * 0.5,
            w,
            h: HUD_BADGE_H,
        }
    }

    #[test]
    fn player_hud_entries_align_outside_hangar_edges() {
        let profile = test_profile(1280.0, 720.0);
        let board = gameplay_board_screen_rect(1280.0, 720.0, profile);
        let p1 = player_hud_entry_rect(1280.0, 720.0, profile, PlayerSeat::Blue);
        let p2 = player_hud_entry_rect(1280.0, 720.0, profile, PlayerSeat::Red);
        let p3 = player_hud_entry_rect(1280.0, 720.0, profile, PlayerSeat::Green);
        let p4 = player_hud_entry_rect(1280.0, 720.0, profile, PlayerSeat::Yellow);
        let blue = seat_hangar_screen_rect(board, PlayerSeat::Blue);
        let red = seat_hangar_screen_rect(board, PlayerSeat::Red);
        let green = seat_hangar_screen_rect(board, PlayerSeat::Green);
        let yellow = seat_hangar_screen_rect(board, PlayerSeat::Yellow);

        assert!((p1.x - blue.x).abs() < 0.001);
        assert!((p1.y + p1.h - blue.y).abs() < 0.001);
        assert!((p2.x + p2.w - (red.x + red.w)).abs() < 0.001);
        assert!((p2.y + p2.h - red.y).abs() < 0.001);
        assert!((p3.x - green.x).abs() < 0.001);
        assert!((p3.y - (green.y + green.h)).abs() < 0.001);
        assert!((p4.x + p4.w - (yellow.x + yellow.w)).abs() < 0.001);
        assert!((p4.y - (yellow.y + yellow.h)).abs() < 0.001);
        for (entry, hangar) in [(p1, blue), (p2, red), (p3, green), (p4, yellow)] {
            assert!(!entry.overlaps(hangar));
        }
    }

    #[test]
    fn visible_player_hud_badges_align_to_outer_hangar_edges() {
        for (width, height) in [(1280.0, 720.0), (1366.0, 1024.0), (1840.0, 2800.0)] {
            let profile = test_profile(width, height);
            let board = gameplay_board_screen_rect(width, height, profile);
            for seat in PlayerSeat::ALL {
                let hangar = seat_hangar_screen_rect(board, seat);
                let entry = player_hud_entry_rect(width, height, profile, seat);
                let badges = player_hud_badge_cluster_rect(entry, seat);
                match seat {
                    PlayerSeat::Blue | PlayerSeat::Green => {
                        assert!(
                            (badges.x - hangar.x).abs() < 0.001,
                            "{seat:?} badges should align to hangar left edge at {width}x{height}"
                        );
                    }
                    PlayerSeat::Red | PlayerSeat::Yellow => {
                        assert!(
                            (badges.x + badges.w - (hangar.x + hangar.w)).abs() < 0.001,
                            "{seat:?} badges should align to hangar right edge at {width}x{height}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn player_hud_badge_alignment_matches_seat_side() {
        assert_eq!(
            player_hud_badge_justify_content(PlayerSeat::Blue),
            JustifyContent::FlexStart
        );
        assert_eq!(
            player_hud_badge_justify_content(PlayerSeat::Green),
            JustifyContent::FlexStart
        );
        assert_eq!(
            player_hud_badge_justify_content(PlayerSeat::Red),
            JustifyContent::FlexEnd
        );
        assert_eq!(
            player_hud_badge_justify_content(PlayerSeat::Yellow),
            JustifyContent::FlexEnd
        );
    }

    #[test]
    fn player_hud_entry_position_follows_player_seat() {
        let profile = test_profile(1280.0, 720.0);
        let board = gameplay_board_screen_rect(1280.0, 720.0, profile);
        let p1_red = player_hud_entry_rect(1280.0, 720.0, profile, PlayerSeat::Red);
        let red = seat_hangar_screen_rect(board, PlayerSeat::Red);

        assert!((p1_red.x + p1_red.w - (red.x + red.w)).abs() < 0.001);
        assert!((p1_red.y + p1_red.h - red.y).abs() < 0.001);
        assert!(!p1_red.overlaps(red));
    }

    #[test]
    fn top_right_controls_match_global_settings_entry() {
        for width in [360.0, 1280.0, 2560.0] {
            let controls = top_right_controls_rect(width);
            let (x, y, w, h) = global_settings_entry_screen_rect(width);

            assert_eq!(controls.x, x);
            assert_eq!(controls.y, y);
            assert_eq!(controls.w, w);
            assert_eq!(controls.h, h);
        }
    }

    #[test]
    fn gameplay_board_screen_rect_matches_camera_world_size_cap_on_large_screens() {
        let profile = test_profile(2560.0, 1600.0);
        let board = gameplay_board_screen_rect(2560.0, 1600.0, profile);

        assert_eq!(board.w, BOARD_WORLD_SIZE);
        assert_eq!(board.h, BOARD_WORLD_SIZE);
        assert!((board.x - (2560.0 - BOARD_WORLD_SIZE) * 0.5).abs() < f32::EPSILON);
        assert!((board.y - (1600.0 - BOARD_WORLD_SIZE) * 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn top_right_player_entry_does_not_cover_game_controls() {
        for (width, height) in [(360.0, 640.0), (640.0, 360.0), (1280.0, 720.0)] {
            let profile = test_profile(width, height);
            let p2 = player_hud_entry_rect(width, height, profile, PlayerSeat::Red);
            let controls = top_right_controls_rect(width);

            assert!(!p2.overlaps(controls));
        }
    }

    #[test]
    fn player_status_cards_fit_within_hangar_width() {
        assert!(HUD_ENTRY_W <= 150.0);
        assert!(HUD_ENTRY_H <= 54.0);
        assert!(player_hud_badges_total_width() <= HUD_ENTRY_W);
        for kind in PLAYER_HUD_BADGES {
            assert!(player_hud_badge_width(kind) >= 28.0);
        }
    }

    #[test]
    fn right_side_player_status_badges_mirror_left_side_order() {
        assert_eq!(
            player_hud_badges_for_seat(PlayerSeat::Blue),
            PLAYER_HUD_BADGES
        );
        assert_eq!(
            player_hud_badges_for_seat(PlayerSeat::Green),
            PLAYER_HUD_BADGES
        );
        assert_eq!(
            player_hud_badges_for_seat(PlayerSeat::Red),
            [
                PlayerHudBadgeKind::Turn,
                PlayerHudBadgeKind::Team,
                PlayerHudBadgeKind::Player,
            ]
        );
        assert_eq!(
            player_hud_badges_for_seat(PlayerSeat::Yellow),
            [
                PlayerHudBadgeKind::Turn,
                PlayerHudBadgeKind::Team,
                PlayerHudBadgeKind::Player,
            ]
        );
    }

    #[test]
    fn player_status_badges_format_independent_info() {
        let roster = test_roster();
        let player = roster
            .players
            .iter()
            .find(|player| player.state.player_id == 1)
            .unwrap();

        assert_eq!(
            player_hud_badge_text(
                PlayerHudBadgeKind::Player,
                player,
                true,
                false,
                Language::SimplifiedChinese,
                0.0,
            ),
            "1"
        );
        assert_eq!(
            player_hud_badge_text(
                PlayerHudBadgeKind::Team,
                player,
                true,
                false,
                Language::SimplifiedChinese,
                0.0,
            ),
            "队1"
        );
        assert_eq!(
            player_hud_badge_text(
                PlayerHudBadgeKind::Player,
                player,
                true,
                false,
                Language::English,
                0.0,
            ),
            "P1"
        );
        assert_eq!(
            player_hud_badge_text(
                PlayerHudBadgeKind::Team,
                player,
                true,
                false,
                Language::English,
                0.0,
            ),
            "T1"
        );
        assert_eq!(
            player_hud_badge_text(
                PlayerHudBadgeKind::Turn,
                player,
                true,
                false,
                Language::SimplifiedChinese,
                0.0,
            ),
            ">"
        );
        assert_eq!(
            player_hud_badge_text(
                PlayerHudBadgeKind::Turn,
                player,
                false,
                false,
                Language::SimplifiedChinese,
                0.0,
            ),
            ""
        );
        assert_eq!(
            player_hud_badge_text(
                PlayerHudBadgeKind::Player,
                player,
                false,
                true,
                Language::SimplifiedChinese,
                0.0,
            ),
            "1✓"
        );
        assert_eq!(
            player_hud_badge_text(
                PlayerHudBadgeKind::Player,
                player,
                false,
                true,
                Language::English,
                0.0,
            ),
            "P1✓"
        );
    }

    #[test]
    fn turn_badge_hides_non_current_and_animates_current_marker() {
        assert_eq!(
            player_hud_badge_visibility(PlayerHudBadgeKind::Turn, false),
            Visibility::Hidden
        );
        assert_eq!(
            player_hud_badge_visibility(PlayerHudBadgeKind::Turn, true),
            Visibility::Visible
        );
        assert_eq!(
            player_hud_turn_indicator_text(false, HUD_TURN_INDICATOR_STEP_SECS),
            ""
        );
        assert_eq!(player_hud_turn_indicator_text(true, 0.0), ">");
        assert_eq!(
            player_hud_turn_indicator_text(true, HUD_TURN_INDICATOR_STEP_SECS),
            ">>"
        );

        let low = player_hud_turn_indicator_pulse(0.75);
        let high = player_hud_turn_indicator_pulse(0.25);
        assert!((0.0..=1.0).contains(&low));
        assert!((0.0..=1.0).contains(&high));
        assert!(high > low);
    }

    #[test]
    fn skill_buttons_use_icon_assets_and_badges_each_charge_independently() {
        let skills = PlayerSkillState {
            player_id: 1,
            dash_charges: 1,
            dash_armed: false,
            snipe_charges: 2,
            swap_charges: 0,
            shield_charges: 1,
            double_dice_charges: 3,
            double_dice_armed: true,
            skip_next_skill_turn: false,
            skill_blocked_this_turn: false,
        };

        assert_eq!(
            skill_icon_asset_path(SkillUiAction::Dash),
            "ui/skills/dash.png"
        );
        assert_eq!(
            skill_icon_asset_path(SkillUiAction::Snipe),
            "ui/skills/snipe.png"
        );
        assert_eq!(
            skill_icon_asset_path(SkillUiAction::Swap),
            "ui/skills/swap.png"
        );
        assert_eq!(
            skill_icon_asset_path(SkillUiAction::Shield),
            "ui/skills/shield.png"
        );
        assert_eq!(
            skill_icon_asset_path(SkillUiAction::DoubleDice),
            "ui/skills/double_dice.png"
        );
        assert_eq!(
            skill_badge_text(skill_charge(SkillUiAction::Dash, &skills)),
            "1"
        );
        assert_eq!(
            skill_badge_text(skill_charge(SkillUiAction::Snipe, &skills)),
            "2"
        );
        assert_eq!(
            skill_badge_text(skill_charge(SkillUiAction::Swap, &skills)),
            "0"
        );
        assert_eq!(
            skill_badge_text(skill_charge(SkillUiAction::Shield, &skills)),
            "1"
        );
        assert_eq!(
            skill_badge_text(skill_charge(SkillUiAction::DoubleDice, &skills)),
            "3"
        );
        assert_eq!(skill_badge_text(120), "99+");
    }

    #[test]
    fn skill_badge_is_circular() {
        assert_eq!(SKILL_BADGE_W, SKILL_BADGE_H);
    }

    #[test]
    fn disabled_skill_controls_remain_readable() {
        assert!(skill_icon_color(false).to_srgba().alpha >= 0.70);
        assert!(skill_badge_color(false).to_srgba().alpha >= 0.80);
        assert!(skill_badge_text_color(false).to_srgba().alpha >= 0.70);
        assert!(skill_button_color(false, false).to_srgba().alpha >= 0.30);
        assert!(skill_button_color(false, true).to_srgba().alpha >= 0.42);
        assert!(skill_block_marker_color().to_srgba().alpha >= 0.90);
        assert!(skill_block_marker_border_color().to_srgba().alpha >= 0.90);
    }

    #[test]
    fn skill_block_marker_only_appears_on_blocked_turn() {
        let mut skills = PlayerSkillState {
            player_id: 1,
            dash_charges: 1,
            dash_armed: false,
            snipe_charges: 1,
            swap_charges: 1,
            shield_charges: 1,
            double_dice_charges: 1,
            double_dice_armed: false,
            skip_next_skill_turn: true,
            skill_blocked_this_turn: false,
        };

        assert!(!skill_block_marker_visible(Some(&skills), false));

        skills.skip_next_skill_turn = false;
        skills.skill_blocked_this_turn = true;
        assert!(skill_block_marker_visible(Some(&skills), false));
        assert!(!skill_block_marker_visible(Some(&skills), true));
        assert!(!skill_block_marker_visible(None, false));
    }

    #[test]
    fn skill_board_availability_excludes_home_lane_swap_teammate() {
        let mut world = World::new();
        world.spawn((
            PieceId(1),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: PieceStatus::Active,
                progress: 3,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
        ));
        world.spawn((
            PieceId(2),
            PieceState {
                owner_player_id: 3,
                team_id: 1,
                status: PieceStatus::Active,
                progress: HOME_ENTRY_PROGRESS + 1,
                shield: 0,
                stack_shield: 0,
                motion_serial: 0,
            },
        ));
        let mut system_state: SystemState<HudPieceQuery> = SystemState::new(&mut world);
        let query = system_state.get_mut(&mut world).unwrap();

        let availability = SkillBoardAvailability::from_query(1, 1, GameMode::TwoVsTwo, &query);

        assert!(availability.active_self);
        assert!(!availability.swap_target);
    }

    #[test]
    fn dash_button_requires_a_movable_piece_after_roll() {
        let skills = PlayerSkillState {
            player_id: 1,
            dash_charges: 1,
            dash_armed: false,
            snipe_charges: 0,
            swap_charges: 0,
            shield_charges: 0,
            double_dice_charges: 0,
            double_dice_armed: false,
            skip_next_skill_turn: false,
            skill_blocked_this_turn: false,
        };
        let no_movable_piece = SkillBoardAvailability {
            dash_move_target: false,
            ..default()
        };
        let with_movable_piece = SkillBoardAvailability {
            dash_move_target: true,
            ..default()
        };

        assert!(!is_skill_button_ready(
            SkillUiAction::Dash,
            &skills,
            true,
            &GamePhase::AwaitPieceSelect,
            GameMode::TwoVsTwo,
            no_movable_piece,
        ));
        assert!(is_skill_button_ready(
            SkillUiAction::Dash,
            &skills,
            true,
            &GamePhase::AwaitPieceSelect,
            GameMode::TwoVsTwo,
            with_movable_piece,
        ));
    }

    #[test]
    fn swap_button_requires_ready_source_and_mode_valid_target() {
        let skills = PlayerSkillState {
            player_id: 1,
            dash_charges: 0,
            dash_armed: false,
            snipe_charges: 0,
            swap_charges: 1,
            shield_charges: 0,
            double_dice_charges: 0,
            double_dice_armed: false,
            skip_next_skill_turn: false,
            skill_blocked_this_turn: false,
        };
        let only_source = SkillBoardAvailability {
            active_self: true,
            swap_target: false,
            ..default()
        };
        let source_and_target = SkillBoardAvailability {
            active_self: true,
            swap_target: true,
            ..default()
        };

        assert!(!is_skill_button_ready(
            SkillUiAction::Swap,
            &skills,
            true,
            &GamePhase::AwaitDice,
            GameMode::TwoVsTwo,
            only_source,
        ));
        assert!(is_skill_button_ready(
            SkillUiAction::Swap,
            &skills,
            true,
            &GamePhase::AwaitDice,
            GameMode::TwoVsTwo,
            source_and_target,
        ));
        assert!(is_skill_button_ready(
            SkillUiAction::Swap,
            &skills,
            true,
            &GamePhase::AwaitDice,
            GameMode::OneVsOne,
            source_and_target,
        ));
    }

    #[test]
    fn board_roll_button_stays_inside_center_board_area() {
        for (width, height) in [(1280.0, 720.0), (2560.0, 1600.0), (640.0, 360.0)] {
            let profile = test_profile(width, height);
            let board = gameplay_board_screen_rect(width, height, profile);
            let roll = board_roll_button_rect(width, height, profile);

            assert!(board.contains(Vec2::new(roll.x, roll.y)));
            assert!(board.contains(Vec2::new(roll.x + roll.w, roll.y + roll.h)));
            assert!((roll.x + roll.w * 0.5 - (board.x + board.w * 0.5)).abs() < f32::EPSILON);
            assert!((roll.y + roll.h * 0.5 - (board.y + board.h * 0.5)).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn board_roll_button_is_square_and_large_enough_for_touch() {
        let roll = board_roll_button_rect(1280.0, 720.0, test_profile(1280.0, 720.0));

        assert_eq!(roll.w, BOARD_ROLL_BUTTON_W);
        assert_eq!(roll.h, BOARD_ROLL_BUTTON_H);
        assert_eq!(roll.w, roll.h);
        assert!(roll.w >= 44.0);
    }

    #[test]
    fn shared_skill_buttons_are_square_and_below_board_when_space_allows() {
        let width = 720.0;
        let height = 1280.0;
        let profile = test_profile(width, height);
        let board = gameplay_board_screen_rect(width, height, profile);
        let first = shared_skill_button_rect(width, height, profile, SkillUiAction::Dash);
        let last = shared_skill_button_rect(width, height, profile, SkillUiAction::DoubleDice);
        let bar_center = (first.x + last.x + last.w) * 0.5;

        assert_eq!(first.w, SKILL_BUTTON_SIZE);
        assert_eq!(first.h, SKILL_BUTTON_SIZE);
        assert_eq!(first.w, first.h);
        assert!(first.y >= board.y + board.h);
        assert!((bar_center - (board.x + board.w * 0.5)).abs() < f32::EPSILON);
    }

    #[test]
    fn finish_bounce_charge_bar_matches_skill_bar_width_below_buttons() {
        let width = 720.0;
        let height = 1280.0;
        let profile = test_profile(width, height);
        let first = shared_skill_button_rect(width, height, profile, SkillUiAction::Dash);
        let last = shared_skill_button_rect(width, height, profile, SkillUiAction::DoubleDice);
        let charge_bar = finish_bounce_charge_bar_rect(width, height, profile);

        assert_eq!(charge_bar.w, shared_skill_bar_width());
        assert_eq!(charge_bar.h, FINISH_BOUNCE_CHARGE_BAR_H);
        assert!((charge_bar.x - first.x).abs() < f32::EPSILON);
        assert!((charge_bar.x + charge_bar.w - (last.x + last.w)).abs() < f32::EPSILON);
        assert_eq!(
            charge_bar.y,
            first.y + SKILL_BUTTON_SIZE + FINISH_BOUNCE_CHARGE_BAR_GAP
        );
    }

    #[test]
    fn finish_bounce_charge_bar_formats_progress() {
        let bar_width = shared_skill_bar_width();

        assert_eq!(
            finish_bounce_charge_bar_text(0, true, Language::SimplifiedChinese),
            "折返充能 0/2"
        );
        assert_eq!(
            finish_bounce_charge_bar_text(1, true, Language::SimplifiedChinese),
            "折返充能 1/2"
        );
        assert_eq!(
            finish_bounce_charge_bar_text(2, true, Language::SimplifiedChinese),
            "折返充能 2/2"
        );
        assert_eq!(
            finish_bounce_charge_bar_text(1, true, Language::English),
            "Bounce Charge 1/2"
        );
        assert_eq!(
            finish_bounce_charge_bar_text(1, false, Language::SimplifiedChinese),
            ""
        );
        assert_eq!(finish_bounce_charge_fill_width(0, bar_width), 0.0);
        assert_eq!(
            finish_bounce_charge_fill_width(1, bar_width),
            bar_width * 0.5
        );
        assert_eq!(finish_bounce_charge_fill_width(2, bar_width), bar_width);
        assert_eq!(finish_bounce_charge_fill_width(9, bar_width), bar_width);
    }

    #[test]
    fn event_notice_panel_mirrors_shared_skill_bar_above_board() {
        let width = 720.0;
        let height = 1280.0;
        let profile = test_profile(width, height);
        let board = gameplay_board_screen_rect(width, height, profile);
        let notice = event_notice_panel_rect(width, height, profile);
        let first = shared_skill_button_rect(width, height, profile, SkillUiAction::Dash);
        let last = shared_skill_button_rect(width, height, profile, SkillUiAction::DoubleDice);

        assert_eq!(notice.w, shared_skill_bar_width());
        assert_eq!(notice.h, SKILL_BUTTON_SIZE);
        assert!((notice.x - first.x).abs() < f32::EPSILON);
        assert!((notice.x + notice.w - (last.x + last.w)).abs() < f32::EPSILON);
        assert!((notice.x + notice.w * 0.5 - (board.x + board.w * 0.5)).abs() < f32::EPSILON);
        assert!((notice.y + notice.h + HUD_EDGE_MARGIN - board.y).abs() < f32::EPSILON);
    }

    #[test]
    fn shared_skill_buttons_are_large_enough_for_touch() {
        for action in HUD_SKILL_ACTIONS {
            let rect = shared_skill_button_rect(1280.0, 720.0, test_profile(1280.0, 720.0), action);
            assert!(rect.w >= 44.0);
            assert!(rect.h >= 44.0);
        }
    }

    #[test]
    fn skill_tip_rect_stays_inside_window_and_near_skill_button() {
        let width = 720.0;
        let height = 1280.0;
        let profile = test_profile(width, height);

        for action in HUD_SKILL_ACTIONS {
            let button = shared_skill_button_rect(width, height, profile, action);
            let tip = skill_tip_rect(width, height, profile, action);

            assert!(tip.x >= HUD_EDGE_MARGIN);
            assert!(tip.y >= HUD_EDGE_MARGIN);
            assert!(tip.x + tip.w <= width - HUD_EDGE_MARGIN + f32::EPSILON);
            assert!(tip.y + tip.h <= height - HUD_EDGE_MARGIN + f32::EPSILON);
            assert!((tip.x + tip.w * 0.5 - (button.x + button.w * 0.5)).abs() <= tip.w * 0.5);
        }
    }

    #[test]
    fn skill_tip_text_covers_every_skill() {
        for action in HUD_SKILL_ACTIONS {
            assert!(!skill_action_name(action, Language::SimplifiedChinese).is_empty());
            assert!(!skill_action_name(action, Language::English).is_empty());
            assert!(skill_tip_body(action, Language::SimplifiedChinese).len() > 24);
            assert!(skill_tip_body(action, Language::English).len() > 24);
        }
    }

    #[test]
    fn skill_button_hit_testing_maps_points_to_actions() {
        let width = 1280.0;
        let height = 720.0;
        let profile = test_profile(width, height);

        for action in HUD_SKILL_ACTIONS {
            let rect = shared_skill_button_rect(width, height, profile, action);
            assert_eq!(
                skill_action_at_point(
                    Vec2::new(rect.x + rect.w * 0.5, rect.y + rect.h * 0.5),
                    width,
                    height,
                    profile
                ),
                Some(action)
            );
        }
    }

    #[test]
    fn skill_release_queue_rules_keep_tips_read_only() {
        assert!(should_queue_skill_from_release(true, false, true));
        assert!(!should_queue_skill_from_release(true, true, true));
        assert!(!should_queue_skill_from_release(false, false, true));
        assert!(!should_queue_skill_from_release(true, false, false));
    }

    #[test]
    fn same_frame_tap_release_inside_skill_button_counts_as_click() {
        let width = 1280.0;
        let height = 720.0;
        let profile = test_profile(width, height);
        let action = SkillUiAction::Dash;
        let rect = shared_skill_button_rect(width, height, profile, action);
        let release_position = Vec2::new(rect.x + rect.w * 0.5, rect.y + rect.h * 0.5);

        let release_inside =
            skill_release_inside_button(action, release_position, width, height, profile);

        assert!(release_inside);
        assert!(should_queue_skill_from_release(release_inside, false, true));
    }

    #[test]
    fn visible_shared_controls_are_treated_as_interactive() {
        let window = Window {
            resolution: (1280, 720).into(),
            ..default()
        };
        let profile = test_profile(1280.0, 720.0);
        let roster = test_roster();
        let mut hud_state = PlayerHudState::default();
        let roll = board_roll_button_rect(window.width(), window.height(), profile);
        let settings = top_right_controls_rect(window.width());
        let skill = shared_skill_button_rect(
            window.width(),
            window.height(),
            profile,
            SkillUiAction::Shield,
        );
        let tip = skill_tip_rect(
            window.width(),
            window.height(),
            profile,
            SkillUiAction::Shield,
        );
        let log = event_log_toggle_rect(window.width(), window.height(), profile);
        let log_panel = event_log_panel_rect(window.width(), window.height(), profile);

        assert!(!player_hud_point_is_interactive(
            Vec2::new(roll.x + 2.0, roll.y + 2.0),
            &window,
            profile,
            &roster,
            &hud_state,
            true
        ));
        hud_state.board_roll_button_visible = true;
        assert!(player_hud_point_is_interactive(
            Vec2::new(roll.x + 2.0, roll.y + 2.0),
            &window,
            profile,
            &roster,
            &hud_state,
            true
        ));
        hud_state.board_roll_button_visible = false;
        assert!(player_hud_point_is_interactive(
            Vec2::new(settings.x + 2.0, settings.y + 2.0),
            &window,
            profile,
            &roster,
            &hud_state,
            true
        ));
        assert!(player_hud_point_is_interactive(
            Vec2::new(skill.x + 2.0, skill.y + 2.0),
            &window,
            profile,
            &roster,
            &hud_state,
            true
        ));
        assert!(!player_hud_point_is_interactive(
            Vec2::new(skill.x + 2.0, skill.y + 2.0),
            &window,
            profile,
            &roster,
            &hud_state,
            false
        ));
        assert!(!player_hud_point_is_interactive(
            Vec2::new(tip.x + 2.0, tip.y + 2.0),
            &window,
            profile,
            &roster,
            &hud_state,
            true
        ));
        hud_state.skill_tip_action = Some(SkillUiAction::Shield);
        assert!(player_hud_point_is_interactive(
            Vec2::new(tip.x + 2.0, tip.y + 2.0),
            &window,
            profile,
            &roster,
            &hud_state,
            true
        ));
        assert!(!player_hud_point_is_interactive(
            Vec2::new(tip.x + 2.0, tip.y + 2.0),
            &window,
            profile,
            &roster,
            &hud_state,
            false
        ));
        hud_state.skill_tip_action = None;
        assert!(player_hud_point_is_interactive(
            Vec2::new(log.x + 2.0, log.y + 2.0),
            &window,
            profile,
            &roster,
            &hud_state,
            true
        ));
        assert!(!player_hud_point_is_interactive(
            Vec2::new(log_panel.x + 2.0, log_panel.y + 2.0),
            &window,
            profile,
            &roster,
            &hud_state,
            true
        ));
        hud_state.event_log_expanded = true;
        assert!(player_hud_point_is_interactive(
            Vec2::new(log_panel.x + 2.0, log_panel.y + 2.0),
            &window,
            profile,
            &roster,
            &hud_state,
            true
        ));
    }

    #[test]
    fn roll_button_text_never_shows_roll_prompt_or_dice_values() {
        assert_eq!(roll_button_text(false, Language::SimplifiedChinese), "");
        assert_eq!(roll_button_text(true, Language::SimplifiedChinese), "×");
        assert_eq!(roll_button_text(true, Language::English), "x");
    }

    #[test]
    fn event_log_collects_turn_and_skill_actions() {
        let mut event_log = EventLogState::default();
        let mut turn_state = TurnState::opening_turn();
        let mut skill_roster = SkillRoster::default();

        record_turn_action(&mut turn_state, "P1 rolled 4");
        sync_event_log(
            &mut event_log,
            &turn_state,
            &skill_roster,
            Language::SimplifiedChinese,
        );
        sync_event_log(
            &mut event_log,
            &turn_state,
            &skill_roster,
            Language::SimplifiedChinese,
        );

        assert_eq!(event_log.entries, vec!["第1回合 玩家1：掷出 4"]);

        record_skill_action(
            &mut skill_roster,
            turn_state.turn_index,
            1,
            "P1 used Shield",
        );
        sync_event_log(
            &mut event_log,
            &turn_state,
            &skill_roster,
            Language::SimplifiedChinese,
        );

        assert_eq!(
            event_log.entries,
            vec!["第1回合 玩家1：掷出 4", "第1回合 玩家1：使用护盾"]
        );
    }

    #[test]
    fn event_log_formats_player_prefix_for_bare_skill_actions() {
        let mut event_log = EventLogState::default();
        let turn_state = TurnState::opening_turn();
        let mut skill_roster = SkillRoster::default();

        record_skill_action(
            &mut skill_roster,
            turn_state.turn_index,
            1,
            "Snipe selection cancelled",
        );
        sync_event_log(
            &mut event_log,
            &turn_state,
            &skill_roster,
            Language::SimplifiedChinese,
        );

        assert_eq!(event_log.entries, vec!["第1回合 玩家1：已取消狙击选择"]);
    }

    #[test]
    fn event_notice_formats_tile_event_actions() {
        assert_eq!(
            event_notice_text_from_action(
                "rolled 3, moved piece #1 to tile 5; event advance +2",
                Language::SimplifiedChinese,
            ),
            Some("事件：前进 +2\n额外前进 2 格。".to_string())
        );
        assert_eq!(
            event_notice_text_from_action(
                "rolled 3, moved piece #1 to tile 5; event advance +2",
                Language::English,
            ),
            Some("Event: Advance +2\nMove forward 2 extra tiles.".to_string())
        );
        assert_eq!(
            event_notice_text_from_action(
                "rolled 4, moved piece #1 to tile 9; event GainShield: gained shield (2)",
                Language::SimplifiedChinese,
            ),
            Some("事件：护盾 +1\n当前护盾：2".to_string())
        );
        assert_eq!(
            event_notice_text_from_action(
                "rolled 2, moved piece #1 to tile 3; jumped to next same-color tile 6, pre-jump event tile 0: event GainSkillCharge: gained 1 Dash charge",
                Language::SimplifiedChinese,
            ),
            Some("事件：技能充能\n冲刺 +1".to_string())
        );
        assert_eq!(
            event_notice_text_from_action(
                "rolled 2, moved piece #1 to tile 55; finish bounce 1/2",
                Language::SimplifiedChinese,
            ),
            Some("终点折返 1/2\n累计到 2/2 奖励技能。".to_string())
        );
        assert_eq!(
            event_notice_text_from_action(
                "rolled 2, moved piece #1 to tile 55; finish bounce 2/2: gained 1 Shield charge",
                Language::SimplifiedChinese,
            ),
            Some("终点折返 2/2\n护盾 +1".to_string())
        );
    }

    #[test]
    fn event_notice_ignores_non_event_actions() {
        assert_eq!(
            event_notice_text_from_action(
                "rolled 4, moved piece #1 to tile 9",
                Language::SimplifiedChinese,
            ),
            None
        );
        assert_eq!(
            event_notice_text_from_action(
                "P1 used Shield on piece #1",
                Language::SimplifiedChinese
            ),
            None
        );
    }

    #[test]
    fn event_log_does_not_replay_stale_turn_action_on_later_turns() {
        let mut event_log = EventLogState::default();
        let mut turn_state = TurnState::opening_turn();
        let skill_roster = SkillRoster::default();

        record_turn_action(&mut turn_state, "P1 rolled 1 but had no legal action");
        sync_event_log(
            &mut event_log,
            &turn_state,
            &skill_roster,
            Language::SimplifiedChinese,
        );

        turn_state.turn_index += 1;
        sync_event_log(
            &mut event_log,
            &turn_state,
            &skill_roster,
            Language::SimplifiedChinese,
        );

        assert_eq!(
            event_log.entries,
            vec!["第1回合 玩家1：掷出 1，没有可执行动作"]
        );

        record_turn_action(&mut turn_state, "P1 rolled 1 but had no legal action");
        sync_event_log(
            &mut event_log,
            &turn_state,
            &skill_roster,
            Language::SimplifiedChinese,
        );

        assert_eq!(
            event_log.entries,
            vec![
                "第1回合 玩家1：掷出 1，没有可执行动作",
                "第2回合 玩家1：掷出 1，没有可执行动作"
            ]
        );
    }

    #[test]
    fn event_log_does_not_replay_stale_skill_action_on_later_turns() {
        let mut event_log = EventLogState::default();
        let mut turn_state = TurnState::opening_turn();
        let mut skill_roster = SkillRoster::default();

        record_skill_action(
            &mut skill_roster,
            turn_state.turn_index,
            1,
            "P1 armed Dash for +3 movement",
        );
        sync_event_log(
            &mut event_log,
            &turn_state,
            &skill_roster,
            Language::SimplifiedChinese,
        );

        turn_state.turn_index += 1;
        sync_event_log(
            &mut event_log,
            &turn_state,
            &skill_roster,
            Language::SimplifiedChinese,
        );

        assert_eq!(
            event_log.entries,
            vec!["第1回合 玩家1：已启用冲刺，本回合移动 +3"]
        );

        record_skill_action(
            &mut skill_roster,
            turn_state.turn_index,
            1,
            "P1 armed Dash for +3 movement",
        );
        sync_event_log(
            &mut event_log,
            &turn_state,
            &skill_roster,
            Language::SimplifiedChinese,
        );

        assert_eq!(
            event_log.entries,
            vec![
                "第1回合 玩家1：已启用冲刺，本回合移动 +3",
                "第2回合 玩家1：已启用冲刺，本回合移动 +3"
            ]
        );
    }

    #[test]
    fn event_log_prunes_old_entries_above_limit() {
        let mut event_log = EventLogState::default();

        for index in 0..(EVENT_LOG_MAX_ENTRIES + 3) {
            push_event_log_entry(&mut event_log, format!("entry {index}"));
        }

        assert_eq!(event_log.entries.len(), EVENT_LOG_MAX_ENTRIES);
        assert_eq!(event_log.entries.first().unwrap(), "entry 3");
        assert_eq!(
            event_log.entries.last().unwrap(),
            &format!("entry {}", EVENT_LOG_MAX_ENTRIES + 2)
        );
    }

    #[test]
    fn result_winner_players_are_formatted_with_player_ids() {
        let result = MatchResult {
            winner_team_id: Some(1),
            winner_player_ids: vec![1, 3],
            finished: true,
        };

        assert_eq!(
            format_winner_players(&result, Language::SimplifiedChinese),
            "玩家1, 玩家3"
        );
        assert_eq!(format_winner_players(&result, Language::English), "P1, P3");
        assert_eq!(
            format_winner_players(&MatchResult::default(), Language::SimplifiedChinese),
            "-"
        );
    }

    #[test]
    fn result_buttons_are_same_size_and_centered_in_panel() {
        let restart = result_button_local_rect(ResultAction::RestartMatch);
        let main_menu = result_button_local_rect(ResultAction::MainMenu);

        assert_eq!(restart.w, main_menu.w);
        assert_eq!(restart.h, main_menu.h);
        assert_eq!(main_menu.x, restart.x + restart.w + RESULT_BUTTON_GAP);
        assert!(
            ((restart.x + main_menu.x + main_menu.w) * 0.5 - RESULT_PANEL_W * 0.5).abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn result_click_targets_map_to_buttons() {
        let restart = result_button_screen_rect(ResultAction::RestartMatch, 1280.0, 720.0);
        let main_menu = result_button_screen_rect(ResultAction::MainMenu, 1280.0, 720.0);

        assert_eq!(
            result_action_at(
                Vec2::new(restart.x + restart.w * 0.5, restart.y + restart.h * 0.5),
                1280.0,
                720.0
            ),
            Some(ResultAction::RestartMatch)
        );
        assert_eq!(
            result_action_at(
                Vec2::new(
                    main_menu.x + main_menu.w * 0.5,
                    main_menu.y + main_menu.h * 0.5
                ),
                1280.0,
                720.0
            ),
            Some(ResultAction::MainMenu)
        );
        assert_eq!(result_action_at(Vec2::new(20.0, 20.0), 1280.0, 720.0), None);
    }
}
