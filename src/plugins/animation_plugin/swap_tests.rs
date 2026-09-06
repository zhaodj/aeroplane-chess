use super::*;
use crate::data::{game_mode::GameMode, rule_set::RuleSet};
use crate::domain::{player::PlayerControl, rules::LaunchRule};
use crate::gameplay::ai::AiDifficulty;
use crate::gameplay::match_flow::{MatchSetup, PlayerSeat, build_match_resources};
use crate::gameplay::swap_flow::{
    SWAP_MOTION_PENDING, execute_selected_swap, swapped_piece_states,
};
use crate::gameplay::turn_flow::{
    HOME_ENTRY_PROGRESS, MIN_ROUTE_PROGRESS, compute_move_target_distance_on_board,
};
use crate::plugins::piece_plugin::HangarSlot;
use bevy::ecs::system::SystemState;
use bevy::time::TimeUpdateStrategy;
use std::time::Duration;

fn setup(
    owner: u8,
    landing: PieceProgress,
    fast: bool,
) -> (App, [Entity; 2], BoardLayout, PlayerRoster) {
    let setup = MatchSetup {
        mode: GameMode::FreeForAll,
        rule_set: RuleSet::Creative,
        ai_difficulty: AiDifficulty::Normal,
        fast_mode: fast,
        launch_rule: LaunchRule::SixOnly,
        player_seats: PlayerSeat::ALL,
        pieces_per_player: 1,
        player_controls: [PlayerControl::Human; 4],
    };
    let config = MatchConfig {
        mode: setup.mode,
        rule_set: setup.rule_set,
        ai_difficulty: setup.ai_difficulty,
        fast_mode: fast,
        launch_rule: setup.launch_rule,
        player_seats: setup.player_seats,
        pieces_per_player: 1,
        player_controls: setup.player_controls,
    };
    let (board, roster, _) = build_match_resources(&setup);
    let other = owner % 4 + 1;
    let wanted = pos(owner, landing, &board, &roster);
    let other_progress = (MIN_ROUTE_PROGRESS..=HOME_ENTRY_PROGRESS)
        .find(|&p| pos(other, p, &board, &roster) == wanted)
        .unwrap();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
        .insert_state(AppState::InGame)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            20,
        )))
        .insert_resource(config)
        .insert_resource(board.clone())
        .insert_resource(roster.clone())
        .insert_resource(TurnState {
            current_player: owner,
            ..default()
        })
        .init_resource::<PieceMotionEffects>()
        .init_resource::<VisualEffectQueue>()
        .add_plugins(AnimationPlugin);
    let entities = [(owner, 3), (other, other_progress)].map(|(player, progress)| {
        app.world_mut()
            .spawn((
                PieceId(player),
                HangarSlot(Vec2::ZERO),
                PieceState {
                    owner_player_id: player,
                    team_id: player,
                    status: PieceStatus::Active,
                    progress,
                    shield: 0,
                    stack_shield: 0,
                    motion_serial: 0,
                },
                Transform::from_translation(pos(player, progress, &board, &roster)),
            ))
            .id()
    });
    app.update();
    (app, entities, board, roster)
}

fn pos(owner: u8, progress: PieceProgress, board: &BoardLayout, roster: &PlayerRoster) -> Vec3 {
    world_position_for_piece(owner, progress, PieceStatus::Active, board, roster)
        .unwrap()
        .extend(1.0)
}

fn swap(
    app: &mut App,
    owner: u8,
    board: &BoardLayout,
    roster: &PlayerRoster,
) -> Result<String, &'static str> {
    let mut system: SystemState<(
        Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        MovingPieceQuery,
    )> = SystemState::new(app.world_mut());
    let (mut query, moving) = system.get_mut(app.world_mut()).unwrap();
    execute_selected_swap(
        owner,
        GameMode::FreeForAll,
        owner,
        owner % 4 + 1,
        board,
        roster,
        &mut query,
        &moving,
    )
}

