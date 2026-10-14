
use std::fmt;

use crate::chapters::{VariantKey, VariantTable};
use crate::menu::{Cursor, Rect, cursor, piece};
use crate::rooms::{Image, RoomTableError, step};
use crate::text;

/// Archive file that holds the place map.
pub const MAP_FILE: usize = 41;
/// RAM address of the first byte of [`MAP_FILE`].
pub const MAP_FILE_BASE: u32 = 0x800B_6890;
/// Maps in the file: two of the village, four of the forest.
pub const MAP_COUNT: usize = 6;
/// The entry of a village map that is the forest gate's line.
pub const GATE_ENTRY: usize = 12;
/// Rows of the list on screen.
pub const ROWS: usize = 3;
/// The value of the state word at +0x2E0 that stands for the village of the second map.
pub const SECOND_VILLAGE: i16 = 1;

/// Passes of the screen in a second: game frames of the field engine.
pub const PASSES_A_SECOND: u32 = 25;
/// The flag word of the list's menu: the frame of the cursor's line pulses.
pub const LIST_FLAGS: u32 = 2;
/// The flag word of a row's piece (its text lies in the middle of its room) and the
/// margin of the room.
pub const ROW_FLAGS: u16 = piece::CENTRED;
pub const ROW_MARGIN: (i16, i16) = (2, 2);
/// How high the frame around the title is, and how far it reaches over the text.
const TITLE_FRAME: (i16, i16) = (18, 2);
/// The first row of the list, and how far apart the rows are.
const FIRST_ROW: Rect = Rect {
    x: 16,
    y: 152,
    w: 130,
    h: 18,
};
/// Where the middle of the scroll sign above the list is, and of the one below a list
/// without rows.
const SIGN_ABOVE: (i16, i16) = (81, 147);
const SIGN_BELOW: (i16, i16) = (81, 157);

/// Where the title starts, whose middle is at the x of `at`, and the frame around it.
pub fn title(at: (i16, i16), width: i16) -> ((i16, i16), Rect) {
    let x = at.0.wrapping_sub(width / 2);
    let (high, over) = TITLE_FRAME;
    let frame = Rect {
        x: x.wrapping_sub(over),
        y: at.1.wrapping_sub(over),
        w: width.wrapping_add(2 * over),
        h: high,
    };
    ((x, at.1), frame)
}

/// The rectangle of a row of the list.
pub fn row(row: i16) -> Rect {
    Rect {
        y: FIRST_ROW.y + FIRST_ROW.h * row,
        ..FIRST_ROW
    }
}

pub const SPRITE_BLEND_MODE: u8 = 1;

/// How the marker of a map is drawn: solid on the two village maps, blended on the forest
/// maps (`None` is solid, else the blend mode). Every coloured texel of a forest map's
/// marker carries the blend mark, so the whole marker is blended.
pub fn marker_blend(map: usize) -> Option<u8> {
    (map >= FIRST_FOREST_MAP).then_some(SPRITE_BLEND_MODE)
}

/// The middles of the scroll signs above and below a list of so many rows.
pub fn signs(rows: i16) -> [(i16, i16); 2] {
    let below = (SIGN_BELOW.0, SIGN_BELOW.1 + FIRST_ROW.h * rows);
    [SIGN_ABOVE, below]
}

/// Where the middle of the place's name is on the screen of the question.
pub const QUESTION_NAME: (i16, i16) = (160, 20);
/// Where the question's text starts, and the frame around it.
pub const QUESTION_AT: (i16, i16) = (134, 170);
pub const QUESTION_FRAME: Rect = Rect {
    x: 116,
    y: 166,
    w: 88,
    h: 18,
};
/// The cursor record of the question as the file holds it: two answers side by side,
/// and a step over a side leads to the other one.
pub const QUESTION_CURSOR: Cursor = Cursor {
    columns: 2,
    rows: 1,
    lines: 2,
    flags: cursor::WRAPS_ACROSS,
    at: 0,
    first: 0,
};
/// The answer that goes to the place; the other one brings the map back.
pub const ANSWER_YES: i16 = 0;
/// The text of the question, and the two templates of its answers: each a piece of a
/// menu's line whose value is the address of the answer's text.
const QUESTION_TEXT: u32 = 0x800B_6B48;
const ANSWERS: [u32; 2] = [0x800B_6B64, 0x800B_6BA4];

