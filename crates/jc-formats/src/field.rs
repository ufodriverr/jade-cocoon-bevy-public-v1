
use std::fmt;

use crate::command::quotient;
use crate::mover::ArcTangent;

/// Bytes of the header that follows the view count.
pub const HEADER_SIZE: usize = 0x104;
/// Region tables in a room.
pub const TABLE_COUNT: usize = 5;
/// Bytes of the head of a region table: its size, the region count and two unused words.
const TABLE_HEAD: usize = 16;
/// Bytes of the head of a region on disc; only the triangle count in its first word is used.
const REGION_HEAD: usize = 40;
/// Bytes of a triangle on disc.
const TRIANGLE_SIZE: usize = 40;
/// Bytes of a record of the position table.
const POSITION_SIZE: usize = 16;
/// Bytes of a view record.
const VIEW_SIZE: usize = 12;

/// A position in room coordinates: x, y, z. The y axis points down.
pub type Position = [i32; 3];

/// Why bytes are not room data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldError {
    /// The data ends inside a block. Holds the offset of the block.
    Truncated(usize),
    /// A size field is too small for what it has to hold. Holds the offset of the field.
    BadSize(usize),
}

impl fmt::Display for FieldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated(at) => write!(f, "room data ends inside the block at {at:#x}"),
            Self::BadSize(at) => write!(f, "room data has an impossible size at {at:#x}"),
        }
    }
}

impl std::error::Error for FieldError {}

/// A resource block. What the kinds hold is not read yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resource<'a> {
    /// Picks the routine that takes the block: 0 to 4.
    pub kind: u32,
    /// The second word of the block's head.
    pub value: u32,
    pub data: &'a [u8],
}

/// A triangle of a region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Triangle {
    /// Surface normal of length 4096. A level floor has y = -4096.
    pub normal: [i16; 3],
    /// The corners, in the winding the containment test relies on.
    pub corners: [[i16; 3]; 3],
    /// For the edge from corner `i` to the next corner: true when another triangle of the
    /// same region shares it. The other edges are the outline of the region.
    pub inner_edges: [bool; 3],
}

impl Triangle {
    /// The game's test for "the position is on this triangle", in the horizontal plane:
    /// first the triangle's bounding box, then the side of each of the three edges. A
    /// position exactly on an edge is inside.
    pub fn contains(&self, position: Position) -> bool {
        let (x, z) = (position[0], position[2]);
        let xs = self.corners.map(|corner| i32::from(corner[0]));
        let zs = self.corners.map(|corner| i32::from(corner[2]));
        let outside_box = |value: i32, of: [i32; 3]| {
            value < of.into_iter().min().expect("three values")
                || value > of.into_iter().max().expect("three values")
        };
        if outside_box(x, xs) || outside_box(z, zs) {
            return false;
        }
        // The console multiplies in 32 bits and keeps the low word.
        (0..3).all(|from| {
            let to = (from + 1) % 3;
            let along = (xs[to] - xs[from]).wrapping_mul(z.wrapping_sub(zs[from]));
            let across = (zs[to] - zs[from]).wrapping_mul(x.wrapping_sub(xs[from]));
            along.wrapping_sub(across) <= 0
        })
    }

    /// The height (y) of the triangle's plane at the position's x and z, as the game solves
    /// it from the normal and corner 0, with its whole-number division. `None` for a
    /// vertical triangle, where the game would divide by zero.
    pub fn height_at(&self, position: Position) -> Option<i32> {
        let [nx, ny, nz] = self.normal.map(i32::from);
        let [x0, y0, z0] = self.corners[0].map(i32::from);
        let x_part = nx.wrapping_mul(position[0].wrapping_sub(x0));
        let z_part = nz.wrapping_mul(position[2].wrapping_sub(z0));
        (ny * y0)
            .wrapping_sub(x_part)
            .wrapping_sub(z_part)
            .checked_div(ny)
    }

    /// The two ends of the edge that starts at a corner.
    fn edge(&self, corner: usize) -> ([i16; 3], [i16; 3]) {
        (self.corners[corner], self.corners[(corner + 1) % 3])
    }
}

/// An edge of a region: a triangle and the corner the edge starts at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edge {
    pub triangle: usize,
    pub corner: usize,
}

/// The direction of an edge in 4096ths: x and z.
pub type Direction = [i32; 2];

/// Bits behind the point of a direction.
const POINT: u32 = 12;
/// A mover is put off an edge by the edge's direction shifted right by this: four units.
const PUSH_SHIFT: u32 = 10;

/// Where the game does not go on, in a mover of file 53.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    /// Two edges were crossed and they do not meet in a corner: the game stops with its
    /// own assertion.
    Assertion,
    /// The position is off the ground and no outline edge passes the rules of the crossed
    /// edge. The game has no guard for it and reads a triangle that is not there; no step
    /// on the walk meshes of the disc was seen to come here.
    NoEdge,
}

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Assertion => write!(f, "two crossed edges do not meet (the game's assertion)"),
            Self::NoEdge => write!(f, "off the ground and no edge was crossed"),
        }
    }
}

/// x and z of a corner.
fn flat(corner: [i16; 3]) -> [i32; 2] {
    [i32::from(corner[0]), i32::from(corner[2])]
}

