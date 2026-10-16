
use crate::menu::{self, Cursor, Menu, Rect};
use crate::pad::{self, Pad};
use crate::player_menu::{SOUND_CANCEL, SOUND_CONFIRM, SOUND_CURSOR, SOUND_REFUSED};
use crate::rooms::Image;
use crate::save;

/// The slots of a card for this game's saves.
pub const SLOTS: usize = 15;
pub const EXTRA: usize = 4;
/// The most rows a list has.
pub const ROWS_MOST: usize = SLOTS + EXTRA;
/// The bytes of a slot's head in the list of heads.
pub const HEAD: usize = 128;
/// The buffer of a save, and what a file of one block holds of it.
pub const BUFFER: usize = 8576;
pub const BLOCK: usize = 8192;

/// The states of the screen (the half word at +48 of the task's record).
pub mod state {
    /// The first list: save, load, go on, leave the game.
    pub const LIST: i16 = 0;
    /// The two ports.
    pub const PORT: i16 = 1;
    /// The question whether to format the card.
    pub const FORMAT: i16 = 2;
    /// The fifteen slots.
    pub const SLOTS: i16 = 3;
    /// The question in front of a save or a load.
    pub const ASK: i16 = 4;
    /// The question whether to go back to the title.
    pub const TITLE: i16 = 5;
    /// No card in the port.
    pub const NO_CARD: i16 = 6;
    /// Message: the card is full.
    pub const FULL: i16 = 7;
    /// Message: no save of this game on the card.
    pub const NO_SAVE: i16 = 8;
    /// The slots with the line that no block is free for a new save.
    pub const NO_ROOM: i16 = 9;
    /// The slots with the line that the data is damaged.
    pub const DAMAGED: i16 = 10;
    /// The ports with the line that the card is not formatted.
    pub const UNFORMATTED: i16 = 11;
    /// Messages: the formatting, the save, the load failed.
    pub const FORMAT_FAILED: i16 = 13;
    pub const SAVE_FAILED: i16 = 14;
    pub const LOAD_FAILED: i16 = 15;
    /// The card is being checked.
    pub const CHECK: i16 = 18;
    /// Not the game's: the question in front of a save that is deleted, of the build's
    /// screen for a PC ([`super::Shape::Plain`]).
    pub const DELETE: i16 = 19;
}

/// What the task does in place of its states (the word at +40 of the record).
pub mod deed {
    pub const STATES: i32 = 0;
    pub const SAVE: i32 = 2;
    pub const LOAD: i32 = 3;
    pub const FORMAT: i32 = 4;
    pub const END: i32 = 5;
    pub const LOADED: i32 = 6;
}

/// Bits of the word at +84 of the record, which the end of the screen goes by.
pub mod word {
    /// A game was loaded.
    pub const LOADED: u32 = 0x01;
    /// The screen of "Continue".
    pub const OF_TITLE: u32 = 0x04;
    /// The player goes back to the title.
    pub const TO_TITLE: u32 = 0x10;
    /// A save without the first list.
    pub const NO_LIST: u32 = 0x20;
}

/// The lines of the first list.
pub mod line {
    pub const SAVE: i16 = 0;
    pub const LOAD: i16 = 1;
    pub const GO_ON: i16 = 2;
    pub const LEAVE: i16 = 3;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Room,
    Continue,
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Shape {
    #[default]
    Original,
    Plain,
}

/// A text of the build's own in the units of the game's texts: the wide letters of the
/// game's font, a unit a letter. Only for what the game has no text for.
pub const fn wide<const N: usize>(letters: &[u8; N]) -> [u16; N] {
    let mut units = [0; N];
    let mut n = 0;
    while n < N {
        units[n] = wide_unit(letters[n]);
        n += 1;
    }
    units
}

/// The unit of the wide letter of a letter of ASCII: the letters, the digits and a few
/// signs; anything else is a space.
pub const fn wide_unit(letter: u8) -> u16 {
    let code: u16 = match letter {
        b'A'..=b'Z' => 0x8260 + (letter - b'A') as u16,
        b'a'..=b'z' => 0x8281 + (letter - b'a') as u16,
        b'0'..=b'9' => 0x824F + (letter - b'0') as u16,
        b'?' => 0x8148,
        b':' => 0x8146,
        b'.' => 0x8144,
        b',' => 0x8143,
        b'-' => 0x817C,
        b'/' => 0x815E,
        _ => 0x8140,
    };
    code.swap_bytes()
}

/// The question in front of a save that is deleted, and the word of the legend for it:
/// the build's, for its screen for a PC.
pub static ASKS_DELETE: [u16; 17] = wide(b"Delete this save?");
pub static DELETE_WORD: [u16; 6] = wide(b"Delete");

/// The names of the build's rows above the slots ([`EXTRA`]), in their order.
static QUICK: [u16; 5] = wide(b"Quick");
static AUTO_1: [u16; 6] = wide(b"Auto 1");
static AUTO_2: [u16; 6] = wide(b"Auto 2");
static AUTO_3: [u16; 6] = wide(b"Auto 3");
pub static EXTRA_NAMES: [&[u16]; EXTRA] = [&QUICK, &AUTO_1, &AUTO_2, &AUTO_3];

/// A card as the screen asks for it. The results are the library's: 0 is fine.
pub trait Card {
    /// The result of command 1 for a port: whether a card is there.
    fn present(&mut self, _port: u8) -> i32 {
        0
    }
    /// The result of command 2 for a port: 0 or 3 for a card that can be used, 4 for
    /// one that is not formatted.
    fn check(&mut self, _port: u8) -> i32 {
        0
    }
    /// The formatting: 0 done, 1 the card went, 2 failed.
    fn format(&mut self, _port: u8) -> i32 {
        0
    }
    /// The blocks every file of the card takes together.
    fn blocks(&self, port: u8) -> i32;
    /// Whether the file of a slot is on the card.
    fn has(&self, port: u8, slot: usize) -> bool;
    /// The buffer of a slot's file, or the result of a read that failed.
    fn read(&mut self, port: u8, slot: usize) -> Result<Vec<u8>, i32>;
    /// The buffer is written as the slot's file, made when it is not there: the result.
    fn write(&mut self, port: u8, slot: usize, buffer: &[u8]) -> i32;
    /// The slot's file is taken off the card, for the build's "Delete": the result. A
    /// card that cannot do it refuses.
    fn remove(&mut self, _port: u8, _slot: usize) -> i32 {
        1
    }
    /// How many of the build's rows ([`EXTRA`]) a list of loads of the screen for a PC
    /// shows above the slots; they are the card's slots from [`SLOTS`] on.
    fn extra(&self) -> usize {
        0
    }
}

/// Two cards held in memory, one a port, always there and formatted: a file is the
/// first block of the buffer.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Cards {
    pub files: [[Option<Vec<u8>>; SLOTS]; 2],
}

impl Cards {
    fn file(&self, port: u8, slot: usize) -> Option<&Vec<u8>> {
        self.files.get(usize::from(port))?.get(slot)?.as_ref()
    }
}

impl Card for Cards {
    fn blocks(&self, port: u8) -> i32 {
        (self.files.get(usize::from(port))).map_or(0, |files| files.iter().flatten().count() as i32)
    }

    fn has(&self, port: u8, slot: usize) -> bool {
        self.file(port, slot).is_some()
    }

    fn read(&mut self, port: u8, slot: usize) -> Result<Vec<u8>, i32> {
        let mut buffer = self.file(port, slot).cloned().ok_or(5)?;
        buffer.resize(BUFFER, 0);
        Ok(buffer)
    }

    fn write(&mut self, port: u8, slot: usize, buffer: &[u8]) -> i32 {
        let place = (self.files.get_mut(usize::from(port))).and_then(|files| files.get_mut(slot));
        let Some(place) = place else {
            return 1;
        };
        let mut file = buffer.to_vec();
        file.resize(BLOCK, 0);
        *place = Some(file);
        0
    }

    fn remove(&mut self, port: u8, slot: usize) -> i32 {
        let place = (self.files.get_mut(usize::from(port))).and_then(|files| files.get_mut(slot));
        match place.and_then(Option::take) {
            Some(_) => 0,
            None => 1,
        }
    }
}

/// The running game as the screen asks for it.
pub trait Game {
    /// The writer (`0x800B9D78`): the buffer of a save.
    fn write(&mut self) -> Vec<u8>;
    /// The reader (`0x800BA6D8`): takes a buffer and answers whether it was a save.
    fn read(&mut self, buffer: &[u8]) -> bool;
}

/// What a pass of the screen did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Passed {
    /// The sounds asked for, in their order.
    pub sounds: Vec<u8>,
    /// The task ended in this pass, with this word at +84 ([`word`]).
    pub ended: Option<u32>,
}

