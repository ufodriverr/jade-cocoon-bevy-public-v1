
use crate::sound::{self, Asked, Bank, Event, Levels, Sequence, SoundError, Tone};

/// Voices of the sound chip.
pub const VOICES: usize = 24;
/// The level word of a voice that was just taken.
pub const LEVEL_TAKEN: u16 = 0x7FFF;
/// Words of the marks of silent voices that go round, and how many of them, from the
/// first on, free a voice that is marked in all of them.
const MARKS: usize = 16;
const MARKS_READ: usize = 15;
/// The controls of a channel's volume and pan.
const VOLUME_CONTROL: u8 = 7;
const PAN_CONTROL: u8 = 10;
/// The mode bit of a tone that asks for reverb.
const REVERB: u8 = 4;
/// Events done in one step before a sequence is taken for one that never waits.
const EVENTS_A_STEP: usize = 100_000;

/// What the library remembers of a voice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Voice {
    /// Whether a key holds the voice. Cleared when the key is let go and when the
    /// voice was silent long enough.
    pub in_use: bool,
    /// The level read at the start of the step, [`LEVEL_TAKEN`] behind a key pressed.
    pub level: u16,
    /// Keys pressed, on any voice, since this one was taken.
    pub age: u16,
    pub priority: u8,
    /// The sequence that pressed it, its program, key, tone of the program, loudness
    /// and channel.
    pub owner: Option<u16>,
    pub program: u8,
    pub key: u8,
    pub tone: usize,
    pub loudness: u8,
    pub channel: u8,
    /// The pan of the channel when the key was pressed.
    pub pan: u8,
}

/// The words of a voice that changed and are given to the chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Words {
    pub volumes: Option<[u16; 2]>,
    pub pitch: Option<u16>,
    /// The sample, counted from 1.
    pub sample: Option<usize>,
    pub envelope: Option<[u16; 2]>,
}

/// What the chip is given at the start of a step: what the events of the step before
/// asked for. The masks have a bit for every voice.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Given {
    /// The voices with changed words, in the order of their numbers.
    pub words: Vec<(usize, Words)>,
    /// Voices that are let go, and then voices that are started.
    pub off: u32,
    pub on: u32,
    /// The voices with reverb, all of them, changed or not.
    pub reverb: u32,
}

/// The voices of the chip as the library keeps them, shared by every sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Voices {
    voices: [Voice; VOICES],
    marks: [u32; MARKS],
    newest: usize,
    words: [Words; VOICES],
    off: u32,
    on: u32,
    reverb: u32,
}

impl Default for Voices {
    fn default() -> Self {
        Self::new()
    }
}

impl Voices {
    pub fn new() -> Self {
        Self {
            voices: [Voice::default(); VOICES],
            marks: [0; MARKS],
            newest: 0,
            words: [Words::default(); VOICES],
            off: 0,
            on: 0,
            reverb: 0,
        }
    }

    /// What the library remembers of every voice.
    pub fn voices(&self) -> &[Voice; VOICES] {
        &self.voices
    }

    /// The start of a step, in front of the sequences: reads the level of every voice
    /// with `level`, frees the voices that were silent long enough, and answers what
    /// the chip is to be given now.
    pub fn step(&mut self, level: impl Fn(usize) -> u16) -> Given {
        self.newest = (self.newest + 1) % MARKS;
        self.marks[self.newest] = 0;
        for (number, voice) in self.voices.iter_mut().enumerate() {
            voice.level = level(number);
            if voice.level == 0 {
                self.marks[self.newest] |= 1 << number;
            }
        }
        let silent = self.marks[..MARKS_READ]
            .iter()
            .fold(u32::MAX, |all, marks| all & marks);
        for (number, voice) in self.voices.iter_mut().enumerate() {
            if silent & 1 << number != 0 {
                voice.in_use = false;
            }
        }
        self.on &= !self.off;
        let words = std::mem::take(&mut self.words);
        let changed = words.into_iter().enumerate();
        let given = Given {
            words: changed
                .filter(|(_, words)| *words != Words::default())
                .collect(),
            off: self.off,
            on: self.on,
            reverb: self.reverb,
        };
        (self.off, self.on) = (0, 0);
        given
    }

