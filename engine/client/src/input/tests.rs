#![allow(clippy::unwrap_used)]

use super::*;
use glam::Vec2;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Action {
    Move,
    Menu,
    Look,
}

fn frame(
    input: &mut Input,
    map: &mut InputMap<Action>,
    label: &str,
    expected: [(bool, bool, bool); 2],
) {
    let actions = map.update(input, Duration::from_millis(16));

    for (action, expected) in [Action::Move, Action::Menu].into_iter().zip(expected) {
        let state = actions.button(action);

        assert_eq!(
            (state.pressed(), state.just_pressed(), state.just_released()),
            expected,
            "{label}: {action:?}"
        );
    }

    input.finish_update();
}

// Holds a movement key while opening and closing a menu that uses the same key.
// The held key must not accidentally activate the menu or restart movement after closing it;
// a fresh press is required. Losing and regaining window focus must not leave a key stuck.
#[test]
fn consuming_menu_handoff_requires_release_without_leaking_held_input() {
    let mut input = Input::default();

    input.focus(true);

    let mut map = InputMap::new().bind(Action::Move, KeyCode::KeyW);

    let menu = map.add_context(
        InputContext::new()
            .bind(Action::Menu, KeyCode::KeyW)
            .with_priority(10)
            .with_active(false),
    );

    let idle = (false, false, false);

    let press = (true, true, false);

    let release = (false, false, true);

    frame(&mut input, &mut map, "initial", [idle, idle]);

    input.change(KeyCode::KeyW, true);

    frame(&mut input, &mut map, "movement starts", [press, idle]);

    map.context_mut(menu).unwrap().set_active(true);

    frame(
        &mut input,
        &mut map,
        "menu opens while held",
        [release, idle],
    );

    frame(
        &mut input,
        &mut map,
        "held key remains blocked",
        [idle, idle],
    );

    input.change(KeyCode::KeyW, false);

    input.change(KeyCode::KeyW, true);

    frame(
        &mut input,
        &mut map,
        "fresh press in same frame as release",
        [idle, press],
    );

    map.context_mut(menu).unwrap().set_active(false);

    frame(
        &mut input,
        &mut map,
        "menu closes while held",
        [idle, release],
    );

    frame(
        &mut input,
        &mut map,
        "retired claim still blocks movement",
        [idle, idle],
    );

    input.change(KeyCode::KeyW, false);

    frame(&mut input, &mut map, "retired key released", [idle, idle]);

    input.change(KeyCode::KeyW, true);

    frame(&mut input, &mut map, "movement resumes", [press, idle]);

    input.focus(false);

    frame(&mut input, &mut map, "focus lost", [release, idle]);

    input.change(KeyCode::KeyW, true);

    frame(
        &mut input,
        &mut map,
        "unfocused presses ignored",
        [idle, idle],
    );

    input.focus(true);

    frame(
        &mut input,
        &mut map,
        "focus restored without stuck keys",
        [idle, idle],
    );

    input.change(KeyCode::KeyW, true);

    frame(&mut input, &mut map, "new focused press", [press, idle]);
}

// Uses two keys for the same action and checks that switching between them keeps the
// action held, repeated key notifications do not create extra presses, and a quick tap
// is not lost. Reassigning a held key must not trigger its new action until it is pressed again.
#[test]
fn alternative_bindings_and_rebinding_preserve_action_edges_across_frames() {
    let mut input = Input::default();

    input.focus(true);

    let mut map = InputMap::new().bind(
        Action::Move,
        ButtonBinding::new(KeyCode::KeyW).or(KeyCode::ArrowUp),
    );

    let idle = (false, false, false);

    let press = (true, true, false);

    let held = (true, false, false);

    let release = (false, false, true);

    frame(&mut input, &mut map, "initial", [idle, idle]);

    input.change(KeyCode::KeyW, true);

    input.change(KeyCode::KeyW, true);

    frame(
        &mut input,
        &mut map,
        "key repeats do not duplicate presses",
        [press, idle],
    );

    input.change(KeyCode::ArrowUp, true);

    input.change(KeyCode::KeyW, false);

    frame(
        &mut input,
        &mut map,
        "alternative keeps action held",
        [held, idle],
    );

    input.change(KeyCode::ArrowUp, false);

    input.change(KeyCode::KeyW, true);

    input.change(KeyCode::KeyW, false);

    frame(
        &mut input,
        &mut map,
        "release and tap in one frame",
        [(false, true, true), idle],
    );

    input.change(KeyCode::KeyW, true);

    frame(&mut input, &mut map, "held before rebinding", [press, idle]);

    map.remove(Action::Move);

    map.insert(Action::Menu, KeyCode::KeyW);

    frame(
        &mut input,
        &mut map,
        "rebind does not activate from existing hold",
        [release, idle],
    );

    input.change(KeyCode::KeyW, false);

    frame(
        &mut input,
        &mut map,
        "release after rebinding",
        [idle, idle],
    );

    input.change(KeyCode::KeyW, true);

    frame(
        &mut input,
        &mut map,
        "fresh press uses new action",
        [idle, press],
    );
}

// Checks that mouse movement goes to the right control depending on whether the cursor
// is captured for gameplay. Switching capture must discard old movement so the view does
// not jump, and a control that takes the movement must not also pass it to a lower-priority control.
#[test]
fn cursor_capture_discards_stale_motion_and_routes_only_enabled_bindings() {
    let mut input = Input::default();

    input.focus(true);

    let mut map = InputMap::new().bind(Action::Look, Axis2Binding::mouse_motion());

    let overlay = map.add_context(
        InputContext::new()
            .bind(
                Action::Menu,
                Axis2Binding::mouse_motion()
                    .when_cursor_captured()
                    .scale(Vec2::splat(2.0)),
            )
            .with_priority(10),
    );

    input.motion((3.0, -2.0));

    let actions = map.update(&input, Duration::from_millis(16));

    assert_eq!(actions.axis2(Action::Look), Vec2::new(3.0, -2.0));

    assert_eq!(actions.axis2(Action::Menu), Vec2::ZERO);

    input.finish_update();

    input.motion((100.0, 100.0));

    input.sync_cursor(true, 1);

    input.motion((4.0, -1.0));

    let actions = map.update(&input, Duration::from_millis(16));

    assert_eq!(actions.axis2(Action::Menu), Vec2::new(8.0, -2.0));

    assert_eq!(actions.axis2(Action::Look), Vec2::ZERO);

    input.finish_update();

    assert_eq!(
        map.update(&input, Duration::from_millis(16))
            .axis2(Action::Menu),
        Vec2::ZERO
    );

    map.context_mut(overlay).unwrap().set_active(false);

    input.motion((1.0, 2.0));

    assert_eq!(
        map.update(&input, Duration::from_millis(16))
            .axis2(Action::Look),
        Vec2::new(1.0, 2.0)
    );

    input.sync_cursor(false, 2);

    assert_eq!(
        map.update(&input, Duration::from_millis(16))
            .axis2(Action::Look),
        Vec2::ZERO
    );
}
