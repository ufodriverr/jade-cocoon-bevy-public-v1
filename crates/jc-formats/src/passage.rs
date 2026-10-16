
use crate::scene::ScenePackage;

/// The type of the object, the extra bytes its task gets, and the bytes of its record.
pub const KIND: u32 = 0x0D;
const EXTRA: u32 = 12;
const SIZE: usize = 44;
/// The start routine calls the routine of the object here, and the routine names its
/// script at this place: an upper half in `a0`, and a lower half one word behind.
const CALL: u32 = 0x20;
const SCRIPT: u32 = 0xB0;
/// The states the script tells a passage: show a list, put the player across.
pub const SHOW: i32 = 1;
pub const CROSS: i32 = 2;

/// The record of one bank of a passage, the data of its object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Passage {
    /// The story flag that opens the passage.
    pub flag: i32,
    /// The trigger region the player stands in, and the point the player then faces.
    pub region: i32,
    pub point: [i32; 3],
    /// Where the object puts the player, and the player's angle there.
    pub place: [i32; 3],
    pub angle: i16,
    /// The two show lists of the package that the object hands the spawner in turn.
    pub shows: [u32; 2],
}

impl Passage {
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let bytes: &[u8; SIZE] = bytes.get(..SIZE)?.try_into().ok()?;
        let word = |at: usize| i32::from_le_bytes(bytes[at..at + 4].try_into().expect("4 bytes"));
        let half = |at: usize| i16::from_le_bytes(bytes[at..at + 2].try_into().expect("2 bytes"));
        Some(Self {
            flag: word(0),
            region: word(4),
            point: [word(8), word(12), word(16)],
            place: [word(20), word(24), word(28)],
            angle: half(32),
            shows: [word(36) as u32, word(40) as u32],
        })
    }
}

/// The banks of a package's passage, each with the address of its record, and the script
/// their routine starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Passages {
    pub passages: Vec<(u32, Passage)>,
    pub script: u32,
}

impl Passages {
    /// The passage in the object lists of a package; none for a package without. The
    /// lock of a door has the same type and extra bytes (`crate::lock`): an object is a
    /// bank when its routine names a script where this one does and its two show lists
    /// lie in the package.
    pub fn of(package: &ScenePackage) -> Option<Self> {
        let word = |address: u32| {
            let bytes = package.bytes_at(address, 4)?;
            Some(u32::from_le_bytes(bytes.try_into().expect("4 bytes")))
        };
        // The script of an object by its start routine: a `jal` of the routine, and in
        // the routine the two halves of an address for `a0`.
        let script_of = |start: u32| {
            let call = word(start.checked_add(CALL)?)?;
            if call >> 26 != 3 {
                return None;
            }
            let at = (0x8000_0000 | (call & 0x03FF_FFFF) << 2) + SCRIPT;
            let (upper, lower) = (word(at)?, word(at + 4)?);
            (upper >> 16 == 0x3C04 && lower >> 16 == 0x2484)
                .then(|| (upper << 16).wrapping_add(lower as u16 as i16 as u32))
        };
        let lists = package.object_lists();
        let mut passages: Vec<(u32, Passage)> = Vec::new();
        let mut script = None;
        for object in (lists.iter().flat_map(|list| &list.objects))
            .filter(|object| object.kind == KIND && object.extra == EXTRA)
        {
            if passages.iter().any(|(at, _)| *at == object.data) {
                continue;
            }
            let (Some(found), Some(passage)) = (
                script_of(object.function),
                package.bytes_at(object.data, SIZE).and_then(Passage::parse),
            ) else {
                continue;
            };
            if (passage.shows.iter()).any(|&list| package.bytes_at(list, 4).is_none())
                || package.bytes_at(found, 16).is_none()
            {
                continue;
            }
            passages.push((object.data, passage));
            script = Some(found);
        }
        Some(Self {
            passages,
            script: script?,
        })
    }
}

/// What a passage's routine is told about the game frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Around {
    /// The player stands in the object's region and faces its point.
    pub at: bool,
    /// The script the object started last still runs.
    pub script_runs: bool,
    /// The object's story flag is set.
    pub flag: bool,
}

/// What a passage's routine did in a game frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Passed {
    Nothing,
    /// The script of the package's routine starts, with the object as its owner.
    Starts,
    /// The show list at this address goes to the spawner.
    Shows(u32),
    /// The player is put on the object's place.
    Crosses,
}

/// The words of a passage's task that its routine works on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Passing {
    /// The state a script or a press told it (+124 of the task), -1 for none, and the
    /// state it has reached (+128).
    pub told: i32,
    pub reached: i32,
    /// The mark of the game frame (+120): a press of the confirm button tells the
    /// object state 0.
    pub marked: bool,
    /// Which of the two show lists is next.
    pub side: usize,
}

impl Default for Passing {
    fn default() -> Self {
        Self::new()
    }
}

