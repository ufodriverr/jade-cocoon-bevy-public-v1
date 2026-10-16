
use bevy::input::gamepad::GamepadButton;
use bevy::prelude::KeyCode;
use jc_formats::pad;
use serde::{Deserialize, Serialize};

/// What a key or a button does: one of the console pad's buttons, or one of the build's
/// own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    Up,
    Down,
    Left,
    Right,
    Cross,
    Circle,
    Triangle,
    Square,
    Start,
    Select,
    L1,
    R1,
    L2,
    R2,
    /// The menu over a room: the circle in a room, the triangle in the menu.
    Menu,
    /// Held: a run is a walk, or a walk a run (`always_run`).
    Run,
    /// Pressed: the same, until it is pressed again (`run_toggle`).
    RunToggle,
    /// Held: the clock runs fast (`pace.rs`).
    FastForward,
}

impl Action {
    pub const ALL: [Action; 18] = [
        Action::Up,
        Action::Down,
        Action::Left,
        Action::Right,
        Action::Cross,
        Action::Circle,
        Action::Triangle,
        Action::Square,
        Action::Start,
        Action::Select,
        Action::L1,
        Action::R1,
        Action::L2,
        Action::R2,
        Action::Menu,
        Action::Run,
        Action::RunToggle,
        Action::FastForward,
    ];

    /// The console pad's button, 0 for an action of the build.
    pub fn button(self) -> u16 {
        match self {
            Action::Up => pad::UP,
            Action::Down => pad::DOWN,
            Action::Left => pad::LEFT,
            Action::Right => pad::RIGHT,
            Action::Cross => pad::CROSS,
            Action::Circle => pad::CIRCLE,
            Action::Triangle => pad::TRIANGLE,
            Action::Square => pad::SQUARE,
            Action::Start => pad::START,
            Action::Select => pad::SELECT,
            Action::L1 => pad::L1,
            Action::R1 => pad::R1,
            Action::L2 => pad::L2,
            Action::R2 => pad::R2,
            Action::Menu | Action::Run | Action::RunToggle | Action::FastForward => 0,
        }
    }

    /// The key of the action in the tables.
    pub fn name(self) -> &'static str {
        match self {
            Action::Up => "up",
            Action::Down => "down",
            Action::Left => "left",
            Action::Right => "right",
            Action::Cross => "cross",
            Action::Circle => "circle",
            Action::Triangle => "triangle",
            Action::Square => "square",
            Action::Start => "start",
            Action::Select => "select",
            Action::L1 => "l1",
            Action::R1 => "r1",
            Action::L2 => "l2",
            Action::R2 => "r2",
            Action::Menu => "menu",
            Action::Run => "run",
            Action::RunToggle => "run_toggle",
            Action::FastForward => "fast_forward",
        }
    }

    /// The default keys of the keyboard (`keys.rs`).
    fn keys(self) -> &'static [KeyCode] {
        use KeyCode::*;
        match self {
            Action::Up => &[KeyW, ArrowUp],
            Action::Down => &[KeyS, ArrowDown],
            Action::Left => &[KeyA, ArrowLeft],
            Action::Right => &[KeyD, ArrowRight],
            // The four, then those of the user's first layout (J K L I, as they lie on
            // the pad), those from before it (X, Backspace) and the numbers' block.
            Action::Cross => &[Space, KeyF, KeyK, KeyX, Numpad2],
            Action::Circle => &[KeyR, KeyL, Numpad6],
            Action::Triangle => &[Escape, Backspace, KeyI, Numpad8],
            Action::Square => &[KeyC, KeyJ],
            Action::Start => &[Enter, NumpadEnter],
            Action::Select => &[Tab],
            Action::L1 => &[KeyQ, KeyU],
            Action::R1 => &[KeyE],
            Action::L2 => &[KeyZ, KeyO],
            Action::R2 => &[KeyV],
            Action::Menu => &[KeyI, Tab],
            Action::Run => &[ShiftLeft, ShiftRight],
            Action::RunToggle => &[CapsLock],
            Action::FastForward => &[ControlLeft],
        }
    }

    /// The default buttons of a gamepad, in an Xbox pad's places: A confirms and B backs
    /// out, as on a PC; Y is the circle, which opens the menu over a room as on the
    /// console. The right trigger is R2 on a screen of menus and the fast forward
    /// everywhere else ([`on_screen`]).
    fn buttons(self) -> &'static [GamepadButton] {
        use GamepadButton::*;
        match self {
            Action::Up => &[DPadUp],
            Action::Down => &[DPadDown],
            Action::Left => &[DPadLeft],
            Action::Right => &[DPadRight],
            Action::Cross => &[South],
            Action::Circle => &[North],
            Action::Triangle => &[East],
            Action::Square => &[West],
            Action::Start => &[Start],
            Action::Select => &[Select],
            Action::L1 => &[LeftTrigger],
            Action::R1 => &[RightTrigger],
            Action::L2 => &[LeftTrigger2],
            Action::R2 => &[RightTrigger2],
            Action::FastForward => &[RightTrigger2],
            Action::Menu | Action::Run => &[],
            Action::RunToggle => &[LeftThumb],
        }
    }
}

