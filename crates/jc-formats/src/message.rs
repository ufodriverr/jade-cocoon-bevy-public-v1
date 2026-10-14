
use crate::pad::Pad;
use crate::text::{self, Font, Glyph};

/// How far below a line the next one starts.
pub const LINE: i16 = 12;
/// The number of letters of a page that shows all of them.
pub const ALL: u32 = 0x0FFF_FFFF;
/// The first line of the window at the bottom of the screen with the speaker's picture
/// on the right, as the window's opener computes it.
pub const BOTTOM_LINE: (i16, i16) = (23, 175);

/// Held buttons that hurry a page, and buttons that hurry it when they go down.
const HURRY_HELD: u16 = 0x60F;
const HURRY_PRESSED: u16 = 0xF0;

/// What the text routine does with a code of the text font.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    /// A glyph: one letter.
    Glyph,
    /// The palette row of the glyphs from here on.
    Colour(u8),
    /// The player's name is put in.
    Name,
    /// One of five texts is put in.
    Inserted(u8),
    /// A cue: the page says how far it has come.
    Cue(u8),
    /// A text that is shown from its beginning only the first time, and its mark after
    /// that first time.
    Once,
    Again,
    Nothing,
}

impl Unit {
    pub fn of(code: u16) -> Self {
        match code {
            1..=180 => Self::Glyph,
            181 => Self::Colour(0),
            182 => Self::Colour(1),
            183 => Self::Colour(3),
            184..=187 => Self::Colour((code - 181) as u8),
            188 | 189 => Self::Name,
            190..=194 => Self::Inserted((code - 190) as u8),
            199..=246 => Self::Cue((code - 199) as u8),
            247 => Self::Once,
            248 => Self::Again,
            _ => Self::Nothing,
        }
    }

    /// Whether an item that holds the unit does not move the place down a line.
    pub fn is_mark(self) -> bool {
        matches!(self, Self::Cue(_) | Self::Once | Self::Again)
    }
}

/// One group of a room's text: what one message command shows. Items are rows of units
/// as they stand on the disc.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Group {
    /// The pages: the items between two null entries.
    pub pages: Vec<Vec<Vec<u16>>>,
    /// The answers of a question, behind the two null entries that end the pages.
    pub answers: Vec<Vec<u16>>,
}

fn word(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        data.get(at..)?.get(..4)?.try_into().ok()?,
    ))
}

/// The groups of a room's text resource (kind 1 of the room data's resources).
pub fn groups(resource: &[u8]) -> Option<Vec<Group>> {
    // The block's own size comes first; every offset counts from the word behind it.
    let body = resource.get(4..)?;
    let count = word(body, 0)? as usize;
    if count > body.len() / 8 {
        return None;
    }
    (0..count)
        .map(|group| {
            let list = word(body, 4 + 8 * group)? as usize;
            let entries = word(body, 8 + 8 * group)? as usize;
            if entries > body.len() / 4 {
                return None;
            }
            let items = (0..entries)
                .map(|entry| match word(body, list.checked_add(4 * entry)?)? {
                    u32::MAX => Some(None),
                    item => Some(Some(text::units(body.get(item as usize..)?))),
                })
                .collect::<Option<Vec<_>>>()?;
            Some(Group::of(items))
        })
        .collect()
}

/// The largest value of a message command that is a group of the room's text; a larger
/// one is the address of a list (`0x800A66C8`).
pub const LAST_GROUP: u32 = 0xFFFE;

/// The most entries a list in memory is read for.
const LISTED: usize = 256;

pub fn listed<'a>(
    list: u32,
    question: bool,
    from: impl Fn(u32) -> Option<&'a [u8]>,
) -> Option<Group> {
    let words = from(list)?;
    let mut items = Vec::new();
    let (mut answers, mut page_is_empty) = (false, true);
    for entry in 0..LISTED {
        let item = match word(words, 4 * entry)? {
            0 => None,
            item => Some(text::units(from(item)?)),
        };
        let is_null = item.is_none();
        items.push(item);
        if answers {
            // A null entry ends the answers.
            if is_null {
                return Some(Group::of(items));
            }
        } else if is_null && page_is_empty {
            // A page without an item ends the pages.
            if !question {
                return Some(Group::of(items));
            }
            answers = true;
        } else {
            page_is_empty = is_null;
        }
    }
    None
}

