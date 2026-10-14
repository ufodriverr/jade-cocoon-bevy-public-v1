
use std::fmt;

use crate::rooms::{Image, RoomTableError, step};

/// RAM address of the movie table in the loaded executable.
pub const MOVIE_TABLE: u32 = 0x8008_6B54;
/// Number of records in the movie table.
pub const MOVIES: usize = 26;
/// Bytes per record of the movie table.
const RECORD_SIZE: usize = 0x34;

/// Bytes of a sector as it lies on the disc.
pub const RAW_SECTOR: usize = 2352;
/// Offset of the subheader in a raw sector, behind the sync pattern and the header.
const SUBHEADER: usize = 16;
/// Offset of the data in a raw sector, behind the two copies of the subheader.
const DATA: usize = 24;
/// Bytes of the header that starts the data of a video sector.
const VIDEO_HEADER: usize = 32;
/// Bytes of a frame's bitstream that one video sector carries.
pub const VIDEO_PAYLOAD: usize = 2016;
/// Bytes of sound in an audio sector: eighteen groups of 128 bytes.
pub const AUDIO_PAYLOAD: usize = 2304;

/// Submode bit of a sector with sound.
const SUBMODE_AUDIO: u8 = 0x04;
/// Submode bit of a sector with data.
const SUBMODE_DATA: u8 = 0x08;
/// The first two words of a video sector's header.
const VIDEO_MAGIC: (u16, u16) = (0x0160, 0x8001);
/// The second word of a frame's bitstream.
pub const BITSTREAM_MARKER: u16 = 0x3800;

/// Why a movie table or a sector could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovieError {
    /// The table leads outside the image. Holds the RAM address that was wanted.
    OutOfImage(u32),
    /// A sector is not [`RAW_SECTOR`] bytes long. Holds its length.
    SectorSize(usize),
    /// The picture decoder's tables do not start with their command words. Holds the RAM
    /// address they were looked for at.
    NoTables(u32),
}

impl fmt::Display for MovieError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoTables(address) => {
                write!(f, "no picture decoder tables at {address:#010x}")
            }
            Self::OutOfImage(address) => {
                write!(f, "movie table leads outside the image, to {address:#010x}")
            }
            Self::SectorSize(size) => {
                write!(f, "a raw sector has {RAW_SECTOR} bytes, not {size}")
            }
        }
    }
}

impl std::error::Error for MovieError {}

impl From<RoomTableError> for MovieError {
    fn from(err: RoomTableError) -> Self {
        match err {
            RoomTableError::OutOfImage(address) => Self::OutOfImage(address),
        }
    }
}

/// How many bits of colour the pictures of a movie are decoded to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Depth {
    Sixteen,
    TwentyFour,
}

/// A rectangle of the frame buffer or the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Area {
    pub x: i16,
    pub y: i16,
    pub width: i16,
    pub height: i16,
}

/// The loudest volume the sound library takes for the drive's sound.
pub const LOUDEST: u16 = 127;

/// One record of the movie table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MovieRecord {
    /// Record of the disc's file table that gives the place of the movie.
    pub file: u32,
    pub depth: Depth,
    /// Number of the frame the stream is told to start at.
    pub first_frame: u16,
    /// Playback ends when a frame with this number or a higher one arrives.
    pub end_frame: u16,
    /// The largest number of video sectors a frame of the movie has (as counted on the
    /// disc; the code read so far does not use the value).
    pub most_sectors: u16,
    /// The word at +0x0E, not read by the code looked at so far.
    pub unknown: u16,
    /// Where the pictures are drawn in the frame buffer, and their size.
    pub picture: Area,
    /// The area of the frame buffer that is shown.
    pub display: Area,
    /// Screen x offset of the display.
    pub screen_x: i16,
    /// Screen width and height.
    pub screen: (i16, i16),
    /// Pad buttons that end the movie; zero for a movie that cannot be skipped.
    pub skip: u32,
    /// Left and right volume of the sound, 4096 for full.
    pub volume: (u16, u16),
    /// The drive runs at double speed while the movie plays.
    pub double_speed: bool,
}

impl MovieRecord {
    fn parse(image: &Image, at: u32) -> Result<Self, RoomTableError> {
        let area = |offset: u32| -> Result<Area, RoomTableError> {
            Ok(Area {
                x: image.i16(at + offset)?,
                y: image.i16(at + offset + 2)?,
                width: image.i16(at + offset + 4)?,
                height: image.i16(at + offset + 6)?,
            })
        };
        Ok(Self {
            file: image.u32(at)?,
            depth: match image.u32(at + 0x04)? {
                0 => Depth::Sixteen,
                _ => Depth::TwentyFour,
            },
            first_frame: image.u16(at + 0x08)?,
            end_frame: image.u16(at + 0x0A)?,
            most_sectors: image.u16(at + 0x0C)?,
            unknown: image.u16(at + 0x0E)?,
            picture: area(0x10)?,
            display: area(0x18)?,
            screen_x: image.i16(at + 0x20)?,
            screen: (image.i16(at + 0x24)?, image.i16(at + 0x26)?),
            skip: image.u32(at + 0x28)?,
            volume: (image.u16(at + 0x2C)?, image.u16(at + 0x2E)?),
            double_speed: image.u16(at + 0x30)? != 0,
        })
    }

    /// Whether the pad can end the movie.
    pub fn can_be_skipped(&self) -> bool {
        self.skip != 0
    }

    /// Whether buttons that went down since the player last asked the pad
    /// (`pad::Pad::pressed`) end the movie.
    pub fn skipped_by(&self, pressed: u16) -> bool {
        self.skip & u32::from(pressed) != 0
    }

    /// Whether the table plays the movie's sound at all.
    pub fn is_silent(&self) -> bool {
        self.volume == (0, 0)
    }

    /// Left and right volume the player sets for the drive's sound, out of [`LOUDEST`].
    pub fn sound_volume(&self) -> (u16, u16) {
        let scaled = |volume: u16| ((u32::from(volume) * 112) >> 12) as u16;
        (scaled(self.volume.0), scaled(self.volume.1))
    }

    /// Sectors the drive hands over in a second while the movie streams. Nothing else
    /// sets the movie's pace: a picture is there when its sectors are.
    pub fn sectors_a_second(&self) -> u32 {
        if self.double_speed { 150 } else { 75 }
    }
}

/// The movie table: one record per movie, indexed by the number the player is given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovieTable {
    records: Vec<MovieRecord>,
}

impl MovieTable {
    /// Reads the table from the loaded executable, whose first byte is at RAM address `base`.
    pub fn from_executable(image: &[u8], base: u32) -> Result<Self, MovieError> {
        Self::parse(image, base, MOVIE_TABLE, MOVIES)
    }

    /// Reads `count` records at RAM address `table`.
    pub fn parse(data: &[u8], base: u32, table: u32, count: usize) -> Result<Self, MovieError> {
        let image = Image { data, base };
        let records = (0..count)
            .map(|index| MovieRecord::parse(&image, step(table, index, RECORD_SIZE)?))
            .collect::<Result<_, _>>()?;
        Ok(Self { records })
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn get(&self, movie: usize) -> Option<&MovieRecord> {
        self.records.get(movie)
    }

    pub fn iter(&self) -> impl Iterator<Item = (usize, &MovieRecord)> {
        self.records.iter().enumerate()
    }
}

/// A sector of a movie stream, told apart by its subheader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sector<'a> {
    Video(VideoSector<'a>),
    Audio(AudioSector<'a>),
    /// Padding, or anything else that is neither a piece of a picture nor sound.
    Other,
}

impl<'a> Sector<'a> {
    /// Reads one raw sector of [`RAW_SECTOR`] bytes.
    pub fn parse(raw: &'a [u8]) -> Result<Self, MovieError> {
        if raw.len() != RAW_SECTOR {
            return Err(MovieError::SectorSize(raw.len()));
        }
        let (submode, coding) = (raw[SUBHEADER + 2], raw[SUBHEADER + 3]);
        let data = &raw[DATA..];
        if submode & SUBMODE_AUDIO != 0 {
            return Ok(Self::Audio(AudioSector {
                stereo: coding & 0x03 != 0,
                half_rate: coding & 0x0C != 0,
                sound: &data[..AUDIO_PAYLOAD],
            }));
        }
        let short = |at: usize| u16::from_le_bytes([data[at], data[at + 1]]);
        let long =
            |at: usize| u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
        if submode & SUBMODE_DATA == 0 || (short(0), short(2)) != VIDEO_MAGIC {
            return Ok(Self::Other);
        }
        Ok(Self::Video(VideoSector {
            number: short(4),
            count: short(6),
            frame: long(8),
            size: long(0x0C),
            width: short(0x10),
            height: short(0x12),
            start: data[0x14..0x1C].try_into().expect("eight bytes"),
            payload: &data[VIDEO_HEADER..VIDEO_HEADER + VIDEO_PAYLOAD],
        }))
    }
}

/// A sector with sound: compressed samples, four bits each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioSector<'a> {
    pub stereo: bool,
    /// 18900 samples a second instead of 37800.
    pub half_rate: bool,
    /// The eighteen sound groups of the sector.
    pub sound: &'a [u8],
}

impl AudioSector<'_> {
    /// Samples a second.
    pub fn rate(&self) -> u32 {
        if self.half_rate { 18_900 } else { 37_800 }
    }

