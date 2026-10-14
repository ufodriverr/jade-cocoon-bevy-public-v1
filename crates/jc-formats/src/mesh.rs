
use std::fmt;

use crate::rooms::{FIELD_ENGINE_BASE, Image};

/// Bytes of the head of a block: size, object count, stitch count, stitch offset.
const BLOCK_HEAD: usize = 16;
/// Chunk types, and so counts in the head of an object.
pub const CHUNK_TYPES: usize = 10;
/// Bytes of a seam vertex on disc.
const SEAM_VERTEX: usize = 20;
/// Bytes of a stitch quad on disc.
const STITCH: usize = 16;
/// Bit of a chunk's command byte that asks for blending with what is behind.
const BLEND_BIT: u8 = 2;

/// Why bytes are not a mesh block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeshError {
    /// The data ends inside the part that starts at this offset.
    Truncated(usize),
    /// The chunk at this offset has a type that is not known.
    UnknownChunk { at: usize, kind: u8 },
    /// The object at this offset holds other chunks than its counts say.
    Counts(usize),
    /// The head's size or stitch offset does not agree with where the objects end.
    BadSize,
    /// The stitch quad with this number names an object or a seam vertex that is not there.
    BadStitch(usize),
}

impl fmt::Display for MeshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated(at) => write!(f, "mesh block ends inside the part at {at:#x}"),
            Self::UnknownChunk { at, kind } => {
                write!(f, "mesh chunk of unknown type {kind} at {at:#x}")
            }
            Self::Counts(at) => write!(f, "mesh object at {at:#x} does not match its counts"),
            Self::BadSize => write!(f, "mesh block sizes do not agree"),
            Self::BadStitch(index) => {
                write!(f, "stitch quad {index} names a missing seam vertex")
            }
        }
    }
}

impl std::error::Error for MeshError {}

/// A texture page and a palette, as the console encodes them in two 16-bit words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TexturePage {
    pub page: u16,
    pub palette: u16,
}

impl TexturePage {
    /// Where the page starts in video memory: x and y in pixels of the 16-bit picture.
    pub fn origin(self) -> (u16, u16) {
        ((self.page & 0xF) * 64, ((self.page >> 4) & 1) * 256)
    }

    /// How a blended face is mixed with what is behind it: the console's mode 0 to 3.
    pub fn blend_mode(self) -> u8 {
        ((self.page >> 5) & 3) as u8
    }

    /// The colour depth of the page: 0 = 4-bit, 1 = 8-bit, 2 = 16-bit.
    pub fn depth(self) -> u8 {
        ((self.page >> 7) & 3) as u8
    }

    /// Where the palette row starts in video memory: x and y.
    pub fn palette_origin(self) -> (u16, u16) {
        ((self.palette & 0x3F) * 16, (self.palette >> 6) & 0x1FF)
    }

    pub fn lowered(self, rows: u8) -> Self {
        Self {
            page: self.page,
            palette: self.palette.wrapping_add(u16::from(rows) << 6),
        }
    }
}

/// A corner of a face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Corner {
    /// Position in the object's space. The y axis points down.
    pub position: [i16; 3],
    /// The corner's normal, in a shaded face. Usually, not always, of length 4096.
    pub normal: Option<[i16; 3]>,
    /// The corner's colour, in a shaded face.
    pub colour: Option<[u8; 3]>,
    /// Pixel position inside the texture page, in a textured face.
    pub uv: Option<[u8; 2]>,
}

/// A triangle or a quad.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Face {
    /// The chunk type, 0 to 7.
    pub kind: u8,
    /// The console's drawing command for the shape.
    pub command: u8,
    pub texture: Option<TexturePage>,
    /// The colour of an untextured flat face.
    pub colour: Option<[u8; 3]>,
    /// The normal of a flat face, of length 4096.
    pub normal: Option<[i16; 3]>,
    /// Three or four corners. A quad's are in the console's order: see [`Face::triangles`].
    pub corners: Vec<Corner>,
}

impl Face {
    /// Whether the face is mixed with what is behind it instead of covering it.
    pub fn is_blended(&self) -> bool {
        self.command & BLEND_BIT != 0
    }

    pub fn blends_by_itself(&self) -> bool {
        self.is_blended() && self.kind == 6
    }

