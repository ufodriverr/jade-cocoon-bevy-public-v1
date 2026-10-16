
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use jc_formats::pad;
use serde::{Deserialize, Serialize};

use crate::display::{self, Frame, Presents};
use crate::keys::Pad;
use crate::notice::Notices;
use crate::prefs::Prefs;
use crate::quick::Screens;
use crate::saving::PlayClock;
use crate::shown::Things;

/// Seconds of the real clock Start is held before the scene is skipped.
pub const HOLD: f64 = 1.0;
/// Seconds of the hold before the ring shows: a press that turns a page shows none.
pub const RING_FROM: f64 = 0.25;
/// Seconds of the hold on a scene not seen yet before "Not seen yet" is said.
const NOT_SEEN_AFTER: f64 = 0.5;
/// How many times as fast the game's clock runs while a scene is skipped.
pub const SPEED: f64 = 400.0;
/// The real time the game frames of one frame of the app may take while a scene is
/// skipped; the rest of the frame's game frames are dropped, as if the clock ran slower.
const BUDGET: Duration = Duration::from_millis(25);
/// Game frames the hero walks freely before a skip is over: between two scripts of one
/// scene the selector can be free for a game frame.
pub const FREE_FRAMES: u32 = 3;
/// Seconds of the real clock between two writes of `seen.toml`.
const WRITE_EVERY: f64 = 5.0;

/// `[pace] scene_skip`: which scenes a held Start skips.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SceneSkip {
    /// The scenes seen to their end before.
    #[default]
    Seen,
    /// Any scene.
    Always,
    /// None.
    Off,
}

/// A scene: the room and the number of the script's task that holds the game.
pub type Scene = (u16, i32);

/// What this game folder has seen (`seen.toml`).
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Seen {
    /// The scripts that held the game to their end: the room and the task's number.
    pub scenes: BTreeSet<(u16, i32)>,
    /// The groups of a room's text that a window showed: the room and the group.
    pub texts: BTreeSet<(u16, usize)>,
}

/// The first lines of `seen.toml`.
const HEAD: &str = "# The scenes and the texts this game folder has seen, for the skip of a scene \
                    (HOW-TO-PLAY.md).\n# The game writes this file; delete it and every scene \
                    is new again.\n\n";

impl Seen {
    /// The file's text.
    fn text(&self) -> Result<String, toml::ser::Error> {
        Ok(format!("{HEAD}{}", toml::to_string(self)?))
    }

    /// What the file at `path` holds: nothing where it is missing or broken.
    fn read(path: &std::path::Path) -> (Self, String) {
        match std::fs::read_to_string(path) {
            Ok(text) => match toml::from_str::<Seen>(&text) {
                Ok(seen) => {
                    let note = format!(
                        "the skip of a scene: {} names {} scenes and {} texts seen",
                        path.display(),
                        seen.scenes.len(),
                        seen.texts.len()
                    );
                    (seen, note)
                }
                Err(err) => {
                    let note = format!(
                        "the skip of a scene: {} is broken ({err}), nothing counts as seen",
                        path.display()
                    );
                    (Self::default(), note)
                }
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => (
                Self::default(),
                format!("the skip of a scene: no {} yet", path.display()),
            ),
            Err(err) => (
                Self::default(),
                format!(
                    "the skip of a scene: {} cannot be read: {err}",
                    path.display()
                ),
            ),
        }
    }
}

/// What the game is doing in a game frame, as far as the skip asks.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Now {
    /// The room on screen, where one is and its scripts run.
    pub room: Option<u16>,
    /// A screen stands over the room, or the game is somewhere a room does not run (a
    /// movie, the title, the map, the chapter screen, a load).
    pub screen: bool,
    pub fight: bool,
    pub question: bool,
    /// The hero walks freely.
    pub free: bool,
    /// The script that holds the game.
    pub holder: Option<i32>,
    /// The groups of the room's text the windows show.
    pub texts: Vec<usize>,
}

impl Now {
    /// The scene that holds the game.
    fn scene(&self) -> Option<Scene> {
        Some((self.room?, self.holder?))
    }
}

/// Whether a skip could start now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Can {
    /// No scene runs, or one the skip leaves alone at the moment.
    No,
    /// The scene has not been seen and the setting is `seen`.
    NotSeen(Scene),
    Skip(Scene),
}

