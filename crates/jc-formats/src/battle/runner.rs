
use super::fight::{Fall, OPPONENTS, PLAYER, Run, SLOTS};
use crate::sound;

/// The kinds that put an opponent on the removal list when it is let go, and the two
/// kinds of the capture, whose parameter is the grade.
const RUNS_AWAY: u32 = 7;
const FALLS: u32 = 11;
const STONE_DEATH: u32 = 19;
const CAPTURES: [u32; 2] = [5, 26];
/// The command "nothing", which nobody is asked for.
const NOTHING: u32 = 20;
/// The game frames a number task counts an HP or MP change into a record.
pub const NUMBER_COUNTS: i32 = 16;

/// The fighters of a fight as the runner asks them.
pub trait Field {
    /// The first test (`0x800BDF0C`): the fighter of the player's side and every
    /// opponent on the field are free.
    fn free(&mut self) -> bool;
    /// Before the times (`0x800BC638`): the slots of `turned` turn to the mean place of
    /// the slots of `to` ([`mean`]). Only the player and his creature turn; an opponent
    /// does nothing.
    fn turn(&mut self, _to: u8, _turned: u8) {}
    /// A slot is asked for the move of a record, the command's or its reaction's, and
    /// answers its two times K and E; (0, 0) for a slot that is not on the field.
    fn ask(&mut self, slot: usize, record: (u32, u32), reaction: bool) -> [i16; 2];
    fn ask_nothing(&mut self, _slot: usize) {}
    /// Such a slot is let go at its delay like every other (`0x800BE858`: the events of
    /// the clock look at the delay of a slot, not at the kind of its record). For the
    /// player this "go" is what brings his order state from 4 back to 1.
    fn go_on_nothing(&mut self, _slot: usize) {}
    /// The second test (`0x800BDFD4`): everybody is at the hold of its move.
    fn ready(&mut self) -> bool;
    /// A slot is let go from its hold.
    fn go_on(&mut self, slot: usize);
    fn own_command_goes(&mut self, _slot: usize, _kind: u32) {}
    fn shake(&mut self) {}
    /// The screen is made white for three passes (`0x800BF57C`, which hands white to
    /// the executable's maker of the screen in one colour): the shown change of a slot
    /// is a critical hit.
    fn flash(&mut self) {}
    fn sound(&mut self, _number: u32) {}
    /// A row of the fight's sequences is started (`0x800311BC`).
    fn row(&mut self, _row: usize) {}
    /// A number task is started for a slot (`0x800BF5AC`): from this game frame on it
    /// counts the amount, a loss above 0, into the record of the slot (way 0 the HP,
    /// 1 the MP, 2 neither; `hud::Number`).
    fn number(&mut self, _slot: usize, _way: u8, _amount: i32) {}
    /// The status of a slot loses and gets bits at its delay (`0x800BE90C`).
    fn give(&mut self, _slot: usize, _clear: u32, _add: u32) {}
    /// An opponent of the removal list leaves the field.
    fn leave(&mut self, slot: usize);
}

/// Where the runner's task stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// State 1: the command is resolved and the camera asked for.
    Resolves,
    /// State 2: the wait for everybody to be free; then the times and the delays.
    Waits,
    /// State 3: the wait for everybody to be at its hold.
    Holds,
    /// State 4: the clock runs.
    Clocked,
    Done,
}

/// The delay of a participant and the game frame of the clock in which its move is
/// over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Timed {
    pub delay: i32,
    pub end: i32,
}

/// A run of the action runner that is being shown.
#[derive(Debug, Clone)]
pub struct Showing {
    run: Run,
    state: State,
    clock: i32,
    /// L, the largest K: the game frame of the clock all key moments fall on.
    longest: i32,
    actor: Timed,
    slots: [Timed; SLOTS],
    /// The opponents that leave the field when the run ends.
    removal: Vec<usize>,
    /// The slots that were asked for nothing.
    nothing: u8,
}

