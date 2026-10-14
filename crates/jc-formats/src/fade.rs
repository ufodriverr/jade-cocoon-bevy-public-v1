
use crate::light::{BoneLight, ONE};

/// The bits of the fade record that are in code: the brightness rises, or falls.
pub mod bit {
    pub const RISES: u16 = 1;
    pub const FALLS: u16 = 2;
    /// A white rectangle is added to the picture, brighter with every step, and stays.
    pub const WHITE_STAYS: u16 = 4;
    /// The white rectangle goes, darker with every step.
    pub const WHITE_GOES: u16 = 8;
    /// The bits that keep the record from being at rest whatever its counter says.
    pub const BUSY: u16 = 0x1B;
}

/// The screen brightness `0x8008D39C`, bit `0x400` of the engine's state word
/// `0x8008D608` and the fade record that the task `0x8008D5B8` steps once a game frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fade {
    /// 4096 is the picture as it is, 0 black.
    pub brightness: i32,
    /// Bit `0x400`: the lights of the models and their share of black follow the
    /// brightness.
    pub dims: bool,
    /// The record: its bits at +0, its counter at +2 and its length in game frames at +6.
    pub bits: u16,
    pub counter: i16,
    pub length: u16,
    /// What the white rectangles of this game frame add to every colour of the picture,
    /// 255 leaving it white. Whoever steps the record clears it when a game frame starts.
    pub white: u8,
}

impl Default for Fade {
    fn default() -> Self {
        Self {
            brightness: ONE,
            dims: false,
            bits: 0,
            counter: 0,
            length: 0,
            white: 0,
        }
    }
}

impl Fade {
    /// `0x8001A9A4`: the picture at full brightness at once, the record at rest.
    pub fn bright(&mut self) {
        (self.brightness, self.dims) = (ONE, false);
        self.counter = 0;
        self.bits &= 0xFFF0;
    }

    /// `0x8001A9E4`: the picture black at once. The record is left as it is.
    pub fn black(&mut self) {
        (self.brightness, self.dims) = (0, true);
    }

    /// `0x8001AA04`: the brightness rises to full over `frames` game frames. The routine
    /// makes the first step itself.
    pub fn fade_in(&mut self, frames: u16) {
        (self.length, self.counter) = (frames, 0);
        self.bits |= bit::RISES;
        self.step();
    }

    /// `0x8001AA50`: the brightness falls to black over `frames` game frames, the models
    /// following it. The routine makes the first step itself.
    pub fn fade_out(&mut self, frames: u16) {
        self.dims = true;
        (self.length, self.counter) = (frames, 0);
        self.bits |= bit::FALLS;
        self.step();
    }

    /// `0x8001AAA8`: a white rectangle is added to the picture, from nothing to white
    /// over `frames` game frames, and stays. The routine makes the first step itself.
    pub fn whiten(&mut self, frames: u16) {
        (self.length, self.counter) = (frames, 0);
        self.bits |= bit::WHITE_STAYS;
        self.step();
    }

    /// `0x8001AAF4` alone: the white that stays is ended at once. The counter and the
    /// length are left as they are.
    pub fn white_ends(&mut self) {
        self.bits &= !bit::WHITE_STAYS;
    }

    /// `0x8001AAF4`, then `0x8001AB1C`: the white that stays is ended, and a white
    /// rectangle goes from white to nothing over `frames` game frames. The second routine
    /// makes the first step itself.
    pub fn unwhiten(&mut self, frames: u16) {
        self.white_ends();
        (self.length, self.counter) = (frames, 0);
        self.bits |= bit::WHITE_GOES;
        self.step();
    }

    /// The colour of a white rectangle at `count` of the record's length. The console's
    /// division by nothing leaves -1, whose low byte is 255.
    fn rectangle(&mut self, count: i32) {
        let colour = (count * 255).checked_div(i32::from(self.length));
        self.white = (self.white).saturating_add(colour.unwrap_or(-1) as u8);
    }