impl Group {
    /// Cuts a list of items, a null entry as `None`, into pages and answers.
    fn of(items: Vec<Option<Vec<u16>>>) -> Self {
        let mut group = Self::default();
        let mut items = items.into_iter();
        let mut page = Vec::new();
        for item in items.by_ref() {
            match item {
                Some(item) => page.push(item),
                None if page.is_empty() => break,
                None => group.pages.push(std::mem::take(&mut page)),
            }
        }
        if !page.is_empty() {
            group.pages.push(page);
        }
        group.answers = items.map_while(|item| item).collect();
        group
    }
}

/// A glyph of a page and the palette row it is drawn with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Letter {
    pub glyph: Glyph,
    pub row: u8,
}

/// What one pass of the text routine draws of a page.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Shown {
    pub letters: Vec<Letter>,
    /// The cues the page has reached, in the order of the text.
    pub cues: Vec<u8>,
    /// Every item went through: the page is whole.
    pub whole: bool,
}

/// A page drawn up to a number of letters, the first line's cell at `at`. `name` holds
/// the units of the player's name. The five inserted texts are left out: nothing is
/// known to fill them.
pub fn shown(font: &Font, page: &[Vec<u16>], at: (i16, i16), letters: u32, name: &[u16]) -> Shown {
    let mut shown = Shown::default();
    let (mut x, mut y) = at;
    let mut row = 0;
    for item in page {
        let codes = font.codes(item);
        let mut codes = codes.iter().copied();
        // A text that is put in is read before the item goes on.
        let mut put_in: Vec<u16> = Vec::new();
        let mut is_mark = false;
        while let Some(code) = put_in.pop().or_else(|| codes.next()) {
            if letters as usize <= shown.letters.len() {
                return shown;
            }
            match Unit::of(code) {
                Unit::Glyph => {
                    let glyph = Glyph {
                        code,
                        at: (x, y),
                        cell: font.cell_of(code),
                    };
                    shown.letters.push(Letter { glyph, row });
                    x = x.wrapping_add(font.width(code) as i16);
                }
                Unit::Colour(to) => row = to,
                Unit::Name => put_in = font.codes(name).into_iter().rev().collect(),
                Unit::Cue(cue) => {
                    shown.cues.push(cue);
                    is_mark = true;
                }
                Unit::Once | Unit::Again => is_mark = true,
                Unit::Inserted(_) | Unit::Nothing => {}
            }
        }
        if !is_mark {
            x = at.0;
            y = y.wrapping_add(LINE);
        }
    }
    shown.whole = true;
    shown
}

/// How wide a text is in the text font (`0x8003D260`): the widths of its glyphs, with
/// the player's name put in. A control unit has no width.
pub fn width(font: &Font, item: &[u16], name: &[u16]) -> i16 {
    let glyphs = |codes: &[u16]| {
        (codes.iter())
            .filter(|&&code| Unit::of(code) == Unit::Glyph)
            .fold(0i16, |sum, &code| sum.wrapping_add(font.width(code) as i16))
    };
    (font.codes(item).iter()).fold(0i16, |sum, &code| match Unit::of(code) {
        Unit::Glyph => sum.wrapping_add(font.width(code) as i16),
        Unit::Name => sum.wrapping_add(glyphs(&font.codes(name))),
        _ => sum,
    })
}

