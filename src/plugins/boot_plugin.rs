use bevy::prelude::*;

use crate::constants::BOARD_WORLD_SIZE;
use crate::data::game_mode::GameMode;
use crate::data::rule_set::RuleSet;
use crate::domain::player::PlayerControl;
use crate::domain::rules::LaunchRule;
use crate::gameplay::ai::AiDifficulty;
use crate::gameplay::match_flow::{MatchSetup, PlayerSeat};
use crate::platform::DeviceProfile;
use crate::states::AppState;
use crate::ui::game_layout::GameLayout;

/// 启动插件：初始化相机与默认 MatchSetup，并跳转主菜单。
pub struct BootPlugin;

#[derive(Resource)]
/// 隐藏的浏览器 smoke 模式：让运行时自动推进所有玩家，便于 wasm 端到端验证。
pub struct AutoplayMatch;

impl Plugin for BootPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_camera).add_systems(
            Update,
            fit_in_game_camera.run_if(in_state(AppState::InGame)),
        );
    }
}

fn setup_camera(mut commands: Commands, mut next_state: ResMut<NextState<AppState>>) {
    commands.spawn(Camera2d);

    let autoplay_query = autoplay_query_string()
        .filter(|query| !query.is_empty())
        .or_else(autoplay_shell_query_string);
    let autoplay_setup = autoplay_query
        .as_deref()
        .and_then(autoplay_match_setup_from_query);
    let autoplay_enabled = autoplay_setup.is_some();
    let smoke_requested = autoplay_smoke_shell_enabled()
        || autoplay_query
            .as_deref()
            .is_some_and(|query| query.contains("ac_autoplay="));
    if smoke_requested {
        let state = if autoplay_enabled {
            "boot"
        } else if autoplay_query
            .as_deref()
            .is_some_and(|query| query.contains("ac_autoplay="))
        {
            "query-invalid"
        } else {
            "query-missing"
        };
        set_autoplay_boot_state(state, autoplay_query.as_deref().unwrap_or_default());
    }
    let match_setup = autoplay_setup.unwrap_or_else(default_match_setup);
    if autoplay_enabled {
        commands.insert_resource(AutoplayMatch);
        next_state.set(AppState::LoadingGame);
    } else {
        next_state.set(AppState::MainMenu);
    }
    commands.insert_resource(match_setup);
}

fn default_match_setup() -> MatchSetup {
    MatchSetup {
        mode: GameMode::TwoVsTwo,
        rule_set: RuleSet::Creative,
        ai_difficulty: AiDifficulty::Normal,
        fast_mode: false,
        launch_rule: LaunchRule::SixOnly,
        player_seats: [
            PlayerSeat::Blue,
            PlayerSeat::Red,
            PlayerSeat::Green,
            PlayerSeat::Yellow,
        ],
        pieces_per_player: 2,
        player_controls: [
            PlayerControl::Human,
            PlayerControl::Ai,
            PlayerControl::Human,
            PlayerControl::Ai,
        ],
    }
}

#[cfg(target_arch = "wasm32")]
fn autoplay_query_string() -> Option<String> {
    web_sys::window()?.location().search().ok()
}

#[cfg(not(target_arch = "wasm32"))]
fn autoplay_query_string() -> Option<String> {
    None
}

#[cfg(target_arch = "wasm32")]
fn autoplay_shell_query_string() -> Option<String> {
    web_sys::window()?
        .document()?
        .body()?
        .get_attribute("data-ac-smoke-query-shell")
}

#[cfg(not(target_arch = "wasm32"))]
fn autoplay_shell_query_string() -> Option<String> {
    None
}

#[cfg(target_arch = "wasm32")]
fn autoplay_smoke_shell_enabled() -> bool {
    web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.body())
        .is_some_and(|body| body.has_attribute("data-ac-smoke-shell"))
}

#[cfg(not(target_arch = "wasm32"))]
fn autoplay_smoke_shell_enabled() -> bool {
    false
}

#[cfg(target_arch = "wasm32")]
fn set_autoplay_boot_state(state: &str, query: &str) {
    let Some(body) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.body())
    else {
        return;
    };
    let _ = body.set_attribute("data-ac-smoke-state", state);
    let _ = body.set_attribute("data-ac-smoke-query", query);
}

#[cfg(not(target_arch = "wasm32"))]
fn set_autoplay_boot_state(_state: &str, _query: &str) {}

