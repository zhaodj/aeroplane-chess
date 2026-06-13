use bevy::{
    asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology,
};
use std::f32::consts::PI;

use crate::constants::BOARD_Z_LAYER;
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
struct DrawStyle {
    fill: Color,
    border: Color,
    border_width: f32,
    z: f32,
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

#[derive(Clone, Copy)]
/// 棋盘上的 SVG 线性图标（飞机、箭头、星形等）使用原始填色键映射玩家色。
struct SvgIcon {
    center: Vec2,
    fill: &'static str,
    rotation: f32,
}

#[derive(Clone, Copy)]
/// 双箭头/方向提示图标。
struct ChevronIcon {
    center: Vec2,
    fill: &'static str,
    direction: Vec2,
    count: u8,
    size: f32,
}

#[derive(Clone, Copy)]
struct DirectionIconDraw {
    center: Vec2,
    direction: Vec2,
    color: Color,
    z: f32,
}

#[derive(Clone, Copy)]
struct ChevronDraw {
    center: Vec2,
    direction: Vec2,
    count: u8,
    size: f32,
    color: Color,
    z: f32,
}

#[derive(Clone, Copy)]
struct StarDraw {
    center: Vec2,
    radius: f32,
    color: Color,
    z: f32,
}

#[derive(Clone)]
/// 棋盘四色槽位：SVG 里的四个固定色只用于定位到 P1~P4。
struct BoardPalette {
    player_colors: [Color; 4],
}

impl BoardPalette {
    fn from_player_roster(player_roster: &PlayerRoster) -> Self {
        Self {
            player_colors: player_roster.player_colors,
        }
    }

    fn player_color(&self, player_id: u8) -> Color {
        let player_index = player_id.saturating_sub(1) as usize;
        self.player_colors
            .get(player_index)
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
        self.player_colors[route_index as usize % self.player_colors.len()]
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
        Vec2::splat(683.0),
        DrawStyle {
            fill: Color::srgb(245.0 / 255.0, 245.0 / 255.0, 245.0 / 255.0),
            border: Color::srgb(245.0 / 255.0, 245.0 / 255.0, 245.0 / 255.0),
            border_width: 0.0,
            z: BOARD_Z_LAYER - 3.0,
        },
        "BoardBackdrop",
    );

    // 先画矩形网格，再画三角区域，保证层次与 SVG 基本一致。
    for rect in SVG_RECTS {
        spawn_square_with_border(
            &mut commands,
            rect.center,
            rect.size,
            DrawStyle {
                fill: board_palette.color_for_svg_fill(rect.fill),
                border: Color::BLACK,
                border_width: 1.0,
                z: BOARD_Z_LAYER - 1.0,
            },
            "SvgRect",
        );
    }

    for tri in SVG_TRIANGLES {
        spawn_triangle_with_border(
            &mut commands,
            &mut meshes,
            &mut materials,
            [tri.a, tri.b, tri.c],
            DrawStyle {
                fill: board_palette.color_for_svg_fill(tri.fill),
                border: Color::BLACK,
                border_width: 1.0,
                z: BOARD_Z_LAYER - 0.9,
            },
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
            [launch.a, launch.b, launch.c],
            DrawStyle {
                fill: launch_color,
                border: Color::BLACK,
                border_width: 1.0,
                z: BOARD_Z_LAYER - 0.75,
            },
            format!("LaunchTriangle_P{}", launch.player_id),
        );
        spawn_circle_with_border(
            &mut commands,
            &mut meshes,
            &mut materials,
            launch.center,
            16.0,
            DrawStyle {
                fill: Color::WHITE,
                border: Color::WHITE,
                border_width: 0.0,
                z: BOARD_Z_LAYER + 0.08,
            },
            format!("LaunchDot_P{}", launch.player_id),
        );
        spawn_arrow_icon(
            &mut commands,
            &mut meshes,
            &mut materials,
            DirectionIconDraw {
                center: launch.center,
                direction: launch.arrow_direction,
                color: launch_color,
                z: BOARD_Z_LAYER + 0.55,
            },
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
            DrawStyle {
                fill: Color::WHITE,
                border: Color::WHITE,
                border_width: 0.0,
                z: BOARD_Z_LAYER + 0.05,
            },
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
                DrawStyle {
                    fill: Color::WHITE,
                    border: Color::WHITE,
                    border_width: 0.0,
                    z: BOARD_Z_LAYER + 0.05,
                },
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
                DrawStyle {
                    fill: Color::WHITE,
                    border: Color::BLACK,
                    border_width: 1.0,
                    z: BOARD_Z_LAYER + 0.20,
                },
                "HangarPad",
            );
        }
    }
    for icon in HANGAR_PLANE_ICONS {
        spawn_plane_icon(
            &mut commands,
            icon.center,
            icon.rotation,
            1.0,
            board_palette.color_for_svg_fill(icon.fill),
            BOARD_Z_LAYER + 0.56,
            "HangarPlane",
        );
    }