    /// The voice a tone of that priority gets, if any: a free one, or the one with the
    /// lowest priority not above the tone's, the quietest of those, the oldest of
    /// those.
    fn take(&mut self, priority: u8) -> Option<usize> {
        let free = |voice: &Voice| !voice.in_use && voice.level == 0;
        let found = self.voices.iter().position(free).or_else(|| {
            let mut found: Option<usize> = None;
            let mut lowest = priority;
            for (number, voice) in self.voices.iter().enumerate() {
                let better = match found.map(|found| &self.voices[found]) {
                    _ if voice.priority < lowest => true,
                    _ if voice.priority > lowest => false,
                    None => true,
                    Some(best) => (voice.level, best.age) < (best.level, voice.age),
                };
                if better {
                    (found, lowest) = (Some(number), voice.priority);
                }
            }
            found
        })?;
        for voice in &mut self.voices {
            voice.age = voice.age.wrapping_add(1);
        }
        (self.voices[found].age, self.voices[found].priority) = (0, priority);
        Some(found)
    }

    /// Gives a tone a voice, if one can be taken, and notes what the chip is to be
    /// told of it. `held` is what the library remembers of the voice. Answers the voice.
    fn hold(&mut self, held: Voice, tone: &Tone, levels: Option<Levels>) -> Option<usize> {
        let taken = self.take(tone.priority)?;
        self.voices[taken] = Voice {
            in_use: true,
            level: LEVEL_TAKEN,
            age: self.voices[taken].age,
            priority: tone.priority,
            ..held
        };
        for marks in &mut self.marks {
            *marks &= !(1 << taken);
        }
        let words = &mut self.words[taken];
        words.sample = Some(tone.sample);
        words.envelope = Some(tone.envelope);
        words.volumes = levels.as_ref().map(sound::volumes);
        words.pitch = Some(tone.pitch(held.key));
        if tone.mode & REVERB != 0 {
            self.reverb |= 1 << taken;
        } else {
            self.reverb &= !(1 << taken);
        }
        self.on |= 1 << taken;
        self.off &= !(1 << taken);
        Some(taken)
    }

    pub fn press_tone(
        &mut self,
        owner: u16,
        bank: &Bank,
        (number, index): (u8, usize),
        key: u8,
        sides: [u16; 2],
        mono: bool,
    ) -> Option<usize> {
        let (volume, of_program, tone) = bank.one_tone(number, index)?;
        if tone.sample == 0 {
            return None;
        }
        let (loudness, pan) = loudness_and_pan(sides);
        let levels = Levels {
            loudness,
            bank: bank.volume,
            program: volume,
            tone: tone.volume,
            pans: [tone.pan, of_program, pan],
            sequence: None,
            mono,
        };
        let held = Voice {
            owner: Some(owner),
            program: number,
            key,
            tone: index,
            loudness,
            pan,
            ..Voice::default()
        };
        self.hold(held, tone, Some(levels))
    }

    /// Lets the voice of a number go, the library's `0x80067554`: whatever the voice
    /// plays by now. Nothing for a number that is no voice.
    pub fn lift(&mut self, number: usize) {
        if number < VOICES {
            self.let_go(number);
        }
    }

    pub fn press(
        &mut self,
        owner: u16,
        bank: &Bank,
        number: u8,
        key: u8,
        sides: [u16; 2],
        mono: bool,
    ) {
        let Some(program) = bank.program(number) else {
            return;
        };
        let (loudness, pan) = loudness_and_pan(sides);
        for (index, tone) in program.tones.iter().enumerate() {
            if !tone.answers(key) {
                continue;
            }
            let levels = Levels {
                loudness,
                bank: bank.volume,
                program: program.volume,
                tone: tone.volume,
                pans: [tone.pan, program.pan, pan],
                sequence: None,
                mono,
            };
            let held = Voice {
                owner: Some(owner),
                program: number,
                key,
                tone: index,
                loudness,
                pan,
                ..Voice::default()
            };
            self.hold(held, tone, Some(levels));
        }
    }

    /// Every voice is let go, and no changed word of one waits for the next step
    /// (the library's `0x80066D5C`, which writes the chip's words itself).
    pub fn all_off(&mut self) {
        for number in 0..VOICES {
            self.let_go(number);
        }
        self.words = [Words::default(); VOICES];
    }

    /// Lets a voice go.
    fn let_go(&mut self, number: usize) {
        self.voices[number].in_use = false;
        self.off |= 1 << number;
        self.on &= !(1 << number);
    }
}

