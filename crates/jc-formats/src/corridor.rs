
use crate::battle::record::{Draws, LEVEL};
use crate::battle::{self, Base, SPECIES, UNIQUES, Unique};
use crate::roaming::{self, FILL, GROUPS, Group, NOBODY, Stand};
use crate::rooms::{self, Image};
use crate::scene::PACKAGE_BASE;

/// The archive file of the package with the generator's code and tables.
pub const FILE: usize = 114;
/// The rooms of the corridor, each with a package of its own from [`FILE`] on, and
/// where they lie in the room table: a piece is the slot of its room there.
pub const ROOMS: usize = 75;
pub const AREA: i16 = 0;
pub const PLACE: i16 = 3;
/// The generator's object in the package's list: its type, the extra bytes of its task
/// and its start routine.
pub const OBJECT: u32 = 0x0D;
pub const EXTRA: u32 = 0x3C;
pub const START: u32 = 0x800B_34EC;
/// RAM addresses of the tables in the package: the record of each kind of piece, the
/// chooser of a new piece's kind, the species of the room of kind 4, the rows of groups
/// by the floor and the rows of the room of kind 4.
pub const KINDS: u32 = 0x800B_5574;
pub const CHOOSER: u32 = 0x800B_5678;
pub const LEADERS: u32 = 0x800B_56A4;
pub const ROWS: u32 = 0x800B_5710;
pub const LEADER_ROWS: u32 = 0x800B_57D0;
/// RAM addresses of what the package answers the set-up of a creature record for the
/// species 201 to 210: ten base records, ten rows of modifiers, the words of the
/// spells and the move entries (both by species - 57, the uniques behind the 144 of
/// the executable), and ten addresses of names.
pub const UNIQUE_BASES: u32 = 0x800B_5438;
pub const UNIQUE_MODIFIERS: u32 = 0x800B_5488;
pub const UNIQUE_SPELLS: u32 = 0x800B_4338;
pub const UNIQUE_MOVES: u32 = 0x800B_45A0;
pub const UNIQUE_NAMES: u32 = 0x800B_5410;
/// The pieces of a look and of a shape, and how many looks and shapes there are.
pub const PIECES_OF_A_LOOK: u32 = 15;
pub const PIECES_OF_A_SHAPE: u32 = 5;
pub const LOOKS: u32 = 5;
pub const SHAPES: u32 = 3;
/// The kinds of a piece.
pub mod kind {
    pub const START: u32 = 0;
    pub const STRETCH: u32 = 1;
    pub const FORK: u32 = 2;
    pub const END: u32 = 3;
    /// The room of the floor's leader, which leads a floor deeper.
    pub const DEEPER: u32 = 4;
}
/// The depths the chooser knows, and its columns.
const DEPTHS: usize = 4;
const COLUMNS: usize = 7;
/// The first element variant of a species (57 + base * 4 + element), and the first
/// variant of the two base creatures that come more often the deeper the floor.
const VARIANTS: u32 = 57;
const RARE: u32 = 185;
/// What the second draw of a species is held against, beside the floor's number.
const RARELY: u32 = 256;
/// The largest raise of the levels.
const MOST_RAISED: u32 = 7;

/// What can be wrong with the tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// A table leads outside the image. Holds the RAM address that was wanted.
    OutOfImage(u32),
    /// The chooser names a kind that is not a stretch, a fork or an end, or its last
    /// row names one with a way on: the game's builder would not come to an end.
    Chooser(u8),
}

impl From<rooms::RoomTableError> for Error {
    fn from(err: rooms::RoomTableError) -> Self {
        let rooms::RoomTableError::OutOfImage(address) = err;
        Self::OutOfImage(address)
    }
}

/// Where the player stands in a piece he comes into: a place and his angle about the
/// upright axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Arrival {
    pub place: [i32; 3],
    pub angle: i32,
}

/// The record of a kind of piece: its ways on (links 1 and 2) and its arrivals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Kind {
    pub ways: i32,
    pub arrivals: [Arrival; 3],
}

/// The generator's tables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tables {
    pub kinds: [Kind; 5],
    chooser: [[u8; COLUMNS]; DEPTHS],
    leaders: [[u32; 3]; 8],
    rows: [[Group; GROUPS]; 16],
    leader_rows: [[Group; GROUPS]; 3],
}

