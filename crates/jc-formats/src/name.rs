
use crate::menu::{self, Menu, Rect, Style, cursor};
use crate::pad::{self, Pad};
use crate::rooms::{self, Image};
use crate::text::{self, Font, Glyph};
use crate::title::{self, Placed};

/// The archive file of the screen's code and records.
pub const OVERLAY: usize = 49;
/// The archive file of the picture behind the screen, a texture set of three images.
pub const PICTURES: usize = 57;
/// The archive file of the menus' sprite pool, a texture set: the label of the name's
/// box, the three commands and the signs of the legend are images of it.
pub const SPRITES: usize = 59;

/// Passes of the screen's task in a second, as for the title that waits for it.
pub const PASSES_A_SECOND: u32 = title::PASSES_A_SECOND;

/// The letters a name can have.
pub const LETTERS: usize = 10;

/// The name a new game starts the screen with: a text of the title's file.
pub const DEFAULT_NAME: (usize, u32) = (title::OVERLAY, 0x800A_2AEC);

/// The two halves of the picture behind the screen, drawn by the title's task.
pub const BACKGROUND: [Placed; 2] = [
    Placed {
        image: 0,
        x: 0,
        y: 0,
    },
    Placed {
        image: 1,
        x: 160,
        y: 0,
    },
];

/// The sounds of the screen.
pub const SOUND_CANCEL: u8 = 125;
pub const SOUND_CONFIRM: u8 = 126;
pub const SOUND_MOVE: u8 = 127;

/// Bits of the screen's mode word.
pub mod mode {
    /// Set by the screen when the name is taken.
    pub const TAKEN: u16 = 0x01;
    /// Cleared by any confirm or cancel; nothing of the screen sets it.
    pub const UNTOUCHED: u16 = 0x02;
    /// Select puts the name back as it came.
    pub const SELECT_RESTORES: u16 = 0x08;
    /// Every pass draws the wall of the menus first, and at the end the screen is held
    /// and the caller's file is told: the name entry of file 43, on a creature's name.
    pub const OVER_THE_WALL: u16 = 0x10;
}

/// Where the cursor is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Area {
    /// The grid of letters.
    Grid = 0,
    /// The menu of the grid's pages.
    Pages = 1,
    /// The menu of the three commands.
    Commands = 2,
}

/// The lines of the commands' menu.
pub const CLEAR: i16 = 0;
pub const AS_IT_WAS: i16 = 1;
pub const DONE: i16 = 2;

/// The grid of an area: the flags of its cursor (`menu::cursor`) and its size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cells {
    pub flags: u16,
    pub columns: i16,
    pub rows: i16,
}

impl Area {
    pub const fn cells(self) -> Cells {
        match self {
            Area::Grid => Cells {
                flags: cursor::WRAPS_DOWN | cursor::LEAVES_RIGHT,
                columns: 12,
                rows: 6,
            },
            Area::Pages => Cells {
                flags: cursor::WRAPS_DOWN | cursor::LEAVES_RIGHT | cursor::LEAVES_LEFT,
                columns: 1,
                rows: 2,
            },
            Area::Commands => Cells {
                flags: cursor::WRAPS_DOWN | cursor::LEAVES_LEFT,
                columns: 1,
                rows: 3,
            },
        }
    }
}

