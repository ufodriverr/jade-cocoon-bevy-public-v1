
use bevy::input::gamepad::GamepadButton;
use bevy::prelude::*;
use jc_disc::Movement;
use jc_formats::card_screen::wide_unit;
use jc_formats::menu::{Rect, Style};
use jc_formats::options_panel::{HELP_BOX, HELP_TEXT};
use jc_formats::pad::{self, Pad};
use jc_formats::player_menu::{SOUND_CANCEL, SOUND_CONFIRM, SOUND_CURSOR, SOUND_REFUSED};
use jc_formats::text::Font;

use crate::assist::{Encounters, Respawn};
use crate::bindings::{self, Action, Table};
use crate::display::{Content, Filter, Mode, RESOLUTIONS, Render};
use crate::gamepad::{Device, InputDevice, Pads};
use crate::name_entry::{Picture, WHOLE};
use crate::options_view::{Parts, glyphs_lit, put_sign};
use crate::pointer::Mouse;
use crate::prefs::{self, ControlPrefs, Prefs};
use crate::prompts::{self, Style as PromptStyle};

/// Seconds "Keep these settings?" waits before it puts the settings before back.
const KEEP_SECONDS: f64 = 10.0;
/// Seconds a rebinding waits for a key or a button.
const CAPTURE_SECONDS: f64 = 10.0;
/// Seconds a line of what changed stays in the line of help.
const NOTICE_SECONDS: f64 = 4.0;
/// Seconds into a rebinding a check presses its `--pc-key`.
const CHECK_KEY_AFTER: f64 = 1.0;

/// The keys of the build that no action may take: the speed, a restart, the quick save
/// and load, the full screen.
const KEYS_OF_THE_BUILD: [KeyCode; 6] = [
    KeyCode::F1,
    KeyCode::F2,
    KeyCode::F5,
    KeyCode::F6,
    KeyCode::F9,
    KeyCode::F11,
];

/// The signs of the pad's buttons in the menus' sprite pool (`prompts.rs`): the cross,
/// the triangle, L1 and R1.
const SIGN_CROSS: usize = 16;
const SIGN_TRIANGLE: usize = 167;
const SIGN_L1: usize = 99;
const SIGN_R1: usize = 153;

/// The place of things on the console's screen: the tabs, the rows (their names and
/// their values), the box of a question.
const TAB_TOP: i16 = 10;
const TAB_LEFT: i16 = 12;
const TAB_WIDTH: i16 = 72;
const TAB_STEP: i16 = 76;
const ROW_HEIGHT: i16 = 16;
const LIST_TOP: i16 = 34;
const ROW_STEP: i16 = 17;
/// Rows seen at once; a longer page scrolls.
const VISIBLE: usize = 9;
const NAME_LEFT: i16 = 12;
const NAME_WIDTH: i16 = 136;
const VALUE_LEFT: i16 = 150;
const VALUE_WIDTH: i16 = 148;
/// A row of the controls: the key's half of the value and the button's.
const KEY_WIDTH: i16 = 72;
/// The scroll signs right of the rows.
const SIGNS_X: i16 = 308;
const QUESTION: Rect = Rect {
    x: 40,
    y: 76,
    w: 240,
    h: 66,
};
const ANSWER_WIDTH: i16 = 64;
/// A frame of the menus: plain, under the cursor (it pulses), marked.
const PLAIN: u8 = 0;
const CURSOR: u8 = 1;
const MARKED: u8 = 2;
/// A text at half its brightness: a row that does nothing with the other settings.
const DIM: [u8; 3] = [72; 3];

/// A page of the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Display,
    Sound,
    Gameplay,
    Controls,
}

impl Page {
    const ALL: [Page; 4] = [Page::Display, Page::Sound, Page::Gameplay, Page::Controls];

    /// The page of `--pc-settings=`.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "display" => Some(Page::Display),
            "sound" => Some(Page::Sound),
            "gameplay" => Some(Page::Gameplay),
            "controls" => Some(Page::Controls),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Page::Display => "Display",
            Page::Sound => "Sound",
            Page::Gameplay => "Gameplay",
            Page::Controls => "Controls",
        }
    }

    fn number(self) -> usize {
        Self::ALL.iter().position(|page| *page == self).unwrap_or(0)
    }

    /// The page `step` tabs on, round.
    fn turned(self, step: i32) -> Self {
        let count = Self::ALL.len() as i32;
        Self::ALL[(self.number() as i32 + step).rem_euclid(count) as usize]
    }

    /// The rows of the page.
    fn items(self) -> Vec<Item> {
        use Item::*;
        match self {
            Page::Display => vec![
                DisplayMode,
                Resolution,
                Vsync,
                FrameCap,
                SmoothMotion,
                FourThree,
                Integer,
                WideFights,
                Rendering,
                Msaa,
                Crt,
                Dither,
                PauseAway,
                MuteAway,
            ],
            Page::Sound => vec![Master, Music, Effects, Voices, Movies],
            Page::Gameplay => vec![
                FightSpeed,
                FastForward,
                SceneSkip,
                Autosave,
                Exp,
                Money,
                Respawning,
                Roamers,
                Prompts,
                Hint,
            ],
            Page::Controls => {
                let mut items = vec![Steering, AlwaysRun, RunToggle];
                items.extend(Action::ALL.map(Bind));
                items.push(Reset);
                items
            }
        }
    }
}

/// A row of a page: a setting of [`Prefs`], an action to bind, or the reset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Item {
    DisplayMode,
    Resolution,
    Vsync,
    FrameCap,
    SmoothMotion,
    FourThree,
    Integer,
    WideFights,
    Rendering,
    Msaa,
    Crt,
    Dither,
    PauseAway,
    MuteAway,
    Master,
    Music,
    Effects,
    Voices,
    Movies,
    FightSpeed,
    FastForward,
    SceneSkip,
    Autosave,
    Exp,
    Money,
    Respawning,
    Roamers,
    Prompts,
    Hint,
    Steering,
    AlwaysRun,
    RunToggle,
    Bind(Action),
    Reset,
}

/// What a row does with left, right and a confirm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Left and right change the value at once; a confirm steps it on, round.
    Value,
    /// Left and right choose a value, a confirm applies it and asks to keep it.
    Applied,
    /// A confirm waits for a key or a button.
    Bind,
    /// A confirm asks, then puts the controls back as they were at first.
    Reset,
}

/// The frame caps offered; 0 is none.
const FRAME_CAPS: [u32; 7] = [0, 30, 60, 120, 144, 165, 240];
/// The samples of the 3D's edges offered.
const SAMPLES: [u8; 4] = [0, 2, 4, 8];
/// The values of "Skip scenes", in their order.
const SCENE_SKIPS: [crate::skip::SceneSkip; 3] = [
    crate::skip::SceneSkip::Seen,
    crate::skip::SceneSkip::Always,
    crate::skip::SceneSkip::Off,
];
/// The multipliers of the assists offered.
const MULTIPLIERS: [f64; 6] = [0.5, 1.0, 1.5, 2.0, 3.0, 4.0];
/// A volume's steps: 0 to 100 by 5.
const VOLUME_STEP: u32 = 5;
const VOLUMES: usize = 21;

const OFF_ON: [&str; 2] = ["Off", "On"];

/// The place of the nearest of `offered` to `value`.
fn nearest<T: Copy>(offered: &[T], value: T, apart: impl Fn(T, T) -> f64) -> usize {
    (offered.iter().enumerate())
        .min_by(|(_, a), (_, b)| apart(**a, value).total_cmp(&apart(**b, value)))
        .map_or(0, |(at, _)| at)
}

impl Item {
    fn kind(self) -> Kind {
        match self {
            Item::DisplayMode | Item::Resolution => Kind::Applied,
            Item::Bind(_) => Kind::Bind,
            Item::Reset => Kind::Reset,
            _ => Kind::Value,
        }
    }

