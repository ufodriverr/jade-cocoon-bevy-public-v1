
use crate::battle::record::Record;
use crate::mover::ArcTangent;
use crate::skeleton::sine;

/// The records of the state block (`0x8008BE88`, 248 bytes each).
pub const RECORDS: usize = 15;
/// The creatures a fight and the menu know as carried: the slots 1 to 3.
pub const MOST_CARRIED: usize = 3;

/// The state of a record, its first half word.
pub mod state {
    pub const KEPT: i16 = 2;
    /// The player carries it.
    pub const CARRIED: i16 = 3;
    /// The record is free: what "Start Game" writes into all fifteen (`0x800A2824`).
    pub const FREE: i16 = 4;
}

/// The ancestors a record keeps: its two parents, then the two parents of each.
pub const ANCESTORS: usize = 6;
/// The species bytes a record keeps for its figure.
pub const LINE: usize = 48;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub look: u8,
    /// +4: the names of the six ancestors, each kept as a creature's own name is.
    pub names: [[u8; crate::name::LETTERS]; ANCESTORS],
    /// +76: 48 species, the low byte of each, which a merge mixes of both creatures'
    /// and a figure's models and their weights are taken from.
    pub species: [u8; LINE],
    /// +124: the element bits of the six ancestors.
    pub signs: [u8; ANCESTORS],
}

impl Line {
    /// The line of a creature that was caught and never merged (`0x800BA064` of file
    /// 44): it is its own ancestor six times over, and every species is its own.
    pub fn of_a_caught(record: &Record) -> Self {
        let species = record.species as u8;
        Self {
            look: species,
            names: [record.name; ANCESTORS],
            species: [species; LINE],
            signs: [record.element_bit as u8; ANCESTORS],
        }
    }
}

/// A record in use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Creature {
    pub state: i16,
    pub record: Record,
    pub line: Line,
}

impl Creature {
    /// The bytes of a record of the state block.
    pub const BYTES: usize = 248;

    pub fn write(&self, bytes: &mut [u8; Self::BYTES]) {
        let letters = crate::name::LETTERS;
        bytes[0..2].copy_from_slice(&self.state.to_le_bytes());
        (bytes[2], bytes[3]) = (self.record.species as u8, self.line.look);
        let put = |to: &mut [u8], name: &[u8; crate::name::LETTERS]| {
            to[..letters].copy_from_slice(name);
            to[letters..].fill(0);
        };
        for (name, to) in (self.line.names.iter()).zip(bytes[4..76].chunks_exact_mut(12)) {
            put(to, name);
        }
        bytes[76..124].copy_from_slice(&self.line.species);
        bytes[124..130].copy_from_slice(&self.line.signs);
        let head: &mut [u8; Record::HEAD] = (&mut bytes[132..204]).try_into().expect("a head");
        self.record.write_head(head);
        put(&mut bytes[148..160], &self.record.name);
        bytes[163] = crate::battle::record::NOTHING;
        self.record.write_moves(&mut bytes[222..242]);
        bytes[244..248].copy_from_slice(&self.record.spells.to_le_bytes());
    }

    /// The way back of [`Self::write`]; `None` for a record that is free.
    pub fn read(bytes: &[u8; Self::BYTES]) -> Option<Self> {
        let state = i16::from_le_bytes([bytes[0], bytes[1]]);
        let name = |at: usize| std::array::from_fn(|n| bytes[at + n]);
        let record = Record {
            species: u32::from(bytes[2]),
            moves: Record::read_moves(&bytes[222..242]),
            spells: u32::from_le_bytes([bytes[244], bytes[245], bytes[246], bytes[247]]),
            name: name(148),
            ..Record::head(&bytes[132..])?
        };
        let line = Line {
            look: bytes[3],
            names: std::array::from_fn(|k| name(4 + 12 * k)),
            species: std::array::from_fn(|n| bytes[76 + n]),
            signs: std::array::from_fn(|n| bytes[124 + n]),
        };
        (state != state::FREE).then_some(Self {
            state,
            record,
            line,
        })
    }

