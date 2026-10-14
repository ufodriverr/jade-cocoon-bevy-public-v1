
/// The mask bit of a pixel of video memory. The 15 bits under it are the colour, five bits
/// each for red, green and blue from the low end.
pub const MARK: u16 = 0x8000;

/// The pixel behind one frame with the picture before: `before` is the pixel of the frame
/// before as it stands in video memory, `now` what the new frame has drawn there (its own
/// drawing never sets the mark).
pub fn laid(before: u16, now: u16) -> u16 {
    match before {
        // A texel of all zeroes is left out.
        0 => now & !MARK,
        // A marked texel is mixed half and half with what lies under it.
        before if before & MARK != 0 => MARK | halves(before, now),
        // Any other is copied as it is, and the mask bit is on for all that is drawn.
        before => MARK | before,
    }
}

/// Half the sum of two colours, each of the three parts by itself, the fraction dropped.
fn halves(one: u16, other: u16) -> u16 {
    (0..3).fold(0, |mixed, part| {
        let part_of = |colour: u16| (colour >> (5 * part)) & 0x1F;
        mixed | ((part_of(one) + part_of(other)) / 2) << (5 * part)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Frames of one pixel: what stood there before the first, then what each frame draws.
    fn frames(mut pixel: u16, drawn: &[u16]) -> Vec<u16> {
        (drawn.iter())
            .map(|now| {
                pixel = laid(pixel, *now);
                pixel
            })
            .collect()
    }

    #[test]
    fn a_thing_that_stands_looks_as_ever() {
        let colour = 0x1234;
        assert_eq!(
            frames(0, &[colour; 4]),
            [colour, MARK | colour, MARK | colour, MARK | colour]
        );
    }

    #[test]
    fn a_thing_that_leaves_stays_a_frame_and_fades_by_halves() {
        // Full red, drawn once; then nothing is drawn there.
        let seen = frames(0, &[31, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(
            seen,
            [
                31,
                MARK | 31,
                MARK | 15,
                MARK | 7,
                MARK | 3,
                MARK | 1,
                MARK,
                MARK
            ]
        );
    }

    #[test]
    fn a_marked_pixel_lets_a_new_thing_in_by_halves() {
        // The mark stays on a pixel that went black, so what comes there fades in.
        let seen = frames(MARK, &[30 << 5; 4]);
        let green = |pixel: u16| (pixel >> 5) & 0x1F;
        assert!(seen.iter().all(|pixel| pixel & MARK != 0));
        assert_eq!(
            seen.iter().map(|p| green(*p)).collect::<Vec<_>>(),
            [15, 22, 26, 28]
        );
    }

    #[test]
    fn the_front_edge_of_a_thing_shows_the_frame_before() {
        // Not black and not marked: the frame before wins over what is drawn now.
        assert_eq!(laid(0x0421, 0x7FFF), MARK | 0x0421);
    }

    #[test]
    fn the_parts_of_a_colour_do_not_carry_into_one_another() {
        assert_eq!(laid(MARK | 0x7FFF, 0x7FFF), MARK | 0x7FFF);
        assert_eq!(laid(MARK | 0x001F, 0x03E0), MARK | 0x01EF);
    }
}
