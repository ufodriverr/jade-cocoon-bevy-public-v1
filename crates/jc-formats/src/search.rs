
use crate::scene::{NoticeThing, ScenePackage};
use crate::skeleton::{self, Placement, cosine, sine};

/// The type of the object, the extra bytes its task gets, and the bytes of its record.
const KIND: u32 = 0x04;
const EXTRA: u32 = 0x18;
const SIZE: usize = 72;
/// The script's commands that ask the four routines, in the order of [`Routine`].
const ASKS: [usize; 4] = [6, 13, 30, 46];
const COMMAND_ASK: u32 = 0x4E;
const COMMANDS: usize = 53;
/// The bits of a record: gone once it is searched, kept by its flag, to be searched
/// again, and state 3 waits for the lid.
pub const GONE: i32 = 1;
pub const KEPT: i32 = 2;
pub const AGAIN: i32 = 4;
pub const WAITS: i32 = 8;
/// The look whose drawer draws, and the game frames a lid takes.
pub const LOOK_DRAWN: i32 = 3;
pub const OPEN: i32 = 10;
/// Where each of the two mesh objects lies from the thing's place before the thing's
/// angle turns it (the same six words in every package), and how far each is turned
/// about its third axis when the thing is open.
const OFFSETS: [[i32; 3]; 2] = [[0, 0, 0], [74, -55, 1]];
const TURNS: [i32; 2] = [0, 1024];
/// The states a script tells the thing: it is searched, it is drawn or not, it opens,
/// and it hands itself to the script.
pub const FOUND: i32 = 1;
pub const SHOWN: i32 = 2;
pub const OPENS: i32 = 3;
pub const ITSELF: i32 = 4;

/// The record of a thing to be searched, the data of its object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Find {
    /// The story flag that says it has been searched, negative for none.
    pub flag: i32,
    /// The item the player must have, negative for none.
    pub needs: i32,
    pub look: i32,
    pub bits: i32,
    /// The trigger region the player stands in, and the place the player then faces,
    /// where the thing is drawn with its angle.
    pub region: i32,
    pub place: [i32; 3],
    pub angle: i16,
    /// The lines that say where it lies, and the lines of a thing without a need that
    /// is searched again; 0 for none.
    pub lines: u32,
    pub again: u32,
    /// What a search gives: the list of its notice.
    pub given: [Option<NoticeThing>; 3],
    /// The mesh objects of the room's model block it is drawn with; the second is the
    /// lid.
    pub meshes: [Option<u16>; 2],
}

impl Find {
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let bytes: &[u8; SIZE] = bytes.get(..SIZE)?.try_into().ok()?;
        let word = |at: usize| i32::from_le_bytes(bytes[at..at + 4].try_into().expect("4 bytes"));
        let half = |at: usize| i16::from_le_bytes(bytes[at..at + 2].try_into().expect("2 bytes"));
        let given = |n: usize| {
            let at = 44 + 8 * n;
            (half(at) >= 0).then(|| NoticeThing {
                kind: half(at).into(),
                number: half(at + 2).into(),
                count: word(at + 4),
            })
        };
        let mesh = |at: usize| u16::try_from(half(at)).ok();
        Some(Self {
            flag: word(0),
            needs: word(4),
            look: word(8),
            bits: word(12),
            region: word(16),
            place: [word(20), word(24), word(28)],
            angle: half(32),
            lines: word(36) as u32,
            again: word(40) as u32,
            given: [given(0), given(1), given(2)],
            meshes: [mesh(68), mesh(70)],
        })
    }

    fn has(&self, bit: i32) -> bool {
        self.bits & bit != 0
    }

    /// Where the drawer of the look that draws puts the thing's mesh objects when the
    /// lid's amount is `amount`: each mesh object of the room's model block with its
    /// place and angles for the mesh drawer. The offset is turned by the thing's angle,
    /// each product divided towards zero; the angles are the thing's about the upright
    /// axis and the lid's share of a quarter turn about the third.
    pub fn stands(&self, amount: i32) -> Vec<(usize, Placement)> {
        let turn = i32::from(self.angle);
        let (sin, cos) = (sine(turn), cosine(turn));
        let meshes = self.meshes.iter().zip(OFFSETS).zip(TURNS);
        meshes
            .filter_map(|((mesh, [x, y, z]), quarter)| {
                let position = [
                    self.place[0] + x * cos / 4096 + z * sin / 4096,
                    self.place[1] + y,
                    self.place[2] - x * sin / 4096 + z * cos / 4096,
                ];
                let lid = (quarter * amount / OPEN) as i16;
                Some((
                    usize::from((*mesh)?),
                    Placement {
                        matrix: skeleton::rotation_yxz([0, self.angle, lid]),
                        position,
                    },
                ))
            })
            .collect()
    }
}