    /// A creature as the screen of file 44 makes it of a caught one (`0x800BA064`).
    pub fn of_a_caught(state: i16, record: Record) -> Self {
        Self {
            state,
            line: Line::of_a_caught(&record),
            record,
        }
    }
}

/// The fifteen records; a free one holds nothing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Creatures {
    records: [Option<Creature>; RECORDS],
}

impl Creatures {
    /// Every record free, as a new game has them.
    pub fn new_game() -> Self {
        Self::default()
    }

    /// The fifteen records go over their bytes in the game's memory: a creature as
    /// [`Creature::write`] has it, a free record with nothing but its state, as the
    /// reset of "Start Game" leaves it (`0x800A2824`).
    pub fn write(&self, memory: &mut [[u8; Creature::BYTES]; RECORDS]) {
        for (record, bytes) in self.records.iter().zip(memory) {
            match record {
                Some(creature) => creature.write(bytes),
                None => bytes[0..2].copy_from_slice(&state::FREE.to_le_bytes()),
            }
        }
    }

    /// The way back of [`Self::write`].
    pub fn read(memory: &[[u8; Creature::BYTES]; RECORDS]) -> Self {
        Self {
            records: std::array::from_fn(|n| Creature::read(&memory[n])),
        }
    }

    /// The records in a state, in their order.
    fn of(&self, state: i16) -> impl Iterator<Item = &Creature> + '_ {
        (self.records.iter().flatten()).filter(move |creature| creature.state == state)
    }

    /// How many records have a state (`0x80010548`); state 0 counts every record in
    /// use.
    pub fn count(&self, state: i16) -> usize {
        match state {
            0 => self.records.iter().flatten().count(),
            _ => self.of(state).count(),
        }
    }

    /// The n-th record of a state (`0x80010598`, `0x800107A0`).
    pub fn nth(&self, state: i16, n: usize) -> Option<&Creature> {
        self.of(state).nth(n)
    }

    /// The carried creatures, in the order of their slots.
    pub fn carried(&self) -> impl Iterator<Item = &Record> + '_ {
        self.of(state::CARRIED).map(|creature| &creature.record)
    }

    /// The n-th record of a state, to be changed in place.
    pub fn nth_mut(&mut self, state: i16, n: usize) -> Option<&mut Creature> {
        (self.records.iter_mut().flatten())
            .filter(|creature| creature.state == state)
            .nth(n)
    }

    /// The carried creatures, to be changed in place.
    pub fn carried_mut(&mut self) -> impl Iterator<Item = &mut Record> + '_ {
        (self.records.iter_mut().flatten())
            .filter(|creature| creature.state == state::CARRIED)
            .map(|creature| &mut creature.record)
    }

    /// The mask of the carried creatures as a fight's menu notes it (`0x800C0C6C`): a
    /// bit for each of the slots 1 to as many as are carried.
    pub fn carried_mask(&self) -> u8 {
        (1..=self.count(state::CARRIED)).fold(0, |mask, slot| mask | 1 << slot)
    }

    /// Puts a creature into the first free record, in a state; `None` when all fifteen
    /// are in use. HYPOTHESIS: the game takes the first free record too (the routines
    /// that fill one, in the merge's file, are not read for it).
    pub fn put(&mut self, state: i16, record: Record) -> Option<usize> {
        let free = self.records.iter().position(Option::is_none)?;
        self.records[free] = Some(Creature::of_a_caught(state, record));
        Some(free)
    }
}

/// The places of the list the screen of file 44 works on: three for the carried
/// creatures and twenty-four behind them.
pub const PLACES: usize = 27;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    places: [Option<Creature>; PLACES],
}

