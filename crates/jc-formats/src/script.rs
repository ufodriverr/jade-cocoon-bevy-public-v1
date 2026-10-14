
use crate::notice::Shown;
use crate::scene::{COMMAND_END, COMMAND_GOTO, COMMAND_JUMP, COMMAND_LABEL, Command, ScenePackage};
use crate::window::{SETTING_AT_START, Where, Window};

/// `0x01`: wait for the confirm button.
pub const COMMAND_CONFIRM: u32 = 0x01;
/// `0x04`: close the section.
pub const COMMAND_CLOSE: u32 = 0x04;
/// `0x05`: open a section.
pub const COMMAND_OPEN: u32 = 0x05;
/// `0x06`: begin or end a polling block.
pub const COMMAND_POLL: u32 = 0x06;
/// `0x07`: take things.
pub const COMMAND_TAKE: u32 = 0x07;
/// `0x08`: give things back.
pub const COMMAND_GIVE_BACK: u32 = 0x08;
/// `0x0C`: wait a number of game frames.
pub const COMMAND_WAIT: u32 = 0x0C;
/// `0x0D`: wait for things that are busy with an order.
pub const COMMAND_WAIT_FOR: u32 = 0x0D;
/// `0x0E`: set a variable.
pub const COMMAND_SET: u32 = 0x0E;
/// `0x0F`: the choice index from a local variable.
pub const COMMAND_COUNT_LOCAL: u32 = 0x0F;
/// `0x10`: the choice index from a story variable.
pub const COMMAND_COUNT_STORY: u32 = 0x10;

/// The things a script can hold: the player and four actor slots of the room.
pub const THINGS: usize = 5;

/// The order kinds whose "go on" the order routine notes in the task's mask at +28.
const KIND_NOTED: u32 = 3;
const KIND_UNNOTED: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Order {
    /// The kind, at +4: 6 show, 7 walk, 8 carry, 9 an action, 10 two actions and so on.
    pub kind: u32,
    /// The values at +12 and +16 and the bits at +20.
    pub value: u32,
    pub second: u32,
    pub bits: u32,
    /// The address at +36: the command list of an order of kind 5. For kind 11 the
    /// number of the room's path, whose record the handler puts there.
    pub list: u32,
    /// The three words at +24: the place of an order of kind 0, 1 or 2. Such an order
    /// has its action, or -1 for the thing's own, in `value` (the game keeps it at +20,
    /// where the other kinds have their bits).
    pub place: [i32; 3],
}

/// The things of a room as the order routine asks them: thing 0 is the player, thing n
/// the actor in slot n - 1.
pub trait Things {
    /// The thing can take an order (`0x800BBC70`): the player's order state is 9, an
    /// actor runs no move and has no target. A slot without an actor is never free.
    fn free(&mut self, thing: usize) -> bool;
    /// The order is given (`0x800BBCD8`). `told` is the word at +8 of the record: the
    /// script wants to be told when the move is over.
    fn give(&mut self, thing: usize, order: &Order, told: bool);
    /// The thing is ready to go on (`0x800BBCA4`): an actor stands at its hold.
    fn ready(&mut self, thing: usize) -> bool;
    /// The thing goes on (`0x800BBD1C`). True when that ends the move at once, which the
    /// game tells the script tasks from inside the call.
    fn go_on(&mut self, thing: usize, order: &Order) -> bool;
}
/// The local variables of a task.
pub const LOCALS: usize = 8;
/// The variable number that makes `0x0F` draw a random number.
pub const RANDOM: u32 = 9;
/// Commands one pass may run before it is taken for a script that never waits. The
/// console has no such limit: there the game would stand still.
pub const PASS_LIMIT: usize = 0x10000;

/// What a handler answers: the script goes on with its current command in this game
/// frame, or the handler is asked again in the next one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    GoOn,
    Wait,
}

/// Where a script cannot go on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    /// The current command is not in the package, or is no command.
    NoCommand(u32),
    /// A command names a local variable the task does not have.
    NoVariable(u32),
    /// `0x0F` asks for a random number below 0.
    NoRange(u32),
    /// A pass ran [`PASS_LIMIT`] commands without a wait.
    NeverWaits(u32),
}

impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoCommand(at) => write!(f, "no command at {at:#010x}"),
            Self::NoVariable(at) => write!(f, "no such local variable at {at:#010x}"),
            Self::NoRange(at) => write!(f, "a random number of no range at {at:#010x}"),
            Self::NeverWaits(at) => write!(f, "the script never waits, last at {at:#010x}"),
        }
    }
}

impl std::error::Error for Fault {}

/// Where the commands of a script are read from.
pub trait Commands {
    /// The command at a RAM address.
    fn command_at(&self, address: u32) -> Option<Command>;
}

impl Commands for ScenePackage<'_> {
    fn command_at(&self, address: u32) -> Option<Command> {
        ScenePackage::command_at(self, address)
    }
}

/// What a script task asks of the game around it.
pub trait Engine {
    /// The task selector `0x800AEDA8`: the number of the task that has an open section,
    /// below 0 when none has.
    fn selector(&mut self) -> &mut i32;
    /// `0x01`: the confirm bit (`0x100`) of the pad word `0x800918DC` is set.
    fn confirm(&mut self) -> bool;
    /// `0x05`: the engine lets a section open (the word `0x800B1090` is 1, the word
    /// `0x800B1430` is 0 and the engine's states 0 and 2 agree).
    fn may_open(&mut self) -> bool;
    /// `0x05` opened a section (`0x800A1894`).
    fn opened(&mut self);
    /// `0x04` closed the last section of a task that holds nothing (`0x800A1904`).
    fn closed(&mut self);
    /// `0x07`: takes a thing (0 the player, n the actor slot n - 1) for the task; `false`
    /// while an actor's slot is not free. The player is always given.
    fn take(&mut self, thing: usize) -> bool;
    /// `0x08` and `0x04`: gives a thing back.
    fn give_back(&mut self, thing: usize);
    /// `0x04`, only for a task whose mask at +28 names things it holds: an order for those
    /// things through the order routine `0x800BBB50`; which order the handler makes was
    /// not read. `true` lets the close go on.
    fn settle(&mut self, things: u32) -> bool;
    /// A story variable (`0x800AD324`).
    fn story_variable(&mut self, number: u32) -> i32;
    /// Writes a story variable (`0x800AD33C`).
    fn set_story_variable(&mut self, number: u32, value: i32);
    /// The library's random number (`0x8004E2BC`), not below 0.
    fn random(&mut self) -> i32;
    /// Every command that does not steer the script. A handler that answers
    /// [`Answer::GoOn`] has moved the task on itself, with [`Task::next`] as a rule.
    fn run(&mut self, task: &mut Task, command: Command) -> Answer;
    /// A confirm was taken in the message window of a task that is busy with this
    /// thing: the player's `0x800AC4E8`, an actor's `0x8001DA18`.
    fn confirmed(&mut self, _thing: usize) {}
    /// The game's setting word `0x8008B51C`, which the waiter of a message reads.
    fn setting(&mut self) -> u32 {
        SETTING_AT_START
    }
}

/// Bit 3 of the setting word: the waiter lets a window take a confirm while a line of
/// the voices plays, so a confirm cuts the line short. A new game has it set.
const WAITS_FOR_NO_LINE: u32 = 0x08;

/// The step of the message handler `0x800BA1F0` (+180 of the task) that a game frame can
/// end in; the others are passed inside one call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Said {
    /// Step 0: no message command runs.
    #[default]
    Nothing,
    /// Step 3: the waiter is asked.
    Waited,
    /// Step 5: the window was closed and has not gone yet.
    Leaving,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Speech {
    step: Said,
    /// Flag `0x40`: the message did not make the script wait, so the task's update asks
    /// the waiter behind the commands of every game frame.
    follows: bool,
    /// Flag `0x80`: the confirm at the end of such a message was taken.
    confirmed: bool,
    /// Flag `0x02`: the window was let take a confirm, which is waited for.
    allowed: bool,
    /// The things that a taken confirm is to be told to (`0x800A6744`), until
    /// [`Task::tell_confirm`] has done it.
    tells: u32,
    /// A line of the voices plays (`0x80040E20` answers 0), as [`Task::hear`] was told
    /// last. The waiter asks it while bit 3 of the setting word is clear.
    line_plays: bool,
    /// The window. It is a task of its own, which whoever runs the room steps once a
    /// game frame behind the actors; the script only asks it and tells it.
    pub window: Option<Window>,
    /// The notice (`0x3B` to `0x3E`), a task of the same pool: never up with a window.
    pub notice: Option<Shown>,
    /// The answer of the last question.
    pub answer: Option<i16>,
}

/// The task of one script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    /// The task's own number, which the selector names.
    pub number: i32,
    /// The first command of the current segment.
    pub segment: u32,
    /// The current command.
    pub current: u32,
    /// The mark: the last `0x06` that began a polling block, or the target of a jump.
    pub mark: u32,
    /// The script is in a section of its own.
    pub in_section: bool,
    /// A polling block runs: the next game frame starts at the mark again.
    pub polls: bool,
    /// The things the script holds, one bit each.
    pub held: u32,
    /// The things an order is still busy with, which `0x0D` waits on. Kept by whoever
    /// gives the orders.
    pub busy: u32,
    /// The mask at +28 of the task, which the control flow only reads (see
    /// [`Engine::settle`]).
    pub settles: u32,
    /// The local variables.
    pub locals: [i32; LOCALS],
    /// The choice index: which of the records behind a `0x0A` or `0x0B` counts.
    pub choice: i32,
    /// The answers that are switched off for the task's questions, one bit each (+276;
    /// `0x43`).
    pub closed_answers: u32,
    /// The place of the task's windows when a script has named one (+260; `0x35`).
    pub window_place: Option<Where>,
    /// Game frames the running `0x0C` has waited.
    waited: u32,
    /// The step the running `0x04` is in.
    closing: u8,
    /// The order routine: the things the running order still has to reach (+56), whether
    /// an order runs (+188) and whether the thing at hand has it and is waited for (+192).
    ordering: u32,
    orders: bool,
    given: bool,
    /// The step the running `0x45` is in (+184).
    moved: bool,
    /// The message of the task and its window.
    pub speech: Speech,
    ended: bool,
}

