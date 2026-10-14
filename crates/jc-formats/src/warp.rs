
use crate::scene::ScenePackage;

/// The type of the object, the extra bytes its task gets, and the bytes of its record
/// that the routines read.
pub const KIND: u32 = 0x11;
const EXTRA: u32 = 12;
const SIZE: usize = 40;
/// The story flag a departure sets and an arrival clears.
pub const FLAG: u32 = 114;
/// The kept sound of told state 0, which told state 3 lets go, and the sound of told
/// state 4.
pub const KEPT_SOUND: u32 = 45;
pub const SOUND: u32 = 48;

/// Words of the start routine that tell it from any other, by their distance from its
/// start: the -1 for the script's handle, the stores of the state and of the handle, and
/// the store of the step as the routine of every game frame. The call of the step stands
/// at `CALL`, its address as an upper and a lower half at `STEP`.
const START: [(u32, u32); 5] = [
    (0x00, 0x27BD_FFE8),
    (0x0C, 0x2402_FFFF),
    (0x14, 0xAE00_0014),
    (0x1C, 0xAE02_0018),
    (0x28, 0xAE02_0090),
];
const CALL: u32 = 0x18;
const STEP: (u32, u32) = (0x20, 0x24);
/// Words of the step: the kept sound, the sound of state 4, the story flag.
const STEP_WORDS: [(u32, u32); 3] = [
    (0x58, 0x2404_0000 | KEPT_SOUND),
    (0x124, 0x2404_0000 | SOUND),
    (0x178, 0x2404_0000 | FLAG),
];
/// Places in the step of the two tables of music by the place (an upper half in `v1`
/// and the `addiu` into `t1`) and of the two scripts (an upper half in `a0`).
const MUSIC: [(u32, u32); 2] = [(0x74, 0x78), (0xDC, 0xE0)];
const ARRIVAL: (u32, u32) = (0x18C, 0x190);
const DEPARTURE: (u32, u32) = (0x204, 0x208);

/// The record of the object, its data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Warp {
    /// The thing of the story the player must have, -1 for none (the object then never
    /// sends the game anywhere).
    pub thing: i32,
    /// Where its show lists are drawn, and their upright angle.
    pub shows_at: [i32; 3],
    pub shows_angle: i16,
    /// Where the player stands in the room behind, and his upright angle there.
    pub arrival: [i32; 3],
    pub angle: i16,
    /// The trigger region of table 5 the player steps into.
    pub region: i16,
    /// The slot of the same area and place the object sends the game to.
    pub slot: i16,
}

impl Warp {
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let bytes: &[u8; SIZE] = bytes.get(..SIZE)?.try_into().ok()?;
        let word = |at: usize| i32::from_le_bytes(bytes[at..at + 4].try_into().expect("4 bytes"));
        let half = |at: usize| i16::from_le_bytes(bytes[at..at + 2].try_into().expect("2 bytes"));
        Some(Self {
            thing: word(0),
            shows_at: [word(4), word(8), word(12)],
            shows_angle: half(16),
            arrival: [word(20), word(24), word(28)],
            angle: half(32),
            region: half(36),
            slot: half(38),
        })
    }
}

/// The object of a package with the address of its record, its two scripts and its two
/// tables of music, each of four numbers by the place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Warps {
    pub at: u32,
    pub warp: Warp,
    pub departure: u32,
    pub arrival: u32,
    pub music: [[u32; 4]; 2],
}

