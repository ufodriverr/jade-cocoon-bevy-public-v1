
use std::fmt;
use std::str::FromStr;
use std::time::{Duration, Instant};

use bevy::asset::uuid_handle;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{CameraUpdateSystems, CompositingSpace, RenderTarget, ScalingMode, Viewport};
use bevy::ecs::system::SystemParam;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, TextureFormat};
use bevy::render::renderer::RenderAdapter;
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dPlugin};
use bevy::window::{
    Monitor, MonitorSelection, OnMonitor, PresentMode, PrimaryMonitor, PrimaryWindow,
    VideoModeSelection, WindowMode, WindowMoved, WindowPosition, WindowResized, WindowResolution,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::field::Fitted;
use crate::prefs::{self, Prefs};

/// The console's screen: the shape of every screen of the game.
pub const SCREEN: UVec2 = UVec2::new(320, 240);
/// The units every screen but a room's sees: the window of old (`crate::WINDOW_SIZE`).
const UNITS: Vec2 = Vec2::new(crate::WINDOW_SIZE.0 as f32, crate::WINDOW_SIZE.1 as f32);
/// The small picture the blurred sides are made from.
const SMALL: UVec2 = UVec2::new(80, 60);
/// How bright the blurred sides are.
const SIDES_BRIGHTNESS: f32 = 0.45;
/// The layers and the order of this module's cameras: behind every screen's.
const SHRINKING: usize = 29;
const PRESENTING: usize = 30;
/// The layer of what is drawn over the whole window, on top of the picture and outside
/// it: the camera that lays the picture into the window sees it, in the window's
/// physical pixels, the middle at the origin and y up ([`Frame::window`]). What is drawn
/// there is marked [`Presents`], so no screen takes it away (the marker of `pace.rs`).
pub const OVER_THE_WINDOW: usize = PRESENTING;
const SHRINK_ORDER: isize = 999;
const PRESENT_ORDER: isize = 1000;
/// Seconds the window rests before its new size or place is kept.
const SETTLE: f32 = 0.5;
/// Seconds behind a change of the mode in which the window's size is not kept: the
/// window is still on its way.
const AFTER_A_CHANGE: f32 = 1.0;

const SHRINK_SHADER: Handle<Shader> = uuid_handle!("3b7e5d0c-1f2a-4c8e-9d6b-5a4f3e2d1c0b");
const SIDES_SHADER: Handle<Shader> = uuid_handle!("8c1d2e3f-4a5b-4c6d-8e7f-9a0b1c2d3e4f");
const PRESENT_SHADER: Handle<Shader> = uuid_handle!("5e2a7c91-3d4b-4f60-8a1e-c7b9d2f0a364");

/// The resolutions to offer, 16:9 first (the user, 2026-10-15), then 16:10, 21:9 and 4:3.
pub const RESOLUTIONS: [Size; 15] = [
    Size::new(1280, 720),
    Size::new(1600, 900),
    Size::new(1920, 1080),
    Size::new(2560, 1440),
    Size::new(3840, 2160),
    Size::new(1680, 1050),
    Size::new(1920, 1200),
    Size::new(2560, 1600),
    Size::new(2560, 1080),
    Size::new(3440, 1440),
    Size::new(960, 720),
    Size::new(1440, 1080),
    Size::new(1600, 1200),
    Size::new(1024, 768),
    Size::new(1280, 960),
];

/// A size in pixels, written `1920x1080`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    fn uvec2(self) -> UVec2 {
        UVec2::new(self.width, self.height)
    }
}

impl fmt::Display for Size {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

impl FromStr for Size {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        let wrong = || format!("{text:?} is not a size like 1920x1080");
        let (width, height) = (text.trim().split_once(['x', 'X'])).ok_or_else(wrong)?;
        let width: u32 = width.trim().parse().map_err(|_| wrong())?;
        let height: u32 = height.trim().parse().map_err(|_| wrong())?;
        match width > 0 && height > 0 {
            true => Ok(Self::new(width, height)),
            false => Err(wrong()),
        }
    }
}

impl Serialize for Size {
    fn serialize<S: Serializer>(&self, to: S) -> Result<S::Ok, S::Error> {
        to.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Size {
    fn deserialize<D: Deserializer<'de>>(from: D) -> Result<Self, D::Error> {
        String::deserialize(from)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// `mode` of `[display]`: how the window takes the monitor.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// The whole monitor, at its own resolution, without a border.
    #[default]
    Borderless,
    /// The whole monitor in the video mode of `resolution` (exclusive).
    Fullscreen,
    /// A window with a border.
    Windowed,
}

/// `content` of `[display]`: what a 4:3 screen does with a window of another shape.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Content {
    /// In the middle, the sides a blurred copy of the picture.
    #[default]
    Fill,
    /// In the middle, black beside it.
    Bars,
    /// Over the whole window, out of its shape.
    Stretch,
    /// Over the whole window, cut at the top and the bottom: the movies and the title
    /// only, the other screens fill.
    Zoom,
}

impl FromStr for Content {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        match text {
            "fill" => Ok(Self::Fill),
            "bars" => Ok(Self::Bars),
            "stretch" => Ok(Self::Stretch),
            "zoom" => Ok(Self::Zoom),
            other => Err(format!("{other:?} is not fill, bars, stretch or zoom")),
        }
    }
}

/// `render` of `[display]`: how many pixels the screens are drawn with.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Render {
    /// As many as the picture has in the window: the 3D's edges sharp.
    #[default]
    Sharp,
    /// The console's 320 by 240, enlarged.
    Authentic,
}

impl FromStr for Render {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        match text {
            "sharp" => Ok(Self::Sharp),
            "authentic" => Ok(Self::Authentic),
            other => Err(format!("{other:?} is not sharp or authentic")),
        }
    }
}

/// `crt` of `[display]`: what lies over the picture in the window.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Filter {
    /// Nothing.
    #[default]
    Off,
    /// The console's 240 lines, a darker seam between each two.
    Scanlines,
    /// The lines, a mask of the three colours, a slight glow and darker corners.
    Crt,
}

impl Filter {
    /// The number `present.wgsl` knows it by.
    fn number(self) -> f32 {
        match self {
            Self::Off => 0.0,
            Self::Scanlines => 1.0,
            Self::Crt => 2.0,
        }
    }
}

impl FromStr for Filter {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        match text {
            "off" => Ok(Self::Off),
            "scanlines" => Ok(Self::Scanlines),
            "crt" => Ok(Self::Crt),
            other => Err(format!("{other:?} is not off, scanlines or crt")),
        }
    }
}

/// The samples `msaa` of `[display]` asks for: the most of 2, 4 and 8 it reaches, none
/// below 2.
pub fn asked_samples(msaa: u8) -> Msaa {
    match msaa {
        0 | 1 => Msaa::Off,
        2 | 3 => Msaa::Sample2,
        4..=7 => Msaa::Sample4,
        _ => Msaa::Sample8,
    }
}

/// The samples to draw with: those asked for, or fewer where the graphics card has not
/// as many for the picture and its depth (4 every card has). Without a card known, 4 at
/// most.
pub fn samples(asked: Msaa, adapter: Option<&RenderAdapter>) -> Msaa {
    let offered = |msaa: Msaa| match adapter {
        Some(adapter) => [TextureFormat::Rgba8UnormSrgb, TextureFormat::Depth32Float]
            .iter()
            .all(|format| {
                (adapter.get_texture_format_features(*format).flags)
                    .sample_count_supported(msaa.samples())
            }),
        None => msaa.samples() <= 4,
    };
    [Msaa::Sample8, Msaa::Sample4, Msaa::Sample2]
        .into_iter()
        .filter(|msaa| msaa.samples() <= asked.samples())
        .find(|msaa| offered(*msaa))
        .unwrap_or(Msaa::Off)
}

