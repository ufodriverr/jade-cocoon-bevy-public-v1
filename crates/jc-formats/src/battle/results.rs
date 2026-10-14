
use super::Tables;
use super::fight::Beaten;
use super::record::{Draws, Growth, LEVEL, Record};
use crate::items::{Keeping, Worn};
use crate::menu;
use crate::notice;
use crate::pad::{self, Pad};
use crate::player_menu::{MENU_FILE_BASE, SOUND_CANCEL, SOUND_CONFIRM, SOUND_CURSOR};
use crate::player_panel::rank;

/// The fighters of the player's side: he and up to three carried creatures.
pub const FIGHTERS: usize = 4;
/// The most of an experience word that is no level yet.
const WAY: u32 = LEVEL - 1;
/// A level as the sums count it.
const U: u128 = LEVEL as u128;
/// The levels the reading of a sum stops at.
const HIGHEST_READ: u32 = 99;
/// The table of the spoils in file 42: eight bytes a species.
const SPOILS: u32 = 0x800C_1278;
const SPOIL_ROWS: usize = 211;
/// The kinds of a command that name nothing done to an opponent, and a capture.
const UNTOUCHED: u32 = 7;
const CAPTURES: [u32; 2] = [5, 26];
/// The share of black of a rank's name that is gone, and its step a pass.
pub const DARK: i16 = 128;
const FADE: i16 = 4;
/// The list of the line of help in file 42: "Quit" and "Status".
pub const HELP: u32 = 0x800C_1924;
/// The way of the notice of the spoils: things handed over, each with its count.
pub const SPOILS_WAY: i32 = -1;

/// The bits of the task's flag word. Bit n + 1 says fighter n is gone through.
pub mod flag {
    /// The player is gone through.
    pub const PLAYER: u32 = 0x02;
    /// A panel of a level up is shown.
    pub const PANEL: u32 = 0x20;
    pub const LEAVES: u32 = 0x40;
    /// The counting is over.
    pub const COUNTED: u32 = 0x80;
}

/// What an opponent of a level is worth, and where a fighter of a level stands with
/// its way to the next: a level costs twice the level before (`0x80012204`).
fn worth(level: u32, way: u32) -> u128 {
    match level {
        0 => U,
        // The game counts in double precision and no fighter comes near level 100,
        // where this would leave the 128 bits.
        level => (U + u128::from(way)) << level.min(HIGHEST_READ),
    }
}

/// The experience word of a worth (`0x80012358`): the levels in bits 26 and up, the
/// way to the next below them.
fn read(total: u128) -> u32 {
    let mut rest = total.saturating_sub(U);
    let (mut levels, mut cost) = (0u32, U);
    while levels < HIGHEST_READ && cost <= rest {
        rest -= cost;
        cost *= 2;
        levels += 1;
    }
    (levels << 26).wrapping_add((rest / (cost / U)) as u32)
}

/// What a fighter of `level` with the experience word `way` gains from an opponent
/// of the level `opponent` (`0x8001247C`): an opponent of the own level is one level,
/// each level it is below halves that, each level above doubles it.
pub fn gain(level: u32, way: u32, opponent: u32) -> u32 {
    let had = (level << 26).wrapping_add(way);
    read(worth(level, way) + worth(opponent, 0)).wrapping_sub(had)
}

/// Adds one opponent to a fighter's sum (`0x800122A0`): a later one is counted from
/// where the sum so far would bring the fighter.
pub fn add(fighter: &Record, opponent: u8, sum: u32) -> u32 {
    let (level, opponent) = (u32::from(fighter.level), u32::from(opponent));
    if sum == 0 {
        return gain(level, fighter.experience, opponent);
    }
    let way = fighter.experience.wrapping_add(sum & WAY);
    let more = way / LEVEL;
    let level = level + (sum >> 26) + more;
    sum.wrapping_add(gain(level, way - more * LEVEL, opponent))
}

/// The sums of experience of a fight ("Who gets experience"): only the one that
/// finished an opponent gets it, the player only for a capture, and the dead nothing.
/// `fighters` are the player and his carried creatures in slot order.
pub fn sums(beaten: &[Beaten; 3], fighters: &[Record]) -> [u32; FIGHTERS] {
    let mut sums = [0; FIGHTERS];
    for record in beaten.iter().filter(|record| record.kind != UNTOUCHED) {
        let gets = record.by != 0 || CAPTURES.contains(&record.kind);
        let (Some(who), Some(sum)) = (fighters.get(record.by), sums.get_mut(record.by)) else {
            continue;
        };
        if gets && !who.is_dead() {
            *sum = add(who, record.level, *sum);
        }
    }
    sums
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thing {
    pub kind: u8,
    pub number: u8,
}

/// What a species can leave: two things, each with its chance out of 32768.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spoil {
    pub first: (u16, Thing),
    pub second: (u16, Thing),
}