/// The delays of "Timing" (`0x800BE498`): L is the largest K, compared as signed
/// numbers, the delay of each is L minus its K, and its end its E plus its delay.
pub fn delays(times: &[[i16; 2]]) -> (i32, Vec<Timed>) {
    let longest = times.iter().map(|pair| i32::from(pair[0])).max();
    let longest = longest.unwrap_or(0);
    let timed = (times.iter())
        .map(|&[key, whole]| {
            let delay = longest - i32::from(key);
            Timed {
                delay,
                end: i32::from(whole) + delay,
            }
        })
        .collect();
    (longest, timed)
}

pub fn turns(actors: u8, targets: u8) -> [Option<(u8, u8)>; 2] {
    let as_one = |mask: u8| match mask & 0xF {
        0 => mask,
        _ => mask & !0xF | 1,
    };
    let (acting, aimed) = (as_one(actors), as_one(targets));
    let (to_targets, to_actors) = (aimed & !acting, acting & !aimed);
    [
        (to_targets != 0 && actors != 0).then_some((to_targets, actors)),
        (to_actors != 0 && targets != 0).then_some((to_actors, targets)),
    ]
}

/// The mean place of the game (`0x800BC7BC` with the midpoint `0x8003B960`): every place
/// is replaced by the midpoint of it and the next one, the halves rounded towards zero,
/// until one is left. So two places give their midpoint and three give the middle one
/// twice the weight of the outer two. None for no place.
pub fn mean(places: &[[i32; 3]]) -> Option<[i32; 3]> {
    let mut places = places.to_vec();
    while places.len() > 1 {
        places = (places.windows(2))
            .map(|pair| std::array::from_fn(|axis| pair[0][axis].wrapping_add(pair[1][axis]) / 2))
            .collect();
    }
    places.first().copied()
}

/// Whether a record that an opponent is let go with puts it on the removal list: it
/// falls, dies of the stone, runs away, or is caught (the grades 0 and 2 of a capture).
pub fn removes(kind: u32, parameter: u32) -> bool {
    matches!(kind, STONE_DEATH | FALLS | RUNS_AWAY)
        || CAPTURES.contains(&kind) && matches!(parameter, 0 | 2)
}

impl Showing {
    pub fn of(run: Run) -> Self {
        Self {
            run,
            state: State::Resolves,
            clock: 0,
            longest: 0,
            actor: Timed::default(),
            slots: [Timed::default(); SLOTS],
            removal: Vec::new(),
            nothing: 0,
        }
    }

    pub fn run(&self) -> &Run {
        &self.run
    }

    pub fn state(&self) -> State {
        self.state
    }

    /// The clock of state 4, and L.
    pub fn clock(&self) -> i32 {
        self.clock
    }

    pub fn longest(&self) -> i32 {
        self.longest
    }

    /// The delay and the end of the actor, and of a slot's reaction.
    pub fn actor(&self) -> Timed {
        self.actor
    }

    pub fn slot(&self, slot: usize) -> Timed {
        self.slots[slot]
    }

    /// The acting slot; none for the second runner, which is nobody's command.
    fn acting(&self) -> Option<usize> {
        (self.run.actors != 0).then(|| self.run.actors.trailing_zeros() as usize)
    }

    fn of_mask(mask: u8) -> impl Iterator<Item = usize> {
        (0..SLOTS).filter(move |slot| mask & 1 << slot != 0)
    }

