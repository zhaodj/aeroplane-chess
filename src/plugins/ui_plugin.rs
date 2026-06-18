use bevy::prelude::*;

use crate::data::game_mode::GameMode;
use crate::domain::piece::PieceState;
use crate::domain::player::PlayerControl;
use crate::gameplay::match_flow::{
    MatchConfig, MatchResult, PlayerProfile, PlayerRoster, PlayerSeat,
};
use crate::gameplay::skill_flow::{
    PlayerSkillState, SkillRoster, can_use_skill_this_turn, is_active_teammate_piece,
    is_current_player_active_piece, is_legal_shield_target, is_legal_snipe_target,
    player_skill_state,
};
use crate::gameplay::turn_flow::{TurnInputState, TurnState};
use crate::platform::{DeviceProfile, PointerInputState};
use crate::plugins::menu_plugin::SoundSettingsOverlayState;
use crate::plugins::piece_plugin::PieceId;
use crate::plugins::skill_plugin::{SkillTargetState, SkillUiAction, SkillUiRequest};
use crate::plugins::turn_plugin::TurnUiRequest;
use crate::states::{AppState, GamePhase};

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerHudState>()
            .add_systems(OnEnter(AppState::InGame), spawn_hud)
            .add_systems(
                Update,
                (
                    handle_player_hud_click,
                    update_player_hud_layout,
                    update_hud_content,
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

#[derive(Component)]
struct PlayerHudEntryText {
    player_id: u8,
}

#[derive(Component)]
struct PlayerHudPanel;

#[derive(Component)]
struct PlayerHudPanelText;

#[derive(Component)]
struct HudSkillButton {
    action: SkillUiAction,
}

#[derive(Component)]
struct HudRollButton;

#[derive(Component)]
struct HudRollButtonText;

#[derive(Resource, Default)]
pub struct PlayerHudState {
    active_player: Option<u8>,
}

type HudPieceQuery<'w, 's> = Query<'w, 's, (&'static PieceId, &'static PieceState)>;
type PlayerHudEntryLayoutQuery<'w, 's> =
    Query<'w, 's, (&'static PlayerHudEntry, &'static mut Node), Without<PlayerHudPanel>>;
type PlayerHudEntryStyleQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static PlayerHudEntry,
        &'static mut BackgroundColor,
        &'static mut BorderColor,
    ),
    (Without<HudSkillButton>, Without<HudRollButton>),
>;
type PlayerHudEntryTextQuery<'w, 's> = Query<
    'w,
    's,
    (&'static PlayerHudEntryText, &'static mut Text),
    (Without<PlayerHudPanelText>, Without<HudRollButtonText>),
>;
type PlayerHudPanelTextQuery<'w, 's> = Query<
    'w,
    's,
    &'static mut Text,
    (
        With<PlayerHudPanelText>,
        Without<PlayerHudEntryText>,
        Without<HudRollButtonText>,
    ),
>;
type HudSkillButtonQuery<'w, 's> = Query<
    'w,
    's,
    (&'static HudSkillButton, &'static mut BackgroundColor),
    (Without<PlayerHudEntry>, Without<HudRollButton>),
>;
type HudRollButtonQuery<'w, 's> = Query<
    'w,
    's,
    &'static mut BackgroundColor,
    (
        With<HudRollButton>,
        Without<PlayerHudEntry>,
        Without<HudSkillButton>,
    ),
>;
type HudRollButtonTextQuery<'w, 's> = Query<
    'w,
    's,
    &'static mut Text,
    (
        With<HudRollButtonText>,
        Without<PlayerHudEntryText>,
        Without<PlayerHudPanelText>,
    ),
>;

const HUD_EDGE_MARGIN: f32 = 10.0;
const HUD_ENTRY_W: f32 = 86.0;
const HUD_ENTRY_H: f32 = 48.0;
const HUD_PANEL_W: f32 = 312.0;
const HUD_PANEL_H: f32 = 338.0;
const HUD_PANEL_INSET: f32 = 14.0;
const HUD_PANEL_GAP: f32 = 12.0;
const HUD_SKILL_ROW_HEIGHT: f32 = 24.0;
const HUD_SKILL_ROW_GAP: f32 = 4.0;
const HUD_SKILL_ROW_START: f32 = 156.0;
const HUD_ROLL_BUTTON_TOP: f32 = 296.0;
const HUD_ROLL_BUTTON_H: f32 = 28.0;
const TOP_RIGHT_AUDIO_W: f32 = 108.0;
const TOP_RIGHT_AUDIO_H: f32 = 38.0;
const TOP_RIGHT_AUDIO_MARGIN: f32 = 16.0;
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

#[derive(Clone, Copy, Debug)]
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

    fn overlaps(self, other: Self) -> bool {
        self.x < other.x + other.w
            && self.x + self.w > other.x
            && self.y < other.y + other.h
            && self.y + self.h > other.y
    }
}

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

