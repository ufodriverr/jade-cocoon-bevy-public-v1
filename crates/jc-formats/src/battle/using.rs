
use super::record::{Draws, Record, Side, status};
use super::resolve::{Command, NO_DEVA, Order, Resolved, resolve};
use super::{Tables, item};

/// The answers of the test of a list's row (`0x80033B58`).
pub mod row {
    pub const FINE: i16 = 0;
    /// The place holds nothing.
    pub const NO_PIECE: i16 = 1;
    /// The item is for neither a fight nor the time outside one.
    pub const NEVER: i16 = 2;
    /// Not outside a fight, or not in this room.
    pub const NOT_HERE: i16 = 3;
    /// Not in a fight.
    pub const NOT_IN_A_FIGHT: i16 = 4;
}

/// The answers of the test of an item's target (`0x80033E50`).
pub mod aim {
    pub const FINE: i16 = 0;
    pub const NOT_FOR_CREATURES: i16 = 1;
    pub const NOT_FOR_THE_PLAYER: i16 = 2;
    pub const NO_NEED: i16 = 3;
    pub const DEAD: i16 = 4;
}

/// The item that only works in a forest when it is used outside a fight.
pub const ONLY_IN_A_FOREST: usize = 41;
/// The one item that may be aimed at the dead.
const REVIVES: usize = 13;
/// The bad statuses item 13 clears.
const BAD: u32 = 0x07FB;
/// The highest HP a record holds and the highest stat an item raises.
const HIGHEST_POINTS: i16 = 999;
const HIGHEST_STAT: u8 = 99;
/// The stat each of the items 4 to 8 raises, as a place of a record's six.
const RAISED: [usize; 5] = [0, 4, 1, 2, 3];

/// The test of a list's row (`0x80033B58`): `count` pieces of `number` lie there, the
/// item has `flags`, and the list is the one of a fight or of the player's menu.
pub fn row_test(count: u8, number: usize, flags: u16, in_a_fight: bool, in_a_forest: bool) -> i16 {
    if count == 0 {
        return row::NO_PIECE;
    }
    if number == ONLY_IN_A_FOREST && !in_a_fight {
        return match in_a_forest {
            true => row::FINE,
            false => row::NOT_HERE,
        };
    }
    if flags & (item::OUTSIDE_A_FIGHT | item::IN_A_FIGHT) == 0 {
        return row::NEVER;
    }
    match in_a_fight {
        true if flags & item::IN_A_FIGHT == 0 => row::NOT_IN_A_FIGHT,
        false if flags & item::OUTSIDE_A_FIGHT == 0 => row::NOT_HERE,
        _ => row::FINE,
    }
}

/// The test of an item's target (`0x80033E50`): target 0 is the player, 1 to 3 a
/// carried creature, and `record` the target's.
pub fn aim_test(number: usize, flags: u16, target: usize, record: &Record) -> i16 {
    if flags & (item::AT_THE_OPPONENTS | item::ALL_OF_A_SIDE) != 0 {
        return aim::FINE;
    }
    if flags & item::ON_THE_PLAYER == 0 && target == 0 {
        return aim::NOT_FOR_THE_PLAYER;
    }
    if flags & item::ON_A_MINION == 0 && (1..=3).contains(&target) {
        return aim::NOT_FOR_CREATURES;
    }
    if number != REVIVES && record.is_dead() {
        return aim::DEAD;
    }
    let (hp_full, mp_full) = (record.hp == record.max_hp, record.mp == record.max_mp);
    let nothing = match number {
        0..=2 => hp_full,
        3 => record.max_hp == HIGHEST_POINTS,
        4..=8 => record.stats[RAISED[number - 4]] >= HIGHEST_STAT,
        9..=12 => hp_full && mp_full,
        REVIVES => record.status & BAD == 0 && hp_full && mp_full,
        14 => record.status & status::POISON == 0,
        15 => record.status & status::STONE == 0,
        _ => false,
    };
    match nothing {
        true => aim::NO_NEED,
        false => aim::FINE,
    }
}