/// The edge value of a place against the line from `a` to `b`, in x and z: above zero on
/// the outside of an edge. The console multiplies in 32 bits and keeps the low word.
fn side(a: [i32; 2], b: [i32; 2], place: [i32; 2]) -> i32 {
    let along = (b[0].wrapping_sub(a[0])).wrapping_mul(place[1].wrapping_sub(a[1]));
    let across = (b[1].wrapping_sub(a[1])).wrapping_mul(place[0].wrapping_sub(a[0]));
    along.wrapping_sub(across)
}

/// A value in 4096ths as a whole number, divided towards zero.
fn whole(value: i32) -> i32 {
    value / (1 << POINT)
}

/// The square root, rounded down (`0x8003B91C`).
fn root(value: i32) -> i32 {
    if value <= 0 {
        return 0;
    }
    let value = i64::from(value);
    let mut guess = value;
    loop {
        let last = guess;
        guess = (value / guess + last) >> 1;
        if guess >= last {
            return last as i32;
        }
    }
}

/// The corner (`0x800B81F8`): when two of the noted ends of the crossed edges are the same
/// point, that point becomes the position.
fn corner(position: &mut Position, ends: &[[i16; 3]]) -> bool {
    let twice = (ends.iter().enumerate()).find(|(index, end)| {
        (ends.iter().enumerate()).any(|(other, same)| other != *index && same == *end)
    });
    match twice {
        Some((_, end)) => {
            *position = end.map(i32::from);
            true
        }
        None => false,
    }
}

/// The answer of a mover for a step that slid along an edge: the angle of the edge's
/// direction is taken from the mover's turn, and a difference of 1025 to 2047 or of 3073 to
/// 4095 is the first answer, anything else the second.
fn slide(direction: Direction, turn: i16, arc: &ArcTangent, answers: [i32; 2]) -> i32 {
    let edge = i32::from(arc.angle(direction[0], direction[1]));
    match (i32::from(turn) - edge) & 0xFFF {
        1025..=2047 | 3073.. => answers[0],
        _ => answers[1],
    }
}

/// Where the player stands: the region of the walk mesh, which is the camera view, and the
/// triangle in it (+132 and +128 of the player's task).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Foothold {
    pub region: usize,
    pub triangle: usize,
}

/// What a mover found for a position that is off the ground at most twice.
enum Looked {
    /// A triangle holds the position, after so many crossed edges; the direction is that
    /// of the last one.
    Ground(Foothold, usize, Option<Direction>),
    /// The position was put on the corner where the two crossed edges meet.
    Corner,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Ground {
    pub regions: Vec<Region>,
    pub barriers: Vec<Region>,
    /// One byte per barrier, all 1 when a room starts; a barrier whose byte is zero is
    /// switched off. A barrier without a byte counts as switched on.
    pub flags: Vec<u8>,
}

impl Ground {
    pub(crate) fn new(regions: &[Region], barriers: &[Region]) -> Self {
        Self {
            regions: regions.to_vec(),
            barriers: barriers.to_vec(),
            flags: vec![1; barriers.len()],
        }
    }

    /// The barriers (`0x800B8498`): the first switched-on barrier that the step from
    /// `before` enters through an outline edge puts the position on that edge and four
    /// units to the outside. False when a barrier stopped the step.
    pub fn barred(&self, position: &mut Position, before: Position) -> bool {
        for (number, barrier) in self.barriers.iter().enumerate() {
            if self.flags.get(number) == Some(&0) {
                continue;
            }
            let Some(edge) = barrier.crossed_edge(before, *position, None) else {
                continue;
            };
            if let Some(direction) = barrier.onto_edge(edge, position) {
                position[0] = position[0].wrapping_sub(direction[1] >> PUSH_SHIFT);
                position[2] = position[2].wrapping_add(direction[0] >> PUSH_SHIFT);
            }
            return false;
        }
        true
    }

    /// The triangle under a position (`0x800B8298`, step 2 of the player's mover on its
    /// own): looked for from the foothold's region on and round all regions. True when one
    /// holds the position, which makes it the foothold; otherwise the foothold stays.
    pub fn stand(&self, foothold: &mut Foothold, position: Position) -> bool {
        let found = (0..self.regions.len()).find_map(|step| {
            let region = (foothold.region + step) % self.regions.len();
            let triangle = self.regions[region].triangle_at(position)?;
            Some(Foothold { region, triangle })
        });
        if let Some(found) = found {
            *foothold = found;
        }
        found.is_some()
    }

    pub fn view_from(&self, current: usize, position: Position) -> Option<usize> {
        let mut foothold = Foothold {
            region: current % self.regions.len().max(1),
            triangle: 0,
        };
        (self.stand(&mut foothold, position)).then_some(foothold.region)
    }

