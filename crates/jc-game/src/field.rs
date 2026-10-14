
use std::collections::HashMap;
use std::collections::hash_map::Entry;

use bevy::asset::{RenderAssetUsages, uuid_handle};
use bevy::camera::visibility::NoFrustumCulling;
use bevy::camera::{CompositingSpace, ScalingMode, Viewport};
use bevy::mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, Extent3d,
    RenderPipelineDescriptor, SpecializedMeshPipelineError, TextureDimension, TextureFormat,
};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dKey, Material2dPlugin};
use jc_formats::area::DrawArea;
use jc_formats::background::{
    self, Group, GroupClock, Picture, Pixels, SCREEN, TILE, TILE_LAST_TEXEL, View, ViewCamera,
    ViewHeader, ZOOM_ONE,
};
use jc_formats::field::Region;

/// Screen refreshes in a second: the disc sets the European picture standard.
const BLANKS_PER_SECOND: f64 = 50.0;
const BLANKS_PER_GAME_FRAME: f64 = 2.0;
/// Game frames in a second.
const GAME_FRAMES_PER_SECOND: f64 = BLANKS_PER_SECOND / BLANKS_PER_GAME_FRAME;
/// Pixels the debug keys scroll in one game frame.
const KEY_SCROLL: i32 = 2;
/// Distance on the depth axis between two slots of the ordering table. The slots have to fit
/// between the camera's near and far planes.
pub const SLOT_DEPTH: f32 = 0.9;
/// Distance on the depth axis between two groups of the same slot.
const GROUP_DEPTH: f32 = 0.001;
/// Where the black outside the draw area lies on the depth axis: in front of every layer
/// and thing, which lie at zero and behind, and behind the windows.
const OUTSIDE_DEPTH: f32 = 5.0;
const WHITE_DEPTH: f32 = 4.0;
/// Where the black of a held picture lies: over everything of the room, the white too.
const DARK_DEPTH: f32 = 4.5;
/// The colour of a tile that leaves its texels as they are.
const TILE_UNCHANGED: u8 = 128;
/// Where the shade of a group's solid pixels lies between them and its added ones.
const SHADE_OVER: f32 = 0.25;

const ADDED_SHADER: Handle<Shader> = uuid_handle!("6f0c2f0e-3d0b-4f0a-9a57-2f6f1f1b7c11");

pub struct FieldPlugin;

impl Plugin for FieldPlugin {
    fn build(&self, app: &mut App) {
        let shader = Shader::from_wgsl(include_str!("added.wgsl"), "added.wgsl");
        app.world_mut()
            .resource_mut::<Assets<Shader>>()
            .insert(&ADDED_SHADER, shader)
            .expect("a fixed handle takes an asset");
        app.add_plugins(Material2dPlugin::<AddedMaterial>::default())
            .add_plugins(Material2dPlugin::<TakenMaterial>::default())
            .insert_resource(Time::<Fixed>::from_hz(GAME_FRAMES_PER_SECOND))
            .add_systems(
                FixedUpdate,
                (scroll_with_keys, tick_groups).run_if(crate::movie_view::room_runs),
            )
            .add_systems(
                Update,
                (fit_screen, place_groups, cut_to_the_area, draw_walk_lines),
            );
    }
}

/// How a picture of the background meets what lies behind it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Drawn {
    /// Its pixels are added to the picture behind them.
    Added,
    /// Its pixels lie over the picture behind them: the solid pixels of a layer.
    Over,
    /// The solid pixels in black, over themselves as thick as the tiles are darkened.
    Shade,
}

impl From<&AddedMaterial> for Drawn {
    fn from(material: &AddedMaterial) -> Self {
        material.drawn
    }
}

/// Draws a picture of the background: its pixels added to what lies behind them, or laid
/// over it ([`Drawn`]).
#[derive(Asset, TypePath, AsBindGroup, Clone)]
#[bind_group_data(Drawn)]
pub struct AddedMaterial {
    #[texture(0)]
    #[sampler(1)]
    texture: Handle<Image>,
    /// What the colours are multiplied by: the brightness of the tiles. Of a picture that
    /// lies over the one behind it the fourth share is how thick it lies.
    #[uniform(2)]
    share: Vec4,
    drawn: Drawn,
}

