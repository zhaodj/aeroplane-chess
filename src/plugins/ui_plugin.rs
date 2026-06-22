use bevy::ecs::system::SystemParam;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;

use crate::constants::{BOARD_WORLD_SIZE, gameplay_board_target_pixels};
use crate::data::game_mode::GameMode;
use crate::domain::piece::PieceState;
use crate::domain::player::PlayerControl;
use crate::gameplay::match_flow::{
    MatchConfig, MatchResult, PlayerProfile, PlayerRoster, PlayerSeat, hangar_center_for_seat,
};
use crate::gameplay::skill_flow::{
    PlayerSkillState, SkillRoster, can_use_skill_this_turn, is_active_teammate_piece,
    is_current_player_active_piece, is_current_player_dash_move_piece, is_legal_shield_target,
    is_legal_snipe_target, player_skill_state,
};
use crate::gameplay::turn_flow::TurnState;
use crate::platform::{DeviceProfile, PointerInputState};
use crate::plugins::effects_plugin::EffectRevealDelays;
use crate::plugins::menu_plugin::{SoundSettingsOverlayState, global_settings_entry_screen_rect};
use crate::plugins::piece_plugin::PieceId;
use crate::plugins::skill_plugin::{SkillTargetState, SkillUiAction, SkillUiRequest};
use crate::plugins::turn_plugin::TurnUiRequest;
use crate::states::{AppState, GamePhase};

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerHudState>()
            .init_resource::<EventLogState>()
            .add_systems(OnEnter(AppState::InGame), spawn_hud)
            .add_systems(
                Update,
                (
                    handle_player_hud_click,
                    handle_event_log_scroll,
                    update_player_hud_layout,
                    update_hud_content,
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
}

#[derive(Resource, Default)]
struct EventLogState {
    expanded: bool,
    scroll_to_bottom_requested: bool,
    entries: Vec<String>,
    last_turn_action_key: Option<u64>,
    last_skill_action_key: Option<u64>,
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
type BoardRollButtonQuery<'w, 's> = Query<
    'w,
    's,
    &'static mut BackgroundColor,
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
    board_roll_button_query: BoardRollButtonQuery<'w, 's>,
    board_roll_button_text_query: BoardRollButtonTextQuery<'w, 's>,
}

const HUD_EDGE_MARGIN: f32 = 10.0;
const HANGAR_BACKGROUND_WORLD_SIZE: f32 = 150.0;
const HUD_ENTRY_W: f32 = 98.0;
const HUD_ENTRY_H: f32 = 32.0;
const HUD_BADGE_H: f32 = 28.0;
const HUD_BADGE_GAP: f32 = 3.0;
const HUD_BADGE_PLAYER_W: f32 = 32.0;
const HUD_BADGE_TEAM_W: f32 = 32.0;
const HUD_BADGE_TURN_W: f32 = 28.0;
const SKILL_BUTTON_SIZE: f32 = 54.0;
const SKILL_ICON_SIZE: f32 = 42.0;
const SKILL_BADGE_W: f32 = 20.0;
const SKILL_BADGE_H: f32 = 20.0;
const SKILL_BUTTON_GAP: f32 = 8.0;
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
            availability.dash_move_target |=
                is_current_player_dash_move_piece(current_player, piece_state);
            availability.active_self |= is_current_player_active_piece(current_player, piece_state);
            availability.active_teammate |=
                is_active_teammate_piece(current_player, current_team, piece_state);
        }
        availability
    }
}

