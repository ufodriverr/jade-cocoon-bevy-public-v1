
use crate::pad::{self, Pad};
use crate::rooms::Image;

/// The buttons that cancel in a menu.
pub const CANCEL: u16 = pad::TRIANGLE | pad::L2;

/// Whether the pad's words of a pass confirm: the cross or L1 went down while the other
/// of the two is neither held from before nor just let go.
pub fn confirmed(pad: &Pad) -> bool {
    if pad.pressed & pad::CROSS != 0 && pad.held & pad::L1 == 0 && pad.released & pad::L1 == 0 {
        return true;
    }
    if pad.pressed & pad::L1 == 0 {
        return false;
    }
    match pad.held & pad::CROSS != 0 {
        true => pad.pressed & pad::CROSS != 0,
        false => pad.released & pad::CROSS == 0,
    }
}

/// Whether the pad's words of a pass cancel.
pub fn cancelled(pad: &Pad) -> bool {
    pad.pressed & CANCEL != 0
}

/// Bits of a cursor's flag word.
pub mod cursor {
    /// Left of the first column is the last one, and the other way round.
    pub const WRAPS_ACROSS: u16 = 0x01;
    /// Above the first row is the last one, and the other way round.
    pub const WRAPS_DOWN: u16 = 0x02;
    /// A step over the right, left, upper or lower edge is answered with 1, 2, 3 or 4.
    pub const LEAVES_RIGHT: u16 = 0x04;
    pub const LEAVES_LEFT: u16 = 0x08;
    pub const LEAVES_UP: u16 = 0x10;
    pub const LEAVES_DOWN: u16 = 0x20;
    /// The lines scroll under the rows.
    pub const SCROLLS: u16 = 0x40;
}

/// The answers of a cursor whose lines moved under its rows by a row: up and down.
pub const SCROLLED_UP: u16 = 8;
pub const SCROLLED_DOWN: u16 = 7;

/// The cursor of a menu: a grid of columns and rows over a number of lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub columns: i16,
    pub rows: i16,
    pub lines: u16,
    pub flags: u16,
    /// The place of the cursor in the grid, row by row.
    pub at: i16,
    /// The first line shown.
    pub first: i16,
}

/// What a pass of the cursor did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Moved {
    /// 0, the edge the cursor left by (1 right, 2 left, 3 up, 4 down), or
    /// [`SCROLLED_DOWN`] or [`SCROLLED_UP`].
    pub answer: u16,
    /// The cursor is somewhere else than before: the game plays its sound for it.
    pub sounds: bool,
}

impl Cursor {
    /// One pass of the cursor with the pad's word of pressed or repeated buttons.
    pub fn step(&mut self, repeated: u16) -> Moved {
        self.step_with(repeated, 0)
    }

    /// One pass of the cursor with the pad's words of pressed or repeated and of held
    /// buttons: a cursor whose lines scroll turns a page with R2 and R1.
    pub fn step_with(&mut self, repeated: u16, held: u16) -> Moved {
        let before = (self.at, self.first);
        let answer = self.moved(repeated, held);
        let last = i32::from(self.lines) - 1;
        if answer == 0 && last < i32::from(self.at) {
            self.at = last as i16;
        }
        Moved {
            answer,
            sounds: before != (self.at, self.first),
        }
    }

    /// How many lines the rows show at a time.
    fn page(&self) -> i32 {
        i32::from(self.columns) * i32::from(self.rows)
    }

    /// Whether lines lie above the first row and below the last one: where a list shows
    /// its scroll signs.
    pub fn hides(&self) -> (bool, bool) {
        let below = self.page() + i32::from(self.first) < i32::from(self.lines);
        (self.first > 0, below)
    }

