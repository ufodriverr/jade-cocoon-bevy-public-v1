
use crate::tim::Tim;

/// Steps of the hue circle: red 0, green 64, blue 128.
pub const CIRCLE: i32 = 192;
/// Steps of one sixth of the circle, the stretch between two of the six plain colours.
const SIXTH: i32 = 32;
/// The largest value of a colour component, of the saturation and of the value.
const FULL: i32 = 31;
/// Bit of a colour that survives the turn untouched (the blend mark).
const MARK: u16 = 0x8000;
/// Bit of a turn that sends the game down its second way of counting.
const OTHER_WAY: i32 = 0x1_0000;

/// A colour as the game's conversion holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hsv {
    /// 0 up to [`CIRCLE`], as the conversion gives it; a turn can leave it outside.
    pub hue: i16,
    /// 0 to 31.
    pub saturation: i16,
    /// 0 to 31: the largest of the three components.
    pub value: i16,
}

/// Hue, saturation and value of a 15-bit colour (red in the lowest five bits), or `None`
/// for a colour without a hue: black and every grey. The game leaves such a colour alone.
pub fn to_hsv(colour: u16) -> Option<Hsv> {
    let red = i32::from(colour & 31);
    let green = i32::from(colour >> 5 & 31);
    let blue = i32::from(colour >> 10 & 31);
    // The largest component, blue first: a later one only wins when it is larger.
    let (mut value, mut largest) = (blue, 0);
    if value < red {
        (value, largest) = (red, 1);
    }
    if value < green {
        (value, largest) = (green, 2);
    }
    let span = value - red.min(green).min(blue);
    if value == 0 || span * FULL / value == 0 {
        return None;
    }
    let hue = match largest {
        0 => (red - green) * SIXTH / span + 4 * SIXTH,
        1 => (green - blue) * SIXTH / span,
        _ => (blue - red) * SIXTH / span + 2 * SIXTH,
    };
    let hue = if hue < 0 { hue + CIRCLE } else { hue };
    Some(Hsv {
        hue: hue as i16,
        saturation: (span * FULL / value) as i16,
        value: value as i16,
    })
}

/// The three components (red, green, blue) of a colour given as hue, saturation and value,
/// or `None` when the hue lies outside the circle: the game then writes no components and
/// the colour stays what it was.
pub fn from_hsv(colour: Hsv) -> Option<[u16; 3]> {
    let (hue, saturation, value) = (
        i32::from(colour.hue),
        i32::from(colour.saturation),
        i32::from(colour.value),
    );
    // The sixth of the circle, rounded towards zero, and the way into it.
    let sixth = (if hue < 0 { hue + FULL } else { hue }) >> 5;
    let into = hue - sixth * SIXTH;
    let whole = SIXTH * FULL;
    let part = |taken: i32| ((value * (whole - taken) + whole / 2) / whole) as u16;
    let low = part(saturation * SIXTH);
    let falling = part(saturation * into);
    let rising = part(saturation * (SIXTH - into));
    let value = value as u16;
    match sixth {
        0 => Some([value, rising, low]),
        1 => Some([falling, value, low]),
        2 => Some([low, value, rising]),
        3 => Some([low, falling, value]),
        4 => Some([rising, low, value]),
        5 => Some([value, low, falling]),
        _ => None,
    }
}

/// One colour of a palette after the turn `turn`, an angle in degrees: 360 is the whole
/// circle of [`CIRCLE`] steps. Black and the greys stay, the mark bit stays, and the value
/// of the colour stays; only its hue moves.
///
/// A turn with bit 16 set (every negative one) goes the game's second way, which no model of
/// a room asks for; it is written as the code computes it and is not understood further.
pub fn turn_colour(colour: u16, turn: i32) -> u16 {
    let Some(mut hsv) = to_hsv(colour) else {
        return colour;
    };
    let steps = |degrees: u32| (degrees.wrapping_mul(3) << 6 >> 3) / 45;
    if turn & OTHER_WAY == 0 {
        let hue = (hsv.hue as u16).wrapping_add(steps(turn as u32) as u16) as i16;
        hsv.hue = (i32::from(hue) % CIRCLE) as i16;
    } else {
        let degrees = i32::from(hsv.hue).wrapping_mul(360) / CIRCLE;
        let upper = (turn as u32 >> 16) & 0xFFFE;
        let hue = i32::from(hsv.hue)
            .wrapping_add(turn.wrapping_sub(degrees))
            .wrapping_add((upper * 24 / 45) as i32);
        hsv.hue = hue as i16;
    }
    match from_hsv(hsv) {
        Some([red, green, blue]) => colour & MARK | red | green << 5 | blue << 10,
        None => colour,
    }
}

