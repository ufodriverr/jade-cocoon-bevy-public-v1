
use crate::action::Action;
use crate::command::Runner;

/// The kinds that are asked differently from the others.
const NOTHING: u32 = 20;
const DEFENDS: u32 = 18;

/// What a kind asks of a creature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Asked {
    /// The entry of the move table whose list the creature is given.
    pub entry: usize,
    /// The action whose windows are turned into times (+14 of the request); none where
    /// the routine leaves the -1 it starts with.
    pub window: Option<i16>,
}

/// The number a body attack is played by, from the slot of the attack that the command
/// has as its parameter (`0x800318CC`): it picks the list and the action.
pub fn body_attack(slot: u32) -> usize {
    const BY_SLOT: [usize; 5] = [3, 0, 2, 1, 4];
    (BY_SLOT.get(slot as usize).copied()).unwrap_or(0)
}

impl Asked {
    /// The row of a kind (`0x80018C00`, the jump table `0x800721C4`); none for a kind
    /// that leaves the request empty (4, 8, 20, 23, 24, 25, 27 and every kind from 28
    /// on). The parameter counts for a body attack and for the capture.
    pub fn of(kind: u32, parameter: u32) -> Option<Self> {
        let (entry, window) = match kind {
            0 => (2, Some(5)),
            1 => (9, Some(12)),
            2 => (17, Some(4)),
            3 => (18, Some(4)),
            5 | 26 => (28 + parameter as usize, None),
            6 => {
                let number = body_attack(parameter);
                (3 + number, Some(6 + number as i16))
            }
            7 => (13, Some(4)),
            9 => (10, Some(13)),
            10 => (11, Some(14)),
            11 => (16, Some(21)),
            12 => (12, Some(20)),
            13 => (20, None),
            14 => (19, None),
            15 => (22, Some(24)),
            16 => (21, Some(23)),
            17 => (15, Some(19)),
            18 => (14, Some(18)),
            19 => (23, Some(25)),
            21 => (17, Some(26)),
            22 => (18, Some(27)),
            _ => return None,
        };
        Some(Self { entry, window })
    }
}

