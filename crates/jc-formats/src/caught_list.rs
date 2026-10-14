
use crate::menu::{self, Rect};
use crate::pad::{self, Pad};
use crate::player_menu::{SOUND_CANCEL, SOUND_CURSOR};
use crate::player_panel::{Drawn, Listed, in_capitals, sprite};
use crate::rooms::Image;

/// The species of the game, the pages of the list and the cells of a page: four
/// columns of sixteen rows, filled row by row.
pub const SPECIES: u32 = 211;
pub const PAGES: i32 = 3;
pub const ON_A_PAGE: i32 = 64;
pub const ACROSS: i32 = 4;
/// The cells of the whole list, which the box of the count names.
pub const CELLS: i32 = 171;
/// The five species that share one cell.
pub const SHARED: std::ops::RangeInclusive<u32> = 36..=40;

/// In the menu's file: the piece of the heading and the text of a cell's mark.
pub const HEADING_AT: u32 = 0x800B_BB74;
pub const MARK_AT: u32 = 0x800B_BB90;
/// The executable's table of the species' names.
const SPECIES_NAMES: u32 = 0x8007_A094;
/// The bytes of a name the list copies.
const NAME_BYTES: usize = 22;

/// The heading: its frame, the margin of its content, the flag word of its piece and
/// its sprite, "#Captured".
pub const HEADING: Rect = rect(116, 16, 88, 18);
pub const HEADING_MARGIN: (i16, i16) = (14, 2);
pub const HEADING_FLAGS: u16 = 0x10;
/// The box of the page with its word, and the box of the count.
pub const PAGE_BOX: Rect = rect(10, 16, 88, 18);
pub const COUNT_BOX: Rect = rect(224, 16, 88, 18);
pub const PAGE_WORD: usize = 148;
const PAGE_WORD_AT: (i16, i16) = (26, 20);
/// Where the first number of a box ends and its stroke starts, and where the second
/// number ends.
const PAGE_ENDS: [(i16, i16); 2] = [(66, 22), (81, 22)];
const COUNT_ENDS: [(i16, i16); 2] = [(264, 22), (292, 22)];
/// The frame round the cells, the first cell's mark, the steps to the next column and
/// row, and how far right of the mark a name starts.
pub const CELLS_FRAME: Rect = rect(14, 41, 292, 162);
const FIRST_MARK: (i16, i16) = (18, 43);
const STEP: (i16, i16) = (72, 10);
const NAME_RIGHT: i16 = 8;
/// The two scroll signs: quarter turns and place.
pub const SIGNS: [(u16, (i16, i16)); 2] = [(2, (8, 122)), (0, (312, 122))];

const SCREEN: Rect = rect(0, 0, 320, 240);

const fn rect(x: i16, y: i16, w: i16, h: i16) -> Rect {
    Rect { x, y, w, h }
}

/// Whether a species has a cell in the list (`0x800BDF90`).
pub fn listed(species: u32) -> bool {
    matches!(species, 0..=31 | 34..=40 | 57..=192)
}

/// The cell of every listed species, in the order of the species (`0x800BDDF8`): each
/// takes the next cell, but the five of [`SHARED`] take one together.
pub fn cells() -> impl Iterator<Item = (i32, u32)> {
    let mut next = 0;
    (0..SPECIES)
        .filter(|&species| listed(species))
        .map(move |species| {
            let cell = next;
            if !SHARED.contains(&species) || species == *SHARED.end() {
                next += 1;
            }
            (cell, species)
        })
}

/// The 224 bits of the caught kinds (`0x8008BD14`), a bit for each species.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Kinds(pub [u32; 7]);

impl Kinds {
    /// A capture marks the species (`0x800BFBC0` of the fights' file).
    pub fn mark(&mut self, species: u32) {
        if let Some(word) = self.0.get_mut((species / 32) as usize) {
            *word |= 1 << (species % 32);
        }
    }

