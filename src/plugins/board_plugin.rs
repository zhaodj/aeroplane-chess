use bevy::prelude::*;

use crate::constants::BOARD_Z_LAYER;
use crate::domain::tile::TileKind;
use crate::gameplay::match_flow::{BoardLayout, PlayerRoster};
use crate::states::AppState;

/// 棋盘渲染插件：按 SVG 的几何元素重建棋盘外观。
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

#[derive(Clone, Copy)]
/// SVG 复刻矩形图元。
struct SvgRect {
    center: Vec2,
    size: Vec2,
    fill: &'static str,
}

#[derive(Clone, Copy)]
/// SVG 复刻三角图元。
struct SvgTriangle {
    a: Vec2,
    b: Vec2,
    c: Vec2,
    fill: &'static str,
}

#[derive(Clone, Copy)]
/// 机场旁起飞点三角图元。
struct LaunchTriangle {
    player_id: u8,
    center: Vec2,
    a: Vec2,
    b: Vec2,
    c: Vec2,
    arrow_direction: Vec2,
}

#[derive(Clone)]
/// 棋盘四色槽位：SVG 里的四个固定色只用于定位到 P1~P4。
struct BoardPalette {
    player_colors: [Color; 4],
    active_player_colors: Vec<Color>,
}

impl BoardPalette {
    fn from_player_roster(player_roster: &PlayerRoster) -> Self {
        let mut player_colors = [
            Color::srgb(0.0, 0.50, 1.0),
            Color::srgb(1.0, 0.0, 0.0),
            Color::srgb(0.0, 0.50, 0.0),
            Color::srgb(0.95, 0.85, 0.29),
        ];

        let mut active_player_colors = Vec::with_capacity(player_roster.players.len());
        for player in &player_roster.players {
            active_player_colors.push(player.color);
            let index = player.state.player_id.saturating_sub(1) as usize;
            if let Some(slot) = player_colors.get_mut(index) {
                *slot = player.color;
            }
        }

        Self {
            player_colors,
            active_player_colors,
        }
    }

    fn player_color(&self, player_id: u8) -> Color {
        let player_index = player_id.saturating_sub(1) as usize;
        if player_index < self.active_player_colors.len() {
            return self.player_colors[player_index];
        }

        self.active_player_colors
            .get(player_index % self.active_player_colors.len().max(1))
            .copied()
            .unwrap_or(Color::srgb(0.90, 0.90, 0.90))
    }

    fn color_for_svg_fill(&self, fill: &str) -> Color {
        match fill {
            "#0080FF" => self.player_color(1),
            "#FF0000" => self.player_color(2),
            "#008000" => self.player_color(3),
            "#F3D849" => self.player_color(4),
            "#F5F5F5" | "white" => Color::srgb(0.96, 0.96, 0.96),
            "black" => Color::BLACK,
            _ => Color::srgb(0.90, 0.90, 0.90),
        }
    }

