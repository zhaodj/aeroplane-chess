use crate::domain::tile::TileKind;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// 玩法规则集：传统玩法保留经典飞行棋移动/撞击，创新玩法启用技能与特殊点。
pub enum RuleSet {
    Traditional,
    Creative,
}

impl RuleSet {
    pub const ALL: [Self; 2] = [Self::Traditional, Self::Creative];

    pub fn label(self) -> &'static str {
        match self {
            Self::Traditional => "Traditional",
            Self::Creative => "Creative",
        }
    }

    pub fn skills_enabled(self) -> bool {
        matches!(self, Self::Creative)
    }

    pub fn shields_enabled(self) -> bool {
        matches!(self, Self::Creative)
    }

    /// 返回当前玩法下真正参与结算/展示的格子类型。
    pub fn effective_tile_kind(self, kind: TileKind) -> TileKind {
        match (self, kind) {
            (Self::Traditional, TileKind::Attack | TileKind::Defense | TileKind::Event) => {
                TileKind::Normal
            }
            _ => kind,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traditional_rule_set_removes_creative_tile_kinds_and_skills() {
        assert!(!RuleSet::Traditional.skills_enabled());
        assert!(!RuleSet::Traditional.shields_enabled());
        assert_eq!(
            RuleSet::Traditional.effective_tile_kind(TileKind::Attack),
            TileKind::Normal
        );
        assert_eq!(
            RuleSet::Traditional.effective_tile_kind(TileKind::Defense),
            TileKind::Normal
        );
        assert_eq!(
            RuleSet::Traditional.effective_tile_kind(TileKind::Event),
            TileKind::Normal
        );
        assert_eq!(
            RuleSet::Traditional.effective_tile_kind(TileKind::Jump),
            TileKind::Jump
        );
    }

    #[test]
    fn creative_rule_set_keeps_current_feature_set() {
        assert!(RuleSet::Creative.skills_enabled());
        assert!(RuleSet::Creative.shields_enabled());
        assert_eq!(
            RuleSet::Creative.effective_tile_kind(TileKind::Attack),
            TileKind::Attack
        );
        assert_eq!(
            RuleSet::Creative.effective_tile_kind(TileKind::Defense),
            TileKind::Defense
        );
        assert_eq!(
            RuleSet::Creative.effective_tile_kind(TileKind::Event),
            TileKind::Event
        );
    }
}