/// The loudness and the pan the direct call makes of a left and a right volume: the
/// louder side is the loudness, and the pan is 64 for equal sides, below it by the
/// share of the right side when the left is louder and above it otherwise.
pub fn loudness_and_pan([left, right]: [u16; 2]) -> (u8, u8) {
    let (loudness, pan) = match left.cmp(&right) {
        std::cmp::Ordering::Equal => (left, 64),
        std::cmp::Ordering::Greater => (left, right * 64 / left),
        std::cmp::Ordering::Less => (right, 127 - left * 64 / right),
    };
    (loudness as u8, pan as u8)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Fade {
    /// What is added to both volumes by the end, and the steps until then.
    change: i32,
    time: u32,
    /// Steps gone by, and what was added so far.
    steps: u32,
    done: i32,
}

/// One sequence that plays on a bank. See the module's start.
#[derive(Debug, Clone)]
pub struct Stepper<'a> {
    /// The number the voices know the sequence by.
    owner: u16,
    bank: &'a Bank,
    events: Vec<Event>,
    ticks: u16,
    /// The event that is done next, the tenths of a tick left until then and the
    /// tenths a step.
    at: usize,
    left: u32,
    step: u32,
    /// The tenths a step the sequence was opened with.
    first_step: u32,
    playing: bool,
    /// How often the sequence is played through (0: without end) and how often it was.
    times: u8,
    played: u8,
    /// The loop: whether a start waits for its count, the count, and the event the
    /// loop goes back to.
    waits: bool,
    count: u8,
    back: usize,
    programs: [u8; 16],
    pans: [u8; 16],
    volumes: [u8; 16],
    /// The left and right volume of the sequence, and the setting "one loudspeaker".
    sides: [u16; 2],
    mono: bool,
    fade: Option<Fade>,
}

impl<'a> Stepper<'a> {
    /// Opens a sequence and starts it. `None` for a sequence so slow that the library
    /// would count steps for a tick, which no sequence of the disc is.
    pub fn start(
        owner: u16,
        sequence: &Sequence<'_>,
        bank: &'a Bank,
        sides: [u16; 2],
        times: u8,
    ) -> Result<Option<Self>, SoundError> {
        let events: Vec<Event> = sequence.events().collect::<Result<_, _>>()?;
        let Some(step) = sequence.ticks_a_step() else {
            return Ok(None);
        };
        Ok(Some(Self {
            owner,
            bank,
            left: events.first().map_or(0, |event| event.wait) * sound::WAIT_UNITS,
            events,
            ticks: sequence.ticks,
            at: 0,
            step,
            first_step: step,
            playing: true,
            times,
            played: 0,
            waits: false,
            count: 0,
            back: 0,
            programs: std::array::from_fn(|channel| channel as u8),
            pans: [64; 16],
            volumes: [127; 16],
            sides,
            mono: false,
            fade: None,
        }))
    }

    /// Sets "one loudspeaker" for the keys pressed from here on.
    pub fn set_mono(&mut self, mono: bool) {
        self.mono = mono;
    }

    /// Whether the sequence still plays.
    pub fn playing(&self) -> bool {
        self.playing
    }

    /// The left and right volume of the sequence. While it plays each is 127 at most
    /// and every voice it holds gets its volumes anew, from the channel's volume as it
    /// is now and the pan its key was pressed with; otherwise they are only kept.
    pub fn set_sides(&mut self, voices: &mut Voices, sides: [u16; 2]) {
        self.sides = sides;
        if self.playing {
            self.level_all(voices, sides);
        }
    }

    /// The two volumes, each 127 at most, and the volumes of every voice the sequence
    /// holds anew.
    fn level_all(&mut self, voices: &mut Voices, sides: [u16; 2]) {
        self.sides = sides.map(|side| side.min(127));
        for number in 0..VOICES {
            let voice = voices.voices[number];
            if voice.owner != Some(self.owner) {
                continue;
            }
            let of_channel = [self.volumes[usize::from(voice.channel)], voice.pan];
            let levels = self.levels(voice.loudness, of_channel, voice.program, voice.tone);
            if let Some(levels) = levels {
                voices.words[number].volumes = Some(sound::volumes(&levels));
            }
        }
    }

    /// Asks for a fade: both volumes go down by `loudness` over `time` steps, in
    /// equal shares, and stay at 0 when they reach it. The sequence plays on. Nothing
    /// for a sequence that does not play, and for no loudness or no time.
    pub fn fade(&mut self, loudness: u16, time: u32) {
        if self.playing && loudness != 0 && time != 0 {
            self.fade = Some(Fade {
                change: -i32::from(loudness),
                time,
                steps: 0,
                done: 0,
            });
        }
    }

    /// The fade's part of a step, behind the events.
    fn fade_on(&mut self, voices: &mut Voices) {
        let Some(fade) = &mut self.fade else {
            return;
        };
        fade.steps = fade.steps.saturating_add(1);
        if fade.steps > fade.time {
            return;
        }
        // The share of the whole change that is due, cut off, less what was added.
        let more = fade.change * fade.steps as i32 / fade.time as i32 - fade.done;
        if more == 0 {
            return;
        }
        fade.done += more;
        let sides = self.sides;
        self.level_all(
            voices,
            sides.map(|side| (i32::from(side) + more).clamp(0, 127) as u16),
        );
    }

