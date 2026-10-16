
use crate::leave;
use crate::package::Placement;
use crate::scene::ScenePackage;
use crate::skeleton::sine;

/// The type of the object and the bytes of its record.
pub const KIND: u32 = 0x0C;
const SIZE: usize = 16;
/// The archive file that is the mesh block, and its two mesh objects the routine draws:
/// the first rises and sinks and takes the mixed colour, the second swells around it.
pub const BLOCK: usize = 10;
pub const CORE: usize = 1;
pub const SHELL: usize = 2;
/// The two colours the routine mixes for the corners of the first mesh object: the one
/// of an object nobody stands at, and the one it has reached after [`BRIGHTEST`] game
/// frames with the player at it.
pub const RESTING: [u8; 3] = [128, 64, 32];
pub const MARKED: [u8; 3] = [32, 64, 128];
pub const BRIGHTEST: i32 = 15;
/// What the turn and the count of the wave grow by in a game frame; twice that while
/// the object is marked.
const TURN: i32 = 32;
const WAVE: i32 = 113;
/// The sine of the count is divided by these for the rise of the first mesh object,
/// for its brightness beside the full colour, and (one added to it) for what the second
/// mesh object swells by.
const RISE: i32 = 163;
const GLOW: i32 = 2;
const SWELL: i32 = 10;
const ONE: i32 = 4096;
/// The flag words of the two calls without the fade: lit and blended, the first half
/// and half and the second added; a marked object's first call is added too.
const CORE_FLAGS: u32 = 0x11;
const SHELL_FLAGS: u32 = 0x31;
const ADDED: u32 = 0x20;

/// The record of the object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Card {
    /// RAM address of the record, by which a script names the object (`0x2E`).
    pub record: u32,
    /// The views of the room in which the object is active.
    pub view_mask: u32,
    /// Where it floats.
    pub place: [i32; 3],
    /// A text of the package, which the request for the screen hands on.
    pub text: u32,
}

impl Card {
    /// The place and the text of a record.
    pub fn parse(bytes: &[u8]) -> Option<([i32; 3], u32)> {
        let bytes = bytes.get(..SIZE)?;
        let word = |at: usize| i32::from_le_bytes(bytes[at..at + 4].try_into().expect("4"));
        Some(([word(0), word(4), word(8)], word(12) as u32))
    }

    /// The objects of the memory card in the object lists of a package, each once.
    pub fn of(package: &ScenePackage) -> Vec<Self> {
        let mut cards: Vec<Self> = Vec::new();
        let lists = package.object_lists();
        for object in (lists.iter().flat_map(|list| &list.objects))
            .filter(|object| object.kind == KIND && object.function != 0)
        {
            if cards.iter().any(|known| known.record == object.data) {
                continue;
            }
            let read = package.bytes_at(object.data, SIZE).and_then(Self::parse);
            cards.extend(read.map(|(place, text)| Self {
                record: object.data,
                view_mask: object.view_mask,
                place,
                text,
            }));
        }
        cards
    }
}

/// One of the two mesh objects as the routine draws it in a game frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Drawn {
    /// The mesh object of the block, what is added to the record's place and what the
    /// drawer is given beside it: [`Placement::stands`] with the record's place and no
    /// angles is where it stands.
    pub placement: Placement,
    /// The colour the routine has written to every corner of the mesh object in place
    /// of the stored ones.
    pub colour: Option<[u8; 3]>,
}

/// What one game frame of the task did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Passed {
    /// The routine asked the field for the memory card's screen.
    pub asks: bool,
    /// The two mesh objects in the order of the calls; none when the routine did not
    /// run.
    pub drawn: Option<[Drawn; 2]>,
}

/// The words of the object's task that its routine works on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Floating {
    /// The state a script or the confirm button told it (+124 of the task), -1 for
    /// none.
    pub told: i32,
    /// The three words of the task's extra bytes: the turn about the upright axis, the
    /// count of the wave and the brightness between the two colours.
    pub turn: i32,
    pub count: i32,
    pub brightness: i32,
    /// The mark (+120): the player stands at the object, so a press of the confirm
    /// button is meant for it.
    pub marked: bool,
    /// The start routine has run.
    pub started: bool,
    /// The task has ended (told state -2).
    pub ended: bool,
}

impl Default for Floating {
    fn default() -> Self {
        Self::new()
    }
}

