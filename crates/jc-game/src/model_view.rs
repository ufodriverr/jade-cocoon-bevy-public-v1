
use bevy::asset::{RenderAssetUsages, uuid_handle};
use bevy::camera::CompositingSpace;
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::mesh::{MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, Extent3d,
    RenderPipelineDescriptor, SpecializedMeshPipelineError, TextureDimension, TextureFormat,
};
use bevy::shader::ShaderRef;
use jc_disc::{Archive, DiscImage, Executable, LocalConfig};
use jc_formats::action::model_scale;
use jc_formats::animation::{Animation, Playback, Span, Start, WHOLE};
use jc_formats::background::ViewHeader;
use jc_formats::field::RoomData;
use jc_formats::light::{self, BoneLight, Lights, TEXEL_UNCHANGED};
use jc_formats::mesh::{Face, MeshBlock, TexturePage};
use jc_formats::model::{ModelTable, Section};
use jc_formats::rooms::{FIELD_ENGINE_BASE, FIELD_ENGINE_FILE, RoomTable};
use jc_formats::scene::ScenePackage;
use jc_formats::skeleton::{
    Bone, BoneScale, CREATURE_SKELETON, HUMAN_SKELETON, MODEL_LIST, MODEL_LIST_BASE,
    MODEL_LIST_FILE, ModelList, Placement, Scales, Skeleton, bone_groups,
};
use jc_formats::vram::{PAGE, Texels, Vram};

const MODEL_SHADER: Handle<Shader> = uuid_handle!("b0a4c5d2-7e31-4c7b-8f43-5d1e9a6c2f08");
/// What the view shows behind the model. Not black, so that a face which takes light away
/// from what is behind it (the patch on the ground) can be seen.
const BACKDROP: Color = Color::srgb_u8(72, 80, 96);
/// Moves the pieces that are subtracted in front of the other mixed pieces in the order of
/// drawing: the patch on the ground lies under the model.
const DRAWN_FIRST: f32 = -1000.0;

pub struct ModelViewPlugin;

