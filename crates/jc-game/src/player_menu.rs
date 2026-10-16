
use std::collections::HashMap;

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use jc_disc::{Archive, DiscImage, Executable, LocalConfig};
use jc_formats::battle::Tables as FightTables;
use jc_formats::battle::record::{LEVEL, Random, Record};
use jc_formats::battle::using::{self, aim};
use jc_formats::carried::Carried;
use jc_formats::caught_list::{self, KindList, Kinds};
use jc_formats::equip_panel::{self, GearRow};
use jc_formats::equip_screen::{self, EquipScreen};
use jc_formats::forest::Forests;
use jc_formats::items::{FREE, Keeping, Kind};
use jc_formats::leave_forest;
use jc_formats::menu::{self, Frames, Menu, Piece, Rect, Style};
use jc_formats::model::ModelTable;
use jc_formats::name;
use jc_formats::notice;
use jc_formats::options_panel;
use jc_formats::options_screen::{self, OptionsScreen};
use jc_formats::pad::{Pad, Repeat};
use jc_formats::player_menu::{self, PlayerMenu, Spot, Tables, Things};
use jc_formats::player_panel::{
    self, AttackRow, Counts, Detail, Drawn, Family, ItemRow, Listed, StoryRow, attacks, bar,
    detail, items, message, spells, story,
};
use jc_formats::roamer::ItemCount;
use jc_formats::rooms::{self, RoomTable};
use jc_formats::text::{self, Font, Fonts};
use jc_formats::tim;
use jc_formats::window::{self, Faces};

use crate::music::Music;
use crate::name_entry::{Picture, SCREEN, WHOLE, blend, blit, letter};
use crate::options_view::{
    self, Look, Parts, Pc, ThingLook, blit_lit, inside, picture_in_row, thing_looks,
};
use crate::pc_settings::PcSettings;
use crate::place_map::picture;
use crate::pointer::{self, Aim, Pointer};
use crate::prefs::Prefs;
use crate::title::Press;

/// The longest stretch of time one pass of the app may stand for.
const LONGEST_PASS: f64 = 0.1;
/// Pictures of the screen in a game frame of the field engine.
const GAME_FRAME: u32 = 2;
/// The player's two model files; either brings his face.
const PLAYER_FILES: [u16; 2] = [831, 832];

type Failure = Box<dyn std::error::Error>;

/// `--menu[=SPECIES[:LEVEL],...]`, with the buttons of `--press=SECONDS:BUTTON,...`
/// for a hidden run. The species are the creatures the player carries, three at most.
#[derive(Resource)]
pub struct ShowMenu {
    pub presses: Vec<Press>,
    pub creatures: Vec<(u32, u32)>,
    /// `--menu-items=ITEM[:COUNT],...`: items the player is given for the look.
    pub items: Vec<(i32, i32)>,
    /// `--menu-hp=HP`: the HP the player and his creatures have for the look.
    pub hp: Option<i16>,
    /// `--menu-things=THING,...`: things of the story he is given for the look.
    pub things: Vec<i32>,
    /// `--menu-forest`: the look is taken as if the room were in a forest.
    pub forest: bool,
    /// `--menu-caught=SPECIES[:LEVEL],...`: caught creatures that wait, for the look.
    pub caught: Vec<(u32, u32)>,
    /// `--menu-kinds=SPECIES,...`: the kinds that count as caught, for the look.
    pub kinds: Vec<u32>,
    /// `--menu-gear=KIND:THING[:COUNT],...`: weapons (kind 0), armours (1) and third
    /// things (2) he is given for the look.
    pub gear: Vec<(usize, i32, i32)>,
}

impl ShowMenu {
    /// The value of `--menu=`: species with a level each, 1 where none is given.
    pub fn creatures(value: &str) -> Option<Vec<(u32, u32)>> {
        let creatures = Self::species(value)?;
        (creatures.len() <= player_panel::CREATURE_BOXES.len()).then_some(creatures)
    }

    /// The value of `--menu-caught=`: the same, twelve at most.
    pub fn caught(value: &str) -> Option<Vec<(u32, u32)>> {
        let caught = Self::species(value)?;
        (caught.len() <= player_menu::CAUGHT_RECORDS as usize).then_some(caught)
    }

    /// The value of `--menu-kinds=`: species of the game.
    pub fn kinds(value: &str) -> Option<Vec<u32>> {
        let known = |species: &u32| *species < caught_list::SPECIES;
        (value.split(','))
            .map(|species| species.parse().ok().filter(known))
            .collect()
    }

    fn species(value: &str) -> Option<Vec<(u32, u32)>> {
        (value.split(','))
            .map(|creature| {
                let (species, level) = creature.split_once(':').unwrap_or((creature, "1"));
                let level: u32 = level
                    .parse()
                    .ok()
                    .filter(|level| (1..=32).contains(level))?;
                Some((species.parse().ok()?, level))
            })
            .collect()
    }

    /// The value of `--menu-items=`: items with a count each, 1 where none is given.
    pub fn items(value: &str) -> Option<Vec<(i32, i32)>> {
        (value.split(','))
            .map(|item| {
                let (number, count) = item.split_once(':').unwrap_or((item, "1"));
                let count: i32 = count
                    .parse()
                    .ok()
                    .filter(|count| (1..=20).contains(count))?;
                let number: i32 = number.parse().ok().filter(|n| (0..43).contains(n))?;
                Some((number, count))
            })
            .collect()
    }

    /// The value of `--menu-gear=`: a kind, a thing of that kind and a count each, 1
    /// where none is given.
    pub fn gear(value: &str) -> Option<Vec<(usize, i32, i32)>> {
        (value.split(','))
            .map(|thing| {
                let mut parts = thing.split(':');
                let kind: usize = parts.next()?.parse().ok().filter(|kind| *kind < 3)?;
                let most = equip_panel::THINGS[kind] as i32;
                let number: i32 = parts.next()?.parse().ok()?;
                let count: i32 = parts.next().map_or(Some(1), |count| count.parse().ok())?;
                let fits = (0..most).contains(&number) && (1..=20).contains(&count);
                (fits && parts.next().is_none()).then_some((kind, number, count))
            })
            .collect()
    }

    /// The value of `--menu-things=`: things of the story that have a name.
    pub fn things(value: &str) -> Option<Vec<i32>> {
        let named = 0..i32::from(player_menu::STORY_THINGS);
        (value.split(','))
            .map(|thing| thing.parse().ok().filter(|thing| named.contains(thing)))
            .collect()
    }
}

/// Whether a thing of the story can be used where the player stands. Thing 0 goes by
/// whether the room is in a forest (`0x8003419C`: [`in_a_forest`] for the room the menu
/// came up over, the look's `--menu-forest` for the menu alone). Any other thing goes
/// by an object of the room that waits for it (`0x800A5E10`): `waits` holds the things
/// the marked objects of the room name (`Scripts::waited_things`).
fn thing_fits(thing: i16, forest: bool, waits: &[i16]) -> bool {
    match thing {
        0 => forest,
        _ => waits.contains(&thing),
    }
}

struct Held<'a> {
    keeping: &'a mut Keeping,
    fight: &'a FightTables,
    draws: &'a mut Random,
    player: &'a mut Record,
    creatures: &'a mut [Record],
    forest: bool,
    waits: &'a [i16],
    free: bool,
    count: &'a mut ItemCount,
}

impl Held<'_> {
    fn place(place: i16) -> usize {
        usize::try_from(place).unwrap_or(usize::MAX)
    }
}

impl Things for Held<'_> {
    fn item_at(&self, place: i16) -> u8 {
        self.keeping.number_at(Self::place(place), Kind::Item)
    }

    fn count_at(&self, place: i16) -> u8 {
        self.keeping.count_at(Self::place(place), Kind::Item)
    }

    fn last_place(&self) -> i16 {
        self.keeping.last_place(Kind::Item) as i16
    }

    fn flags(&self, item: u8) -> u16 {
        let item = self.fight.items.get(usize::from(item));
        item.map_or(0, |item| item.flags)
    }

    fn row_test(&self, place: i16) -> i16 {
        let item = self.item_at(place);
        let (count, flags) = (self.count_at(place), self.flags(item));
        using::row_test(count, item.into(), flags, false, self.forest)
    }

    fn aim_test(&self, item: u8, target: i16) -> i16 {
        let record = match target {
            0 => Some(&*self.player),
            n => self.creatures.get(Self::place(n - 1)),
        };
        record.map_or(aim::FINE, |record| {
            using::aim_test(item.into(), self.flags(item), Self::place(target), record)
        })
    }

    fn tidy(&mut self) {
        self.keeping.tidy(Kind::Item);
    }

    fn use_alone(&mut self, item: u8) {
        self.count.start(item);
        self.keeping.take(item.into(), 1, Kind::Item);
    }

    fn use_on(&mut self, item: u8, target: i16) {
        let number = usize::from(item);
        if target == 0 {
            using::raise(number, self.player, self.draws);
            let actor = self.player.clone();
            let result = using::resolved(self.fight, number, &actor, &actor, self.draws);
            if let Some(result) = result {
                using::apply(&result, self.player);
            }
        } else if let Some(record) = self.creatures.get_mut(Self::place(target - 1)) {
            using::raise(number, record, self.draws);
            let result = using::resolved(self.fight, number, self.player, record, self.draws);
            if let Some(result) = result {
                using::apply(&result, record);
            }
        }
        self.keeping.take(item.into(), 1, Kind::Item);
    }

    fn story_things(&self) -> i16 {
        self.keeping.story_things() as i16
    }

    fn story_thing_at(&self, n: i16) -> i16 {
        self.keeping.story_thing_at(Self::place(n)) as i16
    }

    fn has_story_thing(&self, thing: i16) -> bool {
        self.keeping.has_story_thing(thing.into())
    }

    fn thing_fits(&self, thing: i16) -> bool {
        thing_fits(thing, self.forest, self.waits)
    }

    fn free_to_use(&self) -> bool {
        self.free
    }

    fn spells_known(&self, creature: i16) -> i16 {
        let record = self.creatures.get(Self::place(creature));
        record.map_or(0, |record| player_panel::spells_known(record.spells))
    }

    fn in_a_forest(&self) -> bool {
        self.forest
    }
}

/// The layer of what the menu draws over a room; the movie of a script has layer 1.
const OVER_THE_ROOM: usize = 4;

/// What the menu puts over a room: its camera and its picture.
#[derive(Component)]
pub struct OverTheRoom;

/// A hidden run of a room with the menu: `--menu-at=FRAME[,FRAME...]` are the game
/// frames of a room's scripts in which the button of the menu goes down, and
/// `--menu-press=SECONDS:BUTTON,...[;...]` the buttons on the menu, counted from the
/// moment it opens: a set for every time it opens, in order, and the last set for
/// every time after that.
#[derive(Resource, Default)]
pub struct Opens {
    at: Vec<u32>,
    sets: Vec<Vec<Press>>,
    opened: usize,
}

impl Opens {
    /// The values of `--menu-at` and `--menu-press`.
    pub fn of(at: Option<&str>, presses: Option<&str>) -> Option<Self> {
        let at = at.map_or_else(
            || Some(Vec::new()),
            |value| value.split(',').map(|frame| frame.parse().ok()).collect(),
        )?;
        let sets = presses.map_or_else(
            || Some(Vec::new()),
            |value| {
                (value.split(';'))
                    .map(|set| match set {
                        "" => Some(Vec::new()),
                        set => crate::title::presses(set),
                    })
                    .collect()
            },
        )?;
        Some(Self {
            at,
            sets,
            opened: 0,
        })
    }

