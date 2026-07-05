use bevy::app::AppExit;
use bevy::prelude::*;

use crate::data::game_mode::GameMode;
use crate::data::rule_set::RuleSet;
use crate::domain::player::PlayerControl;
use crate::domain::rules::LaunchRule;
use crate::gameplay::ai::AiDifficulty;
use crate::gameplay::match_flow::{MatchSetup, PlayerSeat};
use crate::platform::PointerInputState;
use crate::plugins::audio_plugin::AudioSettings;
use crate::plugins::performance_plugin::{PerformanceSettings, fps_toggle_label};
use crate::states::AppState;

/// 菜单插件：主菜单与开局配置页的渲染和交互。
pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SoundSettingsOverlayState>()
            .init_resource::<ModeSelectRenderState>()
            .add_systems(Startup, spawn_global_sound_overlay)
            .add_systems(PreUpdate, update_sound_overlay_input_capture)
            .add_systems(
                Update,
                (
                    update_global_sound_overlay,
                    handle_global_sound_overlay_input,
                    handle_global_sound_overlay_click,
                ),
            )
            .add_systems(OnEnter(AppState::MainMenu), spawn_main_menu)
            .add_systems(OnEnter(AppState::ModeSelect), spawn_mode_select)
            .add_systems(
                Update,
                (
                    update_main_menu_layout.run_if(in_state(AppState::MainMenu)),
                    handle_main_menu_input.run_if(in_state(AppState::MainMenu)),
                    handle_main_menu_click.run_if(in_state(AppState::MainMenu)),
                    update_mode_select_option_visuals.run_if(in_state(AppState::ModeSelect)),
                    handle_mode_select_input.run_if(in_state(AppState::ModeSelect)),
                    handle_mode_select_click.run_if(in_state(AppState::ModeSelect)),
                    refresh_mode_select_layout.run_if(in_state(AppState::ModeSelect)),
                ),
            )
            .add_systems(OnExit(AppState::MainMenu), cleanup_menu)
            .add_systems(OnExit(AppState::ModeSelect), cleanup_menu);
    }
}

#[derive(Resource, Default)]
/// 全局声音弹窗状态；用于所有页面共享音量入口并阻止下层误点。
pub struct SoundSettingsOverlayState {
    pub open: bool,
    pub input_captured: bool,
}

pub fn sound_settings_overlay_blocks_input(overlay_state: &SoundSettingsOverlayState) -> bool {
    overlay_state.open || overlay_state.input_captured
}

#[derive(Resource, Default)]
/// 配置页当前已渲染的布局 key；模式或窗口尺寸变化后用于重建 UI。
struct ModeSelectRenderState {
    key: Option<ModeSelectRenderKey>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ModeSelectRenderKey {
    mode: GameMode,
    active_player_count: usize,
    window_width: u32,
    window_height: u32,
}

#[derive(Component)]
/// 菜单实体分组标记。
struct MenuEntity;

#[derive(Component)]
/// 常驻声音入口实体。
struct GlobalSoundEntry;

#[derive(Component)]
/// 全局声音弹窗实体。
struct GlobalSoundModal;

#[derive(Component)]
/// 全局声音设置 UI 实体分组。
struct GlobalSoundEntity;

#[derive(Component)]
/// 主菜单开始按钮点击区域标记。
struct MainMenuStartArea;

#[derive(Component)]
/// 主菜单标题节点标记。
struct MainMenuTitleNode;

#[derive(Component)]
/// 主菜单开始按钮视觉节点标记。
struct MainMenuStartButton;

#[derive(Component)]
/// 声音设置页摘要文本节点。
struct SoundSettingsText;

#[derive(Clone, Copy, Component, Debug, Eq, PartialEq)]
enum SoundSettingsValueKind {
    Music,
    Effects,
    Mute,
    Fps,
}

#[derive(Component)]
/// 声音设置页百分比文本节点。
struct SoundSettingsValueText {
    kind: SoundSettingsValueKind,
}

#[derive(Clone, Copy, Component, Debug, Eq, PartialEq)]
struct SoundSettingsToggleTrack {
    kind: SoundSettingsValueKind,
}

#[derive(Clone, Copy, Component, Debug, Eq, PartialEq)]
struct SoundSettingsToggleThumb {
    kind: SoundSettingsValueKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SettingsToggleVisualState {
    active: bool,
    label: &'static str,
}

#[derive(Clone, Copy, Component, Debug)]
/// 通用点击矩形（屏幕坐标）。
struct ClickRect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl ClickRect {
    /// 判断点击点是否落在当前矩形区域内。
    fn contains(self, point: Vec2) -> bool {
        point.x >= self.x
            && point.x <= self.x + self.w
            && point.y >= self.y
            && point.y <= self.y + self.h
    }
}

#[derive(Clone, Copy, Component, Debug, Eq, PartialEq)]
enum ModeSelectAction {
    SetMode(GameMode),
    SetRuleSet(RuleSet),
    SetPlayerSeat {
        player_index: usize,
        seat: PlayerSeat,
    },
    SetPieces(u8),
    SetLaunchRule(LaunchRule),
    SetAiDifficulty(AiDifficulty),
    SetPlayerControl {
        player_index: usize,
        control: PlayerControl,
    },
    StartMatch,
    Back,
}

#[derive(Component)]
/// 配置项节点元数据（动作 + 基础色）。
struct ModeSelectOption {
    action: ModeSelectAction,
    base_color: Color,
}

#[derive(Component)]
/// 配置项文字节点；颜色随选中态同步。
struct ModeSelectOptionLabel;

#[derive(Clone, Copy, Component, Debug, Eq, PartialEq)]
enum SoundSettingsAction {
    MusicDown,
    MusicUp,
    EffectsDown,
    EffectsUp,
    ToggleMute,
    ToggleFps,
    MainMenu,
    QuitGame,
    Back,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GlobalSettingsCommand {
    None,
    MainMenu,
    QuitGame,
}

#[derive(Component)]
/// 声音设置项元数据。
struct SoundSettingsOption {
    action: SoundSettingsAction,
}

type SoundSettingsValueQuery<'w, 's> = Query<
    'w,
    's,
    (&'static SoundSettingsValueText, &'static mut Text),
    (With<SoundSettingsValueText>, Without<SoundSettingsText>),
>;

const MENU_LEFT: f32 = 96.0;
const MAIN_TITLE_WIDTH: f32 = 620.0;
const MAIN_MENU_BLOCK_HEIGHT: f32 = 204.0;
const MAIN_START_TOP_IN_BLOCK: f32 = 122.0;
const MAIN_START_WIDTH: f32 = 360.0;
const MAIN_START_HEIGHT: f32 = 62.0;
const MAIN_BUTTON_GAP: f32 = 22.0;

const GLOBAL_SOUND_ENTRY_LEFT: f32 = 16.0;
const GLOBAL_SOUND_ENTRY_TOP: f32 = 16.0;
const GLOBAL_SOUND_ENTRY_W: f32 = 128.0;
const GLOBAL_SOUND_ENTRY_H: f32 = 38.0;
const GLOBAL_SOUND_PANEL_W: f32 = 462.0;
const GLOBAL_SOUND_PANEL_H: f32 = 448.0;
const GLOBAL_SOUND_ROW_LEFT: f32 = 34.0;
const GLOBAL_SOUND_CONTROL_LEFT: f32 = 244.0;
const GLOBAL_SOUND_ROW_TOP: f32 = 98.0;
const GLOBAL_SOUND_ROW_GAP: f32 = 58.0;
const GLOBAL_SOUND_MUTE_ROW_TOP: f32 = GLOBAL_SOUND_ROW_TOP + GLOBAL_SOUND_ROW_GAP * 2.0;
const GLOBAL_SOUND_FPS_ROW_TOP: f32 = GLOBAL_SOUND_ROW_TOP + GLOBAL_SOUND_ROW_GAP * 3.0;
const GLOBAL_SOUND_BUTTON: f32 = 42.0;
const GLOBAL_SOUND_VALUE_W: f32 = 82.0;
const GLOBAL_SOUND_TOGGLE_W: f32 = 176.0;
const GLOBAL_SETTINGS_ACTION_TOP: f32 = 362.0;
const GLOBAL_SETTINGS_ACTION_W: f32 = 160.0;
const GLOBAL_SETTINGS_ACTION_H: f32 = 44.0;
const GLOBAL_SETTINGS_ACTION_GAP: f32 = 18.0;

const SOUND_PANEL_TOP: f32 = 170.0;
const SOUND_ROW_GAP: f32 = 92.0;
const SOUND_CONTROL_LEFT: f32 = 430.0;
const SOUND_BUTTON: f32 = 52.0;
const SOUND_VALUE_W: f32 = 100.0;
const SOUND_MUTE_TOP: f32 = SOUND_PANEL_TOP + SOUND_ROW_GAP * 2.0;
const SOUND_TOGGLE_W: f32 = 184.0;
const SOUND_BACK_TOP: f32 = 488.0;

const SETTINGS_TOGGLE_TRACK_W: f32 = 64.0;
const SETTINGS_TOGGLE_TRACK_H: f32 = 34.0;
const SETTINGS_TOGGLE_THUMB: f32 = 26.0;
const SETTINGS_TOGGLE_PADDING: f32 = 4.0;
const SETTINGS_TOGGLE_TEXT_GAP: f32 = 14.0;
const SETTINGS_TOGGLE_ACTIVE_TRACK: Color = Color::srgb(0.14, 0.39, 0.82);
const SETTINGS_TOGGLE_ACTIVE_BORDER: Color = Color::srgba(0.08, 0.23, 0.55, 0.24);
const SETTINGS_TOGGLE_INACTIVE_TRACK: Color = Color::srgb(0.78, 0.82, 0.88);
const SETTINGS_TOGGLE_INACTIVE_BORDER: Color = Color::srgba(0.25, 0.31, 0.40, 0.18);
const SETTINGS_TOGGLE_THUMB_COLOR: Color = Color::srgb(1.0, 1.0, 1.0);
const SETTINGS_TOGGLE_THUMB_BORDER: Color = Color::srgba(0.12, 0.18, 0.28, 0.16);

const SECTION_LABEL_X: f32 = 96.0;
const OPTION_LEFT: f32 = 336.0;
const OPTION_W: f32 = 112.0;
const RULE_SET_OPTION_W: f32 = 168.0;
const OPTION_H: f32 = 36.0;
const OPTION_GAP: f32 = 12.0;
const OPTION_LABEL_SAFETY_PX: f32 = 8.0;
const MODE_ROW_TOP: f32 = 72.0;
const RULE_SET_ROW_TOP: f32 = MODE_ROW_TOP + SETTING_ROW_GAP;
const PLAYER_ROW_START_TOP: f32 = RULE_SET_ROW_TOP + SETTING_ROW_GAP;
const PLAYER_ROW_GAP: f32 = 48.0;
const PLAYER_COLOR_LEFT: f32 = 250.0;
const PLAYER_CONTROL_LEFT: f32 = 486.0;
const PLAYER_CONTROL_W: f32 = 92.0;
const PLAYER_CONTROL_GAP: f32 = 10.0;
const PLAYER_SETTINGS_GAP: f32 = 26.0;
const SETTING_ROW_GAP: f32 = 48.0;
const COLOR_SWATCH_W: f32 = 46.0;
const COLOR_SWATCH_H: f32 = 32.0;
const MODE_LAYOUT_BASE_LEFT: f32 = MENU_LEFT;
const MODE_LAYOUT_BASE_TOP: f32 = MODE_ROW_TOP;
const SETTING_ROW_BAND_LEFT: f32 = 72.0;
const SETTING_ROW_BAND_W: f32 = 666.0;
const SETTING_ROW_BAND_H: f32 = 40.0;
const MODE_LAYOUT_VISIBLE_LEFT: f32 = SETTING_ROW_BAND_LEFT;
const MODE_LAYOUT_VISIBLE_W: f32 = SETTING_ROW_BAND_W;
const BOTTOM_ACTION_W: f32 = 150.0;
const BOTTOM_ACTION_H: f32 = OPTION_H + 6.0;
const MODE_SELECT_BLACK: Color = Color::BLACK;
const MODE_SELECT_UNSELECTED_TEXT: Color = Color::srgb(0.18, 0.24, 0.34);
const MODE_SELECT_DISABLED_TEXT: Color = Color::srgba(0.18, 0.24, 0.34, 0.42);

fn spawn_global_sound_overlay(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(GLOBAL_SOUND_ENTRY_LEFT),
                top: Val::Px(GLOBAL_SOUND_ENTRY_TOP),
                width: Val::Px(GLOBAL_SOUND_ENTRY_W),
                height: Val::Px(GLOBAL_SOUND_ENTRY_H),
                border: UiRect::all(Val::Px(1.0)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                padding: UiRect::horizontal(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.97, 0.99, 1.0, 0.94)),
            BorderColor::all(Color::srgba(0.22, 0.30, 0.42, 0.30)),
            ZIndex(80),
            Visibility::Hidden,
            Name::new("GlobalSoundEntry"),
            GlobalSoundEntry,
            GlobalSoundEntity,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("Settings"),
                TextFont {
                    font_size: FontSize::Px(16.0),
                    ..default()
                },
                TextColor(Color::srgb(0.10, 0.16, 0.24)),
                TextLayout::justify(Justify::Center),
                Name::new("GlobalSoundEntryLabel"),
            ));
        });

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
            BackgroundColor(Color::srgba(0.05, 0.07, 0.10, 0.34)),
            ZIndex(90),
            Visibility::Hidden,
            Name::new("GlobalSoundModal"),
            GlobalSoundModal,
            GlobalSoundEntity,
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    Node {
                        width: Val::Px(GLOBAL_SOUND_PANEL_W),
                        height: Val::Px(GLOBAL_SOUND_PANEL_H),
                        border: UiRect::all(Val::Px(1.0)),
                        position_type: PositionType::Relative,
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.98, 0.99, 1.0, 0.98)),
                    BorderColor::all(Color::srgba(0.34, 0.42, 0.55, 0.42)),
                    ZIndex(91),
                    Name::new("GlobalSoundPanel"),
                ))
                .with_children(|panel| {
                    panel.spawn((
                        Text::new("Settings"),
                        TextFont {
                            font_size: FontSize::Px(28.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.10, 0.16, 0.24)),
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(GLOBAL_SOUND_ROW_LEFT),
                            top: Val::Px(24.0),
                            ..default()
                        },
                        Name::new("GlobalSoundTitle"),
                    ));
                    panel.spawn((
                        Text::new("Audio"),
                        TextFont {
                            font_size: FontSize::Px(18.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.16, 0.22, 0.32)),
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(GLOBAL_SOUND_ROW_LEFT),
                            top: Val::Px(68.0),
                            ..default()
                        },
                        Name::new("GlobalSoundAudioSection"),
                    ));

                    spawn_global_sound_row(
                        panel,
                        "Music",
                        SoundSettingsValueKind::Music,
                        GLOBAL_SOUND_ROW_TOP,
                    );
                    spawn_global_sound_row(
                        panel,
                        "Effects",
                        SoundSettingsValueKind::Effects,
                        GLOBAL_SOUND_ROW_TOP + GLOBAL_SOUND_ROW_GAP,
                    );
                    spawn_global_sound_toggle_row(
                        panel,
                        "Mute",
                        SoundSettingsValueKind::Mute,
                        GLOBAL_SOUND_MUTE_ROW_TOP,
                    );
                    spawn_global_sound_toggle_row(
                        panel,
                        "FPS Counter",
                        SoundSettingsValueKind::Fps,
                        GLOBAL_SOUND_FPS_ROW_TOP,
                    );

                    spawn_global_sound_panel_button(
                        panel,
                        global_settings_main_menu_rect(),
                        "Main Menu",
                        18.0,
                    );
                    spawn_global_sound_panel_button(
                        panel,
                        global_settings_quit_game_rect(),
                        "Quit Game",
                        18.0,
                    );
                });
        });
}

