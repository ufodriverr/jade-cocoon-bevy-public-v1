
use std::collections::HashMap;
use std::ops::Range;

use bevy::prelude::*;
use jc_disc::{Archive, DiscImage, Executable, LocalConfig};
use jc_formats::chip;
use jc_formats::mixer::{Player, VALUES_A_STEP};
use jc_formats::reverb::Reverb;
use jc_formats::script;
use jc_formats::sound::{self, Bank, Effect, EffectTable, MusicTable, SoundFile};
use jc_formats::window::SETTING_AT_START;

use crate::audio::Loudness;
use crate::movie_sound::{Line, MovieSound};
use crate::movie_view::Failure;

/// The records of "The four records": the music, the second music of a row, what
/// stays in memory for the whole game, and the sequences of a fight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Which {
    Music,
    Second,
    Kept,
    Fight,
}

/// Sequences a record can hold: two sets of 16.
const SEQUENCES: u16 = 32;

/// The number the player knows a sequence of a record by.
fn owner(which: Which, sequence: usize) -> u16 {
    which as u16 * SEQUENCES + sequence as u16
}

/// A record: the music number it holds and its running sequence.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Record {
    number: Option<usize>,
    running: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Case {
    /// A number of the file that stays in memory: played at once.
    Kept,
    /// The music record has the number and the row's sequence runs.
    Runs,
    /// The music record holds the number or one of the same file: its running
    /// sequence is stopped and the row's is started.
    Again,
    /// The second record holds a number of the same file: the row's sequence is
    /// started there.
    Second,
    /// Both records are given back and the row's file is read.
    Read,
}

/// Which case a number is. `file` answers the file of a music number.
fn case(
    number: usize,
    file: impl Fn(usize) -> Option<usize>,
    sequence: Option<usize>,
    (music, second): (Record, Record),
) -> Case {
    // An empty record names a file no row has.
    let same = |held: Option<usize>| held.is_some_and(|held| file(held) == file(number));
    if number < sound::KEPT_BELOW {
        Case::Kept
    } else if music.number == Some(number) && music.running == sequence {
        Case::Runs
    } else if music.number == Some(number) || same(music.number) {
        Case::Again
    } else if same(second.number) {
        Case::Second
    } else {
        Case::Read
    }
}
/// The longest stretch of time one pass of the app may stand for.
const LONGEST_PASS: f64 = 0.1;
/// Seconds of sound that wait on the line at least and at most: below the first the
/// player is stepped ahead of the time, above the second it waits.
const FEWEST: f64 = 0.05;
const MOST: f64 = 0.3;
/// Steps between two lines of the log about a music that plays.
const SAID_EVERY: u64 = 600;
/// The fade of the title: the loudness taken off both volumes, and the steps it takes.
const FADE: (u16, u32) = (127, 120);
/// Sound numbers below this one are programs of the second record's bank, the others
/// of the bank that stays; the key every sound is played with; and the number the
/// player knows the voices of a file's sounds by, plus the file.
const SOUNDS_OF_THE_PLACE: u32 = 63;
const SOUND_KEY: u8 = 60;
const SOUND_OWNERS: u16 = 0x8000;
/// The fade of the second record's sequence.
const SECOND_FADE: (u16, u32) = (127, 64);
/// The timers of the sound's task, and the steps one counts behind that fade: 32 game
/// frames of the field, 25 a second, are 76.8 steps; the app counts whole steps.
const TIMERS: usize = 5;
const TIMER_STEPS: u32 = 77;

/// A sound file as it stays in memory.
struct Kept {
    file: Vec<u8>,
    /// The bank and where its body lies. The file of a fight's sequences has none.
    bank: Option<(Bank, Range<usize>)>,
    sets: [Option<Range<usize>>; 2],
}

impl Kept {
    /// The bank of the file and its body.
    fn bank(&'static self) -> Result<(&'static Bank, &'static [u8]), Failure> {
        let (bank, body) = self.bank.as_ref().ok_or("the sound file has no bank")?;
        Ok((bank, &self.file[body.clone()]))
    }
}

/// Where a section lies in its file.
fn place(file: &[u8], section: &[u8]) -> Range<usize> {
    let start = section.as_ptr() as usize - file.as_ptr() as usize;
    start..start + section.len()
}