    fn moved(&mut self, repeated: u16, held: u16) -> u16 {
        const RIGHT_EDGE: u16 = 0x02;
        const LEFT_EDGE: u16 = 0x04;
        const UPPER_EDGE: u16 = 0x08;
        const LOWER_EDGE: u16 = 0x10;
        let (columns, rows) = (self.columns, self.rows);
        let mut edges = 0;
        if self.at < 0 {
            self.at = 0;
        }
        if repeated & pad::RIGHT != 0 {
            match self.at.checked_rem(columns) == Some(columns - 1) {
                true => edges = RIGHT_EDGE,
                false => self.at = self.at.wrapping_add(1),
            }
        }
        if repeated & pad::LEFT != 0 {
            match self.at.checked_rem(columns) == Some(0) {
                true => edges |= LEFT_EDGE,
                false => self.at = self.at.wrapping_sub(1),
            }
        }
        if repeated & pad::UP != 0 {
            match self.at < columns {
                true => edges |= UPPER_EDGE,
                false => self.at = self.at.wrapping_sub(columns),
            }
        }
        let last_row = i32::from(columns) * (i32::from(rows) - 1);
        if repeated & pad::DOWN != 0 {
            match last_row - 1 < i32::from(self.at) {
                true => edges |= LOWER_EDGE,
                false => self.at = self.at.wrapping_add(columns),
            }
        }
        // A step over the upper or lower edge moves the lines by a row while there are
        // lines that way.
        let spare = i32::from(self.lines) - self.page();
        if self.flags & cursor::SCROLLS != 0 {
            if edges & UPPER_EDGE != 0 {
                if self.first >= columns {
                    self.first = self.first.wrapping_sub(columns);
                    return SCROLLED_UP;
                }
            } else if edges & LOWER_EDGE != 0 && i32::from(self.first) < spare {
                self.first = self.first.wrapping_add(columns);
                return SCROLLED_DOWN;
            }
        }
        if self.flags & cursor::WRAPS_ACROSS != 0 {
            if edges & RIGHT_EDGE != 0 {
                self.at = self.at.wrapping_sub(columns - 1);
            }
            if edges & LEFT_EDGE != 0 {
                self.at = self.at.wrapping_add(columns - 1);
            }
        }
        if self.flags & cursor::WRAPS_DOWN != 0 {
            if edges & UPPER_EDGE != 0 {
                self.at = (i32::from(self.at) + last_row) as i16;
            }
            if edges & LOWER_EDGE != 0 {
                self.at = (i32::from(self.at) - last_row) as i16;
            }
        }
        let leaves = [
            (cursor::LEAVES_RIGHT, RIGHT_EDGE),
            (cursor::LEAVES_LEFT, LEFT_EDGE),
            (cursor::LEAVES_UP, UPPER_EDGE),
            (cursor::LEAVES_DOWN, LOWER_EDGE),
        ];
        let left = (leaves.iter().zip(1..))
            .find(|((flag, edge), _)| self.flags & flag != 0 && edges & edge != 0)
            .map(|(_, answer)| answer);
        if let Some(answer) = left {
            return answer;
        }
        if self.flags & cursor::SCROLLS != 0 {
            // R2 turns a page down, R1 one up unless R2 is held; the first line shown
            // is kept inside the lines either way.
            let mut first = i32::from(self.first);
            if repeated & pad::R2 != 0 {
                first += self.page();
            } else if repeated & pad::R1 != 0 && held & pad::R2 == 0 {
                first -= self.page();
            }
            self.first = first.min(spare).max(0) as i16;
        }
        0
    }
}

/// A rectangle of the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i16,
    pub y: i16,
    pub w: i16,
    pub h: i16,
}

impl Rect {
    /// The rectangle without a margin on every side: the room for a line's content.
    pub fn inside(self, margin: (i16, i16)) -> Self {
        Self {
            x: self.x.wrapping_add(margin.0),
            y: self.y.wrapping_add(margin.1),
            w: self.w.wrapping_sub(margin.0.wrapping_mul(2)),
            h: self.h.wrapping_sub(margin.1.wrapping_mul(2)),
        }
    }
}

/// Bits of the flag word of a line's piece.
pub mod piece {
    /// The last piece of a line: it is not drawn.
    pub const LAST: u16 = 0x01;
    /// Drawn at half the brightness.
    pub const DIM: u16 = 0x02;
    /// The content ends at the right edge of its room.
    pub const RIGHT: u16 = 0x04;
    /// The content lies in the middle of its room.
    pub const CENTRED: u16 = 0x08;
    /// No frame around the piece.
    pub const BARE: u16 = 0x20;
    /// The piece is left out, its frame is not.
    pub const EMPTY: u16 = 0x40;
}

/// The kinds of a piece's content that are in code.
pub mod kind {
    /// An image of a texture set: the value is the sprite's number, or -1 for none.
    pub const PICTURE: u16 = 1;
    /// A text in the small font, two pixels lower than a picture would be: the value is
    /// the text's address.
    pub const SMALL_TEXT: u16 = 2;
    /// A text in the font of the texts.
    pub const TEXT: u16 = 3;
}

/// One piece of a menu's line as the drawer reads it: 28 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece {
    pub flags: u16,
    pub kind: u16,
    pub value: u32,
    pub rect: Rect,
    pub margin: (i16, i16),
}

const PIECE: usize = 28;
/// No menu of the disc comes near this many lines, or a line this many pieces.
const MOST: usize = 64;

/// A menu's record of 24 bytes with its lines: each the pieces before the one that ends
/// the line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Menu {
    pub flags: u32,
    pub openness: i32,
    pub cursor: i32,
    pub slot: i32,
    pub lines: Vec<Vec<Piece>>,
}

impl Menu {
    /// Reads the record at an address of a memory image whose first byte is at `base`.
    pub fn at(image: &[u8], base: u32, address: u32) -> Option<Self> {
        let record = Image { data: image, base };
        Some(Self {
            flags: record.u32(address).ok()?,
            openness: record.i32(address + 8).ok()?,
            cursor: record.i32(address + 12).ok()?,
            slot: record.i32(address + 20).ok()?,
            lines: Self::lines_at(image, base, record.u32(address + 16).ok()?)?,
        })
    }

