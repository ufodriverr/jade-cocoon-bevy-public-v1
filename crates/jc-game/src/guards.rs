
use std::sync::{Arc, OnceLock};

use bevy::prelude::*;
use bevy::window::WindowCloseRequested;

use crate::saving::PlayClock;

/// The play that may follow a save or a load in its room before the game counts as
/// gone on: ten seconds of the play clock, which runs on while the screen of the save is
/// up.
pub const QUIET: u32 = 10 * jc_formats::carried::REFRESHES_A_SECOND;

/// What the question of the close button says.
const QUESTION: &str = "Quit? Progress since your last save will be lost.";
/// What the question of Ctrl+F5 says.
const START_OVER: &str = "Start over? Progress since your last save will be lost.";
/// What the close button asks while a fight of two players runs.
const LEAVE_THE_MATCH: &str = "Leave the match? It counts as a loss.";

/// Whether the running game has gone on since its last save or load.
#[derive(Resource, Debug, Default)]
pub struct Unsaved {
    gone_on: bool,
    /// The room and the play clock of the last save or load.
    kept: Option<(u16, u32)>,
}

impl Unsaved {
    /// The game was saved or loaded in room `room` (none: not in a room) at the play
    /// clock `clock`.
    pub fn kept(&mut self, room: Option<u16>, clock: u32) {
        self.gone_on = false;
        self.kept = room.map(|room| (room, clock));
    }

    /// The game was left ("End Game") or starts anew: nothing to lose until a room of
    /// it is up.
    pub fn left(&mut self) {
        *self = Self::default();
    }

    pub fn gone_on(&self) -> bool {
        self.gone_on
    }
}

/// While a room is on screen and no save screen stands over it: whether the game has
/// gone on since its last save or load.
pub fn note_play(show: Res<crate::ShowRoom>, clock: Res<PlayClock>, mut unsaved: ResMut<Unsaved>) {
    if unsaved.gone_on {
        return;
    }
    let quiet = unsaved
        .kept
        .is_some_and(|(room, at)| room == show.number && clock.refreshes.wrapping_sub(at) <= QUIET);
    if !quiet {
        info!(
            "the game has gone on since its last save or load (room {})",
            show.on_screen
        );
        unsaved.gone_on = true;
    }
}

/// What a yes to a question does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Then {
    /// The close button: the app ends.
    Quit,
    /// Ctrl+F5: the game starts over.
    StartOver,
}

/// The question of the close button or of Ctrl+F5, while it is up: the answer, once
/// given, and what a yes does. It is asked on a thread of its own, so the window stays
/// as it is behind it.
#[derive(Resource, Default)]
pub struct Asking(Option<(Arc<OnceLock<bool>>, Then)>);

impl Asking {
    /// Asks `question` on a thread of its own.
    fn ask(&mut self, question: &'static str, then: Then) {
        let answer = Arc::new(OnceLock::new());
        let given = answer.clone();
        std::thread::spawn(move || {
            let _ = given.set(crate::dialog::ask_yes_no(question));
        });
        self.0 = Some((answer, then));
    }

    /// The answer, once it is given: what a yes does, or none for a no.
    fn answered(&mut self) -> Option<Option<Then>> {
        let (answer, then) = self.0.as_ref()?;
        let yes = *answer.get()?;
        let then = *then;
        self.0 = None;
        Some(yes.then_some(then))
    }
}

/// Whether the close button asks: only a play session (a check, whose window is hidden,
/// never shows a question), only in a game that has gone on since its last save, and
/// never on the title.
pub fn must_ask(live: bool, gone_on: bool, on_title: bool) -> bool {
    live && gone_on && !on_title
}

