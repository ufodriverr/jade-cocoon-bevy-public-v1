
use std::fmt;

use crate::battle::fight::ended;
use crate::rooms::{self, Image, RoomTableError};

/// RAM address of the music table in the executable.
pub const MUSIC_TABLE: u32 = 0x8008_2E30;
/// Rows of the music table.
pub const MUSIC_COUNT: usize = 155;
/// Bytes of one row.
const ROW_SIZE: usize = 8;
/// Music numbers below this one are sequences of the file that stays in memory for the
/// whole game and are played at once.
pub const KEPT_BELOW: usize = 10;
/// The music number of the sequences of a fight.
pub const FIGHT_MUSIC: usize = 10;
/// Bytes of the four pairs of an offset and a size at the start of a sound file.
const HEAD_SIZE: usize = 32;
/// What the head of a sound bank starts with.
pub const BANK_MARK: [u8; 4] = *b"pBAV";
/// What a set of sequences starts with.
pub const SET_MARK: [u8; 4] = *b"pQES";

/// Why a piece of the sound could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundError {
    /// The music table leads outside the image. Holds the RAM address that was wanted.
    OutOfImage(u32),
    /// A sound file is shorter than its four pairs.
    Short(usize),
    /// A section leaves the file. Holds the number of the section, from 0.
    OutOfFile(usize),
    /// The head of a bank or a set of sequences does not start with its mark.
    NoMark,
    /// The head of a bank is shorter than its counts say, or its counts do not agree.
    Bank,
    /// A sequence leaves its set. Holds the offset of its head in the set.
    Sequence(usize),
    /// The events of a sequence cannot be read on. Holds the offset in the events.
    Event(usize),
}

impl fmt::Display for SoundError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfImage(address) => {
                write!(f, "music table leads outside the image, to {address:#010x}")
            }
            Self::Short(len) => write!(f, "a sound file of {len} bytes has no room for its head"),
            Self::OutOfFile(section) => write!(f, "section {section} of a sound file leaves it"),
            Self::NoMark => write!(f, "a bank or a set of sequences without its mark"),
            Self::Bank => write!(f, "the head of a bank does not fit its counts"),
            Self::Sequence(at) => write!(f, "the sequence at {at:#x} leaves its set"),
            Self::Event(at) => write!(f, "no event of a sequence at {at:#x}"),
        }
    }
}

impl std::error::Error for SoundError {}

impl From<RoomTableError> for SoundError {
    fn from(err: RoomTableError) -> Self {
        match err {
            RoomTableError::OutOfImage(address) => Self::OutOfImage(address),
        }
    }
}

/// One row of the music table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Music {
    /// The archive file the music is read from.
    pub file: usize,
    /// The sequence of the file's first set that is started, if the row names one.
    pub sequence: Option<usize>,
    /// How many sequences the file's first and second set hold.
    pub sets: [u8; 2],
    /// The music that is read along with this one, if the row names one.
    pub second: Option<usize>,
}

/// The music table: a row for every music number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicTable {
    rows: Vec<Music>,
}

impl MusicTable {
    /// Reads the table from the loaded executable.
    pub fn from_executable(image: &[u8], base: u32) -> Result<Self, SoundError> {
        let image = Image { data: image, base };
        let named = |value: i16| usize::try_from(value).ok();
        let rows = (0..MUSIC_COUNT)
            .map(|number| {
                let row = rooms::step(MUSIC_TABLE, number, ROW_SIZE)?;
                Ok(Music {
                    file: usize::from(image.u16(row)?),
                    sequence: named(image.i16(row + 2)?),
                    sets: [image.u8(row + 4)?, image.u8(row + 5)?],
                    second: named(image.i16(row + 6)?),
                })
            })
            .collect::<Result<_, SoundError>>()?;
        Ok(Self { rows })
    }

    /// The row of a music number.
    pub fn get(&self, number: usize) -> Option<&Music> {
        self.rows.get(number)
    }

    /// The rows in the order of their numbers.
    pub fn iter(&self) -> impl Iterator<Item = &Music> {
        self.rows.iter()
    }

    /// How many rows the table has.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether the table has no row.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

pub const EFFECT_TABLE: u32 = 0x8007_C674;
/// Records of the effect table.
pub const EFFECTS: usize = 110;
const EFFECT_SIZE: usize = 8;
/// The two lists behind the table: the rows of a fight's sequences that take the place
/// of the rows 2 and 3, five half words each, picked by the player's level.
const ROWS_BY_LEVEL: [(usize, u32); 2] = [(2, 0x8007_C9E4), (3, 0x8007_C9F0)];
const LEVEL_ROWS: usize = 5;
/// The level is divided by this and held at the last row of a list.
const LEVEL_STEP: u8 = 4;

/// What a record of the effect table asks of the sound, by its way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Way 0: a sound by its number (`0x800314F0`).
    Sound(u32),
    /// Way 1: a music is asked for (`0x8003013C` with 1).
    Music(usize),
    /// Way 2: a row of a fight's sequences is started (`0x800311BC` with 1).
    Row(usize),
    /// Way 3: the running sequence of the record that stays is faded and forgotten.
    FadeKept,
    /// Way 4, any other way, and a number a way cannot take: nothing.
    Nothing,
}

/// The effect table and its two lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectTable {
    effects: Vec<Effect>,
    by_level: [(usize, [usize; LEVEL_ROWS]); 2],
}

impl EffectTable {
    /// Reads the table and the lists from the loaded executable.
    pub fn from_executable(image: &[u8], base: u32) -> Result<Self, SoundError> {
        let image = Image { data: image, base };
        let effects = (0..EFFECTS)
            .map(|number| {
                let record = rooms::step(EFFECT_TABLE, number, EFFECT_SIZE)?;
                let number = image.i16(record + 4)?;
                let index = usize::try_from(number).ok();
                Ok(match (image.u32(record)?, index) {
                    (0, Some(sound)) => Effect::Sound(sound as u32),
                    (1, Some(music)) => Effect::Music(music),
                    (2, Some(row)) => Effect::Row(row),
                    (3, _) => Effect::FadeKept,
                    _ => Effect::Nothing,
                })
            })
            .collect::<Result<_, SoundError>>()?;
        let mut by_level = [(0, [0; LEVEL_ROWS]); 2];
        for (list, (row, address)) in by_level.iter_mut().zip(ROWS_BY_LEVEL) {
            list.0 = row;
            for (index, entry) in list.1.iter_mut().enumerate() {
                *entry = usize::from(image.u16(rooms::step(address, index, 2)?)?);
            }
        }
        Ok(Self { effects, by_level })
    }

    /// What effect `number` asks for while the player's level (the byte `0x8008BD0C`)
    /// is `level`: the record's way and number, with the rows 2 and 3 of a fight's
    /// sequences replaced by the row their list has for the level divided by 4, the
    /// last one from 16 on.
    /// Nothing for a number outside the table.
    pub fn get(&self, number: u16, level: u8) -> Effect {
        match self.effects.get(usize::from(number)) {
            Some(&Effect::Row(row)) => {
                let index = usize::from(level / LEVEL_STEP).min(LEVEL_ROWS - 1);
                let listed = self.by_level.iter().find(|(named, _)| *named == row);
                Effect::Row(listed.map_or(row, |(_, rows)| rows[index]))
            }
            Some(&effect) => effect,
            None => Effect::Nothing,
        }
    }
}

/// The sections of a sound file. A section of size 0 is not there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoundFile<'a> {
    /// The head of the sound bank.
    pub bank_head: Option<&'a [u8]>,
    /// The body of the sound bank.
    pub bank_body: Option<&'a [u8]>,
    /// The first and the second set of sequences.
    pub sets: [Option<&'a [u8]>; 2],
}

impl<'a> SoundFile<'a> {
    /// Reads the four pairs at the start of an archive file of the music table.
    pub fn parse(file: &'a [u8]) -> Result<Self, SoundError> {
        let head = file.get(..HEAD_SIZE).ok_or(SoundError::Short(file.len()))?;
        let word = |index: usize| {
            let bytes = head[4 * index..4 * index + 4]
                .try_into()
                .expect("four bytes");
            u32::from_le_bytes(bytes) as usize
        };
        let section = |number: usize| {
            let (offset, size) = (word(2 * number), word(2 * number + 1));
            if size == 0 {
                return Ok(None);
            }
            let end = offset.checked_add(size);
            let bytes = end.and_then(|end| file.get(offset..end));
            bytes.map(Some).ok_or(SoundError::OutOfFile(number))
        };
        Ok(Self {
            bank_head: section(0)?,
            bank_body: section(1)?,
            sets: [section(2)?, section(3)?],
        })
    }
}