/// The table of the spoils.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spoils {
    pub rows: Vec<Spoil>,
}

impl Spoils {
    /// The table of file 42; `None` when the file does not hold it.
    pub fn from_file(file: &[u8]) -> Option<Self> {
        let start = (SPOILS - MENU_FILE_BASE) as usize;
        let table = file.get(start..start + 8 * SPOIL_ROWS)?;
        let half = |row: &[u8; 8], at: usize| {
            let thing = Thing {
                kind: row[at + 2],
                number: row[at + 3],
            };
            (u16::from_le_bytes([row[at], row[at + 1]]), thing)
        };
        Some(Self {
            rows: (table.as_chunks::<8>().0.iter())
                .map(|row| Spoil {
                    first: half(row, 0),
                    second: half(row, 4),
                })
                .collect(),
        })
    }

    /// What the opponents of a fight leave (`0x800BD400`): two draws for every record
    /// that has a species and was not left untouched, the first thing when the first
    /// draw is below its chance, else the second by the second draw. Who finished the
    /// opponent does not count.
    pub fn won(&self, beaten: &[Beaten; 3], draws: &mut impl Draws) -> [Option<Thing>; 3] {
        beaten.map(|record| {
            if record.species == -1 || record.kind == UNTOUCHED {
                return None;
            }
            let (first, second) = (draws.draw(), draws.draw());
            let row = self.rows.get(usize::try_from(record.species).ok()?)?;
            if first < u32::from(row.first.0) {
                Some(row.first.1)
            } else if second < u32::from(row.second.0) {
                Some(row.second.1)
            } else {
                None
            }
        })
    }
}

/// What a pass of the screen did, for the app.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Pass {
    /// The sounds asked for, in order.
    pub sounds: Vec<u8>,
    /// How often the music of a gained level was asked for (`0x8003013C(4, 1)`).
    pub jingles: u32,
    /// The rank's name as the player's panel draws it in this pass: whether it is the
    /// name of the rank before, and its share of black out of [`DARK`].
    pub name: Option<(bool, i16)>,
    /// The screen leaves for the room.
    pub leaves: bool,
    pub menu: bool,
}

/// The task of the results screen (`0x800BBD64` to `0x800BD400` of file 42).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Results {
    pub flags: u32,
    /// Each fighter's sum, what of it is counted, the levels the growth answered last,
    /// whether it gained a level, and the experience word as the counting shows it.
    pub sums: [u32; FIGHTERS],
    pub counted: [u32; FIGHTERS],
    pub levels: [u32; FIGHTERS],
    pub gained: [bool; FIGHTERS],
    pub shown: [u32; FIGHTERS],
    /// The working copies the counting changes, and the records as they came out of
    /// the fight.
    pub copies: Vec<Record>,
    pub before: Vec<Record>,
    /// The things won, by opponent. The screen hands them over in one notice window
    /// in front of its first pass.
    pub won: [Option<Thing>; 3],
    /// The share of black of the rank's name on the player's panel, and whether the
    /// name is still the one of the rank before.
    fade: i16,
    old_name: bool,
    /// The task sleeps: the screen has left or the menu was opened.
    over: bool,
}

impl Results {
    /// The entry and the first pass: the sums, the copies of the fighters (the player
    /// and his carried creatures in slot order, at most four) and the spoils.
    pub fn open(
        beaten: &[Beaten; 3],
        fighters: &[Record],
        spoils: &Spoils,
        draws: &mut impl Draws,
    ) -> Self {
        let fighters = &fighters[..fighters.len().min(FIGHTERS)];
        let mut shown = [0; FIGHTERS];
        for (shown, fighter) in shown.iter_mut().zip(fighters) {
            *shown = fighter.experience;
        }
        Self {
            flags: 0,
            sums: sums(beaten, fighters),
            counted: [0; FIGHTERS],
            levels: [0; FIGHTERS],
            gained: [false; FIGHTERS],
            shown,
            copies: fighters.to_vec(),
            before: fighters.to_vec(),
            won: spoils.won(beaten, draws),
            fade: 0,
            old_name: true,
            over: false,
        }
    }