    /// Whether the run presses the button of the menu in this game frame.
    pub fn presses(&self, game_frame: u32) -> bool {
        self.at.contains(&game_frame)
    }

    /// The presses of the menu that opens now.
    fn next(&mut self) -> Vec<Press> {
        let set = self.sets.get(self.opened).or(self.sets.last());
        self.opened += 1;
        set.cloned().unwrap_or_default()
    }
}

/// The player's menu while it is up.
#[derive(Resource)]
pub struct MenuScreen {
    menu: PlayerMenu,
    tables: Tables,
    /// The player's record, his name as a text, and the counts of the small boxes.
    player: Record,
    name: Vec<u16>,
    counts: Counts,
    /// The creatures he carries, and their names as texts.
    creatures: Vec<Record>,
    creature_names: Vec<Vec<u16>>,
    /// What each of them keeps of where it comes from.
    families: Vec<Family>,
    /// The twelve places of the caught creatures that wait, and their names.
    waiting: Vec<Option<Record>>,
    waiting_names: Vec<Vec<u16>>,
    kinds: Kinds,
    list: KindList,
    list_up: bool,
    list_drawn: Option<bool>,
    kind_names: Vec<Vec<u16>>,
    mark: Vec<u16>,
    /// The five values a fight counts with, for each of them.
    creature_values: Vec<[u8; 5]>,
    /// The state of the game with what the player keeps and has on, the tables of the
    /// fights and the generator a use draws from; whether an item was used and whether
    /// what he has on changed, so that the game takes the change.
    carried: Carried,
    fight: FightTables,
    draws: Random,
    used: bool,
    changed: bool,
    /// The lines of the two rows of entries: the player's and a creature's.
    entries: [Vec<Vec<Piece>>; 2],
    /// The player's "Detail": its numbers, the lines of what he has on, and the name
    /// of his rank.
    detail: Detail,
    worn_lines: Vec<Vec<Piece>>,
    rank: Vec<u16>,
    /// The screen behind "Equip" while it is up, the three lines of its list, the small
    /// picture, the name and the description of every weapon, armour and third thing,
    /// the line of its message, the colours of a number that would stay, rise or fall,
    /// and the five numbers as the thing under the list's cursor would make them.
    equip: Option<EquipScreen>,
    options: Option<OptionsScreen>,
    options_aim: Aim<options_panel::Spot>,
    ended: bool,
    /// What the options screen brings in its own file, and the settings of the app as
    /// the last pass left them, for the painter.
    options_look: Look,
    settings: options_screen::Settings,
    equip_lines: Vec<Vec<Piece>>,
    gear: [Vec<(Option<Picture>, Vec<u16>)>; 3],
    gear_texts: [Vec<[Vec<u16>; message::LINES]>; 3],
    refusal: Vec<u16>,
    digit_colours: [[u8; 3]; 3],
    would: [u8; 5],
    font: Font,
    font_picture: Picture,
    small: Font,
    small_picture: Picture,
    /// The menus' sprite pool.
    sprites: Vec<Picture>,
    frames: Frames,
    /// The colours of a bar's rows by the image of the fonts' file it is made of.
    bars: Bars,
    face: Picture,
    /// The lists of the line of help by their address.
    help: HashMap<u32, Vec<(usize, Vec<u16>)>>,
    /// The texts of the messages by their result.
    messages: HashMap<i16, Vec<u16>>,
    /// The list of the items: its four lines, the small picture, the name and the
    /// description of every item, the font of the texts in its second row of colours,
    /// the colour of dim digits and the scroll sign's image.
    item_lines: Vec<Vec<Piece>>,
    item_looks: Vec<(Option<Picture>, Vec<u16>)>,
    descriptions: Vec<[Vec<u16>; message::LINES]>,
    dim_font: Picture,
    dim_small: Picture,
    dim_digits: [u8; 3],
    signs: Vec<Picture>,
    /// The list of the things of the story: its four lines, the small picture, the
    /// name and the text of every thing, and the question whether to leave the forest
    /// with its answers.
    story_lines: Vec<Vec<Piece>>,
    thing_looks: Vec<(Option<Picture>, Vec<u16>)>,
    thing_texts: Vec<[Vec<u16>; message::LINES]>,
    question: [Vec<u16>; 2],
    /// A creature's two lists: their lines, the sprite of every spell and the five of
    /// every row of the body attacks, and the names and texts of the spells, the
    /// attacks and the grades.
    spell_lines: Vec<Vec<Piece>>,
    attack_lines: Vec<Vec<Piece>>,
    spell_pictures: [u8; spells::SPELLS],
    attack_pictures: [[u8; 5]; attacks::ROWS],
    spell_names: Vec<Vec<u16>>,
    attack_names: Vec<Vec<u16>>,
    grade_names: Vec<Vec<u16>>,
    spell_texts: Vec<[Vec<u16>; message::LINES]>,
    attack_texts: Vec<[Vec<u16>; message::LINES]>,
    /// Whether the room counts as one of a forest ([`thing_fits`]).
    forest: bool,
    /// The things of the story a marked object of the room waits for ([`thing_fits`]),
    /// and the one the menu handed over, which goes to the room when the menu is gone.
    waits: Vec<i16>,
    handed: Option<i16>,
    /// Whether the menu may use a thing of the story at all (`0x800C11F8`).
    free: bool,
    /// The HP and the MP a use of the item under the cursor would give its target, as
    /// the last pass asked for them.
    gain: (i32, i32),
    /// Whether the circle is held, for the description of an item.
    circle: bool,
    pad: Option<Pad>,
    /// Passes of the menu's task so far, and the share of one that is owed.
    passes: u32,
    owed: f64,
    time: f64,
    presses: Vec<Press>,
    shown: Entity,
    /// The spot the mouse steers the cursor to (`pointer.rs`), on the menu's own
    /// screens and on the screen behind "Equip".
    aim: Aim<Spot>,
    equip_aim: Aim<equip_panel::Spot>,
    /// Whether a room stands behind the menu, to which it leaves.
    over_room: bool,
    /// Where that room lies, for the task that leaves the forest, and whether the
    /// player has said yes to leaving it (the word `0x800C2BB4`).
    way_out: Option<WayOut>,
    leaves_forest: bool,
}

pub type WayOut = ([i32; leave_forest::VIEWS], leave_forest::From);

/// What the menu is told of the room it comes up over.
pub struct Stands {
    /// The room is in a forest, seen from the view that is shown.
    pub forest: bool,
    /// The things of the story a marked object of the room waits for (`0x800A5E10`).
    pub waits: Vec<i16>,
    pub way_out: Option<WayOut>,
    pub free: bool,
}

/// A flat triangle as the console fills one: a pixel belongs to it when its corner lies
/// inside, and of the edges only the upper and the left ones count.
pub(crate) fn triangle(screen: &mut [u8], corners: [(i16, i16); 3], colour: [u8; 3], clip: Rect) {
    let [mut a, mut b, c] = corners.map(|(x, y)| (i32::from(x), i32::from(y)));
    let area = |a: (i32, i32), b: (i32, i32), p: (i32, i32)| {
        (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0)
    };
    if area(a, b, c) < 0 {
        std::mem::swap(&mut a, &mut b);
    }
    // With the corners in this order the inside is on the positive side of each edge.
    let edges = [(a, b), (b, c), (c, a)];
    let xs = corners.map(|corner| corner.0);
    let ys = corners.map(|corner| corner.1);
    let (Some(&left), Some(&right)) = (xs.iter().min(), xs.iter().max()) else {
        return;
    };
    let (Some(&top), Some(&bottom)) = (ys.iter().min(), ys.iter().max()) else {
        return;
    };
    for (x, y) in (top..=bottom).flat_map(|y| (left..=right).map(move |x| (x, y))) {
        let point = (i32::from(x), i32::from(y));
        let covered = edges.iter().all(|&(from, to)| {
            let side = area(from, to, point);
            // An edge counts when it is an upper one (it runs to the right along a
            // row) or a left one (it runs upwards).
            let counts = (to.1 == from.1 && to.0 > from.0) || to.1 < from.1;
            side > 0 || (side == 0 && counts)
        });
        if covered && inside(clip, x, y) {
            blend(screen, x, y, colour, None);
        }
    }
}

/// The colours of the bars of the gauges by the number of their image in the fonts'
/// file. A bar is the first column of its image: a colour in each of its rows.
pub(crate) type Bars = HashMap<usize, [[u8; 3]; bar::HIGH]>;

pub(crate) fn bars(font_set: &[tim::Tim]) -> Result<Bars, Failure> {
    let mut bars = HashMap::new();
    for number in [bar::USUAL, bar::LOW, bar::EXPERIENCE] {
        let image = picture(font_set, number)?;
        if image.wide == 0 || image.high < bar::HIGH {
            return Err("the image of a bar is too small".into());
        }
        let rows = std::array::from_fn(|row| {
            let texel = row * image.wide * 4;
            [0, 1, 2].map(|part| image.rgba[texel + part])
        });
        bars.insert(number, rows);
    }
    Ok(bars)
}

/// A bar of a gauge on the screen where it lies inside `clip`.
pub(crate) fn put_bar(
    screen: &mut [u8],
    rows: &[[u8; 3]; bar::HIGH],
    length: i16,
    at: (i16, i16),
    clip: Rect,
) {
    for (row, colour) in (0i16..).zip(rows) {
        for x in 0..length {
            let (x, y) = (at.0.wrapping_add(x), at.1.wrapping_add(row));
            let inside = (clip.x..clip.x.saturating_add(clip.w)).contains(&x)
                && (clip.y..clip.y.saturating_add(clip.h)).contains(&y);
            if inside {
                blend(screen, x, y, *colour, None);
            }
        }
    }
}

/// The face a model file of the player brings: the picture of its texture set that lies
/// where the face routine looks for the player's.
pub(crate) fn face(
    disc: &mut DiscImage<std::fs::File>,
    archive: &Archive,
    exe: &Executable,
) -> Result<Picture, Failure> {
    let table = ModelTable::from_executable(&exe.image, exe.base)?;
    let faces = Faces::from_executable(&exe.image, exe.base)
        .ok_or("the executable does not hold the places of the faces")?;
    let side = window::FACE as u32;
    for file in PLAYER_FILES {
        let Some((_, record)) = table.by_file(file) else {
            continue;
        };
        let images = record.textures(&archive.read(disc, usize::from(file))?)?;
        let found = images.iter().find(|image| {
            (image.width, image.height) == (side, side)
                && (image.vram.x as i16, image.vram.y as i16) == faces.player.picture
        });
        if let Some(found) = found {
            let palette = found.palette(0).or_else(|| {
                let clut = found.clut.as_ref()?;
                tim::shared_palette(&images, clut.rect, 0)
            });
            return Ok(Picture {
                rgba: found.to_rgba(palette)?,
                wide: side as usize,
                high: side as usize,
            });
        }
    }
    Err("no model file of the player brings his face".into())
}

