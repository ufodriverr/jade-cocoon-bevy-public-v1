
use bevy::input::InputSystems;
use bevy::input::gamepad::{Gamepad, GamepadButton};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use jc_formats::pad;

/// The share of the stick's way out that counts as resting: worn sticks do not come
/// back to the middle.
pub const DEAD_ZONE: f32 = 0.25;
/// How far the stick is pushed before it holds a direction in a menu, and how far back
/// it has to come before it lets go of it.
const PRESS: f32 = 0.5;
const LET_GO: f32 = 0.35;
/// The share along a way from which a direction counts: the eight ways each take an
/// eighth of the circle (the cosine of 67.5 degrees).
const ALONG: f32 = 0.383;
/// Logical pixels the mouse moves in a frame before it counts as the keyboard's hand.
const MOUSE_MOVED: f32 = 4.0;

/// The pads, read once a frame in front of `keys.rs`.
pub struct GamepadPlugin {
    /// A play session reads the pads; a check does not.
    pub reads: bool,
}

impl Plugin for GamepadPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Pads {
            reads: self.reads,
            ..default()
        })
        .init_resource::<InputDevice>()
        .add_systems(
            PreUpdate,
            (read_pads, follow_device).chain().after(InputSystems),
        );
    }
}

/// Which hand the player used last: for the help lines, which show its buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Device {
    #[default]
    Keyboard,
    Gamepad,
}

/// The device used last, and whether a pad is plugged in.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InputDevice {
    pub last: Device,
    pub pad_connected: bool,
}

/// What happened in a frame, for [`InputDevice::follow`].
#[derive(Debug, Clone, Copy, Default)]
pub struct Seen {
    pub pad_connected: bool,
    /// A pad was plugged in.
    pub pad_joined: bool,
    /// A button of a pad went down, or its stick left the middle.
    pub pad_used: bool,
    /// A key or a mouse button went down, the wheel turned or the mouse moved.
    pub keyboard_used: bool,
}

impl InputDevice {
    /// The pad as soon as it is plugged in or used, the keyboard as soon as it or the
    /// mouse is used, and the keyboard always without a pad.
    pub fn follow(&mut self, seen: Seen) {
        self.pad_connected = seen.pad_connected;
        self.last = match () {
            _ if !seen.pad_connected => Device::Keyboard,
            _ if seen.pad_joined || seen.pad_used => Device::Gamepad,
            _ if seen.keyboard_used => Device::Keyboard,
            _ => self.last,
        };
    }
}

/// The pads plugged in and the one that drives.
#[derive(Resource, Debug, Default)]
pub struct Pads {
    reads: bool,
    /// The pad that drives: the one used last.
    active: Option<Entity>,
    /// The pads plugged in, as the frame before saw them.
    known: Vec<Entity>,
    /// The buttons the driving pad holds and those that went down in this frame.
    held: Vec<GamepadButton>,
    went_down: Vec<GamepadButton>,
    /// Its left stick, up above 0, each axis from -1 to 1.
    stick: Vec2,
    /// What this frame saw.
    seen: Seen,
}

impl Pads {
    pub fn pressed(&self, button: GamepadButton) -> bool {
        self.held.contains(&button)
    }

    pub fn just_pressed(&self, button: GamepadButton) -> bool {
        self.went_down.contains(&button)
    }

    /// The driving pad's left stick as it lies, up above 0.
    pub fn stick(&self) -> Vec2 {
        self.stick
    }

    /// The driving pad holds `buttons` and its stick at `stick`.
    #[cfg(test)]
    pub fn hold_for_a_test(&mut self, buttons: &[GamepadButton], stick: Vec2) {
        let before = std::mem::take(&mut self.held);
        self.held = buttons.to_vec();
        self.went_down = (buttons.iter().copied())
            .filter(|button| !before.contains(button))
            .collect();
        self.stick = stick;
    }
}

