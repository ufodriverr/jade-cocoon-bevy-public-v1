
use crate::background::SCREEN;

/// The record `0x8008D3B0`: four 16-bit values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrawArea {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Default for DrawArea {
    fn default() -> Self {
        Self::WHOLE
    }
}

impl DrawArea {
    /// The whole screen: what a room starts with.
    pub const WHOLE: Self = Self {
        x: 0,
        y: 0,
        width: SCREEN.0,
        height: SCREEN.1,
    };
    /// The picture between two bars: 160 lines from line 32.
    pub const BARS: Self = Self {
        x: 0,
        y: 32,
        width: 320,
        height: 160,
    };

    /// The record as a script command names it by address: x, y, width, height.
    pub fn at(bytes: &[u8]) -> Option<Self> {
        let value = |n: usize| {
            let pair = bytes.get(2 * n..2 * n + 2)?;
            Some(i32::from(i16::from_le_bytes([pair[0], pair[1]])))
        };
        Some(Self {
            x: value(0)?,
            y: value(1)?,
            width: value(2)?,
            height: value(3)?,
        })
    }

    /// The middle of the area on the screen.
    pub fn centre(&self) -> (i32, i32) {
        (self.x + self.width / 2, self.y + self.height / 2)
    }

    /// How far the middle of the screen lies right of and below the middle of the area.
    pub fn shift(&self) -> (i32, i32) {
        let (x, y) = self.centre();
        (SCREEN.0 / 2 - x, SCREEN.1 / 2 - y)
    }

    /// The second step of the scroll routine: an offset kept within half of what layer 0
    /// (`layer`: its size in pixels at the zoom in use) is larger than the area.
    pub fn limited(&self, wanted: (i32, i32), layer: (i32, i32)) -> (i32, i32) {
        let limited = |wanted: i32, larger: i32| {
            let (low, high) = (-larger / 2, larger / 2);
            match wanted < low {
                true => low,
                false => wanted.min(high),
            }
        };
        (
            limited(wanted.0, layer.0 - self.width),
            limited(wanted.1, layer.1 - self.height),
        )
    }

    /// One call of the scroll routine `0x800398B8`: the scroll offset behind it, from the
    /// offset `stored` before it and the followed point as it was projected to the screen
    /// with that offset. The point is brought to the middle of the area, the offset is
    /// limited, and then moved by [`Self::shift`] the other way.
    pub fn scroll(&self, stored: (i32, i32), point: (i32, i32), layer: (i32, i32)) -> (i32, i32) {
        let (centre, shift) = (self.centre(), self.shift());
        let wanted = (
            stored.0 - (point.0 - centre.0),
            stored.1 - (point.1 - centre.1),
        );
        let limited = self.limited(wanted, layer);
        (limited.0 - shift.0, limited.1 - shift.1)
    }

    /// The parts of the screen outside the area, each as x, y, width and height: above,
    /// below, left and right of it. A part can be empty.
    pub fn outside(&self) -> [(i32, i32, i32, i32); 4] {
        let (right, below) = (self.x + self.width, self.y + self.height);
        [
            (0, 0, SCREEN.0, self.y),
            (0, below, SCREEN.0, SCREEN.1 - below),
            (0, self.y, self.x, self.height),
            (right, self.y, SCREEN.0 - right, self.height),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A layer of 320 by 448 pixels, the followed point 100 lines below the middle of the
    /// picture when nothing is scrolled.
    const LAYER: (i32, i32) = (320, 448);

    #[test]
    fn the_whole_screen_keeps_the_point_in_the_middle() {
        let area = DrawArea::WHOLE;
        assert_eq!(area.centre(), (160, 120));
        assert_eq!(area.shift(), (0, 0));
        // Unscrolled the point is seen at (170, 220); the layer scrolls 104 lines either way.
        assert_eq!(area.scroll((0, 0), (170, 220), LAYER), (0, -100));
        // Projected with that offset it stands in the middle, and the offset stays.
        assert_eq!(area.scroll((0, -100), (170, 120), LAYER), (0, -100));
        assert_eq!(area.scroll((0, 0), (160, 400), LAYER), (0, -104));
        assert_eq!(area.scroll((0, 0), (160, -400), LAYER), (0, 104));
    }

    #[test]
    fn the_bars_move_the_picture_up_twice() {
        let area = DrawArea::BARS;
        assert_eq!(area.centre(), (160, 112));
        assert_eq!(area.shift(), (0, 8));
        // The same point: 8 lines for the middle of the area and 8 for the shift.
        assert_eq!(area.scroll((0, 0), (160, 220), LAYER), (0, -116));
        // With that offset the point is seen on line 104, and the offset stays.
        assert_eq!(area.scroll((0, -116), (160, 104), LAYER), (0, -116));
        // The layer is 288 lines higher than the area: 144 either way, then the shift.
        assert_eq!(area.scroll((0, 0), (160, 600), LAYER), (0, -152));
        assert_eq!(area.scroll((0, 0), (160, -600), LAYER), (0, 136));
    }

    #[test]
    fn a_layer_smaller_than_the_area_takes_the_low_limit() {
        let area = DrawArea::WHOLE;
        assert_eq!(area.limited((0, 0), (288, 240)), (16, 0));
        assert_eq!(area.limited((40, -3), (288, 240)), (-16, 0));
    }

    #[test]
    fn a_record_reads_as_four_values() {
        let bytes = [16, 0, 32, 0, 0x20, 1, 160, 0];
        let area = DrawArea::at(&bytes).expect("eight bytes");
        assert_eq!(
            (area.x, area.y, area.width, area.height),
            (16, 32, 288, 160)
        );
        assert_eq!(DrawArea::at(&bytes[..7]), None);
        assert_eq!(
            area.outside(),
            [
                (0, 0, 320, 32),
                (0, 192, 320, 48),
                (0, 32, 16, 160),
                (304, 32, 16, 160)
            ]
        );
        assert_eq!(
            DrawArea::WHOLE.outside().map(|part| part.2 * part.3),
            [0; 4]
        );
    }
}
