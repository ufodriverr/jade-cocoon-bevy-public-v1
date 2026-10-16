
use std::collections::HashMap;

use bevy::math::DVec3;
use bevy::prelude::*;

/// The furthest, in pixels of the console's screen, a corner may move in one game frame
/// and still glide; further is a warp and snaps.
pub const JUMP: f32 = 64.0;
/// A camera that turns further than this in one game frame, in degrees, has cut.
pub const TURN: f64 = 25.0;
/// A camera that moves, or whose point looked at moves, by more than this share of the
/// distance between the two in one game frame has cut.
pub const MOVE: f64 = 0.4;
/// A projection distance that grows or shrinks by more than this factor has cut.
pub const DISTANCE: f64 = 1.25;
/// The most game frames between two drawings that still glide.
pub const MOST_TICKS: u32 = 8;

/// Which triangle a game frame drew: its thing and its face. A face's number counts the
/// faces of the thing that were looked at, drawn or not, two to a quad (its halves).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Id {
    pub thing: u32,
    pub face: u32,
}

/// A triangle as a game frame put it on the screen: its corners, right of and below the
/// projection centre, and the places of the texture they show.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Corners {
    pub at: [Vec2; 3],
    pub uv: [[u8; 2]; 3],
}

/// The triangles of one drawing by their [`Id`].
pub type Shapes = HashMap<Id, Corners>;

/// The bit of a thing that has no lasting number of its own: the things an effect draws
/// come and go in one list, so the number of one in the list says little about which it
/// was before. It is known again by its kind and its place instead ([`tracked`]).
pub const TRACKED: u32 = 1 << 31;
/// The part of a tracked thing's number that is its kind.
const KIND: u32 = !0xFFFF;

/// The number of the `nth` tracked thing of a kind (15 bits of it are kept) in a drawing.
pub fn tracked(kind: u32, nth: usize) -> u32 {
    TRACKED | (kind & 0x7FFF) << 16 | (nth as u32 & 0xFFFF)
}

/// The tracked things of `now` known again among those `before`: the unknown thing of
/// the same kind whose triangles lay nearest on the average, within [`JUMP`], each
/// taken once. A thing found no match for is new.
fn aliases(before: &Shapes, now: &[(Id, Corners)]) -> HashMap<u32, u32> {
    fn centres<'a>(
        shapes: impl Iterator<Item = (&'a Id, &'a Corners)>,
    ) -> std::collections::BTreeMap<u32, Vec2> {
        let mut sums = std::collections::BTreeMap::<u32, (Vec2, f32)>::new();
        for (id, corners) in shapes.filter(|(id, _)| id.thing & TRACKED != 0) {
            let sum = sums.entry(id.thing).or_default();
            sum.0 += corners.at.iter().sum::<Vec2>();
            sum.1 += 3.0;
        }
        (sums.into_iter())
            .map(|(thing, (sum, count))| (thing, sum / count))
            .collect()
    }
    let (was, is) = (
        centres(before.iter()),
        centres(now.iter().map(|(id, c)| (id, c))),
    );
    let mut taken = std::collections::HashSet::new();
    let mut aliases = HashMap::new();
    for (thing, centre) in &is {
        let nearest = (was.iter())
            .filter(|(other, _)| *other & KIND == thing & KIND && !taken.contains(*other))
            .map(|(other, was)| (*other, was.distance(*centre)))
            .filter(|(_, distance)| *distance <= JUMP)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((other, _)) = nearest {
            taken.insert(other);
            aliases.insert(*thing, other);
        }
    }
    aliases
}