impl Warps {
    /// The object in the object lists of a package; none for a package without. It is
    /// told apart by its start routine and its step, whose words are the same in all
    /// fourteen packages but for the package's own addresses.
    pub fn of(package: &ScenePackage) -> Option<Self> {
        let word = |address: u32| {
            let bytes = package.bytes_at(address, 4)?;
            Some(u32::from_le_bytes(bytes.try_into().expect("4 bytes")))
        };
        // An address from an upper half (`lui` of the register `upper`) and the `addiu`
        // that adds the lower half (`lower`: the upper half of the instruction).
        let address = |at: (u32, u32), upper: u32, lower: u32| {
            let (high, low) = (word(at.0)?, word(at.1)?);
            (high >> 16 == upper && low >> 16 == lower)
                .then(|| (high << 16).wrapping_add(low as u16 as i16 as u32))
        };
        let lists = package.object_lists();
        (lists.iter().flat_map(|list| &list.objects))
            .filter(|object| object.kind == KIND && object.extra == EXTRA)
            .find_map(|object| {
                let start = object.function;
                for (at, code) in START {
                    if word(start.checked_add(at)?)? != code {
                        return None;
                    }
                }
                let step = address((start + STEP.0, start + STEP.1), 0x3C02, 0x2442)?;
                let call = word(start + CALL)?;
                if call != 0x0C00_0000 | (step & 0x0FFF_FFFF) >> 2 {
                    return None;
                }
                for (at, code) in STEP_WORDS {
                    if word(step.checked_add(at)?)? != code {
                        return None;
                    }
                }
                let script = |at: (u32, u32)| {
                    address((step + at.0, step + at.1), 0x3C04, 0x2484)
                        .filter(|script| package.bytes_at(*script, 16).is_some())
                };
                let table = |at: (u32, u32)| -> Option<[u32; 4]> {
                    let table = address((step + at.0, step + at.1), 0x3C03, 0x2469)?;
                    let bytes = package.bytes_at(table, 16)?;
                    Some(std::array::from_fn(|n| {
                        u32::from_le_bytes(bytes[4 * n..4 * n + 4].try_into().expect("4 bytes"))
                    }))
                };
                Some(Self {
                    at: object.data,
                    warp: package.bytes_at(object.data, SIZE).and_then(Warp::parse)?,
                    departure: script(DEPARTURE)?,
                    arrival: script(ARRIVAL)?,
                    music: [table(MUSIC[0])?, table(MUSIC[1])?],
                })
            })
    }

    /// The music of a table by the wanted place (1 to 4), none for another place.
    pub fn music_of(&self, table: usize, place: u16) -> Option<u32> {
        let row = usize::from(place).checked_sub(1)?;
        self.music.get(table)?.get(row).copied()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Heard {
    /// State 0: the kept sound 45 is pressed.
    Kept,
    /// State 1: the music of the first table.
    Music,
    /// State 3: the kept sound is let go.
    Ended,
    /// State 4: the music of the second table and sound 48.
    MusicAndSound,
}

/// What the step did in a game frame, besides the sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Warped {
    Nothing,
    /// Story flag 114 was set: it is cleared and the arrival script starts with the
    /// object as its owner.
    Arrives,
    /// The player stepped in with the thing: flag 114 is set and the departure script
    /// starts with the object as its owner.
    Departs,
    /// The departure script has ended: the game goes to the record's slot, with the
    /// player on its arrival place.
    Leaves,
}

/// What the step asks in a game frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Around {
    /// Story flag 114 is set.
    pub flag: bool,
    /// The player's place is in the record's trigger region.
    pub in_region: bool,
    /// The player has the thing of the story the record names.
    pub has_thing: bool,
    /// The script the object started last still runs.
    pub script_runs: bool,
}

/// The words of the object's task that its step works on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Warping {
    /// The state (+20): 0 the first game frame, 1 waiting, 2 leaving, 3 arrived.
    pub state: i32,
    /// The state told (+124), -1 for none.
    pub told: i32,
    /// A script of the object runs (the handle at +24 is not -1): its show lists are
    /// drawn at the record's place.
    pub script: bool,
}

impl Default for Warping {
    fn default() -> Self {
        Self::new()
    }
}

impl Warping {
    /// The task as the start routine leaves it, before its call of the step.
    pub fn new() -> Self {
        Self {
            state: 0,
            told: -1,
            script: false,
        }
    }

    /// The script the object started tells it a state (`0x30`).
    pub fn tell(&mut self, state: i32) {
        self.told = state;
    }

