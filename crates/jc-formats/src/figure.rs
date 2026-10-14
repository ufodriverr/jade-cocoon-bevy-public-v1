
use crate::action::{Point, Stage};
use crate::animation::Animation;
use crate::creatures::LINE;
use crate::mesh::{MeshBlock, MeshObject};
use crate::skeleton::{BoneScale, GROUPS, ONE, Placement, Scales, Shape, Skeleton};

/// The models a request names.
pub const SOURCES: usize = 3;
/// The species the count of a line knows: a byte above them is not counted here (the
/// game counts it into what lies behind its 211 counters).
const SPECIES: usize = 211;

/// The species that stand most often in a line of 48, with their shares of
/// [`ONE`] (`0x80033938`): the one with the most places first, of two with as many
/// the one with the lower number. Where fewer than three species stand in the line,
/// the places behind them have the first species again with no share. Each share is
/// the count times 4096 divided by the sum of the three counts, cut. Answers how many
/// species have a share.
pub fn most_common(line: &[u8; LINE]) -> (usize, [u8; SOURCES], [i32; SOURCES]) {
    let mut counts = [0i32; SPECIES];
    for &species in line {
        if let Some(count) = counts.get_mut(usize::from(species)) {
            *count += 1;
        }
    }
    let (mut species, mut shares) = ([0u8; SOURCES], [0i32; SOURCES]);
    let mut found = 0;
    'counting: for wanted in (1..=LINE as i32).rev() {
        for (number, count) in counts.iter_mut().enumerate() {
            if *count >= wanted {
                (species[found], shares[found]) = (number as u8, wanted);
                *count = 0;
                found += 1;
                if found == SOURCES {
                    break 'counting;
                }
            }
        }
    }
    for place in found..SOURCES {
        species[place] = species[0];
    }
    let sum: i32 = shares.iter().sum();
    if sum != 0 {
        shares = shares.map(|share| (share << 12) / sum);
    }
    (found, species, shares)
}

/// What the model job is asked for (`0x800C081C` of file 36 and its twin `0x800BF208`
/// of file 44): the words +0 to +24 and +40 of the request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Request {
    /// +0: how many of the models are mixed.
    pub count: usize,
    /// +4: the models. The first is the creature's own, whose mesh the others are
    /// mixed into.
    pub models: [u32; SOURCES],
    /// +16: their weights, of [`ONE`].
    pub weights: [i32; SOURCES],
    /// +40: the model whose textures the figure has, the one of the look's species.
    pub textures: u32,
}

impl Request {
    /// The request of a creature of the species `own` with the look `look` and the
    /// line `line`; `model_of` is the model of a species (`model::species_model`).
    /// `None` when a species has no model.
    ///
    /// The three species of [`most_common`] are looked up. Two places with one model
    /// become one place with both weights, and the count goes down. Then the creature's
    /// own model comes to the first place: it changes places with the place that has
    /// it, and when none has it, it takes the first place with the weight 0, what was
    /// there goes to the place behind the last one counted (the third when two or three
    /// were counted, and what stood there is lost), and the count goes up to at most
    /// three.
    pub fn of(
        own: u32,
        look: u8,
        line: &[u8; LINE],
        model_of: impl Fn(u32) -> Option<u32>,
    ) -> Option<Self> {
        let (found, species, shares) = most_common(line);
        let mut m = [0u32; SOURCES];
        for (model, species) in m.iter_mut().zip(species) {
            *model = model_of(species.into())?;
        }
        let (mut w, mut count) = (shares, found);
        if m[0] == m[1] {
            if m[1] == m[2] {
                (w, count) = ([w[0] + w[1] + w[2], 0, 0], 1);
            } else {
                (w, count) = ([w[0] + w[1], w[2], 0], 2);
                m[1] = m[2];
            }
        } else if m[0] == m[2] {
            (w, count) = ([w[0] + w[2], w[1], 0], 2);
        } else if m[1] == m[2] {
            (w, count) = ([w[0], w[1] + w[2], 0], 2);
        }
        let own = model_of(own)?;
        if m[0] != own {
            if m[1] == own {
                m.swap(0, 1);
                w.swap(0, 1);
            } else if m[2] == own {
                m.swap(0, 2);
                w.swap(0, 2);
            } else {
                let behind = if count < 2 { 1 } else { 2 };
                (m[behind], w[behind]) = (m[0], w[0]);
                (m[0], w[0]) = (own, 0);
                count = behind + 1;
            }
        }
        Some(Self {
            count,
            models: m,
            weights: w,
            textures: model_of(look.into())?,
        })
    }

