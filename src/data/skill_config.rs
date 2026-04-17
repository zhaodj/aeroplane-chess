use crate::domain::skill::SkillId;

#[derive(Clone, Debug)]
/// 技能基础配置：技能 ID 与最大充能。
pub struct SkillConfig {
    pub id: SkillId,
    pub max_charges: u8,
}