/// Bytes of the counts at the start of the head of a bank.
const BANK_HEAD: usize = 32;
/// Program records in the head of a bank, used or not.
pub const PROGRAMS: usize = 128;
/// Bytes of one program record.
const PROGRAM_SIZE: usize = 16;
/// Tone records of every program in use, filled or not.
pub const TONES: usize = 16;
/// Bytes of one tone record.
const TONE_SIZE: usize = 32;
/// Half words of the size table at the end of the head. The first is no sample.
const SAMPLE_SIZES: usize = 256;
/// A size of the table counts this many bytes.
const SIZE_UNIT: usize = 8;
/// Bytes of one block of a sample.
pub const BLOCK_SIZE: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tone {
    pub priority: u8,
    pub mode: u8,
    pub volume: u8,
    pub pan: u8,
    /// The key the sample sounds at as it was recorded, and the fine step above it.
    pub centre: u8,
    pub shift: u8,
    /// The lowest and the highest key the tone answers.
    pub keys: [u8; 2],
    /// The bend range down and up.
    pub bend: [u8; 2],
    /// The two envelope words.
    pub envelope: [u16; 2],
    /// The sample, counted from 1.
    pub sample: usize,
}

/// One program of a bank.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    /// The number a sequence asks for.
    pub number: u8,
    pub volume: u8,
    pub priority: u8,
    pub mode: u8,
    pub pan: u8,
    pub tones: Vec<Tone>,
}

/// The head of a sound bank: its programs and where each sample lies in the body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bank {
    pub volume: u8,
    pub pan: u8,
    programs: Vec<Program>,
    /// The volume and the pan of every program record, with tones or without.
    records: Vec<[u8; 2]>,
    /// Where sample n + 1 starts in the body; one more entry for the end of the last.
    starts: Vec<usize>,
}

impl Bank {
    /// Reads the head of a bank.
    pub fn parse(head: &[u8]) -> Result<Self, SoundError> {
        if !head.starts_with(&BANK_MARK) {
            return Err(SoundError::NoMark);
        }
        let half = |at: usize| -> Result<u16, SoundError> {
            let bytes = head.get(at..at + 2).ok_or(SoundError::Bank)?;
            Ok(u16::from_le_bytes(bytes.try_into().expect("two bytes")))
        };
        let (used, samples) = (usize::from(half(18)?), usize::from(half(22)?));
        let tones_at = BANK_HEAD + PROGRAMS * PROGRAM_SIZE;
        let sizes_at = tones_at + used * TONES * TONE_SIZE;
        if head.len() < sizes_at + 2 * SAMPLE_SIZES || samples >= SAMPLE_SIZES {
            return Err(SoundError::Bank);
        }
        let mut programs = Vec::with_capacity(used);
        let mut records = Vec::with_capacity(PROGRAMS);
        for number in 0..PROGRAMS {
            let record = &head[BANK_HEAD + number * PROGRAM_SIZE..][..PROGRAM_SIZE];
            records.push([record[1], record[4]]);
            let count = usize::from(record[0]);
            if count == 0 {
                continue;
            }
            // The tone records lie in the order of the programs in use.
            let slot = programs.len();
            if slot == used || count > TONES {
                return Err(SoundError::Bank);
            }
            let tones = (0..count)
                .map(|tone| {
                    let at = tones_at + (slot * TONES + tone) * TONE_SIZE;
                    let t = &head[at..at + TONE_SIZE];
                    Ok(Tone {
                        priority: t[0],
                        mode: t[1],
                        volume: t[2],
                        pan: t[3],
                        centre: t[4],
                        shift: t[5],
                        keys: [t[6], t[7]],
                        bend: [t[12], t[13]],
                        envelope: [half(at + 16)?, half(at + 18)?],
                        sample: usize::from(half(at + 22)?),
                    })
                })
                .collect::<Result<Vec<_>, SoundError>>()?;
            if tones
                .iter()
                .any(|tone| !(1..=samples).contains(&tone.sample))
            {
                return Err(SoundError::Bank);
            }
            programs.push(Program {
                number: number as u8,
                volume: record[1],
                priority: record[2],
                mode: record[3],
                pan: record[4],
                tones,
            });
        }
        if programs.len() != used {
            return Err(SoundError::Bank);
        }
        let mut starts = vec![0];
        for sample in 1..=samples {
            let size = usize::from(half(sizes_at + 2 * sample)?) * SIZE_UNIT;
            starts.push(starts[sample - 1] + size);
        }
        Ok(Self {
            volume: head[24],
            pan: head[25],
            programs,
            records,
            starts,
        })
    }

    /// The programs in use, in the order of their numbers.
    pub fn programs(&self) -> &[Program] {
        &self.programs
    }

    /// The program a sequence asks for by number.
    pub fn program(&self, number: u8) -> Option<&Program> {
        self.programs
            .iter()
            .find(|program| program.number == number)
    }

    pub fn one_tone(&self, number: u8, index: usize) -> Option<(u8, u8, &Tone)> {
        let [volume, pan] = *self.records.get(usize::from(number))?;
        let below = self.programs.iter().filter(|p| p.number < number);
        let read = self.programs.get(below.count())?;
        Some((volume, pan, read.tones.get(index)?))
    }

    /// How many samples the body holds.
    pub fn samples(&self) -> usize {
        self.starts.len() - 1
    }

    /// How many bytes the samples take: the size of the body.
    pub fn body_size(&self) -> usize {
        self.starts[self.samples()]
    }

    /// The blocks of a sample, counted from 1 as the tones name it.
    pub fn sample<'a>(&self, body: &'a [u8], number: usize) -> Option<&'a [u8]> {
        let start = *self.starts.get(number.checked_sub(1)?)?;
        body.get(start..*self.starts.get(number)?)
    }
}

/// Samples of sound in one block of a sample.
pub const BLOCK_SOUND: usize = 28;
/// The flag of the last block that is played.
pub const BLOCK_ENDS: u8 = 1;
/// With [`BLOCK_ENDS`]: the sound goes on at the loop's start. Without it, it is over.
pub const BLOCK_GOES_BACK: u8 = 2;
/// The flag of the block a loop starts with.
pub const BLOCK_STARTS_LOOP: u8 = 4;
/// Share of the value before and of the one before that in a value, in 64ths, by filter.
const BLOCK_FILTERS: [(i32, i32); 5] = [(0, 0), (60, 0), (115, -52), (98, -55), (122, -60)];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BlockDecoder {
    /// The value before and the one before that.
    before: [i32; 2],
    limited: u64,
}

impl BlockDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// How many values so far lay outside 16 bits and were set to the nearest end.
    pub fn limited(&self) -> u64 {
        self.limited
    }

    /// The sound of one block.
    pub fn decode(&mut self, block: &[u8; BLOCK_SIZE]) -> [i16; BLOCK_SOUND] {
        // No block on the disc names a shift above 12 or a filter above 4; by the public
        // descriptions the chip takes 9 for such a shift.
        let shift = match u32::from(block[0] & 0x0F) {
            shift @ 0..=12 => shift,
            _ => 9,
        };
        let (share, share_before) = BLOCK_FILTERS[usize::from(block[0] >> 4).min(4)];
        let [mut old, mut older] = self.before;
        let mut sound = [0; BLOCK_SOUND];
        for (index, value) in sound.iter_mut().enumerate() {
            // Four bits a value, the low half of a byte first.
            let byte = block[2 + index / 2];
            let bits = if index & 1 == 0 {
                byte << 4
            } else {
                byte & 0xF0
            };
            let wanted = ((i32::from(bits as i8) << 8) >> shift)
                + ((old * share + older * share_before + 32) >> 6);
            let limited = wanted.clamp(i32::from(i16::MIN), i32::from(i16::MAX));
            self.limited += u64::from(limited != wanted);
            (old, older) = (limited, old);
            *value = limited as i16;
        }
        self.before = [old, older];
        sound
    }
}

