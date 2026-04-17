use bevy::prelude::*;

use crate::domain::tile::TileKind;

#[derive(Clone, Debug)]
/// 单个环道格子的静态配置（ID、类型、索引与坐标）。
pub struct TileConfig {
    pub id: String,
    pub kind: TileKind,
    pub route_index: Option<u8>,
    pub world_pos: Vec2,
}

pub fn default_board_tiles() -> Vec<TileConfig> {
    let route = [
        (-2.0, 4.0),
        (-1.0, 4.0),
        (0.0, 4.0),
        (1.0, 4.0),
        (2.0, 4.0),
        (2.0, 3.0),
        (2.0, 2.0),
        (3.0, 2.0),
        (4.0, 2.0),
        (4.0, 1.0),
        (4.0, 0.0),
        (4.0, -1.0),
        (4.0, -2.0),
        (3.0, -2.0),
        (2.0, -2.0),
        (2.0, -3.0),
        (2.0, -4.0),
        (1.0, -4.0),
        (0.0, -4.0),
        (-1.0, -4.0),
        (-2.0, -4.0),
        (-2.0, -3.0),
        (-2.0, -2.0),
        (-3.0, -2.0),
        (-4.0, -2.0),
        (-4.0, -1.0),
        (-4.0, 0.0),
        (-4.0, 1.0),
        (-4.0, 2.0),
        (-3.0, 2.0),
        (-2.0, 2.0),
        (-2.0, 3.0),
    ];

    route
        .into_iter()
        .enumerate()
        .map(|(index, (x, y))| TileConfig {
            id: format!("tile_{index:02}"),
            kind: tile_kind_for_index(index),
            route_index: Some(index as u8),
            world_pos: Vec2::new(x * 64.0, y * 64.0),
        })
        .collect()
}

fn tile_kind_for_index(index: usize) -> TileKind {
    match index {
        2 | 10 | 18 | 26 => TileKind::Jump,
        5 | 13 | 21 | 29 => TileKind::Attack,
        7 | 15 | 23 | 31 => TileKind::Defense,
        0 | 8 | 16 | 24 => TileKind::Event,
        _ => TileKind::Normal,
    }
}
