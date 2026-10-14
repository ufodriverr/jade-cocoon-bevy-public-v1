
use crate::sound::{self, BLOCK_SIZE, BLOCK_SOUND, BlockDecoder};

/// Values of sound the chip makes in a second; the envelope goes on once for each.
pub const RATE: u32 = 44_100;
/// The highest level of an envelope.
pub const LEVEL_LIMIT: u16 = 0x7FFF;
/// Above this level a rise by the curved kind is four times slower.
const SLOW_ABOVE: i32 = 0x6000;
/// The shift at which a change is one step for every value of sound.
const PLAIN_SHIFT: u32 = 11;
/// The step of a fall by its two bits 0, and of a rise.
const FALL: i32 = -8;
const RISE: i32 = 7;

/// Where an envelope is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// The rise to the limit behind "key on".
    Attack,
    /// The fall to the level the first word names.
    Decay,
    /// What the second word asks for while the key is held.
    Sustain,
    /// The fall to silence behind "key off".
    Release,
    /// Silent.
    Off,
}

/// One change of the level: its shift, the step at shift 11, whether it is the curved
/// kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Change {
    shift: u32,
    step: i32,
    curved: bool,
}

impl Change {
    /// How many values of sound pass until the level changes, and by how much.
    fn of(self, level: i32) -> (u32, i32) {
        let mut wait = 1 << self.shift.saturating_sub(PLAIN_SHIFT);
        let mut step = self.step << PLAIN_SHIFT.saturating_sub(self.shift);
        if self.curved && self.step > 0 && level > SLOW_ABOVE {
            wait *= 4;
        }
        if self.curved && self.step < 0 {
            step = (step * level) >> 15;
        }
        (wait, step)
    }
}

/// The envelope of a voice: its level from the two envelope words of a tone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Envelope {
    words: [u16; 2],
    phase: Phase,
    level: i32,
    /// Values of sound until the level changes next.
    wait: u32,
}

impl Default for Envelope {
    fn default() -> Self {
        Self::new()
    }
}

impl Envelope {
    /// A silent voice.
    pub fn new() -> Self {
        Self {
            words: [0; 2],
            phase: Phase::Off,
            level: 0,
            wait: 0,
        }
    }

    /// "Key on" with the two envelope words: the level starts at 0 and rises.
    pub fn key_on(&mut self, words: [u16; 2]) {
        (self.words, self.level) = (words, 0);
        self.enter(Phase::Attack);
    }

    /// "Key off": the level falls to silence from where it is.
    pub fn key_off(&mut self) {
        if self.phase != Phase::Off {
            self.enter(Phase::Release);
        }
    }

    /// The level now, 0 to [`LEVEL_LIMIT`]: what the library reads back for a voice.
    pub fn level(&self) -> u16 {
        self.level as u16
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// The level the decay ends at.
    fn sustain_level(&self) -> i32 {
        (i32::from(self.words[0] & 15) + 1) * 0x800
    }

    /// The change of the phase the envelope is in.
    fn change(&self) -> Option<Change> {
        let [first, second] = self.words.map(u32::from);
        Some(match self.phase {
            Phase::Attack => Change {
                shift: first >> 10 & 31,
                step: RISE - (first >> 8 & 3) as i32,
                curved: first & 0x8000 != 0,
            },
            Phase::Decay => Change {
                shift: first >> 4 & 15,
                step: FALL,
                curved: true,
            },
            Phase::Sustain => {
                let bits = (second >> 6 & 3) as i32;
                let falls = second & 0x4000 != 0;
                Change {
                    shift: second >> 8 & 31,
                    step: if falls { FALL + bits } else { RISE - bits },
                    curved: second & 0x8000 != 0,
                }
            }
            Phase::Release => Change {
                shift: second & 31,
                step: FALL,
                curved: second & 0x20 != 0,
            },
            Phase::Off => return None,
        })
    }

    fn enter(&mut self, phase: Phase) {
        self.phase = phase;
        self.wait = self.change().map_or(0, |change| change.of(self.level).0);
    }

    /// One value of sound on: the level behind it.
    pub fn tick(&mut self) -> u16 {
        let Some(change) = self.change() else {
            return 0;
        };
        self.wait = self.wait.saturating_sub(1);
        if self.wait != 0 {
            return self.level();
        }
        let step = change.of(self.level).1;
        self.level = (self.level + step).clamp(0, i32::from(LEVEL_LIMIT));
        match self.phase {
            Phase::Attack if self.level == i32::from(LEVEL_LIMIT) => self.enter(Phase::Decay),
            Phase::Decay if self.level <= self.sustain_level() => self.enter(Phase::Sustain),
            Phase::Release if self.level == 0 => self.enter(Phase::Off),
            phase => self.enter(phase),
        }
        self.level()
    }
}

/// The pitch word that plays a sample as it was recorded: one value of the sample for
/// every value of sound.
const WHOLE_VALUE: u32 = 0x1000;
/// The chip takes a higher pitch word for this one.
const PITCH_LIMIT: u32 = 0x4000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChipVoice<'a> {
    /// The blocks of the sample, the block that is played and the one a way back
    /// leads to.
    sample: &'a [u8],
    block: usize,
    loop_block: usize,
    decoder: BlockDecoder,
    /// The values of the block, the one that sounds, and the 4096ths of a value the
    /// voice is past it.
    values: [i16; BLOCK_SOUND],
    at: usize,
    fraction: u32,
    /// The value in front of the one that sounds: what the stand-in for the chip's
    /// interpolation goes from.
    before: i16,
    pitch: u16,
    volumes: [u16; 2],
    envelope: Envelope,
}

