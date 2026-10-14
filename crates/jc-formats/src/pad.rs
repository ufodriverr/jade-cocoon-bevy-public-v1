
/// The buttons in the game's word: the first button byte of the pad in the upper half,
/// the second in the lower.
pub const L2: u16 = 0x0001;
pub const R2: u16 = 0x0002;
pub const L1: u16 = 0x0004;
pub const R1: u16 = 0x0008;
pub const TRIANGLE: u16 = 0x0010;
pub const CIRCLE: u16 = 0x0020;
pub const CROSS: u16 = 0x0040;
pub const SQUARE: u16 = 0x0080;
pub const SELECT: u16 = 0x0100;
pub const START: u16 = 0x0800;
pub const UP: u16 = 0x1000;
pub const RIGHT: u16 = 0x2000;
pub const DOWN: u16 = 0x4000;
pub const LEFT: u16 = 0x8000;

/// The word of held buttons from the two button bytes of the pad, where a button that is
/// down is a zero bit.
pub fn held(buttons: [u8; 2]) -> u16 {
    !(u16::from(buttons[0]) << 8 | u16::from(buttons[1]))
}

/// How long a button is held before it counts again, and how often it does then.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Repeat {
    /// Calls with the button down before it counts again.
    pub after: u16,
    /// It counts again in every call whose count is a multiple of this.
    pub every: u32,
}

impl Repeat {
    /// The routine's choice by the length of a game frame in pictures of the screen: 40
    /// calls and every second one while a game frame is one picture (the title, the
    /// movies), 20 calls and every one while it is longer (the field).
    pub fn of(game_frame: u32) -> Self {
        match game_frame < 2 {
            true => Self {
                after: 40,
                every: 2,
            },
            false => Self {
                after: 20,
                every: 1,
            },
        }
    }
}

/// The words of one pad.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pad {
    /// The buttons that are down in this call.
    pub held: u16,
    /// The buttons that were down in the call before.
    pub before: u16,
    /// The buttons that went down between the two calls.
    pub pressed: u16,
    /// The buttons that came up between the two calls.
    pub released: u16,
    /// The pressed buttons and the ones held long enough to count again.
    pub repeated: u16,
    /// Calls each button has been down for, the first one not counted.
    counts: [u16; 16],
}

impl Pad {
    /// One call of the routine with a pad in the port: `held` is the word of this call,
    /// `counter` the count of the main loop that paces the repeat.
    pub fn read(&mut self, held: u16, repeat: Repeat, counter: u32) {
        self.before = self.held;
        self.held = held;
        let changed = self.held ^ self.before;
        self.pressed = changed & self.held;
        self.released = changed & !self.held;
        self.repeated = self.pressed;
        let paced = counter & (repeat.every - 1) == 0;
        for (button, count) in self.counts.iter_mut().enumerate() {
            if self.held & self.before & (1 << button) == 0 {
                *count = 0;
                continue;
            }
            *count = count.wrapping_add(1);
            if *count >= repeat.after && paced {
                self.repeated |= 1 << button;
            }
        }
    }

    /// One call of the routine without a pad in the port: every word is cleared.
    pub fn absent(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_button_that_is_down_is_a_zero_bit_of_its_byte() {
        assert_eq!(held([0xFF, 0xFF]), 0);
        // Start is bit 3 of the first byte, the cross bit 6 of the second.
        assert_eq!(held([0xF7, 0xFF]), START);
        assert_eq!(held([0xFF, 0xBF]), CROSS);
        assert_eq!(held([0x6F, 0xFE]), LEFT | UP | L2);
    }

    #[test]
    fn a_button_is_pressed_in_the_call_it_went_down_before() {
        let mut pad = Pad::default();
        let repeat = Repeat::of(1);
        pad.read(START, repeat, 0);
        assert_eq!((pad.pressed, pad.released, pad.repeated), (START, 0, START));
        pad.read(START | CROSS, repeat, 1);
        assert_eq!((pad.pressed, pad.released), (CROSS, 0));
        assert_eq!((pad.held, pad.before), (START | CROSS, START));
        pad.read(CROSS, repeat, 2);
        assert_eq!((pad.pressed, pad.released), (0, START));
        pad.absent();
        assert_eq!(pad, Pad::default());
        // A button that was down before the pad came back is pressed again.
        pad.read(CROSS, repeat, 3);
        assert_eq!(pad.pressed, CROSS);
    }

    #[test]
    fn a_held_button_counts_again_after_its_wait() {
        // In the movies and the title: 40 calls after the one that pressed it, then in
        // every call with an even count.
        let mut pad = Pad::default();
        let repeat = Repeat::of(1);
        assert_eq!((repeat.after, repeat.every), (40, 2));
        pad.read(DOWN, repeat, 0);
        let mut again = Vec::new();
        for call in 1..=44 {
            pad.read(DOWN, repeat, call);
            assert_eq!(pad.pressed, 0);
            if pad.repeated == DOWN {
                again.push(call);
            }
        }
        assert_eq!(again, [40, 42, 44]);
        // Letting go starts the wait again.
        pad.read(0, repeat, 45);
        pad.read(DOWN, repeat, 46);
        pad.read(DOWN, repeat, 48);
        assert_eq!(pad.repeated, 0);

        // In the field: 20 calls, then every call.
        let mut pad = Pad::default();
        let repeat = Repeat::of(2);
        pad.read(UP, repeat, 0);
        let again: Vec<u32> = (1..=22)
            .filter(|&call| {
                pad.read(UP, repeat, call);
                pad.repeated == UP
            })
            .collect();
        assert_eq!(again, [20, 21, 22]);
    }
}
