
use crate::battle::using::{aim, row};
use crate::menu::{self, Cursor, cursor};
use crate::pad::{self, Pad};
use crate::rooms::{Image, RoomTableError, step};

/// The archive file of the menu and the RAM address of its first byte.
pub const MENU_FILE: usize = 42;
pub const MENU_FILE_BASE: u32 = 0x800B_6890;

/// The screens of the menu's tables.
pub const SCREENS: usize = 17;
/// The first level, the player's entries and a carried creature's entries.
pub const FIRST_LEVEL: i16 = 0;
pub const OF_PLAYER: i16 = 6;
pub const OF_CREATURE: i16 = 7;
/// The player's items, and who gets the one under the cursor.
pub const ITEMS: i16 = 1;
pub const TARGET: i16 = 2;
/// The things of the story, and the question whether to leave the forest.
pub const STORY: i16 = 14;
pub const LEAVE_FOREST: i16 = 16;
/// The spells a carried creature knows, and its five body attacks.
pub const MAGIC: i16 = 4;
pub const SPECIAL: i16 = 11;
/// The caught creatures that wait, behind the player's first entry.
pub const CAUGHT: i16 = 13;
/// What the player has on, behind his fourth entry: the screen of overlay number 18.
pub const EQUIP: i16 = 10;
/// The options, behind the third row of the first level: the screen of overlay number
/// 15 (`crate::options_screen`).
pub const OPTIONS: i16 = 8;
/// How many caught creatures can wait, and how many a row of their screen shows.
pub const CAUGHT_RECORDS: i32 = 12;
pub const CAUGHT_ACROSS: i32 = 3;

/// Passes of the menu's task in a second: a game frame of the field.
pub const PASSES_A_SECOND: u32 = 25;

/// The openness at which a screen is whole and listens to confirm and cancel.
pub const OPEN: i16 = 8;
/// The passes a message stands when no button goes down.
pub const MESSAGE_PASSES: i16 = 150;
/// The result of the player's first entry while no caught creature waits.
pub const NOTHING_CAUGHT: i16 = 9;
/// The answer of a screen's test for an entry that is nothing but its panel.
const ONLY_A_PANEL: i16 = 15;
/// The results of the items' screens: the place is empty, the item cannot be used
/// here, it is for the creatures, creatures cannot use it, this creature cannot fight
/// now, there is no need to use it.
pub const NO_ITEM: i16 = 1;
pub const NOT_HERE: i16 = 2;
pub const FOR_CREATURES: i16 = 10;
pub const NOT_FOR_CREATURES: i16 = 11;
pub const CANNOT_FIGHT: i16 = 12;
pub const NO_NEED: i16 = 13;
/// The result of a confirm on a place without a thing of the story.
pub const NO_THING: i16 = 3;
/// The result that shows a thing's own text.
pub const OWN_TEXT: i16 = 14;
/// The things of the story that have a name, and the first of those whose use shows
/// their own text (`0x8001D3D4`: the things 25 to 37).
pub const STORY_THINGS: i16 = 38;
const FIRST_WITH_A_TEXT: i16 = 25;
/// The answer the cursor of the question starts on: the no.
const NO: i16 = 1;
/// The entry the cursor starts on in screens 6 and 7.
const FIRST_ENTRY: i16 = 2;
/// The screens a confirm can lead to that are built.
const BUILT: [i16; 10] = [
    OF_PLAYER,
    OF_CREATURE,
    ITEMS,
    TARGET,
    STORY,
    MAGIC,
    SPECIAL,
    CAUGHT,
    EQUIP,
    OPTIONS,
];

/// The sounds of the menu: a refused confirm, a cancel, a confirm, a moved cursor.
pub const SOUND_REFUSED: u8 = 124;
pub const SOUND_CANCEL: u8 = 125;
pub const SOUND_CONFIRM: u8 = 126;
pub const SOUND_CURSOR: u8 = 127;
/// The sound of an item that is used.
pub const SOUND_USED: u8 = 68;
/// The bit of an item's flags that has it used from the list, without a target.
pub const WITHOUT_TARGET: u16 = 0x20;
/// The rows of the items' list.
pub const ITEM_ROWS: i16 = 4;

/// Bits of the task's flag word.
pub mod flag {
    /// The cursor of the first level is in the player's row, the creatures' row, the
    /// third row.
    pub const PLAYER_ROW: u16 = 0x01;
    pub const CREATURES_ROW: u16 = 0x02;
    pub const THIRD_ROW: u16 = 0x04;
    /// The menu leaves.
    pub const LEAVES: u16 = 0x08;
    /// The screen of another overlay is up.
    pub const OVERLAY: u16 = 0x20;
}

/// Where the tables lie in the file.
const CURSORS: u32 = 0x800C_1BD4;
const CONFIRM: u32 = 0x800C_1B48;
const CANCEL: u32 = 0x800C_1B6C;
const PANELS: u32 = 0x800C_1B90;
const KEPT: u32 = 0x800C_1D6C;
const BACK: u32 = 0x800C_1D90;
const PLAYER_SCREENS: u32 = 0x800C_1B34;
const CREATURE_SCREENS: u32 = 0x800C_1B40;
const PLAYER_PANELS: u32 = 0x800C_1DB4;
const CREATURE_PANELS: u32 = 0x800C_1DC8;

/// The tables the task walks its screens by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tables {
    /// Columns, rows, lines and flag word of a screen's cursor.
    pub cursors: [[i16; 4]; SCREENS],
    /// The screen a confirm leads to, or a negative way of its own.
    pub confirm: [i16; SCREENS],
    /// The screen a cancel leads to, or -1.
    pub cancel: [i16; SCREENS],
    /// The panels of a screen, one bit a panel.
    pub panels: [u32; SCREENS],
    /// The kept cursor a screen writes in every pass and reads back after a cancel.
    pub kept: [i16; SCREENS],
    pub back: [i16; SCREENS],
    /// The screens behind the player's five entries and a creature's four.
    pub of_player: [i16; 5],
    pub of_creature: [i16; 4],
    /// The panels of screens 6 and 7 by the cursor.
    pub player_panels: [u32; 5],
    pub creature_panels: [u32; 4],
}

impl Tables {
    /// Reads the tables from the menu's file as it is on the disc.
    pub fn from_file(file: &[u8]) -> Result<Self, RoomTableError> {
        let image = Image {
            data: file,
            base: MENU_FILE_BASE,
        };
        let halves = |at: u32, n: usize| image.i16(step(at, n, 2)?);
        let words = |at: u32, n: usize| image.u32(step(at, n, 4)?);
        let mut tables = Self {
            cursors: [[0; 4]; SCREENS],
            confirm: [0; SCREENS],
            cancel: [0; SCREENS],
            panels: [0; SCREENS],
            kept: [0; SCREENS],
            back: [0; SCREENS],
            of_player: [0; 5],
            of_creature: [0; 4],
            player_panels: [0; 5],
            creature_panels: [0; 4],
        };
        for screen in 0..SCREENS {
            for (n, value) in tables.cursors[screen].iter_mut().enumerate() {
                *value = halves(CURSORS, 8 * screen + n)?;
            }
            tables.confirm[screen] = halves(CONFIRM, screen)?;
            tables.cancel[screen] = halves(CANCEL, screen)?;
            tables.panels[screen] = words(PANELS, screen)?;
            tables.kept[screen] = halves(KEPT, screen)?;
            tables.back[screen] = halves(BACK, screen)?;
        }
        for n in 0..5 {
            tables.of_player[n] = halves(PLAYER_SCREENS, n)?;
            tables.player_panels[n] = words(PLAYER_PANELS, n)?;
        }
        for n in 0..4 {
            tables.of_creature[n] = halves(CREATURE_SCREENS, n)?;
            tables.creature_panels[n] = words(CREATURE_PANELS, n)?;
        }
        Ok(tables)
    }
}

/// What the screens of the items ask of the game that keeps them.
pub trait Things {
    /// The item at a place of the player's list and how many of it lie there
    /// (`0x800AD458`, `0x800AD498`).
    fn item_at(&self, place: i16) -> u8;
    fn count_at(&self, place: i16) -> u8;
    /// The last place in use of the list (`0x800AD4D8`).
    fn last_place(&self) -> i16;
    /// The item's word of flags (`0x8001CED8`).
    fn flags(&self, item: u8) -> u16;
    /// The test of a row outside a fight (`0x80033B58`), an answer of
    /// [`crate::battle::using::row`].
    fn row_test(&self, place: i16) -> i16;
    /// The test of a target (`0x80033E50`), an answer of [`crate::battle::using::aim`]:
    /// target 0 is the player, n a carried creature.
    fn aim_test(&self, item: u8, target: i16) -> i16;
    /// The list is put in the order of the items' numbers (`0x800AD5C0`).
    fn tidy(&mut self);
    /// An item is used from the list (`0x800BF5A4`) or on a target (`0x800BF5D8`),
    /// and one piece of it is taken.
    fn use_alone(&mut self, item: u8);
    fn use_on(&mut self, item: u8, target: i16);
    /// How many things of the story the player holds (`0x800AD96C`), the n-th of
    /// them, [`STORY_THINGS`] for none (`0x800AD9C8`), and whether he holds one
    /// (`0x800AD90C`). A game without any is the default.
    fn story_things(&self) -> i16 {
        0
    }
    fn story_thing_at(&self, _n: i16) -> i16 {
        STORY_THINGS
    }
    fn has_story_thing(&self, _thing: i16) -> bool {
        false
    }
    /// Whether a thing of the story can be used where the player stands: thing 0 in
    /// a forest (`0x8003419C`), any other when an object of the room waits for it
    /// (`0x800A5E10`).
    fn thing_fits(&self, _thing: i16) -> bool {
        false
    }
    /// Whether the menu may use a thing at all (`0x800C11F8`): not when the file's
    /// other task opened it while a script holds the game.
    fn free_to_use(&self) -> bool {
        true
    }
    /// A thing of the story is handed to the script that waits for it (`0x800A91FC`).
    fn hand_over(&mut self, _thing: i16) {}
    /// How many spells a carried creature knows (`0x8001AE54` on its record).
    fn spells_known(&self, _creature: i16) -> i16 {
        0
    }
    /// Whether the room is in a forest (`0x8003419C`).
    fn in_a_forest(&self) -> bool {
        false
    }
}