impl MenuScreen {
    /// The parts the screens of the menus share (`options_view`).
    fn parts(&self) -> Parts<'_> {
        Parts {
            sprites: &self.sprites,
            frames: &self.frames,
            font: &self.font,
            font_picture: &self.font_picture,
        }
    }

    /// The small picture and the name of what the player has on in a place.
    fn worn_look(&self, place: usize) -> Option<&(Option<Picture>, Vec<u16>)> {
        let thing = usize::from(*self.carried.worn.get(place)?);
        self.gear.get(place)?.get(thing)
    }

    /// The first five of the six values a fight counts with, for the player with a
    /// thing on in each of the three places (`0x80033680`).
    fn values_with(&self, worn: [u8; 3]) -> [u8; 5] {
        let mut dressed = self.player.clone();
        dressed.equipment = worn;
        let counted = dressed.effective(&self.fight).stats;
        std::array::from_fn(|n| counted[n])
    }

    /// The numbers of the "Detail" follow what he has on.
    fn dress(&mut self) {
        self.detail.values = self.values_with(self.carried.worn);
        self.detail.worn = self.carried.worn.map(|thing| thing != FREE);
    }

    /// What the screen behind "Equip" draws in this pass.
    fn equip_panel(&self, screen: &EquipScreen, time: u32) -> Vec<Drawn> {
        let list = screen.list();
        let (_, first) = screen.list_place(screen.kind);
        let rows = std::array::from_fn(|n| {
            let place = Held::place(first.wrapping_add(n as i16));
            GearRow {
                thing: self.carried.keeping.number_at(place, list),
                count: self.carried.keeping.count_at(place, list),
            }
        });
        let shown = equip_panel::Shown {
            worn_lines: &self.worn_lines,
            list_lines: &self.equip_lines,
            worn: self.carried.worn,
            values: self.detail.values,
            would: self.would,
            rows,
            last_place: self.carried.keeping.last_place(list) as i16,
        };
        equip_panel::drawn(screen, &shown, time, |number| self.width(number))
    }

    fn sprite(&self, screen: &mut [u8], number: usize, at: (i16, i16), clip: Rect) {
        self.parts().sprite(screen, number, at, clip);
    }

    /// The rows of the list of the items from its kept first line on.
    fn item_rows(&self) -> [ItemRow; items::ROWS] {
        let first = self.menu.first_item();
        std::array::from_fn(|n| {
            let place = Held::place(first.wrapping_add(n as i16));
            let item = self.carried.keeping.number_at(place, Kind::Item);
            let count = self.carried.keeping.count_at(place, Kind::Item);
            let flags = (self.fight.items.get(usize::from(item))).map_or(0, |item| item.flags);
            let test = using::row_test(count, item.into(), flags, false, self.forest);
            ItemRow {
                item,
                // A place whose item is none of the game's shows the count 0.
                count: if item < items::ITEMS { count } else { 0 },
                refused: test != using::row::FINE,
            }
        })
    }

    /// The list of the items with the box in front of it: the description of the item
    /// under the cursor while the circle is held on the list's screen, else who gets
    /// it; neither while a message is up.
    fn item_panel(&self, time: u32) -> Vec<Drawn> {
        let menu = &self.menu;
        let last = self.carried.keeping.last_place(Kind::Item) as i16;
        let mut drawn =
            player_panel::item_list(menu, &self.item_lines, self.item_rows(), last, time);
        if menu.said != 0 {
            return drawn;
        }
        let place = Held::place(menu.item_place());
        let item = self.carried.keeping.number_at(place, Kind::Item);
        if player_panel::describes(menu, item, self.circle) {
            drawn.extend(player_panel::description(item));
            return drawn;
        }
        let creature = match menu.screen {
            player_menu::TARGET => usize::try_from(menu.target() - 1).ok(),
            _ => None,
        };
        let record = match creature {
            Some(creature) => self.creatures.get(creature),
            None => Some(&self.player),
        };
        if let Some(record) = record {
            let gain = self.gain.0;
            drawn.extend(player_panel::who_gets(creature, record, gain, self.passes));
        }
        drawn
    }

    /// The thing of the story at a place among the ones that are held.
    fn thing_at(&self, place: i16) -> i16 {
        self.carried.keeping.story_thing_at(Held::place(place)) as i16
    }

    /// The list of the things of the story with the box in front of it: the text of
    /// the thing under the cursor while the circle is held on the list's screen, else
    /// the head of the player's box; neither while a message is up, and no head
    /// behind the question.
    fn story_panel(&self, time: u32) -> Vec<Drawn> {
        let menu = &self.menu;
        let first = menu.first_thing();
        let rows = std::array::from_fn(|n| {
            let thing = self.thing_at(first.wrapping_add(n as i16));
            let usable = player_menu::shows_own_text(thing)
                || thing_fits(thing, self.forest, &self.waits) && self.free;
            StoryRow { thing, usable }
        });
        let held = self.carried.keeping.story_things() as i16;
        let mut drawn = player_panel::story_list(menu, &self.story_lines, rows, held, time);
        if menu.said != 0 {
            return drawn;
        }
        let thing = self.thing_at(menu.thing_place());
        if player_panel::thing_describes(menu, thing, self.circle) {
            drawn.extend(player_panel::thing_description(thing));
        } else if menu.screen != player_menu::LEAVE_FOREST {
            drawn.extend(player_panel::who_gets(None, &self.player, 0, self.passes));
        }
        drawn
    }

    /// What the choice of who gets the item adds to the list: the picker of the
    /// target, each row closed where the test of the target refuses, and the line of
    /// mana of a carried creature under the cursor.
    fn target_panel(&self, time: u32) -> Vec<Drawn> {
        let menu = &self.menu;
        let place = Held::place(menu.item_place());
        let item = self.carried.keeping.number_at(place, Kind::Item);
        let flags = (self.fight.items.get(usize::from(item))).map_or(0, |item| item.flags);
        let closed = std::array::from_fn(|target| {
            let record = match target {
                0 => Some(&self.player),
                n => self.creatures.get(n - 1),
            };
            record.is_some_and(|record| {
                let answer = using::aim_test(item.into(), flags, target, record);
                player_panel::picker_closes(target, answer)
            })
        });
        let creature = (usize::try_from(menu.target() - 1).ok())
            .filter(|_| menu.screen == player_menu::TARGET)
            .and_then(|creature| self.creatures.get(creature));
        player_panel::target_panel(menu, closed, creature, self.gain.1, time, self.passes)
    }

    /// The carried creature the lists behind a creature's row are about.
    fn listed_creature(&self) -> Option<(usize, &Record)> {
        let n = usize::try_from(self.menu.carried_creature()).ok()?;
        Some((n, self.creatures.get(n)?))
    }

    /// The list of a creature's spells with the box in front of it: the text of the
    /// spell under the cursor while the circle is held on the list's screen, else the
    /// head of the creature's box; neither while a message is up.
    fn spell_panel(&self, time: u32) -> Vec<Drawn> {
        let Some((n, record)) = self.listed_creature() else {
            return Vec::new();
        };
        let menu = &self.menu;
        let first = menu.first_spell();
        let rows = std::array::from_fn(|row| {
            player_panel::spell_at(record.spells, first.wrapping_add(row as i16))
        });
        let known = player_panel::spells_known(record.spells);
        let pictures = &self.spell_pictures;
        let mut drawn =
            player_panel::spell_list(menu, &self.spell_lines, rows, known, pictures, time);
        if menu.said != 0 {
            return drawn;
        }
        let spell = player_panel::spell_at(record.spells, menu.spell_place());
        if player_panel::spell_describes(menu, spell, self.circle) {
            drawn.extend(player_panel::list_description(Listed::Spell(spell)));
        } else {
            drawn.extend(player_panel::who_gets(Some(n), record, 0, self.passes));
        }
        drawn
    }

    /// The list of a creature's body attacks with the box in front of it: the text of
    /// the attack under the cursor while the circle is held on the list's screen, else
    /// the head of the creature's box.
    fn attack_panel(&self, time: u32) -> Vec<Drawn> {
        let Some((n, record)) = self.listed_creature() else {
            return Vec::new();
        };
        let menu = &self.menu;
        let rows: [AttackRow; attacks::ROWS] = std::array::from_fn(|row| {
            let entry = record.moves[row];
            let effect = self.fight.body_attacks.get(usize::from(entry.attack));
            AttackRow {
                known: entry.known,
                attack: entry.attack,
                grade: entry.grade,
                // An attack past the table has no element.
                class: effect.map_or(u8::MAX, |effect| effect.class),
                // Outside a fight no row is refused for its cost.
                refused: false,
            }
        });
        let pictures = &self.attack_pictures;
        let mut drawn = player_panel::attack_list(menu, &self.attack_lines, rows, pictures, time);
        let row = usize::try_from(menu.attack_row()).ok();
        let row = row.and_then(|row| rows.get(row)).copied();
        match row.filter(|&row| player_panel::attack_describes(menu, row, self.circle)) {
            Some(row) if menu.said == 0 => {
                let attack = Listed::Attack(row.attack.into());
                drawn.extend(player_panel::list_description(attack));
            }
            _ => drawn.extend(player_panel::who_gets(Some(n), record, 0, self.passes)),
        }
        drawn
    }

    /// The name a piece of a creature's lists shows.
    fn listed_name(&self, what: Listed) -> Option<&Vec<u16>> {
        match what {
            Listed::Spell(spell) => self.spell_names.get(spell),
            Listed::Attack(attack) => self.attack_names.get(attack),
            Listed::Grade(grade) => self.grade_names.get(grade),
            Listed::Sprite(_) => None,
        }
    }

    /// A text in the font of the texts, or in the small one.
    fn text(&self, screen: &mut [u8], small: bool, units: &[u16], at: (i16, i16), clip: Rect) {
        let (font, picture) = match small {
            true => (&self.small, &self.small_picture),
            false => (&self.font, &self.font_picture),
        };
        self.glyphs(screen, font, picture, units, at, clip);
    }

    /// A text in a font, from a picture of the font's image.
    fn glyphs(
        &self,
        screen: &mut [u8],
        font: &Font,
        picture: &Picture,
        units: &[u16],
        at: (i16, i16),
        clip: Rect,
    ) {
        options_view::glyphs(screen, font, picture, units, at, clip);
    }

    /// A frame of the menus over what the screen holds.
    fn frame(&self, screen: &mut [u8], rect: Rect, style: Style, time: u32) {
        self.parts().frame(screen, rect, style, time);
    }

    /// One thing of a panel.
    fn put(&self, screen: &mut [u8], drawn: Drawn) {
        match drawn {
            Drawn::Frame { rect, mode, time } => {
                let style = Style {
                    kind: self.settings.colour as i16,
                    mode,
                    blend: 1,
                };
                self.frame(screen, rect, style, time);
            }
            Drawn::Sprite { number, at, clip } => self.sprite(screen, number, at, clip),
            Drawn::Number {
                value,
                ends,
                small,
                clip,
            } => {
                let width = |number: usize| {
                    (self.sprites.get(number)).map_or(0, |sprite| sprite.wide as i16)
                };
                for (number, at) in player_panel::digits(value, ends, small, width) {
                    self.sprite(screen, number, at, clip);
                }
            }
            Drawn::Bar {
                picture,
                length,
                at,
                clip,
            } => {
                if let Some(rows) = self.bars.get(&picture) {
                    put_bar(screen, rows, length, at, clip);
                }
            }
            Drawn::Name { at, clip } => self.text(screen, false, &self.name, at, clip),
            Drawn::CreatureName { creature, at, clip } => {
                let Some(name) = self.creature_names.get(creature) else {
                    return;
                };
                let name: Vec<u16> = (name.iter().copied())
                    .map(player_panel::in_capitals)
                    .collect();
                self.text(screen, true, &name, at, clip);
            }
            Drawn::Face { at, clip } => {
                let size = (self.face.wide, self.face.high);
                blit(screen, &self.face, (0, 0), size, at, clip);
            }
            Drawn::Rank { at, clip } => self.text(screen, false, &self.rank, at, clip),
            Drawn::WornPicture { place, at, clip } => {
                if let Some((Some(picture), _)) = self.worn_look(place) {
                    blit(
                        screen,
                        picture,
                        (0, 0),
                        (picture.wide, picture.high),
                        at,
                        clip,
                    );
                }
            }
            Drawn::WornName { place, at, clip } => {
                if let Some((_, name)) = self.worn_look(place) {
                    self.text(screen, false, name, at, clip);
                }
            }
            // No list of the player's menu has a dim row of these.
            Drawn::GearPicture {
                kind,
                thing,
                at,
                clip,
                ..
            } => {
                let look = (self.gear.get(kind)).and_then(|looks| looks.get(usize::from(thing)));
                if let Some((Some(picture), _)) = look {
                    let size = (picture.wide, picture.high);
                    blit(screen, picture, (0, 0), size, at, clip);
                }
            }
            Drawn::GearName {
                kind,
                thing,
                at,
                clip,
                ..
            } => {
                let look = (self.gear.get(kind)).and_then(|looks| looks.get(usize::from(thing)));
                if let Some((_, name)) = look {
                    self.text(screen, false, name, at, clip);
                }
            }
            Drawn::GearDescribed {
                kind,
                thing,
                line,
                at,
            } => {
                let texts = self.gear_texts.get(kind);
                let lines = texts.and_then(|texts| texts.get(usize::from(thing)));
                if let Some(said) = lines.and_then(|lines| lines.get(line)) {
                    self.text(screen, false, said, at, WHOLE);
                }
            }
            Drawn::GearRefused { at } => self.text(screen, false, &self.refusal, at, WHOLE),
            // The pieces only the options screen has are its painter's.
            piece @ (Drawn::FrameOf { .. }
            | Drawn::DimSprite { .. }
            | Drawn::OptionsHelp { .. }
            | Drawn::OptionsRefused { .. }) => {
                let colour = self.settings.colour as i16;
                self.parts().put(screen, &self.options_look, colour, piece);
            }
            // The pieces only the screen of the blacksmith and the shop has are its
            // painter's (`shop_view`).
            Drawn::LongNumber { .. }
            | Drawn::WindowFrame { .. }
            | Drawn::WindowFill { .. }
            | Drawn::KeeperFace { .. }
            | Drawn::KeeperSaid { .. } => {}
            // The rank's name that changes and the arrow of a difference are the
            // results screen's (`results_view`).
            Drawn::RankName { .. } | Drawn::Arrow { .. } => {}
            // The names of the places and the sign of "more" are the screen of the
            // caught's (`merge_view`).
            Drawn::PlaceName { .. } | Drawn::More { .. } | Drawn::SignOf { .. } => {}
            Drawn::Tinted {
                value,
                ends,
                colour,
                clip,
            } => {
                let colour = self.digit_colours.get(colour).copied().unwrap_or([128; 3]);
                let width = |number: usize| self.width(number);
                for (number, at) in player_panel::digits(value, ends, false, width) {
                    if let Some(sprite) = self.sprites.get(number) {
                        blit_lit(screen, sprite, at, colour, clip);
                    }
                }
            }
            Drawn::Message { at } => {
                if let Some(said) = self.messages.get(&self.menu.said) {
                    self.text(screen, false, said, at, WHOLE);
                }
            }
            Drawn::FullName { creature, at, clip } => {
                if let Some(name) = self.creature_names.get(creature) {
                    self.text(screen, false, name, at, clip);
                }
            }
            Drawn::CaughtName { caught, at, clip } => {
                if let Some(name) = self.waiting_names.get(caught) {
                    let name: Vec<u16> = (name.iter().copied())
                        .map(player_panel::in_capitals)
                        .collect();
                    self.text(screen, true, &name, at, clip);
                }
            }
            Drawn::KindMark { at } => self.text(screen, true, &self.mark, at, WHOLE),
            Drawn::KindName { species, at } => {
                if let Some(name) = self.kind_names.get(species as usize) {
                    self.text(screen, true, name, at, WHOLE);
                }
            }
            Drawn::AncestorName {
                creature,
                ancestor,
                at,
            } => {
                let family = self.families.get(creature);
                if let Some(name) = family.and_then(|family| family.names.get(ancestor)) {
                    let name: Vec<u16> = (name.iter().copied())
                        .map(player_panel::in_capitals)
                        .collect();
                    self.text(screen, true, &name, at, WHOLE);
                }
            }
            Drawn::Triangle {
                corners,
                colour,
                clip,
            } => triangle(screen, corners, colour, clip),
            Drawn::Line {
                from,
                to,
                grey,
                clip,
            } => {
                // The lines of the menu run along the screen's rows and columns.
                let (xs, ys) = (
                    from.0.min(to.0)..=from.0.max(to.0),
                    from.1.min(to.1)..=from.1.max(to.1),
                );
                for (x, y) in ys.flat_map(|y| xs.clone().map(move |x| (x, y))) {
                    if inside(clip, x, y) {
                        blend(screen, x, y, [grey; 3], None);
                    }
                }
            }
            Drawn::Sign { turns, at, time } => {
                let Some(image) = self.signs.get(usize::from(self.settings.colour)) else {
                    return;
                };
                options_view::put_sign(screen, image, turns, at, time);
            }
            Drawn::ItemPicture {
                item,
                at,
                dim,
                clip,
            } => {
                if let Some((Some(picture), _)) = self.item_looks.get(usize::from(item)) {
                    let colour = [menu::brightness(if dim { menu::piece::DIM } else { 0 }); 3];
                    blit_lit(screen, picture, at, colour, clip);
                }
            }
            Drawn::ItemName {
                item,
                at,
                dim,
                clip,
            } => {
                if let Some((_, name)) = self.item_looks.get(usize::from(item)) {
                    let picture = match dim {
                        true => &self.dim_font,
                        false => &self.font_picture,
                    };
                    self.glyphs(screen, &self.font, picture, name, at, clip);
                }
            }
            Drawn::Count {
                value,
                ends,
                dim,
                clip,
            } => {
                let colour = if dim { self.dim_digits } else { [128; 3] };
                let width = |number: usize| self.width(number);
                for (number, at) in player_panel::digits(value, ends, false, width) {
                    if let Some(sprite) = self.sprites.get(number) {
                        blit_lit(screen, sprite, at, colour, clip);
                    }
                }
            }
            Drawn::SmallName { at, clip } => {
                let name: Vec<u16> = (self.name.iter().copied())
                    .map(player_panel::in_capitals)
                    .collect();
                self.text(screen, true, &name, at, clip);
            }
            Drawn::PickedName {
                target,
                middle,
                dim,
                clip,
            } => {
                let name = match target.checked_sub(1) {
                    None => Some(&self.name),
                    Some(creature) => self.creature_names.get(creature),
                };
                let Some(name) = name else {
                    return;
                };
                let name: Vec<u16> = (name.iter().copied())
                    .map(player_panel::in_capitals)
                    .collect();
                let width = self.small.length(&self.small.codes(&name));
                let picture = match dim {
                    true => &self.dim_small,
                    false => &self.small_picture,
                };
                let at = (middle.0.wrapping_sub(width / 2), middle.1);
                self.glyphs(screen, &self.small, picture, &name, at, clip);
            }
            Drawn::Described { item, line, at } => {
                let lines = self.descriptions.get(usize::from(item));
                if let Some(said) = lines.and_then(|lines| lines.get(line)) {
                    self.text(screen, false, said, at, WHOLE);
                }
            }
            Drawn::ThingPicture {
                thing,
                at,
                dim,
                clip,
            } => {
                let look = usize::try_from(thing)
                    .ok()
                    .and_then(|n| self.thing_looks.get(n));
                if let Some((Some(picture), _)) = look {
                    let colour = [menu::brightness(if dim { menu::piece::DIM } else { 0 }); 3];
                    blit_lit(screen, picture, at, colour, clip);
                }
            }
            Drawn::ThingName {
                thing,
                at,
                dim,
                clip,
            } => {
                let look = usize::try_from(thing)
                    .ok()
                    .and_then(|n| self.thing_looks.get(n));
                if let Some((_, name)) = look {
                    let picture = match dim {
                        true => &self.dim_font,
                        false => &self.font_picture,
                    };
                    self.glyphs(screen, &self.font, picture, name, at, clip);
                }
            }
            Drawn::ThingDescribed { thing, line, at } => {
                let lines = usize::try_from(thing)
                    .ok()
                    .and_then(|n| self.thing_texts.get(n));
                if let Some(said) = lines.and_then(|lines| lines.get(line)) {
                    self.text(screen, false, said, at, WHOLE);
                }
            }
            Drawn::Asked { text, at } => {
                if let Some(said) = self.question.get(text) {
                    self.text(screen, false, said, at, WHOLE);
                }
            }
            Drawn::Listed {
                what: Listed::Sprite(number),
                room,
                flags,
                dim,
                clip,
            } => {
                if let Some(sprite) = self.sprites.get(number) {
                    let at = menu::placed(flags, room, sprite.wide as i16);
                    let colour = [menu::brightness(if dim { menu::piece::DIM } else { 0 }); 3];
                    blit_lit(screen, sprite, at, colour, clip);
                }
            }
            Drawn::Listed {
                what,
                room,
                flags,
                dim,
                clip,
            } => {
                if let Some(name) = self.listed_name(what) {
                    let width = self.font.length(&self.font.codes(name));
                    let picture = match dim {
                        true => &self.dim_font,
                        false => &self.font_picture,
                    };
                    let at = menu::placed(flags, room, width);
                    self.glyphs(screen, &self.font, picture, name, at, clip);
                }
            }
            Drawn::ListDescribed { what, line, at } => {
                let lines = match what {
                    Listed::Spell(spell) => self.spell_texts.get(spell),
                    Listed::Attack(attack) => self.attack_texts.get(attack),
                    _ => None,
                };
                if let Some(said) = lines.and_then(|lines| lines.get(line)) {
                    self.text(screen, false, said, at, WHOLE);
                }
            }
        }
    }

    /// The width of a sprite of the pool, as the drawers place by it.
    fn width(&self, number: usize) -> i16 {
        (self.sprites.get(number)).map_or(0, |sprite| sprite.wide as i16)
    }

    /// The screen of this pass, four bytes a pixel; the PC Settings are drawn over the
    /// options while they are up.
    fn paint(&self, pc: &PcSettings, prefs: &Prefs) -> Vec<u8> {
        let mut screen = [0, 0, 0, 255].repeat(SCREEN.0 * SCREEN.1);
        // The wall lies in the slot behind the frames', whole from the first pass on.
        for at in player_panel::wall() {
            self.sprite(&mut screen, player_panel::sprite::WALL, at, WHOLE);
        }
        let time = menu::frame_time(self.passes, GAME_FRAME);
        let shown = self.menu.shown(&self.tables);
        let width = |number: usize| self.width(number);
        // While the list of the caught kinds is the task's routine it draws itself and
        // the menu nothing; its line of help lies with the wall, behind the rest.
        if let Some(whole) = self.list_drawn {
            self.legend(&mut screen, player_panel::HELP_LEAVE);
            for drawn in caught_list::drawn(&self.list, &self.kinds, whole, time) {
                self.put(&mut screen, drawn);
            }
            return screen;
        }
        // While the screen behind "Equip" is up it draws itself, and the menu its row
        // of entries and nothing else.
        if let Some(equip) = self.equip.as_ref().filter(|_| self.menu.waits()) {
            for drawn in self.equip_panel(equip, time) {
                self.put(&mut screen, drawn);
            }
            let row = player_panel::entries(&self.menu, false, &self.entries[0], time, width);
            for drawn in row {
                self.put(&mut screen, drawn);
            }
            if equip.up {
                self.legend(&mut screen, equip_panel::HELP);
            }
            return screen;
        }
        // While the options screen is up it draws itself and the menu nothing.
        if let Some(options) = self.options.as_ref().filter(|_| self.menu.waits()) {
            let look = &self.options_look;
            let pc = Pc {
                settings: pc,
                prefs,
                signs: &self.signs,
            };
            let shown = (options, &self.settings);
            (self.parts()).options(&mut screen, look, shown, time, pc);
            return screen;
        }
        // A message is added behind the panels, so it is drawn before them.
        for drawn in player_panel::message(&self.menu).into_iter().flatten() {
            self.put(&mut screen, drawn);
        }
        // The message of a thing with a text of its own is that text.
        if self.menu.said == player_menu::OWN_TEXT {
            let thing = self.thing_at(self.menu.thing_place());
            for drawn in player_panel::thing_description(thing) {
                self.put(&mut screen, drawn);
            }
        }
        // The panels share a slot, where the one of the highest bit is drawn first.
        if shown & player_panel::QUESTION_PANEL != 0 {
            for drawn in player_panel::question(&self.menu, time) {
                self.put(&mut screen, drawn);
            }
        }
        if shown & player_panel::TARGET_PANEL != 0 && self.menu.said == 0 {
            for drawn in self.target_panel(time) {
                self.put(&mut screen, drawn);
            }
        }
        if shown & player_panel::STORY_PANEL != 0 {
            for drawn in self.story_panel(time) {
                self.put(&mut screen, drawn);
            }
        }
        if shown & player_panel::ITEMS_PANEL != 0 {
            for drawn in self.item_panel(time) {
                self.put(&mut screen, drawn);
            }
        }
        // Beside "Equip": what he has on, and the five numbers the "Detail" shows.
        if shown & player_panel::EQUIP_PANEL != 0 {
            let panel =
                player_panel::equip_panel(&self.menu, &self.detail, &self.worn_lines, width);
            for drawn in panel {
                self.put(&mut screen, drawn);
            }
        }
        if shown & player_panel::CAUGHT_PANEL != 0 {
            for drawn in player_panel::caught_grid(&self.menu, &self.waiting, time) {
                self.put(&mut screen, drawn);
            }
        }
        let creature = usize::try_from(self.menu.carried_creature()).ok();
        // A creature's "Family" is whole at once: nothing of it opens.
        let of = creature.and_then(|n| Some((n, self.creatures.get(n)?, self.families.get(n)?)));
        if let Some((n, record, family)) = of.filter(|_| shown & player_panel::FAMILY_PANEL != 0) {
            for drawn in player_panel::family_panel(n, record, family) {
                self.put(&mut screen, drawn);
            }
        }
        if shown & player_panel::ATTACKS_PANEL != 0 {
            for drawn in self.attack_panel(time) {
                self.put(&mut screen, drawn);
            }
        }
        if shown & player_panel::CREATURE_ENTRIES != 0 {
            let row = player_panel::entries(&self.menu, true, &self.entries[1], time, width);
            for drawn in row {
                self.put(&mut screen, drawn);
            }
        }
        // A creature's "Detail" is the third entry's alone too.
        let of =
            creature.and_then(|n| Some((n, self.creatures.get(n)?, self.creature_values.get(n)?)));
        let on_detail = self.menu.screen == player_menu::OF_CREATURE && self.menu.cursor.at == 2;
        if let Some((n, record, &values)) = of.filter(|_| on_detail)
            && shown & player_panel::CREATURE_DETAIL != 0
        {
            for drawn in player_panel::creature_detail(&self.menu, n, record, values) {
                self.put(&mut screen, drawn);
            }
        }
        // The panel of the "Detail" is the third entry's alone, whatever the table says.
        let on_detail = self.menu.screen == player_menu::OF_PLAYER && self.menu.cursor.at == 2;
        if shown & player_panel::PLAYER_DETAIL != 0 && on_detail {
            let panel = player_panel::player_detail(
                &self.menu,
                &self.player,
                self.counts,
                &self.detail,
                &self.worn_lines,
                width,
            );
            for drawn in panel {
                self.put(&mut screen, drawn);
            }
        }
        if shown & player_panel::SPELLS_PANEL != 0 {
            for drawn in self.spell_panel(time) {
                self.put(&mut screen, drawn);
            }
        }
        if shown & player_panel::PLAYER_PANEL != 0 {
            for drawn in player_panel::player_panel(&self.menu, &self.player, self.counts, time) {
                self.put(&mut screen, drawn);
            }
        }
        if shown & player_panel::CREATURES_PANEL != 0 {
            for drawn in player_panel::creature_boxes(&self.menu, &self.creatures, time) {
                self.put(&mut screen, drawn);
            }
        }
        if shown & player_panel::PLAYER_ENTRIES != 0 {
            let row = player_panel::entries(&self.menu, false, &self.entries[0], time, width);
            for drawn in row {
                self.put(&mut screen, drawn);
            }
        }
        self.legend(
            &mut screen,
            player_panel::help_where(&self.menu, self.forest),
        );
        screen
    }

    /// The line of help of a list: its frame, added last, lies under the signs and the
    /// words.
    fn legend(&self, screen: &mut [u8], list: u32) {
        let pairs = self.help.get(&list).map_or(&[][..], Vec::as_slice);
        (self.parts()).legend(screen, self.settings.colour as i16, pairs);
    }

    /// What the log says of where the menu is.
    fn place(&self) -> (i16, u16, i16, bool) {
        let menu = &self.menu;
        (menu.screen, menu.flags, menu.cursor.at, menu.message)
    }
}

