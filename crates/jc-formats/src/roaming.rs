
use crate::battle::fight::{Block, Spec};
use crate::battle::record::Draws;
use crate::battle::{Base, EMPTY};
use crate::rooms::{self, FIELD_ENGINE_BASE, Image, RoomTableError};

/// RAM addresses of the tables in file 27: the lists of room numbers that name a row,
/// the rows, the lists of room numbers that name a zone, and the zones' species.
pub const ROOM_LISTS: u32 = 0x800A_EBAC;
pub const ROWS: u32 = 0x800A_E6F4;
pub const ZONE_LISTS: u32 = 0x800A_E5A0;
pub const ZONE_SPECIES: u32 = 0x800A_E6B0;
/// How many lists of rooms and of zones there are.
pub const ROOMS: usize = 73;
pub const ZONES: usize = 17;
/// The places of a room that can have a creature, and the groups of a row.
pub const PLACES: usize = 3;
pub const GROUPS: usize = 3;
/// The species that stands for nobody in the list of four.
pub const NOBODY: u32 = EMPTY;
/// The first element variant: species * 4 + element + 57.
const VARIANTS: u32 = 57;
/// Bit 15 of a room's word: fill the room anew.
pub const FILL: u16 = 0x8000;
/// Bit 4 of a place's five bits: a creature stands here. Bits 0 and 1 are its group.
pub const STANDS: u16 = 0x10;
/// What the weights of a row are drawn against.
const WEIGHTS: u32 = 16;
/// The forests with a table of words: area, place, and the words of the table (one for
/// each slot of a room), in the order of the state block.
pub const FORESTS: [(usize, usize, usize); 7] = [
    (4, 1, 12),
    (4, 2, 18),
    (4, 3, 44),
    (4, 4, 22),
    (5, 1, 10),
    (5, 2, 16),
    (5, 3, 42),
];

/// One group word of a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Group(pub u32);

impl Group {
    /// The weight, of 16.
    pub fn weight(self) -> u32 {
        self.0 & 0xFF
    }

    /// Opponent `n` (0 to 2): its index into the list of four species and its
    /// experience word, (level - 1) << 26.
    pub fn opponent(self, n: usize) -> (usize, u32) {
        let bits = self.0 >> (8 + 8 * n);
        ((bits & 7) as usize, (bits >> 3 & 31) << 26)
    }

    /// The three specs of the group for a list of four species. An index past the
    /// list reads what lies behind it in the game; no row of the disc has one.
    fn specs(self, list: &[u32; 4]) -> [Spec; GROUPS] {
        std::array::from_fn(|n| {
            let (index, experience) = self.opponent(n);
            Spec {
                species: list.get(index).copied().unwrap_or(NOBODY),
                experience,
                ai: 0,
            }
        })
    }
}

/// The tables of file 27.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tables {
    /// The room numbers of each row.
    rooms: Vec<Vec<i16>>,
    rows: Vec<[Group; GROUPS]>,
    /// The room numbers of each zone, and the zone's three species.
    zones: Vec<Vec<i16>>,
    species: Vec<Option<[u32; 3]>>,
}

impl Tables {
    /// Reads the tables from the field engine file as it is on the disc.
    pub fn from_field_engine(file: &[u8]) -> Result<Self, RoomTableError> {
        Self::parse(file, FIELD_ENGINE_BASE)
    }

