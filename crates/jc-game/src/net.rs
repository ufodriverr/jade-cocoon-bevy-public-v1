
use std::time::Duration;

use bevy::prelude::*;
use jc_net::session::VersusSession;

/// How long the closing game waits for its "leave" to go out.
const LAST_WORD: Duration = Duration::from_millis(500);

/// The session of a fight of two players while one is on: inserted by the arena's
/// screens once connected, removed when the player leaves the arena (dropping it sends
/// the "leave").
#[derive(Resource, Deref, DerefMut)]
pub struct Versus(pub VersusSession);

/// The window closed or the game ends: the other side is told this side leaves, rather
/// than waiting out the silence.
pub fn close_on_exit(mut exits: MessageReader<AppExit>, versus: Option<ResMut<Versus>>) {
    if exits.read().next().is_some()
        && let Some(mut versus) = versus
    {
        versus.finish(LAST_WORD);
    }
}