enum PlayerHudPanelAction {
    Roll,
    Skill(SkillUiAction),
}

fn spawn_hud(
    mut commands: Commands,
    mut hud_state: ResMut<PlayerHudState>,
    player_roster: Res<PlayerRoster>,
) {
    hud_state.active_player = None;

    for player in &player_roster.players {
        let player_id = player.state.player_id;
        commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(HUD_ENTRY_W),
                    height: Val::Px(HUD_ENTRY_H),
                    border: UiRect::all(Val::Px(2.0)),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    padding: UiRect::horizontal(Val::Px(6.0)),
                    ..default()
                },
                BackgroundColor(player.color.mix(&Color::WHITE, 0.68).with_alpha(0.90)),
                BorderColor::all(Color::srgba(0.10, 0.16, 0.24, 0.36)),
                ZIndex(32),
                Name::new(format!("PlayerHudEntryP{player_id}")),
                PlayerHudEntry { player_id },
                HudEntity,
            ))
            .with_children(|parent| {
                parent.spawn((
                    Text::new(""),
                    TextFont {
                        font_size: 14.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.08, 0.12, 0.18)),
                    TextLayout::new_with_justify(Justify::Center),
                    Name::new(format!("PlayerHudEntryTextP{player_id}")),
                    PlayerHudEntryText { player_id },
                ));
            });
    }

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(HUD_PANEL_W),
                height: Val::Px(HUD_PANEL_H),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.98, 0.99, 1.0, 0.97)),
            BorderColor::all(Color::srgba(0.34, 0.42, 0.55, 0.42)),
            ZIndex(40),
            Visibility::Hidden,
            Name::new("PlayerHudPanel"),
            PlayerHudPanel,
            HudEntity,
        ))
        .with_children(|panel| {
            panel.spawn((
                Text::new(""),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::srgb(0.10, 0.16, 0.24)),
                TextLayout::new_with_linebreak(LineBreak::WordOrCharacter),
                ZIndex(41),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(HUD_PANEL_INSET),
                    top: Val::Px(14.0),
                    width: Val::Px(HUD_PANEL_W - HUD_PANEL_INSET * 2.0),
                    ..default()
                },
                Name::new("PlayerHudPanelText"),
                PlayerHudPanelText,
            ));

            for action in HUD_SKILL_ACTIONS {
                spawn_panel_button(
                    panel,
                    panel_skill_button_rect(action),
                    skill_action_label(action),
                    Some(action),
                );
            }

            spawn_panel_button(panel, panel_roll_button_rect(), "Roll", None);
        });
}

fn spawn_panel_button(
    panel: &mut ChildSpawnerCommands<'_>,
    rect: ScreenRect,
    label: &str,
    action: Option<SkillUiAction>,
) {
    let mut entity = panel.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(rect.x),
            top: Val::Px(rect.y),
            width: Val::Px(rect.w),
            height: Val::Px(rect.h),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            padding: UiRect::horizontal(Val::Px(6.0)),
            ..default()
        },
        BackgroundColor(skill_button_color(false, false)),
        ZIndex(42),
        Name::new(format!("PlayerHudButton{label}")),
    ));
    if let Some(action) = action {
        entity.insert(HudSkillButton { action });
    } else {
        entity.insert(HudRollButton);
    }

    entity.with_children(|parent| {
        let mut text_entity = parent.spawn((
            Text::new(label),
            TextFont {
                font_size: 14.0,
                ..default()
            },
            TextColor(Color::srgb(0.09, 0.16, 0.24)),
            TextLayout::new_with_justify(Justify::Center),
            Name::new(format!("PlayerHudButtonLabel{label}")),
        ));
        if action.is_none() {
            text_entity.insert(HudRollButtonText);
        }
    });
}