    /// The row's name.
    fn name(self) -> &'static str {
        match self {
            Item::DisplayMode => "Display mode",
            Item::Resolution => "Resolution",
            Item::Vsync => "VSync",
            Item::FrameCap => "Frame cap",
            Item::SmoothMotion => "Smooth motion",
            Item::FourThree => "4:3 screens",
            Item::Integer => "Integer scaling",
            Item::WideFights => "Wide fights",
            Item::Rendering => "Render",
            Item::Msaa => "Anti-aliasing",
            Item::Crt => "CRT filter",
            Item::Dither => "Dither",
            Item::PauseAway => "Pause in background",
            Item::MuteAway => "Mute in background",
            Item::Master => "Master volume",
            Item::Music => "Music",
            Item::Effects => "Effects",
            Item::Voices => "Voices",
            Item::Movies => "Movies",
            Item::FightSpeed => "Fight speed",
            Item::FastForward => "Fast forward speed",
            Item::SceneSkip => "Skip scenes",
            Item::Autosave => "Autosave",
            Item::Exp => "Experience",
            Item::Money => "Money",
            Item::Respawning => "Creatures come back",
            Item::Roamers => "Creatures in a room",
            Item::Prompts => "Button prompts",
            Item::Hint => "Interaction hint",
            Item::Steering => "Movement",
            Item::AlwaysRun => "Always run",
            Item::RunToggle => "Caps Lock walks",
            Item::Bind(action) => action_name(action),
            Item::Reset => "Reset to defaults",
        }
    }

    /// The line of help of the row: what it does, in one line of the box.
    fn about(self) -> &'static str {
        match self {
            Item::DisplayMode => "Borderless, exclusive fullscreen or window.",
            Item::Resolution => "Fullscreen's video mode, or the window size.",
            Item::Vsync => "Waits for the monitor: no tearing.",
            Item::FrameCap => "The most frames a second.",
            Item::SmoothMotion => "Motion drawn between the game's 25 steps.",
            Item::FourThree => "What fills the sides of a 4:3 screen.",
            Item::Integer => "Every pixel the same whole size.",
            Item::WideFights => "Fights take the whole wide screen.",
            Item::Rendering => "Sharp 3D, or the console's 320x240.",
            Item::Msaa => "Smoother edges of the 3D (Sharp only).",
            Item::Crt => "Scanlines, or an old TV's look.",
            Item::Dither => "The console's dither (Authentic only).",
            Item::PauseAway => "The game stands while another window is up.",
            Item::MuteAway => "Silent while another window is up.",
            Item::Master => "Everything the game sounds.",
            Item::Music => "The music.",
            Item::Effects => "Steps, hits and the menus' sounds.",
            Item::Voices => "The spoken lines.",
            Item::Movies => "The sound of the movies.",
            Item::FightSpeed => "How fast every fight runs.",
            Item::FastForward => "How fast the game runs while Ctrl is held.",
            Item::SceneSkip => "Hold Enter (Start) in a scene to skip it.",
            Item::Autosave => "Saves by itself at forests and bosses.",
            Item::Exp => "The experience of every won fight, times.",
            Item::Money => "Money given and sales, times.",
            Item::Respawning => "Beaten creatures come back to their room.",
            Item::Roamers => "Fewer roaming creatures in a new room.",
            Item::Prompts => "The buttons the lines of help show.",
            Item::Hint => "The confirm button over the hero.",
            Item::Steering => "Modern: the way you press. Tank: turns.",
            Item::AlwaysRun => "The directions run, Shift walks.",
            Item::RunToggle => "Caps Lock walks until pressed again.",
            Item::Bind(action) => action_about(action),
            Item::Reset => "All keys and buttons as they were at first.",
        }
    }

    /// How many values the row offers.
    fn count(self) -> usize {
        match self {
            Item::DisplayMode | Item::Crt => 3,
            Item::Resolution => RESOLUTIONS.len(),
            Item::FrameCap => FRAME_CAPS.len(),
            Item::FourThree | Item::Msaa | Item::Prompts => 4,
            Item::Master | Item::Music | Item::Effects | Item::Voices | Item::Movies => VOLUMES,
            Item::FightSpeed | Item::SceneSkip => 3,
            Item::FastForward => crate::pace::FAST_FORWARD_SPEEDS.len(),
            Item::Exp | Item::Money => MULTIPLIERS.len(),
            Item::Bind(_) | Item::Reset => 0,
            _ => 2,
        }
    }

    /// The volume of a row of the sound.
    fn volume(self, prefs: &mut Prefs) -> Option<&mut u32> {
        let audio = &mut prefs.feel.audio;
        match self {
            Item::Master => Some(&mut audio.master),
            Item::Music => Some(&mut audio.music),
            Item::Effects => Some(&mut audio.effects),
            Item::Voices => Some(&mut audio.voices),
            Item::Movies => Some(&mut audio.movies),
            _ => None,
        }
    }

    /// The volume of a row of the sound, as it is.
    fn volume_of(self, prefs: &Prefs) -> Option<u32> {
        let audio = &prefs.feel.audio;
        match self {
            Item::Master => Some(audio.master),
            Item::Music => Some(audio.music),
            Item::Effects => Some(audio.effects),
            Item::Voices => Some(audio.voices),
            Item::Movies => Some(audio.movies),
            _ => None,
        }
    }

    /// A switch of the row, on or off.
    fn switch(self, prefs: &mut Prefs) -> Option<&mut bool> {
        match self {
            Item::Vsync => Some(&mut prefs.display.vsync),
            Item::SmoothMotion => Some(&mut prefs.display.smooth_motion),
            Item::Integer => Some(&mut prefs.display.integer_scaling),
            Item::WideFights => Some(&mut prefs.display.wide_fights),
            Item::Dither => Some(&mut prefs.display.dither),
            Item::PauseAway => Some(&mut prefs.display.pause_on_focus_loss),
            Item::MuteAway => Some(&mut prefs.display.mute_on_focus_loss),
            Item::Autosave => Some(&mut prefs.feel.saves.autosave),
            Item::Hint => Some(&mut prefs.feel.prompts.interaction_hint),
            Item::AlwaysRun => Some(&mut prefs.controls.always_run),
            Item::RunToggle => Some(&mut prefs.controls.run_toggle),
            _ => None,
        }
    }

    /// A switch of the row, as it is.
    fn switch_of(self, prefs: &Prefs) -> Option<bool> {
        let display = &prefs.display;
        match self {
            Item::Vsync => Some(display.vsync),
            Item::SmoothMotion => Some(display.smooth_motion),
            Item::Integer => Some(display.integer_scaling),
            Item::WideFights => Some(display.wide_fights),
            Item::Dither => Some(display.dither),
            Item::PauseAway => Some(display.pause_on_focus_loss),
            Item::MuteAway => Some(display.mute_on_focus_loss),
            Item::Autosave => Some(prefs.feel.saves.autosave),
            Item::Hint => Some(prefs.feel.prompts.interaction_hint),
            Item::AlwaysRun => Some(prefs.controls.always_run),
            Item::RunToggle => Some(prefs.controls.run_toggle),
            _ => None,
        }
    }

    /// The value of the settings as one of the row's, the nearest one for a value the
    /// row does not offer (one written into the file by hand).
    fn chosen(self, prefs: &Prefs) -> usize {
        let display = &prefs.display;
        let feel = &prefs.feel;
        if let Some(on) = self.switch_of(prefs) {
            return usize::from(on);
        }
        if let Some(volume) = self.volume_of(prefs) {
            return ((volume + VOLUME_STEP / 2) / VOLUME_STEP).min(VOLUMES as u32 - 1) as usize;
        }
        let ratio = |a: f64, b: f64| (a - b).abs();
        match self {
            Item::DisplayMode => match display.mode {
                Mode::Borderless => 0,
                Mode::Fullscreen => 1,
                Mode::Windowed => 2,
            },
            Item::Resolution => {
                let size = display.resolution;
                let area = |size: crate::display::Size| f64::from(size.width * size.height);
                (RESOLUTIONS.iter().position(|offered| *offered == size))
                    .unwrap_or_else(|| nearest(&RESOLUTIONS, size, |a, b| ratio(area(a), area(b))))
            }
            Item::FrameCap => nearest(&FRAME_CAPS, display.frame_cap, |a, b| {
                ratio(f64::from(a), f64::from(b))
            }),
            Item::FourThree => match display.content {
                Content::Fill => 0,
                Content::Bars => 1,
                Content::Stretch => 2,
                Content::Zoom => 3,
            },
            Item::Rendering => usize::from(display.render == Render::Authentic),
            Item::Msaa => nearest(&SAMPLES, display.msaa, |a, b| {
                ratio(f64::from(a), f64::from(b))
            }),
            Item::Crt => match display.crt {
                Filter::Off => 0,
                Filter::Scanlines => 1,
                Filter::Crt => 2,
            },
            Item::FightSpeed => crate::pace::fight_speed(&feel.pace) as usize - 1,
            Item::FastForward => {
                let speed = crate::pace::fast_forward_speed(&feel.pace);
                let speeds = crate::pace::FAST_FORWARD_SPEEDS;
                speeds
                    .iter()
                    .position(|offered| *offered == speed)
                    .unwrap_or(0)
            }
            Item::SceneSkip => (SCENE_SKIPS.iter())
                .position(|offered| *offered == feel.pace.scene_skip)
                .unwrap_or(0),
            Item::Exp => nearest(&MULTIPLIERS, feel.assist.exp_multiplier, ratio),
            Item::Money => nearest(&MULTIPLIERS, feel.assist.money_multiplier, ratio),
            Item::Respawning => usize::from(feel.assist.respawn == Respawn::Always),
            Item::Roamers => usize::from(feel.assist.encounters == Encounters::Fewer),
            Item::Prompts => match feel.prompts.style {
                PromptStyle::Auto => 0,
                PromptStyle::Keyboard => 1,
                PromptStyle::Xbox => 2,
                PromptStyle::Playstation => 3,
            },
            Item::Steering => usize::from(prefs.controls.movement == Movement::Tank),
            _ => 0,
        }
    }

    /// The settings with the row's value `at`.
    fn set(self, prefs: &mut Prefs, at: usize) {
        let at = at.min(self.count().saturating_sub(1));
        if let Some(on) = self.switch(prefs) {
            *on = at == 1;
            return;
        }
        if let Some(volume) = self.volume(prefs) {
            *volume = at as u32 * VOLUME_STEP;
            return;
        }
        let display = &mut prefs.display;
        let feel = &mut prefs.feel;
        match self {
            Item::DisplayMode => {
                display.mode = [Mode::Borderless, Mode::Fullscreen, Mode::Windowed][at]
            }
            Item::Resolution => {
                display.resolution = RESOLUTIONS[at];
                // A window takes the size at once; the full screen keeps the window's
                // size for when it is a window again.
                if display.mode == Mode::Windowed {
                    display.window_size = Some(RESOLUTIONS[at]);
                }
            }
            Item::FrameCap => display.frame_cap = FRAME_CAPS[at],
            Item::FourThree => {
                display.content = [
                    Content::Fill,
                    Content::Bars,
                    Content::Stretch,
                    Content::Zoom,
                ][at]
            }
            Item::Rendering => display.render = [Render::Sharp, Render::Authentic][at],
            Item::Msaa => display.msaa = SAMPLES[at],
            Item::Crt => display.crt = [Filter::Off, Filter::Scanlines, Filter::Crt][at],
            Item::FightSpeed => feel.pace.fight_speed = crate::pace::FIGHT_SPEEDS[at],
            Item::FastForward => {
                feel.pace.fast_forward_speed = crate::pace::FAST_FORWARD_SPEEDS[at]
            }
            Item::SceneSkip => feel.pace.scene_skip = SCENE_SKIPS[at],
            Item::Exp => feel.assist.exp_multiplier = MULTIPLIERS[at],
            Item::Money => feel.assist.money_multiplier = MULTIPLIERS[at],
            Item::Respawning => feel.assist.respawn = [Respawn::Original, Respawn::Always][at],
            Item::Roamers => feel.assist.encounters = [Encounters::Normal, Encounters::Fewer][at],
            Item::Prompts => {
                feel.prompts.style = [
                    PromptStyle::Auto,
                    PromptStyle::Keyboard,
                    PromptStyle::Xbox,
                    PromptStyle::Playstation,
                ][at]
            }
            Item::Steering => prefs.controls.movement = [Movement::Modern, Movement::Tank][at],
            _ => {}
        }
    }

    /// The words of the row's value `at`.
    fn said(self, at: usize) -> String {
        let at = at.min(self.count().saturating_sub(1));
        let times = |speed: u32| format!("{speed}x");
        match self {
            Item::DisplayMode => ["Borderless", "Fullscreen", "Windowed"][at].into(),
            Item::Resolution => RESOLUTIONS[at].to_string(),
            Item::FrameCap => match FRAME_CAPS[at] {
                0 => "Off".into(),
                cap => cap.to_string(),
            },
            Item::FourThree => ["Fill", "Bars", "Stretch", "Zoom"][at].into(),
            Item::Rendering => ["Sharp", "Authentic"][at].into(),
            Item::Msaa => match SAMPLES[at] {
                0 => "Off".into(),
                samples => format!("{samples}x"),
            },
            Item::Crt => ["Off", "Scanlines", "CRT"][at].into(),
            Item::Master | Item::Music | Item::Effects | Item::Voices | Item::Movies => {
                (at as u32 * VOLUME_STEP).to_string()
            }
            Item::FightSpeed => times(crate::pace::FIGHT_SPEEDS[at]),
            Item::FastForward => times(crate::pace::FAST_FORWARD_SPEEDS[at]),
            Item::SceneSkip => ["Seen", "Always", "Off"][at].into(),
            Item::Exp | Item::Money => format!("x{}", MULTIPLIERS[at]),
            Item::Respawning => ["Original", "Always"][at].into(),
            Item::Roamers => ["Normal", "Fewer"][at].into(),
            Item::Prompts => ["Auto", "Keyboard", "Xbox", "PlayStation"][at].into(),
            Item::Steering => ["Modern", "Tank"][at].into(),
            Item::Bind(_) | Item::Reset => String::new(),
            _ => OFF_ON[at].into(),
        }
    }

    /// The words of the row's value as the settings have it: a value the row does not
    /// offer is shown as it is.
    fn shown(self, prefs: &Prefs) -> String {
        let at = self.chosen(prefs);
        let display = &prefs.display;
        let assist = &prefs.feel.assist;
        match self {
            Item::Resolution if RESOLUTIONS[at] != display.resolution => {
                display.resolution.to_string()
            }
            Item::FrameCap if FRAME_CAPS[at] != display.frame_cap => display.frame_cap.to_string(),
            Item::Exp if MULTIPLIERS[at] != assist.exp_multiplier => {
                format!("x{}", assist.exp_multiplier)
            }
            Item::Money if MULTIPLIERS[at] != assist.money_multiplier => {
                format!("x{}", assist.money_multiplier)
            }
            _ => self.said(at),
        }
    }

    /// Whether the row does nothing with the other settings as they are: drawn dim.
    fn idle(self, prefs: &Prefs) -> bool {
        let display = &prefs.display;
        match self {
            Item::Resolution => display.mode == Mode::Borderless,
            Item::Msaa => display.render == Render::Authentic,
            Item::Dither => display.render == Render::Sharp,
            Item::MuteAway => display.pause_on_focus_loss,
            _ => false,
        }
    }
}

