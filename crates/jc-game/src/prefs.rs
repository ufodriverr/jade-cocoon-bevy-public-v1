
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use jc_disc::{Controls, LocalConfig, Movement};
use jc_formats::options_screen;
use serde::{Deserialize, Serialize};

use crate::bindings::{KeyBindings, PadBindings};
use crate::display;
use crate::settings::Settings;

/// The first lines of the file.
const HEAD: &str = "# The settings of Jade Cocoon for a PC. The game writes this file whenever a \
                    setting changes;\n# a key that is left out has its default. See \
                    HOW-TO-PLAY.md.\n\n";
/// The first lines of the file beside a save.
const FEEL_HEAD: &str = "# When this save was made, its chapter, and its game feel (the settings of the build \
     for a PC),\n# which a load takes back.\n\n";
/// The ending of the file beside a save, in place of the `.dat` of the save's file.
pub const BESIDE_A_SAVE: &str = "pc.toml";

/// All the settings of the player.
#[derive(Resource, Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// `[game]`: the game's own options.
    pub game: GameOptions,
    /// `[audio]`, `[pace]`, `[prompts]`, `[assist]`: what a save keeps besides.
    #[serde(flatten)]
    pub feel: GameFeel,
    /// `[display]`: the window and the picture.
    pub display: Display,
    /// `[controls]`: the keyboard, the mouse and the pad.
    pub controls: ControlPrefs,
    /// `[versus]`: the fight of two players (`lobby.rs`).
    pub versus: VersusPrefs,
}

/// `[versus]`: what the lobby of the fight of two players remembers on this machine
/// (`lobby.rs`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VersusPrefs {
    /// The player's name in the arena; empty for the hero's name of the save.
    pub name: String,
    /// The address last joined by hand (`IP` or `IP:PORT`).
    pub last_address: String,
    /// The TCP port a host waits on (47820); "who hosts?" goes to UDP 47821.
    pub port: u16,
}

impl Default for VersusPrefs {
    fn default() -> Self {
        Self {
            name: String::new(),
            last_address: String::new(),
            port: jc_net::PORT,
        }
    }
}

/// The settings that make the game feel the way the player wants it, which every save
/// keeps beside it and a load takes back.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GameFeel {
    /// `[audio]`: the volumes for a PC.
    pub audio: Audio,
    /// `[pace]`: the speed of the game.
    pub pace: Pace,
    /// `[prompts]`: the buttons the help lines show.
    pub prompts: Prompts,
    /// `[assist]`: help for the player.
    pub assist: Assist,
    /// `[saves]`: the build's own saves.
    pub saves: SavePrefs,
}

/// `[game]`: what the options screen reads and writes (`options_screen::Settings`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GameOptions {
    #[serde(with = "hex_word")]
    pub word: u32,
    /// The window colour, 0 to 3.
    pub colour: u16,
    /// The pad is not shaken.
    pub vibration_off: bool,
}

impl Default for GameOptions {
    fn default() -> Self {
        Self::of(options_screen::Settings::at_boot())
    }
}

impl GameOptions {
    pub fn of(settings: options_screen::Settings) -> Self {
        Self {
            word: settings.word,
            colour: settings.colour,
            vibration_off: settings.vibration_off,
        }
    }

    /// The game's settings of these options; a colour out of the four is the first.
    pub fn settings(&self) -> options_screen::Settings {
        options_screen::Settings {
            word: self.word,
            colour: match self.colour {
                colour @ 0..=3 => colour,
                _ => 0,
            },
            vibration_off: self.vibration_off,
        }
    }
}

/// `[audio]`: the volumes of a PC (`audio.rs`), each from 0 (silent) to 100 (as the
/// console, the default). They multiply with the game's own volumes of the options
/// screen (Music and Effects, Mute and 1 to 7), which keep working as in the original.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Audio {
    /// Everything the game sounds.
    pub master: u32,
    /// The music.
    pub music: u32,
    /// The sounds: steps, hits, the menus' sounds and the like.
    pub effects: u32,
    /// The spoken lines.
    pub voices: u32,
    /// The sound of the movies.
    pub movies: u32,
}