/// The four native routines the script asks, in the order it asks them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Routine {
    /// Has the thing not been searched yet?
    Fresh,
    /// Does it need a thing, and has the player got it?
    Needs,
    /// Does this search give something?
    Gives,
    /// What is said of a thing that was searched before.
    Before,
}

/// The thing to be searched of a package: its record with the address of it, the script
/// its routine starts, the four routines by address, and the two pairs of lines every
/// package has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Finds {
    pub at: u32,
    pub find: Find,
    pub script: u32,
    pub routines: [u32; 4],
    /// The lines of a box the player has not the thing for, and of an empty one.
    pub locked: u32,
    pub empty: u32,
}

impl Finds {
    /// The thing of a package; none for a package without.
    pub fn of(package: &ScenePackage) -> Option<Self> {
        let word = |address: u32| {
            let bytes = package.bytes_at(address, 4)?;
            Some(u32::from_le_bytes(bytes.try_into().expect("4 bytes")))
        };
        // The words of a routine up to its return.
        let words = |routine: u32| {
            (0..0x80u32)
                .map_while(move |n| word(routine + 4 * n))
                .take_while(|word| *word != 0x03E0_0008)
        };
        let lists = package.object_lists();
        let object = (lists.iter().flat_map(|list| &list.objects))
            .find(|object| object.kind == KIND && object.extra == EXTRA)?;
        let find = package.bytes_at(object.data, SIZE).and_then(Find::parse)?;
        // The start routine's last call into the package is the routine of every game
        // frame, and the first address that one hands on is its script.
        let inside = |address: u32| package.bytes_at(address, 4).is_some();
        let routine = words(object.function)
            .filter(|word| word >> 26 == 3)
            .map(|word| 0x8000_0000 | (word & 0x03FF_FFFF) << 2)
            .filter(|target| inside(*target))
            .last()?;
        let lower = |word: u32| 0x800B_0000u32.wrapping_add(word as u16 as i16 as u32);
        let script = words(routine)
            .find(|word| word >> 16 == 0x2484)
            .map(lower)?;
        let commands = package.commands_at(script);
        if commands.len() != COMMANDS {
            return None;
        }
        let mut routines = [0; 4];
        for (routine, ask) in routines.iter_mut().zip(ASKS) {
            if commands[ask].code != COMMAND_ASK {
                return None;
            }
            *routine = commands[ask].c;
        }
        // The lines a routine names: the lower half behind an upper half of the package.
        let named = |routine: u32| {
            let mut upper = false;
            words(routine).find_map(|word| {
                let found = upper && word >> 16 == 0x2442;
                upper = word == 0x3C02_800B || (upper && !found);
                found.then(|| lower(word))
            })
        };
        Some(Self {
            at: object.data,
            find,
            script,
            routines,
            locked: named(routines[1])?,
            empty: named(routines[3])?,
        })
    }

    pub fn routine(&self, address: u32) -> Option<Routine> {
        const ORDER: [Routine; 4] = [
            Routine::Fresh,
            Routine::Needs,
            Routine::Gives,
            Routine::Before,
        ];
        let at = self
            .routines
            .iter()
            .position(|routine| *routine == address)?;
        Some(ORDER[at])
    }
}

/// What a routine left with the thing for the script's next notice (+140 of the task;
/// the used thing is at +132).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handed {
    /// The task itself, as state 4 leaves it.
    Itself,
    /// Lines at an address, for `0x3B` with a `c` of -1; 0 for none.
    Lines(u32),
    /// The record's list, for `0x3C` with a `c` of 0.
    Given,
}

/// What the thing's routine did in a game frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Passed {
    Nothing,
    /// Its script starts.
    Starts,
    /// Its story flag is set.
    Found,
}

/// The words of the thing's task that its routine works on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Search {
    /// The state a script or a press told it (+124 of the task), -1 for none, and the
    /// state it has reached (+128).
    pub told: i32,
    pub reached: i32,
    /// The mark of the game frame (+120): a press of the confirm button tells it state 0.
    pub marked: bool,
    /// The six words the start routine fills, less the script's handle: a press starts
    /// the script, it has been searched, it is drawn, the lid opens, and how far it has.
    pub open_to_search: bool,
    pub searched: bool,
    pub drawn: bool,
    pub opening: bool,
    pub amount: i32,
    pub handed: Handed,
    /// The thing of the player that the search used.
    pub used: Option<i32>,
}