impl Task {
    /// A task at the first command of a script.
    pub fn new(number: i32, address: u32) -> Self {
        Self {
            number,
            segment: address,
            current: address,
            mark: address,
            in_section: false,
            polls: false,
            held: 0,
            busy: 0,
            settles: 0,
            locals: [0; LOCALS],
            choice: 0,
            closed_answers: 0,
            window_place: None,
            waited: 0,
            closing: 0,
            ordering: 0,
            orders: false,
            given: false,
            moved: false,
            speech: Speech::default(),
            ended: false,
        }
    }

    /// `0x35`: names one of the three places for the task's windows, or with a negative
    /// number gives the place back to the command that opens a window.
    pub fn place_windows(&mut self, place: i32) {
        self.window_place = match place {
            0 => Some(Where::Top),
            1 => Some(Where::Middle),
            2 => Some(Where::Bottom),
            _ => None,
        };
    }

    /// `0x43`: switches answers of the task's questions off or on again.
    pub fn switch_answers(&mut self, off: bool, answers: u32) {
        match off {
            true => self.closed_answers |= answers,
            false => self.closed_answers &= !answers,
        }
    }

    pub fn hear(&mut self, line_plays: bool) {
        self.speech.line_plays = line_plays;
    }

    /// `0x37` to `0x3A`: a message. In its first call the builder `0x800BBFFC` opens the
    /// window, which `open` makes (none while the game has no task left for it: the
    /// command is asked again). A command that does not wait (`0x38`, `0x3A`) goes on at
    /// once and leaves the window to the task's update; one that waits asks the waiter
    /// in every game frame from this one on, and when that has closed the window, goes
    /// on in the game frame that finds the window gone.
    pub fn say(
        &mut self,
        waits: bool,
        setting: u32,
        open: impl FnOnce() -> Option<Window>,
    ) -> Answer {
        if self.speech.step == Said::Nothing {
            let Some(window) = open() else {
                return Answer::Wait;
            };
            self.speech.window = Some(window);
            if !waits {
                self.speech.follows = true;
                self.next();
                return Answer::GoOn;
            }
            self.speech.step = Said::Waited;
        }
        self.wait_for_message(setting)
    }

    pub fn notice(&mut self, setting: u32, open: impl FnOnce() -> bool) -> Answer {
        if self.speech.step == Said::Nothing {
            if !open() {
                return Answer::Wait;
            }
            self.speech.notice = Some(Shown::default());
            self.speech.step = Said::Waited;
        }
        if self.speech.step == Said::Waited {
            if !self.notice_waiter(setting) {
                return Answer::Wait;
            }
            self.speech.step = Said::Leaving;
        }
        if self.speech.notice.is_some_and(|notice| !notice.is_gone()) {
            return Answer::Wait;
        }
        self.speech.notice = None;
        self.speech.step = Said::Nothing;
        self.next();
        Answer::GoOn
    }

    /// The waiter `0x800BC47C` with a notice in the window's place: it lets a notice
    /// that stands take a confirm, under the rule of a page that stands whole, and
    /// closes it when the confirm was taken (the opener has written "at the end" into
    /// the task once and for all). The things the task is busy with are told of the
    /// confirm as for a message.
    fn notice_waiter(&mut self, setting: u32) -> bool {
        let speech = &mut self.speech;
        let Some(notice) = &mut speech.notice else {
            return true;
        };
        if speech.allowed {
            if !notice.taken() {
                return false;
            }
            speech.allowed = false;
            speech.tells |= self.busy;
            notice.close();
            return true;
        }
        if notice.stands()
            && (setting & WAITS_FOR_NO_LINE != 0 || !speech.line_plays)
            && (setting & 0x10 != 0 || self.busy == 0)
        {
            notice.allow();
            speech.allowed = true;
        }
        false
    }

    /// `0x3F`: waits for the message of a command that did not wait. The task's update no
    /// longer asks the waiter; the command does, as one that waits.
    pub fn await_message(&mut self, setting: u32) -> Answer {
        if self.speech.step == Said::Nothing {
            self.speech.step = Said::Waited;
            self.speech.follows = false;
        }
        self.wait_for_message(setting)
    }

    /// Steps 3 to 5 of the message handler.
    fn wait_for_message(&mut self, setting: u32) -> Answer {
        if self.speech.step == Said::Waited {
            if !self.waiter(setting) {
                return Answer::Wait;
            }
            self.speech.step = Said::Leaving;
        }
        if (self.speech.window.as_ref()).is_some_and(|window| !window.is_gone()) {
            return Answer::Wait;
        }
        self.speech.window = None;
        self.speech.step = Said::Nothing;
        self.next();
        Answer::GoOn
    }

    /// The waiter `0x800BC47C`: true once the message is through. When a page or the
    /// answers stand whole it lets the window take a confirm; a taken confirm at the
    /// window's end closes the window. With bit 3 of the setting word clear the window
    /// is only let take a confirm while no line of the voices plays ([`Task::hear`]),
    /// and with bit 4 clear only while the task waits for no order. A taken confirm
    /// is first of all told to the things the task is busy with (`0x800A6744`), which
    /// is left to [`Task::tell_confirm`] right behind the waiter. Not here: the look
    /// through the cue table that starts the line of a page, which whoever runs the
    /// room does. The console's waiter is never asked
    /// without a window; here that is a message that is through.
    fn waiter(&mut self, setting: u32) -> bool {
        let speech = &mut self.speech;
        if speech.confirmed {
            speech.confirmed = false;
            return true;
        }
        let Some(window) = &mut speech.window else {
            return true;
        };
        if speech.allowed {
            let Some(taken) = window.taken() else {
                return false;
            };
            speech.allowed = false;
            speech.tells |= self.busy;
            if !taken.at_end {
                return false;
            }
            if speech.follows {
                (speech.follows, speech.confirmed) = (false, true);
            }
            // The picked answer is the task's choice (+236), for the records behind
            // the question.
            if taken.picked {
                speech.answer = Some(taken.answer);
                self.choice = i32::from(taken.answer);
            }
            window.close();
            return true;
        }
        if window.stands()
            && (setting & WAITS_FOR_NO_LINE != 0 || !speech.line_plays)
            && (setting & 0x10 != 0 || self.busy == 0)
        {
            window.allow();
            speech.allowed = true;
        }
        false
    }

    /// Tells the things the waiter noted that a confirm was taken (`0x800A6744`: the
    /// things 0 to 4 of the task's mask of busy things). To be called behind
    /// everything that asks the waiter: [`Task::say`], [`Task::await_message`] and
    /// [`Task::follow_message`], which [`Task::pass`] calls and tells for itself.
    pub fn tell_confirm(&mut self, engine: &mut impl Engine) {
        let things = std::mem::take(&mut self.speech.tells);
        (0..THINGS)
            .filter(|thing| things & 1 << thing != 0)
            .for_each(|thing| engine.confirmed(thing));
    }

    /// What the task's update `0x800A5FF0` does behind the commands of a game frame: it
    /// asks the waiter for a message that did not make the script wait.
    pub fn follow_message(&mut self, setting: u32) {
        if self.speech.follows {
            self.waiter(setting);
        }
    }

    /// `0x42`: waits until the page on screen has reached a cue (`0x80034B38`, which
    /// goes on answering yes for a cue that was reached, until the next page). The
    /// console asks the window of the task whatever has become of it; without one this
    /// goes on.
    pub fn await_cue(&mut self, cue: usize) -> Answer {
        if (self.speech.window.as_ref()).is_some_and(|window| !window.cue(cue)) {
            return Answer::Wait;
        }
        self.next();
        Answer::GoOn
    }

    /// The order routine `0x800BBB50`: an order for the things of a mask, from thing 0
    /// up. Each thing is given the order when no earlier order keeps it busy and it is
    /// free, and is told to go on when it is ready; the routine waits at the first thing
    /// that is not that far, with the things before it done. With `told` the thing is
    /// busy for the script from the giving on, until whoever runs the thing reports the
    /// move over ([`Task::told`]). With `advance` the task moves on when every thing has
    /// gone on.
    pub fn order(
        &mut self,
        things: &mut impl Things,
        mask: u32,
        order: &Order,
        told: bool,
        advance: bool,
    ) -> Answer {
        if !self.orders {
            self.orders = true;
            self.ordering = mask;
        }
        for thing in 0..THINGS {
            let bit = 1 << thing;
            if self.ordering & bit == 0 {
                continue;
            }
            if !self.given {
                if self.busy & bit != 0 || !things.free(thing) {
                    return Answer::Wait;
                }
                things.give(thing, order, told);
                if told {
                    self.busy |= bit;
                }
                self.given = true;
            }
            if !things.ready(thing) {
                return Answer::Wait;
            }
            if things.go_on(thing, order) {
                self.told(thing);
            }
            match order.kind {
                KIND_NOTED => self.settles |= bit,
                KIND_UNNOTED => self.settles &= !bit,
                _ => {}
            }
            self.given = false;
            self.ordering &= !bit;
        }
        self.orders = false;
        if advance {
            self.next();
        }
        Answer::GoOn
    }

