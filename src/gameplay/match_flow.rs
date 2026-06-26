use bevy::prelude::*;

use crate::data::board_config::{TileConfig, default_board_tiles};
use crate::data::game_mode::GameMode;
use crate::domain::player::{PlayerControl, PlayerState};
use crate::domain::rules::LaunchRule;
use crate::domain::team::TeamState;
use crate::domain::tile::TileKind;
use crate::domain::victory::all_pieces_finished;
use crate::gameplay::ai::AiDifficulty;

#[derive(Clone, Debug, Resource)]
/// 对局运行时配置（进入 InGame 后使用的不可变快照）。
pub struct MatchConfig {
    pub mode: GameMode,
    pub ai_difficulty: AiDifficulty,
    pub fast_mode: bool,
    pub launch_rule: LaunchRule,
    pub player_seats: [PlayerSeat; 4],
    pub pieces_per_player: u8,
    pub player_controls: [PlayerControl; 4],
}

#[derive(Clone, Debug, Resource)]
/// 对局开始前可编辑的配置（菜单页使用）。
pub struct MatchSetup {
    pub mode: GameMode,
    pub ai_difficulty: AiDifficulty,
    pub fast_mode: bool,
    pub launch_rule: LaunchRule,
    pub player_seats: [PlayerSeat; 4],
    pub pieces_per_player: u8,
    pub player_controls: [PlayerControl; 4],
}

impl MatchSetup {
    /// 返回当前模式下参与对局的玩家数量（1v1=2，2v2=4）。
    pub fn active_player_count(&self) -> usize {
        match self.mode {
            GameMode::OneVsOne => 2,
            GameMode::TwoVsTwo | GameMode::FreeForAll => 4,
        }
    }

    /// 标准化人机配置：强制至少保留 1 名人类玩家。
    pub fn normalized_player_controls(&self) -> [PlayerControl; 4] {
        let mut controls = self.player_controls;
        let active_count = self.active_player_count();
        if controls[..active_count]
            .iter()
            .all(|control| matches!(control, PlayerControl::Ai))
        {
            controls[0] = PlayerControl::Human;
        }
        controls
    }

    /// 读取指定序号玩家的人机类型。
    pub fn player_control(&self, player_index: usize) -> Option<PlayerControl> {
        self.player_controls.get(player_index).copied()
    }

    /// 原地修正人机配置（主要在模式切换与开局前调用）。
    pub fn sanitize_player_controls(&mut self) {
        self.player_controls = self.normalized_player_controls();
    }

    /// 返回去重后的玩家座位；若配置异常则按固定棋盘座位补齐。
    pub fn normalized_player_seats(&self) -> [PlayerSeat; 4] {
        let mut seats = self.player_seats;
        let mut used = Vec::with_capacity(seats.len());

        for (index, seat) in seats.iter_mut().enumerate() {
            if used.contains(seat) {
                *seat = PlayerSeat::ALL
                    .iter()
                    .copied()
                    .find(|choice| !used.contains(choice))
                    .unwrap_or(PlayerSeat::ALL[index]);
            }
            used.push(*seat);
        }

        seats
    }

    /// 原地修正玩家座位，保证四名玩家座位不重复。
    pub fn sanitize_player_seats(&mut self) {
        self.player_seats = self.normalized_player_seats();
    }

    /// 读取指定序号玩家座位。
    pub fn player_seat(&self, player_index: usize) -> Option<PlayerSeat> {
        self.player_seats.get(player_index).copied()
    }

    /// 设置指定玩家座位；若座位已被其它玩家使用，则两名玩家交换座位。
    pub fn set_player_seat(&mut self, player_index: usize, seat: PlayerSeat) {
        if player_index >= self.player_seats.len() {
            return;
        }

        if let Some(owner_index) =
            self.player_seats
                .iter()
                .enumerate()
                .find_map(|(index, selected)| {
                    (index != player_index && *selected == seat).then_some(index)
                })
        {
            self.player_seats[owner_index] = self.player_seats[player_index];
        }

        self.player_seats[player_index] = seat;
    }

