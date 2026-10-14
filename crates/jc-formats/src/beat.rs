
use crate::scene::ScenePackage;

/// The archive file of the scene package that holds the object.
pub const PACKAGE_FILE: usize = 197;
/// The record a script's `0x11` names, its type and its start routine, which hands on
/// to `0x800B32B8`.
pub const RECORD: u32 = 0x800B_4FB0;
const KIND: u32 = 0x0D;
const START_ROUTINE: u32 = 0x800B_326C;
/// The sound of both beats (`0x800314F0`).
pub const SOUND: u32 = 53;
/// The longest and the shortest period, in game frames.
const SLOWEST: i32 = 30;
const FASTEST: i32 = 15;
/// The count of the object from which the period grows again.
const TURNS_AT: i32 = 900;
/// The game frames between two changes of the period, before and behind that count.
const QUICKENS_EVERY: i32 = 29;
const SLOWS_EVERY: i32 = 63;

/// Whether the record at `record` of the package in archive file `file` is the
/// object's.
pub fn is_at(file: usize, package: &ScenePackage, record: u32) -> bool {
    let word = |address: u32| {
        let bytes = package.bytes_at(address, 4)?;
        Some(u32::from_le_bytes(bytes.try_into().expect("4 bytes")))
    };
    file == PACKAGE_FILE
        && record == RECORD
        && word(record + 4) == Some(KIND)
        && word(record + 16) == Some(START_ROUTINE)
}

/// The five words of the object's extra bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Beat {
    /// The game frames the routine has run.
    pub count: i32,
    /// The game frames since the period last changed.
    since: i32,
    pub period: i32,
    /// The game frames to the first and to the second beat of a period.
    first: i32,
    second: i32,
}

impl Default for Beat {
    fn default() -> Self {
        Self::new()
    }
}

impl Beat {
    /// The words as the start routine sets them.
    pub fn new() -> Self {
        Self {
            count: 0,
            since: 0,
            period: SLOWEST,
            first: 0,
            second: SLOWEST,
        }
    }

    /// One game frame of the routine `0x800B32B8`. Answers whether the first beat falls
    /// into it (with which the routine also asks for the pad's motors, `0x800365A4(0,
    /// 12)`) and whether the second does; each is one sound [`SOUND`].
    pub fn pass(&mut self) -> (bool, bool) {
        let first = self.first == 0;
        if first {
            self.first = self.period;
            self.second = self.period / 3;
        }
        let second = self.second == 0;
        if self.count < TURNS_AT {
            if self.since >= QUICKENS_EVERY {
                if self.period > FASTEST {
                    self.period -= 1;
                }
                self.since = 0;
            }
        } else if self.since >= SLOWS_EVERY {
            if self.period < SLOWEST {
                self.period += 1;
            }
            self.since = 0;
        }
        self.count += 1;
        self.since += 1;
        self.first -= 1;
        self.second -= 1;
        (first, second)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The game frames, counted from 0, in which each beat falls over `frames` passes.
    fn beats(frames: i32) -> (Vec<i32>, Vec<i32>, Beat) {
        let mut beat = Beat::new();
        let (mut firsts, mut seconds) = (Vec::new(), Vec::new());
        for frame in 0..frames {
            let (first, second) = beat.pass();
            firsts.extend(first.then_some(frame));
            seconds.extend(second.then_some(frame));
        }
        (firsts, seconds, beat)
    }

    #[test]
    fn two_beats_a_period_the_second_a_third_behind_the_first() {
        let (firsts, seconds, _) = beats(100);
        // The period is 30 at the first beat, 29 at the second and 28 at the third.
        assert_eq!(firsts, [0, 30, 59, 87]);
        assert_eq!(seconds, [10, 39, 68, 96]);
    }

    #[test]
    fn the_period_shortens_to_fifteen_and_grows_back_from_count_nine_hundred() {
        // One less every 29 game frames, first in the pass of count 29.
        let (_, _, beat) = beats(29);
        assert_eq!(beat.period, 30);
        let (_, _, beat) = beats(30);
        assert_eq!(beat.period, 29);
        let (_, _, beat) = beats(29 * 15 + 1);
        assert_eq!(beat.period, 15);
        let (firsts, seconds, beat) = beats(900);
        assert_eq!(beat.period, 15);
        // At the shortest period a beat every 15 game frames, the second 5 behind.
        let late: Vec<i32> = firsts.iter().rev().take(3).copied().collect();
        assert_eq!(late[0] - late[1], 15);
        assert_eq!(late[1] - late[2], 15);
        let behind = seconds.iter().find(|&&at| at > late[1]).unwrap();
        assert_eq!(behind - late[1], 5);
        // From count 900 on one more every 63 game frames, up to 30 and no further.
        let (_, _, beat) = beats(900 + 63 * 15 + 63);
        assert_eq!(beat.period, 30);
        let (_, _, beat) = beats(900 + 63 * 40);
        assert_eq!(beat.period, 30);
    }
}
