use bevy::app::AppExit;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;

use crate::data::game_mode::GameMode;
use crate::data::rule_set::RuleSet;
use crate::domain::player::PlayerControl;
use crate::domain::rules::LaunchRule;
use crate::gameplay::ai::AiDifficulty;
use crate::gameplay::match_flow::{MatchSetup, PlayerSeat};
use crate::i18n::{
    Language, LanguageSettings, LocalizedText, TextKey, ai_difficulty_label as i18n_ai_label,
    launch_rule_label, mode_label, rule_set_label, text,
};
use crate::platform::{PointerInputState, PointerSource};
use crate::plugins::audio_plugin::AudioSettings;
use crate::plugins::performance_plugin::{
    PerformanceSettings, fps_toggle_label, fps_toggle_label_for_language,
};
use crate::states::AppState;
use crate::ui::game_layout::{
    GLOBAL_SETTINGS_MARGIN, GLOBAL_SETTINGS_RADIUS, GLOBAL_SETTINGS_SIZE, global_settings_rect,
};

/// 菜单插件：主菜单与开局配置页的渲染和交互。
pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SoundSettingsOverlayState>()
            .init_resource::<SettingsEntryState>()
            .init_resource::<ModeSelectRenderState>()
            .add_systems(Startup, spawn_global_sound_overlay)
            .add_systems(
                PreUpdate,
                update_sound_overlay_input_capture.after(crate::platform::PlatformInputSet),
            )
            .add_systems(
                Update,
                (
                    update_settings_entry,
                    update_global_sound_overlay,
                    update_global_settings_scroll,
                    handle_global_sound_overlay_input,
                    handle_global_sound_overlay_click,
                )
                    .chain(),
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
                    update_compact_mode_scroll.run_if(in_state(AppState::ModeSelect)),
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
    language: Language,
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
struct GlobalSettingsHint;

#[derive(Resource, Default)]
struct SettingsEntryState {
    hover_started_at: Option<f32>,
    press_started_at: Option<f32>,
    press_position: Option<Vec2>,
    press_cancelled: bool,
    long_pressed: bool,
    hint_until: f32,
    hint_visible: bool,
}

impl SettingsEntryState {
    fn update(
        &mut self,
        pointer: PointerInputState,
        cursor: Option<Vec2>,
        rect: ClickRect,
        now: f32,
    ) {
        let touch = pointer.current_source() == Some(PointerSource::Touch);
        let position = if touch {
            pointer.current_position()
        } else {
            cursor
        };
        let inside = position.is_some_and(|p| rect.contains(p));
        if pointer.just_pressed() {
            *self = Self::default();
            self.press_position = pointer.just_pressed_position();
            self.press_started_at =
                (touch && self.press_position.is_some_and(|p| rect.contains(p))).then_some(now);
            self.press_cancelled = self.press_started_at.is_none();
        }
        if touch {
            self.hover_started_at = None;
            if self.press_started_at.is_some()
                && (!inside
                    || self
                        .press_position
                        .zip(position)
                        .is_some_and(|(a, b)| a.distance(b) >= 12.0))
            {
                self.press_cancelled = true;
                self.hint_until = 0.0;
            }
            if !self.press_cancelled
                && self
                    .press_started_at
                    .is_some_and(|start| now - start >= SETTINGS_HINT_DELAY)
                && (pointer.is_pressed() || pointer.just_released())
            {
                self.long_pressed = true;
                self.hint_until = now + SETTINGS_HINT_HOLD;
            }
            self.hint_visible = inside && !self.press_cancelled && now < self.hint_until;
        } else {
            self.hover_started_at = if inside && !pointer.is_pressed() {
                Some(self.hover_started_at.unwrap_or(now))
            } else {
                None
            };
            self.hint_visible = self
                .hover_started_at
                .is_some_and(|start| now - start >= SETTINGS_HINT_DELAY);
        }
    }

    fn allows_touch_tap(&self) -> bool {
        self.press_started_at.is_some() && !self.press_cancelled && !self.long_pressed
    }
}

#[derive(Component)]
/// 全局声音弹窗实体。
struct GlobalSoundModal;

#[derive(Component)]
struct GlobalSettingsViewport;

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
    Language,
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
struct CompactModeViewport;

#[derive(Component)]
struct CompactModeItem;

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
    CycleLanguage,
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

const SETTINGS_ICON_SIZE: f32 = 24.0;
const SETTINGS_ICON_COLOR: Color = Color::srgb(0.26, 0.33, 0.43);
const SETTINGS_ENTRY_FILL: Color = Color::srgb(0.96, 0.975, 0.99);
const SETTINGS_ENTRY_HOVER_FILL: Color = Color::srgb(0.87, 0.91, 0.96);
const SETTINGS_ENTRY_PRESSED_FILL: Color = Color::srgb(0.78, 0.85, 0.93);
const SETTINGS_HINT_DELAY: f32 = 0.52;
const SETTINGS_HINT_HOLD: f32 = 1.2;
const GLOBAL_SOUND_PANEL_W: f32 = 320.0;
const GLOBAL_SOUND_PANEL_H: f32 = 506.0;
const GLOBAL_SOUND_ROW_LEFT: f32 = 16.0;
const GLOBAL_SOUND_CONTROL_LEFT: f32 = 116.0;
const GLOBAL_SOUND_ROW_TOP: f32 = 98.0;
const GLOBAL_SOUND_ROW_GAP: f32 = 58.0;
const GLOBAL_SOUND_MUTE_ROW_TOP: f32 = GLOBAL_SOUND_ROW_TOP + GLOBAL_SOUND_ROW_GAP * 2.0;
const GLOBAL_SOUND_FPS_ROW_TOP: f32 = GLOBAL_SOUND_ROW_TOP + GLOBAL_SOUND_ROW_GAP * 3.0;
const GLOBAL_SOUND_LANGUAGE_ROW_TOP: f32 = GLOBAL_SOUND_ROW_TOP + GLOBAL_SOUND_ROW_GAP * 4.0;
const GLOBAL_SOUND_BUTTON: f32 = 48.0;
const GLOBAL_SOUND_VALUE_W: f32 = 64.0;
const GLOBAL_SOUND_TOGGLE_W: f32 = 180.0;
const GLOBAL_SETTINGS_ACTION_TOP: f32 = 424.0;
const GLOBAL_SETTINGS_ACTION_W: f32 = 128.0;
const GLOBAL_SETTINGS_ACTION_H: f32 = 48.0;
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
const OPTION_H: f32 = 48.0;
const OPTION_GAP: f32 = 12.0;
const OPTION_LABEL_SAFETY_PX: f32 = 8.0;
const MODE_ROW_TOP: f32 = 72.0;
const RULE_SET_ROW_TOP: f32 = MODE_ROW_TOP + SETTING_ROW_GAP;
const PLAYER_ROW_START_TOP: f32 = RULE_SET_ROW_TOP + SETTING_ROW_GAP;
const PLAYER_ROW_GAP: f32 = 60.0;
const PLAYER_COLOR_LEFT: f32 = 250.0;
const PLAYER_CONTROL_LEFT: f32 = 486.0;
const PLAYER_CONTROL_W: f32 = 92.0;
const PLAYER_CONTROL_GAP: f32 = 10.0;
const PLAYER_SETTINGS_GAP: f32 = 26.0;
const SETTING_ROW_GAP: f32 = 60.0;
const COLOR_SWATCH_W: f32 = 48.0;
const COLOR_SWATCH_H: f32 = 48.0;
const MODE_LAYOUT_BASE_LEFT: f32 = MENU_LEFT;
const MODE_LAYOUT_BASE_TOP: f32 = MODE_ROW_TOP;
const SETTING_ROW_BAND_LEFT: f32 = 72.0;
const SETTING_ROW_BAND_W: f32 = 666.0;
const SETTING_ROW_BAND_H: f32 = 52.0;
const MODE_LAYOUT_VISIBLE_LEFT: f32 = SETTING_ROW_BAND_LEFT;
const MODE_LAYOUT_VISIBLE_W: f32 = SETTING_ROW_BAND_W;
const BOTTOM_ACTION_W: f32 = 150.0;
const BOTTOM_ACTION_H: f32 = OPTION_H + 6.0;
const MODE_SELECT_BLACK: Color = Color::BLACK;
const MODE_SELECT_UNSELECTED_TEXT: Color = Color::srgb(0.18, 0.24, 0.34);
const MODE_SELECT_DISABLED_TEXT: Color = Color::srgba(0.18, 0.24, 0.34, 0.42);

/// Code-native gear: crisp at any DPI, without a platform-dependent Unicode glyph.
fn spawn_settings_gear(parent: &mut ChildSpawnerCommands) {
    parent
        .spawn((
            Node {
                width: Val::Px(SETTINGS_ICON_SIZE),
                height: Val::Px(SETTINGS_ICON_SIZE),
                ..default()
            },
            Name::new("GlobalSettingsGear"),
        ))
        .with_children(|gear| {
            for i in 0..8 {
                let angle = i as f32 * std::f32::consts::FRAC_PI_4;
                let center = Vec2::splat(12.0) + Vec2::new(angle.cos(), angle.sin()) * 9.0;
                gear.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(center.x - 2.5),
                        top: Val::Px(center.y - 2.0),
                        width: Val::Px(5.0),
                        height: Val::Px(4.0),
                        border_radius: BorderRadius::all(Val::Px(1.0)),
                        ..default()
                    },
                    UiTransform::from_rotation(Rot2::radians(angle)),
                    BackgroundColor(SETTINGS_ICON_COLOR),
                ));
            }
            gear.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(3.0),
                    top: Val::Px(3.0),
                    width: Val::Px(18.0),
                    height: Val::Px(18.0),
                    border: UiRect::all(Val::Px(5.0)),
                    border_radius: BorderRadius::MAX,
                    ..default()
                },
                BorderColor::all(SETTINGS_ICON_COLOR),
            ));
        });
}