impl Default for Audio {
    fn default() -> Self {
        Self {
            master: 100,
            music: 100,
            effects: 100,
            voices: 100,
            movies: 100,
        }
    }
}

/// `[pace]`: how fast the game's clock runs (`pace.rs`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pace {
    /// How many times as fast the clock runs while the fast forward is held: 2, 3, 4 or
    /// 8.
    pub fast_forward_speed: u32,
    /// How many times as fast the clock runs while a fight is on screen, from its start
    /// to the end of the screen behind it: 1, 2 or 3. It multiplies with the fast
    /// forward.
    pub fight_speed: u32,
    /// Which scenes a held Start skips (`skip.rs`): `seen` (the scenes seen to their end
    /// before, the default), `always` (any) or `off`.
    pub scene_skip: crate::skip::SceneSkip,
}

impl Default for Pace {
    fn default() -> Self {
        Self {
            fast_forward_speed: crate::pace::FAST_FORWARD_SPEED,
            fight_speed: 1,
            scene_skip: crate::skip::SceneSkip::default(),
        }
    }
}

/// `[prompts]`: the buttons the help lines show (`prompts.rs`), and the hint over the
/// hero (`hint.rs`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prompts {
    /// `auto` (the keys of the keyboard while it is played with, an Xbox pad's buttons
    /// while a gamepad is), `playstation` (the game's own signs), `keyboard` or `xbox`.
    pub style: crate::prompts::Style,
    /// The confirm button floats over the hero's head while a press of it would do
    /// something where he stands: a talk, a door, the light to save at and the like.
    pub interaction_hint: bool,
}

impl Default for Prompts {
    fn default() -> Self {
        Self {
            style: crate::prompts::Style::default(),
            interaction_hint: true,
        }
    }
}

/// `[assist]`: help for the player (`assist.rs`); the defaults are the original game.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Assist {
    /// The experience of a won fight, times this: 0.5 to 4.0, 1.0 by default.
    pub exp_multiplier: f64,
    /// The money the player is given or sells for, times this: 0.5 to 4.0, 1.0 by
    /// default.
    pub money_multiplier: f64,
    /// `original` (a room whose creatures are all gone gets one on two visits of three)
    /// or `always` (every roaming creature beaten or fled from is back the next time
    /// the room is entered).
    pub respawn: crate::assist::Respawn,
    /// `normal` or `fewer` (half the roaming creatures a room is filled with, rounded
    /// up).
    pub encounters: crate::assist::Encounters,
}

impl Default for Assist {
    fn default() -> Self {
        Self {
            exp_multiplier: 1.0,
            money_multiplier: 1.0,
            respawn: crate::assist::Respawn::Original,
            encounters: crate::assist::Encounters::Normal,
        }
    }
}

/// `[saves]`: the build's quick save and autosave (`quick.rs`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavePrefs {
    /// The game is saved by itself into one of three autosaves on the way into a forest
    /// and at the start of a chapter.
    pub autosave: bool,
}

impl Default for SavePrefs {
    fn default() -> Self {
        Self { autosave: true }
    }
}

