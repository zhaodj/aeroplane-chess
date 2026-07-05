use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::{AppLifecycle, WindowFocused};
#[cfg(not(target_arch = "wasm32"))]
use bevy::{
    audio::{AudioSinkPlayback, Volume},
    prelude::AudioSource,
};
#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
use web_sys::HtmlAudioElement;

use crate::gameplay::match_flow::MatchResult;
use crate::gameplay::skill_flow::SkillRoster;
use crate::gameplay::turn_flow::TurnState;
use crate::platform::PointerInputState;
use crate::states::AppState;

/// 音频插件入口：播放背景音乐，并用短音效反馈掷骰、移动、碰撞、护盾和胜利。
pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AudioSettings>()
            .init_resource::<AudioFeedbackState>()
            .init_resource::<AudioInteractionState>()
            .init_resource::<AudioFocusState>()
            .add_systems(
                Update,
                (
                    capture_audio_interaction,
                    sync_background_music_focus,
                    ensure_background_music,
                    sync_background_music_volume,
                    play_audio_feedback.run_if(in_state(AppState::InGame)),
                )
                    .chain(),
            );

        #[cfg(not(target_arch = "wasm32"))]
        app.add_systems(Startup, load_audio_assets);

        #[cfg(target_arch = "wasm32")]
        app.init_resource::<GameAudioAssets>()
            .init_resource::<WebBackgroundMusicState>();
    }
}

#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct AudioSettings {
    pub music_volume: f32,
    pub effects_volume: f32,
    pub muted: bool,
}

impl AudioSettings {
    pub const MUSIC_STEP: f32 = 0.05;
    pub const EFFECTS_STEP: f32 = 0.05;

    pub fn adjust_music(&mut self, delta: f32) {
        self.music_volume = clamp_audio_volume(self.music_volume + delta);
    }

    pub fn adjust_effects(&mut self, delta: f32) {
        self.effects_volume = clamp_audio_volume(self.effects_volume + delta);
    }

    pub fn toggle_mute(&mut self) {
        self.muted = !self.muted;
    }

    pub fn effective_music_volume(self) -> f32 {
        effective_audio_volume(self.music_volume, self.muted)
    }

    pub fn effective_effects_volume(self) -> f32 {
        effective_audio_volume(self.effects_volume, self.muted)
    }
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            music_volume: 0.35,
            effects_volume: 0.60,
            muted: false,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Resource, Clone)]
struct GameAudioAssets {
    bgm: Handle<AudioSource>,
    dice: Handle<AudioSource>,
    movement: Handle<AudioSource>,
    launch: Handle<AudioSource>,
    collision: Handle<AudioSource>,
    shield: Handle<AudioSource>,
    event: Handle<AudioSource>,
    victory: Handle<AudioSource>,
}

#[cfg(target_arch = "wasm32")]
#[derive(Resource, Clone, Copy)]
struct GameAudioAssets {
    bgm: &'static str,
    dice: &'static str,
    movement: &'static str,
    launch: &'static str,
    collision: &'static str,
    shield: &'static str,
    event: &'static str,
    victory: &'static str,
}

#[cfg(target_arch = "wasm32")]
impl Default for GameAudioAssets {
    fn default() -> Self {
        Self {
            bgm: "assets/audio/bgm.ogg",
            dice: "assets/audio/dice.ogg",
            movement: "assets/audio/move.ogg",
            launch: "assets/audio/launch.ogg",
            collision: "assets/audio/collision.ogg",
            shield: "assets/audio/shield.ogg",
            event: "assets/audio/event.ogg",
            victory: "assets/audio/victory.ogg",
        }
    }
}

#[derive(Resource, Default)]
struct AudioFeedbackState {
    last_roll_serial: u32,
    last_action: Option<String>,
    last_skill_action: Option<String>,
    victory_played: bool,
}

#[derive(Resource, Default)]
struct AudioInteractionState {
    unlocked: bool,
}

#[derive(Resource)]
struct AudioFocusState {
    foreground: bool,
    paused_for_background: bool,
}

