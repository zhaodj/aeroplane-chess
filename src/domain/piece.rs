use bevy::prelude::*;

/// Swap intentionally uses a distinct motion tick so the animation layer can
/// render an arc instead of a normal route walk.
pub const SWAP_MOTION_SERIAL_DELTA: u32 = 2;

/// 相对本方起点的路径进度；换位到起点前两格时为 -2/-1，前进后自然回到 0。
pub type PieceProgress = i16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PieceStatus {
    InHangar,
    AtLaunch,
    Active,
    Finished,
}

#[derive(Clone, Copy, Debug, Component, Eq, PartialEq)]
/// 棋子状态快照：归属、阶段、进度与护盾信息。
pub struct PieceState {
    pub owner_player_id: u8,
    pub team_id: u8,
    pub status: PieceStatus,
    pub progress: PieceProgress,
    pub shield: u8,
    pub stack_shield: u8,
    pub motion_serial: u32,
}
