use bevy::prelude::*;

use crate::data::game_mode::GameMode;
use crate::domain::player::PlayerControl;
use crate::domain::rules::LaunchRule;
use crate::gameplay::match_flow::{MatchSetup, PlayerColorChoice};
use crate::platform::PointerInputState;
use crate::plugins::audio_plugin::AudioSettings;
use crate::states::AppState;

/// 菜单插件：主菜单与开局配置页的渲染和交互。
pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SoundSettingsOverlayState>()
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
                    handle_main_menu_input.run_if(in_state(AppState::MainMenu)),
                    handle_main_menu_click.run_if(in_state(AppState::MainMenu)),
                    update_mode_select_text.run_if(in_state(AppState::ModeSelect)),
                    update_mode_select_option_visuals.run_if(in_state(AppState::ModeSelect)),
                    handle_mode_select_input.run_if(in_state(AppState::ModeSelect)),
                    handle_mode_select_click.run_if(in_state(AppState::ModeSelect)),
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

#[derive(Component)]
/// 菜单实体分组标记。
struct MenuEntity;

#[derive(Component)]
/// 常驻声音入口实体。
struct GlobalSoundEntry;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GlobalSoundEntryAnchor {
    Left,
    Right,
}

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
/// 声音设置页摘要文本节点。
struct SoundSettingsText;

#[derive(Clone, Copy, Component, Debug, Eq, PartialEq)]
enum SoundSettingsValueKind {
    Music,
    Effects,
}

#[derive(Component)]
/// 声音设置页百分比文本节点。
struct SoundSettingsValueText {
    kind: SoundSettingsValueKind,
}

#[derive(Component)]
/// 配置页顶部摘要文本节点。
struct ModeSelectText;

#[derive(Clone, Copy, Component)]
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
    SetPlayerColor {
        player_index: usize,
        color: PlayerColorChoice,
    },
    SetPieces(u8),
    SetLaunchRule(LaunchRule),
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

