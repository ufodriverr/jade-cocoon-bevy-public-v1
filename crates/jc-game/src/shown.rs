
use std::collections::BTreeMap;
use std::sync::Arc;

use bevy::asset::{RenderAssetUsages, uuid_handle};
use bevy::mesh::{MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, Extent3d, RenderPipelineDescriptor, SpecializedMeshPipelineError,
    TextureDimension, TextureFormat,
};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dKey, Material2dPlugin};
use jc_formats::action::{Action, Point as BodyPoint};
use jc_formats::animation::{Animation, EventAction, Playback};
use jc_formats::area::DrawArea;
use jc_formats::background::{SCREEN, ViewCamera, ViewHeader, ZOOM_ONE};
use jc_formats::battle::arena::{self, Centre, Place, Shot};
use jc_formats::carried::{Carried, LEVEL_AT_THE_START};
use jc_formats::clip;
use jc_formats::command::{self, Feet, OWN_SIZE, Runner};
use jc_formats::fade::Fade;
use jc_formats::field::Ground;
use jc_formats::glide;
use jc_formats::halo;
use jc_formats::light::{self, BoneLight, Lights, Movable, TEXEL_UNCHANGED};
use jc_formats::mesh::{Face, Kept, MeshBlock, MeshObject, TexturePage};
use jc_formats::mover::{self, ArcTangent, Footing, Mover};
use jc_formats::movie::{CodeTable, PictureDecoder};
use jc_formats::package::{Kind, Package, Placement as Piece, Quad, Sprite, SpriteTexture, Stand};
use jc_formats::player::{self, Held, Thing};
use jc_formats::ride;
use jc_formats::scatter;
use jc_formats::scene::{ActorMoves, Show, ShowClock, ShowWay};
use jc_formats::show_mover::{Moved, ShowMover};
use jc_formats::skeleton::{self, BoneScale, Placement, Scales, Skeleton};
use jc_formats::sky;
use jc_formats::tim::Tim;
use jc_formats::tint;
use jc_formats::vram::{PAGE, Texels, Vram};
use jc_formats::window::{self, Faces};

use crate::field::{self, AddedMaterial, FieldScreen, GroupPart, RoomViews, SLOT_DEPTH, WalkLines};
use crate::fight_effects::Playing;
use crate::model_view::Mix;
use crate::name_entry::Named;
use crate::scripts::{Noticed, Scripts, Spoken};
use crate::smooth::{self, Corners, Id, Motion};
use crate::walker::Walker;

const THING_SHADER: Handle<Shader> = uuid_handle!("3c7a1b52-94e6-4d0f-a8b3-61f2c07d9e45");
/// What the matrix unit multiplies the summed depths of three and of four corners by, 4096
/// being one: a quarter of the average depth comes out.
const DEPTH_OF_THREE: i64 = 341;
const DEPTH_OF_FOUR: i64 = 256;
/// The largest scaled depth a face is drawn at, and the scaled depths to one slot.
const DEPTH_LIMIT: i64 = 4095;
const DEPTHS_PER_SLOT: i64 = 4;
const TABLE_SLOTS: u32 = 1024;
/// The share of a slot's depth that a slot of the second table takes on the depth axis,
/// so that both tables fit in front of the camera's far end.
const SECOND_TABLE_SHARE: f32 = 0.08;
const FACES_FROM: f32 = -0.5;
const FACES_SPAN: f32 = 0.3;
/// The same for the faces that an object task adds, which runs before the background's:
/// they lie in front of the layers of their slot, and behind the faces of the next one.
const FACES_OVER: f32 = 0.05;
/// The mode word of a sprite record: the bit that puts its quads into slot 0, in front of
/// everything, the bit that blends them and where the blend mode starts.
const SPRITE_IN_FRONT: u16 = 1 << 3;
const SPRITE_BLENDS: u16 = 1 << 4;
const SPRITE_BLEND_MODE: u16 = 5;
const MIXED_FLAGS: u32 = 0x31;
const FADED: u32 = 0x80;
const SCROLL_POINT_ABOVE: i32 = 350;

pub struct ShownPlugin;

impl Plugin for ShownPlugin {
    fn build(&self, app: &mut App) {
        let shader = Shader::from_wgsl(include_str!("thing.wgsl"), "thing.wgsl");
        app.world_mut()
            .resource_mut::<Assets<Shader>>()
            .insert(&THING_SHADER, shader)
            .expect("a fixed handle takes an asset");
        app.add_plugins(Material2dPlugin::<ThingMaterial>::default())
            .init_resource::<Steering>()
            .init_resource::<smooth::Glide>()
            .add_systems(
                PostUpdate,
                crate::display::keep_dither::<ThingMaterial>.after(crate::display::Fitting),
            )
            .add_systems(FixedUpdate, smooth::count_steps)
            .add_systems(
                FixedUpdate,
                (
                    step_things,
                    change_view,
                    follow_flags.after(field::tick_groups),
                    count_game_frames,
                )
                    .chain()
                    .run_if(crate::movie_view::room_runs),
            )
            // The background takes its brightness, scroll, zoom and draw area from the
            // room before it is placed, or a new room shows one frame bright and uncut.
            .add_systems(
                Update,
                (
                    switch_glide,
                    draw_things,
                    share_glide,
                    glide_things,
                    draw_form,
                    follow_player
                        .before(field::place_groups)
                        .before(field::cut_to_the_area),
                    glide_scroll.before(field::place_groups),
                    place_things,
                )
                    .chain(),
            )
            .add_systems(Update, point_at_answers)
            .add_systems(Update, crate::text_view::paint_script_talk)
            .add_systems(Update, crate::fight_menu_view::paint_fight_menu);
    }
}

/// The material of the model view for the console's screen: texels times corner colour in
/// the console's colour values, mixed with what is behind in one of the console's ways.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
#[bind_group_data(Mix)]
pub struct ThingMaterial {
    #[texture(0)]
    #[sampler(1)]
    texture: Handle<Image>,
    #[uniform(2)]
    share: Vec4,
    mix: Mix,
}

impl From<&ThingMaterial> for Mix {
    fn from(material: &ThingMaterial) -> Self {
        material.mix
    }
}

impl crate::display::Dithers for ThingMaterial {
    fn dither(&self) -> f32 {
        self.share.w
    }

    fn set_dither(&mut self, dither: f32) {
        self.share.w = dither;
    }
}

impl Material2d for ThingMaterial {
    fn fragment_shader() -> ShaderRef {
        THING_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        // Sorted by depth with the pictures of the background; a piece that is not mixed
        // has a share of one and covers what is behind it.
        AlphaMode2d::Blend
    }

    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // The faces that are turned away are left out before they come here.
        descriptor.primitive.cull_mode = None;
        if let (Some(blend), Some(fragment)) =
            (key.bind_group_data.blend(), &mut descriptor.fragment)
        {
            for target in fragment.targets.iter_mut().flatten() {
                target.blend = Some(blend);
            }
        }
        Ok(())
    }
}

/// Whether a texture page holds the place a clip is unpacked to.
fn holds_the_clip(page: TexturePage) -> bool {
    let (x, y) = page.origin();
    let (x, y) = (usize::from(x), usize::from(y));
    (x..x + PAGE).contains(&clip::AT.0) && (y..y + PAGE).contains(&clip::AT.1)
}

/// What a triangle is drawn with: a texture page (all of its drawn texels, or one of the two
/// drawing steps of a blended face) or the corner colours alone, and how it is mixed.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Skin {
    /// Which video memory of the room the page is read from (`Things::vrams`): the one of
    /// its object list for an actor, the first one for a thing of the room's package, whose
    /// images are the same in all of them.
    source: usize,
    page: Option<TexturePage>,
    texels: Option<Texels>,
    mix: Mix,
}

impl Skin {
    fn of(face: &Face, forced: Option<u8>, source: usize) -> impl Iterator<Item = Self> {
        let skins = match face.texture {
            None => {
                let mix = match (forced, face.blends_by_itself()) {
                    (Some(mode), _) => Mix::of_mode(mode),
                    (None, true) => Mix::Added,
                    (None, false) => Mix::Solid,
                };
                [Some((None, mix)), None]
            }
            Some(page) => match forced.or(face.blends_by_itself().then(|| page.blend_mode())) {
                Some(mode) => [
                    Some((Some(Texels::Unmarked), Mix::Solid)),
                    Some((Some(Texels::Marked), Mix::of_mode(mode))),
                ],
                None => [Some((None, Mix::Solid)), None],
            },
        };
        let page = face.texture;
        (skins.into_iter().flatten()).map(move |(texels, mix)| Self {
            source,
            page,
            texels,
            mix,
        })
    }

    /// The skins a quad of a sprite is drawn with: the sprite's picture as it is, or, when
    /// the record blends, its marked texels mixed in the record's mode after the others.
    fn of_sprite(page: TexturePage, blend: Option<u8>) -> impl Iterator<Item = Self> {
        let skins = match blend {
            Some(mode) => [
                Some((Some(Texels::Unmarked), Mix::Solid)),
                Some((Some(Texels::Marked), Mix::of_mode(mode))),
            ],
            None => [Some((None, Mix::Solid)), None],
        };
        let page = Some(page);
        (skins.into_iter().flatten()).map(move |(texels, mix)| Self {
            source: 0,
            page,
            texels,
            mix,
        })
    }

    /// The skin with its palette `rows` rows further down, as the second byte of the word
    /// an object is drawn with moves it (`TexturePage::lowered`).
    fn lowered(mut self, rows: u8) -> Self {
        self.page = self.page.map(|page| page.lowered(rows));
        self
    }

    /// The material of the skin, its page read from the video memory of its source.
    fn material(self, vram: &Vram, images: &mut Assets<Image>) -> ThingMaterial {
        let (size, pixels) = match (self.page, self.texels) {
            (None, _) => (1, vec![u8::MAX; 4]),
            (Some(page), None) => (PAGE as u32, vram.page_rgba(page, false)),
            (Some(page), Some(texels)) => (PAGE as u32, vram.page_part(page, texels)),
        };
        ThingMaterial {
            // The console's colour values as they are, which is what the shader works in.
            texture: images.add(Image::new(
                Extent3d {
                    width: size,
                    height: size,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                pixels,
                TextureFormat::Rgba8Unorm,
                RenderAssetUsages::RENDER_WORLD,
            )),
            // The fourth share is the dither (`display::keep_dither`).
            share: Vec3::splat(self.mix.share()).extend(0.0),
            mix: self.mix,
        }
    }
}

/// A corner on the screen: pixels right of and below the projection centre.
#[derive(Clone, Copy)]
struct Point {
    at: Vec2,
    colour: Vec3,
    uv: [u8; 2],
}

/// A triangle in its slot of the ordering table.
struct Triangle {
    slot: u32,
    /// Added before the background's layers, so drawn over those of its slot.
    over: bool,
    skin: Skin,
    corners: [Point; 3],
}

/// The triangles of a drawing, each with its [`smooth::Id`]: the thing they are added for
/// now and the number of their face among the faces of the thing that were looked at, so
/// that the next game frame knows them again (`smooth.rs`).
#[derive(Default)]
struct Drawing {
    triangles: Vec<(Triangle, Id)>,
    thing: u32,
    faces: u32,
}

impl Drawing {
    /// The triangles from here on are of `thing`, its faces counted from 0.
    fn thing(&mut self, thing: u32) {
        (self.thing, self.faces) = (thing, 0);
    }

    /// The faces of the thing from here on are counted from `faces`: a second kind of
    /// face of the same thing (the stitch quads of a model).
    fn faces_from(&mut self, faces: u32) {
        self.faces = faces;
    }

    /// The number of the next face or quad of the thing, counted whether it is drawn or
    /// not, so that a face turned away does not change the numbers of the others.
    fn face(&mut self) -> u32 {
        let face = self.faces;
        self.faces = self.faces.wrapping_add(1);
        face
    }

    /// Adds a triangle: `half` of the face (or quad) numbered `face`.
    fn push(&mut self, (face, half): (u32, usize), triangle: Triangle) {
        let face = face.wrapping_mul(2).wrapping_add(half as u32);
        let id = Id {
            thing: self.thing,
            face,
        };
        self.triangles.push((triangle, id));
    }

    fn iter(&self) -> impl Iterator<Item = &Triangle> {
        self.triangles.iter().map(|(triangle, _)| triangle)
    }
}

/// The things of a drawing as [`Drawing::thing`] names them: a kind in the upper half of
/// the number, the number of one of its kind in the lower.
mod thing {
    pub const FIGHT_GROUND: u32 = 1 << 16;
    pub const FIGHT_PICTURES: u32 = 2 << 16;
    pub const FIGHT_STRIP: u32 = 3 << 16;
    pub const SKY: u32 = 4 << 16;
    pub const DOORS: u32 = 5 << 16;
    pub const FOUND: u32 = 6 << 16;
    pub const CARD: u32 = 7 << 16;
    pub const SCATTERED: u32 = 8 << 16;
    pub const SPARKS: u32 = 9 << 16;
    pub const PLAYED: u32 = 10 << 16;
    pub const SHOWN: u32 = 11 << 16;
    pub const PLAYER: u32 = 12 << 16;
    pub const ACTOR: u32 = 13 << 16;

    /// The one of a kind with this number.
    pub fn nth(kind: u32, number: usize) -> u32 {
        kind | (number as u32 & 0xFFFF)
    }
}

/// Where the stitch quads of a model are counted from among its faces.
const STITCH_FACES: u32 = 1 << 24;

/// What became of the faces of one record.
#[derive(Default)]
struct Count {
    drawn: usize,
    turned_away: usize,
    /// A corner nearer than the camera draws, or a depth outside the ordering table.
    out_of_depth: usize,
    slots: Option<(u32, u32)>,
    /// The corners of the drawn faces lie between these two places on the screen.
    extent: Option<(Vec2, Vec2)>,
}

impl Count {
    /// One more face or quad is drawn, in this slot with these corners.
    fn add(&mut self, slot: u32, points: &[Point]) {
        self.drawn += 1;
        self.slots = Some(match self.slots {
            Some((low, high)) => (low.min(slot), high.max(slot)),
            None => (slot, slot),
        });
        for point in points {
            self.extent = Some(match self.extent {
                Some((low, high)) => (low.min(point.at), high.max(point.at)),
                None => (point.at, point.at),
            });
        }
    }

    /// Where the drawn things lie, for the line about a record. The screen reaches 160
    /// pixels to either side of the projection centre and 120 up and down.
    fn place(&self) -> String {
        let slots = match self.slots {
            Some((low, high)) => format!("in slots {low} to {high}"),
            None => "in no slot".to_string(),
        };
        match self.extent {
            Some((low, high)) => format!("{slots} from {low:.0} to {high:.0} of the screen"),
            None => slots,
        }
    }
}

/// The signed area of a triangle on the screen, twice: above zero when its corners run the
/// way round that the console draws.
fn area(a: Vec2, b: Vec2, c: Vec2) -> f32 {
    (b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y)
}

/// What becomes of a face with corners in the room.
enum Seen {
    /// Its slot of the ordering table and its corners on the screen.
    Drawn(u32, Vec<Vec2>),
    TurnedAway,
    OutOfDepth,
}

pub struct Standing {
    pub block: MeshBlock,
    /// Which video memory of the room's models the actor is drawn from: the one of its
    /// object list, where its model lies at the place of its slot (the block's page and
    /// palette words name that place).
    pub memory: usize,
    pub skeleton: Skeleton,
    /// Where every bone stands about the actor's place, the actor's turn in it.
    pub pose: Vec<Option<skeleton::Placement>>,
    /// The scale of the whole model and the bone scales of the actor's record.
    pub model_scale: Option<[i32; 3]>,
    pub bone_scales: Vec<BoneScale>,
    pub place: [i32; 3],
    /// The turn about the upright axis, 4096 to the circle.
    pub angle: i16,
    /// The animations the actor can play, by their number in its model: that of the action
    /// it starts in and those of the actions a list it was given starts. And the action
    /// table of its model.
    pub animations: BTreeMap<usize, Animation>,
    pub actions: Vec<Action>,
    pub points: Vec<BodyPoint>,
    pub shape: Option<skeleton::Shape>,
    pub shelf: BTreeMap<usize, Animation>,
    pub sets: Vec<Vec<usize>>,
    pub brought: Vec<usize>,
    /// What the game's interpreter and the step of the animation work on.
    pub runner: Runner,
    pub mover: Mover,
    pub arc: Arc<ArcTangent>,
    /// The ground an actor with flag 0x40000 walks over: tables 3 and 4 of the room. None
    /// until something gives the actor that flag.
    pub ground: Option<Arc<Ground>>,
    /// Game frames since the actor was made, which stands in for the game's counter of the
    /// turn filter.
    pub game_frame: u32,
    pub name: Vec<u16>,
    pub mirrored: bool,
    /// The slot of the field's model loader that holds the actor's model: its face lies
    /// at the place of that slot in the video memory of its list.
    pub slot: usize,
    /// The actions and the turn step of the actor's record for a script's order to go or
    /// turn somewhere; none for the player's body.
    pub moves: Option<ActorMoves>,
    pub kept: Kept,
    pub roams: Option<Roams>,
    pub mate: Option<(usize, usize)>,
    pub carried: Option<usize>,
    pub opponent: Option<usize>,
    pub table: u32,
}

/// The longest step along an axis that the log takes for a roaming creature's walk; a
/// longer one in a game frame gets a line.
const LONGEST_STEP: i32 = 150;

pub struct Roams {
    pub routine: Option<jc_formats::roamer::Routine>,
    pub record: jc_formats::roamer::Roamer,
    /// What the picking answered for the creature: its number in the room, which the
    /// forgetting goes by, and the specs of the fight with it. And the slot of the
    /// room's word in what the game carries (area, place, slot).
    pub stand: jc_formats::roaming::Stand,
    pub slot: (usize, usize, i16),
    /// Where the creature stood in the game frame before, for the log's line about a
    /// step that is far longer than a walk's.
    pub stood: Option<[i32; 3]>,
    pub held: bool,
}

/// What a script's message command takes from the room for its window: the speaker's
/// name, face and mirror word, and the side of the picture.
pub struct Speaker {
    pub name: Vec<u16>,
    pub mirrored: bool,
    /// The face as it lies in video memory, RGBA.
    pub face: Vec<u8>,
    pub picture_left: bool,
    /// Where the speaker and the thing it speaks to are seen, pixels right of the
    /// projection centre.
    pub seen: [i32; 2],
}

impl Standing {
    /// The loader reads one animation of the model (`0x800254A8`): whether it was not in
    /// memory before. Such an animation stays when a set is dropped.
    pub fn read_animation(&mut self, number: usize) -> bool {
        let Some(animation) = self.shelf.remove(&number) else {
            return false;
        };
        self.animations.insert(number, animation);
        true
    }

    /// An animation that was read goes back to what the loader can bring. An animation
    /// that is not in memory stays where it is.
    pub fn give_back(&mut self, number: usize) {
        if let Some(animation) = self.animations.remove(&number) {
            self.shelf.insert(number, animation);
        }
    }

    pub fn read_set(&mut self, set: usize) -> &[usize] {
        for number in std::mem::take(&mut self.brought) {
            if let Some(animation) = self.animations.remove(&number) {
                self.shelf.insert(number, animation);
            }
        }
        for number in self.sets.get(set).cloned().unwrap_or_default() {
            if self.read_animation(number) {
                self.brought.push(number);
            }
        }
        &self.brought
    }

    pub fn scale_in_use(&self) -> Option<[i32; 3]> {
        self.model_scale.map(|own| self.runner.scale(own))
    }

    /// The scales the actor is posed and drawn with: those of its record, the model's
    /// times the size its command list has given it.
    fn scales(&self) -> Scales<'_> {
        let model = match (self.model_scale, self.runner.size) {
            (None, OWN_SIZE) => None,
            (own, _) => Some(self.runner.scale(own.unwrap_or([light::ONE; 3]))),
        };
        Scales {
            model,
            bones: &self.bone_scales,
        }
    }

    pub fn stand(&mut self) {
        let playback = &self.runner.playback;
        let Some(animation) = self.animations.get(&playback.animation) else {
            return;
        };
        let frame = playback.frame(animation.frames);
        let turn = skeleton::rotation([0, self.angle, 0]);
        let scales = self.scales();
        let fading =
            (playback.fade).and_then(|fade| Some((fade, self.animations.get(&fade.animation)?)));
        self.pose = match fading {
            Some((fade, old)) => {
                let old = (old, Playback::frame_of(fade.counter, old.frames));
                let share = fade.old_share();
                let (new, shape) = ((animation, frame), self.shape.as_ref());
                (self.skeleton).pose_faded_shaped_from(turn, new, old, share, &scales, shape)
            }
            None => {
                let shape = self.shape.as_ref();
                (self.skeleton).pose_shaped_from(turn, animation, frame, &scales, shape)
            }
        };
    }

    fn fire_events(&mut self) -> (Vec<(i16, i16)>, Vec<u16>) {
        let playback = &self.runner.playback;
        let Some(animation) = self.animations.get(&playback.animation) else {
            return (Vec::new(), Vec::new());
        };
        let frame = playback.frame(animation.frames);
        let fired = || {
            (animation.events.iter())
                .filter(|event| usize::try_from(event.frame).is_ok_and(|at| at == frame))
        };
        let swaps: Vec<(i16, i16)> = fired()
            .filter_map(|event| match event.action() {
                EventAction::Mesh { object, source } => Some((object, source)),
                _ => None,
            })
            .collect();
        let effects: Vec<u16> = fired()
            .filter_map(|event| match event.action() {
                EventAction::Effect(number) => Some(number),
                _ => None,
            })
            .collect();
        let mut changed = Vec::new();
        for (object, source) in swaps {
            let shown = |block: &MeshBlock| {
                let object = usize::try_from(object).ok()?;
                block.objects.get(object).cloned()
            };
            let before = shown(&self.block);
            if self.block.swap(&self.kept, object, source) && shown(&self.block) != before {
                changed.push((object, source));
            }
        }
        (changed, effects)
    }