/// What the items 3 to 8 do before the resolver is asked (`0x80013C28`): item 3 raises
/// the highest HP and the HP by 1, 2 or 3 (one draw of six: a 0 gives 1, a 5 gives 3),
/// the items 4 to 8 a stat by 1, or by 2 for one draw of six.
pub fn raise(number: usize, record: &mut Record, draws: &mut impl Draws) {
    match number {
        3 => {
            let by = match draws.draw() % 6 {
                0 => 1,
                5 => 3,
                _ => 2,
            };
            record.max_hp = (record.max_hp + by).clamp(0, HIGHEST_POINTS);
            record.hp = (record.hp + by).clamp(0, record.max_hp);
        }
        4..=8 => {
            let by = match draws.draw() % 6 {
                0 => 2,
                _ => 1,
            };
            let stat = &mut record.stats[RAISED[number - 4]];
            *stat = (*stat + by).min(HIGHEST_STAT);
        }
        _ => {}
    }
}

/// What the resolver answers for an item the player uses on `target` outside a fight
/// (`0x80012BB0` behind `0x80013C28`): no deva is on a field.
pub fn resolved(
    tables: &Tables,
    number: usize,
    player: &Record,
    target: &Record,
    draws: &mut impl Draws,
) -> Option<Resolved> {
    let order = Order {
        command: Command::Item(number),
        actor: player,
        actor_side: Side::Player,
        target,
        target_side: Side::Player,
        deva: NO_DEVA,
        lock: NO_DEVA,
        odd_frame: false,
    };
    resolve(tables, &order, draws)
}

/// A result is written into the record it was decided for (`0x800C02F0` of file 42,
/// through `0x80012614`): the HP and the MP within 0 and their highest, the bits to
/// clear and then the bits to add, of which the stone's are dropped for a record that
/// already has one of them.
pub fn apply(result: &Resolved, record: &mut Record) {
    let within = |value: i16, change: i32, most: i16| {
        (i32::from(value) - change).clamp(0, i32::from(most)) as i16
    };
    record.hp = within(record.hp, result.hp, record.max_hp);
    record.mp = within(record.mp, result.mp, record.max_mp);
    record.status &= !result.clear;
    let add = match record.status & status::STONE != 0 {
        true => result.add & !status::STONE,
        false => result.add,
    };
    record.status |= add;
}