/// An image as a model loader sends it when the model's colours are turned: the first row of
/// the palette it brings, colour by colour through [`turn_colour`]. A turn of zero, an image
/// without a palette block and one that brings no colours come back unchanged.
pub fn turned(image: &Tim, turn: i32) -> Tim {
    let mut turned = image.clone();
    if turn == 0 {
        return turned;
    }
    if let Some(clut) = &mut turned.clut {
        let row = usize::from(clut.rect.w);
        for colour in clut.colors.iter_mut().take(row) {
            *colour = turn_colour(*colour, turn);
        }
    }
    turned
}

#[cfg(test)]
mod tests {
    use super::*;

    fn colour(red: u16, green: u16, blue: u16) -> u16 {
        red | green << 5 | blue << 10
    }

    #[test]
    fn the_six_plain_colours_lie_a_sixth_apart() {
        let hue = |c: u16| to_hsv(c).map(|hsv| (hsv.hue, hsv.saturation, hsv.value));
        assert_eq!(hue(colour(31, 0, 0)), Some((0, 31, 31)));
        assert_eq!(hue(colour(31, 31, 0)), Some((32, 31, 31)));
        assert_eq!(hue(colour(0, 31, 0)), Some((64, 31, 31)));
        assert_eq!(hue(colour(0, 31, 31)), Some((96, 31, 31)));
        assert_eq!(hue(colour(0, 0, 31)), Some((128, 31, 31)));
        assert_eq!(hue(colour(31, 0, 31)), Some((160, 31, 31)));
        // A hue below red comes out at the end of the circle.
        assert_eq!(hue(colour(20, 0, 10)), Some((176, 31, 20)));
    }

    #[test]
    fn a_colour_without_a_hue_has_none() {
        assert_eq!(to_hsv(0), None);
        assert_eq!(to_hsv(colour(9, 9, 9)), None);
        assert_eq!(to_hsv(colour(31, 31, 31)), None);
    }

    #[test]
    fn a_turn_moves_the_hue_and_keeps_the_value() {
        let red = colour(31, 0, 0);
        assert_eq!(turn_colour(red, 90), colour(16, 31, 0));
        assert_eq!(turn_colour(red, 180), colour(0, 31, 31));
        assert_eq!(turn_colour(red, 270), colour(16, 0, 31));
        assert_eq!(turn_colour(red, 360), red);
        // 8 / 15 of a step to the degree, rounded down: 44 degrees are 23 steps.
        assert_eq!(turn_colour(red, 44), colour(31, 22, 0));
        // A dull colour: the components keep their spread.
        assert_eq!(turn_colour(colour(20, 10, 10), 120), colour(10, 20, 10));
    }

    #[test]
    fn greys_black_and_the_mark_stay() {
        for turn in [90, 180, 270] {
            assert_eq!(turn_colour(0, turn), 0);
            assert_eq!(turn_colour(MARK, turn), MARK);
            assert_eq!(turn_colour(colour(12, 12, 12), turn), colour(12, 12, 12));
        }
        assert_eq!(
            turn_colour(MARK | colour(31, 0, 0), 180),
            MARK | colour(0, 31, 31)
        );
    }

    #[test]
    fn the_second_way_spills_as_the_routine_does() {
        assert_eq!(turn_colour(0x004A, 0x1_0000), 0xFFEA);
        assert_eq!(turn_colour(0x006F, 0x2E_004D), 0xFFAF);
        assert_eq!(turn_colour(colour(31, 0, 0), -1), colour(31, 0, 0));
    }

    #[test]
    fn a_hue_outside_the_circle_leaves_the_colour() {
        let hsv = |hue| Hsv {
            hue,
            saturation: 31,
            value: 31,
        };
        assert_eq!(from_hsv(hsv(192)), None);
        assert_eq!(from_hsv(hsv(-40)), None);
        assert_eq!(from_hsv(hsv(191)), Some([31, 0, 1]));
    }
}
