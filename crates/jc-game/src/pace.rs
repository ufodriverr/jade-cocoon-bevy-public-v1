
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

use crate::display::{self, Frame, Presents};
use crate::keys::Pad;
use crate::prefs::{self, Prefs};

/// The fast forward's speed by default, and the speeds it may have.
pub const FAST_FORWARD_SPEED: u32 = 4;
pub const FAST_FORWARD_SPEEDS: [u32; 4] = [2, 3, 4, 8];
/// The fight's speeds.
pub const FIGHT_SPEEDS: [u32; 3] = [1, 2, 3];
/// The fastest the clock runs.
pub const FASTEST: f64 = 8.0;

/// The fast forward's speed of the settings; one it may not have is the default.
pub fn fast_forward_speed(pace: &prefs::Pace) -> u32 {
    match FAST_FORWARD_SPEEDS.contains(&pace.fast_forward_speed) {
        true => pace.fast_forward_speed,
        false => FAST_FORWARD_SPEED,
    }
}

/// The fight's speed of the settings; one it may not have is 1.
pub fn fight_speed(pace: &prefs::Pace) -> u32 {
    match FIGHT_SPEEDS.contains(&pace.fight_speed) {
        true => pace.fight_speed,
        false => 1,
    }
}

/// The seconds of the game's clock a pass of a screen stands for: the time gone by, but
/// at most `longest` seconds of the real clock at the speed the clock runs (a pass that
/// took long does not make a screen catch up with a burst, and a fast clock still runs at
/// its whole speed). The speed is the one asked for now: a skip of a scene that stopped
/// in this frame (`skip.rs`) has put it back to 1, and a screen that came up then does
/// not take the skip's time.
pub fn passed(time: &Time<Virtual>, longest: f64) -> f64 {
    cut(time.delta_secs_f64(), time.relative_speed_f64(), longest)
}

/// `seconds` of the game's clock, at most `longest` seconds of the real one at `speed`.
fn cut(seconds: f64, speed: f64, longest: f64) -> f64 {
    seconds.min(longest * speed.max(1.0))
}

/// What the clock's speed is made of in a frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Asked {
    /// F2's speed.
    pub tester: f64,
    /// The fast forward is held, and its speed.
    pub held: bool,
    pub fast_forward: u32,
    /// The fight's speed while a fight is on.
    pub fight: Option<u32>,
    /// A movie is on screen.
    pub movie: bool,
    /// A scene is skipped (`skip.rs`).
    pub skipping: bool,
}

/// The speed of the clock.
pub fn speed(asked: Asked) -> f64 {
    if asked.movie {
        return 1.0;
    }
    if asked.skipping {
        return crate::skip::SPEED;
    }
    let held = if asked.held { asked.fast_forward } else { 1 };
    let player = f64::from(held * asked.fight.unwrap_or(1));
    asked.tester.max(player).clamp(1.0, FASTEST)
}

/// The pace of this run: F2's speed, the spans a check holds the fast forward in, and the
/// speed the clock runs at.
#[derive(Resource, Debug)]
pub struct Clock {
    tester: f64,
    holds: Vec<(f64, f64)>,
    speed: f64,
    /// The window is in the background and the game stands (`audio.rs`).
    away: bool,
    /// A scene is skipped: no marker.
    skipping: bool,
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            tester: 1.0,
            holds: Vec::new(),
            speed: 1.0,
            away: false,
            skipping: false,
        }
    }
}

impl Clock {
    /// The game stands while the window is in the background, from this frame on, and
    /// goes on where it was when it is not (`audio.rs`, E2).
    pub fn set_away(&mut self, away: bool) {
        if self.away != away {
            self.away = away;
        }
    }

    #[cfg(test)]
    pub fn away(&self) -> bool {
        self.away
    }
}

/// The actions `--hold=` knows: the fast forward, and Start held for the skip of a scene
/// (`skip.rs`).
const HELD: [&str; 2] = ["fast_forward", "skip"];