impl Tables {
    /// Reads the tables from the package file as it is on the disc.
    pub fn from_package(file: &[u8]) -> Result<Self, Error> {
        Self::parse(file, PACKAGE_BASE)
    }

    /// Reads the tables from an image whose first byte is at RAM address `base`.
    pub fn parse(data: &[u8], base: u32) -> Result<Self, Error> {
        let image = Image { data, base };
        let mut kinds = [Kind::default(); 5];
        for (number, kind) in kinds.iter_mut().enumerate() {
            let at = rooms::step(KINDS, number, 52)?;
            kind.ways = image.i32(at)?;
            for (n, arrival) in kind.arrivals.iter_mut().enumerate() {
                let at = rooms::step(at + 4, n, 16)?;
                for (axis, value) in arrival.place.iter_mut().enumerate() {
                    *value = image.i32(rooms::step(at, axis, 4)?)?;
                }
                arrival.angle = image.i32(at + 12)?;
            }
        }
        let mut chooser = [[0; COLUMNS]; DEPTHS];
        for (depth, row) in chooser.iter_mut().enumerate() {
            for (column, picked) in row.iter_mut().enumerate() {
                *picked = image.u8(rooms::step(CHOOSER, depth * COLUMNS + column, 1)?)?;
                let ways = kinds.get(usize::from(*picked)).map_or(0, |kind| kind.ways);
                let fits = (1..=3).contains(picked) && (depth + 1 < DEPTHS || ways <= 0);
                if !fits {
                    return Err(Error::Chooser(*picked));
                }
            }
        }
        let mut leaders = [[0; 3]; 8];
        for (n, species) in leaders.iter_mut().flatten().enumerate() {
            *species = image.u32(rooms::step(LEADERS, n, 4)?)?;
        }
        let groups = |at: u32, row: usize| -> Result<[Group; GROUPS], Error> {
            let mut words = [Group::default(); GROUPS];
            for (group, word) in words.iter_mut().enumerate() {
                *word = Group(image.u32(rooms::step(at, GROUPS * row + group, 4)?)?);
            }
            Ok(words)
        };
        let mut rows = [[Group::default(); GROUPS]; 16];
        for (row, words) in rows.iter_mut().enumerate() {
            *words = groups(ROWS, row)?;
        }
        let mut leader_rows = [[Group::default(); GROUPS]; 3];
        for (row, words) in leader_rows.iter_mut().enumerate() {
            *words = groups(LEADER_ROWS, row)?;
        }
        Ok(Self {
            kinds,
            chooser,
            leaders,
            rows,
            leader_rows,
        })
    }

    /// The ways on of a piece, by its kind.
    fn ways(&self, piece: u32) -> usize {
        usize::try_from(self.kinds[(piece % PIECES_OF_A_SHAPE) as usize].ways).unwrap_or(0)
    }
}

