use bevy::prelude::*;

use crate::constants::{HUD_PANEL_WIDTH, HUD_Z_LAYER};
use crate::domain::player::PlayerControl;
use crate::gameplay::match_flow::{MatchConfig, MatchResult, PlayerRoster, TeamRoster};
use crate::gameplay::skill_flow::{player_skill_state, SkillRoster};
use crate::gameplay::turn_flow::{TurnInputState, TurnState};
use crate::plugins::skill_plugin::{SkillTargetState, SkillUiAction, SkillUiRequest};
use crate::states::{AppState, GamePhase};

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_hud)
            .add_systems(
                Update,
                (update_hud, handle_skill_panel_click).run_if(in_state(AppState::InGame)),
            )
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
struct HudSkillsText;

#[derive(Component)]
struct HudPromptText;

#[derive(Component)]
struct HudSkillButton {
    action: SkillUiAction,
}

const HUD_SKILL_PANEL_LEFT: f32 = 20.0;
const HUD_SKILL_PANEL_RIGHT: f32 = 380.0;
const HUD_SKILL_ROW_TOPS: [f32; 5] = [136.0, 160.0, 184.0, 208.0, 232.0];
const HUD_SKILL_ROW_HEIGHT: f32 = 22.0;

fn spawn_hud(
    mut commands: Commands,
) {
    commands.spawn((
        Sprite::from_color(
            Color::srgba(0.98, 0.99, 1.0, 0.90),
            Vec2::new(HUD_PANEL_WIDTH, 282.0),
        ),
        Transform::from_xyz(-365.0, 212.0, HUD_Z_LAYER),
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
            top: Val::Px(22.0),
            left: Val::Px(28.0),
            ..default()
        },
        Name::new("HudPrimaryText"),
        HudPrimaryText,
        HudEntity,
    ));
    for (row_index, action) in [
        SkillUiAction::Dash,
        SkillUiAction::Snipe,
        SkillUiAction::Swap,
        SkillUiAction::Shield,
        SkillUiAction::DoubleDice,
    ]
    .iter()
    .enumerate()
    {
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(HUD_SKILL_PANEL_LEFT),
                top: Val::Px(HUD_SKILL_ROW_TOPS[row_index]),
                width: Val::Px(HUD_SKILL_PANEL_RIGHT - HUD_SKILL_PANEL_LEFT),
                height: Val::Px(HUD_SKILL_ROW_HEIGHT),
                ..default()
            },
            BackgroundColor(skill_button_color(false, false)),
            Name::new(format!("HudSkillButton{:?}", action)),
            HudSkillButton { action: *action },
            HudEntity,
        ));
    }
    commands.spawn((
        Text::new(""),
        TextFont {
            font_size: 18.0,
            ..default()
        },
        TextColor(Color::srgb(0.20, 0.28, 0.40)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(116.0),
            left: Val::Px(28.0),
            ..default()
        },
        Name::new("HudSkillsText"),
        HudSkillsText,
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
            top: Val::Px(252.0),
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
    mut primary_query: Query<&mut Text, (With<HudPrimaryText>, Without<HudSkillsText>, Without<HudPromptText>)>,
    mut skills_query: Query<&mut Text, (With<HudSkillsText>, Without<HudPrimaryText>, Without<HudPromptText>)>,
    mut prompt_query: Query<&mut Text, (With<HudPromptText>, Without<HudPrimaryText>, Without<HudSkillsText>)>,
    mut skill_button_query: Query<(&HudSkillButton, &mut BackgroundColor)>,
) {
    let Ok(mut primary_text) = primary_query.single_mut() else {
        return;
    };
    let Ok(mut skills_text_node) = skills_query.single_mut() else {
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
    let is_human_turn = matches!(current_control, "Human");
    let can_use_skill = skill_roster.active_turn_player == Some(turn_state.current_player)
        && !skill_roster.skill_used_this_turn
        && is_human_turn;
    let current_skills = player_skill_state(&skill_roster, turn_state.current_player);
    for (button, mut background) in &mut skill_button_query {
        let ready = current_skills
            .map(|skills| is_skill_button_ready(
                button.action,
                skills,
                can_use_skill,
                game_phase.get(),
                match_config.mode,
            ))
            .unwrap_or(false);
        *background = BackgroundColor(skill_button_color(ready, can_use_skill));
    }
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
    let skill_text = current_skills
        .map(|skills| format_skill_panel(skills, is_human_turn, can_use_skill, match_config.mode, game_phase.get()))
        .unwrap_or_else(|| {
            "Skills\nDash [E]: -\nSnipe [S]: -\nSwap [A]: -\nShield [Q]: -\nDoubleDice [W]: -".to_string()
        });

    *primary_text = Text::new(format!(
        "Mode: {:?}  |  AI: {:?}\nTurn: P{} ({})  |  Round: {}\nPhase: {}  |  Last Roll: {}\nPlayers: {}  |  Teams: {}\n{}\nLast Skill: {}\nLast Action: {}",
        match_config.mode,
        match_config.ai_difficulty,
        turn_state.current_player,
        current_control,
        turn_state.turn_index,
        phase_label,
        roll_text,
        player_roster.players.len(),
        team_roster.teams.len(),
        skill_action_text,
        result_text,
        action_text,
    ));
    *skills_text_node = Text::new(skill_text);
    *prompt_text_node = Text::new(if stacked_hint.is_empty() {
        format!("Prompt: {prompt_text}")
    } else {
        format!("Prompt: {prompt_text}\n{stacked_hint}")
    });
}

fn is_skill_button_ready(
    action: SkillUiAction,
    skills: &crate::gameplay::skill_flow::PlayerSkillState,
    can_use_skill: bool,
    phase: &GamePhase,
    mode: crate::data::game_mode::GameMode,
) -> bool {
    if !can_use_skill {
        return false;
    }
    match action {
        SkillUiAction::Dash => {
            matches!(phase, GamePhase::AwaitPieceSelect) && !skills.dash_armed && skills.dash_charges > 0
        }
        SkillUiAction::Snipe => matches!(phase, GamePhase::AwaitDice) && skills.snipe_charges > 0,
        SkillUiAction::Swap => {
            matches!(phase, GamePhase::AwaitDice) && mode == crate::data::game_mode::GameMode::TwoVsTwo && skills.swap_charges > 0
        }
        SkillUiAction::Shield => matches!(phase, GamePhase::AwaitDice) && skills.shield_charges > 0,
        SkillUiAction::DoubleDice => {
            matches!(phase, GamePhase::AwaitDice) && !skills.double_dice_armed && skills.double_dice_charges > 0
        }
    }
}

fn skill_button_color(ready: bool, can_use_skill: bool) -> Color {
    if ready {
        Color::srgba(0.53, 0.77, 0.96, 0.42)
    } else if can_use_skill {
        Color::srgba(0.78, 0.82, 0.89, 0.28)
    } else {
        Color::srgba(0.70, 0.73, 0.79, 0.16)
    }
}

fn handle_skill_panel_click(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    game_phase: Res<State<GamePhase>>,
    match_result: Res<MatchResult>,
    player_roster: Res<PlayerRoster>,
    turn_state: Res<TurnState>,
    skill_roster: Res<SkillRoster>,
    mut skill_ui_request: ResMut<SkillUiRequest>,
) {
    if match_result.finished
        || !matches!(game_phase.get(), GamePhase::AwaitDice | GamePhase::AwaitPieceSelect)
        || !mouse.just_pressed(MouseButton::Left)
    {
        return;
    }

    let Some(current_player) = player_roster
        .players
        .iter()
        .find(|player| player.state.player_id == turn_state.current_player)
    else {
        return;
    };
    if current_player.state.control != PlayerControl::Human {
        return;
    }

    let can_use_skill = skill_roster.active_turn_player == Some(turn_state.current_player)
        && !skill_roster.skill_used_this_turn;
    if !can_use_skill {
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    if cursor.x < HUD_SKILL_PANEL_LEFT || cursor.x > HUD_SKILL_PANEL_RIGHT {
        return;
    }

    let Some(action) = skill_action_for_cursor_y(cursor.y) else {
        return;
    };
    skill_ui_request.queue(action);
}

fn skill_action_for_cursor_y(cursor_y: f32) -> Option<SkillUiAction> {
    HUD_SKILL_ROW_TOPS
        .iter()
        .enumerate()
        .find_map(|(index, row_top)| {
            (cursor_y >= *row_top && cursor_y <= *row_top + HUD_SKILL_ROW_HEIGHT).then_some(index)
        })
        .and_then(|index| match index {
            0 => Some(SkillUiAction::Dash),
            1 => Some(SkillUiAction::Snipe),
            2 => Some(SkillUiAction::Swap),
            3 => Some(SkillUiAction::Shield),
            4 => Some(SkillUiAction::DoubleDice),
            _ => None,
        })
}

fn format_skill_panel(
    skills: &crate::gameplay::skill_flow::PlayerSkillState,
    is_human_turn: bool,
    can_use_skill: bool,
    mode: crate::data::game_mode::GameMode,
    phase: &GamePhase,
) -> String {
    let header = if is_human_turn {
        if can_use_skill {
            "Skills ready"
        } else {
            "Skill slot spent"
        }
    } else {
        "AI skills auto"
    };
    let dash_state = if skills.dash_armed {
        "armed +3 move"
    } else if skills.dash_charges == 0 {
        "cooldown"
    } else {
        "ready"
    };
    let snipe_state = if matches!(phase, GamePhase::ResolveSkillEffect) {
        "targeting"
    } else if skills.snipe_charges == 0 {
        "reloading"
    } else {
        "ready"
    };
    let swap_state = if mode == crate::data::game_mode::GameMode::TwoVsTwo {
        if skills.swap_charges == 0 {
            "empty"
        } else {
            "ready"
        }
    } else {
        "locked (2v2)"
    };
    let shield_state = if skills.shield_charges == 0 {
        "empty"
    } else {
        "ready"
    };
    let dice_state = if skills.double_dice_armed {
        "armed"
    } else if skills.double_dice_charges == 0 {
        "empty"
    } else {
        "ready"
    };

    format!(
        "{header}\n[Dash][E]: {dash_state} | {}/tries\n[Snipe][S]: {snipe_state} | {}/tries\n[Swap][A]: {swap_state} | {}/tries\n[Shield][Q]: {shield_state} | {}/tries\n[DoubleDice][W]: {dice_state} | {}/tries",
        skills.dash_charges,
        skills.snipe_charges,
        skills.swap_charges,
        skills.shield_charges,
        skills.double_dice_charges,
    )
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