    /// Stops the sequence: every voice it holds is let go, and it is as it was
    /// opened, but for its two volumes.
    pub fn stop(&mut self, voices: &mut Voices) {
        self.playing = false;
        self.fade = None;
        for number in 0..VOICES {
            if voices.voices[number].owner == Some(self.owner) {
                voices.let_go(number);
            }
        }
        (self.at, self.step, self.played) = (0, self.first_step, 0);
        (self.waits, self.count, self.back) = (false, 0, 0);
        self.left = self.wait();
        self.programs = std::array::from_fn(|channel| channel as u8);
        (self.pans, self.volumes) = ([64; 16], [127; 16]);
    }

    /// The tenths of a tick the sequence goes on with each step.
    pub fn tenths_a_step(&self) -> u32 {
        self.step
    }

    /// One step of the library, a 60th of a second: the events whose ticks run out in
    /// it are done on the voices.
    pub fn step(&mut self, voices: &mut Voices) {
        if !self.playing {
            return;
        }
        self.events_of_a_step(voices);
        self.fade_on(voices);
    }

    /// The events whose ticks run out in this step.
    fn events_of_a_step(&mut self, voices: &mut Voices) {
        if self.left > self.step {
            self.left -= self.step;
            return;
        }
        let mut sum = self.left;
        for _ in 0..EVENTS_A_STEP {
            let wait = self.event(voices);
            sum += wait;
            if wait != 0 && sum >= self.step {
                self.left = sum - self.step;
                return;
            }
        }
        self.playing = false;
    }

    /// The wait in front of the event that is done next, in tenths of a tick.
    fn wait(&self) -> u32 {
        let next = self.events.get(self.at);
        next.map_or(0, |event| event.wait) * sound::WAIT_UNITS
    }

    /// Does the next event and answers the wait behind it.
    fn event(&mut self, voices: &mut Voices) -> u32 {
        let Some(event) = self.events.get(self.at).copied() else {
            self.playing = false;
            return self.step;
        };
        self.at += 1;
        match event.asked {
            Asked::Note {
                channel,
                key,
                loudness: 0,
            } => self.lift(voices, channel, key),
            Asked::Note {
                channel,
                key,
                loudness,
            } => self.press(voices, channel, key, loudness),
            Asked::Program { channel, program } => {
                self.programs[usize::from(channel)] = program;
            }
            Asked::Bend { channel, bytes } => self.bend(voices, channel, bytes[1]),
            Asked::Tempo(tempo) => {
                let beats = sound::beats_of_an_event(tempo);
                if let Some(step) = beats.and_then(|beats| sound::ticks_a_step(self.ticks, beats)) {
                    self.step = step;
                }
            }
            Asked::Control {
                channel,
                control,
                value,
            } => return self.control(voices, channel, control, value),
            Asked::End => {
                self.played = self.played.wrapping_add(1);
                self.at = 0;
                self.waits = false;
                if self.times == 0 || self.played < self.times {
                    return 0;
                }
                self.playing = false;
                self.back = 0;
                for number in 0..VOICES {
                    if voices.voices[number].owner == Some(self.owner) {
                        voices.let_go(number);
                    }
                }
                return self.step;
            }
        }
        self.wait()
    }

    /// A control: the channel's volume and pan, and the loop.
    fn control(&mut self, voices: &mut Voices, channel: u8, control: u8, value: u8) -> u32 {
        match (control, value) {
            (VOLUME_CONTROL, _) => {
                self.volumes[usize::from(channel)] = value;
                self.level(voices, channel);
            }
            (PAN_CONTROL, _) => {
                self.pans[usize::from(channel)] = value;
                self.level(voices, channel);
            }
            (sound::LOOP_CONTROL, sound::LOOP_START) => {
                (self.waits, self.back) = (true, self.at);
            }
            (sound::LOOP_COUNT_CONTROL, _) if self.waits => {
                (self.waits, self.count) = (false, value);
            }
            (sound::LOOP_CONTROL, sound::LOOP_END) => match self.count {
                0 => {}
                sound::LOOP_FOR_EVER.. => {
                    self.at = self.back;
                    return 0;
                }
                _ => {
                    // The wait behind the end is waited, then the loop or what
                    // follows it.
                    let wait = self.wait();
                    self.count -= 1;
                    if self.count != 0 {
                        self.at = self.back;
                    }
                    return wait;
                }
            },
            _ => {}
        }
        self.wait()
    }

