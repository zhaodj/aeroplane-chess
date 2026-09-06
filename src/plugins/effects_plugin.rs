use bevy::{
    asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology,
};

use crate::domain::piece::{PieceProgress, PieceState, PieceStatus};
use crate::gameplay::turn_flow::{EventNoticeDetail, TurnState};
use crate::i18n::{Language, LanguageSettings};
use crate::platform::DeviceProfile;
use crate::plugins::animation_plugin::{PieceAnimationSystems, PieceMoveAnimation};
use crate::plugins::piece_plugin::PieceId;
use crate::plugins::skill_plugin::{SkillSystems, SkillUiAction};
use crate::plugins::turn_plugin::TurnSystems;
use crate::plugins::ui_plugin::shared_skill_button_rect;
use crate::states::AppState;

const EFFECT_Z: f32 = 28.0;
const LOCK_DURATION: f32 = 0.34;
const MISSILE_DELAY: f32 = 0.22;
const MISSILE_DURATION: f32 = 0.32;
const SHIELD_FLASH_DURATION: f32 = 0.55;
const FLOATING_TEXT_DURATION: f32 = 0.62;
const SHIELD_GAIN_REVEAL_DURATION: f32 = 0.70;
// 浮字起始下沿高于盾牌最大脉冲上沿，避免两个反馈互相遮挡。
const SHIELD_GAIN_TEXT_OFFSET: Vec2 = Vec2::new(0.0, 32.0);
const HUD_SKILL_FX_DURATION: f32 = 0.64;
pub const TARGETED_MISSILE_REVEAL_DURATION: f32 = LOCK_DURATION + MISSILE_DELAY + MISSILE_DURATION;
const HUD_SKILL_FX_ACTIONS: [SkillUiAction; 5] = [
    SkillUiAction::Dash,
    SkillUiAction::Snipe,
    SkillUiAction::Swap,
    SkillUiAction::Shield,
    SkillUiAction::DoubleDice,
];

pub struct EffectsPlugin;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum EffectSystems {
    Prepare,
    Playback,
}

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VisualEffectQueue>()
            .init_resource::<TurnEventEffectState>()
            .init_resource::<EffectRevealDelays>()
            .init_resource::<PieceMotionEffects>()
            .configure_sets(
                Update,
                (
                    SkillSystems,
                    TurnSystems,
                    EffectSystems::Prepare,
                    PieceAnimationSystems,
                    EffectSystems::Playback,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                enqueue_turn_event_effects
                    .in_set(EffectSystems::Prepare)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (
                    play_landed_shield_gain_effects,
                    drain_visual_effect_queue,
                    animate_lock_effects,
                    animate_missile_effects,
                    animate_shield_flash_effects,
                    animate_floating_text_effects,
                    animate_hud_skill_effects,
                    tick_effect_reveal_delays,
                )
                    .chain()
                    .in_set(EffectSystems::Playback)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(OnExit(AppState::InGame), cleanup_visual_effects);
    }
}

