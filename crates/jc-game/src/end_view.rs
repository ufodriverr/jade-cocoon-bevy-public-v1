
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use jc_disc::{Archive, DiscImage, LocalConfig};
use jc_formats::ending;
use jc_formats::tim;

use crate::name_entry::{Picture, SCREEN, WHOLE, blit};
use crate::shown::Things;

type Failure = Box<dyn std::error::Error>;

/// The layer of what stands over a room.
const OVER_THE_ROOM: usize = 4;

/// The picture while it is on screen.
#[derive(Resource)]
pub struct EndPicture {
    /// The two halves on a black screen, as the file has them.
    rgba: Vec<u8>,
    /// The colour byte the picture on screen was drawn with.
    colour: u8,
    shown: Entity,
}

/// What belongs to the picture, for the day something takes it away.
#[derive(Component)]
pub struct OverTheRoom;

/// The two halves of the picture of `file`, each on its place of a black screen.
fn read(file: usize) -> Result<Vec<u8>, Failure> {
    let config = LocalConfig::load()?;
    let mut disc = DiscImage::open(&config.disc_image)?;
    let bytes = Archive::open(&mut disc)?.read(&mut disc, file)?;
    let set = tim::texture_set(&bytes).ok_or("the file is not a texture set")?;
    let mut screen = [0, 0, 0, 255].repeat(SCREEN.0 * SCREEN.1);
    for (image, x, y) in ending::HALVES {
        let picture = Picture::of(set.get(image).ok_or("the set has no such image")?)?;
        let size = (picture.wide, picture.high);
        blit(&mut screen, &picture, (0, 0), size, (x, y), WHOLE);
    }
    Ok(screen)
}

/// The picture under a colour byte, as a sprite of the console is drawn under one: each
/// part of a texel times the byte over 128.
fn under(rgba: &[u8], colour: u8) -> Vec<u8> {
    let lit = |part: u8| {
        (u32::from(part) * u32::from(colour) / u32::from(ending::AS_IT_IS)).min(255) as u8
    };
    (rgba.as_chunks::<4>().0.iter())
        .flat_map(|texel| [lit(texel[0]), lit(texel[1]), lit(texel[2]), 255])
        .collect()
}

/// Puts the last picture over the room once the room's script has read it, with a
/// camera of its own above the room's, and keeps its colour at the one of the room's
/// game frame. The room's script stays on the call that draws it, so nothing takes
/// the picture away but the room's own end.
pub fn run_ending(
    things: Option<Res<Things>>,
    current: Option<ResMut<EndPicture>>,
    drawn: Query<Entity, With<OverTheRoom>>,
    mut sprites: Query<&mut Sprite>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
) {
    let Some((file, colour)) = things.and_then(|things| things.ending()) else {
        if current.is_some() {
            for entity in &drawn {
                commands.entity(entity).despawn();
            }
            commands.remove_resource::<EndPicture>();
        }
        return;
    };
    let (wide, high) = (SCREEN.0 as u32, SCREEN.1 as u32);
    let Some(mut current) = current else {
        let rgba = match read(file) {
            Ok(rgba) => rgba,
            Err(err) => {
                error!("the last picture, file {file}: {err}");
                return;
            }
        };
        info!("the last picture, file {file}, comes up over the room with the colour {colour}");
        let shown =
            crate::spawn_picture(&mut commands, &mut images, wide, high, under(&rgba, colour));
        let layer = RenderLayers::layer(OVER_THE_ROOM);
        commands.entity(shown).insert((OverTheRoom, layer.clone()));
        commands.spawn((
            OverTheRoom,
            Camera2d,
            Camera {
                order: 1,
                clear_color: ClearColorConfig::Custom(Color::BLACK),
                ..default()
            },
            // Two cameras of one window must agree on this, and the room's has none.
            Msaa::Off,
            layer,
        ));
        commands.insert_resource(EndPicture {
            rgba,
            colour,
            shown,
        });
        return;
    };
    if current.colour == colour {
        return;
    }
    current.colour = colour;
    if colour == ending::AS_IT_IS {
        info!("the last picture is as it is and stays: the end of the game");
    }
    if let Ok(mut sprite) = sprites.get_mut(current.shown) {
        let image = crate::picture_image(wide, high, under(&current.rgba, colour));
        sprite.image = images.add(image);
    }
}