impl Passing {
    /// The task as the spawner and the start routine leave it.
    pub fn new() -> Self {
        Self {
            told: -1,
            reached: -1,
            marked: false,
            side: 0,
        }
    }

    /// A script tells the object a state (`0x30` of the script the object started).
    pub fn tell(&mut self, state: i32) {
        self.told = state;
    }

    /// Whether a press of the confirm button would tell the object state 0, asked
    /// without telling it: the test of [`Self::press`] (the build's hint of what the
    /// press would do, `interaction_hint`).
    pub fn would_press(&self) -> bool {
        self.marked
    }

    /// The second half of the pad routine's call for a talk (`0x800A5BD0`): a marked
    /// object is told state 0. Answers whether this one was.
    pub fn press(&mut self) -> bool {
        let marked = self.would_press();
        if marked {
            self.told = 0;
        }
        marked
    }

    /// One game frame of the task: the wrapper, which takes the mark away, and the
    /// object's routine.
    pub fn pass(&mut self, passage: &Passage, around: Around) -> Passed {
        self.marked = false;
        if matches!(self.told, -3 | -2) {
            return Passed::Nothing;
        }
        if around.at && !around.script_runs {
            self.marked = true;
        }
        match self.told {
            // State 0 is not noted as reached: only the states 1 and 2 are.
            0 => {
                self.told = -1;
                match around.flag {
                    true => Passed::Starts,
                    false => Passed::Nothing,
                }
            }
            SHOW => {
                let list = passage.shows[self.side];
                self.side ^= 1;
                (self.reached, self.told) = (SHOW, -1);
                Passed::Shows(list)
            }
            CROSS => {
                (self.reached, self.told) = (CROSS, -1);
                Passed::Crosses
            }
            _ => Passed::Nothing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn passage() -> Passage {
        let mut bytes = [0u8; SIZE];
        let words: [i32; 11] = [41, 1, 10, 20, 30, 40, 50, 60, -2048, 0x1000, 0x2000];
        for (n, word) in words.iter().enumerate() {
            bytes[4 * n..4 * n + 4].copy_from_slice(&word.to_le_bytes());
        }
        Passage::parse(&bytes).expect("a record")
    }

    #[test]
    fn a_record_reads_its_fields() {
        let passage = passage();
        assert_eq!((passage.flag, passage.region), (41, 1));
        assert_eq!((passage.point, passage.place), ([10, 20, 30], [40, 50, 60]));
        assert_eq!((passage.angle, passage.shows), (-2048, [0x1000, 0x2000]));
        assert!(Passage::parse(&[0; SIZE - 1]).is_none());
    }

    #[test]
    fn without_the_flag_a_press_starts_nothing() {
        let passage = passage();
        let mut passing = Passing::new();
        let at = Around {
            at: true,
            ..Around::default()
        };
        // Away from the bank a press tells nothing.
        assert_eq!(passing.pass(&passage, Around::default()), Passed::Nothing);
        assert!(!passing.press());
        assert_eq!(passing.pass(&passage, at), Passed::Nothing);
        assert!(passing.press());
        assert_eq!(passing.pass(&passage, at), Passed::Nothing);
        // The press is used up, and the player is marked again.
        assert_eq!((passing.told, passing.reached), (-1, -1));
        assert!(passing.marked);
    }

    #[test]
    fn with_the_flag_the_script_shows_a_list_crosses_and_shows_the_other() {
        let passage = passage();
        let mut passing = Passing::new();
        let at = Around {
            at: true,
            flag: true,
            ..Around::default()
        };
        passing.pass(&passage, at);
        assert!(passing.press());
        assert_eq!(passing.pass(&passage, at), Passed::Starts);
        // While the script runs there is no mark.
        let runs = Around {
            script_runs: true,
            ..at
        };
        assert_eq!(passing.pass(&passage, runs), Passed::Nothing);
        assert!(!passing.marked && !passing.press());
        passing.tell(SHOW);
        assert_eq!(passing.pass(&passage, runs), Passed::Shows(0x1000));
        assert_eq!((passing.reached, passing.told), (SHOW, -1));
        // Nothing waits for the list: the player is put across when the script says so.
        passing.tell(CROSS);
        assert_eq!(passing.pass(&passage, runs), Passed::Crosses);
        assert_eq!(passing.reached, CROSS);
        passing.tell(SHOW);
        assert_eq!(passing.pass(&passage, runs), Passed::Shows(0x2000));
        // The lists go on in turn at the next crossing.
        passing.tell(SHOW);
        assert_eq!(passing.pass(&passage, runs), Passed::Shows(0x1000));
        // A state the routine does not know stays told and does nothing.
        passing.tell(3);
        assert_eq!(passing.pass(&passage, runs), Passed::Nothing);
        assert_eq!(passing.told, 3);
    }
}
