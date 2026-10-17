//! The touch screen as the console's pad, for a phone (`android.rs`): buttons drawn
//! over the window, beside the 4:3 picture where the screen is wide enough, and a tap
//! anywhere else as the mouse's left button of a PC (`pointer.rs`), so that a row of a
//! menu is tapped as it is clicked and a tap in a room is the cross.
//!
//! | on the screen | the pad |
//! |---|---|
//! | the stick, bottom left: a thumb put down anywhere there is its middle | the left stick of a gamepad (`gamepad.rs`): a light push walks, a firm one runs; in a menu, the directions |
//! | the four signs, bottom right | cross, circle, triangle, square |
//! | L1, L2, R1, R2 in the top corners | the shoulder buttons |
//! | SELECT and START | Select and Start (held, Start skips a scene, `skip.rs`) |
//! | MENU | the menu over a room ([`crate::keys::Pad::menu_key`]) |
//! | `>>` | the fast forward (`pace.rs`), on until it is tapped again |
//!
//! A thumb that slides from one sign to another takes the other. The back gesture of
//! the phone is the triangle. The buttons hide while a gamepad drives, and come back
//! with the next touch. `--touch-controls` shows them on a PC, for a look.

use std::collections::HashMap;

use bevy::camera::visibility::RenderLayers;
use bevy::input::InputSystems;
use bevy::input::touch::Touches;
use bevy::prelude::*;
use bevy::sprite_render::AlphaMode2d;
use bevy::window::PrimaryWindow;
use jc_formats::pad;

use crate::display::{self, Frame, Presents};
use crate::gamepad::{Device, InputDevice};

/// The size of everything, as a share of the window's height.
const UNIT: f32 = 1.0 / 10.0;
/// How far out of its middle the stick reaches, in units.
const STICK_REACH: f32 = 1.5;
/// The ring of the stick and its knob, in units.
const STICK_RING: f32 = 1.6;
const STICK_KNOB: f32 = 0.7;
/// The four signs: the radius of one and how far each lies from their middle, in units.
const FACE: f32 = 0.62;
const FACE_APART: f32 = 1.08;
/// The small round buttons (shoulders, Start, Select, Menu, the fast forward).
const SMALL: f32 = 0.5;
/// A touch counts on a button this much beyond its edge.
const REACH: f32 = 1.3;
/// The draw order of the buttons over the window: above the pace's marker.
const Z: f32 = 20.0;

/// The buttons over the window and the touches on them.
pub struct TouchPlugin {
    /// The buttons are drawn (a phone always; a PC with `--touch-controls`).
    pub shown: bool,
}

impl Plugin for TouchPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(TouchPad {
            enabled: self.shown,
            shown: self.shown,
            ..default()
        })
        .add_systems(
            PreUpdate,
            read_touches
                .after(InputSystems)
                .before(crate::pointer::read_mouse),
        )
        .add_systems(Update, draw_buttons);
    }
}

/// One button over the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Control {
    Stick,
    Cross,
    Circle,
    Triangle,
    Square,
    L1,
    L2,
    R1,
    R2,
    Select,
    Start,
    Menu,
    Fast,
}

impl Control {
    const FACES: [Control; 4] = [
        Control::Cross,
        Control::Circle,
        Control::Triangle,
        Control::Square,
    ];

    /// The pad's buttons it holds.
    fn buttons(self) -> u16 {
        match self {
            Control::Cross => pad::CROSS,
            Control::Circle => pad::CIRCLE,
            Control::Triangle => pad::TRIANGLE,
            Control::Square => pad::SQUARE,
            Control::L1 => pad::L1,
            Control::L2 => pad::L2,
            Control::R1 => pad::R1,
            Control::R2 => pad::R2,
            Control::Select => pad::SELECT,
            Control::Start => pad::START,
            Control::Stick | Control::Menu | Control::Fast => 0,
        }
    }

    fn label(self) -> Option<&'static str> {
        match self {
            Control::L1 => Some("L1"),
            Control::L2 => Some("L2"),
            Control::R1 => Some("R1"),
            Control::R2 => Some("R2"),
            Control::Select => Some("SEL"),
            Control::Start => Some("START"),
            Control::Menu => Some("MENU"),
            Control::Fast => Some(">>"),
            _ => None,
        }
    }

    /// The colour of a sign: the console's own.
    fn colour(self) -> Color {
        match self {
            Control::Cross => Color::srgb(0.49, 0.71, 1.0),
            Control::Circle => Color::srgb(1.0, 0.42, 0.42),
            Control::Triangle => Color::srgb(0.25, 0.84, 0.62),
            Control::Square => Color::srgb(0.96, 0.55, 0.85),
            _ => Color::srgb(0.92, 0.92, 0.92),
        }
    }
}