/// The keys of the keyboard whose names the tables know.
const NAMED_KEYS: &[KeyCode] = {
    use KeyCode::*;
    &[
        KeyA,
        KeyB,
        KeyC,
        KeyD,
        KeyE,
        KeyF,
        KeyG,
        KeyH,
        KeyI,
        KeyJ,
        KeyK,
        KeyL,
        KeyM,
        KeyN,
        KeyO,
        KeyP,
        KeyQ,
        KeyR,
        KeyS,
        KeyT,
        KeyU,
        KeyV,
        KeyW,
        KeyX,
        KeyY,
        KeyZ,
        Digit0,
        Digit1,
        Digit2,
        Digit3,
        Digit4,
        Digit5,
        Digit6,
        Digit7,
        Digit8,
        Digit9,
        Numpad0,
        Numpad1,
        Numpad2,
        Numpad3,
        Numpad4,
        Numpad5,
        Numpad6,
        Numpad7,
        Numpad8,
        Numpad9,
        NumpadAdd,
        NumpadSubtract,
        NumpadMultiply,
        NumpadDivide,
        NumpadDecimal,
        NumpadEnter,
        F1,
        F2,
        F3,
        F4,
        F5,
        F6,
        F7,
        F8,
        F9,
        F10,
        F11,
        F12,
        ArrowUp,
        ArrowDown,
        ArrowLeft,
        ArrowRight,
        Space,
        Enter,
        Tab,
        Escape,
        Backspace,
        ShiftLeft,
        ShiftRight,
        ControlLeft,
        ControlRight,
        AltLeft,
        AltRight,
        CapsLock,
        Insert,
        Delete,
        Home,
        End,
        PageUp,
        PageDown,
        Minus,
        Equal,
        BracketLeft,
        BracketRight,
        Backslash,
        Semicolon,
        Quote,
        Comma,
        Period,
        Slash,
        Backquote,
    ]
};

/// The buttons of a gamepad with their Xbox names.
const NAMED_BUTTONS: &[(&str, GamepadButton)] = {
    use GamepadButton::*;
    &[
        ("A", South),
        ("B", East),
        ("X", West),
        ("Y", North),
        ("LB", LeftTrigger),
        ("RB", RightTrigger),
        ("LT", LeftTrigger2),
        ("RT", RightTrigger2),
        ("Back", Select),
        ("Start", Start),
        ("Guide", Mode),
        ("LS", LeftThumb),
        ("RS", RightThumb),
        ("DPadUp", DPadUp),
        ("DPadDown", DPadDown),
        ("DPadLeft", DPadLeft),
        ("DPadRight", DPadRight),
        ("C", C),
        ("Z", Z),
    ]
};

/// The name of a key as the tables write it.
pub fn key_name(key: KeyCode) -> String {
    format!("{key:?}")
}

/// The key of a name: Bevy's name, or a letter or a digit alone, or `Esc`.
pub fn key_named(name: &str) -> Option<KeyCode> {
    let name = name.trim();
    let alias = match name.to_ascii_lowercase().as_str() {
        "esc" => Some(KeyCode::Escape),
        "return" => Some(KeyCode::Enter),
        _ => None,
    };
    alias.or_else(|| {
        (NAMED_KEYS.iter().copied()).find(|key| {
            let own = key_name(*key);
            let short = (own.strip_prefix("Key"))
                .or_else(|| own.strip_prefix("Digit"))
                .unwrap_or(&own);
            own.eq_ignore_ascii_case(name) || short.eq_ignore_ascii_case(name)
        })
    })
}