#[derive(Resource, Default)]
struct TurnEventEffectState {
    last_action_serial: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ShieldRevealDelay {
    piece_id: u8,
    visible_delta: i8,
    remaining_secs: f32,
}

#[derive(Clone, Copy, Debug)]
struct LandingShieldGain {
    entity: Entity,
    piece_id: u8,
    motion_serial: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SkillChargeRevealDelay {
    action: SkillUiAction,
    hidden_delta: u8,
    remaining_ms: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PieceStartDelay {
    piece_id: u8,
    remaining_ms: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AdvanceTwoMotionCue {
    piece_id: u8,
    event_progress: PieceProgress,
    pause_ms: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct PieceMotionEffect {
    pub(crate) start_delay_secs: f32,
    pub(crate) advance_two: Option<AdvanceTwoPause>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AdvanceTwoPause {
    pub(crate) event_progress: PieceProgress,
    pub(crate) pause_secs: f32,
}

#[derive(Resource, Default)]
pub struct EffectRevealDelays {
    shield_delays: Vec<ShieldRevealDelay>,
    landing_shield_gains: Vec<LandingShieldGain>,
    skill_charge_delays: Vec<SkillChargeRevealDelay>,
}

impl EffectRevealDelays {
    pub fn visible_shield(&self, piece_id: u8, actual_shield: u8) -> u8 {
        let pending_gain = self
            .landing_shield_gains
            .iter()
            .any(|gain| gain.piece_id == piece_id) as i8;
        let delta = self
            .shield_delays
            .iter()
            .filter(|delay| delay.piece_id == piece_id)
            .map(|delay| delay.visible_delta)
            .sum::<i8>()
            - pending_gain;
        if delta >= 0 {
            actual_shield.saturating_add(delta as u8)
        } else {
            actual_shield.saturating_sub(delta.unsigned_abs())
        }
    }

    pub fn visible_skill_charge(&self, action: SkillUiAction, actual_charge: u8) -> u8 {
        let hidden = self
            .skill_charge_delays
            .iter()
            .filter(|delay| delay.action == action)
            .map(|delay| delay.hidden_delta)
            .sum::<u8>();
        actual_charge.saturating_sub(hidden)
    }

    fn delay_shield_gain(&mut self, piece_id: u8, duration: f32) {
        self.shield_delays.push(ShieldRevealDelay {
            piece_id,
            visible_delta: -1,
            remaining_secs: duration,
        });
    }

    pub fn delay_shield_loss(&mut self, piece_id: u8, duration: f32) {
        self.shield_delays.push(ShieldRevealDelay {
            piece_id,
            visible_delta: 1,
            remaining_secs: duration,
        });
    }

    fn delay_skill_charge_gain(&mut self, action: SkillUiAction, duration: f32) {
        self.skill_charge_delays.push(SkillChargeRevealDelay {
            action,
            hidden_delta: 1,
            remaining_ms: seconds_to_millis(duration),
        });
    }

    fn tick(&mut self, delta_secs: f32) {
        let elapsed_ms = seconds_to_millis(delta_secs);
        for delay in &mut self.shield_delays {
            // 不逐帧取整毫秒，避免高刷新率下护盾先于浮字结束就显示。
            delay.remaining_secs = (delay.remaining_secs - delta_secs).max(0.0);
        }
        for delay in &mut self.skill_charge_delays {
            delay.remaining_ms = delay.remaining_ms.saturating_sub(elapsed_ms);
        }
        self.shield_delays
            .retain(|delay| delay.remaining_secs > 0.0);
        self.skill_charge_delays
            .retain(|delay| delay.remaining_ms > 0);
    }

    fn clear(&mut self) {
        self.shield_delays.clear();
        self.landing_shield_gains.clear();
        self.skill_charge_delays.clear();
    }
}

#[derive(Resource, Default)]
pub struct PieceMotionEffects {
    start_delays: Vec<PieceStartDelay>,
    advance_two_cues: Vec<AdvanceTwoMotionCue>,
}

impl PieceMotionEffects {
    pub fn delay_piece_motion(&mut self, piece_id: u8, duration: f32) {
        let remaining_ms = seconds_to_millis(duration);
        if let Some(delay) = self
            .start_delays
            .iter_mut()
            .find(|delay| delay.piece_id == piece_id)
        {
            delay.remaining_ms = delay.remaining_ms.max(remaining_ms);
        } else {
            self.start_delays.push(PieceStartDelay {
                piece_id,
                remaining_ms,
            });
        }
    }

    fn cue_advance_two(&mut self, piece_id: u8, event_progress: PieceProgress, duration: f32) {
        self.advance_two_cues.retain(|cue| cue.piece_id != piece_id);
        self.advance_two_cues.push(AdvanceTwoMotionCue {
            piece_id,
            event_progress,
            pause_ms: seconds_to_millis(duration),
        });
    }

    pub(crate) fn take_for_piece(&mut self, piece_id: u8) -> PieceMotionEffect {
        let effect = self.peek_for_piece(piece_id);
        self.start_delays.retain(|delay| delay.piece_id != piece_id);
        self.advance_two_cues.retain(|cue| cue.piece_id != piece_id);
        effect
    }

    pub(crate) fn peek_for_piece(&self, piece_id: u8) -> PieceMotionEffect {
        let start_delay_secs = self
            .start_delays
            .iter()
            .find(|delay| delay.piece_id == piece_id)
            .map(|delay| delay.remaining_ms)
            .map(millis_to_seconds)
            .unwrap_or_default();
        let advance_two = self
            .advance_two_cues
            .iter()
            .find(|cue| cue.piece_id == piece_id)
            .map(|cue| AdvanceTwoPause {
                event_progress: cue.event_progress,
                pause_secs: millis_to_seconds(cue.pause_ms),
            });
        PieceMotionEffect {
            start_delay_secs,
            advance_two,
        }
    }

    fn clear(&mut self) {
        self.start_delays.clear();
        self.advance_two_cues.clear();
    }
}

#[derive(Resource, Default)]
pub struct VisualEffectQueue {
    requests: Vec<VisualEffectRequest>,
}

impl VisualEffectQueue {
    pub fn hud_skill_missile(&mut self, action: SkillUiAction, target_world: Vec2) {
        self.requests.push(VisualEffectRequest::HudSkillMissile {
            action,
            target_world,
        });
    }

    pub fn world_missile(&mut self, source_world: Vec2, target_world: Vec2) {
        self.requests.push(VisualEffectRequest::WorldMissile {
            source_world,
            target_world,
        });
    }

    pub fn shield_flash(&mut self, target_world: Vec2) {
        self.requests
            .push(VisualEffectRequest::ShieldFlash { target_world });
    }

    pub fn floating_text(&mut self, target_world: Vec2, text: impl Into<String>) {
        self.requests.push(VisualEffectRequest::FloatingText {
            target_world,
            text: text.into(),
        });
    }

    pub fn hud_skill_charge(&mut self, action: SkillUiAction) {
        self.requests.push(VisualEffectRequest::HudSkillEffect {
            action,
            locked: false,
        });
    }

    pub fn hud_skill_lock(&mut self, action: SkillUiAction) {
        self.requests.push(VisualEffectRequest::HudSkillEffect {
            action,
            locked: true,
        });
    }

    pub fn pending_count(&self) -> usize {
        self.requests.len()
    }

    fn drain(&mut self) -> Vec<VisualEffectRequest> {
        self.requests.drain(..).collect()
    }
}

#[derive(Clone, Debug)]
enum VisualEffectRequest {
    HudSkillMissile {
        action: SkillUiAction,
        target_world: Vec2,
    },
    WorldMissile {
        source_world: Vec2,
        target_world: Vec2,
    },
    ShieldFlash {
        target_world: Vec2,
    },
    ShieldGain {
        target_world: Vec2,
    },
    FloatingText {
        target_world: Vec2,
        text: String,
    },
    HudSkillEffect {
        action: SkillUiAction,
        locked: bool,
    },
}

#[derive(Component)]
struct VisualEffectEntity;

#[derive(Component)]
struct LockEffect {
    age: f32,
    duration: f32,
}

#[derive(Component)]
struct MissileEffect {
    age: f32,
    delay: f32,
    duration: f32,
    from: Vec2,
    to: Vec2,
}

#[derive(Component)]
struct ShieldFlashEffect {
    age: f32,
    duration: f32,
}

#[derive(Component)]
struct FloatingTextEffect {
    age: f32,
    duration: f32,
    start: Vec2,
}

#[derive(Component)]
struct HudSkillEffect {
    age: f32,
    duration: f32,
    locked: bool,
}

fn enqueue_turn_event_effects(
    turn_state: Res<TurnState>,
    mut effect_state: ResMut<TurnEventEffectState>,
    mut queue: ResMut<VisualEffectQueue>,
    mut reveal_delays: ResMut<EffectRevealDelays>,
    mut motion_effects: ResMut<PieceMotionEffects>,
    piece_query: Query<(&PieceId, &Transform)>,
    shield_piece_query: Query<(Entity, &PieceId, &PieceState)>,
) {
    if effect_state.last_action_serial == turn_state.last_action_serial {
        return;
    }
    effect_state.last_action_serial = turn_state.last_action_serial;

    // 使用结构化目标，不依赖日志字符串；同色跳跃补结算的事件也走这里。
    if let Some(notice) = turn_state.last_event_notice
        && matches!(notice.detail, EventNoticeDetail::GainShield { .. })
    {
        if let Some((entity, piece_id, piece_state)) = shield_piece_query
            .iter()
            .find(|(_, id, _)| Some(id.0) == notice.target_piece_id)
            && piece_state.status == PieceStatus::Active
            && piece_state.shield > 0
        {
            reveal_delays
                .landing_shield_gains
                .retain(|gain| gain.entity != entity);
            reveal_delays.landing_shield_gains.push(LandingShieldGain {
                entity,
                piece_id: piece_id.0,
                motion_serial: piece_state.motion_serial,
            });
        }
        return;
    }

    let Some(action) = turn_state.last_action.as_deref() else {
        return;
    };
    let Some(event_note) = extract_event_note(action) else {
        return;
    };

    let moving_piece_id = parse_first_piece_id(action);
    if event_note == "event advance +2" {
        if let Some(piece_id) = moving_piece_id
            && let Some(event_progress) = parse_event_trigger_progress(action, event_note)
        {
            motion_effects.cue_advance_two(piece_id, event_progress, FLOATING_TEXT_DURATION);
        }
        return;
    }

    if event_note.starts_with("event GainSkillCharge:") {
        if let Some(action) = charged_skill_from_event_note(event_note) {
            reveal_delays.delay_skill_charge_gain(action, HUD_SKILL_FX_DURATION);
            queue.hud_skill_charge(action);
        }
        return;
    }

    if event_note.starts_with("event DisableNextSkill:") {
        for action in HUD_SKILL_FX_ACTIONS {
            queue.hud_skill_lock(action);
        }
        return;
    }

    if let Some(target_piece_id) =
        event_note.strip_prefix("event RemoveEnemyShield: removed shield from piece #")
    {
        let Some(target_piece_id) = parse_leading_u8(target_piece_id) else {
            return;
        };
        let Some(source_world) =
            moving_piece_id.and_then(|piece_id| piece_position(piece_id, &piece_query))
        else {
            return;
        };
        let Some(target_world) = piece_position(target_piece_id, &piece_query) else {
            return;
        };
        reveal_delays.delay_shield_loss(target_piece_id, TARGETED_MISSILE_REVEAL_DURATION);
        queue.world_missile(source_world, target_world);
    }
}

fn play_landed_shield_gain_effects(
    mut reveal_delays: ResMut<EffectRevealDelays>,
    mut queue: ResMut<VisualEffectQueue>,
    piece_query: Query<(&PieceState, &Transform, Has<PieceMoveAnimation>)>,
) {
    // 必须在移动捕获、插值以及延迟组件删除全部完成后检查，不按骰点估算时间。
    let pending = std::mem::take(&mut reveal_delays.landing_shield_gains);
    for gain in pending {
        let Ok((piece, transform, moving)) = piece_query.get(gain.entity) else {
            continue;
        };
        if piece.status != PieceStatus::Active
            || piece.shield == 0
            || piece.motion_serial != gain.motion_serial
        {
            continue;
        }
        if moving {
            reveal_delays.landing_shield_gains.push(gain);
            continue;
        }

        reveal_delays.delay_shield_gain(gain.piece_id, SHIELD_GAIN_REVEAL_DURATION);
        queue.requests.push(VisualEffectRequest::ShieldGain {
            target_world: transform.translation.truncate(),
        });
    }
}

fn drain_visual_effect_queue(
    mut commands: Commands,
    windows: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform)>,
    device_profile: Res<DeviceProfile>,
    language_settings: Res<LanguageSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut queue: ResMut<VisualEffectQueue>,
) {
    let requests = queue.drain();
    if requests.is_empty() {
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };
    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };

    for request in requests {
        match request {
            VisualEffectRequest::HudSkillMissile {
                action,
                target_world,
            } => {
                let source_world = skill_button_center_world(
                    action,
                    window,
                    *device_profile,
                    camera,
                    camera_transform,
                )
                .unwrap_or(target_world + Vec2::new(-90.0, -90.0));
                spawn_lock_effect(&mut commands, target_world, language_settings.language);
                spawn_missile_effect(&mut commands, source_world, target_world);
            }
            VisualEffectRequest::WorldMissile {
                source_world,
                target_world,
            } => {
                spawn_lock_effect(&mut commands, target_world, language_settings.language);
                spawn_missile_effect(&mut commands, source_world, target_world);
            }
            VisualEffectRequest::ShieldFlash { target_world } => {
                spawn_shield_flash_effect(&mut commands, &mut meshes, &mut materials, target_world);
            }
            VisualEffectRequest::ShieldGain { target_world } => {
                spawn_shield_flash_effect(&mut commands, &mut meshes, &mut materials, target_world);
                spawn_floating_text_effect(
                    &mut commands,
                    target_world + SHIELD_GAIN_TEXT_OFFSET,
                    shield_gain_label(language_settings.language).into(),
                );
            }
            VisualEffectRequest::FloatingText { target_world, text } => {
                spawn_floating_text_effect(&mut commands, target_world, text);
            }
            VisualEffectRequest::HudSkillEffect { action, locked } => {
                spawn_hud_skill_effect(
                    &mut commands,
                    window,
                    *device_profile,
                    action,
                    locked,
                    language_settings.language,
                );
            }
        }
    }
}

fn shield_gain_label(language: Language) -> &'static str {
    match language {
        Language::SimplifiedChinese => "护盾 +1",
        Language::English => "Shield +1",
    }
}

fn extract_event_note(action: &str) -> Option<&str> {
    let mut note = None;
    for segment in action.split(';').flat_map(|part| part.split(", ")) {
        let segment = segment.trim();
        if let Some(pre_jump) = segment.strip_prefix("pre-jump event tile ") {
            if let Some((_, event_note)) = pre_jump.split_once(": ")
                && event_note.starts_with("event ")
            {
                note = Some(event_note);
            }
        } else if segment.starts_with("event ") {
            note = Some(segment);
        } else if let Some(index) = segment.find(": event ") {
            let event_note = &segment[index + 2..];
            if event_note.starts_with("event ") {
                note = Some(event_note);
            }
        }
    }
    note
}

fn parse_first_piece_id(action: &str) -> Option<u8> {
    action.split("piece #").nth(1).and_then(parse_leading_u8)
}

fn parse_leading_u8(text: &str) -> Option<u8> {
    let digits = text
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    digits.parse::<u8>().ok()
}

fn charged_skill_from_event_note(event_note: &str) -> Option<SkillUiAction> {
    let skill = event_note
        .strip_prefix("event GainSkillCharge: gained 1 ")
        .and_then(|detail| detail.strip_suffix(" charge"))?;
    match skill {
        "Dash" => Some(SkillUiAction::Dash),
        "Snipe" => Some(SkillUiAction::Snipe),
        "Swap" => Some(SkillUiAction::Swap),
        "Shield" => Some(SkillUiAction::Shield),
        "DoubleDice" => Some(SkillUiAction::DoubleDice),
        _ => None,
    }
}

fn piece_position(piece_id: u8, piece_query: &Query<(&PieceId, &Transform)>) -> Option<Vec2> {
    piece_query
        .iter()
        .find(|(query_piece_id, _)| query_piece_id.0 == piece_id)
        .map(|(_, transform)| transform.translation.truncate())
}

fn skill_button_center_world(
    action: SkillUiAction,
    window: &Window,
    device_profile: DeviceProfile,
    camera: &Camera,
    camera_transform: &GlobalTransform,
) -> Option<Vec2> {
    let rect = shared_skill_button_rect(window.width(), window.height(), device_profile, action);
    let center = Vec2::new(rect.x + rect.w * 0.5, rect.y + rect.h * 0.5);
    camera.viewport_to_world_2d(camera_transform, center).ok()
}

fn spawn_lock_effect(commands: &mut Commands, target_world: Vec2, language: Language) {
    commands
        .spawn((
            Sprite::from_color(Color::srgba(0.98, 0.08, 0.08, 0.24), Vec2::splat(46.0)),
            Transform::from_xyz(target_world.x, target_world.y, EFFECT_Z),
            LockEffect {
                age: 0.0,
                duration: LOCK_DURATION,
            },
            VisualEffectEntity,
            Name::new("TargetLockEffect"),
        ))
        .with_children(|lock| {
            lock.spawn((
                Text2d::new(effect_lock_label(language)),
                TextFont {
                    font_size: FontSize::Px(11.0),
                    ..default()
                },
                TextColor(Color::srgba(1.0, 0.96, 0.92, 0.96)),
                TextLayout::justify(Justify::Center),
                Transform::from_xyz(0.0, 0.0, 0.1),
                Name::new("TargetLockLabel"),
            ));
        });
}

fn spawn_missile_effect(commands: &mut Commands, from: Vec2, to: Vec2) {
    let rotation = rotation_for_direction(to - from);
    commands.spawn((
        Sprite::from_color(Color::srgba(1.0, 0.58, 0.12, 0.94), Vec2::new(28.0, 7.0)),
        Transform {
            translation: Vec3::new(from.x, from.y, EFFECT_Z + 1.0),
            rotation,
            ..default()
        },
        Visibility::Hidden,
        MissileEffect {
            age: 0.0,
            delay: MISSILE_DELAY,
            duration: MISSILE_DURATION,
            from,
            to,
        },
        VisualEffectEntity,
        Name::new("MissileEffect"),
    ));
}

fn spawn_shield_flash_effect(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    target_world: Vec2,
) {
    commands
        .spawn((
            Transform::from_xyz(target_world.x, target_world.y + 24.0, EFFECT_Z + 2.0),
            ShieldFlashEffect {
                age: 0.0,
                duration: SHIELD_FLASH_DURATION,
            },
            VisualEffectEntity,
            Name::new("ShieldFlashEffect"),
        ))
        .with_children(|shield| {
            shield.spawn((
                Mesh2d(meshes.add(shield_mesh(16.0, 20.0))),
                MeshMaterial2d(
                    materials.add(ColorMaterial::from(Color::srgba(0.06, 0.18, 0.32, 0.84))),
                ),
                Transform::from_xyz(0.0, 0.0, 0.0),
                Name::new("ShieldFlashBorder"),
            ));
            shield.spawn((
                Mesh2d(meshes.add(shield_mesh(12.0, 16.0))),
                MeshMaterial2d(
                    materials.add(ColorMaterial::from(Color::srgba(0.72, 0.90, 1.0, 0.95))),
                ),
                Transform::from_xyz(0.0, 0.0, 0.1),
                Name::new("ShieldFlashIcon"),
            ));
        });
}

fn shield_mesh(width: f32, height: f32) -> Mesh {
    let half_width = width * 0.5;
    let top = height * 0.42;
    let lower_y = -height * 0.20;
    let bottom = -height * 0.50;
    let points = [
        Vec2::new(-half_width, top),
        Vec2::new(half_width, top),
        Vec2::new(half_width * 0.82, lower_y),
        Vec2::new(0.0, bottom),
        Vec2::new(-half_width * 0.82, lower_y),
    ];
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    let mut positions = vec![[0.0, 0.0, 0.0]];
    positions.extend(points.iter().map(|point| [point.x, point.y, 0.0]));

    let mut indices = Vec::new();
    for index in 1..=points.len() {
        let next = if index == points.len() { 1 } else { index + 1 };
        indices.extend_from_slice(&[0, index as u32, next as u32]);
    }

    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn spawn_floating_text_effect(commands: &mut Commands, target_world: Vec2, text: String) {
    commands.spawn((
        Text2d::new(text),
        TextFont {
            font_size: FontSize::Px(22.0),
            ..default()
        },
        TextColor(Color::srgba(0.08, 0.16, 0.24, 0.96)),
        TextLayout::justify(Justify::Center),
        Transform::from_xyz(target_world.x, target_world.y + 26.0, EFFECT_Z + 2.0),
        FloatingTextEffect {
            age: 0.0,
            duration: FLOATING_TEXT_DURATION,
            start: target_world + Vec2::new(0.0, 26.0),
        },
        VisualEffectEntity,
        Name::new("FloatingTextEffect"),
    ));
}

fn spawn_hud_skill_effect(
    commands: &mut Commands,
    window: &Window,
    device_profile: DeviceProfile,
    action: SkillUiAction,
    locked: bool,
    language: Language,
) {
    let rect = shared_skill_button_rect(window.width(), window.height(), device_profile, action);
    let label = hud_skill_effect_label(locked, language);
    let color = if locked {
        Color::srgba(0.12, 0.14, 0.18, 0.46)
    } else {
        Color::srgba(0.34, 0.70, 0.96, 0.42)
    };
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(rect.x),
                top: Val::Px(rect.y),
                width: Val::Px(rect.w),
                height: Val::Px(rect.h),
                border: UiRect::all(Val::Px(2.0)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                padding: UiRect::horizontal(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(color),
            BorderColor::all(if locked {
                Color::srgba(0.02, 0.04, 0.07, 0.74)
            } else {
                Color::srgba(0.04, 0.22, 0.40, 0.78)
            }),
            ZIndex(58),
            HudSkillEffect {
                age: 0.0,
                duration: HUD_SKILL_FX_DURATION,
                locked,
            },
            VisualEffectEntity,
            Name::new(if locked {
                "HudSkillLockEffect"
            } else {
                "HudSkillChargeEffect"
            }),
        ))
        .with_children(|effect| {
            effect.spawn((
                Text::new(label),
                TextFont {
                    font_size: FontSize::Px(if locked { 12.0 } else { 9.5 }),
                    ..default()
                },
                TextColor(Color::srgba(1.0, 1.0, 1.0, 0.94)),
                TextLayout::justify(Justify::Center),
                Name::new("HudSkillEffectLabel"),
            ));
        });
}

fn effect_lock_label(language: Language) -> &'static str {
    match language {
        Language::SimplifiedChinese => "锁定",
        Language::English => "LOCK",
    }
}

fn hud_skill_effect_label(locked: bool, language: Language) -> &'static str {
    match (locked, language) {
        (true, Language::SimplifiedChinese) => "禁用",
        (false, Language::SimplifiedChinese) => "充能",
        (true, Language::English) => "LOCK",
        (false, Language::English) => "CHARGE",
    }
}

fn animate_lock_effects(
    time: Res<Time>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut Sprite, &mut Transform, &mut LockEffect)>,
) {
    for (entity, mut sprite, mut transform, mut effect) in &mut query {
        effect.age += time.delta_secs();
        let t = (effect.age / effect.duration).clamp(0.0, 1.0);
        let pulse = 1.0 + (1.0 - t) * 0.20 + (t * std::f32::consts::PI * 4.0).sin().abs() * 0.08;
        transform.scale = Vec3::splat(pulse);
        sprite.color = Color::srgba(0.98, 0.08, 0.08, 0.30 * (1.0 - t));

        if effect.age >= effect.duration {
            commands.entity(entity).despawn();
        }
    }
}

fn animate_missile_effects(
    time: Res<Time>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut Visibility, &mut Transform, &mut MissileEffect)>,
) {
    for (entity, mut visibility, mut transform, mut effect) in &mut query {
        effect.age += time.delta_secs();
        if effect.age < effect.delay {
            *visibility = Visibility::Hidden;
            continue;
        }

        *visibility = Visibility::Visible;
        let t = ((effect.age - effect.delay) / effect.duration).clamp(0.0, 1.0);
        let pos = effect.from.lerp(effect.to, ease_out_cubic(t));
        transform.translation.x = pos.x;
        transform.translation.y = pos.y;
        transform.rotation = rotation_for_direction(effect.to - effect.from);

        if effect.age >= effect.delay + effect.duration {
            commands.entity(entity).despawn();
        }
    }
}