/// The routine of the task: its states, or one of the deeds that take passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Routine {
    States,
    Saving,
    Loading,
    /// The three steps of the formatting.
    Formatting(u8),
    Ending,
}

/// The library's commands the screen starts.
const EXISTS: u8 = 1;
const ACCEPT: u8 = 2;
const READ: u8 = 3;
const WRITE: u8 = 4;

/// The record of the screen's task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardScreen {
    pub shape: Shape,
    /// The state (+48) and the one asked for (+36), -1 for none.
    pub state: i16,
    pub wanted: i32,
    /// What the task does in place of its states (+40), and its routine.
    pub deed: i32,
    pub routine: Routine,
    /// The word at +84.
    pub word: u32,
    /// The cursor records of the states 0 to 5.
    pub cursors: [Cursor; 6],
    starts: [(i16, i16); 6],
    /// The heads read so far and the next slot to read (+32): of the rows of the list,
    /// which are the slots unless [`Self::extra`] rows stand above them.
    pub heads: Vec<[u8; HEAD]>,
    pub count: i32,
    /// The rows that have a file, the free blocks and the saves found, as the last
    /// reading of the directory left them.
    pub filled: [bool; ROWS_MOST],
    /// The build's rows above the slots in this list ([`EXTRA`]): only a list of loads of
    /// the screen for a PC has them, so a save never goes to them.
    pub extra: usize,
    pub free: i32,
    pub saves: i32,
    /// A game was loaded (+8).
    pub loaded: bool,
    /// A command of the library that is over and not asked for yet.
    over: Option<(u8, i32)>,
    buffer: Vec<u8>,
}

impl CardScreen {
    /// The screen as its maker leaves it (`0x800B788C`), with the cursor records of
    /// the file ([`Tables::cursors`]).
    pub fn open(kind: Kind, records: [Cursor; 6]) -> Self {
        let starts = records.map(|record| (record.at, record.first));
        let mut cursors = records;
        let (word, state, line) = match kind {
            Kind::Room => (0, state::LIST, line::SAVE),
            Kind::Continue => (word::OF_TITLE, state::PORT, line::LOAD),
            Kind::Finished => (word::NO_LIST, state::PORT, line::SAVE),
        };
        cursors[0].at = line;
        Self {
            shape: Shape::Original,
            state,
            wanted: -1,
            deed: deed::STATES,
            routine: Routine::States,
            word,
            cursors,
            starts,
            heads: vec![[0; HEAD]; ROWS_MOST],
            count: 0,
            filled: [false; ROWS_MOST],
            extra: 0,
            free: 0,
            saves: 0,
            loaded: false,
            over: None,
            buffer: Vec::new(),
        }
    }

    /// The build's screen for a PC ([`Shape::Plain`]) over the one list of saves that
    /// `card` has in its first port: a room's starts on the first list as the game's
    /// does, and "Continue" of the title and the save behind the end of the story,
    /// which have no first list, start on the saves.
    pub fn open_plain(kind: Kind, records: [Cursor; 6], card: &dyn Card) -> Self {
        let mut screen = Self::open(kind, records);
        screen.shape = Shape::Plain;
        if kind != Kind::Room {
            screen.rows_for(card);
            screen.directory(card);
            screen.state = state::SLOTS;
        }
        screen
    }

    /// A list of the screen for a PC gets the build's rows above its slots when it is a
    /// list of loads.
    fn rows_for(&mut self, card: &dyn Card) {
        self.extra = match self.line() {
            line::LOAD => card.extra().min(EXTRA),
            _ => 0,
        };
        self.cursors[3].lines = self.rows() as u16;
    }

    /// The saves come up without a port and a check of it ([`Shape::Plain`]): the
    /// directory is read, and the heads from the first slot on.
    fn to_slots(&mut self, card: &dyn Card) {
        self.over = None;
        self.rows_for(card);
        self.directory(card);
        self.count = 0;
        (self.cursors[3].at, self.cursors[3].first) = self.starts[3];
        self.wanted = i32::from(state::SLOTS);
    }

    /// The line of the first list the screen works for.
    pub fn line(&self) -> i16 {
        self.cursors[0].at
    }

    /// The port the card code works on: the cursor of state 1.
    pub fn port(&self) -> u8 {
        self.cursors[1].at as u8
    }

    /// The row under the cursor of the slots.
    pub fn row(&self) -> usize {
        (self.cursors[3].at + self.cursors[3].first).max(0) as usize
    }

    /// The card's slot of the row under the cursor.
    pub fn slot(&self) -> usize {
        self.slot_of(self.row())
    }

    /// The rows of the list: the slots, and the build's rows above them.
    pub fn rows(&self) -> usize {
        SLOTS + self.extra
    }

    /// The card's slot of a row: a row above the slots is one of the card's slots from
    /// [`SLOTS`] on.
    pub fn slot_of(&self, row: usize) -> usize {
        match row.checked_sub(self.extra) {
            Some(slot) => slot,
            None => SLOTS + row,
        }
    }

    /// Whether the head of a row is in: the count is two above its number.
    pub fn is_read(&self, row: usize) -> bool {
        (row as i32) < self.count - 1
    }

    /// The message on screen, as its number in the records of `0x800BE1A8`
    /// ([`Tables::messages`]).
    pub fn message(&self) -> Option<usize> {
        // The messages of a card that works are the card's: the list stays up instead.
        if self.shape == Shape::Plain && matches!(self.routine, Routine::Loading | Routine::Saving)
        {
            return None;
        }
        match self.routine {
            Routine::Formatting(_) => return Some(1),
            Routine::Loading => return Some(2),
            Routine::Saving => return Some(3),
            Routine::Ending => return None,
            Routine::States => {}
        }
        match self.state {
            state::FULL => Some(0),
            state::NO_SAVE => Some(4),
            state::FORMAT_FAILED => Some(6),
            state::SAVE_FAILED => Some(7),
            state::LOAD_FAILED => Some(8),
            _ => None,
        }
    }

    /// One pass of the task with the pad's words of the pass.
    pub fn pass(&mut self, pad: &Pad, card: &mut dyn Card, game: &mut dyn Game) -> Passed {
        let mut passed = Passed::default();
        match self.routine {
            Routine::States => self.dispatch(pad, card, game, &mut passed),
            Routine::Saving => self.save(card, &mut passed),
            Routine::Loading => self.load(card, game, &mut passed),
            Routine::Formatting(step) => self.format(step, card),
            Routine::Ending => passed.ended = Some(self.word),
        }
        passed
    }

    /// The routine of the task (`0x800B7B64`): the states, or the start of a deed.
    fn dispatch(
        &mut self,
        pad: &Pad,
        card: &mut dyn Card,
        game: &mut dyn Game,
        passed: &mut Passed,
    ) {
        match self.deed {
            deed::STATES => self.states(pad, card, passed),
            deed::SAVE => {
                self.buffer = game.write();
                self.buffer.resize(BUFFER, 0);
                self.routine = Routine::Saving;
            }
            deed::LOAD => self.routine = Routine::Loading,
            deed::FORMAT => self.routine = Routine::Formatting(0),
            deed::END | deed::LOADED => self.routine = Routine::Ending,
            _ => {}
        }
    }

    /// The library's answer (`0x800BCCC8`): the command that is over, with its result.
    fn asked(&mut self) -> Option<(u8, i32)> {
        self.over.take()
    }

    /// Command 1 or 2 about the port (`0x800BB1F8`, `0x800BB154`): -1 until it is over.
    fn about(&mut self, command: u8, card: &mut dyn Card) -> i32 {
        match self.asked() {
            None => {
                let port = self.port();
                let result = match command {
                    EXISTS => card.present(port),
                    _ => card.check(port),
                };
                self.over = Some((command, result));
                -1
            }
            Some((over, result)) if over == command => result,
            Some(_) => -1,
        }
    }

    /// The directory of the card (`0x800BB29C`).
    fn directory(&mut self, card: &dyn Card) {
        let port = self.port();
        let rows = self.rows();
        self.filled = std::array::from_fn(|row| row < rows && card.has(port, self.slot_of(row)));
        self.saves = self.filled.iter().filter(|filled| **filled).count() as i32;
        self.free = SLOTS as i32 - card.blocks(port);
    }