/// A sample played through once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sound {
    /// The sound up to the end of the block that ends the sample.
    pub values: Vec<i16>,
    /// Where the sound goes on behind its last value, if the sample goes round: the
    /// first value of the block the loop starts with. A player that goes round makes the
    /// blocks again from there with the decoder it has, since the values before carry on.
    pub back: Option<usize>,
    /// How many blocks were played.
    pub blocks: usize,
    /// How many values were limited to 16 bits.
    pub limited: u64,
}

/// Plays the blocks of a sample ([`Bank::sample`]) up to the block that ends it. A
/// sample without such a block is played to its last block and does not go round.
pub fn sound(sample: &[u8]) -> Sound {
    let mut decoder = BlockDecoder::new();
    let (mut values, mut start, mut back, mut blocks) = (Vec::new(), 0, None, 0);
    for block in sample.as_chunks::<BLOCK_SIZE>().0 {
        if block[1] & BLOCK_STARTS_LOOP != 0 {
            start = values.len();
        }
        values.extend_from_slice(&decoder.decode(block));
        blocks += 1;
        if block[1] & BLOCK_ENDS != 0 {
            back = (block[1] & BLOCK_GOES_BACK != 0).then_some(start);
            break;
        }
    }
    Sound {
        values,
        back,
        blocks,
        limited: decoder.limited(),
    }
}

/// Bytes in front of the first sequence of a set: the mark and a half word.
const SET_HEAD: usize = 6;
/// Bytes in front of the events of a sequence.
const SEQUENCE_HEAD: usize = 13;
/// A section is padded with zeros to a whole word.
const PADDING: usize = 4;

/// The control that marks a loop: value [`LOOP_START`] where it begins and
/// [`LOOP_END`] where the sequence goes back there.
pub const LOOP_CONTROL: u8 = 0x63;
pub const LOOP_START: u8 = 20;
pub const LOOP_END: u8 = 30;
/// The control whose value, behind a loop's start, is how often the loop is gone
/// through; [`LOOP_FOR_EVER`] and above: without end.
pub const LOOP_COUNT_CONTROL: u8 = 6;
pub const LOOP_FOR_EVER: u8 = 127;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sequence<'a> {
    /// Its number in the set.
    pub number: u16,
    /// Ticks of a quarter note.
    pub ticks: u16,
    /// Microseconds of a quarter note at the start.
    pub tempo: u32,
    /// The two bytes of the bar.
    pub bar: [u8; 2],
    /// The events.
    pub data: &'a [u8],
}

pub const STEPS_A_SECOND: u32 = 60;

/// The quarter notes in a minute the library takes for a count of microseconds a
/// quarter note: rounded to the nearest whole number. `None` for a count of 0.
pub fn beats_a_minute(tempo: u32) -> Option<u32> {
    let (beats, rest) = (60_000_000u32.checked_div(tempo)?, 60_000_000 % tempo);
    Some(beats + u32::from(rest > tempo / 2))
}

/// The quarter notes in a minute the library takes for the count of a tempo event: cut
/// off, not rounded as for the head of a sequence. `None` for a count of 0.
pub fn beats_of_an_event(tempo: u32) -> Option<u32> {
    60_000_000u32.checked_div(tempo)
}

/// The library keeps the wait in front of an event as this many times its ticks, and
/// [`ticks_a_step`] is in that unit.
pub const WAIT_UNITS: u32 = 10;

/// How far a sequence goes on with each step of the library, **in tenths of a tick**
/// ([`WAIT_UNITS`]), from the ticks of a quarter note and the quarter notes in a minute
/// as it is opened: rounded to the nearest whole number. `None` for a sequence so slow
/// that it would go on by less than a tenth: the library counts steps for a tick then,
/// and no sequence of the disc is like that.
pub fn ticks_a_step(ticks: u16, beats: u32) -> Option<u32> {
    let a_minute = STEPS_A_SECOND * 60;
    let wanted = u32::from(ticks)
        .checked_mul(beats)?
        .checked_mul(WAIT_UNITS)?;
    if wanted < a_minute {
        return None;
    }
    Some(wanted / a_minute + u32::from(wanted % a_minute > a_minute / 2))
}

impl Sequence<'_> {
    /// The ticks the sequence goes on with each step, by the tempo of its head.
    pub fn ticks_a_step(&self) -> Option<u32> {
        ticks_a_step(self.ticks, beats_a_minute(self.tempo)?)
    }
}

pub const PITCH_AS_RECORDED: u16 = 0x1000;
/// The highest pitch word the library asks for: two octaves above the centre key and
/// anything higher.
pub const PITCH_LIMIT: u16 = 0x3FFF;
/// Fine steps of a half tone.
const FINE_STEPS: i32 = 128;
/// Half tones of an octave.
const HALF_TONES: i32 = 12;
/// A tone's shift above this is taken for this when a key is pressed.
const SHIFT_LIMIT: u8 = 127;
/// A bend of this value leaves the key as it is.
pub const NO_BEND: u8 = 64;
/// The loudest a side of a voice can be.
pub const VOLUME_LIMIT: u16 = 0x3FFF;

/// Entry `step` of a pitch table of the library: 2 to the power of `step / steps`, in
/// units of 1/32768 and cut off.
fn power(step: i32, steps: i32) -> u32 {
    (32768.0 * 2f64.powf(f64::from(step) / f64::from(steps))) as u32
}

/// The library's table of the twelve half tones of an octave, entry `step`.
pub fn half_tone(step: u8) -> u16 {
    power(i32::from(step), HALF_TONES) as u16
}

/// The library's table of the 128 fine steps of a half tone, entry `step`.
pub fn fine_step(step: u8) -> u16 {
    power(i32::from(step), HALF_TONES * FINE_STEPS) as u16
}

/// The pitch word of a key that is `fine` steps of 128 above itself, for a tone with
/// that centre key and shift. The octave of the centre key is looked up, lower ones
/// are halved with rounding, and from two octaves up it is the limit.
pub fn pitch(key: i32, fine: i32, centre: u8, shift: u8) -> u16 {
    let fine = fine + i32::from(shift);
    let whole = key - i32::from(centre) + fine.div_euclid(FINE_STEPS);
    let octave = whole.div_euclid(HALF_TONES) - 2;
    if octave >= 0 {
        return PITCH_LIMIT;
    }
    let product = power(whole.rem_euclid(HALF_TONES), HALF_TONES)
        * power(fine.rem_euclid(FINE_STEPS), HALF_TONES * FINE_STEPS);
    let base = product >> 16;
    let down = octave.unsigned_abs();
    (base + (1 << (down - 1)))
        .checked_shr(down)
        .map_or(0, |word| word as u16)
}

/// The key and the fine steps a bend makes of a key, by the second byte of the bend
/// event and the tone's bend range down and up. Up, the whole range is 63 steps of
/// the byte; down it is 64, and the key is always taken one lower with the fine
/// steps counted up from there, so that even a range of 0 lowers the note by a step.
pub fn bent(key: u8, bend: u8, range: [u8; 2]) -> (i32, i32) {
    let (key, by) = (i32::from(key), i32::from(bend) - i32::from(NO_BEND));
    match by.cmp(&0) {
        std::cmp::Ordering::Greater => {
            let all = by * i32::from(range[1]);
            (key + all / 63, all % 63 * 2)
        }
        std::cmp::Ordering::Less => {
            let all = by * i32::from(range[0]);
            (key + all / 64 - 1, all % 64 * 2 + FINE_STEPS - 1)
        }
        std::cmp::Ordering::Equal => (key, 0),
    }
}

impl Tone {
    /// Whether the tone answers a key.
    pub fn answers(&self, key: u8) -> bool {
        (self.keys[0]..=self.keys[1]).contains(&key)
    }

    /// The pitch word of a key as it is pressed.
    pub fn pitch(&self, key: u8) -> u16 {
        pitch(i32::from(key), 0, self.centre, self.shift.min(SHIFT_LIMIT))
    }

    /// The pitch word of a sounding key under a bend. The shift is not limited here.
    pub fn bent_pitch(&self, key: u8, bend: u8) -> u16 {
        let (key, fine) = bent(key, bend, self.bend);
        pitch(key, fine, self.centre, self.shift)
    }
}

