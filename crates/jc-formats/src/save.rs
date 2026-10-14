
use crate::creatures::{CAUGHT, RECORDS};

/// The archive file of the card code and the address it is loaded at.
pub const OVERLAY: usize = 48;
pub const BASE: u32 = 0x800B_6890;
/// The reader of a save in it.
pub const READER: u32 = 0x800B_A6D8;

/// The bytes of the state block `0x8008B500`, which end where the creature records
/// start.
pub const BLOCK: usize = 2440;
/// The bytes of a creature record and of a caught record in the game's memory.
pub const CREATURE: usize = 248;
pub const CAUGHT_RECORD: usize = 136;
/// The bytes each is packed to.
pub const PACKED_CREATURE: usize = 160;
pub const PACKED_CAUGHT: usize = 108;

/// Where the head of a save starts in its buffer, where the game starts and where it
/// ends; the check sum is the sum of the bytes from [`GAME`] to [`END`].
pub const HEAD: usize = 640;
pub const GAME: usize = 768;
pub const END: usize = 8128;

/// The pieces of the game behind the block, counted from [`GAME`]: the packed creature
/// records, the packed caught records, the six names of each creature's ancestors (ten
/// bytes each), each creature's own name and each caught creature's name (ten bytes in
/// a place of twelve).
const CREATURES_AT: usize = GAME + BLOCK;
const CAUGHT_AT: usize = CREATURES_AT + RECORDS * PACKED_CREATURE;
const ANCESTORS_AT: usize = CAUGHT_AT + CAUGHT * PACKED_CAUGHT;
const NAMES_AT: usize = ANCESTORS_AT + RECORDS * ANCESTORS * LETTERS;
const CAUGHT_NAMES_AT: usize = NAMES_AT + RECORDS * NAME;

/// The letters of a name a save keeps, and the bytes a name has in a record.
const LETTERS: usize = 10;
const NAME: usize = 12;
/// The names of a creature's ancestors, from +4 of its record.
const ANCESTORS: usize = 6;
const ANCESTORS_OF_A_RECORD: usize = 4;
/// A creature's own name in its record, and the four equipment bytes of its head,
/// which a load fills with 255.
const NAME_OF_A_CREATURE: usize = 148;
const EQUIPMENT_OF_A_CREATURE: usize = 160;
/// The same of a caught record.
const NAME_OF_A_CAUGHT: usize = 16;
const EQUIPMENT_OF_A_CAUGHT: usize = 28;
const NOTHING: u8 = 255;

/// The mark a save has to carry in the last word of the block.
pub const MARK_AT: usize = 2436;
pub const MARK: u32 = 5;

/// The offsets of the block the writer and the reader name.
mod block {
    /// The play clock, one a refresh.
    pub const CLOCK: usize = 4;
    /// The setting word and the four bytes of the window colour behind it, which a
    /// load can leave as the running game has them.
    pub const SETTINGS: std::ops::Range<usize> = 28..36;
    pub const MONEY: usize = 36;
    pub const HP: usize = 2000;
    pub const MOST_HP: usize = 2002;
    pub const NAME: usize = 2008;
    pub const LEVEL: usize = 2060;
    /// The word that picks the text of the place.
    pub const PLACE: usize = 2064;
}

/// A run of bytes the packing moves: where it lies packed, where in the record, and
/// its length.
type Run = (usize, usize, usize);

/// The writer's table of a creature record (`0x800B9EAC` on). The packed bytes 114,
/// 115, 154 and 155 are not written.
const CREATURE_RUNS: [Run; 11] = [
    (0, 0, 4),
    (4, 76, 48),
    (52, 124, 8),
    (60, 132, 16),
    (76, 164, 12),
    (88, 176, 18),
    (106, 194, 4),
    (110, 198, 4),
    (116, 204, 18),
    (134, 222, 20),
    (156, 244, 4),
];

/// The writer's table of a caught record (`0x800BA168` on). Of the 108 packed bytes
/// only these 58 are written.
const CAUGHT_RUNS: [Run; 8] = [
    (0, 0, 8),
    (8, 8, 8),
    (16, 32, 12),
    (28, 44, 18),
    (46, 62, 4),
    (50, 66, 2),
    (52, 68, 2),
    (76, 92, 4),
];