impl Cells {
    /// One pass of an area's cursor with the pad's word of pressed or repeated buttons:
    /// the edge the cursor left by (1 right, 2 left, 3 up, 4 down, or 0) and whether the
    /// game plays the sound of a move. The first direction that runs into an edge ends
    /// the pass; an edge that wraps moves the cursor round, and an edge the flags hand
    /// on is the answer.
    pub fn step(self, at: &mut i16, repeated: u16) -> (u16, bool) {
        let before = *at;
        let Self {
            flags,
            columns,
            rows,
        } = self;
        let last_row = i32::from(columns) * (i32::from(rows) - 1);
        if *at < 0 {
            *at = 0;
        }
        let mut edge: u16 = 0;
        if repeated & pad::RIGHT != 0 {
            match at.checked_rem(columns) == Some(columns - 1) {
                true => edge = 1,
                false => *at = at.wrapping_add(1),
            }
        }
        if edge == 0 && repeated & pad::LEFT != 0 {
            match at.checked_rem(columns) == Some(0) {
                true => edge = 2,
                false => *at = at.wrapping_sub(1),
            }
        }
        if edge == 0 && repeated & pad::UP != 0 {
            match *at < columns {
                true => edge = 3,
                false => *at = at.wrapping_sub(columns),
            }
        }
        if edge == 0 && repeated & pad::DOWN != 0 {
            match last_row - 1 < i32::from(*at) {
                true => edge = 4,
                false => *at = at.wrapping_add(columns),
            }
        }
        let wrapped = match edge {
            1 if flags & cursor::WRAPS_ACROSS != 0 => i32::from(*at) - i32::from(columns - 1),
            2 if flags & cursor::WRAPS_ACROSS != 0 => i32::from(*at) + i32::from(columns - 1),
            3 if flags & cursor::WRAPS_DOWN != 0 => i32::from(*at) + last_row,
            4 if flags & cursor::WRAPS_DOWN != 0 => i32::from(*at) - last_row,
            _ => i32::from(*at),
        };
        *at = wrapped as i16;
        let leaves = [
            cursor::LEAVES_RIGHT,
            cursor::LEAVES_LEFT,
            cursor::LEAVES_UP,
            cursor::LEAVES_DOWN,
        ];
        match (edge.checked_sub(1)).and_then(|edge| leaves.get(usize::from(edge))) {
            Some(leave) if flags & leave != 0 => (edge, true),
            _ => (0, before != *at),
        }
    }
}

/// A page of the grid: its rows of glyph codes, as the row drawer leaves them.
pub type Page = Vec<Vec<u16>>;

/// What a pass did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Pass {
    /// The sounds the pass played, in their order.
    pub sounds: Vec<u8>,
    /// The name was taken: the screen is over.
    pub taken: bool,
}

/// The screen's record while it is up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub mode: u16,
    pub area: Area,
    /// The page of the grid that is shown.
    pub page: i16,
    pub cursor: i16,
    /// The grid's cursor as it was before the last pass that began in the grid.
    pub saved: i16,
    /// The name being written: a code for each letter and zeros behind them.
    codes: [u16; LETTERS + 1],
    /// The letters the name has.
    length: usize,
    /// The name as it came.
    original: [u16; LETTERS],
}

impl Entry {
    /// The screen as its start leaves it, on a name in codes of the font of the texts.
    pub fn new(name: &[u16], mode: u16) -> Self {
        let mut original = [0; LETTERS];
        for (place, &code) in original.iter_mut().zip(name) {
            *place = code;
        }
        let mut entry = Self {
            mode,
            area: Area::Grid,
            page: 0,
            cursor: 0,
            saved: 0,
            codes: [0; LETTERS + 1],
            length: 0,
            original,
        };
        entry.as_it_was();
        entry
    }

    /// The name being written, up to its first free place.
    pub fn name(&self) -> &[u16] {
        let end = (self.codes.iter()).position(|&code| code == 0);
        &self.codes[..end.unwrap_or(LETTERS)]
    }

    /// The letters the screen counts: the place of the next one.
    pub fn letters(&self) -> usize {
        self.length
    }

    fn as_it_was(&mut self) {
        self.codes[..LETTERS].copy_from_slice(&self.original);
        let end = (self.original.iter()).position(|&code| code == 0);
        self.length = end.unwrap_or(LETTERS);
    }

    fn clear(&mut self) {
        self.codes[..LETTERS].fill(0);
        self.length = 0;
    }

    /// Whether the name is refused: every place is free or holds code 145.
    pub fn is_blank(&self) -> bool {
        (self.codes[..LETTERS].iter()).all(|&code| matches!(code & 0xFF, 0 | 145))
    }

    /// The cursor's part of a pass: it moves in its area and goes over to the next one
    /// where an edge leads there.
    fn moved(&mut self, repeated: u16, sounds: &mut Vec<u8>) {
        if self.area == Area::Grid {
            self.saved = self.cursor;
        }
        let (answer, sounded) = self.area.cells().step(&mut self.cursor, repeated);
        if sounded {
            sounds.push(SOUND_MOVE);
        }
        match (self.area, answer) {
            (Area::Grid, 1) => {
                self.area = Area::Pages;
                self.cursor = self.page;
            }
            (Area::Pages, 1) => {
                self.area = Area::Commands;
                self.cursor = 0;
            }
            (Area::Pages, 2) => {
                // A place in the first column becomes the last one of its row.
                let columns = Area::Grid.cells().columns;
                self.cursor = match self.saved % columns == 0 {
                    true => self.saved.wrapping_add(columns - 1),
                    false => self.saved,
                };
                self.area = Area::Grid;
            }
            (Area::Commands, 2) => {
                self.area = Area::Pages;
                self.cursor = self.page;
            }
            _ => {}
        }
    }