/// The rectangle of an answer of the question.
pub fn answer(answer: usize) -> Rect {
    Rect {
        x: [114, 160][answer & 1],
        y: 184,
        w: 46,
        h: 18,
    }
}

/// The texts of the question behind a confirm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    /// The units of the question.
    pub text: Vec<u16>,
    /// The units of the answers, the one that goes first.
    pub answers: [Vec<u16>; 2],
}

impl Question {
    /// Reads the texts from the place map's file as it is on the disc.
    pub fn from_file(file: &[u8]) -> Result<Self, MapError> {
        let image = Image {
            data: file,
            base: MAP_FILE_BASE,
        };
        let units = |address: u32| -> Result<Vec<u16>, MapError> {
            let from = address.checked_sub(MAP_FILE_BASE);
            let bytes = from.and_then(|from| file.get(from as usize..));
            Ok(text::units(bytes.ok_or(MapError::OutOfImage(address))?))
        };
        let [yes, no] = ANSWERS;
        Ok(Self {
            text: units(QUESTION_TEXT)?,
            answers: [units(image.u32(yes + 4)?)?, units(image.u32(no + 4)?)?],
        })
    }
}

/// Bytes of one map record.
const MAP_SIZE: usize = 24;
/// Bytes of one entry record.
const ENTRY_SIZE: usize = 28;
/// The first forest map.
const FIRST_FOREST_MAP: usize = 2;
/// The area of the forest gate and the forests, and the place of the gate in it.
const FOREST_AREA: i16 = 4;
const GATE_PLACE: i16 = 0;
/// Forest places: 1 to this.
const FORESTS: i16 = 4;

/// Where the tables of the place map lie in an image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    /// The map records.
    pub maps: u32,
    /// Entries per map, 16 bits each.
    pub counts: u32,
    /// Per map the address of a list of name addresses.
    pub names: u32,
    /// Per map the address of the destination bytes.
    pub destinations: u32,
    /// Per map one byte: the area.
    pub areas: u32,
    /// Per map one byte: how many destination bytes are looked through for a place.
    pub searched: u32,
    /// How many maps there are.
    pub count: usize,
}

impl Layout {
    /// The tables of the file as it is on the disc.
    pub const DISC: Self = Self {
        maps: 0x800B_93D0,
        counts: 0x800B_8E48,
        names: 0x800B_94AC,
        destinations: 0x800B_94F8,
        areas: 0x800B_9510,
        searched: 0x800B_9518,
        count: MAP_COUNT,
    };
}

/// Why the place map could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapError {
    /// A pointer or a list leads outside the image. Holds the RAM address that was wanted.
    OutOfImage(u32),
}

impl fmt::Display for MapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfImage(address) => {
                write!(f, "place map leads outside the image, to {address:#010x}")
            }
        }
    }
}

impl std::error::Error for MapError {}

impl From<RoomTableError> for MapError {
    fn from(err: RoomTableError) -> Self {
        match err {
            RoomTableError::OutOfImage(address) => Self::OutOfImage(address),
        }
    }
}

/// Where a line of a map leads, from its destination byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Destination {
    /// A place of the map's area.
    Place(u8),
    /// A forest, 1 to 4: the gate, with the forest noted in story variable 4.
    Forest(u8),
    /// The forest gate, always open (`0xFE`).
    Gate,
    /// The forest gate, open once story flag 1 is set (`0xFF`).
    FlaggedGate,
}