#[derive(Clone, Copy, Component, Debug, Eq, PartialEq)]
enum SoundSettingsAction {
    MusicDown,
    MusicUp,
    EffectsDown,
    EffectsUp,
    Back,
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
const MAIN_START_TOP: f32 = 250.0;
const MAIN_START_WIDTH: f32 = 360.0;
const MAIN_START_HEIGHT: f32 = 62.0;
const MAIN_BUTTON_GAP: f32 = 22.0;

const GLOBAL_SOUND_ENTRY_LEFT: f32 = 16.0;
const GLOBAL_SOUND_ENTRY_TOP: f32 = 16.0;
const GLOBAL_SOUND_ENTRY_W: f32 = 108.0;
const GLOBAL_SOUND_ENTRY_H: f32 = 38.0;
const GLOBAL_SOUND_PANEL_W: f32 = 462.0;
const GLOBAL_SOUND_PANEL_H: f32 = 292.0;
const GLOBAL_SOUND_ROW_LEFT: f32 = 34.0;
const GLOBAL_SOUND_CONTROL_LEFT: f32 = 244.0;
const GLOBAL_SOUND_ROW_TOP: f32 = 84.0;
const GLOBAL_SOUND_ROW_GAP: f32 = 72.0;
const GLOBAL_SOUND_BUTTON: f32 = 42.0;
const GLOBAL_SOUND_VALUE_W: f32 = 82.0;
const GLOBAL_SOUND_CLOSE_W: f32 = 104.0;
const GLOBAL_SOUND_CLOSE_H: f32 = 42.0;

const SOUND_PANEL_TOP: f32 = 170.0;
const SOUND_ROW_GAP: f32 = 92.0;
const SOUND_CONTROL_LEFT: f32 = 430.0;
const SOUND_BUTTON: f32 = 52.0;
const SOUND_VALUE_W: f32 = 100.0;
const SOUND_BACK_TOP: f32 = 488.0;

const SECTION_LABEL_X: f32 = 96.0;
const OPTION_LEFT: f32 = 336.0;
const OPTION_W: f32 = 112.0;
const OPTION_H: f32 = 36.0;
const OPTION_GAP: f32 = 12.0;
const MODE_ROW_TOP: f32 = 128.0;
const COLOR_ROW_START_TOP: f32 = 184.0;
const COLOR_ROW_GAP: f32 = 40.0;
const PIECES_ROW_TOP: f32 = 356.0;
const LAUNCH_RULE_ROW_TOP: f32 = 404.0;
const CONTROL_ROW_START_TOP: f32 = 456.0;
const CONTROL_ROW_GAP: f32 = 44.0;
const BOTTOM_ROW_TOP: f32 = 646.0;
const COLOR_SWATCH_W: f32 = 54.0;
const COLOR_SWATCH_H: f32 = 32.0;

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
                Text::new("Audio"),
                TextFont {
                    font_size: 17.0,
                    ..default()
                },
                TextColor(Color::srgb(0.10, 0.16, 0.24)),
                TextLayout::new_with_justify(Justify::Center),
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
                        Text::new("Sound Settings"),
                        TextFont {
                            font_size: 28.0,
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

                    spawn_global_sound_row(
                        panel,
                        "Background Music",
                        SoundSettingsValueKind::Music,
                        GLOBAL_SOUND_ROW_TOP,
                    );
                    spawn_global_sound_row(
                        panel,
                        "Action Effects",
                        SoundSettingsValueKind::Effects,
                        GLOBAL_SOUND_ROW_TOP + GLOBAL_SOUND_ROW_GAP,
                    );

                    spawn_global_sound_panel_button(
                        panel,
                        ClickRect {
                            x: GLOBAL_SOUND_PANEL_W - GLOBAL_SOUND_ROW_LEFT - GLOBAL_SOUND_CLOSE_W,
                            y: GLOBAL_SOUND_PANEL_H - 62.0,
                            w: GLOBAL_SOUND_CLOSE_W,
                            h: GLOBAL_SOUND_CLOSE_H,
                        },
                        "Close",
                        20.0,
                    );
                });
        });
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
            font_size: 20.0,
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
            font_size: 22.0,
            ..default()
        },
        TextColor(Color::srgb(0.10, 0.16, 0.24)),
        TextLayout::new_with_justify(Justify::Center),
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
                    font_size,
                    ..default()
                },
                TextColor(Color::srgb(0.10, 0.16, 0.24)),
                TextLayout::new_with_justify(Justify::Center),
                Name::new("GlobalSoundButtonLabel"),
            ));
        });
}

fn update_sound_overlay_input_capture(
    mut overlay_state: ResMut<SoundSettingsOverlayState>,
    app_state: Res<State<AppState>>,
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

    overlay_state.input_captured =
        global_sound_entry_rect(window, app_state.get()).contains(cursor);
}