    /// One step of the record (`0x8001A41C`): the first of its bits that is set, in the
    /// order rising, falling, white that stays, white that goes. A step of the brightness
    /// is 4096 divided by the length, the rest dropped, so a fade over 30 game frames
    /// makes 31 steps; the last one sets the end value and clears the bit, and a rise
    /// that is through clears bit `0x400` too. A white rectangle has the colour of the
    /// counter before the step; the counter stops at the length, where the white that
    /// stays goes on being drawn and the white that goes ends.
    pub fn step(&mut self) {
        // The console's division by nothing leaves -1.
        let by = ONE.checked_div(i32::from(self.length)).unwrap_or(-1);
        if self.bits & bit::RISES != 0 {
            self.brightness += by;
            if self.brightness >= ONE {
                self.brightness = ONE;
                self.dims = false;
                self.bits &= !bit::RISES;
            }
        } else if self.bits & bit::FALLS != 0 {
            self.brightness -= by;
            if self.brightness < 0 {
                self.brightness = 0;
                self.bits &= !bit::FALLS;
            }
        } else if self.bits & bit::WHITE_STAYS != 0 {
            self.rectangle(i32::from(self.counter));
            self.counter = self.counter.wrapping_add(1);
            if i32::from(self.length) < i32::from(self.counter) {
                self.counter = self.length as i16;
            }
        } else if self.bits & bit::WHITE_GOES != 0 {
            self.rectangle(i32::from(self.length) - i32::from(self.counter));
            self.counter = self.counter.wrapping_add(1);
            if i32::from(self.length) < i32::from(self.counter) {
                self.bits &= !bit::WHITE_GOES;
                self.counter = self.length as i16;
            }
        }
    }

    /// `0x8001AB94`: the record is at rest, which the script command `0x67` with 6 waits
    /// for. A white rectangle that stays is at rest when it has grown to its length.
    pub fn at_rest(&self) -> bool {
        if self.bits & bit::BUSY != 0 {
            return false;
        }
        self.bits & bit::WHITE_STAYS == 0 || self.counter as u16 == self.length
    }

    pub fn tile(&self) -> u8 {
        ((self.brightness << 7) >> 12) as u8
    }

    pub fn lights(&self, mut light: BoneLight) -> BoneLight {
        if self.dims {
            let dim = |colour: [i32; 3]| colour.map(|part| (part * self.brightness) >> 12);
            light.colours[0] = dim(light.colours[0]);
            light.colours[1] = dim(light.colours[1]);
            light.ambient = dim(light.ambient);
            if let Some((colour, share)) = light.movable {
                let share = (share * self.brightness) >> 12;
                light.colours[2] = colour.map(|part| (part * share) >> 12);
            }
        }
        light
    }

