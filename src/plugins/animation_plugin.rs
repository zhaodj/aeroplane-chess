use bevy::prelude::*;

use crate::domain::piece::{PieceState, PieceStatus, SWAP_MOTION_SERIAL_DELTA};
use crate::gameplay::match_flow::{BoardLayout, MatchConfig, PlayerRoster};
use crate::gameplay::turn_flow::{
    FINISH_DISTANCE, TurnState, movement_steps_between_progresses, world_position_for_piece,
};
use crate::plugins::effects_plugin::{
    AdvanceTwoPause, PieceMotionEffect, PieceMotionEffects, VisualEffectQueue,
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
const SWAP_ARC_RADIUS_LIFT: f32 = 52.0;
const SWAP_ARC_Z_LIFT: f32 = 14.0;

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
pub(crate) struct PieceMoveAnimation {
    waypoints: Vec<Vec3>,
    segment_index: usize,
    segment_elapsed: f32,
    segment_duration: f32,
    start_delay: f32,
    pause_cue: Option<PieceMovePauseCue>,
}

#[derive(Clone, Debug)]
struct PieceMovePauseCue {
    waypoint_index: usize,
    duration: f32,
    remaining: f32,
    started: bool,
    text: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct CollisionArrival {
    target: Vec2,
    delay_secs: f32,
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
        &'static PieceId,
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
    mut motion_effects: ResMut<PieceMotionEffects>,
    mut query: ChangedPieceAnimationQuery,
) {
    let collision_arrivals = collect_collision_arrivals(
        match_config.fast_mode,
        &board_layout,
        &player_roster,
        &motion_effects,
        &mut query,
    );

    for (entity, piece_id, piece_state, mut transform, mut animation_state) in &mut query {
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

        let motion_effect = motion_effects.take_for_piece(piece_id.0);
        let start_delay = motion_effect.start_delay_secs.max(collision_return_delay(
            previous_piece,
            current_piece,
            from,
            &collision_arrivals,
        ));
        let pause_cue = motion_effect.advance_two.and_then(|cue| {
            advance_two_pause_cue(
                cue,
                current_piece.owner_player_id,
                to.z,
                &waypoints,
                &board_layout,
                &player_roster,
            )
        });
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
            start_delay,
            pause_cue,
        });
    }
}

fn collect_collision_arrivals(
    fast_mode: bool,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
    motion_effects: &PieceMotionEffects,
    query: &mut ChangedPieceAnimationQuery<'_, '_>,
) -> Vec<CollisionArrival> {
    let mut arrivals = Vec::new();
    for (_, piece_id, piece_state, transform, animation_state) in query.iter_mut() {
        let from = animation_state.logical_translation;
        let to = transform.translation;
        let previous_piece = animation_state.logical_piece;
        let current_piece = PieceAnimationSnapshot::from_piece_state(*piece_state);
        if is_stale_transform_change(previous_piece, current_piece, from, to)
            || is_returning_to_hangar(previous_piece, current_piece)
            || previous_piece.owner_player_id != current_piece.owner_player_id
            || !matches!(
                current_piece.status,
                PieceStatus::Active | PieceStatus::Finished
            )
        {
            continue;
        }

        let waypoints = build_motion_waypoints(
            previous_piece,
            current_piece,
            from,
            to,
            board_layout,
            player_roster,
        );
        if waypoints.len() < 2 {
            continue;
        }

        let motion_effect = motion_effects.peek_for_piece(piece_id.0);
        arrivals.push(CollisionArrival {
            target: to.truncate(),
            delay_secs: arrival_animation_duration(
                previous_piece,
                current_piece,
                fast_mode,
                motion_effect,
                &waypoints,
                board_layout,
                player_roster,
            ),
        });
    }
    arrivals
}