/// Whether the scene of `now` can be skipped under `mode` with `seen`.
pub fn can(now: &Now, mode: SceneSkip, seen: &Seen) -> Can {
    let Some(scene) = now.scene() else {
        return Can::No;
    };
    if now.screen || now.fight || now.question || mode == SceneSkip::Off {
        return Can::No;
    }
    match mode == SceneSkip::Seen && !seen.scenes.contains(&scene) {
        true => Can::NotSeen(scene),
        false => Can::Skip(scene),
    }
}

/// Why a skip stops in the game frame of `now`, if it does; `free` counts the game
/// frames the hero has walked freely, this one with them.
pub fn stops(now: &Now, mode: SceneSkip, seen: &Seen, free: u32) -> Option<&'static str> {
    let Some(room) = now.room else {
        return Some("the room is gone");
    };
    let unseen_text = (now.texts.iter()).any(|group| !seen.texts.contains(&(room, *group)));
    let unseen_scene = now
        .scene()
        .is_some_and(|scene| !seen.scenes.contains(&scene));
    [
        (mode == SceneSkip::Off, "the setting is off"),
        (now.screen, "a screen comes up"),
        (now.fight, "a fight starts"),
        (now.question, "a question is asked"),
        (
            free >= FREE_FRAMES,
            "the hero walks freely: the scene is over",
        ),
        (
            mode == SceneSkip::Seen && unseen_scene,
            "a script not seen to its end takes the game",
        ),
        (
            mode == SceneSkip::Seen && unseen_text,
            "a text not shown before comes up",
        ),
    ]
    .into_iter()
    .find_map(|(stops, why)| stops.then_some(why))
}

/// A skip under way.
#[derive(Debug, Clone, Copy)]
struct Run {
    scene: Scene,
    /// The second of the real clock it started.
    began: f64,
    /// Game frames run, and the last ones in a row in which the hero walked freely.
    frames: u32,
    free: u32,
}

/// The skip of a scene: what was seen, the hold of Start and a skip under way.
#[derive(Resource, Debug, Default)]
pub struct Skip {
    seen: Seen,
    /// Where `seen.toml` is; none in a check, which keeps nothing.
    file: Option<PathBuf>,
    /// What was seen has changed since the file was written, and the second of the real
    /// clock it was written last.
    changed: bool,
    written: f64,
    notes: Vec<String>,
    /// Start went down while this scene ran: the second of the real clock, the scene,
    /// and whether it may be skipped (it may not under `seen` before it is seen).
    held: Option<(f64, Scene, bool)>,
    run: Option<Run>,
    /// Behind a skip the player's pad stays away from the room until Start and the
    /// confirm are up, or a held Enter would confirm what the skip stopped at.
    latched: bool,
    /// Start went down while the hero walked freely (Enter, the cross too, that starts
    /// a talk): it holds no scene that follows until it is let go.
    from_before: bool,
    /// The scene "Not seen yet" was said for.
    told: Option<Scene>,
    /// The scene of the last game frame, which is seen once it lets go of the game.
    holder: Option<Scene>,
    /// A check: every scene the setting allows is skipped at once (`--skip-scenes`), and
    /// the spans of the real clock Start is held in (`--hold=FROM-UNTIL:skip`).
    every: bool,
    spans: Vec<(f64, f64)>,
    /// When the game frames of this frame of the app began.
    began: Option<Instant>,
}

impl Skip {
    /// Whether a scene is skipped now.
    pub fn skipping(&self) -> bool {
        self.run.is_some()
    }

    /// Whether the screen is black and the room not drawn.
    pub fn covers(&self) -> bool {
        self.skipping()
    }

    /// Whether the player's keys and buttons are kept from the room: while a scene is
    /// skipped, and behind it until Start and the confirm are let go.
    pub fn holds_the_pad(&self) -> bool {
        self.skipping() || self.latched
    }

    /// How far the ring is filled, 0 to 1, while Start is held long enough to show it.
    fn ring(&self, now: f64) -> Option<f32> {
        let (since, ..) = self
            .held
            .filter(|&(.., skips)| skips && self.run.is_none())?;
        let held = now - since;
        (held >= RING_FROM).then(|| (held / HOLD).clamp(0.0, 1.0) as f32)
    }

