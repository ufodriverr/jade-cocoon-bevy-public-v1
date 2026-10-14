
use crate::rooms::{self, Image};
use crate::sound::SoundError;

/// How many kinds the library knows.
pub const KINDS: usize = 10;
/// RAM address of the word of each kind: where the chip's work area starts.
const STARTS: u32 = 0x8008_B0B4;
/// RAM address of the rows of the kinds: a mask word and the chip's words.
const ROWS: u32 = 0x8008_B0E4;
const ROW_SIZE: usize = 68;
/// The words of a kind, in the order of the chip's registers.
pub const WORDS: usize = 32;
/// The kind the start of the game asks for, and the depth the read callbacks set on
/// both sides, of 127.
pub const GAME_KIND: usize = 4;
pub const GAME_DEPTH: u16 = 50;
/// The work area ends where the chip's memory does, in units of 8 bytes.
const MEMORY_END: usize = 0x1_0000;
/// Values of the work area in a unit of the words that name a place.
const VALUES_A_UNIT: usize = 4;

/// The places of the words in a row. A word whose name starts with `V` is a volume,
/// with `D` a distance and with `M` a place in the work area; the places and
/// distances of the right side follow those of the left.
const D_APF1: usize = 0;
const D_APF2: usize = 1;
const V_IIR: usize = 2;
const V_COMB: [usize; 4] = [3, 4, 5, 6];
const V_WALL: usize = 7;
const V_APF1: usize = 8;
const V_APF2: usize = 9;
const M_SAME: usize = 10;
const M_COMB: [usize; 4] = [12, 14, 20, 22];
const D_SAME: usize = 16;
const M_DIFF: usize = 18;
const D_DIFF: usize = 24;
const M_APF1: usize = 26;
const M_APF2: usize = 28;
const V_IN: usize = 30;

/// The filter between the chip's rate and the half of it the reverb runs at: 39
/// numbers, alike from both ends, every second one 0 but the middle.
const FILTER_OUTER: [i32; 10] = [-1, 2, -10, 35, -103, 266, -616, 1_332, -2_960, 10_246];
const FILTER_MIDDLE: i32 = 16_384;
const FILTER: usize = 39;

/// One kind of reverb as the library keeps it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kind {
    /// Where the work area starts, in units of 8 bytes.
    pub start: u16,
    /// The words the setter hands to the chip that are named by a bit; 0 for all.
    pub mask: u32,
    pub words: [u16; WORDS],
}

impl Kind {
    /// Reads a kind from the loaded executable. `None` for a number the library does
    /// not take.
    pub fn from_executable(
        image: &[u8],
        base: u32,
        kind: usize,
    ) -> Result<Option<Self>, SoundError> {
        if kind >= KINDS {
            return Ok(None);
        }
        let image = Image { data: image, base };
        let row = rooms::step(ROWS, kind, ROW_SIZE)?;
        let mut words = [0; WORDS];
        for (index, word) in words.iter_mut().enumerate() {
            *word = image.u16(rooms::step(row + 4, index, 2)?)?;
        }
        Ok(Some(Self {
            start: image.u32(rooms::step(STARTS, kind, 4)?)? as u16,
            mask: image.u32(row)?,
            words,
        }))
    }

    /// Values of sound the work area holds.
    pub fn values(&self) -> usize {
        (MEMORY_END - usize::from(self.start)) * VALUES_A_UNIT
    }
}

/// The word the library makes of a depth of 0 to 127.
pub fn depth(of_127: u16) -> i16 {
    (i32::from(of_127.min(127)) * 0x7FFF / 127) as i16
}

fn limit(value: i32) -> i32 {
    value.clamp(i32::from(i16::MIN), i32::from(i16::MAX))
}

/// The chip's reverb at work on a kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reverb {
    words: [u16; WORDS],
    area: Vec<i16>,
    /// The value of the work area the places are counted from.
    at: usize,
    depth: [i16; 2],
    /// The last values that went in, by side, and the last values the reverb made
    /// at half the rate.
    went_in: [[i16; 64]; 2],
    made: [[i16; 32]; 2],
    ticks: usize,
}

impl Reverb {
    /// A silent work area and no depth.
    pub fn new(kind: &Kind) -> Self {
        Self {
            words: kind.words,
            area: vec![0; kind.values().max(1)],
            at: 0,
            depth: [0; 2],
            went_in: [[0; 64]; 2],
            made: [[0; 32]; 2],
            ticks: 0,
        }
    }

    /// The reverb the game plays through: its kind at its depth.
    pub fn of_the_game(image: &[u8], base: u32) -> Result<Self, SoundError> {
        let kind = Kind::from_executable(image, base, GAME_KIND)?;
        let mut reverb = Self::new(&kind.ok_or(SoundError::OutOfImage(ROWS))?);
        reverb.set_depth([depth(GAME_DEPTH); 2]);
        Ok(reverb)
    }

