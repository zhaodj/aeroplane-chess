use bevy::ecs::system::SystemParam;
use bevy::{prelude::*, sprite::Anchor};

use crate::constants::BOARD_Z_LAYER;
use crate::domain::piece::{PieceState, PieceStatus};
use crate::domain::player::PlayerControl;
use crate::gameplay::match_flow::{BoardLayout, PlayerRoster};
use crate::gameplay::skill_flow::{SkillRoster, dash_bonus};
use crate::gameplay::turn_flow::{
    FINISH_DISTANCE, PieceEffectKind, PlannedAction, TurnInputState, TurnState,
    world_position_for_piece,
};
use crate::i18n::{Language, LanguageSettings};
use crate::plugins::effects_plugin::EffectRevealDelays;
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
                (
                    update_piece_highlight,
                    update_piece_stack_visuals,
                    update_move_target_guides,
                    update_piece_stack_count_badges,
                    update_piece_shield_badges,
                    update_piece_effect_badges,
                )
                    .chain()
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
/// 棋子可见图形根节点：叠放错位只作用在这里，不污染逻辑 Transform。
struct PieceVisual {
    piece_id: u8,
}

#[derive(Component)]
/// 当前可点击棋子的高亮外环。
struct PieceSelectableHalo {
    piece_id: u8,
}

#[derive(Component)]
/// 选棋阶段展示的目标虚影根节点。
struct MoveTargetGuideTarget {
    piece_id: u8,
}

#[derive(Component)]
/// 目标虚影上的虚线飞机线段。
struct MoveTargetGuidePlaneSegment {
    piece_id: u8,
}

#[derive(Component)]
/// 从当前棋子指向目标虚影的虚线连接段。
struct MoveTargetGuideConnectorDash {
    piece_id: u8,
    dash_index: usize,
}

#[derive(Component)]
/// 当前不可点击棋子的灰色遮罩。
struct PieceDisabledOverlay {
    piece_id: u8,
}

#[derive(Component)]
/// 当前不可点击棋子的禁止斜杠。
struct PieceDisabledSlash {
    piece_id: u8,
}

#[derive(Component)]
/// 同一逻辑格上多枚棋子的数量徽标。
struct PieceStackCountBadge {
    piece_id: u8,
}

#[derive(Component)]
/// 同格棋子数量徽标文本。
struct PieceStackCountBadgeText {
    piece_id: u8,
}

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

#[derive(Component)]
/// 最近一次特殊格结果附加在棋子旁的短标签。
struct PieceEffectBadge {
    piece_id: u8,
}

#[derive(Component)]
/// 特殊格结果标签文字。
struct PieceEffectBadgeText {
    piece_id: u8,
}

const PIECE_HITBOX_SIZE: f32 = 32.0;
const PIECE_TOKEN_RADIUS: f32 = 14.0;
const SELECTABLE_HALO_RADIUS: f32 = PIECE_TOKEN_RADIUS + 5.0;
const MOVE_TARGET_GUIDE_Z: f32 = BOARD_Z_LAYER + 1.36;
const MOVE_TARGET_CONNECTOR_Z: f32 = BOARD_Z_LAYER + 0.72;
const MOVE_TARGET_CONNECTOR_DASH_COUNT: usize = 10;
const MOVE_TARGET_CONNECTOR_THICKNESS: f32 = 2.0;
const MOVE_TARGET_PLANE_THICKNESS: f32 = 1.75;
const MOVE_TARGET_PLANE_SCALE: f32 = 0.82;
const MOVE_TARGET_DASH_LENGTH: f32 = 4.2;
const MOVE_TARGET_DASH_GAP: f32 = 3.0;
const DISABLED_OVERLAY_RADIUS: f32 = PIECE_TOKEN_RADIUS + 1.6;
const DISABLED_SLASH_SIZE: Vec2 = Vec2::new(31.0, 3.8);
const STACK_BADGE_SIZE: Vec2 = Vec2::new(25.0, 15.0);
const STACK_BADGE_OFFSET: Vec2 = Vec2::new(-18.0, 20.0);
const SHIELD_BADGE_MIN_SIZE: Vec2 = Vec2::new(28.0, 16.0);
const SHIELD_BADGE_OFFSET: Vec2 = Vec2::new(17.0, 14.0);
const EFFECT_BADGE_SIZE: Vec2 = Vec2::new(50.0, 14.0);
const EFFECT_BADGE_OFFSET: Vec2 = Vec2::new(0.0, -23.0);
const PLANE_ICON_BASE_ANGLE: f32 = std::f32::consts::FRAC_PI_4;
const PLANE_ICON_POINTS: &[Vec2] = &[
    Vec2::new(-9.12, -3.40),
    Vec2::new(-5.31, -5.31),
    Vec2::new(-3.40, -9.12),
    Vec2::new(-1.49, -7.21),
    Vec2::new(-1.49, -4.35),
    Vec2::new(1.85, -1.01),
    Vec2::new(4.71, -9.60),
    Vec2::new(7.58, -6.74),
    Vec2::new(5.19, 2.33),
    Vec2::new(9.01, 6.14),
    Vec2::new(6.14, 9.01),
    Vec2::new(2.33, 5.19),
    Vec2::new(-6.74, 7.58),
    Vec2::new(-9.60, 4.71),
    Vec2::new(-1.01, 1.85),
    Vec2::new(-4.35, -1.49),
    Vec2::new(-7.21, -1.49),
    Vec2::new(-9.12, -3.40),
];

type PieceTransformQuery<'w, 's> = Query<
    'w,
    's,
    (&'static PieceId, &'static PieceState, &'static Transform),
    (
        Without<MoveTargetGuideTarget>,
        Without<MoveTargetGuideConnectorDash>,
        Without<MoveTargetGuidePlaneSegment>,
    ),
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
        &'static mut TextColor,
        &'static mut Transform,
        &'static mut Visibility,
    ),
    (Without<PieceId>, Without<PieceShieldBadge>),
>;

#[derive(SystemParam)]
struct ShieldBadgeData<'w, 's> {
    board_layout: Res<'w, BoardLayout>,
    player_roster: Res<'w, PlayerRoster>,
    skill_roster: Res<'w, SkillRoster>,
    turn_state: Res<'w, TurnState>,
    input_state: Res<'w, TurnInputState>,
    reveal_delays: Res<'w, EffectRevealDelays>,
    language_settings: Res<'w, LanguageSettings>,
    piece_query: PieceTransformQuery<'w, 's>,
}

#[derive(SystemParam)]
struct ShieldBadgeNodes<'w, 's> {
    badge_query: ShieldBadgeQuery<'w, 's>,
    badge_text_query: ShieldBadgeTextQuery<'w, 's>,
}

type PieceVisualQuery<'w, 's> = Query<
    'w,
    's,
    (&'static PieceVisual, &'static mut Transform),
    (Without<PieceId>, Without<PieceShieldBadge>),
>;

#[derive(Clone)]
struct ShieldBadgeInfo {
    label: String,
    fill: Color,
    text: Color,
    size: Vec2,
}

#[derive(Clone, Copy)]
struct PieceVisualInfo {
    piece_id: u8,
    visual_local_translation: Vec3,
    visual_scale: f32,
    visual_translation: Vec3,
    stack_badge_translation: Vec3,
    stack_count: usize,
    is_stack_leader: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PieceInteractionVisualState {
    Neutral,
    Selectable,
    Disabled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct StackKey {
    x: i32,
    y: i32,
}

#[derive(Clone, Copy)]
struct StackEntry {
    piece_id: u8,
    key: StackKey,
    current_translation: Vec3,
    current_rotation: Quat,
    current_scale: Vec3,
}

type StackCountBadgeQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static PieceStackCountBadge,
        &'static mut Sprite,
        &'static mut Transform,
        &'static mut Visibility,
    ),
    (Without<PieceId>, Without<PieceStackCountBadgeText>),
>;
type StackCountBadgeTextQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static PieceStackCountBadgeText,
        &'static mut Text2d,
        &'static mut TextColor,
        &'static mut Transform,
        &'static mut Visibility,
    ),
    (Without<PieceId>, Without<PieceStackCountBadge>),
>;

type EffectBadgeQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static PieceEffectBadge,
        &'static mut Sprite,
        &'static mut Transform,
        &'static mut Visibility,
    ),
    (Without<PieceId>, Without<PieceEffectBadgeText>),
>;
type EffectBadgeTextQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static PieceEffectBadgeText,
        &'static mut Text2d,
        &'static mut TextColor,
        &'static mut Transform,
        &'static mut Visibility,
    ),
    (Without<PieceId>, Without<PieceEffectBadge>),
>;

type MoveTargetGuideTargetQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static MoveTargetGuideTarget,
        &'static mut Transform,
        &'static mut Visibility,
    ),
    (
        Without<MoveTargetGuideConnectorDash>,
        Without<MoveTargetGuidePlaneSegment>,
    ),
>;
type MoveTargetGuidePlaneQuery<'w, 's> = Query<
    'w,
    's,
    (&'static MoveTargetGuidePlaneSegment, &'static mut Sprite),
    (
        Without<MoveTargetGuideTarget>,
        Without<MoveTargetGuideConnectorDash>,
    ),
>;
type MoveTargetGuideConnectorQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static MoveTargetGuideConnectorDash,
        &'static mut Sprite,
        &'static mut Transform,
        &'static mut Visibility,
    ),
    (
        Without<MoveTargetGuideTarget>,
        Without<MoveTargetGuidePlaneSegment>,
    ),
