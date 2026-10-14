
use crate::background::ViewHeader;
use crate::scene::ScenePackage;

/// The type of the object, the extra bytes its task gets, and the bytes of its record.
pub const KIND: u32 = 0x0D;
const EXTRA: u32 = 4;
const SIZE: usize = 32;
/// The story variable a way back sets, which the start routine asks and clears, and the
/// story flag of a lost fight, which picks the script of a player who comes back.
pub const BACK_VARIABLE: u32 = 4;
pub const LOST_FLAG: u32 = 0;
/// The two story variables the object writes the room behind the ride into, the upper
/// and the lower byte of its number.
pub const ROOM_VARIABLES: [u32; 2] = [0, 1];
/// The state the object's script tells it: send the game to the ride.
pub const LEAVE: i32 = 1;

/// Places in the start routine: the number of the story variable it asks for, the call
/// that reads it, the two halves of the script of a player who lost, of the script of
/// one who did not, and the call of the step.
const ASKS: (u32, u32) = (0x0C, 0x2404_0004);
const READS: (u32, u32) = (0x18, 0x0C02_B4C9);
const LOST: (u32, u32) = (0x34, 0x3C);
const BACK: (u32, u32) = (0x40, 0x44);
const STEP: u32 = 0x74;
/// The two halves of the script the step starts for a player in the region.
const LEAVES: (u32, u32) = (0x74, 0x78);

/// The record of the object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ride {
    /// The room the object sends the game to, and the room that one sends it on to.
    pub room: i32,
    pub goes_to: i32,
    /// The story flag the object sets when the game leaves.
    pub flag: i32,
    /// The trigger region the player stands in.
    pub region: i32,
    /// Where the entry routine puts a player who comes back, and the angle there.
    pub place: [i32; 3],
    pub angle: i16,
}

impl Ride {
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let bytes: &[u8; SIZE] = bytes.get(..SIZE)?.try_into().ok()?;
        let word = |at: usize| i32::from_le_bytes(bytes[at..at + 4].try_into().expect("4 bytes"));
        Some(Self {
            room: word(0),
            goes_to: word(4),
            flag: word(8),
            region: word(12),
            place: [word(16), word(20), word(24)],
            angle: i16::from_le_bytes([bytes[28], bytes[29]]),
        })
    }

    /// What the object writes into the two story variables: the upper and the lower
    /// byte of the room behind the ride (the routine divides by 256 towards zero).
    pub fn variables(&self) -> [i32; 2] {
        let upper = self.goes_to / 256;
        [upper, self.goes_to - upper * 256]
    }
}

/// The room a ride's native routine sends the game to, from the two story variables.
pub fn room_of(variables: [i32; 2]) -> i32 {
    (variables[0] << 8) + variables[1]
}

/// The object of a package with the address of its record, and the three scripts its
/// routines start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rides {
    pub at: u32,
    pub ride: Ride,
    /// The script for a player in the region.
    pub leaves: u32,
    /// The scripts for a player who comes back: after a lost fight, and otherwise.
    pub lost: u32,
    pub back: u32,
}