    /// Whether anything is mixed into the creature's own model.
    pub fn is_mixed(&self) -> bool {
        self.count > 1
    }

    /// The weights the mix of a mesh object goes by (`0x80010EAC` to `0x80010FC0` of
    /// the job's second state). `words` are the words the stages of the three models
    /// have for the object (`Stage::shares`). Where the three are the same, which they
    /// are in every model but three, the weights are the request's. Else each word is
    /// brought to its share of 4096 among the three and multiplied by the request's
    /// weight. Then, in both cases, the three are brought to shares of 4096. A share
    /// is the value times 4096 divided by the sum of the three, cut, where the sum of
    /// a later one is made with the earlier ones as they came out; the console counts
    /// in 32 bits, which a product of two shares times 4096 runs over. A sum of 0
    /// leaves 0 (HYPOTHESIS: the console's division by 0 was not followed).
    pub fn mesh_weights(&self, words: [i32; SOURCES]) -> [i32; SOURCES] {
        let shares = |mut w: [i32; SOURCES]| {
            for place in 0..SOURCES {
                let sum = w[0].wrapping_add(w[1]).wrapping_add(w[2]);
                w[place] = (w[place].wrapping_shl(12)).checked_div(sum).unwrap_or(0);
            }
            w
        };
        let mut w = self.weights;
        if words[0] != words[1] || words[1] != words[2] {
            let of = shares(words);
            w = std::array::from_fn(|place| of[place].wrapping_mul(self.weights[place]));
        }
        shares(w)
    }

    /// The mesh of the figure: `own`, the mesh of the first model, with the meshes of
    /// the second and the third model (`others`) mixed in, object by object: first the
    /// second model's object, then the third's into what that left, each by its weight
    /// of [`Self::mesh_weights`] for the words of that object in `words`, the tables of
    /// the three models' stages (an object behind a table's end has the word 0). A
    /// model behind the count, or with the weight 0, is left out. The models that can
    /// be mixed have the same faces in the same order; of two blocks that differ, what
    /// has no partner stays. Answers how many models were mixed into an object.
    pub fn mix(
        &self,
        own: &mut MeshBlock,
        others: [&MeshBlock; SOURCES - 1],
        words: [&[i32]; SOURCES],
    ) -> usize {
        let mut mixed = [false; SOURCES - 1];
        for (index, object) in own.objects.iter_mut().enumerate() {
            let word = |place: usize| words[place].get(index).copied().unwrap_or(0);
            let weights = self.mesh_weights([word(0), word(1), word(2)]);
            for (place, other) in others.iter().enumerate() {
                let weight = weights[place + 1];
                if self.count > place + 1
                    && weight != 0
                    && let Some(theirs) = other.objects.get(index)
                {
                    object.mix(theirs, weight);
                    mixed[place] = true;
                }
            }
        }
        mixed.into_iter().filter(|mixed| *mixed).count()
    }

    /// The scale of the figure at a stage (`0x800110A8` to `0x800111B0`): of each of
    /// the three models the scale of that stage times its weight, divided by 4096 and
    /// cut towards zero, and the three added. All three models count, whatever the
    /// count of the request.
    pub fn scale(&self, scales: [[i32; 3]; SOURCES]) -> [i32; 3] {
        std::array::from_fn(|axis| {
            (scales.iter().zip(self.weights))
                .map(|(scale, weight)| scale[axis] * weight / ONE)
                .sum()
        })
    }
}

