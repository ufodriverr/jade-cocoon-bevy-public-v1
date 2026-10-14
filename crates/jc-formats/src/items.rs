
/// Pairs in a list.
pub const PAIRS: usize = 64;
/// The number of a pair that is free.
pub const FREE: u8 = 255;
/// Nobody has more than this of a thing.
pub const MOST_OF_A_THING: i32 = 20;
/// The most money the player has.
pub const MOST_MONEY: i32 = 9_999_999;
/// The most empty cocoons.
pub const MOST_COCOONS: i32 = 12;
/// Bits for the things of the story.
pub const STORY_THINGS: i32 = 64;
/// The things of the story that have a name: the ones that are counted.
pub const NAMED_STORY_THINGS: i32 = 38;
/// Flags that a new game clears.
pub const FLAGS: u32 = 152;
/// The small values behind the flags.
pub const BYTES: usize = 20;
/// Bytes of the flags.
const FLAG_BYTES: usize = FLAGS as usize / 8;
/// The bytes of [`Keeping`] in the state block, and where its pieces lie in them.
pub const SAVED: usize = MARKS_AT + FLAG_BYTES + BYTES;
const LISTS_AT: usize = 4;
const COCOONS_AT: usize = LISTS_AT + 5 * PAIRS * 2;
const STORY_AT: usize = COCOONS_AT + 4;
const MARKS_AT: usize = STORY_AT + 8;

/// The kinds of thing, as a notice and the give and take routines number them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    Item = 0,
    Weapon = 1,
    Armour = 2,
    Ornament = 3,
    /// What the pairs of this list stand for is not known.
    Creature = 4,
    /// One bit each, no list.
    StoryThing = 5,
    /// A count, no list.
    Cocoons = 6,
    /// A count, no list.
    Money = 7,
}

impl Kind {
    /// The kind of a number, `None` for a number that is no kind.
    pub fn of(kind: i32) -> Option<Self> {
        Some(match kind {
            0 => Self::Item,
            1 => Self::Weapon,
            2 => Self::Armour,
            3 => Self::Ornament,
            4 => Self::Creature,
            5 => Self::StoryThing,
            6 => Self::Cocoons,
            7 => Self::Money,
            _ => return None,
        })
    }

    /// The list of the kind among the five (`0x800AD7D4`), `None` for a kind without one.
    fn list(self) -> Option<usize> {
        let list = self as usize;
        (list < 5).then_some(list)
    }
}

/// One pair of a list: a thing and how many of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pair {
    /// The thing's number, [`FREE`] for a pair that holds nothing.
    pub number: u8,
    pub count: u8,
}

impl Pair {
    const NONE: Self = Self {
        number: FREE,
        count: 0,
    };
}

pub type Worn = [u8; 3];

/// What the game keeps of the player's things.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keeping {
    money: i32,
    lists: [[Pair; PAIRS]; 5],
    cocoons: i32,
    story: [u32; 2],
    /// The flags and, right behind them in memory, the small values: a flag number past
    /// the last flag reaches into the values, as it does in the game.
    marks: [u8; FLAG_BYTES + BYTES],
}

impl Default for Keeping {
    fn default() -> Self {
        Self::new()
    }
}

/// The first pair of a list with a number (`0x800AD75C`). The game compares the whole
/// number with a byte, so a number outside a byte finds nothing.
fn find(list: &[Pair; PAIRS], number: i32) -> Option<usize> {
    list.iter()
        .position(|pair| i32::from(pair.number) == number)
}

impl Keeping {
    /// The state a new game starts with (`0x800A272C` of the title overlay): nothing.
    pub fn new() -> Self {
        Self {
            money: 0,
            lists: [[Pair::NONE; PAIRS]; 5],
            cocoons: 0,
            story: [0; 2],
            marks: [0; FLAG_BYTES + BYTES],
        }
    }

