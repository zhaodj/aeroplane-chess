use bevy::prelude::*;

use crate::constants::{HUD_PANEL_WIDTH, HUD_Z_LAYER};
use crate::domain::player::PlayerControl;
use crate::gameplay::match_flow::{MatchConfig, MatchResult, PlayerRoster, TeamRoster};
use crate::gameplay::skill_flow::{player_skill_state, SkillRoster};
use crate::gameplay::turn_flow::{TurnInputState, TurnState};
use crate::plugins::skill_plugin::SkillTargetState;
use crate::states::{AppState, GamePhase};

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_hud)
            .add_systems(Update, update_hud.run_if(in_state(AppState::InGame)))
            .add_systems(OnEnter(AppState::Result), spawn_result_screen)
            .add_systems(
                Update,
                handle_result_input.run_if(in_state(AppState::Result)),
            )
            .add_systems(OnExit(AppState::InGame), cleanup_hud)
            .add_systems(OnExit(AppState::Result), cleanup_result);
    }
}

#[derive(Component)]
struct HudEntity;

#[derive(Component)]
struct ResultEntity;

#[derive(Component)]
struct HudPrimaryText;

#[derive(Component)]
struct HudPromptText;

fn spawn_hud(
    mut commands: Commands,
) {
    commands.spawn((
        Sprite::from_color(
            Color::srgba(0.98, 0.99, 1.0, 0.90),
            Vec2::new(HUD_PANEL_WIDTH, 148.0),
        ),
        Transform::from_xyz(-365.0, 280.0, HUD_Z_LAYER),
        Name::new("HudPanelBackdrop"),
        HudEntity,
    ));
    commands.spawn((
        Text::new("Loading HUD..."),
        TextFont {
            font_size: 24.0,
            ..default()
        },
        TextColor(Color::srgb(0.10, 0.16, 0.24)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(28.0),
            left: Val::Px(28.0),
            ..default()
        },
        Name::new("HudPrimaryText"),
        HudPrimaryText,
        HudEntity,
    ));
    commands.spawn((
        Text::new(""),
        TextFont {
            font_size: 18.0,
            ..default()
        },
        TextColor(Color::srgb(0.28, 0.35, 0.46)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(122.0),
            left: Val::Px(28.0),
            ..default()
        },
        Name::new("HudPromptText"),
        HudPromptText,
        HudEntity,
    ));
}