fn settings_hint_rect(width: f32) -> ClickRect {
    let entry = global_settings_rect(width);
    ClickRect {
        x: entry.x - 8.0 - 88.0,
        y: entry.y + 8.0,
        w: 88.0,
        h: 32.0,
    }
}

fn update_settings_entry(
    windows: Query<&Window>,
    pointer: Res<PointerInputState>,
    time: Res<Time>,
    app_state: Res<State<AppState>>,
    overlay: Res<SoundSettingsOverlayState>,
    mut state: ResMut<SettingsEntryState>,
    mut last_page: Local<Option<AppState>>,
    mut entry: Query<&mut BackgroundColor, With<GlobalSoundEntry>>,
    mut hints: Query<(&mut Node, &mut Visibility), With<GlobalSettingsHint>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    if *last_page != Some(*app_state.get()) {
        *state = SettingsEntryState::default();
        *last_page = Some(*app_state.get());
    }
    let rect = global_sound_entry_rect(window);
    if overlay.open || matches!(app_state.get(), AppState::Boot) {
        *state = SettingsEntryState::default();
    } else {
        state.update(
            *pointer,
            window.cursor_position(),
            rect,
            time.elapsed_secs(),
        );
    }
    let point = if pointer.current_source() == Some(PointerSource::Touch) {
        pointer.current_position().filter(|_| pointer.is_pressed())
    } else {
        window.cursor_position()
    };
    let hovered = point.is_some_and(|p| rect.contains(p));
    for mut fill in &mut entry {
        fill.0 = if overlay.open || (hovered && pointer.is_pressed()) {
            SETTINGS_ENTRY_PRESSED_FILL
        } else if hovered {
            SETTINGS_ENTRY_HOVER_FILL
        } else {
            SETTINGS_ENTRY_FILL
        };
    }
    for (mut node, mut visibility) in &mut hints {
        let hint = settings_hint_rect(window.width());
        node.left = Val::Px(hint.x);
        node.top = Val::Px(hint.y);
        *visibility = if state.hint_visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

fn spawn_global_sound_overlay(mut commands: Commands, language_settings: Res<LanguageSettings>) {
    let language = language_settings.language;
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(GLOBAL_SETTINGS_MARGIN),
                top: Val::Px(GLOBAL_SETTINGS_MARGIN),
                width: Val::Px(GLOBAL_SETTINGS_SIZE),
                height: Val::Px(GLOBAL_SETTINGS_SIZE),
                border_radius: BorderRadius::all(Val::Px(GLOBAL_SETTINGS_RADIUS)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                padding: UiRect::horizontal(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(SETTINGS_ENTRY_FILL),
            ZIndex(80),
            Visibility::Hidden,
            Name::new("GlobalSoundEntry"),
            GlobalSoundEntry,
            GlobalSoundEntity,
        ))
        .with_children(spawn_settings_gear);

    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: Val::Px(88.0),
            height: Val::Px(32.0),
            padding: UiRect::top(Val::Px(6.0)),
            border_radius: BorderRadius::all(Val::Px(8.0)),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        Text::new(text(language, TextKey::Settings)),
        TextFont {
            font_size: FontSize::Px(14.0),
            ..default()
        },
        TextColor(Color::WHITE),
        TextLayout::justify(Justify::Center),
        BackgroundColor(Color::srgb(0.19, 0.24, 0.32)),
        LocalizedText {
            key: TextKey::Settings,
        },
        Visibility::Hidden,
        ZIndex(90),
        GlobalSettingsHint,
        GlobalSoundEntity,
        Name::new("GlobalSettingsHint"),
    ));

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
                        overflow: Overflow::scroll_y(),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.98, 0.99, 1.0, 0.98)),
                    BorderColor::all(Color::srgba(0.34, 0.42, 0.55, 0.42)),
                    ZIndex(91),
                    Name::new("GlobalSoundPanel"),
                    ScrollPosition::default(),
                    GlobalSettingsViewport,
                ))
                .with_children(|panel| {
                    panel.spawn(Node {
                        width: Val::Px(1.0),
                        height: Val::Px(GLOBAL_SOUND_PANEL_H),
                        flex_shrink: 0.0,
                        ..default()
                    });
                    spawn_global_sound_panel_button(
                        panel,
                        global_settings_close_rect(),
                        "×",
                        None,
                        24.0,
                    );
                    panel.spawn((
                        Text::new(text(language, TextKey::Settings)),
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
                        LocalizedText {
                            key: TextKey::Settings,
                        },
                        Name::new("GlobalSoundTitle"),
                    ));
                    panel.spawn((
                        Text::new(text(language, TextKey::Audio)),
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
                        LocalizedText {
                            key: TextKey::Audio,
                        },
                        Name::new("GlobalSoundAudioSection"),
                    ));

                    spawn_global_sound_row(
                        panel,
                        text(language, TextKey::Music),
                        TextKey::Music,
                        SoundSettingsValueKind::Music,
                        GLOBAL_SOUND_ROW_TOP,
                    );
                    spawn_global_sound_row(
                        panel,
                        text(language, TextKey::Effects),
                        TextKey::Effects,
                        SoundSettingsValueKind::Effects,
                        GLOBAL_SOUND_ROW_TOP + GLOBAL_SOUND_ROW_GAP,
                    );
                    spawn_global_sound_toggle_row(
                        panel,
                        text(language, TextKey::Mute),
                        TextKey::Mute,
                        SoundSettingsValueKind::Mute,
                        GLOBAL_SOUND_MUTE_ROW_TOP,
                        language,
                    );
                    spawn_global_sound_toggle_row(
                        panel,
                        text(language, TextKey::FpsCounter),
                        TextKey::FpsCounter,
                        SoundSettingsValueKind::Fps,
                        GLOBAL_SOUND_FPS_ROW_TOP,
                        language,
                    );
                    spawn_global_language_row(
                        panel,
                        text(language, TextKey::Language),
                        TextKey::Language,
                        GLOBAL_SOUND_LANGUAGE_ROW_TOP,
                        language,
                    );

                    spawn_global_sound_panel_button(
                        panel,
                        global_settings_main_menu_rect(),
                        text(language, TextKey::MainMenu),
                        Some(TextKey::MainMenu),
                        18.0,
                    );
                    spawn_global_sound_panel_button(
                        panel,
                        global_settings_quit_game_rect(),
                        text(language, TextKey::QuitGame),
                        Some(TextKey::QuitGame),
                        18.0,
                    );
                });
        });
}