/// The window's close button and Ctrl+F5: in a game that has gone on since its last
/// save, a play session asks first; otherwise, and on a yes, the app ends or starts
/// over. A second press while the question is up does nothing.
#[allow(clippy::too_many_arguments)]
pub fn ask_before_closing(
    mut requests: MessageReader<WindowCloseRequested>,
    (unsaved, title, file): (
        Res<Unsaved>,
        Option<Res<crate::title::Title>>,
        Res<crate::prefs::PrefsFile>,
    ),
    keys: Option<Res<ButtonInput<KeyCode>>>,
    (restarts, pc): (
        Option<Res<Restarts>>,
        Option<Res<crate::pc_settings::PcSettings>>,
    ),
    (windows, arena): (Query<&Window>, Option<Res<crate::versus_view::Arena>>),
    mut asking: ResMut<Asking>,
    mut exit: MessageWriter<AppExit>,
) {
    match asking.answered() {
        Some(Some(Then::Quit)) => {
            info!("the window's close button: yes, the app ends");
            exit.write(AppExit::Success);
        }
        Some(Some(Then::StartOver)) => {
            info!("Ctrl+F5: yes, the game starts over");
            start_over(&mut exit);
        }
        Some(None) => info!("the question: no, the player stays in the game"),
        None => {}
    }
    let closes = requests.read().count() > 0;
    // Ctrl+F5, where a play session has keys; not while a rebinding waits for a key.
    let restarts = restarts.is_some()
        && !pc.is_some_and(|pc| pc.capturing())
        && keys.is_some_and(|keys| {
            keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight])
                && keys.just_pressed(KeyCode::F5)
        });
    if !(closes || restarts) || asking.0.is_some() {
        return;
    }
    let shown = windows.iter().any(|window| window.visible);
    // The arena keeps no game: it asks only while a fight runs, which leaving loses.
    if let Some(arena) = arena {
        match (closes, shown && file.live() && arena.match_runs()) {
            (true, true) => {
                info!("the window's close button: a fight of two players runs, it asks");
                asking.ask(LEAVE_THE_MATCH, Then::Quit);
            }
            (true, false) => {
                info!("the window's close button: the app ends");
                exit.write(AppExit::Success);
            }
            (false, _) => start_over(&mut exit),
        }
        return;
    }
    let asks = shown && must_ask(file.live(), unsaved.gone_on(), title.is_some());
    match (closes, asks) {
        (true, false) => {
            info!("the window's close button: the app ends");
            exit.write(AppExit::Success);
        }
        (true, true) => {
            info!("the window's close button: the game has gone on since its last save, it asks");
            asking.ask(QUESTION, Then::Quit);
        }
        (false, false) => start_over(&mut exit),
        (false, true) => {
            info!("Ctrl+F5: the game has gone on since its last save, it asks");
            asking.ask(START_OVER, Then::StartOver);
        }
    }
}

/// Ctrl+F5 starts the whole game over in a run with keys (a play session, `--show`): a
/// debug key of the build, not of the game; F5 alone did it before 2026-10-15.
#[derive(Resource)]
pub struct Restarts;