/// The way of the picture this frame: the settings and what is on screen.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Shaping {
    pub content: Content,
    pub integer: bool,
    /// A movie or the title is on screen alone, which `zoom` may cut.
    pub zooms: bool,
    /// The picture takes the window's shape ([`Wide`]).
    pub wide: bool,
    /// The picture has the console's own pixels (`render = "authentic"`).
    pub authentic: bool,
}

/// Where the picture lies in the window, all in the window's physical pixels.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Layout {
    pub window: UVec2,
    /// The size of the picture in the window.
    pub picture: UVec2,
    /// The picture's top left corner in the window; left of or above it when zoomed.
    pub corner: IVec2,
    /// The 4:3 part of the picture the screens of the console's shape draw in: the whole
    /// picture, unless it is wide.
    pub square: URect,
    /// The window round the picture shows the blurred copy.
    pub sides: bool,
    pub wide: bool,
    /// The picture's own pixels, which its cameras draw: as many as it has in the
    /// window (the window shows it one to one), or the console's (`authentic`).
    pub pixels: UVec2,
    /// The 4:3 part in the picture's own pixels.
    pub pixel_square: URect,
}

impl Layout {
    pub fn of(window: UVec2, shaping: Shaping) -> Self {
        let window = window.max(UVec2::ONE);
        let whole = |picture: UVec2| URect::from_corners(UVec2::ZERO, picture);
        let content = match shaping.content {
            Content::Zoom if !shaping.zooms => Content::Fill,
            content => content,
        };
        let layout = if shaping.wide || content == Content::Stretch {
            let square = match shaping.wide {
                true => {
                    let square = fit(window);
                    let corner = (window - square) / 2;
                    URect::from_corners(corner, corner + square)
                }
                false => whole(window),
            };
            Self {
                window,
                picture: window,
                corner: IVec2::ZERO,
                square,
                sides: false,
                wide: shaping.wide,
                pixels: window,
                pixel_square: square,
            }
        } else {
            let whole_factor = (window.x / SCREEN.x).min(window.y / SCREEN.y);
            let picture = match content {
                Content::Zoom => cover(window),
                _ if shaping.integer && whole_factor > 0 => SCREEN * whole_factor,
                _ => fit(window),
            };
            let leaves_room = picture.x < window.x || picture.y < window.y;
            Self {
                window,
                picture,
                corner: (window.as_ivec2() - picture.as_ivec2()) / 2,
                square: whole(picture),
                sides: content == Content::Fill && leaves_room,
                wide: false,
                pixels: picture,
                pixel_square: whole(picture),
            }
        };
        match shaping.authentic {
            true => layout.in_console_pixels(),
            false => layout,
        }
    }

    /// The same picture drawn in the console's own pixels: 320 by 240, or 240 lines as
    /// wide as a wide picture's shape, the 4:3 part in the middle with as many columns
    /// on either side. The 4:3 part in the window is where those pixels lie.
    fn in_console_pixels(mut self) -> Self {
        let (pixels, square) = match self.wide {
            true => {
                // At most four times the console's width: a window of a few lines would
                // otherwise ask for a picture wider than a graphics card makes.
                let columns = (SCREEN.y as f32 * self.picture.x as f32 / self.picture.y as f32)
                    .min(4.0 * SCREEN.x as f32);
                let side = UVec2::new(((columns - SCREEN.x as f32) / 2.0).round() as u32, 0);
                let pixels = UVec2::new(SCREEN.x + 2 * side.x, SCREEN.y);
                (pixels, URect::from_corners(side, side + SCREEN))
            }
            false => (SCREEN, URect::from_corners(UVec2::ZERO, SCREEN)),
        };
        let scale = self.picture.as_vec2() / pixels.as_vec2();
        let in_window = |corner: UVec2| (corner.as_vec2() * scale).round().as_uvec2();
        self.square = URect::from_corners(in_window(square.min), in_window(square.max));
        self.pixels = pixels;
        self.pixel_square = square;
        self
    }

    /// The picture's own pixels in one pixel of the window: 1, unless `authentic`.
    fn pixels_per_window_pixel(&self) -> Vec2 {
        self.pixels.as_vec2() / self.picture.as_vec2()
    }

    /// The pixel of the picture under a physical pixel of the window, none off it,
    /// counted in the picture's own pixels as its cameras count.
    pub fn to_picture(self, at: Vec2) -> Option<Vec2> {
        let at = at - self.corner.as_vec2();
        let inside = at.cmpge(Vec2::ZERO).all() && at.cmplt(self.picture.as_vec2()).all();
        inside.then(|| at * self.pixels_per_window_pixel())
    }

    /// The physical pixel of the window where a place of the console's screen lies: in
    /// the 4:3 part of the picture, and beyond its sides in a wide one. For the mouse of
    /// a check run (`pointer.rs`).
    pub fn on_window(self, at: Vec2) -> Vec2 {
        let square = self.pixel_square.as_rect();
        let pixel = square.min + at / SCREEN.as_vec2() * square.size();
        self.corner.as_vec2() + pixel / self.pixels_per_window_pixel()
    }

    /// The units the camera of a room or a fight sees: the console's screen, as wide as
    /// the picture's shape when the picture is wide.
    pub fn screen(&self) -> Vec2 {
        let high = SCREEN.y as f32;
        match self.wide {
            true => Vec2::new(high * self.pixels.x as f32 / self.pixels.y as f32, high),
            false => SCREEN.as_vec2(),
        }
    }

    /// Where a camera of the picture draws, in the picture's own pixels: the 4:3 part
    /// for a screen of the console's shape, the whole picture for the room's camera when
    /// it is wide. None is the whole.
    fn viewport(&self, fitted: bool) -> Option<Viewport> {
        let whole = URect::from_corners(UVec2::ZERO, self.pixels);
        let part = match fitted && self.wide {
            true => whole,
            false => self.pixel_square,
        };
        (part != whole).then(|| Viewport {
            physical_position: part.min,
            physical_size: part.size(),
            ..default()
        })
    }
}

/// The largest 4:3 that fits.
fn fit(window: UVec2) -> UVec2 {
    let size = match window.x * 3 >= window.y * 4 {
        true => UVec2::new((window.y * 4 + 1) / 3, window.y),
        false => UVec2::new(window.x, (window.x * 3 + 2) / 4),
    };
    size.min(window).max(UVec2::ONE)
}

/// The smallest 4:3 that covers.
fn cover(window: UVec2) -> UVec2 {
    match window.x * 3 >= window.y * 4 {
        true => UVec2::new(window.x, (window.x * 3 + 2) / 4),
        false => UVec2::new((window.y * 4 + 1) / 3, window.y),
    }
    .max(window)
}

/// Set to true, the picture takes the window's shape and the camera of a room or a fight
/// sees as far as it reaches: `widescreen.rs` sets it while a fight is on screen (G5).
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wide(pub bool);

/// The picture every screen draws into, and how it lies in the window.
#[derive(Resource)]
pub struct Frame {
    layout: Option<Layout>,
    /// Physical pixels of the window to a logical one.
    scale: f32,
    picture: Handle<Image>,
    /// The picture before the last change of its size, whose cameras come to the new one.
    was: Option<Handle<Image>>,
    small: Handle<Image>,
    shrink: Handle<ShrinkMaterial>,
    sides: Handle<SidesMaterial>,
    present: Handle<PresentMaterial>,
    square: Handle<Mesh>,
    small_square: Handle<Mesh>,
    /// The samples every camera of the picture draws with (`msaa`, `sharp` only).
    msaa: Msaa,
    /// What lies over the picture in the window.
    crt: Filter,
    /// The 3D is dithered as the console's (`dither`, `authentic` only).
    dither: bool,
}

impl Frame {
    /// The picture the cameras of the screens draw into.
    pub fn picture(&self) -> &Handle<Image> {
        &self.picture
    }