/// A button's place over the window, in the window's physical pixels, the middle at
/// the origin and y up (`display::OVER_THE_WINDOW`).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Placed {
    control: Control,
    at: Vec2,
    radius: f32,
}

/// Where every button lies in a window of a size.
#[derive(Debug, Clone, PartialEq)]
struct Layout {
    window: Vec2,
    unit: f32,
    buttons: Vec<Placed>,
    /// The stick's middle while no thumb is on it.
    stick_home: Vec2,
    /// A thumb put down left of this and under `stick_top` is on the stick.
    stick_right: f32,
    stick_top: f32,
}

impl Layout {
    fn of(window: Vec2) -> Self {
        let unit = (window.y * UNIT).max(24.0);
        let (left, right) = (-window.x / 2.0, window.x / 2.0);
        let (bottom, top) = (-window.y / 2.0, window.y / 2.0);
        // The free sides of a wide screen beside the 4:3 picture.
        let side = ((window.x - window.y * 4.0 / 3.0) / 2.0).max(0.0);
        let stick_home = Vec2::new(left + 2.2 * unit, bottom + 2.6 * unit);
        let face = Vec2::new(right - 2.3 * unit, bottom + 2.7 * unit);
        let mut buttons = vec![
            Placed {
                control: Control::Cross,
                at: face - Vec2::Y * FACE_APART * unit,
                radius: FACE * unit,
            },
            Placed {
                control: Control::Circle,
                at: face + Vec2::X * FACE_APART * unit,
                radius: FACE * unit,
            },
            Placed {
                control: Control::Triangle,
                at: face + Vec2::Y * FACE_APART * unit,
                radius: FACE * unit,
            },
            Placed {
                control: Control::Square,
                at: face - Vec2::X * FACE_APART * unit,
                radius: FACE * unit,
            },
        ];
        let small = |control, x: f32, y: f32| Placed {
            control,
            at: Vec2::new(x, y),
            radius: SMALL * unit,
        };
        let row = top - 0.8 * unit;
        let second = top - 2.1 * unit;
        buttons.extend([
            small(Control::L2, left + 0.9 * unit, row),
            small(Control::L1, left + 2.3 * unit, row),
            small(Control::R1, right - 2.3 * unit, row),
            small(Control::R2, right - 0.9 * unit, row),
            small(Control::Select, left + 0.9 * unit, second),
            small(Control::Menu, left + 2.3 * unit, second),
            small(Control::Fast, right - 2.3 * unit, second),
            small(Control::Start, right - 0.9 * unit, second),
        ]);
        Self {
            window,
            unit,
            buttons,
            stick_home,
            stick_right: left + side.max(4.4 * unit),
            stick_top: second - SMALL * unit * REACH,
        }
    }

    /// The button a touch at `at` lands on: the nearest one it reaches.
    fn button_at(&self, at: Vec2) -> Option<Control> {
        (self.buttons.iter())
            .map(|placed| (placed, placed.at.distance(at)))
            .filter(|(placed, distance)| *distance <= placed.radius * REACH)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(placed, _)| placed.control)
    }

    /// The sign a sliding thumb at `at` is on, of the four.
    fn face_at(&self, at: Vec2) -> Option<Control> {
        self.button_at(at)
            .filter(|control| Control::FACES.contains(control))
    }

    /// Whether a touch at `at` takes the stick.
    fn on_stick(&self, at: Vec2) -> bool {
        at.x < self.stick_right && at.y < self.stick_top
    }

    /// The middle of a stick whose thumb went down at `at`: there, but with the whole
    /// ring on the screen.
    fn stick_middle(&self, at: Vec2) -> Vec2 {
        let ring = STICK_RING * self.unit;
        let half = self.window / 2.0;
        Vec2::new(
            at.x.clamp(-half.x + ring, (half.x - ring).max(-half.x + ring)),
            at.y.clamp(-half.y + ring, (half.y - ring).max(-half.y + ring)),
        )
    }
}

/// The stick of a thumb at `at` around a middle: up to 1 each way, up above 0.
fn stick_of(middle: Vec2, at: Vec2, reach: f32) -> Vec2 {
    let way = (at - middle) / reach;
    match way.length() > 1.0 {
        true => way.normalize(),
        false => way,
    }
}

