
use crate::scene::ScenePackage;
use crate::skeleton::{self, Placement};

/// The type of the object in an object list.
pub const KIND: u32 = 2;
/// The state a script tells the door to open it, and the state the open door reports.
pub const OPEN: i32 = 1;
pub const IS_OPEN: i32 = 2;

/// The bytes of a door record.
const SIZE: usize = 80;
/// Where the two leaves lie in it, and the bytes of one.
const LEAVES: [usize; 2] = [28, 52];

/// One leaf of a door: a mesh object of the room, slid and turned as the door opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Leaf {
    /// The mesh object of the room's model block.
    pub object: usize,
    /// Where the closed leaf stands, and its turn about the upright axis.
    pub place: [i32; 3],
    pub angle: i16,
    /// What the turn has changed by when the door is open, and the x of the open leaf.
    pub turn: i16,
    pub open_x: i32,
}

impl Leaf {
    /// Where the leaf stands when the door's counter is `counter` of `frames`: both
    /// divisions go towards zero, and the angle is kept in 16 bits.
    pub fn stands(&self, counter: i32, frames: i16) -> Placement {
        let share = |way: i32| match frames {
            0 => 0,
            frames => way.wrapping_mul(counter) / i32::from(frames),
        };
        let x = (self.place[0]).wrapping_add(share(self.open_x.wrapping_sub(self.place[0])));
        let angle = (self.angle).wrapping_add(share(i32::from(self.turn)) as i16);
        Placement {
            matrix: skeleton::rotation_yxz([0, angle, 0]),
            position: [x, self.place[1], self.place[2]],
        }
    }
}

/// The record of a door, the data of its object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Door {
    /// The trigger region the player stands in to be at the door, and the point the
    /// player then has to face.
    pub region: i32,
    pub point: [i32; 3],
    /// The pair of 16-bit numbers handed to the ground's switch when the door is open.
    pub ground: [i16; 2],
    /// The story flag that says the door is open, negative for none: set when the door
    /// opens, and a door whose flag is set starts open.
    pub flag: i32,
    /// A number the routine asks the player's things with when the player looks at the
    /// door, negative for none.
    pub thing: i32,
    pub leaves: Vec<Leaf>,
    /// The game frames the door takes to open, and the sound it makes.
    pub frames: i16,
    pub sound: i16,
}

impl Door {
    /// The record in the bytes that start at its address.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let bytes = bytes.get(..SIZE)?;
        let word = |at: usize| i32::from_le_bytes(bytes[at..at + 4].try_into().expect("4"));
        let half = |at: usize| i16::from_le_bytes([bytes[at], bytes[at + 1]]);
        let leaf = |at: usize| {
            Some(Leaf {
                object: usize::try_from(word(at)).ok()?,
                place: [word(at + 4), word(at + 8), word(at + 12)],
                angle: half(at + 16),
                turn: half(at + 18),
                open_x: word(at + 20),
            })
        };
        Some(Self {
            region: word(0),
            point: [word(4), word(8), word(12)],
            ground: [half(16), half(18)],
            flag: word(20),
            thing: word(24),
            leaves: LEAVES.into_iter().filter_map(leaf).collect(),
            frames: half(76),
            sound: half(78),
        })
    }
}

/// The state that makes the door start its script.
pub const ASKED: i32 = 0;
/// Where the start routine of the object names the door's routine (an upper half, a
/// lower half one word behind, and the store into the task behind that), and where the
/// routine names its three scripts: the upper half they share, and the three lower
/// halves.
const ROUTINE_MOST: u32 = 64;
const SCRIPTS_UPPER: u32 = 192;
const SCRIPTS_LOWER: [u32; 3] = [200, 224, 232];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scripts {
    /// For a door without a thing of its own.
    pub plain: u32,
    /// For a door with a thing: the player has it, or has it not.
    pub with_thing: u32,
    pub without: u32,
}