    /// The scene is seen, its script having let go of the game.
    fn see(&mut self, scene: Scene) {
        if self.seen.scenes.insert(scene) {
            self.changed = true;
            info!(
                "the skip of a scene: script {} of room {} is seen to its end",
                scene.1, scene.0
            );
        }
    }

    /// The skip starts.
    fn start(&mut self, scene: Scene, now: f64, things: &mut Things) {
        info!(
            "the skip of a scene: script {} of room {} is skipped",
            scene.1, scene.0
        );
        self.run = Some(Run {
            scene,
            began: now,
            frames: 0,
            free: 0,
        });
        self.held = None;
        self.latched = true;
        things.set_skipping(true);
    }

    /// A quick load (F9, `quick.rs`) takes the room away: a skip under way stops, so
    /// that the room the load brings is not skipped in its turn, and Start held for one
    /// counts no more.
    pub fn stop_for_a_load(&mut self, now: f64) {
        self.held = None;
        self.stop("a quick load takes the room away", now, None);
    }

    /// The skip stops, for `why`.
    fn stop(&mut self, why: &str, now: f64, things: Option<&mut Things>) {
        let Some(run) = self.run.take() else {
            return;
        };
        info!(
            "the skip of script {} of room {} stops: {why}; {} game frames in {:.2} s",
            run.scene.1,
            run.scene.0,
            run.frames,
            now - run.began
        );
        self.latched = true;
        if let Some(things) = things {
            things.set_skipping(false);
        }
    }
}

/// The skip of a scene: the hold, the skip in every game frame, the ring and the cover.
pub struct SkipPlugin {
    /// A play session keeps what was seen in `seen.toml`; a check keeps nothing.
    pub live: bool,
}

impl Plugin for SkipPlugin {
    fn build(&self, app: &mut App) {
        let args: Vec<String> = std::env::args().collect();
        let spans = (args.iter())
            .find_map(|arg| arg.strip_prefix("--hold="))
            .map(|value| crate::pace::holds(value, "skip"))
            .transpose()
            .unwrap_or_else(|err| panic!("{err}"))
            .unwrap_or_default();
        let mut skip = Skip {
            every: args.iter().any(|arg| arg == "--skip-scenes"),
            spans,
            ..default()
        };
        if self.live {
            let path = crate::paths::seen();
            let (seen, note) = Seen::read(&path);
            (skip.seen, skip.file) = (seen, Some(path));
            skip.notes.push(note);
        }
        app.insert_resource(skip)
            .add_systems(
                PreUpdate,
                watch
                    .after(crate::keys::read_pad)
                    .before(crate::pace::set_speed),
            )
            .add_systems(FixedLast, each_game_frame)
            .add_systems(PostUpdate, show.after(display::Fitting))
            .add_systems(Last, keep);
        if args.iter().any(|arg| arg == "--state-log") {
            app.add_systems(FixedLast, note_state.after(each_game_frame));
        }
    }
}

/// What the game is doing, for [`can`] and [`stops`].
fn now_of(things: Option<&Things>, room: Option<&crate::ShowRoom>, screens: &Screens) -> Now {
    let Some((things, room)) = things.zip(room) else {
        return Now::default();
    };
    // The story's last picture rising out of the black stands for a screen too.
    let ending = things.ending().is_some();
    Now {
        room: Some(room.number),
        screen: !screens.none_over_the_room() || !screens.may_leave() || ending,
        fight: things.fight_on(),
        question: things.asks(),
        free: things.walks_freely(),
        holder: things.holder(),
        texts: things.said_groups(),
    }
}

