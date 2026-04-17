use bevy::prelude::*;

use crate::data::game_mode::GameMode;
use crate::domain::piece::PieceState;
use crate::gameplay::match_flow::{MatchConfig, MatchResult, PlayerRoster};
use crate::gameplay::skill_flow::{
    SkillRoster, arm_dash, arm_double_dice, build_skill_roster, can_use_skill_this_turn,
    current_player_type, dash_bonus, mark_skill_used, player_skill_state, spend_shield_charge,
    spend_snipe_charge, spend_swap_charge, sync_turn_skill_usage,
};
use crate::gameplay::turn_flow::MAIN_ROUTE_STEPS;
use crate::gameplay::turn_flow::TurnState;
use crate::plugins::piece_plugin::{HangarSlot, PieceId};
use crate::states::{AppState, GamePhase};

pub struct SkillPlugin;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SkillUiAction {
    Shield,
    Snipe,
    Swap,
    DoubleDice,
    Dash,
}

#[derive(Resource, Default)]
pub struct SkillUiRequest {
    pending: Option<SkillUiAction>,
}

impl SkillUiRequest {
    /// 向技能请求队列写入一次动作（同一时刻只保留一个待处理请求）。
    pub fn queue(&mut self, action: SkillUiAction) {
        if self.pending.is_none() {
            self.pending = Some(action);
        }
    }

    /// 取出并清空待处理技能请求。
    fn take(&mut self) -> Option<SkillUiAction> {
        self.pending.take()
    }
}

#[derive(Resource, Default)]
pub struct SkillTargetState {
    candidate_piece_ids: Vec<u8>,
    pub prompt: Option<String>,
    active: bool,
}

impl SkillTargetState {
    /// 当前技能（如 Snipe）可选目标列表。
    pub fn candidate_piece_ids(&self) -> &[u8] {
        &self.candidate_piece_ids
    }

    /// 是否处于“等待技能目标选择”状态。
    pub fn is_active(&self) -> bool {
        self.active
    }
}

impl Plugin for SkillPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), setup_skill_roster)
            .add_systems(
                Update,
                (
                    sync_skill_turn_state,
                    handle_human_skill_input,
                    handle_human_snipe_key_select,
                    handle_human_snipe_click,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(OnExit(AppState::InGame), cleanup_skill_roster);
    }
}

fn setup_skill_roster(mut commands: Commands, player_roster: Res<PlayerRoster>) {
    // 进入对局时初始化技能资源与技能目标状态。
    commands.insert_resource(build_skill_roster(&player_roster));
    commands.insert_resource(SkillTargetState::default());
    commands.insert_resource(SkillUiRequest::default());
}

fn cleanup_skill_roster(mut commands: Commands) {
    // 离开对局时回收技能相关资源。
    commands.remove_resource::<SkillRoster>();
    commands.remove_resource::<SkillTargetState>();
    commands.remove_resource::<SkillUiRequest>();
}

fn sync_skill_turn_state(
    turn_state: Res<TurnState>,
    mut skill_roster: ResMut<SkillRoster>,
    mut target_state: ResMut<SkillTargetState>,
) {
    // 同步当前回合玩家技能状态；若玩家切换则清除旧目标选择。
    sync_turn_skill_usage(&mut skill_roster, turn_state.current_player);
    if target_state.is_active()
        && skill_roster.active_turn_player != Some(turn_state.current_player)
    {
        clear_target_state(&mut target_state);
    }
}

