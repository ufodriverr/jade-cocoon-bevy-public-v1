
use crate::menu::Rect;
use crate::rooms::{self, Image};
use crate::text::{self, Font, Glyph};

/// Things a notice has room for.
pub const THINGS: usize = 3;
/// The kind of a thing that is not there.
pub const NO_THING: i32 = -1;
/// The kinds without a picture and a name of their own row: empty cocoons and money.
const COCOONS: i32 = 6;
const MONEY: i32 = 7;

/// The box: its left edge, its width, the height of a row and what is added to the rows.
const LEFT: i16 = 47;
const WIDE: i16 = 227;
const ROW: i16 = 16;
const EDGES: i16 = 8;
/// The top of a box of three rows, and how far it moves down for every row less.
const TOP: i16 = 90;
const TOP_STEP: i16 = 9;
/// The first row below the box's upper edge.
const FIRST_ROW: i16 = 7;
/// A row's content right of the box's left edge.
const INDENT: i16 = 6;
/// The "X" and the count right of the box's left edge.
const TIMES_AT: i16 = 0x84;
const COUNT_AT: i16 = 0x8D;
/// The way's text right of the name's end.
const WAY_GAP: i16 = 25;
/// The ways with a text of their own: 0 to 10.
pub const WAY_TEXTS: i32 = 11;

/// The tables of the names by kind and how many names each has; the getter of kind 4
/// has no limit.
const NAMES: [(u32, Option<u32>); 6] = [
    (0x8007_CA9C, Some(43)),
    (0x8007_CF14, Some(23)),
    (0x8007_D308, Some(15)),
    (0x8007_D590, Some(17)),
    (0x8007_CA44, None),
    (0x8007_CE54, Some(38)),
];
/// A byte per item and per thing of the story: the number of its picture.
const ITEM_PICTURES: (u32, u32) = (0x8007_CDD0, 43);
const STORY_PICTURES: (u32, u32) = (0x8007_CEEC, 38);
/// The records of the weapons, armours and ornaments, 24 bytes each, and how many.
const RECORDS: [(u32, u32); 3] = [(0x8007_CF70, 23), (0x8007_D344, 15), (0x8007_D5D4, 17)];
const RECORD_SIZE: usize = 24;
/// The picture by the first value of such a record.
const RECORD_PICTURES: u32 = 0x8007_D85C;
/// The picture of a thing without a record or past its table, and of every creature.
const NO_PICTURE: usize = 36;
const CREATURE_PICTURE: usize = 54;
/// What the item getter answers past its table: no image of the set.
const NO_ITEM_PICTURE: usize = 255;
/// The texts of the rows of money and of empty cocoons, and the "X" before a count.
const MONEY_LABEL: u32 = 0x8007_4774;
const COCOONS_LABEL: u32 = 0x8007_4780;
const TIMES_LABEL: u32 = 0x8007_4788;
/// The addresses of the eleven texts of the ways.
const WAYS: u32 = 0x8008_373C;

/// One thing of a notice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thing {
    /// The notice's number of the kind; [`NO_THING`] for none.
    pub kind: i32,
    pub number: i32,
    pub count: i32,
}

impl Thing {
    pub const NONE: Self = Self {
        kind: NO_THING,
        number: 0,
        count: 0,
    };

    /// A thing with a small picture and a name: kinds 0 to 5.
    fn is_named(&self) -> bool {
        (0..COCOONS).contains(&self.kind)
    }
}

/// A fixed text of a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Label {
    Money,
    /// Two Japanese letters that the text font cannot draw.
    Cocoons,
    /// The "X" before a count, in the small font.
    Times,
}

