
use super::fight::SLOTS;
use super::record::{Record, status};
use crate::menu::Rect;
use crate::player_panel::{self, creature, gain_bar, head, head_of_box};
use crate::window::{Way, opened};

/// The opponents a fight can have on the field, and the slot of the first.
pub const OPPONENTS: usize = 3;
pub const FIRST_SLOT: usize = 4;
/// The size of a box, and the passes it takes to open, from the middle in height.
pub const BOX: (i16, i16) = (100, 41);
pub const OPENING: i32 = 4;
/// Where the box of an opponent stands, by how many stand and by its number
/// (`0x800C9310`); a place the table does not have is (-1, -1).
pub const PLACES: [[(i16, i16); OPPONENTS]; OPPONENTS] = [
    [(110, 20), (-1, -1), (-1, -1)],
    [(60, 20), (160, 20), (-1, -1)],
    [(10, 20), (110, 20), (210, 20)],
];
/// The box of the own side.
pub const OWN: Rect = Rect {
    x: 110,
    y: 172,
    w: BOX.0,
    h: BOX.1,
};
/// The frame of a creature's line of mana beside it.
pub const MANA: Rect = Rect {
    x: 210,
    y: 192,
    w: 100,
    h: 21,
};
/// Where the row of the status signs starts, how high a sign's frame is, and the signs
/// (`0x8002F4DC`, the pieces at `0x800742F8` with the masks at `0x8007434C`): the bits
/// of the status, the sprite, the width of its frame and where the sprite stands in it.
pub const SIGNS_AT: (i16, i16) = (210, 172);
pub const SIGN_HIGH: i16 = 20;
pub const SIGNS: [(u32, usize, i16, (i16, i16)); 3] = [
    (0x001, 151, 21, (3, 4)),
    (0x3F8, 189, 34, (3, 5)),
    (0x002, 177, 34, (6, 5)),
];
/// The passes a slide of the opponents' boxes takes.
const SLIDE: u32 = 4;
/// The group of status bits of which a record holds one at most (`0x80012794`).
const ONE_OF: u32 = 0x3F8;

/// One thing of the HUD, as a painter draws it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drawn {
    /// A thing of a fighter's box, as the player's menu has it.
    Boxed(player_panel::Drawn),
    /// The name of the fighter of a slot in the small font, in capitals.
    Name {
        slot: usize,
        at: (i16, i16),
        clip: Rect,
    },
}

/// What the menu of the fight marks in a pass (`0x800C75F4`) and previews
/// (`0x800C8580`): the mode of the opponents' frames with the time of each, the same
/// for the own side, and the changes of the HP and of the mana to come.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Marks {
    pub opponents: u8,
    pub times: [u32; OPPONENTS],
    pub own: u8,
    pub own_time: u32,
    pub hp: i16,
    pub mana: i16,
}

impl Marks {
    /// The marks of a pass of the menu: the marked opponents by their numbers and the
    /// own side, the menus' time (`0x8002C6A8`) and the preview. One marked opponent
    /// gives every opponent's frame the mode, and only the marked ones a time.
    pub fn of((opponents, own): (u8, bool), time: u32, (hp, mana): (i16, i16)) -> Self {
        Self {
            opponents: u8::from(opponents != 0),
            times: std::array::from_fn(|n| match opponents >> n & 1 {
                0 => 0,
                _ => time,
            }),
            own: u8::from(own),
            own_time: if own { time } else { 0 },
            hp,
            mana,
        }
    }
}

/// What a pass is told of the fight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Told {
    /// The mask of the opponents on the field (1 << slot).
    pub standing: u8,
    /// 0 outside a fight of a master and his creature, 1 with the master in front, 2
    /// with his creature (`0x800A2A44`).
    pub master: u8,
    /// The battle flags `0x10000` and `0x8000` are set.
    pub opponents_away: bool,
    pub own_away: bool,
    /// A lesson in which the acting slot is 10: the box of the own side is left out.
    pub lesson_waits: bool,
}

/// The box of an opponent: its place and the count of its opening.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Standing {
    place: (i16, i16),
    count: i32,
}

/// The HUD from its making to the end of the fight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hud {
    boxes: [Option<Standing>; OPPONENTS],
    /// The mask of the pass before, and the slide that a change of it starts.
    seen: u8,
    slides: bool,
    slid: u32,
    own: i32,
    /// What the last pass drew: the count of each opponent's box, and of the own side's.
    drew: [Option<i32>; OPPONENTS],
    drew_own: Option<i32>,
    master: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Number {
    pub slot: usize,
    /// 0 the HP, 1 the MP; any other way changes nothing.
    pub way: u8,
    /// What is left of the change, a loss above 0, and the passes so far.
    left: i32,
    count: i32,
}

