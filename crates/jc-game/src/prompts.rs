
use std::sync::{Arc, RwLock};

use bevy::input::gamepad::GamepadButton;
use bevy::prelude::*;
use jc_formats::menu::Rect;
use jc_formats::name::{self, LegendPiece};
use jc_formats::text::Font;
use serde::{Deserialize, Serialize};

use crate::bindings::{self, Action, Bindings, KeyBindings, PadBindings};
use crate::gamepad::{Device, InputDevice};
use crate::name_entry::blend;
use crate::prefs::{self, Prefs};

/// `[prompts] style`: which buttons the lines of help show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Style {
    /// The keyboard's keys, or an Xbox pad's buttons while a gamepad is played with.
    #[default]
    Auto,
    /// The game's own signs.
    Playstation,
    Keyboard,
    Xbox,
}

/// What the lines of help show at the moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shown {
    PlayStation,
    Keyboard,
    Xbox,
}

impl Style {
    /// What the style shows with the device the player used last.
    pub fn shown(self, device: &InputDevice) -> Shown {
        match self {
            Style::Playstation => Shown::PlayStation,
            Style::Keyboard => Shown::Keyboard,
            Style::Xbox => Shown::Xbox,
            Style::Auto => match device.last {
                Device::Gamepad if device.pad_connected => Shown::Xbox,
                // A phone's buttons are the touch screen's, which bear the game's own
                // signs (`touch.rs`).
                _ if cfg!(target_os = "android") => Shown::PlayStation,
                _ => Shown::Keyboard,
            },
        }
    }
}

/// `--prompts=auto|playstation|keyboard|xbox`: the style of one check;
/// `--interaction-hint=on|off` its hint over the hero (`hint.rs`).
pub fn for_a_check(prompts: &mut prefs::Prompts, args: &[String]) {
    if let Some(value) = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--interaction-hint="))
    {
        prompts.interaction_hint = match value {
            "on" => true,
            "off" => false,
            _ => panic!("--interaction-hint= takes on or off"),
        };
    }
    let Some(value) = args.iter().find_map(|arg| arg.strip_prefix("--prompts=")) else {
        return;
    };
    prompts.style = match value {
        "auto" => Style::Auto,
        "playstation" => Style::Playstation,
        "keyboard" => Style::Keyboard,
        "xbox" => Style::Xbox,
        _ => panic!("--prompts= takes auto, playstation, keyboard or xbox"),
    };
}

const SPRITES: [(usize, Action); 8] = [
    (16, Action::Cross),
    (110, Action::Circle),
    (167, Action::Triangle),
    (94, Action::Square),
    (99, Action::L1),
    (100, Action::L2),
    (153, Action::R1),
    (154, Action::R2),
];

const UNITS: [(u16, Action); 4] = [
    (0x9B81, Action::Circle),
    (0x7E81, Action::Cross),
    (0xA281, Action::Triangle),
    (0xA081, Action::Square),
];

/// Where a glyph of the font of the texts has its middle in its cell: a sign of a button
/// is 10 pixels wide and lies in the rows 2 to 11.
const GLYPH_MIDDLE: (i16, i16) = (5, 7);

/// A picture made here: four bytes a texel, an alpha of 255 drawn whole, 128 half over
/// what is under it, 0 not at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Glyph {
    pub(crate) wide: usize,
    pub(crate) high: usize,
    rgba: Vec<u8>,
}

impl Glyph {
    fn new(wide: usize, high: usize) -> Self {
        Self {
            wide,
            high,
            rgba: vec![0; wide * high * 4],
        }
    }

    fn set(&mut self, x: usize, y: usize, colour: [u8; 3], alpha: u8) {
        if x < self.wide && y < self.high {
            let at = (y * self.wide + x) * 4;
            self.rgba[at..at + 3].copy_from_slice(&colour);
            self.rgba[at + 3] = alpha;
        }
    }

    /// The picture as an image of the console's screen (the interaction hint draws it
    /// over the room, `hint.rs`).
    pub(crate) fn image(&self) -> Image {
        crate::picture_image(self.wide as u32, self.high as u32, self.rgba.clone())
    }

    /// The colour of a texel and whether it is drawn whole, half or not at all.
    #[cfg(test)]
    fn texel(&self, x: usize, y: usize) -> ([u8; 3], u8) {
        let at = (y * self.wide + x) * 4;
        (
            [0, 1, 2].map(|part| self.rgba[at + part]),
            self.rgba[at + 3],
        )
    }
}

/// The pictures of a sign of the pool: with the long name and with the short one.
type OfSprite = (usize, Arc<Glyph>, Arc<Glyph>);

/// The pictures of one style.
#[derive(Debug)]
pub(crate) struct Signs {
    /// For every sign of [`SPRITES`] that has a key or a button: the picture of a line
    /// of help.
    menus: Vec<OfSprite>,
    /// The same on a name entry, where the keys that type are no buttons
    /// (`name_entry::typing_keys`): the first key that does not type, or else the
    /// mouse's button.
    naming: Vec<OfSprite>,
    /// For every sign of [`UNITS`] that has one: the picture inside a text.
    texts: Vec<(u16, Arc<Glyph>)>,
}

impl Signs {
    /// The pictures of a style that is not the game's own, with the keys and the buttons
    /// of `bindings`.
    pub(crate) fn new(shown: Shown, bindings: &Bindings) -> Self {
        let look = |action: Action, narrow: usize, naming: bool| -> Option<(Glyph, Glyph)> {
            match shown {
                Shown::PlayStation => None,
                Shown::Keyboard => {
                    let key = (bindings.keys_of(action))
                        .find(|key| !naming || !crate::name_entry::typing_keys(*key));
                    let Some(key) = key else {
                        let glyph = mouse(mouse_button(action)?);
                        return Some((glyph.clone(), glyph));
                    };
                    let (long, short) = key_label(key);
                    Some((key_cap(&long, narrow, false), key_cap(&short, narrow, true)))
                }
                Shown::Xbox => {
                    let button = (bindings.buttons.iter())
                        .find(|(_, of)| *of == action)
                        .map(|(button, _)| *button)?;
                    let glyph = pad_button(button);
                    Some((glyph.clone(), glyph))
                }
            }
        };
        let of_sprites = |naming: bool| -> Vec<OfSprite> {
            (SPRITES.iter())
                .filter_map(|&(sprite, action)| {
                    let (long, short) = look(action, MENU_KEY, naming)?;
                    Some((sprite, Arc::new(long), Arc::new(short)))
                })
                .collect()
        };
        let texts = (UNITS.iter())
            .filter_map(|&(unit, action)| Some((unit, Arc::new(look(action, TEXT_KEY, false)?.1))))
            .collect();
        Self {
            menus: of_sprites(false),
            naming: of_sprites(true),
            texts,
        }
    }