fn word(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

fn half(bytes: &[u8], at: usize) -> i16 {
    i16::from_le_bytes([bytes[at], bytes[at + 1]])
}

/// The check sum of a buffer: the plain sum of its bytes from [`GAME`] to [`END`].
/// `None` for a buffer that ends before.
pub fn sum(buffer: &[u8]) -> Option<u32> {
    let game = buffer.get(GAME..END)?;
    Some(game.iter().map(|byte| u32::from(*byte)).sum())
}

/// The hours and the minutes the head of a save shows for a play clock (`0x800BAF00`,
/// `0x800BAF54`): the game takes the clock for sixtieths of a second, and lets the
/// hours start again at 60.
pub fn clock(refreshes: u32) -> (u32, u32) {
    (refreshes / 216_000 % 60, refreshes / 3600 % 60)
}

/// Which of the three texts of the place a head has, by the word at +2064 of the
/// block: the text of the card's task for -2, else one of two texts of file 48.
pub fn place_kind(word: u32) -> u16 {
    match word as i32 {
        -2 => 0,
        -1 => 1,
        _ => 2,
    }
}

/// The two texts of the place that file 48 has itself, 24 bytes each: the one for the
/// word -1 and the one for any other word but -2.
pub const FIXED_PLACES: [u32; 2] = [0x800B_6E1C, 0x800B_6E08];

/// The two fixed texts of the place as file 48 has them ([`FIXED_PLACES`]).
pub fn fixed_places(file: &[u8]) -> Option<[[u8; 24]; 2]> {
    let text = |address: u32| -> Option<[u8; 24]> {
        let start = address.checked_sub(BASE)? as usize;
        file.get(start..start + 24)?.try_into().ok()
    };
    Some([text(FIXED_PLACES[0])?, text(FIXED_PLACES[1])?])
}

/// The head of a save, for the list of saves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    /// The hero's name, the twelve bytes of his record.
    pub name: [u8; NAME],
    /// The text of the place, and which of the three it is.
    pub place: [u8; 24],
    pub place_kind: u16,
    /// The low half of the word that picked it.
    pub place_word: u16,
    pub level: u32,
    pub hp: i32,
    pub most_hp: i32,
    pub hours: u32,
    pub minutes: u32,
    pub money: u32,
    pub sum: u32,
}

impl Head {
    /// The head of a buffer; `None` for a buffer that ends before the game starts.
    pub fn read(buffer: &[u8]) -> Option<Self> {
        let b = buffer.get(HEAD..GAME)?;
        Some(Self {
            name: b[..12].try_into().ok()?,
            place: b[12..36].try_into().ok()?,
            place_kind: half(b, 36) as u16,
            place_word: half(b, 38) as u16,
            level: word(b, 40),
            hp: word(b, 44) as i32,
            most_hp: word(b, 48) as i32,
            hours: word(b, 52),
            minutes: word(b, 56),
            money: word(b, 60),
            sum: word(b, 64),
        })
    }
}

/// The word of file 48 that points at the title of a save, which the writer changes
/// in place: 64 bytes of wide letters and a zero.
pub const TITLE: u32 = 0x800B_E298;
/// The three ends of a title by the kind of the place, with the bytes the writer
/// copies of each (the last of them a zero), and where they go in the title.
pub const TITLE_ENDS: [(u32, usize); 3] = [(0x800B_6E30, 23), (0x800B_6E48, 23), (0x800B_6E60, 17)];
const TITLE_END_AT: usize = 42;
/// Where the two wide digits of the hero's level go in the title, the lead byte of a
/// wide digit and the second byte of the wide zero.
const TITLE_LEVEL_AT: usize = 36;
const WIDE: u8 = 0x82;
const WIDE_ZERO: u8 = 0x4F;
/// The letters of a title in the head of a file.
const TITLE_BYTES: usize = 64;
/// The table of the icon's three pictures: a word for the image of the font's texture
/// set and two half words for a place of sixteen colours in video memory.
pub const ICON: u32 = 0x800B_E38C;
pub const ICON_PICTURES: usize = 3;
/// A picture of the icon: 16 by 16 pixels of four bits.
const ICON_WORDS: u16 = 4;
const ICON_ROWS: usize = 16;
const ICON_BYTES: usize = 128;
/// The four letters of file 48 for the pocket console, and its picture of 32 by 32
/// pixels of one bit.
pub const POCKET_MARK: u32 = 0x800B_7518;
pub const POCKET_PICTURE: u32 = 0x800B_E3A4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHead {
    /// The title as file 48 has it, with its zero.
    title: [u8; TITLE_BYTES + 1],
    ends: [Vec<u8>; 3],
    /// The sixteen colours of the icon, as the last of the three passes reads them.
    colours: [u8; 32],
    icon: [[u8; ICON_BYTES]; ICON_PICTURES],
    pocket_mark: [u8; 4],
    pocket_picture: [u8; 128],
}