/// Whether the use of a thing of the story shows its own text (`0x8001D3D4`).
pub fn shows_own_text(thing: i16) -> bool {
    (FIRST_WITH_A_TEXT..STORY_THINGS).contains(&thing)
}

/// Whether a row of the things of the story is open (`0x800C0E34`): a thing with a
/// text of its own always, any other when it fits the place and the menu may use it.
pub fn thing_usable(things: &dyn Things, thing: i16) -> bool {
    shows_own_text(thing) || things.thing_fits(thing) && things.free_to_use()
}

/// A game that keeps no item: every place of the list is empty.
#[derive(Debug, Clone, Copy, Default)]
pub struct Nothing;

impl Things for Nothing {
    fn item_at(&self, _place: i16) -> u8 {
        crate::items::FREE
    }

    fn count_at(&self, _place: i16) -> u8 {
        0
    }

    fn last_place(&self) -> i16 {
        0
    }

    fn flags(&self, _item: u8) -> u16 {
        0
    }

    fn row_test(&self, _place: i16) -> i16 {
        row::NO_PIECE
    }

    fn aim_test(&self, _item: u8, _target: i16) -> i16 {
        aim::FINE
    }

    fn tidy(&mut self) {}

    fn use_alone(&mut self, _item: u8) {}

    fn use_on(&mut self, _item: u8, _target: i16) {}
}

/// An item that a pass used, and on whom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Used {
    pub item: u8,
    pub target: Option<i16>,
}

/// What a pass of the menu did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Passed {
    /// The sounds asked for, in their order.
    pub sounds: Vec<u8>,
    /// A confirm would have led to this screen, which is not built: nothing changed.
    pub not_built: Option<i16>,
    /// The pass called the start of this screen's overlay: the menu waits from now on.
    pub starts: Option<i16>,
    /// An item was used.
    pub used: Option<Used>,
    /// A thing of the story was handed to the script that waits for it.
    pub handed: Option<i16>,
    /// The question whether to leave the forest was answered with yes.
    pub leaves_forest: bool,
    /// The task has the routine of the list of the caught kinds from the next pass on
    /// (`crate::caught_list::KindList`), until that one gives the menu's back.
    pub lists: bool,
}

/// The record of the menu's task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerMenu {
    pub flags: u16,
    pub screen: i16,
    /// The screen the last confirm was made on.
    pub confirmed_on: i16,
    /// The result of a refused confirm, 0 for none.
    pub result: i16,
    /// The creatures the player carries (the game keeps one more than that).
    pub carried: i16,
    /// The caught creatures that wait.
    pub caught: i32,
    /// The panels the screen asks for and the ones that stand fully open.
    pub panels: u32,
    pub stands: u32,
    confirm: bool,
    cancel: bool,
    /// A message is up, and the passes it has been.
    pub message: bool,
    pub message_passes: i16,
    /// The result whose message the last pass drew, 0 for none.
    pub said: i16,
    pub openness: i16,
    /// The eleven kept cursors and the four kept first lines.
    pub kept: [i16; 11],
    pub kept_first: [i16; 4],
    pub cursor: Cursor,
}

/// The kept cursors by what they are of.
const KEPT_ENTRY: usize = 0;
const KEPT_CREATURE: usize = 1;
const KEPT_CREATURE_ENTRY: usize = 2;
const KEPT_FIGHTER: usize = 6;
const KEPT_FIRST: usize = 3;
/// The cursor of the items' list (+48), its first line (+70) and the target (+52).
const KEPT_ITEM: usize = 3;
const KEPT_TARGET: usize = 5;
const FIRST_ITEM: usize = 0;
/// The cursor of the things of the story (+50) and its first line (+72).
const KEPT_THING: usize = 4;
const FIRST_THING: usize = 1;
/// The cursor of a creature's spells (+56) with its first line (+74), and the cursor
/// of its body attacks (+62).
const KEPT_SPELL: usize = 7;
const FIRST_SPELL: usize = 2;
const KEPT_ATTACK: usize = 10;
/// The first row of the caught creatures' screen (+60).
const KEPT_CAUGHT: usize = 9;

/// The rows the caught creatures' screen has for a count of them (`0x800BFC78`,
/// `0x800C03EC`): three across, the last row as full as it gets.
pub fn caught_rows(caught: i32) -> i32 {
    (caught + CAUGHT_ACROSS - 1) / CAUGHT_ACROSS
}

impl PlayerMenu {
    /// The menu as its task starts it: on the first level, in the player's row, closed.
    pub fn open(tables: &Tables, carried: i16, caught: i32) -> Self {
        let mut menu = Self {
            flags: flag::PLAYER_ROW,
            screen: FIRST_LEVEL,
            confirmed_on: 0,
            result: 0,
            carried,
            caught,
            panels: tables.panels[FIRST_LEVEL as usize],
            stands: 0,
            confirm: false,
            cancel: false,
            message: false,
            message_passes: 0,
            said: 0,
            openness: 0,
            kept: [0; 11],
            kept_first: [0; 4],
            cursor: Cursor {
                columns: 0,
                rows: 0,
                lines: 0,
                flags: 0,
                at: 0,
                first: 0,
            },
        };
        menu.take_cursor(tables);
        menu
    }

    /// Whether the menu has said it leaves: its task goes on with the way out.
    pub fn leaves(&self) -> bool {
        self.flags & flag::LEAVES != 0
    }

    /// The panels a drawing of the menu shows: on screens 6 and 7 they go by the cursor.
    pub fn shown(&self, tables: &Tables) -> u32 {
        let at = usize::try_from(self.cursor.at).ok();
        let by_cursor = |panels: &[u32]| at.and_then(|at| panels.get(at)).copied();
        match self.screen {
            OF_PLAYER => by_cursor(&tables.player_panels).unwrap_or(0),
            OF_CREATURE => by_cursor(&tables.creature_panels).unwrap_or(0),
            _ => self.panels,
        }
    }

    /// The entry of the player's row and of a creature's row as the screen last kept it
    /// (+42 and +46): the cursor the rows of entries are drawn with.
    pub fn entry_of_player(&self) -> i16 {
        self.kept[KEPT_ENTRY]
    }

    pub fn entry_of_creature(&self) -> i16 {
        self.kept[KEPT_CREATURE_ENTRY]
    }

    /// The carried creature the first level's cursor was last kept on (+44): the one
    /// the screens behind a creature's row are about.
    pub fn carried_creature(&self) -> i16 {
        self.kept[KEPT_CREATURE]
    }

    /// The place of the list the items' cursor was last kept on (+48 and +70), its
    /// first line, and the target of screen 2 (+52: 0 the player, n a carried creature).
    pub fn item_place(&self) -> i16 {
        self.kept[KEPT_ITEM] + self.kept_first[FIRST_ITEM]
    }

    pub fn first_item(&self) -> i16 {
        self.kept_first[FIRST_ITEM]
    }

    pub fn target(&self) -> i16 {
        self.kept[KEPT_TARGET]
    }

    /// The place among the things of the story the cursor was last kept on (+50 and
    /// +72), and the first line of their list.
    pub fn thing_place(&self) -> i16 {
        self.kept[KEPT_THING] + self.kept_first[FIRST_THING]
    }

    pub fn first_thing(&self) -> i16 {
        self.kept_first[FIRST_THING]
    }

    /// The place among the spells a creature knows that the cursor was last kept on
    /// (+56 and +74), the first line of their list, and the row of its body attacks
    /// (+62). Nothing but the menu's start clears them: they stand over a visit of
    /// another creature too.
    pub fn spell_place(&self) -> i16 {
        self.kept[KEPT_SPELL] + self.kept_first[FIRST_SPELL]
    }

    pub fn first_spell(&self) -> i16 {
        self.kept_first[FIRST_SPELL]
    }

    pub fn attack_row(&self) -> i16 {
        self.kept[KEPT_ATTACK]
    }

    /// The first row the caught creatures' screen shows, as it was last kept (+60).
    pub fn caught_row(&self) -> i16 {
        self.kept[KEPT_CAUGHT]
    }

    /// The openness a panel is drawn at: whole once it stands, else the screen's.
    pub fn openness_of(&self, panel: u32) -> i16 {
        match self.stands & panel != 0 {
            true => OPEN,
            false => self.openness,
        }
    }

    /// Whether the menu waits for the screen of another overlay: behind "Equip" its
    /// pass draws the row of the player's entries and nothing else (`0x800C00B8`),
    /// behind the options nothing at all (`0x800C0130`).
    pub fn waits(&self) -> bool {
        self.flags & flag::OVERLAY != 0
    }

    /// The screen of the other overlay has ended (`0x800C017C`): the menu goes back as
    /// a cancel does, with the panels of the screen it comes back to standing. The
    /// openness is left where the start of the overlay found it.
    pub fn overlay_ended(&mut self, tables: &Tables, things: &dyn Things) {
        self.flags &= !flag::OVERLAY;
        let back = tables.cancel[self.screen as usize];
        if back != -1 {
            self.screen = back;
        }
        self.panels = tables.panels[self.screen as usize];
        self.stands = self.panels;
        self.take_cursor(tables);
        self.list_cursor(things);
        self.read_back(tables);
    }

