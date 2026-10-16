// A release exe for players opens no console window: its log goes to `logs\` of the game
// folder (`logging.rs`). A debug build keeps the console.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::time::SystemTime;

use bevy::asset::RenderAssetUsages;
use bevy::audio::AddAudioSource;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use jc_disc::{Archive, DiscImage, Executable, LocalConfig};
use jc_formats::background::{View, ViewHeader};
use jc_formats::battle::arena::{self, Cameras, Centre, Places, Shot};
use jc_formats::battle::record::Random;
use jc_formats::carried::Carried;
use jc_formats::chapters::{self, ChapterTable, VariantKey, VariantTable};
use jc_formats::field::RoomData;
use jc_formats::items::Kind;
use jc_formats::mesh::MeshBlock;
use jc_formats::model::Worn;
use jc_formats::mover;
use jc_formats::package::Package;
use jc_formats::roaming;
use jc_formats::rooms::{self, RoomLocation, RoomTable};
use jc_formats::scene::{Exit, KeepingCommand, LocationCommand, ScenePackage, ShowWay};
use jc_formats::tim;
use jc_formats::vram::Vram;

mod actors;
mod assist;
mod audio;
mod before;
mod bindings;
mod card_view;
mod chapter;
mod disc_prompt;
mod display;
mod duel_check;
mod end_view;
mod field;
mod fight_effects;
mod fight_menu_view;
mod gamepad;
mod guards;
mod hint;
mod keys;
mod lobby;
mod local_time;
mod logging;
mod merge_view;
mod model_view;
mod movie_sound;
mod movie_view;
mod music;
mod name_entry;
mod net;
mod notice;
mod options_view;
mod pace;
mod paths;
mod pc_settings;
mod place_map;
mod player_menu;
mod pointer;
mod prefs;
mod prompts;
mod quick;
mod results_view;
mod saving;
mod scripts;
mod settings;
mod shop_view;
mod shown;
mod skip;
mod smooth;
mod text_view;
mod title;
mod versus_check;
mod versus_view;
mod voices;
mod walker;
mod widescreen;

/// The window of a check run, and the units every screen of the console's shape but a
/// room is drawn in, three to a pixel of the console (`display.rs` lays them into the
/// window).
const WINDOW_SIZE: (u32, u32) = (960, 720);
/// Frames to run before a smoke exit or a screenshot when no count is given.
const DEFAULT_FRAMES: u32 = 120;
/// Frames to wait for the screenshot file before giving up.
const CAPTURE_TIMEOUT_FRAMES: u32 = 600;

/// `--smoke[=FRAMES]` runs the app for a fixed number of frames and exits. The routine's
/// verify gate uses it to prove the app starts.
///
/// `--screenshot=PATH` does the same and saves a PNG of the window before exiting, so a run
/// can look at what the app shows. The frame count still comes from `--smoke=FRAMES`.
/// `--screenshot-frames=N` saves N frames one after the other in place of one, as
/// `PATH` with `_00`, `_01` and so on before its extension (to look at the glide of
/// `smooth.rs` between two game frames).
///
/// Both keep the window hidden; `--show` puts it on screen as in a normal run.
#[derive(Resource)]
struct Smoke {
    frames_left: u32,
    capture: Option<Capture>,
}

struct Capture {
    path: PathBuf,
    /// When the screenshot was requested; `None` until the frame count has run out.
    requested: Option<SystemTime>,
    frames_waited: u32,
    /// The frames to save one after the other, and those asked for so far.
    frames: u32,
    taken: u32,
}

impl Capture {
    /// The file of the `nth` frame: the path itself for a single one.
    fn path_of(&self, nth: u32) -> PathBuf {
        if self.frames <= 1 {
            return self.path.clone();
        }
        let stem = self.path.file_stem().unwrap_or_default().to_string_lossy();
        let extension = self.path.extension().unwrap_or_default().to_string_lossy();
        self.path
            .with_file_name(format!("{stem}_{nth:02}.{extension}"))
    }
}

/// `--texture=FILE[:N]` is a debug view: it shows texture `N` (0 when left out) of file
/// `FILE` of the disc's archive, enlarged by a whole factor to fit the window.
#[derive(Resource)]
struct ShowTexture {
    file: usize,
    index: usize,
}

impl ShowTexture {
    fn parse(value: &str) -> Option<Self> {
        let (file, index) = value.split_once(':').unwrap_or((value, "0"));
        Some(Self {
            file: file.parse().ok()?,
            index: index.parse().ok()?,
        })
    }
}

#[derive(Resource)]
struct ShowRoom {
    number: u16,
    /// The room on screen in words: the one asked for, then the one behind the last exit.
    on_screen: String,
    chapter: Option<usize>,
    /// Things of the story the player has from the start (`--has=N,...`), a stand-in
    /// for the rooms before this one.
    has: Vec<i32>,
    /// Items the player has one of from the start (`--items=N,...`), a stand-in too.
    items: Vec<i32>,
    /// Minion skins the player has one of from the start (`--skins=N,...`: his things
    /// of kind 4, for the merge of the screen of file 44), a stand-in too.
    skins: Vec<i32>,
    /// Weapons, armours and third things the player is given at the start
    /// (`--gear=KIND:THING[:COUNT],...`, as `--menu-gear=`), a stand-in too.
    gear: Vec<(usize, i32, i32)>,
    /// Money the player is given at the start (`--money=N`), a stand-in too.
    money: i32,
    /// Empty cocoons the player is given at the start (`--cocoons=N`), a stand-in too.
    cocoons: i32,
    /// Story flags that are set from the start (`--flags=N,...`), a stand-in too.
    flags: Vec<u32>,
    /// Story variables with a value from the start (`--variables=N:V,...`), a stand-in
    /// too: 4:1 is a player who picked the first forest on the place map.
    variables: Vec<(u32, i32)>,
    view: Option<usize>,
    scroll: IVec2,
    walk_mesh: bool,
    show_list: Option<(usize, Option<u32>)>,
    /// Asked for, and the game frame to stand at when one was given.
    actors: Option<Option<u32>>,
    /// The move of the creatures' table that the creatures among the actors are given.
    creature_move: Option<actors::MoveAsked>,
    /// What `--carried`, `--walks` and `--path` ask of the actors.
    ordered: actors::Ordered,
    /// The player, when asked for.
    player: Option<walker::Asked>,
    /// The scripts of the room run: the game frame to stand at when one was given, and
    /// whether the confirm button goes down by itself when a message stands.
    script: Option<(Option<u32>, bool)>,
    arena: Option<Option<u32>>,
}

impl ShowRoom {
    fn parse(value: &str) -> Option<Self> {
        let (number, view) = match value.split_once(':') {
            Some((number, view)) => (number, Some(view.parse().ok()?)),
            None => (value, None),
        };
        Some(Self {
            number: number.parse().ok()?,
            on_screen: String::new(),
            chapter: None,
            has: Vec::new(),
            items: Vec::new(),
            skins: Vec::new(),
            gear: Vec::new(),
            money: 0,
            cocoons: 0,
            flags: Vec::new(),
            variables: Vec::new(),
            view,
            scroll: IVec2::ZERO,
            walk_mesh: false,
            show_list: None,
            actors: None,
            creature_move: None,
            ordered: actors::Ordered::default(),
            player: None,
            script: None,
            arena: None,
        })
    }

    fn arrived(arrival: &chapter::Arrival, carried: &Carried) -> Option<Self> {
        Some(Self::entered(arrival.record.number?, carried))
    }

    fn entered(number: u16, carried: &Carried) -> Self {
        let confirms = std::env::args().any(|arg| arg == "--confirms");
        let of_room = |option: &str| {
            std::env::args().find_map(|arg| {
                let (room, value) = arg.strip_prefix(option)?.split_once(':')?;
                (room.parse() == Ok(number)).then(|| value.to_string())
            })
        };
        let pads = of_room("--pad-in=").map(|value| {
            walker::Asked::pads(&value).expect("--pad-in=ROOM:NAMES[;FRAME:NAMES...] is a --pad")
        });
        let crosses = of_room("--cross-in=").map(|value| {
            walker::Asked::crosses(&value).expect("--cross-in=ROOM:FRAME[,FRAME...] is a --cross")
        });
        Self {
            number,
            on_screen: String::new(),
            chapter: Some(carried.chapter),
            has: Vec::new(),
            items: Vec::new(),
            skins: Vec::new(),
            gear: Vec::new(),
            money: 0,
            cocoons: 0,
            flags: Vec::new(),
            variables: Vec::new(),
            view: None,
            scroll: IVec2::ZERO,
            walk_mesh: false,
            show_list: None,
            actors: Some(None),
            creature_move: None,
            ordered: actors::Ordered::default(),
            player: Some(walker::Asked {
                frame: None,
                pad: pads.map(|(first, _)| first),
                later: pads.map_or(walker::NO_LATER, |(_, later)| later),
                crosses: crosses.unwrap_or(walker::NO_CROSSES),
                worn: Worn::of(carried.worn),
                stands: None,
            }),
            script: Some((None, confirms)),
            arena: None,
        }
    }
}

/// `--room-log[=FILE]`, a help for the tester and not the game's (asked for by the user,
/// 2026-10-14): each room the game comes to, and each floor and node of the corridor of
/// the second round, is added as a line to FILE (`rooms-visited.txt` by default) with the
/// minutes and seconds since the start, so a hand-played walk says where it went.
#[derive(Resource)]
struct RoomLog {
    path: std::path::PathBuf,
    start: std::time::Instant,
    last: Option<(String, Option<(u32, usize)>)>,
}

impl RoomLog {
    fn asked(arg: &str) -> Option<Self> {
        let path = match arg.strip_prefix("--room-log")? {
            "" => "rooms-visited.txt",
            rest => rest.strip_prefix('=')?,
        };
        Some(Self {
            path: path.into(),
            start: std::time::Instant::now(),
            last: None,
        })
    }

    fn write(&self, line: &str) {
        use std::io::Write;
        let written = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .and_then(|mut file| writeln!(file, "{line}"));
        if let Err(err) = written {
            warn!("--room-log: {}: {err}", self.path.display());
        }
    }
}

/// Writes a line of `--room-log` when the room on screen or the corridor's floor or node
/// is another than at the last line.
fn note_rooms(show: Res<ShowRoom>, things: Option<Res<shown::Things>>, mut log: ResMut<RoomLog>) {
    let corridor = things.as_ref().and_then(|things| things.corridor_at());
    let now = (show.on_screen.clone(), corridor);
    if log.last.as_ref() == Some(&now) {
        return;
    }
    if log.last.is_none() {
        log.write("--- a new start of the game");
    }
    let seconds = log.start.elapsed().as_secs();
    let place = match corridor {
        Some((floor, node)) => format!(" (corridor floor {floor}, node {node})"),
        None => String::new(),
    };
    log.write(&format!(
        "{:02}:{:02}  room {}{place}",
        seconds / 60,
        seconds % 60,
        now.0
    ));
    log.last = Some(now);
}

#[derive(Resource, Default)]
pub struct Game(pub Carried);

impl Game {
    /// Takes what the scripts of a room that goes have made of the state. A room whose
    /// scripts do not run has changed nothing.
    fn keep(&mut self, things: &shown::Things, room: &str) {
        let Some(carried) = things.carried() else {
            return;
        };
        if carried != self.0 {
            info!(
                "room {room} hands on: state word {}, {} variants, {} money, {} things of the \
                 story, weapon, armour and third thing {:?}",
                carried.word,
                carried.variants.entries().len(),
                carried.keeping.money(),
                carried.keeping.story_things(),
                carried.worn
            );
        }
        self.0 = carried;
    }
}

/// The value of `--show-list=LIST[:FRAME]`.
fn parse_show_list(value: &str) -> Option<(usize, Option<u32>)> {
    Some(match value.split_once(':') {
        Some((list, frame)) => (list.parse().ok()?, Some(frame.parse().ok()?)),
        None => (value.parse().ok()?, None),
    })
}

/// The value of `--scroll=X,Y`.
fn parse_scroll(value: &str) -> Option<IVec2> {
    let (x, y) = value.split_once(',')?;
    Some(IVec2::new(x.parse().ok()?, y.parse().ok()?))
}