    fn walk(&mut self) {
        let playback = &self.runner.playback;
        let Some(animation) = self.animations.get(&playback.animation) else {
            return;
        };
        let frame = playback.frame(animation.frames);
        let whole = skeleton::ONE;
        let footing = Footing {
            frame,
            travel: animation.travel(frame).unwrap_or_default(),
            // The game uses 32 for a block without the word.
            divisor: animation.extra.map_or(32, |extra| extra as i32),
            scales: [
                (self.bone_scales.first()).map_or(whole, |first| i32::from(first.scale[2])),
                self.model_scale.map_or(whole, |scale| scale[2]),
            ],
        };
        self.mover.step(&footing, &self.arc, self.game_frame);
        if let Some(ground) = &self.ground {
            self.mover.walk(ground, &self.arc);
        }
        (self.place, self.angle) = (self.mover.position, self.mover.turn);
    }

    /// Whether the actor's task is held: a creature that roams, while another one of
    /// its room fights.
    fn is_held(&self) -> bool {
        self.roams.as_ref().is_some_and(|roams| roams.held)
    }

    /// What the actor's task does in a game frame before the drawing: a pass over the
    /// command list, the mover, then the size and the word the model is drawn with.
    fn begin(&mut self) {
        if self.is_held() {
            return;
        }
        self.pass();
        self.walk();
        self.runner.update();
        self.stand();
    }

    pub fn pass(&mut self) {
        // Who stands near a target is not asked.
        let feet = Feet {
            mover: &mut self.mover,
            arc: &self.arc,
            counter: self.game_frame,
            someone_near: false,
        };
        let ran = self.runner.pass_with(&self.actions, Some(feet));
        // A list at its end runs that end in every pass.
        if ran.iter().any(|kind| *kind != command::Kind::End) {
            debug!(
                "game frame {}: a list ran {ran:?}, now at {:?}, stuck {:?}",
                self.game_frame,
                self.runner.command(),
                self.runner.stuck
            );
        }
    }

    /// The step of the animation that ends a game frame. An actor on a path plays its
    /// action round until the path is through, which puts it on the action's end frame.
    fn end(&mut self) {
        if self.is_held() {
            return;
        }
        if self.runner.step().looped {
            self.mover.flags |= mover::flag::NO_STEP;
        }
        if self.mover.flags & mover::flag::PATH != 0 {
            let through = self.mover.path_step();
            self.runner.path_step(through);
        }
        self.game_frame = self.game_frame.wrapping_add(1);
    }
}

pub fn in_the_way(actors: &[Standing]) -> Vec<Thing> {
    let thing = |actor: &Standing| Thing {
        position: actor.place,
        hidden: actor.runner.hidden,
    };
    let own = |actor: &&Standing| {
        actor.roams.is_none()
            && actor.mate.is_none()
            && actor.carried.is_none()
            && actor.opponent.is_none()
    };
    actors.iter().filter(own).map(thing).collect()
}

/// The list of a room that is shown and the actors that stand in it, and what the next game
/// frame needs to draw them again.
#[derive(Resource)]
pub struct Things {
    /// The package of the room's things, when the room has one.
    package: Option<Package>,
    vrams: Vec<Vram>,
    header: ViewHeader,
    camera: ViewCamera,
    actors: Vec<Standing>,
    /// The player, when the room is shown with one, and whether it stands at one game
    /// frame.
    player: Option<Walker>,
    player_still: bool,
    /// The exit the exit routine took in this game frame, for the room change.
    exit_due: Option<jc_formats::scene::Exit>,
    /// The records that are shown, each with its clock and what its mover gave in this
    /// game frame: none without a mover or with one that is left out.
    shows: Vec<(Show, ShowClock, Option<Moved>)>,
    room_light: Option<Movable>,
    /// The list stands at one game frame.
    still: bool,
    /// The actors stand at one game frame.
    actors_still: bool,
    /// The field's list ended in front of the player's and the actors' tasks in the
    /// game frame before (`Scripts::stalled`).
    stalled: bool,
    materials: Vec<(Skin, Handle<ThingMaterial>)>,
    /// The tables of the console's picture decoder, which unpack the pictures of a clip;
    /// without them no clip is played. And the clip and picture that lie in the video
    /// memories now.
    clips: Option<(CodeTable, PictureDecoder)>,
    clip_on: Option<(usize, usize)>,
    /// What is on screen is the state of the clocks.
    drawn: bool,
    /// The lines about the first drawing have been logged.
    told: bool,
    /// The scripts of the room, when they are run (`scripts.rs`).
    scripts: Option<Scripts>,
    /// The player's name as a text, empty until the name entry has given one.
    name: Vec<u16>,
    /// Game frames the actors run before the first drawing.
    actors_ahead: u32,
    room_models: Option<MeshBlock>,
    view: usize,
    /// The header of every view of the room, for a view that a script puts in place;
    /// empty when the room keeps one view.
    headers: Vec<Option<ViewHeader>>,
    /// The zoom the room is seen at, [`ZOOM_ONE`] without a script that orders one.
    zoom: i32,
    /// The place of a fight, while one is drawn in place of the painted background.
    arena: Option<Arena>,
    /// The last drawing was of a fight: a held picture is the fight's.
    fight_drawn: bool,
    /// Pixels of the console's screen the picture of a fight reaches beyond either side
    /// of it in a wide window (`widescreen.rs`), 0 as on the console.
    reach: i32,
    /// What the room was seen with before a fight of its scripts took the picture: the
    /// header of the view, the zoom and the video memories. None without such a fight.
    before_fight: Option<(ViewHeader, i32, Vec<Vram>)>,
    ride_camera: Option<ride::Following>,
    /// How far the fight's picture is moved down in this game frame: the shake of a hit.
    fight_scroll: i32,
    room_shake: i32,
    held: Option<u8>,
    effects: Vec<u16>,
    /// The word of the game's generator while this room is on screen: the behaviour of
    /// the creatures that roam draws from it, and it goes on with what the room hands on
    /// ([`Things::carried`]).
    draws: jc_formats::battle::record::Random,
    second_view: Option<(ViewHeader, usize)>,
    /// The creature that is looked at, while it is.
    form: Option<Form>,
    fight_tints: BTreeMap<Option<usize>, usize>,
    effect: Option<Playing>,
    /// What the tasks of an effect have asked of the sound and nobody has taken yet.
    effect_sounds: Vec<crate::scripts::Heard>,
    effect_half: i16,
    effect_kept: jc_formats::battle::effects::Kept,
    /// The movable light of a fight as the bones of this game frame are lit by it:
    /// what the effect tasks of the game frame before have left, since they stand
    /// behind the player's task and the actors' in the list.
    fight_light: Option<Movable>,
    /// What the last drawings put on screen, for the glide between two game frames
    /// (`smooth.rs`); a new room starts without.
    motion: Motion,
}

/// The creature of "View form": its actor, which stands at the room's origin, the video
/// memory of its model among those of the room, and how many pixels of the window a
/// pixel of the console's screen is where the screen of file 44 is drawn.
struct Form {
    actor: Standing,
    factor: f32,
    drawn: bool,
}

struct Arena {
    package: Package,
    place: Place,
    centre: Centre,
}

