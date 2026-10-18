
use crate::battle::record::Record;
use crate::carried::Carried;
use crate::creatures::{self, Creature, Listed, MOST_CARRIED};
use crate::fade::Fade;
use crate::items::{self, Keeping};
use crate::menu::{self, Cursor, cursor};
use crate::pad::{self, Pad};
use crate::player_menu::{
    MESSAGE_PASSES, OPEN, SOUND_CANCEL, SOUND_CONFIRM, SOUND_CURSOR, SOUND_REFUSED,
};
use crate::player_panel::{attacks, spells, spells_known};

/// The archive file of the screen and the RAM address of its first byte.
pub const MERGE_FILE: usize = 44;
pub const MERGE_FILE_BASE: u32 = 0x800B_6ED0;
/// The kind of script command `0x59` that asks for the screen.
pub const KIND: u32 = 2;
/// The kind that calls nothing: with the screen's task asleep behind an answer, file
/// 43's watch takes it as the step that was asked for and the screen is back.
pub const BACK_KIND: u32 = 3;

/// The states.
pub const STATES: usize = 16;
pub mod state {
    /// The main menu of five rows.
    pub const MAIN: i16 = 0;
    /// The merge: the first creature, the second, the question, "once more?".
    pub const MERGE_FIRST: i16 = 1;
    pub const MERGE_SECOND: i16 = 2;
    pub const MERGE_ASKED: i16 = 3;
    pub const MERGE_MORE: i16 = 4;
    /// The spinning: the cocoons are marked, and the question.
    pub const SPIN: i16 = 5;
    pub const SPIN_ASKED: i16 = 6;
    /// Who goes with the player: a place is picked, and the place it changes with.
    pub const TAKE: i16 = 7;
    pub const EXCHANGE: i16 = 10;
    /// The view of the creatures, the question of a new name, a creature's lists.
    pub const VIEW: i16 = 8;
    pub const RENAME_ASKED: i16 = 9;
    pub const ABILITIES: i16 = 15;
    /// The keeper's last line, behind which the screen leaves; and her line when too
    /// many are kept to leave, behind which the main menu is back.
    pub const DONE: i16 = 11;
    pub const FULL: i16 = 12;
    /// The question of the minion skins and their list.
    pub const SKINS_ASKED: i16 = 13;
    pub const SKINS: i16 = 14;
}

/// Where a confirm leads by the state (the half words at `0x800BFA44`): a state, or -1
/// for a case of its own, -3 for the worker of the state, anything else for nothing.
pub const CONFIRM: [i16; STATES] = [-1, 2, -1, -3, -1, 6, -3, 10, -1, -3, -3, -3, -3, -1, -1, -4];
/// The state a state's own way back leads to (the half words at `0x800BFA64`).
pub const CANCEL: [i16; STATES] = [-2, 0, 1, 4, 0, 0, 5, 0, 0, 8, 7, 0, 0, 2, 3, 8];
/// The states behind the first four rows of the main menu (`0x800BFA84`).
pub const ROWS: [i16; 4] = [state::MERGE_FIRST, state::SPIN, state::TAKE, state::VIEW];
/// The row of the main menu that stops.
pub const STOP: i16 = 4;
/// The panels of each state, a bit a panel (the half words at `0x800BFA90`).
pub const PANELS: [u32; STATES] = [
    0x01, 0x06, 0x16, 0x3E, 0x0A, 0x06, 0x0E, 0x06, 0x06, 0x0E, 0x06, 0x00, 0x01, 0x3A, 0x72, 0x06,
];
/// The slots in which a state keeps its cursor, its scroll and its list while another
/// state is up, -1 for none (the half words at `0x800BFEF8`, `0x800BFF18`, `0x800BFF38`).
pub const CURSOR_SLOT: [i16; STATES] = [0, 1, 2, 3, 4, 1, 5, 1, 1, 6, 2, 1, 1, 7, 8, 9];
pub const SCROLL_SLOT: [i16; STATES] = [-1, 0, 1, -1, -1, 0, -1, 0, 0, -1, 1, -1, -1, -1, 2, 3];
pub const LIST_SLOT: [i16; STATES] = [-1, 0, 1, -1, -1, 0, -1, 0, 0, -1, 1, -1, -1, -1, -1, -1];
/// The cursor record of each state: columns, rows, lines, flag word (`0x800BF9C4`). A
/// state of the two lists has the record of the list the cursor is in instead.
pub const CURSORS: [[i16; 4]; STATES] = [
    [1, 5, 5, 2],
    [1, 4, 13, 2],
    [1, 4, 13, 2],
    [1, 2, 2, 2],
    [1, 2, 2, 2],
    [1, 4, 13, 2],
    [1, 2, 2, 2],
    [1, 4, 13, 2],
    [1, 4, 13, 2],
    [1, 2, 2, 2],
    [1, 4, 13, 2],
    [1, 1, 1, 2],
    [1, 1, 1, 2],
    [1, 2, 2, 2],
    [1, 4, 4, 64],
    [1, 1, 1, 64],
];
/// The cursor record in the list of the carried, which is left by its lower edge, and
/// in the list of the kept, which scrolls and is left by its upper edge (`0x800BDA30`,
/// `0x800BBFF4`).
pub const CARRIED_CURSOR: [i16; 4] = [1, 3, 3, cursor::LEAVES_DOWN as i16];
pub const KEPT_CURSOR: [i16; 4] = [
    1,
    4,
    (creatures::PLACES - MOST_CARRIED) as i16,
    (cursor::LEAVES_UP | cursor::SCROLLS) as i16,
];
/// The panel of the bar under the cursor of an answer.
pub const ANSWER_PANEL: u32 = 1 << 3;
/// What the screen answers the room's script behind a spin (`0x800A5F2C(2)`), which the
/// script's command `0x59` takes as its choice; a merge answers 1 and the leaving 0.
pub const ANSWER_OF_A_SPIN: i16 = 2;
/// And behind the yes of a merge.
pub const ANSWER_OF_A_MERGE: i16 = 1;
/// The two species a merge can make new (`0x800BAEA0`), whose kinds its yes marks as
/// caught: the bits 2 and 3 of the second word of the caught kinds.
pub const NEW_SPECIES: [u8; 2] = [34, 35];
/// The game frames over which the white of a spin grows, and over which it goes
/// (`0x8001AAA8(32)`, `0x8001AB1C(32)`).
pub const WHITE: u16 = 32;
/// The most creatures that can stay behind for the screen to leave.
pub const MOST_KEPT: i16 = 12;
/// The cursor of state 3 over the three creatures of a merge (+52, set up by
/// `0x800BB318`): the first, the second and the new one, which it starts on.
pub const OF_THREE: [i16; 4] = [3, 1, 3, cursor::WRAPS_ACROSS as i16];
pub const ON_THE_NEW: i16 = 2;
/// The new creature of a merge as a place: it lies aside (`0x800C0AC0`), in no place
/// of the list, until the yes puts it over the first.
pub const MADE: usize = creatures::PLACES;
/// The rows of the list of the minion skins, and the lines its cursor has at least
/// besides the one behind the last skin (`0x800BDB74`).
pub const SKIN_ROWS: usize = 4;
const LEAST_SKIN_LINES: usize = 3;

/// Bits of the flag word (+36).
pub mod flag {
    /// The counts of the kept and of the carried are to be taken anew.
    pub const COUNT: u32 = 0x01;
    /// The keeper's second lines of the state are up (the states 3 and 13) and wait
    /// for a button: no pad routine, no bar of an answer, and the sign of "more".
    pub const MORE: u32 = 0x02;
    /// The task ends.
    pub const ENDS: u32 = 0x04;
    /// The state has the two lists.
    pub const LISTS: u32 = 0x08;
    /// The cursor is in the list of the carried.
    pub const IN_CARRIED: u32 = 0x10;
    /// A creature's form is looked at: the task has left its step.
    pub const FORM: u32 = 0x20;
    /// The page of the creature's box that is up in place of its "Detail": its five
    /// body attacks, its spells, its "Family". None of the three is the "Detail".
    pub const ATTACKS: u32 = 0x40;
    pub const SPELLS: u32 = 0x80;
    pub const FAMILY: u32 = 0x100;
    pub const PAGES: u32 = ATTACKS | SPELLS | FAMILY;
    /// The merge takes the minion skin under the cursor of the list of the skins.
    pub const SKIN: u32 = 0x200;
    /// The square asked for the pages: the confirm it notes leads to them. In state 3
    /// the pages are up while it is set.
    pub const ABILITIES: u32 = 0x400;
    /// The white of a spin or of a merge is over the screen: the bar of an answer is
    /// not drawn.
    pub const WHITE: u32 = 0x800;
}

/// The item a creature is spun into (`0x800BB050`): one of the seven silks, by its
/// level alone.
pub fn silk(level: u8) -> u8 {
    match level {
        0..=3 => 33,
        4..=6 => 34,
        7..=11 => 35,
        12..=17 => 36,
        18..=24 => 37,
        25..=30 => 38,
        _ => 39,
    }
}

/// The refusals in front of a confirm (`0x800BD150`).
pub const TOO_FEW: i16 = 1;
pub const NONE: i16 = 2;
pub const SAME: i16 = 3;
pub const NO_SKIN: i16 = 4;

/// Which three lines stand in the keeper's box (`0x800BBD24`, `0x800BBC7C`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Said {
    /// The lines of a row of the main menu.
    Row(i16),
    /// The lines of a state.
    State(i16),
    /// The lines of a refusal.
    Refusal(i16),
    /// The second lines of a state (kind 3), which the states 3 and 13 say first.
    Second(i16),
    /// The three fixed lines while a creature's form is looked at (kind 2).
    Form,
}

/// The look at a creature's form (`0x800BE490`, the record at `0x800C0BB8`): a model
/// job is asked for the creature, and when it is through an actor is made of it at the
/// room's origin, which left and right turn and two shoulder buttons make smaller and
/// larger until a cancel ends it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Form {
    /// The place whose creature is looked at ([`MADE`] for the new one of a merge).
    pub place: usize,
    /// The model job is through and the actor is made (`0x800BE6AC`).
    pub made: bool,
    /// The actor's turn about the upright axis (+94 of the actor), 4096 a whole turn.
    pub turn: i16,
    /// The actor's own scale (+312, +316, +320), which its maker takes from the stage
    /// of the model: the caller's, who has the model ([`MergeScreen::form_stands`]).
    pub scale: Option<[i32; 3]>,
}

impl Form {
    /// What a pass adds to the turn while left is held, and takes away while right is.
    pub const TURN: i16 = 136;
    /// What a pass adds to each of the three scales while R1 is held, and takes away
    /// while R2 is.
    pub const STEP: i32 = 64;
    /// The first scale stays below this and above that.
    pub const LARGEST: i32 = 4096;
    pub const SMALLEST: i32 = 1024;
    /// The action the actor is made in, from the action's first frame (the record of
    /// `0x800BF750`, +32 and +36).
    pub const ACTION: i32 = 4;

    /// The turn after a pass with these buttons held (`0x800BE53C`): left wins.
    pub fn turned(turn: i16, held: u16) -> i16 {
        if held & pad::LEFT != 0 {
            turn.wrapping_add(Self::TURN)
        } else if held & pad::RIGHT != 0 {
            turn.wrapping_sub(Self::TURN)
        } else {
            turn
        }
    }

    /// The three scales after a pass with these buttons held (`0x800BE580`): R1 wins.
    /// Only the first is tested, and all three take the step or none does: a step up
    /// that would reach 4096 is not made, nor a step down to 1024 or below.
    pub fn scaled(scale: [i32; 3], held: u16) -> [i32; 3] {
        let step = if held & pad::R1 != 0 {
            Self::STEP
        } else if held & pad::R2 != 0 {
            -Self::STEP
        } else {
            return scale;
        };
        let first = scale[0].wrapping_add(step);
        match (step > 0 && first < Self::LARGEST) || (step < 0 && first > Self::SMALLEST) {
            true => scale.map(|part| part.wrapping_add(step)),
            false => scale,
        }
    }
}

/// A merge the screen asks for (`0x800BD8FC`): the place picked first, the place
/// picked behind it, and the minion skin it takes, a thing of the player's kind 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Merging {
    pub first: usize,
    pub second: usize,
    pub skin: Option<u8>,
}

/// A merge the yes has taken (`0x800BDC3C`, `0x800BE12C`): the new creature is in
/// the first place and the second is free. The caller takes the minion skin from the
/// player (`0x800AD3B0(thing, 1, 4)`, then `0x800AD5C0(4)`) and marks the kind of a
/// new species as caught.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Merged {
    pub first: usize,
    pub second: usize,
    pub skin: Option<u8>,
    pub new_species: Option<u8>,
}

/// What a pass of the screen did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Passed {
    /// The sounds asked for, in their order.
    pub sounds: Vec<u8>,
    /// Two places have changed with each other.
    pub exchanged: Option<(usize, usize)>,
    /// "Rearrange" has closed gaps of the kept places.
    pub rearranged: bool,
    /// The yes of a merge was taken in this pass.
    pub merged: Option<Merged>,
    /// The task goes to sleep for the name entry on the name of this place's creature.
    pub names: Option<usize>,
    /// The items the marked creatures were spun into, one each, in the order of their
    /// places: the caller gives them to the player (`0x800AD354(item, 1, 0)`).
    pub spun: Vec<u8>,
    /// The task goes to sleep and answers the room's script this, which shows the
    /// room again and says and plays its part: [`MergeScreen::back`] wakes the task
    /// when the script asks for the screen once more (`0x59` of kind 3).
    pub answers: Option<i16>,
    /// The two places are to be merged in this pass: the caller, who has the tables
    /// of the fights, hands them to [`MergeScreen::merge`] before it draws.
    pub merges: Option<Merging>,
    /// "View form": the model job is asked for the creature of this place.
    pub looks: Option<usize>,
    /// The look at a form is over: its actor is ended and the model job given back.
    pub looked: bool,
    /// The screen leaves in this pass.
    pub ended: bool,
}