    /// Reads the tables from an image whose first byte is at RAM address `base`.
    pub fn parse(data: &[u8], base: u32) -> Result<Self, RoomTableError> {
        let image = Image { data, base };
        // A list of 16-bit numbers, ended by -1.
        let list = |at: u32| {
            let mut numbers = Vec::new();
            loop {
                match image.i16(rooms::step(at, numbers.len(), 2)?)? {
                    -1 => return Ok(numbers),
                    number => numbers.push(number),
                }
            }
        };
        let lists = |at: u32, count: usize| {
            (0..count)
                .map(|index| list(image.u32(rooms::step(at, index, 4)?)?))
                .collect::<Result<Vec<_>, RoomTableError>>()
        };
        let rows = (0..ROOMS)
            .map(|row| {
                let mut groups = [Group::default(); GROUPS];
                for (group, word) in groups.iter_mut().enumerate() {
                    *word = Group(image.u32(rooms::step(ROWS, GROUPS * row + group, 4)?)?);
                }
                Ok(groups)
            })
            .collect::<Result<_, RoomTableError>>()?;
        let species = (0..ZONES)
            .map(
                |zone| match image.u32(rooms::step(ZONE_SPECIES, zone, 4)?)? {
                    0 => Ok(None),
                    at => {
                        let mut three = [0; 3];
                        for (n, species) in three.iter_mut().enumerate() {
                            *species = image.u32(rooms::step(at, n, 4)?)?;
                        }
                        Ok(Some(three))
                    }
                },
            )
            .collect::<Result<_, RoomTableError>>()?;
        Ok(Self {
            rooms: lists(ROOM_LISTS, ROOMS)?,
            rows,
            zones: lists(ZONE_LISTS, ZONES)?,
            species,
        })
    }

    /// The lookup `0x800A527C`: the first list that holds a number.
    fn first(lists: &[Vec<i16>], number: i32) -> Option<usize> {
        (lists.iter()).position(|list| list.iter().any(|&listed| i32::from(listed) == number))
    }

    /// The row of a room, by its number.
    pub fn row_of(&self, room: i32) -> Option<&[Group; GROUPS]> {
        self.rows.get(Self::first(&self.rooms, room)?)
    }

    /// The zone of a room, by its number.
    pub fn zone_of(&self, room: i32) -> Option<usize> {
        Self::first(&self.zones, room)
    }

    /// The three species of a zone.
    pub fn species_of(&self, zone: usize) -> Option<[u32; 3]> {
        *self.species.get(zone)?
    }

    /// Every row a list of rooms names, with those rooms.
    pub fn rows(&self) -> impl Iterator<Item = (&[i16], &[Group; GROUPS])> {
        (self.rooms.iter())
            .zip(&self.rows)
            .map(|(rooms, row)| (rooms.as_slice(), row))
    }
}

/// The species of a list that the room loader notes models for: the first three
/// (`0x800B8B10`, which does not ask whether a species is nobody).
pub const NOTED: usize = 3;

pub fn noted(list: &[u32; 4], model: impl Fn(u32) -> Option<(u32, i32)>) -> Vec<(u32, i32)> {
    let mut noted: Vec<(u32, i32)> = Vec::new();
    for (model, turn) in list
        .iter()
        .take(NOTED)
        .filter_map(|species| model(*species))
    {
        if noted.iter().all(|(known, _)| *known != model) {
            noted.push((model, turn));
        }
    }
    noted
}

pub fn reach(scale: i32) -> i32 {
    scale.wrapping_mul(400) / 4096
}

/// A creature the picking puts into a room.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stand {
    /// Its number in the room: its place in the picking's answer. The fight forgets a
    /// creature by this number, which is the number of its place only while no place in
    /// front of it is empty.
    pub number: usize,
    /// The record of the room's position table it stands on.
    pub place: usize,
    /// Its group in the room's row.
    pub group: usize,
    /// The three opponents of a fight with it; the first is the creature that roams.
    pub specs: [Spec; GROUPS],
}

impl Stand {
    /// The head of the fight block the maker `0x800B8738` builds for the creature and
    /// its opponents: mode 2, three on the field at once, a retreat allowed, the
    /// results screen. The AI types are 0, "by species".
    pub fn block(&self) -> Block {
        Block {
            mode: 2,
            on_field: GROUPS,
            retreat: true,
            results: true,
            specs: (self.specs.iter().copied())
                .take_while(|spec| spec.species < NOBODY)
                .collect(),
        }
    }
}

/// What the game keeps of the roaming creatures from room to room: the seven tables of
/// the state block, the word `0x8008B4E4` and the list of four species at `0x800AED3C`
/// (which is the field engine's own memory, not the state block's).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Roaming {
    /// One table for each entry of [`FORESTS`], one word for each slot of a room.
    words: [Vec<u16>; FORESTS.len()],
    /// The word `0x8008B4E4` is not 0: the next picking of the species takes the plain
    /// ones, else their variants.
    pub plain: bool,
    /// The species the groups of a row name by index; the fourth is nobody.
    pub list: [u32; 4],
}