    /// The picture in place of a sprite of the pool, if it is the sign of a button; on a
    /// name entry (`naming`) or elsewhere.
    fn of_sprite(&self, sprite: usize, short: bool, naming: bool) -> Option<&Arc<Glyph>> {
        let signs = if naming { &self.naming } else { &self.menus };
        (signs.iter())
            .find(|(of, ..)| *of == sprite)
            .map(|(_, long, narrow)| if short { narrow } else { long })
    }

    /// The picture in place of a glyph of a text, if its unit is the sign of a button.
    fn of_unit(&self, unit: u16) -> Option<&Arc<Glyph>> {
        (self.texts.iter())
            .find(|(of, _)| *of == unit)
            .map(|(_, glyph)| glyph)
    }
}

/// The pictures in use; none while the game's own signs are shown.
static NOW: RwLock<Option<Arc<Signs>>> = RwLock::new(None);

/// The pictures the painters draw with now.
pub(crate) fn now() -> Option<Arc<Signs>> {
    NOW.read().ok().and_then(|now| now.clone())
}

fn set(signs: Option<Arc<Signs>>) {
    if let Ok(mut now) = NOW.write() {
        *now = signs;
    }
}

/// The picture of the confirm button in use, in every style; none before the first
/// pass of [`follow`].
static CONFIRM: RwLock<Option<Arc<Glyph>>> = RwLock::new(None);

/// The picture of the confirm button now, for the interaction hint (`hint.rs`).
pub(crate) fn confirm() -> Option<Arc<Glyph>> {
    CONFIRM.read().ok().and_then(|confirm| confirm.clone())
}

/// The picture of the confirm button (the cross of the game's pad) as `shown` draws it:
/// the first key `[controls.keys]` binds to it as a key cap (the mouse's left button
/// when none), the first button `[controls.pad]` binds to it as an Xbox pad's (A when
/// none), or the console's cross.
pub(crate) fn confirm_glyph(shown: Shown, bindings: &Bindings) -> Glyph {
    match shown {
        Shown::PlayStation => cross_button(),
        Shown::Keyboard => match bindings.keys_of(Action::Cross).next() {
            Some(key) => key_cap(&key_label(key).0, MENU_KEY, false),
            None => mouse(MouseButton::Left),
        },
        Shown::Xbox => match (bindings.buttons.iter()).find(|(_, of)| *of == Action::Cross) {
            Some((button, _)) => pad_button(*button),
            None => round("A", XBOX_A, WHITE),
        },
    }
}

/// The picture of Start in use, in every style; none before the first pass of
/// [`follow`].
static START: RwLock<Option<Arc<Glyph>>> = RwLock::new(None);

/// The picture of Start now, for the ring of the skip of a scene (`skip.rs`).
pub(crate) fn start() -> Option<Arc<Glyph>> {
    START.read().ok().and_then(|start| start.clone())
}

/// The picture of Start as `shown` draws it: the first key `[controls.keys]` binds to
/// it as a key cap, the first button `[controls.pad]` binds to it as an Xbox pad's
/// (Menu when none), or the console's word START on a dark bar.
pub(crate) fn start_glyph(shown: Shown, bindings: &Bindings) -> Glyph {
    match shown {
        Shown::PlayStation => bar("START"),
        Shown::Keyboard => match bindings.keys_of(Action::Start).next() {
            Some(key) => key_cap(&key_label(key).0, MENU_KEY, false),
            None => bar("START"),
        },
        Shown::Xbox => match (bindings.buttons.iter()).find(|(_, of)| *of == Action::Start) {
            Some((button, _)) => pad_button(*button),
            None => bar("MENU"),
        },
    }
}

/// Follows the style, the device and the bindings, in front of every painter.
pub struct PromptsPlugin;

impl Plugin for PromptsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, follow.after(crate::gamepad::follow_device));
    }
}

/// What the pictures were made for.
type Made = (Shown, KeyBindings, PadBindings);

/// Makes the pictures again whenever the style, the device the player used last or the
/// bindings have changed: the lines of help follow from the next pass of their screen on.
fn follow(prefs: Res<Prefs>, device: Res<InputDevice>, mut made: Local<Option<Made>>) {
    let shown = prefs.feel.prompts.style.shown(&device);
    let controls = &prefs.controls;
    let same =
        |(was, keys, pad): &Made| *was == shown && *keys == controls.keys && *pad == controls.pad;
    if made.as_ref().is_some_and(same) {
        return;
    }
    let (bindings, _) = bindings::resolve(&controls.keys, &controls.pad);
    let signs = match shown {
        Shown::PlayStation => None,
        _ => Some(Arc::new(Signs::new(shown, &bindings))),
    };
    set(signs);
    if let Ok(mut confirm) = CONFIRM.write() {
        *confirm = Some(Arc::new(confirm_glyph(shown, &bindings)));
    }
    if let Ok(mut start) = START.write() {
        *start = Some(Arc::new(start_glyph(shown, &bindings)));
    }
    info!("the button prompts: {shown:?}");
    *made = Some((shown, controls.keys.clone(), controls.pad.clone()));
}

/// A sign or a word of a line of help, and where it goes.
pub(crate) enum Laid {
    /// A sprite of the pool, as the game draws it.
    Sprite {
        sprite: usize,
        at: (i16, i16),
    },
    /// A picture of a key or a button in place of a sign.
    Glyph {
        glyph: Arc<Glyph>,
        at: (i16, i16),
    },
    Word {
        units: Vec<u16>,
        at: (i16, i16),
    },
}