fn spawn_global_sound_toggle_row(
    panel: &mut ChildSpawnerCommands<'_>,
    label: &str,
    value_kind: SoundSettingsValueKind,
    top: f32,
) {
    panel.spawn((
        Text::new(label),
        TextFont {
            font_size: FontSize::Px(20.0),
            ..default()
        },
        TextColor(Color::srgb(0.10, 0.16, 0.24)),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(GLOBAL_SOUND_ROW_LEFT),
            top: Val::Px(top + 10.0),
            ..default()
        },
        Name::new(format!("GlobalSoundLabel{label}")),
    ));

    let rect = ClickRect {
        x: GLOBAL_SOUND_CONTROL_LEFT,
        y: top,
        w: GLOBAL_SOUND_TOGGLE_W,
        h: GLOBAL_SOUND_BUTTON,
    };
    let state = global_sound_toggle_initial_state(value_kind);
    panel
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(rect.x),
                top: Val::Px(rect.y),
                width: Val::Px(rect.w),
                height: Val::Px(rect.h),
                ..default()
            },
            Name::new(format!("GlobalSoundToggle{label}")),
        ))
        .with_children(|toggle| {
            spawn_settings_toggle_visual(
                toggle,
                value_kind,
                state,
                rect.w,
                rect.h,
                18.0,
                &format!("GlobalSound{label}"),
            );
        });
}

fn global_sound_toggle_initial_state(
    value_kind: SoundSettingsValueKind,
) -> SettingsToggleVisualState {
    match value_kind {
        SoundSettingsValueKind::Mute => SettingsToggleVisualState {
            active: false,
            label: "Sound On",
        },
        SoundSettingsValueKind::Fps => SettingsToggleVisualState {
            active: cfg!(debug_assertions),
            label: if cfg!(debug_assertions) {
                "FPS On"
            } else {
                "FPS Off"
            },
        },
        SoundSettingsValueKind::Music | SoundSettingsValueKind::Effects => {
            SettingsToggleVisualState {
                active: false,
                label: "",
            }
        }
    }
}

fn spawn_global_sound_row(
    panel: &mut ChildSpawnerCommands<'_>,
    label: &str,
    value_kind: SoundSettingsValueKind,
    top: f32,
) {
    panel.spawn((
        Text::new(label),
        TextFont {
            font_size: FontSize::Px(20.0),
            ..default()
        },
        TextColor(Color::srgb(0.10, 0.16, 0.24)),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(GLOBAL_SOUND_ROW_LEFT),
            top: Val::Px(top + 10.0),
            ..default()
        },
        Name::new(format!("GlobalSoundLabel{label}")),
    ));

    spawn_global_sound_panel_button(
        panel,
        ClickRect {
            x: GLOBAL_SOUND_CONTROL_LEFT,
            y: top,
            w: GLOBAL_SOUND_BUTTON,
            h: GLOBAL_SOUND_BUTTON,
        },
        "-",
        24.0,
    );

    panel.spawn((
        Text::new("  0%"),
        TextFont {
            font_size: FontSize::Px(22.0),
            ..default()
        },
        TextColor(Color::srgb(0.10, 0.16, 0.24)),
        TextLayout::justify(Justify::Center),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(GLOBAL_SOUND_CONTROL_LEFT + GLOBAL_SOUND_BUTTON + 10.0),
            top: Val::Px(top + 8.0),
            width: Val::Px(GLOBAL_SOUND_VALUE_W),
            ..default()
        },
        SoundSettingsValueText { kind: value_kind },
        Name::new("GlobalSoundValue"),
    ));

    spawn_global_sound_panel_button(
        panel,
        ClickRect {
            x: GLOBAL_SOUND_CONTROL_LEFT + GLOBAL_SOUND_BUTTON + GLOBAL_SOUND_VALUE_W + 20.0,
            y: top,
            w: GLOBAL_SOUND_BUTTON,
            h: GLOBAL_SOUND_BUTTON,
        },
        "+",
        24.0,
    );
}

fn spawn_global_sound_panel_button(
    panel: &mut ChildSpawnerCommands<'_>,
    rect: ClickRect,
    label: &str,
    font_size: f32,
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
                ..default()
            },
            BackgroundColor(Color::srgba(0.55, 0.70, 0.88, 0.22)),
            BorderColor::all(Color::srgba(0.22, 0.30, 0.42, 0.24)),
            Name::new(format!("GlobalSoundButton{label}")),
        ))
        .with_children(|button| {
            button.spawn((
                Text::new(label),
                TextFont {
                    font_size: FontSize::Px(font_size),
                    ..default()
                },
                TextColor(Color::srgb(0.10, 0.16, 0.24)),
                TextLayout::justify(Justify::Center),
                Name::new("GlobalSoundButtonLabel"),
            ));
        });
}

fn update_sound_overlay_input_capture(
    mut overlay_state: ResMut<SoundSettingsOverlayState>,
    keyboard: Res<ButtonInput<KeyCode>>,
    pointer: Res<PointerInputState>,
    windows: Query<&Window>,
) {
    overlay_state.input_captured = false;

    if overlay_state.open
        && (pointer.just_pressed()
            || keyboard.just_pressed(KeyCode::Escape)
            || keyboard.just_pressed(KeyCode::Backspace))
    {
        overlay_state.input_captured = true;
        return;
    }

    let Some(cursor) = pointer.just_pressed_position() else {
        return;
    };
    let Ok(window) = windows.single() else {
        return;
    };

    overlay_state.input_captured = global_sound_entry_rect(window).contains(cursor);
}