>;

#[derive(Clone, Copy, Debug)]
pub(crate) struct MoveTargetGuidePiece {
    pub piece_id: u8,
    pub piece_state: PieceState,
    pub origin: Vec2,
    pub connector_origin: Vec2,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MoveTargetGuideInfo {
    pub piece_id: u8,
    pub origin: Vec2,
    pub connector_origin: Vec2,
    pub target: Vec2,
    pub display_target: Vec2,
    pub connector_target: Option<Vec2>,
}

fn spawn_pieces(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    player_roster: Res<PlayerRoster>,
) {
    // 按玩家机库槽位生成所有棋子实体，并初始化为 InHangar 状态。
    let mut piece_id = 1;

    for player in &player_roster.players {
        for &hangar_slot in &player.hangar_slots {
            let current_piece_id = piece_id;
            commands
                .spawn((
                    Sprite::from_color(
                        Color::srgba(1.0, 1.0, 1.0, 0.0),
                        Vec2::splat(PIECE_HITBOX_SIZE),
                    ),
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
                        motion_serial: 0,
                    },
                    Name::new(format!(
                        "Piece_P{}_{}",
                        player.state.player_id, current_piece_id
                    )),
                    PieceEntity,
                ))
                .with_children(|parent| {
                    parent
                        .spawn((
                            Transform::default(),
                            Visibility::Visible,
                            PieceVisual {
                                piece_id: current_piece_id,
                            },
                            Name::new(format!("PieceVisual_{}", current_piece_id)),
                            PieceEntity,
                        ))
                        .with_children(|visual| {
                            spawn_piece_token(
                                visual,
                                &mut meshes,
                                &mut materials,
                                player.color,
                                current_piece_id,
                            );
                        });
                });
            spawn_move_target_guide(&mut commands, player.color, current_piece_id);
            commands.spawn((
                Sprite::from_color(Color::srgba(0.10, 0.12, 0.16, 0.92), STACK_BADGE_SIZE),
                Transform::from_xyz(
                    hangar_slot.x + STACK_BADGE_OFFSET.x,
                    hangar_slot.y + STACK_BADGE_OFFSET.y,
                    BOARD_Z_LAYER + 2.05,
                ),
                Visibility::Hidden,
                PieceStackCountBadge {
                    piece_id: current_piece_id,
                },
                Name::new(format!("PieceStackCountBadge_{}", current_piece_id)),
                PieceEntity,
            ));
            commands.spawn((
                Text2d::new(""),
                TextFont {
                    font_size: FontSize::Px(10.5),
                    ..default()
                },
                TextColor(Color::WHITE),
                TextLayout::justify(Justify::Center),
                Anchor::CENTER,
                Transform::from_xyz(
                    hangar_slot.x + STACK_BADGE_OFFSET.x,
                    hangar_slot.y + STACK_BADGE_OFFSET.y - 0.5,
                    BOARD_Z_LAYER + 3.05,
                ),
                Visibility::Hidden,
                PieceStackCountBadgeText {
                    piece_id: current_piece_id,
                },
                Name::new(format!("PieceStackCountBadgeText_{}", current_piece_id)),
                PieceEntity,
            ));
            commands.spawn((
                Sprite::from_color(Color::srgba(0.06, 0.26, 0.58, 0.94), SHIELD_BADGE_MIN_SIZE),
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
                    font_size: FontSize::Px(11.0),
                    ..default()
                },
                TextColor(Color::WHITE),
                TextLayout::justify(Justify::Center),
                Anchor::CENTER,
                Transform::from_xyz(
                    hangar_slot.x + SHIELD_BADGE_OFFSET.x,
                    hangar_slot.y + SHIELD_BADGE_OFFSET.y - 0.5,
                    BOARD_Z_LAYER + 3.0,
                ),
                Visibility::Hidden,
                PieceShieldBadgeText {
                    piece_id: current_piece_id,
                },
                Name::new(format!("PieceShieldBadgeText_{}", current_piece_id)),
                PieceEntity,
            ));

            commands.spawn((
                Sprite::from_color(Color::srgba(0.12, 0.16, 0.22, 0.90), EFFECT_BADGE_SIZE),
                Transform::from_xyz(
                    hangar_slot.x + EFFECT_BADGE_OFFSET.x,
                    hangar_slot.y + EFFECT_BADGE_OFFSET.y,
                    BOARD_Z_LAYER + 2.15,
                ),
                Visibility::Hidden,
                PieceEffectBadge {
                    piece_id: current_piece_id,
                },
                Name::new(format!("PieceEffectBadge_{}", current_piece_id)),
                PieceEntity,
            ));
            commands.spawn((
                Text2d::new(""),
                TextFont {
                    font_size: FontSize::Px(10.5),
                    ..default()
                },
                TextColor(Color::WHITE),
                TextLayout::justify(Justify::Center),
                Transform::from_xyz(
                    hangar_slot.x + EFFECT_BADGE_OFFSET.x,
                    hangar_slot.y + EFFECT_BADGE_OFFSET.y - 6.0,
                    BOARD_Z_LAYER + 3.15,
                ),
                Visibility::Hidden,
                PieceEffectBadgeText {
                    piece_id: current_piece_id,
                },
                Name::new(format!("PieceEffectBadgeText_{}", current_piece_id)),
                PieceEntity,
            ));

            piece_id += 1;
        }
    }
}

fn spawn_move_target_guide(commands: &mut Commands, color: Color, piece_id: u8) {
    commands
        .spawn((
            Transform::from_xyz(0.0, 0.0, MOVE_TARGET_GUIDE_Z),
            Visibility::Hidden,
            MoveTargetGuideTarget { piece_id },
            Name::new(format!("MoveTargetGuideTarget_{piece_id}")),
            PieceEntity,
        ))
        .with_children(|parent| {
            spawn_dashed_target_plane(parent, color, piece_id);
        });

    for dash_index in 0..MOVE_TARGET_CONNECTOR_DASH_COUNT {
        commands.spawn((
            Sprite::from_color(color.with_alpha(0.0), Vec2::ZERO),
            Transform::from_xyz(0.0, 0.0, MOVE_TARGET_CONNECTOR_Z),
            Visibility::Hidden,
            MoveTargetGuideConnectorDash {
                piece_id,
                dash_index,
            },
            Name::new(format!("MoveTargetGuideConnector_{piece_id}_{dash_index}")),
            PieceEntity,
        ));
    }
}

fn spawn_dashed_target_plane(parent: &mut ChildSpawnerCommands, color: Color, piece_id: u8) {
    for (segment_index, segment) in PLANE_ICON_POINTS.windows(2).enumerate() {
        spawn_dashed_target_plane_segment(
            parent,
            segment[0] * MOVE_TARGET_PLANE_SCALE,
            segment[1] * MOVE_TARGET_PLANE_SCALE,
            color,
            piece_id,
            segment_index,
        );
    }
}

fn spawn_dashed_target_plane_segment(
    parent: &mut ChildSpawnerCommands,
    start: Vec2,
    end: Vec2,
    color: Color,
    piece_id: u8,
    segment_index: usize,
) {
    let delta = end - start;
    let length = delta.length();
    if length <= 0.01 {
        return;
    }

    let direction = delta / length;
    let mut cursor = 0.0;
    let mut dash_index = 0;
    while cursor < length {
        let dash_end = (cursor + MOVE_TARGET_DASH_LENGTH).min(length);
        if dash_end > cursor {
            let dash_start = start + direction * cursor;
            let dash_stop = start + direction * dash_end;
            spawn_move_target_line(
                parent,
                dash_start,
                dash_stop,
                MOVE_TARGET_PLANE_THICKNESS,
                color.with_alpha(0.0),
                0.0,
                MoveTargetGuidePlaneSegment { piece_id },
                format!("MoveTargetGuidePlane_{piece_id}_{segment_index}_{dash_index}"),
            );
        }
        cursor += MOVE_TARGET_DASH_LENGTH + MOVE_TARGET_DASH_GAP;
        dash_index += 1;
    }
}

fn spawn_move_target_line<T: Component>(
    parent: &mut ChildSpawnerCommands,
    start: Vec2,
    end: Vec2,
    thickness: f32,
    color: Color,
    z: f32,
    marker: T,
    name: impl Into<String>,
) {
    let delta = end - start;
    let length = delta.length();
    if length <= 0.01 {
        return;
    }
    let center = (start + end) * 0.5;
    parent.spawn((
        Sprite::from_color(color, Vec2::new(length, thickness)),
        Transform {
            translation: Vec3::new(center.x, center.y, z),
            rotation: Quat::from_rotation_z(delta.y.atan2(delta.x)),
            ..default()
        },
        marker,
        Name::new(name.into()),
        PieceEntity,
    ));
}

fn facing_rotation_for_piece(
    piece_state: PieceState,
    current_translation: Vec3,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
) -> Option<Quat> {
    let current_position = current_piece_position_for_facing(
        piece_state,
        current_translation,
        board_layout,
        player_roster,
    );
    let target_position = next_facing_target(piece_state, board_layout, player_roster)?;
    rotation_for_direction(target_position - current_position)
}

fn current_piece_position_for_facing(
    piece_state: PieceState,
    current_translation: Vec3,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
) -> Vec2 {
    if matches!(piece_state.status, PieceStatus::InHangar) {
        return current_translation.truncate();
    }

    world_position_for_piece(
        piece_state.owner_player_id,
        piece_state.progress,
        piece_state.status,
        board_layout,
        player_roster,
    )
    .unwrap_or_else(|| current_translation.truncate())
}