    /// `0x45`: an order that the handler itself waits for. The order is given to the
    /// things as by [`Task::order`], always with the wish to be told; then the command
    /// waits until none of them is busy.
    pub fn order_and_wait(&mut self, things: &mut impl Things, mask: u32, order: &Order) -> Answer {
        if !self.moved {
            if self.order(things, mask, order, true, false) == Answer::Wait {
                return Answer::Wait;
            }
            self.moved = true;
        }
        if self.busy & mask != 0 {
            return Answer::Wait;
        }
        self.moved = false;
        self.next();
        Answer::GoOn
    }

    /// The move of a thing is over (`0x800A5B5C`, called for every script task): the
    /// thing is no longer busy for the script.
    pub fn told(&mut self, thing: usize) {
        self.busy &= !(1 << thing);
    }

    /// The script ran its end command to the end: the task is over.
    pub fn ended(&self) -> bool {
        self.ended
    }

    /// Moves on to the command behind the current one.
    pub fn next(&mut self) {
        self.current += COMMAND_SIZE;
    }

    /// One game frame of the task: handlers are called until one waits. Then a polling
    /// block goes back to its mark for the next game frame.
    pub fn pass(&mut self, script: &impl Commands, engine: &mut impl Engine) -> Result<(), Fault> {
        if self.ended {
            return Ok(());
        }
        let mut run = 0;
        loop {
            let at = self.current;
            let command = script.command_at(at).ok_or(Fault::NoCommand(at))?;
            if self.handle(script, engine, command)? == Answer::Wait {
                break;
            }
            run += 1;
            if run >= PASS_LIMIT {
                return Err(Fault::NeverWaits(at));
            }
        }
        self.follow_message(engine.setting());
        self.tell_confirm(engine);
        if self.polls {
            self.polls = false;
            self.current = self.mark;
        }
        Ok(())
    }

    fn handle(
        &mut self,
        script: &impl Commands,
        engine: &mut impl Engine,
        command: Command,
    ) -> Result<Answer, Fault> {
        let at = command.address;
        let (high, low) = (command.b >> 16, command.b & 0xFFFF);
        match command.code {
            COMMAND_CONFIRM => {
                if !engine.confirm() {
                    return Ok(Answer::Wait);
                }
                self.next();
            }
            COMMAND_END => {
                // The end runs the close; the task is over once that is through.
                if self.close(engine) == Answer::GoOn {
                    self.ended = true;
                }
                return Ok(Answer::Wait);
            }
            COMMAND_CLOSE => return Ok(self.close(engine)),
            COMMAND_OPEN => {
                let selector = *engine.selector();
                let free = selector < 0 || selector == self.number;
                if !free || !engine.may_open() {
                    return Ok(Answer::Wait);
                }
                self.in_section = true;
                engine.opened();
                *engine.selector() = self.number;
                self.next();
            }
            COMMAND_POLL => {
                if command.b != 0 {
                    // The mark is the command itself: a game frame that starts there
                    // sets the block up again.
                    self.mark = at;
                    self.polls = true;
                } else {
                    self.polls = false;
                }
                self.next();
            }
            COMMAND_TAKE => {
                for thing in 0..THINGS {
                    if command.b & (1 << thing) == 0 {
                        continue;
                    }
                    // What was taken before an actor that is not free stays taken.
                    if !engine.take(thing) {
                        return Ok(Answer::Wait);
                    }
                    self.held |= 1 << thing;
                }
                self.next();
            }
            COMMAND_GIVE_BACK => {
                self.give_back(engine, command.b);
                self.next();
            }
            COMMAND_GOTO => {
                // The label number is in the record of the chosen outcome.
                let chosen = self.chosen(script, at)?;
                let mut label = self.segment;
                loop {
                    let found = script.command_at(label).ok_or(Fault::NoCommand(label))?;
                    if found.code == COMMAND_END {
                        return Ok(Answer::Wait);
                    }
                    if found.code == COMMAND_LABEL && found.b == chosen.b {
                        break;
                    }
                    label += COMMAND_SIZE;
                }
                self.current = label;
                self.choice = 0;
            }
            COMMAND_JUMP => {
                let target = self.chosen(script, at)?.c;
                self.choice = 0;
                self.current = target;
                self.segment = target;
                self.mark = target;
            }
            COMMAND_WAIT => {
                // The handler is asked `b` times in vain.
                self.waited += 1;
                if self.waited <= command.b {
                    return Ok(Answer::Wait);
                }
                self.waited = 0;
                self.next();
            }
            COMMAND_WAIT_FOR => {
                if self.busy & command.b != 0 {
                    return Ok(Answer::Wait);
                }
                self.next();
            }
            COMMAND_SET => {
                if command.a == 0 {
                    *self.local(high, at)? = low as i32;
                } else {
                    engine.set_story_variable(high, low as i32);
                }
                self.next();
            }
            COMMAND_COUNT_LOCAL => {
                let (number, restart) = ((command.a as u32) >> 16, command.a & 0xFFFF);
                let limit = high as i32;
                if number == RANDOM {
                    if limit <= restart {
                        return Err(Fault::NoRange(at));
                    }
                    let drawn = loop {
                        let drawn = engine.random() % limit;
                        if drawn >= restart {
                            break drawn;
                        }
                    };
                    self.choice = drawn;
                    // The low half of `b` names a local variable that gets the number too.
                    if (low as usize) < LOCALS {
                        self.locals[low as usize] = drawn;
                    }
                } else {
                    let local = self.local(number, at)?;
                    let value = *local;
                    *local = match (value + 1 >= limit, low != 0) {
                        (false, _) => value + 1,
                        (true, true) => restart,
                        (true, false) => limit - 1,
                    };
                    self.choice = value;
                }
                self.next();
            }
            COMMAND_COUNT_STORY => {
                let (number, restart) = ((command.a as u32) >> 16, command.a & 0xFFFF);
                let limit = high as i32;
                let value = engine.story_variable(number);
                self.choice = value;
                engine.set_story_variable(number, value + 1);
                if engine.story_variable(number) >= limit {
                    let back = if low != 0 { restart } else { limit - 1 };
                    engine.set_story_variable(number, back);
                }
                self.next();
            }
            // `0x00`, `0x02` and the label `0x09` do nothing; `0x40` and `0x54`, which do
            // nothing either, go to the engine like every other command.
            0x00 | 0x02 | COMMAND_LABEL => self.next(),
            _ => return Ok(engine.run(self, command)),
        }
        Ok(Answer::GoOn)
    }

    /// The record of the chosen outcome behind a `0x0A` or `0x0B` at `at`.
    fn chosen(&self, script: &impl Commands, at: u32) -> Result<Command, Fault> {
        let chosen = at.wrapping_add((self.choice as u32).wrapping_mul(COMMAND_SIZE));
        script.command_at(chosen).ok_or(Fault::NoCommand(chosen))
    }

    fn local(&mut self, number: u32, at: u32) -> Result<&mut i32, Fault> {
        self.locals
            .get_mut(number as usize)
            .ok_or(Fault::NoVariable(at))
    }

    fn give_back(&mut self, engine: &mut impl Engine, things: u32) {
        for thing in 0..THINGS {
            if self.held & things & (1 << thing) != 0 {
                engine.give_back(thing);
                self.held &= !(1 << thing);
            }
        }
    }

