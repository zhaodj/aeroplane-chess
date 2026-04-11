use bevy::prelude::*;

use crate::constants::BOARD_Z_LAYER;
use crate::domain::piece::{PieceState, PieceStatus};
use crate::plugins::game_plugin::PlayerRoster;
use crate::plugins::turn_plugin::TurnInputState;
use crate::states::GamePhase;
use crate::states::AppState;

pub struct PiecePlugin;

impl Plugin for PiecePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_pieces)
            .add_systems(Update, update_piece_highlight.run_if(in_state(AppState::InGame)))
            .add_systems(OnExit(AppState::InGame), cleanup_pieces);
    }
}

#[derive(Component)]
struct PieceEntity;

#[derive(Component)]
pub struct PieceId(pub u8);

#[derive(Component, Clone, Copy)]
pub struct HangarSlot(pub Vec2);

#[derive(Component, Clone, Copy)]
struct PieceBaseColor(pub Color);

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
                PieceBaseColor(player.color),
                PieceState {
                    owner_player_id: player.state.player_id,
                    team_id: player.state.team_id,
                    status: PieceStatus::InHangar,
                    progress: 0,
                    shield: 0,
                    stack_shield: 0,
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

fn update_piece_highlight(
    input_state: Res<TurnInputState>,
    game_phase: Res<State<GamePhase>>,
    mut query: Query<(&PieceId, &PieceBaseColor, &mut Sprite, &mut Transform), With<PieceEntity>>,
) {
    let selectable = matches!(game_phase.get(), GamePhase::AwaitPieceSelect);

    for (piece_id, base_color, mut sprite, mut transform) in &mut query {
        if selectable && input_state.candidate_piece_ids().contains(&piece_id.0) {
            sprite.color = base_color.0.mix(&Color::WHITE, 0.35);
            transform.scale = Vec3::splat(1.18);
        } else {
            sprite.color = base_color.0;
            transform.scale = Vec3::ONE;
        }
    }
}