impl Default for Roaming {
    fn default() -> Self {
        Self::new_game()
    }
}

impl Roaming {
    /// The state behind "Start Game" (`0x800A2914`): every room is to be filled anew,
    /// and the first picking of the species takes the plain ones. The list is as file 27
    /// has it on the disc: nobody four times.
    pub fn new_game() -> Self {
        Self {
            words: FORESTS.map(|(_, _, slots)| vec![FILL; slots]),
            plain: true,
            list: [NOBODY; 4],
        }
    }

    /// The bytes of the seven tables in the state block (+2104, `0x8008BD38`).
    pub const SAVED: usize = 328;

    pub fn tables_to_bytes(&self) -> [u8; Self::SAVED] {
        let mut bytes = [0; Self::SAVED];
        let words = self.words.iter().flatten();
        for (word, to) in words.zip(bytes.chunks_exact_mut(2)) {
            to.copy_from_slice(&word.to_le_bytes());
        }
        bytes
    }

    /// The way back of [`Self::tables_to_bytes`]; the word `0x8008B4E4` and the list
    /// of species are not in the block and stay.
    pub fn tables_from_bytes(&mut self, bytes: &[u8; Self::SAVED]) {
        let words = self.words.iter_mut().flatten();
        for (word, from) in words.zip(bytes.chunks_exact(2)) {
            *word = u16::from_le_bytes([from[0], from[1]]);
        }
    }

    /// The table of a forest (`0x800A535C`).
    fn table(area: usize, place: usize) -> Option<usize> {
        FORESTS
            .iter()
            .position(|&(of_area, of_place, _)| (of_area, of_place) == (area, place))
    }

    /// The word of a room's slot. A slot past its table has none here; the game does
    /// not test it, and no room of the disc has one.
    pub fn word(&self, area: usize, place: usize, slot: i16) -> Option<u16> {
        let slot = usize::try_from(slot).ok()?;
        self.words[Self::table(area, place)?].get(slot).copied()
    }

    /// Writes the word of every slot of a forest, for a look at its rooms as earlier
    /// visits left them (0: every creature is gone). The answer is whether the place
    /// is a forest with a table.
    pub fn set_forest(&mut self, area: usize, place: usize, to: u16) -> bool {
        let table = Self::table(area, place);
        if let Some(table) = table {
            self.words[table].fill(to);
        }
        table.is_some()
    }

    /// The word of a room's slot, to change.
    pub fn word_mut(&mut self, area: usize, place: usize, slot: i16) -> Option<&mut u16> {
        let slot = usize::try_from(slot).ok()?;
        self.words[Self::table(area, place)?].get_mut(slot)
    }

    /// The list of species when a room is loaded (`0x800A4FC0(room, room before,
    /// force)`; -1 for no room before). Nothing happens unless `force` is set or the two
    /// rooms lie in different zones (`0x800A51D0`). Then, for a room whose zone has
    /// species, each one with an element is written to the list as it is or as its
    /// variant, and the word flips. `bases` are the base records of the species
    /// (`battle::Tables::bases`), for the element. The answer is the new list, which is
    /// also what makes the loader note the models of its species.
    pub fn enter(
        &mut self,
        tables: &Tables,
        bases: &[Base],
        room: i32,
        before: i32,
        force: bool,
    ) -> Option<[u32; 4]> {
        let zone = tables.zone_of(room);
        let differs = before == -1 || zone != tables.zone_of(before);
        if !(force || differs) {
            return None;
        }
        let species = tables.species_of(zone?)?;
        for (listed, species) in self.list.iter_mut().zip(species) {
            let element = (bases.get(species as usize)).map_or(-1, |base| i32::from(base.element));
            *listed = match element {
                -1 => species,
                _ if self.plain => species,
                element => (species * 4).wrapping_add_signed((element + 1) % 4) + VARIANTS,
            };
        }
        self.list[3] = NOBODY;
        self.plain = !self.plain;
        Some(self.list)
    }

