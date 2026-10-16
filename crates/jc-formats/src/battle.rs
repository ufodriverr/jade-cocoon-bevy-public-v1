
pub mod ai;
pub mod arena;
pub mod duel;
pub mod effects;
pub mod fight;
pub mod hud;
pub mod lesson;
pub mod menu;
pub mod menu_panel;
pub mod moves;
pub mod record;
pub mod resolve;
pub mod results;
pub mod runner;
pub mod using;

/// The species with a base record and a row of defensive modifiers.
pub const BASES: usize = 57;
/// The species with a word of spells and five move entries.
pub const SPECIES: usize = 201;
pub const UNIQUES: usize = 10;
/// The species number of an empty record.
pub const EMPTY: u32 = 211;
/// The elements, as every table numbers them.
pub const FIRE: usize = 0;
pub const WIND: usize = 1;
pub const EARTH: usize = 2;
pub const WATER: usize = 3;
/// The classes of an effect behind the four elements: the index of the target's
/// defensive modifier.
pub mod class {
    pub const POISON: u8 = 4;
    pub const SLEEP: u8 = 5;
    pub const STONE: u8 = 6;
    pub const DEATH: u8 = 7;
    pub const DRAIN: u8 = 8;
    pub const HEALING: u8 = 9;
    pub const PHYSICAL: u8 = 10;
    pub const ALWAYS: u8 = 11;
    /// No formula: the amount is the power itself.
    pub const NONE: u8 = 255;
}
/// The bits of an item's flags.
pub mod item {
    pub const OUTSIDE_A_FIGHT: u16 = 0x01;
    pub const IN_A_FIGHT: u16 = 0x02;
    pub const ALL_OF_A_SIDE: u16 = 0x04;
    pub const ON_THE_PLAYER: u16 = 0x08;
    pub const ON_A_MINION: u16 = 0x10;
    pub const AT_THE_OWN_SIDE: u16 = 0x40;
    pub const AT_THE_OPPONENTS: u16 = 0x80;
}

/// Where the tables lie in the executable, and how many rows each has.
mod at {
    pub const BASES: u32 = 0x8007_A3D8;
    pub const GROWTH_STEPS: (u32, usize) = (0x8007_926C, 20);
    pub const CAPTURE_STEPS: u32 = 0x8007_92E8;
    pub const CAPTURE: u32 = 0x8007_92EC;
    pub const SPELL_BITS: u32 = 0x8007_A5D0;
    pub const MOVES: u32 = 0x8007_A91C;
    pub const RELATIONS: u32 = 0x8007_B998;
    pub const MODIFIERS: u32 = 0x8007_B9A8;
    pub const PLAIN_ATTACK: u32 = 0x8007_1E7C;
    pub const SPELLS: (u32, usize) = (0x8007_C35C, 30);
    pub const SPELL_EFFECTS: u32 = 0x8007_C4C4;
    pub const BODY_ATTACKS: (u32, usize) = (0x8008_34B8, 39);
    pub const ITEM_FLAGS: u32 = 0x8007_CB74;
    pub const ITEMS: (u32, usize) = (0x8007_CBCC, 43);
    pub const WEAPON_EFFECTS: u32 = 0x8007_D198;
    pub const WEAPONS: (u32, usize) = (0x8007_CF74, 23);
    pub const ARMOURS: (u32, usize) = (0x8007_D348, 15);
    pub const ARMOUR_MODIFIERS: u32 = 0x8007_D4BC;
    pub const THIRDS: (u32, usize) = (0x8007_D5D8, 17);
    pub const THIRD_MODIFIERS: u32 = 0x8007_D76C;
}