    /// Steps 2 to 5 of a mover over `regions`: the triangle under the position, looked for
    /// from region `first` on and round all of them, with the position put back onto the
    /// ground of region `first` at most twice.
    fn look(
        regions: &[Region],
        first: usize,
        position: &mut Position,
        before: Position,
    ) -> Result<Looked, Fault> {
        let mut found = None;
        let mut direction = None;
        let mut ends = Vec::new();
        for tries in 0.. {
            let hit = (0..regions.len()).find_map(|step| {
                let region = (first + step) % regions.len();
                let triangle = regions[region].triangle_at(*position)?;
                Some(Foothold { region, triangle })
            });
            if let Some(foothold) = hit {
                return Ok(Looked::Ground(foothold, tries, direction));
            }
            if tries == 2 {
                break;
            }
            let region = regions.get(first).ok_or(Fault::NoEdge)?;
            direction = Some(region.back_onto(position, before, &mut found)?);
            let (a, b) = (found
                .and_then(|edge| Some(region.triangles.get(edge.triangle)?.edge(edge.corner))))
            .ok_or(Fault::NoEdge)?;
            ends.extend([a, b]);
        }
        match corner(position, &ends) {
            true => Ok(Looked::Corner),
            false => Err(Fault::Assertion),
        }
    }

    /// The height of the triangle of a foothold, as the movers set it at their end. A
    /// vertical triangle, where the game would divide by zero, leaves the height alone.
    fn set_height(regions: &[Region], foothold: Foothold, position: &mut Position) {
        let triangle = &regions[foothold.region].triangles[foothold.triangle];
        if let Some(height) = triangle.height_at(*position) {
            position[1] = height;
        }
    }

    /// The player's mover (`0x800B6FA4`), for the ground of [`RoomData::ground`]: the
    /// position after the step of this game frame is stopped by the barriers, kept on the
    /// walk mesh and given its height; the foothold becomes the region and triangle found.
    /// The answer is 0 for a free step or one that ended on a corner (which sets no height
    /// and leaves the foothold), and for a step that slid along an edge 1 or -1 by the
    /// angle between the edge and the player's turn.
    pub fn walk_player(
        &self,
        foothold: &mut Foothold,
        position: &mut Position,
        before: Position,
        turn: i16,
        arc: &ArcTangent,
    ) -> Result<i32, Fault> {
        // A stopped step goes on from where the barrier left it.
        self.barred(position, before);
        match Self::look(&self.regions, foothold.region, position, before)? {
            Looked::Corner => Ok(0),
            Looked::Ground(found, _, direction) => {
                *foothold = found;
                Self::set_height(&self.regions, found, position);
                Ok(direction.map_or(0, |direction| slide(direction, turn, arc, [1, -1])))
            }
        }
    }

    /// The second mover (`0x800B733C`), for the ground of [`RoomData::second_ground`] and
    /// an actor that walks over the ground (flag 0x40000): only the first region counts.
    /// The answer is 0 for a free step, 3 when a barrier stopped the step, when two edges
    /// were crossed or when the step ended on a corner, and for a step that slid along one
    /// edge 1 or 2 by the angle between the edge and the actor's turn. No height is set
    /// behind a barrier or on a corner.
    pub fn walk_actor(
        &self,
        position: &mut Position,
        before: Position,
        turn: i16,
        arc: &ArcTangent,
    ) -> Result<i32, Fault> {
        if !self.barred(position, before) {
            return Ok(3);
        }
        let regions = &self.regions[..self.regions.len().min(1)];
        match Self::look(regions, 0, position, before)? {
            Looked::Corner => Ok(3),
            Looked::Ground(found, tries, direction) => {
                Self::set_height(regions, found, position);
                Ok(match direction {
                    _ if tries == 2 => 3,
                    None => 0,
                    Some(direction) => slide(direction, turn, arc, [1, 2]),
                })
            }
        }
    }
}

/// A region: a list of triangles that are tested as one.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Region {
    pub triangles: Vec<Triangle>,
}

impl Region {
    /// Index of the first triangle that contains the position.
    pub fn triangle_at(&self, position: Position) -> Option<usize> {
        self.triangles
            .iter()
            .position(|triangle| triangle.contains(position))
    }

    /// The crossed edge (`0x800B7BB0`): the first outline edge that `one` lies strictly
    /// outside of and `other` does not, that the way between the two places meets, and that
    /// is not `found`. When no edge passes, the answer is `found` again.
    pub fn crossed_edge(
        &self,
        one: Position,
        other: Position,
        found: Option<Edge>,
    ) -> Option<Edge> {
        let (out, inside) = ([one[0], one[2]], [other[0], other[2]]);
        for (triangle, shape) in self.triangles.iter().enumerate() {
            for corner in 0..3 {
                if shape.inner_edges[corner] {
                    continue;
                }
                let (a, b) = shape.edge(corner);
                let (a, b) = (flat(a), flat(b));
                // The box of the edge and the box of the two places; touching is enough.
                let apart = |axis: usize| {
                    a[axis].max(b[axis]) < out[axis].min(inside[axis])
                        || out[axis].max(inside[axis]) < a[axis].min(b[axis])
                };
                if apart(0) || apart(1) {
                    continue;
                }
                if side(a, b, out) <= 0 || side(a, b, inside) > 0 {
                    continue;
                }
                // The two ends of the edge are not on the same side of the way.
                if side(inside, out, a).signum() * side(inside, out, b).signum() > 0 {
                    continue;
                }
                let edge = Edge { triangle, corner };
                if found != Some(edge) {
                    return Some(edge);
                }
            }
        }
        found
    }