/// Where every triangle of `now` comes from, by the rules of the module: the corners it
/// glides from, or none when it snaps. Also the way every thing that glides moved on the
/// average, from before to now.
pub fn glides(
    before: &Shapes,
    now: &[(Id, Corners)],
) -> (Vec<Option<[Vec2; 3]>>, HashMap<u32, Vec2>) {
    let aliases = aliases(before, now);
    let was = |id: &Id| -> Option<&Corners> {
        let thing = match id.thing & TRACKED {
            0 => id.thing,
            _ => *aliases.get(&id.thing)?,
        };
        before.get(&Id { thing, ..*id })
    };
    // What every thing did: whether it snaps, its moves by the place a corner has now,
    // and the sum and count of them.
    #[derive(Default)]
    struct Thing {
        snaps: bool,
        matched: bool,
        moves: HashMap<[u32; 2], Vec2>,
        sum: Vec2,
        corners: u32,
    }
    let key = |at: Vec2| [at.x.to_bits(), at.y.to_bits()];
    let mut things: HashMap<u32, Thing> = HashMap::new();
    for (id, corners) in now {
        let thing = things.entry(id.thing).or_default();
        let Some(was) = was(id) else {
            continue;
        };
        thing.matched = true;
        if was.uv != corners.uv {
            thing.snaps = true;
        }
        for (from, to) in was.at.iter().zip(corners.at) {
            let moved = to - *from;
            if !moved.is_finite() || moved.length() > JUMP {
                thing.snaps = true;
            }
            thing.moves.insert(key(to), moved);
            thing.sum += moved;
            thing.corners += 1;
        }
    }
    let shifts: HashMap<u32, Vec2> = (things.iter())
        .filter(|(_, thing)| thing.matched && !thing.snaps)
        .map(|(number, thing)| (*number, thing.sum / thing.corners as f32))
        .collect();
    let from = (now.iter())
        .map(|(id, corners)| {
            let shift = shifts.get(&id.thing)?;
            if let Some(was) = was(id) {
                return Some(was.at);
            }
            let moves = &things[&id.thing].moves;
            Some(
                corners
                    .at
                    .map(|at| at - moves.get(&key(at)).unwrap_or(shift)),
            )
        })
        .collect();
    (from, shifts)
}

/// The share of the way from the drawing before to the last one that a frame of the
/// monitor shows: `ticks` game frames lay between the two drawings, and the clock has gone
/// `fraction` of a game frame on since the last one. One game frame shows the fraction;
/// over more the frame shows the time of the game frame before the last plus the fraction,
/// so a monitor frame that took long goes on at the same pace. None is one, the last.
pub fn share(ticks: u32, fraction: f32) -> f32 {
    if ticks == 0 {
        return 1.0;
    }
    let fraction = fraction.clamp(0.0, 1.0);
    ((ticks - 1) as f32 + fraction) / ticks as f32
}

/// A place `share` of the way from `from` to `to`.
pub fn between(from: Vec2, to: Vec2, share: f32) -> Vec2 {
    from + (to - from) * share
}

/// How far the drawn scroll offset lies from the last game frame's, `share` of the way
/// from the one before: none when the offset jumped further than [`JUMP`].
pub fn scroll_glide(before: IVec2, now: IVec2, share: f32) -> Option<Vec2> {
    let moved = (now - before).as_vec2();
    (moved.length() <= JUMP).then(|| moved * (share - 1.0))
}

/// What the picture of a game frame is seen with: the camera of the view or the fight,
/// where it stands and looks, its roll and projection distance, and what else changes
/// the whole picture.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Eye {
    pub at: [i32; 3],
    pub looks: [i32; 3],
    pub roll: i16,
    pub distance: i32,
    pub view: usize,
    pub zoom: i32,
    pub fight: bool,
    pub reach: i32,
}

/// Why the picture cuts from `before` to `now`, or none when it goes on.
pub fn cut(before: &Eye, now: &Eye) -> Option<&'static str> {
    if before.view != now.view {
        return Some("another view");
    }
    if before.fight != now.fight {
        return Some("a fight starts or ends");
    }
    if before.zoom != now.zoom || before.reach != now.reach {
        return Some("another zoom or reach");
    }
    let vector = |from: [i32; 3], to: [i32; 3]| {
        DVec3::new(
            f64::from(to[0] - from[0]),
            f64::from(to[1] - from[1]),
            f64::from(to[2] - from[2]),
        )
    };
    let (sight, seen) = (vector(before.at, before.looks), vector(now.at, now.looks));
    let length = sight.length().max(1.0);
    if vector(before.at, now.at).length() > MOVE * length
        || vector(before.looks, now.looks).length() > MOVE * length
    {
        return Some("the camera moved too far");
    }
    let turn = sight.angle_between(seen).to_degrees();
    let rolled = f64::from(now.roll.wrapping_sub(before.roll)).abs() * 360.0 / 4096.0;
    if turn > TURN || rolled > TURN {
        return Some("the camera turned too far");
    }
    let ratio = f64::from(now.distance.max(1)) / f64::from(before.distance.max(1));
    if !(1.0 / DISTANCE..=DISTANCE).contains(&ratio) {
        return Some("another projection distance");
    }
    None
}

