
/// The share that puts the picture on the target alone.
pub const ALL: i32 = 4096;
/// How far above a point of the room the camera's routines look (the room's y points down).
pub const RAISED: i32 = 350;

/// The glide's part of the camera record: bit `0x10` of its flags (+4), the share (+36),
/// the length in game frames (+38) and the target (+40).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glide {
    /// The share rises towards the target; otherwise it falls back.
    pub there: bool,
    pub share: i16,
    pub length: i16,
    pub target: [i32; 3],
}

impl Default for Glide {
    /// The record as the camera task starts it (`0x800376E0`).
    fn default() -> Self {
        Self {
            there: false,
            share: 0,
            length: ALL as i16,
            target: [0; 3],
        }
    }
}

impl Glide {
    /// `0x80037A08`: the camera glides to a point over a number of game frames.
    pub fn to(&mut self, target: [i32; 3], frames: i16) {
        self.target = target;
        self.length = frames;
        self.there = true;
    }

    /// `0x80037A4C`: the camera glides back over a number of game frames.
    pub fn back(&mut self, frames: i16) {
        self.length = frames;
        self.there = false;
    }

    /// `0x80037944`, once a game frame: the share the frame's picture is made with; the
    /// share then moves by 4096 over the length, up to 4096 or down to 0.
    pub fn step(&mut self) -> i32 {
        let share = i32::from(self.share);
        // The console's division by zero answers minus one here.
        let by = match self.length {
            0 => -1,
            length => ALL / i32::from(length),
        };
        self.share = match self.there {
            true => match (share + by) as i16 {
                above if i32::from(above) > ALL => ALL as i16,
                moved => moved,
            },
            false => ((share - by) as i16).max(0),
        };
        share
    }

    /// What script command `0x5D` waits for: the share all the way there, or all the way
    /// back.
    pub fn arrived(&self, there: bool) -> bool {
        match there {
            true => i32::from(self.share) == ALL,
            false => self.share == 0,
        }
    }
}

/// The zoom that leaves the picture unscaled, and the largest one the step allows.
pub const ZOOM_ONE: i32 = 4096;
pub const ZOOM_MOST: i32 = 0x10000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Zoom {
    pub value: i32,
    /// A zoom over several game frames runs.
    pub runs: bool,
    from: u16,
    frames: u16,
    target: u32,
}

impl Default for Zoom {
    /// The zoom as the camera task starts it (`0x800376E0`).
    fn default() -> Self {
        Self {
            value: ZOOM_ONE,
            runs: false,
            from: 0,
            frames: 0,
            target: 0,
        }
    }
}

impl Zoom {
    /// Script command `0x5F` (`0x80037A98`): with a count below 2 the zoom is the value
    /// at once, which the caller answers with a step of the camera task; otherwise it
    /// runs there over that many game frames. True when it was set at once.
    pub fn order(&mut self, value: i32, frames: i32) -> bool {
        if frames < 2 {
            self.value = value;
            return true;
        }
        self.target = value as u32;
        self.frames = frames as u16;
        self.from = self.value as u16;
        self.runs = true;
        false
    }

    /// The zoom's step (`0x80038570`), which both callers of the scroll routine make
    /// first. `distance` is the projection distance of the view's header and `depth` what
    /// the projection of the point the picture is kept on answered the last time (the
    /// word at `0x8008D5FC`; HYPOTHESIS: a quarter of the point's depth, as the
    /// library's projection answers it).
    ///
    /// A run multiplies the zoom by the same factor every time and drops the fraction;
    /// it is over once the zoom has reached or passed the target, and the zoom stays what
    /// it is then. HYPOTHESIS: the console's floating point routines give what these
    /// operations give, a single for the quotients and a double for the power.
    pub fn step(&mut self, distance: i16, depth: i32) {
        let target = self.target;
        let value = self.value as u32;
        if self.runs && target != value {
            let ratio = target as f32 / f32::from(self.from);
            let part = 1.0f32 / f32::from(self.frames);
            let factor = f64::from(ratio).powf(f64::from(part));
            let next = (f64::from(self.value as f32) * factor) as f32 as i32;
            self.value = next;
            self.runs = match target < value {
                true => target < next as u32,
                false => (next as u32) < target,
            };
        }
        if self.value < ZOOM_ONE {
            self.value = ZOOM_ONE;
            self.runs = false;
        }
        if self.value > ZOOM_MOST {
            self.value = ZOOM_MOST;
            self.runs = false;
        }
        // The projection distance in use is never more than four times the answer.
        let distance = i32::from(distance);
        if distance != 0 && depth.wrapping_mul(4) < distance.wrapping_mul(self.value) / ALL {
            self.value = (depth.wrapping_shl(14) / distance).max(ZOOM_ONE);
            self.runs = false;
        }
    }
}