/// The name of a gamepad's button as the tables write it.
pub fn button_name(button: GamepadButton) -> String {
    (NAMED_BUTTONS.iter())
        .find(|(_, named)| *named == button)
        .map_or_else(|| format!("{button:?}"), |(name, _)| (*name).to_owned())
}

/// The button of a name: its Xbox name or Bevy's.
pub fn button_named(name: &str) -> Option<GamepadButton> {
    let name = name.trim();
    (NAMED_BUTTONS.iter())
        .find(|(named, button)| {
            named.eq_ignore_ascii_case(name) || format!("{button:?}").eq_ignore_ascii_case(name)
        })
        .map(|(_, button)| *button)
}

/// Whether the tables know a key by a name.
pub fn is_named_key(key: KeyCode) -> bool {
    NAMED_KEYS.contains(&key)
}

/// The gamepad's buttons the tables know by a name.
pub fn named_buttons() -> impl Iterator<Item = GamepadButton> {
    NAMED_BUTTONS.iter().map(|(_, button)| *button)
}

/// A table of names, one list an action: `[controls.keys]` or `[controls.pad]`.
pub trait Table {
    fn names(&self, action: Action) -> &Vec<String>;
    fn names_mut(&mut self, action: Action) -> &mut Vec<String>;
}

/// Whether two actions share a key or a button on purpose, as the defaults have it: the
/// menu key is the circle in a room and the triangle in the menu (I is the triangle
/// besides, Tab Select), and the fast forward is R2 on a screen of menus only (RT,
/// [`on_screen`]). Every other two actions are one context: a key of both would press
/// both.
pub fn share_on_purpose(one: Action, other: Action) -> bool {
    let pair = |a: Action, b: Action| (one == a && other == b) || (one == b && other == a);
    pair(Action::Menu, Action::Triangle)
        || pair(Action::Menu, Action::Select)
        || pair(Action::Menu, Action::Circle)
        || pair(Action::FastForward, Action::R2)
}

/// What a new key or button of an action did to another action that had it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Taken {
    /// The action it was taken from.
    pub from: Action,
    /// The key or button that action got in its place: the one the new one pushed out.
    pub gets: Option<String>,
}

/// Gives `action` the key or button `name` as its first one, the one the prompts show
/// (the PC Settings screen, `pc_settings.rs`). The first one it had goes, the others
/// stay. Another action of the same context that had the new one loses it; when it was
/// that action's first, it gets the one `action` gave up in its place (a swap), so that
/// nothing is left without a key. `of` reads a name as the key or button it is, so that
/// `W` and `KeyW` are the same.
pub fn rebind<T: Table, I: PartialEq>(
    table: &mut T,
    action: Action,
    name: &str,
    of: impl Fn(&str) -> Option<I>,
) -> Vec<Taken> {
    let Some(new) = of(name) else {
        return Vec::new();
    };
    let is_new = |other: &String| of(other).as_ref() == Some(&new);
    let names = table.names_mut(action);
    let mut given_up = names.first().filter(|first| !is_new(first)).cloned();
    let rest: Vec<String> = (names.iter().skip(1))
        .filter(|other| !is_new(other))
        .cloned()
        .collect();
    *names = std::iter::once(name.to_owned()).chain(rest).collect();
    let mut taken = Vec::new();
    for other in Action::ALL {
        if other == action || share_on_purpose(action, other) {
            continue;
        }
        // The one `action` gave up may still be another action's that shares it with
        // `action` on purpose (Tab of the menu key, RT of the fast forward): handed on,
        // it would press two actions of one context at once.
        let elsewhere = |old: &String| {
            (Action::ALL.iter())
                .filter(|&&holder| holder != other && holder != action)
                .any(|&holder| table.names(holder).iter().any(|name| of(name) == of(old)))
        };
        let shared = given_up.as_ref().is_some_and(elsewhere);
        let names = table.names_mut(other);
        if !names.iter().any(is_new) {
            continue;
        }
        let was_first = names.first().is_some_and(is_new);
        names.retain(|name| !is_new(name));
        // Left with nothing, the action takes it even so: a key of two is better than none.
        let gets = given_up
            .take_if(|_| was_first)
            .filter(|_| !shared || names.is_empty())
            .filter(|old| !names.iter().any(|name| of(name) == of(old)));
        if let Some(old) = &gets {
            names.insert(0, old.clone());
        }
        taken.push(Taken { from: other, gets });
    }
    taken
}