/// The whole game starts over: the app starts itself anew with the options it was
/// given and ends, so that nothing of the session is left.
fn start_over(exit: &mut MessageWriter<AppExit>) {
    let started = std::env::current_exe().and_then(|exe| {
        std::process::Command::new(exe)
            .args(std::env::args_os().skip(1))
            .spawn()
    });
    match started {
        Ok(_) => {
            info!("Ctrl+F5: the game starts over");
            exit.write(AppExit::Success);
        }
        Err(err) => error!("Ctrl+F5: cannot start the game over: {err}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noted(unsaved: Unsaved, room: u16, clock: u32) -> bool {
        let mut app = App::new();
        let mut show = crate::ShowRoom::parse(&room.to_string()).unwrap();
        show.on_screen = room.to_string();
        let mut play = PlayClock::default();
        play.set(clock);
        app.insert_resource(show)
            .insert_resource(play)
            .insert_resource(unsaved)
            .add_systems(Update, note_play);
        app.update();
        app.world().resource::<Unsaved>().gone_on()
    }

    #[test]
    fn only_a_play_session_in_a_game_that_went_on_asks() {
        assert!(must_ask(true, true, false));
        assert!(!must_ask(false, true, false), "a check never asks");
        assert!(!must_ask(true, false, false), "nothing to lose");
        assert!(!must_ask(true, true, true), "the title");
    }

    #[test]
    fn a_check_closes_at_once_without_a_question() {
        let mut app = App::new();
        app.add_message::<WindowCloseRequested>()
            .add_message::<AppExit>()
            .insert_resource(crate::prefs::at_start(false).1)
            .insert_resource(Unsaved {
                gone_on: true,
                kept: None,
            })
            .init_resource::<Asking>()
            .add_systems(Update, ask_before_closing);
        let window = app.world_mut().spawn(Window::default()).id();
        app.world_mut()
            .write_message(WindowCloseRequested { window });
        app.update();
        assert!(app.world().resource::<Asking>().0.is_none());
        let exits = app.world().resource::<Messages<AppExit>>();
        assert_eq!(exits.iter_current_update_messages().count(), 1);
    }

    /// The app with a question up whose answer is already given.
    fn answered(yes: bool, then: Then) -> App {
        let mut app = App::new();
        app.add_message::<WindowCloseRequested>()
            .add_message::<AppExit>()
            .insert_resource(crate::prefs::at_start(false).1)
            .insert_resource(Unsaved::default())
            .init_resource::<Asking>()
            .add_systems(Update, ask_before_closing);
        let answer = Arc::new(OnceLock::new());
        answer.set(yes).unwrap();
        app.world_mut().resource_mut::<Asking>().0 = Some((answer, then));
        app
    }

    #[test]
    fn a_no_keeps_the_game_and_a_yes_to_the_close_button_ends_it() {
        let mut app = answered(false, Then::Quit);
        app.update();
        assert!(app.world().resource::<Asking>().0.is_none());
        let exits = app.world().resource::<Messages<AppExit>>();
        assert_eq!(exits.iter_current_update_messages().count(), 0);
        // The game runs on, and a question that is still up does not end it either.
        app.world_mut().resource_mut::<Asking>().0 = Some((Arc::new(OnceLock::new()), Then::Quit));
        app.update();
        app.update();
        assert!(app.world().resource::<Asking>().0.is_some());
        let exits = app.world().resource::<Messages<AppExit>>();
        assert_eq!(exits.iter_current_update_messages().count(), 0);
        let mut app = answered(true, Then::Quit);
        app.update();
        let exits = app.world().resource::<Messages<AppExit>>();
        assert_eq!(exits.iter_current_update_messages().count(), 1);
    }

    #[test]
    fn ctrl_f5_does_nothing_without_keys_or_while_a_key_is_bound() {
        let mut app = App::new();
        app.add_message::<WindowCloseRequested>()
            .add_message::<AppExit>()
            .insert_resource(crate::prefs::at_start(false).1)
            .insert_resource(Unsaved::default())
            .init_resource::<Asking>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_systems(Update, ask_before_closing);
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.press(KeyCode::ControlLeft);
        keys.press(KeyCode::F5);
        // A check has no `Restarts`: nothing starts.
        app.update();
        let exits = app.world().resource::<Messages<AppExit>>();
        assert_eq!(exits.iter_current_update_messages().count(), 0);
    }

    #[test]
    fn a_game_goes_on_by_a_new_room_or_by_play() {
        // No save yet: the first room counts.
        assert!(noted(Unsaved::default(), 84, 0));
        let mut saved = Unsaved::default();
        saved.kept(Some(84), 1000);
        assert!(!noted(saved, 84, 1000 + QUIET));
        let mut saved = Unsaved::default();
        saved.kept(Some(84), 1000);
        assert!(noted(saved, 84, 1001 + QUIET));
        let mut saved = Unsaved::default();
        saved.kept(Some(84), 1000);
        assert!(noted(saved, 85, 1000));
    }
}