impl Things {
    /// The list `shows` of a room with this package, seen through the camera of the view
    /// with this header, `frame` game frames after its start, where it stands when a frame
    /// is given; and the actors of the room, `actors_frame` game frames after they were
    /// made, where they stand when a frame is given. Without a package there is no list.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        package: Option<Package>,
        header: ViewHeader,
        camera: ViewCamera,
        shows: &[Show],
        frame: Option<u32>,
        actors: Vec<Standing>,
        actors_frame: Option<u32>,
        models: Vec<Vram>,
    ) -> Self {
        let mut shows: Vec<(Show, ShowClock, Option<Moved>)> = (shows.iter())
            .map(|show| (*show, ShowClock::default(), None))
            .collect();
        for _ in 0..frame.unwrap_or(0) {
            (shows.iter_mut()).for_each(|(show, clock, _)| clock.step(show));
        }
        let mut vrams = models;
        if vrams.is_empty() {
            vrams.push(Vram::new());
        }
        if let Some(package) = &package {
            for vram in &mut vrams {
                package.textures.iter().for_each(|image| vram.upload(image));
            }
        }
        Self {
            vrams,
            package,
            header,
            camera,
            actors_still: actors_frame.is_some() || actors.is_empty(),
            stalled: false,
            actors,
            player: None,
            player_still: true,
            exit_due: None,
            still: frame.is_some() || shows.is_empty(),
            shows,
            room_light: None,
            materials: Vec::new(),
            clips: None,
            clip_on: None,
            drawn: false,
            told: false,
            scripts: None,
            name: Vec::new(),
            actors_ahead: actors_frame.unwrap_or(0),
            room_models: None,
            view: 0,
            headers: Vec::new(),
            zoom: ZOOM_ONE,
            arena: None,
            fight_drawn: false,
            reach: 0,
            before_fight: None,
            ride_camera: None,
            fight_scroll: 0,
            room_shake: 0,
            held: None,
            effects: Vec::new(),
            draws: jc_formats::battle::record::Random { seed: 0 },
            second_view: None,
            form: None,
            fight_tints: BTreeMap::new(),
            effect: None,
            effect_sounds: Vec::new(),
            effect_half: 0,
            effect_kept: Default::default(),
            fight_light: None,
            motion: Motion::default(),
        }
    }

    /// The room plays the clips of its show lists with these tables of the picture
    /// decoder; a list that stands at one game frame shows the picture of that frame.
    pub fn with_clips(mut self, tables: Option<(CodeTable, PictureDecoder)>) -> Self {
        self.clips = tables;
        self.play_clip();
        self
    }

    fn play_clip(&mut self) {
        let (Some(package), Some((table, decoder))) = (&self.package, &self.clips) else {
            return;
        };
        let playing = self.shows.iter().find_map(|(show, clock, _)| {
            if show.shown() != Some(ShowWay::Clip) {
                return None;
            }
            let clip = package.find(Kind::Clip, u32::try_from(show.number).ok()?)?;
            let frame = clock.frame(show, package.clips[clip].len())?;
            Some((clip, frame, show.mode))
        });
        let Some((clip, frame, mode)) = playing else {
            return;
        };
        if self.clip_on == Some((clip, frame)) {
            return;
        }
        let picture = &package.clips[clip][frame];
        match clip::words(picture, mode as u16 as u32, table, decoder) {
            Ok(words) => {
                for vram in &mut self.vrams {
                    vram.send(clip::AT, clip::SIDE, &words);
                }
                self.clip_on = Some((clip, frame));
                // A page that holds the clip's place is read from video memory again.
                (self.materials).retain(|(skin, _)| !skin.page.is_some_and(holds_the_clip));
            }
            Err(err) => {
                if self.clip_on.is_none() {
                    warn!("picture {frame} of clip {clip} is not shown: {err}");
                }
                self.clip_on = Some((clip, frame));
            }
        }
    }

    /// The room draws from the game's generator where this word has left it.
    pub fn with_draws(mut self, draws: jc_formats::battle::record::Random) -> Self {
        self.draws = draws;
        self
    }

    pub fn with_effects(mut self, half: i16, kept: jc_formats::battle::effects::Kept) -> Self {
        (self.effect_half, self.effect_kept) = (half, kept);
        self
    }

    fn pass_roamers(&mut self) {
        let Some(walker) = &self.player else {
            return;
        };
        let (player, command) = (walker.player.position, walker.player.command);
        let player_turn = walker.player.angle;
        if (self.scripts.as_ref()).is_some_and(|scripts| scripts.fought().is_some()) {
            return;
        }
        let held = self.scripts.as_ref().is_some_and(Scripts::holds_roamers);
        let item =
            (self.scripts.as_ref()).map_or(jc_formats::roamer::item::NONE, Scripts::item_count);
        let let_go = (self.scripts.as_mut()).is_some_and(Scripts::takes_roamers_let_go);
        // The other actors a creature can have ahead: those that are shown.
        let places: Vec<(usize, [i32; 3])> = (self.actors.iter().enumerate())
            .filter(|(_, actor)| !actor.runner.hidden)
            .map(|(number, actor)| (number, actor.mover.position))
            .collect();
        for number in 0..self.actors.len() {
            let actor = &mut self.actors[number];
            let Some(roams) = &mut actor.roams else {
                continue;
            };
            // A step far longer than a walk's is a jump on screen.
            let now = actor.place;
            if let Some(before) = roams.stood.replace(now) {
                let far = (0..3).map(|at| (now[at] - before[at]).abs()).max();
                if far.is_some_and(|far| far > LONGEST_STEP) {
                    info!(
                        "roaming creature, actor {number}, after {} game frames: it jumps from \
                         {before:?} to {now:?}, in state {} and action {} at frame {} of \
                         animation {}, mover at {:?}",
                        actor.game_frame,
                        roams.record.state,
                        roams.record.action,
                        actor.runner.playback.counter,
                        actor.runner.playback.animation,
                        actor.mover.position
                    );
                }
            }
            let mut starts = Vec::new();
            if let_go && let Ok(action) = i16::try_from(roams.record.action) {
                starts.push((action, false, false));
            }
            if let (false, Some(routine)) = (held, roams.routine) {
                let others: Vec<[i32; 3]> = (places.iter())
                    .filter(|(other, _)| *other != number)
                    .map(|(_, place)| *place)
                    .collect();
                let senses = jc_formats::roamer::Senses {
                    arc: &actor.arc,
                    counter: actor.game_frame,
                    player,
                    player_turn,
                    player_runs: command & 0xF_FFFF == jc_formats::player::command::RUN,
                    item,
                    others: &others,
                };
                let before = roams.record.state;
                let passed =
                    (roams.record).pass(routine, &mut actor.mover, &senses, &mut self.draws);
                if roams.record.state != before {
                    info!(
                        "roaming creature, actor {number}, after {} game frames: state {before} \
                         to {} at {:?}, action {}, count {}, turn {} to side {}",
                        actor.game_frame,
                        roams.record.state,
                        actor.mover.position,
                        roams.record.action,
                        roams.record.count,
                        roams.record.left,
                        roams.record.side
                    );
                }
                starts.extend(passed.iter().filter_map(|start| {
                    let from_loop = start.start == jc_formats::roamer::FROM_THE_LOOP;
                    Some((i16::try_from(start.action).ok()?, from_loop, start.fade))
                }));
            }
            for (action, from_loop, fade) in starts {
                if action == -1 {
                    debug!("roaming creature, actor {number}: the start of action -1 is left out");
                    continue;
                }
                let started = (actor.runner).start_now(&actor.actions, action, from_loop, fade);
                let kept = (actor.animations).contains_key(&actor.runner.playback.animation);
                if !started || !kept {
                    warn!(
                        "roaming creature, actor {number}: action {action} cannot be played \
                         (in the model's table: {started}, its animation in memory: {kept})"
                    );
                }
            }
        }
    }

    fn follow_fight(&mut self) {
        self.held = self.scripts.as_ref().and_then(Scripts::held);
        let over = (self.scripts.as_ref()).is_some_and(|scripts| scripts.fought().is_none());
        if self.held.is_some() && !(over && self.before_fight.is_some()) {
            return;
        }
        let starts = self.before_fight.is_none();
        let fought = (self.scripts.as_ref())
            .and_then(Scripts::fought)
            .map(|fought| {
                let place = &fought.place;
                // The next fight of two players may be in another place than the last.
                let moved = (self.arena.as_ref()).is_some_and(|arena| arena.place != place.place);
                let placed =
                    (starts || moved).then(|| (place.package.clone(), place.place.clone()));
                (fought.centre, fought.shot, fought.scroll, placed)
            });
        match fought {
            Some((centre, shot, scroll, placed)) => {
                self.fight_scroll = scroll;
                if let Some((package, place)) = placed {
                    info!(
                        "the fight of a script takes the picture: the place of file {} round \
                         the centre {:?}",
                        place.file, centre.place
                    );
                    if self.before_fight.is_none() {
                        self.before_fight = Some((self.header, self.zoom, self.vrams.clone()));
                    }
                    for vram in &mut self.vrams {
                        package.textures.iter().for_each(|image| vram.upload(image));
                    }
                    self.materials.clear();
                    self.arena = Some(Arena {
                        package,
                        place,
                        centre,
                    });
                }
                let room = (self.before_fight.as_ref()).map_or(self.header, |kept| kept.0);
                self.zoom = ZOOM_ONE;
                self.header = centre.header(&room, &shot);
                self.camera = self.header.view_camera(self.zoom);
                self.drawn = false;
            }
            None => {
                self.fight_scroll = 0;
                if let Some((header, zoom, vrams)) = self.before_fight.take() {
                    info!("the fight of a script is over: the room's own view is back");
                    (self.header, self.zoom, self.vrams) = (header, zoom, vrams);
                    self.clip_on = None;
                    self.camera = self.header.view_camera(self.zoom);
                    self.arena = None;
                    self.materials.clear();
                    self.drawn = false;
                }
            }
        }
    }

    pub fn with_arena(
        mut self,
        package: Package,
        place: Place,
        centre: Centre,
        shot: &Shot,
    ) -> Self {
        for vram in &mut self.vrams {
            package.textures.iter().for_each(|image| vram.upload(image));
        }
        self.zoom = ZOOM_ONE;
        self.header = centre.header(&self.header, shot);
        self.camera = self.header.view_camera(self.zoom);
        self.arena = Some(Arena {
            package,
            place,
            centre,
        });
        self.drawn = false;
        self
    }

    pub fn with_room(mut self, models: Option<MeshBlock>, textures: &[Tim], view: usize) -> Self {
        for vram in &mut self.vrams {
            textures.iter().for_each(|image| vram.upload(image));
        }
        (self.room_models, self.view) = (models, view);
        self
    }

    /// The first game frame of the room, and the game frames before the first drawing.
    /// Without scripts the actors run theirs alone (the player has run its own,
    /// `Walker::run`). With scripts the room runs as a whole: the scripts, the player and
    /// the actors start together and run `frame` game frames, where they stand when a
    /// frame is given.
    pub fn started(mut self, scripts: Option<(Scripts, Option<u32>)>) -> Self {
        let Some((scripts, frame)) = scripts else {
            self.begin_actors();
            for _ in 0..self.actors_ahead {
                self.actors.iter_mut().for_each(Standing::end);
                self.begin_actors();
            }
            return self;
        };
        self.scripts = Some(scripts);
        self.pass_scripts(false);
        self.step_player(Held::default());
        self.begin_actors();
        self.pass_windows();
        (self.still, self.actors_still, self.player_still) = (false, false, false);
        // The buttons of `--pad` and `--cross` count in these game frames as they do in
        // a run.
        for _ in 0..frame.unwrap_or(0) {
            // A room that is stood at a game frame has run its game frames before
            // anything is drawn: a movie of its scripts cannot play in them.
            if frame.is_some()
                && let Some(movie) = self.take_movie()
            {
                info!(
                    "movie {movie} of a script is left out of a room that is stood at a game frame"
                );
            }
            let held = self.player.as_ref().and_then(Walker::held);
            let cross = self.player.as_ref().is_some_and(Walker::cross);
            self.game_frame(held.unwrap_or_default(), cross);
            if let Some(scripts) = &mut self.scripts {
                scripts.voices_ahead();
            }
            // The camera task looks the view up behind every game frame, as `change_view`
            // does in a room that runs: the next game frame starts in the view it found.
            let found = self.looked_view().filter(|(view, _)| *view != self.view);
            let header = found.and_then(|(view, _)| self.headers.get(view).copied().flatten());
            if let (Some((view, followed)), Some(header)) = (found, header) {
                info!(
                    "view {view} in place of view {} before the first drawing: the followed \
                     point {followed:?} lies in its region",
                    self.view
                );
                self.set_view(header, view);
            }
        }
        let still = frame.is_some();
        (self.still, self.actors_still, self.player_still) = (still, still, still);
        // The effects of the game frames that were run ahead are not heard: they would
        // all sound at once.
        self.effects.clear();
        if let Some(walker) = &mut self.player {
            walker.take_effects();
            walker.take_sounds();
        }
        self
    }

    fn pass_scripts(&mut self, confirm: bool) -> usize {
        let mut made = 0;
        if let Some(scripts) = &mut self.scripts {
            let player = self.player.as_mut();
            let distance = self.header.projection_distance;
            let seen = (&self.camera, distance);
            scripts.pass(&mut self.actors, player, confirm, seen, self.view);
            // The exits a script has told to rest are not asked from this game frame on.
            if let Some(walker) = &mut self.player {
                let resting = scripts.resting_exits();
                if walker.resting != resting {
                    walker.resting = resting;
                }
            }
            if scripts.take_cleared() {
                self.shows.clear();
                self.room_light = None;
            }
            let started = scripts.take_shows();
            made = started.len();
            for show in started.iter().filter(|show| show.mover != 0) {
                match scripts.show_mover(show.mover) {
                    Some(mover) => info!(
                        "the mover {:#010x} of a shown thing puts it on {mover:?}",
                        show.mover
                    ),
                    None => warn!(
                        "the mover {:#010x} of a shown thing is not read",
                        show.mover
                    ),
                }
            }
            let started = started.into_iter();
            (self.shows).extend(started.map(|show| (show, ShowClock::default(), None)));
            // A view that a script names is in place in its own game frame: camera and
            // lights here, the pictures by `change_view`.
            let wanted = scripts.take_view().filter(|view| *view != self.view);
            let rides = scripts.ride_camera();
            self.room_shake = scripts.shaken();
            let header = wanted.and_then(|view| self.headers.get(view).copied().flatten());
            let zoom = scripts.zoom();
            if let (Some(view), Some(header)) = (wanted, header) {
                self.set_view(header, view);
            }
            self.follow_fight();
            // The zoom of the game frame is in the projection distance of everything
            // that is drawn from now on; a fight has its own.
            if zoom != self.zoom && self.before_fight.is_none() {
                self.zoom = zoom;
                self.camera = self.header.view_camera(zoom);
                self.drawn = false;
            }
            // The camera's object of a ride's room works on the live header: the point
            // the camera looks at has the player's third coordinate and the projection
            // distance goes with the camera's distance from it. Its start, which runs
            // the step once, is taken in the room's first game frame.
            if rides && let Some(walker) = &self.player {
                let place = walker.player.position;
                match &self.ride_camera {
                    Some(task) => task.pass(&mut self.header, place),
                    None => self.ride_camera = ride::Following::new(&mut self.header, place),
                }
                self.camera = self.header.view_camera(self.zoom);
                self.drawn = false;
            }
        }
        made
    }

    /// The headers of the room's views, by which a script can put another view in place.
    pub fn with_views(mut self, headers: Vec<Option<ViewHeader>>) -> Self {
        self.headers = headers;
        self
    }

    /// The room's second view header and the archive file of its scene package, for the
    /// creature the screen of file 44 shows on "View form".
    pub fn with_second_view(mut self, header: Option<ViewHeader>, package_file: usize) -> Self {
        self.second_view = header.map(|header| (header, package_file));
        self
    }

    pub fn look_at(
        &mut self,
        of: &crate::actors::Figure,
        factor: f32,
    ) -> Result<([i32; 3], String), Box<dyn std::error::Error>> {
        use jc_disc::{Archive, DiscImage, LocalConfig};
        self.end_look();
        let (_, package_file) = (self.second_view).ok_or("the room has no second view")?;
        let config = LocalConfig::load()?;
        let mut disc = DiscImage::open(&config.disc_image)?;
        let archive = Archive::open(&mut disc)?;
        let file = archive.read(&mut disc, package_file)?;
        let package = jc_formats::scene::ScenePackage::parse(&file)?;
        let (line, mut actor, vram) = crate::actors::form(&mut disc, &archive, &package, of)?;
        actor.memory = self.vrams.len();
        actor.runner.hidden = true;
        self.vrams.push(vram);
        let scale = actor.model_scale.unwrap_or([light::ONE; 3]);
        self.form = Some(Form {
            actor,
            factor,
            drawn: false,
        });
        Ok((scale, line))
    }

    /// A pass of the screen of file 44 while the creature is looked at: its turn and its
    /// scale are what the screen has made of them, and its actor's task has its game
    /// frame. HYPOTHESIS: the screen's task is passed in front of the actor's.
    pub fn pass_form(&mut self, turn: i16, scale: [i32; 3]) {
        let Some(form) = &mut self.form else { return };
        let actor = &mut form.actor;
        (actor.mover.turn, actor.angle) = (turn, turn);
        actor.model_scale = Some(scale);
        actor.runner.hidden = false;
        actor.begin();
        actor.end();
        form.drawn = false;
    }

    /// The look at a creature is over: its actor is ended and its model given back.
    pub fn end_look(&mut self) {
        let Some(form) = self.form.take() else { return };
        let memory = form.actor.memory;
        if memory + 1 == self.vrams.len() {
            self.vrams.pop();
        }
        self.materials.retain(|(skin, _)| skin.source != memory);
    }

    /// The triangles of the creature that is looked at, with a line about it: seen
    /// through the room's second view at the room's zoom and lit by that view's lights.
    /// HYPOTHESIS: nothing of a fade of the room is in its colours.
    fn form_triangles(&self) -> Option<(Drawing, String)> {
        let (form, (header, _)) = self.form.as_ref().zip(self.second_view.as_ref())?;
        if form.actor.runner.hidden {
            return Some((
                Drawing::default(),
                "the creature of \"View form\": not made yet".into(),
            ));
        }
        let camera = header.view_camera(self.zoom);
        let mut triangles = Drawing::default();
        let eye = (header, &camera, Fade::default(), None);
        let line = Self::actor_seen(
            "the creature of \"View form\"",
            &form.actor,
            eye,
            &mut triangles,
        );
        Some((triangles, line))
    }

    /// One game frame of the message windows of the scripts, behind the actors.
    fn pass_windows(&mut self) {
        if let Some(scripts) = &mut self.scripts {
            scripts.windows(&self.name);
        }
    }

    /// The player's name, which a text of the room can put in.
    pub fn name_player(&mut self, name: &[u16]) {
        if self.name != name {
            self.name = name.to_vec();
        }
    }

    /// The movie a script of the room has handed the package's movie routine since the
    /// last call. No game frame of the room runs while it plays.
    pub fn take_movie(&mut self) -> Option<u32> {
        self.scripts.as_mut().and_then(Scripts::take_movie)
    }

    pub fn asks_menu(&self) -> Option<u32> {
        let scripts = self.scripts.as_ref().filter(|_| !self.actors_still)?;
        scripts.asks_menu().then(|| scripts.game_frames())
    }

    /// Whether a menu the results screen opened may use a thing of the story
    /// ([`Scripts::frees_things`]).
    pub fn frees_things(&self) -> bool {
        self.scripts.as_ref().is_none_or(Scripts::frees_things)
    }

    /// The things of the story a marked object of the room waits for
    /// ([`Scripts::waited_things`]).
    pub fn waited_things(&self) -> Vec<i16> {
        (self.scripts.as_ref()).map_or_else(Vec::new, Scripts::waited_things)
    }

    /// The menu has used a thing of the story here ([`Scripts::use_story_thing`]).
    pub fn use_story_thing(&mut self, thing: i16) {
        if let Some(scripts) = &mut self.scripts {
            scripts.use_story_thing(thing);
        }
    }

    /// Whether a script of the room has asked for the place map since the last call.
    pub fn take_map(&mut self) -> bool {
        self.scripts.as_mut().is_some_and(Scripts::take_map)
    }

    /// The screen of field mode 5 a script of the room has asked for since the last
    /// call: its kind and its argument.
    pub fn take_screen(&mut self) -> Option<(u32, u32)> {
        self.scripts.as_mut().and_then(Scripts::take_screen)
    }

    /// The text of the place, when the object of the memory card has asked for the
    /// memory card's screen and the field's task takes the request
    /// ([`Scripts::take_card`]).
    pub fn take_card(&mut self) -> Option<[u8; 24]> {
        self.scripts.as_mut().and_then(Scripts::take_card)
    }

    /// The text of the place, when the last script of the story has asked for the save
    /// behind its end ([`Scripts::take_end_card`]).
    pub fn take_end_card(&mut self) -> Option<[u8; 24]> {
        self.scripts.as_mut().and_then(Scripts::take_end_card)
    }

    /// Whether the hero walks freely in the room, for the build's quick save and
    /// autosave ([`Scripts::held_by`]): the room's scripts run and say so, the
    /// actors and the player are passed and the picture is not held.
    pub fn walks_freely(&self) -> bool {
        self.held_by().is_none()
    }

    /// What a press of the confirm button would do while the hero walks freely
    /// ([`Scripts::confirm_takes`]), for the build's interaction hint (`hint.rs`); none
    /// in a scene, a fight, a fade or while a window stands.
    pub fn confirm_takes(&self) -> Option<crate::scripts::Takes> {
        if !self.walks_freely() {
            return None;
        }
        self.scripts.as_ref()?.confirm_takes()
    }

    /// Where the top of the hero is seen as he was last drawn, in pixels right of and
    /// below the projection centre: the middle of his picture across, its highest
    /// point, `share` of the way from the game frame before when he glides
    /// (`smooth.rs`). None without a hero or while a script hides him.
    pub fn hero_top(&self, share: f32) -> Option<Vec2> {
        let walker = self.player.as_ref()?;
        if walker.player.flags & player::flag::HIDDEN != 0 || walker.body.runner.hidden {
            return None;
        }
        let mut triangles = Drawing::default();
        self.actor("player", &walker.body, &mut triangles);
        let corners = triangles.iter().flat_map(|triangle| triangle.corners);
        let (low, high) = corners.fold(
            (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)),
            |(low, high), corner| (low.min(corner.at), high.max(corner.at)),
        );
        // Between two game frames the hero is drawn a share of the way (`smooth.rs`).
        let glide = self.motion.shift(thing::PLAYER, share);
        (low.x <= high.x).then(|| Vec2::new((low.x + high.x) / 2.0, low.y) + glide)
    }

    /// Whether the room's scripts hold a fight of the story ([`Scripts::holds_story_fight`]).
    pub fn holds_story_fight(&self) -> bool {
        self.scripts
            .as_ref()
            .is_some_and(Scripts::holds_story_fight)
    }

    /// The running script that still leads to a fight of the story
    /// ([`Scripts::story_fight_ahead`]).
    pub fn story_fight_ahead(&self) -> Option<i32> {
        self.scripts.as_ref()?.story_fight_ahead()
    }

    /// What keeps the hero from walking freely, for the log.
    pub fn held_by(&self) -> Option<&'static str> {
        let Some(scripts) = self.scripts.as_ref().filter(|_| !self.actors_still) else {
            return Some("the room's scripts do not run");
        };
        if self.player.is_none() || self.player_still {
            return Some("the room has no hero who walks");
        }
        if self.held.is_some() {
            return Some("the picture is held");
        }
        scripts.held_by()
    }

    /// The script that holds the game while the room's scripts run
    /// ([`Scripts::holder`]), for the build's skip of a scene (`skip.rs`).
    pub fn holder(&self) -> Option<i32> {
        let scripts = self.scripts.as_ref().filter(|_| !self.actors_still)?;
        scripts.holder()
    }

    /// Whether a question of a script stands up ([`Scripts::asks`]).
    pub fn asks(&self) -> bool {
        self.scripts.as_ref().is_some_and(Scripts::asks)
    }

    /// The groups of the room's text the windows show now ([`Scripts::said_groups`]).
    pub fn said_groups(&self) -> Vec<usize> {
        (self.scripts.as_ref()).map_or_else(Vec::new, Scripts::said_groups)
    }

    /// The build skips the scene, or no longer ([`Scripts::set_skipping`]).
    pub fn set_skipping(&mut self, skipping: bool) {
        if let Some(scripts) = self.scripts.as_mut() {
            scripts.set_skipping(skipping);
        }
    }

    /// Whether the room's scripts are told that the build skips the scene.
    pub fn skipping(&self) -> bool {
        self.scripts.as_ref().is_some_and(Scripts::skipping)
    }

    /// The text of the place of the room's object of the memory card, when it has one
    /// ([`Scripts::place_text`]).
    pub fn place_text(&self) -> Option<[u8; 24]> {
        self.scripts.as_ref().and_then(Scripts::place_text)
    }

    /// The screen a script of the room asked for has left.
    pub fn screen_left(&mut self) {
        if let Some(scripts) = self.scripts.as_mut() {
            scripts.screen_left();
        }
    }

    /// The screen of file 44 answers the script of the room and sleeps.
    pub fn screen_answered(&mut self, answer: i16) {
        if let Some(scripts) = self.scripts.as_mut() {
            scripts.screen_answered(answer);
        }
    }

    /// The screen of file 44 has left: the player's creatures are as it left them.
    pub fn listed(&mut self, game: &Carried) {
        if let Some(scripts) = self.scripts.as_mut() {
            scripts.listed(game);
        }
    }

    /// The task of the results screen that a fight of the room has opened since the
    /// last call.
    pub fn take_results(&mut self) -> Option<jc_formats::battle::results::Results> {
        self.scripts.as_mut().and_then(Scripts::take_results)
    }

    /// The results screen could not be put up: what it would give is given at once.
    pub fn results_without_a_screen(&mut self, results: jc_formats::battle::results::Results) {
        if let Some(scripts) = self.scripts.as_mut() {
            scripts.results_without_a_screen(results);
        }
    }

    /// Nobody is at the pad of the room's scripts (`--confirms`).
    pub fn nobody_at_the_pad(&self) -> bool {
        (self.scripts.as_ref()).is_some_and(Scripts::nobody_at_the_pad)
    }

    /// What the voices answer, for the windows of the room's scripts
    /// ([`Scripts::hear_voices`]).
    pub fn hear_voices(&mut self, silent: bool, none_asked: bool) {
        if let Some(scripts) = self.scripts.as_mut() {
            scripts.hear_voices(silent, none_asked);
        }
    }

    /// What the scripts of the room have asked of the music and the sounds since the
    /// last call, and behind it the effects of the animation events of the actors and
    /// of the player.
    pub fn take_music(&mut self) -> Vec<crate::scripts::Heard> {
        let mut heard = (self.scripts.as_mut()).map_or_else(Vec::new, Scripts::take_music);
        let of_player = (self.player.as_mut()).map_or_else(Vec::new, Walker::take_effects);
        // Without scripts nothing has changed the record of a new game.
        let carried = self.scripts.as_ref().map(Scripts::carried);
        let level = carried.map_or(LEVEL_AT_THE_START, Carried::player_level);
        let effects = self.effects.drain(..).chain(of_player);
        heard.extend(effects.map(|number| crate::scripts::Heard::Effect { number, level }));
        // What the player's states of a fight ask for: the sound of an item, and the
        // row of the fight's sequences that is started in front of it.
        let signs = (self.player.as_mut()).map_or_else(Vec::new, Walker::take_sounds);
        use jc_formats::player::arms::Sign;
        heard.extend(signs.into_iter().filter_map(|sign| match sign {
            Sign::Sound(sound) => Some(crate::scripts::Heard::Sound(sound)),
            Sign::BeforeItem => Some(crate::scripts::Heard::Fight(
                jc_formats::sound::FightAsks::Start(jc_formats::sound::ITEM_ROW),
            )),
            _ => None,
        }));
        heard.append(&mut self.effect_sounds);
        heard
    }

    /// The room number a script of the room has sent the game to since the last call.
    pub fn take_room(&mut self) -> Option<u32> {
        self.scripts.as_mut().and_then(Scripts::take_room)
    }

    /// The chapter whose screen a script of the room has handed the game to since the
    /// last call.
    pub fn take_chapter(&mut self) -> Option<u32> {
        self.scripts.as_mut().and_then(Scripts::take_chapter)
    }

    /// The slot a script of the room has sent the game to since the last call: area,
    /// place, slot and the address of an arrival place.
    pub fn take_slot(&mut self) -> Option<(i32, u16, u16, Option<u32>)> {
        self.scripts.as_mut().and_then(Scripts::take_slot)
    }

    /// What the generator of the corridor has asked since the last call
    /// ([`Scripts::take_corridor`]).
    pub fn take_corridor(&mut self) -> Option<crate::scripts::CorridorAsks> {
        self.scripts.as_mut().and_then(Scripts::take_corridor)
    }

    /// The last picture of the story while it is on screen ([`Scripts::ending`]).
    pub fn ending(&self) -> Option<(usize, u8)> {
        self.scripts.as_ref().and_then(Scripts::ending)
    }

    /// Whether a script has asked for the same room once more since the last call.
    pub fn take_again(&mut self) -> bool {
        self.scripts.as_mut().is_some_and(Scripts::take_again)
    }

    /// The number of the view that is shown.
    pub fn view_shown(&self) -> usize {
        self.view
    }

    /// The player's place and the angle he faces; nothing of a room without him.
    pub fn stands(&self) -> Option<(jc_formats::field::Position, i16)> {
        (self.player.as_ref()).map(|walker| (walker.player.position, walker.player.angle))
    }

    /// What the game carries on as the room's scripts have left it; nothing of a room
    /// whose scripts do not run.
    pub fn carried(&self) -> Option<Carried> {
        (self.scripts.as_ref()).map(|scripts| Carried {
            random: self.draws,
            effect_half: self.effect_half,
            effect_kept: self.effect_kept,
            ..scripts.carried().clone()
        })
    }

    /// The floor of the corridor of the second round and the node the player is in,
    /// while its generator lives; nothing elsewhere.
    pub fn corridor_at(&self) -> Option<(u32, usize)> {
        let scripts = self.scripts.as_ref()?;
        (scripts.carried().corridor.as_ref()).map(|corridor| (corridor.floor, corridor.at))
    }

    /// The game's setting word as the room's scripts have it; nothing of a room whose
    /// scripts do not run, where nobody reads it.
    pub fn setting(&self) -> Option<u32> {
        self.scripts.as_ref().map(Scripts::setting)
    }

    /// The app hands the room's scripts the setting word.
    pub fn set_setting(&mut self, word: u32) {
        if let Some(scripts) = self.scripts.as_mut() {
            scripts.set_setting(word);
        }
    }

    pub fn kept(
        &mut self,
        keeping: jc_formats::items::Keeping,
        player: Option<jc_formats::battle::record::Record>,
        worn: jc_formats::items::Worn,
        creatures: Option<Vec<jc_formats::battle::record::Record>>,
    ) {
        if let Some(scripts) = self.scripts.as_mut() {
            scripts.kept(keeping, player, worn, creatures);
        }
        self.wear(worn);
    }

    pub fn wear(&mut self, worn: jc_formats::items::Worn) {
        let worn = jc_formats::model::Worn::of(worn);
        let Some(walker) = (self.player.as_mut()).filter(|walker| walker.worn() != worn) else {
            return;
        };
        let read = || -> Result<_, Box<dyn std::error::Error>> {
            let config = jc_disc::LocalConfig::load()?;
            let mut disc = jc_disc::DiscImage::open(&config.disc_image)?;
            let archive = jc_disc::Archive::open(&mut disc)?;
            Ok((disc, archive))
        };
        let line = read().and_then(|(mut disc, archive)| walker.wear(&mut disc, &archive, worn));
        match line {
            Ok(line) => {
                let kept = self.before_fight.iter_mut().flat_map(|kept| &mut kept.2);
                for vram in self.vrams.iter_mut().chain(kept) {
                    walker.images.iter().for_each(|image| vram.upload(image));
                }
                self.drawn = false;
                bevy::log::info!("the player's model is read again: {line}");
            }
            Err(error) => bevy::log::warn!(
                "the player's model stays as it was, reading it for {worn:?} failed: {error}"
            ),
        }
    }

    /// The count of the two items that make the creatures rest or come, as the menu
    /// has left it.
    pub fn count_kept(&mut self, count: jc_formats::roamer::ItemCount) {
        if let Some(scripts) = self.scripts.as_mut() {
            scripts.count_kept(count);
        }
    }

    pub fn leave_forest(
        &mut self,
        trees: [i32; jc_formats::leave_forest::VIEWS],
        from: jc_formats::leave_forest::From,
    ) -> bool {
        let Some(scripts) = self.scripts.as_mut() else {
            return false;
        };
        scripts.leave_forest(trees, from);
        true
    }

    /// The message window of the room's scripts that is up.
    pub fn spoken(&self) -> Option<Spoken<'_>> {
        self.scripts.as_ref().and_then(Scripts::spoken)
    }

    /// Whether the command menu of the room's fight waits for the player
    /// ([`crate::scripts::Scripts::fight_menu_waits`]).
    pub fn fight_menu_waits(&self) -> bool {
        self.scripts.as_ref().is_some_and(Scripts::fight_menu_waits)
    }

    /// The command menu of the room's fight while it waits for the player, and the
    /// game frame of its last pass.
    pub fn fight_menu(&self) -> Option<crate::scripts::FightMenu<'_>> {
        self.scripts.as_ref().and_then(Scripts::fight_menu)
    }

    /// A fight of two players comes on screen in the room's next game frame
    /// (`scripts/versus.rs`), in place of the one that is there.
    pub fn start_versus(&mut self, setup: crate::scripts::VersusSetup) {
        if let Some(scripts) = &mut self.scripts {
            scripts.start_versus(setup);
        }
    }

    /// The room is the arena of a fight of two players: its scripts stop. False while
    /// the room has no scripts.
    pub fn for_versus(&mut self) -> bool {
        let scripts = self.scripts.as_mut();
        scripts.map(Scripts::for_versus).is_some()
    }

    /// Whether a fight of two players is on screen or about to come.
    pub fn versus_on(&self) -> bool {
        self.scripts.as_ref().is_some_and(Scripts::versus_on)
    }

    /// The duel as the session left it, what it waits for and the tally.
    pub fn versus_update(
        &mut self,
        duel: &jc_formats::battle::duel::Duel,
        step: Option<jc_formats::battle::duel::Step>,
        series: jc_formats::versus::Series,
    ) {
        if let Some(scripts) = &mut self.scripts {
            scripts.versus_update(duel, step, series);
        }
    }

    /// The command the local menu of a fight of two players handed over.
    pub fn take_versus_choice(&mut self) -> Option<jc_formats::battle::duel::Choice> {
        self.scripts.as_mut()?.take_versus_choice()
    }

    /// The session did not take that command: the menu is made again.
    pub fn versus_refused(&mut self) {
        if let Some(scripts) = &mut self.scripts {
            scripts.versus_refused();
        }
    }

    /// The answer to "Rematch?", once given.
    pub fn take_versus_rematch(&mut self) -> Option<bool> {
        self.scripts.as_mut()?.take_versus_rematch()
    }

    /// The game frames the local menu of a fight of two players has been up for, and
    /// those the end of the fight has waited for its answer.
    pub fn versus_menu_frames(&self) -> Option<u32> {
        self.scripts.as_ref()?.versus_menu_frames()
    }

    pub fn versus_over_frames(&self) -> Option<u32> {
        self.scripts.as_ref()?.versus_over_frames()
    }

    /// The moments of a fight of two players a check takes pictures of.
    pub fn take_versus_moments(&mut self) -> Vec<crate::scripts::VersusMoment> {
        (self.scripts.as_mut()).map_or_else(Vec::new, Scripts::take_versus_moments)
    }

    /// What the build draws over a fight of two players.
    pub fn versus_shown(&self) -> Option<crate::scripts::VersusShown> {
        self.scripts.as_ref()?.versus_shown()
    }

    /// Once a frame: the mouse over a spot of those boxes, while they have any.
    pub fn versus_point(
        &mut self,
        mouse: &crate::pointer::Mouse,
        under: Option<crate::scripts::VersusSpot>,
        aimed: bool,
    ) {
        if let Some(scripts) = &mut self.scripts {
            scripts.versus_point(mouse, under, aimed);
        }
    }

    /// Where the opponents of the fight on screen stand on the console's screen, for a
    /// pointer of the build (the game has none; `pointer.rs`): for each slot of an
    /// opponent that an actor stands for, the rectangle round the joints of its pose as
    /// the fight's camera projects them, from the projection centre in the middle of
    /// the screen, a little larger. In a wide fight it may lie beyond the console's 320
    /// columns. Empty outside a fight.
    pub fn opponents_on_screen(&self) -> Vec<(usize, jc_formats::menu::Rect)> {
        use jc_formats::battle::fight::{OPPONENTS, SLOTS};
        /// Pixels the rectangle reaches beyond the outermost joints.
        const AROUND: f64 = 8.0;
        let Some(scripts) = self.scripts.as_ref().filter(|_| self.arena.is_some()) else {
            return Vec::new();
        };
        let centre = [
            f64::from(SCREEN.0 / 2),
            f64::from(SCREEN.1 / 2 + self.fight_scroll),
        ];
        (OPPONENTS..SLOTS)
            .filter_map(|slot| {
                let actor = self.actors.get(scripts.stands_for(slot)?)?;
                let joints = (actor.pose.iter().flatten())
                    .map(|placement| {
                        std::array::from_fn(|k| actor.place[k] + placement.position[k])
                    })
                    .chain([actor.place]);
                let seen: Vec<[f64; 2]> = joints.filter_map(|at| self.camera.project(at)).collect();
                let edge = |axis: usize, most: bool| {
                    let values = seen.iter().map(|at| at[axis] + centre[axis]);
                    match most {
                        true => values.fold(f64::MIN, f64::max) + AROUND,
                        false => values.fold(f64::MAX, f64::min) - AROUND,
                    }
                };
                let (left, top) = (edge(0, false), edge(1, false));
                let (right, bottom) = (edge(0, true), edge(1, true));
                let fits = |value: f64| (-1024.0..1024.0).contains(&value);
                if seen.is_empty() || ![left, top, right, bottom].into_iter().all(fits) {
                    return None;
                }
                let rect = jc_formats::menu::Rect {
                    x: left.floor() as i16,
                    y: top.floor() as i16,
                    w: (right - left).ceil() as i16,
                    h: (bottom - top).ceil() as i16,
                };
                Some((slot, rect))
            })
            .collect()
    }

    /// The mouse over a spot of the menu of the room's fight (`pointer.rs`).
    pub fn point_at_fight(
        &mut self,
        mouse: &crate::pointer::Mouse,
        under: Option<jc_formats::battle::menu_panel::Spot>,
    ) {
        if let Some(scripts) = &mut self.scripts {
            scripts.point_at_fight(mouse, under);
        }
    }

    /// The notice of the room's scripts that is up.
    pub fn noticed(&self) -> Option<&Noticed> {
        self.scripts.as_ref().and_then(Scripts::noticed)
    }

    /// One game frame of the player: its order states, of which a script is told when
    /// they say that a move is over, then the controller with the buttons that are held,
    /// or with none while a script has the player.
    fn step_player(&mut self, held: Held) {
        let Some(walker) = &mut self.player else {
            return;
        };
        let held = match &mut self.scripts {
            Some(scripts) => {
                // A fight of a script: the player's own states of a fight run with it.
                walker.fight(scripts.fought().map(|fought| fought.camera_state));
                let arc = walker.body.arc.clone();
                let before = scripts.orders().state;
                if scripts.orders().pass(&mut walker.player, &arc) {
                    scripts.told(0);
                }
                // The order states of a fight (1 to 8), for the log.
                walker.note_order_state(before, scripts.orders().state);
                walker.ordered = scripts.orders().command;
                // A press of the confirm button that a waiting script takes for a talk
                // leaves the pad's command without a movement in that game frame.
                let talks = scripts.talk();
                match scripts.player_is_free() && !talks {
                    true => held,
                    false => {
                        walker.stick = None;
                        Held::default()
                    }
                }
            }
            None => held,
        };
        walker.step(held);
    }

    fn draws(&self, actor: &Standing) -> bool {
        !actor.runner.hidden && self.camera.within_limits(actor.place)
    }

    /// The animation events of the actors, for the frame each was drawn in: behind that
    /// drawing and before the step that ends its game frame.
    fn fire_events(&mut self) {
        for number in 0..self.actors.len() {
            if !self.draws(&self.actors[number]) {
                continue;
            }
            let actor = &mut self.actors[number];
            let (animation, counter) = (
                actor.runner.playback.animation,
                actor.runner.playback.counter,
            );
            let (swaps, effects) = actor.fire_events();
            let after = actor.game_frame;
            for effect in &effects {
                debug!(
                    "actor {number} after {after} game frames: an event of animation \
                     {animation} at counter {counter} asks for effect {effect}"
                );
            }
            self.effects.extend(effects);
            for (object, source) in swaps {
                info!(
                    "actor {number} after {} game frames: an event of animation {animation} \
                     at counter {counter} gives mesh object {object} the mesh of object \
                     {source}",
                    after
                );
            }
        }
    }

    /// The actors a fight has put away: their tasks are held, so they have no game frame
    /// ([`Scripts::put_away`]).
    fn put_away(&self) -> Vec<usize> {
        (self.scripts.as_ref()).map_or_else(Vec::new, |scripts| scripts.put_away().to_vec())
    }

    /// The start of a game frame for every actor; a script is told of a move that came to
    /// its end in the pass.
    fn begin_actors(&mut self) {
        let away = self.put_away();
        for (slot, actor) in self.actors.iter_mut().enumerate() {
            if away.contains(&slot) {
                continue;
            }
            actor.begin();
            if let Some(scripts) = &mut self.scripts
                && actor.runner.take_over()
            {
                scripts.told(slot + 1);
            }
        }
    }

    fn game_frame(&mut self, held: Held, confirm: bool) {
        // A movie that a script asked for has not been taken yet: it plays first.
        if self.scripts.as_ref().is_some_and(Scripts::asks_movie) {
            return;
        }
        let in_the_way = in_the_way(&self.actors);
        let mut made = 0;
        if !self.actors_still {
            // An actor that was not passed in the game frame before has no game frame
            // to end.
            if !self.stalled {
                self.fire_events();
                let away = self.put_away();
                for (number, actor) in self.actors.iter_mut().enumerate() {
                    if !away.contains(&number) {
                        actor.end();
                    }
                }
            }
            made = self.pass_scripts(confirm);
        }
        self.tint_fighters();
        let rested = self.scripts.as_ref().is_some_and(Scripts::rested);
        // The task of the two items' count is one of those (place 25).
        if let (false, Some(scripts)) = (rested, self.scripts.as_mut()) {
            scripts.pass_item_count();
        }
        if !self.still && !rested {
            let before = self.shows.len().saturating_sub(made);
            (self.shows[..before].iter_mut()).for_each(|(show, clock, _)| clock.step(show));
        }
        if !rested {
            self.move_shows();
            self.play_clip();
        }
        self.stalled = self.scripts.as_ref().is_some_and(Scripts::stalled);
        if self.stalled {
            self.drawn = false;
            return;
        }
        if self.scripts.as_ref().is_some_and(Scripts::asks_movie) {
            self.drawn = false;
            return;
        }
        self.exit_due = match (self.objects_rest(), self.player_still) {
            (false, false) => self.player.as_mut().and_then(Walker::leaves),
            _ => None,
        };
        if !self.player_still {
            if let Some(walker) = &mut self.player {
                walker.things = in_the_way;
            }
            self.step_player(held);
        }
        if !self.actors_still {
            self.pass_roamers();
            self.begin_actors();
            self.pass_windows();
        }
        self.pass_effects();
        let lives = self.effect.is_some();
        if let Some(scripts) = &mut self.scripts {
            scripts.effect_lives(lives);
        }
        self.drawn = false;
    }

    fn pass_effects(&mut self) {
        const HELD_BY_AN_EFFECT: u32 = 0xFFFF_0000;
        // The actors the runner of the fight has let go from their hold in this game
        // frame start the effect their request named (`0x8001E2F4`).
        let let_go = (self.scripts.as_mut()).map_or_else(Vec::new, Scripts::take_effects);
        let Some(walker) = &mut self.player else {
            return;
        };
        let mut started = None;
        if let Some(sign) = walker.take_started() {
            let (scripts, half) = (self.scripts.as_ref(), &mut self.effect_half);
            let kept = self.effect_kept;
            match Playing::start(sign, walker, &self.actors, scripts, half, kept) {
                Ok((playing, line)) => {
                    info!("player after {} game frames: {line}", walker.game_frames());
                    started = Some(playing);
                }
                Err(err) => warn!("the effect of the player's action is not shown: {err}"),
            }
        }
        for asked in let_go {
            let (scripts, half) = (self.scripts.as_ref(), &mut self.effect_half);
            let kept = self.effect_kept;
            match Playing::start_actor(asked, walker, &self.actors, scripts, half, kept) {
                Ok((playing, line)) => {
                    info!("{line}");
                    started = Some(playing);
                }
                Err(err) => warn!(
                    "the effect {} of thing {} is not shown: {err}",
                    asked.number,
                    asked.actor + 1
                ),
            }
        }
        if let Some(playing) = started {
            for vram in &mut self.vrams {
                (playing.package.textures.iter()).for_each(|image| vram.upload(image));
            }
            self.materials.clear();
            // The starter clears the half word that ends the loops of a capture.
            walker.player.fight.loops_end = false;
            self.effect = Some(playing);
        }
        // The tear-down of a fight ends the pools of its list.
        if self.arena.is_none() {
            self.effect = None;
        }
        let Some(playing) = &mut self.effect else {
            self.fight_light = None;
            return;
        };
        // The bones of this game frame were lit before the effect's tasks are stepped.
        let lit = playing.effects.light();
        if lit.is_some() != self.fight_light.is_some() {
            match lit {
                Some(light) => info!(
                    "effect {} in its game frame {}: the movable light of the fight is on at \
                     {:?}, colour {:?}",
                    playing.number,
                    playing.passed(),
                    light.place,
                    light.colour
                ),
                None => info!(
                    "effect {} in its game frame {}: the movable light of the fight is out",
                    playing.number,
                    playing.passed()
                ),
            }
        }
        self.fight_light = lit;
        let loops_end = walker.player.fight.loops_end;
        let asked = playing.pass(&walker.body, &self.actors, &mut self.draws, loops_end);
        self.effect_kept = playing.effects.kept();
        for asks in asked {
            use crate::scripts::Heard;
            use jc_formats::battle::effects::Asks;
            match asks {
                Asks::Sound(number) => self.effect_sounds.push(Heard::Sound(number)),
                Asks::Row(row) => {
                    self.effect_sounds
                        .push(Heard::Fight(jc_formats::sound::FightAsks::Start(
                            row as usize,
                        )))
                }
                Asks::RowStop(row) => {
                    self.effect_sounds
                        .push(Heard::Fight(jc_formats::sound::FightAsks::Stop(
                            row as usize,
                        )))
                }
                Asks::Clip(_) => {}
                Asks::Way(number) => {
                    self.effect_sounds
                        .push(Heard::Fight(jc_formats::sound::FightAsks::Music(
                            number as usize,
                        )))
                }
                Asks::WayEnd(number) => {
                    self.effect_sounds
                        .push(Heard::Music(jc_formats::script::Music::Other {
                            code: 0xFF03,
                            c: number,
                        }))
                }
                Asks::SoundHeld(number) => self.effect_sounds.push(Heard::Kept {
                    at: HELD_BY_AN_EFFECT | number,
                    sound: number,
                }),
                Asks::SoundStop(number) => self.effect_sounds.push(Heard::Ended {
                    at: HELD_BY_AN_EFFECT | number,
                }),
            }
        }
        // The clip of the effect's package goes where a room's clip does, in the mode
        // its player names: the faces and sprites of the package that have their
        // texture there show it.
        if let Some(picture) = playing.clip_picture()
            && let Some((table, decoder)) = &self.clips
        {
            let mode = jc_formats::battle::effects::CLIP_MODE;
            match clip::words(picture, mode, table, decoder) {
                Ok(words) => {
                    for vram in &mut self.vrams {
                        vram.send(clip::AT, clip::SIDE, &words);
                    }
                    self.clip_on = None;
                    (self.materials).retain(|(skin, _)| !skin.page.is_some_and(holds_the_clip));
                }
                Err(err) => debug!("a picture of the clip of effect {}: {err}", playing.number),
            }
        }
        if playing.effects.is_over() && playing.effects.drawn().is_empty() {
            info!(
                "effect {} is over after {} game frames: its package is let go",
                playing.number,
                playing.passed()
            );
            self.effect = None;
        }
        // A line about what is drawn of it, every 30th game frame; the check option
        // `--effect-log=N` asks for every Nth, to find the game frame of a short piece.
        static EVERY: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
        let every = *EVERY.get_or_init(|| {
            std::env::args()
                .find_map(|arg| arg.strip_prefix("--effect-log=")?.parse().ok())
                .filter(|&every| every > 0)
                .unwrap_or(30)
        });
        if (self.effect.as_ref()).is_some_and(|playing| playing.passed() % every == 0)
            && let Some(line) = self.effect_of_the_fight(&mut Drawing::default())
        {
            info!("{line}");
        }
    }

    fn effect_of_the_fight(&self, triangles: &mut Drawing) -> Option<String> {
        use jc_formats::battle::effects::{self, Drawn, SPRITE_BLEND};
        let playing = self.effect.as_ref()?;
        let package = &playing.package;
        let blend = (SPRITE_BLEND & SPRITE_BLENDS != 0)
            .then_some((SPRITE_BLEND >> SPRITE_BLEND_MODE) as u8 & 3);
        let (mut sprites, mut models, mut shapes, mut lights) = (0, 0, 0, 0);
        let (mut pieces, mut parts) = (0, Count::default());
        let (mut quads, mut faces) = (Count::default(), Count::default());
        let (mut shells, mut glows) = (Count::default(), Count::default());
        let (mut ribbons, mut seams) = (0, Count::default());
        // Every thing an effect draws is a thing of its own, known from one game frame to
        // the next by its kind and its place (`smooth::tracked`).
        let mut kinds: BTreeMap<u32, usize> = BTreeMap::new();
        for drawn in playing.effects.drawn() {
            let kind = effect_kind(drawn);
            let nth = kinds.entry(kind).or_default();
            triangles.thing(smooth::tracked(kind, *nth));
            *nth += 1;
            match *drawn {
                Drawn::Sprite {
                    sprite,
                    frame,
                    place,
                    angles,
                } => {
                    sprites += 1;
                    self.quads(
                        (&package.sprites[sprite], frame),
                        (place, angles),
                        (false, blend, false),
                        &mut quads,
                        triangles,
                    );
                }
                Drawn::Model {
                    model,
                    frame,
                    place,
                    turn,
                } => {
                    models += 1;
                    let model = &package.models[model];
                    let (sine, cosine) = (
                        i64::from(skeleton::sine(i32::from(turn))),
                        i64::from(skeleton::cosine(i32::from(turn))),
                    );
                    // The seam vertices of a placement's object are kept under the
                    // number of the placement, which is what a stitch quad names.
                    let mut stood = Vec::new();
                    for mut piece in model.frame(frame) {
                        let index = (piece.object).filter(|&o| o < model.mesh.objects.len());
                        let Some(index) = index else {
                            stood.push(None);
                            continue;
                        };
                        let [x, _, z] = piece.place.map(i64::from);
                        piece.place[0] = (x * cosine / 4096 + z * sine / 4096) as i32;
                        piece.place[2] = (z * cosine / 4096 - x * sine / 4096) as i32;
                        piece.angles[1] = piece.angles[1].wrapping_add(turn);
                        let stand = piece.stands(place, [0; 3]);
                        self.placed(
                            &model.mesh.objects[index],
                            (&piece, &stand),
                            (None, false),
                            &mut faces,
                            triangles,
                        );
                        stood.push(Some((index, piece, stand)));
                    }
                    self.stitched(&model.mesh, &stood, &mut seams, triangles);
                }
                Drawn::Shape {
                    model,
                    share,
                    place,
                    scale,
                    whole,
                } => {
                    shapes += 1;
                    let [first, second, ..] = package.models[model].mesh.objects.as_slice() else {
                        continue;
                    };
                    let mut mixed = first.clone();
                    for (face, other) in mixed.faces.iter_mut().zip(&second.faces) {
                        for (corner, from) in face.corners.iter_mut().zip(&other.corners) {
                            corner.position = effects::mix(corner.position, from.position, share);
                        }
                    }
                    let piece = Piece {
                        object: Some(0),
                        brightness: share as i16,
                        place: [0; 3],
                        angles: [0; 3],
                        scale: match whole {
                            true => effects::shell_scale(scale),
                            false => [effects::SHARE_ONE; 3],
                        },
                        flags: effects::SHELL_FLAGS,
                    };
                    let stand = piece.stands(place, [0; 3]);
                    self.placed(
                        &mixed,
                        (&piece, &stand),
                        (None, false),
                        &mut shells,
                        triangles,
                    );
                }
                Drawn::Ribbon {
                    model,
                    points,
                    angles,
                } => {
                    ribbons += 1;
                    let block = &package.models[model].mesh;
                    let stood: Vec<_> = (points.iter().enumerate())
                        .map(|(index, &place)| {
                            let piece = Piece {
                                object: Some(index),
                                brightness: effects::SHARE_ONE as i16,
                                place: [0; 3],
                                angles: [0; 3],
                                scale: [effects::SHARE_ONE; 3],
                                flags: effects::PIECE_FLAGS,
                            };
                            let stand = piece.stands(place, angles);
                            (index < block.objects.len()).then_some((index, piece, stand))
                        })
                        .collect();
                    self.stitched(block, &stood, &mut seams, triangles);
                }
                Drawn::Mixed {
                    model,
                    share,
                    place,
                    angles,
                    scale,
                    fade,
                } => {
                    shapes += 1;
                    let [first, second, ..] = package.models[model].mesh.objects.as_slice() else {
                        continue;
                    };
                    let mut mixed = first.clone();
                    for (face, other) in mixed.faces.iter_mut().zip(&second.faces) {
                        for (corner, from) in face.corners.iter_mut().zip(&other.corners) {
                            corner.position = effects::mix(corner.position, from.position, share);
                        }
                    }
                    let piece = Piece {
                        object: Some(0),
                        brightness: (effects::SHARE_ONE - fade) as i16,
                        place: [0; 3],
                        angles: [0; 3],
                        scale: [scale; 3],
                        flags: MIXED_FLAGS | if fade > 0 { FADED } else { 0 },
                    };
                    let stand = piece.stands(place, angles);
                    self.placed(
                        &mixed,
                        (&piece, &stand),
                        (None, false),
                        &mut shells,
                        triangles,
                    );
                }
                Drawn::Thrown {
                    sprite,
                    frame,
                    place,
                    angles,
                    offset,
                } => {
                    sprites += 1;
                    let sprite = &package.sprites[sprite];
                    let mut thrown = sprite.quads(frame);
                    for quad in &mut thrown {
                        quad.place = offset;
                    }
                    self.quad_list(
                        (thrown, sprite.texture()),
                        (place, angles),
                        (false, blend, false),
                        &mut quads,
                        triangles,
                    );
                }
                Drawn::Shed {
                    sprite,
                    place,
                    angles,
                    size,
                } => {
                    sprites += 1;
                    let sprite = &package.sprites[sprite];
                    let Some(mut quad) = sprite.quads(0).first().copied() else {
                        continue;
                    };
                    quad.place = effects::SHED_OFF;
                    quad.size = (quad.size)
                        .map(|side| (i32::from(side) * size / effects::SHARE_ONE) as i16);
                    self.quad_list(
                        (vec![quad], sprite.texture()),
                        (place, angles),
                        (false, blend, false),
                        &mut quads,
                        triangles,
                    );
                }
                Drawn::Piece {
                    model,
                    object,
                    place,
                    angles,
                    scale,
                } => {
                    pieces += 1;
                    let Some(object) = package.models[model].mesh.objects.get(object) else {
                        continue;
                    };
                    let piece = Piece {
                        object: Some(0),
                        brightness: effects::SHARE_ONE as i16,
                        place: [0; 3],
                        angles: [0; 3],
                        scale: [scale; 3],
                        flags: effects::PIECE_FLAGS,
                    };
                    let stand = piece.stands(place, angles);
                    self.placed(
                        object,
                        (&piece, &stand),
                        (None, false),
                        &mut parts,
                        triangles,
                    );
                }
                Drawn::Glow {
                    glow,
                    flare,
                    place,
                    count,
                } => {
                    lights += 1;
                    let sprite = &package.sprites[glow];
                    let Some(mut quad) = sprite.quads(0).first().copied() else {
                        continue;
                    };
                    let sized =
                        |size: i16, by: i32| (i32::from(size) * by / effects::SHARE_ONE) as i16;
                    let swell = effects::glow_size(count);
                    quad.turn = (effects::GLOW_TURN * count) as i16;
                    quad.size = quad.size.map(|size| sized(size, swell));
                    let how = (true, blend, false);
                    let stands = (place, [0; 3]);
                    self.quad_list(
                        (vec![quad], sprite.texture()),
                        stands,
                        how,
                        &mut glows,
                        triangles,
                    );
                    let Some(ring) = flare.map(|flare| &package.sprites[flare]) else {
                        continue;
                    };
                    let distance = i32::from(self.header.projection_distance);
                    let (eye, looks) = (self.header.camera, self.header.look_at);
                    // A ring is the light's own quad as it is turned and swelled now,
                    // with the picture of the ring's sprite, at the depth of its place.
                    // Its red and green are an eighth of the light's, and its blue is
                    // an eighth of the light's green, as the game takes it.
                    let dim = [quad.colour[0] / 8, quad.colour[1] / 8, quad.colour[1] / 8];
                    let how = (false, blend, false);
                    for at in effects::flare(place, eye, looks, distance, &playing.unit) {
                        let mut piece = quad;
                        piece.size = quad.size.map(|size| sized(size, at.size));
                        piece.colour = dim;
                        let quads = (vec![piece], ring.texture());
                        self.quad_list(quads, (at.place, [0; 3]), how, &mut glows, triangles);
                    }
                }
            }
        }
        let (tasks, small) = playing.effects.living();
        Some(format!(
            "effect {} in its game frame {}: {tasks} tasks and {small} small tasks live; \
             {sprites} sprites, {} quads drawn {}, {} out of depth; {models} models, {} faces \
             drawn {}, {} turned away, {} out of depth; {shapes} shapes, {} faces drawn {}, {} \
             turned away; {lights} lights, {} quads drawn {}; {pieces} pieces, {} faces \
             drawn {}, {} turned away, {} out of depth; {ribbons} ribbons, {} stitch quads \
             drawn {}, {} turned away, {} out of depth",
            playing.number,
            playing.passed(),
            quads.drawn,
            quads.place(),
            quads.out_of_depth,
            faces.drawn,
            faces.place(),
            faces.turned_away,
            faces.out_of_depth,
            shells.drawn,
            shells.place(),
            shells.turned_away,
            glows.drawn,
            glows.place(),
            parts.drawn,
            parts.place(),
            parts.turned_away,
            parts.out_of_depth,
            seams.drawn,
            seams.place(),
            seams.turned_away,
            seams.out_of_depth
        ))
    }

    pub fn with_player(mut self, mut walker: Walker, still: bool) -> Self {
        for vram in &mut self.vrams {
            walker.images.iter().for_each(|image| vram.upload(image));
        }
        // What the game frames he ran ahead asked of the sound is not heard.
        walker.take_effects();
        walker.take_sounds();
        self.player_still = still;
        self.player = Some(walker);
        self
    }

    pub fn speaker(&self, faces: &Faces, thing: usize, other: usize) -> Result<Speaker, String> {
        let actor = |thing: usize| {
            (thing.checked_sub(1))
                .and_then(|actor| self.actors.get(actor))
                .ok_or_else(|| format!("the room has {} things, not {thing}", self.actors.len()))
        };
        let place = |thing: usize| match thing {
            0 => (self.player.as_ref())
                .map(|player| player.body.place)
                .ok_or_else(|| "thing 0 is the player: ask for --player".to_string()),
            thing => Ok(actor(thing)?.place),
        };
        // The matrix unit's screen place; behind the camera its side is all that counts.
        let seen = |place: [i32; 3]| match self.camera.project(place) {
            Some(at) => at[0].floor() as i32,
            None => self.camera.to_view(place)[0].floor() as i32,
        };
        let speaker = actor(thing)?;
        let seen = [seen(speaker.place), seen(place(other)?)];
        let face = (faces.slots.get(speaker.slot))
            .ok_or_else(|| format!("the faces have no slot {}", speaker.slot))?;
        Ok(Speaker {
            name: speaker.name.clone(),
            mirrored: speaker.mirrored,
            face: face.rgba(&self.vrams[speaker.memory]),
            picture_left: window::picture_left(seen[0], seen[1]),
            seen,
        })
    }

    /// The fade of the picture: what the scripts of the room have made of it, full
    /// brightness without them.
    pub fn fade(&self) -> Fade {
        let mut fade =
            (self.scripts.as_ref()).map_or_else(Fade::default, |scripts| *scripts.fade());
        // The screen in one colour of a critical hit: an opaque white rectangle in the
        // table of the fade's, which the room's white at its whole stands for.
        if self.scripts.as_ref().is_some_and(Scripts::flashes) {
            fade.white = u8::MAX;
        }
        fade
    }

    pub fn objects_rest(&self) -> bool {
        self.scripts.as_ref().is_some_and(Scripts::rested)
    }

    /// The exit the exit routine took in this game frame, once.
    pub fn take_exit(&mut self) -> Option<jc_formats::scene::Exit> {
        self.exit_due.take()
    }

    /// The player, when the room has one that is not standing at one game frame.
    pub fn walking_player(&mut self) -> Option<&mut Walker> {
        let still = self.player_still;
        self.player.as_mut().filter(|_| !still)
    }

    /// Another view of the room: everything is seen through its camera and lit by its
    /// lights from the next drawing on.
    fn set_view(&mut self, header: ViewHeader, view: usize) {
        self.camera = header.view_camera(self.zoom);
        self.view = view;
        self.header = header;
        self.drawn = false;
    }

    fn scroll_point(&self) -> Option<Option<IVec2>> {
        let seen = |[x, y, z]: [i32; 3]| {
            let point = self.camera.project([x, y - SCROLL_POINT_ABOVE, z])?;
            Some((point[0].floor() as i32, point[1].floor() as i32))
        };
        let Some(scripts) = &self.scripts else {
            return Some(seen(self.player.as_ref()?.body.place).map(IVec2::from));
        };
        let points = scripts.camera()?;
        let mixed = match points.share {
            0 => seen(points.followed),
            glide::ALL => seen(points.target),
            share => (seen(points.followed).zip(seen(points.target)))
                .map(|(followed, target)| glide::mixed(followed, target, share)),
        };
        Some(mixed.map(IVec2::from))
    }

    /// The point the camera task follows, which it asks the view lookup with: what
    /// the scripts of the room have named, as the camera task of the last game frame
    /// found it, and the player without scripts (the task's start puts the player
    /// there).
    fn followed(&self) -> Option<[i32; 3]> {
        match &self.scripts {
            Some(scripts) => scripts.camera().map(|points| points.followed),
            None => (self.player.as_ref()).map(|walker| walker.player.position),
        }
    }

    fn looked_view(&self) -> Option<(usize, [i32; 3])> {
        // The camera task looks no view up in its states of a fight.
        if self.before_fight.is_some() {
            return None;
        }
        let (walker, followed) = self.player.as_ref().zip(self.followed())?;
        Some((walker.view_of(self.view, followed)?, followed))
    }

    /// The zoom the room is seen at.
    pub fn zoom(&self) -> i32 {
        self.zoom
    }

    /// Whether the painted background is drawn: always, unless a script of the room has
    /// left it out or the place of a fight is drawn in its place.
    pub fn background(&self) -> bool {
        self.arena.is_none() && (self.scripts.as_ref()).is_none_or(|scripts| scripts.background())
    }

    /// Whether the picture on screen is a fight's: the place of a fight is drawn, or a
    /// held picture is the last one of a fight (the way back from it). The room's
    /// picture held in front of a fight is the room's.
    pub fn fight_on_screen(&self) -> bool {
        match self.held {
            None => self.arena.is_some(),
            Some(_) => self.fight_drawn,
        }
    }

    /// Whether a fight of the room's scripts is on, from its start to the end of the
    /// results screen behind it ([`Scripts::fight_on`]).
    pub fn fight_on(&self) -> bool {
        self.scripts.as_ref().is_some_and(Scripts::fight_on)
    }

    /// Whether the picture is held as it was last drawn.
    pub fn holds(&self) -> bool {
        self.held.is_some()
    }

    /// What the picture of this game frame is seen with, for the cuts of `smooth.rs`.
    fn eye(&self) -> smooth::Eye {
        smooth::Eye {
            at: self.header.camera,
            looks: self.header.look_at,
            roll: self.header.roll,
            distance: i32::from(self.header.projection_distance),
            view: self.view,
            zoom: self.zoom,
            fight: self.arena.is_some(),
            reach: self.reach,
        }
    }

    /// The share of the way between the last two drawings that a frame of the monitor
    /// shows when the fixed clock has made `steps` steps and gone `fraction` of a step
    /// on (`smooth.rs`): one, the last drawing as it is, after a cut, while the picture
    /// is held and while the picture before is laid over every frame.
    pub fn glide_share(&self, fraction: f32, steps: u64) -> f32 {
        match self.held.is_some() || self.picture_before().is_some() {
            true => 1.0,
            false => self.motion.share(fraction, steps),
        }
    }

    /// Whether the scroll offset may glide between the last two game frames: not after
    /// a cut, at a zoom (the tiles are cut for one offset), in a fight (its offset is
    /// the shake of a hit) or through the shake of a room's picture.
    fn scroll_glides(&self) -> bool {
        !self.motion.cuts()
            && self.zoom == ZOOM_ONE
            && self.arena.is_none()
            && self.before_fight.is_none()
            && self.room_shake == 0
    }

    /// The picture of a fight reaches `reach` pixels of the console's screen beyond
    /// either side of it (`widescreen.rs`); a fight on screen is drawn again for it.
    pub fn set_reach(&mut self, reach: i32) {
        if self.reach != reach {
            self.reach = reach;
            if self.arena.is_some() {
                self.drawn = false;
            }
        }
    }

    pub fn picture_before(&self) -> Option<u32> {
        self.scripts.as_ref().and_then(Scripts::before)
    }

    /// The draw area: what the scripts of the room have made of it, the whole screen
    /// without them.
    pub fn area(&self) -> DrawArea {
        (self.scripts.as_ref()).map_or(DrawArea::WHOLE, |scripts| scripts.area())
    }

    /// The two black bars an object of the room lays over the picture (`Scripts::cover`).
    pub fn cover(&self) -> [(i32, i32, i32, i32); 2] {
        (self.scripts.as_ref()).map_or([(0, 0, 0, 0); 2], |scripts| scripts.cover())
    }

    fn seen(&self, corners: &[[i32; 3]], one_sided: bool) -> Seen {
        Self::seen_by(&self.camera, corners, one_sided)
    }

    /// [`Things::seen`] through a camera that is named.
    fn seen_by(camera: &ViewCamera, corners: &[[i32; 3]], one_sided: bool) -> Seen {
        let seen: Vec<[f64; 3]> = (corners.iter())
            .map(|corner| camera.to_view(*corner))
            .collect();
        // The matrix unit keeps the whole part of a depth.
        let depths = seen.iter().map(|point| point[2].floor() as i64);
        let factor = match seen.len() {
            4 => DEPTH_OF_FOUR,
            _ => DEPTH_OF_THREE,
        };
        let depth = (depths.clone().sum::<i64>() * factor) >> 12;
        if depths.clone().any(|depth| depth < 1) || !(1..=DEPTH_LIMIT).contains(&depth) {
            return Seen::OutOfDepth;
        }
        let at: Vec<Vec2> = (seen.iter())
            .map(|seen| {
                Vec2::new(
                    (seen[0] * camera.distance / seen[2]) as f32,
                    (seen[1] * camera.distance / seen[2]) as f32,
                )
            })
            .collect();
        // A triangle is drawn when its corners run the right way round, a quad when either
        // of its halves does.
        let faces_viewer =
            area(at[0], at[1], at[2]) > 0.0 || (at.len() == 4 && area(at[1], at[2], at[3]) < 0.0);
        if one_sided && !faces_viewer {
            return Seen::TurnedAway;
        }
        Seen::Drawn((depth / DEPTHS_PER_SLOT) as u32, at)
    }

    fn triangles(&self) -> (Drawing, Vec<String>) {
        let mut triangles = Drawing::default();
        let mut lines = Vec::new();
        lines.extend(self.place_of_the_fight(&mut triangles));
        // The place of a fight goes into the second table, the fighters and the effects
        // into the field's.
        for (triangle, _) in &mut triangles.triangles {
            triangle.slot += TABLE_SLOTS;
        }
        if self.arena.is_none() {
            triangles.thing(thing::SKY);
            lines.extend(self.sky(&mut triangles));
            triangles.thing(thing::DOORS);
            lines.extend(self.doors(&mut triangles));
            triangles.thing(thing::FOUND);
            lines.extend(self.found(&mut triangles));
            triangles.thing(thing::CARD);
            lines.extend(self.card(&mut triangles));
            lines.extend(self.scattered(&mut triangles));
            lines.extend(self.sparks(&mut triangles));
            lines.extend(self.played(&mut triangles));
            self.shown(&mut triangles, &mut lines);
        }
        if let Some(scripts) = &self.scripts {
            lines.push(format!(
                "the scripts of the room have run {} game frames",
                scripts.game_frames()
            ));
        }
        if let Some(walker) = &self.player {
            triangles.thing(thing::PLAYER);
            lines.push(walker.line());
            // A player that a script has hidden is left out like a hidden actor.
            lines.push(match walker.player.flags & player::flag::HIDDEN {
                0 if walker.body.runner.hidden => "player: in the black of a fight".to_string(),
                0 => self.actor("player", &walker.body, &mut triangles),
                _ => "player: hidden by a script".to_string(),
            });
        }
        for (number, actor) in self.actors.iter().enumerate() {
            if self.scripts.is_some() {
                let playback = &actor.runner.playback;
                lines.push(format!(
                    "actor {number}: action {:?}, animation {} at counter {}, angle {}, \
                     flags of the mover {:#x}",
                    actor.runner.action,
                    playback.animation,
                    playback.counter,
                    actor.angle,
                    actor.mover.flags
                ));
            }
            // An actor with the hidden flag is left out by the game's draw routine, and
            // one whose place the matrix unit cannot bring to the screen.
            triangles.thing(thing::nth(thing::ACTOR, number));
            lines.push(match (actor.runner.hidden, self.draws(actor)) {
                (true, _) => format!("actor {number}: hidden until a script shows it"),
                (false, false) => {
                    format!("actor {number}: its place is outside the screen's limits")
                }
                (false, true) => self.actor(&format!("actor {number}"), actor, &mut triangles),
            });
        }
        // The tasks of an effect stand behind the player's and the actors' in the list.
        lines.extend(self.effect_of_the_fight(&mut triangles));
        (triangles, lines)
    }

    /// Adds what the records of the list show now, and a line about each record.
    fn shown(&self, triangles: &mut Drawing, lines: &mut Vec<String>) {
        for (number, (show, clock, moved)) in self.shows.iter().enumerate() {
            triangles.thing(thing::nth(thing::SHOWN, number));
            lines.push(match show.shown() {
                Some(ShowWay::Model) => self.model(number, (show, *moved), clock, triangles),
                Some(ShowWay::Sprite) => self.sprite(number, (show, *moved), clock, triangles),
                Some(ShowWay::Clip) => self.clip_line(number, show, clock),
                Some(ShowWay::Light) => match self.room_light {
                    Some(light) => format!(
                        "record {number}: a light; the fourth light of the room is at {:?}, \
                         colour {:?}, range {}",
                        light.place, light.colour, light.range
                    ),
                    None => format!("record {number}: a light, which is not on"),
                },
                None => format!("record {number}: nothing known, not drawn"),
            });
        }
    }

    fn tint_fighters(&mut self) {
        let wanted = (self.scripts.as_ref()).map_or_else(Vec::new, Scripts::fighter_tints);
        let mut asked: BTreeMap<Option<usize>, Option<(usize, jc_formats::tint::Tint, usize)>> =
            BTreeMap::new();
        for (slot, actor, tint) in wanted {
            let row = match (slot, actor) {
                (0, _) => jc_formats::tint::PLAYER_COPY.1,
                (1..=3, _) => tint::COPY.1 + tint::FIGURE,
                _ => tint::COPY.1 + (slot - 4),
            };
            asked.insert(actor, tint.map(|(number, mix)| (number, mix, row)));
        }
        // Whoever was tinted and is no longer asked for goes back to its own colours.
        let gone: Vec<Option<usize>> = (self.fight_tints.keys())
            .filter(|who| asked.get(*who).is_none_or(Option::is_none))
            .copied()
            .collect();
        for who in gone {
            self.fight_tints.remove(&who);
            let body = match who {
                None => self.player.as_mut().map(|walker| &mut walker.body),
                Some(actor) => self.actors.get_mut(actor),
            };
            if let Some(body) = body {
                body.runner.untint();
                info!("{}: its own colours again", Self::fighter_name(who));
            }
        }
        for (who, tint) in asked {
            let Some((number, mix, row)) = tint else {
                continue;
            };
            if self.fight_tints.get(&who) == Some(&number) {
                continue;
            }
            let body = match who {
                None => self.player.as_mut().map(|walker| &mut walker.body),
                Some(actor) => self.actors.get_mut(actor),
            };
            let Some(body) = body else {
                continue;
            };
            // The palette row of the model, as the palette word of its textured faces
            // names it.
            let origins: std::collections::BTreeSet<(u16, u16)> = (body.block.objects.iter())
                .flat_map(|object| &object.faces)
                .filter_map(|face| face.texture)
                .map(|page| page.palette_origin())
                .collect();
            // The copy is a whole row from x 768, where a model's palette of 256 colours
            // starts (`0x8007DBA4`, `0x8007DBB0`); a part with a palette of its own
            // elsewhere (the hero's weapon) is drawn from its bone without the rows.
            let source = (origins
                .iter()
                .find(|(x, _)| usize::from(*x) == tint::SOURCE.0))
            .or(origins.first())
            .copied();
            let (Some((x, y)), Some(vram)) = (source, self.vrams.get_mut(body.memory)) else {
                continue;
            };
            let (x, y) = (usize::from(x), usize::from(y));
            let colours: Vec<u16> = (0..tint::COLOURS).map(|n| vram.word(x + n, y)).collect();
            vram.send((x, row), tint::COLOURS, &mix.row(&colours));
            body.runner.tint(row as i16 - y as i16);
            let memory = body.memory;
            self.materials.retain(|(skin, _)| {
                skin.source != memory
                    || skin
                        .page
                        .is_none_or(|page| usize::from(page.palette_origin().1) != row)
            });
            self.fight_tints.insert(who, number);
            info!(
                "{}: tinted by record {number} of the table (towards {:?}, {} of 4096 grey, {} \
                 of 4096 of the colour): its palette row {y} is copied to row {row}",
                Self::fighter_name(who),
                mix.colour,
                mix.grey,
                mix.share
            );
        }
    }

    fn fighter_name(who: Option<usize>) -> String {
        match who {
            None => "the hero".to_string(),
            Some(actor) => format!("actor {actor}"),
        }
    }

    fn move_shows(&mut self) {
        let Some(scripts) = &self.scripts else {
            return;
        };
        let bone_of = |body: &Standing, bone: usize| -> [i32; 3] {
            match body.pose.get(bone).copied().flatten() {
                Some(placement) => std::array::from_fn(|k| body.place[k] + placement.position[k]),
                None => body.place,
            }
        };
        let moved: Vec<Option<Moved>> = (self.shows.iter())
            .map(|(show, ..)| {
                let mover = scripts.show_mover(show.mover).filter(|_| show.mover != 0)?;
                match mover {
                    // The warp of a forest, the object of type 0x11: its place while a
                    // script of it runs, else 0.
                    ShowMover::Marker => Some(scripts.marker()),
                    ShowMover::Player { .. } => {
                        let body = &self.player.as_ref()?.body;
                        let bone = mover.player_bone(|word| scripts.show_word(word))?;
                        Some(mover.on_bone(bone_of(body, bone)))
                    }
                    ShowMover::Actor { actor, bone } => {
                        Some(mover.on_bone(bone_of(self.actors.get(actor)?, bone)))
                    }
                }
            })
            .collect();
        let before = self.room_light;
        for ((show, clock, at), moved) in self.shows.iter_mut().zip(moved) {
            *at = moved;
            let lights = show.shown() == Some(ShowWay::Light);
            if lights && !clock.ended && clock.waited >= show.wait {
                let (place, _) = moved.unwrap_or_default().with(show.place, [0; 3]);
                self.room_light = Some(Movable {
                    place,
                    colour: show.angles.map(i32::from),
                    range: show.mode.into(),
                });
            }
        }
        let lit = |light: Option<Movable>| light.map(|light| (light.colour, light.range));
        if let (true, Some(light)) = (lit(before) != lit(self.room_light), self.room_light) {
            info!(
                "the fourth light of the room is on at {:?}, colour {:?}, range {}",
                light.place, light.colour, light.range
            );
        }
    }

    /// A line about a record that plays a clip.
    fn clip_line(&self, number: usize, show: &Show, clock: &ShowClock) -> String {
        let name = show.number;
        let pictures = self.package.as_ref().and_then(|package| {
            let clip = package.find(Kind::Clip, u32::try_from(name).ok()?)?;
            Some(package.clips[clip].len())
        });
        let Some(count) = pictures else {
            return format!("record {number}: clip {name} is not in the room's package");
        };
        match (clock.frame(show, count), &self.clips) {
            (None, _) => {
                format!("record {number}: clip {name} of {count} pictures is not played now")
            }
            (Some(frame), None) => format!(
                "record {number}: clip {name}, picture {frame} of {count}, is not played: \
                 the decoder's tables were not read"
            ),
            (Some(frame), Some(_)) => format!(
                "record {number}: clip {name}, picture {frame} of {count} (counter {}), in \
                 video memory at {:?}",
                clock.counter,
                clip::AT
            ),
        }
    }

    fn actor(&self, who: &str, actor: &Standing, triangles: &mut Drawing) -> String {
        let light = self.fight_light.or(self.room_light);
        let eye = (&self.header, &self.camera, self.fade(), light);
        Self::actor_seen(who, actor, eye, triangles)
    }

    /// [`Things::actor`] with the view named: its header for the lights, its camera,
    /// the fade of the screen and the movable light of a fight.
    fn actor_seen(
        who: &str,
        actor: &Standing,
        (header, camera, fade, movable): (&ViewHeader, &ViewCamera, Fade, Option<Movable>),
        triangles: &mut Drawing,
    ) -> String {
        let source = actor.memory;
        let scales = actor.scales();
        // The light record is picked for the place of the actor.
        let lights = Lights::of_view(header, actor.place).with_movable(movable);
        let in_room =
            |at: [i32; 3]| -> [i32; 3] { std::array::from_fn(|k| actor.place[k] + at[k]) };
        // Where each mesh object stands and the light on it when its bone is lit; None for
        // an object that no placed bone draws.
        let objects = actor.block.objects.len();
        // The word an object is drawn with is the model's, which the command list of the
        // actor makes, joined with the flags of the bone.
        let mut places: Vec<Option<(skeleton::Placement, Option<BoneLight>, u32)>> =
            vec![None; objects];
        for (bone, placement) in actor.skeleton.bones.iter().zip(&actor.pose) {
            let object = usize::try_from(bone.mesh).ok();
            if let Some(place) = object.and_then(|object| places.get_mut(object)) {
                *place = placement.filter(|_| bone.is_drawn()).map(|placement| {
                    let light = (bone.is_lit()).then(|| {
                        fade.lights(lights.on_bone(in_room(placement.position), &placement.matrix))
                    });
                    let word = command::bone_word(actor.runner.word, bone.flags);
                    (scales.drawn(bone, &placement), light, word)
                });
            }
        }
        let mut count = Count::default();
        let objects_and_places = actor.block.objects.iter().zip(&places);
        for (number_of_object, (object, place)) in objects_and_places.enumerate() {
            let Some((place, light, word)) = place else {
                continue;
            };
            // The faces of an object are numbered from the object's number on, so that an
            // object left out does not change the numbers of the others.
            triangles.faces_from((number_of_object as u32) << 16);
            // Bit 4 of the word blends every face in the mode of bits 5 and 6, bit 7 mixes
            // the colours towards black by the share in the upper half.
            let forced = command::blend_of(*word);
            let fade = fade.share(command::fade_of(*word));
            // The second byte moves the palette of every textured face down: the tint.
            let rows = (*word >> 8) as u8;
            for face in &object.faces {
                let number = triangles.face();
                let corners: Vec<[i32; 3]> = (face.corners.iter())
                    .map(|corner| in_room(place.place(corner.position)))
                    .collect();
                let (slot, at) = match Self::seen_by(camera, &corners, true) {
                    Seen::Drawn(slot, at) => (slot, at),
                    Seen::TurnedAway => {
                        count.turned_away += 1;
                        continue;
                    }
                    Seen::OutOfDepth => {
                        count.out_of_depth += 1;
                        continue;
                    }
                };
                let points: Vec<Point> = (face.corners.iter().zip(at))
                    .map(|(corner, at)| Point {
                        at,
                        colour: corner_colour(
                            light.as_ref(),
                            corner.normal.or(face.normal),
                            corner.colour.or(face.colour),
                            face.texture.is_some(),
                            fade,
                        ),
                        uv: corner.uv.unwrap_or_default(),
                    })
                    .collect();
                count.add(slot, &points);
                for (half, corners) in face.triangles().iter().enumerate() {
                    for skin in Skin::of(face, forced, source) {
                        triangles.push(
                            (number, half),
                            Triangle {
                                slot,
                                over: false,
                                skin: skin.lowered(rows),
                                corners: corners.map(|index| points[index]),
                            },
                        );
                    }
                }
            }
        }
        let mut stitched = Count::default();
        for (number, stitch) in actor.block.stitches.iter().enumerate() {
            let number = STITCH_FACES + number as u32;
            let first = actor.skeleton.bones.get(stitch[0].object as usize);
            let Some(first) = first.filter(|bone| bone.is_drawn()) else {
                continue;
            };
            let stands = stitch.map(|corner| places.get(corner.object as usize).copied().flatten());
            let [Some(a), Some(b), Some(c), Some(d)] = stands else {
                continue;
            };
            let vertices = stitch.map(|corner| actor.block.seam_vertex(corner));
            let corners: Vec<[i32; 3]> = ([a, b, c, d].iter().zip(vertices))
                .map(|((place, ..), vertex)| in_room(place.place(vertex.position)))
                .collect();
            let (slot, at) = match Self::seen_by(camera, &corners, true) {
                Seen::Drawn(slot, at) => (slot, at),
                Seen::TurnedAway => {
                    stitched.turned_away += 1;
                    continue;
                }
                Seen::OutOfDepth => {
                    stitched.out_of_depth += 1;
                    continue;
                }
            };
            let points: Vec<Point> = ([a, b, c, d].iter().zip(vertices).zip(at))
                .map(|(((_, light, word), vertex), at)| Point {
                    at,
                    colour: corner_colour(
                        light.as_ref(),
                        Some(vertex.normal),
                        Some(vertex.colour),
                        vertex.texture.is_some(),
                        fade.share(command::fade_of(*word)),
                    ),
                    uv: vertex.uv,
                })
                .collect();
            stitched.add(slot, &points);
            // Without a blend bit in the word the quad is drawn solid, marked texels and
            // all; with it the marked texels are mixed in the word's mode.
            let word = command::bone_word(actor.runner.word, first.flags);
            let page = vertices[0]
                .texture
                .map(|page| page.lowered((word >> 8) as u8));
            let skins = match (command::blend_of(word), page) {
                (Some(mode), Some(_)) => [
                    Some((Some(Texels::Unmarked), Mix::Solid)),
                    Some((Some(Texels::Marked), Mix::of_mode(mode))),
                ],
                (Some(mode), None) => [Some((None, Mix::of_mode(mode))), None],
                (None, _) => [Some((None, Mix::Solid)), None],
            };
            for (half, corners) in [[0, 1, 2], [1, 3, 2]].into_iter().enumerate() {
                for (texels, mix) in skins.into_iter().flatten() {
                    triangles.push(
                        (number, half),
                        Triangle {
                            slot,
                            over: false,
                            skin: Skin {
                                source,
                                page,
                                texels,
                                mix,
                            },
                            corners: corners.map(|index| points[index]),
                        },
                    );
                }
            }
        }
        format!(
            "{who} at {:?}, word {:#010x}, size {}: {} of {objects} mesh objects \
             placed, {} faces drawn {}, {} turned away, {} out of depth; {} of {} stitch quads \
             drawn, {} turned away",
            actor.place,
            actor.runner.word,
            actor.runner.size,
            places.iter().flatten().count(),
            count.drawn,
            count.place(),
            count.turned_away,
            count.out_of_depth + stitched.out_of_depth,
            stitched.drawn,
            actor.block.stitches.len(),
            stitched.turned_away
        )
    }

    fn sky(&self, triangles: &mut Drawing) -> Option<String> {
        let (stand, travelled) = self.scripts.as_ref()?.sky()?;
        let Some(block) = &self.room_models else {
            return Some("the object of the sky: the room has no model block".to_string());
        };
        let fade = self.fade().share(0);
        let mut count = Count::default();
        for object in block.objects.iter().take(sky::MESH_OBJECTS) {
            for face in &object.faces {
                let number = triangles.face();
                let corners: Vec<[i32; 3]> = (face.corners.iter())
                    .map(|corner| stand.place(corner.position))
                    .collect();
                let (slot, at) = match self.seen(&corners, true) {
                    Seen::Drawn(slot, at) => (slot, at),
                    Seen::TurnedAway => {
                        count.turned_away += 1;
                        continue;
                    }
                    Seen::OutOfDepth => {
                        count.out_of_depth += 1;
                        continue;
                    }
                };
                let points: Vec<Point> = (face.corners.iter().zip(at))
                    .map(|(corner, at)| Point {
                        at,
                        colour: corner_colour(
                            None,
                            None,
                            corner.colour.or(face.colour),
                            face.texture.is_some(),
                            fade,
                        ),
                        uv: corner.uv.unwrap_or_default(),
                    })
                    .collect();
                count.add(slot, &points);
                for (half, corners) in face.triangles().iter().enumerate() {
                    for skin in Skin::of(face, None, 0) {
                        triangles.push(
                            (number, half),
                            Triangle {
                                slot,
                                over: true,
                                skin,
                                corners: corners.map(|index| points[index]),
                            },
                        );
                    }
                }
            }
        }
        Some(format!(
            "the object of the sky at {:?}, count {travelled} of its travel: {} faces drawn \
             {}, {} turned away, {} out of depth",
            stand.position,
            count.drawn,
            count.place(),
            count.turned_away,
            count.out_of_depth
        ))
    }

    fn doors(&self, triangles: &mut Drawing) -> Option<String> {
        let leaves = self.scripts.as_ref()?.door_leaves();
        if leaves.is_empty() {
            return None;
        }
        let Some(block) = &self.room_models else {
            return Some("the doors: the room has no model block".to_string());
        };
        let count = self.lit_meshes(block, &leaves, triangles);
        Some(format!(
            "{} door leaves, the first at {:?}: {} faces drawn {}, {} turned away, {} out of \
             depth",
            leaves.len(),
            leaves[0].1.position,
            count.drawn,
            count.place(),
            count.turned_away,
            count.out_of_depth
        ))
    }

    fn found(&self, triangles: &mut Drawing) -> Option<String> {
        let meshes = self.scripts.as_ref()?.found_meshes();
        if meshes.is_empty() {
            return None;
        }
        let Some(block) = &self.room_models else {
            return Some("the thing to be searched: the room has no model block".to_string());
        };
        let count = self.lit_meshes(block, &meshes, triangles);
        Some(format!(
            "the thing to be searched, {} mesh objects, the first at {:?}: {} faces drawn {}, \
             {} turned away, {} out of depth",
            meshes.len(),
            meshes[0].1.position,
            count.drawn,
            count.place(),
            count.turned_away,
            count.out_of_depth
        ))
    }

    fn place_of_the_fight(&self, triangles: &mut Drawing) -> Option<String> {
        let arena = self.arena.as_ref()?;
        let (package, place, centre) = (&arena.package, &arena.place, &arena.centre);
        let model = package.find(Kind::Model, u32::from(place.ground));
        triangles.thing(thing::FIGHT_GROUND);
        let ground = model.map_or_else(Count::default, |index| {
            let stand = Placement {
                position: centre.place,
                ..Placement::IDENTITY
            };
            self.lit_meshes(&package.models[index].mesh, &[(0, stand)], triangles)
        });

        // A picture's colour is times the screen brightness.
        let bright = self.fade().brightness as f32 / light::ONE as f32;
        let colour_one = f32::from(TEXEL_UNCHANGED);
        let distance = i32::from(self.header.projection_distance);
        let on_screen = |seen: [f64; 3], axis: usize| {
            (seen[axis] * self.camera.distance / seen[2]).floor() as i32
        };
        let add = |triangles: &mut Drawing,
                   number: u32,
                   (slot, page): (u32, TexturePage),
                   points: [Point; 4]| {
            for (half, corners) in [[0, 1, 2], [1, 3, 2]].into_iter().enumerate() {
                for skin in Skin::of_sprite(page, None) {
                    triangles.push(
                        (number, half),
                        Triangle {
                            slot,
                            over: true,
                            skin,
                            corners: corners.map(|index| points[index]),
                        },
                    );
                }
            }
        };

        let mut pictures = Count::default();
        triangles.thing(thing::FIGHT_PICTURES);
        for &number in &place.sprites {
            let Some(index) = package.find(Kind::Sprite, u32::from(number)) else {
                continue;
            };
            let sprite = &package.sprites[index];
            let texture = sprite.texture();
            let texels = texture.whole();
            for quad in (0..sprite.frames.len()).flat_map(|frame| sprite.quads(frame)) {
                let face = triangles.face();
                // A quad without a height is left out.
                if quad.size[1] == 0 {
                    continue;
                }
                let seen = self.camera.to_view(centre.stands(quad.place));
                // A quarter of the whole part of the distance into the screen.
                let depth = (seen[2].floor() as i64 >> 2).clamp(0, i64::from(i32::MAX)) as i32;
                let Some(corners) = quad.hung(texels, distance, depth, self.zoom) else {
                    pictures.out_of_depth += 1;
                    continue;
                };
                let at = IVec2::new(on_screen(seen, 0), on_screen(seen, 1));
                let colour = arena::picture_colour(quad.colour, &self.header);
                let points: [Point; 4] = std::array::from_fn(|corner| Point {
                    at: (at + IVec2::from(corners[corner])).as_vec2(),
                    colour: Vec3::from(colour.map(|part| f32::from(part) / colour_one)) * bright,
                    uv: texels[corner],
                });
                let slot = (i64::from(depth) / DEPTHS_PER_SLOT) as u32;
                pictures.add(slot, &points);
                add(triangles, face, (slot, texture.page), points);
            }
        }

        // The strip: its height on screen comes from one point the way the camera looks.
        let mut pieces = 0;
        triangles.thing(thing::FIGHT_STRIP);
        if let Some(first) = place.strip {
            let turn = Shot {
                at: self.header.camera,
                looks: self.header.look_at,
                distance,
            }
            .turn();
            let foot = on_screen(self.camera.to_view(centre.strip_point(turn)), 1) + 1;
            // A wide picture has the strip's pieces beyond the screen's edges too.
            for piece in arena::strip_pieces(distance, self.reach) {
                let picture = piece.rem_euclid(arena::STRIP_PICTURES as i32) as usize;
                let record = usize::from(first) + picture;
                let found = (package.sprites.get(record)).zip(arena::strip_piece_reaching(
                    turn, piece, distance, self.reach,
                ));
                let Some((sprite, (left, width))) = found else {
                    continue;
                };
                let texture = sprite.texture();
                let texels = texture.whole();
                // The piece's edge counts from the left edge of the screen.
                let left = left - SCREEN.0 / 2;
                let (right, top) = (left + width + 1, foot - 1 - width);
                let corners = [[left, top], [right, top], [left, foot], [right, foot]];
                let points: [Point; 4] = std::array::from_fn(|corner| Point {
                    at: IVec2::from(corners[corner]).as_vec2(),
                    colour: Vec3::splat(bright),
                    uv: texels[corner],
                });
                // A piece is known by its place in the strip.
                let slot = (arena::STRIP_SLOT, texture.page);
                add(triangles, piece as u32, slot, points);
                pieces += 1;
            }
        }
        Some(format!(
            "the place of the fight round {:?}: the ground, {} faces drawn {}, {} turned away, \
             {} out of depth; {} standing pictures drawn {}, {} out of depth; {pieces} pieces \
             of the strip",
            centre.place,
            ground.drawn,
            ground.place(),
            ground.turned_away,
            ground.out_of_depth,
            pictures.drawn,
            pictures.place(),
            pictures.out_of_depth
        ))
    }

    /// Adds mesh objects of the room's model block as the mesh drawer's entry draws
    /// them with a flag word of 1 and no scale: each at its place and angles, lit by
    /// the light record picked for that place.
    fn lit_meshes(
        &self,
        block: &MeshBlock,
        meshes: &[(usize, Placement)],
        triangles: &mut Drawing,
    ) -> Count {
        let fade = self.fade();
        let mut count = Count::default();
        for (object, stand) in meshes {
            let Some(object) = block.objects.get(*object) else {
                continue;
            };
            let lights = Lights::of_view(&self.header, stand.position);
            let light = fade.lights(lights.on_bone(stand.position, &stand.matrix));
            for face in &object.faces {
                let number = triangles.face();
                let corners: Vec<[i32; 3]> = (face.corners.iter())
                    .map(|corner| stand.place(corner.position))
                    .collect();
                let (slot, at) = match self.seen(&corners, true) {
                    Seen::Drawn(slot, at) => (slot, at),
                    Seen::TurnedAway => {
                        count.turned_away += 1;
                        continue;
                    }
                    Seen::OutOfDepth => {
                        count.out_of_depth += 1;
                        continue;
                    }
                };
                let points: Vec<Point> = (face.corners.iter().zip(at))
                    .map(|(corner, at)| Point {
                        at,
                        colour: corner_colour(
                            Some(&light),
                            corner.normal.or(face.normal),
                            corner.colour.or(face.colour),
                            face.texture.is_some(),
                            fade.share(0),
                        ),
                        uv: corner.uv.unwrap_or_default(),
                    })
                    .collect();
                count.add(slot, &points);
                for (half, corners) in face.triangles().iter().enumerate() {
                    for skin in Skin::of(face, None, 0) {
                        triangles.push(
                            (number, half),
                            Triangle {
                                slot,
                                over: true,
                                skin,
                                corners: corners.map(|index| points[index]),
                            },
                        );
                    }
                }
            }
        }
        count
    }

    fn played(&self, triangles: &mut Drawing) -> Vec<String> {
        let Some(parts) = self.scripts.as_ref().and_then(|scripts| scripts.played()) else {
            return Vec::new();
        };
        let found = |name: u32| {
            let package = self.package.as_ref()?;
            Some(&package.models[package.find(Kind::Model, name)?])
        };
        let models = parts.map(|(part, _)| found(part.model));
        let fewest = (models.iter().flatten())
            .map(|model| model.frame(0).len())
            .min()
            .unwrap_or(0);
        (parts.iter().zip(models).enumerate())
            .map(|(number, (&(part, frame), model))| {
                triangles.thing(thing::nth(thing::PLAYED, number));
                let name = part.model;
                let Some(model) = model else {
                    return format!("the played model {name} is not in the room's package");
                };
                let mut count = Count::default();
                let placements = model.frame(usize::try_from(frame).unwrap_or(0));
                for placement in placements.iter().take(fewest) {
                    let object = placement.object.and_then(|o| model.mesh.objects.get(o));
                    let Some(object) = object else { continue };
                    let stand = placement.stands(part.place, part.angles);
                    self.placed(
                        object,
                        (placement, &stand),
                        (None, true),
                        &mut count,
                        triangles,
                    );
                }
                format!(
                    "the played model {name} at {:?}, frame {frame} of {}: {} placements, {} \
                     faces drawn {}, {} turned away, {} out of depth",
                    part.place,
                    model.frames(),
                    placements.len().min(fewest),
                    count.drawn,
                    count.place(),
                    count.turned_away,
                    count.out_of_depth
                )
            })
            .collect()
    }

    /// Adds the faces of the frame a model record shows now. Returns a line about it.
    fn model(
        &self,
        number: usize,
        (show, moved): (&Show, Option<Moved>),
        clock: &ShowClock,
        triangles: &mut Drawing,
    ) -> String {
        let name = show.number;
        let found = self.package.as_ref().and_then(|package| {
            let index = package.find(Kind::Model, u32::try_from(name).ok()?)?;
            Some(&package.models[index])
        });
        let Some(model) = found else {
            return format!("record {number}: model {name} is not in the room's package");
        };
        let Some(frame) = clock.frame(show, model.frames()) else {
            return format!(
                "record {number}: model {name} is not shown now (waited {} of {}, {} runs \
                 ended)",
                clock.waited, show.wait, clock.runs
            );
        };
        let mover = match (show.mover, moved) {
            (0, _) => "",
            (_, Some(_)) => ", where its mover puts it",
            (_, None) => ", its mover left out",
        };
        let (place, angles) = moved.unwrap_or_default().with(show.place, show.angles);
        let mut count = Count::default();
        let placements = model.frame(frame);
        for placement in &placements {
            let object = placement.object.and_then(|o| model.mesh.objects.get(o));
            let Some(object) = object else { continue };
            let stand = placement.stands(place, angles);
            self.placed(
                object,
                (placement, &stand),
                (None, false),
                &mut count,
                triangles,
            );
        }
        format!(
            "record {number}: model {name} at {:?}{mover}, frame {frame} of {} (counter {}): \
             {} placements, {} faces drawn {}, {} turned away, {} out of depth",
            place,
            model.frames(),
            clock.counter,
            placements.len(),
            count.drawn,
            count.place(),
            count.turned_away,
            count.out_of_depth
        )
    }

    fn placed(
        &self,
        object: &MeshObject,
        (placement, stand): (&Piece, &Stand),
        (colour, over): (Option<[u8; 3]>, bool),
        count: &mut Count,
        triangles: &mut Drawing,
    ) {
        let fade = self.fade();
        // The light record is picked for the place the object is drawn at.
        let light = placement.is_lit().then(|| {
            let lights = Lights::of_view(&self.header, stand.drawn.position);
            fade.lights(lights.on_bone(stand.drawn.position, &stand.turn))
        });
        for face in &object.faces {
            let number = triangles.face();
            let corners: Vec<[i32; 3]> = (face.corners.iter())
                .map(|corner| stand.drawn.place(corner.position))
                .collect();
            let (slot, at) = match self.seen(&corners, true) {
                Seen::Drawn(slot, at) => (slot, at),
                Seen::TurnedAway => {
                    count.turned_away += 1;
                    continue;
                }
                Seen::OutOfDepth => {
                    count.out_of_depth += 1;
                    continue;
                }
            };
            let points: Vec<Point> = (face.corners.iter().zip(at))
                .map(|(corner, at)| Point {
                    at,
                    colour: corner_colour(
                        light.as_ref(),
                        corner.normal.or(face.normal),
                        colour.or(corner.colour).or(face.colour),
                        face.texture.is_some(),
                        fade.share(placement.fade()),
                    ),
                    uv: corner.uv.unwrap_or_default(),
                })
                .collect();
            count.add(slot, &points);
            for (half, corners) in face.triangles().iter().enumerate() {
                for skin in Skin::of(face, placement.blend(), 0) {
                    triangles.push(
                        (number, half),
                        Triangle {
                            slot,
                            over,
                            skin,
                            corners: corners.map(|index| points[index]),
                        },
                    );
                }
            }
        }
    }

    fn stitched(
        &self,
        block: &MeshBlock,
        stood: &[Option<(usize, Piece, Stand)>],
        count: &mut Count,
        triangles: &mut Drawing,
    ) {
        use jc_formats::battle::effects;
        let fade = self.fade();
        let lights: Vec<Option<BoneLight>> = (stood.iter())
            .map(|stood| {
                let (_, piece, stand) = stood.as_ref()?;
                piece.is_lit().then(|| {
                    let lights = Lights::of_view(&self.header, stand.drawn.position);
                    fade.lights(lights.on_bone(stand.drawn.position, &stand.turn))
                })
            })
            .collect();
        let mode = command::blend_of(effects::STITCH_WORD);
        for (number, stitch) in block.stitches.iter().enumerate() {
            let number = STITCH_FACES + number as u32;
            let named = stitch.map(|corner| {
                let number = usize::from(corner.object);
                let (object, piece, stand) = stood.get(number)?.as_ref()?;
                let vertex = block.objects[*object]
                    .seam
                    .get(usize::from(corner.vertex))?;
                Some((number, piece, stand, vertex))
            });
            let [Some(a), Some(b), Some(c), Some(d)] = named else {
                continue;
            };
            let corners: Vec<[i32; 3]> = ([a, b, c, d].iter())
                .map(|(_, _, stand, vertex)| stand.drawn.place(vertex.position))
                .collect();
            let (slot, at) = match self.seen(&corners, true) {
                Seen::Drawn(slot, at) => (slot, at),
                Seen::TurnedAway => {
                    count.turned_away += 1;
                    continue;
                }
                Seen::OutOfDepth => {
                    count.out_of_depth += 1;
                    continue;
                }
            };
            let points: Vec<Point> = ([a, b, c, d].iter().zip(at))
                .map(|((number, piece, _, vertex), at)| Point {
                    at,
                    colour: corner_colour(
                        lights[*number].as_ref(),
                        Some(vertex.normal),
                        Some(vertex.colour),
                        vertex.texture.is_some(),
                        fade.share(piece.fade()),
                    ),
                    uv: vertex.uv,
                })
                .collect();
            count.add(slot, &points);
            let page = a.3.texture;
            let skins = match (mode, page) {
                (Some(mode), Some(_)) => [
                    Some((Some(Texels::Unmarked), Mix::Solid)),
                    Some((Some(Texels::Marked), Mix::of_mode(mode))),
                ],
                (Some(mode), None) => [Some((None, Mix::of_mode(mode))), None],
                (None, _) => [Some((None, Mix::Solid)), None],
            };
            for (half, corners) in [[0, 1, 2], [1, 3, 2]].into_iter().enumerate() {
                for (texels, mix) in skins.into_iter().flatten() {
                    triangles.push(
                        (number, half),
                        Triangle {
                            slot,
                            over: false,
                            skin: Skin {
                                source: 0,
                                page,
                                texels,
                                mix,
                            },
                            corners: corners.map(|index| points[index]),
                        },
                    );
                }
            }
        }
    }

    fn card(&self, triangles: &mut Drawing) -> Option<String> {
        let (block, place, drawn) = self.scripts.as_ref()?.card_meshes()?;
        let mut count = Count::default();
        for piece in &drawn {
            let object = (piece.placement.object).and_then(|object| block.objects.get(object));
            let Some(object) = object else { continue };
            let stand = piece.placement.stands(place, [0; 3]);
            self.placed(
                object,
                (&piece.placement, &stand),
                (piece.colour, true),
                &mut count,
                triangles,
            );
        }
        let [core, shell] = drawn;
        Some(format!(
            "the object of the memory card at {place:?}: turned by {}, raised by {}, colour \
             {:?}, brightness {} and size {} of 4096, {}: {} faces drawn {}, {} turned away, \
             {} out of depth",
            core.placement.angles[1],
            core.placement.place[1],
            core.colour.unwrap_or_default(),
            core.placement.brightness,
            shell.placement.scale[0],
            match core.placement.blend() {
                Some(1) => "added",
                _ => "half and half",
            },
            count.drawn,
            count.place(),
            count.turned_away,
            count.out_of_depth
        ))
    }

    fn sprite(
        &self,
        number: usize,
        (show, moved): (&Show, Option<Moved>),
        clock: &ShowClock,
        triangles: &mut Drawing,
    ) -> String {
        let name = show.number;
        let found = self.package.as_ref().and_then(|package| {
            let index = package.find(Kind::Sprite, u32::try_from(name).ok()?)?;
            Some(&package.sprites[index])
        });
        let Some(sprite) = found else {
            return format!("record {number}: sprite {name} is not in the room's package");
        };
        let frames = sprite.frames.len();
        let Some(frame) = clock.frame(show, frames) else {
            return format!(
                "record {number}: sprite {name} is not shown now (waited {} of {}, {} runs \
                 ended)",
                clock.waited, show.wait, clock.runs
            );
        };
        let mover = match (show.mover, moved) {
            (0, _) => "",
            (_, Some(_)) => ", where its mover puts it",
            (_, None) => ", its mover left out",
        };
        let (place, angles) = moved.unwrap_or_default().with(show.place, show.angles);
        // The record's mode word: one bit for "in front of everything", the rest is the
        // blend word, which names the mode in place of the one of the sprite's page.
        let mode = show.mode as u16;
        let in_front = mode & SPRITE_IN_FRONT != 0;
        let blend = (mode & SPRITE_BLENDS != 0).then_some((mode >> SPRITE_BLEND_MODE) as u8 & 3);
        let mut count = Count::default();
        let quads = sprite.quads(frame).len();
        let too_large = self.quads(
            (sprite, frame),
            (place, angles),
            (in_front, blend, false),
            &mut count,
            triangles,
        );
        let how = match blend {
            Some(mode) => format!("blended in mode {mode}"),
            None => "not blended".to_string(),
        };
        format!(
            "record {number}: sprite {name} at {:?}{mover}, frame {frame} of {frames} (counter \
             {}), {how}: {} quads, {} drawn {}, {} out of depth, {too_large} too large",
            place,
            clock.counter,
            quads,
            count.drawn,
            count.place(),
            count.out_of_depth
        )
    }

    fn scattered(&self, triangles: &mut Drawing) -> Option<String> {
        let (spots, counted) = self.scripts.as_ref()?.scattered()?;
        let found = self.package.as_ref().and_then(|package| {
            let index = package.find(Kind::Sprite, scatter::SPRITE)?;
            Some(&package.sprites[index])
        });
        let Some(sprite) = found else {
            return Some(format!(
                "the object of the places: sprite {} is not in the room's package",
                scatter::SPRITE
            ));
        };
        let mut count = Count::default();
        let (mut started, mut too_large) = (0, 0);
        for (number, spot) in spots.iter().enumerate() {
            triangles.thing(thing::nth(thing::SCATTERED, number));
            let Some(frame) = spot.frame(counted, sprite.frames.len()) else {
                continue;
            };
            started += 1;
            too_large += self.quads(
                (sprite, frame),
                (spot.place, [0; 3]),
                (false, Some(scatter::BLEND_MODE), true),
                &mut count,
                triangles,
            );
        }
        Some(format!(
            "the object of the places at count {counted}: sprite {} at {started} of {} places, \
             {} quads drawn {}, {} out of depth, {too_large} too large",
            scatter::SPRITE,
            spots.len(),
            count.drawn,
            count.place(),
            count.out_of_depth
        ))
    }

    fn sparks(&self, triangles: &mut Drawing) -> Option<String> {
        let sparks = self.scripts.as_ref()?.sparks();
        if sparks.is_empty() {
            return None;
        }
        let package = self.package.as_ref()?;
        let sprite = |number: u32| -> Option<&Sprite> {
            Some(&package.sprites[package.find(Kind::Sprite, number)?])
        };
        let (Some(spark), Some(ball)) = (sprite(halo::SPARK), sprite(halo::BALL)) else {
            return Some(format!(
                "the light: the sprites {} and {} are not both in the room's package",
                halo::SPARK,
                halo::BALL
            ));
        };
        // The player's place and the entry of his bone 1, as the mover of the show
        // records gives it.
        let body = &self.player.as_ref()?.body;
        let centre = match body.pose.get(1).copied().flatten() {
            Some(bone) => std::array::from_fn(|k| body.place[k] + bone.position[k]),
            None => body.place,
        };
        let mut count = Count::default();
        let mut too_large = 0;
        for (number, one) in sparks.iter().enumerate() {
            triangles.thing(thing::nth(thing::SPARKS, number));
            let Some(on) = ball.quads(one.on).first().map(|quad| quad.place) else {
                continue;
            };
            let place = std::array::from_fn(|k| centre[k] + i32::from(on[k]));
            too_large += self.quads(
                (spark, one.frame),
                (place, [0; 3]),
                (false, Some(halo::BLEND_MODE), true),
                &mut count,
                triangles,
            );
        }
        Some(format!(
            "the light: {} sparks round {centre:?}, {} quads drawn {}, {} out of depth, \
             {too_large} too large",
            sparks.len(),
            count.drawn,
            count.place(),
            count.out_of_depth
        ))
    }

    fn quads(
        &self,
        (sprite, frame): (&Sprite, usize),
        stands: ([i32; 3], [i16; 3]),
        how: (bool, Option<u8>, bool),
        count: &mut Count,
        triangles: &mut Drawing,
    ) -> usize {
        let quads = (sprite.quads(frame), sprite.texture());
        self.quad_list(quads, stands, how, count, triangles)
    }

    /// [`Things::quads`] for quads that are named one by one, with the texture of
    /// their sprite.
    fn quad_list(
        &self,
        (quads, texture): (Vec<Quad>, SpriteTexture),
        (place, angles): ([i32; 3], [i16; 3]),
        (in_front, blend, over): (bool, Option<u8>, bool),
        count: &mut Count,
        triangles: &mut Drawing,
    ) -> usize {
        // A quad's colour is times the screen brightness, whatever bit 0x400 says.
        let bright = self.fade().brightness as f32 / light::ONE as f32;
        let colour_one = f32::from(TEXEL_UNCHANGED);
        let mut too_large = 0;
        for quad in &quads {
            let number = triangles.face();
            let seen = self.camera.to_view(quad.stands(place, angles));
            // A quarter of the whole part of the distance into the screen.
            let depth = (seen[2].floor() as i64) >> 2;
            if !(1..=DEPTH_LIMIT).contains(&depth) {
                count.out_of_depth += 1;
                continue;
            }
            let distance = self.camera.distance;
            let Some(corners) = quad.corners(distance as i32, depth as i32, self.zoom) else {
                too_large += 1;
                continue;
            };
            // The console sets the corners around a whole pixel. It then leaves out a quad
            // that lies beyond an edge of the screen, which changes nothing that is seen.
            let centre = Vec2::new(
                (seen[0] * distance / seen[2]).floor() as f32,
                (seen[1] * distance / seen[2]).floor() as f32,
            );
            let texels = quad.texels(texture.corner);
            let points: [Point; 4] = std::array::from_fn(|corner| Point {
                at: centre + IVec2::from(corners[corner]).as_vec2(),
                colour: Vec3::from(quad.colour.map(|part| f32::from(part) / colour_one)) * bright,
                uv: texels[corner],
            });
            let slot = match in_front {
                true => 0,
                false => (depth / DEPTHS_PER_SLOT) as u32,
            };
            count.add(slot, &points);
            for (half, corners) in [[0, 1, 2], [1, 3, 2]].into_iter().enumerate() {
                for skin in Skin::of_sprite(texture.page, blend) {
                    triangles.push(
                        (number, half),
                        Triangle {
                            slot,
                            over,
                            skin,
                            corners: corners.map(|index| points[index]),
                        },
                    );
                }
            }
        }
        too_large
    }
}