/// The white over the screen behind a yes with the row "Magic FX" off: it grows and
/// stays (`0x800BE020`), then it goes (`0x800BE084`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum White {
    Grows,
    Goes,
}

/// What the white stands for (the half word +148): 1 a merge, 2 a spin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Yes {
    Merge,
    Spin,
}

/// The block of the screen's task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeScreen {
    /// The argument of the script's command (+0x96): 4 picks the second set of texts.
    pub argument: i16,
    /// The state (+0) and the state to go back to (+2).
    pub state: i16,
    pub back: i16,
    /// The counts of the kept and of the carried places (+4, +6).
    pub kept: i16,
    pub carried: i16,
    /// The clock of the panels (+12), which counts to 8.
    pub clock: i16,
    confirm: bool,
    cancel: bool,
    /// The refusal that is up (+8 with +24), 0 for none, and its passes (+0x98).
    pub refusal: i16,
    pub refusal_passes: i16,
    /// The panels asked for (+28) and those that are open (+32).
    pub panels: u32,
    pub opened: u32,
    /// The flag word (+36).
    pub flags: u32,
    /// The cursor record (+40) and the list it is in (+126): 0 the carried, 1 the kept.
    pub cursor: Cursor,
    pub list: i16,
    /// The second cursor record (+64): the row of a page of the creature's box.
    pub page_cursor: Cursor,
    /// The cursor of state 3 over the three creatures of a merge (+52).
    pub which: Cursor,
    /// The creature that lies aside (`0x800C0AC0`): a copy of the first one from the
    /// first pick on, the new creature once the two are merged.
    pub made: Option<Creature>,
    /// What the player has, for the list of the minion skins (his things of kind 4).
    pub things: Keeping,
    /// The cursor as a confirm on a list left it: in the carried (+88), and in the
    /// kept with its scroll (+90, +92).
    saved: (i16, i16, i16),
    cursor_slots: [i16; 10],
    scroll_slots: [i16; 4],
    list_slots: [i16; 2],
    /// The place picked first (+76) and the place under the cursor of the state behind
    /// it (+80).
    pub first: usize,
    pub second: usize,
    /// The marks: a bit a place (+132), a bit a row of the carried (+136) and a bit a
    /// row of the kept (+140).
    pub marks: u32,
    pub carried_marks: u32,
    pub kept_marks: u32,
    /// The count of the marks of state 5 (+128): a mark set adds one and a mark taken
    /// away subtracts one, and nothing else changes it, not even what clears the
    /// marks.
    pub marked: i16,
    /// The three lines in the keeper's box.
    pub said: Said,
    /// The place whose creature is being named: the task sleeps (bit 0 of its flag word
    /// is clear) until the name entry is over ([`Self::named`]).
    pub naming: Option<usize>,
    /// The row "Magic FX" of the options (bit 5 of the setting word), on in a new
    /// game: what follows the yes of a spin goes by it.
    pub magic_fx: bool,
    /// The task sleeps while the room plays what the script does with the answer.
    pub away: bool,
    /// The white over the screen while it grows or goes (the half word +148 is not 0),
    /// and the fade record that draws it, which its own task steps behind the screen's
    /// in every game frame.
    whitens: Option<White>,
    yes: Yes,
    fade: Fade,
    /// The creature's form that is looked at: the task is in the two routines of the
    /// look and its step is not passed.
    pub form: Option<Form>,
    /// Everything the player has, while the screen is up.
    pub listed: Listed,
}

/// The second cursor on a page without rows (`0x800BC2BC`, `0x800BC3AC`).
const ONE_PLACE: Cursor = Cursor {
    columns: 1,
    rows: 1,
    lines: 1,
    flags: 0,
    at: 0,
    first: 0,
};

fn shaped(cursor: &mut Cursor, [columns, rows, lines, flags]: [i16; 4]) {
    cursor.columns = columns;
    cursor.rows = rows;
    cursor.lines = lines as u16;
    cursor.flags = flags as u16;
}

/// Whether a state has the two lists (`0x800BD854`).
pub fn has_lists(state: i16) -> bool {
    use state::*;
    matches!(
        state,
        MERGE_FIRST | MERGE_SECOND | SPIN | TAKE | VIEW | EXCHANGE
    )
}

impl MergeScreen {
    /// The screen as its set-up leaves it (`0x800BB318`): the player, the caught
    /// creatures and the carried ones are made whole, the list is opened, which
    /// empties the game's own records, the cocoons are filled again, and the main
    /// menu is up.
    pub fn open(argument: i16, game: &mut Carried) -> Self {
        game.make_whole();
        game.caught.each_mut().for_each(Record::make_whole);
        game.creatures.carried_mut().for_each(Record::make_whole);
        let listed = Listed::open(&mut game.creatures, &mut game.caught);
        // The set-up's last calls (`0x800BB534`, `0x800BB53C`): what is left of twelve
        // after the empty cocoons and the records in use is given as empty cocoons.
        // The list has just emptied the records, so the player has twelve again.
        let in_use = game.creatures.count(0) as i32;
        game.keeping.give_cocoons(game.keeping.cocoons_left(in_use));
        let mut cursor = Cursor {
            columns: 0,
            rows: 0,
            lines: 0,
            flags: 0,
            at: 0,
            first: 0,
        };
        shaped(&mut cursor, CURSORS[state::MAIN as usize]);
        let mut which = cursor;
        shaped(&mut which, OF_THREE);
        Self {
            which,
            made: None,
            things: game.keeping.clone(),
            argument,
            state: state::MAIN,
            back: -2,
            kept: 0,
            carried: 0,
            clock: 0,
            confirm: false,
            cancel: false,
            refusal: 0,
            refusal_passes: 0,
            naming: None,
            magic_fx: true,
            away: false,
            whitens: None,
            yes: Yes::Spin,
            fade: Fade::default(),
            marked: 0,
            panels: PANELS[state::MAIN as usize],
            opened: 0,
            flags: flag::COUNT,
            cursor,
            list: 0,
            page_cursor: ONE_PLACE,
            saved: (0, 0, 0),
            cursor_slots: [0; 10],
            scroll_slots: [0; 4],
            list_slots: [0; 2],
            first: 0,
            second: 0,
            marks: 0,
            carried_marks: 0,
            kept_marks: 0,
            said: Said::Row(0),
            form: None,
            listed,
        }
    }

    /// The leaving (`0x800B9F78`, asked by file 43's step 3): the list goes back into
    /// the game's records. The keeper lets the screen leave only with twelve kept
    /// creatures or fewer, so the list always fits.
    pub fn close(self, game: &mut Carried) -> Result<(), Box<Listed>> {
        self.listed.close(&mut game.creatures)
    }

    /// The openness of a panel (`0x800BC550`): whole once it has been up for eight
    /// passes, the clock before.
    pub fn openness(&self, panel: u32) -> i16 {
        if self.opened & panel != 0 {
            OPEN
        } else {
            self.clock
        }
    }

    /// The scroll of the list of the kept (`0x800BED3C`).
    pub fn scroll(&self) -> i16 {
        use state::*;
        match self.state {
            MERGE_FIRST | MERGE_SECOND | SPIN | TAKE | VIEW | EXCHANGE => self.cursor.first,
            MERGE_ASKED | MERGE_MORE | SKINS_ASKED | SKINS => self.scroll_slots[1],
            SPIN_ASKED | RENAME_ASKED | ABILITIES => self.scroll_slots[0],
            _ => -1,
        }
    }

    /// The row under the cursor in the list of the carried and in the list of the kept
    /// (`0x800BE718`, `0x800BE9CC`), `None` for a list the cursor is not in.
    pub fn cursor_rows(&self) -> (Option<i16>, Option<i16>) {
        if !has_lists(self.state) {
            return (None, None);
        }
        match self.flags & flag::IN_CARRIED != 0 {
            true => (Some(self.cursor.at), None),
            false => (None, Some(self.cursor.at)),
        }
    }

    /// The place whose creature the box beside the lists shows (`0x800BEEA4`): the
    /// place under the cursor; in state 3 the one of the three creatures of the merge
    /// that its cursor is on.
    pub fn shown_place(&self) -> usize {
        match self.state {
            state::MERGE_SECOND | state::EXCHANGE => self.second,
            state::MERGE_ASKED => match self.which.at {
                1 => self.second,
                ON_THE_NEW => MADE,
                _ => self.first,
            },
            _ => self.first,
        }
    }

    /// The creature of a place; [`MADE`] is the one that lies aside.
    pub fn creature(&self, place: usize) -> Option<&Creature> {
        match place {
            MADE => self.made.as_ref(),
            place => self.listed.get(place),
        }
    }

    /// The creature the box of state 3 compares the new one with (`0x800BC9E0`): the
    /// first of the two, while the cursor of the three is on the new one.
    pub fn compared(&self) -> Option<&Creature> {
        (self.state == state::MERGE_ASKED && self.which.at == ON_THE_NEW)
            .then(|| self.listed.get(self.first))?
    }

    /// The row of the list of the minion skins under its cursor, and its first line
    /// (the slots of state 14, +110 and +118).
    pub fn skin_cursor(&self) -> (i16, i16) {
        (self.cursor_slots[8], self.scroll_slots[2])
    }

    /// The thing of a line of the list of the minion skins (`0x800AD458(line, 4)`).
    fn skin_at(&self, line: i32) -> u8 {
        usize::try_from(line).map_or(items::FREE, |line| {
            self.things.number_at(line, items::Kind::Creature)
        })
    }

    /// The merge of the two picked places (`0x800BD8FC`): with the skin of the list's
    /// cursor when one was taken there.
    fn merging(&self) -> Merging {
        let (row, first) = self.skin_cursor();
        let skin =
            (self.flags & flag::SKIN != 0).then(|| self.skin_at(i32::from(row) + i32::from(first)));
        Merging {
            first: self.first,
            second: self.second,
            skin,
        }
    }

    /// The caller's half of a merge (`0x800BA198`): the new creature lies aside. A
    /// thing that is no skin leaves the look as the merge makes it.
    pub fn merge(
        &mut self,
        merging: Merging,
        tables: &crate::battle::Tables,
        draws: &mut impl crate::battle::record::Draws,
    ) {
        let (Some(first), Some(second)) = (
            self.listed.get(merging.first),
            self.listed.get(merging.second),
        ) else {
            return;
        };
        let look = (merging.skin).and_then(|thing| crate::merge::skin_look(thing.into()));
        if let Some(made) = crate::merge::merged(first, second, look, tables, draws) {
            self.made = Some(made);
        }
    }

    /// The state's cursor, scroll and list go into its slots (`0x800BB968`).
    fn keep_cursor(&mut self) {
        let state = self.state as usize;
        if let Ok(slot) = usize::try_from(CURSOR_SLOT[state]) {
            self.cursor_slots[slot] = self.cursor.at;
        }
        if let Ok(slot) = usize::try_from(SCROLL_SLOT[state]) {
            self.scroll_slots[slot] = self.cursor.first;
        }
        if let Ok(slot) = usize::try_from(LIST_SLOT[state]) {
            self.list_slots[slot] = self.list;
        }
    }

    /// And come back from them (`0x800BB9F8`).
    fn restore_cursor(&mut self) {
        let state = self.state as usize;
        if let Ok(slot) = usize::try_from(CURSOR_SLOT[state]) {
            self.cursor.at = self.cursor_slots[slot];
        }
        if let Ok(slot) = usize::try_from(SCROLL_SLOT[state]) {
            self.cursor.first = self.scroll_slots[slot];
        }
        if let Ok(slot) = usize::try_from(LIST_SLOT[state]) {
            self.list = self.list_slots[slot];
        }
    }

    /// The place under the cursor of a state of the two lists (`0x800BEDA4`): the row
    /// of the carried, or 3 and the scroll more for a row of the kept. The states of
    /// a first pick note it as the first place, the states behind them as the second.
    fn places(&mut self) {
        use state::*;
        let slot = match self.state {
            MERGE_FIRST | SPIN | TAKE | VIEW | ABILITIES => 0,
            MERGE_SECOND | EXCHANGE => 1,
            _ => return,
        };
        let in_carried = self.flags & flag::IN_CARRIED != 0;
        let row = i32::from(self.cursor_slots[slot + 1]);
        let place = match in_carried {
            true => row,
            false => row + i32::from(self.scroll()) + MOST_CARRIED as i32,
        };
        let place = usize::try_from(place).unwrap_or(usize::MAX);
        match slot {
            0 => self.first = place,
            _ => self.second = place,
        }
        self.list_slots[slot] = i16::from(!in_carried);
    }

    /// The page of the creature's box that is up, 0 for its "Detail".
    pub fn page(&self) -> u32 {
        self.flags & flag::PAGES
    }