    /// `0x04` in its three steps: the unread routine for a task with a mask at +28, the
    /// wait for the things that are busy, and then everything the script holds goes back,
    /// the section ends and the selector is free again.
    fn close(&mut self, engine: &mut impl Engine) -> Answer {
        if self.closing == 0 {
            let things = self.settles & self.held;
            if things != 0 && !engine.settle(things) {
                return Answer::Wait;
            }
            self.closing = 1;
        }
        if self.closing == 1 {
            if self.busy != 0 {
                return Answer::Wait;
            }
            self.closing = 2;
        }
        self.give_back(engine, u32::MAX);
        if self.in_section {
            self.in_section = false;
            engine.closed();
        }
        if *engine.selector() == self.number {
            *engine.selector() = -1;
        }
        self.closing = 0;
        self.next();
        Answer::GoOn
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scene {
    /// `0x11`: one more object of the room starts, from the object record at this address.
    StartObject { record: u32 },
    /// `0x13`: a thing is put on the place and angle at an address (three words and an
    /// angle), or on what the task noted for it.
    Put { thing: i32, place: Option<u32> },
    /// `0x1A`: order kind 6, things are shown or hidden.
    Show { things: u32, shown: bool },
    /// `0x1B`: order kind 7, things walk over the ground or not.
    Walk { things: u32, on: bool },
    /// `0x1C`: order kind 8, things are carried by their animation or not.
    Carry { things: u32, on: bool },
    /// `0x2E`: the object made from a record is told a state. With `by_data` the object
    /// is found by the word at +16 of its task instead of its record.
    Tell {
        record: u32,
        by_data: bool,
        state: u32,
    },
    /// `0x2F`: waits until that object has reached a state.
    AwaitObject {
        record: u32,
        by_data: bool,
        state: u32,
    },
    /// `0x30`: the object that started the script is told a state.
    TellOwner { state: u32 },
    /// `0x31`: waits until the object that started the script has reached a state.
    AwaitOwner { state: u32 },
    /// `0x14` and `0x16`: order kinds 0 and 1, a thing goes to the place at an address of
    /// the package, `runs` with `0x16`; `action` is -1 for the thing's own.
    GoTo {
        thing: i32,
        action: i32,
        place: u32,
        runs: bool,
    },
    /// `0x18`: order kind 2, a thing turns to the place at an address of the package.
    TurnTo { thing: i32, action: i32, place: u32 },
    /// `0x19`: order kind 2, a thing turns to where the thing `other` stands; when both
    /// are the same thing, to the place the task has noted of it.
    TurnToThing { thing: i32, other: i32, action: i32 },
    /// `0x37` to `0x3A`: a thing speaks a group of the room's text; `side` is the second
    /// argument of the routine that picks the picture's side. `0x38` and `0x3A` do not
    /// wait; `0x39` and `0x3A` are a question, with the answers of the group.
    Say {
        thing: i32,
        side: u32,
        group: u32,
        waits: bool,
        question: bool,
    },
    SwitchAnswers { off: bool, answers: u32 },
    WindowPlace { place: i32 },
    /// `0x3F`: waits for the message of a command that did not wait.
    AwaitMessage,
    /// `0x42`: waits for a cue of the message on screen.
    AwaitCue { cue: u32 },
    /// `0x45`: order kind 4, a move of the move table for things; its number is the word
    /// `slot` of the task's array at +36. Waits until the things are through.
    Move { things: u32, slot: i32 },
    /// `0x46`: order kind 5, a thing gets the command list at an address.
    List { thing: i32, list: u32 },
    PauseAtEnd { thing: i32, pause: bool },
    /// `0x4B`: the task's argument pointer is set to an address; without one, to what
    /// `0x800BBFB4` answers.
    Argument { address: Option<u32> },
    /// `0x4C`: the three arguments are copied to the task's argument record, which the
    /// pointer then names.
    Arguments { a: i32, b: u32, c: u32 },
    /// `0x4D`: a native routine of the package is called with the argument pointer:
    /// always, or only while the task selector is free or names this task.
    Native { routine: u32, always: bool },
    /// `0x4E`: the same call, and what the routine answers becomes the task's choice.
    NativeChoice { routine: u32, always: bool },
    /// `0x48`: order kind 9, a thing plays an action.
    Act { thing: i32, action: u32, bits: u32 },
    /// `0x49`: order kind 10, a thing plays two actions: `action` (the high half of the
    /// argument) is in the order where `0x48` has its one, `other` (the low half) behind.
    ActTwo {
        thing: i32,
        action: u32,
        other: u32,
        bits: u32,
    },
    /// `0x4A`: order kind 11, a thing plays two actions as with `0x49` while it goes
    /// along a path of the room (the kind 4 resource, by its number). The thing is the
    /// high half of the first argument and the bits its low half.
    Path {
        thing: i32,
        action: u32,
        other: u32,
        bits: u32,
        path: u32,
    },
    /// `0x55`: a view is read ahead.
    ReadView { view: u32 },
    /// `0x56`: a view that was read ahead is shown.
    ShowView { view: u32 },
    /// `0x61`: a view is wanted (the low half of the argument) and the script waits
    /// until it is the loaded one.
    AwaitView { view: u32 },
    AwaitTalk { region: Option<u32>, things: u32 },
    /// `0x64`: the script waits until a thing stands in a trigger region of the room.
    AwaitPlace { thing: i32, region: u32 },
    GoToRoom { room: u32 },
    StoreRoom { room: i32 },
    /// `0x58`: the place map.
    PlaceMap,
    Screen { kind: u32, argument: u32 },
    /// `0x5A`: the camera glides to the point at an address (raised by 350), over a
    /// number of game frames.
    GlideTo { point: u32, frames: u32 },
    /// `0x5B`: the camera glides to a thing.
    GlideToThing { thing: i32, frames: u32 },
    /// `0x5C`: the camera glides back to what it follows.
    GlideBack { frames: u32 },
    /// `0x5D`: waits until the glide is all the way there, or all the way back.
    AwaitGlide { there: bool },
    /// `0x5E`: the draw area.
    DrawArea(Area),
    /// `0x5F`: the zoom is a value at once (a count below 2) or runs there over a count
    /// of game frames.
    Zoom { value: i32, frames: u32 },
    /// `0x60`: the camera follows the point at an address, or a thing.
    Follow(Followed),
    /// `0x67`: one of the engine's small calls.
    Call(Call),
    /// `0x68`: a pattern of the routine `0x800365A4` starts on record 0, or stops.
    Pattern(Option<u8>),
}

/// What `0x5E` makes the draw area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Area {
    /// The whole screen, 320 by 240.
    Whole,
    /// 320 by 160 from line 32: the picture between two black bars.
    Bars,
    /// Four 16-bit values at an address.
    At(u32),
    /// Any other argument leaves the area as it is.
    Kept,
}

/// What `0x60` gives the camera to follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Followed {
    Point(u32),
    Thing(i32),
}

/// The calls of `0x67` by its first argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Call {
    /// 0: the picture at full brightness at once.
    Bright,
    /// 1: the picture black at once.
    Black,
    /// 2: black, then a fade in over game frames.
    FadeIn { frames: u32 },
    /// 3: a fade to black over game frames.
    FadeOut { frames: u32 },
    /// 4: the white over the picture goes over game frames.
    WhiteGoes { frames: u32 },
    /// 5: a white is added to the picture over game frames, and stays.
    White { frames: u32 },
    /// 6: waits until the fade is at rest.
    AwaitFade,
    /// 10 and 11: the painted background of the room is drawn again, or left out.
    Background { drawn: bool },
    /// 12: the music.
    Music(Music),
    /// 17: hold reason 64 on or off.
    Hold { on: u32 },
    /// 18: the picture of the frame before is laid into every frame, or no longer.
    Before { on: bool },
    /// The calls that were not read: the first argument and the two others.
    Other { number: i32, b: u32, c: u32 },
}

/// The music calls of `0x67` with 12, by the second argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Music {
    /// `0xFF00`: the music record's running sequence is faded (`0x80062740`) and the
    /// record has none from then on.
    End,
    /// `0xFF01`: it is stopped (`0x80066404`) and the record has none from then on.
    Stop,
    /// `0xFF04`: the music of the third argument is asked for and started.
    Start(u32),
    /// Any number that is not one of the five codes: that music is asked for.
    Ask(u32),
    Other { code: u32, c: u32 },
}

impl Scene {
    /// The order a command hands to the order routine (`0x1A` to `0x1C`, `0x46`, `0x48`
    /// to `0x4A`): the mask of things, the order, and whether the script is told when the
    /// move is over, which the list command and the two action commands ask for. `0x45` is not among them: it brings
    /// its own wait ([`Task::order_and_wait`]).
    pub fn order(&self) -> Option<(u32, Order, bool)> {
        let one = |thing: i32| 1u32.wrapping_shl(thing as u32);
        self.order_with(one)
    }

    /// The order of a command that sends a thing to a place or turns it there (`0x14`,
    /// `0x16`, `0x18`, `0x19`; the handlers `0x800B9578`, `0x800B978C` and `0x800B97FC`
    /// of file 53), with the place the handler has read for it: the same triple as
    /// [`Scene::order`]. The script is told when the move is over.
    pub fn order_to(&self, place: [i32; 3]) -> Option<(u32, Order, bool)> {
        let (thing, kind, action) = match *self {
            Self::GoTo {
                thing,
                action,
                runs,
                ..
            } => (thing, u32::from(runs), action),
            Self::TurnTo { thing, action, .. } | Self::TurnToThing { thing, action, .. } => {
                (thing, 2, action)
            }
            _ => return None,
        };
        let order = Order {
            kind,
            value: action as u32,
            place,
            ..Order::default()
        };
        Some((1u32.wrapping_shl(thing as u32), order, true))
    }

    fn order_with(&self, one: impl Fn(i32) -> u32) -> Option<(u32, Order, bool)> {
        let switch = |kind, things, on: bool| {
            let order = Order {
                kind,
                value: u32::from(on),
                ..Order::default()
            };
            (things, order, false)
        };
        Some(match *self {
            Self::Show { things, shown } => switch(6, things, shown),
            Self::Walk { things, on } => switch(7, things, on),
            Self::Carry { things, on } => switch(8, things, on),
            Self::List { thing, list } => {
                let order = Order {
                    kind: 5,
                    list,
                    ..Order::default()
                };
                (one(thing), order, true)
            }
            Self::Act {
                thing,
                action,
                bits,
            } => {
                let order = Order {
                    kind: 9,
                    value: action,
                    bits,
                    ..Order::default()
                };
                (one(thing), order, true)
            }
            Self::ActTwo {
                thing,
                action,
                other,
                bits,
            } => {
                let order = Order {
                    kind: 10,
                    value: action,
                    second: other,
                    bits,
                    ..Order::default()
                };
                (one(thing), order, true)
            }
            Self::Path {
                thing,
                action,
                other,
                bits,
                path,
            } => {
                let order = Order {
                    kind: 11,
                    value: action,
                    second: other,
                    bits,
                    list: path,
                    ..Order::default()
                };
                (one(thing), order, true)
            }
            _ => return None,
        })
    }

