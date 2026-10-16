
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

use crate::display::{self, Frame, Presents};

/// Seconds a notice stands, the last [`FADES`] of them fading.
pub const SHOWN: f64 = 2.0;
const FADES: f64 = 0.4;

/// The notice on the window, if any.
#[derive(Resource, Debug, Default)]
pub struct Notices {
    /// The line, and the second of the real clock it was said at; the second is set
    /// when it is first drawn.
    said: Option<(String, Option<f64>)>,
    /// A new line was said since the last drawing.
    new: bool,
}

impl Notices {
    /// Puts `text` on the window for a while, in place of what stands there.
    pub fn say(&mut self, text: impl Into<String>) {
        let text = text.into();
        info!("the notice on the window: {text}");
        self.said = Some((text, None));
        self.new = true;
    }
}

/// How visible a notice is `age` seconds after it was said: whole, then fading, then
/// gone.
fn seen(age: f64) -> f32 {
    ((SHOWN - age) / FADES).clamp(0.0, 1.0) as f32
}

/// The notices over the window.
pub struct NoticePlugin;

impl Plugin for NoticePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Notices>()
            .add_systems(PostUpdate, show_notices.after(display::Fitting));
    }
}

/// A part of the notice: its box or its text.
#[derive(Component)]
enum Part {
    Box,
    Text,
}

/// Draws the notice in the top left corner, made anew when it or the window's size
/// changes, and fades it.
fn show_notices(
    real: Res<Time<Real>>,
    frame: Option<Res<Frame>>,
    mut notices: ResMut<Notices>,
    mut parts: Query<(Entity, &Part, Option<&mut Sprite>, Option<&mut TextColor>)>,
    mut made: Local<Option<Vec2>>,
    mut commands: Commands,
) {
    let now = real.elapsed_secs_f64();
    let window = frame.and_then(|frame| frame.window());
    let notices = &mut *notices;
    if let Some((_, at)) = &mut notices.said {
        at.get_or_insert(now);
    }
    let age = (notices.said.as_ref())
        .and_then(|(_, at)| *at)
        .map(|at| now - at);
    if age.is_some_and(|age| age >= SHOWN) {
        notices.said = None;
    }
    // A screen that took everything away may have taken the notice too.
    let gone = made.is_some() && parts.is_empty();
    let wanted = window.filter(|_| notices.said.is_some());
    if std::mem::take(&mut notices.new) || *made != wanted || gone {
        for (entity, ..) in &parts {
            commands.entity(entity).despawn();
        }
        *made = wanted;
        let (Some(window), Some((text, _))) = (wanted, &notices.said) else {
            return;
        };
        // A unit: a 36th of the window's height, at least 12 pixels, as the marker's.
        let unit = (window.y / 36.0).max(12.0);
        let size = 0.9 * unit;
        let wide = text.chars().count() as f32 * 0.6 * size + 1.4 * unit;
        let boxed = Vec2::new(wide, 1.6 * unit);
        let margin = 0.6 * unit;
        let middle = Vec2::new(-window.x, window.y) / 2.0
            + Vec2::new(margin + boxed.x / 2.0, -margin - boxed.y / 2.0);
        let layer = RenderLayers::layer(display::OVER_THE_WINDOW);
        commands.spawn((
            Part::Box,
            Presents,
            Sprite::from_color(Color::srgba(0.0, 0.0, 0.0, 0.6), boxed),
            Transform::from_translation(middle.extend(5.0)),
            layer.clone(),
        ));
        commands.spawn((
            Part::Text,
            Presents,
            Text2d::new(text.clone()),
            TextFont::from_font_size(size),
            TextColor(Color::WHITE),
            Transform::from_translation(middle.extend(6.0)),
            layer,
        ));
        return;
    }
    let seen = age.map_or(1.0, seen);
    for (_, part, sprite, colour) in &mut parts {
        match (part, sprite, colour) {
            (Part::Box, Some(mut sprite), _) => sprite.color.set_alpha(0.6 * seen),
            (Part::Text, _, Some(mut colour)) => colour.0.set_alpha(seen),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_notice_stands_and_then_fades() {
        assert_eq!(seen(0.0), 1.0);
        assert_eq!(seen(SHOWN - FADES), 1.0);
        assert!(seen(SHOWN - FADES / 2.0) > 0.4 && seen(SHOWN - FADES / 2.0) < 0.6);
        assert_eq!(seen(SHOWN), 0.0);
        let mut notices = Notices::default();
        notices.say("Saved");
        notices.say("Loaded");
        assert_eq!(notices.said, Some(("Loaded".into(), None)));
        assert!(notices.new);
    }
}
