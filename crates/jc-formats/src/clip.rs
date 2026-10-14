
use crate::movie::{BitstreamError, CodeTable, PictureDecoder, PictureError};

/// Where the picture goes in video memory (`0x8001BED4`: four strips of 16 by 64 from x
/// 576 on, at y 256), and its width and height.
pub const AT: (usize, usize) = (576, 256);
pub const SIDE: usize = 64;

/// Bit 1 of the mode word a record hands the decoder (`0x800618F0`): every pixel that
/// comes out carries the blend mark. Bit 0 would ask for the 24-bit depth, which no
/// record does.
pub const MODE_MARKED: u32 = 2;

/// A picture of a clip that cannot be shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipError {
    Bitstream(BitstreamError),
    Picture(PictureError),
}

impl std::fmt::Display for ClipError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bitstream(err) => write!(f, "{err}"),
            Self::Picture(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for ClipError {}

/// The 16-bit word the decoder gives for a pixel in its 16-bit depth: the upper five bits
/// of red, green and blue from the low bits up, and the blend mark when the mode asks
/// for it.
pub fn word([red, green, blue]: [u8; 3], mode: u32) -> u16 {
    let five = |value: u8| u16::from(value >> 3);
    let mark = match mode & MODE_MARKED {
        0 => 0,
        _ => crate::vram::BLEND_MARK,
    };
    five(red) | five(green) << 5 | five(blue) << 10 | mark
}

/// One picture of a clip as the [`SIDE`] by [`SIDE`] words that are sent to [`AT`], row
/// by row from the top.
pub fn words(
    picture: &[u8],
    mode: u32,
    table: &CodeTable,
    decoder: &PictureDecoder,
) -> Result<Vec<u16>, ClipError> {
    let codes = table.expand(picture).map_err(ClipError::Bitstream)?;
    let decoded = (decoder.decode(&codes, SIDE, SIDE)).map_err(ClipError::Picture)?;
    Ok((0..SIDE * SIDE)
        .map(|at| word(decoded.pixel(at % SIDE, at / SIDE), mode))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pixel_keeps_its_upper_five_bits_and_the_mode_marks_it() {
        assert_eq!(word([0xFF, 0, 0], 0), 0x001F);
        assert_eq!(word([0, 0xFF, 0], 0), 0x03E0);
        assert_eq!(word([0, 0, 0xFF], 0), 0x7C00);
        assert_eq!(word([7, 7, 7], 0), 0);
        // The mode of every record: black is a marked black, not the colour that is not
        // drawn.
        assert_eq!(word([0, 0, 0], MODE_MARKED), 0x8000);
        assert_eq!(
            word([8, 16, 24], MODE_MARKED),
            0x8000 | 1 | 2 << 5 | 3 << 10
        );
    }

    #[test]
    fn a_picture_without_a_header_is_an_error() {
        let table = CodeTable::from_words(vec![0; crate::movie::CODE_TABLE_WORDS]);
        let table = table.expect("a table of the right size");
        let decoder = PictureDecoder {
            brightness: [1; 64],
            colour: [1; 64],
            transform: [0; 64],
        };
        assert_eq!(
            words(&[0, 0], MODE_MARKED, &table, &decoder),
            Err(ClipError::Bitstream(BitstreamError::NoHeader))
        );
    }
}
