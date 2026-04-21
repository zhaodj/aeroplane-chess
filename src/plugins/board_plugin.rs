use bevy::prelude::*;

use crate::constants::BOARD_Z_LAYER;
use crate::domain::tile::TileKind;
use crate::gameplay::match_flow::{BoardLayout, PlayerRoster};
use crate::states::AppState;

/// 棋盘渲染插件：按 SVG 设计风格分层绘制棋盘图元。
pub struct BoardPlugin;

impl Plugin for BoardPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_board)
            .add_systems(OnExit(AppState::InGame), cleanup_board);
    }
}

#[derive(Component)]
/// 棋盘场景实体标记，用于状态切换时统一清理。
struct BoardSceneEntity;

/// 主环道格子的边长（像素）。
const TRACK_TILE_SIZE: f32 = 60.0;
/// 主环道白色圆点半径。
const TRACK_DOT_RADIUS: f32 = 16.0;
/// 机场色块边长。
const AIRPORT_SIZE: f32 = 248.0;
/// 棋盘底板尺寸。
const BOARD_BACKDROP_SIZE: f32 = 980.0;

fn spawn_board(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    board_layout: Res<BoardLayout>,
    player_roster: Res<PlayerRoster>,
) {
    spawn_board_backdrop(&mut commands);
    spawn_center_goal_zone(&mut commands, &mut meshes, &mut materials);
    spawn_guide_arrows(&mut commands, &mut meshes, &mut materials);
    spawn_airports(&mut commands, &mut meshes, &mut materials, &player_roster);

    // 主环道：彩色底格 + 白圆点 + 特殊格标记。
    for tile in &board_layout.tiles {
        let route_index = tile.route_index.unwrap_or_default();
        let color = route_band_color(route_index);
        spawn_square_with_border(
            &mut commands,
            tile.world_pos,
            Vec2::splat(TRACK_TILE_SIZE),
            color,
            Color::BLACK,
            2.0,
            BOARD_Z_LAYER,
            format!("{}_tile", tile.id),
        );

        spawn_circle_with_border(
            &mut commands,
            &mut meshes,
            &mut materials,
            tile.world_pos,
            TRACK_DOT_RADIUS,
            Color::WHITE,
            Color::srgb(0.36, 0.39, 0.42),
            1.6,
            BOARD_Z_LAYER + 0.08,
            format!("{}_dot", tile.id),
        );

        spawn_tile_marker(
            &mut commands,
            &mut meshes,
            &mut materials,
            tile.world_pos,
            tile.kind,
            route_index,
        );
    }

    // 冲线道与终点：按玩家序号固定颜色渲染，贴近 SVG 的视觉分区。
    for player in &player_roster.players {
        let lane_color = board_color_for_player(player.state.player_id);
        for (lane_index, lane_pos) in player.home_lane_positions.iter().enumerate() {
            spawn_square_with_border(
                &mut commands,
                *lane_pos,
                Vec2::splat(TRACK_TILE_SIZE),
                lane_color,
                Color::BLACK,
                2.0,
                BOARD_Z_LAYER + 0.01,
                format!("HomeLane_P{}_{}", player.state.player_id, lane_index),
            );

            spawn_circle_with_border(
                &mut commands,
                &mut meshes,
                &mut materials,
                *lane_pos,
                TRACK_DOT_RADIUS,
                Color::WHITE,
                Color::srgb(0.36, 0.39, 0.42),
                1.6,
                BOARD_Z_LAYER + 0.10,
                format!("HomeLaneDot_P{}_{}", player.state.player_id, lane_index),
            );
        }

        spawn_circle_with_border(
            &mut commands,
            &mut meshes,
            &mut materials,
            player.goal_position,
            18.0,
            Color::WHITE,
            Color::BLACK,
            2.2,
            BOARD_Z_LAYER + 0.20,
            format!("Goal_P{}", player.state.player_id),
        );

        spawn_plus_icon(
            &mut commands,
            player.goal_position,
            14.0,
            Color::BLACK,
            BOARD_Z_LAYER + 0.24,
            format!("GoalCore_P{}", player.state.player_id),
        );
    }
}

fn cleanup_board(mut commands: Commands, query: Query<Entity, With<BoardSceneEntity>>) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
}

