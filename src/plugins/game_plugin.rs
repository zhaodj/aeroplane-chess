use bevy::prelude::*;

use crate::data::board_config::{default_board_tiles, TileConfig};
use crate::data::game_mode::GameMode;
use crate::domain::player::{PlayerControl, PlayerState};
use crate::domain::team::TeamState;
use crate::domain::tile::TileKind;
use crate::gameplay::ai::AiDifficulty;
use crate::gameplay::turn_flow::TurnState;
use crate::states::AppState;
use crate::states::GamePhase;

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::LoadingGame), prepare_match);
    }
}

#[derive(SystemSet, Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GameSet {
    Flow,
}

#[derive(Clone, Debug, Resource)]
pub struct MatchConfig {
    pub mode: GameMode,
    pub ai_difficulty: AiDifficulty,
    pub fast_mode: bool,
}

#[derive(Clone, Debug, Resource)]
pub struct BoardLayout {
    pub tiles: Vec<TileConfig>,
}

impl BoardLayout {
    pub fn route_len(&self) -> usize {
        self.tiles.len()
    }

    pub fn world_pos_for_route_index(&self, route_index: u8) -> Option<Vec2> {
        self.tiles
            .iter()
            .find(|tile| tile.route_index == Some(route_index))
            .map(|tile| tile.world_pos)
    }

    pub fn tile_kind_for_route_index(&self, route_index: u8) -> Option<TileKind> {
        self.tiles
            .iter()
            .find(|tile| tile.route_index == Some(route_index))
            .map(|tile| tile.kind)
    }
}

#[derive(Clone, Debug, Resource)]
pub struct PlayerRoster {
    pub players: Vec<PlayerProfile>,
}

#[derive(Clone, Debug)]
pub struct PlayerProfile {
    pub state: PlayerState,
    pub color: Color,
    pub hangar_slots: Vec<Vec2>,
    pub launch_tile_index: u8,
    pub home_lane_positions: Vec<Vec2>,
    pub goal_position: Vec2,
}

#[derive(Clone, Debug, Resource)]
pub struct TeamRoster {
    pub teams: Vec<TeamState>,
}

fn prepare_match(
    mut commands: Commands,
    mut next_app_state: ResMut<NextState<AppState>>,
    mut next_game_phase: ResMut<NextState<GamePhase>>,
) {
    commands.insert_resource(MatchConfig {
        mode: GameMode::TwoVsTwo,
        ai_difficulty: AiDifficulty::Normal,
        fast_mode: false,
    });
    commands.insert_resource(BoardLayout {
        tiles: default_board_tiles(),
    });
    let (players, teams) = build_match_rosters(GameMode::TwoVsTwo);
    commands.insert_resource(PlayerRoster { players });
    commands.insert_resource(TeamRoster { teams });
    commands.insert_resource(TurnState::opening_turn());

    next_game_phase.set(GamePhase::AwaitDice);
    next_app_state.set(AppState::InGame);
}

fn build_match_rosters(mode: GameMode) -> (Vec<PlayerProfile>, Vec<TeamState>) {
    match mode {
        GameMode::OneVsOne => (
            vec![
                PlayerProfile {
                    state: PlayerState {
                        player_id: 1,
                        team_id: 1,
                        control: PlayerControl::Human,
                    },
                    color: Color::srgb(0.88, 0.30, 0.26),
                    hangar_slots: vec![Vec2::new(-320.0, 280.0), Vec2::new(-260.0, 280.0)],
                    launch_tile_index: 30,
                    home_lane_positions: vec![
                        Vec2::new(-128.0, 192.0),
                        Vec2::new(-128.0, 128.0),
                        Vec2::new(-128.0, 64.0),
                        Vec2::new(-128.0, 0.0),
                    ],
                    goal_position: Vec2::new(-64.0, 0.0),
                },
                PlayerProfile {
                    state: PlayerState {
                        player_id: 2,
                        team_id: 2,
                        control: PlayerControl::Ai,
                    },
                    color: Color::srgb(0.28, 0.50, 0.90),
                    hangar_slots: vec![Vec2::new(260.0, -280.0), Vec2::new(320.0, -280.0)],
                    launch_tile_index: 6,
                    home_lane_positions: vec![
                        Vec2::new(192.0, 128.0),
                        Vec2::new(128.0, 128.0),
                        Vec2::new(64.0, 128.0),
                        Vec2::new(0.0, 128.0),
                    ],
                    goal_position: Vec2::new(0.0, 64.0),
                },
            ],
            vec![
                TeamState {
                    team_id: 1,
                    player_ids: vec![1],
                },
                TeamState {
                    team_id: 2,
                    player_ids: vec![2],
                },
            ],
        ),
        GameMode::TwoVsTwo => (
            vec![
                PlayerProfile {
                    state: PlayerState {
                        player_id: 1,
                        team_id: 1,
                        control: PlayerControl::Human,
                    },
                    color: Color::srgb(0.88, 0.30, 0.26),
                    hangar_slots: vec![Vec2::new(-320.0, 280.0)],
                    launch_tile_index: 30,
                    home_lane_positions: vec![
                        Vec2::new(-128.0, 192.0),
                        Vec2::new(-128.0, 128.0),
                        Vec2::new(-128.0, 64.0),
                        Vec2::new(-128.0, 0.0),
                    ],
                    goal_position: Vec2::new(-64.0, 0.0),
                },
                PlayerProfile {
                    state: PlayerState {
                        player_id: 2,
                        team_id: 2,
                        control: PlayerControl::Ai,
                    },
                    color: Color::srgb(0.28, 0.50, 0.90),
                    hangar_slots: vec![Vec2::new(320.0, 280.0)],
                    launch_tile_index: 6,
                    home_lane_positions: vec![
                        Vec2::new(192.0, 128.0),
                        Vec2::new(128.0, 128.0),
                        Vec2::new(64.0, 128.0),
                        Vec2::new(0.0, 128.0),
                    ],
                    goal_position: Vec2::new(0.0, 64.0),
                },
                PlayerProfile {
                    state: PlayerState {
                        player_id: 3,
                        team_id: 1,
                        control: PlayerControl::Human,
                    },
                    color: Color::srgb(0.97, 0.78, 0.25),
                    hangar_slots: vec![Vec2::new(-320.0, -280.0)],
                    launch_tile_index: 22,
                    home_lane_positions: vec![
                        Vec2::new(-192.0, -128.0),
                        Vec2::new(-128.0, -128.0),
                        Vec2::new(-64.0, -128.0),
                        Vec2::new(0.0, -128.0),
                    ],
                    goal_position: Vec2::new(0.0, -64.0),
                },
                PlayerProfile {
                    state: PlayerState {
                        player_id: 4,
                        team_id: 2,
                        control: PlayerControl::Ai,
                    },
                    color: Color::srgb(0.26, 0.74, 0.47),
                    hangar_slots: vec![Vec2::new(320.0, -280.0)],
                    launch_tile_index: 14,
                    home_lane_positions: vec![
                        Vec2::new(128.0, -192.0),
                        Vec2::new(128.0, -128.0),
                        Vec2::new(128.0, -64.0),
                        Vec2::new(128.0, 0.0),
                    ],
                    goal_position: Vec2::new(64.0, 0.0),
                },
            ],
            vec![
                TeamState {
                    team_id: 1,
                    player_ids: vec![1, 3],
                },
                TeamState {
                    team_id: 2,
                    player_ids: vec![2, 4],
                },
            ],
        ),
    }
}
