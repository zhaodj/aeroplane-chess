use crate::domain::dice::DiceRoll;
use crate::domain::piece::{PieceState, PieceStatus};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
/// 棋子从机库起飞所需的骰子条件。
pub enum LaunchRule {
    Even,
    #[default]
    SixOnly,
}

impl LaunchRule {
    pub const ALL: [Self; 2] = [Self::Even, Self::SixOnly];

    pub fn allows(self, roll: DiceRoll) -> bool {
        match self {
            Self::Even => matches!(roll.0, 2 | 4 | 6),
            Self::SixOnly => roll.0 == 6,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Even => "2/4/6",
            Self::SixOnly => "6 only",
        }
    }
}

pub fn can_launch(piece: &PieceState, roll: DiceRoll, launch_rule: LaunchRule) -> bool {
    piece.status == PieceStatus::InHangar && launch_rule.allows(roll)
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
            motion_serial: 0,
        }
    }

    #[test]
    fn launch_requires_hangar_and_configured_roll() {
        assert!(can_launch(
            &piece(PieceStatus::InHangar),
            DiceRoll(6),
            LaunchRule::SixOnly
        ));
        assert!(!can_launch(
            &piece(PieceStatus::InHangar),
            DiceRoll(4),
            LaunchRule::SixOnly
        ));
        assert!(can_launch(
            &piece(PieceStatus::InHangar),
            DiceRoll(4),
            LaunchRule::Even
        ));
        assert!(!can_launch(
            &piece(PieceStatus::InHangar),
            DiceRoll(5),
            LaunchRule::Even
        ));
        assert!(!can_launch(
            &piece(PieceStatus::AtLaunch),
            DiceRoll(6),
            LaunchRule::Even
        ));
        assert!(!can_launch(
            &piece(PieceStatus::Active),
            DiceRoll(6),
            LaunchRule::Even
        ));
    }
}
