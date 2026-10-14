
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bevy::log::{debug, info, warn};
use jc_disc::{Archive, DiscImage, Executable};
use jc_formats::action::WINDOWS;
use jc_formats::area::DrawArea;
use jc_formats::background::ViewCamera;
use jc_formats::bars::{self, Bars, Sliding};
use jc_formats::battle::Tables;
use jc_formats::battle::arena::{self, Cameras, Centre, Places, Shot};
use jc_formats::battle::effects;
use jc_formats::battle::fight::{
    Block, Choice, Event, Fight, OPPONENTS, PLAYER, Party, Refusal, Run, SLOTS, Saying, Step,
    ended, is_boss, surprise,
};
use jc_formats::battle::hud as fight_hud;
use jc_formats::battle::lesson;
use jc_formats::battle::menu as fight_menu;
use jc_formats::battle::menu_panel as fight_menu_panel;
use jc_formats::battle::moves::{self, Asked};
use jc_formats::battle::record::{Draws, LEVEL, Random, Record};
use jc_formats::battle::resolve::NO_DEVA;
use jc_formats::battle::results::{Results, Spoils};
use jc_formats::battle::runner::{self, Showing};
use jc_formats::beat::{self, Beat};
use jc_formats::card::{self, Card, Floating};
use jc_formats::carried::{Carried, FOREST_VARIABLE};
use jc_formats::command::{self, Request};
use jc_formats::corridor::{self, Corridor};
use jc_formats::creatures;
use jc_formats::door::{self, Door, Swing};
use jc_formats::ending;
use jc_formats::fade::Fade;
use jc_formats::field::{Ground, Region, RoomData};
use jc_formats::gate::{self, Crossing, Gates};
use jc_formats::glide::{self, Glide, Zoom};
use jc_formats::halo::{self, Halo, Haloing};
use jc_formats::items;
use jc_formats::leave::{self, MapTrigger, Waiting};
use jc_formats::leave_forest::{self, Leaving};
use jc_formats::lock::{self, Lock, Locking};
use jc_formats::mesh::MeshBlock;
use jc_formats::message::{self, Group};
use jc_formats::mover;
use jc_formats::movie_object::{self, Shower};
use jc_formats::notice;
use jc_formats::package::Package;
use jc_formats::pad::{self, Pad, Repeat};
use jc_formats::passage::{self, Passages, Passing};
use jc_formats::played::{self, Played, Playing};
use jc_formats::player::{self, Orders};
use jc_formats::ride::{self, Rides, Riding, Sounding, Sounds};
use jc_formats::roamer::ItemCount;
use jc_formats::roaming;
use jc_formats::scatter::{self, Scatter};
use jc_formats::scene::{
    self, EquipRoutine, Exit, KeepingCommand, LocationCommand, PACKAGE_BASE, ReadRoutine, Reads,
    RoomBits, ScenePackage, Show, ShowClock,
};
use jc_formats::script::{self, Answer, Area, Call, Engine, Followed, Order, Scene, THINGS, Task};
use jc_formats::search::{self, Finds, Search};
use jc_formats::show_mover::{self, Moved, ShowMover};
use jc_formats::skeleton::{Placement, cosine, sine};
use jc_formats::sky::{Course, Sky};
use jc_formats::sound;
use jc_formats::spot::{self, Marked, Spot};
use jc_formats::switch::{self, Barriers, Follower, Following, Pressed, Switch};
use jc_formats::text::{self, Font, Fonts};
use jc_formats::toll::{self, Toll};
use jc_formats::voice;
use jc_formats::warp::{self, Warping, Warps};
use jc_formats::window::{self, Window};

use crate::field::FieldScreen;
use crate::shown::Standing;
use crate::walker::Walker;

/// A place and an angle about the upright axis.
type Placed = ([i32; 3], i16);

/// The code of cue 0 in the text font, which the notice's step drops as a first line.
const CUE_0: u16 = 199;
const SPEECH: u32 = 1 << 22;

/// The script tasks of a room and what they share.
pub struct Scripts {
    /// The file of the scene package, which holds the commands and what they name.
    file: Vec<u8>,
    /// The archive file it is.
    number: usize,
    /// Every task with the game frame it ended or stopped in, none while it runs.
    tasks: Vec<(Task, Option<u32>)>,
    room: Room,
}

/// What limits the zoom while no point has been projected yet: nothing.
const NO_DEPTH: i32 = i32::MAX / 4;

#[derive(Debug, Clone, Copy)]
pub struct CameraPoints {
    pub followed: [i32; 3],
    pub target: [i32; 3],
    pub share: i32,
}

#[derive(Debug, Clone, Copy)]
struct AskedScreen {
    script: i32,
    kind: u32,
    argument: u32,
    state: Asking,
    /// What the screen answers (`0x800AEDC4`), which the script takes as its choice.
    answer: i32,
}

/// How far a screen of field mode 5 is: asked for by the script, started by the field
/// (which clears the first word), and left again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Asking {
    Asked,
    Up,
    Left,
}

/// What the tasks of a room share.
struct Room {
    /// The four command lists of the executable that a request for an action runs.
    built_in: [Vec<command::Command>; 4],
    /// The player's order record.
    orders: Orders,
    selector: i32,
    carried: Carried,
    setting: u32,
    drawn: i32,
    /// Where a thing stood when a task took it (`0x07`; HYPOTHESIS: what `0x13` without an
    /// address puts it back on).
    noted: [Option<Placed>; THINGS],
    /// Tables 3 and 4 of the room, for an actor that is told to walk over the ground.
    ground: Option<Arc<Ground>>,
    /// The fade of the picture.
    fade: Fade,
    background: bool,
    before: bool,
    /// The draw area as `0x5E` has left it.
    area: DrawArea,
    /// What the camera follows (`0x60`) and its glide to another point (`0x5A` to `0x5D`).
    followed: Followed,
    glide: Glide,
    /// The zoom (`0x5F`), and what the projection of the point the picture is kept on
    /// answered in the last game frame, which limits it.
    zoom: Zoom,
    depth: Option<i32>,
    /// What the camera task made of the last game frame.
    camera: Option<CameraPoints>,
    /// The view that is read ahead (`0x55`), and the view a script wants on screen and
    /// the room view has not taken yet (`0x56`, `0x61`).
    read_ahead: Option<u32>,
    view: Option<usize>,
    /// A script has asked for the place map (`0x58`) and the room view has not taken
    /// it yet.
    map: bool,
    screen: Option<AskedScreen>,
    screen_sleeps: bool,
    results: Option<Results>,
    /// The script whose fight has ended with the results screen and which goes on when
    /// the screen has left (the field's fight task waits for field mode 1 in its state
    /// `0x800A160C`, and the `0x65` for that task).
    awaits_results: Option<i32>,
    /// What the scripts, a gate and the message windows have asked of the music and
    /// the sounds and the app's music has not taken yet, in the order it was asked,
    /// each with its game frame.
    music: Vec<(u32, Heard)>,
    cues: Vec<voice::Cue>,
    spoken: Option<voice::Cue>,
    spoke: bool,
    /// The page of the window a line was started for last (`0x800CD124` is 1 while it
    /// is the page on screen), and what the voices answered behind the game frame
    /// before: no line plays (`0x80040E20`), none is asked for (`0x80040E40`).
    spoken_page: Option<usize>,
    voice_silent: bool,
    voice_none_asked: bool,
    /// The stand-in for the voices of a room stood at a game frame
    /// ([`Scripts::voices_ahead`]): the game frames the line asked for last still plays,
    /// and how many of the requests of the sound it has looked at.
    voice_ahead: Option<u32>,
    voice_seen: usize,
    /// The room number a script has sent the game to (`0x53`). Nothing of the room runs
    /// from then on: the console tears the room down.
    leaves: Option<u32>,
    /// The room view has taken that room.
    left: bool,
    leaving: Option<(Leaving, [i32; leave_forest::VIEWS], leave_forest::From)>,
    /// The chapter whose screen a script has handed the game to (`0x4F`), and the slot a
    /// script has sent it to (`0x52`: area, place, slot and the address of an arrival
    /// place), which the room view has not taken yet. The room is over as behind `0x53`.
    chapter: Option<u32>,
    slot: Option<(i32, u16, u16, Option<u32>)>,
    /// The generator of the corridor of the second round in this room, and what it has
    /// asked of the room loader that the room view has not taken yet. The room is over
    /// as behind `0x53`.
    generator: Option<Generator>,
    corridor_asks: Option<CorridorAsks>,
    /// A native routine has asked for the same room once more (the force flag of the
    /// room change) and the room view has not taken it yet.
    again: bool,
    /// The confirm button was pressed in this game frame.
    confirm: bool,
    /// The other buttons a player holds in this game frame: the directions that move
    /// the cursor of a question, and the triangle.
    keys: u16,
    /// The text font and the groups of the room's text, for the windows of the messages.
    font: Font,
    groups: Vec<Group>,
    /// Where among the groups the text of a message stands that names it by address.
    named: BTreeMap<u32, usize>,
    /// The pad's words of the game frame, which the windows read.
    pad: Pad,
    /// Nobody is at the pad: the confirm goes down whenever a window stands.
    confirms: bool,
    /// The answers a run with nobody at the pad gives to the questions of the room, in
    /// their order (`--answer-in`), and the questions asked so far.
    answers: Vec<i16>,
    asked: usize,
    /// The spawner of the package's show records, when it has one; the argument
    /// pointer of each task by its number (`0x4B`), or the argument record it names
    /// instead (`0x4C`); and the records of the lists that were handed to the spawner
    /// and that the room view has not taken yet.
    spawner: Option<u32>,
    arguments: BTreeMap<i32, u32>,
    records: BTreeMap<i32, [u32; 3]>,
    shows: Vec<Show>,
    movie_routine: Option<u32>,
    movie: Option<u32>,
    busy: bool,
    behind: Option<u32>,
    seen_at: u32,
    darkened: Option<u32>,
    shake: Option<usize>,
    shake_table: [i8; arena::SHAKE_FRAMES],
    tints: Vec<jc_formats::tint::Tint>,
    shaken: i32,
    flipped: BTreeMap<u32, bool>,
    rested: bool,
    roamers_held: bool,
    roamers_let_go: bool,
    flash: u8,
    clear_routine: Option<u32>,
    cleared: bool,
    read_routines: Vec<ReadRoutine>,
    kept_set_routines: Vec<(u32, i16)>,
    kept_set: Option<i16>,
    /// The routines of the package that fade the running sequence of the sound record
    /// that stays in memory.
    fade_routines: Vec<u32>,
    fights: Option<Rules>,
    outcome: u32,
    lesson: Option<u32>,
    fight_place: Option<FightPlace>,
    fight: Option<Lasting>,
    creature_moves: Vec<Vec<command::Command>>,
    own_moves: BTreeMap<u32, Vec<Vec<command::Command>>>,
    fight_asks: Option<(u32, u32)>,
    /// The attack --fight-asks=6:SLOT:ATTACK names for the body attack it asks for.
    fight_attack: Option<u8>,
    /// The effect's loader is idle (`0x800148D4`): no effect of a move lives, as the
    /// room's view has left it behind the effect tasks of the game frame before.
    effect_idle: bool,
    /// The effects of the actors that were let go from their hold in this game frame,
    /// for the room's view to start.
    effects_started: Vec<ActorEffect>,
    /// A fight that is on screen stands where the player's side chooses a command and
    /// waits for it, in a game somebody plays and with `--fight-menu`; in a run nobody
    /// plays (`--smoke`, `--screenshot`) and with `--fight-auto` the build chooses in
    /// the game frame of the fight's start. The menu of the choice is driven by the
    /// pad and painted by `fight_menu_view`.
    fight_menu: bool,
    /// `--fight-press=FRAME[-FRAME]:BUTTON,...`: buttons that are down in game frames
    /// of the room, a check option for a hidden run that drives the menu of a fight.
    fight_presses: Vec<(u32, u32, u16)>,
    /// The cursor among the opponents as the last menu of a fight left it (the byte
    /// `0x800C9F26`). Stand-in: it is kept with the room, the game keeps it for good.
    menu_kept: i16,
    /// The paths of the room, its kind 4 resource, which `0x4A` names by number.
    paths: Vec<mover::Path>,
    equip_routines: Vec<EquipRoutine>,
    /// The object of the sky with the address of its record, once a script has started
    /// it, and whether its routine
    /// drew in the last game frame.
    sky: Option<(u32, Course, Sky, bool)>,
    beat: Option<Beat>,
    toll: Option<Toll>,
    spots: Vec<(Spot, Option<Region>, Option<Marked>)>,
    ending: Option<(usize, u32)>,
    end_card: Option<[u8; 24]>,
    doors: Vec<(u32, Door, Swing)>,
    door_scripts: Option<door::Scripts>,
    door_owners: BTreeMap<i32, u32>,
    /// The lock of a door, with its task.
    lock: Option<Locked>,
    /// The place of the forests' area the room lies in, for the way back of a fight
    /// that is lost for good; none for a room outside the forests.
    forest_place: Option<i16>,
    shower: Option<(u32, Shower)>,
    idle: Vec<u32>,
    /// The object has played its movie and the music it kept is still to be asked for
    /// again: the console asks behind the movie player's return, in the same pass, the
    /// build in the first game frame behind the movie.
    music_back: bool,
    scatter: Option<Scattered>,
    halo: Option<(Haloing, Random)>,
    sparks: Vec<halo::Spark>,
    gates: Option<Gates>,
    crossings: Vec<Crossed>,
    owners: BTreeMap<i32, usize>,
    passages: Option<Passages>,
    passings: Vec<Passaged>,
    passage_owners: BTreeMap<i32, usize>,
    rides: Option<(Rides, Riding, Option<i32>, bool)>,
    warps: Option<(Warps, Warping, Option<i32>, (i16, i16))>,
    /// The object of the ride's room that plays a sound at a pace, and its task.
    ride_sounds: Option<(Sounds, Sounding)>,
    /// The camera's object of the ride's room, when the list the room uses has it: the
    /// room view passes its task, which works on the view's header.
    ride_camera: Option<ride::Follows>,
    slides: Option<(u32, Bars, Sliding, [bars::Rect; 2])>,
    plays: Option<(u32, Played, Playing, [i32; 2])>,
    /// The object that plays a gate for a player who came from the place map with the
    /// number of a forest, when the room started with one.
    arriving: Option<Arrives>,
    finds: Option<(Finds, Searched)>,
    card: Option<Carded>,
    switches: Vec<(Switch, Pressed)>,
    barriers: Vec<Barriers>,
    /// The pictures that follow story flags, each with its task once the task has
    /// started in a view: that view and the group of its background the object holds.
    followers: Vec<(Follower, Option<Held>)>,
    /// The state a script has told a picture, by the address of its record: told state
    /// 0 it changes as if its flag were set.
    pictures_told: BTreeMap<u32, i32>,
    exits: BTreeMap<u32, i32>,
    /// The triggers of the place map, each with its task and the number of the script
    /// it started.
    map_triggers: Vec<(MapTrigger, Waiting, Option<i32>)>,
    /// What the task of a number last said.
    said: BTreeMap<i32, Said>,
    /// The notice of the task of a number, while it is up.
    noticed: BTreeMap<i32, Noticed>,
    /// Table 5 of the room: the trigger regions that `0x62` and `0x64` name.
    triggers: Vec<Region>,
    /// The tasks that wait for a talk (`0x62`) with a thing the player can talk to (bit
    /// `0x08` of a task's flags), and those the pad routine has given the press of the
    /// confirm button (bit `0x04`).
    can_talk: BTreeSet<i32>,
    talked: BTreeSet<i32>,
    game_frame: u32,
    bits: u32,
    by_view: Vec<(i32, u32, u32)>,
    made_for: Option<usize>,
    /// The commands that went on without being run, by their number.
    left_out: BTreeMap<u32, u32>,
}

/// What a room asks of the app's music: a music call of a script, a sound by its
/// number, or a call a fight of a script makes of the sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Heard {
    Music(script::Music),
    Sound(u32),
    Fight(sound::FightAsks),
    /// The calls 14 and 15 of a script's `0x67`: a sound whose voice is kept in the
    /// word at an address, and the end of the voice that word names.
    Kept {
        at: u32,
        sound: u32,
    },
    Ended {
        at: u32,
    },
    Line(voice::Line),
    LineEnds,
    Got(usize),
    FadeKept,
    Effect {
        number: u16,
        level: u8,
    },
}

/// The task of a picture that follows a story flag, started in a view: the group of that
/// view's background it holds, none when the view has no group of its number.
struct Held {
    view: usize,
    group: Option<usize>,
    following: Following,
}

/// The object of the memory card with its task.
struct Carded {
    card: Card,
    floating: Floating,
    /// What the routine drew in the last game frame.
    drawn: Option<[card::Drawn; 2]>,
    /// The mesh block the routine draws from, archive file 10.
    block: Option<MeshBlock>,
    text: [u8; 24],
    /// The four words the routine left for the field's task, until that task reads
    /// them: the screen is asked for.
    asked: bool,
}

/// A message command as its window needs it.
#[derive(Debug, Clone)]
struct Said {
    /// The thing that speaks and the thing it speaks to.
    thing: i32,
    other: u32,
    /// The group of the room's text.
    group: usize,
    /// The picture's box is on the left of the text.
    picture_left: bool,
    /// The answers of a question (`0x39`, `0x3A`) that can be picked.
    open_answers: u32,
    /// Where the window stands: the command's place, or the one the task holds (`0x35`).
    place: window::Where,
    /// What the window's last game frame drew, and how many it has had.
    pass: window::Pass,
    passes: u32,
}

#[derive(Debug, Clone)]
pub struct Noticed {
    /// The way, -1 to 11: what the notice says was done with the things.
    pub way: i32,
    pub things: [notice::Thing; notice::THINGS],
    /// The lines of text, in the units of the room's text.
    pub lines: Vec<Vec<u16>>,
}

/// What a fight draws over its picture, as the last game frame left it, for whoever
/// paints it: the command menu while the fight waits for the player's side (none while
/// a command plays), what its list of the items shows, the rows of the picker of a
/// target on the own side (the slot of each and whether it is closed; empty in a pass
/// that does not draw the picker), the HUD, and the game frame of the scripts.
pub struct FightMenu<'a> {
    pub menu: Option<&'a fight_menu::Menu>,
    pub items: fight_menu_panel::Items,
    /// What the two lists of the creature that chooses show; none for the player.
    pub moves: Option<fight_menu_panel::Moves>,
    pub own: Vec<(usize, bool)>,
    /// What the HUD's last pass drew, and the name of the record of each slot's
    /// fighter, which a box of the HUD shows.
    pub hud: Vec<fight_hud::Drawn>,
    pub names: [Option<[u8; jc_formats::name::LETTERS]>; SLOTS],
    pub game_frame: u32,
}

/// The item at each place of the player's list of items; `None` for a free place.
/// `--merged=SPECIES[:LEVEL],...`: a check option for a look at a figure of several
/// models. The creature a merge makes of `first` and a creature of that species, made
/// as the set-up of a fight makes one; with more species, what one merge after the
/// other makes of it. `--merged-skin=THING` with it: the last merge takes that minion
/// skin. `None` without the option.
pub fn merged_for_a_look(
    first: &creatures::Creature,
    tables: &Tables,
    draws: &mut Random,
) -> Option<creatures::Creature> {
    let asked = std::env::args()
        .find_map(|arg| crate::player_menu::ShowMenu::creatures(arg.strip_prefix("--merged=")?))?;
    let skin = std::env::args()
        .find_map(|arg| arg.strip_prefix("--merged-skin=")?.parse::<usize>().ok())
        .and_then(jc_formats::merge::skin_look);
    let mut made: Option<creatures::Creature> = None;
    for (at, &(species, level)) in asked.iter().enumerate() {
        let experience = level.saturating_sub(1).wrapping_mul(LEVEL);
        let second = Record::make(tables, species, experience, draws)?;
        let second = creatures::Creature::of_a_caught(creatures::state::KEPT, second);
        let skin = skin.filter(|_| at + 1 == asked.len());
        let own = made.as_ref().unwrap_or(first);
        made = Some(jc_formats::merge::merged(
            own, &second, skin, tables, draws,
        )?);
    }
    made
}

fn item_places(keeping: &items::Keeping) -> Vec<Option<usize>> {
    (0..items::PAIRS)
        .map(|place| keeping.number_at(place, items::Kind::Item))
        .map(|number| (number != items::FREE).then_some(usize::from(number)))
        .collect()
}

/// The value of `--fight-press=FRAME[-FRAME]:BUTTON,...`: buttons that are down in a
/// game frame of the room, or from one to another, for a hidden run that drives the
/// menu of a fight.
/// Whether a fight waits for the player's commands, by the app's options: always with
/// `--fight-menu`, never with `--fight-auto`, and else in every run somebody plays,
/// which is one without `--smoke` and `--screenshot`.
fn fight_menu_asked(args: impl Iterator<Item = String>) -> bool {
    let (mut menu, mut auto, mut nobody_plays) = (false, false, false);
    for arg in args {
        menu |= arg == "--fight-menu";
        auto |= arg == "--fight-auto";
        nobody_plays |= arg.starts_with("--smoke") || arg.starts_with("--screenshot=");
    }
    menu || !(auto || nobody_plays)
}

fn fight_presses(value: &str) -> Option<Vec<(u32, u32, u16)>> {
    (value.split(','))
        .map(|press| {
            let (frame, button) = press.split_once(':')?;
            let button = match button {
                "cross" => pad::CROSS,
                "triangle" => pad::TRIANGLE,
                "circle" => pad::CIRCLE,
                "up" => pad::UP,
                "down" => pad::DOWN,
                "left" => pad::LEFT,
                "right" => pad::RIGHT,
                "r1" => pad::R1,
                "r2" => pad::R2,
                _ => return None,
            };
            let (from, to) = frame.split_once('-').unwrap_or((frame, frame));
            Some((from.parse().ok()?, to.parse().ok()?, button))
        })
        .collect()
}

/// A message window of the room's scripts as its last game frame left it, for whoever
/// paints it.
pub struct Spoken<'a> {
    /// The archive file of the room's package.
    pub file: usize,
    /// The thing that speaks and the thing it speaks to.
    pub thing: usize,
    pub other: usize,
    pub group: &'a Group,
    pub picture_left: bool,
    /// The answers of a question that can be picked.
    pub open_answers: u32,
    /// Where the window stands: a question at the top and any other message at the
    /// bottom, unless the script's task holds a place.
    pub place: window::Where,
    pub pass: &'a window::Pass,
    /// Game frames the window has had: the count of the sign's pulse.
    pub passes: u32,
    /// The game frame of the scripts.
    pub game_frame: u32,
}

/// The task of a gate: its words, the number of the script it started last, and the
/// first record of the show list it handed over with the clock of that record's task.
#[derive(Default)]
struct Crossed {
    crossing: Crossing,
    script: Option<i32>,
    shown: Option<(Show, ShowClock)>,
}

/// The task of a bank of a passage: its words and the number of the script it started
/// last.
#[derive(Default)]
struct Passaged {
    passing: Passing,
    script: Option<i32>,
}

/// The generator of the corridor of the second round as a room has it: its start
/// routine has still to run (the object was made with this room), or its step runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Generator {
    Starts,
    Steps,
}

/// What that generator asks of the room loader: floor 1 and its first piece, or the
/// move through a link of the player's node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorridorAsks {
    Enter,
    Link(usize),
}

/// The task of the object for a player who came from the place map: the address of its
/// record, the record, its words and the number of the script it started.
struct Arrives {
    at: u32,
    arrival: gate::Arrival,
    arriving: gate::Arriving,
    script: i32,
    /// The light of a lost fight that state 2 starts in the packages 304 and 307.
    light: Option<Halo>,
}

/// The lock of a door with its task: the number of the script it started last and the
/// list of lines it left for that script (+140 of its task).
struct Locked {
    lock: Lock,
    locking: Locking,
    script: Option<i32>,
    lines: u32,
}

/// The task of a thing to be searched: its words and the number of the script it started
/// last.
struct Searched {
    search: Search,
    script: Option<i32>,
}

/// The object of room 8 with the words of its task, and the list and the count its
/// routine drew with in the last game frame.
struct Scattered {
    places: Scatter,
    run: scatter::Run,
    drawn: Option<(usize, i32)>,
}

pub struct FightPlace {
    pub package: Package,
    pub place: arena::Place,
    cameras: Cameras,
    arc: mover::ArcTangent,
}

impl FightPlace {
    /// The place of the room with this number; a room without a number is in no list of
    /// the places and fights in place 0, as a room in no list does.
    fn read(disc: &mut DiscImage<File>, exe: &Executable, room: Option<u16>) -> Option<Self> {
        let archive = Archive::open(disc).ok()?;
        let battle = archive.read(disc, arena::PLACES_FILE).ok()?;
        let engine = archive.read(disc, arena::ROOMS_FILE).ok()?;
        let places = Places::from_files(&battle, &engine).ok()?;
        let place = places.get(room.map_or(0, |room| places.of_room(room)))?;
        let package = archive.read(disc, place.file as usize).ok()?;
        Some(Self {
            package: Package::parse(&package).ok()?,
            place: place.clone(),
            cameras: Cameras::from_executable(&exe.image, exe.base).ok()?,
            arc: mover::ArcTangent::from_image(&exe.image, exe.base)?,
        })
    }
}

struct Lasting {
    /// The number of the script whose `0x65` waits. No other script runs meanwhile: the
    /// fight's overlay lies in memory where the script interpreter's does (file 36 from
    /// `0x800B6890`, the handlers of file 53 from `0x800B6ED0`).
    script: i32,
    /// The middle of the fight, from where the player stood when it started.
    centre: Centre,
    /// Game frames since the opening view was put in place, and the one the way back is
    /// taken in when no run is shown. Behind the runs the player is sent on his way back
    /// (`back`), and the pause is counted (`paused`) once he is through.
    frame: u32,
    ends: u32,
    pause: u32,
    back: bool,
    /// Game frames since the `0x65`, up to the one the opening view is put in place in
    /// ([`Lasting::LEAD`]): the picture is held and darkened meanwhile.
    lead: u32,
    paused: u32,
    trail: Option<u32>,
    /// The outcome word and the turns, when the rules played the fight.
    played: Option<(u32, usize)>,
    /// The runs of the action runner that the rules decided, in order, the number of
    /// the one that is shown or comes next, and the runner's task while one is shown.
    runs: Vec<Run>,
    at: usize,
    showing: Option<Showing>,
    between: u32,
    done: u32,
    told: Vec<Saying>,
    told_at: usize,
    saying: Option<(Window, Said)>,
    /// The game frames a ready test of the run has said no.
    waited: u32,
    /// The actors of the room that the fight hid and noted, which the way back shows
    /// again, by their place among the actors.
    noted: Vec<usize>,
    /// The actors that stand for the opponents.
    stood: Vec<Stood>,
    eye: arena::Eye,
    species: Option<u32>,
    roamer: Option<Touched>,
    results: Option<Results>,
    bout: Option<Box<Bout>>,
    asks: Option<usize>,
    fighter: usize,
    menu: Option<fight_menu::Menu>,
    command: Option<Choice>,
    /// What the menu's last pass previewed of the HP and of the mana of the fighter the
    /// box of the own side shows (`0x800C9350`, `0x800C9352`), a gain above 0.
    preview: (i16, i16),
    /// How many opponents stand on the field at once in this fight: the menu's camera
    /// is picked by it.
    at_once: usize,
    hud: Option<fight_hud::Hud>,
    seen: Option<fight_hud::Seen>,
    /// 0 outside a fight of a master and his creature, 1 in one (`0x800A2A44`).
    /// Stand-in: the swap is not kept, so the master is always the one in front.
    master: u8,
}

/// The creature that roams whose touch started a fight.
struct Touched {
    /// Its place among the actors of the room.
    index: usize,
    /// Its number in the room and the slot of the room's word in what the game carries
    /// (area, place, slot): what the forgetting goes by.
    number: usize,
    slot: (usize, usize, i16),
}

struct Stood {
    /// Its place among the actors, and the number of the opponent it stands for.
    index: usize,
    opponent: usize,
    /// It has left the field.
    gone: bool,
    /// It is not on the field yet: an opponent behind the count that fight at once
    /// (the word "waits" of the spawner), or the one of a master and his creature
    /// that is not in front: the swap sets it for the pair.
    waits: bool,
    /// Its mover and its hidden word as the room had them.
    mover: mover::Mover,
    hidden: bool,
    /// What its command list worked on when the fight started. The game asks the copy
    /// it made of the actor, which is gone behind the fight.
    runner: Option<command::Runner>,
    /// The animations the fight has read for its moves.
    read: Vec<usize>,
    carried: Option<usize>,
    /// The effect its request has asked for, until it is let go from its hold.
    effect: Option<ActorEffect>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActorEffect {
    /// The actor's place among the actors of the room.
    pub actor: usize,
    pub number: usize,
    pub windows: [[i16; 2]; WINDOWS],
    pub targets: u8,
    /// The half word of a move the request leaves in the effects' loader: a body
    /// attack's (`0x80031748`); every other request leaves it as it stands.
    pub half: Option<i16>,
}

impl Stood {
    /// The opponent is on the field: the runner tests it, asks it and turns to it.
    fn on_field(&self) -> bool {
        !self.gone && !self.waits && self.carried.is_none()
    }

    /// The actor stands on the field for a slot: an opponent's for a slot of the
    /// opponents, a carried creature's figure for the slots 1 to 3.
    fn stands_for(&self, slot: usize) -> bool {
        match slot.checked_sub(OPPONENTS) {
            Some(number) => self.opponent == number && self.on_field(),
            None => !self.gone && slot.checked_sub(1) == self.carried && slot != PLAYER,
        }
    }
}

impl Lasting {
    const SINGLE: u32 = 1;
    /// The script of a fight that no script waits for: no task has this number, so no
    /// script runs while the fight is on screen.
    const NO_SCRIPT: i32 = -1;
    /// The passes of the screen in one colour (`0x8001C0C0`: a rectangle in each of its
    /// first three passes, the task ended in the fourth).
    const FLASH: u8 = 3;
    /// The game frame from which the camera stands at the side of the field: the opening
    /// view's frames, the frame that shows it once more, and the swing.
    const CAMERA_STANDS: u32 =
        arena::OPENING_FRAMES + 1 + (arena::SWING_START / arena::SWING_STEP) as u32;
    const HUB: u32 = arena::OPENING_FRAMES + 1;
    /// The opening view is put in place this many game frames behind the `0x65`: the
    /// screen's fade makes 33 passes from the game frame of the `0x65` on, the
    /// director's step 4 finds it through in the game frame behind them and the
    /// camera takes its sign in the one after. HYPOTHESIS: the reads of the disc
    /// that the steps wait for are through by then.
    const LEAD: u32 = 34;
    /// What a pass of the screen's fade takes from every colour of the held picture.
    const TAKEN: u32 = 8;
    /// The game frames the brightness rises over from the opening view's first game
    /// frame (`0x8001AA04(8)` of the camera task's state 2).
    const RISES_OVER: u16 = 8;
    const BACK: u32 = 7;
    const SEEN: u32 = 2;
    /// The tear-down is the second of the passes of the way back: the pass behind the
    /// one that sets the hold. A flight's 30 passes are counted in front of it.
    const TORN_DOWN: u32 = 2;
    const SILENT: u32 = 1;
    const MUSIC: u32 = 2;
    const TO_THE_OPPONENTS: i32 = 1000;
    /// The build's own guard, not the game's: a ready test of a run that has said no
    /// for this many game frames is taken as a yes, so that a hidden run comes to its
    /// end. The longest move read is the capture's, 257 game frames.
    const WAITS_AT_MOST: u32 = 900;
    const TEACHER: i32 = 1;

    /// What is taken from every colour of the held picture `lead` game frames behind
    /// the `0x65`, none once the opening view is in place. The pass of each game frame
    /// takes [`Lasting::TAKEN`] from the picture of the one before, so the picture is
    /// black from the 32nd pass on.
    fn held(lead: u32) -> Option<u8> {
        (lead < Lasting::LEAD).then(|| (Lasting::TAKEN * (lead + 1)).min(255) as u8)
    }

    /// What is taken from every colour of the fight's last picture `behind` game frames
    /// behind the last pass of the pause: the screen's fade starts in the next pass
    /// and nothing waits for its end.
    fn trails(behind: u32) -> Option<u8> {
        (behind > 0).then(|| (Lasting::TAKEN * behind).min(255) as u8)
    }

    /// Whether nothing behind the field's fight task is passed `behind` game frames
    /// behind the last pass of the pause, on a way back of `back` game frames: the
    /// pass that sets the hold still runs the whole list, and so does the one in which
    /// the view is found read.
    fn stalls(behind: u32, back: u32) -> bool {
        (2..back).contains(&behind)
    }

    fn rests(fought: bool, fights: bool) -> bool {
        fought && fights
    }

    fn back(outcome: Option<u32>) -> u32 {
        match outcome {
            Some(ended::RAN) => Lasting::BACK + 30,
            _ => Lasting::BACK,
        }
    }

    /// Whether the tear-down is the pass `behind` game frames behind the last pass of
    /// the pause.
    fn torn_down(behind: u32, outcome: Option<u32>) -> bool {
        behind + Lasting::BACK == Lasting::back(outcome) + Lasting::TORN_DOWN
    }

    /// Whether the intro has set the battle flag of a boss's music: opponent 0 is of a
    /// boss species.
    fn boss(&self) -> bool {
        self.species.is_some_and(is_boss)
    }

    fn pause(outcome: u32) -> u32 {
        match outcome {
            ended::WON => 60,
            ended::LOST => 1,
            _ => 0,
        }
    }

    fn waits_for_his_action(outcome: Option<u32>) -> bool {
        matches!(outcome, Some(ended::RAN | ended::TAUGHT | ended::REFUSED))
    }

    /// How far the spawner turns the opponent of a number from the way the player faces,
    /// by the number of opponents on the field (the table `0x800AE06C`).
    fn turned(on_field: usize, opponent: usize) -> i32 {
        match (on_field, opponent) {
            (2, 0) => -284,
            (2, 1) => 284,
            (3, 0) => -512,
            (3, 2) => 512,
            _ => 0,
        }
    }
}

/// The runs of the action runner of a fight, for its picture, the director's game
/// frames behind the last one (`Fight::done`), and the messages of a lesson.
type Runs = (Vec<Run>, u32, Vec<Saying>);

/// A fighter's colour for [`Scripts::fighter_tints`]: the slot, the actor that stands for
/// it, and the record of the tint by its number and as a mix.
pub type FighterTint = (
    usize,
    Option<usize>,
    Option<(usize, jc_formats::tint::Tint)>,
);

/// More passes than a press in every second one needs to bring the results screen
/// of four fighters to its leaving.
const RESULTS_PASSES: u32 = 64;

/// A fight of a script as the room view has to show it in a game frame.
pub struct Fought<'a> {
    pub place: &'a FightPlace,
    pub centre: Centre,
    /// Where the camera of the fight stands and looks in this game frame.
    pub shot: Shot,
    /// How far the picture is moved down in this game frame: the shake of a hit.
    pub scroll: i32,
    pub camera_state: i32,
}

struct Rules {
    tables: Tables,
    /// The player's record of a new game. Stand-in: the build does not keep his record
    /// from fight to fight yet, so he comes to every fight unhurt.
    player: Record,
    /// What is chosen for the player's fighter in every turn: with `--fight=defend` he
    /// defends, else he attacks the first opponent on the field.
    defends: bool,
    /// With `--fight=summon` he calls the first creature he carries in as soon as he
    /// may, and it fights for him: a check option for a hidden run of a fight the hero
    /// alone does not win (give him one with `--creatures=`).
    summons: bool,
    /// With `--no-touch` a creature that roams starts no fight: for a hidden run that
    /// has to walk a forest without the fights on its way, which the player of the
    /// build (no menu, no healing, no experience yet) does not all win.
    touches: bool,
    /// The executable's arc tangent, for who is surprised at the start of a fight.
    arc: mover::ArcTangent,
    /// The table of the spoils in the menus' file, when it could be read.
    spoils: Option<Spoils>,
}

impl Rules {
    /// More turns than this and the fight is given up as one that does not end.
    const LONGEST: usize = 1000;
    /// The archive file of the title, which holds the player's record of a new game.
    const TITLE: usize = 23;

    fn read(disc: &mut DiscImage<File>, exe: &Executable) -> Option<Self> {
        let archive = Archive::open(disc).ok()?;
        let title = archive.read(disc, Self::TITLE).ok()?;
        let menus = archive.read(disc, jc_formats::player_menu::MENU_FILE).ok();
        Some(Self {
            tables: Tables::read(&exe.image, exe.base)?,
            player: Record::of_the_player(&title)?,
            defends: std::env::args().any(|arg| arg == "--fight=defend"),
            summons: std::env::args().any(|arg| arg == "--fight=summon"),
            touches: !std::env::args().any(|arg| arg == "--no-touch"),
            arc: mover::ArcTangent::from_image(&exe.image, exe.base)?,
            spoils: menus.and_then(|file| Spoils::from_file(&file)),
        })
    }

