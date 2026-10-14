
use std::fmt;

use crate::movie::{self, Sector, SoundDecoder};
use crate::scene::ScenePackage;

/// The record of the file table that names the file's place on the disc.
pub const FILE_RECORD: usize = 1081;
/// The file's path on the disc.
pub const PATH: &str = "/DATA/VOICE.XA";
/// Channels of the file, and so the sectors from one row of a channel to its next.
pub const CHANNELS: u32 = 32;
/// Samples a second of a line, one side.
pub const RATE: u32 = 18_900;
/// Samples of a row.
pub const ROW_SAMPLES: usize = movie::SECTOR_SAMPLES;
/// Object type of a room's cue table.
pub const OBJECT_CUES: u32 = 0x0B;
/// The `a` of script command `0x67` that asks for the line at its `c`.
pub const CALL_LINE: i32 = 16;
/// Bytes of a cue table's record.
const CUE_SIZE: usize = 8;
/// Bytes of a line: three 16-bit values.
const LINE_SIZE: usize = 6;
/// Where the subheader's channel byte lies in a raw sector.
const CHANNEL_BYTE: usize = 17;

/// Why a sector is not a row of a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceError {
    /// The sector is not a raw sector, by its size.
    Sector,
    /// The sector holds no sound, or sound of another kind than a line's.
    NotALine,
    /// The sector belongs to another channel.
    Channel { wanted: u16, found: u8 },
}

impl fmt::Display for VoiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sector => write!(f, "not a raw sector"),
            Self::NotALine => write!(f, "the sector holds no sound of a voice line"),
            Self::Channel { wanted, found } => {
                write!(
                    f,
                    "a sector of channel {found} where channel {wanted} is played"
                )
            }
        }
    }
}

impl std::error::Error for VoiceError {}

/// A line of the file: what the setter `0x80040E00` is handed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Line {
    /// 0 to 31.
    pub channel: u16,
    /// The first row.
    pub start: u16,
    /// How many rows the line is long.
    pub rows: u16,
}

impl Line {
    /// Reads the three 16-bit values of a line. `None` for fewer bytes, and for values
    /// that are no line: a channel the file has not, a negative row, no length.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let bytes = bytes.get(..LINE_SIZE)?;
        let half = |at: usize| i16::from_le_bytes([bytes[at], bytes[at + 1]]);
        let (channel, start, rows) = (half(0), half(2), half(4));
        (0..CHANNELS as i16).contains(&channel).then_some(())?;
        (start >= 0 && rows > 0).then_some(Self {
            channel: channel as u16,
            start: start as u16,
            rows: rows as u16,
        })
    }

    /// The sectors of the line, counted from the file's first: one a row.
    pub fn sectors(&self) -> impl Iterator<Item = u32> + use<> {
        let (channel, start) = (u32::from(self.channel), u32::from(self.start));
        (0..u32::from(self.rows)).map(move |row| CHANNELS * (start + row) + channel)
    }

    /// The row behind the line's last.
    pub fn end(&self) -> u32 {
        u32::from(self.start) + u32::from(self.rows)
    }

    /// How many samples the line is long.
    pub fn samples(&self) -> usize {
        usize::from(self.rows) * ROW_SAMPLES
    }

    /// How long the line lasts.
    pub fn seconds(&self) -> f32 {
        self.samples() as f32 / RATE as f32
    }
}

/// A record of a cue table: the line of one page of a spoken text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cue {
    /// The group of the room's text.
    pub group: u8,
    /// The cue in front of the page.
    pub cue: u8,
    pub line: Line,
}

/// The cue table at an address of a scene package: what its object of type
/// [`OBJECT_CUES`] names. The table ends with a negative channel. `None` when it
/// leaves the code image or a record names no line.
pub fn cues(package: &ScenePackage, address: u32) -> Option<Vec<Cue>> {
    let mut cues = Vec::new();
    let mut at = address;
    loop {
        let bytes = package.bytes_at(at, CUE_SIZE)?;
        if i16::from_le_bytes([bytes[2], bytes[3]]) < 0 {
            return Some(cues);
        }
        cues.push(Cue {
            group: bytes[0],
            cue: bytes[1],
            line: Line::parse(&bytes[2..])?,
        });
        at = at.checked_add(CUE_SIZE as u32)?;
    }
}

/// The line a script's `0x67` with [`CALL_LINE`] names at its `c`.
pub fn line_at(package: &ScenePackage, address: u32) -> Option<Line> {
    Line::parse(package.bytes_at(address, LINE_SIZE)?)
}

