#[derive(Clone, Debug, Default)]
/// 棋盘逻辑拓扑长度信息（主环道与冲线道长度）。
pub struct BoardGraph {
    pub main_route_len: u8,
    pub home_route_len: u8,
}