    /// Plays a fight to its end and answers the outcome word and the turns it took;
    /// `None` when a record cannot be made or the fight does not end. The player comes
    /// with the record the last fight left him and goes with the one of this fight's
    /// way out, and the cocoons he threw and the items he used are gone. With `lesson`
    /// the fight goes by those steps, and the player chooses what the step in turn
    /// asks. `surprised` is the surprise number of the start ([`surprise`]). Stand-ins:
    /// the generator starts from the game frame (the console's state is not known), the
    /// player brings no creature, and a creature he catches is not kept. `runs` gets
    /// the runs of the action runner of the fight, for its picture, and the director's
    /// game frames behind the last one. `results` gets the task of the results screen
    /// when the fight ends with one (outcome 0 or 2 of a block that asks for it): its
    /// sums and the draws of the spoils, which go on from the fight's own. With `waits`
    /// the fight is played to the first choice of the player's side only and handed
    /// back as it waits there ([`Rules::go_on`]). `seen` gets the fighters as they
    /// stand in front of the first command, for the fight's HUD.
    #[allow(clippy::too_many_arguments)]
    fn play(
        &self,
        block: &Block,
        lesson: Option<Vec<lesson::Step>>,
        carried: &mut Carried,
        (game_frame, surprised, waits): (u32, u32, bool),
        runs: &mut Runs,
        results: &mut Option<Results>,
        seen: &mut Option<fight_hud::Seen>,
    ) -> Option<Went> {
        let bout = self.open(block, lesson, carried, (game_frame, surprised))?;
        let (records, standing) = (bout.fight.records.clone(), bout.fight.opponents());
        *seen = Some(fight_hud::Seen::new(records, standing));
        self.go_on(bout, waits, carried, runs, results)
    }

    /// The start of a fight, in front of its first command: the player comes with the
    /// record the last fight left him and with what he carries.
    fn open(
        &self,
        block: &Block,
        lesson: Option<Vec<lesson::Step>>,
        carried: &Carried,
        (game_frame, surprised): (u32, u32),
    ) -> Option<Box<Bout>> {
        let tables = &self.tables;
        let mut player = (carried.player.clone()).unwrap_or_else(|| self.player.clone());
        player.equipment = carried.worn;
        // `--hero-status=BITS` (hexadecimal): a check option that starts the fight with
        // these status bits on the hero, for a look at an item or a spell that a
        // healthy hero is refused.
        let status = std::env::args().find_map(|arg| {
            let bits = arg.strip_prefix("--hero-status=")?;
            u32::from_str_radix(bits.trim_start_matches("0x"), 16).ok()
        });
        if let Some(bits) = status {
            player.status |= bits;
            info!("the hero starts the fight with the status bits {bits:#x}");
        }
        let cocoons = u32::try_from(carried.keeping.cocoons()).unwrap_or(0);
        let pieces = |item: usize| carried.keeping.count(item as i32, items::Kind::Item);
        let brought: Vec<(usize, u32)> = (0..tables.items.len())
            .map(|item| (item, u32::from(pieces(item))))
            .filter(|(_, pieces)| *pieces != 0)
            .collect();
        // `--creature-status=BITS` (hexadecimal): the same for every carried creature,
        // for a look at the colour a status gives its figure on the field.
        let creature_status = std::env::args().find_map(|arg| {
            let bits = arg.strip_prefix("--creature-status=")?;
            u32::from_str_radix(bits.trim_start_matches("0x"), 16).ok()
        });
        let mut minions: Vec<Record> = carried.creatures.carried().cloned().collect();
        if let Some(bits) = creature_status {
            for minion in &mut minions {
                minion.status |= bits;
            }
            info!("the carried creatures start the fight with the status bits {bits:#x}");
        }
        let party = Party {
            player,
            minions,
            cocoons,
            items: brought.clone(),
            room: carried.caught.free(),
        };
        let mut random = Random { seed: game_frame };
        let mut fight = Fight::new(tables, block, party, surprised, NO_DEVA, &mut random)?;
        if let Some(steps) = lesson {
            fight.teach(steps);
        }
        Some(Box::new(Bout {
            fight,
            random,
            turn: 0,
            game_frame,
            cocoons,
            brought,
        }))
    }

    /// Goes on with a fight to its end, or, with `waits`, to the next choice of the
    /// player's side, which [`Rules::answer`] has to be given before the next call;
    /// without `waits` the player's side takes what the build chooses. `runs` gets the
    /// runs and the messages that have come since it was filled last, and the end
    /// gives `carried` and `results` what [`Rules::close`] says. `None` for a fight
    /// that cannot go on.
    fn go_on(
        &self,
        mut bout: Box<Bout>,
        waits: bool,
        carried: &mut Carried,
        runs: &mut Runs,
        results: &mut Option<Results>,
    ) -> Option<Went> {
        loop {
            match self.advance(&mut bout)? {
                Stops::Over { outcome } => {
                    let turns = bout.turn;
                    self.close(*bout, outcome, carried, runs, results);
                    return Some(Went::Over { outcome, turns });
                }
                Stops::Asks { slot } => {
                    debug_assert_eq!(slot, bout.fight.fighter);
                    if waits {
                        bout.shown(runs);
                        return Some(Went::Asks(bout));
                    }
                    self.answer(&mut bout, None).ok()?
                }
            }
        }
    }

    /// Goes on with a fight until the player's side has to choose or the fight is over;
    /// the opponents choose for themselves on the way. `None` for a fight that does not
    /// end or an opponent that cannot act at all.
    fn advance(&self, bout: &mut Bout) -> Option<Stops> {
        let tables = &self.tables;
        while bout.turn < Self::LONGEST {
            let odd = bout.odd();
            let Bout { fight, random, .. } = &mut *bout;
            match fight.next(tables, random) {
                Step::Over { outcome } => return Some(Stops::Over { outcome }),
                Step::Choose { slot } if slot < OPPONENTS => return Some(Stops::Asks { slot }),
                Step::Choose { .. } => {}
            }
            // An opponent whose routine is not read attacks.
            let attack = Choice::Attack {
                targets: 1 << fight.fighter,
            };
            (fight.opponent_acts(tables, odd, random))
                .or_else(|_| fight.act(tables, attack, odd, random))
                .or_else(|_| fight.act(tables, Choice::Defend, odd, random))
                .ok()?;
            bout.turn += 1;
        }
        None
    }

    /// Hands a fight the command of the player's side that [`Rules::advance`] stopped
    /// for. Without a choice the build chooses: an attack on the first opponent on the
    /// field (in a lesson what the step asks), and a defence when that is refused (a
    /// player without a weapon) or with `--fight=defend`. A choice that is refused
    /// spends no turn.
    fn answer(&self, bout: &mut Bout, choice: Option<Choice>) -> Result<(), Refusal> {
        let tables = &self.tables;
        let odd = bout.odd();
        let Bout { fight, random, .. } = &mut *bout;
        match choice {
            Some(choice) => fight.act(tables, choice, odd, random)?,
            None => {
                let attack = Choice::Attack {
                    targets: fight.opponents().isolate_lowest_one(),
                };
                let asked = fight.taught().and_then(lesson::Step::choice);
                let summon = Choice::Summon { slot: 1 };
                if self.summons && fight.act(tables, summon, odd, random).is_ok() {
                    bout.turn += 1;
                    return Ok(());
                }
                let taken = match self.defends {
                    true => Err(Refusal::NotBuilt),
                    false => fight.act(tables, asked.unwrap_or(attack), odd, random),
                };
                taken.or_else(|_| fight.act(tables, Choice::Defend, odd, random))?;
            }
        }
        bout.turn += 1;
        Ok(())
    }

    /// The end of a fight that is over: the way out, the task of the results screen
    /// when the fight has one, and what the player goes with.
    fn close(
        &self,
        bout: Bout,
        outcome: u32,
        carried: &mut Carried,
        runs: &mut Runs,
        results: &mut Option<Results>,
    ) {
        bout.shown(runs);
        let Bout {
            mut fight,
            mut random,
            cocoons,
            brought,
            ..
        } = bout;
        fight.way_out();
        let screen = fight.results && matches!(outcome, ended::WON | ended::RAN);
        if let (true, Some(spoils), Some(_)) = (screen, &self.spoils, &fight.records[0]) {
            // The fighters of the screen: the player and the creatures he carries, in
            // the order of their slots.
            let own = fight.records[..OPPONENTS].iter();
            let fighters: Vec<Record> = own.map_while(Clone::clone).collect();
            let opened = Results::open(&fight.beaten, &fighters, spoils, &mut random);
            *results = Some(opened);
        }
        carried.player = fight.records[0].take();
        // The carried creatures are the fighters of the slots 1 to 3, in their order.
        for (slot, creature) in carried.creatures.carried_mut().enumerate() {
            if let Some(record) = fight.records.get_mut(slot + 1).and_then(Option::take) {
                *creature = record;
            }
        }
        // What he caught: the kind counts as caught, and the creature waits in the
        // first empty of the twelve records as it stood when it was caught.
        for record in std::mem::take(&mut fight.caught) {
            let (species, level, hp) = (record.species, record.level, record.hp);
            carried.kinds.mark(species);
            match carried.caught.put(record) {
                Some(place) => info!(
                    "the caught creature of species {species}, level {level} with {hp} HP, \
                     waits in record {place}; {} kinds count as caught",
                    carried.kinds.count()
                ),
                None => warn!("the twelve records are taken: the caught creature is lost"),
            }
        }
        let thrown = cocoons.saturating_sub(fight.cocoons);
        carried.keeping.take_cocoons(thrown as i32);
        for (item, pieces) in brought {
            let left = fight.items.iter().find(|entry| entry.0 == item);
            let used = pieces.saturating_sub(left.map_or(0, |entry| entry.1));
            (carried.keeping).take(item as i32, used as i32, items::Kind::Item);
        }
        for event in &fight.events {
            if let Event::Says { message } = event {
                info!("the lesson says the room's text {message}");
            }
        }
    }
}

/// Where [`Rules::go_on`] leaves a fight.
enum Went {
    /// It is over, with this outcome word after this many commands.
    Over { outcome: u32, turns: usize },
    /// The player's side chooses: the fight as it waits for the command.
    Asks(Box<Bout>),
}

impl Went {
    /// The outcome word and the turns of a fight that is over, and the fight that
    /// waits for a command.
    fn apart(went: Option<Self>) -> (Option<(u32, usize)>, Option<Box<Bout>>) {
        match went {
            Some(Went::Over { outcome, turns }) => (Some((outcome, turns)), None),
            Some(Went::Asks(bout)) => (None, Some(bout)),
            None => (None, None),
        }
    }
}

/// A fight of the rules between two commands: what [`Rules::advance`] and
/// [`Rules::answer`] go on with, so that a fight can wait for the player's side.
struct Bout {
    fight: Fight,
    random: Random,
    /// The commands taken so far.
    turn: usize,
    /// The game frame the fight started in, and the cocoons and items the player
    /// brought.
    game_frame: u32,
    cocoons: u32,
    brought: Vec<(usize, u32)>,
}

impl Bout {
    /// The frame bit of the next resolution. Stand-in: the game frame of the fight's
    /// start plus the commands so far, since the build's fight takes no game frames of
    /// its own while it is decided.
    fn odd(&self) -> bool {
        (self.game_frame as usize + self.turn) % 2 == 1
    }

    /// Hands over what the picture does not have yet: the runs and the messages
    /// behind those of `runs`, and the director's game frames behind the last run,
    /// which count once the fight is over.
    fn shown(&self, runs: &mut Runs) {
        let (shown, done, said) = runs;
        shown.extend_from_slice(&self.fight.runs[shown.len()..]);
        said.extend_from_slice(&self.fight.said[said.len()..]);
        *done = self.fight.done;
    }
}

/// Where [`Rules::advance`] stops.
enum Stops {
    /// This slot of the player's side chooses a command.
    Asks {
        slot: usize,
    },
    Over {
        outcome: u32,
    },
}

impl Room {
    /// A command goes on without being run: it is counted for the list a script's end
    /// and the room's leaving print, and the first of its number is named at once, so
    /// that a run which never leaves the room shows it too.
    fn leave_out(&mut self, code: u32) {
        let times = self.left_out.entry(code).or_default();
        if *times == 0 {
            info!(
                "game frame {}: command {code:#04x} goes on without being run, the first \
                 of its number in this room",
                self.game_frame
            );
        }
        *times += 1;
    }

    /// Whether a fight that is on screen waits where the player's side chooses
    /// ([`fight_menu_asked`]). With `--fight-asks` no run of a fight is shown, so nothing
    /// could come to the choice.
    fn waits_for_commands(&self) -> bool {
        self.fight_menu && self.fight_asks.is_none()
    }

    fn group_of(&self, text: u32) -> Option<usize> {
        // The groups named by address stand behind the room's own.
        let own = self.groups.len() - self.named.len();
        match text <= message::LAST_GROUP {
            true => Some(text as usize).filter(|&group| group < own),
            false => self.named.get(&text).copied(),
        }
    }
}

/// A number for a new script task: behind every task's and behind those the scripts of
/// single views keep while they wait for their view.
fn free_number(tasks: &[(Task, Option<u32>)], by_view: &[(i32, u32, u32)]) -> i32 {
    let waiting = by_view.iter().map(|(number, ..)| *number);
    (tasks
        .iter()
        .map(|(task, _)| task.number)
        .chain(waiting)
        .max())
    .unwrap_or(0)
        + 1
}

impl Scripts {
    /// The scripts of a scene package, each at its first command. `file` is the package's
    /// file, `number` the archive file it is, `room` the room's number and `ground` the room's tables 3 and 4. With `confirms` the confirm button
    /// goes down by itself whenever a message window stands, for a run nobody plays.
    pub fn load(
        disc: &mut DiscImage<File>,
        (file, number, room): (&[u8], usize, Option<u16>),
        ground: Option<Ground>,
        confirms: bool,
        (list, carried): (Option<usize>, Carried),
        lines: &mut Vec<String>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let exe = Executable::read(disc)?;
        let mut fights = Rules::read(disc, &exe);
        // The table of the fifteen game frames of a shake (`0x80083AF8`).
        let shake_table = Cameras::from_executable(&exe.image, exe.base)
            .map_or([0; arena::SHAKE_FRAMES], |cameras| cameras.shake);
        let tints = jc_formats::tint::table(&exe.image, exe.base).unwrap_or_default();
        if fights.is_none() {
            warn!("the rules of the fights are not read: a fight of a script goes on unfought");
        }
        // `--creatures=SPECIES[:LEVEL],...`: a check option that gives the hero carried
        // creatures, each as the set-up of a fight makes one of a species at a level.
        // `--creature-hp=N` with it: they have at most N HP left, for a look at a fall;
        // `--creature-mp=N`: at most N mana, for a look at a spell that costs too much.
        // `--creature-strong`: a cheat for hidden runs (asked for by the user,
        // 2026-10-13), not the game's: every stat of them is the largest a byte holds,
        // with 9999 HP and MP, so that a fight nobody can win by the rules yet is won
        // and what lies behind it is seen.
        const STRONG_POINTS: i16 = 9999;
        let strong = std::env::args().any(|arg| arg == "--creature-strong");
        let mut carried = carried;
        if let (Some(rules), Some(_)) = (&mut fights, &carried.corridor) {
            let fonts = jc_formats::text::Fonts::from_executable(&exe.image, exe.base);
            let small = fonts
                .as_ref()
                .and_then(|fonts| fonts.get(jc_formats::text::SMALL_FONT));
            let package = Archive::open(disc)?.read(disc, corridor::FILE)?;
            match small.map(|small| {
                corridor::uniques(&package, |units| jc_formats::name::stored(units, small))
            }) {
                Some(Ok(uniques)) => rules.tables.uniques = uniques,
                Some(Err(err)) => warn!("the records of the species 201 to 210: {err:?}"),
                None => warn!("the small font is not read: no record of the species 201 to 210"),
            }
        }
        // The check options of this block give their creatures once in a run of the
        // app: a room the game comes to behind an exit is set up here again, with
        // what the room before left.
        static GIVEN: AtomicBool = AtomicBool::new(false);
        let gives = !GIVEN.swap(true, Ordering::Relaxed);
        let asked = std::env::args().find_map(|arg| {
            let value = arg.strip_prefix("--creatures=")?.to_owned();
            let asked = crate::player_menu::ShowMenu::creatures(&value);
            if asked.is_none() {
                warn!("--creatures={value} is not read (three at most, the levels 1 to 32)");
            }
            asked
        });
        let hurt = std::env::args()
            .find_map(|arg| arg.strip_prefix("--creature-hp=")?.parse::<i16>().ok());
        let spent = std::env::args()
            .find_map(|arg| arg.strip_prefix("--creature-mp=")?.parse::<i16>().ok());
        if let (Some(asked), Some(rules)) = (asked.filter(|_| gives), &fights) {
            for (species, level) in asked {
                let experience = level.saturating_sub(1).wrapping_mul(LEVEL);
                let made = Record::make(&rules.tables, species, experience, &mut carried.random);
                let put = made.and_then(|mut record| {
                    if strong {
                        record.stats = [u8::MAX; 6];
                        (record.hp, record.max_hp) = (STRONG_POINTS, STRONG_POINTS);
                        (record.mp, record.max_mp) = (STRONG_POINTS, STRONG_POINTS);
                    }
                    if let Some(left) = hurt {
                        record.hp = record.hp.min(left.max(1));
                    }
                    if let Some(left) = spent {
                        record.mp = record.mp.min(left.max(0));
                    }
                    let (hp, mp) = (record.hp, record.mp);
                    let at = (carried.creatures).put(creatures::state::CARRIED, record)?;
                    Some((at, hp, mp))
                });
                match put {
                    Some((at, hp, mp)) => info!(
                        "the hero carries species {species} at level {level} in record {at}: \
                         {hp} HP and {mp} MP"
                    ),
                    None => warn!("species {species} cannot be made or has no free record"),
                }
            }
        }
        // `--merged=SPECIES[:LEVEL]`: a check option, the first creature he carries
        // has taken one of that species in a merge ([`merged_for_a_look`]).
        if gives
            && let Some(rules) = &fights
            && let Some(first) = carried.creatures.nth_mut(creatures::state::CARRIED, 0)
            && let Some(made) = merged_for_a_look(first, &rules.tables, &mut carried.random)
        {
            info!(
                "the first creature he carries is merged: species {}, look {}, level {}",
                made.record.species, made.line.look, made.record.level
            );
            *first = made;
        }
        // `--caught=SPECIES[:LEVEL],...`: creatures that wait in their cocoons, as a
        // capture leaves them, for a look at the screen that makes them his own.
        let waiting = std::env::args()
            .find_map(|arg| crate::player_menu::ShowMenu::caught(arg.strip_prefix("--caught=")?));
        if let (Some(waiting), Some(rules)) = (waiting.filter(|_| gives), &fights) {
            for (species, level) in waiting {
                let experience = level.saturating_sub(1).wrapping_mul(LEVEL);
                let made = Record::make(&rules.tables, species, experience, &mut carried.random);
                match made.and_then(|record| carried.caught.put(record)) {
                    Some(at) => {
                        info!("species {species} at level {level} waits in the caught record {at}")
                    }
                    None => warn!("species {species} cannot be made or has no free record"),
                }
            }
        }
        let images = [(&exe.image[..], exe.base)];
        let mut built_in = Vec::new();
        for at in command::BUILT_IN {
            let list = command::list_in(&images, at);
            built_in.push(list.ok_or("the executable has no built-in command lists")?);
        }
        let built_in: [Vec<command::Command>; 4] =
            (built_in.try_into()).map_err(|_| "four built-in command lists")?;
        let creature_moves: Vec<Vec<command::Command>> =
            (command::move_table(&images, command::CREATURE_MOVES).iter())
                .filter_map(|address| command::list_in(&images, *address))
                .collect();
        let fight_asks = std::env::args().find_map(|arg| {
            let asked = arg.strip_prefix("--fight-asks=")?.to_owned();
            let mut fields = asked.split(':');
            let (kind, parameter) = (fields.next()?, fields.next().unwrap_or("0"));
            Some((kind.parse().ok()?, parameter.parse().ok()?))
        });
        // A third field names the attack of a body attack (kind 6), which the move
        // entry of a fighter's own turn would give: --fight-asks=6:SLOT:ATTACK.
        let fight_attack = std::env::args().find_map(|arg| {
            let asked = arg.strip_prefix("--fight-asks=")?;
            asked.split(':').nth(2)?.parse::<u8>().ok()
        });
        let fonts = Fonts::from_executable(&exe.image, exe.base)
            .ok_or("the executable does not hold the fonts")?;
        let font = (fonts.get(text::TEXT_FONT))
            .ok_or("there is no text font")?
            .clone();
        let package = ScenePackage::parse(file)?;
        // The move table of every actor's record, by its address; empty where the
        // address names a place without a list.
        let with_package = [(package.image(), PACKAGE_BASE), images[0]];
        let own_moves: BTreeMap<u32, Vec<Vec<command::Command>>> = (package.actors().iter())
            .filter(|actor| actor.record.word != 0)
            .map(|actor| {
                let lists = (command::move_table(&with_package, actor.record.word).iter())
                    .filter_map(|address| command::list_in(&with_package, *address))
                    .collect();
                (actor.record.word, lists)
            })
            .collect();
        let roamed = room.is_some_and(|room| {
            let engine = (Archive::open(disc).ok()).and_then(|archive| {
                archive
                    .read(disc, jc_formats::rooms::FIELD_ENGINE_FILE)
                    .ok()
            });
            let tables = engine.and_then(|engine| roaming::Tables::from_field_engine(&engine).ok());
            tables.is_some_and(|tables| tables.row_of(room.into()).is_some())
        });
        // A piece of the corridor of the second round has no row in those tables: its
        // generator answers for it.
        let roamed = roamed || (carried.corridor.is_some() && Corridor::has_room(number));
        let fights_here = roamed
            || (package.script_commands().iter())
                .any(|command| command.code == scene::COMMAND_FIGHT);
        let fight_place = (fights_here.then(|| FightPlace::read(disc, &exe, room))).flatten();
        if fights_here && fight_place.is_none() {
            warn!("the place of the room's fights is not read: a fight of a script is not seen");
        }
        // A room without text has no groups, and its scripts no messages.
        let data = RoomData::parse(package.room_data()).ok();
        let mut groups = (data.as_ref())
            .and_then(|data| {
                let text = data.resources.iter().find(|resource| resource.kind == 1)?;
                message::groups(text.data)
            })
            .unwrap_or_default();
        let mut named = BTreeMap::new();
        for command in package.script_commands() {
            let Some(Scene::Say {
                group: list,
                question,
                ..
            }) = Scene::of(&command)
            else {
                continue;
            };
            if list <= message::LAST_GROUP || named.contains_key(&list) {
                continue;
            }
            if let Some(group) = package.said_at(list, question) {
                named.insert(list, groups.len());
                groups.push(group);
            }
        }
        let paths = (data.iter())
            .flat_map(|data| &data.resources)
            .find(|resource| resource.kind == 4)
            .and_then(|resource| mover::paths(resource.data))
            .unwrap_or_default();
        // The spots that answer a press, each with the region of table 1 it asks.
        let spots: Vec<(Spot, Option<Region>, Option<Marked>)> = (Spot::of(&package).into_iter())
            .map(|spot| {
                let ground =
                    (data.as_ref()).and_then(|data| data.tables[0].get(spot.region).cloned());
                info!(
                    "a spot that answers a press at {:#010x}: the ground of view {} ({}), \
                     names thing {} of the story",
                    spot.record,
                    spot.region,
                    match &ground {
                        Some(ground) => format!("{} triangles", ground.triangles.len()),
                        None => "which the room does not have".to_owned(),
                    },
                    spot.thing
                );
                (spot, ground, None)
            })
            .collect();
        let triggers = data.map_or_else(Vec::new, |data| data.triggers().to_vec());
        // A room that uses one object list of its package runs the scripts of that list.
        let lists = package.object_lists();
        let only = list.and_then(|list| lists.get(list));
        let cues = (only
            .into_iter()
            .chain(lists.iter().filter(|_| only.is_none())))
        .flat_map(|list| &list.objects)
        .find(|object| object.kind == voice::OBJECT_CUES)
        .and_then(|object| voice::cues(&package, object.data))
        .unwrap_or_default();
        let listed: Vec<(i32, u32, u32)> = (package.scripts().iter())
            .filter(|found| only.is_none_or(|list| list.has_script(found.address)))
            .zip(1..)
            .map(|(found, number)| (number, found.address, found.view_mask))
            .collect();
        let mut tasks: Vec<(Task, Option<u32>)> = (listed.iter())
            .filter(|(.., views)| *views == scene::EVERY_VIEW)
            .map(|&(number, address, _)| (Task::new(number, address), None))
            .collect();
        let by_view: Vec<(i32, u32, u32)> = (listed.into_iter())
            .filter(|(.., views)| *views != scene::EVERY_VIEW)
            .collect();
        for (number, address, views) in &by_view {
            lines.push(format!(
                "script {number} at {address:#010x} belongs to the views {views:#x}: it starts \
                 when one of them comes on screen"
            ));
        }
        let at: Vec<String> = (tasks.iter())
            .map(|(task, _)| format!("{:#010x}", task.current))
            .collect();
        // The doors of the list, or of every list when the room names none; a door
        // whose story flag is set starts open.
        let mut doors: Vec<(u32, Door, Swing)> = Vec::new();
        let mut door_scripts = None;
        for list in lists
            .iter()
            .filter(|list| only.is_none_or(|only| only == *list))
        {
            for object in list
                .objects
                .iter()
                .filter(|object| object.kind == door::KIND)
            {
                let read = package.bytes_at(object.data, 80).and_then(Door::parse);
                match read {
                    Some(_) if doors.iter().any(|(at, ..)| *at == object.data) => {}
                    Some(door) => {
                        let open =
                            u32::try_from(door.flag).is_ok_and(|flag| carried.keeping.flag(flag));
                        lines.push(format!(
                            "a door at {:#010x}: {} leaves, opens in {} game frames, story \
                             flag {}, {}",
                            object.data,
                            door.leaves.len(),
                            door.frames,
                            door.flag,
                            if open { "open" } else { "closed" }
                        ));
                        let swing = Swing::new(&door, open);
                        doors.push((object.data, door, swing));
                        door_scripts =
                            door_scripts.or_else(|| door::Scripts::of(&package, object.function));
                    }
                    None => lines.push(format!("no door record at {:#010x}", object.data)),
                }
            }
        }
        // The object that plays a movie, and the objects that do nothing.
        let listed = || {
            (lists.iter())
                .filter(|list| only.is_none_or(|only| only == *list))
                .flat_map(|list| &list.objects)
        };
        let shower = listed()
            .find(|object| object.kind == movie_object::KIND)
            .map(|object| {
                lines.push(format!(
                    "an object that plays a movie of the table at {:#010x}: entry 0 is {:?}",
                    object.data,
                    movie_object::Entry::read(&package, object.data, 0)
                ));
                (object.data, Shower::new())
            });
        let idle: Vec<u32> = listed()
            .filter(|object| package.does_nothing(object.function))
            .map(|object| object.data)
            .collect();
        let scatter = Scatter::of(number, &package).map(|scatter| {
            lines.push(format!(
                "an object at {:#010x} that plays sprite {} at the places of a list: {} and \
                 {} places",
                scatter.data,
                scatter::SPRITE,
                scatter.lists[0].len(),
                scatter.lists[1].len()
            ));
            // The start routine runs the routine once: no list yet, nothing drawn.
            Scattered {
                places: scatter,
                run: scatter::Run::new(),
                drawn: None,
            }
        });
        let gates = Gates::of(number, &package);
        if let Some(gates) = &gates {
            lines.push(format!(
                "{} gates; their scripts are {:#010x} with the gate's thing and {:#010x} \
                 without",
                gates.gates.len(),
                gates.with_thing,
                gates.without
            ));
        }
        let crossings = (gates.iter().flat_map(|gates| &gates.gates))
            .map(|_| Crossed::default())
            .collect();
        // A player who comes from the place map with the number of a forest: the one
        // object with that number starts the third script of the gates and stays.
        let package_file = number;
        let arriving = gates.as_ref().and_then(|gates| {
            let forest = carried.variable(FOREST_VARIABLE);
            let ((at, arrival), script) = (gates.arrival(forest)?, gates.arrives?);
            let number = free_number(&tasks, &by_view);
            lines.push(format!(
                "story variable {FOREST_VARIABLE} is {forest}: the object at {at:#010x} starts \
                 script {number} at {script:#010x} with the show list {:#010x}",
                arrival.show
            ));
            tasks.push((Task::new(number, script), None));
            let light = Halo::of(package_file, &package);
            if let Some(light) = light {
                lines.push(format!(
                    "its state 2 starts the light of a lost fight: the show list {:#010x} and \
                     a task of type 0x0D from {:#010x}",
                    light.list, light.start
                ));
            }
            Some(Arrives {
                at,
                arrival,
                arriving: gate::Arriving::new(),
                script: number,
                light,
            })
        });
        let finds = Finds::of(&package).map(|finds| {
            let find = &finds.find;
            let flag = u32::try_from(find.flag).is_ok_and(|flag| carried.keeping.flag(flag));
            let search = Search::new(find, flag);
            lines.push(format!(
                "a thing to be searched at {:#010x}: trigger region {}, place {:?}, story \
                 flag {} ({}), needs item {}, gives {} things; its script is {:#010x}",
                finds.at,
                find.region,
                find.place,
                find.flag,
                if flag { "set" } else { "not set" },
                find.needs,
                find.given.iter().flatten().count(),
                finds.script
            ));
            let searched = Searched {
                search,
                script: None,
            };
            (finds, searched)
        });
        // The switches and the lists of barriers of the list the room uses.
        let in_list = |kind: u32, record: u32| {
            (lists.iter())
                .filter(|list| only.is_none_or(|only| only == *list))
                .flat_map(|list| &list.objects)
                .any(|object| object.kind == kind && object.data == record)
        };
        let switches: Vec<(Switch, Pressed)> = (Switch::of(&package).into_iter())
            .filter(|found| in_list(switch::KIND, found.record))
            .map(|found| {
                let set = u32::try_from(found.flag).is_ok_and(|flag| carried.keeping.flag(flag));
                (found, Pressed::new(set))
            })
            .collect();
        for (found, pressed) in &switches {
            lines.push(format!(
                "a switch at {:#010x}: trigger region {}, place {:?}, story flag {} ({}), \
                 way {}",
                found.record,
                found.region,
                found.place,
                found.flag,
                if pressed.on { "set" } else { "not set" },
                found.way
            ));
        }
        let barriers: Vec<Barriers> = (Barriers::of(&package).into_iter())
            .filter(|found| in_list(switch::BARRIERS_KIND, found.record))
            .collect();
        for found in &barriers {
            let list: Vec<String> = (found.barriers.iter())
                .map(|barrier| {
                    format!(
                        "region {} of table {} with story flag {}",
                        barrier.region,
                        if barrier.second { 4 } else { 2 },
                        barrier.flag
                    )
                })
                .collect();
            lines.push(format!(
                "barriers at {:#010x}: {}",
                found.record,
                list.join(", ")
            ));
        }
        let followers: Vec<(Follower, Option<Held>)> = (Follower::of(&package).into_iter())
            .filter(|found| in_list(switch::FOLLOWER_KIND, found.record))
            .map(|found| (found, None))
            .collect();
        for (found, _) in &followers {
            lines.push(format!(
                "a picture that follows story flag {} at {:#010x}: the group of the \
                 background named {}, frames {:?} without the flag and {:?} with it, \
                 views {:#x}",
                found.flag,
                found.record,
                found.number,
                found.states[0],
                found.states[1],
                found.view_mask
            ));
        }
        // The lock of a door, when the list the room uses has one.
        let lock = Lock::of(&package)
            .filter(|found| in_list(lock::KIND, found.record))
            .map(|found| {
                let done = u32::try_from(found.flag).is_ok_and(|flag| carried.keeping.flag(flag));
                lines.push(format!(
                    "a lock at {:#010x} of the door at {:#010x}: {} sides (trigger regions \
                     {:?}), thing {} of the story, story flag {} ({}); its scripts are \
                     {:#010x} with the thing and {:#010x} without; the door's scripts: \
                     {door_scripts:x?}",
                    found.record,
                    found.door,
                    found.sides.len(),
                    found
                        .sides
                        .iter()
                        .map(|side| side.region)
                        .collect::<Vec<_>>(),
                    found.thing,
                    found.flag,
                    if done { "set" } else { "not set" },
                    found.with_thing,
                    found.without
                ));
                Locked {
                    lock: found,
                    locking: Locking::new(done),
                    script: None,
                    lines: 0,
                }
            });
        // The passage over a gap, when the list the room uses has its banks.
        let passages = Passages::of(&package)
            .filter(|found| (found.passages.iter()).all(|(at, _)| in_list(passage::KIND, *at)));
        if let Some(found) = &passages {
            for (at, bank) in &found.passages {
                let set = u32::try_from(bank.flag).is_ok_and(|flag| carried.keeping.flag(flag));
                lines.push(format!(
                    "a bank of a passage at {at:#010x}: trigger region {}, the point to face \
                     {:?}, story flag {} ({}), the player is put on {:?} with angle {}; its \
                     script is {:#010x}",
                    bank.region,
                    bank.point,
                    bank.flag,
                    if set { "set" } else { "not set" },
                    bank.place,
                    bank.angle,
                    found.script
                ));
            }
        }
        let passings = (passages.iter().flat_map(|found| &found.passages))
            .map(|_| Passaged::default())
            .collect();
        // The way into area 5, when the list the room uses has its object. The start
        // routine starts a script for a player who comes back and clears the variable.
        let rides = Rides::of(&package)
            .filter(|found| in_list(ride::KIND, found.at))
            .map(|found| {
                let back = carried.variable(ride::BACK_VARIABLE);
                let lost = carried.keeping.flag(ride::LOST_FLAG);
                lines.push(format!(
                    "the way into area 5 at {:#010x}: trigger region {}, the game goes to \
                     room {} and on to room {}, story flag {} is set with it; a player who \
                     comes back is put on {:?} with angle {}; its scripts are {:#010x}, and \
                     {:#010x} and {:#010x} for one who comes back",
                    found.at,
                    found.ride.region,
                    found.ride.room,
                    found.ride.goes_to,
                    found.ride.flag,
                    found.ride.place,
                    found.ride.angle,
                    found.leaves,
                    found.back,
                    found.lost
                ));
                let script = found.arrival(back, lost).map(|script| {
                    let number = free_number(&tasks, &by_view);
                    lines.push(format!(
                        "story variable {} is {back} and story flag {} is {}: the object \
                         starts script {number} at {script:#010x} and clears the variable",
                        ride::BACK_VARIABLE,
                        ride::LOST_FLAG,
                        if lost { "set" } else { "not set" }
                    ));
                    tasks.push((Task::new(number, script), None));
                    carried.set_variable(ride::BACK_VARIABLE, 0);
                    number
                });
                (found, Riding::new(), script, script.is_some())
            });
        // The warp of a forest, when the list the room uses has it, with the area and the
        // place of the room from the table of the rooms.
        let warps = Warps::of(&package)
            .filter(|found| in_list(warp::KIND, found.at))
            .and_then(|found| {
                let engine = (Archive::open(disc).ok()).and_then(|archive| {
                    archive
                        .read(disc, jc_formats::rooms::FIELD_ENGINE_FILE)
                        .ok()
                })?;
                let table = jc_formats::rooms::RoomTable::from_field_engine(&engine).ok()?;
                let (at, _) = (table.iter())
                    .find(|(_, record)| usize::from(record.package_file) == number)?;
                let (area, place) = (i16::try_from(at.area).ok()?, i16::try_from(at.place).ok()?);
                lines.push(format!(
                    "the warp at {:#010x}: trigger region {}, needs thing {} of the story, \
                     leads to slot {} of area {area} place {place} with the player on {:?} \
                     with angle {}; its scripts are {:#010x} and {:#010x} for one who \
                     comes by it, its lists are drawn at {:?}",
                    found.at,
                    found.warp.region,
                    found.warp.thing,
                    found.warp.slot,
                    found.warp.arrival,
                    found.warp.angle,
                    found.departure,
                    found.arrival,
                    found.warp.shows_at
                ));
                Some((found, Warping::new(), None, (area, place)))
            });
        let generator = match &carried.corridor {
            Some(corridor) if Corridor::has_room(number) => {
                lines.push(format!(
                    "the corridor: floor {}, look {}, the species {:?}; the player is in \
                     node {} of {}, a piece of kind {}, and its links are {:?}",
                    corridor.floor,
                    corridor.look,
                    corridor.species,
                    corridor.at,
                    corridor.nodes.len(),
                    corridor.kind(),
                    corridor.nodes[corridor.at].links
                ));
                Some(Generator::Steps)
            }
            Some(_) => {
                lines.push(
                    "the corridor's generator lives in a room that is not the corridor's: \
                     it does nothing here"
                        .to_string(),
                );
                None
            }
            None => (number == corridor::FILE)
                .then(|| {
                    (lists.iter())
                        .filter(|list| only.is_none_or(|only| only == *list))
                        .flat_map(|list| &list.objects)
                        .find(|object| {
                            (object.kind, object.extra, object.function)
                                == (corridor::OBJECT, corridor::EXTRA, corridor::START)
                        })
                })
                .flatten()
                .map(|object| {
                    lines.push(format!(
                        "the generator of the corridor at {:#010x}: its start routine makes \
                         floor 1 and sends the game to the floor's first piece",
                        object.address
                    ));
                    Generator::Starts
                }),
        };
        // The object of a ride's room that plays a sound at a pace.
        let ride_sounds = Sounds::of(&package)
            .filter(|found| in_list(ride::KIND, found.at))
            .map(|found| {
                lines.push(format!(
                    "an object at {:#010x} that plays sound {} every {} game frames, the \
                     first {} game frames behind its start",
                    found.at, found.sound, found.every, found.wait
                ));
                (found, Sounding::new(&found))
            });
        // The camera's object of a ride's room. It has no record: its routine tells it.
        let ride_camera = ride::Follows::of(&package).filter(|found| {
            (lists.iter())
                .filter(|list| only.is_none_or(|only| only == *list))
                .flat_map(|list| &list.objects)
                .any(|object| object.kind == ride::KIND && object.function == found.start)
        });
        if let Some(found) = &ride_camera {
            lines.push(format!(
                "the camera's object of the ride, start {:#010x} and step {:#010x}: the point \
                 the camera looks at goes with the player, the projection distance with its \
                 distance",
                found.start, found.step
            ));
        }
        // The object of the sliding bars, when the list the room uses has it. Its start
        // routine runs the step once: open bars, nothing told.
        let slides = Bars::of(&package)
            .filter(|(_, at, _)| in_list(bars::KIND, *at))
            .map(|(_, at, found)| {
                lines.push(format!(
                    "the sliding bars at {at:#010x}: shut they leave {:?} of {:?} free, a \
                     slide takes {} game frames",
                    found.inside, found.outside, found.frames
                ));
                let mut task = Sliding::new();
                let drawn = task.pass(&found);
                (at, found, task, drawn)
            });
        // The object that plays two models, when the list the room uses has it. Its
        // start routine runs the step once.
        let plays = Played::of(&package)
            .filter(|(_, at, _)| in_list(played::KIND, *at))
            .map(|(_, at, found)| {
                lines.push(format!(
                    "the object that plays two models at {at:#010x}: {:?}",
                    found.parts
                ));
                let mut task = Playing::new();
                let (drawn, _) = task.pass(false);
                (at, found, task, drawn)
            });
        // The object of the memory card, when the list the room uses has one. Its two
        // mesh objects are in a block of their own.
        let card = (Card::of(&package).into_iter()).find(|card| {
            (lists.iter())
                .filter(|list| only.is_none_or(|only| only == *list))
                .flat_map(|list| &list.objects)
                .any(|object| object.kind == card::KIND && object.data == card.record)
        });
        let card = match card {
            Some(card) => {
                let block = Archive::open(disc)?.read(disc, card::BLOCK)?;
                let block = MeshBlock::parse(&block).ok();
                lines.push(format!(
                    "the object of the memory card at {:#010x}: place {:?}, its text is at \
                     {:#010x}; mesh block of archive file {}: {}",
                    card.record,
                    card.place,
                    card.text,
                    card::BLOCK,
                    match &block {
                        Some(block) => format!("{} objects", block.objects.len()),
                        None => "not read".to_string(),
                    }
                ));
                // A text that ends with the package is as long as the package has it.
                let mut text = [0; 24];
                let length = |length: &usize| package.bytes_at(card.text, *length).is_some();
                let found = (0..=text.len()).rev().find(length).unwrap_or(0);
                if let Some(bytes) = package.bytes_at(card.text, found) {
                    text[..found].copy_from_slice(bytes);
                }
                Some(Carded {
                    card,
                    floating: Floating::new(),
                    drawn: None,
                    block,
                    text,
                    asked: false,
                })
            }
            None => None,
        };
        let exits = (lists.iter())
            .filter(|list| only.is_none_or(|only| only == *list))
            .flat_map(|list| &list.objects)
            .filter_map(|object| Some((package.exit(object)?.record, -1)))
            .collect();
        // A trigger of a list the room does not use is not made.
        let listed = |record: u32| {
            (lists.iter())
                .filter(|list| only.is_none_or(|only| only == *list))
                .flat_map(|list| &list.objects)
                .any(|object| object.kind == leave::KIND && object.data == record)
        };
        let map_triggers: Vec<(MapTrigger, Waiting, Option<i32>)> = (MapTrigger::of(&package)
            .into_iter())
        .filter(|trigger| listed(trigger.record))
        .map(|trigger| (trigger, Waiting::new(), None))
        .collect();
        for (trigger, ..) in &map_triggers {
            lines.push(format!(
                "a trigger of the place map at {:#010x}: trigger region {}, its script is \
                 {:#010x}",
                trigger.record, trigger.region, trigger.script
            ));
        }
        lines.push(format!(
            "{} scripts of the room run: {}",
            tasks.len(),
            at.join(", ")
        ));
        Ok(Self {
            file: file.to_vec(),
            number,
            tasks,
            room: Room {
                built_in,
                orders: Orders::default(),
                selector: -1,
                carried,
                setting: window::SETTING_AT_START,
                drawn: 0,
                noted: [None; THINGS],
                ground: ground.map(Arc::new),
                fade: Fade::default(),
                background: true,
                before: false,
                area: DrawArea::WHOLE,
                followed: Followed::Thing(0),
                glide: Glide::default(),
                zoom: Zoom::default(),
                depth: None,
                camera: None,
                read_ahead: None,
                view: None,
                map: false,
                screen: None,
                screen_sleeps: false,
                results: None,
                awaits_results: None,
                music: Vec::new(),
                cues,
                spoken: None,
                spoke: false,
                spoken_page: None,
                voice_silent: true,
                voice_none_asked: true,
                voice_ahead: None,
                voice_seen: 0,
                leaves: None,
                left: false,
                leaving: None,
                chapter: None,
                slot: None,
                generator,
                corridor_asks: None,
                again: false,
                confirm: false,
                keys: 0,
                font,
                groups,
                named,
                pad: Pad::default(),
                confirms,
                answers: Vec::new(),
                asked: 0,
                spawner: package.show_routines().map(|routines| routines.spawner),
                arguments: BTreeMap::new(),
                records: BTreeMap::new(),
                shows: Vec::new(),
                movie_routine: package.movie_routine(),
                movie: None,
                busy: false,
                behind: None,
                seen_at: 0,
                darkened: None,
                shake: None,
                shake_table,
                tints,
                shaken: 0,
                flipped: BTreeMap::new(),
                rested: false,
                roamers_held: false,
                roamers_let_go: false,
                flash: 0,
                clear_routine: package.clear_routine(),
                read_routines: package.read_routines(),
                kept_set_routines: package.kept_set_routines(),
                kept_set: None,
                fade_routines: package.fade_routines(),
                fights,
                outcome: ended::WON,
                lesson: None,
                fight_place,
                fight: None,
                creature_moves,
                own_moves,
                fight_asks,
                fight_attack,
                effect_idle: true,
                effects_started: Vec::new(),
                fight_menu: fight_menu_asked(std::env::args()),
                fight_presses: std::env::args()
                    .find_map(|arg| fight_presses(arg.strip_prefix("--fight-press=")?))
                    .unwrap_or_default(),
                menu_kept: 0,
                paths,
                equip_routines: package.equip_routines(),
                cleared: false,
                sky: None,
                beat: None,
                toll: None,
                spots,
                ending: None,
                end_card: None,
                doors,
                door_scripts,
                door_owners: BTreeMap::new(),
                lock,
                forest_place: None,
                shower,
                idle,
                music_back: false,
                scatter,
                halo: None,
                sparks: Vec::new(),
                gates,
                crossings,
                owners: BTreeMap::new(),
                passages,
                passings,
                passage_owners: BTreeMap::new(),
                rides,
                warps,
                ride_sounds,
                ride_camera,
                slides,
                plays,
                arriving,
                finds,
                card,
                switches,
                barriers,
                followers,
                pictures_told: BTreeMap::new(),
                exits,
                map_triggers,
                said: BTreeMap::new(),
                noticed: BTreeMap::new(),
                triggers,
                can_talk: BTreeSet::new(),
                talked: BTreeSet::new(),
                game_frame: 0,
                bits: 0,
                by_view,
                made_for: None,
                left_out: BTreeMap::new(),
            },
        })
    }

