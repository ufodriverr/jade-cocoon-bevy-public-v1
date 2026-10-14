
use crate::scene::ScenePackage;

/// The records a script's `0x11` names, each with the archive file of its scene package.
pub const RECORDS: [(usize, u32); 2] = [(469, 0x800B_3860), (470, 0x800B_4564)];
const KIND: u32 = 0x0D;
/// The sound (`0x800314F0`).
pub const SOUND: u32 = 31;
/// The game frames from one sound to the next.
pub const EVERY: i32 = 45;
/// The two instructions of the step routine that load the sound and the pace, by their
/// distance from the start routine, which hands on to the step routine behind it.
const LOADS: [(u32, u32); 2] = [
    (0x58, 0x2404_0000 | SOUND),
    (0x5C, 0x2402_0000 | EVERY as u32),
];
/// The state the wrapper of an object's task ends it at.
pub const ENDS: i32 = -2;

/// Whether the record at `record` of the package is the object's: its type, and the two
/// instructions of the routine its start routine hands on to.
pub fn is_at(package: &ScenePackage, record: u32) -> bool {
    let word = |address: u32| {
        let bytes = package.bytes_at(address, 4)?;
        Some(u32::from_le_bytes(bytes.try_into().expect("4 bytes")))
    };
    let Some(start) = word(record + 16) else {
        return false;
    };
    word(record + 4) == Some(KIND)
        && LOADS
            .iter()
            .all(|&(at, code)| word(start + at) == Some(code))
}
/// The one word of the object's extra bytes: the game frames to the next sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Toll {
    count: i32,
}

impl Toll {
    /// The word as the start routine sets it.
    pub fn new() -> Self {
        Self::default()
    }

    /// One game frame of the routine `0x800B2B28`. Answers whether the sound falls into
    /// it.
    pub fn pass(&mut self) -> bool {
        let sounds = self.count == 0;
        if sounds {
            self.count = EVERY;
        }
        self.count -= 1;
        sounds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sound_in_the_first_game_frame_and_every_45_behind_it() {
        let mut toll = Toll::new();
        let sounds: Vec<i32> = (0..200).filter(|_| toll.pass()).collect();
        assert_eq!(sounds, [0, 45, 90, 135, 180]);
    }
}
