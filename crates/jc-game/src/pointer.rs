//! The mouse in the game's menus, a choice of the build for a PC (2026-10-14): the row
//! under the mouse takes the cursor, and a left click on a row confirms that row. The
//! game knows nothing of a mouse; the buttons are the pad's as before (`keys.rs`).
//!
//! A screen asks [`Pointer`] for the console's pixel under the mouse and finds its row
//! there. Its [`Aim`] then steers the cursor to that row with the presses of the
//! directions a pad would give, one a pass, so that the screen's own code moves the
//! cursor and plays its sound. A click on a row the cursor has not reached yet waits for
//! it: the cross reaches the screen in the pass the cursor gets there. A click beside the
//! rows is the cross of `keys.rs`, and a direction of the keys takes the cursor back
//! from the mouse until the mouse moves again.
//!
//! On a screen with an aim the wheel goes through the aim as well: each notch is a
//! press of up or down in a pass of its own, so that a spin moves the cursor a row a
//! pass and no notch is lost between the screen's passes. Other screens have the
//! wheel of `keys.rs`.
//!
//! Every screen draws into the picture of `display.rs`, which lies in the window as the
//! settings say: the mouse's place in the window is first turned into the picture's
//! pixel ([`crate::display::Frame::to_picture`]), and the camera of a screen finds the
//! console's pixel there. Off the picture, on the blurred sides, the mouse points at
//! nothing.
//!
//! A check run has a mouse of its own, since its window is hidden and has no focus:
//! `--point=X,Y@SECONDS;...` puts the mouse on a place of the console's screen (in its
//! 320 by 240 pixels; beyond them on the sides of a wide fight) from that second of the
//! run on, and `--click=SECONDS,...` presses the left button then, for a tenth of a
//! second. The seconds count from the start of the run, or with `--point-from-menu`
//! from the frame the last screen with an aim came up (a screen whose rows the mouse
//! points at), and the mouse does nothing while none is up.

use bevy::camera::RenderTarget;
use bevy::camera::visibility::RenderLayers;
use std::sync::atomic::{AtomicBool, Ordering};

use bevy::ecs::system::SystemParam;
use bevy::input::InputSystems;
use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use jc_formats::menu::Rect;
use jc_formats::pad::{self, Pad};

use crate::display::Frame;
use crate::field::Fitted;
use crate::keys::DIRECTIONS;

/// The console's screen.
const SCREEN: Vec2 = Vec2::new(320.0, 240.0);
/// Pixels of a touchpad's scroll that make one notch of the wheel.
const NOTCH_PIXELS: f32 = 40.0;
/// The most notches an aim keeps waiting: a spin does not run on long after it stops.
const NOTCHES_WAITING: i32 = 6;
/// Pixels of the console's screen a point may lie beside a row and still be on it.
const NEAR: i32 = 6;
/// Seconds the left button of a check run's click is held.
const CLICK_HELD: f32 = 0.1;

/// The mouse of this frame, read once in front of every screen.
pub struct PointerPlugin;

impl Plugin for PointerPlugin {
    fn build(&self, app: &mut App) {
        let args: Vec<String> = std::env::args().collect();
        let checked = Checked::of(&args);
        if let Some(checked) = &checked {
            info!(
                "the mouse of the check: points (second, place) {:?}, clicks {:?}",
                checked.points, checked.clicks
            );
        }
        app.insert_resource(Mouse {
            checked,
            ..Mouse::default()
        })
        .add_systems(PreUpdate, read_mouse.after(InputSystems));
    }
}

/// The mouse of a check run (`--point=` and `--click=`): where it is from each second
/// on, in pixels of the console's screen, and the seconds its left button goes down.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Checked {
    points: Vec<(f32, Vec2)>,
    clicks: Vec<f32>,
    /// `--point-from-menu`: the seconds count from the frame a screen with an aim came
    /// up, not from the start of the run.
    from_menu: bool,
}

