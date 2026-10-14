
use crate::battle::record::{Draws, Growth, Record};
use crate::battle::{EARTH, FIRE, Move, Tables, WATER, WIND};
use crate::creatures::{Creature, LINE, Line};

/// The minion skins: thing n of the player's kind 4 gives the look of species
/// 201 + n (the first byte of the rows at `0x8007CA70`; the row behind them, which
/// a merge without a skin asks for, has 255 for "none").
pub const SKINS: usize = 10;
pub const SKINS_AT: u32 = 0x8007_CA70;
const FIRST_SKIN: u8 = 201;
/// One level of an experience word as a number.
const ONE_LEVEL: f64 = 67_108_864.0;
/// The levels the way back from a worth counts at most.
const MOST_LEVELS: u32 = 99;
/// The highest growth and the highest stat.
const HIGHEST_GROWTH: i32 = 200;
const HIGHEST_STAT: u8 = 100;
/// A strength of this or more shows as one of the creature's elements.
const SHOWN: u8 = 8;
/// The strength of the one element of a creature that has no other.
const PURE: u8 = 32;
/// The four spells only a creature of one pure element keeps, with that element.
const PURE_SPELLS: [(usize, usize); 4] = [(21, FIRE), (22, WATER), (23, EARTH), (24, WIND)];
/// The spells a merge looks through.
const SPELLS: usize = 30;
/// The two species a line of the right families makes, each with the base creatures
/// that must stand in the line three times or more, as themselves or as one of
/// their four element variants. The first is asked first.
const NEW_SPECIES: [(u32, &[usize]); 2] = [(34, &[30, 14, 15]), (35, &[3, 7, 15, 9])];
const OFTEN: usize = 3;

/// The look a minion skin gives (`0x8001CDC0`), `None` for a thing that is none.
pub fn skin_look(thing: usize) -> Option<u8> {
    (thing < SKINS).then(|| FIRST_SKIN + thing as u8)
}

/// What a level and an experience word are worth (`0x80012358`): every level costs
/// twice the one before it. The console counts this in double precision, and so
/// does this. A level of 0 is worth one level whatever its word.
fn worth(level: u8, word: u32) -> f64 {
    let (mut total, mut scale) = (ONE_LEVEL, 1.0);
    for _ in 0..level {
        total += scale * ONE_LEVEL;
        scale += scale;
    }
    if scale != 1.0 {
        total += f64::from(word as i32) * scale;
    }
    total
}

/// The way back (`0x8001247C`): the level a worth pays for in the bits 26 and up, and
/// below them what is left over, in the measure of that level.
fn packed(mut worth: f64) -> u32 {
    worth -= ONE_LEVEL;
    let (mut scale, mut level) = (1.0, 0u32);
    while level < MOST_LEVELS {
        let step = scale * ONE_LEVEL;
        if step > worth {
            break;
        }
        worth -= step;
        scale += scale;
        level += 1;
    }
    (f64::from((level << 26) as i32) + worth / scale) as i32 as u32
}

/// The growth of a stat (`0x800BA63C`): the mean of the two, and a seventh of what
/// they have together above or below 200.
fn growth(first: u8, second: u8) -> u8 {
    let sum = i32::from(first) + i32::from(second);
    ((sum >> 1) + (sum - HIGHEST_GROWTH) / 7).clamp(0, HIGHEST_GROWTH) as u8
}

/// The four element strengths: the mean of the two, cut. With two or four odd
/// means the stronger half of them is raised by one and the weaker lowered, so that
/// nothing of the sum is lost; with one or three they stay.
fn strengths(first: [u8; 4], second: [u8; 4]) -> [u8; 4] {
    let mut mean: [u8; 4] =
        std::array::from_fn(|n| ((u16::from(first[n]) + u16::from(second[n])) >> 1) as u8);
    let odd = mean.iter().filter(|&&strength| strength & 1 != 0).count();
    if odd == 2 || odd == 4 {
        let mut order = [0, 1, 2, 3];
        order.sort_by_key(|&element| std::cmp::Reverse(mean[element]));
        let mut left = odd;
        for element in order {
            if mean[element] & 1 != 0 {
                mean[element] = if left > odd / 2 {
                    mean[element].wrapping_add(1)
                } else {
                    mean[element].wrapping_sub(1)
                };
                left -= 1;
            }
        }
    }
    mean
}