/// The points of a head that are mixed.
pub const MIXED_POINTS: usize = 43;
/// The kind of the point the figure is set on the ground by.
pub const GROUND_POINT: i16 = 38;

/// The part of a turn's way to another that a weight takes (`0x8004A704` to
/// `0x8004A740`): the shorter way round of 4096 to the circle, times the weight, divided
/// by 4096 and cut towards zero, and what the angle kept to the circle then differs by.
fn turned(own: i16, other: i16, weight: i32) -> i32 {
    let mut way = i32::from(other.wrapping_sub(own)) & 0xFFF;
    if way >= 2048 {
        way -= 4096;
    }
    let own = i32::from(own as u16);
    ((own + way * weight / ONE) & 0xFFF) - own
}

impl Request {
    pub fn shape(&self, skeleton: &Skeleton, firsts: [&Animation; SOURCES]) -> Option<Shape> {
        let bones = firsts[0].bones;
        let mut shape = Shape {
            rest: vec![[0; 3]; bones],
            turns: vec![0; bones],
        };
        let [_, second, third] = self.weights;
        for (index, bone) in skeleton.bones.iter().enumerate() {
            let number = usize::from(bone.number);
            if number >= bones {
                return None;
            }
            // Bone number 0 has nothing in a block: all three are taken as zero.
            let (mut rests, mut angles) = ([[0i16; 3]; SOURCES], [0i16; SOURCES]);
            if number != 0 {
                for (place, first) in firsts.iter().enumerate() {
                    rests[place] = first.rest(number)?;
                    angles[place] = first.angles(0, number)?[0];
                }
            }
            let turn = turned(angles[0], angles[1], second) + turned(angles[0], angles[2], third);
            shape.turns[number] = turn as i16;
            let mut rest: [i16; 3] = std::array::from_fn(|axis| {
                let own = i32::from(rests[0][axis]);
                let step = |other: i16, weight: i32| (i32::from(other) - own) * weight / ONE;
                (own + step(rests[1][axis], second) + step(rests[2][axis], third)) as i16
            });
            if index == 1 {
                rest[1] = rest[1].wrapping_sub(rests[0][1]);
            }
            shape.rest[number] = rest;
        }
        Some(shape)
    }

    /// The points of the figure's head (`0x80011904`): of each of the first 43 records
    /// the three values of the place are, in the console's floating point of 32 bits,
    /// the sum over the three models of the weight times 1/4096 times the model's value,
    /// cut to a whole number. Kind and bone stay the first model's. A record that only
    /// stands for another kind is mixed like every other, so with weights that make 4095
    /// together its -1 becomes 0 and it stands for nothing any more.
    pub fn points(&self, own: &mut [Point], others: [&[Point]; SOURCES - 1]) {
        for (index, point) in own.iter_mut().enumerate().take(MIXED_POINTS) {
            let (Some(second), Some(third)) = (others[0].get(index), others[1].get(index)) else {
                continue;
            };
            let places = [point.place, second.place, third.place];
            point.place = std::array::from_fn(|axis| {
                let mut sum = 0f32;
                for (place, weight) in places.iter().zip(self.weights) {
                    sum += weight as f32 * (1.0 / 4096.0) * f32::from(place[axis]);
                }
                sum as i32 as i16
            });
        }
    }
}

