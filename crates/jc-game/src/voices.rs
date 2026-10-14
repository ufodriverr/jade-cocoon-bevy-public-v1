
use std::collections::VecDeque;
use std::fs::File;

use bevy::prelude::*;
use jc_disc::{DiscImage, LocalConfig};
use jc_formats::movie::LOUDEST;
use jc_formats::voice::{self, Speaker, Task};

use crate::movie_sound::{Line, MovieSound};
use crate::movie_view::Failure;

/// Passes of the task a second.
const PASSES_A_SECOND: f64 = 25.0;
/// The longest stretch of time one pass of the app may stand for.
const LONGEST_PASS: f64 = 0.1;

/// The drive while it plays: the rows it has read and the sound not yet put on the line.
struct Drive {
    line: voice::Line,
    speaker: Speaker,
    /// Rows read so far, counted from the line's first.
    rows: u32,
    /// Samples read and not yet played.
    waiting: VecDeque<i16>,
    /// Samples played so far, and the share of one that is owed.
    played: u64,
    owed: f64,
    /// The loudest sample played.
    loudest: u16,
    /// The end was asked while it played (for the log, which names it once).
    ending: bool,
}

/// The voices of the game: the task, the disc, and the way to the sound device.
#[derive(Resource)]
pub struct Voices {
    /// A run that is a check plays the voices as silence.
    muted: bool,
    task: Task,
    /// The disc image and the first sector of the file, once a line was asked for.
    disc: Option<(DiscImage<File>, u32, u32)>,
    drive: Option<Drive>,
    /// The CD volume, of [`LOUDEST`]: 0 until the task sets it.
    volume: u8,
    /// The share of a pass of the task that is owed.
    owed: f64,
    line: Option<(Line, Entity)>,
}

impl Voices {
    pub fn new(muted: bool) -> Self {
        Self {
            muted,
            task: Task::new(),
            disc: None,
            drive: None,
            volume: 0,
            owed: 0.0,
            line: None,
        }
    }

    /// The setter `0x80040E00`: a line is asked for.
    pub fn ask(&mut self, line: voice::Line) {
        info!(
            "voice: channel {} from row {}, {} rows ({:.2} s), is asked for",
            line.channel,
            line.start,
            line.rows,
            line.seconds()
        );
        self.task.ask(line);
    }

    /// The end `0x80040DC0`.
    pub fn end(&mut self) -> bool {
        let taken = self.task.end();
        if taken && let Some(drive) = self.drive.as_mut().filter(|drive| !drive.ending) {
            drive.ending = true;
            info!(
                "voice: the line of channel {} from row {} is asked to end, {:.2} s of its {:.2} s \
                 played",
                drive.line.channel,
                drive.line.start,
                drive.played as f64 / f64::from(voice::RATE),
                drive.line.seconds()
            );
        }
        taken
    }

    /// `0x80040E20`: no line plays.
    pub fn is_silent(&self) -> bool {
        self.task.is_silent()
    }

    /// `0x80040E40`: no line is asked for that has not started.
    pub fn none_asked(&self) -> bool {
        self.task.none_asked()
    }

    /// The disc image, the file's first sector and its sectors.
    fn disc(&mut self) -> Result<&mut (DiscImage<File>, u32, u32), Failure> {
        if self.disc.is_none() {
            let config = LocalConfig::load()?;
            let mut disc = DiscImage::open(&config.disc_image)?;
            let files = disc.list_files()?;
            let file = (files.iter().find(|file| file.path == voice::PATH))
                .ok_or("the disc has no file of the voices")?;
            self.disc = Some((disc, file.lba, file.size / 2048));
        }
        Ok(self.disc.as_mut().expect("just read"))
    }

    /// Reads the next row the drive passes: of the line, or behind it while the
    /// line fades. Silence behind the channel's end.
    fn read_row(&mut self) -> Result<(), Failure> {
        let Some(drive) = &self.drive else {
            return Ok(());
        };
        let row = u32::from(drive.line.start) + drive.rows;
        let sector = voice::CHANNELS * row + u32::from(drive.line.channel);
        let (disc, first, sectors) = self.disc()?;
        let raw = (sector < *sectors)
            .then(|| disc.read_raw_sector(*first + sector))
            .transpose()?;
        let drive = self.drive.as_mut().expect("asked above");
        let samples = raw.and_then(|raw| drive.speaker.row(&raw).ok());
        match samples {
            Some(samples) => drive.waiting.extend(samples),
            None => (drive.waiting).extend(std::iter::repeat_n(0, voice::ROW_SAMPLES)),
        }
        drive.rows += 1;
        Ok(())
    }

