
use crate::battle::duel::{self, Duel, Hash, Options, for_a_minion};
use crate::battle::record::{NOTHING, Record};
use crate::battle::{SPECIES, Tables};
use crate::card_screen;
use crate::carried;
use crate::creatures::{Creature, RECORDS, state};
use crate::items::{self, Keeping, Kind, MOST_OF_A_THING};
use crate::save::{self, Memory};

/// The minions of a team.
pub const MOST: usize = duel::MOST;
/// The highest HP and MP of a record, and its highest level.
const HIGHEST_POINTS: i16 = 999;
const HIGHEST_LEVEL: u8 = 99;

/// Why a team cannot be made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamError {
    /// The buffer is not a save: its sum or its mark is wrong.
    NotASave,
    /// No minion was picked (file 31's "Start fighting" is refused with an empty team),
    /// or more than three.
    Empty,
    TooMany,
    /// A pick names a record past the fifteen, or one that is free.
    NoSuchRecord(usize),
}

/// One player's side of the duel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Team {
    /// One to three minions in fight order, whole, each with what its record keeps of
    /// where it comes from (the look of its figure).
    pub members: Vec<Creature>,
    /// The items of the save that count in the duel ([`fight_items`]): number and
    /// pieces, in the order of the save's list.
    pub items: Vec<(usize, u32)>,
}

/// The game's memory of a save's buffer (the card file of a slot), or `None` when the
/// buffer is no save (`save::Memory::read`).
pub fn memory_of(buffer: &[u8]) -> Option<Memory> {
    let mut buffer = buffer.to_vec();
    buffer.resize(buffer.len().max(card_screen::BUFFER), 0);
    let mut memory = Memory::zeroed();
    memory.read(&buffer, false).then_some(memory)
}

/// The list of file 31's "Enter minion from list.": every record of the fifteen that is
/// not free, with its number, carried and kept alike (`0x800A1918`, `0x800A1A7C`).
pub fn listed(memory: &Memory) -> Vec<(usize, Creature)> {
    (memory.creatures.iter().enumerate())
        .filter_map(|(number, bytes)| Some((number, Creature::read(bytes)?)))
        .collect()
}

/// The picks a team gets when nobody picks: the carried minions in their order, then
/// the kept ones, up to three. (The build's default; the original's player picks.)
pub fn first_three(memory: &Memory) -> Vec<usize> {
    let listed = listed(memory);
    let of = |wanted: i16| {
        (listed.iter())
            .filter(move |(_, creature)| creature.state == wanted)
            .map(|(number, _)| *number)
    };
    of(state::CARRIED)
        .chain(of(state::KEPT))
        .take(MOST)
        .collect()
}

/// The items of a player's list that count in the duel, with their pieces: those usable
/// on a minion in a fight ([`for_a_minion`]).
pub fn fight_items(tables: &Tables, keeping: &Keeping) -> Vec<(usize, u32)> {
    let pairs = keeping.pairs(Kind::Item).into_iter().flatten();
    let mut found: Vec<(usize, u32)> = Vec::new();
    for pair in pairs.filter(|pair| pair.number != items::FREE && pair.count != 0) {
        let number = usize::from(pair.number);
        let flags = tables.items.get(number).map_or(0, |item| item.flags);
        if !for_a_minion(flags) {
            continue;
        }
        match found.iter_mut().find(|entry| entry.0 == number) {
            Some(entry) => entry.1 += u32::from(pair.count),
            None => found.push((number, pair.count.into())),
        }
    }
    found
}

impl Team {
    pub fn from_memory(
        tables: &Tables,
        memory: &Memory,
        picks: &[usize],
    ) -> Result<Self, TeamError> {
        if picks.is_empty() {
            return Err(TeamError::Empty);
        }
        if picks.len() > MOST {
            return Err(TeamError::TooMany);
        }
        let members = (picks.iter())
            .map(|&number| {
                let bytes = memory.creatures.get(number);
                let mut creature =
                    (bytes.and_then(Creature::read)).ok_or(TeamError::NoSuchRecord(number))?;
                creature.record.make_whole();
                Ok(creature)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let keeping = carried::keeping_of(&memory.block);
        Ok(Self {
            members,
            items: fight_items(tables, &keeping),
        })
    }

    /// [`Self::from_memory`] of a save's buffer.
    pub fn from_save(tables: &Tables, buffer: &[u8], picks: &[usize]) -> Result<Self, TeamError> {
        let memory = memory_of(buffer).ok_or(TeamError::NotASave)?;
        Self::from_memory(tables, &memory, picks)
    }

    /// The records the duel works on.
    pub fn records(&self) -> Vec<Record> {
        (self.members.iter())
            .map(|creature| creature.record.clone())
            .collect()
    }

    /// The team as it travels: the count of minions, each as its record of 248 bytes
    /// (`Creature::write`, the save's own form), the count of items and two bytes each,
    /// number and pieces.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![self.members.len() as u8];
        for creature in &self.members {
            let mut record = [0; Creature::BYTES];
            creature.write(&mut record);
            bytes.extend_from_slice(&record);
        }
        bytes.push(self.items.len() as u8);
        for &(number, pieces) in &self.items {
            bytes.extend_from_slice(&[number.min(255) as u8, pieces.min(255) as u8]);
        }
        bytes
    }

    /// The way back of [`Self::to_bytes`]; the minions are made whole. `None` for bytes
    /// of another length or shape.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let (&count, rest) = bytes.split_first()?;
        let count = usize::from(count);
        let (records, rest) = rest.split_at_checked(count * Creature::BYTES)?;
        let members = (records.as_chunks::<{ Creature::BYTES }>().0.iter())
            .map(|chunk| {
                let mut creature = Creature::read(chunk)?;
                creature.record.make_whole();
                Some(creature)
            })
            .collect::<Option<Vec<_>>>()?;
        let (&items, rest) = rest.split_first()?;
        if rest.len() != usize::from(items) * 2 {
            return None;
        }
        let items = (rest.as_chunks::<2>().0.iter())
            .map(|&[number, pieces]| (usize::from(number), u32::from(pieces)))
            .collect();
        Some(Self { members, items })
    }