    fn color_for_route_index(&self, route_index: u8) -> Color {
        self.active_player_colors
            .get(route_index as usize % self.active_player_colors.len().max(1))
            .copied()
            .unwrap_or(Color::srgb(0.90, 0.90, 0.90))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::player::{PlayerControl, PlayerState};
    use crate::gameplay::match_flow::PlayerProfile;

    fn player(player_id: u8, color: Color) -> PlayerProfile {
        PlayerProfile {
            state: PlayerState {
                player_id,
                team_id: player_id,
                control: PlayerControl::Human,
            },
            color,
            hangar_slots: Vec::new(),
            launch_position: Vec2::ZERO,
            launch_tile_index: 0,
            home_lane_positions: Vec::new(),
            goal_position: Vec2::ZERO,
        }
    }

    #[test]
    fn route_colors_cycle_by_active_player_order() {
        let red = Color::srgb(1.0, 0.0, 0.0);
        let blue = Color::srgb(0.0, 0.0, 1.0);
        let palette = BoardPalette::from_player_roster(&PlayerRoster {
            players: vec![player(1, red), player(2, blue)],
        });

        assert_eq!(palette.color_for_route_index(0), red);
        assert_eq!(palette.color_for_route_index(1), blue);
        assert_eq!(palette.color_for_route_index(2), red);
        assert_eq!(palette.color_for_route_index(3), blue);
    }

    #[test]
    fn inactive_svg_slots_reuse_active_player_cycle() {
        let red = Color::srgb(1.0, 0.0, 0.0);
        let blue = Color::srgb(0.0, 0.0, 1.0);
        let palette = BoardPalette::from_player_roster(&PlayerRoster {
            players: vec![player(1, red), player(2, blue)],
        });

        assert_eq!(palette.color_for_svg_fill("#008000"), red);
        assert_eq!(palette.color_for_svg_fill("#F3D849"), blue);
    }

    #[test]
    fn launch_triangles_use_corner_consistent_right_angles() {
        for triangle in LAUNCH_TRIANGLES {
            assert!(
                (triangle.a.x - triangle.b.x).abs() < 0.001
                    || (triangle.a.y - triangle.b.y).abs() < 0.001
            );
            assert!(
                (triangle.a.x - triangle.c.x).abs() < 0.001
                    || (triangle.a.y - triangle.c.y).abs() < 0.001
            );
        }
    }
}

fn spawn_board(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    board_layout: Res<BoardLayout>,
    player_roster: Res<PlayerRoster>,
) {
    let board_palette = BoardPalette::from_player_roster(&player_roster);

    spawn_square_with_border(
        &mut commands,
        Vec2::ZERO,
        Vec2::splat(690.0),
        Color::srgb(0.96, 0.96, 0.93),
        Color::srgb(0.16, 0.16, 0.16),
        3.0,
        BOARD_Z_LAYER - 3.0,
        "BoardBackdrop",
    );

    // 先画矩形网格，再画三角区域，保证层次与 SVG 基本一致。
    for rect in SVG_RECTS {
        spawn_square_with_border(
            &mut commands,
            rect.center,
            rect.size,
            board_palette.color_for_svg_fill(rect.fill),
            Color::BLACK,
            1.8,
            BOARD_Z_LAYER - 1.0,
            "SvgRect",
        );
    }

    for tri in SVG_TRIANGLES {
        spawn_triangle_with_border(
            &mut commands,
            &mut meshes,
            &mut materials,
            tri.a,
            tri.b,
            tri.c,
            board_palette.color_for_svg_fill(tri.fill),
            Color::BLACK,
            1.8,
            BOARD_Z_LAYER - 0.9,
            "SvgTri",
        );
    }

    // 起飞点三角：背景与箭头统一绑定机场/玩家颜色。
    for launch in LAUNCH_TRIANGLES {
        let launch_color = board_palette.player_color(launch.player_id);
        spawn_triangle_with_border(
            &mut commands,
            &mut meshes,
            &mut materials,
            launch.a,
            launch.b,
            launch.c,
            launch_color,
            Color::BLACK,
            1.8,
            BOARD_Z_LAYER - 0.75,
            format!("LaunchTriangle_P{}", launch.player_id),
        );
        spawn_circle_with_border(
            &mut commands,
            &mut meshes,
            &mut materials,
            launch.center,
            16.0,
            Color::WHITE,
            Color::srgb(0.48, 0.48, 0.48),
            1.2,
            BOARD_Z_LAYER + 0.08,
            format!("LaunchDot_P{}", launch.player_id),
        );
        spawn_arrow_icon(
            &mut commands,
            &mut meshes,
            &mut materials,
            launch.center,
            launch.arrow_direction,
            launch_color,
            BOARD_Z_LAYER + 0.55,
            format!("LaunchArrow_P{}", launch.player_id),
        );
    }

    // 主环道圆点：严格按逻辑路径坐标绘制，保证棋子与圆心对齐。
    for tile in &board_layout.tiles {
        spawn_circle_with_border(
            &mut commands,
            &mut meshes,
            &mut materials,
            tile.world_pos,
            16.0,
            Color::WHITE,
            Color::srgb(0.48, 0.48, 0.48),
            1.2,
            BOARD_Z_LAYER + 0.05,
            "TrackDot",
        );
    }

    // 冲线支路圆点：同样按逻辑坐标绘制，避免出现“一格双圆圈”。
    for player in &player_roster.players {
        for &lane_pos in &player.home_lane_positions {
            spawn_circle_with_border(
                &mut commands,
                &mut meshes,
                &mut materials,
                lane_pos,
                16.0,
                Color::WHITE,
                Color::srgb(0.48, 0.48, 0.48),
                1.2,
                BOARD_Z_LAYER + 0.05,
                "HomeLaneDot",
            );
        }
    }

    // 固定机库圆槽（始终展示 4 槽，棋子数量少时仅部分被占用）。
    for airport_center in [
        Vec2::new(-260.0, 260.0),
        Vec2::new(260.0, 260.0),
        Vec2::new(-260.0, -260.0),
        Vec2::new(260.0, -260.0),
    ] {
        for offset in [
            Vec2::new(-35.0, 35.0),
            Vec2::new(35.0, 35.0),
            Vec2::new(-35.0, -35.0),
            Vec2::new(35.0, -35.0),
        ] {
            spawn_circle_with_border(
                &mut commands,
                &mut meshes,
                &mut materials,
                airport_center + offset,
                24.5,
                Color::WHITE,
                Color::BLACK,
                2.0,
                BOARD_Z_LAYER + 0.20,
                "HangarPad",
            );
        }
    }

    // 中心四向目标点。
    for pos in [
        Vec2::new(-36.0, 0.0),
        Vec2::new(0.0, 36.0),
        Vec2::new(36.0, 0.0),
        Vec2::new(0.0, -36.0),
    ] {
        spawn_circle_with_border(
            &mut commands,
            &mut meshes,
            &mut materials,
            pos,
            17.0,
            Color::WHITE,
            Color::BLACK,
            2.0,
            BOARD_Z_LAYER + 0.30,
            "CenterNode",
        );
        spawn_plus_icon(
            &mut commands,
            pos,
            12.0,
            Color::BLACK,
            BOARD_Z_LAYER + 0.34,
            "CenterNodeGlyph",
        );
    }

    // 特殊格标记继续按逻辑格子叠加，保证玩法可读性。
    for tile in &board_layout.tiles {
        if let Some(route_index) = tile.route_index {
            spawn_tile_marker(
                &mut commands,
                &mut meshes,
                &mut materials,
                tile.world_pos,
                tile.kind,
                route_index,
                &board_palette,
            );
        }
    }
}

fn cleanup_board(mut commands: Commands, query: Query<Entity, With<BoardSceneEntity>>) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
}