fn animate_shield_flash_effects(
    time: Res<Time>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut Transform, &mut ShieldFlashEffect)>,
) {
    for (entity, mut transform, mut effect) in &mut query {
        effect.age += time.delta_secs();
        let t = (effect.age / effect.duration).clamp(0.0, 1.0);
        let pulse = 1.0 + (1.0 - t) * 0.20 + (t * std::f32::consts::PI * 3.0).sin().abs() * 0.18;
        transform.scale = Vec3::splat(pulse);

        if effect.age >= effect.duration {
            commands.entity(entity).despawn();
        }
    }
}

fn animate_floating_text_effects(
    time: Res<Time>,
    mut commands: Commands,
    mut query: Query<(
        Entity,
        &mut TextColor,
        &mut Transform,
        &mut FloatingTextEffect,
    )>,
) {
    for (entity, mut text_color, mut transform, mut effect) in &mut query {
        effect.age += time.delta_secs();
        let t = (effect.age / effect.duration).clamp(0.0, 1.0);
        transform.translation.x = effect.start.x;
        transform.translation.y = effect.start.y + t * 28.0;
        transform.scale = Vec3::splat(1.0 + (1.0 - t) * 0.18);
        text_color.0 = Color::srgba(0.08, 0.16, 0.24, 0.96 * (1.0 - t));

        if effect.age >= effect.duration {
            commands.entity(entity).despawn();
        }
    }
}