    /// The lines of a list of line addresses ended by 0, for a menu whose record is made
    /// by its caller and only names the list.
    pub fn lines_at(image: &[u8], base: u32, list: u32) -> Option<Vec<Vec<Piece>>> {
        let image = Image { data: image, base };
        let mut lines = Vec::new();
        loop {
            let line = image
                .u32(crate::rooms::step(list, lines.len(), 4).ok()?)
                .ok()?;
            if line == 0 {
                break;
            }
            let mut pieces = Vec::new();
            loop {
                let piece = crate::rooms::step(line, pieces.len(), PIECE).ok()?;
                let flags = image.u16(piece).ok()?;
                if flags & self::piece::LAST != 0 {
                    break;
                }
                pieces.push(Piece {
                    flags,
                    kind: image.u16(piece + 2).ok()?,
                    value: image.u32(piece + 4).ok()?,
                    rect: Rect {
                        x: image.i16(piece + 16).ok()?,
                        y: image.i16(piece + 18).ok()?,
                        w: image.i16(piece + 20).ok()?,
                        h: image.i16(piece + 22).ok()?,
                    },
                    margin: (image.i16(piece + 24).ok()?, image.i16(piece + 26).ok()?),
                });
                if pieces.len() > MOST {
                    return None;
                }
            }
            lines.push(pieces);
            if lines.len() > MOST {
                return None;
            }
        }
        Some(lines)
    }
}

/// The upper left corner of a content that is `width` wide in its room.
pub fn placed(flags: u16, room: Rect, width: i16) -> (i16, i16) {
    let spare = i32::from(room.w) - i32::from(width);
    let x = match (flags & piece::RIGHT != 0, flags & piece::CENTRED != 0) {
        (true, _) => i32::from(room.x) + spare,
        // The routine halves the 32-bit difference as an unsigned number and keeps 16
        // bits of the sum: a content wider than its room starts left of it, by half
        // of what is missing, rounded up.
        (false, true) => i32::from(room.x).wrapping_add(((spare as u32) >> 1) as i32),
        (false, false) => i32::from(room.x),
    };
    (x as i16, room.y)
}

/// The brightness a picture is drawn with, where 128 leaves its colours as they are.
pub fn brightness(flags: u16) -> u8 {
    match flags & piece::DIM != 0 {
        true => 64,
        false => 128,
    }
}

/// The time the frames of a menu pulse by: half the runner's passes in pictures of the
/// screen.
pub fn frame_time(passes: u32, game_frame: u32) -> u32 {
    passes.wrapping_mul(game_frame) >> 1
}

/// How a frame is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Style {
    /// One of the five colour sets.
    pub kind: i16,
    /// 0 the plain colour, 1 the colour that can pulse, 2 the marked colour.
    pub mode: u8,
    /// 0 solid, otherwise one more than the console's blend mode.
    pub blend: u8,
}

impl Style {
    /// The style of a line's frame from the flag word of its menu, and whether the frame
    /// pulses. `own_kind` is the kind the game's state names, for a menu that names none.
    pub fn of_line(flags: u32, on_cursor: bool, own_kind: i16) -> (Self, bool) {
        let blend = match flags {
            flags if flags & 0x08 != 0 => 0,
            flags if flags & 0x10 != 0 => 1,
            flags if flags & 0x20 != 0 => 2,
            flags if flags & 0x40 != 0 => 3,
            flags if flags & 0x80 != 0 => 4,
            _ => 1,
        };
        let (mode, pulses) = match flags {
            flags if flags & 0x01 != 0 => (0, false),
            flags if flags & 0x02 != 0 => (1, on_cursor),
            flags if flags & 0x04 != 0 && on_cursor => (2, false),
            _ => (0, false),
        };
        let kind = match flags {
            flags if flags & 0x0100 == 0 => own_kind,
            flags if flags & 0x2000 != 0 => 4,
            flags if flags & 0x1000 != 0 => 3,
            flags if flags & 0x0800 != 0 => 2,
            flags if flags & 0x0400 != 0 => 1,
            _ => 0,
        };
        (Self { kind, mode, blend }, pulses)
    }

    /// The console's blend mode of the frame, if it is blended with what lies under it.
    pub fn blend_mode(self) -> Option<u8> {
        self.blend.checked_sub(1)
    }
}

/// One of the five flat quads a frame is made of: the upper, left, lower and right edge
/// and the middle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameQuad {
    /// The corners in the console's order: two triangles, corners 0, 1, 2 and 1, 2, 3.
    pub corners: [(i16, i16); 4],
    pub colour: [u8; 3],
}

/// The width of a frame's edge.
pub const EDGE: i32 = 2;

const KINDS: usize = 5;
const MODES: usize = 3;
const QUADS: usize = 5;