/// The HP and the MP a use of the item would give the target, as the menu shows them
/// before the confirm (`0x800C0E9C` of file 42): nothing where the target's test
/// refuses, nothing for an item that is not one of 0 to 2 and 9 to 13, and no more
/// than the record lacks. The resolver makes its draws each time this is asked, and the
/// menu asks in every pass in which it draws the box.
pub fn would_give(
    tables: &Tables,
    number: usize,
    flags: u16,
    target: usize,
    player: &Record,
    record: &Record,
    draws: &mut impl Draws,
) -> (i32, i32) {
    let shown = matches!(number, 0..=2 | 9..=13);
    if aim_test(number, flags, target, record) != aim::FINE || !shown {
        return (0, 0);
    }
    let Some(result) = resolved(tables, number, player, record, draws) else {
        return (0, 0);
    };
    let lacks = |value: i16, most: i16| i32::from(most) - i32::from(value);
    (
        (-result.hp).min(lacks(record.hp, record.max_hp)),
        (-result.mp).min(lacks(record.mp, record.max_mp)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> Record {
        let mut bytes = [0u8; Record::HEAD];
        bytes[8..16].copy_from_slice(&[30, 0, 50, 0, 4, 0, 9, 0]);
        Record::head(&bytes).expect("a head")
    }

    #[test]
    fn a_row_is_tested_by_its_pieces_its_flags_and_the_place() {
        assert_eq!(row_test(0, 0, 0x4B, false, false), row::NO_PIECE);
        assert_eq!(row_test(1, 0, 0x4B, false, false), row::FINE);
        assert_eq!(row_test(1, 0, 0x4B, true, false), row::FINE);
        // An item with neither bit, and one for a fight alone.
        assert_eq!(row_test(1, 33, 0x00, false, false), row::NEVER);
        assert_eq!(row_test(1, 14, 0x5A, false, false), row::NOT_HERE);
        assert_eq!(row_test(1, 3, 0x49, true, false), row::NOT_IN_A_FIGHT);
        // Item 41 outside a fight goes by the room alone.
        assert_eq!(row_test(1, 41, 0x61, false, false), row::NOT_HERE);
        assert_eq!(row_test(1, 41, 0x61, false, true), row::FINE);
        assert_eq!(row_test(1, 42, 0x61, false, false), row::FINE);
    }

    #[test]
    fn a_target_is_tested_in_the_order_of_the_game() {
        let mut hurt = record();
        assert_eq!(aim_test(25, 0x82, 0, &hurt), aim::FINE);
        assert_eq!(aim_test(4, 0x51, 0, &hurt), aim::NOT_FOR_THE_PLAYER);
        assert_eq!(aim_test(0, 0x4B, 2, &hurt), aim::NOT_FOR_CREATURES);
        assert_eq!(aim_test(0, 0x4B, 0, &hurt), aim::FINE);
        assert_eq!(aim_test(9, 0x53, 1, &hurt), aim::FINE);
        hurt.status = status::DEAD;
        assert_eq!(aim_test(9, 0x53, 1, &hurt), aim::DEAD);
        assert_eq!(aim_test(13, 0x5B, 1, &hurt), aim::FINE);
        let mut whole = record();
        (whole.hp, whole.mp) = (whole.max_hp, whole.max_mp);
        assert_eq!(aim_test(0, 0x4B, 0, &whole), aim::NO_NEED);
        assert_eq!(aim_test(9, 0x53, 1, &whole), aim::NO_NEED);
        assert_eq!(aim_test(13, 0x5B, 1, &whole), aim::NO_NEED);
        assert_eq!(aim_test(14, 0x5A, 1, &whole), aim::NO_NEED);
        whole.status = status::POISON;
        assert_eq!(aim_test(13, 0x5B, 1, &whole), aim::FINE);
        assert_eq!(aim_test(14, 0x5A, 1, &whole), aim::FINE);
        assert_eq!(aim_test(15, 0x5A, 1, &whole), aim::NO_NEED);
        // The raisers: the highest HP for item 3, a stat for 4 to 8.
        assert_eq!(aim_test(3, 0x49, 0, &whole), aim::FINE);
        whole.max_hp = 999;
        assert_eq!(aim_test(3, 0x49, 0, &whole), aim::NO_NEED);
        whole.stats = [99, 98, 98, 98, 100, 0];
        assert_eq!(aim_test(4, 0x51, 1, &whole), aim::NO_NEED);
        assert_eq!(aim_test(5, 0x51, 1, &whole), aim::NO_NEED);
        assert_eq!(aim_test(6, 0x51, 1, &whole), aim::FINE);
    }

    #[test]
    fn the_raisers_make_one_draw_each() {
        let mut raised = record();
        let draws = [0u32, 5, 3, 6, 7];
        let mut left = draws.iter().copied();
        let mut draw = || left.next().expect("a draw");
        raise(3, &mut raised, &mut draw);
        assert_eq!((raised.hp, raised.max_hp), (31, 51));
        raise(3, &mut raised, &mut draw);
        assert_eq!((raised.hp, raised.max_hp), (34, 54));
        raise(3, &mut raised, &mut draw);
        assert_eq!((raised.hp, raised.max_hp), (36, 56));
        raise(5, &mut raised, &mut draw);
        raise(8, &mut raised, &mut draw);
        assert_eq!(raised.stats, [0, 0, 0, 1, 2, 0]);
        // No other item draws.
        raise(0, &mut raised, &mut draw);
        assert_eq!(left.next(), None);
        raised.stats[0] = 98;
        raise(4, &mut raised, &mut || 0);
        assert_eq!(raised.stats[0], 99);
        raised.max_hp = 998;
        raise(3, &mut raised, &mut || 5);
        assert_eq!((raised.hp, raised.max_hp), (39, 999));
    }

    #[test]
    fn a_result_is_written_into_its_record() {
        let mut hurt = record();
        hurt.status = status::POISON | 0x0008;
        let result = Resolved {
            hp: -40,
            mp: 9,
            clear: status::POISON,
            add: 0x0010 | status::SLEEP,
            ..Resolved::default()
        };
        apply(&result, &mut hurt);
        assert_eq!((hurt.hp, hurt.mp), (50, 0));
        // A record that turns to stone takes no other step of it.
        assert_eq!(hurt.status, 0x0008 | status::SLEEP);
    }
}