/// One thing the step draws into the box. A place is the upper left corner on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Piece {
    /// The small picture of the thing in a slot, at full brightness.
    Picture { slot: usize, at: (i16, i16) },
    /// The name of the thing in a slot, in the text font.
    Name { slot: usize, at: (i16, i16) },
    /// A fixed text: the small font for [`Label::Times`], the text font for the others.
    Label { label: Label, at: (i16, i16) },
    /// How many, in the text font from its place to the right (`0x8003DD0C`).
    Count { value: i32, at: (i16, i16) },
    /// An amount of money in the small font, ending at its place (`0x8003DFFC`).
    Amount { value: i32, right: (i16, i16) },
    /// The text of the way behind the name of the only thing.
    Way { way: i32, at: (i16, i16) },
    /// A line of text, counted from 0.
    Line { line: usize, at: (i16, i16) },
}

/// A notice as its step draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    /// The box, which gets the frame and the fill of the message window.
    pub rect: Rect,
    /// Whether the rows carry a count: more than one thing, or a way below 0.
    pub counted: bool,
    pub pieces: Vec<Piece>,
}

/// Lays a notice out as the step `0x80035C54` does.
///
/// `lines` is the number of lines of text and `widest` the width the game measures for
/// them (0 without lines); a first line that is only cue 0 is dropped by the caller.
/// `pictures` is the width of each thing's small picture and `name` the width of the
/// first thing's name.
pub fn laid_out(
    things: &[Thing; THINGS],
    way: i32,
    lines: usize,
    widest: i16,
    pictures: [i16; THINGS],
    name: i16,
) -> Notice {
    let there = things.iter().filter(|thing| thing.kind != NO_THING).count();
    let counted = there > 1 || (way < 0 && there != 0);
    let rows = (there + lines) as i16;
    let rect = Rect {
        x: LEFT,
        y: (3 - rows) * TOP_STEP + TOP,
        w: match lines {
            0 => WIDE,
            _ => WIDE.max(widest + EDGES),
        },
        h: (rows * ROW) | EDGES,
    };
    let mut pieces = Vec::new();
    let mut row = FIRST_ROW;
    let mut filled = 0;
    for (slot, thing) in things.iter().enumerate() {
        let times = |pieces: &mut Vec<Piece>, y: i16| {
            pieces.push(Piece::Label {
                label: Label::Times,
                at: (rect.x + TIMES_AT, y),
            });
            pieces.push(Piece::Count {
                value: thing.count,
                at: (rect.x + COUNT_AT, y),
            });
        };
        match thing.kind {
            MONEY => {
                let y = rect.y + row + 2;
                pieces.push(Piece::Label {
                    label: Label::Money,
                    at: (rect.x + INDENT, y),
                });
                pieces.push(Piece::Amount {
                    value: thing.count,
                    right: (rect.x + rect.w - 4, y),
                });
            }
            COCOONS => {
                let y = rect.y + row + 2;
                pieces.push(Piece::Label {
                    label: Label::Cocoons,
                    at: (rect.x + INDENT, y),
                });
                if counted {
                    times(&mut pieces, y);
                }
            }
            _ if thing.is_named() => {
                pieces.push(Piece::Picture {
                    slot,
                    at: (rect.x + INDENT, rect.y + row - 2),
                });
                let y = rect.y + row - 1;
                pieces.push(Piece::Name {
                    slot,
                    at: (rect.x + pictures[slot] + 9, y),
                });
                if counted {
                    times(&mut pieces, y);
                }
            }
            _ => continue,
        }
        row += ROW;
        filled += 1;
    }
    if (0..WAY_TEXTS).contains(&way) && !counted && things[0].is_named() {
        pieces.push(Piece::Way {
            way,
            at: (rect.x + name + WAY_GAP, rect.y + 6),
        });
    }
    for line in 0..lines {
        pieces.push(Piece::Line {
            line,
            at: (rect.x + INDENT, rect.y + 4 + (filled + line as i16) * ROW),
        });
    }
    Notice {
        rect,
        counted,
        pieces,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Shown {
    /// The step has run: the task's state word is 3, which the owner reads as "stands".
    stood: bool,
    /// Flag `0x800`: the owner lets a confirm count.
    allowed: bool,
    /// Flag `0x200`: a confirm was taken.
    taken: bool,
    /// Flag `0x4000`: the owner has closed the notice.
    closed: bool,
    gone: bool,
}

impl Shown {
    /// One step of the task, behind its drawing: a confirm counts when the owner allows
    /// it, and a closed notice ends. True in the step that ends it, which is where the
    /// notice does what it said with its things.
    pub fn step(&mut self, confirm: bool) -> bool {
        if self.gone {
            return false;
        }
        self.stood = true;
        if confirm && self.allowed {
            (self.allowed, self.taken) = (false, true);
        }
        if self.closed {
            (self.closed, self.gone) = (false, true);
        }
        self.gone
    }

    /// `0x80034A8C`: the notice stands and can be confirmed.
    pub fn stands(&self) -> bool {
        self.stood && !self.gone
    }

    /// `0x80034AC4`.
    pub fn allow(&mut self) {
        self.allowed = true;
    }

    /// `0x80034A10`: a confirm was taken. The notice's record says "at the end" with it.
    pub fn taken(&self) -> bool {
        self.taken
    }

    /// `0x80034AE0`.
    pub fn close(&mut self) {
        self.closed = true;
    }

    /// The task has ended.
    pub fn is_gone(&self) -> bool {
        self.gone
    }
}

pub fn opening_music(way: i32) -> Option<usize> {
    const GOT: usize = 1;
    (0..4).contains(&way).then_some(GOT)
}

/// The sound of one thing of a notice that ends, played before the thing is taken: the
/// ways 4 to 7, for an item and for a thing of the story, nothing for any other kind.
/// Two sounds: one for item 40 and most things of the story, the other for every other
/// item and six things of the story.
pub fn ending_sound(way: i32, kind: i32, number: i32) -> Option<u32> {
    const PLAIN: u32 = 30;
    const OTHER: u32 = 51;
    const ITEM: i32 = 0;
    const STORY_THING: i32 = 5;
    const OTHER_ITEM: i32 = 40;
    const PLAIN_STORY_THINGS: [i32; 6] = [2, 9, 16, 17, 18, 24];
    if !(4..8).contains(&way) {
        return None;
    }
    match kind {
        ITEM if number == OTHER_ITEM => Some(OTHER),
        ITEM => Some(PLAIN),
        STORY_THING if PLAIN_STORY_THINGS.contains(&number) => Some(PLAIN),
        STORY_THING => Some(OTHER),
        _ => None,
    }
}

/// A number as the game writes it (`0x8003E324`, in reading order as `0x8003E3A8` turns
/// it): full-width digits, with a full-width minus sign in front of a number below 0.
pub fn number_units(value: i32) -> Vec<u16> {
    let unit = |first: u8, second: u8| u16::from_le_bytes([first, second]);
    let mut left = i64::from(value).abs();
    let mut units = Vec::new();
    loop {
        units.push(unit(0x82, 0x4F + (left % 10) as u8));
        left /= 10;
        if left == 0 {
            break;
        }
    }
    if value < 0 {
        units.push(unit(0x81, 0x7C));
    }
    units.reverse();
    units
}

/// A number that ends at a place (`0x8003DFFC`): the drawer goes through the digits
/// from the last to the first and moves left by each glyph's width before it draws it.
pub fn amount_glyphs(font: &Font, value: i32, right: (i16, i16)) -> Vec<Glyph> {
    let mut x = right.0;
    let codes = font.codes(&number_units(value));
    (codes.iter().rev())
        .map(|&code| {
            x = x.wrapping_sub(font.width(code) as i16);
            Glyph {
                code,
                at: (x, right.1),
                cell: font.cell_of(code),
            }
        })
        .collect()
}

/// The names, pictures and short texts of the things, read from the loaded executable.
pub struct Tables<'a> {
    image: Image<'a>,
}

impl<'a> Tables<'a> {
    pub fn new(image: &'a [u8], base: u32) -> Self {
        Self {
            image: Image { data: image, base },
        }
    }

    /// The units of the text at an address, empty for an address outside the image.
    pub fn text(&self, address: u32) -> Vec<u16> {
        (address.checked_sub(self.image.base))
            .and_then(|offset| self.image.data.get(offset as usize..))
            .map(text::units)
            .unwrap_or_default()
    }

    /// The address of a thing's name (the getters behind `0x8007CA2C`), `None` for a
    /// number past the kind's table and for a kind without names.
    pub fn name(&self, kind: i32, number: i32) -> Option<u32> {
        let (table, names) = *NAMES.get(usize::try_from(kind).ok()?)?;
        let number = u32::try_from(number).ok()?;
        if names.is_some_and(|names| number >= names) {
            return None;
        }
        let address = rooms::step(table, number as usize, 4).ok()?;
        self.image.u32(address).ok().filter(|&name| name != 0)
    }

    /// The image of the fonts' file that is a thing's small picture (`0x8001D314`,
    /// `0x8001D33C`, `0x8001CDD4`, `0x8001D3AC`); `None` for a kind without a picture
    /// and for an item past its table, which the game answers with no image of the set.
    pub fn picture(&self, kind: i32, number: i32) -> Option<usize> {
        let byte = |(table, entries): (u32, u32), otherwise: usize| match u32::try_from(number)
            .ok()
            .filter(|&n| n < entries)
        {
            Some(n) => self.image.u8(table + n).ok().map(usize::from),
            None => Some(otherwise),
        };
        match kind {
            0 => byte(ITEM_PICTURES, NO_ITEM_PICTURE).filter(|&picture| picture != NO_ITEM_PICTURE),
            1..=3 => {
                let (records, entries) = RECORDS[kind as usize - 1];
                let Some(n) = u32::try_from(number).ok().filter(|&n| n < entries) else {
                    return Some(NO_PICTURE);
                };
                let record = rooms::step(records, n as usize, RECORD_SIZE).ok()?;
                let look = usize::try_from(self.image.i16(record).ok()?).ok()?;
                let picture = self.image.i16(rooms::step(RECORD_PICTURES, look, 2).ok()?);
                usize::try_from(picture.ok()?).ok()
            }
            4 => Some(CREATURE_PICTURE),
            5 => byte(STORY_PICTURES, NO_PICTURE),
            _ => None,
        }
    }

    /// The text of a fixed label.
    pub fn label(&self, label: Label) -> Vec<u16> {
        self.text(match label {
            Label::Money => MONEY_LABEL,
            Label::Cocoons => COCOONS_LABEL,
            Label::Times => TIMES_LABEL,
        })
    }

    /// The short text of a way from 0 to 10.
    pub fn way(&self, way: i32) -> Option<Vec<u16>> {
        let way = usize::try_from(way)
            .ok()
            .filter(|&way| way < WAY_TEXTS as usize)?;
        let address = self.image.u32(rooms::step(WAYS, way, 4).ok()?).ok()?;
        Some(self.text(address))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_notice_takes_a_confirm_only_with_its_owners_leave_and_ends_when_closed() {
        let mut shown = Shown::default();
        assert!(!shown.stands());
        // A confirm before the owner allows it does not count.
        assert!(!shown.step(true));
        assert!(shown.stands() && !shown.taken());
        shown.allow();
        assert!(!shown.step(false));
        assert!(!shown.taken());
        assert!(!shown.step(true));
        assert!(shown.taken() && !shown.is_gone());
        // The owner closes it; the next step is its last.
        shown.close();
        assert!(shown.step(false));
        assert!(shown.is_gone() && !shown.stands());
        assert!(!shown.step(true));
    }

    #[test]
    fn a_notice_asks_for_its_music_and_its_sounds_by_the_way() {
        // Only the ways 0 to 3 ask for the music; way -1 hands things over without it.
        let asked: Vec<i32> = (-1..12)
            .filter(|&way| opening_music(way).is_some())
            .collect();
        assert_eq!(asked, [0, 1, 2, 3]);
        assert_eq!(opening_music(0), Some(1));
        // The ways 4 to 7: an item, a thing of the story, and nothing else.
        for way in 4..8 {
            assert_eq!(ending_sound(way, 0, 12), Some(30));
            assert_eq!(ending_sound(way, 0, 40), Some(51));
            assert_eq!(ending_sound(way, 5, 3), Some(51));
            for number in [2, 9, 16, 17, 18, 24] {
                assert_eq!(ending_sound(way, 5, number), Some(30));
            }
            for kind in [NO_THING, 1, 2, 3, 4, 6, 7] {
                assert_eq!(ending_sound(way, kind, 40), None);
            }
        }
        for way in [-1, 0, 3, 8, 11] {
            assert_eq!(ending_sound(way, 0, 12), None);
        }
    }

    fn thing(kind: i32, number: i32, count: i32) -> Thing {
        Thing {
            kind,
            number,
            count,
        }
    }

    #[test]
    fn one_thing_has_its_way_behind_the_name() {
        let things = [thing(5, 3, 1), Thing::NONE, Thing::NONE];
        let notice = laid_out(&things, 1, 0, 0, [16, 0, 0], 60);
        // One row: the box is 24 high and 18 below the top of a box of three rows.
        let rect = Rect {
            x: 47,
            y: 108,
            w: 227,
            h: 24,
        };
        assert_eq!((notice.rect, notice.counted), (rect, false));
        assert_eq!(
            notice.pieces,
            [
                Piece::Picture {
                    slot: 0,
                    at: (53, 113)
                },
                Piece::Name {
                    slot: 0,
                    at: (72, 114)
                },
                Piece::Way {
                    way: 1,
                    at: (132, 114)
                },
            ]
        );
        // Way 11 has no text, and a way below 0 counts instead.
        let plain = laid_out(&things, 11, 0, 0, [16, 0, 0], 60);
        assert_eq!(plain.pieces.len(), 2);
        let counted = laid_out(&things, -1, 0, 0, [16, 0, 0], 60);
        assert!(counted.counted);
        assert_eq!(
            counted.pieces[2..],
            [
                Piece::Label {
                    label: Label::Times,
                    at: (179, 114)
                },
                Piece::Count {
                    value: 1,
                    at: (188, 114)
                },
            ]
        );
    }

    #[test]
    fn several_things_are_counted_row_by_row() {
        let things = [thing(7, 0, 200), thing(6, 0, 12), thing(0, 14, 2)];
        let notice = laid_out(&things, 1, 0, 0, [0, 0, 12], 0);
        let rect = Rect {
            x: 47,
            y: 90,
            w: 227,
            h: 56,
        };
        assert_eq!((notice.rect, notice.counted), (rect, true));
        let label = |label, at| Piece::Label { label, at };
        assert_eq!(
            notice.pieces,
            [
                label(Label::Money, (53, 99)),
                Piece::Amount {
                    value: 200,
                    right: (270, 99)
                },
                label(Label::Cocoons, (53, 115)),
                label(Label::Times, (179, 115)),
                Piece::Count {
                    value: 12,
                    at: (188, 115)
                },
                Piece::Picture {
                    slot: 2,
                    at: (53, 127)
                },
                Piece::Name {
                    slot: 2,
                    at: (68, 128)
                },
                label(Label::Times, (179, 128)),
                Piece::Count {
                    value: 2,
                    at: (188, 128)
                },
            ]
        );
    }

    #[test]
    fn lines_follow_the_things_and_widen_the_box() {
        let things = [thing(0, 1, 1), Thing::NONE, Thing::NONE];
        let notice = laid_out(&things, 11, 2, 240, [16, 0, 0], 40);
        assert_eq!((notice.rect.w, notice.rect.h, notice.rect.y), (248, 56, 90));
        assert_eq!(
            notice.pieces[2..],
            [
                Piece::Line {
                    line: 0,
                    at: (53, 110)
                },
                Piece::Line {
                    line: 1,
                    at: (53, 126)
                },
            ]
        );
        // Lines only: a narrow line leaves the box at its width.
        let lines = laid_out(&[Thing::NONE; THINGS], 11, 1, 100, [0; THINGS], 0);
        assert_eq!((lines.rect.w, lines.rect.h, lines.rect.y), (227, 24, 108));
        assert_eq!(
            lines.pieces,
            [Piece::Line {
                line: 0,
                at: (53, 112)
            }]
        );
    }

    #[test]
    fn a_number_is_full_width_digits() {
        let unit = |first: u8, second: u8| u16::from_le_bytes([first, second]);
        assert_eq!(number_units(0), [unit(0x82, 0x4F)]);
        assert_eq!(
            number_units(205),
            [unit(0x82, 0x51), unit(0x82, 0x4F), unit(0x82, 0x54)]
        );
        assert_eq!(number_units(-7), [unit(0x81, 0x7C), unit(0x82, 0x56)]);
        assert_eq!(number_units(i32::MIN).len(), 11);
    }

    #[test]
    fn an_amount_ends_at_its_place() {
        // Codes 1 to 10 are the digits 0 to 9; the first two are 3 wide, the rest 5.
        let mut limits = [0; 12];
        limits[4] = 10;
        limits[6] = 2;
        let font = Font {
            image: 0,
            columns: 5,
            cell: (10, 8),
            palette: (0, 0),
            page: 0,
            last: 10,
            table: (0..10u8)
                .map(|digit| u16::from_le_bytes([0x82, 0x4F + digit]))
                .collect(),
            limits,
        };
        let glyphs = amount_glyphs(&font, 105, (100, 20));
        // From the last digit: 5 is 5 wide, 0 is 3 wide, 1 is 3 wide.
        let places: Vec<(u16, i16)> = glyphs
            .iter()
            .map(|glyph| (glyph.code, glyph.at.0))
            .collect();
        assert_eq!(places, [(6, 95), (1, 92), (2, 89)]);
        assert_eq!(glyphs[0].cell, Some((0, 8)));
    }

    #[test]
    fn the_tables_answer_by_kind() {
        // An image from the first label up to the texts of the ways, all zero but what
        // the test puts in.
        let base = MONEY_LABEL;
        let mut image = vec![0u8; (WAYS - base) as usize + 64];
        let at = |address: u32| (address - base) as usize;
        let mut word = |address: u32, value: u32| {
            image[at(address)..at(address) + 4].copy_from_slice(&value.to_le_bytes());
        };
        word(NAMES[5].0 + 4 * 3, MONEY_LABEL);
        word(WAYS + 4, TIMES_LABEL);
        // The record of armour 2 names look 5, whose picture is 44.
        word(RECORDS[1].0 + 2 * RECORD_SIZE as u32, 5);
        word(RECORD_PICTURES + 2 * 5, 44);
        image[at(MONEY_LABEL)..at(MONEY_LABEL) + 2].copy_from_slice(&[0x82, 0x60]);
        image[at(ITEM_PICTURES.0) + 14] = 21;
        image[at(STORY_PICTURES.0) + 3] = 40;
        let tables = Tables::new(&image, base);
        assert_eq!(tables.name(5, 3), Some(MONEY_LABEL));
        assert_eq!(
            (tables.name(5, 38), tables.name(5, 4), tables.name(6, 0)),
            (None, None, None)
        );
        assert_eq!(tables.text(MONEY_LABEL), [0x6082]);
        assert_eq!(tables.label(Label::Money), [0x6082]);
        assert_eq!((tables.way(1), tables.way(11)), (Some(Vec::new()), None));
        assert_eq!(
            (tables.picture(0, 14), tables.picture(0, 43)),
            (Some(21), None)
        );
        assert_eq!(
            (tables.picture(5, 3), tables.picture(5, 38)),
            (Some(40), Some(36))
        );
        assert_eq!(
            (tables.picture(2, 2), tables.picture(2, 15)),
            (Some(44), Some(36))
        );
        assert_eq!(
            (tables.picture(4, 9), tables.picture(7, 0)),
            (Some(54), None)
        );
    }
}