/// The kind of a thing an effect draws, for the glide of `smooth.rs`: its way and the
/// sprite or model of the effect's package it shows.
fn effect_kind(drawn: &jc_formats::battle::effects::Drawn) -> u32 {
    use jc_formats::battle::effects::Drawn;
    let (way, number) = match *drawn {
        Drawn::Sprite { sprite, .. } => (0, sprite),
        Drawn::Model { model, .. } => (1, model),
        Drawn::Shape { model, .. } => (2, model),
        Drawn::Piece { model, object, .. } => (3, model << 4 | object),
        Drawn::Mixed { model, .. } => (4, model),
        Drawn::Thrown { sprite, .. } => (5, sprite),
        Drawn::Shed { sprite, .. } => (6, sprite),
        Drawn::Ribbon { model, .. } => (7, model),
        Drawn::Glow { glow, .. } => (8, glow),
    };
    way << 11 | (number as u32 & 0x7FF)
}

fn corner_colour(
    light: Option<&BoneLight>,
    normal: Option<[i16; 3]>,
    stored: Option<[u8; 3]>,
    textured: bool,
    fade: i32,
) -> Vec3 {
    let unchanged = [TEXEL_UNCHANGED; 3];
    let colour = match (light, normal) {
        (Some(light), Some(normal)) => {
            let base = if textured { Some(unchanged) } else { stored };
            light.colour(normal, base.unwrap_or(unchanged), fade)
        }
        _ => light::unlit(stored.unwrap_or(unchanged), fade),
    };
    let one = if textured {
        f32::from(TEXEL_UNCHANGED)
    } else {
        255.0
    };
    Vec3::from(colour.map(|part| f32::from(part) / one))
}