impl Material2d for AddedMaterial {
    fn fragment_shader() -> ShaderRef {
        ADDED_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        // Sorted with the other pictures; `specialize` replaces the blending itself.
        AlphaMode2d::Blend
    }

    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        if key.bind_group_data != Drawn::Added {
            // Laid over the picture behind it, as a sprite is.
            if let Some(fragment) = &mut descriptor.fragment {
                fragment.shader_defs.push("OVER".into());
            }
            return Ok(());
        }
        // The console's additive mode: background plus pixel, limited at full brightness.
        let added = BlendState {
            color: BlendComponent {
                src_factor: BlendFactor::SrcAlpha,
                dst_factor: BlendFactor::One,
                operation: BlendOperation::Add,
            },
            alpha: BlendComponent {
                src_factor: BlendFactor::Zero,
                dst_factor: BlendFactor::One,
                operation: BlendOperation::Add,
            },
        };
        if let Some(fragment) = &mut descriptor.fragment {
            for target in fragment.targets.iter_mut().flatten() {
                target.blend = Some(added);
            }
        }
        Ok(())
    }
}

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct TakenMaterial {
    #[texture(0)]
    #[sampler(1)]
    texture: Handle<Image>,
    /// What the colours are multiplied by.
    #[uniform(2)]
    share: Vec4,
}

impl Material2d for TakenMaterial {
    fn fragment_shader() -> ShaderRef {
        ADDED_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        // Sorted with the other pictures; `specialize` replaces the blending itself.
        AlphaMode2d::Blend
    }

    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let taken = BlendState {
            color: BlendComponent {
                src_factor: BlendFactor::SrcAlpha,
                dst_factor: BlendFactor::One,
                operation: BlendOperation::ReverseSubtract,
            },
            alpha: BlendComponent {
                src_factor: BlendFactor::Zero,
                dst_factor: BlendFactor::One,
                operation: BlendOperation::Add,
            },
        };
        if let Some(fragment) = &mut descriptor.fragment {
            for target in fragment.targets.iter_mut().flatten() {
                target.blend = Some(taken);
            }
        }
        Ok(())
    }
}

/// The two pictures of one frame of a group. A part without a drawn pixel is left out.
#[derive(Clone)]
struct Parts {
    solid: Option<Picture>,
    added: Option<Picture>,
}

/// One frame of a group: its slot in the ordering table and the layer, palette row and blend
/// bit that name its pictures.
#[derive(Clone, Copy)]
struct FrameArt {
    slot: u32,
    key: (u8, u8, bool),
}

/// A view of a background file, read into what the screen needs.
#[derive(Clone)]
pub struct Backdrop {
    groups: Vec<(Group, Vec<FrameArt>)>,
    pictures: HashMap<(u8, u8, bool), Parts>,
    /// The size of layer 0 in pixels, unscaled.
    layer: (i32, i32),
    /// The depth word of every layer, by which an object names a group.
    depths: Vec<u32>,
}

impl Backdrop {
    pub fn from_view(view: &View) -> Result<Self, String> {
        let drawn = |picture: Picture| {
            picture
                .rgba
                .iter()
                .any(|byte| *byte != 0)
                .then_some(picture)
        };
        let mut pictures = HashMap::new();
        let mut groups = Vec::new();
        for group in &view.groups {
            let mut frames = Vec::new();
            for frame in &group.frames {
                let key = (frame.layer, frame.palette_row, frame.blend());
                let (layer, row) = (usize::from(frame.layer), usize::from(frame.palette_row));
                if let Entry::Vacant(place) = pictures.entry(key) {
                    let part = |pixels| {
                        view.layer_part(layer, row, pixels)
                            .ok_or_else(|| format!("no palette row {row} for layer {layer}"))
                    };
                    let parts = match frame.blend() {
                        true => Parts {
                            solid: drawn(part(Pixels::Solid)?),
                            added: drawn(part(Pixels::Added)?),
                        },
                        false => Parts {
                            solid: drawn(part(Pixels::All)?),
                            added: None,
                        },
                    };
                    place.insert(parts);
                }
                frames.push(FrameArt {
                    slot: view.ordering_slot(view.layers[layer].depth),
                    key,
                });
            }
            groups.push((group.clone(), frames));
        }
        Ok(Self {
            groups,
            pictures,
            layer: view.layer_size(ZOOM_ONE),
            depths: view.layers.iter().map(|layer| layer.depth).collect(),
        })
    }
}

