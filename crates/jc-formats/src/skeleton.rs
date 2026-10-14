
use std::f64::consts::TAU;
use std::fmt;

use crate::animation::Animation;

/// RAM address of the bone table the executable gives every creature (25 bones).
pub const CREATURE_SKELETON: u32 = 0x8007_9064;
/// RAM address of the people's bone table in the field engine (22 bones), see
/// [`crate::rooms::FIELD_ENGINE_FILE`].
pub const HUMAN_SKELETON: u32 = 0x800B_10C8;
pub const MODEL_LIST_FILE: usize = 25;
pub const MODEL_LIST_BASE: u32 = 0x800A_0530;
pub const MODEL_LIST: u32 = 0x800A_3340;
/// Bytes of an entry of the model list, and more entries than this is not a list.
const LIST_RECORD: usize = 12;
const MAX_LISTED: usize = 1024;
/// Bytes of a bone record.
const RECORD: usize = 8;
/// Depth of the record that ends a table.
const END: i16 = -1;
/// More bones than this is not a table.
const MAX_BONES: usize = 256;
/// One in the game's fixed point, and the bits behind the point.
pub const ONE: i32 = 4096;
const POINT: u32 = 12;
/// Bit of a bone's flags that says its mesh object is drawn.
const DRAWN: u16 = 2;
/// Bit of a bone's flags that says its mesh object takes the light of the room.
const LIT: u16 = 1;
/// Bit of a bone's flags that a creature's stage sets for the bones of a marked group.
const MARKED: u16 = 0x10;
/// RAM address in the executable of the three bone groups a creature's stage shows or marks:
/// 32-bit masks with a bit per bone, by the bone's place in the table.
pub const BONE_GROUPS: u32 = 0x8007_2F60;
/// The bone groups of a stage.
pub const GROUPS: usize = 3;
/// A group is drawn when its "shown" value is this or more.
const SHOWN_FROM: u16 = 2;

/// Why bytes are not a bone table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkeletonError {
    /// The table has no end record within the bytes, or the address is outside the image.
    NoEnd,
    /// A bone lies deeper than one level below the bone before it.
    BadDepth(usize),
}

impl fmt::Display for SkeletonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoEnd => write!(f, "bone table without an end"),
            Self::BadDepth(bone) => write!(f, "bone {bone} hangs below no bone"),
        }
    }
}

impl std::error::Error for SkeletonError {}

/// One record of a bone table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bone {
    /// Level in the tree; the bone hangs on the last bone before it that is one level up.
    pub depth: i16,
    /// The bone's number in an animation block.
    pub number: u8,
    /// Which record of the bone scales stretches this bone's offset.
    pub scale: u8,
    /// The mesh object that moves with the bone.
    pub mesh: i16,
    pub flags: u16,
}

impl Bone {
    /// Whether the game draws the bone's mesh object.
    pub fn is_drawn(&self) -> bool {
        self.flags & DRAWN != 0
    }

    pub fn is_lit(&self) -> bool {
        self.flags & LIT != 0
    }

    pub fn is_marked(&self) -> bool {
        self.flags & MARKED != 0
    }
}

/// One record of a list of bone scales: how far a bone is stretched along its own axes,
/// [`ONE`] being its stored size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoneScale {
    pub scale: [i16; 3],
    /// The bone's own mesh object is only stretched when this is not zero.
    pub used: i16,
}

impl BoneScale {
    /// The record an actor without a list gets for every bone: nothing is stretched.
    pub const NONE: Self = Self {
        scale: [ONE as i16; 3],
        used: 0,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scales<'a> {
    /// The scale of the whole model, when it has one.
    pub model: Option<[i32; 3]>,
    /// The bone scales by record; a bone whose record is missing is not stretched.
    pub bones: &'a [BoneScale],
}

impl Scales<'_> {
    /// No scale at all.
    pub const NONE: Scales<'static> = Scales {
        model: None,
        bones: &[],
    };

    fn record(&self, index: u8) -> BoneScale {
        (self.bones.get(index as usize).copied()).unwrap_or(BoneScale::NONE)
    }

    /// A bone's offset against its parent as the pose uses it: stretched by the model's scale
    /// and, below the top level, by the record that the bone's scale byte names.
    pub fn offset(&self, bone: &Bone, offset: [i16; 3]) -> [i32; 3] {
        let mut offset = offset.map(i32::from);
        if let Some(model) = self.model {
            // The matrix unit takes 16 bits of each value and rounds down.
            offset = std::array::from_fn(|k| (offset[k] * model[k] as i16 as i32) >> POINT);
        }
        if bone.depth > 0 {
            let record = self.record(bone.scale);
            // The game's division: towards zero.
            offset = std::array::from_fn(|k| offset[k] * record.scale[k] as i32 / ONE);
        }
        offset
    }

    pub fn offset_mixed(&self, bone: &Bone, offset: [i16; 3]) -> [i32; 3] {
        let mut offset = offset.map(i32::from);
        if bone.depth > 0 {
            let record = self.record(bone.scale);
            offset = std::array::from_fn(|k| offset[k] * record.scale[k] as i32 / ONE);
        }
        if let Some(model) = self.model {
            offset = std::array::from_fn(|k| offset[k].wrapping_mul(model[k]) / ONE);
        }
        offset
    }

    /// The placement a bone's mesh object is drawn with: the columns of the bone's matrix
    /// stretched by the record of the bone's own number, then by the model's scale. The bones
    /// below build on the unstretched placement, not on this one.
    ///
    /// The game does this on the bone's matrix for the screen, the camera's times the bone's,
    /// which comes to the same as doing it here. The model's scale is the same on its three
    /// axes in every stage of the disc, so along which axes of the matrix the game applies it
    /// does not show; it is applied to the columns like the bone's.
    pub fn drawn(&self, bone: &Bone, placement: &Placement) -> Placement {
        let mut matrix = placement.matrix;
        let record = self.record(bone.number);
        if record.used != 0 {
            matrix =
                matrix.map(|row| std::array::from_fn(|c| row[c] * record.scale[c] as i32 / ONE));
        }
        if let Some(model) = self.model {
            matrix = matrix
                .map(|row| std::array::from_fn(|c| (row[c] * model[c] as i16 as i32) >> POINT));
        }
        Placement {
            matrix,
            position: placement.position,
        }
    }
}