    /// One pass of the screen's task with the pad's words and the pages of the grid.
    pub fn pass(&mut self, pad: &Pad, pages: &[Page]) -> Pass {
        let mut sounds = Vec::new();
        self.moved(pad.repeated, &mut sounds);
        // The drawing comes here. It makes the cursor of the pages' menu the page.
        if self.area == Area::Pages {
            self.page = self.cursor;
        }
        let confirmed = menu::confirmed(pad);
        let cancelled = menu::cancelled(pad);
        match self.area {
            Area::Grid => {
                if confirmed && self.length < LETTERS {
                    sounds.push(SOUND_CONFIRM);
                    let cells = Area::Grid.cells();
                    let (row, column) = (self.cursor / cells.columns, self.cursor % cells.columns);
                    let letter = usize::try_from(self.page).ok().and_then(|page| {
                        let row = pages.get(page)?.get(usize::try_from(row).ok()?)?;
                        row.get(usize::try_from(column).ok()?)
                    });
                    self.codes[self.length] = letter.map_or(0, |&code| code & 0xFF);
                    self.length += 1;
                }
                if cancelled {
                    sounds.push(SOUND_CANCEL);
                    if self.length != 0 {
                        self.codes[self.length] = 0;
                        self.length -= 1;
                        self.codes[self.length] = 0;
                    }
                }
            }
            Area::Pages => {
                // A cursor outside the menu leaves the routine before the cancel.
                let inside = (0..Area::Pages.cells().rows).contains(&self.cursor);
                if confirmed && inside {
                    self.page = self.cursor;
                }
                if cancelled && (inside || !confirmed) {
                    self.area = Area::Grid;
                    self.cursor = self.saved;
                }
            }
            Area::Commands => {
                if confirmed {
                    match self.cursor {
                        CLEAR => {
                            self.clear();
                            sounds.push(SOUND_CONFIRM);
                        }
                        AS_IT_WAS => {
                            self.as_it_was();
                            sounds.push(SOUND_CONFIRM);
                        }
                        DONE if !self.is_blank() => {
                            sounds.push(SOUND_CONFIRM);
                            self.mode |= mode::TAKEN;
                        }
                        _ => {}
                    }
                }
                if cancelled {
                    self.area = Area::Grid;
                    self.cursor = self.saved;
                }
            }
        }
        if pad.pressed & pad::START != 0 {
            self.area = Area::Commands;
            self.cursor = DONE;
        }
        if pad.pressed & pad::SELECT != 0 && self.mode & mode::SELECT_RESTORES != 0 {
            self.as_it_was();
        }
        let count = pages.len() as i16;
        if pad.pressed & pad::R2 != 0 {
            self.page = self.page.wrapping_add(1);
            if count - 1 < self.page {
                self.page = 0;
            }
        } else if pad.pressed & pad::R1 != 0 {
            self.page = self.page.wrapping_sub(1);
            if self.page < 0 {
                self.page = count - 1;
            }
        }
        let taken = self.mode & mode::TAKEN != 0;
        if !taken && (confirmed || cancelled) {
            self.mode &= !mode::UNTOUCHED;
        }
        Pass { sounds, taken }
    }

    /// The name as the game's state keeps it once it is taken: back through the table
    /// of the font of the texts to units, and on to the codes of the small font, one
    /// byte a letter.
    pub fn stored(&self, font: &Font, small: &Font) -> [u8; LETTERS] {
        stored(&font.units(self.name()), small)
    }
}

/// The name of the game's state from a text: the low byte of each unit's code in the
/// small font, and zeros from the text's end on.
pub fn stored(units: &[u16], small: &Font) -> [u8; LETTERS] {
    let codes = small.codes(units);
    let mut name = [0; LETTERS];
    for (place, code) in name.iter_mut().zip(codes) {
        if code == 0 {
            break;
        }
        *place = code as u8;
    }
    name
}

/// The way back: the units of the name of the game's state.
pub fn units(stored: &[u8; LETTERS], small: &Font) -> Vec<u16> {
    let codes: Vec<u16> = (stored.iter())
        .take_while(|&&code| code != 0)
        .map(|&code| u16::from(code))
        .collect();
    small.units(&codes)
}

