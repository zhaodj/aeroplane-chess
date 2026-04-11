pub fn all_pieces_finished(piece_statuses: &[bool]) -> bool {
    piece_statuses.iter().all(|finished| *finished)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_pieces_finished_only_when_every_piece_is_true() {
        assert!(all_pieces_finished(&[true, true]));
        assert!(!all_pieces_finished(&[true, false]));
        assert!(!all_pieces_finished(&[false, false]));
    }

    #[test]
    fn empty_slice_is_treated_as_finished() {
        assert!(all_pieces_finished(&[]));
    }
}
