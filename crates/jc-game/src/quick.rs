
use std::path::{Path, PathBuf};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use jc_disc::{Archive, DiscImage, Executable, LocalConfig};
use jc_formats::card_screen::{self, BUFFER, Game as _};
use jc_formats::carried::Carried;
use jc_formats::forest::Forests;
use jc_formats::rooms::{self, RoomTable};

use crate::name_entry::Named;
use crate::notice::Notices;
use crate::prefs::{self, Prefs, PrefsFile};
use crate::saving::{self, PlayClock};
use crate::settings::Settings;
use crate::shown::Things;

type Failure = Box<dyn std::error::Error>;

/// The autosaves, written in turn.
pub const AUTOSAVES: usize = 3;

/// The file of the quick save in the folder of the saves.
pub fn quick_file(folder: &Path) -> PathBuf {
    folder.join("quick").join("quick.dat")
}

/// The file of autosave `n` (from 0) in the folder of the saves.
pub fn auto_file(folder: &Path, n: usize) -> PathBuf {
    folder.join("auto").join(format!("auto{}.dat", n + 1))
}

/// The build's files of the list of loads, in the order of its rows above the slots
/// (`card_screen::EXTRA`): the quick save, then the autosaves.
pub fn extra_file(folder: &Path, n: usize) -> PathBuf {
    match n {
        0 => quick_file(folder),
        n => auto_file(folder, n - 1),
    }
}

/// What a key or a check asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asked {
    Save,
    Load,
    /// A check's stand-in for a way into a forest or a chapter's start: an autosave
    /// waits.
    Auto,
}

/// The value of `--quick-press=SECONDS:save|load,...`.
pub fn presses(value: &str) -> Option<Vec<(f64, Asked)>> {
    (value.split(','))
        .map(|press| {
            let (at, asked) = press.split_once(':')?;
            let asked = match asked {
                "save" => Asked::Save,
                "load" => Asked::Load,
                "auto" => Asked::Auto,
                _ => return None,
            };
            Some((at.parse().ok()?, asked))
        })
        .collect()
}

/// Where the quick save and the autosaves go, and what a check presses.
#[derive(Resource)]
pub struct QuickSaves {
    /// The folder of the saves (`--card-dir=DIR`, by default `saves`).
    pub folder: PathBuf,
    /// Whether this run writes saves of its own: a play session, or a check with a
    /// folder of its own.
    pub writes: bool,
    /// `--quick-press`: the presses still to come, in their order.
    pub presses: Vec<(f64, Asked)>,
}

/// Why an autosave waits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Why {
    /// The way into a forest, into this room.
    Forest(u16),
    /// The start of a chapter: its screen ran.
    Chapter,
    /// `--quick-press=SECONDS:auto` of a check.
    Check,
    /// The room of a talk and fight of the story, into this room: its scripts hold a
    /// fight of a script.
    Boss(u16),
}

/// The autosave: what it waits for, and what it knows of the rooms.
#[derive(Resource, Default)]
pub struct Autosave {
    waits: Option<Why>,
    /// The room on screen when it was last looked at, and whether it is in a forest.
    room: Option<(u16, bool)>,
    /// A load is under way: the room it brings is no way into a forest.
    loading: bool,
    /// The room last asked whether its scripts hold a fight of the story, and whether a
    /// load brought it.
    asked_for_a_fight: Option<u16>,
    loading_a_fight: bool,
    places: Option<Places>,
    /// The table could not be read: no room counts as a forest.
    no_places: bool,
}

struct Places {
    forests: Forests,
    rooms: RoomTable,
}

impl Autosave {
    /// A load is under way: what waits is cancelled, and the room it brings is new to
    /// the autosave but no way into a forest and no room whose fight to save in front of.
    fn a_load(&mut self) {
        self.loading = true;
        self.loading_a_fight = true;
        self.waits = None;
        self.room = None;
        self.asked_for_a_fight = None;
    }
}

