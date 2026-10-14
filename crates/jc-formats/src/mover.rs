
use crate::command::quotient;
use crate::field::{Fault, Ground};
use crate::skeleton::{cosine, sine};

/// Where the arc tangent table of the console library lies in the executable.
pub const ARC_TANGENT: u32 = 0x8008_90C4;
/// Entries of that table: one per 1024th of the ratio, both ends counted.
const ARC_STEPS: usize = 1025;
/// The weights every actor's turn filter is made with: eight of [`EVEN_WEIGHT`].
pub const TURN_WEIGHTS: u32 = 0x8007_94C4;
const EVEN_WEIGHT: i32 = 512;
/// How near someone must stand to an actor's goal for the actor to stop short of it
/// (type 18 of the command list).
pub const NEAR: i32 = 200;
/// The distance the go routine asks the distance routine for.
const LIMIT: u32 = 4095;
/// What the go routine adds to the last step before it asks who is near.
const SHORT: i32 = 50;

/// The bits of an actor's flag word that movement reads and writes.
pub mod flag {
    /// No step in this game frame. The mover clears it.
    pub const NO_STEP: u32 = 8;
    /// The actor walks over the ground by the difference of the travel.
    pub const WALKS: u32 = 0x4_0000;
    /// A request gave the actor a target.
    pub const TARGET: u32 = 0x8_0000;
    /// The actor is where its animation says.
    pub const CARRIED: u32 = 0x0100_0000;
    /// The actor is on a path.
    pub const PATH: u32 = 0x0200_0000;
    /// The mover turns the actor to its target, a step per game frame.
    pub const TURNS: u32 = 0x0400_0000;
    /// The forward step is multiplied by the actor's speed.
    pub const SPEED: u32 = 0x2000_0000;
}

/// A product of two values in 4096ths, as the mover makes it: divided towards zero.
fn part(product: i32) -> i32 {
    product / 4096
}

/// The arc tangent table of the executable and the library routine `0x8005BC5C` that reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArcTangent(Vec<i16>);

impl ArcTangent {
    /// The table of an executable loaded at `base`.
    pub fn from_image(image: &[u8], base: u32) -> Option<Self> {
        let start = usize::try_from(ARC_TANGENT.checked_sub(base)?).ok()?;
        let bytes = image.get(start..start + ARC_STEPS * 2)?;
        let (steps, _) = bytes.as_chunks::<2>();
        Some(Self(
            steps.iter().map(|pair| i16::from_le_bytes(*pair)).collect(),
        ))
    }

    /// A table of the given steps, the angle of a ratio in 1024ths.
    pub fn of(steps: Vec<i16>) -> Self {
        Self(steps)
    }

    pub fn steps(&self) -> &[i16] {
        &self.0
    }

    /// The angle of a direction, 4096 to the circle: 0 along `along`, 1024 along `across`,
    /// between -2048 and 2048. The actors hand it the differences in x and in z.
    pub fn angle(&self, across: i32, along: i32) -> i16 {
        let (back, other_side) = (along < 0, across < 0);
        let (along, across) = (along.wrapping_abs(), across.wrapping_abs());
        if along == 0 && across == 0 {
            return 0;
        }
        let step = |ratio: Option<i32>| {
            let at = ratio.and_then(|ratio| usize::try_from(ratio).ok());
            i32::from(at.and_then(|at| self.0.get(at)).copied().unwrap_or(0))
        };
        // The smaller value over the larger one in 1024ths; a value too large to be
        // shifted up has the other one shifted down instead.
        let ratio = |small: i32, large: i32| match small & 0x7FE0_0000 {
            0 => small.wrapping_shl(10).checked_div(large),
            _ => small.checked_div(large >> 10),
        };
        let mut angle = match across < along {
            true => step(ratio(across, along)),
            false => 1024 - step(ratio(along, across)),
        };
        if back {
            angle = 2048 - angle;
        }
        if other_side {
            angle = -angle;
        }
        angle as i16
    }
}