/// A piece of the shown things on screen: triangles of one slot and one skin.
#[derive(Component)]
pub struct ThingPiece {
    /// The place on the depth axis.
    depth: f32,
    /// Where its corners glide from and to between two game frames (`smooth.rs`), and
    /// the share of the way its mesh was last given; none for a piece that stands.
    glides: Option<Glides>,
    share: Option<f32>,
}

/// The corners of a piece's mesh as the game frame before put them and as the last one
/// did.
pub struct Glides {
    from: Vec<[f32; 3]>,
    to: Vec<[f32; 3]>,
}

/// One game frame: every record's clock and every actor's animation move on, and the
/// player's controller runs with the buttons of its pad or, without one, of the keyboard.
/// With the modern movement of `[controls]` the directions steer as the screen shows
/// ([`Steering`]); with the tank movement up, down and the two sides of `keys.rs` are
/// forward, back and the turns. Shift turns a run into a walk or the other way round. A
/// rising angle turns the player to the right as seen from above, the room's y pointing
/// down.
fn step_things(
    things: Option<ResMut<Things>>,
    named: Option<Res<Named>>,
    keys: Res<crate::keys::Pad>,
    mut steering: ResMut<Steering>,
    opens: Option<Res<crate::player_menu::Opens>>,
    skip: Option<Res<crate::skip::Skip>>,
) {
    let Some(mut things) = things else { return };
    if let Some(named) = named {
        things.name_player(&named.units);
    }
    let player_still = things.player_still || things.player.is_none();
    if things.still && things.actors_still && player_still {
        return;
    }
    // A hidden run presses the button of the menu in the game frames it names.
    let game_frame = (things.scripts.as_ref()).map_or(0, Scripts::game_frames);
    let circle = opens.is_some_and(|opens| opens.presses(game_frame));
    // The menu keys of the build are the button of the menu too, but in a fight, where
    // the circle describes.
    let fights = things.fight_menu().is_some();
    // While the build skips a scene, and behind it until Start and the confirm are let
    // go, the player's keys and buttons do not reach the room (`skip.rs`).
    let muted = skip.is_some_and(|skip| skip.holds_the_pad());
    let circle = circle || (keys.menu_key() && !fights && !muted);
    let circle = if circle { jc_formats::pad::CIRCLE } else { 0 };
    let pressed = if muted { 0 } else { crate::keys::held(&keys) };
    let buttons = pressed | circle;
    // The player walks by the keys' directions alone, not by the wheel's.
    let directions = if muted { 0 } else { keys.directions() };
    let pushed = keys.stick().filter(|_| !muted);
    let modern = keys.controls.movement == jc_disc::Movement::Modern;
    let keyboard = Held {
        forward: !modern && directions & jc_formats::pad::UP != 0,
        fast: crate::keys::fast(&keys),
        run: false,
        back: !modern && directions & jc_formats::pad::DOWN != 0,
        falls: !modern && directions & jc_formats::pad::LEFT != 0,
        rises: !modern && directions & jc_formats::pad::RIGHT != 0,
    };
    let pad = things.player.as_ref().and_then(Walker::held);
    // The stick of the steering, for the keyboard and a gamepad only: a pad of a hidden
    // run has none. The directions of the keys and the d-pad come first; without them
    // the gamepad's left stick steers as the stick it is.
    let things = &mut *things;
    let stick = match (modern, pad.is_some(), &things.player) {
        (true, false, Some(walker)) => {
            let view = (&things.camera, &things.header);
            let player = (walker.player.position, walker.player.angle);
            match (directions, pushed) {
                (0, Some(stick)) => steering.analog(stick, keys.runs(), view, player),
                _ => steering.stick(directions, keys.runs(), view, player),
            }
        }
        _ => {
            steering.release();
            None
        }
    };
    if let Some(walker) = &mut things.player {
        walker.stick = stick;
    }
    // The directions reach a question's cursor as well as the walk.
    if let Some(scripts) = &mut things.scripts {
        scripts.hold(buttons);
    }
    // The cross confirms; held counts as pressed here.
    let cross = things.player.as_ref().is_some_and(Walker::cross);
    things.game_frame(
        pad.unwrap_or(keyboard),
        cross || buttons & jc_formats::pad::CROSS != 0,
    );
}

