
use crate::scene::ScenePackage;

/// The archive file of the scene package that holds the object.
pub const PACKAGE_FILE: usize = 197;
/// The type of the object and its start routine, which hands on to `0x800B2D30`.
const KIND: u32 = 0x0D;
const START_ROUTINE: u32 = 0x800B_3230;
/// The sprite of the room's package that the routine asks for.
pub const SPRITE: u32 = 326;
/// The blend word the routine hands the quad drawer: blended, in mode 1 (added).
pub const BLEND_MODE: u8 = 1;
/// The told state that takes the sprites away; 0 and 1 name a list.
const NONE: i32 = 2;
/// The size of a place record: three words of the place and the game frame it starts in.
const SPOT_SIZE: usize = 16;

/// One place of a list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spot {
    pub place: [i32; 3],
    /// The count of the object at which the sprite starts here.
    pub start: i32,
}

impl Spot {
    /// The frame of a sprite of `frames` frames that is drawn here when the object's
    /// count is `count`: none before the start, then the frames round and round.
    pub fn frame(&self, count: i32, frames: usize) -> Option<usize> {
        let since = usize::try_from(count.checked_sub(self.start)?).ok()?;
        (frames > 0).then(|| since % frames)
    }
}

/// The object's data: two lists of places.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scatter {
    /// The address of the data, by which `0x2E` finds the object.
    pub data: u32,
    pub lists: [Vec<Spot>; 2],
}

impl Scatter {
    /// The object of the package in archive file `file`, when an object list of it holds
    /// one; none for any other package.
    pub fn of(file: usize, package: &ScenePackage) -> Option<Self> {
        if file != PACKAGE_FILE {
            return None;
        }
        let lists = package.object_lists();
        let object = (lists.iter().flat_map(|list| &list.objects))
            .find(|object| object.kind == KIND && object.function == START_ROUTINE)?;
        let word = |address: u32| {
            let bytes = package.bytes_at(address, 4)?;
            Some(i32::from_le_bytes(bytes.try_into().expect("4 bytes")))
        };
        // A list ends at the record whose start is negative.
        let list = |n: u32| -> Option<Vec<Spot>> {
            let mut at = word(object.data + 4 * n)? as u32;
            let mut spots = Vec::new();
            loop {
                let start = word(at + 12)?;
                if start < 0 {
                    return Some(spots);
                }
                spots.push(Spot {
                    place: [word(at)?, word(at + 4)?, word(at + 8)?],
                    start,
                });
                at += SPOT_SIZE as u32;
            }
        };
        Some(Self {
            data: object.data,
            lists: [list(0)?, list(1)?],
        })
    }
}

/// The words of the object's task that its routine works on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Run {
    /// The state a script told it (+124 of the task), -1 for none.
    pub told: i32,
    /// The two words of its extra bytes: the list in use, negative for none, and the
    /// game frames it has been drawn.
    pub list: i32,
    pub count: i32,
}

impl Default for Run {
    fn default() -> Self {
        Self::new()
    }
}

impl Run {
    /// The task as the spawner and the start routine leave it: no list.
    pub fn new() -> Self {
        Self {
            told: -1,
            list: -1,
            count: 0,
        }
    }

    /// `0x2E`: a script tells the object a state.
    pub fn tell(&mut self, state: i32) {
        self.told = state;
    }

    /// One game frame of the routine `0x800B2D30`. A told state of 0 or 1 takes that list
    /// from count 0, a told state of 2 takes the list away; any other stays told and
    /// changes nothing. Answers the list and the count the routine draws with in this
    /// game frame, none without a list; the count goes up behind the drawing.
    pub fn pass(&mut self) -> Option<(usize, i32)> {
        match self.told {
            0 | 1 => (self.list, self.count, self.told) = (self.told, 0, -1),
            NONE => (self.list, self.told) = (-1, -1),
            _ => {}
        }
        let list = usize::try_from(self.list).ok()?;
        let count = self.count;
        self.count += 1;
        Some((list, count))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_told_list_starts_at_count_zero_and_state_two_ends_it() {
        let mut run = Run::new();
        assert_eq!(run.pass(), None);
        run.tell(1);
        assert_eq!(run.pass(), Some((1, 0)));
        assert_eq!(run.pass(), Some((1, 1)));
        assert_eq!(run.told, -1);
        run.tell(0);
        assert_eq!(run.pass(), Some((0, 0)));
        run.tell(2);
        assert_eq!(run.pass(), None);
        assert_eq!((run.told, run.list), (-1, -1));
        // Another state is not taken and does nothing.
        run.tell(3);
        assert_eq!(run.pass(), None);
        assert_eq!(run.told, 3);
    }

    #[test]
    fn a_place_plays_the_frames_round_from_its_start() {
        let spot = Spot {
            place: [0; 3],
            start: 12,
        };
        assert_eq!(spot.frame(11, 120), None);
        assert_eq!(spot.frame(12, 120), Some(0));
        assert_eq!(spot.frame(131, 120), Some(119));
        assert_eq!(spot.frame(132, 120), Some(0));
        assert_eq!(spot.frame(500, 0), None);
    }
}