/// The turn filter of an actor (`0x80014588`): the last values put in, weighted by a table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    weights: Vec<i32>,
    ring: Vec<i32>,
    /// What came out of the last run.
    pub out: i32,
}

impl Filter {
    /// A filter with these weights in 4096ths, the newest value first.
    pub fn new(weights: Vec<i32>) -> Self {
        let ring = vec![0; weights.len()];
        Self {
            weights,
            ring,
            out: 0,
        }
    }

    /// The filter every actor is made with: the average of the last eight values.
    pub fn even() -> Self {
        Self::new(vec![EVEN_WEIGHT; 8])
    }

    /// Puts a value in at the place of the game's counter and answers the weighted sum,
    /// rounded down. A game frame without a run leaves its place in the ring as it was.
    pub fn run(&mut self, put: i32, counter: u32) -> i32 {
        let size = self.weights.len() as u32;
        if size == 0 {
            return self.out;
        }
        self.ring[(counter % size) as usize] = put;
        let sum = (0..size).fold(0i32, |sum, back| {
            let value = self.ring[(counter.wrapping_sub(back) % size) as usize];
            sum.wrapping_add(value.wrapping_mul(self.weights[back as usize]))
        });
        self.out = sum >> 12;
        self.out
    }
}

/// A point of a path: a place in parts of a unit from the actor's base place, and its turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathPoint {
    pub place: [i16; 3],
    pub turn: i16,
}

/// A path of a room: one point per game frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path {
    /// What the places of the points are divided by.
    pub divisor: i32,
    pub points: Vec<PathPoint>,
}

/// The paths of a room's kind 4 resource. None when the block is not one.
pub fn paths(block: &[u8]) -> Option<Vec<Path>> {
    let word = |at: usize| {
        let bytes = block.get(at..at.checked_add(4)?)?;
        Some(u32::from_le_bytes(bytes.try_into().ok()?))
    };
    let count = usize::try_from(word(0)?).ok()?;
    // A count that the block cannot hold is no count.
    if count > block.len() / 4 {
        return None;
    }
    (0..count)
        .map(|list| {
            let start = usize::try_from(word(4 + 4 * list)?).ok()?;
            let points = usize::try_from(word(start)?).ok()?;
            let divisor = word(start.checked_add(4)?)? as i32;
            let end = points.checked_mul(8)?.checked_add(start.checked_add(8)?)?;
            let (records, _) = block.get(start + 8..end)?.as_chunks::<8>();
            let value =
                |record: &[u8; 8], at: usize| i16::from_le_bytes([record[at], record[at + 1]]);
            let points = records.iter().map(|record| PathPoint {
                place: [value(record, 0), value(record, 2), value(record, 4)],
                turn: value(record, 6),
            });
            Some(Path {
                divisor,
                points: points.collect(),
            })
        })
        .collect()
}

/// What the mover needs of the running animation in a game frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Footing {
    /// The frame of the animation that is shown.
    pub frame: usize,
    /// Entry 0 of that frame.
    pub travel: [i16; 3],
    /// The extra word of the animation, 32 where the block has none.
    pub divisor: i32,
    /// The third scale of the first bone's record and the third word of the actor's own
    /// scale, in 4096ths: a walking actor's step is stretched by both.
    pub scales: [i32; 2],
}

