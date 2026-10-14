
use crate::rooms::{self, Image};

/// The archive file with the fonts' images, a texture set.
pub const FONT_SET: usize = 9;
/// The font of the texts, and the small font of capitals.
pub const TEXT_FONT: usize = 2;
pub const SMALL_FONT: usize = 1;

/// Three font records.
const FONTS: u32 = 0x8008_6464;
const FONT_RECORD: usize = 24;
const FONT_COUNT: usize = 3;
/// Per font: the address of the code table, the last code that is drawn, the entries.
const TABLES: u32 = 0x8008_67B8;
const TABLE_RECORD: usize = 12;
/// Twelve limits per font, from font 1 on.
const LIMITS: u32 = 0x8008_67F4;
const LIMIT_COUNT: usize = 12;
/// No code table of the disc comes near this many entries.
const MOST_ENTRIES: u32 = 0x1000;

/// A text whose first unit is below this is taken as changed into codes already.
const CHANGED_BELOW: u16 = 8321;

/// The units of a text: two bytes each, up to a unit whose first byte is 0.
pub fn units(bytes: &[u8]) -> Vec<u16> {
    (bytes.as_chunks::<2>().0.iter())
        .take_while(|unit| unit[0] != 0)
        .map(|unit| u16::from_le_bytes(*unit))
        .collect()
}

/// One font of the game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Font {
    /// The font's image in the texture set of file [`FONT_SET`].
    pub image: usize,
    /// Glyphs to a row of the image.
    pub columns: u16,
    /// Width and height of a glyph's cell.
    pub cell: (u16, u16),
    /// The place of the palette in video memory. A row further down is another colour.
    pub palette: (u16, u16),
    /// Glyphs to a page, or 0.
    pub page: u32,
    /// The last code that is drawn.
    pub last: u32,
    /// The units of a text in the order of their codes: code 1 first.
    pub table: Vec<u16>,
    /// Up to which code a glyph is 1, 2, ... 12 pixels narrower than the cell.
    pub limits: [u32; LIMIT_COUNT],
}

/// One unit of a text as the drawer places it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glyph {
    pub code: u16,
    /// The upper left corner of the cell on screen.
    pub at: (i16, i16),
    /// The upper left corner of the cell in the font's image, or `None` for a code that
    /// draws nothing.
    pub cell: Option<(u16, u16)>,
}

impl Font {
    /// A text changed into the font's glyph codes: a unit of the table becomes its place
    /// in it counted from 1, any other unit stays. A text that is changed already is left
    /// alone.
    pub fn codes(&self, units: &[u16]) -> Vec<u16> {
        if units.first().is_none_or(|&first| first < CHANGED_BELOW) {
            return units.to_vec();
        }
        (units.iter())
            .map(
                |unit| match self.table.iter().position(|entry| entry == unit) {
                    Some(place) => (place + 1) as u16,
                    None => *unit,
                },
            )
            .collect()
    }

    /// The way back: a code becomes the unit of its place in the table, anything that is
    /// not a code of the table stays.
    pub fn units(&self, codes: &[u16]) -> Vec<u16> {
        (codes.iter())
            .map(|&code| {
                let entry = usize::from(code).checked_sub(1);
                match entry.and_then(|entry| self.table.get(entry)) {
                    Some(&unit) if code < CHANGED_BELOW => unit,
                    _ => code,
                }
            })
            .collect()
    }

    /// How much narrower than its cell a code's glyph is drawn.
    pub fn narrower(&self, code: u16) -> u16 {
        self.trim(code)
    }

    /// Whether the drawer puts a sprite for the code.
    pub fn is_drawn(&self, code: u16) -> bool {
        u32::from(code) <= self.last
    }

    /// How much narrower than the cell the glyph of a code is.
    fn trim(&self, code: u16) -> u16 {
        let before = u32::from(code).wrapping_sub(1);
        (1..=LIMIT_COUNT)
            .rev()
            .find(|&trim| before < self.limits[trim - 1])
            .unwrap_or(0) as u16
    }

    /// The width of a code's glyph.
    pub fn width(&self, code: u16) -> u16 {
        self.cell.0.wrapping_sub(self.trim(code))
    }