/// Reads the masks of the three bone groups at [`BONE_GROUPS`] from the executable's image.
pub fn bone_groups(image: &[u8], base: u32) -> Option<[u32; GROUPS]> {
    let start = BONE_GROUPS.checked_sub(base)? as usize;
    let bytes = image.get(start..start + 4 * GROUPS)?;
    Some(std::array::from_fn(|group| {
        let at = 4 * group;
        u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
    }))
}

/// A turn and a place: what the pose gives a bone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    /// Rows of the rotation, [`ONE`] being one.
    pub matrix: [[i32; 3]; 3],
    pub position: [i32; 3],
}

impl Placement {
    pub const IDENTITY: Self = Self {
        matrix: [[ONE, 0, 0], [0, ONE, 0], [0, 0, ONE]],
        position: [0, 0, 0],
    };

    /// A direction of the bone's space in the model's.
    pub fn turn(&self, v: [i32; 3]) -> [i32; 3] {
        self.matrix.map(|row| {
            let sum: i64 = (0..3).map(|k| row[k] as i64 * v[k] as i64).sum();
            (sum >> POINT) as i32
        })
    }

    /// A point of the bone's space in the model's.
    pub fn place(&self, v: [i16; 3]) -> [i32; 3] {
        let turned = self.turn(v.map(i32::from));
        std::array::from_fn(|k| self.position[k] + turned[k])
    }

    /// The placement of a bone that turns by `matrix` at `offset` in this one's space.
    fn child(&self, matrix: [[i32; 3]; 3], offset: [i32; 3]) -> Self {
        // Columns of the child's turn, each turned by the parent.
        let columns: [[i32; 3]; 3] =
            std::array::from_fn(|c| self.turn(std::array::from_fn(|r| matrix[r][c])));
        let turned = self.turn(offset);
        Self {
            matrix: std::array::from_fn(|r| std::array::from_fn(|c| columns[c][r])),
            position: std::array::from_fn(|k| self.position[k] + turned[k]),
        }
    }
}

/// The game's sine of an angle of 4096 to the full turn, [`ONE`] being one. The executable
/// holds it as a table of 4096 values, which are these.
pub fn sine(angle: i32) -> i32 {
    let turn = (angle & 0xFFF) as f64 / 4096.0;
    (ONE as f64 * (turn * TAU).sin()).round() as i32
}

pub fn cosine(angle: i32) -> i32 {
    sine(angle + 0x400)
}

/// The rotation of a bone's three angles, as the game's pose routine builds it.
pub fn rotation(angles: [i16; 3]) -> [[i32; 3]; 3] {
    let [s0, s1, s2] = angles.map(|angle| sine(angle as i32));
    let [c0, c1, c2] = angles.map(|angle| cosine(angle as i32));
    let mul = |a: i32, b: i32| (a * b) >> POINT;
    [
        [
            mul(c2, c1),
            mul(mul(c2, s1), s0) - mul(s2, c0),
            mul(s2, s0) + mul(mul(c2, s1), c0),
        ],
        [
            mul(s2, c1),
            mul(c2, c0) + mul(mul(s2, s1), s0),
            mul(mul(s2, s1), c0) - mul(c2, s0),
        ],
        [-s1, mul(c1, s0), mul(c1, c0)],
    ]
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Shape {
    /// The rest offsets the bones take in place of those of the animation that plays. The
    /// one of the second bone of the table is not an offset: its height is added to where
    /// the frame puts that bone.
    pub rest: Vec<[i16; 3]>,
    /// A turn about x that every bone gets behind its own, 4096 to the full turn.
    pub turns: Vec<i16>,
}

/// The rotation of a bone's three angles with the turn of a [`Shape`] behind it, as the
/// branch of the pose routine for a second block builds it (`0x80046E3C` to `0x800471DC`):
/// the first row is the one of [`rotation`], and the second and the third are those two
/// turned about x by `turn`, every product shifted by itself. No turn gives [`rotation`].
pub fn rotation_turned(angles: [i16; 3], turn: i16) -> [[i32; 3]; 3] {
    let [first, second, third] = rotation(angles);
    let (s, c) = (sine(turn as i32), cosine(turn as i32));
    let mul = |a: i32, b: i32| (a * b) >> POINT;
    [
        first,
        std::array::from_fn(|k| mul(second[k], c) - mul(third[k], s)),
        std::array::from_fn(|k| mul(second[k], s) + mul(third[k], c)),
    ]
}

pub fn rotation_yxz(angles: [i16; 3]) -> [[i32; 3]; 3] {
    let [sx, sy, sz] = angles.map(|angle| sine(angle as i32));
    let [cx, cy, cz] = angles.map(|angle| cosine(angle as i32));
    let mul = |a: i32, b: i32| (a * b) >> POINT;
    [
        [
            mul(cy, cz) + mul(mul(sy, sx), sz),
            mul(mul(sy, sx), cz) - mul(cy, sz),
            mul(sy, cx),
        ],
        [mul(cx, sz), mul(cx, cz), -sx],
        [
            mul(mul(cy, sx), sz) - mul(sy, cz),
            mul(sy, sz) + mul(mul(cy, sx), cz),
            mul(cy, cx),
        ],
    ]
}

pub fn rotation_zxy(angles: [i16; 3]) -> [[i32; 3]; 3] {
    let [sx, sy, sz] = angles.map(|angle| sine(angle as i32));
    let [cx, cy, cz] = angles.map(|angle| cosine(angle as i32));
    let mul = |a: i32, b: i32| (a * b) >> POINT;
    [
        [
            mul(cz, cy) - mul(mul(sz, sx), sy),
            -mul(sz, cx),
            mul(cz, sy) + mul(mul(sz, sx), cy),
        ],
        [
            mul(sz, cy) + mul(mul(cz, sx), sy),
            mul(cz, cx),
            mul(sz, sy) - mul(mul(cz, sx), cy),
        ],
        [-mul(cx, sy), sx, mul(cx, cy)],
    ]
}

/// A bone table: the tree of a model's bones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skeleton {
    pub bones: Vec<Bone>,
}

