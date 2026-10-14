
use crate::scene::ScenePackage;

/// The type of the object, the extra bytes its task gets, and the bytes of its record.
pub const KIND: u32 = 0x0D;
const EXTRA: u32 = 0x24;
const PART: usize = 24;
const SIZE: usize = 2 * PART;
/// Words of the start routine: the clearing of the phase and the -1 of the kept voice.
const START: [(u32, u32); 2] = [(0x18, 0xAE00_0014), (0x3C, 0xAC43_0020)];
/// What the object leaves in the told state when it has taken one.
const NOTHING: i32 = -1;
/// The frames the first model runs to and stands on in the phases 2 and 3.
pub const FIRST_ENDS: [i32; 2] = [236, 379];
/// The frames the second model goes round from phase 1 on.
pub const SECOND_ROUND: i32 = 15;
/// The sound of the bank at the start and the end of a stretch, and the one that is
/// kept on a voice between them.
pub const SOUND: u32 = 49;
pub const KEPT: u32 = 50;
/// The frames of the first model at which a stretch of sound starts and ends: the
/// start of phase 2 and its end, the start of phase 3 and its end.
const STARTS: [i32; 2] = [1, 237];
const ENDS: [i32; 2] = [210, 345];
/// The sound the second model asks for while it stands on this frame.
pub const SECOND_SOUND: u32 = 23;
const SECOND_SOUNDS_AT: i32 = 7;

/// One of the two models: its number in the room's package, and where it stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Part {
    pub model: u32,
    pub place: [i32; 3],
    pub angles: [i16; 3],
}

impl Part {
    fn parse(bytes: &[u8; PART]) -> Self {
        let word = |at: usize| i32::from_le_bytes(bytes[at..at + 4].try_into().expect("4 bytes"));
        let half = |at: usize| i16::from_le_bytes([bytes[at], bytes[at + 1]]);
        Self {
            model: word(0) as u32,
            place: [word(4), word(8), word(12)],
            angles: [half(16), half(18), half(20)],
        }
    }
}

/// The record of the object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Played {
    pub parts: [Part; 2],
}

impl Played {
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let bytes: &[u8; SIZE] = bytes.get(..SIZE)?.try_into().ok()?;
        let (first, second) = bytes.split_at(PART);
        Some(Self {
            parts: [
                Part::parse(first.try_into().ok()?),
                Part::parse(second.try_into().ok()?),
            ],
        })
    }

    /// The object in the lists of a package, with the list it is in and the address of
    /// its record; none for a package without. It is known by its type, its extra bytes
    /// and two stores of its start routine.
    pub fn of(package: &ScenePackage) -> Option<(usize, u32, Self)> {
        let word = |address: u32| {
            let bytes = package.bytes_at(address, 4)?;
            Some(u32::from_le_bytes(bytes.try_into().expect("4 bytes")))
        };
        for (list, objects) in package.object_lists().iter().enumerate() {
            for object in (objects.objects.iter())
                .filter(|object| object.kind == KIND && object.extra == EXTRA)
            {
                let starts =
                    (START.iter()).all(|&(at, store)| word(object.function + at) == Some(store));
                let found = (starts.then_some(()))
                    .and_then(|()| package.bytes_at(object.data, SIZE))
                    .and_then(Self::parse);
                if let Some(played) = found {
                    return Some((list, object.data, played));
                }
            }
        }
        None
    }
}

/// What a game frame of the object asks of the sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asked {
    /// A sound of the bank (`0x800314F0`).
    Sound(u32),
    /// A sound of the bank kept on a voice (`0x80031550`).
    Keep(u32),
    /// The kept sound is let go (`0x800315D0`).
    LetGo(u32),
}

/// The words of the object's task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Playing {
    /// The state a script has told and the object has not taken yet.
    pub told: i32,
    /// How many states were taken.
    pub phase: i32,
    /// The frame of the first model, where it goes back to and the frame it does not
    /// reach.
    pub first: i32,
    first_round: (i32, i32),
    /// The same of the second model.
    pub second: i32,
    second_round: (i32, i32),
    /// Whether the kept sound is on its voice.
    pub sounds: bool,
}

impl Default for Playing {
    fn default() -> Self {
        Self::new()
    }
}

impl Playing {
    /// The task as the start routine leaves it, in front of its first game frame.
    pub fn new() -> Self {
        Self {
            told: NOTHING,
            phase: 0,
            first: 0,
            first_round: (0, 0),
            second: 0,
            second_round: (0, 0),
            sounds: false,
        }
    }

    /// A script tells the object a state (`0x2E`).
    pub fn tell(&mut self, state: i32) {
        self.told = state;
    }

