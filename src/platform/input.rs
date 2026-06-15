use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::prelude::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerSource {
    Mouse,
    Touch,
}

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct PointerInputState {
    just_pressed_position: Option<Vec2>,
    source: Option<PointerSource>,
}

impl PointerInputState {
    pub fn just_pressed(self) -> bool {
        self.just_pressed_position.is_some()
    }

    pub fn just_pressed_position(self) -> Option<Vec2> {
        self.just_pressed_position
    }

    pub fn source(self) -> Option<PointerSource> {
        self.source
    }

    fn reset(&mut self) {
        self.just_pressed_position = None;
        self.source = None;
    }

    fn record_press(&mut self, position: Vec2, source: PointerSource) {
        if self.just_pressed_position.is_none() {
            self.just_pressed_position = Some(position);
            self.source = Some(source);
        }
    }
}

pub fn update_pointer_input(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    mut touch_inputs: MessageReader<TouchInput>,
    mut pointer_state: ResMut<PointerInputState>,
) {
    pointer_state.reset();

    for touch_input in touch_inputs.read() {
        if matches!(touch_input.phase, TouchPhase::Started) {
            pointer_state.record_press(touch_input.position, PointerSource::Touch);
        }
    }

    if pointer_state.just_pressed() || !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor_position) = window.cursor_position() else {
        return;
    };
    pointer_state.record_press(cursor_position, PointerSource::Mouse);
}