impl Rides {
    /// The object in the object lists of a package; none for a package without. An
    /// object is told apart by its start routine, which asks for story variable 4 and
    /// names its scripts where this one does.
    pub fn of(package: &ScenePackage) -> Option<Self> {
        let word = |address: u32| {
            let bytes = package.bytes_at(address, 4)?;
            Some(u32::from_le_bytes(bytes.try_into().expect("4 bytes")))
        };
        // An address from an upper half for the register `upper` and the `addiu` that
        // adds the lower half into `a0`.
        let address = |at: (u32, u32), upper: u32, lower: u32| {
            let (high, low) = (word(at.0)?, word(at.1)?);
            (high >> 16 == upper && low >> 16 == lower)
                .then(|| (high << 16).wrapping_add(low as u16 as i16 as u32))
                .filter(|script| package.bytes_at(*script, 16).is_some())
        };
        let lists = package.object_lists();
        (lists.iter().flat_map(|list| &list.objects))
            .filter(|object| object.kind == KIND && object.extra == EXTRA)
            .find_map(|object| {
                let start = object.function;
                if word(start.checked_add(ASKS.0)?)? != ASKS.1 || word(start + READS.0)? != READS.1
                {
                    return None;
                }
                let call = word(start + STEP)?;
                if call >> 26 != 3 {
                    return None;
                }
                let step = 0x8000_0000 | (call & 0x03FF_FFFF) << 2;
                Some(Self {
                    at: object.data,
                    ride: package.bytes_at(object.data, SIZE).and_then(Ride::parse)?,
                    leaves: address((step + LEAVES.0, step + LEAVES.1), 0x3C04, 0x2484)?,
                    lost: address((start + LOST.0, start + LOST.1), 0x3C02, 0x2444)?,
                    back: address((start + BACK.0, start + BACK.1), 0x3C02, 0x2444)?,
                })
            })
    }

    /// The script the start routine starts: none while story variable 4 is 0.
    pub fn arrival(&self, back: i32, lost: bool) -> Option<u32> {
        (back != 0).then_some(if lost { self.lost } else { self.back })
    }
}

/// The native routine at the end of the ride's script: it reads the story variables 0
/// and 1, looks the room of that number up and asks for it as an exit does.
const LEAVES_BY_VARIABLES: [u32; 30] = [
    0x27BD_FFE0,
    0x0000_2021,
    0xAFBF_001C,
    0x0C02_B4C9,
    0xAFB0_0018,
    0x2404_0001,
    0x0C02_B4C9,
    0x0002_8200,
    0x27A4_0010,
    0x0C02_B7AF,
    0x0202_2821,
    0x2402_0001,
    0x3C04_8009,
    0x2484_B500,
    0x3C05_8009,
    0x0040_1821,
    0xA480_02F0,
    0xACA3_1C24,
    0x97A5_0010,
    0x97A6_0012,
    0x97A7_0014,
    0xA485_02E4,
    0xA486_02E8,
    0xA487_02EC,
    0x8FBF_001C,
    0x8FB0_0018,
    0x3C03_8009,
    0xAC60_1BE4,
    0x03E0_0008,
    0x27BD_0020,
];

/// Whether a native routine of the package is the one that sends the game to the room
/// of the story variables 0 and 1 ([`room_of`]).
pub fn leaves_by_variables(package: &ScenePackage, routine: u32) -> bool {
    (package.bytes_at(routine, 4 * LEAVES_BY_VARIABLES.len())).is_some_and(|code| {
        (code.as_chunks::<4>().0.iter())
            .map(|word| u32::from_le_bytes(*word))
            .eq(LEAVES_BY_VARIABLES)
    })
}

/// What the object's step did in a game frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rode {
    Nothing,
    /// The script for a player in the region starts, with the object as its owner.
    Starts,
    /// The game goes to the ride's room, with the variables written and the flag set.
    Leaves,
}

/// The words of the object's task that its step works on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Riding {
    /// The state told (+124 of the task), -1 for none, and the state reached (+128).
    pub told: i32,
    pub reached: i32,
}

impl Default for Riding {
    fn default() -> Self {
        Self::new()
    }
}

impl Riding {
    pub fn new() -> Self {
        Self {
            told: -1,
            reached: -1,
        }
    }

    /// The script the object started tells it a state (`0x30`).
    pub fn tell(&mut self, state: i32) {
        self.told = state;
    }

    /// One game frame of the step: a player in the region, while no script of the
    /// object runs, tells it state 0. Both states are noted as reached.
    pub fn pass(&mut self, in_region: bool, script_runs: bool) -> Rode {
        if in_region && !script_runs {
            self.told = 0;
        }
        match self.told {
            0 => {
                (self.reached, self.told) = (0, -1);
                Rode::Starts
            }
            LEAVE => {
                (self.reached, self.told) = (LEAVE, -1);
                Rode::Leaves
            }
            _ => Rode::Nothing,
        }
    }
}