    /// The parameters of the eight sound units of a group, `None` behind the last group.
    pub fn parameters(&self, group: usize) -> Option<[SoundParameter; SOUND_UNITS]> {
        let bytes = self.groups().get(group)?;
        Some(std::array::from_fn(|unit| SoundParameter(bytes[4 + unit])))
    }

    /// Whether the two copies of a group's parameter bytes are alike.
    pub fn copies_agree(&self, group: usize) -> bool {
        self.groups()
            .get(group)
            .is_some_and(|bytes| bytes[0..4] == bytes[4..8] && bytes[8..12] == bytes[12..16])
    }

    /// The sound groups of the sector.
    fn groups(&self) -> &[[u8; SOUND_GROUP]] {
        self.sound.as_chunks().0
    }
}

/// Bytes of a sound group: sixteen parameter bytes and 28 words of four bytes.
pub const SOUND_GROUP: usize = 128;
/// Sound groups of an audio sector.
pub const SOUND_GROUPS: usize = AUDIO_PAYLOAD / SOUND_GROUP;
/// Parameter bytes at the start of a sound group.
const SOUND_PARAMETERS: usize = 16;
/// Sound units of a sound group.
pub const SOUND_UNITS: usize = 8;
/// Samples of a sound unit.
pub const UNIT_SAMPLES: usize = 28;
/// Samples of an audio sector, both sides of a stereo sector counted.
pub const SECTOR_SAMPLES: usize = SOUND_GROUPS * SOUND_UNITS * UNIT_SAMPLES;
/// Share of the sample before and of the one before that in a sample, in 64ths, by filter.
const SOUND_FILTERS: [(i32, i32); 4] = [(0, 0), (60, 0), (115, -52), (98, -55)];

/// The parameter byte of a sound unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoundParameter(pub u8);

impl SoundParameter {
    /// Which of the four filters the unit's samples are made with.
    pub fn filter(self) -> usize {
        usize::from(self.0 >> 4) & 3
    }

    /// How far the four bits of a sample are shifted down from the top of 16 bits. No
    /// byte on the disc names more than 12; by the public descriptions the chip takes 9
    /// for those.
    pub fn shift(self) -> u32 {
        match u32::from(self.0 & 0x0F) {
            shift @ 0..=12 => shift,
            _ => 9,
        }
    }
}

/// Makes the samples of a movie's audio sectors. The two samples before, which every
/// sample is made from, are kept from sector to sector for each side.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SoundDecoder {
    /// The sample before and the one before that, of the left (or only) side and the right.
    before: [[i32; 2]; 2],
    limited: u64,
}

impl SoundDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// How many samples so far lay outside 16 bits and were set to the nearest end.
    pub fn limited(&self) -> u64 {
        self.limited
    }

    /// The samples of one sector: [`SECTOR_SAMPLES`] of them, in a stereo sector left and
    /// right in turn.
    pub fn decode(&mut self, sector: &AudioSector) -> Vec<i16> {
        let groups = sector.groups();
        let mut samples = vec![0; groups.len() * SOUND_UNITS * UNIT_SAMPLES];
        for (group, bytes) in groups.iter().enumerate() {
            for unit in 0..SOUND_UNITS {
                let parameter = SoundParameter(bytes[4 + unit]);
                let (share, share_before) = SOUND_FILTERS[parameter.filter()];
                // Even units are the left side of a stereo sector, odd ones the right.
                let side = if sector.stereo { unit & 1 } else { 0 };
                let [mut old, mut older] = self.before[side];
                for index in 0..UNIT_SAMPLES {
                    let byte = bytes[SOUND_PARAMETERS + 4 * index + unit / 2];
                    let bits = if unit & 1 == 0 {
                        byte << 4
                    } else {
                        byte & 0xF0
                    };
                    let value = i32::from(bits as i8) << 8;
                    let wanted = (value >> parameter.shift())
                        + ((old * share + older * share_before + 32) >> 6);
                    let sample = wanted.clamp(i32::from(i16::MIN), i32::from(i16::MAX));
                    self.limited += u64::from(sample != wanted);
                    (old, older) = (sample, old);
                    let at = if sector.stereo {
                        2 * ((group * SOUND_UNITS / 2 + unit / 2) * UNIT_SAMPLES + index) + side
                    } else {
                        (group * SOUND_UNITS + unit) * UNIT_SAMPLES + index
                    };
                    samples[at] = sample as i16;
                }
                self.before[side] = [old, older];
            }
        }
        samples
    }
}

/// A sector with a piece of a picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoSector<'a> {
    /// Number of this sector within its frame, from 0.
    pub number: u16,
    /// Number of sectors of the frame.
    pub count: u16,
    /// Number of the frame, from 1.
    pub frame: u32,
    /// Bytes of the frame's bitstream.
    pub size: u32,
    pub width: u16,
    pub height: u16,
    /// The header's copy of the first eight bytes of the frame's bitstream.
    pub start: [u8; 8],
    /// This sector's piece of the bitstream.
    pub payload: &'a [u8],
}

/// The four words a frame's bitstream starts with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BitstreamHeader {
    /// Number of 32-bit words the codes of the frame fill: two codes a word, filled up
    /// with end-of-block codes.
    pub words: u16,
    /// [`BITSTREAM_MARKER`] in every frame of the disc: with the count below it, the
    /// command word the picture decoder is started with.
    pub marker: u16,
    /// The quantisation scale of the frame.
    pub scale: u16,
    /// 2 or 3: how the bitstream packs its values.
    pub version: u16,
}

impl BitstreamHeader {
    /// Reads the header from the start of a bitstream, or `None` when that is too short.
    pub fn parse(bitstream: &[u8]) -> Option<Self> {
        let word = |at: usize| {
            Some(u16::from_le_bytes([
                *bitstream.get(at)?,
                *bitstream.get(at + 1)?,
            ]))
        };
        Some(Self {
            words: word(0)?,
            marker: word(2)?,
            scale: word(4)?,
            version: word(6)?,
        })
    }
}

/// One picture of a movie, still packed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Number of the frame, from 1.
    pub number: u32,
    pub width: u16,
    pub height: u16,
    /// The pieces of the frame's sectors in a row, cut to the size the sectors name.
    pub bitstream: Vec<u8>,
}

impl Frame {
    pub fn header(&self) -> Option<BitstreamHeader> {
        BitstreamHeader::parse(&self.bitstream)
    }
}

/// Puts the video sectors of a stream together into frames.
///
/// A frame is whole when its sectors 0 to count - 1 came in a row. Anything else is left
/// out and counted: a frame that starts in its middle (the game starts every movie on the
/// file's second sector), one that another frame cuts short, a stray sector.
#[derive(Debug, Default)]
pub struct Demuxer {
    partial: Option<Frame>,
    /// Number of the sector the frame in the making waits for.
    wanted: u16,
    /// Number of the frame that was left out last.
    left_out: Option<u32>,
    dropped: u32,
}

impl Demuxer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes the next video sector of the stream and gives the frame it completes.
    pub fn push(&mut self, sector: &VideoSector) -> Option<Frame> {
        let goes_on = self
            .partial
            .as_ref()
            .is_some_and(|frame| frame.number == sector.frame && self.wanted == sector.number);
        if !goes_on {
            if let Some(frame) = self.partial.take() {
                self.leave_out(frame.number);
            }
            if sector.number != 0 {
                // A piece without its start: the rest of this frame is of no use either.
                self.leave_out(sector.frame);
                return None;
            }
            self.partial = Some(Frame {
                number: sector.frame,
                width: sector.width,
                height: sector.height,
                bitstream: Vec::with_capacity(sector.count as usize * VIDEO_PAYLOAD),
            });
        }
        let frame = self.partial.as_mut()?;
        frame.bitstream.extend_from_slice(sector.payload);
        self.wanted = sector.number + 1;
        if self.wanted < sector.count {
            return None;
        }
        let mut frame = self.partial.take()?;
        frame.bitstream.truncate(sector.size as usize);
        Some(frame)
    }

    /// Counts a frame as left out, once however many of its sectors say so.
    fn leave_out(&mut self, frame: u32) {
        if self.left_out != Some(frame) {
            self.left_out = Some(frame);
            self.dropped += 1;
        }
    }

    /// Number of frames that were left out so far. A frame still in the making at the end
    /// of the stream is not among them.
    pub fn dropped(&self) -> u32 {
        self.dropped
    }

    /// Whether a frame is in the making, waiting for more sectors.
    pub fn is_waiting(&self) -> bool {
        self.partial.is_some()
    }
}