    /// The size of the 4:3 part of the picture in its own pixels, once the picture is
    /// laid into the window: a picture drawn in place of it (`before.rs`) has as many,
    /// so it looks the same.
    pub fn square_pixels(&self) -> Option<UVec2> {
        self.layout.map(|layout| layout.pixel_square.size())
    }

    /// Whether the 3D is dithered as the console's (`thing.wgsl`, `model.wgsl`).
    pub fn dithers(&self) -> bool {
        self.dither
    }

    /// The pixel of the picture under a place of the window in logical pixels (where
    /// the mouse is), none off the picture.
    pub fn to_picture(&self, at: Vec2) -> Option<Vec2> {
        self.layout?.to_picture(at * self.scale)
    }

    /// The place of the window in logical pixels where a place of the console's screen
    /// lies ([`Layout::on_window`]).
    pub fn on_window(&self, at: Vec2) -> Option<Vec2> {
        Some(self.layout?.on_window(at) / self.scale)
    }

    /// The size of the window in physical pixels, once the picture is laid into it: the
    /// units of [`OVER_THE_WINDOW`].
    pub fn window(&self) -> Option<Vec2> {
        self.layout.map(|layout| layout.window.as_vec2())
    }

    /// The units the camera of a room or a fight sees ([`Layout::screen`]).
    pub fn screen(&self) -> Vec2 {
        self.layout
            .map_or(SCREEN.as_vec2(), |layout| layout.screen())
    }
}

/// The cameras of this module and what they draw: never taken away with a screen.
#[derive(Component)]
pub struct Presents;

/// The camera that draws the picture into the window.
#[derive(Component)]
struct PresentCamera;

/// The camera that shrinks the picture for the blurred sides.
#[derive(Component)]
struct ShrinkCamera;

/// The picture in the window, one to one.
#[derive(Component)]
struct Shown;

/// The picture in the window through [`PresentMaterial`]: enlarged (`authentic`) or
/// under a `crt` filter. In place of [`Shown`] then.
#[derive(Component)]
struct Filtered;

/// The blurred copy behind it.
#[derive(Component)]
struct Sides;

/// Shrinks the picture to [`SMALL`], each texel the mean of its part of the picture.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct ShrinkMaterial {
    #[texture(0)]
    #[sampler(1)]
    picture: Handle<Image>,
    /// xy: one texel of the small picture in the picture's coordinates.
    #[uniform(2)]
    step: Vec4,
}

impl Material2d for ShrinkMaterial {
    fn fragment_shader() -> ShaderRef {
        SHRINK_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Opaque
    }
}

/// Draws the small picture over the whole window, blurred and darkened.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct SidesMaterial {
    #[texture(0)]
    #[sampler(1)]
    small: Handle<Image>,
    /// xy: one texel of the small picture, z: the brightness.
    #[uniform(2)]
    look: Vec4,
}

impl Material2d for SidesMaterial {
    fn fragment_shader() -> ShaderRef {
        SIDES_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Opaque
    }
}

/// Lays the picture into the window when it is not one to one: its blocks enlarged
/// alike, and the `crt` filter over it (`present.wgsl`).
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct PresentMaterial {
    #[texture(0)]
    #[sampler(1)]
    picture: Handle<Image>,
    /// xy: the picture's own pixels, zw: its size in the window.
    #[uniform(2)]
    size: Vec4,
    /// x: the filter ([`Filter::number`]), y: the console's lines down the picture.
    #[uniform(3)]
    look: Vec4,
}

impl Material2d for PresentMaterial {
    fn fragment_shader() -> ShaderRef {
        PRESENT_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Opaque
    }
}

/// A material of the 3D that the console's dither can reach: `dither` of `[display]`
/// ([`Frame::dithers`]), the fourth share of `thing.wgsl` and `model.wgsl`.
pub trait Dithers: Asset {
    /// 1 dithers, 0 not.
    fn dither(&self) -> f32;
    fn set_dither(&mut self, dither: f32);
}

/// Gives every material of the 3D the dither the picture has now, a new one too. Only a
/// material that has it wrong is touched.
pub fn keep_dither<M: Dithers>(frame: Option<Res<Frame>>, mut materials: ResMut<Assets<M>>) {
    let wanted = match frame.is_some_and(|frame| frame.dithers()) {
        true => 1.0,
        false => 0.0,
    };
    let wrong: Vec<AssetId<M>> = (materials.iter())
        .filter(|(_, material)| material.dither() != wanted)
        .map(|(id, _)| id)
        .collect();
    for id in wrong {
        if let Some(mut material) = materials.get_mut(id) {
            material.set_dither(wanted);
        }
    }
}

/// [`fit_picture`] and [`fit_cameras`], in `PostUpdate`: whatever sets [`Wide`] comes
/// before.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Fitting;

/// The picture and the window: [`fit_picture`] and [`fit_cameras`] in every run, and
/// in a run whose window is on screen (`manages_the_window`) the settings of the window,
/// the keys F11 and Alt+Enter and the frame cap.
pub struct DisplayPlugin {
    pub manages_the_window: bool,
}

impl Plugin for DisplayPlugin {
    fn build(&self, app: &mut App) {
        let mut shaders = app.world_mut().resource_mut::<Assets<Shader>>();
        let shrink = Shader::from_wgsl(include_str!("shrink.wgsl"), "shrink.wgsl");
        let sides = Shader::from_wgsl(include_str!("sides.wgsl"), "sides.wgsl");
        let present = Shader::from_wgsl(include_str!("present.wgsl"), "present.wgsl");
        (shaders.insert(&SHRINK_SHADER, shrink)).expect("a fixed handle takes an asset");
        (shaders.insert(&SIDES_SHADER, sides)).expect("a fixed handle takes an asset");
        (shaders.insert(&PRESENT_SHADER, present)).expect("a fixed handle takes an asset");
        app.add_plugins((
            Material2dPlugin::<ShrinkMaterial>::default(),
            Material2dPlugin::<SidesMaterial>::default(),
            Material2dPlugin::<PresentMaterial>::default(),
        ))
        .init_resource::<Wide>()
        .add_systems(Startup, start)
        .add_systems(
            PostUpdate,
            (fit_picture, fit_cameras)
                .chain()
                .in_set(Fitting)
                .before(CameraUpdateSystems),
        );
        if self.manages_the_window {
            let display = (app.world().get_resource::<Prefs>())
                .map_or_else(prefs::Display::default, |prefs| prefs.display.clone());
            app.insert_resource(Applied::at_start(&display))
                .add_systems(Update, (toggle_on_keys, apply, remember).chain())
                .add_systems(Last, cap_frames);
        }
    }
}

/// A picture the cameras draw into, `size` pixels, read back with a linear sampler.
fn target(size: UVec2) -> Image {
    let mut image = Image::new_target_texture(size.x, size.y, TextureFormat::Rgba8UnormSrgb, None);
    image.sampler = ImageSampler::linear();
    image
}

fn start(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut shrinks: ResMut<Assets<ShrinkMaterial>>,
    mut sides: ResMut<Assets<SidesMaterial>>,
    mut presents: ResMut<Assets<PresentMaterial>>,
) {
    let picture = images.add(target(UNITS.as_uvec2()));
    let small = images.add(target(SMALL));
    let texel = Vec2::ONE / SMALL.as_vec2();
    let frame = Frame {
        layout: None,
        scale: 1.0,
        shrink: shrinks.add(ShrinkMaterial {
            picture: picture.clone(),
            step: texel.extend(0.0).extend(0.0),
        }),
        sides: sides.add(SidesMaterial {
            small: small.clone(),
            look: texel.extend(SIDES_BRIGHTNESS).extend(0.0),
        }),
        present: presents.add(PresentMaterial {
            picture: picture.clone(),
            size: UNITS.extend(UNITS.x).extend(UNITS.y),
            look: Vec4::new(Filter::Off.number(), SCREEN.y as f32, 0.0, 0.0),
        }),
        square: meshes.add(Rectangle::new(1.0, 1.0)),
        small_square: meshes.add(Rectangle::new(SMALL.x as f32, SMALL.y as f32)),
        picture,
        was: None,
        small,
        msaa: Msaa::Off,
        crt: Filter::Off,
        dither: false,
    };
    spawn_parts(&mut commands, &frame);
    commands.insert_resource(frame);
}

