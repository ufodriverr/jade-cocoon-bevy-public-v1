
use crate::chip::{self, ChipVoice};
use crate::reverb::Reverb;
use crate::sequencer::{Given, Stepper, VOICES, Voices};
use crate::sound::{self, Bank, Sequence, SoundError};

/// Values of sound the chip makes in one step of the library.
pub const VALUES_A_STEP: usize = (chip::RATE / sound::STEPS_A_SECOND) as usize;
/// The main volume the library's start gives the chip, on both sides.
pub const MAIN_VOLUME: u16 = 0x3FFF;

/// A sequence that was started, with the bank it plays on.
#[derive(Debug, Clone)]
struct Playing<'a> {
    owner: u16,
    stepper: Stepper<'a>,
    bank: &'a Bank,
    body: &'a [u8],
}

/// The library's voices, the chip's voices and the sequences that play on them.
#[derive(Debug, Clone)]
pub struct Player<'a> {
    voices: Voices,
    chip: [ChipVoice<'a>; VOICES],
    /// The sample and the envelope words each voice was given last.
    asked: [(&'a [u8], [u16; 2]); VOICES],
    playing: Vec<Playing<'a>>,
    /// The banks of the sounds played without a sequence, by the number their
    /// voices are known by, each with its body.
    sounds: Vec<(u16, &'a Bank, &'a [u8])>,
    main: [u16; 2],
    /// The library's word "one loudspeaker" (`0x800A01A8`).
    mono: bool,
    /// The chip's reverb, if the player has one, and the voices that go into it.
    reverb: Option<Reverb>,
    wet: u32,
    limited: u64,
}

impl Default for Player<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Player<'a> {
    /// Silent voices and no sequence.
    pub fn new() -> Self {
        Self {
            voices: Voices::new(),
            chip: std::array::from_fn(|_| ChipVoice::new()),
            asked: [(&[], [0; 2]); VOICES],
            playing: Vec::new(),
            sounds: Vec::new(),
            main: [MAIN_VOLUME; 2],
            mono: false,
            reverb: None,
            wet: 0,
            limited: 0,
        }
    }

    /// Gives the player a reverb: from here on the voices whose tones ask for it go
    /// into it, and what it makes is added to the mix.
    pub fn set_reverb(&mut self, reverb: Reverb) {
        self.reverb = Some(reverb);
    }

    pub fn set_mono(&mut self, mono: bool) {
        self.mono = mono;
        self.main = [MAIN_VOLUME; 2];
        for playing in &mut self.playing {
            playing.stepper.set_mono(mono);
        }
    }

    /// Whether the library was told one loudspeaker.
    pub fn mono(&self) -> bool {
        self.mono
    }

    /// Starts a sequence on a bank and its body, under a number no other sequence of
    /// this player has. `false` for a sequence the stepper does not take.
    pub fn start(
        &mut self,
        owner: u16,
        sequence: &Sequence<'_>,
        bank: &'a Bank,
        body: &'a [u8],
        sides: [u16; 2],
        times: u8,
    ) -> Result<bool, SoundError> {
        let Some(mut stepper) = Stepper::start(owner, sequence, bank, sides, times)? else {
            return Ok(false);
        };
        stepper.set_mono(self.mono);
        self.playing.retain(|playing| playing.owner != owner);
        self.playing.push(Playing {
            owner,
            stepper,
            bank,
            body,
        });
        Ok(true)
    }

    /// Plays a sound without a sequence, the library's direct call: the tones of a
    /// program of the bank that answer the key, at a left and a right volume. `owner`
    /// is a number no sequence and no other bank of this player has.
    pub fn sound(
        &mut self,
        owner: u16,
        (bank, body): (&'a Bank, &'a [u8]),
        program: u8,
        key: u8,
        sides: [u16; 2],
    ) {
        self.sounds.retain(|(known, ..)| *known != owner);
        self.sounds.push((owner, bank, body));
        (self.voices).press(owner, bank, program, key, sides, self.mono);
    }

    /// Plays tone 0 of a program on a voice that is kept, as the library's
    /// `0x800671FC` does, and answers the voice, if one could be taken. `owner` is as
    /// for [`Player::sound`].
    pub fn sound_kept(
        &mut self,
        owner: u16,
        (bank, body): (&'a Bank, &'a [u8]),
        program: u8,
        key: u8,
        sides: [u16; 2],
    ) -> Option<usize> {
        self.sounds.retain(|(known, ..)| *known != owner);
        self.sounds.push((owner, bank, body));
        (self.voices).press_tone(owner, bank, (program, 0), key, sides, self.mono)
    }

    /// Lets the voice of a number go, as the library's `0x80067554` does.
    pub fn let_go(&mut self, voice: usize) {
        self.voices.lift(voice);
    }

    pub fn all_off(&mut self) {
        self.voices.all_off();
        for voice in &mut self.chip {
            voice.set_volumes([0; 2]);
        }
    }

    /// Whether the sequence of that number still plays.
    pub fn playing(&self, owner: u16) -> bool {
        let found = self.playing.iter().find(|playing| playing.owner == owner);
        found.is_some_and(|playing| playing.stepper.playing())
    }

    /// Sets the left and right volume of the sequence of that number.
    pub fn set_sides(&mut self, owner: u16, sides: [u16; 2]) {
        let found = self.playing.iter_mut().find(|p| p.owner == owner);
        if let Some(playing) = found {
            playing.stepper.set_sides(&mut self.voices, sides);
        }
    }

    /// Asks for a fade of the sequence of that number: its volumes go down by
    /// `loudness` over `time` steps.
    pub fn fade(&mut self, owner: u16, loudness: u16, time: u32) {
        let found = self.playing.iter_mut().find(|p| p.owner == owner);
        if let Some(playing) = found {
            playing.stepper.fade(loudness, time);
        }
    }

    /// Stops the sequence of that number: its voices are let go.
    pub fn stop(&mut self, owner: u16) {
        let found = self.playing.iter_mut().find(|p| p.owner == owner);
        if let Some(playing) = found {
            playing.stepper.stop(&mut self.voices);
        }
    }

    /// How many voices of the chip make a sound.
    pub fn sounding(&self) -> usize {
        self.chip.iter().filter(|voice| voice.sounding()).count()
    }

    /// Values of the mix that met the limit of 16 bits, on either side.
    pub fn limited(&self) -> u64 {
        self.limited
    }

    /// The library's memory of the voices.
    pub fn voices(&self) -> &Voices {
        &self.voices
    }

    /// One step of the library, a 60th of a second: the voices' step, what it answers
    /// given to the chip, every sequence stepped, and then the values of sound the
    /// chip makes until the next step, left and right, added to `out`.
    pub fn step(&mut self, out: &mut Vec<[i16; 2]>) {
        let chip = &self.chip;
        let given = self.voices.step(|voice| chip[voice].level());
        self.give(given);
        for playing in &mut self.playing {
            playing.stepper.step(&mut self.voices);
        }
        out.extend((0..VALUES_A_STEP).map(|_| self.tick()));
    }

    /// Gives the chip what the voices' step answered: the changed words, then "key
    /// off", then "key on".
    fn give(&mut self, given: Given) {
        self.wet = given.reverb;
        for (voice, words) in given.words {
            if let Some(number) = words.sample {
                let owner = self.voices.voices()[voice].owner;
                let of = self.playing.iter().find(|p| Some(p.owner) == owner);
                let of = of.map(|p| (p.bank, p.body)).or_else(|| {
                    let sound = self.sounds.iter().find(|(known, ..)| Some(*known) == owner);
                    sound.map(|&(_, bank, body)| (bank, body))
                });
                let sample = of.and_then(|(bank, body)| bank.sample(body, number));
                self.asked[voice].0 = sample.unwrap_or(&[]);
            }
            if let Some(envelope) = words.envelope {
                self.asked[voice].1 = envelope;
            }
            if let Some(pitch) = words.pitch {
                self.chip[voice].set_pitch(pitch);
            }
            if let Some(volumes) = words.volumes {
                self.chip[voice].set_volumes(volumes);
            }
        }
        for voice in 0..VOICES {
            if given.off & 1 << voice != 0 {
                self.chip[voice].key_off();
            }
            if given.on & 1 << voice != 0 {
                let (sample, envelope) = self.asked[voice];
                self.chip[voice].key_on(sample, envelope);
            }
        }
    }

    /// One value of sound: the sum of the voices on each side, with what the reverb
    /// makes of the voices that go into it, kept inside 16 bits, times the main volume.
    pub fn tick(&mut self) -> [i16; 2] {
        let (mut sum, mut wet) = ([0i32; 2], [0i32; 2]);
        for (number, voice) in self.chip.iter_mut().enumerate() {
            let sides = voice.tick();
            sum = [sum[0] + sides[0], sum[1] + sides[1]];
            if self.wet & 1 << number != 0 {
                wet = [wet[0] + sides[0], wet[1] + sides[1]];
            }
        }
        if let Some(reverb) = &mut self.reverb {
            let back = reverb.tick(wet);
            sum = [sum[0] + back[0], sum[1] + back[1]];
        }
        let (low, high) = (i32::from(i16::MIN), i32::from(i16::MAX));
        if sum.iter().any(|side| !(low..=high).contains(side)) {
            self.limited += 1;
        }
        let side = |side: usize| {
            let kept = sum[side].clamp(low, high);
            ((kept * i32::from(self.main[side])) >> 14) as i16
        };
        [side(0), side(1)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sequencer::tests::{bank, set};
    use crate::sound::{BLOCK_ENDS, BLOCK_GOES_BACK, BLOCK_SIZE, BLOCK_STARTS_LOOP};

    /// The body of a bank of one sample: two blocks that go round, every value
    /// `nibble << 12`.
    fn body(nibble: u8) -> Vec<u8> {
        let block = |flags: u8| {
            let mut block = [nibble << 4 | nibble; BLOCK_SIZE];
            (block[0], block[1]) = (0, flags);
            block
        };
        let round = BLOCK_ENDS | BLOCK_GOES_BACK;
        [block(BLOCK_STARTS_LOOP), block(round)].concat()
    }

    /// The program, a key at once, let go 48 ticks (three steps) on, and the end.
    const ONE_KEY: [u8; 14] = [
        0x00, 0xC0, 0x05, 0x00, 0x90, 0x3C, 0x7F, 0x30, 0x3C, 0x00, 0x00, 0xFF, 0x2F, 0x00,
    ];

    #[test]
    fn a_key_sounds_a_step_after_its_tick_and_is_silent_two_values_after_it_is_let_go() {
        let bank = bank(&[(5, &[(0, 0, 0, 127)])]);
        let (body, bytes) = (body(4), set(&ONE_KEY));
        let found = sound::sequences(&bytes).expect("set");
        let mut player = Player::new();
        assert!(player.start(7, &found[0], &bank, &body, [127, 127], 1) == Ok(true));

        // The first step presses the key; the chip hears of it in the second.
        let mut out = Vec::new();
        player.step(&mut out);
        assert_eq!(out.len(), VALUES_A_STEP);
        assert!(out.iter().all(|&value| value == [0; 2]) && player.sounding() == 0);
        out.clear();
        player.step(&mut out);
        // 16384 by the level, by the voice's volume and by the main volume, each just
        // short of the whole.
        assert_eq!(out[..3], [[0; 2], [7_166; 2], [14_334; 2]]);
        assert_eq!(out[VALUES_A_STEP - 1], [16_377; 2]);
        assert!(player.sounding() == 1 && player.playing(7));

        // The third step lets the key go and ends the sequence; the fourth tells the
        // chip, and the fastest release takes two values.
        player.step(&mut out);
        assert!(!player.playing(7) && player.sounding() == 1);
        out.clear();
        player.step(&mut out);
        assert_eq!(out[..3], [[16_377; 2], [8_185; 2], [0; 2]]);
        assert_eq!((player.sounding(), player.limited()), (0, 0));
        assert!(!player.playing(8));
    }

    #[test]
    fn a_tone_that_asks_for_reverb_is_heard_on_after_its_voice_is_silent() {
        // Tone mode 4 is "with reverb"; the kind hands on half of what goes in.
        let (dry, wet) = (
            bank(&[(5, &[(0, 0, 0, 127)])]),
            bank(&[(5, &[(0, 4, 0, 127)])]),
        );
        let (body, bytes) = (body(4), set(&ONE_KEY));
        let found = sound::sequences(&bytes).expect("set");
        let mut words = [0; crate::reverb::WORDS];
        (words[2], words[30], words[31]) = (0x7FFF, 0x7FFF, 0x7FFF);
        (words[10], words[11], words[12], words[13]) = (6, 7, 4, 5);
        words[3] = 0x4000;
        let kind = crate::reverb::Kind {
            start: 0xFFF0,
            mask: 0,
            words,
        };
        let played = |bank, with: bool| {
            let mut player = Player::new();
            if with {
                let mut reverb = Reverb::new(&kind);
                reverb.set_depth([crate::reverb::depth(127); 2]);
                player.set_reverb(reverb);
            }
            assert!(player.start(7, &found[0], bank, &body, [80, 80], 1) == Ok(true));
            let mut out = Vec::new();
            for _ in 0..5 {
                player.step(&mut out);
            }
            (out, player.sounding())
        };
        // A tone that does not ask is not changed by a reverb.
        let (plain, sounding) = played(&dry, false);
        assert_eq!((played(&dry, true).0 == plain, sounding), (true, 0));
        assert_eq!(played(&wet, false).0, plain);
        // One that asks is heard half as loud again while it sounds, and its voice
        // is silent from the third value of the fourth step on, the reverb is not. What
        // the reverb's shifts leave behind is no more than 1.
        let (heard, sounding) = played(&wet, true);
        let (held, after) = (3 * VALUES_A_STEP - 1, 3 * VALUES_A_STEP + 10);
        assert_eq!(
            (plain[held], plain[after], sounding),
            ([6_497; 2], [0; 2], 0)
        );
        assert!((9_735..=9_745).contains(&heard[held][0]) && heard[held][0] == heard[held][1]);
        assert!(heard[after][0] > 3_000);
        assert!(
            heard[5 * VALUES_A_STEP - 1]
                .iter()
                .all(|side| side.abs() <= 1)
        );
    }

    #[test]
    fn a_sound_without_a_sequence_is_heard_from_the_next_step_on() {
        let bank = bank(&[(5, &[(0, 0, 0, 127)])]);
        let body = body(4);
        let mut player = Player::new();
        player.sound(900, (&bank, &body), 5, 60, [127, 127]);
        // A program the bank has not: no voice.
        player.sound(900, (&bank, &body), 6, 60, [127, 127]);
        let mut out = Vec::new();
        player.step(&mut out);
        // Everything at 127 is the whole volume, as for a sequence at its loudest.
        assert_eq!(out[..3], [[0; 2], [7_166; 2], [14_334; 2]]);
        assert_eq!(player.sounding(), 1);
        // At the volume of the sounds of a new game it is about half of that: the
        // volume is not squared.
        let mut quiet = Player::new();
        quiet.sound(900, (&bank, &body), 5, 60, [64, 64]);
        let mut out = Vec::new();
        quiet.step(&mut out);
        assert_eq!(out[VALUES_A_STEP - 1], [8_252; 2]);
    }

    #[test]
    fn a_new_volume_reaches_the_voices_that_sound_and_a_stop_lets_them_go() {
        let bank = bank(&[(5, &[(0, 0, 0, 127)])]);
        // The key of the other tests, held for 127 ticks: let go in the eighth step.
        let mut held = ONE_KEY;
        held[7] = 0x7F;
        let (body, bytes) = (body(4), set(&held));
        let found = sound::sequences(&bytes).expect("set");
        let mut player = Player::new();
        assert!(player.start(7, &found[0], &bank, &body, [127, 127], 1) == Ok(true));
        let mut out = Vec::new();
        player.step(&mut out);
        player.step(&mut out);
        // The music's volume of a new game on the left and nothing on the right: 80 of
        // 127 squared, a step later. More than 127 counts as 127 while it plays.
        let setting = crate::window::SETTING_AT_START;
        assert_eq!(
            (sound::music_volume(setting), sound::sound_volume(setting)),
            (80, 64)
        );
        assert!(!sound::one_loudspeaker(setting));
        player.set_sides(7, [sound::music_volume(setting), 0]);
        player.set_sides(8, [0, 0]);
        out.clear();
        player.step(&mut out);
        assert_eq!(out[VALUES_A_STEP - 1], [6_497, 0]);
        player.set_sides(7, [300, 127]);
        // Stopped while the key is held: the chip hears the new volumes and the key
        // let go in the next step, and the sequence is over.
        player.stop(7);
        assert!(!player.playing(7));
        out.clear();
        player.step(&mut out);
        assert_eq!(out[..3], [[16_377; 2], [8_185; 2], [0; 2]]);
        assert_eq!(player.sounding(), 0);
        out.clear();
        for _ in 0..8 {
            player.step(&mut out);
        }
        assert!(out.iter().all(|&value| value == [0; 2]));
    }

    #[test]
    fn one_loudspeaker_reaches_a_voice_when_its_volumes_are_worked_out_again() {
        let bank = bank(&[(5, &[(0, 0, 0, 127)])]);
        let mut held = ONE_KEY;
        held[7] = 0x7F;
        let (body, bytes) = (body(4), set(&held));
        let found = sound::sequences(&bytes).expect("set");
        let mut player = Player::new();
        // Nothing on the right: the key sounds on the left alone.
        assert!(player.start(7, &found[0], &bank, &body, [80, 0], 1) == Ok(true));
        let mut out = Vec::new();
        for _ in 0..3 {
            out.clear();
            player.step(&mut out);
        }
        assert_eq!(out[VALUES_A_STEP - 1], [6_497, 0]);
        // Told one loudspeaker, the voice that sounds keeps its volumes ...
        player.set_mono(true);
        assert!(player.mono());
        out.clear();
        player.step(&mut out);
        assert_eq!(out[VALUES_A_STEP - 1], [6_497, 0]);
        // ... until its sequence is given its volume anew, as the page "Audio" does in
        // every pass: both sides are the louder one, a step later.
        player.set_sides(7, [80, 0]);
        out.clear();
        player.step(&mut out);
        assert_eq!(out[VALUES_A_STEP - 1], [6_497; 2]);
        // A sound without a sequence that is pressed from then on has it too.
        let mut direct = Player::new();
        direct.set_mono(true);
        direct.sound(900, (&bank, &body), 5, 60, [64, 0]);
        let mut out = Vec::new();
        direct.step(&mut out);
        assert_eq!(out[VALUES_A_STEP - 1], [8_252; 2]);
        // And a sequence that is started from then on.
        assert!(direct.start(7, &found[0], &bank, &body, [80, 0], 1) == Ok(true));
        assert!(direct.mono());
    }

    #[test]
    fn a_fade_takes_its_share_off_the_volume_each_step_and_leaves_the_sequence_playing() {
        let bank = bank(&[(5, &[(0, 0, 0, 127)])]);
        // The key held for 127 ticks, a volume of 80: the game's fade of 127 over 120
        // steps takes 1, 2, 3, ... off it, a step later on the chip.
        let mut held = ONE_KEY;
        held[7] = 0x7F;
        let (body, bytes) = (body(4), set(&held));
        let found = sound::sequences(&bytes).expect("set");
        let mut player = Player::new();
        assert!(player.start(7, &found[0], &bank, &body, [80, 80], 1) == Ok(true));
        let mut out = Vec::new();
        player.step(&mut out);
        player.step(&mut out);
        // 80 of 127 squared, of 16378.
        assert_eq!(out[2 * VALUES_A_STEP - 1], [6_497; 2]);
        player.fade(7, 127, 120);
        player.fade(8, 127, 120);
        let mut last = Vec::new();
        for _ in 0..4 {
            out.clear();
            player.step(&mut out);
            last.push(out[VALUES_A_STEP - 1][0]);
        }
        // Still 80 in the step that asks; then 79, 78 and 77 of 127, squared.
        assert_eq!(last, [6_497, 6_336, 6_176, 6_019]);
        assert!(player.playing(7));
        // No loudness and no time are no fade.
        player.fade(7, 0, 120);
        player.fade(7, 127, 0);
        out.clear();
        player.step(&mut out);
        assert!(out[VALUES_A_STEP - 1][0] < 6_019 && out[VALUES_A_STEP - 1][0] > 5_000);
    }

    #[test]
    fn the_voices_of_several_sequences_are_summed_from_their_own_banks_inside_16_bits() {
        let bank = bank(&[(5, &[(0, 0, 0, 127)])]);
        let (loud, quiet, bytes) = (body(4), body(2), set(&ONE_KEY));
        let found = sound::sequences(&bytes).expect("set");
        let held = |bodies: &[&[u8]]| {
            let mut player = Player::new();
            for (owner, body) in bodies.iter().enumerate() {
                let started = player.start(owner as u16, &found[0], &bank, body, [127; 2], 1);
                assert_eq!(started, Ok(true));
            }
            let mut out = Vec::new();
            player.step(&mut out);
            player.step(&mut out);
            assert_eq!(player.sounding(), bodies.len());
            (out[2 * VALUES_A_STEP - 1], player.limited())
        };
        // One voice of 16378 and one of 8188, by the main volume.
        assert_eq!(held(&[&loud, &quiet]), ([24_564; 2], 0));
        // Three of 16378 are more than 16 bits hold: the sum stops at the limit.
        let (value, limited) = held(&[&loud, &loud, &loud]);
        assert_eq!(value, [32_765; 2]);
        assert!(limited > 700);
        // A sequence started again under its number takes the place of the first.
        let mut player = Player::new();
        for _ in 0..2 {
            let started = player.start(3, &found[0], &bank, &loud, [127; 2], 1);
            assert_eq!(started, Ok(true));
        }
        let mut out = Vec::new();
        player.step(&mut out);
        player.step(&mut out);
        assert_eq!(player.sounding(), 1);
    }
}