/// The two times of a creature behind its request and the pass that follows it: K, the
/// game frames from "go on" to its key moment, and E, those to the end of its move.
/// Kind 20 is not asked at all and has no times. K is the time of the first window of
/// the request's action (`0x8001F2D0`: -1 for a window without frames, 0 for a request
/// without an action), and it is put to 0 for a reaction that is not kind 18; E is the
/// time of the list to its "move over" (`0x8001F0A4` with no action). Both are kept as
/// 16 bits.
pub fn times(
    runner: &Runner,
    actions: &[Action],
    kind: u32,
    asked: Option<Asked>,
    reaction: bool,
) -> [i16; 2] {
    if kind == NOTHING {
        return [0, 0];
    }
    let key = (asked.and_then(|asked| asked.window))
        .map_or(0, |action| runner.window_time(actions, action));
    let whole = runner.whole_time(actions) as i16;
    match reaction && kind != DEFENDS {
        true => [0, whole],
        false => [key as i16, whole],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{TAIL, WINDOWS};
    use crate::animation::{Playback, WHOLE};
    use crate::command::{Command, Kind, When};

    fn action(animation: usize, end: i16, window: [i16; 2]) -> Action {
        let mut windows = [[-1; 2]; WINDOWS];
        windows[0] = window;
        Action {
            animation: Some(animation),
            resident: 1,
            extra: [0; 2],
            first: 0,
            loop_start: 0,
            hold: end,
            end,
            windows,
            tail: [-1; TAIL],
        }
    }

    /// The shape of a move of the creatures' table: the hold, the action, "move over",
    /// and the standing action again.
    fn attack() -> Vec<Command> {
        [
            (When::Any, Kind::Read(5)),
            (When::Any, Kind::WaitForRead),
            (When::Any, Kind::Hold),
            (When::Any, Kind::Fade),
            (When::Any, Kind::Action(5)),
            (When::Done, Kind::GiveBack(5)),
            (When::Any, Kind::MoveOver),
            (When::Any, Kind::FromLoopStart),
            (When::Any, Kind::Action(4)),
            (When::Any, Kind::End),
        ]
        .map(|(when, kind)| Command {
            when,
            kind,
            units: 1,
        })
        .to_vec()
    }

    fn actions() -> Vec<Action> {
        let mut actions = vec![action(0, 9, [-1, -1]); 6];
        actions[4] = action(1, 19, [-1, -1]);
        actions[5] = action(2, 29, [12, 14]);
        actions
    }

    #[test]
    fn every_kind_has_the_row_of_the_routine() {
        let row = |kind, parameter| Asked::of(kind, parameter).map(|a| (a.entry, a.window));
        assert_eq!(row(0, 0), Some((2, Some(5))));
        assert_eq!(row(1, 7), Some((9, Some(12))));
        assert_eq!(row(5, 3), Some((31, None)));
        assert_eq!(row(26, 0), Some((28, None)));
        // Defending stays in its action; to stop defending is the way back.
        assert_eq!(row(18, 0), Some((14, Some(18))));
        assert_eq!(row(17, 0), Some((15, Some(19))));
        assert_eq!(row(21, 0), Some((17, Some(26))));
        for kind in [4, 8, 20, 23, 24, 25, 27, 28, 400] {
            assert_eq!(row(kind, 0), None);
        }
        // A body attack by the slot of the attack.
        assert_eq!([0, 1, 2, 3, 4, 5].map(body_attack), [3, 0, 2, 1, 4, 0]);
        assert_eq!(row(6, 0), Some((6, Some(9))));
        assert_eq!(row(6, 1), Some((3, Some(6))));
    }

    #[test]
    fn the_times_count_from_the_hold() {
        let actions = actions();
        let mut runner = Runner::new(Playback::new(1, actions[4].span(), WHOLE as i16), 4, 4);
        let asked = Asked::of(0, 0);
        runner.request_list(attack(), true);
        runner.pass(&actions);
        assert!(runner.ready() && !runner.free());
        // The first window of action 5 opens on frame 12; the action has 30 frames, of
        // which the one it starts at is not counted.
        assert_eq!(times(&runner, &actions, 0, asked, false), [12, 29]);
        // A reaction has no key moment, a kind without a row no window.
        assert_eq!(times(&runner, &actions, 0, asked, true), [0, 29]);
        assert_eq!(times(&runner, &actions, 18, asked, true), [12, 29]);
        assert_eq!(times(&runner, &actions, 4, None, false), [0, 29]);
        assert_eq!(times(&runner, &actions, 20, None, false), [0, 0]);
        // The windows of the action as the effect of the move gets them: both frames
        // of a pair as times, a pair without frames left alone, and no action none.
        let pairs = runner.window_times(&actions, 5);
        assert_eq!(pairs[0], [12, 14]);
        assert!(pairs[1..].iter().all(|pair| *pair == [-1, -1]));
        assert_eq!(runner.window_times(&actions, -1), [[-1; 2]; WINDOWS]);

        // Let go, the creature plays the action to its end: the pass that starts it and
        // 29 more, the last of which tells that the move is over and goes on to the
        // standing action.
        runner.goes_on();
        let mut frames = 0;
        while !runner.take_over() {
            runner.pass(&actions);
            runner.step();
            frames += 1;
            assert!(frames < 100, "the move does not end");
        }
        assert_eq!((frames, runner.action), (30, Some(4)));
    }

    #[test]
    fn a_window_without_frames_has_no_time() {
        let mut actions = actions();
        actions[5] = action(2, 29, [-1, -1]);
        let mut runner = Runner::new(Playback::new(1, actions[4].span(), WHOLE as i16), 4, 4);
        runner.request_list(attack(), false);
        runner.pass(&actions);
        assert_eq!(
            times(&runner, &actions, 0, Asked::of(0, 0), false),
            [-1, 29]
        );
    }
}