    /// Onto the edge (`0x800B8010`): puts x and z of the position on the line of an edge,
    /// not held between its two ends, and answers the direction of the edge. `None` when
    /// the region has no such triangle.
    pub fn onto_edge(&self, edge: Edge, position: &mut Position) -> Option<Direction> {
        let (a, b) = self.triangles.get(edge.triangle)?.edge(edge.corner % 3);
        let (a, b) = (flat(a), flat(b));
        let (x, z) = (b[0] - a[0], b[1] - a[1]);
        let length = root(x.wrapping_mul(x).wrapping_add(z.wrapping_mul(z)));
        let direction = [quotient(x << POINT, length), quotient(z << POINT, length)];
        let from = [
            position[0].wrapping_sub(a[0]),
            position[2].wrapping_sub(a[1]),
        ];
        let along = whole(
            (direction[0].wrapping_mul(from[0])).wrapping_add(direction[1].wrapping_mul(from[1])),
        );
        position[0] = a[0].wrapping_add(whole(along.wrapping_mul(direction[0])));
        position[2] = a[1].wrapping_add(whole(along.wrapping_mul(direction[1])));
        Some(direction)
    }

    /// Back onto the ground (`0x800B83F0`): the outline edge a step from `before` left the
    /// region through, the position put on it and [`PUSH`] units to the inside. `found` is
    /// the edge of the call before and becomes the edge of this one.
    fn back_onto(
        &self,
        position: &mut Position,
        before: Position,
        found: &mut Option<Edge>,
    ) -> Result<Direction, Fault> {
        *found = self.crossed_edge(*position, before, *found);
        let direction =
            (found.and_then(|edge| self.onto_edge(edge, position))).ok_or(Fault::NoEdge)?;
        position[0] = position[0].wrapping_add(direction[1] >> PUSH_SHIFT);
        position[2] = position[2].wrapping_sub(direction[0] >> PUSH_SHIFT);
        Ok(direction)
    }

    /// Marks the edges that two triangles of the region share, as the game does after
    /// loading: edges are compared by x and z only, in either direction.
    pub(crate) fn mark_inner_edges(&mut self) {
        let flat = |corner: [i16; 3]| (corner[0], corner[2]);
        let edges: Vec<[_; 3]> = self
            .triangles
            .iter()
            .map(|triangle| {
                [0, 1, 2].map(|i| {
                    (
                        flat(triangle.corners[i]),
                        flat(triangle.corners[(i + 1) % 3]),
                    )
                })
            })
            .collect();
        for (index, triangle) in self.triangles.iter_mut().enumerate() {
            for (edge, inner) in edges[index].iter().zip(&mut triangle.inner_edges) {
                *inner = edges.iter().enumerate().any(|(other, theirs)| {
                    other != index
                        && theirs
                            .iter()
                            .any(|their| their == edge || (their.1, their.0) == *edge)
                });
            }
        }
    }
}

/// A view record: where the view lies in the room's background file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewRecord {
    /// Offset of the view in the background file.
    pub offset: u32,
    /// Bytes of the view without its optional tail.
    pub size: u32,
    /// Bytes of the view with the tail; the next view starts behind them.
    pub full_size: u32,
}

/// The room data of a scene package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomData<'a> {
    pub resources: Vec<Resource<'a>>,
    /// The word after the view count, `0x3D000` in every room.
    pub view_word: u32,
    /// The header: a copy of the header of view 0 in the background file.
    pub header: &'a [u8],
    /// The five region tables in file order. See the accessors for what each is for.
    pub tables: [Vec<Region>; TABLE_COUNT],
    /// The model block with its size word: a mesh block, read by `mesh::MeshBlock::parse`.
    pub models: &'a [u8],
    /// The texture block without its size word. Not read yet.
    pub textures: &'a [u8],
    /// The word in front of the position table's count.
    pub position_word: u32,
    /// The position table: records of four values. What they mean is not read yet.
    pub positions: Vec<[i32; 4]>,
    /// One record per view of the room.
    pub views: Vec<ViewRecord>,
    /// Bytes of the room data that the layout covers.
    pub len: usize,
}

/// Reads little-endian values from a slice, front to back.
struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], FieldError> {
        let bytes = self.data.get(self.at..).and_then(|rest| rest.get(..len));
        let bytes = bytes.ok_or(FieldError::Truncated(self.at))?;
        self.at += len;
        Ok(bytes)
    }

    fn u32(&mut self) -> Result<u32, FieldError> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes(bytes.try_into().expect("four bytes")))
    }

    fn i32(&mut self) -> Result<i32, FieldError> {
        Ok(self.u32()? as i32)
    }

    /// A count or a size, as a `usize`.
    fn size(&mut self) -> Result<usize, FieldError> {
        Ok(self.u32()? as usize)
    }

    /// Three 16-bit values and the fourth one that pads them to eight bytes.
    fn vector(&mut self) -> Result<[i16; 3], FieldError> {
        let bytes = self.take(8)?;
        Ok([0, 2, 4].map(|at| i16::from_le_bytes([bytes[at], bytes[at + 1]])))
    }

    /// One region table; the reader ends up where the table says the next one starts.
    fn region_table(&mut self) -> Result<Vec<Region>, FieldError> {
        let start = self.at;
        let size = self.size()?;
        let count = self.size()?;
        self.take(TABLE_HEAD - 8)?;
        let mut regions = Vec::new();
        for _ in 0..count {
            let head = self.at;
            let triangles = self.size()?;
            self.take(REGION_HEAD - 4)?;
            // Checked before reading, so that a wild count cannot ask for the memory.
            let fits = triangles
                .checked_mul(TRIANGLE_SIZE)
                .is_some_and(|bytes| bytes <= self.data.len() - self.at);
            if !fits {
                return Err(FieldError::Truncated(head));
            }
            let mut region = Region::default();
            for _ in 0..triangles {
                // Two words the game skips.
                self.take(8)?;
                region.triangles.push(Triangle {
                    normal: self.vector()?,
                    corners: [self.vector()?, self.vector()?, self.vector()?],
                    inner_edges: [false; 3],
                });
            }
            region.mark_inner_edges();
            regions.push(region);
        }
        let end = start.checked_add(size).filter(|end| *end >= self.at);
        self.at = end.ok_or(FieldError::BadSize(start))?;
        Ok(regions)
    }
}