    /// One pass of the task with the pad's words of the pass.
    pub fn pass(&mut self, tables: &Tables, things: &mut dyn Things, pad: &Pad) -> Passed {
        let mut passed = Passed::default();
        if self.waits() {
            return passed;
        }
        // On the screen of an overlay the pass starts it, and is the menu's last
        // until that screen has ended (`0x800C0010`).
        if self.screen == EQUIP || self.screen == OPTIONS {
            self.flags |= flag::OVERLAY;
            passed.starts = Some(self.screen);
        }
        self.panels = self.shown(tables);
        // The drawing: a panel that is drawn whole stands from then on.
        if self.openness == OPEN {
            self.stands |= self.panels;
        }
        // The message's routine draws its box before it asks whether the message ends.
        self.said = if self.message { self.result } else { 0 };
        if self.message {
            self.pass_message(pad, &mut passed);
        } else {
            self.keep(tables);
            if self.confirm || self.cancel {
                self.act(tables, things, &mut passed);
            }
            if !self.leaves() {
                self.input(things, pad, &mut passed);
            }
        }
        if self.openness < OPEN {
            self.openness += 1;
        }
        // Last in the pass, whatever else it did: the square on the first level of a
        // room in a forest hands the task to the list of the caught kinds.
        if pad.pressed & pad::SQUARE != 0 && self.screen == FIRST_LEVEL && things.in_a_forest() {
            passed.sounds.push(SOUND_CONFIRM);
            passed.lists = true;
        }
        passed
    }

    /// The cursor record of the screen; the first level's is made from its row.
    fn take_cursor(&mut self, tables: &Tables) {
        let [columns, rows, lines, flags] = tables.cursors[self.screen as usize];
        self.cursor.columns = columns;
        self.cursor.rows = rows;
        self.cursor.lines = lines as u16;
        self.cursor.flags = flags as u16;
        if self.screen != FIRST_LEVEL {
            return;
        }
        if self.flags & flag::PLAYER_ROW != 0 {
            self.one_entry(cursor::LEAVES_DOWN);
            self.cursor.at = 0;
        } else if self.flags & flag::CREATURES_ROW != 0 {
            self.creatures_row();
        } else {
            self.one_entry(cursor::LEAVES_UP);
        }
    }

    /// What the cursor records of the lists take from the game: the items' list has
    /// a line for every place up to the last in use, four at least, and stands where
    /// it was kept; the targets are the player and each carried creature; the list of
    /// the things of the story has a line for each that is held, four at least, and
    /// stands where it was kept too; so does the list of a creature's spells, with a
    /// line for each it knows, and the cursor of its body attacks.
    fn list_cursor(&mut self, things: &dyn Things) {
        match self.screen {
            ITEMS => {
                self.cursor.at = self.kept[KEPT_ITEM];
                self.cursor.first = self.kept_first[FIRST_ITEM];
                self.cursor.lines = Self::item_lines(things);
            }
            TARGET => {
                self.cursor.columns = 1;
                self.cursor.rows = self.carried + 1;
                self.cursor.lines = (self.carried + 1) as u16;
                self.cursor.flags = cursor::WRAPS_DOWN;
            }
            STORY => {
                self.cursor.at = self.kept[KEPT_THING];
                self.cursor.first = self.kept_first[FIRST_THING];
                self.cursor.lines = things.story_things().max(ITEM_ROWS) as u16;
            }
            MAGIC => {
                self.cursor.at = self.kept[KEPT_SPELL];
                self.cursor.first = self.kept_first[FIRST_SPELL];
                let known = things.spells_known(self.carried_creature());
                self.cursor.lines = known.max(ITEM_ROWS) as u16;
            }
            SPECIAL => self.cursor.at = self.kept[KEPT_ATTACK],
            // The cursor is the first of the three rows that show: one place while
            // nine or fewer wait, else a place for each row but the last two.
            CAUGHT => {
                if self.caught > 9 {
                    let rows = (caught_rows(self.caught) - 2) as i16;
                    self.cursor.rows = rows;
                    self.cursor.lines = rows as u16;
                }
                self.cursor.at = self.kept[KEPT_CAUGHT];
            }
            _ => {}
        }
    }

    fn item_lines(things: &dyn Things) -> u16 {
        (things.last_place().max(ITEM_ROWS - 1) + 1) as u16
    }

    fn one_entry(&mut self, flags: u16) {
        self.cursor.columns = 1;
        self.cursor.rows = 1;
        self.cursor.lines = 1;
        self.cursor.flags = flags;
    }

    fn creatures_row(&mut self) {
        self.cursor.columns = self.carried;
        self.cursor.rows = 1;
        self.cursor.lines = self.carried as u16;
        self.cursor.flags = cursor::WRAPS_ACROSS | cursor::LEAVES_UP | cursor::LEAVES_DOWN;
        self.cursor.at = self.kept[KEPT_CREATURE];
    }

    /// A step down on the first level. Answers whether the row changed.
    fn down(&mut self) -> bool {
        if self.flags & flag::PLAYER_ROW != 0 {
            match self.carried != 0 {
                true => {
                    self.creatures_row();
                    self.flags |= flag::CREATURES_ROW;
                }
                false => {
                    self.one_entry(cursor::LEAVES_UP);
                    self.flags |= flag::THIRD_ROW;
                }
            }
            self.flags &= !flag::PLAYER_ROW;
            true
        } else if self.flags & flag::CREATURES_ROW != 0 {
            self.kept[KEPT_CREATURE] = self.cursor.at;
            self.one_entry(cursor::LEAVES_UP);
            self.cursor.at = 0;
            self.flags = self.flags & !flag::CREATURES_ROW | flag::THIRD_ROW;
            true
        } else {
            false
        }
    }

    /// A step up on the first level. Answers whether the row changed.
    fn up(&mut self) -> bool {
        if self.flags & flag::CREATURES_ROW != 0 {
            self.one_entry(cursor::LEAVES_DOWN);
            self.cursor.at = 0;
            self.flags = self.flags & !flag::CREATURES_ROW | flag::PLAYER_ROW;
            true
        } else if self.flags & flag::THIRD_ROW != 0 {
            match self.carried != 0 {
                true => {
                    self.creatures_row();
                    self.flags |= flag::CREATURES_ROW;
                }
                false => {
                    self.one_entry(cursor::LEAVES_DOWN);
                    self.flags |= flag::PLAYER_ROW;
                }
            }
            self.flags &= !flag::THIRD_ROW;
            true
        } else {
            false
        }
    }

    /// The cursor is written to where the screen keeps it, in every pass.
    fn keep(&mut self, tables: &Tables) {
        let kept = usize::try_from(tables.kept[self.screen as usize]).ok();
        if let Some(place) = kept.and_then(|place| self.kept.get_mut(place)) {
            *place = self.cursor.at;
        }
        if self.screen == ITEMS {
            self.kept[KEPT_TARGET] = 0;
            self.kept_first[FIRST_ITEM] = self.cursor.first;
        }
        if self.screen == STORY {
            self.kept_first[FIRST_THING] = self.cursor.first;
        }
        if self.screen == MAGIC {
            self.kept_first[FIRST_SPELL] = self.cursor.first;
        }
        if self.screen != FIRST_LEVEL {
            return;
        }
        if self.flags & flag::PLAYER_ROW != 0 {
            self.kept_first[KEPT_FIRST] = self.cursor.first;
            self.kept[KEPT_FIGHTER] = 0;
        } else if self.flags & flag::CREATURES_ROW != 0 {
            self.kept[KEPT_CREATURE] = self.cursor.at;
            self.kept[KEPT_FIGHTER] = self.cursor.at + 1;
            self.kept_first[KEPT_FIRST] = self.cursor.first;
        } else {
            self.kept[KEPT_ENTRY] = self.cursor.at;
        }
    }

    /// The screen's test of a confirm: 0 lets it pass.
    fn test(&self, things: &dyn Things) -> i16 {
        let place = self.item_place();
        match (self.screen, self.cursor.at) {
            // A spell cannot be picked here: the whole screen is its panel.
            (OF_PLAYER, 2) | (OF_CREATURE, 2 | 3) | (MAGIC, _) => ONLY_A_PANEL,
            (OF_PLAYER, 0) if self.caught == 0 => NOTHING_CAUGHT,
            (ITEMS, _) => match things.row_test(place) {
                row::NO_PIECE => NO_ITEM,
                row::NEVER | row::NOT_HERE => NOT_HERE,
                _ => 0,
            },
            (TARGET, _) if things.count_at(place) == 0 => NO_ITEM,
            (TARGET, _) => match things.aim_test(things.item_at(place), self.target()) {
                aim::NOT_FOR_THE_PLAYER => FOR_CREATURES,
                aim::NOT_FOR_CREATURES => NOT_FOR_CREATURES,
                aim::DEAD => CANNOT_FIGHT,
                aim::NO_NEED => NO_NEED,
                _ => 0,
            },
            (STORY, _) => {
                let thing = things.story_thing_at(self.thing_place());
                if !things.has_story_thing(thing) {
                    NO_THING
                } else if shows_own_text(thing) {
                    OWN_TEXT
                } else if things.thing_fits(thing) && things.free_to_use() {
                    0
                } else {
                    NOT_HERE
                }
            }
            _ => 0,
        }
    }

    /// The screen a confirm leads to, for the screens that are built.
    fn behind(&self, tables: &Tables) -> Option<i16> {
        let at = usize::try_from(self.cursor.at).ok()?;
        match self.screen {
            FIRST_LEVEL if self.flags & flag::PLAYER_ROW != 0 => Some(OF_PLAYER),
            FIRST_LEVEL if self.flags & flag::CREATURES_ROW != 0 => Some(OF_CREATURE),
            FIRST_LEVEL => Some(8),
            OF_PLAYER => tables.of_player.get(at).copied(),
            OF_CREATURE => tables.of_creature.get(at).copied(),
            _ => None,
        }
    }