impl Number {
    /// The passes that change the record; the task ends in the one behind them.
    pub const PASSES: i32 = 16;
    /// The tasks of the pool.
    pub const AT_ONCE: usize = 8;

    pub fn new(slot: usize, way: u8, amount: i32) -> Self {
        Self {
            slot,
            way,
            left: amount,
            count: 0,
        }
    }

    /// One pass on the record of its slot; false in the pass the task ends in. The
    /// share is what is left over the passes to come, so the last one takes the rest;
    /// the record keeps the value between nothing and its highest (`0x80012614`), and
    /// a way 0 that finds no HP behind its share marks the record dead (`0x800127B8`).
    pub fn pass(&mut self, record: Option<&mut Record>) -> bool {
        self.count += 1;
        if self.count > Self::PASSES {
            return false;
        }
        let share = self.left / (Self::PASSES + 1 - self.count);
        self.left -= share;
        let Some(record) = record else {
            return true;
        };
        let held =
            |has: i16, highest: i16| (i32::from(has) - share).clamp(0, highest.into()) as i16;
        match self.way {
            0 => {
                record.hp = held(record.hp, record.max_hp);
                if record.hp <= 0 {
                    record.hp = 0;
                    record.status |= status::DEAD;
                    record.experience = 0;
                }
            }
            1 => record.mp = held(record.mp, record.max_mp),
            _ => {}
        }
        true
    }
}

/// The fighters of a fight as its picture has them: the rules are ahead of the picture
/// (they decide a command before its run is shown), so the HUD goes by records of its
/// own, which the events of the runner's clock change as the game's do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    pub records: [Option<Record>; SLOTS],
    /// The mask of the opponents on the field (1 << slot).
    pub standing: u8,
    numbers: Vec<Number>,
}

impl Seen {
    /// The records and the opponents on the field in front of the first command.
    pub fn new(records: [Option<Record>; SLOTS], standing: u8) -> Self {
        Self {
            records,
            standing,
            numbers: Vec::new(),
        }
    }

    /// A number task is made (`0x800BF5AC`); the pool has eight, and a ninth is not
    /// made.
    pub fn number(&mut self, slot: usize, way: u8, amount: i32) {
        if self.numbers.len() < Number::AT_ONCE {
            self.numbers.push(Number::new(slot, way, amount));
        }
    }

    /// The status of a slot loses bits and gets bits (`0x80012614`, kinds 17 and 16):
    /// a record that has one of the group `0x3F8` gets no other of it.
    pub fn give(&mut self, slot: usize, clear: u32, add: u32) {
        let Some(record) = self.records.get_mut(slot).and_then(Option::as_mut) else {
            return;
        };
        record.status &= !clear;
        let add = match record.status & ONE_OF != 0 {
            true => add & !ONE_OF,
            false => add,
        };
        record.status |= add;
    }

    /// The start of a turn writes the slot's status word (`0x800BF994`).
    pub fn tick(&mut self, slot: usize, word: u32) {
        if let Some(record) = self.records.get_mut(slot).and_then(Option::as_mut) {
            record.status = word;
        }
    }

    /// An opponent has left the field.
    pub fn leave(&mut self, slot: usize) {
        self.standing &= !(1 << slot);
    }

    /// One pass of every number task, in the order they were made.
    pub fn pass(&mut self) {
        let records = &mut self.records;
        (self.numbers).retain_mut(|number| {
            let record = records.get_mut(number.slot).and_then(Option::as_mut);
            number.pass(record)
        });
    }

    /// Whether a number task counts (`0x800BF64C`).
    pub fn counts(&self) -> bool {
        !self.numbers.is_empty()
    }
}

/// The mask the HUD goes by: slot 4 alone in a fight of a master and his creature.
fn mask(standing: u8, master: u8) -> u8 {
    match master {
        0 => standing,
        _ => 1 << FIRST_SLOT,
    }
}