/// The name of an action on its row.
fn action_name(action: Action) -> &'static str {
    match action {
        Action::Up => "Up",
        Action::Down => "Down",
        Action::Left => "Left",
        Action::Right => "Right",
        Action::Cross => "Confirm",
        Action::Circle => "Describe",
        Action::Triangle => "Cancel",
        Action::Square => "Square",
        Action::Start => "Start",
        Action::Select => "Select",
        Action::L1 => "L1",
        Action::R1 => "R1",
        Action::L2 => "L2",
        Action::R2 => "R2",
        Action::Menu => "Menu",
        Action::Run => "Walk, held",
        Action::RunToggle => "Walk, toggle",
        Action::FastForward => "Fast forward",
    }
}

/// What an action does, for the line of help.
fn action_about(action: Action) -> &'static str {
    match action {
        Action::Up | Action::Down | Action::Left | Action::Right => "A direction of the pad.",
        Action::Cross => "The cross: confirm, talk, open.",
        Action::Circle => "The circle: describe; the menu in a room.",
        Action::Triangle => "The triangle: cancel, back out.",
        Action::Square => "The square of the pad.",
        Action::Start => "Start: the title, skips a movie.",
        Action::Select => "Select of the pad.",
        Action::L1 => "L1 of the pad.",
        Action::R1 => "R1 of the pad.",
        Action::L2 => "L2 of the pad.",
        Action::R2 => "R2: turns the pages of the menus.",
        Action::Menu => "Opens the menu over a room, and closes it.",
        Action::Run => "Held: walk instead of run.",
        Action::RunToggle => "Walk until pressed again.",
        Action::FastForward => "Held: the game runs faster.",
    }
}

/// Where the cursor is: on the tabs or on a row of the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum At {
    Tabs,
    Row(usize),
}

/// A place of the screen the mouse can point at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spot {
    Tab(Page),
    /// A row's name, or the whole of a row of the controls.
    Row(usize),
    /// The left and the right half of a row's value.
    Less(usize),
    More(usize),
    Yes,
    No,
}

/// A question or a wait in front of the page.
#[derive(Debug, Clone, PartialEq)]
enum Dialog {
    /// "Keep these settings?" behind a new display mode or resolution: the display as it
    /// was before, until when the question waits.
    Keep {
        before: prefs::Display,
        until: f64,
        yes: bool,
    },
    /// "Reset all controls?"
    Reset { yes: bool },
    /// A key, or a button of the pad, is waited for.
    Capture {
        action: Action,
        device: Device,
        since: f64,
    },
}

