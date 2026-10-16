
use std::sync::Arc;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use jc_formats::mixer::WHOLE;

pub use crate::movie_sound::Gain;
use crate::music::Music;
use crate::prefs::{self, Prefs, PrefsFile};

/// The highest value of a volume of the settings.
pub const LOUDEST: u32 = 100;

/// The gains of the lines to the sound device: one for the music and the sounds, one for
/// the voices, one for the movies.
#[derive(Resource, Default)]
pub struct Loudness {
    pub music: Arc<Gain>,
    pub voices: Arc<Gain>,
    pub movies: Arc<Gain>,
}

/// A volume of the settings as a share of the whole: 0 to 1.
pub fn share(volume: u32) -> f32 {
    volume.min(LOUDEST) as f32 / LOUDEST as f32
}

/// A volume of the settings as the chip's mix has a share, of [`WHOLE`].
pub fn chip_share(volume: u32) -> u16 {
    (volume.min(LOUDEST) * u32::from(WHOLE) / LOUDEST) as u16
}

/// What the window in the background does in this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Away {
    /// The game stands and its lines are held.
    pub paused: bool,
    /// Nothing is heard.
    pub silent: bool,
}

/// What the window in the background does: `background` is a play session whose window
/// is not in front.
pub fn away(background: bool, display: &prefs::Display) -> Away {
    let paused = background && display.pause_on_focus_loss;
    Away {
        paused,
        silent: paused || (background && display.mute_on_focus_loss),
    }
}

/// The gains of the music's line, the voices' and the movies', from the volumes of the
/// settings; nothing while the game is silent.
pub fn gains(audio: &prefs::Audio, silent: bool) -> [f32; 3] {
    if silent {
        return [0.0; 3];
    }
    let master = share(audio.master);
    [
        master,
        master * share(audio.voices),
        master * share(audio.movies),
    ]
}

/// The gains of the lines and the pace's clock.
pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Loudness>()
            .add_systems(PreUpdate, follow.before(crate::pace::set_speed));
    }
}

/// The volumes of the settings and the window in front or not reach the lines, the
/// chip's mix and the game's clock.
pub fn follow(
    prefs: Res<Prefs>,
    file: Res<PrefsFile>,
    windows: Query<&Window, With<PrimaryWindow>>,
    loudness: Res<Loudness>,
    music: Option<ResMut<Music>>,
    mut clock: ResMut<crate::pace::Clock>,
    mut was: Local<Away>,
) {
    let background = file.live() && windows.single().is_ok_and(|window| !window.focused);
    let now = away(background, &prefs.display);
    if now != *was {
        info!(
            "the window is {}: {}",
            if background {
                "in the background"
            } else {
                "in front"
            },
            match (now.paused, now.silent) {
                (true, _) => "the game stands and is silent",
                (false, true) => "the game goes on silent",
                (false, false) => "the game goes on and is heard",
            }
        );
        *was = now;
    }
    clock.set_away(now.paused);
    let [music_gain, voices, movies] = gains(&prefs.feel.audio, now.silent);
    loudness.music.set_share(music_gain);
    loudness.voices.set_share(voices);
    loudness.movies.set_share(movies);
    for gain in [&loudness.music, &loudness.voices, &loudness.movies] {
        gain.set_held(now.paused);
    }
    let audio = &prefs.feel.audio;
    let shares = (chip_share(audio.music), chip_share(audio.effects));
    if let Some(mut music) = music
        && music.shares() != shares
    {
        music.set_shares(shares);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_volume_is_a_share_of_the_whole() {
        assert_eq!(share(100), 1.0);
        assert_eq!(share(50), 0.5);
        assert_eq!(share(0), 0.0);
        assert_eq!(share(250), 1.0);
        assert_eq!(chip_share(100), WHOLE);
        assert_eq!(chip_share(50), WHOLE / 2);
        assert_eq!(chip_share(0), 0);
        assert_eq!(chip_share(1000), WHOLE);
    }

    #[test]
    fn the_master_multiplies_the_voices_and_the_movies() {
        let mut audio = prefs::Audio::default();
        assert_eq!(gains(&audio, false), [1.0; 3]);
        audio.master = 50;
        audio.voices = 50;
        audio.movies = 0;
        assert_eq!(gains(&audio, false), [0.5, 0.25, 0.0]);
        // The music and the effects are the chip's mix, not a line's.
        audio.music = 0;
        assert_eq!(gains(&audio, false)[0], 0.5);
        assert_eq!(gains(&audio, true), [0.0; 3]);
    }

    #[test]
    fn the_window_in_the_background_pauses_or_mutes_by_the_settings() {
        let mut display = prefs::Display::default();
        // In front nothing happens.
        assert_eq!(away(false, &display), Away::default());
        // The default: the game stands and is silent.
        let paused = Away {
            paused: true,
            silent: true,
        };
        assert_eq!(away(true, &display), paused);
        // The pause implies the silence, whatever the mute says.
        display.mute_on_focus_loss = true;
        assert_eq!(away(true, &display), paused);
        // Without the pause the game goes on, silent or heard.
        display.pause_on_focus_loss = false;
        let silent = Away {
            paused: false,
            silent: true,
        };
        assert_eq!(away(true, &display), silent);
        display.mute_on_focus_loss = false;
        assert_eq!(away(true, &display), Away::default());
    }

    /// A window that is not in front, in a run that keeps no settings (a check), never
    /// stands: the clock is not held and the lines are heard.
    #[test]
    fn a_check_never_pauses() {
        let mut app = App::new();
        app.insert_resource(Prefs::default())
            .insert_resource(prefs::at_start(false).1)
            .insert_resource(crate::pace::Clock::default())
            .init_resource::<Loudness>()
            .add_systems(Update, follow);
        app.world_mut().spawn((
            Window {
                focused: false,
                ..default()
            },
            PrimaryWindow,
        ));
        app.update();
        assert!(!app.world().resource::<crate::pace::Clock>().away());
        let loudness = app.world().resource::<Loudness>();
        assert!(!loudness.music.held() && loudness.music.share() == 1.0);
    }
}