fn animate_hud_skill_effects(
    time: Res<Time>,
    mut commands: Commands,
    mut query: Query<(
        Entity,
        &mut BackgroundColor,
        &mut BorderColor,
        &mut HudSkillEffect,
    )>,
) {
    for (entity, mut background, mut border, mut effect) in &mut query {
        effect.age += time.delta_secs();
        let t = (effect.age / effect.duration).clamp(0.0, 1.0);
        let alpha = (1.0 - t) * (0.42 + (t * std::f32::consts::PI * 4.0).sin().abs() * 0.20);
        *background = BackgroundColor(if effect.locked {
            Color::srgba(0.12, 0.14, 0.18, alpha)
        } else {
            Color::srgba(0.34, 0.70, 0.96, alpha)
        });
        *border = BorderColor::all(if effect.locked {
            Color::srgba(0.02, 0.04, 0.07, alpha + 0.18)
        } else {
            Color::srgba(0.04, 0.22, 0.40, alpha + 0.18)
        });

        if effect.age >= effect.duration {
            commands.entity(entity).despawn();
        }
    }
}

fn tick_effect_reveal_delays(time: Res<Time>, mut reveal_delays: ResMut<EffectRevealDelays>) {
    reveal_delays.tick(time.delta_secs());
}

fn cleanup_visual_effects(
    mut commands: Commands,
    query: Query<Entity, With<VisualEffectEntity>>,
    mut queue: ResMut<VisualEffectQueue>,
    mut reveal_delays: ResMut<EffectRevealDelays>,
    mut motion_effects: ResMut<PieceMotionEffects>,
    mut event_state: ResMut<TurnEventEffectState>,
) {
    queue.requests.clear();
    reveal_delays.clear();
    motion_effects.clear();
    *event_state = TurnEventEffectState::default();
    for entity in &query {
        commands.entity(entity).despawn();
    }
}

