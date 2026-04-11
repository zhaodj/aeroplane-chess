#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TileKind {
    Normal,
    Jump,
    Attack,
    Defense,
    Event,
    Goal,
}