fn spawn_hud(
    mut commands: Commands,
    mut hud_state: ResMut<PlayerHudState>,
    mut event_log: ResMut<EventLogState>,
    player_roster: Res<PlayerRoster>,
    asset_server: Res<AssetServer>,
) {
    hud_state.event_log_expanded = false;
    event_log.expanded = false;
    event_log.scroll_to_bottom_requested = false;
    event_log.entries.clear();
    event_log.last_turn_action_key = None;
    event_log.last_skill_action_key = None;
    event_log.entries.push("Match started".to_string());

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
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
                Text::new("Roll"),
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
                Name::new(format!("SharedSkillButton{}", skill_action_name(action))),
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
                        skill_action_name(action)
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
                            skill_action_name(action)
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
                                skill_action_name(action)
                            )),
                            SharedSkillButtonText { action },
                        ));
                    });
            });
    }

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
                Text::new("Log"),
                TextFont {
                    font_size: FontSize::Px(16.0),
                    ..default()
                },
                TextColor(Color::srgb(0.10, 0.16, 0.24)),
                TextLayout::justify(Justify::Center),
                Name::new("EventLogToggleText"),
                EventLogToggleText,
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
                        Text::new("Match started"),
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
                    justify_content: JustifyContent::Center,
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
    }

    for mut node in &mut event_notice_panel_query {
        apply_rect_to_node(
            &mut node,
            event_notice_panel_rect(window_width, window_height, *device_profile),
        );
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
    time: Res<Time>,
    piece_query: HudPieceQuery,
    mut queries: HudContentQueries,
) {
    for (entry, mut background, mut border) in &mut queries.entry_style_query {
        let is_current = entry.player_id == turn_state.current_player;
        let _color = player_roster
            .players
            .iter()
            .find(|player| player.state.player_id == entry.player_id)
            .map(|player| player.color)
            .unwrap_or(Color::srgb(0.78, 0.82, 0.89));
        *background = BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0));
        *border = BorderColor::all(if is_current {
            Color::srgba(0.12, 0.22, 0.32, 0.0)
        } else {
            Color::srgba(0.10, 0.16, 0.24, 0.0)
        });
    }

    for (badge, mut background, mut border) in &mut queries.badge_style_query {
        let is_current = badge.player_id == turn_state.current_player;
        let color = player_roster
            .players
            .iter()
            .find(|player| player.state.player_id == badge.player_id)
            .map(|player| player.color)
            .unwrap_or(Color::srgb(0.78, 0.82, 0.89));
        *background = BackgroundColor(player_hud_badge_color(badge.kind, color, is_current));
        *border = BorderColor::all(player_hud_badge_border_color(badge.kind, is_current));
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
        *text = Text::new(player_hud_badge_text(badge_text.kind, player, is_current));
        *text_color = TextColor(player_hud_badge_text_color(badge_text.kind, is_current));
    }

    let current_profile = player_roster
        .players
        .iter()
        .find(|player| player.state.player_id == turn_state.current_player);
    let current_human_turn = current_profile.is_some_and(|player| {
        player.state.control == PlayerControl::Human && !match_result.finished
    });
    let mut can_use_skill = false;
    let mut board_availability = SkillBoardAvailability::default();
    if let Some(player) = current_profile {
        can_use_skill =
            current_human_turn && can_use_skill_this_turn(&skill_roster, player.state.player_id);
        board_availability = SkillBoardAvailability::from_query(
            player.state.player_id,
            player.state.team_id,
            &piece_query,
        );
    }

    let current_skills = player_skill_state(&skill_roster, turn_state.current_player);
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

    let roll_ready = current_human_turn && matches!(game_phase.get(), GamePhase::AwaitDice);
    let cancel_target_ready = current_human_turn
        && matches!(game_phase.get(), GamePhase::ResolveSkillEffect)
        && skill_target_state.is_active();

    for mut background in &mut queries.board_roll_button_query {
        *background = BackgroundColor(skill_button_color(
            roll_ready || cancel_target_ready,
            current_human_turn,
        ));
    }
    for mut text in &mut queries.board_roll_button_text_query {
        *text = Text::new(roll_button_text(
            roll_ready,
            cancel_target_ready,
            turn_state.current_roll,
            time.elapsed_secs(),
        ));
    }
}