/// What a touch holds.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Owner {
    Stick,
    Button(Control),
    /// The mouse of `pointer.rs`.
    Pointer,
}

/// A touch on the screen, or one just lifted that still holds its button.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Held {
    owner: Owner,
    /// The second it went down (`Time<Real>`).
    since: f32,
    /// It was lifted: it lets go once it has held [`SHORTEST`].
    lifted: bool,
    /// Where it is (or was lifted), in logical pixels of the window.
    at: Vec2,
}

/// The shortest press of a tap: a tap that comes and goes within one frame still
/// reaches a game frame (the game reads the pad at its own pace).
const SHORTEST: f32 = 0.1;

/// The touch screen's pad of this frame, which `keys.rs` and `pointer.rs` read.
#[derive(Resource, Debug, Default)]
pub struct TouchPad {
    enabled: bool,
    /// The buttons show: not while a gamepad drives.
    shown: bool,
    layout: Option<Layout>,
    touches: HashMap<u64, Held>,
    /// The middle of the stick while a thumb is on it, and where the thumb is.
    stick_middle: Option<Vec2>,
    stick_thumb: Vec2,
    /// The fast forward is on.
    fast: bool,
    /// The pad's buttons held by touches.
    held: u16,
    /// The menu key is held.
    menu: bool,
    /// Where the mouse of the touches is, in logical pixels of the window: the touch
    /// that is the mouse, or where the last one was lifted, where a mouse would rest.
    pointer: Option<Vec2>,
    /// A touch that is the mouse is down: its left button.
    pointer_down: bool,
    /// The phone's back button holds the triangle.
    back: bool,
}

impl TouchPad {
    /// The pad's buttons the touches hold.
    pub fn held(&self) -> u16 {
        self.held | if self.back { pad::TRIANGLE } else { 0 }
    }

    /// The stick of the touches, up above 0, each way up to 1.
    pub fn stick(&self) -> Option<Vec2> {
        let reach = STICK_REACH * self.layout.as_ref()?.unit;
        let middle = self.stick_middle?;
        Some(stick_of(middle, self.stick_thumb, reach))
    }

    pub fn menu_key(&self) -> bool {
        self.menu
    }

    pub fn fast_forward(&self) -> bool {
        self.fast
    }

    /// Where the mouse of the touches is, in logical pixels of the window.
    pub fn pointer(&self) -> Option<Vec2> {
        self.pointer
    }
}

