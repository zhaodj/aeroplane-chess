use bevy::prelude::*;

use crate::constants::{BOARD_TILE_SIZE, BOARD_Z_LAYER};
use crate::domain::tile::TileKind;
use crate::plugins::game_plugin::BoardLayout;
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

fn spawn_board(mut commands: Commands, board_layout: Res<BoardLayout>) {
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