    /// One game frame of the runner's task. True in the game frame the run is over.
    pub fn step(&mut self, field: &mut impl Field) -> bool {
        match self.state {
            State::Resolves => {
                // What the command's handler has started in front of the runner.
                if let Some(row) = self.run.sounds.start {
                    field.row(row);
                }
                self.state = State::Waits;
            }
            State::Waits => {
                if field.free() {
                    self.ask(field);
                    self.state = State::Holds;
                }
            }
            State::Holds => {
                if field.ready() {
                    self.removal.clear();
                    self.clock = 0;
                    self.state = State::Clocked;
                }
            }
            State::Clocked => {
                // No event runs in the game frame of the end.
                if self.longest < self.clock && self.ends() {
                    for slot in std::mem::take(&mut self.removal) {
                        field.leave(slot);
                    }
                    self.state = State::Done;
                    return true;
                }
                self.events(field);
                self.clock += 1;
            }
            State::Done => return true,
        }
        false
    }

    /// The times and the delays (`0x800BE144`, `0x800BE17C`, `0x800BE498`): the actor is
    /// asked with the command, every opponent of its mask in slot order into one pair
    /// of times, so the last one's are the answer; then every slot but the acting one
    /// with its reaction record. Kind 20 has the times (0, 0): a slot that is left with
    /// it is told so ([`Field::ask_nothing`]). In front of the times the fighters are
    /// turned ([`turns`], `0x800BE08C`).
    fn ask(&mut self, field: &mut impl Field) {
        for (to, turned) in turns(self.run.actors, self.run.targets)
            .into_iter()
            .flatten()
        {
            field.turn(to, turned);
        }
        let command = (self.run.kind, self.run.parameter);
        let mut times = [[0i16; 2]; SLOTS + 1];
        let mut asked = 0u8;
        if command.0 != NOTHING {
            for slot in Self::of_mask(self.run.actors) {
                times[SLOTS] = field.ask(slot, command, false);
                asked |= 1 << slot;
            }
        }
        let acting = self.acting();
        for shown in &self.run.reactions {
            if Some(shown.slot) != acting && shown.kind != NOTHING && shown.slot < SLOTS {
                times[shown.slot] = field.ask(shown.slot, (shown.kind, shown.parameter), true);
                asked |= 1 << shown.slot;
            }
        }
        self.nothing = !asked;
        for slot in Self::of_mask(self.nothing) {
            field.ask_nothing(slot);
        }
        let (longest, timed) = delays(&times);
        self.longest = longest;
        self.slots.copy_from_slice(&timed[..SLOTS]);
        self.actor = timed[SLOTS];
    }

    /// The events of the clock that concern the fighters (`0x800BE590`, `0x800BE31C`):
    /// the actor is let go at its delay and every slot with a reaction at its own, and
    /// an opponent whose record takes it off the field goes on the removal list.
    fn events(&mut self, field: &mut impl Field) {
        let command = (self.run.kind, self.run.parameter);
        if self.clock == self.actor.delay && command.0 != NOTHING {
            for slot in Self::of_mask(self.run.actors) {
                field.go_on(slot);
                if slot >= OPPONENTS && removes(command.0, command.1) {
                    self.removal.push(slot);
                }
                field.own_command_goes(slot, command.0);
                // The two numbers of the actor's own words, whatever they are.
                field.number(slot, 0, self.run.own.0);
                if self.run.own_loss() {
                    field.shake();
                    field.sound(sound::HIT_SOUND);
                }
                field.number(slot, 1, self.run.own.1);
                // Anybody but the player who runs away.
                if command.0 == RUNS_AWAY && slot != PLAYER {
                    field.row(sound::FLIGHT_ROW);
                }
            }
        }
        // The key moment, in front of the slots' own events: the sound of the targets
        // and what is heard of a fall.
        if self.clock == self.longest {
            if let Some(hit) = self.run.sounds.hit {
                field.sound(hit);
            }
            match self.run.sounds.fall {
                Some(Fall::Row) => field.row(sound::FALL_ROW),
                Some(Fall::Sound) => field.sound(sound::FALL_SOUND),
                None => {}
            }
        }
        let acting = self.acting();
        for shown in &self.run.reactions {
            let asked = Some(shown.slot) != acting && shown.kind != NOTHING && shown.slot < SLOTS;
            if asked && self.clock == self.slots[shown.slot].delay {
                field.go_on(shown.slot);
                if shown.slot >= OPPONENTS && removes(shown.kind, shown.parameter) {
                    self.removal.push(shown.slot);
                }
            }
        }
        for slot in Self::of_mask(self.nothing) {
            if self.clock == self.slots[slot].delay {
                field.go_on_nothing(slot);
            }
        }
        // The change of a slot is shown at the slot's delay, the acting one's too: its
        // status bits are cleared and added, and the numbers of its shown kind start.
        for given in &self.run.given {
            if given.slot < SLOTS && self.clock == self.slots[given.slot].delay {
                field.give(given.slot, given.clear, given.add);
            }
        }
        for counted in &self.run.counted {
            if counted.slot < SLOTS && self.clock == self.slots[counted.slot].delay {
                field.number(counted.slot, counted.way, counted.amount);
            }
        }
        for slot in Self::of_mask(self.run.shakes) {
            if slot < SLOTS && self.clock == self.slots[slot].delay {
                // A critical hit makes the screen white in front of its shake.
                if self.run.flashes & 1 << slot != 0 {
                    field.flash();
                }
                field.shake();
            }
        }
    }

