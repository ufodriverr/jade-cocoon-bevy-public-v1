
use std::fmt;

/// The first word of every TIM.
pub const TIM_ID: u32 = 0x10;
/// Bytes of a block header: its length, then the VRAM rectangle.
const BLOCK_HEADER: usize = 12;
/// Offsets at which [`find_all`] looks for a TIM.
const SCAN_STEP: usize = 4;

/// How the pixel block stores its pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelMode {
    /// 4 bits per pixel, an index into a 16-colour palette.
    Clut4,
    /// 8 bits per pixel, an index into a 256-colour palette.
    Clut8,
    /// 16 bits per pixel, a colour.
    Direct15,
    /// 24 bits per pixel, one byte each of red, green and blue.
    Direct24,
}

impl PixelMode {
    fn from_flags(flags: u32) -> Option<Self> {
        match flags & 7 {
            0 => Some(Self::Clut4),
            1 => Some(Self::Clut8),
            2 => Some(Self::Direct15),
            3 => Some(Self::Direct24),
            _ => None,
        }
    }

    /// Bits one pixel takes in the pixel block.
    pub fn bits_per_pixel(self) -> u32 {
        match self {
            Self::Clut4 => 4,
            Self::Clut8 => 8,
            Self::Direct15 => 16,
            Self::Direct24 => 24,
        }
    }

    /// Colours in one palette, or `None` when pixels are colours themselves.
    pub fn palette_len(self) -> Option<usize> {
        match self {
            Self::Clut4 => Some(16),
            Self::Clut8 => Some(256),
            Self::Direct15 | Self::Direct24 => None,
        }
    }
}

/// A rectangle in video memory. `w` counts 16-bit words, not pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VramRect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

/// The palette block of a TIM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clut {
    /// Where the palette goes in video memory.
    pub rect: VramRect,
    /// `rect.w * rect.h` colours, one palette per row. Empty when the TIM brings no palette
    /// of its own and uses the one another TIM put at `rect`.
    pub colors: Vec<u16>,
}

impl Clut {
    /// True when the palette has to come from another TIM.
    pub fn is_shared(&self) -> bool {
        self.colors.is_empty()
    }
}

/// One parsed TIM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tim {
    pub mode: PixelMode,
    pub clut: Option<Clut>,
    /// Where the pixels go in video memory.
    pub vram: VramRect,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Bytes the whole TIM takes in its file.
    pub size: usize,
    pixels: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimError {
    /// The texture is indexed and no palette was available for it.
    MissingPalette,
}

impl fmt::Display for TimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingPalette => write!(f, "indexed texture without a palette"),
        }
    }
}

impl std::error::Error for TimError {}

fn u16_at(data: &[u8], pos: usize) -> Option<u16> {
    Some(u16::from_le_bytes(data.get(pos..pos + 2)?.try_into().ok()?))
}

fn u32_at(data: &[u8], pos: usize) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(pos..pos + 4)?.try_into().ok()?))
}

/// Reads a block header at `pos`: the block length and its rectangle. `None` unless the
/// block lies inside `data` and has a non-empty rectangle.
fn block_header(data: &[u8], pos: usize) -> Option<(usize, VramRect)> {
    let len = u32_at(data, pos)? as usize;
    let rect = VramRect {
        x: u16_at(data, pos + 4)?,
        y: u16_at(data, pos + 6)?,
        w: u16_at(data, pos + 8)?,
        h: u16_at(data, pos + 10)?,
    };
    let fits = len >= BLOCK_HEADER && pos.checked_add(len)? <= data.len();
    (fits && rect.w != 0 && rect.h != 0).then_some((len, rect))
}

/// Bytes of block data a rectangle of 16-bit words holds.
fn rect_bytes(rect: VramRect) -> usize {
    rect.w as usize * rect.h as usize * 2
}

impl Tim {
    /// Parses a TIM that starts at `offset` in `data`. `None` when the bytes there are not a
    /// well-formed TIM, which makes this usable for scanning.
    pub fn parse(data: &[u8], offset: usize) -> Option<Self> {
        if u32_at(data, offset)? != TIM_ID {
            return None;
        }
        let flags = u32_at(data, offset + 4)?;
        if flags & !0xF != 0 {
            return None;
        }
        let mode = PixelMode::from_flags(flags)?;
        let mut pos = offset + 8;

        let mut clut = None;
        if flags & 8 != 0 {
            let (len, rect) = block_header(data, pos)?;
            let colors = if len == BLOCK_HEADER {
                Vec::new()
            } else if len == BLOCK_HEADER + rect_bytes(rect) {
                data[pos + BLOCK_HEADER..pos + len]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| u16::from_le_bytes(*c))
                    .collect()
            } else {
                return None;
            };
            clut = Some(Clut { rect, colors });
            pos += len;
        }

