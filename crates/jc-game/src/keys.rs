
use bevy::ecs::system::SystemParam;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorOptions, PrimaryWindow};
use jc_disc::{Controls, LocalConfig, Movement};
use jc_formats::pad;

use crate::bindings::{Action, Bindings};
use crate::gamepad::{self, Device, InputDevice, Pads};
use crate::prefs::{ControlPrefs, Prefs};

const MOUSE: [(MouseButton, u16); 5] = [
    (MouseButton::Left, pad::CROSS),
    (MouseButton::Right, pad::TRIANGLE),
    (MouseButton::Middle, pad::CIRCLE),
    (MouseButton::Back, pad::L1),
    (MouseButton::Forward, pad::R1),
];

/// The keys of Enter, which with Alt are the switch of `display.rs` and not Start.
const ENTER: [KeyCode; 2] = [KeyCode::Enter, KeyCode::NumpadEnter];

/// The four directions of the pad's word.
pub const DIRECTIONS: u16 = pad::UP | pad::DOWN | pad::LEFT | pad::RIGHT;
/// Game frames a notch of the wheel holds its direction, and game frames with it
/// released behind that, so that the next notch is a press of its own.
const NOTCH_HELD: u8 = 2;
const NOTCH_GAP: u8 = 1;
/// The most notches that wait: a fast spin does not run on long after the wheel stops.
const NOTCHES_WAITING: i32 = 3;
/// Seconds the mouse may rest in the window before the cursor hides.
const CURSOR_REST: f32 = 2.5;

/// The pad of the keyboard, the mouse and a gamepad, read once a frame in front of
/// every screen.
pub struct KeysPlugin;

impl Plugin for KeysPlugin {
    fn build(&self, app: &mut App) {
        // `[controls]` of the player's settings (`prefs.rs`), which start as the table of
        // `jc.local.toml`; that table itself where the app has no settings.
        let prefs = app.world().get_resource::<Prefs>();
        let mut controls = match prefs {
            Some(prefs) => prefs.controls.controls(),
            None => (LocalConfig::load()).map_or(Controls::default(), |config| config.controls),
        };
        // `--tank-controls` tries the pad's own steering without changing the file.
        let tank = std::env::args().any(|arg| arg == "--tank-controls");
        if tank {
            controls.movement = Movement::Tank;
        }
        info!(
            "the controls: {:?} movement, {}",
            controls.movement,
            match controls.always_run {
                true => "the directions run and Shift walks",
                false => "the directions walk and Shift runs",
            }
        );
        // `--steer=up+left` holds those directions for a hidden run, which has no keys,
        // and `--stick=X,Y` a gamepad's left stick (right and up above 0, up to 1).
        let steer = (std::env::args())
            .find_map(|arg| arg.strip_prefix("--steer=").map(steered))
            .unwrap_or(0);
        let stick = (std::env::args()).find_map(|arg| arg.strip_prefix("--stick=").map(sticked));
        app.insert_resource(Pad {
            controls,
            tank,
            steer,
            stick_held: stick.flatten(),
            ..default()
        })
        .add_systems(
            PreUpdate,
            (follow_settings, read_pad)
                .chain()
                .after(crate::pointer::read_mouse)
                .after(gamepad::read_pads),
        )
        .add_systems(FixedLast, pass_wheel)
        .add_systems(Update, hide_resting_cursor);
    }
}

