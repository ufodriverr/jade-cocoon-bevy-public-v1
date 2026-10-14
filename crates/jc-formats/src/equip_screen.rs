
use crate::carried::Carried;
use crate::items::{FREE, Kind};
use crate::menu::{self, Cursor, cursor};
use crate::pad::{self, Pad};
use crate::player_menu::{
    MESSAGE_PASSES, OPEN, SOUND_CANCEL, SOUND_CONFIRM, SOUND_CURSOR, SOUND_REFUSED,
};

/// The archive file of the screen and the RAM address of its first byte.
pub const EQUIP_FILE: usize = 50;
pub const EQUIP_FILE_BASE: u32 = 0x800C_2BB8;

/// The step of each depth, and the end behind them (the half words at `0x800C5B98`).
pub const STEPS: [i16; 4] = [KIND, LIST, PUT_ON, END];
/// The steps: the kind is picked, a thing of its list is picked, the thing is put on.
pub const KIND: i16 = 0;
pub const LIST: i16 = 1;
pub const PUT_ON: i16 = 2;
const END: i16 = -1;

/// The cursor record of each step: columns, rows, lines, flag word (the records of
/// `0x800C5BA8` that the bits of `0x800C5BA0` pick). The lines of the list are counted.
pub const CURSORS: [[i16; 4]; 3] = [
    [1, 3, 3, cursor::WRAPS_DOWN as i16],
    [1, 3, 64, cursor::SCROLLS as i16],
    [1, 1, 1, 0],
];
/// The rows of the list.
pub const LIST_ROWS: i16 = 3;
/// The kinds: a weapon, an armour, the third thing.
pub const KINDS: i16 = 3;

/// What a pass of the screen did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Passed {
    /// The sounds asked for, in their order.
    pub sounds: Vec<u8>,
    /// The equip routine took this thing of this kind ([`FREE`]: the place was empty).
    pub put_on: Option<(u8, i16)>,
    /// The task ended in this pass: the menu goes back as after a cancel.
    pub ended: bool,
}

/// The record of the screen's task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquipScreen {
    /// The kept cursors (+0) and first lines (+12): of the kind at 0, of each kind's
    /// list at "kind + 1".
    pub kept: [i16; 6],
    pub kept_first: [i16; 6],
    /// The kind the first step's cursor is on (+24).
    pub kind: i16,
    /// The depth (+26) and its step (+28).
    pub depth: i16,
    pub step: i16,
    confirm: bool,
    cancel: bool,
    pub cursor: Cursor,
    /// The thing under the list's cursor (+64), [`FREE`] for an empty place.
    pub thing: u8,
    pub openness: i16,
    /// The flags at +68: the description of the list's thing is up (`0x02`), the one of
    /// the worn thing (`0x04`), the message (`0x08`).
    pub describes_thing: bool,
    pub describes_worn: bool,
    pub message: bool,
    /// The passes the message has been up (+70).
    pub message_passes: i16,
    /// The top bit of the engine's flag word: the screen is up.
    pub up: bool,
}

impl Default for EquipScreen {
    fn default() -> Self {
        Self::open()
    }
}

impl EquipScreen {
    /// The screen as its start leaves it (`0x800C4990`): on the kinds, closed.
    pub fn open() -> Self {
        let [columns, rows, lines, flags] = CURSORS[KIND as usize];
        Self {
            kept: [0; 6],
            kept_first: [0; 6],
            kind: 0,
            depth: 0,
            step: KIND,
            confirm: false,
            cancel: false,
            cursor: Cursor {
                columns,
                rows,
                lines: lines as u16,
                flags: flags as u16,
                at: 0,
                first: 0,
            },
            thing: 0,
            openness: 0,
            describes_thing: false,
            describes_worn: false,
            message: false,
            message_passes: 0,
            up: true,
        }
    }

    /// The list of the kind the cursor is on.
    pub fn list(&self) -> Kind {
        Self::list_of(self.kind)
    }