/// The spans of `--hold=FROM-UNTIL:ACTION,...` in which `action` is held: seconds of the
/// real clock. The actions are `fast_forward` and `skip`.
pub fn holds(value: &str, action: &str) -> Result<Vec<(f64, f64)>, String> {
    let spans = (value.split(',')).map(|span| {
        let wrong = || format!("--hold: {span:?} is not FROM-UNTIL:ACTION");
        let (times, held) = span.split_once(':').ok_or_else(wrong)?;
        let held = held.trim();
        if !HELD.contains(&held) {
            return Err(format!("--hold: no action {held:?} (fast_forward or skip)"));
        }
        let (from, until) = times.split_once('-').ok_or_else(wrong)?;
        let from: f64 = from.trim().parse().map_err(|_| wrong())?;
        let until: f64 = until.trim().parse().map_err(|_| wrong())?;
        match from <= until {
            true => Ok((held, (from, until))),
            false => Err(wrong()),
        }
    });
    let spans: Vec<(&str, (f64, f64))> = spans.collect::<Result<_, _>>()?;
    Ok((spans.into_iter())
        .filter(|(held, _)| *held == action)
        .map(|(_, span)| span)
        .collect())
}

/// The options of a check that change the pace for one run: `--fast-forward-speed=N`,
/// `--fight-speed=N` and `--scene-skip=seen|always|off`.
pub fn for_a_check(pace: &mut prefs::Pace, args: &[String]) {
    let option = |name: &str| args.iter().find_map(|arg| arg.strip_prefix(name));
    if let Some(value) = option("--fast-forward-speed=") {
        pace.fast_forward_speed = value
            .parse()
            .expect("--fast-forward-speed=N takes 2, 3, 4 or 8");
    }
    if let Some(value) = option("--fight-speed=") {
        pace.fight_speed = value.parse().expect("--fight-speed=N takes 1, 2 or 3");
    }
    if let Some(value) = option("--scene-skip=") {
        pace.scene_skip = match value {
            "seen" => crate::skip::SceneSkip::Seen,
            "always" => crate::skip::SceneSkip::Always,
            "off" => crate::skip::SceneSkip::Off,
            _ => panic!("--scene-skip= takes seen, always or off"),
        };
    }
}

/// The clock's speed and its marker; F1 and F2 in a run with keys (`keys`), and the spans
/// of `--hold=` in a check.
pub struct PacePlugin {
    pub keys: bool,
}

impl Plugin for PacePlugin {
    fn build(&self, app: &mut App) {
        let holds = (std::env::args())
            .find_map(|arg| {
                arg.strip_prefix("--hold=")
                    .map(|value| holds(value, "fast_forward"))
            })
            .transpose()
            .unwrap_or_else(|err| panic!("{err}"))
            .unwrap_or_default();
        app.insert_resource(Clock { holds, ..default() })
            .add_systems(PreUpdate, set_speed.after(crate::keys::read_pad))
            .add_systems(PostUpdate, show_marker.after(display::Fitting));
        if self.keys {
            app.add_systems(PreUpdate, tester_keys.before(set_speed));
        }
    }
}

/// F2 makes the clock run faster, for a tester who has seen a scene before: 2, 4 and 8
/// times, then as it was; F1 puts it back at once (keys of the build, not of the game).
fn tester_keys(
    keys: Res<ButtonInput<KeyCode>>,
    pc: Option<Res<crate::pc_settings::PcSettings>>,
    mut clock: ResMut<Clock>,
) {
    // A rebinding that waits for a key refuses F1 and F2 and does nothing else with them.
    if pc.is_some_and(|pc| pc.capturing()) {
        return;
    }
    let tester = if keys.just_pressed(KeyCode::F1) {
        1.0
    } else if keys.just_pressed(KeyCode::F2) {
        match clock.tester {
            speed if speed < 2.0 => 2.0,
            speed if speed < 4.0 => 4.0,
            speed if speed < 8.0 => 8.0,
            _ => 1.0,
        }
    } else {
        return;
    };
    clock.tester = tester;
    info!("F2: the tester's speed is {tester}");
}