/// What the player's loop does when the stream hands it a frame.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Arrival {
    /// The frame whose picture goes on screen now: the one that came before.
    pub show: Option<Frame>,
    /// The frame that came is the end frame or one behind it. It is not shown, and the
    /// loop ends once the picture of `show` has been on screen.
    pub ended: bool,
}

#[derive(Debug, Clone)]
pub struct Screening {
    end_frame: u32,
    waiting: Option<Frame>,
    ended: bool,
}

impl Screening {
    pub fn new(record: &MovieRecord) -> Self {
        Self {
            end_frame: record.end_frame.into(),
            waiting: None,
            ended: false,
        }
    }

    /// Takes the next whole frame of the stream.
    pub fn arrived(&mut self, frame: Frame) -> Arrival {
        if self.ended {
            return Arrival {
                show: None,
                ended: true,
            };
        }
        let show = self.waiting.take();
        if frame.number >= self.end_frame {
            self.ended = true;
        } else {
            self.waiting = Some(frame);
        }
        Arrival {
            show,
            ended: self.ended,
        }
    }

    /// Whether the end frame has come.
    pub fn has_ended(&self) -> bool {
        self.ended
    }

    /// Number of the frame that waits for the one behind it, if any.
    pub fn waiting(&self) -> Option<u32> {
        self.waiting.as_ref().map(|frame| frame.number)
    }
}

/// RAM address of the packed code table in the loaded executable.
pub const CODE_TABLE: u32 = 0x8008_9B24;
/// 16-bit words of the unpacked code table.
pub const CODE_TABLE_WORDS: usize = 0x8800;
/// Word of the table where the records asked with thirteen bits start. The words before
/// them belong to bitstream version 3.
const WIDE_RECORDS: usize = 1024;
/// Word of the table where the records asked with nine bits start.
const NARROW_RECORDS: usize = WIDE_RECORDS + 4 * 8192;
/// The value of the table that stands for a code written out in the stream.
const ESCAPE: u16 = 0x7C1F;
/// The code that ends a block, also what the codes of a frame are filled up with.
pub const END_OF_BLOCK: u16 = 0xFE00;
/// The ten bits that stand in place of a block's first value behind the last block.
const END_OF_FRAME: u32 = 0x1FF;
/// Bytes of the four header words and nothing else.
const BITSTREAM_HEADER: usize = 8;

/// Why a bitstream could not be expanded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitstreamError {
    /// The bitstream is shorter than its header.
    NoHeader,
    /// The bitstream is of a version this module does not expand. Holds the version.
    Version(u16),
    /// The table has no code for the bits at this bit of the stream (counted from the end
    /// of the header), or the stream ended without its end mark.
    NoCode(usize),
    /// The bitstream holds more codes than its header names. Holds the count so far.
    TooLong(usize),
}

impl fmt::Display for BitstreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoHeader => write!(f, "a bitstream without its header"),
            Self::Version(version) => write!(f, "bitstream version {version} is not expanded"),
            Self::NoCode(bit) => write!(f, "no code for the bits at bit {bit} of the stream"),
            Self::TooLong(codes) => {
                write!(f, "more codes than the header names ({codes} so far)")
            }
        }
    }
}

impl std::error::Error for BitstreamError {}

/// The bits of a stream of 16-bit words, the high bit of a word first.
struct Bits<'a> {
    data: &'a [u8],
    /// Bits taken so far.
    at: usize,
}

impl Bits<'_> {
    fn word(&self, index: usize) -> u64 {
        match self.data.get(2 * index..2 * index + 2) {
            Some(bytes) => u64::from(u16::from_le_bytes([bytes[0], bytes[1]])),
            // Behind the end the game reads whatever lies there; no frame of the disc
            // gets that far, and zeros lead to a record without a code.
            None => 0,
        }
    }

    /// The next `count` bits (16 at most) without taking them.
    fn peek(&self, count: usize) -> u32 {
        let index = self.at / 16;
        let window = self.word(index) << 32 | self.word(index + 1) << 16 | self.word(index + 2);
        ((window >> (48 - self.at % 16 - count)) & ((1 << count) - 1)) as u32
    }

    fn skip(&mut self, count: usize) {
        self.at += count;
    }

    fn is_spent(&self) -> bool {
        self.at > 8 * self.data.len()
    }
}

/// The codes of one frame, as the picture decoder takes them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Codes {
    /// The header the bitstream came with.
    pub header: BitstreamHeader,
    /// Two codes for every word the header names. A block is its first value with the
    /// frame's scale in the upper six bits, then pairs of a count of zeros (six bits) and
    /// a value (ten bits), then [`END_OF_BLOCK`].
    pub codes: Vec<u16>,
    /// How many of the codes came out of the bitstream; the rest is filling.
    pub used: usize,
}

impl Codes {
    /// The blocks of the frame: the first value and the pairs of each, without its end
    /// code. A first value is never taken for an end code, whatever its bits.
    pub fn blocks(&self) -> impl Iterator<Item = &[u16]> {
        let mut rest = &self.codes[..self.used];
        std::iter::from_fn(move || {
            let (_, pairs) = rest.split_first()?;
            let end = pairs
                .iter()
                .position(|&code| code == END_OF_BLOCK)
                .map_or(rest.len(), |at| at + 1);
            let (block, behind) = rest.split_at(end);
            rest = behind.get(1..).unwrap_or(&[]);
            Some(block)
        })
    }
}

/// The table the library expands a bitstream with: for the next bits of the stream, how
/// many of them to take and the one to three codes they stand for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeTable {
    words: Vec<u16>,
}

impl CodeTable {
    /// Unpacks the table from the loaded executable, whose first byte is at RAM address
    /// `base`.
    pub fn from_executable(image: &[u8], base: u32) -> Result<Self, MovieError> {
        let packed = CODE_TABLE
            .checked_sub(base)
            .and_then(|at| image.get(at as usize..))
            .ok_or(MovieError::OutOfImage(CODE_TABLE))?;
        let words = unpack(packed, CODE_TABLE_WORDS).ok_or(MovieError::OutOfImage(
            base.wrapping_add(image.len() as u32),
        ))?;
        Ok(Self { words })
    }

    /// A table from its unpacked words, or `None` when there are not [`CODE_TABLE_WORDS`].
    pub fn from_words(words: Vec<u16>) -> Option<Self> {
        (words.len() == CODE_TABLE_WORDS).then_some(Self { words })
    }

    /// The unpacked words.
    pub fn words(&self) -> &[u16] {
        &self.words
    }

    /// The record asked with the next thirteen bits of the stream: the bits to take and
    /// up to three codes (zero for none). `None` for a record without a code.
    pub fn wide(&self, bits: usize) -> Option<(usize, [u16; 3])> {
        let at = WIDE_RECORDS + 4 * (bits & 0x1FFF);
        let record = &self.words[at..at + 4];
        (record[..2] != [0, 0]).then_some((
            usize::from(record[0] & 0xFF),
            [record[1], record[2], record[3]],
        ))
    }

    /// The record asked with the next nine bits once eight zero bits are taken: the bits
    /// to take and the code. `None` for a record without a code.
    pub fn narrow(&self, bits: usize) -> Option<(usize, u16)> {
        let at = NARROW_RECORDS + 2 * (bits & 0x1FF);
        let record = &self.words[at..at + 2];
        (record != [0, 0]).then_some((usize::from(record[0] & 0xFF), record[1]))
    }

    /// Expands the bitstream of a frame to its codes, as the library routine `0x80061E4C`
    /// does for version 2.
    pub fn expand(&self, bitstream: &[u8]) -> Result<Codes, BitstreamError> {
        let header = BitstreamHeader::parse(bitstream).ok_or(BitstreamError::NoHeader)?;
        if header.version != 2 {
            return Err(BitstreamError::Version(header.version));
        }
        let scale = header.scale << 10;
        let room = 2 * usize::from(header.words);
        let mut codes = Vec::with_capacity(room);
        let mut bits = Bits {
            data: &bitstream[BITSTREAM_HEADER..],
            at: 0,
        };
        loop {
            // A block starts with its first value in ten bits, or the frame ends here.
            let first = bits.peek(10);
            if first == END_OF_FRAME {
                break;
            }
            bits.skip(10);
            codes.push(scale | first as u16);
            'block: loop {
                if bits.is_spent() {
                    return Err(BitstreamError::NoCode(bits.at));
                }
                if codes.len() > room {
                    return Err(BitstreamError::TooLong(codes.len()));
                }
                let (taken, record) = match self.wide(bits.peek(13) as usize) {
                    Some(record) => record,
                    None => {
                        bits.skip(8);
                        let (taken, code) = self
                            .narrow(bits.peek(9) as usize)
                            .filter(|&(taken, _)| taken != 0)
                            .ok_or(BitstreamError::NoCode(bits.at - 8))?;
                        (taken, [code, 0, 0])
                    }
                };
                bits.skip(taken);
                for (place, code) in record.into_iter().enumerate() {
                    if place > 0 && code == 0 {
                        break;
                    }
                    match code {
                        ESCAPE => {
                            // The code itself follows in sixteen bits.
                            codes.push(bits.peek(16) as u16);
                            bits.skip(16);
                            break;
                        }
                        END_OF_BLOCK => {
                            codes.push(code);
                            break 'block;
                        }
                        _ => codes.push(code),
                    }
                }
            }
        }
        if codes.len() > room {
            return Err(BitstreamError::TooLong(codes.len()));
        }
        let used = codes.len();
        codes.resize(room, END_OF_BLOCK);
        Ok(Codes {
            header,
            codes,
            used,
        })
    }
}