    /// One game frame of the object (`0x800B2D14` of file 404): a told state takes it a
    /// phase on, the phase sets the rounds of the two models, the sounds of the frames
    /// are asked for, the two frames to draw are answered, and then both frames step,
    /// the second one only in a pass whose number is odd.
    pub fn pass(&mut self, odd: bool) -> ([i32; 2], Vec<Asked>) {
        if self.told != NOTHING {
            self.told = NOTHING;
            self.phase += 1;
        }
        match self.phase {
            0 => (self.first_round, self.second_round) = ((0, 0), (0, 0)),
            1 => (self.first_round, self.second_round) = ((0, 0), (0, SECOND_ROUND)),
            phase @ (2 | 3) => {
                let end = FIRST_ENDS[phase as usize - 2];
                (self.first_round, self.second_round) = ((end, end), (0, SECOND_ROUND));
            }
            _ => {}
        }
        let mut asked = Vec::new();
        let start = [Asked::Sound(SOUND), Asked::Keep(KEPT)];
        let end = [Asked::Sound(SOUND), Asked::LetGo(KEPT)];
        if self.first == STARTS[0] && !self.sounds {
            self.sounds = true;
            asked.extend(start);
        }
        if self.first == ENDS[0] && self.phase == 2 && self.sounds {
            self.sounds = false;
            asked.extend(end);
        }
        if self.first == STARTS[1] && !self.sounds && self.phase == 3 {
            self.sounds = true;
            asked.extend(start);
        }
        if self.first == ENDS[1] && self.sounds {
            self.sounds = false;
            asked.extend(end);
        }
        if self.second == SECOND_SOUNDS_AT {
            asked.push(Asked::Sound(SECOND_SOUND));
        }
        let drawn = [self.first, self.second];
        self.first += 1;
        if self.first >= self.first_round.1 {
            self.first = self.first_round.0;
        }
        self.second += i32::from(odd);
        if self.second >= self.second_round.1 {
            self.second = self.second_round.0;
        }
        (drawn, asked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_reads_as_two_models_with_their_places() {
        let mut bytes = Vec::new();
        for (model, place, angles) in [
            (130i32, [1, 2, 3], [4i16, 5, 6]),
            (131, [-1, -2, -3], [0, 0, 0]),
        ] {
            bytes.extend(model.to_le_bytes());
            for word in place {
                bytes.extend(i32::to_le_bytes(word));
            }
            for half in angles {
                bytes.extend(half.to_le_bytes());
            }
            bytes.extend([0, 0]);
        }
        let played = Played::parse(&bytes).expect("a record");
        assert_eq!(played.parts[0].model, 130);
        assert_eq!(played.parts[0].angles, [4, 5, 6]);
        assert_eq!(played.parts[1].place, [-1, -2, -3]);
        assert_eq!(Played::parse(&bytes[..SIZE - 1]), None);
    }

    #[test]
    fn nothing_told_both_models_stand_on_their_first_frame() {
        let mut task = Playing::new();
        for pass in 0..40 {
            assert_eq!(task.pass(pass % 2 == 1), ([0, 0], vec![]));
        }
    }

    #[test]
    fn the_first_state_sets_the_second_model_going_round() {
        let mut task = Playing::new();
        task.tell(0);
        let mut frames = Vec::new();
        let mut sounds = 0;
        for pass in 0..64 {
            let ([first, second], asked) = task.pass(pass % 2 == 1);
            assert_eq!(first, 0);
            frames.push(second);
            sounds += asked.len();
            assert!(
                asked
                    .iter()
                    .all(|&asked| asked == Asked::Sound(SECOND_SOUND))
            );
        }
        // A step in every second pass, fifteen frames round.
        assert_eq!(frames[..6], [0, 0, 1, 1, 2, 2]);
        assert_eq!(frames[28..32], [14, 14, 0, 0]);
        // Frame 7 is drawn in two passes of each round.
        assert_eq!(sounds, 4);
    }

    #[test]
    fn the_second_and_third_state_run_the_first_model_on_with_its_sounds() {
        let mut task = Playing::new();
        task.tell(0);
        task.pass(false);
        task.tell(0);
        let mut heard = Vec::new();
        for pass in 0..300 {
            let ([first, _], asked) = task.pass(false);
            assert_eq!(first, pass.min(FIRST_ENDS[0]));
            heard.extend(asked.into_iter().map(|asked| (first, asked)));
        }
        assert_eq!(
            heard,
            [
                (1, Asked::Sound(SOUND)),
                (1, Asked::Keep(KEPT)),
                (210, Asked::Sound(SOUND)),
                (210, Asked::LetGo(KEPT)),
            ]
        );
        // Any state takes it on: the frames to 379, a stretch of sound from 237 to 345.
        task.tell(7);
        heard.clear();
        for pass in 0..200 {
            let ([first, _], asked) = task.pass(false);
            assert_eq!(first, (FIRST_ENDS[0] + pass).min(FIRST_ENDS[1]));
            heard.extend(asked.into_iter().map(|asked| (first, asked)));
        }
        assert_eq!(
            heard,
            [
                (237, Asked::Sound(SOUND)),
                (237, Asked::Keep(KEPT)),
                (345, Asked::Sound(SOUND)),
                (345, Asked::LetGo(KEPT)),
            ]
        );
        // A fourth state changes nothing more.
        task.tell(0);
        assert_eq!(task.pass(false).0[0], FIRST_ENDS[1]);
        assert_eq!(task.phase, 4);
    }
}