    /// Whether a species is marked (`0x800BDDB4`).
    pub fn holds(&self, species: u32) -> bool {
        let word = self.0.get((species / 32) as usize);
        word.is_some_and(|word| word >> (species % 32) & 1 != 0)
    }

    /// The count the list shows (`0x800125B8`): every set bit, a cell or not.
    pub fn count(&self) -> u32 {
        self.0.iter().map(|word| word.count_ones()).sum()
    }
}

/// What a pass of the list did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Turned {
    /// The sounds asked for, in their order.
    pub sounds: Vec<u8>,
    /// The task has the menu's routine again from the next pass on.
    pub ends: bool,
}

/// The list while the menu's task runs it (`0x800BD9E8`). The page is a word of the
/// menu's file and stands from one visit of the list to the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KindList {
    pub page: i32,
}

impl KindList {
    /// One pass with the pad's words of the pass: a cancel goes back to the menu, and
    /// right and left, pressed or repeated, turn the pages round and round.
    pub fn pass(&mut self, pad: &Pad) -> Turned {
        let mut turned = Turned::default();
        if menu::cancelled(pad) {
            turned.sounds.push(SOUND_CANCEL);
            turned.ends = true;
            return turned;
        }
        for (button, step) in [(pad::RIGHT, 1), (pad::LEFT, -1)] {
            if pad.repeated & button != 0 {
                turned.sounds.push(SOUND_CURSOR);
                self.page += step;
            }
        }
        if self.page < 0 {
            self.page = PAGES - 1;
        }
        if self.page >= PAGES {
            self.page = 0;
        }
        turned
    }
}

/// A box above the cells: its frame, and a small number, the stroke and a second small
/// number on it.
fn pair(frame: Rect, values: (i32, i32), ends: [(i16, i16); 2]) -> [Drawn; 4] {
    let number = |value: i32, ends: (i16, i16)| Drawn::Number {
        value,
        ends,
        small: true,
        clip: SCREEN,
    };
    [
        Drawn::Frame {
            rect: frame,
            mode: 0,
            time: 0,
        },
        number(values.1, ends[1]),
        Drawn::Sprite {
            number: sprite::STROKE,
            at: ends[0],
            clip: SCREEN,
        },
        number(values.0, ends[0]),
    ]
}

/// What a pass of the list draws, from what lies underneath to what lies on top.
/// `whole` is false in the pass of the cancel, which draws only the frame round the
/// cells and the two signs.
pub fn drawn(list: &KindList, kinds: &Kinds, whole: bool, time: u32) -> Vec<Drawn> {
    let mut drawn: Vec<Drawn> = (SIGNS.iter().rev())
        .map(|&(turns, at)| Drawn::Sign { turns, at, time })
        .collect();
    drawn.push(Drawn::Frame {
        rect: CELLS_FRAME,
        mode: 0,
        time: 0,
    });
    if !whole {
        return drawn;
    }
    let first = list.page * ON_A_PAGE;
    let shown: Vec<(i32, u32)> = cells()
        .filter(|(cell, _)| (first..first + ON_A_PAGE).contains(cell))
        .collect();
    for &(cell, species) in shown.iter().rev() {
        let (column, row) = ((cell % ACROSS) as i16, ((cell - first) / ACROSS) as i16);
        let at = (FIRST_MARK.0 + STEP.0 * column, FIRST_MARK.1 + STEP.1 * row);
        if kinds.holds(species) {
            drawn.push(Drawn::KindName {
                species,
                at: (at.0 + NAME_RIGHT, at.1),
            });
        }
        drawn.push(Drawn::KindMark { at });
    }
    drawn.extend(pair(COUNT_BOX, (kinds.count() as i32, CELLS), COUNT_ENDS));
    drawn.extend(pair(PAGE_BOX, (list.page + 1, PAGES), PAGE_ENDS));
    drawn.push(Drawn::Sprite {
        number: PAGE_WORD,
        at: PAGE_WORD_AT,
        clip: SCREEN,
    });
    // The heading lies in a slot in front of everything else.
    drawn.extend([
        Drawn::Frame {
            rect: HEADING,
            mode: 0,
            time: 0,
        },
        Drawn::Listed {
            what: Listed::Sprite(sprite::CAPTURED),
            room: HEADING.inside(HEADING_MARGIN),
            flags: HEADING_FLAGS,
            dim: false,
            clip: SCREEN,
        },
    ]);
    drawn
}