    /// The tests a peer makes of the other's team (the build's; without a server only
    /// plausibility can be tested): one to three minions, each carried or kept, of a
    /// species with a record, of a level the game has, with HP and MP the game holds and
    /// nothing worn; items only those that count, each once, with pieces a list holds.
    pub fn plausible(&self, tables: &Tables) -> bool {
        let member = |creature: &Creature| {
            let record = &creature.record;
            matches!(creature.state, state::CARRIED | state::KEPT)
                && (record.species as usize) < SPECIES
                && (1..=HIGHEST_LEVEL).contains(&record.level)
                && (1..=HIGHEST_POINTS).contains(&record.max_hp)
                && (0..=HIGHEST_POINTS).contains(&record.max_mp)
                && record.equipment == [NOTHING; 3]
        };
        let item = |n: usize, &(number, pieces): &(usize, u32)| {
            let flags = tables.items.get(number).map_or(0, |item| item.flags);
            for_a_minion(flags)
                && (1..=MOST_OF_A_THING as u32).contains(&pieces)
                && self.items[..n].iter().all(|entry| entry.0 != number)
        };
        (1..=MOST).contains(&self.members.len())
            && self.members.iter().all(member)
            && self.items.len() <= items::PAIRS
            && self
                .items
                .iter()
                .enumerate()
                .all(|(n, entry)| item(n, entry))
    }

    /// A hash of [`Self::to_bytes`].
    pub fn hash(&self) -> u64 {
        let mut hash = Hash::new();
        hash.bytes(&self.to_bytes());
        hash.0
    }
}

/// The duel of two teams: side 0 is `teams[0]` (player 1, the Minions of the Dawn).
pub fn start(tables: &Tables, teams: [&Team; 2], options: Options, seed: u32) -> Duel {
    Duel::new(
        tables,
        teams.map(Team::records),
        teams.map(|team| team.items.clone()),
        options,
        seed,
    )
}

/// How a side came out of the last fight (`0x800A3A28`: 0 won, 1 beaten, 2 gave up).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
    Won,
    Beaten,
    GaveUp,
}

/// The tally of a series of fights with the same teams (`0x800A3A20`, `0x800A3A28`):
/// each side's lost fights, at most 99, and how each came out of the last one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Series {
    pub lost: [u8; 2],
    pub last: [Option<Ending>; 2],
}

impl Series {
    /// The most lost fights the tally counts.
    pub const MOST_LOST: u8 = 99;

    /// Counts the end of a fight.
    pub fn count(&mut self, winner: usize, gave_up: bool) {
        let loser = 1 - winner;
        self.lost[loser] = (self.lost[loser] + 1).min(Self::MOST_LOST);
        self.last[winner] = Some(Ending::Won);
        self.last[loser] = Some(match gave_up {
            true => Ending::GaveUp,
            false => Ending::Beaten,
        });
    }

    /// Each side's won fights: the other side's lost ones.
    pub fn won(&self) -> [u8; 2] {
        [self.lost[1], self.lost[0]]
    }
}

