#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlayerControl {
    Human,
    Ai,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayerState {
    pub player_id: u8,
    pub team_id: u8,
    pub control: PlayerControl,
}