    /// The page the drawing of a pass finds: the step clears the pages in front of
    /// the drawing of every state but the pages' own, so a cancel's pass shows none;
    /// it leaves them while the square's flag is set, which state 3 keeps for as long
    /// as its pages are up.
    pub fn drawn_page(&self) -> u32 {
        match self.state == state::ABILITIES || self.flags & flag::ABILITIES != 0 {
            true => self.page(),
            false => 0,
        }
    }

    /// The second cursor in the shape of the page that is up (`0x800BB794`), on its
    /// first row: the cursor of the three has moved to another creature.
    fn shape_page_cursor(&mut self) {
        self.page_cursor = ONE_PLACE;
        match self.page() {
            flag::ATTACKS => {
                self.page_cursor.rows = attacks::ROWS as i16;
                self.page_cursor.lines = attacks::ROWS as u16;
            }
            flag::SPELLS => {
                let known = (self.creature(self.shown_place()))
                    .map_or(0, |creature| spells_known(creature.record.spells));
                self.page_cursor.rows = spells::ROWS as i16;
                self.page_cursor.lines = known.max(spells::ROWS as i16) as u16;
                self.page_cursor.flags = cursor::SCROLLS;
            }
            _ => {}
        }
    }

    /// The square on a page (`0x800BC378`): the next of the four pages, with the
    /// second cursor in the shape of its rows. Nothing for a free place.
    fn turn_page(&mut self, passed: &mut Passed) {
        let Some(known) = (self.creature(self.shown_place()))
            .map(|creature| spells_known(creature.record.spells))
        else {
            return;
        };
        let page = self.page();
        self.flags &= !flag::PAGES;
        self.page_cursor = ONE_PLACE;
        match page {
            0 => {
                self.flags |= flag::ATTACKS;
                self.page_cursor.rows = attacks::ROWS as i16;
                self.page_cursor.lines = attacks::ROWS as u16;
            }
            flag::ATTACKS => {
                self.flags |= flag::SPELLS;
                self.page_cursor.rows = spells::ROWS as i16;
                self.page_cursor.lines = known.max(spells::ROWS as i16) as u16;
                self.page_cursor.flags = cursor::SCROLLS;
            }
            flag::SPELLS => self.flags |= flag::FAMILY,
            // Behind the "Family" the "Detail" is back; in state 3, where the pages
            // are up only with the square's flag, that flag goes with it.
            _ => {
                if self.state == state::MERGE_ASKED && self.flags & flag::MORE != 0 {
                    self.flags &= !flag::ABILITIES;
                }
            }
        }
        passed.sounds.push(SOUND_CURSOR);
    }

    /// The refusal in front of a confirm (`0x800BD150`), 0 for none, for the states
    /// that are in code: the merge needs two creatures, the other rows of the main
    /// menu but the last need one, and the view a creature in the place.
    fn refused(&mut self) -> i16 {
        let free = |place: usize| self.listed.state(place) == creatures::state::FREE;
        match self.state {
            state::VIEW | state::MERGE_FIRST => {
                return if free(self.first) { NONE } else { 0 };
            }
            // The second of a merge: a creature, and not the first one.
            state::MERGE_SECOND => {
                return match (free(self.second), self.second == self.first) {
                    (true, _) => NONE,
                    (false, true) => SAME,
                    (false, false) => 0,
                };
            }
            state::MERGE_MORE => {
                return if self.kept + self.carried < 2 {
                    TOO_FEW
                } else {
                    0
                };
            }
            // A line of the list of the skins that holds one: the merge takes it.
            state::SKINS => {
                let (row, first) = self.skin_cursor();
                if self.skin_at(i32::from(row) + i32::from(first)) == items::FREE {
                    return NO_SKIN;
                }
                self.flags |= flag::SKIN;
                return 0;
            }
            _ => {}
        }
        if self.state == state::SPIN {
            return match self.marks == 0 && self.carried_marks == 0 {
                true => NONE,
                false => 0,
            };
        }
        if self.state != state::MAIN {
            return 0;
        }
        let has = self.kept + self.carried;
        match self.cursor.at {
            0 if has < 2 => TOO_FEW,
            1..=3 if has == 0 => NONE,
            _ => 0,
        }
    }

    /// A confirm on a list notes where the cursor was (`0x800BD2B0`).
    fn note_cursor(&mut self) {
        if self.list == 0 {
            self.flags |= flag::IN_CARRIED;
            self.saved.0 = self.cursor.at;
        } else {
            self.flags &= !flag::IN_CARRIED;
            (self.saved.1, self.saved.2) = (self.cursor.at, self.cursor.first);
        }
    }

    /// And marks the place under it.
    fn mark(&mut self) {
        let (at, first) = (self.cursor.at, self.cursor.first);
        if self.list == 0 {
            let bit = 1u32.wrapping_shl(at as u32);
            self.marks |= bit;
            self.carried_marks |= bit;
        } else {
            let row = (i32::from(at) + i32::from(first)) as u32;
            self.marks |= 1u32.wrapping_shl(row + MOST_CARRIED as u32);
            self.kept_marks |= 1u32.wrapping_shl(row);
        }
    }

    /// A confirm in state 5 (`0x800BC168`): the place under the cursor changes its
    /// mark and the count follows; a free place does nothing and makes no sound.
    fn toggle(&mut self, passed: &mut Passed) {
        if self.listed.state(self.first) == creatures::state::FREE {
            return;
        }
        let row = (i32::from(self.cursor.at) + i32::from(self.cursor.first)) as u32;
        let (rows, place) = match self.flags & flag::IN_CARRIED != 0 {
            true => (&mut self.carried_marks, row),
            false => (&mut self.kept_marks, row + MOST_CARRIED as u32),
        };
        let (row, place) = (1u32.wrapping_shl(row), 1u32.wrapping_shl(place));
        if *rows & row == 0 {
            *rows |= row;
            self.marks |= place;
            self.marked = self.marked.wrapping_add(1);
        } else {
            *rows &= !row;
            self.marks &= !place;
            self.marked = self.marked.wrapping_sub(1);
        }
        passed.sounds.push(SOUND_CONFIRM);
    }

    /// The cursor record of the state (`0x800BDA30`).
    fn set_cursor(&mut self) {
        shaped(&mut self.cursor, CURSORS[self.state as usize]);
        if self.panels & ANSWER_PANEL != 0 {
            self.cursor.at = i16::from(self.state != state::MERGE_MORE);
        }
        if self.state == state::MERGE_ASKED {
            self.which.at = ON_THE_NEW;
        }
        // The list of the skins has a line for every place of his things of kind 4 up
        // to the last in use, three at least, and one more.
        if self.state == state::SKINS {
            let last = self.things.last_place(items::Kind::Creature);
            self.cursor.lines = (last.max(LEAST_SKIN_LINES) + 1) as u16;
        }
        if self.flags & flag::LISTS == 0 {
            return;
        }
        let behind = matches!(self.state, state::MERGE_SECOND | state::EXCHANGE);
        if self.list == 0 {
            self.flags |= flag::IN_CARRIED;
            shaped(&mut self.cursor, CARRIED_CURSOR);
            if behind {
                self.cursor.at = self.saved.0;
            }
        } else {
            self.flags &= !flag::IN_CARRIED;
            shaped(&mut self.cursor, KEPT_CURSOR);
            if behind {
                (self.cursor.at, self.cursor.first) = (self.saved.1, self.saved.2);
            }
        }
    }

    /// The lines of the state in the keeper's box (`0x800BBD24` with 0).
    fn say(&mut self) {
        self.said = match self.state {
            state::MAIN => Said::Row(self.cursor.at),
            state => Said::State(state),
        };
    }

    /// The way back of a cancel (`0x800BBA94`).
    fn go_back(&mut self, passed: &mut Passed) {
        match self.state {
            state::MAIN => self.cursor.at = STOP,
            state::FULL => self.state = state::MAIN,
            state::DONE => self.flags |= flag::ENDS,
            // From the new creature back to the second pick, the cursor where that
            // pick left it, and no skin taken.
            state::MERGE_ASKED => {
                (self.state, self.back) = (state::MERGE_SECOND, state::MERGE_FIRST);
                self.flags &= !flag::SKIN;
                (self.cursor.at, self.cursor.first) = (self.cursor_slots[2], self.scroll_slots[1]);
            }
            state::MERGE_MORE => {
                self.state = state::MAIN;
                (self.cursor.at, self.cursor.first) = (0, 0);
            }
            // The "No" to the skins: the two are merged without one.
            state::SKINS_ASKED => {
                passed.merges = Some(self.merging());
                (self.state, self.back) = (state::MERGE_ASKED, state::MERGE_SECOND);
            }
            state::SKINS => (self.state, self.back) = (state::SKINS_ASKED, state::MERGE_SECOND),
            from => {
                if matches!(from, state::MERGE_SECOND | state::EXCHANGE) {
                    self.list = self.list_slots[0];
                }
                self.state = self.back;
                let own = usize::try_from(self.state).ok();
                self.back = own.and_then(|own| CANCEL.get(own)).copied().unwrap_or(-2);
                self.restore_cursor();
            }
        }
    }

    /// The worker of a confirm (`0x800BDBB8`), for the states that are in code.
    fn worker(&mut self, passed: &mut Passed) {
        match self.state {
            state::EXCHANGE => {
                if self.listed.exchange(self.first, self.second) {
                    passed.exchanged = Some((self.first, self.second));
                }
                (self.marks, self.carried_marks, self.kept_marks) = (0, 0, 0);
                (self.state, self.back) = (state::TAKE, state::MAIN);
                self.flags |= flag::COUNT;
                self.say();
            }
            state::DONE => self.flags |= flag::ENDS,
            state::FULL => self.state = state::MAIN,
            // The yes to a spin (`0x800BDD28`): every marked place gives a silk and is
            // freed. What follows goes by the row "Magic FX" of the options: with it
            // on the task answers the script and sleeps while the room plays its part
            // (`0x800BDF50`), with it off a white grows over the screen
            // (`0x800BE020`); state 5 comes back behind either (`0x800BE258`).
            state::SPIN_ASKED => {
                for place in 0..creatures::PLACES {
                    if self.marks & 1u32.wrapping_shl(place as u32) == 0 {
                        continue;
                    }
                    let level = (self.listed.get(place)).map_or(0, |c| c.record.level);
                    passed.spun.push(silk(level));
                    self.listed.free(place);
                }
                if passed.spun.is_empty() {
                    return;
                }
                (self.marks, self.carried_marks, self.kept_marks) = (0, 0, 0);
                self.flags |= flag::COUNT;
                self.follow(Yes::Spin, passed);
            }
            // The yes to a merge (`0x800BDC10`): with "Magic FX" on the two are merged
            // at once and the room plays its part; with it off the white grows first
            // and the merge is done where the white has gone (`0x800BE084`).
            state::MERGE_ASKED => {
                if self.magic_fx {
                    self.take_merged(passed);
                }
                self.follow(Yes::Merge, passed);
            }
            // The yes to a new name (`0x800BDEC8`): the task sleeps while file 43 has
            // the name entry up on the name of the creature of the place.
            state::RENAME_ASKED => {
                self.naming = Some(self.first);
                passed.names = Some(self.first);
                (self.marks, self.carried_marks, self.kept_marks) = (0, 0, 0);
            }
            _ => {}
        }
    }

    /// What follows a yes, by the row "Magic FX" of the options (`0x800BDDE0`,
    /// `0x800BDE1C`): on, the task answers the script and sleeps while the room plays
    /// its part; off, a white grows over the screen.
    fn follow(&mut self, yes: Yes, passed: &mut Passed) {
        if self.magic_fx {
            self.away = true;
            passed.answers = Some(match yes {
                Yes::Merge => ANSWER_OF_A_MERGE,
                Yes::Spin => ANSWER_OF_A_SPIN,
            });
        } else {
            // HYPOTHESIS: `0x8001ABEC` puts the fade record to rest.
            self.yes = yes;
            self.flags |= flag::WHITE;
            self.fade = Fade::default();
            self.fade.whiten(WHITE);
            self.whitens = Some(White::Grows);
        }
    }

    /// The merge is taken (`0x800BDC3C`, and the same at `0x800BE12C`): the creature
    /// that lies aside is copied over the first place, the second place is freed, a
    /// new species is noted by the first byte of the line (+76 of the record), the
    /// skin of the list's cursor is the one to take from the player when bit 9 of the
    /// flags says one was picked, the marks go and the counts are asked for anew.
    fn take_merged(&mut self, passed: &mut Passed) {
        let mut new_species = None;
        if let Some(made) = self.made.clone() {
            new_species = Some(made.line.species[0]).filter(|kind| NEW_SPECIES.contains(kind));
            self.listed.put(self.first, made);
        }
        self.listed.free(self.second);
        passed.merged = Some(Merged {
            first: self.first,
            second: self.second,
            skin: self.merging().skin,
            new_species,
        });
        self.flags &= !flag::SKIN;
        (self.marks, self.carried_marks, self.kept_marks) = (0, 0, 0);
        self.flags |= flag::COUNT;
    }

    /// The way back from the name entry (`0x800BE304`): the creature has the name.
    /// The question of a new name becomes the view again, with its panels, its lines
    /// and its two lists, and the cursor back on the row and the scroll of the view's
    /// slots; behind a merge "once more?" is up, the cursor on the yes. Nothing sounds.
    pub fn named(&mut self, name: [u8; crate::name::LETTERS]) {
        let Some(place) = self.naming.take() else {
            return;
        };
        if let Some(record) = self.listed.record_mut(place) {
            record.name = name;
        }
        self.wake(match self.state {
            state::RENAME_ASKED => state::VIEW,
            _ => state::MERGE_MORE,
        });
    }