impl Destination {
    /// The destination a byte of the lists stands for.
    pub fn of(byte: u8) -> Self {
        match byte {
            0xFF => Self::FlaggedGate,
            0xFE => Self::Gate,
            0xF0..=0xFD => Self::Forest(byte - 0xEF),
            place => Self::Place(place),
        }
    }
}

/// One entry of a map: a place the list can show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The destination byte.
    pub byte: u8,
    /// The units of the place's name.
    pub name: Vec<u16>,
    /// Number of the small picture in the map's second file, and where it is drawn.
    pub picture: u32,
    pub picture_at: (i16, i16),
    /// Number of the marker in the map's second file, and where it is drawn.
    pub marker: u32,
    pub marker_at: (i16, i16),
    /// The two colours the marker goes between.
    pub colours: [u32; 2],
    /// Archive file of the place's full-screen picture.
    pub large_file: u32,
}

impl Entry {
    pub fn destination(&self) -> Destination {
        Destination::of(self.byte)
    }
}

/// One map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Map {
    /// The area its places are in.
    pub area: u8,
    /// Archive file of the map's picture.
    pub background_file: u32,
    /// Archive file of the small pictures and markers.
    pub pictures_file: u32,
    /// The units of the title, the x of its middle and its y.
    pub title: Vec<u16>,
    pub title_at: (i16, i16),
    /// Number of the map's music.
    pub music: u32,
    pub entries: Vec<Entry>,
    /// The destination bytes that are looked through for the place the player is at. They
    /// can be more than the map has entries.
    pub searched: Vec<u8>,
}

/// A map and an entry of it: the two words the game keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chosen {
    pub map: usize,
    pub entry: usize,
}

/// What a confirm on a line of the list leads to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirmed {
    /// Another map comes up, with this entry as the one the player is at.
    Map(Chosen),
    /// The place is asked about, or gone to at once with the square.
    Place(Chosen),
}

/// What choosing an entry writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Choice {
    /// The wanted area and place; the room is slot 0.
    pub area: i16,
    pub place: i16,
    /// The value for story variable 4, when the entry is a forest.
    pub forest: Option<u8>,
}

/// What is known of the game's state where the map asks.
#[derive(Debug, Clone, Copy)]
pub struct State<'a> {
    pub variants: &'a VariantTable,
    /// The state word at +0x2E0.
    pub word: i16,
    /// Story flag 1.
    pub gate_flag: bool,
}

/// The maps of the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Maps {
    pub maps: Vec<Map>,
}

impl Maps {
    /// Reads the maps from the place map's file as it is on the disc.
    pub fn from_file(file: &[u8]) -> Result<Self, MapError> {
        Self::parse(file, MAP_FILE_BASE, &Layout::DISC)
    }