/// Unpacks the code table as the library routine `0x8006219C` does: runs of bytes that
/// are either given or copied from a set distance back, then every word is combined with
/// the word four places before it. `None` when the packed bytes end too early.
pub fn unpack(packed: &[u8], words: usize) -> Option<Vec<u16>> {
    /// The distance that ends the packed bytes.
    const END: usize = 3840;
    let mut bytes = Vec::with_capacity(2 * words);
    let mut packed = packed.iter().copied();
    let mut back = 0;
    while back != END {
        let byte = usize::from(packed.next()?);
        match byte {
            0..240 => {
                for _ in 0..=byte {
                    let next = match back {
                        0 => packed.next()?,
                        _ => *bytes.get(bytes.len().checked_sub(back)?)?,
                    };
                    bytes.push(next);
                }
            }
            240 => back = 0,
            _ => back = (byte << 8 | usize::from(packed.next()?)) - 0xF0FF,
        }
    }
    bytes.resize(2 * words, 0);
    let mut table: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&pair| u16::from_le_bytes(pair))
        .collect();
    for at in 4..words {
        table[at] ^= table[at - 4];
    }
    Some(table)
}

/// RAM address of the two blocks the library sets the picture decoder up with: a command
/// word and the two quantisation tables, then a command word and the transform table.
pub const DECODER_TABLES: u32 = 0x8008_99A4;
/// The command word in front of the quantisation tables: "two tables follow".
const QUANTISATION_COMMAND: u32 = 0x4000_0001;
/// The command word in front of the transform table.
const TRANSFORM_COMMAND: u32 = 0x6000_0000;
/// Pixels along the side of a block.
const BLOCK: usize = 8;
/// Blocks of a 16 by 16 piece of the picture: two of colour, four of brightness.
const BLOCKS_OF_A_PIECE: usize = 6;

/// The place of every value of a block, in the order the values come in: from the top
/// left corner in diagonals, to and fro.
const ZIGZAG: [usize; 64] = zigzag();

const fn zigzag() -> [usize; 64] {
    let mut order = [0; 64];
    let (mut x, mut y, mut at) = (0, 0, 0);
    while at < 64 {
        order[at] = y * BLOCK + x;
        if (x + y) % 2 == 0 {
            // Up and to the right, along the top and down the right side.
            if x == BLOCK - 1 {
                y += 1;
            } else if y == 0 {
                x += 1;
            } else {
                x += 1;
                y -= 1;
            }
        } else if y == BLOCK - 1 {
            x += 1;
        } else if x == 0 {
            y += 1;
        } else {
            x -= 1;
            y += 1;
        }
        at += 1;
    }
    order
}

/// Why the codes of a frame did not become a picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PictureError {
    /// The codes do not hold six blocks for each 16 by 16 pixels of the picture.
    Blocks { wanted: usize, found: usize },
}

impl fmt::Display for PictureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Blocks { wanted, found } => {
                write!(
                    f,
                    "the picture needs {wanted} blocks, the codes hold {found}"
                )
            }
        }
    }
}

impl std::error::Error for PictureError {}

/// One picture of a movie as the picture decoder gives it in its 24-bit mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    pub width: usize,
    pub height: usize,
    /// Red, green and blue of every pixel, row by row from the top.
    pub rgb: Vec<u8>,
}

impl Picture {
    /// Red, green and blue of the pixel at (`x`, `y`).
    pub fn pixel(&self, x: usize, y: usize) -> [u8; 3] {
        let at = 3 * (y * self.width + x);
        [self.rgb[at], self.rgb[at + 1], self.rgb[at + 2]]
    }

    /// The picture as RGBA in the depth a movie is decoded to. In 16-bit mode the decoder
    /// keeps the upper five bits of each colour, shown here the way the other 15-bit
    /// colours of the disc are ([`crate::tim`]).
    pub fn rgba(&self, depth: Depth) -> Vec<u8> {
        let shown = |value: u8| match depth {
            Depth::TwentyFour => value,
            Depth::Sixteen => value & 0xF8 | value >> 5,
        };
        self.rgb
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|&[red, green, blue]| [shown(red), shown(green), shown(blue), 0xFF])
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PictureDecoder {
    /// Quantisation of the brightness blocks, in the order the values of a block come in.
    pub brightness: [u8; 64],
    /// Quantisation of the colour blocks.
    pub colour: [u8; 64],
    /// The transform table: a row for each frequency, a column for each pixel.
    pub transform: [i16; 64],
}

impl PictureDecoder {
    /// Reads the tables from the loaded executable, whose first byte is at RAM address
    /// `base`.
    pub fn from_executable(image: &[u8], base: u32) -> Result<Self, MovieError> {
        Self::parse(image, base, DECODER_TABLES)
    }

    /// Reads the two blocks at RAM address `tables`: each must start with its command word.
    pub fn parse(data: &[u8], base: u32, tables: u32) -> Result<Self, MovieError> {
        let image = Image { data, base };
        let transform_at = tables + 4 + 128;
        if image.u32(tables)? != QUANTISATION_COMMAND
            || image.u32(transform_at)? != TRANSFORM_COMMAND
        {
            return Err(MovieError::NoTables(tables));
        }
        let mut decoder = Self {
            brightness: [0; 64],
            colour: [0; 64],
            transform: [0; 64],
        };
        for at in 0..64 {
            decoder.brightness[at] = image.u8(tables + 4 + at as u32)?;
            decoder.colour[at] = image.u8(tables + 4 + 64 + at as u32)?;
            decoder.transform[at] = image.i16(transform_at + 4 + 2 * at as u32)?;
        }
        Ok(decoder)
    }

    /// The values of one block, each at its place and multiplied back up: `block` is the
    /// first value with the scale above it and the pairs of zeros and value that follow.
    pub fn values(block: &[u16], table: &[u8; 64]) -> [i32; 64] {
        let mut values = [0; 64];
        let Some((&first, pairs)) = block.split_first() else {
            return values;
        };
        let scale = i32::from(first >> 10);
        // Ten bits with a sign.
        let value = |code: u16| i32::from(((code << 6) as i16) >> 6);
        let mut put = |at: usize, value: i32| {
            // Without a scale the values are taken as they come, in rows.
            let place = if scale == 0 { at } else { ZIGZAG[at] };
            values[place] = value.clamp(-0x400, 0x3FF);
        };
        put(
            0,
            match scale {
                0 => 2 * value(first),
                _ => value(first) * i32::from(table[0]),
            },
        );
        let mut at = 0;
        for &pair in pairs {
            at += usize::from(pair >> 10) + 1;
            if at > 63 {
                break;
            }
            put(
                at,
                match scale {
                    0 => 2 * value(pair),
                    // Rounded to the nearest, a half upwards.
                    _ => (value(pair) * i32::from(table[at]) * scale + 4) >> 3,
                },
            );
        }
        values
    }

    /// One pass of the transform: the columns of `block` become rows of the result.
    fn pass(&self, block: &[i32; 64]) -> [i32; 64] {
        let mut out = [0; 64];
        for x in 0..BLOCK {
            for y in 0..BLOCK {
                let sum: i32 = (0..BLOCK)
                    .map(|z| block[y + z * BLOCK] * i32::from(self.transform[x + z * BLOCK] >> 3))
                    .sum();
                out[x + y * BLOCK] = (sum + 0xFFF) >> 13;
            }
        }
        out
    }

    /// The 64 samples of a block from its values: the transform in both directions.
    pub fn samples(&self, values: &[i32; 64]) -> [i32; 64] {
        self.pass(&self.pass(values))
    }