/// The colour of a frame drawn without one.
const PLAIN_COLOUR: u32 = 0x8007_4874;
/// The share of the colour each quad gets, in hundredths.
const QUAD_SHARES: u32 = 0x8008_688C;
/// Three shares per kind that make the pulsing colour's first end.
const PULSE_SHARES: u32 = 0x8008_68A0;
/// Per kind, the address of the pulsing colour's other end.
const PULSE_ENDS: u32 = 0x8008_6940;
/// Per mode, the address of a table of colour addresses per kind.
const MODE_TABLES: u32 = 0x8008_6954;
/// Per quad, eight words that say where a corner's x and y lie in the rectangle.
const QUAD_CORNERS: u32 = 0x8008_6960;

/// The length of a pulse and of its way up, in units of `frame_time`.
const PULSE: (u32, u32) = (40, 10);

/// The colour of a pulse at a time: from the first colour to the second on the way up and
/// back in the rest of the pulse.
pub fn pulse(from: [u8; 3], to: [u8; 3], time: u32) -> [u8; 3] {
    let (length, up) = PULSE;
    let (at, of) = match time % length {
        at if at < up => (at, up),
        at => (length - at, length - up),
    };
    [0, 1, 2].map(|part| {
        let (from, to) = (i32::from(from[part]), i32::from(to[part]));
        (from + (to - from) * at as i32 / of as i32).clamp(0, 255) as u8
    })
}

pub mod sign {
    use crate::rooms::Image;

    /// Per window colour three image numbers of the font set, 16 bits each.
    const IMAGES: u32 = 0x8008_26C0;
    const FAMILIES: u32 = 3;
    /// The family of images a list of lines uses.
    pub const OF_LIST: u16 = 2;
    /// Quarter turns of the sign above a list and of the one below it.
    pub const ABOVE: u16 = 3;
    pub const BELOW: u16 = 1;
    /// The two brightnesses a sign pulses between, where 128 leaves its colours as they
    /// are.
    pub const DIM: [u8; 3] = [0x80; 3];
    pub const BRIGHT: [u8; 3] = [0xFF; 3];

    /// The image of the font set a sign is, from the tables of the loaded executable.
    pub fn image(executable: &[u8], base: u32, window_colour: i16, family: u16) -> Option<usize> {
        let image = Image {
            data: executable,
            base,
        };
        let index = u32::try_from(window_colour).ok()? * FAMILIES + u32::from(family);
        let number = image.i16(IMAGES + index * 2).ok()?;
        usize::try_from(number).ok()
    }

    /// Where a texel of the sign's image lies on the screen, from the sign's place: the
    /// image lies around the place and is turned about it by quarter turns.
    pub fn texel(turns: u16, size: (i16, i16), texel: (i16, i16)) -> (i16, i16) {
        let (x, y) = (1 - size.0 / 2 + texel.0, 1 - size.1 / 2 + texel.1);
        let (cos, sin) = [(1, 0), (0, 1), (-1, 0), (0, -1)][usize::from(turns & 3)];
        (x * cos - y * sin, x * sin + y * cos)
    }
}

/// The tables of the frame drawer in the executable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frames {
    plain: [u8; 3],
    quad_shares: [i32; QUADS],
    corners: [[u32; 8]; QUADS],
    colours: [[[u8; 3]; KINDS]; MODES],
    pulse_shares: [[i32; 3]; KINDS],
    pulse_ends: [[u8; 3]; KINDS],
}

fn share(colour: [u8; 3], shares: [i32; 3]) -> [u8; 3] {
    [0, 1, 2]
        .map(|part| (i32::from(colour[part]).wrapping_mul(shares[part]) / 100).clamp(0, 255) as u8)
}

impl Frames {
    /// Reads the tables from the loaded executable.
    pub fn from_executable(image: &[u8], base: u32) -> Option<Self> {
        let image = Image { data: image, base };
        let colour = |address: u32| -> Option<[u8; 3]> {
            let word = image.u32(address).ok()?.to_le_bytes();
            Some([word[0], word[1], word[2]])
        };
        let word = |start: u32, index: usize| image.u32(start + 4 * index as u32).ok();
        let mut frames = Self {
            plain: colour(PLAIN_COLOUR)?,
            quad_shares: [0; QUADS],
            corners: [[0; 8]; QUADS],
            colours: [[[0; 3]; KINDS]; MODES],
            pulse_shares: [[0; 3]; KINDS],
            pulse_ends: [[0; 3]; KINDS],
        };
        for quad in 0..QUADS {
            frames.quad_shares[quad] = word(QUAD_SHARES, quad)? as i32;
            for corner in 0..8 {
                frames.corners[quad][corner] = word(QUAD_CORNERS, quad * 8 + corner)?;
            }
        }
        for mode in 0..MODES {
            let table = word(MODE_TABLES, mode)?;
            for kind in 0..KINDS {
                frames.colours[mode][kind] = colour(word(table, kind)?)?;
            }
        }
        for kind in 0..KINDS {
            for part in 0..3 {
                frames.pulse_shares[kind][part] = word(PULSE_SHARES, kind * 3 + part)? as i32;
            }
            frames.pulse_ends[kind] = colour(word(PULSE_ENDS, kind)?)?;
        }
        Some(frames)
    }