impl Skeleton {
    /// Reads the records at the start of `data` up to the end record.
    pub fn parse(data: &[u8]) -> Result<Self, SkeletonError> {
        let mut bones = Vec::new();
        for record in data.as_chunks::<RECORD>().0.iter().take(MAX_BONES + 1) {
            let word = |at: usize| i16::from_le_bytes([record[at], record[at + 1]]);
            let depth = word(0);
            if depth == END {
                return Ok(Self { bones });
            }
            // A bone hangs on one that came before it.
            let above = bones.last().map_or(0, |bone: &Bone| bone.depth + 1);
            if depth < 0 || depth > above {
                return Err(SkeletonError::BadDepth(bones.len()));
            }
            bones.push(Bone {
                depth,
                number: record[2],
                scale: record[3],
                mesh: word(4),
                flags: word(6) as u16,
            });
        }
        Err(SkeletonError::NoEnd)
    }

    /// Reads the table at RAM address `address` of an image whose first byte is at `base`.
    pub fn at(image: &[u8], base: u32, address: u32) -> Result<Self, SkeletonError> {
        let start = address.checked_sub(base).ok_or(SkeletonError::NoEnd)? as usize;
        Self::parse(image.get(start..).ok_or(SkeletonError::NoEnd)?)
    }

    /// For every bone the index of the bone it hangs on: the last one before it that is one
    /// level up. None for a bone of the top level.
    pub fn parents(&self) -> Vec<Option<usize>> {
        let mut level: Vec<usize> = Vec::new();
        (self.bones.iter().enumerate())
            .map(|(index, bone)| {
                let depth = bone.depth as usize;
                level.truncate(depth);
                level.push(index);
                depth.checked_sub(1).map(|above| level[above])
            })
            .collect()
    }

    pub fn with_groups(
        &self,
        groups: [u32; GROUPS],
        shown: [u16; GROUPS],
        marked: [u16; GROUPS],
    ) -> Self {
        let mut bones = self.bones.clone();
        for (index, bone) in bones.iter_mut().enumerate().take(32) {
            for ((mask, shown), marked) in groups.iter().zip(shown).zip(marked) {
                if mask >> index & 1 == 0 {
                    continue;
                }
                if shown < SHOWN_FROM {
                    bone.flags &= !DRAWN;
                }
                if marked == 0 {
                    bone.flags |= MARKED;
                }
            }
        }
        Self { bones }
    }

    /// Where every bone stands in a frame of an animation, in the model's own space and
    /// without the scales a creature's growth adds. None for a bone the animation has no
    /// values for, and for the bones below such a bone.
    pub fn pose(&self, animation: &Animation, frame: usize) -> Vec<Option<Placement>> {
        self.pose_scaled(animation, frame, &Scales::NONE)
    }

    /// The pose with the scales of the model and its bones: they stretch the offsets between
    /// the bones. What stretches a bone's own mesh object is not in the result, because it is
    /// not handed down the tree: [`Scales::drawn`] gives it.
    pub fn pose_scaled(
        &self,
        animation: &Animation,
        frame: usize,
        scales: &Scales,
    ) -> Vec<Option<Placement>> {
        self.pose_from(Placement::IDENTITY.matrix, animation, frame, scales)
    }

    pub fn pose_from(
        &self,
        turn: [[i32; 3]; 3],
        animation: &Animation,
        frame: usize,
        scales: &Scales,
    ) -> Vec<Option<Placement>> {
        self.pose_shaped_from(turn, animation, frame, scales, None)
    }

    pub fn pose_shaped_from(
        &self,
        turn: [[i32; 3]; 3],
        animation: &Animation,
        frame: usize,
        scales: &Scales,
        shape: Option<&Shape>,
    ) -> Vec<Option<Placement>> {
        let root = Placement {
            matrix: turn,
            position: [0; 3],
        };
        self.placed(&root, |index, bone| {
            let (own, offset) = Self::bone_values(animation, frame, index, bone, shape)?;
            Some((own, scales.offset(bone, offset)))
        })
    }

    pub fn pose_faded(
        &self,
        new: (&Animation, usize),
        old: (&Animation, usize),
        old_share: i32,
    ) -> Vec<Option<Placement>> {
        self.pose_faded_scaled(new, old, old_share, &Scales::NONE)
    }

    /// [`Self::pose_faded`] with scales, applied to the mixed offsets
    /// ([`Scales::offset_mixed`]).
    pub fn pose_faded_scaled(
        &self,
        new: (&Animation, usize),
        old: (&Animation, usize),
        old_share: i32,
        scales: &Scales,
    ) -> Vec<Option<Placement>> {
        self.pose_faded_from(Placement::IDENTITY.matrix, new, old, old_share, scales)
    }

    /// [`Self::pose_faded_scaled`] of an actor that is turned by `turn`, as
    /// [`Self::pose_from`] is the pose of one animation: the routine for two animations
    /// starts with the same matrix of the actor's angles.
    pub fn pose_faded_from(
        &self,
        turn: [[i32; 3]; 3],
        new: (&Animation, usize),
        old: (&Animation, usize),
        old_share: i32,
        scales: &Scales,
    ) -> Vec<Option<Placement>> {
        self.pose_faded_shaped_from(turn, new, old, old_share, scales, None)
    }