/// Once a frame, between the game frames: the mouse over the answers of a question in
/// the room (`pointer.rs`).
fn point_at_answers(things: Option<ResMut<Things>>, pointer: crate::pointer::Pointer) {
    let Some(mut things) = things else { return };
    let things = things.bypass_change_detection();
    if let Some(scripts) = &mut things.scripts {
        scripts.point(pointer.mouse(), pointer.on_room());
    }
}

#[derive(Resource, Debug, Default)]
pub struct Steering {
    /// The directions held in the game frame before, and the camera they were seen with.
    directions: u16,
    camera: Option<ViewCamera>,
    /// The angle kept since the camera changed under the held keys.
    kept: Option<i16>,
}

/// The stick's length of a walk and of a run: the game walks from about 52 and runs from
/// about 105 (`player::stick_command`).
const STICK_WALK: f64 = 80.0;
const STICK_RUN: f64 = 127.0;
/// Below this share of the two projected steps' lengths their cross product counts as
/// none: the view is too flat to tell the room's axes apart on screen.
const FLAT_VIEW: f64 = 0.05;

impl Steering {
    /// No direction is held.
    fn release(&mut self) {
        *self = Self::default();
    }

    /// The stick of this game frame for the held directions, in the room's own axes; none
    /// without a direction (two opposite ones cancel).
    fn stick(
        &mut self,
        directions: u16,
        runs: bool,
        view: (&ViewCamera, &ViewHeader),
        player: ([i32; 3], i16),
    ) -> Option<[i8; 2]> {
        use jc_formats::pad;
        let on = |button: u16| f64::from(u8::from(directions & button != 0));
        let screen = [on(pad::RIGHT) - on(pad::LEFT), on(pad::DOWN) - on(pad::UP)];
        let length = if runs { STICK_RUN } else { STICK_WALK };
        self.steer(directions, screen, length, view, player)
    }