impl Dialog {
    fn yes_mut(&mut self) -> Option<&mut bool> {
        match self {
            Dialog::Keep { yes, .. } | Dialog::Reset { yes } => Some(yes),
            Dialog::Capture { .. } => None,
        }
    }
}

/// What the mouse did since the last pass.
#[derive(Debug, Clone, Copy, Default)]
struct Pointed {
    under: Option<Spot>,
    moved: bool,
    clicked: bool,
    notches: i32,
}

/// The screen while it is up.
#[derive(Debug, Clone)]
struct Screen {
    page: Page,
    at: At,
    /// The first row seen.
    first: usize,
    /// A display mode or resolution chosen and not applied yet.
    pending: Option<usize>,
    dialog: Option<Dialog>,
    /// A line of what changed, in place of the line of help, until a second of the app.
    notice: Option<(String, f64)>,
    /// The pad is not read until nothing of it is held: the key that was just bound
    /// is still down.
    deaf: bool,
    pointed: Pointed,
    /// Sounds the systems between two passes asked for.
    sounds: Vec<u8>,
}

impl Screen {
    fn new(page: Page) -> Self {
        Self {
            page,
            at: At::Row(0),
            first: 0,
            pending: None,
            dialog: None,
            notice: None,
            deaf: false,
            pointed: Pointed::default(),
            sounds: Vec::new(),
        }
    }

    fn items(&self) -> Vec<Item> {
        self.page.items()
    }

    fn notice(&mut self, said: String, now: f64) {
        info!("the PC Settings: {said}");
        self.notice = Some((said, now + NOTICE_SECONDS));
    }

    /// The first row seen keeps the cursor's row in sight.
    fn scroll(&mut self) {
        let count = self.items().len();
        if let At::Row(row) = self.at {
            if row < self.first {
                self.first = row;
            } else if row >= self.first + VISIBLE {
                self.first = row + 1 - VISIBLE;
            }
        }
        self.first = self.first.min(count.saturating_sub(VISIBLE));
    }
}

/// What a pass of the screen did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Passed {
    pub sounds: Vec<u8>,
    /// The screen closed in this pass: the options screen is back.
    pub closed: bool,
}

/// The PC Settings screen, closed or up, with what the systems between the passes tell
/// it: the app's seconds and the hand the player used last.
#[derive(Resource, Debug, Default)]
pub struct PcSettings {
    screen: Option<Screen>,
    now: f64,
    device: Device,
    /// `--pc-key=NAME`: the key a check presses when a rebinding waits.
    key_of_a_check: Option<KeyCode>,
}

impl PcSettings {
    pub fn is_open(&self) -> bool {
        self.screen.is_some()
    }

    /// Whether a rebinding waits for a key: the keys of the build (F1, F2, F5, F6, F9,
    /// F11) are refused there and do nothing else either.
    pub fn capturing(&self) -> bool {
        (self.screen.as_ref())
            .is_some_and(|screen| matches!(screen.dialog, Some(Dialog::Capture { .. })))
    }

    /// The screen goes without a word, as with the menu it stands in when a quick load
    /// takes the room away (`quick.rs`).
    pub fn close(&mut self) {
        if self.screen.take().is_some() {
            info!("the PC Settings close with the screen under them");
        }
    }

    /// Opens the screen on a page.
    pub fn open(&mut self, page: Page) {
        info!("the PC Settings open on the page {}", page.name());
        self.screen = Some(Screen::new(page));
    }

    /// Once a frame while the screen is up: what the mouse points at, its clicks and the
    /// wheel's notches, which the screen takes from `keys.rs`. `at` is the console's
    /// pixel under the mouse.
    pub fn look(&mut self, mouse: &Mouse, at: Option<(i16, i16)>) {
        let Some(screen) = self.screen.as_mut() else {
            return;
        };
        mouse.take_wheel();
        let under = crate::pointer::row_under(spots(screen), at);
        let pointed = &mut screen.pointed;
        pointed.notches += mouse.notches();
        if mouse.moved() || mouse.clicked() {
            pointed.under = under;
        }
        pointed.moved |= mouse.moved() && under.is_some();
        pointed.clicked |= mouse.clicked() && under.is_some();
    }

    /// One pass of the screen with the pad's words of the pass, on the settings.
    pub fn pass(&mut self, pad: &Pad, prefs: &mut Prefs) -> Passed {
        let now = self.now;
        let device = self.device;
        let Some(screen) = self.screen.as_mut() else {
            return Passed::default();
        };
        let mut passed = Passed {
            sounds: std::mem::take(&mut screen.sounds),
            closed: false,
        };
        if screen
            .notice
            .as_ref()
            .is_some_and(|(_, until)| now >= *until)
        {
            screen.notice = None;
        }
        let pointed = std::mem::take(&mut screen.pointed);
        if screen.deaf {
            screen.deaf = pad.held != 0;
            return passed;
        }
        // A click on a spot is that spot's, not also the cross of the cursor's row.
        let mut pad = *pad;
        if pointed.clicked {
            pad.pressed &= !pad::CROSS;
        }
        let waits = matches!(screen.dialog, Some(Dialog::Capture { .. }));
        match (waits, screen.dialog.is_some()) {
            (true, _) => {}
            (false, true) => question(screen, &pad, pointed, prefs, now, &mut passed),
            (false, false) => {
                let closed = list(screen, &pad, pointed, prefs, (now, device), &mut passed);
                if closed {
                    self.screen = None;
                    passed.closed = true;
                    info!("the PC Settings close");
                }
            }
        }
        passed
    }

    /// What the screen draws over the options' wall: the tabs, the rows of the page, the
    /// line of help and the legend, and a question in front of them. `signs` are the
    /// scroll signs in every window colour; `time` is the menus' time.
    pub(crate) fn paint(
        &self,
        parts: &Parts,
        screen: &mut [u8],
        prefs: &Prefs,
        (colour, signs, time): (i16, &[Picture], u32),
    ) {
        let Some(shown) = self.screen.as_ref() else {
            return;
        };
        let sign = signs.get(usize::try_from(colour).unwrap_or(0));
        let painter = Painter {
            parts,
            colour,
            time,
            sign,
        };
        painter.tabs(screen, shown);
        painter.rows(screen, shown, prefs);
        let count = shown.items().len();
        if let Some(sign) = sign {
            if shown.first > 0 {
                put_sign(screen, sign, 3, (SIGNS_X, LIST_TOP + 4), time);
            }
            if shown.first + VISIBLE < count {
                let bottom = LIST_TOP + ROW_STEP * VISIBLE as i16 - 6;
                put_sign(screen, sign, 1, (SIGNS_X, bottom), time);
            }
        }
        painter.help(screen, &self.help_line());
        if let Some(dialog) = &shown.dialog {
            painter.question(screen, dialog, self.now);
        }
        let mut pairs = Vec::new();
        let mut pair = |sprite: usize, said: &str| pairs.push((sprite, parts.letters(said)));
        match &shown.dialog {
            Some(Dialog::Capture { .. }) => {}
            Some(_) => {
                pair(SIGN_CROSS, "Select");
                pair(SIGN_TRIANGLE, "Back");
            }
            None => {
                pair(SIGN_CROSS, "Select");
                pair(SIGN_TRIANGLE, "Back");
                pair(SIGN_L1, "");
                pair(SIGN_R1, "Page");
            }
        }
        if !pairs.is_empty() {
            parts.legend(screen, colour, &pairs);
        }
    }

    /// The line of help: what changed last, else what the row under the cursor does.
    fn help_line(&self) -> String {
        let Some(screen) = self.screen.as_ref() else {
            return String::new();
        };
        if let Some(Dialog::Capture { .. }) = screen.dialog {
            return "Esc or the right mouse button cancel.".into();
        }
        if let Some((said, _)) = &screen.notice {
            return said.clone();
        }
        match screen.at {
            At::Tabs => "The pages: left and right, or L1 and R1.".into(),
            At::Row(row) => match (screen.items().get(row), screen.pending) {
                (Some(Item::DisplayMode | Item::Resolution), Some(_)) => {
                    "A confirm applies it; moving away does not.".into()
                }
                (Some(item), _) => item.about().into(),
                (None, _) => String::new(),
            },
        }
    }

    /// The screen while it is up (for the tests).
    #[cfg(test)]
    fn screen(&self) -> &Screen {
        self.screen.as_ref().expect("the screen is up")
    }
}