fn arrival_animation_duration(
    previous_piece: PieceAnimationSnapshot,
    current_piece: PieceAnimationSnapshot,
    fast_mode: bool,
    motion_effect: PieceMotionEffect,
    waypoints: &[Vec3],
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
) -> f32 {
    let base_duration = waypoints.len().saturating_sub(1) as f32
        * animation_segment_duration(previous_piece, current_piece, fast_mode);
    let pause_duration = motion_effect
        .advance_two
        .and_then(|cue| {
            advance_two_pause_cue(
                cue,
                current_piece.owner_player_id,
                waypoints
                    .last()
                    .map(|waypoint| waypoint.z)
                    .unwrap_or_default(),
                waypoints,
                board_layout,
                player_roster,
            )
        })
        .map(|cue| cue.duration)
        .unwrap_or_default();
    base_duration + motion_effect.start_delay_secs + pause_duration
}

fn collision_return_delay(
    previous_piece: PieceAnimationSnapshot,
    current_piece: PieceAnimationSnapshot,
    from: Vec3,
    arrivals: &[CollisionArrival],
) -> f32 {
    if !is_returning_to_hangar(previous_piece, current_piece) {
        return 0.0;
    }

    arrivals
        .iter()
        .filter(|arrival| arrival.target.distance_squared(from.truncate()) < 0.25)
        .map(|arrival| arrival.delay_secs)
        .fold(0.0, f32::max)
}

fn animate_piece_motion(
    time: Res<Time>,
    mut commands: Commands,
    mut turn_state: ResMut<TurnState>,
    mut effect_queue: ResMut<VisualEffectQueue>,
    mut query: Query<(Entity, &mut Transform, &mut PieceMoveAnimation)>,
) {
    let mut saw_animation = false;
    let mut any_animation_continues = false;
    for (entity, mut transform, mut animation) in &mut query {
        saw_animation = true;
        if animation.waypoints.len() < 2 {
            commands.entity(entity).remove::<PieceMoveAnimation>();
            continue;
        }

        if consume_start_delay(&mut animation, time.delta_secs()) {
            any_animation_continues = true;
            continue;
        }

        if consume_pause_delay(&mut animation, time.delta_secs()) {
            any_animation_continues = true;
            continue;
        }

        if animation.segment_index + 1 >= animation.waypoints.len() {
            transform.translation = *animation.waypoints.last().unwrap_or(&transform.translation);
            commands.entity(entity).remove::<PieceMoveAnimation>();
            continue;
        }

        animation.segment_elapsed += time.delta_secs();
        let mut paused = false;
        let mut finished = false;
        while animation.segment_elapsed >= animation.segment_duration {
            let next_index = animation.segment_index + 1;
            transform.translation = animation.waypoints[next_index];
            if let Some(rotation) = waypoint_segment_rotation(&animation.waypoints, next_index - 1)
            {
                transform.rotation = rotation;
            }
            animation.segment_elapsed -= animation.segment_duration;
            animation.segment_index = next_index;

            if start_pause_cue(&mut animation, &mut effect_queue) {
                paused = true;
                break;
            }

            if animation.segment_index + 1 >= animation.waypoints.len() {
                commands.entity(entity).remove::<PieceMoveAnimation>();
                finished = true;
                break;
            }
        }

        if paused {
            any_animation_continues = true;
            continue;
        }
        if finished {
            continue;
        }

        any_animation_continues = true;
        let from = animation.waypoints[animation.segment_index];
        let to = animation.waypoints[animation.segment_index + 1];
        let fraction = (animation.segment_elapsed / animation.segment_duration).clamp(0.0, 1.0);
        if let Some(rotation) = rotation_for_direction(to - from) {
            transform.rotation = rotation;
        }
        transform.translation = from.lerp(to, ease_out_cubic(fraction));
    }

    sync_roll_display_hold_after_animation(&mut turn_state, saw_animation, any_animation_continues);
}

fn consume_start_delay(animation: &mut PieceMoveAnimation, delta_secs: f32) -> bool {
    if animation.start_delay <= 0.0 {
        return false;
    }

    animation.start_delay = (animation.start_delay - delta_secs).max(0.0);
    true
}

fn consume_pause_delay(animation: &mut PieceMoveAnimation, delta_secs: f32) -> bool {
    let Some(cue) = animation.pause_cue.as_mut() else {
        return false;
    };
    if !cue.started || cue.remaining <= 0.0 {
        return false;
    }

    cue.remaining = (cue.remaining - delta_secs).max(0.0);
    true
}