/// Reads what the screen needs from the disc and sets the menu up as its task starts it:
/// for the player of a new game, or for the one of the game that `running` carries,
/// with his name.
fn open(
    show: &ShowMenu,
    running: Option<(&Carried, &[u16])>,
    shown: Entity,
) -> Result<MenuScreen, Failure> {
    let config = LocalConfig::load()?;
    let mut disc = DiscImage::open(&config.disc_image)?;
    let archive = Archive::open(&mut disc)?;
    let exe = Executable::read(&mut disc)?;
    let fonts = Fonts::from_executable(&exe.image, exe.base)
        .ok_or("the executable does not hold the font tables")?;
    let frames = Frames::from_executable(&exe.image, exe.base)
        .ok_or("the executable does not hold the tables of the frames")?;
    let font = (fonts.get(text::TEXT_FONT).cloned()).ok_or("there is no font of the texts")?;
    let font_set = tim::texture_set(&archive.read(&mut disc, text::FONT_SET)?)
        .ok_or("the font file is not a texture set")?;
    let font_picture = picture(&font_set, font.image)?;
    let small = (fonts.get(text::SMALL_FONT).cloned()).ok_or("there is no small font")?;
    let small_picture = picture(&font_set, small.image)?;
    let bars = bars(&font_set)?;
    let sprite_set = tim::texture_set(&archive.read(&mut disc, name::SPRITES)?)
        .ok_or("the sprite file is not a texture set")?;
    let sprites = (sprite_set.iter())
        .map(Picture::of)
        .collect::<Result<Vec<_>, _>>()?;
    let face = face(&mut disc, &archive, &exe)?;
    let file = archive.read(&mut disc, player_menu::MENU_FILE)?;
    let tables = Tables::from_file(&file)?;
    // The words of the lists of help lie in the file of their list or in the executable.
    let word = |address: u32| -> Option<Vec<u16>> {
        let from = |image: &[u8], base: u32| {
            let offset = address.checked_sub(base)? as usize;
            image.get(offset..).map(text::units)
        };
        from(&file, player_menu::MENU_FILE_BASE)
            .filter(|_| address >= player_menu::MENU_FILE_BASE)
            .or_else(|| from(&exe.image, exe.base))
    };
    let mut help = HashMap::new();
    for list in [
        player_panel::HELP_OF_A_ROW,
        player_panel::HELP_OF_A_ROW_IN_A_FOREST,
        player_panel::HELP_OF_THIRD_ROW_IN_A_FOREST,
        player_panel::HELP_PLAIN,
        player_panel::HELP_LEAVE,
        player_panel::HELP_OF_ITEMS,
        player_panel::HELP_OF_STORY,
        player_panel::HELP_OF_LISTS,
    ] {
        let pairs = match list >= player_menu::MENU_FILE_BASE {
            true => player_panel::help_list(&file, player_menu::MENU_FILE_BASE, list),
            false => player_panel::help_list(&exe.image, exe.base, list),
        }
        .ok_or("a list of help is not where it should be")?;
        let pairs = (pairs.into_iter())
            .map(|(sprite, address)| Some((sprite, word(address)?)))
            .collect::<Option<Vec<_>>>()
            .ok_or("a word of a list of help is not in its file")?;
        help.insert(list, pairs);
    }
    let messages: HashMap<i16, Vec<u16>> = (1..message::RESULTS)
        .filter_map(|result| {
            let said = player_panel::message_text(&file, player_menu::MENU_FILE_BASE, result)?;
            Some((result, said))
        })
        .collect();
    if !messages.contains_key(&player_menu::NOTHING_CAUGHT) {
        return Err("the menu's file does not hold the texts of its messages".into());
    }
    // The player of a new game: the template of his record and the name the name entry
    // starts with, nothing carried and nothing caught. A running game brings its own
    // name, and its own record once a fight has left one.
    let (name_file, address) = name::DEFAULT_NAME;
    let title = archive.read(&mut disc, name_file)?;
    let name = match running {
        Some((_, name)) if !name.is_empty() => name.to_vec(),
        _ => (address.checked_sub(rooms::load_address(name_file)))
            .and_then(|offset| title.get(offset as usize..))
            .map(text::units)
            .ok_or("the default name is not in its file")?,
    };
    let mut carried = running.map_or_else(Carried::new_game, |(carried, _)| carried.clone());
    let mut player = match carried.player.clone() {
        Some(player) => player,
        None => Record::of_the_player(&title).ok_or("the title's file has no player")?,
    };
    // The look's own: items and things to put on that the options give, and the HP
    // one sets.
    let has_on = carried.worn;
    for &(item, count) in &show.items {
        if !carried.keeping.give(item, count, Kind::Item, has_on) {
            return Err(format!("{count} of item {item} do not fit the player's list").into());
        }
    }
    for &(kind, thing, count) in &show.gear {
        let list = [Kind::Weapon, Kind::Armour, Kind::Ornament][kind];
        if !carried.keeping.give(thing, count, list, has_on) {
            let said = format!("{count} of thing {thing} do not fit the list of kind {kind}");
            return Err(said.into());
        }
    }
    for &thing in &show.things {
        carried.keeping.give_story_thing(thing);
    }
    if let Some(hp) = show.hp {
        player.hp = hp.clamp(0, player.max_hp);
    }
    // The caught creatures that wait and the kinds that count as caught are the
    // running game's, and behind them the look's own.
    let mut kinds = carried.kinds;
    for &species in &show.kinds {
        kinds.mark(species);
    }
    let counts = Counts {
        cocoons: carried.keeping.cocoons(),
        waiting: (carried.caught.waiting().count() + show.caught.len()) as i32,
        kinds: kinds.count() as i32,
        floor: carried
            .corridor
            .as_ref()
            .map(|corridor| corridor.floor as i32),
    };
    // The list of the caught kinds: the name of every species that has a cell, and
    // the mark of a cell.
    let kind_names = (0..caught_list::SPECIES)
        .map(|species| match caught_list::listed(species) {
            true => caught_list::kind_name(&exe.image, exe.base, species),
            false => Some(Vec::new()),
        })
        .collect::<Option<Vec<_>>>()
        .ok_or("a listed species has no name")?;
    let mark = caught_list::mark_text(&file, player_menu::MENU_FILE_BASE)
        .ok_or("the menu's file does not hold the mark of the caught kinds")?;
    // The creatures of the option, as the set-up of a fight makes one of a species at
    // a level, which names it after its species.
    let fight = FightTables::read(&exe.image, exe.base)
        .ok_or("the executable does not hold the tables of the fights")?;
    let mut draws = Random { seed: 0 };
    let mut creatures = Vec::new();
    let mut creature_names = Vec::new();
    let mut families = Vec::new();
    let mut creature_values = Vec::new();
    // A running game brings the creatures its hero carries, in front of the look's own.
    let mut brought: Vec<Record> = (carried.creatures.carried())
        .take(player_panel::CREATURE_BOXES.len())
        .cloned()
        .collect();
    // What each of them keeps of where it comes from, which a merge has written.
    let carried_state = jc_formats::creatures::state::CARRIED;
    let lines: Vec<_> = (0..brought.len())
        .map_while(|n| carried.creatures.nth(carried_state, n))
        .map(|creature| creature.line.clone())
        .collect();
    let room_for = player_panel::CREATURE_BOXES.len() - brought.len();
    for &(species, level) in show.creatures.iter().take(room_for) {
        let experience = (level - 1).wrapping_mul(LEVEL);
        let mut record = Record::make(&fight, species, experience, &mut draws)
            .ok_or("the set-up makes no creature of that species")?;
        if let Some(hp) = show.hp {
            record.hp = hp.clamp(0, record.max_hp);
        }
        brought.push(record);
    }
    for (n, record) in brought.into_iter().enumerate() {
        let species = record.species;
        let name = name::units(&record.name, &small);
        info!(
            "the player's menu: carries species {species} ({:?}) at level {}: HP {} of {}, mana {} of {}, element bits {:#x}",
            name.iter().map(|&unit| letter(unit)).collect::<String>(),
            record.level,
            record.hp,
            record.max_hp,
            record.mp,
            record.max_mp,
            record.element_bit
        );
        let counted = record.effective(&fight).stats;
        info!(
            "the player's menu: its detail: values {:?}, strengths {:?}, growth {:?}, the marker at {:?}",
            &counted[..5],
            record.strengths,
            record.growth,
            player_panel::build_marker(record.growth)
        );
        creature_values.push(std::array::from_fn(|n| counted[n]));
        // A carried creature's family is what its record keeps. A creature of the look
        // was never merged: it is its own family, as a caught one is.
        families.push(match lines.get(n) {
            Some(line) => Family::of_a_line(line, |name| name::units(name, &small)),
            None => Family::of_a_caught(&name, record.element_bit),
        });
        creatures.push(record);
        creature_names.push(name);
    }
    // The caught creatures of the option, made the same way, in the first places.
    let mut waiting = Vec::new();
    let mut waiting_names = Vec::new();
    let mut waits: Vec<Record> = carried.caught.waiting().cloned().collect();
    for &(species, level) in &show.caught {
        let experience = (level - 1).wrapping_mul(LEVEL);
        let record = Record::make(&fight, species, experience, &mut draws)
            .ok_or("the set-up makes no creature of that species")?;
        waits.push(record);
    }
    for record in waits {
        let species = record.species;
        let name = name::units(&record.name, &small);
        info!(
            "the player's menu: species {species} ({:?}) at level {} waits in place {}",
            name.iter().map(|&unit| letter(unit)).collect::<String>(),
            record.level,
            waiting.len()
        );
        waiting.push(Some(record));
        waiting_names.push(name);
    }
    // The rows of entries and the lines of what the player has on, as the file has them.
    let lines = |list: u32| {
        Menu::lines_at(&file, player_menu::MENU_FILE_BASE, list)
            .ok_or("a list of the menu's lines is not where it should be")
    };
    let entries = [
        lines(player_panel::PLAYER_LINES)?,
        lines(player_panel::CREATURE_LINES)?,
    ];
    let worn_lines = lines(detail::WORN_LINES)?;
    // The "Detail": the values a fight counts with, which have what he has on added;
    // the things themselves with their small pictures and names; the rank; the money.
    let mut dressed = player.clone();
    dressed.equipment = carried.worn;
    let counted = dressed.effective(&fight).stats;
    let things = notice::Tables::new(&exe.image, exe.base);
    // Every weapon, armour and third thing with its small picture and its name, and
    // what the screen behind "Equip" brings in its own file: the lines of its list,
    // the descriptions and the line of its message.
    let gear: [Vec<ThingLook>; 3] = std::array::from_fn(|place| {
        let count = equip_panel::THINGS[place] as i32;
        thing_looks(&things, &font_set, place as i32 + 1, count)
    });
    let equip_file = archive.read(&mut disc, equip_screen::EQUIP_FILE)?;
    let equip_base = equip_screen::EQUIP_FILE_BASE;
    let equip_lines = Menu::lines_at(&equip_file, equip_base, equip_panel::LINES)
        .filter(|lines| lines.len() == equip_panel::ROWS)
        .ok_or("the file of the screen behind Equip does not hold the lines of its list")?;
    let mut gear_texts: [Vec<[Vec<u16>; message::LINES]>; 3] = Default::default();
    for (kind, texts) in gear_texts.iter_mut().enumerate() {
        *texts = (0..equip_panel::THINGS[kind])
            .map(|thing| equip_panel::description_text(&equip_file, equip_base, kind, thing))
            .collect::<Option<Vec<_>>>()
            .ok_or("the file of the screen behind Equip does not hold its descriptions")?;
    }
    let refusal = equip_panel::refusal_text(&equip_file, equip_base)
        .ok_or("the file of the screen behind Equip does not hold its message")?;
    // What the options screen brings in its own file: its lists of lines, the texts of
    // its line of help and of its refusals, and its legend.
    let options_file = archive.read(&mut disc, options_screen::OPTIONS_FILE)?;
    let options_look = Look::read(&options_file, &exe)?;
    let mut digit_colours = [[128; 3]; 3];
    for (entry, colour) in digit_colours.iter_mut().enumerate() {
        *colour = player_panel::digit_colour(&exe.image, exe.base, entry)
            .ok_or("the executable does not hold the colours of the digits")?;
    }
    let worn_name = |place: usize| -> &[u16] {
        let look = gear[place].get(usize::from(carried.worn[place]));
        look.map_or(&[], |look| &look.1)
    };
    // The list of the items: its lines, every item's small picture, name and
    // description, the dim colours of a refused row and the scroll sign.
    let item_lines = Menu::lines_at(&exe.image, exe.base, items::LINES)
        .filter(|lines| lines.len() == items::ROWS)
        .ok_or("the executable does not hold the lines of the items' list")?;
    let item_looks = thing_looks(&things, &font_set, 0, items::ITEMS.into());
    let descriptions = (0..items::ITEMS)
        .map(|item| player_panel::description_text(&file, player_menu::MENU_FILE_BASE, item))
        .collect::<Option<Vec<_>>>()
        .ok_or("the menu's file does not hold the descriptions of the items")?;
    // The list of the things of the story: its lines, every thing's small picture,
    // name and text, and the question of thing 0.
    let story_lines = Menu::lines_at(&file, player_menu::MENU_FILE_BASE, story::LINES)
        .filter(|lines| lines.len() == story::ROWS)
        .ok_or("the menu's file does not hold the lines of the things' list")?;
    let thing_looks = thing_looks(&things, &font_set, 5, player_menu::STORY_THINGS.into());
    let thing_texts = (0..player_menu::STORY_THINGS)
        .map(|thing| player_panel::thing_text(&file, player_menu::MENU_FILE_BASE, thing))
        .collect::<Option<Vec<_>>>()
        .ok_or("the menu's file does not hold the texts of the things of the story")?;
    let question = player_panel::question_texts(&file, player_menu::MENU_FILE_BASE)
        .ok_or("the menu's file does not hold the question of thing 0")?;
    // A creature's two lists: their lines, sprites, names and texts.
    let spell_lines = Menu::lines_at(&exe.image, exe.base, spells::LINES)
        .filter(|lines| lines.len() == spells::ROWS)
        .ok_or("the executable does not hold the lines of the spells' list")?;
    let attack_lines = Menu::lines_at(&exe.image, exe.base, attacks::LINES)
        .filter(|lines| lines.len() == attacks::ROWS)
        .ok_or("the executable does not hold the lines of the body attacks' list")?;
    let spell_pictures = player_panel::spell_pictures(&exe.image, exe.base)
        .ok_or("the executable does not hold the sprites of the spells")?;
    let attack_pictures = player_panel::attack_pictures(&exe.image, exe.base)
        .ok_or("the executable does not hold the sprites of the body attacks")?;
    let spell_names = (0..spells::SPELLS)
        .map(|spell| player_panel::spell_name(&exe.image, exe.base, spell))
        .collect::<Option<Vec<_>>>()
        .ok_or("the executable does not hold the names of the spells")?;
    let attack_names = (0..attacks::ATTACKS)
        .map(|attack| player_panel::attack_name(&exe.image, exe.base, attack))
        .collect::<Option<Vec<_>>>()
        .ok_or("the executable does not hold the names of the body attacks")?;
    let grade_names = (0..attacks::GRADES)
        .map(|grade| player_panel::grade_name(&exe.image, exe.base, grade))
        .collect::<Option<Vec<_>>>()
        .ok_or("the executable does not hold the names of the grades")?;
    let spell_texts = (0..spells::SPELLS)
        .map(|spell| player_panel::spell_text(&file, player_menu::MENU_FILE_BASE, spell))
        .collect::<Option<Vec<_>>>()
        .ok_or("the menu's file does not hold the texts of the spells")?;
    let attack_texts = (0..attacks::ATTACKS)
        .map(|attack| player_panel::attack_text(&file, player_menu::MENU_FILE_BASE, attack))
        .collect::<Option<Vec<_>>>()
        .ok_or("the menu's file does not hold the texts of the body attacks")?;
    for (n, record) in creatures.iter().enumerate() {
        let known = player_panel::spells_known(record.spells);
        let spells: Vec<usize> = (0..known)
            .map(|place| player_panel::spell_at(record.spells, place))
            .collect();
        let moves: Vec<(bool, u8, u8)> = (record.moves.iter())
            .map(|entry| (entry.known, entry.attack, entry.grade))
            .collect();
        info!(
            "the player's menu: creature {n} knows the spells {spells:?}; its body attacks (known, attack, grade): {moves:?}"
        );
    }
    let dim_font = picture_in_row(&font_set, font.image, 1)?;
    let dim_small = picture_in_row(&font_set, small.image, 1)?;
    let dim_digits = player_panel::dim_digits(&exe.image, exe.base)
        .ok_or("the executable does not hold the colours of the digits")?;
    // The scroll sign of every window colour.
    let signs = options_view::scroll_signs(&exe, &font_set)?;
    let numbers = Detail {
        values: std::array::from_fn(|n| counted[n]),
        worn: carried.worn.map(|item| item != FREE),
        money: carried.keeping.money(),
    };
    let rank = player_panel::rank(player.level);
    let rank_name = player_panel::rank_name(&exe.image, exe.base, rank)
        .ok_or("the executable does not hold the names of the ranks")?;
    let said = |units: &[u16]| units.iter().map(|&unit| letter(unit)).collect::<String>();
    info!(
        "the player's menu: the detail: values {:?}, has on {:?} ({:?}, {:?}, {:?}), rank {rank} ({:?}), money {}",
        numbers.values,
        carried.worn,
        said(worn_name(0)),
        said(worn_name(1)),
        said(worn_name(2)),
        said(&rank_name),
        numbers.money
    );
    let menu = PlayerMenu::open(&tables, creatures.len() as i16, counts.waiting);
    info!(
        "the player's menu: {} sprites in file {}, the player {:?} with HP {} of {}, level {}, experience {:#x}; panels {:#x}",
        sprites.len(),
        name::SPRITES,
        name.iter().map(|&unit| letter(unit)).collect::<String>(),
        player.hp,
        player.max_hp,
        player.level,
        player.experience,
        menu.shown(&tables)
    );
    Ok(MenuScreen {
        menu,
        tables,
        player,
        name,
        counts,
        creatures,
        creature_names,
        families,
        waiting,
        waiting_names,
        kinds,
        list: KindList::default(),
        list_up: false,
        list_drawn: None,
        kind_names,
        mark,
        creature_values,
        carried,
        fight,
        draws,
        used: false,
        changed: false,
        entries,
        detail: numbers,
        worn_lines,
        rank: rank_name,
        equip: None,
        options: None,
        options_aim: Aim::default(),
        ended: false,
        options_look,
        settings: options_screen::Settings::at_boot(),
        equip_lines,
        gear,
        gear_texts,
        refusal,
        digit_colours,
        would: [0; 5],
        font,
        font_picture,
        small,
        small_picture,
        sprites,
        frames,
        bars,
        face,
        help,
        messages,
        item_lines,
        item_looks,
        descriptions,
        dim_font,
        dim_small,
        dim_digits,
        signs,
        story_lines,
        thing_looks,
        thing_texts,
        question,
        spell_lines,
        attack_lines,
        spell_pictures,
        attack_pictures,
        spell_names,
        attack_names,
        grade_names,
        spell_texts,
        attack_texts,
        forest: show.forest,
        waits: Vec::new(),
        handed: None,
        free: true,
        gain: (0, 0),
        circle: false,
        pad: None,
        passes: 0,
        owed: 0.0,
        time: 0.0,
        presses: show.presses.clone(),
        shown,
        aim: Aim::default(),
        equip_aim: Aim::default(),
        over_room: running.is_some(),
        way_out: None,
        leaves_forest: false,
    })
}