    /// The face as triangles of corner indices. A quad is the triangle of its first three
    /// corners and the one of corners 1, 3 and 2.
    pub fn triangles(&self) -> &'static [[usize; 3]] {
        if self.corners.len() == 4 {
            &[[0, 1, 2], [1, 3, 2]]
        } else {
            &[[0, 1, 2]]
        }
    }
}

/// A vertex of an object's seam pool: a corner that stitch quads use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeamVertex {
    pub position: [i16; 3],
    pub normal: [i16; 3],
    pub colour: [u8; 3],
    /// Pixel position inside the texture page. Zero in a chunk without a texture.
    pub uv: [u8; 2],
    /// The texture of the chunk the vertex came in; `None` for a chunk of type 8.
    pub texture: Option<TexturePage>,
}

/// A mesh object: in a model the piece that moves with one bone, in a room a piece of its own.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MeshObject {
    /// Faces in file order.
    pub faces: Vec<Face>,
    /// The seam pool: the vertices of every seam chunk, in file order.
    pub seam: Vec<SeamVertex>,
}

/// A corner of a stitch quad: a seam vertex of an object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StitchCorner {
    pub object: u16,
    pub vertex: u16,
}

/// A mesh block.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MeshBlock {
    pub objects: Vec<MeshObject>,
    /// Quads between seam vertices, the corners in the order of a face's.
    pub stitches: Vec<[StitchCorner; 4]>,
    /// Bytes of the block, as its head says and as the layout covers.
    pub len: usize,
}

/// What a face chunk type holds.
struct FaceLayout {
    corners: usize,
    textured: bool,
    shaded: bool,
}

impl FaceLayout {
    /// The layout of chunk types 0 to 7: bit 0 shaded, bit 1 textured, bit 2 four corners.
    fn of(kind: u8) -> Option<Self> {
        (kind < 8).then_some(Self {
            corners: if kind & 4 != 0 { 4 } else { 3 },
            textured: kind & 2 != 0,
            shaded: kind & 1 != 0,
        })
    }
}

/// The sizes that follow from a layout: the check that it fills the documented bodies.
#[cfg(test)]
impl FaceLayout {
    /// Bytes of the body in front of the corners.
    fn head(&self) -> usize {
        let texture = if self.textured { 4 } else { 0 };
        let shading = if self.shaded {
            4 * self.corners
        } else if self.textured {
            8
        } else {
            12
        };
        texture + shading
    }

    fn corner_size(&self) -> usize {
        if self.shaded { 16 } else { 8 }
    }

    fn body(&self) -> usize {
        self.head() + self.corners * self.corner_size()
    }
}