/// 人类技能入口：统一处理按键与 HUD 点击触发的技能动作。
fn handle_human_skill_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    match_config: Res<MatchConfig>,
    player_roster: Res<PlayerRoster>,
    match_result: Res<MatchResult>,
    game_phase: Res<State<GamePhase>>,
    turn_state: Res<TurnState>,
    mut skill_roster: ResMut<SkillRoster>,
    mut skill_ui_request: ResMut<SkillUiRequest>,
    mut target_state: ResMut<SkillTargetState>,
    mut next_phase: ResMut<NextState<GamePhase>>,
    mut piece_query: Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) {
    let requested_action = if keyboard.just_pressed(KeyCode::KeyQ) {
        Some(SkillUiAction::Shield)
    } else if keyboard.just_pressed(KeyCode::KeyS) {
        Some(SkillUiAction::Snipe)
    } else if keyboard.just_pressed(KeyCode::KeyW) {
        Some(SkillUiAction::DoubleDice)
    } else if keyboard.just_pressed(KeyCode::KeyE) {
        Some(SkillUiAction::Dash)
    } else if keyboard.just_pressed(KeyCode::KeyA) {
        Some(SkillUiAction::Swap)
    } else {
        None
    }
    .or_else(|| skill_ui_request.take());

    let Some(action) = requested_action else {
        return;
    };

    if match_result.finished {
        return;
    }

    if current_player_type(&player_roster, turn_state.current_player)
        != Some(crate::domain::player::PlayerControl::Human)
    {
        return;
    }

    if !can_use_skill_this_turn(&skill_roster, turn_state.current_player) {
        let blocked_by_event = player_skill_state(&skill_roster, turn_state.current_player)
            .map(|state| state.skill_blocked_this_turn)
            .unwrap_or(false);
        skill_roster.last_skill_action = Some(if blocked_by_event {
            format!(
                "P{} cannot use skills this turn (event lock)",
                turn_state.current_player
            )
        } else {
            format!(
                "P{} already used a skill this turn",
                turn_state.current_player
            )
        });
        return;
    }

    match action {
        SkillUiAction::Shield if matches!(game_phase.get(), GamePhase::AwaitDice) => {
            let Some(target_piece_id) =
                preferred_shield_target_for_full_query(turn_state.current_player, &piece_query)
            else {
                skill_roster.last_skill_action = Some(format!(
                    "P{} could not find a piece for Shield",
                    turn_state.current_player
                ));
                return;
            };

            if !spend_shield_charge(&mut skill_roster, turn_state.current_player) {
                skill_roster.last_skill_action = Some(format!(
                    "P{} has no Shield charges left",
                    turn_state.current_player
                ));
                return;
            }

            if let Some(shield_value) =
                apply_shield_to_piece_for_full_query(target_piece_id, &mut piece_query)
            {
                mark_skill_used(&mut skill_roster, turn_state.current_player);
                skill_roster.last_skill_action = Some(format!(
                    "P{} used Shield on piece #{} ({})",
                    turn_state.current_player, target_piece_id, shield_value
                ));
            }
        }
        SkillUiAction::Snipe if matches!(game_phase.get(), GamePhase::AwaitDice) => {
            let Some(current_player_profile) = player_roster
                .players
                .iter()
                .find(|player| player.state.player_id == turn_state.current_player)
            else {
                return;
            };
            let targets = collect_snipe_targets_for_full_query(
                turn_state.current_player,
                current_player_profile.state.team_id,
                &piece_query,
            );

            if targets.is_empty() {
                skill_roster.last_skill_action = Some(format!(
                    "P{} found no Snipe target",
                    turn_state.current_player
                ));
                return;
            }
            if !spend_snipe_charge(&mut skill_roster, turn_state.current_player) {
                skill_roster.last_skill_action = Some(format!(
                    "P{} has no Snipe charges left",
                    turn_state.current_player
                ));
                return;
            }

            if targets.len() == 1 {
                mark_skill_used(&mut skill_roster, turn_state.current_player);
                skill_roster.last_skill_action = Some(execute_snipe(targets[0], &mut piece_query));
                return;
            }

            mark_skill_used(&mut skill_roster, turn_state.current_player);
            target_state.candidate_piece_ids = targets;
            target_state.prompt = Some(format!(
                "Select a Snipe target with click or {}",
                (1..=target_state.candidate_piece_ids.len())
                    .map(|index| index.to_string())
                    .collect::<Vec<_>>()
                    .join("/")
            ));
            target_state.active = true;
            next_phase.set(GamePhase::ResolveSkillEffect);
        }
        SkillUiAction::DoubleDice if matches!(game_phase.get(), GamePhase::AwaitDice) => {
            if arm_double_dice(&mut skill_roster, turn_state.current_player) {
                mark_skill_used(&mut skill_roster, turn_state.current_player);
                skill_roster.last_skill_action = Some(format!(
                    "P{} armed DoubleDice for the next roll",
                    turn_state.current_player
                ));
            } else {
                let armed = player_skill_state(&skill_roster, turn_state.current_player)
                    .map(|state| state.double_dice_armed)
                    .unwrap_or(false);
                let message = if armed {
                    format!(
                        "P{} already has DoubleDice armed",
                        turn_state.current_player
                    )
                } else {
                    format!(
                        "P{} has no DoubleDice charges left",
                        turn_state.current_player
                    )
                };
                skill_roster.last_skill_action = Some(message);
            }
        }
        SkillUiAction::Dash
            if matches!(game_phase.get(), GamePhase::AwaitPieceSelect)
                && dash_bonus(&skill_roster, turn_state.current_player) == 0 =>
        {
            if arm_dash(&mut skill_roster, turn_state.current_player) {
                mark_skill_used(&mut skill_roster, turn_state.current_player);
                skill_roster.last_skill_action = Some(format!(
                    "P{} armed Dash for +3 movement",
                    turn_state.current_player
                ));
            } else {
                skill_roster.last_skill_action = Some(format!(
                    "P{} has no Dash charges left",
                    turn_state.current_player
                ));
            }
        }
        SkillUiAction::Swap if matches!(game_phase.get(), GamePhase::AwaitDice) => {
            if match_config.mode != GameMode::TwoVsTwo {
                skill_roster.last_skill_action = Some("Swap is only available in 2v2".to_string());
                return;
            }

            let Some(current_player_profile) = player_roster
                .players
                .iter()
                .find(|player| player.state.player_id == turn_state.current_player)
            else {
                return;
            };

            let Some(teammate_piece_id) = find_active_teammate_piece_for_swap(
                turn_state.current_player,
                current_player_profile.state.team_id,
                &piece_query,
            ) else {
                skill_roster.last_skill_action = Some(format!(
                    "P{} found no teammate piece to Swap with",
                    turn_state.current_player
                ));
                return;
            };
            if !current_player_has_active_piece(turn_state.current_player, &piece_query) {
                skill_roster.last_skill_action = Some(format!(
                    "P{} needs an active piece to use Swap",
                    turn_state.current_player
                ));
                return;
            }
            if !spend_swap_charge(&mut skill_roster, turn_state.current_player) {
                skill_roster.last_skill_action = Some(format!(
                    "P{} has no Swap charges left",
                    turn_state.current_player
                ));
                return;
            }

            mark_skill_used(&mut skill_roster, turn_state.current_player);
            skill_roster.last_skill_action = Some(execute_swap(
                turn_state.current_player,
                teammate_piece_id,
                &mut piece_query,
            ));
        }
        _ => {
            skill_roster.last_skill_action =
                Some("Skill not available in current phase".to_string());
        }
    }
}

