use bevy::prelude::*;

use crate::domain::piece::{PieceState, PieceStatus};
use crate::gameplay::match_flow::{BoardLayout, MatchConfig, PlayerRoster};
use crate::gameplay::turn_flow::{
    FINISH_DISTANCE, movement_steps_between_progresses, world_position_for_piece,
};
use crate::plugins::piece_plugin::PieceId;
use crate::states::AppState;

const PLANE_ICON_BASE_ANGLE: f32 = std::f32::consts::FRAC_PI_4;
const NORMAL_MOVE_SEGMENT_DURATION: f32 = 0.13;
const FAST_MOVE_SEGMENT_DURATION: f32 = 0.045;
const NORMAL_HANGAR_RETURN_SEGMENT_DURATION: f32 = 0.18;
const FAST_HANGAR_RETURN_SEGMENT_DURATION: f32 = 0.065;
const HANGAR_RETURN_ARC_OFFSET: f32 = 54.0;
const HANGAR_RETURN_Z_LIFT: f32 = 8.0;

/// 动画插件入口：把规则层的瞬时位置变化转成短视觉插值。
pub struct AnimationPlugin;

impl Plugin for AnimationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                initialize_piece_animation_state,
                capture_piece_motion,
                animate_piece_motion,
            )
                .chain()
                .run_if(in_state(AppState::InGame)),
        );
    }
}

#[derive(Component)]
struct PieceAnimationState {
    logical_translation: Vec3,
    logical_piece: PieceAnimationSnapshot,
}

#[derive(Component)]
struct PieceMoveAnimation {
    waypoints: Vec<Vec3>,
    segment_index: usize,
    segment_elapsed: f32,
    segment_duration: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PieceAnimationSnapshot {
    owner_player_id: u8,
    status: PieceStatus,
    progress: u8,
    motion_serial: u32,
}

type NewPieceAnimationQuery<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static PieceState, &'static Transform),
    (With<PieceId>, Added<PieceId>),
>;
type ChangedPieceAnimationQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PieceState,
        &'static mut Transform,
        &'static mut PieceAnimationState,
    ),
    (
        With<PieceId>,
        Changed<Transform>,
        Without<PieceMoveAnimation>,
    ),
>;

fn initialize_piece_animation_state(mut commands: Commands, query: NewPieceAnimationQuery) {
    for (entity, piece_state, transform) in &query {
        commands.entity(entity).insert(PieceAnimationState {
            logical_translation: transform.translation,
            logical_piece: PieceAnimationSnapshot::from_piece_state(*piece_state),
        });
    }
}

fn capture_piece_motion(
    mut commands: Commands,
    match_config: Res<MatchConfig>,
    board_layout: Res<BoardLayout>,
    player_roster: Res<PlayerRoster>,
    mut query: ChangedPieceAnimationQuery,
) {
    for (entity, piece_state, mut transform, mut animation_state) in &mut query {
        let from = animation_state.logical_translation;
        let to = transform.translation;
        let previous_piece = animation_state.logical_piece;
        let current_piece = PieceAnimationSnapshot::from_piece_state(*piece_state);
        if is_stale_transform_change(previous_piece, current_piece, from, to) {
            animation_state.logical_translation = to;
            animation_state.logical_piece = current_piece;
            continue;
        }

        let waypoints = build_motion_waypoints(
            previous_piece,
            current_piece,
            from,
            to,
            &board_layout,
            &player_roster,
        );
        if waypoints.len() < 2 {
            animation_state.logical_translation = to;
            animation_state.logical_piece = current_piece;
            continue;
        }

        animation_state.logical_translation = to;
        animation_state.logical_piece = current_piece;
        transform.translation = from;
        if let Some(rotation) = first_waypoint_rotation(&waypoints) {
            transform.rotation = rotation;
        }
        commands.entity(entity).insert(PieceMoveAnimation {
            waypoints,
            segment_index: 0,
            segment_elapsed: 0.0,
            segment_duration: animation_segment_duration(
                previous_piece,
                current_piece,
                match_config.fast_mode,
            ),
        });
    }
}