/// The element a body attack of the numbers 2 to 5 belongs to; the others have none.
fn element_of(attack: u8) -> Option<usize> {
    match attack {
        2 => Some(EARTH),
        3 => Some(WATER),
        4 => Some(FIRE),
        5 => Some(WIND),
        _ => None,
    }
}

/// The five attack slots (`0x800BA8FC`). A slot one of the two knows is known. Its
/// attack is the first creature's, unless that is of an element the merged creature
/// does not show; an attack of the second that is of an element it shows takes its
/// place, and a special one of the second (6 and up) only an empty place. The grade
/// is the second's when that has one.
fn moves(first: &[Move; 5], second: &[Move; 5], bits: u16) -> [Move; 5] {
    let shown = |attack: u8| element_of(attack).is_none_or(|element| bits >> element & 1 != 0);
    std::array::from_fn(|slot| {
        let (own, other) = (first[slot], second[slot]);
        if !own.known && !other.known {
            return Move::default();
        }
        let mut attack = if shown(own.attack) { own.attack } else { 0 };
        match element_of(other.attack) {
            Some(_) if shown(other.attack) => attack = other.attack,
            None if other.attack >= 6 && attack == 0 => attack = other.attack,
            _ => {}
        }
        if attack == 1 {
            attack = 0;
        }
        let (grade, flag) = match other.grade {
            1 => (1, 0),
            2..=4 => (other.grade, 1),
            _ => (own.grade, own.flag),
        };
        Move {
            known: true,
            attack,
            grade,
            flag,
        }
    })
}

/// The species a line of 48 makes of the creature that has it (`0x800BAEA0`), if
/// any: each base creature of one of the two lists must stand in it three times or
/// more.
pub fn new_species(line: &[u8; LINE]) -> Option<u32> {
    let often = |base: usize| {
        let of_it = |&&species: &&u8| Tables::base_of(species.into()) == Some(base);
        line.iter().filter(of_it).count() >= OFTEN
    };
    (NEW_SPECIES.iter())
        .find(|(_, bases)| bases.iter().all(|&base| often(base)))
        .map(|&(species, _)| species)
}

/// The line of the merged creature (`0x800BAD08`): of its 48 species 32 are the
/// first's (every second one, and eight of those between) and 16 the second's (every
/// third one); the two merged are its parents, by their names and element bits as
/// they were, and the parents of each the four behind them.
fn line(first: &Creature, second: &Creature) -> Line {
    let (own, other) = (&first.line, &second.line);
    Line {
        look: other.look,
        names: [
            first.record.name,
            second.record.name,
            own.names[0],
            own.names[1],
            other.names[0],
            other.names[1],
        ],
        species: std::array::from_fn(|n| match n {
            0..24 => own.species[2 * n],
            24..32 => own.species[6 * n - 143],
            _ => other.species[3 * n - 96],
        }),
        signs: [
            first.record.element_bit as u8,
            second.record.element_bit as u8,
            own.signs[0],
            own.signs[1],
            other.signs[0],
            other.signs[1],
        ],
    }
}