    /// The picking (`0x800A495C`) for the room `room` in the slot `slot` of a forest's
    /// place, with `places` records in its position table. A room without a table of
    /// words or without a row has nobody.
    ///
    /// 1. Bit 15 of the word: it is cleared and every place (three at most) gets a
    ///    creature, each with a draw for its group.
    /// 2. The word is 0: with no place nobody; a draw that divides by 3 gives nobody;
    ///    else a second draw picks the place of one creature, and a third its group.
    /// 3. Else the creatures the word remembers come back: the same places and groups,
    ///    with the species of the list as it is now.
    pub fn pick(
        &mut self,
        tables: &Tables,
        (area, place, slot): (usize, usize, i16),
        room: i32,
        places: usize,
        draws: &mut impl Draws,
    ) -> Vec<Stand> {
        let list = self.list;
        let (Some(word), Some(row)) = (self.word_mut(area, place, slot), tables.row_of(room))
        else {
            return Vec::new();
        };
        pick_in(word, row, &list, places, draws)
    }

    /// The forgetting (`0x800A48D0(number, slot)`), when the last opponent of a fight
    /// with a roaming creature is gone: the five bits of place `number` of the room's
    /// word are cleared. `number` is the creature's number in the picking's answer
    /// ([`Stand::number`]), not its place.
    pub fn forget(&mut self, (area, place, slot): (usize, usize, i16), number: usize) {
        if let Some(word) = self.word_mut(area, place, slot) {
            forget_in(word, number);
        }
    }
}

/// The forgetting on the word of a room, wherever the word is kept.
pub fn forget_in(word: &mut u16, number: usize) {
    if number < PLACES {
        *word &= !(31 << (5 * number));
    }
}

pub fn pick_in(
    word: &mut u16,
    row: &[Group; GROUPS],
    list: &[u32; 4],
    places: usize,
    draws: &mut impl Draws,
) -> Vec<Stand> {
    let mut stands = Vec::new();
    // `0x800A4D60`: a draw of 16 against the weights in their order. A draw no
    // weight takes makes nobody (the game counts a creature all the same; the
    // weights of every row of the disc add up to 16).
    let mut group_of = |word: &mut u16, drawn: u32, on: usize, number: usize| {
        let mut drawn = drawn % WEIGHTS;
        for (group, words) in row.iter().enumerate() {
            if drawn < words.weight() {
                *word |= (group as u16 | STANDS) << (5 * on);
                stands.push(Stand {
                    number,
                    place: on,
                    group,
                    specs: words.specs(list),
                });
                return;
            }
            drawn -= words.weight();
        }
    };
    if *word & FILL != 0 {
        *word &= !FILL;
        for on in 0..places.min(PLACES) {
            group_of(word, draws.draw(), on, on);
        }
    } else if *word == 0 {
        if places == 0 || draws.draw().is_multiple_of(3) {
            return Vec::new();
        }
        let on = draws.draw() as usize % places.min(PLACES);
        group_of(word, draws.draw(), on, 0);
    } else {
        let mut bits = *word;
        for on in 0..PLACES {
            if let Some(words) = row.get(usize::from(bits & 3))
                && bits & STANDS != 0
            {
                stands.push(Stand {
                    number: stands.len(),
                    place: on,
                    group: usize::from(bits & 3),
                    specs: words.specs(list),
                });
            }
            bits >>= 5;
        }
    }
    stands
}

#[cfg(test)]
mod tests {
    use super::*;

    const BEETLES: (usize, usize, i16) = (4, 1, 2);

    /// A group word: the weight and up to three opponents as list index and level.
    fn group(weight: u32, opponents: &[(u32, u32)]) -> Group {
        let mut word = weight;
        for n in 0..3 {
            let (index, level) = opponents.get(n).copied().unwrap_or((3, 1));
            word |= (index | (level - 1) << 3) << (8 + 8 * n);
        }
        Group(word)
    }