    /// The typed form of a command, `None` for the commands that are not read as a
    /// scene's and for the control flow.
    pub fn of(command: &Command) -> Option<Self> {
        let Command { a, b, c, .. } = *command;
        let (high, low) = (b >> 16, b & 0xFFFF);
        Some(match command.code {
            0x11 => Self::StartObject { record: c },
            0x13 => Self::Put {
                thing: a,
                place: (c != 0).then_some(c),
            },
            0x14 | 0x16 => Self::GoTo {
                thing: a,
                action: b as i32,
                place: c,
                runs: command.code == 0x16,
            },
            0x18 => Self::TurnTo {
                thing: a,
                action: b as i32,
                place: c,
            },
            0x19 => Self::TurnToThing {
                thing: a,
                other: b as i32,
                action: c as i32,
            },
            0x1A => Self::Show {
                things: a as u32,
                shown: b != 0,
            },
            0x1B => Self::Walk {
                things: a as u32,
                on: b != 0,
            },
            0x1C => Self::Carry {
                things: a as u32,
                on: b != 0,
            },
            0x2E => Self::Tell {
                record: c,
                by_data: a != 0,
                state: b,
            },
            0x2F => Self::AwaitObject {
                record: c,
                by_data: a != 0,
                state: b,
            },
            0x30 => Self::TellOwner { state: b },
            0x31 => Self::AwaitOwner { state: b },
            0x37..=0x3A => Self::Say {
                thing: a,
                side: b,
                group: c,
                waits: command.code & 1 != 0,
                question: command.code >= 0x39,
            },
            0x35 => Self::WindowPlace { place: b as i32 },
            0x43 => Self::SwitchAnswers {
                off: a == 1,
                answers: b,
            },
            0x3F => Self::AwaitMessage,
            0x42 => Self::AwaitCue { cue: b },
            0x45 => Self::Move { things: b, slot: a },
            0x46 => Self::List { thing: a, list: c },
            0x47 => Self::PauseAtEnd {
                thing: a,
                pause: b != 0,
            },
            0x4B => Self::Argument {
                address: (c != 0).then_some(c),
            },
            0x4C => Self::Arguments { a, b, c },
            0x4D => Self::Native {
                routine: c,
                always: b == 1,
            },
            0x4E => Self::NativeChoice {
                routine: c,
                always: b == 1,
            },
            0x48 => Self::Act {
                thing: a,
                action: b,
                bits: c,
            },
            0x49 => Self::ActTwo {
                thing: a,
                action: high,
                other: low,
                bits: c,
            },
            0x4A => Self::Path {
                thing: ((a as u32) >> 16) as i32,
                action: high,
                other: low,
                bits: a as u32 & 0xFFFF,
                path: c,
            },
            0x53 => Self::GoToRoom { room: b },
            0x55 => Self::ReadView { view: b },
            0x56 => Self::ShowView { view: b },
            0x57 => Self::StoreRoom { room: b as i32 },
            0x58 => Self::PlaceMap,
            0x59 => Self::Screen {
                kind: high,
                argument: low,
            },
            0x5A => Self::GlideTo {
                point: c,
                frames: b,
            },
            0x5B => Self::GlideToThing {
                thing: a,
                frames: b,
            },
            0x5C => Self::GlideBack { frames: b },
            // Any other argument is a wait without an end.
            0x5D if b <= 1 => Self::AwaitGlide { there: b == 1 },
            0x5E => Self::DrawArea(match (c, b) {
                (0, 0) => Area::Whole,
                (0, 1) => Area::Bars,
                (0, _) => Area::Kept,
                (at, _) => Area::At(at),
            }),
            0x5F => Self::Zoom {
                value: a,
                frames: b,
            },
            0x60 => Self::Follow(match c {
                0 => Followed::Thing(a),
                at => Followed::Point(at),
            }),
            0x61 => Self::AwaitView { view: low },
            0x62 => Self::AwaitTalk {
                region: u32::try_from(a).ok(),
                things: b,
            },
            0x64 => Self::AwaitPlace {
                thing: a,
                region: b,
            },
            0x67 => Self::Call(match a {
                0 => Call::Bright,
                1 => Call::Black,
                // The fades read the low half of the argument.
                2 => Call::FadeIn { frames: low },
                3 => Call::FadeOut { frames: low },
                4 => Call::WhiteGoes { frames: low },
                5 => Call::White { frames: low },
                6 => Call::AwaitFade,
                10 => Call::Background { drawn: true },
                11 => Call::Background { drawn: false },
                12 => Call::Music(match b {
                    0xFF00 => Music::End,
                    0xFF01 => Music::Stop,
                    0xFF02 | 0xFF03 => Music::Other { code: b, c },
                    0xFF04 => Music::Start(c),
                    number => Music::Ask(number),
                }),
                17 => Call::Hold { on: b },
                18 => Call::Before { on: b != 0 },
                number => Call::Other { number, b, c },
            }),
            0x68 => Self::Pattern((a == 1).then_some(b as u8)),
            _ => return None,
        })
    }
}

/// Bytes of a command.
const COMMAND_SIZE: u32 = 16;

#[cfg(test)]
mod tests {
    use super::*;

    /// A script made of command words at address 0x1000.
    struct Words(Vec<[u32; 4]>);

    const BASE: u32 = 0x1000;

    impl Commands for Words {
        fn command_at(&self, address: u32) -> Option<Command> {
            let index = address.checked_sub(BASE)? / COMMAND_SIZE;
            let [code, a, b, c] = *self.0.get(index as usize)?;
            Some(Command {
                address,
                code,
                a: a as i32,
                b,
                c,
            })
        }
    }

    fn at(index: u32) -> u32 {
        BASE + index * COMMAND_SIZE
    }

    /// Things that are free and ready as told, and note what they are given.
    struct Room {
        free: [bool; THINGS],
        ready: [bool; THINGS],
        ends_at_go_on: bool,
        log: Vec<String>,
    }

    impl Things for Room {
        fn free(&mut self, thing: usize) -> bool {
            self.free[thing]
        }
        fn give(&mut self, thing: usize, order: &Order, told: bool) {
            self.log
                .push(format!("give {thing} kind {} {told}", order.kind));
        }
        fn ready(&mut self, thing: usize) -> bool {
            self.ready[thing]
        }
        fn go_on(&mut self, thing: usize, _order: &Order) -> bool {
            self.log.push(format!("go on {thing}"));
            self.ends_at_go_on
        }
    }

    #[test]
    fn an_order_reaches_each_thing_when_it_is_free_and_ready() {
        let mut room = Room {
            free: [true, true, false, true, true],
            ready: [true; THINGS],
            ends_at_go_on: false,
            log: Vec::new(),
        };
        let mut task = Task::new(1, BASE);
        let order = Order {
            kind: 9,
            value: 17,
            ..Order::default()
        };
        // Things 0 and 2: the first is given the order and goes on, the second is not
        // free, and the routine waits there.
        let ask = |task: &mut Task, room: &mut Room| task.order(room, 0b101, &order, true, true);
        assert_eq!(ask(&mut task, &mut room), Answer::Wait);
        assert_eq!(room.log, ["give 0 kind 9 true", "go on 0"]);
        assert_eq!((task.busy, task.current), (0b001, BASE));
        // Free but not ready: given once, and waited for.
        room.free[2] = true;
        room.ready[2] = false;
        assert_eq!(ask(&mut task, &mut room), Answer::Wait);
        assert_eq!(ask(&mut task, &mut room), Answer::Wait);
        assert_eq!(room.log.len(), 3);
        assert_eq!(
            (room.log[2].as_str(), task.busy),
            ("give 2 kind 9 true", 0b101)
        );
        room.ready[2] = true;
        assert_eq!(ask(&mut task, &mut room), Answer::GoOn);
        assert_eq!((room.log.len(), task.current), (4, at(1)));
        assert_eq!(task.settles, 0);

        // A thing that is still busy with an order is not given the next one.
        room.log.clear();
        assert_eq!(ask(&mut task, &mut room), Answer::Wait);
        assert!(room.log.is_empty());
        task.told(0);
        assert_eq!(ask(&mut task, &mut room), Answer::Wait);
        assert_eq!(room.log, ["give 0 kind 9 true", "go on 0"]);
        task.told(2);
        assert_eq!(ask(&mut task, &mut room), Answer::GoOn);
        assert_eq!(task.busy, 0b101);

        // An order nobody is to be told of leaves no thing busy; one that ends at "go
        // on" is over inside the call.
        let mut task = Task::new(1, BASE);
        assert_eq!(
            task.order(&mut room, 0b10, &order, false, false),
            Answer::GoOn
        );
        assert_eq!((task.busy, task.current), (0, BASE));
        room.ends_at_go_on = true;
        assert_eq!(
            task.order(&mut room, 0b10, &order, true, true),
            Answer::GoOn
        );
        assert_eq!(task.busy, 0);

        // Kinds 3 and 4 note the thing in the mask the close reads, and take it out.
        let moved = |kind| Order {
            kind,
            ..Order::default()
        };
        task.order(&mut room, 0b110, &moved(3), false, false);
        assert_eq!(task.settles, 0b110);
        task.order(&mut room, 0b010, &moved(4), false, false);
        assert_eq!(task.settles, 0b100);
    }

    #[test]
    fn a_move_command_waits_for_its_things_itself() {
        let mut room = Room {
            free: [true; THINGS],
            ready: [true; THINGS],
            ends_at_go_on: false,
            log: Vec::new(),
        };
        let mut task = Task::new(1, BASE);
        let order = Order {
            kind: 4,
            ..Order::default()
        };
        assert_eq!(task.order_and_wait(&mut room, 0b11, &order), Answer::Wait);
        assert_eq!((room.log.len(), task.busy, task.current), (4, 0b11, BASE));
        task.told(1);
        assert_eq!(task.order_and_wait(&mut room, 0b11, &order), Answer::Wait);
        task.told(0);
        assert_eq!(task.order_and_wait(&mut room, 0b11, &order), Answer::GoOn);
        // Given once, and the task has moved on.
        assert_eq!((room.log.len(), task.current), (4, at(1)));
    }