    /// The stick of this game frame for a gamepad's left stick out of its dead zone (up
    /// above 0, as far as it is pushed up to 1): its way on screen as the keys' way, and
    /// its length as far as it is pushed (`gamepad::stick_length`). The nearest of its
    /// eight ways stands for the held keys over a change of view.
    fn analog(
        &mut self,
        stick: Vec2,
        runs: bool,
        view: (&ViewCamera, &ViewHeader),
        player: ([i32; 3], i16),
    ) -> Option<[i8; 2]> {
        let directions = crate::gamepad::directions(stick.normalize_or_zero(), 0);
        let screen = [f64::from(stick.x), -f64::from(stick.y)];
        let length = crate::gamepad::stick_length(stick.length(), runs);
        self.steer(directions, screen, length, view, player)
    }

    /// The stick for a way on screen (right, down) `length` long; `directions` are the
    /// held ones that keep the angle over a change of view.
    fn steer(
        &mut self,
        directions: u16,
        screen: [f64; 2],
        length: f64,
        (camera, header): (&ViewCamera, &ViewHeader),
        (place, angle): ([i32; 3], i16),
    ) -> Option<[i8; 2]> {
        if screen == [0.0, 0.0] {
            self.release();
            return None;
        }
        if self.directions != directions {
            self.kept = None;
        } else if self.kept.is_none() && self.camera.is_some_and(|seen| seen != *camera) {
            info!(
                "steering: the view changed under the held keys, the player keeps the angle \
                 {angle}"
            );
            self.kept = Some(angle);
        }
        (self.directions, self.camera) = (directions, Some(*camera));
        let wanted = match self.kept {
            Some(kept) => f64::from(kept) * std::f64::consts::TAU / 4096.0,
            None => on_screen(camera, place, screen)
                .unwrap_or_else(|| by_camera_direction(header, screen)),
        };
        // Angle 0 lies along z and a quarter turn along x, as `ArcTangent::angle` has it.
        let part = |value: f64| (value * length).round().clamp(-127.0, 127.0) as i8;
        Some([part(wanted.sin()), part(wanted.cos())])
    }
}

/// The room's direction, as a turn in radians from z towards x, that goes the way of the
/// screen's direction (right, down) where the player stands; none in a view too flat.
fn on_screen(camera: &ViewCamera, place: [i32; 3], screen: [f64; 2]) -> Option<f64> {
    const STEP: i32 = 64;
    let at = |x: i32, z: i32| camera.project([place[0] + x, place[1], place[2] + z]);
    let (centre, along_x, along_z) = (at(0, 0)?, at(STEP, 0)?, at(0, STEP)?);
    let x = [along_x[0] - centre[0], along_x[1] - centre[1]];
    let z = [along_z[0] - centre[0], along_z[1] - centre[1]];
    let cross = x[0] * z[1] - z[0] * x[1];
    if cross.abs() < FLAT_VIEW * x[0].hypot(x[1]) * z[0].hypot(z[1]) {
        return None;
    }
    // The room's directions that go straight right and straight down the screen, each
    // of length one: the depth of the picture does not squeeze a diagonal.
    let room = |screen: [f64; 2]| {
        let room_x = (z[1] * screen[0] - z[0] * screen[1]) / cross;
        let room_z = (x[0] * screen[1] - x[1] * screen[0]) / cross;
        let length = room_x.hypot(room_z);
        [room_x / length, room_z / length]
    };
    let (right, down) = (room([1.0, 0.0]), room([0.0, 1.0]));
    let room_x = right[0] * screen[0] + down[0] * screen[1];
    let room_z = right[1] * screen[0] + down[1] * screen[1];
    Some(room_x.atan2(room_z))
}

/// The game's own rule for a stick: the stick's angle on top of the direction the camera
/// looks in.
fn by_camera_direction(header: &ViewHeader, [right, down]: [f64; 2]) -> f64 {
    let toward: [f64; 3] =
        std::array::from_fn(|axis| f64::from(header.look_at[axis] - header.camera[axis]));
    toward[0].atan2(toward[2]) + right.atan2(-down)
}

