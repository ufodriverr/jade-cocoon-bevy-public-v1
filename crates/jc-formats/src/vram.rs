
use crate::mesh::{MeshBlock, TexturePage};
use crate::rooms::{Image, RoomTableError, step};
use crate::tim::{PixelMode, Tim, color_to_rgba};

/// Width of video memory in 16-bit words.
pub const WIDTH: usize = 1024;
/// Height of video memory in rows.
pub const HEIGHT: usize = 512;
/// Pixels a texture page is wide and high: the range of a face's texture coordinates.
pub const PAGE: usize = 256;
/// Bit of a colour that asks for blending where the face is drawn blended.
pub const BLEND_MARK: u16 = 0x8000;
/// Alpha that [`Vram::page_rgba`] gives a texel that is blended half and half.
pub const HALF: u8 = 128;

/// Video memory: 1024 by 512 words of 16 bits.
#[derive(Clone, PartialEq, Eq)]
pub struct Vram {
    words: Vec<u16>,
}

impl Default for Vram {
    fn default() -> Self {
        Self::new()
    }
}

impl Vram {
    /// Empty video memory: every word zero.
    pub fn new() -> Self {
        Self {
            words: vec![0; WIDTH * HEIGHT],
        }
    }

    /// Video memory after sending every image of a texture set, in the order of the set.
    pub fn with<'a>(images: impl IntoIterator<Item = &'a Tim>) -> Self {
        let mut vram = Self::new();
        images.into_iter().for_each(|image| vram.upload(image));
        vram
    }

    /// The word at a place, zero outside the memory.
    pub fn word(&self, x: usize, y: usize) -> u16 {
        if x < WIDTH && y < HEIGHT {
            self.words[y * WIDTH + x]
        } else {
            0
        }
    }

    fn put(&mut self, x: usize, y: usize, word: u16) {
        if x < WIDTH && y < HEIGHT {
            self.words[y * WIDTH + x] = word;
        }
    }

    /// Sends an image to its place: its palette, when it brings colours, and its pixels.
    pub fn upload(&mut self, image: &Tim) {
        if let Some(clut) = &image.clut {
            let (x, y, w) = (
                clut.rect.x as usize,
                clut.rect.y as usize,
                clut.rect.w as usize,
            );
            for (index, &colour) in clut.colors.iter().enumerate() {
                self.put(x + index % w, y + index / w, colour);
            }
        }
        let (x, y, w) = (
            image.vram.x as usize,
            image.vram.y as usize,
            image.vram.w as usize,
        );
        for (index, word) in image.indexes().as_chunks::<2>().0.iter().enumerate() {
            self.put(x + index % w, y + index / w, u16::from_le_bytes(*word));
        }
    }

    /// Sends a picture of 16-bit words, `wide` to a row, to the place (`x`, `y`), as the
    /// console's load of a rectangle does it.
    pub fn send(&mut self, (x, y): (usize, usize), wide: usize, words: &[u16]) {
        for (index, &word) in words.iter().enumerate() {
            self.put(x + index % wide.max(1), y + index / wide.max(1), word);
        }
    }

    /// The colour a face with this page and palette shows at texture coordinates `u`, `v`:
    /// the 16-bit colour as stored, zero for a texel that is not drawn.
    pub fn texel(&self, page: TexturePage, u: u8, v: u8) -> u16 {
        let (x, y) = page.origin();
        let (x, y, u) = (x as usize, y as usize + v as usize, u as usize);
        let (palette_x, palette_y) = page.palette_origin();
        let colour =
            |index: u16| self.word(palette_x as usize + index as usize, palette_y as usize);
        match page.depth() {
            // Four pixels to a word, the lowest four bits first.
            0 => colour(self.word(x + u / 4, y) >> (4 * (u % 4)) & 0xF),
            // Two pixels to a word, the low byte first.
            1 => colour(self.word(x + u / 2, y) >> (8 * (u % 2)) & 0xFF),
            _ => self.word(x + u, y),
        }
    }

    /// A whole texture page as 256 by 256 RGBA pixels, top row first.
    ///
    /// A texel of colour zero is not drawn and gets alpha 0. With `blended`, the picture is
    /// the one for a face that is drawn with the console's blending in mode 0 (half of the
    /// face, half of what is behind): a texel whose colour carries [`BLEND_MARK`] gets alpha
    /// [`HALF`]. Every other texel is opaque.
    pub fn page_rgba(&self, page: TexturePage, blended: bool) -> Vec<u8> {
        let mut out = Vec::with_capacity(PAGE * PAGE * 4);
        for v in 0..=u8::MAX {
            for u in 0..=u8::MAX {
                let colour = self.texel(page, u, v);
                let mut rgba = color_to_rgba(colour);
                if blended && colour & BLEND_MARK != 0 {
                    rgba[3] = HALF;
                }
                out.extend_from_slice(&rgba);
            }
        }
        out
    }

    /// The texels of a page that one drawing step of a blended face shows, as 256 by 256
    /// RGBA pixels, top row first: the chosen texels opaque, every other one with alpha 0.
    ///
    /// The console mixes a blended face with what is behind it only in the texels whose
    /// colour carries [`BLEND_MARK`] and draws its other texels as they are, so a blended
    /// face is two pictures: [`Texels::Unmarked`] drawn as it is and [`Texels::Marked`] drawn
    /// in the page's blend mode.
    pub fn page_part(&self, page: TexturePage, texels: Texels) -> Vec<u8> {
        let mut out = Vec::with_capacity(PAGE * PAGE * 4);
        for v in 0..=u8::MAX {
            for u in 0..=u8::MAX {
                let colour = self.texel(page, u, v);
                let mut rgba = color_to_rgba(colour);
                if (colour & BLEND_MARK != 0) != (texels == Texels::Marked) {
                    rgba[3] = 0;
                }
                out.extend_from_slice(&rgba);
            }
        }
        out
    }
}