impl<'a> RoomData<'a> {
    /// Reads room data: what `scene::ScenePackage::room_data` returns.
    pub fn parse(data: &'a [u8]) -> Result<Self, FieldError> {
        let mut reader = Reader { data, at: 0 };
        let mut resources = Vec::new();
        for _ in 0..reader.size()? {
            let (kind, value) = (reader.u32()?, reader.u32()?);
            let size = reader.size()?;
            let data = reader.take(size)?;
            resources.push(Resource { kind, value, data });
        }
        let view_count = reader.size()?;
        let view_word = reader.u32()?;
        let header = reader.take(HEADER_SIZE)?;

        let mut tables: [Vec<Region>; TABLE_COUNT] = Default::default();
        for table in &mut tables {
            *table = reader.region_table()?;
        }

        // The model block's size counts its own size word, the texture block's does not.
        let start = reader.at;
        let size = reader.size()?;
        reader.at = start;
        if size < 4 {
            return Err(FieldError::BadSize(start));
        }
        let models = reader.take(size)?;
        let size = reader.size()?;
        let textures = reader.take(size)?;

        let position_word = reader.u32()?;
        let mut positions = Vec::new();
        for _ in 0..reader.size()? {
            let record = reader.take(POSITION_SIZE)?;
            positions.push(
                [0, 4, 8, 12].map(|at| {
                    i32::from_le_bytes(record[at..at + 4].try_into().expect("four bytes"))
                }),
            );
        }

        // Checked before reading, so that a wild count cannot ask for the memory.
        let fits = view_count
            .checked_mul(VIEW_SIZE)
            .is_some_and(|bytes| bytes <= data.len() - reader.at);
        if !fits {
            return Err(FieldError::Truncated(reader.at));
        }
        let mut views = Vec::with_capacity(view_count);
        for _ in 0..view_count {
            views.push(ViewRecord {
                offset: reader.u32()?,
                size: reader.u32()?,
                full_size: reader.u32()?,
            });
        }

        Ok(Self {
            resources,
            view_word,
            header,
            tables,
            models,
            textures,
            position_word,
            positions,
            views,
            len: reader.at,
        })
    }

