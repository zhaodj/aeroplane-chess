use bevy::prelude::*;

#[derive(Clone, Debug, Default, Eq, PartialEq, Resource)]
pub struct TurnState {
    pub current_player: u8,
    pub extra_rolls_remaining: u8,
    pub turn_index: u32,
    pub current_roll: Option<u8>,
    pub last_roll: Option<u8>,
    pub last_action: Option<String>,
}

impl TurnState {
    pub fn opening_turn() -> Self {
        Self {
            current_player: 1,
            extra_rolls_remaining: 0,
            turn_index: 1,
            current_roll: None,
            last_roll: None,
            last_action: None,
        }
    }
}