/// Once a frame in front of the clock's speed: the hold of Start, the start of a skip,
/// and the stop of one whose room has gone between two game frames.
#[allow(clippy::too_many_arguments)]
fn watch(
    real: Res<Time<Real>>,
    keys: Res<Pad>,
    prefs: Option<Res<Prefs>>,
    mut things: Option<ResMut<Things>>,
    room: Option<Res<crate::ShowRoom>>,
    screens: Screens,
    mut notices: ResMut<Notices>,
    mut skip: ResMut<Skip>,
) {
    let skip = &mut *skip;
    skip.began = Some(Instant::now());
    for note in std::mem::take(&mut skip.notes) {
        info!("{note}");
    }
    let time = real.elapsed_secs_f64();
    let held = keys.held();
    if skip.latched && held & (pad::START | pad::CROSS) == 0 {
        skip.latched = false;
    }
    if held & pad::START == 0 {
        skip.from_before = false;
    }
    let mode = prefs.map_or_else(SceneSkip::default, |prefs| prefs.feel.pace.scene_skip);
    let now = now_of(things.as_deref(), room.as_deref(), &screens);
    if skip.run.is_some() {
        if now.room.is_none() || now.screen {
            let why = match now.room {
                None => "the room is gone",
                Some(_) => "a screen comes up",
            };
            skip.stop(why, time, things.as_deref_mut());
        }
        return;
    }
    let start = held & pad::START != 0
        || skip.every
        || (skip.spans.iter()).any(|&(from, until)| (from..until).contains(&time));
    let can = can(&now, mode, &skip.seen);
    let scene = match can {
        Can::Skip(scene) | Can::NotSeen(scene) if start && !skip.latched && !skip.from_before => {
            scene
        }
        _ => {
            skip.held = None;
            // Start held while the hero walks freely (Enter, the cross too, that starts
            // a talk) holds no scene that follows: it has to go down in the scene.
            if now.free && held & pad::START != 0 {
                skip.from_before = true;
            }
            return;
        }
    };
    let skips = matches!(can, Can::Skip(_));
    let since = match skip.held {
        Some((since, was, skipped)) if (was, skipped) == (scene, skips) => since,
        _ => {
            info!(
                "the skip of a scene: Start is held on script {} of room {}{}",
                scene.1,
                scene.0,
                if skips { "" } else { ", not seen yet" }
            );
            skip.held = Some((time, scene, skips));
            time
        }
    };
    match can {
        Can::NotSeen(_) => {
            if time - since >= NOT_SEEN_AFTER && skip.told != Some(scene) {
                skip.told = Some(scene);
                notices.say("Not seen yet");
            }
        }
        Can::Skip(_) if skip.every || time - since >= HOLD => {
            if let Some(things) = things.as_deref_mut() {
                skip.start(scene, time, things);
            }
        }
        _ => {}
    }
}

/// Behind every game frame: the scenes seen and the texts shown, and while a scene is
/// skipped whether it stops here or the frame of the app has had its share.
#[allow(clippy::too_many_arguments)]
fn each_game_frame(
    real: Res<Time<Real>>,
    prefs: Option<Res<Prefs>>,
    mut things: Option<ResMut<Things>>,
    room: Option<Res<crate::ShowRoom>>,
    screens: Screens,
    mut fixed: ResMut<Time<Fixed>>,
    (mut virtual_time, mut played): (ResMut<Time<Virtual>>, Option<ResMut<PlayClock>>),
    mut skip: ResMut<Skip>,
) {
    let skip = &mut *skip;
    let mode = prefs.map_or_else(SceneSkip::default, |prefs| prefs.feel.pace.scene_skip);
    let now = now_of(things.as_deref(), room.as_deref(), &screens);
    // A script that has let go of the game, or whose room has gone, is seen.
    let scene = now.scene();
    if skip.holder != scene {
        if let Some(seen) = skip.holder {
            skip.see(seen);
        }
        skip.holder = scene;
    }
    let Some(run) = skip.run.as_mut() else {
        // The texts the player is shown.
        if let Some(room) = now.room {
            for group in &now.texts {
                skip.changed |= skip.seen.texts.insert((room, *group));
            }
        }
        return;
    };
    // A room the skip has come to is told so in its first game frame.
    if things.as_deref().is_some_and(|things| !things.skipping())
        && let Some(things) = things.as_deref_mut()
    {
        things.set_skipping(true);
    }
    run.frames += 1;
    run.free = if now.free { run.free + 1 } else { 0 };
    let free = run.free;
    let time = real.elapsed_secs_f64();
    if let Some(why) = stops(&now, mode, &skip.seen, free) {
        skip.stop(why, time, things.as_deref_mut());
        // The game frames still owed in this frame of the app are dropped, and the next
        // frame's clock runs at 1 from its start, not at the skip's speed.
        let owed = fixed.overstep();
        fixed.discard_overstep(owed);
        if let Some(played) = played.as_mut() {
            played.unplayed(owed.as_secs_f64());
        }
        virtual_time.set_relative_speed_f64(1.0);
        return;
    }
    if skip.began.is_some_and(|began| began.elapsed() >= BUDGET) {
        let owed = fixed.overstep();
        fixed.discard_overstep(owed);
        // The play clock counts the game frames that ran, not those dropped.
        if let Some(played) = played.as_mut() {
            played.unplayed(owed.as_secs_f64());
        }
    }
}

