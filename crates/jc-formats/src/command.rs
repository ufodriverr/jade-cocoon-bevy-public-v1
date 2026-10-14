
use std::fmt;

use crate::action::{Action, WINDOWS};
use crate::animation::{Playback, Start, Stepped};
use crate::mover::{self, ArcTangent, Mover};
use crate::skeleton::cosine;

/// Bytes of one unit of a list. A command is one to three units.
pub const UNIT: usize = 4;
/// The number of command types.
pub const TYPES: usize = 25;
/// The action number that stands for the action a request has queued.
pub const QUEUED: i16 = 0x4003;
/// RAM addresses of the four lists of the executable that a request without a list of its
/// own takes: go to a target, turn to a target, play an action, play two actions.
pub const BUILT_IN: [u32; 4] = [0x8007_D880, 0x8007_D8A4, 0x8007_D8C8, 0x8007_D8E4];
/// RAM address of the creatures' move table in the executable.
pub const CREATURE_MOVES: u32 = 0x8007_9F48;

/// The longest list this reader takes, in commands. The longest on the disc is far shorter.
const MAX_COMMANDS: usize = 64;
/// The highest frame a command of this reader waits for. No animation is that long.
const MAX_FRAME: i16 = 2000;
/// The longest move table this reader takes. The longest on the disc has 32 lists.
const MAX_MOVES: usize = 64;

/// Why bytes are not a command list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandError {
    /// The list leaves the image at this RAM address.
    OutOfImage(u32),
    /// A type above 24.
    Type(u8),
    /// A command of this type with this many units.
    Length(u8, u8),
    /// A frame below -2 or above what any animation has.
    Frame(i16),
    /// No end within the longest list the reader takes.
    Endless,
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfImage(address) => write!(f, "the list leaves the image at {address:#010x}"),
            Self::Type(kind) => write!(f, "no command type {kind}"),
            Self::Length(kind, units) => write!(f, "a command of type {kind} with {units} units"),
            Self::Frame(frame) => write!(f, "a command that waits for frame {frame}"),
            Self::Endless => write!(f, "no end in {MAX_COMMANDS} commands"),
        }
    }
}

impl std::error::Error for CommandError {}

/// What a command waits for before it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    /// Nothing: it runs as soon as the list comes to it.
    Any,
    /// The done mark: the running action has played the loops wanted, or the command
    /// before it has waited.
    Done,
    /// This value of the actor's frame counter.
    Frame(i16),
}

/// What a command does, with its values. The numbers of the types are in [`Kind::number`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The end of the list. The list stays here for good.
    End,
    /// Start this action, [`QUEUED`] for the one a request queued.
    Action(i16),
    /// The next action starts at its loop start.
    FromLoopStart,
    /// The next action fades in.
    Fade,
    /// The action is to run this number of loops before it is done.
    Loops(i16),
    /// The same with the number of the request.
    RequestLoops,
    /// The share of black falls to none in the time to mark 16384; the value is set in the
    /// actor's flags.
    ShareFalls(i16),
    /// The share of black rises to all in that time.
    ShareRises(i16),
    /// Share, pulse and blend modes off.
    ShareOff,
    /// The size of the actor moves to this value in the time to mark 16385.
    Size(i16),
    /// The same to the size of the request.
    RequestSize,
    /// Shown or hidden.
    Show(bool),
    /// Ask the loader for the animation of this action.
    Read(i16),
    /// Wait until that read is through.
    WaitForRead,
    /// Give the animation of this action back.
    GiveBack(i16),
    /// Turn to the target when there is one, then wait until told to go on.
    Hold,
    /// The move is over.
    MoveOver,
    /// Start the action that ran before the list was given, at its loop start.
    ActionBefore,
    /// Wait until the actor has gone to its target.
    GoToTarget,
    /// Wait until the actor has turned to its target.
    TurnToTarget,
    /// A mark with a number. Does nothing when run.
    Mark(i16),
    /// Wait until no loops are wanted and the counter is on the action's end frame.
    WaitForEnd,
    /// Pause on or off.
    Pause(bool),
    /// Wait this number of game frames.
    Wait(i16),
    /// The share swings: the angle added per game frame and bits for the actor's flags.
    Pulse { step: i16, flags: i16 },
}

impl Kind {
    /// The type number of the command, 0 to 24.
    pub fn number(&self) -> u8 {
        match self {
            Self::End => 0,
            Self::Action(_) => 1,
            Self::FromLoopStart => 2,
            Self::Fade => 3,
            Self::Loops(_) => 4,
            Self::RequestLoops => 5,
            Self::ShareFalls(_) => 6,
            Self::ShareRises(_) => 7,
            Self::ShareOff => 8,
            Self::Size(_) => 9,
            Self::RequestSize => 10,
            Self::Show(_) => 11,
            Self::Read(_) => 12,
            Self::WaitForRead => 13,
            Self::GiveBack(_) => 14,
            Self::Hold => 15,
            Self::MoveOver => 16,
            Self::ActionBefore => 17,
            Self::GoToTarget => 18,
            Self::TurnToTarget => 19,
            Self::Mark(_) => 20,
            Self::WaitForEnd => 21,
            Self::Pause(_) => 22,
            Self::Wait(_) => 23,
            Self::Pulse { .. } => 24,
        }
    }
}

/// One command of a list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Command {
    pub when: When,
    pub kind: Kind,
    /// The length of the command in units.
    pub units: u8,
}

/// The units a command of a type must have, for the types that read a value.
fn units_of(kind: u8) -> Option<u8> {
    match kind {
        1 | 4 | 6 | 7 | 9 | 11 | 12 | 14 | 20 | 22 | 23 => Some(2),
        24 => Some(3),
        _ => None,
    }
}

/// The command list at a RAM address of an image whose first byte is at `base`, the end
/// included.
pub fn list_at(image: &[u8], base: u32, address: u32) -> Result<Vec<Command>, CommandError> {
    let mut at = address;
    let mut list = Vec::new();
    while list.len() < MAX_COMMANDS {
        let out = CommandError::OutOfImage(at);
        let start = at.checked_sub(base).ok_or(out)? as usize;
        let head = image.get(start..).and_then(|rest| rest.get(..UNIT));
        let head = head.ok_or(out)?;
        let (kind, units) = (head[0], head[1]);
        let frame = i16::from_le_bytes([head[2], head[3]]);
        if usize::from(kind) >= TYPES {
            return Err(CommandError::Type(kind));
        }
        if !(1..=3).contains(&units) || units_of(kind).is_some_and(|want| want != units) {
            return Err(CommandError::Length(kind, units));
        }
        let when = match frame {
            -2 => When::Any,
            -1 => When::Done,
            0..=MAX_FRAME => When::Frame(frame),
            _ => return Err(CommandError::Frame(frame)),
        };
        let bytes = (image.get(start..)).and_then(|rest| rest.get(..UNIT * usize::from(units)));
        let bytes = bytes.ok_or(out)?;
        // The value is the second half of the second unit, the second value that of the third.
        let value = |unit: usize| i16::from_le_bytes([bytes[4 * unit + 2], bytes[4 * unit + 3]]);
        let kind = match kind {
            0 => Kind::End,
            1 => Kind::Action(value(1)),
            2 => Kind::FromLoopStart,
            3 => Kind::Fade,
            4 => Kind::Loops(value(1)),
            5 => Kind::RequestLoops,
            6 => Kind::ShareFalls(value(1)),
            7 => Kind::ShareRises(value(1)),
            8 => Kind::ShareOff,
            9 => Kind::Size(value(1)),
            10 => Kind::RequestSize,
            11 => Kind::Show(value(1) != 0),
            12 => Kind::Read(value(1)),
            13 => Kind::WaitForRead,
            14 => Kind::GiveBack(value(1)),
            15 => Kind::Hold,
            16 => Kind::MoveOver,
            17 => Kind::ActionBefore,
            18 => Kind::GoToTarget,
            19 => Kind::TurnToTarget,
            20 => Kind::Mark(value(1)),
            21 => Kind::WaitForEnd,
            22 => Kind::Pause(value(1) != 0),
            23 => Kind::Wait(value(1)),
            _ => Kind::Pulse {
                step: value(1),
                flags: value(2),
            },
        };
        list.push(Command { when, kind, units });
        if kind == Kind::End {
            return Ok(list);
        }
        at = at.wrapping_add(4 * u32::from(units));
    }
    Err(CommandError::Endless)
}

/// The command list at a RAM address that lies in one of several images, each given with
/// the address of its first byte: an actor of a scene package names lists of the package
/// and of the executable alike.
pub fn list_in(images: &[(&[u8], u32)], address: u32) -> Option<Vec<Command>> {
    (images.iter()).find_map(|(image, base)| list_at(image, *base, address).ok())
}

/// The move table at a RAM address: the addresses of its lists by move number. The table
/// has no count; it ends at the first word that is not the address of a list.
pub fn move_table(images: &[(&[u8], u32)], address: u32) -> Vec<u32> {
    let word = |at: u32| {
        images.iter().find_map(|(image, base)| {
            let start = at.checked_sub(*base)? as usize;
            let bytes = image.get(start..)?.get(..4)?;
            Some(u32::from_le_bytes(bytes.try_into().ok()?))
        })
    };
    (0..MAX_MOVES as u32)
        .map_while(|index| word(address.checked_add(4 * index)?))
        .take_while(|list| list_in(images, *list).is_some())
        .collect()
}

/// The mark the share of black takes its time to (types 6 and 7).
pub const SHARE_MARK: i16 = 16384;
/// The mark the size takes its time to (types 9 and 10).
pub const SIZE_MARK: i16 = 16385;
/// The size of an actor that is as large as its record says.
pub const OWN_SIZE: i16 = 4096;

