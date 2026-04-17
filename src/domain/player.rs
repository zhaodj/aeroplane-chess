use bevy::prelude::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlayerControl {
    Human,
    Ai,
}

#[derive(Clone, Debug, Component, Eq, PartialEq)]
/// 玩家状态：包含玩家编号、队伍编号和人机控制类型。
pub struct PlayerState {
    pub player_id: u8,
    pub team_id: u8,
    pub control: PlayerControl,
}