impl Scripts {
    /// The scripts of the door whose object has the start routine `function`: the
    /// routine is the one the start routine stores into its task, and the addresses
    /// are built in it. `None` when the code there is another one.
    pub fn of(package: &ScenePackage, function: u32) -> Option<Self> {
        let word = |address: u32| {
            let bytes = package.bytes_at(address, 4)?;
            Some(u32::from_le_bytes(bytes.try_into().expect("4 bytes")))
        };
        let address =
            |upper: u32, lower: u32| (upper << 16).wrapping_add(lower as u16 as i16 as u32);
        // `lui v0`, `addiu v0, v0`, `sw v0, 144(..)`.
        let routine = (0..ROUTINE_MOST).find_map(|n| {
            let at = function + 4 * n;
            let (upper, lower, store) = (word(at)?, word(at + 4)?, word(at + 8)?);
            (upper >> 16 == 0x3C02 && lower >> 16 == 0x2442 && store & 0xFC1F_FFFF == 0xAC02_0090)
                .then(|| address(upper, lower))
        })?;
        let upper = word(routine + SCRIPTS_UPPER)?;
        if upper >> 16 != 0x3C02 {
            return None;
        }
        let script = |at: u32| {
            let lower = word(routine + at)?;
            let script = address(upper, lower);
            // `addiu a0, v0`, and a script of the package.
            (lower >> 16 == 0x2444 && package.bytes_at(script, 16).is_some()).then_some(script)
        };
        Some(Self {
            plain: script(SCRIPTS_LOWER[0])?,
            with_thing: script(SCRIPTS_LOWER[1])?,
            without: script(SCRIPTS_LOWER[2])?,
        })
    }

    /// The script of a door: by whether it has a thing and whether the player has it.
    pub fn pick(&self, door: &Door, has_thing: impl Fn(i32) -> bool) -> u32 {
        match door.thing {
            thing if thing < 0 => self.plain,
            thing if has_thing(thing) => self.with_thing,
            _ => self.without,
        }
    }
}

/// What a pass of the door's routine did besides moving the leaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Passed {
    /// The door was told state 0 in this pass: it starts its script.
    pub asks: bool,
    /// The door was told to open in this pass: its flag is set and its sound plays.
    pub opens: bool,
    /// The door is fully open in this pass: the ground's switch is called.
    pub is_open: bool,
}

/// The part of the object's task that the routine works on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Swing {
    /// The door opens or is open.
    pub opening: bool,
    /// How far it has come, up to the frames of its record.
    pub counter: i32,
    /// The state a script has told and the routine has not taken yet.
    told: Option<i32>,
    /// The state the routine reports, 0 until the door is open.
    pub reached: i32,
    /// For whoever runs the door: it has said once that the door is open.
    pub told_open: bool,
    /// The player stands at the door and looks at it (the mark at +120 of the task).
    pub marked: bool,
}

impl Swing {
    /// The task as the object's maker leaves it: closed, or with `open` (the story flag
    /// of the record is set) fully open.
    pub fn new(door: &Door, open: bool) -> Self {
        Self {
            opening: open,
            counter: if open { i32::from(door.frames) } else { 0 },
            told: None,
            reached: 0,
            told_open: false,
            marked: false,
        }
    }

