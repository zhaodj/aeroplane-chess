use bevy::prelude::*;

use crate::gameplay::match_flow::MatchConfig;
use crate::plugins::piece_plugin::PieceId;
use crate::states::AppState;

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
}

#[derive(Component)]
struct PieceMoveAnimation {
    from: Vec3,
    to: Vec3,
    timer: Timer,
}

type NewPieceAnimationQuery<'w, 's> =
    Query<'w, 's, (Entity, &'static Transform), (With<PieceId>, Added<PieceId>)>;
type ChangedPieceAnimationQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
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
    for (entity, transform) in &query {
        commands.entity(entity).insert(PieceAnimationState {
            logical_translation: transform.translation,
        });
    }
}

fn capture_piece_motion(
    mut commands: Commands,
    match_config: Res<MatchConfig>,
    mut query: ChangedPieceAnimationQuery,
) {
    let duration = if match_config.fast_mode { 0.08 } else { 0.24 };
    for (entity, mut transform, mut animation_state) in &mut query {
        let from = animation_state.logical_translation;
        let to = transform.translation;
        if from.distance_squared(to) < 0.25 {
            animation_state.logical_translation = to;
            continue;
        }

        animation_state.logical_translation = to;
        transform.translation = from;
        commands.entity(entity).insert(PieceMoveAnimation {
            from,
            to,
            timer: Timer::from_seconds(duration, TimerMode::Once),
        });
    }
}

fn animate_piece_motion(
    time: Res<Time>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut Transform, &mut PieceMoveAnimation)>,
) {
    for (entity, mut transform, mut animation) in &mut query {
        animation.timer.tick(time.delta());
        let eased = ease_out_cubic(animation.timer.fraction());
        transform.translation = animation.from.lerp(animation.to, eased);

        if animation.timer.is_finished() {
            transform.translation = animation.to;
            commands.entity(entity).remove::<PieceMoveAnimation>();
        }
    }
}

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}