/// The twelve bytes that say what an attack, a spell, an item or a weapon does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Descriptor {
    /// The status bits the action adds or clears.
    pub status: u16,
    /// The power in hundredths; 0 is no damage part and no hit roll.
    pub power: i16,
    /// The element of the deva the action calls onto the field, -1 for none.
    pub deva: i8,
    /// The accuracy (pair 0 only), the status rate in percent, and what is added to
    /// the critical chance.
    pub accuracy: u8,
    pub rate: u8,
    pub critical: i8,
    /// The class: the index of the target's modifier (`class`).
    pub class: u8,
    /// 0 attack against defence, 1 magic attack against magic defence, 2 the power
    /// itself against 1.
    pub pair: u8,
    /// The status mode: 3 against the target (rolled), 0 for the target.
    pub mode: u8,
    /// What the amount does: 0 HP, 1 MP, 2 HP and MP, 3 "down to 1 HP" on a roll.
    pub amount: u8,
}

impl Descriptor {
    pub const SIZE: usize = 12;

    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let bytes: &[u8; Self::SIZE] = bytes.get(..Self::SIZE)?.try_into().ok()?;
        Some(Self {
            status: u16::from_le_bytes([bytes[0], bytes[1]]),
            power: i16::from_le_bytes([bytes[2], bytes[3]]),
            deva: bytes[4] as i8,
            accuracy: bytes[5],
            rate: bytes[6],
            critical: bytes[7] as i8,
            class: bytes[8],
            pair: bytes[9],
            mode: bytes[10],
            amount: bytes[11],
        })
    }
}

/// The base record of a species: what a new creature record is made from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Base {
    /// The growth values in the order of a creature record: attack, defence, magic
    /// attack, magic defence, speed, sixth. (The table has defence and magic attack the
    /// other way round.)
    pub growth: [u8; 6],
    /// The element, -1 for none.
    pub element: i8,
    /// The body flags, bits 0 and 1.
    pub body: u8,
}

impl Base {
    pub const SIZE: usize = 8;

    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let b: &[u8; Self::SIZE] = bytes.get(..Self::SIZE)?.try_into().ok()?;
        Some(Self {
            growth: [b[0], b[2], b[1], b[3], b[4], b[5]],
            element: b[6] as i8,
            body: b[7],
        })
    }
}

/// One of the five move entries of a species: an attack slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Move {
    pub known: bool,
    /// The number of the body attack.
    pub attack: u8,
    /// The grade: 0, or 2 to 4.
    pub grade: u8,
    /// A flag with no reader found.
    pub flag: u8,
}

impl Move {
    /// The MP a body attack costs (`0x80031A60`): nothing for an entry that is not
    /// known, else by the attack's number, and 10 more with a grade.
    pub fn cost(&self) -> i32 {
        if !self.known {
            return 0;
        }
        let by_number = match self.attack {
            14 | 15 => 5,
            13 => 35,
            12 => 15,
            2..=10 => 10,
            _ => 0,
        };
        by_number + if matches!(self.grade, 2..=4) { 10 } else { 0 }
    }
}

/// A spell: its record of twelve bytes and what it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spell {
    /// Word +0 of the record; what its bits mean is not read (the targets among them).
    pub word: u32,
    /// The MP it costs (byte 4 of the record).
    pub cost: u8,
    pub shown: u8,
    /// Byte 7 of the record: what the target of a spell for the own side must lack
    /// for the menu of a fight to take it (`0x800340C4`): 0 HP, 1 a status to cure.
    pub need: u8,
    /// The half word +8 of the record: the spell's element, which a merge asks
    /// (`0x8001AD30`): the merged creature keeps a spell only with that element.
    pub element: i16,
    pub effect: Descriptor,
}

/// An item in a fight: its flags (`item`) and what it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Item {
    pub flags: u16,
    pub effect: Descriptor,
}

/// A row of one of the three equipment tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Equipment {
    /// What it adds to the six stats, in the order of a creature record.
    pub stats: [i8; 6],
    /// When not 0, a level that takes the place of the wearer's in the formulas: the
    /// largest of the three rows counts.
    pub level: u32,
    /// The caps of the wearer's twelve defensive modifiers; a weapon has none.
    pub modifiers: Option<[i8; 12]>,
    /// What an attack with it does; only a weapon has this.
    pub effect: Option<Descriptor>,
}