fn update_global_sound_overlay(
    app_state: Res<State<AppState>>,
    audio_settings: Res<AudioSettings>,
    overlay_state: Res<SoundSettingsOverlayState>,
    mut entry_query: Query<
        (&mut Node, &mut Visibility),
        (With<GlobalSoundEntry>, Without<GlobalSoundModal>),
    >,
    mut modal_query: Query<&mut Visibility, (With<GlobalSoundModal>, Without<GlobalSoundEntry>)>,
    mut value_query: Query<(&SoundSettingsValueText, &mut Text)>,
) {
    let visible_on_page = !matches!(app_state.get(), AppState::Boot);
    let entry_visibility = if visible_on_page {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    let entry_anchor = global_sound_entry_anchor(app_state.get());
    for (mut node, mut visibility) in &mut entry_query {
        apply_global_sound_entry_anchor(&mut node, entry_anchor);
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

    if !audio_settings.is_changed() && !overlay_state.is_changed() {
        return;
    }
    for (value_text, mut text) in &mut value_query {
        *text = Text::new(match value_text.kind {
            SoundSettingsValueKind::Music => format_volume_percent(audio_settings.music_volume),
            SoundSettingsValueKind::Effects => format_volume_percent(audio_settings.effects_volume),
        });
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
        overlay_state.open = false;
    }
}

fn handle_global_sound_overlay_click(
    pointer: Res<PointerInputState>,
    app_state: Res<State<AppState>>,
    windows: Query<&Window>,
    mut audio_settings: ResMut<AudioSettings>,
    mut overlay_state: ResMut<SoundSettingsOverlayState>,
) {
    let Some(cursor) = pointer.just_pressed_position() else {
        return;
    };
    let Ok(window) = windows.single() else {
        return;
    };

    if overlay_state.open {
        if let Some(action) = global_sound_action_at(cursor, window) {
            apply_global_sound_action(action, &mut audio_settings, &mut overlay_state);
            return;
        }

        if !global_sound_panel_rect(window).contains(cursor) {
            overlay_state.open = false;
        }
        return;
    }

    if global_sound_entry_rect(window, app_state.get()).contains(cursor) {
        overlay_state.open = true;
    }
}

fn apply_global_sound_action(
    action: SoundSettingsAction,
    audio_settings: &mut AudioSettings,
    overlay_state: &mut SoundSettingsOverlayState,
) {
    match action {
        SoundSettingsAction::MusicDown => audio_settings.adjust_music(-AudioSettings::STEP),
        SoundSettingsAction::MusicUp => audio_settings.adjust_music(AudioSettings::STEP),
        SoundSettingsAction::EffectsDown => audio_settings.adjust_effects(-AudioSettings::STEP),
        SoundSettingsAction::EffectsUp => audio_settings.adjust_effects(AudioSettings::STEP),
        SoundSettingsAction::Back => overlay_state.open = false,
    }
}

fn global_sound_entry_anchor(app_state: &AppState) -> GlobalSoundEntryAnchor {
    if matches!(app_state, AppState::InGame) {
        GlobalSoundEntryAnchor::Left
    } else {
        GlobalSoundEntryAnchor::Right
    }
}

fn apply_global_sound_entry_anchor(node: &mut Node, anchor: GlobalSoundEntryAnchor) {
    match anchor {
        GlobalSoundEntryAnchor::Left => {
            node.left = Val::Px(GLOBAL_SOUND_ENTRY_LEFT);
            node.right = Val::Auto;
        }
        GlobalSoundEntryAnchor::Right => {
            node.left = Val::Auto;
            node.right = Val::Px(GLOBAL_SOUND_ENTRY_LEFT);
        }
    }
    node.top = Val::Px(GLOBAL_SOUND_ENTRY_TOP);
}

fn global_sound_entry_rect(window: &Window, app_state: &AppState) -> ClickRect {
    let x = match global_sound_entry_anchor(app_state) {
        GlobalSoundEntryAnchor::Left => GLOBAL_SOUND_ENTRY_LEFT,
        GlobalSoundEntryAnchor::Right => {
            (window.width() - GLOBAL_SOUND_ENTRY_W - GLOBAL_SOUND_ENTRY_LEFT)
                .max(GLOBAL_SOUND_ENTRY_LEFT)
        }
    };

    ClickRect {
        x,
        y: GLOBAL_SOUND_ENTRY_TOP,
        w: GLOBAL_SOUND_ENTRY_W,
        h: GLOBAL_SOUND_ENTRY_H,
    }
}

fn global_sound_panel_rect(window: &Window) -> ClickRect {
    ClickRect {
        x: (window.width() - GLOBAL_SOUND_PANEL_W) * 0.5,
        y: (window.height() - GLOBAL_SOUND_PANEL_H) * 0.5,
        w: GLOBAL_SOUND_PANEL_W,
        h: GLOBAL_SOUND_PANEL_H,
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
            SoundSettingsAction::Back,
            ClickRect {
                x: GLOBAL_SOUND_PANEL_W - GLOBAL_SOUND_ROW_LEFT - GLOBAL_SOUND_CLOSE_W,
                y: GLOBAL_SOUND_PANEL_H - 62.0,
                w: GLOBAL_SOUND_CLOSE_W,
                h: GLOBAL_SOUND_CLOSE_H,
            },
        ),
    ];

    actions
        .iter()
        .find_map(|(action, rect)| rect.contains(local).then_some(*action))
}

fn spawn_main_menu(mut commands: Commands, windows: Query<&Window>) {
    // 主菜单：标题 + 开始与声音设置入口。
    let window_width = windows.single().map(Window::width).unwrap_or(1280.0);
    let title_left = ((window_width - 520.0) * 0.5).max(MENU_LEFT);
    let start_rect = main_menu_start_rect(window_width);

    commands.spawn((
        Text::new("Aeroplane Chess"),
        TextFont {
            font_size: 54.0,
            ..default()
        },
        TextColor(Color::srgb(0.10, 0.16, 0.24)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(130.0),
            left: Val::Px(title_left),
            ..default()
        },
        Name::new("MainMenuTitle"),
        MenuEntity,
    ));

    spawn_box_with_label(
        &mut commands,
        start_rect,
        Color::srgba(0.42, 0.61, 0.88, 0.30),
        "Start Match",
        30.0,
        None,
    );

    commands.spawn((
        MainMenuStartArea,
        start_rect,
        Name::new("MainMenuStartArea"),
        MenuEntity,
    ));
}

fn main_menu_start_rect(window_width: f32) -> ClickRect {
    ClickRect {
        x: ((window_width - MAIN_START_WIDTH) * 0.5).max(MENU_LEFT),
        y: MAIN_START_TOP,
        w: MAIN_START_WIDTH,
        h: MAIN_START_HEIGHT,
    }
}

fn spawn_sound_settings(mut commands: Commands, audio_settings: Res<AudioSettings>) {
    commands.spawn((
        Text::new("Sound Settings"),
        TextFont {
            font_size: 46.0,
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
            font_size: 19.0,
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
            font_size: 24.0,
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
            font_size: 26.0,
            ..default()
        },
        TextColor(Color::srgb(0.10, 0.16, 0.24)),
        TextLayout::new_with_justify(Justify::Center),
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

fn spawn_mode_select(mut commands: Commands, match_setup: Res<MatchSetup>) {
    // 对局配置页：按“模式/颜色/棋子数/人机控制/开始返回”分区渲染。
    commands.spawn((
        Text::new(mode_select_content(&match_setup)),
        TextFont {
            font_size: 20.0,
            ..default()
        },
        TextColor(Color::srgb(0.10, 0.16, 0.24)),
        TextLayout::new_with_linebreak(LineBreak::WordOrCharacter),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(22.0),
            left: Val::Px(MENU_LEFT),
            width: Val::Px(760.0),
            ..default()
        },
        Name::new("ModeSelectText"),
        ModeSelectText,
        MenuEntity,
    ));

    spawn_section_label(&mut commands, "Mode", MODE_ROW_TOP + 7.0);
    spawn_option(
        &mut commands,
        ModeSelectAction::SetMode(GameMode::OneVsOne),
        ClickRect {
            x: OPTION_LEFT,
            y: MODE_ROW_TOP,
            w: OPTION_W,
            h: OPTION_H,
        },
        "1v1",
        Color::srgba(0.53, 0.77, 0.96, 0.26),
    );
    spawn_option(
        &mut commands,
        ModeSelectAction::SetMode(GameMode::TwoVsTwo),
        ClickRect {
            x: OPTION_LEFT + OPTION_W + OPTION_GAP,
            y: MODE_ROW_TOP,
            w: OPTION_W,
            h: OPTION_H,
        },
        "2v2",
        Color::srgba(0.53, 0.77, 0.96, 0.26),
    );

    for player_index in 0..4usize {
        let row_top = COLOR_ROW_START_TOP + player_index as f32 * COLOR_ROW_GAP;
        spawn_section_label(
            &mut commands,
            &format!("P{} Color", player_index + 1),
            row_top + 5.0,
        );
        for (color_index, choice) in PlayerColorChoice::ALL.iter().enumerate() {
            let x = OPTION_LEFT + color_index as f32 * (COLOR_SWATCH_W + OPTION_GAP);
            spawn_option(
                &mut commands,
                ModeSelectAction::SetPlayerColor {
                    player_index,
                    color: *choice,
                },
                ClickRect {
                    x,
                    y: row_top,
                    w: COLOR_SWATCH_W,
                    h: COLOR_SWATCH_H,
                },
                "",
                choice.to_color(),
            );
        }
    }

    spawn_section_label(&mut commands, "Pieces / Player", PIECES_ROW_TOP + 7.0);
    for pieces in 1..=4u8 {
        let x = OPTION_LEFT + (pieces as f32 - 1.0) * (OPTION_W * 0.7 + OPTION_GAP);
        spawn_option(
            &mut commands,
            ModeSelectAction::SetPieces(pieces),
            ClickRect {
                x,
                y: PIECES_ROW_TOP,
                w: OPTION_W * 0.7,
                h: OPTION_H,
            },
            &pieces.to_string(),
            Color::srgba(0.53, 0.77, 0.96, 0.26),
        );
    }

    spawn_section_label(&mut commands, "Launch Rule", LAUNCH_RULE_ROW_TOP + 7.0);
    for (rule_index, launch_rule) in LaunchRule::ALL.iter().enumerate() {
        spawn_option(
            &mut commands,
            ModeSelectAction::SetLaunchRule(*launch_rule),
            ClickRect {
                x: OPTION_LEFT + rule_index as f32 * (OPTION_W + OPTION_GAP),
                y: LAUNCH_RULE_ROW_TOP,
                w: OPTION_W,
                h: OPTION_H,
            },
            launch_rule.label(),
            Color::srgba(0.53, 0.77, 0.96, 0.26),
        );
    }

    for player_index in 0..4usize {
        let row_top = CONTROL_ROW_START_TOP + player_index as f32 * CONTROL_ROW_GAP;
        spawn_section_label(
            &mut commands,
            &format!("P{} Control", player_index + 1),
            row_top + 7.0,
        );
        spawn_option(
            &mut commands,
            ModeSelectAction::SetPlayerControl {
                player_index,
                control: PlayerControl::Human,
            },
            ClickRect {
                x: OPTION_LEFT,
                y: row_top,
                w: OPTION_W,
                h: OPTION_H,
            },
            "Human",
            Color::srgba(0.53, 0.77, 0.96, 0.26),
        );
        spawn_option(
            &mut commands,
            ModeSelectAction::SetPlayerControl {
                player_index,
                control: PlayerControl::Ai,
            },
            ClickRect {
                x: OPTION_LEFT + OPTION_W + OPTION_GAP,
                y: row_top,
                w: OPTION_W,
                h: OPTION_H,
            },
            "AI",
            Color::srgba(0.53, 0.77, 0.96, 0.26),
        );
    }

    spawn_option(
        &mut commands,
        ModeSelectAction::StartMatch,
        ClickRect {
            x: OPTION_LEFT,
            y: BOTTOM_ROW_TOP,
            w: OPTION_W * 1.58,
            h: OPTION_H + 6.0,
        },
        "Start Match",
        Color::srgba(0.40, 0.72, 0.55, 0.40),
    );
    spawn_option(
        &mut commands,
        ModeSelectAction::Back,
        ClickRect {
            x: OPTION_LEFT + OPTION_W * 1.58 + OPTION_GAP,
            y: BOTTOM_ROW_TOP,
            w: OPTION_W * 1.2,
            h: OPTION_H + 6.0,
        },
        "Back",
        Color::srgba(0.72, 0.54, 0.44, 0.28),
    );
}

fn spawn_section_label(commands: &mut Commands, label: &str, top: f32) {
    // 左侧分区标题。
    commands.spawn((
        Text::new(label),
        TextFont {
            font_size: 20.0,
            ..default()
        },
        TextColor(Color::srgb(0.10, 0.16, 0.24)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(top),
            left: Val::Px(SECTION_LABEL_X),
            ..default()
        },
        Name::new(format!("ModeLabel{label}")),
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
) {
    // 通用方块渲染器：用于按钮底板与色块选项。
    let mut entity = commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(rect.x),
            top: Val::Px(rect.y),
            width: Val::Px(rect.w),
            height: Val::Px(rect.h),
            border: UiRect::all(Val::Px(2.0)),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            padding: UiRect::horizontal(Val::Px(6.0)),
            ..default()
        },
        BackgroundColor(color),
        BorderColor::all(Color::srgba(0.16, 0.22, 0.32, 0.20)),
        Name::new("MenuOptionBox"),
        MenuEntity,
    ));
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
            parent.spawn((
                Text::new(label),
                TextFont {
                    font_size,
                    ..default()
                },
                TextColor(Color::srgb(0.10, 0.16, 0.24)),
                TextLayout::new_with_justify(Justify::Center),
                Name::new("MenuOptionLabel"),
            ));
        });
    }
}