    /// One game frame of every task that runs, with the actors and the player as the game
    /// frame before left them. `camera` is the view's, by which a message picks the side
    /// of its picture, with the projection distance of the view's header, and `view` its
    /// number.
    pub fn pass(
        &mut self,
        actors: &mut [Standing],
        mut player: Option<&mut Walker>,
        confirm: bool,
        (camera, distance): (&ViewCamera, i16),
        view: usize,
    ) {
        let Ok(package) = ScenePackage::parse(&self.file) else {
            return;
        };
        // A script has sent the game to another room: this one is over.
        if self.room.leaves.is_some() || self.room.again || self.room.left {
            return;
        }
        if self.room.chapter.is_some() || self.room.slot.is_some() {
            return;
        }
        if self.room.corridor_asks.is_some() {
            return;
        }
        self.room.confirm = confirm;
        if let Some(behind) = &mut self.room.behind {
            *behind += 1;
            if *behind >= self.room.seen_at {
                self.room.behind = None;
                // The fight task has ended a white that stays in the pass that set
                // the hold (`0x8001AAF4` in `0x800BA7F4`). The build draws the held
                // picture anew in every game frame, so its white lasts as long as
                // the hold.
                self.room.fade.white_ends();
                info!(
                    "game frame {}: the hold of the screen is over, the room is seen again",
                    self.room.game_frame
                );
            }
        }
        // The screen's fade a script asked for takes from the held picture once more.
        if let Some(passes) = &mut self.room.darkened {
            *passes += 1;
        }
        // The white rectangles of the game frame before are gone with its picture.
        self.room.fade.white = 0;
        self.room.flash = self.room.flash.saturating_sub(1);
        let game_frame = self.room.game_frame;
        // The pad of the game frame. A button must come up before it counts again, so
        // the confirm that goes down by itself does so every other game frame.
        let stands = (self.tasks.iter()).any(|(task, _)| {
            task.speech.window.as_ref().is_some_and(Window::stands)
                || task.speech.notice.is_some_and(|notice| notice.stands())
        });
        let taught = (self.room.fight.as_ref()).and_then(|fight| fight.saying.as_ref());
        let stands = stands || taught.is_some_and(|(window, _)| window.stands());
        // A fight that waits for a command takes the confirm for one.
        let asks = (self.room.fight.as_ref()).is_some_and(|fight| fight.asks.is_some());
        let stands = stands || asks;
        let by_itself = self.room.confirms && stands && self.room.pad.held == 0;
        // A question of a run nobody plays: the cursor goes down, every other game
        // frame, to the answer the run names for it, and the confirm waits for that.
        let pointed = (self.tasks.iter())
            .find_map(|(task, _)| task.speech.window.as_ref().and_then(Window::pointed));
        let wanted = (self.room.asked.checked_sub(1))
            .and_then(|question| self.room.answers.get(question).copied());
        // While the answers' box opens no button goes down, or the confirm would pick
        // the first answer in the game frame the box stands.
        let opening = (self.tasks.iter()).any(|(task, _)| {
            (task.speech.window.as_ref())
                .is_some_and(|window| window.answers == window::AnswersBox::Opening)
        });
        let held = match (pointed, wanted) {
            (None, Some(_)) if opening => 0,
            (Some(pointed), Some(wanted)) if pointed != wanted => match self.room.pad.held {
                0 => pad::DOWN,
                _ => 0,
            },
            _ if confirm || by_itself => pad::CROSS | self.room.keys,
            _ => self.room.keys,
        };
        let pressed = (self.room.fight_presses.iter())
            .filter(|(from, to, _)| (*from..=*to).contains(&game_frame))
            .fold(0, |held, (_, _, button)| held | button);
        self.room
            .pad
            .read(held | pressed, Repeat::of(2), game_frame);
        if self.room.made_for != Some(view) {
            let before = self.room.made_for.replace(view).map(|before| before as u32);
            for &(number, address, views) in &self.room.by_view {
                let at = (self.tasks.iter()).position(|(task, _)| task.number == number);
                if scene::view_ends(views, view as u32) {
                    if let Some((_, over @ None)) = at.map(|at| &mut self.tasks[at]) {
                        *over = Some(game_frame);
                        info!(
                            "game frame {game_frame}: view {view} is on screen and ends \
                             script {number}, which is of the views {views:#x}"
                        );
                    }
                } else if scene::view_makes(views, view as u32, before) {
                    info!(
                        "game frame {game_frame}: view {view} is on screen and starts script \
                         {number} at {address:#010x}"
                    );
                    let made = (Task::new(number, address), None);
                    match at {
                        Some(at) => self.tasks[at] = made,
                        None => self.tasks.push(made),
                    }
                }
            }
        }
        // A mode that is asked for is the field's behind the list of that pass: the game
        // frame of a `0x65` still runs the list of mode 1, and the one in which the
        // script goes on runs it whole again.
        let fought = self.room.fight.is_some();
        // A fight that a touch has started is the room's own to run: no script waits
        // for it.
        if (self.room.fight.as_ref()).is_some_and(|fight| fight.roamer.is_some()) {
            let mut engine = Running {
                room: &mut self.room,
                actors: &mut *actors,
                player: player.as_deref_mut(),
                package: &package,
                file: self.number,
                camera,
                distance,
                view,
            };
            engine.pass_touch();
        }
        for (task, over) in &mut self.tasks {
            if over.is_some() {
                continue;
            }
            // While a fight is on screen only the script that waits for it runs.
            let fights = self.room.fight.as_ref().map(|fight| fight.script);
            if fights.is_some_and(|script| script != task.number) {
                continue;
            }
            let mut engine = Running {
                room: &mut self.room,
                actors: &mut *actors,
                player: player.as_deref_mut(),
                package: &package,
                file: self.number,
                camera,
                distance,
                view,
            };
            match task.pass(&package, &mut engine) {
                Ok(()) if task.ended() => {
                    *over = Some(game_frame);
                    let left_out: Vec<String> = (self.room.left_out.iter())
                        .map(|(code, times)| format!("{code:#04x} {times} times"))
                        .collect();
                    info!(
                        "script {}: ended in game frame {game_frame}; commands that went on \
                         without being run: {}",
                        task.number,
                        left_out.join(", ")
                    );
                }
                Ok(()) => {}
                Err(fault) => {
                    *over = Some(game_frame);
                    warn!(
                        "script {}: stopped in game frame {game_frame}: {fault}",
                        task.number
                    );
                }
            }
            if self.room.busy {
                break;
            }
        }
        let rested = Lasting::rests(fought, self.room.fight.is_some());
        if rested != self.room.rested {
            info!(
                "game frame {game_frame}: the objects of the room and what it shows {}",
                match rested {
                    true => "are not passed from here on: the field runs the list of a fight",
                    false => "are passed again",
                }
            );
        }
        self.room.rested = rested;
        if std::mem::take(&mut self.room.busy) {
            self.room.game_frame += 1;
            return;
        }
        // The list of a fight has no pool of object tasks, and on the way back the busy
        // word ends the field's list in front of it: no object of the room is passed.
        let rested = self.room.rested;
        // The doors: a told state is taken, an opening door counts the game frame.
        for (at, door, swing) in (self.room.doors.iter_mut()).filter(|_| !rested) {
            // Step 1: the player at the door who faces its point is marked, unless the
            // script the door started still runs.
            let at_door = player.as_deref().is_some_and(|walker| {
                let (place, angle) = (walker.player.position, walker.player.angle);
                (usize::try_from(door.region).ok())
                    .and_then(|region| self.room.triggers.get(region))
                    .is_some_and(|region| region.triangle_at(place).is_some())
                    && player::talk_faced(&walker.body.arc, place, angle, door.point)
            });
            let script_runs = (self.room.door_owners.iter()).any(|(number, record)| {
                record == at
                    && (self.tasks.iter())
                        .any(|(task, over)| task.number == *number && over.is_none())
            });
            swing.look(at_door, script_runs);
            let passed = swing.pass(door);
            // Told state 0 the door starts a script of the package with itself as the
            // owner; while the walking word is not 1 the state stays told.
            if passed.asks && self.room.leaving.is_some() {
                swing.tell(door::ASKED);
            } else if passed.asks {
                let keeping = &self.room.carried.keeping;
                match &self.room.door_scripts {
                    Some(scripts) => {
                        let script = scripts.pick(door, |thing| keeping.has_story_thing(thing));
                        let number = free_number(&self.tasks, &self.room.by_view);
                        info!(
                            "game frame {game_frame}: the door at {at:#010x} starts script \
                             {number} at {script:#010x}"
                        );
                        self.tasks.push((Task::new(number, script), None));
                        self.room.door_owners.insert(number, *at);
                    }
                    None => warn!(
                        "game frame {game_frame}: the door at {at:#010x} is told state 0 and \
                         its scripts were not found"
                    ),
                }
            }
            if passed.opens {
                info!(
                    "game frame {game_frame}: the door at {at:#010x} opens, sound {}",
                    door.sound
                );
                // The door plays its sound once, as a script's call 13 does.
                let sound = u32::try_from(door.sound).map(Heard::Sound);
                (self.room.music).extend(sound.map(|sound| (game_frame, sound)));
                if let Ok(flag) = u32::try_from(door.flag) {
                    self.room.carried.keeping.set_flag(flag);
                }
            }
            if passed.is_open && swing.counter == i32::from(door.frames) && !swing.told_open {
                swing.told_open = true;
                info!(
                    "game frame {game_frame}: the door at {at:#010x} is open; the ground's \
                     switch takes the barriers {:?} of the tables 2 and 4 away",
                    door.ground
                );
            }
            // The ground's switch `0x800A64F4`, in every pass of an open door: the
            // barrier of table 2 and the one of table 4 its record names are taken away.
            if passed.is_open {
                if let (Ok(region), Some(walker)) = (usize::try_from(door.ground[0]), &mut player) {
                    walker.set_barrier(region, 0);
                }
                if let (Ok(region), Some(ground)) =
                    (usize::try_from(door.ground[1]), &mut self.room.ground)
                    && ground.flags.get(region).is_some_and(|flag| *flag != 0)
                {
                    Arc::make_mut(ground).flags[region] = 0;
                }
            }
        }
        if let Some(Scattered { run, drawn, .. }) = (self.room.scatter.as_mut()).filter(|_| !rested)
        {
            *drawn = run.pass();
        }
        // The object that plays a movie: what its pass asks is done at once, and the
        // movie is handed to the room view as a script's is.
        if std::mem::take(&mut self.room.music_back) {
            let asked = Heard::Fight(sound::FightAsks::Noted);
            self.room.music.push((game_frame, asked));
        }
        if let Some((table, showing)) = (self.room.shower.as_mut()).filter(|_| !rested) {
            let entry = |index| movie_object::Entry::read(&package, *table, index);
            for asked in showing.pass(entry, self.room.fade.at_rest()) {
                info!("game frame {game_frame}: the object that plays a movie asks: {asked:?}");
                match asked {
                    movie_object::Asked::FadeOut(frames) => self.room.fade.fade_out(frames),
                    movie_object::Asked::FadeIn(frames) => self.room.fade.fade_in(frames),
                    movie_object::Asked::Music(music) => {
                        let asked = Heard::Fight(sound::FightAsks::Music(music as usize));
                        self.room.music.push((game_frame, asked));
                    }
                    movie_object::Asked::MusicBack => self.room.music_back = true,
                    movie_object::Asked::BackgroundOut => self.room.background = false,
                    movie_object::Asked::BackgroundIn => self.room.background = true,
                    movie_object::Asked::Movie(movie) => self.room.movie = Some(movie),
                }
            }
        }
        if !rested {
            Self::pass_gates(
                &mut self.room,
                &mut self.tasks,
                &package,
                player.as_deref_mut(),
            );
            Self::pass_passages(
                &mut self.room,
                &mut self.tasks,
                &package,
                player.as_deref_mut(),
            );
            Self::pass_rides(&mut self.room, &mut self.tasks, player.as_deref_mut());
            Self::pass_warps(&mut self.room, &mut self.tasks, player.as_deref_mut());
            Self::pass_corridor(&mut self.room, player.as_deref());
            Self::pass_find(&mut self.room, &mut self.tasks, player.as_deref());
            Self::pass_lock(&mut self.room, &mut self.tasks, player.as_deref());
            Self::pass_card(&mut self.room, player.as_deref());
            Self::pass_switches(&mut self.room, player.as_deref_mut(), view);
            // The spots that answer a press: the triangle lookup on the ground of their
            // view, with the player's place.
            let place = player.as_deref().map(|walker| walker.player.position);
            for (_, ground, task) in &mut self.room.spots {
                let stands = (place.zip(ground.as_ref()))
                    .is_some_and(|(place, ground)| ground.triangle_at(place).is_some());
                if let Some(task) = task {
                    task.pass(stands);
                }
            }
        }
        // A barrier of table 4 that a switch has turned is turned in the room's ground:
        // the actors walk over that one, not over the ground of the room's start.
        if let Some(ground) = &self.room.ground {
            for actor in actors.iter_mut() {
                hand_on(&mut actor.ground, ground);
            }
        }
        if !rested {
            Self::pass_map_triggers(&mut self.room, &mut self.tasks, player.as_deref(), view);
        }
        if let Some((_, _, sky, drawn)) = (self.room.sky.as_mut()).filter(|_| !rested) {
            let count = sky.count;
            *drawn = sky.pass(view);
            if sky.count != count && (sky.count == 1 || sky.count == jc_formats::sky::LENGTH) {
                info!(
                    "game frame {game_frame}: the object of the sky is at count {} of its \
                     travel",
                    sky.count
                );
            }
        }
        // The sliding bars take the state a script has told them and draw. HYPOTHESIS: the
        // object's task stands behind the scripts' in the game frame, as the sky's here.
        if let Some((_, found, task, drawn)) = (self.room.slides.as_mut()).filter(|_| !rested) {
            let (told, count) = (task.told, task.count);
            *drawn = task.pass(found);
            if told != task.told || (count != task.count && [0, found.frames].contains(&count)) {
                info!(
                    "game frame {game_frame}: the sliding bars are at count {} of {} (told \
                     state {told}): {drawn:?}",
                    task.count, found.frames
                );
            }
        }
        // The two played models take a told state, ask for their sounds and step.
        // HYPOTHESIS: the task runner's pass counter, whose lowest bit steps the second
        // model, is odd in the game frames that are.
        if let Some((at, _, task, drawn)) = (self.room.plays.as_mut()).filter(|_| !rested) {
            let (told, phase) = (task.told, task.phase);
            let (frames, asked) = task.pass(game_frame % 2 == 1);
            *drawn = frames;
            let round = played::Asked::Sound(played::SECOND_SOUND);
            if phase != task.phase || asked.iter().any(|&asked| asked != round) {
                info!(
                    "game frame {game_frame}: the object that plays two models is in phase {} \
                     (told state {told}), draws the frames {frames:?} and asks for {asked:?}",
                    task.phase
                );
            }
            let at = *at;
            (self.room.music).extend(asked.into_iter().map(|asked| {
                let heard = match asked {
                    played::Asked::Sound(sound) => Heard::Sound(sound),
                    played::Asked::Keep(sound) => Heard::Kept { at, sound },
                    played::Asked::LetGo(_) => Heard::Ended { at },
                };
                (game_frame, heard)
            }));
        }
        // The shake of the picture: the scroll routine of the camera task, which stands
        // behind the scripts in the field's list, adds the byte of the counter to the
        // vertical offset, and with the fifteenth the shake is over.
        self.room.shaken = 0;
        if let Some(count) = self.room.shake {
            self.room.shaken = self.room.shake_table[count].into();
            info!(
                "game frame {game_frame}: the shake moves the picture down by {}",
                self.room.shaken
            );
            self.room.shake = Some(count + 1).filter(|count| *count < arena::SHAKE_FRAMES);
        }
        // The sound of a ride's room, at its pace.
        if let Some((found, task)) = (self.room.ride_sounds.as_mut()).filter(|_| !rested) {
            let sound = task.pass(found);
            (self.room.music).extend(sound.map(|sound| (game_frame, Heard::Sound(sound))));
        }
        // The beat of room 8: a sound for each beat that falls into the game frame. The
        // pad's motors the first beat also asks for are not in code.
        if let Some(beat) = (self.room.beat.as_mut()).filter(|_| !rested) {
            let (first, second) = beat.pass();
            let sounds = [first, second].into_iter().filter(|&beats| beats);
            (self.room.music).extend(sounds.map(|_| (game_frame, Heard::Sound(beat::SOUND))));
        }
        // The sound at an even pace of the rooms 262 and 263.
        if let Some(toll) = (self.room.toll.as_mut()).filter(|_| !rested) {
            let sound = toll.pass().then_some(toll::SOUND);
            (self.room.music).extend(sound.map(|sound| (game_frame, Heard::Sound(sound))));
        }
        // The task that leaves the forest is place 12 of the field's list, behind the
        // object tasks and right in front of the fade's.
        if let Some((mut leaving, trees, from)) = self.room.leaving.take() {
            let state = leaving.state;
            let mut field = Leaves {
                room: &mut self.room,
                player: player.as_deref_mut(),
            };
            if leaving.pass(&mut field) {
                let room = leave_forest::leave(&mut self.room.carried, &trees, from, view);
                info!(
                    "game frame {game_frame}: the task that leaves the forest is through: \
                     story variable {FOREST_VARIABLE} is {} (from {from:?}, view {view}), the \
                     game goes to the stored room {room}",
                    self.room.carried.variable(FOREST_VARIABLE)
                );
                self.room.leaves = Some(room.into());
            } else {
                if leaving.state != state {
                    info!(
                        "game frame {game_frame}: the task that leaves the forest comes to \
                         state {}",
                        leaving.state
                    );
                }
                self.room.leaving = Some((leaving, trees, from));
            }
        }
        self.room.fade.step();
        let followed = match self.room.followed {
            Followed::Point(address) => words_at(&package, address),
            Followed::Thing(0) => player.as_deref().map(|walker| walker.player.position),
            Followed::Thing(thing) => (usize::try_from(thing - 1).ok())
                .and_then(|slot| actors.get(slot))
                .map(|actor| actor.mover.position),
        };
        // Both of the camera's scroll callers step the zoom first, with the answer the
        // projection gave them the last time.
        let zoom = self.room.zoom;
        let depth = self.room.depth.unwrap_or(NO_DEPTH);
        self.room.zoom.step(distance, depth);
        if zoom != self.room.zoom && !self.room.zoom.runs {
            info!(
                "game frame {game_frame}: the zoom stands at {} of 4096",
                self.room.zoom.value
            );
        }
        let share = self.room.glide.step();
        // The answer of the projection: a quarter of the depth of the followed point
        // raised, mixed with the target's by the glide's share.
        let quarter = |[x, y, z]: [i32; 3]| {
            (camera.to_view([x, y - glide::RAISED, z])[2].floor() as i32) >> 2
        };
        self.room.depth = followed.map(|followed| {
            let (from, to) = (quarter(followed), quarter(self.room.glide.target));
            match share {
                0 => from,
                share => from + (to - from) * share / glide::ALL,
            }
        });
        self.room.camera = followed.map(|followed| CameraPoints {
            followed,
            target: self.room.glide.target,
            share,
        });
        // The field's fight task stands behind the camera's in the list of a game frame.
        let mut engine = Running {
            room: &mut self.room,
            actors: &mut *actors,
            player,
            package: &package,
            file: self.number,
            camera,
            distance,
            view,
        };
        engine.touch();
        self.room.game_frame += 1;
    }

    fn pass_gates(
        room: &mut Room,
        tasks: &mut Vec<(Task, Option<u32>)>,
        package: &ScenePackage,
        mut player: Option<&mut Walker>,
    ) {
        let Some(gates) = &room.gates else {
            return;
        };
        let game_frame = room.game_frame;
        let stands = (player.as_deref()).map(|walker| {
            (
                walker.player.position,
                walker.player.angle,
                walker.body.arc.clone(),
            )
        });
        for (n, (at, gate)) in gates.gates.iter().enumerate() {
            let crossed = &mut room.crossings[n];
            // The task of the list's first record ends with its last run.
            if let Some((show, clock)) = &mut crossed.shown {
                clock.step(show);
            }
            let around = gate::Around {
                at: stands.as_ref().is_some_and(|(place, angle, arc)| {
                    (usize::try_from(gate.region).ok())
                        .and_then(|region| room.triggers.get(region))
                        .is_some_and(|region| region.triangle_at(*place).is_some())
                        && player::talk_faced(arc, *place, *angle, gate.point)
                }),
                script_runs: crossed.script.is_some_and(|number| {
                    (tasks.iter()).any(|(task, over)| task.number == number && over.is_none())
                }),
                has_thing: room.carried.keeping.has_story_thing(gate.thing),
                shown: (crossed.shown.as_ref()).is_some_and(|(_, clock)| !clock.ended),
            };
            let (passed, over) = crossed.crossing.pass(gate, around);
            if over {
                crossed.shown = None;
            }
            match passed {
                gate::Passed::Nothing => {}
                gate::Passed::Starts { with_thing } => {
                    let script = match with_thing {
                        true => gates.with_thing,
                        false => gates.without,
                    };
                    let number = free_number(tasks, &room.by_view);
                    info!(
                        "game frame {game_frame}: the gate at {at:#010x} starts script {number} \
                         at {script:#010x}, {} thing {} of the story",
                        if with_thing { "with" } else { "without" },
                        gate.thing
                    );
                    tasks.push((Task::new(number, script), None));
                    room.owners.insert(number, n);
                    crossed.script = Some(number);
                }
                gate::Passed::Shows(list) => {
                    let shows = package.shows_at(list);
                    info!(
                        "game frame {game_frame}: the gate at {at:#010x} hands the show list \
                         {list:#010x} to the spawner, {} records",
                        shows.len()
                    );
                    crossed.shown = shows.first().map(|show| (*show, ShowClock::default()));
                    room.shows.extend(shows);
                }
                gate::Passed::Crosses => {
                    gate.opens(&mut room.carried.variants);
                    info!(
                        "game frame {game_frame}: the gate at {at:#010x} puts the player on \
                         {:?} with angle {}; music {} is asked for",
                        gate.place, gate.angle, gate.music
                    );
                    // The gate asks for its music and notes it, as `0xFF04` does.
                    let asked = u32::try_from(gate.music).map(script::Music::Start);
                    room.music
                        .extend(asked.map(|asked| (game_frame, Heard::Music(asked))));
                    if let Some(walker) = &mut player {
                        walker.player.arrive(gate.place, gate.angle);
                    }
                }
            }
        }
        // The object for a player from the place map comes behind the gates in the list.
        if let Some(arrives) = &mut room.arriving {
            let at = arrives.at;
            let light = arrives.light.map(|light| light.list);
            match arrives.arriving.pass(&arrives.arrival, light) {
                gate::Arrived::Nothing => {}
                gate::Arrived::Shows(list) => {
                    let shows = package.shows_at(list);
                    info!(
                        "game frame {game_frame}: the object at {at:#010x} hands the show list \
                         {list:#010x} to the spawner, {} records",
                        shows.len()
                    );
                    room.shows.extend(shows);
                }
                gate::Arrived::Ends => {
                    info!(
                        "game frame {game_frame}: the object at {at:#010x} clears story \
                         variable {FOREST_VARIABLE} and ends"
                    );
                    room.carried.set_variable(FOREST_VARIABLE, 0);
                }
                gate::Arrived::Lights(list) => {
                    let shows = package.shows_at(list);
                    info!(
                        "game frame {game_frame}: the object at {at:#010x} reaches state 2: the \
                         show list {list:#010x} goes to the spawner, {} records, and the task \
                         of the light is made",
                        shows.len()
                    );
                    room.shows.extend(shows);
                    // HYPOTHESIS: the seed the library's generator has then.
                    let random = Random { seed: game_frame };
                    room.halo = Some((Haloing::new(), random));
                }
            }
        }
        // The task of the light: its start routine makes the first call in the game
        // frame it is made, the wrapper one in every game frame after.
        room.sparks.clear();
        if let Some((task, random)) = &mut room.halo {
            let step = task.call(|| random.draw());
            if step.chime {
                let chime = Heard::Music(script::Music::Start(halo::CHIME));
                room.music.push((game_frame, chime));
            }
            room.sparks = step.sparks;
            if step.ends {
                info!("game frame {game_frame}: the task of the light ends itself");
                room.halo = None;
            }
        }
    }

    fn pass_passages(
        room: &mut Room,
        tasks: &mut Vec<(Task, Option<u32>)>,
        package: &ScenePackage,
        mut player: Option<&mut Walker>,
    ) {
        let Some(passages) = &room.passages else {
            return;
        };
        let game_frame = room.game_frame;
        let stands = (player.as_deref()).map(|walker| {
            (
                walker.player.position,
                walker.player.angle,
                walker.body.arc.clone(),
            )
        });
        for (n, (at, bank)) in passages.passages.iter().enumerate() {
            let passed = &mut room.passings[n];
            let around = passage::Around {
                at: stands.as_ref().is_some_and(|(place, angle, arc)| {
                    (usize::try_from(bank.region).ok())
                        .and_then(|region| room.triggers.get(region))
                        .is_some_and(|region| region.triangle_at(*place).is_some())
                        && player::talk_faced(arc, *place, *angle, bank.point)
                }),
                script_runs: passed.script.is_some_and(|number| {
                    (tasks.iter()).any(|(task, over)| task.number == number && over.is_none())
                }),
                flag: u32::try_from(bank.flag).is_ok_and(|flag| room.carried.keeping.flag(flag)),
            };
            match passed.passing.pass(bank, around) {
                passage::Passed::Nothing => {}
                passage::Passed::Starts => {
                    let number = free_number(tasks, &room.by_view);
                    info!(
                        "game frame {game_frame}: the bank of the passage at {at:#010x} starts \
                         script {number} at {:#010x}: story flag {} is set",
                        passages.script, bank.flag
                    );
                    tasks.push((Task::new(number, passages.script), None));
                    room.passage_owners.insert(number, n);
                    passed.script = Some(number);
                }
                passage::Passed::Shows(list) => {
                    let shows = package.shows_at(list);
                    info!(
                        "game frame {game_frame}: the bank of the passage at {at:#010x} hands \
                         the show list {list:#010x} to the spawner, {} records",
                        shows.len()
                    );
                    room.shows.extend(shows);
                }
                passage::Passed::Crosses => {
                    info!(
                        "game frame {game_frame}: the bank of the passage at {at:#010x} puts \
                         the player on {:?} with angle {}",
                        bank.place, bank.angle
                    );
                    if let Some(walker) = &mut player {
                        walker.player.arrive(bank.place, bank.angle);
                    }
                }
            }
        }
    }

    fn pass_rides(
        room: &mut Room,
        tasks: &mut Vec<(Task, Option<u32>)>,
        mut player: Option<&mut Walker>,
    ) {
        let Some((found, riding, script, comes_back)) = &mut room.rides else {
            return;
        };
        let (game_frame, at, way) = (room.game_frame, found.at, found.ride);
        if std::mem::take(comes_back) {
            info!(
                "game frame {game_frame}: the player who came back is put on {:?} with \
                 angle {}",
                way.place, way.angle
            );
            if let Some(walker) = &mut player {
                walker.player.arrive(way.place, way.angle);
            }
        }
        let in_region = player.as_deref().is_some_and(|walker| {
            (usize::try_from(way.region).ok())
                .and_then(|region| room.triggers.get(region))
                .is_some_and(|region| region.triangle_at(walker.player.position).is_some())
        });
        let script_runs = script.is_some_and(|number| {
            (tasks.iter()).any(|(task, over)| task.number == number && over.is_none())
        });
        match riding.pass(in_region, script_runs) {
            ride::Rode::Nothing => {}
            ride::Rode::Starts => {
                let number = free_number(tasks, &room.by_view);
                info!(
                    "game frame {game_frame}: the player stands in region {} and the object \
                     at {at:#010x} starts script {number} at {:#010x}",
                    way.region, found.leaves
                );
                tasks.push((Task::new(number, found.leaves), None));
                *script = Some(number);
            }
            ride::Rode::Leaves => {
                let variables = way.variables();
                for (number, value) in ride::ROOM_VARIABLES.into_iter().zip(variables) {
                    room.carried.set_variable(number, value);
                }
                if let Ok(flag) = u32::try_from(way.flag) {
                    room.carried.keeping.set_flag(flag);
                }
                info!(
                    "game frame {game_frame}: the object at {at:#010x} sends the game to \
                     room {}: the story variables {:?} are {variables:?} (room {}), story \
                     flag {} is set",
                    way.room,
                    ride::ROOM_VARIABLES,
                    way.goes_to,
                    way.flag
                );
                room.leaves = u32::try_from(way.room).ok();
            }
        }
    }