fn move_to(
    app: &mut App,
    entity: Entity,
    progress: PieceProgress,
    board: &BoardLayout,
    roster: &PlayerRoster,
) {
    let owner = {
        let mut piece = app.world_mut().get_mut::<PieceState>(entity).unwrap();
        piece.progress = progress;
        piece.motion_serial = piece.motion_serial.wrapping_add(1);
        piece.owner_player_id
    };
    app.world_mut()
        .get_mut::<Transform>(entity)
        .unwrap()
        .translation = pos(owner, progress, board, roster);
}

fn settle(app: &mut App) {
    // The coincident-home-entry fixture first traverses almost the full main route.
    for _ in 0..600 {
        app.update();
        if app
            .world_mut()
            .query_filtered::<Entity, With<PieceMoveAnimation>>()
            .iter(app.world())
            .count()
            == 0
        {
            app.update();
            return;
        }
    }
    panic!("animation never settled");
}

#[test]
fn settled_swap_next_move_starts_at_new_location_all_colors_and_boundaries() {
    let mut cases = 0;
    for owner in 1..=4 {
        for landing in [-2, -1, 0, 5, 24, 48, 49] {
            for fast in [false, true] {
                for roll in [1, 3, 6] {
                    let (mut app, entities, board, roster) = setup(owner, landing, fast);
                    swap(&mut app, owner, &board, &roster).unwrap();
                    settle(&mut app);
                    for entity in entities {
                        let piece = *app.world().get::<PieceState>(entity).unwrap();
                        let expected = pos(piece.owner_player_id, piece.progress, &board, &roster);
                        assert_eq!(
                            app.world().get::<Transform>(entity).unwrap().translation,
                            expected
                        );
                        assert_eq!(
                            app.world()
                                .get::<PieceAnimationState>(entity)
                                .unwrap()
                                .logical_translation,
                            expected
                        );
                        let target = compute_move_target_distance_on_board(
                            piece.owner_player_id,
                            piece.status,
                            piece.progress,
                            roll,
                            &board,
                            &roster,
                        )
                        .unwrap();
                        move_to(&mut app, entity, target, &board, &roster);
                        app.update();
                        let animation = app.world().get::<PieceMoveAnimation>(entity).unwrap();
                        assert_eq!(
                            animation.waypoints[0], expected,
                            "owner={owner} landing={landing} fast={fast} roll={roll}"
                        );
                        settle(&mut app);
                        assert_eq!(
                            app.world().get::<Transform>(entity).unwrap().translation,
                            pos(piece.owner_player_id, target, &board, &roster)
                        );
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases, 336);
}

#[test]
fn swapping_either_moving_aircraft_is_atomic_and_retry_after_landing_succeeds() {
    for moving_index in [0, 1] {
        let (mut app, entities, board, roster) = setup(1, 24, false);
        let moving = entities[moving_index];
        let original = *app.world().get::<PieceState>(moving).unwrap();
        move_to(&mut app, moving, original.progress + 6, &board, &roster);
        app.update();
        assert!(app.world().get::<PieceMoveAnimation>(moving).is_some());
        let before = entities.map(|entity| {
            (
                *app.world().get::<PieceState>(entity).unwrap(),
                app.world().get::<Transform>(entity).unwrap().translation,
            )
        });
        assert_eq!(swap(&mut app, 1, &board, &roster), Err(SWAP_MOTION_PENDING));
        assert_eq!(
            before,
            entities.map(|entity| (
                *app.world().get::<PieceState>(entity).unwrap(),
                app.world().get::<Transform>(entity).unwrap().translation,
            ))
        );
        settle(&mut app);
        swap(&mut app, 1, &board, &roster).unwrap();
        settle(&mut app);
        for entity in entities {
            let piece = *app.world().get::<PieceState>(entity).unwrap();
            let expected = pos(piece.owner_player_id, piece.progress, &board, &roster);
            assert_eq!(
                app.world().get::<Transform>(entity).unwrap().translation,
                expected
            );
            let target = compute_move_target_distance_on_board(
                piece.owner_player_id,
                piece.status,
                piece.progress,
                1,
                &board,
                &roster,
            )
            .unwrap();
            move_to(&mut app, entity, target, &board, &roster);
            app.update();
            assert_eq!(
                app.world()
                    .get::<PieceMoveAnimation>(entity)
                    .unwrap()
                    .waypoints[0],
                expected
            );
            settle(&mut app);
        }
    }
}

#[test]
fn interrupted_old_motion_cannot_overwrite_new_state_or_next_origin() {
    // Fault injection bypasses the commit guard to test animation-layer defence.
    // Test normal route motion and a lifted swap arc (whose transient z must not persist).
    for interrupt_swap_arc in [false, true] {
        let (mut app, entities, board, roster) = setup(1, 24, false);
        let moving = entities[1];
        if interrupt_swap_arc {
            swap(&mut app, 1, &board, &roster).unwrap();
        } else {
            let original = *app.world().get::<PieceState>(moving).unwrap();
            move_to(&mut app, moving, original.progress + 6, &board, &roster);
        }
        app.update();
        let rendered_before = app.world().get::<Transform>(moving).unwrap().translation;
        assert!(app.world().get::<PieceMoveAnimation>(moving).is_some());
        let first = *app.world().get::<PieceState>(entities[0]).unwrap();
        let second = *app.world().get::<PieceState>(entities[1]).unwrap();
        let (first, second) = swapped_piece_states(first, second, &roster).unwrap();
        for (entity, piece) in entities.into_iter().zip([first, second]) {
            *app.world_mut().get_mut::<PieceState>(entity).unwrap() = piece;
            let mut transform = app.world_mut().get_mut::<Transform>(entity).unwrap();
            transform.translation = pos(piece.owner_player_id, piece.progress, &board, &roster)
                .with_z(transform.translation.z);
        }
        let expected = pos(second.owner_player_id, second.progress, &board, &roster);
        app.update();
        let replacement = app.world().get::<PieceMoveAnimation>(moving).unwrap();
        assert_eq!(replacement.waypoints[0], rendered_before);
        assert_eq!(*replacement.waypoints.last().unwrap(), expected);
        settle(&mut app);
        assert_eq!(
            app.world().get::<Transform>(moving).unwrap().translation,
            expected
        );
        assert_eq!(
            app.world()
                .get::<PieceAnimationState>(moving)
                .unwrap()
                .logical_translation,
            expected
        );
        let target = compute_move_target_distance_on_board(
            second.owner_player_id,
            second.status,
            second.progress,
            1,
            &board,
            &roster,
        )
        .unwrap();
        move_to(&mut app, moving, target, &board, &roster);
        app.update();
        assert_eq!(
            app.world()
                .get::<PieceMoveAnimation>(moving)
                .unwrap()
                .waypoints[0],
            expected
        );
    }
}

#[test]
fn swapping_coincident_aircraft_at_home_entry_does_not_create_a_finish_bounce() {
    let (mut app, entities, board, roster) = setup(1, HOME_ENTRY_PROGRESS, false);
    move_to(&mut app, entities[0], HOME_ENTRY_PROGRESS, &board, &roster);
    settle(&mut app);
    let before = entities.map(|entity| app.world().get::<Transform>(entity).unwrap().translation);
    assert_eq!(before[0], before[1]);
    swap(&mut app, 1, &board, &roster).unwrap();
    app.update();
    for (index, entity) in entities.into_iter().enumerate() {
        assert!(app.world().get::<PieceMoveAnimation>(entity).is_none());
        assert_eq!(
            app.world().get::<Transform>(entity).unwrap().translation,
            before[index]
        );
        assert_eq!(
            app.world()
                .get::<PieceAnimationState>(entity)
                .unwrap()
                .logical_translation,
            before[index]
        );
    }
}