/// `seen.toml` follows what was seen, now and then and when the app ends.
fn keep(real: Res<Time<Real>>, mut exits: MessageReader<AppExit>, mut skip: ResMut<Skip>) {
    let ends = exits.read().count() > 0;
    let time = real.elapsed_secs_f64();
    if !skip.changed || (time - skip.written < WRITE_EVERY && !ends) {
        return;
    }
    let Some(path) = skip.file.clone() else {
        skip.changed = false;
        return;
    };
    let written = (skip.seen.text())
        .map_err(|err| err.to_string())
        .and_then(|text| crate::prefs::write_whole(&path, &text).map_err(|err| err.to_string()));
    match written {
        Ok(()) => debug!("the skip of a scene: {} is written", path.display()),
        Err(err) => warn!(
            "the skip of a scene: {} is not written: {err}",
            path.display()
        ),
    }
    (skip.changed, skip.written) = (false, time);
}

/// A part of the ring or of the cover.
#[derive(Component)]
struct Part;

/// What the ring or the cover is made for: the window's size, and the cover or the
/// ring.
type Made = (Vec2, bool);

/// What [`show`] has made: for what, and the ring's arc with its share in sixtieths.
#[derive(Default)]
struct Shown {
    made: Option<Made>,
    arc: Option<(Handle<Mesh>, u32)>,
}

/// Sixtieths: the steps the ring fills in.
const STEPS: f32 = 60.0;