/// The music of the game: what plays, and its way to the sound device.
#[derive(Resource)]
pub struct Music {
    /// A run that is a check plays the music as silence.
    muted: bool,
    setting: u32,
    /// The sound files read so far. A file is read once and stays for the run, so that
    /// the player can keep its bank and samples by reference.
    kept: HashMap<usize, &'static Kept>,
    /// The music table, once it was read.
    table: Option<MusicTable>,
    /// The table of the effects of the animation events, once it was read.
    effects: Option<EffectTable>,
    player: Player<'static>,
    reverbed: bool,
    /// The music record and the second record. A fade leaves the music record its
    /// number and takes its running sequence.
    music: Record,
    second: Record,
    /// The running sequence of the record that stays in memory.
    always: Option<usize>,
    /// The music that was noted last (`0x80031634`), which the tear-down of a fight
    /// asks for again. The word is 0 in the executable.
    noted: usize,
    /// A row of the fight's sequences was started and the fight's record not closed.
    fights: bool,
    /// The voices of the sounds that are kept, by the address of the word a script
    /// keeps each in.
    voices_kept: HashMap<u32, usize>,
    /// The marked timers of the sound's task: the steps left and the sequence that
    /// is stopped when they are through.
    timers: Vec<(u32, u16)>,
    /// A sound was asked for since the last step.
    asked: bool,
    /// The sequence and the volume the page "Audio" gave last, for the log.
    followed: Option<(usize, u16)>,
    line: Option<(Line, Entity)>,
    /// The share of a step that is owed.
    owed: f64,
    /// Steps made for this music, the loudest value of a side and the most voices
    /// that sounded at once.
    steps: u64,
    loudest: u16,
    busiest: usize,
    /// The shares of the music and of the sounds in the chip's mix, the volumes of a PC
    /// (`audio.rs`).
    shares: (u16, u16),
}

impl Music {
    pub fn new(muted: bool) -> Self {
        Self {
            muted,
            setting: SETTING_AT_START,
            kept: HashMap::new(),
            table: None,
            effects: None,
            player: Player::new(),
            reverbed: false,
            music: Record::default(),
            second: Record::default(),
            always: None,
            noted: 0,
            fights: false,
            voices_kept: HashMap::new(),
            timers: Vec::new(),
            asked: false,
            followed: None,
            line: None,
            owed: 0.0,
            steps: 0,
            loudest: 0,
            busiest: 0,
            shares: (jc_formats::mixer::WHOLE, jc_formats::mixer::WHOLE),
        }
    }

    /// The shares of the music and of the sounds in the chip's mix (`audio.rs`).
    pub fn shares(&self) -> (u16, u16) {
        self.shares
    }

    /// The volumes of a PC for the music and for the sounds, as shares of
    /// `jc_formats::mixer::WHOLE`: heard from the next step of the mix on.
    pub fn set_shares(&mut self, shares: (u16, u16)) {
        self.shares = shares;
        self.player.set_shares(shares.0, shares.1);
        info!(
            "the volumes of a PC: the music at {} and the sounds at {} of {}",
            shares.0,
            shares.1,
            jc_formats::mixer::WHOLE
        );
    }

    /// The setting word the volumes are taken from.
    pub fn setting(&self) -> u32 {
        self.setting
    }

    pub fn set_setting(&mut self, word: u32) {
        self.setting = word;
        if self.player.mono() != sound::one_loudspeaker(word) {
            self.tell_output(word);
        }
    }

    pub fn audio_page(&mut self, word: u32) {
        let (Some(_), Some(running)) = (self.music.number, self.music.running) else {
            return;
        };
        let volume = sound::music_volume(word);
        (self.player).set_sides(owner(Which::Music, running), [volume; 2]);
        if self.followed != Some((running, volume)) {
            info!("the options: the music's sequence {running} has the volume {volume} of 127");
            self.followed = Some((running, volume));
        }
    }

    /// Tells the sound library one or two loudspeakers by bit 6 of the setting word,
    /// as `0x800309E4` does behind the row "Output Format".
    pub fn tell_output(&mut self, word: u32) {
        let mono = sound::one_loudspeaker(word);
        self.player.set_mono(mono);
        info!(
            "the sound library is told {} from here on",
            if mono {
                "one loudspeaker: both sides of a voice are its louder one"
            } else {
                "two loudspeakers"
            }
        );
    }

