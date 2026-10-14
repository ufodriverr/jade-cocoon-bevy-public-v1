
use crate::carried::{Carried, FOREST_VARIABLE};
use crate::rooms::load_address;

/// The archive file of the routine and of its table.
pub const FILE: usize = 53;
/// RAM address of the table of nine words: what story variable 4 gets in the room of
/// the four gates, by the wanted view.
pub const TREES: u32 = 0x800B_6F68;
/// The views of that table.
pub const VIEWS: usize = 9;
/// The action the player is ordered, and the bits of the order (start at the first
/// frame).
pub const ACTION: i16 = 29;
pub const ORDER_KIND: u32 = 9;
pub const ORDER_BITS: u32 = 1;
/// The game frames over which the white grows.
pub const WHITE: u16 = 60;
/// The area of the forests, and the place and slot of the room of the four gates in it.
pub const AREA: i16 = 4;
const GATES: (i16, i16) = (0, 0);
/// What is added to the place of a forest: the gates 5 to 8 are the trees.
const TREE: i32 = 4;
/// The story variables of the stored room, high byte and low byte, and the room they
/// stand for while they read 0.
const STORED: (u32, u32) = (5, 6);
const STORED_AT_FIRST: u16 = 102;

/// Where the room the player leaves from lies: area, place and the slot of its record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct From {
    pub area: i16,
    pub place: i16,
    pub slot: i16,
}

/// The table of the views, read from the file as it is on the disc.
pub fn trees(file: &[u8]) -> Option<[i32; VIEWS]> {
    let at = TREES.checked_sub(load_address(FILE))? as usize;
    let (words, _) = file.get(at..at + 4 * VIEWS)?.as_chunks::<4>();
    let mut trees = [0; VIEWS];
    for (tree, word) in trees.iter_mut().zip(words) {
        *tree = i32::from_le_bytes(*word);
    }
    Some(trees)
}

/// The gate the player comes out at, for story variable 4: in the room of the four
/// gates the table's word of the wanted view, anywhere else the place plus 4. A view
/// past the table reads what lies behind it on the console's stack; nothing here.
pub fn comes_out(trees: &[i32; VIEWS], from: From, view: usize) -> i32 {
    if from.area == AREA && (from.place, from.slot) == GATES {
        return trees.get(view).copied().unwrap_or(0);
    }
    i32::from(from.place) + TREE
}

/// The stored room (`0x800A67CC(-1)`): the story variables 5 and 6, which get room 102
/// when they read 0.
pub fn stored_room(carried: &mut Carried) -> u16 {
    let (high, low) = STORED;
    let room = (carried.variable(high) << 8) + carried.variable(low);
    if room != 0 {
        return room as u16;
    }
    carried.set_variable(high, 0);
    carried.set_variable(low, i32::from(STORED_AT_FIRST));
    STORED_AT_FIRST
}

pub fn store_room(carried: &mut Carried, room: i32) -> u16 {
    let before = stored_room(carried);
    if room >= 0 {
        let (high, low) = STORED;
        carried.set_variable(high, room >> 8);
        carried.set_variable(low, room & 0xFF);
    }
    before
}

/// The end of the task as far as it writes the state a room hands on: story variable 4
/// and the stored room, which is the room the game goes to, the record of that number
/// itself. The corridor of the second round ends with it (`0x800BCF64`: bit `0x20000`
/// and the word `0x80091C1C` are cleared).
pub fn leave(carried: &mut Carried, trees: &[i32; VIEWS], from: From, view: usize) -> u16 {
    carried.corridor = None;
    carried.set_variable(FOREST_VARIABLE, comes_out(trees, from, view));
    stored_room(carried)
}

pub const LOST_FLAG: u32 = 0;
pub const LOST_WHITE: u16 = 60;

pub fn lost(carried: &mut Carried, place: i16) -> u16 {
    carried.corridor = None;
    carried.set_variable(FOREST_VARIABLE, i32::from(place) + TREE);
    let room = stored_room(carried);
    carried.keeping.set_flag(LOST_FLAG);
    room
}

