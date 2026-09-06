use bevy::prelude::*;
#[cfg(any(target_os = "android", target_os = "ios"))]
use bevy::window::{MonitorSelection, WindowMode};

use crate::constants::{WINDOW_HEIGHT, WINDOW_WIDTH};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlatformFamily {
    Android,
    Ios,
    Web,
    Desktop,
}

impl PlatformFamily {
    pub fn current() -> Self {
        #[cfg(target_os = "android")]
        {
            Self::Android
        }
        #[cfg(all(not(target_os = "android"), target_os = "ios"))]
        {
            Self::Ios
        }
        #[cfg(all(
            not(target_os = "android"),
            not(target_os = "ios"),
            target_arch = "wasm32"
        ))]
        {
            Self::Web
        }
        #[cfg(all(
            not(target_os = "android"),
            not(target_os = "ios"),
            not(target_arch = "wasm32")
        ))]
        {
            Self::Desktop
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceClass {
    Phone,
    Tablet,
    Desktop,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HudLayoutMode {
    SidePanel,
    OverlayPanel,
}

#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct DeviceProfile {
    pub family: PlatformFamily,
    pub class: DeviceClass,
    pub hud_layout: HudLayoutMode,
    pub window_size: Vec2,
    pub touch_first: bool,
}

impl Default for DeviceProfile {
    fn default() -> Self {
        Self::from_window_size(WINDOW_WIDTH as f32, WINDOW_HEIGHT as f32)
    }
}

impl DeviceProfile {
    pub fn from_window_size(width: f32, height: f32) -> Self {
        let family = PlatformFamily::current();
        let touch_first = matches!(family, PlatformFamily::Android | PlatformFamily::Ios)
            || browser_touch_capable();
        let shortest_side = width.min(height);
        let class = match family {
            PlatformFamily::Android | PlatformFamily::Ios => {
                if shortest_side >= 600.0 {
                    DeviceClass::Tablet
                } else {
                    DeviceClass::Phone
                }
            }
            PlatformFamily::Web => {
                if shortest_side < 600.0 {
                    DeviceClass::Phone
                } else {
                    DeviceClass::Desktop
                }
            }
            PlatformFamily::Desktop => DeviceClass::Desktop,
        };
        let hud_layout = if width > height && width >= 640.0 {
            HudLayoutMode::SidePanel
        } else {
            HudLayoutMode::OverlayPanel
        };

        Self {
            family,
            class,
            hud_layout,
            window_size: Vec2::new(width, height),
            touch_first,
        }
    }

    pub fn should_start_hud_collapsed(self) -> bool {
        matches!(self.class, DeviceClass::Phone)
    }

    pub fn piece_pick_radius_world(self) -> f32 {
        if self.touch_first {
            let board = crate::ui::game_layout::GameLayout::new(
                self.window_size.x,
                self.window_size.y,
                self,
            )
            .board;
            (24.0 * crate::constants::BOARD_WORLD_SIZE / board.w).max(28.0)
        } else {
            28.0
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn browser_touch_capable() -> bool {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.body())
        .is_some_and(|body| body.get_attribute("data-ac-touch").as_deref() == Some("true"))
}

#[cfg(not(target_arch = "wasm32"))]
fn browser_touch_capable() -> bool {
    false
}

pub fn primary_window() -> Window {
    let mut window = Window {
        title: "Aeroplane Chess".into(),
        resolution: (WINDOW_WIDTH, WINDOW_HEIGHT).into(),
        resizable: true,
        canvas: canvas_selector(),
        fit_canvas_to_parent: cfg!(target_arch = "wasm32"),
        ..default()
    };

    configure_primary_window(&mut window);
    window
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn configure_primary_window(window: &mut Window) {
    window.resizable = false;
    window.mode = WindowMode::BorderlessFullscreen(MonitorSelection::Primary);
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn configure_primary_window(_window: &mut Window) {}

#[cfg(target_arch = "wasm32")]
fn canvas_selector() -> Option<String> {
    Some("canvas#bevy".into())
}

#[cfg(not(target_arch = "wasm32"))]
fn canvas_selector() -> Option<String> {
    None
}

pub fn update_device_profile(windows: Query<&Window>, mut device_profile: ResMut<DeviceProfile>) {
    let Ok(window) = windows.single() else {
        return;
    };

    let next_profile = DeviceProfile::from_window_size(window.width(), window.height());
    if *device_profile != next_profile {
        *device_profile = next_profile;
    }
}

#[cfg(test)]
mod tests {
    use super::{DeviceProfile, HudLayoutMode};

    #[test]
    fn android_activity_follows_full_sensor_orientation() {
        let manifest = include_str!("../../platforms/android/app/src/main/AndroidManifest.xml");
        let main_activity = include_str!(
            "../../platforms/android/app/src/main/java/com/zhaodaojun/aeroplanechess/MainActivity.java"
        );

        assert!(manifest.contains(r#"android:screenOrientation="fullSensor""#));
        assert!(!manifest.contains(r#"android:screenOrientation="portrait""#));
        assert!(main_activity.contains("ActivityInfo.SCREEN_ORIENTATION_FULL_SENSOR"));
    }

    #[test]
    fn landscape_uses_side_hud_but_portrait_uses_overlay_hud() {
        assert_eq!(
            DeviceProfile::from_window_size(1280.0, 720.0).hud_layout,
            HudLayoutMode::SidePanel
        );
        assert_eq!(
            DeviceProfile::from_window_size(720.0, 1280.0).hud_layout,
            HudLayoutMode::OverlayPanel
        );
        assert_eq!(
            DeviceProfile::from_window_size(1024.0, 600.0).hud_layout,
            HudLayoutMode::SidePanel
        );
    }

    #[test]
    fn touch_pick_diameter_stays_large_when_portrait_board_shrinks() {
        for (w, h) in [(360., 640.), (720., 1280.), (1280., 720.)] {
            let mut profile = DeviceProfile::from_window_size(w, h);
            profile.touch_first = true;
            let board = crate::ui::game_layout::GameLayout::new(w, h, profile).board;
            assert!(
                profile.piece_pick_radius_world() * 2.0 * board.w
                    / crate::constants::BOARD_WORLD_SIZE
                    >= 48.0 - 0.01
            );
        }
    }
}
