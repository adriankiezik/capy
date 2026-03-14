use capy_core::KeyCode;
use winit::keyboard::KeyCode as WinitKeyCode;

macro_rules! convert_keys {
    ($($variant:ident),* $(,)?) => {
        pub(crate) fn convert_key(winit_key: WinitKeyCode) -> Option<KeyCode> {
            match winit_key {
                $(WinitKeyCode::$variant => Some(KeyCode::$variant),)*
                _ => None,
            }
        }
    };
}

convert_keys! {
    KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ,
    KeyK, KeyL, KeyM, KeyN, KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT,
    KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
    Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
    ArrowUp, ArrowDown, ArrowLeft, ArrowRight,
    ShiftLeft, ShiftRight, ControlLeft, ControlRight, AltLeft, AltRight,
    Space, Enter, Escape, Backspace, Tab, CapsLock,
    Delete, Insert, Home, End, PageUp, PageDown,
    Minus, Equal, BracketLeft, BracketRight, Backslash,
    Semicolon, Quote, Comma, Period, Slash, Backquote,
}
