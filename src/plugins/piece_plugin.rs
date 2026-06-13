use bevy::prelude::*;

use crate::constants::BOARD_Z_LAYER;
use crate::domain::piece::{PieceState, PieceStatus};
use crate::domain::player::PlayerControl;
use crate::gameplay::match_flow::PlayerRoster;
use crate::gameplay::turn_flow::{TurnInputState, TurnState};
use crate::plugins::skill_plugin::SkillTargetState;
use crate::states::AppState;
use crate::states::GamePhase;

/// 棋子渲染与高亮插件。
pub struct PiecePlugin;

impl Plugin for PiecePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_pieces)
            .add_systems(
                Update,
                (update_piece_highlight, update_piece_shield_badges)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(OnExit(AppState::InGame), cleanup_pieces);
    }
}

#[derive(Component)]
/// 棋子实体标记，用于清理与查询分组。
struct PieceEntity;

#[derive(Component)]
/// 棋子唯一编号。
pub struct PieceId(pub u8);

#[derive(Component, Clone, Copy)]
/// 机库槽位坐标（棋子回家时复位位置）。
pub struct HangarSlot(pub Vec2);

#[derive(Component, Clone, Copy)]
/// 棋子基础颜色缓存（高亮结束后恢复原色用）。
struct PieceBaseColor(pub Color);

#[derive(Component)]
/// 棋子护盾角标。
struct PieceShieldBadge {
    piece_id: u8,
}

#[derive(Component)]
/// 棋子护盾层数字文本。
struct PieceShieldBadgeText {
    piece_id: u8,
}

const PIECE_SIZE: f32 = 28.0;
const SHIELD_BADGE_SIZE: f32 = 14.0;
const SHIELD_BADGE_OFFSET: Vec2 = Vec2::new(10.0, 10.0);

type ShieldBadgePieceQuery<'w, 's> = Query<
    'w,
    's,
    (&'static PieceId, &'static PieceState, &'static Transform),
    (Without<PieceShieldBadge>, Without<PieceShieldBadgeText>),
>;
type ShieldBadgeQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static PieceShieldBadge,
        &'static mut Sprite,
        &'static mut Transform,
        &'static mut Visibility,
    ),
    (Without<PieceId>, Without<PieceShieldBadgeText>),
>;
type ShieldBadgeTextQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static PieceShieldBadgeText,
        &'static mut Text2d,
        &'static mut Transform,
        &'static mut Visibility,
    ),
    (Without<PieceId>, Without<PieceShieldBadge>),
>;

fn spawn_pieces(mut commands: Commands, player_roster: Res<PlayerRoster>) {
    // 按玩家机库槽位生成所有棋子实体，并初始化为 InHangar 状态。
    let mut piece_id = 1;

    for player in &player_roster.players {
        for &hangar_slot in &player.hangar_slots {
            let current_piece_id = piece_id;
            commands.spawn((
                Sprite::from_color(player.color, Vec2::splat(PIECE_SIZE)),
                Transform::from_xyz(hangar_slot.x, hangar_slot.y, BOARD_Z_LAYER + 1.0),
                player.state.clone(),
                PieceId(current_piece_id),
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
                    player.state.player_id, current_piece_id
                )),
                PieceEntity,
            ));
            commands.spawn((
                Sprite::from_color(
                    Color::srgba(0.20, 0.76, 1.0, 0.88),
                    Vec2::splat(SHIELD_BADGE_SIZE),
                ),
                Transform::from_xyz(
                    hangar_slot.x + SHIELD_BADGE_OFFSET.x,
                    hangar_slot.y + SHIELD_BADGE_OFFSET.y,
                    BOARD_Z_LAYER + 2.0,
                ),
                Visibility::Hidden,
                PieceShieldBadge {
                    piece_id: current_piece_id,
                },
                Name::new(format!("PieceShieldBadge_{}", current_piece_id)),
                PieceEntity,
            ));
            commands.spawn((
                Text2d::new(""),
                TextFont {
                    font_size: 11.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                Transform::from_xyz(
                    hangar_slot.x + SHIELD_BADGE_OFFSET.x - 3.5,
                    hangar_slot.y + SHIELD_BADGE_OFFSET.y - 7.0,
                    BOARD_Z_LAYER + 3.0,
                ),
                Visibility::Hidden,
                PieceShieldBadgeText {
                    piece_id: current_piece_id,
                },
                Name::new(format!("PieceShieldBadgeText_{}", current_piece_id)),
                PieceEntity,
            ));

            piece_id += 1;
        }
    }
}

fn update_piece_shield_badges(
    piece_query: ShieldBadgePieceQuery,
    mut badge_query: ShieldBadgeQuery,
    mut badge_text_query: ShieldBadgeTextQuery,
) {
    let pieces = piece_query
        .iter()
        .map(|(piece_id, piece_state, transform)| {
            (
                piece_id.0,
                piece_state.shield.saturating_add(piece_state.stack_shield),
                transform.translation,
            )
        })
        .collect::<Vec<_>>();

    for (badge, mut sprite, mut transform, mut visibility) in &mut badge_query {
        let Some((_, shield_layers, translation)) = pieces
            .iter()
            .find(|(piece_id, _, _)| *piece_id == badge.piece_id)
        else {
            *visibility = Visibility::Hidden;
            continue;
        };

        if *shield_layers == 0 {
            *visibility = Visibility::Hidden;
            continue;
        }

        sprite.color = if *shield_layers > 1 {
            Color::srgba(0.08, 0.58, 1.0, 0.96)
        } else {
            Color::srgba(0.20, 0.76, 1.0, 0.88)
        };
        transform.translation =
            *translation + Vec3::new(SHIELD_BADGE_OFFSET.x, SHIELD_BADGE_OFFSET.y, 0.0);
        transform.translation.z = BOARD_Z_LAYER + 2.0;
        *visibility = Visibility::Visible;
    }

    for (badge_text, mut text, mut transform, mut visibility) in &mut badge_text_query {
        let Some((_, shield_layers, translation)) = pieces
            .iter()
            .find(|(piece_id, _, _)| *piece_id == badge_text.piece_id)
        else {
            *visibility = Visibility::Hidden;
            continue;
        };

        if *shield_layers == 0 {
            *visibility = Visibility::Hidden;
            continue;
        }

        *text = Text2d::new(shield_layers.to_string());
        transform.translation = *translation
            + Vec3::new(
                SHIELD_BADGE_OFFSET.x - 3.5,
                SHIELD_BADGE_OFFSET.y - 7.0,
                0.0,
            );
        transform.translation.z = BOARD_Z_LAYER + 3.0;
        *visibility = Visibility::Visible;
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
    // 根据阶段与候选列表更新高亮：可行动作 > 技能选目标 > 当前玩家可选提示。
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