    /// What the volumes of a voice of this sequence are made of.
    fn levels(&self, loudness: u8, channel: [u8; 2], program: u8, tone: usize) -> Option<Levels> {
        let program = self.bank.program(program)?;
        let tone = program.tones.get(tone)?;
        Some(Levels {
            loudness,
            bank: self.bank.volume,
            program: program.volume,
            tone: tone.volume,
            pans: [tone.pan, program.pan, channel[1]],
            sequence: Some((channel[0], self.sides)),
            mono: self.mono,
        })
    }

    /// A key pressed: every tone of the channel's program that answers it gets a voice.
    fn press(&mut self, voices: &mut Voices, channel: u8, key: u8, loudness: u8) {
        let number = self.programs[usize::from(channel)];
        let Some(program) = self.bank.program(number) else {
            return;
        };
        let of_channel = [
            self.volumes[usize::from(channel)],
            self.pans[usize::from(channel)],
        ];
        for (index, tone) in program.tones.iter().enumerate() {
            if !tone.answers(key) {
                continue;
            }
            let held = Voice {
                owner: Some(self.owner),
                program: number,
                key,
                tone: index,
                loudness,
                channel,
                pan: of_channel[1],
                ..Voice::default()
            };
            let levels = self.levels(loudness, of_channel, number, index);
            voices.hold(held, tone, levels);
        }
    }

    /// The voices of this sequence that play the channel's program, in use or not: the
    /// library picks them by program, not by channel.
    fn of_program(&self, voices: &Voices, channel: u8) -> Vec<usize> {
        let (owner, program) = (Some(self.owner), self.programs[usize::from(channel)]);
        let all = voices.voices.iter().enumerate();
        all.filter(|(_, voice)| voice.owner == owner && voice.program == program)
            .map(|(number, _)| number)
            .collect()
    }

    /// A key let go: every voice with that program and key.
    fn lift(&mut self, voices: &mut Voices, channel: u8, key: u8) {
        for number in self.of_program(voices, channel) {
            if voices.voices[number].key == key {
                voices.let_go(number);
            }
        }
    }

    /// A bend: the pitch of every voice of the channel's program anew.
    fn bend(&mut self, voices: &mut Voices, channel: u8, value: u8) {
        for number in self.of_program(voices, channel) {
            let voice = voices.voices[number];
            let program = self.bank.program(voice.program);
            if let Some(tone) = program.and_then(|program| program.tones.get(voice.tone)) {
                voices.words[number].pitch = Some(tone.bent_pitch(voice.key, value));
            }
        }
    }