    /// [`Self::pose_faded_from`] with a [`Shape`]: the offsets are then not mixed, they
    /// are the shape's with the new animation's frame for the second bone, as in
    /// [`Self::pose_shaped_from`] (`0x8004956C` to `0x8004973C`), and each of the two
    /// turns gets the shape's turn behind it before they are mixed (`0x800482B4` to
    /// `0x80048930`).
    pub fn pose_faded_shaped_from(
        &self,
        turn: [[i32; 3]; 3],
        new: (&Animation, usize),
        old: (&Animation, usize),
        old_share: i32,
        scales: &Scales,
        shape: Option<&Shape>,
    ) -> Vec<Option<Placement>> {
        let root = Placement {
            matrix: turn,
            position: [0; 3],
        };
        self.placed(&root, |index, bone| {
            let (own, offset) = Self::bone_values(new.0, new.1, index, bone, shape)?;
            let (old_own, old_offset) = Self::bone_values(old.0, old.1, index, bone, shape)?;
            let offset = match shape {
                Some(_) => offset,
                None => std::array::from_fn(|k| {
                    let (from, to) = (offset[k] as i32, old_offset[k] as i32);
                    // The game's division: towards zero.
                    (from + (to - from) * old_share / ONE) as i16
                }),
            };
            let turn = turn_towards(own, old_own, old_share);
            Some((turn, scales.offset_mixed(bone, offset)))
        })
    }

    /// The turn and the offset an animation's frame gives the bone at `index` of the
    /// table, with those of a [`Shape`] when the pose is given one.
    fn bone_values(
        animation: &Animation,
        frame: usize,
        index: usize,
        bone: &Bone,
        shape: Option<&Shape>,
    ) -> Option<([[i32; 3]; 3], [i16; 3])> {
        let number = bone.number as usize;
        // Bone number 0 neither turns nor moves, whatever the block says.
        let (angles, rest, turn) = match (number, shape) {
            (0, _) => ([0; 3], [0; 3], 0),
            (_, None) => (animation.angles(frame, number)?, animation.rest(number)?, 0),
            (_, Some(shape)) => (
                animation.angles(frame, number)?,
                *shape.rest.get(number)?,
                *shape.turns.get(number)?,
            ),
        };
        // The second bone of the table is moved by the frame, not by a rest offset; a
        // shape's offset lifts it.
        let offset = match (index, shape) {
            (1, None) => animation.root(frame)?,
            (1, Some(_)) => {
                let root = animation.root(frame)?;
                [root[0], root[1].wrapping_add(rest[1]), root[2]]
            }
            _ => rest,
        };
        let own = match shape {
            Some(_) => rotation_turned(angles, turn),
            None => rotation(angles),
        };
        Some((own, offset))
    }

    /// Chains what `values` gives every bone (its turn and its offset against its parent,
    /// stretched by the scales) down the tree, the bones of the top level hanging on `root`.
    /// None for a bone without values and for the bones below it.
    fn placed(
        &self,
        root: &Placement,
        values: impl Fn(usize, &Bone) -> Option<([[i32; 3]; 3], [i32; 3])>,
    ) -> Vec<Option<Placement>> {
        let parents = self.parents();
        let mut placed: Vec<Option<Placement>> = Vec::with_capacity(self.bones.len());
        for (index, bone) in self.bones.iter().enumerate() {
            let placement = (|| {
                let above = match parents[index] {
                    Some(parent) => placed[parent]?,
                    None => *root,
                };
                let (matrix, offset) = values(index, bone)?;
                Some(above.child(matrix, offset))
            })();
            placed.push(placement);
        }
        placed
    }
}

/// The rotation that lies `share` of [`ONE`] of the way from `from` to `to`: `from`, turned on
/// about the axis of the turn that leads to `to` by that share of its angle. The angle is
/// counted in 4096ths of a turn and the share of it cut to a whole number, as the game does;
/// a share that comes to nothing leaves `from` as it is.
fn turn_towards(from: [[i32; 3]; 3], to: [[i32; 3]; 3], share: i32) -> [[i32; 3]; 3] {
    if from == to {
        return from;
    }
    let real = |m: [[i32; 3]; 3]| m.map(|row| row.map(|value| value as f64 / ONE as f64));
    let (a, b) = (real(from), real(to));
    // The turn from `from` to `to`, seen from `from`: its transpose times `to`.
    let between: [[f64; 3]; 3] =
        std::array::from_fn(|r| std::array::from_fn(|c| (0..3).map(|k| a[k][r] * b[k][c]).sum()));
    let mut axis = [
        between[2][1] - between[1][2],
        between[0][2] - between[2][0],
        between[1][0] - between[0][1],
    ];
    let sine = axis.iter().map(|part| part * part).sum::<f64>().sqrt() / 2.0;
    let cosine = (between[0][0] + between[1][1] + between[2][2] - 1.0) / 2.0;
    if sine < 1e-6 {
        if cosine > 0.0 {
            return from;
        }
        // A half turn: its axis is the column of the turn plus one that is longest.
        let columns: [[f64; 3]; 3] = std::array::from_fn(|c| {
            std::array::from_fn(|r| between[r][c] + if r == c { 1.0 } else { 0.0 })
        });
        let length = |v: &[f64; 3]| v.iter().map(|part| part * part).sum::<f64>();
        axis = columns
            .into_iter()
            .max_by(|x, y| length(x).total_cmp(&length(y)))
            .expect("three columns");
    }
    let length = axis.iter().map(|part| part * part).sum::<f64>().sqrt();
    let [x, y, z] = axis.map(|part| part / length);
    let angle = (sine.atan2(cosine) / TAU * 4096.0).round() as i32;
    let part = angle * share / ONE;
    if part == 0 {
        return from;
    }
    let (s, c) = (part as f64 / 4096.0 * TAU).sin_cos();
    let t = 1.0 - c;
    let turn = [
        [t * x * x + c, t * x * y - s * z, t * x * z + s * y],
        [t * x * y + s * z, t * y * y + c, t * y * z - s * x],
        [t * x * z - s * y, t * y * z + s * x, t * z * z + c],
    ];
    std::array::from_fn(|r| {
        std::array::from_fn(|col| {
            let value: f64 = (0..3).map(|k| a[r][k] * turn[k][col]).sum();
            (value * ONE as f64).round() as i32
        })
    })
}

