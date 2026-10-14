
use std::fmt;

use crate::background::ZOOM_ONE;
use crate::mesh::{MeshBlock, MeshError, TexturePage};
use crate::skeleton::{self, rotation_yxz, rotation_zxy};
use crate::tim::{self, Tim};

/// Sections of a package, and the first word of every package.
pub const SECTIONS: usize = 6;
/// First and last archive file that is a package on its own.
pub const FIRST_FILE: usize = 934;
pub const LAST_FILE: usize = 1080;
/// Bytes of the first section behind its size word.
pub const DATA_SIZE: usize = 200;
/// Bytes of one quad of a sprite frame.
pub const QUAD_SIZE: usize = 28;
/// Bytes of the texture description that ends a sprite.
pub const DESCRIPTION_SIZE: usize = 40;
/// Bytes of one placement of a model.
pub const PLACEMENT_SIZE: usize = 40;
/// Bytes of one record of the first section, and how many point kinds it holds.
pub const START_SIZE: usize = 20;
pub const START_POINTS: usize = 6;

/// Bytes of the head: the section count and one offset per section.
const HEAD: usize = 4 + 4 * SECTIONS;
/// Bytes of a record: kind, number, index.
const RECORD: usize = 12;
/// Bytes of the head of a model entry: the word 2 and the offsets of its two pieces.
const MODEL_HEAD: usize = 12;

/// Why bytes are not a package.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageError {
    /// The bytes do not start with the section count and rising offsets.
    NotAPackage,
    /// The data ends inside this section (0 to 5).
    Truncated(usize),
    /// A count or an offset of this section does not fit the section.
    BadSection(usize),
    /// The mesh block of the model with this number does not read.
    Mesh(usize, MeshError),
    /// The last section is not a texture set.
    Textures,
}

impl fmt::Display for PackageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAPackage => write!(f, "not a six-section package"),
            Self::Truncated(section) => write!(f, "package ends inside section {section}"),
            Self::BadSection(section) => write!(f, "section {section} of the package is damaged"),
            Self::Mesh(model, err) => write!(f, "model {model} of the package: {err}"),
            Self::Textures => write!(f, "the last section of the package is not a texture set"),
        }
    }
}

impl std::error::Error for PackageError {}

/// What a record names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Sprite,
    Model,
    Clip,
}

impl Kind {
    /// The kind behind the first word of a record.
    pub fn of(word: u32) -> Option<Self> {
        match word {
            0 => Some(Self::Sprite),
            1 => Some(Self::Model),
            2 => Some(Self::Clip),
            _ => None,
        }
    }
}

/// One entry of the record list: the thing with this number of this kind is entry `index`
/// of the kind's section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Record {
    pub kind: Kind,
    /// The game's own number of the thing, the same in every package that holds it.
    pub number: u32,
    pub index: usize,
}

/// A flat picture that plays frame by frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sprite {
    /// The quads of every frame; a frame may have none.
    pub frames: Vec<Vec<[u8; QUAD_SIZE]>>,
    /// The words the game makes the sprite's texture page and palette from.
    pub description: [u8; DESCRIPTION_SIZE],
}

impl Sprite {
    /// The quads of a frame as fields; nothing for a frame the sprite does not have.
    pub fn quads(&self, frame: usize) -> Vec<Quad> {
        self.frames
            .get(frame)
            .map(|quads| quads.iter().map(Quad::parse).collect())
            .unwrap_or_default()
    }

    pub fn texture(&self) -> SpriteTexture {
        let d = &self.description;
        let depth = (long(d, 4) & 3) as u16;
        let [palette_x, palette_y, x, y, across, down] =
            [12, 14, 28, 30, 32, 34].map(|at| short(d, at) as u16);
        let texels = |words: u16| ((u32::from(words) << 2) >> depth) as u16;
        SpriteTexture {
            page: TexturePage {
                page: (depth << 7) | SPRITE_BLEND | ((y >> 8) & 1) << 4 | ((x >> 6) & 0xF),
                palette: (palette_y << 6) | ((palette_x >> 4) & 0x3F),
            },
            corner: [texels(x % 64) as u8, y as u8],
            size: [texels(across), down],
        }
    }
}

/// The blend mode a sprite's page word is made with: the texel is added to what is behind.
/// A quad only blends when its caller asks for it, and that caller names the mode again.
const SPRITE_BLEND: u16 = 1 << 5;

/// The picture of a sprite in video memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpriteTexture {
    /// The page and palette words of the sprite's drawing command.
    pub page: TexturePage,
    /// The corner of the picture inside the page, which a quad's own corner is added to.
    pub corner: [u8; 2],
    /// Width and height of the picture in texels.
    pub size: [u16; 2],
}

