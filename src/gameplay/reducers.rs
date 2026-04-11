pub fn reduce_turn_index(turn_index: u32) -> u32 {
    turn_index.saturating_add(1)
}
