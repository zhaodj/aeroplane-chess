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

fn spawn_board(
    mut commands: Commands,
    board_layout: Res<BoardLayout>,
    player_roster: Res<PlayerRoster>,
) {
    // 绘制棋盘底板与固定主题区域（机场、中心终点区）。
    commands.spawn((
        Sprite::from_color(Color::srgb(0.84, 0.89, 0.96), Vec2::new(980.0, 980.0)),
        Transform::from_xyz(0.0, 0.0, BOARD_Z_LAYER - 1.0),
        Name::new("BoardBackdrop"),
        BoardSceneEntity,
    ));

    spawn_airport_blocks(&mut commands);
    spawn_center_goal_zone(&mut commands);

    // 绘制主环道格子：先底色，再白色圆点，最后叠加特殊格标记。
    for tile in &board_layout.tiles {
        let route_index = tile.route_index.unwrap_or_default();
        commands.spawn((
            Sprite::from_color(
                route_band_color(route_index).with_alpha(0.96),
                Vec2::splat(BOARD_TILE_SIZE),
            ),
            Transform::from_xyz(tile.world_pos.x, tile.world_pos.y, BOARD_Z_LAYER),
            Name::new(tile.id.clone()),
            BoardSceneEntity,
        ));
        commands.spawn((
            Sprite::from_color(
                Color::srgb(0.95, 0.96, 0.98),
                Vec2::splat(BOARD_TILE_SIZE * 0.58),
            ),
            Transform::from_xyz(tile.world_pos.x, tile.world_pos.y, BOARD_Z_LAYER + 0.05),
            Name::new(format!("{}_dot", tile.id)),
            BoardSceneEntity,
        ));
        spawn_tile_marker(&mut commands, tile.world_pos, tile.kind, route_index);
    }

    // 绘制各玩家冲线道与终点格。
    for player in &player_roster.players {
        let lane_color = board_color_for_player(player.state.player_id);
        for lane_pos in &player.home_lane_positions {
            commands.spawn((
                Sprite::from_color(
                    lane_color.with_alpha(0.55),
                    Vec2::splat(BOARD_TILE_SIZE * 0.82),
                ),
                Transform::from_xyz(lane_pos.x, lane_pos.y, BOARD_Z_LAYER),
                Name::new(format!("HomeLane_P{}", player.state.player_id)),
                BoardSceneEntity,
            ));
            commands.spawn((
                Sprite::from_color(
                    Color::srgb(0.96, 0.97, 0.99),
                    Vec2::splat(BOARD_TILE_SIZE * 0.46),
                ),
                Transform::from_xyz(lane_pos.x, lane_pos.y, BOARD_Z_LAYER + 0.04),
                Name::new(format!("HomeLaneDot_P{}", player.state.player_id)),
                BoardSceneEntity,
            ));
        }

        commands.spawn((
            Sprite::from_color(
                lane_color.with_alpha(0.9),
                Vec2::splat(BOARD_TILE_SIZE * 0.9),
            ),
            Transform::from_xyz(
                player.goal_position.x,
                player.goal_position.y,
                BOARD_Z_LAYER,
            ),
            Name::new(format!("Goal_P{}", player.state.player_id)),
            BoardSceneEntity,
        ));
        commands.spawn((
            Sprite::from_color(
                Color::srgb(0.96, 0.97, 0.99),
                Vec2::splat(BOARD_TILE_SIZE * 0.52),
            ),
            Transform::from_xyz(
                player.goal_position.x,
                player.goal_position.y,
                BOARD_Z_LAYER + 0.04,
            ),
            Name::new(format!("GoalDot_P{}", player.state.player_id)),
            BoardSceneEntity,
        ));
        commands.spawn((
            Sprite::from_color(Color::BLACK, Vec2::splat(BOARD_TILE_SIZE * 0.20)),
            Transform::from_xyz(
                player.goal_position.x,
                player.goal_position.y,
                BOARD_Z_LAYER + 0.08,
            ),
            Name::new(format!("GoalCore_P{}", player.state.player_id)),
            BoardSceneEntity,
        ));
    }

    // 绘制机库停机位底座。
    for player in &player_roster.players {
        for hangar_slot in &player.hangar_slots {
            commands.spawn((
                Sprite::from_color(
                    Color::srgb(0.95, 0.96, 0.98),
                    Vec2::splat(BOARD_TILE_SIZE * 0.72),
                ),
                Transform::from_xyz(hangar_slot.x, hangar_slot.y, BOARD_Z_LAYER + 0.02),
                Name::new(format!("HangarPad_P{}", player.state.player_id)),
                BoardSceneEntity,
            ));
        }
    }
}

fn cleanup_board(mut commands: Commands, query: Query<Entity, With<BoardSceneEntity>>) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
}

