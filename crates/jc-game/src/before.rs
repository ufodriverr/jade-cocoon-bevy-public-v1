
use bevy::asset::uuid_handle;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{CompositingSpace, RenderTarget};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, TextureFormat};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dPlugin};
use jc_formats::background::SCREEN;

use crate::field::{self, Fitted, ScreenCamera};
use crate::shown::{self, Things};

const BEFORE_SHADER: Handle<Shader> = uuid_handle!("0c5b1f5a-8a52-4c1d-9b56-6a0e7f3d2b18");

/// The layers of the two squares, each seen by its own camera only. Layer 0 is the room,
/// layer 1 a movie over the room (`movie_view.rs`).
const LAYING: usize = 2;
const SHOWING: usize = 3;
/// The order of the cameras while the picture before is on: the room, the laying and the
/// showing, which has the order the room's camera has without it.
const ROOM_FIRST: isize = -2;
const LAYING_SECOND: isize = -1;
const ROOM_ALONE: isize = 0;

pub struct BeforePlugin;

impl Plugin for BeforePlugin {
    fn build(&self, app: &mut App) {
        let shader = Shader::from_wgsl(include_str!("before.wgsl"), "before.wgsl");
        app.world_mut()
            .resource_mut::<Assets<Shader>>()
            .insert(&BEFORE_SHADER, shader)
            .expect("a fixed handle takes an asset");
        app.add_plugins(Material2dPlugin::<BeforeMaterial>::default())
            .add_systems(Update, lay.after(shown::draw_things));
    }
}

/// Lays `before` over `now` by the three rules, or shows `now` as it is.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct BeforeMaterial {
    #[texture(0)]
    #[sampler(1)]
    now: Handle<Image>,
    #[texture(2)]
    #[sampler(3)]
    before: Handle<Image>,
    /// x: 0 lays, 1 shows.
    #[uniform(4)]
    way: Vec4,
}

impl Material2d for BeforeMaterial {
    fn fragment_shader() -> ShaderRef {
        BEFORE_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        // The alpha is written as the shader gives it: it is the mask bit.
        AlphaMode2d::Opaque
    }
}

/// On the camera that lays the picture before: the kept pictures and whose turn it is.
#[derive(Component)]
pub struct Laying {
    kept: [Handle<Image>; 2],
    /// The kept picture that was drawn last, the one on screen.
    now: usize,
    /// The game frame that was laid last.
    laid: u32,
    /// How many were.
    frames: u32,
    lays: Handle<BeforeMaterial>,
    shows: Handle<BeforeMaterial>,
}

/// The camera that shows the kept picture in the window.
#[derive(Component)]
pub struct Shows;

/// What goes when the picture before does.
#[derive(Component)]
pub struct Part;

type Pictures<'w> = (
    ResMut<'w, Assets<Image>>,
    ResMut<'w, Assets<Mesh>>,
    ResMut<'w, Assets<BeforeMaterial>>,
);
type RoomCamera = (With<ScreenCamera>, Without<Laying>, Without<Shows>);
type LayingCamera = (Without<ScreenCamera>, Without<Shows>);
type ShowingCamera = (With<Shows>, Without<ScreenCamera>, Without<Laying>);

/// Starts, steps and ends the picture before with the scripts of the room. The kept
/// pictures take turns once per game frame. In the other frames of the window the same
/// game frame is laid again over the same picture before, which gives the same picture
/// (and the right one in the end when the first try came before the graphics card had the
/// room's pictures).
fn lay(
    things: Option<Res<Things>>,
    mut commands: Commands,
    (mut images, mut meshes, mut materials): Pictures,
    mut room: Query<(Entity, &mut Camera), RoomCamera>,
    mut laying: Query<(Entity, &mut Laying, &mut RenderTarget), LayingCamera>,
    showing: Query<&Camera, ShowingCamera>,
    parts: Query<Entity, With<Part>>,
) {
    let frame = things.as_ref().and_then(|things| things.picture_before());
    match (frame, laying.single_mut()) {
        (Some(frame), Err(_)) => {
            let Ok((entity, mut camera)) = room.single_mut() else {
                return;
            };
            let (across, down) = (SCREEN.0 as u32, SCREEN.1 as u32);
            let mut picture = || {
                images.add(Image::new_target_texture(
                    across,
                    down,
                    TextureFormat::Rgba8UnormSrgb,
                    None,
                ))
            };
            let (drawn, kept) = (picture(), [picture(), picture()]);
            let square = meshes.add(Rectangle::new(across as f32, down as f32));
            // The first frame has nothing before it. HYPOTHESIS: on the console it has
            // whatever the other buffer held, which is not known here.
            let lays = materials.add(BeforeMaterial {
                now: drawn.clone(),
                before: kept[1].clone(),
                way: Vec4::ZERO,
            });
            let shows = materials.add(BeforeMaterial {
                now: kept[0].clone(),
                before: kept[0].clone(),
                way: Vec4::X,
            });
            // The room goes into its picture, whole; the window's part goes to the
            // camera that shows the kept picture.
            let viewport = camera.viewport.take();
            camera.order = ROOM_FIRST;
            commands
                .entity(entity)
                .remove::<Fitted>()
                .insert(RenderTarget::Image(drawn.into()));
            commands.spawn((
                Camera2d,
                Camera {
                    order: LAYING_SECOND,
                    ..default()
                },
                RenderTarget::Image(kept[0].clone().into()),
                CompositingSpace::Srgb,
                Msaa::Off,
                field::screen_projection(),
                RenderLayers::layer(LAYING),
                Laying {
                    kept,
                    now: 0,
                    laid: frame,
                    frames: 1,
                    lays: lays.clone(),
                    shows: shows.clone(),
                },
            ));
            commands.spawn((
                Part,
                Mesh2d(square.clone()),
                MeshMaterial2d(lays),
                RenderLayers::layer(LAYING),
            ));
            commands.spawn((
                Part,
                Shows,
                Fitted,
                Camera2d,
                Camera {
                    order: ROOM_ALONE,
                    viewport,
                    ..default()
                },
                CompositingSpace::Srgb,
                Msaa::Off,
                field::screen_projection(),
                RenderLayers::layer(SHOWING),
            ));
            commands.spawn((
                Part,
                Mesh2d(square),
                MeshMaterial2d(shows),
                RenderLayers::layer(SHOWING),
            ));
            info!("the picture before: kept from game frame {frame} on");
        }
        (Some(frame), Ok((_, mut state, mut target))) => {
            if state.laid == frame {
                return;
            }
            state.laid = frame;
            state.frames += 1;
            state.now = 1 - state.now;
            let (new, old) = (
                state.kept[state.now].clone(),
                state.kept[1 - state.now].clone(),
            );
            if let Some(mut lays) = materials.get_mut(&state.lays) {
                lays.before = old;
            }
            if let Some(mut shows) = materials.get_mut(&state.shows) {
                shows.now = new.clone();
                shows.before = new.clone();
            }
            *target = RenderTarget::Image(new.into());
        }
        (None, Ok((entity, state, ..))) => {
            info!(
                "the picture before: dropped behind {} game frames",
                state.frames
            );
            if let Ok((room, mut camera)) = room.single_mut() {
                camera.order = ROOM_ALONE;
                camera.viewport = showing
                    .single()
                    .ok()
                    .and_then(|shows| shows.viewport.clone());
                commands
                    .entity(room)
                    .insert((Fitted, RenderTarget::default()));
            }
            commands.entity(entity).despawn();
            for part in &parts {
                commands.entity(part).despawn();
            }
        }
        (None, Err(_)) => {}
    }
}