fn autoplay_match_setup_from_query(query: &str) -> Option<MatchSetup> {
    let value = query_param(query, "ac_autoplay")?;
    let mut setup = default_match_setup();
    setup.fast_mode = true;
    setup.ai_difficulty = AiDifficulty::Easy;
    setup.player_controls = [
        PlayerControl::Ai,
        PlayerControl::Ai,
        PlayerControl::Ai,
        PlayerControl::Ai,
    ];

    match value {
        "1" | "1v1-even-1" => {
            setup.mode = GameMode::OneVsOne;
            setup.launch_rule = LaunchRule::Even;
            setup.pieces_per_player = 1;
            setup.player_seats = [
                PlayerSeat::Red,
                PlayerSeat::Blue,
                PlayerSeat::Green,
                PlayerSeat::Yellow,
            ];
        }
        "2v2-even-1" => {
            setup.mode = GameMode::TwoVsTwo;
            setup.launch_rule = LaunchRule::Even;
            setup.pieces_per_player = 1;
            setup.player_seats = [
                PlayerSeat::Red,
                PlayerSeat::Yellow,
                PlayerSeat::Blue,
                PlayerSeat::Green,
            ];
        }
        "ffa-even-1" => {
            setup.mode = GameMode::FreeForAll;
            setup.launch_rule = LaunchRule::Even;
            setup.pieces_per_player = 1;
            setup.player_seats = [
                PlayerSeat::Green,
                PlayerSeat::Blue,
                PlayerSeat::Yellow,
                PlayerSeat::Red,
            ];
        }
        _ => return None,
    }

    if let Some(ai_difficulty) = query_param(query, "ac_ai").and_then(autoplay_ai_difficulty) {
        setup.ai_difficulty = ai_difficulty;
    }

    Some(setup)
}

fn query_param<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query
        .trim_start_matches('?')
        .split('&')
        .filter_map(|part| part.split_once('='))
        .find_map(|(candidate, value)| (candidate == key).then_some(value))
}

fn autoplay_ai_difficulty(value: &str) -> Option<AiDifficulty> {
    match value {
        "easy" => Some(AiDifficulty::Easy),
        "normal" => Some(AiDifficulty::Normal),
        "hard" => Some(AiDifficulty::Hard),
        _ => None,
    }
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
    let camera_scale = centered_board_camera_scale(window_width, window_height, *device_profile);
    let center = GameLayout::new(window_width, window_height, *device_profile)
        .board
        .center();

    for (mut transform, mut projection) in &mut camera_query {
        if let Projection::Orthographic(orthographic) = &mut *projection {
            orthographic.scale = camera_scale;
            transform.translation.x = (window_width * 0.5 - center.x) * camera_scale;
            transform.translation.y = (center.y - window_height * 0.5) * camera_scale;
        }
    }
}

fn centered_board_camera_scale(
    window_width: f32,
    window_height: f32,
    device_profile: DeviceProfile,
) -> f32 {
    BOARD_WORLD_SIZE
        / GameLayout::new(window_width, window_height, device_profile)
            .board
            .w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_and_ui_project_the_same_board_bounds_in_both_orientations() {
        for (w, h) in [
            (360., 640.),
            (720., 1280.),
            (1024., 600.),
            (1280., 720.),
            (1920., 1080.),
        ] {
            let profile = DeviceProfile::from_window_size(w, h);
            let board = GameLayout::new(w, h, profile).board;
            let scale = centered_board_camera_scale(w, h, profile);
            let camera = Vec2::new(
                (w / 2. - board.center().x) * scale,
                (board.center().y - h / 2.) * scale,
            );
            let project = |world: Vec2| {
                Vec2::new(
                    w / 2. + (world.x - camera.x) / scale,
                    h / 2. - (world.y - camera.y) / scale,
                )
            };
            assert!(
                project(Vec2::new(-BOARD_WORLD_SIZE / 2., BOARD_WORLD_SIZE / 2.))
                    .distance(Vec2::new(board.x, board.y))
                    < 0.01
            );
            assert!(
                project(Vec2::new(BOARD_WORLD_SIZE / 2., -BOARD_WORLD_SIZE / 2.))
                    .distance(Vec2::new(board.x + board.w, board.y + board.h))
                    < 0.01
            );
        }
    }

    #[test]
    fn autoplay_query_builds_fast_all_ai_match_setup() {
        let setup = autoplay_match_setup_from_query("?ac_autoplay=ffa-even-1")
            .expect("autoplay setup should parse");

        assert_eq!(setup.mode, GameMode::FreeForAll);
        assert_eq!(setup.launch_rule, LaunchRule::Even);
        assert_eq!(setup.pieces_per_player, 1);
        assert_eq!(setup.ai_difficulty, AiDifficulty::Easy);
        assert!(setup.fast_mode);
        assert!(
            setup
                .player_controls
                .iter()
                .all(|control| *control == PlayerControl::Ai)
        );
        assert_eq!(
            setup.player_seats,
            [
                PlayerSeat::Green,
                PlayerSeat::Blue,
                PlayerSeat::Yellow,
                PlayerSeat::Red,
            ]
        );
    }

    #[test]
    fn autoplay_query_can_request_hard_ai_for_skill_smoke() {
        let setup = autoplay_match_setup_from_query("?ac_autoplay=2v2-even-1&ac_ai=hard")
            .expect("autoplay setup should parse");

        assert_eq!(setup.mode, GameMode::TwoVsTwo);
        assert_eq!(setup.ai_difficulty, AiDifficulty::Hard);
    }

    #[test]
    fn autoplay_query_ignores_unknown_values() {
        assert!(autoplay_match_setup_from_query("?ac_autoplay=unknown").is_none());
        assert!(autoplay_match_setup_from_query("?other=1").is_none());
    }
}