    fn pass_warps(
        room: &mut Room,
        tasks: &mut Vec<(Task, Option<u32>)>,
        mut player: Option<&mut Walker>,
    ) {
        let Some((found, warping, script, (area, place))) = &mut room.warps else {
            return;
        };
        let (game_frame, at, way) = (room.game_frame, found.at, found.warp);
        let in_region = player.as_deref().is_some_and(|walker| {
            (usize::try_from(way.region).ok())
                .and_then(|region| room.triggers.get(region))
                .is_some_and(|region| region.triangle_at(walker.player.position).is_some())
        });
        let around = warp::Around {
            flag: room.carried.keeping.flag(warp::FLAG),
            in_region,
            has_thing: way.thing >= 0 && room.carried.keeping.has_story_thing(way.thing),
            script_runs: script.is_some_and(|number| {
                (tasks.iter()).any(|(task, over)| task.number == number && over.is_none())
            }),
        };
        let (heard, warped) = warping.pass(&way, around);
        let wanted = u16::try_from(*place).unwrap_or(0);
        let heard: Vec<Heard> = match heard {
            None => Vec::new(),
            Some(warp::Heard::Kept) => vec![Heard::Kept {
                at,
                sound: warp::KEPT_SOUND,
            }],
            Some(warp::Heard::Ended) => vec![Heard::Ended { at }],
            Some(warp::Heard::Music) => (found.music_of(0, wanted).into_iter())
                .map(|music| Heard::Music(script::Music::Ask(music)))
                .collect(),
            Some(warp::Heard::MusicAndSound) => (found.music_of(1, wanted).into_iter())
                .map(|music| Heard::Music(script::Music::Ask(music)))
                .chain([Heard::Sound(warp::SOUND)])
                .collect(),
        };
        if !heard.is_empty() {
            info!("game frame {game_frame}: the warp at {at:#010x} asks the sound for {heard:?}");
        }
        (room.music).extend(heard.into_iter().map(|heard| (game_frame, heard)));
        let mut start = |script_at: u32, why: &str| {
            let number = free_number(tasks, &room.by_view);
            info!(
                "game frame {game_frame}: {why}: the warp at {at:#010x} starts script {number} \
                 at {script_at:#010x}"
            );
            tasks.push((Task::new(number, script_at), None));
            number
        };
        match warped {
            warp::Warped::Nothing => {}
            warp::Warped::Arrives => {
                room.carried.keeping.clear_flag(warp::FLAG);
                *script = Some(start(found.arrival, "story flag 114 is set and is cleared"));
            }
            warp::Warped::Departs => {
                room.carried.keeping.set_flag(warp::FLAG);
                *script = Some(start(
                    found.departure,
                    "the player stands in its region with its thing; story flag 114 is set",
                ));
            }
            warp::Warped::Leaves => {
                let exit = Exit {
                    record: at,
                    view_mask: u32::MAX,
                    arrival: way.arrival,
                    angle: 0,
                    leave_flag: 0,
                    region: way.region,
                    area: *area,
                    place: *place,
                    slot: way.slot,
                };
                if let Some(walker) = &mut player
                    && walker.warp_out(exit, way.angle)
                {
                    info!(
                        "game frame {game_frame}: the script of the warp at {at:#010x} has \
                         ended: the game goes to slot {} of area {area} place {place}, the \
                         player on {:?} with angle {}",
                        way.slot, way.arrival, way.angle
                    );
                }
            }
        }
    }

    pub fn marker(&self) -> Moved {
        match &self.room.warps {
            Some((found, warping, ..)) if warping.script => Moved {
                place: found.warp.shows_at,
                angles: [0, found.warp.shows_angle, 0],
            },
            _ => Moved::default(),
        }
    }

    fn pass_corridor(room: &mut Room, player: Option<&Walker>) {
        let game_frame = room.game_frame;
        match room.generator {
            None => {}
            Some(Generator::Starts) => {
                info!("game frame {game_frame}: the generator of the corridor starts");
                room.corridor_asks = Some(CorridorAsks::Enter);
            }
            Some(Generator::Steps) => {
                let (Some(corridor), Some(walker)) = (&room.carried.corridor, player) else {
                    return;
                };
                let place = walker.player.position;
                let link = (corridor.links().into_iter().enumerate()).position(|(n, linked)| {
                    linked
                        && (room.triggers.get(n))
                            .is_some_and(|region| region.triangle_at(place).is_some())
                });
                if let Some(link) = link {
                    info!(
                        "game frame {game_frame}: the player stands at {place:?} in trigger \
                         region {link}: the generator of the corridor moves through link {link}"
                    );
                    room.corridor_asks = Some(CorridorAsks::Link(link));
                }
            }
        }
    }

    fn pass_find(room: &mut Room, tasks: &mut Vec<(Task, Option<u32>)>, player: Option<&Walker>) {
        let Some((finds, searched)) = &mut room.finds else {
            return;
        };
        let (game_frame, find) = (room.game_frame, &finds.find);
        let at = player.is_some_and(|walker| {
            let (place, angle) = (walker.player.position, walker.player.angle);
            (usize::try_from(find.region).ok())
                .and_then(|region| room.triggers.get(region))
                .is_some_and(|region| region.triangle_at(place).is_some())
                && player::talk_faced(&walker.body.arc, place, angle, find.place)
        });
        let script_runs = searched.script.is_some_and(|number| {
            (tasks.iter()).any(|(task, over)| task.number == number && over.is_none())
        });
        let amount = searched.search.amount;
        match searched.search.pass(find, at, script_runs) {
            search::Passed::Nothing => {}
            search::Passed::Starts => {
                let number = free_number(tasks, &room.by_view);
                info!(
                    "game frame {game_frame}: the thing to be searched at {:#010x} starts \
                     script {number} at {:#010x}",
                    finds.at, finds.script
                );
                tasks.push((Task::new(number, finds.script), None));
                searched.script = Some(number);
            }
            search::Passed::Found => {
                info!(
                    "game frame {game_frame}: the thing at {:#010x} is searched, story flag \
                     {} is set",
                    finds.at, find.flag
                );
                if let Ok(flag) = u32::try_from(find.flag) {
                    room.carried.keeping.set_flag(flag);
                }
            }
        }
        if searched.search.amount != amount && searched.search.amount == search::OPEN {
            info!(
                "game frame {game_frame}: the thing at {:#010x} is open",
                finds.at
            );
        }
    }

    fn pass_lock(room: &mut Room, tasks: &mut Vec<(Task, Option<u32>)>, player: Option<&Walker>) {
        let Some(locked) = &mut room.lock else {
            return;
        };
        let game_frame = room.game_frame;
        let triggers = &room.triggers;
        let at = |side: &lock::Side| {
            player.is_some_and(|walker| {
                let (place, angle) = (walker.player.position, walker.player.angle);
                (usize::try_from(side.region).ok())
                    .and_then(|region| triggers.get(region))
                    .is_some_and(|region| region.triangle_at(place).is_some())
                    && player::talk_faced(&walker.body.arc, place, angle, side.point)
            })
        };
        let script_runs = locked.script.is_some_and(|number| {
            (tasks.iter()).any(|(task, over)| task.number == number && over.is_none())
        });
        let has_thing = room.carried.keeping.has_story_thing(locked.lock.thing);
        let Some(started) = (locked.locking).pass(&locked.lock, at, script_runs, has_thing) else {
            return;
        };
        let number = free_number(tasks, &room.by_view);
        info!(
            "game frame {game_frame}: the lock at {:#010x} starts script {number} at {:#010x} \
             on side {}: {}",
            locked.lock.record,
            started.script,
            locked.locking.side,
            match started.opens {
                true => "the door opens",
                false => "the side's lines",
            }
        );
        tasks.push((Task::new(number, started.script), None));
        (locked.script, locked.lines) = (Some(number), started.lines);
    }

    pub fn pass_followers(&mut self, view: usize, screen: &mut FieldScreen) {
        let room = &mut self.room;
        let game_frame = room.game_frame;
        for (found, held) in &mut room.followers {
            if view < 32 && found.view_mask >> view & 1 == 0 {
                continue;
            }
            let set = u32::try_from(found.flag).is_ok_and(|flag| room.carried.keeping.flag(flag));
            // "The flag is set" is the flag itself or a told state of 0.
            let told = room.pictures_told.get(&found.record) == Some(&0);
            if held.as_ref().is_none_or(|held| held.view != view) {
                let Some(group) = screen.group_named(found.number) else {
                    if held.is_none() {
                        warn!(
                            "the picture at {:#010x}: view {view} has no animated group \
                             named {}",
                            found.record, found.number
                        );
                    }
                    // Not asked again for this view.
                    *held = Some(Held {
                        view,
                        group: None,
                        following: Following::new(found, set),
                    });
                    continue;
                };
                let following = Following::new(found, set);
                screen.hold(group, usize::from(following.frame));
                info!(
                    "game frame {game_frame}: the picture at {:#010x} holds group {group} \
                     of view {view} on frame {} (story flag {} is {})",
                    found.record,
                    following.frame,
                    found.flag,
                    if set { "set" } else { "not set" }
                );
                *held = Some(Held {
                    view,
                    group: Some(group),
                    following,
                });
            }
            let Some(Held {
                group: Some(group),
                following,
                ..
            }) = held
            else {
                continue;
            };
            let (group, before) = (*group, following.on);
            let through = screen.through(group, usize::from(following.frame));
            let Some(followed) = following.pass(found, set || told, through) else {
                continue;
            };
            screen.hold(group, usize::from(followed.frame));
            if following.on != before || followed.sound.is_some() {
                info!(
                    "game frame {game_frame}: the picture at {:#010x} goes to the frames \
                     {} the flag from frame {}; sound {:?}",
                    found.record,
                    if following.on { "with" } else { "without" },
                    followed.frame,
                    followed.sound
                );
            }
            // The sound of a change and of the first round behind it (`0x800314F0`).
            let sound = followed.sound.and_then(|sound| u32::try_from(sound).ok());
            (room.music).extend(sound.map(|sound| (game_frame, Heard::Sound(sound))));
        }
    }

    fn pass_switches(room: &mut Room, mut player: Option<&mut Walker>, view: usize) {
        let game_frame = room.game_frame;
        let shown = |mask: u32| view >= 32 || mask >> view & 1 != 0;
        let stands = (player.as_deref()).map(|walker| {
            (
                walker.player.position,
                walker.player.angle,
                walker.body.arc.clone(),
            )
        });
        for (found, pressed) in &mut room.switches {
            if !shown(found.view_mask) {
                continue;
            }
            let at = stands.as_ref().is_some_and(|(place, angle, arc)| {
                (usize::try_from(found.region).ok())
                    .and_then(|region| room.triggers.get(region))
                    .is_some_and(|region| region.triangle_at(*place).is_some())
                    && player::talk_faced(arc, *place, *angle, found.place)
            });
            let turned = pressed.pass(found, at);
            let (Some(turned), Ok(flag)) = (turned, u32::try_from(found.flag)) else {
                continue;
            };
            match turned {
                switch::Turned::Set => room.carried.keeping.set_flag(flag),
                switch::Turned::Cleared => room.carried.keeping.clear_flag(flag),
            }
            info!(
                "game frame {game_frame}: the switch at {:#010x} is pressed, story flag \
                 {flag} is {}",
                found.record,
                match turned {
                    switch::Turned::Set => "set",
                    switch::Turned::Cleared => "cleared",
                }
            );
        }
        for list in (room.barriers.iter()).filter(|list| shown(list.view_mask)) {
            for barrier in &list.barriers {
                let set =
                    u32::try_from(barrier.flag).is_ok_and(|flag| room.carried.keeping.flag(flag));
                let Ok(region) = usize::try_from(barrier.region) else {
                    continue;
                };
                let byte = barrier.byte(set);
                // Table 4 is the ground of the actors that walk, table 2 the player's.
                let changed = if barrier.second {
                    room.ground.as_mut().is_some_and(|ground| {
                        match Arc::make_mut(ground).flags.get_mut(region) {
                            Some(flag) if *flag != byte => {
                                *flag = byte;
                                true
                            }
                            _ => false,
                        }
                    })
                } else {
                    (player.as_deref_mut()).is_some_and(|walker| walker.set_barrier(region, byte))
                };
                if changed {
                    info!(
                        "game frame {game_frame}: barrier {region} of table {} {} (story \
                         flag {})",
                        if barrier.second { 4 } else { 2 },
                        if byte == 0 { "is taken away" } else { "stands" },
                        barrier.flag
                    );
                }
            }
        }
    }

    fn pass_card(room: &mut Room, player: Option<&Walker>) {
        let Some(carded) = &mut room.card else {
            return;
        };
        let (game_frame, card) = (room.game_frame, &carded.card);
        let at = player.is_some_and(|walker| {
            let (place, angle) = (walker.player.position, walker.player.angle);
            player::talk_near(place, card.place)
                && player::talk_faced(&walker.body.arc, place, angle, card.place)
        });
        // The maker of the task runs the start routine, which runs the routine once,
        // before the first game frame of the room.
        if !carded.floating.started {
            carded.floating.start(at);
        }
        let (marked, ended) = (carded.floating.marked, carded.floating.ended);
        let passed = carded.floating.pass(at);
        carded.drawn = passed.drawn;
        if carded.floating.marked != marked {
            info!(
                "game frame {game_frame}: the object of the memory card at {:#010x} is {}",
                card.record,
                if marked {
                    "not marked any more"
                } else {
                    "marked"
                }
            );
        }
        if carded.floating.ended && !ended {
            info!(
                "game frame {game_frame}: the object of the memory card at {:#010x} ends",
                card.record
            );
        }
        if passed.asks {
            info!(
                "game frame {game_frame}: the object of the memory card asks for field mode \
                 6 with the text at {:#010x}",
                card.text
            );
            carded.asked = true;
        }
    }

    fn pass_map_triggers(
        room: &mut Room,
        tasks: &mut Vec<(Task, Option<u32>)>,
        player: Option<&Walker>,
        view: usize,
    ) {
        let game_frame = room.game_frame;
        let place = player.map(|walker| walker.player.position);
        for (trigger, waiting, script) in &mut room.map_triggers {
            let masked = view < 32 && trigger.view_mask >> view & 1 == 0;
            let inside = !masked
                && (place.zip(usize::try_from(trigger.region).ok()))
                    .and_then(|(place, region)| room.triggers.get(region)?.triangle_at(place))
                    .is_some();
            if !waiting.pass(inside) {
                continue;
            }
            let number = free_number(tasks, &room.by_view);
            info!(
                "game frame {game_frame}: the player stands in trigger region {}: the \
                 trigger of the place map at {:#010x} starts script {number} at {:#010x}",
                trigger.region, trigger.record, trigger.script
            );
            tasks.push((Task::new(number, trigger.script), None));
            *script = Some(number);
        }
    }

    /// The game frame of the message windows: tasks of their own, which run behind the
    /// actors' (place 23 of the field's list). A window that has gone stays with its task
    /// until the script lets go of it. `name` is the player's, as a text.
    pub fn windows(&mut self, name: &[u16]) {
        let room = &mut self.room;
        let game_frame = room.game_frame.saturating_sub(1);
        let setting = room.setting;
        let mut spoke = false;
        for (task, _) in &mut self.tasks {
            let number = task.number;
            // A notice: the confirm of the menus counts when the script's waiter has
            // allowed it, and the step that ends the notice hands its things over.
            if let Some(shown) = task.speech.notice.as_mut() {
                let ends = shown.step(room.pad.pressed & pad::CROSS != 0);
                if let (true, Some(noticed)) = (ends, room.noticed.get(&number)) {
                    let things =
                        (noticed.things.iter()).filter(|thing| thing.kind != notice::NO_THING);
                    for thing in things {
                        let sound = notice::ending_sound(noticed.way, thing.kind, thing.number);
                        (room.music).extend(sound.map(|sound| (game_frame, Heard::Sound(sound))));
                        (room.carried.keeping).notice_ends(
                            noticed.way,
                            thing.kind,
                            thing.number,
                            thing.count,
                            room.carried.worn,
                        );
                    }
                    info!(
                        "game frame {game_frame}: the notice of script {number} ends and does \
                         what it said (way {}); the player has {} money and {} things of the \
                         story",
                        noticed.way,
                        room.carried.keeping.money(),
                        room.carried.keeping.story_things()
                    );
                }
                continue;
            }
            let Some(window) = (task.speech.window.as_mut()).filter(|window| !window.is_gone())
            else {
                continue;
            };
            let Some(said) = room.said.get_mut(&number) else {
                continue;
            };
            let text = (&room.groups[..], &room.font);
            // "Auto Play" asks the voices: the group has a cue, the page on screen
            // has had its line started, and no line plays.
            let has_cue = (room.cues.iter()).any(|cue| usize::from(cue.group) == said.group);
            window.hear(has_cue && room.spoken_page == Some(window.on_screen) && room.voice_silent);
            let waits = !room.voice_none_asked;
            if let Some(line) = window_pass(window, said, text, &room.pad, name, setting, waits) {
                info!("game frame {game_frame}: the window of script {number} {line}");
            }
            let sounds = said.pass.sounds.iter();
            (room.music).extend(sounds.map(|&sound| (game_frame, Heard::Sound(sound.into()))));
            let ends_voice = said.pass.ends_voice;
            spoke = true;
            let reached = |cue: u8| window.cue(usize::from(cue));
            let found = voice::spoken(&room.cues, said.group as u32, reached).copied();
            if let Some(cue) = found.filter(|_| setting & SPEECH != 0)
                && room.spoken != Some(cue)
            {
                room.spoken = Some(cue);
                // The line is asked for from this game frame on.
                room.spoken_page = Some(window.on_screen);
                room.voice_none_asked = false;
                info!(
                    "game frame {game_frame}: the window of script {number} speaks group {} \
                     cue {}: channel {} from row {}, {} rows",
                    cue.group, cue.cue, cue.line.channel, cue.line.start, cue.line.rows
                );
                room.music.push((game_frame, Heard::Line(cue.line)));
            }
            // The pad's half asks the voice to end while the window waits for the
            // answers or for one of them. Behind the look above, which stands for the
            // waiter of the game frame to come: an end does nothing to a line that is
            // asked for and has not started.
            if ends_voice {
                room.music.push((game_frame, Heard::LineEnds));
            }
        }
        // The window's last confirm forgets the record started last, and the message's
        // command asks the voice to end (step 4 of the handler).
        if room.spoke && !spoke {
            room.spoken = None;
            room.spoken_page = None;
            room.music.push((game_frame, Heard::LineEnds));
        }
        room.spoke = spoke;
        // The window of a lesson's message is a task of the same pool.
        let taught = room.fight.as_mut().and_then(|fight| fight.saying.as_mut());
        if let Some((window, said)) = taught.filter(|(window, _)| !window.is_gone()) {
            let text = (&room.groups[..], &room.font);
            if let Some(line) = window_pass(window, said, text, &room.pad, name, setting, false) {
                info!("game frame {game_frame}: the window of the lesson {line}");
            }
            let sounds = said.pass.sounds.iter();
            (room.music).extend(sounds.map(|&sound| (game_frame, Heard::Sound(sound.into()))));
        }
    }

    /// The buttons besides the confirm that a player holds in the game frame to come:
    /// the directions, which move the cursor of a question, and the triangle.
    pub fn hold(&mut self, keys: u16) {
        // The circle and L1 are here for the field task's question of the menu alone.
        let read = pad::UP | pad::DOWN | pad::LEFT | pad::RIGHT | pad::TRIANGLE;
        self.room.keys = keys & (read | pad::CIRCLE | pad::L1);
    }

    pub fn asks_menu(&self) -> bool {
        let pressed = self.room.pad.pressed;
        self.room.selector < 0
            && self.room.fight.is_none()
            && self.room.leaving.is_none()
            && pressed & (pad::CROSS | pad::L1) == 0
            && pressed & pad::CIRCLE != 0
    }

    pub fn frees_things(&self) -> bool {
        self.room.selector < 0 && self.room.leaving.is_none()
    }

    /// The answers a run with nobody at the pad gives to the questions of the room, in
    /// their order (`--answer-in=ROOM:N,...`).
    pub fn answer_with(&mut self, answers: Vec<i16>) {
        self.room.answers = answers;
    }

    /// The message window that is up: the one of a lesson's message, or of the first
    /// task that has one.
    pub fn spoken(&self) -> Option<Spoken<'_>> {
        let taught = (self.room.fight.as_ref())
            .and_then(|fight| fight.saying.as_ref())
            .filter(|(window, _)| !window.is_gone());
        if let Some((_, said)) = taught {
            return self.spoken_of(said);
        }
        self.tasks.iter().find_map(|(task, _)| {
            (task.speech.window.as_ref()).filter(|window| !window.is_gone())?;
            self.spoken_of(self.room.said.get(&task.number)?)
        })
    }

    fn spoken_of<'a>(&'a self, said: &'a Said) -> Option<Spoken<'a>> {
        Some(Spoken {
            file: self.number,
            thing: usize::try_from(said.thing).ok()?,
            other: said.other as usize,
            group: self.room.groups.get(said.group)?,
            picture_left: said.picture_left,
            open_answers: said.open_answers,
            place: said.place,
            pass: &said.pass,
            passes: said.passes,
            game_frame: self.room.game_frame,
        })
    }

    /// The command menu of the fight while it waits for the player's side, as its last
    /// pass left it, and the game frame of that pass, for whoever paints it.
    pub fn fight_menu(&self) -> Option<FightMenu<'_>> {
        let fight = self.room.fight.as_ref()?;
        // The menu is there while the fight waits for a command of the player's side.
        let asked = (fight.menu.as_ref())
            .zip(fight.bout.as_ref())
            .zip(self.room.fights.as_ref());
        if asked.is_none() && fight.hud.is_none() {
            return None;
        }
        let places = item_places(&self.room.carried.keeping);
        let (mut items, mut own) = (fight_menu_panel::Items::default(), Vec::new());
        // The box of the own side shows its subject (`0x800C9E69`), which the states of
        // a summon and of a leave write with the fighter on the field (`0x800C9E52`),
        // and the target while the picker of the own side is on one.
        let mut subject = fight.fighter;
        let mut moves = None;
        if let Some(((menu, bout), rules)) = asked {
            let lists = fight_menu::Lists {
                fight: &bout.fight,
                tables: &rules.tables,
                places: &places,
            };
            let first = usize::try_from(menu.items().first).unwrap_or(0);
            items = fight_menu_panel::Items {
                rows: std::array::from_fn(|row| lists.row(first + row)),
                last_place: lists.last_place(),
            };
            if menu.side() != 0 {
                moves = Some(fight_menu_panel::Moves {
                    attacks: lists.attack_rows(),
                    spells: lists.spell_rows(menu.spells().first),
                });
            }
            if menu.drew().picks.is_some() {
                own = lists.picker(menu);
            }
            subject = menu.drew().subject;
        }
        // The marks of the menu's last pass, with the menus' time (a game frame of a
        // fight is two refreshes), and its preview.
        let drew = (asked.map(|((menu, _), _)| menu.drew())).unwrap_or_default();
        let time = jc_formats::menu::frame_time(self.room.game_frame, 2);
        let marks = fight_hud::Marks::of(drew.marks, time, fight.preview);
        // The HUD goes by the records as the picture has them.
        let none = Default::default();
        let records = (fight.seen.as_ref()).map_or(&none, |seen| &seen.records);
        Some(FightMenu {
            menu: asked.map(|((menu, _), _)| menu),
            items,
            moves,
            own,
            hud: fight.hud.as_ref().map_or_else(Vec::new, |hud| {
                let boss = |opponent: usize| {
                    let record = records.get(OPPONENTS + opponent).and_then(Option::as_ref);
                    record.is_some_and(|record| is_boss(record.species))
                };
                hud.drawn(records, boss, subject, &marks, self.room.game_frame)
            }),
            names: std::array::from_fn(|slot| records[slot].as_ref().map(|record| record.name)),
            game_frame: self.room.game_frame,
        })
    }

    /// The notice that is up, of the first task that has one.
    pub fn noticed(&self) -> Option<&Noticed> {
        self.tasks.iter().find_map(|(task, _)| {
            task.speech.notice.filter(|notice| !notice.is_gone())?;
            self.room.noticed.get(&task.number)
        })
    }

    /// The move of a thing is over: every task is told (`0x800A5B5C`).
    pub fn told(&mut self, thing: usize) {
        info!(
            "game frame {}: the move of thing {thing} is over",
            self.room.game_frame
        );
        for (task, _) in &mut self.tasks {
            task.told(thing);
        }
    }

    /// The player's order record.
    pub fn orders(&mut self) -> &mut Orders {
        &mut self.room.orders
    }

    /// Whether the pad moves the player: not while a script has it, nor until the order
    /// record has seen it stand again.
    pub fn player_is_free(&self) -> bool {
        self.room.orders.state == 0 && !self.room.orders.taken && self.room.fight.is_none()
    }

    /// The fight of a script that is on screen in this game frame, with its camera.
    pub fn fought(&self) -> Option<Fought<'_>> {
        let (fight, place) = (self.room.fight.as_ref()).zip(self.room.fight_place.as_ref())?;
        Some(Fought {
            place,
            centre: fight.centre,
            shot: fight.eye.shot(&place.cameras, &fight.centre),
            scroll: fight.eye.scroll(),
            camera_state: match fight.frame < arena::OPENING_FRAMES {
                _ if fight.lead < Lasting::LEAD => 2,
                true => 3,
                false => 4,
            },
        })
    }

    /// The byte of every slot in the effect block, as the resolver left them for the
    /// run of the fight that is shown; all 0 without one. `--effect-byte=BITS`
    /// (hexadecimal) is a check option for a look with `--fight-asks`, which has no
    /// run and so no resolver: every slot has that byte then, as if the move had
    /// missed it, been refused or taken a layer of its shield.
    pub fn effect_bytes(&self) -> [u8; SLOTS] {
        let showing = (self.room.fight.as_ref()).and_then(|fight| fight.showing.as_ref());
        match showing {
            Some(showing) => showing.run().effect_bytes,
            None => {
                let asked = std::env::args().find_map(|arg| {
                    let bits = arg.strip_prefix("--effect-byte=")?;
                    u8::from_str_radix(bits.trim_start_matches("0x"), 16).ok()
                });
                [asked.unwrap_or(0); SLOTS]
            }
        }
    }

    /// The weapon the player has: the byte of his record, 255 for none.
    pub fn weapon(&self) -> u8 {
        self.room.carried.worn[0]
    }

    /// The room's view says whether the effect of a move still lives: its package is
    /// read or one of its tasks is left. The next run of a fight waits for its end.
    pub fn effect_lives(&mut self, lives: bool) {
        self.room.effect_idle = !lives;
    }

    /// The effects of the actors of a fight that were let go from their hold since the
    /// last call (`0x8001E2F4`).
    pub fn take_effects(&mut self) -> Vec<ActorEffect> {
        std::mem::take(&mut self.room.effects_started)
    }

    pub fn fighter_tints(&self) -> Vec<FighterTint> {
        let Some(seen) = (self.room.fight.as_ref()).and_then(|fight| fight.seen.as_ref()) else {
            return Vec::new();
        };
        (seen.records.iter().enumerate())
            .filter_map(|(slot, record)| {
                let status = record.as_ref()?.status;
                let actor = match slot {
                    0 => None,
                    _ => Some(self.stands_for(slot)?),
                };
                let tint = jc_formats::tint::of_status(status)
                    .and_then(|number| Some((number, *self.room.tints.get(number)?)));
                Some((slot, actor, tint))
            })
            .collect()
    }

    /// The actor of the room that stands on the field for a slot of the fight on
    /// screen: an opponent's, or the figure of a carried creature.
    pub fn stands_for(&self, slot: usize) -> Option<usize> {
        let fight = self.room.fight.as_ref()?;
        let stood = fight.stood.iter().find(|stood| stood.stands_for(slot))?;
        Some(stood.index)
    }

    pub fn held(&self) -> Option<u8> {
        let darkened = |passes: u32| (Lasting::TAKEN * passes).min(255) as u8;
        match &self.room.fight {
            Some(fight) => (self.room.fight_place.as_ref())
                .and_then(|_| Lasting::held(fight.lead).or(fight.trail.and_then(Lasting::trails))),
            None => {
                (self.room.behind.and_then(Lasting::trails)).or(self.room.darkened.map(darkened))
            }
        }
    }

    /// Whether the screen in one colour paints the picture white in this game frame
    /// (a critical hit of a fight).
    pub fn flashes(&self) -> bool {
        self.room.flash > 0
    }

    pub fn rested(&self) -> bool {
        self.room.rested
    }

    pub fn put_away(&self) -> &[usize] {
        (self.room.fight.as_ref()).map_or(&[], |fight| &fight.noted)
    }

    pub fn holds_roamers(&self) -> bool {
        self.room.roamers_held
    }

    /// Whether a script has let the creatures that roam go on since the last call.
    pub fn takes_roamers_let_go(&mut self) -> bool {
        std::mem::take(&mut self.room.roamers_let_go)
    }

    pub fn stalled(&self) -> bool {
        (self.room.fight.as_ref()).is_some_and(|fight| {
            let back = Lasting::back(fight.played.map(|(outcome, _)| outcome));
            (fight.trail).is_some_and(|behind| Lasting::stalls(behind, back))
        })
    }

    pub fn talk(&mut self) -> bool {
        if self.room.pad.pressed & pad::CROSS == 0 || !self.room.confirm {
            return false;
        }
        let taker = (self.tasks.iter())
            .filter(|(_, over)| over.is_none())
            .map(|(task, _)| task.number)
            .find(|number| self.room.can_talk.contains(number));
        let Some(number) = taker else {
            // The second half: with the task selector free, every object task that is
            // marked is told state 0. An object without a script of its own does not
            // take the press, so the pad's command stays.
            if self.room.selector < 0 && self.player_is_free() {
                self.call_objects(spot::PRESS, "the confirm button");
            }
            return false;
        };
        self.room.talked.insert(number);
        info!(
            "game frame {}: the confirm button starts a talk, script {number} takes it",
            self.room.game_frame.saturating_sub(1)
        );
        true
    }

    fn call_objects(&mut self, called: i32, by: &str) {
        let game_frame = self.room.game_frame.saturating_sub(1);
        for (n, crossed) in self.room.crossings.iter_mut().enumerate() {
            if crossed.crossing.press() {
                info!("game frame {game_frame}: {by} tells gate {n} state 0");
            }
        }
        for (n, passed) in self.room.passings.iter_mut().enumerate() {
            if passed.passing.press() {
                info!(
                    "game frame {game_frame}: {by} tells bank {n} of the \
                     passage state 0"
                );
            }
        }
        if let Some(locked) = &mut self.room.lock
            && locked.locking.press()
        {
            info!(
                "game frame {game_frame}: {by} tells the lock at \
                 {:#010x} state 0",
                locked.lock.record
            );
        }
        for (at, _, swing) in &mut self.room.doors {
            if swing.press() {
                info!(
                    "game frame {game_frame}: {by} tells the door at \
                     {at:#010x} state 0"
                );
            }
        }
        if let Some((finds, searched)) = &mut self.room.finds
            && searched.search.press()
        {
            info!(
                "game frame {game_frame}: {by} tells the thing to be \
                 searched at {:#010x} state 0",
                finds.at
            );
        }
        if let Some(carded) = &mut self.room.card
            && carded.floating.press()
        {
            info!(
                "game frame {game_frame}: {by} tells the object of the \
                 memory card at {:#010x} state 0",
                carded.card.record
            );
        }
        for (spot, _, task) in &mut self.room.spots {
            if task
                .as_mut()
                .is_some_and(|task| task.call(spot.thing, called))
            {
                info!(
                    "game frame {game_frame}: {by} tells the spot at \
                     {:#010x} state 0",
                    spot.record
                );
            }
        }
        for (found, pressed) in &mut self.room.switches {
            if pressed.press() {
                info!(
                    "game frame {game_frame}: {by} tells the switch at \
                     {:#010x} state 0",
                    found.record
                );
            }
        }
    }

    /// The things of the story a marked object of the room waits for (`0x800A5E10`):
    /// the menu lets those be used here.
    pub fn waited_things(&self) -> Vec<i16> {
        (self.room.spots.iter())
            .filter(|(spot, _, task)| {
                (task.as_ref()).is_some_and(|task| task.waits_for(spot.thing, spot.thing as i32))
            })
            .filter_map(|(spot, ..)| i16::try_from(spot.thing).ok())
            .collect()
    }

    /// The menu has used thing `thing` of the story (`0x800A91FC`, which calls
    /// `0x800A5BD0(thing)`). The first half of that call, a script that waits with
    /// `0x29` and `a` 0 for the thing, is not built: no listed script waits so.
    pub fn use_story_thing(&mut self, thing: i16) {
        if self.room.selector < 0 && self.player_is_free() {
            let by = format!("the use of thing {thing} of the story");
            self.call_objects(i32::from(thing), &by);
        }
    }

    pub fn resting_exits(&self) -> Vec<u32> {
        (self.room.exits.iter())
            .filter(|(_, told)| !leave::runs(**told))
            .map(|(record, _)| *record)
            .collect()
    }

    /// The fade of the picture as the scripts have left it.
    pub fn fade(&self) -> &Fade {
        &self.room.fade
    }

    /// Whether the scripts have left the painted background drawn.
    pub fn background(&self) -> bool {
        self.room.background
    }

    /// The game frame of the room, while the scripts have the picture of the frame before
    /// laid into every frame.
    pub fn before(&self) -> Option<u32> {
        self.room.before.then_some(self.room.game_frame)
    }

    /// The draw area as the scripts have left it.
    pub fn area(&self) -> DrawArea {
        self.room.area
    }

    pub fn cover(&self) -> [bars::Rect; 2] {
        (self.room.slides.as_ref()).map_or([(0, 0, 0, 0); 2], |(.., drawn)| *drawn)
    }

    pub fn played(&self) -> Option<[(played::Part, i32); 2]> {
        let (_, found, _, frames) = self.room.plays.as_ref()?;
        Some([(found.parts[0], frames[0]), (found.parts[1], frames[1])])
    }

    /// What the camera task made of the last game frame; none before the first one and
    /// when the thing it follows is not in the room.
    pub fn camera(&self) -> Option<CameraPoints> {
        self.room.camera
    }

    /// The show records that the scripts handed the spawner since the last call: their
    /// tasks are made in that game frame and run from it on.
    pub fn take_shows(&mut self) -> Vec<Show> {
        std::mem::take(&mut self.room.shows)
    }

    pub fn show_mover(&self, routine: u32) -> Option<ShowMover> {
        let package = ScenePackage::parse(&self.file).ok()?;
        show_mover::read(&package, routine)
    }

    /// Whether a word of the package that picks a mover's bone is not 0 now: what the
    /// routine that flips it has left, or what the package brings.
    pub fn show_word(&self, word: u32) -> bool {
        self.room.flipped.get(&word).copied().unwrap_or_else(|| {
            (ScenePackage::parse(&self.file).ok())
                .and_then(|package| package.bytes_at(word, 4).map(|bytes| bytes != [0; 4]))
                .unwrap_or(false)
        })
    }

    pub fn take_cleared(&mut self) -> bool {
        std::mem::take(&mut self.room.cleared)
    }

    pub fn ride_camera(&self) -> bool {
        self.room.ride_camera.is_some()
    }

    /// How far the room's picture is moved down in this game frame by a shake a script
    /// asked for: the byte the scroll routine adds to the vertical offset.
    pub fn shaken(&self) -> i32 {
        self.room.shaken
    }

    /// The zoom as the camera task of the last game frame left it.
    pub fn zoom(&self) -> i32 {
        self.room.zoom.value
    }

    pub fn take_view(&mut self) -> Option<usize> {
        self.room.view.take()
    }

    /// Whether a script has handed the package's movie routine a movie that the room view
    /// has not taken yet: the game frame ended with that script.
    pub fn asks_movie(&self) -> bool {
        self.room.movie.is_some()
    }

    pub fn take_movie(&mut self) -> Option<u32> {
        self.room.movie.take()
    }

    pub fn take_map(&mut self) -> bool {
        std::mem::take(&mut self.room.map)
    }

    pub fn take_screen(&mut self) -> Option<(u32, u32)> {
        let asked = (self.room.screen.as_mut()).filter(|asked| asked.state == Asking::Asked)?;
        asked.state = Asking::Up;
        Some((asked.kind, asked.argument))
    }

    pub fn take_card(&mut self) -> Option<[u8; 24]> {
        if self.room.selector >= 0 || self.room.fight.is_some() {
            return None;
        }
        let screen_asked =
            (self.room.screen.as_ref()).is_some_and(|asked| asked.state == Asking::Asked);
        if self.room.leaving.is_some() || screen_asked {
            return None;
        }
        let carded = self.room.card.as_mut().filter(|carded| carded.asked)?;
        carded.asked = false;
        Some(carded.text)
    }

    pub fn take_end_card(&mut self) -> Option<[u8; 24]> {
        if self.room.selector >= 0 || self.room.fight.is_some() {
            return None;
        }
        let screen_asked =
            (self.room.screen.as_ref()).is_some_and(|asked| asked.state == Asking::Asked);
        if self.room.leaving.is_some() || screen_asked {
            return None;
        }
        self.room.end_card.take()
    }

    /// The screen a script asked for has left and the field has its mode 1 again: the
    /// script goes on in the next game frame.
    pub fn screen_left(&mut self) {
        self.room.screen_sleeps = false;
        if let Some(asked) = &mut self.room.screen {
            (asked.state, asked.answer) = (Asking::Left, 0);
        }
    }

    /// The screen of file 44 answers the script and its task sleeps (`0x800BDF50`:
    /// 1 behind a merge, 2 behind a spin): the field has its mode 1 again and the
    /// script goes on with the answer as its choice in the next game frame. A `0x59`
    /// of kind 3 brings the screen back.
    pub fn screen_answered(&mut self, answer: i16) {
        self.room.screen_sleeps = true;
        if let Some(asked) = &mut self.room.screen {
            (asked.state, asked.answer) = (Asking::Left, answer.into());
        }
    }

    pub fn take_results(&mut self) -> Option<Results> {
        self.room.results.take()
    }

    /// The results screen could not be put up: what it would give is given at once.
    pub fn results_without_a_screen(&mut self, results: Results) {
        Running::give_results(&mut self.room, results);
    }

    /// Nobody is at the pad (`--confirms`): the confirm goes down by itself.
    pub fn nobody_at_the_pad(&self) -> bool {
        self.room.confirms
    }

    pub fn voices_ahead(&mut self) {
        for (_, heard) in &self.room.music[self.room.voice_seen.min(self.room.music.len())..] {
            match heard {
                Heard::Line(line) => {
                    self.room.voice_ahead = Some((line.seconds() * 25.0).ceil() as u32);
                }
                Heard::LineEnds => self.room.voice_ahead = None,
                _ => {}
            }
        }
        self.room.voice_seen = self.room.music.len();
        let plays = match &mut self.room.voice_ahead {
            Some(left) if *left > 0 => {
                *left -= 1;
                true
            }
            _ => false,
        };
        if !plays {
            self.room.voice_ahead = None;
        }
        self.hear_voices(!plays, true);
    }

    pub fn hear_voices(&mut self, silent: bool, none_asked: bool) {
        (self.room.voice_silent, self.room.voice_none_asked) = (silent, none_asked);
        // The scripts' waiter asks it too, while bit 3 of the setting word is clear.
        for (task, _) in &mut self.tasks {
            task.hear(!silent);
        }
    }

    pub fn take_music(&mut self) -> Vec<Heard> {
        let now = self.room.game_frame;
        self.room.voice_seen = 0;
        let asked = std::mem::take(&mut self.room.music).into_iter();
        asked
            .filter(|(at, heard)| {
                !matches!(
                    heard,
                    Heard::Sound(_) | Heard::Kept { .. } | Heard::Got(_) | Heard::Line(_)
                ) || at + 1 >= now
            })
            .map(|(_, heard)| heard)
            .collect()
    }

    pub fn leave_forest(&mut self, trees: [i32; leave_forest::VIEWS], from: leave_forest::From) {
        self.room.leaving = Some((Leaving::default(), trees, from));
    }

    pub fn lies_in(&mut self, _area: usize, place: usize) {
        self.room.forest_place = i16::try_from(place).ok();
    }

    pub fn take_room(&mut self) -> Option<u32> {
        self.room.left |= self.room.leaves.is_some();
        self.room.leaves.take()
    }

    pub fn take_chapter(&mut self) -> Option<u32> {
        self.room.left |= self.room.chapter.is_some();
        self.room.chapter.take()
    }

    /// The slot a script has sent the game to since the last call (`0x52`): area, place,
    /// slot, and the address of an arrival place when the command names one. The scripts
    /// stand still from the game frame of that command on.
    pub fn take_slot(&mut self) -> Option<(i32, u16, u16, Option<u32>)> {
        self.room.left |= self.room.slot.is_some();
        self.room.slot.take()
    }

    /// What the generator of the corridor of the second round has asked of the room
    /// loader since the last call. The scripts stand still from then on.
    pub fn take_corridor(&mut self) -> Option<CorridorAsks> {
        self.room.left |= self.room.corridor_asks.is_some();
        self.room.corridor_asks.take()
    }

    pub fn take_again(&mut self) -> bool {
        self.room.left |= self.room.again;
        std::mem::take(&mut self.room.again)
    }

    pub fn ending(&self) -> Option<(usize, u8)> {
        let (file, since) = self.room.ending?;
        Some((
            file,
            ending::colour(self.room.game_frame.saturating_sub(since)),
        ))
    }

    /// What the game carries on, as the scripts have left it, for the room that comes next.
    pub fn carried(&self) -> &Carried {
        &self.room.carried
    }

    /// The game's setting word as the room's scripts, windows and fights read it.
    pub fn setting(&self) -> u32 {
        self.room.setting
    }

    /// The app hands the room the setting word (the options screen has changed it, or
    /// the room was made behind a change).
    pub fn set_setting(&mut self, word: u32) {
        self.room.setting = word;
    }

    /// The player's menu has changed what the player keeps and has on, and his own
    /// record when an item was used.
    pub fn kept(
        &mut self,
        keeping: items::Keeping,
        player: Option<Record>,
        worn: items::Worn,
        creatures: Option<Vec<Record>>,
    ) {
        self.room.carried.keeping = keeping;
        self.room.carried.worn = worn;
        if player.is_some() {
            self.room.carried.player = player;
        }
        // The creatures he carries, in the order of the menu's boxes.
        let carried = self.room.carried.creatures.carried_mut();
        for (record, left) in carried.zip(creatures.unwrap_or_default()) {
            *record = left;
        }
    }

    pub fn listed(&mut self, game: &Carried) {
        let carried = &mut self.room.carried;
        carried.creatures = game.creatures.clone();
        carried.caught = game.caught.clone();
        // The silks of the creatures that were spun are among his items.
        carried.keeping = game.keeping.clone();
        if game.player.is_some() {
            carried.player = game.player.clone();
        }
        info!(
            "game frame {}: the screen of the caught has left: he carries {} creatures \
             and keeps {}",
            self.room.game_frame,
            carried.creatures.count(creatures::state::CARRIED),
            carried.creatures.count(creatures::state::KEPT)
        );
    }

    /// The count of the two items as the player's menu has left it (`0x80011B24`).
    pub fn count_kept(&mut self, count: ItemCount) {
        if count != self.room.carried.item_count {
            info!(
                "game frame {}: an item of the menu starts the count of the roaming \
                 creatures: value {} for {} passes",
                self.room.game_frame,
                count.value(),
                count.left()
            );
        }
        self.room.carried.item_count = count;
    }

    /// One pass of the count's task (`0x80011AF8`), and the value its getter answers.
    pub fn pass_item_count(&mut self) -> i32 {
        let count = &mut self.room.carried.item_count;
        let before = count.value();
        count.pass();
        if count.value() != before {
            info!(
                "game frame {}: the count of the roaming creatures is through",
                self.room.game_frame
            );
        }
        count.value()
    }

    /// The value of the count of the two items (`0x80011B6C`).
    pub fn item_count(&self) -> i32 {
        self.room.carried.item_count.value()
    }

    /// The leaves of the room's doors as they stand in this game frame: the mesh object
    /// of the room's model block and where it is drawn.
    pub fn door_leaves(&self) -> Vec<(usize, Placement)> {
        (self.room.doors.iter())
            .flat_map(|(_, door, swing)| {
                (door.leaves.iter())
                    .map(|leaf| (leaf.object, leaf.stands(swing.counter, door.frames)))
            })
            .collect()
    }

    /// The mesh objects of the room's model block that the thing to be searched is
    /// drawn with in this game frame, and where: none when it is not drawn.
    pub fn found_meshes(&self) -> Vec<(usize, Placement)> {
        (self.room.finds.iter())
            .filter(|(finds, searched)| {
                searched.search.drawn && finds.find.look == search::LOOK_DRAWN
            })
            .flat_map(|(finds, searched)| finds.find.stands(searched.search.amount))
            .collect()
    }

    /// What the object of the memory card drew in the last game frame: its mesh block,
    /// the place of its record and the two mesh objects. None when the room has no such
    /// object or its routine did not run.
    pub fn card_meshes(&self) -> Option<(&MeshBlock, [i32; 3], [card::Drawn; 2])> {
        let carded = self.room.card.as_ref()?;
        Some((carded.block.as_ref()?, carded.card.place, carded.drawn?))
    }

    /// The places at which the object of room 8 drew its sprite in the last game frame,
    /// with the count each place is seen at (`Spot::frame`).
    /// The sparks the task of the light behind a lost fight drew in the last game frame.
    pub fn sparks(&self) -> &[halo::Spark] {
        &self.room.sparks
    }

    pub fn scattered(&self) -> Option<(&[scatter::Spot], i32)> {
        let scattered = self.room.scatter.as_ref()?;
        let (list, count) = scattered.drawn?;
        Some((&scattered.places.lists[list], count))
    }

    /// Where the object of the sky stands and the count of its travel, when a script
    /// has started it and its routine drew in the last game frame.
    pub fn sky(&self) -> Option<(Placement, i32)> {
        let (_, course, sky, drawn) = self.room.sky.as_ref()?;
        drawn.then(|| (course.stands(sky.count), sky.count))
    }

    /// Game frames the tasks have run.
    pub fn game_frames(&self) -> u32 {
        self.room.game_frame
    }
}