/// The creature a merge makes of `first`, the one picked first, and `second`
/// (`0x800BA198`), with the look of a minion skin if one is taken. It has the state
/// of the first. A line that makes a new species asks the set-up for a creature of
/// that species one level up, which takes six draws; nothing else draws. `None` when
/// the tables lack a row the merge reads.
pub fn merged(
    first: &Creature,
    second: &Creature,
    skin: Option<u8>,
    tables: &Tables,
    draws: &mut impl Draws,
) -> Option<Creature> {
    let (a, b) = (&first.record, &second.record);
    let mut line = line(first, second);
    if let Some(look) = skin {
        line.look = look;
    }
    let mut record = a.clone();

    record.strengths = strengths(a.strengths, b.strengths);
    let bits = (record.strengths.iter().enumerate())
        .filter(|&(_, &strength)| strength >= SHOWN)
        .fold(0, |bits, (element, _)| bits | 1 << element);
    record.element_bit = bits;

    // The stats lose what the old growth gave them and get what the new one gives.
    record.growth = std::array::from_fn(|n| growth(a.growth[n], b.growth[n]));
    record.stats = std::array::from_fn(|n| {
        let bare = a.stats[n].wrapping_sub(a.growth[n] / 20);
        (u32::from(bare) + u32::from(record.growth[n] / 20)).min(HIGHEST_STAT.into()) as u8
    });

    // Of two modifiers the lower one counts twice. The four of the elements are then
    // made anew from the strengths, as the set-up makes them for one element.
    record.modifiers = std::array::from_fn(|class| {
        let (mut own, mut other) = (i32::from(a.modifiers[class]), i32::from(b.modifiers[class]));
        if own < other {
            own *= 2;
        } else {
            other *= 2;
        }
        ((own + other) / 3).clamp(i8::MIN.into(), i8::MAX.into()) as i8
    });
    for class in 0..4 {
        let sum: i32 = (record.strengths.iter().zip(&tables.relations))
            .map(|(&strength, row)| i32::from(strength) * i32::from(row[class] as i8))
            .sum();
        record.modifiers[class] = (sum / 32) as i8;
    }

    // What both have learnt, as levels that wait above the first's own.
    let both = packed(worth(a.level, a.experience) + worth(b.level, b.experience));
    record.experience = both.wrapping_sub(u32::from(a.level) << 26);
    record.grow(tables, Growth::Merge)?;

    record.moves = moves(&a.moves, &b.moves, bits);
    record.spells = a.spells | b.spells;
    for spell in 0..SPELLS {
        let element = tables.spells.get(spell)?.element;
        if !(0..16).contains(&element) || bits >> element & 1 == 0 {
            record.spells &= !(1 << spell);
        }
    }
    for (spell, element) in PURE_SPELLS {
        if record.strengths[element] != PURE {
            record.spells &= !(1 << spell);
        }
    }
    record.hp = record.max_hp;
    record.mp = record.max_mp;

    if let Some(species) = new_species(&line.species) {
        let word = (u32::from(record.level) << 26).wrapping_add(record.experience);
        let made = Record::make(tables, species, word, draws)?;
        return Some(Creature::of_a_caught(first.state, made));
    }
    Some(Creature {
        state: first.state,
        record,
        line,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::battle::record::LEVEL;
    use crate::battle::record::tests::tables as made_up;
    use crate::battle::{Descriptor, Spell};
    use crate::creatures::state;

    /// The made-up tables of the record's tests, with thirty spells whose elements
    /// are the ones the vectors were run with (`re/t255f/run.py`).
    fn tables() -> Tables {
        let mut tables = made_up();
        tables.spells = (0..SPELLS as i16)
            .map(|spell| Spell {
                word: 0,
                cost: 0,
                shown: 0,
                need: 0,
                element: (spell * 7 + 1) % 4,
                effect: Descriptor::parse(&[0; 12]).expect("a descriptor"),
            })
            .collect();
        tables
    }

    /// A creature from the hexadecimal digits of a printed record of 248 bytes.
    fn creature(digits: &str) -> Creature {
        let b: Vec<u8> = (0..digits.len() / 2)
            .map(|n| u8::from_str_radix(&digits[2 * n..2 * n + 2], 16).expect("two digits"))
            .collect();
        assert_eq!(b.len(), 248);
        let name = |at: usize| -> [u8; crate::name::LETTERS] { std::array::from_fn(|n| b[at + n]) };
        let mut record = Record::head(&b[132..]).expect("a head");
        record.species = b[2].into();
        record.name = name(148);
        record.moves = std::array::from_fn(|slot| Move {
            known: b[222 + 4 * slot] != 0,
            attack: b[223 + 4 * slot],
            grade: b[224 + 4 * slot],
            flag: b[225 + 4 * slot],
        });
        record.spells = u32::from_le_bytes([b[244], b[245], b[246], b[247]]);
        Creature {
            state: i16::from_le_bytes([b[0], b[1]]),
            record,
            line: Line {
                look: b[3],
                names: std::array::from_fn(|n| name(4 + 12 * n)),
                species: std::array::from_fn(|n| b[76 + n]),
                signs: std::array::from_fn(|n| b[124 + n]),
            },
        }
    }

    fn no_draws() -> impl Draws {
        || -> u32 { panic!("a merge that makes no new species draws nothing") }
    }

    /// Records run through the original routine in the interpreter of the research
    /// (`re/t255f/run.py`, with made-up elements of the spells and the relations of
    /// the made-up tables in the console's memory): the thing, the two made-up
    /// records before and the record the merge wrote.
    #[test]
    fn a_merge_is_the_original_routine_s() {
        let vectors = [
            // A minion skin, and both above level 31, where the word of the way back is negative.
            (
                0,
                "0200de4a76e522ebb771fda337ed45532819e33c7830c0add85f2f5b23cb223b\
                 44ce8ca26066ced2bf5747e0b8ed988094b0bbf252be66c0b6deb4efa1c0f8fa\
                 b74a889fa3ab125e4e657b2c42f5e65a707a16e4ec2f50f56120f7071a592a5b\
                 13eae0e2bbc2a66f028a523cd3d598638a4878a3e6265c5033ed7f18f824c834\
                 5440246b00000000a4698d000c003e00220031001a079a8dc033363164940aa5\
                 ffffffff20e04038e0101040002010e01a391a2a331406b38abc24504e411932\
                 223500041f200c0028000000e01b73fe97201eefebae80ea2c13644e75cc0101\
                 050001060401000105010103050001050400dbe63aefc4a8",
                "0200a2b3bb45f1e904366b554289ba649786bfece4336ec520b12bc9df72df74\
                 f85862799c419d3094797131c178d992564f122b5e9aa0dc7838c49da7a893e2\
                 20aeef4ede3589d6d24c1903c9de0732fd500e518841cab9aa57d270126b78d5\
                 b604489493213626299bc560b810a29672ea47a5157e7acdc9ea3c26904cd9f6\
                 dbed3a3300000000cb24d503450175011a012e036deac820276d2175bc5f0df9\
                 ffffffff0040e0e010202040003820402f30352d211b2e7099299859364b050e\
                 0006002000000200200000004706077a0a21a62434523d890c9d25a54bf1000f\
                 02010106020001050100010f0001010002018e5affffffff",
                "0200dec91a079a8dc033363164940aa56deac820276d2175bc5f0df976e522eb\
                 b771fda337ed45532819e33c7830c0add85f2f5bbb45f1e904366b554289ba64\
                 9786bfece4336ec520b12bc942e67016ec5061f71a2a13e0bba60252d3988a78\
                 e65c337ff5e4205bc23c4850c9320e41aa7078049326c51072a57aea0c02f824\
                 904c246b00000000c93e9500e002e002c000c0001a079a8dc033363164940aa5\
                 ffffffff38293b40f0151540002815001a381b263614059e9e765d504e411932\
                 223500120f100e0028000000e01b73fe97201eefebae80ea2c13644e75cc0100\
                 020101060201010501000103050001050201dbe6dddd1ddc",
            ),
            // A pure creature of level 8 takes one of level 30: 22 levels wait and are applied.
            (
                10,
                "02009d90634602fdba33e60379e1eb8d0d2295846579a36c5c2bb8de33cc4964\
                 690f6ad16a32f164835c8fd3207c8885c231b357f4cc98155f96de0bfae3f8d2\
                 1d20ad9528764470c618df1d97f551007c914ebc43863431f34bee722fefd830\
                 bf63b9c841ca88b5d094b3f9ee9c0edd54065e4e6a4ffca6f776d4fea859c6b2\
                 d94536e8000000006d350701b60056030d002601c3026d4948ee45ae07abe5b2\
                 ffffffff4000e0e0e000381038003810411932133319c6789c3099502b132d0f\
                 322600002000040008000000fe6fc7302fcbccfb65c07ef847cc2c65fcba0100\
                 01010001000001010000010603010100040125ea17bc83f6",
                "0200127529d7693db38c56e96dd5dcdca3877fa5cbc7a43ee9f758ccaa7cc199\
                 fcf2f303d34686e0350c51b2944c883e5c0c7048145d6ca7f8d0b7992d271a86\
                 36edc13c82b29738714dae8bbcd5fe895831ca93d80d2cbb538e5e55fda0685b\
                 9b7d2f96b3ccc7314df66dd95830bae99544e9d89936c4dc87dd738f77ad38c1\
                 7ca29d55000000005382bb01ed004603d0001301ede4313fb330d5c88742dd94\
                 ffffffff40100038202020104000000036291037123087ad213940930a621f1b\
                 25270920090707001e000000bffe75940da0973c5adcd15c9d93e10ff4ac0106\
                 0501010102010000020101010200010c0000aee9fdb1c652",
                "02009d75c3026d4948ee45ae07abe5b2ede4313fb330d5c88742dd94634602fd\
                 ba33e60379e1eb8d0d2295846579a36c5c2bb8de29d7693db38c56e96dd5dcdc\
                 a3877fa5cbc7a43ee9f758cc97517c4e4334f3ee2fd8bfb94188d0b3ee0e545e\
                 6afcf7d4f5bc4b30caf906a6bc89ca0d5355687db3316d3095d8c4dd0407a859\
                 77ad36e8000000006782bb01450345035f005f00c3026d4948ee45ae07abe5b2\
                 ffffffff2625393df60a28103a001205552c3c1a4029b99f5d276e7438014149\
                 180c0410140306001e000000fe6fc7302fcbccfb65c07ef847cc2c65fcba0106\
                 0101010002010100020101060201010c040125ea999901d0",
            ),
            // Odd means among the strengths.
            (
                10,
                "0300b3619fd3d8f8c87a4dc925c7dc0f0aa008ca0bd637066d88656e79ee8b14\
                 19540870f48ac5407d31efb74eefc655b296856475332ff953a950ffb2ecad7e\
                 b4b2199f18f9ec0c5084efde18680f0a2a637d98c2f40f2d073ad44a302e7369\
                 62daff7c764908f1e2770b2d48c67226a0a88f0db28e2fc17f45af5b16b97dbd\
                 b3c6f36e00000000fce0be021b01d5016e00a40080730cacfecf355b7d28439d\
                 ffffffff38e010382020e000003838102f2c2f272316371ac035101d1954524a\
                 02580f11100f0f00200000008432ac2060a41ebc9edfa5abef2ede6be98d010c\
                 0001000f0000010305010006040101040300d94b1719be37",
                "03000be81419b04f734a139e1c5474efd2881537ece600a4f41c4db16bae7ffd\
                 67e748c377ba7fe1913262022e739a3b08d71c6e562286d914f7f38cb7647b68\
                 916d568b0600e6a081fbe6a761c134f8a965fbca293b8f750bd1950f722e5d03\
                 e8e77ae56b77c9caedde2a7f2460fa3cb321c4b8fa39f39897a1324cb196639f\
                 a56c761f0000000055f609032501cb03a800b8000ff31179c9a50d763112321f\
                 ffffffff2010e0400000e02020e040380f1c373b3b1e034dbc6aac3b43130554\
                 151910091f070700280000009803ea4a7c7e13894ec03d7be2b6d416a9cc0106\
                 000101030000010f01000004020101090500058fc8f30916",
                "0300b3e880730cacfecf355b7d28439d0ff31179c9a50d763112321f9fd3d8f8\
                 c87a4dc925c7dc0f0aa008ca0bd637066d88656e1419b04f734a139e1c5474ef\
                 d2881537ece600a4f41c4db1180f2a7dc20f07d4307362ff7608e20b4872a08f\
                 b22f7faf68983a69492da8c161f8fb3b0b0f5de76bca2a60b3b8f3a10f0716b9\
                 b196f36e0000000036b51003e902e902be00be0080730cacfecf355b7d28439d\
                 ffffffff4242454e0a0ae00a0afe3a1d322f382c2b190926c84a5d1c05602a62\
                 5248100c180a0f00280000008432ac2060a41ebc9edfa5abef2ede6be98d010c\
                 000101030000010301000000000001040300d94bdffb1f36",
            ),
            // Two of level 1.
            (
                10,
                "030098a087c97f0932186232e1df3d9de37cb61228008c2676a196dbeb282604\
                 f82efe8ee335f6c74f7b40797db57101f9ecb413a8cee43cf16d6c335a846e7c\
                 3e8db44107381c55f172a8c39e28f2d63cef3e48657c57f761d49fb8716f0598\
                 b0439dbc4de996aa718703153b52f87e54b319a4f6dcd05637ee074931b5b1cd\
                 f68b54fd000000006a2afd008e0261035600cc03fce1778a0bf36228ad9ded56\
                 ffffffff384040203838383840e0380025122b171c3eb502970e75a34b234f4d\
                 1d2d080004110900010000005c31ddc8d5bbb6cc3fe4d5c0b7d1806d611b0100\
                 030100040101000604000101000001000500c9f2496e8fee",
                "03008f82523844feed90de10c9d453c6835eb0f3eac490a31b5ecfb02312332f\
                 937a2823a05e6303ec25520427d1f0e7e3b35b906f8c629a920352e954d49d27\
                 c68778401d068540c34b23e3abf6eb6a321252764be089a12333d74758bfc18c\
                 98e3ce24f3f347fb6b16e8f7bfbea133637f6423014c6c0afa6ec53de752c169\
                 fdc3f7df0000000079d5d70318008000ae00f600c30b4a7887f37a468398dcc2\
                 ffffffff0010e0e0401010381038e038382138203117a91366a3046842001b56\
                 1609090020000500010000008de80f7afb2216ba6d2af2ab5d0d0752ea260101\
                 0500010f03010004050101030100000c00018f9c2fdcfcce",
                "03009882fce1778a0bf36228ad9ded56c30b4a7887f37a468398dcc287c97f09\
                 32186232e1df3d9de37cb61228008c2676a196db523844feed90de10c9d453c6\
                 835eb0f3eac490a31b5ecfb09ef23c3e6557619f7105b09d4d9671033bf85419\
                 f6d037072848d498e915b356ab6a52e02347c1e3f3fbe8be63236c6e090531b5\
                 e75254fd00000000f17f6a022b002b0017001700fce1778a0bf36228ad9ded56\
                 ffffffff2a211e303a1d1d3820fefe1226122b1c193ec4008555318e465f3521\
                 4716080012080d00020000005c31ddc8d5bbb6cc3fe4d5c0b7d1806d611b0100\
                 0301010403010000000001030100010c0500c9f26eee0eee",
            ),
            // Another skin; the second has all four strengths.
            (
                4,
                "0200b7a63e5d801363629ed44a9917e537e35d6b5e66131e7e01262c0ff1cde2\
                 8d10ad1b3537c2bd187e5938f4289a365a8d6eac0b1d45fbc3832b9f68389941\
                 e40bae069679fb1a0a8908ea45a374fd93965981471d791cc1df8eb331c85184\
                 908b6ea595171a8b140a7f8e199b1ac65af929813bff79eb105170609d3d9908\
                 3d71758400000000c1d31d01c4018b029b007c026ef2001de8103499b7c1196e\
                 ffffffffe0003820101020e0400040102217281e1a2c79775347a34719251e1b\
                 5a2d0820101f0f001e000000876cfa937dd5b4a5e1eb91a00443d169dd080003\
                 00000001010101010301010100010006040089b19494f7c7",
                "02001260e49209ace451b2ed7b700d63438fa10f999cf55c3f767d30cf28816e\
                 344d02454dfec399259a4b7492071b3fc6053798e12f0e6d3957773899e5460a\
                 067f5d87e002fb986d4e02e4567807e300d087ffab1ffa5efb192386f77efe71\
                 d2c7fa9bb15de97159d3aaad0f6601efc3086f7321159fd50591ece8ffc0cdee\
                 603322aa00000000d720bb01fb000103d1002f03614cc293ce07c6d159263da4\
                 ffffffff000000001038e0383820e0102e333739321e2d278e4b025922422d4e\
                 42331f1f1800070003000000876ba16d995126a7f564c77c1887543bbf880109\
                 0301010101000109010000000000000c0400d136f2bfb63e",
                "0200b7cd6ef2001de8103499b7c1196e614cc293ce07c6d159263da43e5d8013\
                 63629ed44a9917e537e35d6b5e66131e7e01262ce49209ace451b2ed7b700d63\
                 438fa10f999cf55c3f767d30457493594779c18e3151906e951a147f191a5a29\
                 3b791070a381df84178ef9eb56e3871ffb86fec7b171aa66c3739f910f079d3d\
                 ffc0758400000000c1d31d01ab02ab02750075006ef2001de8103499b7c1196e\
                 ffffffff545e6c5f101df6fe3a0a00101f15291e152c4f4973424d4b19251e1b\
                 5a2d131f140f0f001e000000876cfa937dd5b4a5e1eb91a00443d169dd080103\
                 03010100010001000100010000010000000089b1f6bf17fe",
            ),
        ];
        let tables = tables();
        for (thing, first, second, after) in vectors {
            let (first, second) = (creature(first), creature(second));
            let made = merged(&first, &second, skin_look(thing), &tables, &mut no_draws());
            assert_eq!(made, Some(creature(after)), "thing {thing}");
        }
    }

    #[test]
    fn what_two_have_learnt_is_added_as_levels_that_double() {
        // Level and word there and back.
        for (level, word) in [(1, 0), (5, 0x0123_4567), (31, 1), (60, 0x03FF_FFFF)] {
            let back = packed(worth(level, word));
            assert_eq!(back, (u32::from(level) << 26).wrapping_add(word), "{level}");
        }
        // Two of the same level make one level more, and the words are halved.
        assert_eq!(packed(worth(5, 0) + worth(5, 0)), 6 * LEVEL);
        assert_eq!(packed(worth(5, 1000) + worth(5, 3000)), 6 * LEVEL + 2000);
        // A level 1 creature adds little to one of level 10: two levels of its own
        // measure, which is a 512th of the tenth level's.
        assert_eq!(
            packed(worth(10, 0) + worth(1, 0)),
            10 * LEVEL + (LEVEL >> 9)
        );
        // Level 0 is worth one level, whatever its word.
        assert_eq!(worth(0, 12345), ONE_LEVEL);
        assert_eq!(packed(worth(0, 12345) + worth(0, 0)), LEVEL);
    }

    #[test]
    fn the_small_rules_of_a_merge() {
        // The growth: the mean, and a seventh of what the two have over 200.
        assert_eq!(growth(100, 100), 100);
        assert_eq!(growth(170, 30), 100);
        assert_eq!(growth(170, 170), 190);
        assert_eq!(growth(200, 200), 200);
        assert_eq!(growth(30, 20), 4);
        assert_eq!(growth(0, 0), 0);
        // Two pure creatures of different elements: 16 and 16.
        assert_eq!(strengths([32, 0, 0, 0], [0, 0, 32, 0]), [16, 0, 16, 0]);
        // Two odd means: the stronger goes up, the weaker down.
        assert_eq!(strengths([32, 0, 0, 0], [15, 31, 4, 0]), [24, 14, 2, 0]);
        // Four: the two strongest up, the two weakest down; one or three stay.
        assert_eq!(strengths([18, 14, 10, 6], [0; 4]), [10, 8, 4, 2]);
        assert_eq!(strengths([18, 14, 10, 8], [0; 4]), [9, 7, 5, 4]);
        assert_eq!(strengths([18, 0, 0, 0], [0; 4]), [9, 0, 0, 0]);
        // The skins are the ten species from 201 on.
        assert_eq!(skin_look(0), Some(201));
        assert_eq!(skin_look(9), Some(210));
        assert_eq!(skin_look(10), None);
    }

    #[test]
    fn a_slot_takes_the_second_s_attack_of_an_element_the_creature_shows() {
        let entry = |attack: u8, grade: u8| Move {
            known: true,
            attack,
            grade,
            flag: 0,
        };
        let none = Move::default();
        let fire = 1 << FIRE;
        // Slot 0: a fire attack of the second takes the place of the first's plain
        // one. Slot 1: an earth attack does not, the creature shows no earth. Slot 2:
        // the first's own earth attack is lost and the second's special one fills the
        // place. Slot 3: known by the second alone. Slot 4: known by neither.
        let first = [entry(0, 0), entry(9, 3), entry(2, 0), none, none];
        let second = [entry(4, 2), entry(2, 1), entry(12, 0), entry(13, 4), none];
        let made = moves(&first, &second, fire);
        assert_eq!(
            made[0],
            Move {
                flag: 1,
                ..entry(4, 2)
            }
        );
        assert_eq!(made[1], entry(9, 1));
        assert_eq!(made[2], entry(12, 0));
        assert_eq!(
            made[3],
            Move {
                flag: 1,
                ..entry(13, 4)
            }
        );
        assert_eq!(made[4], none);
        // A special attack of the second does not take a place that has an attack.
        assert_eq!(
            moves(&[entry(6, 0); 5], &[entry(12, 0); 5], fire)[0],
            entry(6, 0)
        );
    }

    #[test]
    fn a_line_of_the_right_families_makes_a_new_species() {
        let tables = tables();
        let mut draws = [3u32, 14, 15, 92, 65, 35].into_iter();
        let mut first = Creature::of_a_caught(
            state::CARRIED,
            Record::make(&tables, 59, 8 * LEVEL, &mut || 0).expect("a record"),
        );
        let mut second = Creature::of_a_caught(
            state::KEPT,
            Record::make(&tables, 21, 3 * LEVEL, &mut || 0).expect("a record"),
        );
        // Base creature 30 in the first's line; 14 as a variant and 15 in the second's.
        first.line.species = [30; LINE];
        second.line.species = std::array::from_fn(|n| [113, 15][n / 3 % 2]);
        assert_eq!(new_species(&line(&first, &second).species), Some(34));
        let made = merged(&first, &second, skin_look(2), &tables, &mut || {
            draws.next().expect("six draws")
        })
        .expect("a creature");
        // Level 9 and level 4 are level 9 together, and the new one is one above.
        assert_eq!((made.record.species, made.record.level), (34, 10));
        assert_eq!(made.state, state::CARRIED);
        assert_eq!(made.line, Line::of_a_caught(&made.record));
        assert_eq!(made.line.look, 34);
        assert!(draws.next().is_none());

        // The other list: 3, 7, 15 and 9. Two of a family are too few.
        let mut line = [0; LINE];
        line[..12].copy_from_slice(&[3, 69, 72, 7, 85, 88, 15, 117, 120, 9, 93, 96]);
        assert_eq!(new_species(&line), Some(35));
        line[0] = 4;
        assert_eq!(new_species(&line), None);
        assert_eq!(new_species(&[30; LINE]), None);
    }

    #[test]
    fn a_merge_keeps_the_first_and_writes_the_family() {
        let tables = tables();
        let record = |species: u32, levels: u32| {
            Record::make(&tables, species, levels * LEVEL, &mut || 50).expect("a record")
        };
        // Species 59 is of earth, and the test tables give it a name, a fire attack
        // in slot 2 and the spells 2 and 5; species 21 has no element.
        let first = Creature::of_a_caught(state::KEPT, record(59, 4));
        let second = Creature::of_a_caught(state::CARRIED, record(21, 4));
        let made = merged(&first, &second, None, &tables, &mut no_draws()).expect("a creature");
        assert_eq!((made.state, made.record.species), (state::KEPT, 59));
        // Two of level 5 are one of level 6, whole.
        assert_eq!((made.record.level, made.record.experience), (6, 0));
        assert_eq!(
            (made.record.hp, made.record.mp),
            (made.record.max_hp, made.record.max_mp)
        );
        // Earth 32 and 8 all round: 20 of earth and 4 of the others, which do not show.
        assert_eq!(made.record.strengths, [4, 4, 20, 4]);
        assert_eq!(made.record.element_bit, 1 << EARTH);
        // The fire attack is not of an element the creature shows.
        assert_eq!(made.record.moves[2].attack, 0);
        assert!(made.record.moves[2].known);
        // Its 48 species: 32 of the first and 16 of the second; the look the second's.
        let of_first = made.line.species.iter().filter(|&&s| s == 59).count();
        assert_eq!((of_first, made.line.look), (32, 21));
        assert_eq!(made.line.names[0], first.record.name);
        assert_eq!(made.line.names[1], second.record.name);
        assert_eq!(made.line.signs, [4, 0, 4, 4, 0, 0]);
        // With a skin the look is the skin's.
        let skinned = merged(&first, &second, skin_look(9), &tables, &mut no_draws());
        assert_eq!(skinned.map(|creature| creature.line.look), Some(210));
        // Without the spells' rows there is no merge.
        let bare = made_up();
        assert!(merged(&first, &second, None, &bare, &mut no_draws()).is_none());
    }
}