#[allow(clippy::too_many_arguments)]
pub fn change_view(
    things: Option<ResMut<Things>>,
    views: Option<ResMut<RoomViews>>,
    screen: Option<ResMut<FieldScreen>>,
    mut walk: Option<ResMut<WalkLines>>,
    parts: Query<Entity, With<GroupPart>>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<AddedMaterial>>,
    mut commands: Commands,
) {
    let (Some(mut things), Some(mut views), Some(mut screen)) = (things, views, screen) else {
        return;
    };
    // A script of this game frame may have put a view in place (`Things::pass_scripts`);
    // the camera's task comes behind the scripts and looks from that view on.
    let scripted = things.view;
    let looked = things.looked_view();
    let wanted = looked.map_or(scripted, |(view, _)| view);
    if wanted == views.shown {
        return;
    }
    let Some(view) = views.views.get(wanted).cloned().flatten() else {
        return;
    };
    match looked.filter(|(view, _)| *view != scripted) {
        Some((_, followed)) => info!(
            "view {wanted} in place of view {}: the followed point {followed:?} lies in its \
             region",
            views.shown
        ),
        None => info!(
            "view {wanted} in place of view {}: a script has named it, or a game frame \
             before the first drawing found it",
            views.shown
        ),
    }
    views.shown = wanted;
    things.set_view(view.header, wanted);
    field::change_view(
        &mut commands,
        (&mut images, &mut meshes, &mut materials),
        &parts,
        &mut screen,
        walk.as_deref_mut(),
        view,
    );
}

/// One game frame of the pictures that follow story flags: behind the scripts, the view's
/// change and the step of the background's groups, so that the frame an object holds is
/// the one that is drawn.
fn follow_flags(things: Option<ResMut<Things>>, screen: Option<ResMut<FieldScreen>>) {
    let (Some(mut things), Some(mut screen)) = (things, screen) else {
        return;
    };
    let view = things.view;
    if let Some(scripts) = &mut things.scripts {
        scripts.pass_followers(view, &mut screen);
    }
}

/// Puts the state of the clocks on screen, in place of what was there.
#[allow(clippy::too_many_arguments)]
pub fn draw_things(
    things: Option<ResMut<Things>>,
    (glide, times, skip): (
        Res<smooth::Glide>,
        Option<ResMut<smooth::FrameTimes>>,
        Option<Res<crate::skip::Skip>>,
    ),
    pieces: Query<Entity, With<ThingPiece>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<ThingMaterial>>,
    mut commands: Commands,
) {
    let Some(mut things) = things else { return };
    // While the build skips a scene the screen is black (`skip.rs`): nothing is drawn,
    // and the first drawing behind it glides from nowhere.
    if skip.is_some_and(|skip| skip.covers()) {
        things.bypass_change_detection().motion.forget();
        return;
    }
    if things.held.is_some() {
        things.bypass_change_detection().motion.hold();
    }
    if things.drawn || things.held.is_some() {
        return;
    }
    let things = &mut *things;
    things.drawn = true;
    things.fight_drawn = things.arena.is_some();
    for piece in &pieces {
        commands.entity(piece).despawn();
    }
    let (drawing, lines) = things.triangles();
    if !things.told {
        things.told = true;
        lines.iter().for_each(|line| info!("{line}"));
    }
    // With the glide on, every triangle is known again from the drawing before and
    // glides from where it was there (`smooth.rs`).
    let (triangles, ids): (Vec<Triangle>, Vec<Id>) = drawing.triangles.into_iter().unzip();
    let started = std::time::Instant::now();
    let froms = match glide.on {
        true => {
            let shapes: Vec<(Id, Corners)> = (triangles.iter().zip(&ids))
                .map(|(triangle, id)| {
                    let corners = Corners {
                        at: triangle.corners.map(|corner| corner.at),
                        uv: triangle.corners.map(|corner| corner.uv),
                    };
                    (*id, corners)
                })
                .collect();
            things.motion.draw(things.eye(), &shapes, glide.steps)
        }
        false => {
            things.motion.forget();
            vec![None; triangles.len()]
        }
    };
    if let Some(mut times) = times {
        times.glided(started.elapsed());
    }
    let (vrams, known) = (&things.vrams, &mut things.materials);
    let assets = (&mut *meshes, &mut *images, &mut *materials);
    let triangles = triangles.into_iter().zip(froms).collect();
    for (depth, mesh, material, glides) in pieces_of(triangles, vrams, known, assets) {
        let piece = ThingPiece {
            depth,
            glides,
            share: None,
        };
        commands.spawn((piece, Mesh2d(mesh), MeshMaterial2d(material)));
    }
}

/// The pieces a list of triangles is drawn with, in the order the faces were added to
/// the ordering table: for each its place on the depth axis, its mesh and its material.
/// `known` are the materials that were made before, by their skin.
fn pieces_of(
    mut triangles: Vec<Gliding>,
    vrams: &[Vram],
    known: &mut Vec<(Skin, Handle<ThingMaterial>)>,
    (meshes, images, materials): (
        &mut Assets<Mesh>,
        &mut Assets<Image>,
        &mut Assets<ThingMaterial>,
    ),
) -> Vec<Made> {
    let mut pieces = Vec::new();
    triangles.reverse();
    triangles.sort_by_key(|(triangle, _)| std::cmp::Reverse(triangle.slot));
    let mut at = 0;
    while at < triangles.len() {
        let slot = triangles[at].0.slot;
        let end = at
            + (triangles[at..].iter())
                .take_while(|(t, _)| t.slot == slot)
                .count();
        // Runs of one skin, each a piece of its own a little nearer than the one before.
        let mut runs: Vec<&[Gliding]> = Vec::new();
        let mut start = at;
        for index in at + 1..=end {
            let other = |a: &Triangle, b: &Triangle| a.skin != b.skin || a.over != b.over;
            if index == end || other(&triangles[index].0, &triangles[start].0) {
                runs.push(&triangles[start..index]);
                start = index;
            }
        }
        let step = FACES_SPAN / runs.len() as f32;
        for (number, run) in runs.into_iter().enumerate() {
            let skin = run[0].0.skin;
            let from = match run[0].0.over {
                true => FACES_OVER,
                false => FACES_FROM,
            };
            let corners = || run.iter().flat_map(|(triangle, _)| triangle.corners);
            // The screen's y points down.
            let place = |at: Vec2| [at.x, -at.y, 0.0];
            let to: Vec<[f32; 3]> = corners().map(|corner| place(corner.at)).collect();
            // Where the corners glide from between two game frames (`smooth.rs`): kept
            // with the piece, whose mesh the glide changes in every frame of the monitor.
            let glides = run.iter().any(|(_, from)| from.is_some()).then(|| Glides {
                from: (run.iter())
                    .flat_map(|(triangle, from)| {
                        from.unwrap_or(triangle.corners.map(|corner| corner.at))
                    })
                    .map(place)
                    .collect(),
                to: to.clone(),
            });
            let usage = match glides {
                Some(_) => RenderAssetUsages::default(),
                None => RenderAssetUsages::RENDER_WORLD,
            };
            let mesh = Mesh::new(PrimitiveTopology::TriangleList, usage)
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, to)
                .with_inserted_attribute(
                    Mesh::ATTRIBUTE_COLOR,
                    corners()
                        .map(|corner| [corner.colour.x, corner.colour.y, corner.colour.z, 1.0])
                        .collect::<Vec<_>>(),
                )
                .with_inserted_attribute(
                    Mesh::ATTRIBUTE_UV_0,
                    corners()
                        .map(|corner| corner.uv.map(|part| part as f32 / PAGE as f32))
                        .collect::<Vec<_>>(),
                );
            let material = match known.iter().find(|(have, _)| *have == skin) {
                Some((_, handle)) => handle.clone(),
                None => {
                    let handle = materials.add(skin.material(&vrams[skin.source], images));
                    known.push((skin, handle.clone()));
                    handle
                }
            };
            let in_table = |slot: u32| -(slot as f32) * SLOT_DEPTH + from + step * number as f32;
            let depth = match slot.checked_sub(TABLE_SLOTS) {
                None => in_table(slot),
                Some(slot) => in_table(TABLE_SLOTS) + in_table(slot) * SECOND_TABLE_SHARE,
            };
            pieces.push((depth, meshes.add(mesh), material, glides));
        }
        at = end;
    }
    pieces
}

/// A triangle with the corners it glides from between two game frames (`smooth.rs`).
type Gliding = (Triangle, Option<[Vec2; 3]>);

/// A piece made of triangles: its place on the depth axis, its mesh, its material and
/// the glide of its corners.
type Made = (f32, Handle<Mesh>, Handle<ThingMaterial>, Option<Glides>);

/// A piece of the creature the screen of file 44 shows on "View form".
#[derive(Component)]
pub struct FormPiece;

const OVER_THE_ROOM: usize = 4;
const FORM_FROM: f32 = 1.0;
const FORM_SPAN: f32 = 900.0;

/// Puts the creature of "View form" on screen in place of what was there of it, and
/// takes it away when the look is over. The projection centre is the room's: the screen
/// centre moved by the scroll offset.
fn draw_form(
    things: Option<ResMut<Things>>,
    screen: Option<Res<FieldScreen>>,
    pieces: Query<Entity, With<FormPiece>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<ThingMaterial>>,
    mut commands: Commands,
) {
    let mut things = things.filter(|things| things.form.is_some());
    let stale = match &things {
        Some(things) => things.form.as_ref().is_some_and(|form| !form.drawn),
        None => !pieces.is_empty(),
    };
    if !stale {
        return;
    }
    for piece in &pieces {
        commands.entity(piece).despawn();
    }
    let Some(things) = things.as_deref_mut() else {
        return;
    };
    let Some((triangles, line)) = things.form_triangles() else {
        return;
    };
    let Some(form) = things.form.as_mut() else {
        return;
    };
    form.drawn = true;
    let factor = form.factor;
    debug!("{line}");
    let scroll = screen.map_or(IVec2::ZERO, |screen| screen.scroll);
    let (vrams, known) = (&things.vrams, &mut things.materials);
    let assets = (&mut *meshes, &mut *images, &mut *materials);
    // The depths of the ordering table, far to near, laid into the span over the screen.
    let deepest = (DEPTH_LIMIT / DEPTHS_PER_SLOT + 2) as f32 * SLOT_DEPTH;
    let triangles = (triangles.triangles.into_iter())
        .map(|(triangle, _)| (triangle, None))
        .collect();
    for (depth, mesh, material, _) in pieces_of(triangles, vrams, known, assets) {
        let over = FORM_FROM + FORM_SPAN * (1.0 + depth / deepest).clamp(0.0, 1.0);
        let place = Vec3::new(scroll.x as f32 * factor, -scroll.y as f32 * factor, over);
        commands.spawn((
            FormPiece,
            Mesh2d(mesh),
            MeshMaterial2d(material),
            Transform::from_translation(place).with_scale(Vec3::new(factor, factor, 1.0)),
            bevy::camera::visibility::RenderLayers::layer(OVER_THE_ROOM),
        ));
    }
}

fn follow_player(
    things: Option<Res<Things>>,
    screen: Option<ResMut<FieldScreen>>,
    mut before_fight: Local<Option<(IVec2, bool)>>,
) {
    let (Some(things), Some(mut screen)) = (things, screen) else {
        return;
    };
    // A held picture: the background stays as it was drawn, under the black.
    if screen.held != things.held {
        screen.held = things.held;
    }
    if things.held.is_some() {
        return;
    }
    let tile = things.fade().tile();
    if screen.tile != tile {
        screen.tile = tile;
    }
    let white = things.fade().white;
    if screen.white != white {
        screen.white = white;
    }
    if screen.drawn != things.background() {
        screen.drawn = things.background();
    }
    let area = things.area();
    if screen.area() != area {
        screen.set_area(area);
    }
    if screen.cover != things.cover() {
        screen.cover = things.cover();
    }
    if screen.zoom() != things.zoom() {
        screen.set_zoom(things.zoom());
    }
    if things.before_fight.is_some() {
        if before_fight.is_none() {
            *before_fight = Some((screen.scroll, screen.follows));
        }
        let shaken = IVec2::new(0, things.fight_scroll);
        if screen.scroll != shaken {
            screen.scroll = shaken;
        }
        return;
    }
    if let Some((scroll, follows)) = before_fight.take() {
        (screen.scroll, screen.follows) = (scroll, follows);
    }
    let Some(point) = things.scroll_point() else {
        return;
    };
    // The scroll routine adds a shake's byte to the offset behind its limits.
    let scroll = screen.followed(point) + IVec2::new(0, things.room_shake);
    if !screen.follows || scroll != screen.scroll {
        screen.follow(scroll);
    }
}

/// Moves the pieces with the background: the projection centre is the screen centre moved by
/// the scroll offset, and between two game frames by its glide (`smooth.rs`).
pub(crate) fn place_things(
    screen: Option<Res<FieldScreen>>,
    mut pieces: Query<(&ThingPiece, &mut Transform)>,
) {
    let Some(screen) = screen else { return };
    let scroll = screen.scroll.as_vec2() + screen.glide;
    for (piece, mut transform) in &mut pieces {
        let place = Vec3::new(scroll.x, -scroll.y, piece.depth);
        transform.set_if_neq(Transform::from_translation(place));
    }
}

/// Every game frame of a room that changed what is drawn, for the glide (`smooth.rs`):
/// behind the steps of the game frame, so that the drawing knows how many game frames
/// lie between it and the one before.
fn count_game_frames(things: Option<ResMut<Things>>) {
    let Some(mut things) = things else { return };
    if !things.drawn {
        things.bypass_change_detection().motion.tick();
    }
}

/// The glide follows `smooth_motion` of `[display]`, in front of the drawing.
fn switch_glide(prefs: Option<Res<crate::prefs::Prefs>>, mut glide: ResMut<smooth::Glide>) {
    let on = prefs.is_some_and(|prefs| prefs.display.smooth_motion);
    if glide.on != on {
        info!(
            "smooth motion: {}",
            match on {
                true => "on",
                false => "off",
            }
        );
        glide.on = on;
    }
}

/// The share of the way between the last two drawings that this frame of the monitor
/// shows, behind the drawing: how far the clock has gone into the next game frame, or
/// what `--glide-at` of a check names.
fn share_glide(
    things: Option<Res<Things>>,
    time: Res<Time<Fixed>>,
    at: Option<Res<smooth::GlideAt>>,
    mut glide: ResMut<smooth::Glide>,
) {
    let fraction = at.map_or_else(|| time.overstep_fraction(), |at| at.0);
    let share = match (glide.on, things) {
        (true, Some(things)) => things.glide_share(fraction, glide.steps),
        _ => 1.0,
    };
    if glide.share != share {
        glide.share = share;
    }
}

/// Gives the mesh of every piece that glides its corners at the share of this frame.
fn glide_things(
    (glide, times): (Res<smooth::Glide>, Option<ResMut<smooth::FrameTimes>>),
    mut pieces: Query<(&mut ThingPiece, &Mesh2d)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let started = std::time::Instant::now();
    let share = glide.share;
    for (mut piece, mesh) in &mut pieces {
        let piece = piece.bypass_change_detection();
        let Some(glides) = &piece.glides else {
            continue;
        };
        if piece.share == Some(share) {
            continue;
        }
        let Some(mut mesh) = meshes.get_mut(&mesh.0) else {
            continue;
        };
        let at: Vec<[f32; 3]> = (glides.from.iter().zip(&glides.to))
            .map(|(from, to)| {
                let (from, to) = (Vec2::new(from[0], from[1]), Vec2::new(to[0], to[1]));
                smooth::between(from, to, share).extend(0.0).to_array()
            })
            .collect();
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, at);
        piece.share = Some(share);
    }
    if let Some(mut times) = times {
        times.glided(started.elapsed());
    }
}

/// The scroll offsets of the last two game frames, for the glide of the scroll.
#[derive(Default)]
struct KeptScroll {
    drawings: u64,
    before: IVec2,
    now: IVec2,
}

/// The glide of the scroll offset (`smooth.rs`): where the background and the pieces
/// are drawn between two game frames, a share of the way from the offset of the game
/// frame before to the last one's. Behind the scroll of this frame and in front of
/// everything that is placed by it.
fn glide_scroll(
    things: Option<Res<Things>>,
    screen: Option<ResMut<FieldScreen>>,
    glide: Res<smooth::Glide>,
    mut kept: Local<KeptScroll>,
) {
    let Some(mut screen) = screen else { return };
    let wanted = match things {
        Some(things) if glide.on => {
            let drawings = things.motion.drawings();
            if drawings != kept.drawings {
                (kept.drawings, kept.before, kept.now) = (drawings, kept.now, screen.scroll);
            } else if screen.scroll != kept.now {
                // Moved without a game frame: no glide.
                (kept.before, kept.now) = (screen.scroll, screen.scroll);
            }
            (things.scroll_glides())
                .then(|| smooth::scroll_glide(kept.before, kept.now, glide.share))
                .flatten()
                .unwrap_or(Vec2::ZERO)
        }
        _ => Vec2::ZERO,
    };
    if screen.glide != wanted {
        screen.glide = wanted;
    }
}

#[cfg(test)]
mod steering_tests {
    use super::*;
    use jc_formats::pad;

    /// A camera 1000 above and 2000 behind the origin along -z, looking at it: up the
    /// screen is along +z, right along +x.
    fn behind() -> ViewHeader {
        let mut header = ViewHeader::parse(&[0; 0x200]).expect("a header");
        (header.camera, header.look_at) = ([0, -1000, -2000], [0, 0, 0]);
        header.projection_distance = 300;
        header
    }

    fn turn_of(stick: [i8; 2]) -> f64 {
        f64::from(stick[0]).atan2(f64::from(stick[1])).to_degrees()
    }

    #[test]
    fn the_keys_go_the_way_the_screen_shows() {
        let header = behind();
        let camera = header.view_camera(ZOOM_ONE);
        let mut steering = Steering::default();
        let mut go = |directions| {
            let stick = steering.stick(directions, true, (&camera, &header), ([0; 3], 0));
            steering.release();
            stick.map(turn_of)
        };
        let near = |seen: Option<f64>, wanted: f64| seen.is_some_and(|s| (s - wanted).abs() < 1.0);
        assert!(near(go(pad::UP), 0.0), "{:?}", go(pad::UP));
        assert!(near(go(pad::RIGHT), 90.0), "{:?}", go(pad::RIGHT));
        assert!(near(go(pad::LEFT), -90.0), "{:?}", go(pad::LEFT));
        assert!(
            near(go(pad::DOWN).map(f64::abs), 180.0),
            "{:?}",
            go(pad::DOWN)
        );
        assert!(
            near(go(pad::UP | pad::RIGHT), 45.0),
            "{:?}",
            go(pad::UP | pad::RIGHT)
        );
        assert_eq!(go(pad::UP | pad::DOWN), None);
        assert_eq!(go(0), None);
    }

    #[test]
    fn a_run_is_a_long_stick_and_a_walk_a_short_one() {
        let header = behind();
        let camera = header.view_camera(ZOOM_ONE);
        let mut steering = Steering::default();
        let run = steering.stick(pad::UP, true, (&camera, &header), ([0; 3], 0));
        assert_eq!(
            player::stick_command(run.unwrap()) & player::command::MOVEMENT,
            1
        );
        let walk = steering.stick(pad::UP, false, (&camera, &header), ([0; 3], 0));
        let walk = player::stick_command(walk.unwrap()) & player::command::MOVEMENT;
        assert_eq!(walk, player::command::WALK);
    }

    #[test]
    fn a_new_view_under_the_held_keys_keeps_the_angle_until_they_change() {
        let header = behind();
        let camera = header.view_camera(ZOOM_ONE);
        // The camera of the next view looks the other way: up the screen is along -z.
        let mut turned = header;
        (turned.camera, turned.look_at) = ([0, -1000, 2000], [0, 0, 0]);
        let other = turned.view_camera(ZOOM_ONE);
        let mut steering = Steering::default();
        let held = pad::UP;
        let first = steering.stick(held, true, (&camera, &header), ([0; 3], 0));
        assert!(turn_of(first.unwrap()).abs() < 1.0);
        // The player's angle is 0 (along +z) when the view changes: it goes on that way.
        let kept = steering.stick(held, true, (&other, &turned), ([0; 3], 0));
        assert!(turn_of(kept.unwrap()).abs() < 1.0);
        // Another key, and the new view counts.
        let fresh = steering.stick(held | pad::LEFT, true, (&other, &turned), ([0; 3], 0));
        let fresh = turn_of(fresh.unwrap());
        assert!((fresh - 135.0).abs() < 1.0, "{fresh}");
    }

    #[test]
    fn a_gamepads_stick_goes_its_own_way_and_runs_when_pushed_far() {
        let header = behind();
        let camera = header.view_camera(ZOOM_ONE);
        let mut steering = Steering::default();
        let mut go = |stick: Vec2, runs: bool| {
            let view = (&camera, &header);
            let stick = steering.analog(stick, runs, view, ([0; 3], 0)).unwrap();
            steering.release();
            let movement = player::stick_command(stick) & player::command::MOVEMENT;
            (turn_of(stick), movement)
        };
        // Up and a little right: a little right of straight on, not the keys' 45.
        let (turn, movement) = go(Vec2::new(0.3, 0.95).normalize(), true);
        assert!((turn - 17.5).abs() < 1.0, "{turn}");
        assert_eq!(movement, player::command::RUN);
        let (turn, movement) = go(Vec2::new(-0.3, 0.0), true);
        assert!((turn + 90.0).abs() < 1.0, "{turn}");
        assert_eq!(movement, player::command::WALK);
        // A walk is a walk however far the stick goes.
        assert_eq!(go(Vec2::new(0.0, 1.0), false).1, player::command::WALK);
    }
}