    /// Two zones (rooms 10 and 11, room 20) and three rows (rooms 10, 11 and 20).
    fn tables() -> Tables {
        Tables {
            rooms: vec![vec![10], vec![11], vec![20]],
            rows: vec![
                [group(16, &[(2, 1)]), Group(0), Group(0)],
                [
                    group(8, &[(0, 1), (2, 4)]),
                    group(6, &[(1, 2)]),
                    group(2, &[(1, 1), (1, 1), (0, 3)]),
                ],
                [group(16, &[(0, 1)]), Group(0), Group(0)],
            ],
            zones: vec![vec![10, 11], vec![20]],
            species: vec![Some([15, 12, 6]), Some([18, 0, 6])],
        }
    }

    /// Base records: species 6 has element 3, 12, 15 and 18 element 0, 0 element 1, and
    /// 5 none.
    fn bases() -> Vec<Base> {
        (0..20)
            .map(|species| Base {
                growth: [0; 6],
                element: match species {
                    5 => -1,
                    6 => 3,
                    0 => 1,
                    _ => 0,
                },
                body: 0,
            })
            .collect()
    }

    /// Draws handed in, in their order.
    fn draws(numbers: &[u32]) -> impl Draws + use<> {
        let mut numbers: std::collections::VecDeque<u32> = numbers.iter().copied().collect();
        move || numbers.pop_front().expect("a draw too many")
    }

    #[test]
    fn the_models_of_the_first_three_species_are_noted_once_each() {
        // Species s has model 100 + s / 2 and the turn 2 s: 12 and 13 share a model.
        let model =
            |species: u32| (species < 200).then_some((100 + species / 2, 2 * species as i32));
        assert_eq!(
            noted(&[15, 12, 6, NOBODY], model),
            [(107, 30), (106, 24), (103, 12)]
        );
        // The second species of a model keeps the note of the first, and the fourth
        // species of a list is not noted.
        assert_eq!(noted(&[12, 13, 6, 40], model), [(106, 24), (103, 12)]);
        // A species without a record is left out.
        assert_eq!(noted(&[NOBODY, 6, NOBODY, NOBODY], model), [(103, 12)]);
    }

    #[test]
    fn a_group_word_holds_a_weight_and_three_opponents() {
        let word = group(8, &[(0, 1), (2, 4)]);
        assert_eq!(word.weight(), 8);
        assert_eq!(word.opponent(0), (0, 0));
        assert_eq!(word.opponent(1), (2, 3 << 26));
        assert_eq!(word.opponent(2), (3, 0));
        let specs = word.specs(&[15, 12, 6, NOBODY]);
        assert_eq!(specs.map(|spec| spec.species), [15, 6, NOBODY]);
        assert_eq!(specs.map(|spec| spec.ai), [0; 3]);
        // The highest level of a word.
        assert_eq!(Group(0xF800_0000).opponent(2), (0, 31 << 26));
    }

    #[test]
    fn the_tables_are_read_from_an_image() {
        // The image starts at the zones' lists; the lists themselves lie behind the
        // room lists' pointers.
        let base = ZONE_LISTS;
        let end = (ROOM_LISTS - base) as usize + 4 * ROOMS;
        let mut image = vec![0u8; end];
        let put = |image: &mut Vec<u8>, numbers: &[i16]| {
            let at = base + image.len() as u32;
            for number in numbers.iter().chain(&[-1]) {
                image.extend_from_slice(&number.to_le_bytes());
            }
            at
        };
        let nothing = put(&mut image, &[]);
        let word = |image: &mut Vec<u8>, at: u32, value: u32| {
            let at = (at - base) as usize;
            image[at..at + 4].copy_from_slice(&value.to_le_bytes());
        };
        for zone in 0..ZONES {
            word(&mut image, ZONE_LISTS + 4 * zone as u32, nothing);
        }
        for row in 0..ROOMS {
            word(&mut image, ROOM_LISTS + 4 * row as u32, nothing);
        }
        let zone = put(&mut image, &[106, 107]);
        word(&mut image, ZONE_LISTS + 4, zone);
        let rooms = put(&mut image, &[107]);
        word(&mut image, ROOM_LISTS + 8, rooms);
        let species = base + image.len() as u32;
        for kind in [15u32, 12, 6] {
            image.extend_from_slice(&kind.to_le_bytes());
        }
        word(&mut image, ZONE_SPECIES + 4, species);
        word(&mut image, ROWS + 24, group(16, &[(1, 3)]).0);

        let tables = Tables::parse(&image, base).expect("tables");
        assert_eq!(
            (tables.zone_of(106), tables.zone_of(107)),
            (Some(1), Some(1))
        );
        assert_eq!(tables.zone_of(108), None);
        assert_eq!(tables.species_of(1), Some([15, 12, 6]));
        assert_eq!(
            (tables.species_of(0), tables.species_of(ZONES)),
            (None, None)
        );
        let row = tables.row_of(107).expect("a row");
        assert_eq!((row[0].weight(), row[0].opponent(0)), (16, (1, 2 << 26)));
        assert!(tables.row_of(106).is_none() && tables.row_of(-1).is_none());
        assert_eq!(tables.rows().count(), ROOMS);
        // Tables that leave the image are an error.
        assert!(Tables::parse(&image[..end], base).is_err());
        assert!(Tables::from_field_engine(&[]).is_err());
    }

