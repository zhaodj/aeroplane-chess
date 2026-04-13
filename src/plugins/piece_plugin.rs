use bevy::prelude::*;

use crate::constants::BOARD_Z_LAYER;
use crate::domain::piece::{PieceState, PieceStatus};
use crate::domain::player::PlayerControl;
use crate::gameplay::match_flow::PlayerRoster;
use crate::gameplay::turn_flow::{TurnInputState, TurnState};
use crate::plugins::skill_plugin::SkillTargetState;
use crate::states::AppState;
use crate::states::GamePhase;

pub struct PiecePlugin;

impl Plugin for PiecePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_pieces)
            .add_systems(
                Update,
                update_piece_highlight.run_if(in_state(AppState::InGame)),
            )
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
                Name::new(format!("Piece_P{}_{}", player.state.player_id, piece_id)),
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
    skill_target_state: Res<SkillTargetState>,
    player_roster: Res<PlayerRoster>,
    turn_state: Res<TurnState>,
    game_phase: Res<State<GamePhase>>,
    mut query: Query<
        (
            &PieceId,
            &PieceState,
            &PieceBaseColor,
            &mut Sprite,
            &mut Transform,
        ),
        With<PieceEntity>,
    >,
) {
    let selectable = matches!(game_phase.get(), GamePhase::AwaitPieceSelect);
    let skill_selectable = matches!(game_phase.get(), GamePhase::ResolveSkillEffect);
    let current_player_control = player_roster
        .players
        .iter()
        .find(|player| player.state.player_id == turn_state.current_player)
        .map(|player| player.state.control);

    for (piece_id, piece_state, base_color, mut sprite, mut transform) in &mut query {
        if selectable && input_state.candidate_piece_ids().contains(&piece_id.0) {
            sprite.color = base_color.0.mix(&Color::WHITE, 0.35);
            transform.scale = Vec3::splat(1.18);
        } else if skill_selectable
            && skill_target_state
                .candidate_piece_ids()
                .contains(&piece_id.0)
        {
            sprite.color = base_color.0.mix(&Color::srgb(1.0, 0.88, 0.60), 0.45);
            transform.scale = Vec3::splat(1.18);
        } else if matches!(current_player_control, Some(PlayerControl::Human))
            && input_state.candidate_piece_ids().is_empty()
            && skill_target_state.candidate_piece_ids().is_empty()
            && piece_state.owner_player_id == turn_state.current_player
        {
            sprite.color = base_color.0.mix(&Color::WHITE, 0.18);
            transform.scale = Vec3::splat(1.08);
        } else {
            sprite.color = base_color.0;
            transform.scale = Vec3::ONE;
        }
    }
}