    /// Whether fighter `n` is gone through.
    pub fn is_through(&self, n: usize) -> bool {
        self.flags & 1 << (n + 1) != 0
    }

    /// Whether every fighter is gone through: the line of help is shown, a confirm or
    /// a cancel leaves and the circle opens the menu.
    pub fn all_through(&self) -> bool {
        self.flags & 1 << self.copies.len() != 0
    }

    /// The fighter whose panel of a level up is shown.
    pub fn panel(&self) -> Option<usize> {
        if self.flags & flag::PANEL == 0 {
            return None;
        }
        match self.flags & flag::PLAYER {
            0 => Some(0),
            _ => (1..self.copies.len()).find(|n| !self.is_through(*n)),
        }
    }

    /// Whether the task sleeps.
    pub fn is_over(&self) -> bool {
        self.over
    }

    /// One pass of the screen (`0x800BC118`). `real` are the fighters' own records in
    /// the order of [`Results::open`]: from the end of the counting on, every pass
    /// writes the working copies over them. `None` when the tables have no steps of
    /// growth.
    pub fn pass(&mut self, tables: &Tables, pad: &Pad, real: &mut [Record]) -> Option<Pass> {
        let mut pass = Pass::default();
        if self.over {
            return Some(pass);
        }
        if self.flags & flag::COUNTED == 0 {
            self.count(tables, pad, &mut pass)?;
        } else if self.flags & flag::LEAVES == 0 {
            for (real, copy) in real.iter_mut().zip(&self.copies) {
                *real = copy.clone();
            }
            self.step(pad, &mut pass);
        }
        if self.flags & (flag::PANEL | flag::PLAYER) == flag::PANEL {
            pass.name = Some((self.old_name, self.fade));
            if self.fade >= DARK {
                self.old_name = false;
            }
            if self.old_name {
                self.fade += FADE;
            } else if self.fade > FADE {
                self.fade -= FADE;
            }
        }
        if self.flags & flag::LEAVES != 0 {
            (pass.leaves, self.over) = (true, true);
        } else if pad.pressed & pad::CIRCLE != 0 && self.all_through() {
            (pass.menu, self.over) = (true, true);
        }
        Some(pass)
    }

    /// A level of a working copy: the growth of the results screen, which answers the
    /// levels that waited in the copy's experience word.
    fn grow(&mut self, tables: &Tables, n: usize) -> Option<()> {
        self.levels[n] = self.copies[n].grow(tables, Growth::Results)?;
        Some(())
    }

    /// The counting (`0x800BC73C`): a confirm or a cancel gives every fighter the rest
    /// of its sum at once; otherwise each fighter that is not through gets a 32nd of
    /// its sum, and the pass that finds nothing left ends the counting.
    fn count(&mut self, tables: &Tables, pad: &Pad, pass: &mut Pass) -> Option<()> {
        let fighters = self.copies.len();
        if menu::confirmed(pad) || menu::cancelled(pad) {
            for n in 0..fighters {
                let rest = self.sums[n].wrapping_sub(self.counted[n]);
                let word = self.copies[n].experience.wrapping_add(rest);
                self.copies[n].experience = word;
                if word > WAY {
                    self.grow(tables, n)?;
                }
                self.gained[n] |= self.levels[n] != 0;
            }
            self.flags |= flag::COUNTED;
            pass.sounds.push(SOUND_CONFIRM);
            pass.jingles += u32::from(self.gained[..fighters].contains(&true));
            return Some(());
        }
        if (0..fighters).all(|n| self.counted[n] >= self.sums[n]) {
            self.flags |= flag::COUNTED;
            return Some(());
        }
        for n in 0..fighters {
            let sum = self.sums[n];
            if self.counted[n] >= sum {
                continue;
            }
            let step = match sum >> 5 {
                0 => sum & 0x1F,
                step => step,
            };
            self.shown[n] = self.shown[n].wrapping_add(step);
            self.counted[n] = self.counted[n].wrapping_add(step);
            // A word that is past a level already takes the level without the step.
            let mut word = self.copies[n].experience;
            if word <= WAY {
                word = word.wrapping_add(step);
                self.copies[n].experience = word;
            }
            if word > WAY {
                self.copies[n].experience = LEVEL;
                self.grow(tables, n)?;
                self.copies[n].experience = word - LEVEL;
                self.shown[n] = word - LEVEL;
                self.gained[n] = true;
                pass.jingles += 1;
            }
        }
        pass.sounds.push(SOUND_CURSOR);
        Some(())
    }

