
use crate::leave;
use crate::scene::ScenePackage;
use crate::script::Scene;

const KIND: u32 = 0x0D;
/// Bytes of one entry of table 1 of a room, which has one region of ground for a view.
const ENTRY: u32 = 8;
/// The instructions of the routine that tell it from any other, by their distance from
/// its start: the load of table 1's address, the call of the triangle lookup and the
/// upper half of the word that names a thing. Two more carry the region and the thing.
const LOADS: [(u32, u32); 3] = [
    (0x14, 0x8C44_B49C),
    (0x20, 0x0C02_DE22),
    (0x2C, 0x3C02_4000),
];
const REGION: (u32, u32) = (0x24, 0x2484_0000);
const THING: (u32, u32) = (0x30, 0x3442_0000);

/// What the pad routine hands the call for a talk (`0x800A5BD0`) for a press of the
/// confirm button; no thing of the story has that number, so a press never tells a
/// spot anything.
pub const PRESS: i32 = -1;

/// The object, as its record and its routine give it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spot {
    /// RAM address of the record a script's `0x11` names.
    pub record: u32,
    pub routine: u32,
    /// The region of table 1 the player has to stand on: the ground of this view.
    pub region: usize,
    /// The thing of the story the marked object names (+132 of its task, under the bit
    /// `0x40000000`).
    pub thing: u32,
}

impl Spot {
    /// The object behind the record at `record` of the package, when its routine is the
    /// spot's.
    pub fn at(package: &ScenePackage, record: u32) -> Option<Self> {
        let word = |address: u32| {
            let bytes = package.bytes_at(address, 4)?;
            Some(u32::from_le_bytes(bytes.try_into().expect("4 bytes")))
        };
        if word(record + 4)? != KIND {
            return None;
        }
        let routine = word(record + 16)?;
        for (at, code) in LOADS {
            if word(routine.checked_add(at)?)? != code {
                return None;
            }
        }
        // An instruction with a 16-bit value in its low half.
        let value = |(at, code): (u32, u32)| {
            let found = word(routine + at)?;
            (found & 0xFFFF_0000 == code).then_some(found & 0xFFFF)
        };
        Some(Self {
            record,
            routine,
            region: (value(REGION)? / ENTRY) as usize,
            thing: value(THING)?,
        })
    }

    /// The spots the listed scripts of a package start, each once.
    pub fn of(package: &ScenePackage) -> Vec<Self> {
        let mut spots: Vec<Self> = Vec::new();
        for command in package.script_commands() {
            let Some(Scene::StartObject { record }) = Scene::of(&command) else {
                continue;
            };
            if spots.iter().all(|known| known.record != record) {
                spots.extend(Self::at(package, record));
            }
        }
        spots
    }
}

/// The words of the object's task that its routine and the wrapper of the task work on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Marked {
    /// The state a script or the confirm button told it (+124), -1 for none.
    pub told: i32,
    /// The state it has reached (+128), which a script's `0x2F` waits for.
    pub reached: i32,
    /// The mark (+120): the player stands on the spot.
    pub marked: bool,
    /// The task has ended (told state -2).
    pub ended: bool,
}

impl Default for Marked {
    fn default() -> Self {
        Self::new()
    }
}

impl Marked {
    /// The task as its maker leaves it.
    pub fn new() -> Self {
        Self {
            told: -1,
            reached: -1,
            marked: false,
            ended: false,
        }
    }

    /// A script tells the object a state (`0x2E`).
    pub fn tell(&mut self, state: i32) {
        self.told = state;
    }

    /// Whether the object waits for `n` (`0x800A5E10` of the menu for a thing of the
    /// story): it is marked, and its word +132 carries the bit `0x40000000` with `thing`
    /// in the rest, so it is `n` alone that it answers.
    pub fn waits_for(&self, thing: u32, n: i32) -> bool {
        self.marked && u32::try_from(n).is_ok_and(|n| n == thing)
    }

    /// The second half of the call for a talk (`0x800A5BD0(n)`): the pad's press hands
    /// it [`PRESS`], the menu's use of a thing of the story the thing. The object is told
    /// state 0 when it waits for `n`. Answers whether it was.
    pub fn call(&mut self, thing: u32, n: i32) -> bool {
        let told = self.waits_for(thing, n);
        if told {
            self.told = 0;
        }
        told
    }

    /// One game frame of the task: the wrapper of an object task, which takes the mark
    /// away, then the routine. `stands`: the triangle lookup finds the player's place on
    /// the region.
    pub fn pass(&mut self, stands: bool) {
        self.marked = false;
        if self.ended {
            return;
        }
        if !leave::runs(self.told) {
            self.ended = self.told == leave::ENDS;
            return;
        }
        self.marked = stands;
        // Told state 0 is taken and answered for one game frame; any other is left.
        self.reached = match self.told {
            0 => {
                self.told = -1;
                0
            }
            _ => -1,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_use_of_its_thing_on_the_spot_is_answered_for_one_game_frame() {
        let mut task = Marked::new();
        // Off the spot nothing is marked and the thing tells nothing.
        task.pass(false);
        assert!(!task.waits_for(7, 7) && !task.call(7, 7));
        assert_eq!((task.told, task.reached), (-1, -1));
        // On it a press tells nothing, nor another thing.
        task.pass(true);
        assert!(!task.call(7, PRESS) && !task.call(7, 6));
        assert_eq!(task.told, -1);
        // The thing tells state 0, which the next pass answers once.
        assert!(task.waits_for(7, 7));
        assert!(task.call(7, 7));
        task.pass(true);
        assert_eq!((task.told, task.reached), (-1, 0));
        task.pass(true);
        assert_eq!(task.reached, -1);
    }

    #[test]
    fn a_resting_spot_is_not_marked_and_an_ended_one_stays_ended() {
        let mut task = Marked::new();
        task.tell(leave::RESTS);
        task.pass(true);
        assert!(!task.marked && !task.ended);
        task.tell(-1);
        task.pass(true);
        assert!(task.marked);
        task.tell(leave::ENDS);
        task.pass(true);
        assert!(task.ended && !task.marked);
        task.tell(-1);
        task.pass(true);
        assert!(!task.marked);
    }
}