/// One picture of one frame of a group on screen. It is visible while its group shows that
/// frame.
#[derive(Component)]
pub struct GroupPart {
    group: usize,
    frame: usize,
    /// The place on the depth axis.
    depth: f32,
    /// The picture's size in pixels, which a square of one by one is enlarged by.
    scale: Vec2,
    shade: bool,
    /// The layer's size in cells.
    cells: (u16, u16),
    /// The square of one by one the picture is drawn on at a zoom of one.
    square: Handle<Mesh>,
}

fn tile_mesh(cells: (u16, u16), zoom: i32, scroll: IVec2) -> Option<Mesh> {
    let size = Vec2::new(f32::from(cells.0), f32::from(cells.1)) * TILE as f32;
    let last = TILE_LAST_TEXEL as f32;
    let (mut places, mut texels, mut corners) = (Vec::new(), Vec::new(), Vec::new());
    for row in 0..cells.1 {
        for column in 0..cells.0 {
            let [left, right, top, bottom] = background::tile_edges(cells, column, row, zoom);
            let seen = IVec2::new(SCREEN.0, SCREEN.1) / 2 + scroll + IVec2::new(left, top);
            if background::tile_left_out(seen.x, seen.y, zoom) {
                continue;
            }
            let first = places.len() as u32;
            let start = Vec2::new(f32::from(column), f32::from(row)) * TILE as f32;
            for (x, y, across, down) in [
                (left, top, 0.0, 0.0),
                (left, bottom, 0.0, last),
                (right, bottom, last, last),
                (right, top, last, 0.0),
            ] {
                // The screen's y points down.
                places.push([x as f32, -(y as f32), 0.0]);
                texels.push(((start + Vec2::new(across, down)) / size).to_array());
            }
            corners.extend([first, first + 1, first + 2, first, first + 2, first + 3]);
        }
    }
    (!corners.is_empty()).then(|| {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, places)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, texels)
        .with_inserted_indices(Indices::U32(corners))
    })
}

/// The meshes of the layers at the zoom and scroll offset they were made for, by a
/// layer's size in cells.
#[derive(Default)]
pub struct TileMeshes {
    made_for: Option<(i32, IVec2)>,
    meshes: HashMap<(u16, u16), Option<Handle<Mesh>>>,
}

/// The state of the background on screen.
#[derive(Resource)]
pub struct FieldScreen {
    groups: Vec<Group>,
    clocks: Vec<GroupClock>,
    /// The depth word of every layer of the view.
    depths: Vec<u32>,
    /// The scroll offset in screen pixels: positive moves the picture right and down.
    pub scroll: IVec2,
    /// The size of layer 0 at a zoom of one; the scroll routine measures it, at the zoom
    /// in use, against the draw area.
    layer: IVec2,
    zoom: i32,
    area: DrawArea,
    pub cover: [(i32, i32, i32, i32); 2],
    /// Something keeps the scroll offset on a point of the room: the debug keys leave it
    /// alone.
    pub follows: bool,
    /// The colour of the tiles, 128 leaving them as they are, and the one the added
    /// pictures were last given.
    pub tile: u8,
    added_tile: u8,
    /// What the fade's white rectangle adds to every colour of the picture, and its
    /// material.
    pub white: u8,
    whitens: Handle<AddedMaterial>,
    pub held: Option<u8>,
    pub drawn: bool,
}

impl FieldScreen {
    /// Moves the scroll offset, as far as layer 0 allows over the draw area. A layer that
    /// is smaller than the area stays in its middle here; the game's routine would push
    /// it to an edge (`DrawArea::limited`), which is what [`Self::followed`] does.
    pub fn scroll_to(&mut self, wanted: IVec2) {
        let shift = IVec2::from(self.area.shift());
        let area = IVec2::new(self.area.width, self.area.height);
        let room = ((self.layer_now() - area) / 2).max(IVec2::ZERO);
        self.scroll = (wanted + shift).clamp(-room, room) - shift;
    }

