use bevy::prelude::*;

use crate::data::game_mode::GameMode;
use crate::domain::player::PlayerControl;
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
    let controls = match_setup.normalized_player_controls();
    let control_label = |control: PlayerControl| match control {
        PlayerControl::Human => "Human",
        PlayerControl::Ai => "AI",
    };
    let current_mode = match match_setup.mode {
        GameMode::OneVsOne => "1v1",
        GameMode::TwoVsTwo => "2v2",
    };
    let players_line = if match_setup.mode == GameMode::OneVsOne {
        format!(
            "Q/W: P1/P2 人机 ({}/{})",
            control_label(controls[0]),
            control_label(controls[1]),
        )
    } else {
        format!(
            "Q/W/E/R: P1..P4 人机 ({}/{}/{}/{})",
            control_label(controls[0]),
            control_label(controls[1]),
            control_label(controls[2]),
            control_label(controls[3]),
        )
    };

    format!(
        "对局设置\n\n\
1/2: 模式 ({})\n\
C: 人类颜色 ({})\n\
[-]/[=]: 棋子数量 ({} / 1~4)\n\n\
{}\n\
约束: 至少 1 名 Human\n\n\
Enter: 开始对局\n\
Esc: 返回",
        current_mode,
        match_setup.human_color.label(),
        match_setup.pieces_per_player,
        players_line
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
        match_setup.sanitize_player_controls();
    } else if keyboard.just_pressed(KeyCode::Digit2) {
        match_setup.mode = GameMode::TwoVsTwo;
        match_setup.sanitize_player_controls();
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

    if keyboard.just_pressed(KeyCode::KeyQ) {
        match_setup.toggle_player_control(0);
    }
    if keyboard.just_pressed(KeyCode::KeyW) {
        match_setup.toggle_player_control(1);
    }
    if keyboard.just_pressed(KeyCode::KeyE) {
        match_setup.toggle_player_control(2);
    }
    if keyboard.just_pressed(KeyCode::KeyR) {
        match_setup.toggle_player_control(3);
    }

    if keyboard.just_pressed(KeyCode::Enter) {
        match_setup.sanitize_player_controls();
        next_state.set(AppState::LoadingGame);
    }
}

fn cleanup_menu(mut commands: Commands, query: Query<Entity, With<MenuEntity>>) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
}