    /// The picture of a frame from its codes. The 16 by 16 pieces come in columns, each
    /// from the top down; a piece is two colour blocks (red, then blue) for all of it and
    /// four brightness blocks for its quarters.
    pub fn decode(
        &self,
        codes: &Codes,
        width: usize,
        height: usize,
    ) -> Result<Picture, PictureError> {
        let (across, down) = (width.div_ceil(2 * BLOCK), height.div_ceil(2 * BLOCK));
        let wanted = across * down * BLOCKS_OF_A_PIECE;
        let found = codes.blocks().count();
        if found != wanted {
            return Err(PictureError::Blocks { wanted, found });
        }
        let mut picture = Picture {
            width,
            height,
            rgb: vec![0; 3 * width * height],
        };
        let mut blocks = codes.blocks();
        let mut next = |table: &[u8; 64]| {
            let block = blocks.next().unwrap_or(&[]);
            self.samples(&Self::values(block, table))
        };
        for piece in 0..across * down {
            let (left, top) = (2 * BLOCK * (piece / down), 2 * BLOCK * (piece % down));
            let (red, blue) = (next(&self.colour), next(&self.colour));
            for quarter in 0..4 {
                let brightness = next(&self.brightness);
                let (right, low) = (BLOCK * (quarter % 2), BLOCK * (quarter / 2));
                for (at, &sample) in brightness.iter().enumerate() {
                    let (x, y) = (right + at % BLOCK, low + at / BLOCK);
                    if left + x >= width || top + y >= height {
                        continue;
                    }
                    let shared = x / 2 + y / 2 * BLOCK;
                    let colour = colours(sample, red[shared], blue[shared]);
                    let to = 3 * ((top + y) * width + left + x);
                    picture.rgb[to..to + 3].copy_from_slice(&colour);
                }
            }
        }
        Ok(picture)
    }
}

/// Red, green and blue of a pixel from its brightness and the two colour samples it
/// shares with its neighbours.
fn colours(brightness: i32, red: i32, blue: i32) -> [u8; 3] {
    let shown = |colour: i32| ((brightness + colour).clamp(-128, 127) + 128) as u8;
    [
        shown((359 * red + 128) >> 8),
        shown((-88 * blue - 183 * red + 128) >> 8),
        shown((454 * blue + 128) >> 8),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw_sector(submode: u8, coding: u8, data: &[u8]) -> Vec<u8> {
        let mut raw = vec![0u8; RAW_SECTOR];
        for copy in [SUBHEADER, SUBHEADER + 4] {
            raw[copy..copy + 4].copy_from_slice(&[1, 1, submode, coding]);
        }
        raw[DATA..DATA + data.len()].copy_from_slice(data);
        raw
    }

    /// A video sector whose payload is `fill` all over, of a 320 by 240 frame.
    fn video(frame: u32, number: u16, count: u16, size: u32, fill: u8) -> Vec<u8> {
        let mut data = vec![fill; VIDEO_HEADER + VIDEO_PAYLOAD];
        data[0..2].copy_from_slice(&VIDEO_MAGIC.0.to_le_bytes());
        data[2..4].copy_from_slice(&VIDEO_MAGIC.1.to_le_bytes());
        data[4..6].copy_from_slice(&number.to_le_bytes());
        data[6..8].copy_from_slice(&count.to_le_bytes());
        data[8..12].copy_from_slice(&frame.to_le_bytes());
        data[12..16].copy_from_slice(&size.to_le_bytes());
        data[16..18].copy_from_slice(&320u16.to_le_bytes());
        data[18..20].copy_from_slice(&240u16.to_le_bytes());
        data[20..28].copy_from_slice(&[9, 0, 0, 0x38, 5, 0, 2, 0]);
        data[28..32].fill(0);
        raw_sector(0x48, 0, &data)
    }

    fn video_of(raw: &[u8]) -> VideoSector<'_> {
        match Sector::parse(raw).unwrap() {
            Sector::Video(sector) => sector,
            other => panic!("not a video sector: {other:?}"),
        }
    }

    #[test]
    fn tells_the_kinds_of_sector_apart() {
        let raw = video(7, 1, 3, 5000, 0xAB);
        let sector = video_of(&raw);
        assert_eq!((sector.frame, sector.number, sector.count), (7, 1, 3));
        assert_eq!((sector.size, sector.width, sector.height), (5000, 320, 240));
        assert_eq!(sector.payload.len(), VIDEO_PAYLOAD);
        assert!(sector.payload.iter().all(|&byte| byte == 0xAB));
        assert_eq!(
            BitstreamHeader::parse(&sector.start),
            Some(BitstreamHeader {
                words: 9,
                marker: BITSTREAM_MARKER,
                scale: 5,
                version: 2
            })
        );

        for (coding, stereo, rate) in [
            (0x01, true, 37_800),
            (0x05, true, 18_900),
            (0x04, false, 18_900),
        ] {
            let raw = raw_sector(0x64, coding, &[3; AUDIO_PAYLOAD]);
            let Sector::Audio(sound) = Sector::parse(&raw).unwrap() else {
                panic!("not an audio sector");
            };
            assert_eq!((sound.stereo, sound.rate()), (stereo, rate));
            assert_eq!(sound.sound, [3; AUDIO_PAYLOAD]);
        }

        // Padding has no submode bits; data without the magic words is no picture either.
        assert_eq!(Sector::parse(&raw_sector(0, 0, &[])), Ok(Sector::Other));
        assert_eq!(
            Sector::parse(&raw_sector(0x48, 0, &[1, 2, 3, 4])),
            Ok(Sector::Other)
        );
        assert_eq!(Sector::parse(&[0; 2048]), Err(MovieError::SectorSize(2048)));
        assert_eq!(BitstreamHeader::parse(&[0; 7]), None);
    }

    /// The sound of an audio sector: `parameter(group, unit)` is a unit's parameter byte
    /// and `bits(group, unit, index)` the four bits of one of its samples.
    fn sound(
        parameter: impl Fn(usize, usize) -> u8,
        bits: impl Fn(usize, usize, usize) -> u8,
    ) -> Vec<u8> {
        let mut sound = vec![0u8; AUDIO_PAYLOAD];
        for group in 0..SOUND_GROUPS {
            let bytes = &mut sound[group * SOUND_GROUP..][..SOUND_GROUP];
            for unit in 0..SOUND_UNITS {
                // The first four parameters have their copy before them, the others behind.
                let copy = if unit < 4 { unit } else { unit + 8 };
                bytes[4 + unit] = parameter(group, unit);
                bytes[copy] = parameter(group, unit);
                for index in 0..UNIT_SAMPLES {
                    bytes[SOUND_PARAMETERS + 4 * index + unit / 2] |=
                        (bits(group, unit, index) & 0x0F) << (4 * (unit & 1));
                }
            }
        }
        sound
    }

    fn audio(stereo: bool, sound: &[u8]) -> AudioSector<'_> {
        AudioSector {
            stereo,
            half_rate: true,
            sound,
        }
    }

    #[test]
    fn reads_the_parameters_of_a_sound_group() {
        let mut bytes = sound(|group, unit| (group * 8 + unit) as u8, |_, _, _| 0);
        let sector = audio(false, &bytes);
        let parameters = sector.parameters(2).unwrap();
        assert_eq!(
            parameters.map(|parameter| parameter.0),
            [16, 17, 18, 19, 20, 21, 22, 23]
        );
        assert_eq!(sector.parameters(SOUND_GROUPS), None);
        assert!((0..SOUND_GROUPS).all(|group| sector.copies_agree(group)));
        assert!(!sector.copies_agree(SOUND_GROUPS));

        // A copy that differs, once of the first four bytes and once of the last four.
        bytes[SOUND_GROUP + 1] ^= 1;
        bytes[3 * SOUND_GROUP + 15] ^= 1;
        let sector = audio(false, &bytes);
        let apart: Vec<usize> = (0..SOUND_GROUPS)
            .filter(|&group| !sector.copies_agree(group))
            .collect();
        assert_eq!(apart, [1, 3]);

        let parameter = SoundParameter(0x2B);
        assert_eq!((parameter.filter(), parameter.shift()), (2, 11));
        // The upper two bits are not part of the filter, and a shift above 12 is 9.
        let parameter = SoundParameter(0xFD);
        assert_eq!((parameter.filter(), parameter.shift()), (3, 9));
    }

    #[test]
    fn a_sample_is_its_four_bits_shifted_down() {
        // Filter 0 takes nothing from the samples before.
        let bytes = sound(
            |_, unit| [0x00, 0x0C, 0x0D][unit.min(2)],
            |group, unit, index| match (group, unit, index) {
                (0, 0 | 1, 0) => 7,
                (0, 0 | 1, 1) => 8,
                (0, 0 | 1, 2) => 0xF,
                (0, 2, 0) | (17, 7, 27) => 1,
                _ => 0,
            },
        );
        let mut decoder = SoundDecoder::new();
        let samples = decoder.decode(&audio(false, &bytes));
        assert_eq!(samples.len(), SECTOR_SAMPLES);
        assert_eq!(SECTOR_SAMPLES, 4032);
        // In a mono sector the units follow each other.
        assert_eq!(samples[..4], [28672, -32768, -4096, 0]);
        assert_eq!(samples[28..32], [7, -8, -1, 0]);
        assert_eq!(samples[56..58], [8, 0]);
        assert_eq!(samples[4031], 8);
        assert_eq!(samples.iter().filter(|&&sample| sample != 0).count(), 8);
        assert_eq!(decoder.limited(), 0);
    }