    #[test]
    fn the_order_commands_have_their_orders() {
        let act = Scene::ActTwo {
            thing: 2,
            action: 6,
            other: 8,
            bits: 1,
        };
        let order = Order {
            kind: 10,
            value: 6,
            second: 8,
            bits: 1,
            ..Order::default()
        };
        assert_eq!(act.order(), Some((0b100, order, true)));
        // `0x4A` packs the thing and the bits into its first argument and names a path.
        let command = Command {
            address: 0,
            code: 0x4A,
            a: 0x0001_0025,
            b: 0x002C_0004,
            c: 1,
        };
        let walk = Scene::of(&command).expect("a path order");
        let order = Order {
            kind: 11,
            value: 0x2C,
            second: 4,
            bits: 0x25,
            list: 1,
            ..Order::default()
        };
        assert_eq!(walk.order(), Some((0b10, order, true)));
        let hide = Scene::Show {
            things: 4,
            shown: false,
        };
        let order = Order {
            kind: 6,
            ..Order::default()
        };
        assert_eq!(hide.order(), Some((4, order, false)));
        let carry = Scene::Carry {
            things: 7,
            on: true,
        };
        assert_eq!(carry.order().map(|order| order.1.kind), Some(8));
        assert_eq!(carry.order().map(|order| order.1.value), Some(1));
        assert_eq!(Scene::PlaceMap.order(), None);
        assert_eq!(Scene::GoToRoom { room: 0 }.order(), None);

        // `0x46`: one thing, kind 5, the address of the list, and the wish to be told.
        let list = 0x800B_3558;
        let command = Command {
            address: BASE,
            code: 0x46,
            a: 2,
            b: 0,
            c: list,
        };
        // `0x14`, `0x16`, `0x18`, `0x19`: one thing, kind 0, 1 or 2, the action where
        // the other kinds have their bits' neighbour, the place the handler read, and
        // the wish to be told. They are no order without their place.
        let to = |code, a, b, c| {
            let command = Command {
                address: BASE,
                code,
                a,
                b,
                c,
            };
            Scene::of(&command).expect("an order to a place")
        };
        let place = [10, -20, 30];
        let ordered = |kind, action: i32| Order {
            kind,
            value: action as u32,
            place,
            ..Order::default()
        };
        let goes = to(0x14, 0, u32::MAX, 0x800B_2F24);
        assert_eq!(goes.order(), None);
        assert_eq!(goes.order_to(place), Some((1, ordered(0, -1), true)));
        let runs = to(0x16, 3, 7, 0x800B_2F24);
        assert_eq!(runs.order_to(place), Some((0b1000, ordered(1, 7), true)));
        let turns = to(0x18, 1, u32::MAX, 0x800B_2F30);
        assert_eq!(turns.order_to(place), Some((0b10, ordered(2, -1), true)));
        let faces = to(0x19, 2, 0, 5);
        let typed = Scene::TurnToThing {
            thing: 2,
            other: 0,
            action: 5,
        };
        assert_eq!(faces, typed);
        assert_eq!(faces.order_to(place), Some((0b100, ordered(2, 5), true)));
        assert_eq!(Scene::PlaceMap.order_to(place), None);

        let listed = Scene::of(&command).expect("a scene command");
        assert_eq!(listed, Scene::List { thing: 2, list });
        let order = Order {
            kind: 5,
            list,
            ..Order::default()
        };
        assert_eq!(listed.order(), Some((0b100, order, true)));
    }

    /// An engine that lets everything go on and notes what it was asked.
    struct Notes {
        selector: i32,
        confirm: bool,
        free: [bool; THINGS],
        story: [i32; 16],
        random: Vec<i32>,
        ran: Vec<u32>,
        log: Vec<String>,
    }

    impl Notes {
        fn new() -> Self {
            Self {
                selector: -1,
                confirm: false,
                free: [true; THINGS],
                story: [0; 16],
                random: Vec::new(),
                ran: Vec::new(),
                log: Vec::new(),
            }
        }
    }

    impl Engine for Notes {
        fn selector(&mut self) -> &mut i32 {
            &mut self.selector
        }
        fn confirm(&mut self) -> bool {
            self.confirm
        }
        fn may_open(&mut self) -> bool {
            true
        }
        fn opened(&mut self) {
            self.log.push("opened".into());
        }
        fn closed(&mut self) {
            self.log.push("closed".into());
        }
        fn take(&mut self, thing: usize) -> bool {
            self.free[thing]
        }
        fn give_back(&mut self, thing: usize) {
            self.log.push(format!("back {thing}"));
        }
        fn settle(&mut self, _things: u32) -> bool {
            true
        }
        fn story_variable(&mut self, number: u32) -> i32 {
            self.story[number as usize]
        }
        fn set_story_variable(&mut self, number: u32, value: i32) {
            self.story[number as usize] = value;
        }
        fn random(&mut self) -> i32 {
            self.random.remove(0)
        }
        fn run(&mut self, task: &mut Task, command: Command) -> Answer {
            self.ran.push(command.code);
            task.next();
            Answer::GoOn
        }
    }

    fn passes(task: &mut Task, script: &Words, engine: &mut Notes, most: usize) -> usize {
        for pass in 1..=most {
            task.pass(script, engine).expect("a pass");
            if task.ended() {
                return pass;
            }
        }
        panic!("the script did not end in {most} passes");
    }

    #[test]
    fn a_wait_asks_in_vain_as_often_as_it_says() {
        let script = Words(vec![[0x0C, 0, 3, 0], [0x20, 0, 0, 0], [0x03, 0, 0, 0]]);
        let (mut task, mut engine) = (Task::new(1, BASE), Notes::new());
        for _ in 0..3 {
            task.pass(&script, &mut engine).unwrap();
            assert!(engine.ran.is_empty());
        }
        // The fourth pass goes on, runs the other command and ends.
        task.pass(&script, &mut engine).unwrap();
        assert_eq!(engine.ran, [0x20]);
        assert!(task.ended());
        // A wait of 0 does not wait.
        let script = Words(vec![[0x0C, 0, 0, 0], [0x03, 0, 0, 0]]);
        let mut task = Task::new(1, BASE);
        assert_eq!(passes(&mut task, &script, &mut engine, 1), 1);
    }

    #[test]
    fn a_goto_takes_the_label_of_the_chosen_record() {
        // Local variable 2 counts 0, 1, 0, ... and picks the label.
        let script = Words(vec![
            [0x09, 0, 7, 0],
            [0x0F, 2 << 16, (2 << 16) | 1, 0],
            [0x0A, 0, 5, 0],
            [0x0A, 0, 6, 0],
            [0x09, 0, 5, 0],
            [0x21, 0, 0, 0],
            [0x0C, 0, 1, 0],
            [0x0A, 0, 7, 0],
            [0x09, 0, 6, 0],
            [0x22, 0, 0, 0],
            [0x03, 0, 0, 0],
        ]);
        let (mut task, mut engine) = (Task::new(1, BASE), Notes::new());
        assert_eq!(passes(&mut task, &script, &mut engine, 5), 2);
        assert_eq!(engine.ran, [0x21, 0x22]);
        assert_eq!((task.locals[2], task.choice), (0, 0));
        // A label that is not in the segment holds the script where it is.
        let script = Words(vec![[0x0A, 0, 9, 0], [0x03, 0, 0, 0]]);
        let mut task = Task::new(1, BASE);
        task.pass(&script, &mut engine).unwrap();
        assert_eq!((task.current, task.ended()), (BASE, false));
    }

    #[test]
    fn a_counter_stays_at_its_limit_or_starts_again() {
        let script = Words(vec![[0x0F, 3 << 16, 3 << 16, 0], [0x0B, 0, 0, BASE]]);
        let mut task = Task::new(1, BASE);
        let mut seen = Vec::new();
        for _ in 0..5 {
            let command = script.command_at(BASE).unwrap();
            task.handle(&script, &mut Notes::new(), command).unwrap();
            seen.push(task.choice);
            task.choice = 0;
            task.current = BASE;
        }
        assert_eq!(seen, [0, 1, 2, 2, 2]);
        // A story variable with a restart value.
        let script = Words(vec![[0x10, (4 << 16) | 1, (3 << 16) | 1, 0]]);
        let (mut task, mut engine) = (Task::new(1, BASE), Notes::new());
        let mut seen = Vec::new();
        for _ in 0..5 {
            let command = script.command_at(BASE).unwrap();
            task.handle(&script, &mut engine, command).unwrap();
            seen.push(task.choice);
            task.current = BASE;
        }
        assert_eq!(seen, [0, 1, 2, 1, 2]);
    }

    #[test]
    fn a_random_choice_is_drawn_until_it_is_in_its_range() {
        let script = Words(vec![[0x0F, (9 << 16) | 2, (5 << 16) | 3, 0]]);
        let (mut task, mut engine) = (Task::new(1, BASE), Notes::new());
        engine.random = vec![10, 6, 13];
        let command = script.command_at(BASE).unwrap();
        task.handle(&script, &mut engine, command).unwrap();
        assert_eq!((task.choice, task.locals[3]), (3, 3));
        assert!(engine.random.is_empty());
        let none = Words(vec![[0x0F, (9 << 16) | 5, 5 << 16, 0]]);
        let command = none.command_at(BASE).unwrap();
        assert_eq!(
            task.handle(&none, &mut engine, command),
            Err(Fault::NoRange(BASE))
        );
    }