/// 绘制棋盘背景底板。
fn spawn_board_backdrop(commands: &mut Commands) {
    spawn_square_with_border(
        commands,
        Vec2::ZERO,
        Vec2::splat(BOARD_BACKDROP_SIZE),
        Color::srgb(0.93, 0.93, 0.87),
        Color::srgb(0.16, 0.16, 0.16),
        3.0,
        BOARD_Z_LAYER - 2.0,
        "BoardBackdrop",
    );
}

/// 绘制四角机场与机库圆槽。
fn spawn_airports(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    player_roster: &PlayerRoster,
) {
    for player in &player_roster.players {
        let base_color = board_color_for_player(player.state.player_id);
        let airport_center = average_position(&player.hangar_slots);

        spawn_square_with_border(
            commands,
            airport_center,
            Vec2::splat(AIRPORT_SIZE),
            base_color,
            Color::BLACK,
            2.5,
            BOARD_Z_LAYER - 0.60,
            format!("Airport_P{}", player.state.player_id),
        );

        for (slot_index, &slot_pos) in player.hangar_slots.iter().enumerate() {
            spawn_circle_with_border(
                commands,
                meshes,
                materials,
                slot_pos,
                25.0,
                Color::WHITE,
                Color::BLACK,
                2.2,
                BOARD_Z_LAYER + 0.30,
                format!("HangarPad_P{}_{}", player.state.player_id, slot_index),
            );
            spawn_plus_icon(
                commands,
                slot_pos,
                11.0,
                base_color,
                BOARD_Z_LAYER + 0.34,
                format!("HangarGlyph_P{}_{}", player.state.player_id, slot_index),
            );
        }

        spawn_airport_gate(
            commands,
            meshes,
            materials,
            airport_center,
            base_color,
            player.state.player_id,
        );
    }
}

/// 机场连接口（双色三角）用于接近 SVG 中的角落引导块。
fn spawn_airport_gate(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    airport_center: Vec2,
    base_color: Color,
    player_id: u8,
) {
    let toward_center = Vec2::new(-airport_center.x.signum(), -airport_center.y.signum());
    let gate_center = airport_center + Vec2::new(toward_center.x * 124.0, toward_center.y * 124.0);

    spawn_square_with_border(
        commands,
        gate_center,
        Vec2::splat(80.0),
        Color::srgb(0.93, 0.93, 0.87),
        Color::BLACK,
        2.0,
        BOARD_Z_LAYER - 0.15,
        format!("AirportGate_P{player_id}"),
    );

    let half = 38.0;
    let a = gate_center + Vec2::new(-half, -half);
    let b = gate_center + Vec2::new(half, -half);
    let c = gate_center + Vec2::new(half, half);
    let d = gate_center + Vec2::new(-half, half);

    spawn_triangle_with_border(
        commands,
        meshes,
        materials,
        a,
        c,
        d,
        base_color,
        Color::BLACK,
        1.8,
        BOARD_Z_LAYER - 0.11,
        format!("AirportGateMain_P{player_id}"),
    );

    spawn_triangle_with_border(
        commands,
        meshes,
        materials,
        a,
        b,
        c,
        Color::srgb(0.93, 0.93, 0.87),
        Color::BLACK,
        1.8,
        BOARD_Z_LAYER - 0.10,
        format!("AirportGateSub_P{player_id}"),
    );

    spawn_circle_with_border(
        commands,
        meshes,
        materials,
        gate_center,
        14.0,
        Color::WHITE,
        Color::BLACK,
        1.8,
        BOARD_Z_LAYER + 0.28,
        format!("AirportGateDot_P{player_id}"),
    );
}