    /// The way back from the room's part, when the script asks for the screen again
    /// (`0x59` of kind 3, which file 43's watch takes as the step that was asked for).
    /// Behind a spin that is step 2 and `0x800BE258`: state 5 with the spun places
    /// gone. Behind a merge it is step 1, the name entry on the new creature's name:
    /// the place whose creature is to be named is answered, and [`Self::named`] wakes
    /// the task. Nothing sounds.
    pub fn back(&mut self) -> Option<usize> {
        if !std::mem::take(&mut self.away) {
            return None;
        }
        if self.state == state::MERGE_ASKED {
            self.naming = Some(self.first);
            return self.naming;
        }
        self.wake(state::SPIN);
        None
    }

    pub fn white(&self) -> u8 {
        match self.whitens {
            Some(_) => self.fade.white,
            None => 0,
        }
    }

    /// A pass under the white (`0x800BE020`, `0x800BE084`): the screen is drawn and no
    /// pad is read. The white that has grown is let go, and when that is at rest too
    /// the task sleeps and file 43 is asked for a step: behind a spin its step 2, and
    /// state 5 comes back; behind a merge, which is taken only here when "Magic FX"
    /// is off, its step 1, the name entry on the new creature's name. The hold of the
    /// screen round that step takes no time here.
    fn whiten(&mut self, white: White, passed: &mut Passed) {
        self.fade.white = 0;
        if self.fade.at_rest() {
            match white {
                White::Grows => {
                    self.fade.unwhiten(WHITE);
                    self.whitens = Some(White::Goes);
                }
                White::Goes => {
                    self.whitens = None;
                    self.flags &= !flag::WHITE;
                    self.fade = Fade::default();
                    match self.yes {
                        Yes::Spin => self.wake(state::SPIN),
                        Yes::Merge => {
                            if !self.magic_fx {
                                self.take_merged(passed);
                            }
                            self.naming = Some(self.first);
                            passed.names = self.naming;
                        }
                    }
                    return;
                }
            }
        }
        self.fade.step();
    }

    /// A state is up again when the task is woken (`0x800BE304`, `0x800BE258`): its
    /// panels, its lines, and the cursor on the row and the scroll of the slots of
    /// the two lists (+96 and +114), with the main menu to go back to. "Once more?"
    /// has its cursor on the yes.
    fn wake(&mut self, state: i16) {
        (self.state, self.back) = (state, state::MAIN);
        self.panels = PANELS[self.state as usize];
        self.opened &= self.panels;
        self.say();
        match has_lists(state) {
            true => self.flags |= flag::LISTS,
            false => self.flags &= !flag::LISTS,
        }
        self.set_cursor();
        if state == state::MERGE_MORE {
            (self.cursor.at, self.cursor.first) = (0, self.scroll_slots[0]);
        } else {
            self.restore_cursor();
        }
    }

    /// A confirm or a cancel is worked (`0x800BD2B0`).
    fn work(&mut self, passed: &mut Passed) {
        let mut changed = false;
        let mut refusal = 0;
        if self.confirm {
            refusal = self.refused();
            let lists = self.flags & flag::LISTS != 0;
            if lists {
                self.note_cursor();
            }
            if refusal != 0 {
                passed.sounds.push(SOUND_REFUSED);
                self.refusal = refusal;
                self.said = Said::Refusal(refusal);
            } else {
                if lists && self.state != state::SPIN {
                    self.mark();
                }
                changed = self.confirmed(passed);
            }
        } else if self.cancel && !(self.state == state::MAIN && self.cursor.at == STOP) {
            passed.sounds.push(SOUND_CANCEL);
            // The marks go, but for the two states of the skins; behind the new
            // creature the first pick has its mark again (`0x800BD6E0`).
            if !matches!(self.state, state::SKINS_ASKED | state::SKINS) {
                (self.marks, self.carried_marks, self.kept_marks) = (0, 0, 0);
            }
            if self.state == state::MERGE_ASKED {
                let row = self.cursor_slots[1];
                if self.list_slots[0] == 0 {
                    let bit = 1u32.wrapping_shl(row as u32);
                    self.marks |= bit;
                    self.carried_marks |= bit;
                } else if self.list_slots[0] == 1 {
                    let row = i32::from(row.wrapping_add(self.scroll_slots[0])) as u32;
                    self.marks |= 1u32.wrapping_shl(row.wrapping_add(MOST_CARRIED as u32));
                    self.kept_marks |= 1u32.wrapping_shl(row);
                }
            }
            self.go_back(passed);
            changed = true;
        }
        (self.confirm, self.cancel) = (false, false);
        if self.flags & flag::ENDS != 0 {
            return;
        }
        if refusal == 0 && changed {
            // The new creature and the question of the skins start with the keeper's
            // second lines, which wait for a button (`0x800BD82C`).
            if matches!(self.state, state::MERGE_ASKED | state::SKINS_ASKED) {
                self.flags |= flag::MORE;
                self.said = Said::Second(self.state);
            } else {
                self.say();
            }
        }
        // The panels of the state; one that was open and is still asked for stays so.
        self.panels = PANELS[self.state as usize];
        self.opened &= self.panels;
        match has_lists(self.state) {
            true => self.flags |= flag::LISTS,
            false => self.flags &= !flag::LISTS,
        }
        self.set_cursor();
    }

    /// Where a confirm leads; whether the state's lines are to be said anew.
    fn confirmed(&mut self, passed: &mut Passed) -> bool {
        let (mut back, mut merges) = (self.state, false);
        let next = match CONFIRM[self.state as usize] {
            -3 => {
                passed.sounds.push(SOUND_CONFIRM);
                self.worker(passed);
                return false;
            }
            // The second pick (`0x800BD51C`): with a minion skin among his things the
            // question of the skins, else the two are merged and the new one is shown.
            -1 if self.state == state::MERGE_SECOND => match self.things.has_creatures() {
                true => state::SKINS_ASKED,
                false => {
                    merges = true;
                    state::MERGE_ASKED
                }
            },
            // The yes to the skins leads to their list, and a skin picked there to the
            // merge; either way a cancel of what follows leads back to the picks.
            -1 if self.state == state::SKINS_ASKED => {
                back = state::MERGE_SECOND;
                state::SKINS
            }
            -1 if self.state == state::SKINS => {
                (back, merges) = (state::MERGE_FIRST, true);
                state::MERGE_ASKED
            }
            // "Once more?": the yes starts with the first pick again.
            -1 if self.state == state::MERGE_MORE => {
                back = state::MAIN;
                state::MERGE_FIRST
            }
            -1 if self.state == state::MAIN => match self.cursor.at {
                STOP if self.kept <= MOST_KEPT => state::DONE,
                STOP => state::FULL,
                row => ROWS[row.clamp(0, 3) as usize],
            },
            // The confirm the square noted leads to the pages, any other to the
            // question of a new name (`0x800BD59C`).
            -1 if self.state == state::VIEW => match self.flags & flag::ABILITIES != 0 {
                true => state::ABILITIES,
                false => state::RENAME_ASKED,
            },
            next if next >= 0 => next,
            _ => return false,
        };
        // The first pick of a merge is copied aside (`0x800BD3F0`).
        if self.state == state::MERGE_FIRST {
            self.made = self.listed.get(self.first).cloned();
        }
        if merges {
            passed.merges = Some(self.merging());
        }
        self.flags &= !flag::ABILITIES;
        passed.sounds.push(SOUND_CONFIRM);
        (self.back, self.state) = (back, next);
        (self.cursor.at, self.cursor.first) = (0, 0);
        self.clock = 0;
        true
    }

    /// The pad of a pass (`0x800BBFF4`): the cursor, the change between the two
    /// lists, the second cursor on a page, the confirm and the cancel, which the
    /// next pass works, and the square: "Rearrange", or "View other abilities" in the
    /// view, where it notes a confirm that leads to the pages, and on a page, where it
    /// turns to the next.
    fn input(&mut self, pad: &Pad, passed: &mut Passed) {
        if self.flags & flag::LISTS == 0 {
            if self.cursor.step(pad.repeated).sounds {
                passed.sounds.push(SOUND_CURSOR);
            }
        } else {
            self.places();
            let moved = self.cursor.step(pad.repeated);
            if moved.sounds {
                passed.sounds.push(SOUND_CURSOR);
            }
            match moved.answer {
                // Down from the last of the carried: the first row of the kept.
                4 => {
                    shaped(&mut self.cursor, KEPT_CURSOR);
                    self.cursor.at = 0;
                    self.list = 1;
                    self.flags &= !flag::IN_CARRIED;
                    passed.sounds.push(SOUND_CURSOR);
                }
                // Up from the first of the kept: the last row of the carried.
                3 => {
                    shaped(&mut self.cursor, CARRIED_CURSOR);
                    self.cursor.at = MOST_CARRIED as i16 - 1;
                    self.list = 0;
                    self.flags |= flag::IN_CARRIED;
                    self.keep_cursor();
                    passed.sounds.push(SOUND_CURSOR);
                }
                _ => {}
            }
        }
        if self.state == state::ABILITIES && self.page_cursor.step(pad.repeated).sounds {
            passed.sounds.push(SOUND_CURSOR);
        }
        if menu::confirmed(pad) && self.state == state::SPIN {
            self.toggle(passed);
        } else if menu::confirmed(pad) {
            // A confirm on the "No" of a question counts as a cancel (`0x800BD8A0`),
            // and a confirm on the place picked first takes the pick back.
            let asked = {
                use state::*;
                matches!(
                    self.state,
                    MERGE_ASKED | MERGE_MORE | SPIN_ASKED | RENAME_ASKED | SKINS_ASKED
                )
            };
            let no = asked && self.cursor.at == 1;
            let behind = matches!(self.state, state::MERGE_SECOND | state::EXCHANGE);
            match no || (behind && self.first == self.second) {
                true => self.cancel = true,
                false => self.confirm = true,
            }
        } else if menu::cancelled(pad) {
            self.cancel = true;
        } else if pad.pressed & pad::SQUARE != 0 {
            match self.state {
                state::VIEW => {
                    self.page_cursor = ONE_PLACE;
                    self.confirm = true;
                    self.flags |= flag::ABILITIES;
                }
                state::ABILITIES => self.turn_page(passed),
                state::MERGE_FIRST | state::TAKE if self.listed.close_gaps() => {
                    passed.sounds.push(SOUND_CONFIRM);
                    passed.rearranged = true;
                }
                // In state 5 only while nothing is marked (`0x800BB5B8`).
                state::SPIN if self.marks == 0 && self.listed.close_gaps() => {
                    passed.sounds.push(SOUND_CONFIRM);
                    passed.rearranged = true;
                }
                _ => {}
            }
        } else if pad.pressed & pad::CIRCLE != 0 && self.marked > 0 && self.state == state::SPIN {
            // "Spin": the circle notes a confirm while the count says a place is marked.
            self.confirm = true;
        }
    }

    /// The pad of a pass while the keeper's second lines are up (`0x800BB76C`,
    /// `0x800BC4AC`). Beside the new creature left and right go through the three
    /// creatures of the merge, whose box the square turns to its other pages; a
    /// confirm or a cancel closes those pages, and with none up it leads on to the
    /// keeper's question. The circle, which wins over the square, is the step's
    /// ([`Self::ask_form`]).
    fn more(&mut self, pad: &Pad, passed: &mut Passed) {
        let shows = self.state == state::MERGE_ASKED;
        if shows {
            let on = self.which.at;
            if self.which.step(pad.repeated).sounds {
                passed.sounds.push(SOUND_CURSOR);
            }
            if self.which.at != on {
                self.shape_page_cursor();
            }
            if self.flags & flag::ABILITIES != 0 && self.page_cursor.step(pad.repeated).sounds {
                passed.sounds.push(SOUND_CURSOR);
            }
        }
        if menu::confirmed(pad) || menu::cancelled(pad) {
            if shows && self.flags & flag::ABILITIES != 0 {
                passed.sounds.push(SOUND_CANCEL);
                self.flags &= !flag::ABILITIES;
            } else {
                passed.sounds.push(SOUND_CONFIRM);
                self.say();
                self.flags &= !flag::MORE;
            }
        }
        if shows && pad.pressed & pad::CIRCLE == 0 && pad.pressed & pad::SQUARE != 0 {
            self.flags |= flag::ABILITIES;
            self.turn_page(passed);
        }
    }

    /// Whether the circle is "View form" in the state (`0x800BB55C`, the jump table
    /// `0x800B9774`): in the states of the two lists, in state 5 only with nothing
    /// marked. The step asks it in every state but 8, which has the circle anyway, and
    /// 3 with the keeper's second lines up, which has it too.
    fn forms(&self) -> bool {
        use state::*;
        match self.state {
            MERGE_FIRST | MERGE_SECOND | TAKE | VIEW | EXCHANGE => true,
            SPIN => self.marks == 0,
            _ => false,
        }
    }

