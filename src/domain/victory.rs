pub fn all_pieces_finished(piece_statuses: &[bool]) -> bool {
    piece_statuses.iter().all(|finished| *finished)
}