impl Hud {
    /// The maker (`0x800BFE10`, `0x800C03A8`): a box for each opponent on the field, in
    /// the place of its number among as many as stand.
    pub fn new(standing: u8, master: u8) -> Self {
        let row = (standing.count_ones() as usize).clamp(1, OPPONENTS) - 1;
        let made = mask(standing, master);
        Self {
            boxes: std::array::from_fn(|n| {
                (made >> (FIRST_SLOT + n) & 1 != 0).then_some(Standing {
                    place: PLACES[row][n],
                    count: 0,
                })
            }),
            seen: 0,
            slides: false,
            slid: 0,
            own: 0,
            drew: [None; OPPONENTS],
            drew_own: None,
            master,
        }
    }

    /// One pass of the four tasks. The answer is whether the HUD's own task asks for
    /// battle flag `0x10000`: an opponent left with a master's creature in front.
    pub fn pass(&mut self, told: Told) -> bool {
        let standing = mask(told.standing, told.master);
        let mut asks = false;
        self.master = told.master;
        // The HUD's own task (`0x800BFFBC`): the boxes of those who left end, and the
        // others slide to the places of the new count.
        if standing != self.seen {
            let gone = self.seen & !standing;
            (self.seen, self.slides, self.slid) = (standing, true, 0);
            for (n, standing) in self.boxes.iter_mut().enumerate() {
                if gone >> (FIRST_SLOT + n) & 1 != 0 {
                    match told.master {
                        2 => asks = true,
                        _ => *standing = None,
                    }
                }
            }
        }
        if self.slides {
            match self.slid < SLIDE {
                true => {
                    self.slid += 1;
                    let row = (standing.count_ones() as usize).clamp(1, OPPONENTS) - 1;
                    let step = |from: i16, to: i16| {
                        let way = (i32::from(to) - i32::from(from)) * self.slid as i32;
                        from.wrapping_add((way >> 2) as i16)
                    };
                    for (n, standing) in self.boxes.iter_mut().enumerate() {
                        if let Some(standing) = standing {
                            let to = PLACES[row][n];
                            let (x, y) = standing.place;
                            standing.place = (step(x, to.0), step(y, to.1));
                        }
                    }
                }
                false => self.slides = false,
            }
        }
        // The boxes of the opponents (`0x800C0244`) and of the own side (`0x800C0424`):
        // a box that is left out does not open either.
        for (drew, standing) in self.drew.iter_mut().zip(&mut self.boxes) {
            *drew = match standing {
                Some(standing) if !told.opponents_away => {
                    standing.count += 1;
                    Some(standing.count - 1)
                }
                _ => None,
            };
        }
        self.drew_own = match told.own_away || told.lesson_waits {
            true => None,
            false => {
                let count = self.own;
                self.own = (self.own + 1).min(OPENING);
                Some(count)
            }
        };
        asks
    }

    /// Where the box of an opponent stands now.
    pub fn place(&self, opponent: usize) -> Option<(i16, i16)> {
        (self.boxes.get(opponent)?.as_ref()).map(|standing| standing.place)
    }

    /// A box of the executable (`0x8002F224`): the frame, the name and the head.
    #[allow(clippy::too_many_arguments)]
    fn boxed(
        whole: Rect,
        count: i32,
        (slot, record): (usize, &Record),
        (mode, time): (u8, u32),
        change: i32,
        flags: u32,
        passes: u32,
    ) -> Vec<Drawn> {
        // The content starts at the corner of the opened box and is cut to it.
        let clip = opened(whole, count, OPENING, Way::MiddleHigh);
        let corner = (clip.x, clip.y);
        let mut drawn = vec![
            Drawn::Boxed(player_panel::Drawn::Frame {
                rect: clip,
                mode,
                time,
            }),
            Drawn::Name {
                slot,
                at: (corner.0.wrapping_add(7), corner.1.wrapping_add(3)),
                clip,
            },
        ];
        let head = head_of_box(record, corner, change, passes, flags, clip);
        drawn.extend(head.into_iter().map(Drawn::Boxed));
        drawn
    }