/// Reads the touches of this frame into the touch screen's pad, and the one that is
/// the mouse into the mouse's left button.
#[allow(clippy::too_many_arguments)]
fn read_touches(
    touches: Res<Touches>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time<Real>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    frame: Option<Res<Frame>>,
    device: Res<InputDevice>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut touch: ResMut<TouchPad>,
) {
    let touch = &mut *touch;
    if !touch.enabled {
        return;
    }
    touch.back = keys.pressed(KeyCode::BrowserBack);
    let window = frame.as_ref().and_then(|frame| frame.window());
    if touch.layout.as_ref().map(|layout| layout.window) != window {
        touch.layout = window.map(Layout::of);
    }
    let scale = windows.single().map_or(1.0, Window::scale_factor);
    let Some(layout) = &touch.layout else {
        return;
    };
    let now = time.elapsed_secs();
    // A touch's place over the window: physical pixels, the middle at the origin, y up.
    let over = |at: Vec2| {
        let at = at * scale;
        Vec2::new(at.x - layout.window.x / 2.0, layout.window.y / 2.0 - at.y)
    };
    let driving = device.last == Device::Gamepad && device.pad_connected;
    if touches.any_just_pressed() {
        touch.shown = true;
    } else if driving {
        touch.shown = false;
    }
    // The lifted touches that have held long enough let go.
    (touch.touches).retain(|_, held| !(held.lifted && now - held.since >= SHORTEST));
    let mut ticks = false;
    for finger in touches.iter_just_pressed() {
        let at = over(finger.position());
        let thumbs =
            |owner: Owner| (touch.touches.values()).any(|held| !held.lifted && held.owner == owner);
        let owner = match layout.button_at(at) {
            Some(control) if touch.shown => Owner::Button(control),
            _ if touch.shown && layout.on_stick(at) && !thumbs(Owner::Stick) => {
                touch.stick_middle = Some(layout.stick_middle(at));
                touch.stick_thumb = at;
                Owner::Stick
            }
            _ if thumbs(Owner::Pointer) => continue,
            _ => Owner::Pointer,
        };
        if let Owner::Button(control) = owner {
            ticks = true;
            if control == Control::Fast {
                touch.fast = !touch.fast;
                info!(
                    "the touch screen: the fast forward {}",
                    if touch.fast { "on" } else { "off" }
                );
            }
        }
        let held = Held {
            owner,
            since: now,
            lifted: false,
            at: finger.position(),
        };
        touch.touches.insert(finger.id(), held);
    }
    for finger in touches.iter() {
        let at = over(finger.position());
        let Some(held) = touch.touches.get_mut(&finger.id()) else {
            continue;
        };
        held.at = finger.position();
        match &mut held.owner {
            Owner::Stick => touch.stick_thumb = at,
            // A thumb that slides from one sign onto another takes it.
            Owner::Button(control) if Control::FACES.contains(control) => {
                if let Some(other) = layout.face_at(at).filter(|other| other != control) {
                    *control = other;
                    ticks = true;
                }
            }
            _ => {}
        }
    }
    let ended = (touches.iter_just_released()).chain(touches.iter_just_canceled());
    for finger in ended {
        if let Some(held) = touch.touches.get_mut(&finger.id()) {
            held.lifted = true;
            held.at = finger.position();
        }
    }
    // A finger gone without a word of its end is lifted too.
    for (id, held) in &mut touch.touches {
        if touches.get_pressed(*id).is_none() {
            held.lifted = true;
        }
    }
    // The stick lets go as soon as its thumb is lifted.
    (touch.touches).retain(|_, held| !(held.lifted && held.owner == Owner::Stick));
    if !(touch.touches.values()).any(|held| held.owner == Owner::Stick) {
        touch.stick_middle = None;
    }
    let (mut held, mut menu, mut pointer) = (0, false, None);
    for touched in touch.touches.values() {
        match touched.owner {
            Owner::Button(control) => {
                held |= control.buttons();
                menu |= control == Control::Menu;
            }
            Owner::Pointer => pointer = Some(touched.at),
            Owner::Stick => {}
        }
    }
    touch.held = held;
    touch.menu = menu;
    // The mouse's left button follows the touch that is the mouse; once it is lifted
    // the mouse rests where it was, as a PC's mouse stays where it clicked.
    match (touch.pointer_down, pointer.is_some()) {
        (false, true) => mouse.press(MouseButton::Left),
        (true, false) => mouse.release(MouseButton::Left),
        _ => {}
    }
    touch.pointer_down = pointer.is_some();
    if pointer.is_some() {
        touch.pointer = pointer;
    }
    if ticks {
        tick();
    }
}

/// A short tick of the phone's vibration under the thumb.
fn tick() {
    #[cfg(target_os = "android")]
    crate::android::tick();
}

/// A part of the drawn buttons.
#[derive(Component)]
struct Part {
    control: Control,
    /// The disc behind a button, which lights while it is held.
    disc: bool,
}

/// The knob of the stick.
#[derive(Component)]
struct Knob;

/// The ring of the stick.
#[derive(Component)]
struct Ring;

/// What the drawn buttons were made for, and their two looks.
#[derive(Default)]
struct Drawn {
    window: Option<Vec2>,
    idle: Handle<ColorMaterial>,
    lit: Handle<ColorMaterial>,
}

