
use crate::scene::ScenePackage;

/// The type of the object, the extra bytes its task gets, and the bytes of its record
/// and of one side.
pub const KIND: u32 = 0x0D;
const EXTRA: u32 = 12;
const SIZE: usize = 24;
const SIDE: usize = 24;
/// More sides than this and the table is taken to have no end.
const SIDES_MOST: usize = 16;

/// One side of a lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Side {
    /// The trigger region the player stands in, and the point the player then faces.
    pub region: i32,
    pub point: [i32; 3],
    /// The list of the addresses of the side's lines.
    pub lines: u32,
    /// The door opens from this side.
    pub opens: bool,
}

/// The record of a lock, the data of its object, with its sides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lock {
    /// RAM address of the record.
    pub record: u32,
    /// The story flag that says the lock is done, negative for none.
    pub flag: i32,
    /// The thing of the story that opens it.
    pub thing: i32,
    /// The script for a player who has the thing, on a side that opens, and the one for
    /// every other press.
    pub with_thing: u32,
    pub without: u32,
    /// The record of the door.
    pub door: u32,
    pub sides: Vec<Side>,
}

fn word(bytes: &[u8], at: usize) -> i32 {
    i32::from_le_bytes(bytes[at..at + 4].try_into().expect("4 bytes"))
}

impl Side {
    /// A side in the bytes that start at its address; `None` for the record that ends
    /// the table, whose list of lines is 0.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let bytes = bytes.get(..SIDE)?;
        let lines = word(bytes, 16) as u32;
        (lines != 0).then(|| Self {
            region: word(bytes, 0),
            point: [word(bytes, 4), word(bytes, 8), word(bytes, 12)],
            lines,
            opens: word(bytes, 20) != 0,
        })
    }
}

impl Lock {
    /// The lock in the object lists of a package; none for a package without. The
    /// table of sides, the lines of each side and the two scripts must lie in the
    /// package.
    pub fn of(package: &ScenePackage) -> Option<Self> {
        let lists = package.object_lists();
        (lists.iter().flat_map(|list| &list.objects))
            .filter(|object| object.kind == KIND && object.extra == EXTRA)
            .find_map(|object| Self::read(package, object.data))
    }

    fn read(package: &ScenePackage, record: u32) -> Option<Self> {
        let bytes = package.bytes_at(record, SIZE)?;
        let table = word(bytes, 0) as u32;
        let mut sides = Vec::new();
        loop {
            let at = table.checked_add((sides.len() * SIDE) as u32)?;
            match Side::parse(package.bytes_at(at, SIDE)?) {
                Some(side) if sides.len() < SIDES_MOST => {
                    package.lines_at(side.lines)?;
                    sides.push(side);
                }
                Some(_) => return None,
                None => break,
            }
        }
        let (with_thing, without) = (word(bytes, 12) as u32, word(bytes, 16) as u32);
        package.bytes_at(with_thing, 16)?;
        package.bytes_at(without, 16)?;
        (!sides.is_empty()).then(|| Self {
            record,
            flag: word(bytes, 4),
            thing: word(bytes, 8),
            with_thing,
            without,
            door: word(bytes, 20) as u32,
            sides,
        })
    }
}

/// The script a lock starts, and what it leaves in its task for that script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Started {
    /// The address of the script, and whether it is the one that opens the door.
    pub script: u32,
    pub opens: bool,
    /// The list of the lines of the side the player stands at (+140 of the task).
    pub lines: u32,
}

/// The words of a lock's task that its routine works on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Locking {
    /// The lock is done: no side marks the player any more.
    pub done: bool,
    /// The side the player last stood at.
    pub side: usize,
    /// The state a script or a press told it (+124 of the task), -1 for none.
    pub told: i32,
    /// The mark of the game frame (+120): a press of the confirm button tells the lock
    /// state 0.
    pub marked: bool,
}

impl Locking {
    /// The task as the start routine leaves it: done when the story flag of the record
    /// is set.
    pub fn new(flag_set: bool) -> Self {
        Self {
            done: flag_set,
            side: 0,
            told: -1,
            marked: false,
        }
    }

