#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// 选中棋子的输入命令（用于统一输入层与结算层）。
pub struct SelectPieceCommand {
    pub piece_id: u8,
}
