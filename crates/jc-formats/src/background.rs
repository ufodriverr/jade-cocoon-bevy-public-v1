
use std::cmp::Reverse;
use std::fmt;

use crate::tim::{self, PixelMode, Tim};

/// Bytes of the header a view starts with.
pub const HEADER_SIZE: usize = 0x104;
/// Width and height of a tile, and of a layer's cell, in pixels.
pub const TILE: usize = 32;
/// Where the tile sheet starts in video memory, in 16-bit words.
const SHEET_X: u16 = 320;
/// Lines of the tile sheet.
const SHEET_HEIGHT: usize = 256;
/// Tiles in one column of the sheet.
const TILES_PER_COLUMN: usize = SHEET_HEIGHT / TILE;
/// The line of video memory that holds palette row 0.
const PALETTE_Y: u16 = 496;
/// Colours of one palette row.
const PALETTE_LEN: usize = 256;
/// A layer entry with this bit skips cells instead of naming a tile.
const SKIP: u16 = 0x8000;
/// The bit of a colour that takes part in blending.
const BLEND_BIT: u16 = 0x8000;
/// Width and height of the screen the layers are centred on, in pixels.
pub const SCREEN: (i32, i32) = (320, 240);
/// The zoom value that leaves the picture unscaled.
pub const ZOOM_ONE: i32 = 4096;
/// Slots of the ordering table the layers are sorted into.
const ORDERING_SLOTS: i64 = 1024;
pub const TILE_LAST_TEXEL: usize = TILE - 1;

pub fn tile_edges(cells: (u16, u16), column: u16, row: u16, zoom: i32) -> [i32; 4] {
    let (tile, half) = (TILE as i32, TILE as i32 / 2);
    let x = tile * i32::from(column) - half * i32::from(cells.0);
    let y = tile * i32::from(row) - half * i32::from(cells.1);
    let scaled = |edge: i32| edge * zoom / ZOOM_ONE;
    [scaled(x), scaled(x + tile), scaled(y), scaled(y + tile)]
}

pub fn tile_left_out(left: i32, top: i32, zoom: i32) -> bool {
    let before = -(zoom / 128);
    left < before || left > SCREEN.0 || top < before || top > SCREEN.1
}

/// Why bytes are not a view. The offsets count from the start of the view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundError {
    /// The data ends inside the part at this offset.
    Truncated(usize),
    /// The image at this offset is not a strip of the tile sheet or a palette for it.
    BadTexture(usize),
    /// The layer at this offset names a tile or a cell that does not exist.
    BadLayer(usize),
    /// The group at this offset names a layer that does not exist.
    BadGroup(usize),
}

impl fmt::Display for BackgroundError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated(at) => write!(f, "the view ends inside the part at {at:#x}"),
            Self::BadTexture(at) => write!(f, "the image at {at:#x} does not fit a tile sheet"),
            Self::BadLayer(at) => write!(f, "the layer at {at:#x} leaves its sheet or grid"),
            Self::BadGroup(at) => write!(f, "the group at {at:#x} names a missing layer"),
        }
    }
}

impl std::error::Error for BackgroundError {}

/// A grid of cells with a tile in some of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layer {
    /// Sorts the layer against the 3D scene: 0 is the view's far value (the backdrop), 255
    /// its near value.
    pub depth: u32,
    /// Width in cells.
    pub columns: u16,
    /// Height in cells.
    pub rows: u16,
    /// The run-length entries as stored. [`Layer::cells`] decodes them.
    pub entries: Vec<u16>,
}

impl Layer {
    /// The filled cells in file order: the cell number (left to right, top to bottom) and
    /// the number of its tile.
    pub fn cells(&self) -> impl Iterator<Item = (usize, u16)> + '_ {
        let mut cell = 0;
        self.entries.iter().filter_map(move |&entry| {
            if entry & SKIP != 0 {
                cell += usize::from(entry & !SKIP);
                None
            } else {
                cell += 1;
                Some((cell - 1, entry))
            }
        })
    }
}

/// One step of a group's animation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    /// The layer to draw.
    pub layer: u8,
    /// The palette row to draw it with.
    pub palette_row: u8,
    /// Duration and blend bit as stored.
    pub word: u16,
}

impl Frame {
    /// Game frames the step lasts.
    pub fn duration(&self) -> u8 {
        (self.word & 0x7F) as u8
    }

    /// True when the tiles are drawn in the console's additive mode: the pixels whose colour
    /// has its top bit set are added to the picture behind them.
    pub fn blend(&self) -> bool {
        self.word & 0x80 != 0
    }
}

/// What the game draws as one unit: a still layer (one frame) or an animation that steps
/// through its frames and starts over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub frames: Vec<Frame>,
}

/// Where a group's animation stands: the frame it shows and the game frames that frame has
/// been up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GroupClock {
    pub frame: usize,
    pub count: u8,
}

impl GroupClock {
    /// One game frame passes. A group with one frame stands still; any other counts up and
    /// moves on when the count reaches the frame's duration, from the last frame back to the
    /// first. Returns true when the frame changed.
    pub fn tick(&mut self, group: &Group) -> bool {
        if group.frames.len() < 2 {
            return false;
        }
        self.count = self.count.saturating_add(1);
        if self.count < group.frames[self.frame % group.frames.len()].duration() {
            return false;
        }
        self.count = 0;
        self.frame = (self.frame + 1) % group.frames.len();
        true
    }

    pub fn through(&self, group: &Group, frame: usize) -> bool {
        self.frame != frame
            || (group.frames.get(frame))
                .is_none_or(|shown| u16::from(self.count) + 1 >= u16::from(shown.duration()))
    }

    /// Puts the group on a frame, which starts its time over (`0x800B37D0`).
    pub fn hold(&mut self, frame: usize) {
        (self.frame, self.count) = (frame, 0);
    }
}

/// The group an object names by a number (`0x800B3718`): the number is the depth word of
/// a layer, and the group is the first of two frames or more whose first frame draws the
/// first layer with that word. None where the game would answer the number of groups.
pub fn group_named(depths: &[u32], groups: &[Group], number: i16) -> Option<usize> {
    let layer = (depths.iter()).position(|depth| *depth == i32::from(number) as u32)?;
    (groups.iter())
        .position(|group| group.frames.len() >= 2 && usize::from(group.frames[0].layer) == layer)
}

/// Which pixels of a layer a picture takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pixels {
    /// Every drawn pixel.
    All,
    /// What a blending frame draws over the picture behind it: the colours without the blend
    /// bit.
    Solid,
    /// What a blending frame adds to the picture behind it: the colours with the blend bit.
    Added,
}

impl Pixels {
    fn takes(self, color: u16) -> bool {
        match self {
            Self::All => true,
            Self::Solid => color & BLEND_BIT == 0,
            Self::Added => color & BLEND_BIT != 0,
        }
    }
}

