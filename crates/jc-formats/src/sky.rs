
use crate::scene::ScenePackage;
use crate::skeleton::{self, Placement};

/// Where a scene package holds the object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Object {
    /// The archive file of the scene package.
    pub file: usize,
    /// The object record that command `0x11` starts there.
    pub record: u32,
    /// Its start routine, which hands on to the routine of every game frame
    /// (`0x800B2F18` in file 280, `0x800B2820` in file 281).
    start_routine: u32,
    /// The three constants the routine copies to its stack, one behind the other: where
    /// the object stands at first, how far it travels (three words each), and its angles.
    constants: u32,
}

/// The two packages with the object: rooms 84 and 85.
pub const OBJECTS: [Object; 2] = [
    Object {
        file: 280,
        record: 0x800B_3DE8,
        start_routine: 0x800B_33EC,
        constants: 0x800B_24BC,
    },
    Object {
        file: 281,
        record: 0x800B_3444,
        start_routine: 0x800B_2B58,
        constants: 0x800B_24A8,
    },
];
/// The type of the record.
const KIND: u32 = 0x0D;
/// The routine does nothing in any other view.
pub const VIEW: usize = 0;
/// The mesh objects of the room's model block that it draws.
pub const MESH_OBJECTS: usize = 3;
/// The step that counts the travel, and the game frames the travel lasts.
const TRAVELS: u32 = 6;
pub const LENGTH: i32 = 120;

/// The constants of the routine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Course {
    pub from: [i32; 3],
    pub travel: [i32; 3],
    pub angles: [i16; 3],
}

impl Course {
    /// The course of the object that the record at `record` of the package in archive
    /// file `file` starts; none for any other record.
    pub fn of(file: usize, package: &ScenePackage, record: u32) -> Option<Self> {
        let object =
            (OBJECTS.iter()).find(|object| object.file == file && object.record == record)?;
        let word = |address: u32| {
            let bytes = package.bytes_at(address, 4)?;
            Some(u32::from_le_bytes(bytes.try_into().expect("4 bytes")))
        };
        if word(record + 4)? != KIND || word(record + 16)? != object.start_routine {
            return None;
        }
        let (from, travel, angles) = (
            object.constants,
            object.constants + 12,
            object.constants + 24,
        );
        let words = |address: u32| -> Option<[i32; 3]> {
            Some([
                word(address)? as i32,
                word(address + 4)? as i32,
                word(address + 8)? as i32,
            ])
        };
        let angles = package.bytes_at(angles, 6)?;
        let angle = |n: usize| i16::from_le_bytes([angles[2 * n], angles[2 * n + 1]]);
        Some(Self {
            from: words(from)?,
            travel: words(travel)?,
            angles: [angle(0), angle(1), angle(2)],
        })
    }

    /// Where the object stands when its count is `count`: the travel's share of
    /// count / 120, divided towards zero.
    pub fn place(&self, count: i32) -> [i32; 3] {
        std::array::from_fn(|k| self.from[k] + self.travel[k] * count / LENGTH)
    }

    pub fn stands(&self, count: i32) -> Placement {
        Placement {
            matrix: skeleton::rotation_yxz(self.angles),
            position: self.place(count),
        }
    }
}

/// The words of the object's task that its routine works on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sky {
    /// The state a script told it (+124 of the task), -1 for none.
    pub told: i32,
    /// The two words of its extra bytes: the step and the count of the travel.
    pub step: u32,
    pub count: i32,
}

impl Default for Sky {
    fn default() -> Self {
        Self::new()
    }
}

impl Sky {
    /// The task as the spawner and the start routine leave it.
    pub fn new() -> Self {
        Self {
            told: -1,
            step: 0,
            count: 0,
        }
    }

    /// `0x2E`: a script tells the object a state.
    pub fn tell(&mut self, state: i32) {
        self.told = state;
    }

    /// One game frame of the routine (`0x800B2F18` in file 280) with the view that is loaded. Answers
    /// whether it draws: it does nothing at all in another view than [`VIEW`]. A told
    /// state of 0 starts step 1; steps 1 to 5 last a game frame each, the step behind
    /// them counts to [`LENGTH`] and stays.
    pub fn pass(&mut self, view: usize) -> bool {
        if view != VIEW {
            return false;
        }
        if self.told == 0 {
            (self.step, self.told) = (1, -1);
        }
        match self.step {
            0 => {}
            step if step < TRAVELS => self.step += 1,
            TRAVELS => self.count = (self.count + 1).clamp(0, LENGTH),
            _ => {}
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_travel_starts_five_game_frames_after_the_word() {
        let mut sky = Sky::new();
        for _ in 0..10 {
            assert!(sky.pass(VIEW));
        }
        assert_eq!((sky.step, sky.count), (0, 0));
        sky.tell(0);
        // The game frame of the word and four more are the steps 1 to 5.
        for _ in 0..5 {
            sky.pass(VIEW);
            assert_eq!(sky.count, 0);
        }
        assert_eq!((sky.told, sky.step), (-1, TRAVELS));
        sky.pass(VIEW);
        assert_eq!(sky.count, 1);
        for _ in 0..200 {
            sky.pass(VIEW);
        }
        assert_eq!(sky.count, LENGTH);
    }

    #[test]
    fn nothing_moves_in_another_view() {
        let mut sky = Sky::new();
        sky.tell(0);
        assert!(!sky.pass(VIEW + 1));
        assert_eq!((sky.told, sky.step), (0, 0));
    }

    #[test]
    fn the_place_is_the_share_of_the_travel_divided_towards_zero() {
        let course = Course {
            from: [10, -10, 0],
            travel: [-7, 7, 240],
            angles: [0; 3],
        };
        assert_eq!(course.place(0), [10, -10, 0]);
        // -7 * 60 / 120 is -3.5: towards zero.
        assert_eq!(course.place(60), [7, -7, 120]);
        assert_eq!(course.place(LENGTH), [3, -3, 240]);
    }
}