pub const PLACE_TABLES: u32 = 0x8007_DB90;
/// Place tables the executable has.
pub const PLACE_TABLE_COUNT: usize = 5;
/// The place table of the field's model loader: the k-th different model of a room's object
/// list is read into slot k of it.
pub const FIELD_PLACES: usize = 1;
pub const CARRIED_PLACES: usize = 2;
/// Bytes of one record of a place table.
const PLACE_SIZE: usize = 16;
/// More slots than this is not a place table.
const MAX_PLACES: usize = 64;

/// The moves of one slot of a model loader: how far a model that is read into the slot lies
/// from where its file says, in video memory. The images of nearly every model file name the
/// same place, so only slot 0 has a model as its file stores it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Place {
    /// Words to the right and rows down for the pixels of an image.
    pub pixels: [i16; 2],
    /// The same for a palette of 16 colours.
    pub small_palette: [i16; 2],
    /// The same for a palette of 256 colours.
    pub large_palette: [i16; 2],
}

impl Place {
    /// The image as the loader sends it: its pixels moved, and its palette by the moves of
    /// its colour depth.
    pub fn image(&self, image: &Tim) -> Tim {
        let mut moved = image.clone();
        moved.vram.x = moved.vram.x.wrapping_add_signed(self.pixels[0]);
        moved.vram.y = moved.vram.y.wrapping_add_signed(self.pixels[1]);
        let by = match image.mode {
            PixelMode::Clut4 => self.small_palette,
            _ => self.large_palette,
        };
        if let Some(clut) = &mut moved.clut {
            clut.rect.x = clut.rect.x.wrapping_add_signed(by[0]);
            clut.rect.y = clut.rect.y.wrapping_add_signed(by[1]);
        }
        moved
    }

    /// The page and palette words of a face of the model as the mesh builder leaves them:
    /// the page grows by the pixel moves in pages (64 words, 256 rows), the palette word is
    /// taken apart into x and row, moved by the pair of the page's colour depth and put
    /// together again.
    pub fn page(&self, page: TexturePage) -> TexturePage {
        let pages = self.pixels[0] / 64 + 16 * (self.pixels[1] / 256);
        let by = match page.depth() {
            0 => self.small_palette,
            _ => self.large_palette,
        };
        let (x, row) = page.palette_origin();
        let (x, row) = (x.wrapping_add_signed(by[0]), row.wrapping_add_signed(by[1]));
        TexturePage {
            page: page.page.wrapping_add_signed(pages),
            palette: (x / 16) & 0x3F | (row & 0x3FF) << 6,
        }
    }

    /// Moves the page and palette words of every textured face and seam vertex of a mesh
    /// block, as the mesh builder does when the model is read into the slot.
    pub fn block(&self, block: &mut MeshBlock) {
        for object in &mut block.objects {
            for face in &mut object.faces {
                face.texture = face.texture.map(|page| self.page(page));
            }
            for vertex in &mut object.seam {
                vertex.texture = vertex.texture.map(|page| self.page(page));
            }
        }
    }
}