/// A small model with its table of placements.
#[derive(Debug, Clone, PartialEq)]
pub struct PackageModel {
    pub mesh: MeshBlock,
    /// The two counts in front of the placements: the mesh objects drawn in one frame and
    /// the number of frames. The table holds one times the other.
    pub rows: usize,
    pub columns: usize,
    pub placements: Vec<[u8; PLACEMENT_SIZE]>,
}

impl PackageModel {
    /// The number of frames of the placement table.
    pub fn frames(&self) -> usize {
        self.columns
    }

    /// What is drawn in a frame: the placements from `frame` times `rows` on.
    pub fn frame(&self, frame: usize) -> Vec<Placement> {
        if frame >= self.columns {
            return Vec::new();
        }
        self.placements[frame * self.rows..(frame + 1) * self.rows]
            .iter()
            .map(Placement::parse)
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quad {
    /// Corner of the piece in the sprite's picture.
    pub corner: [u8; 2],
    /// Width and height of the piece; a negative one counts by its size on screen, and its
    /// low byte is what the far texture corner is moved by.
    pub width: i16,
    pub height: i16,
    /// The point of the piece that lies on the quad's place.
    pub centre: [i16; 2],
    /// The size of the piece along each axis of the screen.
    pub size: [i16; 2],
    /// Added to the position the sprite is drawn at.
    pub place: [i16; 3],
    /// A turn about the line of sight, 4096 to the circle.
    pub turn: i16,
    pub colour: [u8; 3],
}

impl Quad {
    pub fn parse(bytes: &[u8; QUAD_SIZE]) -> Self {
        Self {
            corner: [bytes[0], bytes[2]],
            width: short(bytes, 4),
            height: short(bytes, 6),
            centre: [short(bytes, 8), short(bytes, 10)],
            size: [short(bytes, 12), short(bytes, 14)],
            place: [short(bytes, 16), short(bytes, 18), short(bytes, 20)],
            turn: short(bytes, 22),
            colour: [bytes[24], bytes[25], bytes[26]],
        }
    }

    /// Left, top, right and bottom edge of the piece from its place, before the sizes, the
    /// projection and the turn are applied.
    pub fn edges(&self) -> [i32; 4] {
        let [x, y] = self.centre.map(i32::from);
        [
            -x,
            -y,
            i32::from(self.width).abs() - x,
            i32::from(self.height).abs() - y,
        ]
    }

    /// Where the quad's place is in the room, for a sprite drawn at `position` with these
    /// angles: the place is turned as the camera's angles-to-matrix routine turns (about y
    /// first, then x, then z) and added to the position.
    pub fn stands(&self, position: [i32; 3], angles: [i16; 3]) -> [i32; 3] {
        let turn = skeleton::Placement {
            matrix: rotation_zxy(angles),
            position,
        };
        turn.place(self.place)
    }

    /// The four corners on screen, in pixels right of and below the point the quad's place
    /// is projected to: top left, top right, bottom left, bottom right before the turn.
    /// `distance` is the projection distance with the zoom in it, `depth` a quarter of the
    /// place's distance into the screen, `zoom` the zoom of the field ([`ZOOM_ONE`] for
    /// none). `None` for a quad that would come out nine times its size or more, which the
    /// game leaves out. Every division rounds towards zero, as the game's do.
    pub fn corners(&self, distance: i32, depth: i32, zoom: i32) -> Option<[[i32; 2]; 4]> {
        let on_screen =
            |value: i32| i64::from(value) * i64::from(distance) / 256 / i64::from(depth) / 4;
        let [across, down] = self.size.map(i32::from);
        if on_screen(across) * i64::from(zoom) / i64::from(ZOOM_ONE) >= TOO_LARGE {
            return None;
        }
        let [left, top, right, bottom] = self.edges();
        let (left, right) = (on_screen(left * across), on_screen(right * across));
        let (top, bottom) = (on_screen(top * down), on_screen(bottom * down));
        let (sine, cosine) = match self.turn {
            0 => (0, i64::from(skeleton::ONE)),
            turn => (
                i64::from(skeleton::sine(turn.into())),
                i64::from(skeleton::cosine(turn.into())),
            ),
        };
        let one = i64::from(skeleton::ONE);
        let turned = |x: i64, y: i64| {
            [
                ((x * cosine + y * sine) / one) as i32,
                ((y * cosine - x * sine) / one) as i32,
            ]
        };
        Some([
            turned(left, top),
            turned(right, top),
            turned(left, bottom),
            turned(right, bottom),
        ])
    }

    /// The corners of the piece in the texture page of a sprite whose picture starts at
    /// `corner` of the page, in the order of [`Quad::corners`]. The far corners are moved by
    /// the low byte of the width and of the height, so a negative height reads the picture
    /// upwards.
    pub fn texels(&self, corner: [u8; 2]) -> [[u8; 2]; 4] {
        let near = [
            corner[0].wrapping_add(self.corner[0]),
            corner[1].wrapping_add(self.corner[1]),
        ];
        let far = [
            near[0].wrapping_add(self.width as u8),
            near[1].wrapping_add(self.height as u8),
        ];
        [near, [far[0], near[1]], [near[0], far[1]], far]
    }
}

/// The size on screen, in times the quad's own, from which the game leaves a quad out.
const TOO_LARGE: i64 = 9;

pub const HUNG_DEPTHS: std::ops::RangeInclusive<i32> = 101..=4095;
/// The most a standing picture is enlarged by, 256 being one, and its most pixels a side.
const HUNG_SCALE: i32 = 2048;
const HUNG_PIXELS: i32 = 512;

/// What a standing picture is enlarged by along one axis, 256 being one: the size over
/// the depth, times 128, the zoom and the projection distance, two fifths of that over 2
/// to the 18th. The products are the console's 32-bit ones.
fn hung_scale(size: i16, distance: i32, depth: i32, zoom: i32) -> i32 {
    let product = ((i32::from(size) << 7) / depth)
        .wrapping_mul(zoom)
        .wrapping_mul(distance);
    let fifths = ((i64::from(product) * 0x6666_6667) >> 50) as i32;
    (fifths - (product >> 31)).min(HUNG_SCALE)
}

impl SpriteTexture {
    pub fn whole(&self) -> [[u8; 2]; 4] {
        let near = self.corner;
        let far = [
            near[0].wrapping_add(self.size[0].wrapping_sub(1) as u8),
            near[1].wrapping_add(self.size[1].wrapping_sub(1) as u8),
        ];
        [near, [far[0], near[1]], [near[0], far[1]], far]
    }
}

impl Quad {
    pub fn hung(
        &self,
        texels: [[u8; 2]; 4],
        distance: i32,
        depth: i32,
        zoom: i32,
    ) -> Option<[[i32; 2]; 4]> {
        if !HUNG_DEPTHS.contains(&depth) {
            return None;
        }
        let side = |axis: usize| {
            let span = (i32::from(texels[3][axis]) - i32::from(texels[0][axis])).abs();
            let scale = hung_scale(self.size[axis], distance, depth, zoom);
            ((span * scale.abs() / 256).min(HUNG_PIXELS), scale < 0)
        };
        let ((width, mirrored), (height, upside_down)) = (side(0), side(1));
        let left = -(width / 2);
        let (left, right) = match mirrored {
            false => (left, left + width),
            true => (left + width, left),
        };
        let (top, bottom) = match upside_down {
            false => (-height, 0),
            true => (0, -height),
        };
        Some([[left, top], [right, top], [left, bottom], [right, bottom]])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    /// The mesh object of the model; a few placements name none.
    pub object: Option<usize>,
    /// 4096 is the full colour.
    pub brightness: i16,
    /// Added to the position of the thing shown.
    pub place: [i32; 3],
    /// Added to its angles.
    pub angles: [i16; 3],
    /// 4096 = 1.
    pub scale: [i32; 3],
    /// The flag word of the mesh drawer: lit, blended, the blend mode.
    pub flags: u32,
}

impl Placement {
    pub fn parse(bytes: &[u8; PLACEMENT_SIZE]) -> Self {
        Self {
            object: usize::try_from(short(bytes, 0)).ok(),
            brightness: short(bytes, 2),
            place: [long(bytes, 4), long(bytes, 8), long(bytes, 12)],
            angles: [short(bytes, 16), short(bytes, 18), short(bytes, 20)],
            scale: [long(bytes, 24), long(bytes, 28), long(bytes, 32)],
            flags: long(bytes, 36) as u32,
        }
    }

    /// The flag word as the mesh drawer is given it: the fade bit and, in the upper half,
    /// the share of black that the brightness leaves.
    pub fn drawn_flags(&self) -> u32 {
        let share = (4096 - i32::from(self.brightness)) as u32;
        (share << 16) | self.flags | 0x80
    }

    /// The share of black the brightness leaves, 4096 being all of it. The mesh drawer reads
    /// it with its sign: below zero for a brightness above the full colour.
    pub fn fade(&self) -> i32 {
        4096 - i32::from(self.brightness)
    }

    /// Whether the room's light falls on the object; its stored colours are shown otherwise.
    pub fn is_lit(&self) -> bool {
        self.flags & 1 != 0
    }

    /// The console's blend mode that every face of the object is drawn with, in place of the
    /// mode of its texture page; `None` when the faces keep their own way.
    pub fn blend(&self) -> Option<u8> {
        (self.flags & 0x10 != 0).then_some((self.flags >> 5 & 3) as u8)
    }

    pub fn stands(&self, position: [i32; 3], angles: [i16; 3]) -> Stand {
        let turn = rotation_yxz(std::array::from_fn(|k| {
            angles[k].wrapping_add(self.angles[k])
        }));
        let matrix = turn.map(|row| {
            std::array::from_fn(|c| (i64::from(row[c]) * i64::from(self.scale[c]) / 4096) as i32)
        });
        Stand {
            turn,
            drawn: skeleton::Placement {
                matrix,
                position: std::array::from_fn(|k| position[k].wrapping_add(self.place[k])),
            },
        }
    }
}

/// Where a mesh object of a shown model stands in the room.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stand {
    /// The turn alone, which the room's light is brought into the object's space with.
    pub turn: [[i32; 3]; 3],
    /// The turn stretched by the scale, and the position: what a corner is placed with.
    pub drawn: skeleton::Placement,
}

/// On whom the effect of a start is put.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Whom {
    /// The first party of the description the effect reader is given.
    First,
    /// Every entry of that description's list.
    Each,
}

/// One record of the first section: an effect to start at points of a model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Start {
    /// The number the effect task keeps.
    pub number: u32,
    pub whom: Whom,
    /// Kinds of points of the model's point list; one effect is started at each.
    pub points: Vec<u16>,
}