    /// The depth words of the two sides: how much of the reverb is heard.
    pub fn set_depth(&mut self, depth: [i16; 2]) {
        self.depth = depth;
    }

    fn volume(&self, word: usize) -> i32 {
        i32::from(self.words[word] as i16)
    }

    /// The value at the place a word names, `back` values in front of it.
    fn place(&self, word: usize, back: usize) -> usize {
        let place = usize::from(self.words[word]) * VALUES_A_UNIT;
        let size = self.area.len();
        (self.at + place % size + size - back % size) % size
    }

    fn read(&self, word: usize, back: usize) -> i32 {
        i32::from(self.area[self.place(word, back)])
    }

    fn write(&mut self, word: usize, value: i32) {
        let place = self.place(word, 0);
        self.area[place] = limit(value) as i16;
    }

    /// A reflection: what came in and what a wall sends back, drawn to the value in
    /// front of its place by the share `V_IIR`.
    fn reflect(&mut self, came_in: i32, place: usize, from: usize) {
        let wall = (self.read(from, 0) * self.volume(V_WALL)) >> 15;
        let before = self.read(place, 1);
        let drawn = ((came_in + wall - before) * self.volume(V_IIR)) >> 15;
        self.write(place, drawn + before);
    }

    /// The filter that passes all: the value a distance back, taken off and given on.
    fn pass(&mut self, value: i32, place: usize, distance: usize, volume: usize) -> i32 {
        let back = usize::from(self.words[distance]) * VALUES_A_UNIT;
        let (held, volume) = (self.read(place, back), self.volume(volume));
        let stored = limit(value - ((volume * held) >> 15));
        self.write(place, stored);
        limit(((stored * volume) >> 15) + held)
    }

    /// One value at half the chip's rate, left and right.
    fn run(&mut self, came_in: [i32; 2]) -> [i16; 2] {
        let mut out = [0; 2];
        for side in 0..2 {
            let other = 1 - side;
            let came_in = (came_in[side] * self.volume(V_IN + side)) >> 15;
            self.reflect(came_in, M_SAME + side, D_SAME + side);
            self.reflect(came_in, M_DIFF + side, D_DIFF + other);
            let early: i32 = (0..4)
                .map(|comb| self.volume(V_COMB[comb]) * self.read(M_COMB[comb] + side, 0))
                .sum();
            let value = self.pass(limit(early >> 15), M_APF1 + side, D_APF1, V_APF1);
            out[side] = self.pass(value, M_APF2 + side, D_APF2, V_APF2) as i16;
        }
        self.at = (self.at + 1) % self.area.len();
        out
    }