fn update_global_sound_overlay(
    app_state: Res<State<AppState>>,
    audio_settings: Res<AudioSettings>,
    performance_settings: Res<PerformanceSettings>,
    overlay_state: Res<SoundSettingsOverlayState>,
    mut entry_query: Query<
        (&mut Node, &mut Visibility),
        (With<GlobalSoundEntry>, Without<GlobalSoundModal>),
    >,
    mut modal_query: Query<&mut Visibility, (With<GlobalSoundModal>, Without<GlobalSoundEntry>)>,
    mut value_query: Query<(&SoundSettingsValueText, &mut Text)>,
    mut toggle_track_query: Query<(
        &SoundSettingsToggleTrack,
        &mut BackgroundColor,
        &mut BorderColor,
    )>,
    mut toggle_thumb_query: Query<
        (&SoundSettingsToggleThumb, &mut Node),
        Without<GlobalSoundEntry>,
    >,
) {
    let visible_on_page = !matches!(app_state.get(), AppState::Boot);
    let entry_visibility = if visible_on_page {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    for (mut node, mut visibility) in &mut entry_query {
        apply_global_sound_entry_position(&mut node);
        *visibility = entry_visibility;
    }

    let modal_visibility = if visible_on_page && overlay_state.open {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    for mut visibility in &mut modal_query {
        *visibility = modal_visibility;
    }

    if !audio_settings.is_changed()
        && !performance_settings.is_changed()
        && !overlay_state.is_changed()
    {
        return;
    }
    for (value_text, mut text) in &mut value_query {
        *text = Text::new(match value_text.kind {
            SoundSettingsValueKind::Music => format_volume_percent(audio_settings.music_volume),
            SoundSettingsValueKind::Effects => format_volume_percent(audio_settings.effects_volume),
            SoundSettingsValueKind::Mute | SoundSettingsValueKind::Fps => {
                global_settings_toggle_state(
                    value_text.kind,
                    &audio_settings,
                    &performance_settings,
                )
                .map_or_else(String::new, |state| state.label.to_owned())
            }
        });
    }
    for (track, mut background, mut border) in &mut toggle_track_query {
        let Some(state) =
            global_settings_toggle_state(track.kind, &audio_settings, &performance_settings)
        else {
            continue;
        };
        *background = BackgroundColor(settings_toggle_track_color(state.active));
        *border = BorderColor::all(settings_toggle_track_border_color(state.active));
    }
    for (thumb, mut node) in &mut toggle_thumb_query {
        let Some(state) =
            global_settings_toggle_state(thumb.kind, &audio_settings, &performance_settings)
        else {
            continue;
        };
        node.left = Val::Px(settings_toggle_thumb_left(state.active));
    }
}

fn handle_global_sound_overlay_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut overlay_state: ResMut<SoundSettingsOverlayState>,
) {
    if !overlay_state.open {
        return;
    }
    if keyboard.just_pressed(KeyCode::Escape) || keyboard.just_pressed(KeyCode::Backspace) {
        overlay_state.input_captured = true;
        overlay_state.open = false;
    }
}

fn handle_global_sound_overlay_click(
    pointer: Res<PointerInputState>,
    windows: Query<&Window>,
    mut audio_settings: ResMut<AudioSettings>,
    mut performance_settings: ResMut<PerformanceSettings>,
    mut overlay_state: ResMut<SoundSettingsOverlayState>,
    mut next_app_state: ResMut<NextState<AppState>>,
    mut app_exit: MessageWriter<AppExit>,
) {
    let Some(cursor) = pointer.just_pressed_position() else {
        return;
    };
    let Ok(window) = windows.single() else {
        return;
    };

    if overlay_state.open {
        overlay_state.input_captured = true;
        if let Some(action) = global_sound_action_at(cursor, window) {
            match apply_global_sound_action(
                action,
                &mut audio_settings,
                &mut performance_settings,
                &mut overlay_state,
            ) {
                GlobalSettingsCommand::None => {}
                GlobalSettingsCommand::MainMenu => next_app_state.set(AppState::MainMenu),
                GlobalSettingsCommand::QuitGame => {
                    app_exit.write(AppExit::Success);
                }
            }
            return;
        }

        if !global_sound_panel_rect(window).contains(cursor) {
            overlay_state.open = false;
        }
        return;
    }

    if global_sound_entry_rect(window).contains(cursor) {
        overlay_state.open = true;
        overlay_state.input_captured = true;
    }
}

fn apply_global_sound_action(
    action: SoundSettingsAction,
    audio_settings: &mut AudioSettings,
    performance_settings: &mut PerformanceSettings,
    overlay_state: &mut SoundSettingsOverlayState,
) -> GlobalSettingsCommand {
    match action {
        SoundSettingsAction::MusicDown => audio_settings.adjust_music(-AudioSettings::MUSIC_STEP),
        SoundSettingsAction::MusicUp => audio_settings.adjust_music(AudioSettings::MUSIC_STEP),
        SoundSettingsAction::EffectsDown => {
            audio_settings.adjust_effects(-AudioSettings::EFFECTS_STEP)
        }
        SoundSettingsAction::EffectsUp => {
            audio_settings.adjust_effects(AudioSettings::EFFECTS_STEP)
        }
        SoundSettingsAction::ToggleMute => audio_settings.toggle_mute(),
        SoundSettingsAction::ToggleFps => performance_settings.toggle_fps(),
        SoundSettingsAction::MainMenu => {
            overlay_state.open = false;
            return GlobalSettingsCommand::MainMenu;
        }
        SoundSettingsAction::QuitGame => {
            overlay_state.open = false;
            return GlobalSettingsCommand::QuitGame;
        }
        SoundSettingsAction::Back => overlay_state.open = false,
    }
    GlobalSettingsCommand::None
}

fn apply_global_sound_entry_position(node: &mut Node) {
    node.left = Val::Auto;
    node.right = Val::Px(GLOBAL_SOUND_ENTRY_LEFT);
    node.top = Val::Px(GLOBAL_SOUND_ENTRY_TOP);
}

fn global_sound_entry_rect(window: &Window) -> ClickRect {
    let (x, y, w, h) = global_settings_entry_screen_rect(window.width());
    ClickRect { x, y, w, h }
}

pub fn global_settings_entry_screen_rect(window_width: f32) -> (f32, f32, f32, f32) {
    (
        (window_width - GLOBAL_SOUND_ENTRY_W - GLOBAL_SOUND_ENTRY_LEFT)
            .max(GLOBAL_SOUND_ENTRY_LEFT),
        GLOBAL_SOUND_ENTRY_TOP,
        GLOBAL_SOUND_ENTRY_W,
        GLOBAL_SOUND_ENTRY_H,
    )
}

fn global_sound_panel_rect(window: &Window) -> ClickRect {
    ClickRect {
        x: (window.width() - GLOBAL_SOUND_PANEL_W) * 0.5,
        y: (window.height() - GLOBAL_SOUND_PANEL_H) * 0.5,
        w: GLOBAL_SOUND_PANEL_W,
        h: GLOBAL_SOUND_PANEL_H,
    }
}

fn global_settings_action_start_x() -> f32 {
    (GLOBAL_SOUND_PANEL_W - GLOBAL_SETTINGS_ACTION_W * 2.0 - GLOBAL_SETTINGS_ACTION_GAP) * 0.5
}

fn global_settings_main_menu_rect() -> ClickRect {
    ClickRect {
        x: global_settings_action_start_x(),
        y: GLOBAL_SETTINGS_ACTION_TOP,
        w: GLOBAL_SETTINGS_ACTION_W,
        h: GLOBAL_SETTINGS_ACTION_H,
    }
}

fn global_settings_quit_game_rect() -> ClickRect {
    ClickRect {
        x: global_settings_action_start_x() + GLOBAL_SETTINGS_ACTION_W + GLOBAL_SETTINGS_ACTION_GAP,
        y: GLOBAL_SETTINGS_ACTION_TOP,
        w: GLOBAL_SETTINGS_ACTION_W,
        h: GLOBAL_SETTINGS_ACTION_H,
    }
}

fn global_sound_action_at(cursor: Vec2, window: &Window) -> Option<SoundSettingsAction> {
    let panel = global_sound_panel_rect(window);
    let local = Vec2::new(cursor.x - panel.x, cursor.y - panel.y);
    let actions = [
        (
            SoundSettingsAction::MusicDown,
            ClickRect {
                x: GLOBAL_SOUND_CONTROL_LEFT,
                y: GLOBAL_SOUND_ROW_TOP,
                w: GLOBAL_SOUND_BUTTON,
                h: GLOBAL_SOUND_BUTTON,
            },
        ),
        (
            SoundSettingsAction::MusicUp,
            ClickRect {
                x: GLOBAL_SOUND_CONTROL_LEFT + GLOBAL_SOUND_BUTTON + GLOBAL_SOUND_VALUE_W + 20.0,
                y: GLOBAL_SOUND_ROW_TOP,
                w: GLOBAL_SOUND_BUTTON,
                h: GLOBAL_SOUND_BUTTON,
            },
        ),
        (
            SoundSettingsAction::EffectsDown,
            ClickRect {
                x: GLOBAL_SOUND_CONTROL_LEFT,
                y: GLOBAL_SOUND_ROW_TOP + GLOBAL_SOUND_ROW_GAP,
                w: GLOBAL_SOUND_BUTTON,
                h: GLOBAL_SOUND_BUTTON,
            },
        ),
        (
            SoundSettingsAction::EffectsUp,
            ClickRect {
                x: GLOBAL_SOUND_CONTROL_LEFT + GLOBAL_SOUND_BUTTON + GLOBAL_SOUND_VALUE_W + 20.0,
                y: GLOBAL_SOUND_ROW_TOP + GLOBAL_SOUND_ROW_GAP,
                w: GLOBAL_SOUND_BUTTON,
                h: GLOBAL_SOUND_BUTTON,
            },
        ),
        (
            SoundSettingsAction::ToggleMute,
            ClickRect {
                x: GLOBAL_SOUND_CONTROL_LEFT,
                y: GLOBAL_SOUND_MUTE_ROW_TOP,
                w: GLOBAL_SOUND_TOGGLE_W,
                h: GLOBAL_SOUND_BUTTON,
            },
        ),
        (
            SoundSettingsAction::ToggleFps,
            ClickRect {
                x: GLOBAL_SOUND_CONTROL_LEFT,
                y: GLOBAL_SOUND_FPS_ROW_TOP,
                w: GLOBAL_SOUND_TOGGLE_W,
                h: GLOBAL_SOUND_BUTTON,
            },
        ),
        (
            SoundSettingsAction::MainMenu,
            global_settings_main_menu_rect(),
        ),
        (
            SoundSettingsAction::QuitGame,
            global_settings_quit_game_rect(),
        ),
    ];

    actions
        .iter()
        .find_map(|(action, rect)| rect.contains(local).then_some(*action))
}

fn spawn_main_menu(mut commands: Commands, windows: Query<&Window>) {
    // 主菜单：标题 + 开始按钮按当前窗口居中。
    let (window_width, window_height) = windows
        .single()
        .map(|window| (window.width(), window.height()))
        .unwrap_or((1280.0, 720.0));
    let title_rect = main_menu_title_rect(window_width, window_height);
    let start_rect = main_menu_start_rect(window_width, window_height);

    commands.spawn((
        Text::new("Aeroplane Chess"),
        TextFont {
            font_size: FontSize::Px(54.0),
            ..default()
        },
        TextColor(Color::srgb(0.10, 0.16, 0.24)),
        TextLayout::justify(Justify::Center),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(title_rect.y),
            left: Val::Px(title_rect.x),
            width: Val::Px(title_rect.w),
            ..default()
        },
        Name::new("MainMenuTitle"),
        MainMenuTitleNode,
        MenuEntity,
    ));

    let start_button = spawn_box_with_label(
        &mut commands,
        start_rect,
        Color::srgba(0.42, 0.61, 0.88, 0.30),
        "Start Match",
        30.0,
        None,
    );
    commands.entity(start_button).insert(MainMenuStartButton);

    commands.spawn((
        MainMenuStartArea,
        start_rect,
        Name::new("MainMenuStartArea"),
        MenuEntity,
    ));
}

fn main_menu_title_rect(window_width: f32, window_height: f32) -> ClickRect {
    let width = MAIN_TITLE_WIDTH.min((window_width - 32.0).max(280.0));
    ClickRect {
        x: centered_axis(window_width, width, 16.0),
        y: main_menu_block_top(window_height),
        w: width,
        h: 72.0,
    }
}

fn main_menu_start_rect(window_width: f32, window_height: f32) -> ClickRect {
    let width = MAIN_START_WIDTH.min((window_width - 32.0).max(240.0));
    ClickRect {
        x: centered_axis(window_width, width, 16.0),
        y: main_menu_block_top(window_height) + MAIN_START_TOP_IN_BLOCK,
        w: width,
        h: MAIN_START_HEIGHT,
    }
}

fn main_menu_block_top(window_height: f32) -> f32 {
    centered_axis(
        window_height,
        MAIN_MENU_BLOCK_HEIGHT,
        GLOBAL_SOUND_ENTRY_TOP + GLOBAL_SOUND_ENTRY_H + 24.0,
    )
}

fn centered_axis(container: f32, item: f32, min_margin: f32) -> f32 {
    if container <= item + min_margin * 2.0 {
        min_margin
    } else {
        (container - item) * 0.5
    }
}

#[derive(Clone, Copy)]
struct ModeSelectLayout {
    left: f32,
    top: f32,
    scale: f32,
}

impl ModeSelectLayout {
    fn rect(self, rect: ClickRect) -> ClickRect {
        ClickRect {
            x: self.x(rect.x),
            y: self.y(rect.y),
            w: rect.w * self.scale,
            h: rect.h * self.scale,
        }
    }

    fn size(self, value: f32) -> f32 {
        value * self.scale
    }

    fn font(self, value: f32) -> f32 {
        (value * self.scale).max(10.0)
    }

    fn border(self, value: f32) -> f32 {
        (value * self.scale).max(1.0)
    }

    fn x(self, x: f32) -> f32 {
        self.left + (x - MODE_LAYOUT_BASE_LEFT) * self.scale
    }

    fn y(self, y: f32) -> f32 {
        self.top + (y - MODE_LAYOUT_BASE_TOP) * self.scale
    }
}

fn mode_select_layout(
    window_width: f32,
    window_height: f32,
    active_player_count: usize,
) -> ModeSelectLayout {
    let layout_height = mode_select_layout_height(active_player_count);
    let scale = mode_select_layout_scale(window_width, window_height, active_player_count);
    ModeSelectLayout {
        left: mode_select_layout_left(window_width, scale),
        top: centered_axis(window_height, layout_height * scale, 16.0),
        scale,
    }
}

fn mode_select_layout_left(window_width: f32, scale: f32) -> f32 {
    centered_axis(window_width, MODE_LAYOUT_VISIBLE_W * scale, 16.0)
        + (MODE_LAYOUT_BASE_LEFT - MODE_LAYOUT_VISIBLE_LEFT) * scale
}

fn mode_select_layout_scale(
    window_width: f32,
    window_height: f32,
    active_player_count: usize,
) -> f32 {
    let layout_height = mode_select_layout_height(active_player_count);
    available_axis_scale(window_width, MODE_LAYOUT_VISIBLE_W, 16.0)
        .min(available_axis_scale(window_height, layout_height, 16.0))
        .min(1.0)
}

fn available_axis_scale(container: f32, item: f32, min_margin: f32) -> f32 {
    if item <= f32::EPSILON {
        return 1.0;
    }
    ((container - min_margin * 2.0).max(1.0) / item).min(1.0)
}

fn mode_select_render_key(
    windows: &Query<&Window>,
    match_setup: &MatchSetup,
) -> ModeSelectRenderKey {
    let (window_width, window_height) = windows
        .single()
        .map(|window| (window.width(), window.height()))
        .unwrap_or((1280.0, 720.0));
    mode_select_render_key_from_size(window_width, window_height, match_setup)
}

