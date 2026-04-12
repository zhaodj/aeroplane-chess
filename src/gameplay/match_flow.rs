use bevy::prelude::*;

use crate::data::board_config::{default_board_tiles, TileConfig};
use crate::data::game_mode::GameMode;
use crate::domain::player::{PlayerControl, PlayerState};
use crate::domain::team::TeamState;
use crate::domain::tile::TileKind;
use crate::domain::victory::all_pieces_finished;
use crate::gameplay::ai::AiDifficulty;

#[derive(Clone, Debug, Resource)]
pub struct MatchConfig {
    pub mode: GameMode,
    pub ai_difficulty: AiDifficulty,
    pub fast_mode: bool,
    pub human_color: PlayerColorChoice,
    pub pieces_per_player: u8,
}

#[derive(Clone, Debug, Resource)]
pub struct MatchSetup {
    pub mode: GameMode,
    pub ai_difficulty: AiDifficulty,
    pub fast_mode: bool,
    pub human_color: PlayerColorChoice,
    pub pieces_per_player: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlayerColorChoice {
    Crimson,
    Amber,
    Lime,
    Cyan,
    Violet,
    Rose,
}

impl PlayerColorChoice {
    pub const ALL: [Self; 6] = [
        Self::Crimson,
        Self::Amber,
        Self::Lime,
        Self::Cyan,
        Self::Violet,
        Self::Rose,
    ];

    pub fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|choice| *choice == self)
            .unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Crimson => "Crimson",
            Self::Amber => "Amber",
            Self::Lime => "Lime",
            Self::Cyan => "Cyan",
            Self::Violet => "Violet",
            Self::Rose => "Rose",
        }
    }

    fn to_color(self) -> Color {
        match self {
            Self::Crimson => Color::srgb(0.88, 0.30, 0.26),
            Self::Amber => Color::srgb(0.96, 0.66, 0.22),
            Self::Lime => Color::srgb(0.52, 0.78, 0.28),
            Self::Cyan => Color::srgb(0.24, 0.72, 0.86),
            Self::Violet => Color::srgb(0.53, 0.44, 0.92),
            Self::Rose => Color::srgb(0.94, 0.43, 0.62),
        }
    }

    fn enemy_colors(self) -> (Color, Color) {
        match self {
            Self::Crimson | Self::Amber | Self::Rose => {
                (Color::srgb(0.28, 0.50, 0.90), Color::srgb(0.26, 0.74, 0.47))
            }
            Self::Lime | Self::Cyan | Self::Violet => {
                (Color::srgb(0.88, 0.30, 0.26), Color::srgb(0.96, 0.66, 0.22))
            }
        }
    }
}

#[derive(Clone, Debug, Resource)]
pub struct BoardLayout {
    pub tiles: Vec<TileConfig>,
}