pub fn music_volume(setting: u32) -> u16 {
    (setting >> 7 & 127) as u16
}

/// The volume the game gives every other sequence and its sounds: 64 in a new game.
pub fn sound_volume(setting: u32) -> u16 {
    (setting >> 14 & 127) as u16
}

/// Whether the setting is one loudspeaker.
pub fn one_loudspeaker(setting: u32) -> bool {
    setting & 1 << 6 != 0
}

pub fn room_music(music: i16, variable: i32) -> Option<usize> {
    let music = usize::try_from(music).ok()?;
    Some(match (variable, music) {
        (1, 13) => 15,
        (2, 13) => 145,
        (3, 41) => 38,
        (4, 42) => 37,
        _ => music,
    })
}

/// The rows of the table of a fight's sequences, and the two the fight's flow names:
/// the one an ordinary fight starts in its intro and the one a fight that is won
/// starts. HYPOTHESIS for what they are: the music of a fight and of a victory.
pub const FIGHT_ROWS: usize = 32;
pub const FIGHT_ROW: usize = 0;
pub const WON_ROW: usize = 1;

pub const HIT_SOUND: u32 = 114;
pub const CRITICAL_SOUND: u32 = 113;
pub const MISS_SOUND: u32 = 112;
pub const FALL_SOUND: u32 = 111;
pub const FALL_ROW: usize = 28;
pub const FLIGHT_ROW: usize = 28;
/// The player's use of an item in a fight (`0x800ABFC0` of file 27): the sound on the
/// first frame of the action's first pair, and the row started ten frames before it.
pub const ITEM_SOUND: u32 = 68;
pub const ITEM_ROW: usize = 29;
/// The row the handler of a change starts when a carried creature goes for another
/// (`0x800BBE14` of file 36); the player's leave starts the same row by an event of
/// his animation.
pub const CHANGE_ROW: usize = 17;

/// Whether an opponent of this species falls with the row in place of the sound
/// (`0x800BFB84` of file 36, which sets battle flag `0x100000`).
pub fn falls_with_the_row(species: u32) -> bool {
    matches!(species, 41 | 44 | 46 | 48..=51 | 55)
}

pub fn fight_row(row: usize) -> Option<(usize, usize)> {
    (row < FIGHT_ROWS).then_some((row / 16, row % 16))
}

/// Whether a row of a fight's sequences is started at the volume of the music; the
/// others are started at the volume of the sounds.
pub fn fight_row_is_music(row: usize) -> bool {
    row < 2
}

/// The music of a fight whose opponent 0 is of a boss species.
pub fn boss_music(species: u32) -> usize {
    match species {
        55 => 146,
        50 => 35,
        _ => 36,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FightAsks {
    /// `0x80030DC4` on the music record: every sequence of it is stopped and made
    /// silent, and the record has no running one. It keeps its music.
    Silence,
    /// The same on the record that stays in memory.
    SilenceKept,
    /// `0x8003013C`: a music is asked for.
    Music(usize),
    /// `0x80031650`: the music record and the second record are given back.
    GiveBack,
    /// `0x800311BC(row, 1)`, `0x80031390(row)` and `0x80031424(row)`: a row of the
    /// fight's sequences is started, stopped and made silent, or faded.
    Start(usize),
    Stop(usize),
    Fade(usize),
    /// The fade of the music record's running sequence, which the record then has no
    /// more: nothing without one.
    FadeMusic,
    /// `0x800306C0`: the fight's sequences are stopped and their sets closed.
    Close,
    /// The library's `0x80066D5C`: every voice that is not kept out is made silent at
    /// once and let go.
    AllOff,
    /// `0x8003013C` with the answer of `0x80031640`: the music that was noted last.
    Noted,
}

/// The first state of the field's fight task, the game frame behind the fight's
/// command: the room's music is made silent and music 0 is asked for.
pub fn fight_starts() -> Vec<FightAsks> {
    vec![FightAsks::Silence, FightAsks::Music(0)]
}

/// Step 1 of the director's intro, by the species of opponent 0 and whether it is a
/// boss species: a boss has a music of its own; for any other fight the room's music
/// is given back and the fight's first row is started.
pub fn fight_music(species: u32, boss: bool) -> Vec<FightAsks> {
    match boss {
        true => vec![FightAsks::Music(boss_music(species))],
        false => vec![FightAsks::GiveBack, FightAsks::Start(FIGHT_ROW)],
    }
}

/// The fade of a fight's music: the music record's for a boss, the first row's
/// otherwise.
fn fight_fade(boss: bool) -> FightAsks {
    match boss {
        true => FightAsks::FadeMusic,
        false => FightAsks::Fade(FIGHT_ROW),
    }
}

/// The case of the outcome behind the director (`0x800BA424`), in the game frame the
/// player is sent on his way back.
pub fn fight_ends(outcome: u32, boss: bool) -> Vec<FightAsks> {
    match outcome {
        ended::WON => vec![
            FightAsks::SilenceKept,
            FightAsks::FadeMusic,
            FightAsks::Stop(FIGHT_ROW),
            FightAsks::Start(WON_ROW),
        ],
        ended::RAN | ended::TAUGHT | ended::REFUSED => vec![fight_fade(boss)],
        _ => Vec::new(),
    }
}

/// The last pass of the state that waits behind that case: the outcomes whose case
/// fades nothing fade the fight's music here. Behind a fight that is won the first row
/// is stopped by then, so the fade does nothing to it.
pub fn fight_paused(outcome: u32, boss: bool) -> Vec<FightAsks> {
    match outcome {
        ended::WON | ended::EMPTY | ended::LOST => vec![fight_fade(boss)],
        _ => Vec::new(),
    }
}

/// The tear-down of a fight (`0x800BA958`).
pub fn fight_torn_down() -> Vec<FightAsks> {
    vec![
        FightAsks::SilenceKept,
        FightAsks::Stop(FIGHT_ROW),
        FightAsks::Close,
        FightAsks::AllOff,
        FightAsks::Noted,
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Levels {
    /// The loudness of the key.
    pub loudness: u8,
    /// The volumes of the bank, the program and the tone.
    pub bank: u8,
    pub program: u8,
    pub tone: u8,
    /// The pans of the tone, the program and the channel: 64 is the middle.
    pub pans: [u8; 3],
    /// The volume of the channel and the left and right volume of the sequence. `None`
    /// for a sound that is played without a sequence.
    pub sequence: Option<(u8, [u16; 2])>,
    /// Whether the setting is one loudspeaker.
    pub mono: bool,
}

/// The left and the right volume of a voice, as the library works them out: every
/// division cut off, in this order.
pub fn volumes(levels: &Levels) -> [u16; 2] {
    let mut loudness = u32::from(levels.loudness);
    if let Some((channel, _)) = levels.sequence {
        loudness = loudness * u32::from(channel) / 127;
    }
    let all = loudness * u32::from(levels.bank) * 16383 / 16129;
    let all = all * u32::from(levels.program) * u32::from(levels.tone) / 16129;
    let [mut left, mut right] = match levels.sequence {
        Some((_, sides)) => sides.map(|side| all * u32::from(side) / 127),
        None => [all; 2],
    };
    for pan in levels.pans.map(u32::from) {
        if pan < 64 {
            right = right * pan / 63;
        } else {
            left = left * (127 - pan) / 63;
        }
    }
    if levels.mono {
        (left, right) = (left.max(right), left.max(right));
    }
    if levels.sequence.is_some() {
        let squared = |side: u32| side.wrapping_mul(side) / 16383;
        (left, right) = (squared(left), squared(right));
    }
    [left as u16, right as u16]
}

/// Reads every sequence of a set.
pub fn sequences(set: &[u8]) -> Result<Vec<Sequence<'_>>, SoundError> {
    if !set.starts_with(&SET_MARK) {
        return Err(SoundError::NoMark);
    }
    let mut found = Vec::new();
    let mut at = SET_HEAD;
    loop {
        let rest = set.get(at..).unwrap_or_default();
        if rest.len() < PADDING && rest.iter().all(|&byte| byte == 0) {
            break;
        }
        let head = set
            .get(at..at + SEQUENCE_HEAD)
            .ok_or(SoundError::Sequence(at))?;
        let size = u32::from_be_bytes(head[9..13].try_into().expect("four bytes")) as usize;
        let start = at + SEQUENCE_HEAD;
        let data = start
            .checked_add(size)
            .and_then(|end| set.get(start..end))
            .ok_or(SoundError::Sequence(at))?;
        found.push(Sequence {
            number: u16::from_be_bytes([head[0], head[1]]),
            ticks: u16::from_be_bytes([head[2], head[3]]),
            tempo: u32::from_be_bytes([0, head[4], head[5], head[6]]),
            bar: [head[7], head[8]],
            data,
        });
        at = start + size;
    }
    Ok(found)
}

/// What an event of a sequence asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asked {
    /// A key is pressed, or let go with loudness 0.
    Note {
        channel: u8,
        key: u8,
        loudness: u8,
    },
    Control {
        channel: u8,
        control: u8,
        value: u8,
    },
    Program {
        channel: u8,
        program: u8,
    },
    /// The two bytes of a bend, as they stand.
    Bend {
        channel: u8,
        bytes: [u8; 2],
    },
    /// Microseconds of a quarter note from here on.
    Tempo(u32),
    End,
}

/// One event of a sequence: what is asked for, and how many ticks behind the event in
/// front of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
    pub wait: u32,
    pub asked: Asked,
}

impl<'a> Sequence<'a> {
    /// The events in order, the end event last. A byte that is none of the events the
    /// disc has is an error with its offset, and so is a sequence without its end.
    pub fn events(&self) -> Events<'a> {
        Events {
            data: self.data,
            at: 0,
            running: None,
            done: false,
        }
    }
}