/// What the keyboard, the mouse and the pad hold in this frame.
#[derive(Resource, Debug, Default)]
pub struct Pad {
    /// The table `[controls]` of the settings.
    pub controls: Controls,
    /// `--tank-controls`: the tank movement, whatever the settings say.
    tank: bool,
    /// Caps Lock turns the run round (`run_toggle`).
    run_toggle: bool,
    /// The keys and the buttons of every action, and the settings they were read from.
    bindings: Bindings,
    followed: Option<ControlPrefs>,
    /// The directions of `--steer`, held whatever the keys say.
    steer: u16,
    /// The stick of `--stick`, held whatever the pad says.
    stick_held: Option<Vec2>,
    keys: u16,
    mouse: u16,
    /// The buttons of the gamepad, and the directions its left stick holds in a menu.
    buttons: u16,
    stick_directions: u16,
    /// The left stick out of its dead zone (`gamepad::live`).
    stick: Option<Vec2>,
    /// A key or a button of the run is down.
    run: bool,
    /// Caps Lock was pressed an odd number of times.
    toggled: bool,
    /// A key or a button of the menu over a room is down.
    menu: bool,
    /// A key or a button of the fast forward is down.
    fast_forward: bool,
    /// While a key or a button that is the fast forward and a button of the pad at once
    /// (RT) is held: whether it went down on a screen of menus, where it is the pad's
    /// button. It keeps that until it is let go, so that RT held into a menu does not
    /// turn its pages, nor R2 held out of one run the clock fast.
    shared_on_menus: Option<bool>,
    /// Enter went down with Alt and is still down: it is not Start.
    alt_enter: bool,
    /// Enter went down somewhere else than on the name entry and is still down: it is
    /// the cross as well.
    enter_crosses: bool,
    wheel: Wheel,
    /// Seconds since the mouse last moved or pressed a button.
    rest: f32,
}

/// The wheel's notches on their way to the pad's word.
#[derive(Debug, Default)]
struct Wheel {
    /// Notches not passed on yet: up above 0, down below.
    waiting: i32,
    /// The direction a notch holds, and its game frames left.
    held: u16,
    frames: u8,
    gap: u8,
}

impl Pad {
    /// The pad's word of held buttons: the keys, the mouse's buttons, the wheel and the
    /// gamepad.
    pub fn held(&self) -> u16 {
        self.keys | self.mouse | self.wheel.held | self.buttons | self.stick_directions
    }

    /// The directions a room's player walks by: those of the keys and of the d-pad, not
    /// the wheel's; the left stick's too with the tank movement, which has no stick.
    pub fn directions(&self) -> u16 {
        let stick = match self.controls.movement {
            Movement::Tank => self.stick_directions,
            Movement::Modern => 0,
        };
        (self.keys | self.buttons | stick) & DIRECTIONS
    }

    /// The left stick out of its dead zone, up above 0, as far as it is pushed (up to 1).
    pub fn stick(&self) -> Option<Vec2> {
        self.stick
    }

    /// Whether a key of the menu over a room is down: the circle in a room, the
    /// triangle in the menu.
    pub fn menu_key(&self) -> bool {
        self.menu
    }

    /// Whether a walk is a run.
    pub fn runs(&self) -> bool {
        (self.run != self.controls.always_run) != (self.run_toggle && self.toggled)
    }

    /// Whether a key or a button of the fast forward is down (`fast_forward` of
    /// `[controls.keys]` and `[controls.pad]`; `pace.rs`).
    pub fn fast_forward(&self) -> bool {
        self.fast_forward
    }
}

/// The pad's word of held buttons.
pub fn held(pad: &Pad) -> u16 {
    pad.held()
}

/// The directions of `--steer=`: `up`, `down`, `left` and `right`, joined by `+`.
fn steered(value: &str) -> u16 {
    (value.split('+'))
        .map(|direction| match direction {
            "up" => pad::UP,
            "down" => pad::DOWN,
            "left" => pad::LEFT,
            "right" => pad::RIGHT,
            other => {
                warn!("--steer: no direction {other:?}");
                0
            }
        })
        .fold(0, |held, direction| held | direction)
}

/// The stick of `--stick=X,Y`.
fn sticked(value: &str) -> Option<Vec2> {
    let parts: Vec<Option<f32>> = (value.split(','))
        .map(|part| part.trim().parse().ok())
        .collect();
    match parts[..] {
        [Some(x), Some(y)] => Some(Vec2::new(x, y).clamp(Vec2::splat(-1.0), Vec2::splat(1.0))),
        _ => {
            warn!("--stick: {value:?} is not X,Y");
            None
        }
    }
}

/// Whether the player runs (the pad's run button with "forward").
pub fn fast(pad: &Pad) -> bool {
    pad.runs()
}

