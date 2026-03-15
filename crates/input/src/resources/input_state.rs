use std::collections::HashSet;
use std::time::Instant;

use bevy_ecs::message::{MessageReader, MessageRegistry};
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Res, ResMut};
use bevy_ecs::world::World;
use capy_core::{
    CursorMode, CursorMovedMessage, FrameTime, GameWindow, KeyCode, KeyboardInputMessage,
    MouseButton, MouseButtonMessage, MouseMotionMessage, RawInput, Window,
};

use super::CursorPosition;

#[derive(Resource)]
pub struct InputState {
    keys_held: HashSet<KeyCode>,
    mouse_buttons_held: HashSet<MouseButton>,
    mouse_delta: (f64, f64),
    last_frame: Option<Instant>,
    cursor_grabbed: bool,
    saved_cursor_pos: Option<(f64, f64)>,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            keys_held: HashSet::new(),
            mouse_buttons_held: HashSet::new(),
            mouse_delta: (0.0, 0.0),
            last_frame: None,
            cursor_grabbed: false,
            saved_cursor_pos: None,
        }
    }

    fn apply_keyboard_message(&mut self, event: KeyboardInputMessage) {
        if event.pressed {
            self.keys_held.insert(event.key);
        } else {
            self.keys_held.remove(&event.key);
        }
    }

    fn apply_mouse_button_message(&mut self, event: MouseButtonMessage) {
        if event.pressed {
            self.mouse_buttons_held.insert(event.button);
        } else {
            self.mouse_buttons_held.remove(&event.button);
        }
    }

    fn apply_mouse_motion_message(&mut self, event: MouseMotionMessage) {
        self.mouse_delta.0 += event.dx;
        self.mouse_delta.1 += event.dy;
    }

    fn sync_cursor_mode(
        &mut self,
        desired: CursorMode,
        window: &dyn Window,
        cursor: &CursorPosition,
    ) {
        match desired {
            CursorMode::Confined if !self.cursor_grabbed => {
                self.saved_cursor_pos = Some((cursor.x as f64, cursor.y as f64));
                window.confine_or_lock_cursor();
                window.set_cursor_visible(false);
                self.cursor_grabbed = true;
            }
            CursorMode::Free if self.cursor_grabbed => {
                window.release_cursor();
                if let Some((x, y)) = self.saved_cursor_pos.take() {
                    window.set_cursor_position(x, y);
                }
                window.set_cursor_visible(true);
                self.cursor_grabbed = false;
            }
            _ => {}
        }
    }

    fn next_frame(&mut self) -> (RawInput, FrameTime) {
        let now = Instant::now();
        let dt = self
            .last_frame
            .map(|prev| now.duration_since(prev).as_secs_f32())
            .unwrap_or(1.0 / 60.0);
        self.last_frame = Some(now);

        let input = RawInput {
            keys_held: self.keys_held.clone(),
            mouse_buttons_held: self.mouse_buttons_held.clone(),
            mouse_dx: self.mouse_delta.0 as f32,
            mouse_dy: self.mouse_delta.1 as f32,
        };

        self.mouse_delta = (0.0, 0.0);
        (input, FrameTime { dt })
    }
}

impl Default for InputState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn init_input_resources(world: &mut World) {
    if world.get_resource::<InputState>().is_none() {
        world.insert_resource(InputState::new());
    }
    world.get_resource_or_init::<RawInput>();
    world.get_resource_or_init::<FrameTime>();
    world.get_resource_or_init::<CursorPosition>();
    MessageRegistry::register_message::<KeyboardInputMessage>(world);
    MessageRegistry::register_message::<MouseButtonMessage>(world);
    MessageRegistry::register_message::<MouseMotionMessage>(world);
    MessageRegistry::register_message::<CursorMovedMessage>(world);
}

pub fn apply_keyboard_messages(
    mut state: ResMut<InputState>,
    mut events: MessageReader<KeyboardInputMessage>,
) {
    for event in events.read() {
        state.apply_keyboard_message(*event);
    }
}

pub fn apply_mouse_button_messages(
    mut state: ResMut<InputState>,
    mut events: MessageReader<MouseButtonMessage>,
) {
    for event in events.read() {
        state.apply_mouse_button_message(*event);
    }
}

pub fn apply_mouse_motion_messages(
    mut state: ResMut<InputState>,
    mut events: MessageReader<MouseMotionMessage>,
) {
    for event in events.read() {
        state.apply_mouse_motion_message(*event);
    }
}

pub fn flush_input_system(
    mut state: ResMut<InputState>,
    mut raw_input: ResMut<RawInput>,
    mut frame_time: ResMut<FrameTime>,
) {
    let (next_raw_input, next_frame_time) = state.next_frame();
    *raw_input = next_raw_input;
    *frame_time = next_frame_time;
}

pub fn sync_cursor_mode_system(
    mut state: ResMut<InputState>,
    cursor_mode: Option<Res<CursorMode>>,
    game_window: Option<Res<GameWindow>>,
    cursor: Res<CursorPosition>,
) {
    let Some(cursor_mode) = cursor_mode else {
        return;
    };
    let Some(game_window) = game_window else {
        return;
    };

    state.sync_cursor_mode(*cursor_mode, game_window.handle.as_ref(), &cursor);
}

pub fn update_cursor_position(
    mut cursor: ResMut<CursorPosition>,
    mut events: MessageReader<CursorMovedMessage>,
    state: Res<InputState>,
) {
    for event in events.read() {
        if !state.cursor_grabbed {
            cursor.x = event.x as f32;
            cursor.y = event.y as f32;
        }
    }
}