fn update_player_hud_layout(
    windows: Query<&Window>,
    device_profile: Res<DeviceProfile>,
    hud_state: Res<PlayerHudState>,
    player_roster: Res<PlayerRoster>,
    mut entry_query: PlayerHudEntryLayoutQuery,
    mut panel_query: Query<(&mut Node, &mut Visibility), With<PlayerHudPanel>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let window_width = window.width();
    let window_height = window.height();

    for (entry, mut node) in &mut entry_query {
        let Some(player) = player_profile(&player_roster, entry.player_id) else {
            continue;
        };
        let rect = player_hud_entry_rect(window_width, window_height, *device_profile, player.seat);
        apply_rect_to_node(&mut node, rect);
    }

    let active_player = hud_state.active_player.filter(|player_id| {
        player_roster
            .players
            .iter()
            .any(|player| player.state.player_id == *player_id)
    });
    for (mut node, mut visibility) in &mut panel_query {
        if let Some(player_id) = active_player {
            let Some(player) = player_profile(&player_roster, player_id) else {
                *visibility = Visibility::Hidden;
                continue;
            };
            let rect =
                player_hud_panel_rect(window_width, window_height, *device_profile, player.seat);
            apply_rect_to_node(&mut node, rect);
            *visibility = Visibility::Visible;
        } else {
            *visibility = Visibility::Hidden;
        }
    }
}

fn update_hud_content(
    mut hud_state: ResMut<PlayerHudState>,
    match_config: Res<MatchConfig>,
    player_roster: Res<PlayerRoster>,
    skill_roster: Res<SkillRoster>,
    skill_target_state: Res<SkillTargetState>,
    input_state: Res<TurnInputState>,
    turn_state: Res<TurnState>,
    game_phase: Res<State<GamePhase>>,
    match_result: Res<MatchResult>,
    piece_query: HudPieceQuery,
    mut entry_style_query: PlayerHudEntryStyleQuery,
    mut entry_text_query: PlayerHudEntryTextQuery,
    mut panel_text_query: PlayerHudPanelTextQuery,
    mut skill_button_query: HudSkillButtonQuery,
    mut roll_button_query: HudRollButtonQuery,
    mut roll_button_text_query: HudRollButtonTextQuery,
) {
    if hud_state.active_player.is_some()
        && !player_roster
            .players
            .iter()
            .any(|player| Some(player.state.player_id) == hud_state.active_player)
    {
        hud_state.active_player = None;
    }

    for (entry, mut background, mut border) in &mut entry_style_query {
        let is_current = entry.player_id == turn_state.current_player;
        let is_active = hud_state.active_player == Some(entry.player_id);
        let color = player_roster
            .players
            .iter()
            .find(|player| player.state.player_id == entry.player_id)
            .map(|player| player.color)
            .unwrap_or(Color::srgb(0.78, 0.82, 0.89));
        *background = BackgroundColor(
            color
                .mix(&Color::WHITE, if is_current { 0.42 } else { 0.68 })
                .with_alpha(if is_active { 0.98 } else { 0.90 }),
        );
        *border = BorderColor::all(if is_active {
            Color::srgba(0.06, 0.10, 0.16, 0.94)
        } else if is_current {
            Color::srgba(0.12, 0.22, 0.32, 0.76)
        } else {
            Color::srgba(0.10, 0.16, 0.24, 0.32)
        });
    }

    for (entry_text, mut text) in &mut entry_text_query {
        let control = player_roster
            .players
            .iter()
            .find(|player| player.state.player_id == entry_text.player_id)
            .map(|player| control_label(player.state.control))
            .unwrap_or("-");
        let marker = if entry_text.player_id == turn_state.current_player {
            "*"
        } else {
            ""
        };
        *text = Text::new(format!("P{}{}\n{}", entry_text.player_id, marker, control));
    }

    let active_player_id = hud_state.active_player;
    let active_profile = active_player_id.and_then(|player_id| {
        player_roster
            .players
            .iter()
            .find(|player| player.state.player_id == player_id)
    });

    let mut roll_ready = false;
    let mut can_use_skill = false;
    let mut board_availability = SkillBoardAvailability::default();
    if let Some(player) = active_profile {
        let is_current_player = player.state.player_id == turn_state.current_player;
        let is_human_turn = is_current_player && player.state.control == PlayerControl::Human;
        roll_ready = is_human_turn
            && matches!(game_phase.get(), GamePhase::AwaitDice)
            && !match_result.finished;
        can_use_skill =
            is_human_turn && can_use_skill_this_turn(&skill_roster, player.state.player_id);
        if is_current_player {
            board_availability = SkillBoardAvailability::from_query(
                player.state.player_id,
                player.state.team_id,
                &piece_query,
            );
        }
    }

    for mut text in &mut panel_text_query {
        *text = Text::new(active_profile.map_or_else(String::new, |player| {
            format_player_panel_text(
                player.state.player_id,
                player.state.team_id,
                player.state.control,
                player.state.player_id == turn_state.current_player,
                game_phase.get(),
                player_skill_state(&skill_roster, player.state.player_id),
                panel_prompt_text(
                    player.state.player_id == turn_state.current_player,
                    player.state.control,
                    game_phase.get(),
                    &input_state,
                    &skill_target_state,
                ),
            )
        }));
    }

    let active_skills =
        active_player_id.and_then(|player_id| player_skill_state(&skill_roster, player_id));
    for (button, mut background) in &mut skill_button_query {
        let ready = active_skills
            .map(|skills| {
                is_skill_button_ready(
                    button.action,
                    skills,
                    can_use_skill,
                    game_phase.get(),
                    match_config.mode,
                    board_availability,
                )
            })
            .unwrap_or(false);
        *background = BackgroundColor(skill_button_color(ready, can_use_skill));
    }

    for mut background in &mut roll_button_query {
        *background = BackgroundColor(skill_button_color(roll_ready, active_profile.is_some()));
    }
    for mut text in &mut roll_button_text_query {
        *text = Text::new(if roll_ready { "Roll" } else { "Roll locked" });
    }
}

