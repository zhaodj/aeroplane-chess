use bevy::prelude::*;

use crate::constants::BOARD_Z_LAYER;
use crate::domain::piece::{PieceState, PieceStatus};
use crate::plugins::game_plugin::PlayerRoster;
use crate::states::AppState;

pub struct PiecePlugin;

impl Plugin for PiecePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_pieces)
            .add_systems(OnExit(AppState::InGame), cleanup_pieces);
    }
}

#[derive(Component)]
struct PieceEntity;

#[derive(Component)]
pub struct PieceId(pub u8);

#[derive(Component, Clone, Copy)]
pub struct HangarSlot(pub Vec2);

fn spawn_pieces(mut commands: Commands, player_roster: Res<PlayerRoster>) {
    let mut piece_id = 1;

    for player in &player_roster.players {
        for &hangar_slot in &player.hangar_slots {
            commands.spawn((
                Sprite::from_color(player.color, Vec2::splat(34.0)),
                Transform::from_xyz(hangar_slot.x, hangar_slot.y, BOARD_Z_LAYER + 1.0),
                player.state.clone(),
                PieceId(piece_id),
                HangarSlot(hangar_slot),
                PieceState {
                    owner_player_id: player.state.player_id,
                    team_id: player.state.team_id,
                    status: PieceStatus::InHangar,
                    progress: 0,
                    shield: 0,
                },
                Name::new(format!(
                    "Piece_P{}_{}",
                    player.state.player_id,
                    piece_id
                )),
                PieceEntity,
            ));

            piece_id += 1;
        }
    }
}

fn cleanup_pieces(mut commands: Commands, query: Query<Entity, With<PieceEntity>>) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
}