/// `[display]`: the window and the picture in it (`display.rs`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Display {
    /// `borderless` (the whole monitor), `fullscreen` (exclusive, at `resolution`) or
    /// `windowed`.
    pub mode: display::Mode,
    /// The video mode of exclusive fullscreen, and the size of a window that was never
    /// resized: `1920x1080`.
    pub resolution: display::Size,
    /// The size and the place of the window, kept whenever the player changes them.
    pub window_size: Option<display::Size>,
    pub window_position: Option<[i32; 2]>,
    pub vsync: bool,
    /// Frames a second at most; 0 is no cap.
    pub frame_cap: u32,
    /// What moves in a room or a fight is drawn between the game's 25 game frames a
    /// second too, at the monitor's own rate (`smooth.rs`); false steps 25 times a
    /// second as on the console.
    pub smooth_motion: bool,
    /// What a 4:3 screen does in a window of another shape: `fill`, `bars`, `stretch`
    /// or `zoom` (the movies and the title only).
    pub content: display::Content,
    /// The picture is a whole multiple of the console's screen.
    pub integer_scaling: bool,
    /// A fight takes the whole of a window wider than 4:3, its place and fighters seen
    /// further to the sides (`widescreen.rs`); false keeps it 4:3 as the other screens.
    pub wide_fights: bool,
    /// `sharp` (every screen drawn at the size it has on the monitor) or `authentic`
    /// (at the console's 320 by 240, enlarged).
    pub render: display::Render,
    /// Samples a pixel of the 3D's edges in `sharp`: 0 (none), 2, 4 or 8.
    pub msaa: u8,
    /// `off`, `scanlines` or `crt` (scanlines, a mask, a glow, darker corners).
    pub crt: display::Filter,
    /// The 3D dithered in the console's 15-bit colours (`authentic` only).
    pub dither: bool,
    /// The game stands and is silent while the window is in the background, and goes on
    /// where it was when the window is back in front (`audio.rs`).
    pub pause_on_focus_loss: bool,
    /// The game is silent while the window is in the background and goes on.
    pub mute_on_focus_loss: bool,
}

/// `smooth_motion` of a fresh `settings.toml`.
pub const SMOOTH_MOTION: bool = true;

impl Default for Display {
    fn default() -> Self {
        Self {
            mode: display::Mode::Borderless,
            resolution: display::Size::new(1920, 1080),
            window_size: None,
            window_position: None,
            vsync: true,
            frame_cap: 0,
            smooth_motion: SMOOTH_MOTION,
            content: display::Content::Fill,
            integer_scaling: false,
            wide_fights: true,
            render: display::Render::Sharp,
            msaa: 0,
            crt: display::Filter::Off,
            dither: false,
            pause_on_focus_loss: true,
            mute_on_focus_loss: false,
        }
    }
}

/// `[controls]`: how the keyboard and the pad steer the player, as the table `[controls]`
/// of `jc.local.toml` has it (`keys.rs`), and which key and which button presses what
/// (`[controls.keys]`, `[controls.pad]`, `bindings.rs`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ControlPrefs {
    /// `modern` or `tank`.
    pub movement: Movement,
    /// The directions alone run and Shift walks; false is the other way round.
    pub always_run: bool,
    /// Caps Lock (the `run_toggle` keys) turns the run round until it is pressed again.
    pub run_toggle: bool,
    /// `[controls.keys]`: the keys of every action.
    pub keys: KeyBindings,
    /// `[controls.pad]`: the gamepad's buttons of every action.
    pub pad: PadBindings,
}

impl Default for ControlPrefs {
    fn default() -> Self {
        Self::of(Controls::default())
    }
}

impl ControlPrefs {
    pub fn of(controls: Controls) -> Self {
        Self {
            movement: controls.movement,
            always_run: controls.always_run,
            run_toggle: true,
            keys: KeyBindings::default(),
            pad: PadBindings::default(),
        }
    }

    pub fn controls(&self) -> Controls {
        Controls {
            movement: self.movement,
            always_run: self.always_run,
        }
    }
}

