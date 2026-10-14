
use crate::scene::ScenePackage;

/// Where every model file but the player's keeps its palette of 256 colours, in slot 0 of
/// a loader: the place the copy is read from, moved down by the rows of the model's slot.
pub const SOURCE: (usize, usize) = (768, 385);
/// Where the copy of the first actor of a room goes; the n-th actor's lies n rows below.
pub const COPY: (usize, usize) = (768, 394);
pub const FIGURE: usize = 3;
/// The colours of the row.
pub const COLOURS: usize = 256;
/// A share that is the whole.
const WHOLE: i32 = 4096;
/// Bit of a colour that the mix leaves as it is (the blend mark).
const MARK: u16 = 0x8000;
/// Bytes of a record.
const SIZE: usize = 12;

/// The type of the object that tints actors, and the extra bytes its task gets.
pub const KIND: u32 = 0x0D;
const EXTRA: u32 = 0;
/// The things the object's loop goes through: bit n of its data for thing n, from 1.
const THINGS: std::ops::Range<u32> = 1..5;
/// How many words of the object's routine are looked through for its two calls.
const ROUTINE_WORDS: u32 = 40;
/// The calls of the routine: the getter of thing n's actor and the setter of the tint.
const JAL_ACTOR: u32 = 0x0C02_9E7C;
const JAL_TINT: u32 = 0x0C00_76C5;

/// One record of the executable's table of tints (`0x80079FF8`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tint {
    /// The colour the mix goes towards: red, green and blue, 0 to 31.
    pub colour: [i16; 3],
    /// How much of a colour becomes the mean of its three parts, out of 4096.
    pub grey: i16,
    /// How much of the result becomes [`Self::colour`], out of 4096.
    pub share: i16,
}

impl Tint {
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let bytes: &[u8; SIZE] = bytes.get(..SIZE)?.try_into().ok()?;
        let half = |at: usize| i16::from_le_bytes([bytes[at], bytes[at + 1]]);
        Some(Self {
            colour: [half(0), half(2), half(4)],
            grey: half(6),
            share: half(8),
        })
    }

    /// The record at a RAM address of an image whose first byte is at `base`.
    pub fn at(image: &[u8], base: u32, address: u32) -> Option<Self> {
        let start = address.checked_sub(base)? as usize;
        Self::parse(image.get(start..)?)
    }

    /// One colour of the copy (`0x80029338`, the body of its loop). Black is left as it
    /// is, mark and all. Every other colour is taken apart, each part mixed with the mean
    /// of the three and then with the record's colour, each product divided by 4096
    /// towards zero, and put together again with its mark.
    pub fn colour(&self, colour: u16) -> u16 {
        let parts = [colour & 31, colour >> 5 & 31, colour >> 10 & 31].map(i32::from);
        if parts == [0; 3] {
            return colour;
        }
        let mean = parts.iter().sum::<i32>() / 3;
        let (grey, share) = (i32::from(self.grey), i32::from(self.share));
        let mixed: [i32; 3] = std::array::from_fn(|part| {
            let greyed = (parts[part] * (WHOLE - grey) + mean * grey) / WHOLE;
            greyed * (WHOLE - share) / WHOLE + i32::from(self.colour[part]) * share / WHOLE
        });
        colour & MARK | mixed[0] as u16 | (mixed[1] as u16) << 5 | (mixed[2] as u16) << 10
    }

    /// A whole row of the copy.
    pub fn row(&self, row: &[u16]) -> Vec<u16> {
        row.iter().map(|&colour| self.colour(colour)).collect()
    }
}

pub const TABLE: u32 = 0x8007_9FC8;
pub const RECORDS: usize = 17;
/// Where the player's palette lies and where the copy of it goes (`0x8007DBA4`).
pub const PLAYER_SOURCE: (usize, usize) = (768, 256);
pub const PLAYER_COPY: (usize, usize) = (768, 393);
/// The status bits that pick a tint: poison, and the six steps of turning to stone.
const POISON: u32 = 0x0001;
const STONE: [u32; 7] = [0x0008, 0x0010, 0x0020, 0x0040, 0x0080, 0x0100, 0x0200];
/// The record of poison alone; poison with a step of stone follows it.
const POISONED: usize = 7;