    fn header_reader(&self) -> Reader<'a> {
        Reader {
            data: self.header,
            at: 0,
        }
    }

    /// Where the player stands when the room is entered without an arrival position.
    pub fn default_position(&self) -> Position {
        let mut reader = self.header_reader();
        [0; 3].map(|_| reader.i32().expect("the header is long enough"))
    }

    /// The player's three angles that go with [`Self::default_position`].
    pub fn default_angles(&self) -> [i16; 3] {
        let mut reader = self.header_reader();
        reader.at = 12;
        reader.vector().expect("the header is long enough")
    }

    /// Table 1, one region per view: the player's walk mesh. The region the player stands
    /// in is the number of the camera view.
    pub fn walk_mesh(&self) -> &[Region] {
        &self.tables[0]
    }

    /// Table 2: barriers for the player, each switched by a flag.
    pub fn barriers(&self) -> &[Region] {
        &self.tables[1]
    }

    /// Table 3, a single region: the walk mesh of the game's second mover.
    pub fn second_walk_mesh(&self) -> &[Region] {
        &self.tables[2]
    }

    /// Table 4: barriers for the second mover, each switched by a flag.
    pub fn second_barriers(&self) -> &[Region] {
        &self.tables[3]
    }

    /// Table 5: the trigger regions that exits and map triggers name by index.
    pub fn triggers(&self) -> &[Region] {
        &self.tables[4]
    }

    /// The trigger test `0x800A6494`: the position lies in trigger region `region`. A region
    /// the room does not have holds nothing.
    pub fn in_trigger(&self, region: usize, position: Position) -> bool {
        (self.triggers().get(region)).is_some_and(|region| region.triangle_at(position).is_some())
    }

    /// The ground of the player: tables 1 and 2, every barrier switched on.
    pub fn ground(&self) -> Ground {
        Ground::new(self.walk_mesh(), self.barriers())
    }

    /// The ground of an actor that walks: tables 3 and 4, every barrier switched on.
    pub fn second_ground(&self) -> Ground {
        Ground::new(self.second_walk_mesh(), self.second_barriers())
    }

    /// The view the camera takes for a player position: the first region of the walk mesh
    /// that contains it. This is how the room loader picks a view when none is asked for.
    pub fn view_at(&self, position: Position) -> Option<usize> {
        self.walk_mesh()
            .iter()
            .position(|region| region.triangle_at(position).is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(out: &mut Vec<u8>, values: &[u32]) {
        for value in values {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }

    fn vector(out: &mut Vec<u8>, values: [i16; 3]) {
        for value in values {
            out.extend_from_slice(&value.to_le_bytes());
        }
        out.extend_from_slice(&[0, 0]);
    }

    /// A level floor triangle at height `y`.
    fn floor(corners: [[i16; 2]; 3], y: i16) -> Triangle {
        Triangle {
            normal: [0, -4096, 0],
            corners: corners.map(|[x, z]| [x, y, z]),
            inner_edges: [false; 3],
        }
    }

    /// A region table as it is on disc.
    fn table(regions: &[&[Triangle]]) -> Vec<u8> {
        let mut body = Vec::new();
        for region in regions {
            words(&mut body, &[region.len() as u32]);
            body.extend_from_slice(&[0; REGION_HEAD - 4]);
            for triangle in *region {
                words(&mut body, &[0x1234, 0x5678]);
                vector(&mut body, triangle.normal);
                for corner in triangle.corners {
                    vector(&mut body, corner);
                }
            }
        }
        let mut out = Vec::new();
        let size = (TABLE_HEAD + body.len()) as u32;
        words(&mut out, &[size, regions.len() as u32, 0, 0]);
        out.extend(body);
        out
    }

    /// A square of two triangles from (0, 0) to (100, 100), in the game's winding.
    fn square(y: i16) -> [Triangle; 2] {
        [
            floor([[0, 0], [100, 100], [100, 0]], y),
            floor([[0, 0], [0, 100], [100, 100]], y),
        ]
    }

    /// Room data with one resource, two views and a region in tables 1, 3 and 5.
    fn sample() -> Vec<u8> {
        let mut data = Vec::new();
        words(&mut data, &[1, 3, 7, 5]);
        data.extend_from_slice(b"abcde");
        words(&mut data, &[2, 0x3D000]);
        let mut header = Vec::new();
        words(&mut header, &[50, (-20i32) as u32, 60]);
        vector(&mut header, [0, 0x400, 0]);
        header.resize(HEADER_SIZE, 0);
        data.extend(header);
        let near = square(-20);
        let far = [floor([[100, 0], [100, 100], [200, 0]], -20)];
        data.extend(table(&[&near, &far]));
        data.extend(table(&[]));
        data.extend(table(&[&near]));
        data.extend(table(&[]));
        data.extend(table(&[&far]));
        // Model block of 16 bytes, texture block of 8 bytes behind its size.
        words(&mut data, &[16, 0, 0, 0]);
        words(&mut data, &[8, 0, 0]);
        // One position.
        words(&mut data, &[0, 1, 10, 20, 30, 0x800]);
        // Two views back to back.
        words(&mut data, &[0, 0x1000, 0x1800, 0x1800, 0x2000, 0x2000]);
        data
    }

    #[test]
    fn reads_every_block() {
        let data = sample();
        let room = RoomData::parse(&data).expect("room data");
        assert_eq!(room.len, data.len());
        let resource = Resource {
            kind: 3,
            value: 7,
            data: b"abcde",
        };
        assert_eq!(room.resources, [resource]);
        assert_eq!(room.view_word, 0x3D000);
        assert_eq!(room.default_position(), [50, -20, 60]);
        assert_eq!(room.default_angles(), [0, 0x400, 0]);
        let regions: Vec<usize> = room.tables.iter().map(Vec::len).collect();
        assert_eq!(regions, [2, 0, 1, 0, 1]);
        assert_eq!(room.walk_mesh()[0].triangles.len(), 2);
        assert_eq!(room.second_walk_mesh()[0].triangles.len(), 2);
        assert!(room.barriers().is_empty() && room.second_barriers().is_empty());
        assert_eq!(room.triggers()[0].triangles[0].corners[1], [100, -20, 100]);
        assert_eq!(room.models.len(), 16);
        assert_eq!(room.textures.len(), 8);
        assert_eq!(room.positions, [[10, 20, 30, 0x800]]);
        let view = |offset, size, full_size| ViewRecord {
            offset,
            size,
            full_size,
        };
        assert_eq!(
            room.views,
            [view(0, 0x1000, 0x1800), view(0x1800, 0x2000, 0x2000)]
        );
    }

    #[test]
    fn marks_the_shared_edge_only() {
        let data = sample();
        let room = RoomData::parse(&data).expect("room data");
        // The diagonal of the square: the first edge of triangle 0, the last of triangle 1.
        let marks: Vec<[bool; 3]> = room.walk_mesh()[0]
            .triangles
            .iter()
            .map(|triangle| triangle.inner_edges)
            .collect();
        assert_eq!(marks, [[true, false, false], [false, false, true]]);
        // The neighbouring region shares an edge in space, but regions are marked apart.
        assert_eq!(room.walk_mesh()[1].triangles[0].inner_edges, [false; 3]);
    }

    #[test]
    fn finds_the_triangle_and_the_view() {
        let data = sample();
        let room = RoomData::parse(&data).expect("room data");
        let near = &room.walk_mesh()[0];
        assert_eq!(near.triangle_at([80, 0, 20]), Some(0));
        assert_eq!(near.triangle_at([20, 0, 80]), Some(1));
        // On the diagonal both contain it; the first one wins. Height plays no part.
        assert_eq!(near.triangle_at([50, 999, 50]), Some(0));
        assert_eq!(near.triangle_at([101, 0, 50]), None);
        assert_eq!(near.triangle_at([-1, 0, 0]), None);
        // The border is inside.
        assert_eq!(near.triangle_at([100, 0, 100]), Some(0));
        assert_eq!(room.view_at([150, 0, 10]), Some(1));
        assert_eq!(room.view_at([150, 0, 90]), None);
        // The other winding contains nothing but its own outline.
        let turned = floor([[0, 0], [100, 0], [100, 100]], 0);
        assert!(!turned.contains([80, 0, 20]));
    }

    #[test]
    fn solves_the_height_from_the_plane() {
        assert_eq!(
            floor([[0, 0], [100, 100], [100, 0]], -20).height_at([5, 0, 5]),
            Some(-20)
        );
        // A slope that drops one unit of y for every unit of x: normal (1, -1, 0) / sqrt 2.
        let slope = Triangle {
            normal: [2896, -2896, 0],
            corners: [[0, 0, 0], [100, 100, 0], [100, 100, 100]],
            inner_edges: [false; 3],
        };
        assert_eq!(slope.height_at([40, 0, 7]), Some(40));
        let wall = Triangle {
            normal: [4096, 0, 0],
            ..slope
        };
        assert_eq!(wall.height_at([40, 0, 7]), None);
    }

    /// A region of these triangles with its inner edges marked.
    fn region(triangles: &[Triangle]) -> Region {
        let mut region = Region {
            triangles: triangles.to_vec(),
        };
        region.mark_inner_edges();
        region
    }

    /// A table like the game's, which the tests need only to be near the true angle.
    fn arc() -> ArcTangent {
        let step = |ratio: usize| {
            ((ratio as f64 / 1024.0).atan() * 4096.0 / std::f64::consts::TAU).round() as i16
        };
        ArcTangent::of((0..1025).map(step).collect())
    }

    #[test]
    fn a_step_inside_or_into_another_region_is_free() {
        let data = sample();
        let ground = RoomData::parse(&data).expect("room data").ground();
        let mut foothold = Foothold::default();
        let mut position = [80, 0, 20];
        let answer = ground.walk_player(&mut foothold, &mut position, [70, -20, 20], 0, &arc());
        assert_eq!((answer, position), (Ok(0), [80, -20, 20]));
        // Over the outline of region 0 where region 1 lies behind it.
        let mut position = [110, -20, 20];
        let answer = ground.walk_player(&mut foothold, &mut position, [90, -20, 20], 0, &arc());
        assert_eq!((answer, position), (Ok(0), [110, -20, 20]));
        let there = Foothold {
            region: 1,
            triangle: 0,
        };
        assert_eq!(foothold, there);
        // And back: the search starts at the region the player is in and goes round.
        let mut position = [90, -20, 20];
        let answer = ground.walk_player(&mut foothold, &mut position, [110, -20, 20], 0, &arc());
        assert_eq!((answer, foothold.region), (Ok(0), 0));
    }

    #[test]
    fn the_view_is_the_first_region_from_the_wanted_one_on() {
        // Two regions that share the strip from x = 50 to x = 100.
        let shifted = square(-20).map(|mut triangle| {
            triangle.corners.iter_mut().for_each(|c| c[0] += 50);
            triangle
        });
        let ground = Ground::new(&[region(&square(-20)), region(&shifted)], &[]);
        assert_eq!(ground.view_from(0, [20, -20, 50]), Some(0));
        assert_eq!(ground.view_from(1, [20, -20, 50]), Some(0));
        assert_eq!(ground.view_from(0, [120, -20, 50]), Some(1));
        // Where both hold the place, the view that is wanted so far stays.
        assert_eq!(ground.view_from(0, [70, -20, 50]), Some(0));
        assert_eq!(ground.view_from(1, [70, -20, 50]), Some(1));
        // Off the ground the game keeps the view, and a room without regions has none.
        assert_eq!(ground.view_from(1, [300, -20, 50]), None);
        assert_eq!(Ground::new(&[], &[]).view_from(3, [0, 0, 0]), None);
    }

    #[test]
    fn a_step_off_the_ground_slides_along_the_edge() {
        let ground = Ground::new(&[region(&square(-20))], &[]);
        let edge = Edge {
            triangle: 0,
            corner: 1,
        };
        let crossed = ground.regions[0].crossed_edge([110, 0, 50], [90, 0, 50], None);
        assert_eq!(crossed, Some(edge));
        // The same step is not found twice, and a step that stays inside crosses nothing.
        let again = ground.regions[0].crossed_edge([110, 0, 50], [90, 0, 50], crossed);
        assert_eq!(again, crossed);
        assert_eq!(
            ground.regions[0].crossed_edge([95, 0, 50], [90, 0, 50], None),
            None
        );
        let mut position = [110, 7, 50];
        let direction = ground.regions[0].onto_edge(edge, &mut position);
        assert_eq!((direction, position), (Some([0, -4096]), [100, 7, 50]));

        // On the edge, four units to the inside, at the height of the floor. The edge runs
        // towards minus z: the answer goes by the turn against it.
        for (turn, player, actor) in [(0, -1, 2), (1024, -1, 2), (1100, 1, 1), (-900, 1, 1)] {
            let mut foothold = Foothold::default();
            let mut position = [110, 0, 50];
            let answer =
                ground.walk_player(&mut foothold, &mut position, [90, -20, 50], turn, &arc());
            assert_eq!((answer, position), (Ok(player), [96, -20, 50]), "{turn}");
            let mut position = [110, 0, 50];
            let answer = ground.walk_actor(&mut position, [90, -20, 50], turn, &arc());
            assert_eq!((answer, position), (Ok(actor), [96, -20, 50]), "{turn}");
        }
    }

    #[test]
    fn two_edges_end_on_the_ground_or_on_their_corner() {
        // Out over two edges of the square at once: put back twice, then on the ground.
        let ground = Ground::new(&[region(&square(-20))], &[]);
        let mut foothold = Foothold::default();
        let mut position = [110, 0, -10];
        let answer = ground.walk_player(&mut foothold, &mut position, [90, -20, 10], 0, &arc());
        assert_eq!((answer, position), (Ok(-1), [96, -20, 4]));
        let mut position = [110, 0, -10];
        let answer = ground.walk_actor(&mut position, [90, -20, 10], 0, &arc());
        assert_eq!((answer, position), (Ok(3), [96, -20, 4]));

        // Out over a sharp corner: the second push leaves the ground again, and the mover
        // is put on the point the two edges share. The foothold stays as it was.
        let wedge = [floor([[0, 0], [100, 100], [100, 0]], -20)];
        let ground = Ground::new(&[region(&wedge)], &[]);
        let mut foothold = Foothold {
            region: 0,
            triangle: 7,
        };
        let mut position = [-10, 0, -3];
        let answer = ground.walk_player(&mut foothold, &mut position, [10, -20, 5], 0, &arc());
        assert_eq!((answer, position), (Ok(0), [0, -20, 0]));
        assert_eq!(foothold.triangle, 7);
        let mut position = [-10, 0, -3];
        let answer = ground.walk_actor(&mut position, [10, -20, 5], 0, &arc());
        assert_eq!((answer, position), (Ok(3), [0, -20, 0]));

        // Off the ground without a step: nothing was crossed, where the game has no guard.
        let mut position = [300, 0, 300];
        let answer = ground.walk_actor(&mut position, [300, 0, 300], 0, &arc());
        assert_eq!(answer, Err(Fault::NoEdge));
    }

    #[test]
    fn a_barrier_stops_the_step_that_enters_it() {
        let barrier = [floor([[40, 40], [60, 60], [60, 40]], -20)];
        let mut ground = Ground::new(&[region(&square(-20))], &[region(&barrier)]);
        assert_eq!(ground.flags, [1]);
        // Put on the barrier's edge and to its outside; the player goes on from there,
        // an actor is done with the answer 3 and keeps its height.
        let mut foothold = Foothold::default();
        let mut position = [45, 0, 45];
        let answer = ground.walk_player(&mut foothold, &mut position, [30, -20, 45], 0, &arc());
        assert_eq!((answer, position), (Ok(0), [42, -20, 46]));
        let mut position = [45, 0, 45];
        let answer = ground.walk_actor(&mut position, [30, -20, 45], 0, &arc());
        assert_eq!((answer, position), (Ok(3), [42, 0, 46]));
        // A step that starts inside and leaves is free.
        let mut position = [30, 0, 45];
        assert!(ground.barred(&mut position, [50, 0, 45]));
        assert_eq!(position, [30, 0, 45]);
        // Switched off, the barrier is not there.
        ground.flags[0] = 0;
        let mut position = [45, 0, 45];
        let answer = ground.walk_actor(&mut position, [30, -20, 45], 0, &arc());
        assert_eq!((answer, position), (Ok(0), [45, -20, 45]));
    }

    #[test]
    fn the_root_is_rounded_down() {
        let roots: Vec<i32> = [0, 1, 2, 3, 4, 800, 20000, i32::MAX]
            .into_iter()
            .map(root)
            .collect();
        assert_eq!(roots, [0, 1, 1, 1, 2, 28, 141, 46340]);
    }

    #[test]
    fn rejects_short_and_impossible_data() {
        let data = sample();
        // Every cut of the data is an error, never a panic.
        for len in 0..data.len() {
            assert!(RoomData::parse(&data[..len]).is_err(), "cut at {len}");
        }
        // A region count without the regions.
        let mut wild = data.clone();
        let table = 4 + 12 + 5 + 8 + HEADER_SIZE;
        wild[table + 4..table + 8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(RoomData::parse(&wild).is_err());
        // A table smaller than its regions.
        let mut small = data.clone();
        small[table..table + 4].copy_from_slice(&20u32.to_le_bytes());
        assert_eq!(RoomData::parse(&small), Err(FieldError::BadSize(table)));
    }
}