    /// What the last pass drew, from what lies underneath to what lies on top.
    /// `records` are the fighters by slot, `boss` says whether an opponent's species is
    /// one of the boss test (`0x800BFB50`), `subject` is the slot the box of the own
    /// side shows (`0x800C9E69`) and `passes` the counter a change to come blinks by.
    pub fn drawn(
        &self,
        records: &[Option<Record>],
        boss: impl Fn(usize) -> bool,
        subject: usize,
        marks: &Marks,
        passes: u32,
    ) -> Vec<Drawn> {
        let record = |slot: usize| records.get(slot).and_then(Option::as_ref);
        let mut drawn = Vec::new();
        for (n, (standing, count)) in self.boxes.iter().zip(self.drew).enumerate() {
            let (Some(standing), Some(count)) = (standing, count) else {
                continue;
            };
            // With a master the one box shows whoever is in front.
            let shown = match self.master {
                1 => 0,
                2 => 1,
                _ => n,
            };
            let slot = FIRST_SLOT + shown;
            let Some(record) = record(slot) else {
                continue;
            };
            let whole = Rect {
                x: standing.place.0,
                y: standing.place.1,
                w: BOX.0,
                h: BOX.1,
            };
            let flags = if boss(shown) { head::HIDDEN } else { 0 };
            let mark = (
                marks.opponents,
                marks.times.get(shown).copied().unwrap_or(0),
            );
            drawn.extend(Self::boxed(
                whole,
                count,
                (slot, record),
                mark,
                0,
                flags,
                passes,
            ));
        }
        let (Some(count), Some(record)) = (self.drew_own, record(subject)) else {
            return drawn;
        };
        let flags = if subject == 0 { head::CAPTURE } else { 0 };
        let mark = (marks.own, marks.own_time);
        drawn.extend(Self::boxed(
            OWN,
            count,
            (subject, record),
            mark,
            marks.hp.into(),
            flags,
            passes,
        ));
        let whole = Rect {
            x: 0,
            y: 0,
            w: 320,
            h: 240,
        };
        let plain = |rect: Rect| player_panel::Drawn::Frame {
            rect,
            mode: 0,
            time: 0,
        };
        let mut own = Vec::new();
        if subject != 0 {
            // A creature's line of mana, with the mana the command would cost or give.
            let from = |right: i16, down: i16| (MANA.x + right, MANA.y + down);
            let (mp, most) = (record.mp, record.max_mp);
            own.push(plain(MANA));
            own.push(player_panel::Drawn::Sprite {
                number: creature::MANA,
                at: from(7, 4),
                clip: whole,
            });
            own.extend(player_panel::line((mp, most), from(32, 3), whole));
            own.extend(gain_bar(
                1,
                (mp.into(), most.into(), marks.mana.into()),
                from(32, 3),
                passes,
                whole,
            ));
        }
        // The signs of the status, in a row to the right.
        let mut x = SIGNS_AT.0;
        for (bits, number, wide, margin) in SIGNS {
            if record.status & bits == 0 {
                continue;
            }
            own.push(plain(Rect {
                x,
                y: SIGNS_AT.1,
                w: wide,
                h: SIGN_HIGH,
            }));
            own.push(player_panel::Drawn::Sprite {
                number,
                at: (x + margin.0, SIGNS_AT.1 + margin.1),
                clip: whole,
            });
            x += wide;
        }
        drawn.extend(own.into_iter().map(Drawn::Boxed));
        drawn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(hp: i16) -> Record {
        let mut bytes = [0u8; Record::HEAD];
        bytes[8..10].copy_from_slice(&hp.to_le_bytes());
        bytes[10..12].copy_from_slice(&40i16.to_le_bytes());
        Record::head(&bytes).unwrap()
    }

    fn told(standing: u8) -> Told {
        Told {
            standing,
            ..Told::default()
        }
    }

    #[test]
    fn the_boxes_of_the_opponents_stand_by_their_count_and_slide_when_one_leaves() {
        let mut hud = Hud::new(0x70, 0);
        let places = |hud: &Hud| [hud.place(0), hud.place(1), hud.place(2)];
        assert_eq!(
            places(&hud),
            [Some((10, 20)), Some((110, 20)), Some((210, 20))]
        );
        for _ in 0..6 {
            assert!(!hud.pass(told(0x70)));
        }
        assert_eq!(hud.place(2), Some((210, 20)));
        // The last opponent leaves: its box ends, and the two others go to the places
        // of two in four passes, each pass a share of what is left of the way.
        hud.pass(told(0x30));
        assert_eq!(places(&hud), [Some((22, 20)), Some((122, 20)), None]);
        hud.pass(told(0x30));
        assert_eq!(hud.place(0), Some((41, 20)));
        hud.pass(told(0x30));
        hud.pass(told(0x30));
        assert_eq!(places(&hud), [Some((60, 20)), Some((160, 20)), None]);
        hud.pass(told(0x30));
        assert_eq!(hud.place(0), Some((60, 20)));
        // As read: the table has no place for opponent 2 among two.
        let mut odd = Hud::new(0x70, 0);
        odd.pass(told(0x70));
        for _ in 0..5 {
            odd.pass(told(0x50));
        }
        assert_eq!(places(&odd), [Some((60, 20)), None, Some((-1, -1))]);
    }

    #[test]
    fn a_box_opens_in_four_passes_and_the_battle_flags_leave_the_boxes_out() {
        let mut hud = Hud::new(0x10, 0);
        let records = [Some(record(30)), None, None, None, Some(record(12))];
        let marks = Marks::default();
        let picture = |hud: &Hud| hud.drawn(&records, |_| false, 0, &marks, 0);
        let frames = |drawn: &[Drawn]| {
            let frames = drawn.iter().filter_map(|drawn| match drawn {
                Drawn::Boxed(player_panel::Drawn::Frame { rect, .. }) => Some(*rect),
                _ => None,
            });
            frames.collect::<Vec<_>>()
        };
        // The first pass: no height yet, about the middle of the boxes.
        hud.pass(told(0x10));
        let first = frames(&picture(&hud));
        assert_eq!(first.len(), 2);
        assert_eq!((first[0].x, first[0].w, first[0].h), (110, 100, 0));
        for _ in 0..4 {
            hud.pass(told(0x10));
        }
        let whole = picture(&hud);
        let rect = |y: i16| Rect {
            x: 110,
            y,
            w: 100,
            h: 41,
        };
        assert_eq!(frames(&whole), [rect(20), rect(172)]);
        // The names of the opponent's slot and of the player, 7 right and 3 below.
        let names: Vec<_> = (whole.iter())
            .filter_map(|drawn| match drawn {
                Drawn::Name { slot, at, .. } => Some((*slot, *at)),
                _ => None,
            })
            .collect();
        assert_eq!(names, [(4, (117, 23)), (0, (117, 175))]);
        // The menu marks opponent 0 alone: its frame pulses by the menus' time; with the
        // own side marked the opponent's is plain and the own pulses.
        let modes = |marks: Marks| {
            let drawn = hud.drawn(&records, |_| false, 0, &marks, 0);
            let frames = drawn.into_iter().filter_map(|drawn| match drawn {
                Drawn::Boxed(player_panel::Drawn::Frame { mode, time, .. }) => Some((mode, time)),
                _ => None,
            });
            frames.collect::<Vec<_>>()
        };
        assert_eq!(modes(Marks::of((1, false), 9, (0, 0))), [(1, 9), (0, 0)]);
        assert_eq!(modes(Marks::of((0, true), 9, (0, 0))), [(0, 0), (1, 9)]);
        // Of three, one marked: all have the mode and the marked one the time.
        assert_eq!(Marks::of((0b100, false), 9, (0, 0)).times, [0, 0, 9]);
        // The preview blinks on the gauge of the own side's box, in the passes of the
        // bit: a bar more than without it.
        let bars = |change: i16, passes: u32| {
            let marks = Marks::of((0, true), 9, (change, 0));
            let drawn = hud.drawn(&records, |_| false, 0, &marks, passes);
            let bars = drawn
                .iter()
                .filter(|drawn| matches!(drawn, Drawn::Boxed(player_panel::Drawn::Bar { .. })));
            bars.count()
        };
        assert_eq!(bars(5, 0x10), bars(0, 0x10) + 1);
        assert_eq!(bars(5, 0), bars(0, 0));
        // A list of the menu is up: the opponents' box is left out; a text: the own.
        hud.pass(Told {
            opponents_away: true,
            ..told(0x10)
        });
        assert_eq!(frames(&picture(&hud)), [rect(172)]);
        hud.pass(Told {
            own_away: true,
            ..told(0x10)
        });
        assert_eq!(frames(&picture(&hud)), [rect(20)]);
    }

    #[test]
    fn a_boss_hides_its_numbers_and_the_status_has_its_signs() {
        let mut hud = Hud::new(0x10, 0);
        let mut player = record(30);
        player.status = 0x009;
        let records = [Some(player), None, None, None, Some(record(12))];
        for _ in 0..5 {
            hud.pass(told(0x10));
        }
        let sprites = |drawn: &[Drawn], wanted: usize| {
            let places = drawn.iter().filter_map(|drawn| match drawn {
                Drawn::Boxed(player_panel::Drawn::Sprite { number, at, .. }) => {
                    (*number == wanted).then_some(*at)
                }
                _ => None,
            });
            places.collect::<Vec<_>>()
        };
        let numbers = |drawn: &[Drawn]| {
            let numbers = drawn
                .iter()
                .filter(|drawn| matches!(drawn, Drawn::Boxed(player_panel::Drawn::Number { .. })));
            numbers.count()
        };
        let marks = Marks::default();
        let plain = hud.drawn(&records, |_| false, 0, &marks, 0);
        assert!(sprites(&plain, head::MARK).is_empty());
        assert_eq!(numbers(&plain), 6);
        // The boss: three marks on either side of the stroke and two for the level.
        let boss = hud.drawn(&records, |opponent| opponent == 0, 0, &marks, 0);
        assert_eq!(numbers(&boss), 3);
        assert_eq!(
            sprites(&boss, head::MARK),
            [
                (121, 38),
                (128, 38),
                (135, 38),
                (148, 38),
                (155, 38),
                (162, 38),
                (152, 49),
                (159, 49)
            ]
        );
        // Poison and one of the bits of 0x3F8: two signs in a row from (210, 172).
        assert_eq!(sprites(&plain, 151), [(213, 176)]);
        assert_eq!(sprites(&plain, 189), [(234, 177)]);
        assert!(sprites(&plain, 177).is_empty());
    }

    #[test]
    fn a_number_counts_its_change_into_the_record_in_sixteen_passes() {
        let mut records: [Option<Record>; SLOTS] = Default::default();
        records[4] = Some(record(30));
        let mut seen = Seen::new(records, 0x10);
        let hp = |seen: &Seen| seen.records[4].as_ref().map(|record| record.hp);
        // A loss of 21: a sixteenth of what is left, rounded down, then a fifteenth.
        seen.number(4, 0, 21);
        seen.pass();
        assert_eq!(hp(&seen), Some(29));
        seen.pass();
        assert_eq!(hp(&seen), Some(28));
        for _ in 0..13 {
            seen.pass();
        }
        assert_eq!(hp(&seen), Some(11));
        // The sixteenth pass takes the rest, and the task ends in the one behind it.
        seen.pass();
        assert_eq!((hp(&seen), seen.counts()), (Some(9), true));
        seen.pass();
        assert!(!seen.counts());
        // A gain stops at the highest, and a loss of all there is marks it dead.
        seen.number(4, 0, -100);
        for _ in 0..17 {
            seen.pass();
        }
        assert_eq!(hp(&seen), Some(40));
        seen.number(4, 0, 40);
        seen.number(4, 2, 0);
        for _ in 0..16 {
            seen.pass();
        }
        let it = seen.records[4].as_ref().expect("it");
        assert_eq!((it.hp, it.status & status::DEAD), (0, status::DEAD));
        // The pool has eight tasks.
        for _ in 0..12 {
            seen.number(4, 1, 0);
        }
        assert_eq!(seen.numbers.len(), Number::AT_ONCE);
    }

    #[test]
    fn a_record_gets_one_bit_of_the_group_at_most() {
        let mut records: [Option<Record>; SLOTS] = Default::default();
        records[0] = Some(record(30));
        let mut seen = Seen::new(records, 0x30);
        let status = |seen: &Seen| seen.records[0].as_ref().map(|record| record.status);
        seen.give(0, 0, 0x009);
        assert_eq!(status(&seen), Some(0x009));
        seen.give(0, 0x001, 0x012);
        assert_eq!(status(&seen), Some(0x00A));
        // The start of a turn writes the whole word, the stone step moved up.
        seen.tick(0, 0x012);
        assert_eq!(status(&seen), Some(0x012));
        seen.leave(5);
        assert_eq!(seen.standing, 0x10);
    }

    #[test]
    fn with_a_master_the_one_box_shows_who_is_in_front() {
        // The master in front and his creature waiting: one box.
        let mut hud = Hud::new(0x10, 1);
        assert_eq!(hud.place(0), Some((110, 20)));
        let records = [
            Some(record(30)),
            None,
            None,
            None,
            Some(record(12)),
            Some(record(9)),
        ];
        let marks = Marks::default();
        let slot = |hud: &Hud| {
            let drawn = hud.drawn(&records, |_| false, 0, &marks, 0);
            drawn.into_iter().find_map(|drawn| match drawn {
                Drawn::Name { slot, .. } => Some(slot),
                _ => None,
            })
        };
        let master = |master: u8| Told {
            master,
            ..told(0x10)
        };
        hud.pass(master(1));
        assert_eq!(slot(&hud), Some(4));
        hud.pass(master(2));
        assert_eq!(slot(&hud), Some(5));
    }
}