/// A table of names, one list an action: `[controls.keys]` or `[controls.pad]`.
macro_rules! table {
    ($(#[$what:meta])* $name:ident, $default:expr) => {
        $(#[$what])*
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(default)]
        pub struct $name {
            pub up: Vec<String>,
            pub down: Vec<String>,
            pub left: Vec<String>,
            pub right: Vec<String>,
            pub cross: Vec<String>,
            pub circle: Vec<String>,
            pub triangle: Vec<String>,
            pub square: Vec<String>,
            pub start: Vec<String>,
            pub select: Vec<String>,
            pub l1: Vec<String>,
            pub r1: Vec<String>,
            pub l2: Vec<String>,
            pub r2: Vec<String>,
            pub menu: Vec<String>,
            pub run: Vec<String>,
            pub run_toggle: Vec<String>,
            pub fast_forward: Vec<String>,
        }

        impl Default for $name {
            fn default() -> Self {
                let names: fn(Action) -> Vec<String> = $default;
                Self {
                    up: names(Action::Up),
                    down: names(Action::Down),
                    left: names(Action::Left),
                    right: names(Action::Right),
                    cross: names(Action::Cross),
                    circle: names(Action::Circle),
                    triangle: names(Action::Triangle),
                    square: names(Action::Square),
                    start: names(Action::Start),
                    select: names(Action::Select),
                    l1: names(Action::L1),
                    r1: names(Action::R1),
                    l2: names(Action::L2),
                    r2: names(Action::R2),
                    menu: names(Action::Menu),
                    run: names(Action::Run),
                    run_toggle: names(Action::RunToggle),
                    fast_forward: names(Action::FastForward),
                }
            }
        }

        impl Table for $name {
            fn names(&self, action: Action) -> &Vec<String> {
                match action {
                    Action::Up => &self.up,
                    Action::Down => &self.down,
                    Action::Left => &self.left,
                    Action::Right => &self.right,
                    Action::Cross => &self.cross,
                    Action::Circle => &self.circle,
                    Action::Triangle => &self.triangle,
                    Action::Square => &self.square,
                    Action::Start => &self.start,
                    Action::Select => &self.select,
                    Action::L1 => &self.l1,
                    Action::R1 => &self.r1,
                    Action::L2 => &self.l2,
                    Action::R2 => &self.r2,
                    Action::Menu => &self.menu,
                    Action::Run => &self.run,
                    Action::RunToggle => &self.run_toggle,
                    Action::FastForward => &self.fast_forward,
                }
            }

            fn names_mut(&mut self, action: Action) -> &mut Vec<String> {
                match action {
                    Action::Up => &mut self.up,
                    Action::Down => &mut self.down,
                    Action::Left => &mut self.left,
                    Action::Right => &mut self.right,
                    Action::Cross => &mut self.cross,
                    Action::Circle => &mut self.circle,
                    Action::Triangle => &mut self.triangle,
                    Action::Square => &mut self.square,
                    Action::Start => &mut self.start,
                    Action::Select => &mut self.select,
                    Action::L1 => &mut self.l1,
                    Action::R1 => &mut self.r1,
                    Action::L2 => &mut self.l2,
                    Action::R2 => &mut self.r2,
                    Action::Menu => &mut self.menu,
                    Action::Run => &mut self.run,
                    Action::RunToggle => &mut self.run_toggle,
                    Action::FastForward => &mut self.fast_forward,
                }
            }
        }

        impl $name {
            /// Every action with its names.
            pub fn each(&self) -> [(Action, &[String]); 18] {
                [
                    (Action::Up, &self.up),
                    (Action::Down, &self.down),
                    (Action::Left, &self.left),
                    (Action::Right, &self.right),
                    (Action::Cross, &self.cross),
                    (Action::Circle, &self.circle),
                    (Action::Triangle, &self.triangle),
                    (Action::Square, &self.square),
                    (Action::Start, &self.start),
                    (Action::Select, &self.select),
                    (Action::L1, &self.l1),
                    (Action::R1, &self.r1),
                    (Action::L2, &self.l2),
                    (Action::R2, &self.r2),
                    (Action::Menu, &self.menu),
                    (Action::Run, &self.run),
                    (Action::RunToggle, &self.run_toggle),
                    (Action::FastForward, &self.fast_forward),
                ]
            }
        }
    };
}

table!(
    /// `[controls.keys]`: the keys of every action.
    KeyBindings,
    |action| action.keys().iter().copied().map(key_name).collect()
);

table!(
    /// `[controls.pad]`: the gamepad's buttons of every action.
    PadBindings,
    |action| action.buttons().iter().copied().map(button_name).collect()
);

/// The tables as the pad of `keys.rs` reads them: every key and every button with its
/// action, a key of two actions twice.
#[derive(Debug, Clone, PartialEq)]
pub struct Bindings {
    pub keys: Vec<(KeyCode, Action)>,
    pub buttons: Vec<(GamepadButton, Action)>,
}

impl Default for Bindings {
    fn default() -> Self {
        resolve(&KeyBindings::default(), &PadBindings::default()).0
    }
}

impl Bindings {
    /// The keys of an action.
    pub fn keys_of(&self, action: Action) -> impl Iterator<Item = KeyCode> + '_ {
        (self.keys.iter())
            .filter(move |(_, of)| *of == action)
            .map(|(key, _)| *key)
    }
}