impl Plugin for ModelViewPlugin {
    fn build(&self, app: &mut App) {
        let shader = Shader::from_wgsl(include_str!("model.wgsl"), "model.wgsl");
        app.world_mut()
            .resource_mut::<Assets<Shader>>()
            .insert(&MODEL_SHADER, shader)
            .expect("a fixed handle takes an asset");
        app.add_plugins(MaterialPlugin::<ModelMaterial>::default());
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Mix {
    /// Drawn as it is.
    Solid,
    /// Mode 0: half of what is behind plus half of the texel.
    Half,
    /// Mode 1: what is behind plus the texel.
    Added,
    /// Mode 2: what is behind minus the texel.
    Subtracted,
    /// Mode 3: what is behind plus a quarter of the texel.
    AddedQuarter,
}

impl Mix {
    /// The mixing of the console's blend mode, the two bits of a page word.
    pub fn of_mode(mode: u8) -> Self {
        match mode & 3 {
            0 => Self::Half,
            1 => Self::Added,
            2 => Self::Subtracted,
            _ => Self::AddedQuarter,
        }
    }

    /// How much of the texel goes into the mix.
    pub fn share(self) -> f32 {
        match self {
            Self::Half => 0.5,
            Self::AddedQuarter => 0.25,
            Self::Solid | Self::Added | Self::Subtracted => 1.0,
        }
    }

    /// The blend state that does the mixing, in the colour values the camera composes in.
    pub fn blend(self) -> Option<BlendState> {
        let (dst_factor, operation) = match self {
            Self::Solid => return None,
            Self::Half => (BlendFactor::OneMinusSrcAlpha, BlendOperation::Add),
            Self::Added | Self::AddedQuarter => (BlendFactor::One, BlendOperation::Add),
            Self::Subtracted => (BlendFactor::One, BlendOperation::ReverseSubtract),
        };
        Some(BlendState {
            color: BlendComponent {
                src_factor: BlendFactor::SrcAlpha,
                dst_factor,
                operation,
            },
            alpha: BlendComponent {
                src_factor: BlendFactor::Zero,
                dst_factor: BlendFactor::One,
                operation: BlendOperation::Add,
            },
        })
    }
}

/// Draws a piece of a model as the console does: texels times corner brightness, in the
/// console's colour values, mixed with what is behind it in one of the console's ways.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
#[bind_group_data(Mix)]
pub struct ModelMaterial {
    #[texture(0)]
    #[sampler(1)]
    texture: Handle<Image>,
    #[uniform(2)]
    share: Vec4,
    mix: Mix,
}

impl From<&ModelMaterial> for Mix {
    fn from(material: &ModelMaterial) -> Self {
        material.mix
    }
}

impl Material for ModelMaterial {
    fn fragment_shader() -> ShaderRef {
        MODEL_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        match self.mix {
            // The shader leaves out the texels that are not drawn.
            Mix::Solid => AlphaMode::Mask(0.5),
            // Drawn after the solid pieces; `specialize` replaces the blending itself.
            _ => AlphaMode::Blend,
        }
    }

    fn depth_bias(&self) -> f32 {
        match self.mix {
            Mix::Subtracted => DRAWN_FIRST,
            _ => 0.0,
        }
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // The console draws both sides of a face.
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

/// How far the camera stands from a model that is scaled to a radius of one.
const CAMERA_DISTANCE: f32 = 2.7;
/// The corners of a stitch quad as two triangles, in the order of a face's.
const QUAD: [[usize; 3]; 2] = [[0, 1, 2], [1, 3, 2]];

/// Which bone table poses the model.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Rig {
    /// The one the model list names, or else the one with as many bones as the animation.
    ByBoneCount,
    Creature,
    Human,
    None,
}

#[derive(Resource)]
pub struct ShowModel {
    file: u16,
    object: Option<usize>,
    animation: usize,
    /// The action of the model's table that is played, in place of a whole animation.
    action: Option<usize>,
    /// The one frame that is shown; the animation plays when there is none.
    frame: Option<usize>,
    /// The animation the view starts in and the game frames it takes to fade out.
    fade: Option<(usize, i16)>,
    rig: Rig,
    /// The room, and the view of it, whose lights fall on the model.
    light: Option<(u16, Option<usize>)>,
    /// The stage of growth the model is shown at.
    stage: Option<usize>,
    /// The part the model has on, and whether its other two objects are shown.
    part: Option<(usize, bool)>,
}

impl ShowModel {
    pub fn parse(value: &str) -> Option<Self> {
        let (file, object) = match value.split_once(':') {
            Some((file, object)) => (file, Some(object.parse().ok()?)),
            None => (value, None),
        };
        Some(Self {
            file: file.parse().ok()?,
            object,
            animation: 0,
            action: None,
            frame: None,
            fade: None,
            rig: Rig::ByBoneCount,
            light: None,
            stage: None,
            part: None,
        })
    }

    /// The value of `--part=N[:other]`.
    pub fn part(mut self, value: &str) -> Option<Self> {
        self.part = Some(match value.split_once(':') {
            Some((part, "other")) => (part.parse().ok()?, true),
            Some(_) => return None,
            None => (value.parse().ok()?, false),
        });
        Some(self)
    }

    /// The value of `--stage=N`.
    pub fn stage(mut self, value: &str) -> Option<Self> {
        self.stage = Some(value.parse().ok()?);
        Some(self)
    }

    /// The value of `--light=ROOM[:VIEW]`.
    pub fn light(mut self, value: &str) -> Option<Self> {
        self.light = Some(match value.split_once(':') {
            Some((room, view)) => (room.parse().ok()?, Some(view.parse().ok()?)),
            None => (value.parse().ok()?, None),
        });
        Some(self)
    }

    /// The value of `--animation=N[:FRAME]`.
    pub fn animation(mut self, value: &str) -> Option<Self> {
        (self.animation, self.frame) = match value.split_once(':') {
            Some((animation, frame)) => (animation.parse().ok()?, Some(frame.parse().ok()?)),
            None => (value.parse().ok()?, None),
        };
        Some(self)
    }

    /// The value of `--action=N[:FRAME]`: the action's animation takes the place of
    /// `--animation`.
    pub fn action(mut self, value: &str) -> Option<Self> {
        (self.action, self.frame) = match value.split_once(':') {
            Some((action, frame)) => (Some(action.parse().ok()?), Some(frame.parse().ok()?)),
            None => (Some(value.parse().ok()?), None),
        };
        Some(self)
    }

    /// The value of `--fade=M:FRAMES`.
    pub fn fade(mut self, value: &str) -> Option<Self> {
        let (animation, frames) = value.split_once(':')?;
        let frames: i16 = frames.parse().ok()?;
        if frames <= 0 {
            return None;
        }
        self.fade = Some((animation.parse().ok()?, frames));
        Some(self)
    }

    /// The value of `--rig=creature|human|none`.
    pub fn rig(mut self, value: &str) -> Option<Self> {
        self.rig = match value {
            "creature" => Rig::Creature,
            "human" => Rig::Human,
            "none" => Rig::None,
            _ => return None,
        };
        Some(self)
    }
}

/// A position or direction of the console (y down, z away from the viewer) in Bevy's space.
fn to_bevy(v: [i32; 3]) -> Vec3 {
    Vec3::new(v[0] as f32, -(v[1] as f32), -(v[2] as f32))
}

/// What a triangle is drawn with.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Surface {
    /// The colours of its corners.
    Plain,
    /// A texture page with a palette: every drawn texel, as it is.
    Textured(TexturePage),
    Blended { page: TexturePage, texels: Texels },
}

impl Surface {
    /// The surfaces a face or stitch quad is drawn with, one after the other.
    fn of(texture: Option<TexturePage>, blended: bool) -> impl Iterator<Item = Self> {
        let surfaces = match texture {
            None => [Some(Self::Plain), None],
            Some(page) if blended => [Texels::Unmarked, Texels::Marked]
                .map(|texels| Some(Self::Blended { page, texels })),
            Some(page) => [Some(Self::Textured(page)), None],
        };
        surfaces.into_iter().flatten()
    }

    /// How the surface meets what is behind it.
    fn mix(self) -> Mix {
        match self {
            Self::Blended {
                page,
                texels: Texels::Marked,
            } => Mix::of_mode(page.blend_mode()),
            _ => Mix::Solid,
        }
    }
}

/// A corner of a triangle that is drawn.
#[derive(Clone, Copy)]
struct Point {
    position: Vec3,
    /// What the texel, or white in an untextured face, is multiplied by.
    colour: Vec3,
    /// Pixel position inside the texture page.
    uv: [u8; 2],
}

fn corner_colour(
    light: Option<&BoneLight>,
    normal: Option<[i16; 3]>,
    stored: Option<[u8; 3]>,
    textured: bool,
) -> Vec3 {
    let unchanged = [TEXEL_UNCHANGED; 3];
    let colour = match (light, normal) {
        (Some(light), Some(normal)) => {
            let base = if textured { Some(unchanged) } else { stored };
            light.colour(normal, base.unwrap_or(unchanged), 0)
        }
        _ => light::unlit(stored.unwrap_or(unchanged), 0),
    };
    let one = if textured {
        f32::from(TEXEL_UNCHANGED)
    } else {
        255.0
    };
    Vec3::from(colour.map(|part| f32::from(part) / one))
}

/// Triangles of one surface: positions, colours and texture coordinates, three per triangle.
#[derive(Default)]
struct Triangles {
    positions: Vec<Vec3>,
    colours: Vec<[f32; 4]>,
    uvs: Vec<[f32; 2]>,
}

impl Triangles {
    fn add(&mut self, corners: [Point; 3]) {
        for corner in corners {
            self.positions.push(corner.position);
            self.colours
                .push([corner.colour.x, corner.colour.y, corner.colour.z, 1.0]);
            self.uvs
                .push(corner.uv.map(|part| part as f32 / PAGE as f32));
        }
    }
}

/// Everything that is drawn, by surface.
#[derive(Default)]
struct Drawn {
    groups: Vec<(Surface, Triangles)>,
}

impl Drawn {
    fn group(&mut self, surface: Surface) -> &mut Triangles {
        let index = match self.groups.iter().position(|(have, _)| *have == surface) {
            Some(index) => index,
            None => {
                self.groups.push((surface, Triangles::default()));
                self.groups.len() - 1
            }
        };
        &mut self.groups[index].1
    }

    /// A face of a mesh object that stands at `placement`, lit by `light` when its object is.
    fn add_face(&mut self, face: &Face, placement: &Placement, light: Option<&BoneLight>) {
        for triangle in face.triangles() {
            let corners = triangle.map(|index| {
                let corner = &face.corners[index];
                Point {
                    position: to_bevy(placement.place(corner.position)),
                    colour: corner_colour(
                        light,
                        corner.normal.or(face.normal),
                        corner.colour.or(face.colour),
                        face.texture.is_some(),
                    ),
                    uv: corner.uv.unwrap_or_default(),
                }
            });
            for surface in Surface::of(face.texture, face.blends_by_itself()) {
                self.group(surface).add(corners);
            }
        }
    }

    fn positions(&self) -> impl Iterator<Item = Vec3> + '_ {
        (self.groups.iter()).flat_map(|(_, triangles)| triangles.positions.iter().copied())
    }
}

/// What the disc holds of the model that this view draws.
struct Loaded {
    block: MeshBlock,
    /// Video memory with the model's texture set sent to it.
    vram: Vram,
    /// The bone table and the animation that pose the model, when there is a table for it.
    pose: Option<Posing>,
    /// The lights of the room the model is shown in, and where it stands there.
    lights: Option<(Lights, [i32; 3])>,
}

/// The lights of a view of a room for a model at the view's default place for the player.
fn room_lights(
    disc: &mut DiscImage<std::fs::File>,
    archive: &Archive,
    field_engine: &[u8],
    (number, view): (u16, Option<usize>),
) -> Result<(String, Lights, [i32; 3]), Box<dyn std::error::Error>> {
    let table = RoomTable::from_field_engine(field_engine)?;
    let (_, room) = table.by_number(number).ok_or("no room has this number")?;
    let package = archive.read(disc, room.package_file as usize)?;
    let package = ScenePackage::parse(&package)?;
    let data = RoomData::parse(package.room_data())?;
    let header = match view {
        // The room data repeats the header of view 0.
        None => ViewHeader::parse(data.header),
        Some(view) => {
            let record = data.views.get(view).ok_or("the room has no such view")?;
            let background = archive.read(disc, room.background_file as usize)?;
            (background.get(record.offset as usize..)).and_then(ViewHeader::parse)
        }
    }
    .ok_or("the view has no header")?;
    let position = header.player_position;
    let lights = Lights::of_view(&header, position);
    let line = format!(
        "lit as in room {number} at {position:?}: light towards {:?} of colour {:?}, ambient \
         {:?}, point light at {:?} of colour {:?} and range {}",
        lights.direction,
        lights.colour,
        lights.ambient,
        lights.point.position,
        lights.point.color,
        lights.point.range
    );
    Ok((line, lights, position))
}

/// What poses the model.
struct Posing {
    skeleton: Skeleton,
    /// The number of the animation among the model's.
    number: usize,
    animation: Animation,
    /// The frames of it that are played.
    span: Span,
    /// The animation the view starts in and fades out of.
    old: Option<Animation>,
    /// The scale of the whole model and of its bones at the stage that is shown.
    stage: Option<([i32; 3], Vec<BoneScale>)>,
}

impl Posing {
    fn scales(&self) -> Scales<'_> {
        match &self.stage {
            Some((model, bones)) => Scales {
                model: Some(*model),
                bones,
            },
            None => Scales::NONE,
        }
    }