/// Reads little-endian values from a slice, front to back.
struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], MeshError> {
        let bytes = self.data.get(self.at..).and_then(|rest| rest.get(..len));
        let bytes = bytes.ok_or(MeshError::Truncated(self.at))?;
        self.at += len;
        Ok(bytes)
    }

    fn u16(&mut self) -> Result<u16, MeshError> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    /// A count, a size or an offset, as a `usize`.
    fn size(&mut self) -> Result<usize, MeshError> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes(bytes.try_into().expect("four bytes")) as usize)
    }

    /// Three 16-bit values.
    fn vector(&mut self) -> Result<[i16; 3], MeshError> {
        let bytes = self.take(6)?;
        Ok([0, 2, 4].map(|at| i16::from_le_bytes([bytes[at], bytes[at + 1]])))
    }

    fn colour(&mut self) -> Result<[u8; 3], MeshError> {
        let bytes = self.take(3)?;
        Ok([bytes[0], bytes[1], bytes[2]])
    }

    fn uv(&mut self) -> Result<[u8; 2], MeshError> {
        let bytes = self.take(2)?;
        Ok([bytes[0], bytes[1]])
    }

    fn texture(&mut self) -> Result<TexturePage, MeshError> {
        Ok(TexturePage {
            page: self.u16()?,
            palette: self.u16()?,
        })
    }

    /// The body of a face chunk.
    fn face(&mut self, kind: u8, command: u8, layout: &FaceLayout) -> Result<Face, MeshError> {
        let texture = if layout.textured {
            Some(self.texture()?)
        } else {
            None
        };
        let (mut colour, mut normal) = (None, None);
        let mut colours = [None; 4];
        if layout.shaded {
            for corner in colours.iter_mut().take(layout.corners) {
                *corner = Some(self.colour()?);
                self.take(1)?;
            }
        } else {
            if !layout.textured {
                colour = Some(self.colour()?);
                self.take(1)?;
            }
            normal = Some(self.vector()?);
            self.take(2)?;
        }
        let mut corners = Vec::with_capacity(layout.corners);
        for colour in colours.into_iter().take(layout.corners) {
            let normal = if layout.shaded {
                let normal = self.vector()?;
                self.take(2)?;
                Some(normal)
            } else {
                None
            };
            let position = self.vector()?;
            let uv = self.uv()?;
            corners.push(Corner {
                position,
                normal,
                colour,
                uv: layout.textured.then_some(uv),
            });
        }
        Ok(Face {
            kind,
            command,
            texture,
            colour,
            normal,
            corners,
        })
    }

    /// The body of a seam chunk, added to the pool.
    fn seam(&mut self, textured: bool, pool: &mut Vec<SeamVertex>) -> Result<(), MeshError> {
        let start = self.at;
        let count = self.size()?;
        let texture = if textured {
            Some(self.texture()?)
        } else {
            None
        };
        // Checked before reading, so that a wild count cannot ask for the memory.
        let fits = count
            .checked_mul(SEAM_VERTEX)
            .is_some_and(|bytes| bytes <= self.data.len() - self.at);
        if !fits {
            return Err(MeshError::Truncated(start));
        }
        for _ in 0..count {
            let normal = self.vector()?;
            self.take(2)?;
            let position = self.vector()?;
            let uv = self.uv()?;
            let colour = self.colour()?;
            self.take(1)?;
            pool.push(SeamVertex {
                position,
                normal,
                colour,
                uv,
                texture,
            });
        }
        Ok(())
    }

    fn object(&mut self) -> Result<MeshObject, MeshError> {
        let start = self.at;
        let mut counts = [0usize; CHUNK_TYPES];
        for count in &mut counts {
            *count = self.size()?;
        }
        let mut object = MeshObject::default();
        let mut seen = [0usize; CHUNK_TYPES];
        // Every chunk is at least its four head bytes.
        let chunks = counts
            .iter()
            .try_fold(0usize, |sum, count| sum.checked_add(*count))
            .filter(|chunks| *chunks <= (self.data.len() - self.at) / 4)
            .ok_or(MeshError::Truncated(start))?;
        for _ in 0..chunks {
            let at = self.at;
            let head = self.take(4)?;
            let (kind, command) = (head[0], head[3]);
            match (FaceLayout::of(kind), kind) {
                (Some(layout), _) => {
                    object.faces.push(self.face(kind, command, &layout)?);
                }
                (None, 8) => self.seam(false, &mut object.seam)?,
                (None, 9) => self.seam(true, &mut object.seam)?,
                _ => return Err(MeshError::UnknownChunk { at, kind }),
            }
            seen[usize::from(kind)] += 1;
        }
        if seen != counts {
            return Err(MeshError::Counts(start));
        }
        Ok(object)
    }
}

impl MeshBlock {
    /// Reads a mesh block. `data` may be longer than the block; `len` says how much is used.
    pub fn parse(data: &[u8]) -> Result<Self, MeshError> {
        let mut reader = Reader { data, at: 0 };
        let size = reader.size()?;
        let object_count = reader.size()?;
        let stitch_count = reader.size()?;
        let stitch_offset = reader.size()?;
        if size > data.len() {
            return Err(MeshError::Truncated(0));
        }
        // An object is at least its ten counts.
        if size < BLOCK_HEAD || object_count > (size - BLOCK_HEAD) / (4 * CHUNK_TYPES) {
            return Err(MeshError::BadSize);
        }
        // Nothing of the block lies behind its size.
        reader.data = &data[..size];
        let mut objects = Vec::with_capacity(object_count);
        for _ in 0..object_count {
            objects.push(reader.object()?);
        }

        // A block without stitch quads has no stitch offset either.
        if stitch_count == 0 && stitch_offset == 0 {
            if reader.at != size {
                return Err(MeshError::BadSize);
            }
            return Ok(Self {
                objects,
                stitches: Vec::new(),
                len: size,
            });
        }
        let end = stitch_count
            .checked_mul(STITCH)
            .and_then(|bytes| bytes.checked_add(stitch_offset));
        if stitch_offset != reader.at || end != Some(size) {
            return Err(MeshError::BadSize);
        }
        let mut stitches = Vec::with_capacity(stitch_count);
        for index in 0..stitch_count {
            let mut quad = [StitchCorner {
                object: 0,
                vertex: 0,
            }; 4];
            for corner in &mut quad {
                *corner = StitchCorner {
                    object: reader.u16()?,
                    vertex: reader.u16()?,
                };
                let pool = objects.get(usize::from(corner.object));
                if pool.is_none_or(|object| usize::from(corner.vertex) >= object.seam.len()) {
                    return Err(MeshError::BadStitch(index));
                }
            }
            stitches.push(quad);
        }
        Ok(Self {
            objects,
            stitches,
            len: size,
        })
    }