/// The part of an actor that movement works on.
///
/// Once per game frame, after the pass of the command list and before the drawing:
/// [`Mover::step`]; at the end of the game frame, with the step of the animation:
/// [`Mover::path_step`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mover {
    /// The bits of [`flag`].
    pub flags: u32,
    pub position: [i32; 3],
    /// The position as the mover found it in this game frame.
    pub before: [i32; 3],
    /// The base place: where the actor was made, or where its last path ended.
    pub base: [i32; 3],
    /// The turn about the upright axis. The game keeps 16 bits and does not bring them
    /// back into one circle.
    pub turn: i16,
    /// The travel the mover read in the last game frame.
    pub travel: [i32; 3],
    /// The target place and the turn step of a request.
    pub target: [i32; 3],
    pub turn_step: i32,
    /// The speed of the travel in 4096ths, used with [`flag::SPEED`].
    pub speed: i32,
    pub filter: Filter,
    /// The path the actor is on and the point it has come to.
    pub path: Option<Path>,
    pub path_at: usize,
    /// What the walk-mesh mover answered in this game frame and in the one before.
    pub answers: [i32; 2],
    /// Set when the walk-mesh mover came to a place where the game does not go on.
    pub fault: Option<Fault>,
}

impl Mover {
    /// An actor made at `place` with this turn.
    pub fn new(place: [i32; 3], turn: i16) -> Self {
        Self {
            flags: 0,
            position: place,
            before: place,
            base: place,
            turn,
            travel: [0; 3],
            target: [0; 3],
            turn_step: 0,
            speed: 4096,
            filter: Filter::even(),
            path: None,
            path_at: 0,
            answers: [0; 2],
            fault: None,
        }
    }

    /// Puts the actor on a path, as a request with one does.
    pub fn set_path(&mut self, path: Path) {
        self.path = Some(path);
        self.path_at = 0;
        self.flags |= flag::PATH;
    }

    /// The mover `0x8001F3BC`: one game frame.
    pub fn step(&mut self, footing: &Footing, arc: &ArcTangent, counter: u32) {
        self.before = self.position;
        let last = self.travel;
        self.travel = footing.travel.map(i32::from);

        // The step: sideways and forward. The sideways one is made and never used.
        let walks = self.flags & flag::WALKS != 0;
        let held = footing.frame == 0 || self.flags & flag::NO_STEP != 0;
        if held {
            self.flags &= !flag::NO_STEP;
        }
        let forward = match (walks, held) {
            (true, true) => 0,
            (true, false) => self.travel[2].wrapping_sub(last[2]),
            (false, _) => self.travel[2],
        };
        let mut forward = forward.max(0);
        if self.flags & flag::SPEED != 0 {
            forward = part(forward.wrapping_mul(self.speed));
        }

        if self.flags & flag::TURNS != 0 && self.turn_to(arc, self.target, self.turn_step, counter)
        {
            self.flags &= !flag::TURNS;
        }

        let turn = i32::from(self.turn);
        let (sine, cosine) = (sine(turn), cosine(turn));
        let over = |value: i32| quotient(value, footing.divisor);
        if self.flags & flag::PATH != 0 {
            let path = self.path.as_ref();
            let point = path.and_then(|path| Some((*path.points.get(self.path_at)?, path.divisor)));
            // A path whose counter names no point leaves the actor where it is.
            if let Some((point, divisor)) = point {
                self.travel = point.place.map(i32::from);
                for axis in 0..3 {
                    self.position[axis] =
                        self.base[axis].wrapping_add(quotient(self.travel[axis], divisor));
                }
                self.turn = point.turn;
            }
        } else if self.flags & flag::CARRIED != 0 {
            let [x, y, z] = self.travel;
            let along = z.wrapping_mul(cosine).wrapping_sub(x.wrapping_mul(sine));
            let across = z.wrapping_mul(sine).wrapping_add(x.wrapping_mul(cosine));
            self.position = [
                self.base[0].wrapping_add(over(part(across))),
                self.base[1].wrapping_add(over(y)),
                self.base[2].wrapping_add(over(part(along))),
            ];
        } else if walks {
            let stretched = |share: i32| {
                let step = part(forward.wrapping_mul(share));
                let step = part(step.wrapping_mul(footing.scales[0]));
                over(part(step.wrapping_mul(footing.scales[1])))
            };
            self.position[0] = self.position[0].wrapping_add(stretched(sine));
            self.position[2] = self.position[2].wrapping_add(stretched(cosine));
        } else {
            self.position[0] = self.base[0].wrapping_add(over(part(forward.wrapping_mul(sine))));
            self.position[2] = self.base[2].wrapping_add(over(part(forward.wrapping_mul(cosine))));
        }
    }