    /// The colour of a frame at a time. A style outside the tables has none.
    pub fn colour(&self, style: Style, time: u32) -> Option<[u8; 3]> {
        let kind = usize::try_from(style.kind)
            .ok()
            .filter(|&kind| kind < KINDS)?;
        let colour = *self.colours.get(usize::from(style.mode))?.get(kind)?;
        if style.mode != 1 {
            return Some(colour);
        }
        let from = share(colour, self.pulse_shares[kind]);
        Some(pulse(from, self.pulse_ends[kind], time))
    }

    /// The five quads of a frame around a rectangle, in the order they are added to the
    /// ordering table. Without a colour all five have the plain one.
    pub fn quads(&self, rect: Rect, colour: Option<[u8; 3]>) -> [FrameQuad; QUADS] {
        let along = |rule: u32, start: i16, size: i16| -> i16 {
            let (start, size) = (i32::from(start), i32::from(size));
            (match rule {
                0 => start,
                1 => start + EDGE,
                2 => start - EDGE + size,
                3 => start + size,
                _ => -1,
            }) as i16
        };
        std::array::from_fn(|quad| {
            let rules = self.corners[quad];
            FrameQuad {
                corners: std::array::from_fn(|corner| {
                    (
                        along(rules[corner * 2], rect.x, rect.w),
                        along(rules[corner * 2 + 1], rect.y, rect.h),
                    )
                }),
                colour: match colour {
                    Some(colour) => share(colour, [self.quad_shares[quad]; 3]),
                    None => self.plain,
                },
            }
        })
    }
}

impl FrameQuad {
    /// Whether the quad covers the pixel: its two triangles with the rule that an edge
    /// on the left or on top belongs to a triangle and one on the right or below does
    /// not. (How the console's chip fills a triangle is not on the disc.)
    pub fn covers(&self, x: i16, y: i16) -> bool {
        let [a, b, c, d] = self.corners;
        triangle_covers([a, b, c], (x, y)) || triangle_covers([b, c, d], (x, y))
    }
}