fn spawn_global_sound_toggle_row(
    panel: &mut ChildSpawnerCommands<'_>,
    label: &str,
    label_key: TextKey,
    value_kind: SoundSettingsValueKind,
    top: f32,
    language: Language,
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
        LocalizedText { key: label_key },
        Name::new(format!("GlobalSoundLabel{label}")),
    ));

    let rect = ClickRect {
        x: GLOBAL_SOUND_CONTROL_LEFT,
        y: top,
        w: GLOBAL_SOUND_TOGGLE_W,
        h: GLOBAL_SOUND_BUTTON,
    };
    let state = global_sound_toggle_initial_state(value_kind, language);
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

fn spawn_global_language_row(
    panel: &mut ChildSpawnerCommands<'_>,
    label: &str,
    label_key: TextKey,
    top: f32,
    language: Language,
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
        LocalizedText { key: label_key },
        Name::new(format!("GlobalSoundLabel{label}")),
    ));

    panel
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(GLOBAL_SOUND_CONTROL_LEFT),
                top: Val::Px(top),
                width: Val::Px(GLOBAL_SOUND_TOGGLE_W),
                height: Val::Px(GLOBAL_SOUND_BUTTON),
                border: UiRect::all(Val::Px(1.0)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                padding: UiRect::horizontal(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.55, 0.70, 0.88, 0.22)),
            BorderColor::all(Color::srgba(0.22, 0.30, 0.42, 0.24)),
            Name::new("GlobalSoundLanguageButton"),
        ))
        .with_children(|button| {
            button.spawn((
                Text::new(language.label()),
                TextFont {
                    font_size: FontSize::Px(18.0),
                    ..default()
                },
                TextColor(Color::srgb(0.10, 0.16, 0.24)),
                TextLayout::justify(Justify::Center),
                SoundSettingsValueText {
                    kind: SoundSettingsValueKind::Language,
                },
                Name::new("GlobalSoundLanguageValue"),
            ));
        });
}

fn global_sound_toggle_initial_state(
    value_kind: SoundSettingsValueKind,
    language: Language,
) -> SettingsToggleVisualState {
    match value_kind {
        SoundSettingsValueKind::Mute => SettingsToggleVisualState {
            active: false,
            label: format_mute_label_for_language(false, language),
        },
        SoundSettingsValueKind::Fps => SettingsToggleVisualState {
            active: cfg!(debug_assertions),
            label: if cfg!(debug_assertions) {
                fps_toggle_label_for_language(true, language)
            } else {
                fps_toggle_label_for_language(false, language)
            },
        },
        SoundSettingsValueKind::Music
        | SoundSettingsValueKind::Effects
        | SoundSettingsValueKind::Language => SettingsToggleVisualState {
            active: false,
            label: "",
        },
    }
}

fn spawn_global_sound_row(
    panel: &mut ChildSpawnerCommands<'_>,
    label: &str,
    label_key: TextKey,
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
        LocalizedText { key: label_key },
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
        None,
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
        None,
        24.0,
    );
}

fn spawn_global_sound_panel_button(
    panel: &mut ChildSpawnerCommands<'_>,
    rect: ClickRect,
    label: &str,
    label_key: Option<TextKey>,
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
            let mut text_entity = button.spawn((
                Text::new(label),
                TextFont {
                    font_size: FontSize::Px(font_size),
                    ..default()
                },
                TextColor(Color::srgb(0.10, 0.16, 0.24)),
                TextLayout::justify(Justify::Center),
                Name::new("GlobalSoundButtonLabel"),
            ));
            if let Some(label_key) = label_key {
                text_entity.insert(LocalizedText { key: label_key });
            }
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
            || pointer.just_released()
            || keyboard.just_pressed(KeyCode::Escape)
            || keyboard.just_pressed(KeyCode::Backspace))
    {
        overlay_state.input_captured = true;
        return;
    }

    let Some(cursor) = pointer
        .just_pressed_position()
        .or(pointer.just_released_position())
    else {
        return;
    };
    let Ok(window) = windows.single() else {
        return;
    };

    overlay_state.input_captured = global_sound_entry_rect(window).contains(cursor);
}

