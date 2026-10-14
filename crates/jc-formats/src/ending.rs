
use crate::scene::ScenePackage;

/// The archive file of the last picture, a texture set of two images of 160 by 240.
pub const PICTURES: usize = 58;
/// The two halves of the picture: the image of the set and its place on the screen.
pub const HALVES: [(usize, i16, i16); 2] = [(0, 0, 0), (1, 160, 0)];
/// The game frames over which the picture comes out of the black (`0x8001AA04(120)`).
pub const RISE: i32 = 120;
/// The brightness of a picture as it is, and the colour byte that draws a sprite as it
/// is.
const FULL: i32 = 4096;
pub const AS_IT_IS: u8 = 128;

/// The words of the routine that asks for the save: 1, 0, a text and 1 go to the four
/// words at `0x800B088C`, which the field's task reads for the memory card's screen.
/// `None` stands for the low half of the text's address.
const ASKS_SAVE: [Option<u32>; 11] = [
    Some(0x3C02_800B),
    Some(0x2404_0001),
    Some(0xAC44_088C),
    Some(0x2442_088C),
    Some(0x3C03_800B),
    None,
    Some(0xAC40_0004),
    Some(0xAC43_0008),
    Some(0xAC44_000C),
    Some(0x03E0_0008),
    Some(0x0080_1021),
];
/// The calls of the routine that reads the picture, by their place in it: the blocking
/// read, the texture set loader, the black, the rise with its length, the background
/// left out and the actors hidden.
const READS: [(usize, u32); 7] = [
    (3, 0x0C01_0751),
    (14, 0x0C00_A3B7),
    (18, 0x0C00_6A79),
    (20, 0x0C00_6A81),
    (21, 0x2404_0000 | RISE as u32),
    (22, 0x0C02_823F),
    (24, 0x0C02_9EA4),
];
/// Words of the routine that draws the two halves, by their place in it: the shift of
/// the brightness, the store of the colour, the two additions to the frame's table and
/// the answer 0.
const DRAWS: [(usize, u32); 6] = [
    (0, 0x27BD_FFD8),
    (16, 0x2406_0005),
    (23, 0xA062_0004),
    (46, 0x0C00_FC0C),
    (55, 0x0C00_FC0C),
    (59, 0x0000_1021),
];

fn words(package: &ScenePackage, routine: u32, count: usize) -> Option<Vec<u32>> {
    let code = package.bytes_at(routine, 4 * count)?;
    Some(
        (code.as_chunks::<4>().0.iter())
            .map(|word| u32::from_le_bytes(*word))
            .collect(),
    )
}

/// The address of the text a native routine of the package hands on, when it is the
/// one that asks for the save behind the end of the story.
pub fn asks_save(package: &ScenePackage, routine: u32) -> Option<u32> {
    let words = words(package, routine, ASKS_SAVE.len())?;
    let mut text = None;
    for (shape, word) in ASKS_SAVE.into_iter().zip(words) {
        match shape {
            Some(shape) if shape == word => {}
            // `addiu v1,v1,N` behind `lui v1,0x800B`.
            None if word >> 16 == 0x2463 => {
                text = Some(0x800B_0000_u32.wrapping_add_signed(i32::from(word as u16 as i16)));
            }
            _ => return None,
        }
    }
    text
}

/// The archive file a native routine of the package reads, when it is the one that
/// reads the last picture.
pub fn reads(package: &ScenePackage, routine: u32) -> Option<usize> {
    let words = words(package, routine, 26)?;
    let file = words[1];
    (file >> 16 == 0x2404 && READS.iter().all(|&(at, word)| words[at] == word))
        .then_some((file & 0xFFFF) as usize)
}

/// Whether a native routine of the package is the one that draws the two halves of the
/// last picture. It answers 0, so a script stays on its call.
pub fn draws(package: &ScenePackage, routine: u32) -> bool {
    words(package, routine, 60)
        .is_some_and(|words| DRAWS.iter().all(|&(at, word)| words[at] == word))
}

/// The colour byte of both halves, `passed` game frames behind the routine that read
/// the picture: the brightness shifted down by five. The brightness is 0 at the read
/// and rises by 4096 over the length in every game frame, the first time at once.
pub fn colour(passed: u32) -> u8 {
    let steps = i32::try_from(passed).unwrap_or(i32::MAX).saturating_add(1);
    let brightness = steps.saturating_mul(FULL / RISE).min(FULL);
    (brightness >> 5) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_picture_rises_out_of_the_black_and_stays() {
        assert_eq!(colour(0), 1);
        assert_eq!(colour(59), 63);
        assert_eq!(colour(119), 127);
        assert_eq!(colour(120), AS_IT_IS);
        assert_eq!(colour(100_000), AS_IT_IS);
    }
}