    /// The heads, one at a time (`0x800B9A10`): false when the card went.
    fn read_heads(&mut self, card: &mut dyn Card) -> bool {
        let rows = self.rows() as i32;
        match self.asked() {
            None if self.count < rows => {
                let row = self.count as usize;
                if self.filled[row] {
                    let port = self.port();
                    let result = match card.read(port, self.slot_of(row)) {
                        Ok(buffer) => {
                            let head = buffer.get(save::HEAD..save::HEAD + HEAD);
                            match head {
                                Some(head) => {
                                    self.heads[row].copy_from_slice(head);
                                    0
                                }
                                None => 5,
                            }
                        }
                        Err(result) => result,
                    };
                    self.over = Some((READ, result));
                } else if row as i32 == rows - 1 {
                    self.count = rows;
                }
                self.count += 1;
                true
            }
            None => {
                let port = self.port();
                self.over = Some((EXISTS, card.present(port)));
                true
            }
            Some((EXISTS, result)) => result == 0,
            Some((READ, 0)) => {
                if self.count == rows {
                    self.count += 1;
                }
                true
            }
            Some((READ, 1..=3 | 5)) => false,
            Some(_) => true,
        }
    }

    /// The states (`0x800B7C10`): the state asked for is taken, the cursor of a menu
    /// steps, and the state has its pass.
    fn states(&mut self, pad: &Pad, card: &mut dyn Card, passed: &mut Passed) {
        self.take_wanted();
        // The build's question has the cursor of the game's question of a save.
        let record = match self.state {
            state::DELETE => state::ASK,
            state => state,
        };
        if let Some(cursor) =
            (usize::try_from(record).ok()).and_then(|state| self.cursors.get_mut(state))
            && cursor.step(pad.repeated).sounds
        {
            passed.sounds.push(SOUND_CURSOR);
        }
        let confirm = menu::confirmed(pad);
        let cancel = menu::cancelled(pad);
        let pressed = pad.pressed != 0;
        match self.state {
            state::LIST => {
                if confirm {
                    passed.sounds.push(SOUND_CONFIRM);
                    match self.line() {
                        line::GO_ON => self.deed = deed::END,
                        line::LEAVE => self.wanted = i32::from(state::TITLE),
                        0.. if self.shape == Shape::Plain => self.to_slots(card),
                        0.. => self.wanted = i32::from(state::PORT),
                        _ => {}
                    }
                } else if cancel {
                    passed.sounds.push(SOUND_CANCEL);
                    self.deed = deed::END;
                }
            }
            state::PORT => {
                if confirm {
                    passed.sounds.push(SOUND_CONFIRM);
                    self.over = None;
                    self.wanted = i32::from(state::CHECK);
                } else if cancel {
                    passed.sounds.push(SOUND_CANCEL);
                    match self.word & (word::OF_TITLE | word::NO_LIST) != 0 {
                        true => self.deed = deed::END,
                        false => self.wanted = i32::from(state::LIST),
                    }
                }
            }
            state::FORMAT => {
                let answer = self.about(ACCEPT, card);
                if answer < -1 || (answer > 0 && answer != 4) {
                    self.wanted = i32::from(state::PORT);
                }
                if cancel {
                    passed.sounds.push(SOUND_CANCEL);
                    self.wanted = i32::from(state::PORT);
                }
                if confirm {
                    passed.sounds.push(SOUND_CONFIRM);
                    match self.cursors[2].at {
                        0 => self.deed = deed::FORMAT,
                        1 => self.wanted = i32::from(state::PORT),
                        _ => {}
                    }
                }
            }
            state::SLOTS => {
                if !self.read_heads(card) {
                    self.wanted = i32::from(state::PORT);
                }
                // The build's "Delete": the square on a save whose head is in.
                if self.shape == Shape::Plain && pad.pressed & pad::SQUARE != 0 {
                    let row = self.row();
                    let filled = self.filled.get(row).copied().unwrap_or(false);
                    if filled && self.is_read(row) {
                        passed.sounds.push(SOUND_CONFIRM);
                        self.wanted = i32::from(state::DELETE);
                    } else {
                        passed.sounds.push(SOUND_REFUSED);
                    }
                } else {
                    self.pick(cancel, confirm, passed);
                }
            }
            state::DELETE => {
                let row = self.row();
                if cancel || (confirm && self.cursors[4].at != 0) {
                    passed.sounds.push(SOUND_CANCEL);
                    self.wanted = i32::from(state::SLOTS);
                } else if confirm {
                    let done = card.remove(self.port(), self.slot_of(row)) == 0;
                    if done {
                        if let Some(head) = self.heads.get_mut(row) {
                            *head = [0; HEAD];
                        }
                        self.directory(card);
                    }
                    passed.sounds.push(match done {
                        true => SOUND_CONFIRM,
                        false => SOUND_REFUSED,
                    });
                    self.wanted = i32::from(state::SLOTS);
                }
            }
            state::ASK => {
                if !self.read_heads(card) {
                    self.wanted = i32::from(state::PORT);
                }
                if cancel {
                    passed.sounds.push(SOUND_CANCEL);
                    self.wanted = i32::from(state::SLOTS);
                }
                if confirm {
                    if self.cursors[4].at == 0 {
                        passed.sounds.push(SOUND_CONFIRM);
                        match self.line() {
                            line::LOAD => {
                                self.over = None;
                                self.deed = deed::LOAD;
                            }
                            line::SAVE => {
                                self.over = None;
                                self.deed = deed::SAVE;
                            }
                            _ => {}
                        }
                    } else {
                        passed.sounds.push(SOUND_CANCEL);
                        self.wanted = i32::from(state::SLOTS);
                    }
                }
            }
            state::TITLE => {
                if confirm && self.cursors[5].at == 0 {
                    self.word = word::TO_TITLE;
                    self.deed = deed::END;
                    passed.sounds.push(SOUND_CONFIRM);
                } else if confirm || cancel {
                    self.wanted = i32::from(state::LIST);
                    passed.sounds.push(SOUND_CANCEL);
                }
            }
            state::NO_CARD => {
                if matches!(self.about(EXISTS, card), 0 | 3 | 4) {
                    self.over = None;
                    self.wanted = i32::from(state::CHECK);
                }
                if pressed {
                    self.wanted = i32::from(state::PORT);
                }
            }
            state::FULL => {
                if !self.read_heads(card) || pressed {
                    self.wanted = i32::from(state::PORT);
                }
            }
            state::NO_SAVE => {
                let answer = self.about(ACCEPT, card);
                if answer < -1 || (answer > 0 && answer != 4) || pressed {
                    self.wanted = i32::from(state::PORT);
                }
            }
            state::NO_ROOM | state::DAMAGED => {
                if pressed {
                    self.wanted = i32::from(state::SLOTS);
                }
                if !self.read_heads(card) {
                    self.wanted = i32::from(state::PORT);
                }
            }
            state::UNFORMATTED => {
                if self.about(EXISTS, card) == 1 || pressed {
                    self.wanted = i32::from(state::PORT);
                }
            }
            state::FORMAT_FAILED | state::SAVE_FAILED | state::LOAD_FAILED => {
                if pressed {
                    self.wanted = i32::from(state::PORT);
                }
            }
            state::CHECK => self.check(card, passed),
            _ => {}
        }
    }

    /// The state asked for becomes the state, with its cursor set up anew
    /// (`0x800B8310`): the port starts the slots' cursor again, the question of a save
    /// stands on its second answer.
    fn take_wanted(&mut self) {
        let Ok(wanted) = i16::try_from(self.wanted) else {
            return;
        };
        if wanted == -1 {
            return;
        }
        // Without ports, what leads back to them leads to where they were come from:
        // the first list, or the end of a screen that has none.
        let wanted = match (self.shape, wanted) {
            (Shape::Plain, state::PORT) if self.word & (word::OF_TITLE | word::NO_LIST) != 0 => {
                self.wanted = -1;
                self.deed = deed::END;
                return;
            }
            (Shape::Plain, state::PORT) => state::LIST,
            _ => wanted,
        };
        let anew = match wanted {
            state::PORT => Some(3),
            state::FORMAT => Some(2),
            state::ASK | state::DELETE => Some(4),
            state::TITLE => Some(5),
            _ => None,
        };
        if let Some(record) = anew {
            (self.cursors[record].at, self.cursors[record].first) = self.starts[record];
        }
        if (wanted == state::ASK && self.line() == line::SAVE) || wanted == state::DELETE {
            self.cursors[4].at = 1;
        }
        self.state = wanted;
        self.wanted = -1;
    }

    /// The pad on the slots (`0x800B9818`).
    fn pick(&mut self, cancel: bool, confirm: bool, passed: &mut Passed) {
        if cancel {
            passed.sounds.push(SOUND_CANCEL);
            self.wanted = i32::from(state::PORT);
        }
        if !confirm || !matches!(self.line(), line::SAVE | line::LOAD) {
            return;
        }
        let row = self.row();
        let filled = self.filled.get(row).copied().unwrap_or(false);
        let mut sound = SOUND_REFUSED;
        if self.is_read(row) {
            if filled || (self.line() == line::SAVE && self.free > 0) {
                self.wanted = i32::from(state::ASK);
                sound = SOUND_CONFIRM;
            } else if self.line() == line::SAVE {
                self.wanted = i32::from(state::NO_ROOM);
            }
        }
        passed.sounds.push(sound);
    }