/// The record the scripts' waiter `0x800BC47C` picks for a window: the last one of the
/// group on screen whose cue the window has reached.
pub fn spoken(cues: &[Cue], group: u32, reached: impl Fn(u8) -> bool) -> Option<&Cue> {
    (cues.iter()).rfind(|cue| u32::from(cue.group) == group && reached(cue.cue))
}

/// Makes the samples of a line from its sectors, row by row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Speaker {
    channel: u16,
    decoder: SoundDecoder,
}

impl Speaker {
    /// For the rows of one line, in their order.
    pub fn new(line: &Line) -> Self {
        Self {
            channel: line.channel,
            decoder: SoundDecoder::new(),
        }
    }

    /// How many samples so far lay outside 16 bits.
    pub fn limited(&self) -> u64 {
        self.decoder.limited()
    }

    /// The [`ROW_SAMPLES`] samples of the next row, from its raw sector.
    pub fn row(&mut self, raw: &[u8]) -> Result<Vec<i16>, VoiceError> {
        let Sector::Audio(sector) = Sector::parse(raw).map_err(|_| VoiceError::Sector)? else {
            return Err(VoiceError::NotALine);
        };
        if sector.stereo || !sector.half_rate {
            return Err(VoiceError::NotALine);
        }
        let found = raw[CHANNEL_BYTE];
        if u16::from(found) != self.channel {
            return Err(VoiceError::Channel {
                wanted: self.channel,
                found,
            });
        }
        Ok(self.decoder.decode(&sector))
    }
}

/// The CD volume a line plays at, of [`movie::LOUDEST`]: the movies' full value.
pub const VOLUME: i16 = 112;
/// What a pass of the fade takes off the volume.
const FADE_STEP: i16 = 14;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// `0x80040984`: nothing plays.
    Idle,
    /// `0x80040A60`: the drive was told the line's channel.
    Filter(Line),
    /// `0x80040AA0`: the drive is sent to the line's first sector and plays from there.
    Seek(Line),
    /// `0x80040B88`: the line plays.
    Playing(Line),
    /// `0x80040B00`: the volume goes down, the drive reads on.
    Fade(i16),
}

/// What a pass of the task asks of the drive and of the CD volume.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pass {
    /// The drive starts to play this line.
    pub starts: Option<Line>,
    /// The CD volume of both sides from this pass on, of [`movie::LOUDEST`].
    pub volume: Option<u8>,
    /// The drive is paused: nothing more is heard.
    pub paused: bool,
}

/// The task that plays the lines, without the drive. Stand-in: every answer of the
/// drive is there in the pass behind the one that asked for it, and the archive's
/// reader never holds the drive; how long the console's drive takes is not known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Task {
    /// The line asked for that has not started (`0x80095548` is not negative).
    asked: Option<Line>,
    /// Bit 0 of the record's flag word: an end was asked.
    end: bool,
    state: State,
}

impl Default for Task {
    fn default() -> Self {
        Self::new()
    }
}

impl Task {
    pub fn new() -> Self {
        Self {
            asked: None,
            end: false,
            state: State::Idle,
        }
    }

    /// The setter `0x80040E00`: a line is asked for. One asked for before that has
    /// not started is forgotten.
    pub fn ask(&mut self, line: Line) {
        self.asked = Some(line);
    }

    /// The end `0x80040DC0`: the line that plays is to fade. Does nothing and answers
    /// false while a line is asked for that has not started.
    pub fn end(&mut self) -> bool {
        self.end |= self.asked.is_none();
        self.asked.is_none()
    }

    /// `0x80040E20`: no line plays, starts or fades.
    pub fn is_silent(&self) -> bool {
        self.state == State::Idle
    }

    /// `0x80040E40`: no line is asked for that has not started.
    pub fn none_asked(&self) -> bool {
        self.asked.is_none()
    }

    /// The line that plays, while it has not begun to fade.
    pub fn playing(&self) -> Option<Line> {
        match self.state {
            State::Playing(line) => Some(line),
            _ => None,
        }
    }