/// 绘制中心四向终点三角区。
fn spawn_center_goal_zone(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
) {
    let half = 64.0;

    // 上（红）
    spawn_triangle_with_border(
        commands,
        meshes,
        materials,
        Vec2::new(-half, 0.0),
        Vec2::new(half, 0.0),
        Vec2::new(0.0, half),
        board_color_for_player(2),
        Color::BLACK,
        2.2,
        BOARD_Z_LAYER - 0.40,
        "CenterRed",
    );

    // 右（黄）
    spawn_triangle_with_border(
        commands,
        meshes,
        materials,
        Vec2::new(0.0, half),
        Vec2::new(0.0, -half),
        Vec2::new(half, 0.0),
        board_color_for_player(4),
        Color::BLACK,
        2.2,
        BOARD_Z_LAYER - 0.39,
        "CenterYellow",
    );

    // 下（绿）
    spawn_triangle_with_border(
        commands,
        meshes,
        materials,
        Vec2::new(-half, 0.0),
        Vec2::new(0.0, -half),
        Vec2::new(half, 0.0),
        board_color_for_player(3),
        Color::BLACK,
        2.2,
        BOARD_Z_LAYER - 0.38,
        "CenterGreen",
    );

    // 左（蓝）
    spawn_triangle_with_border(
        commands,
        meshes,
        materials,
        Vec2::new(-half, 0.0),
        Vec2::new(0.0, half),
        Vec2::new(0.0, -half),
        board_color_for_player(1),
        Color::BLACK,
        2.2,
        BOARD_Z_LAYER - 0.37,
        "CenterBlue",
    );

    for (name, pos) in [
        ("CenterNodeTop", Vec2::new(0.0, 35.0)),
        ("CenterNodeRight", Vec2::new(35.0, 0.0)),
        ("CenterNodeBottom", Vec2::new(0.0, -35.0)),
        ("CenterNodeLeft", Vec2::new(-35.0, 0.0)),
    ] {
        spawn_circle_with_border(
            commands,
            meshes,
            materials,
            pos,
            16.0,
            Color::WHITE,
            Color::BLACK,
            2.0,
            BOARD_Z_LAYER + 0.16,
            name,
        );

        spawn_plus_icon(
            commands,
            pos,
            12.0,
            Color::BLACK,
            BOARD_Z_LAYER + 0.20,
            format!("{name}_glyph"),
        );
    }
}

/// 绘制四条指向中心的虚线箭头。
fn spawn_guide_arrows(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
) {
    spawn_dashed_arrow(
        commands,
        meshes,
        materials,
        Vec2::new(0.0, 216.0),
        Vec2::new(0.0, 96.0),
        board_color_for_player(2),
        BOARD_Z_LAYER - 0.05,
        "GuideTop",
    );
    spawn_dashed_arrow(
        commands,
        meshes,
        materials,
        Vec2::new(216.0, 0.0),
        Vec2::new(96.0, 0.0),
        board_color_for_player(4),
        BOARD_Z_LAYER - 0.05,
        "GuideRight",
    );
    spawn_dashed_arrow(
        commands,
        meshes,
        materials,
        Vec2::new(0.0, -216.0),
        Vec2::new(0.0, -96.0),
        board_color_for_player(3),
        BOARD_Z_LAYER - 0.05,
        "GuideBottom",
    );
    spawn_dashed_arrow(
        commands,
        meshes,
        materials,
        Vec2::new(-216.0, 0.0),
        Vec2::new(-96.0, 0.0),
        board_color_for_player(1),
        BOARD_Z_LAYER - 0.05,
        "GuideLeft",
    );
}