/// The settings' `[controls]` as they change reach the pad: the movement, the run and
/// the keys and buttons of every action. A name the game does not know is said once.
fn follow_settings(prefs: Option<Res<Prefs>>, mut pad: ResMut<Pad>) {
    let Some(prefs) = prefs else { return };
    if pad.followed.as_ref() == Some(&prefs.controls) {
        return;
    }
    let controls = &prefs.controls;
    let (bindings, unknown) = crate::bindings::resolve(&controls.keys, &controls.pad);
    for unknown in unknown {
        warn!("the settings: {unknown}; it is left out");
    }
    if pad.followed.is_some() {
        info!(
            "the controls follow the settings: {:?}",
            controls.controls()
        );
    }
    pad.controls = controls.controls();
    if pad.tank {
        pad.controls.movement = Movement::Tank;
    }
    pad.run_toggle = controls.run_toggle;
    pad.bindings = bindings;
    pad.followed = Some(controls.clone());
}

/// What the actions of the keys or of the pad's buttons hold.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Pressed {
    buttons: u16,
    run: bool,
    menu: bool,
    fast_forward: bool,
    /// A key of the run's toggle went down.
    toggle: bool,
}

impl Pressed {
    /// `down` holds and `went_down` went down in this frame.
    fn of<T: Copy>(
        bound: &[(T, Action)],
        down: impl Fn(T) -> bool,
        went_down: impl Fn(T) -> bool,
    ) -> Self {
        let mut pressed = Self::default();
        for (input, action) in bound.iter().filter(|(input, _)| down(*input)) {
            match action {
                Action::Menu => pressed.menu = true,
                Action::Run => pressed.run = true,
                Action::FastForward => pressed.fast_forward = true,
                Action::RunToggle => pressed.toggle |= went_down(*input),
                action => pressed.buttons |= action.button(),
            }
        }
        pressed
    }
}

/// The screens that read R2 (and the other shoulder buttons): the menus, and a fight's
/// command menu while it waits (R2 turns the pages of its items and its spells). A
/// button of the fast forward that is a button of the pad too (RT, which is R2) is that
/// button there, and the fast forward everywhere else ([`crate::bindings::on_screen`]).
#[derive(SystemParam)]
pub struct Menus<'w> {
    menu: Option<Res<'w, crate::player_menu::MenuScreen>>,
    shop: Option<Res<'w, crate::shop_view::ShopView>>,
    card: Option<Res<'w, crate::card_view::CardView>>,
    map: Option<Res<'w, crate::place_map::PlaceMap>>,
    things: Option<Res<'w, crate::shown::Things>>,
    lobby: Option<Res<'w, crate::lobby::Lobby>>,
}

impl Menus<'_> {
    fn up(&self, naming: bool, merge: bool) -> bool {
        naming
            || merge
            || self.menu.is_some()
            || self.shop.is_some()
            || self.card.is_some()
            || self.map.is_some()
            || self.lobby.is_some()
            || (self.things.as_ref()).is_some_and(|things| things.fight_menu_waits())
    }
}

/// The screens that may take the typed keys for a name or an address.
type Typing<'w> = (
    Option<Res<'w, crate::name_entry::NameEntry>>,
    Option<Res<'w, crate::merge_view::MergeView>>,
    Option<Res<'w, crate::lobby::Lobby>>,
);