    /// The circle of a pass, behind the pad routine (`0x800BB8B0` to `0x800BB90C`):
    /// "View form". Nothing while the place picked first is free (the test is on that
    /// record, +76, also where the box shows another, +84: behind a first pick the
    /// circle on a free place starts the look, of a record that is not in use, which
    /// the caller has no model for). The pages go, the task leaves its step
    /// (`0x800BE490`): the sound of a confirm, and the model job of the shown place.
    fn ask_form(&mut self, pad: &Pad, more: bool, passed: &mut Passed) {
        if pad.pressed & pad::CIRCLE == 0 {
            return;
        }
        let asked = match (more, self.state) {
            (true, state::MERGE_ASKED) | (false, state::VIEW) => true,
            _ => self.forms(),
        };
        if !asked || self.listed.get(self.first).is_none() {
            return;
        }
        self.flags = (self.flags | flag::FORM) & !(flag::ABILITIES | flag::PAGES);
        passed.sounds.push(SOUND_CONFIRM);
        let place = self.shown_place();
        self.form = Some(Form {
            place,
            made: false,
            turn: 0,
            scale: None,
        });
        passed.looks = Some(place);
    }

    /// The actor of the form is made with this scale, the stage's own: the caller's
    /// word, who reads the model.
    pub fn form_stands(&mut self, scale: [i32; 3]) {
        if let Some(form) = self.form.as_mut() {
            form.scale = Some(scale);
        }
    }

    /// A pass of the task while a form is looked at. First the wait for the model job
    /// (`0x800BE6AC`; STAND-IN: the job is through in the first pass that asks, the
    /// read of a model takes no time here): the keeper's three fixed lines and the
    /// actor. Then (`0x800BE510`) the held buttons turn and scale the actor, and a
    /// cancel ends the look: the lines of the state are back, the second ones in
    /// state 3, the confirm that the circle noted in state 5 is forgotten, and the
    /// step is the task's routine again. Both routines draw the screen, so a panel
    /// that was opening stays as far open as it was: the clock is the step's.
    fn look(&mut self, pad: &Pad, passed: &mut Passed) {
        if self.clock == OPEN {
            self.opened |= self.panels;
        }
        let Some(form) = self.form.as_mut() else {
            return;
        };
        if !form.made {
            form.made = true;
            self.said = Said::Form;
            return;
        }
        form.turn = Form::turned(form.turn, pad.held);
        form.scale = form.scale.map(|scale| Form::scaled(scale, pad.held));
        if !menu::cancelled(pad) {
            return;
        }
        passed.sounds.push(SOUND_CANCEL);
        passed.looked = true;
        self.form = None;
        match self.state {
            state::MERGE_ASKED => self.said = Said::Second(state::MERGE_ASKED),
            _ => self.say(),
        }
        if self.state == state::SPIN {
            self.confirm = false;
        }
        self.flags &= !flag::FORM;
    }