fn short(bytes: &[u8], at: usize) -> i16 {
    i16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn long(bytes: &[u8], at: usize) -> i32 {
    i32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// A package, read to its last section.
#[derive(Debug, Clone, PartialEq)]
pub struct Package {
    /// The first section: 200 bytes the game keeps as they are.
    pub data: Vec<u8>,
    pub records: Vec<Record>,
    pub sprites: Vec<Sprite>,
    pub models: Vec<PackageModel>,
    /// Clips: each a list of compressed pictures, in the form of a movie frame.
    pub clips: Vec<Vec<Vec<u8>>>,
    pub textures: Vec<Tim>,
}

/// A section of the package with reads that fail instead of running over its end.
#[derive(Clone, Copy)]
struct Part<'a> {
    data: &'a [u8],
    number: usize,
}

impl<'a> Part<'a> {
    fn bytes(&self, at: usize, len: usize) -> Result<&'a [u8], PackageError> {
        at.checked_add(len)
            .and_then(|end| self.data.get(at..end))
            .ok_or(PackageError::Truncated(self.number))
    }

    fn word(&self, at: usize) -> Result<usize, PackageError> {
        let bytes = self.bytes(at, 4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize)
    }

    /// A count of things of at least `least` bytes each, checked against the section.
    fn count(&self, at: usize, least: usize) -> Result<usize, PackageError> {
        let count = self.word(at)?;
        if count > self.data.len() / least {
            return Err(PackageError::BadSection(self.number));
        }
        Ok(count)
    }

    /// The entries of a section that starts with a count and one offset per entry.
    fn entries(&self) -> Result<Vec<usize>, PackageError> {
        let count = self.count(0, 4)?;
        (0..count).map(|index| self.word(4 + 4 * index)).collect()
    }
}

