use bevy::prelude::*;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, States)]
pub enum AppState {
    #[default]
    Boot,
    MainMenu,
    ModeSelect,
    CharacterSelect,
    SkillSelect,
    LoadingGame,
    InGame,
    Result,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, States)]
pub enum GamePhase {
    #[default]
    RoundStart,
    AwaitDice,
    DiceRolling,
    AwaitPieceSelect,
    PieceMoving,
    ResolveTileEffect,
    ResolveSkillEffect,
    ResolveCombat,
    CheckVictory,
    RoundEnd,
}