/// The history of the drawings a room keeps for the glide, in [`crate::shown::Things`]:
/// a new room has none, so its first drawing snaps.
#[derive(Default)]
pub struct Motion {
    /// Game frames that changed the picture since the last drawing.
    pending: u32,
    /// The game frames between the drawing before and the last one.
    ticks: u32,
    /// The last drawing cut, or a held picture is to be let go with a cut.
    cut: bool,
    held: bool,
    before: Shapes,
    now: Shapes,
    eye_before: Option<Eye>,
    eye_now: Option<Eye>,
    /// How the things that glide moved on the average.
    shifts: HashMap<u32, Vec2>,
    /// Drawings of a new game frame so far (the scroll offset follows them).
    drawings: u64,
    /// The steps of the fixed clock when the last game frame was drawn ([`Glide::steps`]).
    drawn_at: u64,
}

impl Motion {
    /// A game frame ran and changed the picture.
    pub fn tick(&mut self) {
        self.pending = self.pending.saturating_add(1);
    }

    /// The picture is held as it was: the drawing that lets it go cuts.
    pub fn hold(&mut self) {
        self.held = true;
    }

    /// The glide is off: nothing is kept, and the next drawing with it on cuts.
    pub fn forget(&mut self) {
        let drawings = self.drawings + u64::from(self.pending > 0);
        *self = Self {
            drawings,
            ..Self::default()
        };
    }

    /// A drawing seen with `eye` of the triangles `now`, when the fixed clock has made
    /// `steps` steps: where each triangle comes from, none for one that snaps. A drawing
    /// in the same game frame as the last (the window changed) glides from the same game
    /// frame before.
    pub fn draw(&mut self, eye: Eye, now: &[(Id, Corners)], steps: u64) -> Vec<Option<[Vec2; 3]>> {
        let shapes: Shapes = now.iter().copied().collect();
        if self.pending > 0 {
            self.ticks = std::mem::take(&mut self.pending);
            self.before = std::mem::replace(&mut self.now, shapes);
            self.eye_before = self.eye_now.replace(eye);
            self.drawings += 1;
            self.drawn_at = steps;
        } else {
            self.now = shapes;
            self.eye_now = Some(eye);
        }
        let why = match self.eye_before.as_ref() {
            None => Some("a new room"),
            Some(_) if std::mem::take(&mut self.held) => Some("a held picture lets go"),
            Some(_) if self.ticks > MOST_TICKS => Some("many game frames at once"),
            Some(before) => cut(before, &eye),
        };
        if let Some(why) = why
            && !self.cut
            && self.eye_before.is_some()
        {
            info!("smooth motion: the picture cuts ({why})");
        }
        self.cut = why.is_some();
        if self.cut {
            self.shifts.clear();
            return vec![None; now.len()];
        }
        let (from, shifts) = glides(&self.before, now);
        self.shifts = shifts;
        from
    }

    /// The share of the way a frame of the monitor shows when the fixed clock has made
    /// `steps` steps and gone `fraction` of a step on: one after a cut, and once a step
    /// of the clock went by without a drawing (the room stands under a menu, or nothing
    /// moved), so that what stands is drawn where it stands.
    pub fn share(&self, fraction: f32, steps: u64) -> f32 {
        match self.cut || steps > self.drawn_at {
            true => 1.0,
            false => share(self.ticks, fraction),
        }
    }

    /// The last drawing cut.
    pub fn cuts(&self) -> bool {
        self.cut
    }

    /// How far a thing that glides lies from where the last game frame put it, at `share`
    /// of the way.
    pub fn shift(&self, thing: u32, share: f32) -> Vec2 {
        (self.shifts.get(&thing)).map_or(Vec2::ZERO, |moved| *moved * (share - 1.0))
    }

    /// Drawings of a new game frame so far.
    pub fn drawings(&self) -> u64 {
        self.drawings
    }
}

/// The share of the way every frame of the monitor shows, one for the last game frame as
/// it is: set once a frame behind the drawing of the room's things (`shown.rs`), read by
/// whatever places them and the background.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct Glide {
    pub share: f32,
    /// The glide of `smooth_motion` is on.
    pub on: bool,
    /// The steps the fixed clock has made, whether a room ran in them or not.
    pub steps: u64,
}

impl Default for Glide {
    fn default() -> Self {
        Self {
            share: 1.0,
            on: false,
            steps: 0,
        }
    }
}