fn sound_settings_content(audio_settings: &AudioSettings) -> String {
    format!(
        "Background Music {}   |   Action Effects {}",
        format_volume_percent(audio_settings.music_volume),
        format_volume_percent(audio_settings.effects_volume)
    )
}

fn format_volume_percent(value: f32) -> String {
    format!("{:>3}%", (value.clamp(0.0, 1.0) * 100.0).round() as u8)
}

fn mode_select_content(match_setup: &MatchSetup) -> String {
    // 顶部摘要文本，实时反映当前配置状态。
    let mode = if match_setup.mode == GameMode::OneVsOne {
        "1v1"
    } else {
        "2v2"
    };
    let controls = match_setup.normalized_player_controls();
    let c = |control: PlayerControl| {
        if control == PlayerControl::Human {
            "Human"
        } else {
            "AI"
        }
    };

    let colors = match_setup.normalized_player_colors();

    let active = match_setup.active_player_count();
    let player_summary = (0..4usize)
        .map(|index| {
            let state = if index < active { "" } else { " off" };
            format!(
                "P{} {}/{}{}",
                index + 1,
                colors[index].label(),
                c(controls[index]),
                state
            )
        })
        .collect::<Vec<_>>()
        .join("   ");

    format!(
        "Match Setup\n\
Mode {mode} | Pieces {} | Launch {} | unique colors | at least 1 human\n\
{player_summary}",
        match_setup.pieces_per_player,
        match_setup.launch_rule.label(),
    )
}