/// One line as the answers' drawer `0x8003D9C4` draws it: glyphs and the colour codes,
/// and nothing for any other control unit.
pub fn line(font: &Font, item: &[u16], at: (i16, i16)) -> Vec<Letter> {
    let mut x = at.0;
    let mut row = 0;
    let mut letters = Vec::new();
    for code in font.codes(item) {
        match Unit::of(code) {
            Unit::Glyph => {
                let glyph = Glyph {
                    code,
                    at: (x, at.1),
                    cell: font.cell_of(code),
                };
                letters.push(Letter { glyph, row });
                x = x.wrapping_add(font.width(code) as i16);
            }
            Unit::Colour(to) => row = to,
            _ => {}
        }
    }
    letters
}

/// Whether the pad's words of a pass hurry a page that can be hurried.
pub fn hurries(pad: &Pad) -> bool {
    pad.held & HURRY_HELD != 0 || pad.pressed & HURRY_PRESSED != 0
}

/// The count of a page's letters from pass to pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Typing {
    /// The letters shown.
    pub letters: u32,
    /// Passes since the last new letter.
    pub passes: u32,
}

impl Typing {
    /// The start of a pass: the number of letters the page is drawn with. `setting` is
    /// the game's setting word, whose low three bits are the speed.
    pub fn begin(&mut self, setting: u32, typed: bool) -> u32 {
        if 3u32.wrapping_sub(setting & 7) <= self.passes {
            self.passes = 0;
            self.letters = self.letters.wrapping_add(1);
        }
        if !typed {
            self.letters = ALL;
        }
        self.letters
    }

    /// The end of a pass, with what the drawing said and whether the page is hurried.
    pub fn end(&mut self, whole: bool, hurried: bool) {
        match (whole, hurried) {
            (true, _) => {}
            (false, true) => self.letters = ALL,
            (false, false) => self.passes += 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The unit of a code in the test font. Its first byte is never zero.
    fn unit(code: u16) -> u16 {
        0x9000 + code
    }

    /// A font with the text font's grid in which every glyph is 13 wide.
    fn font() -> Font {
        let mut limits = [0; 12];
        limits[0] = 180;
        Font {
            image: 18,
            columns: 18,
            cell: (14, 14),
            palette: (768, 258),
            page: 180,
            last: 180,
            table: (1..=248).map(unit).collect(),
            limits,
        }
    }

    fn line(codes: &[u16]) -> Vec<u16> {
        codes.iter().map(|&code| unit(code)).collect()
    }

    #[test]
    fn codes_behind_the_glyphs_are_control_units() {
        assert_eq!(Unit::of(1), Unit::Glyph);
        assert_eq!(Unit::of(180), Unit::Glyph);
        assert_eq!(Unit::of(181), Unit::Colour(0));
        assert_eq!(Unit::of(182), Unit::Colour(1));
        assert_eq!(Unit::of(183), Unit::Colour(3));
        assert_eq!(Unit::of(184), Unit::Colour(3));
        assert_eq!(Unit::of(187), Unit::Colour(6));
        assert_eq!(Unit::of(189), Unit::Name);
        assert_eq!(Unit::of(194), Unit::Inserted(4));
        assert_eq!(Unit::of(198), Unit::Nothing);
        assert_eq!(Unit::of(199), Unit::Cue(0));
        assert_eq!(Unit::of(246), Unit::Cue(47));
        assert_eq!(Unit::of(248), Unit::Again);
        assert_eq!(Unit::of(0x8281), Unit::Nothing);
        assert!(Unit::of(200).is_mark() && Unit::of(247).is_mark());
        assert!(!Unit::of(181).is_mark() && !Unit::of(5).is_mark());
    }

    /// A text resource with one group: its entries, `None` for a null entry.
    fn resource(entries: &[Option<&[u16]>]) -> Vec<u8> {
        // The body: count, one pair, the list, the items.
        let list = 12;
        let mut items = Vec::new();
        let mut body = [1u32, list, entries.len() as u32]
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect::<Vec<u8>>();
        for entry in entries {
            let offset = match entry {
                Some(units) => {
                    let at = list as usize + 4 * entries.len() + items.len();
                    items.extend(units.iter().flat_map(|unit| unit.to_le_bytes()));
                    items.extend([0, 0]);
                    at as u32
                }
                None => u32::MAX,
            };
            body.extend(offset.to_le_bytes());
        }
        body.extend(items);
        let mut out = ((body.len() + 4) as u32).to_le_bytes().to_vec();
        out.extend(body);
        out
    }

    #[test]
    fn a_group_is_pages_and_answers() {
        let (cue0, cue1) = ([unit(199)], [unit(200)]);
        let (one, two, yes, no) = (line(&[1, 2]), line(&[3]), line(&[4, 5, 6]), line(&[7]));
        let bytes = resource(&[
            Some(&cue0),
            Some(&one),
            Some(&two),
            None,
            Some(&cue1),
            Some(&one),
            None,
            None,
            Some(&yes),
            Some(&no),
            None,
        ]);
        let groups = groups(&bytes).expect("a text resource");
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].pages.len(), 2);
        assert_eq!(groups[0].pages[0], [cue0.to_vec(), one.clone(), two]);
        assert_eq!(groups[0].pages[1], [cue1.to_vec(), one.clone()]);
        assert_eq!(groups[0].answers, [yes, no]);
        // Without answers the group ends with its two null entries.
        let bytes = resource(&[Some(&cue0), Some(&one), None, None]);
        let plain = &super::groups(&bytes).expect("a text resource")[0];
        assert_eq!((plain.pages.len(), plain.answers.len()), (1, 0));
        // An offset behind the block is an error.
        assert_eq!(super::groups(&bytes[..bytes.len() - 8]), None);
    }