impl FileHead {
    /// The head by the tables of file 48 (`card`) and the font's texture set, archive
    /// file 9 (`set`), whose images lie in video memory where the game reads the icon
    /// back from. `None` when a table lies outside the file, when an image of the icon
    /// is not sixteen pixels wide of four bits and at least sixteen high, or when no
    /// image of the set brings colours to the place the table names.
    pub fn read(card: &[u8], set: &[crate::tim::Tim]) -> Option<Self> {
        let bytes = |address: u32, length: usize| -> Option<&[u8]> {
            let start = address.checked_sub(BASE)? as usize;
            card.get(start..start.checked_add(length)?)
        };
        let mut title = [0; TITLE_BYTES + 1];
        title[..TITLE_BYTES].copy_from_slice(bytes(word(bytes(TITLE, 4)?, 0), TITLE_BYTES)?);
        let end = |n: usize| Some(bytes(TITLE_ENDS[n].0, TITLE_ENDS[n].1)?.to_vec());
        let ends = [end(0)?, end(1)?, end(2)?];

        let table = bytes(ICON, ICON_PICTURES * 8)?;
        let mut icon = [[0; ICON_BYTES]; ICON_PICTURES];
        let mut colours = [0; 32];
        for (n, picture) in icon.iter_mut().enumerate() {
            let image = set.get(word(table, n * 8) as usize)?;
            let sixteen_wide =
                image.mode == crate::tim::PixelMode::Clut4 && image.vram.w == ICON_WORDS;
            if !sixteen_wide || (image.height as usize) < ICON_ROWS {
                return None;
            }
            picture.copy_from_slice(image.indexes().get(..ICON_BYTES)?);
            // Every pass reads sixteen colours to the same place of the head, so the
            // last one's stay.
            let (x, y) = (half(table, n * 8 + 4) as u16, half(table, n * 8 + 6) as u16);
            let row = set.iter().find_map(|image| {
                let clut = image.clut.as_ref().filter(|clut| !clut.is_shared())?;
                let rect = clut.rect;
                let (right, down) = (x.checked_sub(rect.x)?, y.checked_sub(rect.y)?);
                if right + 16 > rect.w || down >= rect.h {
                    return None;
                }
                let start = usize::from(down) * usize::from(rect.w) + usize::from(right);
                clut.colors.get(start..start + 16)
            })?;
            for (to, colour) in colours.chunks_exact_mut(2).zip(row) {
                to.copy_from_slice(&colour.to_le_bytes());
            }
        }
        Some(Self {
            title,
            ends,
            colours,
            icon,
            pocket_mark: bytes(POCKET_MARK, 4)?.try_into().ok()?,
            pocket_picture: bytes(POCKET_PICTURE, 128)?.try_into().ok()?,
        })
    }

    /// The title of a save: the level as two wide digits, where a level above 100
    /// counts as 99 and a level of 100 has the letter ten behind the wide zero for its
    /// tens, and the end by the kind of the place. The letters up to the first zero,
    /// 64 at most.
    pub fn title(&self, level: i32, place_kind: i16) -> Vec<u8> {
        let mut title = self.title;
        let level = if level > 100 { 99 } else { level };
        let digit = |n: i32| WIDE_ZERO.wrapping_add(n as u8);
        title[TITLE_LEVEL_AT..TITLE_LEVEL_AT + 4].copy_from_slice(&[
            WIDE,
            digit(level / 10),
            WIDE,
            digit(level % 10),
        ]);
        let end = &self.ends[match place_kind {
            0 => 0,
            1 => 1,
            _ => 2,
        }];
        title[TITLE_END_AT..TITLE_END_AT + end.len()].copy_from_slice(end);
        let letters = title.iter().position(|byte| *byte == 0);
        title[..letters.unwrap_or(TITLE_BYTES).min(TITLE_BYTES)].to_vec()
    }

