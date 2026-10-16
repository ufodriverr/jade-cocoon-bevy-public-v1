
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::display::{self, Presents, SCREEN, Wide};
use crate::field::Fitted;
use crate::prefs::Prefs;
use crate::shown::{self, Things};

pub struct WidescreenPlugin;

impl Plugin for WidescreenPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, reach_of_fights.before(shown::draw_things))
            .add_systems(PostUpdate, widen.before(display::Fitting));
    }
}

/// Pixels of the console's screen a picture of the window's shape, 240 lines high,
/// reaches beyond either side of the console's 320 columns: none in a window of 4:3 or
/// narrower.
pub fn reach(window: UVec2) -> i32 {
    if window.y == 0 || window.x * 3 <= window.y * 4 {
        return 0;
    }
    let columns = SCREEN.y as f32 * window.x as f32 / window.y as f32;
    ((columns - SCREEN.x as f32) / 2.0).ceil() as i32
}

/// What decides the shape of the picture in a frame.
#[derive(Clone, Copy, Debug, Default)]
struct Seen {
    /// `wide_fights` is on.
    on: bool,
    window: UVec2,
    /// A fight's picture is on screen ([`Things::fight_on_screen`]).
    fight: bool,
    /// The picture is held as it was last drawn.
    held: bool,
    /// Another screen's camera draws.
    covered: bool,
    /// The picture was wide in the frame before.
    was: bool,
}

/// Whether the picture takes the window's shape: the setting is on, the window is wider
/// than 4:3, a fight's picture is on screen and nothing of another screen covers it. A
/// held picture keeps the shape it had: the fight's last one, held on the way back
/// behind the results screen, does not come back wide for the moment before the room.
fn wanted(seen: Seen) -> bool {
    seen.on && reach(seen.window) > 0 && seen.fight && !seen.covered && (seen.was || !seen.held)
}

fn window_size(windows: &Query<&Window, With<PrimaryWindow>>) -> UVec2 {
    windows
        .single()
        .map_or(UVec2::ZERO, |window| window.physical_size())
}

/// How far a fight is drawn beyond the sides of the console's screen: as far as a wide
/// picture reaches, when the setting is on.
fn reach_of_fights(
    prefs: Option<Res<Prefs>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    things: Option<ResMut<Things>>,
) {
    let Some(mut things) = things else { return };
    let on = prefs.is_some_and(|prefs| prefs.display.wide_fights);
    let reach = match on {
        true => reach(window_size(&windows)),
        false => 0,
    };
    things.bypass_change_detection().set_reach(reach);
}

/// Sets [`Wide`] for the frame, in front of the picture's fitting.
fn widen(
    prefs: Option<Res<Prefs>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    things: Option<Res<Things>>,
    others: Query<&Camera, (Without<Presents>, Without<Fitted>)>,
    mut wide: ResMut<Wide>,
) {
    let things = things.as_deref();
    let wanted = wanted(Seen {
        on: prefs.is_some_and(|prefs| prefs.display.wide_fights),
        window: window_size(&windows),
        fight: things
            .is_some_and(|things| things.fight_on_screen() && things.picture_before().is_none()),
        held: things.is_some_and(Things::holds),
        covered: others.iter().any(|camera| camera.is_active),
        was: wide.0,
    });
    if wide.0 != wanted {
        match wanted {
            true => info!("the fight takes the window's shape"),
            false => info!("the picture is 4:3 again"),
        }
        wide.0 = wanted;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wide_window_reaches_beyond_the_consoles_columns() {
        // 16:9: 426.7 columns, 53.3 beyond either side, counted up.
        assert_eq!(reach(UVec2::new(1920, 1080)), 54);
        assert_eq!(reach(UVec2::new(1280, 720)), 54);
        // 16:10 and 21:9.
        assert_eq!(reach(UVec2::new(1920, 1200)), 32);
        assert_eq!(reach(UVec2::new(2560, 1080)), 125);
        // 4:3, a tall window and a window taken down to the task bar reach nowhere.
        assert_eq!(reach(UVec2::new(960, 720)), 0);
        assert_eq!(reach(UVec2::new(1080, 1920)), 0);
        assert_eq!(reach(UVec2::new(0, 0)), 0);
    }

    #[test]
    fn only_a_fight_alone_in_a_wide_window_with_the_setting_on_is_wide() {
        let fight = Seen {
            on: true,
            window: UVec2::new(1920, 1080),
            fight: true,
            ..Seen::default()
        };
        assert!(wanted(fight));
        assert!(!wanted(Seen { on: false, ..fight }));
        assert!(!wanted(Seen {
            window: UVec2::new(960, 720),
            ..fight
        }));
        assert!(!wanted(Seen {
            fight: false,
            ..fight
        }));
        assert!(!wanted(Seen {
            covered: true,
            ..fight
        }));
        // The fight's last picture, held, stays wide; held again behind the results
        // screen, it stays 4:3.
        let held = Seen {
            held: true,
            ..fight
        };
        assert!(wanted(Seen { was: true, ..held }));
        assert!(!wanted(held));
    }
}