    /// Step 1 of the routine, in front of [`Swing::pass`]. `at`: the player stands in
    /// the door's region and faces its point; `script_runs`: the script the door
    /// started still runs. A door that opens or is open is not marked.
    pub fn look(&mut self, at: bool, script_runs: bool) {
        self.marked = at && !self.opening && !script_runs;
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
            self.told = Some(ASKED);
        }
        marked
    }

    /// A script tells the door a state (`0x2E`).
    pub fn tell(&mut self, state: i32) {
        self.told = Some(state);
    }

    /// One game frame of the routine `0x800B24B0`: a told state 0 asks for the door's
    /// script, a told state 1 starts the opening,
    /// an opening door counts a game frame, in the same pass, and reports state 2 in
    /// every pass in which its counter stands at the frames of the record.
    pub fn pass(&mut self, door: &Door) -> Passed {
        let mut passed = Passed::default();
        match self.told.take() {
            Some(ASKED) => passed.asks = true,
            Some(OPEN) => {
                self.opening = true;
                passed.opens = true;
            }
            _ => {}
        }
        if self.opening {
            let frames = i32::from(door.frames);
            self.counter = match self.counter.wrapping_add(1) {
                counter if counter < 0 => 0,
                counter => counter.min(frames),
            };
            if self.counter == frames {
                self.reached = IS_OPEN;
                passed.is_open = true;
            }
        }
        passed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> Vec<u8> {
        let mut bytes = vec![0u8; SIZE];
        let mut put = |at: usize, value: i32| {
            bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
        };
        put(0, 3);
        put(8, -250);
        put(16, -1);
        put(20, 7);
        put(24, -1);
        // One leaf: mesh object 0 at (-100, 0, -600), turned by -1000 and slid to -60.
        put(28, 0);
        put(32, -100);
        put(40, -600);
        put(44, (-1000i32 << 16) | 64);
        put(48, -60);
        put(52, -1);
        put(76, (26 << 16) | 16);
        bytes
    }

    #[test]
    fn a_press_tells_a_door_the_player_looks_at_state_0() {
        let door = Door::parse(&record()).expect("a door");
        let mut swing = Swing::new(&door, false);
        // Nobody at the door: a press does nothing, and the question without a press
        // says so and tells nothing.
        swing.look(false, false);
        assert!(!swing.would_press());
        assert!(!swing.press());
        assert!(!swing.pass(&door).asks);
        // At the door while its script runs: not marked.
        swing.look(true, true);
        assert!(!swing.would_press() && !swing.press());
        // At the door: the press tells state 0, and the next pass asks for the script.
        swing.look(true, false);
        assert!(swing.would_press());
        assert_eq!(swing.told, None, "asking tells nothing");
        assert!(swing.press());
        assert!(swing.pass(&door).asks);
        // The script tells it to open: an opening door is not marked again.
        swing.tell(OPEN);
        assert!(swing.pass(&door).opens);
        swing.look(true, false);
        assert!(!swing.press());
    }

    #[test]
    fn a_door_record_is_read_with_the_leaves_it_has() {
        let door = Door::parse(&record()).expect("a door");
        assert_eq!((door.region, door.point), (3, [0, -250, 0]));
        assert_eq!((door.ground, door.flag, door.thing), ([-1, -1], 7, -1));
        assert_eq!((door.frames, door.sound), (16, 26));
        let leaf = Leaf {
            object: 0,
            place: [-100, 0, -600],
            angle: 64,
            turn: -1000,
            open_x: -60,
        };
        assert_eq!(door.leaves, [leaf]);
        assert_eq!(Door::parse(&record()[..SIZE - 1]), None);
        // Half open: half the slide and half the turn, towards zero.
        let stands = leaf.stands(8, 16);
        assert_eq!(stands.position, [-80, 0, -600]);
        assert_eq!(stands.matrix, skeleton::rotation_yxz([0, 64 - 500, 0]));
        assert_eq!(leaf.stands(1, 16).position, [-98, 0, -600]);
        assert_eq!(leaf.stands(16, 16).position, [-60, 0, -600]);
    }

    #[test]
    fn a_door_opens_when_it_is_told_and_reports_when_it_is_open() {
        let door = Door::parse(&record()).expect("a door");
        let mut swing = Swing::new(&door, false);
        assert_eq!(swing.pass(&door), Passed::default());
        assert_eq!((swing.counter, swing.reached), (0, 0));
        // Another state than 1 is taken and does nothing.
        swing.tell(5);
        assert_eq!(swing.pass(&door), Passed::default());
        // Told to open: the first game frame is counted in the same pass.
        swing.tell(OPEN);
        let passed = swing.pass(&door);
        assert_eq!(
            (passed.opens, passed.is_open, swing.counter),
            (true, false, 1)
        );
        for _ in 0..14 {
            assert_eq!(swing.pass(&door), Passed::default());
        }
        assert_eq!((swing.counter, swing.reached), (15, 0));
        let open = Passed {
            is_open: true,
            ..Passed::default()
        };
        assert_eq!(swing.pass(&door), open);
        assert_eq!((swing.counter, swing.reached), (16, IS_OPEN));
        // An open door says so in every pass.
        assert_eq!(swing.pass(&door), open);
        // A door whose flag is set starts open and reports it in its first pass.
        let mut swing = Swing::new(&door, true);
        assert_eq!((swing.counter, swing.reached), (16, 0));
        assert_eq!(swing.pass(&door), open);
        assert_eq!(swing.reached, IS_OPEN);
    }

    #[test]
    fn a_door_told_state_0_asks_for_its_script_once() {
        let door = Door::parse(&record()).expect("a door");
        let mut swing = Swing::new(&door, false);
        swing.tell(ASKED);
        let asks = Passed {
            asks: true,
            ..Passed::default()
        };
        assert_eq!(swing.pass(&door), asks);
        assert_eq!(swing.pass(&door), Passed::default());
        assert_eq!((swing.opening, swing.counter), (false, 0));
        // Which script: by the door's thing and whether the player has it.
        let scripts = Scripts {
            plain: 1,
            with_thing: 2,
            without: 3,
        };
        assert_eq!(scripts.pick(&door, |_| true), 1);
        let door = Door { thing: 5, ..door };
        assert_eq!(scripts.pick(&door, |thing| thing == 5), 2);
        assert_eq!(scripts.pick(&door, |_| false), 3);
    }
}
