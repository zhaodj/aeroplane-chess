use bevy::prelude::*;

use crate::data::game_mode::GameMode;
use crate::domain::player::PlayerControl;
use crate::domain::rules::LaunchRule;
use crate::gameplay::match_flow::{MatchSetup, PlayerColorChoice};
use crate::states::AppState;

/// 菜单插件：主菜单与开局配置页的渲染和交互。
pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::MainMenu), spawn_main_menu)
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

#[derive(Component)]
/// 菜单实体分组标记。
struct MenuEntity;

#[derive(Component)]
/// 主菜单开始按钮点击区域标记。
struct MainMenuStartArea;

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

const MENU_LEFT: f32 = 96.0;
const MAIN_START_TOP: f32 = 250.0;
const MAIN_START_WIDTH: f32 = 360.0;
const MAIN_START_HEIGHT: f32 = 62.0;

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

fn spawn_main_menu(mut commands: Commands) {
    // 主菜单：标题 + 开始按钮（支持点击与回车）。
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
            left: Val::Px(MENU_LEFT),
            ..default()
        },
        Name::new("MainMenuTitle"),
        MenuEntity,
    ));

    spawn_box_with_label(
        &mut commands,
        ClickRect {
            x: MENU_LEFT,
            y: MAIN_START_TOP,
            w: MAIN_START_WIDTH,
            h: MAIN_START_HEIGHT,
        },
        Color::srgba(0.42, 0.61, 0.88, 0.30),
        "Start Match (Click / Enter)",
        30.0,
        None,
    );

    commands.spawn((
        MainMenuStartArea,
        ClickRect {
            x: MENU_LEFT,
            y: MAIN_START_TOP,
            w: MAIN_START_WIDTH,
            h: MAIN_START_HEIGHT,
        },
        Name::new("MainMenuStartArea"),
        MenuEntity,
    ));
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
) {
    // 键盘兜底：回车进入配置页。
    if keyboard.just_pressed(KeyCode::Enter) {
        next_state.set(AppState::ModeSelect);
    }
}

fn handle_main_menu_click(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    mut next_state: ResMut<NextState<AppState>>,
    query: Query<&ClickRect, With<MainMenuStartArea>>,
) {
    // 鼠标主操作：点击开始区域进入配置页。
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };

    for rect in &query {
        if rect.contains(cursor) {
            next_state.set(AppState::ModeSelect);
            return;
        }
    }
}

fn handle_mode_select_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut match_setup: ResMut<MatchSetup>,
    mut next_state: ResMut<NextState<AppState>>,
) {
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
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    mut match_setup: ResMut<MatchSetup>,
    mut next_state: ResMut<NextState<AppState>>,
    query: Query<(&ClickRect, &ModeSelectOption)>,
) {
    // 鼠标主操作：点击命中对应配置项并立即生效。
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
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

fn cleanup_menu(mut commands: Commands, query: Query<Entity, With<MenuEntity>>) {
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