    /// Reads the maps from an image whose first byte is at RAM address `base`.
    pub fn parse(data: &[u8], base: u32, layout: &Layout) -> Result<Self, MapError> {
        let image = Image { data, base };
        let name = |address: u32| -> Result<Vec<u16>, MapError> {
            let from = address.checked_sub(base).map(|offset| offset as usize);
            let bytes = from.and_then(|from| data.get(from..));
            Ok(text::units(bytes.ok_or(MapError::OutOfImage(address))?))
        };
        let mut maps = Vec::with_capacity(layout.count);
        for map in 0..layout.count {
            let at = step(layout.maps, map, MAP_SIZE)?;
            let count = image.i16(step(layout.counts, map, 2)?)?.max(0) as usize;
            let names = image.u32(step(layout.names, map, 4)?)?;
            let bytes = image.u32(step(layout.destinations, map, 4)?)?;
            let records = image.u32(at + 20)?;
            let mut entries = Vec::with_capacity(count);
            for entry in 0..count {
                let record = step(records, entry, ENTRY_SIZE)?;
                entries.push(Entry {
                    byte: image.u8(step(bytes, entry, 1)?)?,
                    name: name(image.u32(step(names, entry, 4)?)?)?,
                    picture: image.u32(record)?,
                    picture_at: (image.i16(record + 4)?, image.i16(record + 6)?),
                    marker: image.u32(record + 8)?,
                    marker_at: (image.i16(record + 12)?, image.i16(record + 14)?),
                    colours: [image.u32(record + 16)?, image.u32(record + 20)?],
                    large_file: image.u32(record + 24)?,
                });
            }
            let looked = image.u8(step(layout.searched, map, 1)?)? as usize;
            let mut searched = Vec::with_capacity(looked);
            for index in 0..looked {
                searched.push(image.u8(step(bytes, index, 1)?)?);
            }
            maps.push(Map {
                area: image.u8(step(layout.areas, map, 1)?)?,
                background_file: image.u32(at)?,
                pictures_file: image.u32(at + 4)?,
                title: name(image.u32(at + 8)?)?,
                title_at: (image.i16(at + 12)?, image.i16(at + 14)?),
                music: image.u32(at + 16)?,
                entries,
                searched,
            });
        }
        Ok(Self { maps })
    }

    pub fn get(&self, map: usize) -> Option<&Map> {
        self.maps.get(map)
    }

    fn entry(&self, chosen: Chosen) -> Option<(&Map, &Entry)> {
        let map = self.maps.get(chosen.map)?;
        Some((map, map.entries.get(chosen.entry)?))
    }

    /// The map and entry the screen comes up with for the wanted area and place.
    pub fn of_location(&self, area: i16, place: i16, state: &State) -> Chosen {
        let village = usize::from(state.word == SECOND_VILLAGE);
        let (map, searches) = match area {
            4 | 5 if place == GATE_PLACE => {
                let entry = GATE_ENTRY;
                return Chosen {
                    map: village,
                    entry,
                };
            }
            4 | 5 => {
                let slot = 0;
                let closed = (1..=FORESTS).find(|&place| {
                    state
                        .variants
                        .get(VariantKey { area, place, slot })
                        .is_none()
                });
                (closed.unwrap_or(FORESTS) as usize + 1, true)
            }
            1..=3 => (village, true),
            _ => (village, false),
        };
        let found = self
            .maps
            .get(map)
            .filter(|_| searches)
            .and_then(|map| (map.searched.iter()).position(|&byte| i16::from(byte) == place));
        let entry = found.unwrap_or(0);
        Chosen { map, entry }
    }

    /// Whether an entry is open: whether the list shows it.
    pub fn is_open(&self, chosen: Chosen, state: &State) -> bool {
        let Some((map, entry)) = self.entry(chosen) else {
            return false;
        };
        let place = match entry.destination() {
            Destination::FlaggedGate => return state.gate_flag,
            Destination::Gate => return true,
            Destination::Forest(forest) => forest,
            Destination::Place(place) => place,
        };
        state
            .variants
            .is_open(i16::from(map.area), i16::from(place))
    }

    /// The open entries of a map in the order of its records: the lines of the list.
    pub fn lines(&self, map: usize, state: &State) -> Vec<usize> {
        let count = self.maps.get(map).map_or(0, |map| map.entries.len());
        (0..count)
            .filter(|&entry| self.is_open(Chosen { map, entry }, state))
            .collect()
    }

    /// The entry of a line of the list. A line behind the last one gives the map's last
    /// entry, as the game's walk through the records leaves it.
    pub fn entry_of_line(&self, map: usize, line: usize, state: &State) -> usize {
        let last = self.maps.get(map).map_or(0, |map| map.entries.len());
        let lines = self.lines(map, state);
        lines.get(line).copied().unwrap_or(last.saturating_sub(1))
    }