    // 中心四向目标点。
    for icon in CENTER_STAR_ICONS {
        spawn_circle_with_border(
            &mut commands,
            &mut meshes,
            &mut materials,
            icon.center,
            17.0,
            DrawStyle {
                fill: Color::WHITE,
                border: Color::WHITE,
                border_width: 0.0,
                z: BOARD_Z_LAYER + 0.30,
            },
            "CenterNode",
        );
        spawn_star_icon(
            &mut commands,
            &mut meshes,
            &mut materials,
            StarDraw {
                center: icon.center,
                radius: 13.0,
                color: board_palette.color_for_svg_fill(icon.fill),
                z: BOARD_Z_LAYER + 0.55,
            },
            "CenterStar",
        );
    }

    // SVG 里的方向提示属于棋盘底图，而不是临时玩法调试标记。
    for icon in CHEVRON_ICONS {
        spawn_chevron_icon(
            &mut commands,
            ChevronDraw {
                center: icon.center,
                direction: icon.direction,
                count: icon.count,
                size: icon.size,
                color: board_palette.color_for_svg_fill(icon.fill),
                z: BOARD_Z_LAYER + 0.58,
            },
            "BoardChevron",
        );
    }
}

fn cleanup_board(mut commands: Commands, query: Query<Entity, With<BoardSceneEntity>>) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
}