impl Places {
    fn read() -> Result<Self, Failure> {
        let config = LocalConfig::load()?;
        let mut disc = DiscImage::open(&config.disc_image)?;
        let archive = Archive::open(&mut disc)?;
        let exe = Executable::read(&mut disc)?;
        let engine = archive.read(&mut disc, rooms::FIELD_ENGINE_FILE)?;
        Ok(Self {
            forests: Forests::from_executable(&exe.image, exe.base)?,
            rooms: RoomTable::from_field_engine(&engine)?,
        })
    }

    /// Whether room `number` seen from `view` is in a forest.
    fn in_a_forest(&self, number: u16, view: usize) -> bool {
        (self.rooms.by_number(number)).is_some_and(|(at, record)| {
            (self.forests).holds(at.area, at.place, record.slot, view as i16)
        })
    }
}

impl Autosave {
    /// Whether room `number` seen from `view` is in a forest; the table is read the
    /// first time.
    fn in_a_forest(&mut self, number: u16, view: usize) -> bool {
        if self.places.is_none() && !self.no_places {
            match Places::read() {
                Ok(places) => self.places = Some(places),
                Err(err) => {
                    warn!("the autosave: the table of the forests is not read: {err}");
                    self.no_places = true;
                }
            }
        }
        (self.places.as_ref()).is_some_and(|places| places.in_a_forest(number, view))
    }
}

/// The screens that may stand over a room or in place of one.
#[derive(SystemParam)]
pub struct Screens<'w> {
    title: Option<Res<'w, crate::title::Title>>,
    playing: Option<Res<'w, crate::movie_view::Playing>>,
    menu: Option<Res<'w, crate::player_menu::MenuScreen>>,
    shop: Option<Res<'w, crate::shop_view::ShopView>>,
    merge: Option<Res<'w, crate::merge_view::MergeView>>,
    results: Option<Res<'w, crate::results_view::ResultsView>>,
    card: Option<Res<'w, crate::card_view::CardView>>,
    chapter: Option<Res<'w, crate::chapter::Screen>>,
    map: Option<Res<'w, crate::place_map::PlaceMap>>,
    names: Option<Res<'w, crate::name_entry::NameEntry>>,
    loaded: Option<Res<'w, saving::Loaded>>,
    ended: Option<Res<'w, saving::Ended>>,
    second: Option<Res<'w, saving::SecondRound>>,
    /// The fight of two players: nothing of it is saved, nor loaded into.
    arena: Option<Res<'w, crate::versus_view::Arena>>,
}

impl Screens<'_> {
    /// No screen of a menu stands over the room.
    pub(crate) fn none_over_the_room(&self) -> bool {
        let merge = self.merge.as_ref().is_some_and(|merge| !merge.asleep());
        !(self.menu.is_some()
            || self.shop.is_some()
            || merge
            || self.results.is_some()
            || self.card.is_some())
    }

    /// The game is somewhere a load may leave: not the title, a movie, the chapter's
    /// screen, the place map or the name entry, and no load or end is under way.
    pub(crate) fn may_leave(&self) -> bool {
        let movie = self
            .playing
            .as_ref()
            .is_some_and(|playing| playing.on_screen());
        !(self.title.is_some()
            || movie
            || self.chapter.is_some()
            || self.map.is_some()
            || self.names.is_some()
            || self.loaded.is_some()
            || self.ended.is_some()
            || self.second.is_some()
            || self.arena.is_some())
    }
}

/// The game of this moment, as a save holds it.
#[derive(SystemParam)]
pub struct Moment<'w> {
    settings: Res<'w, Settings>,
    named: Option<Res<'w, Named>>,
    clock: Res<'w, PlayClock>,
    prefs: Res<'w, Prefs>,
    file: Res<'w, PrefsFile>,
}