/// What the task asks of the field.
pub trait Field {
    /// `0x800A1894`: what a script's opening of a section does.
    fn open(&mut self);
    /// The player is taken as a script takes him and stands again.
    fn take(&mut self);
    /// The loader is asked for the animation of an action of the player.
    fn read(&mut self, action: i16);
    /// The loader of the player's model is through.
    fn is_read(&mut self) -> bool;
    /// The player is free for an order, which is the test for "ready" too.
    fn ready(&mut self) -> bool;
    /// An order of kind 9: the action from its first frame.
    fn order(&mut self, action: i16);
    /// A white grows over the picture and stays.
    fn whiten(&mut self, frames: u16);
    /// The fade record is at rest.
    fn at_rest(&mut self) -> bool;
    /// The animation of the action goes back to the loader.
    fn give_back(&mut self, action: i16);
    /// The player is given back to the pad.
    fn free(&mut self);
}

/// The task's block: the number of its state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Leaving {
    pub state: u32,
}

impl Leaving {
    /// One pass (`0x800BCD00`): a pass does one state, and a state that is through only
    /// writes the next number. True in the pass of the end, behind which the room goes.
    pub fn pass(&mut self, field: &mut impl Field) -> bool {
        match self.state {
            0 => {
                field.open();
                field.take();
                field.read(ACTION);
                self.state = 1;
            }
            1 if field.is_read() => self.state = 2,
            2 if field.ready() => self.state = 3,
            3 if field.ready() => {
                field.order(ACTION);
                field.whiten(WHITE);
                self.state = 4;
            }
            4 if field.at_rest() => {
                field.give_back(ACTION);
                field.free();
                return true;
            }
            _ => {}
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fade::Fade;

    /// A field whose loader needs a pass and whose fade is the game's record.
    #[derive(Default)]
    struct Told {
        calls: Vec<&'static str>,
        asked: bool,
        fade: Fade,
    }

    impl Field for Told {
        fn open(&mut self) {
            self.calls.push("open");
        }
        fn take(&mut self) {
            self.calls.push("take");
        }
        fn read(&mut self, action: i16) {
            assert_eq!(action, ACTION);
            self.calls.push("read");
        }
        fn is_read(&mut self) -> bool {
            std::mem::replace(&mut self.asked, true)
        }
        fn ready(&mut self) -> bool {
            true
        }
        fn order(&mut self, action: i16) {
            assert_eq!(action, ACTION);
            self.calls.push("order");
        }
        fn whiten(&mut self, frames: u16) {
            self.fade.whiten(frames);
            self.calls.push("whiten");
        }
        fn at_rest(&mut self) -> bool {
            self.fade.at_rest()
        }
        fn give_back(&mut self, _: i16) {
            self.calls.push("give back");
        }
        fn free(&mut self) {
            self.calls.push("free");
        }
    }

    #[test]
    fn a_pass_does_one_state_and_the_room_goes_when_the_white_is_whole() {
        let (mut leaving, mut field) = (Leaving::default(), Told::default());
        // The fade's task steps behind the task of every game frame.
        let pass = |leaving: &mut Leaving, field: &mut Told| {
            let goes = leaving.pass(field);
            field.fade.step();
            goes
        };
        assert!(!pass(&mut leaving, &mut field));
        assert_eq!(
            (leaving.state, &field.calls[..]),
            (1, &["open", "take", "read"][..])
        );
        // The loader is not through in the first pass of state 1.
        assert!(!pass(&mut leaving, &mut field));
        assert_eq!(leaving.state, 1);
        assert!(!pass(&mut leaving, &mut field));
        assert!(!pass(&mut leaving, &mut field));
        assert_eq!((leaving.state, field.calls.len()), (3, 3));
        assert!(!pass(&mut leaving, &mut field));
        assert_eq!(
            (leaving.state, &field.calls[3..]),
            (4, &["order", "whiten"][..])
        );
        // The room goes 59 game frames behind the order.
        let waited = (1..).find(|_| pass(&mut leaving, &mut field));
        assert_eq!(waited, Some(59));
        assert_eq!(&field.calls[5..], &["give back", "free"][..]);
    }

    #[test]
    fn the_gate_is_the_tree_of_the_forest_or_the_views_in_the_room_of_the_gates() {
        let trees = [0, 0, 0, 0, 0, 5, 7, 6, 8];
        let from = |place, slot| From {
            area: AREA,
            place,
            slot,
        };
        assert_eq!(comes_out(&trees, from(1, 7), 0), 5);
        assert_eq!(comes_out(&trees, from(4, 0), 6), 8);
        assert_eq!(comes_out(&trees, from(0, 0), 6), 7);
        assert_eq!(comes_out(&trees, from(0, 0), 2), 0);
        assert_eq!(comes_out(&trees, from(0, 0), 9), 0);
        // Another slot of the gates' place, and another area, go by the place.
        assert_eq!(comes_out(&trees, from(0, 1), 6), 4);
        let village = From {
            area: 1,
            place: 0,
            slot: 0,
        };
        assert_eq!(comes_out(&trees, village, 6), 4);
    }

    #[test]
    fn the_end_writes_the_gate_and_takes_the_stored_room() {
        let mut carried = Carried::new_game();
        let trees = [0, 0, 0, 0, 0, 5, 7, 6, 8];
        let from = From {
            area: AREA,
            place: 1,
            slot: 2,
        };
        // A new game has no stored room: it is room 102 from the first asking on.
        assert_eq!(leave(&mut carried, &trees, from, 3), 102);
        assert_eq!(carried.variable(FOREST_VARIABLE), 5);
        assert_eq!((carried.variable(5), carried.variable(6)), (0, 102));
        // A script's stored room is two bytes.
        carried.set_variable(5, 1);
        carried.set_variable(6, 84);
        assert_eq!(stored_room(&mut carried), 340);
    }

    #[test]
    fn a_room_is_stored_as_two_bytes_and_a_negative_one_is_not() {
        let mut carried = Carried::new_game();
        // The first storing finds nothing, which reads as room 102.
        assert_eq!(store_room(&mut carried, 104), 102);
        assert_eq!((carried.variable(5), carried.variable(6)), (0, 104));
        assert_eq!(store_room(&mut carried, 340), 104);
        assert_eq!((carried.variable(5), carried.variable(6)), (1, 84));
        assert_eq!(store_room(&mut carried, -1), 340);
        assert_eq!(stored_room(&mut carried), 340);
        assert_eq!(lost(&mut carried, 4), 340);
    }

    #[test]
    fn a_lost_fight_goes_by_the_place_alone_and_sets_the_story_flag() {
        let mut carried = Carried::new_game();
        assert!(!carried.keeping.flag(LOST_FLAG));
        assert_eq!(lost(&mut carried, 1), 102);
        assert_eq!(carried.variable(FOREST_VARIABLE), 5);
        assert!(carried.keeping.flag(LOST_FLAG));
        // No table of views: the place of the gates' room gives 4.
        carried.set_variable(5, 1);
        carried.set_variable(6, 84);
        assert_eq!(lost(&mut carried, 0), 340);
        assert_eq!(carried.variable(FOREST_VARIABLE), 4);
    }

    #[test]
    fn both_ways_out_end_the_corridor_of_the_second_round() {
        use crate::corridor::Corridor;
        let corridor = Corridor {
            nodes: Vec::new(),
            at: 0,
            arrival: 0,
            floor: 3,
            look: 0,
            species: [57; 3],
        };
        let from = From {
            area: 0,
            place: 3,
            slot: 20,
        };
        let mut carried = Carried::new_game();
        carried.corridor = Some(corridor.clone());
        lost(&mut carried, from.place);
        assert_eq!(carried.corridor, None);
        carried.corridor = Some(corridor);
        leave(&mut carried, &[0; VIEWS], from, 0);
        assert_eq!(carried.corridor, None);
    }

    #[test]
    fn the_table_is_read_at_its_address() {
        let at = (TREES - load_address(FILE)) as usize;
        let mut file = vec![0u8; at + 4 * VIEWS];
        let (words, _) = file[at..].as_chunks_mut::<4>();
        for (view, word) in words.iter_mut().enumerate() {
            *word = (view as i32 - 3).to_le_bytes();
        }
        assert_eq!(trees(&file), Some([-3, -2, -1, 0, 1, 2, 3, 4, 5]));
        assert_eq!(trees(&file[..at + 35]), None);
    }
}