    /// A noted confirm or cancel is acted on, a pass after it was noted.
    fn act(&mut self, tables: &Tables, things: &mut dyn Things, passed: &mut Passed) {
        let mut result = 0;
        if self.confirm {
            result = self.test(things);
            let item = things.item_at(self.item_place());
            let alone = things.flags(item) & WITHOUT_TARGET != 0;
            if result == 0 {
                let behind = match self.screen {
                    ITEMS => Some(TARGET),
                    TARGET | STORY | LEAVE_FOREST | SPECIAL | CAUGHT => None,
                    _ => self.behind(tables),
                };
                let thing = things.story_thing_at(self.thing_place());
                match (self.screen, behind) {
                    // Thing 0 asks first: the question comes up whole, on its no.
                    (STORY, _) if thing == 0 => {
                        self.confirmed_on = self.screen;
                        passed.sounds.push(SOUND_CONFIRM);
                        self.screen = LEAVE_FOREST;
                        self.cursor.at = NO;
                    }
                    (STORY, _) => {
                        self.confirmed_on = self.screen;
                        passed.sounds.push(SOUND_USED);
                        self.flags |= flag::LEAVES;
                        things.hand_over(thing);
                        passed.handed = Some(thing);
                    }
                    (LEAVE_FOREST, _) => {
                        self.confirmed_on = self.screen;
                        passed.sounds.push(SOUND_USED);
                        self.flags |= flag::LEAVES;
                        passed.leaves_forest = true;
                    }
                    (ITEMS, _) if alone => {
                        self.confirmed_on = self.screen;
                        things.use_alone(item);
                        passed.sounds.push(SOUND_USED);
                        passed.used = Some(Used { item, target: None });
                    }
                    (TARGET, _) => {
                        self.confirmed_on = self.screen;
                        let target = self.target();
                        passed.sounds.push(SOUND_USED);
                        things.use_on(item, target);
                        passed.used = Some(Used {
                            item,
                            target: Some(target),
                        });
                    }
                    // A body attack cannot be picked either: the confirm passes the
                    // test and leads nowhere, without a sound. Nor can a caught
                    // creature.
                    (SPECIAL | CAUGHT, _) => {}
                    (_, Some(screen)) if BUILT.contains(&screen) => {
                        self.confirmed_on = self.screen;
                        passed.sounds.push(SOUND_CONFIRM);
                        self.screen = screen;
                        self.cursor.at = 0;
                        self.cursor.first = 0;
                        self.openness = 0;
                    }
                    (_, other) => {
                        passed.not_built = other;
                        self.confirm = false;
                        self.cancel = false;
                        return;
                    }
                }
            } else if result != ONLY_A_PANEL {
                let sound = match result == OWN_TEXT {
                    true => SOUND_CONFIRM,
                    false => SOUND_REFUSED,
                };
                passed.sounds.push(sound);
                self.result = result;
                self.message = true;
            }
        } else if self.screen == FIRST_LEVEL {
            self.flags |= flag::LEAVES;
        } else {
            let back = tables.cancel[self.screen as usize];
            if back != -1 {
                self.screen = back;
            }
            self.take_cursor(tables);
            self.list_cursor(things);
            self.read_back(tables);
        }
        let stays = [ONLY_A_PANEL, NOTHING_CAUGHT, OWN_TEXT].contains(&result);
        if !self.leaves() && !stays {
            if self.cancel {
                // Nothing opens on the way back: every panel of the screen stands.
                self.panels = match self.screen {
                    OF_PLAYER | OF_CREATURE => 0,
                    screen => tables.panels[screen as usize],
                };
                self.stands = self.panels;
            } else {
                // A panel the new screen shares with the old one does not open again.
                self.panels = tables.panels[self.screen as usize];
                self.stands &= self.panels;
                self.take_cursor(tables);
                self.list_cursor(things);
                match self.screen {
                    OF_PLAYER => self.kept[KEPT_ENTRY] = FIRST_ENTRY,
                    OF_CREATURE => self.kept[KEPT_CREATURE_ENTRY] = FIRST_ENTRY,
                    _ => {}
                }
                if matches!(self.screen, OF_PLAYER | OF_CREATURE) {
                    self.cursor.at = FIRST_ENTRY;
                }
            }
        }
        self.confirm = false;
        self.cancel = false;
    }

    /// The cursor is read back from where the screen kept it, after a cancel.
    fn read_back(&mut self, tables: &Tables) {
        let back = usize::try_from(tables.back[self.screen as usize]).ok();
        if let Some(kept) = back.and_then(|place| self.kept.get(place)) {
            self.cursor.at = *kept;
            self.cursor.first = 0;
        }
        match self.screen {
            FIRST_LEVEL => {
                self.cursor.at = if self.flags & flag::PLAYER_ROW != 0 {
                    0
                } else if self.flags & flag::CREATURES_ROW != 0 {
                    self.kept[KEPT_CREATURE]
                } else {
                    self.kept[KEPT_ENTRY]
                };
                self.cursor.first = self.kept_first[KEPT_FIRST];
            }
            OF_PLAYER => {
                if self.flags & flag::PLAYER_ROW != 0 {
                    self.cursor.at = self.kept[KEPT_ENTRY];
                } else if self.flags & flag::CREATURES_ROW != 0 {
                    self.cursor.at = self.kept[KEPT_CREATURE_ENTRY];
                }
                self.cursor.first = 0;
            }
            ITEMS => {
                self.cursor.first = self.kept_first[FIRST_ITEM];
                self.kept[KEPT_TARGET] = 0;
            }
            STORY => self.cursor.first = self.kept_first[FIRST_THING],
            _ => {}
        }
    }

    /// The pad is read: the cursor in every pass, confirm and cancel once the screen is
    /// whole. On the items' list the square puts the list in order.
    fn input(&mut self, things: &mut dyn Things, pad: &Pad, passed: &mut Passed) {
        let moved = self.cursor.step(pad.repeated);
        if moved.sounds {
            passed.sounds.push(SOUND_CURSOR);
        }
        if self.screen == FIRST_LEVEL {
            let changed = match moved.answer {
                4 => self.down(),
                3 => self.up(),
                _ => false,
            };
            if changed {
                passed.sounds.push(SOUND_CURSOR);
            }
        }
        if self.openness < OPEN {
            return;
        }
        if menu::confirmed(pad) {
            // The no of the question is a cancel.
            if self.screen == LEAVE_FOREST && self.cursor.at == NO {
                passed.sounds.push(SOUND_CANCEL);
                self.cancel = true;
            } else {
                self.confirm = true;
            }
        } else if menu::cancelled(pad) {
            passed.sounds.push(SOUND_CANCEL);
            self.cancel = true;
        } else if self.screen == ITEMS && pad.pressed & pad::SQUARE != 0 {
            things.tidy();
            passed.sounds.push(SOUND_CONFIRM);
            self.cursor.at = 0;
            self.cursor.first = 0;
            self.cursor.lines = Self::item_lines(things);
        }
    }

