use bevy::prelude::*;

use crate::data::game_mode::GameMode;
use crate::domain::player::PlayerControl;
use crate::gameplay::match_flow::{MatchSetup, PlayerColorChoice};
use crate::states::AppState;

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
struct MenuEntity;

#[derive(Component)]
struct MainMenuStartArea;

#[derive(Component)]
struct ModeSelectText;

#[derive(Clone, Copy, Component)]
struct ClickRect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl ClickRect {
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
    SetColor(PlayerColorChoice),
    SetPieces(u8),
    SetPlayerControl {
        player_index: usize,
        control: PlayerControl,
    },
    StartMatch,
    Back,
}

#[derive(Component)]
struct ModeSelectOption {
    action: ModeSelectAction,
    base_color: Color,
}

const MENU_LEFT: f32 = 96.0;
const MAIN_START_TOP: f32 = 250.0;
const MAIN_START_WIDTH: f32 = 360.0;
const MAIN_START_HEIGHT: f32 = 62.0;

const SECTION_LABEL_X: f32 = 96.0;
const OPTION_LEFT: f32 = 350.0;
const OPTION_W: f32 = 132.0;
const OPTION_H: f32 = 40.0;
const OPTION_GAP: f32 = 12.0;
const MODE_ROW_TOP: f32 = 176.0;
const COLOR_ROW_TOP: f32 = 238.0;
const PIECES_ROW_TOP: f32 = 300.0;
const CONTROL_ROW_START_TOP: f32 = 362.0;
const CONTROL_ROW_GAP: f32 = 52.0;
const BOTTOM_ROW_TOP: f32 = 588.0;
const COLOR_SWATCH_W: f32 = 54.0;
const COLOR_SWATCH_H: f32 = 34.0;

fn spawn_main_menu(mut commands: Commands) {
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
    commands.spawn((
        Text::new(mode_select_content(&match_setup)),
        TextFont {
            font_size: 24.0,
            ..default()
        },
        TextColor(Color::srgb(0.10, 0.16, 0.24)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(22.0),
            left: Val::Px(MENU_LEFT),
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

    spawn_section_label(&mut commands, "Human Color", COLOR_ROW_TOP + 7.0);
    for (index, choice) in PlayerColorChoice::ALL.iter().enumerate() {
        let x = OPTION_LEFT + index as f32 * (COLOR_SWATCH_W + OPTION_GAP);
        spawn_option(
            &mut commands,
            ModeSelectAction::SetColor(*choice),
            ClickRect {
                x,
                y: COLOR_ROW_TOP,
                w: COLOR_SWATCH_W,
                h: COLOR_SWATCH_H,
            },
            "",
            choice.to_color(),
        );
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
            w: OPTION_W * 1.5,
            h: OPTION_H + 6.0,
        },
        "Start Match",
        Color::srgba(0.40, 0.72, 0.55, 0.40),
    );
    spawn_option(
        &mut commands,
        ModeSelectAction::Back,
        ClickRect {
            x: OPTION_LEFT + OPTION_W * 1.5 + OPTION_GAP,
            y: BOTTOM_ROW_TOP,
            w: OPTION_W * 1.2,
            h: OPTION_H + 6.0,
        },
        "Back",
        Color::srgba(0.72, 0.54, 0.44, 0.28),
    );
}

fn spawn_section_label(commands: &mut Commands, label: &str, top: f32) {
    commands.spawn((
        Text::new(label),
        TextFont {
            font_size: 22.0,
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
    let mut entity = commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(rect.x),
            top: Val::Px(rect.y),
            width: Val::Px(rect.w),
            height: Val::Px(rect.h),
            ..default()
        },
        BackgroundColor(color),
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
        commands.spawn((
            Text::new(label),
            TextFont {
                font_size,
                ..default()
            },
            TextColor(Color::srgb(0.10, 0.16, 0.24)),
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(rect.y + (rect.h - font_size) * 0.5),
                left: Val::Px(rect.x + 14.0),
                ..default()
            },
            Name::new("MenuOptionLabel"),
            MenuEntity,
        ));
    }
}

fn mode_select_content(match_setup: &MatchSetup) -> String {
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

    format!(
        "Match Setup\n\
Direct-select options (no cycle toggle)\n\
Current: Mode {mode}, Color {}, Pieces {}\n\
Players: P1:{}  P2:{}  P3:{}  P4:{}\n\
Constraint: At least 1 Human",
        match_setup.human_color.label(),
        match_setup.pieces_per_player,
        c(controls[0]),
        c(controls[1]),
        c(controls[2]),
        c(controls[3]),
    )
}

fn update_mode_select_text(
    match_setup: Res<MatchSetup>,
    mut query: Query<&mut Text, With<ModeSelectText>>,
) {
    for mut text in &mut query {
        *text = Text::new(mode_select_content(&match_setup));
    }
}

fn update_mode_select_option_visuals(
    match_setup: Res<MatchSetup>,
    mut option_query: Query<(&ModeSelectOption, &mut BackgroundColor)>,
) {
    for (option, mut color) in &mut option_query {
        *color = BackgroundColor(option_fill_color(option, &match_setup));
    }
}

fn option_fill_color(option: &ModeSelectOption, match_setup: &MatchSetup) -> Color {
    if action_disabled(option.action, match_setup) {
        return option.base_color.with_alpha(0.15);
    }
    if action_selected(option.action, match_setup) {
        return option.base_color.mix(&Color::WHITE, 0.20).with_alpha(0.95);
    }
    option.base_color.with_alpha(0.58)
}

fn action_disabled(action: ModeSelectAction, match_setup: &MatchSetup) -> bool {
    match action {
        ModeSelectAction::SetPlayerControl { player_index, .. } => {
            player_index >= match_setup.active_player_count()
        }
        _ => false,
    }
}

fn action_selected(action: ModeSelectAction, match_setup: &MatchSetup) -> bool {
    match action {
        ModeSelectAction::SetMode(mode) => match_setup.mode == mode,
        ModeSelectAction::SetColor(choice) => match_setup.human_color == choice,
        ModeSelectAction::SetPieces(pieces) => match_setup.pieces_per_player == pieces,
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
    if action_disabled(action, match_setup) {
        return;
    }

    match action {
        ModeSelectAction::SetMode(mode) => {
            match_setup.mode = mode;
            match_setup.sanitize_player_controls();
        }
        ModeSelectAction::SetColor(choice) => match_setup.human_color = choice,
        ModeSelectAction::SetPieces(pieces) => match_setup.pieces_per_player = pieces.clamp(1, 4),
        ModeSelectAction::SetPlayerControl {
            player_index,
            control,
        } => match_setup.set_player_control(player_index, control),
        ModeSelectAction::StartMatch => {
            match_setup.sanitize_player_controls();
            next_state.set(AppState::LoadingGame);
        }
        ModeSelectAction::Back => next_state.set(AppState::MainMenu),
    }
}

fn cleanup_menu(mut commands: Commands, query: Query<Entity, With<MenuEntity>>) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
}