impl BoardLayout {
    pub fn default() -> Self {
        Self {
            tiles: default_board_tiles(),
        }
    }

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

#[derive(Clone, Debug, Default, Resource)]
pub struct MatchResult {
    pub winner_team_id: Option<u8>,
    pub winner_player_ids: Vec<u8>,
    pub finished: bool,
}

pub fn build_match_resources(setup: &MatchSetup) -> (BoardLayout, PlayerRoster, TeamRoster) {
    let (players, teams) = build_match_rosters(setup);
    (
        BoardLayout::default(),
        PlayerRoster { players },
        TeamRoster { teams },
    )
}

pub fn build_match_rosters(setup: &MatchSetup) -> (Vec<PlayerProfile>, Vec<TeamState>) {
    let human_primary = setup.human_color.to_color();
    let human_secondary = human_primary.mix(&Color::WHITE, 0.25);
    let (enemy_primary, enemy_secondary) = setup.human_color.enemy_colors();
    let pieces_per_player = setup.pieces_per_player.clamp(1, 4) as usize;

    match setup.mode {
        GameMode::OneVsOne => (
            vec![
                PlayerProfile {
                    state: PlayerState {
                        player_id: 1,
                        team_id: 1,
                        control: PlayerControl::Human,
                    },
                    color: human_primary,
                    hangar_slots: build_hangar_slots(Vec2::new(-290.0, 280.0), pieces_per_player),
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
                    color: enemy_primary,
                    hangar_slots: build_hangar_slots(Vec2::new(290.0, -280.0), pieces_per_player),
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
                    color: human_primary,
                    hangar_slots: build_hangar_slots(Vec2::new(-320.0, 280.0), pieces_per_player),
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
                    color: enemy_primary,
                    hangar_slots: build_hangar_slots(Vec2::new(320.0, 280.0), pieces_per_player),
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
                    color: human_secondary,
                    hangar_slots: build_hangar_slots(Vec2::new(-320.0, -280.0), pieces_per_player),
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
                    color: enemy_secondary,
                    hangar_slots: build_hangar_slots(Vec2::new(320.0, -280.0), pieces_per_player),
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

fn build_hangar_slots(anchor: Vec2, pieces_per_player: usize) -> Vec<Vec2> {
    let offsets = [
        Vec2::new(-30.0, 30.0),
        Vec2::new(30.0, 30.0),
        Vec2::new(-30.0, -30.0),
        Vec2::new(30.0, -30.0),
    ];
    offsets
        .iter()
        .take(pieces_per_player)
        .map(|offset| anchor + *offset)
        .collect()
}

pub fn evaluate_match_result(team_roster: &TeamRoster, player_completion: &[(u8, bool)]) -> MatchResult {
    for team in &team_roster.teams {
        let team_finished = team
            .player_ids
            .iter()
            .map(|player_id| {
                player_completion
                    .iter()
                    .find(|(completion_player_id, _)| completion_player_id == player_id)
                    .map(|(_, finished)| *finished)
                    .unwrap_or(false)
            })
            .collect::<Vec<_>>();

        if all_pieces_finished(&team_finished) {
            return MatchResult {
                winner_team_id: Some(team.team_id),
                winner_player_ids: team.player_ids.clone(),
                finished: true,
            };
        }
    }

    MatchResult::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(mode: GameMode) -> MatchSetup {
        MatchSetup {
            mode,
            ai_difficulty: AiDifficulty::Normal,
            fast_mode: false,
            human_color: PlayerColorChoice::Crimson,
            pieces_per_player: 2,
        }
    }

    #[test]
    fn one_vs_one_roster_has_two_players_and_two_pieces() {
        let (players, teams) = build_match_rosters(&setup(GameMode::OneVsOne));

        assert_eq!(players.len(), 2);
        assert_eq!(teams.len(), 2);
        assert_eq!(players[0].hangar_slots.len(), 2);
        assert_eq!(players[1].hangar_slots.len(), 2);
    }

    #[test]
    fn two_vs_two_roster_has_four_players_and_shared_teams() {
        let mut two_vs_two_setup = setup(GameMode::TwoVsTwo);
        two_vs_two_setup.pieces_per_player = 3;
        let (players, teams) = build_match_rosters(&two_vs_two_setup);

        assert_eq!(players.len(), 4);
        assert_eq!(teams.len(), 2);
        assert_eq!(teams[0].player_ids, vec![1, 3]);
        assert_eq!(teams[1].player_ids, vec![2, 4]);
        assert!(players.iter().all(|player| player.hangar_slots.len() == 3));
    }

    #[test]
    fn victory_requires_all_team_players_finished() {
        let (_, teams) = build_match_rosters(&setup(GameMode::TwoVsTwo));
        let team_roster = TeamRoster { teams };

        let not_finished = evaluate_match_result(&team_roster, &[(1, true), (3, false)]);
        assert!(!not_finished.finished);

        let team_one_finished = evaluate_match_result(&team_roster, &[(1, true), (3, true)]);
        assert!(team_one_finished.finished);
        assert_eq!(team_one_finished.winner_team_id, Some(1));
        assert_eq!(team_one_finished.winner_player_ids, vec![1, 3]);
    }

    #[test]
    fn one_vs_one_victory_requires_the_single_player_to_finish_all_pieces() {
        let (_, teams) = build_match_rosters(&setup(GameMode::OneVsOne));
        let team_roster = TeamRoster { teams };

        let player_one_finished = evaluate_match_result(&team_roster, &[(1, true), (2, false)]);
        assert!(player_one_finished.finished);
        assert_eq!(player_one_finished.winner_team_id, Some(1));
        assert_eq!(player_one_finished.winner_player_ids, vec![1]);
    }
}