/// The object of the ride's room that plays a sound again and again: type `0x0D` with 8
/// extra bytes, a wait in front of the first sound, then the sound every so many game
/// frames. The ride's script tells it -2 at its end, and the wrapper of the object
/// tasks leaves a task that was told so alone (as `crate::passage` has it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sounds {
    pub at: u32,
    pub wait: i32,
    pub every: i32,
    pub sound: u32,
}

const SOUNDS_EXTRA: u32 = 8;
/// Places in the start routine (the wait, the call of the step) and in the step (the
/// game frames between two sounds, the call of the sound routine, the sound).
const WAIT: u32 = 0x0C;
const SOUNDS_STEP: u32 = 0x18;
const EVERY: u32 = 0x38;
const PLAYS: (u32, u32) = (0x40, 0x0C00_C53C);
const SOUND: u32 = 0x44;

impl Sounds {
    pub fn of(package: &ScenePackage) -> Option<Self> {
        let word = |address: u32| {
            let bytes = package.bytes_at(address, 4)?;
            Some(u32::from_le_bytes(bytes.try_into().expect("4 bytes")))
        };
        // A number that an `addiu` from zero puts into the register of `code`.
        let number = |at: u32, code: u32| {
            let found = word(at)?;
            (found >> 16 == code).then_some(found & 0xFFFF)
        };
        let lists = package.object_lists();
        (lists.iter().flat_map(|list| &list.objects))
            .filter(|object| object.kind == KIND && object.extra == SOUNDS_EXTRA)
            .find_map(|object| {
                let start = object.function;
                let call = word(start.checked_add(SOUNDS_STEP)?)?;
                if call >> 26 != 3 {
                    return None;
                }
                let step = 0x8000_0000 | (call & 0x03FF_FFFF) << 2;
                if word(step + PLAYS.0)? != PLAYS.1 {
                    return None;
                }
                Some(Self {
                    at: object.data,
                    wait: number(start + WAIT, 0x2402)? as i32,
                    every: number(step + EVERY, 0x2402)? as i32,
                    sound: number(step + SOUND, 0x2404)?,
                })
            })
    }
}

/// The task of [`Sounds`]: its two counters, and whether it was told to end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sounding {
    wait: i32,
    gap: i32,
    over: bool,
}

impl Sounding {
    /// The task behind its start routine, which runs the step once.
    pub fn new(sounds: &Sounds) -> Self {
        let mut task = Self {
            wait: sounds.wait,
            gap: 0,
            over: false,
        };
        task.pass(sounds);
        task
    }

    /// A script tells the object a state (`0x2E`): -2 and -3 end its game frames.
    pub fn tell(&mut self, state: i32) {
        self.over |= matches!(state, -3 | -2);
    }