/// 根据格子类型叠加符号图标。
fn spawn_tile_marker(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    pos: Vec2,
    kind: TileKind,
    route_index: u8,
) {
    let marker_z = BOARD_Z_LAYER + 0.22;
    match kind {
        TileKind::Normal | TileKind::Goal => {}
        TileKind::Attack => {
            spawn_plus_icon(
                commands,
                pos,
                12.5,
                Color::BLACK,
                marker_z,
                format!("AttackMarker_{route_index}"),
            );
        }
        TileKind::Defense => {
            spawn_plus_icon(
                commands,
                pos,
                10.5,
                Color::srgb(0.08, 0.10, 0.14),
                marker_z,
                format!("DefenseMarker_{route_index}"),
            );
            commands.spawn((
                Sprite::from_color(Color::srgb(0.08, 0.10, 0.14), Vec2::new(16.0, 2.8)),
                Transform {
                    translation: Vec3::new(pos.x, pos.y, marker_z + 0.01),
                    rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_4),
                    ..default()
                },
                Name::new(format!("DefenseMarkerDiag_{route_index}")),
                BoardSceneEntity,
            ));
        }
        TileKind::Event => {
            spawn_circle_with_border(
                commands,
                meshes,
                materials,
                pos,
                9.0,
                Color::WHITE,
                Color::srgb(0.93, 0.22, 0.35),
                3.0,
                marker_z,
                format!("EventRing_{route_index}"),
            );
            commands.spawn((
                Sprite::from_color(Color::srgb(0.18, 0.76, 0.94), Vec2::new(10.0, 3.0)),
                Transform::from_xyz(pos.x, pos.y, marker_z + 0.01),
                Name::new(format!("EventCore_{route_index}")),
                BoardSceneEntity,
            ));
        }
        TileKind::Jump => {
            let direction = jump_arrow_direction(route_index);
            let tail = pos - direction * 6.0;
            let head = pos + direction * 10.0;
            let angle = direction.y.atan2(direction.x);

            commands.spawn((
                Sprite::from_color(Color::srgb(0.16, 0.55, 0.95), Vec2::new(12.0, 4.0)),
                Transform {
                    translation: Vec3::new(tail.x, tail.y, marker_z),
                    rotation: Quat::from_rotation_z(angle),
                    ..default()
                },
                Name::new(format!("JumpTail_{route_index}")),
                BoardSceneEntity,
            ));

            let perp = Vec2::new(-direction.y, direction.x);
            spawn_triangle_with_border(
                commands,
                meshes,
                materials,
                head,
                head - direction * 8.0 + perp * 4.5,
                head - direction * 8.0 - perp * 4.5,
                Color::srgb(0.16, 0.55, 0.95),
                Color::srgb(0.05, 0.17, 0.31),
                1.2,
                marker_z + 0.01,
                format!("JumpHead_{route_index}"),
            );
        }
    }
}

/// 绘制带描边的方形图元。
fn spawn_square_with_border(
    commands: &mut Commands,
    center: Vec2,
    size: Vec2,
    fill: Color,
    border: Color,
    border_width: f32,
    z: f32,
    name: impl Into<String>,
) {
    let name = name.into();
    commands.spawn((
        Sprite::from_color(border, size + Vec2::splat(border_width * 2.0)),
        Transform::from_xyz(center.x, center.y, z),
        Name::new(format!("{name}_border")),
        BoardSceneEntity,
    ));

    commands.spawn((
        Sprite::from_color(fill, size),
        Transform::from_xyz(center.x, center.y, z + 0.01),
        Name::new(name),
        BoardSceneEntity,
    ));
}

/// 绘制带描边的圆形图元。
fn spawn_circle_with_border(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    center: Vec2,
    radius: f32,
    fill: Color,
    border: Color,
    border_width: f32,
    z: f32,
    name: impl Into<String>,
) {
    let name = name.into();

    commands.spawn((
        Mesh2d(meshes.add(Circle::new(radius + border_width))),
        MeshMaterial2d(materials.add(ColorMaterial::from(border))),
        Transform::from_xyz(center.x, center.y, z),
        Name::new(format!("{name}_border")),
        BoardSceneEntity,
    ));

    commands.spawn((
        Mesh2d(meshes.add(Circle::new(radius))),
        MeshMaterial2d(materials.add(ColorMaterial::from(fill))),
        Transform::from_xyz(center.x, center.y, z + 0.01),
        Name::new(name),
        BoardSceneEntity,
    ));
}

/// 绘制带描边三角形，常用于中心区和箭头。
fn spawn_triangle_with_border(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    a: Vec2,
    b: Vec2,
    c: Vec2,
    fill: Color,
    border: Color,
    border_width: f32,
    z: f32,
    name: impl Into<String>,
) {
    let name = name.into();
    let centroid = (a + b + c) / 3.0;

    let outer_a = a - centroid;
    let outer_b = b - centroid;
    let outer_c = c - centroid;

    commands.spawn((
        Mesh2d(meshes.add(Triangle2d::new(outer_a, outer_b, outer_c))),
        MeshMaterial2d(materials.add(ColorMaterial::from(border))),
        Transform::from_xyz(centroid.x, centroid.y, z),
        Name::new(format!("{name}_border")),
        BoardSceneEntity,
    ));

    let max_radius = outer_a
        .length()
        .max(outer_b.length())
        .max(outer_c.length())
        .max(1.0);
    let inset_scale = ((max_radius - border_width) / max_radius).clamp(0.72, 0.98);

    commands.spawn((
        Mesh2d(meshes.add(Triangle2d::new(
            outer_a * inset_scale,
            outer_b * inset_scale,
            outer_c * inset_scale,
        ))),
        MeshMaterial2d(materials.add(ColorMaterial::from(fill))),
        Transform::from_xyz(centroid.x, centroid.y, z + 0.01),
        Name::new(name),
        BoardSceneEntity,
    ));
}

