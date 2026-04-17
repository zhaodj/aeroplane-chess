pub mod ai_plugin;
pub mod animation_plugin;
pub mod audio_plugin;
pub mod board_plugin;
pub mod boot_plugin;
pub mod game_plugin;
pub mod menu_plugin;
pub mod piece_plugin;
pub mod skill_plugin;
pub mod turn_plugin;
pub mod ui_plugin;

use bevy::prelude::*;

use self::ai_plugin::AiPlugin;
use self::animation_plugin::AnimationPlugin;
use self::audio_plugin::AudioPlugin;
use self::board_plugin::BoardPlugin;
use self::boot_plugin::BootPlugin;
use self::game_plugin::GamePlugin;
use self::menu_plugin::MenuPlugin;
use self::piece_plugin::PiecePlugin;
use self::skill_plugin::SkillPlugin;
use self::turn_plugin::TurnPlugin;
use self::ui_plugin::UiPlugin;

/// 游戏插件集合：统一注册所有子系统插件。
pub struct AeroplaneChessPlugins;

impl Plugin for AeroplaneChessPlugins {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            BootPlugin,
            MenuPlugin,
            GamePlugin,
            BoardPlugin,
            PiecePlugin,
            TurnPlugin,
            SkillPlugin,
            AiPlugin,
            UiPlugin,
            AudioPlugin,
            AnimationPlugin,
        ));
    }
}