impl Package {
    /// Whether bytes start like a package: the section count and the first offset.
    pub fn is_package(data: &[u8]) -> bool {
        data.len() >= HEAD
            && data[..4] == (SECTIONS as u32).to_le_bytes()
            && data[4..8] == (HEAD as u32).to_le_bytes()
    }

    /// Reads a package: an archive file from `FIRST_FILE` on or the kind 0 resource of a room.
    pub fn parse(data: &[u8]) -> Result<Self, PackageError> {
        if !Self::is_package(data) {
            return Err(PackageError::NotAPackage);
        }
        let head = Part { data, number: 0 };
        let mut offsets = [0usize; SECTIONS + 1];
        for (index, offset) in offsets.iter_mut().take(SECTIONS).enumerate() {
            *offset = head.word(4 + 4 * index)?;
        }
        offsets[SECTIONS] = data.len();
        if offsets.windows(2).any(|pair| pair[0] > pair[1]) {
            return Err(PackageError::NotAPackage);
        }
        let part = |number: usize| Part {
            data: &data[offsets[number]..offsets[number + 1]],
            number,
        };

        let first = part(0);
        if first.word(0)? != DATA_SIZE {
            return Err(PackageError::BadSection(0));
        }
        let kept = first.bytes(4, DATA_SIZE)?.to_vec();

        let list = part(1);
        let count = list.count(0, RECORD)?;
        let mut records = Vec::with_capacity(count);
        for index in 0..count {
            let at = 4 + RECORD * index;
            let kind = Kind::of(list.word(at)? as u32).ok_or(PackageError::BadSection(1))?;
            records.push(Record {
                kind,
                number: list.word(at + 4)? as u32,
                index: list.word(at + 8)?,
            });
        }

        let section = part(2);
        let mut sprites = Vec::new();
        for entry in section.entries()? {
            sprites.push(sprite(section, entry)?);
        }

        let section = part(3);
        let mut models = Vec::new();
        for (number, entry) in section.entries()?.into_iter().enumerate() {
            models.push(model(section, entry, number)?);
        }

        let section = part(4);
        let mut clips = Vec::new();
        for entry in section.entries()? {
            let count = section.count(entry, 8)?;
            let mut pictures = Vec::with_capacity(count);
            for index in 0..count {
                let at = section.word(entry + 4 + 8 * index)?;
                let size = section.word(entry + 8 + 8 * index)?;
                let start = entry.checked_add(at).ok_or(PackageError::BadSection(4))?;
                pictures.push(section.bytes(start, size)?.to_vec());
            }
            clips.push(pictures);
        }

        let textures = tim::texture_set(part(5).data).ok_or(PackageError::Textures)?;

        // A record has to name an entry that is there.
        let fits = records.iter().all(|record| {
            record.index
                < match record.kind {
                    Kind::Sprite => sprites.len(),
                    Kind::Model => models.len(),
                    Kind::Clip => clips.len(),
                }
        });
        if !fits {
            return Err(PackageError::BadSection(1));
        }
        Ok(Self {
            data: kept,
            records,
            sprites,
            models,
            clips,
            textures,
        })
    }