/// Snipe 目标选择的键盘分支（数字键 + Esc 取消）。
fn handle_human_snipe_key_select(
    keyboard: Res<ButtonInput<KeyCode>>,
    game_phase: Res<State<GamePhase>>,
    mut target_state: ResMut<SkillTargetState>,
    mut skill_roster: ResMut<SkillRoster>,
    mut next_phase: ResMut<NextState<GamePhase>>,
    mut piece_query: Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) {
    if !matches!(game_phase.get(), GamePhase::ResolveSkillEffect) || !target_state.is_active() {
        return;
    }

    if keyboard.just_pressed(KeyCode::Escape) {
        skill_roster.last_skill_action = Some("Snipe selection cancelled".to_string());
        clear_target_state(&mut target_state);
        next_phase.set(GamePhase::AwaitDice);
        return;
    }

    let keys = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
    ];
    let Some(selection) = keys.iter().enumerate().find_map(|(index, key)| {
        (index < target_state.candidate_piece_ids.len() && keyboard.just_pressed(*key))
            .then_some(index)
    }) else {
        return;
    };

    let target_piece_id = target_state.candidate_piece_ids[selection];
    skill_roster.last_skill_action = Some(execute_snipe(target_piece_id, &mut piece_query));
    clear_target_state(&mut target_state);
    next_phase.set(GamePhase::AwaitDice);
}