pub fn in_a_forest(room: u16, view: usize) -> Result<Stands, Failure> {
    let config = LocalConfig::load()?;
    let mut disc = DiscImage::open(&config.disc_image)?;
    let archive = Archive::open(&mut disc)?;
    let exe = Executable::read(&mut disc)?;
    let forests = Forests::from_executable(&exe.image, exe.base)?;
    let engine = archive.read(&mut disc, rooms::FIELD_ENGINE_FILE)?;
    let table = RoomTable::from_field_engine(&engine)?;
    let (at, record) = table
        .by_number(room)
        .ok_or("the room is not in the room table")?;
    let trees = leave_forest::trees(&archive.read(&mut disc, leave_forest::FILE)?);
    let from = leave_forest::From {
        area: at.area as i16,
        place: at.place as i16,
        slot: record.slot,
    };
    Ok(Stands {
        forest: forests.holds(at.area, at.place, record.slot, view as i16),
        way_out: trees.map(|trees| (trees, from)),
        free: true,
        waits: Vec::new(),
    })
}

pub fn over_the_room(
    carried: &Carried,
    name: &[u16],
    Stands {
        forest,
        way_out,
        free,
        waits,
    }: Stands,
    opens: &mut Opens,
    images: &mut Assets<Image>,
    commands: &mut Commands,
) -> Result<(), Failure> {
    let (wide, high) = (SCREEN.0 as u32, SCREEN.1 as u32);
    let black = [0, 0, 0, 255].repeat(SCREEN.0 * SCREEN.1);
    let shown = crate::spawn_picture(commands, images, wide, high, black);
    let layer = RenderLayers::layer(OVER_THE_ROOM);
    commands.entity(shown).insert((OverTheRoom, layer.clone()));
    let show = ShowMenu {
        presses: opens.next(),
        creatures: Vec::new(),
        items: Vec::new(),
        hp: None,
        things: Vec::new(),
        caught: Vec::new(),
        kinds: Vec::new(),
        gear: Vec::new(),
        forest,
    };
    let mut current = match open(&show, Some((carried, name)), shown) {
        Ok(current) => current,
        Err(err) => {
            commands.entity(shown).despawn();
            return Err(err);
        }
    };
    current.way_out = way_out;
    current.free = free;
    current.waits = waits;
    commands.spawn((
        OverTheRoom,
        Camera2d,
        Camera {
            order: 1,
            clear_color: ClearColorConfig::Custom(Color::BLACK),
            ..default()
        },
        // Two cameras of one window must agree on this, and the room's has none.
        Msaa::Off,
        layer,
    ));
    commands.insert_resource(current);
    Ok(())
}