impl Checked {
    /// The check's mouse of the run's options, none without `--point=` and `--click=`.
    fn of(args: &[String]) -> Option<Self> {
        let asked = |name: &str| args.iter().find_map(|arg| arg.strip_prefix(name));
        let (points, clicks) = (asked("--point="), asked("--click="));
        if points.is_none() && clicks.is_none() {
            return None;
        }
        let mut checked = Self {
            from_menu: args.iter().any(|arg| arg == "--point-from-menu"),
            ..Self::default()
        };
        for point in points
            .unwrap_or_default()
            .split(';')
            .filter(|p| !p.is_empty())
        {
            let (place, second) = point.split_once('@').unwrap_or((point, "0"));
            let parts: Vec<Option<f32>> = (place.split(','))
                .map(|part| part.trim().parse().ok())
                .collect();
            match (&parts[..], second.trim().parse::<f32>()) {
                (&[Some(x), Some(y)], Ok(second)) => {
                    checked.points.push((second, Vec2::new(x, y)));
                }
                _ => warn!("--point: {point:?} is not X,Y@SECONDS"),
            }
        }
        for click in clicks
            .unwrap_or_default()
            .split(',')
            .filter(|c| !c.is_empty())
        {
            match click.trim().parse() {
                Ok(second) => checked.clicks.push(second),
                Err(_) => warn!("--click: {click:?} is not a number of seconds"),
            }
        }
        checked.points.sort_by(|a, b| a.0.total_cmp(&b.0));
        Some(checked)
    }

    /// The place of the console's screen the mouse is on at a second of the run.
    fn at(&self, second: f32) -> Option<Vec2> {
        (self.points.iter().rev())
            .find(|(from, _)| *from <= second)
            .map(|(_, at)| *at)
    }

    /// Whether the left button is down in a frame of the run that ends at `second` and
    /// began at `before`: a click that went down in between is, however long the frame.
    fn holds(&self, before: f32, second: f32) -> bool {
        (self.clicks.iter()).any(|&click| {
            (click..click + CLICK_HELD).contains(&second) || (before < click && click <= second)
        })
    }
}

/// Where the mouse is and what it did in this frame.
#[derive(Resource, Debug, Default)]
pub struct Mouse {
    /// The place in the window, in logical pixels, while the mouse is over it and the
    /// window has the focus.
    at: Option<Vec2>,
    /// The place is another than in the frame before.
    moved: bool,
    /// The left button went down.
    clicked: bool,
    /// Whole notches of the wheel in this frame, up above 0, and a touchpad's pixels
    /// short of the next one.
    notches: i32,
    pixels: f32,
    /// An aim took the wheel in this frame, and one did in the frame before: the wheel
    /// of `keys.rs` leaves it to the aim.
    claimed: AtomicBool,
    claimed_before: bool,
    /// The mouse of a check run, the second of its clock its last frame ended at, and
    /// the second of the run its clock counts from.
    checked: Option<Checked>,
    second: f32,
    since: f32,
}

impl Mouse {
    /// Whether a check run moves the mouse (`--point=`, `--click=`): it counts as in a
    /// window with the focus.
    pub fn checked(&self) -> bool {
        self.checked.is_some()
    }

    /// The left button went down in this frame.
    pub fn clicked(&self) -> bool {
        self.clicked
    }

    /// Whole notches of the wheel in this frame, up above 0.
    pub fn notches(&self) -> i32 {
        self.notches
    }

    /// Whether a screen's aim took the wheel in the frame before: a screen whose cursor
    /// the mouse steers is up.
    pub fn wheel_taken(&self) -> bool {
        self.claimed_before
    }

    /// The mouse is at another place than in the frame before.
    pub fn moved(&self) -> bool {
        self.moved
    }

    /// A screen that steers its cursor by the mouse without an [`Aim`] takes the wheel
    /// for this frame, as an aim does (the PC Settings, `pc_settings.rs`).
    pub fn take_wheel(&self) {
        self.claimed.store(true, Ordering::Relaxed);
    }
}