/// Three words at an address of the package: a point of the room.
/// One pass of a message window over the group of its text, with the player's `name`
/// as a text; what a log line says of it when its state changed or it sounded.
fn window_pass(
    window: &mut Window,
    said: &mut Said,
    (groups, font): (&[Group], &Font),
    pad: &Pad,
    name: &[u16],
    setting: u32,
    voice_waits: bool,
) -> Option<String> {
    let group = groups.get(said.group)?;
    let first_line = window.layout().first_line;
    let mut read = |page: usize, letters: u32| match group.pages.get(page) {
        Some(page) => {
            let shown = message::shown(font, page, first_line, letters, name);
            window::Read {
                whole: shown.whole,
                cue: shown.cues.iter().copied().max(),
            }
        }
        None => window::Read {
            whole: true,
            cue: None,
        },
    };
    let before = (window.wait, window.boxes, window.on_screen);
    let pass = window.step(pad, setting, voice_waits, &mut read);
    let after = (window.wait, window.boxes, window.on_screen);
    let line = (before != after || !pass.sounds.is_empty()).then(|| {
        format!(
            "(thing {}, group {}): {:?}, the boxes {:?}, page {} of {}, sounds {:02X?}",
            said.thing,
            said.group,
            after.0,
            after.1,
            after.2 + 1,
            group.pages.len(),
            pass.sounds
        )
    });
    said.pass = pass;
    said.passes += 1;
    line
}

fn words_at(package: &ScenePackage, address: u32) -> Option<[i32; 3]> {
    let at = address.checked_sub(PACKAGE_BASE)? as usize;
    let bytes = package.image().get(at..at + 12)?;
    let word = |n: usize| i32::from_le_bytes(bytes[4 * n..4 * n + 4].try_into().expect("4"));
    Some([word(0), word(1), word(2)])
}

/// The point a script's task notes of a thing it takes besides its place (`0x800BBE40`):
/// 100 ahead of the thing in the direction it faces, with the angle where the height
/// would be. A thing that is told to turn to itself turns to this point.
fn ahead((place, angle): Placed) -> [i32; 3] {
    let turn = i32::from(angle);
    [
        place[0].wrapping_add(sine(turn) * AHEAD / 4096),
        turn,
        place[2].wrapping_add(cosine(turn) * AHEAD / 4096),
    ]
}

/// How far ahead of a thing the point of [`ahead`] lies.
const AHEAD: i32 = 100;

/// The room for one pass of a task: what [`Engine`] and [`script::Things`] work on.
struct Running<'a> {
    room: &'a mut Room,
    actors: &'a mut [Standing],
    player: Option<&'a mut Walker>,
    package: &'a ScenePackage<'a>,
    /// The archive file of the package.
    file: usize,
    camera: &'a ViewCamera,
    /// The projection distance of the view's header, without the zoom.
    distance: i16,
    /// The number of the view on screen.
    view: usize,
}

impl Running<'_> {
    fn finds(&self) -> Option<&(Finds, Searched)> {
        self.room.finds.as_ref()
    }

    /// Whether the script of that number was started by the object for a player who
    /// came from the place map.
    fn arrives(&self, script: i32) -> bool {
        (self.room.arriving.as_ref()).is_some_and(|arrives| arrives.script == script)
    }

    /// The trigger of the place map that started the task of a number.
    fn map_trigger(&self, number: i32) -> Option<MapTrigger> {
        (self.room.map_triggers.iter())
            .find(|(.., script)| *script == Some(number))
            .map(|(trigger, ..)| *trigger)
    }

    fn actor(&mut self, thing: usize) -> Option<&mut Standing> {
        self.actors.get_mut(thing.checked_sub(1)?)
    }

    /// Where a thing stands.
    fn placed(&mut self, thing: usize) -> Option<Placed> {
        match thing {
            0 => (self.player.as_ref()).map(|walker| (walker.player.position, walker.player.angle)),
            thing => (self.actor(thing)).map(|actor| (actor.mover.position, actor.mover.turn)),
        }
    }

    /// The first thing of a mask that the player can talk to (`0x62`): near, or with a
    /// region the player in that trigger region, and faced.
    fn talks_to(&mut self, region: Option<u32>, things: u32) -> Option<usize> {
        let walker = self.player.as_ref()?;
        let (place, angle) = (walker.player.position, walker.player.angle);
        let arc = walker.body.arc.clone();
        let in_region = region.map(|region| {
            (self.room.triggers.get(region as usize))
                .is_some_and(|region| region.triangle_at(place).is_some())
        });
        (1..=self.actors.len()).find(|&thing| {
            let named = thing < 32 && things >> thing & 1 != 0;
            let at = self.actors[thing - 1].mover.position;
            named
                && in_region.unwrap_or_else(|| player::talk_near(place, at))
                && player::talk_faced(&arc, place, angle, at)
        })
    }

    /// The three words and the angle at an address of the package (`0x13`).
    fn placed_at(&self, address: u32) -> Option<Placed> {
        let at = address.checked_sub(PACKAGE_BASE)? as usize;
        let bytes = self.package.image().get(at..at + 16)?;
        let word = |n: usize| i32::from_le_bytes(bytes[4 * n..4 * n + 4].try_into().expect("4"));
        Some(([word(0), word(1), word(2)], word(3) as i16))
    }

    fn notice(
        &mut self,
        task: &mut Task,
        command: &scene::Command,
        asked: scene::NoticeCommand,
    ) -> Answer {
        let group = usize::try_from(command.c).ok();
        // What the thing to be searched that started the task has left for the notice
        // (+140 and +132 of its task).
        let found = (self.room.finds.as_ref())
            .filter(|(_, searched)| searched.script == Some(task.number))
            .map(|(finds, searched)| (finds.find, searched.search.handed, searched.search.used));
        // The lock that started the task has left the lines of its side and its thing.
        let locked = (self.room.lock.as_ref()).filter(|locked| locked.script == Some(task.number));
        let lines = match (command.code, found) {
            (0x3B, _) if command.c == u32::MAX && locked.is_some() => {
                locked.and_then(|locked| self.package.lines_at(locked.lines))
            }
            (0x3B, Some((_, search::Handed::Lines(address), _))) if command.c == u32::MAX => {
                self.package.lines_at(address)
            }
            // A text named by its address: the list of its lines in the package.
            (0x3B, _) if command.c > message::LAST_GROUP && command.c != u32::MAX => {
                self.package.lines_at(command.c)
            }
            (0x3B, _) => group
                .and_then(|group| self.room.groups.get(group))
                .and_then(|group| group.pages.first())
                .map(|page| {
                    let font = &self.room.font;
                    let cue = |line: &&Vec<u16>| font.codes(line).first() == Some(&CUE_0);
                    let skip = usize::from(page.first().as_ref().is_some_and(cue));
                    page[skip..].to_vec()
                }),
            _ => Some(Vec::new()),
        };
        let found = found.and_then(|(find, handed, used)| match command.code {
            0x3C if handed == search::Handed::Given => {
                Some(find.given.iter().flatten().copied().collect())
            }
            0x3E => Some(vec![scene::NoticeThing {
                kind: command.c as i32,
                number: used?,
                count: 1,
            }]),
            _ => None,
        });
        // The thing of the object the task belongs to (`0x3E` without a number): a gate,
        // a lock or a door has left its thing of the story there.
        let door = (self.room.door_owners.get(&task.number))
            .and_then(|record| self.room.doors.iter().find(|(at, ..)| at == record))
            .map(|(_, door, _)| door.thing);
        let owned = (self.room.owners.get(&task.number))
            .and_then(|&gate| self.room.gates.as_ref()?.gates.get(gate))
            .map(|(_, gate)| gate.thing)
            .or(locked.map(|locked| locked.lock.thing))
            .or(door)
            .filter(|_| command.code == 0x3E)
            .map(|thing| {
                vec![scene::NoticeThing {
                    kind: command.c as i32,
                    number: thing,
                    count: 1,
                }]
            });
        // The builder reads all this in the call that opens the notice. In the calls
        // behind it an object may have written something else where its lines were (a
        // thing to be searched that was told state 4 does so in the same game frame).
        let read = lines.zip(asked.things.or(owned).or(found));
        if read.is_none() && !self.room.noticed.contains_key(&task.number) {
            self.room.leave_out(command.code);
            task.next();
            return Answer::GoOn;
        }
        let (lines, listed) = read.unwrap_or_default();
        let mut opened = false;
        let setting = self.setting();
        let answer = task.notice(setting, || {
            opened = true;
            true
        });
        if opened {
            let mut things = [notice::Thing::NONE; notice::THINGS];
            for (thing, listed) in things.iter_mut().zip(&listed) {
                *thing = notice::Thing {
                    kind: listed.kind,
                    number: listed.number,
                    count: listed.count,
                };
            }
            info!(
                "game frame {}: script {} puts up a notice of way {} with {} things and {} lines",
                self.room.game_frame,
                task.number,
                asked.way,
                listed.len(),
                lines.len()
            );
            let noticed = Noticed {
                way: asked.way,
                things,
                lines,
            };
            self.room.noticed.insert(task.number, noticed);
            let music = notice::opening_music(asked.way).map(Heard::Got);
            (self.room.music).extend(music.map(|music| (self.room.game_frame, music)));
        }
        task.tell_confirm(self);
        if answer == Answer::GoOn {
            self.room.noticed.remove(&task.number);
            info!(
                "game frame {}: the notice of script {} is through",
                self.room.game_frame, task.number
            );
        }
        answer
    }

    /// The window of a message as the scripts' builder `0x800BBFFC` asks for it: at the
    /// bottom, or at the place the task holds (`0x35`), the picture on the left when the
    /// speaker is seen left of the thing it speaks to (`0x800BC78C`, with the places the
    /// things are drawn at).
    fn window(
        &self,
        (thing, side): (i32, u32),
        group: usize,
        (question, switched_off, held): (bool, u32, Option<window::Where>),
    ) -> Option<Window> {
        let group = self.room.groups.get(group)?;
        let place = |thing: usize| match thing {
            0 => self.player.as_ref().map(|walker| walker.body.place),
            thing => self.actors.get(thing - 1).map(|actor| actor.place),
        };
        // The matrix unit's screen place; behind the camera its side is all that counts.
        let seen = |place: [i32; 3]| match self.camera.project(place) {
            Some(at) => at[0].floor() as i32,
            None => self.camera.to_view(place)[0].floor() as i32,
        };
        let speaker = usize::try_from(thing).ok().and_then(place);
        let picture_left = match (speaker, place(side as usize)) {
            (Some(speaker), Some(other)) => window::picture_left(seen(speaker), seen(other)),
            _ => false,
        };
        let mut asked = window::Asked::said(
            question,
            self.room.setting,
            group.pages.len(),
            group.answers.len(),
            switched_off,
            picture_left,
        );
        if let Some(place) = held {
            asked.place = place;
        }
        // The answers' box is as wide as the widest answer (`0x8003D040`).
        let widest = match question {
            true => (group.answers.iter())
                .map(|answer| message::width(&self.room.font, answer, &[]))
                .max()
                .unwrap_or(0),
            false => 0,
        };
        Some(Window::open(asked, widest))
    }

    /// `0x13`: the place goes to the thing's position and the angle to its turn about the
    /// upright axis; nothing else of the thing is written.
    fn put(&mut self, thing: usize, (place, angle): Placed) {
        info!(
            "game frame {}: thing {thing} is put on {place:?} with angle {angle}",
            self.room.game_frame
        );
        match thing {
            0 => {
                if let Some(walker) = &mut self.player {
                    walker.player.arrive(place, angle);
                }
            }
            thing => {
                if let Some(actor) = self.actor(thing) {
                    (actor.mover.position, actor.mover.turn) = (place, angle);
                    (actor.place, actor.angle) = (place, angle);
                }
            }
        }
    }
}

struct Leaves<'a> {
    room: &'a mut Room,
    player: Option<&'a mut Walker>,
}

impl leave_forest::Field for Leaves<'_> {
    /// The three records the call goes through are not in code (HYPOTHESIS: the
    /// roaming creatures of the room, T091).
    fn open(&mut self) {}

    fn take(&mut self) {
        self.room.orders.take();
        if let Some(walker) = &mut self.player {
            walker.stand_again();
        }
    }

    fn read(&mut self, action: i16) {
        let read = (self.player.as_mut()).is_some_and(|walker| walker.read_action(action));
        if !read {
            warn!("the task that leaves the forest: action {action} of the player is not read");
        }
    }

    /// The build's loader takes no time.
    fn is_read(&mut self) -> bool {
        true
    }

    fn ready(&mut self) -> bool {
        self.room.orders.ready()
    }

    fn order(&mut self, action: i16) {
        let Some(walker) = &mut self.player else {
            return;
        };
        let order = (
            leave_forest::ORDER_KIND,
            action,
            0,
            leave_forest::ORDER_BITS,
        );
        if !(self.room.orders).give(&mut walker.player, &walker.moves, order) {
            warn!("the task that leaves the forest: the player cannot take action {action}");
        }
    }

    fn whiten(&mut self, frames: u16) {
        self.room.fade.whiten(frames);
    }

    fn at_rest(&mut self) -> bool {
        self.room.fade.at_rest()
    }

    fn give_back(&mut self, action: i16) {
        if let Some(walker) = &mut self.player {
            walker.give_back_action(action);
        }
    }

    fn free(&mut self) {
        if let Some(walker) = &mut self.player {
            (self.room.orders).give_back(&mut walker.player, &walker.moves);
        }
    }
}

impl script::Things for Running<'_> {
    fn free(&mut self, thing: usize) -> bool {
        match thing {
            0 => self.player.is_some() && self.room.orders.ready(),
            // No move runs and the actor has no target (flags 0x280000).
            thing => self.actor(thing).is_some_and(|actor| {
                actor.runner.free() && actor.mover.flags & mover::flag::TARGET == 0
            }),
        }
    }

    fn give(&mut self, thing: usize, order: &Order, told: bool) {
        info!(
            "game frame {}: thing {thing} is given an order of kind {} ({}, {}, bits {:#x})",
            self.room.game_frame, order.kind, order.value, order.second, order.bits
        );
        let (action, second) = (order.value as i16, order.second as i16);
        if thing == 0 {
            let Some(walker) = &mut self.player else {
                return;
            };
            let given = match (order.kind, self.room.paths.get(order.list as usize)) {
                (11, Some(path)) => {
                    let (player, path) = (&mut walker.player, path.clone());
                    (self.room.orders).give_path(player, &walker.moves, action, path)
                }
                (11, None) => {
                    warn!("the player: the room has no path {}", order.list);
                    false
                }
                (0..=2, _) => {
                    self.room.orders.give_place(order.place);
                    true
                }
                _ => {
                    let order = (order.kind, action, second, order.bits);
                    (self.room.orders).give(&mut walker.player, &walker.moves, order)
                }
            };
            if !given {
                warn!("the player cannot take the order: {order:?}");
            }
            return;
        }
        let ground = self.room.ground.clone();
        // A list of an order lies in the package (the executable's are the built-in ones).
        let list = (order.kind == 5)
            .then(|| command::list_in(&[(self.package.image(), PACKAGE_BASE)], order.list))
            .flatten();
        let built_in = &self.room.built_in;
        let Some(actor) = (thing.checked_sub(1)).and_then(|slot| self.actors.get_mut(slot)) else {
            return;
        };
        let switch = |flags: &mut u32, bit: u32| match order.value {
            0 => *flags &= !bit,
            _ => *flags |= bit,
        };
        match order.kind {
            0..=2 => match actor.moves {
                Some(moves) => {
                    let own = match order.kind {
                        2 => moves.turn,
                        kind => moves.go[kind as usize],
                    };
                    let action = if order.value == u32::MAX { own } else { action };
                    actor.mover.target = order.place;
                    actor.mover.turn_step = i32::from(moves.turn_step);
                    actor.mover.flags |= mover::flag::TARGET;
                    (actor.runner).request_target(action, order.kind == 2, told, built_in);
                    actor.pass();
                }
                None => warn!("thing {thing}: no actor record to go or turn by"),
            },
            5 => match list {
                Some(list) => {
                    actor.runner.request_list(list, told);
                    actor.pass();
                }
                None => warn!("thing {thing}: no command list at {:#010x}", order.list),
            },
            6 => actor.runner.hidden = order.value == 0,
            7 => {
                switch(&mut actor.mover.flags, mover::flag::WALKS);
                actor.ground = ground;
            }
            8 => switch(&mut actor.mover.flags, mover::flag::CARRIED),
            9..=11 => {
                if order.kind == 11 {
                    match self.room.paths.get(order.list as usize) {
                        Some(path) => actor.mover.set_path(path.clone()),
                        None => warn!("thing {thing}: the room has no path {}", order.list),
                    }
                }
                let then = (order.kind != 9).then_some(second);
                // The game leaves it to the scene to have read what it orders.
                for action in [Some(action), then].into_iter().flatten() {
                    let animation = usize::try_from(action)
                        .ok()
                        .and_then(|action| actor.actions.get(action)?.animation);
                    if animation.is_some_and(|number| !actor.animations.contains_key(&number)) {
                        warn!(
                            "thing {thing}: animation {animation:?} of action {action} is not \
                             in memory"
                        );
                    }
                }
                let request = match order.kind {
                    11 => Request::of_path_order(action, then, order.bits, told),
                    _ => Request::of_order(action, then, order.bits, told),
                };
                actor.runner.request(&request, built_in);
                // The request runs a pass over its list at once, up to the hold.
                actor.pass();
            }
            kind => warn!("thing {thing}: an order of kind {kind} is not in code"),
        }
    }

    fn ready(&mut self, thing: usize) -> bool {
        match thing {
            0 => self.room.orders.ready(),
            thing => self.actor(thing).is_some_and(|actor| actor.runner.ready()),
        }
    }

    fn go_on(&mut self, thing: usize, order: &Order) -> bool {
        if thing == 0 {
            let Some(walker) = &mut self.player else {
                return false;
            };
            let order = (order.kind, order.value, order.bits);
            return self.room.orders.go_on(&mut walker.player, order);
        }
        let Some(actor) = self.actor(thing) else {
            return false;
        };
        actor.runner.goes_on();
        actor.runner.take_over()
    }
}