    /// The end of the mover for an actor that walks over the ground (flag 0x40000), whatever
    /// rule put it in [`Mover::step`]: the second mover of file 53 with the position, the
    /// place before and the turn. `ground` is `field::RoomData::second_ground` of the room.
    pub fn walk(&mut self, ground: &Ground, arc: &ArcTangent) {
        if self.flags & flag::WALKS == 0 {
            return;
        }
        match ground.walk_actor(&mut self.position, self.before, self.turn, arc) {
            Ok(answer) => self.answers = [answer, self.answers[0]],
            Err(fault) => self.fault = Some(fault),
        }
    }

    /// The end of the animation's step for an actor on a path (`0x800200C0` on): the path
    /// counter goes on by one. True when the path is through: the actor leaves the path,
    /// its base place becomes where it stands, it makes no step in the next game frame, and
    /// the caller puts its animation on the end frame of the action.
    pub fn path_step(&mut self) -> bool {
        if self.flags & flag::PATH == 0 {
            return false;
        }
        self.path_at += 1;
        if self.path_at < self.path.as_ref().map_or(0, |path| path.points.len()) {
            return false;
        }
        self.flags = self.flags & !flag::PATH | flag::NO_STEP;
        self.base = self.position;
        true
    }

    /// Turning to a place (`0x8001DE64`): true when the actor faces it, which sets the turn
    /// on the place; otherwise the step, the short way round, goes through the filter.
    pub fn turn_to(&mut self, arc: &ArcTangent, place: [i32; 3], step: i32, counter: u32) -> bool {
        let across = place[0].wrapping_sub(self.position[0]);
        let along = place[2].wrapping_sub(self.position[2]);
        let wanted = i32::from(arc.angle(across, along));
        let left = wanted.wrapping_sub(i32::from(self.turn)) & 0xFFF;
        if left < step || left >= 4096 - step {
            self.turn = self.turn.wrapping_add(left as i16);
            return true;
        }
        let put = if left < 2048 {
            step
        } else {
            step.wrapping_neg()
        };
        self.turn = self.turn.wrapping_add(self.filter.run(put, counter) as i16);
        false
    }

    /// Turning by an amount (`0x8001DDAC`): to side 1 or the other, by the step or by what
    /// is left when that is less. No step in this game frame. Answers the angle still left.
    pub fn turn_by(&mut self, side: i32, left: i32, step: i32, counter: u32) -> i32 {
        let by = left.min(step);
        let put = if side == 1 { by } else { by.wrapping_neg() };
        self.flags |= flag::NO_STEP;
        self.turn = self.turn.wrapping_add(self.filter.run(put, counter) as i16);
        match left.wrapping_sub(step) {
            rest if rest < 0 => 0,
            rest => rest.min(left),
        }
    }

    /// The distance routine `0x8003B744` with a limit of 4095: whether the place is within
    /// the limit, and the value the routine leaves in place of the limit. That is the squared
    /// distance, or the largest value there is when the two are further apart than the limit
    /// on one axis.
    fn reach(&self, place: [i32; 3]) -> (bool, u32) {
        let apart =
            [0, 1, 2].map(|axis| self.position[axis].wrapping_sub(place[axis]).unsigned_abs());
        if apart.iter().any(|apart| *apart > LIMIT) {
            return (false, u32::MAX);
        }
        let squared = apart
            .iter()
            .fold(0u32, |sum, apart| sum.wrapping_add(apart * apart));
        (squared <= LIMIT * LIMIT, squared)
    }