impl Listed {
    /// The start of the screen: the carried creatures take the places 0 to 2 in their
    /// order, the kept ones the places from 3 on, and behind them every caught
    /// creature that waits becomes a kept one (`0x800BA064` with state 2). Then all
    /// fifteen records and all twelve caught ones are emptied (`0x80010600`): until
    /// the screen leaves, the list is all there is.
    pub fn open(creatures: &mut Creatures, caught: &mut Caught) -> Self {
        let mut places: [Option<Creature>; PLACES] = std::array::from_fn(|_| None);
        let carried = creatures.of(state::CARRIED).take(MOST_CARRIED).cloned();
        for (place, creature) in places.iter_mut().zip(carried) {
            *place = Some(creature);
        }
        let kept = creatures.of(state::KEPT).cloned();
        let made =
            (caught.waiting()).map(|record| Creature::of_a_caught(state::KEPT, record.clone()));
        for (place, creature) in places[MOST_CARRIED..].iter_mut().zip(kept.chain(made)) {
            *place = Some(creature);
        }
        (*creatures, *caught) = (Creatures::default(), Caught::default());
        Self { places }
    }

    /// The state of a place (`0x800B9DE0`), [`state::FREE`] for a free one.
    pub fn state(&self, place: usize) -> i16 {
        (self.places.get(place).and_then(Option::as_ref)).map_or(state::FREE, |c| c.state)
    }

    /// The creature of a place (`0x800B9E2C`), nothing for a free one.
    pub fn get(&self, place: usize) -> Option<&Creature> {
        self.places.get(place).and_then(Option::as_ref)
    }

    /// The record of a place's creature, to be written to.
    pub fn record_mut(&mut self, place: usize) -> Option<&mut Record> {
        Some(&mut self.places.get_mut(place)?.as_mut()?.record)
    }

    /// A creature is written over a place, as the yes of a merge copies the new one
    /// over the first of the two (`0x800BDC3C`).
    pub fn put(&mut self, place: usize, creature: Creature) {
        if let Some(at) = self.places.get_mut(place) {
            *at = Some(creature);
        }
    }

    /// How many places have a state (`0x800B9E48`).
    pub fn count(&self, state: i16) -> usize {
        self.in_use().filter(|(_, c)| c.state == state).count()
    }

    /// The places in use, with their creatures.
    pub fn in_use(&self) -> impl Iterator<Item = (usize, &Creature)> + '_ {
        (self.places.iter().enumerate()).filter_map(|(place, c)| Some((place, c.as_ref()?)))
    }

    /// A kept creature goes with the player (`0x800B9EA8`): its state becomes 3 when
    /// fewer than three of the list have it. False when nothing was changed.
    pub fn take_along(&mut self, place: usize) -> bool {
        let carried = self.in_use().filter(|(_, c)| c.state == state::CARRIED);
        if carried.count() >= MOST_CARRIED {
            return false;
        }
        match self.places.get_mut(place) {
            Some(Some(creature)) if creature.state == state::KEPT => {
                creature.state = state::CARRIED;
                true
            }
            _ => false,
        }
    }

    /// The leaving of the screen: with fewer than sixteen places in use they are
    /// written to the records in the order of the list, and the list is gone. With
    /// more the screen cannot leave and the list is handed back.
    pub fn close(self, creatures: &mut Creatures) -> Result<(), Box<Self>> {
        if self.in_use().count() > RECORDS {
            return Err(Box::new(self));
        }
        *creatures = Creatures::default();
        for (record, creature) in creatures
            .records
            .iter_mut()
            .zip(self.places.into_iter().flatten())
        {
            *record = Some(creature);
        }
        Ok(())
    }

    pub fn exchange(&mut self, first: usize, second: usize) -> bool {
        let list = |place: usize| match place < MOST_CARRIED {
            true => state::CARRIED,
            false => state::KEPT,
        };
        if first >= PLACES || second >= PLACES {
            return false;
        }
        let (was, other) = (self.state(first), self.state(second));
        if ![state::KEPT, state::CARRIED, state::FREE].contains(&was) {
            return false;
        }
        let both = [state::KEPT, state::CARRIED];
        let (to_first, to_second) = match (was, other) {
            // A carried and a kept one, either way round.
            _ if was != other && both.contains(&was) && both.contains(&other) => {
                (Some(other), Some(was))
            }
            // A creature and a free place of the other list.
            _ if both.contains(&was) && other == state::FREE && list(second) != was => {
                (Some(list(second)), None)
            }
            // A free place and a creature of its own list that goes to the other.
            _ if was == state::FREE
                && both.contains(&other)
                && list(second) == other
                && list(first) != other =>
            {
                (None, Some(list(first)))
            }
            _ => (None, None),
        };
        for (place, state) in [(first, to_first), (second, to_second)] {
            if let (Some(creature), Some(state)) = (self.places[place].as_mut(), state) {
                creature.state = state;
            }
        }
        self.places.swap(first, second);
        true
    }

    /// "Rearrange" (`0x800B9980`): every free place from the fourth on gets the next
    /// kept creature behind it (`0x800B9A58`), until none is left behind a free
    /// place. The three carried places are left alone. True when a creature moved.
    pub fn close_gaps(&mut self) -> bool {
        let mut moved = false;
        for place in MOST_CARRIED..PLACES {
            if self.places[place].is_some() {
                continue;
            }
            let next = (place..PLACES).find(|&behind| self.state(behind) == state::KEPT);
            let Some(next) = next else {
                break;
            };
            self.places.swap(place, next);
            moved = true;
        }
        moved
    }

    /// Takes a place out of the list (`0x800B9CF0`: its state becomes 4).
    pub fn free(&mut self, place: usize) -> Option<Creature> {
        self.places.get_mut(place)?.take()
    }
}

