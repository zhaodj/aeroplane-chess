use bevy::prelude::*;

/// AI 插件入口（当前版本仅保留占位，AI 逻辑在 TurnPlugin 中驱动）。
pub struct AiPlugin;

impl Plugin for AiPlugin {
    fn build(&self, _app: &mut App) {}
}