    /// How far the drawer moves right behind a code: the glyph's width, or the whole cell
    /// for a code that draws nothing.
    pub fn advance(&self, code: u16) -> u16 {
        match self.is_drawn(code) {
            true => self.width(code),
            false => self.cell.0,
        }
    }

    /// The upper left corner of a code's cell in the font's image.
    pub fn cell_of(&self, code: u16) -> Option<(u16, u16)> {
        if !self.is_drawn(code) || self.columns == 0 {
            return None;
        }
        let number = u32::from(code).wrapping_sub(1);
        let rest = number.checked_rem(self.page).unwrap_or(number);
        let (row, column) = (
            rest / u32::from(self.columns),
            rest % u32::from(self.columns),
        );
        let x = column.checked_mul(u32::from(self.cell.0))?;
        let y = row.checked_mul(u32::from(self.cell.1))?;
        Some((u16::try_from(x).ok()?, u16::try_from(y).ok()?))
    }

    /// The glyphs of a text of codes whose first cell has its upper left corner at `at`.
    pub fn glyphs(&self, codes: &[u16], at: (i16, i16)) -> Vec<Glyph> {
        let mut x = at.0;
        (codes.iter())
            .map(|&code| {
                let glyph = Glyph {
                    code,
                    at: (x, at.1),
                    cell: self.cell_of(code),
                };
                x = x.wrapping_add(self.advance(code) as i16);
                glyph
            })
            .collect()
    }

    /// How far right of its start the drawer ends behind a text of codes.
    pub fn length(&self, codes: &[u16]) -> i16 {
        (codes.iter()).fold(0, |x: i16, &code| x.wrapping_add(self.advance(code) as i16))
    }
}

/// The fonts of the executable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fonts {
    fonts: Vec<Font>,
}

impl Fonts {
    /// Reads the font records, the code tables and the limits from the loaded executable.
    pub fn from_executable(image: &[u8], base: u32) -> Option<Self> {
        let image = Image { data: image, base };
        let fonts = (0..FONT_COUNT)
            .map(|number| {
                let record = rooms::step(FONTS, number, FONT_RECORD).ok()?;
                let table = rooms::step(TABLES, number, TABLE_RECORD).ok()?;
                let entries = image.u32(table + 8).ok()?;
                let codes = image.u32(table).ok()?;
                if entries > MOST_ENTRIES {
                    return None;
                }
                let mut limits = [0; LIMIT_COUNT];
                // Font 0 has neither a table nor limits.
                if let Some(before) = number.checked_sub(1) {
                    let start = rooms::step(LIMITS, before, 4 * LIMIT_COUNT).ok()?;
                    for (index, limit) in limits.iter_mut().enumerate() {
                        *limit = image.u32(rooms::step(start, index, 4).ok()?).ok()?;
                    }
                }
                Some(Font {
                    image: usize::from(image.u16(record).ok()?),
                    columns: image.u16(record + 2).ok()?,
                    cell: (image.u16(record + 6).ok()?, image.u16(record + 8).ok()?),
                    palette: (image.u16(record + 10).ok()?, image.u16(record + 12).ok()?),
                    page: image.u32(record + 20).ok()?,
                    last: image.u32(table + 4).ok()?,
                    table: (0..entries as usize)
                        .map(|entry| image.u16(rooms::step(codes, entry, 2).ok()?).ok())
                        .collect::<Option<_>>()?,
                    limits,
                })
            })
            .collect::<Option<_>>()?;
        Some(Self { fonts })
    }

    pub fn get(&self, number: usize) -> Option<&Font> {
        self.fonts.get(number)
    }