/// One step of the fixed clock, counted for [`Motion::share`].
pub fn count_steps(mut glide: ResMut<Glide>) {
    glide.steps += 1;
}

/// `--glide-at=F` of a check: every frame at the share `F` of the way.
#[derive(Resource, Debug, Clone, Copy)]
pub struct GlideAt(pub f32);

/// `--glide-at=F`, when a check asks for it.
pub fn glide_at(args: &[String]) -> Option<GlideAt> {
    let value = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--glide-at="))?;
    let share: f32 = value
        .parse()
        .expect("--glide-at=F takes a number from 0 to 1");
    Some(GlideAt(share.clamp(0.0, 1.0)))
}

/// `--frame-times` of a check: the frames of the last two seconds of the real clock, their
/// mean and their longest; how long the app's own work of a frame took (from the first
/// schedule to the last, without the drawing of the graphics card); and how much of that
/// the glide took (knowing the triangles again, giving the meshes their corners).
#[derive(Resource, Default)]
pub struct FrameTimes {
    since: f64,
    frames: u32,
    sum: f64,
    longest: f64,
    started: Option<std::time::Instant>,
    work: f64,
    longest_work: f64,
    glide: f64,
}

impl FrameTimes {
    /// The glide took this long in this frame.
    pub fn glided(&mut self, took: std::time::Duration) {
        self.glide += took.as_secs_f64();
    }
}

/// The start of a frame's work, for `--frame-times`.
pub fn start_frame(mut times: ResMut<FrameTimes>) {
    times.started = Some(std::time::Instant::now());
}