/// 根据格子类型叠加玩法标记。
fn spawn_tile_marker(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    pos: Vec2,
    kind: TileKind,
    route_index: u8,
    board_palette: &BoardPalette,
) {
    let marker_z = BOARD_Z_LAYER + 0.50;
    match kind {
        TileKind::Normal | TileKind::Goal => {}
        TileKind::Attack => {
            spawn_plus_icon(
                commands,
                pos,
                10.0,
                Color::BLACK,
                marker_z,
                format!("Attack_{route_index}"),
            );
        }
        TileKind::Defense => {
            spawn_plus_icon(
                commands,
                pos,
                9.0,
                Color::srgb(0.10, 0.12, 0.15),
                marker_z,
                format!("Defense_{route_index}"),
            );
        }
        TileKind::Event => {
            spawn_circle_with_border(
                commands,
                meshes,
                materials,
                pos,
                8.4,
                Color::WHITE,
                Color::srgb(0.93, 0.22, 0.35),
                2.8,
                marker_z,
                format!("EventRing_{route_index}"),
            );
            commands.spawn((
                Sprite::from_color(Color::srgb(0.20, 0.78, 0.96), Vec2::new(9.0, 3.0)),
                Transform::from_xyz(pos.x, pos.y, marker_z + 0.01),
                Name::new(format!("EventCore_{route_index}")),
                BoardSceneEntity,
            ));
        }
        TileKind::Jump => {
            let marker_color = board_palette.color_for_route_index(route_index);
            let marker_border = marker_color.mix(&Color::BLACK, 0.55);
            let direction = jump_arrow_direction(route_index);
            let tail = pos - direction * 5.0;
            let head = pos + direction * 8.5;
            let angle = direction.y.atan2(direction.x);

            commands.spawn((
                Sprite::from_color(marker_color, Vec2::new(10.0, 3.6)),
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
                head - direction * 7.5 + perp * 4.0,
                head - direction * 7.5 - perp * 4.0,
                marker_color,
                marker_border,
                1.0,
                marker_z + 0.01,
                format!("JumpHead_{route_index}"),
            );
        }
    }
}

/// 绘制带描边方块。
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

/// 绘制带描边圆形。
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

/// 绘制带描边三角形。
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
    let inset_scale = ((max_radius - border_width) / max_radius).clamp(0.72, 0.985);

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

/// 绘制十字图标。
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
        Sprite::from_color(color, Vec2::new(size, 3.2)),
        Transform::from_xyz(center.x, center.y, z),
        Name::new(format!("{name}_h")),
        BoardSceneEntity,
    ));
    commands.spawn((
        Sprite::from_color(color, Vec2::new(3.2, size)),
        Transform::from_xyz(center.x, center.y, z + 0.01),
        Name::new(format!("{name}_v")),
        BoardSceneEntity,
    ));
}