    /// Where every bone stands at a moment of the playback.
    fn placements(&self, playback: &Playback) -> Vec<Option<Placement>> {
        let frame = playback.frame(self.animation.frames);
        match (playback.fade, &self.old) {
            (Some(fade), Some(old)) => self.skeleton.pose_faded_scaled(
                (&self.animation, frame),
                (old, Playback::frame_of(fade.counter, old.frames)),
                fade.old_share(),
                &self.scales(),
            ),
            _ => (self.skeleton).pose_scaled(&self.animation, frame, &self.scales()),
        }
    }
}

/// Reads the model from the disc, with lines that describe it.
fn load(show: &ShowModel) -> Result<(Vec<String>, Loaded), Box<dyn std::error::Error>> {
    let config = LocalConfig::load()?;
    let mut disc = DiscImage::open(&config.disc_image)?;
    let archive = Archive::open(&mut disc)?;
    let exe = Executable::read(&mut disc)?;
    let table = ModelTable::from_executable(&exe.image, exe.base)?;
    let (model, record) = table
        .by_file(show.file)
        .ok_or("the model table has no model in this file")?;
    let file = archive.read(&mut disc, show.file as usize)?;
    let block = record.mesh(&file)?;
    let textures = record.textures(&file)?;
    let vram = Vram::with(&textures);
    let sections = [
        Section::Head,
        Section::Textures,
        Section::Mesh,
        Section::Animations,
    ]
    .map(|section| record.section(section).count);
    let resident = record.animations.iter().filter(|a| a.resident).count();
    let mut lines = vec![
        format!(
            "model {model} (file {}): sections of {sections:?} sectors, {} animations of which \
             {resident} resident, {} animation sets, {} parts",
            show.file,
            record.animations.len(),
            record.sets.len(),
            record.parts.len()
        ),
        format!(
            "model {model}: {} mesh objects with {} faces, {} stitch quads",
            block.objects.len(),
            block.face_count(),
            block.stitches.len()
        ),
        format!(
            "model {model}: {} images in the texture set, {} with colours of their own",
            textures.len(),
            (textures.iter())
                .filter(|image| image.clut.as_ref().is_some_and(|clut| !clut.is_shared()))
                .count()
        ),
    ];

    // A part repaints the model's texture page and gives some mesh objects another mesh.
    let (block, vram) = match show.part {
        Some((number, other)) => {
            let mut part = record.part(&file, number)?;
            if other {
                // The pair of objects the player's controller swaps in by hand: the ones
                // behind those of the pair list.
                for pair in &mut part.pairs {
                    pair.source += 1;
                }
            }
            lines.push(format!(
                "model {model}: part {number} of {}: {} mesh objects, {} images at {:?}, mesh \
                 objects and their sources {:?}",
                record.parts.len(),
                part.mesh.objects.len(),
                part.images.len(),
                (part.images.iter())
                    .map(|image| (image.vram.x, image.vram.y))
                    .collect::<Vec<_>>(),
                (part.pairs.iter())
                    .map(|pair| (pair.object, pair.source))
                    .collect::<Vec<_>>()
            ));
            (
                part.put_on(&block),
                Vram::with(textures.iter().chain(&part.images)),
            )
        }
        None => (block, vram),
    };

    let head = record.head(&file)?;
    lines.push(format!(
        "model {model}: {} actions of {} bytes, {} with an animation",
        head.actions.len(),
        head.record,
        (head.actions.iter())
            .filter(|action| action.animation.is_some())
            .count()
    ));

    let field_engine = archive.read(&mut disc, FIELD_ENGINE_FILE)?;
    let lights = match show.light {
        Some(room) => {
            let (line, lights, position) = room_lights(&mut disc, &archive, &field_engine, room)?;
            lines.push(format!("model {model}: {line}"));
            Some((lights, position))
        }
        None => None,
    };

    let mut pose = None;
    if show.object.is_none() && show.rig != Rig::None {
        // An action names the animation and the frames of it; without one it is the whole
        // of the animation that was asked for.
        let action = match show.action {
            Some(number) => {
                let action = (head.actions.get(number)).ok_or("the model has no such action")?;
                let animation = action.animation.ok_or("the action has no animation")?;
                lines.push(format!(
                    "model {model}: action {number} plays animation {animation} from frame {} \
                     to {}, then from {} (resident byte {}, hold {}, windows {:?}, tail {:?})",
                    action.first,
                    action.end,
                    action.loop_start,
                    action.resident,
                    action.hold,
                    action.windows,
                    action.tail
                ));
                Some((animation, action.span()))
            }
            None => None,
        };
        let number = action.map_or(show.animation, |(animation, _)| animation);
        let animation = record.animation(&file, number)?;
        let span = action.map_or(Span::whole(&animation), |(_, span)| span);
        lines.push(format!(
            "model {model}: animation {number} has {} frames for {} bones, {} events",
            animation.frames,
            animation.bones,
            animation.events.len()
        ));
        if let Some(frame) = show.frame.filter(|frame| *frame >= animation.frames) {
            return Err(format!("the animation has no frame {frame}").into());
        }
        let old = match show.fade {
            Some((number, _)) => Some(record.animation(&file, number)?),
            None => None,
        };
        let creature = Skeleton::at(&exe.image, exe.base, CREATURE_SKELETON)?;
        let human = Skeleton::at(&field_engine, FIELD_ENGINE_BASE, HUMAN_SKELETON)?;
        let overlay = archive.read(&mut disc, MODEL_LIST_FILE)?;
        let listed = ModelList::at(&overlay, MODEL_LIST_BASE, MODEL_LIST)?;
        let skeleton = match show.rig {
            Rig::Creature => Some((creature, "the creatures' table".to_string())),
            Rig::Human => Some((human, "the people's table".to_string())),
            _ => match listed.get(model) {
                Some(entry) => Some((
                    entry.skeleton.clone(),
                    format!("the table the model list gives {}", entry.name),
                )),
                // The few models the list leaves out are creatures, but for one.
                None => [(creature, "creatures'"), (human, "people's")]
                    .into_iter()
                    .find(|(skeleton, _)| skeleton.bones.len() == animation.bones)
                    .map(|(skeleton, whose)| {
                        (skeleton, format!("the {whose} table, by its bone count"))
                    }),
            },
        };
        match skeleton {
            Some((mut skeleton, which)) => {
                lines.push(format!(
                    "model {model}: posed with {which}, {} bones",
                    skeleton.bones.len()
                ));
                let stage = match show.stage {
                    Some(number) => {
                        let stages = head.stages()?;
                        let stage = stages.get(number).ok_or(match stages.len() {
                            0 => "the model has no stages".to_string(),
                            count => format!("the model has stages 0 to {}", count - 1),
                        })?;
                        let scale = model_scale(&stages, number).ok_or("no such stage")?;
                        let groups = bone_groups(&exe.image, exe.base)
                            .ok_or("the executable has no bone groups")?;
                        let before = skeleton.clone();
                        skeleton = skeleton.with_groups(groups, stage.shown, stage.marked);
                        // The bones the stage changed, by their place in the table.
                        let changed = |is: fn(&Bone) -> bool| -> Vec<usize> {
                            (before.bones.iter().zip(&skeleton.bones).enumerate())
                                .filter(|(_, (was, now))| is(was) != is(now))
                                .map(|(bone, _)| bone)
                                .collect()
                        };
                        lines.push(format!(
                            "model {model}: stage {number} of {}: scale of the model {scale:?}, \
                             {} of {} bone scales not one, groups shown {:?} and marked {:?}: \
                             bones {:?} not drawn, bones {:?} marked",
                            stages.len(),
                            (stage.bones.iter())
                                .filter(|bone| bone.scale != BoneScale::NONE.scale)
                                .count(),
                            stage.bones.len(),
                            stage.shown,
                            stage.marked,
                            changed(Bone::is_drawn),
                            changed(Bone::is_marked),
                        ));
                        Some((scale, stage.bones.clone()))
                    }
                    None => None,
                };
                pose = Some(Posing {
                    skeleton,
                    number,
                    animation,
                    span,
                    old,
                    stage,
                });
            }
            None => lines.push(format!(
                "model {model}: no bone table of {} bones is known, the objects are not posed",
                animation.bones
            )),
        }
    }
    let loaded = Loaded {
        block,
        vram,
        pose,
        lights,
    };
    Ok((lines, loaded))
}