impl Search {
    /// The task as the start routine leaves it; `flag` says whether the record's story
    /// flag is set.
    pub fn new(find: &Find, flag: bool) -> Self {
        let fresh = find.has(KEPT) && find.flag >= 0 && !flag;
        Self {
            told: -1,
            reached: -1,
            marked: false,
            open_to_search: fresh || find.has(AGAIN),
            searched: !fresh,
            drawn: fresh || !find.has(GONE),
            opening: !fresh,
            amount: if fresh { 0 } else { OPEN },
            handed: Handed::Lines(0),
            used: None,
        }
    }

    /// A script tells the thing a state (`0x30` of the script it started).
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

    /// One game frame of the task. `at`: the player stands in the region and faces the
    /// place; `script_runs`: the script the thing started last still runs.
    pub fn pass(&mut self, find: &Find, at: bool, script_runs: bool) -> Passed {
        self.marked = at && !script_runs;
        let reached = self.told;
        let passed = match self.told {
            // A thing that is not to be searched keeps the state it was told.
            0 if !self.open_to_search => return self.draw(find, Passed::Nothing),
            0 => Passed::Starts,
            FOUND => {
                self.searched = true;
                self.open_to_search = find.has(AGAIN);
                Passed::Found
            }
            SHOWN => {
                self.drawn = !find.has(GONE);
                Passed::Nothing
            }
            OPENS => {
                self.opening = true;
                if find.has(WAITS) && self.amount < OPEN {
                    return self.draw(find, Passed::Nothing);
                }
                Passed::Nothing
            }
            ITSELF => {
                self.handed = Handed::Itself;
                Passed::Nothing
            }
            _ => return self.draw(find, Passed::Nothing),
        };
        (self.reached, self.told) = (reached, -1);
        self.draw(find, passed)
    }

    /// The drawer of the look that draws: the lid goes on by one a game frame.
    fn draw(&mut self, find: &Find, passed: Passed) -> Passed {
        if self.drawn && find.look == LOOK_DRAWN {
            self.amount = (self.amount + i32::from(self.opening)).clamp(0, OPEN);
        }
        passed
    }