/// The name of a species as a cell shows it, from the loaded executable: what 22
/// bytes of its text hold, in capitals.
pub fn kind_name(executable: &[u8], base: u32, species: u32) -> Option<Vec<u16>> {
    let image = Image {
        data: executable,
        base,
    };
    let entry = crate::rooms::step(SPECIES_NAMES, species as usize, 4).ok()?;
    let offset = image.u32(entry).ok()?.checked_sub(base)? as usize;
    let text = executable.get(offset..)?;
    let units = crate::text::units(&text[..text.len().min(NAME_BYTES)]);
    Some(units.into_iter().map(in_capitals).collect())
}

/// The text of a cell's mark from the menu's file, whose first byte is at `base`.
pub fn mark_text(file: &[u8], base: u32) -> Option<Vec<u16>> {
    let offset = MARK_AT.checked_sub(base)? as usize;
    file.get(offset..).map(crate::text::units)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pad::Repeat;

    fn pad(held: u16) -> Pad {
        let mut pad = Pad::default();
        pad.read(held, Repeat::of(2), 1);
        pad
    }

    #[test]
    fn the_cells_are_171_and_five_species_share_one() {
        let cells: Vec<_> = cells().collect();
        assert_eq!(cells.len(), 175);
        assert_eq!(cells.last(), Some(&(CELLS - 1, 192)));
        assert_eq!(cells[..3], [(0, 0), (1, 1), (2, 2)]);
        // 32 and 33 have none; 34 and 35 follow 31; 36 to 40 are cell 34; 57 is next.
        let of = |species| {
            cells
                .iter()
                .find(|(_, s)| *s == species)
                .map(|(cell, _)| *cell)
        };
        assert_eq!(
            (of(31), of(32), of(33), of(34), of(35)),
            (Some(31), None, None, Some(32), Some(33))
        );
        assert!((36..=40).all(|species| of(species) == Some(34)));
        assert_eq!(
            (of(41), of(56), of(57), of(193)),
            (None, None, Some(35), None)
        );
        // The pages: 64, 64 and 43 cells.
        let on = |page: i32| {
            let mut of_page: Vec<_> = (cells.iter())
                .filter(|(cell, _)| cell / ON_A_PAGE == page)
                .map(|(cell, _)| *cell)
                .collect();
            of_page.dedup();
            of_page.len()
        };
        assert_eq!([on(0), on(1), on(2)], [64, 64, 43]);
    }

    #[test]
    fn the_bits_count_every_species() {
        let mut kinds = Kinds::default();
        assert_eq!((kinds.count(), kinds.holds(0)), (0, false));
        for species in [0, 36, 37, 192, 223, 224, 500] {
            kinds.mark(species);
        }
        assert_eq!(kinds.count(), 5);
        assert!(kinds.holds(36) && kinds.holds(37) && kinds.holds(223));
        assert!(!kinds.holds(38) && !kinds.holds(224));
        assert_eq!(kinds.0[1], 0x30);
    }

    #[test]
    fn the_pages_go_round_and_a_cancel_ends_the_list() {
        let mut list = KindList::default();
        let turned = list.pass(&pad(pad::LEFT));
        assert_eq!(
            (list.page, turned.sounds.as_slice(), turned.ends),
            (2, &[SOUND_CURSOR][..], false)
        );
        let turned = list.pass(&pad(pad::RIGHT));
        assert_eq!((list.page, turned.ends), (0, false));
        list.pass(&pad(pad::RIGHT));
        assert_eq!(list.page, 1);
        // Both at once: two sounds and the page stays.
        let turned = list.pass(&pad(pad::RIGHT | pad::LEFT));
        assert_eq!((list.page, turned.sounds.len()), (1, 2));
        // Nothing pressed: nothing.
        assert_eq!(list.pass(&pad(0)), Turned::default());
        // A cancel comes first and leaves the page alone.
        let turned = list.pass(&pad(pad::TRIANGLE | pad::RIGHT));
        assert_eq!(
            turned,
            Turned {
                sounds: vec![SOUND_CANCEL],
                ends: true
            }
        );
        assert_eq!(list.page, 1);
    }

    #[test]
    fn a_page_draws_its_cells_and_a_name_where_the_kind_is_caught() {
        let mut kinds = Kinds::default();
        kinds.mark(1);
        kinds.mark(38);
        kinds.mark(150);
        let list = KindList::default();
        let page = drawn(&list, &kinds, true, 7);
        // Underneath: the right sign, the left sign, the frame round the cells.
        assert_eq!(
            page[0],
            Drawn::Sign {
                turns: 0,
                at: (312, 122),
                time: 7
            }
        );
        assert_eq!(
            page[1],
            Drawn::Sign {
                turns: 2,
                at: (8, 122),
                time: 7
            }
        );
        assert_eq!(
            page[2],
            Drawn::Frame {
                rect: CELLS_FRAME,
                mode: 0,
                time: 0
            }
        );
        let marks: Vec<_> = (page.iter())
            .filter_map(|drawn| match drawn {
                Drawn::KindMark { at } => Some(*at),
                _ => None,
            })
            .collect();
        // 64 cells, four of them a second to a fifth time for the shared one.
        assert_eq!(marks.len(), 68);
        assert_eq!(marks.last(), Some(&(18, 43)));
        assert_eq!(marks.first(), Some(&(18 + 3 * 72, 43 + 15 * 10)));
        let names: Vec<_> = (page.iter())
            .filter_map(|drawn| match drawn {
                Drawn::KindName { species, at } => Some((*species, *at)),
                _ => None,
            })
            .collect();
        // Species 38 in cell 34 (column 2, row 8), species 1 in cell 1.
        assert_eq!(
            names,
            [(38, (18 + 144 + 8, 43 + 80)), (1, (18 + 72 + 8, 43))]
        );
        // The count is 3 of 171, the page 1 of 3.
        let numbers: Vec<_> = (page.iter())
            .filter_map(|drawn| match drawn {
                Drawn::Number {
                    value,
                    ends,
                    small: true,
                    ..
                } => Some((*value, *ends)),
                _ => None,
            })
            .collect();
        assert_eq!(
            numbers,
            [
                (171, (292, 22)),
                (3, (264, 22)),
                (3, (81, 22)),
                (1, (66, 22))
            ]
        );
        // On top: the heading's frame and its sprite, 14 and 2 inside.
        let top = &page[page.len() - 2..];
        assert_eq!(
            top[0],
            Drawn::Frame {
                rect: HEADING,
                mode: 0,
                time: 0
            }
        );
        let Drawn::Listed {
            what, room, flags, ..
        } = top[1]
        else {
            panic!("the heading's content");
        };
        assert_eq!(
            (what, room, flags),
            (Listed::Sprite(111), rect(130, 18, 60, 14), 0x10)
        );
        assert_eq!(menu::placed(flags, room, 56), (130, 18));

        // The third page has 43 cells and the name of species 150 in its first.
        let list = KindList { page: 2 };
        let page = drawn(&list, &kinds, true, 0);
        let marks = page
            .iter()
            .filter(|drawn| matches!(drawn, Drawn::KindMark { .. }));
        assert_eq!(marks.count(), 43);
        assert!(page.contains(&Drawn::KindName {
            species: 150,
            at: (26, 43)
        }));
        // The pass of the cancel draws the signs and the frame alone.
        assert_eq!(drawn(&list, &kinds, false, 0).len(), 3);
    }
}
