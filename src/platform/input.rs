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
    just_pressed_source: Option<PointerSource>,
    just_released_position: Option<Vec2>,
    just_released_source: Option<PointerSource>,
    current_position: Option<Vec2>,
    current_source: Option<PointerSource>,
    active_touch_id: Option<u64>,
    pressed: bool,
}

impl PointerInputState {
    pub fn just_pressed(self) -> bool {
        self.just_pressed_position.is_some()
    }

    pub fn just_pressed_position(self) -> Option<Vec2> {
        self.just_pressed_position
    }

    pub fn source(self) -> Option<PointerSource> {
        self.just_pressed_source
    }

    pub fn just_released(self) -> bool {
        self.just_released_position.is_some()
    }

    pub fn just_released_position(self) -> Option<Vec2> {
        self.just_released_position
    }

    pub fn just_released_source(self) -> Option<PointerSource> {
        self.just_released_source
    }

    pub fn is_pressed(self) -> bool {
        self.pressed
    }

    pub fn current_position(self) -> Option<Vec2> {
        self.current_position
    }

    pub fn current_source(self) -> Option<PointerSource> {
        self.current_source
    }

    fn reset(&mut self) {
        self.just_pressed_position = None;
        self.just_pressed_source = None;
        self.just_released_position = None;
        self.just_released_source = None;
    }

    fn record_press(&mut self, position: Vec2, source: PointerSource) {
        if self.just_pressed_position.is_none() {
            self.just_pressed_position = Some(position);
            self.just_pressed_source = Some(source);
        }
        self.current_position = Some(position);
        self.current_source = Some(source);
        self.pressed = true;
    }

    fn record_move(&mut self, position: Vec2, source: PointerSource) {
        self.current_position = Some(position);
        self.current_source = Some(source);
        self.pressed = true;
    }

    fn record_release(&mut self, position: Vec2, source: PointerSource) {
        if self.just_released_position.is_none() {
            self.just_released_position = Some(position);
            self.just_released_source = Some(source);
        }
        self.current_position = Some(position);
        self.current_source = Some(source);
        self.pressed = false;
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
        match touch_input.phase {
            TouchPhase::Started if pointer_state.active_touch_id.is_none() => {
                pointer_state.active_touch_id = Some(touch_input.id);
                pointer_state.record_press(touch_input.position, PointerSource::Touch);
            }
            TouchPhase::Moved if pointer_state.active_touch_id == Some(touch_input.id) => {
                pointer_state.record_move(touch_input.position, PointerSource::Touch);
            }
            TouchPhase::Ended | TouchPhase::Canceled
                if pointer_state.active_touch_id == Some(touch_input.id) =>
            {
                pointer_state.record_release(touch_input.position, PointerSource::Touch);
                pointer_state.active_touch_id = None;
            }
            _ => {}
        }
    }

    if pointer_state.current_source == Some(PointerSource::Touch) && pointer_state.is_pressed() {
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor_position) = window.cursor_position() else {
        return;
    };

    if mouse.just_pressed(MouseButton::Left) {
        pointer_state.record_press(cursor_position, PointerSource::Mouse);
    } else if mouse.just_released(MouseButton::Left) {
        pointer_state.record_release(cursor_position, PointerSource::Mouse);
    } else if mouse.pressed(MouseButton::Left) {
        pointer_state.record_move(cursor_position, PointerSource::Mouse);
    } else if pointer_state.current_source != Some(PointerSource::Touch) {
        pointer_state.current_position = Some(cursor_position);
        pointer_state.current_source = Some(PointerSource::Mouse);
        pointer_state.pressed = false;
    }
}