    /// The scroll offset behind one call of the game's scroll routine for a point of the
    /// room that is seen at `point`, right of and below the projection centre; without
    /// one the point is behind the camera and the game takes the middle of the screen.
    pub fn followed(&self, point: Option<IVec2>) -> IVec2 {
        let middle = IVec2::new(SCREEN.0, SCREEN.1) / 2;
        let seen = point.map_or(middle, |point| middle + self.scroll + point);
        let layer = self.layer_now();
        let scroll = (self.area).scroll(self.scroll.into(), seen.into(), layer.into());
        scroll.into()
    }

    /// The size of layer 0 at the zoom in use, the fraction dropped.
    fn layer_now(&self) -> IVec2 {
        self.layer * self.zoom / ZOOM_ONE
    }

    pub fn zoom(&self) -> i32 {
        self.zoom
    }

    /// Another zoom; the scroll offset is kept as far as the layer allows at it until the
    /// next call of the scroll routine.
    pub fn set_zoom(&mut self, zoom: i32) {
        self.zoom = zoom;
        self.scroll_to(self.scroll);
    }

    /// Moves the scroll offset for something that it follows from now on.
    pub fn follow(&mut self, scroll: IVec2) {
        self.follows = true;
        self.scroll = scroll;
    }

    pub fn area(&self) -> DrawArea {
        self.area
    }

    /// The animated group an object names by a number, with its frames
    /// (`background::group_named`).
    pub fn group_named(&self, number: i16) -> Option<usize> {
        background::group_named(&self.depths, &self.groups, number)
    }

    /// Whether a group's frame is through (`GroupClock::through`); a group the view
    /// does not have never is.
    pub fn through(&self, group: usize, frame: usize) -> bool {
        (self.clocks.get(group).zip(self.groups.get(group)))
            .is_some_and(|(clock, frames)| clock.through(frames, frame))
    }

    /// Puts a group on a frame it has.
    pub fn hold(&mut self, group: usize, frame: usize) {
        let frames = self.groups.get(group).map_or(0, |group| group.frames.len());
        if let Some(clock) = self.clocks.get_mut(group).filter(|_| frame < frames) {
            clock.hold(frame);
        }
    }

    /// Another draw area; the scroll offset is kept as far as the area allows until the
    /// next call of the scroll routine.
    pub fn set_area(&mut self, area: DrawArea) {
        self.area = area;
        self.scroll_to(self.scroll);
    }
}

#[derive(Resource, Clone)]
pub struct WalkLines {
    camera: ViewCamera,
    lines: Vec<WalkLine>,
}

#[derive(Clone)]
struct WalkLine {
    ends: [[i32; 3]; 2],
    /// The region is the one the view belongs to: the ground this view shows.
    of_view: bool,
    /// The edge is part of the region's outline.
    outline: bool,
}

impl WalkLines {
    /// `mesh` is the room's walk mesh, whose region number is the view number.
    pub fn new(camera: ViewCamera, mesh: &[Region], view: usize) -> Self {
        let mut lines = Vec::new();
        for (number, region) in mesh.iter().enumerate() {
            for triangle in &region.triangles {
                for from in 0..3 {
                    let corner = |index: usize| triangle.corners[index % 3].map(i32::from);
                    lines.push(WalkLine {
                        ends: [corner(from), corner(from + 1)],
                        of_view: number == view,
                        outline: !triangle.inner_edges[from],
                    });
                }
            }
        }
        Self { camera, lines }
    }
}