    #[test]
    fn a_new_game_fills_every_room_and_starts_with_the_plain_species() {
        let roaming = Roaming::new_game();
        assert_eq!(roaming, Roaming::default());
        assert!(roaming.plain);
        assert_eq!(roaming.list, [NOBODY; 4]);
        for (area, place, slots) in FORESTS {
            for slot in 0..slots {
                assert_eq!(roaming.word(area, place, slot as i16), Some(FILL));
            }
            assert_eq!(roaming.word(area, place, slots as i16), None);
            assert_eq!(roaming.word(area, place, -1), None);
        }
        // The village and the forest of area 0 have no table.
        assert_eq!(roaming.word(1, 1, 0), None);
        assert_eq!(roaming.word(4, 0, 0), None);
        assert_eq!(roaming.word(0, 3, 0), None);
    }

    #[test]
    fn the_reach_of_a_touch_is_four_hundred_times_the_scale() {
        assert_eq!(reach(4096), 400);
        assert_eq!(reach(2048), 200);
        // 2000 * 400 / 4096 is 195.3; and the shift rounds towards 0 below 0 too.
        assert_eq!(reach(2000), 195);
        assert_eq!(reach(-2000), -195);
        assert_eq!(reach(0), 0);
    }

    #[test]
    fn the_species_are_picked_on_coming_into_a_zone_and_flip_each_time() {
        let (tables, bases) = (tables(), bases());
        let mut roaming = Roaming::new_game();
        // From a room without a zone: the plain species, and the word flips.
        assert_eq!(
            roaming.enter(&tables, &bases, 10, 5, false),
            Some([15, 12, 6, NOBODY])
        );
        assert!(!roaming.plain);
        // Inside the zone nothing is picked.
        assert_eq!(roaming.enter(&tables, &bases, 11, 10, false), None);
        assert_eq!((roaming.list, roaming.plain), ([15, 12, 6, NOBODY], false));
        // Into the other zone: the variants, species * 4 + (element + 1) % 4 + 57.
        assert_eq!(
            roaming.enter(&tables, &bases, 20, 11, false),
            Some([130, 59, 81, NOBODY])
        );
        assert!(roaming.plain);
        // The same zone again, when the room that is left had no creatures (its flag
        // forces) or when there was no room before.
        assert_eq!(
            roaming.enter(&tables, &bases, 20, 20, true),
            Some([18, 0, 6, NOBODY])
        );
        assert_eq!(
            roaming.enter(&tables, &bases, 20, -1, false),
            Some([130, 59, 81, NOBODY])
        );
        // A room without a zone leaves list and word alone, forced or not.
        let before = roaming.clone();
        assert_eq!(roaming.enter(&tables, &bases, 5, 20, true), None);
        assert_eq!(roaming, before);

        // A species without an element has no variant.
        let mut other = tables.clone();
        other.species[0] = Some([5, 12, 99]);
        let mut roaming = Roaming::new_game();
        roaming.plain = false;
        assert_eq!(
            roaming.enter(&other, &bases, 10, -1, false),
            Some([5, 106, 99, NOBODY])
        );
        // A zone without species answers nothing and flips nothing.
        other.species[0] = None;
        assert_eq!(roaming.enter(&other, &bases, 10, -1, false), None);
        assert!(roaming.plain);
    }