/// 绘制四个固定机场色块（蓝/红/绿/黄）。
fn spawn_airport_blocks(commands: &mut Commands) {
    for (player_id, center) in [
        (1, Vec2::new(-320.0, 280.0)),
        (2, Vec2::new(320.0, 280.0)),
        (3, Vec2::new(-320.0, -280.0)),
        (4, Vec2::new(320.0, -280.0)),
    ] {
        commands.spawn((
            Sprite::from_color(
                board_color_for_player(player_id).with_alpha(0.92),
                Vec2::new(248.0, 248.0),
            ),
            Transform::from_xyz(center.x, center.y, BOARD_Z_LAYER - 0.55),
            Name::new(format!("Airport_P{player_id}")),
            BoardSceneEntity,
        ));
    }
}

/// 绘制中心四向终点分区。
fn spawn_center_goal_zone(commands: &mut Commands) {
    for (name, color, pos) in [
        (
            "CenterBlue",
            board_color_for_player(1),
            Vec2::new(-32.0, 32.0),
        ),
        (
            "CenterRed",
            board_color_for_player(2),
            Vec2::new(32.0, 32.0),
        ),
        (
            "CenterGreen",
            board_color_for_player(3),
            Vec2::new(-32.0, -32.0),
        ),
        (
            "CenterYellow",
            board_color_for_player(4),
            Vec2::new(32.0, -32.0),
        ),
    ] {
        commands.spawn((
            Sprite::from_color(color.with_alpha(0.96), Vec2::splat(64.0)),
            Transform::from_xyz(pos.x, pos.y, BOARD_Z_LAYER - 0.15),
            Name::new(name),
            BoardSceneEntity,
        ));
    }
}

/// 根据格子类型叠加简化图标，提升可读性。
fn spawn_tile_marker(commands: &mut Commands, pos: Vec2, kind: TileKind, route_index: u8) {
    let marker_z = BOARD_Z_LAYER + 0.10;
    match kind {
        TileKind::Normal | TileKind::Goal => {}
        TileKind::Attack => {
            commands.spawn((
                Sprite::from_color(Color::BLACK, Vec2::splat(BOARD_TILE_SIZE * 0.22)),
                Transform::from_xyz(pos.x, pos.y, marker_z),
                Name::new(format!("AttackMarker_{route_index}")),
                BoardSceneEntity,
            ));
        }
        TileKind::Defense => {
            commands.spawn((
                Sprite::from_color(
                    Color::srgb(0.08, 0.10, 0.14),
                    Vec2::new(BOARD_TILE_SIZE * 0.26, 5.0),
                ),
                Transform::from_xyz(pos.x, pos.y, marker_z),
                Name::new(format!("DefenseMarkerH_{route_index}")),
                BoardSceneEntity,
            ));
            commands.spawn((
                Sprite::from_color(
                    Color::srgb(0.08, 0.10, 0.14),
                    Vec2::new(5.0, BOARD_TILE_SIZE * 0.26),
                ),
                Transform::from_xyz(pos.x, pos.y, marker_z),
                Name::new(format!("DefenseMarkerV_{route_index}")),
                BoardSceneEntity,
            ));
        }
        TileKind::Event => {
            commands.spawn((
                Sprite::from_color(
                    Color::srgb(0.89, 0.29, 0.48),
                    Vec2::new(BOARD_TILE_SIZE * 0.30, 7.0),
                ),
                Transform::from_xyz(pos.x, pos.y, marker_z),
                Name::new(format!("EventMarker_{route_index}")),
                BoardSceneEntity,
            ));
            commands.spawn((
                Sprite::from_color(
                    Color::srgb(0.25, 0.78, 0.96),
                    Vec2::new(BOARD_TILE_SIZE * 0.16, 7.0),
                ),
                Transform::from_xyz(pos.x, pos.y, marker_z + 0.01),
                Name::new(format!("EventMarkerInner_{route_index}")),
                BoardSceneEntity,
            ));
        }
        TileKind::Jump => {
            for (offset_x, alpha) in [(-10.0, 0.45), (0.0, 0.70), (10.0, 0.95)] {
                commands.spawn((
                    Sprite::from_color(
                        Color::srgb(0.17, 0.62, 0.95).with_alpha(alpha),
                        Vec2::new(5.0, BOARD_TILE_SIZE * 0.30),
                    ),
                    Transform::from_xyz(pos.x + offset_x, pos.y, marker_z),
                    Name::new(format!("JumpMarker_{route_index}_{offset_x}")),
                    BoardSceneEntity,
                ));
            }
        }
    }
}

/// 将玩家编号映射为固定棋盘主题色。
fn board_color_for_player(player_id: u8) -> Color {
    match player_id {
        1 => Color::srgb(0.28, 0.52, 0.84),
        2 => Color::srgb(0.92, 0.27, 0.19),
        3 => Color::srgb(0.20, 0.66, 0.41),
        4 => Color::srgb(0.93, 0.79, 0.18),
        _ => Color::srgb(0.84, 0.89, 0.96),
    }
}

/// 主环道四色带配色（按索引循环）。
fn route_band_color(route_index: u8) -> Color {
    match route_index % 4 {
        0 => Color::srgb(0.20, 0.66, 0.41),
        1 => Color::srgb(0.28, 0.52, 0.84),
        2 => Color::srgb(0.92, 0.27, 0.19),
        _ => Color::srgb(0.93, 0.79, 0.18),
    }
}