/// The speed of the clock in this frame, from the fast forward, the fight on screen, F2
/// and a movie.
#[allow(clippy::too_many_arguments)]
pub fn set_speed(
    real: Res<Time<Real>>,
    pad: Res<Pad>,
    prefs: Option<Res<Prefs>>,
    things: Option<Res<crate::shown::Things>>,
    results: Option<Res<crate::results_view::ResultsView>>,
    (playing, skip): (
        Option<Res<crate::movie_view::Playing>>,
        Option<Res<crate::skip::Skip>>,
    ),
    mut clock: ResMut<Clock>,
    mut time: ResMut<Time<Virtual>>,
) {
    let pace = prefs
        .map(|prefs| prefs.feel.pace.clone())
        .unwrap_or_default();
    let now = real.elapsed_secs_f64();
    let held = pad.fast_forward()
        || (clock.holds.iter()).any(|&(from, until)| (from..until).contains(&now));
    let fight = things.is_some_and(|things| things.fight_on()) || results.is_some();
    let asked = Asked {
        tester: clock.tester,
        held,
        fast_forward: fast_forward_speed(&pace),
        fight: fight.then(|| fight_speed(&pace)),
        movie: playing.is_some_and(|playing| playing.on_screen()),
        skipping: skip.is_some_and(|skip| skip.skipping()),
    };
    let speed = speed(asked);
    clock.skipping = asked.skipping;
    if speed != clock.speed {
        info!(
            "the clock runs {speed} times as fast (the fast forward {}, {}, F2 {}{}{})",
            match asked.held {
                true => format!("held at {}", asked.fast_forward),
                false => "not held".into(),
            },
            match asked.fight {
                Some(fight) => format!("a fight at {fight}"),
                None => "no fight".into(),
            },
            asked.tester,
            if asked.movie { ", a movie at 1" } else { "" },
            if asked.skipping {
                ", a scene skipped"
            } else {
                ""
            }
        );
        clock.speed = speed;
    }
    if time.relative_speed_f64() != speed {
        time.set_relative_speed_f64(speed);
    }
    // The window in the background holds the clock: no screen goes on, whatever its
    // speed, and nothing that goes by it is owed when it runs again.
    if clock.away != time.is_paused() {
        match clock.away {
            true => time.pause(),
            false => time.unpause(),
        }
        info!(
            "the game's clock {} (the window in the background)",
            if clock.away { "stands" } else { "runs again" }
        );
    }
}

/// A part of the marker.
#[derive(Component)]
struct Marker;