    fn list_of(kind: i16) -> Kind {
        match kind {
            0 => Kind::Weapon,
            1 => Kind::Armour,
            _ => Kind::Ornament,
        }
    }

    /// The cursor and the first line the list of a kind is drawn with: the kept ones.
    pub fn list_place(&self, kind: i16) -> (i16, i16) {
        let at = usize::try_from(kind + 1).ok().filter(|at| *at < 6);
        at.map_or((0, 0), |at| (self.kept[at], self.kept_first[at]))
    }

    /// The lines of a kind's list: a line for every place up to the last in use, three
    /// at least, and for the third thing one more once the last place in use is the
    /// third or a later one, so that an empty line follows the last ornament.
    pub fn list_lines(carried: &Carried, kind: i16) -> u16 {
        let last = carried.keeping.last_place(Self::list_of(kind)) as i16;
        let extra = i16::from(kind == 2 && last >= 2);
        (last.max(2) + 1 + extra) as u16
    }

    /// One pass of the task with the pad's words of the pass (`0x800C4A9C`, and
    /// `0x800C52D8` while the message is up).
    pub fn pass(&mut self, carried: &mut Carried, pad: &Pad) -> Passed {
        let mut passed = Passed::default();
        if self.message {
            self.pass_message(carried, pad);
            return passed;
        }
        if !self.up {
            // The three lists are put in the order of their numbers, and the task ends.
            for kind in 0..KINDS {
                carried.keeping.tidy(Self::list_of(kind));
            }
            passed.ended = true;
            return passed;
        }
        self.input(pad, &mut passed);
        if self.confirm || self.cancel {
            self.act(carried);
        }
        if self.up {
            self.keep(carried, pad, &mut passed);
        }
        if self.openness < OPEN {
            self.openness += 1;
        }
        passed
    }

    /// The cursor record of the step (`0x800C4B64`).
    fn take_cursor(&mut self, carried: &Carried) {
        let Some([columns, rows, lines, flags]) = (usize::try_from(self.step).ok())
            .and_then(|step| CURSORS.get(step))
            .copied()
        else {
            return;
        };
        self.cursor.columns = columns;
        self.cursor.rows = rows;
        self.cursor.lines = lines as u16;
        self.cursor.flags = flags as u16;
        if self.step == LIST {
            self.cursor.lines = Self::list_lines(carried, self.kind);
        }
    }

    /// The pad is read (`0x800C4F38`): the cursor in every pass, confirm and cancel
    /// once the screen is whole. A confirm that leads to the putting on has no sound of
    /// its own.
    fn input(&mut self, pad: &Pad, passed: &mut Passed) {
        if self.cursor.step(pad.repeated).sounds {
            passed.sounds.push(SOUND_CURSOR);
        }
        if self.openness < OPEN {
            return;
        }
        if menu::confirmed(pad) {
            let next = STEPS.get(self.depth as usize + 1).copied().unwrap_or(END);
            if next == END {
                return;
            }
            if next != PUT_ON {
                passed.sounds.push(SOUND_CONFIRM);
            }
            self.confirm = true;
        } else if menu::cancelled(pad) {
            passed.sounds.push(SOUND_CANCEL);
            self.cancel = true;
        }
    }

    /// A noted confirm or cancel is acted on, in the pass that noted it (`0x800C4FDC`).
    fn act(&mut self, carried: &Carried) {
        let of_kind = (self.kind + 1) as usize;
        if self.confirm {
            if self.step == LIST {
                self.thing = self.under_cursor(carried);
            }
            let depth = self.depth as usize;
            self.kept[depth] = self.cursor.at;
            self.kept_first[depth] = self.cursor.first;
            self.cursor.at = self.kept[depth + 1];
            self.cursor.first = self.kept_first[depth + 1];
            self.depth += 1;
            self.step = STEPS[depth + 1];
            if self.step == LIST {
                self.cursor.at = self.kept[of_kind];
                self.cursor.first = self.kept_first[of_kind];
            } else {
                // Only the putting on starts the openness again.
                self.openness = 0;
            }
        } else {
            self.depth -= 1;
            let Ok(depth) = usize::try_from(self.depth) else {
                self.up = false;
                return;
            };
            self.step = STEPS[depth];
            self.cursor.at = self.kept[depth];
            self.cursor.first = self.kept_first[depth];
            if self.step == LIST {
                self.cursor.at = self.kept[of_kind];
                self.cursor.first = self.kept_first[of_kind];
            }
        }
        self.take_cursor(carried);
        self.confirm = false;
        self.cancel = false;
    }