fn start_pause_cue(
    animation: &mut PieceMoveAnimation,
    effect_queue: &mut VisualEffectQueue,
) -> bool {
    let Some(cue) = animation.pause_cue.as_mut() else {
        return false;
    };
    if cue.started || cue.waypoint_index != animation.segment_index {
        return false;
    }

    cue.started = true;
    cue.remaining = cue.duration;
    animation.segment_elapsed = 0.0;
    if let Some(waypoint) = animation.waypoints.get(cue.waypoint_index) {
        effect_queue.floating_text(waypoint.truncate(), cue.text);
    }
    true
}

fn sync_roll_display_hold_after_animation(
    turn_state: &mut TurnState,
    saw_animation: bool,
    any_animation_continues: bool,
) {
    if saw_animation {
        turn_state.roll_display_animation_started = true;
    }
    if turn_state.hold_last_roll_display
        && turn_state.roll_display_animation_started
        && !any_animation_continues
    {
        turn_state.hold_last_roll_display = false;
        turn_state.roll_display_animation_started = false;
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

    if is_swap_arc_motion(previous_piece, current_piece) {
        append_clockwise_swap_arc_waypoints(&mut waypoints, from, to);
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

fn advance_two_pause_cue(
    cue: AdvanceTwoPause,
    owner_player_id: u8,
    z: f32,
    waypoints: &[Vec3],
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
) -> Option<PieceMovePauseCue> {
    let event_position = world_position_for_piece(
        owner_player_id,
        cue.event_progress,
        PieceStatus::Active,
        board_layout,
        player_roster,
    )?
    .extend(z);
    let waypoint_index = waypoints
        .iter()
        .position(|waypoint| waypoint.distance_squared(event_position) < 0.25)?;
    (waypoint_index + 1 < waypoints.len()).then_some(PieceMovePauseCue {
        waypoint_index,
        duration: cue.pause_secs,
        remaining: 0.0,
        started: false,
        text: "+2",
    })
}

fn is_returning_to_hangar(
    previous_piece: PieceAnimationSnapshot,
    current_piece: PieceAnimationSnapshot,
) -> bool {
    previous_piece.owner_player_id == current_piece.owner_player_id
        && previous_piece.status != PieceStatus::InHangar
        && current_piece.status == PieceStatus::InHangar
}

fn is_swap_arc_motion(
    previous_piece: PieceAnimationSnapshot,
    current_piece: PieceAnimationSnapshot,
) -> bool {
    previous_piece.owner_player_id == current_piece.owner_player_id
        && previous_piece.status == PieceStatus::Active
        && current_piece.status == PieceStatus::Active
        && previous_piece.progress != current_piece.progress
        && current_piece.motion_serial
            == previous_piece
                .motion_serial
                .wrapping_add(SWAP_MOTION_SERIAL_DELTA)
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

fn append_clockwise_swap_arc_waypoints(waypoints: &mut Vec<Vec3>, from: Vec3, to: Vec3) {
    let from_2d = from.truncate();
    let to_2d = to.truncate();
    if from_2d.distance_squared(to_2d) < 0.25 {
        push_distinct_waypoint(waypoints, to);
        return;
    }

    let start_angle = from_2d.y.atan2(from_2d.x);
    let mut end_angle = to_2d.y.atan2(to_2d.x);
    while end_angle >= start_angle {
        end_angle -= std::f32::consts::TAU;
    }

    let radius = from_2d.length().max(to_2d.length()) + SWAP_ARC_RADIUS_LIFT;
    for ratio in [0.33, 0.66] {
        let angle = start_angle + (end_angle - start_angle) * ratio;
        let point = Vec2::new(angle.cos(), angle.sin()) * radius;
        let z = from.z + (to.z - from.z) * ratio + SWAP_ARC_Z_LIFT;
        push_distinct_waypoint(waypoints, point.extend(z));
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
    fn collision_return_delay_waits_for_attacker_arrival() {
        let previous_piece = PieceAnimationSnapshot {
            owner_player_id: 2,
            status: PieceStatus::Active,
            progress: 12,
            motion_serial: 0,
        };
        let current_piece = PieceAnimationSnapshot {
            status: PieceStatus::InHangar,
            progress: 0,
            ..previous_piece
        };
        let defender_position = Vec3::new(80.0, 40.0, 1.0);
        let arrivals = [CollisionArrival {
            target: defender_position.truncate(),
            delay_secs: 0.52,
        }];

        assert_eq!(
            collision_return_delay(previous_piece, current_piece, defender_position, &arrivals),
            0.52
        );
    }

    #[test]
    fn collision_return_delay_ignores_unrelated_returns() {
        let previous_piece = PieceAnimationSnapshot {
            owner_player_id: 2,
            status: PieceStatus::Active,
            progress: 12,
            motion_serial: 0,
        };
        let current_piece = PieceAnimationSnapshot {
            status: PieceStatus::InHangar,
            progress: 0,
            ..previous_piece
        };
        let arrivals = [CollisionArrival {
            target: Vec2::new(-80.0, 40.0),
            delay_secs: 0.52,
        }];

        assert_eq!(
            collision_return_delay(
                previous_piece,
                current_piece,
                Vec3::new(80.0, 40.0, 1.0),
                &arrivals
            ),
            0.0
        );
    }

    #[test]
    fn collision_arrival_duration_includes_event_pause() {
        let (board_layout, player_roster) = test_roster();
        let previous_piece = PieceAnimationSnapshot {
            owner_player_id: 1,
            status: PieceStatus::Active,
            progress: 4,
            motion_serial: 0,
        };
        let current_piece = PieceAnimationSnapshot {
            progress: 8,
            ..previous_piece
        };
        let from =
            world_position_for_piece(1, 4, PieceStatus::Active, &board_layout, &player_roster)
                .unwrap()
                .extend(1.0);
        let to = world_position_for_piece(1, 8, PieceStatus::Active, &board_layout, &player_roster)
            .unwrap()
            .extend(1.0);
        let waypoints = build_motion_waypoints(
            previous_piece,
            current_piece,
            from,
            to,
            &board_layout,
            &player_roster,
        );
        let motion_effect = PieceMotionEffect {
            start_delay_secs: 0.20,
            advance_two: Some(AdvanceTwoPause {
                event_progress: 6,
                pause_secs: 0.62,
            }),
        };

        let duration = arrival_animation_duration(
            previous_piece,
            current_piece,
            false,
            motion_effect,
            &waypoints,
            &board_layout,
            &player_roster,
        );
        let expected = waypoints.len().saturating_sub(1) as f32
            * animation_segment_duration(previous_piece, current_piece, false)
            + 0.20
            + 0.62;

        assert!((duration - expected).abs() < 0.001);
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
        let start_progress = 16;
        let source_progress = 17;
        let target_progress = 29;
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
    fn swap_motion_waypoints_follow_clockwise_arc() {
        let (board_layout, player_roster) = test_roster();
        let previous_piece = PieceAnimationSnapshot {
            owner_player_id: 1,
            status: PieceStatus::Active,
            progress: 8,
            motion_serial: 4,
        };
        let current_piece = PieceAnimationSnapshot {
            owner_player_id: 1,
            status: PieceStatus::Active,
            progress: 24,
            motion_serial: previous_piece
                .motion_serial
                .wrapping_add(SWAP_MOTION_SERIAL_DELTA),
        };
        let from = Vec3::new(0.0, 120.0, 1.0);
        let to = Vec3::new(120.0, 0.0, 1.0);

        assert!(is_swap_arc_motion(previous_piece, current_piece));
        let waypoints = build_motion_waypoints(
            previous_piece,
            current_piece,
            from,
            to,
            &board_layout,
            &player_roster,
        );

        assert_eq!(waypoints.len(), 4);
        assert_eq!(waypoints.first().copied(), Some(from));
        assert_eq!(waypoints.last().copied(), Some(to));
        assert!(waypoints[1].z > from.z);
        let start_angle = from.y.atan2(from.x);
        let first_angle = waypoints[1].y.atan2(waypoints[1].x);
        assert!(first_angle < start_angle);
    }

    #[test]
    fn normal_motion_serial_delta_is_not_swap_arc_motion() {
        let previous_piece = PieceAnimationSnapshot {
            owner_player_id: 1,
            status: PieceStatus::Active,
            progress: 8,
            motion_serial: 4,
        };
        let current_piece = PieceAnimationSnapshot {
            owner_player_id: 1,
            status: PieceStatus::Active,
            progress: 12,
            motion_serial: previous_piece.motion_serial.wrapping_add(1),
        };

        assert!(!is_swap_arc_motion(previous_piece, current_piece));
    }

    #[test]
    fn advance_two_motion_cue_pauses_at_event_waypoint() {
        let (board_layout, player_roster) = test_roster();
        let from =
            world_position_for_piece(1, 4, PieceStatus::Active, &board_layout, &player_roster)
                .unwrap()
                .extend(1.0);
        let to = world_position_for_piece(1, 8, PieceStatus::Active, &board_layout, &player_roster)
            .unwrap()
            .extend(1.0);
        let waypoints = build_motion_waypoints(
            PieceAnimationSnapshot {
                owner_player_id: 1,
                status: PieceStatus::Active,
                progress: 4,
                motion_serial: 0,
            },
            PieceAnimationSnapshot {
                owner_player_id: 1,
                status: PieceStatus::Active,
                progress: 8,
                motion_serial: 0,
            },
            from,
            to,
            &board_layout,
            &player_roster,
        );

        let cue = advance_two_pause_cue(
            AdvanceTwoPause {
                event_progress: 6,
                pause_secs: 0.62,
            },
            1,
            1.0,
            &waypoints,
            &board_layout,
            &player_roster,
        )
        .expect("event waypoint should be present");

        assert_eq!(cue.waypoint_index, 2);
        assert_eq!(cue.duration, 0.62);
        assert_eq!(cue.text, "+2");
    }

    #[test]
    fn motion_pause_cue_queues_plus_two_before_continuing() {
        let mut animation = PieceMoveAnimation {
            waypoints: vec![
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(10.0, 0.0, 1.0),
                Vec3::new(20.0, 0.0, 1.0),
            ],
            segment_index: 1,
            segment_elapsed: 0.0,
            segment_duration: 0.13,
            start_delay: 0.0,
            pause_cue: Some(PieceMovePauseCue {
                waypoint_index: 1,
                duration: 0.62,
                remaining: 0.0,
                started: false,
                text: "+2",
            }),
        };
        let mut effect_queue = VisualEffectQueue::default();

        assert!(start_pause_cue(&mut animation, &mut effect_queue));
        let cue = animation.pause_cue.as_ref().unwrap();
        assert!(cue.started);
        assert_eq!(cue.remaining, 0.62);
        assert_eq!(effect_queue.pending_count(), 1);
    }

    #[test]
    fn motion_start_delay_blocks_initial_interpolation() {
        let mut animation = PieceMoveAnimation {
            waypoints: vec![Vec3::ZERO, Vec3::X],
            segment_index: 0,
            segment_elapsed: 0.0,
            segment_duration: 0.13,
            start_delay: 0.5,
            pause_cue: None,
        };

        assert!(consume_start_delay(&mut animation, 0.2));
        assert!((animation.start_delay - 0.3).abs() < 0.001);
        assert!(consume_start_delay(&mut animation, 0.3));
        assert_eq!(animation.start_delay, 0.0);
        assert!(!consume_start_delay(&mut animation, 0.1));
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

    #[test]
    fn roll_display_hold_clears_only_after_animation_was_seen_and_finished() {
        let mut turn_state = TurnState::opening_turn();
        turn_state.hold_last_roll_display = true;

        sync_roll_display_hold_after_animation(&mut turn_state, false, false);
        assert!(turn_state.hold_last_roll_display);
        assert!(!turn_state.roll_display_animation_started);

        sync_roll_display_hold_after_animation(&mut turn_state, true, true);
        assert!(turn_state.hold_last_roll_display);
        assert!(turn_state.roll_display_animation_started);

        sync_roll_display_hold_after_animation(&mut turn_state, true, false);
        assert!(!turn_state.hold_last_roll_display);
        assert!(!turn_state.roll_display_animation_started);
    }
}