/// `--menu`: the screen alone.
pub fn show_menu(
    show: Res<ShowMenu>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    commands.spawn(Camera2d);
    let (wide, high) = (SCREEN.0 as u32, SCREEN.1 as u32);
    let black = [0, 0, 0, 255].repeat(SCREEN.0 * SCREEN.1);
    let shown = crate::spawn_picture(&mut commands, &mut images, wide, high, black);
    match open(&show, None, shown) {
        Ok(current) => commands.insert_resource(current),
        Err(err) => {
            error!("the player's menu: {err}");
            exit.write(AppExit::error());
        }
    }
}

/// The pad's word of held buttons from the keyboard, or from the presses of a hidden run.
fn buttons(current: &MenuScreen, keys: &crate::keys::Pad) -> u16 {
    let mut held = crate::keys::held(keys);
    // The menu keys of the build back out of the menu as the triangle does.
    if keys.menu_key() {
        held |= jc_formats::pad::TRIANGLE;
    }
    for press in &current.presses {
        if press.holds(current.time) {
            held |= press.button;
        }
    }
    held
}

fn run_equip(current: &mut MenuScreen, equip: &mut EquipScreen, pad: &Pad, music: &mut Music) {
    let passes = current.passes;
    let drawn = equip.clone();
    let before = (equip.step, equip.kind, equip.cursor.at, equip.thing);
    let flags = (equip.describes_thing, equip.describes_worn, equip.message);
    let pass = equip.pass(&mut current.carried, pad);
    for &sound in &pass.sounds {
        music.sound(sound.into());
    }
    if let Some((thing, kind)) = pass.put_on {
        current.changed = true;
        current.dress();
        info!(
            "the player's menu: pass {passes}: thing {thing} of kind {kind} is put on: he has on {:?}, his values {:?}",
            current.carried.worn, current.detail.values
        );
    }
    // What the thing under the list's cursor would make of his numbers.
    let kind = usize::try_from(equip.kind).ok().filter(|kind| *kind < 3);
    let would = kind.filter(|_| equip.step == equip_screen::LIST && equip.thing != FREE);
    current.would = match would {
        Some(kind) => {
            let mut worn = current.carried.worn;
            worn[kind] = equip.thing;
            current.values_with(worn)
        }
        None => current.detail.values,
    };
    let after = (equip.step, equip.kind, equip.cursor.at, equip.thing);
    let now = (equip.describes_thing, equip.describes_worn, equip.message);
    if after != before || now != flags || !pass.sounds.is_empty() {
        let list = equip.list();
        let last = current.carried.keeping.last_place(list);
        let rows: Vec<(u8, u8)> = (0..=last)
            .map(|place| {
                let keeping = &current.carried.keeping;
                (
                    keeping.number_at(place, list),
                    keeping.count_at(place, list),
                )
            })
            .collect();
        info!(
            "the player's menu: pass {passes}: behind Equip: step {}, kind {}, the cursor on {} from line {}, thing {}, the list (thing, count) {rows:?}, values {:?} would be {:?}, descriptions {:?}, message {}, sounds {:?}",
            equip.step,
            equip.kind,
            equip.cursor.at,
            equip.cursor.first,
            equip.thing,
            current.detail.values,
            current.would,
            (equip.describes_thing, equip.describes_worn),
            equip.message,
            pass.sounds
        );
    }
    if pass.ended {
        *equip = drawn;
        current.ended = true;
        info!("the player's menu: pass {passes}: the screen behind Equip has ended");
    }
}