/// The setting word as a text like `0x01F0283A`; a plain number is read too.
mod hex_word {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(word: &u32, to: S) -> Result<S::Ok, S::Error> {
        to.serialize_str(&format!("{word:#010X}").replacen("0X", "0x", 1))
    }

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Written {
        Text(String),
        Number(u32),
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(from: D) -> Result<u32, D::Error> {
        match Written::deserialize(from)? {
            Written::Number(word) => Ok(word),
            Written::Text(text) => {
                let digits = (text.trim().strip_prefix("0x"))
                    .or_else(|| text.trim().strip_prefix("0X"))
                    .unwrap_or(text.trim());
                u32::from_str_radix(digits, 16).map_err(serde::de::Error::custom)
            }
        }
    }
}

/// Where the settings are kept: the file, which a run that ends on its own does not
/// have, and the settings as the file has them now.
#[derive(Resource, Debug)]
pub struct PrefsFile {
    path: Option<PathBuf>,
    written: Option<Prefs>,
    /// What the start has to say, for the log, which is not up yet then.
    notes: Vec<String>,
}

impl PrefsFile {
    /// Whether this run keeps the settings: a play session, not a check.
    pub fn live(&self) -> bool {
        self.path.is_some()
    }
}

/// The table `[controls]` of `jc.local.toml`, with which the settings start when they
/// have no file yet.
fn seeded() -> Prefs {
    let controls = LocalConfig::load().map_or(Controls::default(), |config| config.controls);
    Prefs {
        controls: ControlPrefs::of(controls),
        ..default()
    }
}

/// The settings a run starts with: those of the file in a play session (`live`), made
/// from `jc.local.toml` when there is none yet; the defaults in a check.
pub fn at_start(live: bool) -> (Prefs, PrefsFile) {
    let prefs = seeded();
    if !live {
        let file = PrefsFile {
            path: None,
            written: None,
            notes: Vec::new(),
        };
        return (prefs, file);
    }
    let path = crate::paths::settings();
    let (prefs, written, note) = read(&path, prefs);
    let file = PrefsFile {
        path: Some(path),
        written,
        notes: vec![note],
    };
    (prefs, file)
}

/// The settings of the file at `path`, what of them the file has now, and what the log
/// says of it; `fresh` where it has none or a broken one, which is moved aside.
fn read(path: &Path, fresh: Prefs) -> (Prefs, Option<Prefs>, String) {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            let note = format!("the settings: {} is made", path.display());
            return (fresh, None, note);
        }
        Err(err) => {
            let note = format!("the settings: {} cannot be read: {err}", path.display());
            return (fresh, None, note);
        }
    };
    match toml::from_str::<Prefs>(&text) {
        Ok(prefs) => {
            let note = format!("the settings: read from {}", path.display());
            (prefs.clone(), Some(prefs), note)
        }
        Err(err) => {
            let aside = path.with_extension("broken.toml");
            let moved = std::fs::rename(path, &aside);
            let note = format!(
                "the settings: {} is broken ({err}), the defaults are taken; the broken file \
                 is moved to {}: {moved:?}",
                path.display(),
                aside.display()
            );
            (fresh, None, note)
        }
    }
}

/// The text of a file of the settings.
fn text(prefs: &Prefs) -> Result<String, toml::ser::Error> {
    Ok(format!("{HEAD}{}", toml::to_string_pretty(prefs)?))
}

/// Writes `text` to `path` through a file beside it, so that a broken write leaves the
/// old file whole.
pub(crate) fn write_whole(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let new = path.with_extension("toml.new");
    std::fs::write(&new, text)?;
    std::fs::rename(&new, path)
}

/// The game's options as they change reach the settings, and the settings reach the
/// file whenever they differ from it. A run without the file only follows the options.
pub fn keep(settings: Res<Settings>, mut prefs: ResMut<Prefs>, mut file: ResMut<PrefsFile>) {
    for note in std::mem::take(&mut file.notes) {
        info!("{note}");
    }
    // The settings a run starts with are the file's, or an option's (`--setting=`),
    // which is not kept: only a change counts.
    if settings.is_changed() && !settings.is_added() {
        let game = GameOptions::of(settings.0);
        if prefs.game != game {
            prefs.game = game;
        }
    }
    let Some(path) = file.path.clone() else {
        return;
    };
    if !prefs.is_changed() && file.written.is_some() {
        return;
    }
    if file.written.as_ref() == Some(&*prefs) {
        return;
    }
    let written = text(&prefs)
        .map_err(|err| err.to_string())
        .and_then(|text| write_whole(&path, &text).map_err(|err| err.to_string()));
    match written {
        Ok(()) => info!("the settings: written to {}", path.display()),
        Err(err) => warn!("the settings: not written to {}: {err}", path.display()),
    }
    // A failed write is not tried again in every frame, only at the next change.
    file.written = Some(prefs.clone());
}