fn animate_piece_motion(
    time: Res<Time>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut Transform, &mut PieceMoveAnimation)>,
) {
    for (entity, mut transform, mut animation) in &mut query {
        if animation.waypoints.len() < 2 {
            commands.entity(entity).remove::<PieceMoveAnimation>();
            continue;
        }

        animation.segment_elapsed += time.delta_secs();
        while animation.segment_elapsed >= animation.segment_duration
            && animation.segment_index + 1 < animation.waypoints.len() - 1
        {
            animation.segment_elapsed -= animation.segment_duration;
            animation.segment_index += 1;
        }

        if animation.segment_elapsed >= animation.segment_duration {
            if let Some(rotation) =
                waypoint_segment_rotation(&animation.waypoints, animation.segment_index)
            {
                transform.rotation = rotation;
            }
            transform.translation = *animation.waypoints.last().unwrap_or(&transform.translation);
            commands.entity(entity).remove::<PieceMoveAnimation>();
            continue;
        }

        let from = animation.waypoints[animation.segment_index];
        let to = animation.waypoints[animation.segment_index + 1];
        let fraction = (animation.segment_elapsed / animation.segment_duration).clamp(0.0, 1.0);
        if let Some(rotation) = rotation_for_direction(to - from) {
            transform.rotation = rotation;
        }
        transform.translation = from.lerp(to, ease_out_cubic(fraction));
    }
}

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

fn movement_segment_duration(fast_mode: bool) -> f32 {
    if fast_mode {
        FAST_MOVE_SEGMENT_DURATION
    } else {
        NORMAL_MOVE_SEGMENT_DURATION
    }
}

fn hangar_return_segment_duration(fast_mode: bool) -> f32 {
    if fast_mode {
        FAST_HANGAR_RETURN_SEGMENT_DURATION
    } else {
        NORMAL_HANGAR_RETURN_SEGMENT_DURATION
    }
}

fn animation_segment_duration(
    previous_piece: PieceAnimationSnapshot,
    current_piece: PieceAnimationSnapshot,
    fast_mode: bool,
) -> f32 {
    if is_returning_to_hangar(previous_piece, current_piece) {
        hangar_return_segment_duration(fast_mode)
    } else {
        movement_segment_duration(fast_mode)
    }
}

fn is_stale_transform_change(
    previous_piece: PieceAnimationSnapshot,
    current_piece: PieceAnimationSnapshot,
    from: Vec3,
    to: Vec3,
) -> bool {
    previous_piece == current_piece && from.distance_squared(to) < 0.25
}

impl PieceAnimationSnapshot {
    fn from_piece_state(piece_state: PieceState) -> Self {
        Self {
            owner_player_id: piece_state.owner_player_id,
            status: piece_state.status,
            progress: piece_state.progress,
            motion_serial: piece_state.motion_serial,
        }
    }
}

fn build_motion_waypoints(
    previous_piece: PieceAnimationSnapshot,
    current_piece: PieceAnimationSnapshot,
    from: Vec3,
    to: Vec3,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
) -> Vec<Vec3> {
    let mut waypoints = vec![from];
    if previous_piece.owner_player_id != current_piece.owner_player_id {
        waypoints.push(to);
        return waypoints;
    }

    if is_returning_to_hangar(previous_piece, current_piece) {
        append_hangar_return_waypoints(&mut waypoints, from, to);
        return waypoints;
    }

    append_board_path_waypoints(
        &mut waypoints,
        previous_piece,
        current_piece,
        to.z,
        board_layout,
        player_roster,
    );
    push_distinct_waypoint(&mut waypoints, to);
    waypoints
}

fn is_returning_to_hangar(
    previous_piece: PieceAnimationSnapshot,
    current_piece: PieceAnimationSnapshot,
) -> bool {
    previous_piece.owner_player_id == current_piece.owner_player_id
        && previous_piece.status != PieceStatus::InHangar
        && current_piece.status == PieceStatus::InHangar
}

fn append_hangar_return_waypoints(waypoints: &mut Vec<Vec3>, from: Vec3, to: Vec3) {
    let delta = to - from;
    let delta_2d = delta.truncate();
    if delta_2d.length_squared() >= 0.25 {
        let direction = delta_2d.normalize();
        let mut perpendicular = Vec2::new(-direction.y, direction.x);
        if perpendicular.dot(from.truncate()) < 0.0 {
            perpendicular = -perpendicular;
        }
        let offset = HANGAR_RETURN_ARC_OFFSET.min(delta_2d.length() * 0.35);
        let midpoint = from.lerp(to, 0.45)
            + Vec3::new(
                perpendicular.x * offset,
                perpendicular.y * offset,
                HANGAR_RETURN_Z_LIFT,
            );
        push_distinct_waypoint(waypoints, midpoint);
    }
    push_distinct_waypoint(waypoints, to);
}