    #[test]
    fn a_sample_leans_on_the_two_before_it() {
        // (parameter of unit 0, bits of its samples, the samples).
        let cases: [(u8, &[u8], &[i16]); 3] = [
            (0x1C, &[1; 10], &[1, 2, 3, 4, 5, 6, 7, 8, 9, 9]),
            (0x2C, &[4, 0, 0, 0], &[4, 7, 9, 10]),
            // A share that is not whole is rounded down, below zero too.
            (0x3C, &[0xC, 0, 0, 0], &[-4, -6, -6, -4]),
        ];
        for (parameter, bits, wanted) in cases {
            let bytes = sound(
                |group, unit| {
                    if (group, unit) == (0, 0) {
                        parameter
                    } else {
                        0
                    }
                },
                |group, unit, index| match (group, unit) {
                    (0, 0) => bits.get(index).copied().unwrap_or(0),
                    _ => 0,
                },
            );
            let samples = SoundDecoder::new().decode(&audio(false, &bytes));
            assert_eq!(samples[..wanted.len()], *wanted, "{parameter:#04x}");
        }
    }

    #[test]
    fn the_samples_before_are_kept_for_each_side_and_from_sector_to_sector() {
        // Every unit with filter 1 and the bits 1: the sound climbs to 9 and stays there.
        let bytes = sound(|_, _| 0x1C, |_, _, _| 1);
        let mut decoder = SoundDecoder::new();
        let first = decoder.decode(&audio(false, &bytes));
        assert_eq!(first[..3], [1, 2, 3]);
        assert_eq!(first[27..30], [9, 9, 9]);
        let second = decoder.decode(&audio(false, &bytes));
        assert!(second.iter().all(|&sample| sample == 9));
        assert_eq!(SoundDecoder::new().decode(&audio(false, &bytes))[0], 1);

        // Stereo: the even units climb on the left, the odd ones stay at 2 on the right.
        let bytes = sound(
            |_, unit| if unit & 1 == 0 { 0x1C } else { 0x0C },
            |_, unit, _| if unit & 1 == 0 { 1 } else { 2 },
        );
        let mut decoder = SoundDecoder::new();
        let samples = decoder.decode(&audio(true, &bytes));
        assert_eq!(samples.len(), SECTOR_SAMPLES);
        assert_eq!(samples[..6], [1, 2, 2, 2, 3, 2]);
        // Unit 2 goes on behind unit 0 on the left side.
        assert_eq!(samples[2 * 28..2 * 28 + 2], [9, 2]);
        let pairs = samples.as_chunks::<2>().0;
        assert!(pairs.iter().all(|pair| pair[1] == 2));
        assert!(pairs.iter().skip(8).all(|pair| pair[0] == 9));
    }

    #[test]
    fn a_sample_is_kept_inside_sixteen_bits() {
        let bytes = sound(
            |_, _| 0x10,
            |group, unit, index| match (group, unit, index) {
                (0, 0, 0 | 1) => 7,
                _ => 0,
            },
        );
        let mut decoder = SoundDecoder::new();
        let samples = decoder.decode(&audio(false, &bytes));
        // 28672, then 28672 and 60 64ths of the sample before: 55552.
        assert_eq!(samples[..2], [28672, 32767]);
        assert_eq!(decoder.limited(), 1);
    }

    #[test]
    fn puts_the_sectors_of_a_frame_together() {
        let mut demuxer = Demuxer::new();
        let size = 2 * VIDEO_PAYLOAD as u32 + 100;
        assert_eq!(demuxer.push(&video_of(&video(1, 0, 3, size, 1))), None);
        assert_eq!(demuxer.push(&video_of(&video(1, 1, 3, size, 2))), None);
        assert!(demuxer.is_waiting());
        let frame = demuxer.push(&video_of(&video(1, 2, 3, size, 3))).unwrap();
        assert_eq!((frame.number, frame.width, frame.height), (1, 320, 240));
        assert_eq!(frame.bitstream.len(), size as usize);
        assert_eq!(frame.bitstream[VIDEO_PAYLOAD - 1..=VIDEO_PAYLOAD], [1, 2]);
        assert_eq!(frame.bitstream[size as usize - 1], 3);
        assert!(!demuxer.is_waiting());

        // A frame of one sector is whole at once.
        let frame = demuxer.push(&video_of(&video(2, 0, 1, 8, 4))).unwrap();
        assert_eq!(frame.bitstream, [4; 8]);
        assert_eq!(demuxer.dropped(), 0);
    }

    #[test]
    fn leaves_out_a_frame_that_is_not_whole() {
        // A stream that starts on the second sector of its first frame.
        let mut demuxer = Demuxer::new();
        assert_eq!(demuxer.push(&video_of(&video(1, 1, 3, 100, 0))), None);
        assert_eq!(demuxer.push(&video_of(&video(1, 2, 3, 100, 0))), None);
        assert_eq!(demuxer.dropped(), 1);
        assert!(demuxer.push(&video_of(&video(2, 0, 1, 100, 0))).is_some());

        // A frame cut short by the next one, which is kept.
        assert_eq!(demuxer.push(&video_of(&video(3, 0, 3, 100, 0))), None);
        assert_eq!(demuxer.push(&video_of(&video(4, 0, 2, 100, 0))), None);
        assert_eq!(demuxer.dropped(), 2);
        assert_eq!(
            demuxer
                .push(&video_of(&video(4, 1, 2, 100, 5)))
                .unwrap()
                .number,
            4
        );

        // A sector out of turn ends the frame in the making.
        assert_eq!(demuxer.push(&video_of(&video(5, 0, 3, 100, 0))), None);
        assert_eq!(demuxer.push(&video_of(&video(5, 2, 3, 100, 0))), None);
        assert_eq!(demuxer.dropped(), 3);
        assert!(!demuxer.is_waiting());
    }

    fn frame(number: u32) -> Frame {
        Frame {
            number,
            width: 320,
            height: 240,
            bitstream: Vec::new(),
        }
    }

    #[test]
    fn shows_a_frame_when_the_next_one_has_come() {
        let mut data = vec![0u8; RECORD_SIZE];
        data[0x0A..0x0C].copy_from_slice(&5u16.to_le_bytes());
        let table = MovieTable::parse(&data, 0x8001_0000, 0x8001_0000, 1).unwrap();
        let record = table.get(0).unwrap();
        assert_eq!(record.sectors_a_second(), 75);

        // The stream starts with frame 2: nothing is on screen until frame 3 has come.
        let mut screening = Screening::new(record);
        assert_eq!(screening.arrived(frame(2)), Arrival::default());
        assert_eq!(screening.waiting(), Some(2));
        let shown = |arrival: Arrival| (arrival.show.map(|frame| frame.number), arrival.ended);
        assert_eq!(shown(screening.arrived(frame(3))), (Some(2), false));
        assert_eq!(shown(screening.arrived(frame(4))), (Some(3), false));
        // The end frame brings the last picture and is not shown itself.
        assert_eq!(shown(screening.arrived(frame(5))), (Some(4), true));
        assert!(screening.has_ended());
        assert_eq!(screening.waiting(), None);
        assert_eq!(shown(screening.arrived(frame(6))), (None, true));

        // A frame behind the end frame ends the movie as well.
        let mut screening = Screening::new(record);
        screening.arrived(frame(4));
        assert_eq!(shown(screening.arrived(frame(7))), (Some(4), true));
    }

    /// Gives every record of the wide table whose index starts with `bits` (`length` of
    /// them) the bits to take and the codes.
    fn wide(words: &mut [u16], bits: usize, length: usize, taken: u16, codes: [u16; 3]) {
        for rest in 0..1 << (13 - length) {
            let at = WIDE_RECORDS + 4 * (bits << (13 - length) | rest);
            words[at] = taken;
            words[at + 1..at + 4].copy_from_slice(&codes);
        }
    }

    /// A small table: `10` ends a block, `110` and `111` are the values 1 and -1 without
    /// zeros before them, `11010` both of the first two at once, `000001` the escape,
    /// and eight zeros with `1` behind them (three bits taken) five behind two zeros.
    fn table() -> CodeTable {
        let mut words = vec![0u16; CODE_TABLE_WORDS];
        wide(&mut words, 0b10, 2, 2, [END_OF_BLOCK, 0, 0]);
        wide(&mut words, 0b110, 3, 3, [1, 0, 0]);
        wide(&mut words, 0b111, 3, 3, [0x3FF, 0, 0]);
        wide(&mut words, 0b11010, 5, 5, [1, END_OF_BLOCK, 0]);
        wide(&mut words, 0b000001, 6, 6, [ESCAPE, 0, 0]);
        for rest in 0..256 {
            let at = NARROW_RECORDS + 2 * (0x100 | rest);
            words[at] = 3;
            words[at + 1] = 2 << 10 | 5;
        }
        CodeTable::from_words(words).unwrap()
    }

