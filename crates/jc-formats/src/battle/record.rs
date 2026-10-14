
use super::{Move, SPECIES, Tables};

/// The stats of a record, in its order.
pub mod stat {
    pub const ATTACK: usize = 0;
    pub const DEFENCE: usize = 1;
    pub const MAGIC_ATTACK: usize = 2;
    pub const MAGIC_DEFENCE: usize = 3;
    pub const SPEED: usize = 4;
    pub const SIXTH: usize = 5;
}

/// The bits of the status word that change a side's numbers or a record's growth.
pub mod status {
    pub const POISON: u32 = 0x0001;
    pub const SLEEP: u32 = 0x0002;
    /// Every step of turning to stone.
    pub const STONE: u32 = 0x03F8;
    pub const DEAD: u32 = 0x0400;
    pub const DEFENDING: u32 = 0x8000;
}

/// One level in an experience word: the bits above are the levels that wait, the bits
/// below the way to the next one.
pub const LEVEL: u32 = 0x0400_0000;
/// The equipment byte of an empty slot.
pub const NOTHING: u8 = 0xFF;
/// The highest stat a record holds, and the highest the rules count with.
const HIGHEST_STAT: i32 = 100;
const HIGHEST_COUNTED: i32 = 99;
/// The highest HP and MP.
const HIGHEST_POINTS: i32 = 999;
/// The highest growth value a record takes from a base record.
const HIGHEST_GROWTH: u8 = 200;
/// What a level of the results screen adds to the six totals besides the step.
const BONUS: [i32; 6] = [500, 250, 125, 62, 31, 15];
/// The template of the player's record in file 23, and where that file loads.
const PLAYER_TEMPLATE: u32 = 0x800A_2B04;
const TITLE_BASE: u32 = 0x800A_0530;

/// Where the random numbers of a rule come from, so that a test can give them.
pub trait Draws {
    fn draw(&mut self) -> u32;
}

impl<F: FnMut() -> u32> Draws for F {
    fn draw(&mut self) -> u32 {
        self()
    }
}

/// The game's own generator (`0x8004E2BC`): fifteen bits a draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Random {
    pub seed: u32,
}

impl Draws for Random {
    fn draw(&mut self) -> u32 {
        self.seed = self.seed.wrapping_mul(0x41C6_4E6D).wrapping_add(12345);
        self.seed >> 16 & 0x7FFF
    }
}

/// Who asks for the levels of a record: the set-up of a new one, the results screen,
/// whose levels bring a bonus, or a merge (mode 2 of the routine, which counts as the
/// set-up's does: only mode 1 is asked for).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Growth {
    SetUp,
    Results,
    Merge,
}

/// Which formulas give a side its numbers: the player's or a creature's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Player,
    Creature,
}

/// The ten numbers of a side, by the document's numbering.
pub mod number {
    pub const ATTACK: usize = 0;
    pub const DEFENCE: usize = 1;
    pub const MAGIC_ATTACK: usize = 2;
    pub const MAGIC_DEFENCE: usize = 3;
    pub const ACCURACY: usize = 4;
    pub const EVASION: usize = 5;
    pub const CRITICAL: usize = 6;
    /// The share of the status rate (`n9`): 100 for the player.
    pub const SHARE: usize = 9;
}

/// A fighter's record: the head of 72 bytes every rule works on, and what a creature
/// has behind it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub status: u32,
    /// Bits 26 and up the levels that wait, below them the way to the next level.
    pub experience: u32,
    pub hp: i16,
    pub max_hp: i16,
    pub mp: i16,
    pub max_mp: i16,
    /// Weapon, armour and the third slot; [`NOTHING`] for none.
    pub equipment: [u8; 3],
    /// The defensive modifiers by the class of an effect, in 32nds.
    pub modifiers: [i8; 12],
    pub stats: [u8; 6],
    pub growth: [u8; 6],
    /// The way of each stat to its next point, in hundredths.
    pub progress: [u8; 6],
    /// The strength of the fighter's own attacks by element.
    pub strengths: [u8; 4],
    /// 1 << element, 0 without one.
    pub element_bit: u16,
    pub level: u8,
    /// A level counts but changes no stat and no HP or MP (+0x45 of the record).
    pub fixed: bool,
    pub species: u32,
    pub moves: [Move; 5],
    /// Bit i for spell i.
    pub spells: u32,
    /// The creature's own name (+16 of a caught creature's record, +148 of a carried
    /// one's), kept as the hero's is (`crate::name::stored`): its species' name until
    /// the player gives it another.
    pub name: [u8; crate::name::LETTERS],
}

impl Record {
    /// The bytes of a head.
    pub const HEAD: usize = 72;

