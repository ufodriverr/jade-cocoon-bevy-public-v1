
use std::sync::Arc;

use bevy::prelude::*;
use jc_formats::background::SCREEN;

use crate::field::FieldScreen;
use crate::prefs::Prefs;
use crate::prompts::{self, Glyph};
use crate::quick::Screens;
use crate::scripts::Takes;
use crate::shown::Things;

/// Pixels between the top of the hero and the bottom of the hint.
const ABOVE: f32 = 4.0;
/// Seconds of the fade in.
const FADE_IN: f64 = 0.15;
/// Seconds of one bob up and down, and how far it goes either way in pixels.
const BOB_PERIOD: f64 = 1.2;
const BOB: f64 = 1.0;
/// Where the hint lies on the depth axis of the field's screen: over every layer and
/// thing (0 and behind), the fade's white (4) and the black of a held picture (4.5),
/// under the black outside the draw area (5) and the windows (10).
const DEPTH: f32 = 4.8;

/// The hint's picture.
#[derive(Component)]
struct Hint;

/// The hint over the hero.
pub struct HintPlugin;

impl Plugin for HintPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, show_hint.after(crate::shown::place_things));
    }
}

/// What the hint showed last.
#[derive(Default)]
struct Seen {
    /// What the press would do, and the second of the real clock it was first seen.
    takes: Option<(Takes, f64)>,
    /// The picture the sprite has.
    glyph: Option<Arc<Glyph>>,
}

/// What a press would do, in words for the log.
fn said(takes: Takes) -> String {
    match takes {
        Takes::Talk(script) => format!("start a talk (script {script} waits for it)"),
        Takes::Gate => "tell a gate".into(),
        Takes::Passage => "tell a bank of a passage".into(),
        Takes::Lock => "tell a lock".into(),
        Takes::Door => "tell a door".into(),
        Takes::Search => "search a thing".into(),
        Takes::SaveLight => "open the save screen at the light".into(),
        Takes::Spot => "tell a spot".into(),
        Takes::Switch => "tell a switch".into(),
    }
}

/// The hint's place on the field's screen (the middle of the screen at the origin, y
/// up) for the top of the hero seen at `top` (pixels right of and below the projection
/// centre), the scroll offset, the picture's size and the bob: the middle of a picture
/// whose corners lie on whole pixels, kept inside the console's screen.
fn place(top: Vec2, scroll: IVec2, size: Vec2, bob: f32) -> Vec2 {
    let half = Vec2::new(SCREEN.0 as f32, SCREEN.1 as f32) / 2.0;
    let left = (scroll.x as f32 + top.x - size.x / 2.0).round();
    let bottom = (-(scroll.y as f32) - top.y + ABOVE + bob).round();
    let left = left.clamp(-half.x, half.x - size.x);
    let bottom = bottom.clamp(-half.y, half.y - size.y);
    Vec2::new(left, bottom) + size / 2.0
}

/// Shows the hint while a press would do something, and takes it away otherwise.
#[allow(clippy::too_many_arguments)]
fn show_hint(
    prefs: Res<Prefs>,
    (real, glide): (Res<Time<Real>>, Res<crate::smooth::Glide>),
    (things, screen): (Option<Res<Things>>, Option<Res<FieldScreen>>),
    screens: Screens,
    mut hints: Query<(Entity, &mut Sprite, &mut Transform), With<Hint>>,
    mut images: ResMut<Assets<Image>>,
    mut seen: Local<Seen>,
    mut commands: Commands,
) {
    let now = real.elapsed_secs_f64();
    let open =
        prefs.feel.prompts.interaction_hint && screens.may_leave() && screens.none_over_the_room();
    let things = things.filter(|_| open);
    let takes = things.as_ref().and_then(|things| things.confirm_takes());
    if takes != seen.takes.map(|(takes, _)| takes) {
        match takes {
            Some(takes) => info!("the interaction hint: a press would {}", said(takes)),
            None if seen.takes.is_some() => info!("the interaction hint goes"),
            None => {}
        }
        seen.takes = takes.map(|takes| (takes, now));
    }
    let top = takes.and(
        things
            .as_ref()
            .and_then(|things| things.hero_top(glide.share)),
    );
    let (Some(top), Some(screen), Some(glyph)) = (top, screen, prompts::confirm()) else {
        for (entity, ..) in &hints {
            commands.entity(entity).despawn();
        }
        return;
    };
    let since = seen.takes.map_or(now, |(_, since)| since);
    let alpha = ((now - since) / FADE_IN).clamp(0.0, 1.0) as f32;
    let bob = (BOB * (std::f64::consts::TAU * (now - since) / BOB_PERIOD).sin()) as f32;
    let size = Vec2::new(glyph.wide as f32, glyph.high as f32);
    // Between two game frames the picture glides (`smooth.rs`), the hint with it.
    let at = place(top + screen.glide, screen.scroll, size, bob).extend(DEPTH);
    let colour = Color::srgba(1.0, 1.0, 1.0, alpha);
    let new = !seen
        .glyph
        .as_ref()
        .is_some_and(|had| Arc::ptr_eq(had, &glyph));
    match hints.iter_mut().next() {
        Some((_, mut sprite, mut transform)) => {
            if new {
                sprite.image = images.add(glyph.image());
            }
            sprite.color = colour;
            transform.set_if_neq(Transform::from_translation(at));
        }
        None => {
            let mut sprite = Sprite::from_image(images.add(glyph.image()));
            sprite.color = colour;
            commands.spawn((Hint, sprite, Transform::from_translation(at)));
        }
    }
    seen.glyph = Some(glyph);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hint_lies_over_the_head_on_whole_pixels_and_inside_the_screen() {
        let size = Vec2::new(12.0, 12.0);
        // The top of the hero 40 pixels above the projection centre, no scroll: the
        // picture's bottom 4 pixels above it.
        let at = place(Vec2::new(0.3, -40.0), IVec2::ZERO, size, 0.0);
        assert_eq!(at, Vec2::new(0.0, 50.0));
        // The scroll moves it with the room, the bob a pixel.
        let at = place(Vec2::new(0.0, -40.0), IVec2::new(10, -20), size, 1.0);
        assert_eq!(at, Vec2::new(10.0, 71.0));
        // An odd width keeps the corners on whole pixels.
        let at = place(Vec2::ZERO, IVec2::ZERO, Vec2::new(9.0, 12.0), 0.0);
        assert_eq!(at.x - 4.5, (at.x - 4.5).round());
        // A head above the top of the screen keeps the hint on it.
        let at = place(Vec2::new(0.0, -200.0), IVec2::ZERO, size, 0.0);
        assert_eq!(at.y, 120.0 - 6.0);
        let at = place(Vec2::new(400.0, 0.0), IVec2::ZERO, size, 0.0);
        assert_eq!(at.x, 160.0 - 6.0);
    }

    #[test]
    fn every_press_is_said_in_words() {
        assert_eq!(said(Takes::Door), "tell a door");
        assert!(said(Takes::Talk(3)).contains("script 3"));
    }
}