fn update_event_log_content(
    mut event_log: ResMut<EventLogState>,
    turn_state: Res<TurnState>,
    skill_roster: Res<SkillRoster>,
    mut panel_visibility_query: EventLogPanelVisibilityQuery,
    mut toggle_text_query: EventLogToggleTextQuery,
    mut scroll_area_query: EventLogScrollAreaQuery,
    mut scrollbar_thumb_query: EventLogScrollbarThumbQuery,
    mut log_text_query: EventLogTextQuery,
) {
    sync_event_log(&mut event_log, &turn_state, &skill_roster);
    for mut visibility in &mut panel_visibility_query {
        *visibility = if event_log.expanded {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    for mut text in &mut toggle_text_query {
        *text = Text::new(if event_log.expanded { "Log -" } else { "Log +" });
    }
    let entries = format_event_log_entries(&event_log.entries);
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
    mut panel_query: Query<
        (&mut Visibility, &mut BackgroundColor, &mut BorderColor),
        With<EventNoticePanel>,
    >,
    mut text_query: Query<(&mut Text, &mut TextColor), With<EventNoticeText>>,
) {
    let notice = turn_state
        .last_action
        .as_deref()
        .and_then(event_notice_text_from_action);
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

fn handle_player_hud_click(
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
    piece_query: HudPieceQuery,
    mut hud_state: ResMut<PlayerHudState>,
    mut event_log: ResMut<EventLogState>,
    mut skill_ui_request: ResMut<SkillUiRequest>,
    mut turn_ui_request: ResMut<TurnUiRequest>,
) {
    if overlay_state.open || overlay_state.input_captured || match_result.finished {
        return;
    }
    let Some(cursor) = pointer.just_pressed_position() else {
        return;
    };
    let Ok(window) = windows.single() else {
        return;
    };

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
        if current_human_turn && matches!(game_phase.get(), GamePhase::AwaitDice) {
            turn_ui_request.queue_roll();
        } else if current_human_turn && matches!(game_phase.get(), GamePhase::ResolveSkillEffect) {
            skill_ui_request.queue_cancel_target();
        }
        return;
    }

    let current_profile = player_profile(&player_roster, turn_state.current_player);
    let current_human_turn = current_profile.is_some_and(|player| {
        player.state.control == PlayerControl::Human && !match_result.finished
    });
    let can_use_skill =
        current_human_turn && can_use_skill_this_turn(&skill_roster, turn_state.current_player);
    let board_availability = current_profile
        .map(|player| {
            SkillBoardAvailability::from_query(
                player.state.player_id,
                player.state.team_id,
                &piece_query,
            )
        })
        .unwrap_or_default();
    let current_skills = player_skill_state(&skill_roster, turn_state.current_player);
    for action in HUD_SKILL_ACTIONS {
        let rect =
            shared_skill_button_rect(window.width(), window.height(), *device_profile, action);
        if !rect.contains(cursor) {
            continue;
        }
        if current_skills
            .map(|skills| {
                is_skill_button_ready(
                    action,
                    skills,
                    can_use_skill,
                    game_phase.get(),
                    match_config.mode,
                    board_availability,
                )
            })
            .unwrap_or(false)
        {
            skill_ui_request.queue(action);
        }
        return;
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

pub fn player_hud_point_is_interactive(
    point: Vec2,
    window: &Window,
    device_profile: DeviceProfile,
    player_roster: &PlayerRoster,
    hud_state: &PlayerHudState,
) -> bool {
    if top_right_controls_rect(window.width()).contains(point)
        || board_roll_button_rect(window.width(), window.height(), device_profile).contains(point)
        || event_log_toggle_rect(window.width(), window.height(), device_profile).contains(point)
    {
        return true;
    }

    if hud_state.event_log_expanded
        && event_log_panel_rect(window.width(), window.height(), device_profile).contains(point)
    {
        return true;
    }

    for action in HUD_SKILL_ACTIONS {
        if shared_skill_button_rect(window.width(), window.height(), device_profile, action)
            .contains(point)
        {
            return true;
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

pub(crate) fn shared_skill_button_rect(
    window_width: f32,
    window_height: f32,
    device_profile: DeviceProfile,
    action: SkillUiAction,
) -> ScreenRect {
    let board = gameplay_board_screen_rect(window_width, window_height, device_profile);
    let total_width = shared_skill_bar_width();
    let start_x = (board.x + (board.w - total_width) * 0.5).clamp(
        HUD_EDGE_MARGIN,
        (window_width - total_width - HUD_EDGE_MARGIN).max(HUD_EDGE_MARGIN),
    );
    let index = skill_action_index(action) as f32;
    let y = (board.y + board.h + HUD_EDGE_MARGIN).clamp(
        HUD_EDGE_MARGIN,
        (window_height - SKILL_BUTTON_SIZE - HUD_EDGE_MARGIN).max(HUD_EDGE_MARGIN),
    );
    ScreenRect {
        x: start_x + index * (SKILL_BUTTON_SIZE + SKILL_BUTTON_GAP),
        y,
        w: SKILL_BUTTON_SIZE,
        h: SKILL_BUTTON_SIZE,
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

fn skill_action_name(action: SkillUiAction) -> &'static str {
    match action {
        SkillUiAction::Dash => "Dash",
        SkillUiAction::Snipe => "Snipe",
        SkillUiAction::Swap => "Swap",
        SkillUiAction::Shield => "Shield",
        SkillUiAction::DoubleDice => "DoubleDice",
    }
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
) -> String {
    match kind {
        PlayerHudBadgeKind::Player => format!("P{}", player.state.player_id),
        PlayerHudBadgeKind::Team => format!("T{}", player.state.team_id),
        PlayerHudBadgeKind::Turn => {
            if is_current {
                ">".to_string()
            } else {
                "-".to_string()
            }
        }
    }
}

fn player_hud_badge_color(
    kind: PlayerHudBadgeKind,
    player_color: Color,
    is_current: bool,
) -> Color {
    match kind {
        PlayerHudBadgeKind::Player => player_color
            .mix(&Color::WHITE, if is_current { 0.36 } else { 0.62 })
            .with_alpha(0.94),
        PlayerHudBadgeKind::Team => {
            Color::srgba(0.90, 0.94, 0.98, if is_current { 0.96 } else { 0.82 })
        }
        PlayerHudBadgeKind::Turn if is_current => Color::srgba(0.12, 0.18, 0.26, 0.92),
        PlayerHudBadgeKind::Turn => Color::srgba(0.78, 0.82, 0.88, 0.54),
    }
}

fn player_hud_badge_border_color(kind: PlayerHudBadgeKind, is_current: bool) -> Color {
    match (kind, is_current) {
        (PlayerHudBadgeKind::Turn, true) => Color::srgba(0.05, 0.08, 0.12, 0.92),
        (_, true) => Color::srgba(0.08, 0.14, 0.22, 0.70),
        _ => Color::srgba(0.10, 0.16, 0.24, 0.26),
    }
}

fn player_hud_badge_text_color(kind: PlayerHudBadgeKind, is_current: bool) -> Color {
    match (kind, is_current) {
        (PlayerHudBadgeKind::Turn, true) => Color::WHITE,
        (PlayerHudBadgeKind::Turn, false) => Color::srgba(0.10, 0.16, 0.24, 0.40),
        _ => Color::srgb(0.07, 0.11, 0.17),
    }
}

fn roll_button_text(
    roll_ready: bool,
    cancel_target_ready: bool,
    current_roll: Option<u8>,
    elapsed_secs: f32,
) -> String {
    if cancel_target_ready {
        return "X".to_string();
    }
    if let Some(roll) = current_roll {
        return roll.to_string();
    }
    if roll_ready {
        return (((elapsed_secs * 10.0) as u8 % 6) + 1).to_string();
    }
    "-".to_string()
}

fn event_notice_text_from_action(action: &str) -> Option<String> {
    let mut notice = None;
    for segment in action.split(';').flat_map(|part| part.split(", ")) {
        if let Some(event_note) = extract_event_note(segment.trim()) {
            notice = Some(format_event_notice(event_note));
        }
    }
    notice
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

fn format_event_notice(event_note: &str) -> String {
    if event_note == "event advance +2" {
        return "Event: Advance +2\nMoved forward 2 spaces.".to_string();
    }

    if let Some(detail) = event_note.strip_prefix("event GainShield: gained shield ") {
        let shield = detail.trim().trim_start_matches('(').trim_end_matches(')');
        return format!("Event: Shield +1\nCurrent shield: {shield}");
    }

    if let Some(skill) = event_note
        .strip_prefix("event GainSkillCharge: gained 1 ")
        .and_then(|detail| detail.strip_suffix(" charge"))
    {
        return format!("Event: Skill charge\n{skill} +1");
    }

    if let Some(player) =
        event_note.strip_prefix("event DisableNextSkill: next skill turn disabled for ")
    {
        return format!("Event: Skill jam\n{player} cannot use skills next turn.");
    }

    if let Some(piece_id) =
        event_note.strip_prefix("event RemoveEnemyShield: removed shield from piece #")
    {
        return format!("Event: Shield break\nPiece #{piece_id} loses 1 shield.");
    }

    match event_note {
        "event fizzled: could not disable next skill turn" => {
            "Event fizzled\nNo skill turn could be disabled.".to_string()
        }
        "event fizzled: no enemy shield to remove" => {
            "Event fizzled\nNo enemy shield to remove.".to_string()
        }
        "event failed: selected enemy shield target disappeared" => {
            "Event failed\nTarget disappeared.".to_string()
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
                ),
            );
            event_log.last_skill_action_key = Some(key);
        }
    }
}

fn format_event_log_entry(turn_index: u32, player_id: Option<u8>, action: &str) -> String {
    let Some(player_id) = player_id else {
        return format!("T{}: {}", turn_index, action);
    };
    format!(
        "T{} P{}: {}",
        turn_index,
        player_id,
        strip_player_prefix(action, player_id)
    )
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

fn format_event_log_entries(entries: &[String]) -> String {
    if entries.is_empty() {
        "No events yet".to_string()
    } else {
        entries.join("\n")
    }
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
    mode: GameMode,
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
                && mode == GameMode::TwoVsTwo
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

fn skill_button_color(ready: bool, available_context: bool) -> Color {
    if ready {
        Color::srgba(0.45, 0.70, 0.92, 0.52)
    } else if available_context {
        Color::srgba(0.78, 0.84, 0.91, 0.46)
    } else {
        Color::srgba(0.73, 0.77, 0.84, 0.34)
    }
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

fn spawn_result_screen(mut commands: Commands, match_result: Res<MatchResult>) {
    let winner = match_result.winner_team_id.unwrap_or_default();
    let winner_players = format_winner_players(&match_result);
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
                    Text::new("Match Result"),
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
                ));
                panel.spawn((
                    Text::new(format!("Team {winner} wins\nPlayers: {winner_players}")),
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
                    "Restart Match",
                    Color::srgba(0.42, 0.65, 0.88, 0.38),
                );
                spawn_result_button(
                    panel,
                    result_button_local_rect(ResultAction::MainMenu),
                    "Main Menu",
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
    if overlay_state.open || overlay_state.input_captured {
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
            button.spawn((
                Text::new(label),
                TextFont {
                    font_size: FontSize::Px(18.0),
                    ..default()
                },
                TextColor(Color::srgb(0.10, 0.16, 0.24)),
                TextLayout::justify(Justify::Center),
                Name::new(format!("ResultButtonLabel{label}")),
            ));
        });
}

fn format_winner_players(match_result: &MatchResult) -> String {
    if match_result.winner_player_ids.is_empty() {
        return "-".to_string();
    }
    match_result
        .winner_player_ids
        .iter()
        .map(|player_id| format!("P{player_id}"))
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
    use crate::domain::rules::LaunchRule;
    use crate::gameplay::ai::AiDifficulty;
    use crate::gameplay::match_flow::{MatchSetup, build_match_rosters};
    use crate::gameplay::skill_flow::record_skill_action;
    use crate::gameplay::turn_flow::record_turn_action;

    fn test_profile(width: f32, height: f32) -> DeviceProfile {
        DeviceProfile::from_window_size(width, height)
    }

    fn test_roster() -> PlayerRoster {
        let setup = MatchSetup {
            mode: GameMode::TwoVsTwo,
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
            player_hud_badge_text(PlayerHudBadgeKind::Player, player, true),
            "P1"
        );
        assert_eq!(
            player_hud_badge_text(PlayerHudBadgeKind::Team, player, true),
            "T1"
        );
        assert_eq!(
            player_hud_badge_text(PlayerHudBadgeKind::Turn, player, true),
            ">"
        );
        assert_eq!(
            player_hud_badge_text(PlayerHudBadgeKind::Turn, player, false),
            "-"
        );
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
    fn shared_roll_and_settings_entries_are_treated_as_interactive() {
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
        let log = event_log_toggle_rect(window.width(), window.height(), profile);
        let log_panel = event_log_panel_rect(window.width(), window.height(), profile);

        assert!(player_hud_point_is_interactive(
            Vec2::new(roll.x + 2.0, roll.y + 2.0),
            &window,
            profile,
            &roster,
            &hud_state
        ));
        assert!(player_hud_point_is_interactive(
            Vec2::new(settings.x + 2.0, settings.y + 2.0),
            &window,
            profile,
            &roster,
            &hud_state
        ));
        assert!(player_hud_point_is_interactive(
            Vec2::new(skill.x + 2.0, skill.y + 2.0),
            &window,
            profile,
            &roster,
            &hud_state
        ));
        assert!(player_hud_point_is_interactive(
            Vec2::new(log.x + 2.0, log.y + 2.0),
            &window,
            profile,
            &roster,
            &hud_state
        ));
        assert!(!player_hud_point_is_interactive(
            Vec2::new(log_panel.x + 2.0, log_panel.y + 2.0),
            &window,
            profile,
            &roster,
            &hud_state
        ));
        hud_state.event_log_expanded = true;
        assert!(player_hud_point_is_interactive(
            Vec2::new(log_panel.x + 2.0, log_panel.y + 2.0),
            &window,
            profile,
            &roster,
            &hud_state
        ));
    }

    #[test]
    fn roll_button_text_hides_old_roll_without_current_roll() {
        assert_eq!(roll_button_text(false, false, None, 0.0), "-");
        assert_eq!(roll_button_text(false, false, Some(5), 0.0), "5");
        assert_eq!(roll_button_text(false, true, Some(5), 0.0), "X");
        assert_ne!(roll_button_text(true, false, None, 0.0), "-");
    }

    #[test]
    fn event_log_collects_turn_and_skill_actions() {
        let mut event_log = EventLogState::default();
        let mut turn_state = TurnState::opening_turn();
        let mut skill_roster = SkillRoster::default();

        record_turn_action(&mut turn_state, "P1 rolled 4");
        sync_event_log(&mut event_log, &turn_state, &skill_roster);
        sync_event_log(&mut event_log, &turn_state, &skill_roster);

        assert_eq!(event_log.entries, vec!["T1 P1: rolled 4"]);

        record_skill_action(
            &mut skill_roster,
            turn_state.turn_index,
            1,
            "P1 used Shield",
        );
        sync_event_log(&mut event_log, &turn_state, &skill_roster);

        assert_eq!(
            event_log.entries,
            vec!["T1 P1: rolled 4", "T1 P1: used Shield"]
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
        sync_event_log(&mut event_log, &turn_state, &skill_roster);

        assert_eq!(event_log.entries, vec!["T1 P1: Snipe selection cancelled"]);
    }

    #[test]
    fn event_notice_formats_tile_event_actions() {
        assert_eq!(
            event_notice_text_from_action("rolled 3, moved piece #1 to tile 5; event advance +2"),
            Some("Event: Advance +2\nMoved forward 2 spaces.".to_string())
        );
        assert_eq!(
            event_notice_text_from_action(
                "rolled 4, moved piece #1 to tile 9; event GainShield: gained shield (2)"
            ),
            Some("Event: Shield +1\nCurrent shield: 2".to_string())
        );
        assert_eq!(
            event_notice_text_from_action(
                "rolled 2, moved piece #1 to tile 3; jumped to next same-color tile 6, pre-jump event tile 0: event GainSkillCharge: gained 1 Dash charge"
            ),
            Some("Event: Skill charge\nDash +1".to_string())
        );
    }

    #[test]
    fn event_notice_ignores_non_event_actions() {
        assert_eq!(
            event_notice_text_from_action("rolled 4, moved piece #1 to tile 9"),
            None
        );
        assert_eq!(
            event_notice_text_from_action("P1 used Shield on piece #1"),
            None
        );
    }

    #[test]
    fn event_log_does_not_replay_stale_turn_action_on_later_turns() {
        let mut event_log = EventLogState::default();
        let mut turn_state = TurnState::opening_turn();
        let skill_roster = SkillRoster::default();

        record_turn_action(&mut turn_state, "P1 rolled 1 but had no legal action");
        sync_event_log(&mut event_log, &turn_state, &skill_roster);

        turn_state.turn_index += 1;
        sync_event_log(&mut event_log, &turn_state, &skill_roster);

        assert_eq!(
            event_log.entries,
            vec!["T1 P1: rolled 1 but had no legal action"]
        );

        record_turn_action(&mut turn_state, "P1 rolled 1 but had no legal action");
        sync_event_log(&mut event_log, &turn_state, &skill_roster);

        assert_eq!(
            event_log.entries,
            vec![
                "T1 P1: rolled 1 but had no legal action",
                "T2 P1: rolled 1 but had no legal action"
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
        sync_event_log(&mut event_log, &turn_state, &skill_roster);

        turn_state.turn_index += 1;
        sync_event_log(&mut event_log, &turn_state, &skill_roster);

        assert_eq!(event_log.entries, vec!["T1 P1: armed Dash for +3 movement"]);

        record_skill_action(
            &mut skill_roster,
            turn_state.turn_index,
            1,
            "P1 armed Dash for +3 movement",
        );
        sync_event_log(&mut event_log, &turn_state, &skill_roster);

        assert_eq!(
            event_log.entries,
            vec![
                "T1 P1: armed Dash for +3 movement",
                "T2 P1: armed Dash for +3 movement"
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

        assert_eq!(format_winner_players(&result), "P1, P3");
        assert_eq!(format_winner_players(&MatchResult::default()), "-");
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
