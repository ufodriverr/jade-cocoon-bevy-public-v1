
use std::fmt;

use crate::rooms::{self, Image, RoomTableError, step};

/// Archive file that holds the chapter screen and, inside it, the chapter table.
pub const CHAPTER_FILE: usize = 51;
/// RAM address of the first byte of [`CHAPTER_FILE`].
pub const CHAPTER_FILE_BASE: u32 = 0x800B_6890;
/// RAM address of the chapter table.
pub const CHAPTER_TABLE: u32 = 0x800B_71B0;
/// Chapters in the table.
pub const CHAPTER_COUNT: usize = 7;
/// RAM address, in the field engine file, of the number of places per area that a chapter's
/// place lists cover.
pub const PLACE_COUNTS: u32 = 0x800B_07F4;
/// Bytes of one chapter record.
const RECORD_SIZE: usize = 28;
/// Bytes of one extra variant entry.
const ENTRY_SIZE: usize = 8;
/// Room number that leaves a variant table entry as it is.
pub const KEEP_VARIANT: i16 = -2;

/// Why the chapter table could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChapterTableError {
    /// A pointer or a list leads outside the image. Holds the RAM address that was wanted.
    OutOfImage(u32),
}

impl fmt::Display for ChapterTableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfImage(address) => {
                write!(
                    f,
                    "chapter table leads outside the image, to {address:#010x}"
                )
            }
        }
    }
}

impl std::error::Error for ChapterTableError {}

impl From<RoomTableError> for ChapterTableError {
    fn from(err: RoomTableError) -> Self {
        match err {
            RoomTableError::OutOfImage(address) => Self::OutOfImage(address),
        }
    }
}

/// What the variant table is keyed by: a room slot of a place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VariantKey {
    pub area: i16,
    pub place: i16,
    pub slot: i16,
}

/// Passes of the chapter screen between its movie and the room, for a chapter whose
/// record ends the music.
pub const WAIT_BEHIND_MOVIE: u32 = 60;

/// RAM address of the list of story variables the chapter screen clears before it leaves
/// for the room.
pub const CLEARED_VARIABLES: u32 = 0x800B_7274;
/// The number that ends that list.
const CLEARED_END: i32 = 20;

/// The story variables the chapter screen clears, from the chapter screen file as it is
/// on the disc.
pub fn cleared_variables(chapter_file: &[u8]) -> Result<Vec<i32>, ChapterTableError> {
    let image = Image {
        data: chapter_file,
        base: CHAPTER_FILE_BASE,
    };
    let mut variables = Vec::new();
    loop {
        let variable = image.i32(step(CLEARED_VARIABLES, variables.len(), 4)?)?;
        if variable == CLEARED_END {
            return Ok(variables);
        }
        variables.push(variable);
    }
}

/// One chapter record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chapter {
    pub state_word: i32,
    /// Number of the room the chapter starts in.
    pub start_room: i32,
    /// Number of the music the chapter screen asks for before its movie.
    pub music: i32,
    /// Whether the screen ends the music behind the movie and waits `WAIT_BEHIND_MOVIE`
    /// passes before it goes on.
    pub ends_music: bool,
    /// Number of the chapter's movie.
    pub movie: i32,
    /// The variant of slot 0 of every place of every area the chapter lists, in area and
    /// place order. A room number of -1 closes the place.
    pub place_variants: Vec<(VariantKey, i16)>,
    /// Further variant entries, for slots other than 0.
    pub extra_variants: Vec<(VariantKey, i16)>,
}

/// The chapter table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterTable {
    chapters: Vec<Chapter>,
}

impl ChapterTable {
    /// Reads the table from the chapter screen file and the field engine file as they are
    /// on the disc.
    pub fn from_files(chapter_file: &[u8], field_engine: &[u8]) -> Result<Self, ChapterTableError> {
        let engine = Image {
            data: field_engine,
            base: rooms::FIELD_ENGINE_BASE,
        };
        let mut place_counts = [0; rooms::AREA_COUNT];
        for (area, count) in place_counts.iter_mut().enumerate() {
            *count = engine.u32(step(PLACE_COUNTS, area, 4)?)? as usize;
        }
        Self::parse(
            chapter_file,
            CHAPTER_FILE_BASE,
            CHAPTER_TABLE,
            CHAPTER_COUNT,
            &place_counts,
        )
    }