/// Bits of an actor's flag word that the size, the share of black and the pulse use
/// ([`Runner::flags`]).
pub mod flag {
    /// The size is on its way to its goal.
    pub const SIZE_MOVES: u32 = 1;
    /// It grows on that way.
    pub const SIZE_GROWS: u32 = 2;
    /// The share of black falls.
    pub const SHARE_FALLS: u32 = 0x20;
    /// The share of black rises.
    pub const SHARE_RISES: u32 = 0x40;
    /// Every face is drawn blended, in the mode of the bit's place in this row.
    pub const BLEND: [u32; 4] = [0x80, 0x100, 0x200, 0x400];
    /// The model is drawn with the copy of its palette that the tint has made
    /// (`crate::tint`).
    pub const TINTED: u32 = 0x800;
    /// The share swings.
    pub const PULSE: u32 = 0x4000_0000;
    /// What type 8 takes off: share, blend modes and pulse.
    pub const SHARE_AND_PULSE: u32 = 0x4000_07E0;
}

pub mod word {
    /// Every face is drawn blended, in the mode of the two bits above this one.
    pub const BLENDED: u32 = 0x10;
    /// The place of the blend mode.
    pub const MODE: u32 = 5;
    /// The colours are mixed towards black by the share in the upper half.
    pub const FADED: u32 = 0x80;
}

/// The quotient the console leaves for a division, which does not stop at a divisor of zero.
pub(crate) fn quotient(value: i32, by: i32) -> i32 {
    match by {
        0 if value < 0 => 1,
        0 => -1,
        by => value.wrapping_div(by),
    }
}

/// What the time routine walks a list to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Until {
    /// A mark with this number. The game's marks are numbered from 16384 up.
    Mark(i16),
    /// This frame of the animation that this action plays: the list is walked to the first
    /// action with that animation that does not end before the frame.
    Frame { action: i16, frame: i16 },
    /// The end of the move.
    MoveOver,
}

pub fn time(
    list: &[Command],
    actions: &[Action],
    until: Until,
    request_loops: i16,
    speed: i16,
) -> Option<i32> {
    let action = |number: i16| actions.get(usize::try_from(number).ok()?);
    let answer = |frames: i32| Some(quotient(frames.wrapping_shl(12), i32::from(speed)));
    let (mut frames, mut loops, mut from_loop_start) = (0i32, 0i32, false);
    for command in list {
        match command.kind {
            Kind::End => return None,
            Kind::Action(number) => {
                let played = action(number)?;
                let start = match from_loop_start {
                    true => played.loop_start,
                    false => played.first,
                };
                let (start, loop_start, end) = (
                    i32::from(start),
                    i32::from(played.loop_start),
                    i32::from(played.end),
                );
                if let Until::Frame { action: of, frame } = until {
                    let frame = i32::from(frame);
                    if action(of)?.animation == played.animation && end >= frame {
                        return answer(frames + frame - start);
                    }
                }
                // The way to the loop start once, the loop as often as asked; the frame the
                // action starts at is not counted.
                frames += (end - loop_start + 1) * (loops + 1) + (loop_start - start) - 1;
                from_loop_start = false;
            }
            Kind::Mark(number) if until == Until::Mark(number) => return answer(frames),
            Kind::Hold => frames = 0,
            Kind::FromLoopStart => from_loop_start = true,
            Kind::Loops(wanted) => loops = i32::from(wanted) - 1,
            Kind::RequestLoops => loops = i32::from(request_loops) - 1,
            Kind::Wait(waited) => frames += i32::from(waited),
            Kind::MoveOver => {
                if let Until::Frame { action: of, frame } = until {
                    let (end, frame) = (i32::from(action(of)?.end), i32::from(frame));
                    frames += (frame - end).max(0);
                }
                return answer(frames);
            }
            _ => {}
        }
    }
    None
}

pub fn bone_word(model: u32, bone: u16) -> u32 {
    const MODE_BITS: u32 = 3 << word::MODE;
    let bone = u32::from(bone);
    let joined = match model & word::BLENDED != 0 && bone & 4 == 0 {
        true => model | (bone & !MODE_BITS),
        false => (model & !MODE_BITS) | bone,
    };
    match bone & 8 {
        0 => joined & !0xFF00,
        _ => joined,
    }
}

/// The share of black of a word a mesh object is drawn with, out of 4096. The pulse gives a
/// share below zero, which makes the colours brighter.
pub fn fade_of(word: u32) -> i32 {
    match word & word::FADED {
        0 => 0,
        _ => (word as i32) >> 16,
    }
}

/// The blend mode a word forces on every face, when it does.
pub fn blend_of(word: u32) -> Option<u8> {
    (word & word::BLENDED != 0).then_some((word >> word::MODE & 3) as u8)
}

pub struct Feet<'a> {
    pub mover: &'a mut Mover,
    pub arc: &'a ArcTangent,
    /// The game's counter of the turn filter in this game frame.
    pub counter: u32,
    /// Whether the player or another actor stands within [`mover::NEAR`] of the target.
    pub someone_near: bool,
}