    /// One game frame of the step: first the told state, which is taken, then the
    /// state's work.
    pub fn pass(&mut self, warp: &Warp, around: Around) -> (Option<Heard>, Warped) {
        let heard = match std::mem::replace(&mut self.told, -1) {
            0 => Some(Heard::Kept),
            1 => Some(Heard::Music),
            3 => Some(Heard::Ended),
            4 => Some(Heard::MusicAndSound),
            _ => None,
        };
        let warped = match self.state {
            0 if around.flag => {
                (self.state, self.script) = (3, true);
                Warped::Arrives
            }
            0 => {
                self.state = 1;
                Warped::Nothing
            }
            1 if around.in_region && warp.thing >= 0 && around.has_thing => {
                (self.state, self.script) = (2, true);
                Warped::Departs
            }
            2 if !around.script_runs => Warped::Leaves,
            3 if !around.script_runs && !around.in_region => {
                (self.state, self.script) = (1, false);
                Warped::Nothing
            }
            _ => Warped::Nothing,
        };
        (heard, warped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn warp(thing: i32) -> Warp {
        let mut bytes = [0u8; SIZE];
        let words: [i32; 10] = [
            thing,
            0,
            -184,
            -43,
            0,
            0,
            -175,
            734,
            0xFFFF_F800u32 as i32,
            0x000B_0001,
        ];
        for (n, word) in words.iter().enumerate() {
            bytes[4 * n..4 * n + 4].copy_from_slice(&word.to_le_bytes());
        }
        Warp::parse(&bytes).expect("a record")
    }

    #[test]
    fn a_record_reads_its_fields() {
        let warp = warp(22);
        assert_eq!(
            (warp.thing, warp.shows_at, warp.shows_angle),
            (22, [0, -184, -43], 0)
        );
        assert_eq!((warp.arrival, warp.angle), ([0, -175, 734], -2048));
        assert_eq!((warp.region, warp.slot), (1, 11));
        assert!(Warp::parse(&[0; SIZE - 1]).is_none());
    }

    #[test]
    fn with_the_thing_a_step_into_the_region_departs_and_the_end_of_the_script_leaves() {
        let warp = warp(22);
        let mut task = Warping::new();
        let calm = Around::default();
        // The first game frame without the flag: waiting.
        assert_eq!(task.pass(&warp, calm), (None, Warped::Nothing));
        assert_eq!(task.state, 1);
        // In the region without the thing, or with it but out of the region: nothing.
        let without = Around {
            in_region: true,
            ..calm
        };
        assert_eq!(task.pass(&warp, without).1, Warped::Nothing);
        let away = Around {
            has_thing: true,
            ..calm
        };
        assert_eq!(task.pass(&warp, away).1, Warped::Nothing);
        let steps_in = Around {
            in_region: true,
            has_thing: true,
            ..calm
        };
        assert_eq!(task.pass(&warp, steps_in).1, Warped::Departs);
        assert!(task.script && task.state == 2);
        // The script tells the sounds; each is taken once.
        let runs = Around {
            script_runs: true,
            ..steps_in
        };
        task.tell(0);
        assert_eq!(task.pass(&warp, runs), (Some(Heard::Kept), Warped::Nothing));
        assert_eq!(task.pass(&warp, runs), (None, Warped::Nothing));
        task.tell(1);
        assert_eq!(task.pass(&warp, runs).0, Some(Heard::Music));
        task.tell(2);
        assert_eq!(task.pass(&warp, runs).0, None);
        task.tell(3);
        assert_eq!(task.pass(&warp, runs).0, Some(Heard::Ended));
        // The script's end sends the game away, every game frame until the room goes.
        assert_eq!(task.pass(&warp, steps_in).1, Warped::Leaves);
        assert_eq!(task.pass(&warp, steps_in).1, Warped::Leaves);
    }

    #[test]
    fn a_record_without_a_thing_never_departs() {
        let warp = warp(-1);
        let mut task = Warping::new();
        task.pass(&warp, Around::default());
        let steps_in = Around {
            in_region: true,
            has_thing: true,
            ..Around::default()
        };
        assert_eq!(task.pass(&warp, steps_in).1, Warped::Nothing);
        assert_eq!(task.state, 1);
    }

    #[test]
    fn an_arrival_waits_for_its_script_and_for_the_player_to_step_out() {
        let warp = warp(22);
        let mut task = Warping::new();
        let arrived = Around {
            flag: true,
            in_region: true,
            has_thing: true,
            script_runs: false,
        };
        assert_eq!(task.pass(&warp, arrived).1, Warped::Arrives);
        assert!(task.script && task.state == 3);
        let runs = Around {
            flag: false,
            script_runs: true,
            ..arrived
        };
        task.tell(4);
        assert_eq!(
            task.pass(&warp, runs),
            (Some(Heard::MusicAndSound), Warped::Nothing)
        );
        // Ended, but the player still stands in the region: no departure yet.
        let stands = Around {
            script_runs: false,
            ..runs
        };
        assert_eq!(task.pass(&warp, stands).1, Warped::Nothing);
        assert_eq!(task.state, 3);
        let out = Around {
            in_region: false,
            ..stands
        };
        assert_eq!(task.pass(&warp, out).1, Warped::Nothing);
        assert!(!task.script && task.state == 1);
        // Back in: away again.
        assert_eq!(task.pass(&warp, stands).1, Warped::Departs);
    }

    #[test]
    fn the_music_is_picked_by_the_place() {
        let warps = Warps {
            at: 0,
            warp: warp(22),
            departure: 0,
            arrival: 0,
            music: [[112, 114, 116, 118], [129, 130, 131, 132]],
        };
        assert_eq!(warps.music_of(0, 2), Some(114));
        assert_eq!(warps.music_of(1, 4), Some(132));
        assert_eq!(warps.music_of(0, 0), None);
        assert_eq!(warps.music_of(0, 5), None);
    }
}