/// Draws the walk mesh where the game would draw it: the projection centre is the screen
/// centre moved by the scroll offset, so the lines shift with the background.
fn draw_walk_lines(
    walk: Option<Res<WalkLines>>,
    screen: Option<Res<FieldScreen>>,
    mut gizmos: Gizmos,
) {
    let (Some(walk), Some(screen)) = (walk, screen) else {
        return;
    };
    // The screen's y points down.
    let place = |point: [f64; 2]| {
        Vec2::new(
            point[0] as f32 + screen.scroll.x as f32,
            -(point[1] as f32 + screen.scroll.y as f32),
        )
    };
    for line in &walk.lines {
        let Some([from, to]) = walk.camera.project_line(line.ends[0], line.ends[1]) else {
            continue;
        };
        let color = match (line.of_view, line.outline) {
            (true, true) => Color::srgb(1.0, 1.0, 0.0),
            (true, false) => Color::srgb(0.0, 1.0, 0.0),
            (false, true) => Color::srgb(1.0, 0.0, 1.0),
            (false, false) => Color::srgb(0.0, 0.6, 1.0),
        };
        gizmos.line_2d(place(from), place(to), color);
    }
}

/// Marks the camera that draws the room on the console's screen.
#[derive(Component)]
pub struct ScreenCamera;

/// Marks the camera whose picture is the console's screen in the window: the room's own,
/// unless the room is drawn into a kept picture first (`before.rs`).
#[derive(Component)]
pub struct Fitted;

/// The console's screen as a camera sees it, whatever it draws into.
pub fn screen_projection() -> Projection {
    Projection::Orthographic(OrthographicProjection {
        scaling_mode: ScalingMode::Fixed {
            width: SCREEN.0 as f32,
            height: SCREEN.1 as f32,
        },
        ..OrthographicProjection::default_2d()
    })
}