    fn under_cursor(&self, carried: &Carried) -> u8 {
        let place = usize::try_from(self.cursor.at + self.cursor.first).unwrap_or(usize::MAX);
        carried.keeping.number_at(place, self.list())
    }

    /// The keeping of a pass (`0x800C5398`): the kind follows the first step's cursor,
    /// the list's cursor is kept for its kind, the putting on is done, and the held
    /// circle asks for a description.
    fn keep(&mut self, carried: &mut Carried, pad: &Pad, passed: &mut Passed) {
        match self.step {
            KIND => {
                self.kind = self.cursor.at;
                self.kept[0] = self.cursor.at;
            }
            LIST => {
                self.thing = self.under_cursor(carried);
                let of_kind = (self.kind + 1) as usize;
                self.kept[of_kind] = self.cursor.at;
                self.kept_first[of_kind] = self.cursor.first;
            }
            PUT_ON => self.put_on(carried, passed),
            _ => {}
        }
        let circle = pad.held & pad::CIRCLE != 0;
        let worn = usize::try_from(self.kind)
            .ok()
            .and_then(|kind| carried.worn.get(kind));
        self.describes_worn = circle && self.step == KIND && worn.is_some_and(|&on| on != FREE);
        self.describes_thing = circle && self.step == LIST && self.thing != FREE;
    }

    /// The step that puts the thing on (`0x800C524C`): the equip routine, and then the
    /// list again as after a cancel, or the message when the routine refuses.
    fn put_on(&mut self, carried: &mut Carried, passed: &mut Passed) {
        if carried.equip(i32::from(self.thing), i32::from(self.kind)) {
            passed.sounds.push(SOUND_CONFIRM);
            passed.put_on = Some((self.thing, self.kind));
            self.cancel = true;
            self.act(carried);
        } else {
            passed.sounds.push(SOUND_REFUSED);
            self.message = true;
        }
    }