/// A pass of the page: the mouse, then the pad. Answers whether the screen closes.
fn list(
    screen: &mut Screen,
    pad: &Pad,
    pointed: Pointed,
    prefs: &mut Prefs,
    (now, device): (f64, Device),
    passed: &mut Passed,
) -> bool {
    let items = screen.items();
    let last = items.len().saturating_sub(1);
    let go = |screen: &mut Screen, to: At, passed: &mut Passed| {
        if screen.at != to {
            screen.at = to;
            screen.pending = None;
            passed.sounds.push(SOUND_CURSOR);
        }
    };
    // The mouse: the tab or the row under it takes the cursor, a click acts there.
    if pointed.moved {
        match pointed.under {
            Some(Spot::Tab(_)) => go(screen, At::Tabs, passed),
            Some(Spot::Row(row) | Spot::Less(row) | Spot::More(row)) => {
                go(screen, At::Row(row), passed)
            }
            _ => {}
        }
    }
    for _ in 0..pointed.notches.unsigned_abs() {
        let to = match (screen.at, pointed.notches > 0) {
            (At::Tabs, true) => At::Tabs,
            (At::Tabs, false) | (At::Row(0), true) => At::Row(0),
            (At::Row(row), true) => At::Row(row - 1),
            (At::Row(row), false) => At::Row((row + 1).min(last)),
        };
        go(screen, to, passed);
    }
    if pointed.clicked {
        match pointed.under {
            Some(Spot::Tab(page)) => turn(screen, page, passed),
            Some(Spot::Row(row)) => act(screen, row, prefs, (now, device), passed),
            Some(Spot::Less(row)) => step(screen, row, -1, prefs, passed),
            Some(Spot::More(row)) => step(screen, row, 1, prefs, passed),
            _ => {}
        }
    }
    // The pad.
    if pad.pressed & pad::L1 != 0 {
        let page = screen.page.turned(-1);
        turn(screen, page, passed);
    } else if pad.pressed & pad::R1 != 0 {
        let page = screen.page.turned(1);
        turn(screen, page, passed);
    }
    if pad.repeated & pad::UP != 0 {
        let to = match screen.at {
            At::Tabs | At::Row(0) => At::Tabs,
            At::Row(row) => At::Row(row - 1),
        };
        go(screen, to, passed);
    } else if pad.repeated & pad::DOWN != 0 {
        let to = match screen.at {
            At::Tabs => At::Row(0),
            At::Row(row) => At::Row((row + 1).min(last)),
        };
        go(screen, to, passed);
    }
    let sideways = match pad.repeated & (pad::LEFT | pad::RIGHT) {
        pad::LEFT => -1,
        pad::RIGHT => 1,
        _ => 0,
    };
    if sideways != 0 {
        match screen.at {
            At::Tabs => {
                let page = screen.page.turned(sideways);
                turn(screen, page, passed);
            }
            At::Row(row) => step(screen, row, sideways, prefs, passed),
        }
    }
    if pad.pressed & pad::CROSS != 0
        && let At::Row(row) = screen.at
    {
        act(screen, row, prefs, (now, device), passed);
    } else if pad.pressed & pad::TRIANGLE != 0 {
        passed.sounds.push(SOUND_CANCEL);
        return true;
    }
    screen.scroll();
    false
}

/// The page is turned to `page`, the cursor where it was, or on the first row.
fn turn(screen: &mut Screen, page: Page, passed: &mut Passed) {
    if screen.page == page {
        return;
    }
    screen.page = page;
    screen.pending = None;
    screen.first = 0;
    if let At::Row(_) = screen.at {
        screen.at = At::Row(0);
    }
    passed.sounds.push(SOUND_CURSOR);
}

/// Left or right on a row: the value a step on, not round, as the game's answers.
fn step(screen: &mut Screen, row: usize, way: i32, prefs: &mut Prefs, passed: &mut Passed) {
    let Some(&item) = screen.items().get(row) else {
        return;
    };
    screen.at = At::Row(row);
    let count = item.count() as i32;
    let chosen = item.chosen(prefs);
    match item.kind() {
        Kind::Value => {
            let to = (chosen as i32 + way).clamp(0, count - 1) as usize;
            if to != chosen || item.shown(prefs) != item.said(chosen) {
                item.set(prefs, to);
                passed.sounds.push(SOUND_CURSOR);
            }
        }
        Kind::Applied => {
            let from = screen.pending.unwrap_or(chosen);
            let to = (from as i32 + way).clamp(0, count - 1) as usize;
            if to != from {
                screen.pending = (to != chosen).then_some(to);
                passed.sounds.push(SOUND_CURSOR);
            }
        }
        Kind::Bind | Kind::Reset => {}
    }
}

/// A confirm on a row.
fn act(
    screen: &mut Screen,
    row: usize,
    prefs: &mut Prefs,
    (now, device): (f64, Device),
    passed: &mut Passed,
) {
    let Some(&item) = screen.items().get(row) else {
        return;
    };
    screen.at = At::Row(row);
    match item.kind() {
        Kind::Value => {
            let to = (item.chosen(prefs) + 1) % item.count().max(1);
            item.set(prefs, to);
            passed.sounds.push(SOUND_CURSOR);
        }
        Kind::Applied => {
            let Some(to) = screen.pending.take() else {
                // A first confirm chooses the next value, a second one applies it.
                screen.pending = Some((item.chosen(prefs) + 1) % item.count());
                passed.sounds.push(SOUND_CURSOR);
                return;
            };
            let before = prefs.display.clone();
            item.set(prefs, to);
            passed.sounds.push(SOUND_CONFIRM);
            let asks = before.mode != prefs.display.mode
                || (before.resolution != prefs.display.resolution
                    && prefs.display.mode != Mode::Borderless);
            info!("the PC Settings: {} is {}", item.name(), item.shown(prefs));
            if asks {
                screen.dialog = Some(Dialog::Keep {
                    before,
                    until: now + KEEP_SECONDS,
                    yes: true,
                });
            }
        }
        Kind::Bind => {
            let Item::Bind(action) = item else { return };
            screen.dialog = Some(Dialog::Capture {
                action,
                device,
                since: now,
            });
            passed.sounds.push(SOUND_CONFIRM);
            info!(
                "the PC Settings: a {} is waited for, for {}",
                match device {
                    Device::Keyboard => "key",
                    Device::Gamepad => "button of the pad",
                },
                action.name()
            );
        }
        Kind::Reset => {
            screen.dialog = Some(Dialog::Reset { yes: false });
            passed.sounds.push(SOUND_CONFIRM);
        }
    }
}

/// A pass of a question: Yes or No, a confirm answers, a cancel is No.
fn question(
    screen: &mut Screen,
    pad: &Pad,
    pointed: Pointed,
    prefs: &mut Prefs,
    now: f64,
    passed: &mut Passed,
) {
    let Some(dialog) = screen.dialog.as_mut() else {
        return;
    };
    let Some(yes) = dialog.yes_mut() else {
        return;
    };
    let under = pointed
        .under
        .filter(|spot| matches!(spot, Spot::Yes | Spot::No));
    if let Some(spot) = under.filter(|_| pointed.moved || pointed.clicked)
        && *yes != (spot == Spot::Yes)
    {
        *yes = spot == Spot::Yes;
        passed.sounds.push(SOUND_CURSOR);
    }
    if pad.repeated & (pad::LEFT | pad::RIGHT | pad::UP | pad::DOWN) != 0 {
        *yes = !*yes;
        passed.sounds.push(SOUND_CURSOR);
    }
    let confirmed = pad.pressed & pad::CROSS != 0 || (pointed.clicked && under.is_some());
    let cancelled = pad.pressed & pad::TRIANGLE != 0;
    if !confirmed && !cancelled {
        return;
    }
    let answer = confirmed && *yes;
    passed.sounds.push(match answer {
        true => SOUND_CONFIRM,
        false => SOUND_CANCEL,
    });
    let Some(dialog) = screen.dialog.take() else {
        return;
    };
    match (dialog, answer) {
        (Dialog::Keep { .. }, true) => screen.notice("The new settings are kept.".into(), now),
        (Dialog::Keep { before, .. }, false) => {
            put_back(prefs, &before);
            screen.notice("The settings before are back.".into(), now);
        }
        (Dialog::Reset { .. }, true) => {
            prefs.controls = ControlPrefs::default();
            screen.notice("The controls are as they were at first.".into(), now);
        }
        _ => {}
    }
}

/// The window as it was before a new display mode or resolution.
fn put_back(prefs: &mut Prefs, before: &prefs::Display) {
    let display = &mut prefs.display;
    display.mode = before.mode;
    display.resolution = before.resolution;
    display.window_size = before.window_size;
    display.window_position = before.window_position;
}

/// The name of a key as a row and a line of help show it.
fn key_said(key: KeyCode) -> String {
    prompts::key_label(key).0
}

/// The name of a key of the tables as a line of help says it.
fn key_name_said(name: &str) -> String {
    bindings::key_named(name).map_or_else(|| name.to_owned(), key_said)
}

