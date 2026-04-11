#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TurnState {
    pub current_player: u8,
    pub extra_rolls_remaining: u8,
    pub turn_index: u32,
}