fn image(picture: Picture) -> Image {
    Image::new(
        Extent3d {
            width: picture.width,
            height: picture.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        picture.rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// Puts a view on screen: the camera, the pictures of every frame of every group and the state
/// that steps them.
pub fn spawn(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<AddedMaterial>,
    backdrop: Backdrop,
    scroll: IVec2,
) {
    commands.spawn((
        ScreenCamera,
        Fitted,
        Camera2d,
        // Blending adds the console's colour values, as the console does.
        CompositingSpace::Srgb,
        Msaa::Off,
        screen_projection(),
    ));
    // The four parts outside the draw area, and behind them the two bars of an object.
    for part in 0..DrawArea::WHOLE.outside().len() + COVERS {
        commands.spawn((
            Outside(part),
            Sprite::from_color(Color::BLACK, Vec2::ONE),
            Transform::from_xyz(0.0, 0.0, OUTSIDE_DEPTH),
            Visibility::Hidden,
        ));
    }
    // The fade's white rectangle: one white texel, added over the whole screen as
    // strongly as the fade says.
    let white = images.add(image(Picture {
        width: 1,
        height: 1,
        rgba: vec![255; 4],
    }));
    let whitens = materials.add(AddedMaterial {
        texture: white,
        share: Vec4::ZERO,
        drawn: Drawn::Added,
    });
    commands.spawn((
        White,
        Mesh2d(meshes.add(Rectangle::new(SCREEN.0 as f32, SCREEN.1 as f32))),
        MeshMaterial2d(whitens.clone()),
        Transform::from_xyz(0.0, 0.0, WHITE_DEPTH),
        Visibility::Hidden,
    ));
    let depths = backdrop.depths.clone();
    let (groups, layer) = show(commands, images, meshes, materials, backdrop);
    let mut screen = FieldScreen {
        clocks: vec![GroupClock::default(); groups.len()],
        groups,
        depths,
        scroll: IVec2::ZERO,
        layer,
        zoom: ZOOM_ONE,
        area: DrawArea::WHOLE,
        cover: [(0, 0, 0, 0); 2],
        follows: false,
        tile: TILE_UNCHANGED,
        added_tile: TILE_UNCHANGED,
        white: 0,
        whitens,
        held: None,
        drawn: true,
    };
    screen.scroll_to(scroll);
    commands.insert_resource(screen);
}

/// Makes the pictures of every frame of every group of a view, hidden until `place_groups`
/// shows the ones of the current frames. Gives the groups and the size of layer 0.
fn show(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<AddedMaterial>,
    backdrop: Backdrop,
) -> (Vec<Group>, IVec2) {
    let mut solid = HashMap::new();
    let mut added = HashMap::new();
    for (key, parts) in backdrop.pictures {
        if let Some(picture) = parts.solid {
            let size = UVec2::new(picture.width, picture.height);
            let texture = images.add(image(picture));
            let over = materials.add(AddedMaterial {
                texture: texture.clone(),
                share: Vec4::ONE,
                drawn: Drawn::Over,
            });
            let shade = materials.add(AddedMaterial {
                texture,
                share: Vec4::ZERO,
                drawn: Drawn::Shade,
            });
            solid.insert(key, (over, shade, size));
        }
        if let Some(picture) = parts.added {
            let size = UVec2::new(picture.width, picture.height);
            let texture = images.add(image(picture));
            let material = materials.add(AddedMaterial {
                texture,
                share: Vec4::ONE,
                drawn: Drawn::Added,
            });
            added.insert(key, (material, size));
        }
    }
    let square = meshes.add(Rectangle::new(1.0, 1.0));

    let mut groups = Vec::new();
    for (index, (group, frames)) in backdrop.groups.into_iter().enumerate() {
        for (number, frame) in frames.iter().enumerate() {
            let part = |over: f32, size: UVec2, material: &Handle<AddedMaterial>| {
                (
                    GroupPart {
                        group: index,
                        frame: number,
                        depth: -(frame.slot as f32) * SLOT_DEPTH
                            + (over - index as f32) * GROUP_DEPTH,
                        scale: size.as_vec2(),
                        shade: over == SHADE_OVER,
                        cells: ((size.x / TILE as u32) as u16, (size.y / TILE as u32) as u16),
                        square: square.clone(),
                    },
                    Mesh2d(square.clone()),
                    MeshMaterial2d(material.clone()),
                    NoFrustumCulling,
                    Visibility::Hidden,
                )
            };
            if let Some((over, shade, size)) = solid.get(&frame.key) {
                commands.spawn(part(0.0, *size, over));
                commands.spawn(part(SHADE_OVER, *size, shade));
            }
            if let Some((material, size)) = added.get(&frame.key) {
                commands.spawn(part(0.5, *size, material));
            }
        }
        groups.push(group);
    }

    (groups, IVec2::from(backdrop.layer))
}

/// One view of a room, kept for the game frame in which the player walks into its region.
#[derive(Clone)]
pub struct Kept {
    pub backdrop: Backdrop,
    pub header: ViewHeader,
    pub walk: WalkLines,
}

#[derive(Resource)]
pub struct RoomViews {
    pub views: Vec<Option<Kept>>,
    pub shown: usize,
}

/// Puts another view of the room on screen in place of the one that is there: its pictures
/// with every group at its first frame, its scroll limit and, when the walk mesh is drawn,
/// its lines. The scroll offset is kept as far as the new view allows; whoever follows a
/// point of the room sets it again through the new camera.
pub fn change_view(
    commands: &mut Commands,
    (images, meshes, materials): (
        &mut Assets<Image>,
        &mut Assets<Mesh>,
        &mut Assets<AddedMaterial>,
    ),
    parts: &Query<Entity, With<GroupPart>>,
    screen: &mut FieldScreen,
    walk: Option<&mut WalkLines>,
    view: Kept,
) {
    for part in parts {
        commands.entity(part).despawn();
    }
    screen.depths = view.backdrop.depths.clone();
    let (groups, layer) = show(commands, images, meshes, materials, view.backdrop);
    screen.clocks = vec![GroupClock::default(); groups.len()];
    screen.groups = groups;
    screen.layer = layer;
    // The new pictures come as they are.
    screen.added_tile = TILE_UNCHANGED;
    screen.scroll_to(screen.scroll);
    if let Some(walk) = walk {
        *walk = view.walk;
    }
}

/// One game frame: every group's animation moves on.
pub fn tick_groups(screen: Option<ResMut<FieldScreen>>) {
    let Some(mut screen) = screen else {
        return;
    };
    let screen = &mut *screen;
    if screen.held.is_some() {
        return;
    }
    for (clock, group) in screen.clocks.iter_mut().zip(&screen.groups) {
        clock.tick(group);
    }
}

/// A debug control until the player drives the scrolling: the arrow keys move the picture.
fn scroll_with_keys(keys: Res<ButtonInput<KeyCode>>, screen: Option<ResMut<FieldScreen>>) {
    let Some(mut screen) = screen.filter(|screen| !screen.follows) else {
        return;
    };
    let pressed = |key| i32::from(keys.pressed(key));
    let step = IVec2::new(
        pressed(KeyCode::ArrowRight) - pressed(KeyCode::ArrowLeft),
        pressed(KeyCode::ArrowDown) - pressed(KeyCode::ArrowUp),
    );
    if step != IVec2::ZERO {
        let wanted = screen.scroll + step * KEY_SCROLL;
        screen.scroll_to(wanted);
    }
}

/// Shows the pictures of every group's current frame, moved by the scroll offset. Layers are
/// centred on the screen centre, which is the camera's origin.
pub fn place_groups(
    screen: Option<ResMut<FieldScreen>>,
    mut parts: Query<(&GroupPart, &mut Transform, &mut Visibility, &mut Mesh2d)>,
    mut tiles: Local<TileMeshes>,
    mut materials: ResMut<Assets<AddedMaterial>>,
    mut white: Query<&mut Visibility, WhiteOnly>,
    (mut taken, mut images, mut meshes): Darkening,
    mut dark: Query<(&MeshMaterial2d<TakenMaterial>, &mut Visibility), DarkOnly>,
    mut commands: Commands,
) {
    let Some(mut screen) = screen else {
        return;
    };
    // The black over a held picture, when it takes anything: one black texel's worth of
    // every colour, taken from the whole screen. It is made when a picture is first held.
    let darkens = Vec4::splat(f32::from(screen.held.unwrap_or(0)) / 255.0);
    if screen.held.is_some() && dark.is_empty() {
        let texel = images.add(image(Picture {
            width: 1,
            height: 1,
            rgba: vec![255; 4],
        }));
        commands.spawn((
            Dark,
            Mesh2d(meshes.add(Rectangle::new(SCREEN.0 as f32, SCREEN.1 as f32))),
            MeshMaterial2d(taken.add(TakenMaterial {
                texture: texel,
                share: darkens,
            })),
            Transform::from_xyz(0.0, 0.0, DARK_DEPTH),
        ));
    }
    for (material, mut visibility) in &mut dark {
        if let Some(mut material) = taken.get_mut(&material.0)
            && material.share != darkens
        {
            material.share = darkens;
        }
        visibility.set_if_neq(match screen.held {
            None | Some(0) => Visibility::Hidden,
            Some(_) => Visibility::Inherited,
        });
    }
    // The fade's white rectangle, when it adds anything.
    let whitens = Vec4::splat(f32::from(screen.white) / 255.0);
    if let Some(mut material) = materials.get_mut(&screen.whitens)
        && material.share != whitens
    {
        material.share = whitens;
    }
    for mut visibility in &mut white {
        visibility.set_if_neq(match screen.white {
            0 => Visibility::Hidden,
            _ => Visibility::Inherited,
        });
    }
    // The tiles are drawn with one colour for all: the solid ones get black over them, the
    // added ones are multiplied.
    let kept = f32::from(screen.tile) / f32::from(TILE_UNCHANGED);
    if screen.added_tile != screen.tile {
        screen.added_tile = screen.tile;
        let whitens = screen.whitens.id();
        for (id, material) in materials.iter_mut() {
            if id != whitens {
                material.share = match material.drawn {
                    Drawn::Added => Vec4::splat(kept),
                    Drawn::Over => Vec4::ONE,
                    Drawn::Shade => Vec4::new(0.0, 0.0, 0.0, 1.0 - kept),
                };
            }
        }
    }
    // At a zoom the tiles are placed one by one, and which are left out goes with the
    // scroll offset too.
    let zoomed = screen.zoom != ZOOM_ONE;
    let made_for = zoomed.then_some((screen.zoom, screen.scroll));
    if tiles.made_for != made_for {
        tiles.made_for = made_for;
        tiles.meshes.clear();
    }
    for (part, mut transform, mut visibility, mut mesh) in &mut parts {
        let dark = screen.tile != TILE_UNCHANGED;
        let frame = screen.clocks[part.group].frame == part.frame;
        let shown = screen.drawn && frame && (dark || !part.shade);
        // The screen's y points down. A layer is centred on the screen's middle and moved
        // by the scroll offset: at a zoom of one its picture as it is, at another the
        // quads of its cells, which bring their places with them (`tile_mesh`).
        let (wanted, scale) = match zoomed && shown {
            false => (Some(part.square.clone()), part.scale),
            true => {
                let (zoom, scroll) = (screen.zoom, screen.scroll);
                let made = (tiles.meshes.entry(part.cells))
                    .or_insert_with(|| tile_mesh(part.cells, zoom, scroll).map(|m| meshes.add(m)));
                (made.clone(), Vec2::ONE)
            }
        };
        visibility.set_if_neq(match shown && wanted.is_some() {
            true => Visibility::Inherited,
            false => Visibility::Hidden,
        });
        if let Some(wanted) = wanted
            && mesh.0 != wanted
        {
            mesh.0 = wanted;
        }
        let place = Vec3::new(screen.scroll.x as f32, -screen.scroll.y as f32, part.depth);
        let placed = Transform::from_translation(place).with_scale(scale.extend(1.0));
        transform.set_if_neq(placed);
    }
}

/// The fade's white rectangle over the whole screen.
#[derive(Component)]
pub struct White;

/// The black that is taken from a held picture, over the whole screen.
#[derive(Component)]
pub struct Dark;

type WhiteOnly = (With<White>, Without<GroupPart>, Without<Dark>);
type DarkOnly = (With<Dark>, Without<GroupPart>);
type Darkening<'w> = (
    ResMut<'w, Assets<TakenMaterial>>,
    ResMut<'w, Assets<Image>>,
    ResMut<'w, Assets<Mesh>>,
);

/// One of the four parts of the screen outside the draw area (`DrawArea::outside`), or,
/// behind those, one of the [`COVERS`] bars of an object (`FieldScreen::cover`).
#[derive(Component)]
pub struct Outside(usize);

/// The bars an object of a room can lay over the picture.
const COVERS: usize = 2;

pub fn cut_to_the_area(
    screen: Option<Res<FieldScreen>>,
    mut parts: Query<(&Outside, &mut Sprite, &mut Transform, &mut Visibility)>,
) {
    let Some(screen) = screen else { return };
    let outside = screen.area.outside();
    for (part, mut sprite, mut transform, mut visibility) in &mut parts {
        let (x, y, width, height) = match part.0.checked_sub(outside.len()) {
            None => outside[part.0],
            Some(bar) => screen.cover[bar],
        };
        let shown = match width > 0 && height > 0 {
            true => Visibility::Inherited,
            false => Visibility::Hidden,
        };
        visibility.set_if_neq(shown);
        let size = Some(Vec2::new(width as f32, height as f32));
        if sprite.custom_size != size {
            sprite.custom_size = size;
        }
        // The screen's y points down, and its middle is the camera's origin.
        let place = Vec3::new(
            (2 * x + width - SCREEN.0) as f32 / 2.0,
            (SCREEN.1 - 2 * y - height) as f32 / 2.0,
            OUTSIDE_DEPTH,
        );
        transform.set_if_neq(Transform::from_translation(place));
    }
}

/// Keeps the console's screen as large as fits the window, at its own shape.
fn fit_screen(window: Single<&Window>, mut camera: Single<&mut Camera, With<Fitted>>) {
    let (width, height) = (window.physical_width(), window.physical_height());
    let (across, down) = (SCREEN.0 as u32, SCREEN.1 as u32);
    if width == 0 || height == 0 {
        return;
    }
    let size = match width * down > height * across {
        true => UVec2::new(height * across / down, height),
        false => UVec2::new(width, width * down / across),
    };
    let position = (UVec2::new(width, height) - size) / 2;
    let fits = camera
        .viewport
        .as_ref()
        .is_some_and(|v| v.physical_position == position && v.physical_size == size);
    if !fits {
        camera.viewport = Some(Viewport {
            physical_position: position,
            physical_size: size,
            ..default()
        });
    }
}