fn mode_select_render_key_from_size(
    window_width: f32,
    window_height: f32,
    match_setup: &MatchSetup,
) -> ModeSelectRenderKey {
    ModeSelectRenderKey {
        mode: match_setup.mode,
        active_player_count: match_setup.active_player_count(),
        window_width: window_width.round().max(0.0) as u32,
        window_height: window_height.round().max(0.0) as u32,
    }
}

fn player_row_top(player_index: usize) -> f32 {
    PLAYER_ROW_START_TOP + player_index as f32 * PLAYER_ROW_GAP
}

fn pieces_row_top(active_player_count: usize) -> f32 {
    PLAYER_ROW_START_TOP + active_player_count as f32 * PLAYER_ROW_GAP + PLAYER_SETTINGS_GAP
}

fn launch_rule_row_top(active_player_count: usize) -> f32 {
    pieces_row_top(active_player_count) + SETTING_ROW_GAP
}

fn ai_difficulty_row_top(active_player_count: usize) -> f32 {
    launch_rule_row_top(active_player_count) + SETTING_ROW_GAP
}

fn bottom_row_top(active_player_count: usize) -> f32 {
    ai_difficulty_row_top(active_player_count) + SETTING_ROW_GAP + 6.0
}

fn mode_select_layout_height(active_player_count: usize) -> f32 {
    bottom_row_top(active_player_count) + BOTTOM_ACTION_H + 6.0 - MODE_LAYOUT_BASE_TOP
}

fn update_main_menu_layout(
    windows: Query<&Window>,
    mut title_query: Query<&mut Node, (With<MainMenuTitleNode>, Without<MainMenuStartButton>)>,
    mut start_button_query: Query<
        &mut Node,
        (With<MainMenuStartButton>, Without<MainMenuTitleNode>),
    >,
    mut start_area_query: Query<&mut ClickRect, With<MainMenuStartArea>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let title_rect = main_menu_title_rect(window.width(), window.height());
    let start_rect = main_menu_start_rect(window.width(), window.height());

    for mut node in &mut title_query {
        node.left = Val::Px(title_rect.x);
        node.top = Val::Px(title_rect.y);
        node.width = Val::Px(title_rect.w);
    }
    for mut node in &mut start_button_query {
        node.left = Val::Px(start_rect.x);
        node.top = Val::Px(start_rect.y);
        node.width = Val::Px(start_rect.w);
        node.height = Val::Px(start_rect.h);
    }
    for mut rect in &mut start_area_query {
        *rect = start_rect;
    }
}

fn spawn_sound_settings(mut commands: Commands, audio_settings: Res<AudioSettings>) {
    commands.spawn((
        Text::new("Sound Settings"),
        TextFont {
            font_size: FontSize::Px(46.0),
            ..default()
        },
        TextColor(Color::srgb(0.10, 0.16, 0.24)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(96.0),
            left: Val::Px(MENU_LEFT),
            ..default()
        },
        Name::new("SoundSettingsTitle"),
        MenuEntity,
    ));

    commands.spawn((
        Text::new(sound_settings_content(&audio_settings)),
        TextFont {
            font_size: FontSize::Px(19.0),
            ..default()
        },
        TextColor(Color::srgb(0.16, 0.22, 0.32)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(146.0),
            left: Val::Px(MENU_LEFT),
            width: Val::Px(620.0),
            ..default()
        },
        Name::new("SoundSettingsText"),
        SoundSettingsText,
        MenuEntity,
    ));

    spawn_sound_row(
        &mut commands,
        "Background Music",
        SoundSettingsValueKind::Music,
        SoundSettingsAction::MusicDown,
        SoundSettingsAction::MusicUp,
        SOUND_PANEL_TOP,
        audio_settings.music_volume,
    );
    spawn_sound_row(
        &mut commands,
        "Action Effects",
        SoundSettingsValueKind::Effects,
        SoundSettingsAction::EffectsDown,
        SoundSettingsAction::EffectsUp,
        SOUND_PANEL_TOP + SOUND_ROW_GAP,
        audio_settings.effects_volume,
    );
    spawn_sound_toggle(&mut commands, SOUND_MUTE_TOP, &audio_settings);

    spawn_sound_option(
        &mut commands,
        SoundSettingsAction::Back,
        ClickRect {
            x: MENU_LEFT,
            y: SOUND_BACK_TOP,
            w: MAIN_START_WIDTH * 0.64,
            h: MAIN_START_HEIGHT,
        },
        "Back",
        Color::srgba(0.72, 0.54, 0.44, 0.28),
    );
}

fn spawn_sound_toggle(commands: &mut Commands, top: f32, audio_settings: &AudioSettings) {
    commands.spawn((
        Text::new("Mute"),
        TextFont {
            font_size: FontSize::Px(24.0),
            ..default()
        },
        TextColor(Color::srgb(0.10, 0.16, 0.24)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(top + 12.0),
            left: Val::Px(MENU_LEFT),
            ..default()
        },
        Name::new("SoundSettingLabelMute"),
        MenuEntity,
    ));

    let rect = ClickRect {
        x: SOUND_CONTROL_LEFT,
        y: top,
        w: SOUND_TOGGLE_W,
        h: SOUND_BUTTON,
    };
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(rect.x),
                top: Val::Px(rect.y),
                width: Val::Px(rect.w),
                height: Val::Px(rect.h),
                ..default()
            },
            ClickRect { ..rect },
            SoundSettingsOption {
                action: SoundSettingsAction::ToggleMute,
            },
            Name::new("SoundSettingsMuteToggle"),
            MenuEntity,
        ))
        .with_children(|parent| {
            spawn_settings_toggle_visual(
                parent,
                SoundSettingsValueKind::Mute,
                sound_settings_toggle_state(audio_settings),
                rect.w,
                rect.h,
                22.0,
                "SoundSettingsMute",
            );
        });
}

fn spawn_sound_row(
    commands: &mut Commands,
    label: &str,
    value_kind: SoundSettingsValueKind,
    down_action: SoundSettingsAction,
    up_action: SoundSettingsAction,
    top: f32,
    value: f32,
) {
    commands.spawn((
        Text::new(label),
        TextFont {
            font_size: FontSize::Px(24.0),
            ..default()
        },
        TextColor(Color::srgb(0.10, 0.16, 0.24)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(top + 12.0),
            left: Val::Px(MENU_LEFT),
            ..default()
        },
        Name::new(format!("SoundSettingLabel{label}")),
        MenuEntity,
    ));

    spawn_sound_option(
        commands,
        down_action,
        ClickRect {
            x: SOUND_CONTROL_LEFT,
            y: top,
            w: SOUND_BUTTON,
            h: SOUND_BUTTON,
        },
        "-",
        Color::srgba(0.53, 0.77, 0.96, 0.26),
    );

    commands.spawn((
        Text::new(format_volume_percent(value)),
        TextFont {
            font_size: FontSize::Px(26.0),
            ..default()
        },
        TextColor(Color::srgb(0.10, 0.16, 0.24)),
        TextLayout::justify(Justify::Center),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(top + 10.0),
            left: Val::Px(SOUND_CONTROL_LEFT + SOUND_BUTTON + 12.0),
            width: Val::Px(SOUND_VALUE_W),
            ..default()
        },
        SoundSettingsValueText { kind: value_kind },
        Name::new("SoundSettingValue"),
        MenuEntity,
    ));

    spawn_sound_option(
        commands,
        up_action,
        ClickRect {
            x: SOUND_CONTROL_LEFT + SOUND_BUTTON + SOUND_VALUE_W + 24.0,
            y: top,
            w: SOUND_BUTTON,
            h: SOUND_BUTTON,
        },
        "+",
        Color::srgba(0.53, 0.77, 0.96, 0.26),
    );
}

fn spawn_mode_select(
    mut commands: Commands,
    windows: Query<&Window>,
    match_setup: Res<MatchSetup>,
    mut render_state: ResMut<ModeSelectRenderState>,
) {
    spawn_mode_select_content(&mut commands, &windows, &match_setup);
    render_state.key = Some(mode_select_render_key(&windows, &match_setup));
}

fn spawn_mode_select_content(
    commands: &mut Commands,
    windows: &Query<&Window>,
    match_setup: &MatchSetup,
) {
    // 对局配置页：按“模式/玩家配置/规则/开始返回”分区渲染。
    let (window_width, window_height) = windows
        .single()
        .map(|window| (window.width(), window.height()))
        .unwrap_or((1280.0, 720.0));
    let active_player_count = match_setup.active_player_count();
    let layout = mode_select_layout(window_width, window_height, active_player_count);

    spawn_section_label(commands, layout, "Mode", MODE_ROW_TOP + 7.0);
    for (mode_index, mode) in GameMode::ALL.iter().enumerate() {
        spawn_option(
            commands,
            ModeSelectAction::SetMode(*mode),
            layout.rect(ClickRect {
                x: OPTION_LEFT + mode_index as f32 * (OPTION_W + OPTION_GAP),
                y: MODE_ROW_TOP,
                w: OPTION_W,
                h: OPTION_H,
            }),
            mode.label(),
            Color::srgba(0.53, 0.77, 0.96, 0.26),
        );
    }

    spawn_section_label(commands, layout, "Play Style", RULE_SET_ROW_TOP + 7.0);
    for (rule_set_index, rule_set) in RuleSet::ALL.iter().enumerate() {
        spawn_option(
            commands,
            ModeSelectAction::SetRuleSet(*rule_set),
            layout.rect(ClickRect {
                x: OPTION_LEFT + rule_set_index as f32 * (RULE_SET_OPTION_W + OPTION_GAP),
                y: RULE_SET_ROW_TOP,
                w: RULE_SET_OPTION_W,
                h: OPTION_H,
            }),
            rule_set.label(),
            Color::srgba(0.58, 0.72, 0.58, 0.28),
        );
    }

    for player_index in 0..active_player_count {
        let row_top = player_row_top(player_index);
        spawn_section_label(
            commands,
            layout,
            &format!("P{}", player_index + 1),
            row_top + 6.0,
        );
        for (seat_index, seat) in PlayerSeat::ALL.iter().enumerate() {
            let x = PLAYER_COLOR_LEFT + seat_index as f32 * (COLOR_SWATCH_W + OPTION_GAP);
            spawn_option(
                commands,
                ModeSelectAction::SetPlayerSeat {
                    player_index,
                    seat: *seat,
                },
                layout.rect(ClickRect {
                    x,
                    y: row_top,
                    w: COLOR_SWATCH_W,
                    h: COLOR_SWATCH_H,
                }),
                "",
                seat.to_color(),
            );
        }
        spawn_option(
            commands,
            ModeSelectAction::SetPlayerControl {
                player_index,
                control: PlayerControl::Human,
            },
            layout.rect(ClickRect {
                x: PLAYER_CONTROL_LEFT,
                y: row_top,
                w: PLAYER_CONTROL_W,
                h: OPTION_H,
            }),
            "Human",
            Color::srgba(0.53, 0.77, 0.96, 0.26),
        );
        spawn_option(
            commands,
            ModeSelectAction::SetPlayerControl {
                player_index,
                control: PlayerControl::Ai,
            },
            layout.rect(ClickRect {
                x: PLAYER_CONTROL_LEFT + PLAYER_CONTROL_W + PLAYER_CONTROL_GAP,
                y: row_top,
                w: PLAYER_CONTROL_W,
                h: OPTION_H,
            }),
            "AI",
            Color::srgba(0.53, 0.77, 0.96, 0.26),
        );
    }

    let pieces_top = pieces_row_top(active_player_count);
    spawn_section_label(commands, layout, "Pieces / Player", pieces_top + 7.0);
    for pieces in 1..=4u8 {
        let x = OPTION_LEFT + (pieces as f32 - 1.0) * (OPTION_W * 0.7 + OPTION_GAP);
        spawn_option(
            commands,
            ModeSelectAction::SetPieces(pieces),
            layout.rect(ClickRect {
                x,
                y: pieces_top,
                w: OPTION_W * 0.7,
                h: OPTION_H,
            }),
            &pieces.to_string(),
            Color::srgba(0.53, 0.77, 0.96, 0.26),
        );
    }

    let launch_rule_top = launch_rule_row_top(active_player_count);
    spawn_section_label(commands, layout, "Launch Rule", launch_rule_top + 7.0);
    for (rule_index, launch_rule) in LaunchRule::ALL.iter().enumerate() {
        spawn_option(
            commands,
            ModeSelectAction::SetLaunchRule(*launch_rule),
            layout.rect(ClickRect {
                x: OPTION_LEFT + rule_index as f32 * (OPTION_W + OPTION_GAP),
                y: launch_rule_top,
                w: OPTION_W,
                h: OPTION_H,
            }),
            launch_rule.label(),
            Color::srgba(0.53, 0.77, 0.96, 0.26),
        );
    }

    let ai_difficulty_top = ai_difficulty_row_top(active_player_count);
    spawn_section_label(commands, layout, "AI Difficulty", ai_difficulty_top + 7.0);
    for (difficulty_index, difficulty) in
        [AiDifficulty::Easy, AiDifficulty::Normal, AiDifficulty::Hard]
            .iter()
            .enumerate()
    {
        spawn_option(
            commands,
            ModeSelectAction::SetAiDifficulty(*difficulty),
            layout.rect(ClickRect {
                x: OPTION_LEFT + difficulty_index as f32 * (OPTION_W + OPTION_GAP),
                y: ai_difficulty_top,
                w: OPTION_W,
                h: OPTION_H,
            }),
            ai_difficulty_label(*difficulty),
            Color::srgba(0.61, 0.68, 0.88, 0.30),
        );
    }

    spawn_option(
        commands,
        ModeSelectAction::StartMatch,
        layout.rect(mode_select_start_rect(active_player_count)),
        "Start",
        Color::srgba(0.40, 0.72, 0.55, 0.40),
    );
    spawn_option(
        commands,
        ModeSelectAction::Back,
        layout.rect(mode_select_back_rect(active_player_count)),
        "Back",
        Color::srgba(0.72, 0.54, 0.44, 0.28),
    );
}