fn next_facing_target(
    piece_state: PieceState,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
) -> Option<Vec2> {
    match piece_state.status {
        PieceStatus::InHangar => world_position_for_piece(
            piece_state.owner_player_id,
            0,
            PieceStatus::AtLaunch,
            board_layout,
            player_roster,
        ),
        PieceStatus::AtLaunch => world_position_for_piece(
            piece_state.owner_player_id,
            0,
            PieceStatus::Active,
            board_layout,
            player_roster,
        ),
        PieceStatus::Active if piece_state.progress < FINISH_DISTANCE => {
            let next_progress = piece_state.progress.saturating_add(1).min(FINISH_DISTANCE);
            let next_status = if next_progress == FINISH_DISTANCE {
                PieceStatus::Finished
            } else {
                PieceStatus::Active
            };
            world_position_for_piece(
                piece_state.owner_player_id,
                next_progress,
                next_status,
                board_layout,
                player_roster,
            )
        }
        PieceStatus::Active | PieceStatus::Finished => None,
    }
}

fn rotation_for_direction(direction: Vec2) -> Option<Quat> {
    if direction.length_squared() < 0.25 {
        return None;
    }

    let target_angle = direction.y.atan2(direction.x);
    Some(Quat::from_rotation_z(target_angle - PLANE_ICON_BASE_ANGLE))
}

fn spawn_piece_token(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    color: Color,
    piece_id: u8,
) {
    parent.spawn((
        Mesh2d(meshes.add(Circle::new(SELECTABLE_HALO_RADIUS))),
        MeshMaterial2d(materials.add(ColorMaterial::from(Color::srgba(1.0, 0.86, 0.18, 0.58)))),
        Transform::from_xyz(0.0, 0.0, 0.0),
        Visibility::Hidden,
        PieceSelectableHalo { piece_id },
        Name::new("PieceSelectableHalo"),
        PieceEntity,
    ));
    parent.spawn((
        Mesh2d(meshes.add(Circle::new(PIECE_TOKEN_RADIUS + 1.0))),
        MeshMaterial2d(materials.add(ColorMaterial::from(Color::BLACK))),
        Transform::from_xyz(0.0, 0.0, 0.01),
        Name::new("PieceTokenBorder"),
        PieceEntity,
    ));
    parent.spawn((
        Mesh2d(meshes.add(Circle::new(PIECE_TOKEN_RADIUS))),
        MeshMaterial2d(materials.add(ColorMaterial::from(Color::WHITE))),
        Transform::from_xyz(0.0, 0.0, 0.02),
        Name::new("PieceTokenFill"),
        PieceEntity,
    ));

    for (index, segment) in PLANE_ICON_POINTS.windows(2).enumerate() {
        spawn_piece_line(
            parent,
            segment[0] * 0.82,
            segment[1] * 0.82,
            1.8,
            color,
            0.05 + index as f32 * 0.001,
            format!("PiecePlane_{index}"),
        );
    }

    parent.spawn((
        Mesh2d(meshes.add(Circle::new(DISABLED_OVERLAY_RADIUS))),
        MeshMaterial2d(materials.add(ColorMaterial::from(Color::srgba(0.82, 0.84, 0.86, 0.62)))),
        Transform::from_xyz(0.0, 0.0, 0.095),
        Visibility::Hidden,
        PieceDisabledOverlay { piece_id },
        Name::new("PieceDisabledOverlay"),
        PieceEntity,
    ));
    parent.spawn((
        Sprite::from_color(Color::srgba(0.78, 0.05, 0.04, 0.92), DISABLED_SLASH_SIZE),
        Transform {
            translation: Vec3::new(0.0, 0.0, 0.105),
            rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_4),
            ..default()
        },
        Visibility::Hidden,
        PieceDisabledSlash { piece_id },
        Name::new("PieceDisabledSlash"),
        PieceEntity,
    ));
}

fn spawn_piece_line(
    parent: &mut ChildSpawnerCommands,
    start: Vec2,
    end: Vec2,
    thickness: f32,
    color: Color,
    z: f32,
    name: impl Into<String>,
) {
    let delta = end - start;
    let length = delta.length();
    if length <= 0.01 {
        return;
    }
    let center = (start + end) * 0.5;
    parent.spawn((
        Sprite::from_color(color, Vec2::new(length, thickness)),
        Transform {
            translation: Vec3::new(center.x, center.y, z),
            rotation: Quat::from_rotation_z(delta.y.atan2(delta.x)),
            ..default()
        },
        Name::new(name.into()),
        PieceEntity,
    ));
}

fn update_piece_stack_visuals(
    board_layout: Res<BoardLayout>,
    player_roster: Res<PlayerRoster>,
    piece_query: PieceTransformQuery,
    mut visual_query: PieceVisualQuery,
) {
    let visual_infos = piece_visual_infos(&piece_query, &board_layout, &player_roster);

    for (visual, mut transform) in &mut visual_query {
        let Some(info) = visual_infos
            .iter()
            .find(|info| info.piece_id == visual.piece_id)
        else {
            transform.translation = Vec3::ZERO;
            transform.scale = Vec3::ONE;
            continue;
        };

        transform.translation = info.visual_local_translation;
        transform.scale = Vec3::splat(info.visual_scale);
    }
}

fn update_piece_stack_count_badges(
    board_layout: Res<BoardLayout>,
    player_roster: Res<PlayerRoster>,
    piece_query: PieceTransformQuery,
    mut badge_query: StackCountBadgeQuery,
    mut badge_text_query: StackCountBadgeTextQuery,
) {
    let visual_infos = piece_visual_infos(&piece_query, &board_layout, &player_roster);

    for (badge, mut sprite, mut transform, mut visibility) in &mut badge_query {
        let Some(info) = visual_infos
            .iter()
            .find(|info| info.piece_id == badge.piece_id)
        else {
            *visibility = Visibility::Hidden;
            continue;
        };

        if info.stack_count <= 1 || !info.is_stack_leader {
            *visibility = Visibility::Hidden;
            continue;
        }

        sprite.custom_size = Some(STACK_BADGE_SIZE);
        transform.translation = info.stack_badge_translation
            + Vec3::new(STACK_BADGE_OFFSET.x, STACK_BADGE_OFFSET.y, 0.0);
        transform.translation.z = BOARD_Z_LAYER + 2.05;
        *visibility = Visibility::Visible;
    }

    for (badge_text, mut text, mut text_color, mut transform, mut visibility) in
        &mut badge_text_query
    {
        let Some(info) = visual_infos
            .iter()
            .find(|info| info.piece_id == badge_text.piece_id)
        else {
            *visibility = Visibility::Hidden;
            continue;
        };

        if info.stack_count <= 1 || !info.is_stack_leader {
            *visibility = Visibility::Hidden;
            continue;
        }

        *text = Text2d::new(format!("×{}", info.stack_count));
        *text_color = TextColor(Color::WHITE);
        transform.translation = info.stack_badge_translation
            + Vec3::new(STACK_BADGE_OFFSET.x, STACK_BADGE_OFFSET.y - 0.5, 0.0);
        transform.translation.z = BOARD_Z_LAYER + 3.05;
        *visibility = Visibility::Visible;
    }
}

fn piece_visual_infos(
    piece_query: &PieceTransformQuery,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
) -> Vec<PieceVisualInfo> {
    let mut visual_infos = Vec::new();
    let mut entries = Vec::new();

    for (piece_id, piece_state, transform) in piece_query.iter() {
        if matches!(piece_state.status, PieceStatus::InHangar) {
            visual_infos.push(unstacked_piece_visual_info(
                piece_id.0,
                transform.translation,
            ));
            continue;
        }

        let logical_center = stack_logical_center(
            *piece_state,
            transform.translation,
            board_layout,
            player_roster,
        );
        entries.push(StackEntry {
            piece_id: piece_id.0,
            key: stack_key(logical_center),
            current_translation: transform.translation,
            current_rotation: transform.rotation,
            current_scale: transform.scale,
        });
    }

    entries.sort_by_key(|entry| (entry.key, entry.piece_id));

    let mut group_start = 0;
    while group_start < entries.len() {
        let group_key = entries[group_start].key;
        let mut group_end = group_start + 1;
        while group_end < entries.len() && entries[group_end].key == group_key {
            group_end += 1;
        }

        let group = &entries[group_start..group_end];
        let stack_count = group.len();
        let group_center = group.iter().fold(Vec2::ZERO, |sum, entry| {
            sum + entry.current_translation.truncate()
        }) / stack_count as f32;
        let leader_id = group[0].piece_id;

        for (index, entry) in group.iter().enumerate() {
            let visual_offset = stack_visual_offset(index, stack_count);
            let visual_z_offset = stack_visual_z_offset(index, stack_count);
            visual_infos.push(PieceVisualInfo {
                piece_id: entry.piece_id,
                visual_local_translation: stack_visual_local_translation(
                    visual_offset,
                    visual_z_offset,
                    entry.current_rotation,
                    entry.current_scale,
                ),
                visual_scale: stack_visual_scale(stack_count),
                visual_translation: entry.current_translation + visual_offset.extend(0.0),
                stack_badge_translation: group_center.extend(BOARD_Z_LAYER + 2.05),
                stack_count,
                is_stack_leader: entry.piece_id == leader_id,
            });
        }

        group_start = group_end;
    }

    visual_infos
}