impl Floating {
    /// The task as the maker leaves it, before its start routine.
    pub fn new() -> Self {
        Self {
            told: -1,
            turn: 0,
            count: 0,
            brightness: 0,
            marked: false,
            started: false,
            ended: false,
        }
    }

    /// A script tells the object a state (`0x2E`).
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

    /// The start routine: the three words are cleared and the routine runs once, not
    /// through the wrapper. `at` as for [`Floating::pass`].
    pub fn start(&mut self, at: bool) -> Passed {
        (self.turn, self.count, self.brightness) = (0, 0, 0);
        self.started = true;
        self.run(at)
    }

    /// One game frame of the task: the wrapper of an object task (the mark is cleared,
    /// a told -3 or -2 keeps the routine from running and -2 ends the task), then the
    /// routine. `at`: the player is near the record's place and faces it.
    pub fn pass(&mut self, at: bool) -> Passed {
        self.marked = false;
        if self.ended {
            return Passed::default();
        }
        if !leave::runs(self.told) {
            self.ended = self.told == leave::ENDS;
            return Passed::default();
        }
        self.run(at)
    }

    fn run(&mut self, at: bool) -> Passed {
        if at {
            self.marked = true;
        }
        let asks = self.told == 0;
        if asks {
            self.told = -1;
        }
        let drawn = self.drawn();
        let way = if self.marked { 1 } else { -1 };
        self.brightness = (self.brightness + way).clamp(0, BRIGHTEST);
        let faster = if self.marked { 2 } else { 1 };
        self.turn = self.turn.wrapping_add(TURN * faster);
        self.count = self.count.wrapping_add(WAVE * faster);
        Passed {
            asks,
            drawn: Some(drawn),
        }
    }