/// The box of the name and what is in it.
pub const NAME_BOX: Rect = Rect {
    x: 69,
    y: 36,
    w: 184,
    h: 20,
};
/// The label in the box: a sprite of the pool and where it goes.
pub const LABEL: Placed = Placed {
    image: 17,
    x: 73,
    y: 40,
};
/// Where the name starts.
pub const NAME_AT: (i16, i16) = (116, 40);
/// The frame that marks the place of the next letter, without its x.
const NEXT: Rect = Rect {
    x: 0,
    y: 39,
    w: 8,
    h: 12,
};
/// The row of the dashes and how far apart they are.
const DASH_Y: i16 = 48;
const DASH_STEP: i16 = 8;

/// The frame around the grid.
pub const GRID: Rect = Rect {
    x: 10,
    y: 97,
    w: 194,
    h: 101,
};
/// The first letter's cell and the steps between letters and rows.
const GRID_AT: (i16, i16) = (13, 100);
const GRID_STEP: (i16, i16) = (16, 16);
/// The side of the frame around the letter under the cursor.
const GRID_CURSOR: i16 = 13;

/// The legend: where it starts, and the frame behind it.
pub const LEGEND_AT: (i16, i16) = (10, 214);
pub const LEGEND: Rect = Rect {
    x: 10,
    y: 214,
    w: 300,
    h: 14,
};
/// The height a sign of the legend stands in the middle of.
const LEGEND_SIGN: i32 = 14;

/// A frame of the screen that is not a menu's: its place, how it is drawn, and whether
/// it pulses with the menus' time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Framed {
    pub rect: Rect,
    pub style: Style,
    pub pulses: bool,
}

const fn framed(rect: Rect, kind: i16, mode: u8, pulses: bool) -> Framed {
    Framed {
        rect,
        style: Style {
            kind,
            mode,
            blend: 1,
        },
        pulses,
    }
}

/// The frame of the name's box, in the window colour `kind`.
pub const fn name_box(kind: i16) -> Framed {
    framed(NAME_BOX, kind, 0, false)
}

/// The frame of the grid.
pub const fn grid(kind: i16) -> Framed {
    framed(GRID, kind, 1, false)
}

/// The frame behind the legend.
pub const fn legend(kind: i16) -> Framed {
    framed(LEGEND, kind, 0, false)
}

/// The frame around the grid's letter under the cursor.
pub fn grid_cursor(at: i16, kind: i16) -> Framed {
    let columns = Area::Grid.cells().columns;
    let rect = Rect {
        x: (GRID_AT.0 - 1).wrapping_add((at % columns).wrapping_mul(GRID_STEP.0)),
        y: (GRID_AT.1 - 1).wrapping_add((at / columns).wrapping_mul(GRID_STEP.1)),
        w: GRID_CURSOR,
        h: GRID_CURSOR,
    };
    framed(rect, kind, 1, true)
}

/// The pulsing frame behind the name's last letter: the place of the next one.
pub fn next_letter(font: &Font, name: &[u16], kind: i16) -> Framed {
    let rect = Rect {
        x: NAME_AT.0.wrapping_add(font.length(name)),
        ..NEXT
    };
    framed(rect, kind, 1, true)
}

/// The places of the dashes behind a name: one for every letter that is still free.
pub fn dashes(font: &Font, name: &[u16], letters: usize) -> impl Iterator<Item = (i16, i16)> {
    let from = NAME_AT.0.wrapping_add(font.length(name));
    (0..LETTERS.saturating_sub(letters) as i16)
        .map(move |dash| (from.wrapping_add(dash.wrapping_mul(DASH_STEP)), DASH_Y))
}

/// The glyphs of a row of the grid as the screen's own row drawer places them: one cell
/// every 16 pixels, a glyph moved by half of what its width lacks of 12, and nothing
/// for a code the font does not draw.
pub fn row_glyphs(font: &Font, codes: &[u16], at: (i16, i16)) -> Vec<Glyph> {
    (codes.iter())
        .take_while(|&&code| code != 0)
        .zip(0i16..)
        .filter(|&(&code, _)| font.is_drawn(code))
        .map(|(&code, column)| {
            let moved = (i32::from(font.narrower(code)) - 2) / 2;
            let x = i32::from(at.0) + i32::from(column) * i32::from(GRID_STEP.0) + moved;
            Glyph {
                code,
                at: (x as i16, at.1),
                cell: font.cell_of(code),
            }
        })
        .collect()
}