    #[test]
    fn a_jump_starts_another_segment() {
        let script = Words(vec![
            [0x0B, 0, 0, at(3)],
            [0x23, 0, 0, 0],
            [0x03, 0, 0, 0],
            [0x24, 0, 0, 0],
            [0x03, 0, 0, 0],
        ]);
        let (mut task, mut engine) = (Task::new(1, BASE), Notes::new());
        assert_eq!(passes(&mut task, &script, &mut engine, 1), 1);
        assert_eq!(engine.ran, [0x24]);
        assert_eq!((task.segment, task.mark), (at(3), at(3)));
    }

    #[test]
    fn a_polling_block_starts_again_every_game_frame() {
        // The block waits for the confirm button and runs a command every time it is asked.
        let script = Words(vec![
            [0x06, 0, 1, 0],
            [0x25, 0, 0, 0],
            [0x01, 0, 0, 0],
            [0x06, 0, 0, 0],
            [0x0C, 0, 1, 0],
            [0x03, 0, 0, 0],
        ]);
        let (mut task, mut engine) = (Task::new(1, BASE), Notes::new());
        for _ in 0..3 {
            task.pass(&script, &mut engine).unwrap();
        }
        assert_eq!(engine.ran, [0x25, 0x25, 0x25]);
        assert_eq!((task.current, task.mark), (BASE, BASE));
        engine.confirm = true;
        // Behind the block a wait no longer leads back to the mark.
        assert_eq!(passes(&mut task, &script, &mut engine, 3), 2);
        assert_eq!(engine.ran.len(), 4);
    }

    #[test]
    fn a_section_takes_the_selector_and_gives_everything_back() {
        let script = Words(vec![
            [0x05, 0, 0, 0],
            [0x07, 0, 0b101, 0],
            [0x0D, 0, 0b100, 0],
            [0x08, 0, 0b001, 0],
            [0x03, 0, 0, 0],
        ]);
        let (mut task, mut engine) = (Task::new(4, BASE), Notes::new());
        // Another task has the selector.
        engine.selector = 2;
        task.pass(&script, &mut engine).unwrap();
        assert_eq!(task.current, BASE);
        engine.selector = -1;
        // The actor's slot is not free: the player is taken, the script waits.
        engine.free[2] = false;
        task.pass(&script, &mut engine).unwrap();
        assert_eq!(
            (engine.selector, task.held, task.current),
            (4, 0b001, at(1))
        );
        engine.free[2] = true;
        // An order is busy with the actor.
        task.busy = 0b100;
        task.pass(&script, &mut engine).unwrap();
        assert_eq!((task.held, task.current), (0b101, at(2)));
        task.busy = 0;
        assert_eq!(passes(&mut task, &script, &mut engine, 1), 1);
        assert_eq!(engine.log, ["opened", "back 0", "back 2", "closed"]);
        assert_eq!(
            (engine.selector, task.held, task.in_section),
            (-1, 0, false)
        );
    }

    #[test]
    fn the_end_waits_for_the_busy_things() {
        let script = Words(vec![[0x07, 0, 0b010, 0], [0x03, 0, 0, 0]]);
        let (mut task, mut engine) = (Task::new(1, BASE), Notes::new());
        task.busy = 0b010;
        for _ in 0..3 {
            task.pass(&script, &mut engine).unwrap();
            assert!(!task.ended());
        }
        task.busy = 0;
        task.pass(&script, &mut engine).unwrap();
        assert!(task.ended());
        assert_eq!(engine.log, ["back 1"]);
    }

    #[test]
    fn variables_are_set_by_number() {
        let script = Words(vec![
            [0x0E, 0, (5 << 16) | 77, 0],
            [0x0E, 1, (6 << 16) | 9, 0],
            [0x0E, 0, 8 << 16, 0],
        ]);
        let (mut task, mut engine) = (Task::new(1, BASE), Notes::new());
        assert_eq!(
            task.pass(&script, &mut engine),
            Err(Fault::NoVariable(at(2)))
        );
        assert_eq!((task.locals[5], engine.story[6]), (77, 9));
    }

    #[test]
    fn scene_commands_are_typed_by_their_arguments() {
        let typed = |code, a: i32, b, c| {
            Scene::of(&Command {
                address: BASE,
                code,
                a,
                b,
                c,
            })
        };
        assert_eq!(
            typed(0x49, 2, (7 << 16) | 9, 1),
            Some(Scene::ActTwo {
                thing: 2,
                action: 7,
                other: 9,
                bits: 1
            })
        );
        assert_eq!(
            typed(0x1A, 0b100, 0, 0),
            Some(Scene::Show {
                things: 0b100,
                shown: false
            })
        );
        assert_eq!(
            typed(0x13, 0, 0, 0),
            Some(Scene::Put {
                thing: 0,
                place: None
            })
        );
        assert_eq!(typed(0x53, 0, 340, 0), Some(Scene::GoToRoom { room: 340 }));
        assert_eq!(typed(0x57, 0, 104, 0), Some(Scene::StoreRoom { room: 104 }));
        // The argument is the half word at +8 and the kind the one at +10.
        assert_eq!(
            typed(0x59, 0, (1 << 16) | 2, 0),
            Some(Scene::Screen {
                kind: 1,
                argument: 2
            })
        );
        assert_eq!(typed(0x5E, 0, 1, 0), Some(Scene::DrawArea(Area::Bars)));
        assert_eq!(
            typed(0x5E, 0, 1, 0x2000),
            Some(Scene::DrawArea(Area::At(0x2000)))
        );
        assert_eq!(
            typed(0x5D, 0, 1, 0),
            Some(Scene::AwaitGlide { there: true })
        );
        assert_eq!(typed(0x5D, 0, 2, 0), None);
        assert_eq!(
            typed(0x60, 3, 0, 0),
            Some(Scene::Follow(Followed::Thing(3)))
        );
        assert_eq!(
            typed(0x67, 2, (5 << 16) | 30, 0),
            Some(Scene::Call(Call::FadeIn { frames: 30 }))
        );
        assert_eq!(
            typed(0x67, 12, 0xFF04, 126),
            Some(Scene::Call(Call::Music(Music::Start(126))))
        );
        assert_eq!(
            typed(0x67, 12, 40, 0),
            Some(Scene::Call(Call::Music(Music::Ask(40))))
        );
        assert_eq!(
            [10, 11].map(|call| typed(0x67, call, 0, 0)),
            [true, false].map(|drawn| Some(Scene::Call(Call::Background { drawn })))
        );
        assert_eq!(
            [4, 5].map(|call| typed(0x67, call, (7 << 16) | 90, 0)),
            [
                Some(Scene::Call(Call::WhiteGoes { frames: 90 })),
                Some(Scene::Call(Call::White { frames: 90 }))
            ]
        );
        assert_eq!(
            [1, 0].map(|b| typed(0x67, 18, b, 0)),
            [true, false].map(|on| Some(Scene::Call(Call::Before { on })))
        );
        assert_eq!(typed(0x68, 1, 13, 0), Some(Scene::Pattern(Some(13))));
        assert_eq!(typed(0x68, 0, 13, 0), Some(Scene::Pattern(None)));
        assert_eq!(
            typed(0x38, 1, 0, 4),
            Some(Scene::Say {
                thing: 1,
                side: 0,
                group: 4,
                waits: false,
                question: false
            })
        );
        assert_eq!(
            typed(0x3A, 1, 0, 4),
            Some(Scene::Say {
                thing: 1,
                side: 0,
                group: 4,
                waits: false,
                question: true
            })
        );
        assert!(matches!(
            typed(0x39, 1, 0, 4),
            Some(Scene::Say {
                waits: true,
                question: true,
                ..
            })
        ));
        assert_eq!(
            typed(0x43, 1, 3, 0),
            Some(Scene::SwitchAnswers {
                off: true,
                answers: 3
            })
        );
        assert_eq!(
            typed(0x43, 0, 3, 0),
            Some(Scene::SwitchAnswers {
                off: false,
                answers: 3
            })
        );
        // The switch works on the task's mask of answers that are off.
        let mut task = Task::new(1, 0x800B_3000);
        task.switch_answers(true, 1);
        task.switch_answers(true, 2);
        assert_eq!(task.closed_answers, 3);
        task.switch_answers(false, 1);
        assert_eq!(task.closed_answers, 2);
        task.switch_answers(false, 3);
        assert_eq!(task.closed_answers, 0);
        assert_eq!(
            typed(0x47, 1, 1, 0),
            Some(Scene::PauseAtEnd {
                thing: 1,
                pause: true
            })
        );
        // `0x35` names a place for the task's windows by its second word.
        assert_eq!(typed(0x35, 7, 1, 9), Some(Scene::WindowPlace { place: 1 }));
        assert_eq!(task.window_place, None);
        task.place_windows(0);
        assert_eq!(task.window_place, Some(Where::Top));
        task.place_windows(2);
        assert_eq!(task.window_place, Some(Where::Bottom));
        task.place_windows(-1);
        assert_eq!(task.window_place, None);
        assert_eq!(
            typed(0x4D, 0, 0, 0x3000),
            Some(Scene::Native {
                routine: 0x3000,
                always: false
            })
        );
        assert_eq!(
            typed(0x4B, 0, 0, 0),
            Some(Scene::Argument { address: None })
        );
        assert_eq!(
            typed(0x4E, 0, 1, 0x3000),
            Some(Scene::NativeChoice {
                routine: 0x3000,
                always: true
            })
        );
        assert_eq!(
            typed(0x61, 0, 0x1_0001, 0),
            Some(Scene::AwaitView { view: 1 })
        );
        // The control flow and what was not read have no typed form.
        assert_eq!((typed(0x0C, 0, 30, 0), typed(0x63, 0, 1, 0)), (None, None));
        // The two waits for the player.
        assert_eq!(
            typed(0x62, -1, 2, 0),
            Some(Scene::AwaitTalk {
                region: None,
                things: 2
            })
        );
        assert_eq!(
            typed(0x62, 3, 4, 0),
            Some(Scene::AwaitTalk {
                region: Some(3),
                things: 4
            })
        );
        assert_eq!(
            typed(0x64, 0, 2, 0),
            Some(Scene::AwaitPlace {
                thing: 0,
                region: 2
            })
        );
    }