/// Logs the frame times every two seconds while `--frame-times` asks for it.
pub fn log_frame_times(real: Res<Time<Real>>, glide: Res<Glide>, mut times: ResMut<FrameTimes>) {
    let delta = real.delta_secs_f64();
    let work = times
        .started
        .map_or(0.0, |started| started.elapsed().as_secs_f64());
    times.frames += 1;
    times.sum += delta;
    times.longest = times.longest.max(delta);
    times.work += work;
    times.longest_work = times.longest_work.max(work);
    let now = real.elapsed_secs_f64();
    if now - times.since >= 2.0 {
        let frames = f64::from(times.frames.max(1));
        info!(
            "frame times: {} frames, mean {:.2} ms, longest {:.2} ms; the app's work: mean \
             {:.3} ms, longest {:.3} ms, of it the glide {:.3} ms; smooth motion {}",
            times.frames,
            1000.0 * times.sum / frames,
            1000.0 * times.longest,
            1000.0 * times.work / frames,
            1000.0 * times.longest_work,
            1000.0 * times.glide / frames,
            match glide.on {
                true => "on",
                false => "off",
            }
        );
        *times = FrameTimes {
            since: now,
            ..default()
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(thing: u32, face: u32) -> Id {
        Id { thing, face }
    }

    fn corners(at: [[f32; 2]; 3]) -> Corners {
        Corners {
            at: at.map(Vec2::from),
            uv: [[0, 0], [8, 0], [0, 8]],
        }
    }

    fn moved(corners: Corners, by: Vec2) -> Corners {
        Corners {
            at: corners.at.map(|at| at + by),
            ..corners
        }
    }

    #[test]
    fn a_share_is_the_fraction_over_one_game_frame_and_goes_on_over_more() {
        assert_eq!(share(1, 0.25), 0.25);
        assert_eq!(share(1, 0.0), 0.0);
        assert_eq!(share(2, 0.5), 0.75);
        assert_eq!(share(0, 0.3), 1.0);
        assert_eq!(share(1, 1.5), 1.0);
        assert_eq!(
            between(Vec2::ZERO, Vec2::new(10.0, -4.0), 0.5),
            Vec2::new(5.0, -2.0)
        );
    }

    #[test]
    fn a_thing_that_moved_a_little_glides_from_where_it_was() {
        let a = corners([[0.0, 0.0], [10.0, 0.0], [0.0, 10.0]]);
        let before: Shapes = [(id(1, 0), a)].into();
        let now = [(id(1, 0), moved(a, Vec2::new(4.0, 2.0)))];
        let (from, shifts) = glides(&before, &now);
        assert_eq!(from, vec![Some(a.at)]);
        assert_eq!(shifts[&1], Vec2::new(4.0, 2.0));
    }

    #[test]
    fn a_warp_snaps_the_whole_thing_and_leaves_the_others() {
        let (a, b) = (
            corners([[0.0, 0.0], [10.0, 0.0], [0.0, 10.0]]),
            corners([[50.0, 0.0], [60.0, 0.0], [50.0, 10.0]]),
        );
        let before: Shapes = [(id(1, 0), a), (id(1, 2), b), (id(2, 0), a)].into();
        let now = [
            (id(1, 0), moved(a, Vec2::new(1.0, 0.0))),
            (id(1, 2), moved(b, Vec2::new(JUMP + 1.0, 0.0))),
            (id(2, 0), moved(a, Vec2::new(0.0, 3.0))),
        ];
        let (from, shifts) = glides(&before, &now);
        assert_eq!(from, vec![None, None, Some(a.at)]);
        assert!(!shifts.contains_key(&1));
    }

    #[test]
    fn another_texture_place_or_a_new_thing_snaps() {
        let a = corners([[0.0, 0.0], [10.0, 0.0], [0.0, 10.0]]);
        let mut other = moved(a, Vec2::ONE);
        other.uv[0] = [16, 16];
        let before: Shapes = [(id(1, 0), a)].into();
        let now = [(id(1, 0), other), (id(3, 0), a)];
        assert_eq!(glides(&before, &now).0, vec![None, None]);
    }

    #[test]
    fn a_face_that_turns_into_view_moves_with_its_neighbours() {
        // Face 0 was drawn before and now; face 2 was turned away before. It shares one
        // corner with face 0; its other corners move as the thing does on the average.
        let a = corners([[0.0, 0.0], [10.0, 0.0], [0.0, 10.0]]);
        let step = Vec2::new(2.0, 0.0);
        let before: Shapes = [(id(1, 0), a)].into();
        let new = corners([[12.0, 0.0], [20.0, 5.0], [12.0, 12.0]]);
        let now = [(id(1, 0), moved(a, step)), (id(1, 2), new)];
        let (from, _) = glides(&before, &now);
        assert_eq!(from[0], Some(a.at));
        assert_eq!(from[1], Some(new.at.map(|at| at - step)));
        // A shared corner takes its neighbour's move, not the average.
        let turned = Corners {
            at: [
                Vec2::new(10.0, 0.0),
                Vec2::new(30.0, 0.0),
                Vec2::new(10.0, 10.0),
            ],
            ..a
        };
        let before: Shapes = [(id(1, 0), a), (id(1, 4), moved(a, Vec2::new(40.0, 0.0)))].into();
        let now = [
            (id(1, 0), turned),
            (id(1, 4), moved(a, Vec2::new(40.0, 0.0))),
            (id(1, 2), corners([[30.0, 0.0], [40.0, 0.0], [5.0, 5.0]])),
        ];
        let (from, _) = glides(&before, &now);
        let from = from[2].expect("it glides");
        assert_eq!(
            from[0],
            Vec2::new(10.0, 0.0),
            "the corner at 30 came from 10"
        );
        assert_eq!(
            from[1],
            Vec2::new(40.0, 0.0),
            "the corner at 40 stood still"
        );
    }

    #[test]
    fn a_tracked_thing_is_known_again_by_its_kind_and_place() {
        // Three sparks of one kind; the first one is gone, so the others come one place
        // earlier in the list. Each is known by where it was, not by its place in it.
        let spark = |x: f32| corners([[x, 0.0], [x + 4.0, 0.0], [x, 4.0]]);
        let before: Shapes = [
            (id(tracked(5, 0), 0), spark(0.0)),
            (id(tracked(5, 1), 0), spark(20.0)),
            (id(tracked(5, 2), 0), spark(40.0)),
            (id(tracked(6, 0), 0), spark(100.0)),
        ]
        .into();
        let now = [
            (id(tracked(5, 0), 0), spark(22.0)),
            (id(tracked(5, 1), 0), spark(42.0)),
            // Another kind near where a spark was is new.
            (id(tracked(7, 0), 0), spark(1.0)),
            (id(tracked(6, 0), 0), spark(101.0)),
        ];
        let (from, _) = glides(&before, &now);
        assert_eq!(
            from,
            vec![
                Some(spark(20.0).at),
                Some(spark(40.0).at),
                None,
                Some(spark(100.0).at)
            ]
        );
    }

    #[test]
    fn the_scroll_glides_unless_it_jumps() {
        let glide = scroll_glide(IVec2::new(0, 0), IVec2::new(8, -4), 0.25);
        assert_eq!(glide, Some(Vec2::new(-6.0, 3.0)));
        assert_eq!(scroll_glide(IVec2::ZERO, IVec2::new(100, 0), 0.5), None);
        assert_eq!(
            scroll_glide(IVec2::ZERO, IVec2::new(9, 9), 1.0),
            Some(Vec2::ZERO)
        );
    }

    fn eye() -> Eye {
        Eye {
            at: [0, -1000, -2000],
            looks: [0, 0, 0],
            roll: 0,
            distance: 300,
            view: 0,
            zoom: 4096,
            fight: true,
            reach: 0,
        }
    }

    #[test]
    fn a_camera_that_glides_goes_on_and_one_that_jumps_cuts() {
        let before = eye();
        // A swing of a few degrees and a short move go on.
        let swung = Eye {
            at: [150, -1000, -1990],
            looks: [20, 0, 10],
            ..before
        };
        assert_eq!(cut(&before, &swung), None);
        assert_eq!(cut(&before, &before), None);
        // Another shot: the camera on the other side.
        let other = Eye {
            at: [0, -1000, 2000],
            ..before
        };
        assert!(cut(&before, &other).is_some());
        let turned = Eye {
            at: [1500, -1000, -1500],
            ..before
        };
        assert!(cut(&before, &turned).is_some());
        let closer = Eye {
            distance: 450,
            ..before
        };
        assert!(cut(&before, &closer).is_some());
        let rolled = Eye {
            roll: 400,
            ..before
        };
        assert!(cut(&before, &rolled).is_some());
        for changed in [
            Eye { view: 1, ..before },
            Eye {
                fight: false,
                ..before
            },
            Eye {
                zoom: 5000,
                ..before
            },
            Eye {
                reach: 80,
                ..before
            },
        ] {
            assert!(cut(&before, &changed).is_some(), "{changed:?}");
        }
    }

    #[test]
    fn the_first_drawing_a_hold_and_many_game_frames_cut() {
        let a = corners([[0.0, 0.0], [10.0, 0.0], [0.0, 10.0]]);
        let step = |n: f32| [(id(1, 0), moved(a, Vec2::new(n, 0.0)))];
        let mut motion = Motion::default();
        let mut steps = 0;
        let mut tick = |motion: &mut Motion| {
            motion.tick();
            steps += 1;
            steps
        };
        let at = tick(&mut motion);
        assert_eq!(motion.draw(eye(), &step(0.0), at), vec![None]);
        assert!(motion.cuts());
        assert_eq!(motion.share(0.5, at), 1.0);
        let at = tick(&mut motion);
        assert_eq!(motion.draw(eye(), &step(2.0), at), vec![Some(a.at)]);
        assert_eq!(motion.share(0.5, at), 0.5);
        assert_eq!(motion.shift(1, 0.5), Vec2::new(-1.0, 0.0));
        // A step of the clock without a drawing: everything stands where it is.
        assert_eq!(motion.share(0.5, at + 1), 1.0);
        // The window changed in the same game frame: it glides from the same one before.
        assert_eq!(motion.draw(eye(), &step(3.0), at), vec![Some(a.at)]);
        motion.hold();
        let at = tick(&mut motion);
        assert_eq!(motion.draw(eye(), &step(4.0), at), vec![None]);
        tick(&mut motion);
        let at = tick(&mut motion);
        assert_eq!(
            motion.draw(eye(), &step(5.0), at),
            vec![Some(moved(a, Vec2::X * 4.0).at)]
        );
        assert_eq!(motion.share(0.5, at), 0.75);
        let mut at = at;
        (0..=MOST_TICKS).for_each(|_| at = tick(&mut motion));
        assert_eq!(motion.draw(eye(), &step(6.0), at), vec![None]);
        // A camera cut, then the glide again.
        let at = tick(&mut motion);
        let other = Eye {
            at: [0, -1000, 2000],
            ..eye()
        };
        assert_eq!(motion.draw(other, &step(7.0), at), vec![None]);
        let at = tick(&mut motion);
        assert_eq!(
            motion.draw(other, &step(8.0), at),
            vec![Some(moved(a, Vec2::X * 7.0).at)]
        );
        let drawings = motion.drawings();
        motion.forget();
        assert_eq!(motion.drawings(), drawings);
        let at = tick(&mut motion);
        assert_eq!(motion.draw(other, &step(9.0), at), vec![None]);
    }
}