impl Engine for Running<'_> {
    fn selector(&mut self) -> &mut i32 {
        &mut self.room.selector
    }

    fn confirm(&mut self) -> bool {
        self.room.confirm
    }

    fn setting(&mut self) -> u32 {
        self.room.setting
    }

    fn may_open(&mut self) -> bool {
        // The walking word `0x800B1090` is 1.
        self.room.leaving.is_none()
    }

    fn opened(&mut self) {
        self.room.roamers_held = true;
    }

    fn closed(&mut self) {
        (self.room.roamers_held, self.room.roamers_let_go) = (false, true);
    }

    fn take(&mut self, thing: usize) -> bool {
        let placed = self.placed(thing);
        if let Some(noted) = self.room.noted.get_mut(thing) {
            *noted = placed;
        }
        if thing == 0 {
            self.room.orders.take();
        }
        true
    }

    fn give_back(&mut self, thing: usize) {
        if let (0, Some(walker)) = (thing, &mut self.player) {
            (self.room.orders).give_back(&mut walker.player, &walker.moves);
        }
    }

    fn settle(&mut self, _things: u32) -> bool {
        true
    }

    fn story_variable(&mut self, number: u32) -> i32 {
        self.room.carried.variable(number)
    }

    fn set_story_variable(&mut self, number: u32, value: i32) {
        self.room.carried.set_variable(number, value);
    }

    fn random(&mut self) -> i32 {
        self.room.drawn += 1;
        self.room.drawn
    }

    fn confirmed(&mut self, thing: usize) {
        info!(
            "game frame {}: thing {thing} is told of the confirm",
            self.room.game_frame
        );
        match thing {
            0 => {
                if let Some(walker) = &mut self.player {
                    walker.player.confirmed();
                }
            }
            thing => {
                if let Some(actor) = self.actor(thing) {
                    actor.runner.confirmed();
                }
            }
        }
    }

    fn run(&mut self, task: &mut Task, command: scene::Command) -> Answer {
        let typed = Scene::of(&command);
        // Whether the thing to be searched started this task.
        let number = task.number;
        let owns = |found: &(Finds, Searched)| found.1.script == Some(number);
        if let Some((things, order, told)) = typed.and_then(|typed| typed.order()) {
            return task.order(self, things, &order, told, true);
        }
        let place = match typed {
            Some(Scene::GoTo { place, .. } | Scene::TurnTo { place, .. }) => {
                Some(words_at(self.package, place))
            }
            Some(Scene::TurnToThing { thing, other, .. }) => {
                let it = usize::try_from(other).unwrap_or(usize::MAX);
                Some(match thing == other {
                    true => (self.room.noted.get(it).copied().flatten()).map(ahead),
                    false => self.placed(it).map(|(place, _)| place),
                })
            }
            _ => None,
        };
        match (typed, place) {
            (Some(typed), Some(Some(place))) => {
                if let Some((things, order, told)) = typed.order_to(place) {
                    return task.order(self, things, &order, told, true);
                }
            }
            (_, Some(None)) => warn!(
                "{:#010x}: command {:#04x} names no place: left out",
                command.address, command.code
            ),
            _ => {}
        }
        if let Some(asked) = self.package.notice(&command) {
            return self.notice(task, &command, asked);
        }
        if let Some(asked) = command.keeping() {
            match asked {
                KeepingCommand::WaitForFlag { flag } if !self.room.carried.keeping.flag(flag) => {
                    return Answer::Wait;
                }
                KeepingCommand::WaitForFlag { .. } => {}
                // Not read: it keeps a value in the task and waits on the task's flags.
                KeepingCommand::StoryThingUnread { .. } => {
                    self.room.leave_out(command.code);
                }
                _ => {
                    let choice = asked.run(&mut self.room.carried.keeping, self.room.carried.worn);
                    if let Some(choice) = choice {
                        task.choice = choice as i32;
                    }
                    info!(
                        "game frame {}: script {} runs {asked:?} (choice {choice:?}); the player has {} money",
                        self.room.game_frame,
                        task.number,
                        self.room.carried.keeping.money()
                    );
                }
            }
            task.next();
            return Answer::GoOn;
        }
        if let Some(asked) = command.room_bits() {
            if let RoomBits::WaitFor { bits } = asked
                && !RoomBits::all_set(self.room.bits, bits)
            {
                return Answer::Wait;
            }
            let choice = asked.run(&mut self.room.bits);
            if let Some(choice) = choice {
                task.choice = choice as i32;
            }
            info!(
                "game frame {}: script {} runs {asked:?} (choice {choice:?}); the bits of the room are {:#x}",
                self.room.game_frame, task.number, self.room.bits
            );
            task.next();
            return Answer::GoOn;
        }
        if let Some(asked) = command.location()
            && self.room.carried.run(asked)
        {
            info!(
                "game frame {}: script {} runs {asked:?}; the state word is {}, the variant table has {} entries",
                self.room.game_frame,
                task.number,
                self.room.carried.word,
                self.room.carried.variants.entries().len()
            );
            task.next();
            return Answer::GoOn;
        }
        if let Some(
            asked @ (LocationCommand::StartChapter { .. } | LocationCommand::GoToSlot { .. }),
        ) = command.location()
        {
            let left_out: Vec<String> = (self.room.left_out.iter())
                .map(|(code, times)| format!("{code:#04x} {times} times"))
                .collect();
            info!(
                "game frame {}: script {} runs {asked:?}; commands of the room that went on \
                 without being run: {}",
                self.room.game_frame,
                task.number,
                left_out.join(", ")
            );
            match asked {
                LocationCommand::StartChapter { chapter } => {
                    self.room.carried.chapter = chapter as usize;
                    self.room.chapter = Some(chapter);
                }
                LocationCommand::GoToSlot {
                    area,
                    place,
                    slot,
                    arrival,
                } => self.room.slot = Some((area, place, slot, arrival)),
                _ => {}
            }
            self.room.busy = true;
            task.next();
            return Answer::GoOn;
        }
        match typed {
            Some(Scene::Move { things, .. }) => {
                let order = Order {
                    kind: 4,
                    ..Order::default()
                };
                return task.order_and_wait(self, things, &order);
            }
            Some(Scene::Put { thing, place }) => {
                let thing = usize::try_from(thing).unwrap_or(usize::MAX);
                let placed = match place {
                    Some(address) => self.placed_at(address),
                    None => self.room.noted.get(thing).copied().flatten(),
                };
                match placed {
                    Some(placed) => self.put(thing, placed),
                    None => warn!(
                        "{:#010x}: no place to put thing {thing} on",
                        command.address
                    ),
                }
            }
            Some(Scene::DrawArea(area)) => {
                let at = |address: u32| {
                    let at = address.checked_sub(PACKAGE_BASE)? as usize;
                    DrawArea::at(self.package.image().get(at..)?)
                };
                match area {
                    Area::Whole => self.room.area = DrawArea::WHOLE,
                    Area::Bars => self.room.area = DrawArea::BARS,
                    Area::Kept => {}
                    Area::At(address) => match at(address) {
                        Some(area) => self.room.area = area,
                        None => warn!("{:#010x}: no draw area there", command.address),
                    },
                }
                info!(
                    "game frame {}: the draw area is {:?}",
                    self.room.game_frame, self.room.area
                );
            }
            Some(Scene::GlideTo { point, frames }) => match words_at(self.package, point) {
                Some([x, y, z]) => {
                    info!(
                        "game frame {}: the camera glides to {:?} over {frames} game frames",
                        self.room.game_frame,
                        [x, y, z]
                    );
                    // The camera's routines raise the target again when they look at it.
                    self.room.glide.to([x, y + glide::RAISED, z], frames as i16);
                }
                None => warn!("{:#010x}: no point to glide to", command.address),
            },
            Some(Scene::GlideToThing { thing, frames }) => {
                let placed = usize::try_from(thing)
                    .ok()
                    .and_then(|thing| self.placed(thing));
                match placed {
                    Some((place, _)) => {
                        info!(
                            "game frame {}: the camera glides to thing {thing} over {frames} \
                             game frames",
                            self.room.game_frame
                        );
                        self.room.glide.to(place, frames as i16);
                    }
                    None => warn!("{:#010x}: no thing {thing} to glide to", command.address),
                }
            }
            Some(Scene::GlideBack { frames }) => {
                info!(
                    "game frame {}: the camera glides back over {frames} game frames",
                    self.room.game_frame
                );
                self.room.glide.back(frames as i16);
            }
            Some(Scene::AwaitGlide { there }) if !self.room.glide.arrived(there) => {
                return Answer::Wait;
            }
            Some(Scene::AwaitGlide { .. }) => {}
            Some(Scene::Zoom { value, frames }) => {
                let at_once = self.room.zoom.order(value, frames as i32);
                // A zoom at once is followed by a step of the camera task: the zoom's
                // own step and one more of the glide's share.
                if at_once {
                    let depth = self.room.depth.unwrap_or(NO_DEPTH);
                    self.room.zoom.step(self.distance, depth);
                    self.room.glide.step();
                }
                info!(
                    "game frame {}: the zoom goes to {value} of 4096{}",
                    self.room.game_frame,
                    match at_once {
                        true => " at once".to_string(),
                        false => format!(" over {frames} game frames"),
                    }
                );
            }
            Some(Scene::Follow(followed)) => {
                info!(
                    "game frame {}: the camera follows {followed:?}",
                    self.room.game_frame
                );
                self.room.followed = followed;
            }
            // The read-ahead is refused while another one waits to be shown. The console
            // reads the view from the disc meanwhile; here every view is in memory.
            Some(Scene::ReadView { .. }) if self.room.read_ahead.is_some() => {
                return Answer::Wait;
            }
            Some(Scene::ReadView { view }) => {
                info!(
                    "game frame {}: view {view} is read ahead",
                    self.room.game_frame
                );
                self.room.read_ahead = Some(view);
            }
            // The view that was read ahead comes at once, without a hold. Any other is
            // only written as the wanted one, and the command is tried again.
            Some(Scene::ShowView { view }) if self.room.read_ahead == Some(view) => {
                info!(
                    "game frame {}: script {} shows view {view}, which was read ahead, in \
                     place of view {}",
                    self.room.game_frame, task.number, self.view
                );
                self.room.read_ahead = None;
                self.room.view = Some(view as usize);
            }
            Some(Scene::ShowView { view }) if self.room.read_ahead.is_none() => {
                self.room.view = Some(view as usize);
                return Answer::Wait;
            }
            Some(Scene::ShowView { .. }) => return Answer::Wait,
            // The wanted view is written, and the script waits for the loader.
            Some(Scene::AwaitView { view }) => {
                self.room.view = Some(view as usize);
                if self.view != view as usize {
                    return Answer::Wait;
                }
                info!(
                    "game frame {}: view {view}, which script {} wants, is the loaded one",
                    self.room.game_frame, task.number
                );
            }
            // A thing in a trigger region of the room; a region the room does not have
            // holds nothing.
            Some(Scene::AwaitPlace { thing, region }) => {
                let placed = usize::try_from(thing)
                    .ok()
                    .and_then(|thing| self.placed(thing));
                let inside = (placed.zip(self.room.triggers.get(region as usize)))
                    .is_some_and(|((place, _), region)| region.triangle_at(place).is_some());
                if !inside {
                    return Answer::Wait;
                }
                info!(
                    "game frame {}: thing {thing} stands in trigger region {region}, script \
                     {} goes on",
                    self.room.game_frame, task.number
                );
            }
            // A talk: the handler does nothing while the selector names another task,
            // goes on when the pad routine has given the task the press, and otherwise
            // says anew whether there is a thing the player can talk to.
            Some(Scene::AwaitTalk { region, things }) => {
                let selector = self.room.selector;
                if selector >= 0 && selector != task.number {
                    return Answer::Wait;
                }
                let talked = self.room.talked.remove(&task.number);
                let could = self.room.can_talk.remove(&task.number);
                if !talked {
                    if let Some(thing) = self.talks_to(region, things) {
                        self.room.can_talk.insert(task.number);
                        if !could {
                            debug!(
                                "game frame {}: the player can talk to thing {thing} \
                                 (script {})",
                                self.room.game_frame, task.number
                            );
                        }
                    }
                    return Answer::Wait;
                }
                info!(
                    "game frame {}: the player talks, script {} goes on",
                    self.room.game_frame, task.number
                );
            }
            Some(Scene::Call(Call::Bright)) => self.room.fade.bright(),
            Some(Scene::Call(Call::Black)) => self.room.fade.black(),
            Some(Scene::Call(Call::FadeIn { frames })) => {
                self.room.fade.black();
                self.room.fade.fade_in(frames as u16);
            }
            Some(Scene::Call(Call::FadeOut { frames })) => self.room.fade.fade_out(frames as u16),
            Some(Scene::Call(Call::White { frames })) => {
                self.room.fade.whiten(frames as u16);
                info!(
                    "game frame {}: a white grows over the picture in {frames} game frames",
                    self.room.game_frame
                );
            }
            Some(Scene::Call(Call::WhiteGoes { frames })) => {
                self.room.fade.unwhiten(frames as u16);
                info!(
                    "game frame {}: the white over the picture goes in {frames} game frames",
                    self.room.game_frame
                );
            }
            Some(Scene::Call(Call::Music(asked))) => {
                debug!(
                    "game frame {}: script {} asks the music for {asked:?}",
                    self.room.game_frame, task.number
                );
                (self.room.music).push((self.room.game_frame, Heard::Music(asked)));
            }
            // Call 13: a sound by its number (`0x800314F0`).
            Some(Scene::Call(Call::Other { number: 13, b, .. })) => {
                (self.room.music).push((self.room.game_frame, Heard::Sound(b)));
            }
            // Calls 14 and 15: a sound whose voice is kept in the word at `c`
            // (`0x80031550`), and the end of that voice (`0x800315D0`).
            Some(Scene::Call(Call::Other { number: 14, b, c })) => {
                let kept = Heard::Kept { at: c, sound: b };
                (self.room.music).push((self.room.game_frame, kept));
            }
            Some(Scene::Call(Call::Other { number: 15, c, .. })) => {
                (self.room.music).push((self.room.game_frame, Heard::Ended { at: c }));
            }
            Some(Scene::Call(Call::Other {
                number: voice::CALL_LINE,
                c,
                ..
            })) => match voice::line_at(self.package, c) {
                Some(line) => (self.room.music).push((self.room.game_frame, Heard::Line(line))),
                None => warn!(
                    "game frame {}: script {} asks for a voice line at {c:#010x}, which is none",
                    self.room.game_frame, task.number
                ),
            },
            Some(Scene::Call(Call::AwaitFade)) if !self.room.fade.at_rest() => return Answer::Wait,
            Some(Scene::Call(Call::AwaitFade)) => {}
            Some(Scene::Call(Call::Background { drawn })) => {
                self.room.background = drawn;
                info!(
                    "game frame {}: the background is {}",
                    self.room.game_frame,
                    if drawn { "drawn again" } else { "left out" }
                );
            }
            Some(Scene::Call(Call::Hold { on })) => {
                info!(
                    "game frame {}: hold reason 64 is {} (not held: a read takes no game frame)",
                    self.room.game_frame,
                    if on != 0 { "set" } else { "let go" }
                );
            }
            Some(Scene::Call(Call::Before { on })) => {
                self.room.before = on;
                info!(
                    "game frame {}: the picture before is {}",
                    self.room.game_frame,
                    if on {
                        "laid into every frame"
                    } else {
                        "left out again"
                    }
                );
            }
            // The argument pointer of the task, which a native routine is called with.
            // Without an address it names the answer of `0x800BBFB4`, which is not read.
            Some(Scene::Argument {
                address: Some(address),
            }) => {
                self.room.arguments.insert(task.number, address);
                self.room.records.remove(&task.number);
            }
            // The argument record of the task, which the pointer names from now on.
            Some(Scene::Arguments { a, b, c }) => {
                self.room.records.insert(task.number, [a as u32, b, c]);
                self.room.arguments.remove(&task.number);
            }
            // The native routine of a package with several lists that ends a scene (room
            // 0's and the four of area 5): the story variable its lists are picked by is
            // the record's first word and the room is read again when the second is not
            // zero (the force flag).
            Some(Scene::Native { routine, .. })
                if scene::again_routine(self.file).is_some_and(|(again, _)| again == routine)
                    && self.room.records.contains_key(&task.number) =>
            {
                let [value, force, _] = self.room.records[&task.number];
                let variable = scene::again_routine(self.file).map_or(0, |(_, variable)| variable);
                self.room.carried.set_variable(variable, value as i32);
                self.room.again |= force != 0;
                info!(
                    "game frame {}: script {} sets story variable {variable} to {value}{}",
                    self.room.game_frame,
                    task.number,
                    if force != 0 {
                        " and asks for the room once more"
                    } else {
                        ""
                    }
                );
            }
            Some(Scene::Native { routine, always })
                if Some(routine) == self.room.movie_routine
                    && self.room.records.contains_key(&task.number) =>
            {
                let selector = self.room.selector;
                if !(always || selector < 0 || selector == task.number) {
                    return Answer::Wait;
                }
                let [movie, played, _] = self.room.records[&task.number];
                if played == 0 {
                    info!(
                        "game frame {}: script {} plays movie {movie}",
                        self.room.game_frame, task.number
                    );
                    self.room.records.insert(task.number, [movie, 1, 0]);
                    self.room.movie = Some(movie);
                    self.room.busy = true;
                    return Answer::Wait;
                }
                info!(
                    "game frame {}: script {} goes on behind movie {movie}",
                    self.room.game_frame, task.number
                );
                self.room.records.remove(&task.number);
            }
            Some(Scene::Native { routine, always })
                if (self.room.read_routines.iter()).any(|read| read.address == routine)
                    && self.room.records.contains_key(&task.number) =>
            {
                let selector = self.room.selector;
                if !(always || selector < 0 || selector == task.number) {
                    return Answer::Wait;
                }
                let [state, set, rest] = self.room.records[&task.number];
                if state == 0 {
                    let read = (self.room.read_routines.iter())
                        .find(|read| read.address == routine)
                        .copied();
                    let (frame, number) = (self.room.game_frame, task.number);
                    self.room.records.insert(number, [1, set, rest]);
                    let actor = read
                        .and_then(|read| Some((read, self.actors.get_mut(read.actor as usize)?)));
                    match actor {
                        Some((read, actor)) => match read.reads {
                            Reads::Animation(animation) => info!(
                                "game frame {frame}: script {number} reads animation {animation} for actor {}: {}",
                                read.actor,
                                match actor.read_animation(animation as usize) {
                                    true => "now in memory",
                                    false => "it was in memory or the model has none",
                                }
                            ),
                            Reads::Set => info!(
                                "game frame {frame}: script {number} reads animation set {set} for actor {}, which brings the animations {:?}",
                                read.actor,
                                actor.read_set(set as usize)
                            ),
                        },
                        None => info!(
                            "game frame {frame}: script {number} reads animations for an actor the room does not have"
                        ),
                    }
                    return Answer::Wait;
                }
                self.room.records.remove(&task.number);
            }
            Some(Scene::Native { routine, always }) if Some(routine) == self.room.clear_routine => {
                let selector = self.room.selector;
                if !(always || selector < 0 || selector == task.number) {
                    return Answer::Wait;
                }
                info!(
                    "game frame {}: script {} takes the shown things away",
                    self.room.game_frame, task.number
                );
                self.room.shows.clear();
                self.room.cleared = true;
            }
            Some(Scene::Native { routine, always })
                if routine == scene::LESSON_ROUTINE
                    && self.file == scene::LESSONS_FILE
                    && self.room.records.contains_key(&task.number) =>
            {
                let selector = self.room.selector;
                if !(always || selector < 0 || selector == task.number) {
                    return Answer::Wait;
                }
                let [number, ..] = self.room.records[&task.number];
                info!(
                    "game frame {}: script {} makes the next fight lesson {number}",
                    self.room.game_frame, task.number
                );
                self.room.lesson = Some(number);
            }
            Some(Scene::Native { routine, always })
                if (self.room.kept_set_routines.iter()).any(|(address, _)| *address == routine) =>
            {
                let selector = self.room.selector;
                if !(always || selector < 0 || selector == task.number) {
                    return Answer::Wait;
                }
                let kept = (self.room.kept_set_routines.iter())
                    .find(|(address, _)| *address == routine)
                    .map(|(_, set)| *set);
                info!(
                    "game frame {}: script {} keeps set {kept:?} of the player for the way back \
                     from a fight",
                    self.room.game_frame, task.number
                );
                self.room.kept_set = kept;
            }
            Some(Scene::Native { routine, always })
                if self.room.fade_routines.contains(&routine) =>
            {
                let selector = self.room.selector;
                if !(always || selector < 0 || selector == task.number) {
                    return Answer::Wait;
                }
                info!(
                    "game frame {}: script {} calls the routine that fades the sequence of \
                     the sound record that stays",
                    self.room.game_frame, task.number
                );
                (self.room.music).push((self.room.game_frame, Heard::FadeKept));
            }
            Some(Scene::Native { routine, always })
                if matches!(self.package.screen_fade(routine), Some(1 | 3)) =>
            {
                let selector = self.room.selector;
                if !(always || selector < 0 || selector == task.number) {
                    return Answer::Wait;
                }
                let holds = self.package.screen_fade(routine) == Some(1);
                info!(
                    "game frame {}: script {} {}",
                    self.room.game_frame,
                    task.number,
                    if holds {
                        "has the screen held and darkened"
                    } else {
                        "lets the hold of the screen go"
                    }
                );
                self.room.darkened = holds.then_some(0);
            }
            Some(Scene::Native { routine, always }) if self.package.shakes(routine) => {
                let selector = self.room.selector;
                if !(always || selector < 0 || selector == task.number) {
                    return Answer::Wait;
                }
                info!(
                    "game frame {}: script {} has the picture shaken",
                    self.room.game_frame, task.number
                );
                self.room.shake = Some(0);
            }
            Some(Scene::Native { routine, always })
                if show_mover::flips(self.package, routine).is_some() =>
            {
                let selector = self.room.selector;
                if !(always || selector < 0 || selector == task.number) {
                    return Answer::Wait;
                }
                let word = show_mover::flips(self.package, routine).unwrap_or_default();
                let was = (self.room.flipped.get(&word).copied()).unwrap_or_else(|| {
                    (self.package.bytes_at(word, 4)).is_some_and(|bytes| bytes != [0; 4])
                });
                info!(
                    "game frame {}: script {} flips the word {word:#010x} that picks a \
                     mover's bone: {} now",
                    self.room.game_frame,
                    task.number,
                    u8::from(!was)
                );
                self.room.flipped.insert(word, !was);
            }
            Some(Scene::Native { routine, always })
                if self.package.view_test(routine).is_some() =>
            {
                let selector = self.room.selector;
                if !(always || selector < 0 || selector == task.number) {
                    return Answer::Wait;
                }
                let waited = self.package.view_test(routine).map(usize::from);
                if Some(self.room.view.unwrap_or(self.view)) != waited {
                    return Answer::Wait;
                }
                info!(
                    "game frame {}: script {} finds the wanted view {} it waited for",
                    self.room.game_frame, task.number, self.view
                );
            }
            Some(Scene::Native { routine, always })
                if ending::asks_save(self.package, routine).is_some() =>
            {
                let selector = self.room.selector;
                if !(always || selector < 0 || selector == task.number) {
                    return Answer::Wait;
                }
                let at = ending::asks_save(self.package, routine).unwrap_or_default();
                let mut text = [0; 24];
                match self.package.bytes_at(at, text.len()) {
                    Some(bytes) => text.copy_from_slice(bytes),
                    None => warn!("the text of the place at {at:#010x} is not in the package"),
                }
                info!(
                    "game frame {}: script {} asks for the save behind the end of the story \
                     (the text at {at:#010x})",
                    self.room.game_frame, task.number
                );
                self.room.end_card = Some(text);
            }
            // The second reads the last picture, makes the screen black, lets it rise,
            // leaves the background out and hides the actors, and answers 1.
            Some(Scene::Native { routine, always })
                if ending::reads(self.package, routine).is_some() =>
            {
                let selector = self.room.selector;
                if !(always || selector < 0 || selector == task.number) {
                    return Answer::Wait;
                }
                let file = ending::reads(self.package, routine).unwrap_or(ending::PICTURES);
                info!(
                    "game frame {}: script {} reads the last picture, file {file}, which \
                     rises out of the black over {} game frames",
                    self.room.game_frame,
                    task.number,
                    ending::RISE
                );
                self.room.ending = Some((file, self.room.game_frame));
            }
            // The third draws the picture's two halves and answers 0: the task stays on
            // the call for good, which is the end of the game.
            Some(Scene::Native { routine, .. })
                if self.room.ending.is_some() && ending::draws(self.package, routine) =>
            {
                return Answer::Wait;
            }
            Some(Scene::Native { routine, always }) if self.package.frees_models(routine) => {
                let selector = self.room.selector;
                if !(always || selector < 0 || selector == task.number) {
                    return Answer::Wait;
                }
                info!(
                    "game frame {}: script {} frees the room's package of models",
                    self.room.game_frame, task.number
                );
            }
            Some(Scene::Native { routine, always })
                if ride::leaves_by_variables(self.package, routine) =>
            {
                let selector = self.room.selector;
                if !(always || selector < 0 || selector == task.number) {
                    return Answer::Wait;
                }
                let variables = ride::ROOM_VARIABLES.map(|n| self.room.carried.variable(n));
                let room = ride::room_of(variables);
                let left_out: Vec<String> = (self.room.left_out.iter())
                    .map(|(code, times)| format!("{code:#04x} {times} times"))
                    .collect();
                info!(
                    "game frame {}: script {} sends the game to room {room}, the room of the \
                     story variables {:?} ({variables:?}); commands of the room that went on \
                     without being run: {}",
                    self.room.game_frame,
                    task.number,
                    ride::ROOM_VARIABLES,
                    left_out.join(", ")
                );
                self.room.leaves = u32::try_from(room).ok();
                task.next();
                return Answer::Wait;
            }
            Some(Scene::Native { routine, always }) if self.package.is_way_back(routine) => {
                let selector = self.room.selector;
                if !(always || selector < 0 || selector == task.number) {
                    return Answer::Wait;
                }
                match self.room.forest_place {
                    Some(place) => {
                        let room = leave_forest::lost(&mut self.room.carried, place);
                        info!(
                            "game frame {}: script {} takes the way back to the gates: story \
                             variable {FOREST_VARIABLE} is {}, story flag {} is set, the game \
                             goes to the stored room {room}",
                            self.room.game_frame,
                            task.number,
                            self.room.carried.variable(FOREST_VARIABLE),
                            leave_forest::LOST_FLAG
                        );
                        self.room.leaves = Some(room.into());
                        task.next();
                        return Answer::Wait;
                    }
                    None => self.room.leave_out(command.code),
                }
            }
            Some(Scene::Native { routine, always })
                if (self.room.equip_routines.iter()).any(|equip| equip.address == routine) =>
            {
                let selector = self.room.selector;
                if !(always || selector < 0 || selector == task.number) {
                    return Answer::Wait;
                }
                let equips =
                    (self.room.equip_routines.iter()).filter(|equip| equip.address == routine);
                for equip in equips {
                    let done = self.room.carried.equip(equip.item, equip.kind);
                    info!(
                        "game frame {}: script {} puts thing {} of kind {} on the player ({}): \
                         weapon, armour and third thing are {:?}",
                        self.room.game_frame,
                        task.number,
                        equip.item,
                        equip.kind,
                        if done { "done" } else { "refused" },
                        self.room.carried.worn
                    );
                }
            }
            // The spawner of the show records. A call that is not for every case is made
            // while the task selector is free or names the task.
            Some(Scene::Native { routine, always })
                if Some(routine) == self.room.spawner
                    && self.room.arguments.contains_key(&task.number) =>
            {
                let selector = self.room.selector;
                if always || selector < 0 || selector == task.number {
                    let list = self.room.arguments[&task.number];
                    let shows = self.package.shows_at(list);
                    info!(
                        "game frame {}: script {} hands the show list {list:#010x} to the \
                         spawner, {} records",
                        self.room.game_frame,
                        task.number,
                        shows.len()
                    );
                    self.room.shows.extend(shows);
                }
            }
            // An object of a room has its own code in its package: the one that is read
            // is made here, any other goes on like a command that is not run.
            Some(Scene::StartObject { record }) if beat::is_at(self.file, self.package, record) => {
                info!(
                    "game frame {}: the beat of two sounds starts (sound {})",
                    self.room.game_frame,
                    beat::SOUND
                );
                // The start routine runs the routine once, which is the first beat.
                self.room.beat = Some(Beat::new());
            }
            Some(Scene::StartObject { record }) if toll::is_at(self.package, record) => {
                info!(
                    "game frame {}: the sound at an even pace starts (sound {} every {} game \
                     frames)",
                    self.room.game_frame,
                    toll::SOUND,
                    toll::EVERY
                );
                // The start routine runs the routine once, which is the first sound.
                self.room.toll = Some(Toll::new());
            }
            // Its routine asks no told state; the wrapper of its task ends it.
            Some(Scene::Tell { record, state, .. })
                if self.room.toll.is_some() && toll::is_at(self.package, record) =>
            {
                info!(
                    "game frame {}: the sound at an even pace is told state {}",
                    self.room.game_frame, state as i32
                );
                if state as i32 == toll::ENDS {
                    self.room.toll = None;
                }
            }
            // The spot that answers a press: its maker leaves the task with no told
            // state, and a type below 14 is not run before its first game frame.
            Some(Scene::StartObject { record })
                if self
                    .room
                    .spots
                    .iter()
                    .any(|(spot, ..)| spot.record == record) =>
            {
                info!(
                    "game frame {}: the spot at {record:#010x} starts",
                    self.room.game_frame
                );
                for (spot, _, task) in &mut self.room.spots {
                    if spot.record == record {
                        *task = Some(Marked::new());
                    }
                }
            }
            Some(Scene::Tell { record, state, .. })
                if self
                    .room
                    .spots
                    .iter()
                    .any(|(spot, ..)| spot.record == record) =>
            {
                info!(
                    "game frame {}: the spot at {record:#010x} is told state {}",
                    self.room.game_frame, state as i32
                );
                for (spot, _, task) in &mut self.room.spots {
                    if let (true, Some(task)) = (spot.record == record, task) {
                        task.tell(state as i32);
                    }
                }
            }
            Some(Scene::AwaitObject { record, state, .. })
                if self
                    .room
                    .spots
                    .iter()
                    .any(|(spot, ..)| spot.record == record) =>
            {
                let reached = (self.room.spots.iter()).any(|(spot, _, task)| {
                    spot.record == record && task.is_some_and(|task| task.reached == state as i32)
                });
                if !reached {
                    return Answer::Wait;
                }
                info!(
                    "game frame {}: the spot at {record:#010x} has reached state {}",
                    self.room.game_frame, state as i32
                );
            }
            Some(Scene::StartObject { record }) => {
                match Course::of(self.file, self.package, record) {
                    Some(course) => {
                        info!(
                            "game frame {}: the object of the sky starts at {:?}",
                            self.room.game_frame, course.from
                        );
                        // The start routine runs the routine once, which draws.
                        self.room.sky = Some((record, course, Sky::new(), false));
                    }
                    None => self.room.leave_out(command.code),
                }
            }
            // The object of room 8 is found by the address of its data, as a door is.
            Some(Scene::Tell {
                record,
                state,
                by_data: true,
            }) if (self.room.scatter.as_ref())
                .is_some_and(|scattered| scattered.places.data == record) =>
            {
                info!(
                    "game frame {}: the object at {record:#010x} is told state {state}",
                    self.room.game_frame
                );
                if let Some(scattered) = &mut self.room.scatter {
                    scattered.run.tell(state as i32);
                }
            }
            Some(Scene::Tell {
                record,
                state,
                by_data: true,
            }) if (self.room.shower).is_some_and(|(table, _)| table == record) => {
                info!(
                    "game frame {}: the object that plays a movie is told entry {state}",
                    self.room.game_frame
                );
                if let Some((_, showing)) = &mut self.room.shower {
                    showing.tell(state);
                }
            }
            Some(Scene::AwaitObject {
                record,
                state,
                by_data: true,
            }) if (self.room.shower).is_some_and(|(table, _)| table == record) => {
                let reached =
                    (self.room.shower).is_some_and(|(_, showing)| showing.reached == state as i32);
                if !reached {
                    return Answer::Wait;
                }
                info!(
                    "game frame {}: the object that plays a movie has reached state {state}",
                    self.room.game_frame
                );
            }
            // An object whose routine does nothing takes a told state and that is all.
            Some(Scene::Tell {
                record,
                by_data: true,
                ..
            }) if self.room.idle.contains(&record) => {}
            // A door is found by the address of its record (`by_data`).
            Some(Scene::Tell {
                record,
                state,
                by_data: true,
            }) if self.room.doors.iter().any(|(at, ..)| *at == record) => {
                info!(
                    "game frame {}: the door at {record:#010x} is told state {state}",
                    self.room.game_frame
                );
                for (_, _, swing) in (self.room.doors.iter_mut()).filter(|(at, ..)| *at == record) {
                    swing.tell(state as i32);
                }
            }
            Some(Scene::AwaitObject {
                record,
                state,
                by_data: true,
            }) if self.room.doors.iter().any(|(at, ..)| *at == record) => {
                let reached = (self.room.doors.iter())
                    .any(|(at, _, swing)| *at == record && swing.reached == state as i32);
                if !reached {
                    return Answer::Wait;
                }
                info!(
                    "game frame {}: the door at {record:#010x} has reached state {state}",
                    self.room.game_frame
                );
            }
            // A picture that follows a story flag keeps the state it is told: told
            // state 0 it changes in front of its flag's setting.
            Some(Scene::Tell {
                record,
                state,
                by_data: true,
            }) if (self.room.followers.iter()).any(|(found, _)| found.record == record) => {
                info!(
                    "game frame {}: the picture at {record:#010x} is told state {}",
                    self.room.game_frame, state as i32
                );
                self.room.pictures_told.insert(record, state as i32);
            }
            // The object of the memory card takes a told state in the wrapper of its
            // task: -2 and it ends.
            Some(Scene::Tell {
                record,
                state,
                by_data: true,
            }) if (self.room.card.as_ref()).is_some_and(|carded| carded.card.record == record) => {
                info!(
                    "game frame {}: the object of the memory card at {record:#010x} is told \
                     state {}",
                    self.room.game_frame, state as i32
                );
                if let Some(carded) = &mut self.room.card {
                    carded.floating.tell(state as i32);
                }
            }
            // An exit and a trigger of the place map take a told state in the wrapper of
            // their task: -3 and they rest, -1 and they go on.
            Some(Scene::Tell {
                record,
                state,
                by_data: true,
            }) if self.room.exits.contains_key(&record) => {
                info!(
                    "game frame {}: the exit at {record:#010x} is told state {}",
                    self.room.game_frame, state as i32
                );
                self.room.exits.insert(record, state as i32);
            }
            Some(Scene::Tell {
                record,
                state,
                by_data: true,
            }) if (self.room.map_triggers.iter()).any(|(trigger, ..)| trigger.record == record) => {
                info!(
                    "game frame {}: the trigger of the place map at {record:#010x} is told \
                     state {}",
                    self.room.game_frame, state as i32
                );
                for (trigger, waiting, _) in &mut self.room.map_triggers {
                    if trigger.record == record {
                        waiting.tell(state as i32);
                    }
                }
            }
            // The script of a trigger of the place map: its argument is the trigger's
            // record, and its routine makes the player stand again when the record says
            // so (`0x800A8ECC`). The routine answers 1 whatever it did.
            Some(Scene::Argument { address: None }) if self.map_trigger(task.number).is_some() => {}
            Some(Scene::Native { routine, always })
                if (self.map_trigger(task.number))
                    .is_some_and(|trigger| trigger.routine == routine) =>
            {
                let selector = self.room.selector;
                if !(always || selector < 0 || selector == task.number) {
                    return Answer::Wait;
                }
                let stands = self
                    .map_trigger(task.number)
                    .is_some_and(|trigger| trigger.stands_again);
                if let (true, Some(walker)) = (stands, &mut self.player) {
                    walker.stand_again();
                }
                info!(
                    "game frame {}: script {} of the trigger of the place map: the player \
                     stands{}",
                    self.room.game_frame,
                    task.number,
                    if stands { " again" } else { " as it is" }
                );
            }
            Some(Scene::TellOwner { state }) if self.finds().is_some_and(owns) => {
                info!(
                    "game frame {}: script {} tells the thing to be searched state {state}",
                    self.room.game_frame, task.number
                );
                if let Some((_, searched)) = &mut self.room.finds {
                    searched.search.tell(state as i32);
                }
            }
            Some(Scene::AwaitOwner { state }) if self.finds().is_some_and(owns) => {
                let reached = (self.finds())
                    .is_some_and(|(_, searched)| searched.search.reached == state as i32);
                if !reached {
                    return Answer::Wait;
                }
                info!(
                    "game frame {}: the thing to be searched has reached state {state}",
                    self.room.game_frame
                );
            }
            Some(Scene::Argument { address: None }) if self.finds().is_some_and(owns) => {}
            // The native test behind a fight: whether it did not end won.
            Some(Scene::NativeChoice { routine, always })
                if self.package.is_outcome_test(routine) =>
            {
                let selector = self.room.selector;
                if !(always || selector < 0 || selector == task.number) {
                    return Answer::Wait;
                }
                task.choice = i32::from(self.room.outcome != ended::WON);
                info!(
                    "game frame {}: the test of the fight's outcome answers script {} with {}",
                    self.room.game_frame, task.number, task.choice
                );
            }
            // The test in front of the screen of file 44: once in a room, by its bit
            // `0x20000000`, what there is to do for him (`Carried::in_front_of_the_list`).
            Some(Scene::NativeChoice { routine, always }) if self.package.is_list_test(routine) => {
                let selector = self.room.selector;
                if !(always || selector < 0 || selector == task.number) {
                    return Answer::Wait;
                }
                const ASKED: u32 = 0x2000_0000;
                task.choice = match self.room.bits & ASKED {
                    0 => self.room.carried.in_front_of_the_list(),
                    _ => 0,
                };
                self.room.bits |= ASKED;
                info!(
                    "game frame {}: the test in front of the screen of kind 2 answers \
                     script {} with {}",
                    self.room.game_frame, task.number, task.choice
                );
            }
            Some(Scene::NativeChoice { routine, always })
                if self.package.asked_test(routine).is_some() =>
            {
                let selector = self.room.selector;
                if !(always || selector < 0 || selector == task.number) {
                    return Answer::Wait;
                }
                let mask = self.package.asked_test(routine).unwrap_or(0);
                task.choice = scene::asked(mask, self.room.bits);
                info!(
                    "game frame {}: the test of the room's bits {:#x} by the mask {mask:#x} \
                     answers script {} with {}",
                    self.room.game_frame, self.room.bits, task.number, task.choice
                );
            }
            Some(Scene::NativeChoice { routine, always })
                if self.package.plain_answer(routine).is_some() =>
            {
                let selector = self.room.selector;
                if !(always || selector < 0 || selector == task.number) {
                    return Answer::Wait;
                }
                task.choice = self.package.plain_answer(routine).unwrap_or(0);
                info!(
                    "game frame {}: the test {routine:#010x}, a bare return, answers script {} \
                     with {}",
                    self.room.game_frame, task.number, task.choice
                );
            }
            // The gates' test behind a lesson: whether the lesson refused his choice.
            Some(Scene::NativeChoice { routine, always })
                if self.package.is_refusal_test(routine) =>
            {
                let selector = self.room.selector;
                if !(always || selector < 0 || selector == task.number) {
                    return Answer::Wait;
                }
                task.choice = i32::from(self.room.outcome == ended::REFUSED);
                info!(
                    "game frame {}: the test of a refused lesson answers script {} with {}",
                    self.room.game_frame, task.number, task.choice
                );
            }
            Some(Scene::NativeChoice { routine, always })
                if (self.finds())
                    .is_some_and(|found| owns(found) && found.0.routine(routine).is_some()) =>
            {
                let selector = self.room.selector;
                if !(always || selector < 0 || selector == task.number) {
                    return Answer::Wait;
                }
                let keeping = &self.room.carried.keeping;
                if let Some((finds, searched)) = &mut self.room.finds
                    && let Some(asked) = finds.routine(routine)
                {
                    let has = keeping.count(finds.find.needs, items::Kind::Item) > 0;
                    task.choice = searched.search.answer(asked, finds, has) as i32;
                    info!(
                        "game frame {}: the routine {asked:?} of the thing to be searched \
                         answers script {} with {}",
                        self.room.game_frame, task.number, task.choice
                    );
                }
            }
            // The script of a player from the place map talks to the object that
            // started it.
            Some(Scene::TellOwner { state }) if self.arrives(task.number) => {
                info!(
                    "game frame {}: script {} tells the object of the player from the place \
                     map state {state}",
                    self.room.game_frame, task.number
                );
                if let Some(arrives) = &mut self.room.arriving {
                    arrives.arriving.tell(state as i32);
                }
            }
            Some(Scene::AwaitOwner { state }) if self.arrives(task.number) => {
                let reached = (self.room.arriving.as_ref())
                    .is_some_and(|arrives| arrives.arriving.reached == state as i32);
                if !reached {
                    return Answer::Wait;
                }
                info!(
                    "game frame {}: the object of the player from the place map has reached \
                     state {state}",
                    self.room.game_frame
                );
            }
            // A script that a gate started talks to that gate.
            Some(Scene::TellOwner { state }) if self.room.owners.contains_key(&task.number) => {
                let gate = self.room.owners[&task.number];
                info!(
                    "game frame {}: script {} tells gate {gate} state {state}",
                    self.room.game_frame, task.number
                );
                self.room.crossings[gate].crossing.tell(state as i32);
            }
            Some(Scene::AwaitOwner { state }) if self.room.owners.contains_key(&task.number) => {
                let gate = self.room.owners[&task.number];
                if self.room.crossings[gate].crossing.reached != state as i32 {
                    return Answer::Wait;
                }
                info!(
                    "game frame {}: gate {gate} has reached state {state}",
                    self.room.game_frame
                );
            }
            // The script that a bank of a passage started talks to that bank.
            Some(Scene::TellOwner { state })
                if self.room.passage_owners.contains_key(&task.number) =>
            {
                let bank = self.room.passage_owners[&task.number];
                info!(
                    "game frame {}: script {} tells bank {bank} of the passage state {state}",
                    self.room.game_frame, task.number
                );
                self.room.passings[bank].passing.tell(state as i32);
            }
            Some(Scene::AwaitOwner { state })
                if self.room.passage_owners.contains_key(&task.number) =>
            {
                let bank = self.room.passage_owners[&task.number];
                if self.room.passings[bank].passing.reached != state as i32 {
                    return Answer::Wait;
                }
                info!(
                    "game frame {}: bank {bank} of the passage has reached state {state}",
                    self.room.game_frame
                );
            }
            // The script that the warp of a forest started talks to that object.
            Some(Scene::TellOwner { state })
                if (self.room.warps.as_ref()).is_some_and(|warp| warp.2 == Some(task.number)) =>
            {
                info!(
                    "game frame {}: script {} tells the warp state {state}",
                    self.room.game_frame, task.number
                );
                if let Some((_, warping, ..)) = &mut self.room.warps {
                    warping.tell(state as i32);
                }
            }
            // The script that the way into area 5 started talks to that object.
            Some(Scene::TellOwner { state })
                if (self.room.rides.as_ref()).is_some_and(|ride| ride.2 == Some(task.number)) =>
            {
                info!(
                    "game frame {}: script {} tells the way into area 5 state {state}",
                    self.room.game_frame, task.number
                );
                if let Some((_, riding, ..)) = &mut self.room.rides {
                    riding.tell(state as i32);
                }
            }
            Some(Scene::AwaitOwner { state })
                if (self.room.rides.as_ref()).is_some_and(|ride| ride.2 == Some(task.number)) =>
            {
                let reached = (self.room.rides.as_ref())
                    .is_some_and(|(_, riding, ..)| riding.reached == state as i32);
                if !reached {
                    return Answer::Wait;
                }
                info!(
                    "game frame {}: the way into area 5 has reached state {state}",
                    self.room.game_frame
                );
            }
            // A script that a door started talks to that door.
            Some(Scene::TellOwner { state })
                if self.room.door_owners.contains_key(&task.number) =>
            {
                let record = self.room.door_owners[&task.number];
                info!(
                    "game frame {}: script {} tells the door at {record:#010x} state {state}",
                    self.room.game_frame, task.number
                );
                for (_, _, swing) in (self.room.doors.iter_mut()).filter(|(at, ..)| *at == record) {
                    swing.tell(state as i32);
                }
            }
            Some(Scene::AwaitOwner { state })
                if self.room.door_owners.contains_key(&task.number) =>
            {
                let record = self.room.door_owners[&task.number];
                let reached = (self.room.doors.iter())
                    .any(|(at, _, swing)| *at == record && swing.reached == state as i32);
                if !reached {
                    return Answer::Wait;
                }
                info!(
                    "game frame {}: the door at {record:#010x} has reached state {state}",
                    self.room.game_frame
                );
            }
            Some(Scene::Tell { record, state, .. })
                if (self.room.ride_sounds.as_ref())
                    .is_some_and(|(found, _)| found.at == record) =>
            {
                info!(
                    "game frame {}: the object that plays a sound at a pace is told state {}",
                    self.room.game_frame, state as i32
                );
                if let Some((_, task)) = &mut self.room.ride_sounds {
                    task.tell(state as i32);
                }
            }
            Some(Scene::Tell { record, state, .. })
                if (self.room.plays.as_ref()).is_some_and(|(at, ..)| *at == record) =>
            {
                info!(
                    "game frame {}: the object that plays two models is told state {state}",
                    self.room.game_frame
                );
                if let Some((_, _, task, _)) = &mut self.room.plays {
                    task.tell(state as i32);
                }
            }
            Some(Scene::Tell { record, state, .. })
                if (self.room.slides.as_ref()).is_some_and(|(at, ..)| *at == record) =>
            {
                info!(
                    "game frame {}: the sliding bars are told state {state}",
                    self.room.game_frame
                );
                if let Some((_, _, task, _)) = &mut self.room.slides {
                    task.tell(state as i32);
                }
            }
            Some(Scene::Tell { record, state, .. }) => match &mut self.room.sky {
                Some((at, _, sky, _)) if record == *at => {
                    info!(
                        "game frame {}: the object of the sky is told state {state}",
                        self.room.game_frame
                    );
                    sky.tell(state as i32);
                }
                _ => self.room.leave_out(command.code),
            },
            // A text that neither the room's text nor a listed script's address has.
            Some(Scene::Say { group, .. }) if self.room.group_of(group).is_none() => {
                warn!(
                    "{:#010x}: the room's text has no group {group:#x}",
                    command.address
                );
                self.room.leave_out(command.code);
            }
            Some(Scene::Say {
                thing,
                side,
                group,
                waits,
                question,
            }) => {
                let group = self.room.group_of(group).unwrap_or(self.room.groups.len());
                let mut opened = false;
                let setting = self.setting();
                let (closed, held) = (task.closed_answers, task.window_place);
                let answer = task.say(waits, setting, || {
                    opened = true;
                    self.window((thing, side), group, (question, closed, held))
                });
                if opened {
                    info!(
                        "game frame {}: thing {thing} says group {group} to thing {side}, {} \
                         pages",
                        self.room.game_frame,
                        self.room.groups[group].pages.len()
                    );
                    if question {
                        self.room.asked += 1;
                        info!(
                            "game frame {}: it is question {} of the room, {} answers, \
                             switched off {closed:#x}; the run answers {:?}",
                            self.room.game_frame,
                            self.room.asked,
                            self.room.groups[group].answers.len(),
                            self.room.answers.get(self.room.asked - 1)
                        );
                    }
                    // The opener has put the picture's box on its side of the text.
                    let picture_left = (task.speech.window.as_ref()).is_some_and(|window| {
                        let layout = window.layout();
                        layout.picture.x < layout.text.x
                    });
                    let said = Said {
                        thing,
                        other: side,
                        group,
                        picture_left,
                        open_answers: !closed,
                        place: held.unwrap_or(match question {
                            true => window::Where::Top,
                            false => window::Where::Bottom,
                        }),
                        pass: window::Pass::default(),
                        passes: 0,
                    };
                    self.room.said.insert(task.number, said);
                }
                task.tell_confirm(self);
                return answer;
            }
            Some(Scene::AwaitMessage) => {
                let setting = self.setting();
                let answer = task.await_message(setting);
                task.tell_confirm(self);
                if answer == Answer::GoOn {
                    info!(
                        "game frame {}: the message of script {} is through, the choice is {}",
                        self.room.game_frame, task.number, task.choice
                    );
                }
                return answer;
            }
            // The handler asks the actor of the room's list; the player is not one, and
            // with thing 0 it stays on the command.
            Some(Scene::PauseAtEnd { thing, pause }) => {
                let game_frame = self.room.game_frame;
                let actor = usize::try_from(thing)
                    .ok()
                    .and_then(|thing| self.actor(thing));
                let Some(actor) = actor else {
                    return Answer::Wait;
                };
                let playback = &mut actor.runner.playback;
                if playback.counter != i32::from(playback.span.end) {
                    return Answer::Wait;
                }
                playback.paused = pause;
                info!(
                    "game frame {game_frame}: thing {thing} stands on the end frame {} of its \
                     animation, its pause is {}",
                    playback.span.end,
                    if pause { "set" } else { "cleared" }
                );
            }
            Some(Scene::WindowPlace { place }) => {
                task.place_windows(place);
                info!(
                    "game frame {}: the windows of script {} stand at {:?} from now on",
                    self.room.game_frame, task.number, task.window_place
                );
            }
            Some(Scene::SwitchAnswers { off, answers }) => {
                task.switch_answers(off, answers);
                info!(
                    "game frame {}: script {} switches answers {answers:#x} {}; switched off \
                     are {:#x}",
                    self.room.game_frame,
                    task.number,
                    if off { "off" } else { "on" },
                    task.closed_answers
                );
            }
            Some(Scene::AwaitCue { cue }) => {
                let answer = task.await_cue(cue as usize);
                if answer == Answer::GoOn {
                    info!(
                        "game frame {}: cue {cue} of the message is reached",
                        self.room.game_frame
                    );
                }
                return answer;
            }
            Some(Scene::PlaceMap) => {
                info!(
                    "game frame {}: the place map is asked for",
                    self.room.game_frame
                );
                self.room.map = true;
            }
            Some(Scene::StoreRoom { room }) => {
                let before = leave_forest::store_room(&mut self.room.carried, room);
                info!(
                    "game frame {}: script {} stores room {room}, the stored room was {before}",
                    self.room.game_frame, task.number
                );
            }
            Some(Scene::Screen { kind, argument })
                if kind <= 2
                    || (kind == jc_formats::merge_screen::BACK_KIND
                        && (self.room.screen_sleeps
                            || (self.room.screen).is_some_and(|asked| asked.kind == kind))) =>
            {
                match self.room.screen {
                    None => {
                        info!(
                            "game frame {}: script {} asks for the screen of kind {kind} with \
                         argument {argument} and waits",
                            self.room.game_frame, task.number
                        );
                        self.room.screen = Some(AskedScreen {
                            script: task.number,
                            kind,
                            argument,
                            state: Asking::Asked,
                            answer: 0,
                        });
                        return Answer::Wait;
                    }
                    Some(asked) if asked.script == task.number && asked.state == Asking::Left => {
                        self.room.screen = None;
                        task.choice = asked.answer;
                        info!(
                            "game frame {}: the screen of kind {kind} has {}, script {} goes \
                         on with choice {}",
                            self.room.game_frame,
                            if self.room.screen_sleeps {
                                "answered"
                            } else {
                                "left"
                            },
                            task.number,
                            asked.answer
                        );
                    }
                    Some(_) => return Answer::Wait,
                }
            }
            // The handler moves the task on and answers that its game frame is over.
            Some(Scene::GoToRoom { room }) => {
                let left_out: Vec<String> = (self.room.left_out.iter())
                    .map(|(code, times)| format!("{code:#04x} {times} times"))
                    .collect();
                info!(
                    "game frame {}: script {} sends the game to room {room}; commands of the \
                     room that went on without being run: {}",
                    self.room.game_frame,
                    task.number,
                    left_out.join(", ")
                );
                self.room.leaves = Some(room);
                task.next();
                return Answer::Wait;
            }
            // The command behind a fight: the choice is 0 when its `b` is how the fight
            // ended, else 1.
            _ if command.code == scene::COMMAND_FIGHT_ENDED => {
                task.choice = i32::from(command.b != self.room.outcome);
                info!(
                    "game frame {}: script {} asks whether the fight ended with outcome {}: \
                     choice {}",
                    self.room.game_frame, task.number, command.b, task.choice
                );
            }
            // Everybody is made whole (`0x800B9F20`).
            _ if command.code == scene::COMMAND_MAKE_WHOLE => {
                self.room.carried.make_whole();
                info!(
                    "game frame {}: script {} makes the player whole",
                    self.room.game_frame, task.number
                );
            }
            _ if command.code == scene::COMMAND_FIGHT
                && self.room.awaits_results == Some(task.number) =>
            {
                self.room.awaits_results = None;
                if let Some(untaken) = self.room.results.take() {
                    Self::give_results(self.room, untaken);
                }
                info!(
                    "game frame {}: the results screen of the fight of script {} has left, \
                     the script goes on",
                    self.room.game_frame, task.number
                );
            }
            _ if command.code == scene::COMMAND_FIGHT && self.room.fight.is_some() => {
                let lasts = self.show_fight();
                self.look_at_a_request();
                if lasts {
                    return Answer::Wait;
                }
                if let Some(fight) = self.room.fight.take() {
                    let command = (task.number, command.c);
                    // The hold of the screen ends in the next pass and the room is
                    // seen in the one behind it.
                    let back = Lasting::back(fight.played.map(|(outcome, _)| outcome));
                    (self.room.behind, self.room.seen_at) = (Some(back), back + Lasting::SEEN);
                    if self.leave_fight(command, fight.played, fight.noted, fight.stood) {
                        return Answer::Wait;
                    }
                    // A fight whose block asks for the results screen: the screen
                    // comes up behind this game frame and the script waits for it.
                    if let Some(results) = fight.results {
                        if let Some(untaken) = self.room.results.take() {
                            Self::give_results(self.room, untaken);
                        }
                        self.room.results = Some(results);
                        self.room.awaits_results = Some(task.number);
                        info!(
                            "game frame {}: the fight of script {} ends with the results \
                             screen, the script waits for it",
                            self.room.game_frame, task.number
                        );
                        return Answer::Wait;
                    }
                }
            }
            _ if command.code == scene::COMMAND_FIGHT => {
                let record = self.package.fight_record(command.c);
                let lesson = self.room.lesson.take();
                let steps = lesson.map(|number| self.package.lesson(number));
                if let Some(number) = lesson {
                    info!(
                        "game frame {}: the fight of script {} is lesson {number}",
                        self.room.game_frame, task.number
                    );
                }
                // A lesson whose list cannot be read is not played.
                let playable = record.as_ref().filter(|_| steps != Some(None));
                let mut runs = (Vec::new(), 0, Vec::new());
                let (mut results, mut seen) = (None, None);
                let surprised = (self.player.as_ref())
                    .zip(self.room.fights.as_ref())
                    .and_then(|(he, rules)| {
                        let first = self.package.fight_things(command.c)?[0]?;
                        let actor = self.actors.get((first as usize).checked_sub(1)?)?;
                        let he = (he.player.position, he.player.angle);
                        Some(surprise(&rules.arc, he, (actor.place, actor.angle)))
                    })
                    .unwrap_or(0);
                info!(
                    "game frame {}: the surprise number of the fight of script {} is {surprised}",
                    self.room.game_frame, task.number
                );
                let centre = (self.player.as_ref())
                    .filter(|_| self.room.fight_place.is_some())
                    .map(|he| Centre::of(he.player.position, he.player.angle.into()));
                // Only a fight with a picture can wait for a command.
                let waits = centre.is_some() && self.room.waits_for_commands();
                let went = (playable.zip(self.room.fights.as_ref())).and_then(|(record, rules)| {
                    let (carried, game_frame) = (&mut self.room.carried, self.room.game_frame);
                    rules.play(
                        &record.block,
                        steps.flatten(),
                        carried,
                        (game_frame, surprised, waits),
                        &mut runs,
                        &mut results,
                        &mut seen,
                    )
                });
                let (played, bout) = Went::apart(went);
                if played.is_none() && bout.is_none() {
                    self.room.leave_out(command.code);
                }
                let on_field = record.map_or(1, |record| record.block.on_field);
                match centre {
                    Some(centre) => {
                        let things = (self.package.fight_things(command.c)).unwrap_or_default();
                        let block = (self.package.fight_record(command.c)).map(|is| is.block);
                        let opponents = (things.to_vec(), block);
                        let played = (played, runs);
                        self.enter_fight(task.number, opponents, centre, played, on_field);
                        if let Some(fight) = &mut self.room.fight {
                            (fight.results, fight.bout) = (results, bout);
                            fight.seen = seen;
                        }
                        return Answer::Wait;
                    }
                    // A fight without a picture has no screen either: what the screen
                    // would give is given at once.
                    None => {
                        let command = (task.number, command.c);
                        if self.leave_fight(command, played, Vec::new(), Vec::new()) {
                            return Answer::Wait;
                        }
                        if let Some(results) = results {
                            Self::give_results(self.room, results);
                        }
                    }
                }
            }
            Some(Scene::Pattern(pattern)) => {
                debug!(
                    "game frame {}: script {} asks the pad for {pattern:?}, not built",
                    self.room.game_frame, task.number
                );
            }
            _ => self.room.leave_out(command.code),
        }
        task.next();
        Answer::GoOn
    }
}