/// The buttons over the window: made anew when the window's size changes, lit while
/// they are held, the knob where the thumb is.
#[allow(clippy::too_many_arguments)]
fn draw_buttons(
    touch: Res<TouchPad>,
    parts: Query<Entity, With<Part>>,
    mut discs: Query<(&Part, &mut MeshMaterial2d<ColorMaterial>)>,
    mut knob: Query<&mut Transform, (With<Knob>, Without<Ring>)>,
    mut ring: Query<&mut Transform, (With<Ring>, Without<Knob>)>,
    mut visibility: Query<&mut Visibility, With<Part>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut drawn: Local<Drawn>,
    mut commands: Commands,
) {
    let Some(layout) = touch.layout.as_ref().filter(|_| touch.enabled) else {
        return;
    };
    // A screen that took everything away may have taken the buttons too.
    let gone = drawn.window.is_some() && parts.is_empty();
    if drawn.window != Some(layout.window) || gone {
        for part in &parts {
            commands.entity(part).despawn();
        }
        drawn.window = Some(layout.window);
        let blend = |color: Color| ColorMaterial {
            color,
            alpha_mode: AlphaMode2d::Blend,
            ..default()
        };
        drawn.idle = materials.add(blend(Color::srgba(1.0, 1.0, 1.0, 0.13)));
        drawn.lit = materials.add(blend(Color::srgba(1.0, 1.0, 1.0, 0.42)));
        spawn_buttons(layout, &drawn, &mut meshes, &mut materials, &mut commands);
        return;
    }
    let visible = match touch.shown {
        true => Visibility::Inherited,
        false => Visibility::Hidden,
    };
    for mut shown in &mut visibility {
        shown.set_if_neq(visible);
    }
    for (part, mut material) in &mut discs {
        if !part.disc {
            continue;
        }
        let lit = match part.control {
            Control::Stick => touch.stick_middle.is_some(),
            Control::Menu => touch.menu,
            Control::Fast => touch.fast,
            control => touch.held() & control.buttons() != 0,
        };
        let wanted = if lit { &drawn.lit } else { &drawn.idle };
        if material.0 != *wanted {
            material.0 = wanted.clone();
        }
    }
    let middle = touch.stick_middle.unwrap_or(layout.stick_home);
    let pushed = touch.stick().unwrap_or(Vec2::ZERO) * STICK_REACH * layout.unit;
    for mut transform in &mut ring {
        transform.translation = middle.extend(Z);
    }
    for mut transform in &mut knob {
        transform.translation = (middle + pushed).extend(Z + 1.0);
    }
}