/// The posed model: the object of every drawn bone at the bone's place, and the stitch quads
/// between them. Returns what is drawn and the number of objects no bone drew.
fn posed(
    block: &MeshBlock,
    skeleton: &Skeleton,
    placements: Vec<Option<Placement>>,
    scales: &Scales,
    lights: Option<&(Lights, [i32; 3])>,
) -> (Drawn, usize) {
    let mut triangles = Drawn::default();
    // Where each mesh object stands and the light on it when its bone is lit; None for an
    // object that no placed bone draws.
    let mut places: Vec<Option<(Placement, Option<BoneLight>)>> = vec![None; block.objects.len()];
    for (bone, placement) in skeleton.bones.iter().zip(placements) {
        let object = usize::try_from(bone.mesh).ok();
        if let Some(place) = object.and_then(|object| places.get_mut(object)) {
            *place = placement.filter(|_| bone.is_drawn()).map(|placement| {
                let light = lights.filter(|_| bone.is_lit()).map(|(lights, stands)| {
                    // The bone's place in the room: where the model stands plus its own.
                    let at = std::array::from_fn(|k| stands[k] + placement.position[k]);
                    lights.on_bone(at, &placement.matrix)
                });
                // The object is drawn with the bone's matrix stretched by the scales; the
                // light is worked out with the unstretched one here (which of the two the
                // game's light routine is given is not read: T072).
                (scales.drawn(bone, &placement), light)
            });
        }
    }
    for (object, place) in block.objects.iter().zip(&places) {
        let Some((place, light)) = place else {
            continue;
        };
        for face in &object.faces {
            triangles.add_face(face, place, light.as_ref());
        }
    }
    for stitch in &block.stitches {
        let corners = stitch.map(|corner| {
            let (place, light) = places.get(corner.object as usize).copied().flatten()?;
            let vertex = block.seam_vertex(corner);
            Some(Point {
                position: to_bevy(place.place(vertex.position)),
                colour: corner_colour(
                    light.as_ref(),
                    Some(vertex.normal),
                    Some(vertex.colour),
                    vertex.texture.is_some(),
                ),
                uv: vertex.uv,
            })
        });
        if let [Some(a), Some(b), Some(c), Some(d)] = corners {
            let corners = [a, b, c, d];
            for surface in Surface::of(block.seam_vertex(stitch[0]).texture, false) {
                for triangle in QUAD {
                    (triangles.group(surface)).add(triangle.map(|index| corners[index]));
                }
            }
        }
    }
    let left_out = places.iter().filter(|place| place.is_none()).count();
    (triangles, left_out)
}