/// The pairs of a line of help laid out as the game's drawer lays them out
/// (`name::legend_pieces`), with the pictures of keys or buttons in place of the
/// signs. A line that would run past its bar with the long names takes the short ones.
/// `sizes` gives the width and the height of a sprite of the pool; `naming` is the line
/// of a name entry.
pub(crate) fn legend(
    pairs: &[(usize, Vec<u16>)],
    font: &Font,
    sizes: impl Fn(usize) -> Option<(i16, i16)>,
    naming: bool,
) -> Vec<Laid> {
    let signs = now();
    let lay = |short: bool| {
        let glyph = |sprite: usize| signs.as_ref()?.of_sprite(sprite, short, naming).cloned();
        let size = |sprite: usize| match glyph(sprite) {
            Some(glyph) => Some((glyph.wide as i16, glyph.high as i16)),
            None => sizes(sprite),
        };
        let pieces = name::legend_pieces(pairs, font, size).unwrap_or_default();
        let end = (pieces.iter())
            .filter_map(|piece| match piece {
                LegendPiece::Word { units, at } => Some(at.0 + font.length(&font.codes(units))),
                LegendPiece::Sign { .. } => None,
            })
            .max()
            .unwrap_or(0);
        let laid: Vec<Laid> = (pieces.into_iter())
            .map(|piece| match piece {
                LegendPiece::Sign { sprite, at } => match glyph(sprite) {
                    Some(glyph) => Laid::Glyph { glyph, at },
                    None => Laid::Sprite { sprite, at },
                },
                LegendPiece::Word { units, at } => Laid::Word { units, at },
            })
            .collect();
        (laid, end)
    };
    let bar = name::legend(0).rect;
    let (laid, end) = lay(false);
    match end > bar.x + bar.w - 2 {
        true => lay(true).0,
        false => laid,
    }
}

/// The picture in place of a glyph of a text and where it goes, when the glyph of `code`
/// of `font` with its cell at `at` is the sign of a button: over the middle of the sign.
pub(crate) fn in_text(
    signs: Option<&Signs>,
    font: &Font,
    code: u16,
    at: (i16, i16),
) -> Option<(Arc<Glyph>, (i16, i16))> {
    let signs = signs?;
    // A code is its place in the font's table, counted from 1.
    let unit = *font.table.get(usize::from(code).checked_sub(1)?)?;
    let glyph = signs.of_unit(unit)?.clone();
    let place = (
        at.0 + GLYPH_MIDDLE.0 - (glyph.wide as i16) / 2,
        at.1 + GLYPH_MIDDLE.1 - (glyph.high as i16) / 2,
    );
    Some((glyph, place))
}

/// Puts a picture on the console's screen, inside `clip`.
pub(crate) fn put(screen: &mut [u8], glyph: &Glyph, at: (i16, i16), clip: Rect) {
    for (v, u) in (0..glyph.high).flat_map(|v| (0..glyph.wide).map(move |u| (v, u))) {
        let texel = (v * glyph.wide + u) * 4;
        let alpha = glyph.rgba[texel + 3];
        let (x, y) = (at.0 + u as i16, at.1 + v as i16);
        if alpha == 0 || !crate::options_view::inside(clip, x, y) {
            continue;
        }
        let colour = [0, 1, 2].map(|part| glyph.rgba[texel + part]);
        let mode = if alpha < 255 { Some(0) } else { None };
        blend(screen, x, y, colour, mode);
    }
}

// The pictures.

/// The narrowest key cap of a line of help: as wide as the game's sign.
const MENU_KEY: usize = 12;
/// The narrowest one inside a text: as wide as the sign's glyph.
const TEXT_KEY: usize = 10;
/// A key cap: its rim, the shadow under it, its face and its letters.
const CAP_RIM: [u8; 3] = [198, 198, 206];
const CAP_SHADOW: [u8; 3] = [107, 107, 115];
const CAP_FACE: [u8; 3] = [41, 41, 49];
const CAP_INK: [u8; 3] = [239, 239, 239];
/// The button of a mouse that is meant.
const MOUSE_LIT: [u8; 3] = [247, 206, 82];
/// The faces of an Xbox pad's four buttons, and the dark of the others.
const XBOX_A: [u8; 3] = [90, 180, 60];
const XBOX_B: [u8; 3] = [222, 60, 52];
const XBOX_X: [u8; 3] = [52, 120, 230];
const XBOX_Y: [u8; 3] = [238, 196, 40];
const XBOX_DARK: [u8; 3] = [49, 49, 57];
const XBOX_RIM: [u8; 3] = [148, 148, 156];
const WHITE: [u8; 3] = [247, 247, 247];
/// The console pad's face and the blue of its cross.
const PS_FACE: [u8; 3] = [57, 57, 66];
const PS_CROSS: [u8; 3] = [123, 173, 239];
const INK_ON_YELLOW: [u8; 3] = [33, 33, 33];

/// What a key cap says: the long name, and the short one for a narrow place.
pub(crate) fn key_label(key: KeyCode) -> (String, String) {
    use KeyCode::*;
    let both = |long: &str, short: &str| (long.to_owned(), short.to_owned());
    let name = bindings::key_name(key);
    if let Some(letter) = (name.strip_prefix("Key")).or_else(|| name.strip_prefix("Digit")) {
        return both(letter, letter);
    }
    if let Some(digit) = name.strip_prefix("Numpad").filter(|rest| rest.len() == 1) {
        let named = format!("N{digit}");
        return both(&named, &named);
    }
    if name.len() <= 3 && name.starts_with('F') && name[1..].parse::<u8>().is_ok() {
        return both(&name, &name);
    }
    match key {
        Space => both("SPACE", "SPC"),
        Enter | NumpadEnter => both("ENTER", "ENT"),
        Tab => both("TAB", "TAB"),
        Escape => both("ESC", "ESC"),
        Backspace => both("BKSP", "BS"),
        ShiftLeft | ShiftRight => both("SHIFT", "SH"),
        ControlLeft | ControlRight => both("CTRL", "CT"),
        AltLeft | AltRight => both("ALT", "AL"),
        CapsLock => both("CAPS", "CL"),
        Insert => both("INS", "IN"),
        Delete => both("DEL", "DL"),
        Home => both("HOME", "HM"),
        End => both("END", "EN"),
        PageUp => both("PGUP", "PU"),
        PageDown => both("PGDN", "PD"),
        ArrowUp => both("↑", "↑"),
        ArrowDown => both("↓", "↓"),
        ArrowLeft => both("←", "←"),
        ArrowRight => both("→", "→"),
        Minus => both("-", "-"),
        Equal => both("=", "="),
        BracketLeft => both("[", "["),
        BracketRight => both("]", "]"),
        Backslash => both("\\", "\\"),
        Semicolon => both(";", ";"),
        Quote => both("'", "'"),
        Comma => both(",", ","),
        Period => both(".", "."),
        Slash => both("/", "/"),
        Backquote => both("`", "`"),
        NumpadAdd => both("N+", "N+"),
        NumpadSubtract => both("N-", "N-"),
        NumpadMultiply => both("N*", "N*"),
        NumpadDivide => both("N/", "N/"),
        NumpadDecimal => both("N.", "N."),
        _ => both("?", "?"),
    }
}