/// Snipe 目标选择的鼠标分支（点击高亮目标棋子）。
fn handle_human_snipe_click(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform)>,
    game_phase: Res<State<GamePhase>>,
    mut target_state: ResMut<SkillTargetState>,
    mut skill_roster: ResMut<SkillRoster>,
    mut next_phase: ResMut<NextState<GamePhase>>,
    mut piece_query: Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) {
    if !matches!(game_phase.get(), GamePhase::ResolveSkillEffect) || !target_state.is_active() {
        return;
    }
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor_position) = window.cursor_position() else {
        return;
    };
    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };
    let Ok(cursor_world) = camera.viewport_to_world_2d(camera_transform, cursor_position) else {
        return;
    };

    let mut selected_piece_id = None;
    let mut best_distance_sq = f32::MAX;
    for (piece_id, _, _, transform) in &mut piece_query {
        if !target_state.candidate_piece_ids.contains(&piece_id.0) {
            continue;
        }

        let distance_sq = transform
            .translation
            .truncate()
            .distance_squared(cursor_world);
        if distance_sq <= 28.0 * 28.0 && distance_sq < best_distance_sq {
            best_distance_sq = distance_sq;
            selected_piece_id = Some(piece_id.0);
        }
    }

    let Some(target_piece_id) = selected_piece_id else {
        return;
    };
    skill_roster.last_skill_action = Some(execute_snipe(target_piece_id, &mut piece_query));
    clear_target_state(&mut target_state);
    next_phase.set(GamePhase::AwaitDice);
}

/// 清理技能目标选择状态。
fn clear_target_state(target_state: &mut SkillTargetState) {
    target_state.candidate_piece_ids.clear();
    target_state.prompt = None;
    target_state.active = false;
}