    /// A record from the 72 bytes of a head; it has no species, moves or spells.
    pub fn head(bytes: &[u8]) -> Option<Self> {
        let b: &[u8; Self::HEAD] = bytes.get(..Self::HEAD)?.try_into().ok()?;
        let word = |at: usize| u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]);
        let half = |at: usize| i16::from_le_bytes([b[at], b[at + 1]]);
        let six = |at: usize| -> [u8; 6] { std::array::from_fn(|n| b[at + n]) };
        Some(Self {
            status: word(0),
            experience: word(4),
            hp: half(8),
            max_hp: half(10),
            mp: half(12),
            max_mp: half(14),
            equipment: [b[0x1C], b[0x1D], b[0x1E]],
            modifiers: std::array::from_fn(|n| b[0x20 + n] as i8),
            stats: six(0x2C),
            growth: six(0x32),
            progress: six(0x38),
            strengths: std::array::from_fn(|n| b[0x3E + n]),
            element_bit: u16::from_le_bytes([b[0x42], b[0x43]]),
            level: b[0x44],
            fixed: b[0x45] != 0,
            species: super::EMPTY,
            moves: [Move::default(); 5],
            spells: 0,
            name: [0; crate::name::LETTERS],
        })
    }

    /// The way back of [`Self::head`]: the fields of a head go into its 72 bytes. The
    /// bytes a head has besides them (the name at +16, the byte behind the equipment
    /// and the two at the end) stay what they are.
    pub fn write_head(&self, bytes: &mut [u8; Self::HEAD]) {
        bytes[0..4].copy_from_slice(&self.status.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.experience.to_le_bytes());
        for (n, half) in [self.hp, self.max_hp, self.mp, self.max_mp]
            .into_iter()
            .enumerate()
        {
            bytes[8 + 2 * n..10 + 2 * n].copy_from_slice(&half.to_le_bytes());
        }
        bytes[0x1C..0x1F].copy_from_slice(&self.equipment);
        for (byte, modifier) in bytes[0x20..0x2C].iter_mut().zip(self.modifiers) {
            *byte = modifier as u8;
        }
        bytes[0x2C..0x32].copy_from_slice(&self.stats);
        bytes[0x32..0x38].copy_from_slice(&self.growth);
        bytes[0x38..0x3E].copy_from_slice(&self.progress);
        bytes[0x3E..0x42].copy_from_slice(&self.strengths);
        bytes[0x42..0x44].copy_from_slice(&self.element_bit.to_le_bytes());
        bytes[0x44] = self.level;
        bytes[0x45] = u8::from(self.fixed);
    }

    /// The bytes of a record of 136, as the twelve caught records are.
    pub const BYTES: usize = 136;

    pub fn to_bytes(&self) -> [u8; Self::BYTES] {
        let mut bytes = [0; Self::BYTES];
        let (head, rest) = bytes.split_at_mut(Self::HEAD);
        self.write_head(head.try_into().expect("the head's bytes"));
        head[0x10..0x10 + self.name.len()].copy_from_slice(&self.name);
        head[0x1F] = NOTHING;
        rest[0x14..0x18].copy_from_slice(&self.species.to_le_bytes());
        rest[0x18..0x18 + self.name.len()].copy_from_slice(&self.name);
        self.write_moves(&mut rest[0x24..0x38]);
        rest[0x38..0x3C].copy_from_slice(&self.spells.to_le_bytes());
        bytes
    }

    /// The way back of [`Self::to_bytes`]; `None` for an empty record (species 211).
    pub fn from_bytes(bytes: &[u8; Self::BYTES]) -> Option<Self> {
        let word = |at: usize| u32::from_le_bytes([0, 1, 2, 3].map(|n| bytes[at + n]));
        let record = Self {
            species: word(0x5C),
            moves: Self::read_moves(&bytes[0x6C..0x80]),
            spells: word(0x80),
            name: std::array::from_fn(|n| bytes[0x10 + n]),
            ..Self::head(bytes)?
        };
        (record.species != super::EMPTY).then_some(record)
    }

    /// The five move entries as their twenty bytes: known, attack, grade, flag.
    pub fn write_moves(&self, bytes: &mut [u8]) {
        for (entry, to) in self.moves.iter().zip(bytes.chunks_exact_mut(4)) {
            to.copy_from_slice(&[u8::from(entry.known), entry.attack, entry.grade, entry.flag]);
        }
    }

    /// The way back of [`Self::write_moves`].
    pub fn read_moves(bytes: &[u8]) -> [Move; 5] {
        let mut entries = bytes.chunks_exact(4);
        std::array::from_fn(|_| {
            entries.next().map_or(Move::default(), |entry| Move {
                known: entry[0] != 0,
                attack: entry[1],
                grade: entry[2],
                flag: entry[3],
            })
        })
    }

    /// The player's record as "Start Game" makes it (`0x800A24DC` of file 23): the
    /// template of the file, with weapon 0 and nothing else on.
    pub fn of_the_player(title: &[u8]) -> Option<Self> {
        let template = title.get((PLAYER_TEMPLATE - TITLE_BASE) as usize..)?;
        Some(Self {
            equipment: [0, NOTHING, NOTHING],
            ..Self::head(template)?
        })
    }

    /// A creature as the set-up makes it (`0x8001900C`) from a species and the
    /// experience word of its spec: level 1 and one more for every level of the word.
    /// Six draws, one for the progress of each stat. `None` for a species this does not
    /// make.
    pub fn make(
        tables: &Tables,
        species: u32,
        experience: u32,
        draws: &mut impl Draws,
    ) -> Option<Self> {
        let index = species as usize;
        // The species 201 to 210 have their records in scene package 114.
        let unique = (index.checked_sub(SPECIES)).and_then(|n| tables.uniques.get(n));
        if (index >= SPECIES && unique.is_none()) || (37..=40).contains(&species) {
            return None;
        }
        let base = match unique {
            Some(unique) => &unique.base,
            None => tables.bases.get(Tables::base_of(species)?)?,
        };
        let mut growth = base.growth;
        let mut element = base.element;
        if let Some(variant) = index.checked_sub(super::BASES).map(|n| (n & 3) as i8)
            && unique.is_none()
            && variant != element
        {
            // An element variant of another element: the change in a record's order.
            let change: [i8; 6] = match variant {
                0 => [10, -10, 10, -10, 0, 0],
                1 => [0, -10, 0, -10, 10, 10],
                2 => [0, 10, 0, 10, -10, -10],
                _ => [-10, -10, 10, 10, 0, 0],
            };
            growth = std::array::from_fn(|n| growth[n].wrapping_add_signed(change[n]));
            element = variant;
        }
        let growth = growth.map(|value| value.min(HIGHEST_GROWTH));
        let own = usize::try_from(element).ok();
        let mut modifiers = match unique {
            Some(unique) => unique.modifiers,
            None => *tables.modifiers.get(Tables::base_of(species)?)?,
        };
        if let Some(own) = own
            && !(43..=56).contains(&species)
            && unique.is_none()
        {
            let relations = tables.relations.get(own)?;
            for (modifier, relation) in modifiers.iter_mut().zip(relations) {
                *modifier = *relation as i8;
            }
        }
        let mut record = Self {
            status: 0,
            experience: LEVEL.wrapping_add(experience),
            hp: 0,
            max_hp: 0,
            mp: 0,
            max_mp: 0,
            equipment: [NOTHING; 3],
            modifiers,
            stats: growth.map(|value| (value / 10 + 10).min(HIGHEST_STAT as u8)),
            growth,
            progress: std::array::from_fn(|_| (draws.draw() % 100) as u8),
            strengths: match own {
                Some(own) => std::array::from_fn(|n| if n == own { 32 } else { 0 }),
                None => [8; 4],
            },
            element_bit: own.map_or(0, |own| 1 << own),
            level: 0,
            fixed: false,
            species,
            moves: match unique {
                Some(unique) => unique.moves,
                None => *tables.moves.get(index)?,
            },
            spells: match unique {
                Some(unique) => unique.spell_bits,
                None => *tables.spell_bits.get(index)?,
            },
            name: match unique {
                Some(unique) => unique.name,
                None => *tables.names.get(index)?,
            },
        };
        record.grow(tables, Growth::SetUp)?;
        record.hp = record.max_hp;
        record.mp = record.max_mp;
        Some(record)
    }

    pub fn is_dead(&self) -> bool {
        self.status & status::DEAD != 0
    }

    /// Applies the levels that wait in the experience word (`0x80011D98`) and answers
    /// how many they were. `None` when the tables have no steps.
    pub fn grow(&mut self, tables: &Tables, growth: Growth) -> Option<u32> {
        // The class of a growth: its tens up to 19, and 9 for a growth below 10.
        let classes = self.growth.map(|value| match value {
            0..=9 => 9,
            value => usize::from(value / 10).min(19),
        });
        let mut steps = [0i32; 6];
        for (step, class) in steps.iter_mut().zip(classes) {
            *step = i32::from(*tables.growth_steps.get(class)?);
        }
        let mut totals = self.progress.map(|hundredths| i32::from(hundredths) * 10);
        let mut levels = 0;
        while self.experience >= LEVEL {
            self.experience -= LEVEL;
            levels += 1;
            for n in 0..6 {
                totals[n] += steps[n];
                if growth == Growth::Results {
                    totals[n] += BONUS[n];
                }
            }
        }
        self.level = self.level.wrapping_add(levels as u8);
        if self.fixed {
            return Some(levels);
        }
        for (n, total) in totals.into_iter().enumerate() {
            let stat = i32::from(self.stats[n]) + total / 1000;
            self.stats[n] = stat.clamp(0, HIGHEST_STAT) as u8;
            self.progress[n] = (total % 1000 / 10).min(100) as u8;
        }

        let level = i32::from(self.level);
        let (attack, magic) = (
            i32::from(self.growth[stat::ATTACK]),
            i32::from(self.growth[stat::MAGIC_ATTACK]),
        );
        // The share of one of the two in their sum, out of 200. With a sum of zero the
        // console's division answers -1, which the lower bound makes 1.
        let share = |of: i32| match attack + magic {
            0 => 1,
            sum => (200 * of / sum).clamp(1, 200),
        };
        let points = |base: i32, share: i32| base * (share * 40 + 6000) / 10000;
        let hp = points(20 + level * (9 + level / 2), share(attack));
        let mp = points(20 + 3 * level, share(magic));
        // The fighter keeps what it has lost: both rise by the change of their highest,
        // counted with the highest before it is cut to 999.
        let (old_hp, old_mp) = (i32::from(self.max_hp), i32::from(self.max_mp));
        self.max_hp = hp.clamp(0, HIGHEST_POINTS) as i16;
        self.max_mp = mp.clamp(0, HIGHEST_POINTS) as i16;
        if !self.is_dead() {
            let risen = |now: i16, change: i32, highest: i16| {
                (i32::from(now) + change).clamp(0, i32::from(highest)) as i16
            };
            self.hp = risen(self.hp, hp - old_hp, self.max_hp);
            self.mp = risen(self.mp, mp - old_mp, self.max_mp);
        }
        Some(levels)
    }

    /// The copy the rules work on (`0x8003362C`): the stats with what the equipment
    /// adds, cut to 0 to 99 also without any; the level of the equipment in place of
    /// the fighter's own when a row has one; each modifier at most what the armour and
    /// the third slot allow.
    pub fn effective(&self, tables: &Tables) -> Self {
        let [weapon, armour, third] = self.equipment;
        let rows = [
            tables.weapons.get(usize::from(weapon)),
            tables.armours.get(usize::from(armour)),
            tables.thirds.get(usize::from(third)),
        ];
        let mut copy = self.clone();
        for (n, stat) in copy.stats.iter_mut().enumerate() {
            let added: i32 = (rows.iter().flatten())
                .map(|row| i32::from(row.stats[n]))
                .sum();
            *stat = (i32::from(*stat) + added).clamp(0, HIGHEST_COUNTED) as u8;
        }
        let level = (rows.iter().flatten().map(|row| row.level as i32)).fold(0, i32::max);
        if level != 0 {
            copy.level = level as u8;
        }
        for caps in rows.iter().flatten().filter_map(|row| row.modifiers) {
            for (modifier, cap) in copy.modifiers.iter_mut().zip(caps) {
                *modifier = (*modifier).min(cap);
            }
        }
        copy
    }

    /// The ten numbers of a side (`0x80013EE8`, `0x80012828`) from its effective record
    /// and the accuracy of the action's descriptor, with what the side's own status
    /// does to them.
    pub fn numbers(&self, accuracy: u8, side: Side) -> [i32; 10] {
        let stat = |n: usize| i32::from(self.stats[n]);
        let growth = |n: usize| i32::from(self.growth[n]);
        let mut n = [0i32; 10];
        n[number::ATTACK] = stat(stat::ATTACK);
        n[number::MAGIC_ATTACK] = stat(stat::MAGIC_ATTACK);
        n[number::CRITICAL] = (growth(stat::SIXTH) / 10).pow(3) / 400;
        match side {
            Side::Player => {
                let armoured = |defence: i32| (defence * (defence - 20) / 7).clamp(0, 32767);
                n[number::DEFENCE] = armoured(stat(stat::DEFENCE));
                n[number::MAGIC_DEFENCE] = armoured(stat(stat::MAGIC_DEFENCE));
                n[number::ACCURACY] = 10 + i32::from(accuracy);
                n[number::EVASION] = 20;
                n[number::SHARE] = 100;
            }
            Side::Creature => {
                // How quick the creature is against how well it stands, out of 200.
                let defences = (growth(stat::DEFENCE) + growth(stat::MAGIC_DEFENCE)) >> 1;
                let quick = match growth(stat::SPEED) + defences {
                    0 => 1,
                    sum => match 200 * growth(stat::SPEED) / sum {
                        0 => 1,
                        share => share.min(200),
                    },
                };
                let square = (quick / 10).pow(2);
                n[number::DEFENCE] = stat(stat::DEFENCE);
                n[number::MAGIC_DEFENCE] = stat(stat::MAGIC_DEFENCE);
                n[number::ACCURACY] = square / 10 + i32::from(accuracy);
                n[number::EVASION] = square / 5;
                n[number::SHARE] = quick;
            }
        }
        if self.status & status::POISON != 0 {
            n[number::ATTACK] = n[number::ATTACK] * 4 / 5;
            n[number::MAGIC_ATTACK] = n[number::MAGIC_ATTACK] * 4 / 5;
        }
        if self.status & status::SLEEP != 0 {
            n[number::DEFENCE] = n[number::DEFENCE] * 2 / 3;
            n[number::MAGIC_DEFENCE] = n[number::MAGIC_DEFENCE] * 2 / 3;
            n[number::EVASION] = 0;
        }
        if self.status & status::STONE != 0 {
            n[number::ACCURACY] = n[number::ACCURACY] * 3 / 5;
            n[number::EVASION] = n[number::EVASION] * 3 / 5;
        }
        if self.status & status::DEFENDING != 0 {
            n[number::MAGIC_DEFENCE] = (6 * n[number::MAGIC_DEFENCE]).min(32767);
            n[number::DEFENCE] = (6 * n[number::DEFENCE]).min(32767);
            n[number::EVASION] = 0;
        }
        n
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::super::{Base, Descriptor, Equipment};
    use super::*;

    /// Tables with the game's steps of growth (285 and 35 more for every class) and
    /// made-up rows for everything else.
    pub(crate) fn tables() -> Tables {
        let nothing = Descriptor::parse(&[0; 12]).expect("a descriptor");
        let equipment = |stats: [i8; 6], level: u32, modifiers: Option<[i8; 12]>| Equipment {
            stats,
            level,
            modifiers,
            effect: None,
        };
        let mut bases = vec![
            Base {
                growth: [100; 6],
                element: -1,
                body: 0,
            };
            super::super::BASES
        ];
        bases[0] = Base {
            growth: [150, 140, 40, 50, 120, 100],
            element: 1,
            body: 0,
        };
        bases[1] = Base {
            growth: [5, 255, 0, 199, 30, 100],
            element: -1,
            body: 0,
        };
        let mut modifiers = vec![[32i8; 12]; super::super::BASES];
        modifiers[0] = [1, 2, 3, 4, 5, 6, 7, 8, 9, -32, 11, 12];
        let mut moves = vec![[Move::default(); 5]; SPECIES];
        moves[59][2] = Move {
            known: true,
            attack: 4,
            grade: 2,
            flag: 0,
        };
        let mut spell_bits = vec![0; SPECIES];
        spell_bits[59] = 0x24;
        let mut names = vec![[0; crate::name::LETTERS]; SPECIES];
        names[59] = [4, 15, 7, 12, 5, 0, 0, 0, 0, 0];
        Tables {
            bases,
            modifiers,
            spell_bits,
            moves,
            names,
            relations: [
                [32, 64, 32, 16],
                [16, 32, 64, 32],
                [32, 16, 32, 64],
                [64, 32, 16, 32],
            ],
            growth_steps: (0..20).map(|class| 285 + 35 * class).collect(),
            uniques: Vec::new(),
            body_flags: vec![0; SPECIES],
            capture_steps: [2, 3, 5, 10],
            capture: [
                [0, 0, 4, 16],
                [5, 8, 12, 16],
                [8, 10, 14, 16],
                [14, 16, 16, 16],
                [15, 16, 16, 16],
            ],
            plain_attack: nothing,
            spells: vec![],
            body_attacks: vec![],
            items: vec![],
            weapons: vec![
                equipment([3, 0, 0, 0, 0, 0], 5, None),
                equipment([40, -5, 0, 0, 2, 0], 9, None),
            ],
            armours: vec![equipment(
                [0, 30, 0, 4, -3, 0],
                0,
                Some([32, 32, 32, 32, 0, 24, 127, 16, 24, -32, 127, 127]),
            )],
            thirds: vec![equipment(
                [-60, 0, 90, 0, 0, 0],
                12,
                Some([127, 16, 127, 127, 127, 127, 127, 127, 127, 127, 127, -48]),
            )],
        }
    }

    /// A head of 72 bytes from the hexadecimal digits of a printed record.
    pub(crate) fn head(digits: &str) -> Record {
        let bytes: Vec<u8> = (0..digits.len() / 2)
            .map(|n| u8::from_str_radix(&digits[2 * n..2 * n + 2], 16).expect("two digits"))
            .collect();
        Record::head(&bytes).expect("a head")
    }

    #[test]
    fn the_generator_is_the_game_s() {
        let mut random = Random { seed: 0 };
        // The first values of the C library's generator from seed 0.
        assert_eq!(random.draw(), 0);
        assert_eq!(random.seed, 12345);
        assert_eq!(random.draw(), 21468);
        let mut given = [7u32, 9].into_iter();
        let mut draws = || given.next().expect("a draw");
        assert_eq!((draws.draw(), draws.draw()), (7, 9));
    }

    #[test]
    fn a_head_reads_its_fields() {
        let read = head(
            "010000006430b103cc007e01c601e202000000000000000000000000ffffffff\
             10403810383840203838401061602525562b031407099a003840373030180000\
             000000002a010000",
        );
        assert_eq!((read.status, read.experience), (1, 0x03B1_3064));
        assert_eq!(
            (read.hp, read.max_hp, read.mp, read.max_mp),
            (204, 382, 454, 738)
        );
        assert_eq!(read.equipment, [NOTHING; 3]);
        assert_eq!(read.modifiers[..4], [16, 64, 56, 16]);
        assert_eq!(read.stats, [97, 96, 37, 37, 86, 43]);
        assert_eq!(read.growth, [3, 20, 7, 9, 154, 0]);
        assert_eq!(read.progress, [56, 64, 55, 48, 48, 24]);
        assert_eq!((read.level, read.fixed), (42, true));
        assert!(Record::head(&[0; 71]).is_none());
    }

    /// Records run through the original routine in the interpreter of the research
    /// (`re/t226/vec.py`): the made-up record before, the mode, the record after and
    /// the answer.
    #[test]
    fn growth_is_the_original_routine_s() {
        let vectors = [
            // No level waits: nothing changes but HP and MP, which follow the level.
            (
                Growth::Results,
                "00000000b029bf032c00b40192008702000000000000000000000000ffffffff\
                 40e020004010e00010203810383c0c120f4a06d1248f01ee580f3420012b0000\
                 0000000019000000",
                "00000000b029bf030000840100007900000000000000000000000000ffffffff\
                 40e020004010e00010203810383c0c120f4a06d1248f01ee580f3420012b0000\
                 0000000019000000",
                0,
            ),
            // Seven levels of the results screen, with the bonus.
            (
                Growth::Results,
                "00000000d635d31edc00ed00b000a201000000000000000000000000ffffffff\
                 38002020e0e01020e0403838155e02260c1007771a8edf725f5e35602a2e0000\
                 0000000011000000",
                "00000000d635d3028101920100007000000000000000000000000000ffffffff\
                 38002020e0e01020e04038381d64052c131507771a8edf72412659511c190000\
                 0000000018000000",
                7,
            ),
            // A fixed record: the level counts and nothing else changes.
            (
                Growth::SetUp,
                "000000006f8b4a068e039303e7018002000000000000000000000000ffffffff\
                 381020e01000e020204000202a2804383e19a76908b22bc64a32140e475f0000\
                 000000002e010000",
                "000000006f8b4a028e039303e7018002000000000000000000000000ffffffff\
                 381020e01000e020204000202a2804383e19a76908b22bc64a32140e475f0000\
                 000000002f010000",
                1,
            ),
            // Seven levels of the set-up, without a bonus.
            (
                Growth::SetUp,
                "000000007f04c31ef502c6039b025d03000000000000000000000000ffffffff\
                 e0200010382010204000100045234f1a1926922e9e02ad5c34110028312100000\
                 000000013000000",
                "000000007f04c3027501460200006300000000000000000000000000ffffffff\
                 e020001038201020400010004a26541e1f2a922e9e02ad5c5e0e433c41350000\
                 000000001a000000",
                7,
            ),
            // The dead: the highest HP and MP change, what is left does not.
            (
                Growth::SetUp,
                "00040000bdd5af0bee00b701ec004b01000000000000000000000000ffffffff\
                 20101038102040000010e0400e0402295b2e4758ac4ac8023d5855195a240000\
                 000000002d000000",
                "00040000bdd5af03ee00e703ec00bb00000000000000000000000000ffffffff\
                 20101038102040000010e0400f06042a5d2f4758ac4ac80243013d1f50380000\
                 000000002f000000",
                2,
            ),
            // Seven levels of the results screen on a sleeper; attack and magic attack
            // have no growth.
            (
                Growth::Results,
                "01000000e7090d1eaa009702b5008401000000000000000000000000ffffffff\
                 3810e0e0e01040204038001027424863324700bb007105fd521e154a24090000\
                 000000000a000000",
                "01000000e7090d020000ba0000002a00000000000000000000000000ffffffff\
                 3810e0e0e0104020403800102f4a4d64364d00bb007105fd342d1c564d540000\
                 0000000011000000",
                7,
            ),
        ];
        let tables = tables();
        for (mode, before, after, answer) in vectors {
            let mut record = head(before);
            assert_eq!(record.grow(&tables, mode), Some(answer), "{before}");
            assert_eq!(record, head(after), "{before}");
        }
    }

    #[test]
    fn the_dead_keep_their_points_and_a_record_needs_the_steps() {
        let tables = tables();
        let mut record = head(&"00".repeat(72));
        record.status = status::DEAD;
        record.experience = 3 * LEVEL + 5;
        record.growth = [100, 0, 100, 0, 0, 0];
        assert_eq!(record.grow(&tables, Growth::SetUp), Some(3));
        // Level 3: base 20 + 3 * (9 + 1) = 50 and 29, both shares 100.
        assert_eq!((record.max_hp, record.max_mp), (50, 29));
        assert_eq!((record.hp, record.mp, record.experience), (0, 0, 5));
        // Class 10 for a growth of 100, class 9 for none: 3 * 635 and 3 * 600.
        assert_eq!(record.stats, [1, 1, 1, 1, 1, 1]);
        assert_eq!(record.progress, [90, 80, 90, 80, 80, 80]);
        let empty = Tables {
            growth_steps: vec![],
            ..tables
        };
        assert_eq!(record.grow(&empty, Growth::SetUp), None);
    }

    #[test]
    fn the_set_up_makes_a_variant_of_another_element() {
        let tables = tables();
        // Species 59 is variant 2 (earth) of base creature 0, whose element is wind.
        let mut given = [5u32, 199, 250, 99, 100, 1234].into_iter();
        let made = Record::make(&tables, 59, 6 * LEVEL, &mut || given.next().expect("six"))
            .expect("a record");
        assert_eq!(made.growth, [150, 150, 40, 60, 110, 90]);
        assert_eq!(made.progress.len(), 6);
        assert_eq!((made.strengths, made.element_bit), ([0, 0, 32, 0], 4));
        // The earth row of the relations, then the species' own modifiers.
        assert_eq!(made.modifiers, [32, 16, 32, 64, 5, 6, 7, 8, 9, -32, 11, 12]);
        // Level 7, as the document works it out: stats 30, 30, 16, 19, 25, 23 and one
        // more with a progress that carries; HP 127 and MP 31, both full.
        assert_eq!((made.level, made.experience, made.fixed), (7, 0, false));
        assert_eq!(made.stats, [30, 31, 17, 20, 25, 23]);
        assert_eq!(
            (made.hp, made.max_hp, made.mp, made.max_mp),
            (127, 127, 31, 31)
        );
        assert_eq!((made.species, made.spells), (59, 0x24));
        // Its name is its species'.
        assert_eq!(made.name, [4, 15, 7, 12, 5, 0, 0, 0, 0, 0]);
        assert!(made.moves[2].known && !made.moves[0].known);
        assert_eq!(made.equipment, [NOTHING; 3]);
        assert!(given.next().is_none());

        // The variant of the creature's own element changes nothing, and the experience
        // word's low bits stay as the way to the next level.
        let own = Record::make(&tables, 58, 77, &mut || 0u32).expect("a record");
        assert_eq!(own.growth, [150, 140, 40, 50, 120, 100]);
        assert_eq!((own.level, own.experience), (1, 77));
        assert_eq!(own.modifiers[..4], [16, 32, 64, 32]);
        // No element: 8 for all four, the species' own first four modifiers; a growth
        // above 200 is cut and a stat starts at a tenth of it and ten.
        let plain = Record::make(&tables, 1, 0, &mut || 0u32).expect("a record");
        assert_eq!((plain.strengths, plain.element_bit), ([8; 4], 0));
        assert_eq!(plain.growth, [5, 200, 0, 199, 30, 100]);
        assert_eq!(plain.stats, [10, 30, 10, 29, 13, 20]);
        assert_eq!(plain.modifiers, [32; 12]);
        // What this does not make.
        for species in [37, 40, 201, 211] {
            assert!(Record::make(&tables, species, 0, &mut || 0u32).is_none());
        }
    }

    #[test]
    fn a_unique_is_made_from_its_own_record_while_its_package_is_there() {
        let mut tables = tables();
        let mut moves = [Move::default(); 5];
        moves[1] = Move {
            known: true,
            attack: 13,
            grade: 3,
            flag: 1,
        };
        tables.uniques = vec![crate::battle::Unique {
            base: crate::battle::Base {
                growth: [20, 190, 180, 190, 140, 100],
                element: 0,
                body: 3,
            },
            modifiers: [32, 24, 32, 54, 32, 48, 32, 32, 32, -32, 32, 64],
            spell_bits: 0x10,
            moves,
            name: [7, 1, 0, 0, 0, 0, 0, 0, 0, 0],
        }];
        let made = Record::make(&tables, 201, 19 * LEVEL, &mut || 0u32).expect("species 201");
        assert_eq!((made.species, made.level), (201, 20));
        assert_eq!(made.growth, [20, 190, 180, 190, 140, 100]);
        // It has an element, and its modifiers stay its row's: the relations of the
        // element are not written over the first four.
        assert_eq!(made.element_bit, 1);
        assert_eq!(
            made.modifiers,
            [32, 24, 32, 54, 32, 48, 32, 32, 32, -32, 32, 64]
        );
        assert_eq!((made.moves, made.spells), (moves, 0x10));
        assert_eq!(made.name, [7, 1, 0, 0, 0, 0, 0, 0, 0, 0]);
        // The second unique has no record here.
        assert!(Record::make(&tables, 202, 0, &mut || 0u32).is_none());
    }

    #[test]
    fn the_effective_record_wears_the_equipment() {
        let tables = tables();
        let mut record = head(&"00".repeat(72));
        record.stats = [100, 13, 55, 19, 53, 18];
        record.modifiers = [64, 40, 48, 32, 8, -48, 40, 40, 24, -16, 32, 28];
        record.level = 34;
        record.equipment = [NOTHING; 3];
        // Without anything a stat of 100 still counts as 99.
        let bare = record.effective(&tables);
        assert_eq!(bare.stats, [99, 13, 55, 19, 53, 18]);
        assert_eq!((bare.level, bare.modifiers), (34, record.modifiers));

        record.equipment = [1, 0, 0];
        let worn = record.effective(&tables);
        // 100 + 40 - 60, 13 - 5 + 30, 55 + 90 cut to 99, 19 + 4, 53 + 2 - 3.
        assert_eq!(worn.stats, [80, 38, 99, 23, 52, 18]);
        // The largest level of the three rows takes the place of the wearer's.
        assert_eq!(worn.level, 12);
        assert_eq!(
            worn.modifiers,
            [32, 16, 32, 32, 0, -48, 40, 16, 24, -32, 32, -48]
        );
        // A weapon alone: its level, and no cap on a modifier.
        record.equipment = [0, NOTHING, 200];
        let armed = record.effective(&tables);
        assert_eq!((armed.stats[0], armed.level), (99, 5));
        assert_eq!(armed.modifiers, record.modifiers);
        assert_eq!(record.level, 34);
    }

    /// Effective records and their numbers as the reading that was run against the
    /// original resolver gives them (`re/t226/eff.py`).
    #[test]
    fn the_ten_numbers_of_both_sides() {
        let mut record = head(&"00".repeat(72));
        record.stats = [99, 55, 27, 81, 53, 16];
        record.growth = [99, 195, 35, 158, 32, 0];
        assert_eq!(
            record.numbers(21, Side::Player),
            [99, 275, 27, 705, 31, 20, 0, 0, 0, 100]
        );
        assert_eq!(
            record.numbers(21, Side::Creature),
            [99, 55, 27, 81, 21, 1, 0, 0, 0, 30]
        );
        record.stats = [45, 74, 57, 91, 23, 60];
        record.growth = [16, 90, 123, 80, 123, 180];
        assert_eq!(
            record.numbers(70, Side::Player),
            [45, 570, 57, 923, 80, 20, 14, 0, 0, 100]
        );
        assert_eq!(
            record.numbers(70, Side::Creature),
            [45, 74, 57, 91, 82, 24, 14, 0, 0, 118]
        );
        // Stone and the dead bit: accuracy and evasion times three fifths.
        record.status = 0x0440;
        record.stats = [11, 78, 98, 23, 32, 69];
        record.growth = [53, 131, 92, 87, 179, 23];
        assert_eq!(
            record.numbers(10, Side::Player),
            [11, 646, 98, 9, 12, 12, 0, 0, 0, 100]
        );
        assert_eq!(
            record.numbers(10, Side::Creature),
            [11, 78, 98, 23, 14, 16, 0, 0, 0, 124]
        );
        // Defending: both defences times six, no evasion.
        record.status = status::DEFENDING;
        record.stats = [99, 38, 58, 44, 62, 18];
        record.growth = [105, 43, 95, 15, 75, 116];
        assert_eq!(
            record.numbers(62, Side::Player),
            [99, 582, 58, 900, 72, 0, 3, 0, 0, 100]
        );
        assert_eq!(
            record.numbers(62, Side::Creature),
            [99, 228, 58, 264, 81, 0, 3, 0, 0, 144]
        );
        // Poison and sleep; a creature without any growth has a share of 1.
        record.status = status::POISON | status::SLEEP;
        record.stats = [50, 30, 21, 9, 0, 0];
        record.growth = [0; 6];
        assert_eq!(
            record.numbers(0, Side::Creature),
            [40, 20, 16, 6, 0, 0, 0, 0, 0, 1]
        );
        // A player's defence below 20 is no defence at all.
        assert_eq!(record.numbers(0, Side::Player)[number::MAGIC_DEFENCE], 0);
    }

    #[test]
    fn the_player_s_record_is_the_template_with_a_weapon() {
        let mut title = vec![0u8; (PLAYER_TEMPLATE - TITLE_BASE) as usize + Record::HEAD];
        let template = &mut title[(PLAYER_TEMPLATE - TITLE_BASE) as usize..];
        template[8] = 100;
        template[10] = 100;
        template[0x1C..0x20].fill(0xFF);
        template[0x44] = 1;
        template[0x45] = 1;
        let player = Record::of_the_player(&title).expect("a record");
        assert_eq!((player.hp, player.max_hp), (100, 100));
        assert_eq!(player.equipment, [0, NOTHING, NOTHING]);
        assert_eq!((player.level, player.fixed), (1, true));
        assert!(Record::of_the_player(&title[..title.len() - 1]).is_none());
    }
}