fn append_board_path_waypoints(
    waypoints: &mut Vec<Vec3>,
    previous_piece: PieceAnimationSnapshot,
    current_piece: PieceAnimationSnapshot,
    z: f32,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
) {
    let Some(movement_steps) = movement_steps_between_progresses(
        current_piece.owner_player_id,
        previous_piece.status,
        previous_piece.progress,
        current_piece.status,
        current_piece.progress,
        board_layout,
        player_roster,
    ) else {
        return;
    };

    for step in movement_steps {
        let status = if step.progress == FINISH_DISTANCE {
            PieceStatus::Finished
        } else {
            PieceStatus::Active
        };
        if let Some(pos) = world_position_for_piece(
            current_piece.owner_player_id,
            step.progress,
            status,
            board_layout,
            player_roster,
        ) {
            push_distinct_waypoint(waypoints, pos.extend(z));
        }
    }
}

fn push_distinct_waypoint(waypoints: &mut Vec<Vec3>, waypoint: Vec3) {
    if waypoints
        .last()
        .is_none_or(|last| last.distance_squared(waypoint) >= 0.25)
    {
        waypoints.push(waypoint);
    }
}

fn first_waypoint_rotation(waypoints: &[Vec3]) -> Option<Quat> {
    waypoint_segment_rotation(waypoints, 0)
}

fn waypoint_segment_rotation(waypoints: &[Vec3], segment_index: usize) -> Option<Quat> {
    let from = waypoints.get(segment_index)?;
    let to = waypoints.get(segment_index + 1)?;
    rotation_for_direction(*to - *from)
}