    /// One pass of the task (`0x800BB614`, or one of the two routines of the look at
    /// a form).
    pub fn pass(&mut self, pad: &Pad) -> Passed {
        let mut passed = Passed::default();
        // Asleep, the task is not passed.
        if self.naming.is_some() || self.away {
            return passed;
        }
        if let Some(white) = self.whitens {
            self.whiten(white, &mut passed);
            return passed;
        }
        if self.form.is_some() {
            self.look(pad, &mut passed);
            return passed;
        }
        if self.flags & flag::COUNT != 0 {
            self.kept = self.listed.count(creatures::state::KEPT) as i16;
            self.carried = self.listed.count(creatures::state::CARRIED) as i16;
            self.flags &= !flag::COUNT;
        }
        // Outside the pages none is up, unless the square has just asked for them
        // (`0x800BB66C`).
        if self.state != state::ABILITIES && self.flags & flag::ABILITIES == 0 {
            self.flags &= !flag::PAGES;
        }
        let refused = self.refusal != 0;
        if !refused {
            self.keep_cursor();
            self.places();
        }
        // The drawing stands here: a panel that it draws with the clock at 8 is open.
        if self.clock == OPEN {
            self.opened |= self.panels;
        }
        if refused {
            // A refusal goes with any button, or by itself (`0x800BBC0C`).
            if self.refusal_passes < MESSAGE_PASSES {
                self.refusal_passes += 1;
            }
            if pad.pressed != 0 || self.refusal_passes >= MESSAGE_PASSES {
                (self.refusal, self.refusal_passes) = (0, 0);
                self.say();
            }
        } else {
            self.keep_cursor();
            if self.state == state::MAIN {
                self.said = Said::Row(self.cursor.at);
            }
            if self.confirm || self.cancel {
                self.work(&mut passed);
            }
            if self.flags & flag::ENDS != 0 {
                passed.ended = true;
                return passed;
            }
            let more = self.flags & flag::MORE != 0;
            match more {
                true => self.more(pad, &mut passed),
                false => self.input(pad, &mut passed),
            }
            self.ask_form(pad, more, &mut passed);
        }
        if self.clock < OPEN {
            self.clock += 1;
        }
        // The fade's task is passed behind the screen's: the first step of a white
        // that the worker of this pass has asked for.
        if self.whitens.is_some() {
            self.fade.step();
        }
        passed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::creatures::state::{CARRIED, FREE, KEPT};
    use crate::pad::Repeat;

    fn record(species: u32) -> Record {
        let mut record = Record::head(&[0u8; Record::HEAD]).expect("a head");
        record.species = species;
        (record.hp, record.max_hp) = (1, 30);
        record
    }

    /// A game whose player carries the creatures of `carried` and has caught those of
    /// `caught`.
    fn game(carried: &[u32], caught: &[u32]) -> Carried {
        let mut game = Carried::new_game();
        for &species in carried {
            game.creatures.put(CARRIED, record(species));
        }
        for &species in caught {
            game.caught.put(record(species));
        }
        game
    }

    fn press(buttons: u16) -> Pad {
        let mut pad = Pad::default();
        pad.read(0, Repeat::of(1), 0);
        pad.read(buttons, Repeat::of(1), 1);
        pad
    }

    /// A press, and the pass behind it, in which a confirm or a cancel is worked.
    fn go(screen: &mut MergeScreen, buttons: u16) -> Passed {
        let mut passed = screen.pass(&press(buttons));
        let next = screen.pass(&Pad::default());
        passed.sounds.extend(next.sounds);
        passed.exchanged = passed.exchanged.or(next.exchanged);
        passed.merged = passed.merged.or(next.merged);
        passed.names = passed.names.or(next.names);
        passed.spun.extend(next.spun);
        passed.answers = passed.answers.or(next.answers);
        passed.merges = passed.merges.or(next.merges);
        passed.ended |= next.ended;
        passed
    }

    fn listed(screen: &MergeScreen) -> Vec<(usize, i16, u32)> {
        (screen.listed.in_use())
            .map(|(place, c)| (place, c.state, c.record.species))
            .collect()
    }

    #[test]
    fn the_set_up_makes_everybody_whole_and_the_caught_his_own() {
        let mut game = game(&[5], &[6, 7]);
        let screen = MergeScreen::open(4, &mut game);
        assert_eq!(
            listed(&screen),
            [(0, CARRIED, 5), (3, KEPT, 6), (4, KEPT, 7)]
        );
        assert!(screen.listed.in_use().all(|(_, c)| c.record.hp == 30));
        assert_eq!(
            (game.creatures.count(0), game.caught.waiting().count()),
            (0, 0)
        );
        assert_eq!((screen.state, screen.said), (state::MAIN, Said::Row(0)));
    }

    #[test]
    fn the_set_up_fills_the_cocoons_again() {
        // Twelve cocoons of a new game, three of them thrown in fights and two caught.
        let mut game = game(&[5], &[6, 7]);
        assert!(game.keeping.give_cocoons(12));
        assert!(game.keeping.take_cocoons(3));
        MergeScreen::open(4, &mut game);
        assert_eq!(game.keeping.cocoons(), items::MOST_COCOONS);
        assert_eq!(game.caught.waiting().count(), 0);
    }

    #[test]
    fn the_main_menu_says_its_rows_and_refuses_without_creatures() {
        let mut game = game(&[], &[]);
        let mut screen = MergeScreen::open(4, &mut game);
        // The merge wants two creatures, the next rows one.
        let passed = go(&mut screen, pad::CROSS);
        assert_eq!(passed.sounds, [SOUND_REFUSED]);
        assert_eq!(
            (screen.state, screen.said),
            (state::MAIN, Said::Refusal(TOO_FEW))
        );
        // Any button takes the refusal away.
        screen.pass(&press(pad::DOWN));
        assert_eq!((screen.refusal, screen.cursor.at), (0, 0));
        assert_eq!(go(&mut screen, pad::DOWN).sounds, [SOUND_CURSOR]);
        assert_eq!(screen.said, Said::Row(1));
        go(&mut screen, pad::DOWN);
        go(&mut screen, pad::CROSS);
        assert_eq!((screen.cursor.at, screen.said), (2, Said::Refusal(NONE)));
        // It goes by itself too.
        for _ in 0..MESSAGE_PASSES {
            screen.pass(&Pad::default());
        }
        assert_eq!((screen.refusal, screen.said), (0, Said::Row(2)));
    }

    #[test]
    fn a_cancel_of_the_main_menu_goes_to_the_last_row_and_no_further() {
        let mut game = game(&[5], &[]);
        let mut screen = MergeScreen::open(4, &mut game);
        assert_eq!(go(&mut screen, pad::TRIANGLE).sounds, [SOUND_CANCEL]);
        assert_eq!((screen.state, screen.cursor.at), (state::MAIN, STOP));
        assert!(go(&mut screen, pad::TRIANGLE).sounds.is_empty());
        assert_eq!((screen.state, screen.cursor.at), (state::MAIN, STOP));
        // The last row leads to the keeper's last line, and any answer ends the task.
        assert_eq!(go(&mut screen, pad::CROSS).sounds, [SOUND_CONFIRM]);
        assert_eq!(
            (screen.state, screen.said),
            (state::DONE, Said::State(state::DONE))
        );
        assert_eq!(screen.panels, 0);
        assert!(go(&mut screen, pad::TRIANGLE).ended);
        assert!(screen.close(&mut game).is_ok());
        assert_eq!(game.creatures.count(CARRIED), 1);
    }

    #[test]
    fn with_more_than_twelve_kept_the_screen_does_not_leave() {
        let caught: Vec<u32> = (0..12).collect();
        let mut game = game(&[], &caught);
        game.creatures.put(KEPT, record(50));
        let mut screen = MergeScreen::open(4, &mut game);
        go(&mut screen, pad::TRIANGLE);
        go(&mut screen, pad::CROSS);
        assert_eq!((screen.kept, screen.state), (13, state::FULL));
        let passed = go(&mut screen, pad::CROSS);
        assert_eq!((passed.ended, screen.state), (false, state::MAIN));
        assert_eq!(screen.cursor.at, 0);
    }

    /// The screen on the row "Merge", behind a confirm.
    fn merging(game: &mut Carried) -> MergeScreen {
        let mut screen = MergeScreen::open(4, game);
        assert_eq!(go(&mut screen, pad::CROSS).sounds, [SOUND_CONFIRM]);
        assert_eq!(
            (screen.state, screen.back),
            (state::MERGE_FIRST, state::MAIN)
        );
        assert_eq!(screen.said, Said::State(state::MERGE_FIRST));
        screen
    }

    #[test]
    fn two_picks_are_merged_and_the_new_creature_is_shown_beside_them() {
        let mut game = game(&[5], &[6, 7]);
        let mut screen = merging(&mut game);
        // A free place is no first pick.
        go(&mut screen, pad::DOWN);
        assert_eq!(go(&mut screen, pad::CROSS).sounds, [SOUND_REFUSED]);
        assert_eq!(
            (screen.state, screen.said),
            (state::MERGE_FIRST, Said::Refusal(NONE))
        );
        screen.pass(&press(pad::UP));
        go(&mut screen, pad::UP);
        // The carried one is the first: it is marked and copied aside.
        assert_eq!(go(&mut screen, pad::CROSS).sounds, [SOUND_CONFIRM]);
        assert_eq!(
            (screen.state, screen.back, screen.first, screen.marks),
            (state::MERGE_SECOND, state::MERGE_FIRST, 0, 0b1)
        );
        assert_eq!(
            (screen.panels, screen.cursor_rows()),
            (0x16, (Some(0), None))
        );
        assert_eq!(screen.made.as_ref().map(|c| c.record.species), Some(5));
        // The first place again takes the pick back; a free place is refused.
        assert_eq!(go(&mut screen, pad::CROSS).sounds, [SOUND_CANCEL]);
        assert_eq!((screen.state, screen.marks), (state::MERGE_FIRST, 0));
        go(&mut screen, pad::CROSS);
        go(&mut screen, pad::DOWN);
        assert_eq!(go(&mut screen, pad::CROSS).sounds, [SOUND_REFUSED]);
        assert_eq!(screen.said, Said::Refusal(NONE));
        screen.pass(&press(pad::DOWN));
        // The second of the kept is the second pick: without a minion skin among his
        // things the two are merged at once.
        for _ in 0..3 {
            go(&mut screen, pad::DOWN);
        }
        assert_eq!((screen.second, screen.shown_place()), (4, 4));
        let passed = go(&mut screen, pad::CROSS);
        assert_eq!(passed.sounds, [SOUND_CONFIRM]);
        let asked = Merging {
            first: 0,
            second: 4,
            skin: None,
        };
        assert_eq!(passed.merges, Some(asked));
        assert_eq!(
            (screen.state, screen.back, screen.marks),
            (state::MERGE_ASKED, state::MERGE_SECOND, 0b1_0001)
        );
        // The keeper's second lines are up, and the cursor of the three is on the new
        // creature, which the box compares with the first.
        assert_eq!(screen.said, Said::Second(state::MERGE_ASKED));
        assert_ne!(screen.flags & flag::MORE, 0);
        assert_eq!((screen.panels, screen.which.at), (0x3E, ON_THE_NEW));
        assert_eq!(screen.shown_place(), MADE);
        assert_eq!(screen.compared().map(|c| c.record.species), Some(5));
        // Left and right go through the three and wrap.
        assert_eq!(go(&mut screen, pad::LEFT).sounds, [SOUND_CURSOR]);
        assert_eq!((screen.shown_place(), screen.compared()), (4, None));
        go(&mut screen, pad::LEFT);
        assert_eq!(screen.shown_place(), 0);
        go(&mut screen, pad::LEFT);
        assert_eq!(screen.shown_place(), MADE);
        // The square turns the pages of the box, up and down pick a row there, and a
        // confirm closes them without leading on.
        assert_eq!(go(&mut screen, pad::SQUARE).sounds, [SOUND_CURSOR]);
        assert_eq!(
            (screen.drawn_page(), screen.page_cursor.rows),
            (flag::ATTACKS, 5)
        );
        go(&mut screen, pad::DOWN);
        assert_eq!(screen.page_cursor.at, 1);
        go(&mut screen, pad::RIGHT);
        assert_eq!((screen.shown_place(), screen.page_cursor.at), (0, 0));
        assert_eq!(go(&mut screen, pad::CROSS).sounds, [SOUND_CANCEL]);
        assert_eq!(
            (screen.drawn_page(), screen.flags & flag::MORE),
            (0, flag::MORE)
        );
        // Round the four pages the square's flag goes by itself.
        for _ in 0..4 {
            go(&mut screen, pad::SQUARE);
        }
        assert_eq!((screen.page(), screen.flags & flag::ABILITIES), (0, 0));
        // A button leads on to the question, the cursor on the "No".
        assert_eq!(go(&mut screen, pad::TRIANGLE).sounds, [SOUND_CONFIRM]);
        assert_eq!(
            (screen.flags & flag::MORE, screen.said, screen.cursor.at),
            (0, Said::State(state::MERGE_ASKED), 1)
        );
        // The "No" leads back to the second pick, the first marked again and the
        // cursor where the second was picked.
        assert_eq!(go(&mut screen, pad::CROSS).sounds, [SOUND_CANCEL]);
        assert_eq!(
            (
                screen.state,
                screen.back,
                screen.marks,
                screen.carried_marks
            ),
            (state::MERGE_SECOND, state::MERGE_FIRST, 0b1, 0b1)
        );
        assert_eq!(screen.cursor_rows(), (None, Some(1)));
        assert_eq!(screen.said, Said::State(state::MERGE_SECOND));
        // And a cancel there to the first pick, where it was made.
        go(&mut screen, pad::TRIANGLE);
        assert_eq!((screen.state, screen.marks), (state::MERGE_FIRST, 0));
        assert_eq!(screen.cursor_rows(), (Some(0), None));
        assert_eq!(
            listed(&screen),
            [(0, CARRIED, 5), (3, KEPT, 6), (4, KEPT, 7)]
        );
    }

    #[test]
    fn with_minion_skins_a_merge_asks_for_one() {
        let mut game = game(&[5, 6], &[]);
        let worn = game.worn;
        assert!(game.keeping.give(3, 1, items::Kind::Creature, worn));
        assert!(game.keeping.give(7, 2, items::Kind::Creature, worn));
        let mut screen = merging(&mut game);
        go(&mut screen, pad::CROSS);
        go(&mut screen, pad::DOWN);
        // The second pick leads to the keeper's line about the skins, then to her
        // question, the cursor on the "No".
        let passed = go(&mut screen, pad::CROSS);
        assert_eq!((passed.merges, screen.state), (None, state::SKINS_ASKED));
        assert_eq!((screen.back, screen.panels), (state::MERGE_SECOND, 0x3A));
        assert_eq!(screen.said, Said::Second(state::SKINS_ASKED));
        // No cursor moves while her line waits for a button.
        assert!(go(&mut screen, pad::UP).sounds.is_empty());
        assert_eq!(go(&mut screen, pad::CROSS).sounds, [SOUND_CONFIRM]);
        assert_eq!(
            (screen.said, screen.cursor.at, screen.state),
            (Said::State(state::SKINS_ASKED), 1, state::SKINS_ASKED)
        );
        // The yes: the list of the skins, four lines for two skins.
        go(&mut screen, pad::UP);
        assert_eq!(go(&mut screen, pad::CROSS).sounds, [SOUND_CONFIRM]);
        assert_eq!(
            (screen.state, screen.back),
            (state::SKINS, state::MERGE_SECOND)
        );
        assert_eq!((screen.panels, screen.cursor.lines), (0x72, 4));
        assert_eq!(screen.marks, 0b11);
        // A line without a skin is refused.
        go(&mut screen, pad::DOWN);
        go(&mut screen, pad::DOWN);
        assert_eq!(go(&mut screen, pad::CROSS).sounds, [SOUND_REFUSED]);
        assert_eq!(screen.said, Said::Refusal(NO_SKIN));
        screen.pass(&press(pad::UP));
        go(&mut screen, pad::UP);
        assert_eq!(screen.skin_cursor(), (1, 0));
        // The second skin is taken for the merge.
        let passed = go(&mut screen, pad::CROSS);
        let asked = Merging {
            first: 0,
            second: 1,
            skin: Some(7),
        };
        assert_eq!(passed.merges, Some(asked));
        assert_eq!(
            (screen.state, screen.back),
            (state::MERGE_ASKED, state::MERGE_FIRST)
        );
        assert_ne!(screen.flags & flag::SKIN, 0);
        // Back from the new creature no skin is taken any more.
        go(&mut screen, pad::CROSS);
        go(&mut screen, pad::TRIANGLE);
        assert_eq!(
            (screen.state, screen.flags & flag::SKIN),
            (state::MERGE_SECOND, 0)
        );
        // The "No" to the skins merges without one; a cancel of the list leads back
        // to her line and her question.
        go(&mut screen, pad::CROSS);
        go(&mut screen, pad::CROSS);
        go(&mut screen, pad::UP);
        go(&mut screen, pad::CROSS);
        assert_eq!(go(&mut screen, pad::TRIANGLE).sounds, [SOUND_CANCEL]);
        assert_eq!(
            (screen.state, screen.said, screen.marks),
            (state::SKINS_ASKED, Said::Second(state::SKINS_ASKED), 0b11)
        );
        go(&mut screen, pad::CROSS);
        let passed = go(&mut screen, pad::CROSS);
        assert_eq!(passed.sounds, [SOUND_CANCEL]);
        assert_eq!(
            passed.merges,
            Some(Merging {
                skin: None,
                ..asked
            })
        );
        assert_eq!(
            (screen.state, screen.back),
            (state::MERGE_ASKED, state::MERGE_SECOND)
        );
        assert_eq!(screen.said, Said::Second(state::MERGE_ASKED));
    }

    #[test]
    fn the_yes_of_a_merge_leaves_the_new_creature_in_the_first_place() {
        let mut game = game(&[5], &[6, 7]);
        let mut screen = merging(&mut game);
        go(&mut screen, pad::CROSS);
        for _ in 0..3 {
            go(&mut screen, pad::DOWN);
        }
        let asked = go(&mut screen, pad::CROSS).merges.expect("a merge");
        assert_eq!((asked.first, asked.second, asked.skin), (0, 3, None));
        // What the caller's merge made lies aside.
        screen.made.as_mut().expect("a copy").record.level = 9;
        go(&mut screen, pad::CROSS);
        go(&mut screen, pad::UP);
        // The yes: the new one over the first, the second free, the marks gone, and
        // the task asleep with the answer 1 for the room's script.
        let passed = go(&mut screen, pad::CROSS);
        assert_eq!(passed.sounds, [SOUND_CONFIRM]);
        let merged = Merged {
            first: 0,
            second: 3,
            skin: None,
            new_species: None,
        };
        assert_eq!(passed.merged, Some(merged));
        assert_eq!(passed.answers, Some(ANSWER_OF_A_MERGE));
        assert!(screen.away && screen.state == state::MERGE_ASKED);
        assert_eq!(listed(&screen), [(0, CARRIED, 5), (4, KEPT, 7)]);
        assert_eq!(screen.creature(0).map(|c| c.record.level), Some(9));
        assert_eq!(screen.marks, 0);
        assert_eq!(go(&mut screen, pad::CROSS), Passed::default());
        // The script's next asking is the name entry on the new creature's name, and
        // behind the name "once more?" is up, the cursor on the yes.
        assert_eq!(screen.back(), Some(0));
        assert_eq!((screen.away, screen.naming), (false, Some(0)));
        assert_eq!(go(&mut screen, pad::CROSS), Passed::default());
        let name = [2, 7, 1, 8, 0, 0, 0, 0, 0, 0];
        screen.named(name);
        assert_eq!(
            (screen.naming, screen.state, screen.back),
            (None, state::MERGE_MORE, state::MAIN)
        );
        assert_eq!((screen.panels, screen.cursor.at), (0x0A, 0));
        assert_eq!(screen.said, Said::State(state::MERGE_MORE));
        assert_eq!(screen.flags & flag::LISTS, 0);
        assert_eq!(screen.creature(0).map(|c| c.record.name), Some(name));
        // Two are left: the yes starts with the first pick again.
        assert_eq!(go(&mut screen, pad::CROSS).sounds, [SOUND_CONFIRM]);
        assert_eq!((screen.kept, screen.carried), (1, 1));
        assert_eq!(
            (screen.state, screen.back, screen.marks),
            (state::MERGE_FIRST, state::MAIN, 0)
        );
    }

    #[test]
    fn without_magic_fx_a_merge_is_taken_where_the_white_has_gone() {
        let mut game = game(&[], &[6, 7]);
        let worn = game.worn;
        assert!(game.keeping.give(3, 1, items::Kind::Creature, worn));
        let mut screen = merging(&mut game);
        screen.magic_fx = false;
        for _ in 0..3 {
            go(&mut screen, pad::DOWN);
        }
        go(&mut screen, pad::CROSS);
        go(&mut screen, pad::DOWN);
        // Her line about the skins, her question, the yes, the list, the first skin.
        go(&mut screen, pad::CROSS);
        go(&mut screen, pad::CROSS);
        go(&mut screen, pad::UP);
        go(&mut screen, pad::CROSS);
        let asked = go(&mut screen, pad::CROSS).merges.expect("a merge");
        assert_eq!((asked.first, asked.second, asked.skin), (3, 4, Some(3)));
        // A new species has its own number in the first byte of its line.
        screen.made.as_mut().expect("a copy").line.species[0] = 35;
        go(&mut screen, pad::CROSS);
        screen.pass(&press(pad::UP));
        screen.pass(&press(pad::CROSS));
        // The pass of the yes: nothing is merged yet and nothing answered.
        let passed = screen.pass(&Pad::default());
        assert_eq!((passed.merged, passed.answers), (None, None));
        assert_eq!(passed.sounds, [SOUND_CONFIRM]);
        assert_ne!(screen.flags & flag::WHITE, 0);
        assert_eq!(screen.white(), 7);
        assert_eq!(listed(&screen), [(3, KEPT, 6), (4, KEPT, 7)]);
        for _ in 0..62 {
            assert_eq!(screen.pass(&press(pad::TRIANGLE)), Passed::default());
        }
        // The pass behind the white at rest: the merge with its skin, and the task
        // asleep for the name entry on the new creature's name.
        let passed = screen.pass(&Pad::default());
        let merged = Merged {
            first: 3,
            second: 4,
            skin: Some(3),
            new_species: Some(35),
        };
        assert_eq!((passed.merged, passed.names), (Some(merged), Some(3)));
        assert_eq!(screen.flags & (flag::WHITE | flag::SKIN), 0);
        assert_eq!((screen.white(), screen.naming), (0, Some(3)));
        assert_eq!(listed(&screen), [(3, KEPT, 6)]);
        screen.named([1, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(screen.state, state::MERGE_MORE);
        // One creature is left: the yes of "once more?" is refused, the "No" leads to
        // the main menu.
        assert_eq!(go(&mut screen, pad::CROSS).sounds, [SOUND_REFUSED]);
        assert_eq!(screen.said, Said::Refusal(TOO_FEW));
        screen.pass(&press(pad::LEFT));
        go(&mut screen, pad::DOWN);
        assert_eq!(go(&mut screen, pad::CROSS).sounds, [SOUND_CANCEL]);
        assert_eq!((screen.state, screen.cursor.at), (state::MAIN, 0));
    }

    /// The screen on the row of who goes with the player, behind a confirm.
    fn taking(game: &mut Carried) -> MergeScreen {
        let mut screen = MergeScreen::open(4, game);
        go(&mut screen, pad::DOWN);
        go(&mut screen, pad::DOWN);
        assert_eq!(go(&mut screen, pad::CROSS).sounds, [SOUND_CONFIRM]);
        assert_eq!((screen.state, screen.back), (state::TAKE, state::MAIN));
        screen
    }

    #[test]
    fn the_cursor_goes_from_one_list_to_the_other() {
        let mut game = game(&[5], &[6, 7]);
        let mut screen = taking(&mut game);
        // It starts in the carried, whose panels open over eight passes.
        assert_eq!((screen.panels, screen.opened, screen.list), (0x06, 0, 0));
        assert_eq!(screen.cursor_rows(), (Some(0), None));
        go(&mut screen, pad::DOWN);
        go(&mut screen, pad::DOWN);
        assert_eq!(screen.cursor_rows(), (Some(2), None));
        // Down from the last row of the carried: the first of the kept.
        assert_eq!(go(&mut screen, pad::DOWN).sounds, [SOUND_CURSOR]);
        assert_eq!(screen.cursor_rows(), (None, Some(0)));
        assert_eq!((screen.shown_place(), screen.list), (3, 1));
        // The kept scroll: 24 places under four rows.
        for _ in 0..5 {
            go(&mut screen, pad::DOWN);
        }
        assert_eq!(
            (screen.cursor.at, screen.scroll(), screen.shown_place()),
            (3, 2, 8)
        );
        // Up scrolls back first, and from the first row of all leaves for the carried.
        for _ in 0..5 {
            go(&mut screen, pad::UP);
        }
        assert_eq!((screen.cursor.at, screen.scroll()), (0, 0));
        go(&mut screen, pad::UP);
        assert_eq!(screen.cursor_rows(), (Some(2), None));
        assert_eq!((screen.shown_place(), screen.opened), (2, 0x06));
    }

    #[test]
    fn two_picked_places_change_and_the_choice_goes_on() {
        let mut game = game(&[5], &[6, 7]);
        let mut screen = taking(&mut game);
        // A free place of the carried is picked, then the second of the kept.
        go(&mut screen, pad::DOWN);
        assert_eq!(go(&mut screen, pad::CROSS).sounds, [SOUND_CONFIRM]);
        assert_eq!(
            (screen.state, screen.back, screen.first),
            (state::EXCHANGE, state::TAKE, 1)
        );
        assert_eq!(
            (screen.marks, screen.carried_marks, screen.cursor.at),
            (0b10, 0b10, 1)
        );
        assert_eq!(screen.said, Said::State(state::EXCHANGE));
        go(&mut screen, pad::DOWN);
        go(&mut screen, pad::DOWN);
        go(&mut screen, pad::DOWN);
        assert_eq!((screen.second, screen.shown_place()), (4, 4));
        let passed = go(&mut screen, pad::CROSS);
        assert_eq!(passed.exchanged, Some((1, 4)));
        assert_eq!(
            listed(&screen),
            [(0, CARRIED, 5), (1, CARRIED, 7), (3, KEPT, 6)]
        );
        // Back in the choice, the cursor where it was, nothing marked.
        assert_eq!(
            (screen.state, screen.back, screen.marks),
            (state::TAKE, state::MAIN, 0)
        );
        assert_eq!(screen.cursor_rows(), (None, Some(1)));
        screen.pass(&Pad::default());
        assert_eq!((screen.kept, screen.carried), (1, 2));
        // A creature is left behind by changing it with a free place of the kept.
        go(&mut screen, pad::CROSS);
        for _ in 0..4 {
            go(&mut screen, pad::UP);
        }
        assert_eq!(screen.second, 0);
        assert_eq!(go(&mut screen, pad::CROSS).exchanged, Some((4, 0)));
        assert_eq!(
            listed(&screen),
            [(1, CARRIED, 7), (3, KEPT, 6), (4, KEPT, 5)]
        );
        assert_eq!(screen.listed.state(0), FREE);
    }

    #[test]
    fn a_confirm_on_the_picked_place_and_a_cancel_take_the_pick_back() {
        let mut game = game(&[5], &[6, 7]);
        let mut screen = taking(&mut game);
        for _ in 0..4 {
            go(&mut screen, pad::DOWN);
        }
        go(&mut screen, pad::CROSS);
        assert_eq!(
            (screen.state, screen.first, screen.kept_marks),
            (state::EXCHANGE, 4, 0b10)
        );
        // The same place again: a cancel.
        assert_eq!(go(&mut screen, pad::CROSS).sounds, [SOUND_CANCEL]);
        assert_eq!((screen.state, screen.marks), (state::TAKE, 0));
        assert_eq!(screen.cursor_rows(), (None, Some(1)));
        // A cancel from another place puts the cursor back on the first.
        go(&mut screen, pad::CROSS);
        go(&mut screen, pad::UP);
        go(&mut screen, pad::UP);
        assert_eq!(screen.cursor_rows(), (Some(2), None));
        go(&mut screen, pad::TRIANGLE);
        assert_eq!((screen.state, screen.list), (state::TAKE, 1));
        assert_eq!(screen.cursor_rows(), (None, Some(1)));
        // And a cancel of the choice leads to the main menu, on its row.
        go(&mut screen, pad::TRIANGLE);
        assert_eq!(
            (screen.state, screen.cursor.at, screen.said),
            (state::MAIN, 2, Said::Row(2))
        );
        assert_eq!(screen.flags & flag::LISTS, 0);
        assert_eq!(
            listed(&screen),
            [(0, CARRIED, 5), (3, KEPT, 6), (4, KEPT, 7)]
        );
    }

    /// The screen on the row "View", behind a confirm.
    fn viewing(game: &mut Carried) -> MergeScreen {
        let mut screen = MergeScreen::open(4, game);
        for _ in 0..3 {
            go(&mut screen, pad::DOWN);
        }
        assert_eq!(go(&mut screen, pad::CROSS).sounds, [SOUND_CONFIRM]);
        assert_eq!((screen.state, screen.back), (state::VIEW, state::MAIN));
        assert_eq!(screen.said, Said::State(state::VIEW));
        screen
    }

    #[test]
    fn the_square_of_the_view_leads_to_the_four_pages_of_the_box() {
        let mut game = game(&[5], &[6]);
        // The second carried one knows six spells.
        game.creatures.put(CARRIED, {
            let mut record = record(7);
            record.spells = 0b0011_0101_1010;
            record
        });
        let mut screen = viewing(&mut game);
        go(&mut screen, pad::DOWN);
        assert_eq!(screen.shown_place(), 1);
        // The square notes a confirm, which the next pass works: the pages, with the
        // "Detail" still up and the place marked.
        assert_eq!(go(&mut screen, pad::SQUARE).sounds, [SOUND_CONFIRM]);
        assert_eq!((screen.state, screen.back), (state::ABILITIES, state::VIEW));
        assert_eq!((screen.page(), screen.flags & flag::ABILITIES), (0, 0));
        assert_eq!((screen.panels, screen.marks), (0x06, 0b10));
        assert_eq!(
            (screen.shown_place(), screen.cursor_rows()),
            (1, (None, None))
        );
        assert_eq!(screen.said, Said::State(state::ABILITIES));
        // The body attacks: five rows that do not scroll.
        assert_eq!(go(&mut screen, pad::SQUARE).sounds, [SOUND_CURSOR]);
        assert_eq!(screen.page(), flag::ATTACKS);
        assert_eq!((screen.page_cursor.rows, screen.page_cursor.lines), (5, 5));
        for _ in 0..6 {
            go(&mut screen, pad::DOWN);
        }
        assert_eq!((screen.page_cursor.at, screen.page_cursor.first), (4, 0));
        assert_eq!(screen.shown_place(), 1);
        // The spells: four rows over a line for each it knows, the cursor at the top.
        go(&mut screen, pad::SQUARE);
        assert_eq!(screen.page(), flag::SPELLS);
        assert_eq!((screen.page_cursor.rows, screen.page_cursor.lines), (4, 6));
        assert_eq!((screen.page_cursor.at, screen.page_cursor.first), (0, 0));
        for _ in 0..5 {
            go(&mut screen, pad::DOWN);
        }
        assert_eq!((screen.page_cursor.at, screen.page_cursor.first), (3, 2));
        // The "Family", and round to the "Detail".
        go(&mut screen, pad::SQUARE);
        assert_eq!(
            (screen.page(), screen.page_cursor),
            (flag::FAMILY, ONE_PLACE)
        );
        assert_eq!(go(&mut screen, pad::SQUARE).sounds, [SOUND_CURSOR]);
        assert_eq!(screen.page(), 0);
        // A confirm does nothing on a page; a cancel leads back to the view, the
        // cursor where it was, nothing marked and no page up.
        go(&mut screen, pad::SQUARE);
        assert!(go(&mut screen, pad::CROSS).sounds.is_empty());
        assert_eq!(
            (screen.state, screen.page()),
            (state::ABILITIES, flag::ATTACKS)
        );
        assert_eq!(go(&mut screen, pad::TRIANGLE).sounds, [SOUND_CANCEL]);
        assert_eq!((screen.state, screen.back), (state::VIEW, state::MAIN));
        assert_eq!((screen.cursor_rows(), screen.marks), ((Some(1), None), 0));
        screen.pass(&Pad::default());
        assert_eq!(screen.page(), 0);
    }

    #[test]
    fn the_spells_of_a_creature_that_knows_few_have_four_lines() {
        let mut game = game(&[5], &[]);
        let mut screen = viewing(&mut game);
        go(&mut screen, pad::SQUARE);
        go(&mut screen, pad::SQUARE);
        go(&mut screen, pad::SQUARE);
        assert_eq!(screen.page(), flag::SPELLS);
        assert_eq!((screen.page_cursor.rows, screen.page_cursor.lines), (4, 4));
    }

    #[test]
    fn a_free_place_of_the_view_is_refused() {
        let mut game = game(&[5], &[]);
        let mut screen = viewing(&mut game);
        go(&mut screen, pad::DOWN);
        let passed = go(&mut screen, pad::CROSS);
        assert_eq!(passed.sounds, [SOUND_REFUSED]);
        assert_eq!(
            (screen.state, screen.said, screen.marks),
            (state::VIEW, Said::Refusal(NONE), 0)
        );
        screen.pass(&press(pad::UP));
        assert_eq!(screen.said, Said::State(state::VIEW));
        // The square on a free place is refused as well, and the game keeps its
        // note: the next confirm on a creature leads to the pages.
        go(&mut screen, pad::SQUARE);
        assert_eq!((screen.state, screen.refusal), (state::VIEW, NONE));
        assert_ne!(screen.flags & flag::ABILITIES, 0);
        screen.pass(&press(pad::UP));
        go(&mut screen, pad::UP);
        assert_eq!(screen.shown_place(), 0);
        go(&mut screen, pad::CROSS);
        assert_eq!(screen.state, state::ABILITIES);
        go(&mut screen, pad::TRIANGLE);
        // A cancel leads to the main menu, on its row.
        go(&mut screen, pad::TRIANGLE);
        assert_eq!((screen.state, screen.cursor.at), (state::MAIN, 3));
    }

    #[test]
    fn a_silk_goes_by_the_level() {
        let silks: Vec<u8> = [1, 3, 4, 6, 7, 11, 12, 17, 18, 24, 25, 30, 31, 99]
            .map(silk)
            .to_vec();
        assert_eq!(
            silks,
            [33, 33, 34, 34, 35, 35, 36, 36, 37, 37, 38, 38, 39, 39]
        );
    }

    #[test]
    fn marked_cocoons_are_spun_into_silk() {
        let mut game = game(&[5], &[6, 7, 8]);
        let mut screen = MergeScreen::open(4, &mut game);
        go(&mut screen, pad::DOWN);
        assert_eq!(go(&mut screen, pad::CROSS).sounds, [SOUND_CONFIRM]);
        assert_eq!((screen.state, screen.back), (state::SPIN, state::MAIN));
        // With nothing marked the circle is "View form", and notes no confirm while
        // the count is 0. A free place is not marked and makes no sound.
        let passed = go(&mut screen, pad::CIRCLE);
        assert_eq!(
            (passed.sounds.as_slice(), passed.looks),
            (&[SOUND_CONFIRM][..], Some(0))
        );
        assert_eq!(screen.pass(&press(pad::TRIANGLE)).sounds, [SOUND_CANCEL]);
        assert!(go(&mut screen, 0).sounds.is_empty());
        assert_eq!(
            (screen.state, screen.said),
            (state::SPIN, Said::State(state::SPIN))
        );
        go(&mut screen, pad::DOWN);
        assert_eq!(screen.shown_place(), 1);
        assert!(go(&mut screen, pad::CROSS).sounds.is_empty());
        assert_eq!(
            (screen.state, screen.marks, screen.marked),
            (state::SPIN, 0, 0)
        );
        // A confirm marks the place and stays; a second one takes the mark back.
        go(&mut screen, pad::UP);
        assert_eq!(go(&mut screen, pad::CROSS).sounds, [SOUND_CONFIRM]);
        assert_eq!(
            (screen.state, screen.marks, screen.marked),
            (state::SPIN, 1, 1)
        );
        assert_eq!(screen.carried_marks, 1);
        go(&mut screen, pad::CROSS);
        assert_eq!(
            (screen.marks, screen.carried_marks, screen.marked),
            (0, 0, 0)
        );
        // The carried one and the second of the kept are marked.
        go(&mut screen, pad::CROSS);
        for _ in 0..4 {
            go(&mut screen, pad::DOWN);
        }
        assert_eq!((screen.shown_place(), screen.list), (4, 1));
        go(&mut screen, pad::CROSS);
        assert_eq!((screen.marks, screen.kept_marks), (0b1_0001, 0b10));
        assert_eq!(screen.marked, 2);
        // With a place marked the square closes no gaps.
        assert!(go(&mut screen, pad::SQUARE).sounds.is_empty());
        // The circle asks; a "No" leads back with the marks gone and the count left.
        assert_eq!(go(&mut screen, pad::CIRCLE).sounds, [SOUND_CONFIRM]);
        assert_eq!(
            (screen.state, screen.back),
            (state::SPIN_ASKED, state::SPIN)
        );
        assert_eq!((screen.panels, screen.cursor.at), (0x0E, 1));
        assert_eq!(screen.marks, 0b1_0001);
        assert_eq!(go(&mut screen, pad::CROSS).sounds, [SOUND_CANCEL]);
        assert_eq!((screen.state, screen.back), (state::SPIN, state::MAIN));
        assert_eq!((screen.marks, screen.marked), (0, 2));
        assert_eq!(screen.cursor_rows(), (None, Some(1)));
        // So the circle notes a confirm with nothing marked, and in the same pass it
        // is "View form": the look starts, and its end forgets the confirm, which
        // would have been refused.
        let passed = go(&mut screen, pad::CIRCLE);
        assert_eq!(
            (passed.sounds.as_slice(), passed.looks),
            (&[SOUND_CONFIRM][..], Some(4))
        );
        assert!(screen.pass(&press(pad::TRIANGLE)).looked);
        assert!(go(&mut screen, 0).sounds.is_empty());
        assert_eq!(
            (screen.state, screen.said),
            (state::SPIN, Said::State(state::SPIN))
        );
        // Marked again, asked again, and the yes: a silk for each by its level, the
        // places free, and the task asleep with the answer 2 for the room's script.
        go(&mut screen, pad::CROSS);
        go(&mut screen, pad::UP);
        go(&mut screen, pad::CROSS);
        assert_eq!(screen.marks, 0b1_1000);
        go(&mut screen, pad::CIRCLE);
        go(&mut screen, pad::UP);
        let passed = go(&mut screen, pad::CROSS);
        assert_eq!(passed.sounds, [SOUND_CONFIRM]);
        assert_eq!(passed.spun, [33, 33]);
        assert_eq!(passed.answers, Some(ANSWER_OF_A_SPIN));
        assert!(screen.away && screen.state == state::SPIN_ASKED);
        // Asleep it is not passed; the script's next asking brings state 5 back with
        // the cursor where it was, and the counts are taken anew.
        assert_eq!(go(&mut screen, pad::CROSS), Passed::default());
        screen.back();
        assert!(!screen.away);
        assert_eq!((screen.state, screen.back), (state::SPIN, state::MAIN));
        assert_eq!((screen.marks, screen.kept_marks, screen.marked), (0, 0, 4));
        assert_eq!(screen.said, Said::State(state::SPIN));
        assert_eq!(screen.cursor_rows(), (None, Some(0)));
        assert_eq!(listed(&screen), [(0, CARRIED, 5), (5, KEPT, 8)]);
        screen.pass(&Pad::default());
        assert_eq!((screen.kept, screen.carried), (1, 1));
    }

    #[test]
    fn without_magic_fx_a_white_grows_over_the_screen_and_goes() {
        let mut game = game(&[], &[6, 7]);
        let mut screen = MergeScreen::open(4, &mut game);
        screen.magic_fx = false;
        go(&mut screen, pad::DOWN);
        go(&mut screen, pad::CROSS);
        for _ in 0..3 {
            go(&mut screen, pad::DOWN);
        }
        assert_eq!(screen.shown_place(), 3);
        go(&mut screen, pad::CROSS);
        go(&mut screen, pad::CIRCLE);
        assert_eq!(screen.state, state::SPIN_ASKED);
        screen.pass(&press(pad::UP));
        screen.pass(&press(pad::CROSS));
        // The pass of the yes: nothing is answered, the bar is not drawn any more and
        // the fade's task draws its first white behind the setter's rectangle of none.
        let passed = screen.pass(&Pad::default());
        assert_eq!((passed.spun.as_slice(), passed.answers), (&[33][..], None));
        assert_eq!((screen.state, screen.away), (state::SPIN_ASKED, false));
        assert_ne!(screen.flags & flag::WHITE, 0);
        assert_eq!(screen.white(), 7);
        // It grows for thirty more passes in which no button counts, is whole in the
        // pass that lets it go, and goes over as many.
        let seen: Vec<u8> = (0..63)
            .map(|_| {
                assert_eq!(screen.pass(&press(pad::TRIANGLE)), Passed::default());
                screen.white()
            })
            .collect();
        assert_eq!((seen[0], seen[29], seen[30], seen[31]), (15, 247, 255, 239));
        assert_eq!((seen[60], seen[61], seen[62]), (7, 0, 0));
        // The pass behind the white at rest: state 5 with the place gone.
        assert_eq!((screen.state, screen.back), (state::SPIN, state::MAIN));
        assert_eq!(screen.flags & flag::WHITE, 0);
        assert_eq!(listed(&screen), [(4, KEPT, 7)]);
        assert_eq!(go(&mut screen, pad::TRIANGLE).sounds, [SOUND_CANCEL]);
        assert_eq!(screen.state, state::MAIN);
    }

    #[test]
    fn a_confirm_in_the_view_asks_for_a_new_name() {
        let mut game = game(&[5, 6], &[7, 8]);
        let mut screen = viewing(&mut game);
        // The second of the kept, with the list scrolled by nothing.
        for _ in 0..4 {
            go(&mut screen, pad::DOWN);
        }
        assert_eq!((screen.shown_place(), screen.list), (4, 1));
        // The question: the bar of an answer on the "No", the place marked.
        assert_eq!(go(&mut screen, pad::CROSS).sounds, [SOUND_CONFIRM]);
        assert_eq!(
            (screen.state, screen.back),
            (state::RENAME_ASKED, state::VIEW)
        );
        assert_eq!((screen.panels, screen.cursor.at), (0x0E, 1));
        assert_eq!(screen.said, Said::State(state::RENAME_ASKED));
        assert_eq!((screen.shown_place(), screen.marks), (4, 1 << 4));
        // A confirm on the "No" is a cancel: the view, the cursor where it was.
        assert_eq!(go(&mut screen, pad::CROSS).sounds, [SOUND_CANCEL]);
        assert_eq!((screen.state, screen.back), (state::VIEW, state::MAIN));
        assert_eq!((screen.cursor_rows(), screen.marks), ((None, Some(1)), 0));
        // The "Yes": the task sleeps for the name entry and no pass moves it.
        go(&mut screen, pad::CROSS);
        go(&mut screen, pad::UP);
        assert_eq!(screen.cursor.at, 0);
        let passed = go(&mut screen, pad::CROSS);
        assert_eq!(
            (passed.sounds, passed.names),
            (vec![SOUND_CONFIRM], Some(4))
        );
        assert_eq!((screen.naming, screen.marks), (Some(4), 0));
        assert_eq!(go(&mut screen, pad::TRIANGLE), Passed::default());
        assert_eq!(screen.state, state::RENAME_ASKED);
        // The way back: the name is the creature's and the view is as it was left.
        let name = [3, 1, 4, 1, 5, 0, 0, 0, 0, 0];
        screen.named(name);
        assert_eq!((screen.naming, screen.state), (None, state::VIEW));
        assert_eq!((screen.back, screen.panels), (state::MAIN, 0x06));
        assert_eq!(screen.said, Said::State(state::VIEW));
        assert_eq!(screen.cursor_rows(), (None, Some(1)));
        let named = screen.creature(4).expect("a creature");
        assert_eq!((named.record.species, named.record.name), (8, name));
        assert!(
            screen
                .creature(3)
                .is_some_and(|other| other.record.name != name)
        );
        // And it goes on from there.
        go(&mut screen, pad::UP);
        assert_eq!(screen.shown_place(), 3);
    }

    fn hold(screen: &mut MergeScreen, buttons: u16) -> Passed {
        let mut pad = Pad::default();
        pad.read(buttons, Repeat::of(1), 0);
        pad.read(buttons, Repeat::of(1), 1);
        screen.pass(&pad)
    }

    #[test]
    fn the_circle_looks_at_a_form_until_a_cancel() {
        let mut game = game(&[5], &[]);
        let mut screen = viewing(&mut game);
        let said = screen.said;
        // The circle: the sound of a confirm and the model job of the place.
        let passed = screen.pass(&press(pad::CIRCLE));
        assert_eq!(passed.sounds, [SOUND_CONFIRM]);
        assert_eq!(passed.looks, Some(0));
        assert_ne!(screen.flags & flag::FORM, 0);
        assert_eq!(screen.said, said);
        // The pass that finds the job through: the fixed lines and the actor.
        let clock = screen.clock;
        assert_eq!(screen.pass(&press(pad::LEFT)), Passed::default());
        assert_eq!(screen.said, Said::Form);
        assert_eq!(
            screen.form.map(|form| (form.made, form.turn)),
            Some((true, 0))
        );
        screen.form_stands([2048; 3]);
        // Held buttons turn it and scale it; the cursor does not move.
        hold(&mut screen, pad::LEFT | pad::RIGHT);
        hold(&mut screen, pad::RIGHT);
        hold(&mut screen, pad::RIGHT | pad::R1 | pad::R2);
        let form = screen.form.expect("the form");
        assert_eq!((form.turn, form.scale), (-136, Some([2112; 3])));
        assert_eq!(
            (screen.state, screen.cursor.at, screen.clock),
            (state::VIEW, 0, clock)
        );
        // A confirm is nothing here; the cancel ends it with its sound.
        assert_eq!(screen.pass(&press(pad::CROSS)), Passed::default());
        let passed = screen.pass(&press(pad::TRIANGLE));
        assert_eq!(passed.sounds, [SOUND_CANCEL]);
        assert!(passed.looked);
        assert_eq!((screen.form, screen.flags & flag::FORM), (None, 0));
        assert_eq!((screen.state, screen.said), (state::VIEW, said));
    }

    #[test]
    fn a_form_is_scaled_between_a_quarter_and_the_whole() {
        // Up: a step that would reach 4096 is not made.
        assert_eq!(Form::scaled([3968, 100, 100], pad::R1), [4032, 164, 164]);
        assert_eq!(Form::scaled([4032; 3], pad::R1), [4032; 3]);
        assert_eq!(Form::scaled([4096; 3], pad::R1), [4096; 3]);
        // Down: not to 1024 or below.
        assert_eq!(Form::scaled([4096; 3], pad::R2), [4032; 3]);
        assert_eq!(Form::scaled([1152; 3], pad::R2), [1088; 3]);
        assert_eq!(Form::scaled([1088; 3], pad::R2), [1088; 3]);
        assert_eq!(Form::scaled([2000; 3], 0), [2000; 3]);
        assert_eq!(Form::turned(4000, pad::LEFT), 4136);
        assert_eq!(Form::turned(0, 0), 0);
    }

    #[test]
    fn the_circle_is_view_form_only_where_the_step_says() {
        // The main menu has no circle.
        let mut screen = MergeScreen::open(4, &mut game(&[5, 6], &[]));
        screen.pass(&Pad::default());
        assert_eq!(screen.pass(&press(pad::CIRCLE)).looks, None);
        // A free place under the cursor of a first pick: nothing.
        let mut screen = taking(&mut game(&[5, 6], &[]));
        go(&mut screen, pad::DOWN);
        go(&mut screen, pad::DOWN);
        assert_eq!(screen.pass(&press(pad::CIRCLE)).looks, None);
        assert_eq!(screen.form, None);
        // On a creature the look starts, of the place under the cursor.
        go(&mut screen, pad::UP);
        assert_eq!(screen.pass(&press(pad::CIRCLE)).looks, Some(1));
        screen.pass(&Pad::default());
        screen.pass(&press(pad::TRIANGLE));
        assert_eq!((screen.state, screen.form), (state::TAKE, None));
        // Behind the pick the place it changes with is the one that is shown.
        go(&mut screen, pad::CROSS);
        assert_eq!(screen.state, state::EXCHANGE);
        let looks = screen.pass(&press(pad::CIRCLE)).looks;
        assert_eq!(looks, Some(screen.shown_place()));
        assert_eq!(looks, Some(screen.second));
        screen.pass(&Pad::default());
        assert!(screen.pass(&press(pad::TRIANGLE)).looked);
        assert_eq!(screen.said, Said::State(state::EXCHANGE));
        // On a page of the creature's box there is no look.
        let mut screen = viewing(&mut game(&[5, 6], &[]));
        go(&mut screen, pad::SQUARE);
        assert_eq!(screen.state, state::ABILITIES);
        assert_eq!(screen.pass(&press(pad::CIRCLE)).looks, None);
    }

    #[test]
    fn the_square_closes_the_gaps_of_the_kept() {
        let mut game = game(&[], &[6, 7]);
        let mut screen = taking(&mut game);
        // The first of the kept goes with him: a gap is left in front of the second.
        for _ in 0..3 {
            go(&mut screen, pad::DOWN);
        }
        go(&mut screen, pad::CROSS);
        for _ in 0..3 {
            go(&mut screen, pad::UP);
        }
        go(&mut screen, pad::CROSS);
        assert_eq!(listed(&screen), [(0, CARRIED, 6), (4, KEPT, 7)]);
        let passed = go(&mut screen, pad::SQUARE);
        assert_eq!(
            (passed.rearranged, passed.sounds),
            (true, vec![SOUND_CONFIRM])
        );
        assert_eq!(listed(&screen), [(0, CARRIED, 6), (3, KEPT, 7)]);
        assert!(!go(&mut screen, pad::SQUARE).rearranged);
    }
}