/// Which file a save goes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kept {
    Quick,
    /// Autosave `n`, from 0.
    Auto(usize),
}

/// A short account of a game for the log, by which a save and its load are compared:
/// the chapter, the money, the things of the story and a sum of all he has.
fn account(carried: &Carried) -> String {
    let bytes = carried.keeping.to_bytes();
    let sum = bytes.iter().map(|&byte| u32::from(byte)).sum::<u32>();
    format!(
        "chapter {}, {} money, {} things of the story, what he has adds up to {sum}, \
         weapon, armour and third thing {:?}",
        carried.chapter,
        carried.keeping.money(),
        carried.keeping.story_things(),
        carried.worn
    )
}

/// The text of the place a save in room `number` names: the one of the room's object of
/// the memory card, "FOREST" in a forest, none elsewhere.
fn place_text(things: &Things, forest: bool) -> [u8; 24] {
    if let Some(text) = things.place_text() {
        return text;
    }
    let mut text = [0; 24];
    if forest {
        let units = card_screen::wide(b"FOREST");
        for (bytes, unit) in text.as_chunks_mut::<2>().0.iter_mut().zip(units) {
            bytes.copy_from_slice(&unit.to_le_bytes());
        }
    }
    text
}

/// The autosave to write next: the first that is not there, else the oldest.
fn next_autosave(folder: &Path) -> usize {
    let written =
        |n: usize| std::fs::metadata(auto_file(folder, n)).and_then(|meta| meta.modified());
    let mut oldest = (0, None);
    for n in 0..AUTOSAVES {
        match written(n) {
            Err(_) => return n,
            Ok(at) if oldest.1.is_none_or(|oldest| at < oldest) => oldest = (n, Some(at)),
            Ok(_) => {}
        }
    }
    oldest.0
}

/// Writes the running game into `kept` as the save point writes a save, with the file
/// beside it. Gives the file and the account of the game.
fn write_save(
    kept: Kept,
    folder: &Path,
    (show, things, moment): (&crate::ShowRoom, &Things, &Moment),
    forest: bool,
) -> Result<(PathBuf, String), Failure> {
    let carried = things.carried().ok_or("the room's scripts do not run")?;
    let stands = things.stands().ok_or("the room has no hero")?;
    let room = Some((show.number, things.view_shown(), stands));
    let beside = (&*moment.settings, moment.named.as_deref(), &*moment.clock);
    let place = place_text(things, forest);
    let game = saving::Running::of(carried.clone(), beside, room, place)?;
    // The card's file is the first block of the buffer (`card_screen::Cards::write`).
    let mut file = game.buffer()?;
    file.resize(card_screen::BLOCK, 0);
    let path = match kept {
        Kept::Quick => quick_file(folder),
        Kept::Auto(n) => auto_file(folder, n),
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, &file)?;
    let feel = moment.file.live().then_some(&moment.prefs.feel);
    if let Err(err) = prefs::write_feel(&path, feel, carried.chapter) {
        warn!("the file beside {} is not written: {err}", path.display());
    }
    let said = format!(
        "room {}, the hero at {:?} angle {}, {} refreshes on the clock, {}",
        show.number,
        stands.0,
        stands.1,
        moment.clock.refreshes,
        account(&carried)
    );
    Ok((path, said))
}

/// Reads the quick save as "Load" at a save point reads a save: the game it holds, with
/// the game feel beside it in a play session.
fn read_quick(
    folder: &Path,
    things: &Things,
    game: Option<&crate::Game>,
    moment: &Moment,
) -> Result<saving::Loaded, Failure> {
    let path = quick_file(folder);
    let mut buffer = std::fs::read(&path).map_err(|err| format!("{}: {err}", path.display()))?;
    buffer.resize(BUFFER, 0);
    let carried = (things.carried())
        .or_else(|| game.map(|game| game.0.clone()))
        .unwrap_or_default();
    let beside = (&*moment.settings, moment.named.as_deref(), &*moment.clock);
    let mut running = saving::Running::of(carried, beside, None, [0; 24])?;
    if !running.read(&buffer) {
        return Err(format!("{} is no save the reader takes", path.display()).into());
    }
    let mut loaded = running.loaded.take().ok_or("the reader took no game")?;
    loaded.feel = moment
        .file
        .live()
        .then(|| prefs::read_feel(&path))
        .flatten();
    Ok(loaded)
}