pub(crate) fn read_mouse(
    windows: Query<&Window, With<PrimaryWindow>>,
    mut buttons: ResMut<ButtonInput<MouseButton>>,
    scroll: Res<AccumulatedMouseScroll>,
    time: Res<Time<Real>>,
    frame: Option<Res<Frame>>,
    touch: Option<Res<crate::touch::TouchPad>>,
    mut mouse: ResMut<Mouse>,
) {
    let mouse = &mut *mouse;
    // A phone's touch that is no button of the touch screen is the mouse (`touch.rs`).
    let touched = touch.and_then(|touch| touch.pointer());
    if let Some(checked) = &mouse.checked {
        // A check run's mouse: its place, and its left button as if it were pressed.
        // With `--point-from-menu` its seconds count from the frame a screen with an
        // aim came up, and it does nothing while none is.
        let claimed = mouse.claimed.swap(false, Ordering::Relaxed);
        let now = time.elapsed_secs();
        if checked.from_menu && claimed && !mouse.claimed_before {
            mouse.since = now;
        }
        mouse.claimed_before = claimed;
        let counts = claimed || !checked.from_menu;
        let (before, second) = (mouse.second, now - mouse.since);
        mouse.second = second;
        let window = windows.single().ok();
        let on_window = |at: Vec2| match (&frame, window) {
            (Some(frame), _) => frame.on_window(at),
            (None, Some(window)) => Some(at / SCREEN * window.size()),
            (None, None) => None,
        };
        let at = (checked.at(second).filter(|_| counts)).and_then(on_window);
        match counts && checked.holds(before, second) {
            true => buttons.press(MouseButton::Left),
            false => buttons.release(MouseButton::Left),
        }
        mouse.moved = at.is_some() && at != mouse.at;
        mouse.at = at;
        mouse.clicked = at.is_some() && buttons.just_pressed(MouseButton::Left);
        mouse.notches = 0;
        return;
    }
    // A window without the focus has no mouse, as in `keys.rs`.
    let focused = windows.single().is_ok_and(|window| window.focused) || touched.is_some();
    let at = touched.or_else(|| {
        (windows.single().ok())
            .filter(|_| focused)
            .and_then(Window::cursor_position)
    });
    mouse.moved = at.is_some() && at != mouse.at;
    mouse.at = at;
    mouse.clicked = at.is_some() && buttons.just_pressed(MouseButton::Left);
    mouse.claimed_before = mouse.claimed.swap(false, Ordering::Relaxed);
    mouse.notches = 0;
    if !focused {
        mouse.pixels = 0.0;
        return;
    }
    mouse.pixels += match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y * NOTCH_PIXELS,
        MouseScrollUnit::Pixel => scroll.delta.y,
    };
    while mouse.pixels.abs() >= NOTCH_PIXELS {
        let notch = mouse.pixels.signum();
        mouse.pixels -= notch * NOTCH_PIXELS;
        mouse.notches += notch as i32;
    }
}

/// The mouse with the cameras that show the screens, for the console's pixel under it.
#[derive(SystemParam)]
pub struct Pointer<'w, 's> {
    mouse: Res<'w, Mouse>,
    cameras: Query<
        'w,
        's,
        (
            &'static Camera,
            &'static GlobalTransform,
            Option<&'static RenderLayers>,
            &'static RenderTarget,
        ),
    >,
    places: Query<'w, 's, (&'static GlobalTransform, Option<&'static RenderLayers>)>,
    rooms: Query<'w, 's, &'static Camera, With<Fitted>>,
    frame: Option<Res<'w, Frame>>,
}

impl Pointer<'_, '_> {
    pub fn mouse(&self) -> &Mouse {
        &self.mouse
    }

    /// The pixel under the mouse of a picture of the console's screen whose middle is
    /// the entity's place (`crate::spawn_picture`), `size` pixels across and down.
    pub fn on_picture(&self, picture: Entity, size: (usize, usize)) -> Option<(i16, i16)> {
        let (place, layers) = self.places.get(picture).ok()?;
        self.on(place, layers, size)
    }

    /// The pixel under the mouse of the console's screen stretched to `window` logical
    /// pixels around the middle of the world (the title's pictures).
    pub fn on_stretched(&self, window: Vec2) -> Option<(i16, i16)> {
        let place = GlobalTransform::from(Transform::from_scale((window / SCREEN).extend(1.0)));
        self.on(&place, None, (SCREEN.x as usize, SCREEN.y as usize))
    }