    /// A script tells the lock a state.
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
    /// lock's routine. `at`: the player stands in a side's region and faces its point.
    /// `script_runs`: the script the lock started last still runs.
    pub fn pass(
        &mut self,
        lock: &Lock,
        at: impl Fn(&Side) -> bool,
        script_runs: bool,
        has_thing: bool,
    ) -> Option<Started> {
        self.marked = false;
        if matches!(self.told, -3 | -2) {
            return None;
        }
        for (n, side) in lock.sides.iter().enumerate() {
            if at(side) && !self.done && !script_runs {
                (self.side, self.marked) = (n, true);
            }
        }
        if self.told != 0 {
            return None;
        }
        self.told = -1;
        let side = lock.sides.get(self.side)?;
        let opens = side.opens && has_thing;
        self.done = opens;
        Some(Started {
            script: if opens { lock.with_thing } else { lock.without },
            opens,
            lines: side.lines,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lock() -> Lock {
        let side = |region, lines, opens| Side {
            region,
            point: [0, 0, 100 * region],
            lines,
            opens,
        };
        Lock {
            record: 0x800B_3000,
            flag: 111,
            thing: 19,
            with_thing: 0x800B_3100,
            without: 0x800B_3200,
            door: 0x800B_3300,
            sides: vec![side(3, 0x800B_3400, false), side(2, 0x800B_3410, true)],
        }
    }

    #[test]
    fn a_side_reads_its_fields_and_the_table_ends_without_lines() {
        let mut bytes = [0u8; SIDE];
        let words: [i32; 6] = [2, 10, 20, 30, 0x800B_3410u32 as i32, 1];
        for (n, word) in words.iter().enumerate() {
            bytes[4 * n..4 * n + 4].copy_from_slice(&word.to_le_bytes());
        }
        let side = Side::parse(&bytes).expect("a side");
        assert_eq!((side.region, side.point), (2, [10, 20, 30]));
        assert_eq!((side.lines, side.opens), (0x800B_3410, true));
        bytes[16..20].fill(0);
        assert_eq!(Side::parse(&bytes), None);
        assert_eq!(Side::parse(&bytes[..SIDE - 1]), None);
    }

    #[test]
    fn the_side_that_opens_starts_the_opening_script_for_a_player_with_the_thing() {
        let lock = lock();
        let mut locking = Locking::new(false);
        let near = |side: &Side| side.region == 2;
        // Away from every side a press tells nothing.
        assert_eq!(locking.pass(&lock, |_| false, false, true), None);
        assert!(!locking.press());
        assert_eq!(locking.pass(&lock, near, false, true), None);
        assert_eq!((locking.side, locking.marked), (1, true));
        assert!(locking.press());
        let started = locking.pass(&lock, near, false, true).expect("a script");
        assert_eq!((started.script, started.opens), (0x800B_3100, true));
        assert_eq!((started.lines, locking.told), (0x800B_3410, -1));
        // The lock is done: no mark from the next game frame on.
        assert!(locking.done);
        assert_eq!(locking.pass(&lock, near, true, true), None);
        assert!(!locking.marked && !locking.press());
    }

    #[test]
    fn every_other_press_starts_the_script_of_the_lines() {
        let lock = lock();
        // The near side without the thing.
        let mut locking = Locking::new(false);
        let near = |side: &Side| side.region == 2;
        locking.pass(&lock, near, false, false);
        assert!(locking.press());
        let started = locking.pass(&lock, near, false, false).expect("a script");
        assert_eq!((started.script, started.opens), (0x800B_3200, false));
        assert_eq!(started.lines, 0x800B_3410);
        // While that script runs there is no mark; behind it there is one again.
        assert!(!locking.done);
        locking.pass(&lock, near, true, false);
        assert!(!locking.marked);
        locking.pass(&lock, near, false, false);
        assert!(locking.marked);
        // The far side with the thing: its own lines, and the door stays shut.
        let mut locking = Locking::new(false);
        let far = |side: &Side| side.region == 3;
        locking.pass(&lock, far, false, true);
        assert!(locking.press());
        let started = locking.pass(&lock, far, false, true).expect("a script");
        assert_eq!((started.script, started.opens), (0x800B_3200, false));
        assert_eq!((started.lines, locking.done), (0x800B_3400, false));
        // A lock whose flag is set marks nobody.
        let mut locking = Locking::new(true);
        locking.pass(&lock, near, false, true);
        assert!(!locking.marked);
    }
}