/// The screens over a room that a quick load takes away with the room.
#[derive(SystemParam)]
pub struct OverTheRoom<'w, 's> {
    commands: Commands<'w, 's>,
    voices: ResMut<'w, crate::voices::Voices>,
    pc: Option<ResMut<'w, crate::pc_settings::PcSettings>>,
    skip: Option<ResMut<'w, crate::skip::Skip>>,
}

impl OverTheRoom<'_, '_> {
    /// Whether a rebinding of the PC Settings waits for a key: F6 and F9 are refused
    /// there and do nothing else either.
    fn capturing(&self) -> bool {
        self.pc.as_ref().is_some_and(|pc| pc.capturing())
    }

    /// The screens of the menus go with the room; their pictures go with what the
    /// room drew (`play_loaded`). A voice that speaks stops, the PC Settings over the
    /// options close and a skip of a scene stops (`now`: the real clock).
    fn take_away(&mut self, now: f64) {
        if let Some(pc) = self.pc.as_mut() {
            pc.close();
        }
        if let Some(skip) = self.skip.as_mut() {
            skip.stop_for_a_load(now);
        }
        self.commands
            .remove_resource::<crate::player_menu::MenuScreen>();
        self.commands
            .remove_resource::<crate::shop_view::ShopView>();
        self.commands
            .remove_resource::<crate::merge_view::MergeView>();
        self.commands
            .remove_resource::<crate::results_view::ResultsView>();
        self.commands
            .remove_resource::<crate::card_view::CardView>();
        self.voices.end();
    }
}

/// Notes what the autosave waits for: a chapter that starts, the way into a forest
/// from a room that is not in one, or a room whose scripts hold a fight of the story. A
/// load cancels what waits.
pub fn note_autosave(
    show: Option<Res<crate::ShowRoom>>,
    things: Option<Res<Things>>,
    chapter: Option<Res<crate::chapter::Screen>>,
    loaded: Option<Res<saving::Loaded>>,
    mut auto: ResMut<Autosave>,
    arena: Option<Res<crate::versus_view::Arena>>,
) {
    // The arena's fights are no fights of the story: no autosave waits for them.
    if arena.is_some() {
        return;
    }
    if loaded.is_some() {
        auto.a_load();
    }
    // The chapter screen runs for a while; its first room comes behind it.
    if chapter.is_some() && auto.waits != Some(Why::Chapter) {
        info!("the autosave: a chapter starts, a save waits until the hero walks freely");
        auto.waits = Some(Why::Chapter);
    }
    let Some(show) = show else {
        return;
    };
    note_a_fight(&show, things.as_deref(), &mut auto);
    if auto.room.is_some_and(|(room, _)| room == show.number) {
        return;
    }
    let view = things.as_ref().map_or(0, |things| things.view_shown());
    let forest = auto.in_a_forest(show.number, view);
    let from_outside = auto.room.is_some_and(|(_, forest)| !forest);
    if forest && from_outside && !std::mem::take(&mut auto.loading) && auto.waits.is_none() {
        info!(
            "the autosave: room {} is the way into a forest, a save waits until the hero \
             walks freely",
            show.on_screen
        );
        auto.waits = Some(Why::Forest(show.number));
    }
    auto.loading = false;
    auto.room = Some((show.number, forest));
}

