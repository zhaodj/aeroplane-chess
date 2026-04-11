#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PieceStatus {
    InHangar,
    Active,
    Finished,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PieceState {
    pub owner_player_id: u8,
    pub team_id: u8,
    pub status: PieceStatus,
    pub progress: u8,
    pub shield: u8,
}