    /// The pixel under the mouse of the console's screen as the camera of the room shows
    /// it in the picture (`display.rs`).
    pub fn on_room(&self) -> Option<(i16, i16)> {
        pixel(self.room_at()?)
    }

    /// The same as far as the room's camera reaches: beyond the console's 320 columns,
    /// left of 0 and right of 319, where a wide fight is drawn (`widescreen.rs`).
    pub fn on_fight(&self) -> Option<(i16, i16)> {
        reaching(self.room_at()?)
    }

    /// The place of the console's screen under the mouse as the camera of the room
    /// shows it, none off the room's picture.
    fn room_at(&self) -> Option<Vec2> {
        let at = self.at()?;
        let shown = self.rooms.iter().find_map(Camera::logical_viewport_rect)?;
        let screen = self.frame.as_ref().map_or(SCREEN, |frame| frame.screen());
        shown.contains(at).then(|| on_screen(at, shown, screen))
    }

    /// The mouse's pixel of the picture the screens draw into, none off it.
    fn at(&self) -> Option<Vec2> {
        let at = self.mouse.at?;
        match &self.frame {
            Some(frame) => frame.to_picture(at),
            None => Some(at),
        }
    }

    /// Whether a camera draws into the picture of the screens (the window, without it).
    fn draws_the_screens(&self, target: &RenderTarget) -> bool {
        match (&self.frame, target) {
            (Some(frame), RenderTarget::Image(image)) => image.handle == *frame.picture(),
            (None, RenderTarget::Window(_)) => true,
            _ => false,
        }
    }

    fn on(
        &self,
        place: &GlobalTransform,
        layers: Option<&RenderLayers>,
        (wide, high): (usize, usize),
    ) -> Option<(i16, i16)> {
        let at = self.at()?;
        let seen = layers.cloned().unwrap_or_default();
        // The camera that draws the picture into the screens' picture last.
        let (camera, eye, ..) = (self.cameras.iter())
            .filter(|(camera, _, layers, target)| {
                camera.is_active
                    && self.draws_the_screens(target)
                    && layers.cloned().unwrap_or_default().intersects(&seen)
            })
            .max_by_key(|(camera, ..)| camera.order)?;
        let world = camera.viewport_to_world_2d(eye, at).ok()?;
        let local = (place.affine().inverse()).transform_point3(world.extend(0.0));
        let size = Vec2::new(wide as f32, high as f32);
        pixel(Vec2::new(local.x + size.x / 2.0, size.y / 2.0 - local.y))
    }
}

/// The place of the console's screen under a pixel of the picture, where the camera of
/// the room shows `screen` units in `shown`: the console's 320 by 240 lie in the middle
/// of them (`display::Wide`).
fn on_screen(at: Vec2, shown: bevy::math::Rect, screen: Vec2) -> Vec2 {
    (at - shown.min) / shown.size() * screen - (screen - SCREEN) / 2.0
}

/// A place on the screen as its pixel, off the screen as none.
fn pixel(at: Vec2) -> Option<(i16, i16)> {
    let inside = (0.0..SCREEN.x).contains(&at.x) && (0.0..SCREEN.y).contains(&at.y);
    inside.then(|| (at.x.floor() as i16, at.y.floor() as i16))
}

/// A place on the screen as its pixel as far as a wide picture reaches across, none
/// above or below the screen.
fn reaching(at: Vec2) -> Option<(i16, i16)> {
    let inside = (-SCREEN.x..2.0 * SCREEN.x).contains(&at.x) && (0.0..SCREEN.y).contains(&at.y);
    inside.then(|| (at.x.floor() as i16, at.y.floor() as i16))
}

/// Whether a pixel of the screen lies in a rectangle.
pub fn inside(rect: Rect, (x, y): (i16, i16)) -> bool {
    let (x, y) = (i32::from(x), i32::from(y));
    let (left, top) = (i32::from(rect.x), i32::from(rect.y));
    (left..left + i32::from(rect.w)).contains(&x) && (top..top + i32::from(rect.h)).contains(&y)
}