/// The stick out of its dead zone: its way, and how far it is pushed from the edge of
/// the dead zone (above 0 up to 1); none in the dead zone.
pub fn live(raw: Vec2) -> Option<Vec2> {
    let length = raw.length();
    if !length.is_finite() || length <= DEAD_ZONE {
        return None;
    }
    let pushed = ((length - DEAD_ZONE) / (1.0 - DEAD_ZONE)).min(1.0);
    Some(raw / length * pushed)
}

/// The directions of the pad's word that the stick holds in a menu: the nearest of the
/// eight ways once it is pushed past [`PRESS`], kept until it comes back under
/// [`LET_GO`]; `before` is what it held in the frame before.
pub fn directions(raw: Vec2, before: u16) -> u16 {
    let length = raw.length();
    let wanted = if before == 0 { PRESS } else { LET_GO };
    if !length.is_finite() || length < wanted {
        return 0;
    }
    let way = raw / length;
    let mut held = 0;
    if way.y > ALONG {
        held |= pad::UP;
    }
    if way.y < -ALONG {
        held |= pad::DOWN;
    }
    if way.x > ALONG {
        held |= pad::RIGHT;
    }
    if way.x < -ALONG {
        held |= pad::LEFT;
    }
    held
}

pub fn stick_length(pushed: f32, runs: bool) -> f64 {
    const WALKS: f64 = 54.0;
    const MOST: f64 = 127.0;
    const WALK_ONLY: f64 = 80.0;
    let length = WALKS + f64::from(pushed.clamp(0.0, 1.0)) * (MOST - WALKS);
    match runs {
        true => length,
        false => length.min(WALK_ONLY),
    }
}

/// The pad that drives in this frame: one that was used, the one before while it is
/// plugged in, or else the first one plugged in.
pub fn driver(active: Option<Entity>, plugged: &[Entity], used: &[Entity]) -> Option<Entity> {
    if let Some(active) = active.filter(|active| used.contains(active)) {
        return Some(active);
    }
    if let Some(first) = used.first() {
        return Some(*first);
    }
    active
        .filter(|active| plugged.contains(active))
        .or_else(|| plugged.first().copied())
}

pub(crate) fn read_pads(
    gamepads: Query<(Entity, &Gamepad, Option<&Name>)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut pads: ResMut<Pads>,
) {
    let pads = &mut *pads;
    let plugged: Vec<Entity> = match pads.reads {
        true => gamepads.iter().map(|(entity, ..)| entity).collect(),
        false => Vec::new(),
    };
    let joined = plugged
        .iter()
        .find(|pad| !pads.known.contains(pad))
        .copied();
    for gone in pads.known.iter().filter(|pad| !plugged.contains(pad)) {
        info!("the gamepad {gone} is gone");
    }
    if let Some(joined) = joined {
        let name = (gamepads.get(joined).ok())
            .and_then(|(_, _, name)| name)
            .map_or("", Name::as_str);
        info!("the gamepad {joined} ({name}) is plugged in, and drives");
        pads.active = Some(joined);
    }
    pads.known.clone_from(&plugged);
    // A window without the focus has no pad, as it has no keys.
    let focused = windows.single().is_ok_and(|window| window.focused);
    let used: Vec<Entity> = match focused {
        true => (plugged.iter().copied())
            .filter(|pad| {
                gamepads.get(*pad).is_ok_and(|(_, gamepad, _)| {
                    gamepad.get_just_pressed().next().is_some()
                        || live(gamepad.left_stick()).is_some()
                })
            })
            .collect(),
        false => Vec::new(),
    };
    let active = driver(pads.active, &plugged, &used);
    if active != pads.active
        && let Some(active) = active
    {
        info!("the gamepad {active} drives");
    }
    pads.active = active;
    pads.held.clear();
    pads.went_down.clear();
    pads.stick = Vec2::ZERO;
    let driving = active
        .filter(|_| focused)
        .and_then(|pad| gamepads.get(pad).ok());
    if let Some((_, gamepad, _)) = driving {
        pads.held.extend(gamepad.get_pressed().copied());
        pads.went_down.extend(gamepad.get_just_pressed().copied());
        pads.stick = gamepad.left_stick();
    }
    pads.seen = Seen {
        pad_connected: !plugged.is_empty(),
        pad_joined: joined.is_some(),
        pad_used: !used.is_empty(),
        keyboard_used: false,
    };
}