/// Sets the figure on the ground (the end of the job's second state, `0x800114DC` to
/// `0x800118C4`): the first model's second animation is posed at its first frame with the
/// shape and the bone scales of the first model's stage, without a scale of the model;
/// the point of kind [`GROUND_POINT`] is put through its bone's matrix, whose columns
/// are stretched by the scales of the bone's number, and through the bone's place; and
/// the height that comes out is taken from the shape's offset of the second bone of the
/// table. Answers that height, `None` when the point or its bone is not there.
///
/// HYPOTHESIS: the pose is made with the angles of the player's place record, of which
/// only the one about the upright axis is taken as set; a turn about that axis does not
/// change a height.
pub fn ground(
    shape: &mut Shape,
    skeleton: &Skeleton,
    standing: &Animation,
    bones: &[BoneScale],
    points: &[Point],
) -> Option<i32> {
    let scales = Scales { model: None, bones };
    let plain = Placement::IDENTITY.matrix;
    let pose = skeleton.pose_shaped_from(plain, standing, 0, &scales, Some(shape));
    let of = |kind: i16| points.iter().find(|point| point.kind == kind);
    let first = of(GROUND_POINT)?;
    let point = match first.stands_for() {
        Some(other) => of(other)?,
        None => first,
    };
    let index = usize::try_from(point.bone).ok()?;
    let placed = (*pose.get(index)?)?;
    let number = usize::from(skeleton.bones.get(index)?.number);
    let scale = (bones.get(number)).map_or([ONE; 3], |record| record.scale.map(i32::from));
    let stretched = Placement {
        matrix: (placed.matrix).map(|row| std::array::from_fn(|c| row[c] * scale[c] / ONE)),
        position: placed.position,
    };
    let height = stretched.place(point.place)[1];
    let root = usize::from(skeleton.bones.get(1)?.number);
    let rest = shape.rest.get_mut(root)?;
    rest[1] = rest[1].wrapping_sub(height as i16);
    Some(height)
}

/// The bone groups the figure shows (`0x80011378` to `0x800113EC`): a group the first
/// model's stage has with the value 1, not drawn, gets the value 2, drawn, when the
/// stage of the second or of the third model has it with 2 or more.
pub fn shown(own: &mut [u16; GROUPS], others: [&Stage; SOURCES - 1]) {
    for other in others {
        for (mine, theirs) in own.iter_mut().zip(other.shown) {
            if theirs >= 2 && *mine == 1 {
                *mine = 2;
            }
        }
    }
}

/// One value of a mix (`0x80039FE8` and `0x8003A6C8`): the difference of the two as a
/// half word, times the weight, divided by 4096 and cut downwards, added to the own
/// value and kept to a half word.
fn mixed(own: i16, other: i16, weight: i32) -> i16 {
    let step = (i32::from(other.wrapping_sub(own)) * weight) >> 12;
    (i32::from(own) + step).clamp(i16::MIN.into(), i16::MAX.into()) as i16
}

fn mix_vector(own: &mut [i16; 3], other: &[i16; 3], weight: i32) {
    for (mine, theirs) in own.iter_mut().zip(other) {
        *mine = mixed(*mine, *theirs, weight);
    }
}

impl MeshObject {
    /// Mixes `other` into the object by `weight` of 4096: every place and every normal
    /// of every face and of the seam pool, each with the one of the same face and
    /// corner of the other. Everything else stays the object's own.
    fn mix(&mut self, other: &MeshObject, weight: i32) {
        for (face, theirs) in self.faces.iter_mut().zip(&other.faces) {
            if let (Some(mine), Some(theirs)) = (&mut face.normal, &theirs.normal) {
                mix_vector(mine, theirs, weight);
            }
            for (corner, theirs) in face.corners.iter_mut().zip(&theirs.corners) {
                mix_vector(&mut corner.position, &theirs.position, weight);
                if let (Some(mine), Some(theirs)) = (&mut corner.normal, &theirs.normal) {
                    mix_vector(mine, theirs, weight);
                }
            }
        }
        for (vertex, theirs) in self.seam.iter_mut().zip(&other.seam) {
            mix_vector(&mut vertex.position, &theirs.position, weight);
            mix_vector(&mut vertex.normal, &theirs.normal, weight);
        }
    }
}