/// 绘制“十字”图标（用于星位与重点提示）。
fn spawn_plus_icon(
    commands: &mut Commands,
    center: Vec2,
    size: f32,
    color: Color,
    z: f32,
    name: impl Into<String>,
) {
    let name = name.into();
    commands.spawn((
        Sprite::from_color(color, Vec2::new(size, 3.6)),
        Transform::from_xyz(center.x, center.y, z),
        Name::new(format!("{name}_h")),
        BoardSceneEntity,
    ));
    commands.spawn((
        Sprite::from_color(color, Vec2::new(3.6, size)),
        Transform::from_xyz(center.x, center.y, z + 0.01),
        Name::new(format!("{name}_v")),
        BoardSceneEntity,
    ));
}

/// 画虚线箭头（用于冲线引导）。
fn spawn_dashed_arrow(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    start: Vec2,
    end: Vec2,
    color: Color,
    z: f32,
    name: impl Into<String>,
) {
    let name = name.into();
    let delta = end - start;
    if delta.length() < 0.1 {
        return;
    }

    let direction = delta.normalize();
    let angle = direction.y.atan2(direction.x);
    let total_len = delta.length();
    let dash_len = 12.0;
    let gap_len = 8.0;
    let head_len = 16.0;

    let mut traveled = 0.0;
    let mut index = 0;
    while traveled < total_len - head_len {
        let seg_len = (total_len - head_len - traveled).min(dash_len);
        let center = start + direction * (traveled + seg_len * 0.5);
        commands.spawn((
            Sprite::from_color(color.with_alpha(0.90), Vec2::new(seg_len, 5.0)),
            Transform {
                translation: Vec3::new(center.x, center.y, z),
                rotation: Quat::from_rotation_z(angle),
                ..default()
            },
            Name::new(format!("{name}_dash_{index}")),
            BoardSceneEntity,
        ));

        index += 1;
        traveled += dash_len + gap_len;
    }

    let tip = end;
    let base_center = end - direction * head_len;
    let perp = Vec2::new(-direction.y, direction.x);
    spawn_triangle_with_border(
        commands,
        meshes,
        materials,
        tip,
        base_center + perp * 6.0,
        base_center - perp * 6.0,
        color,
        Color::BLACK,
        1.2,
        z + 0.02,
        format!("{name}_head"),
    );
}

/// 计算机库槽位平均坐标，用于推导机场色块中心。
fn average_position(points: &[Vec2]) -> Vec2 {
    if points.is_empty() {
        return Vec2::ZERO;
    }
    let sum = points
        .iter()
        .copied()
        .fold(Vec2::ZERO, |acc, point| acc + point);
    sum / points.len() as f32
}

/// 将玩家编号映射为固定棋盘主题色（蓝/红/绿/黄）。
fn board_color_for_player(player_id: u8) -> Color {
    match player_id {
        1 => Color::srgb(0.18, 0.50, 0.91),
        2 => Color::srgb(0.91, 0.22, 0.14),
        3 => Color::srgb(0.12, 0.67, 0.39),
        4 => Color::srgb(0.93, 0.82, 0.16),
        _ => Color::srgb(0.84, 0.89, 0.96),
    }
}

/// 主环道四色带配色（按索引循环）。
fn route_band_color(route_index: u8) -> Color {
    match route_index % 4 {
        0 => board_color_for_player(3),
        1 => board_color_for_player(1),
        2 => board_color_for_player(2),
        _ => board_color_for_player(4),
    }
}

/// 跳跃格箭头朝向（按四象限顺时针）。
fn jump_arrow_direction(route_index: u8) -> Vec2 {
    match route_index {
        5 => Vec2::new(1.0, 0.0),
        18 => Vec2::new(0.0, -1.0),
        28 => Vec2::new(-1.0, 0.0),
        39 => Vec2::new(0.0, 1.0),
        _ => Vec2::new(1.0, 0.0),
    }
}
