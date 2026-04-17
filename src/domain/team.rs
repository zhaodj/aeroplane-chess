use bevy::prelude::*;

#[derive(Clone, Debug, Component, Eq, PartialEq)]
/// 队伍状态：队伍编号及其包含的玩家 ID 列表。
pub struct TeamState {
    pub team_id: u8,
    pub player_ids: Vec<u8>,
}