/// One entry of the model list: a model and the bone table the list gives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedModel {
    /// The model's number in the model table of the executable.
    pub model: usize,
    /// RAM address of the bone table; several models share one.
    pub table: u32,
    /// The developers' name of the model.
    pub name: String,
    pub skeleton: Skeleton,
}

/// The model list of the overlay in [`MODEL_LIST_FILE`]: which bone table a model is shown
/// with. The scene packages give their actors the same tables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelList {
    pub entries: Vec<ListedModel>,
}

impl ModelList {
    /// Reads the list at RAM address `address` of an image whose first byte is at `base`:
    /// entries of a model number, the address of a bone table and the address of a name,
    /// up to the entry with a negative model number.
    pub fn at(image: &[u8], base: u32, address: u32) -> Result<Self, SkeletonError> {
        let offset = |address: u32| address.checked_sub(base).map(|offset| offset as usize);
        let start = offset(address).ok_or(SkeletonError::NoEnd)?;
        let records = image.get(start..).ok_or(SkeletonError::NoEnd)?;
        let mut entries = Vec::new();
        for record in records.as_chunks::<LIST_RECORD>().0.iter().take(MAX_LISTED) {
            let word = |at: usize| {
                u32::from_le_bytes([record[at], record[at + 1], record[at + 2], record[at + 3]])
            };
            let Ok(model) = usize::try_from(word(0) as i32) else {
                return Ok(Self { entries });
            };
            let table = word(4);
            // A name is plain letters up to a zero byte; one that cannot be read is left empty.
            let name = (offset(word(8)).and_then(|start| image.get(start..)))
                .and_then(|bytes| bytes.split(|byte| *byte == 0).next())
                .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
                .unwrap_or_default();
            entries.push(ListedModel {
                model,
                table,
                name,
                skeleton: Skeleton::at(image, base, table)?,
            });
        }
        Err(SkeletonError::NoEnd)
    }