/// 绘制带描边方块。
fn spawn_square_with_border(
    commands: &mut Commands,
    center: Vec2,
    size: Vec2,
    style: DrawStyle,
    name: impl Into<String>,
) {
    let name = name.into();
    commands.spawn((
        Sprite::from_color(style.border, size + Vec2::splat(style.border_width * 2.0)),
        Transform::from_xyz(center.x, center.y, style.z),
        Name::new(format!("{name}_border")),
        BoardSceneEntity,
    ));
    commands.spawn((
        Sprite::from_color(style.fill, size),
        Transform::from_xyz(center.x, center.y, style.z + 0.01),
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
    style: DrawStyle,
    name: impl Into<String>,
) {
    let name = name.into();
    commands.spawn((
        Mesh2d(meshes.add(Circle::new(radius + style.border_width))),
        MeshMaterial2d(materials.add(ColorMaterial::from(style.border))),
        Transform::from_xyz(center.x, center.y, style.z),
        Name::new(format!("{name}_border")),
        BoardSceneEntity,
    ));
    commands.spawn((
        Mesh2d(meshes.add(Circle::new(radius))),
        MeshMaterial2d(materials.add(ColorMaterial::from(style.fill))),
        Transform::from_xyz(center.x, center.y, style.z + 0.01),
        Name::new(name),
        BoardSceneEntity,
    ));
}

/// 绘制带描边三角形。
fn spawn_triangle_with_border(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    points: [Vec2; 3],
    style: DrawStyle,
    name: impl Into<String>,
) {
    let name = name.into();
    let [a, b, c] = points;
    let centroid = (a + b + c) / 3.0;
    let outer_a = a - centroid;
    let outer_b = b - centroid;
    let outer_c = c - centroid;

    commands.spawn((
        Mesh2d(meshes.add(Triangle2d::new(outer_a, outer_b, outer_c))),
        MeshMaterial2d(materials.add(ColorMaterial::from(style.border))),
        Transform::from_xyz(centroid.x, centroid.y, style.z),
        Name::new(format!("{name}_border")),
        BoardSceneEntity,
    ));

    let max_radius = outer_a
        .length()
        .max(outer_b.length())
        .max(outer_c.length())
        .max(1.0);
    let inset_scale = ((max_radius - style.border_width) / max_radius).clamp(0.72, 0.985);

    commands.spawn((
        Mesh2d(meshes.add(Triangle2d::new(
            outer_a * inset_scale,
            outer_b * inset_scale,
            outer_c * inset_scale,
        ))),
        MeshMaterial2d(materials.add(ColorMaterial::from(style.fill))),
        Transform::from_xyz(centroid.x, centroid.y, style.z + 0.01),
        Name::new(name),
        BoardSceneEntity,
    ));
}

/// 绘制方向箭头图标。
fn spawn_arrow_icon(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    icon: DirectionIconDraw,
    name: impl Into<String>,
) {
    let name = name.into();
    let direction = icon.direction.normalize_or_zero();
    let angle = direction.y.atan2(direction.x);
    let tail = icon.center - direction * 4.0;
    let head = icon.center + direction * 7.0;
    let perp = Vec2::new(-direction.y, direction.x);

    commands.spawn((
        Sprite::from_color(icon.color, Vec2::new(15.0, 3.0)),
        Transform {
            translation: Vec3::new(tail.x, tail.y, icon.z),
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
        [
            head + direction * 4.0,
            head - direction * 5.5 + perp * 5.0,
            head - direction * 5.5 - perp * 5.0,
        ],
        DrawStyle {
            fill: icon.color,
            border: icon.color,
            border_width: 0.0,
            z: icon.z + 0.01,
        },
        format!("{name}_head"),
    );
}

/// 绘制机库里的飞机线稿。
fn spawn_plane_icon(
    commands: &mut Commands,
    center: Vec2,
    rotation: f32,
    scale: f32,
    color: Color,
    z: f32,
    name: impl Into<String>,
) {
    let name = name.into();
    for (index, segment) in PLANE_ICON_POINTS.windows(2).enumerate() {
        let start = center + rotate_vec(segment[0] * scale, rotation);
        let end = center + rotate_vec(segment[1] * scale, rotation);
        spawn_line_segment(
            commands,
            start,
            end,
            2.0 * scale,
            color,
            z + index as f32 * 0.001,
            format!("{name}_{index}"),
        );
    }
}

/// 绘制 SVG 中的单/双 chevron 方向提示。
fn spawn_chevron_icon(commands: &mut Commands, icon: ChevronDraw, name: impl Into<String>) {
    let name = name.into();
    let direction = icon.direction.normalize_or_zero();
    let perp = Vec2::new(-direction.y, direction.x);
    let spacing = icon.size * 0.58;
    let first_offset = -((icon.count.saturating_sub(1)) as f32) * spacing * 0.5;

    for index in 0..icon.count {
        let base = icon.center + direction * (first_offset + index as f32 * spacing);
        let tip = base + direction * icon.size * 0.45;
        let back = base - direction * icon.size * 0.35;
        let wing_a = back + perp * icon.size * 0.42;
        let wing_b = back - perp * icon.size * 0.42;
        spawn_line_segment(
            commands,
            wing_a,
            tip,
            3.0,
            icon.color,
            icon.z + index as f32 * 0.002,
            format!("{name}_{index}_a"),
        );
        spawn_line_segment(
            commands,
            wing_b,
            tip,
            3.0,
            icon.color,
            icon.z + index as f32 * 0.002 + 0.001,
            format!("{name}_{index}_b"),
        );
    }
}

/// 绘制中心终点星形。
fn spawn_star_icon(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    icon: StarDraw,
    name: impl Into<String>,
) {
    commands.spawn((
        Mesh2d(meshes.add(star_mesh(icon.radius, icon.radius * 0.48))),
        MeshMaterial2d(materials.add(ColorMaterial::from(icon.color))),
        Transform::from_xyz(icon.center.x, icon.center.y, icon.z),
        Name::new(name.into()),
        BoardSceneEntity,
    ));
}

fn star_mesh(outer_radius: f32, inner_radius: f32) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    let mut positions = vec![[0.0, 0.0, 0.0]];

    for index in 0..10 {
        let angle = PI * 0.5 + index as f32 * PI / 5.0;
        let radius = if index % 2 == 0 {
            outer_radius
        } else {
            inner_radius
        };
        positions.push([angle.cos() * radius, angle.sin() * radius, 0.0]);
    }

    let mut indices = Vec::with_capacity(30);
    for index in 1..=10 {
        let next = if index == 10 { 1 } else { index + 1 };
        indices.extend_from_slice(&[0, index as u32, next as u32]);
    }

    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn spawn_line_segment(
    commands: &mut Commands,
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
    commands.spawn((
        Sprite::from_color(color, Vec2::new(length, thickness)),
        Transform {
            translation: Vec3::new(center.x, center.y, z),
            rotation: Quat::from_rotation_z(delta.y.atan2(delta.x)),
            ..default()
        },
        Name::new(name.into()),
        BoardSceneEntity,
    ));
}

fn rotate_vec(vec: Vec2, angle: f32) -> Vec2 {
    let (sin, cos) = angle.sin_cos();
    Vec2::new(vec.x * cos - vec.y * sin, vec.x * sin + vec.y * cos)
}

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

const HANGAR_PLANE_ICONS: &[SvgIcon] = &[
    SvgIcon {
        center: Vec2::new(-295.104, 295.104),
        fill: "#0080FF",
        rotation: 0.0,
    },
    SvgIcon {
        center: Vec2::new(-225.104, 295.104),
        fill: "#0080FF",
        rotation: 0.0,
    },
    SvgIcon {
        center: Vec2::new(-295.104, 225.104),
        fill: "#0080FF",
        rotation: 0.0,
    },
    SvgIcon {
        center: Vec2::new(-225.104, 225.104),
        fill: "#0080FF",
        rotation: 0.0,
    },
    SvgIcon {
        center: Vec2::new(295.317, 295.104),
        fill: "#FF0000",
        rotation: -PI * 0.5,
    },
    SvgIcon {
        center: Vec2::new(225.317, 295.104),
        fill: "#FF0000",
        rotation: -PI * 0.5,
    },
    SvgIcon {
        center: Vec2::new(295.317, 225.104),
        fill: "#FF0000",
        rotation: -PI * 0.5,
    },
    SvgIcon {
        center: Vec2::new(225.317, 225.104),
        fill: "#FF0000",
        rotation: -PI * 0.5,
    },
    SvgIcon {
        center: Vec2::new(-295.104, -295.104),
        fill: "#008000",
        rotation: PI * 0.5,
    },
    SvgIcon {
        center: Vec2::new(-225.104, -295.104),
        fill: "#008000",
        rotation: PI * 0.5,
    },
    SvgIcon {
        center: Vec2::new(-295.104, -225.104),
        fill: "#008000",
        rotation: PI * 0.5,
    },
    SvgIcon {
        center: Vec2::new(-225.104, -225.104),
        fill: "#008000",
        rotation: PI * 0.5,
    },
    SvgIcon {
        center: Vec2::new(295.104, -295.104),
        fill: "#F3D849",
        rotation: PI,
    },
    SvgIcon {
        center: Vec2::new(225.104, -295.104),
        fill: "#F3D849",
        rotation: PI,
    },
    SvgIcon {
        center: Vec2::new(295.104, -225.104),
        fill: "#F3D849",
        rotation: PI,
    },
    SvgIcon {
        center: Vec2::new(225.104, -225.104),
        fill: "#F3D849",
        rotation: PI,
    },
];

const CENTER_STAR_ICONS: &[SvgIcon] = &[
    SvgIcon {
        center: Vec2::new(0.0, 35.958),
        fill: "#FF0000",
        rotation: 0.0,
    },
    SvgIcon {
        center: Vec2::new(-35.958, 0.0),
        fill: "#0080FF",
        rotation: 0.0,
    },
    SvgIcon {
        center: Vec2::new(35.959, 0.0),
        fill: "#F3D849",
        rotation: 0.0,
    },
    SvgIcon {
        center: Vec2::new(0.0, -35.958),
        fill: "#008000",
        rotation: 0.0,
    },
];

const CHEVRON_ICONS: &[ChevronIcon] = &[
    ChevronIcon {
        center: Vec2::new(-156.104, 124.104),
        fill: "#F3D849",
        direction: Vec2::Y,
        count: 2,
        size: 16.0,
    },
    ChevronIcon {
        center: Vec2::new(-124.104, 156.104),
        fill: "#008000",
        direction: Vec2::X,
        count: 2,
        size: 16.0,
    },
    ChevronIcon {
        center: Vec2::new(124.317, 156.104),
        fill: "#008000",
        direction: Vec2::X,
        count: 2,
        size: 16.0,
    },
    ChevronIcon {
        center: Vec2::new(156.317, 124.104),
        fill: "#0080FF",
        direction: Vec2::NEG_Y,
        count: 2,
        size: 16.0,
    },
    ChevronIcon {
        center: Vec2::new(156.104, -124.104),
        fill: "#0080FF",
        direction: Vec2::NEG_Y,
        count: 2,
        size: 16.0,
    },
    ChevronIcon {
        center: Vec2::new(124.104, -156.104),
        fill: "#FF0000",
        direction: Vec2::NEG_X,
        count: 2,
        size: 16.0,
    },
    ChevronIcon {
        center: Vec2::new(-124.104, -156.104),
        fill: "#FF0000",
        direction: Vec2::NEG_X,
        count: 2,
        size: 16.0,
    },
    ChevronIcon {
        center: Vec2::new(-156.104, -124.104),
        fill: "#F3D849",
        direction: Vec2::Y,
        count: 2,
        size: 16.0,
    },
    ChevronIcon {
        center: Vec2::new(-60.104, 156.104),
        fill: "#008000",
        direction: Vec2::X,
        count: 2,
        size: 18.0,
    },
    ChevronIcon {
        center: Vec2::new(59.896, 156.104),
        fill: "#008000",
        direction: Vec2::X,
        count: 2,
        size: 18.0,
    },
    ChevronIcon {
        center: Vec2::new(155.896, 60.104),
        fill: "#0080FF",
        direction: Vec2::NEG_Y,
        count: 2,
        size: 18.0,
    },
    ChevronIcon {
        center: Vec2::new(155.896, -59.896),
        fill: "#0080FF",
        direction: Vec2::NEG_Y,
        count: 2,
        size: 18.0,
    },
    ChevronIcon {
        center: Vec2::new(59.896, -155.896),
        fill: "#FF0000",
        direction: Vec2::NEG_X,
        count: 2,
        size: 18.0,
    },
    ChevronIcon {
        center: Vec2::new(-60.104, -155.896),
        fill: "#FF0000",
        direction: Vec2::NEG_X,
        count: 2,
        size: 18.0,
    },
    ChevronIcon {
        center: Vec2::new(-156.104, -57.896),
        fill: "#F3D849",
        direction: Vec2::Y,
        count: 2,
        size: 18.0,
    },
    ChevronIcon {
        center: Vec2::new(-156.104, 62.104),
        fill: "#F3D849",
        direction: Vec2::Y,
        count: 2,
        size: 18.0,
    },
];

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
    fn route_colors_keep_full_four_color_palette_in_one_vs_one() {
        let red = Color::srgb(1.0, 0.0, 0.0);
        let blue = Color::srgb(0.0, 128.0 / 255.0, 1.0);
        let green = Color::srgb(0.0, 128.0 / 255.0, 0.0);
        let yellow = Color::srgb(243.0 / 255.0, 216.0 / 255.0, 73.0 / 255.0);
        let palette = BoardPalette::from_player_roster(&PlayerRoster {
            players: vec![player(1, red), player(2, blue)],
            player_colors: [red, blue, green, yellow],
        });

        assert_eq!(palette.color_for_route_index(0), red);
        assert_eq!(palette.color_for_route_index(1), blue);
        assert_eq!(palette.color_for_route_index(2), green);
        assert_eq!(palette.color_for_route_index(3), yellow);
    }

    #[test]
    fn inactive_svg_slots_keep_configured_palette_colors() {
        let red = Color::srgb(1.0, 0.0, 0.0);
        let blue = Color::srgb(0.0, 128.0 / 255.0, 1.0);
        let green = Color::srgb(0.0, 128.0 / 255.0, 0.0);
        let yellow = Color::srgb(243.0 / 255.0, 216.0 / 255.0, 73.0 / 255.0);
        let palette = BoardPalette::from_player_roster(&PlayerRoster {
            players: vec![player(1, red), player(2, blue)],
            player_colors: [red, blue, green, yellow],
        });

        assert_eq!(palette.color_for_svg_fill("#008000"), green);
        assert_eq!(palette.color_for_svg_fill("#F3D849"), yellow);
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