    /// Reads `count` 28-byte chapter records at RAM address `table` from an image whose first
    /// byte is at RAM address `base`.
    ///
    /// A record points at one pointer per area (`place_counts` has one entry per area); a
    /// non-null one leads to that many room numbers, one per place. It also points at 8-byte
    /// extra entries, ended by one with room number -1. A null pointer stands for no list.
    pub fn parse(
        data: &[u8],
        base: u32,
        table: u32,
        count: usize,
        place_counts: &[usize],
    ) -> Result<Self, ChapterTableError> {
        let image = Image { data, base };
        let mut chapters = Vec::with_capacity(count);
        for chapter in 0..count {
            let at = step(table, chapter, RECORD_SIZE)?;
            let area_lists = image.u32(at + 20)?;
            let mut place_variants = Vec::new();
            for (area, &places) in place_counts.iter().enumerate() {
                if area_lists == 0 {
                    break;
                }
                let list = image.u32(step(area_lists, area, 4)?)?;
                if list == 0 {
                    continue;
                }
                for place in 0..places {
                    let key = VariantKey {
                        area: area as i16,
                        place: place as i16,
                        slot: 0,
                    };
                    // The game stores the low 16 bits.
                    let room = image.i32(step(list, place, 4)?)? as i16;
                    place_variants.push((key, room));
                }
            }
            let extras = image.u32(at + 24)?;
            let mut extra_variants = Vec::new();
            if extras != 0 {
                loop {
                    let entry = step(extras, extra_variants.len(), ENTRY_SIZE)?;
                    let room = image.i16(entry + 6)?;
                    if room == -1 {
                        break;
                    }
                    let key = VariantKey {
                        area: image.i16(entry)?,
                        place: image.i16(entry + 2)?,
                        slot: image.i16(entry + 4)?,
                    };
                    extra_variants.push((key, room));
                }
            }
            chapters.push(Chapter {
                state_word: image.i32(at)?,
                start_room: image.i32(at + 4)?,
                music: image.i32(at + 8)?,
                ends_music: image.i32(at + 12)? != 0,
                movie: image.i32(at + 16)?,
                place_variants,
                extra_variants,
            });
        }
        Ok(Self { chapters })
    }

    /// Number of chapters.
    pub fn len(&self) -> usize {
        self.chapters.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chapters.is_empty()
    }

    /// The record of a chapter.
    pub fn get(&self, chapter: usize) -> Option<&Chapter> {
        self.chapters.get(chapter)
    }

    /// Every chapter record, in order.
    pub fn iter(&self) -> impl Iterator<Item = &Chapter> {
        self.chapters.iter()
    }
}

/// The variant table of the saved state: which story variant of a room a slot leads to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VariantTable {
    entries: Vec<(VariantKey, i16)>,
}

impl VariantTable {
    pub const ROOM: usize = 150;

    /// The empty table of a new game.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the room number for a key, adding an entry when the key is new.
    /// [`KEEP_VARIANT`] changes nothing.
    pub fn set(&mut self, key: VariantKey, room: i16) {
        if room == KEEP_VARIANT {
            return;
        }
        match self.entries.iter_mut().find(|(have, _)| *have == key) {
            Some(entry) => entry.1 = room,
            None => self.entries.push((key, room)),
        }
    }

    /// The room number for a key. `None` when the key has no entry or its entry is -1: the
    /// game does not tell the two apart. The value fits
    /// [`RoomTable::by_slot`](crate::rooms::RoomTable::by_slot).
    pub fn get(&self, key: VariantKey) -> Option<u16> {
        self.entries
            .iter()
            .find(|(have, _)| *have == key)
            .and_then(|(_, room)| u16::try_from(*room).ok())
    }

    /// Whether the place map lets the player choose a place: slot 0 of it has a variant.
    pub fn is_open(&self, area: i16, place: i16) -> bool {
        let slot = 0;
        self.get(VariantKey { area, place, slot }).is_some()
    }

    /// Applies a chapter's variant lists, the place lists first, as the chapter screen does.
    pub fn apply(&mut self, chapter: &Chapter) {
        let lists = chapter.place_variants.iter().chain(&chapter.extra_variants);
        for &(key, room) in lists {
            self.set(key, room);
        }
    }

    /// The table as it lies in the state block (+756, `0x8008B7F4`): the count as a
    /// word, then an entry of four half words for each key (area, place, slot, room).
    /// `None` for a table that has more entries than the block has room for.
    pub fn to_bytes(&self) -> Option<Vec<u8>> {
        if self.entries.len() > Self::ROOM {
            return None;
        }
        let mut bytes = (self.entries.len() as u32).to_le_bytes().to_vec();
        for (key, room) in &self.entries {
            for half in [key.area, key.place, key.slot, *room] {
                bytes.extend(half.to_le_bytes());
            }
        }
        Some(bytes)
    }

