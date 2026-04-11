use bevy::prelude::*;

use crate::domain::dice::DiceRoll;
use crate::domain::piece::PieceState;
use crate::domain::player::PlayerControl;
use crate::gameplay::match_flow::{
    BoardLayout, MatchConfig, MatchResult, PlayerRoster, TeamRoster,
};
use crate::gameplay::turn_flow::{
    choose_action, collect_actions, current_player_control, execute_action,
    find_pending_action_by_piece_id, finish_turn_without_action, get_pending_action,
    pressed_selection_key, roll_die, set_pending_actions, set_roll, TurnInputState, TurnState,
};
use crate::plugins::piece_plugin::{HangarSlot, PieceId};
use crate::states::{AppState, GamePhase};

pub struct TurnPlugin;

impl Plugin for TurnPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), setup_turn_automation)
            .add_systems(
                Update,
                (
                    drive_ai_turn_loop,
                    handle_human_roll_input,
                    handle_human_action_input,
                    handle_human_action_click,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(OnExit(AppState::InGame), cleanup_turn_automation);
    }
}

#[derive(Resource)]
struct TurnAutomation {
    timer: Timer,
}

fn setup_turn_automation(mut commands: Commands) {
    commands.insert_resource(TurnAutomation {
        timer: Timer::from_seconds(0.9, TimerMode::Repeating),
    });
    commands.insert_resource(TurnInputState::default());
}

fn cleanup_turn_automation(mut commands: Commands) {
    commands.remove_resource::<TurnAutomation>();
    commands.remove_resource::<TurnInputState>();
}

fn drive_ai_turn_loop(
    time: Res<Time>,
    mut automation: ResMut<TurnAutomation>,
    mut input_state: ResMut<TurnInputState>,
    mut turn_state: ResMut<TurnState>,
    mut next_phase: ResMut<NextState<GamePhase>>,
    game_phase: Res<State<GamePhase>>,
    board_layout: Res<BoardLayout>,
    match_config: Res<MatchConfig>,
    player_roster: Res<PlayerRoster>,
    team_roster: Res<TeamRoster>,
    mut match_result: ResMut<MatchResult>,
    mut piece_query: Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) {
    if !matches!(game_phase.get(), GamePhase::AwaitDice) || match_result.finished {
        return;
    }

    if current_player_control(turn_state.current_player, &player_roster) != Some(PlayerControl::Ai)
    {
        return;
    }

    if !automation.timer.tick(time.delta()).just_finished() {
        return;
    }

    let roll_value = roll_die();
    let roll = DiceRoll(roll_value);
    set_roll(&mut turn_state, roll_value);

    let current_player = turn_state.current_player;
    let Some(action) =
        choose_action(current_player, roll, &board_layout, &player_roster, &piece_query)
    else {
        turn_state.last_action =
            Some(format!("P{current_player} rolled {roll_value} but had no legal action"));
        finish_turn_without_action(
            &mut turn_state,
            &mut input_state,
            &player_roster,
            &mut next_phase,
        );
        return;
    };

    execute_action(
        action,
        roll_value,
        &player_roster,
        &team_roster,
        &match_config,
        &board_layout,
        &mut piece_query,
        &mut match_result,
        &mut turn_state,
        &mut input_state,
        &mut next_phase,
    );
}