/// The two cameras of this module and what they draw.
fn spawn_parts(commands: &mut Commands, frame: &Frame) {
    let fixed = |size: Vec2| {
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::Fixed {
                width: size.x,
                height: size.y,
            },
            ..OrthographicProjection::default_2d()
        })
    };
    commands.spawn((
        Presents,
        ShrinkCamera,
        Camera2d,
        Camera {
            order: SHRINK_ORDER,
            is_active: false,
            ..default()
        },
        RenderTarget::Image(frame.small.clone().into()),
        // The shaders give the stored colour values, as the room's camera composes.
        CompositingSpace::Srgb,
        Msaa::Off,
        fixed(SMALL.as_vec2()),
        RenderLayers::layer(SHRINKING),
    ));
    commands.spawn((
        Presents,
        Mesh2d(frame.small_square.clone()),
        MeshMaterial2d(frame.shrink.clone()),
        RenderLayers::layer(SHRINKING),
    ));
    commands.spawn((
        Presents,
        PresentCamera,
        Camera2d,
        Camera {
            order: PRESENT_ORDER,
            clear_color: ClearColorConfig::Custom(Color::BLACK),
            ..default()
        },
        CompositingSpace::Srgb,
        Msaa::Off,
        fixed(UNITS),
        RenderLayers::layer(PRESENTING),
    ));
    commands.spawn((
        Presents,
        Sides,
        Mesh2d(frame.square.clone()),
        MeshMaterial2d(frame.sides.clone()),
        Visibility::Hidden,
        RenderLayers::layer(PRESENTING),
    ));
    commands.spawn((
        Presents,
        Shown,
        Sprite::from_image(frame.picture.clone()),
        Transform::from_xyz(0.0, 0.0, 1.0),
        RenderLayers::layer(PRESENTING),
    ));
    commands.spawn((
        Presents,
        Filtered,
        Mesh2d(frame.square.clone()),
        MeshMaterial2d(frame.present.clone()),
        Transform::from_xyz(0.0, 0.0, 1.0),
        Visibility::Hidden,
        RenderLayers::layer(PRESENTING),
    ));
}

/// Whether a movie or the title is on screen alone, which `zoom` may cut.
#[derive(SystemParam)]
pub struct Zooms<'w> {
    playing: Option<Res<'w, crate::movie_view::Playing>>,
    still: Option<Res<'w, crate::movie_view::ShowMovie>>,
    title: Option<Res<'w, crate::title::Title>>,
    card: Option<Res<'w, crate::card_view::CardView>>,
}

impl Zooms<'_> {
    fn now(&self) -> bool {
        let movie = self.playing.as_ref().is_some_and(|movie| movie.on_screen())
            || self.still.as_ref().is_some_and(|movie| movie.still());
        let title = self.title.as_ref().is_some_and(|title| title.zooms());
        self.card.is_none() && (movie || title)
    }
}

type PresentParts<'w, 's> = (
    Query<'w, 's, (&'static mut Camera, &'static mut Projection), With<PresentCamera>>,
    Query<'w, 's, &'static mut Camera, (With<ShrinkCamera>, Without<PresentCamera>)>,
    Query<
        'w,
        's,
        (
            &'static mut Sprite,
            &'static mut Transform,
            &'static mut Visibility,
        ),
        With<Shown>,
    >,
    Query<'w, 's, (&'static mut Transform, &'static mut Visibility), (With<Sides>, Without<Shown>)>,
    Query<
        'w,
        's,
        (&'static mut Transform, &'static mut Visibility),
        (With<Filtered>, Without<Shown>, Without<Sides>),
    >,
);

/// The materials of the picture's way into the window.
type PresentMaterials<'w> = (
    ResMut<'w, Assets<ShrinkMaterial>>,
    ResMut<'w, Assets<PresentMaterial>>,
);

/// Gives the picture the shape and the size the window and the settings ask for, and
/// lays it into the window.
#[allow(clippy::too_many_arguments)]
fn fit_picture(
    windows: Query<&Window, With<PrimaryWindow>>,
    prefs: Option<Res<Prefs>>,
    adapter: Option<Res<RenderAdapter>>,
    wide: Res<Wide>,
    zooms: Zooms,
    frame: Option<ResMut<Frame>>,
    mut images: ResMut<Assets<Image>>,
    (mut shrinks, mut presents): PresentMaterials,
    (mut present, mut shrink, mut shown, mut sides, mut filtered): PresentParts,
    parts: Query<Entity, With<Presents>>,
    mut commands: Commands,
) {
    let (Some(mut frame), Ok(window)) = (frame, windows.single()) else {
        return;
    };
    // A screen that took everything away took these too: they come back.
    if present.is_empty() {
        warn!("the picture's cameras were gone; they are put back");
        for part in &parts {
            commands.entity(part).despawn();
        }
        spawn_parts(&mut commands, &frame);
        frame.layout = None;
        return;
    }
    let size = window.physical_size();
    if size.x == 0 || size.y == 0 {
        // A window taken down to the task bar keeps its picture.
        return;
    }
    frame.scale = window.scale_factor();
    let display = prefs.map(|prefs| prefs.display.clone()).unwrap_or_default();
    let authentic = display.render == Render::Authentic;
    // The console's pixels have no edges to smooth.
    let msaa = match authentic {
        true => Msaa::Off,
        false => samples(asked_samples(display.msaa), adapter.as_deref()),
    };
    if frame.msaa != msaa {
        info!("the picture: {} samples a pixel", msaa.samples());
        frame.msaa = msaa;
    }
    let dither = authentic && display.dither;
    if frame.dither != dither {
        frame.dither = dither;
    }
    let shaping = Shaping {
        content: display.content,
        integer: display.integer_scaling,
        zooms: zooms.now(),
        wide: wide.0,
        authentic,
    };
    let layout = Layout::of(size, shaping);
    let crt = display.crt;
    if frame.layout == Some(layout) && frame.crt == crt {
        return;
    }
    if frame.layout.is_none_or(|was| was.pixels != layout.pixels) {
        let new = images.add(target(layout.pixels));
        frame.was = Some(std::mem::replace(&mut frame.picture, new));
        if let Some(mut material) = shrinks.get_mut(&frame.shrink) {
            material.picture = frame.picture.clone();
        }
    }
    if frame.layout.is_none_or(|was| {
        was.window != layout.window || was.sides != layout.sides || was.pixels != layout.pixels
    }) || frame.crt != crt
    {
        info!(
            "the picture: {}x{} drawn, {}x{} in a window of {}x{}, at {:?}, crt {:?}",
            layout.pixels.x,
            layout.pixels.y,
            layout.picture.x,
            layout.picture.y,
            layout.window.x,
            layout.window.y,
            shaping,
            crt
        );
    }
    frame.layout = Some(layout);
    frame.crt = crt;
    // One to one the picture is shown as it is; enlarged or filtered it goes through
    // the material.
    let through_the_material = layout.pixels != layout.picture || crt != Filter::Off;
    if let Some(mut material) = presents.get_mut(&frame.present) {
        material.picture = frame.picture.clone();
        let (pixels, picture) = (layout.pixels.as_vec2(), layout.picture.as_vec2());
        material.size = Vec4::new(pixels.x, pixels.y, picture.x, picture.y);
        material.look = Vec4::new(crt.number(), SCREEN.y as f32, 0.0, 0.0);
    }
    let shows = |on: bool| match on {
        true => Visibility::Inherited,
        false => Visibility::Hidden,
    };
    let window = layout.window.as_vec2();
    for (_, mut projection) in &mut present {
        *projection = Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::Fixed {
                width: window.x,
                height: window.y,
            },
            ..OrthographicProjection::default_2d()
        });
    }
    for mut camera in &mut shrink {
        camera.is_active = layout.sides;
    }
    let picture = layout.picture.as_vec2();
    // The middle of the window is the camera's origin, and its y points up.
    let middle = layout.corner.as_vec2() + picture / 2.0 - window / 2.0;
    for (mut sprite, mut transform, mut visibility) in &mut shown {
        sprite.image = frame.picture.clone();
        sprite.custom_size = Some(picture);
        transform.translation = Vec3::new(middle.x, -middle.y, 1.0);
        *visibility = shows(!through_the_material);
    }
    for (mut transform, mut visibility) in &mut filtered {
        *transform = Transform::from_xyz(middle.x, -middle.y, 1.0).with_scale(picture.extend(1.0));
        *visibility = shows(through_the_material);
    }
    for (mut transform, mut visibility) in &mut sides {
        *transform = Transform::from_scale(window.extend(1.0));
        *visibility = shows(layout.sides);
    }
}