/// The ring while Start is held, and the black cover with "Skipping..." while a scene is
/// skipped: over the window (`OVER_THE_WINDOW`) as the marker of `pace.rs`, made anew
/// when one of them comes or the window's size changes. The ring's arc is a mesh that
/// is changed in place as it fills: a part made anew in every frame would never be
/// drawn, as a new mesh and a new text reach the picture a frame late.
#[allow(clippy::too_many_arguments)]
fn show(
    real: Res<Time<Real>>,
    skip: Res<Skip>,
    frame: Option<Res<Frame>>,
    parts: Query<Entity, With<Part>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut shown: Local<Shown>,
    mut commands: Commands,
) {
    let window = frame.and_then(|frame| frame.window());
    let ring = skip.ring(real.elapsed_secs_f64());
    let covers = skip.covers();
    let wanted = window
        .filter(|_| ring.is_some() || covers)
        .map(|window| (window, covers));
    let step = ring.map_or(0, |share| (share * STEPS) as u32);
    let unit = |window: Vec2| (window.y / 36.0).max(12.0);
    let inner = |window: Vec2| 1.05 * unit(window);
    let outer = |window: Vec2| 1.35 * unit(window);
    // A screen that took everything away may have taken the parts too.
    let gone = shown.made.is_some() && parts.is_empty();
    if shown.made == wanted && !gone {
        if let (Some((arc, made)), Some((window, false))) = (&mut shown.arc, wanted)
            && *made != step
        {
            *made = step;
            let mesh = ring_arc(inner(window), outer(window), step as f32 / STEPS);
            if let Err(err) = meshes.insert(arc.id(), mesh) {
                warn!("the ring of the skip: {err}");
            }
        }
        return;
    }
    for part in &parts {
        commands.entity(part).despawn();
    }
    *shown = Shown {
        made: wanted,
        arc: None,
    };
    let Some((window, covers)) = wanted else {
        return;
    };
    // A unit: a 36th of the window's height, at least 12 pixels, as the marker's.
    let unit = unit(window);
    let layer = RenderLayers::layer(display::OVER_THE_WINDOW);
    let corner = window / 2.0 * Vec2::new(1.0, -1.0);
    if covers {
        commands.spawn((
            Part,
            Presents,
            Sprite::from_color(Color::BLACK, window),
            Transform::from_xyz(0.0, 0.0, 1.0),
            layer.clone(),
        ));
        let at = corner + Vec2::new(-4.5, 1.6) * unit;
        commands.spawn((
            Part,
            Presents,
            Text2d::new("Skipping..."),
            TextFont::from_font_size(1.0 * unit),
            TextColor(Color::srgb(0.85, 0.85, 0.85)),
            Transform::from_translation(at.extend(2.0)),
            layer,
        ));
        return;
    }
    let (inner, outer) = (inner(window), outer(window));
    let middle = corner + Vec2::new(-unit - outer, unit + outer);
    commands.spawn((
        Part,
        Presents,
        Mesh2d(meshes.add(Circle::new(outer + 0.2 * unit))),
        MeshMaterial2d(materials.add(Color::srgba(0.0, 0.0, 0.0, 0.6))),
        Transform::from_translation(middle.extend(5.0)),
        layer.clone(),
    ));
    commands.spawn((
        Part,
        Presents,
        Mesh2d(meshes.add(Annulus::new(inner, outer))),
        MeshMaterial2d(materials.add(Color::srgba(1.0, 1.0, 1.0, 0.25))),
        Transform::from_translation(middle.extend(5.5)),
        layer.clone(),
    ));
    let arc = meshes.add(ring_arc(inner, outer, step as f32 / STEPS));
    shown.arc = Some((arc.clone(), step));
    commands.spawn((
        Part,
        Presents,
        Mesh2d(arc),
        MeshMaterial2d(materials.add(Color::WHITE)),
        Transform::from_translation(middle.extend(6.0)),
        layer.clone(),
    ));
    if let Some(glyph) = crate::prompts::start() {
        // The picture is the console's pixels: as large as the ring's hole allows.
        let size = Vec2::new(glyph.wide as f32, glyph.high as f32);
        let scale = (1.6 * inner / size.x).min(1.2 * inner / size.y);
        commands.spawn((
            Part,
            Presents,
            Sprite {
                image: images.add(glyph.image()),
                custom_size: Some(size * scale),
                ..default()
            },
            Transform::from_translation(middle.extend(6.0)),
            layer.clone(),
        ));
    }
    let words = middle + Vec2::new(-outer - 1.8 * unit, 0.0);
    commands.spawn((
        Part,
        Presents,
        Sprite::from_color(Color::srgba(0.0, 0.0, 0.0, 0.6), Vec2::new(2.8, 1.6) * unit),
        Transform::from_translation(words.extend(5.0)),
        layer.clone(),
    ));
    commands.spawn((
        Part,
        Presents,
        Text2d::new("Skip"),
        TextFont::from_font_size(0.9 * unit),
        TextColor(Color::WHITE),
        Transform::from_translation(words.extend(6.0)),
        layer,
    ));
}

/// The part of a ring between the radii `inner` and `outer` from its top clockwise,
/// `share` of the way round; both sides of its triangles face the camera.
fn ring_arc(inner: f32, outer: f32, share: f32) -> Mesh {
    let steps = ((share * 64.0).ceil() as u32).max(1);
    let mut positions = Vec::new();
    for step in 0..=steps {
        let angle = std::f32::consts::FRAC_PI_2
            - share * std::f32::consts::TAU * step as f32 / steps as f32;
        let (sine, cosine) = angle.sin_cos();
        positions.push([cosine * inner, sine * inner, 0.0]);
        positions.push([cosine * outer, sine * outer, 0.0]);
    }
    let indices: Vec<u32> = (0..steps)
        .flat_map(|step| {
            let at = 2 * step;
            [at, at + 1, at + 2, at + 1, at + 3, at + 2]
                .into_iter()
                .chain([at, at + 2, at + 1, at + 1, at + 2, at + 3])
        })
        .collect();
    let corners = positions.len();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; corners])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; corners])
    .with_inserted_indices(Indices::U32(indices))
}