/// A rebinding gets its key or its button: the tables change, and the line of help says
/// what moved.
fn bound(screen: &mut Screen, prefs: &mut Prefs, action: Action, input: Input, now: f64) {
    let controls = &mut prefs.controls;
    let (name, taken, name_of): (String, _, fn(&str) -> String) = match input {
        Input::Key(key) => {
            let name = bindings::key_name(key);
            let taken = bindings::rebind(&mut controls.keys, action, &name, bindings::key_named);
            (name, taken, key_name_said)
        }
        Input::Button(button) => {
            let name = bindings::button_name(button);
            let taken = bindings::rebind(&mut controls.pad, action, &name, bindings::button_named);
            (name, taken, str::to_owned)
        }
    };
    let mut line = format!("{} is {}", action_name(action), name_of(&name));
    if let Some(taken) = taken.first() {
        let other = action_name(taken.from);
        line = match &taken.gets {
            Some(gets) => format!("{line}; {other} gets {}", name_of(gets)),
            None => format!("{line}, not {other} any more"),
        };
    }
    screen.notice(line, now);
    screen.sounds.push(SOUND_CONFIRM);
}

/// A key or a button a rebinding got.
#[derive(Debug, Clone, Copy)]
enum Input {
    Key(KeyCode),
    Button(GamepadButton),
}

/// The spots of the screen with their rectangles: those of a question while one is up,
/// else the tabs and the rows seen.
fn spots(screen: &Screen) -> Vec<(Spot, Rect)> {
    match &screen.dialog {
        Some(Dialog::Capture { .. }) => return Vec::new(),
        Some(_) => {
            let [yes, no] = answers();
            return vec![(Spot::Yes, yes), (Spot::No, no)];
        }
        None => {}
    }
    let mut spots: Vec<(Spot, Rect)> = (Page::ALL.iter())
        .map(|&page| (Spot::Tab(page), tab(page)))
        .collect();
    let items = screen.items();
    for (row, item) in items.iter().enumerate().skip(screen.first).take(VISIBLE) {
        let [name, value] = row_rects(row - screen.first);
        match item.kind() {
            Kind::Value | Kind::Applied => {
                spots.push((Spot::Row(row), name));
                let half = value.w / 2;
                let less = Rect { w: half, ..value };
                let more = Rect {
                    x: value.x + half,
                    w: value.w - half,
                    ..value
                };
                spots.extend([(Spot::Less(row), less), (Spot::More(row), more)]);
            }
            Kind::Bind | Kind::Reset => {
                let whole = Rect {
                    w: value.x + value.w - name.x,
                    ..name
                };
                spots.push((Spot::Row(row), whole));
            }
        }
    }
    spots
}

/// The frame of a tab.
fn tab(page: Page) -> Rect {
    Rect {
        x: TAB_LEFT + TAB_STEP * page.number() as i16,
        y: TAB_TOP,
        w: TAB_WIDTH,
        h: 18,
    }
}

/// The frames of the name and the value of the `seen`th row seen.
fn row_rects(seen: usize) -> [Rect; 2] {
    let y = LIST_TOP + ROW_STEP * seen as i16;
    [
        Rect {
            x: NAME_LEFT,
            y,
            w: NAME_WIDTH,
            h: ROW_HEIGHT,
        },
        Rect {
            x: VALUE_LEFT,
            y,
            w: VALUE_WIDTH,
            h: ROW_HEIGHT,
        },
    ]
}

/// The frames of Yes and No in the box of a question.
fn answers() -> [Rect; 2] {
    let y = QUESTION.y + QUESTION.h - 24;
    let middle = QUESTION.x + QUESTION.w / 2;
    [
        Rect {
            x: middle - ANSWER_WIDTH - 8,
            y,
            w: ANSWER_WIDTH,
            h: 18,
        },
        Rect {
            x: middle + 8,
            y,
            w: ANSWER_WIDTH,
            h: 18,
        },
    ]
}

/// The parts of the menus with the window colour and the menus' time.
struct Painter<'a> {
    parts: &'a Parts<'a>,
    colour: i16,
    time: u32,
    /// The scroll sign of the window colour, also the arrows of a value.
    sign: Option<&'a Picture>,
}

impl Painter<'_> {
    fn frame(&self, screen: &mut [u8], rect: Rect, mode: u8) {
        let style = Style {
            kind: self.colour,
            mode,
            blend: 1,
        };
        let time = if mode == CURSOR { self.time } else { 0 };
        self.parts.frame(screen, rect, style, time);
    }

    /// A text of the build in the font of the texts, dim or not.
    fn text(&self, screen: &mut [u8], said: &str, at: (i16, i16), dim: bool) {
        let units = self.parts.letters(said);
        match dim {
            true => glyphs_lit(
                screen,
                self.parts.font,
                self.parts.font_picture,
                &units,
                at,
                DIM,
            ),
            false => self.parts.text(screen, &units, at, WHOLE),
        }
    }

    /// A text in the middle of a frame.
    fn centred(&self, screen: &mut [u8], said: &str, rect: Rect, dim: bool) {
        let width = self.parts.width_of(said);
        let at = (rect.x + (rect.w - width) / 2, rect.y + (rect.h - 12) / 2);
        self.text(screen, said, at, dim);
    }

    fn tabs(&self, screen: &mut [u8], shown: &Screen) {
        for page in Page::ALL {
            let mode = match (page == shown.page, shown.at) {
                (true, At::Tabs) => CURSOR,
                (true, _) => MARKED,
                (false, _) => PLAIN,
            };
            self.frame(screen, tab(page), mode);
            self.centred(screen, page.name(), tab(page), false);
        }
    }

    fn rows(&self, screen: &mut [u8], shown: &Screen, prefs: &Prefs) {
        let items = shown.items();
        for (row, item) in items.iter().enumerate().skip(shown.first).take(VISIBLE) {
            let [name, value] = row_rects(row - shown.first);
            let here = shown.at == At::Row(row);
            let dim = item.idle(prefs);
            match item.kind() {
                Kind::Reset => {
                    let whole = Rect {
                        w: value.x + value.w - name.x,
                        ..name
                    };
                    self.frame(screen, whole, if here { CURSOR } else { PLAIN });
                    self.centred(screen, item.name(), whole, false);
                    continue;
                }
                _ => {
                    self.frame(screen, name, if here { CURSOR } else { PLAIN });
                    self.text(screen, item.name(), (name.x + 5, name.y + 2), dim);
                }
            }
            match *item {
                Item::Bind(action) => self.binding(screen, action, value, prefs),
                _ => {
                    let pending = shown.pending.filter(|_| here);
                    let mode = if pending.is_some() { MARKED } else { PLAIN };
                    self.frame(screen, value, mode);
                    let said = pending.map_or_else(|| item.shown(prefs), |at| item.said(at));
                    self.centred(screen, &said, value, dim);
                    if here && let Some(sign) = self.sign {
                        let middle = value.y + value.h / 2;
                        put_sign(screen, sign, 2, (value.x + 6, middle), self.time);
                        let right = value.x + value.w - 7;
                        put_sign(screen, sign, 0, (right, middle), self.time);
                    }
                }
            }
        }
    }

    /// The value of an action: its first key as a key cap (with the count of the others)
    /// and its first button as an Xbox pad's.
    fn binding(&self, screen: &mut [u8], action: Action, value: Rect, prefs: &Prefs) {
        let key_half = Rect {
            w: KEY_WIDTH,
            ..value
        };
        let pad_half = Rect {
            x: value.x + KEY_WIDTH + 2,
            w: value.w - KEY_WIDTH - 2,
            ..value
        };
        self.frame(screen, key_half, PLAIN);
        self.frame(screen, pad_half, PLAIN);
        let controls = &prefs.controls;
        let keys: Vec<KeyCode> = (controls.keys.names(action).iter())
            .filter_map(|name| bindings::key_named(name))
            .collect();
        match keys.first() {
            Some(&key) => {
                let cap = prompts::key_glyph(key);
                let more = match keys.len() {
                    1 => String::new(),
                    count => format!("+{}", count - 1),
                };
                let width = cap.wide as i16 + self.parts.width_of(&more);
                let x = key_half.x + (key_half.w - width) / 2;
                let y = key_half.y + (key_half.h - cap.high as i16) / 2;
                prompts::put(screen, &cap, (x, y), WHOLE);
                self.text(screen, &more, (x + cap.wide as i16, key_half.y + 2), true);
            }
            None => self.centred(screen, "-", key_half, true),
        }
        let button =
            (controls.pad.names(action).iter()).find_map(|name| bindings::button_named(name));
        match button {
            Some(button) => {
                let glyph = prompts::button_glyph(button);
                let x = pad_half.x + (pad_half.w - glyph.wide as i16) / 2;
                let y = pad_half.y + (pad_half.h - glyph.high as i16) / 2;
                prompts::put(screen, &glyph, (x, y), WHOLE);
            }
            None => self.centred(screen, "-", pad_half, true),
        }
    }

    /// The line of help in its box.
    fn help(&self, screen: &mut [u8], said: &str) {
        self.frame(screen, HELP_BOX, PLAIN);
        self.text(screen, said, HELP_TEXT, false);
    }

    /// A question or a wait in its box, over a darker page.
    fn question(&self, screen: &mut [u8], dialog: &Dialog, now: f64) {
        // A wait has no answers under its lines.
        let rect = match dialog {
            Dialog::Capture { .. } => Rect { h: 40, ..QUESTION },
            _ => QUESTION,
        };
        darken(screen, rect);
        self.frame(screen, rect, PLAIN);
        let line = |n: i16| Rect {
            y: rect.y + 6 + 14 * n,
            h: 14,
            ..rect
        };
        let left = |until: f64| (until - now).max(0.0).ceil() as u32;
        let (said, yes): (Vec<String>, Option<bool>) = match dialog {
            Dialog::Keep { until, yes, .. } => (
                vec![
                    "Keep these settings?".into(),
                    format!("Back in {} seconds.", left(*until)),
                ],
                Some(*yes),
            ),
            Dialog::Reset { yes } => (
                vec![
                    "Reset all controls?".into(),
                    "Every key and button as at first.".into(),
                ],
                Some(*yes),
            ),
            Dialog::Capture {
                action,
                device,
                since,
            } => {
                let wanted = match device {
                    Device::Keyboard => "Press a key",
                    Device::Gamepad => "Press a button of the pad",
                };
                (
                    vec![
                        format!("{wanted} for {}.", action_name(*action)),
                        format!("Esc cancels ({}).", left(since + CAPTURE_SECONDS)),
                    ],
                    None,
                )
            }
        };
        for (n, said) in said.iter().enumerate() {
            self.centred(screen, said, line(n as i16), false);
        }
        if let Some(yes) = yes {
            let [on_yes, on_no] = answers();
            self.frame(screen, on_yes, if yes { CURSOR } else { PLAIN });
            self.frame(screen, on_no, if yes { PLAIN } else { CURSOR });
            self.centred(screen, "Yes", on_yes, false);
            self.centred(screen, "No", on_no, false);
        }
    }
}