/// The key cap of a key as the lines of help draw it (the PC Settings screen,
/// `pc_settings.rs`).
pub(crate) fn key_glyph(key: KeyCode) -> Glyph {
    key_cap(&key_label(key).0, MENU_KEY, false)
}

/// An Xbox pad's button as the lines of help draw it.
pub(crate) fn button_glyph(button: GamepadButton) -> Glyph {
    pad_button(button)
}

/// A key cap with a name, at least `narrowest` wide and 12 high: a rim with round
/// corners, a shadow under it, the name in the middle of its face with a pixel of room
/// on either side. A `tight` cap has no rim on its sides, only the room.
fn key_cap(label: &str, narrowest: usize, tight: bool) -> Glyph {
    const HIGH: usize = 12;
    let (font, width) = letters_for(label, HIGH - 3);
    // A cap of its narrowest width keeps its sides.
    let tight = tight && width + 4 > narrowest;
    let wide = narrowest.max(width + if tight { 2 } else { 4 });
    let mut glyph = Glyph::new(wide, HIGH);
    let bottom = HIGH - 2;
    for y in 0..=bottom {
        for x in 0..wide {
            let side = x == 0 || x == wide - 1;
            let corner = side && (y == 0 || y == bottom);
            let rim = (side && !tight) || y == 0 || y == bottom;
            match (corner, rim) {
                (true, _) => {}
                (false, true) => glyph.set(x, y, CAP_RIM, 255),
                (false, false) => glyph.set(x, y, CAP_FACE, 255),
            }
        }
    }
    for x in 1..wide - 1 {
        glyph.set(x, HIGH - 1, CAP_SHADOW, 255);
    }
    write(&mut glyph, label, font, (0, 1, wide, bottom - 1), CAP_INK);
    glyph
}

/// An Xbox pad's button: A, B, X and Y as round buttons in their colours, the sticks'
/// buttons as dark round ones, every other as a dark bar with its name.
fn pad_button(button: GamepadButton) -> Glyph {
    use GamepadButton::*;
    match button {
        South => round("A", XBOX_A, WHITE),
        East => round("B", XBOX_B, WHITE),
        West => round("X", XBOX_X, WHITE),
        North => round("Y", XBOX_Y, INK_ON_YELLOW),
        LeftThumb => round("LS", XBOX_DARK, WHITE),
        RightThumb => round("RS", XBOX_DARK, WHITE),
        LeftTrigger => bar("LB"),
        RightTrigger => bar("RB"),
        LeftTrigger2 => bar("LT"),
        RightTrigger2 => bar("RT"),
        Select => bar("VIEW"),
        Start => bar("MENU"),
        Mode => bar("XBOX"),
        DPadUp => bar("↑"),
        DPadDown => bar("↓"),
        DPadLeft => bar("←"),
        DPadRight => bar("→"),
        other => bar(&bindings::button_name(other).to_ascii_uppercase()),
    }
}

/// The mouse's button of a pad's button (`keys.rs`): the left one is the cross, the
/// right one the triangle, the middle one the circle.
fn mouse_button(action: Action) -> Option<MouseButton> {
    match action {
        Action::Cross => Some(MouseButton::Left),
        Action::Triangle => Some(MouseButton::Right),
        Action::Circle => Some(MouseButton::Middle),
        _ => None,
    }
}

/// A mouse, 9 by 12, with the button `button` lit.
fn mouse(button: MouseButton) -> Glyph {
    const WIDE: usize = 9;
    const HIGH: usize = 12;
    let mut glyph = Glyph::new(WIDE, HIGH);
    for y in 0..HIGH {
        for x in 0..WIDE {
            let corner = (!(2..=WIDE - 3).contains(&x) && (y == 0 || y == HIGH - 1))
                || ((x == 0 || x == WIDE - 1) && (y == 1 || y == HIGH - 2));
            let rim = x == 0 || x == WIDE - 1 || y == 0 || y == HIGH - 1;
            let corner_rim = (x == 1 || x == WIDE - 2) && (y == 1 || y == HIGH - 2);
            // The line between the buttons and the one under them.
            let line = (x == WIDE / 2 && y < 5) || y == 5;
            let lit = match button {
                MouseButton::Left => x < WIDE / 2 && y < 5,
                MouseButton::Right => x > WIDE / 2 && y < 5,
                _ => x == WIDE / 2 && (2..4).contains(&y),
            };
            match () {
                _ if corner => {}
                _ if rim || corner_rim || (line && !lit) => glyph.set(x, y, CAP_RIM, 255),
                _ if lit => glyph.set(x, y, MOUSE_LIT, 255),
                _ => glyph.set(x, y, CAP_FACE, 255),
            }
        }
    }
    glyph
}

/// A round button 11 across in a picture of 12 by 12, as the game's signs lie in theirs:
/// a darker rim, the face, the name in the middle.
fn round(label: &str, face: [u8; 3], ink: [u8; 3]) -> Glyph {
    const SIDE: usize = 12;
    let mut glyph = Glyph::new(SIDE, SIDE);
    let rim = face.map(|part| (u16::from(part) * 3 / 5) as u8);
    let middle = 5.0f32;
    for y in 0..SIDE {
        for x in 0..SIDE {
            let away = ((x as f32 - middle).powi(2) + (y as f32 - middle).powi(2)).sqrt();
            match away {
                away if away <= 4.6 => glyph.set(x, y, face, 255),
                away if away <= 5.4 => glyph.set(x, y, rim, 255),
                away if away <= 5.9 => glyph.set(x, y, rim, 128),
                _ => {}
            }
        }
    }
    let (font, _) = letters_for(label, 7);
    write(&mut glyph, label, font, (1, 1, 9, 9), ink);
    glyph
}

/// The console pad's cross: a dark round button 11 across in a picture of 12 by 12, as
/// [`round`] makes one, with the blue cross on it.
fn cross_button() -> Glyph {
    let mut glyph = round("", PS_FACE, WHITE);
    for step in -3i32..=3 {
        for (x, y) in [(5 + step, 5 + step), (5 + step, 5 - step)] {
            glyph.set(x as usize, y as usize, PS_CROSS, 255);
        }
    }
    glyph
}

/// A dark bar with round corners and a name, 9 high: the shoulders, the triggers and the
/// small buttons of an Xbox pad.
fn bar(label: &str) -> Glyph {
    const HIGH: usize = 9;
    let (font, width) = letters_for(label, HIGH - 2);
    let wide = (width + 4).max(9);
    let mut glyph = Glyph::new(wide, HIGH);
    for y in 0..HIGH {
        for x in 0..wide {
            let corner = (x == 0 || x == wide - 1) && (y == 0 || y == HIGH - 1);
            let rim = x == 0 || x == wide - 1 || y == 0 || y == HIGH - 1;
            match (corner, rim) {
                (true, _) => {}
                (false, true) => glyph.set(x, y, XBOX_RIM, 255),
                (false, false) => glyph.set(x, y, XBOX_DARK, 255),
            }
        }
    }
    write(&mut glyph, label, font, (1, 1, wide - 2, HIGH - 2), WHITE);
    glyph
}

