//! The keyboard as the console's pad: one table for every screen of the app (user's
//! layout, 2026-10-09). W, A, S and D or the arrow keys are the directions, Shift makes a
//! run of a walk, Enter is Start and Tab Select; J is the square, K the cross, L the
//! circle and I the triangle, as the four lie on the pad; Q and E are R1 and R2, U and O
//! L1 and L2.

use bevy::prelude::*;
use jc_formats::pad;

const KEYBOARD: [(KeyCode, u16); 20] = [
    (KeyCode::KeyW, pad::UP),
    (KeyCode::KeyS, pad::DOWN),
    (KeyCode::KeyA, pad::LEFT),
    (KeyCode::KeyD, pad::RIGHT),
    (KeyCode::ArrowUp, pad::UP),
    (KeyCode::ArrowDown, pad::DOWN),
    (KeyCode::ArrowLeft, pad::LEFT),
    (KeyCode::ArrowRight, pad::RIGHT),
    (KeyCode::Enter, pad::START),
    (KeyCode::Tab, pad::SELECT),
    (KeyCode::KeyJ, pad::SQUARE),
    (KeyCode::KeyK, pad::CROSS),
    (KeyCode::KeyL, pad::CIRCLE),
    (KeyCode::KeyI, pad::TRIANGLE),
    (KeyCode::KeyQ, pad::R1),
    (KeyCode::KeyE, pad::R2),
    (KeyCode::KeyU, pad::L1),
    (KeyCode::KeyO, pad::L2),
    // The cross and the triangle as they were before the layout, for a hand that is
    // used to them.
    (KeyCode::KeyX, pad::CROSS),
    (KeyCode::Backspace, pad::TRIANGLE),
];

/// The pad's word of held buttons from the keyboard.
pub fn held(keys: &ButtonInput<KeyCode>) -> u16 {
    (KEYBOARD.iter())
        .filter(|(key, _)| keys.pressed(*key))
        .fold(0, |held, (_, button)| held | button)
}

/// Whether the key that makes a run of a walk is down.
pub fn fast(keys: &ButtonInput<KeyCode>) -> bool {
    keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight])
}
