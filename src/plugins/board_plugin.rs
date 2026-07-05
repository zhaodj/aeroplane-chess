use bevy::{
    asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology,
};
use std::f32::consts::PI;

use crate::constants::BOARD_Z_LAYER;
use crate::domain::player::PlayerControl;
use crate::domain::tile::TileKind;
use crate::gameplay::match_flow::{
    BoardLayout, HANGAR_SLOT_OFFSETS, MatchConfig, MatchResult, PlayerProfile, PlayerRoster,
    PlayerSeat, hangar_center_for_seat, player_for_seat,
};
use crate::gameplay::turn_flow::{TurnState, commit_pending_roll_display};
use crate::plugins::animation_plugin::PieceMoveAnimation;
use crate::states::{AppState, GamePhase};

/// 棋盘渲染插件：按 SVG 的几何元素重建棋盘外观。
pub struct BoardPlugin;

impl Plugin for BoardPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DiceRollVisualState>()
            .add_systems(OnEnter(AppState::InGame), spawn_board)
            .add_systems(
                Update,
                (
                    update_dice_roll_visual_state,
                    update_player_dice_displays,
                    update_center_dice_roll_displays,
                    update_center_dice_turn_halo,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(OnExit(AppState::InGame), cleanup_board);
    }
}

#[derive(Component)]
/// 棋盘场景实体标记，用于状态切换时统一清理。
struct BoardSceneEntity;

#[derive(Component)]
/// 玩家停机坪中心的骰面底板。
struct PlayerDiceDisplay {
    player_id: u8,
    die_index: u8,
    layer: PlayerDiceDisplayLayer,
    base_center: Vec2,
}

#[derive(Clone, Copy, Component, PartialEq)]
/// 骰面点位，用于按 1~6 点动态显示。
struct PlayerDicePip {
    player_id: u8,
    die_index: u8,
    slot: DicePipSlot,
    base_center: Vec2,
}

#[derive(Component)]
/// 棋盘中心的临时掷骰动画素材。
struct CenterDiceSprite {
    die_index: u8,
    base_center: Vec2,
}