/// The file beside the save in `save`, which keeps its game feel.
pub fn beside_save(save: &Path) -> PathBuf {
    save.with_extension(BESIDE_A_SAVE)
}

/// When a save was made and in which chapter, as the file beside it keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saved {
    /// The date and the time by the machine's clock: `2026-10-15 20:12`.
    pub saved_at: String,
    /// The chapter of the saved game (`Carried::chapter`).
    pub chapter: u32,
}

/// Keeps `feel` beside the save in `save`, with when and in which chapter it was made;
/// a run that keeps no settings has no `feel` to keep.
pub fn write_beside(
    save: &Path,
    feel: Option<&GameFeel>,
    saved: &Saved,
) -> Result<PathBuf, String> {
    let path = beside_save(save);
    let mut table = toml::Table::try_from(saved).map_err(|err| err.to_string())?;
    if let Some(feel) = feel {
        table.extend(toml::Table::try_from(feel).map_err(|err| err.to_string())?);
    }
    let text = toml::to_string_pretty(&table).map_err(|err| err.to_string())?;
    write_whole(&path, &format!("{FEEL_HEAD}{text}")).map_err(|err| err.to_string())?;
    Ok(path)
}

/// Keeps `feel` beside the save in `save`, with this moment and `chapter`.
pub fn write_feel(save: &Path, feel: Option<&GameFeel>, chapter: usize) -> Result<PathBuf, String> {
    let saved = Saved {
        saved_at: crate::local_time::now(),
        chapter: chapter as u32,
    };
    write_beside(save, feel, &saved)
}

/// The table of the file beside the save in `save`, when there is one that reads.
fn beside_table(save: &Path) -> Option<toml::Table> {
    let path = beside_save(save);
    let text = std::fs::read_to_string(&path).ok()?;
    match text.parse::<toml::Table>() {
        Ok(table) => Some(table),
        Err(err) => {
            warn!(
                "the settings of the save: {} is broken: {err}",
                path.display()
            );
            None
        }
    }
}

/// The game feel kept beside the save in `save`; nothing for a save without it (an
/// older one, or one a check made) or with a broken one.
pub fn read_feel(save: &Path) -> Option<GameFeel> {
    let table = beside_table(save)?;
    let tables = toml::Table::try_from(GameFeel::default()).ok()?;
    if !tables.keys().any(|key| table.contains_key(key)) {
        return None;
    }
    match table.try_into() {
        Ok(feel) => Some(feel),
        Err(err) => {
            warn!("the settings of the save beside {}: {err}", save.display());
            None
        }
    }
}

/// When the save in `save` was made and in which chapter, as the file beside it says;
/// nothing for an older save without it.
pub fn read_saved(save: &Path) -> Option<Saved> {
    beside_table(save)?.try_into().ok()
}