/// 选择 Shield 目标：优先无盾 Active，其次任意 Active，最后任意己方棋子。
fn preferred_shield_target_for_full_query(
    player_id: u8,
    piece_query: &Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> Option<u8> {
    let mut unshielded_active = Vec::new();
    let mut active = Vec::new();
    let mut any_owned = Vec::new();

    for (piece_id, _, piece_state, _) in piece_query.iter() {
        if piece_state.owner_player_id != player_id {
            continue;
        }

        any_owned.push(piece_id.0);
        if piece_state.status == crate::domain::piece::PieceStatus::Active {
            active.push(piece_id.0);
            if piece_state.shield == 0 {
                unshielded_active.push(piece_id.0);
            }
        }
    }

    unshielded_active.sort_unstable();
    active.sort_unstable();
    any_owned.sort_unstable();
    unshielded_active
        .into_iter()
        .next()
        .or_else(|| active.into_iter().next())
        .or_else(|| any_owned.into_iter().next())
}

/// 给目标棋子加盾（技能插件查询版本）。
fn apply_shield_to_piece_for_full_query(
    piece_id: u8,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> Option<u8> {
    const MAX_PIECE_SHIELD: u8 = 2;
    for (query_piece_id, _, mut piece_state, _) in piece_query.iter_mut() {
        if query_piece_id.0 != piece_id {
            continue;
        }
        piece_state.shield = piece_state.shield.saturating_add(1).min(MAX_PIECE_SHIELD);
        return Some(piece_state.shield);
    }
    None
}

/// 收集 Snipe 候选：同规则层一致，优先无盾敌人。
fn collect_snipe_targets_for_full_query(
    current_player: u8,
    current_team: u8,
    piece_query: &Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> Vec<u8> {
    let mut unshielded = Vec::new();
    let mut shielded = Vec::new();

    for (piece_id, _, piece_state, _) in piece_query.iter() {
        if piece_state.owner_player_id == current_player
            || piece_state.team_id == current_team
            || piece_state.status != crate::domain::piece::PieceStatus::Active
            || piece_state.progress >= MAIN_ROUTE_STEPS
        {
            continue;
        }

        if piece_state.shield == 0 && piece_state.stack_shield == 0 {
            unshielded.push(piece_id.0);
        } else {
            shielded.push(piece_id.0);
        }
    }

    unshielded.sort_unstable();
    shielded.sort_unstable();
    unshielded.extend(shielded);
    unshielded
}

/// 执行 Snipe：优先削盾，若无盾则送回机库。
fn execute_snipe(
    piece_id: u8,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> String {
    for (query_piece_id, hangar_slot, mut piece_state, mut transform) in piece_query.iter_mut() {
        if query_piece_id.0 != piece_id {
            continue;
        }

        if piece_state.shield > 0 {
            piece_state.shield -= 1;
            return format!("Snipe hit piece #{} and removed a shield", piece_id);
        }
        if piece_state.stack_shield > 0 {
            piece_state.stack_shield = 0;
            return format!("Snipe hit piece #{} and broke the shared shield", piece_id);
        }

        piece_state.status = crate::domain::piece::PieceStatus::InHangar;
        piece_state.progress = 0;
        piece_state.shield = 0;
        piece_state.stack_shield = 0;
        transform.translation.x = hangar_slot.0.x;
        transform.translation.y = hangar_slot.0.y;
        return format!("Snipe sent piece #{} back to hangar", piece_id);
    }

    "Snipe failed to resolve".to_string()
}

/// 判断当前玩家是否至少有一枚 Active 棋子（Swap 前置条件）。
fn current_player_has_active_piece(
    current_player: u8,
    piece_query: &Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> bool {
    piece_query.iter().any(|(_, _, piece_state, _)| {
        piece_state.owner_player_id == current_player
            && piece_state.status == crate::domain::piece::PieceStatus::Active
    })
}

/// 查找可用于 Swap 的队友 Active 棋子。
fn find_active_teammate_piece_for_swap(
    current_player: u8,
    current_team: u8,
    piece_query: &Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> Option<u8> {
    let mut candidates = piece_query
        .iter()
        .filter(|(_, _, piece_state, _)| {
            piece_state.owner_player_id != current_player
                && piece_state.team_id == current_team
                && piece_state.status == crate::domain::piece::PieceStatus::Active
        })
        .map(|(piece_id, _, _, _)| piece_id.0)
        .collect::<Vec<_>>();
    candidates.sort_unstable();
    candidates.into_iter().next()
}

/// 执行 Swap：交换双方状态与世界坐标。
fn execute_swap(
    current_player: u8,
    teammate_piece_id: u8,
    piece_query: &mut Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
) -> String {
    let Some((current_piece_id, current_state, current_translation)) =
        piece_query
            .iter()
            .find_map(|(piece_id, _, piece_state, transform)| {
                (piece_state.owner_player_id == current_player
                    && piece_state.status == crate::domain::piece::PieceStatus::Active)
                    .then_some((piece_id.0, *piece_state, transform.translation))
            })
    else {
        return "Swap failed: current player's active piece not found".to_string();
    };

    let Some((teammate_state, teammate_translation)) =
        piece_query
            .iter()
            .find_map(|(piece_id, _, piece_state, transform)| {
                (piece_id.0 == teammate_piece_id).then_some((*piece_state, transform.translation))
            })
    else {
        return "Swap failed: teammate piece not found".to_string();
    };

    for (piece_id, _, mut piece_state, mut transform) in piece_query.iter_mut() {
        if piece_id.0 == current_piece_id {
            piece_state.status = teammate_state.status;
            piece_state.progress = teammate_state.progress;
            piece_state.shield = teammate_state.shield;
            piece_state.stack_shield = teammate_state.stack_shield;
            transform.translation = teammate_translation;
        } else if piece_id.0 == teammate_piece_id {
            piece_state.status = current_state.status;
            piece_state.progress = current_state.progress;
            piece_state.shield = current_state.shield;
            piece_state.stack_shield = current_state.stack_shield;
            transform.translation = current_translation;
        }
    }

    format!(
        "Swap exchanged piece #{} with teammate piece #{}",
        current_piece_id, teammate_piece_id
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::SystemState;

    #[test]
    fn execute_snipe_consumes_normal_shield_before_hangar() {
        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::new(-10.0, 20.0)),
            PieceState {
                owner_player_id: 2,
                team_id: 2,
                status: crate::domain::piece::PieceStatus::Active,
                progress: 7,
                shield: 1,
                stack_shield: 0,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world);

        let note = execute_snipe(1, &mut query);
        let states = query
            .iter_mut()
            .map(|(piece_id, _, piece_state, _)| {
                (piece_id.0, piece_state.status, piece_state.shield)
            })
            .collect::<Vec<_>>();

        assert!(note.contains("removed a shield"));
        assert_eq!(
            states,
            vec![(1, crate::domain::piece::PieceStatus::Active, 0)]
        );
    }

    #[test]
    fn execute_snipe_sends_unshielded_piece_to_hangar() {
        let hangar = Vec2::new(30.0, -40.0);
        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(hangar),
            PieceState {
                owner_player_id: 2,
                team_id: 2,
                status: crate::domain::piece::PieceStatus::Active,
                progress: 7,
                shield: 0,
                stack_shield: 0,
            },
            Transform::from_xyz(100.0, 200.0, 0.0),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world);

        let note = execute_snipe(1, &mut query);
        let states = query
            .iter_mut()
            .map(|(piece_id, _, piece_state, transform)| {
                (
                    piece_id.0,
                    piece_state.status,
                    piece_state.progress,
                    transform.translation.x,
                    transform.translation.y,
                )
            })
            .collect::<Vec<_>>();

        assert!(note.contains("back to hangar"));
        assert_eq!(
            states,
            vec![(
                1,
                crate::domain::piece::PieceStatus::InHangar,
                0,
                hangar.x,
                hangar.y
            )]
        );
    }

    #[test]
    fn find_active_teammate_piece_for_swap_returns_smallest_matching_piece_id() {
        let mut world = World::new();
        world.spawn((
            PieceId(9),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: crate::domain::piece::PieceStatus::Active,
                progress: 3,
                shield: 0,
                stack_shield: 0,
            },
            Transform::default(),
        ));
        world.spawn((
            PieceId(4),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 3,
                team_id: 1,
                status: crate::domain::piece::PieceStatus::Active,
                progress: 10,
                shield: 0,
                stack_shield: 0,
            },
            Transform::default(),
        ));
        world.spawn((
            PieceId(7),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 5,
                team_id: 1,
                status: crate::domain::piece::PieceStatus::Active,
                progress: 12,
                shield: 0,
                stack_shield: 0,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let query = system_state.get_mut(&mut world);

        assert_eq!(find_active_teammate_piece_for_swap(1, 1, &query), Some(4));
    }

    #[test]
    fn execute_swap_exchanges_progress_and_positions_between_teammates() {
        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: crate::domain::piece::PieceStatus::Active,
                progress: 3,
                shield: 1,
                stack_shield: 0,
            },
            Transform::from_xyz(-100.0, 0.0, 0.0),
        ));
        world.spawn((
            PieceId(2),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 3,
                team_id: 1,
                status: crate::domain::piece::PieceStatus::Active,
                progress: 18,
                shield: 0,
                stack_shield: 1,
            },
            Transform::from_xyz(100.0, 50.0, 0.0),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let mut query = system_state.get_mut(&mut world);

        let note = execute_swap(1, 2, &mut query);
        let states = query
            .iter_mut()
            .map(|(piece_id, _, piece_state, transform)| {
                (
                    piece_id.0,
                    piece_state.owner_player_id,
                    piece_state.progress,
                    piece_state.shield,
                    piece_state.stack_shield,
                    transform.translation.x,
                    transform.translation.y,
                )
            })
            .collect::<Vec<_>>();

        assert!(note.contains("exchanged"));
        assert_eq!(
            states,
            vec![(1, 1, 18, 0, 1, 100.0, 50.0), (2, 3, 3, 1, 0, -100.0, 0.0),]
        );
    }

    #[test]
    fn collect_snipe_targets_for_full_query_excludes_home_lane_enemy() {
        let mut world = World::new();
        world.spawn((
            PieceId(1),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 1,
                team_id: 1,
                status: crate::domain::piece::PieceStatus::Active,
                progress: 3,
                shield: 0,
                stack_shield: 0,
            },
            Transform::default(),
        ));
        world.spawn((
            PieceId(2),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 2,
                team_id: 2,
                status: crate::domain::piece::PieceStatus::Active,
                progress: MAIN_ROUTE_STEPS + 1,
                shield: 0,
                stack_shield: 0,
            },
            Transform::default(),
        ));
        world.spawn((
            PieceId(3),
            HangarSlot(Vec2::ZERO),
            PieceState {
                owner_player_id: 3,
                team_id: 2,
                status: crate::domain::piece::PieceStatus::Active,
                progress: 6,
                shield: 0,
                stack_shield: 0,
            },
            Transform::default(),
        ));

        let mut system_state: SystemState<
            Query<(&PieceId, &HangarSlot, &mut PieceState, &mut Transform)>,
        > = SystemState::new(&mut world);
        let query = system_state.get_mut(&mut world);

        assert_eq!(collect_snipe_targets_for_full_query(1, 1, &query), vec![3]);
    }
}