    /// The message stands until any button goes down or its passes are over
    /// (`0x800C52D8`); then the screen is on the list again, without a sound.
    fn pass_message(&mut self, carried: &Carried, pad: &Pad) {
        if self.message_passes < MESSAGE_PASSES {
            self.message_passes += 1;
        }
        if pad.pressed != 0 || self.message_passes >= MESSAGE_PASSES {
            self.cancel = true;
            self.message_passes = 0;
            self.message = false;
            self.act(carried);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pad::Repeat;

    /// The pad's words of a pass in which the buttons went down.
    fn press(buttons: u16) -> Pad {
        let mut pad = Pad::default();
        pad.read(0, Repeat::of(1), 0);
        pad.read(buttons, Repeat::of(1), 1);
        pad
    }

    fn rest(screen: &mut EquipScreen, carried: &mut Carried) {
        while screen.openness < OPEN {
            screen.pass(carried, &Pad::default());
        }
    }

    /// A game with the first weapon on and, in its lists, weapons 1 and 2 and
    /// ornament 4.
    fn fitted() -> Carried {
        let mut carried = Carried::new_game();
        let worn = carried.worn;
        carried.keeping.give(2, 1, Kind::Weapon, worn);
        carried.keeping.give(1, 1, Kind::Weapon, worn);
        carried.keeping.give(4, 1, Kind::Ornament, worn);
        carried
    }

    #[test]
    fn the_first_step_is_deaf_for_eight_passes_and_picks_a_kind() {
        let mut carried = fitted();
        let mut screen = EquipScreen::open();
        for _ in 0..OPEN {
            let passed = screen.pass(&mut carried, &press(pad::CROSS));
            assert_eq!((screen.step, passed.sounds.len()), (KIND, 0));
        }
        // The cursor moves at once, wraps, and the kind follows it.
        let passed = screen.pass(&mut carried, &press(pad::UP));
        assert_eq!((screen.kind, passed.sounds), (2, vec![SOUND_CURSOR]));
        screen.pass(&mut carried, &press(pad::DOWN));
        assert_eq!(screen.kind, 0);
        // A confirm is acted on in its own pass.
        let passed = screen.pass(&mut carried, &press(pad::CROSS));
        assert_eq!(passed.sounds, [SOUND_CONFIRM]);
        assert_eq!(
            (screen.depth, screen.step, screen.openness),
            (1, LIST, OPEN)
        );
        assert_eq!((screen.cursor.rows, screen.cursor.lines), (LIST_ROWS, 3));
        assert_eq!(screen.cursor.flags, cursor::SCROLLS);
        assert_eq!(screen.thing, 2);
    }

    #[test]
    fn a_thing_of_the_list_is_put_on_and_the_old_one_comes_back() {
        let mut carried = fitted();
        let mut screen = EquipScreen::open();
        rest(&mut screen, &mut carried);
        screen.pass(&mut carried, &press(pad::CROSS));
        screen.pass(&mut carried, &press(pad::DOWN));
        assert_eq!((screen.thing, screen.list_place(0)), (1, (1, 0)));
        // The confirm has no sound of its own: the putting on has.
        let passed = screen.pass(&mut carried, &press(pad::CROSS));
        assert_eq!(passed.sounds, [SOUND_CONFIRM]);
        assert_eq!(passed.put_on, Some((1, 0)));
        assert_eq!(carried.worn[0], 1);
        // One of the new thing is gone from the list, the old one is in it.
        assert_eq!(carried.keeping.count(1, Kind::Weapon), 0);
        assert_eq!(carried.keeping.count(0, Kind::Weapon), 1);
        // The screen is on the list again, where it was, and deaf for a while.
        assert_eq!((screen.depth, screen.step), (1, LIST));
        assert_eq!((screen.cursor.at, screen.openness), (1, 1));
        let passed = screen.pass(&mut carried, &press(pad::TRIANGLE));
        assert_eq!((screen.step, passed.sounds.len()), (LIST, 0));
        // The lists are put in order when the screen ends.
        rest(&mut screen, &mut carried);
        screen.pass(&mut carried, &press(pad::TRIANGLE));
        assert_eq!(screen.step, KIND);
        let passed = screen.pass(&mut carried, &press(pad::TRIANGLE));
        assert_eq!((passed.sounds, screen.up), (vec![SOUND_CANCEL], false));
        let passed = screen.pass(&mut carried, &Pad::default());
        assert!(passed.ended);
        assert_eq!(carried.keeping.number_at(0, Kind::Weapon), 0);
        assert_eq!(carried.keeping.number_at(1, Kind::Weapon), 2);
    }

    #[test]
    fn an_empty_place_takes_the_third_thing_off_but_no_weapon() {
        let mut carried = fitted();
        let mut screen = EquipScreen::open();
        rest(&mut screen, &mut carried);
        // The weapons: places 0 and 1 are in use, the third row is empty.
        screen.pass(&mut carried, &press(pad::CROSS));
        screen.pass(&mut carried, &press(pad::DOWN));
        screen.pass(&mut carried, &press(pad::DOWN));
        assert_eq!(screen.thing, FREE);
        let passed = screen.pass(&mut carried, &press(pad::CROSS));
        assert_eq!(passed.sounds, [SOUND_REFUSED]);
        assert!(screen.message && passed.put_on.is_none());
        assert_eq!((carried.worn[0], screen.step), (0, PUT_ON));
        // The message goes with any button, and the list is back without a sound.
        screen.pass(&mut carried, &Pad::default());
        assert!(screen.message);
        let passed = screen.pass(&mut carried, &press(pad::SQUARE));
        assert_eq!(
            (screen.message, screen.step, passed.sounds.len()),
            (false, LIST, 0)
        );
        // The third thing: put on, then taken off by the empty row.
        rest(&mut screen, &mut carried);
        screen.pass(&mut carried, &press(pad::TRIANGLE));
        screen.pass(&mut carried, &press(pad::UP));
        assert_eq!(screen.kind, 2);
        screen.pass(&mut carried, &press(pad::CROSS));
        assert_eq!(screen.thing, 4);
        screen.pass(&mut carried, &press(pad::CROSS));
        assert_eq!(carried.worn[2], 4);
        rest(&mut screen, &mut carried);
        assert_eq!(screen.thing, FREE);
        let passed = screen.pass(&mut carried, &press(pad::CROSS));
        assert_eq!(passed.put_on, Some((FREE, 2)));
        assert_eq!(carried.worn[2], FREE);
        assert_eq!(carried.keeping.count(4, Kind::Ornament), 1);
    }

    #[test]
    fn the_message_ends_by_itself_and_the_circle_asks_for_a_description() {
        let mut carried = fitted();
        let mut screen = EquipScreen::open();
        rest(&mut screen, &mut carried);
        let mut held = Pad::default();
        held.read(pad::CIRCLE, Repeat::of(1), 0);
        held.read(pad::CIRCLE, Repeat::of(1), 1);
        // On the kinds: only for a kind that has something on.
        screen.pass(&mut carried, &held);
        assert!(screen.describes_worn && !screen.describes_thing);
        screen.pass(&mut carried, &press(pad::DOWN));
        screen.pass(&mut carried, &held);
        assert!(!screen.describes_worn);
        screen.pass(&mut carried, &press(pad::UP));
        // On the list: only for a row with a thing.
        screen.pass(&mut carried, &press(pad::CROSS));
        screen.pass(&mut carried, &held);
        assert!(screen.describes_thing && !screen.describes_worn);
        screen.pass(&mut carried, &Pad::default());
        assert!(!screen.describes_thing);
        screen.pass(&mut carried, &press(pad::UP));
        assert_eq!((screen.cursor.at, screen.thing), (0, 2));
        screen.cursor.at = 2;
        screen.pass(&mut carried, &held);
        assert!(!screen.describes_thing);
        // The refusal stands for 150 passes when no button goes down.
        screen.pass(&mut carried, &press(pad::CROSS));
        assert!(screen.message);
        for _ in 1..MESSAGE_PASSES {
            screen.pass(&mut carried, &Pad::default());
            assert!(screen.message);
        }
        screen.pass(&mut carried, &Pad::default());
        assert!(!screen.message);
        assert_eq!((screen.step, screen.message_passes), (LIST, 0));
    }

    #[test]
    fn the_list_of_the_third_things_has_an_empty_line_behind_the_last() {
        let mut carried = Carried::new_game();
        assert_eq!(EquipScreen::list_lines(&carried, 0), 3);
        assert_eq!(EquipScreen::list_lines(&carried, 2), 3);
        let worn = carried.worn;
        for thing in 0..3 {
            carried.keeping.give(thing, 1, Kind::Weapon, worn);
            carried.keeping.give(thing, 1, Kind::Ornament, worn);
        }
        assert_eq!(EquipScreen::list_lines(&carried, 0), 3);
        assert_eq!(EquipScreen::list_lines(&carried, 2), 4);
        carried.keeping.give(3, 1, Kind::Weapon, worn);
        assert_eq!(EquipScreen::list_lines(&carried, 0), 4);
    }
}