#[derive(Component)]
/// 棋盘中心骰子的当前玩家色光环。
struct CenterDiceTurnHalo {
    layer: CenterDiceTurnHaloLayer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PlayerDiceDisplayLayer {
    Rim,
    Face,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PlayerDiceDisplayState {
    Hidden,
    Active([u8; 2]),
    Disabled([u8; 2]),
}

impl PlayerDiceDisplayState {
    fn faces(self) -> Option<[u8; 2]> {
        match self {
            Self::Hidden => None,
            Self::Active(faces) | Self::Disabled(faces) => Some(faces),
        }
    }

    fn roll(self, die_index: u8) -> Option<u8> {
        self.faces()
            .and_then(|faces| dice_face_for_index(faces, die_index))
    }

    fn active(self) -> bool {
        matches!(self, Self::Active(_))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CenterDiceTurnHaloLayer {
    Glow,
    Ring,
}

const PLAYER_DICE_DOUBLE_OFFSET: f32 = 20.5;
const CENTER_DICE_DOUBLE_OFFSET: f32 = 38.0;
const CENTER_DICE_SPRITE_SIZE: f32 = 66.0;
const CENTER_DICE_PROMPT_SIZE: f32 = CENTER_DICE_SPRITE_SIZE;
const CENTER_DICE_PROMPT_FRAME: usize = 0;
const CENTER_DICE_PROMPT_ALPHA: f32 = 1.0;
const CENTER_DICE_PROMPT_BASE_ROTATION: f32 = -0.12;
const CENTER_DICE_PROMPT_BOB: f32 = 4.0;
const CENTER_DICE_PROMPT_SCALE_PULSE: f32 = 0.035;
const CENTER_DICE_TURN_HALO_SEGMENTS: usize = 64;
const CENTER_DICE_TURN_HALO_GLOW_INNER_RADIUS: f32 = 42.0;
const CENTER_DICE_TURN_HALO_GLOW_OUTER_RADIUS: f32 = 55.0;
const CENTER_DICE_TURN_HALO_RING_INNER_RADIUS: f32 = 49.0;
const CENTER_DICE_TURN_HALO_RING_OUTER_RADIUS: f32 = 52.0;
const DICE_ROLL_ANIMATION_DURATION: f32 = 1.8;
const DICE_ROLL_SETTLE_START: f32 = 1.35;
const DICE_ROLL_FACE_INTERVAL: f32 = 0.045;
const DICE_ROLL_FRAME_COUNT: usize = 16;
const DICE_ROLL_MAX_SHAKE: f32 = 7.0;
const DICE_ROLL_MAX_HOP: f32 = 14.0;
const DICE_ROLL_MAX_ROTATION: f32 = 0.36;
const DICE_ROLL_MAX_SCALE_BUMP: f32 = 0.11;
const DICE_ROLL_SPIN_TURNS: f32 = 2.0;
const DICE_ROLL_TRAVEL_DISTANCE: f32 = 26.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DiceRollVisualKey {
    roll_serial: u32,
    player_id: u8,
    roll: u8,
    faces: [u8; 2],
}

#[derive(Clone, Copy, Debug)]
struct DiceRollVisualAnimation {
    key: DiceRollVisualKey,
    elapsed: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DiceRollVisualTransform {
    offset: Vec2,
    scale: Vec2,
    rotation: f32,
}

impl Default for DiceRollVisualTransform {
    fn default() -> Self {
        Self {
            offset: Vec2::ZERO,
            scale: Vec2::ONE,
            rotation: 0.0,
        }
    }
}

#[derive(Resource, Default)]
struct DiceRollVisualState {
    observed_roll: Option<DiceRollVisualKey>,
    animation: Option<DiceRollVisualAnimation>,
}

#[derive(Resource, Clone)]
struct DiceSpriteAssets {
    faces: [Handle<Image>; 6],
    roll_frames: [Handle<Image>; DICE_ROLL_FRAME_COUNT],
}

impl DiceSpriteAssets {
    fn load(asset_server: &AssetServer) -> Self {
        Self {
            faces: std::array::from_fn(|index| {
                asset_server.load(dice_face_asset_path(index as u8 + 1))
            }),
            roll_frames: std::array::from_fn(|index| {
                asset_server.load(dice_roll_frame_asset_path(index))
            }),
        }
    }

    fn face_handle(&self, roll: u8) -> Handle<Image> {
        let index = roll.saturating_sub(1).min(5) as usize;
        self.faces[index].clone()
    }

    fn roll_frame_handle(&self, frame: usize) -> Handle<Image> {
        self.roll_frames[frame % DICE_ROLL_FRAME_COUNT].clone()
    }

    fn handle_for_animation(
        &self,
        animation: DiceRollVisualAnimation,
        roll: u8,
        die_index: u8,
    ) -> Handle<Image> {
        if animation.elapsed < DICE_ROLL_SETTLE_START {
            return self.roll_frame_handle(dice_roll_sprite_frame(animation, die_index));
        }
        self.face_handle(roll)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DicePipSlot {
    Center,
    TopLeft,
    TopRight,
    MiddleLeft,
    MiddleRight,
    BottomLeft,
    BottomRight,
}

#[derive(Clone, Copy)]
/// SVG 复刻矩形图元。
struct SvgRect {
    center: Vec2,
    size: Vec2,
    fill: &'static str,
}

#[derive(Clone, Copy)]
/// SVG 复刻三角图元。
struct SvgTriangle {
    a: Vec2,
    b: Vec2,
    c: Vec2,
    fill: &'static str,
}

#[derive(Clone, Copy)]
struct DrawStyle {
    fill: Color,
    border: Color,
    border_width: f32,
    z: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct VisualRectGeometry {
    center: Vec2,
    size: Vec2,
}

#[derive(Clone, Copy)]
/// 机场旁起飞点三角图元。
struct LaunchTriangle {
    seat: PlayerSeat,
    center: Vec2,
    a: Vec2,
    b: Vec2,
    c: Vec2,
    arrow_direction: Vec2,
}

#[derive(Clone, Copy)]
/// 棋盘上的 SVG 点状图标使用原始填色键映射玩家色。
struct SvgIcon {
    center: Vec2,
    fill: &'static str,
}

#[derive(Clone, Copy)]
/// 双箭头/方向提示图标。
struct ChevronIcon {
    center: Vec2,
    fill: &'static str,
    direction: Vec2,
    count: u8,
    size: f32,
}

#[derive(Clone, Copy)]
struct DirectionIconDraw {
    center: Vec2,
    direction: Vec2,
    color: Color,
    z: f32,
}

#[derive(Clone, Copy)]
struct ChevronDraw {
    center: Vec2,
    direction: Vec2,
    count: u8,
    size: f32,
    color: Color,
    z: f32,
}

#[derive(Clone, Copy)]
struct TurnMarkerDraw {
    center: Vec2,
    direction: Vec2,
    color: Color,
    z: f32,
}

#[derive(Clone, Copy)]
struct StarDraw {
    center: Vec2,
    radius: f32,
    color: Color,
    z: f32,
}

#[derive(Clone)]
/// 棋盘四色座位：SVG 里的四个固定色只用于定位到固定棋盘槽位。
struct BoardPalette {
    player_colors: [Color; 4],
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct HomeLaneDotDraw {
    seat: PlayerSeat,
    position: Vec2,
    show_turn_marker: bool,
}

const BOARD_HOME_LANES: [[Vec2; 6]; 4] = [
    [
        Vec2::new(-300.104, -0.104),
        Vec2::new(-240.104, -0.104),
        Vec2::new(-200.104, -0.104),
        Vec2::new(-160.104, -0.104),
        Vec2::new(-120.104, -0.104),
        Vec2::new(-80.104, -0.104),
    ],
    [
        Vec2::new(-0.104, 300.104),
        Vec2::new(-0.104, 240.104),
        Vec2::new(-0.104, 200.104),
        Vec2::new(-0.104, 160.104),
        Vec2::new(-0.104, 120.104),
        Vec2::new(-0.104, 80.104),
    ],
    [
        Vec2::new(0.104, -300.104),
        Vec2::new(0.104, -240.104),
        Vec2::new(0.104, -200.104),
        Vec2::new(0.104, -160.104),
        Vec2::new(0.104, -120.104),
        Vec2::new(0.104, -80.104),
    ],
    [
        Vec2::new(300.317, 0.104),
        Vec2::new(240.317, 0.104),
        Vec2::new(200.317, 0.104),
        Vec2::new(160.317, 0.104),
        Vec2::new(120.317, 0.104),
        Vec2::new(80.317, 0.104),
    ],
];

impl BoardPalette {
    fn from_player_roster(player_roster: &PlayerRoster) -> Self {
        Self {
            player_colors: player_roster.player_colors,
        }
    }

    fn seat_color(&self, seat: PlayerSeat) -> Color {
        let player_index = seat.slot_index();
        self.player_colors
            .get(player_index)
            .copied()
            .unwrap_or(Color::srgb(0.90, 0.90, 0.90))
    }

    fn color_for_svg_fill(&self, fill: &str) -> Color {
        match fill {
            "#0080FF" => self.seat_color(PlayerSeat::Blue),
            "#FF0000" => self.seat_color(PlayerSeat::Red),
            "#008000" => self.seat_color(PlayerSeat::Green),
            "#F3D849" => self.seat_color(PlayerSeat::Yellow),
            "#F5F5F5" | "white" => Color::srgb(0.96, 0.96, 0.96),
            "black" => Color::BLACK,
            _ => Color::srgb(0.90, 0.90, 0.90),
        }
    }

    fn color_for_route_index(&self, route_index: u8) -> Color {
        self.player_colors[route_index as usize % self.player_colors.len()]
    }
}

fn spawn_board(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    asset_server: Res<AssetServer>,
    board_layout: Res<BoardLayout>,
    match_config: Res<MatchConfig>,
    player_roster: Res<PlayerRoster>,
) {
    let board_palette = BoardPalette::from_player_roster(&player_roster);

    spawn_square_with_border(
        &mut commands,
        Vec2::ZERO,
        Vec2::splat(683.0),
        DrawStyle {
            fill: board_surface_color(),
            border: board_surface_color(),
            border_width: 0.0,
            z: BOARD_Z_LAYER - 3.0,
        },
        "BoardBackdrop",
    );

    // 先画矩形网格，再画三角区域；停机坪只收窄内侧，外侧仍和跑道边缘对齐。
    for &rect in SVG_RECTS {
        if !should_draw_svg_rect(rect, &player_roster) {
            continue;
        }

        let hangar_background = is_hangar_background_rect(rect);
        let mut fill = board_palette.color_for_svg_fill(rect.fill);
        if hangar_background {
            fill = fill.mix(&Color::WHITE, 0.08);
        }
        let visual_rect = visual_rect_geometry(rect);
        spawn_square_with_border(
            &mut commands,
            visual_rect.center,
            visual_rect.size,
            DrawStyle {
                fill,
                border: if hangar_background {
                    hangar_outline_color()
                } else {
                    board_grid_line_color()
                },
                border_width: if hangar_background { 1.6 } else { 0.65 },
                z: BOARD_Z_LAYER - 1.0,
            },
            "SvgRect",
        );
    }

    for tri in SVG_TRIANGLES {
        spawn_triangle_with_border(
            &mut commands,
            &mut meshes,
            &mut materials,
            [tri.a, tri.b, tri.c],
            DrawStyle {
                fill: board_palette.color_for_svg_fill(tri.fill),
                border: board_grid_line_color(),
                border_width: 0.65,
                z: BOARD_Z_LAYER - 0.9,
            },
            "SvgTri",
        );
    }

    // 起飞点三角：背景与箭头统一绑定机场/玩家颜色。
    for launch in LAUNCH_TRIANGLES {
        let launch_color = board_palette.seat_color(launch.seat);
        spawn_triangle_with_border(
            &mut commands,
            &mut meshes,
            &mut materials,
            [launch.a, launch.b, launch.c],
            DrawStyle {
                fill: launch_color,
                border: board_grid_line_color(),
                border_width: 0.65,
                z: BOARD_Z_LAYER - 0.75,
            },
            format!("LaunchTriangle_{}", launch.seat.label()),
        );
        spawn_circle_with_border(
            &mut commands,
            &mut meshes,
            &mut materials,
            launch.center,
            16.0,
            DrawStyle {
                fill: Color::WHITE,
                border: Color::WHITE,
                border_width: 0.0,
                z: BOARD_Z_LAYER + 0.08,
            },
            format!("LaunchDot_{}", launch.seat.label()),
        );
        spawn_arrow_icon(
            &mut commands,
            &mut meshes,
            &mut materials,
            DirectionIconDraw {
                center: launch.center,
                direction: launch.arrow_direction,
                color: launch_color,
                z: BOARD_Z_LAYER + 0.55,
            },
            format!("LaunchArrow_{}", launch.seat.label()),
        );
    }

    // 主环道圆点：严格按逻辑路径坐标绘制，保证棋子与圆心对齐。
    for tile in &board_layout.tiles {
        spawn_circle_with_border(
            &mut commands,
            &mut meshes,
            &mut materials,
            tile.world_pos,
            16.0,
            DrawStyle {
                fill: Color::WHITE,
                border: Color::WHITE,
                border_width: 0.0,
                z: BOARD_Z_LAYER + 0.05,
            },
            "TrackDot",
        );
    }

    // 主环道特殊格专属图标：攻击=剑，防御=盾，随机事件=问号。
    for tile in &board_layout.tiles {
        if let Some(route_index) = tile.route_index {
            spawn_special_tile_icon(
                &mut commands,
                &mut meshes,
                &mut materials,
                tile.world_pos,
                match_config.rule_set.effective_tile_kind(tile.kind),
                BOARD_Z_LAYER + 0.62,
                format!("SpecialTileIcon_{route_index:02}"),
            );
        }
    }

    // 冲线支路圆点：参赛玩家显示完整支路；未参赛色位只保留与主路交叉的入口格。
    for dot in visible_home_lane_dots(&player_roster) {
        spawn_circle_with_border(
            &mut commands,
            &mut meshes,
            &mut materials,
            dot.position,
            16.0,
            DrawStyle {
                fill: Color::WHITE,
                border: Color::WHITE,
                border_width: 0.0,
                z: BOARD_Z_LAYER + 0.05,
            },
            "HomeLaneDot",
        );

        if dot.show_turn_marker {
            spawn_home_lane_turn_marker(
                &mut commands,
                TurnMarkerDraw {
                    center: dot.position,
                    direction: home_lane_turn_direction(dot.seat),
                    color: board_palette.seat_color(dot.seat),
                    z: BOARD_Z_LAYER + 0.64,
                },
                format!("HomeLaneTurn_{}", dot.seat.label()),
            );
        }
    }

    // 固定机库圆槽（始终展示 4 槽，棋子数量少时仅部分被占用）。
    for seat in PlayerSeat::ALL {
        let airport_center = hangar_center_for_seat(seat);
        for offset in HANGAR_SLOT_OFFSETS {
            spawn_circle_with_border(
                &mut commands,
                &mut meshes,
                &mut materials,
                airport_center + offset,
                24.5,
                DrawStyle {
                    fill: Color::WHITE,
                    border: hangar_pad_outline_color(),
                    border_width: 1.15,
                    z: BOARD_Z_LAYER + 0.20,
                },
                "HangarPad",
            );
        }
    }

    for player in &player_roster.players {
        spawn_player_dice_display(
            &mut commands,
            &mut meshes,
            &mut materials,
            player.state.player_id,
            hangar_center_for_seat(player.seat),
        );
    }

    // 中心四向目标点。
    for icon in CENTER_STAR_ICONS {
        spawn_circle_with_border(
            &mut commands,
            &mut meshes,
            &mut materials,
            icon.center,
            17.0,
            DrawStyle {
                fill: Color::WHITE,
                border: Color::WHITE,
                border_width: 0.0,
                z: BOARD_Z_LAYER + 0.30,
            },
            "CenterNode",
        );
        spawn_star_icon(
            &mut commands,
            &mut meshes,
            &mut materials,
            StarDraw {
                center: icon.center,
                radius: 13.0,
                color: board_palette.color_for_svg_fill(icon.fill),
                z: BOARD_Z_LAYER + 0.55,
            },
            "CenterStar",
        );
    }

    // SVG 里的方向提示属于棋盘底图，而不是临时玩法调试标记。
    for icon in CHEVRON_ICONS {
        spawn_chevron_icon(
            &mut commands,
            ChevronDraw {
                center: icon.center,
                direction: icon.direction,
                count: icon.count,
                size: icon.size,
                color: board_palette.color_for_svg_fill(icon.fill),
                z: BOARD_Z_LAYER + 0.58,
            },
            "BoardChevron",
        );
    }

    let dice_sprite_assets = DiceSpriteAssets::load(&asset_server);
    spawn_center_dice_turn_halo(&mut commands, &mut meshes, &mut materials);
    spawn_center_dice_roll_display(&mut commands, &dice_sprite_assets);
    commands.insert_resource(dice_sprite_assets);
}

fn cleanup_board(
    mut commands: Commands,
    query: Query<Entity, (With<BoardSceneEntity>, Without<ChildOf>)>,
) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
    commands.remove_resource::<DiceSpriteAssets>();
}

fn update_dice_roll_visual_state(
    time: Res<Time>,
    mut turn_state: ResMut<TurnState>,
    mut visual_state: ResMut<DiceRollVisualState>,
) {
    let roll_key = visible_dice_roll_key(&turn_state);
    if visual_state.observed_roll != roll_key {
        visual_state.observed_roll = roll_key;
        visual_state.animation = roll_key.map(|key| DiceRollVisualAnimation { key, elapsed: 0.0 });
    }

    let mut completed_roll_serial = None;
    if let Some(animation) = visual_state.animation.as_mut() {
        animation.elapsed += dice_roll_animation_delta(time.delta_secs());
        if animation.elapsed >= DICE_ROLL_ANIMATION_DURATION {
            completed_roll_serial = Some(animation.key.roll_serial);
        }
    }
    if let Some(roll_serial) = completed_roll_serial {
        visual_state.animation = None;
        commit_pending_roll_display(&mut turn_state, roll_serial);
    }
}

fn update_player_dice_displays(
    turn_state: Res<TurnState>,
    animation_query: Query<(), With<PieceMoveAnimation>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut display_query: Query<(
        &PlayerDiceDisplay,
        &mut Visibility,
        &mut Transform,
        &MeshMaterial2d<ColorMaterial>,
    )>,
    mut pip_query: Query<
        (
            &PlayerDicePip,
            &mut Visibility,
            &mut Transform,
            &MeshMaterial2d<ColorMaterial>,
        ),
        Without<PlayerDiceDisplay>,
    >,
) {
    let animation_active = !animation_query.is_empty();
    for (display, mut visibility, mut transform, material_handle) in &mut display_query {
        let display_state =
            player_dice_display_state(&turn_state, display.player_id, animation_active);
        *visibility = if display_state.roll(display.die_index).is_some() {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        if let Some(faces) = display_state.faces() {
            let center = dice_center_for_index(display.base_center, faces, display.die_index);
            transform.translation.x = center.x;
            transform.translation.y = center.y;
        }
        transform.rotation = Quat::IDENTITY;
        transform.scale = Vec3::ONE;
        if let Some(mut material) = materials.get_mut(&material_handle.0) {
            material.color = player_dice_display_color(display.layer, display_state.active());
        }
    }

    for (pip, mut visibility, mut transform, material_handle) in &mut pip_query {
        let display_state = player_dice_display_state(&turn_state, pip.player_id, animation_active);
        let Some(roll) = display_state.roll(pip.die_index) else {
            *visibility = Visibility::Hidden;
            transform.scale = Vec3::ONE;
            continue;
        };
        *visibility = if pip_visible_for_roll(roll, pip.slot) {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        if let Some(faces) = display_state.faces() {
            let center = dice_center_for_index(pip.base_center, faces, pip.die_index)
                + dice_pip_offset(pip.slot);
            transform.translation.x = center.x;
            transform.translation.y = center.y;
        }
        transform.rotation = Quat::IDENTITY;
        transform.scale = Vec3::ONE;
        if let Some(mut material) = materials.get_mut(&material_handle.0) {
            material.color = player_dice_pip_color(display_state.active());
        }
    }
}

fn update_center_dice_roll_displays(
    time: Res<Time>,
    dice_visual_state: Res<DiceRollVisualState>,
    dice_sprite_assets: Res<DiceSpriteAssets>,
    game_phase: Res<State<GamePhase>>,
    turn_state: Res<TurnState>,
    player_roster: Res<PlayerRoster>,
    match_result: Res<MatchResult>,
    mut dice_query: Query<(
        &CenterDiceSprite,
        &mut Sprite,
        &mut Visibility,
        &mut Transform,
    )>,
) {
    let Some(animation) = dice_visual_state.animation else {
        if let Some(choice) = turn_state.pending_double_dice_choice {
            render_center_dice_faces(choice.faces, &dice_sprite_assets, &mut dice_query);
            return;
        }
        if center_dice_prompt_visible(&turn_state, game_phase.get(), &player_roster, &match_result)
        {
            render_center_dice_prompt(time.elapsed_secs(), &dice_sprite_assets, &mut dice_query);
        } else {
            hide_center_dice_displays(&mut dice_query);
        }
        return;
    };

    let faces = dice_roll_animation_faces(animation);
    for (display, mut sprite, mut visibility, mut transform) in &mut dice_query {
        let Some(roll) = dice_face_for_index(faces, display.die_index) else {
            *visibility = Visibility::Hidden;
            transform.rotation = Quat::IDENTITY;
            transform.scale = Vec3::ONE;
            continue;
        };
        sprite.custom_size = Some(Vec2::splat(CENTER_DICE_SPRITE_SIZE));
        sprite.color = Color::WHITE;
        sprite.image = dice_sprite_assets.handle_for_animation(animation, roll, display.die_index);
        *visibility = Visibility::Visible;
        let visual_transform = dice_roll_visual_transform(animation, display.die_index);
        let center = center_dice_center_for_index(display.base_center, faces, display.die_index)
            + visual_transform.offset;
        transform.translation.x = center.x;
        transform.translation.y = center.y;
        transform.rotation = Quat::from_rotation_z(visual_transform.rotation);
        transform.scale = Vec3::new(visual_transform.scale.x, visual_transform.scale.y, 1.0);
    }
}

fn update_center_dice_turn_halo(
    time: Res<Time>,
    turn_state: Res<TurnState>,
    dice_visual_state: Res<DiceRollVisualState>,
    player_roster: Res<PlayerRoster>,
    game_phase: Res<State<GamePhase>>,
    match_result: Res<MatchResult>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut halo_query: Query<(
        &CenterDiceTurnHalo,
        &mut Visibility,
        &mut Transform,
        &MeshMaterial2d<ColorMaterial>,
    )>,
) {
    let halo_state = center_dice_turn_halo_state(
        &turn_state,
        &dice_visual_state,
        game_phase.get(),
        &player_roster,
        &match_result,
    );
    let pulse = current_turn_guide_pulse(time.elapsed_secs());

    for (halo, mut visibility, mut transform, material_handle) in &mut halo_query {
        let Some((player, faces)) = halo_state else {
            *visibility = Visibility::Hidden;
            transform.scale = Vec3::ONE;
            continue;
        };

        *visibility = Visibility::Visible;
        transform.scale = center_dice_turn_halo_scale(faces, pulse);
        if let Some(mut material) = materials.get_mut(&material_handle.0) {
            material.color = center_dice_turn_halo_color(halo.layer, player.color, pulse);
        }
    }
}

fn hide_center_dice_displays(
    dice_query: &mut Query<(
        &CenterDiceSprite,
        &mut Sprite,
        &mut Visibility,
        &mut Transform,
    )>,
) {
    for (_, _, mut visibility, mut transform) in dice_query.iter_mut() {
        *visibility = Visibility::Hidden;
        transform.rotation = Quat::IDENTITY;
        transform.scale = Vec3::ONE;
    }
}

fn render_center_dice_prompt(
    elapsed_secs: f32,
    dice_sprite_assets: &DiceSpriteAssets,
    dice_query: &mut Query<(
        &CenterDiceSprite,
        &mut Sprite,
        &mut Visibility,
        &mut Transform,
    )>,
) {
    let visual_transform = center_dice_prompt_transform(elapsed_secs);
    for (display, mut sprite, mut visibility, mut transform) in dice_query.iter_mut() {
        if display.die_index != 0 {
            *visibility = Visibility::Hidden;
            transform.rotation = Quat::IDENTITY;
            transform.scale = Vec3::ONE;
            continue;
        }

        sprite.custom_size = Some(Vec2::splat(CENTER_DICE_PROMPT_SIZE));
        sprite.color = Color::srgba(1.0, 1.0, 1.0, CENTER_DICE_PROMPT_ALPHA);
        sprite.image = dice_sprite_assets.roll_frame_handle(CENTER_DICE_PROMPT_FRAME);
        *visibility = Visibility::Visible;
        transform.translation.x = display.base_center.x + visual_transform.offset.x;
        transform.translation.y = display.base_center.y + visual_transform.offset.y;
        transform.rotation = Quat::from_rotation_z(visual_transform.rotation);
        transform.scale = Vec3::new(visual_transform.scale.x, visual_transform.scale.y, 1.0);
    }
}

fn render_center_dice_faces(
    faces: [u8; 2],
    dice_sprite_assets: &DiceSpriteAssets,
    dice_query: &mut Query<(
        &CenterDiceSprite,
        &mut Sprite,
        &mut Visibility,
        &mut Transform,
    )>,
) {
    for (display, mut sprite, mut visibility, mut transform) in dice_query.iter_mut() {
        let Some(roll) = dice_face_for_index(faces, display.die_index) else {
            *visibility = Visibility::Hidden;
            transform.rotation = Quat::IDENTITY;
            transform.scale = Vec3::ONE;
            continue;
        };

        sprite.custom_size = Some(Vec2::splat(CENTER_DICE_SPRITE_SIZE));
        sprite.color = Color::WHITE;
        sprite.image = dice_sprite_assets.face_handle(roll);
        *visibility = Visibility::Visible;
        let center = center_dice_center_for_index(display.base_center, faces, display.die_index);
        transform.translation.x = center.x;
        transform.translation.y = center.y;
        transform.rotation = Quat::IDENTITY;
        transform.scale = Vec3::ONE;
    }
}

fn visible_dice_roll_key(turn_state: &TurnState) -> Option<DiceRollVisualKey> {
    if turn_state.roll_serial == 0 {
        return None;
    }

    if let Some(choice) = turn_state.pending_double_dice_choice {
        return Some(DiceRollVisualKey {
            roll_serial: choice.roll_serial,
            player_id: choice.player_id,
            roll: visual_roll_for_faces(choice.faces),
            faces: choice.faces,
        });
    }

    if let Some(roll) = turn_state.current_roll {
        let faces = display_faces_for_roll(roll, turn_state.current_roll_faces);
        return Some(DiceRollVisualKey {
            roll_serial: turn_state.roll_serial,
            player_id: turn_state.current_player,
            roll: visual_roll_for_faces(faces),
            faces,
        });
    }

    let player_id = turn_state.last_roll_player?;
    let roll = turn_state.last_roll?;
    let faces = display_faces_for_roll(roll, turn_state.last_roll_faces);
    Some(DiceRollVisualKey {
        roll_serial: turn_state.roll_serial,
        player_id,
        roll: visual_roll_for_faces(faces),
        faces,
    })
}

fn visual_roll_for_faces(faces: [u8; 2]) -> u8 {
    if dice_face_for_index(faces, 1).is_some() {
        faces[0].max(faces[1])
    } else {
        faces[0]
    }
}

fn dice_roll_animation_faces(animation: DiceRollVisualAnimation) -> [u8; 2] {
    if animation.elapsed >= DICE_ROLL_SETTLE_START {
        return animation.key.faces;
    }

    let frame = dice_roll_animation_frame(animation.elapsed);
    [
        animated_die_face(animation.key, frame, 0),
        if dice_face_for_index(animation.key.faces, 1).is_some() {
            animated_die_face(animation.key, frame, 1)
        } else {
            0
        },
    ]
}

fn dice_roll_visual_transform(
    animation: DiceRollVisualAnimation,
    die_index: u8,
) -> DiceRollVisualTransform {
    let progress = (animation.elapsed / DICE_ROLL_ANIMATION_DURATION).clamp(0.0, 1.0);
    if progress >= 1.0 {
        return DiceRollVisualTransform::default();
    }

    let roll_progress = (animation.elapsed / DICE_ROLL_SETTLE_START).clamp(0.0, 1.0);
    let intensity = (1.0 - roll_progress).powf(1.15);
    let travel = 1.0 - ease_out_cubic(roll_progress);
    let frame = dice_roll_animation_frame(animation.elapsed);
    let seed = dice_roll_animation_seed(animation.key);
    let phase = (dice_roll_hash(seed, 0, die_index, 8) % 628) as f32 / 100.0;
    let lane = if die_index == 0 { -1.0 } else { 1.0 };
    let approach = lane * travel * DICE_ROLL_TRAVEL_DISTANCE;
    let horizontal_sway =
        (roll_progress * PI * 3.8 + phase).sin() * DICE_ROLL_MAX_SHAKE * intensity;
    let vertical_sway =
        (roll_progress * PI * 4.2 + phase * 0.6).cos() * DICE_ROLL_MAX_SHAKE * 0.24 * intensity;
    let hop = (roll_progress * PI * 2.65 + die_index as f32 * 0.55)
        .sin()
        .abs()
        * DICE_ROLL_MAX_HOP
        * intensity;
    let settle_progress = ((animation.elapsed - DICE_ROLL_SETTLE_START)
        / (DICE_ROLL_ANIMATION_DURATION - DICE_ROLL_SETTLE_START))
        .clamp(0.0, 1.0);
    let settle_bounce = if animation.elapsed >= DICE_ROLL_SETTLE_START {
        (1.0 - settle_progress) * (settle_progress * PI).sin()
    } else {
        0.0
    };
    let scale_bump = (animation.elapsed * 24.0 + die_index as f32).sin().abs()
        * DICE_ROLL_MAX_SCALE_BUMP
        * intensity;
    let squash = settle_bounce * 0.12;
    let spin_direction = if dice_roll_hash(seed, 0, die_index, 7) % 2 == 0 {
        1.0
    } else {
        -1.0
    };
    let landing_rotation = dice_roll_noise(seed, 0, die_index, 2) * DICE_ROLL_MAX_ROTATION * 0.48;
    let spin_turns = DICE_ROLL_SPIN_TURNS + (dice_roll_hash(seed, 0, die_index, 5) % 2) as f32;
    let rolling_rotation = landing_rotation
        + spin_direction * ease_out_cubic(roll_progress) * PI * 2.0 * spin_turns
        + (roll_progress * PI * 6.0 + phase).sin() * DICE_ROLL_MAX_ROTATION * intensity;
    let settle_rotation = landing_rotation * (1.0 - settle_progress)
        + dice_roll_noise(seed, frame, die_index, 2)
            * DICE_ROLL_MAX_ROTATION
            * 0.08
            * (1.0 - settle_progress);

    DiceRollVisualTransform {
        offset: Vec2::new(
            approach + horizontal_sway,
            vertical_sway + hop + settle_bounce * 2.8,
        ),
        scale: Vec2::new(1.0 + scale_bump + squash, 1.0 + scale_bump * 0.48 - squash),
        rotation: if animation.elapsed < DICE_ROLL_SETTLE_START {
            rolling_rotation
        } else {
            settle_rotation
        },
    }
}

fn ease_out_cubic(progress: f32) -> f32 {
    1.0 - (1.0 - progress).powi(3)
}

fn center_dice_prompt_visible(
    turn_state: &TurnState,
    game_phase: &GamePhase,
    player_roster: &PlayerRoster,
    match_result: &MatchResult,
) -> bool {
    !match_result.finished
        && matches!(game_phase, GamePhase::AwaitDice)
        && turn_state.current_roll.is_none()
        && turn_state.pending_roll_display.is_none()
        && player_roster.players.iter().any(|player| {
            player.state.player_id == turn_state.current_player
                && player.state.control == PlayerControl::Human
        })
}

fn center_dice_prompt_transform(elapsed_secs: f32) -> DiceRollVisualTransform {
    let bob = (elapsed_secs * PI * 1.6).sin() * CENTER_DICE_PROMPT_BOB;
    let rock = (elapsed_secs * PI * 1.1).sin() * 0.055;
    let scale = 1.0 + (elapsed_secs * PI * 1.4).sin().abs() * CENTER_DICE_PROMPT_SCALE_PULSE;
    DiceRollVisualTransform {
        offset: Vec2::new(0.0, bob),
        scale: Vec2::splat(scale),
        rotation: CENTER_DICE_PROMPT_BASE_ROTATION + rock,
    }
}

fn dice_roll_animation_frame(elapsed: f32) -> u32 {
    (elapsed / DICE_ROLL_FACE_INTERVAL).floor() as u32
}

fn dice_roll_animation_delta(delta_secs: f32) -> f32 {
    delta_secs.max(0.0)
}

fn animated_die_face(key: DiceRollVisualKey, frame: u32, die_index: u8) -> u8 {
    (dice_roll_hash(dice_roll_animation_seed(key), frame, die_index, 3) % 6 + 1) as u8
}

fn dice_roll_animation_seed(key: DiceRollVisualKey) -> u32 {
    key.roll_serial
        .wrapping_mul(1_103_515_245)
        .wrapping_add((key.player_id as u32).wrapping_mul(97_531))
        .wrapping_add((key.roll as u32).wrapping_mul(8_191))
        .wrapping_add((key.faces[0] as u32).wrapping_mul(313))
        .wrapping_add((key.faces[1] as u32).wrapping_mul(37))
}

fn dice_roll_noise(seed: u32, frame: u32, die_index: u8, channel: u32) -> f32 {
    let value = dice_roll_hash(seed, frame, die_index, channel) % 2001;
    value as f32 / 1000.0 - 1.0
}

fn dice_roll_hash(seed: u32, frame: u32, die_index: u8, channel: u32) -> u32 {
    let mut value = seed
        ^ frame.wrapping_mul(747_796_405)
        ^ (die_index as u32).wrapping_mul(2_891_336_453)
        ^ channel.wrapping_mul(277_803_737);
    value ^= value >> 16;
    value = value.wrapping_mul(2_246_822_519);
    value ^= value >> 13;
    value = value.wrapping_mul(3_266_489_917);
    value ^ (value >> 16)
}

fn player_dice_display_state(
    turn_state: &TurnState,
    player_id: u8,
    animation_active: bool,
) -> PlayerDiceDisplayState {
    let roll_display_pending = roll_display_is_pending_for_player(turn_state, player_id);
    if let Some(roll) = turn_state.current_roll {
        if turn_state.current_player == player_id && !roll_display_pending {
            return PlayerDiceDisplayState::Active(player_display_faces_for_roll(roll));
        }
        return disabled_player_roll_state(turn_state, player_id);
    }

    if (animation_active || turn_state.hold_last_roll_display)
        && turn_state.last_roll_player == Some(player_id)
        && !roll_display_pending
        && let Some(roll) = turn_state.last_roll
    {
        return PlayerDiceDisplayState::Active(player_display_faces_for_roll(roll));
    }

    disabled_player_roll_state(turn_state, player_id)
}

fn roll_display_is_pending_for_player(turn_state: &TurnState, player_id: u8) -> bool {
    turn_state
        .pending_roll_display
        .is_some_and(|pending| pending.player_id == player_id)
}

fn disabled_player_roll_state(turn_state: &TurnState, player_id: u8) -> PlayerDiceDisplayState {
    turn_state
        .player_last_roll(player_id)
        .map_or(PlayerDiceDisplayState::Hidden, |roll| {
            PlayerDiceDisplayState::Disabled(player_display_faces_for_roll(roll))
        })
}

fn player_display_faces_for_roll(roll: u8) -> [u8; 2] {
    [roll, 0]
}

fn display_faces_for_roll(roll: u8, faces: Option<[u8; 2]>) -> [u8; 2] {
    let faces = faces.unwrap_or([roll, 0]);
    if dice_face_for_index(faces, 0).is_some() {
        faces
    } else {
        [roll, 0]
    }
}

fn dice_face_for_index(faces: [u8; 2], die_index: u8) -> Option<u8> {
    match die_index {
        0 if (1..=6).contains(&faces[0]) => Some(faces[0]),
        1 if (1..=6).contains(&faces[1]) => Some(faces[1]),
        _ => None,
    }
}

fn dice_center_for_index(base_center: Vec2, faces: [u8; 2], die_index: u8) -> Vec2 {
    if dice_face_for_index(faces, 1).is_none() {
        return base_center;
    }

    let offset = if die_index == 0 {
        -PLAYER_DICE_DOUBLE_OFFSET
    } else {
        PLAYER_DICE_DOUBLE_OFFSET
    };
    base_center + Vec2::new(offset, 0.0)
}

fn center_dice_center_for_index(base_center: Vec2, faces: [u8; 2], die_index: u8) -> Vec2 {
    if dice_face_for_index(faces, 1).is_none() {
        return base_center;
    }

    let offset = if die_index == 0 {
        -CENTER_DICE_DOUBLE_OFFSET
    } else {
        CENTER_DICE_DOUBLE_OFFSET
    };
    base_center + Vec2::new(offset, 0.0)
}

fn current_turn_profile<'a>(
    turn_state: &TurnState,
    player_roster: &'a PlayerRoster,
    match_result: &MatchResult,
) -> Option<&'a PlayerProfile> {
    if match_result.finished {
        return None;
    }

    player_roster
        .players
        .iter()
        .find(|player| player.state.player_id == turn_state.current_player)
}

fn center_dice_turn_halo_state<'a>(
    turn_state: &TurnState,
    dice_visual_state: &DiceRollVisualState,
    game_phase: &GamePhase,
    player_roster: &'a PlayerRoster,
    match_result: &MatchResult,
) -> Option<(&'a PlayerProfile, [u8; 2])> {
    if match_result.finished {
        return None;
    }

    if let Some(animation) = dice_visual_state.animation {
        return player_roster
            .players
            .iter()
            .find(|player| player.state.player_id == animation.key.player_id)
            .map(|player| (player, dice_roll_animation_faces(animation)));
    }

    if let Some(choice) = turn_state.pending_double_dice_choice {
        return player_roster
            .players
            .iter()
            .find(|player| player.state.player_id == choice.player_id)
            .map(|player| (player, choice.faces));
    }

    center_dice_prompt_visible(turn_state, game_phase, player_roster, match_result)
        .then(|| current_turn_profile(turn_state, player_roster, match_result))
        .flatten()
        .map(|player| (player, [1, 0]))
}

fn current_turn_guide_pulse(elapsed_secs: f32) -> f32 {
    (elapsed_secs * PI * 1.35).sin() * 0.5 + 0.5
}

fn center_dice_turn_halo_color(
    layer: CenterDiceTurnHaloLayer,
    player_color: Color,
    pulse: f32,
) -> Color {
    match layer {
        CenterDiceTurnHaloLayer::Glow => player_color
            .mix(&Color::WHITE, 0.30)
            .with_alpha(0.14 + pulse * 0.10),
        CenterDiceTurnHaloLayer::Ring => player_color
            .mix(&Color::WHITE, 0.18)
            .with_alpha(0.60 + pulse * 0.22),
    }
}

fn center_dice_turn_halo_scale(faces: [u8; 2], pulse: f32) -> Vec3 {
    let breath = 1.0 + pulse * 0.035;
    if dice_face_for_index(faces, 1).is_some() {
        Vec3::new(1.58 * breath, 1.02 * breath, 1.0)
    } else {
        Vec3::new(breath, breath, 1.0)
    }
}

fn dice_face_asset_path(roll: u8) -> &'static str {
    match roll {
        1 => "ui/dice/die_1.png",
        2 => "ui/dice/die_2.png",
        3 => "ui/dice/die_3.png",
        4 => "ui/dice/die_4.png",
        5 => "ui/dice/die_5.png",
        6 => "ui/dice/die_6.png",
        _ => "ui/dice/die_1.png",
    }
}

fn dice_sprite_asset_path(
    animation: DiceRollVisualAnimation,
    roll: u8,
    die_index: u8,
) -> &'static str {
    if animation.elapsed < DICE_ROLL_SETTLE_START {
        return dice_roll_frame_asset_path(dice_roll_sprite_frame(animation, die_index));
    }
    dice_face_asset_path(roll)
}

fn dice_roll_sprite_frame(animation: DiceRollVisualAnimation, die_index: u8) -> usize {
    let base_frame = dice_roll_animation_frame(animation.elapsed) as usize;
    let seed_offset = (dice_roll_animation_seed(animation.key) as usize)
        .wrapping_add(die_index as usize * 5)
        % DICE_ROLL_FRAME_COUNT;
    (base_frame + seed_offset) % DICE_ROLL_FRAME_COUNT
}

fn dice_roll_frame_asset_path(frame: usize) -> &'static str {
    match frame % DICE_ROLL_FRAME_COUNT {
        0 => "ui/dice/roll_00.png",
        1 => "ui/dice/roll_01.png",
        2 => "ui/dice/roll_02.png",
        3 => "ui/dice/roll_03.png",
        4 => "ui/dice/roll_04.png",
        5 => "ui/dice/roll_05.png",
        6 => "ui/dice/roll_06.png",
        7 => "ui/dice/roll_07.png",
        8 => "ui/dice/roll_08.png",
        9 => "ui/dice/roll_09.png",
        10 => "ui/dice/roll_10.png",
        11 => "ui/dice/roll_11.png",
        12 => "ui/dice/roll_12.png",
        13 => "ui/dice/roll_13.png",
        14 => "ui/dice/roll_14.png",
        _ => "ui/dice/roll_15.png",
    }
}

fn board_surface_color() -> Color {
    Color::srgb(0.965, 0.972, 0.982)
}

fn board_grid_line_color() -> Color {
    Color::srgba(0.12, 0.16, 0.22, 0.30)
}

fn hangar_outline_color() -> Color {
    Color::srgba(0.10, 0.14, 0.20, 0.22)
}

fn hangar_pad_outline_color() -> Color {
    Color::srgba(0.08, 0.12, 0.18, 0.68)
}

fn is_hangar_background_rect(rect: SvgRect) -> bool {
    (rect.size.x - 160.0).abs() < 0.001 && (rect.size.y - 160.0).abs() < 0.001
}

fn visual_rect_geometry(rect: SvgRect) -> VisualRectGeometry {
    if is_hangar_background_rect(rect) {
        let inset = 10.0;
        let outer_direction = Vec2::new(rect.center.x.signum(), rect.center.y.signum());
        return VisualRectGeometry {
            center: rect.center + outer_direction * (inset * 0.5),
            size: rect.size - Vec2::splat(inset),
        };
    }

    VisualRectGeometry {
        center: rect.center,
        size: rect.size,
    }
}

#[cfg(test)]
fn visual_rect_bounds(rect: SvgRect) -> (Vec2, Vec2) {
    let geometry = visual_rect_geometry(rect);
    (
        geometry.center - geometry.size * 0.5,
        geometry.center + geometry.size * 0.5,
    )
}

fn pip_visible_for_roll(roll: u8, slot: DicePipSlot) -> bool {
    match roll {
        1 => matches!(slot, DicePipSlot::Center),
        2 => matches!(slot, DicePipSlot::TopLeft | DicePipSlot::BottomRight),
        3 => matches!(
            slot,
            DicePipSlot::TopLeft | DicePipSlot::Center | DicePipSlot::BottomRight
        ),
        4 => matches!(
            slot,
            DicePipSlot::TopLeft
                | DicePipSlot::TopRight
                | DicePipSlot::BottomLeft
                | DicePipSlot::BottomRight
        ),
        5 => matches!(
            slot,
            DicePipSlot::TopLeft
                | DicePipSlot::TopRight
                | DicePipSlot::Center
                | DicePipSlot::BottomLeft
                | DicePipSlot::BottomRight
        ),
        6 => matches!(
            slot,
            DicePipSlot::TopLeft
                | DicePipSlot::TopRight
                | DicePipSlot::MiddleLeft
                | DicePipSlot::MiddleRight
                | DicePipSlot::BottomLeft
                | DicePipSlot::BottomRight
        ),
        _ => false,
    }
}

fn dice_pip_offset(slot: DicePipSlot) -> Vec2 {
    match slot {
        DicePipSlot::Center => Vec2::ZERO,
        DicePipSlot::TopLeft => Vec2::new(-6.0, 6.0),
        DicePipSlot::TopRight => Vec2::new(6.0, 6.0),
        DicePipSlot::MiddleLeft => Vec2::new(-6.0, 0.0),
        DicePipSlot::MiddleRight => Vec2::new(6.0, 0.0),
        DicePipSlot::BottomLeft => Vec2::new(-6.0, -6.0),
        DicePipSlot::BottomRight => Vec2::new(6.0, -6.0),
    }
}

fn player_dice_display_color(layer: PlayerDiceDisplayLayer, active: bool) -> Color {
    match (layer, active) {
        (PlayerDiceDisplayLayer::Rim, true) => Color::srgba(0.08, 0.11, 0.16, 0.82),
        (PlayerDiceDisplayLayer::Face, true) => Color::srgba(1.0, 1.0, 1.0, 0.92),
        (PlayerDiceDisplayLayer::Rim, false) => Color::srgba(0.18, 0.19, 0.22, 0.48),
        (PlayerDiceDisplayLayer::Face, false) => Color::srgba(0.70, 0.72, 0.76, 0.68),
    }
}

fn player_dice_pip_color(active: bool) -> Color {
    if active {
        Color::srgb(0.08, 0.11, 0.16)
    } else {
        Color::srgba(0.30, 0.32, 0.36, 0.74)
    }
}

fn spawn_player_dice_display(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    player_id: u8,
    center: Vec2,
) {
    for die_index in 0..2 {
        for (radius, layer, z) in [
            (20.0, PlayerDiceDisplayLayer::Rim, BOARD_Z_LAYER + 1.26),
            (17.2, PlayerDiceDisplayLayer::Face, BOARD_Z_LAYER + 1.27),
        ] {
            commands.spawn((
                Mesh2d(meshes.add(Circle::new(radius))),
                MeshMaterial2d(
                    materials.add(ColorMaterial::from(player_dice_display_color(layer, true))),
                ),
                Transform::from_xyz(center.x, center.y, z),
                Visibility::Hidden,
                PlayerDiceDisplay {
                    player_id,
                    die_index,
                    layer,
                    base_center: center,
                },
                Name::new(format!("PlayerDiceDisplay_P{player_id}_D{die_index}")),
                BoardSceneEntity,
            ));
        }

        for slot in [
            DicePipSlot::Center,
            DicePipSlot::TopLeft,
            DicePipSlot::TopRight,
            DicePipSlot::MiddleLeft,
            DicePipSlot::MiddleRight,
            DicePipSlot::BottomLeft,
            DicePipSlot::BottomRight,
        ] {
            let offset = dice_pip_offset(slot);
            commands.spawn((
                Mesh2d(meshes.add(Circle::new(2.7))),
                MeshMaterial2d(materials.add(ColorMaterial::from(player_dice_pip_color(true)))),
                Transform::from_xyz(
                    center.x + offset.x,
                    center.y + offset.y,
                    BOARD_Z_LAYER + 1.29,
                ),
                Visibility::Hidden,
                PlayerDicePip {
                    player_id,
                    die_index,
                    slot,
                    base_center: center,
                },
                Name::new(format!("PlayerDicePip_P{player_id}_D{die_index}_{slot:?}")),
                BoardSceneEntity,
            ));
        }
    }
}

fn spawn_center_dice_roll_display(commands: &mut Commands, dice_sprite_assets: &DiceSpriteAssets) {
    let center = Vec2::ZERO;
    for die_index in 0..2 {
        let mut sprite = Sprite::from_image(dice_sprite_assets.face_handle(1));
        sprite.custom_size = Some(Vec2::splat(CENTER_DICE_SPRITE_SIZE));
        commands.spawn((
            sprite,
            Transform::from_xyz(center.x, center.y, BOARD_Z_LAYER + 3.28),
            Visibility::Hidden,
            CenterDiceSprite {
                die_index,
                base_center: center,
            },
            Name::new(format!("CenterDiceSprite_D{die_index}")),
            BoardSceneEntity,
        ));
    }
}

fn spawn_center_dice_turn_halo(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
) {
    for (layer, inner_radius, outer_radius, z) in [
        (
            CenterDiceTurnHaloLayer::Glow,
            CENTER_DICE_TURN_HALO_GLOW_INNER_RADIUS,
            CENTER_DICE_TURN_HALO_GLOW_OUTER_RADIUS,
            BOARD_Z_LAYER + 3.08,
        ),
        (
            CenterDiceTurnHaloLayer::Ring,
            CENTER_DICE_TURN_HALO_RING_INNER_RADIUS,
            CENTER_DICE_TURN_HALO_RING_OUTER_RADIUS,
            BOARD_Z_LAYER + 3.09,
        ),
    ] {
        commands.spawn((
            Mesh2d(meshes.add(annulus_mesh(
                inner_radius,
                outer_radius,
                CENTER_DICE_TURN_HALO_SEGMENTS,
            ))),
            MeshMaterial2d(materials.add(ColorMaterial::from(Color::srgba(1.0, 1.0, 1.0, 0.0)))),
            Transform::from_xyz(0.0, 0.0, z),
            Visibility::Hidden,
            CenterDiceTurnHalo { layer },
            Name::new(format!("CenterDiceTurnHalo_{layer:?}")),
            BoardSceneEntity,
        ));
    }
}

/// 绘制带描边方块。
fn spawn_square_with_border(
    commands: &mut Commands,
    center: Vec2,
    size: Vec2,
    style: DrawStyle,
    name: impl Into<String>,
) {
    let name = name.into();
    commands.spawn((
        Sprite::from_color(style.border, size + Vec2::splat(style.border_width * 2.0)),
        Transform::from_xyz(center.x, center.y, style.z),
        Name::new(format!("{name}_border")),
        BoardSceneEntity,
    ));
    commands.spawn((
        Sprite::from_color(style.fill, size),
        Transform::from_xyz(center.x, center.y, style.z + 0.01),
        Name::new(name),
        BoardSceneEntity,
    ));
}

fn visible_home_lane_dots(player_roster: &PlayerRoster) -> Vec<HomeLaneDotDraw> {
    let mut dots = Vec::with_capacity(BOARD_HOME_LANES.len() * 6);

    for (lane_index, lane) in BOARD_HOME_LANES.iter().enumerate() {
        let Some(seat) = PlayerSeat::ALL.get(lane_index).copied() else {
            continue;
        };
        let active = player_for_seat(player_roster, seat).is_some();
        let visible_count = if active { lane.len() } else { 1 };

        for (dot_index, &position) in lane.iter().take(visible_count).enumerate() {
            dots.push(HomeLaneDotDraw {
                seat,
                position,
                show_turn_marker: active && dot_index == 0,
            });
        }
    }

    dots
}

fn should_draw_svg_rect(rect: SvgRect, player_roster: &PlayerRoster) -> bool {
    let Some((seat, lane_index)) = svg_rect_home_lane_slot(rect) else {
        return true;
    };

    player_for_seat(player_roster, seat).is_some() || lane_index == 0
}

fn svg_rect_home_lane_slot(rect: SvgRect) -> Option<(PlayerSeat, usize)> {
    for (lane_index, lane) in BOARD_HOME_LANES.iter().enumerate() {
        let seat = PlayerSeat::ALL.get(lane_index).copied()?;
        for (dot_index, &position) in lane.iter().enumerate() {
            if rect.center == position {
                return Some((seat, dot_index));
            }
        }
    }

    None
}

fn home_lane_turn_direction(seat: PlayerSeat) -> Vec2 {
    match seat {
        PlayerSeat::Blue => Vec2::X,
        PlayerSeat::Red => Vec2::NEG_Y,
        PlayerSeat::Green => Vec2::Y,
        PlayerSeat::Yellow => Vec2::NEG_X,
    }
}

/// 绘制带描边圆形。
fn spawn_circle_with_border(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    center: Vec2,
    radius: f32,
    style: DrawStyle,
    name: impl Into<String>,
) {
    let name = name.into();
    commands.spawn((
        Mesh2d(meshes.add(Circle::new(radius + style.border_width))),
        MeshMaterial2d(materials.add(ColorMaterial::from(style.border))),
        Transform::from_xyz(center.x, center.y, style.z),
        Name::new(format!("{name}_border")),
        BoardSceneEntity,
    ));
    commands.spawn((
        Mesh2d(meshes.add(Circle::new(radius))),
        MeshMaterial2d(materials.add(ColorMaterial::from(style.fill))),
        Transform::from_xyz(center.x, center.y, style.z + 0.01),
        Name::new(name),
        BoardSceneEntity,
    ));
}

/// 绘制带描边三角形。
fn spawn_triangle_with_border(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    points: [Vec2; 3],
    style: DrawStyle,
    name: impl Into<String>,
) {
    let name = name.into();
    let [a, b, c] = points;
    let centroid = (a + b + c) / 3.0;
    let outer_a = a - centroid;
    let outer_b = b - centroid;
    let outer_c = c - centroid;

    commands.spawn((
        Mesh2d(meshes.add(Triangle2d::new(outer_a, outer_b, outer_c))),
        MeshMaterial2d(materials.add(ColorMaterial::from(style.border))),
        Transform::from_xyz(centroid.x, centroid.y, style.z),
        Name::new(format!("{name}_border")),
        BoardSceneEntity,
    ));

    let max_radius = outer_a
        .length()
        .max(outer_b.length())
        .max(outer_c.length())
        .max(1.0);
    let inset_scale = ((max_radius - style.border_width) / max_radius).clamp(0.72, 0.985);

    commands.spawn((
        Mesh2d(meshes.add(Triangle2d::new(
            outer_a * inset_scale,
            outer_b * inset_scale,
            outer_c * inset_scale,
        ))),
        MeshMaterial2d(materials.add(ColorMaterial::from(style.fill))),
        Transform::from_xyz(centroid.x, centroid.y, style.z + 0.01),
        Name::new(name),
        BoardSceneEntity,
    ));
}

/// 绘制方向箭头图标。
fn spawn_arrow_icon(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    icon: DirectionIconDraw,
    name: impl Into<String>,
) {
    let name = name.into();
    let direction = icon.direction.normalize_or_zero();
    let angle = direction.y.atan2(direction.x);
    let tail = icon.center - direction * 4.0;
    let head = icon.center + direction * 7.0;
    let perp = Vec2::new(-direction.y, direction.x);

    commands.spawn((
        Sprite::from_color(icon.color, Vec2::new(15.0, 3.0)),
        Transform {
            translation: Vec3::new(tail.x, tail.y, icon.z),
            rotation: Quat::from_rotation_z(angle),
            ..default()
        },
        Name::new(format!("{name}_shaft")),
        BoardSceneEntity,
    ));

    spawn_triangle_with_border(
        commands,
        meshes,
        materials,
        [
            head + direction * 4.0,
            head - direction * 5.5 + perp * 5.0,
            head - direction * 5.5 - perp * 5.0,
        ],
        DrawStyle {
            fill: icon.color,
            border: icon.color,
            border_width: 0.0,
            z: icon.z + 0.01,
        },
        format!("{name}_head"),
    );
}

fn spawn_special_tile_icon(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    center: Vec2,
    kind: TileKind,
    z: f32,
    name: impl Into<String>,
) {
    let name = name.into();
    match kind {
        TileKind::Attack => spawn_sword_icon(
            commands,
            meshes,
            materials,
            center,
            Color::srgb(0.82, 0.08, 0.08),
            z,
            format!("{name}_Sword"),
        ),
        TileKind::Defense => spawn_shield_icon(
            commands,
            center,
            Color::srgb(0.08, 0.28, 0.78),
            z,
            format!("{name}_Shield"),
        ),
        TileKind::Event => spawn_question_icon(
            commands,
            meshes,
            materials,
            center,
            Color::srgb(0.12, 0.12, 0.16),
            z,
            format!("{name}_Question"),
        ),
        TileKind::Normal | TileKind::Jump | TileKind::Goal => {}
    }
}

/// 绘制攻击点剑图标。
fn spawn_sword_icon(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    center: Vec2,
    color: Color,
    z: f32,
    name: impl Into<String>,
) {
    let name = name.into();
    let direction = Vec2::new(1.0, 1.0).normalize();
    let perp = Vec2::new(-direction.y, direction.x);
    let blade_base = center - direction * 3.0;
    let blade_tip = center + direction * 8.0;
    let guard = center - direction * 5.0;

    spawn_line_segment(
        commands,
        blade_base,
        blade_tip,
        2.2,
        color,
        z,
        format!("{name}_blade"),
    );
    spawn_triangle_with_border(
        commands,
        meshes,
        materials,
        [
            blade_tip + direction * 3.2,
            blade_tip - direction * 2.2 + perp * 3.0,
            blade_tip - direction * 2.2 - perp * 3.0,
        ],
        DrawStyle {
            fill: color,
            border: color,
            border_width: 0.0,
            z: z + 0.01,
        },
        format!("{name}_tip"),
    );
    spawn_line_segment(
        commands,
        guard - perp * 5.0,
        guard + perp * 5.0,
        2.0,
        color,
        z + 0.03,
        format!("{name}_guard"),
    );
    spawn_line_segment(
        commands,
        guard - direction * 5.0,
        guard - direction * 1.2,
        2.2,
        color,
        z + 0.04,
        format!("{name}_hilt"),
    );
}

/// 绘制防御点盾牌图标。
fn spawn_shield_icon(
    commands: &mut Commands,
    center: Vec2,
    color: Color,
    z: f32,
    name: impl Into<String>,
) {
    let name = name.into();
    let outline = [
        Vec2::new(-6.2, 5.8),
        Vec2::new(0.0, 8.2),
        Vec2::new(6.2, 5.8),
        Vec2::new(5.0, -1.2),
        Vec2::new(0.0, -8.0),
        Vec2::new(-5.0, -1.2),
        Vec2::new(-6.2, 5.8),
    ];

    for (index, segment) in outline.windows(2).enumerate() {
        spawn_line_segment(
            commands,
            center + segment[0],
            center + segment[1],
            2.2,
            color,
            z + index as f32 * 0.002,
            format!("{name}_outline_{index}"),
        );
    }
    spawn_line_segment(
        commands,
        center + Vec2::new(0.0, 4.7),
        center + Vec2::new(0.0, -4.2),
        1.9,
        color,
        z + 0.02,
        format!("{name}_center"),
    );
}

/// 绘制随机事件问号图标。
fn spawn_question_icon(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    center: Vec2,
    color: Color,
    z: f32,
    name: impl Into<String>,
) {
    let name = name.into();
    let question = [
        Vec2::new(-5.2, 4.5),
        Vec2::new(-3.5, 7.2),
        Vec2::new(1.0, 7.8),
        Vec2::new(4.6, 5.5),
        Vec2::new(4.2, 1.8),
        Vec2::new(0.0, -0.8),
        Vec2::new(0.0, -4.0),
    ];

    for (index, segment) in question.windows(2).enumerate() {
        spawn_line_segment(
            commands,
            center + segment[0],
            center + segment[1],
            2.3,
            color,
            z + index as f32 * 0.002,
            format!("{name}_mark_{index}"),
        );
    }
    spawn_circle_with_border(
        commands,
        meshes,
        materials,
        center + Vec2::new(0.0, -8.0),
        1.8,
        DrawStyle {
            fill: color,
            border: color,
            border_width: 0.0,
            z: z + 0.03,
        },
        format!("{name}_dot"),
    );
}

/// 绘制 SVG 中的单/双 chevron 方向提示。
fn spawn_chevron_icon(commands: &mut Commands, icon: ChevronDraw, name: impl Into<String>) {
    let name = name.into();
    let direction = icon.direction.normalize_or_zero();
    let perp = Vec2::new(-direction.y, direction.x);
    let spacing = icon.size * 0.58;
    let first_offset = -((icon.count.saturating_sub(1)) as f32) * spacing * 0.5;

    for index in 0..icon.count {
        let base = icon.center + direction * (first_offset + index as f32 * spacing);
        let tip = base + direction * icon.size * 0.45;
        let back = base - direction * icon.size * 0.35;
        let wing_a = back + perp * icon.size * 0.42;
        let wing_b = back - perp * icon.size * 0.42;
        spawn_line_segment(
            commands,
            wing_a,
            tip,
            3.0,
            icon.color,
            icon.z + index as f32 * 0.002,
            format!("{name}_{index}_a"),
        );
        spawn_line_segment(
            commands,
            wing_b,
            tip,
            3.0,
            icon.color,
            icon.z + index as f32 * 0.002 + 0.001,
            format!("{name}_{index}_b"),
        );
    }
}

fn spawn_home_lane_turn_marker(
    commands: &mut Commands,
    icon: TurnMarkerDraw,
    name: impl Into<String>,
) {
    let name = name.into();
    let Some(points) = home_lane_turn_marker_path(icon.direction) else {
        return;
    };
    let points = points.map(|point| icon.center + point);

    for (index, segment) in points.windows(2).enumerate() {
        spawn_line_segment(
            commands,
            segment[0],
            segment[1],
            3.1,
            icon.color,
            icon.z + index as f32 * 0.001,
            format!("{name}_elbow_{index}"),
        );
    }

    let tip = *points.last().unwrap_or(&icon.center);
    let before_tip = points
        .get(points.len().saturating_sub(2))
        .copied()
        .unwrap_or(icon.center);
    let direction = (tip - before_tip).normalize_or_zero();
    if direction.length_squared() <= 0.01 {
        return;
    }
    let perp = Vec2::new(-direction.y, direction.x);
    let head_back = tip - direction * 6.2;
    spawn_line_segment(
        commands,
        head_back + perp * 4.4,
        tip,
        3.1,
        icon.color,
        icon.z + 0.01,
        format!("{name}_head_a"),
    );
    spawn_line_segment(
        commands,
        head_back - perp * 4.4,
        tip,
        3.1,
        icon.color,
        icon.z + 0.011,
        format!("{name}_head_b"),
    );
}

fn home_lane_turn_marker_path(direction: Vec2) -> Option<[Vec2; 3]> {
    let direction = direction.normalize_or_zero();
    if direction.length_squared() <= 0.01 {
        return None;
    }

    let incoming = Vec2::new(-direction.y, direction.x);
    let elbow = -direction * 4.0;
    Some([elbow - incoming * 9.0, elbow, elbow + direction * 13.0])
}

/// 绘制中心终点星形。
fn spawn_star_icon(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    icon: StarDraw,
    name: impl Into<String>,
) {
    commands.spawn((
        Mesh2d(meshes.add(star_mesh(icon.radius, icon.radius * 0.48))),
        MeshMaterial2d(materials.add(ColorMaterial::from(icon.color))),
        Transform::from_xyz(icon.center.x, icon.center.y, icon.z),
        Name::new(name.into()),
        BoardSceneEntity,
    ));
}

fn star_mesh(outer_radius: f32, inner_radius: f32) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    let mut positions = vec![[0.0, 0.0, 0.0]];

    for index in 0..10 {
        let angle = PI * 0.5 + index as f32 * PI / 5.0;
        let radius = if index % 2 == 0 {
            outer_radius
        } else {
            inner_radius
        };
        positions.push([angle.cos() * radius, angle.sin() * radius, 0.0]);
    }

    let mut indices = Vec::with_capacity(30);
    for index in 1..=10 {
        let next = if index == 10 { 1 } else { index + 1 };
        indices.extend_from_slice(&[0, index as u32, next as u32]);
    }

    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn annulus_mesh(inner_radius: f32, outer_radius: f32, segments: usize) -> Mesh {
    let segment_count = segments.max(3);
    let inner_radius = inner_radius.max(0.0);
    let outer_radius = outer_radius.max(inner_radius + 0.1);
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    let mut positions = Vec::with_capacity(segment_count * 2);

    for index in 0..segment_count {
        let angle = index as f32 / segment_count as f32 * PI * 2.0;
        let direction = Vec2::new(angle.cos(), angle.sin());
        positions.push([direction.x * outer_radius, direction.y * outer_radius, 0.0]);
        positions.push([direction.x * inner_radius, direction.y * inner_radius, 0.0]);
    }

    let mut indices = Vec::with_capacity(segment_count * 6);
    for index in 0..segment_count {
        let next = (index + 1) % segment_count;
        let outer = (index * 2) as u32;
        let inner = outer + 1;
        let next_outer = (next * 2) as u32;
        let next_inner = next_outer + 1;
        indices.extend_from_slice(&[outer, next_outer, inner]);
        indices.extend_from_slice(&[inner, next_outer, next_inner]);
    }

    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn spawn_line_segment(
    commands: &mut Commands,
    start: Vec2,
    end: Vec2,
    thickness: f32,
    color: Color,
    z: f32,
    name: impl Into<String>,
) {
    let delta = end - start;
    let length = delta.length();
    if length <= 0.01 {
        return;
    }
    let center = (start + end) * 0.5;
    commands.spawn((
        Sprite::from_color(color, Vec2::new(length, thickness)),
        Transform {
            translation: Vec3::new(center.x, center.y, z),
            rotation: Quat::from_rotation_z(delta.y.atan2(delta.x)),
            ..default()
        },
        Name::new(name.into()),
        BoardSceneEntity,
    ));
}

const CENTER_STAR_ICONS: &[SvgIcon] = &[
    SvgIcon {
        center: Vec2::new(0.0, 35.958),
        fill: "#FF0000",
    },
    SvgIcon {
        center: Vec2::new(-35.958, 0.0),
        fill: "#0080FF",
    },
    SvgIcon {
        center: Vec2::new(35.959, 0.0),
        fill: "#F3D849",
    },
    SvgIcon {
        center: Vec2::new(0.0, -35.958),
        fill: "#008000",
    },
];

const CHEVRON_ICONS: &[ChevronIcon] = &[
    ChevronIcon {
        center: Vec2::new(-156.104, 124.104),
        fill: "#F3D849",
        direction: Vec2::Y,
        count: 2,
        size: 16.0,
    },
    ChevronIcon {
        center: Vec2::new(-124.104, 156.104),
        fill: "#008000",
        direction: Vec2::X,
        count: 2,
        size: 16.0,
    },
    ChevronIcon {
        center: Vec2::new(124.317, 156.104),
        fill: "#008000",
        direction: Vec2::X,
        count: 2,
        size: 16.0,
    },
    ChevronIcon {
        center: Vec2::new(156.317, 124.104),
        fill: "#0080FF",
        direction: Vec2::NEG_Y,
        count: 2,
        size: 16.0,
    },
    ChevronIcon {
        center: Vec2::new(156.104, -124.104),
        fill: "#0080FF",
        direction: Vec2::NEG_Y,
        count: 2,
        size: 16.0,
    },
    ChevronIcon {
        center: Vec2::new(124.104, -156.104),
        fill: "#FF0000",
        direction: Vec2::NEG_X,
        count: 2,
        size: 16.0,
    },
    ChevronIcon {
        center: Vec2::new(-124.104, -156.104),
        fill: "#FF0000",
        direction: Vec2::NEG_X,
        count: 2,
        size: 16.0,
    },
    ChevronIcon {
        center: Vec2::new(-156.104, -124.104),
        fill: "#F3D849",
        direction: Vec2::Y,
        count: 2,
        size: 16.0,
    },
    ChevronIcon {
        center: Vec2::new(-60.104, 156.104),
        fill: "#008000",
        direction: Vec2::X,
        count: 2,
        size: 18.0,
    },
    ChevronIcon {
        center: Vec2::new(59.896, 156.104),
        fill: "#008000",
        direction: Vec2::X,
        count: 2,
        size: 18.0,
    },
    ChevronIcon {
        center: Vec2::new(155.896, 60.104),
        fill: "#0080FF",
        direction: Vec2::NEG_Y,
        count: 2,
        size: 18.0,
    },
    ChevronIcon {
        center: Vec2::new(155.896, -59.896),
        fill: "#0080FF",
        direction: Vec2::NEG_Y,
        count: 2,
        size: 18.0,
    },
    ChevronIcon {
        center: Vec2::new(59.896, -155.896),
        fill: "#FF0000",
        direction: Vec2::NEG_X,
        count: 2,
        size: 18.0,
    },
    ChevronIcon {
        center: Vec2::new(-60.104, -155.896),
        fill: "#FF0000",
        direction: Vec2::NEG_X,
        count: 2,
        size: 18.0,
    },
    ChevronIcon {
        center: Vec2::new(-156.104, -57.896),
        fill: "#F3D849",
        direction: Vec2::Y,
        count: 2,
        size: 18.0,
    },
    ChevronIcon {
        center: Vec2::new(-156.104, 62.104),
        fill: "#F3D849",
        direction: Vec2::Y,
        count: 2,
        size: 18.0,
    },
];

const LAUNCH_TRIANGLES: &[LaunchTriangle] = &[
    LaunchTriangle {
        seat: PlayerSeat::Blue,
        center: Vec2::new(-316.104, 156.104),
        a: Vec2::new(-340.104, 180.104),
        b: Vec2::new(-260.104, 180.104),
        c: Vec2::new(-340.104, 100.104),
        arrow_direction: Vec2::new(1.0, 0.0),
    },
    LaunchTriangle {
        seat: PlayerSeat::Red,
        center: Vec2::new(155.896, 316.104),
        a: Vec2::new(180.317, 340.104),
        b: Vec2::new(100.317, 340.104),
        c: Vec2::new(180.317, 260.104),
        arrow_direction: Vec2::new(0.0, -1.0),
    },
    LaunchTriangle {
        seat: PlayerSeat::Green,
        center: Vec2::new(-156.104, -315.896),
        a: Vec2::new(-180.104, -340.104),
        b: Vec2::new(-100.104, -340.104),
        c: Vec2::new(-180.104, -260.104),
        arrow_direction: Vec2::new(0.0, 1.0),
    },
    LaunchTriangle {
        seat: PlayerSeat::Yellow,
        center: Vec2::new(315.896, -155.896),
        a: Vec2::new(340.104, -180.104),
        b: Vec2::new(260.104, -180.104),
        c: Vec2::new(340.104, -100.104),
        arrow_direction: Vec2::new(-1.0, 0.0),
    },
];

const SVG_RECTS: &[SvgRect] = &[
    SvgRect {
        center: Vec2::new(120.317, 0.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#F3D849",
    },
    SvgRect {
        center: Vec2::new(-0.104, 240.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#FF0000",
    },
    SvgRect {
        center: Vec2::new(-0.104, 200.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#FF0000",
    },
    SvgRect {
        center: Vec2::new(-0.104, 160.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#FF0000",
    },
    SvgRect {
        center: Vec2::new(-0.104, 120.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#FF0000",
    },
    SvgRect {
        center: Vec2::new(-0.104, 80.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#FF0000",
    },
    SvgRect {
        center: Vec2::new(200.317, 0.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#F3D849",
    },
    SvgRect {
        center: Vec2::new(240.317, 0.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#F3D849",
    },
    SvgRect {
        center: Vec2::new(-200.104, -0.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#0080FF",
    },
    SvgRect {
        center: Vec2::new(-160.104, -0.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#0080FF",
    },
    SvgRect {
        center: Vec2::new(-120.104, -0.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#0080FF",
    },
    SvgRect {
        center: Vec2::new(-80.104, -0.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#0080FF",
    },
    SvgRect {
        center: Vec2::new(-240.104, -0.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#0080FF",
    },
    SvgRect {
        center: Vec2::new(0.104, -80.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#008000",
    },
    SvgRect {
        center: Vec2::new(0.104, -120.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#008000",
    },
    SvgRect {
        center: Vec2::new(0.104, -160.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#008000",
    },
    SvgRect {
        center: Vec2::new(0.104, -200.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#008000",
    },
    SvgRect {
        center: Vec2::new(0.104, -240.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#008000",
    },
    SvgRect {
        center: Vec2::new(80.317, 0.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#F3D849",
    },
    SvgRect {
        center: Vec2::new(160.317, 0.104),
        size: Vec2::new(40.0, 40.0),
        fill: "#F3D849",
    },
    SvgRect {
        center: Vec2::new(300.104, -80.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#0080FF",
    },
    SvgRect {
        center: Vec2::new(-300.104, -80.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#F3D849",
    },
    SvgRect {
        center: Vec2::new(-240.104, -140.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#0080FF",
    },
    SvgRect {
        center: Vec2::new(-200.104, -140.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#008000",
    },
    SvgRect {
        center: Vec2::new(140.104, -200.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#F3D849",
    },
    SvgRect {
        center: Vec2::new(140.104, -240.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#008000",
    },
    SvgRect {
        center: Vec2::new(-140.104, 200.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#0080FF",
    },
    SvgRect {
        center: Vec2::new(-80.104, 300.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#008000",
    },
    SvgRect {
        center: Vec2::new(-40.104, 300.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#0080FF",
    },
    SvgRect {
        center: Vec2::new(-0.104, 300.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#FF0000",
    },
    SvgRect {
        center: Vec2::new(40.317, 300.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#F3D849",
    },
    SvgRect {
        center: Vec2::new(80.317, 300.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#008000",
    },
    SvgRect {
        center: Vec2::new(140.317, 240.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#FF0000",
    },
    SvgRect {
        center: Vec2::new(-140.104, 240.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#FF0000",
    },
    SvgRect {
        center: Vec2::new(140.317, 200.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#F3D849",
    },
    SvgRect {
        center: Vec2::new(-240.104, 140.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#0080FF",
    },
    SvgRect {
        center: Vec2::new(-200.104, 140.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#FF0000",
    },
    SvgRect {
        center: Vec2::new(-300.104, 80.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#F3D849",
    },
    SvgRect {
        center: Vec2::new(300.317, 80.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#0080FF",
    },
    SvgRect {
        center: Vec2::new(-300.104, 40.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#FF0000",
    },
    SvgRect {
        center: Vec2::new(300.317, 40.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#FF0000",
    },
    SvgRect {
        center: Vec2::new(300.317, 0.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#F3D849",
    },
    SvgRect {
        center: Vec2::new(-300.104, -0.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#0080FF",
    },
    SvgRect {
        center: Vec2::new(300.104, -40.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#008000",
    },
    SvgRect {
        center: Vec2::new(200.104, -140.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#008000",
    },
    SvgRect {
        center: Vec2::new(240.104, -140.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#F3D849",
    },
    SvgRect {
        center: Vec2::new(-140.104, -200.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#0080FF",
    },
    SvgRect {
        center: Vec2::new(-140.104, -240.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#008000",
    },
    SvgRect {
        center: Vec2::new(-80.104, -300.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#FF0000",
    },
    SvgRect {
        center: Vec2::new(-40.104, -300.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#0080FF",
    },
    SvgRect {
        center: Vec2::new(240.317, 140.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#F3D849",
    },
    SvgRect {
        center: Vec2::new(200.317, 140.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#FF0000",
    },
    SvgRect {
        center: Vec2::new(-300.104, -40.104),
        size: Vec2::new(80.0, 40.0),
        fill: "#008000",
    },
    SvgRect {
        center: Vec2::new(0.104, -300.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#008000",
    },
    SvgRect {
        center: Vec2::new(40.104, -300.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#F3D849",
    },
    SvgRect {
        center: Vec2::new(80.104, -300.104),
        size: Vec2::new(40.0, 80.0),
        fill: "#FF0000",
    },
    SvgRect {
        center: Vec2::new(-260.104, 260.104),
        size: Vec2::new(160.0, 160.0),
        fill: "#0080FF",
    },
    SvgRect {
        center: Vec2::new(260.317, 260.104),
        size: Vec2::new(160.0, 160.0),
        fill: "#FF0000",
    },
    SvgRect {
        center: Vec2::new(-260.104, -260.104),
        size: Vec2::new(160.0, 160.0),
        fill: "#008000",
    },
    SvgRect {
        center: Vec2::new(260.104, -260.104),
        size: Vec2::new(160.0, 160.0),
        fill: "#F3D849",
    },
];

const SVG_TRIANGLES: &[SvgTriangle] = &[
    SvgTriangle {
        a: Vec2::new(-340.104, 180.104),
        b: Vec2::new(-260.104, 180.104),
        c: Vec2::new(-340.104, 100.104),
        fill: "white",
    },
    SvgTriangle {
        a: Vec2::new(-260.104, 100.104),
        b: Vec2::new(-340.104, 100.104),
        c: Vec2::new(-260.104, 180.104),
        fill: "#008000",
    },
    SvgTriangle {
        a: Vec2::new(-180.104, 100.104),
        b: Vec2::new(-180.104, 180.104),
        c: Vec2::new(-100.104, 100.104),
        fill: "#F3D849",
    },
    SvgTriangle {
        a: Vec2::new(-100.104, 180.104),
        b: Vec2::new(-100.104, 100.104),
        c: Vec2::new(-180.104, 180.104),
        fill: "#008000",
    },
    SvgTriangle {
        a: Vec2::new(-100.104, 260.104),
        b: Vec2::new(-180.104, 260.104),
        c: Vec2::new(-100.104, 340.104),
        fill: "#F3D849",
    },
    SvgTriangle {
        a: Vec2::new(0.419, -0.104),
        b: Vec2::new(-59.685, 60.0),
        c: Vec2::new(60.523, 60.0),
        fill: "#FF0000",
    },
    SvgTriangle {
        a: Vec2::new(340.104, -180.104),
        b: Vec2::new(260.104, -180.104),
        c: Vec2::new(340.104, -100.104),
        fill: "white",
    },
    SvgTriangle {
        a: Vec2::new(260.104, -100.104),
        b: Vec2::new(340.104, -100.104),
        c: Vec2::new(260.104, -180.104),
        fill: "#FF0000",
    },
    SvgTriangle {
        a: Vec2::new(180.104, -100.104),
        b: Vec2::new(180.104, -180.104),
        c: Vec2::new(100.104, -100.104),
        fill: "#0080FF",
    },
    SvgTriangle {
        a: Vec2::new(100.104, -180.104),
        b: Vec2::new(100.104, -100.104),
        c: Vec2::new(180.104, -180.104),
        fill: "#FF0000",
    },
    SvgTriangle {
        a: Vec2::new(100.104, -260.104),
        b: Vec2::new(180.104, -260.104),
        c: Vec2::new(100.104, -340.104),
        fill: "#0080FF",
    },
    SvgTriangle {
        a: Vec2::new(-0.419, 0.104),
        b: Vec2::new(59.685, -60.0),
        c: Vec2::new(-60.523, -60.0),
        fill: "#008000",
    },
    SvgTriangle {
        a: Vec2::new(-180.104, -340.104),
        b: Vec2::new(-180.104, -260.104),
        c: Vec2::new(-100.104, -340.104),
        fill: "white",
    },
    SvgTriangle {
        a: Vec2::new(-100.104, -260.104),
        b: Vec2::new(-100.104, -340.104),
        c: Vec2::new(-180.104, -260.104),
        fill: "#F3D849",
    },
    SvgTriangle {
        a: Vec2::new(-100.104, -180.104),
        b: Vec2::new(-180.104, -180.104),
        c: Vec2::new(-100.104, -100.104),
        fill: "#FF0000",
    },
    SvgTriangle {
        a: Vec2::new(-180.104, -100.104),
        b: Vec2::new(-100.104, -100.104),
        c: Vec2::new(-180.104, -180.104),
        fill: "#F3D849",
    },
    SvgTriangle {
        a: Vec2::new(-260.104, -100.104),
        b: Vec2::new(-260.104, -180.104),
        c: Vec2::new(-340.104, -100.104),
        fill: "#FF0000",
    },
    SvgTriangle {
        a: Vec2::new(0.104, 0.419),
        b: Vec2::new(-60.0, -59.685),
        c: Vec2::new(-60.0, 60.523),
        fill: "#0080FF",
    },
    SvgTriangle {
        a: Vec2::new(180.317, 340.104),
        b: Vec2::new(180.317, 260.104),
        c: Vec2::new(100.317, 340.104),
        fill: "white",
    },
    SvgTriangle {
        a: Vec2::new(100.317, 260.104),
        b: Vec2::new(100.317, 340.104),
        c: Vec2::new(180.317, 260.104),
        fill: "#0080FF",
    },
    SvgTriangle {
        a: Vec2::new(100.317, 180.104),
        b: Vec2::new(180.317, 180.104),
        c: Vec2::new(100.318, 100.104),
        fill: "#008000",
    },
    SvgTriangle {
        a: Vec2::new(180.317, 100.104),
        b: Vec2::new(100.317, 100.104),
        c: Vec2::new(180.317, 180.104),
        fill: "#0080FF",
    },
    SvgTriangle {
        a: Vec2::new(260.317, 100.104),
        b: Vec2::new(260.317, 180.104),
        c: Vec2::new(340.317, 100.104),
        fill: "#008000",
    },
    SvgTriangle {
        a: Vec2::new(-0.104, 0.0),
        b: Vec2::new(60.0, 60.104),
        c: Vec2::new(60.0, -60.104),
        fill: "#F3D849",
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::player::{PlayerControl, PlayerState};
    use crate::gameplay::match_flow::PlayerProfile;

    fn player(player_id: u8, seat: PlayerSeat) -> PlayerProfile {
        PlayerProfile {
            state: PlayerState {
                player_id,
                team_id: player_id,
                control: PlayerControl::Human,
            },
            seat,
            color: seat.to_color(),
            hangar_slots: Vec::new(),
            launch_position: Vec2::ZERO,
            launch_tile_index: 0,
            home_lane_positions: Vec::new(),
            goal_position: Vec2::ZERO,
        }
    }

    fn roster_with_players(players: Vec<PlayerProfile>) -> PlayerRoster {
        let [blue, red, green, yellow] = PlayerSeat::ALL.map(PlayerSeat::to_color);
        PlayerRoster {
            players,
            player_colors: [blue, red, green, yellow],
        }
    }

    #[test]
    fn route_colors_keep_full_four_color_palette_in_one_vs_one() {
        let [blue, red, green, yellow] = PlayerSeat::ALL.map(PlayerSeat::to_color);
        let palette = BoardPalette::from_player_roster(&PlayerRoster {
            players: vec![player(1, PlayerSeat::Blue), player(2, PlayerSeat::Red)],
            player_colors: [blue, red, green, yellow],
        });

        assert_eq!(palette.color_for_route_index(0), blue);
        assert_eq!(palette.color_for_route_index(1), red);
        assert_eq!(palette.color_for_route_index(2), green);
        assert_eq!(palette.color_for_route_index(3), yellow);
    }

    #[test]
    fn inactive_svg_slots_keep_configured_palette_colors() {
        let [blue, red, green, yellow] = PlayerSeat::ALL.map(PlayerSeat::to_color);
        let palette = BoardPalette::from_player_roster(&PlayerRoster {
            players: vec![player(1, PlayerSeat::Blue), player(2, PlayerSeat::Red)],
            player_colors: [blue, red, green, yellow],
        });

        assert_eq!(palette.color_for_svg_fill("#008000"), green);
        assert_eq!(palette.color_for_svg_fill("#F3D849"), yellow);
    }

    #[test]
    fn one_vs_one_home_lanes_keep_only_inactive_entry_dots() {
        let [blue, red, green, yellow] = PlayerSeat::ALL.map(PlayerSeat::to_color);
        let roster = PlayerRoster {
            players: vec![player(1, PlayerSeat::Blue), player(2, PlayerSeat::Red)],
            player_colors: [blue, red, green, yellow],
        };

        let dots = visible_home_lane_dots(&roster);
        assert_eq!(dots.len(), 14);
        assert_eq!(
            dots.iter()
                .filter(|dot| dot.seat == PlayerSeat::Blue)
                .count(),
            6
        );
        assert_eq!(
            dots.iter()
                .filter(|dot| dot.seat == PlayerSeat::Red)
                .count(),
            6
        );
        assert_eq!(
            dots.iter()
                .filter(|dot| dot.seat == PlayerSeat::Green)
                .count(),
            1
        );
        assert_eq!(
            dots.iter()
                .filter(|dot| dot.seat == PlayerSeat::Yellow)
                .count(),
            1
        );
        assert!(
            dots.iter()
                .any(|dot| dot.seat == PlayerSeat::Blue && dot.show_turn_marker)
        );
        assert!(
            dots.iter()
                .any(|dot| dot.seat == PlayerSeat::Red && dot.show_turn_marker)
        );
        assert!(
            dots.iter()
                .all(|dot| matches!(dot.seat, PlayerSeat::Blue | PlayerSeat::Red)
                    || !dot.show_turn_marker)
        );
        assert!(
            dots.iter()
                .any(|dot| dot.position == Vec2::new(0.104, -300.104))
        );
        assert!(
            !dots
                .iter()
                .any(|dot| dot.position == Vec2::new(0.104, -240.104))
        );
        assert!(should_draw_svg_rect(
            svg_rect_at(Vec2::new(0.104, -300.104)),
            &roster
        ));
        assert!(!should_draw_svg_rect(
            svg_rect_at(Vec2::new(0.104, -240.104)),
            &roster
        ));
        assert!(should_draw_svg_rect(
            svg_rect_at(Vec2::new(300.317, 0.104)),
            &roster
        ));
        assert!(!should_draw_svg_rect(
            svg_rect_at(Vec2::new(240.317, 0.104)),
            &roster
        ));
    }

    #[test]
    fn one_vs_one_home_lanes_follow_active_seats_not_player_ids() {
        let [blue, red, green, yellow] = PlayerSeat::ALL.map(PlayerSeat::to_color);
        let roster = PlayerRoster {
            players: vec![player(1, PlayerSeat::Yellow), player(2, PlayerSeat::Green)],
            player_colors: [blue, red, green, yellow],
        };

        let dots = visible_home_lane_dots(&roster);
        assert_eq!(
            dots.iter()
                .filter(|dot| dot.seat == PlayerSeat::Blue)
                .count(),
            1
        );
        assert_eq!(
            dots.iter()
                .filter(|dot| dot.seat == PlayerSeat::Red)
                .count(),
            1
        );
        assert_eq!(
            dots.iter()
                .filter(|dot| dot.seat == PlayerSeat::Green)
                .count(),
            6
        );
        assert_eq!(
            dots.iter()
                .filter(|dot| dot.seat == PlayerSeat::Yellow)
                .count(),
            6
        );
        assert!(!should_draw_svg_rect(
            svg_rect_at(Vec2::new(-240.104, -0.104)),
            &roster
        ));
        assert!(!should_draw_svg_rect(
            svg_rect_at(Vec2::new(-0.104, 240.104)),
            &roster
        ));
        assert!(should_draw_svg_rect(
            svg_rect_at(Vec2::new(0.104, -240.104)),
            &roster
        ));
        assert!(should_draw_svg_rect(
            svg_rect_at(Vec2::new(240.317, 0.104)),
            &roster
        ));
    }

    #[test]
    fn two_vs_two_home_lanes_show_all_dots_and_turn_markers() {
        let [blue, red, green, yellow] = PlayerSeat::ALL.map(PlayerSeat::to_color);
        let roster = PlayerRoster {
            players: vec![
                player(1, PlayerSeat::Blue),
                player(2, PlayerSeat::Red),
                player(3, PlayerSeat::Green),
                player(4, PlayerSeat::Yellow),
            ],
            player_colors: [blue, red, green, yellow],
        };

        let dots = visible_home_lane_dots(&roster);
        assert_eq!(dots.len(), 24);
        for seat in PlayerSeat::ALL {
            assert_eq!(dots.iter().filter(|dot| dot.seat == seat).count(), 6);
            assert!(
                dots.iter()
                    .any(|dot| dot.seat == seat && dot.show_turn_marker)
            );
        }
        assert_eq!(home_lane_turn_direction(PlayerSeat::Blue), Vec2::X);
        assert_eq!(home_lane_turn_direction(PlayerSeat::Red), Vec2::NEG_Y);
        assert_eq!(home_lane_turn_direction(PlayerSeat::Green), Vec2::Y);
        assert_eq!(home_lane_turn_direction(PlayerSeat::Yellow), Vec2::NEG_X);
        let marker_path = home_lane_turn_marker_path(Vec2::X).expect("path should resolve");
        assert_eq!(marker_path.len(), 3);
        assert_eq!(marker_path[1].y, marker_path[2].y);
        assert!(marker_path[2].x > marker_path[1].x);
        assert!(should_draw_svg_rect(
            svg_rect_at(Vec2::new(0.104, -240.104)),
            &roster
        ));
        assert!(should_draw_svg_rect(
            svg_rect_at(Vec2::new(240.317, 0.104)),
            &roster
        ));
    }

    fn svg_rect_at(center: Vec2) -> SvgRect {
        *SVG_RECTS
            .iter()
            .find(|rect| rect.center == center)
            .expect("svg rect exists")
    }

    #[test]
    fn launch_triangles_use_corner_consistent_right_angles() {
        for triangle in LAUNCH_TRIANGLES {
            assert!(
                (triangle.a.x - triangle.b.x).abs() < 0.001
                    || (triangle.a.y - triangle.b.y).abs() < 0.001
            );
            assert!(
                (triangle.a.x - triangle.c.x).abs() < 0.001
                    || (triangle.a.y - triangle.c.y).abs() < 0.001
            );
        }
    }

    #[test]
    fn dice_pip_visibility_matches_standard_faces() {
        assert!(pip_visible_for_roll(1, DicePipSlot::Center));
        assert!(!pip_visible_for_roll(1, DicePipSlot::TopLeft));

        assert!(pip_visible_for_roll(2, DicePipSlot::TopLeft));
        assert!(pip_visible_for_roll(2, DicePipSlot::BottomRight));
        assert!(!pip_visible_for_roll(2, DicePipSlot::Center));

        assert!(pip_visible_for_roll(5, DicePipSlot::Center));
        assert!(pip_visible_for_roll(6, DicePipSlot::MiddleLeft));
        assert!(pip_visible_for_roll(6, DicePipSlot::MiddleRight));
        assert!(!pip_visible_for_roll(0, DicePipSlot::Center));
    }

    #[test]
    fn dice_display_grays_past_rolls_when_no_current_roll() {
        let mut turn_state = TurnState::opening_turn();
        turn_state.current_player = 2;
        turn_state.current_roll = None;
        turn_state.last_roll = Some(6);
        turn_state.last_roll_faces = Some([6, 0]);
        turn_state.last_roll_player = Some(1);
        turn_state.player_last_rolls = [Some(6), Some(3), None, None];
        turn_state.player_last_roll_faces = [Some([6, 0]), Some([3, 0]), None, None];

        assert_eq!(
            player_dice_display_state(&turn_state, 1, false),
            PlayerDiceDisplayState::Disabled([6, 0])
        );
        assert_eq!(
            player_dice_display_state(&turn_state, 2, false),
            PlayerDiceDisplayState::Disabled([3, 0])
        );
        assert_eq!(
            player_dice_display_state(&turn_state, 3, false),
            PlayerDiceDisplayState::Hidden
        );
        assert_eq!(
            player_dice_display_state(&turn_state, 1, true),
            PlayerDiceDisplayState::Active([6, 0])
        );
        assert_eq!(
            player_dice_display_state(&turn_state, 2, true),
            PlayerDiceDisplayState::Disabled([3, 0])
        );

        turn_state.hold_last_roll_display = true;
        assert_eq!(
            player_dice_display_state(&turn_state, 1, false),
            PlayerDiceDisplayState::Active([6, 0])
        );
        turn_state.hold_last_roll_display = false;

        turn_state.current_roll = Some(4);
        turn_state.current_roll_faces = Some([4, 0]);

        assert_eq!(
            player_dice_display_state(&turn_state, 1, true),
            PlayerDiceDisplayState::Disabled([6, 0])
        );
        assert_eq!(
            player_dice_display_state(&turn_state, 2, true),
            PlayerDiceDisplayState::Active([4, 0])
        );
    }

    #[test]
    fn dice_display_collapses_double_dice_faces_to_final_roll() {
        let mut turn_state = TurnState::opening_turn();
        turn_state.current_player = 1;
        turn_state.current_roll = Some(5);
        turn_state.current_roll_faces = Some([2, 5]);

        let display_state = player_dice_display_state(&turn_state, 1, false);
        assert_eq!(display_state, PlayerDiceDisplayState::Active([5, 0]));
        assert_eq!(display_state.roll(0), Some(5));
        assert_eq!(display_state.roll(1), None);

        let base_center = Vec2::new(10.0, 20.0);
        assert_eq!(dice_center_for_index(base_center, [5, 0], 0), base_center);
        assert_eq!(dice_center_for_index(base_center, [5, 0], 1), base_center);

        turn_state.current_roll = None;
        turn_state.player_last_rolls = [Some(5), None, None, None];
        turn_state.player_last_roll_faces = [Some([2, 5]), None, None, None];
        assert_eq!(
            player_dice_display_state(&turn_state, 1, false),
            PlayerDiceDisplayState::Disabled([5, 0])
        );
    }

    #[test]
    fn player_dice_display_waits_for_roll_animation_commit() {
        let mut turn_state = TurnState::opening_turn();
        crate::gameplay::turn_flow::set_roll_with_faces(&mut turn_state, 5, [2, 5]);

        assert_eq!(
            player_dice_display_state(&turn_state, 1, false),
            PlayerDiceDisplayState::Hidden
        );

        assert!(crate::gameplay::turn_flow::commit_pending_roll_display(
            &mut turn_state,
            1
        ));
        assert_eq!(
            player_dice_display_state(&turn_state, 1, false),
            PlayerDiceDisplayState::Active([5, 0])
        );
    }

    #[test]
    fn dice_roll_visual_key_tracks_current_and_last_rolls() {
        let mut turn_state = TurnState::opening_turn();
        crate::gameplay::turn_flow::set_roll_with_faces(&mut turn_state, 5, [2, 5]);

        assert_eq!(
            visible_dice_roll_key(&turn_state),
            Some(DiceRollVisualKey {
                roll_serial: 1,
                player_id: 1,
                roll: 5,
                faces: [2, 5],
            })
        );

        turn_state.current_player = 2;
        turn_state.current_roll = None;
        turn_state.current_roll_faces = None;

        assert_eq!(
            visible_dice_roll_key(&turn_state),
            Some(DiceRollVisualKey {
                roll_serial: 1,
                player_id: 1,
                roll: 5,
                faces: [2, 5],
            })
        );
    }

    #[test]
    fn dice_roll_visual_key_tracks_pending_double_dice_choice() {
        let mut turn_state = TurnState::opening_turn();
        crate::gameplay::turn_flow::set_pending_double_dice_choice(&mut turn_state, [6, 2]);

        assert_eq!(turn_state.current_roll, None);
        assert_eq!(
            visible_dice_roll_key(&turn_state),
            Some(DiceRollVisualKey {
                roll_serial: 1,
                player_id: 1,
                roll: 6,
                faces: [6, 2],
            })
        );
    }

    #[test]
    fn dice_roll_animation_faces_settle_on_real_result() {
        let key = DiceRollVisualKey {
            roll_serial: 7,
            player_id: 2,
            roll: 6,
            faces: [3, 6],
        };

        let rolling_faces =
            dice_roll_animation_faces(DiceRollVisualAnimation { key, elapsed: 0.0 });
        assert!(dice_face_for_index(rolling_faces, 0).is_some());
        assert!(dice_face_for_index(rolling_faces, 1).is_some());

        let settled_faces = dice_roll_animation_faces(DiceRollVisualAnimation {
            key,
            elapsed: DICE_ROLL_SETTLE_START,
        });
        assert_eq!(settled_faces, [3, 6]);
    }

    #[test]
    fn dice_roll_animation_keeps_single_die_layout_for_normal_roll() {
        let key = DiceRollVisualKey {
            roll_serial: 8,
            player_id: 1,
            roll: 4,
            faces: [4, 0],
        };

        let rolling_faces =
            dice_roll_animation_faces(DiceRollVisualAnimation { key, elapsed: 0.0 });
        assert!(dice_face_for_index(rolling_faces, 0).is_some());
        assert_eq!(dice_face_for_index(rolling_faces, 1), None);
    }

    #[test]
    fn center_dice_roll_animation_temporarily_replaces_faces_and_transform() {
        let key = DiceRollVisualKey {
            roll_serial: 9,
            player_id: 1,
            roll: 5,
            faces: [2, 5],
        };
        let animation = DiceRollVisualAnimation { key, elapsed: 0.12 };

        let rolling_faces = dice_roll_animation_faces(animation);
        assert!(dice_face_for_index(rolling_faces, 0).is_some());
        assert!(dice_face_for_index(rolling_faces, 1).is_some());

        let rolling_transform = dice_roll_visual_transform(animation, 0);
        assert!(rolling_transform.offset.length() > 0.0);
        assert!(rolling_transform.scale.x > 1.0 || rolling_transform.scale.y > 1.0);
        assert!(rolling_transform.rotation.abs() > DICE_ROLL_MAX_ROTATION);
        assert!(center_dice_center_for_index(Vec2::ZERO, [2, 5], 0).x < 0.0);
        assert!(center_dice_center_for_index(Vec2::ZERO, [2, 5], 1).x > 0.0);

        let settled_transform = dice_roll_visual_transform(
            DiceRollVisualAnimation {
                key,
                elapsed: DICE_ROLL_ANIMATION_DURATION,
            },
            0,
        );
        assert_eq!(settled_transform, DiceRollVisualTransform::default());
    }

    #[test]
    fn dice_roll_animation_timing_is_readable() {
        assert!(DICE_ROLL_ANIMATION_DURATION <= 2.0);
        assert_eq!(DICE_ROLL_ANIMATION_DURATION, 1.8);
        assert_eq!(DICE_ROLL_SETTLE_START, 1.35);
        assert!((DICE_ROLL_ANIMATION_DURATION - DICE_ROLL_SETTLE_START - 0.45).abs() < 0.001);
    }

    #[test]
    fn dice_roll_animation_delta_uses_real_elapsed_time() {
        assert_eq!(dice_roll_animation_delta(-1.0), 0.0);
        assert_eq!(dice_roll_animation_delta(0.016), 0.016);
        assert_eq!(dice_roll_animation_delta(3.0), 3.0);
    }

    #[test]
    fn center_dice_prompt_only_appears_when_human_can_roll() {
        let mut turn_state = TurnState::opening_turn();
        let roster = roster_with_players(vec![player(1, PlayerSeat::Blue)]);
        let match_result = MatchResult::default();

        assert!(center_dice_prompt_visible(
            &turn_state,
            &GamePhase::AwaitDice,
            &roster,
            &match_result,
        ));

        turn_state.current_roll = Some(3);
        assert!(!center_dice_prompt_visible(
            &turn_state,
            &GamePhase::AwaitDice,
            &roster,
            &match_result,
        ));

        turn_state.current_roll = None;
        let mut finished = MatchResult::default();
        finished.finished = true;
        assert!(!center_dice_prompt_visible(
            &turn_state,
            &GamePhase::AwaitDice,
            &roster,
            &finished,
        ));
    }

    #[test]
    fn center_dice_prompt_ignores_ai_turns_and_other_phases() {
        let mut ai_player = player(1, PlayerSeat::Blue);
        ai_player.state.control = PlayerControl::Ai;
        let roster = roster_with_players(vec![ai_player]);
        let turn_state = TurnState::opening_turn();
        let match_result = MatchResult::default();

        assert!(!center_dice_prompt_visible(
            &turn_state,
            &GamePhase::AwaitDice,
            &roster,
            &match_result,
        ));
        assert!(!center_dice_prompt_visible(
            &turn_state,
            &GamePhase::AwaitPieceSelect,
            &roster,
            &match_result,
        ));
    }

    #[test]
    fn current_turn_profile_hides_after_match_finished() {
        let mut turn_state = TurnState::opening_turn();
        turn_state.current_player = 2;
        let roster = roster_with_players(vec![
            player(1, PlayerSeat::Blue),
            player(2, PlayerSeat::Red),
        ]);
        let mut match_result = MatchResult::default();

        assert_eq!(
            current_turn_profile(&turn_state, &roster, &match_result)
                .map(|player| player.state.player_id),
            Some(2)
        );

        match_result.finished = true;
        assert!(current_turn_profile(&turn_state, &roster, &match_result).is_none());
    }

    #[test]
    fn center_dice_turn_halo_follows_prompt_and_animation_player() {
        let mut turn_state = TurnState::opening_turn();
        let roster = roster_with_players(vec![
            player(1, PlayerSeat::Blue),
            player(2, PlayerSeat::Red),
        ]);
        let match_result = MatchResult::default();
        let mut visual_state = DiceRollVisualState::default();

        let prompt = center_dice_turn_halo_state(
            &turn_state,
            &visual_state,
            &GamePhase::AwaitDice,
            &roster,
            &match_result,
        )
        .unwrap();
        assert_eq!(prompt.0.state.player_id, 1);
        assert_eq!(prompt.1, [1, 0]);

        let key = DiceRollVisualKey {
            roll_serial: 9,
            player_id: 2,
            roll: 5,
            faces: [2, 5],
        };
        visual_state.animation = Some(DiceRollVisualAnimation {
            key,
            elapsed: DICE_ROLL_SETTLE_START,
        });
        turn_state.current_player = 1;
        let rolling = center_dice_turn_halo_state(
            &turn_state,
            &visual_state,
            &GamePhase::DiceRolling,
            &roster,
            &match_result,
        )
        .unwrap();
        assert_eq!(rolling.0.state.player_id, 2);
        assert_eq!(rolling.1, [2, 5]);
    }

    #[test]
    fn center_dice_turn_halo_hides_without_visible_center_dice() {
        let mut turn_state = TurnState::opening_turn();
        let roster = roster_with_players(vec![player(1, PlayerSeat::Blue)]);
        let match_result = MatchResult::default();
        let visual_state = DiceRollVisualState::default();

        turn_state.current_roll = Some(3);
        assert!(
            center_dice_turn_halo_state(
                &turn_state,
                &visual_state,
                &GamePhase::AwaitPieceSelect,
                &roster,
                &match_result,
            )
            .is_none()
        );
    }

    #[test]
    fn center_dice_turn_halo_widens_for_double_dice() {
        let single = center_dice_turn_halo_scale([4, 0], 0.0);
        let double = center_dice_turn_halo_scale([2, 5], 0.0);

        assert!((single.x - single.y).abs() < 0.001);
        assert!(double.x > double.y * 1.45);
        assert!((double.y - single.y * 1.02).abs() < 0.001);
    }

    #[test]
    fn center_dice_prompt_uses_distinct_angled_asset_motion() {
        let start = center_dice_prompt_transform(0.0);
        let later = center_dice_prompt_transform(0.5);

        assert_eq!(
            dice_roll_frame_asset_path(CENTER_DICE_PROMPT_FRAME),
            "ui/dice/roll_00.png"
        );
        assert_ne!(start, later);
        assert!(start.rotation.abs() > 0.1);
    }

    #[test]
    fn dice_face_assets_cover_every_roll_value() {
        assert_eq!(dice_face_asset_path(1), "ui/dice/die_1.png");
        assert_eq!(dice_face_asset_path(2), "ui/dice/die_2.png");
        assert_eq!(dice_face_asset_path(3), "ui/dice/die_3.png");
        assert_eq!(dice_face_asset_path(4), "ui/dice/die_4.png");
        assert_eq!(dice_face_asset_path(5), "ui/dice/die_5.png");
        assert_eq!(dice_face_asset_path(6), "ui/dice/die_6.png");
        assert_eq!(dice_roll_frame_asset_path(0), "ui/dice/roll_00.png");
        assert_eq!(dice_roll_frame_asset_path(15), "ui/dice/roll_15.png");
    }

    #[test]
    fn center_dice_uses_roll_frames_before_settling_on_final_face() {
        let key = DiceRollVisualKey {
            roll_serial: 12,
            player_id: 1,
            roll: 6,
            faces: [2, 6],
        };

        assert!(
            dice_sprite_asset_path(DiceRollVisualAnimation { key, elapsed: 0.2 }, 6, 1)
                .starts_with("ui/dice/roll_")
        );
        assert_eq!(
            dice_sprite_asset_path(
                DiceRollVisualAnimation {
                    key,
                    elapsed: DICE_ROLL_SETTLE_START,
                },
                6,
                1,
            ),
            "ui/dice/die_6.png"
        );
    }

    #[test]
    fn hangar_centers_match_visual_seat_quadrants() {
        assert_eq!(
            hangar_center_for_seat(PlayerSeat::Blue),
            Vec2::new(-265.104, 265.104)
        );
        assert_eq!(
            hangar_center_for_seat(PlayerSeat::Red),
            Vec2::new(265.317, 265.104)
        );
        assert_eq!(
            hangar_center_for_seat(PlayerSeat::Green),
            Vec2::new(-265.104, -265.104)
        );
        assert_eq!(
            hangar_center_for_seat(PlayerSeat::Yellow),
            Vec2::new(265.104, -265.104)
        );
    }

    #[test]
    fn hangar_backgrounds_are_visually_inset_from_runway_tiles() {
        let hangar = svg_rect_at(Vec2::new(-260.104, 260.104));
        let runway = svg_rect_at(Vec2::new(-300.104, 80.104));
        let (hangar_min, hangar_max) = visual_rect_bounds(hangar);
        let (original_min, original_max) = (
            hangar.center - hangar.size * 0.5,
            hangar.center + hangar.size * 0.5,
        );

        assert!(is_hangar_background_rect(hangar));
        assert!(!is_hangar_background_rect(runway));
        assert_eq!(
            visual_rect_geometry(hangar),
            VisualRectGeometry {
                center: Vec2::new(-265.104, 265.104),
                size: Vec2::new(150.0, 150.0)
            }
        );
        assert_eq!(hangar_min.x, original_min.x);
        assert_eq!(hangar_max.y, original_max.y);
        assert!((original_max.x - hangar_max.x - 10.0).abs() < 0.001);
        assert!((hangar_min.y - original_min.y - 10.0).abs() < 0.001);
        assert_eq!(
            visual_rect_geometry(runway),
            VisualRectGeometry {
                center: runway.center,
                size: runway.size
            }
        );
    }
}