        let (len, vram) = block_header(data, pos)?;
        if len != BLOCK_HEADER + rect_bytes(vram) {
            return None;
        }
        let width = vram.w as u32 * 16 / mode.bits_per_pixel();
        if width == 0 {
            return None;
        }
        Some(Self {
            mode,
            clut,
            vram,
            width,
            height: vram.h as u32,
            size: pos + len - offset,
            pixels: data[pos + BLOCK_HEADER..pos + len].to_vec(),
        })
    }

    /// The pixel block as stored, `vram.w * 2` bytes per row. For an 8-bit TIM this is one
    /// palette index per pixel.
    pub fn indexes(&self) -> &[u8] {
        &self.pixels
    }

    /// The TIM's own palette number `row`, or the last one when it has fewer rows. `None`
    /// when the TIM is not indexed or its palette is shared.
    pub fn palette(&self, row: usize) -> Option<&[u16]> {
        let clut = self.clut.as_ref().filter(|clut| !clut.is_shared())?;
        let per_row = clut.rect.w as usize;
        let start = row.min(clut.rect.h as usize - 1) * per_row;
        let len = self.mode.palette_len()?.min(clut.colors.len() - start);
        Some(&clut.colors[start..start + len])
    }

    /// Decodes to `width * height` RGBA pixels, top row first.
    ///
    /// An indexed texture takes its colours from `palette`. Pass [`Tim::palette`] for a TIM
    /// that has its own, or the palette of another TIM for a shared one. An index past the
    /// end of the palette decodes as colour 0.
    pub fn to_rgba(&self, palette: Option<&[u16]>) -> Result<Vec<u8>, TimError> {
        let count = self.width as usize * self.height as usize;
        let mut out = Vec::with_capacity(count * 4);
        let indexed = |index: u8| -> Result<[u8; 4], TimError> {
            let palette = palette.ok_or(TimError::MissingPalette)?;
            Ok(color_to_rgba(
                palette.get(index as usize).copied().unwrap_or(0),
            ))
        };
        for i in 0..count {
            let rgba = match self.mode {
                PixelMode::Clut4 => {
                    // The low nibble of a byte is the pixel on the left.
                    let byte = self.pixels[i / 2];
                    indexed(if i % 2 == 0 { byte & 0xF } else { byte >> 4 })?
                }
                PixelMode::Clut8 => indexed(self.pixels[i])?,
                PixelMode::Direct15 => color_to_rgba(u16::from_le_bytes([
                    self.pixels[i * 2],
                    self.pixels[i * 2 + 1],
                ])),
                PixelMode::Direct24 => {
                    // Rows are padded to whole 16-bit words.
                    let row_bytes = self.vram.w as usize * 2;
                    let (x, y) = (i % self.width as usize, i / self.width as usize);
                    let p = y * row_bytes + x * 3;
                    [self.pixels[p], self.pixels[p + 1], self.pixels[p + 2], 255]
                }
            };
            out.extend_from_slice(&rgba);
        }
        Ok(out)
    }
}

/// Converts a 15-bit colour (red in the low five bits, then green, then blue) to RGBA. The
/// all-zero word is the console's transparent texel; the top bit, which selects blending on
/// the console, is otherwise ignored here.
pub fn color_to_rgba(color: u16) -> [u8; 4] {
    let expand = |v: u16| {
        let v = (v & 0x1F) as u8;
        v << 3 | v >> 2
    };
    let alpha = if color == 0 { 0 } else { 255 };
    [
        expand(color),
        expand(color >> 5),
        expand(color >> 10),
        alpha,
    ]
}

/// Finds every TIM in a file, with the offset each one starts at. Looks at every fourth
/// byte and continues right after each TIM found.
pub fn find_all(data: &[u8]) -> Vec<(usize, Tim)> {
    let mut found = Vec::new();
    let mut offset = 0;
    while offset + 8 <= data.len() {
        match Tim::parse(data, offset) {
            Some(tim) => {
                let size = tim.size;
                found.push((offset, tim));
                offset += size.max(SCAN_STEP);
            }
            None => offset += SCAN_STEP,
        }
    }
    found
}

