
use crate::scene::{PACKAGE_BASE, ScenePackage};

const PLAYER_PLACES: u32 = 0x800A_9484;
/// The getter of an actor of the room by its number.
const ACTOR_GETTER: u32 = 0x800A_79F0;
/// The two instructions that load the pool of the object tasks, `0x8008D390`.
const POOL: [u32; 2] = [0x3C02_8009, 0x8C44_D390];
/// `addiu a3,zero,17`: the object type the common mover looks for.
const MARKER_TYPE: u32 = 0x2407_0011;
/// Where an actor keeps its table of 12 bytes a bone.
const ACTOR_TABLE: u32 = 452;
/// Where the filled places keep the player's table of 12 bytes a bone, from the stack
/// pointer of a mover.
const PLAYER_TABLE: u32 = 24;
/// Bytes of a bone's entry in such a table.
const ENTRY: u32 = 12;
/// `subu v0,zero,v0`: the height turned over.
const NEGATES: u32 = 0x0002_1023;
const RETURN: u32 = 0x03E0_0008;
/// The longest mover of the disc has 78 instructions.
const LONGEST: usize = 128;

/// Which bone of the player a mover takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bone {
    Always(usize),
    /// By a word of the package that a native routine of the scripts flips
    /// ([`flips`]): one bone while it is 0, another while it is not.
    ByWord {
        word: u32,
        clear: usize,
        set: usize,
    },
}

/// What a mover puts its thing on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShowMover {
    /// The place and the upright angle of the room's object of type `0x11`, nothing
    /// while the room has none that runs.
    Marker,
    /// A bone of the player: his place and the bone's entry of his table. `mirrored`
    /// turns the height over, which puts the thing as far under the level 0 as the
    /// bone is over it.
    Player { bone: Bone, mirrored: bool },
    /// A bone of the actor of that number in the room's list.
    Actor { actor: usize, bone: usize },
}

/// What a mover gives in a game frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Moved {
    pub place: [i32; 3],
    pub angles: [i16; 3],
}

impl Moved {
    /// The place and the angles of a record's task: the record's own added.
    pub fn with(&self, place: [i32; 3], angles: [i16; 3]) -> ([i32; 3], [i16; 3]) {
        (
            std::array::from_fn(|k| self.place[k].wrapping_add(place[k])),
            std::array::from_fn(|k| self.angles[k].wrapping_add(angles[k])),
        )
    }
}

impl ShowMover {
    /// The bone of the player the mover takes with the word of [`Bone::ByWord`] as
    /// `set` says; none for a mover that takes no bone of his.
    pub fn player_bone(&self, set: impl Fn(u32) -> bool) -> Option<usize> {
        match *self {
            ShowMover::Player { bone, .. } => Some(match bone {
                Bone::Always(bone) => bone,
                Bone::ByWord {
                    word,
                    clear,
                    set: other,
                } => match set(word) {
                    true => other,
                    false => clear,
                },
            }),
            _ => None,
        }
    }

    /// What the mover gives for a bone that stands at `at` in the room.
    pub fn on_bone(&self, at: [i32; 3]) -> Moved {
        let mirrored = matches!(self, ShowMover::Player { mirrored: true, .. });
        let place = match mirrored {
            true => [at[0], at[1].wrapping_neg(), at[2]],
            false => at,
        };
        Moved {
            place,
            angles: [0; 3],
        }
    }
}

/// The instructions of the routine at `routine` up to its return.
fn code(package: &ScenePackage, routine: u32) -> Option<Vec<u32>> {
    let mut words = Vec::new();
    loop {
        let at = routine.checked_add(4 * words.len() as u32)?;
        let bytes = package.bytes_at(at, 4)?;
        let word = u32::from_le_bytes(bytes.try_into().ok()?);
        words.push(word);
        if word == RETURN {
            return Some(words);
        }
        if words.len() == LONGEST {
            return None;
        }
    }
}

fn jal(target: u32) -> u32 {
    0x0C00_0000 | (target >> 2 & 0x03FF_FFFF)
}

/// The parts of a load `lw rt,imm(rs)`: (rs, rt, imm).
fn load(word: u32) -> Option<(u32, u32, u32)> {
    (word >> 26 == 0x23).then_some((word >> 21 & 31, word >> 16 & 31, word & 0xFFFF))
}

