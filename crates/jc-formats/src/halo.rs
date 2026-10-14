
use crate::scene::ScenePackage;

/// The archive files of the packages, the address of the task's descriptor of five words
/// and that of the show list in each.
const PACKAGES: [(usize, u32, u32); 2] = [
    (304, 0x800B_24BC, 0x800B_561C),
    (307, 0x800B_24BC, 0x800B_5614),
];
/// The descriptor's words: no number, type `0x0D`, 0, `0x48` extra bytes; the fifth is
/// the start routine.
const DESCRIPTOR: [u32; 4] = [0xFFFF_FFFF, 0x0D, 0, 0x48];
/// The sprites of the room's package the routine asks for: the spark, the ball (whose
/// frames the sparks are put on) and the pulse (whose frame count it waits).
pub const SPARK: u32 = 323;
pub const BALL: u32 = 324;
pub const PULSE: u32 = 325;
/// The frames of the pulse, which the routine waits before anything else.
pub const WAIT: i32 = 40;
/// The last frame of the ball's track.
const LAST: i32 = 239;
/// Spark slots, and the age at which a spark is gone.
pub const SLOTS: usize = 60;
const GONE: u8 = 60;
/// The music number of the chime, asked for every 60 frames of the track.
pub const CHIME: u32 = 4;
const CHIME_EVERY: i32 = 60;
/// The blend word the routine hands the quad drawer: blended, in mode 1 (added).
pub const BLEND_MODE: u8 = 1;

/// The light of a package: its show list and the start routine of its task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Halo {
    pub list: u32,
    pub start: u32,
}

impl Halo {
    /// The light of the package in archive file `file`; none for any other package, or
    /// when its descriptor is not where the findings put it.
    pub fn of(file: usize, package: &ScenePackage) -> Option<Self> {
        let &(_, descriptor, list) = PACKAGES.iter().find(|(at, ..)| *at == file)?;
        let word = |address: u32| {
            let bytes = package.bytes_at(address, 4)?;
            Some(u32::from_le_bytes(bytes.try_into().ok()?))
        };
        let words: Option<Vec<u32>> = (0..5).map(|k| word(descriptor + 4 * k)).collect();
        let words = words?;
        (words[..4] == DESCRIPTOR).then_some(Self {
            list,
            start: words[4],
        })
    }
}

/// One spark that is drawn in a game frame: the frame of the spark sprite (its age) and
/// the frame of the ball whose first quad's place it is put on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spark {
    pub frame: usize,
    pub on: usize,
}

/// What a call of the routine did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Step {
    /// The sparks drawn, in the routine's order.
    pub sparks: Vec<Spark>,
    pub chime: bool,
    /// The task told itself -2: the wrapper ends it.
    pub ends: bool,
}

/// The words of the task (+20 to +91): the frames waited, the frame of the track, the
/// slots in use and the age of each slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Haloing {
    pub waited: i32,
    pub track: i32,
    pub slots: i32,
    pub ages: [u8; SLOTS],
}

impl Default for Haloing {
    fn default() -> Self {
        Self::new()
    }
}

impl Haloing {
    /// The task as its start routine leaves it, before the first call. The ages are not
    /// cleared in the game, but none is read before it is written.
    pub fn new() -> Self {
        Self {
            waited: 0,
            track: 0,
            slots: 0,
            ages: [GONE; SLOTS],
        }
    }

    /// One call of the routine (`0x800B2DC4`); the start routine makes the first in the
    /// game frame the task is made. `draw` is the library's random number.
    pub fn call(&mut self, mut draw: impl FnMut() -> u32) -> Step {
        let mut step = Step::default();
        if self.waited < WAIT {
            self.waited += 1;
            return step;
        }
        step.chime = self.track % CHIME_EVERY == 0;
        for slot in (0..self.slots.clamp(0, SLOTS as i32) as usize).rev() {
            let age = self.ages[slot];
            if age < GONE {
                step.sparks.push(Spark {
                    frame: usize::from(age),
                    on: (self.track - i32::from(age)).max(0) as usize,
                });
                self.ages[slot] = age + 1;
            }
        }
        self.track = (self.track + 1).clamp(0, LAST);
        self.slots = match self.track < LAST {
            true => self.slots + 1,
            false => self.slots - 1,
        }
        .clamp(0, SLOTS as i32);
        let slot = (self.track - 1).rem_euclid(SLOTS as i32) as usize;
        self.ages[slot] = match draw() & 7 {
            0 => 0,
            _ => GONE,
        };
        step.ends = self.slots <= 0;
        step
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_task_waits_sparks_and_ends_in_call_338() {
        let mut task = Haloing::new();
        // Every draw a birth: each slot is filled once it is reached.
        let mut steps = Vec::new();
        for _ in 0..400 {
            let step = task.call(|| 8);
            let ends = step.ends;
            steps.push(step);
            if ends {
                break;
            }
        }
        assert_eq!(steps.len(), 338);
        assert!(steps[..40].iter().all(|step| *step == Step::default()));
        let chimes: Vec<usize> = (steps.iter().enumerate())
            .filter(|(_, step)| step.chime)
            .map(|(call, _)| call + 1)
            .collect();
        assert_eq!(chimes, [41, 101, 161, 221]);
        // Call 42 draws the spark born in call 41, young, on frame 1 of the track.
        assert_eq!(steps[41].sparks, [Spark { frame: 0, on: 1 }]);
        // A spark stays on the frame it was born behind and lives 60 calls.
        assert_eq!(steps[50].sparks.last(), Some(&Spark { frame: 9, on: 1 }));
        assert!(steps[101].sparks.iter().all(|spark| spark.frame < 60));
        // From call 280 the track stands at 239 and the sparks slide back along it.
        assert!(
            steps[285]
                .sparks
                .iter()
                .all(|spark| spark.on + spark.frame == 239)
        );
    }

    #[test]
    fn no_draw_no_spark() {
        let mut task = Haloing::new();
        let drawn: usize = (0..338).map(|_| task.call(|| 1).sparks.len()).sum();
        assert_eq!(drawn, 0);
        assert_eq!(task.slots, 0);
    }
}
