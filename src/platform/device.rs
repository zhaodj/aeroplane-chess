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
        let touch_first = matches!(family, PlatformFamily::Android | PlatformFamily::Ios);
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
        let hud_layout = if width >= 960.0 && height >= 620.0 {
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

    pub fn board_screen_padding(self) -> f32 {
        if self.touch_first { 36.0 } else { 24.0 }
    }

    pub fn hud_reserved_width(self) -> f32 {
        match self.hud_layout {
            HudLayoutMode::SidePanel => 308.0,
            HudLayoutMode::OverlayPanel => 0.0,
        }
    }

    pub fn should_start_hud_collapsed(self) -> bool {
        matches!(self.class, DeviceClass::Phone)
    }

    pub fn piece_pick_radius_world(self) -> f32 {
        if self.touch_first { 36.0 } else { 28.0 }
    }
}

pub fn primary_window() -> Window {
    let window = Window {
        title: "Aeroplane Chess".into(),
        resolution: (WINDOW_WIDTH, WINDOW_HEIGHT).into(),
        resizable: true,
        canvas: canvas_selector(),
        fit_canvas_to_parent: cfg!(target_arch = "wasm32"),
        ..default()
    };

    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let mut window = window;
        window.resizable = false;
        window.mode = WindowMode::BorderlessFullscreen(MonitorSelection::Primary);
        return window;
    }

    window
}

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