    /// Reads a sound file, once.
    fn keep(&mut self, number: usize) -> Result<&'static Kept, Failure> {
        if let Some(&kept) = self.kept.get(&number) {
            return Ok(kept);
        }
        let config = LocalConfig::load()?;
        let mut disc = DiscImage::open(&config.disc_image)?;
        let archive = Archive::open(&mut disc)?;
        let file = archive.read(&mut disc, number)?;
        if !self.reverbed {
            let exe = Executable::read(&mut disc)?;
            (self.player).set_reverb(Reverb::of_the_game(&exe.image, exe.base)?);
            self.reverbed = true;
            info!("the reverb of the game's kind is at its depth from the first read on");
        }
        let parsed = SoundFile::parse(&file)?;
        let bank = match (parsed.bank_head, parsed.bank_body) {
            (Some(head), Some(body)) => Some((Bank::parse(head)?, place(&file, body))),
            (None, None) => None,
            _ => return Err("the sound file's bank lacks its head or its body".into()),
        };
        let sets = parsed.sets.map(|set| set.map(|set| place(&file, set)));
        let kept = Kept { file, bank, sets };
        let kept: &'static Kept = Box::leak(Box::new(kept));
        self.kept.insert(number, kept);
        Ok(kept)
    }

    /// The row of a music number.
    fn row(&mut self, number: usize) -> Result<sound::Music, Failure> {
        if self.table.is_none() {
            let config = LocalConfig::load()?;
            let mut disc = DiscImage::open(&config.disc_image)?;
            let exe = Executable::read(&mut disc)?;
            self.table = Some(MusicTable::from_executable(&exe.image, exe.base)?);
        }
        let row = self.table.as_ref().and_then(|table| table.get(number));
        Ok(*row.ok_or("no such music number")?)
    }

    /// The record of the music or of the second music.
    fn record(&mut self, which: Which) -> &mut Record {
        match which {
            Which::Second => &mut self.second,
            _ => &mut self.music,
        }
    }

    /// Starts the sequence of a row in a record, as `0x800312BC` and `0x800310B8` do:
    /// at the music's volume of a new game in the music record and for number 0, at
    /// the volume of the sounds otherwise. A file without a set of sequences has
    /// nothing to start.
    fn play(&mut self, which: Which, number: usize) -> Result<(), Failure> {
        let row = self.row(number)?;
        let Some(asked) = row.sequence else {
            return Ok(());
        };
        let kept = self.keep(row.file)?;
        let Some(set) = kept.sets[0].clone() else {
            return Ok(());
        };
        let found = sound::sequences(&kept.file[set])?;
        let sequence = found.get(asked);
        let sequence = sequence.ok_or("the music's row names no sequence of its file")?;
        let music = matches!((which, number), (Which::Music, _) | (Which::Kept, 0));
        let volume = match music {
            true => sound::music_volume(self.setting),
            false => sound::sound_volume(self.setting),
        };
        let (bank, body) = kept.bank()?;
        let owner = owner(which, asked);
        // What starts at the music's volume is the music for the volumes of a PC.
        self.player.mark_music(owner, music);
        if !(self.player).start(owner, sequence, bank, body, [volume; 2], 1)? {
            return Err("the music's sequence is of the slow kind".into());
        }
        info!(
            "music {number}: sequence {} of file {} starts{} at volume {volume} of 127{}",
            sequence.number,
            row.file,
            match which {
                Which::Second => " in the second record",
                Which::Kept => " in the record that stays",
                _ => "",
            },
            if self.muted {
                ", muted for this run"
            } else {
                ""
            }
        );
        match which {
            Which::Kept => self.always = Some(asked),
            which => self.record(which).running = Some(asked),
        }
        if which == Which::Music {
            (self.steps, self.loudest, self.busiest) = (0, 0, 0);
        }
        Ok(())
    }

    /// Fills the music record with a music number and starts the row's sequence at the
    /// music's volume of a new game, as the title's entry does for its music.
    pub fn start(&mut self, number: usize) -> Result<(), Failure> {
        self.music = Record {
            number: Some(number),
            running: None,
        };
        self.play(Which::Music, number)?;
        match self.music.running {
            Some(_) => Ok(()),
            None => Err("the music's row has no sequence to start".into()),
        }
    }

    /// Fills a record from the file of a music number, and what the read's callback
    /// `0x80030810` does behind it: the row's sequence is started when the file has a
    /// set of sequences and the row names one.
    fn read(&mut self, which: Which, number: usize) -> Result<(), Failure> {
        *self.record(which) = Record {
            number: Some(number),
            running: None,
        };
        let row = self.row(number)?;
        self.keep(row.file)?;
        if row.sequence.is_none() {
            info!(
                "music {number}: file {} is read{}, its row has no sequence to start",
                row.file,
                match which {
                    Which::Second => " into the second record",
                    _ => "",
                }
            );
        }
        self.play(which, number)
    }

    pub fn ask(&mut self, number: usize) -> Result<(), Failure> {
        let row = self.row(number)?;
        let table = self.table.as_ref();
        let file = |number| Some(table?.get(number)?.file);
        match case(number, file, row.sequence, (self.music, self.second)) {
            Case::Kept => {
                self.always = None;
                self.play(Which::Kept, number)
            }
            Case::Runs => Ok(()),
            Case::Again => {
                // The running sequence is stopped and its volume made 0.
                if let Some(running) = self.music.running {
                    self.player.stop(owner(Which::Music, running));
                    self.player.set_sides(owner(Which::Music, running), [0; 2]);
                }
                self.music.running = row.sequence;
                self.play(Which::Music, number)
            }
            Case::Second => {
                self.second.running = row.sequence;
                self.play(Which::Second, number)
            }
            Case::Read => {
                if let Some(running) = self.music.running.take() {
                    self.player.stop(owner(Which::Music, running));
                }
                self.give_back();
                self.empty(Which::Second);
                self.read(Which::Music, number)?;
                match row.second {
                    Some(second) => self.read(Which::Second, second),
                    None => Ok(()),
                }
            }
        }
    }

    pub fn note(&mut self, number: usize) {
        self.noted = number;
    }

    /// The music a room change asks for and notes: the music of the room's record as
    /// story variable 12 turns it, none for a negative one. A music that cannot be
    /// read leaves the room without it.
    pub fn room_asks(&mut self, music: i16, variable: i32) {
        let Some(number) = sound::room_music(music, variable) else {
            return;
        };
        if let Err(err) = self.ask(number) {
            error!("music {number} of a room: {err}");
        }
        self.note(number);
    }

    pub fn entry_asks(&mut self, number: usize) {
        if let Err(err) = self.ask(number) {
            error!("music {number} of a room's entry routine: {err}");
        }
        self.note(number);
    }

    pub fn notice_asks(&mut self, number: usize) {
        if let Err(err) = self.ask(number) {
            error!("music {number} of a notice: {err}");
        }
    }

    /// Starts a row of the fight's sequences, as `0x800311BC` does: the row's
    /// sequence of the row's set of the fight's file, on the bank of the file that
    /// stays in memory, at the volume of the music for the first two rows and at the
    /// volume of the sounds for the others. The file is read when a row is first asked
    /// for; the game reads it in the fight's intro (`0x80030444`).
    fn start_row(&mut self, row: usize) -> Result<(), Failure> {
        let (set, asked) = sound::fight_row(row).ok_or("no such row of a fight's sequences")?;
        let file = self.row(sound::FIGHT_MUSIC)?.file;
        let kept = self.keep(file)?;
        let always = self.row(0)?.file;
        let (bank, body) = self.keep(always)?.bank()?;
        let range = kept.sets[set].clone();
        let range = range.ok_or("the fight's file lacks a set of sequences")?;
        let found = sound::sequences(&kept.file[range])?;
        let sequence = found.get(asked);
        let sequence = sequence.ok_or("the row names no sequence of the fight's file")?;
        let music = sound::fight_row_is_music(row);
        let volume = match music {
            true => sound::music_volume(self.setting),
            false => sound::sound_volume(self.setting),
        };
        let owner = owner(Which::Fight, row);
        self.player.mark_music(owner, music);
        if !(self.player).start(owner, sequence, bank, body, [volume; 2], 1)? {
            return Err("the fight's sequence is of the slow kind".into());
        }
        info!(
            "the fight's row {row}: sequence {asked} of set {set} of file {file} starts on the \
             bank of file {always} at volume {volume} of 127"
        );
        self.fights = true;
        Ok(())
    }

    pub fn fight_asks(&mut self, asked: sound::FightAsks) {
        use sound::FightAsks as Asks;
        let done = match asked {
            Asks::Silence => {
                self.silence(Which::Music);
                Ok(())
            }
            Asks::SilenceKept => {
                self.silence(Which::Kept);
                Ok(())
            }
            Asks::Music(number) => self.ask(number),
            Asks::GiveBack => {
                self.give_back();
                self.empty(Which::Second);
                Ok(())
            }
            Asks::Start(row) => self.start_row(row),
            Asks::Stop(row) => {
                let row = owner(Which::Fight, row);
                self.player.stop(row);
                self.player.set_sides(row, [0; 2]);
                Ok(())
            }
            // The fade takes the handle of the first set whatever the row's set is.
            Asks::Fade(row) => {
                let row = owner(Which::Fight, row % 16);
                self.player.fade(row, FADE.0, FADE.1);
                Ok(())
            }
            Asks::FadeMusic => {
                self.fade();
                Ok(())
            }
            Asks::Close => {
                self.silence(Which::Fight);
                self.fights = false;
                Ok(())
            }
            Asks::AllOff => {
                self.player.all_off();
                Ok(())
            }
            Asks::Noted => self.ask(self.noted),
        };
        match done {
            Ok(()) => info!("the sound: a fight asked for {asked:?}"),
            Err(err) => error!("the sound a fight asked for, {asked:?}: {err}"),
        }
    }

    /// Stops every sequence of a record and makes it silent, as `0x80030DC4` does: the
    /// record has no running sequence from then on and keeps what it holds.
    fn silence(&mut self, which: Which) {
        for sequence in 0..usize::from(SEQUENCES) {
            self.player.stop(owner(which, sequence));
            self.player.set_sides(owner(which, sequence), [0; 2]);
        }
        match which {
            Which::Kept => self.always = None,
            Which::Fight => {}
            which => self.record(which).running = None,
        }
    }

    pub fn sound(&mut self, number: u32) {
        let played = self.play_sound(number);
        match played {
            Ok(Some((file, tones))) => info!(
                "sound {number}: program {number} of the bank of file {file}, {tones} tones for key {SOUND_KEY}"
            ),
            Ok(None) => info!("sound {number}: the second record holds no bank"),
            Err(err) => error!("sound {number}: {err}"),
        }
    }

    /// The bank a sound of that number is played on and its file: the second record's
    /// for a number below 63, the one that stays otherwise. `None` when the second
    /// record holds nothing.
    #[allow(clippy::type_complexity)]
    fn bank_of_a_sound(
        &mut self,
        number: u32,
    ) -> Result<Option<(usize, (&'static Bank, &'static [u8]))>, Failure> {
        let held = match number < SOUNDS_OF_THE_PLACE {
            true => self.second.number,
            false => Some(0),
        };
        let Some(held) = held else {
            return Ok(None);
        };
        let file = self.row(held)?.file;
        Ok(Some((file, self.keep(file)?.bank()?)))
    }

    pub fn sound_kept(&mut self, at: u32, number: u32) {
        let played = self.bank_of_a_sound(number).and_then(|found| {
            let Some((file, bank)) = found else {
                return Ok(None);
            };
            let volume = sound::sound_volume(self.setting);
            let known = SOUND_OWNERS + u16::try_from(file)?;
            let program = u8::try_from(number)?;
            // A program without tones is played with the tone of the next program in
            // use, at the volume of its own record.
            let read = match bank.0.program(program) {
                Some(_) => String::new(),
                None => match bank.0.one_tone(program, 0) {
                    Some((volume, _, tone)) => format!(
                        " (a program without tones: sample {} of the next program in use, \
                         at the program's volume {volume})",
                        tone.sample
                    ),
                    None => String::new(),
                },
            };
            let voice = (self.player).sound_kept(known, bank, program, SOUND_KEY, [volume; 2]);
            Ok::<_, Failure>(Some((file, voice, read)))
        });
        self.voices_kept.remove(&at);
        match played {
            Ok(Some((file, Some(voice), read))) => {
                self.voices_kept.insert(at, voice);
                self.asked = true;
                info!(
                    "sound {number}: tone 0 of program {number} of the bank of file {file} on \
                     voice {voice}, kept in the word at {at:#010x}{read}"
                );
            }
            Ok(Some((file, None, _))) => {
                info!("sound {number}: no voice for program {number} of the bank of file {file}")
            }
            Ok(None) => info!("sound {number}: the second record holds no bank"),
            Err(err) => error!("sound {number}: {err}"),
        }
    }

    /// Ends the sound whose voice the word at an address names, as `0x800315D0` does:
    /// the voice is let go, whatever it plays by now. Nothing when the word names none.
    pub fn sound_ends(&mut self, at: u32) {
        if let Some(voice) = self.voices_kept.remove(&at) {
            self.player.let_go(voice);
            info!("the sound kept in the word at {at:#010x}: voice {voice} is let go");
        }
    }

    pub fn effect(&mut self, number: u16, level: u8) {
        let done = self.effect_of(number, level).and_then(|effect| {
            match effect {
                Effect::Sound(sound) => {
                    let played = self.play_sound(sound)?;
                    debug!("effect {number}: sound {sound}, {played:?}");
                    return Ok(effect);
                }
                Effect::Music(music) => self.ask(music)?,
                Effect::Row(row) => self.start_row(row)?,
                Effect::FadeKept => {
                    self.fade_kept();
                }
                Effect::Nothing => return Ok(effect),
            }
            info!("the sound: effect {number} of an animation event asked for {effect:?}");
            Ok(effect)
        });
        if let Err(err) = done {
            error!("effect {number} of an animation event: {err}");
        }
    }

    pub fn fade_kept(&mut self) -> bool {
        let running = self.always.take();
        if let Some(running) = running {
            (self.player).fade(owner(Which::Kept, running), FADE.0, FADE.1);
        }
        running.is_some()
    }

    fn effect_of(&mut self, number: u16, level: u8) -> Result<Effect, Failure> {
        if self.effects.is_none() {
            let config = LocalConfig::load()?;
            let mut disc = DiscImage::open(&config.disc_image)?;
            let exe = Executable::read(&mut disc)?;
            self.effects = Some(EffectTable::from_executable(&exe.image, exe.base)?);
        }
        let table = self.effects.as_ref().ok_or("no effect table")?;
        Ok(table.get(number, level))
    }

    /// The file whose bank the sound was played on, if one is there, and how many
    /// tones of the program answer the key.
    fn play_sound(&mut self, number: u32) -> Result<Option<(usize, usize)>, Failure> {
        let Some((file, bank)) = self.bank_of_a_sound(number)? else {
            return Ok(None);
        };
        let volume = sound::sound_volume(self.setting);
        let known = SOUND_OWNERS + u16::try_from(file)?;
        let program = u8::try_from(number)?;
        (self.player).sound(known, bank, program, SOUND_KEY, [volume; 2]);
        self.asked = true;
        let tones = bank.0.program(program).map(|program| &program.tones);
        let answer =
            |tones: &Vec<sound::Tone>| tones.iter().filter(|t| t.answers(SOUND_KEY)).count();
        Ok(Some((file, tones.map_or(0, answer))))
    }

    pub fn script_asks(&mut self, asked: script::Music) {
        let done = match asked {
            script::Music::End => {
                self.fade();
                Ok(())
            }
            script::Music::Stop => {
                self.stop();
                Ok(())
            }
            script::Music::Other { code: 0xFF02, .. } => {
                self.fade_second();
                Ok(())
            }
            script::Music::Other { c, .. } => self.end(c as usize),
            // `0xFF04` notes the music it asks for; a plain number does not.
            script::Music::Start(number) => {
                self.note(number as usize);
                self.ask(number as usize)
            }
            script::Music::Ask(number) => self.ask(number as usize),
        };
        match done {
            Ok(()) => info!("the music: a script asked for {asked:?}"),
            Err(err) => error!("the music a script asked for, {asked:?}: {err}"),
        }
    }

    /// Stops the music record's running sequence, which the record then has no more:
    /// the voices fade by their envelopes.
    fn stop(&mut self) {
        if let Some(running) = self.music.running.take() {
            self.player.stop(owner(Which::Music, running));
        }
    }

    /// `0x80031694`: the second record's running sequence is faded, 127 off both
    /// volumes over 64 steps, and a free timer of the sound's task stops it 32 game
    /// frames later. The record keeps its running sequence. Nothing without one, and
    /// nothing while all five timers count.
    fn fade_second(&mut self) {
        let Some(running) = self.second.running else {
            return;
        };
        if self.timers.len() < TIMERS {
            let owner = owner(Which::Second, running);
            self.player.fade(owner, SECOND_FADE.0, SECOND_FADE.1);
            self.timers.push((TIMER_STEPS, owner));
        }
    }

    /// `0x80030758`: the sequence of a row is stopped and its volume made 0, in the
    /// record that stays for a number below 10, else in the music record and in the
    /// second record when they hold a number of the row's file. No record loses its
    /// running sequence by it.
    fn end(&mut self, number: usize) -> Result<(), Failure> {
        let row = self.row(number)?;
        let Some(sequence) = row.sequence else {
            return Ok(());
        };
        let table = self.table.as_ref();
        let same = |held: Option<usize>| {
            held.and_then(|held| table?.get(held))
                .is_some_and(|held| held.file == row.file)
        };
        let records = [
            (Which::Kept, number < sound::KEPT_BELOW),
            (
                Which::Music,
                number >= sound::KEPT_BELOW && same(self.music.number),
            ),
            (
                Which::Second,
                number >= sound::KEPT_BELOW && same(self.second.number),
            ),
        ];
        for (which, _) in records.into_iter().filter(|(_, named)| *named) {
            self.player.stop(owner(which, sequence));
            self.player.set_sides(owner(which, sequence), [0; 2]);
        }
        Ok(())
    }

    /// Fades the music record's sequence as the title and the chapter screen do: 127
    /// off both volumes over 120 steps. The record has no running sequence from then
    /// on; the sequence plays on, silent, until the music is given back.
    pub fn fade(&mut self) {
        let Some(running) = self.music.running.take() else {
            return;
        };
        self.player
            .fade(owner(Which::Music, running), FADE.0, FADE.1);
        info!(
            "music {}: fades after {} steps, {}",
            self.music.number.unwrap_or(0),
            self.steps,
            self.heard()
        );
    }

    /// Gives a record back when it holds a music, as `0x80030D5C` does: every
    /// sequence of its sets is stopped and the voices they hold fade by their
    /// envelopes. Answers the number it held.
    fn empty(&mut self, which: Which) -> Option<usize> {
        let number = self.record(which).number?;
        self.silence(which);
        *self.record(which) = Record::default();
        Some(number)
    }

    /// Gives the music record back: its sequence is stopped, whether it still runs or
    /// was faded.
    pub fn give_back(&mut self) {
        let Some(number) = self.empty(Which::Music) else {
            return;
        };
        info!(
            "music {number}: given back after {} steps, {}",
            self.steps,
            self.heard()
        );
    }

    pub fn field_ends(&mut self) {
        if let Some(running) = self.music.running.take() {
            self.player.stop(owner(Which::Music, running));
        }
        let music = self.empty(Which::Music);
        self.silence(Which::Kept);
        let second = self.empty(Which::Second);
        self.player.all_off();
        info!(
            "the sound: the field ends, the music {music:?} and the second music {second:?} are given back after {} steps and every voice is let go",
            self.steps
        );
    }

    /// One step of the marked timers: a timer that is through stops its sequence.
    fn count_timers(&mut self) {
        for (left, _) in &mut self.timers {
            *left -= 1;
        }
        let through = self.timers.iter().filter(|(left, _)| *left == 0);
        for &(_, owner) in through.collect::<Vec<_>>() {
            self.player.stop(owner);
        }
        self.timers.retain(|(left, _)| *left > 0);
    }

    /// Whether a record holds a music or a sequence of the file that stays plays.
    fn held(&self) -> bool {
        let always = self.always.map(|sequence| owner(Which::Kept, sequence));
        self.asked
            || self.fights
            || self.music.number.is_some()
            || self.second.number.is_some()
            || always.is_some_and(|owner| self.player.playing(owner))
    }
    /// What the log says about the sound made so far.
    fn heard(&self) -> String {
        let taken = self.line.as_ref().map_or(0, |(line, _)| line.taken());
        format!(
            "the loudest value {} of 32767, at most {} voices at once, the sound device took {:.2} s",
            self.loudest,
            self.busiest,
            taken as f64 / f64::from(chip::RATE)
        )
    }
}