/// Behind [`read_pads`]: which hand the player used last.
pub(crate) fn follow_device(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    pads: Res<Pads>,
    mut device: ResMut<InputDevice>,
) {
    let keyboard_used = keys.get_just_pressed().next().is_some()
        || mouse.get_just_pressed().next().is_some()
        || scroll.delta != Vec2::ZERO
        || motion.delta.length() > MOUSE_MOVED;
    let before = *device;
    let mut now = before;
    now.follow(Seen {
        keyboard_used,
        ..pads.seen
    });
    if now != before {
        info!(
            "the player's hand: {:?} (a gamepad plugged in: {})",
            now.last, now.pad_connected
        );
        *device = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::gamepad::GamepadAxis;

    #[test]
    fn the_dead_zone_is_rest_and_the_rest_of_the_way_is_the_whole() {
        assert_eq!(live(Vec2::ZERO), None);
        assert_eq!(live(Vec2::new(0.2, 0.1)), None);
        let full = live(Vec2::new(0.0, 1.0)).unwrap();
        assert!((full - Vec2::new(0.0, 1.0)).length() < 1e-6);
        // Just past the dead zone is a little; the way stays.
        let little = live(Vec2::new(0.3, 0.0)).unwrap();
        assert!(little.x > 0.0 && little.x < 0.1 && little.y == 0.0);
        // A corner of a square gate is no more than the whole.
        assert!((live(Vec2::new(1.0, 1.0)).unwrap().length() - 1.0).abs() < 1e-6);
        assert_eq!(live(Vec2::new(f32::NAN, 0.0)), None);
    }

    #[test]
    fn the_stick_is_the_nearest_of_eight_ways_in_a_menu() {
        assert_eq!(directions(Vec2::new(0.0, 0.4), 0), 0);
        assert_eq!(directions(Vec2::new(0.0, 0.6), 0), pad::UP);
        assert_eq!(directions(Vec2::new(0.0, -0.9), 0), pad::DOWN);
        assert_eq!(directions(Vec2::new(-0.9, 0.1), 0), pad::LEFT);
        assert_eq!(directions(Vec2::new(0.7, 0.7), 0), pad::UP | pad::RIGHT);
        // A little off the straight way is still straight.
        assert_eq!(directions(Vec2::new(0.3, 0.9), 0), pad::UP);
        // Held, it lets go only below the lower mark.
        assert_eq!(directions(Vec2::new(0.0, 0.4), pad::UP), pad::UP);
        assert_eq!(directions(Vec2::new(0.0, 0.3), pad::UP), 0);
    }

    #[test]
    fn a_light_push_walks_and_a_firm_one_runs() {
        let command = |pushed: f32, runs: bool| {
            let length = stick_length(pushed, runs);
            let stick = [0, length.round() as i8];
            jc_formats::player::stick_command(stick)
        };
        use jc_formats::player::command;
        let walk = command::WALK | command::STICK_ONE;
        let run = command::RUN | command::STICK_ONE;
        assert_eq!(command(0.0, true), walk);
        assert_eq!(command(0.5, true), walk);
        assert_eq!(command(0.8, true), run);
        assert_eq!(command(1.0, true), run);
        assert_eq!(command(1.0, false), walk);
    }

    #[test]
    fn the_pad_used_last_drives() {
        let world = &mut World::new();
        let [one, two, three] = [(); 3].map(|_| world.spawn_empty().id());
        assert_eq!(driver(None, &[], &[]), None);
        assert_eq!(driver(None, &[one, two], &[]), Some(one));
        assert_eq!(driver(Some(one), &[one, two], &[two]), Some(two));
        assert_eq!(driver(Some(two), &[one, two], &[]), Some(two));
        // Both used: the one that drove goes on driving.
        assert_eq!(driver(Some(two), &[one, two], &[one, two]), Some(two));
        // Pulled out: the next one.
        assert_eq!(driver(Some(two), &[one, three], &[]), Some(one));
    }

    #[test]
    fn the_hand_follows_the_device_used_last() {
        let mut device = InputDevice::default();
        assert_eq!(device.last, Device::Keyboard);
        let plugged = Seen {
            pad_connected: true,
            ..default()
        };
        device.follow(Seen {
            pad_joined: true,
            ..plugged
        });
        assert_eq!((device.last, device.pad_connected), (Device::Gamepad, true));
        device.follow(plugged);
        assert_eq!(device.last, Device::Gamepad);
        device.follow(Seen {
            keyboard_used: true,
            ..plugged
        });
        assert_eq!(device.last, Device::Keyboard);
        device.follow(Seen {
            pad_used: true,
            keyboard_used: true,
            ..plugged
        });
        assert_eq!(device.last, Device::Gamepad);
        // Pulled out: the keyboard, whatever was used.
        device.follow(Seen {
            pad_used: true,
            ..default()
        });
        assert_eq!(
            (device.last, device.pad_connected),
            (Device::Keyboard, false)
        );
    }

    /// An app with the window's focus and a pad that Bevy's systems do not touch: the
    /// test sets its buttons and its stick itself.
    fn app(reads: bool) -> App {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<AccumulatedMouseMotion>()
            .init_resource::<AccumulatedMouseScroll>()
            .insert_resource(Pads { reads, ..default() })
            .init_resource::<InputDevice>()
            .add_systems(Update, (read_pads, follow_device).chain());
        app.world_mut().spawn((
            Window {
                focused: true,
                ..default()
            },
            PrimaryWindow,
        ));
        app
    }

    #[test]
    fn a_pad_plugged_in_drives_and_its_buttons_reach_the_pads() {
        let mut app = app(true);
        app.update();
        assert_eq!(
            *app.world().resource::<InputDevice>(),
            InputDevice::default()
        );
        let mut gamepad = Gamepad::default();
        gamepad.digital_mut().press(GamepadButton::South);
        (gamepad.analog_mut()).set(GamepadAxis::LeftStickY, 0.9);
        let pad = app.world_mut().spawn(gamepad).id();
        app.update();
        let pads = app.world().resource::<Pads>();
        assert_eq!(pads.active, Some(pad));
        assert!(pads.pressed(GamepadButton::South) && pads.just_pressed(GamepadButton::South));
        assert!((pads.stick() - Vec2::new(0.0, 0.9)).length() < 1e-6);
        let device = *app.world().resource::<InputDevice>();
        assert_eq!((device.last, device.pad_connected), (Device::Gamepad, true));
        // A key: the keyboard's hand.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        app.world_mut()
            .entity_mut(pad)
            .get_mut::<Gamepad>()
            .unwrap()
            .digital_mut()
            .clear();
        (app.world_mut()
            .entity_mut(pad)
            .get_mut::<Gamepad>()
            .unwrap()
            .analog_mut())
        .set(GamepadAxis::LeftStickY, 0.0);
        app.update();
        assert_eq!(app.world().resource::<InputDevice>().last, Device::Keyboard);
        // Pulled out.
        app.world_mut().entity_mut(pad).remove::<Gamepad>();
        app.update();
        assert_eq!(app.world().resource::<Pads>().active, None);
        assert!(!app.world().resource::<InputDevice>().pad_connected);
    }

    #[test]
    fn a_check_reads_no_pad() {
        let mut app = app(false);
        let mut gamepad = Gamepad::default();
        gamepad.digital_mut().press(GamepadButton::South);
        app.world_mut().spawn(gamepad);
        app.update();
        let pads = app.world().resource::<Pads>();
        assert!(pads.active.is_none() && !pads.pressed(GamepadButton::South));
        assert!(!app.world().resource::<InputDevice>().pad_connected);
    }
}