    /// One game frame of the step: the sound it asks for, if any.
    pub fn pass(&mut self, sounds: &Sounds) -> Option<u32> {
        if self.over {
            return None;
        }
        if self.wait != 0 {
            self.wait -= 1;
            return None;
        }
        let asked = (self.gap == 0).then(|| {
            self.gap = sounds.every;
            sounds.sound
        });
        self.gap -= 1;
        asked
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Follows {
    /// The start routine and the step it calls and leaves as the task's routine.
    pub start: u32,
    pub step: u32,
}

/// Places in the start routine (the two calls of the length and the call of the step)
/// and in the step (the store of the player's third coordinate into the point the
/// camera looks at, and the store of the projection distance).
const SQUARES: (u32, u32) = (0x20, 0x0C00_EE04);
const ROOT: (u32, u32) = (0x28, 0x0C00_EE47);
const FOLLOWS_STEP: u32 = 0x4C;
const LOOKS_AT: (u32, u32) = (0x30, 0xAE02_0028);
const DISTANCE: (u32, u32) = (0x50, 0xA602_002E);

impl Follows {
    /// The object in the object lists of a package; none for a package without.
    pub fn of(package: &ScenePackage) -> Option<Self> {
        let word = |address: u32| {
            let bytes = package.bytes_at(address, 4)?;
            Some(u32::from_le_bytes(bytes.try_into().expect("4 bytes")))
        };
        let lists = package.object_lists();
        (lists.iter().flat_map(|list| &list.objects))
            .filter(|object| object.kind == KIND && object.extra == EXTRA)
            .find_map(|object| {
                let start = object.function;
                if word(start.checked_add(SQUARES.0)?)? != SQUARES.1
                    || word(start + ROOT.0)? != ROOT.1
                {
                    return None;
                }
                let call = word(start + FOLLOWS_STEP)?;
                if call >> 26 != 3 {
                    return None;
                }
                let step = 0x8000_0000 | (call & 0x03FF_FFFF) << 2;
                (word(step + LOOKS_AT.0)? == LOOKS_AT.1 && word(step + DISTANCE.0)? == DISTANCE.1)
                    .then_some(Self { start, step })
            })
    }
}

/// The whole part of the root of the sum of the three squared differences of two
/// places (`0x8003B810`, then `0x8003B91C`).
fn length(a: [i32; 3], b: [i32; 3]) -> u32 {
    let squares = (a.iter().zip(b))
        .map(|(a, b)| a.wrapping_sub(b).wrapping_mul(a.wrapping_sub(b)))
        .fold(0i32, i32::wrapping_add) as u32;
    let mut root = (f64::from(squares)).sqrt() as u32;
    while u64::from(root) * u64::from(root) > u64::from(squares) {
        root -= 1;
    }
    while u64::from(root + 1) * u64::from(root + 1) <= u64::from(squares) {
        root += 1;
    }
    root
}

/// The task of [`Follows`]: the one word it keeps, the projection distance of the view
/// as the room was read, times 4096, over the camera's distance from the point it
/// looked at then.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Following {
    kept: u32,
}

impl Following {
    /// The start routine: the word from the live header as it is, then the step once.
    /// None when the camera stands on the point it looks at (the game divides by 0).
    pub fn new(header: &mut ViewHeader, player: [i32; 3]) -> Option<Self> {
        let distance = header.projection_distance as i32 as u32;
        let task = Self {
            kept: (distance << 12).checked_div(length(header.look_at, header.camera))?,
        };
        task.pass(header, player);
        Some(task)
    }