    /// One value of sound: what the voices with reverb add up to goes in, left and
    /// right, and what the reverb adds to the mix comes out.
    pub fn tick(&mut self, came_in: [i32; 2]) -> [i32; 2] {
        let slot = self.ticks % 64;
        for (went_in, came_in) in self.went_in.iter_mut().zip(came_in) {
            went_in[slot] = limit(came_in) as i16;
        }
        let half = (self.ticks / 2) % 32;
        let fresh = self.ticks.is_multiple_of(2);
        self.ticks = self.ticks.wrapping_add(1);
        // The outer numbers of the filter over 20 values, every second one of 39 or
        // every one at half the rate.
        let outer = |value: &dyn Fn(usize) -> i32| -> i32 {
            let ends = (0..10).map(|n| FILTER_OUTER[n] * (value(n) + value(19 - n)));
            ends.sum()
        };
        if fresh {
            let mut slower = [0; 2];
            for (side, slower) in slower.iter_mut().enumerate() {
                let went_in = &self.went_in[side];
                let back = |back: usize| i32::from(went_in[(slot + 64 - back) % 64]);
                let sum = outer(&|n| back(FILTER - 1 - 2 * n)) + FILTER_MIDDLE * back(FILTER / 2);
                *slower = limit(sum >> 15);
            }
            let made = self.run(slower);
            for (kept, made) in self.made.iter_mut().zip(made) {
                kept[half] = made;
            }
        }
        let mut out = [0; 2];
        for (side, out) in out.iter_mut().enumerate() {
            let made = &self.made[side];
            let back = |back: usize| i32::from(made[(half + 32 - back) % 32]);
            // With a new value of the half rate the outer numbers make one value of
            // twenty; between two the middle hands one on as it is, nine values late.
            let faster = if fresh {
                limit(outer(&back) >> 14)
            } else {
                back(FILTER / 4)
            };
            *out = (faster * i32::from(self.depth[side])) >> 15;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A kind with a work area of 64 values whose left side hands on what goes in,
    /// 16 values of sound later, at half its size, and whose right side is silent.
    fn echo() -> Kind {
        let mut words = [0; WORDS];
        (words[V_IIR], words[V_IN]) = (0x7FFF, 0x7FFF);
        (words[M_SAME], words[M_COMB[0]]) = (6, 4);
        words[V_COMB[0]] = 0x4000;
        Kind {
            start: (MEMORY_END - 16) as u16,
            mask: 0,
            words,
        }
    }

    #[test]
    fn a_kind_is_read_from_its_two_tables_and_a_number_too_high_is_none() {
        let mut image = vec![0u8; (ROWS - STARTS) as usize + KINDS * ROW_SIZE];
        image[4 * 4..][..4].copy_from_slice(&0xF204u32.to_le_bytes());
        let row = (ROWS - STARTS) as usize + 4 * ROW_SIZE;
        image[row..][..4].copy_from_slice(&7u32.to_le_bytes());
        image[row + 4..][..2].copy_from_slice(&0x00E3u16.to_le_bytes());
        image[row + 66..][..2].copy_from_slice(&0x8000u16.to_le_bytes());
        let kind = Kind::from_executable(&image, STARTS, 4).expect("in the image");
        let kind = kind.expect("a kind");
        assert_eq!((kind.start, kind.mask), (0xF204, 7));
        assert_eq!((kind.words[D_APF1], kind.words[V_IN + 1]), (0xE3, 0x8000));
        assert_eq!(kind.values(), 14_320);
        assert_eq!(Kind::from_executable(&image, STARTS, KINDS), Ok(None));
        let short = &image[..row];
        assert!(Kind::from_executable(short, STARTS, 4).is_err());
        assert!(Reverb::of_the_game(short, STARTS).is_err());
        let game = Reverb::of_the_game(&image, STARTS).expect("the game's kind");
        assert_eq!((game.area.len(), game.depth), (14_320, [12_900; 2]));
    }

    #[test]
    fn the_depth_word_is_the_share_of_127() {
        assert_eq!(depth(GAME_DEPTH), 12_900);
        assert_eq!((depth(0), depth(127), depth(300)), (0, 0x7FFF, 0x7FFF));
    }

    #[test]
    fn silence_stays_silence_and_no_depth_is_no_reverb() {
        let mut reverb = Reverb::new(&echo());
        reverb.set_depth([0x7FFF; 2]);
        assert!((0..500).all(|_| reverb.tick([0; 2]) == [0; 2]));
        let mut reverb = Reverb::new(&echo());
        assert!((0..500).all(|_| reverb.tick([20_000; 2]) == [0; 2]));
    }

    #[test]
    fn what_goes_in_comes_back_later_at_the_share_of_the_kind_and_the_depth() {
        let mut reverb = Reverb::new(&echo());
        reverb.set_depth([0x7FFF, 0x4000]);
        let out: Vec<[i32; 2]> = (0..400).map(|_| reverb.tick([16_000, 16_000])).collect();
        // Nothing at first, and no more than the filters' first small numbers until
        // the way through the work area, 16 values, and the outer half of each
        // filter are behind it.
        assert!(out[..18].iter().all(|&value| value == [0; 2]));
        assert!(out[..34].iter().all(|value| value[0].abs() <= 1));
        // Then half of what went in on the left, less what the shifts lose, and
        // nothing on the right, which has no way through this kind.
        assert!(out.iter().all(|value| value[1] == 0));
        assert!(
            out[200..]
                .iter()
                .all(|value| (7_990..=8_000).contains(&value[0]))
        );
        // A sum that leaves 16 bits goes in at the limit.
        let mut loud = Reverb::new(&echo());
        loud.set_depth([0x7FFF; 2]);
        let out: Vec<[i32; 2]> = (0..400).map(|_| loud.tick([90_000, 0])).collect();
        assert!(
            out[200..]
                .iter()
                .all(|value| (16_370..=16_384).contains(&value[0]))
        );
    }

    #[test]
    fn a_short_sound_dies_away_in_a_kind_that_sends_it_round() {
        // The echo with a wall that sends half of it back into its own place.
        let mut kind = echo();
        (kind.words[D_SAME], kind.words[V_WALL]) = (2, 0x4000);
        let mut reverb = Reverb::new(&kind);
        reverb.set_depth([0x7FFF; 2]);
        let mut loudest = Vec::new();
        for part in 0..6 {
            let came_in = if part == 0 { 16_000 } else { 0 };
            let heard = (0..200).map(|_| reverb.tick([came_in; 2])[0].abs()).max();
            loudest.push(heard.unwrap_or(0));
        }
        // Loudest while the first rounds lie on each other, then less each time.
        assert!(loudest[0] > 8_000 && loudest[1] > 8_000);
        assert!(loudest[1..].windows(2).all(|pair| pair[1] <= pair[0]));
        assert_eq!(loudest[4..], [0, 0]);
    }
}