pub fn texture_set(data: &[u8]) -> Option<Vec<Tim>> {
    let count = u32_at(data, 0)? as usize;
    // The first image lies right behind the offsets.
    if count == 0 {
        return Some(Vec::new());
    }
    if count > data.len() / 4 || u32_at(data, 4)? as usize != 4 + 4 * count {
        return None;
    }
    (0..count)
        .map(|index| Tim::parse(data, u32_at(data, 4 + 4 * index)? as usize))
        .collect()
}

/// The palette for a TIM whose own palette block is empty: palette `row` of the first TIM in
/// `tims` that carries colours for the same position in video memory.
pub fn shared_palette<'a>(
    tims: impl IntoIterator<Item = &'a Tim>,
    rect: VramRect,
    row: usize,
) -> Option<&'a [u16]> {
    tims.into_iter()
        .filter(|tim| {
            tim.clut
                .as_ref()
                .is_some_and(|clut| (clut.rect.x, clut.rect.y) == (rect.x, rect.y))
        })
        .find_map(|tim| tim.palette(row))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(rect: (u16, u16, u16, u16), body: &[u8]) -> Vec<u8> {
        let mut out = ((BLOCK_HEADER + body.len()) as u32).to_le_bytes().to_vec();
        for v in [rect.0, rect.1, rect.2, rect.3] {
            out.extend(v.to_le_bytes());
        }
        out.extend(body);
        out
    }

    fn words(values: &[u16]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    fn tim(flags: u32, blocks: &[Vec<u8>]) -> Vec<u8> {
        let mut out = TIM_ID.to_le_bytes().to_vec();
        out.extend(flags.to_le_bytes());
        for block in blocks {
            out.extend(block);
        }
        out
    }

    const RED: u16 = 0x001F;
    const GREEN: u16 = 0x03E0;
    const BLUE: u16 = 0x7C00;
    const WHITE: u16 = 0x7FFF;

    fn palette16() -> Vec<u16> {
        let mut colors = vec![0u16; 16];
        colors[1] = RED;
        colors[2] = GREEN;
        colors[15] = WHITE;
        colors
    }

    #[test]
    fn reads_a_texture_set() {
        let image = |x: u16| tim(2, &[block((x, 0, 1, 1), &words(&[RED]))]);
        let (first, second) = (image(320), image(384));
        let mut set = words(&[2, 0, 12, 0, 12 + first.len() as u16, 0]);
        set.extend(&first);
        set.extend(&second);
        let images = texture_set(&set).unwrap();
        assert_eq!(images.len(), 2);
        assert_eq!((images[0].vram.x, images[1].vram.x), (320, 384));

        assert_eq!(texture_set(&[0, 0, 0, 0]), Some(Vec::new()));
        // The first offset has to be the one behind the offsets, and every image a TIM.
        let mut wrong = set.clone();
        wrong[4] = 16;
        assert_eq!(texture_set(&wrong), None);
        assert_eq!(texture_set(&set[..set.len() - 1]), None);
        assert_eq!(texture_set(&[]), None);
    }

    #[test]
    fn converts_colors() {
        assert_eq!(color_to_rgba(0), [0, 0, 0, 0]);
        assert_eq!(color_to_rgba(0x8000), [0, 0, 0, 255]);
        assert_eq!(color_to_rgba(RED), [255, 0, 0, 255]);
        assert_eq!(color_to_rgba(GREEN), [0, 255, 0, 255]);
        assert_eq!(color_to_rgba(BLUE), [0, 0, 255, 255]);
        assert_eq!(color_to_rgba(0x0010), [132, 0, 0, 255]);
    }

    #[test]
    fn decodes_4bpp_low_nibble_first() {
        // One word wide is four pixels: indices 1, 2, 15, 0.
        let data = tim(
            8,
            &[
                block((0, 480, 16, 1), &words(&palette16())),
                block((320, 0, 1, 1), &[0x21, 0x0F]),
            ],
        );
        let tim = Tim::parse(&data, 0).unwrap();
        assert_eq!(tim.mode, PixelMode::Clut4);
        assert_eq!((tim.width, tim.height), (4, 1));
        assert_eq!(tim.size, data.len());
        let rgba = tim.to_rgba(tim.palette(0)).unwrap();
        assert_eq!(
            rgba,
            [
                [255, 0, 0, 255],
                [0, 255, 0, 255],
                [255, 255, 255, 255],
                [0, 0, 0, 0]
            ]
            .concat()
        );
    }

    #[test]
    fn decodes_8bpp_and_picks_the_palette_row() {
        let mut colors = vec![0u16; 512];
        colors[3] = RED;
        colors[256 + 3] = BLUE;
        let data = tim(
            9,
            &[
                block((0, 480, 256, 2), &words(&colors)),
                block((320, 0, 1, 2), &[3, 0, 0, 3]),
            ],
        );
        let tim = Tim::parse(&data, 0).unwrap();
        assert_eq!((tim.width, tim.height), (2, 2));
        let first = tim.to_rgba(tim.palette(0)).unwrap();
        assert_eq!(first[0..4], [255, 0, 0, 255]);
        assert_eq!(first[12..16], [255, 0, 0, 255]);
        assert_eq!(first[4..8], [0, 0, 0, 0]);
        let second = tim.to_rgba(tim.palette(1)).unwrap();
        assert_eq!(second[0..4], [0, 0, 255, 255]);
        // A row past the last one falls back to the last.
        assert_eq!(tim.palette(7), tim.palette(1));
    }

    #[test]
    fn decodes_16bpp() {
        let data = tim(2, &[block((0, 0, 2, 1), &words(&[GREEN, 0]))]);
        let tim = Tim::parse(&data, 0).unwrap();
        assert_eq!(tim.clut, None);
        assert_eq!((tim.width, tim.height), (2, 1));
        assert_eq!(tim.to_rgba(None).unwrap(), [0, 255, 0, 255, 0, 0, 0, 0]);
    }

    #[test]
    fn decodes_24bpp_with_padded_rows() {
        // Three words per row are six bytes: two pixels. Two rows.
        let body = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        let data = tim(3, &[block((0, 0, 3, 2), &body)]);
        let tim = Tim::parse(&data, 0).unwrap();
        assert_eq!((tim.width, tim.height), (2, 2));
        let rgba = tim.to_rgba(None).unwrap();
        assert_eq!(rgba[0..8], [1, 2, 3, 255, 4, 5, 6, 255]);
        assert_eq!(rgba[8..16], [7, 8, 9, 255, 10, 11, 12, 255]);
    }

    #[test]
    fn shared_palette_comes_from_the_tim_at_the_same_position() {
        let owner = tim(
            8,
            &[
                block((16, 481, 16, 1), &words(&palette16())),
                block((320, 0, 1, 1), &[0, 0]),
            ],
        );
        let sharer = tim(
            8,
            &[
                block((16, 481, 16, 1), &[]),
                block((321, 0, 1, 1), &[0x11, 0x22]),
            ],
        );
        let mut file = vec![0u8; 8];
        file.extend(&owner);
        file.extend(&sharer);
        let found = find_all(&file);
        assert_eq!(
            found.iter().map(|(offset, _)| *offset).collect::<Vec<_>>(),
            [8, 8 + owner.len()]
        );
        let shared = &found[1].1;
        let clut = shared.clut.as_ref().unwrap();
        assert!(clut.is_shared());
        assert_eq!(shared.palette(0), None);
        assert_eq!(shared.to_rgba(None), Err(TimError::MissingPalette));

        let palette = shared_palette(found.iter().map(|(_, tim)| tim), clut.rect, 0);
        let rgba = shared.to_rgba(palette).unwrap();
        assert_eq!(rgba[0..4], [255, 0, 0, 255]);
        assert_eq!(rgba[8..12], [0, 255, 0, 255]);

        let elsewhere = VramRect { x: 32, ..clut.rect };
        assert_eq!(
            shared_palette(found.iter().map(|(_, tim)| tim), elsewhere, 0),
            None
        );
    }

    #[test]
    fn rejects_malformed_input() {
        let good = tim(2, &[block((0, 0, 1, 1), &[0, 0])]);
        assert!(Tim::parse(&good, 0).is_some());

        let mut wrong_id = good.clone();
        wrong_id[0] = 0x11;
        assert!(Tim::parse(&wrong_id, 0).is_none());

        let mut unknown_flags = good.clone();
        unknown_flags[4] = 0x12;
        assert!(Tim::parse(&unknown_flags, 0).is_none());

        let mut wrong_length = good.clone();
        wrong_length[8] += 2;
        wrong_length.extend([0, 0]);
        assert!(Tim::parse(&wrong_length, 0).is_none());

        assert!(Tim::parse(&good[..good.len() - 1], 0).is_none());
        assert!(Tim::parse(&tim(2, &[block((0, 0, 0, 1), &[])]), 0).is_none());
        assert!(Tim::parse(&[], 0).is_none());
        assert!(find_all(&[0x10, 0, 0, 0]).is_empty());
    }
}