fn unstacked_piece_visual_info(piece_id: u8, current_translation: Vec3) -> PieceVisualInfo {
    PieceVisualInfo {
        piece_id,
        visual_local_translation: Vec3::ZERO,
        visual_scale: 1.0,
        visual_translation: current_translation,
        stack_badge_translation: current_translation,
        stack_count: 1,
        is_stack_leader: true,
    }
}

fn stack_logical_center(
    piece_state: PieceState,
    current_translation: Vec3,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
) -> Vec2 {
    if matches!(piece_state.status, PieceStatus::InHangar) {
        return current_translation.truncate();
    }

    world_position_for_piece(
        piece_state.owner_player_id,
        piece_state.progress,
        piece_state.status,
        board_layout,
        player_roster,
    )
    .unwrap_or_else(|| current_translation.truncate())
}

fn stack_key(center: Vec2) -> StackKey {
    StackKey {
        x: (center.x * 10.0).round() as i32,
        y: (center.y * 10.0).round() as i32,
    }
}

fn stack_visual_offset(index: usize, stack_count: usize) -> Vec2 {
    match stack_count {
        0 | 1 => Vec2::ZERO,
        2 => [Vec2::new(-6.5, 0.0), Vec2::new(6.5, 0.0)][index],
        3 => [
            Vec2::new(-6.5, -4.8),
            Vec2::new(6.5, -4.8),
            Vec2::new(0.0, 6.8),
        ][index],
        4 => [
            Vec2::new(-6.5, 6.0),
            Vec2::new(6.5, 6.0),
            Vec2::new(-6.5, -6.0),
            Vec2::new(6.5, -6.0),
        ][index],
        _ => {
            let angle = -std::f32::consts::FRAC_PI_2
                + index as f32 * std::f32::consts::TAU / stack_count as f32;
            Vec2::new(angle.cos() * 8.2, angle.sin() * 8.2)
        }
    }
}

fn stack_visual_scale(stack_count: usize) -> f32 {
    match stack_count {
        0 | 1 => 1.0,
        2 => 0.90,
        3 | 4 => 0.84,
        _ => 0.76,
    }
}

fn stack_visual_z_offset(index: usize, stack_count: usize) -> f32 {
    if stack_count <= 1 {
        0.0
    } else {
        index as f32 * 0.025
    }
}

fn stack_visual_local_translation(
    visual_offset: Vec2,
    visual_z_offset: f32,
    parent_rotation: Quat,
    parent_scale: Vec3,
) -> Vec3 {
    let safe_scale = Vec3::new(
        parent_scale.x.abs().max(0.001).copysign(parent_scale.x),
        parent_scale.y.abs().max(0.001).copysign(parent_scale.y),
        parent_scale.z.abs().max(0.001).copysign(parent_scale.z),
    );
    let rotated = parent_rotation.inverse().mul_vec3(Vec3::new(
        visual_offset.x,
        visual_offset.y,
        visual_z_offset,
    ));
    Vec3::new(
        rotated.x / safe_scale.x,
        rotated.y / safe_scale.y,
        rotated.z / safe_scale.z,
    )
}

fn update_move_target_guides(
    time: Res<Time>,
    input_state: Res<TurnInputState>,
    game_phase: Res<State<GamePhase>>,
    board_layout: Res<BoardLayout>,
    player_roster: Res<PlayerRoster>,
    piece_query: PieceTransformQuery,
    mut target_query: MoveTargetGuideTargetQuery,
    mut plane_query: MoveTargetGuidePlaneQuery,
    mut connector_query: MoveTargetGuideConnectorQuery,
) {
    let guide_infos = if matches!(game_phase.get(), GamePhase::AwaitPieceSelect) {
        let visual_infos = piece_visual_infos(&piece_query, &board_layout, &player_roster);
        let pieces =
            move_target_guide_pieces(&piece_query, &visual_infos, &board_layout, &player_roster);
        move_target_guide_infos(
            input_state.pending_actions(),
            &pieces,
            &board_layout,
            &player_roster,
        )
    } else {
        Vec::new()
    };

    let pulse = move_target_guide_pulse(time.elapsed_secs());
    let target_alpha = 0.38 + pulse * 0.34;
    let connector_alpha = 0.16 + pulse * 0.16;
    let target_scale = 0.92 + pulse * 0.10;

    for (target, mut transform, mut visibility) in &mut target_query {
        let Some(info) = guide_infos
            .iter()
            .find(|info| info.piece_id == target.piece_id)
        else {
            *visibility = Visibility::Hidden;
            continue;
        };

        transform.translation = info.display_target.extend(MOVE_TARGET_GUIDE_Z);
        transform.rotation =
            rotation_for_direction(info.display_target - info.origin).unwrap_or(Quat::IDENTITY);
        transform.scale = Vec3::splat(target_scale);
        *visibility = Visibility::Visible;
    }

    for (segment, mut sprite) in &mut plane_query {
        if guide_infos
            .iter()
            .any(|info| info.piece_id == segment.piece_id)
        {
            sprite.color =
                move_target_guide_piece_color(segment.piece_id, &piece_query, &player_roster)
                    .with_alpha(target_alpha);
        } else {
            sprite.color = Color::WHITE.with_alpha(0.0);
        }
    }

    for (dash, mut sprite, mut transform, mut visibility) in &mut connector_query {
        let Some(info) = guide_infos
            .iter()
            .find(|info| info.piece_id == dash.piece_id)
        else {
            *visibility = Visibility::Hidden;
            continue;
        };

        let Some(connector_target) = info.connector_target else {
            *visibility = Visibility::Hidden;
            continue;
        };

        let Some(segment) =
            move_target_connector_segment(info.connector_origin, connector_target, dash.dash_index)
        else {
            *visibility = Visibility::Hidden;
            continue;
        };

        sprite.color = move_target_guide_piece_color(dash.piece_id, &piece_query, &player_roster)
            .mix(&Color::WHITE, 0.35)
            .with_alpha(connector_alpha);
        sprite.custom_size = Some(segment.size);
        transform.translation = segment.center.extend(MOVE_TARGET_CONNECTOR_Z);
        transform.rotation = segment.rotation;
        *visibility = Visibility::Visible;
    }
}

fn move_target_guide_pieces(
    piece_query: &PieceTransformQuery,
    visual_infos: &[PieceVisualInfo],
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
) -> Vec<MoveTargetGuidePiece> {
    piece_query
        .iter()
        .map(|(piece_id, piece_state, transform)| {
            let origin = visual_infos
                .iter()
                .find(|info| info.piece_id == piece_id.0)
                .map(|info| info.visual_translation.truncate())
                .unwrap_or_else(|| transform.translation.truncate());
            let connector_origin = stack_logical_center(
                *piece_state,
                transform.translation,
                board_layout,
                player_roster,
            );
            MoveTargetGuidePiece {
                piece_id: piece_id.0,
                piece_state: *piece_state,
                origin,
                connector_origin,
            }
        })
        .collect()
}

pub(crate) fn move_target_guide_infos(
    actions: &[PlannedAction],
    pieces: &[MoveTargetGuidePiece],
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
) -> Vec<MoveTargetGuideInfo> {
    let mut infos = actions
        .iter()
        .filter_map(|action| {
            let piece_id = action.piece_id();
            let piece = pieces.iter().find(|piece| piece.piece_id == piece_id)?;
            let target =
                move_target_for_action(action, piece.piece_state, board_layout, player_roster)?;
            Some(MoveTargetGuideInfo {
                piece_id,
                origin: piece.origin,
                connector_origin: piece.connector_origin,
                target,
                display_target: target,
                connector_target: Some(target),
            })
        })
        .collect::<Vec<_>>();

    let mut indices = (0..infos.len()).collect::<Vec<_>>();
    indices.sort_by_key(|index| (stack_key(infos[*index].target), infos[*index].piece_id));

    let mut group_start = 0;
    while group_start < indices.len() {
        let group_key = stack_key(infos[indices[group_start]].target);
        let mut group_end = group_start + 1;
        while group_end < indices.len() && stack_key(infos[indices[group_end]].target) == group_key
        {
            group_end += 1;
        }

        let count = group_end - group_start;
        for (offset_index, index) in indices[group_start..group_end].iter().enumerate() {
            infos[*index].display_target =
                infos[*index].target + move_target_overlap_offset(offset_index, count);
        }

        group_start = group_end;
    }

    assign_move_target_connector_targets(&mut infos);

    infos
}

fn assign_move_target_connector_targets(infos: &mut [MoveTargetGuideInfo]) {
    let mut indices = (0..infos.len()).collect::<Vec<_>>();
    indices.sort_by_key(|index| {
        (
            stack_key(infos[*index].connector_origin),
            stack_key(infos[*index].target),
            infos[*index].piece_id,
        )
    });

    let mut group_start = 0;
    while group_start < indices.len() {
        let origin_key = stack_key(infos[indices[group_start]].connector_origin);
        let target_key = stack_key(infos[indices[group_start]].target);
        let mut group_end = group_start + 1;
        while group_end < indices.len()
            && stack_key(infos[indices[group_end]].connector_origin) == origin_key
            && stack_key(infos[indices[group_end]].target) == target_key
        {
            group_end += 1;
        }

        let group = &indices[group_start..group_end];
        let representative = group[0];
        let duplicate_path = group.len() > 1;
        for index in group {
            infos[*index].connector_target = None;
        }
        infos[representative].connector_target = Some(if duplicate_path {
            infos[representative].target
        } else {
            infos[representative].display_target
        });

        group_start = group_end;
    }
}

