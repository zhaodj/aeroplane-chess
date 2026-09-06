mod device;
mod input;

use bevy::prelude::*;

pub use device::{DeviceProfile, HudLayoutMode, primary_window};
pub use input::{PointerInputState, PointerSource};

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlatformInputSet;

pub struct PlatformPlugin;

impl Plugin for PlatformPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DeviceProfile>()
            .init_resource::<PointerInputState>()
            .add_systems(
                PreUpdate,
                (device::update_device_profile, input::update_pointer_input)
                    .chain()
                    .in_set(PlatformInputSet),
            );
    }
}