fn update_global_sound_overlay(
    windows: Query<&Window>,
    app_state: Res<State<AppState>>,
    audio_settings: Res<AudioSettings>,
    performance_settings: Res<PerformanceSettings>,
    language_settings: Res<LanguageSettings>,
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
        if let Ok(window) = windows.single() {
            let rect = global_sound_entry_rect(window);
            node.left = Val::Px(rect.x);
            node.right = Val::Auto;
            node.top = Val::Px(rect.y);
            node.width = Val::Px(rect.w);
            node.height = Val::Px(rect.h);
        }
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
        && !language_settings.is_changed()
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
                    language_settings.language,
                )
                .map_or_else(String::new, |state| state.label.to_owned())
            }
            SoundSettingsValueKind::Language => language_settings.label().to_owned(),
        });
    }
    for (track, mut background, mut border) in &mut toggle_track_query {
        let Some(state) = global_settings_toggle_state(
            track.kind,
            &audio_settings,
            &performance_settings,
            language_settings.language,
        ) else {
            continue;
        };
        *background = BackgroundColor(settings_toggle_track_color(state.active));
        *border = BorderColor::all(settings_toggle_track_border_color(state.active));
    }
    for (thumb, mut node) in &mut toggle_thumb_query {
        let Some(state) = global_settings_toggle_state(
            thumb.kind,
            &audio_settings,
            &performance_settings,
            language_settings.language,
        ) else {
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
    mut language_settings: ResMut<LanguageSettings>,
    mut overlay_state: ResMut<SoundSettingsOverlayState>,
    mut next_app_state: ResMut<NextState<AppState>>,
    mut app_exit: MessageWriter<AppExit>,
    scroll: Query<&ScrollPosition, With<GlobalSettingsViewport>>,
    mut touch_start: Local<Option<Vec2>>,
    entry_state: Res<SettingsEntryState>,
) {
    let position = if pointer.current_source() == Some(crate::platform::PointerSource::Touch) {
        if pointer.just_pressed() {
            *touch_start = pointer.just_pressed_position();
        }
        pointer
            .just_released_position()
            .filter(|p| touch_start.is_some_and(|start| start.distance(*p) < 12.0))
    } else {
        pointer.just_pressed_position()
    };
    let Some(cursor) = position else {
        return;
    };
    let Ok(window) = windows.single() else {
        return;
    };

    if overlay_state.open {
        overlay_state.input_captured = true;
        let scroll_y = scroll.single().map(|s| s.y).unwrap_or(0.0);
        if global_sound_panel_rect(window).contains(cursor)
            && let Some(action) = global_sound_action_at(cursor + Vec2::Y * scroll_y, window)
        {
            match apply_global_sound_action(
                action,
                &mut audio_settings,
                &mut performance_settings,
                &mut language_settings,
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
        // A long press only explains the icon. A later short tap opens settings.
        if pointer.current_source() == Some(PointerSource::Touch) && !entry_state.allows_touch_tap()
        {
            return;
        }
        overlay_state.open = true;
        overlay_state.input_captured = true;
    }
}

fn apply_global_sound_action(
    action: SoundSettingsAction,
    audio_settings: &mut AudioSettings,
    performance_settings: &mut PerformanceSettings,
    language_settings: &mut LanguageSettings,
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
        SoundSettingsAction::CycleLanguage => language_settings.cycle(),
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

fn global_sound_entry_rect(window: &Window) -> ClickRect {
    let (x, y, w, h) = global_settings_entry_screen_rect(window.width(), window.height());
    ClickRect { x, y, w, h }
}

pub fn global_settings_entry_screen_rect(
    window_width: f32,
    _window_height: f32,
) -> (f32, f32, f32, f32) {
    let rect = global_settings_rect(window_width);
    (rect.x, rect.y, rect.w, rect.h)
}

fn global_sound_panel_rect(window: &Window) -> ClickRect {
    let height = GLOBAL_SOUND_PANEL_H.min((window.height() - 32.0).max(120.0));
    ClickRect {
        x: (window.width() - GLOBAL_SOUND_PANEL_W) * 0.5,
        y: (window.height() - height) * 0.5,
        w: GLOBAL_SOUND_PANEL_W,
        h: height,
    }
}

fn update_global_settings_scroll(
    windows: Query<&Window>,
    overlay: Res<SoundSettingsOverlayState>,
    pointer: Res<PointerInputState>,
    mut wheel: MessageReader<MouseWheel>,
    mut drag: Local<Option<Vec2>>,
    mut panels: Query<
        (&mut Node, &mut ScrollPosition, &ComputedNode),
        With<GlobalSettingsViewport>,
    >,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let panel = global_sound_panel_rect(window);
    for (mut node, mut scroll, computed) in &mut panels {
        node.height = Val::Px(panel.h);
        if !overlay.open {
            scroll.y = 0.0;
            *drag = None;
            wheel.clear();
            continue;
        }
        let Some(point) = pointer.current_position().filter(|p| panel.contains(*p)) else {
            wheel.clear();
            *drag = None;
            continue;
        };
        let max = ((computed.content_size().y - computed.size().y)
            * computed.inverse_scale_factor())
        .max(0.0);
        for event in wheel.read() {
            let delta = match event.unit {
                MouseScrollUnit::Line => event.y * 24.0,
                MouseScrollUnit::Pixel => event.y,
            };
            scroll.y = (scroll.y - delta).clamp(0.0, max);
        }
        if pointer.is_pressed()
            && pointer.current_source() == Some(crate::platform::PointerSource::Touch)
        {
            if !pointer.just_pressed()
                && let Some(previous) = *drag
            {
                scroll.y = (scroll.y + previous.y - point.y).clamp(0.0, max);
            }
            *drag = Some(point);
        } else {
            *drag = None;
        }
    }
}

fn global_settings_action_start_x() -> f32 {
    (GLOBAL_SOUND_PANEL_W - GLOBAL_SETTINGS_ACTION_W * 2.0 - GLOBAL_SETTINGS_ACTION_GAP) * 0.5
}

fn global_settings_close_rect() -> ClickRect {
    ClickRect {
        x: GLOBAL_SOUND_PANEL_W - 64.0,
        y: 16.0,
        w: 48.0,
        h: 48.0,
    }
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
        (SoundSettingsAction::Back, global_settings_close_rect()),
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
            SoundSettingsAction::CycleLanguage,
            ClickRect {
                x: GLOBAL_SOUND_CONTROL_LEFT,
                y: GLOBAL_SOUND_LANGUAGE_ROW_TOP,
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

fn spawn_main_menu(
    mut commands: Commands,
    windows: Query<&Window>,
    language_settings: Res<LanguageSettings>,
) {
    // 主菜单：标题 + 开始按钮按当前窗口居中。
    let (window_width, window_height) = windows
        .single()
        .map(|window| (window.width(), window.height()))
        .unwrap_or((1280.0, 720.0));
    let title_rect = main_menu_title_rect(window_width, window_height);
    let start_rect = main_menu_start_rect(window_width, window_height);

    commands.spawn((
        Text::new(text(language_settings.language, TextKey::GameTitle)),
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
        LocalizedText {
            key: TextKey::GameTitle,
        },
        MainMenuTitleNode,
        MenuEntity,
    ));

    let start_button = spawn_box_with_label(
        &mut commands,
        start_rect,
        Color::srgba(0.42, 0.61, 0.88, 0.30),
        text(language_settings.language, TextKey::StartMatch),
        30.0,
        Some(TextKey::StartMatch),
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
        GLOBAL_SETTINGS_MARGIN + GLOBAL_SETTINGS_SIZE + 24.0,
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
    language: Language,
) -> ModeSelectRenderKey {
    let (window_width, window_height) = windows
        .single()
        .map(|window| (window.width(), window.height()))
        .unwrap_or((1280.0, 720.0));
    mode_select_render_key_from_size(window_width, window_height, match_setup, language)
}

fn mode_select_render_key_from_size(
    window_width: f32,
    window_height: f32,
    match_setup: &MatchSetup,
    language: Language,
) -> ModeSelectRenderKey {
    ModeSelectRenderKey {
        mode: match_setup.mode,
        active_player_count: match_setup.active_player_count(),
        language,
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

fn spawn_sound_settings(
    mut commands: Commands,
    audio_settings: Res<AudioSettings>,
    language_settings: Res<LanguageSettings>,
) {
    let language = language_settings.language;
    commands.spawn((
        Text::new(text(language, TextKey::SoundSettings)),
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
        Text::new(sound_settings_content(&audio_settings, language)),
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
        text(language, TextKey::BackgroundMusic),
        SoundSettingsValueKind::Music,
        SoundSettingsAction::MusicDown,
        SoundSettingsAction::MusicUp,
        SOUND_PANEL_TOP,
        audio_settings.music_volume,
    );
    spawn_sound_row(
        &mut commands,
        text(language, TextKey::ActionEffects),
        SoundSettingsValueKind::Effects,
        SoundSettingsAction::EffectsDown,
        SoundSettingsAction::EffectsUp,
        SOUND_PANEL_TOP + SOUND_ROW_GAP,
        audio_settings.effects_volume,
    );
    spawn_sound_toggle(&mut commands, SOUND_MUTE_TOP, &audio_settings, language);

    spawn_sound_option(
        &mut commands,
        SoundSettingsAction::Back,
        ClickRect {
            x: MENU_LEFT,
            y: SOUND_BACK_TOP,
            w: MAIN_START_WIDTH * 0.64,
            h: MAIN_START_HEIGHT,
        },
        text(language, TextKey::Back),
        Color::srgba(0.72, 0.54, 0.44, 0.28),
    );
}

fn spawn_sound_toggle(
    commands: &mut Commands,
    top: f32,
    audio_settings: &AudioSettings,
    language: Language,
) {
    commands.spawn((
        Text::new(text(language, TextKey::Mute)),
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
                sound_settings_toggle_state(audio_settings, language),
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
    language_settings: Res<LanguageSettings>,
    mut render_state: ResMut<ModeSelectRenderState>,
) {
    spawn_mode_select_content(
        &mut commands,
        &windows,
        &match_setup,
        language_settings.language,
    );
    render_state.key = Some(mode_select_render_key(
        &windows,
        &match_setup,
        language_settings.language,
    ));
}

struct CompactConfigItem {
    rect: ClickRect,
    label: String,
    action: Option<ModeSelectAction>,
    color: Color,
}

fn compact_mode_items(
    width: f32,
    setup: &MatchSetup,
    language: Language,
) -> (Vec<CompactConfigItem>, f32) {
    let mut items = Vec::new();
    let mut y = 0.0;
    let default_color = Color::srgb(0.86, 0.91, 0.97);
    let mut group =
        |title: String, options: Vec<(ModeSelectAction, String, Color)>, columns: usize| {
            items.push(CompactConfigItem {
                rect: ClickRect {
                    x: 0.,
                    y,
                    w: width,
                    h: 24.,
                },
                label: title,
                action: None,
                color: Color::NONE,
            });
            let card_w = (width - (columns - 1) as f32 * 8.0) / columns as f32;
            for (i, (action, label, color)) in options.into_iter().enumerate() {
                items.push(CompactConfigItem {
                    rect: ClickRect {
                        x: (i % columns) as f32 * (card_w + 8.),
                        y: y + 28. + (i / columns) as f32 * 56.,
                        w: card_w,
                        h: 48.,
                    },
                    label,
                    action: Some(action),
                    color,
                });
            }
            y = items.last().map(|i| i.rect.y + i.rect.h + 16.).unwrap_or(y);
        };
    group(
        text(language, TextKey::Mode).into(),
        GameMode::ALL
            .iter()
            .map(|m| {
                (
                    ModeSelectAction::SetMode(*m),
                    mode_label(language, *m).into(),
                    default_color,
                )
            })
            .collect(),
        3,
    );
    group(
        text(language, TextKey::PlayStyle).into(),
        RuleSet::ALL
            .iter()
            .map(|r| {
                (
                    ModeSelectAction::SetRuleSet(*r),
                    rule_set_label(language, *r).into(),
                    default_color,
                )
            })
            .collect(),
        2,
    );
    for player_index in 0..setup.active_player_count() {
        let mut options: Vec<_> = PlayerSeat::ALL
            .iter()
            .map(|seat| {
                let label = match (language, seat) {
                    (Language::SimplifiedChinese, PlayerSeat::Blue) => "蓝",
                    (Language::SimplifiedChinese, PlayerSeat::Red) => "红",
                    (Language::SimplifiedChinese, PlayerSeat::Green) => "绿",
                    (Language::SimplifiedChinese, PlayerSeat::Yellow) => "黄",
                    (_, PlayerSeat::Blue) => "Blue",
                    (_, PlayerSeat::Red) => "Red",
                    (_, PlayerSeat::Green) => "Green",
                    (_, PlayerSeat::Yellow) => "Yellow",
                };
                (
                    ModeSelectAction::SetPlayerSeat {
                        player_index,
                        seat: *seat,
                    },
                    label.to_string(),
                    seat.to_color().mix(&Color::WHITE, 0.6),
                )
            })
            .collect();
        options.push((
            ModeSelectAction::SetPlayerControl {
                player_index,
                control: PlayerControl::Human,
            },
            text(language, TextKey::Human).into(),
            default_color,
        ));
        options.push((
            ModeSelectAction::SetPlayerControl {
                player_index,
                control: PlayerControl::Ai,
            },
            text(language, TextKey::Ai).into(),
            default_color,
        ));
        group(player_label(language, player_index + 1), options, 4);
    }
    group(
        text(language, TextKey::PiecesPerPlayer).into(),
        (1..=4)
            .map(|n| (ModeSelectAction::SetPieces(n), n.to_string(), default_color))
            .collect(),
        4,
    );
    group(
        text(language, TextKey::LaunchRule).into(),
        LaunchRule::ALL
            .iter()
            .map(|r| {
                (
                    ModeSelectAction::SetLaunchRule(*r),
                    launch_rule_label(language, *r).into(),
                    default_color,
                )
            })
            .collect(),
        2,
    );
    group(
        text(language, TextKey::AiDifficulty).into(),
        [AiDifficulty::Easy, AiDifficulty::Normal, AiDifficulty::Hard]
            .iter()
            .map(|a| {
                (
                    ModeSelectAction::SetAiDifficulty(*a),
                    ai_difficulty_label(language, *a).into(),
                    default_color,
                )
            })
            .collect(),
        3,
    );
    (items, y)
}

fn compact_mode_viewport(width: f32, height: f32) -> ClickRect {
    ClickRect {
        x: 16.,
        y: 96.,
        w: width - 32.,
        h: (height - 176.).max(100.),
    }
}

fn compact_mode_action_rect(width: f32, height: f32, index: usize) -> ClickRect {
    let w = (width - 40.) * 0.5;
    ClickRect {
        x: 16. + index as f32 * (w + 8.),
        y: height - 64.,
        w,
        h: 48.,
    }
}

fn spawn_compact_mode_select(
    commands: &mut Commands,
    setup: &MatchSetup,
    language: Language,
    width: f32,
    height: f32,
) {
    let rect = compact_mode_viewport(width, height);
    let (items, content_height) = compact_mode_items(rect.w, setup, language);
    if content_height > rect.h {
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(rect.x),
                top: Val::Px(height - 80.0),
                width: Val::Px(rect.w),
                height: Val::Px(16.0),
                ..default()
            },
            Text::new(match language {
                Language::SimplifiedChinese => "上下滑动 / 滚动查看更多设置",
                Language::English => "Swipe or scroll for more settings",
            }),
            TextFont {
                font_size: FontSize::Px(12.0),
                ..default()
            },
            TextColor(Color::srgb(0.25, 0.31, 0.40)),
            TextLayout::justify(Justify::Center),
            MenuEntity,
        ));
    }
    let viewport = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(rect.x),
                top: Val::Px(rect.y),
                width: Val::Px(rect.w),
                height: Val::Px(rect.h),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ScrollPosition::default(),
            CompactModeViewport,
            MenuEntity,
            Name::new("CompactMatchSettings"),
        ))
        .id();
    let content = commands
        .spawn(Node {
            width: Val::Percent(100.),
            height: Val::Px(content_height),
            flex_shrink: 0.,
            position_type: PositionType::Relative,
            ..default()
        })
        .id();
    commands.entity(viewport).add_child(content);
    for item in items {
        let entity = if let Some(action) = item.action {
            let entity = spawn_box_with_label(
                commands,
                item.rect,
                item.color,
                &item.label,
                16.,
                None,
                Some(action),
            );
            commands.entity(entity).insert(CompactModeItem);
            entity
        } else {
            commands
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(item.rect.x),
                        top: Val::Px(item.rect.y),
                        width: Val::Px(item.rect.w),
                        height: Val::Px(item.rect.h),
                        ..default()
                    },
                    Text::new(item.label),
                    TextFont {
                        font_size: FontSize::Px(18.),
                        ..default()
                    },
                    TextColor(Color::srgb(0.10, 0.16, 0.24)),
                ))
                .id()
        };
        commands.entity(content).add_child(entity);
    }
    for (i, action, key) in [
        (0, ModeSelectAction::StartMatch, TextKey::Start),
        (1, ModeSelectAction::Back, TextKey::Back),
    ] {
        spawn_option(
            commands,
            action,
            compact_mode_action_rect(width, height, i),
            text(language, key),
            Color::srgb(0.72, 0.84, 0.94),
        );
    }
}

fn update_compact_mode_scroll(
    windows: Query<&Window>,
    pointer: Res<PointerInputState>,
    overlay: Res<SoundSettingsOverlayState>,
    mut wheel: MessageReader<MouseWheel>,
    mut previous: Local<Option<Vec2>>,
    mut query: Query<(&mut ScrollPosition, &ComputedNode), With<CompactModeViewport>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(point) = pointer
        .current_position()
        .filter(|p| compact_mode_viewport(window.width(), window.height()).contains(*p))
    else {
        wheel.clear();
        *previous = None;
        return;
    };
    if overlay.open {
        wheel.clear();
        *previous = None;
        return;
    }
    let delta: f32 = wheel
        .read()
        .map(|e| match e.unit {
            MouseScrollUnit::Line => e.y * 24.,
            MouseScrollUnit::Pixel => e.y,
        })
        .sum();
    for (mut scroll, node) in &mut query {
        let max = ((node.content_size().y - node.size().y) * node.inverse_scale_factor()).max(0.);
        scroll.y = (scroll.y - delta).clamp(0., max);
        if pointer.is_pressed()
            && pointer.current_source() == Some(crate::platform::PointerSource::Touch)
        {
            if !pointer.just_pressed()
                && let Some(last) = *previous
            {
                scroll.y = (scroll.y + last.y - point.y).clamp(0., max);
            }
            *previous = Some(point);
        } else {
            *previous = None;
        }
    }
}

fn spawn_mode_select_content(
    commands: &mut Commands,
    windows: &Query<&Window>,
    match_setup: &MatchSetup,
    language: Language,
) {
    // 对局配置页：按“模式/玩家配置/规则/开始返回”分区渲染。
    let (window_width, window_height) = windows
        .single()
        .map(|window| (window.width(), window.height()))
        .unwrap_or((1280.0, 720.0));
    let active_player_count = match_setup.active_player_count();
    let layout = mode_select_layout(window_width, window_height, active_player_count);
    if window_width < 900.0 || window_height < 620.0 || layout.scale < 1.0 {
        spawn_compact_mode_select(commands, match_setup, language, window_width, window_height);
        return;
    }

    spawn_section_label(
        commands,
        layout,
        text(language, TextKey::Mode),
        MODE_ROW_TOP + 7.0,
    );
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
            mode_label(language, *mode),
            Color::srgba(0.53, 0.77, 0.96, 0.26),
        );
    }

    spawn_section_label(
        commands,
        layout,
        text(language, TextKey::PlayStyle),
        RULE_SET_ROW_TOP + 7.0,
    );
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
            rule_set_label(language, *rule_set),
            Color::srgba(0.58, 0.72, 0.58, 0.28),
        );
    }

    for player_index in 0..active_player_count {
        let row_top = player_row_top(player_index);
        spawn_section_label(
            commands,
            layout,
            &player_label(language, player_index + 1),
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
            text(language, TextKey::Human),
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
            text(language, TextKey::Ai),
            Color::srgba(0.53, 0.77, 0.96, 0.26),
        );
    }

    let pieces_top = pieces_row_top(active_player_count);
    spawn_section_label(
        commands,
        layout,
        text(language, TextKey::PiecesPerPlayer),
        pieces_top + 7.0,
    );
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
    spawn_section_label(
        commands,
        layout,
        text(language, TextKey::LaunchRule),
        launch_rule_top + 7.0,
    );
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
            launch_rule_label(language, *launch_rule),
            Color::srgba(0.53, 0.77, 0.96, 0.26),
        );
    }

    let ai_difficulty_top = ai_difficulty_row_top(active_player_count);
    spawn_section_label(
        commands,
        layout,
        text(language, TextKey::AiDifficulty),
        ai_difficulty_top + 7.0,
    );
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
            ai_difficulty_label(language, *difficulty),
            Color::srgba(0.61, 0.68, 0.88, 0.30),
        );
    }

    spawn_option(
        commands,
        ModeSelectAction::StartMatch,
        layout.rect(mode_select_start_rect(active_player_count)),
        text(language, TextKey::Start),
        Color::srgba(0.40, 0.72, 0.55, 0.40),
    );
    spawn_option(
        commands,
        ModeSelectAction::Back,
        layout.rect(mode_select_back_rect(active_player_count)),
        text(language, TextKey::Back),
        Color::srgba(0.72, 0.54, 0.44, 0.28),
    );
}