    pub fn len(&self) -> usize {
        self.fonts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.fonts.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A font of 4 by 4 glyphs in cells of 10 by 8: codes 1 and 2 are 3 wide, 3 to 6 are
    /// 5 wide, the rest up to the last drawn code 12 is 9 wide.
    fn font() -> Font {
        let mut limits = [0; LIMIT_COUNT];
        limits[0] = 12;
        limits[4] = 6;
        limits[6] = 2;
        Font {
            image: 3,
            columns: 4,
            cell: (10, 8),
            palette: (768, 258),
            page: 16,
            last: 12,
            table: (0..14).map(|n| 0x8260 + n).collect(),
            limits,
        }
    }

    #[test]
    fn units_end_at_a_zero_first_byte() {
        assert_eq!(
            units(&[0x82, 0x60, 0x81, 0x49, 0, 0x33, 0x82, 0x61]),
            [0x6082, 0x4981]
        );
        // A zero second byte does not end a text, and an odd byte at the end is no unit.
        assert_eq!(units(&[5, 0, 7]), [5]);
        assert!(units(&[]).is_empty());
    }

    #[test]
    fn a_text_becomes_codes_once() {
        let font = font();
        let codes = font.codes(&[0x8262, 0x8260, 0x9999, 0x826D]);
        assert_eq!(codes, [3, 1, 0x9999, 14]);
        // Codes are below the mark: a second change leaves them alone.
        assert_eq!(font.codes(&codes), codes);
        assert!(font.codes(&[]).is_empty());
    }

    #[test]
    fn glyphs_are_sorted_by_width() {
        let font = font();
        let widths: Vec<u16> = (1..=12).map(|code| font.width(code)).collect();
        assert_eq!(widths, [3, 3, 5, 5, 5, 5, 9, 9, 9, 9, 9, 9]);
        // Behind the last drawn code the place moves by the whole cell.
        assert_eq!(font.advance(12), 9);
        assert_eq!(font.advance(13), 10);
        assert_eq!(font.advance(0x9999), 10);
    }

    #[test]
    fn a_text_is_placed_glyph_by_glyph() {
        let font = font();
        let glyphs = font.glyphs(&[6, 1, 13, 12], (16, 90));
        let at: Vec<_> = glyphs.iter().map(|glyph| glyph.at).collect();
        assert_eq!(at, [(16, 90), (21, 90), (24, 90), (34, 90)]);
        let cells: Vec<_> = glyphs.iter().map(|glyph| glyph.cell).collect();
        assert_eq!(cells, [Some((10, 8)), Some((0, 0)), None, Some((30, 16))]);
        assert_eq!(font.length(&[6, 1, 13, 12]), 27);
    }

    #[test]
    fn a_code_behind_a_page_starts_the_image_again() {
        let mut font = font();
        font.last = 40;
        assert_eq!(font.cell_of(17), Some((0, 0)));
        assert_eq!(font.cell_of(22), Some((10, 8)));
        // Without a page size the rows go on.
        font.page = 0;
        assert_eq!(font.cell_of(17), Some((0, 32)));
    }

    #[test]
    fn fonts_are_read_from_an_image() {
        let base = 0x8008_6000;
        let mut image = vec![0u8; 0x1000];
        let mut put = |address: u32, bytes: &[u8]| {
            let at = (address - base) as usize;
            image[at..at + bytes.len()].copy_from_slice(bytes);
        };
        let table = 0x8008_6900u32;
        let record = FONTS + FONT_RECORD as u32;
        for (offset, value) in [(0, 19u16), (2, 16), (6, 8), (8, 8), (10, 768), (12, 262)] {
            put(record + offset, &value.to_le_bytes());
        }
        put(record + 20, &112u32.to_le_bytes());
        let entry = TABLES + TABLE_RECORD as u32;
        for (offset, value) in [(0, table), (4, 2), (8, 3)] {
            put(entry + offset, &value.to_le_bytes());
        }
        put(LIMITS + 4 * 4, &2u32.to_le_bytes());
        for (index, unit) in [0x4981u16, 0x6082, 0x8182].into_iter().enumerate() {
            put(table + 2 * index as u32, &unit.to_le_bytes());
        }
        let fonts = Fonts::from_executable(&image, base).expect("fonts");
        assert_eq!(fonts.len(), 3);
        let font = fonts.get(1).expect("font 1");
        assert_eq!((font.image, font.columns, font.cell), (19, 16, (8, 8)));
        assert_eq!((font.palette, font.page, font.last), ((768, 262), 112, 2));
        assert_eq!(font.table, [0x4981, 0x6082, 0x8182]);
        assert_eq!(font.codes(&[0x6082, 0x8182, 0x4981]), [2, 3, 1]);
        assert_eq!((font.width(2), font.width(3)), (3, 8));
        assert!(fonts.get(0).expect("font 0").table.is_empty());
        // An image that ends before the tables holds no fonts.
        assert_eq!(Fonts::from_executable(&image[..0x500], base), None);
    }
}