type ScreenCameras<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut Camera,
        &'static mut RenderTarget,
        &'static mut Projection,
        Option<&'static Msaa>,
        Has<Fitted>,
    ),
    Without<Presents>,
>;

/// Every camera of a screen draws into the picture: one that draws into the window is
/// turned to it, and to a new picture when the picture changes. A camera that sees the
/// window's pixels sees the units of the old window instead; the camera of a room sees
/// the console's screen, wider when the picture is ([`Wide`]).
fn fit_cameras(frame: Option<ResMut<Frame>>, mut cameras: ScreenCameras, mut commands: Commands) {
    let Some(mut frame) = frame else { return };
    let Some(layout) = frame.layout else { return };
    for (entity, mut camera, mut target, mut projection, msaa, fitted) in &mut cameras {
        // The cameras of one picture agree on this; the room drawn into a picture of
        // its own (`before.rs`) takes the same, so it looks the same.
        if msaa != Some(&frame.msaa) {
            commands.entity(entity).insert(frame.msaa);
        }
        // A camera that draws into a picture of its own (`before.rs`) keeps it, with its
        // place and its units; one that draws into the window or into the picture before
        // this one comes here.
        let comes = match &*target {
            RenderTarget::Window(_) => true,
            RenderTarget::Image(image) if image.handle == frame.picture => false,
            RenderTarget::Image(image) if frame.was.as_ref() == Some(&image.handle) => true,
            _ => continue,
        };
        if comes {
            *target = RenderTarget::Image(frame.picture.clone().into());
        }
        let viewport = layout.viewport(fitted);
        let corners = |viewport: &Option<Viewport>| {
            (viewport.as_ref()).map(|v| (v.physical_position, v.physical_size))
        };
        if corners(&camera.viewport) != corners(&viewport) {
            camera.viewport = viewport;
        }
        let Projection::Orthographic(orthographic) = &mut *projection else {
            continue;
        };
        let units = match (fitted, &orthographic.scaling_mode) {
            (true, _) => layout.screen(),
            (false, ScalingMode::WindowSize) => UNITS,
            (false, _) => continue,
        };
        let fits = matches!(orthographic.scaling_mode,
            ScalingMode::Fixed { width, height } if width == units.x && height == units.y);
        if !fits {
            orthographic.scaling_mode = ScalingMode::Fixed {
                width: units.x,
                height: units.y,
            };
        }
    }
    if frame.was.is_some() {
        frame.was = None;
    }
}

/// The primary window as a run starts it: a play session's from `[display]`, in
/// physical pixels (`exact`); a check's hidden and at the size it always had.
pub fn window(display: &prefs::Display, hidden: bool, exact: bool) -> Window {
    // A phone's window is its whole screen, at the size Android gives it, counted in
    // physical pixels as a play session's window on a PC is.
    if cfg!(target_os = "android") {
        return Window {
            title: "Jade Cocoon".into(),
            resolution: WindowResolution::default().with_scale_factor_override(1.0),
            present_mode: present_mode(display.vsync),
            ..default()
        };
    }
    let size = display.window_size.unwrap_or(display.resolution);
    let mut resolution = WindowResolution::new(size.width, size.height);
    if exact {
        resolution = resolution.with_scale_factor_override(1.0);
    }
    let mode = match display.mode {
        Mode::Windowed => WindowMode::Windowed,
        // The video mode of exclusive fullscreen is the monitor's, which is known once
        // the window is up ([`apply`]); borderless until then.
        Mode::Borderless | Mode::Fullscreen => {
            WindowMode::BorderlessFullscreen(MonitorSelection::Primary)
        }
    };
    let position = match (hidden, display.window_position) {
        (true, _) => WindowPosition::Automatic,
        (false, Some([x, y])) => WindowPosition::At(IVec2::new(x, y)),
        (false, None) => WindowPosition::Centered(MonitorSelection::Primary),
    };
    Window {
        title: "Jade Cocoon".into(),
        resolution,
        mode,
        position,
        present_mode: present_mode(display.vsync),
        visible: !hidden,
        focused: !hidden,
        ..default()
    }
}

fn present_mode(vsync: bool) -> PresentMode {
    match vsync {
        true => PresentMode::AutoVsync,
        false => PresentMode::AutoNoVsync,
    }
}

/// The display of a check run: a window of 960 by 720 (or `--resolution=WxH`), with
/// `--content=`, `--integer-scaling`, `--narrow-fights`, `--render=`, `--msaa=N`,
/// `--crt=` and `--dither` for a look at those settings.
pub fn for_a_check(display: &mut prefs::Display, args: &[String]) {
    let asked = |name: &str| args.iter().find_map(|arg| arg.strip_prefix(name));
    let size = asked("--resolution=").map(|value| {
        value
            .parse::<Size>()
            .expect("--resolution=WxH takes a size like 1920x1080")
    });
    display.mode = Mode::Windowed;
    display.window_size =
        Some(size.unwrap_or(Size::new(crate::WINDOW_SIZE.0, crate::WINDOW_SIZE.1)));
    display.window_position = None;
    if let Some(content) = asked("--content=") {
        display.content = content
            .parse()
            .expect("--content= takes fill, bars, stretch or zoom");
    }
    display.integer_scaling = args.iter().any(|arg| arg == "--integer-scaling");
    display.wide_fights = !args.iter().any(|arg| arg == "--narrow-fights");
    if let Some(render) = asked("--render=") {
        display.render = render.parse().expect("--render= takes sharp or authentic");
    }
    if let Some(msaa) = asked("--msaa=") {
        display.msaa = msaa.parse().expect("--msaa= takes 0, 2, 4 or 8");
    }
    if let Some(crt) = asked("--crt=") {
        display.crt = crt.parse().expect("--crt= takes off, scanlines or crt");
    }
    display.dither = args.iter().any(|arg| arg == "--dither");
    // A check's pictures stay the same from run to run without the glide (`smooth.rs`).
    display.smooth_motion = args.iter().any(|arg| arg == "--smooth-motion");
    // A check that measures the frames runs as fast as it can.
    display.vsync = !args.iter().any(|arg| arg == "--no-vsync");
}