fn player_label(language: Language, player_index: usize) -> String {
    match language {
        Language::SimplifiedChinese => format!("玩家{player_index}"),
        Language::English => format!("P{player_index}"),
    }
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
    spawn_box_with_label(commands, rect, base_color, label, 24.0, None, Some(action));
}

fn spawn_sound_option(
    commands: &mut Commands,
    action: SoundSettingsAction,
    rect: ClickRect,
    label: &str,
    base_color: Color,
) {
    spawn_box_with_label(commands, rect, base_color, label, 26.0, None, None);
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
    localized_key: Option<TextKey>,
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
            if let Some(localized_key) = localized_key {
                label_entity.insert(LocalizedText { key: localized_key });
            }
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
    if !character.is_ascii() {
        return 1.0;
    }
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

fn sound_settings_toggle_state(
    audio_settings: &AudioSettings,
    language: Language,
) -> SettingsToggleVisualState {
    SettingsToggleVisualState {
        active: audio_settings.muted,
        label: format_mute_label(audio_settings, language),
    }
}

fn global_settings_toggle_state(
    value_kind: SoundSettingsValueKind,
    audio_settings: &AudioSettings,
    performance_settings: &PerformanceSettings,
    language: Language,
) -> Option<SettingsToggleVisualState> {
    match value_kind {
        SoundSettingsValueKind::Mute => Some(sound_settings_toggle_state(audio_settings, language)),
        SoundSettingsValueKind::Fps => Some(SettingsToggleVisualState {
            active: performance_settings.show_fps,
            label: fps_toggle_label(performance_settings, language),
        }),
        SoundSettingsValueKind::Music
        | SoundSettingsValueKind::Effects
        | SoundSettingsValueKind::Language => None,
    }
}

fn sound_settings_content(audio_settings: &AudioSettings, language: Language) -> String {
    match language {
        Language::SimplifiedChinese => format!(
            "{}   |   音乐 {}   |   音效 {}",
            format_mute_label(audio_settings, language),
            format_volume_percent(audio_settings.music_volume),
            format_volume_percent(audio_settings.effects_volume)
        ),
        Language::English => format!(
            "{}   |   Music {}   |   Effects {}",
            format_mute_label(audio_settings, language),
            format_volume_percent(audio_settings.music_volume),
            format_volume_percent(audio_settings.effects_volume)
        ),
    }
}

fn format_mute_label(audio_settings: &AudioSettings, language: Language) -> &'static str {
    format_mute_label_for_language(audio_settings.muted, language)
}

