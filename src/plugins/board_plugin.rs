use bevy::prelude::*;

use crate::constants::{BOARD_TILE_SIZE, BOARD_Z_LAYER};
use crate::domain::tile::TileKind;
use crate::gameplay::match_flow::{BoardLayout, PlayerRoster};
use crate::states::AppState;

pub struct BoardPlugin;

impl Plugin for BoardPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_board)
            .add_systems(OnExit(AppState::InGame), cleanup_board);
    }
}

#[derive(Component)]
struct BoardSceneEntity;

fn spawn_board(mut commands: Commands, board_layout: Res<BoardLayout>, player_roster: Res<PlayerRoster>) {
    commands.spawn((
        Sprite::from_color(Color::srgb(0.84, 0.89, 0.96), Vec2::new(760.0, 760.0)),
        Transform::from_xyz(0.0, 0.0, BOARD_Z_LAYER - 1.0),
        Name::new("BoardBackdrop"),
        BoardSceneEntity,
    ));

    for tile in &board_layout.tiles {
        commands.spawn((
            Sprite::from_color(tile_color(tile.kind), Vec2::splat(BOARD_TILE_SIZE)),
            Transform::from_xyz(tile.world_pos.x, tile.world_pos.y, BOARD_Z_LAYER),
            Name::new(tile.id.clone()),
            BoardSceneEntity,
        ));
    }

    for player in &player_roster.players {
        for lane_pos in &player.home_lane_positions {
            commands.spawn((
                Sprite::from_color(
                    player.color.with_alpha(0.35),
                    Vec2::splat(BOARD_TILE_SIZE * 0.82),
                ),
                Transform::from_xyz(lane_pos.x, lane_pos.y, BOARD_Z_LAYER),
                Name::new(format!("HomeLane_P{}", player.state.player_id)),
                BoardSceneEntity,
            ));
        }

        commands.spawn((
            Sprite::from_color(player.color.with_alpha(0.55), Vec2::splat(BOARD_TILE_SIZE * 0.9)),
            Transform::from_xyz(player.goal_position.x, player.goal_position.y, BOARD_Z_LAYER),
            Name::new(format!("Goal_P{}", player.state.player_id)),
            BoardSceneEntity,
        ));
    }
}

fn cleanup_board(mut commands: Commands, query: Query<Entity, With<BoardSceneEntity>>) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
}

fn tile_color(kind: TileKind) -> Color {
    match kind {
        TileKind::Normal => Color::srgb(0.96, 0.97, 0.99),
        TileKind::Jump => Color::srgb(0.40, 0.75, 0.98),
        TileKind::Attack => Color::srgb(0.96, 0.45, 0.36),
        TileKind::Defense => Color::srgb(0.38, 0.76, 0.53),
        TileKind::Event => Color::srgb(0.98, 0.79, 0.33),
        TileKind::Goal => Color::srgb(0.77, 0.60, 0.97),
    }
}