impl Equipment {
    /// The bytes of a row from the first stat on. (The tables' rows are 24 bytes and
    /// start four bytes in front of that: HYPOTHESIS from the distances between them.)
    const SIZE: usize = 24;
    const LEVEL: usize = 16;
}

/// What a new record of one of the species 201 to 210 is made from: the answers of
/// the four routines of scene package 114 the set-up calls, and its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unique {
    pub base: Base,
    pub modifiers: [i8; 12],
    pub spell_bits: u32,
    pub moves: [Move; 5],
    pub name: [u8; crate::name::LETTERS],
}

/// Every table of the rules that the executable holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tables {
    /// The species 201 to 210, while their package is in memory
    /// (`corridor::uniques`); none anywhere else, and a record of them is not made.
    pub uniques: Vec<Unique>,
    pub bases: Vec<Base>,
    /// The defensive modifiers of a base creature by the class of an effect, in 32nds.
    pub modifiers: Vec<[i8; 12]>,
    /// The spells a species knows, bit i for spell i, and its five move entries.
    pub spell_bits: Vec<u32>,
    pub moves: Vec<[Move; 5]>,
    /// The name a new record of a species gets (the set-up copies the text of the table
    /// `0x8007A094` through `0x8003E560`): ten codes of the small font, as the hero's
    /// name is kept (`crate::name::stored`).
    pub names: Vec<[u8; crate::name::LETTERS]>,
    /// Rows the defender's element, columns the attack's, in 32nds. Read only when a
    /// creature record is made: the row takes the place of its first four modifiers.
    pub relations: [[u8; 4]; 4],
    /// What a level adds to a stat's progress, in thousandths of a point, by the class
    /// of the stat's growth (0 to 19).
    pub growth_steps: Vec<u16>,
    /// The body flags as the rules of a hit read them, by species 0 to 200: byte 7 of
    /// the eight bytes at the species' place in the table of the base records. That
    /// table has 57 rows, so for an element variant the game reads a byte of what lies
    /// behind it (the words of the spells, the move entries), not its base creature's.
    pub body_flags: Vec<u8>,
    /// Capture: the four steps of the score, and by tier the numbers a draw of 16 must
    /// stay below for each grade.
    pub capture_steps: [u8; 4],
    pub capture: [[u8; 4]; 5],
    /// The attack of a fighter without a weapon.
    pub plain_attack: Descriptor,
    pub spells: Vec<Spell>,
    pub body_attacks: Vec<Descriptor>,
    pub items: Vec<Item>,
    pub weapons: Vec<Equipment>,
    pub armours: Vec<Equipment>,
    pub thirds: Vec<Equipment>,
}