/// The records of a memory's fifteen as a save holds them, for a test: a creature in
/// each of the first records, the rest free.
pub fn memory_with(creatures: &[Creature], keeping: &Keeping) -> Memory {
    let mut memory = Memory::zeroed();
    for (n, bytes) in memory.creatures.iter_mut().enumerate().take(RECORDS) {
        match creatures.get(n) {
            Some(creature) => creature.write(bytes),
            None => bytes[0..2].copy_from_slice(&state::FREE.to_le_bytes()),
        }
    }
    let kept = keeping.to_bytes();
    memory.block[36..36 + kept.len()].copy_from_slice(&kept);
    memory.block[save::MARK_AT..save::MARK_AT + 4].copy_from_slice(&save::MARK.to_le_bytes());
    memory
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::battle::record::tests::{head, tables as made_up};
    use crate::battle::{Descriptor, Item, class};

    fn tables() -> Tables {
        let mut tables = made_up();
        let effect =
            Descriptor::parse(&[0, 0, 0xCE, 0xFF, 0xFF, 125, 125, 0, class::NONE, 2, 0, 2])
                .expect("a descriptor");
        // Item 0 is for the player only, 1 for a minion, 2 is thrown at the opponent.
        tables.items = [0x4B, 0x53, 0x82]
            .map(|flags| Item { flags, effect })
            .to_vec();
        tables
    }

    fn creature(state: i16, species: u32, hp: i16) -> Creature {
        let mut record = head(&"00".repeat(72));
        (record.max_hp, record.hp, record.max_mp, record.mp) = (hp, hp / 2, 30, 0);
        record.status = 0x0001;
        record.level = 12;
        record.species = species;
        record.equipment = [NOTHING; 3];
        Creature::of_a_caught(state, record)
    }

    fn keeping() -> Keeping {
        let mut keeping = Keeping::new();
        for (number, count) in [(0, 3), (1, 4), (2, 5)] {
            assert!(keeping.give(number, count, Kind::Item, [NOTHING; 3]));
        }
        keeping
    }

    #[test]
    fn a_team_is_made_of_the_picked_records_whole_with_the_items_that_count() {
        let tables = tables();
        let creatures = [
            creature(state::KEPT, 5, 80),
            creature(state::CARRIED, 6, 90),
            creature(state::CARRIED, 7, 100),
        ];
        let memory = memory_with(&creatures, &keeping());
        assert_eq!(listed(&memory).len(), 3);
        // Carried first, then kept.
        assert_eq!(first_three(&memory), [1, 2, 0]);
        let team = Team::from_memory(&tables, &memory, &[2, 0]).expect("a team");
        let species: Vec<u32> = team.members.iter().map(|c| c.record.species).collect();
        assert_eq!(species, [7, 5]);
        for creature in &team.members {
            let record = &creature.record;
            assert_eq!(
                (record.hp, record.mp, record.status),
                (record.max_hp, 30, 0)
            );
        }
        assert_eq!(team.items, [(1, 4)]);
        assert!(team.plausible(&tables));
        // The rules of file 31: not empty, at most three, no free record.
        assert_eq!(
            Team::from_memory(&tables, &memory, &[]),
            Err(TeamError::Empty)
        );
        assert_eq!(
            Team::from_memory(&tables, &memory, &[0, 1, 2, 0]),
            Err(TeamError::TooMany)
        );
        assert_eq!(
            Team::from_memory(&tables, &memory, &[5]),
            Err(TeamError::NoSuchRecord(5))
        );
        assert_eq!(
            Team::from_memory(&tables, &memory, &[15]),
            Err(TeamError::NoSuchRecord(15))
        );
        // Through a save's buffer.
        let mut buffer = vec![0; save::END];
        assert!(memory.write(&mut buffer, &[0; 24], &[[0; 24]; 2]));
        let saved = Team::from_save(&tables, &buffer, &[2, 0]).expect("a team");
        assert_eq!(saved.records(), team.records());
        assert_eq!(saved.items, team.items);
        assert_eq!(
            Team::from_save(&tables, &[0; 16], &[0]),
            Err(TeamError::NotASave)
        );
    }

    #[test]
    fn a_team_travels_as_bytes_and_a_forged_one_is_not_plausible() {
        let tables = tables();
        let memory = memory_with(&[creature(state::CARRIED, 9, 70)], &keeping());
        let team = Team::from_memory(&tables, &memory, &[0, 0]).expect("a team");
        let bytes = team.to_bytes();
        assert_eq!(bytes.len(), 1 + 2 * 248 + 1 + 2);
        assert_eq!(Team::from_bytes(&bytes), Some(team.clone()));
        assert_eq!(
            team.hash(),
            Team::from_bytes(&bytes).expect("a team").hash()
        );
        assert_eq!(Team::from_bytes(&bytes[..bytes.len() - 1]), None);
        let mut forged = team.clone();
        forged.members[0].record.max_hp = 5000;
        assert!(!forged.plausible(&tables));
        let mut forged = team.clone();
        forged.items.push((2, 1));
        assert!(!forged.plausible(&tables));
        let mut forged = team;
        forged.members.clear();
        assert!(!forged.plausible(&tables));
    }

    #[test]
    fn a_series_counts_the_lost_fights_and_the_last_ending() {
        let mut series = Series::default();
        series.count(0, false);
        series.count(1, true);
        series.count(1, false);
        assert_eq!(series.lost, [2, 1]);
        assert_eq!(series.won(), [1, 2]);
        assert_eq!(series.last, [Some(Ending::Beaten), Some(Ending::Won)]);
        series.count(0, true);
        assert_eq!(series.last, [Some(Ending::Won), Some(Ending::GaveUp)]);
    }
}