fn spawn_buttons(
    layout: &Layout,
    drawn: &Drawn,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    commands: &mut Commands,
) {
    let layer = RenderLayers::layer(display::OVER_THE_WINDOW);
    let unit = layout.unit;
    let blend = |color: Color| ColorMaterial {
        color,
        alpha_mode: AlphaMode2d::Blend,
        ..default()
    };
    // The stick: a ring and its knob.
    commands.spawn((
        Part {
            control: Control::Stick,
            disc: true,
        },
        Ring,
        Presents,
        Mesh2d(meshes.add(Circle::new(STICK_RING * unit))),
        MeshMaterial2d(drawn.idle.clone()),
        Transform::from_translation(layout.stick_home.extend(Z)),
        layer.clone(),
    ));
    let edge = materials.add(blend(Color::srgba(1.0, 1.0, 1.0, 0.35)));
    commands.spawn((
        Part {
            control: Control::Stick,
            disc: false,
        },
        Ring,
        Presents,
        Mesh2d(meshes.add(Annulus::new(
            STICK_RING * unit - 0.08 * unit,
            STICK_RING * unit,
        ))),
        MeshMaterial2d(edge.clone()),
        Transform::from_translation(layout.stick_home.extend(Z + 0.5)),
        layer.clone(),
    ));
    commands.spawn((
        Part {
            control: Control::Stick,
            disc: false,
        },
        Knob,
        Presents,
        Mesh2d(meshes.add(Circle::new(STICK_KNOB * unit))),
        MeshMaterial2d(materials.add(blend(Color::srgba(1.0, 1.0, 1.0, 0.45)))),
        Transform::from_translation(layout.stick_home.extend(Z + 1.0)),
        layer.clone(),
    ));
    for placed in &layout.buttons {
        let control = placed.control;
        let at = |z: f32| Transform::from_translation(placed.at.extend(Z + z));
        commands.spawn((
            Part {
                control,
                disc: true,
            },
            Presents,
            Mesh2d(meshes.add(Circle::new(placed.radius))),
            MeshMaterial2d(drawn.idle.clone()),
            at(0.0),
            layer.clone(),
        ));
        commands.spawn((
            Part {
                control,
                disc: false,
            },
            Presents,
            Mesh2d(meshes.add(Annulus::new(placed.radius - 0.05 * unit, placed.radius))),
            MeshMaterial2d(edge.clone()),
            at(0.5),
            layer.clone(),
        ));
        let sign = materials.add(blend(control.colour().with_alpha(0.9)));
        let r = placed.radius;
        let bar = |length: f32| Rectangle::new(length, 0.09 * unit);
        let mut shape = |mesh: Mesh, transform: Transform| {
            commands.spawn((
                Part {
                    control,
                    disc: false,
                },
                Presents,
                Mesh2d(meshes.add(mesh)),
                MeshMaterial2d(sign.clone()),
                transform,
                layer.clone(),
            ));
        };
        let on = |offset: Vec2, angle: f32| {
            Transform::from_translation((placed.at + offset).extend(Z + 1.0))
                .with_rotation(Quat::from_rotation_z(angle))
        };
        match control {
            Control::Cross => {
                let quarter = std::f32::consts::FRAC_PI_4;
                shape(bar(0.95 * r).into(), on(Vec2::ZERO, quarter));
                shape(bar(0.95 * r).into(), on(Vec2::ZERO, -quarter));
            }
            Control::Circle => {
                shape(
                    Annulus::new(0.34 * r - 0.09 * unit, 0.34 * r).into(),
                    on(Vec2::ZERO, 0.0),
                );
            }
            Control::Triangle => {
                // Three bars round the middle, a third of a turn apart.
                let side = 0.62 * r;
                let inner = side / (2.0 * 3f32.sqrt());
                for turn in 0..3 {
                    let angle = turn as f32 * std::f32::consts::TAU / 3.0;
                    let out = Vec2::from_angle(angle - std::f32::consts::FRAC_PI_2) * inner;
                    shape(bar(side + 0.06 * unit).into(), on(out, angle));
                }
            }
            Control::Square => {
                let half = 0.27 * r;
                for (offset, angle) in [
                    (Vec2::Y * half, 0.0),
                    (-Vec2::Y * half, 0.0),
                    (Vec2::X * half, std::f32::consts::FRAC_PI_2),
                    (-Vec2::X * half, std::f32::consts::FRAC_PI_2),
                ] {
                    shape(bar(2.0 * half + 0.09 * unit).into(), on(offset, angle));
                }
            }
            _ => {}
        }
        if let Some(label) = control.label() {
            let size = match label.len() {
                0..=2 => 0.42,
                3 => 0.32,
                _ => 0.25,
            };
            commands.spawn((
                Part {
                    control,
                    disc: false,
                },
                Presents,
                Text2d::new(label),
                TextFont::from_font_size(size * unit),
                TextColor(Color::srgba(1.0, 1.0, 1.0, 0.85)),
                at(1.0),
                layer.clone(),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PHONE: Vec2 = Vec2::new(2340.0, 1080.0);

    #[test]
    fn every_button_lies_on_the_screen_and_none_on_another() {
        for window in [PHONE, Vec2::new(1920.0, 1080.0), Vec2::new(1280.0, 960.0)] {
            let layout = Layout::of(window);
            for placed in &layout.buttons {
                let edge = placed.at.abs() + Vec2::splat(placed.radius);
                assert!(
                    edge.x <= window.x / 2.0 && edge.y <= window.y / 2.0,
                    "{placed:?}"
                );
                assert_eq!(layout.button_at(placed.at), Some(placed.control));
            }
            for (i, a) in layout.buttons.iter().enumerate() {
                for b in &layout.buttons[i + 1..] {
                    assert!(a.at.distance(b.at) >= a.radius + b.radius, "{a:?} {b:?}");
                }
            }
        }
    }

    #[test]
    fn a_phone_keeps_the_stick_and_the_signs_beside_the_picture() {
        let layout = Layout::of(PHONE);
        let picture = PHONE.y * 4.0 / 3.0 / 2.0;
        for placed in
            (layout.buttons.iter()).filter(|placed| Control::FACES.contains(&placed.control))
        {
            assert!(placed.at.x - placed.radius >= picture, "{placed:?}");
        }
        assert!(layout.stick_home.x + STICK_RING * layout.unit <= -picture);
        assert!(layout.on_stick(layout.stick_home));
        assert!(!layout.on_stick(Vec2::ZERO));
    }

    #[test]
    fn the_stick_reaches_one_at_most_and_up_is_up() {
        assert_eq!(
            stick_of(Vec2::ZERO, Vec2::new(0.0, 50.0), 100.0),
            Vec2::new(0.0, 0.5)
        );
        let far = stick_of(Vec2::ZERO, Vec2::new(300.0, 400.0), 100.0);
        assert!((far.length() - 1.0).abs() < 1e-6);
        assert!((far - Vec2::new(0.6, 0.8)).length() < 1e-6);
    }

    #[test]
    fn a_thumb_between_two_signs_takes_the_nearer() {
        let layout = Layout::of(PHONE);
        let cross = layout.buttons[0].at;
        let circle = layout.buttons[1].at;
        let nearer_cross = cross.lerp(circle, 0.4);
        assert_eq!(layout.face_at(nearer_cross), Some(Control::Cross));
        assert_eq!(layout.face_at(Vec2::ZERO), None);
    }
}