fn handle_human_roll_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut input_state: ResMut<TurnInputState>,
    mut turn_state: ResMut<TurnState>,
    mut next_phase: ResMut<NextState<GamePhase>>,
    game_phase: Res<State<GamePhase>>,
    board_layout: Res<BoardLayout>,
    match_config: Res<MatchConfig>,
    player_roster: Res<PlayerRoster>,
    team_roster: Res<TeamRoster>,
    mut match_result: ResMut<MatchResult>,
    mut piece_query: Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) {
    if !matches!(game_phase.get(), GamePhase::AwaitDice) || match_result.finished {
        return;
    }

    if current_player_control(turn_state.current_player, &player_roster)
        != Some(PlayerControl::Human)
    {
        return;
    }

    input_state.prompt = Some("Press Space to roll".to_string());

    if !keyboard.just_pressed(KeyCode::Space) {
        return;
    }

    let roll_value = roll_die();
    let roll = DiceRoll(roll_value);
    set_roll(&mut turn_state, roll_value);

    let actions = collect_actions(turn_state.current_player, roll, &player_roster, &piece_query);

    if actions.is_empty() {
        turn_state.last_action = Some(format!(
            "P{} rolled {} but had no legal action",
            turn_state.current_player, roll_value
        ));
        finish_turn_without_action(
            &mut turn_state,
            &mut input_state,
            &player_roster,
            &mut next_phase,
        );
        return;
    }

    if actions.len() == 1 {
        execute_action(
            actions[0],
            roll_value,
            &player_roster,
            &team_roster,
            &match_config,
            &board_layout,
            &mut piece_query,
            &mut match_result,
            &mut turn_state,
            &mut input_state,
            &mut next_phase,
        );
        return;
    }

    set_pending_actions(&mut input_state, roll_value, actions, &mut next_phase);
}

fn handle_human_action_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut input_state: ResMut<TurnInputState>,
    mut turn_state: ResMut<TurnState>,
    mut next_phase: ResMut<NextState<GamePhase>>,
    game_phase: Res<State<GamePhase>>,
    board_layout: Res<BoardLayout>,
    match_config: Res<MatchConfig>,
    player_roster: Res<PlayerRoster>,
    team_roster: Res<TeamRoster>,
    mut match_result: ResMut<MatchResult>,
    mut piece_query: Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) {
    if !matches!(game_phase.get(), GamePhase::AwaitPieceSelect) || match_result.finished {
        return;
    }

    let Some(selection) = pressed_selection_key(&keyboard, input_state.candidate_piece_ids().len())
    else {
        return;
    };
    let Some(action) = get_pending_action(&input_state, selection) else {
        return;
    };

    let roll_value = turn_state.last_roll.unwrap_or_default();
    execute_action(
        action,
        roll_value,
        &player_roster,
        &team_roster,
        &match_config,
        &board_layout,
        &mut piece_query,
        &mut match_result,
        &mut turn_state,
        &mut input_state,
        &mut next_phase,
    );
}

fn handle_human_action_click(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform)>,
    mut input_state: ResMut<TurnInputState>,
    mut turn_state: ResMut<TurnState>,
    mut next_phase: ResMut<NextState<GamePhase>>,
    game_phase: Res<State<GamePhase>>,
    board_layout: Res<BoardLayout>,
    match_config: Res<MatchConfig>,
    player_roster: Res<PlayerRoster>,
    team_roster: Res<TeamRoster>,
    mut match_result: ResMut<MatchResult>,
    mut piece_query: Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) {
    if !matches!(game_phase.get(), GamePhase::AwaitPieceSelect) || match_result.finished {
        return;
    }

    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor_position) = window.cursor_position() else {
        return;
    };
    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };
    let Ok(cursor_world) = camera.viewport_to_world_2d(camera_transform, cursor_position) else {
        return;
    };

    let mut selected_piece_id = None;
    let mut best_distance_sq = f32::MAX;
    for (piece_id, _, _, transform) in &mut piece_query {
        if !input_state.candidate_piece_ids().contains(&piece_id.0) {
            continue;
        }

        let distance_sq = transform.translation.truncate().distance_squared(cursor_world);
        if distance_sq <= 28.0 * 28.0 && distance_sq < best_distance_sq {
            best_distance_sq = distance_sq;
            selected_piece_id = Some(piece_id.0);
        }
    }

    let Some(selected_piece_id) = selected_piece_id else {
        return;
    };
    let Some(action) = find_pending_action_by_piece_id(&input_state, selected_piece_id) else {
        return;
    };

    let roll_value = turn_state.last_roll.unwrap_or_default();
    execute_action(
        action,
        roll_value,
        &player_roster,
        &team_roster,
        &match_config,
        &board_layout,
        &mut piece_query,
        &mut match_result,
        &mut turn_state,
        &mut input_state,
        &mut next_phase,
    );
}