    /// The effect starts of the first section. A record whose number is -1 is empty, and one
    /// whose second word is neither 0 nor 1 starts nothing in the game either.
    pub fn starts(&self) -> Vec<Start> {
        self.start_records().into_iter().flatten().collect()
    }

    pub fn start_records(&self) -> Vec<Option<Start>> {
        self.data
            .as_chunks::<START_SIZE>()
            .0
            .iter()
            .map(|record| {
                let number = u32::try_from(long(record, 0)).ok()?;
                let whom = match long(record, 4) {
                    0 => Whom::First,
                    1 => Whom::Each,
                    _ => return None,
                };
                let points = (0..START_POINTS)
                    .filter_map(|index| u16::try_from(short(record, 8 + 2 * index)).ok())
                    .collect();
                Some(Start {
                    number,
                    whom,
                    points,
                })
            })
            .collect()
    }

    /// The entry of the thing with this kind and number, as the game looks one up: the first
    /// record that names both.
    pub fn find(&self, kind: Kind, number: u32) -> Option<usize> {
        self.records
            .iter()
            .find(|record| record.kind == kind && record.number == number)
            .map(|record| record.index)
    }
}

fn sprite(section: Part, entry: usize) -> Result<Sprite, PackageError> {
    let count = section.count(entry, 4)?;
    let mut at = entry + 4;
    let mut frames = Vec::with_capacity(count);
    for _ in 0..count {
        let quads = section.count(at, QUAD_SIZE)?;
        at += 4;
        let mut frame = Vec::with_capacity(quads);
        for _ in 0..quads {
            let mut quad = [0; QUAD_SIZE];
            quad.copy_from_slice(section.bytes(at, QUAD_SIZE)?);
            frame.push(quad);
            at += QUAD_SIZE;
        }
        frames.push(frame);
    }
    let mut description = [0; DESCRIPTION_SIZE];
    description.copy_from_slice(section.bytes(at, DESCRIPTION_SIZE)?);
    Ok(Sprite {
        frames,
        description,
    })
}