    /// A message stands until any button goes down or its passes are over; a thing's
    /// own text until a confirm or a cancel, however long that takes.
    fn pass_message(&mut self, pad: &Pad, passed: &mut Passed) {
        if self.message_passes < MESSAGE_PASSES {
            self.message_passes += 1;
        }
        let ends = match self.result == OWN_TEXT {
            true => {
                let turned = menu::confirmed(pad) || menu::cancelled(pad);
                if turned {
                    passed.sounds.push(SOUND_CANCEL);
                }
                turned
            }
            false => pad.pressed != 0 || self.message_passes >= MESSAGE_PASSES,
        };
        if ends {
            self.result = 0;
            self.message = false;
            self.message_passes = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pad::Repeat;

    /// Tables with the rows of the three screens that are built.
    fn tables() -> Tables {
        let mut tables = Tables {
            cursors: [[1, 1, 1, 0]; SCREENS],
            confirm: [-2; SCREENS],
            cancel: [-1; SCREENS],
            panels: [0; SCREENS],
            kept: [-1; SCREENS],
            back: [-1; SCREENS],
            of_player: [13, 1, 6, 10, 14],
            of_creature: [11, 4, 7, 7],
            player_panels: [0x421, 0x821, 0x21, 0x1021, 0x2021],
            creature_panels: [0x1C0, 0xD0, 0xC0, 0x2C0],
        };
        tables.cursors[0] = [1, 1, 1, 0x20];
        tables.cursors[6] = [5, 1, 5, 0x01];
        tables.cursors[7] = [4, 1, 4, 0x01];
        (tables.confirm[0], tables.confirm[6], tables.confirm[7]) = (-1, -1, -1);
        (tables.cancel[0], tables.cancel[6], tables.cancel[7]) = (-2, 0, 0);
        (tables.panels[0], tables.panels[6], tables.panels[7]) = (0x06, 0x21, 0xC0);
        (tables.kept[6], tables.kept[7], tables.back[7]) = (0, 2, 2);
        tables.cursors[1] = [1, 4, 16, 0x40];
        tables.cursors[2] = [1, 4, 4, 0x02];
        (tables.confirm[1], tables.confirm[2]) = (2, -3);
        (tables.cancel[1], tables.cancel[2]) = (6, 1);
        (tables.panels[1], tables.panels[2]) = (0x801, 0x8801);
        (tables.kept[1], tables.back[1]) = (3, 3);
        (tables.kept[2], tables.back[2]) = (5, 5);
        tables.cursors[14] = [1, 4, 16, 0x40];
        tables.cursors[16] = [2, 1, 2, 0];
        (tables.confirm[14], tables.confirm[16]) = (-3, -3);
        (tables.cancel[14], tables.cancel[16]) = (6, 14);
        (tables.panels[14], tables.panels[16]) = (0x2001, 0x12001);
        (tables.kept[14], tables.back[14]) = (4, 4);
        (tables.kept[16], tables.back[16]) = (8, 4);
        tables.cursors[4] = [1, 4, 16, 0x40];
        tables.cursors[11] = [1, 5, 5, 0];
        (tables.cancel[4], tables.cancel[11]) = (7, 7);
        (tables.panels[4], tables.panels[11]) = (0x90, 0x180);
        (tables.kept[4], tables.back[4]) = (7, 7);
        (tables.kept[11], tables.back[11]) = (10, 10);
        tables.cursors[13] = [1, 1, 1, 0x40];
        (tables.cancel[13], tables.panels[13], tables.kept[13]) = (6, 0x401, 9);
        (tables.cancel[10], tables.panels[10]) = (6, 0x01);
        tables.cancel[8] = 0;
        tables
    }

    /// The game's side of the items' screens: pairs of an item and its count by place,
    /// answers of the two tests that a test of the menu sets, and what was done.
    #[derive(Default)]
    struct Bag {
        pairs: Vec<(u8, u8)>,
        row: i16,
        aim: i16,
        tidied: u32,
        used: Vec<(u8, Option<i16>)>,
        /// The things of the story that are held, whether the place fits them, and
        /// the ones that were handed over.
        story: Vec<i16>,
        fits: bool,
        handed: Vec<i16>,
        /// The spells each carried creature knows.
        spells: Vec<i16>,
        /// Whether the room is in a forest.
        forest: bool,
    }

    impl Bag {
        fn take(&mut self, item: u8) {
            let pair = self.pairs.iter_mut().find(|pair| pair.0 == item);
            let pair = pair.expect("the item is in the bag");
            pair.1 -= 1;
            if pair.1 == 0 {
                pair.0 = 255;
            }
        }
    }

    impl Things for Bag {
        fn item_at(&self, place: i16) -> u8 {
            self.pairs.get(place as usize).map_or(255, |pair| pair.0)
        }

        fn count_at(&self, place: i16) -> u8 {
            self.pairs.get(place as usize).map_or(0, |pair| pair.1)
        }

        fn last_place(&self) -> i16 {
            self.pairs
                .iter()
                .rposition(|pair| pair.0 != 255)
                .unwrap_or(0) as i16
        }

        fn flags(&self, item: u8) -> u16 {
            match item {
                41 | 42 => 0x61,
                _ => 0x4B,
            }
        }

        fn row_test(&self, place: i16) -> i16 {
            match self.count_at(place) {
                0 => row::NO_PIECE,
                _ => self.row,
            }
        }

        fn aim_test(&self, _item: u8, _target: i16) -> i16 {
            self.aim
        }

        fn tidy(&mut self) {
            self.tidied += 1;
        }

        fn use_alone(&mut self, item: u8) {
            self.take(item);
            self.used.push((item, None));
        }

        fn use_on(&mut self, item: u8, target: i16) {
            self.take(item);
            self.used.push((item, Some(target)));
        }

        fn story_things(&self) -> i16 {
            self.story.len() as i16
        }

        fn story_thing_at(&self, n: i16) -> i16 {
            (self.story.get(n as usize).copied()).unwrap_or(STORY_THINGS)
        }

        fn has_story_thing(&self, thing: i16) -> bool {
            self.story.contains(&thing)
        }

        fn thing_fits(&self, _thing: i16) -> bool {
            self.fits
        }

        fn hand_over(&mut self, thing: i16) {
            self.handed.push(thing);
        }

        fn spells_known(&self, creature: i16) -> i16 {
            self.spells.get(creature as usize).copied().unwrap_or(0)
        }

        fn in_a_forest(&self) -> bool {
            self.forest
        }
    }

    #[test]
    fn the_square_asks_for_the_list_of_the_caught_kinds_in_a_forest() {
        let tables = tables();
        let mut bag = Bag::default();
        let mut menu = opened(0, 0);
        // Outside a forest the square does nothing on the first level.
        assert_eq!(
            menu.pass(&tables, &mut bag, &press(pad::SQUARE)),
            Passed::default()
        );
        // In a forest it hands the task on, in the first pass already: the test asks
        // neither for the openness nor for anything else of the pass.
        bag.forest = true;
        let mut menu = PlayerMenu::open(&tables, 0, 0);
        let passed = menu.pass(&tables, &mut bag, &press(pad::SQUARE));
        assert_eq!(
            (passed.lists, passed.sounds.as_slice()),
            (true, &[SOUND_CONFIRM][..])
        );
        assert_eq!((menu.screen, menu.openness), (FIRST_LEVEL, 1));
        // Held on, it is not pressed again; and behind an entry it is not asked.
        let held = {
            let mut pad = press(pad::SQUARE);
            pad.read(pad::SQUARE, Repeat::of(2), 2);
            pad
        };
        assert!(!menu.pass(&tables, &mut bag, &held).lists);
        let mut menu = opened(0, 0);
        tap(&mut menu, pad::CROSS);
        assert_eq!(menu.screen, OF_PLAYER);
        assert!(!menu.pass(&tables, &mut bag, &press(pad::SQUARE)).lists);
    }

    /// The menu on the list of the things of the story, whole.
    fn on_the_things(bag: &mut Bag) -> PlayerMenu {
        let tables = tables();
        let mut menu = opened(0, 0);
        tap(&mut menu, pad::CROSS);
        rest(&mut menu);
        menu.pass(&tables, bag, &press(pad::RIGHT));
        menu.pass(&tables, bag, &press(pad::RIGHT));
        assert_eq!((menu.cursor.at, menu.shown(&tables)), (4, 0x2021));
        assert_eq!(tap_with(&mut menu, bag, pad::CROSS).sounds, [SOUND_CONFIRM]);
        assert_eq!((menu.screen, menu.confirmed_on), (STORY, OF_PLAYER));
        for _ in 0..OPEN {
            menu.pass(&tables, bag, &Pad::default());
        }
        menu
    }

    #[test]
    fn a_thing_of_the_story_is_refused_shown_or_handed_over() {
        let tables = tables();
        let mut bag = Bag {
            story: vec![1, 3, 26],
            ..Bag::default()
        };
        let mut menu = on_the_things(&mut bag);
        assert_eq!((menu.cursor.lines, menu.cursor.at), (4, 0));
        assert!(!thing_usable(&bag, 1) && thing_usable(&bag, 26));
        // Nothing of the room waits for thing 1.
        assert_eq!(
            tap_with(&mut menu, &mut bag, pad::CROSS).sounds,
            [SOUND_REFUSED]
        );
        assert_eq!(
            (menu.result, menu.message, menu.screen),
            (NOT_HERE, true, STORY)
        );
        menu.pass(&tables, &mut bag, &press(pad::SQUARE));
        assert!(!menu.message);
        // Thing 26 has a text of its own, which stands until a confirm or a cancel.
        menu.pass(&tables, &mut bag, &press(pad::DOWN));
        menu.pass(&tables, &mut bag, &press(pad::DOWN));
        assert_eq!(menu.thing_place(), 1);
        menu.pass(&tables, &mut bag, &Pad::default());
        assert_eq!(menu.thing_place(), 2);
        assert_eq!(
            tap_with(&mut menu, &mut bag, pad::CROSS).sounds,
            [SOUND_CONFIRM]
        );
        assert_eq!((menu.result, menu.message), (OWN_TEXT, true));
        for _ in 0..2 * MESSAGE_PASSES {
            assert!(
                menu.pass(&tables, &mut bag, &Pad::default())
                    .sounds
                    .is_empty()
            );
        }
        assert_eq!(menu.said, OWN_TEXT);
        assert!(
            menu.pass(&tables, &mut bag, &press(pad::SQUARE))
                .sounds
                .is_empty()
        );
        assert!(menu.message);
        let turned = menu.pass(&tables, &mut bag, &press(pad::TRIANGLE));
        assert_eq!(turned.sounds, [SOUND_CANCEL]);
        assert_eq!((menu.message, menu.result, menu.screen), (false, 0, STORY));
        // The fourth row is empty.
        menu.pass(&tables, &mut bag, &press(pad::DOWN));
        assert_eq!(
            tap_with(&mut menu, &mut bag, pad::CROSS).sounds,
            [SOUND_REFUSED]
        );
        assert_eq!(menu.result, NO_THING);
        menu.pass(&tables, &mut bag, &press(pad::SQUARE));
        // With an object of the room that waits for it a thing is handed over, and
        // the menu leaves.
        bag.fits = true;
        for _ in 0..3 {
            menu.pass(&tables, &mut bag, &press(pad::UP));
        }
        let handed = tap_with(&mut menu, &mut bag, pad::CROSS);
        assert_eq!((handed.sounds, handed.handed), (vec![SOUND_USED], Some(1)));
        assert!(menu.leaves() && !handed.leaves_forest);
        assert_eq!(bag.handed, [1]);
    }

    #[test]
    fn thing_zero_asks_whether_to_leave_the_forest() {
        let tables = tables();
        let mut bag = Bag {
            story: vec![0, 1, 2, 3, 4, 5],
            fits: true,
            ..Bag::default()
        };
        let mut menu = on_the_things(&mut bag);
        assert_eq!(menu.cursor.lines, 6);
        let asked = tap_with(&mut menu, &mut bag, pad::CROSS);
        assert_eq!((asked.sounds, asked.handed), (vec![SOUND_CONFIRM], None));
        // The question is whole at once, on its no, over the list that stays.
        assert_eq!(
            (menu.screen, menu.cursor.at, menu.openness),
            (LEAVE_FOREST, 1, OPEN)
        );
        assert_eq!((menu.panels, menu.stands), (0x12001, 0x2001));
        assert_eq!((menu.cursor.columns, menu.cursor.lines), (2, 2));
        // A confirm on the no is a cancel, with its sound in the pass of the press.
        let noted = menu.pass(&tables, &mut bag, &press(pad::CROSS));
        assert_eq!(noted.sounds, [SOUND_CANCEL]);
        menu.pass(&tables, &mut bag, &Pad::default());
        assert_eq!(
            (menu.screen, menu.cursor.at, menu.thing_place()),
            (STORY, 0, 0)
        );
        assert!(!menu.leaves());
        // The yes leaves the menu and says so.
        tap_with(&mut menu, &mut bag, pad::CROSS);
        menu.pass(&tables, &mut bag, &press(pad::LEFT));
        assert_eq!((menu.screen, menu.cursor.at), (LEAVE_FOREST, 0));
        let left = tap_with(&mut menu, &mut bag, pad::CROSS);
        assert_eq!((left.sounds, left.leaves_forest), (vec![SOUND_USED], true));
        assert!(menu.leaves() && bag.handed.is_empty());
    }

    /// A press and the pass that acts on it, with a bag.
    fn tap_with(menu: &mut PlayerMenu, bag: &mut Bag, buttons: u16) -> Passed {
        let noted = menu.pass(&tables(), bag, &press(buttons));
        let mut acted = menu.pass(&tables(), bag, &Pad::default());
        acted.sounds.splice(0..0, noted.sounds);
        acted
    }

    /// The menu on the list of the items, whole.
    fn on_the_items(carried: i16, bag: &mut Bag) -> PlayerMenu {
        let tables = tables();
        let mut menu = opened(carried, 0);
        tap(&mut menu, pad::CROSS);
        rest(&mut menu);
        menu.pass(&tables, bag, &press(pad::LEFT));
        assert_eq!((menu.cursor.at, menu.shown(&tables)), (1, 0x821));
        assert_eq!(tap_with(&mut menu, bag, pad::CROSS).sounds, [SOUND_CONFIRM]);
        assert_eq!((menu.screen, menu.confirmed_on), (ITEMS, OF_PLAYER));
        for _ in 0..OPEN {
            menu.pass(&tables, bag, &Pad::default());
        }
        menu
    }

    #[test]
    fn the_list_of_the_items_scrolls_and_is_put_in_order() {
        let tables = tables();
        let mut bag = Bag {
            pairs: vec![(9, 2), (255, 0), (0, 1), (3, 1), (255, 0), (1, 5)],
            ..Bag::default()
        };
        let mut menu = on_the_items(0, &mut bag);
        // A line for every place up to the last in use; the panel of screen 6 stays.
        assert_eq!((menu.cursor.rows, menu.cursor.lines), (4, 6));
        assert_eq!((menu.cursor.at, menu.cursor.first), (0, 0));
        assert_eq!((menu.panels, menu.stands), (0x801, 0x801));
        for _ in 0..3 {
            menu.pass(&tables, &mut bag, &press(pad::DOWN));
        }
        assert_eq!((menu.cursor.at, menu.cursor.first), (3, 0));
        menu.pass(&tables, &mut bag, &press(pad::DOWN));
        menu.pass(&tables, &mut bag, &Pad::default());
        assert_eq!((menu.cursor.at, menu.cursor.first), (3, 1));
        assert_eq!((menu.item_place(), menu.first_item()), (4, 1));
        // An empty place is refused with the result of "no item".
        assert_eq!(
            tap_with(&mut menu, &mut bag, pad::CROSS).sounds,
            [SOUND_REFUSED]
        );
        assert_eq!(
            (menu.message, menu.result, menu.screen),
            (true, NO_ITEM, ITEMS)
        );
        menu.pass(&tables, &mut bag, &press(pad::CROSS));
        assert!(!menu.message);
        // The cursor stands where it was behind the message.
        assert_eq!((menu.cursor.at, menu.cursor.first), (3, 1));
        // The square puts the list in order, with the cursor back on its first line.
        let ordered = menu.pass(&tables, &mut bag, &press(pad::SQUARE));
        assert_eq!((ordered.sounds, bag.tidied), (vec![SOUND_CONFIRM], 1));
        assert_eq!((menu.cursor.at, menu.cursor.first), (0, 0));
        // An item that cannot be used here.
        bag.row = row::NOT_HERE;
        tap_with(&mut menu, &mut bag, pad::CROSS);
        assert_eq!((menu.message, menu.result), (true, NOT_HERE));
        menu.pass(&tables, &mut bag, &press(pad::CROSS));
        // The way back keeps the entry, and the list its cursor for the next visit.
        bag.row = row::FINE;
        menu.pass(&tables, &mut bag, &press(pad::DOWN));
        let back = tap_with(&mut menu, &mut bag, pad::TRIANGLE);
        assert_eq!(back.sounds, [SOUND_CANCEL]);
        assert_eq!((menu.screen, menu.cursor.at), (OF_PLAYER, 1));
        tap_with(&mut menu, &mut bag, pad::CROSS);
        assert_eq!((menu.screen, menu.cursor.at, menu.openness), (ITEMS, 1, 1));
    }

    #[test]
    fn an_item_is_used_on_the_one_the_second_screen_picks() {
        let tables = tables();
        let mut bag = Bag {
            pairs: vec![(9, 1), (42, 2)],
            ..Bag::default()
        };
        let mut menu = on_the_items(2, &mut bag);
        assert_eq!(menu.cursor.lines, 4);
        let picked = tap_with(&mut menu, &mut bag, pad::CROSS);
        assert_eq!((picked.sounds, picked.used), (vec![SOUND_CONFIRM], None));
        assert_eq!((menu.screen, menu.confirmed_on), (TARGET, ITEMS));
        // The player and each carried creature, round and round; the list stays up.
        assert_eq!((menu.cursor.rows, menu.cursor.lines), (3, 3));
        assert_eq!((menu.cursor.at, menu.cursor.flags), (0, cursor::WRAPS_DOWN));
        assert_eq!(
            (menu.panels, menu.stands, menu.openness),
            (0x8801, 0x801, 1)
        );
        // The new screen is deaf until it is whole.
        assert_eq!(tap_with(&mut menu, &mut bag, pad::CROSS), Passed::default());
        for _ in 0..OPEN {
            menu.pass(&tables, &mut bag, &Pad::default());
        }
        menu.pass(&tables, &mut bag, &press(pad::UP));
        menu.pass(&tables, &mut bag, &Pad::default());
        assert_eq!((menu.cursor.at, menu.target()), (2, 2));
        // A refusal of the target's test, by its answer.
        for (answer, result) in [
            (aim::NOT_FOR_THE_PLAYER, FOR_CREATURES),
            (aim::NOT_FOR_CREATURES, NOT_FOR_CREATURES),
            (aim::DEAD, CANNOT_FIGHT),
            (aim::NO_NEED, NO_NEED),
        ] {
            bag.aim = answer;
            let refused = tap_with(&mut menu, &mut bag, pad::CROSS);
            assert_eq!((refused.sounds, menu.result), (vec![SOUND_REFUSED], result));
            menu.pass(&tables, &mut bag, &press(pad::CROSS));
            assert_eq!((menu.screen, menu.cursor.at), (TARGET, 2));
        }
        bag.aim = aim::FINE;
        let used = tap_with(&mut menu, &mut bag, pad::CROSS);
        assert_eq!(used.sounds, [SOUND_USED]);
        let on_second = Used {
            item: 9,
            target: Some(2),
        };
        assert_eq!(
            (used.used, &bag.used[..]),
            (Some(on_second), &[(9, Some(2))][..])
        );
        // The screen stays, and with the last piece gone the place is empty.
        assert_eq!(
            (menu.screen, menu.cursor.at, menu.openness),
            (TARGET, 2, OPEN)
        );
        tap_with(&mut menu, &mut bag, pad::CROSS);
        assert_eq!((menu.message, menu.result), (true, NO_ITEM));
        menu.pass(&tables, &mut bag, &press(pad::CROSS));
        // Back on the list the target is the player again.
        tap_with(&mut menu, &mut bag, pad::TRIANGLE);
        assert_eq!((menu.screen, menu.target()), (ITEMS, 0));
        assert_eq!((menu.cursor.at, menu.cursor.first), (0, 0));
        assert_eq!(
            (menu.panels, menu.stands, menu.openness),
            (0x801, 0x801, OPEN)
        );
        // An item with the bit is used from the list itself.
        menu.pass(&tables, &mut bag, &press(pad::DOWN));
        let alone = tap_with(&mut menu, &mut bag, pad::CROSS);
        assert_eq!(alone.sounds, [SOUND_USED]);
        let from_the_list = Used {
            item: 42,
            target: None,
        };
        assert_eq!(alone.used, Some(from_the_list));
        assert_eq!((menu.screen, menu.cursor.at), (ITEMS, 1));
        assert_eq!(bag.pairs[1], (42, 1));
    }

    /// The pad's words of a pass in which the buttons went down.
    fn press(buttons: u16) -> Pad {
        let mut pad = Pad::default();
        pad.read(0, Repeat::of(1), 0);
        pad.read(buttons, Repeat::of(1), 1);
        pad
    }

    /// The menu after the passes that open a screen.
    fn opened(carried: i16, caught: i32) -> PlayerMenu {
        let mut menu = PlayerMenu::open(&tables(), carried, caught);
        rest(&mut menu);
        menu
    }

    fn rest(menu: &mut PlayerMenu) {
        for _ in 0..OPEN {
            menu.pass(&tables(), &mut Bag::default(), &Pad::default());
        }
    }

    /// A press and the pass that acts on it.
    fn tap(menu: &mut PlayerMenu, buttons: u16) -> Passed {
        let noted = menu.pass(&tables(), &mut Bag::default(), &press(buttons));
        let mut acted = menu.pass(&tables(), &mut Bag::default(), &Pad::default());
        acted.sounds.splice(0..0, noted.sounds);
        acted
    }

    #[test]
    fn a_screen_is_deaf_until_it_is_whole_and_acts_a_pass_late() {
        let tables = tables();
        let mut menu = PlayerMenu::open(&tables, 0, 0);
        assert_eq!((menu.flags, menu.screen), (flag::PLAYER_ROW, FIRST_LEVEL));
        for pass in 0..OPEN {
            assert_eq!(menu.openness_of(0x02), pass);
            menu.pass(&tables, &mut Bag::default(), &press(pad::CROSS));
        }
        menu.pass(&tables, &mut Bag::default(), &Pad::default());
        assert_eq!((menu.screen, menu.openness), (FIRST_LEVEL, OPEN));
        assert_eq!(menu.stands, 0x06);
        let noted = menu.pass(&tables, &mut Bag::default(), &press(pad::CROSS));
        assert_eq!((menu.screen, noted.sounds.len()), (FIRST_LEVEL, 0));
        let acted = menu.pass(&tables, &mut Bag::default(), &Pad::default());
        assert_eq!(acted.sounds, [SOUND_CONFIRM]);
        assert_eq!((menu.screen, menu.confirmed_on), (OF_PLAYER, FIRST_LEVEL));
        assert_eq!(menu.openness, 1);
        // The cursor starts on the third entry, the one that leads nowhere.
        assert_eq!((menu.cursor.columns, menu.cursor.lines), (5, 5));
        assert_eq!((menu.cursor.at, menu.kept[KEPT_ENTRY]), (2, 2));
        // No panel of the first level is one of the new screen.
        assert_eq!(menu.stands, 0);
    }

    #[test]
    fn the_rows_of_the_first_level_follow_the_carried_creatures() {
        let tables = tables();
        let mut menu = opened(2, 0);
        let moved = menu.pass(&tables, &mut Bag::default(), &press(pad::DOWN));
        assert_eq!(moved.sounds, [SOUND_CURSOR]);
        assert_eq!(menu.flags, flag::CREATURES_ROW);
        assert_eq!((menu.cursor.columns, menu.cursor.lines), (2, 2));
        assert_eq!(
            menu.pass(&tables, &mut Bag::default(), &press(pad::RIGHT))
                .sounds,
            [SOUND_CURSOR]
        );
        assert_eq!(menu.cursor.at, 1);
        menu.pass(&tables, &mut Bag::default(), &press(pad::RIGHT));
        assert_eq!(menu.cursor.at, 0);
        menu.pass(&tables, &mut Bag::default(), &press(pad::LEFT));
        assert_eq!(menu.cursor.at, 1);
        menu.pass(&tables, &mut Bag::default(), &press(pad::DOWN));
        assert_eq!((menu.flags, menu.cursor.at), (flag::THIRD_ROW, 0));
        assert_eq!(menu.cursor.lines, 1);
        assert_eq!(
            menu.pass(&tables, &mut Bag::default(), &press(pad::DOWN))
                .sounds
                .len(),
            0
        );
        // The creatures row keeps its cursor over the visit of the third row.
        menu.pass(&tables, &mut Bag::default(), &press(pad::UP));
        assert_eq!((menu.flags, menu.cursor.at), (flag::CREATURES_ROW, 1));
        menu.pass(&tables, &mut Bag::default(), &Pad::default());
        assert_eq!(menu.kept[KEPT_FIGHTER], 2);
        menu.pass(&tables, &mut Bag::default(), &press(pad::UP));
        assert_eq!((menu.flags, menu.cursor.at), (flag::PLAYER_ROW, 0));
        assert_eq!(
            menu.pass(&tables, &mut Bag::default(), &press(pad::UP))
                .sounds
                .len(),
            0
        );
        menu.pass(&tables, &mut Bag::default(), &Pad::default());
        assert_eq!(menu.kept[KEPT_FIGHTER], 0);

        // Without a creature the step down goes to the third row and back.
        let mut alone = opened(0, 0);
        alone.pass(&tables, &mut Bag::default(), &press(pad::DOWN));
        assert_eq!(alone.flags, flag::THIRD_ROW);
        alone.pass(&tables, &mut Bag::default(), &press(pad::UP));
        assert_eq!(alone.flags, flag::PLAYER_ROW);
    }

    #[test]
    fn a_cancel_leaves_the_first_level_and_goes_back_from_the_entries() {
        let tables = tables();
        let mut menu = opened(1, 0);
        menu.pass(&tables, &mut Bag::default(), &press(pad::DOWN));
        assert_eq!(tap(&mut menu, pad::CROSS).sounds, [SOUND_CONFIRM]);
        assert_eq!((menu.screen, menu.cursor.at), (OF_CREATURE, 2));
        assert_eq!(menu.kept[KEPT_CREATURE_ENTRY], 2);
        rest(&mut menu);
        // The third and the fourth entry are nothing but their panel.
        assert_eq!(tap(&mut menu, pad::CROSS), Passed::default());
        assert_eq!(menu.shown(&tables), 0xC0);
        menu.pass(&tables, &mut Bag::default(), &press(pad::RIGHT));
        assert_eq!((menu.cursor.at, menu.shown(&tables)), (3, 0x2C0));
        assert_eq!(tap(&mut menu, pad::CROSS), Passed::default());
        menu.pass(&tables, &mut Bag::default(), &press(pad::RIGHT));
        assert_eq!(menu.cursor.at, 0);
        let back = tap(&mut menu, pad::TRIANGLE);
        assert_eq!(back.sounds, [SOUND_CANCEL]);
        assert_eq!(
            (menu.screen, menu.flags),
            (FIRST_LEVEL, flag::CREATURES_ROW)
        );
        assert_eq!(menu.cursor.at, 0);
        // Nothing opens on the way back.
        assert_eq!(
            (menu.panels, menu.stands, menu.openness),
            (0x06, 0x06, OPEN)
        );
        assert!(!menu.leaves());
        assert_eq!(tap(&mut menu, pad::L2).sounds, [SOUND_CANCEL]);
        assert!(menu.leaves());
        // A menu that leaves reads the pad no more.
        assert_eq!(
            menu.pass(&tables, &mut Bag::default(), &press(pad::DOWN)),
            Passed::default()
        );
    }

    #[test]
    fn the_third_row_starts_the_options_and_waits_for_them() {
        let tables = tables();
        let mut menu = opened(0, 0);
        menu.pass(&tables, &mut Bag::default(), &press(pad::DOWN));
        assert_eq!(menu.flags, flag::THIRD_ROW);
        // The confirm changes the screen with its sound; screen 8 has no panel.
        let acted = tap(&mut menu, pad::CROSS);
        assert_eq!((acted.sounds, acted.starts), (vec![SOUND_CONFIRM], None));
        assert_eq!((acted.not_built, menu.screen), (None, OPTIONS));
        assert_eq!((menu.shown(&tables), menu.waits()), (0, false));
        // The next pass starts the overlay, and from then on the menu does nothing.
        let started = menu.pass(&tables, &mut Bag::default(), &Pad::default());
        assert_eq!(started.starts, Some(OPTIONS));
        assert!(menu.waits());
        let before = menu.clone();
        for buttons in [pad::TRIANGLE, pad::CROSS, pad::UP] {
            let waited = menu.pass(&tables, &mut Bag::default(), &press(buttons));
            assert_eq!(waited, Passed::default());
        }
        assert_eq!(menu, before);
        // The end of the overlay is a cancel without a sound: the first level is back
        // with its third row, and its panels stand.
        menu.overlay_ended(&tables, &Bag::default());
        assert!(!menu.waits());
        assert_eq!(
            (menu.screen, menu.flags & flag::THIRD_ROW),
            (FIRST_LEVEL, flag::THIRD_ROW)
        );
        assert_eq!((menu.panels, menu.stands), (0x06, 0x06));
        // Once the screen is whole again a step up leaves the row.
        rest(&mut menu);
        menu.pass(&tables, &mut Bag::default(), &press(pad::UP));
        assert_eq!(menu.flags, flag::PLAYER_ROW);
    }

    #[test]
    fn the_fourth_entry_of_the_player_starts_an_overlay_and_waits_for_it() {
        let tables = tables();
        let mut menu = opened(0, 0);
        tap(&mut menu, pad::CROSS);
        rest(&mut menu);
        menu.pass(&tables, &mut Bag::default(), &press(pad::RIGHT));
        assert_eq!((menu.cursor.at, menu.shown(&tables)), (3, 0x1021));
        // The confirm changes the screen as any other does, with its sound.
        let acted = tap(&mut menu, pad::CROSS);
        assert_eq!((acted.sounds, acted.starts), (vec![SOUND_CONFIRM], None));
        assert_eq!((menu.screen, menu.confirmed_on), (EQUIP, OF_PLAYER));
        // The row of entries stays as it stood.
        assert_eq!((menu.panels, menu.stands, menu.openness), (0x01, 0x01, 1));
        assert!(!menu.waits());
        // The next pass starts the overlay, and from then on the menu does nothing.
        let started = menu.pass(&tables, &mut Bag::default(), &Pad::default());
        assert_eq!(started.starts, Some(EQUIP));
        assert!(menu.waits());
        assert_eq!((menu.entry_of_player(), menu.openness), (3, 2));
        let before = menu.clone();
        for buttons in [pad::TRIANGLE, pad::CROSS, pad::LEFT] {
            let waited = menu.pass(&tables, &mut Bag::default(), &press(buttons));
            assert_eq!(waited, Passed::default());
        }
        assert_eq!(menu, before);
        // The end of the overlay is a cancel without a sound: the entries are back
        // with their cursor, and the openness goes on from where it was.
        menu.overlay_ended(&tables, &Bag::default());
        assert!(!menu.waits());
        assert_eq!((menu.screen, menu.cursor.at), (OF_PLAYER, 3));
        assert_eq!((menu.cursor.columns, menu.cursor.lines), (5, 5));
        assert_eq!((menu.panels, menu.stands, menu.openness), (0x21, 0x21, 2));
        // The box beside the entry opens again, and the screen is deaf until it has.
        let deaf = menu.pass(&tables, &mut Bag::default(), &press(pad::TRIANGLE));
        assert_eq!((menu.shown(&tables), deaf.sounds.len()), (0x1021, 0));
        assert_eq!(menu.openness_of(0x1000), 3);
        rest(&mut menu);
        assert_eq!(tap(&mut menu, pad::TRIANGLE).sounds, [SOUND_CANCEL]);
        assert_eq!(menu.screen, FIRST_LEVEL);
    }

    #[test]
    fn the_first_entry_of_the_player_is_refused_while_nothing_is_caught() {
        let tables = tables();
        let entries = |caught: i32| {
            let mut menu = opened(0, caught);
            tap(&mut menu, pad::CROSS);
            rest(&mut menu);
            assert_eq!(tap(&mut menu, pad::CROSS), Passed::default());
            menu.pass(&tables, &mut Bag::default(), &press(pad::LEFT));
            menu.pass(&tables, &mut Bag::default(), &press(pad::LEFT));
            assert_eq!((menu.cursor.at, menu.shown(&tables)), (0, 0x421));
            menu
        };
        let mut menu = entries(0);
        assert_eq!(tap(&mut menu, pad::CROSS).sounds, [SOUND_REFUSED]);
        assert_eq!((menu.message, menu.result), (true, NOTHING_CAUGHT));
        assert_eq!(menu.screen, OF_PLAYER);
        // The message stands for its passes, drawn from the pass behind the refusal on.
        assert_eq!(menu.said, 0);
        for _ in 0..MESSAGE_PASSES - 1 {
            menu.pass(&tables, &mut Bag::default(), &Pad::default());
            assert_eq!(menu.said, NOTHING_CAUGHT);
        }
        assert!(menu.message);
        // The pass that ends it has drawn it once more.
        menu.pass(&tables, &mut Bag::default(), &Pad::default());
        assert_eq!(
            (menu.message, menu.result, menu.message_passes, menu.said),
            (false, 0, 0, NOTHING_CAUGHT)
        );
        menu.pass(&tables, &mut Bag::default(), &Pad::default());
        assert_eq!(menu.said, 0);
        // Any button takes it down, and that press does nothing else.
        tap(&mut menu, pad::CROSS);
        assert!(menu.message);
        menu.pass(&tables, &mut Bag::default(), &press(pad::TRIANGLE));
        assert!(!menu.message);
        menu.pass(&tables, &mut Bag::default(), &Pad::default());
        assert_eq!(menu.screen, OF_PLAYER);

        // With one that waits the entry leads to their screen, where a confirm does
        // nothing and a cancel leads back to the entry.
        let mut menu = entries(1);
        assert_eq!(tap(&mut menu, pad::CROSS).sounds, [SOUND_CONFIRM]);
        assert_eq!((menu.screen, menu.message), (CAUGHT, false));
        assert_eq!(menu.shown(&tables), 0x401);
        rest(&mut menu);
        assert_eq!(tap(&mut menu, pad::CROSS), Passed::default());
        assert_eq!(menu.screen, CAUGHT);
        let back = tap(&mut menu, pad::TRIANGLE);
        assert_eq!(back.sounds, [SOUND_CANCEL]);
        assert_eq!((menu.screen, menu.cursor.at), (OF_PLAYER, 0));
    }

    #[test]
    fn the_caught_creatures_scroll_by_rows_once_more_than_nine_wait() {
        let tables = tables();
        assert_eq!([1, 3, 4, 9, 10, 12].map(caught_rows), [1, 1, 2, 3, 4, 4]);
        let on_the_caught = |caught: i32| {
            let mut menu = opened(0, caught);
            tap(&mut menu, pad::CROSS);
            rest(&mut menu);
            menu.pass(&tables, &mut Bag::default(), &press(pad::LEFT));
            menu.pass(&tables, &mut Bag::default(), &press(pad::LEFT));
            tap(&mut menu, pad::CROSS);
            rest(&mut menu);
            assert_eq!(menu.screen, CAUGHT);
            menu
        };
        // Nine fit the screen: the cursor has one place.
        let mut menu = on_the_caught(9);
        assert_eq!((menu.cursor.rows, menu.cursor.lines), (1, 1));
        menu.pass(&tables, &mut Bag::default(), &press(pad::DOWN));
        menu.pass(&tables, &mut Bag::default(), &Pad::default());
        assert_eq!(menu.caught_row(), 0);

        // Ten make a fourth row: the first row shown is 0 or 1, and it does not wrap.
        let mut menu = on_the_caught(10);
        assert_eq!((menu.cursor.rows, menu.cursor.lines), (2, 2));
        let down = menu.pass(&tables, &mut Bag::default(), &press(pad::DOWN));
        assert_eq!(down.sounds, [SOUND_CURSOR]);
        menu.pass(&tables, &mut Bag::default(), &Pad::default());
        assert_eq!(menu.caught_row(), 1);
        menu.pass(&tables, &mut Bag::default(), &press(pad::DOWN));
        menu.pass(&tables, &mut Bag::default(), &Pad::default());
        assert_eq!(menu.caught_row(), 1);
        // The row is kept: the screen stands where it was left.
        tap(&mut menu, pad::TRIANGLE);
        rest(&mut menu);
        tap(&mut menu, pad::CROSS);
        assert_eq!((menu.screen, menu.cursor.at), (CAUGHT, 1));
    }

    #[test]
    fn the_lists_of_a_creature_are_walked_and_nothing_in_them_is_picked() {
        let tables = tables();
        let mut bag = Bag {
            spells: vec![2, 6],
            ..Bag::default()
        };
        let mut menu = opened(2, 0);
        menu.pass(&tables, &mut bag, &press(pad::DOWN));
        menu.pass(&tables, &mut bag, &press(pad::RIGHT));
        assert_eq!(
            tap_with(&mut menu, &mut bag, pad::CROSS).sounds,
            [SOUND_CONFIRM]
        );
        assert_eq!((menu.screen, menu.carried_creature()), (OF_CREATURE, 1));
        for _ in 0..OPEN {
            menu.pass(&tables, &mut bag, &Pad::default());
        }
        // "Magic" is the second entry: a line for each of the six spells.
        menu.pass(&tables, &mut bag, &press(pad::LEFT));
        assert_eq!((menu.cursor.at, menu.shown(&tables)), (1, 0xD0));
        assert_eq!(
            tap_with(&mut menu, &mut bag, pad::CROSS).sounds,
            [SOUND_CONFIRM]
        );
        assert_eq!(
            (menu.screen, menu.cursor.lines, menu.openness),
            (MAGIC, 6, 1)
        );
        // The list stood behind the entries already, so nothing opens again.
        assert_eq!((menu.panels, menu.stands), (0x90, 0x90));
        for _ in 0..OPEN {
            menu.pass(&tables, &mut bag, &Pad::default());
        }
        for _ in 0..4 {
            menu.pass(&tables, &mut bag, &press(pad::DOWN));
        }
        menu.pass(&tables, &mut bag, &Pad::default());
        assert_eq!((menu.spell_place(), menu.first_spell()), (4, 1));
        // A confirm does nothing at all.
        assert_eq!(tap_with(&mut menu, &mut bag, pad::CROSS), Passed::default());
        assert_eq!((menu.screen, menu.message), (MAGIC, false));
        let back = tap_with(&mut menu, &mut bag, pad::TRIANGLE);
        assert_eq!(back.sounds, [SOUND_CANCEL]);
        assert_eq!((menu.screen, menu.cursor.at), (OF_CREATURE, 1));
        // The list stands where it was left.
        tap_with(&mut menu, &mut bag, pad::CROSS);
        assert_eq!(
            (menu.screen, menu.cursor.at, menu.cursor.first),
            (MAGIC, 3, 1)
        );
        for _ in 0..OPEN {
            menu.pass(&tables, &mut bag, &Pad::default());
        }
        tap_with(&mut menu, &mut bag, pad::TRIANGLE);

        // "Special" is the first entry: five rows that do not wrap.
        menu.pass(&tables, &mut bag, &press(pad::LEFT));
        assert_eq!((menu.cursor.at, menu.shown(&tables)), (0, 0x1C0));
        assert_eq!(
            tap_with(&mut menu, &mut bag, pad::CROSS).sounds,
            [SOUND_CONFIRM]
        );
        assert_eq!((menu.screen, menu.cursor.rows), (SPECIAL, 5));
        for _ in 0..OPEN {
            menu.pass(&tables, &mut bag, &Pad::default());
        }
        menu.pass(&tables, &mut bag, &press(pad::UP));
        assert_eq!(menu.cursor.at, 0);
        menu.pass(&tables, &mut bag, &press(pad::DOWN));
        menu.pass(&tables, &mut bag, &press(pad::DOWN));
        menu.pass(&tables, &mut bag, &Pad::default());
        assert_eq!(menu.attack_row(), 2);
        assert_eq!(tap_with(&mut menu, &mut bag, pad::CROSS), Passed::default());
        assert_eq!(
            (menu.screen, menu.cursor.at, menu.openness),
            (SPECIAL, 2, OPEN)
        );
        tap_with(&mut menu, &mut bag, pad::TRIANGLE);
        assert_eq!((menu.screen, menu.cursor.at), (OF_CREATURE, 0));
        // A creature that knows fewer than four spells still has four lines.
        let mut few = PlayerMenu::open(&tables, 2, 0);
        few.screen = MAGIC;
        few.take_cursor(&tables);
        few.list_cursor(&bag);
        assert_eq!(few.cursor.lines, 4);
    }

    #[test]
    fn tables_that_leave_the_file_are_an_error() {
        assert!(Tables::from_file(&[0; 64]).is_err());
        let file = vec![0; (CREATURE_PANELS - MENU_FILE_BASE) as usize + 16];
        let tables = Tables::from_file(&file).expect("tables");
        assert_eq!(tables.cursors[16], [0; 4]);
    }
}