/// The records of [`TABLE`], read from an image whose first byte is at `base`.
pub fn table(image: &[u8], base: u32) -> Option<Vec<Tint>> {
    (0..RECORDS as u32)
        .map(|n| Tint::at(image, base, TABLE + n * SIZE as u32))
        .collect()
}

/// The record of [`TABLE`] a status word asks for (`0x80018EBC`, `0x800AC09C`): the
/// highest step of stone that is set, from 0, and 8 more with poison; poison alone 7;
/// none without either, where the setter puts the actor back in its own colours.
pub fn of_status(status: u32) -> Option<usize> {
    let poisoned = status & POISON != 0;
    match (STONE.iter()).rposition(|bit| status & bit != 0) {
        Some(step) if poisoned => Some(POISONED + 1 + step),
        Some(step) => Some(step),
        None => poisoned.then_some(POISONED),
    }
}

/// The rows between the palette of a model in a slot whose palette of 256 colours lies
/// `slot_rows` below its file's place and the copy of the room's actor `actor` (0 for the
/// first): what the actor's maker leaves in the two places of the actor (`0x8001E5DC`)
/// and the maker of the drawn word takes the difference of.
pub fn rows(actor: usize, slot_rows: i16) -> i16 {
    (COPY.1 + actor) as i16 - (SOURCE.1 as i16 + slot_rows)
}

/// An object of a room that tints actors when the room starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tinted {
    /// The object list the object is in, by its place in `ScenePackage::object_lists`.
    pub list: usize,
    /// Bit n is thing n of the room, from 1: the things that are tinted.
    pub mask: u32,
    /// RAM address of the record the routine hands the setter.
    pub record: u32,
}