impl MeshBlock {
    /// Mixes the mesh objects of `other` into those of the block, object by object,
    /// all by one weight. Of two blocks that differ, what has no partner stays.
    pub fn mix(&mut self, other: &MeshBlock, weight: i32) {
        for (object, theirs) in self.objects.iter_mut().zip(&other.objects) {
            object.mix(theirs, weight);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::{Corner, Face, SeamVertex};

    /// Every species is its own model, but the species from 100 on have the model of
    /// the species 100 below them; species 99 has none.
    fn model_of(species: u32) -> Option<u32> {
        (species != 99).then_some(species % 100)
    }

    fn line(of: &[(u8, usize)]) -> [u8; LINE] {
        let mut line = [0; LINE];
        let mut at = 0;
        for &(species, times) in of {
            line[at..at + times].fill(species);
            at += times;
        }
        assert_eq!(at, LINE);
        line
    }

    #[test]
    fn the_three_species_with_the_most_places_and_their_shares() {
        // A creature that was never merged: itself, with everything.
        assert_eq!(most_common(&[7; LINE]), (1, [7, 7, 7], [4096, 0, 0]));
        // A first merge: 32 and 16 places. The shares are cut, so they make 4095.
        let two = line(&[(21, 16), (4, 32)]);
        assert_eq!(most_common(&two), (2, [4, 21, 4], [2730, 1365, 0]));
        // Three and more: the three with the most; of two with as many the lower.
        let four = line(&[(9, 12), (5, 12), (30, 20), (2, 4)]);
        assert_eq!(most_common(&four), (3, [30, 5, 9], [1861, 1117, 1117]));
    }

    #[test]
    fn a_request_has_the_creature_s_own_model_first() {
        let request = |own, look, of: &[(u8, usize)]| Request::of(own, look, &line(of), model_of);
        // Never merged: one model, nothing to mix.
        let alone = request(7, 7, &[(7, 48)]).expect("a request");
        assert_eq!(
            (alone.count, alone.models, alone.weights),
            (1, [7; 3], [4096, 0, 0])
        );
        assert!(!alone.is_mixed());
        // A first merge: its own model and the second's, the textures of the look.
        let first = request(4, 21, &[(4, 32), (21, 16)]).expect("a request");
        assert_eq!((first.count, first.models), (2, [4, 21, 4]));
        assert_eq!((first.weights, first.textures), ([2730, 1365, 0], 21));
        assert_eq!(first.mesh_weights([0; 3]), [2730, 1365, 0]);
        // Two species of one model are one place: 104 and 4 are both model 4.
        let folded = request(4, 4, &[(104, 20), (4, 16), (21, 12)]).expect("a request");
        assert_eq!((folded.count, folded.models), (2, [4, 21, 21]));
        assert_eq!(folded.weights, [1706 + 1365, 1024, 0]);
        let all = request(4, 4, &[(104, 20), (4, 28)]).expect("a request");
        assert_eq!((all.count, all.weights), (1, [2389 + 1706, 0, 0]));
        // Its own model is not the most common: it changes places with the first.
        let second = request(21, 21, &[(4, 32), (21, 16)]).expect("a request");
        assert_eq!(
            (second.models, second.weights),
            ([21, 4, 4], [1365, 2730, 0])
        );
        let third = request(9, 9, &[(30, 20), (5, 16), (9, 12)]).expect("a request");
        assert_eq!(third.models, [9, 5, 30]);
        assert_eq!(third.weights, [1024, 1365, 1706]);
        // Its own model is not in the line: it comes first with nothing, and the
        // most common goes behind the others, where the third is lost.
        let pushed = request(8, 8, &[(4, 48)]).expect("a request");
        assert_eq!((pushed.count, pushed.models), (2, [8, 4, 4]));
        assert_eq!(pushed.weights, [0, 4096, 0]);
        let lost = request(8, 8, &[(30, 20), (5, 16), (9, 12)]).expect("a request");
        assert_eq!((lost.count, lost.models), (3, [8, 5, 30]));
        assert_eq!(lost.weights, [0, 1365, 1706]);
        // The weights of a mesh are brought to 4096 again, one after the other.
        assert_eq!(lost.mesh_weights([0; 3]), [0, 1820, 1981]);
        // Words that are the same change nothing, whatever they are.
        assert_eq!(lost.mesh_weights([7; 3]), [0, 1820, 1981]);
        // Words that differ, as the disc has them, one model with a 2 and the others
        // with 0. The own model's: nothing of the others is mixed into the object.
        assert_eq!(first.mesh_weights([2, 0, 0])[1..], [0, 0]);
        // The second model's: its share is 4096, times its weight 1365, and that times
        // 4096 runs over the console's 32 bits, so the object is mixed by 255 of 4096
        // and not by the whole; with the weight 2730 it comes out below 0.
        assert_eq!(first.mesh_weights([0, 2, 0]), [0, 255, 0]);
        let second = request(21, 21, &[(4, 32), (21, 16)]).expect("a request");
        assert_eq!(second.mesh_weights([0, 2, 0]), [0, -129, 0]);
        // Each sum of a share is made with the shares that came out before it.
        assert_eq!(lost.mesh_weights([1, 1, 2]), [0, 1045, 3766]);
        // A species without a model: no request.
        assert!(request(99, 4, &[(4, 48)]).is_none());
        assert!(request(4, 99, &[(4, 48)]).is_none());
    }

    #[test]
    fn the_scale_and_the_groups_of_a_mixed_figure() {
        let request = Request {
            count: 2,
            models: [4, 21, 4],
            weights: [2730, 1365, 0],
            textures: 21,
        };
        // 2000 * 2730 / 4096 and 5000 * 1365 / 4096, each cut: 1333 and 1666.
        let scales = [[2000, 2000, -2000], [5000, 4096, -5000], [9999; 3]];
        assert_eq!(request.scale(scales), [2999, 2698, -2999]);
        let stage = |shown| Stage {
            scale: [ONE; 3],
            bones: Vec::new(),
            shown,
            marked: [0; GROUPS],
            shares: Vec::new(),
        };
        // A group that is not drawn is drawn when another model draws it; a 0 stays.
        let mut own = [1, 1, 0];
        shown(&mut own, [&stage([3, 1, 2]), &stage([0, 0, 0])]);
        assert_eq!(own, [2, 1, 0]);
        shown(&mut own, [&stage([0, 0, 0]), &stage([0, 2, 0])]);
        assert_eq!(own, [2, 2, 0]);
    }

    fn object(at: i16, normal: i16) -> MeshObject {
        let corner = |x: i16| Corner {
            position: [x, -x, at],
            normal: Some([normal, 0, 0]),
            colour: None,
            uv: Some([1, 2]),
        };
        MeshObject {
            faces: vec![Face {
                kind: 3,
                command: 0x34,
                texture: None,
                colour: None,
                normal: Some([0, normal, 0]),
                corners: vec![corner(at), corner(at + 10), corner(at + 20)],
            }],
            seam: vec![SeamVertex {
                position: [at, at, at],
                normal: [0, 0, normal],
                colour: [128; 3],
                uv: [0; 2],
                texture: None,
            }],
        }
    }

    #[test]
    fn a_mix_moves_every_place_and_normal_by_the_weight() {
        // Half of a step of 7 is cut downwards: 3 on the way up, -4 on the way down.
        assert_eq!(mixed(10, 17, 2048), 13);
        assert_eq!(mixed(10, 3, 2048), 6);
        assert_eq!(mixed(100, 200, 0), 100);
        assert_eq!(mixed(100, 200, 4096), 200);
        // The difference is a half word, and the sum is kept to one.
        assert_eq!(mixed(30000, -30000, 4096), 32767);

        let block = |at, normal| MeshBlock {
            objects: vec![object(at, normal), object(at + 100, normal)],
            stitches: Vec::new(),
            len: 0,
        };
        let request = Request {
            count: 2,
            models: [1, 2, 1],
            weights: [3072, 1024, 0],
            textures: 1,
        };
        let (mut own, second, third) = (block(0, 4096), block(400, 0), block(-9000, 0));
        assert_eq!(request.mix(&mut own, [&second, &third], [&[]; 3]), 1);
        // A quarter of the way to the second; the third is behind the count.
        let face = &own.objects[0].faces[0];
        assert_eq!(face.normal, Some([0, 3072, 0]));
        assert_eq!(face.corners[1].position, [110, -110, 100]);
        assert_eq!(face.corners[1].normal, Some([3072, 0, 0]));
        assert_eq!(face.corners[1].uv, Some([1, 2]));
        let vertex = own.objects[1].seam[0];
        assert_eq!((vertex.position, vertex.normal), ([200; 3], [0, 0, 3072]));
        // With three, the third is mixed into what the second left.
        let three = Request {
            count: 3,
            weights: [2048, 1024, 1024],
            ..request
        };
        let mut own = block(0, 4096);
        assert_eq!(three.mix(&mut own, [&second, &third], [&[]; 3]), 2);
        // 0 + 400 / 4 = 100, then 100 + (-9000 - 100) / 4 = -2175.
        assert_eq!(own.objects[0].seam[0].position, [-2175; 3]);
    }

    /// A bone table of three bones in a row, numbered in order.
    fn skeleton() -> Skeleton {
        let mut data = Vec::new();
        for (depth, number) in [(0i16, 0u8), (1, 1), (2, 2)] {
            data.extend(depth.to_le_bytes());
            data.extend([number, number.saturating_sub(1)]);
            data.extend(i16::from(number).to_le_bytes());
            data.extend(2u16.to_le_bytes());
        }
        data.extend((-1i16).to_le_bytes());
        data.extend([0; 6]);
        Skeleton::parse(&data).unwrap()
    }

    /// One frame: per bone a rest offset and angles, and the frame's second entry.
    fn animation(root: [i16; 3], bones: &[([i16; 3], [i16; 3])]) -> Animation {
        let mut data = Vec::new();
        data.extend(1u32.to_le_bytes());
        data.extend((bones.len() as u32).to_le_bytes());
        let entries = (bones.iter().map(|bone| bone.0))
            .chain([[0; 3], root])
            .chain(bones.iter().map(|bone| bone.1));
        for value in entries.flatten() {
            data.extend(value.to_le_bytes());
        }
        data.resize(data.len().next_multiple_of(4), 0);
        Animation::parse(&data).unwrap()
    }

    #[test]
    fn the_skeleton_of_a_figure_is_mixed_by_the_weights() {
        // The shorter way round: from 4000 to 100 is 196 on, from 100 to 4000 196 back.
        // The angle is kept to the circle before the difference is taken, so a way
        // over the full turn comes out a full turn less: the same turn.
        assert_eq!(turned(4000, 100, 2048), 98 - 4096);
        assert_eq!(turned(100, 4000, 2048), -98);
        assert_eq!(turned(4000, 100, 4096), 100 - 4000);
        assert_eq!(turned(7, 7, 4096), 0);

        let none = [0; 3];
        let own = animation(
            [0, -100, 0],
            &[
                (none, none),
                ([0, -100, 0], none),
                ([10, -40, 0], [100, 5, 6]),
            ],
        );
        let second = animation(
            [0, -300, 0],
            &[
                (none, none),
                ([0, -300, 0], none),
                ([30, -80, 7], [300, 9, 9]),
            ],
        );
        let third = animation(
            [0, -500, 0],
            &[
                (none, none),
                ([8, -500, 0], none),
                ([-10, -40, -9], [4000, 0, 0]),
            ],
        );
        let request = |weights| Request {
            count: 3,
            models: [1, 2, 3],
            weights,
            textures: 1,
        };
        // Never merged: its own offsets, no turn, and the second bone's height is 0.
        let alone = request([4096, 0, 0]).shape(&skeleton(), [&own; 3]).unwrap();
        assert_eq!(alone.rest, [none, none, [10, -40, 0]]);
        assert_eq!(alone.turns, [0, 0, 0]);
        // Half of the second and a quarter of the third, each cut towards zero.
        let mixed = request([1024, 2048, 1024])
            .shape(&skeleton(), [&own, &second, &third])
            .unwrap();
        // The second bone: 2 of the 8 to the side, and of the height only what was
        // added: -100 and -100.
        assert_eq!(mixed.rest[1], [2, -200, 0]);
        // 10 + 10 - 5, -40 - 20 + 0, 0 + 3 - 2 (the -9 quarter is cut to -2).
        assert_eq!(mixed.rest[2], [15, -60, 1]);
        // Half of 200 on and a quarter of the 196 back: 100 - 49.
        assert_eq!(mixed.turns, [0, 0, 51]);
        // An animation with fewer bones than the table: no shape.
        let short = animation([0; 3], &[(none, none), (none, none)]);
        assert!(
            request([4096, 0, 0])
                .shape(&skeleton(), [&short; 3])
                .is_none()
        );
    }

    #[test]
    fn the_points_are_mixed_and_the_figure_is_set_on_the_ground() {
        let point = |kind, bone, place| Point {
            kind,
            bone,
            place,
            last: 0,
        };
        let stands = Point {
            kind: 5,
            bone: GROUND_POINT,
            place: [-1; 3],
            last: -1,
        };
        let list = |at: i16| vec![point(0, 0, [0; 3]), point(38, 2, [at, at * 2, -at]), stands];
        let request = |weights| Request {
            count: 2,
            models: [1, 2, 1],
            weights,
            textures: 1,
        };
        // Weights that make 4096: a quarter of the way, and a record that stands for
        // another kind stays one.
        let mut own = list(100);
        request([3072, 1024, 0]).points(&mut own, [&list(200), &list(-7000)]);
        assert_eq!(own[1].place, [125, 250, -125]);
        assert_eq!(own[2].stands_for(), Some(GROUND_POINT));
        // Weights that make 4095: every value is cut, and the -1 becomes 0.
        let mut own = list(100);
        request([2730, 1365, 0]).points(&mut own, [&list(100), &list(0)]);
        assert_eq!(own[1].place, [99, 199, -99]);
        assert_eq!((own[2].place, own[2].stands_for()), ([0; 3], None));

        // The ground: bone 2 stands at -100 - 20 - 60 with the shape below, stretched
        // to half its length below the second bone, and the point 30 below it.
        let none = [0; 3];
        let standing = animation(
            [0, -100, 0],
            &[(none, none), ([0, -100, 0], none), ([0, -50, 0], none)],
        );
        let mut shape = Shape {
            rest: vec![none, [0, -20, 0], [0, -120, 0]],
            turns: vec![0; 3],
        };
        let half = BoneScale {
            scale: [4096, 2048, 4096],
            used: 0,
        };
        let bones = [BoneScale::NONE, half, half];
        let points = [point(0, 0, none), stands, point(38, 2, [0, 60, 0])];
        // -120 for the second bone, half of -120 for the third, half of 60 for the point.
        let height = ground(&mut shape, &skeleton(), &standing, &bones, &points);
        assert_eq!(height, Some(-150));
        assert_eq!(shape.rest[1], [0, 130, 0]);
        // No point of the kind: nothing is done.
        assert_eq!(
            ground(&mut shape, &skeleton(), &standing, &bones, &points[..1]),
            None
        );
    }
}