/// The screen at a third of its brightness inside a rectangle.
fn darken(screen: &mut [u8], rect: Rect) {
    for y in rect.y.max(0)..(rect.y + rect.h).min(WHOLE.h) {
        for x in rect.x.max(0)..(rect.x + rect.w).min(WHOLE.w) {
            let at = (y as usize * WHOLE.w as usize + x as usize) * 4;
            for part in &mut screen[at..at + 3] {
                *part /= 3;
            }
        }
    }
}

/// A text of the build as units of the font of the texts: the full-width letters of
/// the encoding, a sign the font does not have a space.
pub(crate) fn letters(font: &Font, said: &str) -> Vec<u16> {
    let space = wide_unit(b' ');
    (said.bytes())
        .map(|letter| match letter {
            b'!' => 0x8149u16.swap_bytes(),
            b'(' => 0x8169u16.swap_bytes(),
            b')' => 0x816Au16.swap_bytes(),
            b'+' => 0x817Bu16.swap_bytes(),
            b'\'' => 0x8166u16.swap_bytes(),
            b';' => 0x8147u16.swap_bytes(),
            b'%' => 0x8193u16.swap_bytes(),
            letter => wide_unit(letter),
        })
        .map(|unit| match font.table.contains(&unit) {
            true => unit,
            false => space,
        })
        .collect()
}

/// Between the passes: the app's seconds and the hand used last for the screen, the end
/// of "Keep these settings?" when it was not answered in time, and the key or the button
/// a rebinding waits for.
#[allow(clippy::too_many_arguments)]
fn tick(
    real: Res<Time<Real>>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    pads: Res<Pads>,
    device: Res<InputDevice>,
    mut pc: ResMut<PcSettings>,
    mut prefs: ResMut<Prefs>,
) {
    let now = real.elapsed_secs_f64();
    pc.now = now;
    pc.device = device.last;
    let check_key = pc.key_of_a_check;
    let Some(screen) = pc.screen.as_mut() else {
        return;
    };
    match screen.dialog.clone() {
        Some(Dialog::Keep { before, until, .. }) if now >= until => {
            screen.dialog = None;
            put_back(&mut prefs, &before);
            screen.sounds.push(SOUND_CANCEL);
            screen.notice("Not kept: the settings before are back.".into(), now);
        }
        Some(Dialog::Capture {
            action,
            device,
            since,
        }) => {
            let cancelled = keys.just_pressed(KeyCode::Escape)
                || mouse.just_pressed(MouseButton::Right)
                || now >= since + CAPTURE_SECONDS;
            if cancelled {
                screen.dialog = None;
                screen.deaf = true;
                screen.sounds.push(SOUND_CANCEL);
                screen.notice(format!("{} stays as it was.", action_name(action)), now);
                return;
            }
            let input = match device {
                Device::Keyboard => {
                    let of_the_build = (keys.get_just_pressed().copied())
                        .find(|key| KEYS_OF_THE_BUILD.contains(key));
                    if let Some(key) = of_the_build {
                        screen.sounds.push(SOUND_REFUSED);
                        let said = format!("{} is a key of the build.", key_said(key));
                        screen.notice(said, now);
                    }
                    let pressed = (keys.get_just_pressed()).copied().find(|key| {
                        bindings::is_named_key(*key) && !KEYS_OF_THE_BUILD.contains(key)
                    });
                    let checked = check_key.filter(|_| now >= since + CHECK_KEY_AFTER);
                    pressed.or(checked).map(Input::Key)
                }
                Device::Gamepad => (bindings::named_buttons()
                    .find(|button| pads.just_pressed(*button)))
                .map(Input::Button),
            };
            if let Some(input) = input {
                screen.dialog = None;
                screen.deaf = true;
                let mut changed = prefs.clone();
                bound(screen, &mut changed, action, input, now);
                if changed != *prefs {
                    *prefs = changed;
                }
            }
        }
        _ => {}
    }
}

/// The screen's resource and the system between its passes. `key_of_a_check` is the
/// key of `--pc-key=`.
pub struct PcSettingsPlugin {
    pub key_of_a_check: Option<KeyCode>,
}