fn run_options(
    current: &mut MenuScreen,
    options: &mut OptionsScreen,
    settings: &mut options_screen::Settings,
    pad: &Pad,
    (music, pc, prefs): (&mut Music, &mut PcSettings, &mut ResMut<Prefs>),
) {
    let passes = current.passes;
    let who = "the player's menu";
    let drawn = options.clone();
    let screen = (&mut *options, &mut current.options_aim);
    // Only a pass of the PC Settings that changes a setting marks the settings changed.
    let mut preferred = (pc.is_open()).then(|| (**prefs).clone());
    let mut unused = Prefs::default();
    let kept = (music, pc, preferred.as_mut().unwrap_or(&mut unused));
    let pass = options_view::pass(who, passes, screen, settings, pad, kept);
    if let Some(preferred) = preferred.filter(|preferred| *preferred != **prefs) {
        **prefs = preferred;
    }
    if pass.ended {
        *options = drawn;
        current.ended = true;
        info!("the player's menu: pass {passes}: the options screen has ended");
    }
}

/// Runs the passes of the menu for the time gone by and paints the last one.
#[allow(clippy::too_many_arguments)]
pub fn run_menu(
    current: Option<ResMut<MenuScreen>>,
    mut music: ResMut<Music>,
    time: Res<Time<Virtual>>,
    keys: Res<crate::keys::Pad>,
    mut images: ResMut<Assets<Image>>,
    mut sprites: Query<&mut Sprite>,
    over_room: Query<Entity, With<OverTheRoom>>,
    mut things: Option<ResMut<crate::shown::Things>>,
    mut game: Option<ResMut<crate::Game>>,
    mut settings: ResMut<crate::settings::Settings>,
    mut commands: Commands,
    pointer: Pointer,
    (mut pc, mut prefs): (ResMut<PcSettings>, ResMut<Prefs>),
) {
    let Some(mut current) = current else {
        return;
    };
    let passed = crate::pace::passed(&time, LONGEST_PASS);
    current.time += passed;
    current.owed += passed * f64::from(player_menu::PASSES_A_SECOND);
    // The frames are drawn in the window colour of the settings.
    current.settings = settings.0;
    let held = buttons(&current, &keys);
    // The mouse over the spots of the menu's own screen, not over a list or a screen of
    // its own that stands in front of it.
    let under = pointer.on_picture(current.shown, SCREEN);
    let own = !current.list_up && current.equip.is_none() && current.options.is_none();
    let current = &mut *current;
    if own {
        let entries = [&current.entries[0][..], &current.entries[1][..]];
        let (items, story) = (&current.item_lines, &current.story_lines);
        let moves = [&current.spell_lines[..], &current.attack_lines[..]];
        let spots = player_panel::spots(&current.menu, entries, (items, story), moves);
        (current.aim).look(pointer.mouse(), pointer::row_under(spots, under));
    } else {
        current.aim.forget();
    }
    if let Some(options) = &current.options {
        let lines = current.options_look.lines();
        let aim = &mut current.options_aim;
        options_view::look((aim, &mut pc), pointer.mouse(), under, options, lines);
    }
    match &current.equip {
        Some(equip) => {
            let spots = equip_panel::spots(equip, &current.worn_lines, &current.equip_lines);
            (current.equip_aim).look(pointer.mouse(), pointer::row_under(spots, under));
        }
        None => current.equip_aim.forget(),
    }
    // The pad routine has run in every pass before the menu's first: a button that is
    // still down from the room did not go down on the screen.
    current.pad.get_or_insert_with(|| {
        let mut pad = Pad::default();
        pad.read(held, Repeat::of(GAME_FRAME), 0);
        pad
    });
    let mut painted = current.passes == 0;
    while current.owed >= 1.0 {
        current.owed -= 1.0;
        current.passes += 1;
        painted = true;
        let passes = current.passes;
        let mut pad = current.pad.unwrap_or_default();
        pad.read(held, Repeat::of(GAME_FRAME), passes);
        current.pad = Some(pad);
        let before = current.place();
        let current = &mut *current;
        current.list_drawn = None;
        if current.list_up {
            // The task's routine is the list's (`0x800BD9E8`): the menu's does not
            // run, and has the task again in the pass behind a cancel.
            let turned = current.list.pass(&pad);
            for &sound in &turned.sounds {
                music.sound(sound.into());
            }
            current.list_drawn = Some(!turned.ends);
            current.list_up = !turned.ends;
            if turned.ends || !turned.sounds.is_empty() {
                info!(
                    "the player's menu: pass {passes}: the list of the caught kinds, page {} of {}{}, sounds {:?}",
                    current.list.page + 1,
                    caught_list::PAGES,
                    if turned.ends {
                        ", goes back to the menu"
                    } else {
                        ""
                    },
                    turned.sounds
                );
            }
            continue;
        }
        if std::mem::take(&mut current.ended) {
            // The menu's task stands in front of the screen's in the list of field mode
            // 4, so its routine sees the end a game frame behind it (`0x800C017C`): it
            // goes back as a cancel does and draws once, and asks no button.
            (current.equip, current.options) = (None, None);
            current
                .menu
                .overlay_ended(&current.tables, &player_menu::Nothing);
            info!(
                "the player's menu: pass {passes}: the menu sees the end and is on screen {} again",
                current.menu.screen
            );
            continue;
        }
        if let Some(mut equip) = current.equip.take() {
            let here = equip_panel::here(&equip);
            (current.equip_aim).steer(&mut pad, here, |spot| equip_panel::way(&equip, spot));
            run_equip(current, &mut equip, &pad, &mut music);
            current.equip = current.menu.waits().then_some(equip);
            continue;
        }
        if let Some(mut options) = current.options.take() {
            // Only a pass that changes a setting marks the resource as changed.
            let mut set = settings.0;
            let pc = (&mut *music, &mut *pc, &mut prefs);
            run_options(current, &mut options, &mut set, &pad, pc);
            if set != settings.0 {
                settings.0 = set;
            }
            current.settings = set;
            current.options = current.menu.waits().then_some(options);
            continue;
        }
        // The mouse steers the cursor while no message is up.
        match current.menu.message {
            true => current.aim.forget(),
            false => {
                let menu = &current.menu;
                current
                    .aim
                    .steer(&mut pad, menu.spot(), |spot| menu.way_to(spot));
            }
        }
        let mut held = Held {
            keeping: &mut current.carried.keeping,
            fight: &current.fight,
            draws: &mut current.draws,
            player: &mut current.player,
            creatures: &mut current.creatures,
            forest: current.forest,
            waits: &current.waits,
            free: current.free,
            count: &mut current.carried.item_count,
        };
        let pass = current.menu.pass(&current.tables, &mut held, &pad);
        for &sound in &pass.sounds {
            music.sound(sound.into());
        }
        if pass.lists {
            current.list_up = true;
            info!(
                "the player's menu: pass {passes}: the square: the list of the caught kinds comes up on page {}, {} of {} kinds caught",
                current.list.page + 1,
                current.kinds.count(),
                caught_list::CELLS
            );
        }
        if let Some(used) = pass.used {
            current.used = true;
            // A creature's values of the "Detail" follow what an item raised.
            for (values, record) in current.creature_values.iter_mut().zip(&current.creatures) {
                let counted = record.effective(&current.fight).stats;
                *values = std::array::from_fn(|n| counted[n]);
            }
            let points = |record: &Record| (record.hp, record.max_hp, record.mp, record.max_mp);
            info!(
                "the player's menu: pass {passes}: item {} is used on {:?}: the player's HP and MP {:?}, the creatures' {:?}, {} of it left",
                used.item,
                used.target,
                points(&current.player),
                current.creatures.iter().map(points).collect::<Vec<_>>(),
                current.carried.keeping.count(used.item.into(), Kind::Item)
            );
        }
        current.circle = pad.held & jc_formats::pad::CIRCLE != 0;
        let gain_before = std::mem::take(&mut current.gain);
        if current.menu.screen == player_menu::TARGET && current.menu.said == 0 {
            let place = Held::place(current.menu.item_place());
            let item = current.carried.keeping.number_at(place, Kind::Item);
            let target = Held::place(current.menu.target());
            let record = match target {
                0 => Some(&current.player),
                n => current.creatures.get(n - 1),
            };
            let pieces = current.carried.keeping.count_at(place, Kind::Item);
            if let Some(record) = record.filter(|_| pieces != 0) {
                let flags =
                    (current.fight.items.get(usize::from(item))).map_or(0, |item| item.flags);
                current.gain = using::would_give(
                    &current.fight,
                    item.into(),
                    flags,
                    target,
                    &current.player,
                    record,
                    &mut current.draws,
                );
            }
        }
        if current.gain != gain_before {
            info!(
                "the player's menu: pass {passes}: a use would give target {} HP and MP {:?}",
                current.menu.target(),
                current.gain
            );
        }
        let (screen, flags, cursor, message) = current.place();
        if screen == player_menu::ITEMS && before.0 != screen {
            let last = current.carried.keeping.last_place(Kind::Item);
            let list = (0..=last).map(|place| {
                let number = current.carried.keeping.number_at(place, Kind::Item);
                (number, current.carried.keeping.count_at(place, Kind::Item))
            });
            info!(
                "the player's menu: pass {passes}: the list of the items (item, count): {:?}, {} lines",
                list.collect::<Vec<_>>(),
                current.menu.cursor.lines
            );
        }
        if current.place() != before || !pass.sounds.is_empty() {
            info!(
                "the player's menu: pass {passes}: screen {screen}, flags {flags:#x}, the cursor on {cursor}, panels {:#x}{}, sounds {:?}",
                current.menu.shown(&current.tables),
                if message { ", a message is up" } else { "" },
                pass.sounds
            );
        }
        if screen == player_menu::STORY && before.0 != screen {
            let held = current.carried.keeping.story_things();
            let list = (0..held).map(|n| current.carried.keeping.story_thing_at(n));
            info!(
                "the player's menu: pass {passes}: the things of the story: {:?}, {} lines",
                list.collect::<Vec<_>>(),
                current.menu.cursor.lines
            );
        }
        if let Some(thing) = pass.handed {
            // It goes to the room when the menu is gone (`0x800A91FC`).
            current.handed = Some(thing);
            info!("the player's menu: pass {passes}: thing {thing} of the story is handed over");
        }
        if pass.leaves_forest {
            current.leaves_forest = true;
            info!("the player's menu: pass {passes}: yes to leaving the forest");
        }
        if message && !before.3 && current.menu.result == player_menu::OWN_TEXT {
            let thing = current.thing_at(current.menu.thing_place());
            let lines = usize::try_from(thing)
                .ok()
                .and_then(|n| current.thing_texts.get(n));
            info!(
                "the player's menu: pass {passes}: the text of thing {thing}: {:?}",
                lines.map(|lines| lines
                    .each_ref()
                    .map(|units| { units.iter().map(|&unit| letter(unit)).collect::<String>() }))
            );
        } else if message && !before.3 {
            let said = current.messages.get(&current.menu.result);
            info!(
                "the player's menu: pass {passes}: the message of result {}: {:?}",
                current.menu.result,
                said.map(|units| units.iter().map(|&unit| letter(unit)).collect::<String>())
            );
        }
        if pass.starts == Some(player_menu::EQUIP) {
            info!("the player's menu: pass {passes}: the screen behind Equip starts");
            let mut equip = EquipScreen::open();
            run_equip(current, &mut equip, &pad, &mut music);
            current.equip = Some(equip);
        }
        if pass.starts == Some(player_menu::OPTIONS) {
            // The start of the screen's task (`0x800C309C`).
            // As for the screen behind "Equip": its first pass is in this game frame.
            info!("the player's menu: pass {passes}: the options screen starts");
            let mut options = OptionsScreen::open(&settings.0);
            let mut set = settings.0;
            let pc = (&mut *music, &mut *pc, &mut prefs);
            run_options(current, &mut options, &mut set, &pad, pc);
            if set != settings.0 {
                settings.0 = set;
            }
            current.settings = set;
            current.options = Some(options);
        }
        if let Some(screen) = pass.not_built {
            info!(
                "the player's menu: pass {passes}: the confirm leads to screen {screen}, which is not built"
            );
        }
        // A menu that leaves in the pass of the square shows the list first: the
        // leaving starts once the list has given the task back.
        if current.list_up {
            continue;
        }
        if current.menu.leaves() && current.over_room {
            info!("the player's menu: pass {passes}: the menu leaves, the room runs on");
            if current.used || current.changed {
                // What the items did and what he has put on is the game's: the room's
                // scripts work on their own copy of it, a room without scripts on the
                // game's. His record is the game's only once an item has changed it.
                let keeping = current.carried.keeping.clone();
                let player = current.used.then(|| current.player.clone());
                let worn = current.carried.worn;
                // The creatures he carries are the game's too, as an item left them.
                let creatures = current.used.then(|| current.creatures.clone());
                let scripts = things.as_mut().filter(|things| things.carried().is_some());
                let count = current.carried.item_count;
                if let Some(things) = scripts {
                    things.kept(keeping, player, worn, creatures);
                    things.count_kept(count);
                } else if let Some(game) = game.as_mut() {
                    game.0.item_count = count;
                    game.0.keeping = keeping;
                    game.0.worn = worn;
                    if player.is_some() {
                        game.0.player = player;
                    }
                    let carried = game.0.creatures.carried_mut();
                    for (record, left) in carried.zip(creatures.unwrap_or_default()) {
                        *record = left;
                    }
                    // A room without scripts shows what he has on all the same.
                    if let Some(things) = things.as_mut() {
                        things.wear(worn);
                    }
                }
            }
            if current.leaves_forest {
                let started =
                    (current.way_out.zip(things.as_mut())).and_then(|((trees, from), things)| {
                        things.leave_forest(trees, from).then_some(from)
                    });
                match started {
                    Some(from) => info!(
                        "the player's menu: pass {passes}: the task that leaves the forest \
                         starts in the room, which lies at {from:?}"
                    ),
                    None => warn!(
                        "the player's menu: pass {passes}: the room has no scripts or no \
                         way out is known, the player stays in the forest"
                    ),
                }
            }
            // A thing of the story the menu used goes to the room's objects (`0x800A91FC`
            // calls `0x800A5BD0` with it).
            if let Some(thing) = current.handed {
                match things.as_mut() {
                    Some(things) => {
                        things.use_story_thing(thing);
                        info!(
                            "the player's menu: pass {passes}: thing {thing} of the story is \
                             used in the room"
                        );
                    }
                    None => warn!(
                        "the player's menu: pass {passes}: thing {thing} of the story is used \
                         where no room runs"
                    ),
                }
            }
            for entity in &over_room {
                commands.entity(entity).despawn();
            }
            commands.remove_resource::<MenuScreen>();
            return;
        }
        if current.menu.leaves() {
            // Alone, the view has nowhere to go: the menu's task starts again.
            info!("the player's menu: pass {passes}: the menu leaves, and comes up again");
            let carried = current.creatures.len() as i16;
            current.menu = PlayerMenu::open(&current.tables, carried, current.counts.waiting);
        }
    }
    if !painted {
        return;
    }
    if let Ok(mut sprite) = sprites.get_mut(current.shown) {
        let (wide, high) = (SCREEN.0 as u32, SCREEN.1 as u32);
        let painted = current.paint(&pc, &prefs);
        sprite.image = images.add(crate::picture_image(wide, high, painted));
    }
}