pub(crate) fn pick_move_target_guide(
    guides: &[MoveTargetGuideInfo],
    cursor_world: Vec2,
    pick_radius: f32,
) -> Option<u8> {
    let pick_radius_sq = pick_radius * pick_radius;
    guides
        .iter()
        .filter_map(|guide| {
            let distance_sq = guide.display_target.distance_squared(cursor_world);
            (distance_sq <= pick_radius_sq).then_some((guide.piece_id, distance_sq))
        })
        .min_by(|(_, left), (_, right)| {
            left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(piece_id, _)| piece_id)
}

fn move_target_for_action(
    action: &PlannedAction,
    piece_state: PieceState,
    board_layout: &BoardLayout,
    player_roster: &PlayerRoster,
) -> Option<Vec2> {
    match *action {
        PlannedAction::Launch {
            target_progress, ..
        } => world_position_for_piece(
            piece_state.owner_player_id,
            target_progress,
            PieceStatus::AtLaunch,
            board_layout,
            player_roster,
        ),
        PlannedAction::Move {
            target_progress, ..
        } => {
            let target_status = if target_progress >= FINISH_DISTANCE {
                PieceStatus::Finished
            } else {
                PieceStatus::Active
            };
            world_position_for_piece(
                piece_state.owner_player_id,
                target_progress,
                target_status,
                board_layout,
                player_roster,
            )
        }
    }
}

fn move_target_overlap_offset(index: usize, count: usize) -> Vec2 {
    stack_visual_offset(index, count) * 0.92
}

fn move_target_guide_pulse(elapsed_secs: f32) -> f32 {
    (elapsed_secs * std::f32::consts::PI * 1.7).sin().abs()
}

fn move_target_guide_piece_color(
    piece_id: u8,
    piece_query: &PieceTransformQuery,
    player_roster: &PlayerRoster,
) -> Color {
    let Some((_, piece_state, _)) = piece_query
        .iter()
        .find(|(candidate_piece_id, _, _)| candidate_piece_id.0 == piece_id)
    else {
        return Color::WHITE;
    };

    player_roster
        .players
        .iter()
        .find(|player| player.state.player_id == piece_state.owner_player_id)
        .map(|player| player.color)
        .unwrap_or(Color::WHITE)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct MoveTargetConnectorSegment {
    center: Vec2,
    size: Vec2,
    rotation: Quat,
}

fn move_target_connector_segment(
    origin: Vec2,
    target: Vec2,
    dash_index: usize,
) -> Option<MoveTargetConnectorSegment> {
    let delta = target - origin;
    let length = delta.length();
    if length < 12.0 {
        return None;
    }

    let dash_count = move_target_connector_dash_count(length);
    if dash_index >= dash_count {
        return None;
    }

    let direction = delta / length;
    let step = length / dash_count as f32;
    let dash_length = (step * 0.46).clamp(5.0, 16.0);
    let center_distance = step * (dash_index as f32 + 0.5);
    let center = origin + direction * center_distance;

    Some(MoveTargetConnectorSegment {
        center,
        size: Vec2::new(dash_length, MOVE_TARGET_CONNECTOR_THICKNESS),
        rotation: Quat::from_rotation_z(delta.y.atan2(delta.x)),
    })
}

fn move_target_connector_dash_count(length: f32) -> usize {
    ((length / 28.0).ceil() as usize).clamp(2, MOVE_TARGET_CONNECTOR_DASH_COUNT)
}

fn update_piece_shield_badges(data: ShieldBadgeData, mut nodes: ShieldBadgeNodes) {
    let visual_infos =
        piece_visual_infos(&data.piece_query, &data.board_layout, &data.player_roster);
    let pieces = data
        .piece_query
        .iter()
        .map(|(piece_id, piece_state, transform)| {
            (
                piece_id.0,
                shield_badge_info(
                    data.reveal_delays
                        .visible_shield(piece_id.0, piece_state.shield),
                    piece_state.stack_shield,
                    movement_buff_bonus_for_piece(
                        piece_id.0,
                        piece_state,
                        &data.turn_state,
                        &data.input_state,
                        &data.skill_roster,
                    ),
                    data.language_settings.language,
                ),
                visual_infos
                    .iter()
                    .find(|info| info.piece_id == piece_id.0)
                    .map(|info| info.visual_translation)
                    .unwrap_or(transform.translation),
            )
        })
        .collect::<Vec<_>>();

    for (badge, mut sprite, mut transform, mut visibility) in &mut nodes.badge_query {
        let Some((_, shield_info, translation)) = pieces
            .iter()
            .find(|(piece_id, _, _)| *piece_id == badge.piece_id)
        else {
            *visibility = Visibility::Hidden;
            continue;
        };

        let Some(shield_info) = shield_info else {
            *visibility = Visibility::Hidden;
            continue;
        };

        sprite.color = shield_info.fill;
        sprite.custom_size = Some(shield_info.size);
        transform.translation =
            *translation + Vec3::new(SHIELD_BADGE_OFFSET.x, SHIELD_BADGE_OFFSET.y, 0.0);
        transform.translation.z = BOARD_Z_LAYER + 2.0;
        *visibility = Visibility::Visible;
    }

    for (badge_text, mut text, mut text_color, mut transform, mut visibility) in
        &mut nodes.badge_text_query
    {
        let Some((_, shield_info, translation)) = pieces
            .iter()
            .find(|(piece_id, _, _)| *piece_id == badge_text.piece_id)
        else {
            *visibility = Visibility::Hidden;
            continue;
        };

        let Some(shield_info) = shield_info else {
            *visibility = Visibility::Hidden;
            continue;
        };

        *text = Text2d::new(shield_info.label.clone());
        *text_color = TextColor(shield_info.text);
        transform.translation =
            *translation + Vec3::new(SHIELD_BADGE_OFFSET.x, SHIELD_BADGE_OFFSET.y - 0.5, 0.0);
        transform.translation.z = BOARD_Z_LAYER + 3.0;
        *visibility = Visibility::Visible;
    }
}

fn movement_buff_bonus_for_piece(
    piece_id: u8,
    piece_state: &PieceState,
    turn_state: &TurnState,
    input_state: &TurnInputState,
    skill_roster: &SkillRoster,
) -> Option<u8> {
    let bonus = dash_bonus(skill_roster, turn_state.current_player);
    if bonus == 0
        || piece_state.owner_player_id != turn_state.current_player
        || !matches!(
            piece_state.status,
            PieceStatus::AtLaunch | PieceStatus::Active
        )
    {
        return None;
    }

    let candidates = input_state.candidate_piece_ids();
    if !candidates.is_empty() && !candidates.contains(&piece_id) {
        return None;
    }

    Some(bonus)
}

fn shield_badge_info(
    shield: u8,
    stack_shield: u8,
    movement_bonus: Option<u8>,
    language: Language,
) -> Option<ShieldBadgeInfo> {
    let label = shield_badge_label(shield, stack_shield, movement_bonus, language)?;
    let has_movement_bonus = movement_bonus.unwrap_or_default() > 0;
    let fill = match (shield > 0, stack_shield > 0, has_movement_bonus) {
        (false, false, true) => Color::srgba(0.86, 0.38, 0.05, 0.95),
        (_, _, true) => Color::srgba(0.40, 0.22, 0.72, 0.95),
        (true, false, false) => Color::srgba(0.05, 0.34, 0.78, 0.95),
        (false, true, false) => Color::srgba(0.05, 0.48, 0.36, 0.95),
        (true, true, false) => Color::srgba(0.34, 0.22, 0.72, 0.95),
        (false, false, false) => return None,
    };

    Some(ShieldBadgeInfo {
        size: shield_badge_size(&label),
        label,
        fill,
        text: Color::WHITE,
    })
}

fn shield_badge_label(
    shield: u8,
    stack_shield: u8,
    movement_bonus: Option<u8>,
    language: Language,
) -> Option<String> {
    let mut labels = Vec::new();
    if shield > 0 {
        labels.push(match language {
            Language::SimplifiedChinese => format!("盾{shield}"),
            Language::English => format!("S{shield}"),
        });
    }
    if stack_shield > 0 {
        labels.push(match language {
            Language::SimplifiedChinese => format!("队盾{stack_shield}"),
            Language::English => format!("TS{stack_shield}"),
        });
    }
    if let Some(movement_bonus) = movement_bonus.filter(|bonus| *bonus > 0) {
        labels.push(match language {
            Language::SimplifiedChinese => format!("冲+{movement_bonus}"),
            Language::English => format!("D+{movement_bonus}"),
        });
    }

    (!labels.is_empty()).then(|| labels.join("+"))
}

fn shield_badge_size(label: &str) -> Vec2 {
    Vec2::new(
        SHIELD_BADGE_MIN_SIZE
            .x
            .max(10.0 + label.chars().count() as f32 * 6.2),
        SHIELD_BADGE_MIN_SIZE.y,
    )
}

fn update_piece_effect_badges(
    turn_state: Res<TurnState>,
    board_layout: Res<BoardLayout>,
    player_roster: Res<PlayerRoster>,
    language_settings: Res<LanguageSettings>,
    piece_query: PieceTransformQuery,
    mut badge_query: EffectBadgeQuery,
    mut badge_text_query: EffectBadgeTextQuery,
) {
    let visual_infos = piece_visual_infos(&piece_query, &board_layout, &player_roster);
    let pieces = piece_query
        .iter()
        .map(|(piece_id, _, transform)| {
            (
                piece_id.0,
                visual_infos
                    .iter()
                    .find(|info| info.piece_id == piece_id.0)
                    .map(|info| info.visual_translation)
                    .unwrap_or(transform.translation),
            )
        })
        .collect::<Vec<_>>();
    let notice = turn_state.last_piece_effect;

    for (badge, mut sprite, mut transform, mut visibility) in &mut badge_query {
        let Some(effect) = notice.filter(|effect| effect.piece_id == badge.piece_id) else {
            *visibility = Visibility::Hidden;
            continue;
        };
        let Some((_, translation)) = pieces
            .iter()
            .find(|(piece_id, _)| *piece_id == badge.piece_id)
        else {
            *visibility = Visibility::Hidden;
            continue;
        };

        sprite.color = piece_effect_color(effect.kind);
        transform.translation =
            *translation + Vec3::new(EFFECT_BADGE_OFFSET.x, EFFECT_BADGE_OFFSET.y, 0.0);
        transform.translation.z = BOARD_Z_LAYER + 2.15;
        *visibility = Visibility::Visible;
    }

    for (badge_text, mut text, mut text_color, mut transform, mut visibility) in
        &mut badge_text_query
    {
        let Some(effect) = notice.filter(|effect| effect.piece_id == badge_text.piece_id) else {
            *visibility = Visibility::Hidden;
            continue;
        };
        let Some((_, translation)) = pieces
            .iter()
            .find(|(piece_id, _)| *piece_id == badge_text.piece_id)
        else {
            *visibility = Visibility::Hidden;
            continue;
        };

        *text = Text2d::new(piece_effect_label(effect.kind, language_settings.language));
        *text_color = TextColor(piece_effect_text_color(effect.kind));
        transform.translation =
            *translation + Vec3::new(EFFECT_BADGE_OFFSET.x, EFFECT_BADGE_OFFSET.y - 6.0, 0.0);
        transform.translation.z = BOARD_Z_LAYER + 3.15;
        *visibility = Visibility::Visible;
    }
}

fn piece_effect_label(kind: PieceEffectKind, language: Language) -> &'static str {
    match (kind, language) {
        (PieceEffectKind::Attack, Language::SimplifiedChinese) => "攻",
        (PieceEffectKind::Defense, Language::SimplifiedChinese) => "盾",
        (PieceEffectKind::Attack, Language::English) => "ATK",
        (PieceEffectKind::Defense, Language::English) => "DEF",
    }
}

fn piece_effect_color(kind: PieceEffectKind) -> Color {
    match kind {
        PieceEffectKind::Attack => Color::srgba(0.86, 0.08, 0.08, 0.92),
        PieceEffectKind::Defense => Color::srgba(0.08, 0.34, 0.86, 0.92),
    }
}

fn piece_effect_text_color(kind: PieceEffectKind) -> Color {
    match kind {
        PieceEffectKind::Attack | PieceEffectKind::Defense => Color::WHITE,
    }
}

fn cleanup_pieces(
    mut commands: Commands,
    query: Query<Entity, (With<PieceEntity>, Without<ChildOf>)>,
) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
}

