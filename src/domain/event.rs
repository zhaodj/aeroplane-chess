#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TileEventKind {
    GainShield,
    GainSkillCharge,
    AdvanceTwo,
    DisableNextSkill,
    RemoveEnemyShield,
}
