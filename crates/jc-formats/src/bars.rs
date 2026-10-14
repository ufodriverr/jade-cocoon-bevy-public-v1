
use crate::scene::ScenePackage;

/// The type of the object, the extra bytes its task gets, and the bytes of its record.
pub const KIND: u32 = 0x0D;
const EXTRA: u32 = 4;
const SIZE: usize = 20;
/// Words of the start routine: its call of the step, which is the routine behind it, and
/// the clearing of the count in the call's delay slot.
const CALL: u32 = 0x10;
const STEP: u32 = 0x34;
const CLEARS_COUNT: u32 = 0xAE00_0014;
/// The first two loads of the step: the told state and the record.
const LOADS: [(u32, u32); 2] = [(0x0C, 0x8C83_007C), (0x10, 0x8C87_0010)];
/// The states a script tells the object.
pub const SHUT: i32 = 0;
pub const OPEN: i32 = 1;
pub const CLOSE: i32 = 2;
pub const SLIDE_OPEN: i32 = 3;
/// What the object leaves in the told state when it has taken one.
const NOTHING: i32 = -1;

/// A rectangle of the screen: x, y, width and height.
pub type Rect = (i32, i32, i32, i32);

/// The record of the object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bars {
    /// The part of the screen the bars leave free when they are shut.
    pub inside: Rect,
    /// The part of the screen the bars are drawn in.
    pub outside: Rect,
    /// The game frames a slide takes: the count of a shut pair.
    pub frames: i32,
}

impl Bars {
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let bytes: &[u8; SIZE] = bytes.get(..SIZE)?.try_into().ok()?;
        let half = |at: usize| i32::from(i16::from_le_bytes([bytes[at], bytes[at + 1]]));
        let rect = |at: usize| (half(at), half(at + 2), half(at + 4), half(at + 6));
        Some(Self {
            inside: rect(0),
            outside: rect(8),
            frames: i32::from_le_bytes(bytes[16..20].try_into().expect("4 bytes")),
        })
    }

    /// The object in the lists of a package, with the address of its record and the list
    /// it is in; none for a package without. The object of a player from the place map
    /// has the same type and extra bytes (`crate::gate`): this one is known by its start
    /// routine, which clears the count and calls the step behind itself, by the step's
    /// first loads and by a record with a count above zero.
    pub fn of(package: &ScenePackage) -> Option<(usize, u32, Self)> {
        let word = |address: u32| {
            let bytes = package.bytes_at(address, 4)?;
            Some(u32::from_le_bytes(bytes.try_into().expect("4 bytes")))
        };
        let is_start = |start: u32| {
            let step = start.checked_add(STEP)?;
            let call = 0x0C00_0000 | (step & 0x0FFF_FFFF) >> 2;
            let fits = word(start + CALL)? == call
                && word(start + CALL + 4)? == CLEARS_COUNT
                && (LOADS.iter()).all(|&(at, load)| word(step + at) == Some(load));
            fits.then_some(())
        };
        for (list, objects) in package.object_lists().iter().enumerate() {
            for object in (objects.objects.iter())
                .filter(|object| object.kind == KIND && object.extra == EXTRA)
            {
                let found = is_start(object.function)
                    .and_then(|()| package.bytes_at(object.data, SIZE))
                    .and_then(Self::parse)
                    .filter(|bars| bars.frames > 0);
                if let Some(bars) = found {
                    return Some((list, object.data, bars));
                }
            }
        }
        None
    }

    /// The two bars at a count (`0x800B2B50` of file 284): the upper one grows down from
    /// the top of the outside to the top of the inside, the lower one up from the bottom
    /// of the outside to the bottom of the inside, both by the count's share of the
    /// record's game frames, each division towards zero.
    pub fn rects(&self, count: i32) -> [Rect; 2] {
        let (_, top, _, high) = self.inside;
        let (x, y, width, height) = self.outside;
        let frames = self.frames.max(1);
        let lower = (height - top - high) * count / frames;
        let from = y + height - lower;
        [
            (x, y, width, (top - y) * count / frames),
            (x, from, width, height - from),
        ]
    }
}

/// The words of the object's task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sliding {
    /// The state a script has told and the object has not taken yet.
    pub told: i32,
    /// How far the bars are shut: 0 not at all, the record's game frames wholly.
    pub count: i32,
}

impl Default for Sliding {
    fn default() -> Self {
        Self::new()
    }
}