    /// The seam vertex a stitch corner names.
    pub fn seam_vertex(&self, corner: StitchCorner) -> &SeamVertex {
        &self.objects[usize::from(corner.object)].seam[usize::from(corner.vertex)]
    }

    /// Faces of every object.
    pub fn face_count(&self) -> usize {
        self.objects.iter().map(|object| object.faces.len()).sum()
    }

    pub fn swap(&mut self, kept: &Kept, object: i16, source: i16) -> bool {
        let (Ok(object), Ok(source)) = (usize::try_from(object), usize::try_from(source)) else {
            return false;
        };
        let mesh = match object == source {
            true => kept.own(object).cloned(),
            false => self.objects.get(source).cloned(),
        };
        match (self.objects.get_mut(object), mesh) {
            (Some(shown), Some(mesh)) => {
                *shown = mesh;
                true
            }
            _ => false,
        }
    }
}

pub const PLAYER_KEPT: u32 = 0x800B_139C;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Kept(Vec<(usize, MeshObject)>);

impl Kept {
    /// The copies the field engine makes of the player's model: the objects of its list
    /// [`PLAYER_KEPT`] in file 27, taken from `block` as it is now. None when the list
    /// does not lie in the file.
    pub fn of_player(field_engine: &[u8], block: &MeshBlock) -> Option<Self> {
        let image = Image {
            data: field_engine,
            base: FIELD_ENGINE_BASE,
        };
        let mut objects = Vec::new();
        for entry in 0.. {
            let number = image.i32(PLAYER_KEPT + 4 * entry).ok()?;
            match usize::try_from(number) {
                Ok(number) => objects.push(number),
                Err(_) => break,
            }
        }
        Some(Self::of(block, &objects))
    }

