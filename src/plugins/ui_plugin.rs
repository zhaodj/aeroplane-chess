use bevy::prelude::*;

use crate::constants::HUD_Z_LAYER;
use crate::plugins::turn_plugin::TurnInputState;
use crate::plugins::game_plugin::{MatchConfig, MatchResult, PlayerRoster, TeamRoster};
use crate::gameplay::turn_flow::TurnState;
use crate::states::{AppState, GamePhase};

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_hud)
            .add_systems(Update, update_hud.run_if(in_state(AppState::InGame)))
            .add_systems(OnExit(AppState::InGame), cleanup_hud);
    }
}

#[derive(Component)]
struct HudEntity;

fn spawn_hud(
    mut commands: Commands,
) {
    commands.spawn((
        Text::new("Loading HUD..."),
        TextFont {
            font_size: 28.0,
            ..default()
        },
        TextColor(Color::srgb(0.12, 0.16, 0.24)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(20.0),
            left: Val::Px(20.0),
            ..default()
        },
        GlobalZIndex(HUD_Z_LAYER as i32),
        Name::new("HudText"),
        HudEntity,
    ));
}

fn update_hud(
    match_config: Res<MatchConfig>,
    player_roster: Res<PlayerRoster>,
    team_roster: Res<TeamRoster>,
    match_result: Res<MatchResult>,
    input_state: Res<TurnInputState>,
    turn_state: Res<TurnState>,
    game_phase: Res<State<GamePhase>>,
    mut query: Query<&mut Text, With<HudEntity>>,
) {
    let Ok(mut text) = query.single_mut() else {
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
    let result_text = if match_result.finished {
        format!(
            " | Result: Team {} wins",
            match_result.winner_team_id.unwrap_or_default()
        )
    } else {
        String::new()
    };
    let prompt_text = input_state
        .prompt
        .as_deref()
        .map(|prompt| format!(" | Prompt: {prompt}"))
        .unwrap_or_default();

    *text = Text::new(format!(
        "Mode: {:?} | AI: {:?} | Players: {} | Teams: {} | Turn: P{} / {} | Phase: {:?} | Last Roll: {} | Last Action: {}{}{}",
        match_config.mode,
        match_config.ai_difficulty,
        player_roster.players.len(),
        team_roster.teams.len(),
        turn_state.current_player,
        turn_state.turn_index,
        game_phase.get(),
        roll_text,
        action_text,
        result_text,
        prompt_text,
    ));
}

fn cleanup_hud(mut commands: Commands, query: Query<Entity, With<HudEntity>>) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
}