/// The place tables of the loaded executable, whose first byte is at RAM address `base`: a
/// [`Place`] per slot of each of the [`PLACE_TABLE_COUNT`] tables. A table ends with a record
/// whose 16-bit value at +2 is -1.
pub fn place_tables(image: &[u8], base: u32) -> Result<Vec<Vec<Place>>, RoomTableError> {
    let image = Image { data: image, base };
    let mut tables = Vec::with_capacity(PLACE_TABLE_COUNT);
    for table in 0..PLACE_TABLE_COUNT {
        let start = image.u32(step(PLACE_TABLES, table, 4)?)?;
        let mut places = Vec::new();
        loop {
            let record = step(start, places.len(), PLACE_SIZE)?;
            if image.i16(record + 2)? == -1 {
                break;
            }
            if places.len() == MAX_PLACES {
                return Err(RoomTableError::OutOfImage(record));
            }
            let pair = |at: u32| -> Result<[i16; 2], RoomTableError> {
                Ok([image.i16(record + at)?, image.i16(record + at + 2)?])
            };
            places.push(Place {
                pixels: pair(4)?,
                small_palette: pair(8)?,
                large_palette: pair(12)?,
            });
        }
        tables.push(places);
    }
    Ok(tables)
}

/// Which drawn texels of a page [`Vram::page_part`] keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Texels {
    /// The ones whose colour carries [`BLEND_MARK`].
    Marked,
    /// The ones without it.
    Unmarked,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A block of a TIM: its rectangle (x, y, width in words, height) and its words.
    type Block<'a> = ((u16, u16, u16, u16), &'a [u16]);

    /// A TIM of `mode` with a palette block and a pixel block.
    fn tim(mode: u32, palette: Option<Block>, pixels: Block) -> Tim {
        let mut data = Vec::new();
        data.extend_from_slice(&0x10u32.to_le_bytes());
        data.extend_from_slice(&(mode | if palette.is_some() { 8 } else { 0 }).to_le_bytes());
        for ((x, y, w, h), words) in palette.into_iter().chain([pixels]) {
            data.extend_from_slice(&(12 + 2 * words.len() as u32).to_le_bytes());
            for value in [x, y, w, h] {
                data.extend_from_slice(&value.to_le_bytes());
            }
            for word in words {
                data.extend_from_slice(&word.to_le_bytes());
            }
        }
        Tim::parse(&data, 0).unwrap()
    }

    /// The page word of a page at `x`, `y` with a depth, and the palette word of a row.
    fn page(x: u16, y: u16, depth: u16, palette: (u16, u16)) -> TexturePage {
        TexturePage {
            page: (x / 64) | ((y / 256) << 4) | (depth << 7),
            palette: (palette.0 / 16) | (palette.1 << 6),
        }
    }

    #[test]
    fn an_eight_bit_page_reads_bytes_through_the_palette() {
        let mut colours = [0u16; 256];
        colours[1] = 0x001F;
        colours[2] = 0x83E0;
        // Two rows of four pixels at (320, 256): indices 1, 2, 0, 1 and 2, 2, 1, 0.
        let image = tim(
            1,
            Some(((768, 385, 256, 1), &colours)),
            ((320, 256, 2, 2), &[0x0201, 0x0100, 0x0202, 0x0001]),
        );
        let vram = Vram::with([&image]);
        let page = page(320, 256, 1, (768, 385));
        assert_eq!(vram.texel(page, 0, 0), 0x001F);
        assert_eq!(vram.texel(page, 1, 0), 0x83E0);
        assert_eq!(vram.texel(page, 2, 0), 0);
        assert_eq!(vram.texel(page, 3, 0), 0x001F);
        assert_eq!(vram.texel(page, 0, 1), 0x83E0);
        assert_eq!(vram.texel(page, 2, 1), 0x001F);

        let opaque = vram.page_rgba(page, false);
        assert_eq!(opaque.len(), PAGE * PAGE * 4);
        assert_eq!(opaque[..12], [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 0, 0]);
        // Only the marked colour is blended, and only where the face is.
        let blended = vram.page_rgba(page, true);
        assert_eq!(blended[..12], [255, 0, 0, 255, 0, 255, 0, HALF, 0, 0, 0, 0]);
        // The two drawing steps of a blended face share out the drawn texels.
        let marked = vram.page_part(page, Texels::Marked);
        assert_eq!(marked[..12], [255, 0, 0, 0, 0, 255, 0, 255, 0, 0, 0, 0]);
        let unmarked = vram.page_part(page, Texels::Unmarked);
        assert_eq!(unmarked[..12], [255, 0, 0, 255, 0, 255, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn a_four_bit_page_reads_nibbles_low_first() {
        let mut colours = [0u16; 16];
        colours[3] = 0x7C00;
        colours[15] = 0x7FFF;
        // The page starts 64 words left of the image: the image's first pixel is u = 8.
        let image = tim(
            0,
            Some(((960, 320, 16, 1), &colours)),
            ((770, 3, 1, 1), &[0xF030]),
        );
        let vram = Vram::with([&image]);
        let page = page(768, 0, 0, (960, 320));
        assert_eq!(vram.texel(page, 8, 3), 0);
        assert_eq!(vram.texel(page, 9, 3), 0x7C00);
        assert_eq!(vram.texel(page, 10, 3), 0);
        assert_eq!(vram.texel(page, 11, 3), 0x7FFF);
        assert_eq!(vram.texel(page, 9, 2), 0);
    }

    #[test]
    fn an_image_without_colours_uses_the_palette_that_is_there() {
        let mut colours = [0u16; 256];
        colours[7] = 0x1234;
        let first = tim(
            1,
            Some(((768, 256, 256, 1), &colours)),
            ((768, 0, 1, 1), &[0]),
        );
        let second = tim(
            1,
            Some(((768, 256, 256, 1), &[])),
            ((768, 96, 1, 1), &[0x0700]),
        );
        let vram = Vram::with([&first, &second]);
        assert_eq!(vram.texel(page(768, 0, 1, (768, 256)), 1, 96), 0x1234);
    }

    /// The second slot of the field's loader.
    const SECOND: Place = Place {
        pixels: [64, 0],
        small_palette: [16, 0],
        large_palette: [0, 2],
    };

    #[test]
    fn a_moved_model_shows_the_same_texels_at_its_new_place() {
        let mut colours = [0u16; 256];
        colours[1] = 0x001F;
        colours[2] = 0x83E0;
        let large = tim(
            1,
            Some(((768, 385, 256, 1), &colours)),
            ((320, 256, 2, 1), &[0x0201, 0x0100]),
        );
        let mut few = [0u16; 16];
        few[3] = 0x7C00;
        let small = tim(
            0,
            Some(((768, 384, 16, 1), &few)),
            ((322, 256, 1, 1), &[0x0030]),
        );
        let pages = [page(320, 256, 1, (768, 385)), page(320, 256, 0, (768, 384))];
        let own = Vram::with([&large, &small]);

        // Another model at the same place of its file, read into slot 0.
        let other = tim(
            1,
            Some(((768, 385, 256, 1), &[0x7FFF; 256])),
            ((320, 256, 4, 1), &[0x0505; 4]),
        );
        let shared = Vram::with([&other, &SECOND.image(&large), &SECOND.image(&small)]);
        let moved = pages.map(|page| SECOND.page(page));
        assert_eq!(moved[0].origin(), (384, 256));
        assert_eq!(moved[0].palette_origin(), (768, 387));
        assert_eq!(moved[1].palette_origin(), (784, 384));
        assert_eq!(moved[0].depth(), 1);
        for (page, moved) in pages.into_iter().zip(moved) {
            for u in 0..16 {
                assert_eq!(shared.texel(moved, u, 0), own.texel(page, u, 0), "u {u}");
            }
        }
        // The model of slot 0 is still there.
        assert_eq!(shared.texel(pages[0], 0, 0), 0x7FFF);
        // Slot 0 moves nothing.
        assert_eq!(Place::default().page(pages[0]), pages[0]);
        assert_eq!(Place::default().image(&large), large);
    }

    #[test]
    fn place_tables_end_with_a_marked_record() {
        // Five pointers, then a table of two slots and its end record; every pointer names it.
        let base = PLACE_TABLES;
        let table = base + 4 * PLACE_TABLE_COUNT as u32;
        let mut image = Vec::new();
        for _ in 0..PLACE_TABLE_COUNT {
            image.extend_from_slice(&table.to_le_bytes());
        }
        for record in [
            [0i16, 0, 0, 0, 0, 0, 0, 0],
            [0, 0, 64, 0, 16, 0, 0, 2],
            [0, -1, 0, 0, 0, 0, 0, 0],
        ] {
            for value in record {
                image.extend_from_slice(&value.to_le_bytes());
            }
        }
        let tables = place_tables(&image, base).unwrap();
        assert_eq!(tables.len(), PLACE_TABLE_COUNT);
        assert_eq!(tables[FIELD_PLACES], [Place::default(), SECOND]);
        // A table without its end record leads out of the image.
        image.truncate(image.len() - PLACE_SIZE);
        assert!(place_tables(&image, base).is_err());
    }

    #[test]
    fn places_outside_the_memory_read_as_zero() {
        let image = tim(2, None, ((1023, 511, 2, 2), &[1, 2, 3, 4]));
        let vram = Vram::with([&image]);
        assert_eq!(vram.word(1023, 511), 1);
        assert_eq!(vram.word(1024, 511), 0);
        assert_eq!(vram.word(0, 512), 0);
        // A 16-bit page at the right edge: its texels beyond the memory are not drawn.
        assert_eq!(vram.texel(page(960, 256, 2, (0, 0)), 63, 255), 1);
        assert_eq!(vram.texel(page(960, 256, 2, (0, 0)), 64, 255), 0);
    }
}