/// The flag word and whether the time runs for the pages' menu and the commands' menu
/// while the cursor is in an area.
pub fn menu_looks(area: Area) -> [(u32, bool); 2] {
    match area {
        Area::Grid => [(4, false), (2, false)],
        Area::Pages => [(2, true), (2, false)],
        Area::Commands => [(4, false), (2, true)],
    }
}

/// A sign of the legend or its word, and where it goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegendPiece {
    Sign { sprite: usize, at: (i16, i16) },
    Word { units: Vec<u16>, at: (i16, i16) },
}

/// A row of a page: where its first cell goes and its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub at: (i16, i16),
    pub units: Vec<u16>,
}

/// The records of the screen in its file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tables {
    /// The rows of each page of the grid.
    pub pages: Vec<Vec<Row>>,
    /// The cursor's grid of each area as the file has it.
    pub cells: [Cells; 3],
    /// The menu of the pages and the menu of the commands.
    pub page_menu: Menu,
    pub command_menu: Menu,
    /// The pairs of the legend: a sprite of the pool and a word.
    pub legend: Vec<(usize, Vec<u16>)>,
    /// The text of a free place of the name.
    pub dash: Vec<u16>,
    image: Vec<u8>,
}

/// Seven pairs of a place and a text's address per page, the last one empty.
const PAGES: u32 = 0x800B_8368;
const PAGE_PAIRS: usize = 7;
/// Three records of 20 bytes, one per area; the first also holds the count of pages.
const AREAS: u32 = 0x800B_83D8;
const AREA_RECORD: usize = 20;
const COMMAND_MENU: u32 = 0x800B_84CC;
const PAGE_MENU: u32 = 0x800B_8560;
const LEGEND_LIST: u32 = 0x800B_8578;
const DASH: u32 = 0x800B_7018;
/// No list of the screen is this long.
const MOST: usize = 16;

impl Tables {
    /// Reads the records from the screen's file.
    pub fn from_overlay(file: &[u8]) -> Option<Self> {
        let base = rooms::load_address(OVERLAY);
        let image = Image { data: file, base };
        let text = |address: u32| -> Option<Vec<u16>> {
            let offset = address.checked_sub(base)? as usize;
            Some(text::units(file.get(offset..)?))
        };
        let count = usize::try_from(image.i16(AREAS + 4).ok()?).ok()?;
        if count > MOST {
            return None;
        }
        let mut pages = Vec::new();
        for page in 0..count {
            let mut rows = Vec::new();
            for pair in 0..PAGE_PAIRS {
                let at = rooms::step(PAGES, page * PAGE_PAIRS + pair, 8).ok()?;
                let address = image.u32(at + 4).ok()?;
                if address == 0 {
                    break;
                }
                rows.push(Row {
                    at: (image.i16(at).ok()?, image.i16(at + 2).ok()?),
                    units: text(address)?,
                });
            }
            pages.push(rows);
        }
        let mut cells = [Area::Grid.cells(); 3];
        for (area, cells) in cells.iter_mut().enumerate() {
            let record = rooms::step(AREAS, area, AREA_RECORD).ok()?;
            *cells = Cells {
                flags: image.u16(record + 2).ok()?,
                columns: image.i16(record + 6).ok()?,
                rows: image.i16(record + 8).ok()?,
            };
        }
        let mut legend = Vec::new();
        loop {
            let pair = rooms::step(LEGEND_LIST, legend.len(), 8).ok()?;
            let Ok(sprite) = usize::try_from(image.i32(pair).ok()?) else {
                break;
            };
            legend.push((sprite, text(image.u32(pair + 4).ok()?)?));
            if legend.len() > MOST {
                return None;
            }
        }
        Some(Self {
            pages,
            cells,
            page_menu: Menu::at(file, base, PAGE_MENU)?,
            command_menu: Menu::at(file, base, COMMAND_MENU)?,
            legend,
            dash: text(DASH)?,
            image: file.to_vec(),
        })
    }

    /// The text of a menu's piece that names one by its address.
    pub fn text(&self, address: u32) -> Option<Vec<u16>> {
        let offset = address.checked_sub(rooms::load_address(OVERLAY))? as usize;
        Some(text::units(self.image.get(offset..)?))
    }

    /// The pages in codes of a font, as the grid's row drawer leaves them and the
    /// buttons read them.
    pub fn letters(&self, font: &Font) -> Vec<Page> {
        (self.pages.iter())
            .map(|rows| rows.iter().map(|row| font.codes(&row.units)).collect())
            .collect()
    }