/// The places of the four elements on the circle of the colours, in steps of the sine
/// table (4096 to the circle): 65, 155, 245 and 335 degrees.
const ELEMENT_STEPS: [i32; 4] = [739, 1763, 2787, 3811];

pub fn colour_turn(strengths: [u8; 4], row: [u8; 2], arc: &ArcTangent) -> i32 {
    let [fire, wind, earth, water] = strengths.map(i32::from);
    let angle = match strengths.iter().filter(|strength| **strength != 0).count() {
        1 => return 2 * i32::from(row[0]),
        2 if water != 0 && wind != 0 => match wind < water {
            true => 245,
            false => 65,
        },
        2 if fire != 0 && earth != 0 => match earth < fire {
            true => 335,
            false => 155,
        },
        _ => {
            let [a, b, c, d] = ELEMENT_STEPS.map(sine);
            let first = a * fire + d * water + c * earth + b * wind;
            let second = d * fire + c * water + b * earth + a * wind;
            // The library's arc tangent, brought into a full turn and into degrees.
            let turn = i32::from(arc.angle(second, first)).rem_euclid(4096);
            match (turn * 360) >> 12 {
                degrees @ 0..360 => degrees,
                _ => 0,
            }
        }
    };
    match angle - 2 * i32::from(row[1]) {
        turn if turn < 0 => turn + 360,
        turn => turn,
    }
}

pub const CAUGHT: usize = 12;

/// The twelve records of the caught creatures; an empty one holds nothing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Caught {
    records: [Option<Record>; CAUGHT],
}

impl Caught {
    /// The twelve records as their bytes in the game's memory: a caught creature as
    /// [`Record::to_bytes`] has it, an empty record with nothing but species 211 in
    /// its word, as the reset of "Start Game" leaves it.
    pub fn write(&self, memory: &mut [[u8; Record::BYTES]; CAUGHT]) {
        for (record, bytes) in self.records.iter().zip(memory) {
            match record {
                Some(record) => *bytes = record.to_bytes(),
                None => bytes[0x5C..0x60].copy_from_slice(&crate::battle::EMPTY.to_le_bytes()),
            }
        }
    }

    /// The way back of [`Self::write`].
    pub fn read(memory: &[[u8; Record::BYTES]; CAUGHT]) -> Self {
        Self {
            records: std::array::from_fn(|n| Record::from_bytes(&memory[n])),
        }
    }

    /// How many records are empty.
    pub fn free(&self) -> usize {
        self.records
            .iter()
            .filter(|record| record.is_none())
            .count()
    }

    /// A caught creature goes into the first empty record as it stands (`0x80010650`);
    /// `None` when all twelve are taken, and the creature is lost.
    pub fn put(&mut self, record: Record) -> Option<usize> {
        let free = self.records.iter().position(Option::is_none)?;
        self.records[free] = Some(record);
        Some(free)
    }