    pub fn share(&self, share: i32) -> i32 {
        match self.dims {
            true => ONE - (((ONE - share) * self.brightness) >> 12),
            false => share,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One game frame of the record's task: the rectangles of the frame before are gone.
    fn game_frame(fade: &mut Fade) -> u8 {
        fade.white = 0;
        fade.step();
        fade.white
    }

    #[test]
    fn a_white_that_stays_grows_to_white_and_is_at_rest_at_its_length() {
        let mut fade = Fade::default();
        // The setter's own step adds a rectangle of nothing.
        fade.whiten(30);
        assert_eq!((fade.white, fade.counter, fade.at_rest()), (0, 1, false));
        let seen: Vec<u8> = (0..31).map(|_| game_frame(&mut fade)).collect();
        assert_eq!(seen[..3], [8, 17, 25]);
        // At rest behind the step that brings the counter to the length; the white is
        // whole one step later and stays.
        assert_eq!((seen[28], seen[29], seen[30]), (246, 255, 255));
        assert!(fade.at_rest() && fade.bits & bit::WHITE_STAYS != 0);
        assert_eq!(fade.brightness, ONE);
    }

    #[test]
    fn a_white_that_is_ended_is_gone_with_the_next_game_frame() {
        let mut fade = Fade::default();
        fade.whiten(2);
        let seen: Vec<u8> = (0..3).map(|_| game_frame(&mut fade)).collect();
        assert_eq!((seen[2], fade.at_rest()), (255, true));
        fade.white_ends();
        assert_eq!((fade.bits, fade.counter, fade.length), (0, 2, 2));
        assert_eq!((game_frame(&mut fade), fade.at_rest()), (0, true));
    }

    #[test]
    fn a_white_that_goes_starts_white_and_ends_with_its_bit() {
        let mut fade = Fade::default();
        fade.whiten(1);
        fade.unwhiten(30);
        // Ended by the first routine, begun by the second, whose step adds a whole white.
        assert_eq!(
            (fade.bits, fade.white, fade.at_rest()),
            (bit::WHITE_GOES, 255, false)
        );
        let seen: Vec<u8> = (0..30).map(|_| game_frame(&mut fade)).collect();
        assert_eq!((seen[0], seen[1], seen[28], seen[29]), (246, 238, 8, 0));
        assert!(fade.at_rest() && fade.bits == 0);
        assert_eq!(game_frame(&mut fade), 0);
    }

    #[test]
    fn the_rectangles_of_one_game_frame_add_up_to_white_at_most() {
        let mut fade = Fade::default();
        fade.unwhiten(2);
        fade.step();
        assert_eq!(fade.white, 255);
        // A length of nothing: the console's division leaves a byte of 255.
        let mut fade = Fade::default();
        fade.whiten(0);
        assert_eq!(fade.white, 255);
    }

    #[test]
    fn a_fade_in_over_thirty_game_frames_makes_thirty_one_steps() {
        let mut fade = Fade::default();
        fade.black();
        assert!(fade.at_rest() && fade.dims);
        fade.fade_in(30);
        assert_eq!((fade.brightness, fade.at_rest()), (136, false));
        let mut steps = 1;
        while !fade.at_rest() {
            fade.step();
            steps += 1;
        }
        assert_eq!((steps, fade.brightness, fade.dims), (31, ONE, false));
        // A step at rest changes nothing.
        fade.step();
        assert_eq!(fade.brightness, ONE);
    }

    #[test]
    fn a_fade_out_ends_black_with_the_models_dimmed() {
        let mut fade = Fade::default();
        fade.fade_out(30);
        assert_eq!((fade.brightness, fade.dims), (ONE - 136, true));
        let mut steps = 1;
        while !fade.at_rest() {
            fade.step();
            steps += 1;
        }
        assert_eq!((steps, fade.brightness, fade.dims), (31, 0, true));
        assert_eq!(fade.tile(), 0);
        fade.bright();
        assert_eq!((fade.brightness, fade.dims, fade.tile()), (ONE, false, 128));
    }

    #[test]
    fn the_brightness_dims_two_lights_the_ambient_colour_and_the_share() {
        let light = BoneLight {
            rows: [[0; 3]; 3],
            colours: [[4096; 3], [2048; 3], [1024; 3]],
            ambient: [800; 3],
            movable: None,
        };
        let mut fade = Fade {
            brightness: 2048,
            ..Fade::default()
        };
        // Without bit 0x400 nothing follows the brightness.
        assert_eq!(fade.lights(light), light);
        assert_eq!(fade.share(1024), 1024);
        fade.dims = true;
        let dimmed = fade.lights(light);
        assert_eq!(dimmed.colours, [[2048; 3], [1024; 3], [1024; 3]]);
        assert_eq!(dimmed.ambient, [400; 3]);
        assert_eq!(fade.share(0), 2048);
        assert_eq!(fade.share(2048), 3072);
        assert_eq!(fade.tile(), 64);
        // The movable light in the third light's place is dimmed by its share.
        let lit = BoneLight {
            movable: Some(([7424, 7424, 4096], 2730)),
            ..light
        };
        assert_eq!(fade.lights(lit).colours[2], [2474, 2474, 1365]);
    }
}