impl Tables {
    /// The tables of the executable `image` loaded at `base`; `None` when one of them
    /// does not lie in it.
    pub fn read(image: &[u8], base: u32) -> Option<Self> {
        let bytes = |address: u32, len: usize| {
            let start = usize::try_from(address.checked_sub(base)?).ok()?;
            image.get(start..start.checked_add(len)?)
        };
        let rows = |address: u32, size: usize, count: usize| {
            (bytes(address, size * count)).map(|table| table.chunks_exact(size))
        };
        let descriptors = |address: u32, count: usize| -> Option<Vec<Descriptor>> {
            (rows(address, Descriptor::SIZE, count)?)
                .map(Descriptor::parse)
                .collect()
        };
        let signed = |row: &[u8]| -> Option<[i8; 12]> {
            let row: [u8; 12] = row.get(..12)?.try_into().ok()?;
            Some(row.map(|byte| byte as i8))
        };
        let equipment = |(address, count): (u32, usize),
                         modifiers: Option<u32>,
                         effects: Option<u32>|
         -> Option<Vec<Equipment>> {
            let modifiers = match modifiers {
                Some(at) => Some(
                    rows(at, 12, count)?
                        .map(signed)
                        .collect::<Option<Vec<_>>>()?,
                ),
                None => None,
            };
            let effects = match effects {
                Some(at) => Some(descriptors(at, count)?),
                None => None,
            };
            (rows(address, Equipment::SIZE, count)?.enumerate())
                .map(|(n, row)| {
                    let stats: [u8; 6] = row[..6].try_into().ok()?;
                    let level = row[Equipment::LEVEL..Equipment::LEVEL + 4]
                        .try_into()
                        .ok()?;
                    Some(Equipment {
                        stats: stats.map(|byte| byte as i8),
                        level: u32::from_le_bytes(level),
                        modifiers: modifiers.as_ref().map(|rows| rows[n]),
                        effect: effects.as_ref().map(|rows| rows[n]),
                    })
                })
                .collect()
        };

        let relations = bytes(at::RELATIONS, 16)?;
        let relations: [[u8; 4]; 4] =
            std::array::from_fn(|row| std::array::from_fn(|column| relations[4 * row + column]));
        let (spells_at, spells) = at::SPELLS;
        let effects = descriptors(at::SPELL_EFFECTS, spells)?;
        let spells = (rows(spells_at, 12, spells)?.zip(effects))
            .map(|(record, effect)| Spell {
                word: u32::from_le_bytes([record[0], record[1], record[2], record[3]]),
                cost: record[4],
                shown: record[6],
                need: record[7],
                element: i16::from_le_bytes([record[8], record[9]]),
                effect,
            })
            .collect();
        let (items_at, items) = at::ITEMS;
        let items = (rows(at::ITEM_FLAGS, 2, items)?.zip(descriptors(items_at, items)?))
            .map(|(flags, effect)| Item {
                flags: u16::from_le_bytes([flags[0], flags[1]]),
                effect,
            })
            .collect();
        Some(Self {
            uniques: Vec::new(),
            bases: (rows(at::BASES, Base::SIZE, BASES)?)
                .map(Base::parse)
                .collect::<Option<_>>()?,
            modifiers: (rows(at::MODIFIERS, 12, BASES)?)
                .map(signed)
                .collect::<Option<_>>()?,
            spell_bits: (rows(at::SPELL_BITS, 4, SPECIES)?)
                .map(|word| u32::from_le_bytes([word[0], word[1], word[2], word[3]]))
                .collect(),
            moves: (rows(at::MOVES, 20, SPECIES)?)
                .map(|five| {
                    std::array::from_fn(|slot| {
                        let entry = &five[4 * slot..4 * slot + 4];
                        Move {
                            known: entry[0] != 0,
                            attack: entry[1],
                            grade: entry[2],
                            flag: entry[3],
                        }
                    })
                })
                .collect(),
            names: {
                let fonts = crate::text::Fonts::from_executable(image, base)?;
                let small = fonts.get(crate::text::SMALL_FONT)?;
                (0..SPECIES as u32)
                    .map(|species| {
                        let name = crate::player_panel::species_name(image, base, species);
                        crate::name::stored(&name.unwrap_or_default(), small)
                    })
                    .collect()
            },
            relations,
            growth_steps: (rows(at::GROWTH_STEPS.0, 2, at::GROWTH_STEPS.1)?)
                .map(|step| u16::from_le_bytes([step[0], step[1]]))
                .collect(),
            body_flags: (rows(at::BASES, Base::SIZE, SPECIES)?)
                .map(|row| row[7])
                .collect(),
            capture_steps: bytes(at::CAPTURE_STEPS, 4)?.try_into().ok()?,
            capture: {
                let table = bytes(at::CAPTURE, 20)?;
                std::array::from_fn(|tier| std::array::from_fn(|grade| table[4 * tier + grade]))
            },
            plain_attack: Descriptor::parse(bytes(at::PLAIN_ATTACK, Descriptor::SIZE)?)?,
            spells,
            body_attacks: descriptors(at::BODY_ATTACKS.0, at::BODY_ATTACKS.1)?,
            items,
            weapons: equipment(at::WEAPONS, None, Some(at::WEAPON_EFFECTS))?,
            armours: equipment(at::ARMOURS, Some(at::ARMOUR_MODIFIERS), None)?,
            thirds: equipment(at::THIRDS, Some(at::THIRD_MODIFIERS), None)?,
        })
    }