    #[test]
    fn a_first_visit_fills_every_place() {
        let tables = tables();
        let mut roaming = Roaming::new_game();
        roaming.list = [15, 12, 6, NOBODY];
        // Room 11: the draws 7, 8 and 15 take the groups 0, 1 and 2.
        let stands = roaming.pick(&tables, BEETLES, 11, 3, &mut draws(&[7, 8 + 32, 15]));
        assert_eq!(
            stands
                .iter()
                .map(|s| (s.number, s.place, s.group))
                .collect::<Vec<_>>(),
            [(0, 0, 0), (1, 1, 1), (2, 2, 2)]
        );
        assert_eq!(stands[0].specs.map(|spec| spec.species), [15, 6, NOBODY]);
        assert_eq!(stands[0].specs[1].experience, 3 << 26);
        assert_eq!(stands[2].specs.map(|spec| spec.species), [12, 12, 15]);
        // The word: bit 15 gone, each place with its bit 4 and its group.
        assert_eq!(roaming.word(4, 1, 2), Some(0x10 | 0x11 << 5 | 0x12 << 10));
        // The fight block of a group ends at nobody.
        let block = stands[0].block();
        assert_eq!((block.mode, block.on_field), (2, 3));
        assert!(block.retreat && block.results);
        assert_eq!(block.specs.len(), 2);
        assert_eq!(stands[2].block().specs.len(), 3);

        // A room with two places gets two, one with more than three gets three.
        let mut roaming = Roaming::new_game();
        assert_eq!(
            roaming
                .pick(&tables, BEETLES, 10, 2, &mut draws(&[0, 15]))
                .len(),
            2
        );
        assert_eq!(roaming.word(4, 1, 2), Some(0x10 | 0x10 << 5));
        let mut roaming = Roaming::new_game();
        assert_eq!(
            roaming
                .pick(&tables, BEETLES, 10, 5, &mut draws(&[0; 3]))
                .len(),
            3
        );
        // Without a place the bit is cleared and nobody stands.
        let mut roaming = Roaming::new_game();
        assert!(
            roaming
                .pick(&tables, BEETLES, 10, 0, &mut draws(&[]))
                .is_empty()
        );
        assert_eq!(roaming.word(4, 1, 2), Some(0));
    }

    #[test]
    fn a_room_without_a_row_or_a_table_has_nobody_and_keeps_its_word() {
        let tables = tables();
        let mut roaming = Roaming::new_game();
        assert!(
            roaming
                .pick(&tables, BEETLES, 99, 3, &mut draws(&[]))
                .is_empty()
        );
        assert_eq!(roaming.word(4, 1, 2), Some(FILL));
        assert!(
            roaming
                .pick(&tables, (1, 1, 0), 10, 3, &mut draws(&[]))
                .is_empty()
        );
        assert!(
            roaming
                .pick(&tables, (4, 1, 12), 10, 3, &mut draws(&[]))
                .is_empty()
        );
        assert_eq!(roaming, Roaming::new_game());
    }

    #[test]
    fn a_forest_is_written_whole_and_the_others_are_left() {
        let mut roaming = Roaming::new_game();
        assert!(roaming.set_forest(4, 1, 0));
        assert_eq!(roaming.word(4, 1, 0), Some(0));
        assert_eq!(roaming.word(4, 1, 11), Some(0));
        assert_eq!(roaming.word(4, 2, 0), Some(FILL));
        assert!(!roaming.set_forest(1, 1, 0));
    }