/// The bone of the first load behind `from` whose base is register `base`.
fn entry(words: &[u32], from: usize, base: u32) -> Option<usize> {
    let (.., offset) = (words.get(from..)?.iter())
        .filter_map(|word| load(*word))
        .find(|(rs, ..)| *rs == base)?;
    (offset % ENTRY == 0).then_some((offset / ENTRY) as usize)
}

/// The number `addiu a0,zero,N` loads, 0 for `addu a0,zero,zero`.
fn number(word: u32) -> Option<usize> {
    match word {
        0x0000_2021 => Some(0),
        word if word >> 16 == 0x2404 && word & 0x8000 == 0 => Some((word & 0xFFFF) as usize),
        _ => None,
    }
}

/// What the routine at `routine` of the package puts a shown thing on; none for a
/// routine that is none of the disc's movers.
pub fn read(package: &ScenePackage, routine: u32) -> Option<ShowMover> {
    let words = code(package, routine)?;
    let call = |target: u32| words.iter().position(|word| *word == jal(target));
    if let Some(at) = call(ACTOR_GETTER) {
        let actor = number(*words.get(at + 1)?)?;
        // The actor's table goes into a register, and the first load from that
        // register names the bone.
        let (table, register) = (words.iter().enumerate()).find_map(|(at, word)| {
            let (rs, rt, offset) = load(*word)?;
            (rs == 2 && offset == ACTOR_TABLE).then_some((at, rt))
        })?;
        let bone = entry(&words, table + 1, register)?;
        return Some(ShowMover::Actor { actor, bone });
    }
    if let Some(at) = call(PLAYER_PLACES) {
        let mirrored = words.contains(&NEGATES);
        // A word of the package picks the bone: `lui`, `lw v0`, and the two numbers
        // behind it, the first in the delay slot of the branch.
        let by_word = words.get(at + 2..).and_then(|rest| {
            let (upper, lower) = (*rest.first()?, *rest.get(1)?);
            let (rs, rt, offset) = load(lower)?;
            if upper >> 26 != 0x0F || upper >> 16 & 31 != rs || rt != 2 {
                return None;
            }
            let word = ((upper & 0xFFFF) << 16).wrapping_add(offset as u16 as i16 as u32);
            let mut numbers = rest.iter().filter(|word| *word >> 16 == 0x2404);
            let (clear, set) = (number(*numbers.next()?)?, number(*numbers.next()?)?);
            (word >= PACKAGE_BASE).then_some(Bone::ByWord { word, clear, set })
        });
        let bone = match by_word {
            Some(bone) => bone,
            None => {
                let (table, register) = (words.iter().enumerate()).find_map(|(at, word)| {
                    let (rs, rt, offset) = load(*word)?;
                    (rs == 29 && offset == PLAYER_TABLE).then_some((at, rt))
                })?;
                Bone::Always(entry(&words, table + 1, register)?)
            }
        };
        return Some(ShowMover::Player { bone, mirrored });
    }
    let pool = words.windows(2).any(|pair| pair == POOL);
    (pool && words.contains(&MARKER_TYPE)).then_some(ShowMover::Marker)
}

pub fn flips(package: &ScenePackage, routine: u32) -> Option<u32> {
    let bytes = package.bytes_at(routine, 4 * FLIPS.len())?;
    let words: Vec<u32> = (bytes.as_chunks::<4>().0.iter())
        .map(|word| u32::from_le_bytes(*word))
        .collect();
    let fits = (FLIPS.iter().zip(&words)).all(|(&(shape, open), &word)| match open {
        true => word & 0xFFFF_0000 == shape,
        false => word == shape,
    });
    // `lui v1,HI`, then `lw v0,LO(v1)` and two stores to the same place.
    let (upper, lower) = (words[0] & 0xFFFF, words[1] & 0xFFFF);
    let one_place = [words[6], words[7]]
        .iter()
        .all(|word| word & 0xFFFF == lower);
    (fits && one_place).then(|| (upper << 16).wrapping_add(lower as u16 as i16 as u32))
}