/// A piece of the model on screen: the triangles of one surface.
#[derive(Component)]
pub struct Piece;

/// A model that is played: what the next game frame needs to draw it again.
#[derive(Resource)]
pub struct Playing {
    loaded: Loaded,
    playback: Playback,
    /// The middle and the size of the model in its first pose, which the picture keeps.
    centre: Vec3,
    radius: f32,
    /// The material of every surface drawn so far.
    materials: Vec<(Surface, Handle<ModelMaterial>)>,
}

/// The material of a surface: the page as the console reads it out of video memory, or one
/// white texel for a surface without a texture. A texel of colour zero is not drawn. The
/// picture holds the console's colour values as they are, which is what the shader works in.
fn material(surface: Surface, vram: &Vram, images: &mut Assets<Image>) -> ModelMaterial {
    let (size, pixels) = match surface {
        Surface::Plain => (1, vec![u8::MAX; 4]),
        Surface::Textured(page) => (PAGE as u32, vram.page_rgba(page, false)),
        Surface::Blended { page, texels } => (PAGE as u32, vram.page_part(page, texels)),
    };
    let mix = surface.mix();
    ModelMaterial {
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
        share: Vec4::splat(mix.share()),
        mix,
    }
}

/// Where the triangles of a model go: the assets, the materials made so far and the place
/// and size of the model in the picture.
struct Stage<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    meshes: &'a mut Assets<Mesh>,
    images: &'a mut Assets<Image>,
    standard: &'a mut Assets<ModelMaterial>,
    materials: &'a mut Vec<(Surface, Handle<ModelMaterial>)>,
    vram: &'a Vram,
    centre: Vec3,
    radius: f32,
}