fn model(section: Part, entry: usize, number: usize) -> Result<PackageModel, PackageError> {
    let bad = PackageError::BadSection(section.number);
    // The head: the word 2, where the mesh block starts, where the placements start.
    let mesh_at = section.word(entry + 4)?;
    let table_at = section.word(entry + 8)?;
    if mesh_at != MODEL_HEAD || table_at < mesh_at {
        return Err(bad);
    }
    let bytes = section.bytes(entry + mesh_at, table_at - mesh_at)?;
    let mesh = MeshBlock::parse(bytes).map_err(|err| PackageError::Mesh(number, err))?;
    if mesh.len != bytes.len() {
        return Err(bad);
    }
    let table = entry + table_at;
    let rows = section.word(table)?;
    let columns = section.word(table + 4)?;
    let count = rows
        .checked_mul(columns)
        .filter(|count| *count <= section.data.len() / PLACEMENT_SIZE)
        .ok_or(bad)?;
    let mut placements = Vec::with_capacity(count);
    for index in 0..count {
        let mut placement = [0; PLACEMENT_SIZE];
        placement
            .copy_from_slice(section.bytes(table + 8 + PLACEMENT_SIZE * index, PLACEMENT_SIZE)?);
        placements.push(placement);
    }
    Ok(PackageModel {
        mesh,
        rows,
        columns,
        placements,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(out: &mut Vec<u8>, values: &[u32]) {
        for value in values {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }

    /// A package with one sprite named twice, one model and one clip, and no texture.
    fn sample() -> Vec<u8> {
        let mut sections: Vec<Vec<u8>> = Vec::new();

        let mut first = Vec::new();
        words(&mut first, &[DATA_SIZE as u32]);
        first.extend((0..DATA_SIZE).map(|index| index as u8));
        sections.push(first);

        let mut list = Vec::new();
        words(&mut list, &[4, 0, 40, 0, 0, 41, 1, 1, 7, 0, 2, 9, 0]);
        sections.push(list);

        // Two sprites that share one entry: a frame of one quad and an empty frame.
        let mut sprites = Vec::new();
        words(&mut sprites, &[2, 12, 12, 2, 1]);
        sprites.extend([5u8; QUAD_SIZE]);
        words(&mut sprites, &[0]);
        sprites.extend([6u8; DESCRIPTION_SIZE]);
        sections.push(sprites);

        // A model: an empty mesh block and a table of one by two placements.
        let mut models = Vec::new();
        words(&mut models, &[1, 8, 2, 12, 28, 16, 0, 0, 0, 1, 2]);
        models.extend([1u8; PLACEMENT_SIZE]);
        models.extend([2u8; PLACEMENT_SIZE]);
        sections.push(models);

        // A clip of two pictures of three and two bytes.
        let mut clips = Vec::new();
        words(&mut clips, &[1, 8, 2, 20, 3, 23, 2]);
        clips.extend([10, 11, 12, 20, 21]);
        sections.push(clips);

        let mut textures = Vec::new();
        words(&mut textures, &[0]);
        sections.push(textures);

        let mut out = Vec::new();
        words(&mut out, &[SECTIONS as u32]);
        let mut at = HEAD;
        for section in &sections {
            words(&mut out, &[at as u32]);
            at += section.len();
        }
        for section in &sections {
            out.extend_from_slice(section);
        }
        out
    }

    #[test]
    fn reads_every_section() {
        let package = Package::parse(&sample()).expect("package");
        assert_eq!(package.data.len(), DATA_SIZE);
        assert_eq!(package.data[199], 199);
        assert_eq!(package.records.len(), 4);
        assert_eq!(
            package.records[2],
            Record {
                kind: Kind::Model,
                number: 7,
                index: 0
            }
        );
        assert_eq!(package.sprites.len(), 2);
        assert_eq!(package.sprites[0], package.sprites[1]);
        assert_eq!(package.sprites[0].frames, [vec![[5; QUAD_SIZE]], vec![]]);
        assert_eq!(package.sprites[0].description, [6; DESCRIPTION_SIZE]);
        let model = &package.models[0];
        assert!(model.mesh.objects.is_empty());
        assert_eq!((model.rows, model.columns), (1, 2));
        assert_eq!(model.placements, [[1; PLACEMENT_SIZE], [2; PLACEMENT_SIZE]]);
        assert_eq!(package.clips, [vec![vec![10, 11, 12], vec![20, 21]]]);
        assert!(package.textures.is_empty());
    }

    #[test]
    fn reads_a_quad() {
        let mut bytes = [0u8; QUAD_SIZE];
        bytes[0] = 16;
        bytes[2] = 32;
        bytes[4..6].copy_from_slice(&31i16.to_le_bytes());
        bytes[6..8].copy_from_slice(&(-15i16).to_le_bytes());
        bytes[8..10].copy_from_slice(&10i16.to_le_bytes());
        bytes[10..12].copy_from_slice(&4i16.to_le_bytes());
        bytes[12..14].copy_from_slice(&256i16.to_le_bytes());
        bytes[14..16].copy_from_slice(&(-256i16).to_le_bytes());
        bytes[20..22].copy_from_slice(&(-7i16).to_le_bytes());
        bytes[22..24].copy_from_slice(&1024i16.to_le_bytes());
        bytes[24..28].copy_from_slice(&[1, 2, 3, 99]);
        let quad = Quad::parse(&bytes);
        assert_eq!(quad.corner, [16, 32]);
        assert_eq!((quad.width, quad.height), (31, -15));
        assert_eq!(quad.size, [256, -256]);
        assert_eq!(quad.place, [0, 0, -7]);
        assert_eq!(quad.turn, 1024);
        assert_eq!(quad.colour, [1, 2, 3]);
        // A negative height counts by its size.
        assert_eq!(quad.edges(), [-10, -4, 21, 11]);
    }

    #[test]
    fn reads_a_placement_and_the_frames_of_a_model() {
        let mut bytes = [0u8; PLACEMENT_SIZE];
        bytes[0..2].copy_from_slice(&2i16.to_le_bytes());
        bytes[2..4].copy_from_slice(&3072i16.to_le_bytes());
        bytes[8..12].copy_from_slice(&(-1312i32).to_le_bytes());
        bytes[18..20].copy_from_slice(&512i16.to_le_bytes());
        bytes[22..24].copy_from_slice(&21i16.to_le_bytes());
        bytes[24..28].copy_from_slice(&4096i32.to_le_bytes());
        bytes[36..40].copy_from_slice(&0x30u32.to_le_bytes());
        let placement = Placement::parse(&bytes);
        assert_eq!(placement.object, Some(2));
        assert_eq!(placement.place, [0, -1312, 0]);
        assert_eq!(placement.angles, [0, 512, 0]);
        assert_eq!(placement.scale, [4096, 0, 0]);
        // A quarter towards black, blended and added, with the fade bit.
        assert_eq!(placement.drawn_flags(), (1024 << 16) | 0xB0);
        assert_eq!(placement.fade(), 1024);
        assert!(!placement.is_lit());
        assert_eq!(placement.blend(), Some(1));
        // A quarter turn about y at the thing's place, the first column stretched by one
        // and the others by nothing.
        let stand = placement.stands([100, 0, -50], [0, 512, 0]);
        assert_eq!(stand.turn, [[0, 0, 4096], [0, 4096, 0], [-4096, 0, 0]]);
        assert_eq!(stand.drawn.matrix, [[0; 3], [0; 3], [-4096, 0, 0]]);
        assert_eq!(stand.drawn.position, [100, -1312, -50]);
        assert_eq!(stand.drawn.place([10, 20, 30]), [100, -1312, -60]);
        bytes[0..2].copy_from_slice(&(-1i16).to_le_bytes());
        assert_eq!(Placement::parse(&bytes).object, None);

        let package = Package::parse(&sample()).expect("package");
        let model = &package.models[0];
        assert_eq!(model.frames(), 2);
        assert_eq!(model.frame(0), [Placement::parse(&[1; PLACEMENT_SIZE])]);
        assert_eq!(model.frame(1), [Placement::parse(&[2; PLACEMENT_SIZE])]);
        assert!(model.frame(2).is_empty());
        assert_eq!(package.sprites[0].quads(0), [Quad::parse(&[5; QUAD_SIZE])]);
        assert!(package.sprites[0].quads(1).is_empty());
        assert!(package.sprites[0].quads(2).is_empty());
    }

    #[test]
    fn reads_the_effect_starts() {
        let mut package = Package::parse(&sample()).expect("package");
        package.data = vec![0xFF; DATA_SIZE];
        assert!(package.starts().is_empty());
        // The third record: number 18 on every entry of the list, at points 37 and 41.
        let at = 2 * START_SIZE;
        package.data[at..at + 4].copy_from_slice(&18u32.to_le_bytes());
        package.data[at + 4..at + 8].copy_from_slice(&1u32.to_le_bytes());
        package.data[at + 8..at + 10].copy_from_slice(&37u16.to_le_bytes());
        package.data[at + 12..at + 14].copy_from_slice(&41u16.to_le_bytes());
        assert_eq!(
            package.starts(),
            [Start {
                number: 18,
                whom: Whom::Each,
                points: vec![37, 41],
            }]
        );
        // A second word the game has no case for starts nothing.
        package.data[at + 4] = 2;
        assert!(package.starts().is_empty());
    }

    #[test]
    fn finds_a_thing_by_kind_and_number() {
        let package = Package::parse(&sample()).expect("package");
        assert_eq!(package.find(Kind::Sprite, 41), Some(1));
        assert_eq!(package.find(Kind::Model, 7), Some(0));
        assert_eq!(package.find(Kind::Clip, 9), Some(0));
        // The number of a sprite is not the number of a model.
        assert_eq!(package.find(Kind::Model, 40), None);
    }

    #[test]
    fn a_sprite_names_its_picture_in_video_memory() {
        // An 8-bit picture of 32 words by 48 lines at (714, 296), its palette at (320, 480).
        let mut description = [0u8; DESCRIPTION_SIZE];
        description[4] = 1;
        for (at, value) in [(12, 320u16), (14, 480), (28, 714), (30, 296), (32, 32)] {
            description[at..at + 2].copy_from_slice(&value.to_le_bytes());
        }
        description[34] = 48;
        let sprite = Sprite {
            frames: Vec::new(),
            description,
        };
        let texture = sprite.texture();
        // Page 11 of the lower row, 8-bit, texels added; palette row 480 from x = 320.
        assert_eq!(texture.page.page, 0x80 | 0x20 | 0x10 | 11);
        assert_eq!(texture.page.origin(), (704, 256));
        assert_eq!(texture.page.depth(), 1);
        assert_eq!(texture.page.palette_origin(), (320, 480));
        // Two texels to a word: ten words into the page are twenty texels.
        assert_eq!(texture.corner, [20, 40]);
        assert_eq!(texture.size, [64, 48]);
        // Four texels to a word in a 4-bit picture.
        let mut sprite = sprite;
        sprite.description[4] = 0;
        assert_eq!(sprite.texture().corner, [40, 40]);
        assert_eq!(sprite.texture().size, [128, 48]);
    }

    #[test]
    fn a_quad_is_set_around_its_place_on_screen() {
        let mut bytes = [0u8; QUAD_SIZE];
        let fields: [(usize, i16); 8] = [
            (0, 4),
            (2, 8),
            (4, 16),
            (6, -16),
            (8, 8),
            (10, 4),
            (12, 256),
            (14, 512),
        ];
        for (at, value) in fields {
            bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
        }
        bytes[16..18].copy_from_slice(&100i16.to_le_bytes());
        let mut quad = Quad::parse(&bytes);
        // A size of 256 at a depth of a quarter of the projection distance: pixel for texel.
        assert_eq!(
            quad.corners(200, 50, ZOOM_ONE),
            Some([[-8, -8], [8, -8], [-8, 24], [8, 24]])
        );
        // Twice as far away: half the size.
        assert_eq!(
            quad.corners(200, 100, ZOOM_ONE),
            Some([[-4, -4], [4, -4], [-4, 12], [4, 12]])
        );
        // Nine times the size or more is not drawn; the zoom counts.
        assert!(quad.corners(200, 5, ZOOM_ONE).is_none());
        assert!(quad.corners(200, 12, ZOOM_ONE).is_some());
        assert!(quad.corners(200, 12, ZOOM_ONE * 3).is_none());
        // A quarter turn about the line of sight.
        quad.turn = 1024;
        assert_eq!(
            quad.corners(200, 50, ZOOM_ONE),
            Some([[-8, 8], [-8, -8], [24, 8], [24, -8]])
        );
        // The picture's corner, the quad's own, and a height below zero that reads upwards.
        assert_eq!(
            quad.texels([20, 40]),
            [[24, 48], [40, 48], [24, 32], [40, 32]]
        );
        // The place is turned about y (x to minus z) and added to the sprite's position.
        assert_eq!(quad.stands([1, 2, 3], [0; 3]), [101, 2, 3]);
        assert_eq!(quad.stands([1, 2, 3], [0, 1024, 0]), [1, 2, -97]);
    }

    #[test]
    fn a_standing_picture_hangs_by_the_middle_of_its_lower_edge() {
        let texture = SpriteTexture {
            page: TexturePage {
                page: 0,
                palette: 0,
            },
            corner: [32, 64],
            size: [96, 48],
        };
        let texels = texture.whole();
        assert_eq!(texels, [[32, 64], [127, 64], [32, 111], [127, 111]]);
        let mut quad = Quad::parse(&[0u8; QUAD_SIZE]);
        quad.size = [614, 614];
        // 614 * 128 / 300 = 261, times 4096 and 300, two fifths of it over 2 to the 18th:
        // 489 of 256. 95 texels across become 181 pixels, 47 down become 89.
        assert_eq!(
            quad.hung(texels, 300, 300, ZOOM_ONE),
            Some([[-90, -89], [91, -89], [-90, 0], [91, 0]])
        );
        // A size below zero mirrors the picture and is as large.
        quad.size = [-614, 614];
        assert_eq!(
            quad.hung(texels, 300, 300, ZOOM_ONE),
            Some([[91, -89], [-90, -89], [91, 0], [-90, 0]])
        );
        // Near the camera the enlargement stops at eight times, and a side at 512 pixels.
        quad.size = [1177, 1177];
        let narrow = [[0, 0], [63, 0], [0, 159], [63, 159]];
        assert_eq!(
            quad.hung(narrow, 300, 101, ZOOM_ONE),
            Some([[-252, -512], [252, -512], [-252, 0], [252, 0]])
        );
        // Nearer than that, or beyond the ordering table, nothing is drawn.
        assert!(quad.hung(narrow, 300, 100, ZOOM_ONE).is_none());
        assert!(quad.hung(narrow, 300, 4096, ZOOM_ONE).is_none());
    }

    #[test]
    fn rejects_other_bytes() {
        let good = sample();
        assert_eq!(Package::parse(&good[..20]), Err(PackageError::NotAPackage));
        let mut other = good.clone();
        other[0] = 5;
        assert_eq!(Package::parse(&other), Err(PackageError::NotAPackage));
        // A record that names a model that is not there.
        let mut other = good.clone();
        let index = HEAD + 4 + DATA_SIZE + 4 + 2 * RECORD + 8;
        other[index] = 3;
        assert_eq!(Package::parse(&other), Err(PackageError::BadSection(1)));
        // A picture of a clip that is longer than what is left of its section.
        let mut other = good.clone();
        let index = good.len() - 4 - 5 - 4;
        other[index] = 200;
        assert_eq!(Package::parse(&other), Err(PackageError::Truncated(4)));
    }
}