/// The two pixel fonts: letters 5 by 7 for a name of one letter where there is room for
/// it, 3 by 5 for every other name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Letters {
    Large,
    Small,
}

impl Letters {
    fn high(self) -> usize {
        match self {
            Letters::Large => 7,
            Letters::Small => 5,
        }
    }

    fn wide(self) -> usize {
        match self {
            Letters::Large => 5,
            Letters::Small => 3,
        }
    }

    /// The rows of a letter, `#` drawn.
    fn rows(self, letter: char) -> Option<&'static [&'static str]> {
        let rows: &[(char, &[&str])] = match self {
            Letters::Large => LARGE,
            Letters::Small => SMALL,
        };
        let letter = letter.to_ascii_uppercase();
        (rows.iter())
            .find(|(of, _)| *of == letter)
            .map(|(_, rows)| *rows)
    }
}

/// The font a name is written in where `room` rows are free, and its width.
fn letters_for(label: &str, room: usize) -> (Letters, usize) {
    let mut letters = label.chars();
    let large = match (letters.next(), letters.next()) {
        (Some(letter), None) => Letters::Large.rows(letter).is_some() && room >= 7,
        _ => false,
    };
    let font = if large {
        Letters::Large
    } else {
        Letters::Small
    };
    let count = label.chars().count();
    (font, count * font.wide() + count.saturating_sub(1))
}

/// Writes a name in the middle of an area (x, y, width, height) of a picture.
fn write(
    glyph: &mut Glyph,
    label: &str,
    font: Letters,
    area: (usize, usize, usize, usize),
    ink: [u8; 3],
) {
    let count = label.chars().count();
    let width = count * font.wide() + count.saturating_sub(1);
    let x = area.0 + area.2.saturating_sub(width) / 2;
    let y = area.1 + area.3.saturating_sub(font.high()) / 2;
    for (number, letter) in label.chars().enumerate() {
        let Some(rows) = font.rows(letter) else {
            continue;
        };
        let left = x + number * (font.wide() + 1);
        for (dy, row) in rows.iter().enumerate() {
            for (dx, texel) in row.chars().enumerate() {
                if texel == '#' {
                    glyph.set(left + dx, y + dy, ink, 255);
                }
            }
        }
    }
}

/// The letters 5 by 7.
const LARGE: &[(char, &[&str])] = &[
    (
        'A',
        &[
            ".###.", "#...#", "#...#", "#####", "#...#", "#...#", "#...#",
        ],
    ),
    (
        'B',
        &[
            "####.", "#...#", "#...#", "####.", "#...#", "#...#", "####.",
        ],
    ),
    (
        'C',
        &[
            ".###.", "#...#", "#....", "#....", "#....", "#...#", ".###.",
        ],
    ),
    (
        'D',
        &[
            "####.", "#...#", "#...#", "#...#", "#...#", "#...#", "####.",
        ],
    ),
    (
        'E',
        &[
            "#####", "#....", "#....", "####.", "#....", "#....", "#####",
        ],
    ),
    (
        'F',
        &[
            "#####", "#....", "#....", "####.", "#....", "#....", "#....",
        ],
    ),
    (
        'G',
        &[
            ".###.", "#...#", "#....", "#.###", "#...#", "#...#", ".####",
        ],
    ),
    (
        'H',
        &[
            "#...#", "#...#", "#...#", "#####", "#...#", "#...#", "#...#",
        ],
    ),
    (
        'I',
        &[
            ".###.", "..#..", "..#..", "..#..", "..#..", "..#..", ".###.",
        ],
    ),
    (
        'J',
        &[
            "..###", "...#.", "...#.", "...#.", "...#.", "#..#.", ".##..",
        ],
    ),
    (
        'K',
        &[
            "#...#", "#..#.", "#.#..", "##...", "#.#..", "#..#.", "#...#",
        ],
    ),
    (
        'L',
        &[
            "#....", "#....", "#....", "#....", "#....", "#....", "#####",
        ],
    ),
    (
        'M',
        &[
            "#...#", "##.##", "#.#.#", "#.#.#", "#...#", "#...#", "#...#",
        ],
    ),
    (
        'N',
        &[
            "#...#", "#...#", "##..#", "#.#.#", "#..##", "#...#", "#...#",
        ],
    ),
    (
        'O',
        &[
            ".###.", "#...#", "#...#", "#...#", "#...#", "#...#", ".###.",
        ],
    ),
    (
        'P',
        &[
            "####.", "#...#", "#...#", "####.", "#....", "#....", "#....",
        ],
    ),
    (
        'Q',
        &[
            ".###.", "#...#", "#...#", "#...#", "#.#.#", "#..#.", ".##.#",
        ],
    ),
    (
        'R',
        &[
            "####.", "#...#", "#...#", "####.", "#.#..", "#..#.", "#...#",
        ],
    ),
    (
        'S',
        &[
            ".####", "#....", "#....", ".###.", "....#", "....#", "####.",
        ],
    ),
    (
        'T',
        &[
            "#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#..",
        ],
    ),
    (
        'U',
        &[
            "#...#", "#...#", "#...#", "#...#", "#...#", "#...#", ".###.",
        ],
    ),
    (
        'V',
        &[
            "#...#", "#...#", "#...#", "#...#", "#...#", ".#.#.", "..#..",
        ],
    ),
    (
        'W',
        &[
            "#...#", "#...#", "#...#", "#.#.#", "#.#.#", "#.#.#", ".#.#.",
        ],
    ),
    (
        'X',
        &[
            "#...#", "#...#", ".#.#.", "..#..", ".#.#.", "#...#", "#...#",
        ],
    ),
    (
        'Y',
        &[
            "#...#", "#...#", ".#.#.", "..#..", "..#..", "..#..", "..#..",
        ],
    ),
    (
        'Z',
        &[
            "#####", "....#", "...#.", "..#..", ".#...", "#....", "#####",
        ],
    ),
    (
        '0',
        &[
            ".###.", "#...#", "#..##", "#.#.#", "##..#", "#...#", ".###.",
        ],
    ),
    (
        '1',
        &[
            "..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###.",
        ],
    ),
    (
        '2',
        &[
            ".###.", "#...#", "....#", "...#.", "..#..", ".#...", "#####",
        ],
    ),
    (
        '3',
        &[
            "####.", "....#", "....#", ".###.", "....#", "....#", "####.",
        ],
    ),
    (
        '4',
        &[
            "...#.", "..##.", ".#.#.", "#..#.", "#####", "...#.", "...#.",
        ],
    ),
    (
        '5',
        &[
            "#####", "#....", "####.", "....#", "....#", "#...#", ".###.",
        ],
    ),
    (
        '6',
        &[
            "..##.", ".#...", "#....", "####.", "#...#", "#...#", ".###.",
        ],
    ),
    (
        '7',
        &[
            "#####", "....#", "...#.", "..#..", ".#...", ".#...", ".#...",
        ],
    ),
    (
        '8',
        &[
            ".###.", "#...#", "#...#", ".###.", "#...#", "#...#", ".###.",
        ],
    ),
    (
        '9',
        &[
            ".###.", "#...#", "#...#", ".####", "....#", "...#.", ".##..",
        ],
    ),
    (
        '↑',
        &[
            "..#..", ".###.", "#.#.#", "..#..", "..#..", "..#..", "..#..",
        ],
    ),
    (
        '↓',
        &[
            "..#..", "..#..", "..#..", "..#..", "#.#.#", ".###.", "..#..",
        ],
    ),
    (
        '←',
        &[
            ".....", "..#..", ".#...", "#####", ".#...", "..#..", ".....",
        ],
    ),
    (
        '→',
        &[
            ".....", "..#..", "...#.", "#####", "...#.", "..#..", ".....",
        ],
    ),
];