    /// The numbers of the kept objects, in the order of the list.
    pub fn objects(&self) -> impl Iterator<Item = usize> + '_ {
        self.0.iter().map(|(number, _)| *number)
    }

    /// The copies of the listed objects of `block` as they are now; a number the block
    /// does not have is left out.
    pub fn of(block: &MeshBlock, objects: &[usize]) -> Self {
        let copy = |number: &usize| Some((*number, block.objects.get(*number)?.clone()));
        Self(objects.iter().filter_map(copy).collect())
    }

    /// The kept copy of an object, the first of its number as the lookup `0x80025194`
    /// finds it.
    pub fn own(&self, object: usize) -> Option<&MeshObject> {
        let (_, mesh) = self.0.iter().find(|(number, _)| *number == object)?;
        Some(mesh)
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

    fn halves(out: &mut Vec<u8>, values: &[i16]) {
        for value in values {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }

    /// A flat untextured triangle (type 0) with corners at x = 1, 2, 3.
    fn flat_triangle(out: &mut Vec<u8>) {
        out.extend_from_slice(&[0, 9, 9, 0x20]);
        out.extend_from_slice(&[10, 20, 30, 0]);
        halves(out, &[0, -4096, 0, 0]);
        for x in 1..=3 {
            halves(out, &[x, 0, -x, 0]);
        }
    }

    /// A textured shaded quad (type 7), blended, with corners at x = 10, 20, 30, 40.
    fn shaded_quad(out: &mut Vec<u8>) {
        out.extend_from_slice(&[7, 0, 0, 0x3E]);
        out.extend_from_slice(&0x0095u16.to_le_bytes());
        out.extend_from_slice(&0x6070u16.to_le_bytes());
        for corner in 0..4u8 {
            out.extend_from_slice(&[128 - corner, 128 - corner, 128 - corner, 0]);
        }
        for corner in 1..=4i16 {
            halves(out, &[4096, 0, 0, 0]);
            halves(out, &[corner * 10, -5, 7]);
            out.extend_from_slice(&[corner as u8, 200]);
        }
    }

    /// A seam chunk of type 9 with `count` vertices at y = 100, 101, ...
    fn seam(out: &mut Vec<u8>, count: u32) {
        out.extend_from_slice(&[9, 0, 0, 0x16]);
        words(out, &[count]);
        out.extend_from_slice(&0x008Cu16.to_le_bytes());
        out.extend_from_slice(&0x4030u16.to_le_bytes());
        for vertex in 0..count as i16 {
            halves(out, &[0, 0, 4096, 0]);
            halves(out, &[0, 100 + vertex, 0]);
            out.extend_from_slice(&[3, 4, 128, 128, 128, 0]);
        }
    }

    /// A block of two objects joined by one stitch quad.
    fn sample() -> Vec<u8> {
        let mut body = Vec::new();
        // Object 0: a seam chunk between two faces, to show that types are mixed.
        words(&mut body, &[1, 0, 0, 0, 0, 0, 0, 1, 0, 1]);
        shaded_quad(&mut body);
        seam(&mut body, 2);
        flat_triangle(&mut body);
        // Object 1: two seam chunks, one pool.
        words(&mut body, &[0, 0, 0, 0, 0, 0, 0, 0, 0, 2]);
        seam(&mut body, 1);
        seam(&mut body, 2);
        let stitch_offset = BLOCK_HEAD + body.len();
        for pair in [[0u16, 0], [0, 1], [1, 0], [1, 2]] {
            body.extend_from_slice(&pair[0].to_le_bytes());
            body.extend_from_slice(&pair[1].to_le_bytes());
        }
        let mut out = Vec::new();
        let size = BLOCK_HEAD + body.len();
        words(&mut out, &[size as u32, 2, 1, stitch_offset as u32]);
        out.extend(body);
        out
    }

    #[test]
    fn face_layouts_fill_the_documented_bodies() {
        let bodies = [0x24, 0x3C, 0x24, 0x40, 0x2C, 0x50, 0x2C, 0x54];
        for (kind, body) in (0..8).zip(bodies) {
            let layout = FaceLayout::of(kind).expect("a face type");
            assert_eq!(layout.body(), body, "type {kind}");
        }
        assert!(FaceLayout::of(8).is_none());
    }

    #[test]
    fn reads_faces_seams_and_stitches() {
        let mut data = sample();
        let len = data.len();
        // Bytes behind the block are not part of it.
        data.extend_from_slice(&[0xFF; 7]);
        let block = MeshBlock::parse(&data).expect("mesh block");
        assert_eq!(block.len, len);
        assert_eq!(block.objects.len(), 2);
        assert_eq!(block.face_count(), 2);

        let quad = &block.objects[0].faces[0];
        assert_eq!((quad.kind, quad.command), (7, 0x3E));
        assert!(quad.is_blended());
        // The bit of a shaded face's command is not what makes the drawer blend it.
        assert!(!quad.blends_by_itself());
        assert_eq!(quad.triangles(), [[0, 1, 2], [1, 3, 2]]);
        assert_eq!((quad.colour, quad.normal), (None, None));
        let texture = quad.texture.expect("texture");
        assert_eq!(texture.origin(), (320, 256));
        assert_eq!((texture.depth(), texture.blend_mode()), (1, 0));
        assert_eq!(texture.palette_origin(), (768, 385));
        assert_eq!(
            quad.corners[3],
            Corner {
                position: [40, -5, 7],
                normal: Some([4096, 0, 0]),
                colour: Some([125; 3]),
                uv: Some([4, 200]),
            }
        );

        let triangle = &block.objects[0].faces[1];
        assert_eq!((triangle.kind, triangle.command), (0, 0x20));
        assert!(!triangle.is_blended());
        assert_eq!(triangle.triangles(), [[0, 1, 2]]);
        assert_eq!(triangle.texture, None);
        assert_eq!(triangle.colour, Some([10, 20, 30]));
        assert_eq!(triangle.normal, Some([0, -4096, 0]));
        assert_eq!(
            triangle.corners[2],
            Corner {
                position: [3, 0, -3],
                normal: None,
                colour: None,
                uv: None,
            }
        );

        // The pool of object 1 runs across its two seam chunks.
        let pools: Vec<usize> = block.objects.iter().map(|o| o.seam.len()).collect();
        assert_eq!(pools, [2, 3]);
        assert_eq!(block.stitches.len(), 1);
        let corners = block.stitches[0].map(|corner| *block.seam_vertex(corner));
        assert_eq!(
            corners.map(|vertex| vertex.position[1]),
            [100, 101, 100, 101]
        );
        assert_eq!(corners[0].uv, [3, 4]);
        assert_eq!(corners[0].colour, [128; 3]);
        assert_eq!(corners[0].normal, [0, 0, 4096]);
        assert_eq!(corners[0].texture.expect("texture").origin(), (768, 0));
    }

    #[test]
    fn reads_a_block_without_objects() {
        let mut data = Vec::new();
        words(&mut data, &[16, 0, 0, 0]);
        let block = MeshBlock::parse(&data).expect("empty block");
        assert_eq!((block.objects.len(), block.len), (0, 16));
    }

    #[test]
    fn rejects_what_does_not_add_up() {
        let good = sample();
        assert_eq!(
            MeshBlock::parse(&good[..good.len() - 1]),
            Err(MeshError::Truncated(0))
        );

        // A count that the chunks do not bear out.
        let mut data = good.clone();
        data[BLOCK_HEAD] = 0;
        data[BLOCK_HEAD + 4] = 1;
        assert_eq!(MeshBlock::parse(&data), Err(MeshError::Counts(BLOCK_HEAD)));

        // A chunk type that does not exist.
        let mut data = good.clone();
        data[BLOCK_HEAD + 40] = 10;
        assert_eq!(
            MeshBlock::parse(&data),
            Err(MeshError::UnknownChunk {
                at: BLOCK_HEAD + 40,
                kind: 10
            })
        );

        // A stitch offset that is not where the objects end.
        let mut data = good.clone();
        data[12] ^= 4;
        assert_eq!(MeshBlock::parse(&data), Err(MeshError::BadSize));

        // A stitch corner outside its object's pool.
        let mut data = good.clone();
        let last = data.len() - 2;
        data[last] = 3;
        assert_eq!(MeshBlock::parse(&data), Err(MeshError::BadStitch(0)));

        // A seam count that asks for more than the block holds.
        let mut data = good;
        let count = BLOCK_HEAD + 40 + 4 + 0x54 + 4;
        data[count + 2] = 0xFF;
        assert_eq!(MeshBlock::parse(&data), Err(MeshError::Truncated(count)));
    }

    #[test]
    fn a_swap_gives_an_object_another_mesh_and_its_own_back() {
        // Three objects told apart by the number of their seam vertices.
        let vertex = SeamVertex {
            position: [0; 3],
            normal: [0; 3],
            colour: [0; 3],
            uv: [0; 2],
            texture: None,
        };
        let object = |vertices: usize| MeshObject {
            faces: Vec::new(),
            seam: vec![vertex; vertices],
        };
        let mut block = MeshBlock {
            objects: vec![object(1), object(2), object(3)],
            ..MeshBlock::default()
        };
        let kept = Kept::of(&block, &[0, 7]);
        assert_eq!(kept.own(0), Some(&object(1)));
        assert_eq!((kept.own(1), kept.own(7)), (None, None));

        // Object 0 shows the mesh of object 2, then that of object 1 as it is now.
        assert!(block.swap(&kept, 0, 2));
        assert_eq!(block.objects, [object(3), object(2), object(3)]);
        assert!(block.swap(&kept, 1, 0));
        assert!(block.swap(&kept, 0, 1));
        assert_eq!(block.objects, [object(3), object(3), object(3)]);

        // Its own number brings the kept copy back; an object without one stays.
        assert!(block.swap(&kept, 0, 0));
        assert_eq!(block.objects[0], object(1));
        assert!(!block.swap(&kept, 1, 1));
        assert_eq!(block.objects[1], object(3));

        // Numbers the block does not have change nothing.
        assert!(!block.swap(&kept, 0, 3));
        assert!(!block.swap(&kept, 3, 0));
        assert!(!block.swap(&kept, -1, 0));
        assert!(!block.swap(&kept, 0, -1));
        assert_eq!(block.objects, [object(1), object(3), object(3)]);
    }
}