impl Running<'_> {
    fn ask_opponent(
        &mut self,
        stands: &mut Stood,
        (kind, parameter): (u32, u32),
        reaction: bool,
        (mask, attack, creature_in_front): (u8, Option<u8>, bool),
    ) -> Option<[i16; 2]> {
        let asked = Asked::of(kind, parameter)?;
        let named = self.actors.get(stands.index).map(|actor| actor.table);
        let own = named
            .and_then(|table| self.room.own_moves.get(&table))
            .filter(|own| !own.is_empty());
        let table = own.unwrap_or(&self.room.creature_moves);
        let list = table.get(asked.entry)?.clone();
        let actor = self.actors.get_mut(stands.index)?;
        let mut wanted = Vec::new();
        for played in &list {
            let command::Kind::Action(action) = played.kind else {
                continue;
            };
            let record = usize::try_from(action)
                .ok()
                .and_then(|at| actor.actions.get(at));
            let number = record.and_then(|record| record.animation)?;
            if !actor.animations.contains_key(&number) && !actor.shelf.contains_key(&number) {
                return None;
            }
            wanted.push(number);
        }
        for number in wanted {
            if actor.read_animation(number) {
                stands.read.push(number);
            }
        }
        if stands.runner.is_none() {
            stands.runner = Some(actor.runner.clone());
        }
        actor.runner.request_list(list, !reaction);
        actor.pass();
        let times = moves::times(&actor.runner, &actor.actions, kind, Some(asked), reaction);
        // The request names the effect of the move and has the windows of its action
        // made times of the list, which stands at its hold now. A body attack names
        // it by the attack of the actor's move entry (`attack`, where it is known), a
        // swap of a master and his creature by who of the two is in front.
        let number = match kind {
            6 => attack.and_then(|attack| effects::of_body_attack(parameter, attack)),
            21 | 22 => effects::of_swap(kind, creature_in_front),
            _ => (self.room.fights.as_ref())
                .and_then(|rules| effects::of_request(kind, parameter, &rules.tables.spells)),
        };
        stands.effect = number
            .zip(asked.window)
            .map(|(number, action)| ActorEffect {
                actor: stands.index,
                number,
                windows: actor.runner.window_times(&actor.actions, action),
                targets: mask,
                half: match kind {
                    6 => attack.map(effects::half_of_body_attack),
                    _ => None,
                },
            });
        Some(times)
    }

    fn put_figure(&mut self, stood: &mut Vec<Stood>, actors: u8) -> Option<usize> {
        let game_frame = self.room.game_frame;
        let slot = (1..OPPONENTS).find(|slot| actors & 1 << slot != 0)?;
        let number = slot - 1;
        self.end_figure(stood);
        let (place, angle) =
            (self.player.as_ref()).map(|he| (he.player.position, he.player.angle))?;
        let figure =
            (self.actors.iter_mut().enumerate()).find(|(_, actor)| actor.carried == Some(number));
        let Some((index, actor)) = figure else {
            info!(
                "game frame {game_frame}: the carried creature of slot {slot} has no figure and \
                 is not seen"
            );
            return None;
        };
        match stood
            .iter_mut()
            .find(|stands| stands.carried == Some(number))
        {
            Some(stands) => stands.gone = false,
            None => stood.push(Stood {
                index,
                opponent: usize::MAX,
                gone: false,
                waits: false,
                mover: actor.mover.clone(),
                hidden: actor.runner.hidden,
                runner: Some(actor.runner.clone()),
                read: Vec::new(),
                carried: Some(number),
                effect: None,
            }),
        }
        actor.mover = mover::Mover::new(place, angle);
        (actor.place, actor.angle) = (place, angle);
        info!(
            "game frame {game_frame}: the figure of the carried creature of slot {slot}, thing \
             {}, is made at the player's place {place:?}, turned {angle}",
            index + 1
        );
        Some(slot)
    }

    fn end_figure(&mut self, stood: &mut [Stood]) {
        let game_frame = self.room.game_frame;
        for stands in (stood.iter_mut()).filter(|stands| stands.carried.is_some() && !stands.gone) {
            stands.gone = true;
            if let Some(actor) = self.actors.get_mut(stands.index) {
                actor.runner.hidden = true;
                info!(
                    "game frame {game_frame}: the figure of the carried creature, thing {}, is \
                     ended",
                    stands.index + 1
                );
            }
        }
    }

    /// A carried creature's figure is given a place to turn to, by 85 a game frame
    /// (`0x800C0F80` with `0x8001DB68`).
    fn turn_figure(&mut self, index: usize, to: [i32; 3]) {
        if let Some(actor) = self.actors.get_mut(index) {
            (actor.mover.target, actor.mover.turn_step) = (to, CREATURE_TURNS_BY);
            actor.mover.flags |= mover::flag::TURNS;
        }
    }

    fn show_fight(&mut self) -> bool {
        let lasts = self.pass_fight();
        self.pass_hud();
        lasts
    }

    fn pass_hud(&mut self) {
        let Some(fight) = self.room.fight.as_mut() else {
            return;
        };
        let (Some(hud), Some(seen)) = (&mut fight.hud, &mut fight.seen) else {
            return;
        };
        let word = (fight.menu.as_ref()).map_or(0, |menu| menu.drew().leaves_out);
        hud.pass(fight_hud::Told {
            standing: seen.standing,
            master: fight.master,
            opponents_away: word & fight_menu::sets::OPPONENTS_AWAY != 0,
            own_away: word & fight_menu::sets::OWN_AWAY != 0,
            lesson_waits: fight.saying.is_some(),
        });
        seen.pass();
    }

    /// The game frame of the fight's own tasks: see [`Running::show_fight`].
    fn pass_fight(&mut self) -> bool {
        // The fight is taken out of the room while its actors are worked on.
        let Some(mut fight) = self.room.fight.take() else {
            return false;
        };
        if fight.lead < Lasting::LEAD {
            fight.lead += 1;
            // What the fight asks of the sound in front of its picture.
            match (fight.lead, fight.species) {
                (Lasting::SILENT, _) => self.fight_sound(sound::fight_starts()),
                (Lasting::MUSIC, Some(species)) => {
                    self.fight_sound(sound::fight_music(species, fight.boss()))
                }
                _ => {}
            }
            if fight.lead == Lasting::LEAD {
                // The field's fight task has let the brightness fall over one game
                // frame behind the held picture (`0x8001AA50(1)`), and the fade's task
                // has made the step that ends the fall.
                self.room.fade.fade_out(1);
                self.room.fade.step();
                info!(
                    "game frame {}: the opening view of the fight is put in place",
                    self.room.game_frame
                );
                let (master, seen) = (fight.master, fight.seen.as_ref());
                fight.hud = seen.map(|seen| fight_hud::Hud::new(seen.standing, master));
            }
            self.room.fight = Some(fight);
            return true;
        }
        if let Some(trail) = &mut fight.trail {
            *trail += 1;
            let (behind, outcome) = (*trail, fight.played.map(|(outcome, _)| outcome));
            if Lasting::torn_down(behind, outcome) {
                self.fight_sound(sound::fight_torn_down());
            }
            self.room.fight = Some(fight);
            return behind < Lasting::back(outcome);
        }
        fight.frame += 1;
        // The camera task's state 2 asks for the rise in the game frame it puts the
        // opening view in place; the fade's task stands in front of it in the list of
        // a fight, so the first picture that is drawn has two steps of it.
        if fight.frame == 1 {
            self.room.fade.fade_in(Lasting::RISES_OVER);
        }
        // A fight that waits for a command is not through with its runs.
        let left = fight.showing.is_some() || fight.at < fight.runs.len() || fight.bout.is_some();
        let shown = fight.frame >= Lasting::HUB && self.room.fight_asks.is_none();
        let game_frame = self.room.game_frame;
        if let Some(place) = &self.room.fight_place {
            let (cameras, centre) = (&place.cameras, fight.centre);
            let sent = !fight.back && fight.between > fight.done;
            if shown && !left && self.player.is_some() && sent {
                info!("game frame {game_frame}: the end of the fight asks for the first camera");
                fight
                    .eye
                    .pick(cameras, &centre, &place.arc, arena::Pick::Plain);
            }
            // A run along points counts from the player as he stands and faces now.
            let he = match &self.player {
                Some(he) => arena::Stand {
                    place: he.player.position,
                    angle: he.player.angle.into(),
                },
                None => centre.stand(),
            };
            let flew = fight.eye.along();
            fight.eye.pass(cameras, &centre, &place.arc, &he);
            if flew.is_some() != fight.eye.along().is_some() {
                info!(
                    "game frame {game_frame}: the camera's run along points {}",
                    match flew {
                        None => "starts with the next game frame",
                        Some(_) => "is over and the first camera is picked",
                    }
                );
            }
        }
        // Every run the rules have so far is shown and the player's side chooses: the
        // director comes to the choice and stands behind it until the command is there.
        let asked = fight.showing.is_none() && fight.at == fight.runs.len();
        if shown && asked && fight.bout.is_some() && self.ask(&mut fight) {
            self.room.fight = Some(fight);
            return true;
        }
        let left = fight.showing.is_some() || fight.at < fight.runs.len();
        if left && shown {
            let number = fight.at;
            // The director's states in front of the run; a message of a lesson holds
            // them while its window is up.
            if fight.showing.is_none() && fight.between < fight.runs[number].before {
                if !self.say(&mut fight) {
                    fight.between += 1;
                }
                self.room.fight = Some(fight);
                return true;
            }
            // The runner's first state asks for a camera, behind the camera's pass.
            let asked = (fight.runs[number].camera).filter(|_| fight.showing.is_none());
            if let (Some(pick), Some(place)) = (asked, &self.room.fight_place) {
                info!("game frame {game_frame}: run {number} of the fight asks for {pick:?}");
                (fight.eye).pick(&place.cameras, &fight.centre, &place.arc, pick);
            }
            let showing = match &mut fight.showing {
                Some(showing) => showing,
                none => {
                    let run = fight.runs[number].clone();
                    info!(
                        "game frame {game_frame}: run {number} of the fight starts: kind {} \
                         with parameter {} of the slots {:#04x} at the slots {:#04x}",
                        run.kind, run.parameter, run.actors, run.targets
                    );
                    // What the starts of the turns in front of it wrote into the records.
                    if let Some(seen) = &mut fight.seen {
                        for &(slot, word) in &run.ticked {
                            seen.tick(slot, word);
                            info!(
                                "game frame {game_frame}: the start of a turn has given slot \
                                 {slot} the status word {word:#06x}"
                            );
                        }
                    }
                    // The entrance of a carried creature: the state in front of this
                    // runner has made its actor (`0x800BBE98`).
                    if run.kind == 2 && run.targets == 0 {
                        // The fighter becomes the picked slot with it.
                        if let Some(slot) = self.put_figure(&mut fight.stood, run.actors) {
                            fight.fighter = slot;
                        }
                    }
                    // A runner of the swap whose actor waits: the state in front of it
                    // has swapped master and creature (`0x800A2A78` in `0x800BC134`
                    // and `0x800BC22C`), so its actor stands on the field now and the
                    // other one of the pair waits.
                    if matches!(run.kind, 21 | 22) {
                        let comes = (fight.stood.iter())
                            .find(|stands| {
                                let slot = stands.opponent.wrapping_add(OPPONENTS);
                                stands.carried.is_none()
                                    && stands.waits
                                    && slot < SLOTS
                                    && run.actors >> slot & 1 != 0
                            })
                            .map(|stands| stands.opponent);
                        if let Some(comes) = comes {
                            for stands in &mut fight.stood {
                                if stands.carried.is_none() {
                                    stands.waits = stands.opponent != comes;
                                }
                            }
                            // The word `0x800B146C` as `0x800A2A44` answers it.
                            fight.master = match comes {
                                0 => 1,
                                _ => 2,
                            };
                            info!(
                                "game frame {game_frame}: master and creature are swapped: \
                                 opponent {comes} stands on the field now"
                            );
                        }
                    }
                    // The second runner of a leave: the state in front of it has made
                    // the player the fighter again (`0x800BBFD0`).
                    if run.kind == 3 && run.actors == 1 << PLAYER && fight.fighter != PLAYER {
                        info!(
                            "game frame {game_frame}: the player is the fighter of the own \
                             side again, in place of slot {}",
                            fight.fighter
                        );
                        fight.fighter = PLAYER;
                    }
                    none.insert(Showing::of(run))
                }
            };
            let mut field = OnField {
                running: self,
                stood: &mut fight.stood,
                waited: &mut fight.waited,
                eye: &mut fight.eye,
                seen: &mut fight.seen,
                run: number,
                targets: showing.run().targets,
                actors: showing.run().actors,
                fighter: fight.fighter,
                creature_in_front: fight.master == 2,
            };
            if showing.step(&mut field) {
                info!(
                    "game frame {game_frame}: run {number} of the fight is over at {} of its \
                     clock, behind the key moment {} and the actor's end {}",
                    showing.clock(),
                    showing.longest(),
                    showing.actor().end
                );
                // The end test of a capture ends the loops of the player's capture
                // (the half word `0x8008D710`).
                if matches!(showing.run().kind, 5 | 26)
                    && let Some(he) = &mut self.player
                {
                    he.player.fight.loops_end = true;
                }
                // The state behind the second runner of a leave ends the figure of the
                // creature (`0x800BC04C` with `0x800C0D70`).
                if showing.run().kind == 3 && showing.run().actors == 1 << PLAYER {
                    self.end_figure(&mut fight.stood);
                }
                fight.showing = None;
                fight.at += 1;
                fight.between = 1;
            }
            self.room.fight = Some(fight);
            return true;
        }
        if shown && self.player.is_some() && !fight.back && fight.between <= fight.done {
            if !self.say(&mut fight) {
                // The director's end ends the tasks of the HUD (`0x800BFD88`).
                if fight.between == fight.done {
                    fight.hud = None;
                }
                fight.between += 1;
            }
            self.room.fight = Some(fight);
            return true;
        }
        let lasts = match (shown, &mut self.player) {
            // The case of the outcome sends him back and names the waiting state, whose
            // first pass is the next game frame's.
            (true, Some(he)) if !fight.back => {
                fight.back = true;
                fight.hud = None;
                let outcome = fight.played.map(|(outcome, _)| outcome);
                he.end_fight(outcome == Some(ended::WON));
                self.room.orders.end_fight();
                // The case of a loss: a white grows over the picture and stays.
                if outcome == Some(ended::LOST) {
                    self.room.fade.whiten(leave_forest::LOST_WHITE);
                    info!(
                        "game frame {}: the fight is lost, a white grows over its picture \
                         for {} game frames",
                        self.room.game_frame,
                        leave_forest::LOST_WHITE
                    );
                }
                if let Some(outcome) = outcome {
                    self.fight_sound(sound::fight_ends(outcome, fight.boss()));
                }
                true
            }
            (true, Some(he)) => {
                let outcome = fight.played.map(|(outcome, _)| outcome);
                let he_is = match Lasting::waits_for_his_action(outcome) {
                    true => he.player.played_out(),
                    // The state behind a loss (`0x800BA614`) does nothing until the
                    // white is whole; from then on it takes the hold off him in every
                    // pass and waits for his states of a fight like the others.
                    false if outcome == Some(ended::LOST) => {
                        let whole = self.room.fade.at_rest();
                        if whole {
                            he.get_up();
                        }
                        whole && he.through()
                    }
                    false => he.through(),
                };
                let guard = !he_is && fight.waited >= Lasting::WAITS_AT_MOST;
                if guard && fight.paused == 0 {
                    warn!(
                        "the fight of script {} has waited {} game frames for the player's \
                         way back and goes on without it",
                        fight.script, fight.waited
                    );
                }
                if he_is || guard {
                    fight.paused += 1;
                    fight.paused <= fight.pause
                } else {
                    fight.waited += 1;
                    true
                }
            }
            _ => fight.frame < fight.ends,
        };
        if !lasts {
            info!(
                "game frame {}: the pause behind the fight is through, its last picture is \
                 held for the way back",
                self.room.game_frame
            );
            fight.trail = Some(0);
            if let Some((outcome, _)) = fight.played {
                self.fight_sound(sound::fight_paused(outcome, fight.boss()));
            }
        }
        self.room.fight = Some(fight);
        true
    }

    fn fight_sound(&mut self, asked: Vec<sound::FightAsks>) {
        let game_frame = self.room.game_frame;
        let asked = asked.into_iter().map(Heard::Fight);
        (self.room.music).extend(asked.map(|asked| (game_frame, asked)));
    }

    fn say(&mut self, fight: &mut Lasting) -> bool {
        let told = (fight.told.get(fight.told_at).copied()).filter(|told| told.run == fight.at);
        let Some(told) = told else {
            return false;
        };
        let game_frame = self.room.game_frame;
        match fight.between.checked_sub(told.at) {
            Some(0) => {
                let group = usize::try_from(told.message).ok();
                let pages = group
                    .and_then(|group| self.room.groups.get(group))
                    .map(|group| group.pages.len());
                let Some((group, pages)) = group.zip(pages) else {
                    warn!(
                        "the room's text has no group {} for the message of the lesson",
                        told.message
                    );
                    return false;
                };
                info!(
                    "game frame {game_frame}: the lesson says group {group} of the room's text \
                     in a window of {pages} pages"
                );
                let asked = window::Asked::taught(self.setting(), pages);
                let said = Said {
                    thing: Lasting::TEACHER,
                    // It speaks to nobody: its picture is on the left whatever is seen.
                    other: Lasting::TEACHER as u32,
                    group,
                    picture_left: true,
                    open_answers: 0,
                    place: window::Where::Bottom,
                    pass: window::Pass::default(),
                    passes: 0,
                };
                fight.saying = Some((Window::open(asked, 0), said));
                false
            }
            Some(1) => match &mut fight.saying {
                Some((window, _)) if window.taken().is_some_and(|taken| taken.at_end) => {
                    info!("game frame {game_frame}: the lesson closes the window of its message");
                    window.close();
                    false
                }
                Some(_) => true,
                None => false,
            },
            Some(2) => {
                if (fight.saying.as_ref()).is_some_and(|(window, _)| !window.is_gone()) {
                    return true;
                }
                if fight.saying.take().is_some() {
                    info!(
                        "game frame {game_frame}: the window of the lesson's message is gone and \
                         the hub goes on"
                    );
                }
                fight.told_at += 1;
                false
            }
            _ => false,
        }
    }

    fn ask(&mut self, fight: &mut Lasting) -> bool {
        let asked = (fight.bout.as_ref())
            .and_then(|bout| Some((bout.fight.asked_at()?, bout.fight.fighter)));
        let Some((at, slot)) = asked else {
            return false;
        };
        let game_frame = self.room.game_frame;
        if fight.between < at {
            if !self.say(fight) {
                fight.between += 1;
            }
            return true;
        }
        if let Some(choice) = fight.command.take() {
            return self.take_command(fight, slot, choice);
        }
        if fight.asks.is_none() {
            info!(
                "game frame {game_frame}: the fight waits for the command of slot {slot}, \
                 behind {} of its runs: its menu is made",
                fight.runs.len()
            );
            fight.asks = Some(slot);
            fight.between += 1;
            // Every run the rules decided is shown: the picture has their records.
            if let (Some(seen), Some(bout)) = (&mut fight.seen, &fight.bout) {
                let points = |records: &[Option<Record>; SLOTS]| {
                    (records.each_ref()).map(|record| record.as_ref().map(|is| (is.hp, is.mp)))
                };
                let (shown, decided) = (points(&seen.records), points(&bout.fight.records));
                if shown != decided || seen.counts() {
                    warn!(
                        "game frame {game_frame}: the picture of the fight has the HP and MP \
                         {shown:?} where the rules have {decided:?}"
                    );
                }
                seen.records = bout.fight.records.clone();
                seen.standing = bout.fight.opponents();
            }
        }
        self.pass_menu(fight);
        true
    }

    fn pass_menu(&mut self, fight: &mut Lasting) {
        let (Some(bout), Some(rules)) = (&mut fight.bout, self.room.fights.as_ref()) else {
            return;
        };
        let game_frame = self.room.game_frame;
        let places = item_places(&self.room.carried.keeping);
        let lists = fight_menu::Lists {
            fight: &bout.fight,
            tables: &rules.tables,
            places: &places,
        };
        let kept = self.room.menu_kept;
        let menu = (fight.menu).get_or_insert_with(|| fight_menu::Menu::new(lists.offer(kept)));
        let pass = menu.pass(&self.room.pad, &lists);
        // The preview stands behind the drawing and in front of the input: the resolver
        // decides the command for the box's fighter, with draws of the fight's own.
        let before = fight.preview;
        fight.preview = lists.preview(menu, game_frame % 2 == 1, &mut bout.random);
        // A list of a creature notes the mana its row costs as a loss, on its own level,
        // where nothing else is previewed.
        fight.preview.1 -= lists.row_costs(menu);
        if fight.preview != before {
            info!(
                "game frame {game_frame}: the menu of the fight previews {} HP and {} MP for \
                 slot {}",
                fight.preview.0,
                fight.preview.1,
                menu.drew().subject
            );
        }
        let (level, command, message) = (menu.level(), menu.command(), menu.message());
        // The marked changed: the fighter turns to their mean place, and the camera is
        // asked with their mask ("The camera and the turn").
        if let Some(marked) = pass.marked {
            let named = |stands: &&Stood| marked >> (stands.opponent + OPPONENTS) & 1 != 0;
            let places: Vec<[i32; 3]> = (fight.stood.iter())
                .filter(|stands| stands.on_field())
                .filter(named)
                .filter_map(|stands| Some(self.actors.get(stands.index)?.mover.position))
                .collect();
            let mean = runner::mean(&places).filter(|_| self.player.is_some());
            // The slot that chooses is turned (`0x800BC638` with the acting slot): the
            // player by his order, his creature by its actor's turn.
            let figure = (fight.asks.filter(|slot| *slot != PLAYER))
                .and_then(|slot| fight.stood.iter().find(|stands| stands.stands_for(slot)))
                .map(|stands| stands.index);
            match (mean, fight.asks, figure) {
                (Some(mean), Some(PLAYER) | None, _) => self.room.orders.face(mean),
                (Some(mean), _, Some(index)) => self.turn_figure(index, mean),
                _ => {}
            }
            // The camera stands lower for a creature that chooses, by its own height.
            let lift = (fight.asks.filter(|slot| *slot != PLAYER))
                .and_then(|slot| fight.stood.iter().find(|stands| stands.stands_for(slot)))
                .and_then(|stands| self.actors.get(stands.index))
                .and_then(|actor| actor.pose.get(arena::LIFT_BONE)?.as_ref())
                .map_or(0, |bone| arena::CREATURE_LIFT + bone.position[1]);
            let swings = (self.room.fight_place.as_ref()).is_some_and(|place| {
                let asked = (fight.at_once, marked);
                (fight.eye).mark(&place.cameras, &fight.centre, &place.arc, asked, lift)
            });
            info!(
                "game frame {game_frame}: the menu of the fight marks the slots {marked:#04x}: \
                 the fighter turns to {mean:?}, and the camera {}, lifted by {lift}",
                match swings {
                    true => "swings to the menu's view of them",
                    false => "stays",
                }
            );
        }
        if !pass.sounds.is_empty() {
            info!(
                "game frame {game_frame}: the menu of the fight is on level {level} at command \
                 {command} and plays the sounds {:?}{}",
                pass.sounds,
                match message {
                    Some(text) => format!(", with its text {text}"),
                    None => String::new(),
                }
            );
        }
        let sounds = pass
            .sounds
            .iter()
            .map(|&sound| (game_frame, Heard::Sound(sound)));
        self.room.music.extend(sounds);
        let Some(chosen) = pass.chosen else {
            return;
        };
        self.room.menu_kept = menu.kept();
        fight.menu = None;
        fight.preview = (0, 0);
        fight.command = chosen.choice();
        info!(
            "game frame {game_frame}: the menu of the fight hands over kind {} with parameter \
             {} of the slots {:#04x} at the slots {:#04x}",
            chosen.kind, chosen.parameter, chosen.actors, chosen.targets
        );
    }

    /// The state behind the menu has found the command (`0x800BBB10`): the rules take
    /// it and go on to the next choice or the end, and what they add is the picture's.
    /// A command the rules refuse spends no turn: the menu is made again and the fight
    /// stands (the game's menu hands over nothing its own tests refuse). Answers
    /// whether the fight stands.
    fn take_command(&mut self, fight: &mut Lasting, slot: usize, choice: Choice) -> bool {
        let game_frame = self.room.game_frame;
        let (Some(mut bout), Some(rules)) = (fight.bout.take(), self.room.fights.as_ref()) else {
            return false;
        };
        if let Err(refusal) = rules.answer(&mut bout, Some(choice)) {
            warn!(
                "game frame {game_frame}: the rules refuse {choice:?} of the menu \
                 ({refusal:?}); the menu is made again"
            );
            fight.bout = Some(bout);
            return true;
        }
        fight.asks = None;
        let told = std::mem::take(&mut fight.told);
        let mut runs: Runs = (std::mem::take(&mut fight.runs), fight.done, told);
        let mut results = None;
        let carried = &mut self.room.carried;
        let went = rules.go_on(bout, true, carried, &mut runs, &mut results);
        (fight.runs, fight.done, fight.told) = runs;
        let has = fight.runs.len();
        match went {
            Some(Went::Asks(bout)) => {
                info!(
                    "game frame {game_frame}: slot {slot} of the fight has its command, the \
                     fight has {has} runs now"
                );
                fight.bout = Some(bout);
            }
            Some(Went::Over { outcome, turns }) => {
                info!(
                    "game frame {game_frame}: slot {slot} of the fight has its command, the \
                     fight has {has} runs now and is decided with them"
                );
                fight.played = Some((outcome, turns));
                fight.pause = Lasting::pause(outcome);
                fight.ends = Lasting::CAMERA_STANDS + fight.pause;
                fight.results = results;
            }
            None => warn!(
                "game frame {game_frame}: the rules cannot go on with the fight; it ends \
                 undecided"
            ),
        }
        false
    }

    fn look_at_a_request(&mut self) {
        let Some(asks) = self.room.fight_asks else {
            return;
        };
        let since = (self.room.fight.as_ref())
            .and_then(|fight| fight.frame.checked_sub(Lasting::CAMERA_STANDS));
        let Some(since @ (0 | 1)) = since else {
            return;
        };
        // The fight is taken out of the room while its actors are worked on.
        let Some(mut fight) = self.room.fight.take() else {
            return;
        };
        let game_frame = self.room.game_frame;
        for stands in &mut fight.stood {
            let thing = stands.index + 1;
            if since == 0 {
                let with = (1 << PLAYER, self.room.fight_attack, false);
                match self.ask_opponent(stands, asks, false, with) {
                    Some([key, whole]) => {
                        info!(
                            "game frame {game_frame}: thing {thing} is asked for the move of \
                             kind {} with parameter {}: its key moment comes {key} game \
                             frames after it is let go, its end {whole}",
                            asks.0, asks.1
                        );
                        // The look lasts until the move is over.
                        let over = fight.frame + 2 + u32::try_from(whole).unwrap_or(0);
                        fight.ends = fight.ends.max(over);
                    }
                    None => info!(
                        "game frame {game_frame}: thing {thing} is not asked: kind {} has no \
                         list, or the model lacks an action of it",
                        asks.0
                    ),
                }
            } else if let Some(actor) = (self.actors.get_mut(stands.index))
                .filter(|actor| stands.runner.is_some() && actor.runner.ready())
            {
                actor.runner.goes_on();
                self.room.effects_started.extend(stands.effect.take());
                info!("game frame {game_frame}: thing {thing} is let go from its hold");
            }
        }
        self.room.fight = Some(fight);
    }

    fn enter_fight(
        &mut self,
        script: i32,
        (things, block): (Vec<Option<u32>>, Option<Block>),
        centre: Centre,
        (played, runs): (Option<(u32, usize)>, Runs),
        on_field: usize,
    ) {
        let game_frame = self.room.game_frame;
        let (runs, done, told) = runs;
        self.room.menu_kept = 0;
        // The kinds of the runs in order, the kinds of their reactions in brackets.
        let kinds: Vec<String> = (runs.iter())
            .map(|run| match run.reactions.as_slice() {
                [] => run.kind.to_string(),
                shown => {
                    let shown: Vec<String> =
                        shown.iter().map(|shown| shown.kind.to_string()).collect();
                    format!("{}[{}]", run.kind, shown.join(","))
                }
            })
            .collect();
        info!(
            "game frame {game_frame}: the picture of the fight has {} runs of the action \
             runner: {}",
            runs.len(),
            kinds.join(" ")
        );
        let (place, angle) = (self.player.as_ref()).map_or(([0; 3], 0), |he| {
            (he.player.position, i32::from(he.player.angle))
        });
        // Where the spawner puts opponent n of the field.
        let stands_at = |opponent: usize| {
            let turn = angle + Lasting::turned(on_field, opponent);
            let at = [
                place[0] + sine(turn) * Lasting::TO_THE_OPPONENTS / 4096,
                place[1],
                place[2] + cosine(turn) * Lasting::TO_THE_OPPONENTS / 4096,
            ];
            (turn, at)
        };
        let places: Vec<[i32; 3]> = (0..on_field.min(things.len()))
            .map(|opponent| stands_at(opponent).1)
            .collect();
        if let (Some(mean), true) = (runner::mean(&places), self.player.is_some()) {
            self.room.orders.start_fight(mean);
        }
        let (mode, count) = (block.as_ref()).map_or((Lasting::SINGLE, on_field), |block| {
            (block.mode, block.specs.len())
        });
        let mut stood = Vec::new();
        for (opponent, thing) in things.iter().enumerate().take(count) {
            let index = thing.and_then(|thing| (thing as usize).checked_sub(1));
            let Some((index, actor)) = index.and_then(|at| Some((at, self.actors.get_mut(at)?)))
            else {
                info!(
                    "game frame {game_frame}: opponent {opponent} of the fight has no actor of \
                     the room and is not seen"
                );
                continue;
            };
            let waits = opponent >= on_field;
            stood.push(Stood {
                index,
                opponent,
                gone: false,
                waits,
                mover: actor.mover.clone(),
                hidden: actor.runner.hidden,
                runner: Some(actor.runner.clone()),
                read: Vec::new(),
                carried: None,
                effect: None,
            });
            // The copy is a new actor: in the action and at the frame of the room's
            // own, without its list, neither walking over the ground nor carried by
            // its animation. The copy of the first opponent is made before the room's
            // actors are hidden and is hidden when the room's own was, every other
            // copy is made behind that and is hidden; the fight of a single opponent
            // (mode 1) then shows its copy.
            let (turn, at) = stands_at(opponent % on_field.max(1));
            actor.mover = mover::Mover::new(at, (turn + 2048) as i16);
            actor.runner = actor.runner.copied();
            let made = actor.mate.is_some();
            if made {
                let draw = Draws::draw(&mut self.room.carried.random);
                let playback = &mut actor.runner.playback;
                playback.counter = playback.span.random(draw);
            }
            actor.runner.hidden = match mode {
                Lasting::SINGLE => false,
                _ => !made && (opponent > 0 || actor.runner.hidden),
            };
            (actor.place, actor.angle) = (at, actor.mover.turn);
            info!(
                "game frame {game_frame}: thing {} stands for opponent {opponent} of the fight \
                 at {at:?}, turned {}, in action {:?} at frame {}{}{}",
                index + 1,
                actor.angle,
                actor.runner.action,
                actor.runner.playback.counter,
                if actor.runner.hidden { ", hidden" } else { "" },
                if waits { ", and waits" } else { "" }
            );
        }
        let mut noted = Vec::new();
        for (index, actor) in self.actors.iter_mut().enumerate() {
            if actor.runner.hidden || stood.iter().any(|stands| stands.index == index) {
                continue;
            }
            actor.runner.hidden = true;
            noted.push(index);
        }
        // Their tasks are held with it: each stands at this frame of its action until
        // the way back.
        let stand: Vec<_> = (noted.iter())
            .filter_map(|&index| self.actors.get(index))
            .map(|actor| (actor.runner.action, actor.runner.playback.counter))
            .collect();
        let pause = played.map_or(0, |(outcome, _)| Lasting::pause(outcome));
        let ends = Lasting::CAMERA_STANDS + pause;
        let whose = match script {
            Lasting::NO_SCRIPT => "a touch".to_string(),
            script => format!("script {script}"),
        };
        info!(
            "game frame {game_frame}: the fight of {whose} is on screen for {ends} game \
             frames and those of its runs round the centre {:?}; the actors {noted:?} of the \
             room are hidden for it and held at the frames {stand:?} of their actions",
            centre.place
        );
        self.room.fight = Some(Lasting {
            script,
            centre,
            frame: 0,
            ends,
            pause,
            back: false,
            lead: 0,
            paused: 0,
            trail: None,
            played,
            runs,
            at: 0,
            showing: None,
            between: 0,
            done,
            told,
            told_at: 0,
            saying: None,
            waited: 0,
            noted,
            stood,
            eye: arena::Eye::default(),
            species: block.and_then(|block| Some(block.specs.first()?.species)),
            roamer: None,
            results: None,
            bout: None,
            asks: None,
            fighter: PLAYER,
            menu: None,
            command: None,
            preview: (0, 0),
            at_once: on_field,
            hud: None,
            seen: None,
            master: u8::from(mode == 0),
        });
    }

    fn leave_fight(
        &mut self,
        (script, record): (i32, u32),
        played: Option<(u32, usize)>,
        noted: Vec<usize>,
        stood: Vec<Stood>,
    ) -> bool {
        let game_frame = self.room.game_frame;
        self.give_back_the_room(noted, stood);
        if let Some((outcome, turns)) = played {
            self.room.outcome = outcome;
            let left = self.room.carried.player.as_ref().map_or(0, |he| he.hp);
            info!(
                "game frame {game_frame}: the fight of script {script} is decided by the \
                 rules: outcome {outcome} after {turns} turns, the player leaves with {left} HP"
            );
            let is_final = (self.package.fight_record(record)).is_some_and(|r| r.loss_is_final);
            match self.room.forest_place {
                Some(place) if outcome == ended::LOST && is_final => {
                    let room = leave_forest::lost(&mut self.room.carried, place);
                    info!(
                        "game frame {game_frame}: the loss is final and takes the way back to \
                         the gates: story variable {FOREST_VARIABLE} is {}, story flag {} is \
                         set, the game goes to the stored room {room}",
                        self.room.carried.variable(FOREST_VARIABLE),
                        leave_forest::LOST_FLAG
                    );
                    self.room.leaves = Some(room.into());
                    return true;
                }
                None if outcome == ended::LOST && is_final => {
                    warn!("the loss is final and the room lies in no forest: the script goes on");
                }
                _ => {}
            }
        }
        let first = (self.package.fight_things(record))
            .and_then(|things| things[0])
            .map(|thing| thing as usize);
        if let Some(thing) = first
            && let Some(actor) = self.actor(thing)
        {
            actor.runner.hidden = true;
            actor.mover.flags &= !mover::flag::WALKS;
            info!("game frame {game_frame}: the fight takes thing {thing} out of the picture");
        }
        let set = (self.room.kept_set).and_then(|set| usize::try_from(set).ok());
        if let (Some(set), Some(walker)) = (set, &mut self.player) {
            info!(
                "game frame {game_frame}: script {script} goes on over a fight; behind it the \
                 player has animation set {set}, which brings the animations {:?}",
                walker.read_set(set)
            );
        }
        false
    }

    fn leave_touch(
        &mut self,
        touched: Touched,
        played: Option<(u32, usize)>,
        noted: Vec<usize>,
        stood: Vec<Stood>,
    ) {
        let game_frame = self.room.game_frame;
        self.give_back_the_room(noted, stood);
        self.room.roamers_let_go = true;
        for roams in self
            .actors
            .iter_mut()
            .filter_map(|actor| actor.roams.as_mut())
        {
            roams.held = false;
        }
        if let Some(actor) = self.actors.get_mut(touched.index) {
            actor.roams = None;
            actor.runner.hidden = true;
            actor.mover.flags &= !mover::flag::WALKS;
        }
        let Some((outcome, turns)) = played else {
            return;
        };
        self.room.outcome = outcome;
        let forgotten = matches!(outcome, ended::WON | ended::RAN);
        // In the corridor of the second round the word is the piece's own.
        match (forgotten, &mut self.room.carried.corridor) {
            (false, _) => {}
            (true, Some(corridor)) => corridor.forget(touched.number),
            (true, None) => (self.room.carried.roaming).forget(touched.slot, touched.number),
        }
        let left = self.room.carried.player.as_ref().map_or(0, |he| he.hp);
        info!(
            "game frame {game_frame}: the fight with the roaming creature, actor {}, is decided \
             by the rules: outcome {outcome} after {turns} turns, the player leaves with {left} \
             HP; the creature is gone from the room, and its number {} is {}",
            touched.index,
            touched.number,
            match forgotten {
                true => "forgotten",
                false => "kept",
            }
        );
        if outcome == ended::LOST {
            let (_, place, _) = touched.slot;
            let room = leave_forest::lost(&mut self.room.carried, place as i16);
            info!(
                "game frame {game_frame}: the fight is lost and takes the way back to the \
                 gates: story variable {FOREST_VARIABLE} is {}, story flag {} is set, the \
                 game goes to the stored room {room}",
                self.room.carried.variable(FOREST_VARIABLE),
                leave_forest::LOST_FLAG
            );
            self.room.leaves = Some(room.into());
        }
    }

    fn give_results(room: &mut Room, mut results: Results) {
        let game_frame = room.game_frame;
        let Some(rules) = room.fights.as_ref() else {
            return;
        };
        let carried = &mut room.carried;
        for thing in results.won.iter().flatten() {
            let given = items::Kind::of(thing.kind.into()).is_some_and(|kind| {
                (carried.keeping).give(thing.number.into(), 1, kind, carried.worn)
            });
            info!(
                "game frame {game_frame}: the results screen: an opponent leaves thing {} of \
                 kind {}, {}",
                thing.number,
                thing.kind,
                match given {
                    true => "which the player gets",
                    false => "which he cannot carry",
                }
            );
        }
        let Some(he) = carried.player.take() else {
            return;
        };
        let (before, mut pad) = (he.level, Pad::default());
        let mut real = vec![he];
        real.extend(carried.creatures.carried().cloned());
        // A press in every second pass: the counting, then each level confirmed away.
        for pass in 0..RESULTS_PASSES {
            let held = if pass % 2 == 0 { pad::CROSS } else { 0 };
            pad.read(held, Repeat::of(2), pass);
            let passed = results.pass(&rules.tables, &pad, &mut real);
            if passed.is_none_or(|passed| passed.leaves) {
                break;
            }
        }
        let mut real = real.into_iter();
        let Some(he) = real.next() else {
            return;
        };
        for (creature, left) in carried.creatures.carried_mut().zip(real) {
            *creature = left;
        }
        info!(
            "game frame {game_frame}: the results screen: the player's sum of experience is \
             {:#x}, his level {} (it was {before}), his experience word {:#x}; the screen \
             itself is not shown yet",
            results.sums[0], he.level, he.experience
        );
        carried.player = Some(he);
    }

    /// What every way back from a fight gives the room again: the player's order record
    /// is out of the fight, the actors that stood for the opponents are as the room had
    /// them, and the noted actors are shown.
    fn give_back_the_room(&mut self, noted: Vec<usize>, stood: Vec<Stood>) {
        let game_frame = self.room.game_frame;
        self.room.orders.end_fight();
        if let Some(he) = &mut self.player {
            he.player.set_shown(true);
        }
        // The build's guard: a fight that did not end by its flow leaves no order state
        // of a fight behind, which no script could take him out of.
        let orders = &mut self.room.orders;
        if (player::order_state::ASKED..=player::order_state::FACES).contains(&orders.state) {
            warn!(
                "game frame {game_frame}: the fight leaves the player in order state {}; he \
                 waits in state 1 in its place",
                orders.state
            );
            orders.state = player::order_state::WAITS;
        }
        for stands in stood {
            if let Some(actor) = self.actors.get_mut(stands.index) {
                // What a move of the fight has done to it is gone with the fight.
                if let Some(runner) = stands.runner {
                    actor.runner = runner;
                }
                for number in stands.read {
                    actor.give_back(number);
                }
                (actor.place, actor.angle) = (stands.mover.position, stands.mover.turn);
                (actor.mover, actor.runner.hidden) = (stands.mover, stands.hidden);
            }
        }
        for &index in &noted {
            if let Some(actor) = self.actors.get_mut(index) {
                actor.runner.hidden = false;
            }
        }
        let stand: Vec<_> = (noted.iter())
            .filter_map(|&index| self.actors.get(index))
            .map(|actor| (actor.runner.action, actor.runner.playback.counter))
            .collect();
        if !noted.is_empty() {
            info!(
                "game frame {game_frame}: the actors {noted:?} of the room are shown again, \
                 at the frames {stand:?} of their actions"
            );
        }
    }

    fn touch(&mut self) {
        let walks = self.room.leaving.is_none() && self.room.selector < 0;
        if self.room.fight.is_some() || !walks || self.room.fight_place.is_none() {
            return;
        }
        let (Some(he), Some(rules)) = (self.player.as_ref(), self.room.fights.as_ref()) else {
            return;
        };
        if !rules.touches {
            return;
        }
        let his = (he.player.position, he.player.angle);
        let touched = self.actors.iter().enumerate().find_map(|(index, actor)| {
            let roams = actor.roams.as_ref()?;
            // The scale at +0x140 of the actor: the third value of its model's.
            let scale = (actor.model_scale).map_or(jc_formats::skeleton::ONE, |scale| scale[2]);
            let reach = roaming::reach(scale);
            let touched = Touched {
                index,
                number: roams.stand.number,
                slot: roams.slot,
            };
            (actor.mover.within(his.0, reach)).then(|| (touched, roams.stand.block(), reach))
        });
        let Some((touched, block, reach)) = touched else {
            return;
        };
        let game_frame = self.room.game_frame;
        let actor = &self.actors[touched.index];
        let surprised = surprise(&rules.arc, his, (actor.place, actor.angle));
        info!(
            "game frame {game_frame}: the player at {:?} touches the roaming creature, actor \
             {}, at {:?} (nearer than {reach}): a fight with its group, species {:?}; the \
             surprise number is {surprised}",
            his.0,
            touched.index,
            actor.mover.position,
            (block.specs.iter())
                .map(|spec| spec.species)
                .collect::<Vec<_>>()
        );
        let mut runs = (Vec::new(), 0, Vec::new());
        let carried = &mut self.room.carried;
        let (mut results, mut seen) = (None, None);
        let at = (
            game_frame,
            surprised,
            self.room.fight_menu && self.room.fight_asks.is_none(),
        );
        let went = rules.play(
            &block,
            None,
            carried,
            at,
            &mut runs,
            &mut results,
            &mut seen,
        );
        let (played, bout) = Went::apart(went);
        if played.is_none() && bout.is_none() {
            warn!("game frame {game_frame}: the rules cannot play the fight; it has no runs");
        }
        let on_field = block.on_field.min(block.specs.len());
        let centre = Centre::of(his.0, his.1.into());
        let named = self.actors.get(touched.index).map(|actor| actor.table);
        let own = named
            .and_then(|table| self.room.own_moves.get(&table))
            .filter(|own| !own.is_empty());
        let list = (own.unwrap_or(&self.room.creature_moves).first()).cloned();
        if let (Some(list), Some(actor)) = (list, self.actors.get_mut(touched.index)) {
            actor.runner.request_list(list, false);
            actor.pass();
            info!(
                "game frame {game_frame}: the touched creature is given entry 0 of its move \
                 table and stands in action {:?}",
                actor.runner.action
            );
        }
        // The creature is thing n + 1 of a script; the others of its group are the
        // actors made for them with the room.
        let thing = |index: usize| u32::try_from(index + 1).ok();
        let mut things = vec![thing(touched.index)];
        things.extend((1..block.specs.len()).map(|opponent| {
            let mate = Some((touched.index, opponent));
            (self.actors.iter().position(|actor| actor.mate == mate)).and_then(thing)
        }));
        self.enter_fight(
            Lasting::NO_SCRIPT,
            (things, Some(block)),
            centre,
            (played, runs),
            on_field,
        );
        // The task of every other creature is held with its actor's own.
        for (index, actor) in self.actors.iter_mut().enumerate() {
            if let Some(roams) = (actor.roams.as_mut()).filter(|_| index != touched.index) {
                roams.held = true;
            }
        }
        if let Some(fight) = &mut self.room.fight {
            (fight.roamer, fight.results) = (Some(touched), results);
            (fight.bout, fight.seen) = (bout, seen);
        }
    }

    /// One game frame of a fight that a touch has started, in place of the script that
    /// waits for a fight of its own: its picture, and behind it the way back.
    fn pass_touch(&mut self) {
        let lasts = self.show_fight();
        // The check option `--fight-asks` looks at a request in such a fight too.
        self.look_at_a_request();
        if lasts {
            return;
        }
        let Some(fight) = self.room.fight.take() else {
            return;
        };
        // The hold of the screen ends in the next pass and the room is seen in the one
        // behind it.
        let back = Lasting::back(fight.played.map(|(outcome, _)| outcome));
        (self.room.behind, self.room.seen_at) = (Some(back), back + Lasting::SEEN);
        if let Some(touched) = fight.roamer {
            self.leave_touch(touched, fight.played, fight.noted, fight.stood);
            // A screen that nobody took (game frames that were run ahead) gives what
            // it has in front of the next one.
            if let Some(untaken) = self.room.results.take() {
                Self::give_results(self.room, untaken);
            }
            self.room.results = fight.results;
        }
    }
}