    /// The end of the writer and the maker of the head `0x800BB598`: the first 640
    /// bytes of a buffer whose head of the save is written (`Memory::write`), from
    /// which the level and the kind of the place are taken. The bytes 88 to 91 are not
    /// written. Answers false, and writes nothing, for a buffer that ends before the
    /// game starts.
    pub fn write(&self, buffer: &mut [u8]) -> bool {
        if buffer.len() < GAME {
            return false;
        }
        let title = self.title(word(buffer, HEAD + 40) as i32, half(buffer, HEAD + 36));
        // The two letters of every file of a card, an icon of three pictures, one block.
        buffer[..4].copy_from_slice(&[b'S', b'C', 0x13, 1]);
        buffer[4..80].fill(0);
        buffer[4..4 + title.len()].copy_from_slice(&title);
        buffer[80..82].copy_from_slice(&1u16.to_le_bytes());
        buffer[82..86].copy_from_slice(&self.pocket_mark);
        buffer[86..88].fill(0);
        buffer[92..96].fill(0);
        buffer[96..128].copy_from_slice(&self.colours);
        for (n, picture) in self.icon.iter().enumerate() {
            buffer[128 + n * ICON_BYTES..128 + (n + 1) * ICON_BYTES].copy_from_slice(picture);
        }
        buffer[512..HEAD].copy_from_slice(&self.pocket_picture);
        true
    }
}

/// The game's memory that a save holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Memory {
    pub block: [u8; BLOCK],
    pub creatures: [[u8; CREATURE]; RECORDS],
    pub caught: [[u8; CAUGHT_RECORD]; CAUGHT],
}

impl Memory {
    /// Memory of nothing but zeros.
    pub fn zeroed() -> Self {
        Self {
            block: [0; BLOCK],
            creatures: [[0; CREATURE]; RECORDS],
            caught: [[0; CAUGHT_RECORD]; CAUGHT],
        }
    }

    /// The writer `0x800B9D78`: the head and the game go into the buffer of a save.
    /// `place` is the text of the place for a word -2 at +2064 of the block (the one
    /// the card's task has), `fixed` the two texts of file 48 for -1 and for any other
    /// word. Bytes of the buffer the writer does not name stay as they are, and the
    /// check sum counts them. Answers false, and writes nothing, for a buffer that
    /// ends before the game does.
    pub fn write(&self, buffer: &mut [u8], place: &[u8; 24], fixed: &[[u8; 24]; 2]) -> bool {
        if buffer.len() < END {
            return false;
        }
        let block = &self.block;
        let place_word = word(block, block::PLACE);
        let kind = place_kind(place_word);
        let text = match kind {
            0 => place,
            1 => &fixed[0],
            _ => &fixed[1],
        };
        let (hours, minutes) = clock(word(block, block::CLOCK));
        let head = &mut buffer[HEAD..GAME];
        head[..12].copy_from_slice(&block[block::NAME..block::NAME + NAME]);
        head[12..36].copy_from_slice(text);
        head[36..38].copy_from_slice(&kind.to_le_bytes());
        head[38..40].copy_from_slice(&(place_word as u16).to_le_bytes());
        let words = [
            u32::from(block[block::LEVEL]),
            i32::from(half(block, block::HP)) as u32,
            i32::from(half(block, block::MOST_HP)) as u32,
            hours,
            minutes,
            word(block, block::MONEY),
        ];
        for (n, value) in words.into_iter().enumerate() {
            head[40 + 4 * n..44 + 4 * n].copy_from_slice(&value.to_le_bytes());
        }

        buffer[GAME..CREATURES_AT].copy_from_slice(block);
        for (n, record) in self.creatures.iter().enumerate() {
            let packed = CREATURES_AT + n * PACKED_CREATURE;
            for (to, from, length) in CREATURE_RUNS {
                buffer[packed + to..packed + to + length]
                    .copy_from_slice(&record[from..from + length]);
            }
            for k in 0..ANCESTORS {
                let to = ANCESTORS_AT + (n * ANCESTORS + k) * LETTERS;
                let from = ANCESTORS_OF_A_RECORD + k * NAME;
                buffer[to..to + LETTERS].copy_from_slice(&record[from..from + LETTERS]);
            }
            let to = NAMES_AT + n * NAME;
            let name = &record[NAME_OF_A_CREATURE..NAME_OF_A_CREATURE + LETTERS];
            buffer[to..to + LETTERS].copy_from_slice(name);
        }
        for (n, record) in self.caught.iter().enumerate() {
            let packed = CAUGHT_AT + n * PACKED_CAUGHT;
            for (to, from, length) in CAUGHT_RUNS {
                buffer[packed + to..packed + to + length]
                    .copy_from_slice(&record[from..from + length]);
            }
            let to = CAUGHT_NAMES_AT + n * NAME;
            let name = &record[NAME_OF_A_CAUGHT..NAME_OF_A_CAUGHT + LETTERS];
            buffer[to..to + LETTERS].copy_from_slice(name);
        }

        let sum: u32 = (buffer[GAME..END].iter())
            .map(|byte| u32::from(*byte))
            .sum();
        buffer[HEAD + 64..HEAD + 68].copy_from_slice(&sum.to_le_bytes());
        true
    }