fn update_hud(
    match_config: Res<MatchConfig>,
    player_roster: Res<PlayerRoster>,
    team_roster: Res<TeamRoster>,
    match_result: Res<MatchResult>,
    skill_roster: Res<SkillRoster>,
    skill_target_state: Res<SkillTargetState>,
    input_state: Res<TurnInputState>,
    turn_state: Res<TurnState>,
    game_phase: Res<State<GamePhase>>,
    mut primary_query: Query<&mut Text, (With<HudPrimaryText>, Without<HudPromptText>)>,
    mut prompt_query: Query<&mut Text, (With<HudPromptText>, Without<HudPrimaryText>)>,
) {
    let Ok(mut primary_text) = primary_query.single_mut() else {
        return;
    };
    let Ok(mut prompt_text_node) = prompt_query.single_mut() else {
        return;
    };

    let roll_text = match turn_state.last_roll {
        Some(value) => value.to_string(),
        None => "-".to_string(),
    };
    let action_text = turn_state
        .last_action
        .as_deref()
        .unwrap_or("waiting for first action");
    let skill_text = player_skill_state(&skill_roster, turn_state.current_player)
        .map(|skills| {
            format!(
                "Dash: {}{}  |  Snipe: {}  |  Swap: {}  |  Shield: {}  |  DoubleDice: {}{}",
                skills.dash_charges,
                if skills.dash_armed { " (armed)" } else { "" },
                skills.snipe_charges,
                skills.swap_charges,
                skills.shield_charges,
                skills.double_dice_charges,
                if skills.double_dice_armed { " (armed)" } else { "" }
            )
        })
        .unwrap_or_else(|| "Dash: -  |  Snipe: -  |  Swap: -  |  Shield: -  |  DoubleDice: -".to_string());
    let skill_action_text = skill_roster
        .last_skill_action
        .as_deref()
        .unwrap_or("none");
    let current_control = player_roster
        .players
        .iter()
        .find(|player| player.state.player_id == turn_state.current_player)
        .map(|player| match player.state.control {
            PlayerControl::Human => "Human",
            PlayerControl::Ai => "AI",
        })
        .unwrap_or("-");
    let phase_label = match game_phase.get() {
        GamePhase::AwaitDice => "Roll",
        GamePhase::AwaitPieceSelect => "Choose Piece",
        GamePhase::CheckVictory => "Victory Check",
        _ => "Resolving",
    };
    let result_text = if match_result.finished {
        format!("Result: Team {} wins", match_result.winner_team_id.unwrap_or_default())
    } else {
        "Result: in progress".to_string()
    };
    let stacked_hint = if matches!(game_phase.get(), GamePhase::AwaitPieceSelect) {
        "Highlighted teammate stacks share one shield.".to_string()
    } else {
        String::new()
    };
    let prompt_text = skill_target_state
        .prompt
        .as_deref()
        .or(input_state.prompt.as_deref())
        .unwrap_or("Space rolls. Q uses Shield. S uses Snipe. A uses Swap. W arms DoubleDice. E arms Dash after rolling.");

    *primary_text = Text::new(format!(
        "Mode: {:?}  |  AI: {:?}\nTurn: P{} ({})  |  Round: {}\nPhase: {}  |  Last Roll: {}\nPlayers: {}  |  Teams: {}\nSkills: {}\nLast Skill: {}\n{}\nLast Action: {}",
        match_config.mode,
        match_config.ai_difficulty,
        turn_state.current_player,
        current_control,
        turn_state.turn_index,
        phase_label,
        roll_text,
        player_roster.players.len(),
        team_roster.teams.len(),
        skill_text,
        skill_action_text,
        result_text,
        action_text,
    ));
    *prompt_text_node = Text::new(if stacked_hint.is_empty() {
        format!("Prompt: {prompt_text}")
    } else {
        format!("Prompt: {prompt_text}\n{stacked_hint}")
    });
}

fn cleanup_hud(mut commands: Commands, query: Query<Entity, With<HudEntity>>) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
}

fn spawn_result_screen(
    mut commands: Commands,
    match_result: Res<MatchResult>,
) {
    let winner = match_result.winner_team_id.unwrap_or_default();
    commands.spawn((
        Sprite::from_color(
            Color::srgba(0.98, 0.99, 1.0, 0.94),
            Vec2::new(420.0, 220.0),
        ),
        Transform::from_xyz(0.0, 40.0, HUD_Z_LAYER),
        Name::new("ResultBackdrop"),
        ResultEntity,
    ));
    commands.spawn((
        Text::new(format!(
            "Match Result\n\nTeam {} wins\n\nR: Restart Match\nEsc: Main Menu",
            winner
        )),
        TextFont {
            font_size: 40.0,
            ..default()
        },
        TextColor(Color::srgb(0.10, 0.16, 0.24)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Percent(28.0),
            left: Val::Percent(22.0),
            ..default()
        },
        Name::new("ResultText"),
        ResultEntity,
    ));
}

fn handle_result_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut next_app_state: ResMut<NextState<AppState>>,
) {
    if keyboard.just_pressed(KeyCode::KeyR) {
        next_app_state.set(AppState::LoadingGame);
    } else if keyboard.just_pressed(KeyCode::Escape) {
        next_app_state.set(AppState::MainMenu);
    }
}

fn cleanup_result(mut commands: Commands, query: Query<Entity, With<ResultEntity>>) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
}