/// 绘制方向箭头图标。
fn spawn_arrow_icon(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    center: Vec2,
    direction: Vec2,
    color: Color,
    z: f32,
    name: impl Into<String>,
) {
    let name = name.into();
    let direction = direction.normalize_or_zero();
    let angle = direction.y.atan2(direction.x);
    let tail = center - direction * 4.0;
    let head = center + direction * 7.0;
    let perp = Vec2::new(-direction.y, direction.x);

    commands.spawn((
        Sprite::from_color(color, Vec2::new(15.0, 3.0)),
        Transform {
            translation: Vec3::new(tail.x, tail.y, z),
            rotation: Quat::from_rotation_z(angle),
            ..default()
        },
        Name::new(format!("{name}_shaft")),
        BoardSceneEntity,
    ));

    spawn_triangle_with_border(
        commands,
        meshes,
        materials,
        head + direction * 4.0,
        head - direction * 5.5 + perp * 5.0,
        head - direction * 5.5 - perp * 5.0,
        color,
        color,
        0.0,
        z + 0.01,
        format!("{name}_head"),
    );
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

const LAUNCH_TRIANGLES: &[LaunchTriangle] = &[
    LaunchTriangle {
        player_id: 1,
        center: Vec2::new(-316.104, 156.104),
        a: Vec2::new(-340.104, 180.104),
        b: Vec2::new(-260.104, 180.104),
        c: Vec2::new(-340.104, 100.104),
        arrow_direction: Vec2::new(1.0, 0.0),
    },
    LaunchTriangle {
        player_id: 2,
        center: Vec2::new(155.896, 316.104),
        a: Vec2::new(180.317, 340.104),
        b: Vec2::new(100.317, 340.104),
        c: Vec2::new(180.317, 260.104),
        arrow_direction: Vec2::new(0.0, -1.0),
    },
    LaunchTriangle {
        player_id: 3,
        center: Vec2::new(-156.104, -315.896),
        a: Vec2::new(-180.104, -340.104),
        b: Vec2::new(-100.104, -340.104),
        c: Vec2::new(-180.104, -260.104),
        arrow_direction: Vec2::new(0.0, 1.0),
    },
    LaunchTriangle {
        player_id: 4,
        center: Vec2::new(315.896, -155.896),
        a: Vec2::new(340.104, -180.104),
        b: Vec2::new(260.104, -180.104),
        c: Vec2::new(340.104, -100.104),
        arrow_direction: Vec2::new(-1.0, 0.0),
    },
];

const SVG_RECTS: &[SvgRect] = &[
    SvgRect {
        center: Vec2::new(120.317, 0.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#F3D849",
    },
    SvgRect {
        center: Vec2::new(-0.104, 240.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#FF0000",
    },
    SvgRect {
        center: Vec2::new(-0.104, 200.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#FF0000",
    },
    SvgRect {
        center: Vec2::new(-0.104, 160.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#FF0000",
    },
    SvgRect {
        center: Vec2::new(-0.104, 120.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#FF0000",
    },
    SvgRect {
        center: Vec2::new(-0.104, 80.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#FF0000",
    },
    SvgRect {
        center: Vec2::new(200.317, 0.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#F3D849",
    },
    SvgRect {
        center: Vec2::new(240.317, 0.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#F3D849",
    },
    SvgRect {
        center: Vec2::new(-200.104, -0.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#0080FF",
    },
    SvgRect {
        center: Vec2::new(-160.104, -0.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#0080FF",
    },
    SvgRect {
        center: Vec2::new(-120.104, -0.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#0080FF",
    },
    SvgRect {
        center: Vec2::new(-80.104, -0.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#0080FF",
    },
    SvgRect {
        center: Vec2::new(-240.104, -0.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#0080FF",
    },
    SvgRect {
        center: Vec2::new(0.104, -80.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#008000",
    },
    SvgRect {
        center: Vec2::new(0.104, -120.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#008000",
    },
    SvgRect {
        center: Vec2::new(0.104, -160.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#008000",
    },
    SvgRect {
        center: Vec2::new(0.104, -200.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#008000",
    },
    SvgRect {
        center: Vec2::new(0.104, -240.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#008000",
    },
    SvgRect {
        center: Vec2::new(80.317, 0.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#F3D849",
    },
    SvgRect {
        center: Vec2::new(160.317, 0.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#F3D849",
    },
    SvgRect {
        center: Vec2::new(300.104, -80.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#0080FF",
    },
    SvgRect {
        center: Vec2::new(-300.104, -80.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#F3D849",
    },
    SvgRect {
        center: Vec2::new(-240.104, -140.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#0080FF",
    },
    SvgRect {
        center: Vec2::new(-200.104, -140.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#008000",
    },
    SvgRect {
        center: Vec2::new(140.104, -200.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#F3D849",
    },
    SvgRect {
        center: Vec2::new(140.104, -240.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#008000",
    },
    SvgRect {
        center: Vec2::new(-140.104, 200.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#0080FF",
    },
    SvgRect {
        center: Vec2::new(-80.104, 300.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#008000",
    },
    SvgRect {
        center: Vec2::new(-40.104, 300.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#0080FF",
    },
    SvgRect {
        center: Vec2::new(-0.104, 300.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#FF0000",
    },
    SvgRect {
        center: Vec2::new(40.317, 300.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#F3D849",
    },
    SvgRect {
        center: Vec2::new(80.317, 300.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#008000",
    },
    SvgRect {
        center: Vec2::new(140.317, 240.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#FF0000",
    },
    SvgRect {
        center: Vec2::new(-140.104, 240.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#FF0000",
    },
    SvgRect {
        center: Vec2::new(140.317, 200.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#F3D849",
    },
    SvgRect {
        center: Vec2::new(-240.104, 140.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#0080FF",
    },
    SvgRect {
        center: Vec2::new(-200.104, 140.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#FF0000",
    },
    SvgRect {
        center: Vec2::new(-300.104, 80.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#F3D849",
    },
    SvgRect {
        center: Vec2::new(300.317, 80.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#0080FF",
    },
    SvgRect {
        center: Vec2::new(-300.104, 40.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#FF0000",
    },
    SvgRect {
        center: Vec2::new(300.317, 40.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#FF0000",
    },
    SvgRect {
        center: Vec2::new(300.317, 0.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#F3D849",
    },
    SvgRect {
        center: Vec2::new(-300.104, -0.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#0080FF",
    },
    SvgRect {
        center: Vec2::new(300.104, -40.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#008000",
    },
    SvgRect {
        center: Vec2::new(200.104, -140.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#008000",
    },
    SvgRect {
        center: Vec2::new(240.104, -140.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#F3D849",
    },
    SvgRect {
        center: Vec2::new(-140.104, -200.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#0080FF",
    },
    SvgRect {
        center: Vec2::new(-140.104, -240.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#008000",
    },
    SvgRect {
        center: Vec2::new(-80.104, -300.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#FF0000",
    },
    SvgRect {
        center: Vec2::new(-40.104, -300.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#0080FF",
    },
    SvgRect {
        center: Vec2::new(240.317, 140.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#F3D849",
    },
    SvgRect {
        center: Vec2::new(200.317, 140.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#FF0000",
    },
    SvgRect {
        center: Vec2::new(-300.104, -40.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#008000",
    },
    SvgRect {
        center: Vec2::new(0.104, -300.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#008000",
    },
    SvgRect {
        center: Vec2::new(40.104, -300.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#F3D849",
    },
    SvgRect {
        center: Vec2::new(80.104, -300.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#FF0000",
    },
    SvgRect {
        center: Vec2::new(-260.104, 260.104),
        size: Vec2::new(160.0, 160.0),
        fill: "#0080FF",
    },
    SvgRect {
        center: Vec2::new(260.317, 260.104),
        size: Vec2::new(160.0, 160.0),
        fill: "#FF0000",
    },
    SvgRect {
        center: Vec2::new(-260.104, -260.104),
        size: Vec2::new(160.0, 160.0),
        fill: "#008000",
    },
    SvgRect {
        center: Vec2::new(260.104, -260.104),
        size: Vec2::new(160.0, 160.0),
        fill: "#F3D849",
    },
];

const SVG_TRIANGLES: &[SvgTriangle] = &[
    SvgTriangle {
        a: Vec2::new(-340.104, 180.104),
        b: Vec2::new(-260.104, 180.104),
        c: Vec2::new(-340.104, 100.104),
        fill: "white",
    },
    SvgTriangle {
        a: Vec2::new(-260.104, 100.104),
        b: Vec2::new(-340.104, 100.104),
        c: Vec2::new(-260.104, 180.104),
        fill: "#008000",
    },
    SvgTriangle {
        a: Vec2::new(-180.104, 100.104),
        b: Vec2::new(-180.104, 180.104),
        c: Vec2::new(-100.104, 100.104),
        fill: "#F3D849",
    },
    SvgTriangle {
        a: Vec2::new(-100.104, 180.104),
        b: Vec2::new(-100.104, 100.104),
        c: Vec2::new(-180.104, 180.104),
        fill: "#008000",
    },
    SvgTriangle {
        a: Vec2::new(-100.104, 260.104),
        b: Vec2::new(-180.104, 260.104),
        c: Vec2::new(-100.104, 340.104),
        fill: "#F3D849",
    },
    SvgTriangle {
        a: Vec2::new(0.419, -0.104),
        b: Vec2::new(-59.685, 60.0),
        c: Vec2::new(60.523, 60.0),
        fill: "#FF0000",
    },
    SvgTriangle {
        a: Vec2::new(340.104, -180.104),
        b: Vec2::new(260.104, -180.104),
        c: Vec2::new(340.104, -100.104),
        fill: "white",
    },
    SvgTriangle {
        a: Vec2::new(260.104, -100.104),
        b: Vec2::new(340.104, -100.104),
        c: Vec2::new(260.104, -180.104),
        fill: "#FF0000",
    },
    SvgTriangle {
        a: Vec2::new(180.104, -100.104),
        b: Vec2::new(180.104, -180.104),
        c: Vec2::new(100.104, -100.104),
        fill: "#0080FF",
    },
    SvgTriangle {
        a: Vec2::new(100.104, -180.104),
        b: Vec2::new(100.104, -100.104),
        c: Vec2::new(180.104, -180.104),
        fill: "#FF0000",
    },
    SvgTriangle {
        a: Vec2::new(100.104, -260.104),
        b: Vec2::new(180.104, -260.104),
        c: Vec2::new(100.104, -340.104),
        fill: "#0080FF",
    },
    SvgTriangle {
        a: Vec2::new(-0.419, 0.104),
        b: Vec2::new(59.685, -60.0),
        c: Vec2::new(-60.523, -60.0),
        fill: "#008000",
    },
    SvgTriangle {
        a: Vec2::new(-180.104, -340.104),
        b: Vec2::new(-180.104, -260.104),
        c: Vec2::new(-100.104, -340.104),
        fill: "white",
    },
    SvgTriangle {
        a: Vec2::new(-100.104, -260.104),
        b: Vec2::new(-100.104, -340.104),
        c: Vec2::new(-180.104, -260.104),
        fill: "#F3D849",
    },
    SvgTriangle {
        a: Vec2::new(-100.104, -180.104),
        b: Vec2::new(-180.104, -180.104),
        c: Vec2::new(-100.104, -100.104),
        fill: "#FF0000",
    },
    SvgTriangle {
        a: Vec2::new(-180.104, -100.104),
        b: Vec2::new(-100.104, -100.104),
        c: Vec2::new(-180.104, -180.104),
        fill: "#F3D849",
    },
    SvgTriangle {
        a: Vec2::new(-260.104, -100.104),
        b: Vec2::new(-260.104, -180.104),
        c: Vec2::new(-340.104, -100.104),
        fill: "#FF0000",
    },
    SvgTriangle {
        a: Vec2::new(0.104, 0.419),
        b: Vec2::new(-60.0, -59.685),
        c: Vec2::new(-60.0, 60.523),
        fill: "#0080FF",
    },
    SvgTriangle {
        a: Vec2::new(180.317, 340.104),
        b: Vec2::new(180.317, 260.104),
        c: Vec2::new(100.317, 340.104),
        fill: "white",
    },
    SvgTriangle {
        a: Vec2::new(100.317, 260.104),
        b: Vec2::new(100.317, 340.104),
        c: Vec2::new(180.317, 260.104),
        fill: "#0080FF",
    },
    SvgTriangle {
        a: Vec2::new(100.317, 180.104),
        b: Vec2::new(180.317, 180.104),
        c: Vec2::new(100.318, 100.104),
        fill: "#008000",
    },
    SvgTriangle {
        a: Vec2::new(180.317, 100.104),
        b: Vec2::new(100.317, 100.104),
        c: Vec2::new(180.317, 180.104),
        fill: "#0080FF",
    },
    SvgTriangle {
        a: Vec2::new(260.317, 100.104),
        b: Vec2::new(260.317, 180.104),
        c: Vec2::new(340.317, 100.104),
        fill: "#008000",
    },
    SvgTriangle {
        a: Vec2::new(-0.104, 0.0),
        b: Vec2::new(60.0, 60.104),
        c: Vec2::new(60.0, -60.104),
        fill: "#F3D849",
    },
];