fn mode_select_start_rect(active_player_count: usize) -> ClickRect {
    ClickRect {
        x: mode_select_bottom_action_left(),
        y: bottom_row_top(active_player_count),
        w: BOTTOM_ACTION_W,
        h: BOTTOM_ACTION_H,
    }
}

fn mode_select_back_rect(active_player_count: usize) -> ClickRect {
    ClickRect {
        x: mode_select_bottom_action_left() + BOTTOM_ACTION_W + OPTION_GAP,
        y: bottom_row_top(active_player_count),
        w: BOTTOM_ACTION_W,
        h: BOTTOM_ACTION_H,
    }
}

fn mode_select_bottom_action_left() -> f32 {
    MODE_LAYOUT_VISIBLE_LEFT + (MODE_LAYOUT_VISIBLE_W - BOTTOM_ACTION_W * 2.0 - OPTION_GAP) * 0.5
}

fn spawn_section_label(commands: &mut Commands, layout: ModeSelectLayout, label: &str, top: f32) {
    // 左侧分区标题。
    spawn_setting_row_band(commands, layout, top - 7.0);
    commands.spawn((
        Text::new(label),
        TextFont {
            font_size: FontSize::Px(layout.font(20.0)),
            ..default()
        },
        TextColor(MODE_SELECT_BLACK),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(layout.y(top)),
            left: Val::Px(layout.x(SECTION_LABEL_X)),
            ..default()
        },
        Name::new(format!("ModeLabel{label}")),
        MenuEntity,
    ));
}

fn spawn_setting_row_band(commands: &mut Commands, layout: ModeSelectLayout, top: f32) {
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(layout.y(top - 2.0)),
            left: Val::Px(layout.x(SETTING_ROW_BAND_LEFT)),
            width: Val::Px(layout.size(SETTING_ROW_BAND_W)),
            height: Val::Px(layout.size(SETTING_ROW_BAND_H)),
            border: UiRect::all(Val::Px(layout.border(1.0))),
            ..default()
        },
        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.22)),
        BorderColor::all(Color::srgba(0.18, 0.24, 0.34, 0.10)),
        Name::new("ModeSettingRowBand"),
        MenuEntity,
    ));
}

fn spawn_option(
    commands: &mut Commands,
    action: ModeSelectAction,
    rect: ClickRect,
    label: &str,
    base_color: Color,
) {
    // 每个可选项都对应一个可点击矩形与行为枚举。
    spawn_box_with_label(commands, rect, base_color, label, 24.0, Some(action));
}

fn spawn_sound_option(
    commands: &mut Commands,
    action: SoundSettingsAction,
    rect: ClickRect,
    label: &str,
    base_color: Color,
) {
    spawn_box_with_label(commands, rect, base_color, label, 26.0, None);
    commands.spawn((
        ClickRect { ..rect },
        SoundSettingsOption { action },
        Name::new("SoundSettingsClickArea"),
        MenuEntity,
    ));
}

fn spawn_box_with_label(
    commands: &mut Commands,
    rect: ClickRect,
    color: Color,
    label: &str,
    font_size: f32,
    action: Option<ModeSelectAction>,
) -> Entity {
    // 通用方块渲染器：用于按钮底板与色块选项。
    let mut entity = commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(rect.x),
            top: Val::Px(rect.y),
            width: Val::Px(rect.w),
            height: Val::Px(rect.h),
            border: UiRect::all(Val::Px(fitted_box_border(rect.h))),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            padding: UiRect::horizontal(Val::Px(fitted_box_padding(rect.h))),
            ..default()
        },
        BackgroundColor(color),
        BorderColor::all(Color::srgba(0.16, 0.22, 0.32, 0.20)),
        Name::new("MenuOptionBox"),
        MenuEntity,
    ));
    let entity_id = entity.id();
    if let Some(action) = action {
        entity.insert((
            ClickRect { ..rect },
            ModeSelectOption {
                action,
                base_color: color,
            },
        ));
    }

    if !label.is_empty() {
        entity.with_children(|parent| {
            let mut label_entity = parent.spawn((
                Text::new(label),
                TextFont {
                    font_size: FontSize::Px(fitted_box_label_font_size(
                        font_size, rect.w, rect.h, label,
                    )),
                    ..default()
                },
                TextColor(MODE_SELECT_UNSELECTED_TEXT),
                TextLayout::justify(Justify::Center),
                Name::new("MenuOptionLabel"),
            ));
            if action.is_some() {
                label_entity.insert(ModeSelectOptionLabel);
            }
        });
    }
    entity_id
}

fn fitted_box_font_size(requested: f32, box_height: f32) -> f32 {
    requested.min((box_height * 0.68).max(10.0))
}

fn fitted_box_label_font_size(requested: f32, box_width: f32, box_height: f32, label: &str) -> f32 {
    let height_limited = fitted_box_font_size(requested, box_height);
    let label_width_units = label_width_units(label);
    if label_width_units <= f32::EPSILON {
        return height_limited;
    }

    let usable_width =
        (box_width - fitted_box_padding(box_height) * 2.0 - OPTION_LABEL_SAFETY_PX).max(1.0);
    height_limited.min((usable_width / label_width_units).max(10.0))
}

fn label_width_units(label: &str) -> f32 {
    label.chars().map(label_char_width_unit).sum()
}

fn label_char_width_unit(character: char) -> f32 {
    match character {
        ' ' => 0.34,
        '/' | '-' => 0.36,
        '0'..='9' => 0.56,
        _ => 0.62,
    }
}

fn fitted_box_border(box_height: f32) -> f32 {
    (box_height * 0.055).clamp(1.0, 2.0)
}

fn fitted_box_padding(box_height: f32) -> f32 {
    (box_height * 0.16).clamp(2.0, 6.0)
}

fn spawn_settings_toggle_visual(
    parent: &mut ChildSpawnerCommands<'_>,
    kind: SoundSettingsValueKind,
    state: SettingsToggleVisualState,
    root_width: f32,
    root_height: f32,
    requested_font_size: f32,
    name_prefix: &str,
) {
    let track_top = ((root_height - SETTINGS_TOGGLE_TRACK_H) * 0.5).max(0.0);
    parent
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(track_top),
                width: Val::Px(SETTINGS_TOGGLE_TRACK_W),
                height: Val::Px(SETTINGS_TOGGLE_TRACK_H),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::MAX,
                ..default()
            },
            BackgroundColor(settings_toggle_track_color(state.active)),
            BorderColor::all(settings_toggle_track_border_color(state.active)),
            SoundSettingsToggleTrack { kind },
            Name::new(format!("{name_prefix}Track")),
        ))
        .with_children(|track| {
            track.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(settings_toggle_thumb_left(state.active)),
                    top: Val::Px(SETTINGS_TOGGLE_PADDING),
                    width: Val::Px(SETTINGS_TOGGLE_THUMB),
                    height: Val::Px(SETTINGS_TOGGLE_THUMB),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::MAX,
                    ..default()
                },
                BackgroundColor(SETTINGS_TOGGLE_THUMB_COLOR),
                BorderColor::all(SETTINGS_TOGGLE_THUMB_BORDER),
                SoundSettingsToggleThumb { kind },
                Name::new(format!("{name_prefix}Thumb")),
            ));
        });

    let status_left = SETTINGS_TOGGLE_TRACK_W + SETTINGS_TOGGLE_TEXT_GAP;
    let status_width = (root_width - status_left).max(1.0);
    let font_size =
        fitted_box_label_font_size(requested_font_size, status_width, root_height, state.label);
    parent.spawn((
        Text::new(state.label),
        TextFont {
            font_size: FontSize::Px(font_size),
            ..default()
        },
        TextColor(MODE_SELECT_UNSELECTED_TEXT),
        TextLayout::justify(Justify::Center),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(status_left),
            top: Val::Px(settings_toggle_status_top(root_height, font_size)),
            width: Val::Px(status_width),
            ..default()
        },
        SoundSettingsValueText { kind },
        Name::new(format!("{name_prefix}Value")),
    ));
}

fn settings_toggle_status_top(root_height: f32, font_size: f32) -> f32 {
    ((root_height - font_size) * 0.5 - 1.0).max(0.0)
}

fn settings_toggle_thumb_left(active: bool) -> f32 {
    if active {
        SETTINGS_TOGGLE_TRACK_W - SETTINGS_TOGGLE_THUMB - SETTINGS_TOGGLE_PADDING
    } else {
        SETTINGS_TOGGLE_PADDING
    }
}

fn settings_toggle_track_color(active: bool) -> Color {
    if active {
        SETTINGS_TOGGLE_ACTIVE_TRACK
    } else {
        SETTINGS_TOGGLE_INACTIVE_TRACK
    }
}

fn settings_toggle_track_border_color(active: bool) -> Color {
    if active {
        SETTINGS_TOGGLE_ACTIVE_BORDER
    } else {
        SETTINGS_TOGGLE_INACTIVE_BORDER
    }
}

fn sound_settings_toggle_state(audio_settings: &AudioSettings) -> SettingsToggleVisualState {
    SettingsToggleVisualState {
        active: audio_settings.muted,
        label: format_mute_label(audio_settings),
    }
}

fn global_settings_toggle_state(
    value_kind: SoundSettingsValueKind,
    audio_settings: &AudioSettings,
    performance_settings: &PerformanceSettings,
) -> Option<SettingsToggleVisualState> {
    match value_kind {
        SoundSettingsValueKind::Mute => Some(sound_settings_toggle_state(audio_settings)),
        SoundSettingsValueKind::Fps => Some(SettingsToggleVisualState {
            active: performance_settings.show_fps,
            label: fps_toggle_label(performance_settings),
        }),
        SoundSettingsValueKind::Music | SoundSettingsValueKind::Effects => None,
    }
}

fn sound_settings_content(audio_settings: &AudioSettings) -> String {
    format!(
        "{}   |   Music {}   |   Effects {}",
        format_mute_label(audio_settings),
        format_volume_percent(audio_settings.music_volume),
        format_volume_percent(audio_settings.effects_volume)
    )
}

fn format_mute_label(audio_settings: &AudioSettings) -> &'static str {
    if audio_settings.muted {
        "Muted"
    } else {
        "Sound On"
    }
}

fn format_volume_percent(value: f32) -> String {
    format!("{:>3}%", (value.clamp(0.0, 1.0) * 100.0).round() as u8)
}

fn ai_difficulty_label(difficulty: AiDifficulty) -> &'static str {
    match difficulty {
        AiDifficulty::Easy => "Easy",
        AiDifficulty::Normal => "Normal",
        AiDifficulty::Hard => "Hard",
    }
}