fn main() -> AppExit {
    std::sync::LazyLock::force(&title::STARTED);
    let args: Vec<String> = std::env::args().collect();
    // `--duel-check=SEED`: a whole fight of two players without a screen, logged, and
    // the run ends (`duel_check.rs`). No window and no App.
    if let Some(seed) = duel_check::asked(&args) {
        let run = seed
            .map_err(Into::into)
            .and_then(|seed| duel_check::run(seed, &args));
        return match run {
            Ok(()) => AppExit::Success,
            Err(err) => {
                eprintln!("duel check: {err}");
                AppExit::error()
            }
        };
    }
    // `--write-versus-saves=DIR`: the saves of two teams for a check of the lobby of the
    // fight of two players (`duel_check.rs`), and the run ends.
    if let Some(folder) = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--write-versus-saves="))
    {
        return match duel_check::write_saves(std::path::Path::new(folder)) {
            Ok(written) => {
                for path in written {
                    println!("versus: a save of a team in {}", path.display());
                }
                AppExit::Success
            }
            Err(err) => {
                eprintln!("versus saves: {err}");
                AppExit::error()
            }
        };
    }
    // `--versus=host[:PORT]` / `--versus=join:ADDR`: one side of a fight of two players
    // over the network, played by a bot on 127.0.0.1, logged, and the run ends
    // (`versus_check.rs`). No window and no App.
    // With a window (`--versus-view`, or `--smoke`/`--screenshot=` for a hidden check) the
    // same option is the fight on screen (`versus_view.rs`).
    let versus_view = versus_view::shown(&args);
    if let Some(mode) = versus_check::asked(&args).filter(|_| !versus_view) {
        let run = mode
            .map_err(Into::into)
            .and_then(|mode| versus_check::run(mode, &args));
        return match run {
            Ok(()) => AppExit::Success,
            Err(err) => {
                println!("versus: {err}");
                AppExit::error()
            }
        };
    }
    let frames = args.iter().find_map(|arg| {
        let rest = arg.strip_prefix("--smoke")?;
        Some(rest.strip_prefix('=').and_then(|n| n.parse().ok()))
    });
    let screenshot = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--screenshot="))
        .map(PathBuf::from);

    let texture = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--texture="))
        .map(|value| ShowTexture::parse(value).expect("--texture=FILE[:N] takes numbers"));
    let room = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--room="))
        .map(|value| ShowRoom::parse(value).expect("--room=N[:VIEW] takes numbers"));
    // `--arena=ROOM[:FRAME]` is the room's view with the place of its fights in it; the
    // number behind the room is a game frame of the fight's opening, not a view.
    let arena = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--arena="))
        .map(|value| ShowRoom::parse(value).expect("--arena=ROOM[:FRAME] takes numbers"))
        .map(|room| ShowRoom {
            arena: Some(room.view.map(|frame| frame as u32)),
            view: None,
            ..room
        });
    let room = arena.or(room);
    let model = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--model="))
        .map(|value| {
            model_view::ShowModel::parse(value).expect("--model=FILE[:OBJECT] takes numbers")
        })
        .map(
            |model| match args.iter().find_map(|arg| arg.strip_prefix("--animation=")) {
                Some(value) => model
                    .animation(value)
                    .expect("--animation=N[:FRAME] takes numbers"),
                None => model,
            },
        )
        .map(
            |model| match args.iter().find_map(|arg| arg.strip_prefix("--action=")) {
                Some(value) => model
                    .action(value)
                    .expect("--action=N[:FRAME] takes numbers"),
                None => model,
            },
        )
        .map(
            |model| match args.iter().find_map(|arg| arg.strip_prefix("--fade=")) {
                Some(value) => model
                    .fade(value)
                    .expect("--fade=M:FRAMES takes an animation and a number of frames above 0"),
                None => model,
            },
        )
        .map(
            |model| match args.iter().find_map(|arg| arg.strip_prefix("--rig=")) {
                Some(value) => model
                    .rig(value)
                    .expect("--rig takes creature, human or none"),
                None => model,
            },
        )
        .map(
            |model| match args.iter().find_map(|arg| arg.strip_prefix("--light=")) {
                Some(value) => model
                    .light(value)
                    .expect("--light=ROOM[:VIEW] takes numbers"),
                None => model,
            },
        )
        .map(
            |model| match args.iter().find_map(|arg| arg.strip_prefix("--stage=")) {
                Some(value) => model.stage(value).expect("--stage=N takes a number"),
                None => model,
            },
        )
        .map(
            |model| match args.iter().find_map(|arg| arg.strip_prefix("--part=")) {
                Some(value) => model.part(value).expect("--part=N[:other] takes a number"),
                None => model,
            },
        );
    let text = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--text="))
        .map(|value| {
            text_view::ShowText::parse(value).expect(
                "--text=IMAGE:ADDRESS[:FONT[:ROW]] takes exe or a file, a hex address and numbers",
            )
        });
    let notice = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--notice="))
        .map(|value| text_view::ShowNotice::parse(value).expect("--notice=ROOM[:N] takes numbers"));
    // `--talk[=PASS]` and what goes with it: the window of `--says` runs.
    let talk = || {
        let value = |option: &str| args.iter().find_map(|arg| arg.strip_prefix(option));
        let stand = args
            .iter()
            .find_map(|arg| match arg.strip_prefix("--talk") {
                Some("") => Some(None),
                Some(pass) => Some(Some(pass.strip_prefix('=').unwrap_or(pass))),
                None => None,
            });
        text_view::Runs::parse(
            stand.flatten(),
            value("--keys="),
            value("--speaker="),
            value("--speed="),
            args.iter().any(|arg| arg == "--answers"),
        )
        .expect(
            "--talk[=PASS], --keys=PASS:BUTTON,..., --speaker=NAME and --speed=N take numbers, buttons and letters",
        )
    };
    let said = args.iter().find_map(|arg| arg.strip_prefix("--says="));
    // Beside `--room` the option is a script's message command in that room.
    let says_in_room = (room.as_ref()).zip(said).map(|(room, value)| {
        text_view::SaysInRoom::parse(room.number, value, talk())
            .expect("--room=N --says=GROUP[:THING[:OTHER]] takes numbers")
    });
    let says = said
        .filter(|_| room.is_none())
        .map(|value| {
            text_view::ShowPage::parse(value).expect(
                "--says=ROOM:GROUP[:PAGE[:LETTERS[:NAME]]] takes numbers and a name in letters",
            )
        })
        .map(|says| {
            let number = |option: &str| -> Option<usize> {
                (args.iter())
                    .find_map(|arg| arg.strip_prefix(option))
                    .map(|value| {
                        value
                            .parse()
                            .expect("--face=FILE and --open=PASSES take a number")
                    })
            };
            let says = says.with(
                number("--face="),
                number("--open=").map(|passes| passes as i32),
            );
            match args.iter().any(|arg| arg.starts_with("--talk")) {
                true => says.runs(talk()),
                false => says,
            }
        });
    let movie = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--movie="))
        .map(|value| movie_view::ShowMovie::parse(value).expect("--movie=N[:FRAME] takes numbers"));
    let press_start = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--press-start="))
        .map(|value| -> f64 { value.parse().expect("--press-start=SECONDS takes a number") });
    let scroll = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--scroll="))
        .map(|value| parse_scroll(value).expect("--scroll=X,Y takes numbers"));
    // The buttons a hidden run taps on the title or on the name entry.
    let presses = || {
        args.iter()
            .find_map(|arg| arg.strip_prefix("--press="))
            .map(|value| {
                title::presses(value).expect(
                    "--press=SECONDS:BUTTON,... takes start, select, cross, triangle, square, circle, up, down, left, right, l1, l2, r1 or r2",
                )
            })
            .unwrap_or_default()
    };

    // A run that ends on its own is a check, not a play session: its window stays hidden so
    // that it does not come up over whatever else is on the screen. `--show` brings it back.
    let hidden =
        (frames.is_some() || screenshot.is_some()) && !args.iter().any(|arg| arg == "--show");
    // A play session keeps the player's settings, the log and the game feel beside the
    // saves in the game folder (`prefs.rs`, `logging.rs`); a run that ends on its own or
    // taps buttons by itself is a check and keeps none of them.
    let automated = frames.is_some()
        || screenshot.is_some()
        || args.iter().any(|arg| {
            arg.contains("press=")
                || arg == "--confirms"
                || arg == "--skip-scenes"
                || arg == "--save-check"
                || [
                    "--pad",
                    "--cross",
                    "--steer=",
                    "--stick=",
                    "--menu-at=",
                    "--hold=",
                ]
                .iter()
                .any(|option| arg.starts_with(option))
        });
    let live = !automated;
    let log = live.then(logging::start);

    // A play session asks for the disc image when none is stored or the stored one is
    // gone, and stores the answer. A run that ends on its own never asks: without an
    // image it fails where it reads the disc, as before.
    if frames.is_none()
        && screenshot.is_none()
        && let Err(err) = disc_prompt::ensure()
    {
        eprintln!("jc-game cannot start: {err}");
        disc_prompt::cannot_start(&err);
        return AppExit::error();
    }

    let mut app = App::new();
    // The settings are there before the plugins, which read them (`keys.rs`).
    let (mut prefs, prefs_file) = prefs::at_start(live);
    // A check keeps its window of 960 by 720 (`--resolution=` and the like change it).
    if !live {
        display::for_a_check(&mut prefs.display, &args);
        pace::for_a_check(&mut prefs.feel.pace, &args);
        assist::for_a_check(&mut prefs.feel.assist, &args);
        prompts::for_a_check(&mut prefs.feel.prompts, &args);
    }
    // The first room may be read before the first frame.
    assist::set(&prefs.feel.assist);
    let exact = live || args.iter().any(|arg| arg.starts_with("--resolution="));
    let window = display::window(&prefs.display, hidden, exact);
    app.insert_resource(prefs.clone())
        .insert_resource(prefs_file)
        .insert_resource(guards::Unsaved::default())
        .init_resource::<guards::Asking>();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(window),
                // The close button asks first in a game that has gone on since its
                // last save (`guards.rs`).
                close_when_requested: false,
                ..default()
            })
            // A play session's log goes to its file too.
            .set(bevy::log::LogPlugin {
                custom_layer: logging::file_layer,
                ..default()
            })
            // The console does not filter textures.
            .set(ImagePlugin::default_nearest()),
    )
    .add_plugins((
        field::FieldPlugin,
        before::BeforePlugin,
        model_view::ModelViewPlugin,
        shown::ShownPlugin,
        // A check reads no gamepad.
        gamepad::GamepadPlugin { reads: live },
        keys::KeysPlugin,
        pointer::PointerPlugin,
        name_entry::TypingPlugin,
        // The window of a check stays as it is made.
        display::DisplayPlugin {
            manages_the_window: !hidden,
        },
        widescreen::WidescreenPlugin,
        // F1 and F2 only where there are keys.
        pace::PacePlugin { keys: !hidden },
        // The volumes of a PC and the window in the background.
        audio::AudioPlugin,
        notice::NoticePlugin,
        // The keys or the pad's buttons in the lines of help.
        prompts::PromptsPlugin,
        // The confirm button over the hero where a press would do something.
        hint::HintPlugin,
    ))
    // The PC Settings of the options screen; `--pc-key=NAME` is a check's key.
    .add_plugins(pc_settings::PcSettingsPlugin {
        key_of_a_check: (args.iter())
            .find_map(|arg| arg.strip_prefix("--pc-key="))
            .map(|name| bindings::key_named(name).expect("--pc-key=NAME takes a key")),
    })
    // Start held skips a scene; a play session keeps what was seen (`skip.rs`).
    .add_plugins(skip::SkipPlugin { live })
    .insert_resource(ClearColor(Color::BLACK));
    match &log {
        Some(Ok(path)) => info!("the log of this start: {}", path.display()),
        Some(Err(err)) => warn!("no log file: {err}"),
        None => {}
    }
    // A play session can be started over from the opening movie with Ctrl+F5. A check
    // has no keys and ends on its own.
    if !hidden {
        app.insert_resource(guards::Restarts);
    }
    // The settings reach their file whenever they change; the close button asks in a
    // game that has gone on since its last save (`guards.rs`).
    app.add_systems(Last, prefs::keep)
        // A fight of two players tells the other side when the game closes (`net.rs`).
        .add_systems(Last, net::close_on_exit)
        // The assists reach the room's loading and its scripts (`assist.rs`).
        .add_systems(First, assist::reach)
        .add_systems(Update, guards::ask_before_closing)
        .add_systems(
            Update,
            guards::note_play
                .run_if(resource_exists::<ShowRoom>)
                // The arena keeps no game to lose.
                .run_if(not(resource_exists::<versus_view::Arena>))
                .run_if(not(resource_exists::<card_view::CardView>)),
        );

    // `--skip-movies=SECONDS` holds Start from that second of every movie that follows
    // the first one, for a hidden run of the chain, and of every movie a script plays;
    // the opening movie that "End Game" plays is such a one.
    let skip_movies = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--skip-movies="))
        .map(|value| -> f64 { value.parse().expect("--skip-movies=SECONDS takes a number") });
    // Whether the start of the game, which plays movies, is what runs.
    let mut plays_movies = false;

    // A room brings the camera of the console's screen and a model a camera in space;
    // everything else is shown as it is.
    // The fight of two players on screen from the command line: the arena's room comes
    // once both teams are in (`versus_view.rs`), and the title follows its end.
    let versus_mode = match versus_check::asked(&args).filter(|_| versus_view) {
        Some(Ok(mode)) => Some(mode),
        Some(Err(err)) => {
            println!("versus: {err}");
            return AppExit::error();
        }
        None => None,
    };
    // The lobby of the fight of two players behind the title's "Arena Battle Stage"
    // (`lobby.rs`) and the arena it opens (`versus_view.rs`), whatever started the run:
    // both do nothing until they are asked for. `--lobby-press=STEP=SECONDS:BUTTON,...;...`
    // taps buttons on the lobby's steps in a hidden run.
    let lobby_presses = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--lobby-press="))
        .map(|value| {
            lobby::LobbyPresses::of(value).expect(
                "--lobby-press=STEP=SECONDS:BUTTON,...;... takes the lobby's steps and the buttons of --press",
            )
        })
        .unwrap_or_default();
    app.insert_resource(lobby_presses).add_systems(
        Update,
        (
            lobby::run_lobby,
            versus_view::run_arena,
            versus_view::paint_arena,
            versus_view::leave_arena,
        )
            .chain()
            .before(shown::draw_things),
    );
    if let Some(texture) = texture {
        app.insert_resource(texture)
            .add_systems(Startup, show_texture);
    } else if let Some(text) = text {
        app.insert_resource(text)
            .add_systems(Startup, text_view::show_text);
    } else if let Some(notice) = notice {
        app.insert_resource(notice)
            .add_systems(Startup, text_view::show_notice);
    } else if args.iter().any(|arg| arg == "--name-entry") {
        app.insert_resource(name_entry::ShowNameEntry { presses: presses() })
            .add_systems(Startup, name_entry::show_name_entry)
            .add_systems(Update, name_entry::run_name_entry);
    } else if let Some(kind) = args.iter().find_map(|arg| {
        arg.strip_prefix("--card")
            .filter(|rest| rest.is_empty() || rest.starts_with('='))
    }) {
        let folder = args.iter().find_map(|arg| arg.strip_prefix("--card-dir="));
        app.insert_resource(card_view::ShowCard {
            of_title: kind == "=continue",
            presses: presses(),
            folder: folder.map_or_else(paths::saves, PathBuf::from),
            told: card_view::Told::of(&args),
            shape: card_view::shape_of(&args),
        })
        .add_systems(Startup, card_view::show_card);
    } else if let Some(carried) = args.iter().find_map(|arg| {
        arg.strip_prefix("--menu")
            .filter(|rest| rest.is_empty() || rest.starts_with('='))
    }) {
        let creatures = carried.strip_prefix('=').map_or_else(Vec::new, |value| {
            player_menu::ShowMenu::creatures(value)
                .expect("--menu=SPECIES[:LEVEL],... takes up to three species, levels 1 to 32")
        });
        // `--menu-items=ITEM[:COUNT],...` gives him items and `--menu-hp=HP` sets his
        // HP and his creatures', for a look at the list of the items and at a use.
        let option = |name: &str| args.iter().find_map(|arg| arg.strip_prefix(name));
        let items = option("--menu-items=").map_or_else(Vec::new, |value| {
            player_menu::ShowMenu::items(value)
                .expect("--menu-items=ITEM[:COUNT],... takes items 0 to 42, counts 1 to 20")
        });
        let hp = option("--menu-hp=").map(|value| value.parse().expect("--menu-hp=HP"));
        // `--menu-things=THING,...` gives him things of the story, and `--menu-forest`
        // takes the look as if the room were in a forest, where thing 0 can be used.
        let things = option("--menu-things=").map_or_else(Vec::new, |value| {
            player_menu::ShowMenu::things(value)
                .expect("--menu-things=THING,... takes things of the story 0 to 37")
        });
        let forest = args.iter().any(|arg| arg == "--menu-forest");
        // `--menu-caught=SPECIES[:LEVEL],...` has caught creatures wait, twelve at most.
        let caught = option("--menu-caught=").map_or_else(Vec::new, |value| {
            player_menu::ShowMenu::caught(value)
                .expect("--menu-caught=SPECIES[:LEVEL],... takes up to twelve, levels 1 to 32")
        });
        // `--menu-gear=KIND:THING[:COUNT],...` gives him weapons (kind 0), armours (1)
        // and third things (2), for a look at the screen behind "Equip".
        let gear = option("--menu-gear=").map_or_else(Vec::new, |value| {
            player_menu::ShowMenu::gear(value).expect(
                "--menu-gear=KIND:THING[:COUNT],... takes kinds 0 to 2, a thing of the kind, counts 1 to 20",
            )
        });
        // `--menu-kinds=SPECIES,...` has kinds count as caught, for a look at the list
        // of the caught kinds (the square, with `--menu-forest`).
        let kinds = option("--menu-kinds=").map_or_else(Vec::new, |value| {
            player_menu::ShowMenu::kinds(value)
                .expect("--menu-kinds=SPECIES,... takes species 0 to 210")
        });
        app.insert_resource(player_menu::ShowMenu {
            presses: presses(),
            creatures,
            items,
            hp,
            things,
            forest,
            caught,
            kinds,
            gear,
        })
        .add_systems(Startup, player_menu::show_menu);
    } else if let Some(shop) = args.iter().find_map(|arg| arg.strip_prefix("--shop=")) {
        let (kind, argument) = shop_view::ShowShop::of(shop)
            .expect("--shop=KIND[:ARGUMENT] takes kind 0 or 1 and a number");
        let money = args
            .iter()
            .find_map(|arg| arg.strip_prefix("--shop-money="))
            .map(|value| value.parse().expect("--shop-money=N takes a number"));
        // `--shop-items=ITEM[:COUNT],...` and `--shop-gear=KIND:THING[:COUNT],...` give
        // him things to sell, as `--menu-items=` and `--menu-gear=` do for the menu.
        let option = |name: &str| args.iter().find_map(|arg| arg.strip_prefix(name));
        let items = option("--shop-items=").map_or_else(Vec::new, |value| {
            player_menu::ShowMenu::items(value)
                .expect("--shop-items=ITEM[:COUNT],... takes items 0 to 42, counts 1 to 20")
        });
        let gear = option("--shop-gear=").map_or_else(Vec::new, |value| {
            player_menu::ShowMenu::gear(value).expect(
                "--shop-gear=KIND:THING[:COUNT],... takes kinds 0 to 2, a thing of the kind, counts 1 to 20",
            )
        });
        app.insert_resource(shop_view::ShowShop {
            kind,
            argument,
            money,
            items,
            gear,
        })
        .add_systems(Startup, shop_view::show_shop);
    } else if let Some(map) = args.iter().find_map(|arg| {
        arg.strip_prefix("--map")
            .filter(|rest| rest.is_empty() || rest.starts_with('='))
    }) {
        let map = map
            .strip_prefix('=')
            .map(|value| value.parse().expect("--map=MAP takes a number"));
        let chapter = args
            .iter()
            .find_map(|arg| arg.strip_prefix("--chapter="))
            .map_or(0, |value| {
                value.parse().expect("--chapter=N takes a number")
            });
        app.insert_resource(place_map::ShowMap {
            map,
            chapter,
            gate_flag: args.iter().any(|arg| arg == "--gate-flag"),
            presses: presses(),
        })
        .add_systems(Startup, place_map::show_place_map);
    } else if let Some(says) = says {
        // The field plugin sets the fixed step to the game frame: a pass of the window.
        app.insert_resource(says)
            .add_systems(Startup, text_view::show_page)
            .add_systems(FixedUpdate, text_view::run_talk);
    } else if let Some(model) = model {
        app.insert_resource(model)
            .add_systems(Startup, model_view::show_model)
            // The field plugin sets the fixed step to the game frame.
            .add_systems(FixedUpdate, model_view::play_model);
    } else if let Some(mut room) = room {
        room.scroll = scroll.unwrap_or_default();
        room.walk_mesh = args.iter().any(|arg| arg == "--walk-mesh");
        room.show_list = args
            .iter()
            .find_map(|arg| arg.strip_prefix("--show-list="))
            .map(|value| parse_show_list(value).expect("--show-list=LIST[:FRAME] takes numbers"));
        room.actors = args
            .iter()
            .find_map(|arg| match arg.strip_prefix("--actors") {
                Some("") => Some(None),
                Some(value) => {
                    let frame = value.strip_prefix('=').and_then(|frame| frame.parse().ok());
                    Some(Some(frame.expect("--actors=FRAME takes a number")))
                }
                None => None,
            });
        room.creature_move = args
            .iter()
            .find_map(|arg| arg.strip_prefix("--move="))
            .map(|value| {
                let mut numbers = value.split(':').map(|number| {
                    (number.parse::<i16>().ok().filter(|number| *number >= 0))
                        .expect("--move=N[:LOOPS[:SIZE]] takes numbers")
                });
                actors::MoveAsked {
                    number: numbers.next().unwrap_or(0) as usize,
                    loops: numbers.next().unwrap_or(0),
                    size: numbers.next().unwrap_or(0),
                }
            });
        room.ordered = actors::Ordered {
            carried: args.iter().any(|arg| arg == "--carried"),
            walks: args.iter().any(|arg| arg == "--walks"),
            path: (args.iter())
                .find_map(|arg| arg.strip_prefix("--path="))
                .map(|value| {
                    (value.split_once(':'))
                        .and_then(|(actor, list)| Some((actor.parse().ok()?, list.parse().ok()?)))
                        .expect("--path=ACTOR:LIST takes numbers")
                }),
        };
        room.player = args
            .iter()
            .find_map(|arg| match arg.strip_prefix("--player") {
                Some("") => Some(None),
                Some(value) => {
                    let frame = value.strip_prefix('=').and_then(|frame| frame.parse().ok());
                    Some(Some(frame.expect("--player=FRAME takes a number")))
                }
                None => None,
            })
            .map(|frame| {
                let pads = (args.iter())
                    .find_map(|arg| arg.strip_prefix("--pad="))
                    .map(|value| {
                        walker::Asked::pads(value).expect(
                            "--pad=NAMES[;FRAME:NAMES...] takes forward, fast, run, back, left, \
                             right and up to 256 later sets",
                        )
                    });
                (frame, pads)
            })
            .map(|(frame, pads)| walker::Asked {
                frame,
                pad: pads.map(|(first, _)| first),
                later: pads.map_or(walker::NO_LATER, |(_, later)| later),
                crosses: (args.iter())
                    .find_map(|arg| arg.strip_prefix("--cross="))
                    .map_or(walker::NO_CROSSES, |value| {
                        walker::Asked::crosses(value)
                            .expect("--cross=FRAME[,FRAME...] takes up to 64 game frames")
                    }),
                worn: (args.iter())
                    .find_map(|arg| arg.strip_prefix("--wears="))
                    .map_or(Worn::AT_THE_START, |value| {
                        walker::Asked::wears(value).expect("--wears=WEAPON[:ARMOUR] takes numbers")
                    }),
                // `--stand=X,Y,Z,ANGLE`: a check option that puts the player of a room
                // with scripts on a place of the room in place of its default one, as
                // a loaded game does, so that a hidden run need not walk him there.
                stands: (args.iter())
                    .find_map(|arg| arg.strip_prefix("--stand="))
                    .map(|value| {
                        let numbers: Vec<i32> = (value.split(','))
                            .map(|number| {
                                number.parse().expect("--stand=X,Y,Z,ANGLE takes numbers")
                            })
                            .collect();
                        let [x, y, z, angle] = numbers[..] else {
                            panic!("--stand=X,Y,Z,ANGLE takes four numbers");
                        };
                        ([x, y, z], angle as i16)
                    }),
            });
        let confirms = args.iter().any(|arg| arg == "--confirms");
        room.script = args
            .iter()
            .find_map(|arg| match arg.strip_prefix("--script") {
                Some("") => Some((None, confirms)),
                Some(value) => {
                    let frame = value.strip_prefix('=').and_then(|frame| frame.parse().ok());
                    let frame = frame.expect("--script=FRAME takes a number");
                    Some((Some(frame), confirms))
                }
                None => None,
            });
        room.chapter = args
            .iter()
            .find_map(|arg| arg.strip_prefix("--chapter="))
            .map(|value| value.parse().expect("--chapter=N takes a number"));
        room.has = args
            .iter()
            .find_map(|arg| arg.strip_prefix("--has="))
            .map_or_else(Vec::new, |value| {
                (value.split(','))
                    .map(|thing| thing.parse().expect("--has=N,... takes numbers"))
                    .collect()
            });
        room.items = args
            .iter()
            .find_map(|arg| arg.strip_prefix("--items="))
            .map_or_else(Vec::new, |value| {
                (value.split(","))
                    .map(|item| item.parse().expect("--items=N,... takes numbers"))
                    .collect()
            });
        room.skins = args
            .iter()
            .find_map(|arg| arg.strip_prefix("--skins="))
            .map_or_else(Vec::new, |value| {
                (value.split(','))
                    .map(|skin| skin.parse().expect("--skins=N,... takes numbers"))
                    .collect()
            });
        room.gear = args
            .iter()
            .find_map(|arg| arg.strip_prefix("--gear="))
            .map_or_else(Vec::new, |value| {
                player_menu::ShowMenu::gear(value).expect(
                    "--gear=KIND:THING[:COUNT],... takes kinds 0 to 2, a thing of the kind, counts 1 to 20",
                )
            });
        room.money = args
            .iter()
            .find_map(|arg| arg.strip_prefix("--money="))
            .map_or(0, |value| value.parse().expect("--money=N takes a number"));
        room.cocoons = args
            .iter()
            .find_map(|arg| arg.strip_prefix("--cocoons="))
            .map_or(0, |value| {
                value.parse().expect("--cocoons=N takes a number")
            });
        room.flags = args
            .iter()
            .find_map(|arg| arg.strip_prefix("--flags="))
            .map_or_else(Vec::new, |value| {
                (value.split(','))
                    .map(|flag| flag.parse().expect("--flags=N,... takes numbers"))
                    .collect()
            });
        room.variables = args
            .iter()
            .find_map(|arg| arg.strip_prefix("--variables="))
            .map_or_else(Vec::new, |value| {
                (value.split(','))
                    .map(|pair| {
                        let (number, value) = pair.split_once(':')?;
                        Some((number.parse().ok()?, value.parse().ok()?))
                    })
                    .collect::<Option<_>>()
                    .expect("--variables=N:V,... takes pairs of numbers")
            });
        app.insert_resource(room).add_systems(Startup, show_room);
        if let Some(says) = says_in_room {
            // The window comes over the room once the room's things are there, and a
            // pass of it is a game frame.
            app.insert_resource(says)
                .add_systems(Startup, text_view::talk_in_room.after(show_room))
                .add_systems(FixedUpdate, text_view::run_talk);
        }
    } else {
        let pc_settings = args.iter().find_map(|arg| {
            let rest = arg.strip_prefix("--pc-settings")?;
            match rest.strip_prefix('=') {
                Some(page) => Some(
                    pc_settings::Page::parse(page)
                        .expect("--pc-settings=PAGE takes display, sound, gameplay or controls"),
                ),
                None => rest.is_empty().then(pc_settings::Page::default),
            }
        });
        let only_title =
            movie.is_none() && (args.iter().any(|arg| arg == "--title") || pc_settings.is_some());
        let movie = movie.unwrap_or_else(movie_view::ShowMovie::opening);
        let movie = match press_start {
            Some(seconds) if !only_title => movie.press_start(seconds),
            _ => movie,
        };
        let name_presses = args
            .iter()
            .find_map(|arg| arg.strip_prefix("--name-press="))
            .map(|value| {
                title::presses(value)
                    .expect("--name-press=SECONDS:BUTTON,... takes the buttons of --press")
            })
            .unwrap_or_default();
        // `--title-again=SECONDS:BUTTON,...` taps buttons on a title that opens behind
        // a movie, counted from the moment it opens.
        let again = args
            .iter()
            .find_map(|arg| arg.strip_prefix("--title-again="))
            .map(|value| {
                title::presses(value)
                    .expect("--title-again=SECONDS:BUTTON,... takes the buttons of --press")
            })
            .unwrap_or_default();
        plays_movies = true;
        if let Some(mode) = versus_mode.clone() {
            app.insert_resource(versus_view::Start {
                mode,
                args: args.clone(),
                hidden: !live,
                lobby: None,
            })
            .add_systems(Startup, versus_view::start_arena);
        }
        app.insert_resource(movie.muted(hidden).skip_later(skip_movies))
            .insert_resource(title::BehindAMovie(again))
            .insert_resource(name_entry::ShowNameEntry {
                presses: name_presses,
            })
            .add_audio_source::<movie_sound::MovieSound>()
            .add_systems(
                Update,
                (
                    movie_view::play_movie,
                    title::run_title,
                    name_entry::run_name_entry,
                )
                    .chain(),
            );
        if versus_mode.is_some() {
            // The arena of the command line comes first; the title only behind it.
        } else if only_title {
            app.insert_resource(title::ShowTitle {
                press_start,
                presses: presses(),
                pc_settings,
            })
            .add_systems(Startup, title::show_title);
        } else {
            app.add_systems(Startup, movie_view::show_movie);
        }
    }

    // A room is on screen while there is a `ShowRoom`, whether an option named it or the
    // game came to it. The field plugin sets the fixed step to the game frame.
    app.add_systems(
        FixedUpdate,
        (
            script_music,
            leave_room,
            open_map,
            start_chapter,
            go_to_room,
            play_script_movie,
            open_menu,
            open_shop,
            open_card,
            open_results,
        )
            .chain()
            .after(shown::change_view)
            .run_if(resource_exists::<ShowRoom>)
            .run_if(movie_view::room_runs),
    );
    app.add_systems(Update, end_view::run_ending.after(shown::draw_things));
    if !plays_movies {
        app.add_audio_source::<movie_sound::MovieSound>()
            .add_systems(Update, movie_view::play_movie);
    }
    app.insert_resource(music::Music::new(hidden))
        .add_systems(Update, music::play_music);
    app.insert_resource(voices::Voices::new(hidden))
        .add_systems(Update, voices::play_voices);
    let mut session = match live {
        true => settings::Settings(prefs.game.settings()),
        false => settings::Settings::default(),
    };
    if let Some(value) = args.iter().find_map(|arg| arg.strip_prefix("--setting=")) {
        session.0.word = u32::from_str_radix(value.trim_start_matches("0x"), 16)
            .expect("--setting=WORD takes the setting word in hexadecimal");
    }
    // `--window-colour=N` starts a run in another of the four window colours.
    if let Some(value) = (args.iter()).find_map(|arg| arg.strip_prefix("--window-colour=")) {
        session.0.colour = (value.parse().ok())
            .filter(|colour| (0..4).contains(colour))
            .expect("--window-colour=N takes a colour from 0 to 3");
    }
    app.insert_resource(session)
        .add_systems(First, settings::reach);
    app.insert_resource(saving::PlayClock::default())
        .add_systems(Update, saving::count);
    app.insert_resource(movie_view::ScriptMovies {
        muted: hidden,
        skip: skip_movies,
    })
    .add_systems(
        Update,
        movie_view::back_to_the_room.after(movie_view::play_movie),
    );
    app.add_systems(
        Update,
        (
            chapter::run_chapter,
            start_room.run_if(resource_added::<chapter::Arrival>),
        )
            .chain()
            // A room that comes in this frame is drawn in it as its first game frame
            // left it, not a frame later.
            .before(shown::draw_things)
            .after(movie_view::play_movie)
            .after(name_entry::run_name_entry),
    );
    let map_presses = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--map-press="))
        .map(|value| {
            place_map::MapPresses::of(value)
                .expect("--map-press=SECONDS:BUTTON,...[;...] takes the buttons of --press")
        })
        .unwrap_or_default();
    app.insert_resource(map_presses).add_systems(
        Update,
        (
            place_map::run_place_map,
            enter_place.run_if(resource_added::<place_map::Leaves>),
        )
            .chain()
            .before(shown::draw_things),
    );
    let option = |name: &str| args.iter().find_map(|arg| arg.strip_prefix(name));
    let opens = player_menu::Opens::of(option("--menu-at="), option("--menu-press="))
        .expect("--menu-at=FRAME,... takes game frames, --menu-press= the buttons of --press");
    app.insert_resource(opens)
        .add_systems(Update, player_menu::run_menu);
    let shop_presses = option("--shop-press=").map_or_else(Vec::new, |value| {
        title::presses(value).expect("--shop-press=SECONDS:BUTTON,... takes the buttons of --press")
    });
    app.insert_resource(shop_view::ShopPresses(shop_presses))
        .add_systems(Update, shop_view::run_shop);
    let card_presses = option("--card-press=").map_or_else(Vec::new, |value| {
        title::presses(value).expect("--card-press=SECONDS:BUTTON,... takes the buttons of --press")
    });
    app.insert_resource(card_view::CardsKept {
        folder: option("--card-dir=").map_or_else(paths::saves, PathBuf::from),
        presses: card_presses,
        told: card_view::Told::of(&args),
        shape: card_view::shape_of(&args),
    })
    .add_systems(Update, card_view::run_card);
    // F6 saves into the quick save and F9 loads it; the autosave writes one of three on
    // the way into a forest, when a chapter starts and in front of a fight of the story
    // (`quick.rs`). A check presses them
    // with `--quick-press=SECONDS:save|load,...` and keeps saves only in the folder of
    // `--card-dir=DIR`.
    let quick_presses = option("--quick-press=").map_or_else(Vec::new, |value| {
        quick::presses(value)
            .expect("--quick-press=SECONDS:save|load|auto,... takes save, load or auto")
    });
    app.insert_resource(quick::QuickSaves {
        folder: option("--card-dir=").map_or_else(paths::saves, PathBuf::from),
        writes: live || option("--card-dir=").is_some(),
        presses: quick_presses,
    })
    .init_resource::<quick::Autosave>()
    .add_systems(
        Update,
        (quick::note_autosave, quick::run_saves)
            .chain()
            .after(card_view::run_card)
            .before(play_loaded),
    );
    app.add_systems(
        Update,
        play_loaded
            .after(card_view::run_card)
            .run_if(resource_exists::<saving::Loaded>)
            // Behind "Continue" the title leaves first.
            .run_if(not(resource_exists::<title::Title>)),
    );
    // "End Game" of the screen: the field ends and the opening movie plays.
    app.add_systems(
        Update,
        end_game
            .after(card_view::run_card)
            .run_if(resource_exists::<saving::Ended>),
    );
    let merge_presses = option("--merge-press=").map_or_else(Vec::new, |value| {
        title::presses(value)
            .expect("--merge-press=SECONDS:BUTTON,... takes the buttons of --press")
    });
    app.insert_resource(merge_view::MergePresses(merge_presses))
        .add_systems(
            Update,
            (merge_view::point_merge, merge_view::run_merge).chain(),
        );
    let results_presses = option("--results-press=").map_or_else(Vec::new, |value| {
        title::presses(value)
            .expect("--results-press=SECONDS:BUTTON,... takes the buttons of --press")
    });
    // `--results-experience=HEX[,HEX...]` is a check option: the screen counts those
    // sums for the player and for the creatures he carries, in their order, so a level
    // and a new rank of his and a level of a creature can be looked at.
    let results_sum = option("--results-experience=").map_or_else(Vec::new, |value| {
        let sum = |sum: &str| {
            u32::from_str_radix(sum, 16)
                .expect("--results-experience=HEX[,HEX...] takes sums in hexadecimal")
        };
        value.split(',').map(sum).collect()
    });
    app.insert_resource(results_view::ResultsPresses(results_presses))
        .insert_resource(results_view::ResultsSum(results_sum))
        .add_systems(Update, results_view::run_results);
    let wants_menu = |results: Option<Res<results_view::ResultsView>>| {
        results.is_some_and(|results| results.wants_menu())
    };
    app.add_systems(
        Update,
        open_menu
            .after(results_view::run_results)
            .run_if(resource_exists::<ShowRoom>)
            .run_if(wants_menu),
    );

    // `--room-log[=FILE]`: every room the game comes to is written down for the tester.
    if let Some(log) = args.iter().find_map(|arg| RoomLog::asked(arg)) {
        app.insert_resource(log).add_systems(
            Update,
            note_rooms
                .run_if(resource_exists::<ShowRoom>)
                .run_if(resource_exists::<RoomLog>),
        );
    }

    // `--glide-at=F` draws a check between two game frames at one share of the way, and
    // `--frame-times` logs how long the frames take (`smooth.rs`).
    if let Some(at) = smooth::glide_at(&args).filter(|_| !live) {
        app.insert_resource(at);
    }
    if args.iter().any(|arg| arg == "--frame-times") {
        app.init_resource::<smooth::FrameTimes>()
            .add_systems(First, smooth::start_frame)
            .add_systems(Last, smooth::log_frame_times);
    }
    if frames.is_some() || screenshot.is_some() {
        app.insert_resource(Smoke {
            frames_left: frames.flatten().unwrap_or(DEFAULT_FRAMES),
            capture: screenshot.map(|path| Capture {
                path,
                requested: None,
                frames_waited: 0,
                frames: (args.iter())
                    .find_map(|arg| arg.strip_prefix("--screenshot-frames="))
                    .map_or(1, |value| {
                        let frames: u32 =
                            value.parse().expect("--screenshot-frames=N takes a number");
                        frames.clamp(1, 99)
                    }),
                taken: 0,
            }),
        })
        .add_systems(Update, smoke_exit);
        // `--save-check` makes the save of the moment the run ends and logs its head.
        if args.iter().any(|arg| arg == "--save-check") {
            let ends = |smoke: Res<Smoke>| smoke.frames_left == 0;
            app.add_systems(Update, saving::check.run_if(ends).before(smoke_exit));
        }
    }
    // Returning the result is what turns a failed run into a non-zero exit code.
    app.run()
}

