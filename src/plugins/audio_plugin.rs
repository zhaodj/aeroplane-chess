use bevy::audio::Volume;
use bevy::prelude::*;
use std::time::Duration;

use crate::gameplay::match_flow::MatchResult;
use crate::gameplay::skill_flow::SkillRoster;
use crate::gameplay::turn_flow::TurnState;
use crate::states::AppState;

/// 音频插件入口：用短提示音反馈掷骰、移动、碰撞、护盾和胜利。
pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AudioFeedbackState>().add_systems(
            Update,
            play_audio_feedback.run_if(in_state(AppState::InGame)),
        );
    }
}

#[derive(Resource, Default)]
struct AudioFeedbackState {
    last_action: Option<String>,
    last_skill_action: Option<String>,
    victory_played: bool,
}

fn play_audio_feedback(
    mut commands: Commands,
    mut pitch_assets: ResMut<Assets<Pitch>>,
    mut feedback_state: ResMut<AudioFeedbackState>,
    turn_state: Res<TurnState>,
    skill_roster: Res<SkillRoster>,
    match_result: Res<MatchResult>,
) {
    if match_result.finished && !feedback_state.victory_played {
        spawn_feedback_pitch(&mut commands, &mut pitch_assets, 880.0, 0.16);
        feedback_state.victory_played = true;
        return;
    }

    if turn_state.last_action != feedback_state.last_action {
        feedback_state
            .last_action
            .clone_from(&turn_state.last_action);
        if let Some(note) = turn_state.last_action.as_deref()
            && let Some((frequency, duration)) = classify_feedback_note(note)
        {
            spawn_feedback_pitch(&mut commands, &mut pitch_assets, frequency, duration);
            return;
        }
    }

    if skill_roster.last_skill_action != feedback_state.last_skill_action {
        feedback_state
            .last_skill_action
            .clone_from(&skill_roster.last_skill_action);
        if let Some(note) = skill_roster.last_skill_action.as_deref()
            && let Some((frequency, duration)) = classify_feedback_note(note)
        {
            spawn_feedback_pitch(&mut commands, &mut pitch_assets, frequency, duration);
        }
    }
}

fn classify_feedback_note(note: &str) -> Option<(f32, f32)> {
    let note = note.to_ascii_lowercase();
    if note.contains("victory") || note.contains("wins") {
        Some((880.0, 0.16))
    } else if note.contains("shield") || note.contains("blocked") {
        Some((660.0, 0.10))
    } else if note.contains("collision")
        || note.contains("back to hangar")
        || note.contains("snipe sent")
    {
        Some((220.0, 0.12))
    } else if note.contains("launched") {
        Some((520.0, 0.09))
    } else if note.contains("moved") || note.contains("dash") || note.contains("swap") {
        Some((420.0, 0.08))
    } else if note.contains("double") || note.contains("rolled") {
        Some((330.0, 0.07))
    } else {
        None
    }
}

fn spawn_feedback_pitch(
    commands: &mut Commands,
    pitch_assets: &mut Assets<Pitch>,
    frequency: f32,
    duration_secs: f32,
) {
    commands.spawn((
        AudioPlayer(pitch_assets.add(Pitch::new(
            frequency,
            Duration::from_secs_f32(duration_secs),
        ))),
        PlaybackSettings::DESPAWN.with_volume(Volume::Linear(0.18)),
    ));
}