/// The square of how far a pixel lies from a rectangle: 0 inside.
fn distance(rect: Rect, (x, y): (i16, i16)) -> i32 {
    let gap = |at: i16, from: i16, size: i16| {
        let (at, from) = (i32::from(at), i32::from(from));
        let to = from + i32::from(size) - 1;
        (from - at).max(at - to).max(0)
    };
    let (across, down) = (gap(x, rect.x, rect.w), gap(y, rect.y, rect.h));
    across * across + down * down
}

/// The row whose rectangle holds the pixel, or else the nearest one that lies at most
/// [`NEAR`] pixels away: the game's rows are small, and a hand is not that exact.
pub fn row_under<T: Copy>(
    rows: impl IntoIterator<Item = (T, Rect)>,
    at: Option<(i16, i16)>,
) -> Option<T> {
    let at = at?;
    (rows.into_iter())
        .map(|(row, rect)| (distance(rect, at), row))
        .filter(|(distance, _)| *distance <= NEAR * NEAR)
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, row)| row)
}

/// The row a screen's cursor is steered to by the mouse, and a click that waits for it.
/// A row is whatever the screen names its cursor's places by.
#[derive(Debug, Clone, Copy)]
pub struct Aim<T> {
    row: Option<T>,
    click: bool,
    /// Where the cursor stood in the pass that last pressed a direction for the row.
    from: Option<T>,
    /// The directions pressed for the row.
    steps: u8,
    /// Notches of the wheel not passed on yet, up above 0.
    notches: i32,
}

/// The most directions an aim presses: no menu of the game has as many places.
const STEPS: u8 = 64;

impl<T> Default for Aim<T> {
    fn default() -> Self {
        Self {
            row: None,
            click: false,
            from: None,
            steps: 0,
            notches: 0,
        }
    }
}

impl<T: Copy + PartialEq> Aim<T> {
    /// Once a frame: the row under the mouse, taken when the mouse moved over it or
    /// clicked it, and the wheel's notches, which this screen takes from `keys.rs`.
    pub fn look(&mut self, mouse: &Mouse, under: Option<T>) {
        mouse.claimed.store(true, Ordering::Relaxed);
        if mouse.notches != 0 {
            let notches = self.notches + mouse.notches;
            *self = Self::default();
            self.notches = notches.clamp(-NOTCHES_WAITING, NOTCHES_WAITING);
        }
        let Some(row) = under else { return };
        if mouse.moved || mouse.clicked {
            (self.row, self.from, self.steps) = (Some(row), None, 0);
        }
        self.click |= mouse.clicked;
    }

    /// Nothing is aimed at any more.
    pub fn forget(&mut self) {
        *self = Self::default();
    }