    fn numbers(&self) -> impl Iterator<Item = i32> + '_ {
        let own = (self.acting().filter(|_| self.run.kind != NOTHING)).map(|_| self.actor.delay);
        let slots = (self.run.counted.iter())
            .filter(|counted| counted.slot < SLOTS)
            .map(|counted| self.slots[counted.slot].delay);
        own.into_iter().chain(slots)
    }

    /// The end test (`0x800BEC64`): no number task counts; then the clock is behind the
    /// actor's end, or, for the capture, not in front of any slot's; when the player
    /// fell, both. A number task has its first pass in the game frame it is made in,
    /// behind the runner's, and ends in its seventeenth: the runner finds it for the
    /// sixteen game frames behind its start.
    fn ends(&self) -> bool {
        let clock = self.clock;
        if self
            .numbers()
            .any(|start| start < clock && clock <= start + NUMBER_COUNTS)
        {
            return false;
        }
        let actor = self.actor.end < clock;
        let everybody = self.slots.iter().all(|slot| slot.end <= clock);
        match (self.run.player_fell, CAPTURES.contains(&self.run.kind)) {
            (true, _) => actor && everybody,
            (false, true) => everybody,
            (false, false) => actor,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::fight::{Counted, Given, Shown, Sounds};
    use super::*;

    #[test]
    fn the_actors_turn_to_the_targets_and_the_targets_to_the_actors() {
        // The player attacks two opponents: he turns to their mean place, they to him.
        assert_eq!(
            turns(1 << 0, 1 << 4 | 1 << 5),
            [Some((0x30, 0x01)), Some((0x01, 0x30))]
        );
        // His creature in slot 2 counts as the player's side, and is the one turned.
        assert_eq!(
            turns(1 << 2, 1 << 4),
            [Some((0x10, 0x04)), Some((0x01, 0x10))]
        );
        // A move at himself turns nobody, nor does one without targets or actors.
        assert_eq!(turns(1 << 0, 1 << 0), [None, None]);
        assert_eq!(turns(1 << 4, 0), [None, None]);
        assert_eq!(turns(0, 1 << 0), [None, None]);
        // An opponent that aims at the player and itself: both turn to the player.
        assert_eq!(turns(1 << 4, 1 << 4 | 1 << 0), [Some((0x01, 0x10)), None]);
    }

    #[test]
    fn the_mean_place_is_made_of_midpoints() {
        assert_eq!(mean(&[]), None);
        assert_eq!(mean(&[[7, -3, 2]]), Some([7, -3, 2]));
        // The halves are rounded towards zero.
        assert_eq!(mean(&[[0, 0, 0], [5, -5, 10]]), Some([2, -2, 5]));
        // Three places: the middle one counts twice.
        assert_eq!(
            mean(&[[0, 0, 0], [400, 0, 80], [800, 0, 0]]),
            Some([400, 0, 40])
        );
    }

    /// A field whose fighters answer fixed times; it notes what the runner does.
    #[derive(Default)]
    struct Made {
        /// The game frames the first test says no for.
        busy: u32,
        times: [[i16; 2]; SLOTS],
        asked: Vec<(usize, u32, bool)>,
        turned: Vec<(u8, u8)>,
        nothing: Vec<usize>,
        went_for_nothing: Vec<usize>,
        went: Vec<usize>,
        /// The own commands let go: slot, kind and how many were let go by then.
        own: Vec<(usize, u32, usize)>,
        /// How many were let go when the picture was shaken.
        shaken: Vec<usize>,
        /// The sounds played and the rows started, each with how many were let go by
        /// then.
        sounds: Vec<(u32, usize)>,
        rows: Vec<(usize, usize)>,
        /// The numbers started (slot, way, amount) and the bits given (slot, cleared,
        /// added), each with how many were let go by then.
        numbers: Vec<(usize, u8, i32, usize)>,
        given: Vec<(usize, u32, u32, usize)>,
        left: Vec<usize>,
    }

    impl Field for Made {
        fn free(&mut self) -> bool {
            let free = self.busy == 0;
            self.busy = self.busy.saturating_sub(1);
            free
        }
        fn ask(&mut self, slot: usize, record: (u32, u32), reaction: bool) -> [i16; 2] {
            self.asked.push((slot, record.0, reaction));
            match reaction && record.0 != 18 {
                true => [0, self.times[slot][1]],
                false => self.times[slot],
            }
        }
        fn turn(&mut self, to: u8, turned: u8) {
            self.turned.push((to, turned));
        }
        fn ask_nothing(&mut self, slot: usize) {
            self.nothing.push(slot);
        }
        fn go_on_nothing(&mut self, slot: usize) {
            self.went_for_nothing.push(slot);
        }
        fn ready(&mut self) -> bool {
            true
        }
        fn go_on(&mut self, slot: usize) {
            self.went.push(slot);
        }
        fn own_command_goes(&mut self, slot: usize, kind: u32) {
            self.own.push((slot, kind, self.went.len()));
        }
        fn shake(&mut self) {
            self.shaken.push(self.went.len());
        }
        fn sound(&mut self, number: u32) {
            self.sounds.push((number, self.went.len()));
        }
        fn row(&mut self, row: usize) {
            self.rows.push((row, self.went.len()));
        }
        fn number(&mut self, slot: usize, way: u8, amount: i32) {
            self.numbers.push((slot, way, amount, self.went.len()));
        }
        fn give(&mut self, slot: usize, clear: u32, add: u32) {
            self.given.push((slot, clear, add, self.went.len()));
        }
        fn leave(&mut self, slot: usize) {
            self.left.push(slot);
        }
    }

    fn run(kind: u32, actor: usize, reactions: &[(usize, u32, u32)]) -> Run {
        Run {
            kind,
            parameter: 0,
            actors: 1 << actor,
            targets: reactions.iter().fold(0, |mask, shown| mask | 1 << shown.0),
            reactions: (reactions.iter())
                .map(|&(slot, kind, parameter)| Shown {
                    slot,
                    kind,
                    parameter,
                })
                .collect(),
            own: (0, 0),
            counted: Vec::new(),
            given: Vec::new(),
            ticked: Vec::new(),
            shakes: 0,
            flashes: 0,
            effect_bytes: [0; SLOTS],
            player_fell: false,
            before: 0,
            camera: None,
            sounds: Sounds::default(),
        }
    }

    /// The game frames a run takes, and the clock of each "go on".
    fn played(showing: &mut Showing, field: &mut Made) -> (u32, Vec<(usize, i32)>) {
        let (mut frames, mut went) = (0, Vec::new());
        loop {
            let before = field.went.len();
            let over = showing.step(field);
            frames += 1;
            went.extend(
                field.went[before..]
                    .iter()
                    .map(|slot| (*slot, showing.clock - 1)),
            );
            assert!(frames < 10_000, "the run does not end");
            if over {
                return (frames, went);
            }
        }
    }

    #[test]
    fn the_delays_put_every_key_moment_on_the_largest() {
        let (longest, timed) = delays(&[[24, 74], [0, 29], [-1, 10]]);
        assert_eq!(longest, 24);
        let pairs: Vec<_> = timed.iter().map(|timed| (timed.delay, timed.end)).collect();
        // A K of -1 gives a delay of L + 1.
        assert_eq!(pairs, [(0, 74), (24, 53), (25, 35)]);
    }

    #[test]
    fn an_attack_lets_the_target_go_at_the_key_moment_and_ends_with_the_actor() {
        // An opponent attacks the player: the attack 24 and 74, the player hurt 29.
        let mut field = Made::default();
        (field.times[4], field.times[0]) = ([24, 74], [0, 29]);
        let mut showing = Showing::of(run(0, 4, &[(0, 9, 0)]));
        let (frames, went) = played(&mut showing, &mut field);
        assert_eq!(field.asked, [(4, 0, false), (0, 9, true)]);
        // The opponent would turn to the player and does nothing; the player turns
        // to the opponent. Every other slot is asked for nothing.
        assert_eq!(field.turned, [(1 << 0, 1 << 4), (1 << 4, 1 << 0)]);
        assert_eq!(field.nothing, [1, 2, 3, 5, 6]);
        // And let go like the others, once, at the key moment.
        assert_eq!(field.went_for_nothing, [1, 2, 3, 5, 6]);
        assert_eq!(showing.longest(), 24);
        assert_eq!(went, [(4, 0), (0, 24)]);
        // One game frame each of the states 1 to 3, then the clock from 0 to the first
        // game frame behind the actor's end, 75.
        assert_eq!(frames, 3 + 76);
        assert!(field.left.is_empty());
    }

    #[test]
    fn a_shown_change_shakes_the_picture_at_the_delay_of_its_slot() {
        // An opponent hits the player: he is let go at the key moment, and the picture
        // is shaken in that game frame of the clock, once.
        let mut field = Made::default();
        (field.times[4], field.times[0]) = ([24, 74], [0, 29]);
        let mut hit = run(0, 4, &[(0, 9, 0)]);
        hit.shakes = 1 << 0;
        let mut showing = Showing::of(hit);
        let (_, went) = played(&mut showing, &mut field);
        assert_eq!(went, [(4, 0), (0, 24)]);
        assert_eq!(field.shaken, [2]);

        // A run that shows nothing of the kind shakes nothing.
        let mut field = Made::default();
        (field.times[4], field.times[0]) = ([24, 74], [0, 29]);
        played(&mut Showing::of(run(0, 4, &[(0, 12, 0)])), &mut field);
        assert!(field.shaken.is_empty());

        // The actor's own loss shakes at the actor's delay: a reaction of a turn's
        // start, the command of its slot. Its key moment is the largest, so that is
        // the first game frame of the clock.
        let mut field = Made::default();
        field.times[0] = [10, 29];
        let mut own = run(9, 0, &[]);
        own.own = (3, 0);
        played(&mut Showing::of(own), &mut field);
        assert_eq!(field.shaken, [1]);
    }

    #[test]
    fn the_sounds_of_a_run_are_played_at_its_key_moment() {
        // An opponent hits the player, who falls: the sound of the hit and the row of
        // his fall come at the key moment, in front of his own go.
        let mut field = Made::default();
        (field.times[4], field.times[0]) = ([24, 74], [0, 120]);
        let mut hit = run(0, 4, &[(0, 11, 0)]);
        hit.sounds = Sounds {
            hit: Some(sound::HIT_SOUND),
            fall: Some(Fall::Row),
            start: None,
        };
        let (_, went) = played(&mut Showing::of(hit), &mut field);
        assert_eq!(went, [(4, 0), (0, 24)]);
        assert_eq!(field.sounds, [(114, 1)]);
        assert_eq!(field.rows, [(28, 1)]);

        // An opponent that falls without the row is heard as a sound, behind a miss
        // too; a run without either plays nothing.
        let mut field = Made::default();
        (field.times[0], field.times[4]) = ([10, 30], [0, 149]);
        let mut fall = run(0, 0, &[(4, 11, 0)]);
        fall.sounds = Sounds {
            hit: Some(sound::MISS_SOUND),
            fall: Some(Fall::Sound),
            start: None,
        };
        played(&mut Showing::of(fall), &mut field);
        assert_eq!(
            (field.sounds, field.rows),
            (vec![(112, 1), (111, 1)], vec![])
        );
        let mut field = Made::default();
        (field.times[0], field.times[4]) = ([10, 30], [0, 149]);
        played(&mut Showing::of(run(0, 0, &[(4, 9, 0)])), &mut field);
        assert!(field.sounds.is_empty() && field.rows.is_empty());

        // What a command's handler has started is heard in the runner's first state.
        let mut field = Made::default();
        field.times[1] = [29, 29];
        let mut leave = run(3, 1, &[]);
        leave.sounds.start = Some(sound::CHANGE_ROW);
        played(&mut Showing::of(leave), &mut field);
        let rows: Vec<_> = field.rows.iter().map(|row| row.0).collect();
        assert_eq!(rows, [sound::CHANGE_ROW]);

        // The actor's own loss is heard where it shakes the picture, at its delay.
        let mut field = Made::default();
        field.times[0] = [10, 29];
        let mut own = run(9, 0, &[]);
        own.own = (3, 0);
        played(&mut Showing::of(own), &mut field);
        assert_eq!((field.shaken, field.sounds), (vec![1], vec![(114, 1)]));

        // An opponent that runs away starts the row when it is let go; the player
        // does not.
        let mut field = Made::default();
        field.times[4] = [24, 74];
        played(&mut Showing::of(run(7, 4, &[])), &mut field);
        assert_eq!(field.rows, [(28, 1)]);
        let mut field = Made::default();
        field.times[0] = [24, 74];
        played(&mut Showing::of(run(7, 0, &[])), &mut field);
        assert!(field.rows.is_empty());
    }

    #[test]
    fn a_run_waits_until_everybody_is_free() {
        let mut field = Made {
            busy: 40,
            ..Made::default()
        };
        field.times[4] = [24, 74];
        let mut showing = Showing::of(run(0, 4, &[]));
        let (frames, _) = played(&mut showing, &mut field);
        assert_eq!(frames, 40 + 3 + 76);
    }

    #[test]
    fn a_number_that_counts_holds_the_end_back() {
        // The player's own times are 0 here: by them the run would end at clock 1. The
        // two numbers of his own words are made at his delay, whatever they hold, and
        // the runner finds them for sixteen game frames behind it: the end is clock 17.
        let mut field = Made::default();
        field.times[4] = [0, 29];
        let mut short = run(0, 0, &[(4, 9, 0)]);
        let (frames, went) = played(&mut Showing::of(short.clone()), &mut field);
        assert_eq!((frames, went), (3 + 18, vec![(0, 0), (4, 0)]));
        assert_eq!(field.numbers, [(0, 0, 0, 1), (0, 1, 0, 1)]);
        // The target's HP changes from its delay on, which is his key moment.
        let mut field = Made::default();
        (field.times[0], field.times[4]) = ([5, 6], [0, 9]);
        short.counted = vec![Counted {
            slot: 4,
            way: 0,
            amount: 7,
        }];
        let (frames, _) = played(&mut Showing::of(short), &mut field);
        assert_eq!(frames, 3 + 5 + 18);
    }

    #[test]
    fn the_numbers_and_the_bits_come_at_the_delays() {
        // An opponent's move costs it 2 MP, takes 7 HP from the player and poisons
        // him: its own two numbers start when it is let go, his bits and his number
        // at the key moment, behind his go.
        let mut field = Made::default();
        (field.times[4], field.times[0]) = ([24, 74], [0, 29]);
        let mut hit = run(0, 4, &[(0, 9, 0)]);
        hit.own = (0, 2);
        hit.counted = vec![Counted {
            slot: 0,
            way: 0,
            amount: 7,
        }];
        hit.given = vec![Given {
            slot: 0,
            clear: 0,
            add: 1,
        }];
        played(&mut Showing::of(hit), &mut field);
        assert_eq!(field.numbers, [(4, 0, 0, 1), (4, 1, 2, 1), (0, 0, 7, 2)]);
        assert_eq!(field.given, [(0, 0, 1, 2)]);
        // The second runner is nobody's: it starts no number.
        let mut field = Made::default();
        field.times[4] = [10, 29];
        let mut second = run(20, 0, &[(4, 18, 0)]);
        second.actors = 0;
        played(&mut Showing::of(second), &mut field);
        assert!(field.numbers.is_empty());
    }

    #[test]
    fn an_opponent_that_falls_leaves_the_field_when_the_run_ends() {
        let mut field = Made::default();
        (field.times[0], field.times[4]) = ([10, 30], [0, 149]);
        let mut showing = Showing::of(run(0, 0, &[(4, 11, 0)]));
        let (frames, went) = played(&mut showing, &mut field);
        // The end is the actor's, in the middle of the fall.
        assert_eq!((frames, went), (3 + 32, vec![(0, 0), (4, 10)]));
        assert_eq!(field.left, [4]);
    }

    #[test]
    fn a_capture_waits_for_everybody_and_only_a_catch_removes() {
        for (grade, left) in [(0, vec![5]), (1, vec![]), (2, vec![5])] {
            let mut field = Made::default();
            (field.times[0], field.times[5]) = ([20, 40], [0, 218]);
            let mut showing = Showing::of(run(5, 0, &[(5, 5, grade)]));
            let (frames, _) = played(&mut showing, &mut field);
            // The opponent is let go at clock 20 and its list takes 218.
            assert_eq!(frames, 3 + 238 + 1);
            assert_eq!(field.left, left);
            // Only his own command is followed by the camera's start, right behind
            // his go; the opponent's reaction is not.
            assert_eq!(field.own, [(0, 5, 1)]);
        }
        assert!(removes(7, 0) && removes(19, 0) && removes(26, 2));
        assert!(!removes(9, 0) && !removes(26, 1));
    }

    #[test]
    fn everybody_is_waited_for_when_the_player_fell() {
        let mut field = Made::default();
        (field.times[4], field.times[0]) = ([24, 74], [0, 120]);
        let mut fell = run(0, 4, &[(0, 11, 0)]);
        fell.player_fell = true;
        let (frames, _) = played(&mut Showing::of(fell), &mut field);
        // His fall ends at clock 24 + 120.
        assert_eq!(frames, 3 + 145);
    }

    #[test]
    fn the_second_runner_asks_only_the_reactions() {
        let mut field = Made::default();
        field.times[4] = [10, 29];
        let mut second = run(20, 0, &[(4, 18, 0)]);
        second.actors = 0;
        let mut showing = Showing::of(second);
        let (frames, went) = played(&mut showing, &mut field);
        // Kind 18 keeps its K as a reaction; nobody's end is 10, the largest K.
        assert_eq!(field.asked, [(4, 18, true)]);
        assert_eq!((frames, went), (3 + 12, vec![(4, 0)]));
        assert!(field.own.is_empty());
    }
}