/// The game's state in a few words, by which a run played and a run skipped are
/// compared (`--state-log`): sums of what the game carries, part by part. No count of
/// game frames and no clock, which a skip changes.
fn carried_line(carried: &jc_formats::carried::Carried) -> String {
    let sum = |part: &dyn std::fmt::Debug| -> u64 {
        // FNV-1a: the same in every run.
        format!("{part:?}")
            .bytes()
            .fold(0xcbf2_9ce4_8422_2325, |sum, byte| {
                (sum ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
            })
    };
    format!(
        "chapter {}, {} money, {} things of the story, what he has {:016x}, what he wears \
         {:?}, creatures {:016x}, caught {:016x}, roaming {:016x}, the story's words \
         {:016x}, the generator {:016x}",
        carried.chapter,
        carried.keeping.money(),
        carried.keeping.story_things(),
        sum(&carried.keeping),
        carried.worn,
        sum(&(&carried.player, &carried.creatures)),
        sum(&(&carried.caught, &carried.kinds)),
        sum(&(&carried.roaming, &carried.item_count)),
        sum(&(
            carried.owed,
            &carried.variants,
            carried.word,
            carried.place_word,
            &carried.corridor
        )),
        sum(&carried.random),
    )
}

/// The same with the room, the view and where the hero stands.
fn state_line(things: &Things, room: &crate::ShowRoom) -> String {
    let Some(carried) = things.carried() else {
        return format!("room {}: no scripts", room.number);
    };
    format!(
        "room {} ({}), view {}, the hero {:?}, {}",
        room.number,
        room.on_screen,
        things.view_shown(),
        things.stands(),
        carried_line(&carried)
    )
}

/// What `--state-log` saw in the game frame before: the room on screen, a question, a
/// fight, the hero walking freely.
#[derive(Default)]
struct Noted {
    room: Option<String>,
    asked: bool,
    fought: bool,
    free: bool,
}

/// `--state-log`: the game's state behind the game frame in which a room comes or goes
/// (to the place map, the chapter screen), a question or a fight starts, or the hero
/// walks freely again.
fn note_state(
    things: Option<Res<Things>>,
    room: Option<Res<crate::ShowRoom>>,
    game: Option<Res<crate::Game>>,
    mut noted: Local<Noted>,
) {
    let (Some(things), Some(room)) = (things, room) else {
        if let (Some(gone), Some(game)) = (noted.room.take(), game) {
            info!(
                "the state as the room goes: room {gone}, {}",
                carried_line(&game.0)
            );
        }
        *noted = Noted::default();
        return;
    };
    let events = [
        (noted.room.as_ref() != Some(&room.on_screen), "a room comes"),
        (things.asks() && !noted.asked, "a question is asked"),
        (things.fight_on() && !noted.fought, "a fight starts"),
        (
            things.walks_freely() && !noted.free,
            "the hero walks freely",
        ),
    ];
    for (_, event) in events.iter().filter(|(happens, _)| *happens) {
        info!("the state as {event}: {}", state_line(&things, &room));
    }
    *noted = Noted {
        room: Some(room.on_screen.clone()),
        asked: things.asks(),
        fought: things.fight_on(),
        free: things.walks_freely(),
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene_now() -> Now {
        Now {
            room: Some(85),
            holder: Some(3),
            ..Now::default()
        }
    }

    #[test]
    fn a_scene_is_a_script_that_holds_the_game_and_nothing_over_it() {
        let seen = Seen::default();
        let now = scene_now();
        assert_eq!(can(&now, SceneSkip::Always, &seen), Can::Skip((85, 3)));
        assert_eq!(can(&now, SceneSkip::Seen, &seen), Can::NotSeen((85, 3)));
        assert_eq!(can(&now, SceneSkip::Off, &seen), Can::No);
        let free = Now {
            holder: None,
            free: true,
            ..scene_now()
        };
        assert_eq!(can(&free, SceneSkip::Always, &seen), Can::No);
        for over in [
            Now {
                screen: true,
                ..scene_now()
            },
            Now {
                fight: true,
                ..scene_now()
            },
            Now {
                question: true,
                ..scene_now()
            },
            Now {
                room: None,
                ..scene_now()
            },
        ] {
            assert_eq!(can(&over, SceneSkip::Always, &seen), Can::No, "{over:?}");
        }
    }

    #[test]
    fn under_seen_only_a_scene_seen_to_its_end_is_skipped() {
        let mut seen = Seen::default();
        seen.scenes.insert((85, 3));
        assert_eq!(
            can(&scene_now(), SceneSkip::Seen, &seen),
            Can::Skip((85, 3))
        );
        // The same script of another room is another scene.
        let other = Now {
            room: Some(69),
            ..scene_now()
        };
        assert_eq!(can(&other, SceneSkip::Seen, &seen), Can::NotSeen((69, 3)));
    }

    #[test]
    fn a_skip_stops_where_the_player_is_wanted() {
        let mut seen = Seen::default();
        seen.scenes.insert((85, 3));
        seen.texts.insert((85, 7));
        let now = scene_now();
        assert_eq!(stops(&now, SceneSkip::Seen, &seen, 0), None);
        let question = Now {
            question: true,
            ..scene_now()
        };
        assert_eq!(
            stops(&question, SceneSkip::Always, &seen, 0),
            Some("a question is asked")
        );
        let fight = Now {
            fight: true,
            ..scene_now()
        };
        assert_eq!(
            stops(&fight, SceneSkip::Always, &seen, 0),
            Some("a fight starts")
        );
        let screen = Now {
            screen: true,
            ..scene_now()
        };
        assert_eq!(
            stops(&screen, SceneSkip::Always, &seen, 0),
            Some("a screen comes up")
        );
        assert_eq!(
            stops(&Now::default(), SceneSkip::Always, &seen, 0),
            Some("the room is gone")
        );
        // Free for a game frame or two between two scripts: on; for three: over.
        let free = Now {
            holder: None,
            free: true,
            ..scene_now()
        };
        assert_eq!(
            stops(&free, SceneSkip::Always, &seen, FREE_FRAMES - 1),
            None
        );
        assert!(stops(&free, SceneSkip::Always, &seen, FREE_FRAMES).is_some());
        // Under `seen`, a text or a script that is new stops it; under `always` not.
        let new_text = Now {
            texts: vec![7, 8],
            ..scene_now()
        };
        assert_eq!(
            stops(&new_text, SceneSkip::Seen, &seen, 0),
            Some("a text not shown before comes up")
        );
        assert_eq!(stops(&new_text, SceneSkip::Always, &seen, 0), None);
        let old_text = Now {
            texts: vec![7],
            ..scene_now()
        };
        assert_eq!(stops(&old_text, SceneSkip::Seen, &seen, 0), None);
        let new_script = Now {
            holder: Some(4),
            ..scene_now()
        };
        assert!(stops(&new_script, SceneSkip::Seen, &seen, 0).is_some());
        assert_eq!(stops(&new_script, SceneSkip::Always, &seen, 0), None);
    }

    #[test]
    fn what_was_seen_goes_through_its_file() {
        let mut seen = Seen::default();
        seen.scenes.extend([(85, 3), (69, 0), (1, 12)]);
        seen.texts.extend([(85, 7), (1, 0)]);
        let text = seen.text().unwrap();
        assert!(text.starts_with("# The scenes"));
        assert_eq!(toml::from_str::<Seen>(&text).unwrap(), seen);
        let folder =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-scratch/seen");
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("seen.toml");
        crate::prefs::write_whole(&path, &text).unwrap();
        assert_eq!(Seen::read(&path).0, seen);
        // A broken file or none: nothing is seen.
        std::fs::write(&path, "scenes = 3").unwrap();
        assert_eq!(Seen::read(&path).0, Seen::default());
        assert_eq!(Seen::read(&folder.join("none.toml")).0, Seen::default());
    }

    #[test]
    fn the_ring_shows_from_a_quarter_second_and_fills_in_one() {
        let mut skip = Skip {
            held: Some((10.0, (85, 3), true)),
            ..Skip::default()
        };
        assert_eq!(skip.ring(10.1), None);
        assert_eq!(skip.ring(10.5), Some(0.5));
        assert_eq!(skip.ring(12.0), Some(1.0));
        // A scene not seen yet shows no ring.
        skip.held = Some((10.0, (85, 3), false));
        assert_eq!(skip.ring(10.5), None);
        skip.run = Some(Run {
            scene: (85, 3),
            began: 11.0,
            frames: 0,
            free: 0,
        });
        assert_eq!(skip.ring(11.5), None);
        assert!(skip.holds_the_pad() && skip.covers());
    }

    #[test]
    fn the_arc_has_a_step_at_least() {
        let mesh = ring_arc(1.0, 2.0, 0.0);
        assert_eq!(mesh.count_vertices(), 4);
        let mesh = ring_arc(1.0, 2.0, 1.0);
        assert_eq!(mesh.count_vertices(), 2 * 65);
    }
}
