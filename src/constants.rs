pub const WINDOW_WIDTH: u32 = 1280;
pub const WINDOW_HEIGHT: u32 = 720;
pub const BOARD_WORLD_SIZE: f32 = 683.0;
pub const BOARD_OUTER_HUD_RESERVE: f32 = 84.0;
pub const BOARD_TILE_SIZE: f32 = 40.0;
pub const BOARD_Z_LAYER: f32 = 0.0;
pub const HUD_Z_LAYER: f32 = 10.0;
pub const HUD_PANEL_WIDTH: f32 = 520.0;

pub fn gameplay_board_target_pixels(
    window_width: f32,
    window_height: f32,
    board_padding: f32,
) -> f32 {
    let reserved = board_padding.max(BOARD_OUTER_HUD_RESERVE);
    (window_width.min(window_height) - reserved)
        .max(240.0)
        .min(BOARD_WORLD_SIZE)
}
