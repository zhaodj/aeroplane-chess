use bevy::prelude::*;

use crate::data::game_mode::GameMode;
use crate::domain::player::PlayerControl;
use crate::domain::rules::LaunchRule;
use crate::gameplay::ai::AiDifficulty;
use crate::gameplay::match_flow::{MatchSetup, PlayerColorChoice};
use crate::platform::DeviceProfile;
use crate::states::AppState;

/// 启动插件：初始化相机与默认 MatchSetup，并跳转主菜单。
pub struct BootPlugin;

impl Plugin for BootPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Boot), setup_camera)
            .add_systems(OnEnter(AppState::Boot), advance_to_main_menu)
            .add_systems(
                Update,
                fit_in_game_camera.run_if(in_state(AppState::InGame)),
            );
    }
}

const BOARD_WORLD_SIZE: f32 = 683.0;
fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands.insert_resource(MatchSetup {
        mode: GameMode::TwoVsTwo,
        ai_difficulty: AiDifficulty::Normal,
        fast_mode: false,
        launch_rule: LaunchRule::SixOnly,
        player_colors: [
            PlayerColorChoice::Blue,
            PlayerColorChoice::Red,
            PlayerColorChoice::Green,
            PlayerColorChoice::Yellow,
        ],
        pieces_per_player: 2,
        player_controls: [
            PlayerControl::Human,
            PlayerControl::Ai,
            PlayerControl::Human,
            PlayerControl::Ai,
        ],
    });
}

fn advance_to_main_menu(mut next_state: ResMut<NextState<AppState>>) {
    next_state.set(AppState::MainMenu);
}

fn fit_in_game_camera(
    windows: Query<&Window>,
    device_profile: Res<DeviceProfile>,
    mut camera_query: Query<(&mut Transform, &mut Projection), With<Camera>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let window_width = window.width().max(1.0);
    let window_height = window.height().max(1.0);
    let reserved_width = device_profile.hud_reserved_width();
    let board_area_width = (window_width - reserved_width).max(240.0);
    let target_pixels =
        (board_area_width.min(window_height) - device_profile.board_screen_padding()).max(240.0);
    let camera_scale = (BOARD_WORLD_SIZE / target_pixels).max(1.0);
    let board_screen_center_x = if reserved_width <= f32::EPSILON {
        window_width * 0.5
    } else {
        board_area_width * 0.5
    };
    let camera_x = (window_width * 0.5 - board_screen_center_x) * camera_scale;

    for (mut transform, mut projection) in &mut camera_query {
        if let Projection::Orthographic(orthographic) = &mut *projection {
            orthographic.scale = camera_scale;
            transform.translation.x = camera_x;
            transform.translation.y = 0.0;
        }
    }
}