impl Default for AudioFocusState {
    fn default() -> Self {
        Self {
            foreground: true,
            paused_for_background: false,
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Resource, Default)]
struct WebBackgroundMusicState {
    started: bool,
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    static WEB_BACKGROUND_MUSIC: RefCell<Option<HtmlAudioElement>> = const { RefCell::new(None) };
    static WEB_SOUND_EFFECTS: RefCell<Vec<HtmlAudioElement>> = const { RefCell::new(Vec::new()) };
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component)]
struct BackgroundMusic;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SoundEffect {
    Dice,
    Move,
    Launch,
    Collision,
    Shield,
    Event,
    Victory,
}

pub fn clamp_audio_volume(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

fn effective_audio_volume(volume: f32, muted: bool) -> f32 {
    if muted {
        0.0
    } else {
        clamp_audio_volume(volume)
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn load_audio_assets(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(GameAudioAssets {
        bgm: asset_server.load("audio/bgm.ogg"),
        dice: asset_server.load("audio/dice.ogg"),
        movement: asset_server.load("audio/move.ogg"),
        launch: asset_server.load("audio/launch.ogg"),
        collision: asset_server.load("audio/collision.ogg"),
        shield: asset_server.load("audio/shield.ogg"),
        event: asset_server.load("audio/event.ogg"),
        victory: asset_server.load("audio/victory.ogg"),
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn ensure_background_music(
    mut commands: Commands,
    audio_assets: Res<GameAudioAssets>,
    audio_settings: Res<AudioSettings>,
    audio_interaction: Res<AudioInteractionState>,
    audio_focus: Res<AudioFocusState>,
    music_query: Query<Entity, With<BackgroundMusic>>,
) {
    if !should_spawn_background_music(
        audio_interaction.unlocked,
        audio_focus.foreground,
        audio_settings.effective_music_volume(),
        !music_query.is_empty(),
    ) {
        return;
    }

    commands.spawn((
        AudioPlayer::new(audio_assets.bgm.clone()),
        PlaybackSettings::LOOP.with_volume(Volume::Linear(audio_settings.effective_music_volume())),
        BackgroundMusic,
        Name::new("BackgroundMusic"),
    ));
}

#[cfg(target_arch = "wasm32")]
fn ensure_background_music(
    audio_assets: Res<GameAudioAssets>,
    audio_settings: Res<AudioSettings>,
    audio_interaction: Res<AudioInteractionState>,
    audio_focus: Res<AudioFocusState>,
    mut music_state: ResMut<WebBackgroundMusicState>,
) {
    if !should_spawn_background_music(
        audio_interaction.unlocked,
        audio_focus.foreground,
        audio_settings.effective_music_volume(),
        music_state.started,
    ) {
        return;
    }

    music_state.started =
        play_web_background_music(audio_assets.bgm, audio_settings.effective_music_volume());
}

fn capture_audio_interaction(
    mut audio_interaction: ResMut<AudioInteractionState>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    pointer: Res<PointerInputState>,
) {
    if audio_interaction.unlocked {
        return;
    }

    if has_audio_unlock_input(&keyboard, &mouse, &pointer) {
        audio_interaction.unlocked = true;
    }
}

fn has_audio_unlock_input(
    keyboard: &ButtonInput<KeyCode>,
    mouse: &ButtonInput<MouseButton>,
    pointer: &PointerInputState,
) -> bool {
    keyboard.get_just_pressed().next().is_some()
        || pointer.just_pressed()
        || mouse.just_pressed(MouseButton::Left)
        || mouse.just_pressed(MouseButton::Right)
        || mouse.just_pressed(MouseButton::Middle)
}

fn should_spawn_background_music(
    audio_unlocked: bool,
    app_foreground: bool,
    music_volume: f32,
    music_exists: bool,
) -> bool {
    audio_unlocked && app_foreground && music_volume > f32::EPSILON && !music_exists
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BackgroundMusicFocusCommand {
    None,
    Pause,
    Resume,
}

fn background_music_focus_command(
    focused: bool,
    music_is_paused: bool,
    audio_focus: &mut AudioFocusState,
) -> BackgroundMusicFocusCommand {
    audio_focus.foreground = focused;
    if !focused {
        audio_focus.paused_for_background = !music_is_paused;
        return if music_is_paused {
            BackgroundMusicFocusCommand::None
        } else {
            BackgroundMusicFocusCommand::Pause
        };
    }

    if audio_focus.paused_for_background {
        audio_focus.paused_for_background = false;
        return if music_is_paused {
            BackgroundMusicFocusCommand::Resume
        } else {
            BackgroundMusicFocusCommand::None
        };
    }
    BackgroundMusicFocusCommand::None
}

fn app_lifecycle_foreground(lifecycle: AppLifecycle) -> Option<bool> {
    match lifecycle {
        AppLifecycle::WillSuspend | AppLifecycle::Suspended => Some(false),
        AppLifecycle::WillResume | AppLifecycle::Running => Some(true),
        AppLifecycle::Idle => None,
    }
}

fn sync_focus_without_music(focused: bool, audio_focus: &mut AudioFocusState) {
    audio_focus.foreground = focused;
    if !focused {
        audio_focus.paused_for_background = false;
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn sync_background_music_focus(
    mut focus_messages: MessageReader<WindowFocused>,
    mut lifecycle_messages: MessageReader<AppLifecycle>,
    mut audio_focus: ResMut<AudioFocusState>,
    music_query: Query<&AudioSink, With<BackgroundMusic>>,
) {
    for focus in focus_messages.read() {
        sync_native_background_music_foreground(focus.focused, &mut audio_focus, &music_query);
    }
    for lifecycle in lifecycle_messages.read() {
        if let Some(foreground) = app_lifecycle_foreground(*lifecycle) {
            sync_native_background_music_foreground(foreground, &mut audio_focus, &music_query);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn sync_native_background_music_foreground(
    foreground: bool,
    audio_focus: &mut AudioFocusState,
    music_query: &Query<&AudioSink, With<BackgroundMusic>>,
) {
    let Some(sink) = music_query.iter().next() else {
        sync_focus_without_music(foreground, audio_focus);
        return;
    };

    match background_music_focus_command(foreground, sink.is_paused(), audio_focus) {
        BackgroundMusicFocusCommand::None => {}
        BackgroundMusicFocusCommand::Pause => sink.pause(),
        BackgroundMusicFocusCommand::Resume => sink.play(),
    }
}

#[cfg(target_arch = "wasm32")]
fn sync_background_music_focus(
    mut focus_messages: MessageReader<WindowFocused>,
    mut lifecycle_messages: MessageReader<AppLifecycle>,
    mut audio_focus: ResMut<AudioFocusState>,
) {
    for focus in focus_messages.read() {
        sync_web_background_music_foreground(focus.focused, &mut audio_focus);
    }
    for lifecycle in lifecycle_messages.read() {
        if let Some(foreground) = app_lifecycle_foreground(*lifecycle) {
            sync_web_background_music_foreground(foreground, &mut audio_focus);
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn sync_web_background_music_foreground(foreground: bool, audio_focus: &mut AudioFocusState) {
    WEB_BACKGROUND_MUSIC.with(|music| {
        let Some(audio) = music.borrow().as_ref().cloned() else {
            sync_focus_without_music(foreground, audio_focus);
            return;
        };
        match background_music_focus_command(foreground, audio.paused(), audio_focus) {
            BackgroundMusicFocusCommand::None => {}
            BackgroundMusicFocusCommand::Pause => {
                let _ = audio.pause();
            }
            BackgroundMusicFocusCommand::Resume => {
                let _ = audio.play();
            }
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn sync_background_music_volume(
    audio_settings: Res<AudioSettings>,
    mut music_query: Query<&mut AudioSink, With<BackgroundMusic>>,
) {
    if !audio_settings.is_changed() {
        return;
    }

    for mut sink in &mut music_query {
        sink.set_volume(Volume::Linear(audio_settings.effective_music_volume()));
    }
}

#[cfg(target_arch = "wasm32")]
fn sync_background_music_volume(audio_settings: Res<AudioSettings>) {
    if !audio_settings.is_changed() {
        return;
    }

    WEB_BACKGROUND_MUSIC.with(|music| {
        if let Some(audio) = music.borrow().as_ref() {
            audio.set_volume(audio_settings.effective_music_volume() as f64);
        }
    });
}

#[derive(SystemParam)]
struct AudioFeedbackParams<'w> {
    audio_assets: Res<'w, GameAudioAssets>,
    audio_settings: Res<'w, AudioSettings>,
    audio_interaction: Res<'w, AudioInteractionState>,
    feedback_state: ResMut<'w, AudioFeedbackState>,
    turn_state: Res<'w, TurnState>,
    skill_roster: Res<'w, SkillRoster>,
    match_result: Res<'w, MatchResult>,
}

fn play_audio_feedback(mut commands: Commands, mut params: AudioFeedbackParams) {
    if !params.audio_interaction.unlocked {
        return;
    }

    if !params.match_result.finished {
        params.feedback_state.victory_played = false;
    } else if !params.feedback_state.victory_played {
        spawn_sound_effect(
            &mut commands,
            &params.audio_assets,
            &params.audio_settings,
            SoundEffect::Victory,
        );
        params.feedback_state.victory_played = true;
        return;
    }

    if should_play_dice_roll_sound(
        &mut params.feedback_state.last_roll_serial,
        params.turn_state.roll_serial,
    ) {
        spawn_sound_effect(
            &mut commands,
            &params.audio_assets,
            &params.audio_settings,
            SoundEffect::Dice,
        );
        return;
    }

    if params.turn_state.last_action != params.feedback_state.last_action {
        params
            .feedback_state
            .last_action
            .clone_from(&params.turn_state.last_action);
        if let Some(note) = params.turn_state.last_action.as_deref()
            && let Some(effect) = classify_feedback_note(note)
        {
            spawn_sound_effect(
                &mut commands,
                &params.audio_assets,
                &params.audio_settings,
                effect,
            );
            return;
        }
    }

    if params.skill_roster.last_skill_action != params.feedback_state.last_skill_action {
        params
            .feedback_state
            .last_skill_action
            .clone_from(&params.skill_roster.last_skill_action);
        if let Some(note) = params.skill_roster.last_skill_action.as_deref()
            && let Some(effect) = classify_feedback_note(note)
        {
            spawn_sound_effect(
                &mut commands,
                &params.audio_assets,
                &params.audio_settings,
                effect,
            );
        }
    }
}

fn should_play_dice_roll_sound(last_roll_serial: &mut u32, current_roll_serial: u32) -> bool {
    if current_roll_serial == 0 {
        *last_roll_serial = 0;
        return false;
    }
    if *last_roll_serial == current_roll_serial {
        return false;
    }

    *last_roll_serial = current_roll_serial;
    true
}

fn classify_feedback_note(note: &str) -> Option<SoundEffect> {
    let note = note.to_ascii_lowercase();
    if note.contains("victory") || note.contains("wins") {
        Some(SoundEffect::Victory)
    } else if note.contains("event") {
        Some(SoundEffect::Event)
    } else if note.contains("shield")
        || note.contains("blocked")
        || note.contains("stacked with teammate")
    {
        Some(SoundEffect::Shield)
    } else if note.contains("collision")
        || note.contains("back to hangar")
        || note.contains("snipe sent")
        || note.contains("removed shield")
        || note.contains("bounced back")
    {
        Some(SoundEffect::Collision)
    } else if note.contains("launched") {
        Some(SoundEffect::Launch)
    } else if note.contains("moved")
        || note.contains("dash")
        || note.contains("swap")
        || note.contains("shortcut")
        || note.contains("jump")
    {
        Some(SoundEffect::Move)
    } else if note.contains("double") || note.contains("rolled") {
        Some(SoundEffect::Dice)
    } else {
        None
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn spawn_sound_effect(
    commands: &mut Commands,
    audio_assets: &GameAudioAssets,
    audio_settings: &AudioSettings,
    effect: SoundEffect,
) {
    let volume = audio_settings.effective_effects_volume();
    if volume <= f32::EPSILON {
        return;
    }

    commands.spawn((
        AudioPlayer::new(sound_effect_handle(audio_assets, effect).clone()),
        PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume)),
        Name::new(format!("{effect:?}SoundEffect")),
    ));
}

#[cfg(target_arch = "wasm32")]
fn spawn_sound_effect(
    _commands: &mut Commands,
    audio_assets: &GameAudioAssets,
    audio_settings: &AudioSettings,
    effect: SoundEffect,
) {
    let volume = audio_settings.effective_effects_volume();
    if volume <= f32::EPSILON {
        return;
    }

    play_web_sound_effect(sound_effect_handle(audio_assets, effect), volume);
}

#[cfg(not(target_arch = "wasm32"))]
fn sound_effect_handle(
    audio_assets: &GameAudioAssets,
    effect: SoundEffect,
) -> &Handle<AudioSource> {
    match effect {
        SoundEffect::Dice => &audio_assets.dice,
        SoundEffect::Move => &audio_assets.movement,
        SoundEffect::Launch => &audio_assets.launch,
        SoundEffect::Collision => &audio_assets.collision,
        SoundEffect::Shield => &audio_assets.shield,
        SoundEffect::Event => &audio_assets.event,
        SoundEffect::Victory => &audio_assets.victory,
    }
}

#[cfg(target_arch = "wasm32")]
fn sound_effect_handle(audio_assets: &GameAudioAssets, effect: SoundEffect) -> &'static str {
    match effect {
        SoundEffect::Dice => audio_assets.dice,
        SoundEffect::Move => audio_assets.movement,
        SoundEffect::Launch => audio_assets.launch,
        SoundEffect::Collision => audio_assets.collision,
        SoundEffect::Shield => audio_assets.shield,
        SoundEffect::Event => audio_assets.event,
        SoundEffect::Victory => audio_assets.victory,
    }
}

#[cfg(target_arch = "wasm32")]
fn play_web_background_music(url: &str, volume: f32) -> bool {
    let Ok(audio) = HtmlAudioElement::new_with_src(url) else {
        return false;
    };
    audio.set_loop(true);
    audio.set_volume(volume as f64);
    let _ = audio.play();
    WEB_BACKGROUND_MUSIC.with(|music| {
        *music.borrow_mut() = Some(audio);
    });
    true
}

#[cfg(target_arch = "wasm32")]
fn play_web_sound_effect(url: &str, volume: f32) {
    let Ok(audio) = HtmlAudioElement::new_with_src(url) else {
        return;
    };
    audio.set_volume(volume as f64);
    let _ = audio.play();
    WEB_SOUND_EFFECTS.with(|effects| {
        let mut effects = effects.borrow_mut();
        effects.push(audio);
        if effects.len() > 16 {
            effects.remove(0);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feedback_notes_map_to_distinct_sound_effects() {
        assert_eq!(
            classify_feedback_note("rolled 6, launched piece #1"),
            Some(SoundEffect::Launch)
        );
        assert_eq!(
            classify_feedback_note("rolled 4, moved piece #1 to tile 12"),
            Some(SoundEffect::Move)
        );
        assert_eq!(
            classify_feedback_note("event GainShield: gained shield (1)"),
            Some(SoundEffect::Event)
        );
        assert_eq!(
            classify_feedback_note("piece #3 blocked collision with shield"),
            Some(SoundEffect::Shield)
        );
        assert_eq!(
            classify_feedback_note("collision sent piece #2 back to hangar"),
            Some(SoundEffect::Collision)
        );
        assert_eq!(
            classify_feedback_note("P1 rolled 3 but had no legal action"),
            Some(SoundEffect::Dice)
        );
    }

    #[test]
    fn dice_sound_plays_once_per_roll_serial_and_resets_between_matches() {
        let mut last_roll_serial = 0;

        assert!(!should_play_dice_roll_sound(&mut last_roll_serial, 0));
        assert!(should_play_dice_roll_sound(&mut last_roll_serial, 1));
        assert!(!should_play_dice_roll_sound(&mut last_roll_serial, 1));
        assert!(should_play_dice_roll_sound(&mut last_roll_serial, 2));
        assert!(!should_play_dice_roll_sound(&mut last_roll_serial, 0));
        assert_eq!(last_roll_serial, 0);
        assert!(should_play_dice_roll_sound(&mut last_roll_serial, 1));
    }

    #[test]
    fn audio_volume_adjustments_are_clamped() {
        let mut settings = AudioSettings {
            music_volume: 0.95,
            effects_volume: 0.05,
            muted: false,
        };

        settings.adjust_music(AudioSettings::MUSIC_STEP);
        settings.adjust_effects(-AudioSettings::EFFECTS_STEP);

        assert_eq!(settings.music_volume, 1.0);
        assert_eq!(settings.effects_volume, 0.0);
    }

    #[test]
    fn mute_zeroes_effective_volume_without_changing_saved_levels() {
        let mut settings = AudioSettings {
            music_volume: 0.35,
            effects_volume: 0.60,
            muted: false,
        };

        assert_eq!(settings.effective_music_volume(), 0.35);
        assert_eq!(settings.effective_effects_volume(), 0.60);

        settings.toggle_mute();
        assert!(settings.muted);
        assert_eq!(settings.music_volume, 0.35);
        assert_eq!(settings.effects_volume, 0.60);
        assert_eq!(settings.effective_music_volume(), 0.0);
        assert_eq!(settings.effective_effects_volume(), 0.0);

        settings.toggle_mute();
        assert_eq!(settings.effective_music_volume(), 0.35);
        assert_eq!(settings.effective_effects_volume(), 0.60);
    }

    #[test]
    fn background_music_waits_for_audio_unlock() {
        assert!(!should_spawn_background_music(false, true, 0.5, false));
        assert!(!should_spawn_background_music(true, false, 0.5, false));
        assert!(!should_spawn_background_music(true, true, 0.0, false));
        assert!(!should_spawn_background_music(true, true, 0.5, true));
        assert!(should_spawn_background_music(true, true, 0.5, false));
    }

    #[test]
    fn background_music_pauses_and_resumes_with_focus() {
        let mut focus = AudioFocusState::default();

        assert_eq!(
            background_music_focus_command(false, false, &mut focus),
            BackgroundMusicFocusCommand::Pause
        );
        assert!(!focus.foreground);
        assert!(focus.paused_for_background);

        assert_eq!(
            background_music_focus_command(true, true, &mut focus),
            BackgroundMusicFocusCommand::Resume
        );
        assert!(focus.foreground);
        assert!(!focus.paused_for_background);
    }

    #[test]
    fn app_lifecycle_maps_suspend_to_background_and_resume_to_foreground() {
        assert_eq!(
            app_lifecycle_foreground(AppLifecycle::WillSuspend),
            Some(false)
        );
        assert_eq!(
            app_lifecycle_foreground(AppLifecycle::Suspended),
            Some(false)
        );
        assert_eq!(
            app_lifecycle_foreground(AppLifecycle::WillResume),
            Some(true)
        );
        assert_eq!(app_lifecycle_foreground(AppLifecycle::Running), Some(true));
        assert_eq!(app_lifecycle_foreground(AppLifecycle::Idle), None);
    }

    #[test]
    fn background_music_pauses_and_resumes_with_lifecycle() {
        let mut focus = AudioFocusState::default();

        let suspended = app_lifecycle_foreground(AppLifecycle::WillSuspend).unwrap();
        assert_eq!(
            background_music_focus_command(suspended, false, &mut focus),
            BackgroundMusicFocusCommand::Pause
        );
        assert!(!focus.foreground);
        assert!(focus.paused_for_background);

        let resumed = app_lifecycle_foreground(AppLifecycle::Running).unwrap();
        assert_eq!(
            background_music_focus_command(resumed, true, &mut focus),
            BackgroundMusicFocusCommand::Resume
        );
        assert!(focus.foreground);
        assert!(!focus.paused_for_background);
    }

    #[test]
    fn background_music_does_not_resume_if_it_was_already_paused() {
        let mut focus = AudioFocusState::default();

        assert_eq!(
            background_music_focus_command(false, true, &mut focus),
            BackgroundMusicFocusCommand::None
        );
        assert!(!focus.foreground);
        assert!(!focus.paused_for_background);

        assert_eq!(
            background_music_focus_command(true, true, &mut focus),
            BackgroundMusicFocusCommand::None
        );
        assert!(focus.foreground);
        assert!(!focus.paused_for_background);
    }
}
