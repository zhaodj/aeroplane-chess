use crate::domain::dice::DiceRoll;
use crate::domain::piece::{PieceState, PieceStatus};

pub fn can_launch(piece: &PieceState, roll: DiceRoll) -> bool {
    piece.status == PieceStatus::InHangar && roll.0 == 6
}

pub fn can_move_exact(piece: &PieceState, roll: DiceRoll, remaining_steps: u8) -> bool {
    piece.status == PieceStatus::Active && roll.0 <= remaining_steps
}