    /// 切换指定玩家的人机类型；若会导致“全 AI”则拒绝本次切换。
    pub fn toggle_player_control(&mut self, player_index: usize) {
        if player_index >= self.active_player_count() {
            return;
        }

        let mut controls = self.player_controls;
        let current = controls[player_index];
        controls[player_index] = match current {
            PlayerControl::Human => PlayerControl::Ai,
            PlayerControl::Ai => PlayerControl::Human,
        };

        let active_count = self.active_player_count();
        if controls[..active_count]
            .iter()
            .all(|control| matches!(control, PlayerControl::Ai))
        {
            return;
        }

        self.player_controls = controls;
    }

    /// 直接设置指定玩家的人机类型；同样受“不能全 AI”约束。
    pub fn set_player_control(&mut self, player_index: usize, control: PlayerControl) {
        if player_index >= self.active_player_count() {
            return;
        }

        let mut controls = self.player_controls;
        controls[player_index] = control;

        let active_count = self.active_player_count();
        if controls[..active_count]
            .iter()
            .all(|selected| matches!(selected, PlayerControl::Ai))
        {
            return;
        }

        self.player_controls = controls;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlayerSeat {
    Blue,
    Red,
    Green,
    Yellow,
}

impl PlayerSeat {
    /// 固定棋盘座位顺序：Blue=旧 P1，Red=旧 P2，Green=旧 P3，Yellow=旧 P4。
    pub const ALL: [Self; 4] = [Self::Blue, Self::Red, Self::Green, Self::Yellow];

    /// 循环到下一个可选座位。
    pub fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|choice| *choice == self)
            .unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    /// 返回座位在 UI 中展示的颜色名称。
    pub fn label(self) -> &'static str {
        match self {
            Self::Blue => "Blue",
            Self::Red => "Red",
            Self::Green => "Green",
            Self::Yellow => "Yellow",
        }
    }

    /// 返回座位槽位下标，供棋盘路径与同色跳跃判断使用。
    pub fn slot_index(self) -> usize {
        match self {
            Self::Blue => 0,
            Self::Red => 1,
            Self::Green => 2,
            Self::Yellow => 3,
        }
    }

    /// 返回颜色值，供棋子与配置面板渲染使用。
    pub fn to_color(self) -> Color {
        match self {
            Self::Blue => Color::srgb(0.0, 128.0 / 255.0, 1.0),
            Self::Red => Color::srgb(1.0, 0.0, 0.0),
            Self::Green => Color::srgb(0.0, 128.0 / 255.0, 0.0),
            Self::Yellow => Color::srgb(243.0 / 255.0, 216.0 / 255.0, 73.0 / 255.0),
        }
    }
}

#[derive(Clone, Debug, Resource)]
/// 棋盘布局资源：包含主环道格子定义。
pub struct BoardLayout {
    pub tiles: Vec<TileConfig>,
}

impl BoardLayout {
    /// 构建默认棋盘布局（主环道 + 特殊格）。
    pub fn default() -> Self {
        Self {
            tiles: default_board_tiles(),
        }
    }

    /// 返回主环道长度。
    pub fn route_len(&self) -> usize {
        self.tiles.len()
    }

    /// 根据环道索引查询世界坐标。
    pub fn world_pos_for_route_index(&self, route_index: u8) -> Option<Vec2> {
        self.tiles
            .iter()
            .find(|tile| tile.route_index == Some(route_index))
            .map(|tile| tile.world_pos)
    }

    /// 根据环道索引查询格子类型。
    pub fn tile_kind_for_route_index(&self, route_index: u8) -> Option<TileKind> {
        self.tiles
            .iter()
            .find(|tile| tile.route_index == Some(route_index))
            .map(|tile| tile.kind)
    }

    /// 查询主环道格子对应的玩家颜色槽位。
    pub fn player_color_slot_for_route_index(&self, route_index: u8) -> Option<usize> {
        self.tiles
            .iter()
            .find(|tile| tile.route_index == Some(route_index))
            .map(|tile| tile.player_color_slot)
    }

    /// 查询指定主环道格子的飞跃快捷目标（若存在）。
    pub fn jump_shortcut_target_for_route_index(&self, route_index: u8) -> Option<u8> {
        self.tiles
            .iter()
            .find(|tile| tile.route_index == Some(route_index))
            .and_then(|tile| tile.jump_shortcut_to)
    }
}

#[derive(Clone, Debug, Resource)]
/// 玩家列表资源。
pub struct PlayerRoster {
    pub players: Vec<PlayerProfile>,
    pub player_colors: [Color; 4],
}