/// The letters 3 by 5.
const SMALL: &[(char, &[&str])] = &[
    ('A', &[".#.", "#.#", "###", "#.#", "#.#"]),
    ('B', &["##.", "#.#", "##.", "#.#", "##."]),
    ('C', &[".##", "#..", "#..", "#..", ".##"]),
    ('D', &["##.", "#.#", "#.#", "#.#", "##."]),
    ('E', &["###", "#..", "##.", "#..", "###"]),
    ('F', &["###", "#..", "##.", "#..", "#.."]),
    ('G', &[".##", "#..", "#.#", "#.#", ".##"]),
    ('H', &["#.#", "#.#", "###", "#.#", "#.#"]),
    ('I', &["###", ".#.", ".#.", ".#.", "###"]),
    ('J', &["..#", "..#", "..#", "#.#", ".#."]),
    ('K', &["#.#", "#.#", "##.", "#.#", "#.#"]),
    ('L', &["#..", "#..", "#..", "#..", "###"]),
    ('M', &["#.#", "###", "###", "#.#", "#.#"]),
    ('N', &["##.", "#.#", "#.#", "#.#", "#.#"]),
    ('O', &[".#.", "#.#", "#.#", "#.#", ".#."]),
    ('P', &["##.", "#.#", "##.", "#..", "#.."]),
    ('Q', &[".#.", "#.#", "#.#", "##.", ".##"]),
    ('R', &["##.", "#.#", "##.", "#.#", "#.#"]),
    ('S', &[".##", "#..", ".#.", "..#", "##."]),
    ('T', &["###", ".#.", ".#.", ".#.", ".#."]),
    ('U', &["#.#", "#.#", "#.#", "#.#", "###"]),
    ('V', &["#.#", "#.#", "#.#", "#.#", ".#."]),
    ('W', &["#.#", "#.#", "###", "###", "#.#"]),
    ('X', &["#.#", "#.#", ".#.", "#.#", "#.#"]),
    ('Y', &["#.#", "#.#", ".#.", ".#.", ".#."]),
    ('Z', &["###", "..#", ".#.", "#..", "###"]),
    ('0', &["###", "#.#", "#.#", "#.#", "###"]),
    ('1', &[".#.", "##.", ".#.", ".#.", "###"]),
    ('2', &["##.", "..#", ".#.", "#..", "###"]),
    ('3', &["##.", "..#", ".#.", "..#", "##."]),
    ('4', &["#.#", "#.#", "###", "..#", "..#"]),
    ('5', &["###", "#..", "##.", "..#", "##."]),
    ('6', &[".##", "#..", "###", "#.#", "###"]),
    ('7', &["###", "..#", ".#.", ".#.", ".#."]),
    ('8', &["###", "#.#", "###", "#.#", "###"]),
    ('9', &["###", "#.#", "###", "..#", "##."]),
    ('-', &["...", "...", "###", "...", "..."]),
    ('+', &["...", ".#.", "###", ".#.", "..."]),
    ('=', &["...", "###", "...", "###", "..."]),
    ('*', &["...", "#.#", ".#.", "#.#", "..."]),
    ('[', &["##.", "#..", "#..", "#..", "##."]),
    (']', &[".##", "..#", "..#", "..#", ".##"]),
    ('/', &["..#", "..#", ".#.", "#..", "#.."]),
    ('\\', &["#..", "#..", ".#.", "..#", "..#"]),
    (';', &["...", ".#.", "...", ".#.", "#.."]),
    ('\'', &[".#.", ".#.", "...", "...", "..."]),
    (',', &["...", "...", "...", ".#.", "#.."]),
    ('.', &["...", "...", "...", "...", ".#."]),
    ('`', &["#..", ".#.", "...", "...", "..."]),
    ('?', &["##.", "..#", ".#.", "...", ".#."]),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn device(last: Device, pad_connected: bool) -> InputDevice {
        InputDevice {
            last,
            pad_connected,
        }
    }

    #[test]
    fn auto_shows_the_hand_the_player_used_last() {
        let keyboard = device(Device::Keyboard, false);
        let pad = device(Device::Gamepad, true);
        assert_eq!(Style::Auto.shown(&keyboard), Shown::Keyboard);
        assert_eq!(Style::Auto.shown(&pad), Shown::Xbox);
        // A pad in, the keyboard used last: the keys.
        assert_eq!(
            Style::Auto.shown(&device(Device::Keyboard, true)),
            Shown::Keyboard
        );
        // No pad: the keys, whatever was used last.
        assert_eq!(
            Style::Auto.shown(&device(Device::Gamepad, false)),
            Shown::Keyboard
        );
        for device in [keyboard, pad] {
            assert_eq!(Style::Playstation.shown(&device), Shown::PlayStation);
            assert_eq!(Style::Keyboard.shown(&device), Shown::Keyboard);
            assert_eq!(Style::Xbox.shown(&device), Shown::Xbox);
        }
    }

    #[test]
    fn the_style_is_read_from_the_settings() {
        let prefs: Prefs = toml::from_str("[prompts]\nstyle = \"xbox\"\n").unwrap();
        assert_eq!(prefs.feel.prompts.style, Style::Xbox);
        let prefs: Prefs = toml::from_str("").unwrap();
        assert_eq!(prefs.feel.prompts.style, Style::Auto);
        let written = toml::to_string(&Prefs::default()).unwrap();
        assert!(written.contains("style = \"auto\""), "{written}");
        let mut prompts = prefs::Prompts::default();
        for_a_check(&mut prompts, &["--prompts=playstation".to_owned()]);
        assert_eq!(prompts.style, Style::Playstation);
    }

    #[test]
    fn every_letter_of_the_fonts_has_its_size() {
        for (letter, rows) in LARGE {
            assert_eq!(rows.len(), 7, "{letter}");
            assert!(rows.iter().all(|row| row.chars().count() == 5), "{letter}");
        }
        for (letter, rows) in SMALL {
            assert_eq!(rows.len(), 5, "{letter}");
            assert!(rows.iter().all(|row| row.chars().count() == 3), "{letter}");
        }
        // Every name a key cap can have is written whole.
        for key in [KeyCode::KeyF, KeyCode::Escape, KeyCode::Space, KeyCode::F12] {
            let (long, short) = key_label(key);
            for label in [long, short] {
                let font = letters_for(&label, 9).0;
                assert!(
                    label.chars().all(|letter| font.rows(letter).is_some()),
                    "{label}"
                );
            }
        }
    }

    #[test]
    fn the_keys_are_the_first_bound_and_the_buttons_an_xbox_pads() {
        let bindings = Bindings::default();
        let keys = Signs::new(Shown::Keyboard, &bindings);
        // The cross is Space first, the triangle Escape, the circle R: as wide as the
        // game's sign for one letter, wider for a longer name.
        let cross = keys.of_sprite(16, false, false).unwrap();
        assert_eq!((cross.wide, cross.high), (23, 12));
        // The short cap of SPACE is SPC without rims on its sides; R keeps them.
        let short = keys.of_sprite(16, true, false).unwrap();
        assert_eq!(short.wide, 13);
        assert_eq!(short.texel(0, 5).0, CAP_FACE);
        assert_eq!(
            keys.of_sprite(110, true, false).unwrap().texel(0, 5).0,
            CAP_RIM
        );
        assert_eq!(keys.of_sprite(167, false, false).unwrap().wide, 15);
        assert_eq!(keys.of_sprite(110, false, false).unwrap().wide, MENU_KEY);
        // The rim, the face and the corner of a cap.
        assert_eq!(cross.texel(1, 0), (CAP_RIM, 255));
        assert_eq!(cross.texel(1, 1), (CAP_FACE, 255));
        assert_eq!(cross.texel(0, 0).1, 0);
        assert!(keys.of_sprite(17, false, false).is_none());
        let pad = Signs::new(Shown::Xbox, &bindings);
        // A is the cross, Y the circle: round, in their colours.
        let a = pad.of_sprite(16, false, false).unwrap();
        assert_eq!((a.wide, a.high), (12, 12));
        assert_eq!(a.texel(1, 5).0, XBOX_A);
        assert_eq!(
            pad.of_sprite(110, false, false).unwrap().texel(1, 5).0,
            XBOX_Y
        );
        // RB is R1, a dark bar.
        let rb = pad.of_sprite(153, false, false).unwrap();
        assert_eq!(rb.high, 9);
        assert_eq!(rb.texel(1, 1).0, XBOX_DARK);
        // On a name entry Space and F type: the cross is the mouse's left button there,
        // the triangle still Escape.
        let mouse = keys.of_sprite(16, false, true).unwrap();
        assert_eq!((mouse.wide, mouse.high), (9, 12));
        assert_eq!(mouse.texel(2, 2).0, MOUSE_LIT);
        assert_eq!(keys.of_sprite(167, false, true).unwrap().wide, 15);
        // Inside a text the signs are there too.
        assert!(pad.of_unit(0x9B81).is_some() && keys.of_unit(0x7E81).is_some());
        assert_eq!(keys.of_unit(0x7E81).unwrap().wide, 13);
        // Rebound: the cross on B shows B, a button without a key keeps the game's sign.
        let table = PadBindings {
            cross: vec!["B".to_owned()],
            ..PadBindings::default()
        };
        let keys_table = KeyBindings {
            square: Vec::new(),
            ..KeyBindings::default()
        };
        let (rebound, _) = bindings::resolve(&keys_table, &table);
        let pad = Signs::new(Shown::Xbox, &rebound);
        assert_eq!(
            pad.of_sprite(16, false, false).unwrap().texel(1, 5).0,
            XBOX_B
        );
        assert!(
            Signs::new(Shown::Keyboard, &rebound)
                .of_sprite(94, false, false)
                .is_none()
        );
    }

    #[test]
    fn the_confirm_button_of_the_hint_is_drawn_in_every_style() {
        let bindings = Bindings::default();
        // Space, as the line of help draws it; A of an Xbox pad; the console's blue cross
        // on a dark button.
        let key = confirm_glyph(Shown::Keyboard, &bindings);
        assert_eq!((key.wide, key.high), (23, 12));
        let a = confirm_glyph(Shown::Xbox, &bindings);
        assert_eq!(a.texel(1, 5).0, XBOX_A);
        let cross = confirm_glyph(Shown::PlayStation, &bindings);
        assert_eq!((cross.wide, cross.high), (12, 12));
        assert_eq!(cross.texel(5, 5).0, PS_CROSS);
        assert_eq!(cross.texel(2, 2).0, PS_CROSS);
        assert_eq!(cross.texel(8, 2).0, PS_CROSS);
        assert_eq!(cross.texel(5, 3).0, PS_FACE);
        // A cross bound to no key is the mouse's left button.
        let keys = KeyBindings {
            cross: Vec::new(),
            ..KeyBindings::default()
        };
        let (rebound, _) = bindings::resolve(&keys, &PadBindings::default());
        let mouse = confirm_glyph(Shown::Keyboard, &rebound);
        assert_eq!(mouse.texel(2, 2).0, MOUSE_LIT);
    }

    #[test]
    fn a_sign_in_a_text_is_put_over_the_middle_of_its_glyph() {
        // A font of the texts whose code 172 is the circle and 173 the cross.
        let mut table = vec![0x4082; 180];
        table[171] = 0x9B81;
        table[172] = 0x7E81;
        let font = Font {
            image: 18,
            columns: 18,
            cell: (14, 14),
            palette: (768, 258),
            page: 180,
            last: 180,
            table,
            limits: [0; 12],
        };
        let keys = Signs::new(Shown::Keyboard, &Bindings::default());
        // The circle is R: a cap of 10 by 12, over the glyph's middle (5, 7) of its cell.
        let (glyph, at) = in_text(Some(&keys), &font, 172, (100, 50)).unwrap();
        assert_eq!((glyph.wide, glyph.high), (10, 12));
        assert_eq!(at, (100, 51));
        // The cross is SPC, 13 wide: it reaches a little past either side of the glyph.
        let (glyph, at) = in_text(Some(&keys), &font, 173, (100, 50)).unwrap();
        assert_eq!((glyph.wide, at), (13, (99, 51)));
        // A letter is no sign, and the game's own signs need nothing.
        assert!(in_text(Some(&keys), &font, 30, (100, 50)).is_none());
        assert!(in_text(None, &font, 172, (100, 50)).is_none());
    }

    #[test]
    fn a_picture_is_drawn_whole_or_half_over_the_screen() {
        let mut screen = vec![0u8; crate::name_entry::SCREEN.0 * crate::name_entry::SCREEN.1 * 4];
        let glyph = round("A", XBOX_A, WHITE);
        put(&mut screen, &glyph, (10, 20), crate::name_entry::WHOLE);
        let pixel = |x: usize, y: usize| {
            let at = (y * crate::name_entry::SCREEN.0 + x) * 4;
            [screen[at], screen[at + 1], screen[at + 2], screen[at + 3]]
        };
        assert_eq!(pixel(10 + 1, 20 + 5)[3], 255);
        assert_eq!(pixel(10 + 11, 20 + 11)[3], 0);
        // The letter in the middle is white.
        assert!(pixel(10 + 5, 20 + 2)[0] > 200);
    }

    #[test]
    #[ignore]
    fn every_sign_of_the_lines_of_help_is_known() {
        use jc_disc::{Archive, DiscImage, Executable, LocalConfig};
        use jc_formats::battle::{menu_panel, results};
        use jc_formats::merge_screen::{MERGE_FILE, MERGE_FILE_BASE};
        use jc_formats::options_screen::{OPTIONS_FILE, OPTIONS_FILE_BASE};
        use jc_formats::player_menu::{MENU_FILE, MENU_FILE_BASE};
        use jc_formats::player_panel::{self, help_list};
        use jc_formats::{card_screen, merge_panel, options_panel, save, text};

        let config = LocalConfig::load().unwrap();
        let mut disc = DiscImage::open(&config.disc_image).unwrap();
        let archive = Archive::open(&mut disc).unwrap();
        let exe = Executable::read(&mut disc).unwrap();
        let mut signs = Vec::new();
        let mut lists = |image: &[u8], base: u32, addresses: &[u32]| {
            for &address in addresses {
                let list = help_list(image, base, address).unwrap();
                signs.extend(list.into_iter().map(|(sprite, _)| sprite));
            }
        };
        lists(
            &exe.image,
            exe.base,
            &[player_panel::HELP_PLAIN, player_panel::HELP_OF_STORY],
        );
        let menu = archive.read(&mut disc, MENU_FILE).unwrap();
        lists(
            &menu,
            MENU_FILE_BASE,
            &[
                player_panel::HELP_OF_A_ROW,
                player_panel::HELP_OF_A_ROW_IN_A_FOREST,
                player_panel::HELP_OF_THIRD_ROW_IN_A_FOREST,
                player_panel::HELP_LEAVE,
                player_panel::HELP_OF_ITEMS,
                player_panel::HELP_OF_LISTS,
                results::HELP,
            ],
        );
        let options = archive.read(&mut disc, OPTIONS_FILE).unwrap();
        lists(&options, OPTIONS_FILE_BASE, &[options_panel::LEGEND]);
        let fight = archive.read(&mut disc, menu_panel::FILE).unwrap();
        let tables = menu_panel::Tables::from_file(&fight, menu_panel::FILE_BASE).unwrap();
        lists(&exe.image, exe.base, &tables.legends);
        let merge = archive.read(&mut disc, MERGE_FILE).unwrap();
        lists(
            &merge,
            MERGE_FILE_BASE,
            &[
                merge_panel::LEGEND_OF_A_PICK,
                merge_panel::LEGEND_OF_THE_SPIN,
                merge_panel::LEGEND_OF_A_SPIN,
                merge_panel::LEGEND_OF_AN_EXCHANGE,
                merge_panel::LEGEND_OF_THE_VIEW,
                merge_panel::LEGEND_OF_A_PAGE,
                merge_panel::LEGEND_OF_ROWS,
                merge_panel::LEGEND_OF_A_SECOND,
                merge_panel::LEGEND_OF_THE_NEW,
                merge_panel::LEGEND_OF_MORE,
                merge_panel::LEGEND_OF_THE_FORM,
            ],
        );
        let card = archive.read(&mut disc, save::OVERLAY).unwrap();
        let card = card_screen::Tables::from_file(&card, save::BASE).unwrap();
        signs.extend(card.legend.iter().map(|(sprite, _)| *sprite as usize));
        let entry = archive.read(&mut disc, name::OVERLAY).unwrap();
        let entry = name::Tables::from_overlay(&entry).unwrap();
        signs.extend(entry.legend.iter().map(|(sprite, _)| *sprite));
        for sign in signs {
            assert!(
                SPRITES.iter().any(|(sprite, _)| *sprite == sign),
                "sprite {sign} of a line of help"
            );
        }
        let fonts = text::Fonts::from_executable(&exe.image, exe.base).unwrap();
        let font = fonts.get(text::TEXT_FONT).unwrap();
        let codes: Vec<u16> = UNITS
            .iter()
            .map(|(unit, _)| font.codes(&[*unit])[0])
            .collect();
        assert_eq!(codes, [172, 173, 174, 175]);
        // The pool's signs are 12 by 12, L1 to R2 12 by 8.
        let set =
            jc_formats::tim::texture_set(&archive.read(&mut disc, name::SPRITES).unwrap()).unwrap();
        for (sprite, action) in SPRITES {
            let size = (set[sprite].width, set[sprite].height);
            let high = if action.button()
                & (jc_formats::pad::L1
                    | jc_formats::pad::L2
                    | jc_formats::pad::R1
                    | jc_formats::pad::R2)
                != 0
            {
                8
            } else {
                12
            };
            assert_eq!(size, (12, high), "sprite {sprite}");
        }
    }
}