    /// Once a pass, behind the pad routine and in front of the screen: the cursor is at
    /// `at`, and `way` gives the directions that bring it a step nearer to a row. A
    /// direction that did not move the cursor in the pass before, a row no direction
    /// leads to, or one not reached in [`STEPS`] ends the aim.
    pub fn steer(&mut self, pad: &mut Pad, at: T, way: impl FnOnce(T) -> u16) {
        if pad.repeated & DIRECTIONS != 0 {
            self.forget();
            return;
        }
        // A notch of the wheel is a press of its own in every pass.
        if self.notches != 0 {
            let way = if self.notches > 0 { pad::UP } else { pad::DOWN };
            self.notches -= self.notches.signum();
            pad.pressed |= way;
            pad.repeated |= way;
            return;
        }
        let Some(row) = self.row else { return };
        if row == at {
            if std::mem::take(&mut self.click) {
                pad.pressed |= pad::CROSS;
                pad.repeated |= pad::CROSS;
            }
            (self.row, self.from) = (None, None);
            return;
        }
        let way = way(row) & DIRECTIONS;
        if way == 0 || self.from == Some(at) || self.steps >= STEPS {
            self.forget();
            return;
        }
        self.from = Some(at);
        self.steps += 1;
        pad.pressed |= way;
        pad.repeated |= way;
        // The click is for the row, not for the place the cursor passes.
        if self.click {
            pad.pressed &= !pad::CROSS;
            pad.repeated &= !pad::CROSS;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jc_formats::menu::{Cursor, cursor};

    fn moved() -> Mouse {
        Mouse {
            at: Some(Vec2::ZERO),
            moved: true,
            clicked: false,
            ..Mouse::default()
        }
    }

    fn clicked() -> Mouse {
        Mouse {
            clicked: true,
            ..moved()
        }
    }

    /// A pass of a column of four lines that wraps, as the title's menu: the cursor's
    /// own step with the words the aim leaves.
    fn pass(aim: &mut Aim<i16>, cursor: &mut Cursor, held: u16) -> Pad {
        let mut pad = Pad::default();
        (pad.held, pad.pressed, pad.repeated) = (held, held, held);
        let way = |row| cursor.way_to(row);
        aim.steer(&mut pad, cursor.at, way);
        cursor.step(pad.repeated);
        pad
    }

    fn column() -> Cursor {
        Cursor {
            columns: 1,
            rows: 4,
            lines: 4,
            flags: cursor::WRAPS_DOWN,
            at: 0,
            first: 0,
        }
    }

    #[test]
    fn the_cursor_walks_to_the_row_under_the_mouse_a_step_a_pass() {
        let (mut aim, mut cursor) = (Aim::default(), column());
        aim.look(&moved(), Some(3));
        let seen: Vec<i16> = (0..5)
            .map(|_| {
                pass(&mut aim, &mut cursor, 0);
                cursor.at
            })
            .collect();
        assert_eq!(seen, [1, 2, 3, 3, 3]);
        // A mouse that rests over a row does not take the cursor back from the keys.
        pass(&mut aim, &mut cursor, pad::UP);
        aim.look(&Mouse::default(), Some(3));
        pass(&mut aim, &mut cursor, 0);
        assert_eq!(cursor.at, 2);
    }

    #[test]
    fn a_click_on_a_row_waits_for_the_cursor_and_confirms_there() {
        let (mut aim, mut cursor) = (Aim::default(), column());
        aim.look(&clicked(), Some(2));
        let crosses: Vec<(i16, bool)> = [pad::CROSS, pad::CROSS, 0, 0]
            .into_iter()
            .map(|held| {
                let pad = pass(&mut aim, &mut cursor, held);
                (cursor.at, pad.pressed & pad::CROSS != 0)
            })
            .collect();
        // The cursor moves first; the cross goes down once, on the row.
        assert_eq!(crosses, [(1, false), (2, false), (2, true), (2, false)]);
        // On the row already, the click is the confirm of that pass.
        aim.look(&clicked(), Some(2));
        let pad = pass(&mut aim, &mut cursor, pad::CROSS);
        assert_eq!((cursor.at, pad.pressed & pad::CROSS), (2, pad::CROSS));
    }

    #[test]
    fn a_row_the_cursor_cannot_reach_is_given_up() {
        let (mut aim, mut cursor) = (Aim::default(), column());
        (cursor.lines, cursor.rows, cursor.flags) = (2, 2, 0);
        aim.look(&clicked(), Some(3));
        pass(&mut aim, &mut cursor, 0);
        assert_eq!(cursor.at, 1);
        // Down from the last line stays: the aim ends, and so does its click.
        pass(&mut aim, &mut cursor, 0);
        let pad = pass(&mut aim, &mut cursor, 0);
        assert_eq!((cursor.at, pad.repeated), (1, 0));
        assert!(aim.row.is_none() && !aim.click);
        // A cursor that wraps round past a row it never meets stops after its steps.
        let (mut aim, mut cursor) = (Aim::default(), column());
        (cursor.lines, cursor.rows) = (2, 2);
        aim.look(&moved(), Some(3));
        for _ in 0..=STEPS {
            pass(&mut aim, &mut cursor, 0);
        }
        assert!(aim.row.is_none());
    }

    #[test]
    fn each_notch_of_the_wheel_is_a_step_of_its_own_pass() {
        let (mut aim, mut cursor) = (Aim::default(), column());
        cursor.flags = 0;
        let spun = Mouse {
            notches: -2,
            ..Mouse::default()
        };
        aim.look(&spun, None);
        assert!(spun.claimed.load(Ordering::Relaxed));
        let seen: Vec<i16> = (0..4)
            .map(|_| {
                pass(&mut aim, &mut cursor, 0);
                cursor.at
            })
            .collect();
        assert_eq!(seen, [1, 2, 2, 2]);
        // A spin takes over from a row the mouse pointed at; a key ends the spin.
        aim.look(&moved(), Some(0));
        aim.look(
            &Mouse {
                notches: -5,
                ..Mouse::default()
            },
            None,
        );
        pass(&mut aim, &mut cursor, 0);
        assert_eq!(cursor.at, 3);
        pass(&mut aim, &mut cursor, pad::UP);
        pass(&mut aim, &mut cursor, 0);
        assert_eq!(cursor.at, 2);
    }

    #[test]
    fn a_point_just_beside_a_row_is_on_it() {
        let rect = |y| Rect {
            x: 100,
            y,
            w: 30,
            h: 12,
        };
        let rows = [(0, rect(100)), (1, rect(112))];
        // Above the first, left of the second, and too far away.
        assert_eq!(row_under(rows, Some((110, 96))), Some(0));
        assert_eq!(row_under(rows, Some((95, 115))), Some(1));
        assert_eq!(row_under(rows, Some((80, 115))), None);
        // Between two rows the nearer one.
        let apart = [(0, rect(100)), (1, rect(120))];
        assert_eq!(row_under(apart, Some((110, 114))), Some(0));
        assert_eq!(row_under(apart, Some((110, 117))), Some(1));
    }

    #[test]
    fn the_rooms_pixel_is_found_in_any_picture() {
        // A picture of 1440 by 1080 in a window of 1920 by 1080 (`display.rs`): the
        // camera of the room fills it.
        let shown = bevy::math::Rect::new(0.0, 0.0, 1440.0, 1080.0);
        let at = |x, y| pixel(on_screen(Vec2::new(x, y), shown, SCREEN));
        assert_eq!(at(0.0, 0.0), Some((0, 0)));
        assert_eq!(at(1439.0, 1079.0), Some((319, 239)));
        assert_eq!(at(720.0, 540.0), Some((160, 120)));
        // Whole factors: 4 pixels of the window to one of the console's.
        let whole = bevy::math::Rect::new(0.0, 0.0, 1280.0, 960.0);
        assert_eq!(
            pixel(on_screen(Vec2::new(7.0, 4.0), whole, SCREEN)),
            Some((1, 1))
        );
        // A wide picture: the console's screen is its middle, the sides lie off it.
        let wide = bevy::math::Rect::new(0.0, 0.0, 1920.0, 1080.0);
        let screen = Vec2::new(1920.0 / 1080.0 * 240.0, 240.0);
        let at = |x, y| pixel(on_screen(Vec2::new(x, y), wide, screen));
        assert_eq!(at(960.0, 540.0), Some((160, 120)));
        assert_eq!(at(240.0, 0.0), Some((0, 0)));
        assert_eq!(at(100.0, 500.0), None);
    }

    #[test]
    fn the_rows_of_a_fights_menu_are_found_in_a_wide_fight() {
        use crate::display::{Layout, Shaping};
        // The menu is a picture of the console's screen in the middle of the wide one
        // (`widescreen.rs`): a pixel of the console lies where the room's camera puts it.
        let rect = |y| Rect {
            x: 16,
            y,
            w: 60,
            h: 12,
        };
        let rows = [(0, rect(150)), (1, rect(162))];
        let wide = Shaping {
            wide: true,
            ..Shaping::default()
        };
        for window in [
            UVec2::new(1920, 1080),
            UVec2::new(1280, 720),
            UVec2::new(2560, 1080),
            UVec2::new(1920, 1200),
        ] {
            let layout = Layout::of(window, wide);
            let picture = layout.picture.as_vec2();
            let shown = bevy::math::Rect::from_corners(Vec2::ZERO, picture);
            let screen = layout.screen();
            let in_picture = |at: Vec2| (at + (screen - SCREEN) / 2.0) / screen * picture;
            let at = |x, y| pixel(on_screen(in_picture(Vec2::new(x, y)), shown, screen));
            assert_eq!(at(0.5, 0.5), Some((0, 0)), "{window}");
            assert_eq!(at(319.5, 239.5), Some((319, 239)), "{window}");
            assert_eq!(row_under(rows, at(20.5, 155.5)), Some(0), "{window}");
            assert_eq!(row_under(rows, at(75.5, 173.5)), Some(1), "{window}");
            // The columns beside the console's screen hold no row.
            assert_eq!(at(-2.0, 155.5), None, "{window}");
            assert_eq!(at(330.0, 155.5), None, "{window}");
        }
    }

    #[test]
    fn an_opponent_beside_the_consoles_columns_is_found_in_a_wide_fight() {
        use crate::display::{Layout, Shaping};
        // An opponent the fight's camera puts left of the console's screen, and one in
        // the middle (`shown::Things::opponents_on_screen`).
        let rect = |x| Rect {
            x,
            y: 90,
            w: 50,
            h: 60,
        };
        let fighters = [(0, rect(-45)), (1, rect(130))];
        let wide = Shaping {
            wide: true,
            ..Shaping::default()
        };
        for window in [UVec2::new(1920, 1080), UVec2::new(2560, 1080)] {
            let layout = Layout::of(window, wide);
            let picture = layout.picture.as_vec2();
            let shown = bevy::math::Rect::from_corners(Vec2::ZERO, picture);
            let screen = layout.screen();
            // The window's pixel of a place of the console's screen, as a check's mouse
            // puts it there, and back.
            let at = |x: f32, y: f32| {
                let window = layout.on_window(Vec2::new(x, y));
                reaching(on_screen(window, shown, screen))
            };
            assert_eq!(at(-30.5, 120.5), Some((-31, 120)), "{window}");
            assert_eq!(row_under(fighters, at(-30.5, 120.5)), Some(0), "{window}");
            assert_eq!(row_under(fighters, at(150.5, 100.5)), Some(1), "{window}");
            assert_eq!(row_under(fighters, at(100.5, 20.5)), None, "{window}");
            // Above and below the screen nothing is found.
            assert_eq!(at(10.0, -5.0), None, "{window}");
        }
        // A 4:3 window: the console's corners are the window's.
        let layout = Layout::of(UVec2::new(960, 720), Shaping::default());
        assert_eq!(layout.on_window(Vec2::ZERO), Vec2::ZERO);
        assert_eq!(layout.on_window(SCREEN), Vec2::new(960.0, 720.0));
        let wide = Layout::of(UVec2::new(1920, 1080), wide);
        assert_eq!(wide.on_window(Vec2::ZERO), Vec2::new(240.0, 0.0));
    }

    #[test]
    fn a_checks_mouse_points_and_clicks_by_its_seconds() {
        let args: Vec<String> = ["--point=10,20@1;30.5,40@0.5;bad@2", "--click=1.5,3"]
            .into_iter()
            .map(String::from)
            .collect();
        let checked = Checked::of(&args).expect("a check's mouse");
        assert!(!checked.from_menu);
        assert_eq!(checked.at(0.2), None);
        assert_eq!(checked.at(0.7), Some(Vec2::new(30.5, 40.0)));
        assert_eq!(checked.at(9.0), Some(Vec2::new(10.0, 20.0)));
        // The button is down for a tenth of a second, and a frame longer than that
        // still has it.
        assert!(checked.holds(1.48, 1.52) && checked.holds(1.52, 1.55));
        assert!(!checked.holds(1.62, 1.7));
        assert!(checked.holds(2.9, 3.4));
        assert!(Checked::of(&["--smoke".to_string()]).is_none());
    }

    #[test]
    fn a_pixel_is_inside_its_rectangle_up_to_the_far_edges() {
        let rect = Rect {
            x: 186,
            y: 84,
            w: 108,
            h: 16,
        };
        assert!(inside(rect, (186, 84)) && inside(rect, (293, 99)));
        assert!(!inside(rect, (294, 90)) && !inside(rect, (200, 100)));
        let rows = [(0, rect), (1, Rect { y: 108, ..rect })];
        assert_eq!(row_under(rows, Some((200, 110))), Some(1));
        assert_eq!(row_under(rows, None), None);
    }
}