    /// The volumes of every voice of the channel's program anew, behind a new volume or
    /// pan of the channel. A volume or a pan of 0 counts as 1 here.
    fn level(&mut self, voices: &mut Voices, channel: u8) {
        let of_channel = [
            self.volumes[usize::from(channel)].max(1),
            self.pans[usize::from(channel)].max(1),
        ];
        for number in self.of_program(voices, channel) {
            let voice = voices.voices[number];
            let levels = self.levels(voice.loudness, of_channel, voice.program, voice.tone);
            if let Some(levels) = levels {
                voices.words[number].volumes = Some(sound::volumes(&levels));
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A tone of the test bank: priority, mode, the lowest and the highest key.
    pub(crate) type TestTone = (u8, u8, u8, u8);

    /// The head of a bank of volume 127 with one sample and the programs given, whose
    /// tones have volume 127, the pan in the middle, the centre key 60 and a bend range
    /// of two half tones.
    pub(crate) fn bank(programs: &[(usize, &[TestTone])]) -> Bank {
        let mut head = vec![0; 32 + 128 * 16];
        head[..4].copy_from_slice(&sound::BANK_MARK);
        head[18..20].copy_from_slice(&(programs.len() as u16).to_le_bytes());
        head[22..24].copy_from_slice(&1u16.to_le_bytes());
        head[24..26].copy_from_slice(&[127, 64]);
        for (number, tones) in programs {
            let record = 32 + number * 16;
            head[record..record + 5].copy_from_slice(&[tones.len() as u8, 127, 0, 0, 64]);
            let mut records = vec![0; 16 * 32];
            for (tone, &(priority, mode, low, high)) in tones.iter().enumerate() {
                let t = &mut records[tone * 32..][..32];
                t[..8].copy_from_slice(&[priority, mode, 127, 64, 60, 0, low, high]);
                t[12..14].copy_from_slice(&[2, 2]);
                t[16..20].copy_from_slice(&[0xFF, 0x80, 0xC0, 0x5F]);
                t[22] = 1;
            }
            head.extend_from_slice(&records);
        }
        let mut sizes = vec![0; 512];
        sizes[2] = 4;
        head.extend_from_slice(&sizes);
        Bank::parse(&head).expect("bank")
    }

    /// A set of one sequence of 480 ticks and half a second a quarter note: 160 tenths
    /// of a tick, 16 ticks, a step.
    pub(crate) fn set(data: &[u8]) -> Vec<u8> {
        let mut bytes = sound::SET_MARK.to_vec();
        bytes.extend_from_slice(&[0, 0, 0, 0, 0x01, 0xE0, 0x07, 0xA1, 0x20, 4, 2]);
        bytes.extend_from_slice(&(data.len() as u32).to_be_bytes());
        bytes.extend_from_slice(data);
        bytes.resize(bytes.len().next_multiple_of(4), 0);
        bytes
    }

    fn start<'a>(bank: &'a Bank, data: &[u8], times: u8) -> Stepper<'a> {
        let bytes = set(data);
        let found = sound::sequences(&bytes).expect("set");
        let stepper = Stepper::start(7, &found[0], bank, [127, 127], times);
        stepper.expect("events").expect("a step")
    }

    const WHOLE: &[TestTone] = &[(0, REVERB, 0, 127)];

    #[test]
    fn a_key_without_a_sequence_sounds_the_tones_that_answer_it_without_the_square() {
        // Two tones of program 5: one for every key, one for the keys above 60.
        let bank = bank(&[(5, &[(0, REVERB, 0, 127), (0, 0, 61, 127)])]);
        let mut voices = Voices::new();
        voices.press(33, &bank, 5, 60, [64, 64], false);
        // A program the bank has not is silent.
        voices.press(33, &bank, 6, 60, [64, 64], false);
        let given = voices.step(|_| 0);
        assert_eq!((given.on, given.off, given.reverb), (1, 0, 1));
        // The loudness 64 times the bank, the program and the tone at 127, in the
        // middle: not squared, and no volume of a sequence in it.
        let words = Words {
            volumes: Some([8256; 2]),
            pitch: Some(0x1000),
            sample: Some(1),
            envelope: Some([0x80FF, 0x5FC0]),
        };
        assert_eq!(given.words, [(0, words)]);
        let voice = voices.voices()[0];
        assert_eq!((voice.owner, voice.program, voice.key), (Some(33), 5, 60));
        assert_eq!((voice.loudness, voice.pan), (64, 64));
    }

    #[test]
    fn a_kept_sound_is_one_tone_on_a_voice_that_its_number_lets_go() {
        // Program 5 as above: its first tone answers every key, its second the keys
        // above 60; program 7 has a tone for the keys above 60 alone.
        let bank = bank(&[
            (5, &[(0, REVERB, 0, 127), (0, 0, 61, 127)]),
            (7, &[(0, 0, 61, 127)]),
        ]);
        let mut voices = Voices::new();
        // Tone 0 alone, and whatever keys it answers.
        assert_eq!(
            voices.press_tone(33, &bank, (5, 0), 60, [64, 64], false),
            Some(0)
        );
        assert_eq!(
            voices.press_tone(33, &bank, (7, 0), 60, [64, 64], false),
            Some(1)
        );
        // A tone the program has not, and a program behind the last one in use, get
        // no voice.
        assert_eq!(
            voices.press_tone(33, &bank, (7, 1), 60, [64, 64], false),
            None
        );
        assert_eq!(
            voices.press_tone(33, &bank, (8, 0), 60, [64, 64], false),
            None
        );
        // A program without tones in front of one in use takes a voice: the tone of
        // the next program in use, at the volume of its own record, which is 0 here.
        assert_eq!(bank.program(6), None);
        assert_eq!(
            voices.press_tone(33, &bank, (6, 0), 60, [64, 64], false),
            Some(2)
        );
        let given = voices.step(|_| 0);
        assert_eq!((given.on, given.off), (0b111, 0));
        assert_eq!(given.words[0].1.volumes, Some([8256; 2]));
        assert_eq!(given.words[2].1.volumes, Some([0; 2]));
        assert_eq!(given.words[2].1.sample, given.words[1].1.sample);
        assert_eq!(voices.voices()[2].program, 6);
        // The number lets the voice go; a number that is no voice does nothing.
        voices.lift(1);
        voices.lift(usize::MAX);
        voices.lift(VOICES);
        let given = voices.step(|_| LEVEL_TAKEN);
        assert_eq!((given.on, given.off), (0, 0b10));
        assert!(voices.voices()[0].in_use && !voices.voices()[1].in_use);
        assert!(voices.voices()[2].in_use);
        // Every voice is let go at once, and a key pressed in the same step gives the
        // chip neither its words nor its start.
        let pressed = voices.press_tone(33, &bank, (5, 0), 62, [64, 64], false);
        assert!(pressed.is_some());
        voices.all_off();
        let given = voices.step(|_| LEVEL_TAKEN);
        assert_eq!((given.on, given.off), (0, (1 << VOICES) - 1));
        assert!(given.words.is_empty());
        assert!(voices.voices().iter().all(|voice| !voice.in_use));
    }

    #[test]
    fn two_volumes_become_a_loudness_and_a_pan() {
        assert_eq!(loudness_and_pan([64, 64]), (64, 64));
        assert_eq!(loudness_and_pan([0, 0]), (0, 64));
        // The left side louder: the pan goes below the middle by the right's share.
        assert_eq!(loudness_and_pan([100, 50]), (100, 32));
        assert_eq!(loudness_and_pan([100, 0]), (100, 0));
        // The right side louder: above the middle.
        assert_eq!(loudness_and_pan([50, 100]), (100, 95));
        assert_eq!(loudness_and_pan([0, 100]), (100, 127));
    }

    #[test]
    fn a_key_reaches_the_chip_a_step_after_its_tick_and_the_end_lets_every_voice_go() {
        let bank = bank(&[(5, WHOLE)]);
        // The program, a key at once, let go 48 ticks on, and the end.
        let data = [
            0x00, 0xC0, 0x05, 0x00, 0x90, 0x3C, 0x7F, 0x30, 0x3C, 0x00, 0x00, 0xFF, 0x2F, 0x00,
        ];
        let mut stepper = start(&bank, &data, 1);
        assert_eq!(stepper.tenths_a_step(), 160);
        let mut voices = Voices::new();

        assert_eq!(voices.step(|_| 0), Given::default());
        stepper.step(&mut voices);
        let voice = voices.voices()[0];
        assert!(voice.in_use && voice.level == LEVEL_TAKEN && voice.owner == Some(7));
        assert_eq!((voice.program, voice.key, voice.tone), (5, 60, 0));

        // What the first step asked for is given at the start of the second.
        let pressed = Words {
            volumes: Some([sound::VOLUME_LIMIT; 2]),
            pitch: Some(sound::PITCH_AS_RECORDED),
            sample: Some(1),
            envelope: Some([0x80FF, 0x5FC0]),
        };
        let given = voices.step(|_| 0);
        assert_eq!(given.words, [(0, pressed)]);
        assert_eq!((given.off, given.on, given.reverb), (0, 1, 1));
        // 48 ticks are three steps: nothing in the second, the key let go in the third.
        stepper.step(&mut voices);
        assert!(voices.voices()[0].in_use && stepper.playing());
        assert_eq!(
            voices.step(|_| 100),
            Given {
                reverb: 1,
                ..Given::default()
            }
        );
        stepper.step(&mut voices);
        assert!(!voices.voices()[0].in_use && !stepper.playing());
        let given = voices.step(|_| 100);
        assert_eq!((given.words.len(), given.off, given.on), (0, 1, 0));
        // A sequence that is over does nothing.
        stepper.step(&mut voices);
        assert_eq!(voices.step(|_| 0).off, 0);
    }

    #[test]
    fn a_loop_goes_round_without_a_wait_or_as_often_as_its_count_says() {
        let bank = bank(&[(5, WHOLE)]);
        let data = |count: u8| {
            vec![
                0x00, 0xC0, 0x05, // the program
                0x00, 0xB0, 0x63, 0x14, // the loop's start
                0x00, 0x06, count, // and its count, by the same status
                0x00, 0x90, 0x3C, 0x7F, // a key
                0x10, 0x3C, 0x00, // let go 16 ticks on
                0x10, 0xB0, 0x63, 0x1E, // the loop's end, 16 ticks on
                0x7F, 0xFF, 0x2F, 0x00, // the end, 127 ticks on
            ]
        };
        let pressed = |count: u8, steps: usize| {
            let mut stepper = start(&bank, &data(count), 1);
            let mut voices = Voices::new();
            let mut steps_with_a_key = Vec::new();
            for step in 0..steps {
                // A voice is silent as soon as it is let go.
                let held: Vec<bool> = voices.voices().iter().map(|v| v.in_use).collect();
                let given = voices.step(|voice| if held[voice] { 100 } else { 0 });
                if given.on != 0 {
                    steps_with_a_key.push(step);
                }
                stepper.step(&mut voices);
            }
            (steps_with_a_key, stepper.playing())
        };
        // Without end: the key every second step, the wait behind the end thrown away.
        assert_eq!(pressed(127, 10), (vec![1, 3, 5, 7, 9], true));
        // Twice: the wait behind the end is waited in front of the second time and
        // behind it, and then the sequence is over.
        assert_eq!(pressed(2, 40), (vec![1, 10], false));
        // A count of 0 is no loop.
        assert_eq!(pressed(0, 40), (vec![1], false));
    }

    #[test]
    fn a_bend_and_a_volume_reach_the_voices_of_the_program_whatever_the_channel() {
        let bank = bank(&[(5, WHOLE)]);
        let data = [
            0x00, 0xC0, 0x05, 0x00, 0xC1, 0x05, // two channels, one program
            0x00, 0x90, 0x3C, 0x7F, // a key of channel 0
            0x00, 0x92, 0x3C, 0x7F, // and one of channel 2, whose program the bank has not
            0x10, 0xE1, 0x55, 0x7F, // channel 1 bends all the way up
            0x10, 0xB1, 0x07, 0x40, // and is turned down to about half
            0x10, 0xB1, 0x0A, 0x00, // and all to the left
            0x10, 0xFF, 0x2F, 0x00,
        ];
        let mut stepper = start(&bank, &data, 1);
        let mut voices = Voices::new();
        let mut given = Vec::new();
        for _ in 0..5 {
            given.push(voices.step(|_| 100).words);
            stepper.step(&mut voices);
        }
        // One voice only: the key of channel 2 is silent.
        assert_eq!(
            voices
                .voices()
                .iter()
                .filter(|voice| voice.owner.is_some())
                .count(),
            1
        );
        assert_eq!(given[1].len(), 1);
        let bent = Words {
            pitch: Some(sound::pitch(62, 0, 60, 0)),
            ..Words::default()
        };
        assert_eq!(given[2], [(0, bent)]);
        let quieter = Words {
            volumes: Some([4160; 2]),
            ..Words::default()
        };
        assert_eq!(given[3], [(0, quieter)]);
        // A pan of 0 counts as 1 for the voices that sound, so the right side is not
        // quite silent: 8256 / 63 is 131, and its square over 16383 is 1.
        let left = Words {
            volumes: Some([4160, 1]),
            ..Words::default()
        };
        assert_eq!(given[4], [(0, left)]);
    }

    #[test]
    fn two_tones_of_a_program_sound_in_two_voices_and_a_key_outside_in_none() {
        let bank = bank(&[(5, &[(0, 0, 0, 60), (0, REVERB, 60, 127)])]);
        let data = [
            0x00, 0xC0, 0x05, 0x00, 0x90, 0x3C, 0x7F, 0x00, 0x3D, 0x7F, 0x00, 0xFF, 0x2F, 0x00,
        ];
        let mut stepper = start(&bank, &data, 1);
        let mut voices = Voices::new();
        voices.step(|_| 0);
        stepper.step(&mut voices);
        let keys: Vec<(u8, usize)> = voices.voices()[..4]
            .iter()
            .map(|v| (v.key, v.tone))
            .collect();
        assert_eq!(keys, [(60, 0), (60, 1), (61, 1), (0, 0)]);
        // Reverb by the tone's mode; the end let all three go again.
        let given = voices.step(|_| 0);
        assert_eq!((given.reverb, given.off, given.on), (0b110, 0b111, 0));
    }

    #[test]
    fn a_voice_is_taken_free_or_by_priority_level_and_age() {
        let mut voices = Voices::new();
        // Every voice is free at first and they are taken in order.
        for number in 0..VOICES {
            assert_eq!(voices.take(3), Some(number));
            (voices.voices[number].in_use, voices.voices[number].level) = (true, 100);
        }
        assert_eq!(voices.voices[0].age, 23);
        // Nothing for a lower priority; for the same the oldest of the quietest.
        assert_eq!(voices.take(2), None);
        assert_eq!(voices.take(3), Some(0));
        voices.voices[7].level = 50;
        assert_eq!(voices.take(3), Some(7));
        voices.voices[7].level = 100;
        assert_eq!(voices.take(3), Some(1));
        // The lowest priority goes first, whatever its level.
        (voices.voices[9].priority, voices.voices[9].level) = (1, 30_000);
        assert_eq!(voices.take(5), Some(9));
        assert_eq!(voices.voices[9].priority, 5);
        // A voice that was let go is free only when its level is 0.
        voices.voices[4].in_use = false;
        assert_eq!(voices.take(0), None);
        voices.voices[4].level = 0;
        assert_eq!(voices.take(0), Some(4));
    }

    #[test]
    fn a_voice_that_stays_silent_is_not_in_use_after_sixteen_steps() {
        let mut voices = Voices::new();
        voices.voices[2].in_use = true;
        voices.voices[3].in_use = true;
        for _ in 0..15 {
            voices.step(|voice| if voice == 3 { 1 } else { 0 });
        }
        assert!(voices.voices[2].in_use);
        voices.step(|voice| if voice == 3 { 1 } else { 0 });
        assert!(!voices.voices[2].in_use && voices.voices[3].in_use);
    }
}
