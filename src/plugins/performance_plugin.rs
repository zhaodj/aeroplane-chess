use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;

use crate::states::AppState;

/// 性能调试插件：提供可开关的 FPS 显示。
pub struct PerformancePlugin;

impl Plugin for PerformancePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PerformanceSettings>()
            .add_plugins(FrameTimeDiagnosticsPlugin::default())
            .add_systems(Startup, spawn_fps_display)
            .add_systems(Update, update_fps_display);
    }
}

#[derive(Clone, Copy, Debug, Resource)]
pub struct PerformanceSettings {
    pub show_fps: bool,
}

impl PerformanceSettings {
    pub fn toggle_fps(&mut self) {
        self.show_fps = !self.show_fps;
    }
}

impl Default for PerformanceSettings {
    fn default() -> Self {
        Self {
            show_fps: cfg!(debug_assertions),
        }
    }
}

#[derive(Component)]
struct FpsDisplay;

const FPS_DISPLAY_LEFT: f32 = 16.0;
const FPS_DISPLAY_TOP: f32 = 60.0;
const FPS_DISPLAY_W: f32 = 128.0;
const FPS_DISPLAY_H: f32 = 30.0;

fn spawn_fps_display(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(FPS_DISPLAY_LEFT),
                top: Val::Px(FPS_DISPLAY_TOP),
                width: Val::Px(FPS_DISPLAY_W),
                height: Val::Px(FPS_DISPLAY_H),
                border: UiRect::all(Val::Px(1.0)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                padding: UiRect::horizontal(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.04, 0.07, 0.10, 0.68)),
            BorderColor::all(Color::srgba(0.88, 0.94, 1.0, 0.24)),
            Visibility::Hidden,
            ZIndex(76),
            FpsDisplay,
            Name::new("FpsDisplay"),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("FPS --"),
                TextFont {
                    font_size: FontSize::Px(14.0),
                    ..default()
                },
                TextColor(Color::srgb(0.94, 0.98, 1.0)),
                TextLayout::justify(Justify::Center),
                FpsDisplay,
                Name::new("FpsDisplayText"),
            ));
        });
}

fn update_fps_display(
    settings: Res<PerformanceSettings>,
    app_state: Res<State<AppState>>,
    diagnostics: Res<DiagnosticsStore>,
    mut container_query: Query<&mut Visibility, (With<FpsDisplay>, Without<Text>)>,
    mut text_query: Query<&mut Text, (With<FpsDisplay>, With<Text>)>,
) {
    let visible = settings.show_fps && !matches!(app_state.get(), AppState::Boot);
    for mut visibility in &mut container_query {
        *visibility = if visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }

    if !visible {
        return;
    }

    let label = fps_display_text(
        diagnostics
            .get(&FrameTimeDiagnosticsPlugin::FPS)
            .and_then(|fps| fps.smoothed()),
    );
    for mut text in &mut text_query {
        *text = Text::new(label.clone());
    }
}

fn fps_display_text(fps: Option<f64>) -> String {
    fps.map_or_else(
        || "FPS --".to_owned(),
        |value| format!("FPS {:>3}", value.round().clamp(0.0, 999.0) as u16),
    )
}

pub fn fps_toggle_label(settings: &PerformanceSettings) -> &'static str {
    if settings.show_fps {
        "FPS On"
    } else {
        "FPS Off"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fps_default_follows_build_profile() {
        assert_eq!(
            PerformanceSettings::default().show_fps,
            cfg!(debug_assertions)
        );
    }

    #[test]
    fn fps_label_tracks_toggle_state() {
        let mut settings = PerformanceSettings { show_fps: false };
        assert_eq!(fps_toggle_label(&settings), "FPS Off");
        settings.toggle_fps();
        assert_eq!(fps_toggle_label(&settings), "FPS On");
    }

    #[test]
    fn fps_display_text_is_stable_width_and_clamped() {
        assert_eq!(fps_display_text(None), "FPS --");
        assert_eq!(fps_display_text(Some(59.6)), "FPS  60");
        assert_eq!(fps_display_text(Some(1500.0)), "FPS 999");
    }
}