    /// The reader `0x800BA6D8`: the game of a buffer goes over this memory, which is
    /// the running game's. A buffer whose check sum is not the sum of its game, or
    /// whose block has not the mark, is refused: the answer is false and nothing
    /// changes. What a save does not hold of a record stays as this memory has it, but
    /// the equipment bytes, which become 255, and the two bytes behind every name's
    /// ten letters, which become 0. With `keep_settings` (bit `0x08` of the word at
    /// +84 of the card's record) the setting word and the window colour stay the
    /// running game's.
    pub fn read(&mut self, buffer: &[u8], keep_settings: bool) -> bool {
        let Some(head) = Head::read(buffer) else {
            return false;
        };
        if sum(buffer) != Some(head.sum) || word(buffer, GAME + MARK_AT) != MARK {
            return false;
        }
        let settings: [u8; 8] = std::array::from_fn(|n| self.block[block::SETTINGS.start + n]);
        self.block.copy_from_slice(&buffer[GAME..CREATURES_AT]);

        let name = |to: &mut [u8], from: usize| {
            to[..LETTERS].copy_from_slice(&buffer[from..from + LETTERS]);
            to[LETTERS..NAME].fill(0);
        };
        for (n, record) in self.creatures.iter_mut().enumerate() {
            let packed = CREATURES_AT + n * PACKED_CREATURE;
            for (from, to, length) in CREATURE_RUNS {
                record[to..to + length]
                    .copy_from_slice(&buffer[packed + from..packed + from + length]);
            }
            record[EQUIPMENT_OF_A_CREATURE..EQUIPMENT_OF_A_CREATURE + 4].fill(NOTHING);
            for k in 0..ANCESTORS {
                let to = ANCESTORS_OF_A_RECORD + k * NAME;
                name(
                    &mut record[to..to + NAME],
                    ANCESTORS_AT + (n * ANCESTORS + k) * LETTERS,
                );
            }
            name(
                &mut record[NAME_OF_A_CREATURE..NAME_OF_A_CREATURE + NAME],
                NAMES_AT + n * NAME,
            );
        }
        for (n, record) in self.caught.iter_mut().enumerate() {
            let packed = CAUGHT_AT + n * PACKED_CAUGHT;
            for (from, to, length) in CAUGHT_RUNS {
                record[to..to + length]
                    .copy_from_slice(&buffer[packed + from..packed + from + length]);
            }
            record[EQUIPMENT_OF_A_CAUGHT..EQUIPMENT_OF_A_CAUGHT + 4].fill(NOTHING);
            name(
                &mut record[NAME_OF_A_CAUGHT..NAME_OF_A_CAUGHT + NAME],
                CAUGHT_NAMES_AT + n * NAME,
            );
        }

        if keep_settings {
            self.block[block::SETTINGS].copy_from_slice(&settings);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLACE: [u8; 24] = [7; 24];
    const FIXED: [[u8; 24]; 2] = [[8; 24], [9; 24]];

    /// Memory whose every byte tells where it lies, with the mark and a place word.
    fn patterned() -> Memory {
        let mut memory = Memory::zeroed();
        for (n, byte) in memory.block.iter_mut().enumerate() {
            *byte = (n * 7 + 1) as u8;
        }
        for (r, record) in memory.creatures.iter_mut().enumerate() {
            for (n, byte) in record.iter_mut().enumerate() {
                *byte = (n + r * 3 + 1) as u8 | 1;
            }
        }
        for (r, record) in memory.caught.iter_mut().enumerate() {
            for (n, byte) in record.iter_mut().enumerate() {
                *byte = (n * 5 + r) as u8 | 1;
            }
        }
        memory.block[MARK_AT..MARK_AT + 4].copy_from_slice(&MARK.to_le_bytes());
        memory.block[block::PLACE..block::PLACE + 4].copy_from_slice(&(-2i32).to_le_bytes());
        memory
    }

    fn saved(memory: &Memory) -> Vec<u8> {
        let mut buffer = vec![0xAA; END + 64];
        assert!(memory.write(&mut buffer, &PLACE, &FIXED));
        buffer
    }

    #[test]
    fn the_pieces_of_a_save_fit_without_a_gap() {
        assert_eq!(CREATURES_AT, 768 + 2440);
        assert_eq!(CAUGHT_AT, 768 + 4840);
        assert_eq!(ANCESTORS_AT, 768 + 6136);
        assert_eq!(NAMES_AT, 768 + 7036);
        assert_eq!(CAUGHT_NAMES_AT, 768 + 7216);
        assert_eq!(CAUGHT_NAMES_AT + CAUGHT * NAME, END);
        // The runs of a table lie in order and inside both records.
        let fits = |runs: &[Run], packed: usize, record: usize| {
            runs.windows(2).all(|pair| {
                let ((to, from, length), (next_to, next_from, _)) = (pair[0], pair[1]);
                to + length <= next_to && from + length <= next_from
            }) && runs
                .iter()
                .all(|(to, from, length)| to + length <= packed && from + length <= record)
        };
        assert!(fits(&CREATURE_RUNS, PACKED_CREATURE, CREATURE));
        assert!(fits(&CAUGHT_RUNS, PACKED_CAUGHT, CAUGHT_RECORD));
        let written = |runs: &[Run]| runs.iter().map(|run| run.2).sum::<usize>();
        assert_eq!(written(&CREATURE_RUNS), PACKED_CREATURE - 4);
        assert_eq!(written(&CAUGHT_RUNS), 58);
    }

    #[test]
    fn the_fixed_texts_of_the_place_are_read_by_their_addresses() {
        let at = |address: u32| (address - BASE) as usize;
        let mut file = vec![0; at(FIXED_PLACES[0]) + 24];
        file[at(FIXED_PLACES[0])..].fill(1);
        file[at(FIXED_PLACES[1])..at(FIXED_PLACES[1]) + 20].fill(2);
        let mut second = [2; 24];
        second[20..].fill(1);
        assert_eq!(fixed_places(&file), Some([[1; 24], second]));
        // A file that ends inside a text has none.
        assert_eq!(fixed_places(&file[..file.len() - 1]), None);
    }

    /// An image of sixteen colours, `words` wide and sixteen high, whose every pixel
    /// byte and colour is `mark`, with its colours at (`x`, `y`) of video memory.
    fn image(words: u16, mark: u8, (x, y): (u16, u16)) -> crate::tim::Tim {
        let mut data = vec![0x10, 0, 0, 0, 8, 0, 0, 0];
        let mut block = |rect: [u16; 4], bytes: Vec<u8>| {
            data.extend((12 + bytes.len() as u32).to_le_bytes());
            data.extend(rect.iter().flat_map(|half| half.to_le_bytes()));
            data.extend(bytes);
        };
        block([x, y, 16, 1], vec![mark; 32]);
        block([0, 0, words, 16], vec![mark; usize::from(words) * 2 * 16]);
        crate::tim::Tim::parse(&data, 0).expect("an image")
    }

    /// A file 48 with the tables of a file's head: a title of the letters `T`, the
    /// three ends, an icon of the images 1, 2 and 0, the mark and the picture.
    fn card_file() -> Vec<u8> {
        let at = |address: u32| (address - BASE) as usize;
        let mut file = vec![0; at(POCKET_PICTURE) + 128];
        let title = 0x800B_6DC4u32;
        file[at(TITLE)..at(TITLE) + 4].copy_from_slice(&title.to_le_bytes());
        file[at(title)..at(title) + 64].fill(b'T');
        for (n, (address, length)) in TITLE_ENDS.into_iter().enumerate() {
            file[at(address)..at(address) + length - 1].fill(b'a' + n as u8);
        }
        for (n, (number, x, y)) in [(1u32, 32u16, 9u16), (2, 48, 9), (0, 16, 9)]
            .into_iter()
            .enumerate()
        {
            let record = at(ICON) + n * 8;
            file[record..record + 4].copy_from_slice(&number.to_le_bytes());
            file[record + 4..record + 6].copy_from_slice(&x.to_le_bytes());
            file[record + 6..record + 8].copy_from_slice(&y.to_le_bytes());
        }
        file[at(POCKET_MARK)..at(POCKET_MARK) + 4].copy_from_slice(b"mark");
        file[at(POCKET_PICTURE)..].fill(0x55);
        file
    }

    #[test]
    fn the_head_of_a_file_has_a_title_an_icon_and_the_pocket_picture() {
        let set = [
            image(4, 1, (16, 9)),
            image(4, 2, (32, 9)),
            image(4, 3, (48, 9)),
        ];
        let head = FileHead::read(&card_file(), &set).expect("a head of a file");

        let mut memory = patterned();
        memory.block[block::LEVEL] = 7;
        let mut buffer = saved(&memory);
        buffer[88..92].fill(0xAA);
        assert!(head.write(&mut buffer));
        assert_eq!(buffer[..4], [b'S', b'C', 0x13, 1]);
        // The title: the level at 36, the end of kind 0 at 42 with its zero at 64.
        let mut title = [b'T'; 64];
        title[36..40].copy_from_slice(&[0x82, 0x4F, 0x82, 0x4F + 7]);
        title[42..].fill(b'a');
        assert_eq!(buffer[4..68], title);
        assert!(buffer[68..80].iter().all(|byte| *byte == 0));
        assert_eq!(buffer[80..88], [1, 0, b'm', b'a', b'r', b'k', 0, 0]);
        assert_eq!(buffer[88..96], [0xAA, 0xAA, 0xAA, 0xAA, 0, 0, 0, 0]);
        // The colours of the last pass, image 0's, and the pictures in the table's order.
        assert!(buffer[96..128].iter().all(|byte| *byte == 1));
        for (n, mark) in [2, 3, 1].into_iter().enumerate() {
            let picture = &buffer[128 + n * 128..256 + n * 128];
            assert!(picture.iter().all(|byte| *byte == mark));
        }
        assert!(buffer[512..640].iter().all(|byte| *byte == 0x55));
        // The save behind it is as it was.
        assert_eq!(buffer[640..], saved(&memory)[640..]);
        assert!(memory.clone().read(&buffer, false));
        assert!(!head.write(&mut buffer[..GAME - 1]));
    }

    #[test]
    fn the_title_of_a_save_tells_the_level_and_the_kind_of_the_place() {
        let set = [
            image(4, 1, (16, 9)),
            image(4, 2, (32, 9)),
            image(4, 3, (48, 9)),
        ];
        let file = card_file();
        let head = FileHead::read(&file, &set).expect("a head of a file");
        let level = |title: &[u8]| [title[37] - 0x4F, title[39] - 0x4F];
        assert_eq!(level(&head.title(42, 0)), [4, 2]);
        assert_eq!(level(&head.title(100, 0)), [10, 0]);
        assert_eq!(level(&head.title(101, 0)), [9, 9]);
        // The ends: 22 letters for the kinds 0 and 1, 16 for every other kind.
        assert_eq!(head.title(1, 1)[42..], [b'b'; 22]);
        assert_eq!(head.title(1, 2)[42..], [b'c'; 16]);
        assert_eq!(head.title(1, -1), head.title(1, 2));

        // An image of the icon that is not sixteen pixels wide, a place without
        // colours and a file that ends inside a table give no head.
        let wide = [image(4, 1, (16, 9)), image(7, 2, (32, 9)), set[2].clone()];
        assert_eq!(FileHead::read(&file, &wide), None);
        let nowhere = [image(4, 1, (17, 9)), set[1].clone(), set[2].clone()];
        assert_eq!(FileHead::read(&file, &nowhere), None);
        assert_eq!(FileHead::read(&file, &set[..2]), None);
        assert_eq!(FileHead::read(&file[..file.len() - 1], &set), None);
    }

    #[test]
    fn the_clock_takes_sixty_refreshes_for_a_second() {
        assert_eq!(clock(0), (0, 0));
        assert_eq!(clock(3599), (0, 0));
        assert_eq!(clock(3600), (0, 1));
        assert_eq!(clock(216_000 + 59 * 3600), (1, 59));
        // The hours start again at 60.
        assert_eq!(clock(61 * 216_000), (1, 0));
    }

    #[test]
    fn the_head_tells_who_and_where_and_how_long() {
        let mut memory = patterned();
        memory.block[block::CLOCK..block::CLOCK + 4]
            .copy_from_slice(&(2 * 216_000u32 + 5 * 3600).to_le_bytes());
        memory.block[block::HP..block::HP + 2].copy_from_slice(&(-3i16).to_le_bytes());
        memory.block[block::MOST_HP..block::MOST_HP + 2].copy_from_slice(&120i16.to_le_bytes());
        memory.block[block::LEVEL] = 9;
        memory.block[block::MONEY..block::MONEY + 4].copy_from_slice(&4321u32.to_le_bytes());
        let buffer = saved(&memory);
        let head = Head::read(&buffer).expect("a head");
        assert_eq!(head.name[..], memory.block[2008..2020]);
        assert_eq!(
            (head.place, head.place_kind, head.place_word),
            (PLACE, 0, 0xFFFE)
        );
        assert_eq!((head.level, head.hp, head.most_hp), (9, -3, 120));
        assert_eq!((head.hours, head.minutes, head.money), (2, 5, 4321));
        assert_eq!(Some(head.sum), sum(&buffer));
        // The bytes in front of the head and behind the game are not the writer's.
        assert!(buffer[..HEAD].iter().all(|byte| *byte == 0xAA));
        assert!(buffer[HEAD + 68..GAME].iter().all(|byte| *byte == 0xAA));
        assert!(buffer[END..].iter().all(|byte| *byte == 0xAA));

        // The two other texts of the place.
        for (word, kind, text) in [(-1i32, 1, FIXED[0]), (4, 2, FIXED[1])] {
            memory.block[block::PLACE..block::PLACE + 4].copy_from_slice(&word.to_le_bytes());
            let head = Head::read(&saved(&memory)).expect("a head");
            assert_eq!((head.place, head.place_kind), (text, kind));
            assert_eq!(head.place_word, word as u16);
        }
        assert_eq!(Head::read(&buffer[..GAME - 1]), None);
    }

    #[test]
    fn a_load_brings_back_what_a_save_took() {
        let memory = patterned();
        let buffer = saved(&memory);
        let mut running = Memory {
            block: [0xEE; BLOCK],
            creatures: [[0xEE; CREATURE]; RECORDS],
            caught: [[0xEE; CAUGHT_RECORD]; CAUGHT],
        };
        assert!(running.read(&buffer, false));
        assert_eq!(running.block, memory.block);

        for (loaded, was) in running.creatures.iter().zip(&memory.creatures) {
            for (_, at, length) in CREATURE_RUNS {
                assert_eq!(loaded[at..at + length], was[at..at + length]);
            }
            // The seven names: ten letters and two zeros.
            for at in [4, 16, 28, 40, 52, 64, 148] {
                assert_eq!(loaded[at..at + 10], was[at..at + 10]);
                assert_eq!(loaded[at + 10..at + 12], [0, 0]);
            }
            assert_eq!(loaded[160..164], [255; 4]);
            // What a save does not hold stays the running game's.
            assert_eq!(
                [loaded[202], loaded[203], loaded[242], loaded[243]],
                [0xEE; 4]
            );
            let held = (loaded.iter().zip(was))
                .filter(|(loaded, was)| loaded == was)
                .count();
            assert_eq!(held, CREATURE - 4 - 7 * 2 - 4);
        }
        for (loaded, was) in running.caught.iter().zip(&memory.caught) {
            for (_, at, length) in CAUGHT_RUNS {
                assert_eq!(loaded[at..at + length], was[at..at + length]);
            }
            assert_eq!(loaded[16..26], was[16..26]);
            assert_eq!(loaded[26..28], [0, 0]);
            assert_eq!(loaded[28..32], [255; 4]);
            assert!(loaded[70..92].iter().all(|byte| *byte == 0xEE));
            assert!(loaded[96..].iter().all(|byte| *byte == 0xEE));
        }
    }

    #[test]
    fn a_load_can_keep_the_settings_of_the_running_game() {
        let buffer = saved(&patterned());
        let mut running = Memory::zeroed();
        running.block[28..36].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        assert!(running.read(&buffer, true));
        assert_eq!(running.block[28..36], [1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(running.block[..28], patterned().block[..28]);
        assert_eq!(running.block[36..], patterned().block[36..]);
    }

    #[test]
    fn a_buffer_that_is_no_save_is_refused() {
        let memory = patterned();
        let before = Memory::zeroed();
        // A byte of the game changed behind the sum.
        let mut buffer = saved(&memory);
        buffer[GAME + 100] ^= 1;
        let mut running = before.clone();
        assert!(!running.read(&buffer, false));
        assert_eq!(running, before);
        // Another mark, with a sum that is right.
        let mut unmarked = memory.clone();
        unmarked.block[MARK_AT] = 4;
        assert!(!running.read(&saved(&unmarked), false));
        assert_eq!(running, before);
        // A buffer that ends early, for both ways.
        let buffer = saved(&memory);
        assert!(!running.read(&buffer[..END - 1], false));
        assert!(!memory.write(&mut vec![0; END - 1], &PLACE, &FIXED));
        assert_eq!(sum(&buffer[..END - 1]), None);
    }
}