/// The walk over the events of a sequence.
#[derive(Debug, Clone)]
pub struct Events<'a> {
    data: &'a [u8],
    at: usize,
    running: Option<u8>,
    done: bool,
}

impl Events<'_> {
    fn byte(&mut self) -> Result<u8, SoundError> {
        let byte = self.data.get(self.at).ok_or(SoundError::Event(self.at))?;
        self.at += 1;
        Ok(*byte)
    }

    fn event(&mut self) -> Result<Event, SoundError> {
        // The wait: seven bits a byte, the high bit says that another byte follows.
        let mut wait = 0u32;
        loop {
            let byte = self.byte()?;
            wait = wait << 7 | u32::from(byte & 0x7F);
            if byte & 0x80 == 0 {
                break;
            }
        }
        // A byte without the high bit here belongs to the status of the event in front.
        let start = self.at;
        let mut status = self.byte()?;
        if status & 0x80 == 0 {
            self.at = start;
            status = self.running.ok_or(SoundError::Event(start))?;
        } else if status != 0xFF {
            self.running = Some(status);
        }
        let channel = status & 0x0F;
        let asked = match status & 0xF0 {
            0x90 => Asked::Note {
                channel,
                key: self.byte()?,
                loudness: self.byte()?,
            },
            0xB0 => Asked::Control {
                channel,
                control: self.byte()?,
                value: self.byte()?,
            },
            0xC0 => Asked::Program {
                channel,
                program: self.byte()?,
            },
            0xE0 => Asked::Bend {
                channel,
                bytes: [self.byte()?, self.byte()?],
            },
            _ if status == 0xFF => match self.byte()? {
                0x51 => Asked::Tempo(u32::from_be_bytes([
                    0,
                    self.byte()?,
                    self.byte()?,
                    self.byte()?,
                ])),
                0x2F => {
                    self.byte()?;
                    Asked::End
                }
                _ => return Err(SoundError::Event(start)),
            },
            _ => return Err(SoundError::Event(start)),
        };
        Ok(Event { wait, asked })
    }
}

