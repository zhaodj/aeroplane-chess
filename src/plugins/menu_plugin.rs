use bevy::prelude::*;

use crate::data::game_mode::GameMode;
use crate::gameplay::match_flow::MatchSetup;
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
                    update_mode_select_text.run_if(in_state(AppState::ModeSelect)),
                    handle_mode_select_input.run_if(in_state(AppState::ModeSelect)),
                ),
            )
            .add_systems(OnExit(AppState::MainMenu), cleanup_menu)
            .add_systems(OnExit(AppState::ModeSelect), cleanup_menu);
    }
}

#[derive(Component)]
struct MenuEntity;

#[derive(Component)]
struct ModeSelectText;

fn spawn_main_menu(mut commands: Commands) {
    commands.spawn((
        Text::new("Aeroplane Chess\n\nPress Enter to Start"),
        TextFont {
            font_size: 42.0,
            ..default()
        },
        TextColor(Color::srgb(0.10, 0.16, 0.24)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Percent(35.0),
            left: Val::Percent(24.0),
            ..default()
        },
        Name::new("MainMenuText"),
        MenuEntity,
    ));
}

fn spawn_mode_select(mut commands: Commands, match_setup: Res<MatchSetup>) {
    commands.spawn((
        Text::new(mode_select_content(&match_setup)),
        TextFont {
            font_size: 36.0,
            ..default()
        },
        TextColor(Color::srgb(0.10, 0.16, 0.24)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Percent(28.0),
            left: Val::Percent(24.0),
            ..default()
        },
        Name::new("ModeSelectText"),
        ModeSelectText,
        MenuEntity,
    ));
}

fn mode_select_content(match_setup: &MatchSetup) -> String {
    let current_mode = match match_setup.mode {
        GameMode::OneVsOne => "1v1",
        GameMode::TwoVsTwo => "2v2",
    };

    format!(
        "对局设置\n\n\
1/2: 模式 ({})\n\
C: 人类颜色 ({})\n\
[-]/[=]: 棋子数量 ({} / 1~4)\n\n\
Enter: 开始对局\n\
Esc: 返回",
        current_mode,
        match_setup.human_color.label(),
        match_setup.pieces_per_player
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

fn handle_main_menu_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if keyboard.just_pressed(KeyCode::Enter) {
        next_state.set(AppState::ModeSelect);
    }
}

fn handle_mode_select_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut match_setup: ResMut<MatchSetup>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if keyboard.just_pressed(KeyCode::Escape) {
        next_state.set(AppState::MainMenu);
        return;
    }

    if keyboard.just_pressed(KeyCode::Digit1) {
        match_setup.mode = GameMode::OneVsOne;
    } else if keyboard.just_pressed(KeyCode::Digit2) {
        match_setup.mode = GameMode::TwoVsTwo;
    }

    if keyboard.just_pressed(KeyCode::KeyC) {
        match_setup.human_color = match_setup.human_color.next();
    }

    if keyboard.just_pressed(KeyCode::BracketLeft) || keyboard.just_pressed(KeyCode::Minus) {
        match_setup.pieces_per_player = match_setup.pieces_per_player.saturating_sub(1).clamp(1, 4);
    }

    if keyboard.just_pressed(KeyCode::BracketRight) || keyboard.just_pressed(KeyCode::Equal) {
        match_setup.pieces_per_player = match_setup.pieces_per_player.saturating_add(1).clamp(1, 4);
    }

    if keyboard.just_pressed(KeyCode::Enter) {
        next_state.set(AppState::LoadingGame);
    }
}

fn cleanup_menu(mut commands: Commands, query: Query<Entity, With<MenuEntity>>) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
}