    /// The entry of a model, when the list has one.
    pub fn get(&self, model: usize) -> Option<&ListedModel> {
        self.entries.iter().find(|entry| entry.model == model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_model_list_with_its_tables() {
        const BASE: u32 = 0x8001_0000;
        // A name, two tables, then the list: two models share the first table.
        let mut image = b"MDL_ONE\0".to_vec();
        let first = BASE + image.len() as u32;
        image.extend(table(&[(0, 0, 0, 0, 0), (1, 1, 0, 1, 2)]));
        let second = BASE + image.len() as u32;
        image.extend(table(&[(0, 0, 0, 0, 0), (1, 2, 0, 2, 2), (2, 1, 1, 1, 2)]));
        let list = BASE + image.len() as u32;
        for (model, table, name) in [(7i32, first, BASE), (3, second, BASE + 4), (9, first, 0)] {
            image.extend(model.to_le_bytes());
            image.extend(table.to_le_bytes());
            image.extend(name.to_le_bytes());
        }
        let end = image.len();
        image.extend((-1i32).to_le_bytes());
        image.extend([0; 8]);

        let models = ModelList::at(&image, BASE, list).unwrap();
        assert_eq!(models.entries.len(), 3);
        let names: Vec<&str> = models.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["MDL_ONE", "ONE", ""]);
        assert_eq!(models.get(3).unwrap().skeleton.bones.len(), 3);
        assert_eq!(models.get(3).unwrap().skeleton.bones[1].number, 2);
        assert_eq!(models.get(9).unwrap().table, first);
        assert_eq!(
            models.get(9).unwrap().skeleton,
            models.get(7).unwrap().skeleton
        );
        assert!(models.get(4).is_none());
        // Without its end, or with a table outside the image, it is not a list.
        assert_eq!(
            ModelList::at(&image[..end], BASE, list),
            Err(SkeletonError::NoEnd)
        );
        image[end - 8..end - 4].copy_from_slice(&0x7000_0000u32.to_le_bytes());
        assert_eq!(ModelList::at(&image, BASE, list), Err(SkeletonError::NoEnd));
    }

    fn table(bones: &[(i16, u8, u8, i16, u16)]) -> Vec<u8> {
        let mut data = Vec::new();
        for &(depth, number, scale, mesh, flags) in bones {
            data.extend(depth.to_le_bytes());
            data.extend([number, scale]);
            data.extend(mesh.to_le_bytes());
            data.extend(flags.to_le_bytes());
        }
        data.extend(END.to_le_bytes());
        data.extend([0; 6]);
        data
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
    fn the_sine_is_the_fixed_point_one() {
        assert_eq!(
            [0, 1024, 2048, 3072, 4096, -1024].map(sine),
            [0, 4096, 0, -4096, 0, -4096]
        );
        assert_eq!([0, 1024, 2048].map(cosine), [4096, 0, -4096]);
        assert_eq!([1, 512, 4095].map(sine), [6, 2896, -6]);
        assert_eq!(rotation([0; 3]), Placement::IDENTITY.matrix);
    }

    #[test]
    fn the_other_rotation_turns_about_z_first() {
        assert_eq!(rotation_yxz([0; 3]), Placement::IDENTITY.matrix);
        let turned = |angles, v| {
            let placement = Placement {
                matrix: rotation_yxz(angles),
                position: [0; 3],
            };
            placement.place(v)
        };
        // Each angle alone: about x (y to z), about y (z to x), about z (x to y).
        assert_eq!(turned([1024, 0, 0], [0, 100, 0]), [0, 0, 100]);
        assert_eq!(turned([0, 1024, 0], [0, 0, 100]), [100, 0, 0]);
        assert_eq!(turned([0, 0, 1024], [100, 0, 0]), [0, 100, 0]);
        // Together: x to y about z, on to z about x, on to x about y.
        assert_eq!(turned([1024, 1024, 1024], [100, 0, 0]), [100, 0, 0]);
        assert_eq!(turned([1024, 0, 1024], [100, 0, 0]), [0, 0, 100]);
        // The bones' routine takes the other order: about x first, about z last.
        let bone = Placement {
            matrix: rotation([1024, 0, 1024]),
            position: [0; 3],
        };
        assert_eq!(bone.place([100, 0, 0]), [0, 100, 0]);
    }

    #[test]
    fn the_camera_rotation_turns_about_y_first() {
        assert_eq!(rotation_zxy([0; 3]), Placement::IDENTITY.matrix);
        let turned = |matrix, v| {
            let placement = Placement {
                matrix,
                position: [0; 3],
            };
            placement.place(v)
        };
        // Each angle alone is the turn of the other routine.
        for angles in [[300, 0, 0], [0, 500, 0], [0, 0, 700]] {
            assert_eq!(rotation_zxy(angles), rotation_yxz(angles));
        }
        // Together: z to x about y, where the turn about x leaves it.
        assert_eq!(
            turned(rotation_zxy([1024, 1024, 0]), [0, 0, 100]),
            [100, 0, 0]
        );
        // The other routine turns about x first: z to minus y, where the turn about y leaves it.
        assert_eq!(
            turned(rotation_yxz([1024, 1024, 0]), [0, 0, 100]),
            [0, -100, 0]
        );
    }

    #[test]
    fn each_angle_turns_about_one_axis() {
        let turned = |angles| {
            let placement = Placement {
                matrix: rotation(angles),
                position: [0; 3],
            };
            [[100, 0, 0], [0, 100, 0], [0, 0, 100]].map(|v| placement.place(v))
        };
        // The first angle turns about x (y to z), the second about y (z to x), the third
        // about z (x to y).
        assert_eq!(
            turned([1024, 0, 0]),
            [[100, 0, 0], [0, 0, 100], [0, -100, 0]]
        );
        assert_eq!(
            turned([0, 1024, 0]),
            [[0, 0, -100], [0, 100, 0], [100, 0, 0]]
        );
        assert_eq!(
            turned([0, 0, 1024]),
            [[0, 100, 0], [-100, 0, 0], [0, 0, 100]]
        );
    }

    #[test]
    fn reads_a_table_and_finds_parents_by_depth() {
        let data = table(&[
            (0, 0, 0, 0, 0),
            (1, 1, 0, 1, 70),
            (2, 2, 1, 2, 11),
            (3, 3, 2, 3, 11),
            (2, 4, 0, 4, 11),
            (1, 5, 0, -1, 0),
        ]);
        let skeleton = Skeleton::parse(&data).unwrap();
        assert_eq!(skeleton.bones.len(), 6);
        assert_eq!(skeleton.bones[4].scale, 0);
        assert_eq!(skeleton.bones[5].mesh, -1);
        assert!(skeleton.bones[2].is_drawn() && !skeleton.bones[0].is_drawn());
        // Bone 4 hangs on bone 1 although its scale byte names bone 0.
        assert_eq!(
            skeleton.parents(),
            [None, Some(0), Some(1), Some(2), Some(1), Some(0)]
        );
        assert_eq!(Skeleton::parse(&data[..40]), Err(SkeletonError::NoEnd));
        // The same table four bytes into an image.
        let image = [&[0xEE; 4][..], &data].concat();
        assert_eq!(Skeleton::at(&image, 0x8000_0000, 0x8000_0004), Ok(skeleton));
        assert_eq!(
            Skeleton::at(&image, 0x8000_0000, 0x7000_0000),
            Err(SkeletonError::NoEnd)
        );
        let jump = table(&[(0, 0, 0, 0, 0), (2, 1, 0, 1, 0)]);
        assert_eq!(Skeleton::parse(&jump), Err(SkeletonError::BadDepth(1)));
    }

    #[test]
    fn a_pose_chains_turns_and_offsets() {
        let skeleton = Skeleton::parse(&table(&[
            (0, 0, 0, 0, 0),
            (1, 1, 0, 1, 2),
            (2, 2, 1, 2, 2),
            (1, 3, 0, 3, 2),
            (2, 4, 3, 4, 2),
        ]))
        .unwrap();
        let animation = animation(
            [0, -100, 0],
            &[
                // Bone 0: what the block says is not used.
                ([9, 9, 9], [500, 500, 500]),
                // The second bone: placed by the frame, turned a quarter about z.
                ([5, 5, 5], [0, 0, 1024]),
                ([10, 0, 0], [0, 0, 1024]),
                ([0, 0, 7], [0, 0, 0]),
            ],
        );
        let pose = skeleton.pose(&animation, 0);
        assert_eq!(pose[0], Some(Placement::IDENTITY));
        let hip = pose[1].unwrap();
        assert_eq!(hip.position, [0, -100, 0]);
        assert_eq!(hip.place([10, 0, 0]), [0, -90, 0]);
        // Its child sits at its rest offset, turned by the parent, and turns on from there.
        let leg = pose[2].unwrap();
        assert_eq!(leg.position, [0, -90, 0]);
        assert_eq!(leg.place([1, 0, 0]), [-1, -90, 0]);
        // A second bone of level 1 hangs on bone 0 and uses its rest offset.
        assert_eq!(pose[3].unwrap().position, [0, 0, 7]);
        // The animation has no fifth bone.
        assert_eq!(pose[4], None);
        // A fade between an animation and itself is the animation.
        for share in [0, 1000, ONE] {
            assert_eq!(
                skeleton.pose_faded((&animation, 0), (&animation, 0), share),
                pose
            );
        }
        assert!(
            skeleton
                .pose(&animation, 1)
                .iter()
                .skip(1)
                .all(Option::is_none)
        );
        // An actor's turn is handed down from above the top level: a quarter about the
        // upright axis turns every place about the actor's own and every bone with it.
        let none = &Scales::NONE;
        let same = skeleton.pose_from(Placement::IDENTITY.matrix, &animation, 0, none);
        assert_eq!(same, pose);
        let quarter = rotation([0, 1024, 0]);
        let turned = skeleton.pose_from(quarter, &animation, 0, none);
        assert_eq!(turned[0].unwrap().matrix, quarter);
        assert_eq!(turned[1].unwrap().position, [0, -100, 0]);
        let at = pose[3].unwrap().position;
        let (root, there) = (turned[0].unwrap(), turned[3].unwrap());
        assert_eq!(there.position, root.turn(at));
        assert_ne!(there.position, at);
    }

    #[test]
    fn scales_stretch_offsets_and_drawn_bones_but_are_not_handed_down() {
        // Bone 2 hangs on bone 1; bone 3 hangs on bone 2 but its scale byte names record 1.
        let skeleton = Skeleton::parse(&table(&[
            (0, 0, 0, 0, 0),
            (1, 1, 0, 1, 2),
            (2, 2, 1, 2, 2),
            (3, 3, 1, 3, 2),
        ]))
        .unwrap();
        let animation = animation(
            [0, -100, 0],
            &[
                ([0; 3], [0; 3]),
                ([0; 3], [0; 3]),
                ([10, -7, 0], [0; 3]),
                ([0, 9, 0], [0; 3]),
            ],
        );
        let half = BoneScale {
            scale: [2048, 2048, 2048],
            used: 1,
        };
        let unused = BoneScale {
            scale: [8192, 4096, 4096],
            used: 0,
        };
        let bones = [BoneScale::NONE, half, unused, BoneScale::NONE];
        // Without scales the pose is the plain one.
        assert_eq!(
            skeleton.pose_scaled(&animation, 0, &Scales::NONE),
            skeleton.pose(&animation, 0)
        );
        let scales = Scales {
            model: None,
            bones: &bones,
        };
        let pose = skeleton.pose_scaled(&animation, 0, &scales);
        // The second bone is placed by the frame and stretched by record 0: not at all.
        assert_eq!(pose[1].unwrap().position, [0, -100, 0]);
        // Bone 2's offset is halved by record 1, towards zero: -7 / 2 is -3.
        assert_eq!(pose[2].unwrap().position, [5, -103, 0]);
        // Bone 3's offset is halved by record 1 as well, not by its parent's record 2.
        assert_eq!(pose[3].unwrap().position, [5, -99, 0]);
        // No matrix of the chain is stretched.
        assert!(
            pose.iter()
                .all(|p| p.unwrap().matrix == Placement::IDENTITY.matrix)
        );
        // A drawn bone is stretched by the record of its own number, when that is in use.
        let drawn = |bone: usize| scales.drawn(&skeleton.bones[bone], &pose[bone].unwrap());
        assert_eq!(drawn(1).place([100, 100, -100]), [50, -50, -50]);
        assert_eq!(drawn(2), pose[2].unwrap());
        assert_eq!(drawn(3), pose[3].unwrap());

        // The model's scale stretches every offset, the frame's too, and rounds down.
        let scales = Scales {
            model: Some([2048; 3]),
            bones: &bones,
        };
        let pose = skeleton.pose_scaled(&animation, 0, &scales);
        assert_eq!(pose[1].unwrap().position, [0, -50, 0]);
        // -7 halved and rounded down is -4, halved again towards zero -2.
        assert_eq!(pose[2].unwrap().position, [2, -52, 0]);
        let drawn = scales.drawn(&skeleton.bones[2], &pose[2].unwrap());
        assert_eq!(drawn.matrix, [[2048, 0, 0], [0, 2048, 0], [0, 0, 2048]]);
        assert_eq!(drawn.position, [2, -52, 0]);
        let drawn = scales.drawn(&skeleton.bones[1], &pose[1].unwrap());
        assert_eq!(drawn.matrix, [[1024, 0, 0], [0, 1024, 0], [0, 0, 1024]]);
    }

    #[test]
    fn a_stage_hides_and_marks_bone_groups() {
        let skeleton = Skeleton::parse(&table(&[
            (0, 0, 0, 0, 3),
            (1, 1, 0, 1, 3),
            (1, 2, 0, 2, 3),
            (1, 3, 0, 3, 3),
        ]))
        .unwrap();
        let masks = [0b0010, 0b0100, 0b1000];
        let staged = skeleton.with_groups(masks, [1, 2, 3], [1, 0, 1]);
        let drawn: Vec<bool> = staged.bones.iter().map(Bone::is_drawn).collect();
        let marked: Vec<bool> = staged.bones.iter().map(Bone::is_marked).collect();
        assert_eq!(drawn, [true, false, true, true]);
        assert_eq!(marked, [false, false, true, false]);
        // Nothing else changes.
        assert!(staged.bones.iter().all(Bone::is_lit));
        assert_eq!(skeleton.with_groups(masks, [2; 3], [1; 3]), skeleton);

        let mut image = vec![0; 8];
        image.extend(masks.iter().flat_map(|mask| mask.to_le_bytes()));
        assert_eq!(bone_groups(&image, BONE_GROUPS - 8), Some(masks));
        assert_eq!(bone_groups(&image[..19], BONE_GROUPS - 8), None);
        assert_eq!(bone_groups(&image, BONE_GROUPS + 4), None);
    }

    #[test]
    fn a_fade_turns_and_moves_a_bone_part_of_the_way() {
        let skeleton =
            Skeleton::parse(&table(&[(0, 0, 0, 0, 0), (1, 1, 0, 1, 2), (2, 2, 1, 2, 2)])).unwrap();
        let new = animation(
            [0, -100, 0],
            &[
                ([0; 3], [0; 3]),
                ([0; 3], [0, 0, 0]),
                ([40, 0, 0], [0, 0, 0]),
            ],
        );
        let old = animation(
            [0, -200, 30],
            &[
                ([0; 3], [0; 3]),
                ([0; 3], [0, 0, 1024]),
                ([80, 0, -9], [0, 2048, 0]),
            ],
        );
        // Nothing of the old animation, all of it, and the middle.
        assert_eq!(
            skeleton.pose_faded((&new, 0), (&old, 0), 0),
            skeleton.pose(&new, 0)
        );
        assert_eq!(
            skeleton.pose_faded((&new, 0), (&old, 0), ONE),
            skeleton.pose(&old, 0)
        );
        let pose = skeleton.pose_faded((&new, 0), (&old, 0), ONE / 2);
        let hip = pose[1].unwrap();
        assert_eq!(hip.position, [0, -150, 15]);
        // Half of a quarter turn about z.
        assert_eq!(hip.matrix, rotation([0, 0, 512]));
        // The child's offset lies between the two, cut towards zero (-9 / 2 is -4), and is
        // turned by the hip; of its half turn about y it makes a quarter.
        let leg = pose[2].unwrap();
        assert_eq!(leg.position, hip.place([60, 0, -4]));
        let quarter = Placement {
            matrix: rotation([0, 1024, 0]),
            position: [0; 3],
        };
        let alone = Placement {
            matrix: turn_towards(rotation([0; 3]), rotation([0, 2048, 0]), ONE / 2),
            position: [0; 3],
        };
        let turned = alone.place([100, 0, 0]);
        assert!(turned == quarter.place([100, 0, 0]) || turned == [0, 0, 100]);
        // A share too small to make a 4096th of a turn leaves the turn alone.
        assert_eq!(
            turn_towards(rotation([0; 3]), rotation([0, 0, 100]), 40),
            rotation([0; 3])
        );
        assert_eq!(
            turn_towards(rotation([0; 3]), rotation([0, 0, 100]), 2048),
            rotation([0, 0, 50])
        );

        // Under an actor's turn the faded pose is the plain one at both ends, and in
        // between every bone is the unturned one brought through the turn.
        let turn = rotation([0, 1024, 0]);
        let none = Scales::NONE;
        assert_eq!(
            skeleton.pose_faded_from(turn, (&new, 0), (&old, 0), 0, &none),
            skeleton.pose_from(turn, &new, 0, &none)
        );
        assert_eq!(
            skeleton.pose_faded_from(turn, (&new, 0), (&old, 0), ONE, &none),
            skeleton.pose_from(turn, &old, 0, &none)
        );
        let actor = Placement {
            matrix: turn,
            position: [0; 3],
        };
        let turned = skeleton.pose_faded_from(turn, (&new, 0), (&old, 0), ONE / 2, &none);
        assert_eq!(turned[1].unwrap().position, actor.turn(hip.position));

        // The scales of a faded pose: the bone's record, then the model's, towards zero.
        let records = [
            BoneScale::NONE,
            BoneScale {
                scale: [2048, 4096, 6144],
                used: 0,
            },
        ];
        let scales = Scales {
            model: Some([2048; 3]),
            bones: &records,
        };
        let leg_bone = &skeleton.bones[2];
        assert_eq!(scales.offset_mixed(leg_bone, [-9, 60, -5]), [-2, 30, -3]);
        assert_eq!(scales.offset(leg_bone, [-9, 60, -5]), [-2, 30, -4]);
        let scaled = skeleton.pose_faded_scaled((&new, 0), (&old, 0), ONE / 2, &scales);
        assert_eq!(scaled[1].unwrap().position, [0, -75, 7]);
    }

    #[test]
    fn a_shape_gives_the_offsets_and_a_turn_about_x() {
        // No turn is the plain rotation; a quarter turn about x of no rotation.
        let angles = [100, -200, 300];
        assert_eq!(rotation_turned(angles, 0), rotation(angles));
        let quarter = [[ONE, 0, 0], [0, 0, -ONE], [0, ONE, 0]];
        assert_eq!(rotation_turned([0; 3], 1024), quarter);

        let skeleton =
            Skeleton::parse(&table(&[(0, 0, 0, 0, 2), (1, 1, 0, 1, 2), (2, 2, 1, 2, 2)])).unwrap();
        let animation = animation(
            [0, -100, 0],
            &[
                ([0; 3], [0; 3]),
                ([0, -100, 0], [0; 3]),
                ([10, -50, 0], [0; 3]),
            ],
        );
        let shape = Shape {
            rest: vec![[0; 3], [7, -20, 9], [30, -60, 0]],
            turns: vec![0, 0, 1024],
        };
        let (none, plain) = (Scales::NONE, Placement::IDENTITY.matrix);
        let pose = skeleton.pose_shaped_from(plain, &animation, 0, &none, Some(&shape));
        // The second bone stands where the frame puts it, lifted by the shape's height.
        assert_eq!(pose[1].unwrap().position, [0, -120, 0]);
        // The bones below take the shape's offset and its turn.
        assert_eq!(pose[2].unwrap().position, [30, -180, 0]);
        assert_eq!(pose[2].unwrap().matrix, quarter);
        // A fade between two frames that are the same is that pose.
        let frame = (&animation, 0);
        let faded =
            skeleton.pose_faded_shaped_from(plain, frame, frame, ONE / 2, &none, Some(&shape));
        assert_eq!(faded, pose);
        // Without a shape it is the pose of the animation alone.
        assert_eq!(
            skeleton.pose_shaped_from(plain, &animation, 0, &none, None),
            skeleton.pose(&animation, 0)
        );
    }
}