/// What the keys or the buttons of a table do on the screen that is up: a key or a
/// button of the fast forward that is a button of the pad too (RT, which is R2) is that
/// button on a screen of menus (`menus`) and the fast forward elsewhere; every other one
/// does all it is bound to.
pub fn on_screen<T: Copy + PartialEq>(bound: &[(T, Action)], menus: bool) -> Vec<(T, Action)> {
    (bound.iter())
        .copied()
        .filter(|&(input, action)| {
            !shared(bound, input)
                || match action {
                    Action::FastForward => !menus,
                    action if action.button() != 0 => menus,
                    _ => true,
                }
        })
        .collect()
}

/// Whether a key or a button is the fast forward and a button of the pad at once.
pub fn shared<T: Copy + PartialEq>(bound: &[(T, Action)], input: T) -> bool {
    let of = |wanted: fn(Action) -> bool| {
        (bound.iter()).any(|&(other, action)| other == input && wanted(action))
    };
    of(|action| action == Action::FastForward) && of(|action| action.button() != 0)
}

/// The bindings of the two tables, and what in them the game does not know.
pub fn resolve(keys: &KeyBindings, buttons: &PadBindings) -> (Bindings, Vec<String>) {
    let mut unknown = Vec::new();
    let mut bindings = Bindings {
        keys: Vec::new(),
        buttons: Vec::new(),
    };
    for (action, names) in keys.each() {
        for name in names {
            match key_named(name) {
                Some(key) => bindings.keys.push((key, action)),
                None => unknown.push(format!(
                    "[controls.keys] {}: no key {name:?}",
                    action.name()
                )),
            }
        }
    }
    for (action, names) in buttons.each() {
        for name in names {
            match button_named(name) {
                Some(button) => bindings.buttons.push((button, action)),
                None => unknown.push(format!(
                    "[controls.pad] {}: no button {name:?}",
                    action.name()
                )),
            }
        }
    }
    (bindings, unknown)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No key and no button is two of the pad's buttons, nor a button and one of the
    /// build's own actions; only the menu keys are a second thing on purpose (I is the
    /// triangle, Tab Select), and RT, which is R2 and the fast forward.
    #[test]
    fn no_key_is_two_buttons() {
        let bindings = Bindings::default();
        let keys: Vec<_> = (bindings.keys.iter())
            .filter(|(_, action)| *action != Action::Menu)
            .collect();
        for (number, (key, action)) in keys.iter().enumerate() {
            assert!(
                !keys[..number].iter().any(|(other, _)| other == key),
                "{key:?} of {action:?} is in the table twice"
            );
        }
        let buttons: Vec<_> = (bindings.buttons.iter())
            .filter(|(_, action)| *action != Action::FastForward)
            .collect();
        for (number, (button, _)) in buttons.iter().enumerate() {
            assert!(
                !buttons[..number].iter().any(|(other, _)| other == button),
                "{button:?} is in the table twice"
            );
        }
        assert!(
            (bindings.buttons.iter())
                .filter(|(_, action)| *action == Action::FastForward)
                .all(|(button, _)| *button == GamepadButton::RightTrigger2)
        );
        // The menu keys are the triangle and Select besides.
        assert!(bindings.keys.contains(&(KeyCode::KeyI, Action::Triangle)));
        assert!(bindings.keys.contains(&(KeyCode::Tab, Action::Select)));
    }

    #[test]
    fn rt_is_r2_in_the_menus_and_the_fast_forward_elsewhere() {
        let bindings = Bindings::default();
        let rt = |menus: bool| {
            (on_screen(&bindings.buttons, menus).into_iter())
                .filter(|(button, _)| *button == GamepadButton::RightTrigger2)
                .map(|(_, action)| action)
                .collect::<Vec<_>>()
        };
        assert_eq!(rt(true), [Action::R2]);
        assert_eq!(rt(false), [Action::FastForward]);
        // Left Ctrl is the fast forward alone, everywhere; V is R2 alone.
        for menus in [false, true] {
            let keys = on_screen(&bindings.keys, menus);
            assert!(keys.contains(&(KeyCode::ControlLeft, Action::FastForward)));
            assert!(keys.contains(&(KeyCode::KeyV, Action::R2)));
            assert_eq!(keys.len(), bindings.keys.len());
        }
        // RT is one of its two actions; the pad's other buttons are all there.
        for menus in [false, true] {
            let buttons = on_screen(&bindings.buttons, menus);
            assert_eq!(buttons.len(), bindings.buttons.len() - 1);
        }
    }

    #[test]
    fn every_default_name_is_known() {
        let (_, unknown) = resolve(&KeyBindings::default(), &PadBindings::default());
        assert_eq!(unknown, Vec::<String>::new());
        let names = Action::ALL.map(Action::name);
        let each = KeyBindings::default();
        assert_eq!(each.each().map(|(action, _)| action.name()), names);
    }

    #[test]
    fn a_new_key_is_the_first_and_another_action_gives_it_up_or_swaps() {
        let mut keys = KeyBindings::default();
        // Cross's own second key: no other action is touched, Space goes.
        assert_eq!(rebind(&mut keys, Action::Cross, "KeyF", key_named), []);
        assert_eq!(keys.cross, ["KeyF", "KeyK", "KeyX", "Numpad2"]);
        // C is the square's first: the square gets F, the key C pushed out of the cross.
        let taken = rebind(&mut keys, Action::Cross, "C", key_named);
        assert_eq!(
            taken,
            [Taken {
                from: Action::Square,
                gets: Some("KeyF".into())
            }]
        );
        assert_eq!(keys.cross, ["C", "KeyK", "KeyX", "Numpad2"]);
        assert_eq!(keys.square, ["KeyF", "KeyJ"]);
        // A second key of another action is only taken away.
        let taken = rebind(&mut keys, Action::Square, "KeyK", key_named);
        assert_eq!(
            taken,
            [Taken {
                from: Action::Cross,
                gets: None
            }]
        );
        assert_eq!(keys.cross, ["C", "KeyX", "Numpad2"]);
        assert_eq!(keys.square, ["KeyK", "KeyJ"]);
        // I is the triangle and the menu key on purpose: the menu keeps it.
        let taken = rebind(&mut keys, Action::Triangle, "KeyI", key_named);
        assert_eq!(taken, []);
        assert_eq!(keys.menu, ["KeyI", "Tab"]);
        // The pad has one button an action: B and A swap.
        let mut pad = PadBindings::default();
        let taken = rebind(&mut pad, Action::Cross, "B", button_named);
        assert_eq!(
            taken,
            [Taken {
                from: Action::Triangle,
                gets: Some("A".into())
            }]
        );
        assert_eq!(
            (pad.cross, pad.triangle),
            (vec!["B".into()], vec!["A".into()])
        );
        // RT stays R2 and the fast forward.
        let mut pad = PadBindings::default();
        assert_eq!(
            rebind(&mut pad, Action::FastForward, "RT", button_named),
            []
        );
        assert_eq!(pad.r2, ["RT"]);
        // A name the game does not know changes nothing.
        let before = PadBindings::default();
        let mut pad = before.clone();
        assert_eq!(rebind(&mut pad, Action::Cross, "Kazoo", button_named), []);
        assert_eq!(pad, before);
    }

    #[test]
    fn a_swap_hands_on_no_key_another_action_still_has() {
        // Select gives up Tab, which the menu key keeps: the cross, which gave up Space,
        // does not get Tab beside it, or Tab would open the menu and confirm at once.
        let mut keys = KeyBindings::default();
        let taken = rebind(&mut keys, Action::Select, "Space", key_named);
        assert_eq!(
            taken,
            [Taken {
                from: Action::Cross,
                gets: None
            }]
        );
        assert_eq!(keys.cross, ["KeyF", "KeyK", "KeyX", "Numpad2"]);
        assert_eq!(keys.menu, ["KeyI", "Tab"]);
        // The menu key gives up I, which the triangle keeps.
        let mut keys = KeyBindings::default();
        rebind(&mut keys, Action::Menu, "Space", key_named);
        assert!(!keys.cross.iter().any(|name| name == "KeyI"));
        // R2 gives up RT, which the fast forward keeps; the cross would have no button
        // left, so it takes RT even so.
        let mut pad = PadBindings::default();
        let taken = rebind(&mut pad, Action::R2, "A", button_named);
        assert_eq!(
            taken,
            [Taken {
                from: Action::Cross,
                gets: Some("RT".into())
            }]
        );
    }

    #[test]
    fn names_of_keys_and_buttons() {
        assert_eq!(key_named("KeyW"), Some(KeyCode::KeyW));
        assert_eq!(key_named("w"), Some(KeyCode::KeyW));
        assert_eq!(key_named("1"), Some(KeyCode::Digit1));
        assert_eq!(key_named("space"), Some(KeyCode::Space));
        assert_eq!(key_named("Esc"), Some(KeyCode::Escape));
        assert_eq!(key_named("capslock"), Some(KeyCode::CapsLock));
        assert_eq!(key_named("Banana"), None);
        assert_eq!(key_name(KeyCode::ArrowUp), "ArrowUp");
        assert_eq!(button_named("a"), Some(GamepadButton::South));
        assert_eq!(button_named("RT"), Some(GamepadButton::RightTrigger2));
        assert_eq!(
            button_named("LeftTrigger2"),
            Some(GamepadButton::LeftTrigger2)
        );
        assert_eq!(button_named("Square"), None);
        assert_eq!(button_name(GamepadButton::Select), "Back");
        for (name, button) in NAMED_BUTTONS {
            assert_eq!(button_named(&button_name(*button)), Some(*button), "{name}");
        }
    }

    #[test]
    fn a_table_of_the_settings_changes_what_it_names() {
        let text = "cross = [\"Enter\", \"Banana\"]\nmenu = []\n";
        let keys: KeyBindings = toml::from_str(text).unwrap();
        // What is left out keeps its default.
        assert_eq!(keys.up, KeyBindings::default().up);
        let pad: PadBindings = toml::from_str("circle = [\"RB\"]\nl1 = [\"Kazoo\"]").unwrap();
        let (bindings, unknown) = resolve(&keys, &pad);
        assert_eq!(
            unknown,
            [
                "[controls.keys] cross: no key \"Banana\"",
                "[controls.pad] l1: no button \"Kazoo\""
            ]
        );
        assert_eq!(
            bindings.keys_of(Action::Cross).collect::<Vec<_>>(),
            [KeyCode::Enter]
        );
        assert_eq!(bindings.keys_of(Action::Menu).count(), 0);
        assert!(
            bindings
                .buttons
                .contains(&(GamepadButton::RightTrigger, Action::Circle))
        );
        assert!(
            !bindings
                .buttons
                .iter()
                .any(|(_, action)| *action == Action::L1)
        );
        // The defaults are written out in full.
        let written = toml::to_string_pretty(&PadBindings::default()).unwrap();
        assert!(written.contains("cross = [\"A\"]"), "{written}");
        assert!(written.contains("run_toggle = [\"LS\"]"), "{written}");
    }
}