    /// How many open entries there are up to and with an entry: one more than its line.
    pub fn lines_to(&self, chosen: Chosen, state: &State) -> usize {
        let lines = self.lines(chosen.map, state);
        lines
            .iter()
            .take_while(|&&entry| entry <= chosen.entry)
            .count()
    }

    /// The cursor record of a map as the screen sets it up: `kept` is the record as the
    /// last visit left it, `last` the map and entry of the last visit.
    pub fn cursor(&self, now: Chosen, last: Option<Chosen>, kept: Cursor, state: &State) -> Cursor {
        let count = self.lines(now.map, state).len();
        let mut cursor = Cursor {
            rows: count.min(ROWS) as i16,
            lines: count as u16,
            flags: if count > ROWS { cursor::SCROLLS } else { 0 },
            ..kept
        };
        let line = (i32::from(kept.at) + i32::from(kept.first)).max(0) as usize;
        let entry = self.entry_of_line(now.map, line, state);
        let map = now.map;
        if last != Some(now) || !self.is_open(Chosen { map, entry }, state) {
            cursor.at = 0;
            cursor.first = 0;
            return cursor;
        }
        let to = self.lines_to(now, state) as i16;
        if count <= ROWS {
            cursor.at = to;
            cursor.first = 0;
        } else {
            let spare = (count - ROWS) as i16;
            if spare < cursor.first {
                cursor.at = to - spare;
                cursor.first = spare;
            }
        }
        cursor
    }

    /// What a confirm on a line of a map leads to.
    pub fn confirm(&self, map: usize, line: usize, state: &State) -> Confirmed {
        let entry = self.entry_of_line(map, line, state);
        if map < FIRST_FOREST_MAP {
            if entry != GATE_ENTRY {
                return Confirmed::Place(Chosen { map, entry });
            }
            // The largest forest map whose last forest is open.
            let forest = (FIRST_FOREST_MAP..self.maps.len())
                .rev()
                .find(|&map| self.is_open(Chosen { map, entry: map }, state));
            return match forest {
                Some(map) => Confirmed::Map(Chosen { map, entry: 0 }),
                None => Confirmed::Place(Chosen { map: 0, entry }),
            };
        }
        if entry == 0 {
            let map = usize::from(state.word != 0);
            let entry = GATE_ENTRY;
            return Confirmed::Map(Chosen { map, entry });
        }
        Confirmed::Place(Chosen { map, entry })
    }

    /// What choosing an entry writes.
    pub fn choice(&self, chosen: Chosen) -> Option<Choice> {
        let (map, entry) = self.entry(chosen)?;
        let gate = |forest| Choice {
            area: FOREST_AREA,
            place: GATE_PLACE,
            forest,
        };
        Some(match entry.destination() {
            Destination::Gate | Destination::FlaggedGate => gate(None),
            Destination::Forest(forest) => gate(Some(forest)),
            Destination::Place(place) => Choice {
                area: i16::from(map.area),
                place: i16::from(place),
                forest: None,
            },
        })
    }
}