fn update_piece_highlight(
    input_state: Res<TurnInputState>,
    skill_target_state: Res<SkillTargetState>,
    board_layout: Res<BoardLayout>,
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
    mut halo_query: Query<
        (&PieceSelectableHalo, &mut Visibility),
        (Without<PieceDisabledOverlay>, Without<PieceDisabledSlash>),
    >,
    mut disabled_overlay_query: Query<
        (&PieceDisabledOverlay, &mut Visibility),
        (Without<PieceSelectableHalo>, Without<PieceDisabledSlash>),
    >,
    mut disabled_slash_query: Query<
        (&PieceDisabledSlash, &mut Visibility),
        (Without<PieceSelectableHalo>, Without<PieceDisabledOverlay>),
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
    let action_candidates = input_state.candidate_piece_ids();
    let skill_candidates = skill_target_state.candidate_piece_ids();
    let mut visual_states = Vec::new();

    for (piece_id, piece_state, _base_color, mut sprite, mut transform) in &mut query {
        sprite.color = Color::srgba(1.0, 1.0, 1.0, 0.0);
        if let Some(rotation) = facing_rotation_for_piece(
            *piece_state,
            transform.translation,
            &board_layout,
            &player_roster,
        ) {
            transform.rotation = rotation;
        }

        let visual_state = piece_interaction_visual_state(
            piece_id.0,
            piece_state,
            turn_state.current_player,
            current_player_control,
            selectable,
            action_candidates,
            skill_selectable,
            skill_candidates,
            skill_target_state.is_active(),
        );
        visual_states.push((piece_id.0, visual_state));

        if visual_state == PieceInteractionVisualState::Selectable {
            transform.scale = Vec3::splat(1.18);
        } else if matches!(current_player_control, Some(PlayerControl::Human))
            && action_candidates.is_empty()
            && skill_candidates.is_empty()
            && piece_state.owner_player_id == turn_state.current_player
        {
            transform.scale = Vec3::splat(1.08);
        } else if visual_state == PieceInteractionVisualState::Disabled {
            transform.scale = Vec3::splat(0.96);
        } else {
            transform.scale = Vec3::ONE;
        }
    }

    for (halo, mut visibility) in &mut halo_query {
        *visibility = visibility_for_piece_state(
            &visual_states,
            halo.piece_id,
            PieceInteractionVisualState::Selectable,
        );
    }

    for (overlay, mut visibility) in &mut disabled_overlay_query {
        *visibility = visibility_for_piece_state(
            &visual_states,
            overlay.piece_id,
            PieceInteractionVisualState::Disabled,
        );
    }

    for (slash, mut visibility) in &mut disabled_slash_query {
        *visibility = visibility_for_piece_state(
            &visual_states,
            slash.piece_id,
            PieceInteractionVisualState::Disabled,
        );
    }
}

fn piece_interaction_visual_state(
    piece_id: u8,
    piece_state: &PieceState,
    current_player: u8,
    current_player_control: Option<PlayerControl>,
    action_selection_active: bool,
    action_candidates: &[u8],
    skill_selection_active: bool,
    skill_candidates: &[u8],
    skill_target_state_active: bool,
) -> PieceInteractionVisualState {
    if action_selection_active && !action_candidates.is_empty() {
        if action_candidates.contains(&piece_id) {
            return PieceInteractionVisualState::Selectable;
        }

        if matches!(current_player_control, Some(PlayerControl::Human))
            && piece_state.owner_player_id == current_player
        {
            return PieceInteractionVisualState::Disabled;
        }
    }

    if skill_selection_active && skill_target_state_active && !skill_candidates.is_empty() {
        if skill_candidates.contains(&piece_id) {
            return PieceInteractionVisualState::Selectable;
        }

        if !matches!(piece_state.status, PieceStatus::Finished) {
            return PieceInteractionVisualState::Disabled;
        }
    }

    PieceInteractionVisualState::Neutral
}