/// A load took the game feel of its save: the settings have it from here on.
pub fn take_feel(prefs: &mut Prefs, feel: Option<GameFeel>) {
    match feel {
        Some(feel) if prefs.feel != feel => {
            info!("the settings: the loaded save brings its game feel {feel:?}");
            prefs.feel = feel;
        }
        Some(_) => {}
        None => {
            info!("the settings: the loaded save has no game feel beside it, the settings stay")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_settings_go_to_a_file_and_back() {
        let mut prefs = Prefs::default();
        prefs.game.word = 0x0123_4567;
        prefs.game.colour = 2;
        prefs.controls.movement = Movement::Tank;
        prefs.display.window_size = Some(display::Size::new(1600, 900));
        prefs.display.window_position = Some([-10, 40]);
        prefs.display.content = display::Content::Bars;
        prefs.display.render = display::Render::Authentic;
        prefs.display.crt = display::Filter::Scanlines;
        let written = text(&prefs).unwrap();
        assert!(written.contains("word = \"0x01234567\""), "{written}");
        for line in [
            "mode = \"borderless\"",
            "resolution = \"1920x1080\"",
            "window_size = \"1600x900\"",
            "content = \"bars\"",
            "render = \"authentic\"",
            "msaa = 0",
            "crt = \"scanlines\"",
            "dither = false",
            "smooth_motion = ",
            "interaction_hint = true",
        ] {
            assert!(written.contains(line), "{line} in {written}");
        }
        for table in [
            "[game]",
            "[audio]",
            "[pace]",
            "[prompts]",
            "[assist]",
            "[saves]",
            "[display]",
            "[controls]",
            "[controls.keys]",
            "[controls.pad]",
            "[versus]",
        ] {
            assert!(written.contains(table), "{table} in {written}");
        }
        assert!(written.contains("port = 47820"), "{written}");
        assert!(written.contains("movement = \"tank\""), "{written}");
        assert_eq!(toml::from_str::<Prefs>(&written).unwrap(), prefs);
    }

    #[test]
    fn an_old_or_newer_file_still_loads() {
        assert_eq!(toml::from_str::<Prefs>("").unwrap(), Prefs::default());
        let text = "something = 1\n[game]\ncolour = 3\nlater = true\n[controls]\nalways_run = \
                    false\n[later]\nkey = 'value'\n";
        let prefs: Prefs = toml::from_str(text).unwrap();
        assert_eq!(prefs.game.colour, 3);
        assert_eq!(prefs.game.word, GameOptions::default().word);
        assert!(!prefs.controls.always_run);
        assert_eq!(prefs.controls.movement, Movement::Modern);
        // A file from before the bindings has the default ones.
        assert!(prefs.controls.run_toggle);
        assert_eq!(prefs.controls.keys, KeyBindings::default());
        let text = "[controls.pad]
cross = [\"B\"]
";
        let prefs: Prefs = toml::from_str(text).unwrap();
        assert_eq!(prefs.controls.pad.cross, ["B"]);
        assert_eq!(prefs.controls.pad.triangle, PadBindings::default().triangle);
        let prefs: Prefs = toml::from_str("[game]\nword = 1234\n").unwrap();
        assert_eq!(prefs.game.word, 1234);
        // The lobby's memory: a table of one key keeps the default port.
        let prefs: Prefs = toml::from_str("[versus]\nname = \"Koris\"\n").unwrap();
        assert_eq!(prefs.versus.name, "Koris");
        assert_eq!(prefs.versus.port, 47820);
        assert!(prefs.versus.last_address.is_empty());
        // A fresh file is fullscreen at 1920x1080; a table of a few keys keeps the rest.
        assert_eq!(prefs.display.mode, display::Mode::Borderless);
        let text = "[display]\nmode = \"windowed\"\nresolution = \"2560x1440\"\n";
        let prefs: Prefs = toml::from_str(text).unwrap();
        assert_eq!(prefs.display.resolution, display::Size::new(2560, 1440));
        assert_eq!(prefs.display.mode, display::Mode::Windowed);
        assert!(prefs.display.vsync && prefs.display.window_size.is_none());
    }

    #[test]
    fn the_options_word_is_the_boots_by_default() {
        let settings = Prefs::default().game.settings();
        assert_eq!(settings, options_screen::Settings::at_boot());
        let wrong = GameOptions {
            colour: 9,
            ..default()
        };
        assert_eq!(wrong.settings().colour, 0);
    }

    /// A folder of the tests' own under `target`, whose files each run writes over.
    fn scratch(name: &str) -> PathBuf {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch/prefs")
            .join(name);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn a_change_of_the_options_reaches_the_file() {
        let path = scratch("keep").join("settings.toml");
        let mut app = App::new();
        app.insert_resource(Settings::default())
            .insert_resource(Prefs::default())
            .insert_resource(PrefsFile {
                path: Some(path.clone()),
                written: None,
                notes: Vec::new(),
            })
            .add_systems(Update, keep);
        app.update();
        let read = |path: &Path| toml::from_str::<Prefs>(&std::fs::read_to_string(path).unwrap());
        assert_eq!(read(&path).unwrap(), Prefs::default());
        app.world_mut().resource_mut::<Settings>().0.colour = 2;
        app.update();
        assert_eq!(read(&path).unwrap().game.colour, 2);
        // The file and the settings agree: nothing more is written.
        let (prefs, written, _) = super::read(&path, Prefs::default());
        assert_eq!(prefs.game.colour, 2);
        assert_eq!(written, Some(prefs));
    }

    #[test]
    fn a_change_of_the_pc_settings_reaches_the_file() {
        let path = scratch("pc").join("settings.toml");
        let mut app = App::new();
        app.insert_resource(Settings::default())
            .insert_resource(Prefs::default())
            .insert_resource(PrefsFile {
                path: Some(path.clone()),
                written: None,
                notes: Vec::new(),
            })
            .add_systems(Update, keep);
        app.update();
        let mut prefs = app.world_mut().resource_mut::<Prefs>();
        prefs.display.mode = display::Mode::Windowed;
        prefs.feel.audio.music = 40;
        prefs.controls.keys.cross = vec!["KeyC".into()];
        app.update();
        let read = toml::from_str::<Prefs>(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(read.display.mode, display::Mode::Windowed);
        assert_eq!(read.feel.audio.music, 40);
        assert_eq!(read.controls.keys.cross, ["KeyC"]);
    }

    #[test]
    fn a_run_without_the_file_writes_nothing() {
        let mut app = App::new();
        app.insert_resource(Settings::default())
            .insert_resource(Prefs::default())
            .insert_resource(at_start(false).1)
            .add_systems(Update, keep);
        app.update();
        app.world_mut().resource_mut::<Settings>().0.colour = 1;
        app.update();
        assert_eq!(app.world().resource::<Prefs>().game.colour, 1);
        assert!(!app.world().resource::<PrefsFile>().live());
    }

    #[test]
    fn a_save_keeps_its_game_feel_and_a_load_takes_it() {
        let save = scratch("feel").join("save00.dat");
        let mut feel = GameFeel::default();
        feel.pace.fight_speed = 2;
        feel.saves.autosave = false;
        let path = write_feel(&save, Some(&feel), 3).unwrap();
        assert_eq!(path, beside_save(&save));
        assert_eq!(read_feel(&save), Some(feel.clone()));
        assert_eq!(read_feel(&save.with_file_name("save14.dat")), None);
        let mut prefs = Prefs::default();
        take_feel(&mut prefs, read_feel(&save));
        assert_eq!(prefs.feel, feel);
        let saved = read_saved(&save).unwrap();
        assert_eq!(saved.chapter, 3);
        assert_eq!(saved.saved_at.len(), "2026-10-15 20:12".len());
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("chapter = 3") && text.contains("[pace]"),
            "{text}"
        );
    }

    #[test]
    fn a_check_keeps_only_when_and_where_beside_a_save() {
        let save = scratch("saved").join("save01.dat");
        let saved = Saved {
            saved_at: "2026-10-15 20:12".into(),
            chapter: 1,
        };
        write_beside(&save, None, &saved).unwrap();
        assert_eq!(read_saved(&save), Some(saved));
        // Without the tables of the game feel a load leaves the settings as they are.
        assert_eq!(read_feel(&save), None);
        // An older file beside a save: the game feel alone.
        let feel = toml::to_string_pretty(&GameFeel::default()).unwrap();
        std::fs::write(beside_save(&save), feel).unwrap();
        assert_eq!(read_feel(&save), Some(GameFeel::default()));
        assert_eq!(read_saved(&save), None);
    }

    #[test]
    fn the_game_feel_lies_beside_the_save() {
        let save = Path::new("saves").join("card1").join("save03.dat");
        assert_eq!(
            beside_save(&save),
            Path::new("saves").join("card1").join("save03.pc.toml")
        );
        let feel = toml::to_string_pretty(&GameFeel::default()).unwrap();
        assert_eq!(
            toml::from_str::<GameFeel>(&feel).unwrap(),
            GameFeel::default()
        );
    }
}