impl PlayerRoster {
    /// 构建测试或临时玩家列表：保留固定四色棋盘调色板。
    pub fn from_players(players: Vec<PlayerProfile>) -> Self {
        Self {
            players,
            player_colors: PlayerSeat::ALL.map(PlayerSeat::to_color),
        }
    }
}

#[derive(Clone, Debug)]
/// 单个玩家的完整对局档案（颜色、机库、起点、冲线道等）。
pub struct PlayerProfile {
    pub state: PlayerState,
    pub seat: PlayerSeat,
    pub color: Color,
    pub hangar_slots: Vec<Vec2>,
    pub launch_position: Vec2,
    pub launch_tile_index: u8,
    pub home_lane_positions: Vec<Vec2>,
    pub goal_position: Vec2,
}

#[derive(Clone, Debug, Resource)]
/// 队伍列表资源。
pub struct TeamRoster {
    pub teams: Vec<TeamState>,
}

#[derive(Clone, Debug, Default, Resource)]
/// 对局胜负结果资源。
pub struct MatchResult {
    pub winner_team_id: Option<u8>,
    pub winner_player_ids: Vec<u8>,
    pub finished: bool,
}

pub const HANGAR_SLOT_OFFSETS: [Vec2; 4] = [
    Vec2::new(-35.0, 35.0),
    Vec2::new(35.0, 35.0),
    Vec2::new(-35.0, -35.0),
    Vec2::new(35.0, -35.0),
];

pub fn hangar_center_for_seat(seat: PlayerSeat) -> Vec2 {
    match seat {
        PlayerSeat::Blue => Vec2::new(-265.104, 265.104),
        PlayerSeat::Red => Vec2::new(265.317, 265.104),
        PlayerSeat::Green => Vec2::new(-265.104, -265.104),
        PlayerSeat::Yellow => Vec2::new(265.104, -265.104),
    }
}

pub fn turn_marker_position_for_seat(seat: PlayerSeat) -> Vec2 {
    match seat {
        PlayerSeat::Blue => Vec2::new(-300.104, -0.104),
        PlayerSeat::Red => Vec2::new(-0.104, 300.104),
        PlayerSeat::Green => Vec2::new(0.104, -300.104),
        PlayerSeat::Yellow => Vec2::new(300.317, 0.104),
    }
}

pub fn player_for_seat<'a>(
    player_roster: &'a PlayerRoster,
    seat: PlayerSeat,
) -> Option<&'a PlayerProfile> {
    player_roster
        .players
        .iter()
        .find(|player| player.seat == seat)
}

/// 构建开局所需资源：棋盘、玩家列表、队伍列表。
pub fn build_match_resources(setup: &MatchSetup) -> (BoardLayout, PlayerRoster, TeamRoster) {
    let (players, teams) = build_match_rosters(setup);
    let player_colors = PlayerSeat::ALL.map(PlayerSeat::to_color);
    (
        BoardLayout::default(),
        PlayerRoster {
            players,
            player_colors,
        },
        TeamRoster { teams },
    )
}

/// 根据开局配置生成玩家与队伍编排（包含座位、起点、机库位置）。
pub fn build_match_rosters(setup: &MatchSetup) -> (Vec<PlayerProfile>, Vec<TeamState>) {
    let player_seats = setup.normalized_player_seats();
    let pieces_per_player = setup.pieces_per_player.clamp(1, 4) as usize;
    let player_controls = setup.normalized_player_controls();
    let players = active_player_ids_for_mode(setup.mode)
        .iter()
        .map(|player_id| {
            let player_index = (*player_id - 1) as usize;
            build_player_profile(
                *player_id,
                team_id_for_player(setup.mode, *player_id),
                player_controls[player_index],
                player_seats[player_index],
                pieces_per_player,
            )
        })
        .collect();
    let teams = team_roster_for_mode(setup.mode);

    (players, teams)
}

fn active_player_ids_for_mode(mode: GameMode) -> &'static [u8] {
    match mode {
        GameMode::OneVsOne => &[1, 2],
        GameMode::TwoVsTwo | GameMode::FreeForAll => &[1, 2, 3, 4],
    }
}