/// What of `[display]` the window has now.
#[derive(Resource, Debug)]
struct Applied {
    /// None until the mode is set (exclusive fullscreen waits for the monitor).
    mode: Option<Mode>,
    resolution: Size,
    vsync: bool,
    /// The fullscreen mode F11 goes back to from the window.
    fullscreen: Mode,
    /// Seconds since the mode changed.
    since: f32,
    /// Frames until a window that came back from the full screen gets its size and place.
    geometry: u8,
    /// The window's place was checked against the monitors.
    placed: bool,
    /// The size of a window the settings asked for last (`window_size`): another one,
    /// as the PC Settings ask for with a resolution, sizes the window anew.
    window_size: Option<Size>,
}

impl Applied {
    fn at_start(display: &prefs::Display) -> Self {
        Self {
            mode: (display.mode != Mode::Fullscreen).then_some(display.mode),
            resolution: display.resolution,
            vsync: display.vsync,
            fullscreen: match display.mode {
                Mode::Windowed => Mode::Borderless,
                mode => mode,
            },
            since: 0.0,
            geometry: 0,
            placed: false,
            window_size: display.window_size,
        }
    }
}

/// F11 and Alt+Enter: from the full screen to a window and back. Enter with Alt is not
/// Start (`keys.rs`).
fn toggle_on_keys(
    keys: Res<ButtonInput<KeyCode>>,
    applied: Res<Applied>,
    mut prefs: ResMut<Prefs>,
    pc: Option<Res<crate::pc_settings::PcSettings>>,
) {
    // A rebinding that waits for a key refuses F11 and does nothing else with it.
    if pc.is_some_and(|pc| pc.capturing()) {
        return;
    }
    let alt = keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);
    let enter = keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]);
    if !keys.just_pressed(KeyCode::F11) && !(alt && enter) {
        return;
    }
    let mode = match prefs.display.mode {
        Mode::Windowed => applied.fullscreen,
        Mode::Borderless | Mode::Fullscreen => Mode::Windowed,
    };
    info!("the window: {:?} to {mode:?} on a key", prefs.display.mode);
    prefs.display.mode = mode;
}

/// The video mode of a monitor that has `size`: the fastest, the deepest.
fn video_mode(monitor: &Monitor, size: Size) -> Option<bevy::window::VideoMode> {
    (monitor.video_modes.iter())
        .filter(|mode| mode.physical_size == size.uvec2())
        .max_by_key(|mode| (mode.refresh_rate_millihertz, mode.bit_depth))
        .copied()
}

/// Whether a place lies on one of the monitors.
fn on_a_monitor<'a>(at: IVec2, mut monitors: impl Iterator<Item = &'a Monitor>) -> bool {
    monitors.any(|monitor| {
        let corner = monitor.physical_position;
        let size = IVec2::new(
            monitor.physical_width as i32,
            monitor.physical_height as i32,
        );
        at.cmpge(corner).all() && at.cmplt(corner + size).all()
    })
}