#[allow(clippy::too_many_arguments)]
pub(crate) fn read_pad(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    pointer: Res<crate::pointer::Mouse>,
    pads: Res<Pads>,
    (entry, merge, lobby): Typing,
    menus: Menus,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut pad: ResMut<Pad>,
) {
    let pad = &mut *pad;
    let alt = keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);
    pad.alt_enter = match keys.any_pressed(ENTER) {
        true => pad.alt_enter || (alt && keys.any_just_pressed(ENTER)),
        false => false,
    };
    let alt_enter = pad.alt_enter;
    // On a name entry the keys that type are no buttons (`name_entry.rs`).
    let naming = crate::name_entry::naming(entry.as_deref(), merge.as_deref(), lobby.as_deref());
    let merging = merge.is_some_and(|merge| !merge.asleep());
    let shared_held = (pad.bindings.keys.iter())
        .any(|&(key, _)| keys.pressed(key) && crate::bindings::shared(&pad.bindings.keys, key))
        || (pad.bindings.buttons.iter()).any(|&(button, _)| {
            pads.pressed(button) && crate::bindings::shared(&pad.bindings.buttons, button)
        });
    let on_menus = match shared_held {
        true => *pad.shared_on_menus.get_or_insert(menus.up(naming, merging)),
        false => {
            pad.shared_on_menus = None;
            menus.up(naming, merging)
        }
    };
    let counts = |key: KeyCode| {
        keys.pressed(key)
            && !(alt_enter && ENTER.contains(&key))
            && !(naming && crate::name_entry::typing_keys(key))
    };
    let bound_keys = crate::bindings::on_screen(&pad.bindings.keys, on_menus);
    let from_keys = Pressed::of(&bound_keys, counts, |key| keys.just_pressed(key));
    // Enter is the cross as well, but on the name entry.
    let starts: Vec<KeyCode> = pad.bindings.keys_of(Action::Start).collect();
    pad.enter_crosses = enter_crosses(
        pad.enter_crosses,
        starts.iter().any(|key| counts(*key)),
        starts.iter().any(|key| keys.just_pressed(*key)),
        naming,
    );
    let enter = if pad.enter_crosses { pad::CROSS } else { 0 };
    pad.keys = from_keys.buttons | enter | pad.steer;
    let from_pad = Pressed::of(
        &crate::bindings::on_screen(&pad.bindings.buttons, on_menus),
        |button| pads.pressed(button),
        |button| pads.just_pressed(button),
    );
    pad.buttons = from_pad.buttons;
    let stick = pad.stick_held.unwrap_or(pads.stick());
    pad.stick_directions = gamepad::directions(stick, pad.stick_directions);
    pad.stick = gamepad::live(stick);
    pad.run = from_keys.run || from_pad.run;
    pad.menu = from_keys.menu || from_pad.menu;
    pad.fast_forward = from_keys.fast_forward || from_pad.fast_forward;
    // Caps Lock on the name entry is the typing's, for capitals: no turn of the run.
    if pad.run_toggle && !naming && (from_keys.toggle || from_pad.toggle) {
        pad.toggled = !pad.toggled;
        info!(
            "the run is turned round: the directions {}",
            if pad.runs() { "run" } else { "walk" }
        );
    }
    // A window without the focus has no mouse: the click that brings it back is not a
    // press of the cross. The mouse of a check run counts as in a window with the focus.
    if !windows.single().is_ok_and(|window| window.focused) && !pointer.checked() {
        pad.mouse = 0;
        return;
    }
    pad.mouse = (MOUSE.iter())
        .filter(|(button, _)| mouse.pressed(*button))
        .fold(0, |held, (_, button)| held | button);
    // A screen whose cursor the mouse steers takes the wheel itself (`pointer.rs`).
    if !pointer.wheel_taken() {
        let wheel = &mut pad.wheel;
        wheel.waiting =
            (wheel.waiting + pointer.notches()).clamp(-NOTCHES_WAITING, NOTCHES_WAITING);
    }
}

/// Whether the held Enter is the cross as well: `before` in the frame before, `held`
/// now (with Alt it is not held), `went_down` in this frame, on the name entry or not.
fn enter_crosses(before: bool, held: bool, went_down: bool, naming: bool) -> bool {
    match (held, went_down) {
        (false, _) => false,
        (true, true) => before || !naming,
        (true, false) => before,
    }
}

/// Behind every game frame: the notch that holds runs out, and after the gap the next
/// one that waits starts.
fn pass_wheel(mut pad: ResMut<Pad>) {
    let wheel = &mut pad.wheel;
    if wheel.frames > 0 {
        wheel.frames -= 1;
        if wheel.frames == 0 {
            (wheel.held, wheel.gap) = (0, NOTCH_GAP);
        }
        return;
    }
    if wheel.gap > 0 {
        wheel.gap -= 1;
        return;
    }
    let held = match wheel.waiting.signum() {
        1 => pad::UP,
        -1 => pad::DOWN,
        _ => return,
    };
    wheel.waiting -= wheel.waiting.signum();
    (wheel.held, wheel.frames) = (held, NOTCH_HELD);
}

/// Whether the cursor shows: never while the player plays with a gamepad, always on a
/// screen whose cursor the mouse steers, and elsewhere until the mouse has rested
/// [`CURSOR_REST`] seconds.
fn cursor_shows(device: Device, aimed: bool, rest: f32) -> bool {
    match device {
        Device::Gamepad => false,
        Device::Keyboard => aimed || rest < CURSOR_REST,
    }
}