fn handle_player_hud_click(
    pointer: Res<PointerInputState>,
    windows: Query<&Window>,
    device_profile: Res<DeviceProfile>,
    overlay_state: Res<SoundSettingsOverlayState>,
    player_roster: Res<PlayerRoster>,
    game_phase: Res<State<GamePhase>>,
    match_result: Res<MatchResult>,
    turn_state: Res<TurnState>,
    mut hud_state: ResMut<PlayerHudState>,
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

    for player in &player_roster.players {
        let player_id = player.state.player_id;
        let rect = player_hud_entry_rect(
            window.width(),
            window.height(),
            *device_profile,
            player.seat,
        );
        if rect.contains(cursor) {
            hud_state.active_player = if hud_state.active_player == Some(player_id) {
                None
            } else {
                Some(player_id)
            };
            return;
        }
    }

    let Some(active_player_id) = hud_state.active_player else {
        return;
    };
    let Some(player) = player_roster
        .players
        .iter()
        .find(|player| player.state.player_id == active_player_id)
    else {
        return;
    };
    let panel_rect = player_hud_panel_rect(
        window.width(),
        window.height(),
        *device_profile,
        player.seat,
    );
    if !panel_rect.contains(cursor) {
        hud_state.active_player = None;
        return;
    }

    let is_actionable_turn = active_player_id == turn_state.current_player
        && player.state.control == PlayerControl::Human;
    if !is_actionable_turn {
        return;
    }

    let Some(action) = player_hud_panel_action_at(cursor, panel_rect) else {
        return;
    };
    match action {
        PlayerHudPanelAction::Roll => {
            if matches!(game_phase.get(), GamePhase::AwaitDice) {
                turn_ui_request.queue_roll();
            }
        }
        PlayerHudPanelAction::Skill(skill_action) => {
            if matches!(
                game_phase.get(),
                GamePhase::AwaitDice | GamePhase::AwaitPieceSelect
            ) {
                skill_ui_request.queue(skill_action);
            }
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
    for player in &player_roster.players {
        let rect =
            player_hud_entry_rect(window.width(), window.height(), device_profile, player.seat);
        if rect.contains(point) {
            return true;
        }
    }

    hud_state.active_player.is_some_and(|player_id| {
        player_profile(player_roster, player_id).is_some_and(|player| {
            player_hud_panel_rect(window.width(), window.height(), device_profile, player.seat)
                .contains(point)
        })
    })
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
    let size = (window_width.min(window_height) - device_profile.board_screen_padding()).max(240.0);
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
    let mut rect = match seat {
        PlayerSeat::Blue => ScreenRect {
            x: board.x + HUD_EDGE_MARGIN,
            y: board.y + HUD_EDGE_MARGIN,
            w: HUD_ENTRY_W,
            h: HUD_ENTRY_H,
        },
        PlayerSeat::Red => ScreenRect {
            x: board.x + board.w - HUD_ENTRY_W - HUD_EDGE_MARGIN,
            y: board.y + HUD_EDGE_MARGIN,
            w: HUD_ENTRY_W,
            h: HUD_ENTRY_H,
        },
        PlayerSeat::Green => ScreenRect {
            x: board.x + HUD_EDGE_MARGIN,
            y: board.y + board.h - HUD_ENTRY_H - HUD_EDGE_MARGIN,
            w: HUD_ENTRY_W,
            h: HUD_ENTRY_H,
        },
        PlayerSeat::Yellow => ScreenRect {
            x: board.x + board.w - HUD_ENTRY_W - HUD_EDGE_MARGIN,
            y: board.y + board.h - HUD_ENTRY_H - HUD_EDGE_MARGIN,
            w: HUD_ENTRY_W,
            h: HUD_ENTRY_H,
        },
    };
    rect = clamp_rect_to_window(rect, window_width, window_height);

    let audio = top_right_audio_rect(window_width);
    if seat == PlayerSeat::Red && rect.overlaps(audio) {
        rect.y = (audio.y + audio.h + HUD_EDGE_MARGIN)
            .min((window_height - rect.h - HUD_EDGE_MARGIN).max(HUD_EDGE_MARGIN));
    }
    rect
}

fn player_hud_panel_rect(
    window_width: f32,
    window_height: f32,
    device_profile: DeviceProfile,
    seat: PlayerSeat,
) -> ScreenRect {
    let board = gameplay_board_screen_rect(window_width, window_height, device_profile);
    let left_outside = board.x - HUD_PANEL_W - HUD_PANEL_GAP;
    let right_outside = board.x + board.w + HUD_PANEL_GAP;
    let x = match seat {
        PlayerSeat::Blue | PlayerSeat::Green if left_outside >= HUD_EDGE_MARGIN => left_outside,
        PlayerSeat::Blue | PlayerSeat::Green => board.x + HUD_PANEL_GAP,
        _ if right_outside + HUD_PANEL_W <= window_width - HUD_EDGE_MARGIN => right_outside,
        _ => board.x + board.w - HUD_PANEL_W - HUD_PANEL_GAP,
    };
    let y = match seat {
        PlayerSeat::Blue | PlayerSeat::Red => board.y + HUD_ENTRY_H + HUD_PANEL_GAP,
        PlayerSeat::Green | PlayerSeat::Yellow => {
            board.y + board.h - HUD_PANEL_H - HUD_ENTRY_H - HUD_PANEL_GAP
        }
    };
    clamp_rect_to_window(
        ScreenRect {
            x,
            y,
            w: HUD_PANEL_W,
            h: HUD_PANEL_H,
        },
        window_width,
        window_height,
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

fn top_right_audio_rect(window_width: f32) -> ScreenRect {
    ScreenRect {
        x: (window_width - TOP_RIGHT_AUDIO_W - TOP_RIGHT_AUDIO_MARGIN).max(TOP_RIGHT_AUDIO_MARGIN),
        y: TOP_RIGHT_AUDIO_MARGIN,
        w: TOP_RIGHT_AUDIO_W,
        h: TOP_RIGHT_AUDIO_H,
    }
}

fn apply_rect_to_node(node: &mut Node, rect: ScreenRect) {
    node.left = Val::Px(rect.x);
    node.top = Val::Px(rect.y);
    node.width = Val::Px(rect.w);
    node.height = Val::Px(rect.h);
}

fn panel_skill_button_rect(action: SkillUiAction) -> ScreenRect {
    let index = HUD_SKILL_ACTIONS
        .iter()
        .position(|candidate| *candidate == action)
        .unwrap_or_default() as f32;
    ScreenRect {
        x: HUD_PANEL_INSET,
        y: HUD_SKILL_ROW_START + index * (HUD_SKILL_ROW_HEIGHT + HUD_SKILL_ROW_GAP),
        w: HUD_PANEL_W - HUD_PANEL_INSET * 2.0,
        h: HUD_SKILL_ROW_HEIGHT,
    }
}

fn panel_roll_button_rect() -> ScreenRect {
    ScreenRect {
        x: HUD_PANEL_INSET,
        y: HUD_ROLL_BUTTON_TOP,
        w: HUD_PANEL_W - HUD_PANEL_INSET * 2.0,
        h: HUD_ROLL_BUTTON_H,
    }
}

fn player_hud_panel_action_at(
    cursor: Vec2,
    panel_rect: ScreenRect,
) -> Option<PlayerHudPanelAction> {
    let local = Vec2::new(cursor.x - panel_rect.x, cursor.y - panel_rect.y);
    if panel_roll_button_rect().contains(local) {
        return Some(PlayerHudPanelAction::Roll);
    }

    HUD_SKILL_ACTIONS.iter().find_map(|action| {
        panel_skill_button_rect(*action)
            .contains(local)
            .then_some(PlayerHudPanelAction::Skill(*action))
    })
}

fn control_label(control: PlayerControl) -> &'static str {
    match control {
        PlayerControl::Human => "Human",
        PlayerControl::Ai => "AI",
    }
}

fn skill_action_label(action: SkillUiAction) -> &'static str {
    match action {
        SkillUiAction::Dash => "Dash",
        SkillUiAction::Snipe => "Snipe",
        SkillUiAction::Swap => "Swap",
        SkillUiAction::Shield => "Shield",
        SkillUiAction::DoubleDice => "Double",
    }
}

fn panel_prompt_text(
    is_current_player: bool,
    control: PlayerControl,
    phase: &GamePhase,
    input_state: &TurnInputState,
    skill_target_state: &SkillTargetState,
) -> String {
    if !is_current_player {
        return "Waiting".to_string();
    }
    if control == PlayerControl::Ai {
        return "AI is playing".to_string();
    }

    let base = match phase {
        GamePhase::AwaitDice => "Ready to roll or use a skill",
        GamePhase::AwaitPieceSelect => "Choose a highlighted piece",
        GamePhase::ResolveSkillEffect => skill_target_state
            .prompt
            .as_deref()
            .unwrap_or("Choose a highlighted target"),
        _ => "Resolving action",
    };
    match candidate_piece_hint(input_state, skill_target_state) {
        Some(hint) => format!("{base}\n{hint}"),
        None => base.to_string(),
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

fn format_player_panel_text(
    player_id: u8,
    team_id: u8,
    control: PlayerControl,
    is_current_player: bool,
    phase: &GamePhase,
    skills: Option<&PlayerSkillState>,
    prompt: String,
) -> String {
    let turn_state = if is_current_player {
        match (control, phase) {
            (PlayerControl::Human, GamePhase::AwaitDice) => "Turn: ready",
            (PlayerControl::Human, GamePhase::AwaitPieceSelect) => "Turn: choose piece",
            (PlayerControl::Human, GamePhase::ResolveSkillEffect) => "Turn: choose target",
            (PlayerControl::Human, _) => "Turn: resolving",
            (PlayerControl::Ai, _) => "Turn: AI",
        }
    } else {
        "Turn: waiting"
    };
    let skill_summary = skills.map_or_else(
        || "Skills: unavailable".to_string(),
        |skills| {
            let armed = match (skills.dash_armed, skills.double_dice_armed) {
                (true, true) => " | Armed: Dash, Double",
                (true, false) => " | Armed: Dash",
                (false, true) => " | Armed: Double",
                (false, false) => "",
            };
            format!(
                "Skills: Dash {}  Snipe {}\nSwap {}  Shield {}  Double {}{}",
                skills.dash_charges,
                skills.snipe_charges,
                skills.swap_charges,
                skills.shield_charges,
                skills.double_dice_charges,
                armed
            )
        },
    );

    format!(
        "P{} {} | Team {}\n{}\n{}\n{}",
        player_id,
        control_label(control),
        team_id,
        turn_state,
        skill_summary,
        prompt
    )
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
        Color::srgba(0.76, 0.82, 0.89, 0.34)
    } else {
        Color::srgba(0.70, 0.73, 0.79, 0.18)
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
                        font_size: 34.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.10, 0.16, 0.24)),
                    TextLayout::new_with_justify(Justify::Center),
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
                        font_size: 24.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.14, 0.20, 0.30)),
                    TextLayout::new_with_justify(Justify::Center),
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
                    font_size: 18.0,
                    ..default()
                },
                TextColor(Color::srgb(0.10, 0.16, 0.24)),
                TextLayout::new_with_justify(Justify::Center),
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

    fn test_profile(width: f32, height: f32) -> DeviceProfile {
        DeviceProfile::from_window_size(width, height)
    }

    #[test]
    fn player_hud_entries_follow_centered_board_corners() {
        let profile = test_profile(1280.0, 720.0);
        let board = gameplay_board_screen_rect(1280.0, 720.0, profile);
        let p1 = player_hud_entry_rect(1280.0, 720.0, profile, PlayerSeat::Blue);
        let p2 = player_hud_entry_rect(1280.0, 720.0, profile, PlayerSeat::Red);
        let p3 = player_hud_entry_rect(1280.0, 720.0, profile, PlayerSeat::Green);
        let p4 = player_hud_entry_rect(1280.0, 720.0, profile, PlayerSeat::Yellow);

        assert!((p1.x - (board.x + HUD_EDGE_MARGIN)).abs() < f32::EPSILON);
        assert!((p1.y - (board.y + HUD_EDGE_MARGIN)).abs() < f32::EPSILON);
        assert!((p2.x + p2.w - (board.x + board.w - HUD_EDGE_MARGIN)).abs() < f32::EPSILON);
        assert!((p3.y + p3.h - (board.y + board.h - HUD_EDGE_MARGIN)).abs() < f32::EPSILON);
        assert!((p4.x + p4.w - (board.x + board.w - HUD_EDGE_MARGIN)).abs() < f32::EPSILON);
    }

    #[test]
    fn player_hud_entry_position_follows_player_seat() {
        let profile = test_profile(1280.0, 720.0);
        let board = gameplay_board_screen_rect(1280.0, 720.0, profile);
        let p1_red = player_hud_entry_rect(1280.0, 720.0, profile, PlayerSeat::Red);

        assert!((p1_red.x + p1_red.w - (board.x + board.w - HUD_EDGE_MARGIN)).abs() < f32::EPSILON);
        assert!((p1_red.y - (board.y + HUD_EDGE_MARGIN)).abs() < f32::EPSILON);
    }

    #[test]
    fn top_right_player_entry_does_not_cover_audio_entry() {
        let profile = test_profile(360.0, 640.0);
        let p2 = player_hud_entry_rect(360.0, 640.0, profile, PlayerSeat::Red);
        let audio = top_right_audio_rect(360.0);

        assert!(!p2.overlaps(audio));
    }

    #[test]
    fn player_hud_panel_stays_inside_common_windows() {
        for (width, height) in [(1280.0, 720.0), (2560.0, 1600.0), (640.0, 360.0)] {
            let profile = test_profile(width, height);
            for seat in PlayerSeat::ALL {
                let panel = player_hud_panel_rect(width, height, profile, seat);
                assert!(panel.x >= HUD_EDGE_MARGIN);
                assert!(panel.y >= HUD_EDGE_MARGIN);
                assert!(panel.x + panel.w <= width - HUD_EDGE_MARGIN || width < panel.w);
                assert!(panel.y + panel.h <= height - HUD_EDGE_MARGIN || height < panel.h);
            }
        }
    }

    #[test]
    fn panel_hit_targets_map_to_expected_actions() {
        let panel = ScreenRect {
            x: 100.0,
            y: 80.0,
            w: HUD_PANEL_W,
            h: HUD_PANEL_H,
        };
        let dash = panel_skill_button_rect(SkillUiAction::Dash);
        let roll = panel_roll_button_rect();

        assert!(matches!(
            player_hud_panel_action_at(
                Vec2::new(panel.x + dash.x + 4.0, panel.y + dash.y + 4.0),
                panel
            ),
            Some(PlayerHudPanelAction::Skill(SkillUiAction::Dash))
        ));
        assert!(matches!(
            player_hud_panel_action_at(
                Vec2::new(panel.x + roll.x + 4.0, panel.y + roll.y + 4.0),
                panel
            ),
            Some(PlayerHudPanelAction::Roll)
        ));
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