    /// One pass of the task.
    fn pass(&mut self) {
        let behind =
            (self.drive.as_ref()).is_some_and(|drive| drive.played >= drive.line.samples() as u64);
        let pass = self.task.pass(behind);
        if let Some(line) = pass.starts {
            info!(
                "voice: the drive plays channel {} from row {}",
                line.channel, line.start
            );
            self.drive = Some(Drive {
                line,
                speaker: Speaker::new(&line),
                rows: 0,
                waiting: VecDeque::new(),
                played: 0,
                owed: 0.0,
                loudest: 0,
                ending: false,
            });
        }
        if let Some(volume) = pass.volume {
            self.volume = volume;
        }
        if pass.paused
            && let Some(drive) = self.drive.take()
        {
            let taken = self.line.as_ref().map_or(0, |(line, _)| line.taken());
            info!(
                "voice: the line of channel {} from row {} is over: {:.2} s of its {:.2} s played, \
                 the loudest value {} of 32767, the sound device took {:.2} s{}",
                drive.line.channel,
                drive.line.start,
                drive.played as f64 / f64::from(voice::RATE),
                drive.line.seconds(),
                drive.loudest,
                taken as f64 / f64::from(voice::RATE),
                if self.muted {
                    ", muted for this run"
                } else {
                    ""
                }
            );
        }
    }

    /// The sound of `seconds` of the drive's playing, at the volume of now.
    fn sound(&mut self, seconds: f64) -> Vec<i16> {
        let Some(drive) = &mut self.drive else {
            return Vec::new();
        };
        drive.owed += seconds * f64::from(voice::RATE);
        let wanted = drive.owed as usize;
        drive.owed -= wanted as f64;
        while self
            .drive
            .as_ref()
            .is_some_and(|drive| drive.waiting.len() < wanted)
        {
            if let Err(err) = self.read_row() {
                warn!("voice: a row cannot be read: {err}");
                self.drive = None;
                return Vec::new();
            }
        }
        let drive = self.drive.as_mut().expect("still there");
        let out: Vec<i16> = drive.waiting.drain(..wanted).collect();
        drive.played += out.len() as u64;
        // What is heard: nothing while the volume is 0.
        if self.volume > 0 {
            let loudest = out.iter().map(|sample| sample.unsigned_abs()).max();
            drive.loudest = drive.loudest.max(loudest.unwrap_or(0));
        }
        out
    }
}

/// Steps the task of the voices for the time gone by and puts the drive's sound on the
/// line.
pub fn play_voices(
    mut voices: ResMut<Voices>,
    time: Res<Time>,
    mut sounds: ResMut<Assets<MovieSound>>,
    mut commands: Commands,
) {
    let voices = &mut *voices;
    if voices.is_silent()
        && voices.none_asked()
        && let Some((line, player)) = voices.line.take()
    {
        // Nothing plays: the line is given back. The task is stepped on: its idle
        // pass is what forgets an end that was asked while nothing played.
        line.close();
        commands.entity(player).despawn();
    }
    let seconds = time.delta_secs_f64().min(LONGEST_PASS);
    voices.owed += seconds * PASSES_A_SECOND;
    while voices.owed >= 1.0 {
        voices.owed -= 1.0;
        voices.pass();
        // The sound of this pass of the task, at its volume.
        let out = voices.sound(1.0 / PASSES_A_SECOND);
        if out.is_empty() {
            continue;
        }
        let (line, _) = voices.line.get_or_insert_with(|| {
            let line = Line::new(voice::RATE);
            let player = AudioPlayer(sounds.add(MovieSound(line.clone())));
            (line, commands.spawn(player).id())
        });
        let share = match voices.muted {
            true => 0.0,
            false => f32::from(voices.volume) / f32::from(LOUDEST),
        };
        line.push(&out, false, (share, share));
    }
}