struct OnField<'a, 'b> {
    running: &'a mut Running<'b>,
    stood: &'a mut Vec<Stood>,
    waited: &'a mut u32,
    /// The camera of the fight, which a capture asks for a run along points.
    eye: &'a mut arena::Eye,
    /// The fighters as the picture has them, for the numbers of the run.
    seen: &'a mut Option<fight_hud::Seen>,
    /// The number of the run, for the log, and its target mask.
    run: usize,
    targets: u8,
    /// The actor mask of the run, which the request of a summon and of a leave has in
    /// the place of the targets.
    actors: u8,
    /// The fighter of the own side ([`Lasting::fighter`]).
    fighter: usize,
    /// Whether the creature of a master stands in front of him (the word `0x800B146C`,
    /// [`Lasting::master`] of 2), which picks the effect of a swap.
    creature_in_front: bool,
}

/// What the turn of a carried creature to a place steps by in a game frame (the 85 of
/// `0x800C0F80`).
const CREATURE_TURNS_BY: i32 = 85;

impl OnField<'_, '_> {
    /// A ready test over every opponent on the field, with the build's guard against a
    /// wait that does not end.
    fn everybody(&mut self, his: bool, test: impl Fn(&Standing) -> bool) -> bool {
        let actors = &*self.running.actors;
        let yes = his
            && (self.stood.iter())
                .filter(|stands| stands.on_field())
                .all(|stands| actors.get(stands.index).is_none_or(&test));
        if !yes && *self.waited < Lasting::WAITS_AT_MOST {
            *self.waited += 1;
            return false;
        }
        if !yes {
            warn!(
                "run {} of the fight has waited {} game frames for its fighters and goes on \
                 without them",
                self.run, self.waited
            );
        }
        *self.waited = 0;
        true
    }

    /// The place of a slot (`0x800BC89C`): the player's own for slot 0, the place of
    /// its actor for an opponent on the field; none for a slot that is not there.
    fn place(&self, slot: usize) -> Option<[i32; 3]> {
        if slot == PLAYER {
            // His creature's while that fights (`0x800C116C`).
            return match self.own_figure() {
                Some(figure) => Some(figure.mover.position),
                None => (self.running.player.as_ref()).map(|he| he.player.position),
            };
        }
        let stands = (self.stood.iter()).find(|stands| stands.stands_for(slot))?;
        (self.running.actors.get(stands.index)).map(|actor| actor.mover.position)
    }

    /// The figure of the carried creature that fights in the player's place, while it
    /// is the fighter of the own side.
    fn own_figure(&self) -> Option<&Standing> {
        let fighter = self.fighter;
        let stands = (self.stood.iter()).find(|stands| stands.stands_for(fighter))?;
        self.running.actors.get(stands.index)
    }

    /// Whether the player is on the field (`0x800BC740` for slot 0): he is while he is
    /// the fighter of the own side, and nothing is asked of him while his creature is.
    fn he_fights(&self) -> bool {
        self.fighter == PLAYER && self.running.player.is_some()
    }

    /// The actor that stands for a slot, while it is on the field: an opponent's, or
    /// the figure of a carried creature.
    fn opponent(&mut self, slot: usize) -> Option<&mut Stood> {
        (self.stood.iter_mut()).find(|stands| stands.stands_for(slot))
    }
}

impl runner::Field for OnField<'_, '_> {
    fn free(&mut self) -> bool {
        let free =
            |actor: &Standing| actor.runner.free() && actor.mover.flags & mover::flag::TARGET == 0;
        // His creature is asked in his place while it fights (`0x800C0F30`).
        let his = match self.own_figure() {
            Some(figure) => free(figure),
            None => (self.running.player.as_ref()).is_none_or(|he| he.player.played_out()),
        };
        // Behind the fighters the test asks the effect's loader (`0x800148D4`): the
        // effect of the run before has to be over.
        let idle = self.running.room.effect_idle;
        self.everybody(his && idle, free)
    }

    fn turn(&mut self, to: u8, turned: u8) {
        // Only the own side turns: an opponent does nothing (`0x800A2568`).
        let places: Vec<[i32; 3]> = (0..SLOTS)
            .filter(|slot| to & 1 << slot != 0)
            .filter_map(|slot| self.place(slot))
            .collect();
        // His creature is given the place as a target, to turn to by 85 a game frame
        // (`0x800C0F80` with `0x8001DB68`).
        let fighter = self.fighter;
        let own = (1..OPPONENTS).find(|slot| turned & 1 << slot != 0 && *slot == fighter);
        if let (Some(slot), Some(mean)) = (own, runner::mean(&places))
            && let Some(index) = self.opponent(slot).map(|stands| stands.index)
        {
            info!(
                "game frame {}: run {}: the creature of slot {slot} turns to {mean:?}, the mean \
                 place of the slots {to:#04x}",
                self.running.room.game_frame, self.run
            );
            self.running.turn_figure(index, mean);
        }
        // A slot that is not on the field is not turned (`0x800BC6A4`).
        if turned & 1 << PLAYER == 0 || !self.he_fights() {
            return;
        }
        if let Some(mean) = runner::mean(&places) {
            info!(
                "game frame {}: run {}: the player turns to {mean:?}, the mean place of the \
                 slots {to:#04x}",
                self.running.room.game_frame, self.run
            );
            self.running.room.orders.face(mean);
        }
    }

    fn ask(&mut self, slot: usize, record: (u32, u32), reaction: bool) -> [i16; 2] {
        let (game_frame, run) = (self.running.room.game_frame, self.run);
        // Only a slot on the field is asked (`0x800BE228` with `0x800BC740`): of the
        // own side the fighter alone.
        if slot < OPPONENTS && slot != self.fighter {
            return [0, 0];
        }
        if slot == PLAYER {
            let Some(he) = self.running.player.as_mut() else {
                return [0, 0];
            };
            let setting = self.running.room.setting;
            let (action, times) = he.asked(record.0, reaction, setting);
            let orders = &mut self.running.room.orders;
            orders.ask(action, !reaction, self.targets.into(), record.1);
            return times;
        }
        let Some(stands) = (self.stood.iter_mut()).find(|stands| stands.stands_for(slot)) else {
            return [0, 0];
        };
        let thing = stands.index + 1;
        let what = match reaction {
            true => "the reaction",
            false => "the command",
        };
        let mask = match record.0 {
            2 | 3 | 21 | 22 => self.actors,
            _ => self.targets,
        };
        // The attack of the slot's move entry, for the effect of a body attack.
        let attack = (self.seen.as_ref())
            .and_then(|seen| seen.records.get(slot)?.as_ref())
            .and_then(|fighter| fighter.moves.get(record.1 as usize))
            .map(|entry| entry.attack);
        let with = (mask, attack, self.creature_in_front);
        match (self.running).ask_opponent(stands, record, reaction, with) {
            Some(times) => {
                info!(
                    "game frame {game_frame}: run {run}: thing {thing} is asked for {what} of \
                     kind {} with parameter {}: its key moment comes {} game frames after it \
                     is let go, its end {}",
                    record.0, record.1, times[0], times[1]
                );
                times
            }
            None => {
                info!(
                    "game frame {game_frame}: run {run}: thing {thing} is not asked for {what} \
                     of kind {}: the kind has no list, or the model lacks an action of it",
                    record.0
                );
                [0, 0]
            }
        }
    }

    fn ask_nothing(&mut self, slot: usize) {
        // The kind "nothing" only turns him to the place he has (order state 5). He is
        // not asked while his creature fights, and the creature never for this kind
        // (`0x800C0FAC`).
        if slot == PLAYER && self.he_fights() {
            self.running.room.orders.ask(None, false, 0, 0);
        }
    }

    fn go_on_nothing(&mut self, slot: usize) {
        if slot == PLAYER && self.he_fights() {
            self.running.room.orders.go();
        }
    }

    fn ready(&mut self) -> bool {
        // His order state is 4: what was asked waits for its go and he faces the place.
        // His creature is asked in his place while it fights (`0x800C0F58`).
        let his = match self.own_figure() {
            Some(figure) => figure.runner.ready(),
            None => self.running.player.is_none() || self.running.room.orders.at_the_ready(),
        };
        self.everybody(his, |actor| actor.runner.ready())
    }

    fn go_on(&mut self, slot: usize) {
        let (game_frame, run) = (self.running.room.game_frame, self.run);
        // Only a slot on the field is let go (`0x800BE31C` with `0x800BC740`).
        if slot < OPPONENTS && slot != self.fighter {
            return;
        }
        if slot == PLAYER {
            if self.running.player.is_some() && self.running.room.orders.at_the_ready() {
                self.running.room.orders.go();
                info!("game frame {game_frame}: run {run}: the player is let go from his hold");
            }
            return;
        }
        let Some((index, asked, effect)) = (self.opponent(slot))
            .map(|stands| (stands.index, stands.runner.is_some(), stands.effect.take()))
        else {
            return;
        };
        if let Some(actor) = self.running.actors.get_mut(index).filter(|_| asked) {
            actor.runner.goes_on();
            self.running.room.effects_started.extend(effect);
            info!(
                "game frame {game_frame}: run {run}: thing {} is let go from its hold",
                index + 1
            );
        }
    }

    fn own_command_goes(&mut self, slot: usize, kind: u32) {
        // The side of the slot: the player, his creature, an opponent (`0x800BC9AC`).
        let side = match slot {
            PLAYER => 0,
            slot if slot < OPPONENTS => 1,
            _ => 2,
        };
        let Some(path) = arena::Path::asked(side, kind) else {
            return;
        };
        // The capture's times lie behind the end of his draw, and with the setting
        // "Capture FX", which a new game has, the run is two segments longer.
        let draw_ends = (self.running.player.as_ref())
            .and_then(|he| player::arms::draw_ends(&he.moves, he.player.group));
        let Some(draw_ends) = draw_ends.or((path == arena::Path::Second).then_some(0)) else {
            return;
        };
        let long = self.running.room.setting & player::arms::CAPTURE_FX != 0;
        let times = path.times(draw_ends.into(), long);
        info!(
            "game frame {}: run {}: the camera is asked for the run along points {path:?} \
             with the times {times:?}",
            self.running.room.game_frame, self.run
        );
        self.eye.fly(path, times);
    }

    fn flash(&mut self) {
        info!(
            "game frame {}: run {}: the screen is made white for {} game frames",
            self.running.room.game_frame,
            self.run,
            Lasting::FLASH
        );
        self.running.room.flash = Lasting::FLASH;
    }

    fn shake(&mut self) {
        info!(
            "game frame {}: run {}: the picture is shaken",
            self.running.room.game_frame, self.run
        );
        self.eye.shake();
    }

    fn sound(&mut self, number: u32) {
        let (game_frame, run) = (self.running.room.game_frame, self.run);
        info!("game frame {game_frame}: run {run}: sound {number} is asked for");
        (self.running.room.music).push((game_frame, Heard::Sound(number)));
    }

    fn row(&mut self, row: usize) {
        let (game_frame, run) = (self.running.room.game_frame, self.run);
        info!(
            "game frame {game_frame}: run {run}: row {row} of the fight's sequences is asked for"
        );
        self.running.fight_sound(vec![sound::FightAsks::Start(row)]);
    }

    fn number(&mut self, slot: usize, way: u8, amount: i32) {
        if amount != 0 {
            info!(
                "game frame {}: run {}: a number counts {amount} from the {} of slot {slot}",
                self.running.room.game_frame,
                self.run,
                match way {
                    0 => "HP",
                    _ => "MP",
                }
            );
        }
        if let Some(seen) = self.seen.as_mut() {
            seen.number(slot, way, amount);
        }
    }

    fn give(&mut self, slot: usize, clear: u32, add: u32) {
        if let Some(seen) = self.seen.as_mut() {
            seen.give(slot, clear, add);
        }
    }

    fn leave(&mut self, slot: usize) {
        let (game_frame, run) = (self.running.room.game_frame, self.run);
        if let Some(seen) = self.seen.as_mut() {
            seen.leave(slot);
        }
        let Some(stands) = self.opponent(slot) else {
            return;
        };
        stands.gone = true;
        let index = stands.index;
        if let Some(actor) = self.running.actors.get_mut(index) {
            actor.runner.hidden = true;
            info!(
                "game frame {game_frame}: run {run}: thing {} leaves the field",
                index + 1
            );
        }
    }
}

fn hand_on(own: &mut Option<Arc<Ground>>, room: &Arc<Ground>) -> bool {
    if own.as_ref().is_some_and(|own| Arc::ptr_eq(own, room)) {
        return false;
    }
    *own = Some(room.clone());
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_played_game_has_the_menu_of_a_fight_and_a_hidden_run_chooses_by_itself() {
        let asked = |args: &[&str]| fight_menu_asked(args.iter().map(|arg| arg.to_string()));
        assert!(asked(&["jc-game"]));
        assert!(asked(&["jc-game", "--room=106", "--player"]));
        assert!(!asked(&["jc-game", "--smoke=120"]));
        assert!(!asked(&["jc-game", "--screenshot=shot.png"]));
        assert!(asked(&["jc-game", "--smoke=120", "--fight-menu"]));
        assert!(!asked(&["jc-game", "--fight-auto"]));
    }

    #[test]
    fn a_fight_lasts_the_opening_and_the_pause_of_its_outcome() {
        assert_eq!(Lasting::CAMERA_STANDS, 78);
        assert_eq!(Lasting::HUB, 62);
        assert_eq!(Lasting::held(0), Some(8));
        assert_eq!(Lasting::held(30), Some(248));
        assert_eq!(Lasting::held(31), Some(255));
        assert_eq!(Lasting::held(33), Some(255));
        assert_eq!(Lasting::held(34), None);
        assert_eq!(Lasting::trails(0), None);
        assert_eq!(Lasting::trails(1), Some(8));
        assert_eq!(Lasting::back(Some(ended::WON)), 7);
        assert_eq!(Lasting::back(None), 7);
        assert_eq!(Lasting::back(Some(ended::RAN)), 37);
        let last_held = Lasting::back(Some(ended::WON)) + Lasting::SEEN - 1;
        assert_eq!(Lasting::trails(last_held), Some(64));
        assert_eq!(Lasting::trails(40), Some(255));
        // Nothing behind the fight's task is passed from the tear-down (the second
        // game frame) to the one in front of the script's going on.
        let stalled = |back: u32| (0..=back).filter(move |&behind| Lasting::stalls(behind, back));
        assert_eq!(stalled(7).collect::<Vec<_>>(), [2, 3, 4, 5, 6]);
        assert_eq!(stalled(37).count(), 35);
        assert!(!Lasting::rests(false, true));
        assert!(Lasting::rests(true, true));
        assert!(!Lasting::rests(true, false));
        assert!(!Lasting::rests(false, false));
        assert_eq!(Lasting::pause(ended::WON), 60);
        // "Outcome 3": the second state behind a loss makes one pass.
        assert_eq!(Lasting::pause(ended::LOST), 1);
        // "The way back by game frames": a flight and a lesson count nothing and wait
        // for the player's wanted action.
        for outcome in [ended::RAN, ended::TAUGHT, ended::REFUSED] {
            assert_eq!(Lasting::pause(outcome), 0);
            assert!(Lasting::waits_for_his_action(Some(outcome)));
        }
        for outcome in [Some(ended::WON), Some(ended::LOST), None] {
            assert!(!Lasting::waits_for_his_action(outcome));
        }
        assert_eq!(Lasting::turned(1, 0), 0);
        assert_eq!([0, 1].map(|n| Lasting::turned(2, n)), [-284, 284]);
        assert_eq!([0, 1, 2].map(|n| Lasting::turned(3, n)), [-512, 0, 512]);
    }

    #[test]
    fn a_turned_barrier_reaches_the_ground_of_an_actor() {
        let start = Ground {
            flags: vec![1, 1],
            ..Ground::default()
        };
        let mut room = Arc::new(start.clone());
        // The actor was handed its ground when the room was made.
        let mut own = Some(Arc::new(start));
        assert!(hand_on(&mut own, &room));
        assert!(!hand_on(&mut own, &room));

        // A switch turns barrier 1 off: the room's ground is written, as a copy while the
        // actor shares it.
        Arc::make_mut(&mut room).flags[1] = 0;
        assert_eq!(own.as_ref().map(|ground| ground.flags[1]), Some(1));
        assert!(hand_on(&mut own, &room));
        assert_eq!(own.as_ref().map(|ground| ground.flags[1]), Some(0));
        assert!(!hand_on(&mut own, &room));
    }
}