fn format_mute_label_for_language(muted: bool, language: Language) -> &'static str {
    match (language, muted) {
        (Language::SimplifiedChinese, true) => "已静音",
        (Language::SimplifiedChinese, false) => "声音开",
        (Language::English, true) => "Muted",
        (Language::English, false) => "Sound On",
    }
}

fn format_volume_percent(value: f32) -> String {
    format!("{:>3}%", (value.clamp(0.0, 1.0) * 100.0).round() as u8)
}

fn ai_difficulty_label(language: Language, difficulty: AiDifficulty) -> &'static str {
    i18n_ai_label(language, difficulty)
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
    language_settings: Res<LanguageSettings>,
    mut summary_query: Query<&mut Text, (With<SoundSettingsText>, Without<SoundSettingsValueText>)>,
    mut value_query: SoundSettingsValueQuery,
    mut toggle_track_query: Query<(
        &SoundSettingsToggleTrack,
        &mut BackgroundColor,
        &mut BorderColor,
    )>,
    mut toggle_thumb_query: Query<(&SoundSettingsToggleThumb, &mut Node)>,
) {
    if !audio_settings.is_changed() && !language_settings.is_changed() {
        return;
    }

    for mut text in &mut summary_query {
        *text = Text::new(sound_settings_content(
            &audio_settings,
            language_settings.language,
        ));
    }

    for (value_text, mut text) in &mut value_query {
        *text = Text::new(match value_text.kind {
            SoundSettingsValueKind::Music => format_volume_percent(audio_settings.music_volume),
            SoundSettingsValueKind::Effects => format_volume_percent(audio_settings.effects_volume),
            SoundSettingsValueKind::Mute => {
                format_mute_label(&audio_settings, language_settings.language).to_owned()
            }
            SoundSettingsValueKind::Fps => match language_settings.language {
                Language::SimplifiedChinese => "帧率 --".to_owned(),
                Language::English => "FPS --".to_owned(),
            },
            SoundSettingsValueKind::Language => String::new(),
        });
    }
    let state = sound_settings_toggle_state(&audio_settings, language_settings.language);
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
        SoundSettingsAction::CycleLanguage => {}
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
    windows: Query<&Window>,
    pointer: Res<PointerInputState>,
    mut match_setup: ResMut<MatchSetup>,
    mut next_state: ResMut<NextState<AppState>>,
    overlay_state: Res<SoundSettingsOverlayState>,
    query: Query<(&ClickRect, &ModeSelectOption, Option<&CompactModeItem>)>,
    viewport: Query<&ScrollPosition, With<CompactModeViewport>>,
    mut touch_start: Local<Option<Vec2>>,
) {
    // 鼠标主操作：点击命中对应配置项并立即生效。
    if sound_settings_overlay_blocks_input(&overlay_state) {
        return;
    }
    let position = if pointer.current_source() == Some(crate::platform::PointerSource::Touch) {
        if pointer.just_pressed() {
            *touch_start = pointer.just_pressed_position();
        }
        pointer
            .just_released_position()
            .filter(|p| touch_start.is_some_and(|start| start.distance(*p) < 12.0))
    } else {
        pointer.just_pressed_position()
    };
    let Some(cursor) = position else {
        return;
    };

    for (rect, option, compact) in &query {
        let point = if compact.is_some() {
            let Ok(window) = windows.single() else {
                continue;
            };
            let area = compact_mode_viewport(window.width(), window.height());
            if !area.contains(cursor) {
                continue;
            }
            cursor - Vec2::new(area.x, area.y)
                + Vec2::Y * viewport.single().map(|s| s.y).unwrap_or(0.)
        } else {
            cursor
        };
        if !rect.contains(point) {
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
    language_settings: Res<LanguageSettings>,
    mut render_state: ResMut<ModeSelectRenderState>,
    query: Query<Entity, (With<MenuEntity>, Without<ChildOf>)>,
) {
    let next_key = mode_select_render_key(&windows, &match_setup, language_settings.language);
    if render_state.key == Some(next_key) {
        return;
    }

    for entity in &query {
        commands.entity(entity).despawn();
    }
    spawn_mode_select_content(
        &mut commands,
        &windows,
        &match_setup,
        language_settings.language,
    );
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

    #[test]
    fn compact_configuration_preserves_touch_targets_and_fixed_footer() {
        for (w, h) in [(360., 640.), (390., 844.), (640., 360.), (1024., 600.)] {
            let viewport = compact_mode_viewport(w, h);
            for mode in GameMode::ALL {
                let mut config = setup();
                config.mode = mode;
                for language in [Language::SimplifiedChinese, Language::English] {
                    let (items, content_height) = compact_mode_items(viewport.w, &config, language);
                    for (i, item) in items.iter().enumerate() {
                        let a = item.rect;
                        assert!(
                            a.x >= 0.
                                && a.x + a.w <= viewport.w + 0.01
                                && a.y + a.h <= content_height
                        );
                        if item.action.is_some() {
                            assert!(a.w >= 48. && a.h >= 48.);
                        }
                        for other in &items[i + 1..] {
                            let b = other.rect;
                            assert!(
                                !(a.x < b.x + b.w
                                    && a.x + a.w > b.x
                                    && a.y < b.y + b.h
                                    && a.y + a.h > b.y)
                            );
                        }
                    }
                    for index in 0..2 {
                        let button = compact_mode_action_rect(w, h, index);
                        assert!(button.y >= viewport.y + viewport.h + 8.);
                        assert!(button.x >= 16. && button.x + button.w <= w - 16.);
                        assert!(button.w >= 48. && button.h >= 48.);
                        assert!(button.y + button.h <= h - 16.);
                    }
                }
            }
        }
    }

    #[test]
    fn settings_fit_narrow_windows_and_scrolled_actions_map_to_content() {
        for (w, h) in [(360, 640), (390, 844), (640, 360), (1280, 720)] {
            let window = Window {
                resolution: (w, h).into(),
                ..default()
            };
            let panel = global_sound_panel_rect(&window);
            assert!(panel.x >= 16. && panel.x + panel.w <= w as f32 - 16.);
            assert!(panel.y >= 16. && panel.y + panel.h <= h as f32 - 16.);
            let close = global_settings_close_rect();
            assert!(close.w >= 48. && close.h >= 48. && close.x + close.w <= panel.w - 16.);
            assert_eq!(
                global_sound_action_at(
                    Vec2::new(
                        panel.x + close.x + close.w / 2.,
                        panel.y + close.y + close.h / 2.
                    ),
                    &window
                ),
                Some(SoundSettingsAction::Back)
            );
            let scroll = (GLOBAL_SOUND_PANEL_H - panel.h).max(0.);
            for (action, local) in [
                (
                    SoundSettingsAction::MainMenu,
                    global_settings_main_menu_rect(),
                ),
                (
                    SoundSettingsAction::QuitGame,
                    global_settings_quit_game_rect(),
                ),
            ] {
                assert!(local.w >= 48. && local.h >= 48.);
                let visible_center = Vec2::new(
                    panel.x + local.x + local.w / 2.,
                    panel.y + local.y + local.h / 2. - scroll,
                );
                assert!(panel.contains(visible_center));
                assert_eq!(
                    global_sound_action_at(visible_center + Vec2::Y * scroll, &window),
                    Some(action)
                );
            }
        }
    }

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
                for language in [Language::SimplifiedChinese, Language::English] {
                    let label = rule_set_label(language, *rule_set);
                    let font_size = fitted_box_label_font_size(24.0, rect.w, rect.h, label);
                    let estimated_label_width = label_width_units(label) * font_size
                        + fitted_box_padding(rect.h) * 2.0
                        + OPTION_LABEL_SAFETY_PX;

                    assert!(
                        estimated_label_width <= rect.w + f32::EPSILON,
                        "{label} label overflows {rect:?} at {width}x{height}",
                    );
                }
            }
        }
    }

    #[test]
    fn mode_select_render_key_tracks_window_size_changes() {
        let match_setup = setup();
        let first = mode_select_render_key_from_size(
            1280.0,
            720.0,
            &match_setup,
            Language::SimplifiedChinese,
        );
        let tablet = mode_select_render_key_from_size(
            2800.0,
            1840.0,
            &match_setup,
            Language::SimplifiedChinese,
        );

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
        assert!(entry.x + entry.w <= window.width() - 16.0);
        assert_eq!(entry.y, 16.0);
        assert!(entry.h >= 48.0);
    }

    fn settings_test_app(width: u32, height: u32) -> (App, Entity) {
        let mut app = App::new();
        app.init_resource::<Time>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .add_message::<bevy::input::touch::TouchInput>()
            .add_message::<AppExit>()
            .add_plugins(crate::platform::PlatformPlugin)
            .insert_resource(State::new(AppState::MainMenu))
            .init_resource::<NextState<AppState>>()
            .init_resource::<AudioSettings>()
            .init_resource::<PerformanceSettings>()
            .init_resource::<LanguageSettings>()
            .init_resource::<SoundSettingsOverlayState>()
            .init_resource::<SettingsEntryState>()
            .add_systems(Startup, spawn_global_sound_overlay)
            .add_systems(
                PreUpdate,
                update_sound_overlay_input_capture.after(crate::platform::PlatformInputSet),
            )
            .add_systems(
                Update,
                (
                    update_settings_entry,
                    update_global_sound_overlay,
                    handle_global_sound_overlay_input,
                    handle_global_sound_overlay_click,
                )
                    .chain(),
            );
        let window = app
            .world_mut()
            .spawn(Window {
                resolution: (width, height).into(),
                ..default()
            })
            .id();
        app.update();
        (app, window)
    }

    fn settings_frame(app: &mut App, seconds: f32) {
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f32(seconds));
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .clear();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
    }

    fn settings_touch(
        app: &mut App,
        window: Entity,
        phase: bevy::input::touch::TouchPhase,
        position: Vec2,
    ) {
        app.world_mut()
            .write_message(bevy::input::touch::TouchInput {
                window,
                phase,
                position,
                id: 1,
                force: None,
            });
        settings_frame(app, 0.0);
    }

    #[test]
    fn rendered_settings_button_keeps_window_anchor_shape_and_icon_on_all_pages() {
        for (w, h) in [(360, 640), (640, 360), (1280, 720), (2560, 1600)] {
            let (mut app, _) = settings_test_app(w, h);
            for page in [
                AppState::MainMenu,
                AppState::ModeSelect,
                AppState::InGame,
                AppState::Result,
            ] {
                app.insert_resource(State::new(page));
                app.update();
                let mut query = app.world_mut().query_filtered::<(&Node, &Visibility, &BackgroundColor), With<GlobalSoundEntry>>();
                let (node, visibility, fill) = query.single(app.world()).unwrap();
                assert_eq!(node.left, Val::Px(w as f32 - 64.0));
                assert_eq!(node.top, Val::Px(16.0));
                assert_eq!(node.width, Val::Px(48.0));
                assert_eq!(node.height, Val::Px(48.0));
                assert_eq!(node.border_radius, BorderRadius::all(Val::Px(12.0)));
                assert_eq!(node.border, UiRect::ZERO);
                assert_eq!(*visibility, Visibility::Visible);
                assert_eq!(fill.0, SETTINGS_ENTRY_FILL);
            }
        }
    }

    #[test]
    fn gear_visible_geometry_fits_24_pixels_without_font_or_bitmap_dependencies() {
        let (mut app, _) = settings_test_app(1280, 720);
        let mut query = app.world_mut().query::<(&Name, &Node, &Children)>();
        let (_, icon, children) = query
            .iter(app.world())
            .find(|(name, _, _)| name.as_str() == "GlobalSettingsGear")
            .unwrap();
        assert_eq!(icon.width, Val::Px(24.0));
        assert_eq!(icon.height, Val::Px(24.0));
        assert_eq!(children.len(), 9);
        let pixels = |val| match val {
            Val::Px(value) => value,
            _ => panic!("icon must use logical pixels"),
        };
        for entity in children.iter() {
            let node = app.world().get::<Node>(entity).unwrap();
            let size = Vec2::new(pixels(node.width), pixels(node.height));
            let center = Vec2::new(pixels(node.left), pixels(node.top)) + size * 0.5;
            let rotation = app.world().get::<UiTransform>(entity).unwrap().rotation;
            for x in [-1.0, 1.0] {
                for y in [-1.0, 1.0] {
                    let corner = center + rotation * (Vec2::new(x, y) * size * 0.5);
                    assert!(
                        corner.cmpge(Vec2::ZERO).all() && corner.cmple(Vec2::splat(24.0)).all()
                    );
                }
            }
        }
    }

    #[test]
    fn settings_mouse_hover_hint_and_click_work_without_click_through() {
        let (mut app, window) = settings_test_app(1280, 720);
        let position = global_settings_rect(1280.0).center();
        app.world_mut()
            .get_mut::<Window>(window)
            .unwrap()
            .set_cursor_position(Some(position));
        settings_frame(&mut app, 0.0);
        assert!(!app.world().resource::<SettingsEntryState>().hint_visible);
        settings_frame(&mut app, 0.6);
        assert!(app.world().resource::<SettingsEntryState>().hint_visible);
        assert!(!app.world().resource::<SoundSettingsOverlayState>().open);
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        settings_frame(&mut app, 0.0);
        assert!(app.world().resource::<SoundSettingsOverlayState>().open);
        assert!(
            app.world()
                .resource::<SoundSettingsOverlayState>()
                .input_captured
        );
        settings_frame(&mut app, 0.0);
        assert!(!app.world().resource::<SettingsEntryState>().hint_visible);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        settings_frame(&mut app, 0.0);
        assert!(!app.world().resource::<SoundSettingsOverlayState>().open);
        assert!(
            app.world()
                .resource::<SoundSettingsOverlayState>()
                .input_captured
        );
    }

    #[test]
    fn settings_long_press_explains_icon_without_opening_on_release_then_short_tap_opens() {
        use bevy::input::touch::TouchPhase;
        let (mut app, window) = settings_test_app(640, 360);
        let position = global_settings_rect(640.0).center();
        settings_touch(&mut app, window, TouchPhase::Started, position);
        assert!(
            app.world()
                .resource::<SoundSettingsOverlayState>()
                .input_captured
        );
        settings_frame(&mut app, 0.6);
        assert!(app.world().resource::<SettingsEntryState>().hint_visible);
        assert!(!app.world().resource::<SoundSettingsOverlayState>().open);
        settings_touch(&mut app, window, TouchPhase::Ended, position);
        assert!(!app.world().resource::<SoundSettingsOverlayState>().open);
        assert!(
            app.world()
                .resource::<SoundSettingsOverlayState>()
                .input_captured
        );
        settings_frame(&mut app, 1.3);
        assert!(!app.world().resource::<SettingsEntryState>().hint_visible);
        settings_touch(&mut app, window, TouchPhase::Started, position);
        settings_touch(&mut app, window, TouchPhase::Ended, position);
        assert!(app.world().resource::<SoundSettingsOverlayState>().open);
    }

    #[test]
    fn settings_drag_out_and_back_does_not_activate_or_show_hint() {
        use bevy::input::touch::TouchPhase;
        let (mut app, window) = settings_test_app(360, 640);
        let position = global_settings_rect(360.0).center();
        settings_touch(&mut app, window, TouchPhase::Started, position);
        settings_touch(
            &mut app,
            window,
            TouchPhase::Moved,
            position - Vec2::X * 40.0,
        );
        settings_touch(&mut app, window, TouchPhase::Moved, position);
        settings_frame(&mut app, 0.7);
        settings_touch(&mut app, window, TouchPhase::Ended, position);
        assert!(!app.world().resource::<SoundSettingsOverlayState>().open);
        assert!(!app.world().resource::<SettingsEntryState>().hint_visible);
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
            .init_resource::<LanguageSettings>()
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
        let mut language_settings = LanguageSettings::default();
        let mut overlay_state = SoundSettingsOverlayState {
            open: true,
            input_captured: false,
        };

        assert_eq!(
            apply_global_sound_action(
                SoundSettingsAction::MusicDown,
                &mut audio_settings,
                &mut performance_settings,
                &mut language_settings,
                &mut overlay_state,
            ),
            GlobalSettingsCommand::None
        );
        assert_eq!(
            apply_global_sound_action(
                SoundSettingsAction::EffectsUp,
                &mut audio_settings,
                &mut performance_settings,
                &mut language_settings,
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
                &mut language_settings,
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
                &mut language_settings,
                &mut overlay_state,
            ),
            GlobalSettingsCommand::None
        );
        assert!(performance_settings.show_fps);
        assert!(overlay_state.open);

        assert_eq!(
            apply_global_sound_action(
                SoundSettingsAction::CycleLanguage,
                &mut audio_settings,
                &mut performance_settings,
                &mut language_settings,
                &mut overlay_state,
            ),
            GlobalSettingsCommand::None
        );
        assert_eq!(language_settings.label(), "English");
        assert!(overlay_state.open);

        assert_eq!(
            apply_global_sound_action(
                SoundSettingsAction::MainMenu,
                &mut audio_settings,
                &mut performance_settings,
                &mut language_settings,
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
                &mut language_settings,
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
                &mut language_settings,
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

        assert_eq!(
            format_mute_label(&audio_settings, Language::SimplifiedChinese),
            "声音开"
        );
        assert_eq!(
            format_mute_label(&audio_settings, Language::English),
            "Sound On"
        );
        assert!(
            sound_settings_content(&audio_settings, Language::SimplifiedChinese)
                .starts_with("声音开")
        );

        audio_settings.toggle_mute();
        assert_eq!(
            format_mute_label(&audio_settings, Language::SimplifiedChinese),
            "已静音"
        );
        assert_eq!(
            format_mute_label(&audio_settings, Language::English),
            "Muted"
        );
        assert!(
            sound_settings_content(&audio_settings, Language::SimplifiedChinese)
                .starts_with("已静音")
        );
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
                Language::SimplifiedChinese,
            ),
            Some(SettingsToggleVisualState {
                active: false,
                label: "声音开",
            })
        );
        assert_eq!(
            global_settings_toggle_state(
                SoundSettingsValueKind::Fps,
                &audio_settings,
                &performance_settings,
                Language::SimplifiedChinese,
            ),
            Some(SettingsToggleVisualState {
                active: false,
                label: "帧率关",
            })
        );

        audio_settings.toggle_mute();
        performance_settings.toggle_fps();
        assert_eq!(
            global_settings_toggle_state(
                SoundSettingsValueKind::Mute,
                &audio_settings,
                &performance_settings,
                Language::SimplifiedChinese,
            )
            .map(|state| state.active),
            Some(true)
        );
        assert_eq!(
            global_settings_toggle_state(
                SoundSettingsValueKind::Fps,
                &audio_settings,
                &performance_settings,
                Language::SimplifiedChinese,
            )
            .map(|state| state.label),
            Some("帧率开")
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

        let language = Vec2::new(
            panel.x + GLOBAL_SOUND_CONTROL_LEFT + 8.0,
            panel.y + GLOBAL_SOUND_LANGUAGE_ROW_TOP + 8.0,
        );
        assert_eq!(
            global_sound_action_at(language, &window),
            Some(SoundSettingsAction::CycleLanguage)
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