    #[test]
    fn a_list_in_memory_is_a_group_too() {
        // Memory from `BASE` on: the list first, then its items, each with a null unit.
        const BASE: u32 = 0x8000_1000;
        let memory = |entries: &[Option<&[u16]>], behind: u32| {
            let mut texts = Vec::new();
            let mut list = Vec::new();
            for entry in entries {
                let address = entry.map_or(0, |units| {
                    let at = BASE + 4 * (entries.len() as u32 + 1) + texts.len() as u32;
                    texts.extend(units.iter().flat_map(|unit| unit.to_le_bytes()));
                    texts.extend([0, 0]);
                    at
                });
                list.extend(address.to_le_bytes());
            }
            list.extend(behind.to_le_bytes());
            list.extend(texts);
            list
        };
        let (one, two, yes, no) = (line(&[1, 2]), line(&[3]), line(&[4, 5, 6]), line(&[7]));
        let entries = [
            Some(&one[..]),
            None,
            Some(&two[..]),
            None,
            None,
            Some(&yes[..]),
            Some(&no[..]),
            None,
        ];
        let bytes = memory(&entries, 9);
        let from = |address: u32| bytes.get(address.checked_sub(BASE)? as usize..);
        let asked = listed(BASE, true, from).expect("a question");
        assert_eq!(asked.pages, [[one.clone()], [two.clone()]]);
        assert_eq!(asked.answers, [yes.clone(), no.clone()]);
        // A message's list ends with its two null entries: what lies behind is not read.
        let bytes = memory(&entries[..5], 9);
        let from = |address: u32| bytes.get(address.checked_sub(BASE)? as usize..);
        let said = listed(BASE, false, from).expect("a message");
        assert_eq!((said.pages.len(), said.answers.len()), (2, 0));
        // An item outside the memory is an error, and so is a list outside it.
        assert_eq!(listed(BASE, true, from), None);
        assert_eq!(listed(BASE - 4, false, from), None);
    }