impl Sliding {
    /// The task as the start routine leaves it: open, nothing told.
    pub fn new() -> Self {
        Self {
            told: NOTHING,
            count: 0,
        }
    }

    /// A script tells the object a state (`0x2E`).
    pub fn tell(&mut self, state: i32) {
        self.told = state;
    }

    /// One game frame of the object (`0x800B2A38`): the told state is taken, and the bars
    /// it then draws are answered. Shut and open at once forget the state in the same
    /// game frame; a slide moves the count by one and forgets the state at its end.
    pub fn pass(&mut self, bars: &Bars) -> [Rect; 2] {
        match self.told {
            SHUT => (self.count, self.told) = (bars.frames, NOTHING),
            OPEN => (self.count, self.told) = (0, NOTHING),
            CLOSE => {
                self.count = Self::held(self.count + 1, bars.frames);
                if self.count == bars.frames {
                    self.told = NOTHING;
                }
            }
            SLIDE_OPEN => {
                self.count = Self::held(self.count - 1, bars.frames);
                if self.count == 0 {
                    self.told = NOTHING;
                }
            }
            _ => {}
        }
        bars.rects(self.count)
    }

    /// A count as the routine keeps it: 0 for one below, the record's for one above.
    fn held(count: i32, frames: i32) -> i32 {
        match count {
            count if count < 0 => 0,
            count if frames < count => frames,
            count => count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The record of room 88: the picture of 160 lines from line 32, in 30 game frames.
    const ROOM: Bars = Bars {
        inside: (0, 32, 320, 160),
        outside: (0, 0, 320, 240),
        frames: 30,
    };

    #[test]
    fn a_record_reads_as_two_rectangles_and_a_count() {
        let mut bytes = Vec::new();
        for half in [0i16, 32, 320, 160, 0, 0, 320, 240] {
            bytes.extend(half.to_le_bytes());
        }
        bytes.extend(30i32.to_le_bytes());
        assert_eq!(Bars::parse(&bytes), Some(ROOM));
        assert_eq!(Bars::parse(&bytes[..19]), None);
    }

    #[test]
    fn the_bars_grow_with_the_count() {
        assert_eq!(ROOM.rects(0), [(0, 0, 320, 0), (0, 240, 320, 0)]);
        assert_eq!(ROOM.rects(30), [(0, 0, 320, 32), (0, 192, 320, 48)]);
        // Half way: 16 lines above and 24 below.
        assert_eq!(ROOM.rects(15), [(0, 0, 320, 16), (0, 216, 320, 24)]);
        // One game frame: 32 / 30 and 48 / 30 are one line each.
        assert_eq!(ROOM.rects(1), [(0, 0, 320, 1), (0, 239, 320, 1)]);
    }

    #[test]
    fn shut_and_open_are_taken_at_once() {
        let mut task = Sliding::new();
        assert_eq!(task.pass(&ROOM), ROOM.rects(0));
        task.tell(SHUT);
        assert_eq!(task.pass(&ROOM), ROOM.rects(30));
        assert_eq!((task.told, task.count), (NOTHING, 30));
        // Nothing told: the bars stay.
        assert_eq!(task.pass(&ROOM), ROOM.rects(30));
        task.tell(OPEN);
        assert_eq!(task.pass(&ROOM), ROOM.rects(0));
        assert_eq!((task.told, task.count), (NOTHING, 0));
    }

    #[test]
    fn a_slide_takes_the_records_game_frames() {
        let mut task = Sliding::new();
        task.tell(SHUT);
        task.pass(&ROOM);
        task.tell(SLIDE_OPEN);
        for count in (1..30).rev() {
            assert_eq!(task.pass(&ROOM), ROOM.rects(count));
            assert_eq!(task.told, SLIDE_OPEN);
        }
        assert_eq!(task.pass(&ROOM), ROOM.rects(0));
        assert_eq!(task.told, NOTHING);
        // And back: shut in thirty game frames, never past the record's count.
        task.tell(CLOSE);
        for _ in 0..29 {
            task.pass(&ROOM);
        }
        assert_eq!((task.told, task.count), (CLOSE, 29));
        task.pass(&ROOM);
        assert_eq!((task.told, task.count), (NOTHING, 30));
        // A slide open of an open pair ends in its first game frame.
        let mut open = Sliding::new();
        open.tell(SLIDE_OPEN);
        open.pass(&ROOM);
        assert_eq!((open.told, open.count), (NOTHING, 0));
    }
}