type Monitors<'w, 's> = Query<'w, 's, (Entity, &'static Monitor, Has<PrimaryMonitor>)>;

/// Gives the window what `[display]` asks for whenever it changes: the mode, the video
/// mode of exclusive fullscreen, the size and place of a window, the vsync.
fn apply(
    time: Res<Time<Real>>,
    prefs: Res<Prefs>,
    mut applied: ResMut<Applied>,
    mut windows: Query<(&mut Window, Option<&OnMonitor>), With<PrimaryWindow>>,
    monitors: Monitors,
) {
    let Ok((mut window, on)) = windows.single_mut() else {
        return;
    };
    applied.since += time.delta_secs();
    let wanted = &prefs.display;
    // The monitor the window is on, else the primary one.
    let monitor = (on.and_then(|on| monitors.get(on.0).ok()))
        .or_else(|| monitors.iter().find(|(_, _, primary)| *primary))
        .map(|(_, monitor, _)| monitor);
    let all = || monitors.iter().map(|(_, monitor, _)| monitor);
    if wanted.vsync != applied.vsync {
        window.present_mode = present_mode(wanted.vsync);
        applied.vsync = wanted.vsync;
        info!("the window: vsync {}", wanted.vsync);
    }
    // A window placed at the start where no monitor is any more goes to the middle,
    // and a window is made no larger than its monitor once the monitors are known (the
    // size of a fresh window is the resolution's, which may be the whole monitor's).
    if !applied.placed && !monitors.is_empty() {
        applied.placed = true;
        if let WindowPosition::At(at) = window.position
            && wanted.mode == Mode::Windowed
            && !on_a_monitor(at, all())
        {
            window.position = WindowPosition::Centered(MonitorSelection::Primary);
        }
        if wanted.mode == Mode::Windowed && applied.geometry == 0 {
            applied.geometry = 1;
        }
    }
    let changed = applied.mode != Some(wanted.mode)
        || (wanted.mode == Mode::Fullscreen && applied.resolution != wanted.resolution);
    if changed {
        window.mode = match wanted.mode {
            Mode::Borderless => WindowMode::BorderlessFullscreen(MonitorSelection::Current),
            Mode::Fullscreen => {
                // The monitors are known a frame or two behind the window.
                let Some(monitor) = monitor else { return };
                let video = video_mode(monitor, wanted.resolution);
                if video.is_none() {
                    let offered: Vec<String> = (RESOLUTIONS.iter())
                        .filter(|size| video_mode(monitor, **size).is_some())
                        .map(Size::to_string)
                        .collect();
                    warn!(
                        "the window: the monitor has no video mode of {}, fullscreen at its                          own; of the list it has {}",
                        wanted.resolution,
                        offered.join(", ")
                    );
                }
                let video = video.map_or(VideoModeSelection::Current, VideoModeSelection::Specific);
                WindowMode::Fullscreen(MonitorSelection::Current, video)
            }
            Mode::Windowed => {
                // The window gets its size and place once it is a window again.
                applied.geometry = 2;
                WindowMode::Windowed
            }
        };
        info!("the window: {:?} at {}", wanted.mode, wanted.resolution);
        if wanted.mode != Mode::Windowed {
            applied.fullscreen = wanted.mode;
        }
        applied.mode = Some(wanted.mode);
        applied.resolution = wanted.resolution;
        applied.window_size = wanted.window_size;
        applied.since = 0.0;
    }
    // A window asked to be of another size (a resolution of the PC Settings) gets it
    // at once, at its place.
    if wanted.mode == Mode::Windowed
        && applied.mode == Some(Mode::Windowed)
        && applied.geometry == 0
        && wanted.window_size != applied.window_size
    {
        applied.window_size = wanted.window_size;
        applied.geometry = 1;
        info!("the window: {:?} asked for", wanted.window_size);
    }
    if applied.geometry > 0 {
        applied.geometry -= 1;
        if applied.geometry == 0 && wanted.mode == Mode::Windowed {
            let mut size = wanted.window_size.unwrap_or(wanted.resolution).uvec2();
            // Not larger than the monitor, with room for the border and the task bar.
            if let Some(monitor) = monitor {
                let room = UVec2::new(monitor.physical_width, monitor.physical_height);
                size = size.min(room.saturating_sub(UVec2::new(32, 96)).max(SCREEN));
            }
            window.resolution.set_physical_resolution(size.x, size.y);
            window.position = match wanted.window_position {
                Some([x, y]) if on_a_monitor(IVec2::new(x, y), all()) => {
                    WindowPosition::At(IVec2::new(x, y))
                }
                _ => WindowPosition::Centered(MonitorSelection::Current),
            };
        }
    }
}

/// The size and the place of a window the player changed are kept, once it rests.
fn remember(
    time: Res<Time<Real>>,
    mut applied: ResMut<Applied>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut resized: MessageReader<WindowResized>,
    mut moved: MessageReader<WindowMoved>,
    mut prefs: ResMut<Prefs>,
    mut resting: Local<Option<f32>>,
) {
    let changed = resized.read().count() + moved.read().count() > 0;
    let Ok(window) = windows.single() else {
        return;
    };
    let a_window = window.mode == WindowMode::Windowed
        && applied.mode == Some(Mode::Windowed)
        && prefs.display.mode == Mode::Windowed
        && applied.geometry == 0
        && applied.since >= AFTER_A_CHANGE;
    if !a_window {
        *resting = None;
        return;
    }
    if changed {
        *resting = Some(0.0);
    }
    let Some(rested) = resting.as_mut() else {
        return;
    };
    *rested += time.delta_secs();
    if *rested < SETTLE {
        return;
    }
    *resting = None;
    let size = window.resolution.physical_size();
    if size.x == 0 || size.y == 0 {
        return;
    }
    let size = Size::new(size.x, size.y);
    let place = match window.position {
        WindowPosition::At(at) => Some([at.x, at.y]),
        _ => prefs.display.window_position,
    };
    if prefs.display.window_size != Some(size) || prefs.display.window_position != place {
        let kept = &mut prefs.display;
        (kept.window_size, kept.window_position) = (Some(size), place);
        // The size the player gave the window is no size to ask for again.
        applied.window_size = Some(size);
        info!("the window: {size} at {place:?} is kept");
    }
}

/// `frame_cap`: the app waits out the rest of a frame that came too soon.
fn cap_frames(prefs: Res<Prefs>, mut last: Local<Option<Instant>>) {
    let cap = prefs.display.frame_cap;
    if cap == 0 {
        *last = None;
        return;
    }
    let frame = Duration::from_secs_f64(1.0 / f64::from(cap.clamp(10, 1000)));
    let now = Instant::now();
    let next = last.map_or(now, |then| then + frame);
    if next > now {
        std::thread::sleep(next - now);
    }
    // A frame that came late starts the count anew.
    *last = Some(match next + frame < now {
        true => now,
        false => next.max(now),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shaped(content: Content) -> Shaping {
        Shaping {
            content,
            ..default()
        }
    }

    #[test]
    fn a_wide_window_shows_the_picture_in_the_middle_with_the_sides_filled() {
        let layout = Layout::of(UVec2::new(1920, 1080), Shaping::default());
        assert_eq!(layout.picture, UVec2::new(1440, 1080));
        assert_eq!(layout.corner, IVec2::new(240, 0));
        assert!(layout.sides);
        assert_eq!(layout.screen(), Vec2::new(320.0, 240.0));
        // Bars are the same picture with nothing beside it.
        let bars = Layout::of(UVec2::new(1920, 1080), shaped(Content::Bars));
        assert_eq!(
            (bars.picture, bars.corner, bars.sides),
            (layout.picture, layout.corner, false)
        );
        // A window of the console's shape has no sides.
        let old = Layout::of(UVec2::new(960, 720), Shaping::default());
        assert_eq!(
            (old.picture, old.corner, old.sides),
            (UVec2::new(960, 720), IVec2::ZERO, false)
        );
        // A tall window: the picture across the whole width, in the middle.
        let tall = Layout::of(UVec2::new(1080, 1920), Shaping::default());
        assert_eq!(
            (tall.picture, tall.corner),
            (UVec2::new(1080, 810), IVec2::new(0, 555))
        );
    }

    #[test]
    fn whole_factors_leave_every_pixel_as_wide_as_the_next() {
        let integer = Shaping {
            integer: true,
            ..default()
        };
        let layout = Layout::of(UVec2::new(1920, 1080), integer);
        assert_eq!(
            (layout.picture, layout.corner),
            (UVec2::new(1280, 960), IVec2::new(320, 60))
        );
        assert!(layout.sides);
        let four_k = Layout::of(UVec2::new(3840, 2160), integer);
        assert_eq!(four_k.picture, UVec2::new(2880, 2160));
        // A window smaller than the console's screen fits as it can.
        let small = Layout::of(UVec2::new(300, 200), integer);
        assert_eq!(small.picture, UVec2::new(267, 200));
    }

    #[test]
    fn stretch_fills_and_zoom_cuts_but_only_movies_and_the_title() {
        let stretch = Layout::of(UVec2::new(1920, 1080), shaped(Content::Stretch));
        assert_eq!(
            (stretch.picture, stretch.corner, stretch.sides),
            (UVec2::new(1920, 1080), IVec2::ZERO, false)
        );
        let zoom = Layout::of(UVec2::new(1920, 1080), shaped(Content::Zoom));
        assert_eq!(zoom, Layout::of(UVec2::new(1920, 1080), Shaping::default()));
        let movie = Shaping {
            content: Content::Zoom,
            zooms: true,
            ..default()
        };
        let zoom = Layout::of(UVec2::new(1920, 1080), movie);
        assert_eq!(
            (zoom.picture, zoom.corner, zoom.sides),
            (UVec2::new(1920, 1440), IVec2::new(0, -180), false)
        );
        // The pixel under the window's middle is the picture's.
        assert_eq!(
            zoom.to_picture(Vec2::new(960.0, 540.0)),
            Some(Vec2::new(960.0, 720.0))
        );
    }

    #[test]
    fn the_mouse_finds_its_pixel_of_the_picture() {
        let layout = Layout::of(UVec2::new(1920, 1080), Shaping::default());
        assert_eq!(layout.to_picture(Vec2::new(240.0, 0.0)), Some(Vec2::ZERO));
        assert_eq!(
            layout.to_picture(Vec2::new(1679.5, 1079.5)),
            Some(Vec2::new(1439.5, 1079.5))
        );
        // The blurred sides are no part of it.
        assert_eq!(layout.to_picture(Vec2::new(239.0, 500.0)), None);
        assert_eq!(layout.to_picture(Vec2::new(1680.0, 500.0)), None);
    }

    #[test]
    fn a_frame_counts_the_mouse_in_physical_pixels() {
        let mut app = App::new();
        app.add_plugins(AssetPlugin::default())
            .init_asset::<Image>()
            .init_asset::<Mesh>()
            .init_asset::<ShrinkMaterial>()
            .init_asset::<SidesMaterial>()
            .init_asset::<PresentMaterial>()
            .add_systems(Startup, start);
        app.update();
        let mut frame = app.world_mut().resource_mut::<Frame>();
        assert_eq!(frame.to_picture(Vec2::ZERO), None);
        frame.layout = Some(Layout::of(UVec2::new(1920, 1080), Shaping::default()));
        // Windows' display scaling at 150 %: the mouse's 160 logical pixels are 240.
        frame.scale = 1.5;
        assert_eq!(frame.to_picture(Vec2::new(160.0, 0.0)), Some(Vec2::ZERO));
        assert_eq!(
            frame.to_picture(Vec2::new(640.0, 360.0)),
            Some(Vec2::new(720.0, 540.0))
        );
    }

    #[test]
    fn a_wide_picture_takes_the_window_and_keeps_a_square_for_the_menus() {
        let wide = Shaping {
            wide: true,
            ..default()
        };
        let layout = Layout::of(UVec2::new(1920, 1080), wide);
        assert_eq!(layout.picture, UVec2::new(1920, 1080));
        assert_eq!(layout.square, URect::new(240, 0, 1680, 1080));
        let screen = layout.screen();
        assert!((screen.x - 426.666_66).abs() < 0.01 && screen.y == 240.0);
        // The room's camera takes the whole picture, a menu's the square.
        assert!(layout.viewport(true).is_none());
        let menu = layout.viewport(false).unwrap();
        assert_eq!(
            (menu.physical_position, menu.physical_size),
            (UVec2::new(240, 0), UVec2::new(1440, 1080))
        );
        // Not wide, both take the whole picture.
        let narrow = Layout::of(UVec2::new(1920, 1080), Shaping::default());
        assert!(narrow.viewport(true).is_none() && narrow.viewport(false).is_none());
    }

    #[test]
    fn an_authentic_picture_has_the_consoles_pixels_and_the_mouse_finds_them() {
        let authentic = Shaping {
            authentic: true,
            ..default()
        };
        let layout = Layout::of(UVec2::new(1920, 1080), authentic);
        // In the window as large as ever, drawn at 320 by 240.
        assert_eq!(
            (layout.picture, layout.corner),
            (UVec2::new(1440, 1080), IVec2::new(240, 0))
        );
        assert_eq!(layout.pixels, SCREEN);
        assert_eq!(layout.pixel_square, URect::new(0, 0, 320, 240));
        assert!(layout.sides && layout.viewport(false).is_none());
        // The mouse counts in the console's pixels, and a place comes back where it was.
        assert_eq!(
            layout.on_window(Vec2::new(100.0, 50.0)),
            Vec2::new(690.0, 225.0)
        );
        assert_eq!(
            layout.to_picture(Vec2::new(690.0, 225.0)),
            Some(Vec2::new(100.0, 50.0))
        );
        assert_eq!(layout.to_picture(Vec2::new(239.0, 500.0)), None);
        // A wide fight: 240 lines as wide as 16:9, the 4:3 part in the middle.
        let wide = Shaping {
            wide: true,
            authentic: true,
            ..default()
        };
        let layout = Layout::of(UVec2::new(1920, 1080), wide);
        assert_eq!(layout.pixels, UVec2::new(426, 240));
        assert_eq!(layout.pixel_square, URect::new(53, 0, 373, 240));
        assert_eq!(layout.screen(), Vec2::new(426.0, 240.0));
        assert!(layout.viewport(true).is_none());
        let menu = layout.viewport(false).unwrap();
        assert_eq!(
            (menu.physical_position, menu.physical_size),
            (UVec2::new(53, 0), UVec2::new(320, 240))
        );
        let at = layout.on_window(Vec2::new(160.0, 120.0));
        let back = layout.to_picture(at).unwrap();
        assert!((back - Vec2::new(213.0, 120.0)).abs().max_element() < 0.001);
        // A window of a few lines asks for no picture wider than a card makes.
        let thin = Layout::of(UVec2::new(1920, 20), wide);
        assert!(thin.pixels.x <= 4 * SCREEN.x, "{:?}", thin.pixels);
        // The same settings drawn sharp keep the window's pixels.
        let sharp = Layout::of(UVec2::new(1920, 1080), Shaping::default());
        assert_eq!(
            (sharp.pixels, sharp.pixel_square),
            (sharp.picture, sharp.square)
        );
    }

    #[test]
    fn msaa_takes_what_the_card_offers() {
        assert_eq!(asked_samples(0), Msaa::Off);
        assert_eq!(asked_samples(1), Msaa::Off);
        assert_eq!(asked_samples(2), Msaa::Sample2);
        assert_eq!(asked_samples(4), Msaa::Sample4);
        assert_eq!(asked_samples(6), Msaa::Sample4);
        assert_eq!(asked_samples(8), Msaa::Sample8);
        assert_eq!(asked_samples(16), Msaa::Sample8);
        // Without a card known, 4 at most.
        assert_eq!(samples(Msaa::Sample8, None), Msaa::Sample4);
        assert_eq!(samples(Msaa::Sample2, None), Msaa::Sample2);
        assert_eq!(samples(Msaa::Off, None), Msaa::Off);
    }

    #[test]
    fn sizes_and_ways_read_and_write_as_text() {
        assert_eq!("1920x1080".parse::<Size>(), Ok(Size::new(1920, 1080)));
        assert_eq!(" 2560 X 1440 ".parse::<Size>(), Ok(Size::new(2560, 1440)));
        assert!("1920".parse::<Size>().is_err() && "0x10".parse::<Size>().is_err());
        assert_eq!(Size::new(1280, 720).to_string(), "1280x720");
        assert_eq!("zoom".parse::<Content>(), Ok(Content::Zoom));
        assert_eq!("authentic".parse::<Render>(), Ok(Render::Authentic));
        assert_eq!("scanlines".parse::<Filter>(), Ok(Filter::Scanlines));
        assert!("blurry".parse::<Render>().is_err() && "tv".parse::<Filter>().is_err());
        // 16:9 first, the default among them.
        assert!(
            RESOLUTIONS[..5]
                .iter()
                .all(|size| size.width * 9 == size.height * 16)
        );
        assert!(RESOLUTIONS.contains(&prefs::Display::default().resolution));
    }

    #[test]
    fn a_check_keeps_its_window_and_takes_its_options() {
        let mut display = prefs::Display::default();
        for_a_check(&mut display, &[]);
        assert_eq!(
            (display.mode, display.window_size),
            (Mode::Windowed, Some(Size::new(960, 720)))
        );
        let args = [
            "--resolution=1920x1080",
            "--content=bars",
            "--integer-scaling",
        ]
        .map(String::from);
        for_a_check(&mut display, &args);
        assert_eq!(display.window_size, Some(Size::new(1920, 1080)));
        assert_eq!(display.content, Content::Bars);
        assert!(display.integer_scaling);
        assert!(display.wide_fights);
        for_a_check(&mut display, &["--narrow-fights".to_string()]);
        assert!(!display.wide_fights);
        assert_eq!(
            (display.render, display.msaa, display.crt, display.dither),
            (Render::Sharp, 0, Filter::Off, false)
        );
        let args = ["--render=authentic", "--msaa=4", "--crt=crt", "--dither"].map(String::from);
        for_a_check(&mut display, &args);
        assert_eq!(
            (display.render, display.msaa, display.crt, display.dither),
            (Render::Authentic, 4, Filter::Crt, true)
        );
    }

    #[test]
    fn f11_goes_to_a_window_and_back_to_the_full_screen_it_came_from() {
        let mut app = App::new();
        let display = prefs::Display {
            mode: Mode::Fullscreen,
            ..default()
        };
        app.insert_resource(ButtonInput::<KeyCode>::default())
            .insert_resource(Applied::at_start(&display))
            .insert_resource(Prefs {
                display,
                ..default()
            })
            .add_systems(Update, toggle_on_keys);
        let press = |app: &mut App, keys: &[KeyCode]| {
            let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            input.reset_all();
            for key in keys {
                input.press(*key);
            }
            app.update();
            app.world().resource::<Prefs>().display.mode
        };
        assert_eq!(press(&mut app, &[KeyCode::F11]), Mode::Windowed);
        assert_eq!(press(&mut app, &[KeyCode::Enter]), Mode::Windowed);
        assert_eq!(
            press(&mut app, &[KeyCode::AltLeft, KeyCode::Enter]),
            Mode::Fullscreen
        );
    }

    #[test]
    fn exclusive_fullscreen_takes_the_monitors_fastest_mode_of_the_size() {
        let mode = |width, height, hz, depth| bevy::window::VideoMode {
            physical_size: UVec2::new(width, height),
            bit_depth: depth,
            refresh_rate_millihertz: hz,
        };
        let monitor = Monitor {
            name: None,
            physical_width: 2560,
            physical_height: 1440,
            physical_position: IVec2::ZERO,
            refresh_rate_millihertz: Some(144_000),
            scale_factor: 1.0,
            video_modes: vec![
                mode(1920, 1080, 60_000, 32),
                mode(1920, 1080, 144_000, 32),
                mode(2560, 1440, 144_000, 32),
            ],
        };
        let found = video_mode(&monitor, Size::new(1920, 1080)).unwrap();
        assert_eq!(found.refresh_rate_millihertz, 144_000);
        assert!(video_mode(&monitor, Size::new(1600, 900)).is_none());
        assert!(on_a_monitor(IVec2::new(100, 100), [&monitor].into_iter()));
        assert!(!on_a_monitor(
            IVec2::new(-5000, 100),
            [&monitor].into_iter()
        ));
    }
}
