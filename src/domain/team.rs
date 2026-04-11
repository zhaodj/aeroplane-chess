#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TeamState {
    pub team_id: u8,
    pub player_ids: Vec<u8>,
}