fn show_texture(
    show: Res<ShowTexture>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    let (width, height, rgba) = match load_texture(&show) {
        Ok(texture) => texture,
        Err(err) => {
            error!("texture {} of file {}: {err}", show.index, show.file);
            exit.write(AppExit::error());
            return;
        }
    };
    info!(
        "texture {} of file {}: {width}x{height}",
        show.index, show.file
    );
    commands.spawn(Camera2d);
    spawn_picture(&mut commands, &mut images, width, height, rgba);
}

/// Puts RGBA pixels in the middle of the window, enlarged by a whole factor to fit it.
fn spawn_picture(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
) -> Entity {
    let image = picture_image(width, height, rgba);
    let factor = (WINDOW_SIZE.0 / width).min(WINDOW_SIZE.1 / height).max(1);
    commands
        .spawn((
            Sprite::from_image(images.add(image)),
            Transform::from_scale(Vec3::splat(factor as f32)),
        ))
        .id()
}

/// RGBA pixels as a picture for the graphics card.
fn picture_image(width: u32, height: u32, rgba: Vec<u8>) -> Image {
    Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// Reads one texture from the disc and decodes it to RGBA.
fn load_texture(show: &ShowTexture) -> Result<(u32, u32, Vec<u8>), Box<dyn std::error::Error>> {
    let config = LocalConfig::load()?;
    let mut disc = DiscImage::open(&config.disc_image)?;
    let archive = Archive::open(&mut disc)?;
    let data = archive.read(&mut disc, show.file)?;
    let tims = tim::find_all(&data);
    let (_, texture) = tims
        .get(show.index)
        .ok_or_else(|| format!("the file has {} textures", tims.len()))?;
    let palette = match &texture.clut {
        Some(clut) if clut.is_shared() => {
            tim::shared_palette(tims.iter().map(|(_, tim)| tim), clut.rect, 0)
        }
        _ => texture.palette(0),
    };
    Ok((texture.width, texture.height, texture.to_rgba(palette)?))
}

fn show_room(
    mut show: ResMut<ShowRoom>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<field::AddedMaterial>>,
    mut commands: Commands,
    mut music: ResMut<music::Music>,
    mut exit: MessageWriter<AppExit>,
) {
    let mut carried = match story_so_far(show.chapter) {
        Ok(carried) => carried,
        Err(err) => {
            error!("room {}: {err}", show.number);
            exit.write(AppExit::error());
            return;
        }
    };
    for &thing in &show.has {
        carried.keeping.give_story_thing(thing);
    }
    for &item in &show.items {
        let worn = carried.worn;
        carried.keeping.give(item, 1, Kind::Item, worn);
    }
    for &skin in &show.skins {
        let worn = carried.worn;
        carried.keeping.give(skin, 1, Kind::Creature, worn);
    }
    for &(kind, thing, count) in &show.gear {
        let list = [Kind::Weapon, Kind::Armour, Kind::Ornament][kind];
        let worn = carried.worn;
        carried.keeping.give(thing, count, list, worn);
    }
    if show.money > 0 {
        carried.keeping.give_money(show.money);
    }
    if show.cocoons > 0 {
        carried.keeping.give_cocoons(show.cocoons);
    }
    for &flag in &show.flags {
        carried.keeping.set_flag(flag);
    }
    for &(number, value) in &show.variables {
        carried.set_variable(number, value);
    }
    if let Some(word) =
        std::env::args().find_map(|arg| arg.strip_prefix("--place-word=")?.parse().ok())
    {
        carried.place_word = word;
    }
    // What `--wears` puts on the player is the state's, as an equip routine leaves it.
    if let Some(asked) = show.player {
        [carried.worn[0], carried.worn[1]] = [asked.worn.weapon, asked.worn.armour];
    }
    commands.insert_resource(Game(carried.clone()));
    put_room(
        &mut show,
        &carried,
        (&mut images, &mut meshes, &mut materials),
        &mut commands,
        &mut music,
        &mut exit,
    );
}

/// The room the chapter screen left for comes on screen in the running app, in place of
/// what the screens before it showed through their camera: the field brings its own.
#[allow(clippy::too_many_arguments)]
fn start_room(
    arrival: Res<chapter::Arrival>,
    game: Res<Game>,
    cameras: Query<Entity, (With<Camera2d>, Without<display::Presents>)>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<field::AddedMaterial>>,
    mut commands: Commands,
    mut music: ResMut<music::Music>,
    mut exit: MessageWriter<AppExit>,
) {
    // Taken away so that the next chapter screen's arrival is a new one.
    commands.remove_resource::<chapter::Arrival>();
    let Some(mut show) = ShowRoom::arrived(&arrival, &game.0) else {
        error!("chapter {}: its first room has no number", arrival.chapter);
        exit.write(AppExit::error());
        return;
    };
    for camera in &cameras {
        commands.entity(camera).despawn();
    }
    put_room(
        &mut show,
        &game.0,
        (&mut images, &mut meshes, &mut materials),
        &mut commands,
        &mut music,
        &mut exit,
    );
    info!(
        "chapter {}: room {} is on screen, its scripts running",
        arrival.chapter, show.on_screen
    );
    commands.insert_resource(show);
}

fn script_music(
    things: Option<ResMut<shown::Things>>,
    mut music: ResMut<music::Music>,
    mut voices: ResMut<voices::Voices>,
    skip: Option<Res<skip::Skip>>,
) {
    let Some(mut things) = things else {
        return;
    };
    // A scene that the build skips (`skip.rs`) is silent but for its music: no line is
    // asked for, which the windows take as a line that is over, and no short sound
    // plays. What a sound that stays does is kept, so that it is right behind the skip.
    let skipping = skip.is_some_and(|skip| skip.skipping());
    for asked in things.take_music() {
        match asked {
            scripts::Heard::Line(_) | scripts::Heard::Sound(_) | scripts::Heard::Got(_)
                if skipping => {}
            scripts::Heard::Line(line) => voices.ask(line),
            scripts::Heard::LineEnds => {
                voices.end();
            }
            scripts::Heard::Music(asked) => music.script_asks(asked),
            scripts::Heard::Sound(number) => music.sound(number),
            scripts::Heard::Fight(asked) => music.fight_asks(asked),
            scripts::Heard::Kept { at, sound } => music.sound_kept(at, sound),
            scripts::Heard::Ended { at } => music.sound_ends(at),
            scripts::Heard::Got(number) => music.notice_asks(number),
            scripts::Heard::FadeKept => {
                let ran = music.fade_kept();
                info!(
                    "the music: a script's routine fades the sequence of the record that \
                     stays ({})",
                    if ran { "one ran" } else { "none ran" }
                );
            }
            scripts::Heard::Effect { number, level } => music.effect(number, level),
        }
    }
    things.hear_voices(voices.is_silent(), voices.none_asked());
}

/// Whatever is drawn or draws: the cameras, the sprites and the meshes.
/// What lays the picture into the window stays (`display.rs`).
type Drawn = (
    Or<(With<Camera>, With<Sprite>, With<Mesh2d>, With<Mesh3d>)>,
    Without<display::Presents>,
);

fn open_map(
    show: Res<ShowRoom>,
    mut game: ResMut<Game>,
    things: Option<ResMut<shown::Things>>,
    drawn: Query<Entity, Drawn>,
    mut commands: Commands,
) {
    let Some(mut things) = things else {
        return;
    };
    if !things.take_map() {
        return;
    }
    game.keep(&things, &show.on_screen);
    info!(
        "room {}: its script opens the place map, the room goes",
        show.on_screen
    );
    for entity in &drawn {
        commands.entity(entity).despawn();
    }
    commands.remove_resource::<shown::Things>();
    commands.remove_resource::<field::FieldScreen>();
    commands.remove_resource::<field::RoomViews>();
    commands.remove_resource::<field::WalkLines>();
    commands.remove_resource::<ShowRoom>();
    commands.insert_resource(place_map::OpenMap { room: show.number });
}

fn start_chapter(
    show: Option<Res<ShowRoom>>,
    mut game: ResMut<Game>,
    things: Option<ResMut<shown::Things>>,
    drawn: Query<Entity, Drawn>,
    mut commands: Commands,
) {
    // The place map may have taken the room away earlier in this game frame.
    let (Some(show), Some(mut things)) = (show, things) else {
        return;
    };
    let Some(chapter) = things.take_chapter() else {
        return;
    };
    game.keep(&things, &show.on_screen);
    info!(
        "room {}: its script starts chapter {chapter}, the room goes and the chapter screen runs",
        show.on_screen
    );
    for entity in &drawn {
        commands.entity(entity).despawn();
    }
    commands.remove_resource::<shown::Things>();
    commands.remove_resource::<field::FieldScreen>();
    commands.remove_resource::<field::RoomViews>();
    commands.remove_resource::<field::WalkLines>();
    commands.remove_resource::<ShowRoom>();
    commands.spawn(Camera2d);
    commands.insert_resource(chapter::StartChapter(chapter as usize));
}

fn play_script_movie(
    show: Option<Res<ShowRoom>>,
    things: Option<ResMut<shown::Things>>,
    how: Res<movie_view::ScriptMovies>,
    mut commands: Commands,
) {
    // The place map may have taken the room away earlier in this game frame.
    let Some(show) = show else {
        return;
    };
    let Some(movie) = things.and_then(|mut things| things.take_movie()) else {
        return;
    };
    info!(
        "room {}: its script plays movie {movie}, the room stands still",
        show.on_screen
    );
    let asked = movie_view::ShowMovie::of_script(movie as usize, &how);
    match movie_view::Playing::start(&asked) {
        Ok(playing) => playing.over_the_room(&mut commands),
        // The script goes on as if the movie had played.
        Err(err) => error!("movie {movie}: {err}"),
    }
}

#[allow(clippy::too_many_arguments)]
fn open_menu(
    show: Option<Res<ShowRoom>>,
    things: Option<Res<shown::Things>>,
    game: Res<Game>,
    named: Option<Res<name_entry::Named>>,
    playing: Option<Res<movie_view::Playing>>,
    results: Option<Res<results_view::ResultsView>>,
    results_drawn: Query<Entity, With<results_view::OverTheRoom>>,
    mut opens: ResMut<player_menu::Opens>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
) {
    // The place map may have taken the room away earlier in this game frame, and a
    // movie of a script plays first.
    let (Some(show), Some(things)) = (show, things) else {
        return;
    };
    let behind_results = results.is_some_and(|results| results.wants_menu());
    if behind_results {
        for entity in &results_drawn {
            commands.entity(entity).despawn();
        }
        commands.remove_resource::<results_view::ResultsView>();
    }
    let asked = things.asks_menu().filter(|_| playing.is_none());
    let Some(game_frame) = asked.or(behind_results.then_some(0)) else {
        return;
    };
    let carried = things.carried().unwrap_or_else(|| game.0.clone());
    let name = named.as_ref().map_or(&[][..], |named| &named.units);
    let view = things.view_shown();
    let mut stands = player_menu::in_a_forest(show.number, view).unwrap_or_else(|err| {
        warn!("room {}: whether it is in a forest: {err}", show.on_screen);
        player_menu::Stands {
            forest: false,
            way_out: None,
            free: true,
            waits: Vec::new(),
        }
    });
    stands.waits = things.waited_things();
    // The one test that differs behind the results screen: nothing of "Valuables" can
    // be used while a script holds the game.
    stands.free = !behind_results || things.frees_things();
    let (forest, free) = (stands.forest, stands.free);
    let opened = player_menu::over_the_room(
        &carried,
        name,
        stands,
        &mut opens,
        &mut images,
        &mut commands,
    );
    match opened {
        Ok(()) if behind_results => info!(
            "room {}: the menu comes up in place of the results screen and the room stands; \
             from view {view} the room is {} a forest, the things of the story are {}",
            show.on_screen,
            if forest { "in" } else { "not in" },
            if free {
                "free to use"
            } else {
                "held by a script"
            }
        ),
        Ok(()) => info!(
            "room {}: the button of the menu went down in game frame {}, the menu comes up \
             and the room stands; from view {view} the room is {} a forest",
            show.on_screen,
            game_frame.saturating_sub(1),
            if forest { "in" } else { "not in" }
        ),
        Err(err) => error!("the player's menu: {err}"),
    }
}

fn open_shop(
    show: Option<Res<ShowRoom>>,
    things: Option<ResMut<shown::Things>>,
    presses: Res<shop_view::ShopPresses>,
    merge_presses: Res<merge_view::MergePresses>,
    merge: Option<ResMut<merge_view::MergeView>>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
) {
    // The place map may have taken the room away earlier in this game frame.
    let (Some(show), Some(mut things)) = (show, things) else {
        return;
    };
    let Some((kind, argument)) = things.take_screen() else {
        return;
    };
    if kind == jc_formats::merge_screen::BACK_KIND {
        match merge {
            Some(mut merge) => {
                info!(
                    "room {}: its script asks for the screen of kind {kind}, the screen \
                     that sleeps comes back and the room stands",
                    show.on_screen
                );
                merge.come_back();
            }
            // The script goes on as if the screen had left.
            None => {
                error!("the screen of the caught: asked to come back, it is not there");
                things.screen_left();
            }
        }
        return;
    }
    let Some(mut carried) = things.carried() else {
        return;
    };
    let asked = (kind as i16, argument as i16);
    if kind == jc_formats::merge_screen::KIND
        && things.nobody_at_the_pad()
        && merge_presses.0.is_empty()
    {
        let line = merge_view::nobody_chooses(&mut carried);
        info!(
            "room {}: its script asks for the screen of kind {kind}: {line}",
            show.on_screen
        );
        things.listed(&carried);
        things.screen_left();
        return;
    }
    // The keeper is thing 1 of the room: his face stands left of his line.
    let face = |faces: &jc_formats::window::Faces| {
        let speaker = things.speaker(faces, 1, 1);
        speaker.ok().map(|speaker| speaker.face)
    };
    if kind == jc_formats::merge_screen::KIND {
        let opened = merge_view::over_the_room(
            argument as i16,
            carried,
            face,
            &merge_presses,
            &mut images,
            &mut commands,
        );
        match opened {
            Ok(()) => info!(
                "room {}: its script asks for the screen of kind {kind}, the screen comes \
                 up and the room stands",
                show.on_screen
            ),
            // The script goes on as if the screen had left.
            Err(err) => {
                error!("the screen of the caught: {err}");
                things.screen_left();
            }
        }
        return;
    }
    let opened = shop_view::over_the_room(
        asked,
        carried,
        show.number,
        face,
        &presses,
        &mut images,
        &mut commands,
    );
    match opened {
        Ok(()) => info!(
            "room {}: its script asks for the screen of kind {kind}, the screen comes up \
             and the room stands",
            show.on_screen
        ),
        // The script goes on as if the screen had left.
        Err(err) => {
            error!("the shop's screen: {err}");
            things.screen_left();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn open_card(
    show: Option<Res<ShowRoom>>,
    things: Option<ResMut<shown::Things>>,
    kept: Res<card_view::CardsKept>,
    settings: Res<settings::Settings>,
    named: Option<Res<name_entry::Named>>,
    clock: Res<saving::PlayClock>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
) {
    // The place map may have taken the room away earlier in this game frame.
    let (Some(show), Some(mut things)) = (show, things) else {
        return;
    };
    // The menu's question stands in front of this one in the field's task: a request
    // waits behind a menu that opens in the same game frame.
    if things.asks_menu().is_some() {
        return;
    }
    let finished = things.take_end_card();
    let Some(place) = finished.or_else(|| things.take_card()) else {
        return;
    };
    let Some(carried) = things.carried() else {
        return;
    };
    let room = (things.stands()).map(|stands| (show.number, things.view_shown(), stands));
    let beside = (&*settings, named.as_deref(), &*clock);
    let opened = saving::Running::of(carried, beside, room, place).and_then(|mut game| {
        let kind = match finished {
            Some(_) => {
                game.finish();
                jc_formats::card_screen::Kind::Finished
            }
            None => jc_formats::card_screen::Kind::Room,
        };
        card_view::over_the_room(kind, game, &kept, settings.0, &mut images, &mut commands)
    });
    match opened {
        Ok(()) if finished.is_some() => info!(
            "room {}: its script asks for the save behind the end of the story, the memory \
             card's screen comes up and the room stands",
            show.on_screen
        ),
        Ok(()) => info!(
            "room {}: its object of the memory card asks for the memory card's screen, the \
             screen comes up and the room stands",
            show.on_screen
        ),
        Err(err) => error!("the memory card screen: {err}"),
    }
}

#[allow(clippy::too_many_arguments)]
fn play_loaded(
    loaded: Res<saving::Loaded>,
    mut settings: ResMut<settings::Settings>,
    mut clock: ResMut<saving::PlayClock>,
    drawn: Query<Entity, Drawn>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<field::AddedMaterial>>,
    mut commands: Commands,
    mut music: ResMut<music::Music>,
    mut exit: MessageWriter<AppExit>,
    (movie, show): (Option<Res<movie_view::ShowMovie>>, Option<Res<ShowRoom>>),
    (mut prefs, mut unsaved): (ResMut<prefs::Prefs>, ResMut<guards::Unsaved>),
) {
    commands.remove_resource::<saving::Loaded>();
    // The game feel kept beside the save comes back with it, as the settings in it do
    // below (`prefs.rs`).
    prefs::take_feel(&mut prefs, loaded.feel.clone());
    unsaved.kept(loaded.room, loaded.beside.clock);
    if loaded.beside.finished {
        if let Some(movie) = &movie {
            info!(
                "the loaded game is a finished one: a second round starts, movie {} plays",
                jc_formats::title::SECOND_ROUND_MOVIE
            );
            if show.is_some() {
                // The field ends as behind "End Game"; under the title nothing of a
                // room is there and the title's camera and music stay.
                for entity in &drawn {
                    commands.entity(entity).despawn();
                }
                commands.remove_resource::<shown::Things>();
                commands.remove_resource::<field::FieldScreen>();
                commands.remove_resource::<field::RoomViews>();
                commands.remove_resource::<field::WalkLines>();
                commands.remove_resource::<ShowRoom>();
                music.field_ends();
                commands.spawn(Camera2d);
            }
            commands.insert_resource(saving::SecondRound {
                carried: loaded.carried.clone(),
                beside: loaded.beside,
            });
            match movie_view::Playing::start(&movie.second_round()) {
                Ok(playing) => commands.insert_resource(playing),
                Err(err) => {
                    error!("the first movie of a second round: {err}");
                    exit.write(AppExit::error());
                }
            }
            return;
        }
        warn!(
            "the loaded game is a finished one, whose load starts a second round; this run \
             started in a room and has neither its movies nor the name entry, the save is \
             played on as it was saved"
        );
    }
    let stands = &loaded.stands;
    let Some(number) = loaded.room else {
        error!(
            "the loaded game: area {} place {} record {} is no room with a number, the room \
             on screen stays",
            stands.area, stands.place, stands.room
        );
        return;
    };
    info!(
        "the loaded game: the field starts anew in room {number} (area {} place {} record {}), \
         the hero at {:?} angle {}, {} refreshes on the clock",
        stands.area, stands.place, stands.room, stands.position, stands.angle, loaded.beside.clock
    );
    for entity in &drawn {
        commands.entity(entity).despawn();
    }
    commands.remove_resource::<shown::Things>();
    commands.remove_resource::<field::FieldScreen>();
    commands.remove_resource::<field::RoomViews>();
    commands.remove_resource::<field::WalkLines>();
    // The field's leaving ends its tasks, the sound's among them: the room's music is
    // given back, and the field that starts anew asks for it again.
    music.field_ends();
    // Behind "Continue" of the title nothing has made a state yet.
    let game = Game(loaded.carried.clone());
    settings.0 = loaded.beside.settings;
    clock.set(loaded.beside.clock);
    commands.insert_resource(name_entry::Named {
        units: loaded.name.clone(),
        stored: loaded.beside.name,
    });
    let mut next = ShowRoom::entered(number, &game.0);
    // The start of a second round takes the room's default place.
    if let Some(player) = (next.player.as_mut()).filter(|_| !loaded.default_place) {
        player.stands = Some((stands.position, stands.angle));
    }
    put_room(
        &mut next,
        &game.0,
        (&mut images, &mut meshes, &mut materials),
        &mut commands,
        &mut music,
        &mut exit,
    );
    info!(
        "room {} is on screen by a loaded game, its scripts running",
        next.on_screen
    );
    commands.insert_resource(next);
    commands.insert_resource(game);
}

fn end_game(
    movie: Option<Res<movie_view::ShowMovie>>,
    drawn: Query<Entity, Drawn>,
    mut music: ResMut<music::Music>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
    mut unsaved: ResMut<guards::Unsaved>,
) {
    commands.remove_resource::<saving::Ended>();
    // The player left the game himself: closing the window behind it does not ask.
    unsaved.left();
    let Some(movie) = movie else {
        warn!(
            "\"End Game\": this run started in a room and has no opening movie and no title \
             to go to, the room runs on"
        );
        return;
    };
    info!("\"End Game\": the field ends, the opening movie plays and the title follows");
    for entity in &drawn {
        commands.entity(entity).despawn();
    }
    commands.remove_resource::<shown::Things>();
    commands.remove_resource::<field::FieldScreen>();
    commands.remove_resource::<field::RoomViews>();
    commands.remove_resource::<field::WalkLines>();
    commands.remove_resource::<ShowRoom>();
    music.field_ends();
    commands.spawn(Camera2d);
    match movie_view::Playing::start(&movie.behind_a_game()) {
        Ok(playing) => commands.insert_resource(playing),
        Err(err) => {
            error!("the opening movie behind \"End Game\": {err}");
            exit.write(AppExit::error());
        }
    }
}

fn open_results(
    show: Option<Res<ShowRoom>>,
    things: Option<ResMut<shown::Things>>,
    presses: Res<results_view::ResultsPresses>,
    sum: Res<results_view::ResultsSum>,
    named: Option<Res<name_entry::Named>>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
) {
    // The place map may have taken the room away earlier in this game frame.
    let (Some(show), Some(mut things)) = (show, things) else {
        return;
    };
    let Some(mut results) = things.take_results() else {
        return;
    };
    let fighters = results.copies.len();
    for (counted, &sum) in results.sums.iter_mut().take(fighters).zip(&sum.0) {
        *counted = sum;
    }
    // `exp_multiplier` (`assist.rs`): every fighter's sum, as the screen counts it up.
    let times = assist::now().exp;
    if times != 1.0 {
        let had = results.sums;
        for (sum, fighter) in results.sums.iter_mut().zip(&results.before) {
            *sum = assist::experience(fighter, *sum, times);
        }
        info!(
            "the assists: the sums of experience {had:x?} are {:x?} with {times} times the \
             experience",
            results.sums
        );
    }
    let confirms = things.nobody_at_the_pad();
    let name = named.as_ref().map_or(&[][..], |named| &named.units);
    let opened = match things.carried() {
        Some(carried) => results_view::over_the_room(
            results,
            carried,
            name,
            confirms,
            &presses,
            &mut images,
            &mut commands,
        ),
        None => Err(Box::new((
            results,
            "the room has no state of the game".into(),
        ))),
    };
    match opened {
        Ok(()) => info!(
            "room {}: the fight ends with the results screen, the screen comes up and the \
             room stands",
            show.on_screen
        ),
        // What the screen would give is given without it.
        Err(failed) => {
            let (results, err) = *failed;
            error!("the results screen: {err}");
            things.results_without_a_screen(results);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn go_to_room(
    show: Option<Res<ShowRoom>>,
    mut game: ResMut<Game>,
    things: Option<ResMut<shown::Things>>,
    drawn: Query<Entity, Drawn>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<field::AddedMaterial>>,
    mut commands: Commands,
    mut music: ResMut<music::Music>,
    mut exit: MessageWriter<AppExit>,
) {
    // The place map may have taken the room away earlier in this game frame: the
    // condition of the systems of a room is asked once for all of them.
    let (Some(show), Some(mut things)) = (show, things) else {
        return;
    };
    let mut stays = None;
    let corridor = things.take_corridor();
    let (mut room, slot) = match (things.take_room(), things.take_slot()) {
        (Some(room), _) => (u16::try_from(room).ok(), None),
        (None, Some(slot)) => (None, Some(slot)),
        (None, None) if corridor.is_some() => (None, None),
        (None, None) if things.take_again() => {
            stays = things.stands();
            (Some(show.number), None)
        }
        (None, None) => return,
    };
    game.keep(&things, &show.on_screen);
    // The generator of the corridor of the second round writes the room change itself:
    // the piece's slot, and for a move the place and the angle of the arrival.
    if let Some(asked) = corridor {
        match corridor_goes(&mut game.0, asked) {
            Ok((line, number, arrival)) => {
                info!("room {}: {line}", show.on_screen);
                (room, stays) = (number, arrival);
            }
            Err(err) => {
                error!("room {}: the corridor: {err}", show.on_screen);
                exit.write(AppExit::error());
                return;
            }
        }
    }
    // A slot goes through the variant table as the scripts of the room have left it,
    // and the player stands on the default position of the room behind it; no script
    // of the disc names an arrival place.
    let room = room.or_else(|| {
        let (area, place, slot, arrival) = slot?;
        if let Some(address) = arrival {
            warn!(
                "room {}: the arrival place {address:#010x} of its script is not read, the \
                 player stands on the room's default position",
                show.on_screen
            );
        }
        slot_room(&game.0, area as i16, place as i16, slot as i16)
    });
    let Some(number) = room else {
        warn!(
            "room {}: its script names a room or a slot ({slot:?}) that no room is",
            show.on_screen
        );
        return;
    };
    let by = match corridor {
        Some(_) => "the corridor's generator",
        None => "its script",
    };
    info!(
        "room {}: {by} goes to room {number}, the room goes",
        show.on_screen
    );
    for entity in &drawn {
        commands.entity(entity).despawn();
    }
    commands.remove_resource::<shown::Things>();
    commands.remove_resource::<field::FieldScreen>();
    commands.remove_resource::<field::RoomViews>();
    commands.remove_resource::<field::WalkLines>();
    let mut next = ShowRoom::entered(number, &game.0);
    if let (Some(player), Some(stays)) = (&mut next.player, stays) {
        player.stands = Some(stays);
    }
    put_room(
        &mut next,
        &game.0,
        (&mut images, &mut meshes, &mut materials),
        &mut commands,
        &mut music,
        &mut exit,
    );
    info!(
        "room {} is on screen by a script, its scripts running",
        next.on_screen
    );
    commands.insert_resource(next);
}

fn corridor_goes(
    carried: &mut Carried,
    asked: scripts::CorridorAsks,
) -> Result<(String, Option<u16>, Option<([i32; 3], i16)>), Box<dyn std::error::Error>> {
    use jc_formats::corridor::{self, Corridor};
    let config = LocalConfig::load()?;
    let mut disc = DiscImage::open(&config.disc_image)?;
    let archive = Archive::open(&mut disc)?;
    let package = archive.read(&mut disc, corridor::FILE)?;
    let tables = corridor::Tables::from_package(&package)
        .map_err(|err| format!("the generator's tables: {err:?}"))?;
    let exe = Executable::read(&mut disc)?;
    let model = |species| jc_formats::model::species_model(&exe.image, exe.base, species);
    let pieces = |corridor: &Corridor| {
        let pieces: Vec<String> = (corridor.nodes.iter())
            .map(|node| format!("{} {:?}", node.piece, node.links))
            .collect();
        format!(
            "floor {} of look {} with the species {:?}, its nodes (piece and links): {}",
            corridor.floor,
            corridor.look,
            corridor.species,
            pieces.join("; ")
        )
    };
    let (line, piece, arrival) = match asked {
        scripts::CorridorAsks::Enter => {
            let seed = std::env::args()
                .find_map(|arg| arg.strip_prefix("--corridor-seed=")?.parse::<u32>().ok());
            if let Some(seed) = seed {
                carried.random.seed = seed;
            }
            let before = carried.random.seed;
            let floor = std::env::args()
                .find_map(|arg| arg.strip_prefix("--corridor-floor=")?.parse::<u32>().ok())
                .unwrap_or(1);
            let corridor = Corridor::enter_at(floor, &tables, model, &mut carried.random);
            let piece = corridor.nodes[corridor.at].piece;
            let line = format!(
                "the corridor starts with the generator's word {before}: {}; the game goes \
                 to the first piece, slot {piece}",
                pieces(&corridor)
            );
            carried.corridor = Some(corridor);
            (line, piece, None)
        }
        scripts::CorridorAsks::Link(link) => {
            let corridor = (carried.corridor.as_mut()).ok_or("no generator lives")?;
            let deepest = &mut carried.place_word;
            let moved = corridor
                .go(link, &tables, model, deepest, &mut carried.random)
                .ok_or("the link is empty")?;
            let place = moved.arrival.place;
            let angle = moved.arrival.angle as i16;
            let made = match moved.deeper {
                true => format!(
                    "; the floor is left for {} (the deepest floor is {deepest})",
                    pieces(corridor)
                ),
                false => String::new(),
            };
            let line = format!(
                "the corridor: link {link} leads to node {} by arrival {}, slot {}, the \
                 player on {place:?} with angle {angle}{made}",
                corridor.at, corridor.arrival, moved.piece
            );
            (line, moved.piece, Some((place, angle)))
        }
    };
    let number = slot_room(carried, corridor::AREA, corridor::PLACE, piece as i16);
    Ok((line, number, arrival))
}

#[allow(clippy::too_many_arguments)]
fn enter_place(
    leaves: Res<place_map::Leaves>,
    mut game: ResMut<Game>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<field::AddedMaterial>>,
    mut commands: Commands,
    mut music: ResMut<music::Music>,
    mut exit: MessageWriter<AppExit>,
) {
    commands.remove_resource::<place_map::Leaves>();
    let choice = leaves.choice;
    game.0.choose(&choice);
    let Some(number) = slot_room(&game.0, choice.area, choice.place, 0) else {
        error!(
            "the place map: area {} place {} has no room with a number in its slot 0",
            choice.area, choice.place
        );
        exit.write(AppExit::error());
        return;
    };
    let mut show = ShowRoom::entered(number, &game.0);
    put_room(
        &mut show,
        &game.0,
        (&mut images, &mut meshes, &mut materials),
        &mut commands,
        &mut music,
        &mut exit,
    );
    info!(
        "the place map: area {} place {} is room {}, on screen with its scripts running",
        choice.area, choice.place, show.on_screen
    );
    commands.insert_resource(show);
}

fn slot_room(carried: &Carried, area: i16, place: i16, slot: i16) -> Option<u16> {
    let source = Source::open(carried).ok()?;
    let variant = (source.carried.variants).get(VariantKey { area, place, slot });
    let (area, place) = (usize::try_from(area).ok()?, usize::try_from(place).ok()?);
    source.table.by_slot(area, place, slot, variant)?.1.number
}

/// A music a room asks for as it comes on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RoomMusic {
    /// The music of the room's record, which story variable 12 turns.
    Record(i16),
    Entry(usize),
}

fn ask_musics(music: &mut music::Music, musics: &[RoomMusic], carried: &Carried) {
    for &asked in musics {
        match asked {
            RoomMusic::Record(asked) => music.room_asks(asked, carried.variable(12)),
            RoomMusic::Entry(asked) => music.entry_asks(asked),
        }
    }
}

/// Reads the room that is asked for and puts its view on screen, with the camera of the
/// console's screen.
fn put_room(
    show: &mut ShowRoom,
    carried: &Carried,
    (images, meshes, materials): (
        &mut Assets<Image>,
        &mut Assets<Mesh>,
        &mut Assets<field::AddedMaterial>,
    ),
    commands: &mut Commands,
    music: &mut music::Music,
    exit: &mut MessageWriter<AppExit>,
) {
    let moves = (show.creature_move, show.ordered);
    let asked = (
        show.view,
        show.show_list,
        show.actors,
        moves,
        show.player,
        show.script,
    );
    let (name, (lines, backdrop), musics) = match rooms_walked(show.number, asked, carried, None) {
        Ok(room) => room,
        Err(err) => {
            error!("room {}: {err}", show.number);
            exit.write(AppExit::error());
            return;
        }
    };
    lines.iter().for_each(|line| info!("{line}"));
    ask_musics(music, &musics, carried);
    let number = show.number;
    show.on_screen = name;
    match backdrop {
        Ok(mut view) => {
            if let Some(frame) = show.arena {
                let (package, place, centre, shot) = match fight_place(number, carried, frame) {
                    Ok(place) => place,
                    Err(err) => {
                        error!("room {number}: the place of its fights: {err}");
                        exit.write(AppExit::error());
                        return;
                    }
                };
                info!(
                    "room {number}: its fights are in the package of file {}, round the centre \
                     {:?}; the camera at {:?} looks at {:?}",
                    place.file, centre.place, shot.at, shot.looks
                );
                let things = view.things.take().unwrap_or_else(|| {
                    let camera = view.header.view_camera(jc_formats::background::ZOOM_ONE);
                    let (shows, actors, models) = (&[], Vec::new(), Vec::new());
                    shown::Things::new(None, view.header, camera, shows, None, actors, None, models)
                });
                view.things = Some(things.with_arena(package, place, centre, &shot));
            }
            field::spawn(
                commands,
                images,
                meshes,
                materials,
                view.backdrop,
                show.scroll,
            );
            if show.walk_mesh {
                commands.insert_resource(view.walk);
            }
            if let Some(views) = view.views {
                commands.insert_resource(views);
            }
            if let Some(things) = view.things {
                commands.insert_resource(things);
            }
        }
        // A view that was asked for has to be there; without one the lines are the result.
        Err(err) if show.view.is_some() => {
            error!("room {number}: {err}");
            exit.write(AppExit::error());
        }
        Err(err) => {
            warn!("room {number}: no background shown: {err}");
            commands.spawn(Camera2d);
        }
    }
}

fn fight_place(
    number: u16,
    carried: &Carried,
    frame: Option<u32>,
) -> Result<(Package, arena::Place, Centre, Shot), Box<dyn std::error::Error>> {
    let mut source = Source::open(carried)?;
    let (disc, archive) = (&mut source.disc, &source.archive);
    let (_, room) = (source.table.by_number(number)).ok_or("no room has this number")?;
    let scene = archive.read(disc, room.package_file as usize)?;
    let scene = ScenePackage::parse(&scene)?;
    let data = RoomData::parse(scene.room_data())?;
    let battle = archive.read(disc, arena::PLACES_FILE)?;
    let engine = archive.read(disc, arena::ROOMS_FILE)?;
    let places = Places::from_files(&battle, &engine)?;
    let place = places.get(places.of_room(number)).ok_or("no such place")?;
    let package = Package::parse(&archive.read(disc, place.file as usize)?)?;
    let exe = Executable::read(disc)?;
    let cameras = Cameras::from_executable(&exe.image, exe.base)?;
    let centre = Centre::of(data.default_position(), data.default_angles()[1].into());
    // `--capture-run=SEGMENT[:HUNDREDTHS]` looks through the camera of a capture's run
    // along points instead, at that share of a segment's time.
    let run = std::env::args().find_map(|arg| {
        let asked = arg.strip_prefix("--capture-run=")?.to_owned();
        let (segment, share) = asked.split_once(':').unwrap_or((&asked, "0"));
        Some((segment.parse::<usize>().ok()?, share.parse::<f32>().ok()?))
    });
    if let Some((segment, share)) = run {
        let curves = (cameras.paths.capture.get(segment))
            .ok_or("the capture's run along points has no such segment")?;
        let shot = curves.shot(share / 100.0, &centre.stand());
        return Ok((package, place.clone(), centre, shot));
    }
    let shot = match frame {
        Some(frame) => {
            let arc = mover::ArcTangent::from_image(&exe.image, exe.base)
                .ok_or("the executable has no arc tangent table")?;
            cameras.at_frame(&centre, &arc, frame)
        }
        None => cameras.field[0].shot(&centre),
    };
    Ok((package, place.clone(), centre, shot))
}

#[allow(clippy::too_many_arguments)]
fn leave_room(
    mut show: ResMut<ShowRoom>,
    mut game: ResMut<Game>,
    things: Option<ResMut<shown::Things>>,
    views: Option<ResMut<field::RoomViews>>,
    screen: Option<ResMut<field::FieldScreen>>,
    mut walk: Option<ResMut<field::WalkLines>>,
    parts: Query<Entity, With<field::GroupPart>>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<field::AddedMaterial>>,
    mut music: ResMut<music::Music>,
    mut commands: Commands,
) {
    let (Some(mut things), Some(mut screen)) = (things, screen) else {
        return;
    };
    // An exit is an object task of the room, and the list of a fight has none of them:
    // a player whom a blow moves into an exit's region stays in the fight. The game
    // frame has asked the exits behind the scripts (`Things::take_exit`).
    let Some(exit) = things.take_exit() else {
        return;
    };
    let Some(walker) = things.walking_player() else {
        return;
    };
    let arrival = walker.leaving(exit);
    game.keep(&things, &show.on_screen);
    let asked = (
        None,
        None,
        show.actors,
        (None, actors::Ordered::default()),
        // The player comes into the room with what the game has on him by now.
        show.player.map(|asked| walker::Asked {
            worn: Worn::of(game.0.worn),
            ..asked
        }),
        running(show.script),
    );
    let scripted = asked.5.is_some();
    let read = rooms_walked(show.number, asked, &game.0, Some(arrival))
        .map_err(|err| err.to_string())
        .and_then(|(name, (lines, view), musics)| Ok((name, lines, view?, musics)));
    let (name, lines, view, musics) = match read {
        Ok(read) => read,
        Err(err) => {
            warn!("room {}: the exit leads nowhere: {err}", show.on_screen);
            if let Some(walker) = things.walking_player() {
                walker.forget_exits();
            }
            return;
        }
    };
    lines.iter().for_each(|line| info!("{line}"));
    ask_musics(&mut music, &musics, &game.0);
    // The room on screen is the one a script's place map or "once more" goes by. The
    // room without a number (file 320) keeps the number of the room before it, which
    // lies in the same place.
    if let Ok(number) = name.parse() {
        show.number = number;
    }
    show.on_screen = name;
    if scripted {
        info!(
            "room {} is on screen behind an exit, its scripts running",
            show.on_screen
        );
    }
    field::change_view(
        &mut commands,
        (&mut images, &mut meshes, &mut materials),
        &parts,
        &mut screen,
        walk.as_deref_mut(),
        field::Kept {
            backdrop: view.backdrop,
            header: view.header,
            walk: view.walk,
        },
    );
    if let Some(new) = view.things {
        *things = new;
    }
    match (views, view.views) {
        (Some(mut views), Some(new)) => *views = new,
        (None, Some(new)) => commands.insert_resource(new),
        _ => {}
    }
}

/// One view of a room for the screen: its background, the room's walk mesh seen from it and
/// the list of show records that was asked for.
struct RoomView {
    /// The number of the view.
    index: usize,
    backdrop: field::Backdrop,
    walk: field::WalkLines,
    header: ViewHeader,
    things: Option<shown::Things>,
    /// Every view of the room, when the view follows the player.
    views: Option<field::RoomViews>,
}

/// What `--room` found: the lines to log, and the view or why there is none.
type RoomReport = (Vec<String>, Result<RoomView, String>);

/// Reads one view of a background file into what the screen draws. Adds lines about the view
/// to `lines`.
fn view_backdrop(
    file: &[u8],
    data: &RoomData,
    view: Option<usize>,
    lines: &mut Vec<String>,
) -> Result<RoomView, String> {
    let index = view
        .or_else(|| data.view_at(data.default_position()))
        .unwrap_or(0);
    let record = data
        .views
        .get(index)
        .ok_or_else(|| format!("view {index} of {}", data.views.len()))?;
    let bytes = file
        .get(record.offset as usize..)
        .ok_or("the view lies behind the background file")?;
    let view = View::parse(bytes).map_err(|err| format!("view {index}: {err}"))?;
    let layers: Vec<String> = view
        .layers
        .iter()
        .map(|layer| format!("{}x{} at depth {}", layer.columns, layer.rows, layer.depth))
        .collect();
    let animated = view
        .groups
        .iter()
        .filter(|group| group.frames.len() > 1)
        .count();
    lines.push(format!(
        "view {index}: {} tiles in the sheet, depth range {} to {}, layers in cells [{}], \
         {} groups of which {animated} animated",
        view.tile_count(),
        view.near,
        view.far,
        layers.join(", "),
        view.groups.len()
    ));
    let header = view.camera();
    let point_lights = header
        .point_lights
        .iter()
        .filter(|light| light.is_used())
        .count();
    lines.push(format!(
        "view {index}: camera at {:?} looks at {:?}, roll {}, projection distance {}, light \
         from {:?} of colour {:?}, {point_lights} point lights",
        header.camera,
        header.look_at,
        header.roll,
        header.projection_distance,
        header.light_direction,
        header.light_color
    ));
    let limit = view.scroll_limit(jc_formats::background::ZOOM_ONE);
    lines.push(format!(
        "view {index}: scrolls {} pixels left and right, {} up and down",
        limit.0, limit.1
    ));
    if view.layers.is_empty() {
        return Err(format!("view {index} has nothing to draw"));
    }
    let backdrop =
        field::Backdrop::from_view(&view).map_err(|err| format!("view {index}: {err}"))?;
    let camera = header.view_camera(jc_formats::background::ZOOM_ONE);
    Ok(RoomView {
        index,
        backdrop,
        walk: field::WalkLines::new(camera, data.walk_mesh(), index),
        header,
        things: None,
        views: None,
    })
}

/// Every view of a room read for the screen, for a view that follows the player; none where
/// the background file has nothing to draw for a region of the walk mesh.
fn kept_views(file: &[u8], data: &RoomData) -> Vec<Option<field::Kept>> {
    (0..data.views.len())
        .map(|index| {
            let view = view_backdrop(file, data, Some(index), &mut Vec::new()).ok()?;
            Some(field::Kept {
                backdrop: view.backdrop,
                header: view.header,
                walk: view.walk,
            })
        })
        .collect()
}

/// What is drawn into the view with this header: the list of show records `list` of a room,
/// when one was asked for (the list of the scene package and the six-section package among
/// the room's resources, which holds what the records name), and the actors that stand in
/// the room.
fn shown_things(
    package: &ScenePackage,
    data: &RoomData,
    header: ViewHeader,
    list: Option<(usize, Option<u32>)>,
    (actors, models): (Vec<shown::Standing>, Vec<Vram>),
    actors_frame: Option<u32>,
) -> Result<shown::Things, String> {
    let camera = header.view_camera(jc_formats::background::ZOOM_ONE);
    // The room's package is in memory whether a list of it is shown or not: its images lie
    // next to the models of the actors.
    let things = (data.resources.iter())
        .find(|resource| resource.kind == 0)
        .map(|resource| Package::parse(resource.data).map_err(|err| err.to_string()));
    let Some((list, frame)) = list else {
        return Ok(shown::Things::new(
            things.and_then(Result::ok),
            header,
            camera,
            &[],
            None,
            actors,
            actors_frame,
            models,
        ));
    };
    let lists = package.show_lists();
    let list = lists
        .get(list)
        .ok_or_else(|| format!("show list {list} of {}", lists.len()))?;
    let things = things.ok_or("the room has no package of things to show")??;
    Ok(shown::Things::new(
        Some(things),
        header,
        camera,
        &list.shows,
        frame,
        actors,
        actors_frame,
        models,
    ))
}

/// What the options ask of a room's view: the view, a show list with its game frame, the
/// actors with theirs, a move for the creatures among them with what stands in for the
/// orders of a script, the player, and the scripts of the room with their game frame.
type Asked = (
    Option<usize>,
    Option<(usize, Option<u32>)>,
    Option<Option<u32>>,
    (Option<actors::MoveAsked>, actors::Ordered),
    Option<walker::Asked>,
    Option<(Option<u32>, bool)>,
);

/// What the rooms are read from: the disc, its archive, the room table and the variant table
/// of the chapter that was asked for.
struct Source {
    disc: DiscImage<std::fs::File>,
    archive: Archive,
    table: RoomTable,
    /// What the game carries from room to room, as the room before left it.
    carried: Carried,
    /// The room that is left for the one that is read, by its number (-1 for a room
    /// without one), and whether its flag says that it cannot have roaming creatures;
    /// `None` for a room that is not come into from another.
    left: Option<(i32, bool)>,
}

impl Source {
    /// Opens the disc of `jc.local.toml` for a room of a game in the state `carried`.
    fn open(carried: &Carried) -> Result<Self, Box<dyn std::error::Error>> {
        let config = LocalConfig::load()?;
        let mut disc = DiscImage::open(&config.disc_image)?;
        let archive = Archive::open(&mut disc)?;
        let file = archive.read(&mut disc, rooms::FIELD_ENGINE_FILE)?;
        let table = RoomTable::from_field_engine(&file)?;
        Ok(Self {
            disc,
            archive,
            table,
            carried: carried.clone(),
            left: None,
        })
    }

    fn behind(&self, exit: &Exit) -> Option<RoomLocation> {
        Some(behind(&self.table, &self.carried.variants, exit)?.0)
    }
}

fn story_so_far(chapter: Option<usize>) -> Result<Carried, Box<dyn std::error::Error>> {
    let Some(chapter) = chapter else {
        return Ok(Carried::new_game());
    };
    let config = LocalConfig::load()?;
    let mut disc = DiscImage::open(&config.disc_image)?;
    let archive = Archive::open(&mut disc)?;
    let engine = archive.read(&mut disc, rooms::FIELD_ENGINE_FILE)?;
    let screen = archive.read(&mut disc, chapters::CHAPTER_FILE)?;
    let table = ChapterTable::from_files(&screen, &engine)?;
    Ok(Carried::at_chapter(chapter, table.iter()).ok_or("no such chapter")?)
}

/// The record of the room table behind an exit, for a variant table.
fn behind<'a>(
    table: &'a RoomTable,
    variants: &VariantTable,
    exit: &Exit,
) -> Option<(RoomLocation, &'a rooms::RoomRecord)> {
    let key = VariantKey {
        area: exit.area,
        place: exit.place,
        slot: exit.slot,
    };
    let area = usize::try_from(exit.area).ok()?;
    let place = usize::try_from(exit.place).ok()?;
    table.by_slot(area, place, exit.slot, variants.get(key))
}

/// The scripts for the room behind an exit: those of a room whose scripts run, not those
/// of a room that stands at a game frame of them (`--script=FRAME`), which is a still of
/// that one room.
fn running(script: Option<(Option<u32>, bool)>) -> Option<(Option<u32>, bool)> {
    script.filter(|(frame, _)| frame.is_none())
}

/// The most rooms a player walks through before the first picture, where the game frames of
/// `--player=FRAME` lead through exits.
const ROOMS_IN_A_ROW: usize = 16;

/// A player that left a room through an exit: the room behind it, what the player takes
/// along and the game frames that are left of the ones asked for.
type Left = (RoomLocation, walker::Arrival, u32);

/// What `describe_room` answers: the room's name, its report, the exit the player left
/// through in its game frames, and the music its entry routine asks for.
type Described = (String, RoomReport, Option<Left>, Option<usize>);

/// A room in words: its number, or its file where it has none (the game itself goes by the
/// place of the record in the room table).
fn room_name(record: &rooms::RoomRecord) -> String {
    match record.number {
        Some(number) => number.to_string(),
        None => format!("without a number (file {})", record.package_file),
    }
}

/// Describes a room as `describe_room` does, and the rooms the player walks into from it
/// before it stands: with `--player=FRAME` every exit on the way leads into the next room,
/// where the game frames go on. `leaving` is a player that walks out of a room through an
/// exit, in place of the room `number`. Gives the name of the room the player ends in and
/// the music of the record of every room on the way, for the room change's asking.
fn rooms_walked(
    number: u16,
    mut asked: Asked,
    carried: &Carried,
    leaving: Option<walker::Arrival>,
) -> Result<(String, RoomReport, Vec<RoomMusic>), Box<dyn std::error::Error>> {
    let mut source = Source::open(carried)?;
    let mut lines = Vec::new();
    let mut musics = Vec::new();
    let as_left = |room: &rooms::RoomRecord| (room.number.map_or(-1, i32::from), room.flag != 0);
    let mut next = match leaving {
        Some(arrival) => {
            source.left = (source.table.by_number(number)).map(|(_, room)| as_left(room));
            let room = source.behind(&arrival.exit);
            let at = room.ok_or("the room table has nothing behind the exit")?;
            lines.push(format!(
                "the player leaves through the exit on region {} in his game frame {}",
                arrival.exit.region, arrival.game_frame
            ));
            (at, Some((arrival, 0)))
        }
        None => {
            let room = source.table.by_number(number);
            (room.ok_or("no room has this number")?.0, None)
        }
    };
    for _ in 0..ROOMS_IN_A_ROW {
        let (at, arrived) = next;
        let room = source.table.rooms(at.area, at.place).get(at.room);
        musics.extend(room.map(|room| RoomMusic::Record(room.music)));
        let this = room.map(as_left);
        let (name, (more, view), left, entry_music) =
            describe_room(&mut source, at, asked, arrived)?;
        source.left = this;
        lines.extend(more);
        musics.extend(entry_music.map(RoomMusic::Entry));
        let Some((target, arrival, frames)) = left else {
            return Ok((name, (lines, view), musics));
        };
        lines.push(format!(
            "the player leaves through the exit on region {} with {frames} game frames left",
            arrival.exit.region
        ));
        // A view, a show list, a move and an order are of the room asked for; scripts
        // that run go on running in the room behind the exit.
        asked = (
            None,
            None,
            asked.2,
            (None, actors::Ordered::default()),
            asked.4,
            running(asked.5),
        );
        next = (target, Some((arrival, frames)));
    }
    Err(format!("the player walked through {ROOMS_IN_A_ROW} rooms and did not stand").into())
}

fn figures_of(
    disc: &mut DiscImage<std::fs::File>,
    carried: &Carried,
) -> Result<Vec<actors::Figure>, Box<dyn std::error::Error>> {
    use jc_formats::battle::record::{LEVEL, Record};
    use jc_formats::creatures::{Creature, state};
    let mut creatures: Vec<Creature> = (0..jc_formats::creatures::MOST_CARRIED)
        .filter_map(|n| carried.creatures.nth(state::CARRIED, n))
        .cloned()
        .collect();
    let asked = std::env::args()
        .find_map(|arg| player_menu::ShowMenu::creatures(arg.strip_prefix("--creatures=")?));
    if let Some(asked) = asked {
        let exe = Executable::read(disc)?;
        let tables = jc_formats::battle::Tables::read(&exe.image, exe.base)
            .ok_or("the fights' tables are not in the executable")?;
        let mut draws = carried.random;
        for (species, level) in asked {
            let experience = level.saturating_sub(1).wrapping_mul(LEVEL);
            if let Some(record) = Record::make(&tables, species, experience, &mut draws) {
                creatures.push(Creature::of_a_caught(state::CARRIED, record));
            }
        }
        // `--merged=`: the first of them is merged, as the scripts' load does it.
        if let Some(first) = creatures.first_mut()
            && let Some(made) = scripts::merged_for_a_look(first, &tables, &mut draws)
        {
            *first = made;
        }
    }
    creatures.truncate(jc_formats::creatures::MOST_CARRIED);
    Ok(creatures.iter().map(actors::Figure::of).collect())
}

#[allow(clippy::too_many_arguments)]
fn roamers_of(
    disc: &mut DiscImage<std::fs::File>,
    archive: &Archive,
    (state, draws, player_level): (&mut roaming::Roaming, &mut Random, u8),
    corridor: &mut Option<jc_formats::corridor::Corridor>,
    left: Option<(i32, bool)>,
    at: RoomLocation,
    room: &rooms::RoomRecord,
    positions: &[[i32; 4]],
) -> Result<Option<(String, actors::Roamers)>, Box<dyn std::error::Error>> {
    let Some(number) = room.number else {
        return Ok(None);
    };
    let number = i32::from(number);
    let engine = archive.read(disc, rooms::FIELD_ENGINE_FILE)?;
    let tables = roaming::Tables::from_field_engine(&engine)?;
    let exe = Executable::read(disc)?;
    let bases = (jc_formats::battle::Tables::read(&exe.image, exe.base))
        .ok_or("the fights' tables are not in the executable")?
        .bases;
    let slot = (at.area, at.place, room.slot);
    let (anew, stands, word) = match corridor {
        Some(corridor) => {
            let package = archive.read(disc, jc_formats::corridor::FILE)?;
            let of_floor = jc_formats::corridor::Tables::from_package(&package)
                .map_err(|err| format!("the corridor's tables: {err:?}"))?;
            let filled = *corridor.word() & roaming::FILL != 0;
            let (stands, anew) = corridor.pick(&of_floor, &mut state.list, positions.len(), draws);
            let encounters = assist::now().encounters;
            let stands = assist::thin(corridor.word(), stands, encounters, filled);
            (anew.then_some(state.list), stands, *corridor.word())
        }
        None => {
            let (before, force) = left.unwrap_or((-1, true));
            let anew = state.enter(&tables, &bases, number, before, force);
            // The task that makes the creatures is only woken in a room whose flag is 0.
            if room.flag != 0 || tables.row_of(number).is_none() {
                return Ok(None);
            }
            let filled = (state.word(at.area, at.place, room.slot))
                .is_some_and(|word| word & roaming::FILL != 0);
            let stands = state.pick(&tables, slot, number, positions.len(), draws);
            // `encounters = fewer` (`assist.rs`): half of a room filled anew, and the
            // room's word with them.
            let encounters = assist::now().encounters;
            let stands = match state.word_mut(at.area, at.place, room.slot) {
                Some(word) => assist::thin(word, stands, encounters, filled),
                None => stands,
            };
            let word = state.word(at.area, at.place, room.slot).unwrap_or(0);
            (anew, stands, word)
        }
    };
    let species = |species| {
        let model = jc_formats::model::species_model(&exe.image, exe.base, species)?;
        let turn = jc_formats::model::species_turn(&exe.image, exe.base, species)?;
        Some((u32::from(model), turn))
    };
    // A creature whose place the room does not have is not made.
    let (made, stood): (Vec<_>, Vec<_>) = (stands.iter())
        .filter_map(|stand| {
            let spec = stand.specs[0];
            let place = *positions.get(stand.place)?;
            Some((actors::roamer(spec.species, spec.experience, place), *stand))
        })
        .unzip();
    // The others of each group, which the fight's spawner makes from their species.
    let mates = (stood.iter().enumerate())
        .flat_map(|(number, stand)| {
            let others = stand.block().specs.into_iter().enumerate().skip(1);
            others.map(move |(opponent, spec)| {
                let mate = actors::mate(spec.species, spec.experience);
                (number, opponent, mate)
            })
        })
        .collect();
    let roamers = actors::Roamers {
        noted: roaming::noted(&state.list, species),
        actors: made,
        stands: stood,
        slot,
        mates,
        player_level,
        figures: Vec::new(),
        versus: Vec::new(),
    };
    let stands: Vec<String> = stands
        .iter()
        .map(|stand| {
            let group: Vec<String> = (stand.block().specs.iter())
                .map(|spec| {
                    format!(
                        "species {} level {}",
                        spec.species,
                        (spec.experience >> 26) + 1
                    )
                })
                .collect();
            let [x, y, z, angle] = positions[stand.place];
            format!(
                "number {} on place {} at ({x}, {y}, {z}) angle {angle}: group {} ({})",
                stand.number,
                stand.place,
                stand.group,
                group.join(", ")
            )
        })
        .collect();
    let who = match stands.is_empty() {
        true => "nobody".to_string(),
        false => stands.join("; "),
    };
    let line = format!(
        "roaming creatures of the species list {:?}{}, models noted {:?}, word {:#06x}: {who}",
        state.list,
        if anew.is_some() {
            " (written anew)"
        } else {
            ""
        },
        roamers.noted,
        word
    );
    Ok(Some((line, roamers)))
}

/// Reads a room's scene package from the disc and describes the room:
/// one line for the room, one for its room data, one for each exit and one for each script,
/// and some for the view whose background comes with them. With `arrived` the player comes
/// through an exit and runs the game frames that are left. When the player walks into an
/// exit in its game frames, the room is not put together and the exit is the answer.
fn describe_room(
    source: &mut Source,
    at: RoomLocation,
    (view, show_list, actors, (creature_move, ordered), player, script): Asked,
    arrived: Option<(walker::Arrival, u32)>,
) -> Result<Described, Box<dyn std::error::Error>> {
    let disc = &mut source.disc;
    let (archive, table, variants) = (&source.archive, &source.table, &source.carried.variants);
    let room = (table.rooms(at.area, at.place).get(at.room)).ok_or("no such room")?;
    let number = room_name(room);
    let mut lines = vec![format!(
        "room {number}: area {} place {} record {} slot {}, scene package file {}, \
         background file {}, music {}, flag {}, entry {:#010x}",
        at.area,
        at.place,
        at.room,
        room.slot,
        room.package_file,
        room.background_file,
        room.music,
        room.flag,
        room.entry
    )];
    let package_file = archive.read(disc, room.package_file as usize)?;
    let package = ScenePackage::parse(&package_file)?;
    let list_index = jc_formats::scene::list_index(room.package_file as usize);
    let list = list_index.and_then(|by| {
        let carried = &source.carried;
        let index = by.pick(
            |number| carried.variable(number),
            0,
            |flag| carried.keeping.flag(flag),
        );
        let lists = package.object_lists().len();
        if index < lists {
            lines.push(format!(
                "room {number}: {by:?} picks object list {index} of the package's {lists}"
            ));
            Some(index)
        } else {
            lines.push(format!(
                "room {number}: {by:?} names object list {index}, the package has {lists}: \
                 every list stands"
            ));
            None
        }
    });
    // The view that entry routine asks for with the list: the room starts in it where
    // nothing else was asked.
    let entry_view = list
        .and_then(|list| jc_formats::scene::list_view(room.package_file as usize, list, &package))
        .map(usize::from);
    if let Some(asked) = entry_view {
        lines.push(format!(
            "room {number}: the entry routine asks for view {asked}"
        ));
    }
    let data = RoomData::parse(package.room_data());
    lines.push(match &data {
        Ok(data) => {
            let regions = data.tables.each_ref().map(Vec::len);
            let triangles: usize = data
                .tables
                .iter()
                .flatten()
                .map(|region| region.triangles.len())
                .sum();
            let position = data.default_position();
            let view = match data.view_at(position) {
                Some(view) => format!("in view {view}"),
                None => "off the walk mesh".to_string(),
            };
            let meshes = match MeshBlock::parse(data.models) {
                Ok(block) => format!(
                    "{} mesh objects with {} faces",
                    block.objects.len(),
                    block.face_count()
                ),
                Err(err) => format!("mesh objects not read: {err}"),
            };
            format!(
                "room {number}: {} views, regions per table {regions:?} with {triangles} \
                 triangles, {} resources, {} positions, {meshes}; default position \
                 {position:?} {view}, angles {:?}",
                data.views.len(),
                data.resources.len(),
                data.positions.len(),
                data.default_angles()
            )
        }
        Err(err) => format!("room {number}: room data not read: {err}"),
    });
    let mut roamers = actors::Roamers::default();
    // The seed of the press of Start on the title, taken by the first room behind it.
    // STAND-IN: the game seeds in the pass of the press, and what draws between the
    // press and the first room is not followed.
    let seeded = title::SEED.swap(0, std::sync::atomic::Ordering::Relaxed);
    if seeded != 0 {
        source.carried.random.seed = seeded;
        lines.push(format!(
            "room {number}: the generator starts from the title's seed {seeded}"
        ));
    }
    if let Ok(data) = &data {
        let roamed = std::env::args().find_map(|arg| {
            let value = arg.strip_prefix("--roamed=")?.to_string();
            let (word, seed) = value.split_once(':').unwrap_or((&value, "0"));
            Some((word.parse::<u16>().ok()?, seed.parse::<u32>().ok()?))
        });
        if let (Some((word, seed)), None) = (roamed, source.left) {
            let known = (source.carried.roaming).set_forest(at.area, at.place, word);
            source.carried.random.seed = seed;
            lines.push(format!(
                "room {number}: the forest's word of every room is set to {word:#06x} ({}), the \
                 generator's word to {seed}",
                if known { "written" } else { "not a forest" }
            ));
        }
        let level = source.carried.player_level();
        let state = (
            &mut source.carried.roaming,
            &mut source.carried.random,
            level,
        );
        let corridor = &mut source.carried.corridor;
        let (left, places) = (source.left, &data.positions);
        let made = roamers_of(disc, archive, state, corridor, left, at, room, places)?;
        if let Some((line, made)) = made {
            lines.push(format!("room {number}: {line}"));
            roamers = made;
        }
    }
    // The generator's word behind the picking: the room goes on drawing from it.
    let draws = source.carried.random;
    let left_by_effects = (source.carried.effect_half, source.carried.effect_kept);
    roamers.figures = figures_of(disc, &source.carried)?;
    // The arena of a fight of two players: the local side's minions stand in the place
    // of the carried creatures, the other side's beside them (`versus_view.rs`).
    if let Some((own, other)) = versus_view::figures() {
        lines.push(format!(
            "room {number}: the arena, {} minions on this side and {} on the other",
            own.len(),
            other.len()
        ));
        (roamers.figures, roamers.versus) = (own, other);
    }
    let exits = package.exits();
    for exit in &exits {
        // Without an entry of the variant table the first record of the slot is the target.
        let target = match behind(table, variants, exit) {
            Some((_, record)) => format!("room {}", room_name(record)),
            None => "no such slot".to_string(),
        };
        lines.push(format!(
            "room {number}: exit on region {} to area {} place {} slot {}, which is {target}; \
             arrival {:?} angle {:#06x}",
            exit.region, exit.area, exit.place, exit.slot, exit.arrival, exit.angle
        ));
    }
    for script in package.scripts() {
        let located: Vec<String> = script
            .commands
            .iter()
            .filter_map(|command| command.location())
            .map(|command| describe_location_command(table, command))
            .collect();
        let located = match located.is_empty() {
            true => "no location commands".to_string(),
            false => located.join("; "),
        };
        lines.push(format!(
            "room {number}: script at {:#010x}, {} commands: {located}",
            script.address,
            script.commands.len()
        ));
    }
    for line in describe_keeping(&package) {
        lines.push(format!("room {number}: the script command at {line}"));
    }
    for list in package.show_lists() {
        let shows: Vec<String> = list
            .shows
            .iter()
            .map(|show| {
                let what = match show.shown() {
                    Some(ShowWay::Sprite) => "sprite",
                    Some(ShowWay::Model) => "model",
                    Some(ShowWay::Clip) => "clip",
                    Some(ShowWay::Light) => "light",
                    None => "unknown way",
                };
                let mover = match show.mover {
                    0 => String::new(),
                    mover => format!(", mover {mover:#010x}"),
                };
                format!(
                    "{what} {} at {:?} (frames {} to {} in {} game frames, wait {}, runs {}{mover})",
                    show.number, show.place, show.first, show.last, show.length, show.wait, show.runs
                )
            })
            .collect();
        lines.push(format!(
            "room {number}: show list at {:#010x}: {}",
            list.address,
            shows.join("; ")
        ));
    }
    for actor in package.actors() {
        let who = match actor.creature {
            Some(creature) => format!("species {}", creature.species),
            None => format!("model {}", actor.record.model),
        };
        let table = match (actor.creature, actor.record.bone_table) {
            (Some(_), _) => "the creatures' bone table".to_string(),
            (None, 0) => match package.actor_defaults(actor.spawner) {
                Some(defaults) => format!("default bone table {:#010x}", defaults.bone_table),
                None => "no bone table".to_string(),
            },
            (None, table) => format!("bone table {table:#010x}"),
        };
        let set = match actor.set {
            Some(set) => format!("set {set}"),
            None => "no set".to_string(),
        };
        lines.push(format!(
            "room {number}: actor, {who}, at {:?} angle {}, action {}, {set}, flags {:#x}, \
             {table}, views {:#x}",
            actor.place, actor.angle, actor.action, actor.flags, actor.view_mask
        ));
    }
    let mut standing = match actors {
        Some(_) => {
            let whole = script.is_some();
            actors::standing(
                disc,
                archive,
                &package,
                creature_move,
                whole,
                list,
                &roamers,
                &mut lines,
            )?
        }
        None => (Vec::new(), Vec::new()),
    };
    if actors.is_some() {
        let paths = (data.iter())
            .flat_map(|data| &data.resources)
            .find(|resource| resource.kind == 4)
            .and_then(|resource| mover::paths(resource.data))
            .unwrap_or_default();
        let ground = data.as_ref().ok().map(RoomData::second_ground);
        if let Some(ground) = &ground {
            actors::on_the_ground(
                &mut standing.0,
                &std::sync::Arc::new(ground.clone()),
                &mut lines,
            );
        }
        actors::ordered(&mut standing.0, ordered, &paths, ground, &mut lines);
    }
    let walker = match (player, &data) {
        (Some(asked), Ok(data)) => {
            // The set of the player that the package's entry asks for by its number, or
            // the one it keeps for the list it picked.
            let kept = list.and_then(|list| {
                jc_formats::scene::list_player_set(room.package_file as usize, list)
            });
            let from_map = (script.is_some() && arrived.is_none())
                .then(|| jc_formats::gate::Gates::of(room.package_file as usize, &package))
                .flatten()
                .and_then(|gates| {
                    let forest = source
                        .carried
                        .variable(jc_formats::carried::FOREST_VARIABLE);
                    gates.arrival_place(forest).copied()
                });
            let room = (data, at.area);
            let scripted = script.is_some().then(|| {
                (package.player_sets().into_iter())
                    .find_map(|set| match set {
                        Some(jc_formats::scene::PlayerSet::Number(number)) => Some(number),
                        _ => None,
                    })
                    .or(kept)
                    .map(usize::from)
            });
            let fights = (package.script_commands().iter())
                .any(|command| command.code == jc_formats::scene::COMMAND_FIGHT);
            let forests = !(1..=3).contains(&at.area);
            let reads = fights || forests || !package.kept_set_routines().is_empty();
            let scripted = (scripted, reads);
            let mut walker =
                walker::load(disc, archive, room, &exits, asked, scripted, &mut lines)?;
            walker.things = shown::in_the_way(&standing.0);
            // The controller runs its first game frame and those up to the one to stand at;
            // a player that came through an exit runs what is left of them.
            let frames = match arrived {
                Some((arrival, left)) => {
                    walker.arrive(arrival);
                    lines.push(format!("the player arrives: {}", walker.line()));
                    left
                }
                // With scripts the room runs as a whole from its first game frame.
                None if script.is_some() => {
                    if let Some((place, angle)) = asked.stands {
                        walker.stand_at(place, angle);
                        lines.push(format!(
                            "the player stands where the loaded game or the room read again \
                             has him: {}",
                            walker.line()
                        ));
                    } else if let Some(gate) = from_map {
                        walker.come_out(gate.place, gate.angle);
                        lines.push(format!(
                            "the player comes from the place map: hidden, {}",
                            walker.line()
                        ));
                    }
                    0
                }
                None => asked.frame.unwrap_or(0) + 1,
            };
            if let Some((exit, left)) = walker.run(frames) {
                match behind(table, variants, &exit) {
                    Some((target, _)) => {
                        let left = (target, walker.leaving(exit), left);
                        let report = (lines, Err("the player left".to_string()));
                        return Ok((number, report, Some(left), None));
                    }
                    None => {
                        lines.push(format!(
                            "room {number}: the room table has nothing behind the exit on region {}",
                            exit.region
                        ));
                        walker.forget_exits();
                        walker.run(left);
                    }
                }
            }
            Some((walker, asked.frame.is_some()))
        }
        _ => None,
    };
    // A view that was asked for stays; without one the view is the player's, as in the game.
    let follows = walker.is_some() && view.is_none();
    let view = (view.or(entry_view)).or(walker.as_ref().map(|(walker, _)| walker.stands_in()));
    let entry_music = package.view_musics(room.entry).and_then(|musics| {
        let music = *musics.get(view.unwrap_or(0))?;
        lines.push(format!(
            "room {number}: the entry routine asks for music {music}, the one of view {}",
            view.unwrap_or(0)
        ));
        usize::try_from(music).ok()
    });
    if let Some(stored) = package.entry_stored_room(room.entry) {
        let before = jc_formats::leave_forest::store_room(&mut source.carried, i32::from(stored));
        lines.push(format!(
            "room {number}: the entry routine stores room {stored}, the stored room was {before}"
        ));
    }
    let scripts = match (script, &data) {
        (Some((frame, confirms)), Ok(data)) => {
            let ground = Some(data.second_ground());
            let package_numbered = (&package_file[..], room.package_file as usize, room.number);
            let story = (list, source.carried.clone());
            let mut scripts = scripts::Scripts::load(
                disc,
                package_numbered,
                ground,
                confirms,
                story,
                &mut lines,
            )?;
            // `--answer-in=ROOM:N,...`: the answers a hidden run gives to the questions
            // of a room, in their order, counted from 0 among all answers of a question.
            let answers = std::env::args().find_map(|arg| {
                let (room, answers) = arg.strip_prefix("--answer-in=")?.split_once(':')?;
                (room == number).then(|| {
                    (answers.split(','))
                        .map(|answer| {
                            answer
                                .parse()
                                .expect("--answer-in=ROOM:N,... takes numbers")
                        })
                        .collect()
                })
            });
            scripts.answer_with(answers.unwrap_or_default());
            scripts.lies_in(at.area, at.place);
            Some((scripts, frame))
        }
        _ => None,
    };
    // The tables of the picture decoder, for the clips of the room's show lists.
    let mut clip_tables = Executable::read(disc).ok().and_then(|exe| {
        use jc_formats::movie::{CodeTable, PictureDecoder};
        Some((
            CodeTable::from_executable(&exe.image, exe.base).ok()?,
            PictureDecoder::from_executable(&exe.image, exe.base).ok()?,
        ))
    });
    let backdrop = match &data {
        Ok(data) => {
            let file = archive.read(disc, room.background_file as usize)?;
            view_backdrop(&file, data, view, &mut lines).and_then(|mut view| {
                // A script can name any view of its room, whatever the room started with.
                let mut headers = Vec::new();
                if follows || scripts.is_some() {
                    let views = kept_views(&file, data);
                    lines.push(format!(
                        "the view follows the player or a script: {} of {} views read",
                        views.iter().flatten().count(),
                        views.len()
                    ));
                    headers = (views.iter())
                        .map(|kept| kept.as_ref().map(|kept| kept.header))
                        .collect();
                    view.views = Some(field::RoomViews {
                        views,
                        shown: view.index,
                    });
                }
                if show_list.is_some() || actors.is_some() || walker.is_some() {
                    let frame = actors.flatten();
                    let things =
                        shown_things(&package, data, view.header, show_list, standing, frame)?;
                    // The room's own mesh objects and their images.
                    let textures = jc_formats::tim::texture_set(data.textures);
                    let things = things.with_room(
                        jc_formats::mesh::MeshBlock::parse(data.models).ok(),
                        &textures.unwrap_or_default(),
                        view.index,
                    );
                    let things = match walker {
                        Some((walker, still)) => things.with_player(walker, still),
                        None => things,
                    };
                    let things = things.with_views(headers).with_draws(draws);
                    let things = things.with_effects(left_by_effects.0, left_by_effects.1);
                    // The second view of the room, which the screen of file 44 looks
                    // at a creature through.
                    let second = (data.resources.iter())
                        .find(|resource| resource.kind == 3)
                        .and_then(|resource| ViewHeader::parse(resource.data));
                    let things = things.with_second_view(second, room.package_file as usize);
                    let things = things.with_clips(clip_tables.take());
                    view.things = Some(things.started(scripts));
                }
                Ok(view)
            })
        }
        Err(err) => Err(format!("room data not read: {err}")),
    };
    Ok((number, (lines, backdrop), None, entry_music))
}

/// One location command of a script in words.
fn describe_location_command(table: &RoomTable, command: LocationCommand) -> String {
    match command {
        LocationCommand::StartChapter { chapter } => format!("start chapter {chapter}"),
        LocationCommand::SetStateWord { value } => format!("state word = {value}"),
        LocationCommand::SetVariant {
            area,
            place,
            slot,
            room,
        } => format!("area {area} place {place} slot {slot} now leads to room {room}"),
        LocationCommand::GoToSlot {
            area,
            place,
            slot,
            arrival,
        } => match arrival {
            Some(address) => {
                format!("go to area {area} place {place} slot {slot}, arrival at {address:#010x}")
            }
            None => format!("go to area {area} place {place} slot {slot}"),
        },
        LocationCommand::GoToRoom { room } => {
            let file = u16::try_from(room)
                .ok()
                .and_then(|room| table.by_number(room))
                .map(|(_, record)| format!("file {}", record.package_file));
            let file = file.unwrap_or_else(|| "no such room".to_string());
            format!("go to room {room} ({file})")
        }
        LocationCommand::OpenPlaceMap => "open the place map".to_string(),
    }
}

fn describe_keeping(package: &ScenePackage) -> Vec<String> {
    let name = |kind: Kind| match kind {
        Kind::Item => "item",
        Kind::Weapon => "weapon",
        Kind::Armour => "armour",
        Kind::Ornament => "ornament",
        Kind::Creature => "thing of kind 4",
        Kind::StoryThing => "thing of the story",
        Kind::Cocoons => "empty cocoons",
        Kind::Money => "money",
    };
    let mut lines = Vec::new();
    for command in package.script_commands() {
        let at = command.address;
        match command.keeping() {
            Some(KeepingCommand::SetFlag { flag }) => {
                lines.push(format!("{at:#010x} sets flag {flag}"))
            }
            Some(KeepingCommand::ClearFlag { flag }) => {
                lines.push(format!("{at:#010x} clears flag {flag}"));
            }
            Some(KeepingCommand::Give {
                kind,
                number,
                count,
            }) => lines.push(format!(
                "{at:#010x} gives {count} of {} {number}",
                name(kind)
            )),
            Some(KeepingCommand::Take {
                kind,
                number,
                count,
            }) => lines.push(format!(
                "{at:#010x} takes {count} of {} {number}",
                name(kind)
            )),
            Some(KeepingCommand::GiveMoney { amount }) => {
                lines.push(format!("{at:#010x} gives {amount} money"));
            }
            Some(KeepingCommand::GiveCocoons { count }) => {
                lines.push(format!("{at:#010x} gives {count} empty cocoons"));
            }
            Some(KeepingCommand::GiveStoryThing { number }) => {
                lines.push(format!("{at:#010x} gives thing {number} of the story"));
            }
            Some(KeepingCommand::TakeStoryThing { number }) => {
                lines.push(format!("{at:#010x} takes thing {number} of the story"));
            }
            _ => {}
        }
        let Some(notice) = package.notice(&command) else {
            continue;
        };
        let Some(things) = notice.things else {
            lines.push(format!(
                "{at:#010x} notice of way {} with the things of its task's thing",
                notice.way
            ));
            continue;
        };
        for thing in things {
            let kind = Kind::of(thing.kind).map_or("thing of no kind", name);
            lines.push(format!(
                "{at:#010x} notice of way {}: {} of {kind} {}",
                notice.way, thing.count, thing.number
            ));
        }
    }
    lines
}

fn smoke_exit(mut smoke: ResMut<Smoke>, mut commands: Commands, mut exit: MessageWriter<AppExit>) {
    if smoke.frames_left > 0 {
        smoke.frames_left -= 1;
        return;
    }
    let Some(capture) = &mut smoke.capture else {
        exit.write(AppExit::Success);
        return;
    };
    if capture.requested.is_none() {
        if let Some(dir) = capture.path.parent()
            && let Err(err) = std::fs::create_dir_all(dir)
        {
            error!("cannot create {}: {err}", dir.display());
            exit.write(AppExit::error());
            return;
        }
        capture.requested = Some(SystemTime::now());
    }
    // One frame after the other is asked for, until all are.
    if capture.taken < capture.frames {
        let path = capture.path_of(capture.taken);
        capture.taken += 1;
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
        return;
    }
    let Some(requested) = capture.requested else {
        return;
    };
    // The capture is read back from the GPU a few frames later. An older file of the same
    // name does not count: it has to be written after the request. The last one asked
    // for is waited for.
    let written = std::fs::metadata(capture.path_of(capture.frames - 1))
        .and_then(|meta| Ok((meta.len(), meta.modified()?)))
        .is_ok_and(|(len, modified)| len > 0 && modified >= requested);
    if written {
        exit.write(AppExit::Success);
    } else if capture.frames_waited >= CAPTURE_TIMEOUT_FRAMES {
        error!("no screenshot at {} after waiting", capture.path.display());
        exit.write(AppExit::error());
    } else {
        capture.frames_waited += 1;
    }
}