    /// A confirm or a cancel of the menus with its sound (`0x8003C118(1)`,
    /// `0x8003C1B4(1)`).
    fn pressed(pad: &Pad, pass: &mut Pass) -> bool {
        let sound = match (menu::confirmed(pad), menu::cancelled(pad)) {
            (true, _) => SOUND_CONFIRM,
            (false, true) => SOUND_CANCEL,
            (false, false) => return false,
        };
        pass.sounds.push(sound);
        true
    }

    /// The stepping through the fighters (`0x800BCBB8`), one step a pass at the first
    /// that is not gone through: one without a level is marked at once; one with a
    /// level waits for a press, which puts its panel up (the player's only when his
    /// rank rose) and the next press takes it down and marks the fighter. With all
    /// marked a press leaves.
    fn step(&mut self, pad: &Pad, pass: &mut Pass) {
        if self.all_through() {
            if Self::pressed(pad, pass) {
                self.flags |= flag::LEAVES;
            }
            return;
        }
        let Some(n) = (0..self.copies.len()).find(|n| !self.is_through(*n)) else {
            return;
        };
        let through = 1 << (n + 1);
        if self.levels[n] == 0 {
            self.flags |= through;
        } else if !Self::pressed(pad, pass) {
        } else if self.flags & flag::PANEL != 0 {
            self.flags = (self.flags | through) & !flag::PANEL;
        } else if n != 0 || rank(self.before[0].level) < rank(self.copies[0].level) {
            self.flags |= flag::PANEL;
        } else {
            self.flags |= flag::PLAYER;
        }
    }
}

/// The state of the task: its first pass, the wait for the notice of the spoils
/// (`0x800BD380`), and the screen (`0x800BC118`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    First,
    Notice,
    Screen,
}

/// What a pass of the task did and drew, for the app. The wall of the menus is drawn
/// in every pass.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Passed {
    /// The sounds, the music, the rank's name, the leaving and the circle.
    pub pass: Pass,
    /// The notice of the spoils was drawn in this pass.
    pub notice: bool,
    /// The screen's own picture was drawn: the boxes or a panel of a level up.
    pub screen: bool,
    /// The notice ended in this pass and handed its things over.
    pub given: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub results: Results,
    state: State,
    notice: notice::Shown,
}

impl Task {
    pub fn new(results: Results) -> Self {
        Self {
            results,
            state: State::First,
            notice: notice::Shown::default(),
        }
    }

    /// The things of the notice of the spoils (`0x800BD1AC`): one of each thing won,
    /// the first three records' in their order with nothing between them.
    pub fn noticed(&self) -> [notice::Thing; notice::THINGS] {
        let mut things = [notice::Thing::NONE; notice::THINGS];
        for (thing, won) in things.iter_mut().zip(self.results.won.iter().flatten()) {
            *thing = notice::Thing {
                kind: won.kind.into(),
                number: won.number.into(),
                count: 1,
            };
        }
        things
    }

