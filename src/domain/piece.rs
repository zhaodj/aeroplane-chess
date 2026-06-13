use bevy::prelude::*;

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
    pub progress: u8,
    pub shield: u8,
    pub stack_shield: u8,
}