/// The game never needs the cursor in a room: it hides once the mouse rests, and comes
/// back when the mouse moves or presses a button. A menu the mouse points in keeps it.
fn hide_resting_cursor(
    time: Res<Time<Real>>,
    motion: Res<AccumulatedMouseMotion>,
    mouse: Res<ButtonInput<MouseButton>>,
    pointer: Res<crate::pointer::Mouse>,
    device: Res<InputDevice>,
    mut pad: ResMut<Pad>,
    mut cursors: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    let busy = motion.delta != Vec2::ZERO || mouse.get_pressed().len() > 0;
    pad.rest = match busy {
        true => 0.0,
        false => pad.rest + time.delta_secs(),
    };
    let visible = cursor_shows(device.last, pointer.wheel_taken(), pad.rest);
    for mut cursor in &mut cursors {
        if cursor.visible != visible {
            cursor.visible = visible;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::gamepad::GamepadButton;

    #[test]
    fn a_notch_of_the_wheel_is_a_short_press_with_a_gap_behind_it() {
        let mut pad = Pad::default();
        pad.wheel.waiting = 2;
        let mut app = App::new();
        app.insert_resource(pad).add_systems(Update, pass_wheel);
        let mut seen = Vec::new();
        for _ in 0..8 {
            app.update();
            seen.push(app.world().resource::<Pad>().held());
        }
        let (up, none) = (pad::UP, 0);
        assert_eq!(seen, [up, up, none, none, up, up, none, none]);
    }

    /// An app with `read_pad` behind the settings, its keys pressed by the test.
    fn app() -> App {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<crate::pointer::Mouse>()
            .init_resource::<Pads>()
            .init_resource::<Pad>()
            .add_systems(Update, (follow_settings, read_pad).chain());
        app
    }

    /// Holds `keys` and nothing else, and answers the pad's word.
    fn press(app: &mut App, keys: &[KeyCode]) -> u16 {
        let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        input.clear();
        let held: Vec<KeyCode> = input.get_pressed().copied().collect();
        for key in held.into_iter().filter(|key| !keys.contains(key)) {
            input.release(key);
        }
        for key in keys {
            input.press(*key);
        }
        app.update();
        app.world().resource::<Pad>().held()
    }

    #[test]
    fn alt_enter_is_not_start() {
        let mut app = app();
        let press = |app: &mut App, keys: &[KeyCode]| press(app, keys) & pad::START;
        assert_eq!(press(&mut app, &[KeyCode::Enter]), pad::START);
        assert_eq!(press(&mut app, &[]), 0);
        assert_eq!(press(&mut app, &[KeyCode::AltLeft, KeyCode::Enter]), 0);
        // Alt let go first: the Enter that switched is still not Start.
        assert_eq!(press(&mut app, &[KeyCode::Enter]), 0);
        assert_eq!(press(&mut app, &[]), 0);
        assert_eq!(press(&mut app, &[KeyCode::NumpadEnter]), pad::START);
    }

    #[test]
    fn enter_is_start_and_the_cross_but_on_the_name_entry() {
        let mut app = app();
        assert_eq!(press(&mut app, &[KeyCode::Enter]), pad::START | pad::CROSS);
        assert_eq!(press(&mut app, &[KeyCode::Enter]), pad::START | pad::CROSS);
        assert_eq!(press(&mut app, &[]), 0);
        // With Alt it is neither.
        assert_eq!(press(&mut app, &[KeyCode::AltLeft, KeyCode::Enter]), 0);
        // On the name entry it is Start alone, and an Enter held from another screen
        // into it, or out of it, stays what it was when it went down.
        assert!(!enter_crosses(false, true, true, true));
        assert!(!enter_crosses(false, true, false, false));
        assert!(enter_crosses(true, true, false, true));
        assert!(!enter_crosses(true, false, false, false));
        assert!(enter_crosses(false, true, true, false));
    }

    #[test]
    fn shift_turns_the_run_round() {
        let mut pad = Pad::default();
        assert!(pad.runs());
        pad.run = true;
        assert!(!pad.runs());
        pad.controls.always_run = false;
        assert!(pad.runs());
    }

    #[test]
    fn caps_lock_turns_the_run_round_until_it_is_pressed_again() {
        let mut app = app();
        app.insert_resource(Prefs::default());
        let runs = |app: &mut App, keys: &[KeyCode]| {
            press(app, keys);
            app.world().resource::<Pad>().runs()
        };
        assert!(runs(&mut app, &[]));
        assert!(!runs(&mut app, &[KeyCode::CapsLock]));
        assert!(!runs(&mut app, &[]));
        // Shift still turns it round while it is held.
        assert!(runs(&mut app, &[KeyCode::ShiftLeft]));
        assert!(runs(&mut app, &[KeyCode::CapsLock]));
        // Without the setting Caps Lock is nothing.
        app.world_mut().resource_mut::<Prefs>().controls.run_toggle = false;
        assert!(runs(&mut app, &[]));
        assert!(runs(&mut app, &[KeyCode::CapsLock]));
    }

    #[test]
    fn the_keys_follow_the_settings() {
        let mut app = app();
        app.insert_resource(Prefs::default());
        assert_eq!(press(&mut app, &[KeyCode::KeyF]), pad::CROSS);
        let mut prefs = app.world_mut().resource_mut::<Prefs>();
        prefs.controls.keys.cross = vec!["KeyG".into(), "Banana".into()];
        prefs.controls.movement = Movement::Tank;
        assert_eq!(press(&mut app, &[KeyCode::KeyF]), 0);
        assert_eq!(press(&mut app, &[KeyCode::KeyG]), pad::CROSS);
        let pad = app.world().resource::<Pad>();
        assert_eq!(pad.controls.movement, Movement::Tank);
    }

    #[test]
    fn a_gamepad_is_the_pad_too() {
        let mut app = app();
        let hold = |app: &mut App, buttons: &[GamepadButton], stick: Vec2| {
            let mut pads = app.world_mut().resource_mut::<Pads>();
            pads.hold_for_a_test(buttons, stick);
            app.update();
            let pad = app.world().resource::<Pad>();
            (pad.held(), pad.directions(), pad.stick())
        };
        let south = [GamepadButton::South];
        assert_eq!(hold(&mut app, &south, Vec2::ZERO), (pad::CROSS, 0, None));
        // Outside the menus RT is the fast forward, not R2 (`bindings::on_screen`).
        let dpad = [GamepadButton::DPadUp, GamepadButton::RightTrigger2];
        assert_eq!(hold(&mut app, &dpad, Vec2::ZERO), (pad::UP, pad::UP, None));
        assert!(app.world().resource::<Pad>().fast_forward());
        let dpad = [GamepadButton::DPadUp, GamepadButton::LeftTrigger2];
        assert_eq!(
            hold(&mut app, &dpad, Vec2::ZERO),
            (pad::UP | pad::L2, pad::UP, None)
        );
        assert!(!app.world().resource::<Pad>().fast_forward());
        // The stick is the directions in a menu, but walks a room as a stick.
        let (held, directions, stick) = hold(&mut app, &[], Vec2::new(0.0, -1.0));
        assert_eq!((held, directions), (pad::DOWN, 0));
        assert!((stick.unwrap() - Vec2::new(0.0, -1.0)).length() < 1e-6);
        // The tank movement has no stick: it walks by the stick's directions.
        app.world_mut().resource_mut::<Pad>().controls.movement = Movement::Tank;
        let (_, directions, _) = hold(&mut app, &[], Vec2::new(0.0, -1.0));
        assert_eq!(directions, pad::DOWN);
        // The left stick's button turns the run round.
        app.insert_resource(Prefs::default());
        hold(&mut app, &[GamepadButton::LeftThumb], Vec2::ZERO);
        assert!(!app.world().resource::<Pad>().runs());
    }

    #[test]
    fn the_cursor_stays_on_a_menu_the_mouse_points_in() {
        assert!(cursor_shows(Device::Keyboard, false, 1.0));
        assert!(!cursor_shows(Device::Keyboard, false, 3.0));
        assert!(cursor_shows(Device::Keyboard, true, 30.0));
        assert!(!cursor_shows(Device::Gamepad, true, 0.0));
    }

    #[test]
    fn the_stick_of_a_check() {
        assert_eq!(sticked("0.5,-1"), Some(Vec2::new(0.5, -1.0)));
        assert_eq!(sticked("2,0"), Some(Vec2::new(1.0, 0.0)));
        assert_eq!(sticked("up"), None);
    }
}