fn update_mode_select_option_visuals(
    match_setup: Res<MatchSetup>,
    mut option_query: Query<(&ModeSelectOption, &mut BackgroundColor, &mut BorderColor)>,
    mut label_query: Query<(&ChildOf, &mut TextColor), With<ModeSelectOptionLabel>>,
    parent_option_query: Query<&ModeSelectOption>,
) {
    // 配置变更后刷新所有选项的高亮/禁用态。
    for (option, mut color, mut border) in &mut option_query {
        *color = BackgroundColor(option_fill_color(option, &match_setup));
        *border = BorderColor::all(option_border_color(option, &match_setup));
    }
    for (parent, mut text_color) in &mut label_query {
        let Ok(option) = parent_option_query.get(parent.parent()) else {
            continue;
        };
        *text_color = TextColor(option_text_color(option, &match_setup));
    }
}

fn option_fill_color(option: &ModeSelectOption, match_setup: &MatchSetup) -> Color {
    // 颜色优先级：禁用 > 选中 > 普通。
    if action_disabled(option.action, match_setup) {
        return option.base_color.mix(&Color::WHITE, 0.65).with_alpha(0.14);
    }
    if matches!(
        option.action,
        ModeSelectAction::StartMatch | ModeSelectAction::Back
    ) {
        return option.base_color.with_alpha(0.78);
    }
    if action_selected(option.action, match_setup) {
        return option.base_color.mix(&Color::WHITE, 0.04).with_alpha(0.98);
    }
    option.base_color.mix(&Color::WHITE, 0.55).with_alpha(0.34)
}

fn option_border_color(option: &ModeSelectOption, match_setup: &MatchSetup) -> Color {
    if action_disabled(option.action, match_setup) {
        return Color::srgba(0.30, 0.35, 0.42, 0.10);
    }
    if action_selected(option.action, match_setup) {
        return MODE_SELECT_BLACK;
    }
    if matches!(
        option.action,
        ModeSelectAction::StartMatch | ModeSelectAction::Back
    ) {
        return Color::srgba(0.14, 0.20, 0.30, 0.44);
    }
    Color::srgba(0.18, 0.24, 0.34, 0.14)
}

fn option_text_color(option: &ModeSelectOption, match_setup: &MatchSetup) -> Color {
    if action_disabled(option.action, match_setup) {
        return MODE_SELECT_DISABLED_TEXT;
    }
    if action_selected(option.action, match_setup) {
        return MODE_SELECT_BLACK;
    }
    MODE_SELECT_UNSELECTED_TEXT
}

fn action_disabled(action: ModeSelectAction, match_setup: &MatchSetup) -> bool {
    // 模式未启用的玩家行不显示，也不接受残留点击区域输入。
    match action {
        ModeSelectAction::SetPlayerSeat { player_index, .. }
        | ModeSelectAction::SetPlayerControl { player_index, .. } => {
            player_index >= match_setup.active_player_count()
        }
        ModeSelectAction::SetAiDifficulty(_) => !has_active_ai(match_setup),
        _ => false,
    }
}

fn action_selected(action: ModeSelectAction, match_setup: &MatchSetup) -> bool {
    // 判断某个选项是否与当前配置一致（用于高亮）。
    match action {
        ModeSelectAction::SetMode(mode) => match_setup.mode == mode,
        ModeSelectAction::SetRuleSet(rule_set) => match_setup.rule_set == rule_set,
        ModeSelectAction::SetPlayerSeat { player_index, seat } => {
            match_setup.player_seat(player_index) == Some(seat)
        }
        ModeSelectAction::SetPieces(pieces) => match_setup.pieces_per_player == pieces,
        ModeSelectAction::SetLaunchRule(launch_rule) => match_setup.launch_rule == launch_rule,
        ModeSelectAction::SetAiDifficulty(difficulty) => match_setup.ai_difficulty == difficulty,
        ModeSelectAction::SetPlayerControl {
            player_index,
            control,
        } => match_setup.player_control(player_index) == Some(control),
        _ => false,
    }
}

fn has_active_ai(match_setup: &MatchSetup) -> bool {
    let controls = match_setup.normalized_player_controls();
    controls[..match_setup.active_player_count()]
        .iter()
        .any(|control| matches!(control, PlayerControl::Ai))
}

fn handle_main_menu_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut next_state: ResMut<NextState<AppState>>,
    mut overlay_state: ResMut<SoundSettingsOverlayState>,
) {
    if sound_settings_overlay_blocks_input(&overlay_state) {
        return;
    }

    // 键盘兜底：回车进入配置页，S 打开声音设置弹窗。
    if keyboard.just_pressed(KeyCode::Enter) {
        next_state.set(AppState::ModeSelect);
    }
    if keyboard.just_pressed(KeyCode::KeyS) {
        overlay_state.open = true;
    }
}

fn handle_main_menu_click(
    pointer: Res<PointerInputState>,
    mut next_state: ResMut<NextState<AppState>>,
    overlay_state: Res<SoundSettingsOverlayState>,
    start_query: Query<&ClickRect, With<MainMenuStartArea>>,
) {
    // 鼠标主操作：点击开始进入配置；设置弹层由全局 Settings 入口打开。
    if sound_settings_overlay_blocks_input(&overlay_state) {
        return;
    }
    let Some(cursor) = pointer.just_pressed_position() else {
        return;
    };

    for rect in &start_query {
        if rect.contains(cursor) {
            next_state.set(AppState::ModeSelect);
            return;
        }
    }
}

fn update_sound_settings_text(
    audio_settings: Res<AudioSettings>,
    mut summary_query: Query<&mut Text, (With<SoundSettingsText>, Without<SoundSettingsValueText>)>,
    mut value_query: SoundSettingsValueQuery,
    mut toggle_track_query: Query<(
        &SoundSettingsToggleTrack,
        &mut BackgroundColor,
        &mut BorderColor,
    )>,
    mut toggle_thumb_query: Query<(&SoundSettingsToggleThumb, &mut Node)>,
) {
    if !audio_settings.is_changed() {
        return;
    }

    for mut text in &mut summary_query {
        *text = Text::new(sound_settings_content(&audio_settings));
    }

    for (value_text, mut text) in &mut value_query {
        *text = Text::new(match value_text.kind {
            SoundSettingsValueKind::Music => format_volume_percent(audio_settings.music_volume),
            SoundSettingsValueKind::Effects => format_volume_percent(audio_settings.effects_volume),
            SoundSettingsValueKind::Mute => format_mute_label(&audio_settings).to_owned(),
            SoundSettingsValueKind::Fps => "FPS --".to_owned(),
        });
    }
    let state = sound_settings_toggle_state(&audio_settings);
    for (track, mut background, mut border) in &mut toggle_track_query {
        if track.kind != SoundSettingsValueKind::Mute {
            continue;
        }
        *background = BackgroundColor(settings_toggle_track_color(state.active));
        *border = BorderColor::all(settings_toggle_track_border_color(state.active));
    }
    for (thumb, mut node) in &mut toggle_thumb_query {
        if thumb.kind != SoundSettingsValueKind::Mute {
            continue;
        }
        node.left = Val::Px(settings_toggle_thumb_left(state.active));
    }
}

fn handle_sound_settings_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if keyboard.just_pressed(KeyCode::Escape) || keyboard.just_pressed(KeyCode::Backspace) {
        next_state.set(AppState::MainMenu);
    }
}

fn handle_sound_settings_click(
    pointer: Res<PointerInputState>,
    mut audio_settings: ResMut<AudioSettings>,
    mut next_state: ResMut<NextState<AppState>>,
    query: Query<(&ClickRect, &SoundSettingsOption)>,
) {
    let Some(cursor) = pointer.just_pressed_position() else {
        return;
    };

    for (rect, option) in &query {
        if !rect.contains(cursor) {
            continue;
        }
        apply_sound_settings_action(option.action, &mut audio_settings, &mut next_state);
        return;
    }
}

fn apply_sound_settings_action(
    action: SoundSettingsAction,
    audio_settings: &mut AudioSettings,
    next_state: &mut NextState<AppState>,
) {
    match action {
        SoundSettingsAction::MusicDown => audio_settings.adjust_music(-AudioSettings::MUSIC_STEP),
        SoundSettingsAction::MusicUp => audio_settings.adjust_music(AudioSettings::MUSIC_STEP),
        SoundSettingsAction::EffectsDown => {
            audio_settings.adjust_effects(-AudioSettings::EFFECTS_STEP)
        }
        SoundSettingsAction::EffectsUp => {
            audio_settings.adjust_effects(AudioSettings::EFFECTS_STEP)
        }
        SoundSettingsAction::ToggleMute => audio_settings.toggle_mute(),
        SoundSettingsAction::ToggleFps => {}
        SoundSettingsAction::MainMenu | SoundSettingsAction::Back => {
            next_state.set(AppState::MainMenu)
        }
        SoundSettingsAction::QuitGame => {}
    }
}

fn handle_mode_select_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut match_setup: ResMut<MatchSetup>,
    mut next_state: ResMut<NextState<AppState>>,
    overlay_state: Res<SoundSettingsOverlayState>,
) {
    if sound_settings_overlay_blocks_input(&overlay_state) {
        return;
    }

    // 键盘兜底操作：保留最常用快捷键。
    if keyboard.just_pressed(KeyCode::Escape) {
        apply_mode_select_action(ModeSelectAction::Back, &mut match_setup, &mut next_state);
        return;
    }
    if keyboard.just_pressed(KeyCode::Digit1) {
        apply_mode_select_action(
            ModeSelectAction::SetMode(GameMode::OneVsOne),
            &mut match_setup,
            &mut next_state,
        );
    }
    if keyboard.just_pressed(KeyCode::Digit2) {
        apply_mode_select_action(
            ModeSelectAction::SetMode(GameMode::TwoVsTwo),
            &mut match_setup,
            &mut next_state,
        );
    }
    if keyboard.just_pressed(KeyCode::Digit3) {
        apply_mode_select_action(
            ModeSelectAction::SetMode(GameMode::FreeForAll),
            &mut match_setup,
            &mut next_state,
        );
    }
    if keyboard.just_pressed(KeyCode::KeyT) {
        apply_mode_select_action(
            ModeSelectAction::SetRuleSet(RuleSet::Traditional),
            &mut match_setup,
            &mut next_state,
        );
    }
    if keyboard.just_pressed(KeyCode::KeyC) {
        apply_mode_select_action(
            ModeSelectAction::SetRuleSet(RuleSet::Creative),
            &mut match_setup,
            &mut next_state,
        );
    }
    if keyboard.just_pressed(KeyCode::BracketLeft) || keyboard.just_pressed(KeyCode::Minus) {
        let next = match_setup.pieces_per_player.saturating_sub(1).clamp(1, 4);
        apply_mode_select_action(
            ModeSelectAction::SetPieces(next),
            &mut match_setup,
            &mut next_state,
        );
    }
    if keyboard.just_pressed(KeyCode::BracketRight) || keyboard.just_pressed(KeyCode::Equal) {
        let next = match_setup.pieces_per_player.saturating_add(1).clamp(1, 4);
        apply_mode_select_action(
            ModeSelectAction::SetPieces(next),
            &mut match_setup,
            &mut next_state,
        );
    }
    if keyboard.just_pressed(KeyCode::KeyQ) {
        apply_mode_select_action(
            ModeSelectAction::SetPlayerControl {
                player_index: 0,
                control: PlayerControl::Human,
            },
            &mut match_setup,
            &mut next_state,
        );
    }
    if keyboard.just_pressed(KeyCode::KeyW) {
        apply_mode_select_action(
            ModeSelectAction::SetPlayerControl {
                player_index: 0,
                control: PlayerControl::Ai,
            },
            &mut match_setup,
            &mut next_state,
        );
    }
    if keyboard.just_pressed(KeyCode::Enter) {
        apply_mode_select_action(
            ModeSelectAction::StartMatch,
            &mut match_setup,
            &mut next_state,
        );
    }
}

fn handle_mode_select_click(
    pointer: Res<PointerInputState>,
    mut match_setup: ResMut<MatchSetup>,
    mut next_state: ResMut<NextState<AppState>>,
    overlay_state: Res<SoundSettingsOverlayState>,
    query: Query<(&ClickRect, &ModeSelectOption)>,
) {
    // 鼠标主操作：点击命中对应配置项并立即生效。
    if sound_settings_overlay_blocks_input(&overlay_state) {
        return;
    }
    let Some(cursor) = pointer.just_pressed_position() else {
        return;
    };

    for (rect, option) in &query {
        if !rect.contains(cursor) {
            continue;
        }
        apply_mode_select_action(option.action, &mut match_setup, &mut next_state);
        return;
    }
}

