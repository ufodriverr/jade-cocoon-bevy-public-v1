
use bevy::prelude::*;
use jc_formats::options_screen;

use crate::{music, shown};

/// The settings of the running app.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings(pub options_screen::Settings);

impl Default for Settings {
    fn default() -> Self {
        Self(options_screen::Settings::at_boot())
    }
}

/// The setting word reaches its readers: the music and the room on screen. Only a word
/// that differs is written, so that nothing counts as changed in a frame without a change.
pub fn reach(
    settings: Res<Settings>,
    mut music: ResMut<music::Music>,
    things: Option<ResMut<shown::Things>>,
) {
    let word = settings.0.word;
    if music.setting() != word {
        info!("the settings: the music has the setting word {word:#010X}");
        music.set_setting(word);
    }
    if let Some(mut things) = things
        && things.setting().is_some_and(|has| has != word)
    {
        info!("the settings: the room has the setting word {word:#010X}");
        things.set_setting(word);
    }
}