fn team_roster_for_mode(mode: GameMode) -> Vec<TeamState> {
    match mode {
        GameMode::OneVsOne => vec![
            TeamState {
                team_id: 1,
                player_ids: vec![1],
            },
            TeamState {
                team_id: 2,
                player_ids: vec![2],
            },
        ],
        GameMode::TwoVsTwo => vec![
            TeamState {
                team_id: 1,
                player_ids: vec![1, 3],
            },
            TeamState {
                team_id: 2,
                player_ids: vec![2, 4],
            },
        ],
        GameMode::FreeForAll => (1..=4)
            .map(|player_id| TeamState {
                team_id: player_id,
                player_ids: vec![player_id],
            })
            .collect(),
    }
}

fn team_id_for_player(mode: GameMode, player_id: u8) -> u8 {
    match mode {
        GameMode::OneVsOne | GameMode::FreeForAll => player_id,
        GameMode::TwoVsTwo => {
            if player_id % 2 == 1 {
                1
            } else {
                2
            }
        }
    }
}

fn build_player_profile(
    player_id: u8,
    team_id: u8,
    control: PlayerControl,
    seat: PlayerSeat,
    pieces_per_player: usize,
) -> PlayerProfile {
    let (launch_position, launch_tile_index, home_lane_positions, goal_position) = match seat {
        PlayerSeat::Blue => (
            Vec2::new(-316.104, 156.104),
            39,
            vec![
                Vec2::new(-300.104, -0.104),
                Vec2::new(-240.104, -0.104),
                Vec2::new(-200.104, -0.104),
                Vec2::new(-160.104, -0.104),
                Vec2::new(-120.104, -0.104),
                Vec2::new(-80.104, -0.104),
            ],
            Vec2::new(-35.958, 0.0),
        ),
        PlayerSeat::Red => (
            Vec2::new(155.896, 316.104),
            3,
            vec![
                Vec2::new(-0.104, 300.104),
                Vec2::new(-0.104, 240.104),
                Vec2::new(-0.104, 200.104),
                Vec2::new(-0.104, 160.104),
                Vec2::new(-0.104, 120.104),
                Vec2::new(-0.104, 80.104),
            ],
            Vec2::new(0.0, 35.958),
        ),
        PlayerSeat::Green => (
            Vec2::new(-156.104, -315.896),
            27,
            vec![
                Vec2::new(0.104, -300.104),
                Vec2::new(0.104, -240.104),
                Vec2::new(0.104, -200.104),
                Vec2::new(0.104, -160.104),
                Vec2::new(0.104, -120.104),
                Vec2::new(0.104, -80.104),
            ],
            Vec2::new(0.0, -35.958),
        ),
        PlayerSeat::Yellow => (
            Vec2::new(315.896, -155.896),
            15,
            vec![
                Vec2::new(300.317, 0.104),
                Vec2::new(240.317, 0.104),
                Vec2::new(200.317, 0.104),
                Vec2::new(160.317, 0.104),
                Vec2::new(120.317, 0.104),
                Vec2::new(80.317, 0.104),
            ],
            Vec2::new(35.959, 0.0),
        ),
    };

    PlayerProfile {
        state: PlayerState {
            player_id,
            team_id,
            control,
        },
        seat,
        color: seat.to_color(),
        hangar_slots: build_hangar_slots(hangar_center_for_seat(seat), pieces_per_player),
        launch_position,
        launch_tile_index,
        home_lane_positions,
        goal_position,
    }
}

/// 生成机库停机位坐标（最多 4 格）。
fn build_hangar_slots(anchor: Vec2, pieces_per_player: usize) -> Vec<Vec2> {
    HANGAR_SLOT_OFFSETS
        .iter()
        .take(pieces_per_player)
        .map(|offset| anchor + *offset)
        .collect()
}