/// The marker in the top right corner of the window while the clock runs fast: two
/// arrows and the speed, over a dark box, drawn over the window (`OVER_THE_WINDOW`) and
/// made anew when the speed or the window's size changes.
fn show_marker(
    clock: Res<Clock>,
    frame: Option<Res<Frame>>,
    parts: Query<Entity, With<Marker>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut shown: Local<Option<(Vec2, u32)>>,
    mut commands: Commands,
) {
    let window = frame.and_then(|frame| frame.window());
    let speed = clock.speed.round() as u32;
    let marked = speed > 1 && !clock.skipping;
    let wanted = window.filter(|_| marked).map(|window| (window, speed));
    // A screen that took everything away may have taken the marker too.
    let gone = shown.is_some() && parts.is_empty();
    if *shown == wanted && !gone {
        return;
    }
    for part in &parts {
        commands.entity(part).despawn();
    }
    *shown = wanted;
    let Some((window, speed)) = wanted else {
        return;
    };
    // A unit of the marker: a 36th of the window's height, at least 12 pixels.
    let unit = (window.y / 36.0).max(12.0);
    let size = Vec2::new(4.4, 1.6) * unit;
    let margin = 0.6 * unit;
    let middle = window / 2.0 - margin - size / 2.0;
    let at = |x: f32, z: f32| Vec3::new(middle.x + x * unit, middle.y, z);
    let layer = RenderLayers::layer(display::OVER_THE_WINDOW);
    commands.spawn((
        Marker,
        Presents,
        Sprite::from_color(Color::srgba(0.0, 0.0, 0.0, 0.6), size),
        Transform::from_translation(at(0.0, 5.0)),
        layer.clone(),
    ));
    let arrow = meshes.add(Triangle2d::new(
        Vec2::new(-0.4, 0.45) * unit,
        Vec2::new(-0.4, -0.45) * unit,
        Vec2::new(0.4, 0.0) * unit,
    ));
    let white = materials.add(Color::WHITE);
    for x in [-1.45, -0.75] {
        commands.spawn((
            Marker,
            Presents,
            Mesh2d(arrow.clone()),
            MeshMaterial2d(white.clone()),
            Transform::from_translation(at(x, 6.0)),
            layer.clone(),
        ));
    }
    commands.spawn((
        Marker,
        Presents,
        Text2d::new(format!("{speed}x")),
        TextFont::from_font_size(1.1 * unit),
        TextColor(Color::WHITE),
        Transform::from_translation(at(0.95, 6.0)),
        layer,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asked() -> Asked {
        Asked {
            tester: 1.0,
            held: false,
            fast_forward: 4,
            fight: None,
            movie: false,
            skipping: false,
        }
    }

    #[test]
    fn the_clock_takes_the_larger_of_f2_and_the_player_and_never_more_than_8() {
        assert_eq!(speed(asked()), 1.0);
        let held = Asked {
            held: true,
            ..asked()
        };
        assert_eq!(speed(held), 4.0);
        // The fight's speed multiplies with the fast forward, up to 8.
        assert_eq!(
            speed(Asked {
                fight: Some(2),
                ..held
            }),
            8.0
        );
        assert_eq!(
            speed(Asked {
                fight: Some(3),
                ..held
            }),
            8.0
        );
        assert_eq!(
            speed(Asked {
                fight: Some(3),
                ..asked()
            }),
            3.0
        );
        // F2's speed and the player's: the larger.
        assert_eq!(
            speed(Asked {
                tester: 2.0,
                ..held
            }),
            4.0
        );
        assert_eq!(
            speed(Asked {
                tester: 8.0,
                ..asked()
            }),
            8.0
        );
        // A movie plays at its own pace.
        let movie = Asked {
            tester: 4.0,
            movie: true,
            ..held
        };
        assert_eq!(speed(movie), 1.0);
        // A skip of a scene runs past the cap; a movie stops it anyway.
        let skipping = Asked {
            skipping: true,
            ..held
        };
        assert_eq!(speed(skipping), crate::skip::SPEED);
        assert_eq!(
            speed(Asked {
                movie: true,
                ..skipping
            }),
            1.0
        );
    }

    #[test]
    fn a_speed_the_settings_may_not_have_is_the_default() {
        let mut pace = prefs::Pace::default();
        assert_eq!((fast_forward_speed(&pace), fight_speed(&pace)), (4, 1));
        pace.fast_forward_speed = 8;
        pace.fight_speed = 3;
        assert_eq!((fast_forward_speed(&pace), fight_speed(&pace)), (8, 3));
        pace.fast_forward_speed = 5;
        pace.fight_speed = 0;
        assert_eq!((fast_forward_speed(&pace), fight_speed(&pace)), (4, 1));
    }

    #[test]
    fn the_spans_of_a_check() {
        let fast = |value| holds(value, "fast_forward");
        assert_eq!(fast("2-6:fast_forward"), Ok(vec![(2.0, 6.0)]));
        assert_eq!(
            fast("1.5-2:fast_forward,10-12:fast_forward"),
            Ok(vec![(1.5, 2.0), (10.0, 12.0)])
        );
        assert!(fast("2-6:cross").is_err());
        assert!(fast("6-2:fast_forward").is_err());
        assert!(fast("2:fast_forward").is_err());
        // Each action takes its own spans.
        let both = "1-2:fast_forward,3-5:skip";
        assert_eq!(fast(both), Ok(vec![(1.0, 2.0)]));
        assert_eq!(holds(both, "skip"), Ok(vec![(3.0, 5.0)]));
    }

    #[test]
    fn a_pass_stands_for_at_most_the_longest_at_the_clocks_speed() {
        // 0.05 s of the real clock are 0.4 s of a clock at 8: more than 0.1, not cut.
        assert_eq!(cut(0.4, 8.0, 0.1), 0.4);
        // A pass of half a second is cut to a tenth at the clock's speed.
        assert_eq!(cut(4.0, 8.0, 0.1), 0.8);
        assert_eq!(cut(0.5, 1.0, 0.1), 0.1);
    }
}