fn update_mode_select_text(
    match_setup: Res<MatchSetup>,
    mut query: Query<&mut Text, With<ModeSelectText>>,
) {
    // 配置变更后刷新顶部摘要。
    for mut text in &mut query {
        *text = Text::new(mode_select_content(&match_setup));
    }
}

fn update_mode_select_option_visuals(
    match_setup: Res<MatchSetup>,
    mut option_query: Query<(&ModeSelectOption, &mut BackgroundColor, &mut BorderColor)>,
) {
    // 配置变更后刷新所有选项的高亮/禁用态。
    for (option, mut color, mut border) in &mut option_query {
        *color = BackgroundColor(option_fill_color(option, &match_setup));
        *border = BorderColor::all(option_border_color(option, &match_setup));
    }
}

fn option_fill_color(option: &ModeSelectOption, match_setup: &MatchSetup) -> Color {
    // 颜色优先级：禁用 > 选中 > 普通。
    if action_disabled(option.action, match_setup) {
        return option.base_color.with_alpha(0.15);
    }
    if action_selected(option.action, match_setup) {
        return option.base_color.mix(&Color::WHITE, 0.20).with_alpha(0.95);
    }
    option.base_color.with_alpha(0.58)
}

fn option_border_color(option: &ModeSelectOption, match_setup: &MatchSetup) -> Color {
    if action_disabled(option.action, match_setup) {
        return Color::srgba(0.30, 0.35, 0.42, 0.16);
    }
    if action_selected(option.action, match_setup) {
        return Color::srgba(0.06, 0.10, 0.16, 0.95);
    }
    Color::srgba(0.18, 0.24, 0.34, 0.34)
}