    /// One pass of the task and, behind it, of the notice's. `real` are the fighters'
    /// own records as [`Results::pass`] takes them; the notice gives its things to
    /// `keeping` in the pass it ends in. `None` when the tables have no steps of
    /// growth.
    pub fn pass(
        &mut self,
        tables: &Tables,
        pad: &Pad,
        real: &mut [Record],
        keeping: &mut Keeping,
        worn: Worn,
    ) -> Option<Passed> {
        let mut passed = Passed::default();
        match self.state {
            // With a thing won the notice is put up, else the boxes are drawn; the
            // screen's own first pass is the next one.
            State::First => match self.results.won.iter().any(Option::is_some) {
                true => self.state = State::Notice,
                false => (self.state, passed.screen) = (State::Screen, true),
            },
            // The pass that finds the window's task gone names the screen and still
            // asks the pad, with the sound of the press.
            State::Notice => {
                if self.notice.is_gone() {
                    self.state = State::Screen;
                }
                if Results::pressed(pad, &mut passed.pass) {
                    self.notice.close();
                }
            }
            State::Screen => {
                passed.pass = self.results.pass(tables, pad, real)?;
                passed.screen = !self.results.is_over() || passed.pass.leaves;
            }
        }
        // The window's task: it draws the notice, and the step that finds it closed
        // hands the things over as a notice of way -1 does and ends the task.
        if self.state == State::Notice && !self.notice.is_gone() {
            passed.notice = true;
            if self.notice.step(false) {
                passed.given = true;
                for thing in self.noticed() {
                    keeping.notice_ends(SPOILS_WAY, thing.kind, thing.number, thing.count, worn);
                }
            }
        }
        Some(passed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::battle::record::tests::tables;

    const QUARTER: u32 = LEVEL / 4;

    fn fighter(level: u8, way: u32) -> Record {
        let mut record = Record::make(&tables(), 12, 0, &mut || 0).expect("a creature");
        (record.level, record.experience) = (level, way);
        record
    }

    fn player(level: u8) -> Record {
        Record {
            fixed: true,
            ..fighter(level, 0)
        }
    }

    fn beaten(species: i32, by: usize, level: u8, kind: u32) -> Beaten {
        Beaten {
            species,
            by,
            level,
            kind,
        }
    }

    const NOBODY: Beaten = Beaten {
        species: -1,
        by: 0,
        level: 0,
        kind: UNTOUCHED,
    };

    fn no_spoils() -> Spoils {
        Spoils { rows: vec![] }
    }

    fn press(button: u16) -> Pad {
        let mut pad = Pad::default();
        pad.read(button, crate::pad::Repeat::of(2), 0);
        pad
    }

    #[test]
    fn a_gain_is_the_document_s() {
        // Level 3 against level 5: two levels and a quarter.
        assert_eq!(gain(3, 0, 5), 2 * LEVEL + QUARTER);
        // Level 10 and half way against level 8: a quarter of a level.
        assert_eq!(gain(10, LEVEL / 2, 8), QUARTER);
        // The own level is one level only from the start of a level: what goes past
        // the next level counts at that level's price, half as much.
        assert_eq!(gain(7, 0, 7), LEVEL);
        assert_eq!(gain(7, 12345, 7), LEVEL - 6173);
        // Two of level 5 for a fighter of level 5: a level, then half of one.
        let minion = fighter(5, 0);
        let first = add(&minion, 5, 0);
        assert_eq!(first, LEVEL);
        assert_eq!(add(&minion, 5, first), LEVEL + LEVEL / 2);
        // The way of the fighter and of the sum are added before the second is read.
        let minion = fighter(5, 3 * QUARTER);
        // Three quarters on the way, an opponent one level below: a quarter to the next
        // level and an eighth of the one behind it. The second opponent is then counted
        // for a fighter of level 6 an eighth on its way.
        let first = add(&minion, 4, 0);
        assert_eq!(first, 3 * LEVEL / 8);
        assert_eq!(gain(6, LEVEL / 8, 7), LEVEL + 7 * LEVEL / 16);
        assert_eq!(add(&minion, 7, first), first + LEVEL + 7 * LEVEL / 16);
    }

    #[test]
    fn only_the_one_that_finished_an_opponent_gets_its_experience() {
        let mut fighters = vec![player(5), fighter(5, 0), fighter(5, 0)];
        // The player's own kill gives nothing, his capture does, a minion's kill does.
        let records = [
            beaten(12, 0, 5, 0),
            beaten(-1, 0, 5, 26),
            beaten(13, 2, 5, 0),
        ];
        assert_eq!(sums(&records, &fighters), [LEVEL, 0, LEVEL, 0]);
        // A dead minion gets nothing, and an opponent nobody touched gives nothing.
        fighters[2].status |= crate::battle::record::status::DEAD;
        let records = [beaten(12, 2, 5, 0), beaten(13, 1, 5, 7), NOBODY];
        assert_eq!(sums(&records, &fighters), [0; FIGHTERS]);
        // A slot without a fighter is passed over.
        assert_eq!(
            sums(&[beaten(12, 3, 5, 0), NOBODY, NOBODY], &fighters),
            [0; 4]
        );
    }

    #[test]
    fn the_spoils_take_two_draws_an_opponent() {
        let thing = |number: u8| Thing { kind: 0, number };
        let spoils = Spoils {
            rows: vec![
                Spoil {
                    first: (6553, thing(9)),
                    second: (327, thing(29)),
                };
                3
            ],
        };
        let records = [beaten(1, 0, 3, 0), beaten(2, 1, 3, 5), beaten(0, 2, 3, 0)];
        let mut given = [6552u32, 0, 6553, 326, 6553, 327].into_iter();
        let won = spoils.won(&records, &mut || given.next().expect("a draw"));
        assert_eq!(won, [Some(thing(9)), Some(thing(29)), None]);
        assert_eq!(given.next(), None);
        // No species, or nothing done to it: no draw. A species past the table: two.
        let records = [beaten(-1, 0, 3, 26), beaten(1, 0, 3, 7), beaten(3, 0, 3, 0)];
        let mut given = [0u32, 0].into_iter();
        let won = spoils.won(&records, &mut || given.next().expect("a draw"));
        assert_eq!((won, given.next()), ([None; 3], None));
    }

    #[test]
    fn the_table_of_the_spoils_is_read_from_its_place() {
        let start = (SPOILS - MENU_FILE_BASE) as usize;
        let mut file = vec![0; start + 8 * SPOIL_ROWS];
        file[start + 8 * 12..start + 8 * 13].copy_from_slice(&[0x99, 0x19, 0, 9, 0x47, 1, 3, 29]);
        let spoils = Spoils::from_file(&file).expect("the table");
        let row = spoils.rows[12];
        assert_eq!(row.first, (6553, Thing { kind: 0, number: 9 }));
        assert_eq!(
            row.second,
            (
                327,
                Thing {
                    kind: 3,
                    number: 29
                }
            )
        );
        assert_eq!(spoils.rows.len(), SPOIL_ROWS);
        assert!(Spoils::from_file(&file[..file.len() - 1]).is_none());
    }

    #[test]
    fn nothing_to_count_goes_straight_to_the_leaving() {
        let tables = tables();
        let mut real = vec![player(1)];
        let records = [beaten(12, 0, 1, 0), NOBODY, NOBODY];
        let mut results = Results::open(&records, &real, &no_spoils(), &mut || 40000);
        assert_eq!((results.sums, results.won), ([0; 4], [None; 3]));
        let idle = Pad::default();
        // The counting ends, the player is marked, and then the screen waits.
        let first = results.pass(&tables, &idle, &mut real).expect("a pass");
        assert_eq!((first, results.flags), (Pass::default(), flag::COUNTED));
        results.pass(&tables, &idle, &mut real).expect("a pass");
        assert!(results.all_through());
        results.pass(&tables, &idle, &mut real).expect("a pass");
        assert!(!results.is_over());
        let last = (results.pass(&tables, &press(pad::TRIANGLE), &mut real)).expect("a pass");
        assert_eq!((last.sounds, last.leaves), (vec![SOUND_CANCEL], true));
        assert!(results.is_over());
        let after = results.pass(&tables, &press(pad::CROSS), &mut real);
        assert_eq!(after, Some(Pass::default()));
    }

    #[test]
    fn the_circle_opens_the_menu_once_all_are_gone_through() {
        let tables = tables();
        let mut real = vec![player(1)];
        let mut results = Results::open(&[NOBODY; 3], &real, &no_spoils(), &mut || 0);
        let circle = press(pad::CIRCLE);
        // Not in the pass that ends the counting; in the pass that marks the last
        // fighter the circle counts already.
        let pass = results.pass(&tables, &circle, &mut real).expect("a pass");
        assert!(!pass.menu && !results.is_over());
        let pass = results.pass(&tables, &circle, &mut real).expect("a pass");
        assert!(pass.menu && !pass.leaves && results.is_over());
    }

    #[test]
    fn the_spoils_are_handed_over_by_a_notice_in_front_of_the_screen() {
        use crate::items::{FREE, Kind};
        let tables = tables();
        let mut real = vec![player(1)];
        let herb = Thing { kind: 0, number: 9 };
        let row = Spoil {
            first: (32767, herb),
            second: (0, herb),
        };
        let spoils = Spoils {
            rows: vec![row; 13],
        };
        // The second of two opponents leaves a thing: it is the notice's first.
        let records = [NOBODY, beaten(12, 0, 1, 0), NOBODY];
        let results = Results::open(&records, &real, &spoils, &mut || 0);
        assert_eq!(results.won, [None, Some(herb), None]);
        let mut task = Task::new(results);
        let named = notice::Thing {
            kind: 0,
            number: 9,
            count: 1,
        };
        assert_eq!(
            task.noticed(),
            [named, notice::Thing::NONE, notice::Thing::NONE]
        );
        let (mut keeping, worn) = (Keeping::new(), [FREE; 3]);
        let (idle, cross) = (Pad::default(), press(pad::CROSS));
        let mut pass = |task: &mut Task, pad: &Pad, keeping: &mut Keeping| {
            (task.pass(&tables, pad, &mut real, keeping, worn)).expect("a pass")
        };
        // The first pass puts the notice up, and it stands until a press.
        for _ in 0..3 {
            let passed = pass(&mut task, &idle, &mut keeping);
            assert!(passed.notice && !passed.screen && !passed.given);
            assert_eq!(passed.pass, Pass::default());
        }
        assert_eq!(keeping.count(9, Kind::Item), 0);
        // The press closes it, and the window's step of the same pass hands over.
        let passed = pass(&mut task, &cross, &mut keeping);
        assert!(passed.notice && passed.given);
        assert_eq!(passed.pass.sounds, [SOUND_CONFIRM]);
        assert_eq!(keeping.count(9, Kind::Item), 1);
        // The next pass finds the window gone and draws the wall alone; the counting
        // has not started.
        let passed = pass(&mut task, &idle, &mut keeping);
        assert_eq!(passed, Passed::default());
        assert_eq!(task.results.flags, 0);
        let passed = pass(&mut task, &idle, &mut keeping);
        assert!(passed.screen && !passed.notice);
        assert_eq!(task.results.flags, flag::COUNTED);
        assert_eq!(keeping.count(9, Kind::Item), 1);
    }

    #[test]
    fn without_spoils_the_first_pass_draws_the_boxes() {
        let tables = tables();
        let mut real = vec![player(1)];
        let results = Results::open(&[NOBODY; 3], &real, &no_spoils(), &mut || 0);
        let mut task = Task::new(results);
        let mut keeping = Keeping::new();
        let cross = press(pad::CROSS);
        // The first pass asks no pad: the press is not the counting's.
        let passed = (task.pass(&tables, &cross, &mut real, &mut keeping, [0; 3])).expect("a pass");
        assert!(passed.screen && !passed.notice);
        assert_eq!((passed.pass, task.results.flags), (Pass::default(), 0));
        let passed =
            (task.pass(&tables, &Pad::default(), &mut real, &mut keeping, [0; 3])).expect("a pass");
        assert!(passed.screen);
        assert_eq!(task.results.flags, flag::COUNTED);
    }

    #[test]
    fn a_sum_is_counted_by_32nds_and_a_level_is_confirmed_away() {
        let tables = tables();
        // A minion of level 5, three quarters on its way, finishes one of its level:
        // a quarter to level 6 and three eighths of the way behind it.
        let mut real = vec![player(5), fighter(5, 3 * QUARTER)];
        let before = real[1].clone();
        let records = [beaten(12, 1, 5, 0), NOBODY, NOBODY];
        let mut results = Results::open(&records, &real, &no_spoils(), &mut || 40000);
        assert_eq!(results.sums, [0, 5 * LEVEL / 8, 0, 0]);
        let idle = Pad::default();
        // Thirteen steps of a 32nd bring it past the level, in the pass with the
        // jingle.
        for n in 1..=13 {
            let pass = results.pass(&tables, &idle, &mut real).expect("a pass");
            assert_eq!(pass.sounds, [SOUND_CURSOR]);
            assert_eq!(
                (pass.jingles, results.gained[1]),
                (u32::from(n == 13), n == 13)
            );
        }
        assert_eq!(
            (results.copies[1].level, results.copies[1].experience),
            (6, LEVEL / 256)
        );
        assert_eq!((results.shown[1], results.levels[1]), (LEVEL / 256, 1));
        // The fighter's own record is not written while the counting runs.
        assert_eq!(real[1], before);
        // 19 more steps make the sum whole, and the pass behind them ends the counting.
        for _ in 0..19 {
            results.pass(&tables, &idle, &mut real).expect("a pass");
        }
        assert_eq!((results.counted[1], results.flags), (5 * LEVEL / 8, 0));
        let pass = results.pass(&tables, &idle, &mut real).expect("a pass");
        assert_eq!((pass, results.flags), (Pass::default(), flag::COUNTED));
        assert_eq!(results.copies[1].experience, 3 * LEVEL / 8);
        // The player has no level and is marked; the minion waits for a press.
        results.pass(&tables, &idle, &mut real).expect("a pass");
        assert_eq!(real[1], results.copies[1]);
        assert!(results.is_through(0) && !results.is_through(1));
        results.pass(&tables, &idle, &mut real).expect("a pass");
        assert_eq!(results.panel(), None);
        // A press puts its panel up, the next takes it down and marks it.
        let cross = press(pad::CROSS);
        let pass = results.pass(&tables, &cross, &mut real).expect("a pass");
        assert_eq!(
            (pass.sounds, results.panel()),
            (vec![SOUND_CONFIRM], Some(1))
        );
        assert_eq!(pass.name, None);
        results.pass(&tables, &idle, &mut real).expect("a pass");
        assert_eq!(results.panel(), Some(1));
        results.pass(&tables, &cross, &mut real).expect("a pass");
        assert!(results.panel().is_none() && results.all_through());
        let pass = results.pass(&tables, &cross, &mut real).expect("a pass");
        assert!(pass.leaves);
        // The level brought the bonus of the results screen: the growth in that mode.
        let mut grown = before.clone();
        grown.experience += 5 * LEVEL / 8;
        grown.grow(&tables, Growth::Results);
        assert_eq!(real[1], grown);
    }

    #[test]
    fn a_press_gives_the_whole_sum_at_once() {
        let tables = tables();
        let mut real = vec![player(5), fighter(3, 0), fighter(5, 0)];
        // Two levels and a quarter for the first minion, a 64th of one for the second.
        let records = [beaten(12, 1, 5, 0), beaten(12, 2, 0, 0), NOBODY];
        let mut results = Results::open(&records, &real, &no_spoils(), &mut || 40000);
        assert_eq!(results.sums, [0, 2 * LEVEL + QUARTER, LEVEL / 32, 0]);
        let idle = Pad::default();
        results.pass(&tables, &idle, &mut real).expect("a pass");
        let pass = (results.pass(&tables, &press(pad::CROSS), &mut real)).expect("a pass");
        assert_eq!((pass.sounds, pass.jingles), (vec![SOUND_CONFIRM], 1));
        assert_eq!(results.flags, flag::COUNTED);
        assert_eq!((results.copies[1].level, results.levels[1]), (5, 2));
        assert_eq!(results.copies[1].experience, QUARTER);
        assert_eq!((results.copies[2].level, results.gained[2]), (5, false));
        assert_eq!(results.copies[2].experience, LEVEL / 32);
    }

    #[test]
    fn the_player_s_panel_comes_only_with_a_new_rank() {
        let tables = tables();
        let idle = Pad::default();
        let cross = press(pad::CROSS);
        let run = |level: u8| {
            // A capture of an opponent of his level is a level for the player.
            let mut real = vec![player(level)];
            let records = [beaten(-1, 0, level, 26), NOBODY, NOBODY];
            let mut results = Results::open(&records, &real, &no_spoils(), &mut || 0);
            results.pass(&tables, &cross, &mut real).expect("a pass");
            results.pass(&tables, &cross, &mut real).expect("a pass");
            (results, real)
        };
        // Level 2 to 3 is the same rank: the press marks him.
        let (results, real) = run(2);
        assert_eq!((real[0].level, results.panel()), (3, None));
        assert!(results.all_through());
        // His level counts and changes nothing else.
        assert_eq!(
            (real[0].stats, real[0].max_hp),
            (player(2).stats, player(2).max_hp)
        );
        // Level 3 to 4 is a new rank: the panel, with the old name going dark over 32
        // passes and the new one coming up over 31.
        let (mut results, mut real) = run(3);
        assert_eq!((real[0].level, results.panel()), (4, Some(0)));
        let mut names = vec![];
        for _ in 0..70 {
            let pass = results.pass(&tables, &idle, &mut real).expect("a pass");
            names.push(pass.name.expect("the panel is up"));
        }
        // The pass of the press drew the old name plain; these go on from there.
        assert_eq!(names[0], (true, 4));
        assert_eq!(names[30], (true, 124));
        assert_eq!(names[31], (true, DARK));
        assert_eq!(names[32], (false, 124));
        assert_eq!(names[62], (false, 4));
        assert_eq!(names[69], (false, 4));
        results.pass(&tables, &cross, &mut real).expect("a pass");
        assert!(results.panel().is_none() && results.all_through());
    }
}