    /// The answer of one of the script's routines, which becomes the script's choice.
    /// `has`: the player has at least one of the thing the record needs.
    pub fn answer(&mut self, routine: Routine, finds: &Finds, has: bool) -> u32 {
        let find = &finds.find;
        match routine {
            Routine::Fresh => u32::from(!self.searched),
            Routine::Needs if find.needs < 0 => {
                self.handed = Handed::Lines(find.lines);
                2
            }
            Routine::Needs if !has => {
                self.handed = Handed::Lines(finds.locked);
                1
            }
            Routine::Needs => {
                self.used = Some(find.needs);
                0
            }
            Routine::Gives if find.has(KEPT) && !self.searched => {
                self.handed = Handed::Given;
                0
            }
            Routine::Gives => 1,
            Routine::Before => {
                let lines = if find.needs < 0 {
                    find.again
                } else {
                    finds.empty
                };
                self.handed = Handed::Lines(lines);
                0
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finds(needs: i32, bits: i32) -> Finds {
        let mut bytes = [0u8; SIZE];
        let words: [i32; 11] = [59, needs, 3, bits, 3, 890, 0, 4481, 1024, 0x5000, 0];
        for (n, word) in words.iter().enumerate() {
            bytes[4 * n..4 * n + 4].copy_from_slice(&word.to_le_bytes());
        }
        // One thing of the story, number 25, and two empty places of the list.
        bytes[44..52].copy_from_slice(&[5, 0, 25, 0, 1, 0, 0, 0]);
        for at in [52, 60] {
            bytes[at..at + 2].copy_from_slice(&(-1i16).to_le_bytes());
        }
        bytes[70..72].copy_from_slice(&(-1i16).to_le_bytes());
        Finds {
            at: 0x4000,
            find: Find::parse(&bytes).expect("a record"),
            script: 0x6000,
            routines: [0x100, 0x200, 0x300, 0x400],
            locked: 0x7000,
            empty: 0x7100,
        }
    }

    #[test]
    fn a_record_reads_its_fields() {
        let finds = finds(-1, 3);
        let find = finds.find;
        assert_eq!(
            (find.flag, find.needs, find.look, find.bits),
            (59, -1, 3, 3)
        );
        assert_eq!((find.region, find.place), (3, [890, 0, 4481]));
        assert_eq!((find.angle, find.lines, find.again), (1024, 0x5000, 0));
        let thing = NoticeThing {
            kind: 5,
            number: 25,
            count: 1,
        };
        assert_eq!(find.given, [Some(thing), None, None]);
        assert_eq!(find.meshes, [Some(0), None]);
        assert!(Find::parse(&[0; SIZE - 1]).is_none());
        // One mesh object, at the thing's place and turned by its angle.
        let stands = find.stands(OPEN);
        assert_eq!(stands.len(), 1);
        assert_eq!((stands[0].0, stands[0].1.position), (0, [890, 0, 4481]));
        assert_eq!(stands[0].1.matrix, skeleton::rotation_yxz([0, 1024, 0]));
        assert_eq!(finds.routine(0x300), Some(Routine::Gives));
        assert_eq!(finds.routine(0x500), None);
    }

    #[test]
    fn a_thing_is_found_once_and_is_gone() {
        let finds = finds(-1, 3);
        let find = &finds.find;
        let mut search = Search::new(find, false);
        assert!(search.open_to_search && search.drawn && !search.searched);
        // Away from it a press tells nothing; at it the press starts the script.
        assert_eq!(search.pass(find, false, false), Passed::Nothing);
        assert!(!search.press());
        search.pass(find, true, false);
        assert!(search.press());
        assert_eq!(search.pass(find, true, false), Passed::Starts);
        assert_eq!((search.reached, search.told), (0, -1));
        // While the script runs there is no mark.
        search.tell(ITSELF);
        assert_eq!(search.pass(find, true, true), Passed::Nothing);
        assert!(!search.marked && search.handed == Handed::Itself);
        assert_eq!(search.answer(Routine::Fresh, &finds, false), 1);
        assert_eq!(search.answer(Routine::Needs, &finds, false), 2);
        assert_eq!(search.handed, Handed::Lines(0x5000));
        assert_eq!(search.answer(Routine::Gives, &finds, false), 0);
        assert_eq!(search.handed, Handed::Given);
        // Nothing waits for a lid here.
        search.tell(OPENS);
        search.pass(find, true, true);
        assert_eq!((search.reached, search.amount), (OPENS, 1));
        search.tell(FOUND);
        assert_eq!(search.pass(find, true, true), Passed::Found);
        search.tell(SHOWN);
        search.pass(find, true, true);
        assert!(search.searched && !search.drawn && !search.open_to_search);
        // A press at it is still taken, and starts nothing.
        search.pass(find, true, false);
        assert!(search.press());
        assert_eq!(search.pass(find, true, false), Passed::Nothing);
        assert_eq!(search.told, 0);
        // With its flag set the thing starts that way.
        let later = Search::new(find, true);
        assert!(later.searched && !later.drawn && !later.open_to_search);
    }

    #[test]
    fn a_box_needs_its_thing_and_stays_open() {
        let finds = finds(40, 0xE);
        let find = &finds.find;
        let mut search = Search::new(find, false);
        // Without the thing the lines say so, and the script ends.
        assert_eq!(search.answer(Routine::Needs, &finds, false), 1);
        assert_eq!(search.handed, Handed::Lines(0x7000));
        assert_eq!(search.used, None);
        // With it the thing is used, and state 3 is reached when the lid is open.
        assert_eq!(search.answer(Routine::Needs, &finds, true), 0);
        assert_eq!(search.used, Some(40));
        assert_eq!(search.answer(Routine::Gives, &finds, true), 0);
        search.tell(OPENS);
        for frame in 1..=OPEN {
            search.pass(find, true, true);
            assert_eq!((search.amount, search.reached), (frame, -1));
        }
        search.pass(find, true, true);
        assert_eq!((search.amount, search.reached), (OPEN, OPENS));
        search.tell(FOUND);
        assert_eq!(search.pass(find, true, true), Passed::Found);
        search.tell(SHOWN);
        search.pass(find, true, true);
        assert!(search.searched && search.drawn && search.open_to_search);
        // Searched again it is empty and gives nothing.
        assert_eq!(search.answer(Routine::Fresh, &finds, true), 0);
        assert_eq!(search.answer(Routine::Before, &finds, true), 0);
        assert_eq!(search.handed, Handed::Lines(0x7100));
        assert_eq!(search.answer(Routine::Gives, &finds, true), 1);
        // The lid lies beside the box's place, turned with the box (a quarter turn
        // here: what is ahead of the box by 74 is at its side), and opens by a tenth of
        // a quarter turn a game frame.
        let lidded = Find {
            meshes: [Some(0), Some(1)],
            ..*find
        };
        let stands = lidded.stands(5);
        assert_eq!(stands[0].1.position, [890, 0, 4481]);
        assert_eq!((stands[1].0, stands[1].1.position), (1, [891, -55, 4407]));
        assert_eq!(stands[1].1.matrix, skeleton::rotation_yxz([0, 1024, 512]));
        assert_eq!(
            lidded.stands(0)[1].1.matrix,
            skeleton::rotation_yxz([0, 1024, 0])
        );
        // With its flag set it starts open.
        let later = Search::new(find, true);
        assert!(later.searched && later.drawn && later.open_to_search);
        assert_eq!(later.amount, OPEN);
    }
}
