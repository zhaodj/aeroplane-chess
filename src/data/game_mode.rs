#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GameMode {
    OneVsOne,
    TwoVsTwo,
    FreeForAll,
}

impl GameMode {
    pub const ALL: [Self; 3] = [Self::OneVsOne, Self::TwoVsTwo, Self::FreeForAll];

    pub fn label(self) -> &'static str {
        match self {
            Self::OneVsOne => "1v1",
            Self::TwoVsTwo => "2v2",
            Self::FreeForAll => "FFA",
        }
    }
}
