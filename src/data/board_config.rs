use bevy::prelude::*;

use crate::domain::tile::TileKind;

#[derive(Clone, Debug)]
/// 单个环道格子的静态配置（ID、类型、索引与坐标）。
pub struct TileConfig {
    pub id: String,
    pub kind: TileKind,
    pub route_index: Option<u8>,
    pub world_pos: Vec2,
    pub jump_shortcut_to: Option<u8>,
}

pub fn default_board_tiles() -> Vec<TileConfig> {
    let route = [
        (-2.0, 4.0),
        (-1.0, 4.0),
        (-1.0, 5.0),
        (-1.0, 6.0),
        (0.0, 6.0),
        (1.0, 6.0),
        (1.0, 5.0),
        (1.0, 4.0),
        (2.0, 4.0),
        (2.0, 3.0),
        (2.0, 2.0),
        (3.0, 2.0),
        (4.0, 2.0),
        (4.0, 1.0),
        (5.0, 1.0),
        (6.0, 1.0),
        (6.0, 0.0),
        (6.0, -1.0),
        (5.0, -1.0),
        (4.0, -1.0),
        (4.0, -2.0),
        (3.0, -2.0),
        (2.0, -2.0),
        (2.0, -3.0),
        (2.0, -4.0),
        (1.0, -4.0),
        (1.0, -5.0),
        (1.0, -6.0),
        (0.0, -6.0),
        (-1.0, -6.0),
        (-1.0, -5.0),
        (-1.0, -4.0),
        (-2.0, -4.0),
        (-2.0, -3.0),
        (-2.0, -2.0),
        (-3.0, -2.0),
        (-4.0, -2.0),
        (-4.0, -1.0),
        (-5.0, -1.0),
        (-6.0, -1.0),
        (-6.0, 0.0),
        (-6.0, 1.0),
        (-5.0, 1.0),
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
            world_pos: Vec2::new(x * 40.0, y * 40.0),
            jump_shortcut_to: jump_shortcut_target_for_index(index),
        })
        .collect()
}

fn tile_kind_for_index(index: usize) -> TileKind {
    match index {
        5 | 18 | 28 | 39 => TileKind::Jump,
        2 | 14 | 26 | 38 => TileKind::Attack,
        8 | 20 | 32 | 44 => TileKind::Defense,
        0 | 12 | 24 | 36 => TileKind::Event,
        _ => TileKind::Normal,
    }
}

fn jump_shortcut_target_for_index(index: usize) -> Option<u8> {
    match index {
        5 => Some(17),
        18 => Some(30),
        28 => Some(40),
        39 => Some(3),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_board_route_has_48_nodes() {
        let tiles = default_board_tiles();
        assert_eq!(tiles.len(), 48);
    }

    #[test]
    fn shortcut_jump_nodes_are_configured() {
        let tiles = default_board_tiles();
        let shortcuts = tiles
            .iter()
            .filter_map(|tile| tile.route_index.zip(tile.jump_shortcut_to))
            .collect::<Vec<_>>();

        assert_eq!(shortcuts, vec![(5, 17), (18, 30), (28, 40), (39, 3)]);
    }
}