    #[test]
    fn a_script_without_an_end_is_a_fault() {
        let script = Words(vec![[0x00, 0, 0, 0]]);
        let (mut task, mut engine) = (Task::new(1, BASE), Notes::new());
        assert_eq!(
            task.pass(&script, &mut engine),
            Err(Fault::NoCommand(at(1)))
        );
        let script = Words(vec![[0x0B, 0, 0, BASE]]);
        let mut task = Task::new(1, BASE);
        assert_eq!(
            task.pass(&script, &mut engine),
            Err(Fault::NeverWaits(BASE))
        );
    }

    /// A window of one page with three letters, whose second letter is behind cue 0.
    fn window() -> Option<Window> {
        let asked = crate::window::Asked::said(false, SETTING_AT_START, 1, 0, 0, false);
        Some(Window::open(asked, 0))
    }

    /// The window's own game frame, with the confirm button down or not.
    fn step(task: &mut Task, pad: &mut crate::pad::Pad, held: u16, counter: u32) {
        pad.read(held, crate::pad::Repeat::of(2), counter);
        let mut read = |_page: usize, letters: u32| crate::window::Read {
            whole: letters >= 3,
            cue: (letters >= 2).then_some(0),
        };
        let window = task.speech.window.as_mut().expect("a window");
        window.step(pad, SETTING_AT_START, false, &mut read);
    }

    #[test]
    fn a_notice_holds_the_script_until_its_task_has_ended() {
        let mut task = Task::new(1, 0x800B_3000);
        let at = task.current;
        let setting = SETTING_AT_START;
        let step = |task: &mut Task, confirm: bool| {
            let notice = task.speech.notice.as_mut().expect("a notice");
            notice.step(confirm)
        };
        // No task for the notice: the command is asked again.
        assert_eq!(task.notice(setting, || false), Answer::Wait);
        assert!(task.speech.notice.is_none());
        // Opened; it has not stood yet, so the waiter allows nothing.
        assert_eq!(task.notice(setting, || true), Answer::Wait);
        assert!(!step(&mut task, true));
        // It stands: the waiter lets it take a confirm, which comes two game frames on.
        assert_eq!(task.notice(setting, || unreachable!()), Answer::Wait);
        assert!(!step(&mut task, false));
        assert_eq!(task.notice(setting, || unreachable!()), Answer::Wait);
        assert!(!step(&mut task, true));
        // The waiter finds the confirm taken and closes the notice, whose next step is
        // its last; the script goes on in the game frame after that.
        assert_eq!(task.notice(setting, || unreachable!()), Answer::Wait);
        assert_eq!(task.current, at);
        assert!(step(&mut task, false));
        assert_eq!(task.notice(setting, || unreachable!()), Answer::GoOn);
        assert!(task.speech.notice.is_none());
        assert_ne!(task.current, at);
    }

    #[test]
    fn a_message_that_does_not_wait_is_followed_by_the_update() {
        let setting = SETTING_AT_START;
        let mut task = Task::new(1, BASE);
        let mut pad = crate::pad::Pad::default();
        // `0x38` goes on at once; `0x42` waits for the cue, `0x3F` for the end.
        assert_eq!(task.say(false, setting, window), Answer::GoOn);
        assert_eq!(task.current, at(1));
        let mut frames = 0;
        while task.await_cue(0) == Answer::Wait {
            task.follow_message(setting);
            step(&mut task, &mut pad, 0, frames);
            frames += 1;
            assert!(frames < 100);
        }
        // Eight game frames of opening, one that types nothing yet, then a letter a game
        // frame: the cue with the second one.
        assert_eq!((frames, task.current), (11, at(2)));
        // The window stands and is let take a confirm, but none comes: the wait goes on.
        for _ in 0..20 {
            assert_eq!(task.await_message(setting), Answer::Wait);
            step(&mut task, &mut pad, 0, frames);
            frames += 1;
        }
        let stood = task.speech.window.as_ref().expect("a window");
        assert!(stood.stands() && stood.cue(0));
        // The confirm: the window takes it in its own game frame, the waiter closes the
        // window in the next, and the command goes on when the window has shrunk away.
        step(&mut task, &mut pad, crate::pad::CROSS, frames);
        let mut closing = 0;
        while task.await_message(setting) == Answer::Wait {
            step(&mut task, &mut pad, 0, frames);
            closing += 1;
            assert!(closing < 100);
        }
        assert_eq!(closing, crate::window::OPENING + 1);
        assert_eq!((task.current, &task.speech.window), (at(3), &None));
        // Without a window the cue's command goes on.
        assert_eq!(task.await_cue(3), Answer::GoOn);
    }

    #[test]
    fn a_message_that_waits_and_one_that_was_confirmed_before_its_wait() {
        let setting = SETTING_AT_START;
        let mut pad = crate::pad::Pad::default();
        // `0x37` holds the script from its first call to the game frame the window is
        // gone in; the confirm is held down every other game frame.
        let mut task = Task::new(1, BASE);
        let mut frames = 0;
        while task.say(true, setting, window) == Answer::Wait {
            let held = if frames % 2 == 0 {
                crate::pad::CROSS
            } else {
                0
            };
            step(&mut task, &mut pad, held, frames);
            frames += 1;
            assert!(frames < 100);
        }
        assert_eq!((task.current, &task.speech.window), (at(1), &None));

        // `0x38`, confirmed at its end while the script does something else: the `0x3F`
        // behind it only waits for the window to go.
        let mut task = Task::new(1, BASE);
        let mut pad = crate::pad::Pad::default();
        assert_eq!(task.say(false, setting, window), Answer::GoOn);
        for frame in 0..40 {
            task.follow_message(setting);
            let held = if frame % 2 == 0 { crate::pad::CROSS } else { 0 };
            step(&mut task, &mut pad, held, frame);
        }
        let kept = task.speech.window.as_ref().expect("the window is kept");
        assert!(kept.is_gone());
        assert_eq!(task.await_message(setting), Answer::GoOn);
        assert_eq!((task.current, &task.speech.window), (at(2), &None));

        // With bit 4 of the setting word clear a task that waits for an order does not
        // let the window take a confirm.
        let mut task = Task::new(1, BASE);
        let mut pad = crate::pad::Pad::default();
        task.busy = 0b10;
        assert_eq!(task.say(false, setting & !0x10, window), Answer::GoOn);
        for frame in 0..40 {
            task.follow_message(setting & !0x10);
            let held = if frame % 2 == 0 { crate::pad::CROSS } else { 0 };
            step(&mut task, &mut pad, held, frame);
        }
        assert!(task.speech.window.as_ref().is_some_and(Window::stands));
    }

    #[test]
    fn with_bit_3_of_the_setting_clear_a_confirm_waits_for_the_line_to_be_over() {
        // The confirm is held down every other game frame from the start.
        let run = |setting: u32, plays_until: u32| {
            let mut task = Task::new(1, BASE);
            let mut pad = crate::pad::Pad::default();
            let mut frames = 0;
            task.hear(true);
            while task.say(true, setting, window) == Answer::Wait {
                let held = if frames % 2 == 0 {
                    crate::pad::CROSS
                } else {
                    0
                };
                step(&mut task, &mut pad, held, frames);
                frames += 1;
                task.hear(frames < plays_until);
                assert!(frames < 200);
            }
            frames
        };
        // As a new game has it, the line holds nothing up.
        let cut = run(SETTING_AT_START, 60);
        assert_eq!(cut, run(SETTING_AT_START, 0));
        // With the bit clear the window is let take a confirm only once the line is
        // over, 60 game frames in: the same wait as without a line counts from there.
        let waited = run(SETTING_AT_START & !0x08, 60);
        assert_eq!(run(SETTING_AT_START & !0x08, 0), cut);
        assert!(waited > 60 && waited <= 60 + cut, "{waited} against {cut}");
    }

    #[test]
    fn a_taken_confirm_is_noted_for_the_things_the_task_is_busy_with() {
        let setting = SETTING_AT_START;
        let mut task = Task::new(1, BASE);
        let mut pad = crate::pad::Pad::default();
        task.busy = 0b110;
        assert_eq!(task.say(false, setting, window), Answer::GoOn);
        let (mut told, mut times) = (0, 0);
        for frame in 0..40 {
            task.follow_message(setting);
            let tells = std::mem::take(&mut task.speech.tells);
            (told, times) = (told | tells, times + u32::from(tells != 0));
            let held = if frame % 2 == 0 { crate::pad::CROSS } else { 0 };
            step(&mut task, &mut pad, held, frame);
        }
        // One page, so one confirm, told to both things.
        assert_eq!((told, times), (0b110, 1));
    }
}