    /// The check of the card (`0x800B964C`).
    fn check(&mut self, card: &mut dyn Card, passed: &mut Passed) {
        let saving = self.line() == line::SAVE;
        let next = match self.about(ACCEPT, card) {
            -1 => return,
            0 | 3 => {
                self.directory(card);
                passed.sounds.push(SOUND_CONFIRM);
                let some = self.saves != 0 || (saving && self.free != 0);
                match (some, saving) {
                    (true, _) => {
                        self.count = 0;
                        state::SLOTS
                    }
                    (false, true) => state::FULL,
                    (false, false) => state::NO_SAVE,
                }
            }
            4 if saving => state::FORMAT,
            4 => state::UNFORMATTED,
            _ => state::NO_CARD,
        };
        self.wanted = i32::from(next);
    }

    /// The passes of a save (`0x800B9BB0`): the buffer is written, and when that is
    /// over the head goes into the list and the slots are back.
    fn save(&mut self, card: &mut dyn Card, passed: &mut Passed) {
        let row = self.row();
        match self.asked() {
            None => {
                let result = card.write(self.port(), self.slot_of(row), &self.buffer);
                self.over = Some((WRITE, result));
            }
            Some((WRITE, 0)) => {
                self.routine = Routine::States;
                self.deed = deed::STATES;
                self.wanted = i32::from(state::SLOTS);
                passed.sounds.push(SOUND_CONFIRM);
                let head = self.buffer.get(save::HEAD..save::HEAD + HEAD);
                if let (Some(place), Some(head)) = (self.heads.get_mut(row), head) {
                    place.copy_from_slice(head);
                }
                self.directory(card);
            }
            Some((WRITE, 1..=3 | 5)) => {
                self.routine = Routine::States;
                self.deed = deed::STATES;
                self.wanted = i32::from(state::SAVE_FAILED);
            }
            Some(_) => {}
        }
    }

    /// The passes of a load (`0x800BA520`): the file is read and handed to the reader.
    fn load(&mut self, card: &mut dyn Card, game: &mut dyn Game, passed: &mut Passed) {
        let (row, slot) = (self.row(), self.slot());
        let failed = |screen: &mut Self| {
            screen.routine = Routine::States;
            screen.deed = deed::STATES;
            screen.wanted = i32::from(state::LOAD_FAILED);
        };
        match self.asked() {
            None if self.filled.get(row).copied().unwrap_or(false) => {
                let result = match card.read(self.port(), slot) {
                    Ok(buffer) => {
                        self.buffer = buffer;
                        0
                    }
                    Err(result) => result,
                };
                self.over = Some((READ, result));
            }
            None => failed(self),
            Some((READ, 0)) => {
                self.routine = Routine::States;
                if game.read(&self.buffer) {
                    self.loaded = true;
                    self.deed = deed::LOADED;
                    self.word |= word::LOADED;
                } else {
                    self.deed = deed::STATES;
                    self.wanted = i32::from(state::DAMAGED);
                }
                passed.sounds.push(SOUND_CONFIRM);
            }
            Some((READ, 1..=3 | 5)) => failed(self),
            Some(_) => {}
        }
    }

    /// The three steps of the formatting (`0x800BAC54`, `0x800BAC98`, `0x800BACDC`):
    /// two passes of the message, then the card is formatted.
    fn format(&mut self, step: u8, card: &mut dyn Card) {
        if step < 2 {
            self.routine = Routine::Formatting(step + 1);
            return;
        }
        let next = match card.format(self.port()) {
            0 => {
                self.count = 0;
                self.directory(card);
                state::SLOTS
            }
            1 => state::PORT,
            2 => state::FORMAT_FAILED,
            _ => return,
        };
        self.routine = Routine::States;
        self.deed = deed::STATES;
        self.wanted = i32::from(next);
        self.over = None;
    }
}

/// A sprite of the menus' pool at a place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sprite {
    pub at: (i16, i16),
    pub number: i16,
    pub slot: i16,
}

/// The tables of file 48 the screen draws by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tables {
    /// The cursor records of the states 0 to 5 (`0x800BE238`).
    pub cursors: [Cursor; 6],
    /// The menus: the four frames of the slots, the two ports, the first list, and the
    /// two answers of the states 2, 4 and 5.
    pub slots: Menu,
    pub ports: [Menu; 2],
    pub list: Menu,
    pub answers: [Menu; 3],
    /// The two words of the answers.
    pub answers_words: [Vec<u16>; 2],
    /// The word above the slots by the first list's line, the sprites on the ports and
    /// the kind of the place in a slot's row.
    pub headings: [Sprite; 2],
    pub on_ports: Vec<Sprite>,
    pub place_kinds: [Sprite; 3],
    /// A slot's row: the places of its nine pieces, its labels, and the area the rows
    /// are cut to.
    pub row: [(i16, i16); 9],
    pub labels: Vec<Sprite>,
    pub area: Rect,
    /// The frames: of the questions of the states 4, 5 and 2, of the ports' heading,
    /// the line of help, the heading, the port in use, the free blocks, the number of
    /// the slot.
    pub frames: [Rect; 9],
    /// The questions of state 4 (a load, a save over a file, a save to a free slot), of
    /// state 5, the two lines of state 2, the words beside the free blocks, the stroke
    /// behind the number of the slot and the dot of the clock.
    pub texts: [Vec<u16>; 9],
    /// The line of help of the states 1, 6, 9, 10, 11 and 18, and of the slots for a
    /// save and for a load.
    pub help: [Vec<u16>; 6],
    pub help_of_slots: [Vec<u16>; 2],
    /// The legend: a sprite and a word.
    pub legend: Vec<(u32, Vec<u16>)>,
    /// The nine messages, up to three lines each.
    pub messages: Vec<Vec<Vec<u16>>>,
}

const CURSORS: u32 = 0x800B_E238;
const MENUS: [u32; 7] = [
    0x800B_DDF8,
    0x800B_DE50,
    0x800B_DEA8,
    0x800B_DFBC,
    0x800B_E050,
    0x800B_E0E4,
    0x800B_E178,
];
const HEADINGS: u32 = 0x800B_DC84;
const ON_PORTS: u32 = 0x800B_DC94;
const PLACE_KINDS: u32 = 0x800B_DCD4;
const ROW: u32 = 0x800B_DC28;
const LABELS: u32 = 0x800B_DC4C;
const AREA: u32 = 0x800B_DCFC;
const FRAMES: [u32; 9] = [
    0x800B_6B20,
    0x800B_6B58,
    0x800B_6C14,
    0x800B_6BE4,
    0x800B_6D30,
    0x800B_6D84,
    0x800B_6D8C,
    0x800B_6DAC,
    0x800B_6DB8,
];
const TEXTS: [u32; 9] = [
    0x800B_6AD0,
    0x800B_6AE8,
    0x800B_6B08,
    0x800B_6B28,
    0x800B_6A18,
    0x800B_6BEC,
    0x800B_6D94,
    0x800B_6DB4,
    0x800B_6DC0,
];
const HELP: [u32; 6] = [
    0x800B_6C1C,
    0x800B_6CCC,
    0x800B_6C48,
    0x800B_6C94,
    0x800B_6A18,
    0x800B_6D00,
];
const HELP_OF_SLOTS: u32 = 0x800B_DCF4;
const LEGEND: u32 = 0x800B_E190;
const MESSAGES: u32 = 0x800B_E1A8;
const MESSAGE_COUNT: u32 = 9;
/// No table of the screen comes near this many entries.
const MOST: usize = 32;