    /// The way back of [`Self::to_bytes`], from bytes that start at the count. `None`
    /// for a count past the block's room or past the bytes.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let count = u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?) as usize;
        if count > Self::ROOM {
            return None;
        }
        let half = |entry: &[u8], n: usize| i16::from_le_bytes([entry[2 * n], entry[2 * n + 1]]);
        let entries = bytes.get(4..4 + 8 * count)?.chunks_exact(8).map(|entry| {
            let key = VariantKey {
                area: half(entry, 0),
                place: half(entry, 1),
                slot: half(entry, 2),
            };
            (key, half(entry, 3))
        });
        Some(Self {
            entries: entries.collect(),
        })
    }

    /// The entries in the order they were added.
    pub fn entries(&self) -> &[(VariantKey, i16)] {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: u32 = 0x8000_1000;

    /// RAM address of word `index` of the image.
    fn word_address(index: u32) -> u32 {
        BASE + index * 4
    }

    /// The two 16-bit halves of a word, low half first.
    fn halves(low: i16, high: i16) -> u32 {
        (low as u16 as u32) | ((high as u16 as u32) << 16)
    }

    fn key(area: i16, place: i16, slot: i16) -> VariantKey {
        VariantKey { area, place, slot }
    }

    /// Two chapters over two areas of three and two places.
    fn sample() -> Vec<u8> {
        let minus = |value: i32| value as u32;
        let mut words = vec![1, 84, 126, 0, 10, word_address(14), word_address(18)];
        words.extend([0, 3, 128, 1, 13, word_address(24), 0]);
        // Words 14 and 15: chapter 0 lists area 1 only. Words 16 and 17: its two places.
        words.extend([0, word_address(16), 67, minus(-1)]);
        // Words 18 to 23: two extra entries and the end entry.
        words.extend([halves(1, 1), halves(1, 80), halves(1, 0), halves(2, -2)]);
        words.extend([halves(0, 0), halves(0, -1)]);
        // Words 24 and 25: chapter 1 lists area 0 only. Words 26 to 28: its three places.
        words.extend([word_address(26), 0, 3, minus(-2), 10]);
        words.iter().flat_map(|word| word.to_le_bytes()).collect()
    }

    fn table() -> ChapterTable {
        ChapterTable::parse(&sample(), BASE, BASE, 2, &[3, 2]).expect("sample table")
    }

    #[test]
    fn reads_the_chapter_records() {
        let table = table();
        assert_eq!(table.len(), 2);
        assert!(!table.is_empty());
        assert_eq!(
            table.get(0),
            Some(&Chapter {
                state_word: 1,
                start_room: 84,
                music: 126,
                ends_music: false,
                movie: 10,
                place_variants: vec![(key(1, 0, 0), 67), (key(1, 1, 0), -1)],
                extra_variants: vec![(key(1, 1, 1), 80), (key(1, 0, 2), -2)],
            })
        );
        let second = table.get(1).expect("chapter 1");
        assert_eq!(second.start_room, 3);
        assert_eq!(
            second.place_variants,
            [(key(0, 0, 0), 3), (key(0, 1, 0), -2), (key(0, 2, 0), 10)]
        );
        // A null pointer is an empty list.
        assert!(second.extra_variants.is_empty());
        assert!(table.get(2).is_none());
        assert_eq!(table.iter().count(), 2);
    }

    #[test]
    fn a_table_that_leaves_the_image_is_an_error() {
        let data = sample();
        let err = ChapterTable::parse(&data, BASE, BASE, 5, &[3, 2]);
        assert!(matches!(err, Err(ChapterTableError::OutOfImage(_))));
        // A place list longer than the image.
        let err = ChapterTable::parse(&data, BASE, BASE, 2, &[30, 2]);
        assert_eq!(
            err,
            Err(ChapterTableError::OutOfImage(word_address(26 + 3)))
        );
        // A table in front of the image.
        let err = ChapterTable::parse(&data, BASE + 0x100, BASE, 2, &[3, 2]);
        assert_eq!(err, Err(ChapterTableError::OutOfImage(BASE + 20)));
    }

    #[test]
    fn the_variant_table_sets_adds_and_keeps() {
        let mut variants = VariantTable::new();
        assert_eq!(variants.get(key(4, 1, 6)), None);
        variants.set(key(4, 1, 6), 113);
        variants.set(key(4, 0, 0), 105);
        assert_eq!(variants.get(key(4, 1, 6)), Some(113));
        // A second value for a key replaces the first; -2 changes nothing.
        variants.set(key(4, 1, 6), 112);
        variants.set(key(4, 0, 0), KEEP_VARIANT);
        variants.set(key(4, 2, 0), KEEP_VARIANT);
        assert_eq!(
            variants.entries(),
            [(key(4, 1, 6), 112), (key(4, 0, 0), 105)]
        );
        // An entry of -1 reads like no entry, and closes the place.
        assert!(variants.is_open(4, 0));
        variants.set(key(4, 0, 0), -1);
        assert_eq!(variants.get(key(4, 0, 0)), None);
        assert!(!variants.is_open(4, 0));
        assert!(!variants.is_open(4, 1));
        assert_eq!(variants.entries().len(), 2);
    }

    #[test]
    fn applying_chapters_in_order_builds_the_variant_table() {
        let table = table();
        let mut variants = VariantTable::new();
        variants.apply(table.get(0).expect("chapter 0"));
        assert!(variants.is_open(1, 0));
        assert!(!variants.is_open(1, 1));
        assert_eq!(variants.get(key(1, 1, 1)), Some(80));
        // The -2 entry of the extra list added nothing.
        assert_eq!(variants.entries().len(), 3);

        variants.apply(table.get(1).expect("chapter 1"));
        assert_eq!(variants.get(key(0, 0, 0)), Some(3));
        assert!(!variants.is_open(0, 1));
        assert_eq!(variants.get(key(0, 2, 0)), Some(10));
        // Chapter 0's entries stay.
        assert_eq!(variants.get(key(1, 0, 0)), Some(67));
        assert_eq!(variants.entries().len(), 5);
    }
}