fn triangle_covers(corners: [(i16, i16); 3], at: (i16, i16)) -> bool {
    let wide = |(x, y): (i16, i16)| (i64::from(x), i64::from(y));
    let [a, mut b, mut c] = corners.map(wide);
    let at = wide(at);
    let turn = (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
    if turn == 0 {
        return false;
    }
    if turn < 0 {
        std::mem::swap(&mut b, &mut c);
    }
    [(a, b), (b, c), (c, a)].iter().all(|&(from, to)| {
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let side = dx * (at.1 - from.1) - dy * (at.0 - from.0);
        side > 0 || (side == 0 && (dy < 0 || (dy == 0 && dx > 0)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pad::Repeat;

    /// The pad after two calls of its routine with these held words.
    fn pad_after(before: u16, held: u16) -> Pad {
        let mut pad = Pad::default();
        pad.read(before, Repeat::of(1), 0);
        pad.read(held, Repeat::of(1), 1);
        pad
    }

    #[test]
    fn the_cross_or_l1_confirms_when_the_other_is_not_in_the_way() {
        assert!(confirmed(&pad_after(0, pad::CROSS)));
        assert!(confirmed(&pad_after(0, pad::L1)));
        assert!(confirmed(&pad_after(0, pad::CROSS | pad::L1)));
        // The other one held from before, or just let go.
        assert!(!confirmed(&pad_after(pad::L1, pad::L1 | pad::CROSS)));
        assert!(!confirmed(&pad_after(pad::L1, pad::CROSS)));
        assert!(!confirmed(&pad_after(pad::CROSS, pad::CROSS | pad::L1)));
        assert!(!confirmed(&pad_after(pad::CROSS, pad::L1)));
        // A button that stays down does not confirm again.
        assert!(!confirmed(&pad_after(pad::CROSS, pad::CROSS)));
        assert!(!confirmed(&pad_after(0, pad::START)));
    }

    #[test]
    fn the_triangle_or_l2_cancels() {
        assert!(cancelled(&pad_after(0, pad::TRIANGLE)));
        assert!(cancelled(&pad_after(0, pad::L2)));
        assert!(!cancelled(&pad_after(pad::TRIANGLE, pad::TRIANGLE)));
        assert!(!cancelled(&pad_after(0, pad::CROSS)));
    }

    fn column(flags: u16, at: i16) -> Cursor {
        Cursor {
            columns: 1,
            rows: 4,
            lines: 4,
            flags,
            at,
            first: 0,
        }
    }

    #[test]
    fn a_cursor_that_wraps_goes_round_a_column() {
        let mut cursor = column(cursor::WRAPS_DOWN, 1);
        let mut seen = Vec::new();
        for _ in 0..5 {
            assert_eq!(
                cursor.step(pad::DOWN),
                Moved {
                    answer: 0,
                    sounds: true
                }
            );
            seen.push(cursor.at);
        }
        assert_eq!(seen, [2, 3, 0, 1, 2]);
        cursor.at = 0;
        assert!(cursor.step(pad::UP).sounds);
        assert_eq!(cursor.at, 3);
        // Nothing pressed, and a step to the side in one column: the cursor stays.
        for word in [0, pad::LEFT, pad::RIGHT] {
            assert_eq!(
                cursor.step(word),
                Moved {
                    answer: 0,
                    sounds: false
                }
            );
        }
        // Up and down in one pass on the first line: up finds the edge, down steps, and
        // the wrap of the edge is added behind both.
        cursor.at = 0;
        cursor.step(pad::UP | pad::DOWN);
        assert_eq!(cursor.at, 3);
    }

    #[test]
    fn a_cursor_without_a_wrap_stops_or_leaves() {
        let mut cursor = column(0, 3);
        assert!(!cursor.step(pad::DOWN).sounds);
        assert_eq!(cursor.at, 3);
        let mut cursor = column(cursor::LEAVES_DOWN | cursor::LEAVES_UP, 3);
        assert_eq!(cursor.step(pad::DOWN).answer, 4);
        cursor.at = 0;
        assert_eq!(cursor.step(pad::UP).answer, 3);
        // A cursor behind the last line is put on it, a negative one on the first.
        let mut cursor = Cursor {
            lines: 3,
            ..column(0, 3)
        };
        assert!(cursor.step(0).sounds);
        assert_eq!(cursor.at, 2);
        cursor.at = -5;
        cursor.step(0);
        assert_eq!(cursor.at, 0);
    }

    #[test]
    fn lines_scroll_under_the_rows() {
        // Six lines under three rows, as the first place map of a new game has them.
        let mut cursor = Cursor {
            columns: 1,
            rows: 3,
            lines: 6,
            flags: cursor::SCROLLS,
            at: 0,
            first: 0,
        };
        assert_eq!(cursor.hides(), (false, true));
        let mut seen = Vec::new();
        for _ in 0..6 {
            let moved = cursor.step(pad::DOWN);
            seen.push((cursor.at, cursor.first, moved.answer, moved.sounds));
        }
        assert_eq!(
            seen,
            [
                (1, 0, 0, true),
                (2, 0, 0, true),
                (2, 1, SCROLLED_DOWN, true),
                (2, 2, SCROLLED_DOWN, true),
                (2, 3, SCROLLED_DOWN, true),
                (2, 3, 0, false),
            ]
        );
        assert_eq!(cursor.hides(), (true, false));
        cursor.step(pad::UP);
        cursor.step(pad::UP);
        assert_eq!((cursor.at, cursor.first), (0, 3));
        assert_eq!(cursor.step(pad::UP).answer, SCROLLED_UP);
        assert_eq!((cursor.at, cursor.first), (0, 2));
        assert_eq!(cursor.hides(), (true, true));
        // R1 turns a page up and R2 one down, as far as there are lines; R1 is deaf
        // while R2 is held.
        cursor.step(pad::R1);
        assert_eq!((cursor.at, cursor.first), (0, 0));
        assert!(!cursor.step(pad::R1).sounds);
        assert!(cursor.step(pad::R2).sounds);
        assert_eq!((cursor.at, cursor.first), (0, 3));
        cursor.step_with(pad::R1, pad::R1 | pad::R2);
        assert_eq!(cursor.first, 3);
        cursor.step_with(pad::R1, pad::R1);
        assert_eq!(cursor.first, 0);
        // A first line behind the lines is pulled back in any pass.
        cursor.first = 9;
        cursor.step(0);
        assert_eq!(cursor.first, 3);
    }

    #[test]
    fn a_scroll_sign_lies_around_its_place_and_turns() {
        // An image of 8 by 6: its first texel, and the texel one right of it.
        assert_eq!(sign::texel(0, (8, 6), (0, 0)), (-3, -2));
        assert_eq!(sign::texel(0, (8, 6), (1, 0)), (-2, -2));
        // A quarter turn takes the image's right to the screen's bottom.
        assert_eq!(sign::texel(sign::BELOW, (8, 6), (0, 0)), (2, -3));
        assert_eq!(sign::texel(sign::BELOW, (8, 6), (1, 0)), (2, -2));
        assert_eq!(sign::texel(sign::ABOVE, (8, 6), (0, 0)), (-2, 3));
        assert_eq!(sign::texel(sign::ABOVE, (8, 6), (1, 0)), (-2, 2));
        // The sign pulses as a frame does: up in 10 of 40 steps of the time.
        assert_eq!(pulse(sign::DIM, sign::BRIGHT, 0), sign::DIM);
        assert_eq!(pulse(sign::DIM, sign::BRIGHT, 10), sign::BRIGHT);
        assert_eq!(pulse(sign::DIM, sign::BRIGHT, 25), [191; 3]);
    }

    #[test]
    fn a_grid_wraps_across_and_leaves_by_its_sides() {
        let grid = |flags: u16, at: i16| Cursor {
            columns: 3,
            rows: 2,
            lines: 6,
            flags,
            at,
            first: 0,
        };
        let mut cursor = grid(cursor::WRAPS_ACROSS, 2);
        cursor.step(pad::RIGHT);
        assert_eq!(cursor.at, 0);
        cursor.step(pad::LEFT);
        assert_eq!(cursor.at, 2);
        cursor.step(pad::DOWN);
        assert_eq!(cursor.at, 5);
        cursor.step(pad::LEFT);
        assert_eq!(cursor.at, 4);
        let mut cursor = grid(cursor::LEAVES_RIGHT | cursor::LEAVES_LEFT, 5);
        assert_eq!(cursor.step(pad::RIGHT).answer, 1);
        cursor.at = 3;
        assert_eq!(cursor.step(pad::LEFT).answer, 2);
        assert_eq!(cursor.at, 3);
    }

    #[test]
    fn a_content_is_placed_by_the_flags_of_its_piece() {
        let line = Rect {
            x: 186,
            y: 84,
            w: 108,
            h: 16,
        };
        let room = line.inside((0, 2));
        assert_eq!(
            room,
            Rect {
                x: 186,
                y: 86,
                w: 108,
                h: 12
            }
        );
        assert_eq!(placed(0, room, 80), (186, 86));
        assert_eq!(placed(piece::CENTRED, room, 80), (200, 86));
        assert_eq!(placed(piece::RIGHT, room, 80), (214, 86));
        assert_eq!(placed(piece::RIGHT | piece::CENTRED, room, 80), (214, 86));
        assert_eq!(placed(piece::CENTRED, room, 57), (211, 86));
        // Wider than the room of 108: three too many put it two to the left.
        assert_eq!(placed(piece::CENTRED, room, 111), (184, 86));
        assert_eq!((brightness(0), brightness(piece::DIM)), (128, 64));
    }

    #[test]
    fn a_menu_record_is_read_with_its_lines() {
        // A record at 0x1000, its list of two lines behind it, then the pieces: a line
        // of one piece and a line of two, each ended by a piece with the last flag.
        let base = 0x1000;
        let mut image = vec![0u8; 24 + 12 + 5 * PIECE];
        let word = |image: &mut Vec<u8>, at: usize, value: u32| {
            image[at..at + 4].copy_from_slice(&value.to_le_bytes());
        };
        let half = |image: &mut Vec<u8>, at: usize, value: i16| {
            image[at..at + 2].copy_from_slice(&value.to_le_bytes());
        };
        word(&mut image, 0, 0x2102);
        word(&mut image, 8, 8);
        word(&mut image, 12, 1);
        word(&mut image, 16, base + 24);
        word(&mut image, 20, 14);
        let pieces = 36;
        word(&mut image, 24, base + pieces as u32);
        word(&mut image, 28, base + (pieces + 2 * PIECE) as u32);
        let mut put = |number: usize, flags: i16, kind: i16, value: u32, x: i16| {
            let at = pieces + number * PIECE;
            half(&mut image, at, flags);
            half(&mut image, at + 2, kind);
            word(&mut image, at + 4, value);
            for (place, part) in [x, 20, 30, 40, 1, 2].into_iter().enumerate() {
                half(&mut image, at + 16 + place * 2, part);
            }
        };
        put(0, 8, 1, 5, 10);
        put(1, 1, 0, 0, 0);
        put(2, 0, 2, 0x1234, 11);
        put(3, 0x20, 3, 0x5678, 12);
        put(4, 1, 0, 0, 0);
        let menu = Menu::at(&image, base, base).expect("a menu");
        assert_eq!(
            (menu.flags, menu.openness, menu.cursor, menu.slot),
            (0x2102, 8, 1, 14)
        );
        assert_eq!(menu.lines.len(), 2);
        assert_eq!(
            menu.lines[0],
            [Piece {
                flags: piece::CENTRED,
                kind: kind::PICTURE,
                value: 5,
                rect: Rect {
                    x: 10,
                    y: 20,
                    w: 30,
                    h: 40
                },
                margin: (1, 2),
            }]
        );
        let kinds: Vec<(u16, u32)> = (menu.lines[1].iter())
            .map(|piece| (piece.kind, piece.value))
            .collect();
        assert_eq!(kinds, [(kind::SMALL_TEXT, 0x1234), (kind::TEXT, 0x5678)]);
        // A list that runs out of the image is no menu.
        assert_eq!(Menu::at(&image[..40], base, base), None);
    }

    #[test]
    fn the_flags_of_a_menu_give_the_style_of_a_frame() {
        // Kind 4 named, the pulsing mode, the default blend.
        let style = Style {
            kind: 4,
            mode: 1,
            blend: 1,
        };
        assert_eq!(Style::of_line(0x2102, true, 0), (style, true));
        assert_eq!(Style::of_line(0x2102, false, 0), (style, false));
        assert_eq!(style.blend_mode(), Some(0));
        // No kind named: the game's own. Flag 4 marks only the cursor's line.
        let (own, _) = Style::of_line(0x0004, true, 3);
        assert_eq!((own.kind, own.mode), (3, 2));
        assert_eq!(Style::of_line(0x0004, false, 3).0.mode, 0);
        assert_eq!(Style::of_line(0x0508, false, 3).0.kind, 1);
        assert_eq!(Style::of_line(0x0508, false, 3).0.blend_mode(), None);
        assert_eq!(Style::of_line(0x0180, false, 3).0.blend, 4);
        assert_eq!(frame_time(7, 1), 3);
        assert_eq!(frame_time(7, 2), 7);
    }

    /// Tables made up for the tests: the rules of the corners are those of a frame with
    /// four slanted edges around a middle, the colours are arbitrary.
    fn frames() -> Frames {
        Frames {
            plain: [9, 9, 9],
            quad_shares: [150, 200, 50, 25, 100],
            corners: [
                [0, 0, 3, 0, 1, 1, 2, 1],
                [0, 0, 1, 1, 0, 3, 1, 2],
                [1, 2, 2, 2, 0, 3, 3, 3],
                [2, 1, 3, 0, 2, 2, 3, 3],
                [1, 1, 2, 1, 1, 2, 2, 2],
            ],
            colours: [
                [[40, 80, 200]; KINDS],
                [[40, 80, 200]; KINDS],
                [[1, 2, 3]; KINDS],
            ],
            pulse_shares: [[100, 50, 200]; KINDS],
            pulse_ends: [[0, 100, 20]; KINDS],
        }
    }

    #[test]
    fn a_frame_pulses_between_two_colours() {
        let frames = frames();
        let style = |mode| Style {
            kind: 2,
            mode,
            blend: 1,
        };
        assert_eq!(frames.colour(style(0), 7), Some([40, 80, 200]));
        assert_eq!(frames.colour(style(2), 7), Some([1, 2, 3]));
        // The first end is the colour by the kind's shares, cut off at 255.
        assert_eq!(frames.colour(style(1), 0), Some([40, 40, 255]));
        // Ten steps up to the other end, thirty back.
        assert_eq!(frames.colour(style(1), 5), Some([20, 70, 138]));
        assert_eq!(frames.colour(style(1), 10), Some([0, 100, 20]));
        assert_eq!(frames.colour(style(1), 25), Some([20, 70, 138]));
        assert_eq!(frames.colour(style(1), 40), Some([40, 40, 255]));
        assert_eq!(frames.colour(style(1), 45), frames.colour(style(1), 5));
        let outside = Style {
            kind: 5,
            mode: 1,
            blend: 1,
        };
        assert_eq!(frames.colour(outside, 0), None);
        assert_eq!(
            frames.colour(
                Style {
                    kind: -1,
                    ..outside
                },
                0
            ),
            None
        );
    }

    #[test]
    fn the_quads_of_a_frame_cover_its_rectangle_once() {
        let frames = frames();
        let rect = Rect {
            x: 10,
            y: 20,
            w: 13,
            h: 8,
        };
        let quads = frames.quads(rect, Some([100, 100, 200]));
        assert_eq!(quads[0].corners, [(10, 20), (23, 20), (12, 22), (21, 22)]);
        assert_eq!(quads[4].corners, [(12, 22), (21, 22), (12, 26), (21, 26)]);
        let colours: Vec<[u8; 3]> = quads.iter().map(|quad| quad.colour).collect();
        assert_eq!(
            colours,
            [
                [150, 150, 255],
                [200, 200, 255],
                [50, 50, 100],
                [25, 25, 50],
                [100, 100, 200]
            ]
        );
        for y in 15..35 {
            for x in 5..30 {
                let covering = quads.iter().filter(|quad| quad.covers(x, y)).count();
                let inside = (10..23).contains(&x) && (20..28).contains(&y);
                assert_eq!(covering, usize::from(inside), "pixel {x}, {y}");
            }
        }
        // The middle is the rectangle without its edge of two pixels.
        assert!(quads[4].covers(12, 22) && quads[4].covers(20, 25));
        assert!(!quads[4].covers(11, 22) && !quads[4].covers(21, 22));
        // Without a colour every quad has the plain one.
        assert!(
            frames
                .quads(rect, None)
                .iter()
                .all(|quad| quad.colour == [9, 9, 9])
        );
    }
}