/// The save in front of a talk and fight of the story (D4): in a room whose scripts hold
/// a fight of a script ([`Things::holds_story_fight`]), once a visit, on the first game
/// frame the hero walks freely in it while a script that leads to the fight still runs
/// ([`run_saves`]). A fight that comes first, or the room left first, takes it back; a
/// room a load brings does not ask for one.
fn note_a_fight(show: &crate::ShowRoom, things: Option<&Things>, auto: &mut Autosave) {
    if let Some(Why::Boss(room)) = auto.waits {
        if room != show.number {
            info!("the autosave: room {room} is left before the hero walked freely in it, no save");
            auto.waits = None;
        } else if things.is_some_and(Things::fight_on) {
            info!(
                "the autosave: a fight comes in room {room} before the hero walked freely in                  it, no save in front of it"
            );
            auto.waits = None;
        }
    }
    let Some(things) = things else {
        return;
    };
    if auto.asked_for_a_fight == Some(show.number) {
        return;
    }
    auto.asked_for_a_fight = Some(show.number);
    let loading = std::mem::take(&mut auto.loading_a_fight);
    if !things.holds_story_fight() || things.fight_on() {
        return;
    }
    match (loading, auto.waits) {
        (true, _) => info!(
            "the autosave: the scripts of room {} hold a fight of the story, but a load              brought it",
            show.on_screen
        ),
        (false, Some(why)) => info!(
            "the autosave: the scripts of room {} hold a fight of the story, the save that              waits ({why:?}) comes first",
            show.on_screen
        ),
        (false, None) => {
            info!(
                "the autosave: the scripts of room {} hold a fight of the story, a save waits                  until the hero walks freely",
                show.on_screen
            );
            auto.waits = Some(Why::Boss(show.number));
        }
    }
}

/// The room on screen, its things and the game it carries on.
type RoomOnScreen<'w> = (
    Option<Res<'w, crate::ShowRoom>>,
    Option<Res<'w, Things>>,
    Option<Res<'w, crate::Game>>,
);