fn rotation_for_direction(direction: Vec2) -> Quat {
    if direction.length_squared() < 0.001 {
        return Quat::IDENTITY;
    }
    Quat::from_rotation_z(direction.y.atan2(direction.x))
}

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

fn seconds_to_millis(seconds: f32) -> u16 {
    (seconds.max(0.0) * 1000.0).round().min(u16::MAX as f32) as u16
}

fn millis_to_seconds(milliseconds: u16) -> f32 {
    f32::from(milliseconds) / 1000.0
}

fn parse_event_trigger_progress(action: &str, event_note: &str) -> Option<PieceProgress> {
    let event_index = action.find(event_note)?;
    let prefix = &action[..event_index];
    if prefix.contains("pre-jump event tile ") {
        return parse_moved_piece_progress(action);
    }

    parse_last_tile_progress(prefix).or_else(|| parse_moved_piece_progress(action))
}

fn parse_moved_piece_progress(action: &str) -> Option<PieceProgress> {
    action
        .split("moved piece #")
        .nth(1)
        .and_then(|detail| detail.split(" to tile ").nth(1))
        .and_then(parse_leading_progress)
}

fn parse_last_tile_progress(text: &str) -> Option<PieceProgress> {
    text.match_indices("tile ")
        .last()
        .and_then(|(index, _)| parse_leading_progress(&text[index + "tile ".len()..]))
}