/// How many steps of the library one pass of the app makes: those the time owes, none
/// while more than enough sound waits on the line, and more while too little does.
fn steps_of_a_pass(owed: f64, waiting: usize) -> u32 {
    let (fewest, most) = (
        (f64::from(chip::RATE) * FEWEST) as usize,
        (f64::from(chip::RATE) * MOST) as usize,
    );
    if waiting > most {
        return 0;
    }
    let mut steps = owed as u32;
    while waiting + steps as usize * VALUES_A_STEP < fewest {
        steps += 1;
    }
    steps
}

/// Steps the music for the time gone by and puts its sound on the line. The time is the
/// real clock's, not the game's: the music and the sounds keep their speed and their
/// pitch while the game's clock runs fast (`pace.rs`), and a sound asked for in a game
/// frame of a fast clock starts at once all the same. While the line is held (the window
/// in the background, `audio.rs`) nothing is stepped, and the music goes on where it was.
pub fn play_music(
    mut music: ResMut<Music>,
    time: Res<Time<Real>>,
    loudness: Res<Loudness>,
    mut sounds: ResMut<Assets<MovieSound>>,
    mut commands: Commands,
) {
    if loudness.music.held() {
        return;
    }
    let music = &mut *music;
    if !music.held() && music.player.sounding() == 0 {
        // Nothing plays and nothing fades: the line is given back.
        if let Some((line, player)) = music.line.take() {
            line.close();
            commands.entity(player).despawn();
        }
        music.owed = 0.0;
        return;
    }
    let (line, _) = music.line.get_or_insert_with(|| {
        let line = Line::with_gain(chip::RATE, &loudness.music);
        let player = AudioPlayer(sounds.add(MovieSound(line.clone())));
        (line, commands.spawn(player).id())
    });
    music.owed += time.delta_secs_f64().min(LONGEST_PASS) * f64::from(sound::STEPS_A_SECOND);
    let steps = steps_of_a_pass(music.owed, line.waiting());
    music.owed = (music.owed - f64::from(steps)).max(0.0);
    let mut out = Vec::with_capacity(steps as usize * VALUES_A_STEP);
    for _ in 0..steps {
        music.player.step(&mut out);
        music.asked = false;
        music.count_timers();
        music.busiest = music.busiest.max(music.player.sounding());
        music.steps += 1;
        if music.steps.is_multiple_of(SAID_EVERY)
            && let Some(number) = music.music.number
        {
            info!("music {number}: {} steps, {}", music.steps, music.heard());
        }
    }
    let loudest = out.as_flattened().iter().map(|side| side.unsigned_abs());
    music.loudest = music.loudest.max(loudest.max().unwrap_or(0));
    let share = if music.muted { 0.0 } else { 1.0 };
    let Some((line, _)) = &music.line else {
        return;
    };
    line.push(out.as_flattened(), true, (share, share));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pass_makes_the_steps_the_time_owes_and_keeps_the_line_neither_dry_nor_full() {
        // A line with a tenth of a second on it: what the time owes, cut off.
        assert_eq!(steps_of_a_pass(0.4, 4410), 0);
        assert_eq!(steps_of_a_pass(2.9, 4410), 2);
        // Too little waits: steps ahead of the time until a 20th of a second does.
        assert_eq!(steps_of_a_pass(0.0, 0), 3);
        assert_eq!(steps_of_a_pass(1.0, 1000), 2);
        assert_eq!(steps_of_a_pass(5.0, 0), 5);
        // Too much waits: none, whatever the time owes.
        assert_eq!(steps_of_a_pass(3.0, 13_231), 0);
        assert_eq!(steps_of_a_pass(3.0, 13_230), 3);
    }

    #[test]
    fn the_asker_takes_a_number_by_what_the_records_hold() {
        // Numbers 20 and 21 are of file 800, 30 of file 801, 5 of the file that stays.
        let file = |number: usize| match number {
            0..=9 => Some(768),
            20 | 21 => Some(800),
            30 => Some(801),
            _ => None,
        };
        let held = |number, running| Record {
            number: Some(number),
            running,
        };
        let none = Record::default();
        let asked = |number, sequence, records| case(number, file, sequence, records);
        // A number below 10 is played at once, whatever the records hold.
        assert_eq!(asked(5, Some(5), (held(20, Some(0)), none)), Case::Kept);
        // The number the music record has: nothing when the row's sequence runs,
        // and a row without one is the same as a record without one.
        assert_eq!(asked(20, Some(0), (held(20, Some(0)), none)), Case::Runs);
        assert_eq!(asked(20, None, (held(20, None), none)), Case::Runs);
        // After a fade the record has no running sequence: started again.
        assert_eq!(asked(20, Some(0), (held(20, None), none)), Case::Again);
        // Another number of the same file.
        assert_eq!(asked(21, Some(1), (held(20, Some(0)), none)), Case::Again);
        // The second record holds a number of the file, the music record does not.
        let both = (held(30, Some(0)), held(20, None));
        assert_eq!(asked(21, Some(1), both), Case::Second);
        // Neither does, or both are empty: the file is read.
        assert_eq!(asked(21, Some(1), (held(30, Some(0)), none)), Case::Read);
        assert_eq!(asked(21, Some(1), (none, none)), Case::Read);
    }

    #[test]
    fn a_timer_is_through_after_its_steps_and_a_sixth_is_not_marked() {
        let mut music = Music::new(true);
        // Without a running sequence in the second record nothing is marked.
        music.fade_second();
        assert!(music.timers.is_empty());
        music.second.running = Some(3);
        for _ in 0..TIMERS + 1 {
            music.fade_second();
        }
        assert_eq!(music.timers.len(), TIMERS);
        assert_eq!(music.timers[0], (TIMER_STEPS, owner(Which::Second, 3)));
        for _ in 1..TIMER_STEPS {
            music.count_timers();
        }
        assert_eq!(music.timers.len(), TIMERS);
        music.count_timers();
        assert!(music.timers.is_empty());
        // The record keeps its running sequence.
        assert_eq!(music.second.running, Some(3));
    }

    #[test]
    fn a_stop_takes_the_running_sequence_of_the_music_record() {
        let mut music = Music::new(true);
        music.music = Record {
            number: Some(20),
            running: Some(1),
        };
        music.stop();
        assert_eq!(music.music.number, Some(20));
        assert_eq!(music.music.running, None);
    }

    #[test]
    fn every_sequence_of_a_record_has_a_number_of_its_own() {
        assert_eq!(owner(Which::Music, 0), 0);
        assert_eq!(owner(Which::Music, 31), 31);
        assert_eq!(owner(Which::Second, 0), 32);
        assert_eq!(owner(Which::Kept, 6), 70);
        assert_eq!(owner(Which::Fight, 29), 125);
    }

    #[test]
    fn a_music_is_noted_and_a_room_without_one_notes_nothing() {
        let mut music = Music::new(true);
        assert_eq!(music.noted, 0);
        music.room_asks(-1, 0);
        assert_eq!(music.noted, 0);
        music.note(25);
        assert_eq!(music.noted, 25);
    }

    #[test]
    fn a_silenced_record_keeps_its_music_and_has_no_running_sequence() {
        let mut music = Music::new(true);
        music.music = Record {
            number: Some(20),
            running: Some(1),
        };
        music.always = Some(0);
        music.fight_asks(sound::FightAsks::Silence);
        assert_eq!(music.music.number, Some(20));
        assert_eq!(music.music.running, None);
        assert_eq!(music.always, Some(0));
        music.fight_asks(sound::FightAsks::SilenceKept);
        assert_eq!(music.always, None);
        // The fight's record is closed.
        music.fights = true;
        music.fight_asks(sound::FightAsks::Close);
        assert!(!music.fights);
    }

    #[test]
    fn a_section_is_found_again_in_its_file() {
        let file = [1u8, 2, 3, 4, 5];
        assert_eq!(place(&file, &file[1..3]), 1..3);
        assert_eq!(place(&file, &file[5..]), 5..5);
    }
}
