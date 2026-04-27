use crate::domain::dice::DiceRoll;
use crate::domain::piece::{PieceState, PieceStatus};

pub fn can_launch(piece: &PieceState, roll: DiceRoll) -> bool {
    piece.status == PieceStatus::InHangar && roll.0 == 6
}

pub fn can_move_exact(piece: &PieceState, roll: DiceRoll, remaining_steps: u8) -> bool {
    matches!(piece.status, PieceStatus::AtLaunch | PieceStatus::Active) && roll.0 <= remaining_steps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn piece(status: PieceStatus) -> PieceState {
        PieceState {
            owner_player_id: 1,
            team_id: 1,
            status,
            progress: 0,
            shield: 0,
            stack_shield: 0,
        }
    }

    #[test]
    fn launch_requires_hangar_and_six() {
        assert!(can_launch(&piece(PieceStatus::InHangar), DiceRoll(6)));
        assert!(!can_launch(&piece(PieceStatus::InHangar), DiceRoll(5)));
        assert!(!can_launch(&piece(PieceStatus::AtLaunch), DiceRoll(6)));
        assert!(!can_launch(&piece(PieceStatus::Active), DiceRoll(6)));
    }

    #[test]
    fn exact_move_rejects_overshoot_and_inactive_piece() {
        assert!(can_move_exact(&piece(PieceStatus::Active), DiceRoll(3), 3));
        assert!(can_move_exact(
            &piece(PieceStatus::AtLaunch),
            DiceRoll(3),
            3
        ));
        assert!(can_move_exact(&piece(PieceStatus::Active), DiceRoll(2), 3));
        assert!(!can_move_exact(&piece(PieceStatus::Active), DiceRoll(4), 3));
        assert!(!can_move_exact(
            &piece(PieceStatus::InHangar),
            DiceRoll(3),
            3
        ));
    }
}