fn apply_mode_select_action(
    action: ModeSelectAction,
    match_setup: &mut MatchSetup,
    next_state: &mut NextState<AppState>,
) {
    // 配置写入入口：集中处理模式切换、颜色选择、人机约束与页面跳转。
    if action_disabled(action, match_setup) {
        return;
    }

    match action {
        ModeSelectAction::SetMode(mode) => {
            match_setup.mode = mode;
            match_setup.sanitize_player_controls();
            match_setup.sanitize_player_seats();
        }
        ModeSelectAction::SetRuleSet(rule_set) => match_setup.rule_set = rule_set,
        ModeSelectAction::SetPlayerSeat { player_index, seat } => {
            match_setup.set_player_seat(player_index, seat)
        }
        ModeSelectAction::SetPieces(pieces) => match_setup.pieces_per_player = pieces.clamp(1, 4),
        ModeSelectAction::SetLaunchRule(launch_rule) => match_setup.launch_rule = launch_rule,
        ModeSelectAction::SetAiDifficulty(ai_difficulty) => {
            match_setup.ai_difficulty = ai_difficulty
        }
        ModeSelectAction::SetPlayerControl {
            player_index,
            control,
        } => match_setup.set_player_control(player_index, control),
        ModeSelectAction::StartMatch => {
            match_setup.sanitize_player_controls();
            match_setup.sanitize_player_seats();
            next_state.set(AppState::LoadingGame);
        }
        ModeSelectAction::Back => next_state.set(AppState::MainMenu),
    }
}

fn refresh_mode_select_layout(
    mut commands: Commands,
    windows: Query<&Window>,
    match_setup: Res<MatchSetup>,
    mut render_state: ResMut<ModeSelectRenderState>,
    query: Query<Entity, (With<MenuEntity>, Without<ChildOf>)>,
) {
    let next_key = mode_select_render_key(&windows, &match_setup);
    if render_state.key == Some(next_key) {
        return;
    }

    for entity in &query {
        commands.entity(entity).despawn();
    }
    spawn_mode_select_content(&mut commands, &windows, &match_setup);
    render_state.key = Some(next_key);
}