    /// Whether the actor stands nearer to a place than a distance: the test the game makes
    /// of the player and of every other actor to find out who is near (`0x8001E030`).
    pub fn within(&self, place: [i32; 3], distance: i32) -> bool {
        let (within, squared) = self.reach(place);
        within && squared < distance.wrapping_mul(distance) as u32
    }

    /// On which side of the actor's heading a place lies (`0x8001E15C`): 0 when it is
    /// not nearer than the distance, else 2 when it is up to a quarter turn to the one
    /// side (the angle from the heading is 0 to 1023), 1 on the other side (3073 to
    /// 4095), and 0 when it is behind.
    pub fn side_of(&self, arc: &ArcTangent, place: [i32; 3], distance: i32) -> i32 {
        if !self.within(place, distance) {
            return 0;
        }
        let across = place[0].wrapping_sub(self.position[0]);
        let along = place[2].wrapping_sub(self.position[2]);
        let wanted = i32::from(arc.angle(across, along));
        match wanted.wrapping_sub(i32::from(self.turn)) & 0xFFF {
            0..=1023 => 2,
            3073..=4095 => 1,
            _ => 0,
        }
    }

    /// Who is ahead (`0x8001DF34` without the player): the side of the first of the
    /// other actors that has one. `others` are the places of the actors that count, in
    /// the order of the game's list, without the one that asks.
    pub fn ahead(&self, arc: &ArcTangent, others: &[[i32; 3]], distance: i32) -> i32 {
        (others.iter())
            .map(|place| self.side_of(arc, *place, distance))
            .find(|side| *side != 0)
            .unwrap_or(0)
    }

    /// The speed of the travel (`0x8001DBC0(actor, speed, on)`): on, the speed with its
    /// flag; off, the animation's own.
    pub fn set_speed(&mut self, speed: i32, on: bool) {
        if on {
            self.speed = speed;
            self.flags |= flag::SPEED;
        } else {
            self.speed = 4096;
            self.flags &= !flag::SPEED;
        }
    }