pub fn uniques(
    file: &[u8],
    name: impl Fn(&[u16]) -> [u8; crate::name::LETTERS],
) -> Result<Vec<Unique>, Error> {
    let image = Image {
        data: file,
        base: PACKAGE_BASE,
    };
    let bytes = |at: u32, len: usize| -> Result<&[u8], Error> {
        let start = at.checked_sub(PACKAGE_BASE).ok_or(Error::OutOfImage(at))? as usize;
        file.get(start..start + len).ok_or(Error::OutOfImage(at))
    };
    // The tables by species - 57 have the uniques behind the species 57 to 200.
    let behind = SPECIES - battle::BASES;
    (0..UNIQUES)
        .map(|n| {
            let base = bytes(rooms::step(UNIQUE_BASES, n, Base::SIZE)?, Base::SIZE)?;
            let modifiers = bytes(rooms::step(UNIQUE_MODIFIERS, n, 12)?, 12)?;
            let moves = bytes(rooms::step(UNIQUE_MOVES, behind + n, 20)?, 20)?;
            let text = image.u32(rooms::step(UNIQUE_NAMES, n, 4)?)?;
            let units = crate::text::units(
                file.get((text - PACKAGE_BASE) as usize..)
                    .ok_or(Error::OutOfImage(text))?,
            );
            Ok(Unique {
                base: Base::parse(base).ok_or(Error::OutOfImage(UNIQUE_BASES))?,
                modifiers: std::array::from_fn(|n| modifiers[n] as i8),
                spell_bits: image.u32(rooms::step(UNIQUE_SPELLS, behind + n, 4)?)?,
                moves: std::array::from_fn(|slot| battle::Move {
                    known: moves[4 * slot] != 0,
                    attack: moves[4 * slot + 1],
                    grade: moves[4 * slot + 2],
                    flag: moves[4 * slot + 3],
                }),
                name: name(&units),
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub piece: u32,
    pub links: [Option<(usize, usize)>; 3],
    pub word: u16,
}

/// What a move through a link leads to: the slot of the room to load, where the player
/// stands in it, and whether a new floor was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Move {
    pub piece: u32,
    pub arrival: Arrival,
    pub deeper: bool,
}

/// The generator's state: the tree of the floor (node 0 is where the floor starts), the
/// node the player is in, the floor's number, its look and its three species.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Corridor {
    pub nodes: Vec<Node>,
    pub at: usize,
    pub arrival: usize,
    pub floor: u32,
    pub look: u32,
    pub species: [u32; 3],
}

impl Corridor {
    /// The start routine (`0x800B34EC`): floor 1 is made. `model` answers the model of
    /// a species (`model::species_model`).
    pub fn enter(
        tables: &Tables,
        model: impl Fn(u32) -> Option<u16>,
        draws: &mut impl Draws,
    ) -> Self {
        Self::enter_at(1, tables, model, draws)
    }

    /// A corridor that starts on another floor than the first, which the game never
    /// does: for a look at a deep floor without the walk to it.
    pub fn enter_at(
        floor: u32,
        tables: &Tables,
        model: impl Fn(u32) -> Option<u16>,
        draws: &mut impl Draws,
    ) -> Self {
        let mut corridor = Self {
            nodes: Vec::new(),
            at: 0,
            arrival: 0,
            floor,
            look: 0,
            species: [0; 3],
        };
        corridor.make(tables, &model, draws);
        corridor
    }

    /// A floor (`0x800B2E3C`) and its three species (`0x800B317C`).
    fn make(
        &mut self,
        tables: &Tables,
        model: &impl Fn(u32) -> Option<u16>,
        draws: &mut impl Draws,
    ) {
        self.at = 0;
        self.look = draws.draw() % LOOKS;
        let piece = self.look * PIECES_OF_A_LOOK + draws.draw() % SHAPES * PIECES_OF_A_SHAPE;
        self.nodes = vec![Node {
            piece,
            links: [None; 3],
            word: FILL,
        }];
        let ends = self.build(tables, draws, 1, 0, 0, 0);
        // One of the ends, counted in the order they were built, becomes the room
        // that leads a floor deeper.
        let picked = draws.draw() % ends.max(1);
        self.lead_on(tables, draws, 0, i64::from(picked));
        self.pick_species(model, draws);
    }

    /// The builder (`0x800B2F68`): a new node behind `exit` of `parent`, and the nodes
    /// behind its ways on. The answer is the count of ends so far.
    fn build(
        &mut self,
        tables: &Tables,
        draws: &mut impl Draws,
        exit: usize,
        parent: usize,
        mut depth: usize,
        mut ends: u32,
    ) -> u32 {
        let node = self.nodes.len();
        self.nodes[parent].links[exit] = Some((node, 0));
        // The chooser (`0x800B38E4`): a stretch or a fork makes what lies behind it
        // one deeper.
        let picked = tables.chooser[depth][draws.draw() as usize % COLUMNS];
        let picked = u32::from(picked);
        if picked == kind::STRETCH || picked == kind::FORK {
            depth = (depth + 1).min(DEPTHS - 1);
        }
        if picked == kind::END {
            ends += 1;
        }
        let piece =
            self.look * PIECES_OF_A_LOOK + draws.draw() % SHAPES * PIECES_OF_A_SHAPE + picked;
        self.nodes.push(Node {
            piece,
            links: [Some((parent, exit)), None, None],
            word: FILL,
        });
        for way in 1..=tables.ways(piece) {
            ends = self.build(tables, draws, way, node, depth, ends);
        }
        ends
    }

    /// The linker (`0x800B3754`): walks the tree in the builder's order, counts `left`
    /// down at every end, and turns the end that finds it 0 into the room of kind 4
    /// with the way to the next floor.
    fn lead_on(&mut self, tables: &Tables, draws: &mut impl Draws, node: usize, left: i64) -> i64 {
        let piece = self.nodes[node].piece;
        if piece % PIECES_OF_A_SHAPE == kind::END {
            if left == 0 {
                let shape = draws.draw() % SHAPES;
                let node = &mut self.nodes[node];
                node.links[1] = Some((0, 0));
                node.piece =
                    self.look * PIECES_OF_A_LOOK + shape * PIECES_OF_A_SHAPE + kind::DEEPER;
            }
            return left - 1;
        }
        let mut left = left;
        for way in 1..=tables.ways(piece) {
            if let Some((behind, _)) = self.nodes[node].links[way] {
                left = self.lead_on(tables, draws, behind, left);
            }
        }
        left
    }

    /// The three species of a floor (`0x800B317C`, with the rule of `0x800B3A40`: a
    /// species whose model an earlier one of the three has gives way to that one).
    fn pick_species(&mut self, model: &impl Fn(u32) -> Option<u16>, draws: &mut impl Draws) {
        for n in 0..self.species.len() {
            let drawn = draws.draw();
            let mut species = match self.look {
                0 => VARIANTS + drawn % 128,
                look => VARIANTS - 1 + look + 4 * (drawn % 32),
            };
            if draws.draw() <= self.floor + RARELY {
                species = match self.look {
                    0 => RARE + draws.draw() % 8,
                    look => RARE + draws.draw() % 2 * (look - 1),
                };
            }
            let earlier = self.species[..n]
                .iter()
                .find(|&&earlier| model(earlier) == model(species));
            self.species[n] = earlier.copied().unwrap_or(species);
        }
    }

    /// Whether an archive file is the package of one of the corridor's rooms.
    pub fn has_room(file: usize) -> bool {
        (FILE..FILE + ROOMS).contains(&file)
    }

    /// The kind of the piece the player is in.
    pub fn kind(&self) -> u32 {
        self.nodes[self.at].piece % PIECES_OF_A_SHAPE
    }

    /// The links of the player's node: the trigger regions the step tests.
    pub fn links(&self) -> [bool; 3] {
        self.nodes[self.at].links.map(|link| link.is_some())
    }

    /// The move (`0x800B3590`) through link `link` of the player's node, and what the
    /// step asks of the room loader behind it. The way from the room of kind 4 back to
    /// the floor's start makes the next floor: the number goes up (from 1000 it falls
    /// back to 999), `deepest` (the word at +2064 of the game's block) follows it, and
    /// the old tree is given back. Nothing for a link that is empty.
    pub fn go(
        &mut self,
        link: usize,
        tables: &Tables,
        model: impl Fn(u32) -> Option<u16>,
        deepest: &mut i32,
        draws: &mut impl Draws,
    ) -> Option<Move> {
        let (to, arrival) = (*self.nodes[self.at].links.get(link)?)?;
        (self.at, self.arrival) = (to, arrival);
        let deeper = to == 0 && arrival == 0;
        if deeper {
            self.floor = match self.floor {
                1000.. => 999,
                floor => floor + 1,
            };
            let floor = self.floor as i32;
            if floor >= *deepest {
                *deepest = floor;
            }
            self.make(tables, &model, draws);
        }
        let piece = self.nodes[self.at].piece;
        let arrivals = &tables.kinds[(piece % PIECES_OF_A_SHAPE) as usize].arrivals;
        Some(Move {
            piece,
            arrival: arrivals.get(self.arrival).copied().unwrap_or_default(),
            deeper,
        })
    }

    /// The word of the player's piece for the roaming code (`0x800B33FC`).
    pub fn word(&mut self) -> &mut u16 {
        &mut self.nodes[self.at].word
    }

    /// The three species for the roaming code (`0x800B3368`): the floor's, or in the
    /// room of kind 4 the leader of the floor three times.
    pub fn species_here(&self, tables: &Tables) -> [u32; 3] {
        match self.kind() {
            kind::DEEPER => tables.leaders[(self.floor as usize).saturating_sub(1) % 8],
            _ => self.species,
        }
    }

    /// The row of groups for the roaming code (`0x800B2D10`): by the floor's number,
    /// and nobody in the room of kind 4 once its word is 0.
    pub fn row<'a>(&self, tables: &'a Tables) -> Option<&'a [Group; GROUPS]> {
        let floor = self.floor as usize;
        match self.kind() {
            kind::DEEPER if self.nodes[self.at].word == 0 => None,
            kind::DEEPER => Some(&tables.leader_rows[(floor.saturating_sub(1) / 8).min(2)]),
            _ => Some(&tables.rows[(floor / 2).min(15)]),
        }
    }

    /// What the floor adds to the level of every opponent (`0x800A4C78`).
    pub fn raise(&self) -> u32 {
        (self.floor.saturating_sub(1) / 4 + 1).min(MOST_RAISED)
    }

    pub fn pick(
        &mut self,
        tables: &Tables,
        list: &mut [u32; 4],
        places: usize,
        draws: &mut impl Draws,
    ) -> (Vec<Stand>, bool) {
        let species = self.species_here(tables);
        let anew = list[..3] != species;
        if anew {
            *list = [species[0], species[1], species[2], NOBODY];
        }
        let Some(row) = self.row(tables) else {
            return (Vec::new(), anew);
        };
        let raise = self.raise().wrapping_mul(LEVEL);
        let mut stands = roaming::pick_in(self.word(), row, list, places, draws);
        for spec in stands.iter_mut().flat_map(|stand| &mut stand.specs) {
            if spec.species < NOBODY {
                spec.experience = spec.experience.wrapping_add(raise);
            }
        }
        (stands, anew)
    }

    /// A creature of the player's piece is gone for the floor (`0x800A48D0` on the
    /// piece's word).
    pub fn forget(&mut self, number: usize) {
        roaming::forget_in(self.word(), number);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An image of the tables from `KINDS` on: the ways on of the disc's five kinds,
    /// arrival n of kind k at (100 * k + n, 0, 0) with the angle k, the chooser given,
    /// leader n as species 201 + n, and group words that name their row.
    fn tables(chooser: [[u8; COLUMNS]; DEPTHS]) -> Result<Tables, Error> {
        let mut data = vec![0u8; (LEADER_ROWS - KINDS) as usize + 36];
        let mut put = |at: u32, value: u32| {
            let at = (at - KINDS) as usize;
            data[at..at + 4].copy_from_slice(&value.to_le_bytes());
        };
        for (k, ways) in [1i32, 1, 2, 0, -1].into_iter().enumerate() {
            let at = KINDS + 52 * k as u32;
            put(at, ways as u32);
            for n in 0..3 {
                put(at + 4 + 16 * n, 100 * k as u32 + n);
                put(at + 16 + 16 * n, k as u32);
            }
        }
        for n in 0..24 {
            put(LEADERS + 4 * n, 201 + n / 3);
        }
        for n in 0..48 {
            put(ROWS + 4 * n, 1000 + n / 3);
        }
        for n in 0..9 {
            put(LEADER_ROWS + 4 * n, 2000 + n / 3);
        }
        let at = (CHOOSER - KINDS) as usize;
        data[at..at + 28].copy_from_slice(chooser.as_flattened());
        Tables::parse(&data, KINDS)
    }

    /// Column 0 an end, column 1 a stretch, column 2 a fork, down to the last depth.
    const BY_COLUMN: [[u8; COLUMNS]; DEPTHS] = [
        [3, 1, 2, 3, 3, 3, 3],
        [3, 1, 2, 3, 3, 3, 3],
        [3, 1, 2, 3, 3, 3, 3],
        [3; COLUMNS],
    ];

    fn draws(list: &[u32]) -> impl Draws + '_ {
        let mut list = list.iter();
        move || *list.next().expect("a draw too many")
    }

    /// Every species its own model.
    fn own(species: u32) -> Option<u16> {
        Some(species as u16)
    }

    /// The draws of three species of a look that is not 0, none of them the rare one.
    const PLAIN: [u32; 6] = [0, 9999, 1, 9999, 2, 9999];

    #[test]
    fn the_corridor_has_the_packages_of_its_75_rooms() {
        assert!(!Corridor::has_room(FILE - 1));
        assert!(Corridor::has_room(FILE));
        assert!(Corridor::has_room(FILE + ROOMS - 1));
        assert!(!Corridor::has_room(FILE + ROOMS));
    }

    #[test]
    fn the_tables_are_read_and_a_chooser_that_cannot_end_is_refused() {
        let read = tables(BY_COLUMN).expect("tables");
        assert_eq!(read.kinds[2].ways, 2);
        assert_eq!(read.kinds[4].ways, -1);
        assert_eq!(read.kinds[2].arrivals[1].place, [201, 0, 0]);
        assert_eq!(read.kinds[2].arrivals[1].angle, 2);
        assert_eq!(read.leaders[7], [208; 3]);
        assert_eq!(read.rows[15][2], Group(1015));
        assert_eq!(read.leader_rows[2][0], Group(2002));
        let mut never = BY_COLUMN;
        never[3][4] = 1;
        assert_eq!(tables(never), Err(Error::Chooser(1)));
        let mut unknown = BY_COLUMN;
        unknown[0][0] = 4;
        assert_eq!(tables(unknown), Err(Error::Chooser(4)));
        assert_eq!(
            Tables::parse(&[0; 64], KINDS),
            Err(Error::OutOfImage(KINDS + 64))
        );
    }

    #[test]
    fn a_floor_of_one_end_leads_on_behind_it() {
        let tables = tables(BY_COLUMN).expect("tables");
        // Look 2, the start's shape 1; an end of shape 2; the end picked; its new
        // shape 1; then the species.
        let mut list = vec![7, 4, 0, 2, 5, 1];
        list.extend(PLAIN);
        let corridor = Corridor::enter(&tables, own, &mut draws(&list));
        assert_eq!((corridor.floor, corridor.look, corridor.at), (1, 2, 0));
        assert_eq!(corridor.nodes.len(), 2);
        assert_eq!(corridor.nodes[0].piece, 35);
        assert_eq!(corridor.nodes[0].links, [None, Some((1, 0)), None]);
        assert_eq!(corridor.nodes[1].piece, 39);
        assert_eq!(corridor.nodes[1].links, [Some((0, 1)), Some((0, 0)), None]);
        assert_eq!(corridor.nodes[1].word, FILL);
        // Look 2 is the corridor of element 1: 57 + 4 * base + 1.
        assert_eq!(corridor.species, [58, 62, 66]);
    }

    #[test]
    fn the_largest_floor_has_fifteen_pieces_and_the_ends_count_in_the_order_built() {
        let tables = tables(BY_COLUMN).expect("tables");
        // Look 0 and shape 0 throughout; forks down to depth 3, where every piece is
        // an end whatever is drawn: seven forks and eight ends.
        // The builder goes down the first way first: fork, fork, fork, two ends, ...
        let order = [2, 2, 2, 6, 6, 2, 6, 6, 2, 2, 6, 6, 2, 6, 6];
        let mut list = vec![0, 0];
        for column in order {
            list.extend([column, 0]);
        }
        // The sixth end, and its new shape 2.
        list.extend([5 + 8 * 3, 2]);
        list.extend([0, 9999, 1, 9999, 2, 9999]);
        let corridor = Corridor::enter(&tables, own, &mut draws(&list));
        assert_eq!(corridor.nodes.len(), 16);
        let kinds: Vec<u32> = corridor.nodes.iter().map(|node| node.piece % 5).collect();
        assert_eq!(kinds, [0, 2, 2, 2, 3, 3, 2, 3, 3, 2, 2, 3, 4, 2, 3, 3]);
        assert_eq!(corridor.nodes[12].piece, 14);
        assert_eq!(
            corridor.nodes[12].links,
            [Some((10, 2)), Some((0, 0)), None]
        );
        // A fork's ways: link 0 back, links 1 and 2 on, each arriving by 0.
        assert_eq!(
            corridor.nodes[1].links,
            [Some((0, 1)), Some((2, 0)), Some((9, 0))]
        );
        // Look 0 has every element: 57 + a draw of 128.
        assert_eq!(corridor.species, [57, 58, 59]);
    }

    #[test]
    fn the_creatures_of_a_piece_go_by_its_word_and_a_beaten_one_stays_away() {
        let mut tables = tables(BY_COLUMN).expect("tables");
        // One group in every row: the species of index 0 alone at level 19.
        let alone = Group(16 | (18 << 3) << 8 | 3 << 16 | 3 << 24);
        tables.rows = [[alone, Group(0), Group(0)]; 16];
        tables.leader_rows = [[alone, Group(0), Group(0)]; 3];
        // A stretch and the room that leads deeper behind it, look 1, on floor 5.
        let mut list = vec![1, 0, 1, 0, 0, 0, 0, 0];
        list.extend(PLAIN);
        let mut corridor = Corridor::enter_at(5, &tables, own, &mut draws(&list));
        let species = corridor.species;
        let mut deepest = -1;
        let none = &mut draws(&[]);
        assert!(corridor.go(1, &tables, own, &mut deepest, none).is_some());
        // A first visit fills both places, a draw each for the group. The floor's
        // species go to the list, and floor 5 raises level 19 by 2.
        let mut listed = [NOBODY; 4];
        let (stands, anew) = corridor.pick(&tables, &mut listed, 2, &mut draws(&[0, 0]));
        assert!(anew);
        assert_eq!(listed, [species[0], species[1], species[2], NOBODY]);
        assert_eq!(stands.len(), 2);
        assert_eq!(stands[0].specs[0].species, species[0]);
        assert_eq!(stands[0].specs[0].experience, 20 * LEVEL);
        assert_eq!(stands[0].specs[1].species, NOBODY);
        assert_eq!(stands[0].specs[1].experience, 0);
        // The first is beaten: the other comes back as it was, without a draw.
        corridor.forget(0);
        let (stands, anew) = corridor.pick(&tables, &mut listed, 2, none);
        assert!(!anew);
        assert_eq!(stands.len(), 1);
        assert_eq!((stands[0].number, stands[0].place), (0, 1));
        // The room that leads deeper has the leader of the floor, the fifth.
        assert!(corridor.go(1, &tables, own, &mut deepest, none).is_some());
        assert_eq!(corridor.kind(), kind::DEEPER);
        let (stands, anew) = corridor.pick(&tables, &mut listed, 1, &mut draws(&[0]));
        assert!(anew);
        assert_eq!(listed, [205, 205, 205, NOBODY]);
        assert_eq!(stands.len(), 1);
        assert_eq!(stands[0].specs[0].species, 205);
        // A beaten leader does not come back: the room has no row any more.
        corridor.forget(0);
        assert!(corridor.row(&tables).is_none());
        let (stands, _) = corridor.pick(&tables, &mut listed, 1, none);
        assert!(stands.is_empty());
    }

    #[test]
    fn a_stretch_makes_what_lies_behind_it_deeper() {
        let tables = tables(BY_COLUMN).expect("tables");
        // Three stretches in a row; the fourth piece is an end though a stretch's
        // column is drawn, because the depth stops at 3.
        let mut list = vec![1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 0, 0];
        list.extend(PLAIN);
        let corridor = Corridor::enter(&tables, own, &mut draws(&list));
        let kinds: Vec<u32> = corridor.nodes.iter().map(|node| node.piece % 5).collect();
        assert_eq!(kinds, [0, 1, 1, 1, 4]);
    }

    #[test]
    fn the_rare_species_come_by_the_floor_and_one_model_comes_once() {
        let tables = tables(BY_COLUMN).expect("tables");
        // Look 3. The first species is base 2 of element 2; the second's other draw is
        // 257, not above floor 1 + 256, so it is the rare one: 185 + 1 * (3 - 1); the
        // third has the model of the first and gives way to it.
        let list = [3, 0, 0, 0, 0, 0, 2, 9999, 5, 257, 1, 9, 258];
        let model = |species: u32| Some(if species == 187 { 1 } else { 0 });
        let corridor = Corridor::enter(&tables, model, &mut draws(&list));
        assert_eq!(corridor.species, [67, 187, 67]);
        // Look 0: the rare one is 185 + a draw of 8.
        let list = [0, 0, 0, 0, 0, 0, 127, 0, 15, 5, 9999, 6, 9999];
        let corridor = Corridor::enter(&tables, own, &mut draws(&list));
        assert_eq!(corridor.species, [192, 62, 63]);
    }

    #[test]
    fn a_move_goes_by_a_link_and_the_way_on_makes_the_next_floor() {
        let tables = tables(BY_COLUMN).expect("tables");
        // A stretch and an end behind it, look 1.
        let mut list = vec![1, 0, 1, 0, 0, 0, 0, 0];
        list.extend(PLAIN);
        let mut corridor = Corridor::enter(&tables, own, &mut draws(&list));
        assert_eq!(corridor.links(), [false, true, false]);
        let mut deepest = -1;
        let none = &mut draws(&[]);
        assert_eq!(corridor.go(0, &tables, own, &mut deepest, none), None);
        // Into the stretch by its arrival 0, on into the room of kind 4, and back
        // into the stretch by its arrival 1.
        let stretch = corridor
            .go(1, &tables, own, &mut deepest, none)
            .expect("a way");
        assert_eq!(
            (stretch.piece, stretch.arrival.place[0], stretch.deeper),
            (16, 100, false)
        );
        assert_eq!(corridor.links(), [true, true, false]);
        let leader = corridor
            .go(1, &tables, own, &mut deepest, none)
            .expect("a way");
        assert_eq!((leader.piece, leader.arrival.place[0]), (19, 400));
        assert_eq!(corridor.kind(), kind::DEEPER);
        *corridor.word() = 0;
        let back = corridor
            .go(0, &tables, own, &mut deepest, none)
            .expect("a way");
        assert_eq!((back.piece, back.arrival.place[0]), (16, 101));
        corridor.go(1, &tables, own, &mut deepest, none);
        // The remembered word of a piece stays for the floor.
        assert_eq!(*corridor.word(), 0);
        assert_eq!((corridor.floor, deepest), (1, -1));
        // The way on: floor 2 with a new tree, the player at its start.
        let mut list = vec![4, 2, 0, 0, 0, 0];
        list.extend(PLAIN);
        let next = corridor
            .go(1, &tables, own, &mut deepest, &mut draws(&list))
            .expect("on");
        assert_eq!(
            (next.piece, next.arrival.place[0], next.deeper),
            (70, 0, true)
        );
        assert_eq!(
            (corridor.floor, corridor.look, corridor.at, deepest),
            (2, 4, 0, 2)
        );
        assert_eq!(corridor.nodes.len(), 2);
        assert_eq!(corridor.nodes[1].word, FILL);
    }

    #[test]
    fn the_count_of_floors_goes_to_a_thousand_and_falls_back() {
        let tables = tables(BY_COLUMN).expect("tables");
        let mut floor = vec![0, 0, 0, 0, 0, 0];
        floor.extend([0, 9999, 1, 9999, 2, 9999]);
        let mut corridor = Corridor::enter(&tables, own, &mut draws(&floor));
        let mut deepest = 1000;
        for (from, to) in [(998, 999), (999, 1000), (1000, 999)] {
            corridor.floor = from;
            corridor.go(1, &tables, own, &mut deepest, &mut draws(&[]));
            corridor.go(1, &tables, own, &mut deepest, &mut draws(&floor));
            assert_eq!(corridor.floor, to);
        }
        // A number below the deepest one leaves it.
        assert_eq!(deepest, 1000);
    }

    #[test]
    fn the_roaming_code_is_answered_by_the_floor_and_the_piece() {
        let tables = tables(BY_COLUMN).expect("tables");
        let mut list = vec![1, 0, 0, 0, 0, 0];
        list.extend(PLAIN);
        let mut corridor = Corridor::enter(&tables, own, &mut draws(&list));
        let mut deepest = 0;
        for (floor, row, raise) in [
            (1, 1000, 1),
            (2, 1001, 1),
            (5, 1002, 2),
            (29, 1014, 7),
            (999, 1015, 7),
        ] {
            corridor.floor = floor;
            assert_eq!(
                corridor.row(&tables),
                Some(&[Group(row); 3]),
                "floor {floor}"
            );
            assert_eq!(corridor.raise(), raise, "floor {floor}");
            assert_eq!(corridor.species_here(&tables), corridor.species);
        }
        corridor.go(1, &tables, own, &mut deepest, &mut draws(&[]));
        for (floor, row, leader) in [
            (1, 2000, 201),
            (8, 2000, 208),
            (9, 2001, 201),
            (17, 2002, 201),
            (900, 2002, 204),
        ] {
            corridor.floor = floor;
            assert_eq!(
                corridor.row(&tables),
                Some(&[Group(row); 3]),
                "floor {floor}"
            );
            assert_eq!(corridor.species_here(&tables), [leader; 3], "floor {floor}");
        }
        // A leader that is beaten does not come back on its floor.
        *corridor.word() = 0;
        assert_eq!(corridor.row(&tables), None);
    }
}
