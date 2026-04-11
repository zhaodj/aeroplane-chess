use bevy::prelude::*;

#[derive(Clone, Debug, Component, Eq, PartialEq)]
pub struct TeamState {
    pub team_id: u8,
    pub player_ids: Vec<u8>,
}