fn parse_leading_progress(text: &str) -> Option<PieceProgress> {
    let end = text
        .char_indices()
        .find(|&(i, c)| !(c.is_ascii_digit() || (i == 0 && c == '-')))
        .map_or(text.len(), |(i, _)| i);
    text[..end].parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{game_mode::GameMode, rule_set::RuleSet};
    use crate::domain::{event::TileEventKind, player::PlayerControl, rules::LaunchRule};
    use crate::gameplay::ai::AiDifficulty;
    use crate::gameplay::match_flow::{MatchConfig, MatchSetup, PlayerSeat, build_match_resources};
    use crate::gameplay::turn_flow::{EventNotice, FINISH_DISTANCE, world_position_for_piece};
    use crate::plugins::animation_plugin::AnimationPlugin;
    use bevy::state::app::StatesPlugin;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    #[derive(Resource)]
    struct TestShieldLanding(Option<(PieceProgress, Vec3)>);

    fn settle_test_shield_gain(
        mut landing: ResMut<TestShieldLanding>,
        mut turn: ResMut<TurnState>,
        mut pieces: Query<(&mut PieceState, &mut Transform), With<PieceId>>,
    ) {
        let Some((progress, target)) = landing.0.take() else {
            return;
        };
        let (mut piece, mut transform) = pieces.single_mut().unwrap();
        piece.progress = progress;
        piece.motion_serial += 1;
        piece.shield += 1;
        transform.translation = target;
        turn.last_action_serial += 1;
        // 故意不提供英文日志，确认加盾反馈只依赖结构化事件。
        turn.last_action = None;
        turn.last_event_notice = Some(EventNotice {
            actor_player_id: 1,
            target_player_id: Some(1),
            target_piece_id: Some(1),
            kind: TileEventKind::GainShield,
            detail: EventNoticeDetail::GainShield {
                current_shield: piece.shield,
            },
        });
    }

    fn shield_landing_app(
        fast_mode: bool,
        same_color_jump: bool,
        initial_shield: u8,
    ) -> (App, Entity, Vec3) {
        let setup = MatchSetup {
            mode: GameMode::TwoVsTwo,
            rule_set: RuleSet::Creative,
            ai_difficulty: AiDifficulty::Normal,
            fast_mode,
            launch_rule: LaunchRule::SixOnly,
            player_seats: PlayerSeat::ALL,
            pieces_per_player: 2,
            player_controls: [PlayerControl::Human; 4],
        };
        let config = MatchConfig {
            mode: setup.mode,
            rule_set: setup.rule_set,
            ai_difficulty: setup.ai_difficulty,
            fast_mode,
            launch_rule: setup.launch_rule,
            player_seats: setup.player_seats,
            pieces_per_player: setup.pieces_per_player,
            player_controls: setup.player_controls,
        };
        let (board, roster, _) = build_match_resources(&setup);
        let progress_at = |tile| {
            let position = board.world_pos_for_route_index(tile).unwrap();
            (0..FINISH_DISTANCE)
                .find(|&progress| {
                    world_position_for_piece(1, progress, PieceStatus::Active, &board, &roster)
                        == Some(position)
                })
                .unwrap()
        };
        let event_tile = if same_color_jump { 0 } else { 12 };
        let destination_tile = if same_color_jump { 3 } else { event_tile };
        assert_eq!(
            board.tile_kind_for_route_index(event_tile),
            Some(crate::domain::tile::TileKind::Event)
        );
        let start_progress = progress_at(event_tile) - 2;
        let final_progress = progress_at(destination_tile);
        let start =
            world_position_for_piece(1, start_progress, PieceStatus::Active, &board, &roster)
                .unwrap()
                .extend(1.0);
        let target = board
            .world_pos_for_route_index(destination_tile)
            .unwrap()
            .extend(1.0);
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_state(AppState::InGame)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                20,
            )))
            .insert_resource(config)
            .insert_resource(board)
            .insert_resource(roster)
            .init_resource::<TurnState>()
            .init_resource::<DeviceProfile>()
            .init_resource::<LanguageSettings>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<ColorMaterial>>()
            .insert_resource(TestShieldLanding(None))
            .add_plugins((EffectsPlugin, AnimationPlugin))
            .add_systems(Update, settle_test_shield_gain.in_set(TurnSystems));
        app.world_mut().spawn(Window::default());
        app.world_mut()
            .spawn((Camera::default(), GlobalTransform::default()));
        let entity = app
            .world_mut()
            .spawn((
                PieceId(1),
                PieceState {
                    owner_player_id: 1,
                    team_id: 1,
                    status: PieceStatus::Active,
                    progress: start_progress,
                    shield: initial_shield,
                    stack_shield: 0,
                    motion_serial: 0,
                },
                Transform::from_translation(start),
            ))
            .id();
        app.update();
        app.world_mut().resource_mut::<TestShieldLanding>().0 = Some((final_progress, target));
        (app, entity, target)
    }

    fn effect_count<T: Component>(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<Entity, With<T>>()
            .iter(app.world())
            .count()
    }

    fn verify_shield_landing(fast_mode: bool, same_color_jump: bool, initial_shield: u8) {
        let (mut app, piece, target) =
            shield_landing_app(fast_mode, same_color_jump, initial_shield);
        // 也覆盖技能/撞击导致的额外等待，不能靠固定移动时长猜测落地。
        app.world_mut()
            .resource_mut::<PieceMotionEffects>()
            .delay_piece_motion(1, 0.6);
        let mut arrived = false;
        for _ in 0..200 {
            app.update();
            let moving = app.world().get::<PieceMoveAnimation>(piece).is_some();
            let shield_fx = effect_count::<ShieldFlashEffect>(&mut app);
            let text_fx = effect_count::<FloatingTextEffect>(&mut app);
            assert_eq!(
                app.world()
                    .resource::<EffectRevealDelays>()
                    .visible_shield(1, initial_shield + 1),
                initial_shield
            );
            if moving {
                assert_eq!(shield_fx, 0, "must not flash during movement/jump");
                assert_eq!(text_fx, 0);
            } else {
                assert_eq!(
                    app.world().get::<Transform>(piece).unwrap().translation,
                    target
                );
                assert_eq!(shield_fx, 1);
                assert_eq!(text_fx, 1);
                let position = app
                    .world_mut()
                    .query_filtered::<&Transform, With<ShieldFlashEffect>>()
                    .single(app.world())
                    .unwrap()
                    .translation;
                assert_eq!(
                    position.truncate(),
                    target.truncate() + Vec2::new(0.0, 24.0)
                );
                let text = app
                    .world_mut()
                    .query_filtered::<&Text2d, With<FloatingTextEffect>>()
                    .single(app.world())
                    .unwrap();
                assert_eq!(text.0, "护盾 +1");
                arrived = true;
                break;
            }
        }
        assert!(arrived);
        assert!(
            app.world()
                .resource::<EffectRevealDelays>()
                .landing_shield_gains
                .is_empty()
        );
        for _ in 0..45 {
            app.update();
            if effect_count::<FloatingTextEffect>(&mut app) > 0 {
                assert_eq!(
                    app.world()
                        .resource::<EffectRevealDelays>()
                        .visible_shield(1, initial_shield + 1),
                    initial_shield
                );
            }
        }
        assert_eq!(effect_count::<ShieldFlashEffect>(&mut app), 0);
        assert_eq!(effect_count::<FloatingTextEffect>(&mut app), 0);
        assert_eq!(
            app.world()
                .resource::<EffectRevealDelays>()
                .visible_shield(1, initial_shield + 1),
            initial_shield + 1
        );
    }

    #[test]
    fn event_shield_gain_waits_for_normal_arrival_before_flash_text_and_badge() {
        for fast_mode in [false, true] {
            verify_shield_landing(fast_mode, false, 0);
        }
    }

    #[test]
    fn event_shield_gain_waits_for_same_color_jump_and_preserves_existing_shield() {
        for fast_mode in [false, true] {
            verify_shield_landing(fast_mode, true, 1);
        }
    }

    #[test]
    fn event_shield_gain_without_motion_plays_once_and_localizes_text() {
        let (mut app, piece, _) = shield_landing_app(false, false, 0);
        let progress = app.world().get::<PieceState>(piece).unwrap().progress;
        let position = app.world().get::<Transform>(piece).unwrap().translation;
        app.world_mut().resource_mut::<TestShieldLanding>().0 = Some((progress, position));
        app.world_mut().resource_mut::<LanguageSettings>().language = Language::English;
        app.update();
        assert_eq!(effect_count::<ShieldFlashEffect>(&mut app), 1);
        let text = app
            .world_mut()
            .query_filtered::<&Text2d, With<FloatingTextEffect>>()
            .single(app.world())
            .unwrap();
        assert_eq!(text.0, "Shield +1");
        for _ in 0..50 {
            app.update();
        }
        assert_eq!(effect_count::<ShieldFlashEffect>(&mut app), 0);
        assert_eq!(effect_count::<FloatingTextEffect>(&mut app), 0);
        assert_eq!(
            app.world()
                .resource::<EffectRevealDelays>()
                .visible_shield(1, 1),
            1
        );
    }

    #[test]
    fn event_shield_gain_discards_missing_returned_or_superseded_piece() {
        for invalidation in 0..3 {
            let (mut app, piece, _) = shield_landing_app(false, true, 0);
            app.update();
            assert_eq!(
                app.world()
                    .resource::<EffectRevealDelays>()
                    .landing_shield_gains
                    .len(),
                1
            );
            match invalidation {
                0 => {
                    app.world_mut().despawn(piece);
                }
                1 => {
                    app.world_mut().get_mut::<PieceState>(piece).unwrap().status =
                        PieceStatus::InHangar;
                }
                _ => {
                    app.world_mut()
                        .get_mut::<PieceState>(piece)
                        .unwrap()
                        .motion_serial += 1;
                }
            }
            app.update();
            assert!(
                app.world()
                    .resource::<EffectRevealDelays>()
                    .landing_shield_gains
                    .is_empty()
            );
            assert_eq!(effect_count::<ShieldFlashEffect>(&mut app), 0);
            assert_eq!(
                app.world()
                    .resource::<EffectRevealDelays>()
                    .visible_shield(1, 1),
                1
            );
        }
    }

    #[test]
    fn event_shield_gain_cleanup_clears_pending_feedback_and_serial() {
        let (mut app, _, _) = shield_landing_app(false, true, 0);
        app.update();
        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::MainMenu);
        app.update();
        assert!(
            app.world()
                .resource::<EffectRevealDelays>()
                .landing_shield_gains
                .is_empty()
        );
        assert_eq!(
            app.world()
                .resource::<TurnEventEffectState>()
                .last_action_serial,
            0
        );
        assert_eq!(effect_count::<ShieldFlashEffect>(&mut app), 0);
        assert_eq!(
            app.world()
                .resource::<EffectRevealDelays>()
                .visible_shield(1, 1),
            1
        );
    }

    #[test]
    fn shield_gain_floating_text_clears_actual_shield_mesh_throughout_pulse() {
        use bevy::mesh::VertexAttributeValues;
        let (mut app, piece, _) = shield_landing_app(false, false, 0);
        let progress = app.world().get::<PieceState>(piece).unwrap().progress;
        let position = app.world().get::<Transform>(piece).unwrap().translation;
        app.world_mut().resource_mut::<TestShieldLanding>().0 = Some((progress, position));
        for _ in 0..27 {
            app.update();
            let (root, shield) = app
                .world_mut()
                .query_filtered::<(Entity, &Transform), With<ShieldFlashEffect>>()
                .single(app.world())
                .unwrap();
            let shield = *shield;
            let mut top = f32::NEG_INFINITY;
            let mut mesh_query = app.world_mut().query::<(&Mesh2d, &Transform, &ChildOf)>();
            let meshes = app.world().resource::<Assets<Mesh>>();
            for (mesh, transform, parent) in mesh_query.iter(app.world()) {
                if parent.parent() != root {
                    continue;
                }
                let Some(VertexAttributeValues::Float32x3(vertices)) = meshes
                    .get(&mesh.0)
                    .unwrap()
                    .attribute(Mesh::ATTRIBUTE_POSITION)
                else {
                    panic!("shield vertices");
                };
                for vertex in vertices {
                    top = top.max(
                        shield
                            .transform_point(transform.transform_point(Vec3::from_array(*vertex)))
                            .y,
                    );
                }
            }
            let (text, font) = app
                .world_mut()
                .query_filtered::<(&Transform, &TextFont), With<FloatingTextEffect>>()
                .single(app.world())
                .unwrap();
            let FontSize::Px(font_size) = font.font_size else {
                panic!("fixed world-space font size");
            };
            // 以 1.5em 的保守可见行框校验，涵盖中英文及缩放脉冲。
            let text_bottom = text.translation.y - font_size * 1.5 * text.scale.y * 0.5;
            assert!(
                text_bottom > top + 2.0,
                "text {text_bottom} overlaps shield {top}"
            );
        }
    }

    #[test]
    fn visual_effect_queue_preserves_request_order() {
        let mut queue = VisualEffectQueue::default();

        queue.floating_text(Vec2::new(1.0, 2.0), "+2");
        queue.shield_flash(Vec2::new(3.0, 4.0));

        let requests = queue.drain();
        assert_eq!(requests.len(), 2);
        assert!(matches!(
            requests[0],
            VisualEffectRequest::FloatingText { .. }
        ));
        assert!(matches!(
            requests[1],
            VisualEffectRequest::ShieldFlash { .. }
        ));
    }

    #[test]
    fn missile_rotation_points_toward_target() {
        let rotation = rotation_for_direction(Vec2::Y);
        let (_, _, angle) = rotation.to_euler(EulerRot::XYZ);
        assert!((angle - std::f32::consts::FRAC_PI_2).abs() < 0.001);
    }

    #[test]
    fn event_effect_parser_finds_latest_event_note() {
        assert_eq!(
            extract_event_note("rolled 3, moved piece #1 to tile 5; event advance +2"),
            Some("event advance +2")
        );
        assert_eq!(
            extract_event_note(
                "rolled 2, moved piece #1 to tile 3; jumped to next same-color tile 6, pre-jump event tile 0: event GainSkillCharge: gained 1 Dash charge"
            ),
            Some("event GainSkillCharge: gained 1 Dash charge")
        );
        assert_eq!(
            extract_event_note("rolled 1, moved piece #1 to tile 2"),
            None
        );
    }

    #[test]
    fn piece_id_parser_reads_action_piece_id() {
        assert_eq!(
            parse_first_piece_id("rolled 4, moved piece #12 to tile 9"),
            Some(12)
        );
        assert_eq!(parse_first_piece_id("rolled 6"), None);
    }

    #[test]
    fn event_trigger_progress_parser_uses_last_tile_before_event() {
        assert_eq!(
            parse_moved_piece_progress("rolled 1, moved piece #1 to tile -1"),
            Some(-1)
        );
        assert_eq!(
            parse_first_piece_id("rolled 1, moved piece #1 to tile -1"),
            Some(1)
        );
        assert_eq!(
            parse_event_trigger_progress(
                "rolled 2, moved piece #1 to tile 3; jumped to next same-color tile 6, event advance +2",
                "event advance +2",
            ),
            Some(6)
        );
        assert_eq!(
            parse_event_trigger_progress(
                "rolled 2, moved piece #1 to tile 3; jumped to next same-color tile 6, pre-jump event tile 0: event advance +2",
                "event advance +2",
            ),
            Some(3)
        );
    }

    #[test]
    fn charged_skill_parser_maps_event_note_to_ui_action() {
        assert_eq!(
            charged_skill_from_event_note("event GainSkillCharge: gained 1 Dash charge"),
            Some(SkillUiAction::Dash)
        );
        assert_eq!(
            charged_skill_from_event_note("event GainSkillCharge: gained 1 DoubleDice charge"),
            Some(SkillUiAction::DoubleDice)
        );
        assert_eq!(
            charged_skill_from_event_note("event GainSkillCharge: gained 1 Unknown charge"),
            None
        );
    }

    #[test]
    fn reveal_delays_hide_new_badge_values_until_timer_expires() {
        let mut delays = EffectRevealDelays::default();

        delays.delay_shield_gain(3, 0.5);
        delays.delay_shield_loss(4, 0.5);
        delays.delay_skill_charge_gain(SkillUiAction::Snipe, 0.5);

        assert_eq!(delays.visible_shield(3, 1), 0);
        assert_eq!(delays.visible_shield(4, 0), 1);
        assert_eq!(delays.visible_skill_charge(SkillUiAction::Snipe, 1), 0);

        delays.tick(0.2);
        assert_eq!(delays.visible_shield(3, 1), 0);
        assert_eq!(delays.visible_shield(4, 0), 1);
        assert_eq!(delays.visible_skill_charge(SkillUiAction::Snipe, 1), 0);

        delays.tick(0.3);
        assert_eq!(delays.visible_shield(3, 1), 1);
        assert_eq!(delays.visible_shield(4, 0), 0);
        assert_eq!(delays.visible_skill_charge(SkillUiAction::Snipe, 1), 1);
    }

    #[test]
    fn shield_gain_reveal_does_not_round_away_high_refresh_frames() {
        for fps in [50, 120, 144, 240, 400] {
            let mut delays = EffectRevealDelays::default();
            delays.delay_shield_gain(1, SHIELD_GAIN_REVEAL_DURATION);
            let delta = 1.0 / fps as f32;
            for _ in 0..(FLOATING_TEXT_DURATION * fps as f32).ceil() as usize {
                delays.tick(delta);
                assert_eq!(delays.visible_shield(1, 1), 0, "fps={fps}");
            }
            delays.tick(0.1);
            assert_eq!(delays.visible_shield(1, 1), 1);
        }
    }

    #[test]
    fn piece_motion_effects_return_start_delay_and_advance_two_cue_once() {
        let mut effects = PieceMotionEffects::default();

        effects.delay_piece_motion(7, 0.25);
        effects.delay_piece_motion(7, 0.5);
        effects.cue_advance_two(7, 12, 0.75);

        assert_eq!(effects.peek_for_piece(7).start_delay_secs, 0.5);

        let effect = effects.take_for_piece(7);
        assert_eq!(effect.start_delay_secs, 0.5);
        assert_eq!(
            effect.advance_two,
            Some(AdvanceTwoPause {
                event_progress: 12,
                pause_secs: 0.75,
            })
        );
        assert_eq!(effects.take_for_piece(7), PieceMotionEffect::default());
    }
}