    #[test]
    fn the_remembered_come_back_and_the_forgotten_stay_away() {
        let tables = tables();
        let mut roaming = Roaming::new_game();
        roaming.list = [15, 12, 6, NOBODY];
        roaming.pick(&tables, BEETLES, 11, 3, &mut draws(&[0, 8, 14]));
        // A second visit draws nothing and has the same three, with the species of the
        // list as it is then.
        roaming.list = [118, 106, 81, NOBODY];
        let again = roaming.pick(&tables, BEETLES, 11, 3, &mut draws(&[]));
        assert_eq!(
            again
                .iter()
                .map(|s| (s.number, s.place, s.group))
                .collect::<Vec<_>>(),
            [(0, 0, 0), (1, 1, 1), (2, 2, 2)]
        );
        assert_eq!(
            again[1].specs.map(|spec| spec.species),
            [106, NOBODY, NOBODY]
        );
        assert_eq!(again[1].specs[0].experience, 1 << 26);

        // The first one is beaten: its place is empty from then on, and the two that
        // are left are numbered anew.
        roaming.forget(BEETLES, again[0].number);
        let left = roaming.pick(&tables, BEETLES, 11, 3, &mut draws(&[]));
        assert_eq!(
            left.iter()
                .map(|s| (s.number, s.place, s.group))
                .collect::<Vec<_>>(),
            [(0, 1, 1), (1, 2, 2)]
        );
        // The forgetting goes by the number: the one on place 2 (number 1) takes the
        // bits of place 1 with it and stays itself; then number 0 is place 2, whose
        // forgetting clears the bits of place 0 and nothing else.
        roaming.forget(BEETLES, left[1].number);
        let last = roaming.pick(&tables, BEETLES, 11, 3, &mut draws(&[]));
        assert_eq!(
            last.iter().map(|s| (s.number, s.place)).collect::<Vec<_>>(),
            [(0, 2)]
        );
        roaming.forget(BEETLES, last[0].number);
        assert_eq!(roaming.word(4, 1, 2), Some(0x12 << 10));
        // A number past the places and a room without a table change nothing.
        let before = roaming.clone();
        roaming.forget(BEETLES, 3);
        roaming.forget((1, 1, 0), 0);
        assert_eq!(roaming, before);
    }

    #[test]
    fn an_emptied_room_gets_one_back_two_visits_of_three() {
        let tables = tables();
        let mut roaming = Roaming::new_game();
        roaming.list = [15, 12, 6, NOBODY];
        roaming.pick(&tables, BEETLES, 11, 0, &mut draws(&[]));
        assert_eq!(roaming.word(4, 1, 2), Some(0));
        // A draw that divides by 3: nobody, and nothing else is drawn.
        assert!(
            roaming
                .pick(&tables, BEETLES, 11, 3, &mut draws(&[9]))
                .is_empty()
        );
        assert_eq!(roaming.word(4, 1, 2), Some(0));
        // Else a second draw picks the place and a third the group.
        let one = roaming.pick(&tables, BEETLES, 11, 3, &mut draws(&[10, 5, 9]));
        assert_eq!(
            one.iter()
                .map(|s| (s.number, s.place, s.group))
                .collect::<Vec<_>>(),
            [(0, 2, 1)]
        );
        assert_eq!(roaming.word(4, 1, 2), Some(0x11 << 10));
        // With two places the second draw is taken modulo 2; with none nothing is drawn.
        let mut roaming = Roaming::new_game();
        roaming.pick(&tables, BEETLES, 10, 0, &mut draws(&[]));
        let one = roaming.pick(&tables, BEETLES, 10, 2, &mut draws(&[1, 5, 0]));
        assert_eq!((one[0].number, one[0].place), (0, 1));
        let mut roaming = Roaming::new_game();
        roaming.pick(&tables, BEETLES, 10, 0, &mut draws(&[]));
        assert!(
            roaming
                .pick(&tables, BEETLES, 10, 0, &mut draws(&[]))
                .is_empty()
        );
    }

    #[test]
    fn a_draw_no_weight_takes_makes_nobody() {
        let mut tables = tables();
        tables.rows[0] = [group(4, &[(0, 1)]), Group(0), Group(0)];
        let mut roaming = Roaming::new_game();
        let stands = roaming.pick(&tables, BEETLES, 10, 2, &mut draws(&[4, 3]));
        assert_eq!(
            stands
                .iter()
                .map(|s| (s.number, s.place))
                .collect::<Vec<_>>(),
            [(1, 1)]
        );
        assert_eq!(roaming.word(4, 1, 2), Some(0x10 << 5));
    }
}