fn rotation_for_direction(direction: Vec3) -> Option<Quat> {
    let direction = direction.truncate();
    if direction.length_squared() < 0.25 {
        return None;
    }

    let target_angle = direction.y.atan2(direction.x);
    Some(Quat::from_rotation_z(target_angle - PLANE_ICON_BASE_ANGLE))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::game_mode::GameMode;
    use crate::domain::player::PlayerControl;
    use crate::domain::rules::LaunchRule;
    use crate::gameplay::ai::AiDifficulty;
    use crate::gameplay::match_flow::{MatchSetup, PlayerRoster, PlayerSeat, build_match_rosters};
    use crate::gameplay::turn_flow::HOME_ENTRY_PROGRESS;

    fn test_roster() -> (BoardLayout, PlayerRoster) {
        let setup = MatchSetup {
            mode: GameMode::TwoVsTwo,
            ai_difficulty: AiDifficulty::Normal,
            fast_mode: false,
            launch_rule: LaunchRule::SixOnly,
            player_seats: [
                PlayerSeat::Blue,
                PlayerSeat::Red,
                PlayerSeat::Green,
                PlayerSeat::Yellow,
            ],
            pieces_per_player: 2,
            player_controls: [
                PlayerControl::Human,
                PlayerControl::Ai,
                PlayerControl::Human,
                PlayerControl::Ai,
            ],
        };
        let (players, _) = build_match_rosters(&setup);
        (BoardLayout::default(), PlayerRoster::from_players(players))
    }

    #[test]
    fn active_move_waypoints_follow_each_route_tile() {
        let (board_layout, player_roster) = test_roster();
        let from =
            world_position_for_piece(1, 0, PieceStatus::Active, &board_layout, &player_roster)
                .unwrap()
                .extend(1.0);
        let to = world_position_for_piece(1, 3, PieceStatus::Active, &board_layout, &player_roster)
            .unwrap()
            .extend(1.0);

        let waypoints = build_motion_waypoints(
            PieceAnimationSnapshot {
                owner_player_id: 1,
                status: PieceStatus::Active,
                progress: 0,
                motion_serial: 0,
            },
            PieceAnimationSnapshot {
                owner_player_id: 1,
                status: PieceStatus::Active,
                progress: 3,
                motion_serial: 0,
            },
            from,
            to,
            &board_layout,
            &player_roster,
        );

        assert_eq!(waypoints.len(), 4);
        assert_eq!(waypoints[0], from);
        assert_eq!(
            waypoints[1],
            world_position_for_piece(1, 1, PieceStatus::Active, &board_layout, &player_roster)
                .unwrap()
                .extend(1.0)
        );
        assert_eq!(waypoints[3], to);
    }

    #[test]
    fn movement_animation_segments_are_slow_enough_to_read() {
        assert_eq!(movement_segment_duration(false), 0.13);
        assert_eq!(movement_segment_duration(true), 0.045);
        assert!(movement_segment_duration(false) > 0.10);
        assert!(movement_segment_duration(true) < movement_segment_duration(false));
        assert!(hangar_return_segment_duration(false) > movement_segment_duration(false));
        assert!(hangar_return_segment_duration(true) > movement_segment_duration(true));
    }

    #[test]
    fn return_to_hangar_waypoints_arc_from_board_to_hangar() {
        let (board_layout, player_roster) = test_roster();
        let from =
            world_position_for_piece(2, 8, PieceStatus::Active, &board_layout, &player_roster)
                .unwrap()
                .extend(1.0);
        let to = Vec3::new(320.0, 280.0, 1.0);

        let waypoints = build_motion_waypoints(
            PieceAnimationSnapshot {
                owner_player_id: 2,
                status: PieceStatus::Active,
                progress: 8,
                motion_serial: 0,
            },
            PieceAnimationSnapshot {
                owner_player_id: 2,
                status: PieceStatus::InHangar,
                progress: 0,
                motion_serial: 0,
            },
            from,
            to,
            &board_layout,
            &player_roster,
        );

        assert_eq!(waypoints.len(), 3);
        assert_eq!(waypoints[0], from);
        assert_eq!(waypoints[2], to);
        assert!(waypoints[1].z > from.z);
        let direct_midpoint = from.lerp(to, 0.45);
        assert!(waypoints[1].truncate().distance(direct_midpoint.truncate()) > 1.0);
    }

    #[test]
    fn return_to_hangar_uses_knockback_duration() {
        let previous_piece = PieceAnimationSnapshot {
            owner_player_id: 2,
            status: PieceStatus::Active,
            progress: 8,
            motion_serial: 0,
        };
        let current_piece = PieceAnimationSnapshot {
            status: PieceStatus::InHangar,
            progress: 0,
            ..previous_piece
        };

        assert_eq!(
            animation_segment_duration(previous_piece, current_piece, false),
            hangar_return_segment_duration(false)
        );
        assert_eq!(
            animation_segment_duration(previous_piece, current_piece, true),
            hangar_return_segment_duration(true)
        );
    }

    #[test]
    fn launch_to_route_waypoints_include_first_main_tile() {
        let (board_layout, player_roster) = test_roster();
        let from =
            world_position_for_piece(1, 0, PieceStatus::AtLaunch, &board_layout, &player_roster)
                .unwrap()
                .extend(1.0);
        let to = world_position_for_piece(1, 2, PieceStatus::Active, &board_layout, &player_roster)
            .unwrap()
            .extend(1.0);

        let waypoints = build_motion_waypoints(
            PieceAnimationSnapshot {
                owner_player_id: 1,
                status: PieceStatus::AtLaunch,
                progress: 0,
                motion_serial: 0,
            },
            PieceAnimationSnapshot {
                owner_player_id: 1,
                status: PieceStatus::Active,
                progress: 2,
                motion_serial: 0,
            },
            from,
            to,
            &board_layout,
            &player_roster,
        );

        assert_eq!(waypoints.len(), 4);
        assert_eq!(
            waypoints[1],
            world_position_for_piece(1, 0, PieceStatus::Active, &board_layout, &player_roster)
                .unwrap()
                .extend(1.0)
        );
        assert_eq!(waypoints[3], to);
    }

    #[test]
    fn home_lane_entry_waypoint_turns_into_branch_without_extra_route_loop() {
        let (board_layout, player_roster) = test_roster();
        let from = world_position_for_piece(
            1,
            HOME_ENTRY_PROGRESS - 1,
            PieceStatus::Active,
            &board_layout,
            &player_roster,
        )
        .unwrap()
        .extend(1.0);
        let to = world_position_for_piece(
            1,
            HOME_ENTRY_PROGRESS,
            PieceStatus::Active,
            &board_layout,
            &player_roster,
        )
        .unwrap()
        .extend(1.0);

        let waypoints = build_motion_waypoints(
            PieceAnimationSnapshot {
                owner_player_id: 1,
                status: PieceStatus::Active,
                progress: HOME_ENTRY_PROGRESS - 1,
                motion_serial: 0,
            },
            PieceAnimationSnapshot {
                owner_player_id: 1,
                status: PieceStatus::Active,
                progress: HOME_ENTRY_PROGRESS,
                motion_serial: 0,
            },
            from,
            to,
            &board_layout,
            &player_roster,
        );

        assert_eq!(waypoints, vec![from, to]);
        assert!(
            !waypoints.contains(
                &board_layout
                    .world_pos_for_route_index(37)
                    .unwrap()
                    .extend(1.0)
            )
        );
    }

    #[test]
    fn shortcut_waypoints_fly_directly_between_dashed_nodes() {
        let (board_layout, player_roster) = test_roster();
        let start_progress = 15;
        let source_progress = 16;
        let target_progress = 27;
        let from = world_position_for_piece(
            1,
            start_progress,
            PieceStatus::Active,
            &board_layout,
            &player_roster,
        )
        .unwrap()
        .extend(1.0);
        let source = world_position_for_piece(
            1,
            source_progress,
            PieceStatus::Active,
            &board_layout,
            &player_roster,
        )
        .unwrap()
        .extend(1.0);
        let to = world_position_for_piece(
            1,
            target_progress,
            PieceStatus::Active,
            &board_layout,
            &player_roster,
        )
        .unwrap()
        .extend(1.0);
        let route_after_source = world_position_for_piece(
            1,
            source_progress + 1,
            PieceStatus::Active,
            &board_layout,
            &player_roster,
        )
        .unwrap()
        .extend(1.0);

        let waypoints = build_motion_waypoints(
            PieceAnimationSnapshot {
                owner_player_id: 1,
                status: PieceStatus::Active,
                progress: start_progress,
                motion_serial: 0,
            },
            PieceAnimationSnapshot {
                owner_player_id: 1,
                status: PieceStatus::Active,
                progress: target_progress,
                motion_serial: 0,
            },
            from,
            to,
            &board_layout,
            &player_roster,
        );

        assert_eq!(waypoints, vec![from, source, to]);
        assert!(!waypoints.contains(&route_after_source));
    }

    #[test]
    fn home_lane_overshoot_waypoints_bounce_from_goal() {
        let (board_layout, player_roster) = test_roster();
        let from = world_position_for_piece(
            1,
            FINISH_DISTANCE - 1,
            PieceStatus::Active,
            &board_layout,
            &player_roster,
        )
        .unwrap()
        .extend(1.0);
        let goal = world_position_for_piece(
            1,
            FINISH_DISTANCE,
            PieceStatus::Finished,
            &board_layout,
            &player_roster,
        )
        .unwrap()
        .extend(1.0);

        let waypoints = build_motion_waypoints(
            PieceAnimationSnapshot {
                owner_player_id: 1,
                status: PieceStatus::Active,
                progress: FINISH_DISTANCE - 1,
                motion_serial: 0,
            },
            PieceAnimationSnapshot {
                owner_player_id: 1,
                status: PieceStatus::Active,
                progress: FINISH_DISTANCE - 1,
                motion_serial: 0,
            },
            from,
            from,
            &board_layout,
            &player_roster,
        );

        assert_eq!(waypoints, vec![from, goal, from]);
    }

    #[test]
    fn stale_transform_change_is_skipped_after_stationary_bounce_animation_finishes() {
        let snapshot = PieceAnimationSnapshot {
            owner_player_id: 1,
            status: PieceStatus::Active,
            progress: FINISH_DISTANCE - 1,
            motion_serial: 7,
        };
        let translation = Vec3::new(12.0, 24.0, 1.0);

        assert!(is_stale_transform_change(
            snapshot,
            snapshot,
            translation,
            translation
        ));
        assert!(!is_stale_transform_change(
            snapshot,
            PieceAnimationSnapshot {
                motion_serial: 8,
                ..snapshot
            },
            translation,
            translation
        ));
    }
}