fn action_disabled(action: ModeSelectAction, match_setup: &MatchSetup) -> bool {
    // 1v1 模式下禁用 P3/P4 的人机控制项。
    match action {
        ModeSelectAction::SetPlayerControl { player_index, .. } => {
            player_index >= match_setup.active_player_count()
        }
        _ => false,
    }
}

fn action_selected(action: ModeSelectAction, match_setup: &MatchSetup) -> bool {
    // 判断某个选项是否与当前配置一致（用于高亮）。
    match action {
        ModeSelectAction::SetMode(mode) => match_setup.mode == mode,
        ModeSelectAction::SetPlayerColor {
            player_index,
            color,
        } => match_setup.player_color_choice(player_index) == Some(color),
        ModeSelectAction::SetPieces(pieces) => match_setup.pieces_per_player == pieces,
        ModeSelectAction::SetLaunchRule(launch_rule) => match_setup.launch_rule == launch_rule,
        ModeSelectAction::SetPlayerControl {
            player_index,
            control,
        } => match_setup.player_control(player_index) == Some(control),
        _ => false,
    }
}

fn handle_main_menu_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut next_state: ResMut<NextState<AppState>>,
    mut overlay_state: ResMut<SoundSettingsOverlayState>,
) {
    if overlay_state.open {
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
    // 鼠标主操作：点击开始进入配置；声音设置由全局 Audio 入口打开。
    if overlay_state.input_captured {
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
        });
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
        SoundSettingsAction::MusicDown => audio_settings.adjust_music(-AudioSettings::STEP),
        SoundSettingsAction::MusicUp => audio_settings.adjust_music(AudioSettings::STEP),
        SoundSettingsAction::EffectsDown => audio_settings.adjust_effects(-AudioSettings::STEP),
        SoundSettingsAction::EffectsUp => audio_settings.adjust_effects(AudioSettings::STEP),
        SoundSettingsAction::Back => next_state.set(AppState::MainMenu),
    }
}