    /// The legend laid out by the executable's drawer: each sign in the middle of 14
    /// pixels of height, its word one pixel behind it and the next pair four behind the
    /// word. `sizes` gives width and height of a sprite of the pool.
    pub fn legend_pieces(
        &self,
        font: &Font,
        sizes: impl Fn(usize) -> Option<(i16, i16)>,
    ) -> Option<Vec<LegendPiece>> {
        legend_pieces(&self.legend, font, sizes)
    }
}

/// The pairs of any legend laid out by the executable's drawer (`0x8002B064`) at the
/// place it takes when none is given: each sign in the middle of 14 pixels of height,
/// its word one pixel behind it and the next pair four behind the word. `sizes` gives
/// width and height of a sprite of the pool.
pub fn legend_pieces(
    pairs: &[(usize, Vec<u16>)],
    font: &Font,
    sizes: impl Fn(usize) -> Option<(i16, i16)>,
) -> Option<Vec<LegendPiece>> {
    let mut x = LEGEND_AT.0.wrapping_add(2);
    let mut pieces = Vec::new();
    for (sprite, units) in pairs {
        let (wide, high) = sizes(*sprite)?;
        let down = (LEGEND_SIGN - i32::from(high)) / 2;
        pieces.push(LegendPiece::Sign {
            sprite: *sprite,
            at: (x, (i32::from(LEGEND_AT.1) + down) as i16),
        });
        x = x.wrapping_add(wide).wrapping_add(1);
        pieces.push(LegendPiece::Word {
            units: units.clone(),
            at: (x, LEGEND_AT.1),
        });
        x = (x.wrapping_add(font.length(&font.codes(units)))).wrapping_add(4);
    }
    Some(pieces)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pad::Repeat;

    /// A pad on which `held` went down in this pass.
    fn down(held: u16) -> Pad {
        let mut pad = Pad::default();
        pad.read(0, Repeat::of(1), 0);
        pad.read(held, Repeat::of(1), 1);
        pad
    }

    /// Two pages of six rows of twelve codes: on page 0 the code is 20 plus the place,
    /// on page 1 it is 145 everywhere.
    fn pages() -> Vec<Page> {
        let first = (0..6)
            .map(|row| (0..12).map(|column| 20 + row * 12 + column).collect())
            .collect();
        vec![first, vec![vec![145; 12]; 6]]
    }

    fn press(entry: &mut Entry, buttons: &[u16]) -> Vec<Pass> {
        let pages = pages();
        (buttons.iter())
            .map(|&held| entry.pass(&down(held), &pages))
            .collect()
    }

    #[test]
    fn a_cursor_stops_at_the_first_edge_and_wraps_or_leaves() {
        let grid = Area::Grid.cells();
        let mut at = 11;
        // The last column hands the step on, and the sound of a move plays.
        assert_eq!(grid.step(&mut at, pad::RIGHT), (1, true));
        assert_eq!(at, 11);
        // Right and down in one pass: the edge ends the pass before the step down.
        assert_eq!(grid.step(&mut at, pad::RIGHT | pad::DOWN), (1, true));
        assert_eq!(at, 11);
        // The first column is an edge that does nothing.
        at = 12;
        assert_eq!(grid.step(&mut at, pad::LEFT), (0, false));
        assert_eq!(at, 12);
        // Up and down go round a column.
        at = 3;
        assert_eq!(grid.step(&mut at, pad::UP), (0, true));
        assert_eq!(at, 63);
        assert_eq!(grid.step(&mut at, pad::DOWN), (0, true));
        assert_eq!(at, 3);
        assert_eq!(grid.step(&mut at, pad::DOWN | pad::LEFT), (0, true));
        assert_eq!(at, 14);
        // A negative cursor is put on the first place, which counts as a move.
        at = -4;
        assert_eq!(grid.step(&mut at, 0), (0, true));
        assert_eq!(at, 0);
        let commands = Area::Commands.cells();
        at = 2;
        assert_eq!(commands.step(&mut at, pad::DOWN), (0, true));
        assert_eq!(at, 0);
        assert_eq!(commands.step(&mut at, pad::LEFT), (2, true));
        assert_eq!(commands.step(&mut at, pad::RIGHT), (0, false));
    }

    #[test]
    fn the_cursor_goes_from_the_grid_to_the_menus_and_back() {
        let mut entry = Entry::new(&[], 0);
        entry.cursor = 23;
        entry.page = 1;
        let passes = press(&mut entry, &[pad::RIGHT]);
        assert_eq!(passes[0].sounds, [SOUND_MOVE]);
        assert_eq!(
            (entry.area, entry.cursor, entry.saved),
            (Area::Pages, 1, 23)
        );
        // In the pages' menu the page follows the cursor at once.
        press(&mut entry, &[pad::UP]);
        assert_eq!((entry.cursor, entry.page), (0, 0));
        press(&mut entry, &[pad::RIGHT]);
        assert_eq!((entry.area, entry.cursor), (Area::Commands, 0));
        press(&mut entry, &[pad::DOWN, pad::LEFT]);
        assert_eq!((entry.area, entry.cursor), (Area::Pages, 0));
        press(&mut entry, &[pad::LEFT]);
        assert_eq!((entry.area, entry.cursor), (Area::Grid, 23));
        // Start takes the cursor to "done" from anywhere; left from the pages then
        // leads to the last column of the row the cursor was in.
        entry.cursor = 24;
        press(&mut entry, &[pad::START]);
        assert_eq!(
            (entry.area, entry.cursor, entry.saved),
            (Area::Commands, DONE, 24)
        );
        press(&mut entry, &[pad::LEFT, pad::LEFT]);
        assert_eq!((entry.area, entry.cursor), (Area::Grid, 35));
        // A cancel in a menu puts the cursor back where it was, first column or not.
        entry.cursor = 24;
        press(&mut entry, &[pad::START, pad::TRIANGLE]);
        assert_eq!((entry.area, entry.cursor), (Area::Grid, 24));
    }

    #[test]
    fn the_cross_adds_a_letter_and_the_triangle_takes_one_away() {
        let mut entry = Entry::new(&[], 0);
        entry.cursor = 14;
        let passes = press(&mut entry, &[pad::CROSS, pad::RIGHT, pad::CROSS]);
        assert_eq!(passes[0].sounds, [SOUND_CONFIRM]);
        assert_eq!(passes[1].sounds, [SOUND_MOVE]);
        assert_eq!(entry.name(), [34, 35]);
        let passes = press(&mut entry, &[pad::TRIANGLE, pad::L2, pad::TRIANGLE]);
        // The sound of a cancel also plays when there is no letter to take.
        assert!(passes.iter().all(|pass| pass.sounds == [SOUND_CANCEL]));
        assert_eq!((entry.name(), entry.letters()), (&[][..], 0));
        // Ten letters and no more: the eleventh confirm does nothing and is silent.
        let passes = press(&mut entry, &[pad::CROSS; 11]);
        assert_eq!(entry.name(), [35; 10]);
        assert_eq!(passes[9].sounds, [SOUND_CONFIRM]);
        assert!(passes[10].sounds.is_empty());
        // Cross and triangle in one pass: the letter is added and taken away again.
        let mut entry = Entry::new(&[7], 0);
        let passes = press(&mut entry, &[pad::CROSS | pad::TRIANGLE]);
        assert_eq!(passes[0].sounds, [SOUND_CONFIRM, SOUND_CANCEL]);
        assert_eq!(entry.name(), [7]);
    }

    #[test]
    fn the_shoulder_buttons_turn_the_page() {
        let mut entry = Entry::new(&[], 0);
        let passes = press(&mut entry, &[pad::R2]);
        assert_eq!((entry.page, &passes[0].sounds[..]), (1, &[][..]));
        press(&mut entry, &[pad::CROSS, pad::R2]);
        assert_eq!((entry.page, entry.name()), (0, &[145][..]));
        press(&mut entry, &[pad::R1]);
        assert_eq!(entry.page, 1);
        press(&mut entry, &[pad::R1 | pad::R2]);
        assert_eq!(entry.page, 0);
        // In the pages' menu the page is the cursor again in the next pass.
        entry.cursor = 11;
        press(&mut entry, &[pad::RIGHT, pad::R2]);
        assert_eq!((entry.area, entry.cursor, entry.page), (Area::Pages, 0, 1));
        press(&mut entry, &[0]);
        assert_eq!(entry.page, 0);
    }

    #[test]
    fn the_commands_clear_put_back_and_take_the_name() {
        let mut entry = Entry::new(&[5, 6, 7], mode::UNTOUCHED);
        assert_eq!((entry.name(), entry.letters()), (&[5, 6, 7][..], 3));
        entry.cursor = 2;
        press(&mut entry, &[pad::CROSS, pad::START, pad::UP, pad::UP]);
        assert_eq!((entry.area, entry.cursor), (Area::Commands, CLEAR));
        assert_eq!(entry.mode, 0);
        let passes = press(&mut entry, &[pad::CROSS]);
        assert_eq!(passes[0].sounds, [SOUND_CONFIRM]);
        assert!(entry.name().is_empty() && entry.is_blank());
        // A blank name is not taken, and nothing sounds.
        let passes = press(&mut entry, &[pad::UP, pad::L1]);
        assert_eq!(entry.cursor, DONE);
        assert_eq!(passes[1], Pass::default());
        let passes = press(&mut entry, &[pad::UP, pad::CROSS]);
        assert_eq!(passes[1].sounds, [SOUND_CONFIRM]);
        assert_eq!(entry.name(), [5, 6, 7]);
        let passes = press(&mut entry, &[pad::DOWN, pad::CROSS]);
        assert_eq!(
            passes[1],
            Pass {
                sounds: vec![SOUND_CONFIRM],
                taken: true
            }
        );
        assert_eq!(entry.mode & mode::TAKEN, mode::TAKEN);
        // A name of nothing but code 145 is blank too.
        let mut entry = Entry::new(&[145, 145], 0);
        assert!(entry.is_blank());
        let passes = press(&mut entry, &[pad::START, pad::CROSS]);
        assert!(!passes[1].taken);
        // Select puts the name back only where the mode says so.
        let mut entry = Entry::new(&[9], mode::SELECT_RESTORES);
        press(&mut entry, &[pad::TRIANGLE, pad::SELECT]);
        assert_eq!(entry.name(), [9]);
        let mut entry = Entry::new(&[9], 0);
        press(&mut entry, &[pad::TRIANGLE, pad::SELECT]);
        assert!(entry.name().is_empty());
    }

    /// A font of 14-pixel cells, ten to a row, whose table is the units 0x9000 on and
    /// whose first four codes are 4, the next four 2 pixels narrower than the cell.
    fn font(entries: u16) -> Font {
        let mut limits = [0; 12];
        limits[1] = 8;
        limits[3] = 4;
        Font {
            image: 0,
            columns: 10,
            cell: (14, 14),
            palette: (0, 0),
            page: 0,
            last: u32::from(entries),
            table: (0..entries).map(|entry| 0x9000 + entry).collect(),
            limits,
        }
    }

    #[test]
    fn the_frames_and_glyphs_stand_where_the_drawer_puts_them() {
        let font = font(20);
        assert_eq!(
            grid_cursor(14, 3).rect,
            Rect {
                x: 44,
                y: 115,
                w: 13,
                h: 13
            }
        );
        assert!(grid_cursor(0, 3).pulses && !grid(3).pulses);
        assert_eq!((grid(3).style.mode, name_box(3).style.mode), (1, 0));
        // Codes 1 and 9: widths 10 and 14.
        let name = [1, 9];
        let next = next_letter(&font, &name, 0);
        assert_eq!((next.rect.x, next.rect.y, next.rect.w), (140, 39, 8));
        let places: Vec<(i16, i16)> = dashes(&font, &name, 2).collect();
        assert_eq!(places.len(), 8);
        assert_eq!((places[0], places[7]), ((140, 48), (196, 48)));
        assert_eq!(dashes(&font, &name, 10).count(), 0);
        // A row: a glyph 4 narrower moves right by 1, one 2 narrower stays, a full one
        // moves left by 1; a code the font does not draw leaves its cell empty, and the
        // row ends at a zero.
        let glyphs = row_glyphs(&font, &[1, 5, 30, 9, 0, 2], (13, 100));
        let places: Vec<(u16, i16)> = glyphs
            .iter()
            .map(|glyph| (glyph.code, glyph.at.0))
            .collect();
        assert_eq!(places, [(1, 14), (5, 29), (9, 60)]);
        assert_eq!(glyphs[1].cell, Some((56, 0)));
    }

    #[test]
    fn a_name_is_stored_in_codes_of_the_small_font() {
        // The font of the texts knows 20 units, the small font the first 12.
        let (font, small) = (font(20), font(12));
        let entry = Entry::new(&[3, 1, 12], 0);
        let name = entry.stored(&font, &small);
        assert_eq!(name, [3, 1, 12, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(units(&name, &small), [0x9002, 0x9000, 0x900B]);
        assert_eq!(font.codes(&units(&name, &small)), [3, 1, 12]);
        // A text ends the name: nothing behind its end is stored.
        assert_eq!(stored(&[0x9001, 0x9004], &small)[..3], [2, 5, 0]);
    }
}