/// 按队伍完成情况计算是否出现胜方。
pub fn evaluate_match_result(
    team_roster: &TeamRoster,
    player_completion: &[(u8, bool)],
) -> MatchResult {
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

    #[test]
    fn one_vs_one_roster_has_two_players_and_two_pieces() {
        let (players, teams) = build_match_rosters(&setup(GameMode::OneVsOne));

        assert_eq!(players.len(), 2);
        assert_eq!(teams.len(), 2);
        assert_eq!(players[0].hangar_slots.len(), 2);
        assert_eq!(players[1].hangar_slots.len(), 2);
    }

    #[test]
    fn one_vs_one_resources_keep_full_board_palette() {
        let mut one_vs_one_setup = setup(GameMode::OneVsOne);
        one_vs_one_setup.player_seats = [
            PlayerSeat::Red,
            PlayerSeat::Blue,
            PlayerSeat::Green,
            PlayerSeat::Yellow,
        ];
        let (_, player_roster, _) = build_match_resources(&one_vs_one_setup);

        assert_eq!(player_roster.players.len(), 2);
        assert_eq!(player_roster.player_colors[0], PlayerSeat::Blue.to_color());
        assert_eq!(player_roster.player_colors[1], PlayerSeat::Red.to_color());
        assert_eq!(player_roster.player_colors[2], PlayerSeat::Green.to_color());
        assert_eq!(
            player_roster.player_colors[3],
            PlayerSeat::Yellow.to_color()
        );
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
        assert_eq!(
            players
                .iter()
                .map(|player| player.state.team_id)
                .collect::<Vec<_>>(),
            vec![1, 2, 1, 2]
        );
        assert!(players.iter().all(|player| player.hangar_slots.len() == 3));
    }

    #[test]
    fn one_vs_one_players_can_choose_any_two_different_seats() {
        let mut one_vs_one_setup = setup(GameMode::OneVsOne);
        one_vs_one_setup.player_seats = [
            PlayerSeat::Yellow,
            PlayerSeat::Green,
            PlayerSeat::Blue,
            PlayerSeat::Red,
        ];
        let (players, teams) = build_match_rosters(&one_vs_one_setup);

        assert_eq!(players.len(), 2);
        assert_eq!(
            players
                .iter()
                .map(|player| (
                    player.state.player_id,
                    player.seat,
                    player.launch_tile_index
                ))
                .collect::<Vec<_>>(),
            vec![(1, PlayerSeat::Yellow, 15), (2, PlayerSeat::Green, 27)]
        );
        assert_eq!(teams[0].player_ids, vec![1]);
        assert_eq!(teams[1].player_ids, vec![2]);
    }

    #[test]
    fn two_vs_two_team_identity_is_independent_from_seats() {
        let mut two_vs_two_setup = setup(GameMode::TwoVsTwo);
        two_vs_two_setup.player_seats = [
            PlayerSeat::Red,
            PlayerSeat::Yellow,
            PlayerSeat::Blue,
            PlayerSeat::Green,
        ];
        let (players, teams) = build_match_rosters(&two_vs_two_setup);

        assert_eq!(teams[0].player_ids, vec![1, 3]);
        assert_eq!(teams[1].player_ids, vec![2, 4]);
        assert_eq!(
            players
                .iter()
                .map(|player| (player.state.player_id, player.state.team_id, player.seat))
                .collect::<Vec<_>>(),
            vec![
                (1, 1, PlayerSeat::Red),
                (2, 2, PlayerSeat::Yellow),
                (3, 1, PlayerSeat::Blue),
                (4, 2, PlayerSeat::Green),
            ]
        );
    }

    #[test]
    fn free_for_all_roster_has_four_players_and_no_shared_teams() {
        let (players, teams) = build_match_rosters(&setup(GameMode::FreeForAll));

        assert_eq!(players.len(), 4);
        assert_eq!(teams.len(), 4);
        assert_eq!(teams[0].player_ids, vec![1]);
        assert_eq!(teams[1].player_ids, vec![2]);
        assert_eq!(teams[2].player_ids, vec![3]);
        assert_eq!(teams[3].player_ids, vec![4]);
        assert_eq!(
            players
                .iter()
                .map(|player| player.state.team_id)
                .collect::<Vec<_>>(),
            vec![1, 2, 3, 4]
        );
    }

    #[test]
    fn roster_hangar_slots_share_visual_hangar_centers() {
        let mut two_vs_two_setup = setup(GameMode::TwoVsTwo);
        two_vs_two_setup.pieces_per_player = 4;
        let (players, _) = build_match_rosters(&two_vs_two_setup);

        for player in players {
            let center = hangar_center_for_seat(player.seat);
            assert_eq!(player.hangar_slots.len(), HANGAR_SLOT_OFFSETS.len());
            for (slot, offset) in player.hangar_slots.iter().zip(HANGAR_SLOT_OFFSETS) {
                assert_eq!(*slot, center + offset);
            }
        }
    }

    #[test]
    fn roster_uses_configured_unique_player_seats() {
        let mut two_vs_two_setup = setup(GameMode::TwoVsTwo);
        two_vs_two_setup.player_seats = [
            PlayerSeat::Yellow,
            PlayerSeat::Green,
            PlayerSeat::Blue,
            PlayerSeat::Red,
        ];
        let (players, _) = build_match_rosters(&two_vs_two_setup);

        assert_eq!(players[0].seat, PlayerSeat::Yellow);
        assert_eq!(players[0].launch_tile_index, 15);
        assert_eq!(players[0].color, PlayerSeat::Yellow.to_color());
        assert_eq!(players[1].seat, PlayerSeat::Green);
        assert_eq!(players[1].launch_tile_index, 27);
        assert_eq!(players[1].color, PlayerSeat::Green.to_color());
        assert_eq!(players[2].seat, PlayerSeat::Blue);
        assert_eq!(players[2].launch_tile_index, 39);
        assert_eq!(players[2].color, PlayerSeat::Blue.to_color());
        assert_eq!(players[3].seat, PlayerSeat::Red);
        assert_eq!(players[3].launch_tile_index, 3);
        assert_eq!(players[3].color, PlayerSeat::Red.to_color());
    }

    #[test]
    fn player_seat_decides_profile_position() {
        let mut one_vs_one_setup = setup(GameMode::OneVsOne);
        one_vs_one_setup.set_player_seat(0, PlayerSeat::Red);
        let (players, _) = build_match_rosters(&one_vs_one_setup);
        let player_one = players
            .iter()
            .find(|player| player.state.player_id == 1)
            .expect("P1 participates");

        assert_eq!(player_one.seat, PlayerSeat::Red);
        assert_eq!(player_one.color, PlayerSeat::Red.to_color());
        assert_eq!(player_one.launch_position, Vec2::new(155.896, 316.104));
        assert_eq!(player_one.launch_tile_index, 3);
        assert_eq!(
            player_one.home_lane_positions.first().copied(),
            Some(Vec2::new(-0.104, 300.104))
        );
        assert_eq!(
            player_one.hangar_slots.first().copied(),
            Some(hangar_center_for_seat(PlayerSeat::Red) + HANGAR_SLOT_OFFSETS[0])
        );
    }

    #[test]
    fn setting_an_used_player_seat_swaps_seats() {
        let mut two_vs_two_setup = setup(GameMode::TwoVsTwo);

        two_vs_two_setup.set_player_seat(0, PlayerSeat::Red);

        assert_eq!(
            two_vs_two_setup.player_seats,
            [
                PlayerSeat::Red,
                PlayerSeat::Blue,
                PlayerSeat::Green,
                PlayerSeat::Yellow,
            ]
        );
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

    #[test]
    fn free_for_all_victory_is_awarded_to_the_finished_player_only() {
        let (_, teams) = build_match_rosters(&setup(GameMode::FreeForAll));
        let team_roster = TeamRoster { teams };

        let player_three_finished = evaluate_match_result(
            &team_roster,
            &[(1, false), (2, false), (3, true), (4, false)],
        );

        assert!(player_three_finished.finished);
        assert_eq!(player_three_finished.winner_team_id, Some(3));
        assert_eq!(player_three_finished.winner_player_ids, vec![3]);
    }

    #[test]
    fn one_vs_one_roster_uses_configured_player_controls() {
        let mut one_vs_one_setup = setup(GameMode::OneVsOne);
        one_vs_one_setup.player_controls = [
            PlayerControl::Ai,
            PlayerControl::Human,
            PlayerControl::Ai,
            PlayerControl::Ai,
        ];
        let (players, _) = build_match_rosters(&one_vs_one_setup);

        assert_eq!(players[0].state.control, PlayerControl::Ai);
        assert_eq!(players[1].state.control, PlayerControl::Human);
    }

    #[test]
    fn one_vs_one_roster_never_allows_all_ai() {
        let mut one_vs_one_setup = setup(GameMode::OneVsOne);
        one_vs_one_setup.player_controls = [
            PlayerControl::Ai,
            PlayerControl::Ai,
            PlayerControl::Human,
            PlayerControl::Human,
        ];
        let (players, _) = build_match_rosters(&one_vs_one_setup);

        assert_eq!(players[0].state.control, PlayerControl::Human);
        assert_eq!(players[1].state.control, PlayerControl::Ai);
    }
}