/// The routine of [`flips`], each instruction with whether its lower half, a part of
/// the word's address, is left open.
const FLIPS: [(u32, bool); 10] = [
    (0x3C03_0000, true),
    (0x8C62_0000, true),
    (0, false),
    (0x1040_0003, false),
    (0x2402_0001, false),
    (RETURN, false),
    (0xAC60_0000, true),
    (0xAC62_0000, true),
    (RETURN, false),
    (0x2402_0001, false),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// A package whose code image is these instructions, the first at the first address
    /// of a package's code.
    fn package_of(words: &[u32]) -> Vec<u8> {
        let mut file = (4 * words.len() as u32).to_le_bytes().to_vec();
        file.extend(words.iter().flat_map(|word| word.to_le_bytes()));
        file
    }

    const AT: u32 = PACKAGE_BASE + 4;

    #[test]
    fn a_mover_on_a_bone_of_an_actor() {
        // The getter with 0, the table at +452 of what it answers, entry 2 of it.
        let file = package_of(&[
            jal(ACTOR_GETTER),
            0x0000_2021,
            0x8C43_01C4,
            0x8C63_0018,
            RETURN,
            0,
        ]);
        let package = ScenePackage::parse(&file).expect("package");
        let mover = read(&package, AT).expect("a mover");
        assert_eq!(mover, ShowMover::Actor { actor: 0, bone: 2 });
        assert_eq!(mover.on_bone([1, 2, 3]).place, [1, 2, 3]);
        assert_eq!(mover.player_bone(|_| false), None);
    }

    #[test]
    fn a_mover_on_a_bone_of_the_player() {
        // The filler, the table from the stack into v1, entry 7 of it, the height
        // turned over.
        let file = package_of(&[
            jal(PLAYER_PLACES),
            0x27A4_0010,
            0x8FA2_0014,
            0x8FA3_0018,
            0x8C45_0000,
            0x8C64_0054,
            NEGATES,
            RETURN,
            0,
        ]);
        let package = ScenePackage::parse(&file).expect("package");
        let mover = read(&package, AT).expect("a mover");
        let bone = Bone::Always(7);
        assert_eq!(
            mover,
            ShowMover::Player {
                bone,
                mirrored: true
            }
        );
        assert_eq!(mover.player_bone(|_| true), Some(7));
        assert_eq!(mover.on_bone([10, 20, 30]).place, [10, -20, 30]);
    }

    #[test]
    fn a_mover_whose_bone_a_word_picks_and_the_routine_that_flips_it() {
        let word = PACKAGE_BASE + 0x1B7C;
        let (upper, lower) = (word >> 16, word & 0xFFFF);
        let file = package_of(&[
            jal(PLAYER_PLACES),
            0x27A4_0010,
            0x3C02_0000 | upper,
            0x8C42_0000 | lower,
            0,
            0x1040_0002,
            0x2404_0001,
            0x2404_0007,
            RETURN,
            0,
            // The routine that flips the word.
            0x3C03_0000 | upper,
            0x8C62_0000 | lower,
            0,
            0x1040_0003,
            0x2402_0001,
            RETURN,
            0xAC60_0000 | lower,
            0xAC62_0000 | lower,
            RETURN,
            0x2402_0001,
        ]);
        let package = ScenePackage::parse(&file).expect("package");
        let mover = read(&package, AT).expect("a mover");
        let bone = Bone::ByWord {
            word,
            clear: 1,
            set: 7,
        };
        assert_eq!(
            mover,
            ShowMover::Player {
                bone,
                mirrored: false
            }
        );
        assert_eq!(mover.player_bone(|_| false), Some(1));
        assert_eq!(mover.player_bone(|at| at == word), Some(7));
        assert_eq!(flips(&package, AT + 40), Some(word));
        assert_eq!(flips(&package, AT), None);
    }

    #[test]
    fn the_common_mover_and_a_routine_that_is_none() {
        let file = package_of(&[
            POOL[0],
            POOL[1],
            MARKER_TYPE,
            RETURN,
            0,
            0x2402_0001,
            RETURN,
            0,
        ]);
        let package = ScenePackage::parse(&file).expect("package");
        assert_eq!(read(&package, AT), Some(ShowMover::Marker));
        assert_eq!(read(&package, AT + 20), None);
    }

    #[test]
    fn the_records_own_place_and_angles_are_added() {
        let moved = Moved {
            place: [100, -50, 7],
            angles: [0, 1024, 0],
        };
        let (place, angles) = moved.with([1, 2, 3], [10, 20, 30]);
        assert_eq!((place, angles), ([101, -48, 10], [10, 1044, 30]));
    }
}