/// RGBA pixels, top row first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Picture {
    fn transparent(width: usize, height: usize) -> Self {
        Self {
            width: width as u32,
            height: height as u32,
            rgba: vec![0; width * height * 4],
        }
    }
}

/// Point-light records in a view header.
pub const POINT_LIGHTS: usize = 8;
/// Bytes of one point-light record.
const POINT_LIGHT_SIZE: usize = 24;
/// Where the point-light records start in the header.
const POINT_LIGHTS_AT: usize = 0x44;

/// One of the eight point-light records of a view header. What it does to a model is in
/// [`crate::light`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PointLight {
    pub position: [i32; 3],
    /// Red, green, blue; 4096 is full.
    pub color: [i16; 3],
    pub range: i32,
}

impl PointLight {
    /// False for a record that is all zero.
    pub fn is_used(&self) -> bool {
        *self != Self::default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewHeader {
    /// Default player position.
    pub player_position: [i32; 3],
    /// Default player angles, 4096 = a full turn. Only the middle one, the turn about the
    /// vertical axis, is used.
    pub player_angles: [i16; 3],
    /// Where the camera is. It never moves within a view.
    pub camera: [i32; 3],
    /// The point the camera looks at.
    pub look_at: [i32; 3],
    /// Camera roll, 4096 = a full turn.
    pub roll: i16,
    /// Distance from the eye to the picture plane in pixels, before the zoom.
    pub projection_distance: i16,
    /// The ambient colour of the room's light on models.
    pub color: [u8; 3],
    /// Direction of the room's directional light.
    pub light_direction: [i16; 3],
    /// Colour of that light: red, green, blue; 4096 is full and larger values occur.
    pub light_color: [i16; 3],
    pub point_lights: [PointLight; POINT_LIGHTS],
}

impl ViewHeader {
    /// Reads a header. `None` when `header` is shorter than [`HEADER_SIZE`].
    pub fn parse(header: &[u8]) -> Option<Self> {
        let header = header.get(..HEADER_SIZE)?;
        let short = |at: usize| i16::from_le_bytes([header[at], header[at + 1]]);
        let long =
            |at: usize| i32::from_le_bytes(header[at..at + 4].try_into().expect("four bytes"));
        let shorts = |at: usize| [short(at), short(at + 2), short(at + 4)];
        let longs = |at: usize| [long(at), long(at + 4), long(at + 8)];
        let mut point_lights = [PointLight::default(); POINT_LIGHTS];
        for (index, light) in point_lights.iter_mut().enumerate() {
            let at = POINT_LIGHTS_AT + POINT_LIGHT_SIZE * index;
            *light = PointLight {
                position: longs(at),
                color: shorts(at + 12),
                range: long(at + 20),
            };
        }
        Some(Self {
            player_position: longs(0x00),
            player_angles: shorts(0x0C),
            camera: longs(0x14),
            look_at: longs(0x20),
            roll: short(0x2C),
            projection_distance: short(0x2E),
            color: [header[0x30], header[0x31], header[0x32]],
            light_direction: shorts(0x34),
            light_color: shorts(0x3C),
            point_lights,
        })
    }

    /// The camera of the view at a zoom ([`ZOOM_ONE`] = unscaled).
    pub fn view_camera(&self, zoom: i32) -> ViewCamera {
        let toward: [f64; 3] =
            std::array::from_fn(|axis| f64::from(self.look_at[axis] - self.camera[axis]));
        let level = toward[0].hypot(toward[2]);
        let turn = -toward[0].atan2(toward[2]);
        let tilt = toward[1].atan2(level);
        let roll = f64::from(self.roll) * std::f64::consts::TAU / 4096.0;
        let ((st, ct), (sx, cx), (sr, cr)) = (turn.sin_cos(), tilt.sin_cos(), roll.sin_cos());
        // The roll times the tilt times the turn, each the plain rotation about its axis.
        let right = [ct, 0.0, st];
        let down = [sx * st, cx, -sx * ct];
        let ahead = [-cx * st, sx, cx * ct];
        let mix = |a: [f64; 3], b: [f64; 3], fa: f64, fb: f64| {
            std::array::from_fn(|axis| a[axis] * fa + b[axis] * fb)
        };
        ViewCamera {
            position: self.camera.map(f64::from),
            rows: [mix(right, down, cr, -sr), mix(right, down, sr, cr), ahead],
            distance: f64::from(i32::from(self.projection_distance) * zoom / ZOOM_ONE),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewCamera {
    position: [f64; 3],
    /// The view matrix by rows: screen right, screen down, into the screen.
    rows: [[f64; 3]; 3],
    /// Distance from the eye to the picture plane in pixels, the zoom applied.
    pub distance: f64,
}

impl ViewCamera {
    /// A point of the room as the camera sees it: right, down, distance into the screen.
    pub fn to_view(&self, point: [i32; 3]) -> [f64; 3] {
        let from: [f64; 3] =
            std::array::from_fn(|axis| f64::from(point[axis]) - self.position[axis]);
        self.rows
            .map(|row| row[0] * from[0] + row[1] * from[1] + row[2] * from[2])
    }

    /// The nearest distance that is drawn: half the projection distance, below which the
    /// console's divider overflows.
    fn nearest(&self) -> f64 {
        (self.distance / 2.0).max(1.0)
    }

    fn on_screen(&self, seen: [f64; 3]) -> [f64; 2] {
        [
            seen[0] * self.distance / seen[2],
            seen[1] * self.distance / seen[2],
        ]
    }

    /// Where a point of the room lies on screen, in pixels right of and below the projection
    /// centre (the screen centre moved by the scroll offset). `None` for a point nearer than
    /// the camera draws, or behind it.
    pub fn project(&self, point: [i32; 3]) -> Option<[f64; 2]> {
        let seen = self.to_view(point);
        (seen[2] >= self.nearest()).then(|| self.on_screen(seen))
    }

    pub fn within_limits(&self, point: [i32; 3]) -> bool {
        const PLACE: std::ops::RangeInclusive<f64> = -32768.0..=32767.0;
        const SCREEN: std::ops::RangeInclusive<f64> = -1024.0..=1023.0;
        let seen = self.to_view(point).map(f64::floor);
        if !PLACE.contains(&seen[0]) || !PLACE.contains(&seen[1]) {
            return false;
        }
        if !(0.0..=65535.0).contains(&seen[2]) || seen[2] * 2.0 <= self.distance {
            return false;
        }
        let at = self.on_screen(seen);
        SCREEN.contains(&at[0].floor()) && SCREEN.contains(&at[1].floor())
    }

    /// The line between two points of the room on screen, cut where it comes nearer than the
    /// camera draws. `None` when all of it does.
    pub fn project_line(&self, from: [i32; 3], to: [i32; 3]) -> Option<[[f64; 2]; 2]> {
        let nearest = self.nearest();
        let (mut a, mut b) = (self.to_view(from), self.to_view(to));
        if a[2] < nearest && b[2] < nearest {
            return None;
        }
        let cut = |near: [f64; 3], far: [f64; 3]| -> [f64; 3] {
            let part = (nearest - near[2]) / (far[2] - near[2]);
            std::array::from_fn(|axis| near[axis] + (far[axis] - near[axis]) * part)
        };
        if a[2] < nearest {
            a = cut(a, b);
        } else if b[2] < nearest {
            b = cut(b, a);
        }
        Some([self.on_screen(a), self.on_screen(b)])
    }
}

/// One view of a background file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View<'a> {
    /// The header as stored: camera and light of the view. [`View::camera`] reads it.
    pub header: &'a [u8],
    /// The near value of the depth range the layers are sorted into.
    pub near: u32,
    /// The far value of that range.
    pub far: u32,
    pub layers: Vec<Layer>,
    pub groups: Vec<Group>,
    /// Bytes from the start of the view to the end of its group block.
    pub len: usize,
    /// Width of the tile sheet in pixels.
    sheet_width: usize,
    /// The tile sheet: `sheet_width * SHEET_HEIGHT` palette indexes.
    sheet: Vec<u8>,
    /// The palette rows; a row the view does not bring is empty.
    palettes: Vec<Vec<u16>>,
}

fn u16_at(data: &[u8], at: usize) -> Result<u16, BackgroundError> {
    let bytes = data.get(at..).and_then(|rest| rest.get(..2));
    let bytes = bytes.ok_or(BackgroundError::Truncated(at))?;
    Ok(u16::from_le_bytes(bytes.try_into().expect("two bytes")))
}

fn u32_at(data: &[u8], at: usize) -> Result<u32, BackgroundError> {
    let bytes = data.get(at..).and_then(|rest| rest.get(..4));
    let bytes = bytes.ok_or(BackgroundError::Truncated(at))?;
    Ok(u32::from_le_bytes(bytes.try_into().expect("four bytes")))
}

/// A count or an offset, as a `usize`.
fn size_at(data: &[u8], at: usize) -> Result<usize, BackgroundError> {
    Ok(u32_at(data, at)? as usize)
}

/// A size word at `at` and the block of that size behind it. Returns the offset of the
/// block and the offset behind it.
fn block(data: &[u8], at: usize) -> Result<(usize, usize), BackgroundError> {
    let size = size_at(data, at)?;
    let end = (at + 4).checked_add(size).filter(|end| *end <= data.len());
    Ok((at + 4, end.ok_or(BackgroundError::Truncated(at))?))
}

/// The start of every item of a block that begins with a count and one offset per item.
/// `table` is where the count is, the offsets count from `base`, and nothing may leave
/// `data[..end]`.
fn items(
    data: &[u8],
    table: usize,
    base: usize,
    end: usize,
) -> Result<Vec<usize>, BackgroundError> {
    let data = &data[..end];
    let count = size_at(data, table)?;
    // Checked before reading, so that a wild count cannot ask for the memory.
    if count > end.saturating_sub(table) / 4 {
        return Err(BackgroundError::Truncated(table));
    }
    (0..count)
        .map(|item| {
            let at = table + 4 + 4 * item;
            let start = base.checked_add(size_at(data, at)?);
            start
                .filter(|start| *start <= end)
                .ok_or(BackgroundError::Truncated(at))
        })
        .collect()
}

impl<'a> View<'a> {
    /// Reads the view that starts at the beginning of `data`: the background file from the
    /// offset a view record gives.
    pub fn parse(data: &'a [u8]) -> Result<Self, BackgroundError> {
        let header = data
            .get(..HEADER_SIZE)
            .ok_or(BackgroundError::Truncated(0))?;

        let (start, end) = block(data, HEADER_SIZE)?;
        let mut textures = Vec::new();
        for at in items(data, start, start, end)? {
            let texture = Tim::parse(&data[..end], at).ok_or(BackgroundError::BadTexture(at))?;
            textures.push((at, texture));
        }
        let (sheet_width, sheet, palettes) = tile_sheet(&textures)?;

        let (start, end) = block(data, end)?;
        let near = u32_at(&data[..end], start)?;
        let far = u32_at(&data[..end], start + 4)?;
        let tiles = sheet_width / TILE * TILES_PER_COLUMN;
        let mut layers = Vec::new();
        for at in items(data, start + 8, start, end)? {
            let layer = &data[..end];
            let count = size_at(layer, at + 4)?;
            if count > (end - at) / 2 {
                return Err(BackgroundError::Truncated(at));
            }
            let entries = (0..count)
                .map(|entry| u16_at(layer, at + 12 + 2 * entry))
                .collect::<Result<_, _>>()?;
            let layer = Layer {
                depth: u32_at(layer, at)?,
                columns: u16_at(layer, at + 8)?,
                rows: u16_at(layer, at + 10)?,
                entries,
            };
            let cells = usize::from(layer.columns) * usize::from(layer.rows);
            if layer
                .cells()
                .any(|(cell, tile)| cell >= cells || usize::from(tile) >= tiles)
            {
                return Err(BackgroundError::BadLayer(at));
            }
            layers.push(layer);
        }

        let (start, end) = block(data, end)?;
        let mut groups = Vec::new();
        for at in items(data, start, start, end)? {
            let group = &data[..end];
            let count = size_at(group, at)?;
            if count > (end - at) / 4 {
                return Err(BackgroundError::Truncated(at));
            }
            let mut frames = Vec::with_capacity(count);
            for frame in 0..count {
                let at = at + 4 + 4 * frame;
                let word = u32_at(group, at)?;
                frames.push(Frame {
                    layer: word as u8,
                    palette_row: (word >> 8) as u8,
                    word: (word >> 16) as u16,
                });
            }
            if frames
                .iter()
                .any(|frame| usize::from(frame.layer) >= layers.len())
            {
                return Err(BackgroundError::BadGroup(at));
            }
            groups.push(Group { frames });
        }

        Ok(Self {
            header,
            near,
            far,
            layers,
            groups,
            len: end,
            sheet_width,
            sheet,
            palettes,
        })
    }

    /// The header's fields: default player placement, camera and lights.
    pub fn camera(&self) -> ViewHeader {
        ViewHeader::parse(self.header).expect("parse kept a whole header")
    }

    /// Palette row `row`, or `None` when the view brings no such row.
    pub fn palette(&self, row: usize) -> Option<&[u16]> {
        self.palettes
            .get(row)
            .map(Vec::as_slice)
            .filter(|palette| !palette.is_empty())
    }

    /// Tiles the sheet has room for.
    pub fn tile_count(&self) -> usize {
        self.sheet_width / TILE * TILES_PER_COLUMN
    }

    pub fn draw_order(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.groups.len())
            .filter(|group| !self.groups[*group].frames.is_empty())
            .collect();
        order.sort_by_key(|group| {
            let layer = &self.layers[usize::from(self.groups[*group].frames[0].layer)];
            (Reverse(self.ordering_slot(layer.depth)), Reverse(*group))
        });
        order
    }

    /// One layer drawn with one palette row, the size of the layer. Cells without a tile and
    /// pixels of colour value 0 stay transparent. Blending is left to whoever draws the
    /// picture. `None` when the layer or the palette row does not exist.
    pub fn layer_picture(&self, layer: usize, palette_row: usize) -> Option<Picture> {
        self.layer_part(layer, palette_row, Pixels::All)
    }

    /// Like [`View::layer_picture`], with only the chosen pixels; the others stay transparent.
    /// A blending frame is its [`Pixels::Solid`] part drawn over the picture behind it and
    /// its [`Pixels::Added`] part added to it.
    pub fn layer_part(&self, layer: usize, palette_row: usize, pixels: Pixels) -> Option<Picture> {
        let layer = self.layers.get(layer)?;
        let palette = self.palette(palette_row)?;
        let mut picture = Picture::transparent(
            usize::from(layer.columns) * TILE,
            usize::from(layer.rows) * TILE,
        );
        self.draw(layer, palette, false, pixels, (0, 0), &mut picture);
        Some(picture)
    }

    /// The slot of the console's ordering table a layer of this depth goes into: depth 0 is
    /// the view's far value, depth 255 its near value, sixteen units of distance to a slot.
    /// A larger slot is further away and drawn earlier.
    pub fn ordering_slot(&self, depth: u32) -> u32 {
        let (near, far) = (i64::from(self.near), i64::from(self.far));
        let distance = near + (far - near) * (255 - i64::from(depth.min(255))) / 255;
        (distance / 16).clamp(0, ORDERING_SLOTS - 1) as u32
    }

    /// The size of layer 0 in pixels at a zoom ([`ZOOM_ONE`] = unscaled): what the scroll
    /// routine measures against the draw area (`crate::area::DrawArea::scroll`).
    pub fn layer_size(&self, zoom: i32) -> (i32, i32) {
        let size = |cells: u16| i32::from(cells) * TILE as i32 * zoom / ZOOM_ONE;
        (self.layers.first()).map_or((0, 0), |first| (size(first.columns), size(first.rows)))
    }

    /// How far the view scrolls either way, horizontally and vertically, at a zoom
    /// ([`ZOOM_ONE`] = unscaled): half of what layer 0, scaled, is larger than the screen.
    pub fn scroll_limit(&self, zoom: i32) -> (i32, i32) {
        let Some(first) = self.layers.first() else {
            return (0, 0);
        };
        let limit = |cells: u16, screen: i32| {
            let size = i32::from(cells) * TILE as i32 * zoom / ZOOM_ONE;
            ((size - screen) / 2).max(0)
        };
        (limit(first.columns, SCREEN.0), limit(first.rows, SCREEN.1))
    }

    /// The whole view as one picture the size of layer 0, which is as far as the view
    /// scrolls: frame `step` of every group (counted round its frames), far groups first,
    /// every layer centred as on screen. `None` for a view without layers, or when a frame
    /// names a palette row the view does not bring.
    pub fn compose(&self, step: usize) -> Option<Picture> {
        let first = self.layers.first()?;
        let (width, height) = (
            usize::from(first.columns) * TILE,
            usize::from(first.rows) * TILE,
        );
        let mut picture = Picture::transparent(width, height);
        for group in self.draw_order() {
            let frames = &self.groups[group].frames;
            let frame = frames[step % frames.len()];
            let layer = &self.layers[usize::from(frame.layer)];
            let palette = self.palette(usize::from(frame.palette_row))?;
            // Layers are centred on the same point, so a smaller one starts further in.
            let origin = (
                (width as isize - (usize::from(layer.columns) * TILE) as isize) / 2,
                (height as isize - (usize::from(layer.rows) * TILE) as isize) / 2,
            );
            self.draw(
                layer,
                palette,
                frame.blend(),
                Pixels::All,
                origin,
                &mut picture,
            );
        }
        Some(picture)
    }

    /// Draws the chosen pixels of the tiles of a layer into a picture, the layer's corner at
    /// `origin`. Pixels outside the picture are dropped.
    fn draw(
        &self,
        layer: &Layer,
        palette: &[u16],
        blend: bool,
        pixels: Pixels,
        origin: (isize, isize),
        picture: &mut Picture,
    ) {
        let columns = usize::from(layer.columns);
        let (width, height) = (picture.width as isize, picture.height as isize);
        for (cell, tile) in layer.cells() {
            // The sheet is filled column by column.
            let tile = usize::from(tile);
            let (sheet_x, sheet_y) = (
                tile / TILES_PER_COLUMN * TILE,
                tile % TILES_PER_COLUMN * TILE,
            );
            let left = origin.0 + (cell % columns * TILE) as isize;
            let top = origin.1 + (cell / columns * TILE) as isize;
            for row in 0..TILE {
                let y = top + row as isize;
                if y < 0 || y >= height {
                    continue;
                }
                for column in 0..TILE {
                    let x = left + column as isize;
                    if x < 0 || x >= width {
                        continue;
                    }
                    let index = self.sheet[(sheet_y + row) * self.sheet_width + sheet_x + column];
                    let color = palette.get(usize::from(index)).copied().unwrap_or(0);
                    if color == 0 || !pixels.takes(color) {
                        continue;
                    }
                    let at = (y * width + x) as usize * 4;
                    let pixel = &mut picture.rgba[at..at + 4];
                    let rgba = tim::color_to_rgba(color);
                    if blend && color & BLEND_BIT != 0 {
                        for (behind, added) in pixel.iter_mut().zip(rgba) {
                            *behind = behind.saturating_add(added);
                        }
                    } else {
                        pixel.copy_from_slice(&rgba);
                    }
                }
            }
        }
    }
}

/// A tile sheet: its width in pixels, its palette indexes and the palette rows.
type TileSheet = (usize, Vec<u8>, Vec<Vec<u16>>);

/// Lays the images of a texture block out as they lie in video memory. Returns the sheet's
/// width in pixels, its palette indexes and the palette rows.
fn tile_sheet(textures: &[(usize, Tim)]) -> Result<TileSheet, BackgroundError> {
    let mut width = 0;
    for (at, texture) in textures {
        let rect = texture.vram;
        let strip = texture.mode == PixelMode::Clut8
            && rect.x >= SHEET_X
            && usize::from(rect.y) + usize::from(rect.h) <= SHEET_HEIGHT
            && (texture.width as usize).is_multiple_of(TILE);
        if !strip {
            return Err(BackgroundError::BadTexture(*at));
        }
        width = width.max(usize::from(rect.x - SHEET_X) * 2 + texture.width as usize);
    }
    let mut sheet = vec![0; width * SHEET_HEIGHT];
    let mut palettes: Vec<Vec<u16>> = Vec::new();
    for (at, texture) in textures {
        let rect = texture.vram;
        let left = usize::from(rect.x - SHEET_X) * 2;
        let strip_width = texture.width as usize;
        for (row, line) in texture.indexes().chunks_exact(strip_width).enumerate() {
            let start = (usize::from(rect.y) + row) * width + left;
            sheet[start..start + strip_width].copy_from_slice(line);
        }
        // Only the first image brings the colours; the others repeat an empty palette block.
        let Some(clut) = texture.clut.as_ref().filter(|clut| !clut.is_shared()) else {
            continue;
        };
        if clut.rect.y < PALETTE_Y || usize::from(clut.rect.w) != PALETTE_LEN {
            return Err(BackgroundError::BadTexture(*at));
        }
        let first = usize::from(clut.rect.y - PALETTE_Y);
        for (row, colors) in clut.colors.as_chunks::<PALETTE_LEN>().0.iter().enumerate() {
            if palettes.len() <= first + row {
                palettes.resize(first + row + 1, Vec::new());
            }
            palettes[first + row] = colors.to_vec();
        }
    }
    Ok((width, sheet, palettes))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: u16 = 0x001F;
    const GREEN: u16 = 0x03E0;
    const BLUE: u16 = 0x7C00;
    /// Dark red with the blend bit.
    const GLOW: u16 = 0x8010;

    #[test]
    fn a_tile_at_a_zoom_drops_the_fraction_of_every_edge() {
        // A layer of six by four cells: the cells start 96 left of and 64 above the middle.
        // At 6000 of 4096 a tile is 46.875 wide, and the edges fall on whole pixels one by
        // one, towards zero on both sides of the middle.
        assert_eq!(tile_edges((6, 4), 0, 0, 6000), [-140, -93, -93, -46]);
        assert_eq!(tile_edges((6, 4), 1, 1, 6000), [-93, -46, -46, 0]);
        assert_eq!(tile_edges((6, 4), 3, 2, 6000), [0, 46, 0, 46]);
        assert_eq!(tile_edges((6, 4), 4, 3, 6000), [46, 93, 46, 93]);
        assert_eq!(tile_edges((6, 4), 5, 3, 6000), [93, 140, 46, 93]);
        // Twice the size has no fraction to drop.
        assert_eq!(tile_edges((6, 4), 0, 0, 8192), [-192, -128, -128, -64]);
    }

    #[test]
    fn a_tile_off_the_screen_is_left_out() {
        // At twice the size a tile is 64 wide: one that starts 64 left of the screen is
        // still drawn, one further left is not.
        assert!(!tile_left_out(-64, 0, 8192));
        assert!(tile_left_out(-65, 0, 8192));
        assert!(!tile_left_out(0, -64, 8192));
        assert!(tile_left_out(0, -65, 8192));
        assert!(!tile_left_out(320, 240, 8192));
        assert!(tile_left_out(321, 0, 8192));
        assert!(tile_left_out(0, 241, 8192));
        // The limit goes with the zoom.
        assert!(tile_left_out(-49, 0, 6144));
        assert!(!tile_left_out(-48, 0, 6144));
    }

    fn words(out: &mut Vec<u8>, values: &[u32]) {
        for value in values {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }

    fn halves(out: &mut Vec<u8>, values: &[u16]) {
        for value in values {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }

    /// An 8-bit TIM strip of the sheet, one tile wide, each tile filled with the palette index
    /// of the same position in `fill`. With the palette rows given, or an empty palette block.
    fn strip(x: u16, fill: &[u8], palette: Option<&[u16]>) -> Vec<u8> {
        let mut out = Vec::new();
        words(&mut out, &[tim::TIM_ID, 9]);
        let colors = palette.unwrap_or(&[]);
        words(&mut out, &[12 + colors.len() as u32 * 2]);
        let rows = (colors.len() / PALETTE_LEN).max(1) as u16;
        halves(&mut out, &[0, PALETTE_Y, PALETTE_LEN as u16, rows]);
        halves(&mut out, colors);
        let height = fill.len() * TILE;
        words(&mut out, &[12 + (TILE * height) as u32]);
        halves(&mut out, &[x, 0, (TILE / 2) as u16, height as u16]);
        for index in fill {
            out.extend(std::iter::repeat_n(*index, TILE * TILE));
        }
        out
    }

    /// A block with a count and one offset per item, the offsets counted from `base` bytes
    /// in front of the count.
    fn table(items: &[Vec<u8>], base: usize) -> Vec<u8> {
        let mut out = Vec::new();
        words(&mut out, &[items.len() as u32]);
        let mut at = base + 4 + 4 * items.len();
        for item in items {
            words(&mut out, &[at as u32]);
            at += item.len();
        }
        for item in items {
            out.extend(item);
        }
        out
    }

    fn layer(depth: u32, columns: u16, rows: u16, entries: &[u16]) -> Vec<u8> {
        let mut out = Vec::new();
        words(&mut out, &[depth, entries.len() as u32]);
        halves(&mut out, &[columns, rows]);
        halves(&mut out, entries);
        out
    }

    fn group(frames: &[(u8, u8, u16)]) -> Vec<u8> {
        let mut out = Vec::new();
        words(&mut out, &[frames.len() as u32]);
        for (layer, row, word) in frames {
            out.extend([*layer, *row]);
            halves(&mut out, &[*word]);
        }
        out
    }

    fn sized(out: &mut Vec<u8>, block: &[u8]) {
        words(out, &[block.len() as u32]);
        out.extend(block);
    }

    /// Two palette rows. Row 0: index 1 red, 2 green, 3 the blending colour. Row 1: index 1
    /// blue.
    fn palettes() -> Vec<u16> {
        let mut colors = vec![0; 2 * PALETTE_LEN];
        colors[1] = RED;
        colors[2] = GREEN;
        colors[3] = GLOW;
        colors[PALETTE_LEN + 1] = BLUE;
        colors
    }

    /// A view with a sheet of two strips (tiles 0 to 2 in the first column, tile 8 in the
    /// second), a backdrop of 2 by 2 cells, a nearer layer of one cell and a blended one.
    fn sample() -> Vec<u8> {
        let mut data = vec![0xAB; HEADER_SIZE];
        let strips = [
            strip(SHEET_X, &[1, 2, 3], Some(&palettes())),
            strip(SHEET_X + 16, &[0], None),
        ];
        sized(&mut data, &table(&strips, 0));
        let layers = [
            // Red, a skipped cell, green and red again.
            layer(0, 2, 2, &[0, SKIP | 1, 1, 0]),
            // One cell that lands on the middle of the backdrop.
            layer(200, 1, 1, &[1]),
            // The blending tile over the top left cell.
            layer(100, 2, 2, &[2]),
            // An empty tile: nothing is drawn.
            layer(255, 2, 2, &[SKIP | 3, 8]),
        ];
        let mut block = Vec::new();
        words(&mut block, &[1000, 12500]);
        block.extend(table(&layers, 8));
        sized(&mut data, &block);
        let groups = [
            group(&[(1, 0, 0)]),
            group(&[(0, 0, 0), (0, 1, 0x0005)]),
            group(&[(2, 0, 0x0080 | 3)]),
            group(&[(3, 0, 0)]),
        ];
        sized(&mut data, &table(&groups, 0));
        data
    }

    fn pixel(picture: &Picture, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * picture.width + x) * 4) as usize;
        picture.rgba[at..at + 4].try_into().expect("four bytes")
    }

    #[test]
    fn reads_layers_groups_and_palettes() {
        let data = sample();
        let view = View::parse(&data).expect("view");
        assert_eq!(view.len, data.len());
        assert_eq!(view.header, [0xAB; HEADER_SIZE]);
        assert_eq!((view.near, view.far), (1000, 12500));
        assert_eq!(view.tile_count(), 16);
        let sizes: Vec<_> = view
            .layers
            .iter()
            .map(|layer| (layer.depth, layer.columns, layer.rows))
            .collect();
        assert_eq!(sizes, [(0, 2, 2), (200, 1, 1), (100, 2, 2), (255, 2, 2)]);
        let cells: Vec<_> = view.layers[0].cells().collect();
        assert_eq!(cells, [(0, 0), (2, 1), (3, 0)]);
        assert_eq!(view.layers[3].cells().collect::<Vec<_>>(), [(3, 8)]);
        let frames: Vec<usize> = view.groups.iter().map(|g| g.frames.len()).collect();
        assert_eq!(frames, [1, 2, 1, 1]);
        let second = view.groups[1].frames[1];
        assert_eq!((second.layer, second.palette_row), (0, 1));
        assert_eq!((second.duration(), second.blend()), (5, false));
        let glow = view.groups[2].frames[0];
        assert_eq!((glow.duration(), glow.blend()), (3, true));
        assert_eq!(view.palette(0).map(|row| row[2]), Some(GREEN));
        assert_eq!(view.palette(1).map(|row| row[1]), Some(BLUE));
        assert_eq!(view.palette(2), None);
        // Far first: the backdrop, the blended layer, the near cell, the empty layer.
        assert_eq!(view.draw_order(), [1, 2, 0, 3]);
    }

    #[test]
    fn reads_the_header_fields() {
        let mut header = Vec::new();
        words(&mut header, &[50, (-20i32) as u32, 60]);
        halves(&mut header, &[0, 0x400, 0, 0]);
        words(&mut header, &[1000, (-2000i32) as u32, 3000]);
        words(&mut header, &[10, 20, (-30i32) as u32]);
        halves(&mut header, &[(-47i16) as u16, 640]);
        header.extend([1, 2, 3, 0]);
        halves(&mut header, &[(-4096i16) as u16, 100, 200, 0]);
        halves(&mut header, &[4096, 8192, 2048, 0]);
        // Point light 0, an unused record, point light 2.
        words(&mut header, &[7, (-8i32) as u32, 9]);
        halves(&mut header, &[4096, 0, 1024, 0]);
        words(&mut header, &[12500]);
        header.resize(POINT_LIGHTS_AT + 2 * POINT_LIGHT_SIZE, 0);
        words(&mut header, &[0, 0, 0]);
        halves(&mut header, &[0, 0, 0, 0]);
        words(&mut header, &[300]);
        header.resize(HEADER_SIZE, 0);

        let read = ViewHeader::parse(&header).expect("header");
        assert_eq!(read.player_position, [50, -20, 60]);
        assert_eq!(read.player_angles, [0, 0x400, 0]);
        assert_eq!(read.camera, [1000, -2000, 3000]);
        assert_eq!(read.look_at, [10, 20, -30]);
        assert_eq!((read.roll, read.projection_distance), (-47, 640));
        assert_eq!(read.color, [1, 2, 3]);
        assert_eq!(read.light_direction, [-4096, 100, 200]);
        assert_eq!(read.light_color, [4096, 8192, 2048]);
        let first = read.point_lights[0];
        assert_eq!(first.position, [7, -8, 9]);
        assert_eq!((first.color, first.range), ([4096, 0, 1024], 12500));
        // A record with nothing but a range still counts as used.
        let used: Vec<bool> = read.point_lights.iter().map(|l| l.is_used()).collect();
        assert_eq!(used, [true, false, true, false, false, false, false, false]);
        assert_eq!(read.point_lights[2].range, 300);

        assert_eq!(ViewHeader::parse(&header[..HEADER_SIZE - 1]), None);
        // A view hands out the same fields.
        let mut data = sample();
        data[..HEADER_SIZE].copy_from_slice(&header);
        assert_eq!(View::parse(&data).expect("view").camera(), read);
    }

    /// A header with nothing but a camera.
    fn camera_header(camera: [i32; 3], look_at: [i32; 3], roll: i16, distance: i16) -> ViewHeader {
        let mut header = ViewHeader::parse(&[0; HEADER_SIZE]).expect("header");
        (header.camera, header.look_at) = (camera, look_at);
        (header.roll, header.projection_distance) = (roll, distance);
        header
    }

    fn close(got: Option<[f64; 2]>, wanted: [f64; 2]) -> bool {
        got.is_some_and(|got| {
            (got[0] - wanted[0]).abs() < 1e-6 && (got[1] - wanted[1]).abs() < 1e-6
        })
    }

    #[test]
    fn projects_through_the_view_camera() {
        // Straight along z: x is right, y is down, sizes shrink with the distance.
        let plain = camera_header([0, 0, 0], [0, 0, 500], 0, 200).view_camera(ZOOM_ONE);
        assert!(close(plain.project([10, 20, 100]), [20.0, 40.0]));
        assert!(close(plain.project([10, 20, 400]), [5.0, 10.0]));
        // The zoom scales the projection distance.
        let zoomed = camera_header([0, 0, 0], [0, 0, 500], 0, 200).view_camera(ZOOM_ONE * 2);
        assert_eq!(zoomed.distance, 400.0);
        assert!(close(zoomed.project([10, 20, 400]), [10.0, 20.0]));

        // Anywhere: the point looked at is the centre, a point over it (y points down) lies
        // above it on screen, and the distance is the one between the two points.
        let (eye, target) = ([1000, -2000, 3000], [400, -1200, 2000]);
        let tilted = camera_header(eye, target, 0, 300).view_camera(ZOOM_ONE);
        assert!(close(tilted.project(target), [0.0, 0.0]));
        let over = tilted.project([400, -1300, 2000]).expect("in front");
        assert!(over[0].abs() < 1e-6 && over[1] < 0.0);
        let length = (600.0f64 * 600.0 + 800.0 * 800.0 + 1000.0 * 1000.0).sqrt();
        assert!((tilted.to_view(target)[2] - length).abs() < 1e-6);

        // Looking along x, a larger z is to the left.
        let sideways = camera_header([0, 0, 0], [500, 0, 0], 0, 200).view_camera(ZOOM_ONE);
        assert!(close(sideways.project([100, 0, 10]), [-20.0, 0.0]));

        // A quarter turn of roll takes the right of the picture to its bottom.
        let rolled = camera_header([0, 0, 0], [0, 0, 500], 1024, 200).view_camera(ZOOM_ONE);
        assert!(close(rolled.project([10, 0, 100]), [0.0, 20.0]));
    }

    #[test]
    fn a_point_is_within_the_limits_of_the_matrix_unit() {
        let camera = camera_header([0, 0, 0], [0, 0, 500], 0, 200).view_camera(ZOOM_ONE);
        assert!(camera.within_limits([0, 0, 500]));
        // Half the projection distance is too near, and so is anything behind the camera.
        assert!(!camera.within_limits([0, 0, 100]));
        assert!(camera.within_limits([0, 0, 101]));
        assert!(!camera.within_limits([0, 0, -500]));
        // 1023 pixels right of the centre is the last place on screen, 1024 left of it.
        assert!(camera.within_limits([1023, 0, 200]));
        assert!(!camera.within_limits([1024, 0, 200]));
        assert!(camera.within_limits([0, -1024, 200]));
        assert!(!camera.within_limits([0, -1025, 200]));
        // The depth and the place beside the camera have 16 bits.
        assert!(camera.within_limits([0, 0, 65535]));
        assert!(!camera.within_limits([0, 0, 65536]));
    }

    #[test]
    fn cuts_lines_at_the_nearest_distance() {
        // Nothing nearer than half the projection distance is drawn.
        let camera = camera_header([0, 0, 0], [0, 0, 500], 0, 200).view_camera(ZOOM_ONE);
        assert_eq!(camera.project([0, 0, 99]), None);
        assert_eq!(camera.project([0, 0, -300]), None);
        assert!(camera.project([0, 0, 100]).is_some());

        let whole = camera
            .project_line([10, 0, 100], [10, 0, 200])
            .expect("in front");
        assert!(close(Some(whole[0]), [20.0, 0.0]) && close(Some(whole[1]), [10.0, 0.0]));
        // One end behind the camera: the line ends where it reaches the nearest distance.
        let cut = camera
            .project_line([50, 0, 300], [-150, 0, -100])
            .expect("partly in front");
        assert!(close(Some(cut[0]), [50.0 * 200.0 / 300.0, 0.0]));
        assert!(close(Some(cut[1]), [-50.0 * 200.0 / 100.0, 0.0]));
        let back = camera
            .project_line([-150, 0, -100], [50, 0, 300])
            .expect("partly in front");
        assert_eq!(back, [cut[1], cut[0]]);
        assert_eq!(camera.project_line([0, 0, 50], [10, 0, -100]), None);
    }

    #[test]
    fn draws_a_layer_with_a_palette_row() {
        let data = sample();
        let view = View::parse(&data).expect("view");
        let picture = view.layer_picture(0, 0).expect("layer 0");
        assert_eq!((picture.width, picture.height), (64, 64));
        assert_eq!(pixel(&picture, 0, 0), [255, 0, 0, 255]);
        assert_eq!(pixel(&picture, 31, 31), [255, 0, 0, 255]);
        // The skipped cell.
        assert_eq!(pixel(&picture, 32, 0), [0, 0, 0, 0]);
        assert_eq!(pixel(&picture, 0, 32), [0, 255, 0, 255]);
        assert_eq!(pixel(&picture, 63, 63), [255, 0, 0, 255]);
        // The other row turns red into blue and has no green.
        let picture = view.layer_picture(0, 1).expect("layer 0");
        assert_eq!(pixel(&picture, 0, 0), [0, 0, 255, 255]);
        assert_eq!(pixel(&picture, 0, 32), [0, 0, 0, 0]);
        // A tile of colour value 0 leaves its cell open.
        let picture = view.layer_picture(3, 0).expect("layer 3");
        assert!(picture.rgba.iter().all(|byte| *byte == 0));
        assert_eq!(view.layer_picture(4, 0), None);
        assert_eq!(view.layer_picture(0, 2), None);
    }

    #[test]
    fn splits_a_blending_layer_into_its_two_parts() {
        let data = sample();
        let view = View::parse(&data).expect("view");
        // Layer 2 is one tile of the blending colour: nothing solid, all of it added.
        let solid = view.layer_part(2, 0, Pixels::Solid).expect("layer 2");
        assert!(solid.rgba.iter().all(|byte| *byte == 0));
        let added = view.layer_part(2, 0, Pixels::Added).expect("layer 2");
        assert_eq!(pixel(&added, 0, 0), [132, 0, 0, 255]);
        assert_eq!(pixel(&added, 32, 0), [0, 0, 0, 0]);
        // Layer 0 has no blending colour: all of it solid.
        let whole = view.layer_picture(0, 0).expect("layer 0");
        assert_eq!(view.layer_part(0, 0, Pixels::Solid), Some(whole));
        let added = view.layer_part(0, 0, Pixels::Added).expect("layer 0");
        assert!(added.rgba.iter().all(|byte| *byte == 0));
        assert_eq!(view.layer_part(4, 0, Pixels::Added), None);
    }

    #[test]
    fn sorts_layers_into_ordering_slots() {
        let data = sample();
        let view = View::parse(&data).expect("view");
        // Depth 0 is the far value, depth 255 the near value, sixteen units to a slot.
        assert_eq!(view.ordering_slot(0), 12500 / 16);
        assert_eq!(view.ordering_slot(255), 1000 / 16);
        assert_eq!(view.ordering_slot(100), (1000 + 11500 * 155 / 255) / 16);
        assert_eq!(view.ordering_slot(999), 1000 / 16);
        let mut far = view.clone();
        far.far = 100_000;
        assert_eq!(far.ordering_slot(0), 1023);
    }

    #[test]
    fn the_lower_group_of_one_slot_lies_on_top() {
        let data = sample();
        let mut view = View::parse(&data).expect("view");
        // The backdrop first, and no distance between near and far: every group in one slot.
        view.groups.swap(0, 1);
        view.near = view.far;
        assert_eq!(view.draw_order(), [3, 2, 1, 0]);
        // The middle cell of the nearer layer (now group 1) is hidden by the backdrop.
        let picture = view.compose(0).expect("picture");
        assert_eq!(pixel(&picture, 20, 20), [255, 0, 0, 255]);
        // Two depths that fall into one slot are ordered by group number as well.
        let mut view = View::parse(&data).expect("view");
        view.far = view.near + 7;
        assert_ne!(view.layers[0].depth, view.layers[1].depth);
        assert_eq!(view.ordering_slot(0), view.ordering_slot(255));
        assert_eq!(view.draw_order(), [3, 2, 1, 0]);
    }

    #[test]
    fn scrolls_as_far_as_layer_0_reaches() {
        let data = sample();
        let mut view = View::parse(&data).expect("view");
        // Smaller than the screen: no scrolling.
        assert_eq!(view.scroll_limit(ZOOM_ONE), (0, 0));
        view.layers[0].columns = 10;
        view.layers[0].rows = 14;
        assert_eq!(view.scroll_limit(ZOOM_ONE), (0, 104));
        view.layers[0].columns = 16;
        view.layers[0].rows = 8;
        assert_eq!(view.scroll_limit(ZOOM_ONE), (96, 8));
        assert_eq!(view.scroll_limit(2 * ZOOM_ONE), (352, 136));
        view.layers.clear();
        assert_eq!(view.scroll_limit(ZOOM_ONE), (0, 0));
    }

    #[test]
    fn steps_a_group_through_its_frames() {
        let frame = |word| Frame {
            layer: 0,
            palette_row: 0,
            word,
        };
        let still = Group {
            frames: vec![frame(1)],
        };
        let mut clock = GroupClock::default();
        assert!(!clock.tick(&still));
        assert_eq!(clock, GroupClock::default());

        // Durations 2 and 3; the blend bit is not part of the duration.
        let group = Group {
            frames: vec![frame(2), frame(0x80 | 3)],
        };
        let shown: Vec<usize> = (0..11)
            .map(|_| {
                clock.tick(&group);
                clock.frame
            })
            .collect();
        assert_eq!(shown, [0, 1, 1, 1, 0, 0, 1, 1, 1, 0, 0]);

        // A held frame starts its time over; it is through in the last game frame of its
        // duration, or when the group has gone on by itself.
        clock.hold(1);
        assert_eq!((clock.frame, clock.count), (1, 0));
        assert!(!clock.through(&group, 1));
        clock.tick(&group);
        assert!(!clock.through(&group, 1));
        clock.tick(&group);
        assert!(clock.through(&group, 1));
        assert!(clock.through(&group, 0) && clock.through(&group, 7));
    }

    #[test]
    fn a_group_is_named_by_the_depth_of_its_first_layer() {
        let frame = |layer| Frame {
            layer,
            palette_row: 0,
            word: 4,
        };
        let group = |layers: &[u8]| Group {
            frames: layers.iter().map(|layer| frame(*layer)).collect(),
        };
        // Layer 1 has the depth 200. Group 0 draws it still, group 2 starts on it.
        let depths = [0, 200, 200, 35];
        let groups = [group(&[1]), group(&[3, 1]), group(&[1, 2]), group(&[1, 3])];
        assert_eq!(group_named(&depths, &groups, 200), Some(2));
        assert_eq!(group_named(&depths, &groups, 35), Some(1));
        // No layer of that depth, and a layer no animated group starts on.
        assert_eq!(group_named(&depths, &groups, 7), None);
        assert_eq!(group_named(&depths, &groups, 0), None);
    }

    #[test]
    fn composes_far_to_near_and_blends() {
        let data = sample();
        let view = View::parse(&data).expect("view");
        let picture = view.compose(0).expect("picture");
        assert_eq!((picture.width, picture.height), (64, 64));
        // Top left: the blending colour (132 red) added to the backdrop's red, limited.
        assert_eq!(pixel(&picture, 0, 0), [255, 0, 0, 255]);
        // The skipped cell of the backdrop is untouched by the blended layer.
        assert_eq!(pixel(&picture, 40, 0), [0, 0, 0, 0]);
        // The near cell is centred: green from (16, 16) to (47, 47), over whatever is there.
        assert_eq!(pixel(&picture, 16, 16), [0, 255, 0, 255]);
        assert_eq!(pixel(&picture, 47, 47), [0, 255, 0, 255]);
        assert_eq!(pixel(&picture, 40, 20), [0, 255, 0, 255]);
        assert_eq!(pixel(&picture, 48, 48), [255, 0, 0, 255]);
        // Bottom left is the backdrop's green: the blended layer only fills the top left cell.
        assert_eq!(pixel(&picture, 0, 40), [0, 255, 0, 255]);

        // The second step of the backdrop's group uses palette row 1: blue, plus the glow.
        let picture = view.compose(1).expect("picture");
        assert_eq!(pixel(&picture, 0, 0), [132, 0, 255, 255]);
        assert_eq!(pixel(&picture, 63, 63), [0, 0, 255, 255]);
        assert_eq!(pixel(&picture, 0, 40), [0, 0, 0, 0]);
    }

    #[test]
    fn rejects_short_and_impossible_data() {
        let data = sample();
        // Every cut of the data is an error, never a panic.
        for len in 0..data.len() {
            assert!(View::parse(&data[..len]).is_err(), "cut at {len}");
        }

        // A tile the sheet does not have.
        let mut view = vec![0; HEADER_SIZE];
        sized(
            &mut view,
            &table(&[strip(SHEET_X, &[1], Some(&palettes()))], 0),
        );
        let texture_end = view.len();
        let mut block = Vec::new();
        words(&mut block, &[0, 0]);
        block.extend(table(&[layer(0, 1, 1, &[8])], 8));
        sized(&mut view, &block);
        sized(&mut view, &table(&[], 0));
        assert!(matches!(
            View::parse(&view),
            Err(BackgroundError::BadLayer(_))
        ));

        // A tile behind the last cell.
        let mut late = view[..texture_end].to_vec();
        let mut block = Vec::new();
        words(&mut block, &[0, 0]);
        block.extend(table(&[layer(0, 1, 1, &[0, 0])], 8));
        sized(&mut late, &block);
        sized(&mut late, &table(&[], 0));
        assert!(matches!(
            View::parse(&late),
            Err(BackgroundError::BadLayer(_))
        ));

        // A group for a layer that is not there.
        let mut lost = view[..texture_end].to_vec();
        let mut block = Vec::new();
        words(&mut block, &[0, 0]);
        block.extend(table(&[layer(0, 1, 1, &[0])], 8));
        sized(&mut lost, &block);
        sized(&mut lost, &table(&[group(&[(1, 0, 0)])], 0));
        assert!(matches!(
            View::parse(&lost),
            Err(BackgroundError::BadGroup(_))
        ));

        // An image that is not an 8-bit strip.
        let mut direct = Vec::new();
        words(&mut direct, &[tim::TIM_ID, 2, 12 + 2]);
        halves(&mut direct, &[SHEET_X, 0, 1, 1, 0]);
        let mut wrong = vec![0; HEADER_SIZE];
        sized(&mut wrong, &table(&[direct], 0));
        assert!(matches!(
            View::parse(&wrong),
            Err(BackgroundError::BadTexture(_))
        ));

        // Wild counts.
        let mut wild = data.clone();
        wild[HEADER_SIZE + 4..HEADER_SIZE + 8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(View::parse(&wild).is_err());
    }
}