/// F6, F9 and the autosave that waits.
#[allow(clippy::too_many_arguments)]
pub fn run_saves(
    keys: Res<ButtonInput<KeyCode>>,
    real: Res<Time<Real>>,
    (mut quick, mut auto): (ResMut<QuickSaves>, ResMut<Autosave>),
    (show, things, game): RoomOnScreen,
    screens: Screens,
    moment: Moment,
    (mut unsaved, mut notices): (ResMut<crate::guards::Unsaved>, ResMut<Notices>),
    mut over: OverTheRoom,
) {
    let now = real.elapsed_secs_f64();
    let mut asked = None;
    if over.capturing() {
        // The PC Settings refuse the key of the build: it does nothing else.
    } else if keys.just_pressed(KeyCode::F6) {
        asked = Some(Asked::Save);
    } else if keys.just_pressed(KeyCode::F9) {
        asked = Some(Asked::Load);
    } else if quick.presses.first().is_some_and(|&(at, _)| at <= now) {
        asked = Some(quick.presses.remove(0).1);
    }
    let room = show.as_deref().zip(things.as_deref());
    // The corridor of the second round is no place to save: its generator lives only
    // while the game runs, and a load in one of its pieces would find none.
    let free = screens.may_leave()
        && screens.none_over_the_room()
        && room.is_some_and(|(_, things)| things.walks_freely() && things.corridor_at().is_none());
    let folder = quick.folder.clone();
    match asked {
        Some(Asked::Auto) => {
            info!("the autosave: a check asks for one, it waits until the hero walks freely");
            auto.waits = Some(Why::Check);
        }
        Some(_) if !quick.writes => {
            warn!("F6 and F9: a check without --card-dir=DIR keeps no saves of its own");
        }
        Some(Asked::Save) => {
            let Some((show, things)) = room.filter(|_| free) else {
                let by = match room {
                    None => Some("no room is on screen"),
                    Some(_) if !screens.may_leave() => Some("a movie or a screen is up"),
                    Some(_) if !screens.none_over_the_room() => Some("a menu is up"),
                    Some((_, things)) if things.corridor_at().is_some() => {
                        Some("the hero is in the corridor of the second round")
                    }
                    Some((_, things)) => things.held_by(),
                };
                info!(
                    "F6: the hero does not walk freely in a room ({}), the game is not saved",
                    by.unwrap_or("?")
                );
                notices.say("Can't save now");
                return;
            };
            let forest = auto.room.is_some_and(|(_, forest)| forest);
            match write_save(Kept::Quick, &folder, (show, things, &moment), forest) {
                Ok((path, said)) => {
                    info!(
                        "F6: the quick save is written to {}: {said}",
                        path.display()
                    );
                    unsaved.kept(Some(show.number), moment.clock.refreshes);
                    notices.say("Quick saved");
                }
                Err(err) => {
                    error!("F6: the quick save is not written: {err}");
                    notices.say("Can't save now");
                }
            }
            return;
        }
        Some(Asked::Load) => {
            let Some((_, things)) = room.filter(|_| screens.may_leave()) else {
                info!("F9: no room of a game is on screen, nothing is loaded");
                notices.say("Can't load now");
                return;
            };
            if !quick_file(&folder).is_file() {
                info!("F9: there is no quick save in {}", folder.display());
                notices.say("No quick save");
                return;
            }
            match read_quick(&folder, things, game.as_deref(), &moment) {
                Ok(loaded) => {
                    info!(
                        "F9: the quick save is loaded: room {:?}, the hero at {:?} angle {}, \
                         {} refreshes on the clock, {}",
                        loaded.room,
                        loaded.stands.position,
                        loaded.stands.angle,
                        loaded.beside.clock,
                        account(&loaded.carried)
                    );
                    over.take_away(now);
                    over.commands.insert_resource(loaded);
                    // `note_autosave` has run in this frame: it is told here.
                    auto.a_load();
                    notices.say("Loaded");
                }
                Err(err) => {
                    error!("F9: the quick save is not loaded: {err}");
                    notices.say("Can't load now");
                }
            }
            return;
        }
        None => {}
    }
    // The autosave that waits, once the hero walks freely.
    let Some(why) = auto.waits.filter(|_| free) else {
        return;
    };
    auto.waits = None;
    // The fight may lie behind the story: no script that leads to it runs any more.
    if let (Why::Boss(room), Some((_, things))) = (why, room) {
        match things.story_fight_ahead() {
            Some(script) => info!(
                "the autosave: the hero walks freely in room {room} and script {script} still                  leads to its fight"
            ),
            None => {
                info!(
                    "the autosave: the hero walks freely in room {room}, but no script that                      leads to its fight runs any more, no save"
                );
                return;
            }
        }
    }
    if !quick.writes || !moment.prefs.feel.saves.autosave {
        info!("the autosave: {why:?}, but this run keeps no autosaves");
        return;
    }
    let Some((show, things)) = room else {
        return;
    };
    let n = next_autosave(&folder);
    let forest = auto.room.is_some_and(|(_, forest)| forest);
    match write_save(Kept::Auto(n), &folder, (show, things, &moment), forest) {
        Ok((path, said)) => {
            info!(
                "the autosave ({why:?}) is written to {}: {said}",
                path.display()
            );
            unsaved.kept(Some(show.number), moment.clock.refreshes);
            notices.say("Saved");
        }
        Err(err) => error!("the autosave ({why:?}) is not written: {err}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_load_is_no_way_into_a_forest_nor_a_room_to_save_in_front_of_a_fight() {
        let mut auto = Autosave {
            waits: Some(Why::Chapter),
            room: Some((84, false)),
            asked_for_a_fight: Some(125),
            ..Autosave::default()
        };
        auto.a_load();
        assert_eq!(auto.waits, None);
        assert!(auto.loading && auto.loading_a_fight);
        // The room the load brings is asked anew, even the one that was on screen.
        assert_eq!((auto.room, auto.asked_for_a_fight), (None, None));
    }

    #[test]
    fn a_check_presses_save_and_load_at_seconds() {
        assert_eq!(
            presses("6:save,9.5:load,10:auto"),
            Some(vec![
                (6.0, Asked::Save),
                (9.5, Asked::Load),
                (10.0, Asked::Auto)
            ])
        );
        assert_eq!(presses("6:quit"), None);
    }

    #[test]
    fn the_build_s_saves_lie_in_folders_of_their_own() {
        let folder = Path::new("saves");
        assert_eq!(quick_file(folder), folder.join("quick").join("quick.dat"));
        assert_eq!(extra_file(folder, 0), quick_file(folder));
        assert_eq!(extra_file(folder, 1), folder.join("auto").join("auto1.dat"));
        assert_eq!(extra_file(folder, 3), folder.join("auto").join("auto3.dat"));
        assert_eq!(card_screen::EXTRA, 1 + AUTOSAVES);
    }

    #[test]
    #[ignore = "reads the disc image named in jc.local.toml"]
    fn the_hall_of_the_gates_is_no_forest_and_the_beetle_forest_is_one() {
        let places = Places::read().unwrap();
        let rooms = [11, 100, 101, 104, 105, 106, 107, 108];
        let found: Vec<(u16, bool)> = (rooms.iter())
            .map(|&room| (room, places.in_a_forest(room, 0)))
            .collect();
        println!("{found:?}");
        assert!(!places.in_a_forest(11, 0), "Garai's home");
        assert!(!places.in_a_forest(101, 0), "the hall of the gates");
        assert!(places.in_a_forest(107, 0), "a room of the Beetle Forest");
    }

    #[test]
    #[ignore = "reads the disc image named in jc.local.toml"]
    fn the_rooms_of_the_story_s_fights_are_found_in_their_scripts() {
        let config = LocalConfig::load().unwrap();
        let mut disc = DiscImage::open(&config.disc_image).unwrap();
        let archive = Archive::open(&mut disc).unwrap();
        let engine = archive.read(&mut disc, rooms::FIELD_ENGINE_FILE).unwrap();
        let table = RoomTable::from_field_engine(&engine).unwrap();
        let mut packages = std::collections::BTreeSet::new();
        let mut found = Vec::new();
        for (_, record) in table.iter() {
            let file = usize::from(record.package_file);
            if !packages.insert(file) {
                continue;
            }
            let data = archive.read(&mut disc, file).unwrap();
            let Ok(package) = jc_formats::scene::ScenePackage::parse(&data) else {
                continue;
            };
            if crate::scripts::holds_story_fight(&package, file) {
                found.push((record.number, file));
            }
        }
        println!("{found:?}");
        let rooms: Vec<Option<u16>> = found.iter().map(|(room, _)| *room).collect();
        assert_eq!(
            rooms,
            [
                Some(0),
                Some(1),
                None,
                Some(113),
                Some(125),
                Some(133),
                Some(141),
                Some(196),
                Some(217),
                Some(228),
                Some(249),
                Some(251),
                Some(253),
                Some(260),
                Some(263)
            ]
        );
        assert!(
            !found.iter().any(|(_, file)| *file == 305),
            "the hall of the gates"
        );
    }

    #[test]
    fn the_autosaves_are_written_in_turn() {
        let folder =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-scratch/quick/turn");
        std::fs::create_dir_all(folder.join("auto")).unwrap();
        for n in 0..AUTOSAVES {
            // Files an earlier run left are written over in their order.
            std::fs::write(auto_file(&folder, n), [n as u8]).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(next_autosave(&folder), 0);
        std::fs::write(auto_file(&folder, 0), [9]).unwrap();
        assert_eq!(next_autosave(&folder), 1);
    }
}