impl Plugin for PcSettingsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PcSettings {
            key_of_a_check: self.key_of_a_check,
            ..default()
        })
        .add_systems(PreUpdate, tick.after(crate::keys::read_pad));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jc_formats::pad::Repeat;

    /// The pad's words of a pass in which `buttons` went down.
    fn press(buttons: u16) -> Pad {
        let mut pad = Pad::default();
        pad.read(0, Repeat::of(1), 0);
        pad.read(buttons, Repeat::of(1), 1);
        pad
    }

    fn opened(page: Page) -> PcSettings {
        let mut pc = PcSettings::default();
        pc.open(page);
        pc
    }

    #[test]
    fn every_row_reads_back_what_it_sets() {
        for page in Page::ALL {
            for item in page.items() {
                for at in 0..item.count() {
                    let mut prefs = Prefs::default();
                    item.set(&mut prefs, at);
                    assert_eq!(item.chosen(&prefs), at, "{item:?} {at}");
                    assert_eq!(item.shown(&prefs), item.said(at), "{item:?} {at}");
                }
            }
        }
        // The defaults are values the rows offer.
        let prefs = Prefs::default();
        for item in Page::ALL.iter().flat_map(|page| page.items()) {
            if item.count() > 0 {
                assert_eq!(
                    item.shown(&prefs),
                    item.said(item.chosen(&prefs)),
                    "{item:?}"
                );
            }
        }
        // A value written by hand is shown as it is and steps from the nearest.
        let mut prefs = Prefs::default();
        prefs.display.frame_cap = 75;
        assert_eq!(Item::FrameCap.shown(&prefs), "75");
        assert_eq!(Item::FrameCap.chosen(&prefs), 2);
    }

    #[test]
    fn left_and_right_change_a_value_at_once_and_stop_at_the_ends() {
        let mut pc = opened(Page::Sound);
        let mut prefs = Prefs::default();
        let passed = pc.pass(&press(pad::LEFT), &mut prefs);
        assert_eq!(passed.sounds, [SOUND_CURSOR]);
        assert_eq!(prefs.feel.audio.master, 95);
        pc.pass(&press(pad::RIGHT), &mut prefs);
        assert!(pc.pass(&press(pad::RIGHT), &mut prefs).sounds.is_empty());
        assert_eq!(prefs.feel.audio.master, 100);
        // Down to the music, then L1 to the page before.
        pc.pass(&press(pad::DOWN), &mut prefs);
        pc.pass(&press(pad::LEFT), &mut prefs);
        assert_eq!(prefs.feel.audio.music, 95);
        pc.pass(&press(pad::L1), &mut prefs);
        assert_eq!(
            (pc.screen().page, pc.screen().at),
            (Page::Display, At::Row(0))
        );
        // Up to the tabs, where right turns the page.
        pc.pass(&press(pad::UP), &mut prefs);
        pc.pass(&press(pad::RIGHT), &mut prefs);
        assert_eq!((pc.screen().page, pc.screen().at), (Page::Sound, At::Tabs));
        // A cancel closes the screen.
        let passed = pc.pass(&press(pad::TRIANGLE), &mut prefs);
        assert!(passed.closed && !pc.is_open());
    }

    #[test]
    fn a_new_display_mode_is_applied_by_a_confirm_and_asked_about() {
        let mut pc = opened(Page::Display);
        let mut prefs = Prefs::default();
        // Right chooses Fullscreen, which is not applied yet.
        pc.pass(&press(pad::RIGHT), &mut prefs);
        assert_eq!(prefs.display.mode, Mode::Borderless);
        assert_eq!(pc.screen().pending, Some(1));
        // Down leaves it; up and right again, then a confirm applies it.
        pc.pass(&press(pad::DOWN), &mut prefs);
        assert_eq!(pc.screen().pending, None);
        pc.pass(&press(pad::UP), &mut prefs);
        pc.pass(&press(pad::RIGHT), &mut prefs);
        pc.pass(&press(pad::RIGHT), &mut prefs);
        pc.pass(&press(pad::CROSS), &mut prefs);
        assert_eq!(prefs.display.mode, Mode::Windowed);
        assert!(matches!(
            pc.screen().dialog,
            Some(Dialog::Keep { yes: true, .. })
        ));
        // No puts the borderless screen back.
        pc.pass(&press(pad::RIGHT), &mut prefs);
        pc.pass(&press(pad::CROSS), &mut prefs);
        assert_eq!(prefs.display.mode, Mode::Borderless);
        assert!(pc.screen().dialog.is_none());
        // A resolution of a window sizes it, and Yes keeps it.
        prefs.display.mode = Mode::Windowed;
        pc.pass(&press(pad::DOWN), &mut prefs);
        pc.pass(&press(pad::LEFT), &mut prefs);
        pc.pass(&press(pad::CROSS), &mut prefs);
        let size = crate::display::Size::new(1600, 900);
        assert_eq!(
            (prefs.display.resolution, prefs.display.window_size),
            (size, Some(size))
        );
        pc.pass(&press(pad::CROSS), &mut prefs);
        assert!(pc.screen().dialog.is_none());
        assert_eq!(prefs.display.resolution, size);
    }

    #[test]
    fn the_question_puts_the_settings_back_when_its_time_is_out() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<Pads>()
            .init_resource::<InputDevice>()
            .insert_resource(Time::<Real>::default())
            .insert_resource(Prefs::default())
            .insert_resource(opened(Page::Display))
            .add_systems(Update, tick);
        let mut before = Prefs::default().display;
        before.mode = Mode::Fullscreen;
        app.world_mut()
            .resource_mut::<PcSettings>()
            .screen
            .as_mut()
            .unwrap()
            .dialog = Some(Dialog::Keep {
            before,
            until: -1.0,
            yes: true,
        });
        app.update();
        let world = app.world();
        assert_eq!(world.resource::<Prefs>().display.mode, Mode::Fullscreen);
        assert!(world.resource::<PcSettings>().screen().dialog.is_none());
    }

    #[test]
    fn a_rebinding_takes_the_key_pressed_and_says_what_moved() {
        // The eighth row of the controls is the cross, whose first key is Space.
        let mut pc = opened(Page::Controls);
        let mut prefs = Prefs::default();
        for _ in 0..7 {
            pc.pass(&press(pad::DOWN), &mut prefs);
        }
        assert_eq!(pc.screen().at, At::Row(7));
        pc.pass(&press(pad::CROSS), &mut prefs);
        assert!(matches!(
            pc.screen().dialog,
            Some(Dialog::Capture {
                action: Action::Cross,
                ..
            })
        ));
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<Pads>()
            .init_resource::<InputDevice>()
            .insert_resource(Time::<Real>::default())
            .insert_resource(prefs)
            .insert_resource(pc)
            .add_systems(Update, tick);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyC);
        app.update();
        let world = app.world();
        let keys = &world.resource::<Prefs>().controls.keys;
        assert_eq!(
            (keys.cross[0].as_str(), keys.square[0].as_str()),
            ("KeyC", "Space")
        );
        let pc = world.resource::<PcSettings>();
        assert!(pc.screen().dialog.is_none() && pc.screen().deaf);
        assert_eq!(pc.help_line(), "Confirm is C; Square gets SPACE");
    }

    #[test]
    fn esc_leaves_a_rebinding_and_the_key_held_is_no_press() {
        let mut pc = opened(Page::Controls);
        let mut prefs = Prefs::default();
        for _ in 0..7 {
            pc.pass(&press(pad::DOWN), &mut prefs);
        }
        pc.pass(&press(pad::CROSS), &mut prefs);
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<Pads>()
            .init_resource::<InputDevice>()
            .insert_resource(Time::<Real>::default())
            .insert_resource(prefs)
            .insert_resource(pc)
            .add_systems(Update, tick);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        let world = app.world_mut();
        assert_eq!(world.resource::<Prefs>().controls, ControlPrefs::default());
        let mut pc = world.resource_mut::<PcSettings>();
        assert!(pc.screen().dialog.is_none());
        assert_eq!(pc.help_line(), "Confirm stays as it was.");
        // Esc is the triangle: held, it closes nothing; let go, the next one does.
        let mut prefs = Prefs::default();
        let mut held = press(pad::TRIANGLE);
        assert!(!pc.pass(&held, &mut prefs).closed);
        held.read(pad::TRIANGLE, Repeat::of(1), 2);
        assert!(!pc.pass(&held, &mut prefs).closed);
        assert!(pc.pass(&Pad::default(), &mut prefs).sounds.is_empty());
        assert!(pc.pass(&press(pad::TRIANGLE), &mut prefs).closed);
    }

    #[test]
    fn reset_asks_first() {
        let mut pc = opened(Page::Controls);
        let mut prefs = Prefs::default();
        prefs.controls.keys.cross = vec!["KeyG".into()];
        let last = Page::Controls.items().len() - 1;
        for _ in 0..last {
            pc.pass(&press(pad::DOWN), &mut prefs);
        }
        pc.pass(&press(pad::CROSS), &mut prefs);
        assert!(matches!(
            pc.screen().dialog,
            Some(Dialog::Reset { yes: false })
        ));
        // No is first: a confirm changes nothing.
        pc.pass(&press(pad::CROSS), &mut prefs);
        assert_eq!(prefs.controls.keys.cross, ["KeyG"]);
        pc.pass(&press(pad::CROSS), &mut prefs);
        pc.pass(&press(pad::LEFT), &mut prefs);
        pc.pass(&press(pad::CROSS), &mut prefs);
        assert_eq!(prefs.controls, ControlPrefs::default());
    }

    #[test]
    fn the_mouse_points_at_rows_and_clicks_a_value_s_halves() {
        let mut pc = opened(Page::Sound);
        let mut prefs = Prefs::default();
        // The third row's name: the cursor goes there.
        let [name, value] = row_rects(2);
        let screen = pc.screen.as_mut().unwrap();
        let under = crate::pointer::row_under(spots(screen), Some((name.x + 4, name.y + 4)));
        assert_eq!(under, Some(Spot::Row(2)));
        screen.pointed = Pointed {
            under,
            moved: true,
            ..default()
        };
        pc.pass(&Pad::default(), &mut prefs);
        assert_eq!(pc.screen().at, At::Row(2));
        // A click on the left half of its value: a step down, and not the cross too.
        let screen = pc.screen.as_mut().unwrap();
        let left = (value.x + 4, value.y + 4);
        let under = crate::pointer::row_under(spots(screen), Some(left));
        screen.pointed = Pointed {
            under,
            clicked: true,
            ..default()
        };
        pc.pass(&press(pad::CROSS), &mut prefs);
        assert_eq!(prefs.feel.audio.effects, 95);
        // The wheel moves the cursor a row a notch.
        pc.screen.as_mut().unwrap().pointed.notches = -2;
        pc.pass(&Pad::default(), &mut prefs);
        assert_eq!(pc.screen().at, At::Row(4));
    }
}