fn visibility_for_piece_state(
    visual_states: &[(u8, PieceInteractionVisualState)],
    piece_id: u8,
    target_state: PieceInteractionVisualState,
) -> Visibility {
    if visual_states
        .iter()
        .any(|(candidate_id, state)| *candidate_id == piece_id && *state == target_state)
    {
        Visibility::Visible
    } else {
        Visibility::Hidden
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::game_mode::GameMode;
    use crate::domain::player::PlayerControl;
    use crate::domain::rules::LaunchRule;
    use crate::gameplay::ai::AiDifficulty;
    use crate::gameplay::match_flow::{MatchSetup, PlayerRoster, PlayerSeat, build_match_rosters};
    use bevy::ecs::system::SystemState;

    fn test_roster() -> (BoardLayout, PlayerRoster) {
        let setup = MatchSetup {
            mode: GameMode::TwoVsTwo,
            rule_set: crate::data::rule_set::RuleSet::Creative,
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

    fn piece_state(owner_player_id: u8, status: PieceStatus, progress: u8) -> PieceState {
        PieceState {
            owner_player_id,
            team_id: owner_player_id,
            status,
            progress,
            shield: 0,
            stack_shield: 0,
            motion_serial: 0,
        }
    }

    #[test]
    fn hangar_piece_faces_launch_position() {
        let (board_layout, player_roster) = test_roster();
        let piece_state = piece_state(1, PieceStatus::InHangar, 0);

        assert_eq!(
            next_facing_target(piece_state, &board_layout, &player_roster),
            world_position_for_piece(1, 0, PieceStatus::AtLaunch, &board_layout, &player_roster)
        );
    }

    #[test]
    fn active_piece_faces_next_route_position() {
        let (board_layout, player_roster) = test_roster();
        let piece_state = piece_state(1, PieceStatus::Active, 0);

        assert_eq!(
            next_facing_target(piece_state, &board_layout, &player_roster),
            world_position_for_piece(1, 1, PieceStatus::Active, &board_layout, &player_roster)
        );
    }

    #[test]
    fn finished_piece_keeps_last_rotation() {
        let (board_layout, player_roster) = test_roster();
        let piece_state = piece_state(1, PieceStatus::Finished, FINISH_DISTANCE);

        assert_eq!(
            next_facing_target(piece_state, &board_layout, &player_roster),
            None
        );
    }

    #[test]
    fn piece_effect_labels_cover_attack_and_defense_results() {
        assert_eq!(
            piece_effect_label(PieceEffectKind::Attack, Language::SimplifiedChinese),
            "攻"
        );
        assert_eq!(
            piece_effect_label(PieceEffectKind::Defense, Language::SimplifiedChinese),
            "盾"
        );
        assert_eq!(
            piece_effect_label(PieceEffectKind::Attack, Language::English),
            "ATK"
        );
        assert_eq!(
            piece_effect_label(PieceEffectKind::Defense, Language::English),
            "DEF"
        );
    }

    #[test]
    fn piece_highlight_system_initializes_without_query_conflicts() {
        let (board_layout, player_roster) = test_roster();
        let mut app = App::new();
        app.add_plugins(bevy::state::app::StatesPlugin)
            .init_state::<GamePhase>()
            .insert_resource(board_layout)
            .insert_resource(player_roster)
            .insert_resource(TurnState::opening_turn())
            .insert_resource(TurnInputState::default())
            .insert_resource(SkillTargetState::default())
            .add_systems(Update, update_piece_highlight);

        app.update();
    }

    #[test]
    fn move_target_guide_system_initializes_without_query_conflicts() {
        let (board_layout, player_roster) = test_roster();
        let mut app = App::new();
        app.add_plugins(bevy::state::app::StatesPlugin)
            .add_plugins(bevy::time::TimePlugin)
            .init_state::<GamePhase>()
            .insert_resource(board_layout)
            .insert_resource(player_roster)
            .insert_resource(TurnInputState::default())
            .add_systems(Update, update_move_target_guides);

        app.update();
    }

    #[test]
    fn action_selection_marks_candidate_selectable_and_own_inactive_piece_disabled() {
        let selectable = piece_interaction_visual_state(
            1,
            &piece_state(1, PieceStatus::Active, 0),
            1,
            Some(PlayerControl::Human),
            true,
            &[1],
            false,
            &[],
            false,
        );
        let disabled = piece_interaction_visual_state(
            2,
            &piece_state(1, PieceStatus::InHangar, 0),
            1,
            Some(PlayerControl::Human),
            true,
            &[1],
            false,
            &[],
            false,
        );
        let opponent = piece_interaction_visual_state(
            3,
            &piece_state(2, PieceStatus::Active, 0),
            1,
            Some(PlayerControl::Human),
            true,
            &[1],
            false,
            &[],
            false,
        );

        assert_eq!(selectable, PieceInteractionVisualState::Selectable);
        assert_eq!(disabled, PieceInteractionVisualState::Disabled);
        assert_eq!(opponent, PieceInteractionVisualState::Neutral);
    }

    #[test]
    fn skill_target_selection_disables_non_candidates_temporarily() {
        let target = piece_interaction_visual_state(
            4,
            &piece_state(2, PieceStatus::Active, 0),
            1,
            Some(PlayerControl::Human),
            false,
            &[],
            true,
            &[4],
            true,
        );
        let non_target = piece_interaction_visual_state(
            1,
            &piece_state(1, PieceStatus::Active, 0),
            1,
            Some(PlayerControl::Human),
            false,
            &[],
            true,
            &[4],
            true,
        );

        assert_eq!(target, PieceInteractionVisualState::Selectable);
        assert_eq!(non_target, PieceInteractionVisualState::Disabled);
    }

    #[test]
    fn inactive_selection_context_keeps_pieces_neutral() {
        let state = piece_interaction_visual_state(
            1,
            &piece_state(1, PieceStatus::Active, 0),
            1,
            Some(PlayerControl::Human),
            false,
            &[1],
            false,
            &[1],
            false,
        );

        assert_eq!(state, PieceInteractionVisualState::Neutral);
        assert_eq!(
            visibility_for_piece_state(
                &[(1, PieceInteractionVisualState::Selectable)],
                1,
                PieceInteractionVisualState::Selectable,
            ),
            Visibility::Visible
        );
    }

    fn guide_piece(
        piece_id: u8,
        owner_player_id: u8,
        status: PieceStatus,
        progress: u8,
        origin: Vec2,
    ) -> MoveTargetGuidePiece {
        guide_piece_with_connector_origin(
            piece_id,
            owner_player_id,
            status,
            progress,
            origin,
            origin,
        )
    }

    fn guide_piece_with_connector_origin(
        piece_id: u8,
        owner_player_id: u8,
        status: PieceStatus,
        progress: u8,
        origin: Vec2,
        connector_origin: Vec2,
    ) -> MoveTargetGuidePiece {
        MoveTargetGuidePiece {
            piece_id,
            piece_state: piece_state(owner_player_id, status, progress),
            origin,
            connector_origin,
        }
    }

    #[test]
    fn move_target_guides_map_launch_move_and_goal_targets() {
        let (board_layout, player_roster) = test_roster();
        let pieces = vec![
            guide_piece(1, 1, PieceStatus::InHangar, 0, Vec2::new(-10.0, 0.0)),
            guide_piece(2, 1, PieceStatus::Active, 0, Vec2::new(0.0, 0.0)),
            guide_piece(
                3,
                1,
                PieceStatus::Active,
                FINISH_DISTANCE - 1,
                Vec2::new(10.0, 0.0),
            ),
        ];
        let actions = vec![
            PlannedAction::Launch {
                piece_id: 1,
                target_progress: 0,
            },
            PlannedAction::Move {
                piece_id: 2,
                target_progress: 3,
            },
            PlannedAction::Move {
                piece_id: 3,
                target_progress: FINISH_DISTANCE,
            },
        ];

        let infos = move_target_guide_infos(&actions, &pieces, &board_layout, &player_roster);

        assert_eq!(
            infos
                .iter()
                .find(|info| info.piece_id == 1)
                .map(|info| info.target),
            world_position_for_piece(1, 0, PieceStatus::AtLaunch, &board_layout, &player_roster)
        );
        assert_eq!(
            infos
                .iter()
                .find(|info| info.piece_id == 2)
                .map(|info| info.target),
            world_position_for_piece(1, 3, PieceStatus::Active, &board_layout, &player_roster)
        );
        assert_eq!(
            infos
                .iter()
                .find(|info| info.piece_id == 3)
                .map(|info| info.target),
            world_position_for_piece(
                1,
                FINISH_DISTANCE,
                PieceStatus::Finished,
                &board_layout,
                &player_roster,
            )
        );
    }

    #[test]
    fn overlapping_move_target_guides_are_visually_offset() {
        let (board_layout, player_roster) = test_roster();
        let pieces = vec![
            guide_piece(1, 1, PieceStatus::Active, 0, Vec2::new(-8.0, 0.0)),
            guide_piece(2, 1, PieceStatus::Active, 1, Vec2::new(8.0, 0.0)),
        ];
        let actions = vec![
            PlannedAction::Move {
                piece_id: 1,
                target_progress: 3,
            },
            PlannedAction::Move {
                piece_id: 2,
                target_progress: 3,
            },
        ];

        let infos = move_target_guide_infos(&actions, &pieces, &board_layout, &player_roster);

        assert_eq!(infos.len(), 2);
        assert_eq!(infos[0].target, infos[1].target);
        assert_ne!(infos[0].display_target, infos[1].display_target);
    }

    #[test]
    fn same_origin_and_target_guides_share_one_connector() {
        let (board_layout, player_roster) = test_roster();
        let shared_origin = Vec2::new(12.0, -4.0);
        let pieces = vec![
            guide_piece_with_connector_origin(
                1,
                1,
                PieceStatus::Active,
                0,
                shared_origin + Vec2::new(-6.5, 0.0),
                shared_origin,
            ),
            guide_piece_with_connector_origin(
                2,
                1,
                PieceStatus::Active,
                0,
                shared_origin + Vec2::new(6.5, 0.0),
                shared_origin,
            ),
        ];
        let actions = vec![
            PlannedAction::Move {
                piece_id: 1,
                target_progress: 3,
            },
            PlannedAction::Move {
                piece_id: 2,
                target_progress: 3,
            },
        ];

        let infos = move_target_guide_infos(&actions, &pieces, &board_layout, &player_roster);
        let connector_targets = infos
            .iter()
            .filter_map(|info| info.connector_target)
            .collect::<Vec<_>>();

        assert_eq!(infos.len(), 2);
        assert_eq!(connector_targets.len(), 1);
        assert_eq!(connector_targets[0], infos[0].target);
        assert_ne!(infos[0].display_target, infos[1].display_target);
    }

    #[test]
    fn move_target_guide_pick_hits_nearest_visible_target() {
        let guides = vec![
            MoveTargetGuideInfo {
                piece_id: 1,
                origin: Vec2::ZERO,
                connector_origin: Vec2::ZERO,
                target: Vec2::new(40.0, 0.0),
                display_target: Vec2::new(40.0, 0.0),
                connector_target: Some(Vec2::new(40.0, 0.0)),
            },
            MoveTargetGuideInfo {
                piece_id: 2,
                origin: Vec2::ZERO,
                connector_origin: Vec2::ZERO,
                target: Vec2::new(48.0, 0.0),
                display_target: Vec2::new(48.0, 0.0),
                connector_target: Some(Vec2::new(48.0, 0.0)),
            },
        ];

        assert_eq!(
            pick_move_target_guide(&guides, Vec2::new(46.0, 0.0), 12.0),
            Some(2)
        );
        assert_eq!(
            pick_move_target_guide(&guides, Vec2::new(80.0, 0.0), 8.0),
            None
        );
        assert_eq!(
            pick_move_target_guide(&[], Vec2::new(40.0, 0.0), 12.0),
            None
        );
    }

    #[test]
    fn move_target_guides_follow_refreshed_dash_actions() {
        let (board_layout, player_roster) = test_roster();
        let pieces = vec![guide_piece(
            1,
            1,
            PieceStatus::Active,
            0,
            Vec2::new(-8.0, 0.0),
        )];
        let base_actions = vec![PlannedAction::Move {
            piece_id: 1,
            target_progress: 2,
        }];
        let dash_actions = vec![PlannedAction::Move {
            piece_id: 1,
            target_progress: 5,
        }];

        let base = move_target_guide_infos(&base_actions, &pieces, &board_layout, &player_roster);
        let dash = move_target_guide_infos(&dash_actions, &pieces, &board_layout, &player_roster);

        assert_ne!(base[0].target, dash[0].target);
        assert_eq!(
            dash[0].target,
            world_position_for_piece(1, 5, PieceStatus::Active, &board_layout, &player_roster)
                .expect("dash target exists")
        );
    }

    fn assert_vec2_close(actual: Vec2, expected: Vec2) {
        assert!(
            (actual - expected).length() < 0.001,
            "expected {expected:?}, got {actual:?}"
        );
    }

    fn assert_vec3_close(actual: Vec3, expected: Vec3) {
        assert!(
            (actual - expected).length() < 0.001,
            "expected {expected:?}, got {actual:?}"
        );
    }

    fn piece_visual_infos_from_world(
        world: &mut World,
        board_layout: &BoardLayout,
        player_roster: &PlayerRoster,
    ) -> Vec<PieceVisualInfo> {
        let mut system_state: SystemState<PieceTransformQuery> = SystemState::new(world);
        let query = system_state.get_mut(world).unwrap();
        piece_visual_infos(&query, board_layout, player_roster)
    }

    fn visual_info_for(infos: &[PieceVisualInfo], piece_id: u8) -> PieceVisualInfo {
        *infos
            .iter()
            .find(|info| info.piece_id == piece_id)
            .expect("piece visual info exists")
    }

    #[test]
    fn stack_visual_offsets_spread_overlapped_pieces() {
        assert_vec2_close(stack_visual_offset(0, 1), Vec2::ZERO);

        let left = stack_visual_offset(0, 2);
        let right = stack_visual_offset(1, 2);
        assert!(left.x < 0.0);
        assert!(right.x > 0.0);
        assert_vec2_close(left + right, Vec2::ZERO);

        let third = stack_visual_offset(2, 3);
        assert!(third.y > 0.0);

        let ring = (0..6)
            .map(|index| stack_visual_offset(index, 6))
            .collect::<Vec<_>>();
        assert!(ring.iter().all(|offset| offset.length() > 8.0));
    }

    #[test]
    fn stack_visual_scale_shrinks_clustered_pieces() {
        assert_eq!(stack_visual_scale(1), 1.0);
        assert!(stack_visual_scale(2) < stack_visual_scale(1));
        assert!(stack_visual_scale(5) < stack_visual_scale(4));
    }

    #[test]
    fn in_hangar_pieces_do_not_share_stack_visuals_when_overlapped() {
        let (board_layout, player_roster) = test_roster();
        let mut world = World::new();
        let overlap = Vec3::new(42.0, 24.0, BOARD_Z_LAYER + 1.0);

        for piece_id in [1, 2] {
            world.spawn((
                PieceId(piece_id),
                piece_state(1, PieceStatus::InHangar, 0),
                Transform::from_translation(overlap),
            ));
        }

        let infos = piece_visual_infos_from_world(&mut world, &board_layout, &player_roster);
        for piece_id in [1, 2] {
            let info = visual_info_for(&infos, piece_id);
            assert_vec3_close(info.visual_local_translation, Vec3::ZERO);
            assert_vec3_close(info.visual_translation, overlap);
            assert_eq!(info.visual_scale, 1.0);
            assert_eq!(info.stack_count, 1);
            assert!(info.is_stack_leader);
        }
    }

    #[test]
    fn returning_hangar_piece_does_not_stack_with_overlapped_board_piece() {
        let (board_layout, player_roster) = test_roster();
        let mut world = World::new();
        let overlap =
            world_position_for_piece(1, 0, PieceStatus::Active, &board_layout, &player_roster)
                .expect("active start exists")
                .extend(BOARD_Z_LAYER + 1.0);

        world.spawn((
            PieceId(1),
            piece_state(1, PieceStatus::InHangar, 0),
            Transform::from_translation(overlap),
        ));
        world.spawn((
            PieceId(2),
            piece_state(1, PieceStatus::Active, 0),
            Transform::from_translation(overlap),
        ));

        let infos = piece_visual_infos_from_world(&mut world, &board_layout, &player_roster);
        let hangar_info = visual_info_for(&infos, 1);
        let board_info = visual_info_for(&infos, 2);

        assert_vec3_close(hangar_info.visual_local_translation, Vec3::ZERO);
        assert_eq!(hangar_info.visual_scale, 1.0);
        assert_eq!(hangar_info.stack_count, 1);
        assert_vec3_close(board_info.visual_local_translation, Vec3::ZERO);
        assert_eq!(board_info.visual_scale, 1.0);
        assert_eq!(board_info.stack_count, 1);
    }

    #[test]
    fn active_overlapped_pieces_keep_stack_visuals() {
        let (board_layout, player_roster) = test_roster();
        let mut world = World::new();

        for piece_id in [1, 2] {
            world.spawn((
                PieceId(piece_id),
                piece_state(1, PieceStatus::Active, 0),
                Transform::from_xyz(0.0, 0.0, BOARD_Z_LAYER + 1.0),
            ));
        }

        let infos = piece_visual_infos_from_world(&mut world, &board_layout, &player_roster);
        let first = visual_info_for(&infos, 1);
        let second = visual_info_for(&infos, 2);

        assert_eq!(first.stack_count, 2);
        assert_eq!(second.stack_count, 2);
        assert!(first.is_stack_leader);
        assert!(!second.is_stack_leader);
        assert_eq!(first.visual_scale, stack_visual_scale(2));
        assert_eq!(second.visual_scale, stack_visual_scale(2));
        assert!(first.visual_local_translation.truncate().length() > 0.0);
        assert!(second.visual_local_translation.truncate().length() > 0.0);
    }

    #[test]
    fn stack_visual_local_translation_keeps_world_offset_after_parent_transform() {
        let desired_offset = Vec2::new(6.5, -4.0);
        let parent_rotation = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
        let parent_scale = Vec3::new(1.5, 0.75, 1.0);

        let local =
            stack_visual_local_translation(desired_offset, 0.0, parent_rotation, parent_scale);
        let world_offset = parent_rotation
            .mul_vec3(Vec3::new(
                local.x * parent_scale.x,
                local.y * parent_scale.y,
                local.z * parent_scale.z,
            ))
            .truncate();

        assert_vec2_close(world_offset, desired_offset);
    }

    #[test]
    fn stack_key_groups_tiny_position_jitter() {
        assert_eq!(
            stack_key(Vec2::new(12.003, -4.004)),
            stack_key(Vec2::new(12.004, -4.003))
        );
    }

    #[test]
    fn shield_badge_labels_explain_personal_and_stack_buffs() {
        assert_eq!(
            shield_badge_label(0, 0, None, Language::SimplifiedChinese),
            None
        );
        assert_eq!(
            shield_badge_label(1, 0, None, Language::SimplifiedChinese),
            Some("盾1".to_string())
        );
        assert_eq!(
            shield_badge_label(0, 1, None, Language::SimplifiedChinese),
            Some("队盾1".to_string())
        );
        assert_eq!(
            shield_badge_label(2, 1, Some(3), Language::SimplifiedChinese),
            Some("盾2+队盾1+冲+3".to_string())
        );
        assert_eq!(
            shield_badge_label(2, 1, Some(3), Language::English),
            Some("S2+TS1+D+3".to_string())
        );

        let combined = shield_badge_info(2, 1, Some(3), Language::SimplifiedChinese)
            .expect("combined badge is visible");
        assert_eq!(combined.label, "盾2+队盾1+冲+3");
        assert!(combined.size.x > SHIELD_BADGE_MIN_SIZE.x);
    }

    #[test]
    fn movement_buff_badge_marks_current_player_active_pieces() {
        let mut turn_state = TurnState::opening_turn();
        turn_state.current_player = 1;
        let input_state = TurnInputState::default();
        let skill_roster = SkillRoster {
            players: vec![crate::gameplay::skill_flow::PlayerSkillState {
                player_id: 1,
                dash_charges: 0,
                dash_armed: true,
                snipe_charges: 0,
                swap_charges: 0,
                shield_charges: 0,
                double_dice_charges: 0,
                double_dice_armed: false,
                skip_next_skill_turn: false,
                skill_blocked_this_turn: false,
            }],
            finish_bounce_counts: [0; 4],
            last_skill_action: None,
            last_skill_action_player_id: None,
            last_skill_action_turn_index: 0,
            last_skill_action_serial: 0,
            active_turn_player: Some(1),
            skill_used_this_turn: true,
        };
        let own_piece = piece_state(1, PieceStatus::Active, 0);
        let enemy_piece = piece_state(2, PieceStatus::Active, 0);

        assert_eq!(
            movement_buff_bonus_for_piece(1, &own_piece, &turn_state, &input_state, &skill_roster),
            Some(3)
        );
        assert_eq!(
            movement_buff_bonus_for_piece(
                2,
                &enemy_piece,
                &turn_state,
                &input_state,
                &skill_roster
            ),
            None
        );
        assert_eq!(
            shield_badge_info(0, 0, Some(3), Language::SimplifiedChinese)
                .map(|badge| badge.label)
                .as_deref(),
            Some("冲+3")
        );
        assert_eq!(
            shield_badge_info(0, 0, Some(3), Language::English)
                .map(|badge| badge.label)
                .as_deref(),
            Some("D+3")
        );
    }
}