impl Tables {
    /// Reads the tables from the screen's file, whose first byte is at `base`.
    pub fn from_file(file: &[u8], base: u32) -> Option<Self> {
        let image = Image { data: file, base };
        let place = |at: u32| Some((image.i16(at).ok()?, image.i16(at + 2).ok()?));
        let sprite = |at: u32| {
            Some(Sprite {
                at: place(at)?,
                number: image.i16(at + 4).ok()?,
                slot: image.i16(at + 6).ok()?,
            })
        };
        let rect = |at: u32| {
            let ((x, y), (w, h)) = (place(at)?, place(at + 4)?);
            Some(Rect { x, y, w, h })
        };
        let text = |at: u32| {
            let from = usize::try_from(at.checked_sub(base)?).ok()?;
            Some(crate::text::units(file.get(from..)?))
        };
        // A list of sprites up to the first whose number ends it.
        let sprites = |from: u32, ends: &dyn Fn(i16) -> bool| {
            let mut list = Vec::new();
            loop {
                let next = sprite(from + 8 * list.len() as u32)?;
                if ends(next.number) {
                    return Some(list);
                }
                list.push(next);
                if list.len() > MOST {
                    return None;
                }
            }
        };
        let mut cursors = Vec::new();
        for record in 0..6 {
            let byte = |n: u32| image.u8(CURSORS + 16 * record + n).ok();
            cursors.push(Cursor {
                columns: i16::from(byte(6)?),
                rows: i16::from(byte(7)?),
                lines: u16::from(byte(8)?),
                flags: u16::from(byte(11)?),
                at: i16::from(byte(9)?),
                first: i16::from(byte(10)?),
            });
        }
        let mut menus = Vec::new();
        for address in MENUS {
            menus.push(Menu::at(file, base, address)?);
        }
        let [slots, port_1, port_2, list, format, ask, title]: [Menu; 7] = menus.try_into().ok()?;
        let mut legend = Vec::new();
        loop {
            let at = LEGEND + 8 * legend.len() as u32;
            let number = image.u32(at).ok()?;
            if number == u32::MAX {
                break;
            }
            legend.push((number, text(image.u32(at + 4).ok()?)?));
            if legend.len() > MOST {
                return None;
            }
        }
        let mut messages = Vec::new();
        for message in 0..MESSAGE_COUNT {
            let mut lines = Vec::new();
            for line in 0..3 {
                match image.u32(MESSAGES + 16 * message + 4 * line).ok()? {
                    0 => break,
                    address => lines.push(text(address)?),
                }
            }
            messages.push(lines);
        }
        let several = |addresses: &[u32]| addresses.iter().map(|&at| text(at)).collect();
        let texts: Option<Vec<_>> = several(&TEXTS);
        let help: Option<Vec<_>> = several(&HELP);
        let mut frames = Vec::new();
        for address in FRAMES {
            frames.push(rect(address)?);
        }
        let mut row = [(0, 0); 9];
        for (n, piece) in row.iter_mut().enumerate() {
            *piece = place(ROW + 4 * n as u32)?;
        }
        let answer = |n: usize| text(format.lines.get(n)?.first()?.value);
        let answers_words = [answer(0)?, answer(1)?];
        Some(Self {
            answers_words,
            cursors: cursors.try_into().ok()?,
            slots,
            ports: [port_1, port_2],
            list,
            answers: [format, ask, title],
            headings: [sprite(HEADINGS)?, sprite(HEADINGS + 8)?],
            on_ports: sprites(ON_PORTS, &|number| number < 0)?,
            place_kinds: [
                sprite(PLACE_KINDS)?,
                sprite(PLACE_KINDS + 8)?,
                sprite(PLACE_KINDS + 16)?,
            ],
            row,
            labels: sprites(LABELS, &|number| number <= 0)?,
            area: rect(AREA)?,
            frames: frames.try_into().ok()?,
            texts: texts?.try_into().ok()?,
            help: help?.try_into().ok()?,
            help_of_slots: [
                text(image.u32(HELP_OF_SLOTS).ok()?)?,
                text(image.u32(HELP_OF_SLOTS + 4).ok()?)?,
            ],
            legend,
            messages,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::cursor;
    use crate::pad::{self, Repeat};

    /// The pad's words of a pass in which the buttons went down.
    fn press(buttons: u16) -> Pad {
        let mut pad = Pad::default();
        pad.read(0, Repeat::of(1), 0);
        pad.read(buttons, Repeat::of(1), 1);
        pad
    }

    /// The cursor records as the file has them.
    fn records() -> [Cursor; 6] {
        let record = |columns, rows, lines, flags, at| Cursor {
            columns,
            rows,
            lines,
            flags,
            at,
            first: 0,
        };
        [
            record(1, 4, 4, 0, 0),
            record(2, 1, 2, 0, 0),
            record(1, 2, 2, 0, 1),
            record(1, 4, 15, cursor::SCROLLS, 0),
            record(2, 1, 2, cursor::WRAPS_ACROSS, 0),
            record(2, 1, 2, 0, 1),
        ]
    }

    /// A game whose save is its number in a buffer with a head.
    struct Numbered {
        number: u8,
        took: Option<u8>,
    }

    impl Game for Numbered {
        fn write(&mut self) -> Vec<u8> {
            let mut buffer = vec![0; BUFFER];
            buffer[save::HEAD] = self.number;
            buffer[save::GAME] = self.number;
            buffer
        }

        fn read(&mut self, buffer: &[u8]) -> bool {
            if buffer[save::GAME] == 0 {
                return false;
            }
            self.took = Some(buffer[save::GAME]);
            true
        }
    }

    fn game(number: u8) -> Numbered {
        Numbered { number, took: None }
    }

    /// Passes without a button until the screen is in a state.
    fn until(screen: &mut CardScreen, cards: &mut Cards, game: &mut Numbered, state: i16) {
        for _ in 0..40 {
            if screen.state == state && screen.wanted == -1 && screen.routine == Routine::States {
                return;
            }
            screen.pass(&Pad::default(), cards, game);
        }
        panic!("state {} is not reached from {}", state, screen.state);
    }

    #[test]
    fn a_save_from_a_room_goes_through_the_port_the_slots_and_the_question() {
        let (mut cards, mut game) = (Cards::default(), game(7));
        let mut screen = CardScreen::open(Kind::Room, records());
        assert_eq!(
            (screen.state, screen.line(), screen.word),
            (state::LIST, 0, 0)
        );
        // The confirm on "save" asks for the port, which the next pass takes.
        let passed = screen.pass(&press(pad::CROSS), &mut cards, &mut game);
        assert_eq!(
            (passed.sounds, screen.state),
            (vec![SOUND_CONFIRM], state::LIST)
        );
        screen.pass(&press(pad::RIGHT), &mut cards, &mut game);
        assert_eq!((screen.state, screen.port()), (state::PORT, 1));
        screen.pass(&press(pad::CROSS), &mut cards, &mut game);
        // The check takes a pass to start and one to answer; an empty card with free
        // blocks leads to the slots.
        until(&mut screen, &mut cards, &mut game, state::SLOTS);
        assert_eq!((screen.free, screen.saves, screen.count), (15, 0, 1));
        // A slot cannot be picked before its head is in: the count is two above it.
        assert!(!screen.is_read(0));
        screen.pass(&press(pad::DOWN), &mut cards, &mut game);
        assert!(screen.is_read(0) && !screen.is_read(1));
        for _ in 0..SLOTS {
            screen.pass(&Pad::default(), &mut cards, &mut game);
        }
        assert_eq!(screen.count, 16);
        let passed = screen.pass(&press(pad::CROSS), &mut cards, &mut game);
        assert_eq!(passed.sounds, [SOUND_CONFIRM]);
        // The question of a save stands on its second answer.
        screen.pass(&Pad::default(), &mut cards, &mut game);
        assert_eq!((screen.state, screen.cursors[4].at), (state::ASK, 1));
        let passed = screen.pass(&press(pad::CROSS), &mut cards, &mut game);
        assert_eq!(
            (passed.sounds, screen.state),
            (vec![SOUND_CANCEL], state::ASK)
        );
        screen.pass(&Pad::default(), &mut cards, &mut game);
        assert_eq!(screen.state, state::SLOTS);
        screen.pass(&press(pad::CROSS), &mut cards, &mut game);
        screen.pass(&press(pad::LEFT), &mut cards, &mut game);
        assert_eq!((screen.state, screen.cursors[4].at), (state::ASK, 0));
        screen.pass(&press(pad::CROSS), &mut cards, &mut game);
        assert_eq!(screen.deed, deed::SAVE);
        // The writer, the write, and the pass that finds it over.
        screen.pass(&Pad::default(), &mut cards, &mut game);
        assert_eq!(
            (screen.routine, screen.message()),
            (Routine::Saving, Some(3))
        );
        screen.pass(&Pad::default(), &mut cards, &mut game);
        let passed = screen.pass(&Pad::default(), &mut cards, &mut game);
        assert_eq!(passed.sounds, [SOUND_CONFIRM]);
        assert_eq!((screen.routine, screen.wanted), (Routine::States, 3));
        assert_eq!((screen.heads[1][0], screen.free, screen.saves), (7, 14, 1));
        assert!(screen.filled[1] && cards.has(1, 1) && !cards.has(0, 1));
        assert_eq!(cards.files[1][1].as_ref().map(Vec::len), Some(BLOCK));
        // The cancel walks back: the port, the first list, the end.
        for wanted in [state::PORT, state::LIST] {
            screen.pass(&press(pad::TRIANGLE), &mut cards, &mut game);
            assert_eq!(screen.wanted, i32::from(wanted));
        }
        screen.pass(&press(pad::TRIANGLE), &mut cards, &mut game);
        assert_eq!(screen.deed, deed::END);
        screen.pass(&Pad::default(), &mut cards, &mut game);
        let passed = screen.pass(&Pad::default(), &mut cards, &mut game);
        assert_eq!(passed.ended, Some(0));
    }

    #[test]
    fn continue_loads_a_game_and_ends_the_screen_at_once() {
        let mut cards = Cards::default();
        let mut buffer = vec![0; BUFFER];
        buffer[save::GAME] = 9;
        cards.write(0, 2, &buffer);
        cards.write(0, 3, &vec![0; BUFFER]);
        let mut game = game(1);
        let mut screen = CardScreen::open(Kind::Continue, records());
        assert_eq!((screen.state, screen.line()), (state::PORT, line::LOAD));
        screen.pass(&press(pad::CROSS), &mut cards, &mut game);
        until(&mut screen, &mut cards, &mut game, state::SLOTS);
        assert_eq!((screen.free, screen.saves), (13, 2));
        for _ in 0..2 * SLOTS {
            screen.pass(&Pad::default(), &mut cards, &mut game);
        }
        // An empty slot is refused for a load.
        let passed = screen.pass(&press(pad::CROSS), &mut cards, &mut game);
        assert_eq!((passed.sounds, screen.wanted), (vec![SOUND_REFUSED], -1));
        // A file the reader refuses leads to the slots with the line of damaged data.
        for _ in 0..3 {
            screen.pass(&press(pad::DOWN), &mut cards, &mut game);
        }
        assert_eq!(screen.slot(), 3);
        screen.pass(&press(pad::CROSS), &mut cards, &mut game);
        screen.pass(&Pad::default(), &mut cards, &mut game);
        // The question of a load stands on its first answer.
        assert_eq!((screen.state, screen.cursors[4].at), (state::ASK, 0));
        screen.pass(&press(pad::CROSS), &mut cards, &mut game);
        until(&mut screen, &mut cards, &mut game, state::DAMAGED);
        assert!(!screen.loaded && game.took.is_none());
        screen.pass(&press(pad::SQUARE), &mut cards, &mut game);
        screen.pass(&press(pad::UP), &mut cards, &mut game);
        assert_eq!((screen.state, screen.slot()), (state::SLOTS, 2));
        screen.pass(&press(pad::CROSS), &mut cards, &mut game);
        screen.pass(&Pad::default(), &mut cards, &mut game);
        screen.pass(&press(pad::CROSS), &mut cards, &mut game);
        let mut ended = None;
        for _ in 0..6 {
            ended = ended.or(screen.pass(&Pad::default(), &mut cards, &mut game).ended);
        }
        assert_eq!(ended, Some(word::OF_TITLE | word::LOADED));
        assert_eq!((screen.loaded, game.took), (true, Some(9)));
        // A cancel on the port of "Continue" ends the screen: it has no first list.
        let mut screen = CardScreen::open(Kind::Continue, records());
        let passed = screen.pass(&press(pad::TRIANGLE), &mut cards, &mut game);
        assert_eq!(
            (passed.sounds, screen.deed),
            (vec![SOUND_CANCEL], deed::END)
        );
    }

    #[test]
    fn the_screen_for_a_pc_leads_from_the_first_list_straight_to_the_saves() {
        let (mut cards, mut game) = (Cards::default(), game(7));
        let mut screen = CardScreen::open_plain(Kind::Room, records(), &cards);
        assert_eq!(
            (screen.shape, screen.state, screen.line()),
            (Shape::Plain, state::LIST, line::SAVE)
        );
        // The confirm on "save" asks for the slots at once: no port, no check.
        let passed = screen.pass(&press(pad::CROSS), &mut cards, &mut game);
        assert_eq!(
            (passed.sounds, screen.wanted),
            (vec![SOUND_CONFIRM], i32::from(state::SLOTS))
        );
        screen.pass(&Pad::default(), &mut cards, &mut game);
        assert_eq!(
            (screen.state, screen.port(), screen.free, screen.saves),
            (state::SLOTS, 0, 15, 0)
        );
        for _ in 0..SLOTS {
            screen.pass(&Pad::default(), &mut cards, &mut game);
        }
        // The slot, the question, its first answer, and the save, with no message.
        screen.pass(&press(pad::CROSS), &mut cards, &mut game);
        screen.pass(&press(pad::LEFT), &mut cards, &mut game);
        assert_eq!((screen.state, screen.cursors[4].at), (state::ASK, 0));
        screen.pass(&press(pad::CROSS), &mut cards, &mut game);
        screen.pass(&Pad::default(), &mut cards, &mut game);
        assert_eq!((screen.routine, screen.message()), (Routine::Saving, None));
        until(&mut screen, &mut cards, &mut game, state::SLOTS);
        assert_eq!((screen.heads[0][0], screen.free, screen.saves), (7, 14, 1));
        assert!(cards.has(0, 0) && !cards.has(1, 0));
        // The cancel walks back to the first list, and from there to the end.
        let passed = screen.pass(&press(pad::TRIANGLE), &mut cards, &mut game);
        assert_eq!(passed.sounds, [SOUND_CANCEL]);
        screen.pass(&Pad::default(), &mut cards, &mut game);
        assert_eq!((screen.state, screen.line()), (state::LIST, line::SAVE));
        // "Load" of the first list shows the same saves.
        screen.pass(&press(pad::DOWN), &mut cards, &mut game);
        screen.pass(&press(pad::CROSS), &mut cards, &mut game);
        screen.pass(&Pad::default(), &mut cards, &mut game);
        assert_eq!(
            (screen.state, screen.line(), screen.saves, screen.slot()),
            (state::SLOTS, line::LOAD, 1, 0)
        );
        screen.pass(&press(pad::TRIANGLE), &mut cards, &mut game);
        screen.pass(&Pad::default(), &mut cards, &mut game);
        screen.pass(&press(pad::TRIANGLE), &mut cards, &mut game);
        assert_eq!(screen.deed, deed::END);
    }

    #[test]
    fn the_square_deletes_a_save_of_the_screen_for_a_pc_behind_a_question() {
        let mut cards = Cards::default();
        let mut buffer = vec![0; BUFFER];
        (buffer[save::HEAD], buffer[save::GAME]) = (9, 9);
        cards.write(0, 0, &buffer);
        cards.write(0, 1, &buffer);
        let mut game = game(1);
        let mut screen = CardScreen::open_plain(Kind::Continue, records(), &cards);
        for _ in 0..2 * SLOTS {
            screen.pass(&Pad::default(), &mut cards, &mut game);
        }
        assert_eq!((screen.saves, screen.free, screen.heads[1][0]), (2, 13, 9));
        // The square on a slot without a save is refused.
        for _ in 0..2 {
            screen.pass(&press(pad::DOWN), &mut cards, &mut game);
        }
        let passed = screen.pass(&press(pad::SQUARE), &mut cards, &mut game);
        assert_eq!((passed.sounds, screen.wanted), (vec![SOUND_REFUSED], -1));
        // On a save it asks, with the cursor on the second answer, which leads back.
        screen.pass(&press(pad::UP), &mut cards, &mut game);
        let passed = screen.pass(&press(pad::SQUARE), &mut cards, &mut game);
        assert_eq!(passed.sounds, [SOUND_CONFIRM]);
        screen.pass(&Pad::default(), &mut cards, &mut game);
        assert_eq!((screen.state, screen.cursors[4].at), (state::DELETE, 1));
        let passed = screen.pass(&press(pad::CROSS), &mut cards, &mut game);
        assert_eq!(passed.sounds, [SOUND_CANCEL]);
        screen.pass(&Pad::default(), &mut cards, &mut game);
        assert_eq!((screen.state, screen.saves), (state::SLOTS, 2));
        // The first answer takes the save off the list, and its row is empty.
        screen.pass(&press(pad::SQUARE), &mut cards, &mut game);
        let passed = screen.pass(&press(pad::LEFT), &mut cards, &mut game);
        assert_eq!(
            (passed.sounds, screen.cursors[4].at),
            (vec![SOUND_CURSOR], 0)
        );
        let passed = screen.pass(&press(pad::CROSS), &mut cards, &mut game);
        assert_eq!(passed.sounds, [SOUND_CONFIRM]);
        screen.pass(&Pad::default(), &mut cards, &mut game);
        assert_eq!(
            (screen.state, screen.slot(), screen.saves, screen.free),
            (state::SLOTS, 1, 1, 14)
        );
        assert!(!screen.filled[1] && screen.filled[0] && screen.heads[1][0] == 0);
        assert!(!cards.has(0, 1) && cards.has(0, 0));
        // The game's own screen knows no square.
        let mut screen = CardScreen::open(Kind::Continue, records());
        screen.pass(&press(pad::CROSS), &mut cards, &mut game);
        until(&mut screen, &mut cards, &mut game, state::SLOTS);
        for _ in 0..2 * SLOTS {
            screen.pass(&Pad::default(), &mut cards, &mut game);
        }
        let passed = screen.pass(&press(pad::SQUARE), &mut cards, &mut game);
        assert_eq!((passed.sounds, screen.wanted), (vec![], -1));
    }

    #[test]
    fn the_save_behind_the_end_has_no_first_list() {
        // The game's own: the port, with the first list on "save"; a cancel ends it.
        let (mut cards, mut game) = (Cards::default(), game(7));
        let mut screen = CardScreen::open(Kind::Finished, records());
        assert_eq!(
            (screen.state, screen.line(), screen.word),
            (state::PORT, line::SAVE, word::NO_LIST)
        );
        screen.pass(&press(pad::TRIANGLE), &mut cards, &mut game);
        assert_eq!(screen.deed, deed::END);
        // For a PC it starts on the saves, saves, and a cancel ends it with its word.
        let mut screen = CardScreen::open_plain(Kind::Finished, records(), &cards);
        assert_eq!(
            (screen.state, screen.line(), screen.saves),
            (state::SLOTS, line::SAVE, 0)
        );
        for _ in 0..SLOTS {
            screen.pass(&Pad::default(), &mut cards, &mut game);
        }
        screen.pass(&press(pad::CROSS), &mut cards, &mut game);
        screen.pass(&press(pad::LEFT), &mut cards, &mut game);
        assert_eq!((screen.state, screen.cursors[4].at), (state::ASK, 0));
        screen.pass(&press(pad::CROSS), &mut cards, &mut game);
        screen.pass(&Pad::default(), &mut cards, &mut game);
        until(&mut screen, &mut cards, &mut game, state::SLOTS);
        assert_eq!((screen.heads[0][0], screen.saves), (7, 1));
        let passed = screen.pass(&press(pad::TRIANGLE), &mut cards, &mut game);
        assert_eq!(passed.sounds, [SOUND_CANCEL]);
        screen.pass(&Pad::default(), &mut cards, &mut game);
        assert_eq!(screen.deed, deed::END);
        let mut ended = None;
        for _ in 0..3 {
            ended = ended.or(screen.pass(&Pad::default(), &mut cards, &mut game).ended);
        }
        assert_eq!(ended, Some(word::NO_LIST));
    }

    #[test]
    fn continue_for_a_pc_starts_on_the_saves_and_a_cancel_ends_it() {
        let mut cards = Cards::default();
        let mut buffer = vec![0; BUFFER];
        buffer[save::GAME] = 9;
        cards.write(0, 2, &buffer);
        let mut game = game(1);
        let mut screen = CardScreen::open_plain(Kind::Continue, records(), &cards);
        assert_eq!(
            (screen.state, screen.line(), screen.saves, screen.free),
            (state::SLOTS, line::LOAD, 1, 14)
        );
        for _ in 0..2 * SLOTS {
            screen.pass(&Pad::default(), &mut cards, &mut game);
        }
        // An empty slot is refused; the save is taken and ends the screen.
        let passed = screen.pass(&press(pad::CROSS), &mut cards, &mut game);
        assert_eq!(passed.sounds, [SOUND_REFUSED]);
        for _ in 0..2 {
            screen.pass(&press(pad::DOWN), &mut cards, &mut game);
        }
        screen.pass(&press(pad::CROSS), &mut cards, &mut game);
        screen.pass(&Pad::default(), &mut cards, &mut game);
        screen.pass(&press(pad::CROSS), &mut cards, &mut game);
        let mut ended = None;
        for _ in 0..6 {
            let passed = screen.pass(&Pad::default(), &mut cards, &mut game);
            assert_eq!(screen.message(), None);
            ended = ended.or(passed.ended);
        }
        assert_eq!(ended, Some(word::OF_TITLE | word::LOADED));
        assert_eq!(game.took, Some(9));
        // It has no first list to go back to: a cancel on the saves ends the screen.
        let mut screen = CardScreen::open_plain(Kind::Continue, records(), &cards);
        let passed = screen.pass(&press(pad::TRIANGLE), &mut cards, &mut game);
        assert_eq!(passed.sounds, [SOUND_CANCEL]);
        screen.pass(&Pad::default(), &mut cards, &mut game);
        assert_eq!(screen.deed, deed::END);
        screen.pass(&Pad::default(), &mut cards, &mut game);
        let passed = screen.pass(&Pad::default(), &mut cards, &mut game);
        assert_eq!(passed.ended, Some(word::OF_TITLE));
    }

    /// A card that answers the checks with what it is told, with no room on it.
    struct Told {
        present: i32,
        check: i32,
        format: i32,
        formats: u32,
    }

    impl Card for Told {
        fn present(&mut self, _port: u8) -> i32 {
            self.present
        }
        fn check(&mut self, _port: u8) -> i32 {
            self.check
        }
        fn format(&mut self, _port: u8) -> i32 {
            self.formats += 1;
            self.format
        }
        fn blocks(&self, _port: u8) -> i32 {
            15
        }
        fn has(&self, _port: u8, _slot: usize) -> bool {
            false
        }
        fn read(&mut self, _port: u8, _slot: usize) -> Result<Vec<u8>, i32> {
            Err(5)
        }
        fn write(&mut self, _port: u8, _slot: usize, _buffer: &[u8]) -> i32 {
            1
        }
    }

    /// The screen of a room in the check of the card, for a line of the first list.
    fn checked(line: i16, card: &mut Told) -> CardScreen {
        let mut game = game(1);
        let mut screen = CardScreen::open(Kind::Room, records());
        screen.cursors[0].at = line;
        screen.pass(&press(pad::CROSS), card, &mut game);
        screen.pass(&press(pad::CROSS), card, &mut game);
        for _ in 0..4 {
            screen.pass(&Pad::default(), card, &mut game);
        }
        screen
    }

    #[test]
    fn the_check_of_a_card_leads_to_a_message_when_nothing_can_be_picked() {
        let told = |check| Told {
            present: 1,
            check,
            format: 0,
            formats: 0,
        };
        let mut game = game(1);
        // A full card without a save of this game, for a save and for a load.
        let mut card = told(0);
        let screen = checked(line::SAVE, &mut card);
        assert_eq!((screen.state, screen.message()), (state::FULL, Some(0)));
        let mut screen = checked(line::LOAD, &mut card);
        assert_eq!((screen.state, screen.message()), (state::NO_SAVE, Some(4)));
        // Any button goes back to the port.
        screen.pass(&press(pad::SELECT), &mut card, &mut game);
        screen.pass(&Pad::default(), &mut card, &mut game);
        assert_eq!(screen.state, state::PORT);
        // No card: the ports stay, and the check starts again when a card is in.
        let mut card = told(1);
        let mut screen = checked(line::SAVE, &mut card);
        assert_eq!((screen.state, screen.message()), (state::NO_CARD, None));
        card.present = 0;
        card.check = 4;
        for _ in 0..5 {
            screen.pass(&Pad::default(), &mut card, &mut game);
        }
        // Not formatted: a load only says so, a save asks, with the cursor on "no".
        assert_eq!((screen.state, screen.cursors[2].at), (state::FORMAT, 1));
        let screen = checked(line::LOAD, &mut card);
        assert_eq!(screen.state, state::UNFORMATTED);
    }

    #[test]
    fn the_formatting_shows_its_message_for_two_passes_and_then_formats() {
        let mut card = Told {
            present: 0,
            check: 4,
            format: 2,
            formats: 0,
        };
        let mut game = game(1);
        let mut screen = checked(line::SAVE, &mut card);
        assert_eq!(screen.state, state::FORMAT);
        screen.pass(&press(pad::UP), &mut card, &mut game);
        let passed = screen.pass(&press(pad::CROSS), &mut card, &mut game);
        assert_eq!(
            (passed.sounds, screen.deed),
            (vec![SOUND_CONFIRM], deed::FORMAT)
        );
        screen.pass(&Pad::default(), &mut card, &mut game);
        for step in 0..2 {
            assert_eq!(screen.routine, Routine::Formatting(step));
            assert_eq!((screen.message(), card.formats), (Some(1), 0));
            screen.pass(&Pad::default(), &mut card, &mut game);
        }
        screen.pass(&Pad::default(), &mut card, &mut game);
        assert_eq!((card.formats, screen.routine), (1, Routine::States));
        screen.pass(&Pad::default(), &mut card, &mut game);
        assert_eq!(
            (screen.state, screen.message()),
            (state::FORMAT_FAILED, Some(6))
        );
        // A formatting that is done leads to the slots with the heads to be read anew.
        card.format = 0;
        screen.routine = Routine::Formatting(2);
        screen.count = 9;
        screen.pass(&Pad::default(), &mut card, &mut game);
        assert_eq!((screen.wanted, screen.count), (i32::from(state::SLOTS), 0));
    }

    #[test]
    fn the_last_line_asks_for_the_title_and_the_third_goes_on() {
        let (mut cards, mut game) = (Cards::default(), game(1));
        let mut screen = CardScreen::open(Kind::Room, records());
        for _ in 0..3 {
            screen.pass(&press(pad::DOWN), &mut cards, &mut game);
        }
        // The first list does not wrap.
        screen.pass(&press(pad::DOWN), &mut cards, &mut game);
        assert_eq!(screen.line(), line::LEAVE);
        screen.pass(&press(pad::CROSS), &mut cards, &mut game);
        screen.pass(&Pad::default(), &mut cards, &mut game);
        // The question stands on "no": a confirm there goes back with the cancel's sound.
        assert_eq!((screen.state, screen.cursors[5].at), (state::TITLE, 1));
        let passed = screen.pass(&press(pad::CROSS), &mut cards, &mut game);
        assert_eq!((passed.sounds, screen.wanted), (vec![SOUND_CANCEL], 0));
        screen.pass(&press(pad::CROSS), &mut cards, &mut game);
        screen.pass(&press(pad::LEFT), &mut cards, &mut game);
        let passed = screen.pass(&press(pad::CROSS), &mut cards, &mut game);
        assert_eq!(passed.sounds, [SOUND_CONFIRM]);
        assert_eq!((screen.word, screen.deed), (word::TO_TITLE, deed::END));
        // "Continue" of the list ends the screen with nothing in the word.
        let mut screen = CardScreen::open(Kind::Room, records());
        screen.cursors[0].at = line::GO_ON;
        screen.pass(&press(pad::CROSS), &mut cards, &mut game);
        screen.pass(&Pad::default(), &mut cards, &mut game);
        let passed = screen.pass(&Pad::default(), &mut cards, &mut game);
        assert_eq!(passed.ended, Some(0));
    }

    #[test]
    fn a_save_to_a_free_slot_of_a_full_card_is_refused_with_its_line() {
        let mut cards = Cards::default();
        for slot in 0..SLOTS - 1 {
            cards.write(0, slot, &vec![0; BUFFER]);
        }
        // One more file of another game takes the last block.
        struct Crowded(Cards);
        impl Card for Crowded {
            fn blocks(&self, port: u8) -> i32 {
                self.0.blocks(port) + 1
            }
            fn has(&self, port: u8, slot: usize) -> bool {
                self.0.has(port, slot)
            }
            fn read(&mut self, port: u8, slot: usize) -> Result<Vec<u8>, i32> {
                self.0.read(port, slot)
            }
            fn write(&mut self, port: u8, slot: usize, buffer: &[u8]) -> i32 {
                self.0.write(port, slot, buffer)
            }
        }
        let (mut card, mut game) = (Crowded(cards), game(1));
        let mut screen = CardScreen::open(Kind::Room, records());
        screen.pass(&press(pad::CROSS), &mut card, &mut game);
        screen.pass(&press(pad::CROSS), &mut card, &mut game);
        for _ in 0..40 {
            screen.pass(&Pad::default(), &mut card, &mut game);
        }
        assert_eq!(
            (screen.state, screen.free, screen.count),
            (state::SLOTS, 0, 16)
        );
        // The list scrolls to the last slot, the free one.
        for _ in 0..SLOTS - 1 {
            screen.pass(&press(pad::DOWN), &mut card, &mut game);
        }
        assert_eq!((screen.slot(), screen.cursors[3].first), (14, 11));
        let passed = screen.pass(&press(pad::CROSS), &mut card, &mut game);
        assert_eq!(passed.sounds, [SOUND_REFUSED]);
        screen.pass(&Pad::default(), &mut card, &mut game);
        assert_eq!(screen.state, state::NO_ROOM);
        // A slot with a file can still be written over.
        screen.pass(&press(pad::START), &mut card, &mut game);
        screen.pass(&press(pad::UP), &mut card, &mut game);
        let passed = screen.pass(&press(pad::CROSS), &mut card, &mut game);
        assert_eq!((passed.sounds, screen.wanted), (vec![SOUND_CONFIRM], 4));
    }

    /// A card with the build's slots behind its fifteen: the quick save and the
    /// autosaves.
    struct WithExtra {
        cards: Cards,
        extra: [Option<Vec<u8>>; EXTRA],
    }

    impl Card for WithExtra {
        fn blocks(&self, port: u8) -> i32 {
            self.cards.blocks(port)
        }

        fn has(&self, port: u8, slot: usize) -> bool {
            match slot.checked_sub(SLOTS) {
                Some(extra) => self.extra.get(extra).is_some_and(Option::is_some),
                None => self.cards.has(port, slot),
            }
        }

        fn read(&mut self, port: u8, slot: usize) -> Result<Vec<u8>, i32> {
            match slot.checked_sub(SLOTS) {
                Some(extra) => self.extra.get(extra).cloned().flatten().ok_or(5),
                None => self.cards.read(port, slot),
            }
        }

        fn write(&mut self, port: u8, slot: usize, buffer: &[u8]) -> i32 {
            match slot < SLOTS {
                true => self.cards.write(port, slot, buffer),
                false => 1,
            }
        }

        fn extra(&self) -> usize {
            EXTRA
        }
    }

    #[test]
    fn a_list_of_loads_has_the_quick_save_and_the_autosaves_above_the_slots() {
        let numbered = |number: u8| {
            let mut buffer = vec![0; BUFFER];
            (buffer[save::HEAD], buffer[save::GAME]) = (number, number);
            buffer
        };
        let mut cards = Cards::default();
        cards.write(0, 0, &numbered(3));
        let mut card = WithExtra {
            cards,
            extra: [Some(numbered(5)), None, Some(numbered(6)), None],
        };
        let mut game = game(1);
        let mut screen = CardScreen::open_plain(Kind::Continue, records(), &card);
        assert_eq!((screen.extra, screen.rows()), (EXTRA, SLOTS + EXTRA));
        assert_eq!(screen.cursors[3].lines as usize, SLOTS + EXTRA);
        for _ in 0..2 * (SLOTS + EXTRA) {
            screen.pass(&Pad::default(), &mut card, &mut game);
        }
        // The quick save is the first row, the first of the fifteen the fifth.
        assert_eq!(screen.saves, 3);
        assert_eq!(
            (
                screen.heads[0][0],
                screen.heads[2][0],
                screen.heads[EXTRA][0]
            ),
            (5, 6, 3)
        );
        assert_eq!((screen.row(), screen.slot()), (0, SLOTS));
        assert_eq!((screen.slot_of(3), screen.slot_of(EXTRA)), (SLOTS + 3, 0));
        // Its load reads the card's slot behind the fifteen.
        screen.pass(&press(pad::CROSS), &mut card, &mut game);
        screen.pass(&Pad::default(), &mut card, &mut game);
        assert_eq!((screen.state, screen.cursors[4].at), (state::ASK, 0));
        screen.pass(&press(pad::CROSS), &mut card, &mut game);
        for _ in 0..4 {
            screen.pass(&Pad::default(), &mut card, &mut game);
        }
        assert_eq!((game.took, screen.deed), (Some(5), deed::LOADED));
        // A room's list of saves has only the fifteen.
        let mut screen = CardScreen::open_plain(Kind::Room, records(), &card);
        screen.pass(&press(pad::CROSS), &mut card, &mut game);
        screen.pass(&Pad::default(), &mut card, &mut game);
        assert_eq!(
            (screen.state, screen.extra, screen.rows(), screen.slot()),
            (state::SLOTS, 0, SLOTS, 0)
        );
        assert_eq!(screen.cursors[3].lines as usize, SLOTS);
        // Its "Load" has the rows above again.
        screen.pass(&press(pad::TRIANGLE), &mut card, &mut game);
        screen.pass(&Pad::default(), &mut card, &mut game);
        screen.pass(&press(pad::DOWN), &mut card, &mut game);
        screen.pass(&press(pad::CROSS), &mut card, &mut game);
        screen.pass(&Pad::default(), &mut card, &mut game);
        assert_eq!((screen.line(), screen.extra), (line::LOAD, EXTRA));
    }

    #[test]
    fn the_words_of_the_build_are_wide_letters() {
        assert_eq!(wide(b"A1:")[0], 0x8260u16.swap_bytes());
        assert_eq!(wide(b"A1:")[1], 0x8250u16.swap_bytes());
        assert_eq!(wide(b"A1:")[2], 0x8146u16.swap_bytes());
        assert_eq!(EXTRA_NAMES.len(), EXTRA);
    }
}