fn handle_mode_select_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut match_setup: ResMut<MatchSetup>,
    mut next_state: ResMut<NextState<AppState>>,
    overlay_state: Res<SoundSettingsOverlayState>,
) {
    if overlay_state.open {
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
    if overlay_state.input_captured {
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
            match_setup.sanitize_player_colors();
        }
        ModeSelectAction::SetPlayerColor {
            player_index,
            color,
        } => match_setup.set_player_color(player_index, color),
        ModeSelectAction::SetPieces(pieces) => match_setup.pieces_per_player = pieces.clamp(1, 4),
        ModeSelectAction::SetLaunchRule(launch_rule) => match_setup.launch_rule = launch_rule,
        ModeSelectAction::SetPlayerControl {
            player_index,
            control,
        } => match_setup.set_player_control(player_index, control),
        ModeSelectAction::StartMatch => {
            match_setup.sanitize_player_controls();
            match_setup.sanitize_player_colors();
            next_state.set(AppState::LoadingGame);
        }
        ModeSelectAction::Back => next_state.set(AppState::MainMenu),
    }
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
            ai_difficulty: AiDifficulty::Normal,
            fast_mode: false,
            launch_rule: LaunchRule::SixOnly,
            player_colors: [
                PlayerColorChoice::Blue,
                PlayerColorChoice::Red,
                PlayerColorChoice::Green,
                PlayerColorChoice::Yellow,
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
        let summary_bottom = 22.0 + 88.0;
        assert!(summary_bottom < MODE_ROW_TOP);

        let mut rows = vec![(MODE_ROW_TOP, MODE_ROW_TOP + OPTION_H)];
        rows.extend((0..4).map(|index| {
            let top = COLOR_ROW_START_TOP + index as f32 * COLOR_ROW_GAP;
            (top, top + COLOR_SWATCH_H)
        }));
        rows.push((PIECES_ROW_TOP, PIECES_ROW_TOP + OPTION_H));
        rows.push((LAUNCH_RULE_ROW_TOP, LAUNCH_RULE_ROW_TOP + OPTION_H));
        rows.extend((0..4).map(|index| {
            let top = CONTROL_ROW_START_TOP + index as f32 * CONTROL_ROW_GAP;
            (top, top + OPTION_H)
        }));
        rows.push((BOTTOM_ROW_TOP, BOTTOM_ROW_TOP + OPTION_H + 6.0));

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
    }

    #[test]
    fn main_menu_start_button_leaves_room_for_global_sound_entry() {
        let window = test_window();
        let start = main_menu_start_rect(window.width());
        let audio = global_sound_entry_rect(&window, &AppState::MainMenu);

        assert!(audio.y + audio.h + 40.0 <= start.y);
        assert!(start.y + start.h <= 720.0);
        assert!(start.x > MENU_LEFT);
    }

    #[test]
    fn global_sound_entry_moves_away_from_ingame_hud() {
        let window = test_window();
        let menu_entry = global_sound_entry_rect(&window, &AppState::MainMenu);
        let ingame_entry = global_sound_entry_rect(&window, &AppState::InGame);

        assert!(menu_entry.x > window.width() * 0.5);
        assert_eq!(ingame_entry.x, GLOBAL_SOUND_ENTRY_LEFT);
        assert!(ingame_entry.x + ingame_entry.w < window.width() * 0.5);
    }

    #[test]
    fn sound_settings_actions_adjust_independent_channels() {
        let mut audio_settings = AudioSettings {
            music_volume: 0.5,
            effects_volume: 0.5,
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

        assert!((audio_settings.music_volume - 0.6).abs() < f32::EPSILON);
        assert!((audio_settings.effects_volume - 0.4).abs() < f32::EPSILON);
    }

    #[test]
    fn global_sound_actions_adjust_audio_and_close_overlay() {
        let mut audio_settings = AudioSettings {
            music_volume: 0.5,
            effects_volume: 0.5,
        };
        let mut overlay_state = SoundSettingsOverlayState {
            open: true,
            input_captured: false,
        };

        apply_global_sound_action(
            SoundSettingsAction::MusicDown,
            &mut audio_settings,
            &mut overlay_state,
        );
        apply_global_sound_action(
            SoundSettingsAction::EffectsUp,
            &mut audio_settings,
            &mut overlay_state,
        );

        assert!((audio_settings.music_volume - 0.4).abs() < f32::EPSILON);
        assert!((audio_settings.effects_volume - 0.6).abs() < f32::EPSILON);
        assert!(overlay_state.open);

        apply_global_sound_action(
            SoundSettingsAction::Back,
            &mut audio_settings,
            &mut overlay_state,
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
    fn global_sound_entry_and_panel_actions_have_stable_hit_targets() {
        let window = test_window();
        let entry = global_sound_entry_rect(&window, &AppState::MainMenu);
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

        let close = Vec2::new(
            panel.x + GLOBAL_SOUND_PANEL_W - GLOBAL_SOUND_ROW_LEFT - 8.0,
            panel.y + GLOBAL_SOUND_PANEL_H - 42.0,
        );
        assert_eq!(
            global_sound_action_at(close, &window),
            Some(SoundSettingsAction::Back)
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
}