    #[test]
    fn a_page_is_drawn_line_by_line() {
        let font = font();
        // The cue, a line with a marked word, a line with the name in it.
        let page = [
            line(&[199]),
            line(&[1, 183, 2, 3, 181, 4]),
            line(&[5, 188, 6]),
        ];
        let name = line(&[7, 8]);
        let shown = shown(&font, &page, (23, 175), ALL, &name);
        assert!(shown.whole);
        assert_eq!(shown.cues, [0]);
        let codes: Vec<u16> = shown.letters.iter().map(|l| l.glyph.code).collect();
        assert_eq!(codes, [1, 2, 3, 4, 5, 7, 8, 6]);
        let rows: Vec<u8> = shown.letters.iter().map(|l| l.row).collect();
        assert_eq!(rows, [0, 3, 3, 0, 0, 0, 0, 0]);
        let at: Vec<(i16, i16)> = shown.letters.iter().map(|l| l.glyph.at).collect();
        // The cue's item does not take a line; every glyph is 13 wide.
        assert_eq!(&at[..4], [(23, 175), (36, 175), (49, 175), (62, 175)]);
        assert_eq!(&at[4..], [(23, 187), (36, 187), (49, 187), (62, 187)]);
        assert_eq!(shown.letters[1].glyph.cell, Some((14, 0)));
    }

    #[test]
    fn a_page_is_drawn_up_to_its_letters() {
        let font = font();
        let page = [line(&[199]), line(&[1, 2]), line(&[200]), line(&[3])];
        // Without a letter not even the first cue is reached.
        let none = shown(&font, &page, (0, 0), 0, &[]);
        assert_eq!(
            (none.letters.len(), none.cues.len(), none.whole),
            (0, 0, false)
        );
        // The second cue is reached with the letters before it.
        let two = shown(&font, &page, (0, 0), 2, &[]);
        assert_eq!(
            (two.letters.len(), two.cues.clone(), two.whole),
            (2, vec![0], false)
        );
        let three = shown(&font, &page, (0, 0), 3, &[]);
        assert_eq!((three.letters.len(), three.cues.clone()), (3, vec![0, 1]));
        // The page is whole in the pass that draws its last letter.
        assert!(three.whole);
        // A mark between two lines keeps the second on the line it stood on.
        assert_eq!(three.letters[2].glyph.at, (0, 12));
    }

    #[test]
    fn a_line_has_the_width_of_its_glyphs() {
        let font = font();
        let name = line(&[7, 8]);
        // Two glyphs, a colour, the name of two glyphs, a cue: four glyphs of 13.
        let item = line(&[1, 183, 2, 188, 199]);
        assert_eq!(width(&font, &item, &name), 52);
        assert_eq!(width(&font, &item, &[]), 26);
        assert_eq!(width(&font, &[], &name), 0);
        // The answers' drawer knows the colours and leaves the name out.
        let letters = super::line(&font, &item, (34, 90));
        let at: Vec<(i16, i16)> = letters.iter().map(|l| l.glyph.at).collect();
        assert_eq!(at, [(34, 90), (47, 90)]);
        assert_eq!((letters[0].row, letters[1].row), (0, 3));
    }

    #[test]
    fn letters_come_by_the_setting() {
        let count = |setting: u32, passes: usize| -> Vec<u32> {
            let mut typing = Typing::default();
            (0..passes)
                .map(|_| {
                    let letters = typing.begin(setting, true);
                    typing.end(false, false);
                    letters
                })
                .collect()
        };
        assert_eq!(count(0, 7), [0, 0, 0, 1, 1, 1, 2]);
        assert_eq!(count(2, 3), [0, 1, 2]);
        assert_eq!(count(3, 3), [1, 2, 3]);
        // A page that is not typed has all its letters, and a hurried one from the next
        // pass on.
        let mut typing = Typing::default();
        assert_eq!(typing.begin(0, false), ALL);
        let mut typing = Typing::default();
        assert_eq!(typing.begin(0, true), 0);
        typing.end(false, true);
        assert_eq!(typing.begin(0, true), ALL);
        // A whole page counts no more passes.
        let mut typing = Typing {
            letters: 9,
            passes: 1,
        };
        typing.end(true, true);
        assert_eq!(
            typing,
            Typing {
                letters: 9,
                passes: 1
            }
        );
    }
}