    /// One game frame of the step: the point the camera looks at gets the player's
    /// third coordinate, and the projection distance is the kept word times the new
    /// length, shifted right by 12.
    pub fn pass(&self, header: &mut ViewHeader, player: [i32; 3]) {
        header.look_at[2] = player[2];
        let product = self
            .kept
            .wrapping_mul(length(header.look_at, header.camera));
        header.projection_distance = (product >> 12) as i16;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ride_s_camera_keeps_the_size_of_what_it_looks_at() {
        let mut header = ViewHeader::parse(&[0; crate::background::HEADER_SIZE]).expect("a header");
        (header.camera, header.look_at) = ([-300, -400, -4000], [0, 0, 0]);
        header.projection_distance = 1860;
        // The camera is 4031 from the point: 1860 * 4096 / 4031 is 1889.
        assert_eq!(length(header.look_at, header.camera), 4031);
        let task = Following::new(&mut header, [5, 7, 0]).expect("a distance");
        assert_eq!(task.kept, 1889);
        // The step of the start: the same point, and the distance by the kept word.
        assert_eq!(header.look_at, [0, 0, 0]);
        assert_eq!(header.projection_distance, (1889 * 4031 >> 12) as i16);
        // The player walks to the camera: only the third coordinate is followed, and
        // the distance falls with the length, 500 for the 3 and 4 of the others.
        task.pass(&mut header, [50, 60, -4000]);
        assert_eq!(header.look_at, [0, 0, -4000]);
        assert_eq!(header.projection_distance, (1889 * 500 >> 12) as i16);
        // A camera on its point has no word.
        header.look_at = header.camera;
        assert_eq!(Following::new(&mut header, [0, 0, -4000]), None);
    }

    #[test]
    fn the_length_is_the_whole_part_of_the_root() {
        assert_eq!(length([0; 3], [0; 3]), 0);
        assert_eq!(length([1, 0, 0], [0; 3]), 1);
        assert_eq!(length([3, 4, 0], [0, 0, 0]), 5);
        assert_eq!(length([1, 1, 1], [0, 0, 0]), 1);
        assert_eq!(length([0, 0, 100], [0, 0, -100]), 200);
        assert_eq!(length([7, 0, 0], [0, 4, 4]), 9);
    }

    #[test]
    fn the_sound_comes_behind_the_wait_and_then_at_its_pace() {
        let sounds = Sounds {
            at: 0x1000,
            wait: 3,
            every: 4,
            sound: 29,
        };
        let mut task = Sounding::new(&sounds);
        let heard: Vec<Option<u32>> = (0..8).map(|_| task.pass(&sounds)).collect();
        let s = Some(29);
        assert_eq!(heard, [None, None, s, None, None, None, s, None]);
        // Another state changes nothing; told -2 it is left alone.
        task.tell(1);
        assert_eq!(task.pass(&sounds), None);
        assert_eq!(task.pass(&sounds), None);
        assert_eq!(task.pass(&sounds), s);
        task.tell(-2);
        assert!((0..8).all(|_| task.pass(&sounds).is_none()));
    }

    fn ride() -> Ride {
        let mut bytes = [0u8; SIZE];
        let words: [i32; 8] = [300, 517, 6, 2, -10, 0, 20, 1556];
        for (n, word) in words.iter().enumerate() {
            bytes[4 * n..4 * n + 4].copy_from_slice(&word.to_le_bytes());
        }
        Ride::parse(&bytes).expect("a record")
    }

    #[test]
    fn a_record_reads_its_fields() {
        let ride = ride();
        assert_eq!(
            (ride.room, ride.goes_to, ride.flag, ride.region),
            (300, 517, 6, 2)
        );
        assert_eq!((ride.place, ride.angle), ([-10, 0, 20], 1556));
        assert!(Ride::parse(&[0; SIZE - 1]).is_none());
    }

    #[test]
    fn the_two_variables_give_the_room_again() {
        let ride = ride();
        assert_eq!(ride.variables(), [2, 5]);
        assert_eq!(room_of(ride.variables()), 517);
    }

    #[test]
    fn the_region_starts_the_script_and_state_1_leaves() {
        let mut riding = Riding::new();
        assert_eq!(riding.pass(false, false), Rode::Nothing);
        assert_eq!(riding.pass(true, false), Rode::Starts);
        assert_eq!((riding.told, riding.reached), (-1, 0));
        // While the script runs the region tells nothing.
        assert_eq!(riding.pass(true, true), Rode::Nothing);
        riding.tell(LEAVE);
        assert_eq!(riding.pass(true, true), Rode::Leaves);
        assert_eq!((riding.told, riding.reached), (-1, LEAVE));
        // A state the step does not know stays told and does nothing.
        riding.tell(2);
        assert_eq!(riding.pass(false, true), Rode::Nothing);
        assert_eq!(riding.told, 2);
    }

    #[test]
    fn a_script_of_arrival_needs_the_variable() {
        let rides = Rides {
            at: 0x1000,
            ride: ride(),
            leaves: 0x2000,
            lost: 0x3000,
            back: 0x4000,
        };
        assert_eq!(rides.arrival(0, true), None);
        assert_eq!(rides.arrival(3, true), Some(0x3000));
        assert_eq!(rides.arrival(3, false), Some(0x4000));
    }
}