    /// The base creature of a species: itself up to 56, the creature under an element
    /// variant (57 + base * 4 + element), none for the uniques and an empty record.
    pub fn base_of(species: u32) -> Option<usize> {
        match species as usize {
            base if base < BASES => Some(base),
            variant if variant < SPECIES => Some((variant - BASES) / 4),
            _ => None,
        }
    }

    /// Whether a species knows a spell.
    pub fn knows(&self, species: u32, spell: usize) -> bool {
        (self.spell_bits.get(species as usize))
            .is_some_and(|bits| spell < 32 && bits >> spell & 1 != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_descriptor_reads_its_twelve_bytes() {
        // "Poison" as the document gives it, with a made-up deva and critical.
        let bytes = [0x01, 0x00, 120, 0, 0xFF, 100, 60, 0xF6, 4, 0, 3, 0];
        let read = Descriptor::parse(&bytes).expect("a descriptor");
        assert_eq!((read.status, read.power, read.deva), (1, 120, -1));
        assert_eq!((read.accuracy, read.rate, read.critical), (100, 60, -10));
        assert_eq!(
            (read.class, read.pair, read.mode, read.amount),
            (class::POISON, 0, 3, 0)
        );
        // A negative power is a gain.
        let heals = Descriptor::parse(&[0, 0, 0xD8, 0xFF, 0xFF, 0, 0, 0, 255, 2, 0, 0]);
        assert_eq!(
            heals.map(|heals| (heals.power, heals.class)),
            Some((-40, class::NONE))
        );
        assert!(Descriptor::parse(&bytes[..11]).is_none());
    }

    #[test]
    fn a_base_record_swaps_defence_and_magic_attack() {
        let base = Base::parse(&[150, 40, 140, 50, 120, 100, 1, 2]).expect("a record");
        assert_eq!(base.growth, [150, 140, 40, 50, 120, 100]);
        assert_eq!((base.element, base.body), (1, 2));
        let none = Base::parse(&[0, 0, 0, 0, 0, 0, 0xFF, 0]).expect("a record");
        assert_eq!(none.element, -1);
    }

    #[test]
    fn a_body_attack_costs_by_its_number_and_its_grade() {
        let entry = |attack: u8, grade: u8| Move {
            known: true,
            attack,
            grade,
            flag: 0,
        };
        let costs: Vec<i32> = [0, 1, 2, 10, 11, 12, 13, 14, 15, 16]
            .iter()
            .map(|&attack| entry(attack, 0).cost())
            .collect();
        assert_eq!(costs, [0, 0, 10, 10, 0, 15, 35, 5, 5, 0]);
        assert_eq!(entry(0, 2).cost(), 10);
        assert_eq!(entry(13, 4).cost(), 45);
        // Grade 1 is none, and an entry that is not known costs nothing.
        assert_eq!(entry(2, 1).cost(), 10);
        let unknown = Move {
            known: false,
            ..entry(13, 3)
        };
        assert_eq!(unknown.cost(), 0);
    }

    #[test]
    fn a_variant_has_the_base_creature_under_it() {
        assert_eq!(Tables::base_of(12), Some(12));
        assert_eq!(Tables::base_of(56), Some(56));
        // 57 + base * 4 + element.
        assert_eq!(Tables::base_of(57), Some(0));
        assert_eq!(Tables::base_of(106), Some(12));
        assert_eq!(Tables::base_of(200), Some(35));
        assert_eq!(Tables::base_of(201), None);
        assert_eq!(Tables::base_of(EMPTY), None);
    }

    #[test]
    fn tables_that_do_not_lie_in_the_image_are_none() {
        assert!(Tables::read(&[0; 64], 0x8001_0000).is_none());
        assert!(Tables::read(&[0; 64], 0x9000_0000).is_none());
    }
}