fn cleanup_menu(
    mut commands: Commands,
    query: Query<Entity, (With<MenuEntity>, Without<ChildOf>)>,
) {
    // 退出菜单状态时清理所有临时 UI 实体。
    for entity in &query {
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gameplay::ai::AiDifficulty;

    fn setup() -> MatchSetup {
        MatchSetup {
            mode: GameMode::TwoVsTwo,
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

    #[test]
    fn mode_select_layout_rows_do_not_overlap_or_overflow() {
        for active_player_count in [2, 4] {
            let mut rows = vec![
                (MODE_ROW_TOP, MODE_ROW_TOP + OPTION_H),
                (RULE_SET_ROW_TOP, RULE_SET_ROW_TOP + OPTION_H),
            ];
            rows.extend((0..active_player_count).map(|index| {
                let top = player_row_top(index);
                (top, top + OPTION_H.max(COLOR_SWATCH_H))
            }));
            rows.push((
                pieces_row_top(active_player_count),
                pieces_row_top(active_player_count) + OPTION_H,
            ));
            rows.push((
                launch_rule_row_top(active_player_count),
                launch_rule_row_top(active_player_count) + OPTION_H,
            ));
            rows.push((
                ai_difficulty_row_top(active_player_count),
                ai_difficulty_row_top(active_player_count) + OPTION_H,
            ));
            rows.push((
                bottom_row_top(active_player_count),
                bottom_row_top(active_player_count) + BOTTOM_ACTION_H,
            ));

            for pair in rows.windows(2) {
                let previous = pair[0];
                let next = pair[1];
                assert!(
                    previous.1 + 8.0 <= next.0,
                    "rows overlap or are too tight: {previous:?} -> {next:?}"
                );
            }

            let bottom = rows.last().map(|(_, bottom)| *bottom).unwrap_or_default();
            assert!(bottom <= 720.0);
            assert_eq!(
                bottom + 6.0 - MODE_LAYOUT_BASE_TOP,
                mode_select_layout_height(active_player_count)
            );
        }
    }

    #[test]
    fn mode_select_layout_centers_content_on_wide_screens() {
        let active_player_count = 4;
        let layout = mode_select_layout(1280.0, 720.0, active_player_count);
        let visible_left = layout.x(MODE_LAYOUT_VISIBLE_LEFT);
        let visible_center = visible_left + MODE_LAYOUT_VISIBLE_W * 0.5;

        assert!((visible_center - 1280.0 * 0.5).abs() < f32::EPSILON);
        assert!(
            (layout.top - (720.0 - mode_select_layout_height(active_player_count)) * 0.5).abs()
                < f32::EPSILON
        );
        assert_eq!(layout.x(MODE_LAYOUT_BASE_LEFT), layout.left);
        assert_eq!(layout.y(MODE_LAYOUT_BASE_TOP), layout.top);
    }

    #[test]
    fn mode_select_visible_rows_are_centered_on_tablet_screen() {
        let layout = mode_select_layout(2800.0, 1840.0, 4);
        let row = layout.rect(ClickRect {
            x: MODE_LAYOUT_VISIBLE_LEFT,
            y: MODE_ROW_TOP,
            w: MODE_LAYOUT_VISIBLE_W,
            h: SETTING_ROW_BAND_H,
        });

        assert!((row.x + row.w * 0.5 - 1400.0).abs() < f32::EPSILON);
    }

    #[test]
    fn mode_select_layout_scales_and_centers_on_narrow_screens() {
        let layout = mode_select_layout(600.0, 960.0, 4);
        let row = layout.rect(ClickRect {
            x: MODE_LAYOUT_VISIBLE_LEFT,
            y: MODE_ROW_TOP,
            w: MODE_LAYOUT_VISIBLE_W,
            h: SETTING_ROW_BAND_H,
        });
        let start = layout.rect(mode_select_start_rect(4));
        let back = layout.rect(mode_select_back_rect(4));

        assert!(layout.scale < 1.0);
        assert!(row.w <= 600.0 - 32.0 + 0.01);
        assert!((row.x + row.w * 0.5 - 300.0).abs() < 0.01);
        assert!((start.x + back.x + back.w - 600.0).abs() < 0.01);
    }

    #[test]
    fn rule_set_option_labels_fit_inside_buttons() {
        for (width, height) in [(1280.0, 720.0), (600.0, 960.0)] {
            let layout = mode_select_layout(width, height, 4);
            for (rule_set_index, rule_set) in RuleSet::ALL.iter().enumerate() {
                let rect = layout.rect(ClickRect {
                    x: OPTION_LEFT + rule_set_index as f32 * (RULE_SET_OPTION_W + OPTION_GAP),
                    y: RULE_SET_ROW_TOP,
                    w: RULE_SET_OPTION_W,
                    h: OPTION_H,
                });
                let font_size = fitted_box_label_font_size(24.0, rect.w, rect.h, rule_set.label());
                let estimated_label_width = label_width_units(rule_set.label()) * font_size
                    + fitted_box_padding(rect.h) * 2.0
                    + OPTION_LABEL_SAFETY_PX;

                assert!(
                    estimated_label_width <= rect.w + f32::EPSILON,
                    "{} label overflows {rect:?} at {width}x{height}",
                    rule_set.label()
                );
            }
        }
    }

    #[test]
    fn mode_select_render_key_tracks_window_size_changes() {
        let match_setup = setup();
        let first = mode_select_render_key_from_size(1280.0, 720.0, &match_setup);
        let tablet = mode_select_render_key_from_size(2800.0, 1840.0, &match_setup);

        assert_ne!(first, tablet);
        assert_eq!(tablet.window_width, 2800);
        assert_eq!(tablet.window_height, 1840);
    }

    #[test]
    fn mode_select_bottom_actions_are_centered_in_visible_content() {
        let start = mode_select_start_rect(4);
        let back = mode_select_back_rect(4);
        let action_center = (start.x + back.x + back.w) * 0.5;
        let visible_center = MODE_LAYOUT_VISIBLE_LEFT + MODE_LAYOUT_VISIBLE_W * 0.5;

        assert!((action_center - visible_center).abs() < f32::EPSILON);
    }

    #[test]
    fn main_menu_start_button_leaves_room_for_global_sound_entry() {
        let window = test_window();
        let start = main_menu_start_rect(window.width(), window.height());
        let audio = global_sound_entry_rect(&window);

        assert!(audio.y + audio.h + 40.0 <= start.y);
        assert!(start.y + start.h <= 720.0);
        assert!(start.x > MENU_LEFT);
    }

    #[test]
    fn global_sound_entry_stays_top_right_on_every_page() {
        let window = test_window();
        let entry = global_sound_entry_rect(&window);

        assert!(entry.x > window.width() * 0.5);
        assert_eq!(entry.x + entry.w + GLOBAL_SOUND_ENTRY_LEFT, window.width());
        assert_eq!(entry.y, GLOBAL_SOUND_ENTRY_TOP);
    }

    #[test]
    fn open_sound_overlay_blocks_lower_input_even_before_capture_flag_updates() {
        assert!(sound_settings_overlay_blocks_input(
            &SoundSettingsOverlayState {
                open: true,
                input_captured: false,
            }
        ));
        assert!(sound_settings_overlay_blocks_input(
            &SoundSettingsOverlayState {
                open: false,
                input_captured: true,
            }
        ));
        assert!(!sound_settings_overlay_blocks_input(
            &SoundSettingsOverlayState {
                open: false,
                input_captured: false,
            }
        ));
    }

    #[test]
    fn global_sound_overlay_system_initializes_without_query_conflicts() {
        let mut app = App::new();
        app.add_plugins(bevy::state::app::StatesPlugin)
            .init_state::<AppState>()
            .init_resource::<AudioSettings>()
            .init_resource::<PerformanceSettings>()
            .init_resource::<SoundSettingsOverlayState>()
            .add_systems(Update, update_global_sound_overlay);

        app.update();
    }

    #[test]
    fn sound_settings_actions_adjust_independent_channels() {
        let mut audio_settings = AudioSettings {
            music_volume: 0.5,
            effects_volume: 0.5,
            muted: false,
        };
        let mut next_state = NextState::<AppState>::default();

        apply_sound_settings_action(
            SoundSettingsAction::MusicUp,
            &mut audio_settings,
            &mut next_state,
        );
        apply_sound_settings_action(
            SoundSettingsAction::EffectsDown,
            &mut audio_settings,
            &mut next_state,
        );

        assert!((audio_settings.music_volume - 0.55).abs() < f32::EPSILON);
        assert!((audio_settings.effects_volume - 0.45).abs() < f32::EPSILON);

        apply_sound_settings_action(
            SoundSettingsAction::ToggleMute,
            &mut audio_settings,
            &mut next_state,
        );
        assert!(audio_settings.muted);
        assert!((audio_settings.music_volume - 0.55).abs() < f32::EPSILON);
        assert!((audio_settings.effects_volume - 0.45).abs() < f32::EPSILON);
    }

    #[test]
    fn global_settings_actions_adjust_audio_return_to_menu_and_quit() {
        let mut audio_settings = AudioSettings {
            music_volume: 0.5,
            effects_volume: 0.5,
            muted: false,
        };
        let mut performance_settings = PerformanceSettings { show_fps: false };
        let mut overlay_state = SoundSettingsOverlayState {
            open: true,
            input_captured: false,
        };

        assert_eq!(
            apply_global_sound_action(
                SoundSettingsAction::MusicDown,
                &mut audio_settings,
                &mut performance_settings,
                &mut overlay_state,
            ),
            GlobalSettingsCommand::None
        );
        assert_eq!(
            apply_global_sound_action(
                SoundSettingsAction::EffectsUp,
                &mut audio_settings,
                &mut performance_settings,
                &mut overlay_state,
            ),
            GlobalSettingsCommand::None
        );

        assert!((audio_settings.music_volume - 0.45).abs() < f32::EPSILON);
        assert!((audio_settings.effects_volume - 0.55).abs() < f32::EPSILON);
        assert!(overlay_state.open);

        assert_eq!(
            apply_global_sound_action(
                SoundSettingsAction::ToggleMute,
                &mut audio_settings,
                &mut performance_settings,
                &mut overlay_state,
            ),
            GlobalSettingsCommand::None
        );
        assert!(audio_settings.muted);
        assert!(overlay_state.open);

        assert_eq!(
            apply_global_sound_action(
                SoundSettingsAction::ToggleFps,
                &mut audio_settings,
                &mut performance_settings,
                &mut overlay_state,
            ),
            GlobalSettingsCommand::None
        );
        assert!(performance_settings.show_fps);
        assert!(overlay_state.open);

        assert_eq!(
            apply_global_sound_action(
                SoundSettingsAction::MainMenu,
                &mut audio_settings,
                &mut performance_settings,
                &mut overlay_state,
            ),
            GlobalSettingsCommand::MainMenu
        );
        assert!(!overlay_state.open);

        overlay_state.open = true;
        assert_eq!(
            apply_global_sound_action(
                SoundSettingsAction::QuitGame,
                &mut audio_settings,
                &mut performance_settings,
                &mut overlay_state,
            ),
            GlobalSettingsCommand::QuitGame
        );
        assert!(!overlay_state.open);

        overlay_state.open = true;
        assert_eq!(
            apply_global_sound_action(
                SoundSettingsAction::Back,
                &mut audio_settings,
                &mut performance_settings,
                &mut overlay_state,
            ),
            GlobalSettingsCommand::None
        );
        assert!(!overlay_state.open);
    }

    #[test]
    fn sound_settings_percent_text_is_clamped() {
        assert_eq!(format_volume_percent(-0.4), "  0%");
        assert_eq!(format_volume_percent(0.354), " 35%");
        assert_eq!(format_volume_percent(1.4), "100%");
    }

    #[test]
    fn sound_settings_mute_label_tracks_state() {
        let mut audio_settings = AudioSettings {
            music_volume: 0.5,
            effects_volume: 0.5,
            muted: false,
        };

        assert_eq!(format_mute_label(&audio_settings), "Sound On");
        assert!(sound_settings_content(&audio_settings).starts_with("Sound On"));

        audio_settings.toggle_mute();
        assert_eq!(format_mute_label(&audio_settings), "Muted");
        assert!(sound_settings_content(&audio_settings).starts_with("Muted"));
    }

    #[test]
    fn settings_toggle_thumb_moves_between_track_edges() {
        let off_left = settings_toggle_thumb_left(false);
        let on_left = settings_toggle_thumb_left(true);

        assert_eq!(off_left, SETTINGS_TOGGLE_PADDING);
        assert!(on_left > off_left);
        assert_eq!(
            on_left + SETTINGS_TOGGLE_THUMB + SETTINGS_TOGGLE_PADDING,
            SETTINGS_TOGGLE_TRACK_W
        );
    }

    #[test]
    fn settings_toggle_state_tracks_mute_and_fps() {
        let mut audio_settings = AudioSettings {
            music_volume: 0.5,
            effects_volume: 0.5,
            muted: false,
        };
        let mut performance_settings = PerformanceSettings { show_fps: false };

        assert_eq!(
            global_settings_toggle_state(
                SoundSettingsValueKind::Mute,
                &audio_settings,
                &performance_settings,
            ),
            Some(SettingsToggleVisualState {
                active: false,
                label: "Sound On",
            })
        );
        assert_eq!(
            global_settings_toggle_state(
                SoundSettingsValueKind::Fps,
                &audio_settings,
                &performance_settings,
            ),
            Some(SettingsToggleVisualState {
                active: false,
                label: "FPS Off",
            })
        );

        audio_settings.toggle_mute();
        performance_settings.toggle_fps();
        assert_eq!(
            global_settings_toggle_state(
                SoundSettingsValueKind::Mute,
                &audio_settings,
                &performance_settings,
            )
            .map(|state| state.active),
            Some(true)
        );
        assert_eq!(
            global_settings_toggle_state(
                SoundSettingsValueKind::Fps,
                &audio_settings,
                &performance_settings,
            )
            .map(|state| state.label),
            Some("FPS On")
        );
    }

    #[test]
    fn global_sound_entry_and_panel_actions_have_stable_hit_targets() {
        let window = test_window();
        let entry = global_sound_entry_rect(&window);
        assert!(entry.contains(Vec2::new(entry.x + 8.0, entry.y + 8.0)));
        assert!(entry.x > window.width() * 0.5);

        let panel = global_sound_panel_rect(&window);
        assert_eq!(panel.w, GLOBAL_SOUND_PANEL_W);
        assert_eq!(panel.h, GLOBAL_SOUND_PANEL_H);
        let plus_right_edge = GLOBAL_SOUND_CONTROL_LEFT
            + GLOBAL_SOUND_BUTTON
            + GLOBAL_SOUND_VALUE_W
            + 20.0
            + GLOBAL_SOUND_BUTTON;
        assert!(GLOBAL_SOUND_PANEL_W - plus_right_edge >= 24.0);

        let music_up = Vec2::new(
            panel.x + GLOBAL_SOUND_CONTROL_LEFT + GLOBAL_SOUND_BUTTON + GLOBAL_SOUND_VALUE_W + 28.0,
            panel.y + GLOBAL_SOUND_ROW_TOP + 8.0,
        );
        assert_eq!(
            global_sound_action_at(music_up, &window),
            Some(SoundSettingsAction::MusicUp)
        );

        let mute = Vec2::new(
            panel.x + GLOBAL_SOUND_CONTROL_LEFT + 8.0,
            panel.y + GLOBAL_SOUND_MUTE_ROW_TOP + 8.0,
        );
        assert_eq!(
            global_sound_action_at(mute, &window),
            Some(SoundSettingsAction::ToggleMute)
        );

        let fps = Vec2::new(
            panel.x + GLOBAL_SOUND_CONTROL_LEFT + 8.0,
            panel.y + GLOBAL_SOUND_FPS_ROW_TOP + 8.0,
        );
        assert_eq!(
            global_sound_action_at(fps, &window),
            Some(SoundSettingsAction::ToggleFps)
        );

        let main_menu = Vec2::new(
            panel.x + global_settings_main_menu_rect().x + 8.0,
            panel.y + global_settings_main_menu_rect().y + 8.0,
        );
        assert_eq!(
            global_sound_action_at(main_menu, &window),
            Some(SoundSettingsAction::MainMenu)
        );

        let quit_game = Vec2::new(
            panel.x + global_settings_quit_game_rect().x + 8.0,
            panel.y + global_settings_quit_game_rect().y + 8.0,
        );
        assert_eq!(
            global_sound_action_at(quit_game, &window),
            Some(SoundSettingsAction::QuitGame)
        );
    }

    fn test_window() -> Window {
        Window {
            resolution: (1280, 720).into(),
            ..default()
        }
    }

    #[test]
    fn selected_unselected_and_disabled_options_are_visually_distinct() {
        let mut match_setup = setup();
        let selected = ModeSelectOption {
            action: ModeSelectAction::SetMode(GameMode::TwoVsTwo),
            base_color: Color::srgba(0.53, 0.77, 0.96, 0.26),
        };
        let unselected = ModeSelectOption {
            action: ModeSelectAction::SetMode(GameMode::OneVsOne),
            base_color: selected.base_color,
        };

        assert_ne!(
            option_fill_color(&selected, &match_setup),
            option_fill_color(&unselected, &match_setup)
        );
        assert_ne!(
            option_border_color(&selected, &match_setup),
            option_border_color(&unselected, &match_setup)
        );
        assert_eq!(option_border_color(&selected, &match_setup), Color::BLACK);
        assert_eq!(option_text_color(&selected, &match_setup), Color::BLACK);
        assert_ne!(option_border_color(&unselected, &match_setup), Color::BLACK);
        assert_ne!(option_text_color(&unselected, &match_setup), Color::BLACK);

        match_setup.mode = GameMode::OneVsOne;
        let disabled = ModeSelectOption {
            action: ModeSelectAction::SetPlayerControl {
                player_index: 2,
                control: PlayerControl::Human,
            },
            base_color: selected.base_color,
        };
        assert_ne!(
            option_fill_color(&disabled, &match_setup),
            option_fill_color(&selected, &match_setup)
        );
        assert_ne!(
            option_border_color(&disabled, &match_setup),
            option_border_color(&selected, &match_setup)
        );
    }

    #[test]
    fn inactive_player_seat_and_control_actions_are_disabled() {
        let mut match_setup = setup();
        match_setup.mode = GameMode::OneVsOne;

        assert!(action_disabled(
            ModeSelectAction::SetPlayerSeat {
                player_index: 2,
                seat: PlayerSeat::Green,
            },
            &match_setup
        ));
        assert!(action_disabled(
            ModeSelectAction::SetPlayerControl {
                player_index: 2,
                control: PlayerControl::Human,
            },
            &match_setup
        ));
        assert!(!action_disabled(
            ModeSelectAction::SetPlayerSeat {
                player_index: 1,
                seat: PlayerSeat::Red,
            },
            &match_setup
        ));
    }

    #[test]
    fn seat_options_follow_selection_and_swap_occupied_seats() {
        let mut match_setup = setup();
        let mut next_state = NextState::<AppState>::default();
        let p1_blue = ModeSelectAction::SetPlayerSeat {
            player_index: 0,
            seat: PlayerSeat::Blue,
        };
        let p1_red = ModeSelectAction::SetPlayerSeat {
            player_index: 0,
            seat: PlayerSeat::Red,
        };
        let p2_blue = ModeSelectAction::SetPlayerSeat {
            player_index: 1,
            seat: PlayerSeat::Blue,
        };

        assert!(action_selected(p1_blue, &match_setup));
        assert!(!action_selected(p1_red, &match_setup));

        apply_mode_select_action(p1_red, &mut match_setup, &mut next_state);

        assert_eq!(
            match_setup.player_seats,
            [
                PlayerSeat::Red,
                PlayerSeat::Blue,
                PlayerSeat::Green,
                PlayerSeat::Yellow,
            ]
        );
        assert!(action_selected(p1_red, &match_setup));
        assert!(action_selected(p2_blue, &match_setup));
    }

    #[test]
    fn rule_set_options_follow_match_setup_selection() {
        let mut match_setup = setup();
        let traditional_option = ModeSelectAction::SetRuleSet(RuleSet::Traditional);
        let creative_option = ModeSelectAction::SetRuleSet(RuleSet::Creative);

        assert!(action_selected(creative_option, &match_setup));
        assert!(!action_selected(traditional_option, &match_setup));

        apply_mode_select_action(
            traditional_option,
            &mut match_setup,
            &mut NextState::<AppState>::default(),
        );

        assert_eq!(match_setup.rule_set, RuleSet::Traditional);
        assert!(action_selected(traditional_option, &match_setup));
        assert!(!action_selected(creative_option, &match_setup));
    }

    #[test]
    fn launch_rule_options_follow_match_setup_selection() {
        let mut match_setup = setup();
        let even_option = ModeSelectAction::SetLaunchRule(LaunchRule::Even);
        let six_only_option = ModeSelectAction::SetLaunchRule(LaunchRule::SixOnly);

        assert!(action_selected(six_only_option, &match_setup));
        assert!(!action_selected(even_option, &match_setup));

        apply_mode_select_action(
            even_option,
            &mut match_setup,
            &mut NextState::<AppState>::default(),
        );

        assert_eq!(match_setup.launch_rule, LaunchRule::Even);
        assert!(action_selected(even_option, &match_setup));
        assert!(!action_selected(six_only_option, &match_setup));
    }

    #[test]
    fn ai_difficulty_options_follow_match_setup_selection() {
        let mut match_setup = setup();
        let easy_option = ModeSelectAction::SetAiDifficulty(AiDifficulty::Easy);
        let normal_option = ModeSelectAction::SetAiDifficulty(AiDifficulty::Normal);

        assert!(has_active_ai(&match_setup));
        assert!(action_selected(normal_option, &match_setup));
        assert!(!action_selected(easy_option, &match_setup));

        apply_mode_select_action(
            easy_option,
            &mut match_setup,
            &mut NextState::<AppState>::default(),
        );

        assert_eq!(match_setup.ai_difficulty, AiDifficulty::Easy);
        assert!(action_selected(easy_option, &match_setup));
        assert!(!action_selected(normal_option, &match_setup));
    }

    #[test]
    fn ai_difficulty_is_disabled_without_active_ai_players() {
        let mut match_setup = setup();
        match_setup.player_controls = [
            PlayerControl::Human,
            PlayerControl::Human,
            PlayerControl::Human,
            PlayerControl::Human,
        ];
        let hard_option = ModeSelectAction::SetAiDifficulty(AiDifficulty::Hard);

        assert!(!has_active_ai(&match_setup));
        assert!(action_disabled(hard_option, &match_setup));
    }

    #[test]
    fn start_and_back_buttons_are_same_size_with_short_start_label() {
        let start = mode_select_start_rect(4);
        let back = mode_select_back_rect(4);

        assert_eq!(start.w, back.w);
        assert_eq!(start.h, back.h);
        assert_eq!(back.x, start.x + start.w + OPTION_GAP);
    }
}
