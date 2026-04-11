use crate::domain::tile::TileKind;

#[derive(Clone, Debug)]
pub struct TileConfig {
    pub id: String,
    pub kind: TileKind,
    pub route_index: Option<u8>,
}