    /// Going to a place (`0x8001DCC8`): true when the actor is there, that is nearer than
    /// its last step, or a little further and `someone_near` (someone else stands within
    /// the distance of the place the routine was given). Otherwise it turns a step to the
    /// place. The feet are moved by the action the actor plays, not here.
    pub fn go_to(
        &mut self,
        arc: &ArcTangent,
        place: [i32; 3],
        step: i32,
        someone_near: bool,
        counter: u32,
    ) -> bool {
        let last = self.position[2].wrapping_sub(self.before[2]).wrapping_abs();
        let (within, squared) = self.reach(place);
        let short = last.wrapping_add(SHORT);
        if within && squared < last.wrapping_mul(last) as u32
            || squared < short.wrapping_mul(short) as u32 && someone_near
        {
            return true;
        }
        self.turn_to(arc, place, step, counter);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::TAU;

    /// A table like the game's, which the tests need only to be near the true angle.
    fn arc() -> ArcTangent {
        let step = |ratio: usize| ((ratio as f64 / 1024.0).atan() * 4096.0 / TAU).round() as i16;
        ArcTangent::of((0..ARC_STEPS).map(step).collect())
    }

    fn footing(frame: usize, travel: [i16; 3]) -> Footing {
        Footing {
            frame,
            travel,
            divisor: 32,
            scales: [4096, 4096],
        }
    }

    #[test]
    fn an_angle_is_found_in_every_quarter() {
        let arc = arc();
        assert_eq!(arc.angle(0, 0), 0);
        assert_eq!(arc.angle(0, 100), 0);
        assert_eq!(arc.angle(100, 100), 512);
        assert_eq!(arc.angle(100, 0), 1024);
        assert_eq!(arc.angle(100, -100), 1536);
        assert_eq!(arc.angle(0, -100), 2048);
        assert_eq!(arc.angle(-100, -100), -1536);
        assert_eq!(arc.angle(-100, 0), -1024);
        assert_eq!(arc.angle(-100, 100), -512);
        // A value too large to be shifted up: the other one is shifted down.
        assert_eq!(arc.angle(0x40_0000, 0x40_0000), 512);
        assert_eq!(arc.angle(0x20_0000, 0x40_0000), arc.angle(1, 2));
    }

    #[test]
    fn the_filter_is_the_average_of_the_last_eight_values() {
        let mut filter = Filter::even();
        // A steady step of 64 comes in over eight game frames.
        let outs: Vec<i32> = (0..10).map(|counter| filter.run(64, counter)).collect();
        assert_eq!(outs, [8, 16, 24, 32, 40, 48, 56, 64, 64, 64]);
        // A game frame without a run leaves its value in the ring.
        assert_eq!(filter.run(0, 11), 56);
        // The sum is rounded down, not towards zero.
        let mut filter = Filter::even();
        assert_eq!(filter.run(-1, 0), -1);
        assert_eq!(filter.run(1, 1), 0);
    }

    #[test]
    fn paths_are_read_from_a_block() {
        let mut block = Vec::new();
        for word in [2u32, 12, 36] {
            block.extend(word.to_le_bytes());
        }
        for word in [2u32, 4] {
            block.extend(word.to_le_bytes());
        }
        for value in [1i16, 2, 3, 100, -4, 5, 6, 200] {
            block.extend(value.to_le_bytes());
        }
        for word in [0u32, 4] {
            block.extend(word.to_le_bytes());
        }
        let paths = paths(&block).expect("two paths");
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0].divisor, 4);
        let point = PathPoint {
            place: [-4, 5, 6],
            turn: 200,
        };
        assert_eq!(paths[0].points[1], point);
        assert!(paths[1].points.is_empty());
        assert_eq!(super::paths(&block[..40]), None);
        assert_eq!(super::paths(&[]), None);
    }

    #[test]
    fn an_actor_without_a_flag_leans_from_its_base_place() {
        let mut mover = Mover::new([1000, 50, 2000], 1024);
        mover.step(&footing(3, [9, 9, 320]), &arc(), 0);
        // Forward travel 320 over 32, along the turn of a quarter circle: x.
        assert_eq!(mover.position, [1010, 50, 2000]);
        assert_eq!(mover.before, [1000, 50, 2000]);
        // An animation never carries its actor backwards.
        mover.step(&footing(4, [0, 0, -320]), &arc(), 1);
        assert_eq!(mover.position, [1000, 50, 2000]);
    }

    #[test]
    fn an_actor_carried_by_its_animation_is_where_the_travel_says() {
        let mut mover = Mover::new([1000, 50, 2000], 0);
        mover.flags |= flag::CARRIED;
        mover.step(&footing(3, [64, -96, 320]), &arc(), 0);
        assert_eq!(mover.position, [1002, 47, 2010]);
        // Turned a quarter: forward is x, the travel's x points along minus z.
        mover.turn = 1024;
        mover.step(&footing(4, [64, -96, 320]), &arc(), 1);
        assert_eq!(mover.position, [1010, 47, 1998]);
        // Back at the base place when the animation starts over.
        mover.step(&footing(0, [0, 0, 0]), &arc(), 2);
        assert_eq!(mover.position, [1000, 50, 2000]);
    }

    #[test]
    fn a_walking_actor_adds_the_difference_of_the_travel() {
        let mut mover = Mover::new([0, 0, 0], 0);
        mover.flags |= flag::WALKS;
        // Frame 0 makes no step.
        mover.step(&footing(0, [0, 0, 640]), &arc(), 0);
        assert_eq!(mover.position, [0, 0, 0]);
        mover.step(&footing(1, [0, 0, 960]), &arc(), 1);
        assert_eq!(mover.position, [0, 0, 10]);
        // Half the size of the first bone, twice the actor's own, and the speed.
        let stretched = Footing {
            scales: [2048, 8192],
            ..footing(2, [0, 0, 1600])
        };
        mover.flags |= flag::SPEED;
        mover.speed = 2048;
        mover.step(&stretched, &arc(), 2);
        assert_eq!(mover.position, [0, 0, 20]);
        // A game frame with the mark makes no step and takes the mark off.
        mover.flags |= flag::NO_STEP;
        mover.step(&footing(3, [0, 0, 3200]), &arc(), 3);
        assert_eq!(mover.position, [0, 0, 20]);
        assert_eq!(mover.flags & flag::NO_STEP, 0);
    }

    #[test]
    fn an_actor_on_a_path_is_put_point_by_point() {
        let points = vec![
            PathPoint {
                place: [40, 8, -80],
                turn: 100,
            },
            PathPoint {
                place: [80, 0, -160],
                turn: 300,
            },
        ];
        let mut mover = Mover::new([1000, 50, 2000], 0);
        mover.set_path(Path { divisor: 4, points });
        mover.step(&footing(5, [0, 0, 9999]), &arc(), 0);
        assert_eq!((mover.position, mover.turn), ([1010, 52, 1980], 100));
        assert!(!mover.path_step());
        mover.step(&footing(6, [0, 0, 9999]), &arc(), 1);
        assert_eq!((mover.position, mover.turn), ([1020, 50, 1960], 300));
        // Through: the place the path ended at is the base place from now on.
        assert!(mover.path_step());
        assert_eq!(mover.flags, flag::NO_STEP);
        assert_eq!(mover.base, [1020, 50, 1960]);
        assert!(!mover.path_step());
    }

    #[test]
    fn a_turn_to_a_place_starts_softly_and_ends_on_it() {
        let arc = arc();
        let mut mover = Mover::new([0, 0, 0], 0);
        // The place lies a quarter circle to the side: 1024 away, 64 a game frame.
        let place = [500, 0, 0];
        let turns: Vec<i16> = (0..9)
            .map(|counter| {
                assert!(!mover.turn_to(&arc, place, 64, counter));
                mover.turn
            })
            .collect();
        assert_eq!(turns, [8, 24, 48, 80, 120, 168, 224, 288, 352]);
        mover.turn = 1000;
        assert!(mover.turn_to(&arc, place, 64, 9));
        assert_eq!(mover.turn, 1024);
        // The short way round to a place on the other side.
        let mut mover = Mover::new([0, 0, 0], 0);
        assert!(!mover.turn_to(&arc, [-500, 0, 0], 64, 0));
        assert_eq!(mover.turn, -8);
        // Turning by an amount: what is left comes back, and no step is made.
        let mut mover = Mover::new([0, 0, 0], 0);
        assert_eq!(mover.turn_by(1, 100, 64, 0), 36);
        assert_eq!(mover.turn_by(0, 36, 64, 1), 0);
        assert_eq!(mover.turn, 8 + 3);
        assert_eq!(mover.flags, flag::NO_STEP);
    }

    #[test]
    fn an_actor_is_there_when_the_place_is_nearer_than_its_last_step() {
        let arc = arc();
        let mut mover = Mover::new([0, 0, 0], 0);
        mover.before = [0, 0, -30];
        // 40 away with a last step of 30: not there, unless someone stands on the goal.
        assert!(!mover.go_to(&arc, [0, 0, 40], 64, false, 0));
        assert!(mover.go_to(&arc, [0, 0, 40], 64, true, 1));
        assert!(mover.go_to(&arc, [0, 0, 29], 64, false, 2));
        // 80 away is not within the last step and 50 more, whoever stands there.
        assert!(!mover.go_to(&arc, [0, 0, 80], 64, true, 3));
        // Further than the routine's limit on one axis: never there.
        assert!(!mover.go_to(&arc, [0, 0, 5000], 64, true, 4));
        assert!(mover.within([0, 100, 0], 101));
        assert!(!mover.within([0, 100, 0], 100));
        assert!(!mover.within([0, 5000, 0], 6000));
    }
}