impl Feet<'_> {
    /// Type 19, and the routine `0x8001DE64` behind it: a step of the turn to the target.
    /// A game frame in which the actor still turns makes no step.
    fn turns_to_target(&mut self) -> bool {
        let (target, step) = (self.mover.target, self.mover.turn_step);
        let faces = self.mover.turn_to(self.arc, target, step, self.counter);
        match faces {
            true => self.mover.flags &= !mover::flag::TARGET,
            false => self.mover.flags |= mover::flag::NO_STEP,
        }
        faces
    }

    /// Type 15: an actor with a target turns to it before it holds.
    fn faces_target(&mut self) -> bool {
        let target = mover::flag::TARGET | mover::flag::TURNS;
        self.mover.flags & target == 0 || self.turns_to_target()
    }

    /// Type 18: the actor is at its target, or turns a step to it.
    fn goes_to_target(&mut self) -> bool {
        let (target, step) = (self.mover.target, self.mover.turn_step);
        let (near, counter) = (self.someone_near, self.counter);
        let there = self.mover.go_to(self.arc, target, step, near, counter);
        if there {
            self.mover.flags &= !mover::flag::TARGET;
        }
        there
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Request {
    /// The action to play (+40) and the one to play after it (+42).
    pub action: i16,
    pub then: Option<i16>,
    /// Bit 0 of the bits (+0): no fade for the next fade command.
    pub no_fade: bool,
    /// Not zero at +28: whoever gave the order is told when the move is over.
    pub told: bool,
    /// Not zero at +44: "go on" also ends the move at once.
    pub ends_at_go_on: bool,
    /// Bit 1 of the bits (+0): a confirm in a message window does not send the action
    /// to its end ([`Runner::confirmed`]).
    pub keeps_at_confirm: bool,
    /// Bit 2 of the bits (+0): the end of a path makes the next fade a quick one and
    /// takes [`Request::no_fade`] back ([`Runner::path_step`]).
    pub fades_at_path_end: bool,
}

/// The bits of a script's order of kind 9 or 10 (the third argument of `0x48` and `0x49`).
pub mod order {
    /// Start at the first frame instead of the loop start. Only the player's side reads
    /// it: for an actor the engine writes -1 or -2 as the request's start frame, and the
    /// interpreter takes a start frame only when it is not negative.
    pub const FIRST_FRAME: u32 = 1;
    /// No fade.
    pub const NO_FADE: u32 = 4;
    /// The move ends at "go on".
    pub const ENDS_AT_GO_ON: u32 = 8;
    /// A confirm in a message window leaves the action alone (the value 16: the order
    /// routine tests it with `andi 0x10`; until T179 the build tested bit 16).
    pub const KEEPS_AT_CONFIRM: u32 = 0x10;
    /// Kind 11 only: bit 2 of the request's bits, which the end of a path reads.
    pub const AT_PATH_END: u32 = 0x20;
}

impl Request {
    /// What the field engine's order routine `0x800A7CD0` makes of an order of kind 9
    /// (`then` is none) or 10 with these bits; `told` is the third word of the order
    /// record. The value 16 of the order becomes bit 1 of the request's bits, which the
    /// routine of a taken confirm tests (`0x8001DA18`).
    pub fn of_order(action: i16, then: Option<i16>, bits: u32, told: bool) -> Self {
        Self {
            action,
            then,
            no_fade: bits & order::NO_FADE != 0,
            told,
            ends_at_go_on: bits & order::ENDS_AT_GO_ON != 0,
            keeps_at_confirm: bits & order::KEEPS_AT_CONFIRM != 0,
            fades_at_path_end: false,
        }
    }

    /// The same for an order of kind 11, whose value 32 becomes bit 2 of the request's
    /// bits (the branch `0x800A7F4C` of the order routine).
    pub fn of_path_order(action: i16, then: Option<i16>, bits: u32, told: bool) -> Self {
        Self {
            fades_at_path_end: bits & order::AT_PATH_END != 0,
            ..Self::of_order(action, then, bits, told)
        }
    }
}

/// Why a runner's list cannot go on in this build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stuck {
    /// The list waits for the actor to go or turn to a target and the pass was made without
    /// [`Feet`].
    Target(Kind),
    /// The list starts an action the model does not have, or one without an animation.
    NoAction(Option<i16>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Runner {
    pub playback: Playback,
    /// Game frames a fade of the actor takes.
    pub fade_length: i16,
    list: Vec<Command>,
    /// The command the list stands at.
    at: usize,
    /// The running action, none once a list has been given and has not started one.
    pub action: Option<i16>,
    /// The action that ran when the list was given.
    pub before: Option<i16>,
    /// The loops the action has played and the loops wanted.
    loops: i16,
    wanted: i16,
    /// The done mark.
    pub done: bool,
    /// The last step put the counter on the action's end frame.
    at_end: bool,
    /// Not drawn.
    pub hidden: bool,
    /// A move is asked of the actor and it is not at its hold yet. A script waits for this
    /// to clear before it lets the actor go on.
    pub not_ready: bool,
    /// The actor is told to go on from its hold.
    pub go_on: bool,
    /// A move is running: from the request to the end of the list or a "move over".
    pub moving: bool,
    /// The action a request has queued for [`QUEUED`], and the one that moves up behind it.
    pub queued: Option<i16>,
    pub queued_next: Option<i16>,
    /// The frame a request wants the next action to start at; used once.
    pub start_frame: Option<i16>,
    /// Whoever asked for the move is told when it is over (flag 0x800000), and has been
    /// told and not asked yet ([`Runner::take_over`]).
    pub told: bool,
    over: bool,
    /// "Go on" also ends the move at once (+0x100 of the actor).
    pub ends_at_go_on: bool,
    /// A request's wish that the next fade command does nothing; used once.
    pub no_fade: bool,
    /// Bit 1 of the last request's bits (+0xD4): a confirm leaves the action alone.
    pub keeps_at_confirm: bool,
    /// Bit 2 of the last request's bits: read where a path ends.
    pub fades_at_path_end: bool,
    /// The top bit of the actor's flags: the next fade is a quick one.
    pub quick_fade: bool,
    /// The loops of a request, for type 5.
    pub request_loops: i16,
    /// The size of a request, for type 10.
    pub request_size: i16,
    /// Game frames a wait command has waited.
    waited: i32,
    pub stuck: Option<Stuck>,
    /// The bits of the actor's flag word that the size, the share and the pulse use
    /// ([`flag`]); the other flags of an actor are fields of their own here.
    pub flags: u32,
    /// How far the share of black has come on its way, out of 4096, and the game frames
    /// the way takes.
    share: i16,
    share_time: i16,
    /// The game frames of a pulse so far and the angle a game frame adds.
    pulse_frames: i16,
    pulse_step: i16,
    /// The size, [`OWN_SIZE`] being the actor's own, what a game frame moves it by and
    /// where it is going.
    pub size: i16,
    size_step: i16,
    size_goal: i16,
    /// The rows of video memory from the palette of the actor's model down to the place
    /// its tinted copy is kept at: what [`flag::TINTED`] adds to the word.
    rows: i16,
    /// The word the model is drawn with, made anew by every [`Runner::update`].
    pub word: u32,
}

impl Runner {
    /// An actor without a list that plays `playback` in action `action`.
    pub fn new(playback: Playback, action: i16, fade_length: i16) -> Self {
        Self {
            playback,
            fade_length,
            list: Vec::new(),
            at: 0,
            action: Some(action),
            before: None,
            loops: 0,
            wanted: 0,
            done: false,
            at_end: false,
            hidden: false,
            not_ready: false,
            go_on: false,
            moving: false,
            queued: None,
            queued_next: None,
            start_frame: None,
            told: false,
            over: false,
            ends_at_go_on: false,
            no_fade: false,
            keeps_at_confirm: false,
            fades_at_path_end: false,
            quick_fade: false,
            request_loops: 0,
            request_size: 0,
            waited: 0,
            stuck: None,
            flags: 0,
            share: 0,
            share_time: 0,
            pulse_frames: 0,
            pulse_step: 0,
            size: OWN_SIZE,
            size_step: 0,
            size_goal: OWN_SIZE,
            rows: 0,
            word: 0,
        }
    }

    pub fn tint(&mut self, rows: i16) {
        self.flags |= flag::TINTED;
        self.rows = rows;
    }

    /// The setter of the tint without a record: the actor is drawn with its model's own
    /// palette again.
    pub fn untint(&mut self) {
        self.flags &= !flag::TINTED;
    }

    /// A word made elsewhere (the player's, whose controller makes it) with the rows to
    /// the tinted palette in its second byte when the actor is tinted, as
    /// [`Runner::update`] adds them to its own.
    pub fn with_tint(&self, word: u32) -> u32 {
        match self.flags & flag::TINTED {
            0 => word,
            _ => word & !0xFF00 | (i32::from(self.rows) << 8) as u32 & 0xFF00,
        }
    }

    pub fn copied(&self) -> Self {
        let old = self.playback;
        let mut playback = Playback::new(old.animation, old.span, old.speed);
        playback.start(old.animation, old.span, Start::At(old.counter), None);
        let action = self.action.or(self.before).unwrap_or(0);
        Self {
            hidden: self.hidden,
            ..Self::new(playback, action, self.fade_length)
        }
    }

    /// The game frames the list takes from the command it stands at to a mark, -1 when
    /// the game finds none ([`time`]).
    fn time_to(&self, actions: &[Action], mark: i16) -> i32 {
        let rest = self.list.get(self.at..).unwrap_or_default();
        let (loops, speed) = (self.request_loops, self.playback.speed);
        time(rest, actions, Until::Mark(mark), loops, speed).unwrap_or(-1)
    }

    pub fn window_time(&self, actions: &[Action], action: i16) -> i32 {
        let window = usize::try_from(action).ok().and_then(|at| actions.get(at));
        let Some([opens, closes]) = window.map(|record| record.windows[0]) else {
            return -1;
        };
        if opens < 0 && closes < 0 {
            return -1;
        }
        let rest = self.list.get(self.at..).unwrap_or_default();
        let until = Until::Frame {
            action,
            frame: opens,
        };
        let (loops, speed) = (self.request_loops, self.playback.speed);
        time(rest, actions, until, loops, speed).unwrap_or(-1)
    }

    pub fn window_times(&self, actions: &[Action], action: i16) -> [[i16; 2]; WINDOWS] {
        let record = usize::try_from(action).ok().and_then(|at| actions.get(at));
        let Some(record) = record else {
            return [[-1; 2]; WINDOWS];
        };
        let rest = self.list.get(self.at..).unwrap_or_default();
        let (loops, speed) = (self.request_loops, self.playback.speed);
        record.windows.map(|pair| match pair {
            [opens, closes] if opens < 0 && closes < 0 => [-1; 2],
            pair => pair.map(|frame| {
                let until = Until::Frame { action, frame };
                time(rest, actions, until, loops, speed).unwrap_or(-1) as i16
            }),
        })
    }

    /// The game frames from the command the list stands at to its "move over", -1 when
    /// it has none (`0x8001F0A4` with no action).
    pub fn whole_time(&self, actions: &[Action]) -> i32 {
        let rest = self.list.get(self.at..).unwrap_or_default();
        let (loops, speed) = (self.request_loops, self.playback.speed);
        time(rest, actions, Until::MoveOver, loops, speed).unwrap_or(-1)
    }

    /// Starts the share of black on its way, as the setters `0x8001DA4C` (falling) and
    /// `0x8001DA70` (rising) do: over `time` game frames, with further bits for the flags.
    fn share_moves(&mut self, way: u32, time: i32, bits: i16) {
        (self.share, self.share_time) = (0, time as i16);
        self.flags |= way | bits as u32;
        self.flags &= !((flag::SHARE_FALLS | flag::SHARE_RISES) & !way);
    }

    /// Starts the size on its way to `goal` over `time` game frames, as the setter
    /// `0x8001DAB4` does. It only sets flags: a size that was growing and is now sent
    /// down keeps the mark of growing.
    fn size_moves(&mut self, time: i32, goal: i16) {
        let (from, to) = (i32::from(self.size), i32::from(goal));
        let step = match to < from {
            true => {
                self.flags |= flag::SIZE_MOVES;
                quotient(from - to, time)
            }
            false => {
                self.flags |= flag::SIZE_MOVES | flag::SIZE_GROWS;
                quotient(to - from, time)
            }
        };
        (self.size_step, self.size_goal) = (step as i16, goal);
    }

    /// What the actor's task does between the pass and the drawing: the step of the size
    /// (`0x8001FA58`) and the making of the word the model is drawn with (`0x8001FB68`).
    pub fn update(&mut self) {
        if self.flags & flag::SIZE_MOVES != 0 {
            if self.flags & flag::SIZE_GROWS != 0 {
                self.size = self.size.wrapping_add(self.size_step);
                if self.size_goal < self.size {
                    self.size = self.size_goal;
                    self.flags &= !(flag::SIZE_MOVES | flag::SIZE_GROWS);
                }
            } else {
                self.size = self.size.wrapping_sub(self.size_step);
                if self.size < self.size_goal {
                    self.size = self.size_goal;
                    self.flags &= !flag::SIZE_MOVES;
                }
            }
        }

        // A blend mode only counts while the share or the pulse is at work.
        let blend = (flag::BLEND.iter())
            .position(|bit| self.flags & bit != 0)
            .map_or(0, |mode| word::BLENDED | (mode as u32) << word::MODE);
        let with_share = |share: i32| blend | word::FADED | (share as u32) << 16;
        self.word = 0;
        if self.flags & flag::PULSE != 0 {
            let angle = i32::from(self.pulse_frames) * i32::from(self.pulse_step);
            self.pulse_frames = self.pulse_frames.wrapping_add(1);
            // Between none and minus all: the colours swing between their own and twice that.
            self.word = with_share(-((cosine(angle) + 4096) / 2));
        }
        if self.flags & (flag::SHARE_FALLS | flag::SHARE_RISES) != 0 {
            let step = quotient(4096, i32::from(self.share_time));
            let come = (i32::from(self.share) + step).clamp(0, 4096);
            self.share = come as i16;
            self.word = with_share(match self.flags & flag::SHARE_FALLS {
                0 => come,
                _ => 4096 - come,
            });
        }
        // The rows to the tinted palette go into the second byte, whatever came before.
        if self.flags & flag::TINTED != 0 {
            self.word |= (i32::from(self.rows) << 8) as u32;
        }
    }

    /// The scale the model is posed with: the actor's own, times the size.
    pub fn scale(&self, own: [i32; 3]) -> [i32; 3] {
        own.map(|part| part.wrapping_mul(i32::from(self.size)) / 4096)
    }

    /// Gives the actor a list, as the setter `0x8001DC00` does: the list starts over, the
    /// running action becomes the action before and is forgotten, so that the first action
    /// command always starts, and the first pass finds the done mark.
    pub fn set_list(&mut self, list: Vec<Command>) {
        self.list = list;
        self.at = 0;
        self.before = self.action.take();
        self.loops = 0;
        self.wanted = 0;
        self.at_end = true;
        self.stuck = None;
    }

    /// A request for an action or two (`0x8001E2BC`, then `0x8001E3B4`): the move runs and
    /// is not ready or told to go on, the action is queued with the one behind it and the
    /// actor gets the built-in list 3, or 4 with a second action (`built_in` are the lists
    /// at [`BUILT_IN`], in that order). The loops and the size of the request are zero in
    /// the field engine's requests. The game's request gives the list with the setter that
    /// runs the interpreter once (`0x8001DC9C`), which takes the actor to the hold at
    /// once: whoever makes a request calls a pass ([`Runner::pass_with`]) right behind it,
    /// and the same behind [`Runner::request_target`] and [`Runner::request_list`].
    pub fn request(&mut self, request: &Request, built_in: &[Vec<Command>; 4]) {
        (self.go_on, self.not_ready, self.moving) = (false, true, true);
        self.no_fade = request.no_fade;
        self.keeps_at_confirm = request.keeps_at_confirm;
        self.fades_at_path_end = request.fades_at_path_end;
        (self.request_loops, self.request_size) = (0, 0);
        self.ends_at_go_on = request.ends_at_go_on;
        self.queued = Some(request.action);
        self.queued_next = request.then;
        // The start frame of such a request is negative, which is none.
        self.start_frame = None;
        self.set_list(match request.then {
            Some(_) => built_in[3].clone(),
            None => built_in[2].clone(),
        });
        self.told |= request.told;
    }

    /// A request with a target (`0x8001E2BC`, then `0x8001E3B4` with a place at +8), as
    /// the field engine makes it of a script's order of kind 0, 1 or 2 (`0x800A7CD0`):
    /// the action is queued and the actor gets the built-in list 1, which goes to the
    /// target, or with `turns` list 2, which turns to it. The request's bits and its
    /// word for "go on ends the move" are zero. The target itself, its flag and the turn
    /// step are the mover's ([`crate::mover::Mover::target`]).
    pub fn request_target(
        &mut self,
        action: i16,
        turns: bool,
        told: bool,
        built_in: &[Vec<Command>; 4],
    ) {
        (self.go_on, self.not_ready, self.moving) = (false, true, true);
        (self.no_fade, self.keeps_at_confirm) = (false, false);
        (self.request_loops, self.request_size) = (0, 0);
        self.ends_at_go_on = false;
        self.fades_at_path_end = false;
        self.queued = Some(action);
        self.queued_next = None;
        self.start_frame = None;
        self.set_list(built_in[usize::from(turns)].clone());
        self.told |= told;
    }

    /// A request with a list of its own (`0x8001E2BC`, then `0x8001E3B4` with the list at
    /// +4), as the field engine makes it of a script's order of kind 5 (`0x800A7CD0`):
    /// the move runs and is not ready or told to go on, and the actor gets the list.
    /// Such a request names no action, so what an earlier request queued stays; its
    /// bits, loops, size and the word for "go on ends the move" are zero.
    pub fn request_list(&mut self, list: Vec<Command>, told: bool) {
        (self.go_on, self.not_ready, self.moving) = (false, true, true);
        (self.no_fade, self.keeps_at_confirm) = (false, false);
        (self.request_loops, self.request_size) = (0, 0);
        self.ends_at_go_on = false;
        self.fades_at_path_end = false;
        self.start_frame = None;
        self.set_list(list);
        self.told |= told;
    }

    /// "Free" (`0x8001E230`): no move runs. The game also asks that the actor has no
    /// target, which is the mover's flag ([`crate::mover::flag::TARGET`]).
    pub fn free(&self) -> bool {
        !self.moving
    }

    /// "Ready" (`0x8001E244`) for an actor of a room: at its hold. (For the actors numbered
    /// below 3 the game also asks for no target; they are a fight's.)
    pub fn ready(&self) -> bool {
        !self.not_ready
    }

    /// "Go on" (`0x8001E2F4`) for an actor of a room: the hold lets the list go on, and a
    /// move that ends at "go on" is over.
    pub fn goes_on(&mut self) {
        self.go_on = true;
        if self.ends_at_go_on {
            self.move_over();
        }
    }

    /// The move is over (`0x8001F02C`): whoever is to be told of a move that runs is told,
    /// and the flags of a move (0x00E80000) are cleared. The target flag among them is the
    /// mover's.
    fn move_over(&mut self) {
        self.over |= self.told && self.moving;
        (self.told, self.not_ready, self.moving) = (false, false, false);
    }

    /// Whether a move was over since the last call, of which the one who asked is told
    /// (`0x800A5B5C` with the actor's thing: every script task loses that bit of its busy
    /// mask).
    pub fn take_over(&mut self) -> bool {
        std::mem::take(&mut self.over)
    }

    /// Whether the actor has a list.
    pub fn has_list(&self) -> bool {
        !self.list.is_empty()
    }

    /// The command the list stands at.
    pub fn command(&self) -> Option<Command> {
        self.list.get(self.at).copied()
    }

    /// Starts an action of the model's table in the playback. False when there is none.
    fn start(&mut self, actions: &[Action], action: Option<i16>, start: Start, fade: bool) -> bool {
        let found = action
            .and_then(|action| actions.get(usize::try_from(action).ok()?))
            .and_then(|record| Some((record.animation?, record.span())));
        let Some((animation, span)) = found else {
            self.stuck = Some(Stuck::NoAction(action));
            return false;
        };
        let fade = fade.then_some(self.fade_length);
        self.playback.start(animation, span, start, fade);
        // The top bit of the flags lasts until a fade has run with it.
        if let Some(fade) = &mut self.playback.fade {
            fade.quick = std::mem::take(&mut self.quick_fade);
        }
        self.action = action;
        true
    }

    pub fn start_now(
        &mut self,
        actions: &[Action],
        action: i16,
        from_loop_start: bool,
        fade: bool,
    ) -> bool {
        let start = match from_loop_start {
            true => Start::LoopStart,
            false => Start::First,
        };
        self.start(actions, Some(action), start, fade)
    }

    /// One pass of the interpreter `0x8001EAB0` over the list, with the action table of the
    /// actor's model, for an actor that is not moved: a list that waits for a target stops
    /// and says so in [`Runner::stuck`]. Returns the commands that ran, in order.
    pub fn pass(&mut self, actions: &[Action]) -> Vec<Kind> {
        self.pass_with(actions, None)
    }

    /// The same pass for an actor with [`Feet`]: types 15, 18 and 19 turn it and send it to
    /// its target.
    pub fn pass_with(&mut self, actions: &[Action], mut feet: Option<Feet<'_>>) -> Vec<Kind> {
        let mut ran = Vec::new();
        if self.list.is_empty() || self.stuck.is_some() {
            return ran;
        }
        if self.at_end {
            self.loops = self.loops.wrapping_add(1);
            if self.loops >= self.wanted {
                self.loops = 0;
                self.wanted = 0;
                self.done = true;
            }
        }
        // What a command's frame is held against for the whole pass: the frame counter as
        // it is now, or "done". A command that runs takes the mark off the actor, not off
        // the pass.
        let seen = (!self.done).then_some(self.playback.counter);
        let (mut from_loop_start, mut fade) = (false, false);
        // An actor on a path (flag 0x02000000) gets no fade and no end of its action.
        let on_path = |feet: &Option<Feet<'_>>| {
            (feet.as_ref()).is_some_and(|feet| feet.mover.flags & mover::flag::PATH != 0)
        };
        while let Some(command) = self.command() {
            let fits = match command.when {
                When::Any => true,
                When::Done => seen.is_none(),
                When::Frame(frame) => {
                    let frame = i32::from(frame);
                    seen == Some(frame) || self.playback.counter == frame
                }
            };
            if !fits {
                break;
            }
            self.done = false;
            ran.push(command.kind);
            // A command that waits sets the done mark and stays where it is.
            let mut waits = false;
            let mut ends_pass = false;
            match command.kind {
                Kind::End => {
                    // The flags of a move are cleared here without a word to anyone.
                    (self.told, self.not_ready, self.moving) = (false, false, false);
                    break;
                }
                Kind::Action(action) => {
                    ends_pass = true;
                    if self.action != Some(action) {
                        self.playback.paused = false;
                        let action = match action {
                            QUEUED => {
                                let queued = self.queued;
                                self.queued = self.queued_next.take();
                                queued
                            }
                            action => Some(action),
                        };
                        let start = match (self.start_frame.take(), from_loop_start) {
                            (Some(frame), _) => Start::At(i32::from(frame)),
                            (None, true) => Start::LoopStart,
                            (None, false) => Start::First,
                        };
                        if !self.start(actions, action, start, fade) {
                            break;
                        }
                    }
                }
                Kind::FromLoopStart => from_loop_start = true,
                // On a path the command does nothing, and the request's bit stays.
                Kind::Fade if on_path(&feet) => {}
                Kind::Fade => {
                    fade |= !self.no_fade;
                    self.no_fade = false;
                }
                Kind::Loops(loops) => (self.loops, self.wanted) = (0, loops),
                Kind::RequestLoops => (self.loops, self.wanted) = (0, self.request_loops),
                Kind::Show(shown) => self.hidden = !shown,
                Kind::Hold => {
                    // An actor with a target first turns to it, a step in each pass.
                    if feet.as_mut().is_some_and(|feet| !feet.faces_target()) {
                        waits = true;
                    } else {
                        self.not_ready = false;
                        match self.go_on {
                            true => self.go_on = false,
                            false => waits = true,
                        }
                    }
                }
                Kind::MoveOver => self.move_over(),
                Kind::ActionBefore => {
                    if self.action != self.before
                        && !self.start(actions, self.before, Start::LoopStart, fade)
                    {
                        break;
                    }
                }
                Kind::GoToTarget | Kind::TurnToTarget => match feet.as_mut() {
                    Some(feet) => {
                        let there = match command.kind {
                            Kind::GoToTarget => feet.goes_to_target(),
                            _ => feet.turns_to_target(),
                        };
                        waits = !there;
                    }
                    None => {
                        self.stuck = Some(Stuck::Target(command.kind));
                        waits = true;
                    }
                },
                Kind::WaitForEnd => {
                    waits = self.wanted != 0
                        || self.playback.counter != i32::from(self.playback.span.end)
                        || on_path(&feet);
                }
                Kind::Pause(paused) => self.playback.paused = paused,
                Kind::Wait(frames) => {
                    self.waited += 1;
                    match i32::from(frames) < self.waited {
                        true => self.waited = 0,
                        false => waits = true,
                    }
                }
                Kind::ShareFalls(bits) => {
                    let time = self.time_to(actions, SHARE_MARK);
                    self.share_moves(flag::SHARE_FALLS, time, bits);
                }
                Kind::ShareRises(bits) => {
                    let time = self.time_to(actions, SHARE_MARK);
                    self.share_moves(flag::SHARE_RISES, time, bits);
                }
                Kind::ShareOff => {
                    self.flags &= !flag::SHARE_AND_PULSE;
                    (self.share, self.share_time) = (0, 0);
                }
                Kind::Size(goal) => {
                    let time = self.time_to(actions, SIZE_MARK);
                    self.size_moves(time, goal);
                }
                Kind::RequestSize => {
                    let time = self.time_to(actions, SIZE_MARK);
                    self.size_moves(time, self.request_size);
                }
                Kind::Pulse { step, flags } => {
                    (self.pulse_frames, self.pulse_step) = (0, step);
                    self.flags |= flag::PULSE | flags as u32;
                }
                Kind::Read(_) | Kind::WaitForRead | Kind::GiveBack(_) | Kind::Mark(_) => {}
            }
            if waits {
                self.done = true;
                break;
            }
            self.at += 1;
            if ends_pass {
                break;
            }
        }
        ran
    }

    pub fn confirmed(&mut self) {
        if !self.keeps_at_confirm {
            self.playback.to_end();
            self.at_end = true;
        }
    }

    /// The step of the animation at the end of a game frame.
    pub fn step(&mut self) -> Stepped {
        let stepped = self.playback.step();
        self.at_end = stepped.at_end;
        stepped
    }

    /// What the end of the animation's step does for an actor on a path (`0x800200C0`
    /// on), after [`Runner::step`], with what [`Mover::path_step`] answered. While the
    /// path runs no game frame has the mark of the action's end, so the action goes
    /// round and no loop is counted. A path that is `through` puts the action on its end
    /// frame with that mark; with bit 2 of the request's bits the next fade is then a
    /// quick one and the request's wish for no fade is taken back.
    pub fn path_step(&mut self, through: bool) {
        self.at_end = through;
        if through {
            self.playback.counter = i32::from(self.playback.span.end);
            if self.fades_at_path_end {
                self.quick_fade = true;
                self.no_fade = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::TAIL;
    use crate::animation::WHOLE;

    const BASE: u32 = 0x8000_1000;

    /// One command as bytes: type, frame and the values of its further units.
    fn unit(kind: u8, frame: i16, values: &[i16]) -> Vec<u8> {
        let mut bytes = vec![kind, 1 + values.len() as u8];
        bytes.extend(frame.to_le_bytes());
        for value in values {
            bytes.extend([0, 0]);
            bytes.extend(value.to_le_bytes());
        }
        bytes
    }

    fn command(when: When, kind: Kind) -> Command {
        let units = units_of(kind.number()).unwrap_or(1);
        Command { when, kind, units }
    }

    /// An action that plays animation `animation` from frame 0 to `end`, looping from
    /// `loop_start`.
    fn action(animation: usize, loop_start: i16, end: i16) -> Action {
        Action {
            animation: Some(animation),
            resident: 1,
            extra: [0; 2],
            first: 0,
            loop_start,
            hold: end,
            end,
            windows: [[-1; 2]; WINDOWS],
            tail: [-1; TAIL],
        }
    }

    fn actions() -> Vec<Action> {
        vec![action(10, 0, 5), action(11, 1, 3), action(12, 2, 4)]
    }

    /// An actor that stands in action 0 of [`actions`].
    fn actor() -> Runner {
        let playback = Playback::new(10, actions()[0].span(), WHOLE as i16);
        Runner::new(playback, 0, 4)
    }

    /// One game frame; returns the commands that ran.
    fn frame(runner: &mut Runner) -> Vec<Kind> {
        let ran = runner.pass(&actions());
        runner.step();
        ran
    }

    #[test]
    fn the_copy_of_an_actor_is_in_its_action_at_its_frame_without_its_list() {
        // An actor that a list has taken from action 0 into action 2, four game frames
        // in, hidden and with a size of its own.
        let mut runner = actor();
        runner.set_list(vec![
            command(When::Any, Kind::Action(2)),
            command(When::Any, Kind::End),
        ]);
        for _ in 0..4 {
            frame(&mut runner);
        }
        (runner.hidden, runner.size, runner.fade_length) = (true, 1378, 7);
        assert_eq!(runner.action, Some(2));
        runner.playback.counter = 3;

        let copy = runner.copied();
        assert_eq!((copy.action, copy.playback.animation), (Some(2), 12));
        assert_eq!(
            (copy.playback.counter, copy.playback.span),
            (3, actions()[2].span())
        );
        // The start value is taken modulo the action's end, 4 here: a counter on the
        // end frame or past it is wrapped.
        for (counter, wrapped) in [(4, 0), (9, 1)] {
            runner.playback.counter = counter;
            assert_eq!(runner.copied().playback.counter, wrapped);
        }
        assert_eq!((copy.playback.speed, copy.fade_length), (WHOLE as i16, 7));
        // No fade from the action before, no list, and a new actor's size; hidden as
        // the actor is.
        assert_eq!(copy.playback.fade, None);
        assert!(!copy.has_list() && copy.free() && copy.ready());
        assert_eq!((copy.hidden, copy.size), (true, OWN_SIZE));

        // An actor whose list has started no action yet is copied in the action before.
        let mut waiting = actor();
        waiting.set_list(vec![command(When::Any, Kind::End)]);
        assert_eq!(waiting.copied().action, waiting.action.or(waiting.before));
        assert_eq!(waiting.copied().action, Some(0));
    }

    #[test]
    fn a_list_is_read_up_to_its_end() {
        let mut image = vec![0xEE; 8];
        image.extend(unit(15, -2, &[]));
        image.extend(unit(3, -1, &[]));
        image.extend(unit(1, -1, &[QUEUED]));
        image.extend(unit(23, 7, &[30]));
        image.extend(unit(24, -2, &[100, 0x80]));
        image.extend(unit(11, -2, &[0]));
        image.extend(unit(0, -2, &[]));
        let list = list_at(&image, BASE, BASE + 8).unwrap();
        assert_eq!(
            list,
            [
                command(When::Any, Kind::Hold),
                command(When::Done, Kind::Fade),
                command(When::Done, Kind::Action(QUEUED)),
                command(When::Frame(7), Kind::Wait(30)),
                command(
                    When::Any,
                    Kind::Pulse {
                        step: 100,
                        flags: 0x80
                    }
                ),
                command(When::Any, Kind::Show(false)),
                command(When::Any, Kind::End),
            ]
        );
        assert_eq!(list[4].units, 3);
        for (number, command) in list.iter().enumerate() {
            let kind = [15, 3, 1, 23, 24, 11, 0][number];
            assert_eq!(command.kind.number(), kind);
        }
        // A list may start anywhere inside another one.
        assert_eq!(list_at(&image, BASE, BASE + 12).unwrap().len(), 6);
    }

    #[test]
    fn what_is_no_list_is_an_error() {
        let read = |bytes: &[u8]| list_at(bytes, BASE, BASE);
        assert_eq!(read(&unit(25, -2, &[])), Err(CommandError::Type(25)));
        assert_eq!(read(&unit(1, -2, &[])), Err(CommandError::Length(1, 1)));
        assert_eq!(read(&unit(24, -2, &[1])), Err(CommandError::Length(24, 2)));
        assert_eq!(read(&[2, 0, 0xFE, 0xFF]), Err(CommandError::Length(2, 0)));
        assert_eq!(read(&unit(2, -3, &[])), Err(CommandError::Frame(-3)));
        assert_eq!(read(&unit(2, 2001, &[])), Err(CommandError::Frame(2001)));
        assert_eq!(
            read(&unit(2, -2, &[])),
            Err(CommandError::OutOfImage(BASE + 4))
        );
        assert_eq!(
            list_at(&unit(0, -2, &[]), BASE, BASE - 4),
            Err(CommandError::OutOfImage(BASE - 4))
        );
        let endless = unit(2, -2, &[]).repeat(MAX_COMMANDS + 1);
        assert_eq!(read(&endless), Err(CommandError::Endless));
    }

    #[test]
    fn a_move_table_ends_at_the_first_word_that_names_no_list() {
        const OTHER: u32 = 0x8010_0000;
        // One image holds the table and a list, the other one a second list.
        let mut image = Vec::new();
        for address in [BASE + 16, OTHER + 4, BASE + 16, 0x1F, BASE + 16] {
            image.extend(u32::to_le_bytes(address));
        }
        image.truncate(16);
        image.extend(unit(16, -2, &[]));
        image.extend(unit(0, -2, &[]));
        let mut other = vec![0xEE; 4];
        other.extend(unit(0, -1, &[]));
        let images = [(&image[..], BASE), (&other[..], OTHER)];
        assert_eq!(move_table(&images, BASE), [BASE + 16, OTHER + 4, BASE + 16]);
        assert_eq!(list_in(&images, OTHER + 4).unwrap().len(), 1);
        assert_eq!(list_in(&images, OTHER), None);
        assert!(move_table(&images, OTHER).is_empty());
    }

    #[test]
    fn a_queued_action_is_played_from_the_hold_to_its_end() {
        // The third built-in list.
        let list = vec![
            command(When::Any, Kind::Hold),
            command(When::Done, Kind::Fade),
            command(When::Done, Kind::Action(QUEUED)),
            command(When::Any, Kind::WaitForEnd),
            command(When::Any, Kind::MoveOver),
            command(When::Any, Kind::End),
        ];
        let mut runner = actor();
        assert!(!runner.has_list());
        assert!(frame(&mut runner).is_empty(), "no list, no pass");
        runner.queued = Some(1);
        runner.queued_next = Some(2);
        (runner.moving, runner.not_ready) = (true, true);
        runner.set_list(list);
        assert_eq!((runner.action, runner.before), (None, Some(0)));

        // The actor comes to its hold in the first pass and stays there, in the old action.
        for _ in 0..3 {
            assert_eq!(frame(&mut runner), [Kind::Hold]);
            assert!(runner.done && !runner.not_ready && runner.moving);
            assert_eq!(runner.playback.animation, 10);
        }
        // Told to go on, it starts the queued action in the same pass, with a fade, and the
        // pass ends there.
        runner.go_on = true;
        let ran = runner.pass(&actions());
        assert_eq!(ran, [Kind::Hold, Kind::Fade, Kind::Action(QUEUED)]);
        assert!(!runner.go_on && !runner.done);
        assert_eq!(
            (runner.action, runner.queued, runner.queued_next),
            (Some(1), Some(2), None)
        );
        assert_eq!(
            (runner.playback.animation, runner.playback.counter),
            (11, 0)
        );
        let fade = runner.playback.fade.expect("a fade");
        // The old action stood at the frame of its four steps so far.
        assert_eq!((fade.animation, fade.counter, fade.length), (10, 4, 4));
        runner.step();
        // The end frame is 3: two passes wait, the third finds the counter there.
        assert_eq!(frame(&mut runner), [Kind::WaitForEnd]);
        assert_eq!(frame(&mut runner), [Kind::WaitForEnd]);
        assert!(runner.moving);
        assert_eq!(runner.playback.counter, 3);
        assert_eq!(
            frame(&mut runner),
            [Kind::WaitForEnd, Kind::MoveOver, Kind::End]
        );
        assert!(!runner.moving);
        // The list stays at its end and the action loops on.
        assert_eq!(frame(&mut runner), [Kind::End]);
        assert_eq!(runner.playback.counter, 2);
        assert_eq!(
            runner.command().map(|command| command.kind),
            Some(Kind::End)
        );
    }

    #[test]
    fn an_action_is_started_at_once_without_a_list() {
        let mut runner = actor();
        // From the loop start, without a fade.
        assert!(runner.start_now(&actions(), 1, true, false));
        assert_eq!((runner.action, runner.playback.animation), (Some(1), 11));
        assert_eq!((runner.playback.counter, runner.playback.fade), (1, None));
        // The running action starts again from its first frame, with the actor's fade.
        runner.step();
        assert!(runner.start_now(&actions(), 1, false, true));
        assert_eq!(runner.playback.counter, 0);
        assert!(runner.playback.fade.is_some());
        // An action the model does not have is not started.
        assert!(!runner.start_now(&actions(), 9, false, false));
    }

    #[test]
    fn loops_frames_and_waits_hold_a_list_back() {
        let list = vec![
            command(When::Any, Kind::FromLoopStart),
            command(When::Any, Kind::Action(1)),
            command(When::Any, Kind::Loops(2)),
            command(When::Done, Kind::Show(false)),
            command(When::Frame(2), Kind::Pause(true)),
            command(When::Any, Kind::Wait(3)),
            command(When::Any, Kind::Pause(false)),
            command(When::Any, Kind::ActionBefore),
            command(When::Any, Kind::Mark(16384)),
            command(When::Any, Kind::End),
        ];
        let mut runner = actor();
        runner.set_list(list);
        // An action ends the pass; it starts at its loop start, without a fade.
        assert_eq!(frame(&mut runner), [Kind::FromLoopStart, Kind::Action(1)]);
        assert_eq!((runner.playback.counter, runner.playback.fade), (2, None));
        assert_eq!(frame(&mut runner), [Kind::Loops(2)]);
        // Action 1 runs 1, 2, 3 and round again: the pass after the step that puts the
        // counter on the end frame for the second time finds two loops played.
        let mut counters = Vec::new();
        while !runner.hidden {
            counters.push(runner.playback.counter);
            let ran = runner.pass(&actions());
            assert!(ran.is_empty() || ran == [Kind::Show(false)]);
            runner.step();
        }
        assert_eq!(counters, [3, 1, 2, 3]);
        // The pause waits for frame 2, and the wait behind it runs in the same pass.
        assert_eq!(runner.playback.counter, 1);
        assert!(frame(&mut runner).is_empty());
        assert_eq!(runner.playback.counter, 2);
        assert_eq!(frame(&mut runner), [Kind::Pause(true), Kind::Wait(3)]);
        // A wait of three game frames is over in the fourth pass; the counter stands.
        assert_eq!(frame(&mut runner), [Kind::Wait(3)]);
        assert_eq!(frame(&mut runner), [Kind::Wait(3)]);
        assert_eq!(runner.playback.counter, 2);
        // The action before starts at its loop start and does not end the pass.
        let ran = runner.pass(&actions());
        let rest = [
            Kind::Wait(3),
            Kind::Pause(false),
            Kind::ActionBefore,
            Kind::Mark(16384),
            Kind::End,
        ];
        assert_eq!(ran, rest);
        assert_eq!((runner.action, runner.playback.animation), (Some(0), 10));
        assert_eq!(
            (runner.playback.counter, runner.playback.paused),
            (0, false)
        );
    }

    #[test]
    fn a_running_action_is_not_started_again_and_a_start_frame_is_used_once() {
        let mut runner = actor();
        runner.set_list(vec![
            command(When::Any, Kind::Action(2)),
            command(When::Any, Kind::Action(2)),
            command(When::Any, Kind::Fade),
            command(When::Any, Kind::Action(1)),
            command(When::Any, Kind::End),
        ]);
        runner.start_frame = Some(7);
        runner.no_fade = true;
        // Frame 7 of a span that ends at 4 is frame 3.
        assert_eq!(frame(&mut runner), [Kind::Action(2)]);
        assert_eq!((runner.playback.counter, runner.start_frame), (4, None));
        // The same action again only ends the pass.
        assert_eq!(frame(&mut runner), [Kind::Action(2)]);
        assert_eq!(runner.playback.counter, 2);
        // The request's bit takes the fade away, once.
        assert_eq!(frame(&mut runner), [Kind::Fade, Kind::Action(1)]);
        assert_eq!((runner.playback.fade, runner.no_fade), (None, false));
    }

    /// The shape of the built-in lists 3 and 4: a hold, a fade, the queued action to its
    /// end and "move over"; list 4 then fades into the second queued action.
    fn built_in() -> [Vec<Command>; 4] {
        let one = vec![
            command(When::Any, Kind::Hold),
            command(When::Done, Kind::Fade),
            command(When::Done, Kind::Action(QUEUED)),
            command(When::Any, Kind::WaitForEnd),
            command(When::Any, Kind::MoveOver),
            command(When::Any, Kind::End),
        ];
        let mut two = one.clone();
        two.splice(
            5..5,
            [
                command(When::Done, Kind::Fade),
                command(When::Done, Kind::Action(QUEUED)),
            ],
        );
        [Vec::new(), Vec::new(), one, two]
    }

    #[test]
    fn a_request_with_a_target_queues_the_action_and_takes_list_one_or_two() {
        let mut lists = built_in();
        lists[0] = vec![
            command(When::Any, Kind::Hold),
            command(When::Any, Kind::End),
        ];
        lists[1] = vec![command(When::Any, Kind::End)];
        let mut runner = actor();
        runner.queued_next = Some(7);
        runner.request_target(2, false, true, &lists);
        assert!(!runner.free() && !runner.ready() && runner.told);
        assert_eq!((runner.queued, runner.queued_next), (Some(2), None));
        assert_eq!(frame(&mut runner), [Kind::Hold]);
        assert!(runner.ready());
        // With `turns` the list is the second one, which here ends at once.
        let mut runner = actor();
        runner.request_target(1, true, false, &lists);
        assert_eq!(frame(&mut runner), [Kind::End]);
        assert_eq!(runner.queued, Some(1));
        assert!(!runner.told);
    }

    #[test]
    fn a_request_with_a_list_runs_it_and_tells_at_its_move_over() {
        // The shape of the lists a script hands over: a hold, an action, its end, the
        // word that the move is over.
        let list = vec![
            command(When::Any, Kind::Hold),
            command(When::Done, Kind::Action(1)),
            command(When::Done, Kind::Show(false)),
            command(When::Any, Kind::MoveOver),
            command(When::Any, Kind::End),
        ];
        let mut runner = actor();
        runner.queued = Some(2);
        runner.request_list(list, true);
        assert!(!runner.free() && !runner.ready());
        assert_eq!(frame(&mut runner), [Kind::Hold]);
        assert!(runner.ready() && !runner.take_over());
        runner.goes_on();
        let mut frames = 0;
        while !runner.take_over() {
            frame(&mut runner);
            frames += 1;
            assert!(frames < 20, "the move is never over");
        }
        // Action 1 runs from frame 0 to frame 3; what was queued before is left alone.
        assert_eq!((runner.action, frames, runner.hidden), (Some(1), 4, true));
        assert_eq!(runner.queued, Some(2));
        assert!(runner.free() && !runner.told);
    }

    #[test]
    fn a_request_plays_its_actions_and_tells_when_the_first_is_through() {
        let mut runner = actor();
        assert!(runner.free() && runner.ready());
        let request = Request::of_order(1, Some(2), order::FIRST_FRAME, true);
        runner.request(&request, &built_in());
        assert!(!runner.free() && !runner.ready());
        // The first pass comes to the hold, where the actor is ready and waits.
        assert_eq!(frame(&mut runner), [Kind::Hold]);
        assert!(runner.ready() && !runner.take_over());
        assert_eq!(frame(&mut runner), [Kind::Hold]);
        runner.goes_on();
        // The first action plays to its end; then the one who asked is told, once.
        let mut frames = 0;
        while !runner.take_over() {
            frame(&mut runner);
            frames += 1;
            assert!(frames < 20, "the move is never over");
        }
        // Action 1 runs from frame 0 to frame 3. The pass that finds it through ends the
        // move and starts the second action, of which nobody is told.
        assert_eq!((runner.action, frames), (Some(2), 4));
        assert!(runner.free() && !runner.told && !runner.take_over());
        assert_eq!(runner.queued, None);
        assert!(runner.playback.fade.is_some());
        (0..10).for_each(|_| drop(frame(&mut runner)));
        assert!(runner.free() && !runner.take_over());

        // Without the word nobody is told, and bit 4 of the order takes the fade away.
        let mut runner = actor();
        runner.request(
            &Request::of_order(1, None, order::NO_FADE, false),
            &built_in(),
        );
        frame(&mut runner);
        runner.goes_on();
        (0..3).for_each(|_| drop(frame(&mut runner)));
        assert_eq!((runner.action, runner.playback.fade), (Some(1), None));
        (0..10).for_each(|_| drop(frame(&mut runner)));
        assert!(runner.free() && !runner.take_over());

        // A move that ends at "go on" is over there, with the action still to come.
        let mut runner = actor();
        let request = Request::of_order(1, None, order::ENDS_AT_GO_ON, true);
        runner.request(&request, &built_in());
        frame(&mut runner);
        runner.goes_on();
        assert!(runner.free() && runner.take_over());
        (0..3).for_each(|_| drop(frame(&mut runner)));
        assert_eq!(runner.action, Some(1));
    }

    /// The list a creature appears with, but for the action it names: hold, show, out of
    /// the black while one loop of the action plays, then all of that off.
    fn appearing() -> Vec<Command> {
        vec![
            command(When::Any, Kind::Hold),
            command(When::Done, Kind::Show(true)),
            command(When::Done, Kind::ShareFalls(0x100)),
            command(When::Done, Kind::Loops(1)),
            command(When::Done, Kind::FromLoopStart),
            command(When::Done, Kind::Action(1)),
            command(When::Any, Kind::Mark(SHARE_MARK)),
            command(When::Done, Kind::ShareOff),
            command(When::Any, Kind::MoveOver),
            command(When::Any, Kind::End),
        ]
    }

    #[test]
    fn the_time_of_a_list_is_counted_without_running_it() {
        let list = appearing();
        let to = |list: &[Command], until| time(list, &actions(), until, 4, WHOLE as i16);
        let mark = Until::Mark(SHARE_MARK);
        // Action 1 from its loop start: frames 1 to 3, the first of them not counted.
        assert_eq!(to(&list[2..], mark), Some(2));
        assert_eq!(to(&list, mark), Some(2));
        // From its first frame the way to the loop start comes on top.
        assert_eq!(to(&list[5..], mark), Some(3));
        // Three loops, and the loops of the request, which are four here.
        let mut longer = list.clone();
        longer[3].kind = Kind::Loops(3);
        assert_eq!(to(&longer, mark), Some(8));
        longer[3].kind = Kind::RequestLoops;
        assert_eq!(to(&longer, mark), Some(11));
        // A wait counts, and a hold puts the time back to none.
        longer[4].kind = Kind::Wait(30);
        assert_eq!(to(&longer, mark), Some(4 * 3 + 1 - 1 + 30));
        longer[6].kind = Kind::Hold;
        assert_eq!(to(&longer, Until::MoveOver), Some(0));
        // Another mark is not on the list: the walk stops at the end of the move, and
        // without one at the end of the list.
        assert_eq!(to(&list, Until::Mark(SIZE_MARK)), Some(2));
        assert_eq!(to(&list[9..], mark), None);
        assert_eq!(to(&[], mark), None);
        // A frame of an animation: action 1 plays animation 11 and reaches frame 2, not 4.
        let frame = |frame| Until::Frame { action: 1, frame };
        assert_eq!(to(&list, frame(2)), Some(1));
        assert_eq!(to(&list, frame(4)), Some(2 + 1));
        assert_eq!(
            to(
                &list,
                Until::Frame {
                    action: 0,
                    frame: 2
                }
            ),
            Some(2)
        );
        // An action the table does not have.
        longer[5].kind = Kind::Action(9);
        assert_eq!(to(&longer, mark), None);
        // An actor of half the speed takes twice the time; the console's division by none.
        let slow = time(&list, &actions(), mark, 0, WHOLE as i16 / 2);
        assert_eq!(slow, Some(4));
        assert_eq!(time(&list, &actions(), mark, 0, 0), Some(-1));
    }

    #[test]
    fn the_share_of_black_moves_over_the_time_to_its_mark() {
        const BLEND_ONE: u32 = word::BLENDED | 1 << word::MODE | word::FADED;
        let mut runner = actor();
        runner.set_list(appearing());
        runner.go_on = true;
        let mut words = Vec::new();
        while runner.moving || words.is_empty() {
            runner.moving |= words.is_empty();
            runner.pass(&actions());
            runner.update();
            words.push(runner.word);
            runner.step();
            assert!(words.len() < 10, "the move does not end");
        }
        // Two game frames to the mark: half the black, none, and then nothing forced.
        assert_eq!(words, [2048 << 16 | BLEND_ONE, BLEND_ONE, 0]);
        assert_eq!(fade_of(words[0]), 2048);
        assert_eq!(blend_of(words[0]), Some(1));
        assert_eq!((fade_of(0), blend_of(0)), (0, None));
        assert_eq!(runner.flags, 0);

        // Into the black, without a mark or an end of the move behind it: the game's time
        // is -1 and the share never leaves its start.
        let mut runner = actor();
        runner.set_list(vec![
            command(When::Any, Kind::ShareRises(0)),
            command(When::Any, Kind::End),
        ]);
        for _ in 0..3 {
            runner.pass(&actions());
            runner.update();
            assert_eq!(runner.word, word::FADED);
        }
    }

    #[test]
    fn the_size_moves_to_its_goal_and_the_pulse_swings() {
        let mut runner = actor();
        runner.set_list(vec![
            command(When::Any, Kind::Size(2048)),
            command(When::Any, Kind::Action(1)),
            command(When::Any, Kind::Mark(SIZE_MARK)),
            command(When::Frame(3), Kind::RequestSize),
            command(
                When::Any,
                Kind::Pulse {
                    step: 1024,
                    flags: 0x80,
                },
            ),
            command(When::Any, Kind::Mark(SIZE_MARK)),
            command(When::Any, Kind::MoveOver),
            command(When::Any, Kind::End),
        ]);
        runner.request_size = OWN_SIZE;
        // Three game frames to the mark: a third of the way in each. The rest of the
        // division would be made up in a fourth.
        let mut sizes = Vec::new();
        for _ in 0..3 {
            runner.pass(&actions());
            runner.update();
            sizes.push(runner.size);
            runner.step();
        }
        assert_eq!(sizes, [3414, 2732, 2050]);
        assert_eq!(runner.flags, flag::SIZE_MOVES);
        // Back to the size of the request with no time to the mark: the console's division
        // by none gives a step of -1, so the size creeps down, away from its goal.
        assert_eq!(runner.playback.counter, 3);
        assert_eq!(runner.pass(&actions()).len(), 5);
        let mut words = Vec::new();
        for _ in 0..3 {
            runner.update();
            words.push(runner.word);
        }
        assert_eq!(runner.size, 2047);
        assert_eq!(runner.scale([4096, 8192, -4096]), [2047, 4094, -2047]);
        // The pulse: the colours at twice their own, at one and a half, at their own.
        let blend = word::BLENDED | word::FADED;
        assert_eq!(words, [0xF000 << 16 | blend, 0xF800 << 16 | blend, blend]);
        let fades: Vec<i32> = words.iter().map(|word| fade_of(*word)).collect();
        assert_eq!(fades, [-4096, -2048, 0]);
        assert_eq!(blend_of(words[0]), Some(0));
    }

    #[test]
    fn a_bone_joins_its_flags_with_the_word_of_the_model() {
        // A model that is drawn blended in mode 1 with half the black.
        let model = 2048 << 16 | word::FADED | word::BLENDED | 1 << word::MODE;
        // A lit bone takes the model's mode; the shadow's bone keeps its own, mode 2.
        assert_eq!(bone_word(model, 0x0B), model | 0x0B);
        assert_eq!(bone_word(model, 0x2B), model | 0x0B);
        let shadow = bone_word(model, 0x46);
        assert_eq!(shadow, 2048 << 16 | 0xD6);
        assert_eq!((blend_of(shadow), fade_of(shadow)), (Some(2), 2048));
        // A model that is not blended leaves a bone its mode.
        assert_eq!(bone_word(0, 0x46), 0x46);
        assert_eq!(bone_word(word::FADED | 0x60, 0x2B), word::FADED | 0x2B);
        // The palette rows count only for a bone with bit 3.
        assert_eq!(bone_word(0x1200, 0x0B), 0x120B);
        assert_eq!(bone_word(0x1200, 0x46), 0x46);
    }

    #[test]
    fn a_tinted_actor_has_the_rows_to_its_palette_in_the_word() {
        let mut runner = actor();
        runner.update();
        assert_eq!(runner.word, 0);
        // Nine rows down, as for the first actor of a room with its model in slot 0.
        runner.tint(9);
        runner.update();
        assert_eq!((runner.flags, runner.word), (flag::TINTED, 9 << 8));
        // A bone that allows the rows draws its faces with them, another without.
        assert_eq!(bone_word(runner.word, 0x0B) >> 8, 9);
        assert_eq!(bone_word(runner.word, 0x46) >> 8, 0);
        assert_eq!(runner.copied().word, 0);
    }

    #[test]
    fn what_is_not_read_stops_a_list_and_says_so() {
        let mut runner = actor();
        runner.set_list(vec![
            command(When::Any, Kind::GoToTarget),
            command(When::Any, Kind::End),
        ]);
        assert_eq!(frame(&mut runner), [Kind::GoToTarget]);
        assert_eq!(runner.stuck, Some(Stuck::Target(Kind::GoToTarget)));
        assert!(frame(&mut runner).is_empty());

        // An action the model does not have, and a queued action when none is queued.
        for (action, missing) in [(9, Some(9)), (QUEUED, None)] {
            let mut runner = actor();
            runner.set_list(vec![
                command(When::Any, Kind::Action(action)),
                command(When::Any, Kind::End),
            ]);
            assert_eq!(frame(&mut runner), [Kind::Action(action)]);
            assert_eq!(runner.stuck, Some(Stuck::NoAction(missing)));
            assert_eq!(runner.playback.animation, 10, "the old action plays on");
        }
    }

    #[test]
    fn an_actor_on_a_path_plays_its_action_round_until_the_path_is_through() {
        use crate::mover::{Path, PathPoint};
        let arc = ArcTangent::of(vec![0; 1025]);
        let point = PathPoint {
            place: [0; 3],
            turn: 0,
        };
        let mut mover = Mover::new([0, 0, 0], 0);
        mover.set_path(Path {
            divisor: 1,
            points: vec![point; 12],
        });
        let mut runner = actor();
        let bits = order::NO_FADE | order::AT_PATH_END;
        runner.request(&Request::of_path_order(1, Some(2), bits, true), &built_in());
        // One game frame as the room makes it: the pass, the step, the path's step.
        let frame = |runner: &mut Runner, mover: &mut Mover| {
            let feet = Feet {
                mover,
                arc: &arc,
                counter: 0,
                someone_near: false,
            };
            let ran = runner.pass_with(&actions(), Some(feet));
            runner.step();
            if mover.flags & mover::flag::PATH != 0 {
                let through = mover.path_step();
                runner.path_step(through);
            }
            ran
        };

        assert_eq!(frame(&mut runner, &mut mover), [Kind::Hold]);
        runner.goes_on();
        // The fade command does nothing on a path and leaves the request's bit.
        assert_eq!(
            frame(&mut runner, &mut mover),
            [Kind::Hold, Kind::Fade, Kind::Action(QUEUED)]
        );
        assert!(runner.no_fade && runner.playback.fade.is_none());
        // Action 1 runs from frame 0 to 3 and loops from 1: it goes round while the
        // path runs, and its end is not taken for the end of the move.
        let mut counters = Vec::new();
        for _ in 2..11 {
            assert_eq!(frame(&mut runner, &mut mover), [Kind::WaitForEnd]);
            counters.push(runner.playback.counter);
        }
        assert_eq!(counters, [2, 3, 1, 2, 3, 1, 2, 3, 1]);
        assert_eq!(runner.action, Some(1));
        assert!(!runner.take_over());
        // The last point: the action stands on its end frame, the path is left.
        assert_eq!(frame(&mut runner, &mut mover), [Kind::WaitForEnd]);
        assert_eq!((runner.playback.counter, mover.flags), (3, 8));
        assert!(runner.quick_fade && !runner.no_fade);
        // Now the move is over and the second action fades in, a fifteenth a game frame.
        assert_eq!(
            frame(&mut runner, &mut mover),
            [
                Kind::WaitForEnd,
                Kind::MoveOver,
                Kind::Fade,
                Kind::Action(QUEUED)
            ]
        );
        assert!(runner.take_over());
        assert_eq!(runner.action, Some(2));
        assert!(runner.playback.fade.is_some_and(|fade| fade.quick));
        assert!(!runner.quick_fade);

        // Without bit 2 the end of the path leaves the wish for no fade as it is.
        let mut runner = actor();
        runner.request(
            &Request::of_path_order(1, Some(2), order::NO_FADE, true),
            &built_in(),
        );
        runner.path_step(true);
        assert!(runner.no_fade && !runner.quick_fade);
    }

    #[test]
    fn an_actor_with_feet_turns_and_goes_to_its_target() {
        // The place below lies a quarter circle to the side whatever the table holds.
        let arc = ArcTangent::of(vec![0; 1025]);
        let mut mover = Mover::new([0, 0, 0], 0);
        (mover.target, mover.turn_step) = ([500, 0, 0], 512);
        mover.flags |= mover::flag::TARGET;
        let mut runner = actor();
        runner.set_list(vec![
            command(When::Any, Kind::TurnToTarget),
            command(When::Any, Kind::GoToTarget),
            command(When::Any, Kind::Hold),
            command(When::Any, Kind::End),
        ]);
        let pass = |runner: &mut Runner, mover: &mut Mover, counter: u32| {
            let feet = Feet {
                mover,
                arc: &arc,
                counter,
                someone_near: false,
            };
            let ran = runner.pass_with(&actions(), Some(feet));
            runner.step();
            ran
        };

        // The turn runs through the filter and holds the step back while it lasts.
        assert_eq!(pass(&mut runner, &mut mover, 0), [Kind::TurnToTarget]);
        assert_eq!((mover.turn, mover.flags & mover::flag::NO_STEP), (64, 8));
        assert!(runner.done && runner.stuck.is_none());
        mover.turn = 600;
        // Facing it: the target is taken off, and the walk starts in the same pass.
        mover.position = [0, 0, 0];
        mover.before = [0, 0, -10];
        let ran = pass(&mut runner, &mut mover, 1);
        assert_eq!(ran, [Kind::TurnToTarget, Kind::GoToTarget]);
        assert_eq!((mover.turn, mover.flags & mover::flag::TARGET), (1024, 0));
        // Nearer than the last step: there. The hold has no target left to turn to.
        mover.position = [495, 0, 0];
        mover.before = [495, 0, -10];
        assert_eq!(
            pass(&mut runner, &mut mover, 2),
            [Kind::GoToTarget, Kind::Hold]
        );
        assert!(!runner.not_ready && runner.done);

        // A hold with a target turns before it holds.
        let mut mover = Mover::new([0, 0, 0], 0);
        (mover.target, mover.turn_step) = ([500, 0, 0], 512);
        mover.flags |= mover::flag::TURNS;
        let mut runner = actor();
        runner.not_ready = true;
        runner.set_list(vec![
            command(When::Any, Kind::Hold),
            command(When::Any, Kind::End),
        ]);
        assert_eq!(pass(&mut runner, &mut mover, 0), [Kind::Hold]);
        assert!(runner.not_ready);
        mover.turn = 1000;
        assert_eq!(pass(&mut runner, &mut mover, 1), [Kind::Hold]);
        assert!(!runner.not_ready);
        assert_eq!(mover.turn, 1024);
    }

    #[test]
    fn a_confirm_puts_the_action_on_its_end_unless_the_request_forbids_it() {
        let span = actions()[0].span();
        let mut runner = Runner::new(Playback::new(10, span, WHOLE as i16), 0, 4);
        runner.step();
        assert_ne!(runner.playback.counter, i32::from(span.end));
        runner.confirmed();
        assert_eq!(runner.playback.counter, i32::from(span.end));
        assert!(runner.at_end);

        let mut runner = Runner::new(Playback::new(10, span, WHOLE as i16), 0, 4);
        runner.keeps_at_confirm = true;
        let before = runner.clone();
        runner.confirmed();
        assert_eq!(runner, before);
        // The value 16 of an order is that wish.
        assert!(Request::of_order(1, None, order::KEEPS_AT_CONFIRM, false).keeps_at_confirm);
        assert!(!Request::of_order(1, None, order::FIRST_FRAME, false).keeps_at_confirm);
    }
}