impl Iterator for Events<'_> {
    type Item = Result<Event, SoundError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let event = self.event();
        self.done = !matches!(event, Ok(Event { asked, .. }) if asked != Asked::End);
        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A table of [`MUSIC_COUNT`] rows that all name `file`, with the rows given changed.
    fn table(file: u16, rows: &[(usize, [u8; ROW_SIZE])]) -> Vec<u8> {
        let mut row = [0; ROW_SIZE];
        row[..2].copy_from_slice(&file.to_le_bytes());
        row[2..4].copy_from_slice(&(-1i16).to_le_bytes());
        row[6..8].copy_from_slice(&(-1i16).to_le_bytes());
        let mut image = row.repeat(MUSIC_COUNT);
        for (number, bytes) in rows {
            image[number * ROW_SIZE..][..ROW_SIZE].copy_from_slice(bytes);
        }
        image
    }

    #[test]
    fn an_effect_is_a_way_and_a_number_and_two_rows_change_with_a_byte() {
        // The table and, right behind it, the two lists of five half words and a gap.
        let mut image = vec![0u8; EFFECTS * EFFECT_SIZE + 24];
        let mut record = |number: usize, way: u32, value: i16| {
            let at = number * EFFECT_SIZE;
            image[at..at + 4].copy_from_slice(&way.to_le_bytes());
            image[at + 4..at + 6].copy_from_slice(&value.to_le_bytes());
        };
        record(0, 0, 123);
        record(1, 1, 3);
        record(2, 2, 2);
        record(3, 2, 3);
        record(4, 2, 17);
        record(5, 3, 0);
        record(6, 4, 24);
        record(7, 0, -1);
        record(8, 9, 5);
        let lists: [[u16; 5]; 2] = [[2, 24, 25, 26, 27], [3, 4, 5, 6, 7]];
        for (list, at) in lists
            .iter()
            .zip([EFFECTS * EFFECT_SIZE, EFFECTS * EFFECT_SIZE + 12])
        {
            for (index, row) in list.iter().enumerate() {
                image[at + 2 * index..][..2].copy_from_slice(&row.to_le_bytes());
            }
        }
        assert_eq!(
            ROWS_BY_LEVEL[0].1,
            EFFECT_TABLE + (EFFECTS * EFFECT_SIZE) as u32
        );
        let table = EffectTable::from_executable(&image, EFFECT_TABLE).expect("table");
        assert_eq!(table.get(0, 0), Effect::Sound(123));
        assert_eq!(table.get(1, 0), Effect::Music(3));
        assert_eq!(table.get(5, 0), Effect::FadeKept);
        // Way 4, a negative number, a way nobody acts on and a number outside.
        assert_eq!(table.get(6, 0), Effect::Nothing);
        assert_eq!(table.get(7, 0), Effect::Nothing);
        assert_eq!(table.get(8, 0), Effect::Nothing);
        assert_eq!(table.get(EFFECTS as u16, 0), Effect::Nothing);
        // The rows 2 and 3 by the byte: a row for every four, the last from 16 on.
        let rows = |number| [0, 3, 4, 8, 15, 16, 255].map(|level| table.get(number, level));
        let of = |rows: [usize; 7]| rows.map(Effect::Row);
        assert_eq!(rows(2), of([2, 2, 24, 25, 26, 27, 27]));
        assert_eq!(rows(3), of([3, 3, 4, 5, 6, 7, 7]));
        assert_eq!(rows(4), of([17; 7]));
        assert_eq!(
            EffectTable::from_executable(&image[..image.len() - 12], EFFECT_TABLE),
            Err(SoundError::OutOfImage(ROWS_BY_LEVEL[1].1))
        );
    }

    /// A sound file with the four sections given, one behind the other.
    fn file(sections: [&[u8]; 4]) -> Vec<u8> {
        let mut bytes = vec![0; HEAD_SIZE];
        for (number, section) in sections.iter().enumerate() {
            let pair = [bytes.len() as u32, section.len() as u32];
            bytes[8 * number..][..4].copy_from_slice(&pair[0].to_le_bytes());
            bytes[8 * number + 4..][..4].copy_from_slice(&pair[1].to_le_bytes());
            bytes.extend_from_slice(section);
        }
        bytes
    }

    #[test]
    fn a_row_names_its_file_its_sequence_its_sets_and_its_second_music() {
        let image = table(700, &[(3, [0x21, 0x03, 2, 0, 5, 16, 44, 0])]);
        let table = MusicTable::from_executable(&image, MUSIC_TABLE).expect("table");
        assert_eq!(table.len(), MUSIC_COUNT);
        assert!(!table.is_empty());
        let plain = Music {
            file: 700,
            sequence: None,
            sets: [0, 0],
            second: None,
        };
        assert_eq!(table.get(0), Some(&plain));
        let named = Music {
            file: 0x321,
            sequence: Some(2),
            sets: [5, 16],
            second: Some(44),
        };
        assert_eq!(table.get(3), Some(&named));
        assert_eq!(table.get(MUSIC_COUNT), None);
        assert_eq!(table.iter().filter(|music| music.file == 700).count(), 154);
    }

    #[test]
    fn a_table_that_leaves_the_image_is_an_error() {
        let mut image = table(700, &[]);
        image.pop();
        let last = MUSIC_TABLE + ((MUSIC_COUNT - 1) * ROW_SIZE) as u32;
        assert_eq!(
            MusicTable::from_executable(&image, MUSIC_TABLE),
            Err(SoundError::OutOfImage(last + 6))
        );
        assert_eq!(
            MusicTable::from_executable(&image, MUSIC_TABLE + 4),
            Err(SoundError::OutOfImage(MUSIC_TABLE))
        );
    }

    #[test]
    fn a_sound_file_gives_its_sections_and_leaves_out_the_empty_ones() {
        let bytes = file([&[1, 2, 3], &[], &[4, 5], &[6]]);
        let sound = SoundFile::parse(&bytes).expect("sound file");
        assert_eq!(sound.bank_head, Some(&[1u8, 2, 3][..]));
        assert_eq!(sound.bank_body, None);
        assert_eq!(sound.sets, [Some(&[4u8, 5][..]), Some(&[6u8][..])]);
    }

    /// The head of a bank with the programs given (number and the samples of its tones)
    /// and the sizes of its samples in units of 8 bytes.
    fn bank(programs: &[(usize, &[u16])], sizes: &[u16]) -> Vec<u8> {
        let mut head = vec![0; BANK_HEAD + PROGRAMS * PROGRAM_SIZE];
        head[..4].copy_from_slice(&BANK_MARK);
        head[18..20].copy_from_slice(&(programs.len() as u16).to_le_bytes());
        head[22..24].copy_from_slice(&(sizes.len() as u16).to_le_bytes());
        head[24..26].copy_from_slice(&[120, 64]);
        for (slot, (number, samples)) in programs.iter().enumerate() {
            let record = BANK_HEAD + number * PROGRAM_SIZE;
            head[record..record + 5].copy_from_slice(&[samples.len() as u8, 100, 9, 3, 60]);
            let mut tones = vec![0; TONES * TONE_SIZE];
            for (tone, sample) in samples.iter().enumerate() {
                let t = &mut tones[tone * TONE_SIZE..][..TONE_SIZE];
                let first = (10 * slot + tone) as u8;
                t[..8].copy_from_slice(&[first, 4, 127, 64, 72, 5, 12, 96]);
                t[12..14].copy_from_slice(&[2, 3]);
                t[16..20].copy_from_slice(&[0xFF, 0x80, 0xC0, 0x1F]);
                t[22..24].copy_from_slice(&sample.to_le_bytes());
            }
            head.extend_from_slice(&tones);
        }
        let mut table = vec![0; 2 * SAMPLE_SIZES];
        for (sample, size) in sizes.iter().enumerate() {
            table[2 * (sample + 1)..][..2].copy_from_slice(&size.to_le_bytes());
        }
        head.extend_from_slice(&table);
        head
    }

    #[test]
    fn a_bank_gives_its_programs_their_tones_and_the_blocks_of_a_sample() {
        let head = bank(&[(5, &[2, 1]), (61, &[2])], &[2, 4]);
        let bank = Bank::parse(&head).expect("bank");
        assert_eq!((bank.volume, bank.pan), (120, 64));
        assert_eq!(bank.programs().len(), 2);
        assert_eq!(bank.program(6), None);
        let program = bank.program(61).expect("program 61");
        assert_eq!(
            (program.volume, program.priority, program.mode, program.pan),
            (100, 9, 3, 60)
        );
        // The second program in use has the second sixteen tone records.
        let tone = Tone {
            priority: 10,
            mode: 4,
            volume: 127,
            pan: 64,
            centre: 72,
            shift: 5,
            keys: [12, 96],
            bend: [2, 3],
            envelope: [0x80FF, 0x1FC0],
            sample: 2,
        };
        assert_eq!(program.tones, [tone]);
        let first = bank.program(5).expect("program 5");
        assert_eq!(first.tones.len(), 2);
        assert_eq!((first.tones[1].priority, first.tones[1].sample), (1, 1));

        let body: Vec<u8> = (0..48).collect();
        assert_eq!((bank.samples(), bank.body_size()), (2, 48));
        assert_eq!(bank.sample(&body, 1), Some(&body[..16]));
        assert_eq!(bank.sample(&body, 2), Some(&body[16..]));
        assert_eq!(bank.sample(&body, 0), None);
        assert_eq!(bank.sample(&body, 3), None);
        assert_eq!(bank.sample(&body[..40], 2), None);
    }

    #[test]
    fn a_bank_that_does_not_fit_its_counts_is_an_error() {
        let head = bank(&[(5, &[1])], &[2]);
        let mut other = head.clone();
        other[0] = b'x';
        assert_eq!(Bank::parse(&other), Err(SoundError::NoMark));
        assert_eq!(Bank::parse(&head[..head.len() - 1]), Err(SoundError::Bank));
        // One more program in use than the head counts.
        let mut other = head.clone();
        other[BANK_HEAD + 7 * PROGRAM_SIZE] = 1;
        assert_eq!(Bank::parse(&other), Err(SoundError::Bank));
        // One fewer.
        let mut other = head.clone();
        other[BANK_HEAD + 5 * PROGRAM_SIZE] = 0;
        assert_eq!(Bank::parse(&other), Err(SoundError::Bank));
        // A tone of a sample the bank does not have.
        assert_eq!(
            Bank::parse(&bank(&[(5, &[2])], &[2])),
            Err(SoundError::Bank)
        );
        assert_eq!(
            Bank::parse(&bank(&[(5, &[0])], &[2])),
            Err(SoundError::Bank)
        );
    }

    /// A block with its two first bytes and the bytes of sound given, zeros behind them.
    fn block(parameter: u8, flag: u8, sound: &[u8]) -> [u8; BLOCK_SIZE] {
        let mut block = [0; BLOCK_SIZE];
        block[..2].copy_from_slice(&[parameter, flag]);
        block[2..2 + sound.len()].copy_from_slice(sound);
        block
    }

    #[test]
    fn a_block_is_made_into_sound_from_its_bits_and_the_two_values_before() {
        // No filter: the low half of a byte first, four bits with a sign, shifted down
        // from the top of 16 bits.
        let mut decoder = BlockDecoder::new();
        let sound = decoder.decode(&block(0x0C, 0, &[0x21, 0xF8]));
        assert_eq!(sound[..5], [1, 2, -8, -1, 0]);
        let sound = decoder.decode(&block(0x00, 0, &[0x21]));
        assert_eq!(sound[..3], [4096, 8192, 0]);

        // Filter 2: 115 and -52 64ths of the two values before, rounded.
        let mut decoder = BlockDecoder::new();
        let sound = decoder.decode(&block(0x20, 0, &[0x01]));
        assert_eq!(sound[..3], [4096, 7360, 9897]);
        // The values before are kept from block to block.
        let carried = decoder.decode(&block(0x20, 0, &[]));
        assert_ne!(carried, [0; BLOCK_SOUND]);
        assert_eq!(BlockDecoder::new().decode(&block(0x20, 0, &[])), [0; 28]);
        assert_eq!(decoder.limited(), 0);

        // A value outside 16 bits is set to the nearest end and counted.
        let mut decoder = BlockDecoder::new();
        let sound = decoder.decode(&block(0x10, 0, &[0x77]));
        assert_eq!(sound[..2], [28672, i16::MAX]);
        assert!(decoder.limited() >= 1);
    }

    #[test]
    fn a_sample_is_played_to_its_end_block_and_knows_where_it_goes_back_to() {
        let blocks = |flags: &[u8]| -> Vec<u8> {
            let all = flags.iter().map(|&flag| block(0x0C, flag, &[0x01]));
            all.flatten().collect()
        };
        // A sample that goes round: the loop starts with its second block.
        let round = sound(&blocks(&[0, 6, 2, 3]));
        assert_eq!((round.blocks, round.back), (4, Some(BLOCK_SOUND)));
        assert_eq!(round.values.len(), 4 * BLOCK_SOUND);
        assert_eq!(round.values[BLOCK_SOUND], 1);
        // A sample that is over at its end block; the block behind it is not played.
        let once = sound(&blocks(&[0, 4, 0, 1, 7]));
        assert_eq!((once.blocks, once.back, once.limited), (4, None, 0));
        // Without a block that starts the loop it goes back to the first value.
        assert_eq!(sound(&blocks(&[0, 3])).back, Some(0));
        // Without an end block it is played through.
        let open = sound(&blocks(&[0, 2]));
        assert_eq!((open.blocks, open.back), (2, None));
    }

    #[test]
    fn the_step_of_a_sequence_comes_from_its_tempo_rounded_twice() {
        // Half a second a quarter note: 120 in a minute, and with 480 ticks a quarter
        // note 160 tenths of a tick a step, which is 960 ticks a second.
        assert_eq!(beats_a_minute(500_000), Some(120));
        assert_eq!(ticks_a_step(480, 120), Some(160));
        // 315789 microseconds: 190.0003 quarter notes, 253.33 ticks.
        assert_eq!(beats_a_minute(0x04D18D), Some(190));
        assert_eq!(ticks_a_step(480, 190), Some(253));
        // The rest decides: above the half goes up, the half itself does not.
        assert_eq!(beats_a_minute(400_001), Some(150));
        assert_eq!(beats_a_minute(7), Some(8_571_429));
        assert_eq!(beats_a_minute(48_000_000), Some(1));
        assert_eq!(beats_a_minute(40_000_000), Some(1));
        assert_eq!(ticks_a_step(480, 191), Some(255));
        assert_eq!(ticks_a_step(180, 3), Some(1));
        assert_eq!(ticks_a_step(180, 4), Some(2));
        assert_eq!(ticks_a_step(182, 3), Some(2));
        // A tempo event cuts the quarter notes off.
        assert_eq!(beats_of_an_event(400_001), Some(149));
        assert_eq!(beats_of_an_event(600_000), Some(100));
        assert_eq!(beats_of_an_event(0), None);
        // Too slow for a tick a step, no tempo at all, and a product too large.
        assert_eq!(ticks_a_step(359, 1), None);
        assert_eq!(beats_a_minute(0), None);
        assert_eq!(ticks_a_step(u16::MAX, u32::MAX), None);

        let bytes = set(&[&[0x00, 0xFF, 0x2F, 0x00]]);
        let found = sequences(&bytes).expect("set");
        assert_eq!(found[0].ticks_a_step(), Some(253));
    }

    #[test]
    fn a_key_becomes_a_pitch_word_by_octave_half_tone_and_fine_step() {
        // The tables are powers of two, cut off.
        assert_eq!(
            (half_tone(0), half_tone(1), half_tone(11)),
            (0x8000, 0x879C, 0xF1A1)
        );
        assert_eq!(
            (fine_step(0), fine_step(1), fine_step(127)),
            (0x8000, 0x800E, 0x878C)
        );
        // The centre key is the sample as it was recorded, an octave up twice that,
        // and two octaves up and anything above is the limit.
        assert_eq!(pitch(60, 0, 60, 0), PITCH_AS_RECORDED);
        assert_eq!(pitch(72, 0, 60, 0), 0x2000);
        assert_eq!(pitch(83, 0, 60, 0), 0x3C68);
        assert_eq!(pitch(84, 0, 60, 0), PITCH_LIMIT);
        assert_eq!(pitch(127, 0, 0, 0), PITCH_LIMIT);
        // Below the centre the octaves are halved, with rounding.
        assert_eq!(pitch(48, 0, 60, 0), 0x0800);
        assert_eq!(pitch(59, 0, 60, 0), 0x0F1A);
        assert_eq!(pitch(0, 0, 127, 0), 3);
        // The shift and the fine steps add up, and 128 of them are a half tone.
        assert_eq!(pitch(60, 0, 60, 1), 0x1002);
        assert_eq!(pitch(60, 100, 60, 28), pitch(61, 0, 60, 0));
        assert_eq!(pitch(60, 0, 60, 200), pitch(61, 72, 60, 0));

        let tone = Tone {
            priority: 0,
            mode: 4,
            volume: 127,
            pan: 64,
            centre: 60,
            shift: 200,
            keys: [40, 80],
            bend: [2, 12],
            envelope: [0x80FF, 0x5FC0],
            sample: 1,
        };
        assert!(tone.answers(40) && tone.answers(80) && !tone.answers(39) && !tone.answers(81));
        // A shift above 127 counts as 127 for the key as it is pressed, but in full
        // under a bend.
        assert_eq!(tone.pitch(60), pitch(60, 0, 60, 127));
        assert_eq!(tone.bent_pitch(60, 127), pitch(72, 0, 60, 200));
    }

    #[test]
    fn a_bend_moves_the_key_by_the_range_of_its_side() {
        assert_eq!(bent(60, NO_BEND, [2, 2]), (60, 0));
        // Up: 63 steps for the whole range.
        assert_eq!(bent(60, 127, [2, 2]), (62, 0));
        assert_eq!(bent(60, 96, [12, 12]), (66, 12));
        assert_eq!(bent(60, 65, [2, 2]), (60, 4));
        // Down: 64 steps, a key lower with the fine steps counted up.
        assert_eq!(bent(60, 0, [2, 2]), (57, 127));
        assert_eq!(bent(60, 63, [2, 2]), (59, 123));
        assert_eq!(bent(60, 32, [12, 2]), (53, 127));
        // A range of 0 does not bend up, and lowers by a fine step.
        assert_eq!(bent(60, 127, [0, 0]), (60, 0));
        assert_eq!(bent(60, 0, [0, 0]), (59, 127));
    }

    #[test]
    fn a_room_asks_for_its_music_as_story_variable_12_turns_it() {
        assert_eq!(room_music(-1, 1), None);
        assert_eq!(room_music(13, 0), Some(13));
        assert_eq!(room_music(13, 1), Some(15));
        assert_eq!(room_music(13, 2), Some(145));
        assert_eq!(room_music(41, 3), Some(38));
        assert_eq!(room_music(42, 4), Some(37));
        // A value turns its own music alone.
        assert_eq!(room_music(41, 1), Some(41));
        assert_eq!(room_music(13, 3), Some(13));
        assert_eq!(room_music(0, 4), Some(0));
    }

    #[test]
    fn a_fight_asks_the_sound_by_its_opponent_and_its_outcome() {
        use FightAsks::*;
        // The rows of the fight's sequences: 16 of the first set, 16 of the second.
        assert_eq!(fight_row(0), Some((0, 0)));
        assert_eq!(fight_row(15), Some((0, 15)));
        assert_eq!(fight_row(16), Some((1, 0)));
        assert_eq!(fight_row(29), Some((1, 13)));
        assert_eq!(fight_row(32), None);
        assert!(fight_row_is_music(FIGHT_ROW) && fight_row_is_music(WON_ROW));
        assert!(!fight_row_is_music(2));
        // The start is the same for every fight.
        assert_eq!(fight_starts(), [Silence, Music(0)]);
        // An ordinary fight gives the room's music back and starts its own row; a
        // boss has one of three musics.
        assert_eq!(fight_music(3, false), [GiveBack, Start(0)]);
        assert_eq!(fight_music(55, true), [Music(146)]);
        assert_eq!(fight_music(50, true), [Music(35)]);
        assert_eq!(fight_music(41, true), [Music(36)]);
        assert_eq!(fight_music(205, true), [Music(36)]);
        // A fight that is won starts the second row, whoever the opponent was, and
        // fades at the end of the pause what is stopped by then.
        let won = [SilenceKept, FadeMusic, Stop(0), Start(1)];
        assert_eq!(fight_ends(ended::WON, false), won);
        assert_eq!(fight_ends(ended::WON, true), won);
        assert_eq!(fight_paused(ended::WON, false), [Fade(0)]);
        assert_eq!(fight_paused(ended::WON, true), [FadeMusic]);
        // A flight and both ends of a lesson fade the music in the case itself, the
        // other outcomes at the end of the state behind it.
        for outcome in [ended::RAN, ended::TAUGHT, ended::REFUSED] {
            assert_eq!(fight_ends(outcome, false), [Fade(0)]);
            assert_eq!(fight_ends(outcome, true), [FadeMusic]);
            assert!(fight_paused(outcome, false).is_empty());
        }
        for outcome in [ended::EMPTY, ended::LOST] {
            assert!(fight_ends(outcome, true).is_empty());
            assert_eq!(fight_paused(outcome, false), [Fade(0)]);
            assert_eq!(fight_paused(outcome, true), [FadeMusic]);
        }
        assert_eq!(
            fight_torn_down(),
            [SilenceKept, Stop(0), Close, AllOff, Noted]
        );
    }

    #[test]
    fn the_volume_of_a_voice_is_multiplied_and_then_squared_for_a_sequence() {
        let full = Levels {
            loudness: 127,
            bank: 127,
            program: 127,
            tone: 127,
            pans: [64; 3],
            sequence: Some((127, [127, 127])),
            mono: false,
        };
        assert_eq!(volumes(&full), [VOLUME_LIMIT; 2]);
        // Half the loudness is a quarter of the volume in a sequence, and half without.
        let half = Levels {
            loudness: 64,
            ..full
        };
        assert_eq!(volumes(&half), [4160; 2]);
        let direct = Levels {
            sequence: None,
            ..half
        };
        assert_eq!(volumes(&direct), [8256; 2]);
        // The channel's volume counts like the loudness, the sequence's by side.
        let channel = Levels {
            sequence: Some((64, [127, 0])),
            ..full
        };
        assert_eq!(volumes(&channel), [4160, 0]);
        // A pan below the middle takes from the right, one above from the left, and
        // the three pans multiply.
        let left = Levels {
            pans: [0, 64, 64],
            ..direct
        };
        assert_eq!(volumes(&left), [8256, 0]);
        let right = Levels {
            pans: [64, 127, 64],
            ..direct
        };
        assert_eq!(volumes(&right), [0, 8256]);
        let both = Levels {
            pans: [32, 95, 63],
            ..direct
        };
        assert_eq!(volumes(&both), [4193; 2]);
        // One loudspeaker: both sides are the louder one.
        let mono = Levels { mono: true, ..left };
        assert_eq!(volumes(&mono), [8256; 2]);
        // A bank of 120 does not reach the limit.
        let quiet = Levels { bank: 120, ..full };
        assert_eq!(volumes(&quiet), [14626; 2]);
    }

    /// A set of the sequences given, padded to a whole word.
    fn set(all: &[&[u8]]) -> Vec<u8> {
        let mut bytes = SET_MARK.to_vec();
        bytes.extend_from_slice(&[0, 0]);
        for (number, data) in all.iter().enumerate() {
            bytes.extend_from_slice(&(number as u16).to_be_bytes());
            bytes.extend_from_slice(&[0x01, 0xE0, 0x04, 0xD1, 0x8D, 4, 2]);
            bytes.extend_from_slice(&(data.len() as u32).to_be_bytes());
            bytes.extend_from_slice(data);
        }
        bytes.resize(bytes.len().next_multiple_of(PADDING), 0);
        bytes
    }

    #[test]
    fn a_set_gives_its_sequences_and_a_sequence_its_events() {
        let first = [
            0x00,
            0xB1,
            0x07,
            0x7D, // a control on channel 1
            0x00,
            LOOP_CONTROL,
            LOOP_START, // the same status again
            0x14,
            0xC1,
            0x05, // a program, 20 ticks on
            0x81,
            0x48,
            0x91,
            0x3C,
            0x64, // a note, 200 ticks on
            0x3C,
            0x3C,
            0x00, // let go, by the same status
            0x00,
            0xE1,
            0x10,
            0x42, // a bend
            0x00,
            0xFF,
            0x51,
            0x07,
            0xA1,
            0x20, // a tempo
            0x00,
            0xB1,
            LOOP_CONTROL,
            LOOP_END, // the status behind a tempo is new
            0x00,
            0xFF,
            0x2F,
            0x00,
        ];
        let second = [0x00, 0xFF, 0x2F, 0x00];
        let bytes = set(&[&first, &second]);
        let found = sequences(&bytes).expect("set");
        assert_eq!(found.len(), 2);
        assert_eq!(
            (
                found[0].number,
                found[0].ticks,
                found[0].tempo,
                found[0].bar
            ),
            (0, 480, 0x04D18D, [4, 2])
        );
        assert_eq!((found[1].number, found[1].data), (1, &second[..]));

        let events: Vec<Event> = found[0].events().map(|e| e.expect("event")).collect();
        let waits: Vec<u32> = events.iter().map(|event| event.wait).collect();
        assert_eq!(waits, [0, 0, 20, 200, 60, 0, 0, 0, 0]);
        let channel = 1;
        let asked: Vec<Asked> = events.iter().map(|event| event.asked).collect();
        assert_eq!(
            asked,
            [
                Asked::Control {
                    channel,
                    control: 7,
                    value: 0x7D
                },
                Asked::Control {
                    channel,
                    control: LOOP_CONTROL,
                    value: LOOP_START
                },
                Asked::Program {
                    channel,
                    program: 5
                },
                Asked::Note {
                    channel,
                    key: 60,
                    loudness: 100
                },
                Asked::Note {
                    channel,
                    key: 60,
                    loudness: 0
                },
                Asked::Bend {
                    channel,
                    bytes: [0x10, 0x42]
                },
                Asked::Tempo(0x07A120),
                Asked::Control {
                    channel,
                    control: LOOP_CONTROL,
                    value: LOOP_END
                },
                Asked::End,
            ]
        );
    }

    #[test]
    fn a_set_or_a_sequence_that_cannot_be_read_is_an_error() {
        let end = [0x00, 0xFF, 0x2F, 0x00];
        let bytes = set(&[&end, &end]);
        assert_eq!(sequences(&bytes[1..]), Err(SoundError::NoMark));
        // The second sequence is cut short.
        let at = SET_HEAD + SEQUENCE_HEAD + end.len();
        assert_eq!(
            sequences(&bytes[..bytes.len() - PADDING]),
            Err(SoundError::Sequence(at))
        );
        // A set of nothing but its head has no sequence.
        assert_eq!(sequences(&bytes[..SET_HEAD]), Ok(Vec::new()));

        let events = |data: &[u8]| -> Vec<Result<Event, SoundError>> {
            let bytes = set(&[data]);
            let found = sequences(&bytes).expect("set");
            found[0].events().collect()
        };
        // No status in front of the first event.
        assert_eq!(events(&[0x00, 0x3C, 0x40]), [Err(SoundError::Event(1))]);
        // A status the disc does not have, and a meta event it does not have.
        assert_eq!(
            events(&[0x00, 0x81, 0x3C, 0x40]),
            [Err(SoundError::Event(1))]
        );
        assert_eq!(
            events(&[0x00, 0xFF, 0x58, 0x00]),
            [Err(SoundError::Event(1))]
        );
        // The events run out without an end: the walk stops at the fault.
        let note = Event {
            wait: 0,
            asked: Asked::Note {
                channel: 0,
                key: 60,
                loudness: 64,
            },
        };
        assert_eq!(
            events(&[0x00, 0x90, 0x3C, 0x40, 0x05]),
            [Ok(note), Err(SoundError::Event(5))]
        );
    }

    #[test]
    fn a_short_file_and_a_section_that_leaves_the_file_are_errors() {
        assert_eq!(
            SoundFile::parse(&[0; HEAD_SIZE - 1]),
            Err(SoundError::Short(HEAD_SIZE - 1))
        );
        let mut bytes = file([&[1], &[2], &[3], &[4, 5]]);
        bytes.pop();
        assert_eq!(SoundFile::parse(&bytes), Err(SoundError::OutOfFile(3)));
        // A size that wraps round is no section either.
        let mut bytes = file([&[1], &[], &[], &[]]);
        bytes[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(SoundFile::parse(&bytes), Err(SoundError::OutOfFile(0)));
    }
}
