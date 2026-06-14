use bevy::prelude::*;

use crate::domain::tile::TileKind;

#[derive(Clone, Debug)]
/// 单个环道格子的静态配置（ID、类型、索引与坐标）。
pub struct TileConfig {
    pub id: String,
    pub kind: TileKind,
    pub route_index: Option<u8>,
    pub player_color_slot: usize,
    pub world_pos: Vec2,
    pub jump_shortcut_to: Option<u8>,
}

/// 主环道格子的设计图原始色槽。
///
/// 槽位对应 `PlayerRoster.player_colors` 的下标：
/// 0=P1 原蓝色槽，1=P2 原红色槽，2=P3 原绿色槽，3=P4 原黄色槽。
/// 棋盘渲染会把这些原始槽位映射为当前玩家配置后的实际颜色。
pub const ROUTE_PLAYER_COLOR_SLOTS: [usize; 48] = [
    0, 3, 2, 0, 1, 3, 2, 0, 1, 3, 2, 0, 1, 2, 0, 1, 3, 2, 0, 1, 3, 2, 0, 1, 3, 0, 1, 3, 2, 0, 1, 3,
    2, 0, 1, 3, 2, 1, 3, 2, 0, 1, 3, 2, 0, 1, 3, 2,
];

pub fn default_board_tiles() -> Vec<TileConfig> {
    let route = [
        (-40.104, 300.104),
        (40.317, 300.104),
        (80.317, 300.104),
        (124.317, 284.104),
        (140.317, 240.104),
        (140.317, 200.104),
        (124.317, 156.104),
        (156.317, 124.104),
        (200.317, 140.104),
        (240.317, 140.104),
        (284.317, 124.104),
        (300.317, 80.104),
        (300.317, 40.104),
        (300.104, -40.104),
        (300.104, -80.104),
        (284.104, -124.104),
        (240.104, -140.104),
        (200.104, -140.104),
        (156.104, -124.104),
        (124.104, -156.104),
        (140.104, -200.104),
        (140.104, -240.104),
        (124.104, -284.104),
        (80.104, -300.104),
        (40.104, -300.104),
        (-40.104, -300.104),
        (-80.104, -300.104),
        (-124.104, -284.104),
        (-140.104, -240.104),
        (-140.104, -200.104),
        (-124.104, -156.104),
        (-156.104, -124.104),
        (-200.104, -140.104),
        (-240.104, -140.104),
        (-284.104, -124.104),
        (-300.104, -80.104),
        (-300.104, -40.104),
        (-300.104, 40.104),
        (-300.104, 80.104),
        (-284.104, 124.104),
        (-240.104, 140.104),
        (-200.104, 140.104),
        (-156.104, 124.104),
        (-124.104, 156.104),
        (-140.104, 200.104),
        (-140.104, 240.104),
        (-124.104, 284.104),
        (-80.104, 300.104),
    ];

    route
        .into_iter()
        .enumerate()
        .map(|(index, (x, y))| TileConfig {
            id: format!("tile_{index:02}"),
            kind: tile_kind_for_index(index),
            route_index: Some(index as u8),
            player_color_slot: player_color_slot_for_index(index),
            world_pos: Vec2::new(x, y),
            jump_shortcut_to: jump_shortcut_target_for_index(index),
        })
        .collect()
}

fn player_color_slot_for_index(index: usize) -> usize {
    ROUTE_PLAYER_COLOR_SLOTS[index]
}

fn tile_kind_for_index(index: usize) -> TileKind {
    match index {
        7 | 19 | 31 | 43 => TileKind::Jump,
        2 | 14 | 26 | 38 => TileKind::Attack,
        8 | 20 | 32 | 44 => TileKind::Defense,
        0 | 12 | 24 | 36 => TileKind::Event,
        _ => TileKind::Normal,
    }
}

fn jump_shortcut_target_for_index(index: usize) -> Option<u8> {
    match index {
        7 => Some(18),
        19 => Some(30),
        31 => Some(42),
        43 => Some(6),
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

        assert_eq!(shortcuts, vec![(7, 18), (19, 30), (31, 42), (43, 6)]);
    }

    #[test]
    fn route_color_slots_follow_svg_palette_slots() {
        let tiles = default_board_tiles();

        assert_eq!(tiles[40].player_color_slot, 0);
        assert_eq!(tiles[45].player_color_slot, 1);
        assert_eq!(tiles[39].player_color_slot, 2);
        assert_eq!(tiles[5].player_color_slot, 3);
    }
}