impl Tinted {
    /// The things of the room the object tints, by their number.
    pub fn things(&self) -> impl Iterator<Item = u32> + '_ {
        THINGS.filter(|thing| self.mask & 1 << thing != 0)
    }

    /// The objects of a package that tint actors: of type `0x0D` without extra bytes, a
    /// mask of things as data, and a routine that calls the getter of a thing's actor and
    /// the setter of the tint, whose second argument is made of an upper half in some
    /// register and a lower half in the call's delay slot.
    pub fn of(package: &ScenePackage) -> Vec<Self> {
        let word = |address: u32| {
            let bytes = package.bytes_at(address, 4)?;
            Some(u32::from_le_bytes(bytes.try_into().expect("4 bytes")))
        };
        let record_of = |routine: u32| {
            let words: Vec<u32> = (0..ROUTINE_WORDS)
                .map_while(|n| word(routine.checked_add(4 * n)?))
                .collect();
            let call = words.iter().position(|&word| word == JAL_TINT)?;
            words[..call].iter().find(|&&word| word == JAL_ACTOR)?;
            // `addiu a1, register, lower` behind the call, `lui register, upper` before.
            let lower = *words.get(call + 1)?;
            if lower >> 26 != 0x09 || lower >> 16 & 31 != 5 {
                return None;
            }
            let register = lower >> 21 & 31;
            let upper = (words[..call].iter().rev())
                .find(|&&word| word >> 26 == 0x0F && word >> 16 & 31 == register)?;
            Some((upper << 16).wrapping_add(lower as u16 as i16 as u32))
        };
        let mut found = Vec::new();
        for (list, objects) in package.object_lists().iter().enumerate() {
            for object in (objects.objects.iter())
                .filter(|object| object.kind == KIND && object.extra == EXTRA)
                .filter(|object| object.data != 0 && object.data < 0x100)
            {
                if let Some(record) = record_of(object.function) {
                    found.push(Self {
                        list,
                        mask: object.data,
                        record,
                    });
                }
            }
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn colour(red: u16, green: u16, blue: u16) -> u16 {
        red | green << 5 | blue << 10
    }

    /// The first record of the table: towards white, 2925 of 4096 both times.
    const PALE: Tint = Tint {
        colour: [31, 31, 31],
        grey: 2925,
        share: 2925,
    };

    #[test]
    fn a_status_picks_the_record_of_its_highest_step_and_of_its_poison() {
        assert_eq!(of_status(0), None);
        assert_eq!(of_status(0x0002), None);
        assert_eq!(of_status(0x0001), Some(7));
        assert_eq!(of_status(0x0008), Some(0));
        assert_eq!(of_status(0x0200), Some(6));
        assert_eq!(of_status(0x0009), Some(8));
        assert_eq!(of_status(0x0201), Some(14));
        // The highest step wins, as the tests run from 0x0200 down.
        assert_eq!(of_status(0x0018), Some(1));
        assert_eq!(of_status(0x8000 | 0x0040), Some(3));
    }

    #[test]
    fn a_record_reads_as_five_values() {
        let bytes = [31, 0, 30, 0, 29, 0, 0x6D, 0x0B, 0x00, 0x08, 0, 0];
        assert_eq!(
            Tint::parse(&bytes),
            Some(Tint {
                colour: [31, 30, 29],
                grey: 2925,
                share: 2048,
            })
        );
        assert_eq!(Tint::parse(&bytes[..11]), None);
        assert_eq!(
            Tint::at(&bytes, 0x8007_9FF8, 0x8007_9FF8).map(|t| t.grey),
            Some(2925)
        );
        assert_eq!(Tint::at(&bytes, 0x8007_9FF8, 0x8007_9FF4), None);
    }

    #[test]
    fn black_stays_and_the_mark_stays() {
        assert_eq!(PALE.colour(0), 0);
        assert_eq!(PALE.colour(MARK), MARK);
        assert_eq!(PALE.colour(MARK | colour(31, 0, 0)) & MARK, MARK);
    }

    #[test]
    fn a_colour_goes_to_its_mean_and_then_to_the_records() {
        // Red: the mean is 10. 31 * 1171 + 10 * 2925 = 65551, 16 of 4096ths; 0 * 1171 +
        // 29250 = 7. Then 16 * 1171 / 4096 = 4 and 31 * 2925 / 4096 = 22: 26; 7 gives 2
        // and 22: 24.
        assert_eq!(PALE.colour(colour(31, 0, 0)), colour(26, 24, 24));
        // White stays white but for the rounding: 31 * 1171 / 4096 = 8, and 22.
        assert_eq!(PALE.colour(colour(31, 31, 31)), colour(30, 30, 30));
        // A whole share leaves the record's colour, whatever the colour was.
        let green = Tint {
            colour: [0, 31, 0],
            grey: 0,
            share: 4096,
        };
        assert_eq!(green.colour(colour(9, 1, 20)), colour(0, 31, 0));
        // No share and no grey leave the colour.
        let none = Tint {
            colour: [31, 31, 31],
            grey: 0,
            share: 0,
        };
        assert_eq!(none.colour(colour(9, 1, 20)), colour(9, 1, 20));
        assert_eq!(PALE.row(&[0, colour(31, 0, 0)]), [0, colour(26, 24, 24)]);
    }

    #[test]
    fn the_rows_lead_from_the_slot_to_the_actor() {
        // The first actor with a model in slot 0: from row 385 to row 394.
        assert_eq!(rows(0, 0), 9);
        // The second actor, its model in slot 1 (two rows down): from 387 to 395.
        assert_eq!(rows(1, 2), 8);
        assert_eq!(rows(3, 6), 6);
    }

    #[test]
    fn the_things_of_a_mask() {
        let tinted = |mask| Tinted {
            list: 0,
            mask,
            record: 0,
        };
        assert_eq!(tinted(2).things().collect::<Vec<_>>(), [1]);
        assert_eq!(tinted(6).things().collect::<Vec<_>>(), [1, 2]);
        // Bit 0 is never asked, and nothing above thing 4.
        assert_eq!(tinted(0x21).things().count(), 0);
    }
}