    /// The two calls of the mesh drawer with the words as they are.
    fn drawn(&self) -> [Drawn; 2] {
        let wave = sine(self.count);
        let colour = std::array::from_fn(|part| {
            let (from, to) = (i32::from(RESTING[part]), i32::from(MARKED[part]));
            (from + (to - from) * self.brightness / BRIGHTEST) as u8
        });
        let angles = [0, self.turn as i16, 0];
        let added = if self.marked { ADDED } else { 0 };
        let swell = ONE + (wave + ONE) / SWELL;
        [
            Drawn {
                placement: Placement {
                    object: Some(CORE),
                    brightness: (ONE + wave / GLOW) as i16,
                    place: [0, wave / RISE, 0],
                    angles,
                    scale: [ONE; 3],
                    flags: CORE_FLAGS | added,
                },
                colour: Some(colour),
            },
            Drawn {
                placement: Placement {
                    object: Some(SHELL),
                    brightness: ONE as i16,
                    place: [0; 3],
                    angles,
                    scale: [swell; 3],
                    flags: SHELL_FLAGS,
                },
                colour: None,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_reads_its_place_and_its_text() {
        let mut bytes = Vec::new();
        for word in [-272i32, 0, -128, 0x800B_2494u32 as i32] {
            bytes.extend(word.to_le_bytes());
        }
        assert_eq!(Card::parse(&bytes), Some(([-272, 0, -128], 0x800B_2494)));
        assert_eq!(Card::parse(&bytes[..15]), None);
    }

    #[test]
    fn an_object_nobody_stands_at_turns_and_keeps_its_first_colour() {
        let mut floating = Floating::new();
        let [core, shell] = floating.start(false).drawn.expect("the start draws");
        // The count is zero: no rise, the full colour, a tenth more than its size.
        assert_eq!(core.placement.object, Some(CORE));
        assert_eq!(core.placement.place, [0, 0, 0]);
        assert_eq!(core.placement.brightness, 4096);
        assert_eq!(core.placement.flags, 0x11);
        assert_eq!(core.placement.drawn_flags(), 0x91);
        assert_eq!(core.placement.scale, [4096; 3]);
        assert_eq!(core.colour, Some(RESTING));
        assert_eq!(shell.placement.object, Some(SHELL));
        assert_eq!(shell.placement.scale, [4096 + 409; 3]);
        assert_eq!(shell.placement.flags, 0x31);
        assert_eq!(shell.colour, None);
        for _ in 0..9 {
            floating.pass(false);
        }
        assert_eq!((floating.turn, floating.count), (320, 1130));
        assert_eq!(floating.brightness, 0);
        let [core, shell] = floating.pass(false).drawn.expect("drawn");
        assert_eq!(core.placement.angles, [0, 320, 0]);
        assert_eq!(shell.placement.angles, [0, 320, 0]);
        assert!(!floating.marked);
    }

    #[test]
    fn the_wave_raises_brightens_and_swells() {
        let mut floating = Floating::new();
        floating.start(false);
        // A quarter of the wave: the sine is one.
        floating.count = 1024;
        let [core, shell] = floating.pass(false).drawn.expect("drawn");
        assert_eq!(core.placement.place, [0, 25, 0]);
        assert_eq!(core.placement.brightness, 6144);
        assert_eq!(core.placement.fade(), -2048);
        assert_eq!(core.placement.drawn_flags(), 0xF800_0091);
        assert_eq!(shell.placement.scale, [4096 + 819; 3]);
        assert_eq!(shell.placement.fade(), 0);
        // Three quarters: minus one.
        floating.count = 3072;
        let [core, shell] = floating.pass(false).drawn.expect("drawn");
        assert_eq!(core.placement.place, [0, -25, 0]);
        assert_eq!(core.placement.brightness, 2048);
        assert_eq!(core.placement.drawn_flags(), 0x0800_0091);
        assert_eq!(shell.placement.scale, [4096; 3]);
        // The divisions go towards zero.
        floating.count = 2048 + 26;
        assert_eq!(sine(floating.count), -163);
        let [core, _] = floating.pass(false).drawn.expect("drawn");
        assert_eq!(core.placement.place, [0, -1, 0]);
        assert_eq!(core.placement.brightness, 4096 - 81);
    }

    #[test]
    fn a_marked_object_turns_twice_as_fast_and_changes_its_colour() {
        let mut floating = Floating::new();
        floating.start(false);
        let (turn, count) = (floating.turn, floating.count);
        let [core, _] = floating.pass(true).drawn.expect("drawn");
        assert!(floating.marked);
        // Drawn with the brightness before the step, and added to the picture.
        assert_eq!(core.colour, Some(RESTING));
        assert_eq!(core.placement.flags, 0x31);
        assert_eq!((floating.turn, floating.count), (turn + 64, count + 226));
        assert_eq!(floating.brightness, 1);
        let [core, _] = floating.pass(true).drawn.expect("drawn");
        // One fifteenth of the way, towards zero: 128 - 6, 64, 32 + 6.
        assert_eq!(core.colour, Some([122, 64, 38]));
        for _ in 0..20 {
            floating.pass(true);
        }
        assert_eq!(floating.brightness, BRIGHTEST);
        let [core, _] = floating.pass(true).drawn.expect("drawn");
        assert_eq!(core.colour, Some(MARKED));
        // The player goes away: the mark is cleared and the colour comes back.
        let [core, _] = floating.pass(false).drawn.expect("drawn");
        assert!(!floating.marked);
        assert_eq!(core.placement.flags, 0x11);
        assert_eq!(floating.brightness, BRIGHTEST - 1);
        for _ in 0..20 {
            floating.pass(false);
        }
        assert_eq!(floating.brightness, 0);
    }

    #[test]
    fn a_press_asks_for_the_screen_once_and_only_of_a_marked_object() {
        let mut floating = Floating::new();
        floating.start(false);
        assert!(!floating.press());
        assert!(!floating.pass(true).asks);
        assert!(floating.press());
        assert_eq!(floating.told, 0);
        assert!(floating.pass(true).asks);
        assert_eq!(floating.told, -1);
        assert!(!floating.pass(true).asks);
    }

    #[test]
    fn a_told_object_rests_or_ends() {
        let mut floating = Floating::new();
        floating.start(false);
        floating.pass(true);
        let words = (floating.turn, floating.count, floating.brightness);
        floating.tell(leave::RESTS);
        let passed = floating.pass(true);
        assert_eq!(passed, Passed::default());
        assert!(!floating.marked && !floating.press());
        assert_eq!((floating.turn, floating.count, floating.brightness), words);
        floating.tell(-1);
        assert!(floating.pass(false).drawn.is_some());
        floating.tell(leave::ENDS);
        assert!(floating.pass(true).drawn.is_none());
        assert!(floating.ended);
        // An ended task does not come back.
        floating.tell(-1);
        assert!(floating.pass(true).drawn.is_none());
    }
}