impl Stage<'_, '_, '_> {
    /// Puts the triangles on screen, one piece per surface, the model in the middle of the
    /// picture and scaled to a radius of one.
    fn put(&mut self, drawn: Drawn) {
        for (surface, group) in drawn.groups {
            let Triangles {
                positions,
                colours,
                uvs,
            } = group;
            let mesh = Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::RENDER_WORLD,
            )
            .with_inserted_attribute(
                Mesh::ATTRIBUTE_POSITION,
                positions
                    .iter()
                    .map(|&position| ((position - self.centre) / self.radius).to_array())
                    .collect::<Vec<_>>(),
            )
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colours)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
            let known = (self.materials.iter()).find(|(have, _)| *have == surface);
            let handle = match known {
                Some((_, handle)) => handle.clone(),
                None => {
                    let handle = (self.standard).add(material(surface, self.vram, self.images));
                    self.materials.push((surface, handle.clone()));
                    handle
                }
            };
            self.commands
                .spawn((Piece, Mesh3d(self.meshes.add(mesh)), MeshMaterial3d(handle)));
        }
    }
}

pub fn show_model(
    show: Res<ShowModel>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut standard: ResMut<Assets<ModelMaterial>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    let loaded = match load(&show) {
        Ok((lines, loaded)) => {
            lines.iter().for_each(|line| info!("{line}"));
            loaded
        }
        Err(err) => {
            error!("model file {}: {err}", show.file);
            exit.write(AppExit::error());
            return;
        }
    };
    let block = &loaded.block;
    // The state of an actor that plays the action or the whole animation, or stands in one
    // frame of it.
    let playback = loaded.pose.as_ref().map(|posing| {
        let (number, span) = (posing.number, posing.span);
        match (show.frame, show.fade, &posing.old) {
            (Some(frame), ..) => {
                let mut still = Playback::new(number, span, WHOLE as i16);
                still.counter = frame as i32;
                still.paused = true;
                still
            }
            (None, Some((number, frames)), Some(old)) => {
                let mut playback = Playback::new(number, Span::whole(old), WHOLE as i16);
                playback.start(number, span, Start::First, Some(frames));
                playback
            }
            _ => Playback::new(number, span, WHOLE as i16),
        }
    });
    let triangles = if let (Some(posing), Some(playback)) = (&loaded.pose, &playback) {
        let (triangles, left_out) = posed(
            block,
            &posing.skeleton,
            posing.placements(playback),
            &posing.scales(),
            loaded.lights.as_ref(),
        );
        if left_out > 0 {
            info!(
                "model file {}: {left_out} mesh objects are not drawn by a bone",
                show.file
            );
        }
        triangles
    } else {
        let objects = match show.object {
            Some(index) => match block.objects.get(index) {
                Some(object) => std::slice::from_ref(object),
                None => {
                    error!(
                        "model file {}: object {index} of {}",
                        show.file,
                        block.objects.len()
                    );
                    exit.write(AppExit::error());
                    return;
                }
            },
            None => &block.objects[..],
        };
        let mut triangles = Drawn::default();
        for face in objects.iter().flat_map(|object| &object.faces) {
            triangles.add_face(face, &Placement::IDENTITY, None);
        }
        triangles
    };
    let count = triangles.positions().count();
    if count == 0 {
        warn!("model file {}: no faces to show", show.file);
    }
    let (low, high) = triangles.positions().fold(
        (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
        |(low, high), position| (low.min(position), high.max(position)),
    );
    let centre = (low + high) / 2.0;
    let radius = ((high - low).length() / 2.0).max(1.0);
    info!(
        "model file {}: {} triangles from {low} to {high}",
        show.file,
        count / 3
    );
    for (surface, group) in &triangles.groups {
        info!(
            "model file {}: {} triangles drawn with {}",
            show.file,
            group.positions.len() / 3,
            match surface {
                Surface::Plain => "their own colours".to_string(),
                Surface::Textured(page) | Surface::Blended { page, .. } => format!(
                    "the {}-bit page at {:?}, palette at {:?}{}",
                    4 << page.depth(),
                    page.origin(),
                    page.palette_origin(),
                    match surface {
                        Surface::Blended { texels, .. } =>
                            format!(", the {texels:?} texels {:?}", surface.mix()),
                        _ => String::new(),
                    }
                ),
            }
        );
    }
    let mut materials = Vec::new();
    Stage {
        commands: &mut commands,
        meshes: &mut meshes,
        images: &mut images,
        standard: &mut standard,
        materials: &mut materials,
        vram: &loaded.vram,
        centre,
        radius,
    }
    .put(triangles);
    commands.spawn((
        Camera3d::default(),
        Camera {
            clear_color: ClearColorConfig::Custom(BACKDROP),
            ..default()
        },
        // Mixing works on the console's colour values, as the console does, and nothing
        // changes a colour afterwards.
        CompositingSpace::Srgb,
        Tonemapping::None,
        DebandDither::Disabled,
        Msaa::Off,
        Transform::from_xyz(0.0, 0.0, CAMERA_DISTANCE).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    if let Some(playback) = playback.filter(|playback| !playback.paused) {
        commands.insert_resource(Playing {
            loaded,
            playback,
            centre,
            radius,
            materials,
        });
    }
}

/// One game frame of a model that is played: the playback steps as an actor's does and the
/// model is drawn again in its new pose.
pub fn play_model(
    playing: Option<ResMut<Playing>>,
    pieces: Query<Entity, With<Piece>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut standard: ResMut<Assets<ModelMaterial>>,
    mut commands: Commands,
) {
    let Some(mut playing) = playing else { return };
    let playing = &mut *playing;
    let Some(posing) = &playing.loaded.pose else {
        return;
    };
    playing.playback.step();
    let placements = posing.placements(&playing.playback);
    let (triangles, _) = posed(
        &playing.loaded.block,
        &posing.skeleton,
        placements,
        &posing.scales(),
        playing.loaded.lights.as_ref(),
    );
    for piece in &pieces {
        commands.entity(piece).despawn();
    }
    Stage {
        commands: &mut commands,
        meshes: &mut meshes,
        images: &mut images,
        standard: &mut standard,
        materials: &mut playing.materials,
        vram: &playing.loaded.vram,
        centre: playing.centre,
        radius: playing.radius,
    }
    .put(triangles);
}