impl Default for ChipVoice<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> ChipVoice<'a> {
    /// A silent voice.
    pub fn new() -> Self {
        Self {
            sample: &[],
            block: 0,
            loop_block: 0,
            decoder: BlockDecoder::new(),
            values: [0; BLOCK_SOUND],
            at: 0,
            fraction: 0,
            before: 0,
            pitch: 0,
            volumes: [0; 2],
            envelope: Envelope::new(),
        }
    }

    /// The pitch word and the two volume words, as the library gives them.
    pub fn set_pitch(&mut self, pitch: u16) {
        self.pitch = pitch;
    }

    pub fn set_volumes(&mut self, volumes: [u16; 2]) {
        self.volumes = volumes;
    }

    /// "Key on": the sample from its first block, the envelope from 0.
    pub fn key_on(&mut self, sample: &'a [u8], envelope: [u16; 2]) {
        (self.sample, self.block, self.loop_block) = (sample, 0, 0);
        (self.at, self.fraction, self.before) = (0, 0, 0);
        self.decoder = BlockDecoder::new();
        self.envelope.key_on(envelope);
        self.decode();
    }

    /// "Key off": the envelope falls; the sample plays on under it.
    pub fn key_off(&mut self) {
        self.envelope.key_off();
    }

    /// The level of the envelope: what the library reads back.
    pub fn level(&self) -> u16 {
        self.envelope.level()
    }

    /// Whether the voice makes a sound.
    pub fn sounding(&self) -> bool {
        self.envelope.phase() != Phase::Off
    }

    /// Makes the values of the block that is played, or ends the voice when the
    /// sample has no such block.
    fn decode(&mut self) {
        let start = self.block * BLOCK_SIZE;
        let block = self.sample.get(start..start + BLOCK_SIZE);
        let Some(block) = block.and_then(|block| <&[u8; BLOCK_SIZE]>::try_from(block).ok()) else {
            self.envelope = Envelope::new();
            return;
        };
        if block[1] & sound::BLOCK_STARTS_LOOP != 0 {
            self.loop_block = self.block;
        }
        self.values = self.decoder.decode(block);
    }

    /// One value of the sample on: into the next block at the end of this one, back
    /// to the loop's start behind a block that says so, and silent behind a block
    /// that ends the sample.
    fn advance(&mut self) {
        self.before = self.values[self.at];
        self.at += 1;
        if self.at < BLOCK_SOUND {
            return;
        }
        self.at = 0;
        let flags = self.sample[self.block * BLOCK_SIZE + 1];
        if flags & sound::BLOCK_ENDS == 0 {
            self.block += 1;
        } else if flags & sound::BLOCK_GOES_BACK != 0 {
            self.block = self.loop_block;
        } else {
            self.envelope = Envelope::new();
            return;
        }
        self.decode();
    }

    /// One value of sound: left and right.
    pub fn tick(&mut self) -> [i32; 2] {
        if !self.sounding() {
            return [0; 2];
        }
        // A stand-in for the chip's interpolation (T240): a straight line from the
        // value before to the one that sounds.
        let (from, to) = (i32::from(self.before), i32::from(self.values[self.at]));
        let value = from + (((to - from) * self.fraction as i32) >> 12);
        let value = (value * i32::from(self.envelope.level())) >> 15;
        let sides = self.volumes.map(|volume| (value * i32::from(volume)) >> 14);
        self.envelope.tick();
        self.fraction += u32::from(self.pitch).min(PITCH_LIMIT);
        while self.fraction >= WHOLE_VALUE && self.sounding() {
            self.fraction -= WHOLE_VALUE;
            self.advance();
        }
        sides
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A block of the plain kind whose 28 values are all `nibble << 12`.
    fn block(nibble: u8, flags: u8) -> [u8; BLOCK_SIZE] {
        let mut block = [nibble << 4 | nibble; BLOCK_SIZE];
        (block[0], block[1]) = (0, flags);
        block
    }

    #[test]
    fn a_voice_plays_its_sample_through_the_envelope_and_is_silent_at_its_end() {
        let sample = [block(4, 0), block(4, sound::BLOCK_ENDS), block(4, 7)].concat();
        let mut voice = ChipVoice::new();
        assert_eq!((voice.tick(), voice.sounding()), ([0; 2], false));
        voice.set_pitch(0x1000);
        voice.set_volumes([0x3FFF, 0x2000]);
        voice.key_on(&sample, [0x80FF, 0x5FC0]);
        // The level is 0 at first, then 14336, then 28672: 16384 of the sample times
        // the level over 32768, times the volume over 16384.
        assert_eq!(voice.tick(), [0, 0]);
        assert_eq!(voice.tick(), [7_167, 3_584]);
        assert_eq!(voice.tick(), [14_335, 7_168]);
        for _ in 3..6 {
            voice.tick();
        }
        assert_eq!(
            (voice.tick(), voice.level()),
            ([16_382, 8_191], LEVEL_LIMIT)
        );
        // Two blocks are 56 values; the block behind the end is not played.
        for _ in 7..55 {
            voice.tick();
        }
        assert!(voice.sounding());
        voice.tick();
        assert!(!voice.sounding() && voice.level() == 0);
        assert_eq!(voice.tick(), [0, 0]);
        // A sample of no block at all is silent at once.
        voice.key_on(&[], [0x80FF, 0x5FC0]);
        assert!(!voice.sounding());
    }

    #[test]
    fn a_voice_goes_back_to_the_loops_start_and_steps_by_its_pitch_word() {
        let (starts, goes_back) = (
            sound::BLOCK_STARTS_LOOP | sound::BLOCK_GOES_BACK,
            sound::BLOCK_ENDS | sound::BLOCK_GOES_BACK,
        );
        let sample = [block(1, 0), block(2, starts), block(3, goes_back)].concat();
        let blocks_at = |pitch: u16, ticks: usize| {
            let mut voice = ChipVoice::new();
            voice.set_pitch(pitch);
            voice.key_on(&sample, [0x80FF, 0x5FC0]);
            let mut blocks = Vec::new();
            for tick in 1..=4 * ticks {
                voice.tick();
                if tick % ticks == 0 {
                    blocks.push(voice.block);
                }
            }
            assert!(voice.sounding());
            blocks
        };
        // As recorded: 28 values of sound a block, and round between the last two.
        assert_eq!(blocks_at(0x1000, 28), [1, 2, 1, 2]);
        // An octave down, and the highest pitch the chip takes: four values at once.
        assert_eq!(blocks_at(0x0800, 56), [1, 2, 1, 2]);
        assert_eq!(blocks_at(0x4000, 7), [1, 2, 1, 2]);
        assert_eq!(blocks_at(0xFFFF, 7), [1, 2, 1, 2]);
        // Half way between two values the stand-in is half way between them.
        let mut voice = ChipVoice::new();
        voice.set_pitch(0x0800);
        voice.set_volumes([0x2000, 0x2000]);
        voice.key_on(&sample, [0x80FF, 0x5FC0]);
        for _ in 0..22 {
            voice.tick();
        }
        // Held at 32759; the values are 4096: 4094 by the level, half by the volume.
        assert_eq!(voice.tick(), [2_047; 2]);
        // Let go, the fastest release: half the level, then silence.
        voice.key_off();
        assert_eq!(voice.tick(), [2_047; 2]);
        assert_eq!(voice.tick(), [1_023; 2]);
        assert_eq!((voice.tick(), voice.sounding()), ([0; 2], false));
    }

    /// The level after so many more values of sound.
    fn after(envelope: &mut Envelope, ticks: usize) -> u16 {
        (0..ticks).map(|_| envelope.tick()).last().expect("ticks")
    }

    #[test]
    fn the_most_frequent_words_rise_at_once_hold_and_fall_in_two_values() {
        let mut envelope = Envelope::new();
        assert_eq!((envelope.tick(), envelope.phase()), (0, Phase::Off));
        envelope.key_on([0x80FF, 0x5FC0]);
        // The fastest curved rise: 7 << 11 a value, and above 0x6000 every fourth.
        assert_eq!(envelope.tick(), 14_336);
        assert_eq!(envelope.tick(), 28_672);
        assert_eq!(after(&mut envelope, 3), 28_672);
        assert_eq!(
            (envelope.tick(), envelope.phase()),
            (LEVEL_LIMIT, Phase::Decay)
        );
        // The decay ends at 0x8000, so its first step, 16 values on, is its last.
        assert_eq!(after(&mut envelope, 15), LEVEL_LIMIT);
        assert_eq!(
            (envelope.tick(), envelope.phase()),
            (32_759, Phase::Sustain)
        );
        // The sustain falls by 5 every 1,048,576 values: it holds.
        assert_eq!(after(&mut envelope, 100_000), 32_759);
        // The fastest straight fall: 8 << 11 a value.
        envelope.key_off();
        assert_eq!(
            (envelope.tick(), envelope.phase()),
            (16_375, Phase::Release)
        );
        assert_eq!((envelope.tick(), envelope.phase()), (0, Phase::Off));
        assert_eq!(envelope.tick(), 0);
        // "Key off" on a silent voice does nothing.
        envelope.key_off();
        assert_eq!(envelope.phase(), Phase::Off);
    }

    #[test]
    fn a_release_is_straight_or_curved_by_its_bit_and_slower_by_its_shift() {
        // Shift 11: 8 a value, so 4,095 values from the level that was held.
        let mut envelope = Envelope::new();
        envelope.key_on([0x80FF, 0x5FCB]);
        after(&mut envelope, 22);
        envelope.key_off();
        assert_eq!(after(&mut envelope, 100), 32_759 - 800);
        assert_eq!(after(&mut envelope, 3_994), 7);
        assert_eq!((envelope.tick(), envelope.phase()), (0, Phase::Off));
        // Shift 13: 8 every fourth value.
        envelope.key_on([0x80FF, 0x5FCD]);
        after(&mut envelope, 22);
        envelope.key_off();
        assert_eq!(after(&mut envelope, 400), 32_759 - 800);
        // Curved, shift 5: 512 a value at the top and less with the level.
        envelope.key_on([0x80FF, 0x5FE5]);
        after(&mut envelope, 22);
        envelope.key_off();
        assert_eq!(envelope.tick(), 32_759 - 512);
        assert_eq!(envelope.tick(), 32_247 - 504);
    }

    #[test]
    fn a_decay_falls_to_the_level_of_the_first_word_and_a_sustain_can_rise() {
        // A straight rise of 4 << 11 a value, a decay of 8 << 3 of the level a value
        // down to 0x4000, then a straight rise of 7 a value.
        let mut envelope = Envelope::new();
        envelope.key_on([0x0387, 0x0B00]);
        assert_eq!(after(&mut envelope, 3), 24_576);
        assert_eq!(
            (envelope.tick(), envelope.phase()),
            (LEVEL_LIMIT, Phase::Decay)
        );
        assert_eq!(envelope.tick(), 32_767 - 64);
        let mut ticks = 1;
        while envelope.phase() == Phase::Decay {
            envelope.tick();
            ticks += 1;
        }
        // A 512th of the level a value: half the level takes about 355 values.
        assert!(envelope.level() <= 0x4000 && envelope.level() > 0x4000 - 34);
        assert!((350..=356).contains(&ticks), "{ticks}");
        let held = envelope.level();
        assert_eq!(after(&mut envelope, 10), held + 70);
    }
}