pub fn music_asked(music: u32, variable: i32) -> u32 {
    match variable {
        1 => 15,
        2 => 145,
        _ => music,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_music_of_a_map_follows_story_variable_12() {
        assert_eq!(super::music_asked(13, 0), 13);
        assert_eq!(super::music_asked(14, 3), 14);
        assert_eq!(super::music_asked(13, 1), 15);
        assert_eq!(super::music_asked(14, 2), 145);
    }

    use super::*;

    fn entry(byte: u8) -> Entry {
        Entry {
            byte,
            name: vec![0x8260 + u16::from(byte & 15)],
            picture: 0,
            picture_at: (12, 18),
            marker: 1,
            marker_at: (100, 100),
            colours: [0x404020, 0x808040],
            large_file: 62,
        }
    }

    fn map(area: u8, bytes: &[u8], searched: &[u8]) -> Map {
        Map {
            area,
            background_file: 60,
            pictures_file: 61,
            title: vec![0x8272],
            title_at: (160, 20),
            music: 14,
            entries: bytes.iter().map(|&byte| entry(byte)).collect(),
            searched: searched.to_vec(),
        }
    }

    #[test]
    fn the_screen_is_laid_out_as_the_map_draws_it() {
        // A title of 61 pixels around x = 160: half its width is rounded down.
        let (at, frame) = title((160, 20), 61);
        assert_eq!(at, (130, 20));
        let (x, y, w, h) = (128, 18, 65, 18);
        assert_eq!(frame, Rect { x, y, w, h });
        assert_eq!((row(0).y, row(2).y), (152, 188));
        assert_eq!((row(2).x, row(2).w, row(2).h), (16, 130, 18));
        assert_eq!(signs(3), [(81, 147), (81, 211)]);
        // The marker is solid on the village maps and added to the picture on the others.
        assert_eq!(
            [0, 1, 2, 5].map(marker_blend),
            [None, None, Some(1), Some(1)]
        );
    }

    /// Two village maps and four forest maps, shaped as the disc's.
    fn maps() -> Maps {
        let village = [0, 1, 2, 3, 6, 4, 5, 9, 10, 11, 7, 8, 0xFF];
        let forest = [0xFE, 0, 0xF0, 0xF1, 0xF2, 0xF3];
        let mut maps = vec![map(1, &village, &village), map(2, &village, &village)];
        maps.extend((3..=6).map(|count| map(4, &forest[..count], &forest)));
        Maps { maps }
    }

    fn open(places: &[(i16, i16)]) -> VariantTable {
        let mut variants = VariantTable::new();
        for &(area, place) in places {
            let slot = 0;
            variants.set(VariantKey { area, place, slot }, 100);
        }
        variants
    }

    fn state(variants: &VariantTable, word: i16, gate_flag: bool) -> State<'_> {
        State {
            variants,
            word,
            gate_flag,
        }
    }

    const KEPT: Cursor = Cursor {
        columns: 1,
        rows: 3,
        lines: 0,
        flags: 0x40,
        at: 0,
        first: 0,
    };

    #[test]
    fn a_destination_byte_is_a_place_a_forest_or_the_gate() {
        assert_eq!(Destination::of(5), Destination::Place(5));
        assert_eq!(Destination::of(0xF0), Destination::Forest(1));
        assert_eq!(Destination::of(0xF3), Destination::Forest(4));
        assert_eq!(Destination::of(0xFE), Destination::Gate);
        assert_eq!(Destination::of(0xFF), Destination::FlaggedGate);
    }

    #[test]
    fn the_location_and_the_state_word_pick_map_and_entry() {
        let maps = maps();
        let variants = open(&[(4, 1)]);
        let first = state(&variants, 0, false);
        let second = state(&variants, 1, false);
        let chosen = |map, entry| Chosen { map, entry };
        // A village place: the map of the state word, the entry whose byte is the place.
        assert_eq!(maps.of_location(2, 5, &second), chosen(1, 6));
        assert_eq!(maps.of_location(1, 6, &first), chosen(0, 4));
        // A place no byte names is entry 0.
        assert_eq!(maps.of_location(1, 12, &first), chosen(0, 0));
        // The gate is the gate's line of the village map.
        assert_eq!(maps.of_location(4, 0, &first), chosen(0, GATE_ENTRY));
        assert_eq!(maps.of_location(5, 0, &second), chosen(1, GATE_ENTRY));
        // A forest: one more than the first closed forest, and no byte is a plain forest.
        assert_eq!(maps.of_location(4, 1, &first), chosen(3, 0));
        let all = open(&[(4, 1), (4, 2), (4, 3), (4, 4)]);
        assert_eq!(maps.of_location(4, 3, &state(&all, 0, false)), chosen(5, 0));
        // Anything else: the village map, entry 0.
        assert_eq!(maps.of_location(0, 4, &second), chosen(1, 0));
    }

    #[test]
    fn the_lines_are_the_open_entries_in_record_order() {
        let maps = maps();
        let variants = open(&[(2, 0), (2, 4), (2, 5), (2, 9), (4, 0), (4, 2)]);
        let closed = state(&variants, 1, false);
        assert_eq!(maps.lines(1, &closed), [0, 5, 6, 7]);
        assert_eq!(maps.lines(0, &closed), [] as [usize; 0]);
        // Story flag 1 opens the gate's line of both village maps.
        let flagged = state(&variants, 1, true);
        assert_eq!(maps.lines(1, &flagged), [0, 5, 6, 7, GATE_ENTRY]);
        assert_eq!(maps.lines(0, &flagged), [GATE_ENTRY]);
        // A forest map: the way back always, the gate by place 0, a forest by its place.
        assert_eq!(maps.lines(3, &closed), [0, 1, 3]);
        assert_eq!(maps.entry_of_line(1, 2, &closed), 6);
        // A line behind the last one is the last entry of the map.
        assert_eq!(maps.entry_of_line(1, 9, &closed), GATE_ENTRY);
        let at = |map, entry| Chosen { map, entry };
        assert_eq!(maps.lines_to(at(1, 6), &closed), 3);
        assert_eq!(maps.lines_to(at(1, 0), &closed), 1);
        assert!(!maps.is_open(at(9, 0), &closed));
    }

    #[test]
    fn the_cursor_starts_on_the_first_line_of_another_map_or_entry() {
        let maps = maps();
        let variants = open(&[(2, 0), (2, 4), (2, 5), (2, 9), (2, 10)]);
        let state = state(&variants, 1, false);
        let now = Chosen { map: 1, entry: 6 };
        let moved = Cursor {
            at: 2,
            first: 1,
            ..KEPT
        };
        let fresh = maps.cursor(now, None, moved, &state);
        assert_eq!((fresh.rows, fresh.lines, fresh.flags), (3, 5, 0x40));
        assert_eq!((fresh.columns, fresh.at, fresh.first), (1, 0, 0));
        // The same map and entry as last time: the record is kept.
        let same = maps.cursor(now, Some(now), moved, &state);
        assert_eq!((same.at, same.first), (2, 1));
        // A first line behind the last one that fills the rows is pulled back.
        let far = Cursor {
            at: 0,
            first: 4,
            ..KEPT
        };
        let pulled = maps.cursor(now, Some(now), far, &state);
        assert_eq!((pulled.at, pulled.first), (1, 2));
    }

    #[test]
    fn a_short_list_puts_the_cursor_behind_the_entry() {
        let maps = maps();
        let variants = open(&[(4, 0), (4, 1)]);
        let state = state(&variants, 0, false);
        let now = Chosen { map: 2, entry: 1 };
        let kept = Cursor { at: 1, ..KEPT };
        let cursor = maps.cursor(now, Some(now), kept, &state);
        assert_eq!((cursor.rows, cursor.lines, cursor.flags), (3, 3, 0));
        // One more than the entry's own line, as the game counts it.
        assert_eq!((cursor.at, cursor.first), (2, 0));
    }

    #[test]
    fn a_confirm_leads_to_a_place_or_to_another_map() {
        let maps = maps();
        let chosen = |map, entry| Chosen { map, entry };
        // No forest is open: the gate's line is a place, and the map number is put to 0.
        let variants = open(&[(1, 0), (1, 1), (4, 0)]);
        let early = state(&variants, 0, true);
        assert_eq!(maps.confirm(0, 1, &early), Confirmed::Place(chosen(0, 1)));
        assert_eq!(
            maps.confirm(0, 2, &early),
            Confirmed::Place(chosen(0, GATE_ENTRY))
        );
        // The first forest is open: the gate's line brings up the first forest map.
        let variants = open(&[(1, 0), (1, 1), (4, 0), (4, 1)]);
        let later = state(&variants, 0, true);
        assert_eq!(maps.confirm(0, 2, &later), Confirmed::Map(chosen(2, 0)));
        // On a forest map the first line leads back to the village of the state word.
        assert_eq!(
            maps.confirm(2, 0, &later),
            Confirmed::Map(chosen(0, GATE_ENTRY))
        );
        let variants = open(&[(4, 0), (4, 1)]);
        let other = state(&variants, 1, true);
        assert_eq!(
            maps.confirm(2, 0, &other),
            Confirmed::Map(chosen(1, GATE_ENTRY))
        );
        assert_eq!(maps.confirm(2, 2, &later), Confirmed::Place(chosen(2, 2)));
    }

    #[test]
    fn a_choice_writes_the_place_or_the_gate() {
        let maps = maps();
        let choice = |map, entry| maps.choice(Chosen { map, entry });
        let to = |area, place, forest| {
            Some(Choice {
                area,
                place,
                forest,
            })
        };
        assert_eq!(choice(1, 4), to(2, 6, None));
        assert_eq!(choice(0, GATE_ENTRY), to(4, 0, None));
        assert_eq!(choice(2, 0), to(4, 0, None));
        assert_eq!(choice(2, 1), to(4, 0, None));
        assert_eq!(choice(2, 2), to(4, 0, Some(1)));
        assert_eq!(choice(5, 5), to(4, 0, Some(4)));
        assert_eq!(choice(2, 3), None);
    }

    #[test]
    fn the_records_are_read_from_an_image() {
        const BASE: u32 = 0x8000_2000;
        let at = |word: u32| BASE + word * 4;
        let halves = |low: i16, high: i16| (low as u16 as u32) | ((high as u16 as u32) << 16);
        // Word 0: the counts. 1: the name list's address. 2: the bytes' address.
        // 3: the area in its first byte, the count of searched bytes in its third.
        // 4: the destination bytes. 5: one name address. 6: the name. 7: nothing.
        let mut words = vec![1, at(5), at(4), 0x0002_0004, 0x0000_FF05, at(6)];
        words.extend([0x0000_8260, 0]);
        // Words 8 to 13: the map record. 14 to 20: the entry record. 21 and 22: the title.
        words.extend([75, 76, at(21), halves(160, 20), 13, at(14)]);
        words.extend([3, halves(12, 18), 12, halves(246, 166)]);
        words.extend([0x404020, 0x808040, 77]);
        words.extend([0x8262_8261, 0]);
        let data: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
        let layout = Layout {
            maps: at(8),
            counts: at(0),
            names: at(1),
            destinations: at(2),
            areas: at(3),
            searched: at(3) + 2,
            count: 1,
        };
        let maps = Maps::parse(&data, BASE, &layout).expect("sample map");
        let map = maps.get(0).expect("map 0");
        assert_eq!(
            (map.area, map.background_file, map.pictures_file),
            (4, 75, 76)
        );
        assert_eq!((map.title_at, map.music), ((160, 20), 13));
        assert_eq!(map.title, [0x8261, 0x8262]);
        assert_eq!(map.searched, [5, 0xFF]);
        assert_eq!(map.entries.len(), 1);
        let entry = &map.entries[0];
        assert_eq!(
            (entry.byte, entry.destination()),
            (5, Destination::Place(5))
        );
        assert_eq!(entry.name, [0x8260]);
        assert_eq!((entry.picture, entry.picture_at), (3, (12, 18)));
        assert_eq!((entry.marker, entry.marker_at), (12, (246, 166)));
        assert_eq!(
            (entry.colours, entry.large_file),
            ([0x404020, 0x808040], 77)
        );
        // A table that leaves the image is an error.
        let short = Layout { count: 2, ..layout };
        let err = Maps::parse(&data, BASE, &short);
        assert!(matches!(err, Err(MapError::OutOfImage(_))));
    }
}