    /// The caught creatures that wait, in the order of their records.
    pub fn waiting(&self) -> impl Iterator<Item = &Record> + '_ {
        self.records.iter().flatten()
    }

    /// The same, to be changed in place.
    pub fn each_mut(&mut self) -> impl Iterator<Item = &mut Record> + '_ {
        self.records.iter_mut().flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_caught_creature_takes_the_first_empty_record_of_twelve() {
        let mut caught = Caught::default();
        assert_eq!(caught.free(), CAUGHT);
        for n in 0..CAUGHT {
            assert_eq!(caught.put(record(n as u32)), Some(n));
        }
        assert_eq!((caught.free(), caught.put(record(99))), (0, None));
        let species: Vec<u32> = caught.waiting().map(|record| record.species).collect();
        assert_eq!(species, (0..12).collect::<Vec<u32>>());
    }

    #[test]
    fn the_colours_of_a_figure_turn_by_its_elements() {
        // A table that answers 0 for every ratio: the arc tangent is a quarter turn's
        // corner, which is enough to tell the branches apart.
        let arc = ArcTangent::of(vec![0; 1025]);
        // One strength: the species' own turn, whatever the second byte is.
        assert_eq!(colour_turn([0, 0, 32, 0], [45, 7], &arc), 90);
        // Two across: wind with water, fire with earth, by which is the stronger.
        assert_eq!(colour_turn([0, 9, 0, 8], [0, 0], &arc), 65);
        assert_eq!(colour_turn([0, 8, 0, 9], [0, 0], &arc), 245);
        assert_eq!(colour_turn([9, 0, 8, 0], [0, 0], &arc), 335);
        assert_eq!(colour_turn([8, 0, 9, 0], [0, 0], &arc), 155);
        // The second byte is taken off twice, and a value below 0 goes round.
        assert_eq!(colour_turn([0, 9, 0, 8], [0, 40], &arc), 345);
    }

    #[test]
    fn the_screen_makes_the_caught_his_own_and_writes_the_list_back() {
        let mut creatures = Creatures::new_game();
        creatures.put(state::KEPT, record(1));
        creatures.put(state::CARRIED, record(2));
        let mut caught = Caught::default();
        caught.put(record(3));
        caught.put(record(4));
        let mut list = Listed::open(&mut creatures, &mut caught);
        // The game's own records are empty while the screen is up.
        assert_eq!((creatures.count(0), caught.free()), (0, CAUGHT));
        // The carried one first, two free places, then the kept one and the caught.
        let listed: Vec<(usize, i16, u32)> = (list.in_use())
            .map(|(place, c)| (place, c.state, c.record.species))
            .collect();
        assert_eq!(listed, [(0, 3, 2), (3, 2, 1), (4, 2, 3), (5, 2, 4)]);
        assert_eq!((list.state(1), list.state(99)), (state::FREE, state::FREE));
        // Two more go with him, a third does not: three are carried.
        assert!(list.take_along(4) && list.take_along(5));
        assert!(!list.take_along(3) && !list.take_along(0) && !list.take_along(1));
        assert!(list.close(&mut creatures).is_ok());
        let carried: Vec<u32> = creatures.carried().map(|record| record.species).collect();
        assert_eq!(carried, [2, 3, 4]);
        assert_eq!((creatures.count(0), creatures.count(state::KEPT)), (4, 1));
    }

    #[test]
    fn a_list_with_more_than_fifteen_cannot_leave() {
        let mut creatures = Creatures::new_game();
        for n in 0..10 {
            creatures.put(state::KEPT, record(n));
        }
        let mut caught = Caught::default();
        for n in 0..6 {
            caught.put(record(20 + n));
        }
        let list = Listed::open(&mut creatures, &mut caught);
        let mut list = *list.close(&mut creatures).expect_err("sixteen in use");
        assert_eq!(creatures.count(0), 0);
        // One is let go, and the list fits the fifteen records.
        assert_eq!(list.free(18).map(|c| c.record.species), Some(25));
        assert!(list.close(&mut creatures).is_ok());
        assert_eq!(creatures.count(state::KEPT), 15);
    }

    #[test]
    fn two_picked_places_change_with_each_other() {
        let mut creatures = Creatures::new_game();
        creatures.put(state::CARRIED, record(1));
        creatures.put(state::KEPT, record(2));
        creatures.put(state::KEPT, record(3));
        let mut list = Listed::open(&mut creatures, &mut Caught::default());
        let listed = |list: &Listed| -> Vec<(usize, i16, u32)> {
            (list.in_use())
                .map(|(place, c)| (place, c.state, c.record.species))
                .collect()
        };
        assert_eq!(listed(&list), [(0, 3, 1), (3, 2, 2), (4, 2, 3)]);
        // A carried and a kept one change places and states.
        assert!(list.exchange(0, 3));
        assert_eq!(listed(&list), [(0, 3, 2), (3, 2, 1), (4, 2, 3)]);
        // A kept one to a free place of the carried: it goes with him.
        assert!(list.exchange(4, 1));
        assert_eq!(listed(&list), [(0, 3, 2), (1, 3, 3), (3, 2, 1)]);
        // A free place of the carried picked first, then a kept one: the same.
        assert!(list.exchange(2, 3));
        assert_eq!(listed(&list), [(0, 3, 2), (1, 3, 3), (2, 3, 1)]);
        // A carried one to a free place of the kept: it stays behind.
        assert!(list.exchange(1, 10));
        assert_eq!(listed(&list), [(0, 3, 2), (2, 3, 1), (10, 2, 3)]);
        // Inside one list only the places change.
        assert!(list.exchange(0, 1));
        assert_eq!(listed(&list), [(1, 3, 2), (2, 3, 1), (10, 2, 3)]);
        assert!(list.exchange(10, 3));
        assert_eq!(listed(&list), [(1, 3, 2), (2, 3, 1), (3, 2, 3)]);
        // Two free places, and a place the list does not have.
        assert!(list.exchange(5, 6) && !list.exchange(0, PLACES));
        assert_eq!(listed(&list), [(1, 3, 2), (2, 3, 1), (3, 2, 3)]);
        // "Rearrange" closes the gaps of the kept places and leaves the carried.
        assert!(!list.close_gaps());
        assert!(list.exchange(3, 9) && list.exchange(2, 20));
        assert_eq!(listed(&list), [(1, 3, 2), (9, 2, 3), (20, 2, 1)]);
        assert!(list.close_gaps());
        assert_eq!(listed(&list), [(1, 3, 2), (3, 2, 3), (4, 2, 1)]);
        assert!(!list.close_gaps());
    }

    fn record(species: u32) -> Record {
        let mut record = Record::head(&[0u8; Record::HEAD]).expect("a head");
        record.species = species;
        record
    }

    #[test]
    fn the_carried_are_the_records_in_state_three_in_their_order() {
        let mut creatures = Creatures::new_game();
        assert_eq!((creatures.count(0), creatures.carried_mask()), (0, 0));
        assert_eq!(creatures.put(state::CARRIED, record(5)), Some(0));
        assert_eq!(creatures.put(1, record(6)), Some(1));
        assert_eq!(creatures.put(state::CARRIED, record(7)), Some(2));
        // Two carried, in the slots 1 and 2; the one in another state is not among them.
        assert_eq!(creatures.count(state::CARRIED), 2);
        assert_eq!(creatures.count(0), 3);
        assert_eq!(creatures.carried_mask(), 0b110);
        let species: Vec<u32> = creatures.carried().map(|record| record.species).collect();
        assert_eq!(species, [5, 7]);
        let second = creatures.nth(state::CARRIED, 1).expect("the second");
        assert_eq!(second.record.species, 7);
        assert!(creatures.nth(state::CARRIED, 2).is_none());
        // A change through the carried reaches the record.
        for record in creatures.carried_mut() {
            record.hp = 9;
        }
        assert_eq!(
            creatures.nth(state::CARRIED, 0).map(|c| c.record.hp),
            Some(9)
        );
        assert_eq!(creatures.nth(1, 0).map(|c| c.record.hp), Some(0));
        // Fifteen records, and no more.
        for _ in 0..12 {
            assert!(creatures.put(1, record(0)).is_some());
        }
        assert_eq!(creatures.put(1, record(0)), None);
    }
}