    pub fn to_bytes(&self) -> [u8; SAVED] {
        let mut bytes = [0; SAVED];
        bytes[..4].copy_from_slice(&self.money.to_le_bytes());
        let pairs = self.lists.iter().flatten();
        for (pair, to) in pairs.zip(bytes[LISTS_AT..COCOONS_AT].chunks_exact_mut(2)) {
            to.copy_from_slice(&[pair.number, pair.count]);
        }
        bytes[COCOONS_AT..COCOONS_AT + 4].copy_from_slice(&self.cocoons.to_le_bytes());
        bytes[STORY_AT..STORY_AT + 4].copy_from_slice(&self.story[0].to_le_bytes());
        bytes[STORY_AT + 4..MARKS_AT].copy_from_slice(&self.story[1].to_le_bytes());
        bytes[MARKS_AT..].copy_from_slice(&self.marks);
        bytes
    }

    /// The way back of [`Self::to_bytes`].
    pub fn from_bytes(bytes: &[u8; SAVED]) -> Self {
        let word = |at: usize| [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]];
        let mut pairs = bytes[LISTS_AT..COCOONS_AT].chunks_exact(2);
        let mut keeping = Self::new();
        keeping.money = i32::from_le_bytes(word(0));
        for pair in keeping.lists.iter_mut().flatten() {
            if let Some(&[number, count]) = pairs.next() {
                *pair = Pair { number, count };
            }
        }
        keeping.cocoons = i32::from_le_bytes(word(COCOONS_AT));
        keeping.story = [STORY_AT, STORY_AT + 4].map(|at| u32::from_le_bytes(word(at)));
        keeping.marks.copy_from_slice(&bytes[MARKS_AT..]);
        keeping
    }

    /// The pairs of a kind's list, `None` for a kind without one.
    pub fn pairs(&self, kind: Kind) -> Option<&[Pair; PAIRS]> {
        kind.list().map(|list| &self.lists[list])
    }

    /// The pair a gift goes to (`0x800AD6A8`): the thing's own pair while the count stays
    /// in its limits, a free pair for a thing the player does not have.
    fn place_for(&self, number: i32, count: i32, kind: Kind, worn: Worn) -> Option<usize> {
        let list = self.pairs(kind)?;
        let Some(at) = find(list, number) else {
            return find(list, i32::from(FREE));
        };
        let sum = i32::from(list[at].count).wrapping_add(count);
        if sum > MOST_OF_A_THING {
            return None;
        }
        // The thing the player has on leaves room for one less.
        let on = (kind as usize).checked_sub(1).and_then(|on| worn.get(on));
        match on {
            Some(&on) if i32::from(on) == number && sum >= MOST_OF_A_THING => None,
            _ => Some(at),
        }
    }

    /// Whether a gift of `count` of a thing finds a pair (`0x800AD6A8`): what the shop
    /// asks before it sells.
    pub fn fits(&self, number: i32, count: i32, kind: Kind, worn: Worn) -> bool {
        self.place_for(number, count, kind, worn).is_some()
    }

    /// Gives `count` of a thing (`0x800AD354`). A gift that does not fit changes nothing
    /// and answers `false`; so does the number of a free pair and a kind without a list.
    pub fn give(&mut self, number: i32, count: i32, kind: Kind, worn: Worn) -> bool {
        let Some(at) = self.place_for(number, count, kind, worn) else {
            return false;
        };
        if number == i32::from(FREE) {
            return false;
        }
        let list = kind.list().expect("a pair was found in the kind's list");
        let pair = &mut self.lists[list][at];
        pair.number = number as u8;
        pair.count = pair.count.wrapping_add(count as u8);
        true
    }

    /// Takes `count` of a thing (`0x800AD3B0`) when the player has that many, and frees the
    /// pair when nothing is left. Answers whether it took them.
    pub fn take(&mut self, number: i32, count: i32, kind: Kind) -> bool {
        let Some(list) = kind.list() else {
            return false;
        };
        let Some(at) = find(&self.lists[list], number) else {
            return false;
        };
        let pair = &mut self.lists[list][at];
        if i32::from(pair.count).wrapping_sub(count) < 0 {
            return false;
        }
        pair.count = pair.count.wrapping_sub(count as u8);
        if pair.count == 0 {
            pair.number = FREE;
        }
        true
    }

    /// How many of a thing the player has (`0x800AD424`).
    pub fn count(&self, number: i32, kind: Kind) -> u8 {
        self.pairs(kind)
            .and_then(|list| Some(list[find(list, number)?].count))
            .unwrap_or(0)
    }

    /// The number of the pair at a place of a kind's list (`0x800AD458`), [`FREE`] for a
    /// kind without a list. A place past the list is taken as free: the game reads on
    /// into what lies behind the list.
    pub fn number_at(&self, place: usize, kind: Kind) -> u8 {
        let pair = self.pairs(kind).and_then(|list| list.get(place));
        pair.map_or(FREE, |pair| pair.number)
    }

    /// The count of the pair at a place of a kind's list (`0x800AD498`).
    pub fn count_at(&self, place: usize, kind: Kind) -> u8 {
        let pair = self.pairs(kind).and_then(|list| list.get(place));
        pair.map_or(0, |pair| pair.count)
    }

    /// The last place in use of a kind's list (`0x800AD4D8`); 0 also when it is the first
    /// place or none.
    pub fn last_place(&self, kind: Kind) -> usize {
        self.pairs(kind)
            .and_then(|list| (1..PAIRS).rev().find(|&place| list[place].number != FREE))
            .unwrap_or(0)
    }

    /// Whether the list of kind 4 holds anything (`0x800AD568`).
    pub fn has_creatures(&self) -> bool {
        self.last_place(Kind::Creature) > 0
            || (self.number_at(0, Kind::Creature) != FREE && self.count_at(0, Kind::Creature) != 0)
    }

    /// Puts a kind's list in order (`0x800AD5C0`): for each number from 0 up to the
    /// list's length the thing's pair changes places with the pair at the next place from
    /// the front. Things with a higher number stay where that leaves them.
    pub fn tidy(&mut self, kind: Kind) {
        let Some(list) = kind.list() else {
            return;
        };
        let list = &mut self.lists[list];
        let mut front = 0;
        for number in 0..PAIRS as i32 {
            if let Some(at) = find(list, number) {
                list.swap(front, at);
                front += 1;
            }
        }
    }

    /// The money (`0x800AD1FC`), never more than [`MOST_MONEY`].
    pub fn money(&self) -> i32 {
        self.money.min(MOST_MONEY)
    }

    /// Adds money up to the limit (`0x800AD228`). Answers `false`, and adds nothing, only
    /// when the money stood at the limit already.
    pub fn give_money(&mut self, amount: i32) -> bool {
        if self.money >= MOST_MONEY {
            return false;
        }
        self.money = self.money.wrapping_add(amount).min(MOST_MONEY);
        true
    }

    /// Takes money when the player has that much (`0x800AD274`).
    pub fn take_money(&mut self, amount: i32) -> bool {
        if self.money < amount {
            return false;
        }
        self.money = self.money.wrapping_sub(amount);
        true
    }

    /// Word and bit of a thing of the story. The game's own arithmetic rounds towards zero,
    /// so the numbers -1 to -31 land in the first word; a number below them reaches memory
    /// in front of the two words, which is not kept here.
    fn story_bit(number: i32) -> Option<(usize, u32)> {
        if !(-31..STORY_THINGS).contains(&number) {
            return None;
        }
        let word = number / 32;
        Some((word as usize, 1 << ((number - word * 32) & 31)))
    }

    /// Gives a thing of the story (`0x800AD85C`). Numbers from [`STORY_THINGS`] up do
    /// nothing and answer `false`.
    pub fn give_story_thing(&mut self, number: i32) -> bool {
        if let Some((word, bit)) = Self::story_bit(number) {
            self.story[word] |= bit;
        }
        number < STORY_THINGS
    }

    /// Takes a thing of the story (`0x800AD8B4`).
    pub fn take_story_thing(&mut self, number: i32) {
        if let Some((word, bit)) = Self::story_bit(number) {
            self.story[word] &= !bit;
        }
    }

    /// Whether the player has a thing of the story (`0x800AD90C`).
    pub fn has_story_thing(&self, number: i32) -> bool {
        Self::story_bit(number).is_some_and(|(word, bit)| self.story[word] & bit != 0)
    }

    /// How many of the named things of the story the player has (`0x800AD96C`).
    pub fn story_things(&self) -> usize {
        (0..NAMED_STORY_THINGS)
            .filter(|&number| self.has_story_thing(number))
            .count()
    }

    /// The number of the n-th thing of the story the player has, counted from 0
    /// (`0x800AD9C8`); [`NAMED_STORY_THINGS`] for none.
    pub fn story_thing_at(&self, n: usize) -> i32 {
        (0..NAMED_STORY_THINGS)
            .filter(|&number| self.has_story_thing(number))
            .nth(n)
            .unwrap_or(NAMED_STORY_THINGS)
    }

    /// The empty cocoons (`0x800ADAB4`).
    pub fn cocoons(&self) -> i32 {
        self.cocoons
    }

    /// Adds empty cocoons (`0x800ADAC4`). When the sum goes over [`MOST_COCOONS`] the count
    /// becomes that and the answer is `false`.
    pub fn give_cocoons(&mut self, count: i32) -> bool {
        let sum = self.cocoons.wrapping_add(count);
        self.cocoons = sum.min(MOST_COCOONS);
        sum <= MOST_COCOONS
    }

    /// Takes empty cocoons (`0x800ADB14`). With too few the count becomes 0 and the answer
    /// is `false`.
    pub fn take_cocoons(&mut self, count: i32) -> bool {
        let left = self.cocoons.wrapping_sub(count);
        self.cocoons = left.max(0);
        left >= 0
    }

    /// The empty cocoons plus the creature records in use (`0x800ADA8C`).
    pub fn cocoons_carried(&self, creatures: i32) -> i32 {
        self.cocoons.wrapping_add(creatures)
    }

    /// What is left of [`MOST_COCOONS`] after the cocoons and the creature records in use
    /// (`0x800ADA58`), never below 0.
    pub fn cocoons_left(&self, creatures: i32) -> i32 {
        MOST_COCOONS
            .wrapping_sub(self.cocoons_carried(creatures))
            .max(0)
    }

    /// Byte and bit of a flag. A number past the small values reaches memory behind them,
    /// which is not kept here.
    fn flag_bit(&self, flag: u32) -> Option<(usize, u8)> {
        let byte = (flag >> 3) as usize;
        (byte < self.marks.len()).then_some((byte, 1 << (flag & 7)))
    }

    /// Whether a flag is set (`0x800AD2A4`).
    pub fn flag(&self, flag: u32) -> bool {
        self.flag_bit(flag)
            .is_some_and(|(byte, bit)| self.marks[byte] & bit != 0)
    }

    /// Sets a flag (`0x800AD2C8`).
    pub fn set_flag(&mut self, flag: u32) {
        if let Some((byte, bit)) = self.flag_bit(flag) {
            self.marks[byte] |= bit;
        }
    }

    /// Clears a flag (`0x800AD2F4`).
    pub fn clear_flag(&mut self, flag: u32) {
        if let Some((byte, bit)) = self.flag_bit(flag) {
            self.marks[byte] &= !bit;
        }
    }

    /// One of the small values behind the flags (`0x800AD324`); 0 past the last one, where
    /// the game reads on.
    pub fn byte(&self, n: usize) -> u8 {
        self.marks.get(FLAG_BYTES + n).copied().unwrap_or(0)
    }

    /// Writes one of the small values (`0x800AD33C`).
    pub fn set_byte(&mut self, n: usize, value: u8) {
        if let Some(byte) = self.marks.get_mut(FLAG_BYTES + n) {
            *byte = value;
        }
    }

    pub fn notice_ends(&mut self, way: i32, kind: i32, number: i32, count: i32, worn: Worn) {
        let Some(kind) = Kind::of(kind) else {
            return;
        };
        match way {
            -1..=3 => {
                match kind {
                    Kind::Money => self.give_money(count),
                    Kind::Cocoons => self.give_cocoons(count),
                    Kind::StoryThing => self.give_story_thing(number),
                    _ => self.give(number, count, kind, worn),
                };
            }
            4..=7 if matches!(kind, Kind::Item | Kind::StoryThing) => {
                self.take(number, 1, kind);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BARE: Worn = [FREE; 3];

    #[test]
    fn a_new_game_has_nothing() {
        let keeping = Keeping::new();
        assert_eq!(keeping.money(), 0);
        assert_eq!(keeping.cocoons(), 0);
        assert_eq!(keeping.story_things(), 0);
        assert_eq!(keeping.story_thing_at(0), NAMED_STORY_THINGS);
        for kind in (0..5).filter_map(Kind::of) {
            let list = keeping.pairs(kind).unwrap();
            assert!(list.iter().all(|pair| *pair == Pair::NONE));
            assert_eq!(keeping.last_place(kind), 0);
        }
        assert!(keeping.pairs(Kind::StoryThing).is_none());
        assert!((0..FLAGS).all(|flag| !keeping.flag(flag)));
        assert!((0..BYTES).all(|n| keeping.byte(n) == 0));
        assert!(!keeping.has_creatures());
    }

    #[test]
    fn nobody_has_more_than_twenty_of_a_thing() {
        let mut keeping = Keeping::new();
        assert!(keeping.give(7, 15, Kind::Item, BARE));
        assert!(keeping.give(7, 5, Kind::Item, BARE));
        assert_eq!(keeping.count(7, Kind::Item), 20);
        // The gift that does not fit is lost: no second pair is opened for it.
        assert!(!keeping.give(7, 1, Kind::Item, BARE));
        assert_eq!(keeping.count(7, Kind::Item), 20);
        assert_eq!(keeping.number_at(1, Kind::Item), FREE);
        // The number of a free pair is no thing, and a kind without a list takes nothing.
        assert!(!keeping.give(i32::from(FREE), 1, Kind::Item, BARE));
        assert!(!keeping.give(3, 1, Kind::StoryThing, BARE));
        assert_eq!(keeping.count(7, Kind::Weapon), 0);
    }

    #[test]
    fn the_thing_that_is_worn_leaves_room_for_one_less() {
        let mut keeping = Keeping::new();
        let worn = [4, FREE, FREE];
        assert!(keeping.give(4, 19, Kind::Weapon, worn));
        assert!(!keeping.give(4, 1, Kind::Weapon, worn));
        assert!(keeping.give(4, 1, Kind::Weapon, BARE));
        // The weapon's byte says nothing about an armour of the same number, and items
        // are never worn.
        assert!(keeping.give(4, 20, Kind::Armour, worn));
        assert!(keeping.give(4, 20, Kind::Item, worn));
        // Without a pair of its own the worn thing gets a free pair like any other.
        assert!(keeping.give(2, 20, Kind::Ornament, [FREE, FREE, 2]));
        assert_eq!(keeping.count(2, Kind::Ornament), 20);
    }

    #[test]
    fn a_list_holds_sixty_four_things() {
        let mut keeping = Keeping::new();
        for number in 0..PAIRS as i32 {
            assert!(keeping.give(number + 100, 1, Kind::Item, BARE));
        }
        assert!(!keeping.give(1, 1, Kind::Item, BARE));
        assert_eq!(keeping.last_place(Kind::Item), PAIRS - 1);
        assert!(keeping.take(100, 1, Kind::Item));
        assert!(keeping.give(1, 1, Kind::Item, BARE));
        assert_eq!(keeping.number_at(0, Kind::Item), 1);
    }

    #[test]
    fn taking_needs_as_many_as_are_taken() {
        let mut keeping = Keeping::new();
        assert!(!keeping.take(5, 1, Kind::Item));
        keeping.give(5, 3, Kind::Item, BARE);
        assert!(!keeping.take(5, 4, Kind::Item));
        assert!(keeping.take(5, 2, Kind::Item));
        assert_eq!(
            (
                keeping.number_at(0, Kind::Item),
                keeping.count_at(0, Kind::Item)
            ),
            (5, 1)
        );
        assert!(keeping.take(5, 1, Kind::Item));
        assert_eq!(keeping.number_at(0, Kind::Item), FREE);
        assert_eq!(keeping.count(5, Kind::Item), 0);
        assert!(!keeping.take(3, 1, Kind::StoryThing));
    }

    #[test]
    fn a_tidied_list_is_in_the_order_of_the_numbers() {
        let mut keeping = Keeping::new();
        for (number, count) in [(9, 1), (70, 2), (3, 4), (12, 5)] {
            keeping.give(number, count, Kind::Item, BARE);
        }
        keeping.take(70, 2, Kind::Item);
        keeping.give(200, 6, Kind::Item, BARE);
        assert_eq!(keeping.last_place(Kind::Item), 3);
        keeping.tidy(Kind::Item);
        let front: Vec<(u8, u8)> = (0..4)
            .map(|place| {
                (
                    keeping.number_at(place, Kind::Item),
                    keeping.count_at(place, Kind::Item),
                )
            })
            .collect();
        // Number 200 is past the numbers the routine asks for: it stays where the changes
        // of place left it.
        assert_eq!(front, [(3, 4), (9, 1), (12, 5), (200, 6)]);
    }

    #[test]
    fn money_stops_at_its_limit() {
        let mut keeping = Keeping::new();
        assert!(keeping.give_money(MOST_MONEY - 1));
        assert!(keeping.give_money(500));
        assert_eq!(keeping.money(), MOST_MONEY);
        assert!(!keeping.give_money(1));
        assert!(!keeping.take_money(MOST_MONEY + 1));
        assert!(keeping.take_money(MOST_MONEY));
        assert_eq!(keeping.money(), 0);
        assert!(!keeping.take_money(1));
    }

    #[test]
    fn twelve_cocoons_at_most() {
        let mut keeping = Keeping::new();
        assert!(keeping.give_cocoons(12));
        assert!(!keeping.give_cocoons(1));
        assert_eq!(keeping.cocoons(), MOST_COCOONS);
        assert_eq!(keeping.cocoons_carried(2), 14);
        assert_eq!(keeping.cocoons_left(2), 0);
        assert!(keeping.take_cocoons(5));
        assert_eq!(keeping.cocoons_left(2), 3);
        assert!(!keeping.take_cocoons(8));
        assert_eq!(keeping.cocoons(), 0);
    }

    #[test]
    fn things_of_the_story_are_bits() {
        let mut keeping = Keeping::new();
        assert!(keeping.give_story_thing(3));
        assert!(keeping.give_story_thing(37));
        assert!(keeping.give_story_thing(63));
        assert!(!keeping.give_story_thing(64));
        assert!(keeping.has_story_thing(3) && keeping.has_story_thing(63));
        assert!(!keeping.has_story_thing(64) && !keeping.has_story_thing(4));
        // Only the named ones are counted.
        assert_eq!(keeping.story_things(), 2);
        assert_eq!(
            [0, 1, 2].map(|n| keeping.story_thing_at(n)),
            [3, 37, NAMED_STORY_THINGS]
        );
        keeping.take_story_thing(3);
        assert!(!keeping.has_story_thing(3));
        assert_eq!(keeping.story_thing_at(0), 37);
    }

    #[test]
    fn flags_and_the_small_values_lie_in_one_row() {
        let mut keeping = Keeping::new();
        keeping.set_flag(0);
        keeping.set_flag(119);
        assert!(keeping.flag(0) && keeping.flag(119) && !keeping.flag(118));
        keeping.clear_flag(119);
        assert!(!keeping.flag(119));
        keeping.set_byte(0, 0x80);
        keeping.set_byte(19, 7);
        assert_eq!(
            (keeping.byte(0), keeping.byte(19), keeping.byte(20)),
            (0x80, 7, 0)
        );
        // The flag behind the last one is the first bit of the first small value.
        assert!(keeping.flag(FLAGS + 7) && !keeping.flag(FLAGS));
        keeping.set_flag(FLAGS);
        assert_eq!(keeping.byte(0), 0x81);
    }

    #[test]
    fn a_notice_hands_over_and_takes_at_its_end() {
        let mut keeping = Keeping::new();
        keeping.notice_ends(1, 5, 3, 1, BARE);
        keeping.notice_ends(1, 0, 12, 2, BARE);
        keeping.notice_ends(-1, 7, 0, 200, BARE);
        keeping.notice_ends(0, 6, 0, 12, BARE);
        keeping.notice_ends(1, -1, 0, 0, BARE);
        assert!(keeping.has_story_thing(3));
        assert_eq!(keeping.count(12, Kind::Item), 2);
        assert_eq!((keeping.money(), keeping.cocoons()), (200, 12));
        // A used item goes, one at a time; a used thing of the story stays.
        keeping.notice_ends(4, 0, 12, 2, BARE);
        keeping.notice_ends(6, 5, 3, 1, BARE);
        keeping.notice_ends(5, 1, 0, 1, BARE);
        assert_eq!(keeping.count(12, Kind::Item), 1);
        assert!(keeping.has_story_thing(3));
        // Ways 8 to 11 only tell.
        keeping.notice_ends(8, 0, 12, 1, BARE);
        assert_eq!(keeping.count(12, Kind::Item), 1);
    }
}