/// The point the scroll routine is given (`0x80038A60`): between where the followed point
/// and the target are seen on the screen, by the share.
pub fn mixed(followed: (i32, i32), target: (i32, i32), share: i32) -> (i32, i32) {
    let mix = |from: i32, to: i32| from + (to - from) * share / ALL;
    (mix(followed.0, target.0), mix(followed.1, target.1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zoom_with_a_count_below_two_is_there_at_once() {
        let mut zoom = Zoom::default();
        assert!(zoom.order(6144, 1));
        assert_eq!((zoom.value, zoom.runs), (6144, false));
        // The step leaves it alone while the point is far enough away.
        zoom.step(320, 1000);
        assert_eq!((zoom.value, zoom.runs), (6144, false));
    }

    #[test]
    fn a_zoom_over_game_frames_grows_by_a_factor_and_ends_past_the_target() {
        let mut zoom = Zoom::default();
        assert!(!zoom.order(8192, 2));
        assert!(zoom.runs);
        // The square root of two, the fraction dropped.
        zoom.step(320, 100_000);
        assert_eq!((zoom.value, zoom.runs), (5792, true));
        zoom.step(320, 100_000);
        assert_eq!((zoom.value, zoom.runs), (8191, true));
        // One more step passes the target, and the zoom stays where that took it.
        zoom.step(320, 100_000);
        assert_eq!((zoom.value, zoom.runs), (11583, false));
        zoom.step(320, 100_000);
        assert_eq!(zoom.value, 11583);
    }

    #[test]
    fn a_zoom_back_ends_at_the_first_value_not_above_the_target() {
        let mut zoom = Zoom::default();
        zoom.order(8192, 1);
        zoom.order(4096, 2);
        zoom.step(320, 100_000);
        assert_eq!((zoom.value, zoom.runs), (5792, true));
        zoom.step(320, 100_000);
        assert_eq!((zoom.value, zoom.runs), (4096, false));
    }

    #[test]
    fn the_zoom_keeps_inside_its_limits() {
        let mut zoom = Zoom::default();
        zoom.order(1000, 1);
        zoom.step(320, 100_000);
        assert_eq!(zoom.value, ZOOM_ONE);
        zoom.order(0x20000, 1);
        zoom.step(320, 100_000);
        assert_eq!(zoom.value, ZOOM_MOST);
        // A point whose answer is 200: the distance in use may be 800 at most, and a
        // zoom of 4 on a distance of 320 would make it 1280.
        zoom.order(4 * ZOOM_ONE, 1);
        zoom.step(320, 200);
        assert_eq!(zoom.value, 200 * 16384 / 320);
        // Never below one, however near the point is.
        zoom.step(320, 10);
        assert_eq!(zoom.value, ZOOM_ONE);
    }

    #[test]
    fn a_glide_of_one_frame_is_there_in_the_next() {
        let mut glide = Glide::default();
        assert_eq!(glide.step(), 0);
        assert_eq!(glide.share, 0);
        glide.to([1, 2, 3], 1);
        assert!(!glide.arrived(true));
        assert_eq!(glide.step(), 0);
        assert!(glide.arrived(true));
        assert_eq!(glide.step(), ALL);
        assert_eq!(i32::from(glide.share), ALL);
    }

    #[test]
    fn the_way_back_drops_the_rest_of_the_division() {
        let mut glide = Glide::default();
        glide.to([0; 3], 1);
        glide.step();
        glide.back(120);
        // 34 a game frame: 16 are left after 120 of them.
        let shares: Vec<i32> = (0..120).map(|_| glide.step()).collect();
        assert_eq!((shares[0], shares[1], shares[119]), (4096, 4062, 50));
        assert_eq!(glide.share, 16);
        assert!(!glide.arrived(false));
        assert_eq!(glide.step(), 16);
        assert!(glide.arrived(false));
        assert_eq!(glide.target, [0; 3]);
    }

    #[test]
    fn a_rising_share_stops_at_the_whole() {
        let mut glide = Glide::default();
        glide.to([0; 3], 3);
        let shares: Vec<i32> = (0..5).map(|_| glide.step()).collect();
        assert_eq!(shares, [0, 1365, 2730, 4095, 4096]);
    }

    #[test]
    fn the_point_lies_between_the_two() {
        assert_eq!(mixed((100, 50), (0, 150), 0), (100, 50));
        assert_eq!(mixed((100, 50), (0, 150), ALL), (0, 150));
        assert_eq!(mixed((100, 50), (0, 150), 2048), (50, 100));
        // Halves are dropped towards the followed point.
        assert_eq!(mixed((0, 0), (-3, 3), 2048), (-1, 1));
    }
}