    /// One pass. `behind` is the drive's answer: its place is behind the last sector
    /// of the line that plays.
    pub fn pass(&mut self, behind: bool) -> Pass {
        let mut pass = Pass::default();
        match self.state {
            State::Idle => {
                if self.end {
                    // An end asked while nothing plays only takes back what was asked.
                    self.end = false;
                    self.asked = None;
                }
                if let Some(line) = self.asked.take() {
                    self.state = State::Filter(line);
                }
            }
            State::Filter(line) => self.state = State::Seek(line),
            State::Seek(line) => {
                pass.starts = Some(line);
                self.state = State::Playing(line);
            }
            State::Playing(_) => {
                if behind || self.asked.is_some() {
                    self.state = State::Fade(VOLUME);
                } else {
                    pass.volume = Some(VOLUME as u8);
                    if self.end {
                        self.end = false;
                        self.state = State::Fade(VOLUME);
                    }
                }
            }
            State::Fade(volume) => {
                let volume = volume - FADE_STEP;
                pass.volume = Some(volume.max(0) as u8);
                if volume < 0 {
                    pass.paused = true;
                    self.state = State::Idle;
                } else {
                    self.state = State::Fade(volume);
                }
            }
        }
        pass
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::PACKAGE_BASE;

    #[test]
    fn the_task_starts_a_line_plays_it_and_fades_it_over_eight_passes() {
        let read = |channel| Line::parse(&line(channel, 10, 6)).expect("line");
        let mut task = Task::new();
        assert!(task.is_silent() && task.none_asked());
        assert_eq!(task.pass(false), Pass::default());

        task.ask(read(3));
        assert!(task.is_silent() && !task.none_asked());
        // The end does nothing while the line has not started.
        assert!(!task.end());
        // The starter, the drive's answer, the seek: the drive plays in the third pass.
        assert_eq!(task.pass(false), Pass::default());
        assert!(!task.is_silent() && task.none_asked());
        assert_eq!(task.pass(false), Pass::default());
        assert_eq!(task.pass(false).starts, Some(read(3)));
        assert_eq!(task.playing(), Some(read(3)));
        // The volume comes with the drive's first answer of its place.
        assert_eq!(task.pass(false).volume, Some(112));
        assert_eq!(task.pass(false).volume, Some(112));

        // Behind the last sector: the fade, 98 down to 0, and the pause in the ninth.
        assert_eq!(task.pass(true), Pass::default());
        assert_eq!(task.playing(), None);
        let volumes: Vec<_> = (0..8).map(|_| task.pass(true).volume).collect();
        assert_eq!(volumes.first(), Some(&Some(98)));
        assert_eq!(volumes.last(), Some(&Some(0)));
        assert!(!task.is_silent());
        let last = task.pass(true);
        assert_eq!((last.volume, last.paused), (Some(0), true));
        assert!(task.is_silent());
    }

    #[test]
    fn an_end_and_a_new_line_fade_the_line_that_plays() {
        let read = |channel| Line::parse(&line(channel, 10, 6)).expect("line");
        let through = |task: &mut Task, passes: usize| {
            (0..passes).map(|_| task.pass(false)).collect::<Vec<_>>()
        };
        let mut task = Task::new();
        task.ask(read(3));
        through(&mut task, 4);
        // An end asked: this pass still sets the volume, the next is the fade's first.
        assert!(task.end());
        assert_eq!(task.pass(false).volume, Some(112));
        assert_eq!(task.pass(false).volume, Some(98));
        through(&mut task, 8);
        assert!(task.is_silent());

        // A new line while one plays: the old one fades first, the new starts from idle.
        task.ask(read(3));
        through(&mut task, 4);
        task.ask(read(9));
        let passes = through(&mut task, 10);
        assert!(passes.iter().all(|pass| pass.starts.is_none()));
        assert!(passes[9].paused && task.is_silent() && !task.none_asked());
        let passes = through(&mut task, 3);
        assert_eq!(passes[2].starts, Some(read(9)));

        // An end asked while nothing plays and nothing is asked for is kept, and the
        // idle pass takes back a line asked for behind it.
        let mut task = Task::new();
        assert!(task.end());
        task.ask(read(1));
        assert_eq!(task.pass(false), Pass::default());
        assert!(task.none_asked() && task.is_silent());
    }

    fn line(channel: i16, start: i16, rows: i16) -> Vec<u8> {
        [channel, start, rows]
            .iter()
            .flat_map(|half| half.to_le_bytes())
            .collect()
    }

    /// A raw sector of sound: file number 1, a channel, a submode and a coding byte.
    fn sector(channel: u8, submode: u8, coding: u8) -> Vec<u8> {
        let mut raw = vec![0; movie::RAW_SECTOR];
        raw[16..20].copy_from_slice(&[1, channel, submode, coding]);
        raw
    }

    #[test]
    fn a_line_is_a_channel_a_row_and_a_length() {
        let read = Line::parse(&line(19, 545, 37)).expect("line");
        assert_eq!((read.channel, read.start, read.rows), (19, 545, 37));
        // Row r of channel c is sector 32 r + c.
        let sectors: Vec<u32> = read.sectors().collect();
        assert_eq!(sectors.len(), 37);
        assert_eq!((sectors[0], sectors[1]), (32 * 545 + 19, 32 * 546 + 19));
        assert_eq!(read.end(), 582);
        assert_eq!(read.samples(), 37 * 4032);
        assert!((read.seconds() - 7.893).abs() < 0.001);
    }

    #[test]
    fn what_is_no_line_is_none() {
        assert_eq!(Line::parse(&line(32, 0, 5)), None);
        assert_eq!(Line::parse(&line(-1, 0, 5)), None);
        assert_eq!(Line::parse(&line(3, -1, 5)), None);
        assert_eq!(Line::parse(&line(3, 0, 0)), None);
        assert_eq!(Line::parse(&line(3, 0, 5)[..5]), None);
    }

    /// A package whose code image is a cue table of two groups, its end, and a line.
    fn package() -> Vec<u8> {
        let mut code = Vec::new();
        for (group, cue, channel, start, rows) in
            [(0u8, 0u8, 4, 10, 6), (0, 1, 7, 12, 9), (2, 0, 4, 16, 3)]
        {
            code.extend([group, cue]);
            code.extend(line(channel, start, rows));
        }
        code.extend([0, 0, 0xFF, 0xFF, 0, 0, 0, 0]);
        code.extend(line(6, 348, 86));
        code.extend([0, 0]);
        let mut file = (code.len() as u32).to_le_bytes().to_vec();
        file.extend(code);
        file
    }

    #[test]
    fn a_cue_table_runs_to_a_negative_channel() {
        let file = package();
        let package = ScenePackage::parse(&file).expect("package");
        let table = cues(&package, PACKAGE_BASE + 4).expect("cues");
        assert_eq!(table.len(), 3);
        assert_eq!((table[1].group, table[1].cue), (0, 1));
        assert_eq!(table[1].line, Line::parse(&line(7, 12, 9)).expect("line"));
        // The line behind the table, as a script's call names it.
        let alone = line_at(&package, PACKAGE_BASE + 4 + 32).expect("line");
        assert_eq!((alone.channel, alone.start, alone.rows), (6, 348, 86));
        // A table without its end leaves the image; so does a line at the image's end.
        assert_eq!(cues(&package, PACKAGE_BASE + 4 + 40), None);
        assert_eq!(line_at(&package, PACKAGE_BASE + 4 + 36), None);
    }

    #[test]
    fn the_waiter_takes_the_last_cue_reached_of_the_group_on_screen() {
        let file = package();
        let package = ScenePackage::parse(&file).expect("package");
        let table = cues(&package, PACKAGE_BASE + 4).expect("cues");
        let at = |group, reached: u8| spoken(&table, group, |cue| cue <= reached).map(|c| c.cue);
        assert_eq!(at(0, 0), Some(0));
        assert_eq!(at(0, 1), Some(1));
        assert_eq!(at(2, 5), Some(0));
        assert_eq!(at(1, 5), None);
        // A text named by its address has the group 0x8000 on screen: no cue fits.
        assert_eq!(at(0x8000, 5), None);
        assert_eq!(spoken(&table, 0, |_| false), None);
    }

    #[test]
    fn a_speaker_takes_only_the_rows_of_its_channel() {
        let read = Line::parse(&line(5, 0, 2)).expect("line");
        let mut speaker = Speaker::new(&read);
        let samples = speaker.row(&sector(5, 0x64, 0x04)).expect("row");
        assert_eq!(samples.len(), ROW_SAMPLES);
        assert!(samples.iter().all(|&sample| sample == 0));
        assert_eq!(speaker.limited(), 0);
        assert_eq!(
            speaker.row(&sector(6, 0x64, 0x04)),
            Err(VoiceError::Channel {
                wanted: 5,
                found: 6
            })
        );
        // An empty sector, a stereo one, one at the full rate, and half a sector.
        assert_eq!(speaker.row(&sector(5, 0, 0)), Err(VoiceError::NotALine));
        assert_eq!(
            speaker.row(&sector(5, 0x64, 0x05)),
            Err(VoiceError::NotALine)
        );
        assert_eq!(
            speaker.row(&sector(5, 0x64, 0x00)),
            Err(VoiceError::NotALine)
        );
        assert_eq!(
            speaker.row(&sector(5, 0x64, 0x04)[..1000]),
            Err(VoiceError::Sector)
        );
    }
}
