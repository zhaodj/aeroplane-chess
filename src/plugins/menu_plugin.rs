use bevy::prelude::*;

use crate::data::game_mode::GameMode;
use crate::plugins::game_plugin::MatchSetup;
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
                    handle_mode_select_input.run_if(in_state(AppState::ModeSelect)),
                ),
            )
            .add_systems(OnExit(AppState::MainMenu), cleanup_menu)
            .add_systems(OnExit(AppState::ModeSelect), cleanup_menu);
    }
}

#[derive(Component)]
struct MenuEntity;

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
    let current = match match_setup.mode {
        GameMode::OneVsOne => "Current: 1v1",
        GameMode::TwoVsTwo => "Current: 2v2",
    };

    commands.spawn((
        Text::new(format!(
            "Select Mode\n\n1. 1v1\n2. 2v2\n\n{}\n\nEsc: Back",
            current
        )),
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
        MenuEntity,
    ));
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
        next_state.set(AppState::LoadingGame);
    } else if keyboard.just_pressed(KeyCode::Digit2) {
        match_setup.mode = GameMode::TwoVsTwo;
        next_state.set(AppState::LoadingGame);
    }
}

fn cleanup_menu(mut commands: Commands, query: Query<Entity, With<MenuEntity>>) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
}
