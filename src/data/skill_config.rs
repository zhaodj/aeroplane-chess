use crate::domain::skill::SkillId;

#[derive(Clone, Debug)]
pub struct SkillConfig {
    pub id: SkillId,
    pub max_charges: u8,
}