    /// A bitstream of the four header words and the given runs of bits (value, count).
    fn stream(words: u16, scale: u16, version: u16, runs: &[(u32, usize)]) -> Vec<u8> {
        let mut bits = Vec::new();
        for &(value, count) in runs {
            bits.extend((0..count).rev().map(|bit| value >> bit & 1 == 1));
        }
        let mut stream = Vec::new();
        for word in [words, BITSTREAM_MARKER, scale, version] {
            stream.extend_from_slice(&word.to_le_bytes());
        }
        for chunk in bits.chunks(16) {
            let word = chunk
                .iter()
                .enumerate()
                .fold(0u16, |word, (at, &bit)| word | u16::from(bit) << (15 - at));
            stream.extend_from_slice(&word.to_le_bytes());
        }
        stream
    }

    #[test]
    fn expands_a_bitstream_to_its_codes() {
        let runs = [
            // First block: 5, then 1, -1, a code written out, the end.
            (5, 10),
            (0b110, 3),
            (0b111, 3),
            (0b000001, 6),
            (0xABCD, 16),
            (0b10, 2),
            // Second block: one record gives a value and the end.
            (0x3FE, 10),
            (0b11010, 5),
            // Third block: a code of the narrow table.
            (0, 10),
            (0, 8),
            (0b101, 3),
            (0b10, 2),
            (END_OF_FRAME, 10),
        ];
        let codes = table().expand(&stream(8, 3, 2, &runs)).unwrap();
        assert_eq!((codes.header.words, codes.header.scale), (8, 3));
        assert_eq!(codes.used, 11);
        assert_eq!(
            codes.codes[..11],
            [
                0x0C05,
                1,
                0x3FF,
                0xABCD,
                END_OF_BLOCK,
                0x0FFE,
                1,
                END_OF_BLOCK,
                0x0C00,
                2 << 10 | 5,
                END_OF_BLOCK
            ]
        );
        assert_eq!(codes.codes[11..], [END_OF_BLOCK; 5]);
        let blocks: Vec<&[u16]> = codes.blocks().collect();
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0], [0x0C05, 1, 0x3FF, 0xABCD]);
        assert_eq!(blocks[2], [0x0C00, 2 << 10 | 5]);

        // A first value with the bits of an end code is a first value.
        let odd = table()
            .expand(&stream(
                2,
                63,
                2,
                &[(0x200, 10), (0b110, 3), (0b10, 2), (END_OF_FRAME, 10)],
            ))
            .unwrap();
        assert_eq!(odd.codes, [END_OF_BLOCK, 1, END_OF_BLOCK, END_OF_BLOCK]);
        assert_eq!(odd.blocks().collect::<Vec<_>>(), [[END_OF_BLOCK, 1]]);
    }

    #[test]
    fn tells_why_a_bitstream_does_not_expand() {
        let table = table();
        assert_eq!(table.expand(&[0; 7]), Err(BitstreamError::NoHeader));
        assert_eq!(
            table.expand(&stream(8, 3, 3, &[(END_OF_FRAME, 10)])),
            Err(BitstreamError::Version(3))
        );
        // Bits no record of either table knows: at bit 10, behind the first value.
        assert_eq!(
            table.expand(&stream(8, 3, 2, &[(5, 10), (0, 8), (0, 9), (0xFFFF, 16)])),
            Err(BitstreamError::NoCode(10))
        );
        // No end mark: the stream runs out.
        assert!(matches!(
            table.expand(&stream(8, 3, 2, &[(5, 10), (0b110, 3)])),
            Err(BitstreamError::NoCode(_))
        ));
        // Three codes where the header has room for two.
        assert_eq!(
            table.expand(&stream(
                1,
                3,
                2,
                &[(5, 10), (0b110, 3), (0b10, 2), (END_OF_FRAME, 10)]
            )),
            Err(BitstreamError::TooLong(3))
        );
        // A frame without a block is only filling.
        let empty = table
            .expand(&stream(1, 3, 2, &[(END_OF_FRAME, 10)]))
            .unwrap();
        assert_eq!((empty.used, empty.codes.len()), (0, 2));
        assert_eq!(empty.blocks().count(), 0);
    }

    #[test]
    fn unpacks_a_packed_table() {
        // Four bytes given, the same four copied from four back, two more given, the end.
        let packed = [3, 1, 0, 2, 0, 0xF1, 0x03, 3, 240, 1, 7, 0, 0xFF, 0xFF];
        // Every word from the fifth on is combined with the word four before it.
        assert_eq!(unpack(&packed, 6), Some(vec![1, 2, 1, 2, 7 ^ 1, 2]));
        assert_eq!(unpack(&packed[..packed.len() - 1], 6), None);
        // A copy from before the start of the table.
        assert_eq!(unpack(&[0xF1, 0x00, 0], 6), None);
        assert_eq!(CodeTable::from_words(vec![0; 6]), None);
    }

    /// A decoder with the cosine table worked out here and flat quantisation tables: 2
    /// for the first value of a block, 16 for the others.
    fn decoder() -> PictureDecoder {
        let mut transform = [0i16; 64];
        for (at, entry) in transform.iter_mut().enumerate() {
            let (frequency, pixel) = ((at / 8) as f64, (at % 8) as f64);
            let angle = (2.0 * pixel + 1.0) * frequency * std::f64::consts::PI / 16.0;
            let size = if at < 8 { 0.5f64.sqrt() } else { 1.0 };
            *entry = (32768.0 * size * angle.cos()).round().min(32767.0) as i16;
        }
        let mut table = [16u8; 64];
        table[0] = 2;
        PictureDecoder {
            brightness: table,
            colour: table,
            transform,
        }
    }

    /// The codes of a frame from its blocks, each closed with the end code.
    fn codes(blocks: &[&[u16]]) -> Codes {
        let mut codes = Vec::new();
        for block in blocks {
            codes.extend_from_slice(block);
            codes.push(END_OF_BLOCK);
        }
        let used = codes.len();
        Codes {
            header: BitstreamHeader {
                words: used.div_ceil(2) as u16,
                marker: BITSTREAM_MARKER,
                scale: 1,
                version: 2,
            },
            codes,
            used,
        }
    }

    /// A first value of a block at scale 1.
    fn first(value: i16) -> u16 {
        1 << 10 | (value as u16 & 0x3FF)
    }

    #[test]
    fn the_values_of_a_block_come_in_diagonals() {
        assert_eq!(ZIGZAG[..10], [0, 1, 8, 16, 9, 2, 3, 10, 17, 24]);
        assert_eq!(ZIGZAG[60..], [47, 55, 62, 63]);
        let mut seen = ZIGZAG;
        seen.sort_unstable();
        assert!(seen.iter().copied().eq(0..64));
    }

    #[test]
    fn multiplies_the_values_of_a_block_back_up() {
        let mut table = [16u8; 64];
        table[0] = 2;
        table[2] = 19;
        // Scale 3: first value -5, then 7 at once, two zeros and -3, and a pair that
        // leads behind the block, which ends it whatever follows.
        let block = [
            3 << 10 | (-5i16 as u16 & 0x3FF),
            7,
            2 << 10 | (-3i16 as u16 & 0x3FF),
            62 << 10 | 1,
            9,
        ];
        let values = PictureDecoder::values(&block, &table);
        let mut wanted = [0; 64];
        wanted[0] = -10;
        // Second in the order is the place right of the corner, fifth the one below that.
        wanted[1] = (7 * 16 * 3 + 4) >> 3;
        wanted[9] = (-3 * 16 * 3 + 4) >> 3;
        assert_eq!(values, wanted);
        assert_eq!((wanted[1], wanted[9]), (42, -18));

        // The third place takes its own entry of the table, and a value stays in 11 bits.
        let values = PictureDecoder::values(&[63 << 10 | 0x1FF, 1 << 10 | 0x1FF], &table);
        assert_eq!((values[0], values[8]), (1022, 0x3FF));
        let values = PictureDecoder::values(&[first(1), 1 << 10 | 8], &table);
        assert_eq!(values[8], (8 * 19 + 4) / 8);

        // Without a scale the values are doubled and stay in rows.
        let values = PictureDecoder::values(&[4, 1 << 10 | 6], &table);
        assert_eq!((values[0], values[2], values[8]), (8, 12, 0));
        assert_eq!(PictureDecoder::values(&[], &table), [0; 64]);
    }

    #[test]
    fn transforms_a_block_to_its_samples() {
        let decoder = decoder();
        // A first value alone is a flat block of an eighth of it.
        let mut values = [0; 64];
        values[0] = 800;
        assert_eq!(decoder.samples(&values), [100; 64]);

        // The value right of the corner is a wave from left to right, the same in every
        // row; the value below the corner the same wave from the top down.
        let mut across = [0; 64];
        across[1] = 400;
        let wave = decoder.samples(&across);
        for at in 0..64 {
            assert_eq!(wave[at], wave[at % 8], "row {}", at / 8);
        }
        assert!(wave[0] > wave[1] && wave[1] > wave[2] && wave[2] > wave[3] && wave[3] > 0);
        for x in 0..4 {
            assert!((wave[x] + wave[7 - x]).abs() <= 1, "{wave:?}");
        }
        assert!((wave[0] - 69).abs() <= 1, "{}", wave[0]);
        let mut down = [0; 64];
        down[8] = 400;
        let turned = decoder.samples(&down);
        for at in 0..64 {
            assert!((turned[at] - wave[at / 8]).abs() <= 1, "place {at}");
        }
    }

    #[test]
    fn puts_the_blocks_of_a_frame_into_a_picture() {
        let decoder = decoder();
        let none = first(0);
        // Two pieces one below the other, then two more to their right. Brightness by
        // quarter in the first piece, one brightness in each of the others.
        let pieces: [[i16; 4]; 4] = [[-400, -200, 200, 400], [-100; 4], [100; 4], [300; 4]];
        let mut blocks = Vec::new();
        for piece in pieces {
            blocks.extend([none, none]);
            blocks.extend(piece.map(first));
        }
        let blocks: Vec<&[u16]> = blocks.iter().map(std::slice::from_ref).collect();
        let picture = decoder.decode(&codes(&blocks), 32, 32).unwrap();
        assert_eq!((picture.width, picture.height), (32, 32));
        assert_eq!(picture.rgb.len(), 3 * 32 * 32);
        // A first value v at table entry 2 is a brightness of v / 4 around the middle.
        let grey = |value: i32| [(128 + value / 4) as u8; 3];
        assert_eq!(picture.pixel(0, 0), grey(-400));
        assert_eq!(picture.pixel(7, 7), grey(-400));
        assert_eq!(picture.pixel(8, 7), grey(-200));
        assert_eq!(picture.pixel(7, 8), grey(200));
        assert_eq!(picture.pixel(15, 15), grey(400));
        assert_eq!(picture.pixel(0, 16), grey(-100));
        assert_eq!(picture.pixel(15, 31), grey(-100));
        assert_eq!(picture.pixel(16, 0), grey(100));
        assert_eq!(picture.pixel(31, 31), grey(300));

        // A picture that is not whole pieces is cut from them.
        let cut = decoder.decode(&codes(&blocks[..12]), 9, 20).unwrap();
        assert_eq!(cut.rgb.len(), 3 * 9 * 20);
        assert_eq!(
            (cut.pixel(8, 0), cut.pixel(8, 19)),
            (grey(-200), grey(-100))
        );

        assert_eq!(
            decoder.decode(&codes(&blocks[..23]), 32, 32),
            Err(PictureError::Blocks {
                wanted: 24,
                found: 23
            })
        );
    }

    #[test]
    fn colours_a_piece_from_its_two_colour_blocks() {
        let decoder = decoder();
        let piece = |red: i16, blue: i16| {
            let blocks = [
                first(red),
                first(blue),
                first(0),
                first(0),
                first(0),
                first(0),
            ];
            let blocks: Vec<&[u16]> = blocks.iter().map(std::slice::from_ref).collect();
            decoder.decode(&codes(&blocks), 16, 16).unwrap().pixel(5, 9)
        };
        assert_eq!(piece(0, 0), [128; 3]);
        // The first block is the red side: more red, less green, blue as it was.
        assert_eq!(piece(200, 0), [128 + 70, 128 - 36, 128]);
        // The second the blue side.
        assert_eq!(piece(0, 200), [128, 128 - 17, 128 + 89]);
        // A colour stops at the ends of its range.
        assert_eq!(piece(-400, 400), [0, 128 + 37, 255]);

        // One colour sample serves two by two pixels: a wave across the red block shows
        // in steps of two pixels.
        let blocks = [first(0), first(0), first(0), first(0), first(0), first(0)];
        let mut blocks: Vec<Vec<u16>> = blocks.iter().map(|&block| vec![block]).collect();
        blocks[0].push(40);
        let blocks: Vec<&[u16]> = blocks.iter().map(Vec::as_slice).collect();
        let picture = decoder.decode(&codes(&blocks), 16, 16).unwrap();
        for x in (0..16).step_by(2) {
            assert_eq!(picture.pixel(x, 3), picture.pixel(x + 1, 4));
        }
        assert!(picture.pixel(0, 0)[0] > picture.pixel(6, 0)[0]);
        assert!(picture.pixel(6, 0)[0] > picture.pixel(15, 0)[0]);
    }

    #[test]
    fn shows_a_picture_in_the_depth_of_its_movie() {
        let picture = Picture {
            width: 2,
            height: 1,
            rgb: vec![0, 7, 8, 127, 200, 255],
        };
        assert_eq!(
            picture.rgba(Depth::TwentyFour),
            [0, 7, 8, 255, 127, 200, 255, 255]
        );
        // Five bits of each colour, spread over the whole range again.
        assert_eq!(
            picture.rgba(Depth::Sixteen),
            [0, 0, 8, 255, 123, 206, 255, 255]
        );
    }

    #[test]
    fn reads_the_tables_of_the_picture_decoder() {
        let base = 0x8001_0000;
        let mut data = vec![0u8; 8];
        data.extend_from_slice(&QUANTISATION_COMMAND.to_le_bytes());
        data.extend((0..64).map(|at| at as u8 + 1));
        data.extend((0..64).map(|at| 200 - at as u8));
        data.extend_from_slice(&TRANSFORM_COMMAND.to_le_bytes());
        for at in 0..64i16 {
            data.extend_from_slice(&(100 * at - 3000).to_le_bytes());
        }
        let decoder = PictureDecoder::parse(&data, base, base + 8).unwrap();
        assert_eq!((decoder.brightness[0], decoder.brightness[63]), (1, 64));
        assert_eq!((decoder.colour[0], decoder.colour[63]), (200, 137));
        assert_eq!((decoder.transform[0], decoder.transform[63]), (-3000, 3300));

        assert_eq!(
            PictureDecoder::parse(&data, base, base + 4),
            Err(MovieError::NoTables(base + 4))
        );
        let short = &data[..data.len() - 1];
        assert_eq!(
            PictureDecoder::parse(short, base, base + 8),
            Err(MovieError::OutOfImage(base + 8 + 4 + 128 + 4 + 126))
        );
    }

    #[test]
    fn reads_the_records_of_a_table() {
        let base = 0x8001_0000;
        let mut data = vec![0u8; 0x10 + 2 * RECORD_SIZE];
        let record = &mut data[0x10 + RECORD_SIZE..];
        record[0x00..0x04].copy_from_slice(&1107u32.to_le_bytes());
        record[0x04..0x08].copy_from_slice(&1u32.to_le_bytes());
        for (at, value) in [
            (0x08, 1i16),
            (0x0A, 2173),
            (0x0C, 9),
            (0x0E, 13568),
            (0x10, 16),
            (0x12, 40),
            (0x14, 480),
            (0x16, 160),
            (0x18, 0),
            (0x1A, 0),
            (0x1C, 512),
            (0x1E, 240),
            (0x20, -4),
            (0x24, 256),
            (0x26, 240),
            (0x2C, 4096),
            (0x2E, 2048),
            (0x30, 1),
        ] {
            record[at..at + 2].copy_from_slice(&value.to_le_bytes());
        }
        record[0x28..0x2C].copy_from_slice(&0x800u32.to_le_bytes());

        let table = MovieTable::parse(&data, base, base + 0x10, 2).unwrap();
        assert_eq!(table.len(), 2);
        let first = table.get(0).unwrap();
        assert_eq!((first.file, first.depth), (0, Depth::Sixteen));
        assert!(!first.can_be_skipped() && first.is_silent() && !first.double_speed);
        let second = table.get(1).unwrap();
        assert_eq!((second.file, second.depth), (1107, Depth::TwentyFour));
        assert_eq!((second.first_frame, second.end_frame), (1, 2173));
        assert_eq!((second.most_sectors, second.unknown), (9, 13568));
        // Full volume is 112 of the 127 the library takes.
        assert_eq!(second.sound_volume(), (112, 56));
        assert_eq!(first.sound_volume(), (0, 0));
        assert_eq!(
            second.picture,
            Area {
                x: 16,
                y: 40,
                width: 480,
                height: 160
            }
        );
        assert_eq!((second.display.width, second.display.height), (512, 240));
        assert_eq!((second.screen_x, second.screen), (-4, (256, 240)));
        assert_eq!((second.skip, second.volume), (0x800, (4096, 2048)));
        assert!(second.can_be_skipped() && !second.is_silent() && second.double_speed);
        assert!(second.skipped_by(crate::pad::START) && !second.skipped_by(crate::pad::CROSS));
        assert!(!first.skipped_by(u16::MAX));
        assert!(table.get(2).is_none());

        assert_eq!(
            MovieTable::parse(&data, base, base + 0x10, 3),
            Err(MovieError::OutOfImage(base + 0x10 + 2 * RECORD_SIZE as u32))
        );
    }
}
