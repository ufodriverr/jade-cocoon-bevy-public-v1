
use crate::action::Action;
use crate::animation::Animation;
use crate::command::quotient;
use crate::field::{Fault, Foothold, Ground, Position};
use crate::mover::{ArcTangent, Filter, Path};
use crate::skeleton::{cosine, sine};

pub mod arms;

/// Where the weights of the player's turn filter lie in the executable: four of
/// [`EVEN_WEIGHT`].
pub const TURN_WEIGHTS: u32 = 0x8007_94E4;
const EVEN_WEIGHT: i32 = 1024;
/// The turn a held button or a slide along a wall asks for in a game frame.
pub const TURN: i32 = 64;
/// The most the stick turns the player in a game frame.
pub const STICK_TURN: i32 = 256;
/// What a fade moves on by in a game frame, of [`WHOLE`].
const FADE_STEP: i16 = 1024;
/// Game frames without a command after which the standing player changes its action.
pub const IDLE: i16 = 90;
/// The squared length of the stick from which the player runs, and from which it walks.
const STICK_RUN: i32 = 10953;
const STICK_WALK: i32 = 2739;
/// The divisor of a carried player's height, whatever the animation's own is.
const HEIGHT_DIVISOR: i32 = 32;
const WHOLE: i32 = 4096;

/// The bits of the command word (+4 of the controller record).
pub mod command {
    /// The low bits name the movement: one of the four values below, or none.
    pub const MOVEMENT: u32 = 0xF_FFFF;
    pub const RUN: u32 = 1;
    pub const BACK: u32 = 2;
    /// Start the ordered action.
    pub const ORDERED: u32 = 3;
    pub const WALK: u32 = 4;
    /// Turn, the angle falls.
    pub const FALLS: u32 = 0x0400_0000;
    /// Turn, the angle rises.
    pub const RISES: u32 = 0x0800_0000;
    /// Half the turn.
    pub const HALF: u32 = 0x1000_0000;
    /// Turn to the direction of the stick: either of two bits, which the turn treats alike.
    pub const STICK: u32 = 0x6000_0000;
    /// The stick bit [`super::stick_command`] sets. The game picks one of the two by the
    /// side the wanted direction lies on.
    pub const STICK_ONE: u32 = 0x2000_0000;
}

/// The bits of the controller's flag word that walking through a room reads and writes.
pub mod flag {
    /// A wanted action of a fight is not played out yet.
    pub const BUSY: u32 = 1;
    /// Going into the black, and coming out of it.
    pub const INTO_BLACK: u32 = 2;
    pub const OUT_OF_BLACK: u32 = 4;
    /// No step in this game frame. The step clears it.
    pub const NO_STEP: u32 = 8;
    /// "Go on": the ordered action that stands ready starts.
    pub const GO_ON: u32 = 0x20;
    /// The action before is still fading out.
    pub const FADE: u32 = 0x10;
    /// The free player leaves for the weapon states: a fight is on.
    pub const ARMS: u32 = 0x100;
    /// The wanted action of a fight is his own command and not a reaction.
    pub const OWN: u32 = 0x200;
    /// After a defence (kind 18 of a command) he stays in its action.
    pub const GUARD: u32 = 0x400;
    /// The action turns back at its hold frame.
    pub const HOLD: u32 = 0x800;
    /// A pose follows the weapon put away behind a fight.
    pub const POSE: u32 = 0x1000;
    /// The states of a fight are through and the player is free again.
    pub const THROUGH: u32 = 0x2000;
    /// The player was put on a base place: carried, and the free player decides nothing.
    pub const PUT: u32 = 0x4000;
    /// The action came to its end, or to its hold frame, and went to its loop start.
    pub const LOOPED: u32 = 0x8000;
    /// After kind 16 of a command he stays in its action.
    pub const STANCE: u32 = 0x1_0000;
    /// Not drawn.
    pub const HIDDEN: u32 = 0x2_0000;
    /// No walk over the ground.
    pub const NO_GROUND: u32 = 0x4_0000;
    /// Carried by the animation from the base place.
    pub const CARRIED: u32 = 0x8_0000;
    /// An ordered action starts at its loop start instead of its first frame.
    pub const FROM_LOOP_START: u32 = 0x10_0000;
    /// The ordered action is sent to where it turns back: its loop start while it is
    /// held, else its end frame. Set by a confirm that a message window of a script
    /// took while the script is busy with the player ([`super::Player::confirmed`]).
    pub const SENT_ON: u32 = 0x80_0000;
    /// The walk plays for a turn on the spot: no step.
    pub const TURN_ONLY: u32 = 0x20_0000;
    /// On a path.
    pub const PATH: u32 = 0x40_0000;
    /// A pose follows the move of a fight in place of the weapon drawn again.
    pub const POSE_BEHIND: u32 = 0x0100_0000;
}

/// The states of the controller that the free player sets.
pub mod state {
    pub const STANDS: i32 = 0;
    pub const WAITS: i32 = 1;
    pub const WALKS: i32 = 2;
    pub const BACKS: i32 = 4;
    pub const RUNS: i32 = 5;
    /// The first of the weapon states, in mode 1.
    pub const ARMS: i32 = 7;
    /// Ready for the ordered action, in modes 2 and 3.
    pub const ORDERED: i32 = 23;
    /// The ordered action plays.
    pub const PLAYS: i32 = 24;
}

/// The actions of the player's files that the free player plays.
pub mod act {
    pub const WALK: i16 = 0;
    pub const BACK: i16 = 1;
    pub const RUN: i16 = 2;
    pub const STAND: i16 = 3;
    /// What the standing player changes to after [`super::IDLE`] game frames.
    pub const WAIT: i16 = 4;
    /// An action that never makes a step.
    pub const STILL: i16 = 29;
}

/// A product of two values in 4096ths, as the controller makes it: divided towards zero.
fn part(product: i32) -> i32 {
    product / WHOLE
}

/// The command word of a stick: the two signed values of the pad record.
pub fn stick_command(stick: [i8; 2]) -> u32 {
    let [x, y] = stick.map(i32::from);
    match x * x + y * y {
        length if length >= STICK_RUN => command::RUN | command::STICK_ONE,
        length if length >= STICK_WALK => command::WALK | command::STICK_ONE,
        _ => 0,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Held {
    /// Mask +0.
    pub forward: bool,
    /// Mask +10: with `forward`, run.
    pub fast: bool,
    /// Mask +16, or the value 4 in the ninth byte of the pad record: run on its own.
    pub run: bool,
    /// Mask +2.
    pub back: bool,
    /// Mask +4: the angle falls.
    pub falls: bool,
    /// Mask +6: the angle rises.
    pub rises: bool,
}

/// The command word of held buttons.
pub fn button_command(held: Held) -> u32 {
    let movement = match held {
        Held { run: true, .. } => command::RUN,
        Held {
            forward: true,
            fast,
            ..
        } => match fast {
            true => command::RUN,
            false => command::WALK,
        },
        Held { back: true, .. } => command::BACK,
        _ => 0,
    };
    let turn = match (held.falls, held.rises) {
        (true, _) => command::FALLS,
        (false, true) => command::RISES,
        (false, false) => 0,
    };
    movement | turn
}

/// The squared distance, over all three axes, up to which a thing of the room is near the
/// player: nearer than 200.
pub const NEAR: i32 = 39999;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thing {
    /// The actor's position (+0x38 of the actor).
    pub position: Position,
    /// Flag 0x2000 of the actor: a hidden one is not asked.
    pub hidden: bool,
}

/// What the two routines `0x800A9244` and `0x800A934C` answer about the things of a room.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InTheWay {
    /// The first near thing in the half ahead: the player's angle less the direction to
    /// it, between -1023 and 1023.
    pub ahead: Option<i16>,
    /// A near thing lies in the half behind.
    pub behind: bool,
}

impl InTheWay {
    /// The things of the list against the player's place and angle. A thing exactly to one
    /// side (1024 off) is neither ahead nor behind.
    pub fn of(arc: &ArcTangent, place: Position, angle: i16, things: &[Thing]) -> Self {
        // The player's angle less the direction to a near thing, brought into a full turn
        // from -2048 on (`0x8003BA1C`).
        let off = |thing: &Thing| {
            let squared = (0..3).fold(0i32, |sum, axis| {
                let apart = place[axis].wrapping_sub(thing.position[axis]);
                sum.wrapping_add(apart.wrapping_mul(apart))
            });
            if thing.hidden || squared > NEAR {
                return None;
            }
            let to = arc.angle(
                thing.position[0].wrapping_sub(place[0]),
                thing.position[2].wrapping_sub(place[2]),
            );
            match i32::from(angle.wrapping_sub(to)) & (WHOLE - 1) {
                off if off < WHOLE / 2 => Some(off as i16),
                off => Some((off - WHOLE) as i16),
            }
        };
        let near = || things.iter().filter_map(off);
        Self {
            ahead: near().find(|off| (-1023..=1023).contains(off)),
            behind: near().any(|off| !(-1024..=1024).contains(&off)),
        }
    }

    /// The command word the routine of the pad hands on: a run or a walk towards a thing
    /// becomes half a turn away from it without a movement, and a step back onto a thing
    /// is dropped. The turn bits of the buttons and the stick bit stay as they are.
    pub fn turned_away(&self, command: u32) -> u32 {
        let rest = command & !command::MOVEMENT;
        match (command & command::MOVEMENT, self.ahead) {
            (command::RUN | command::WALK, Some(off)) if off < 0 => {
                rest | command::HALF | command::FALLS
            }
            (command::RUN | command::WALK, Some(_)) => rest | command::HALF | command::RISES,
            (command::BACK, _) if self.behind => rest,
            _ => command,
        }
    }
}

pub const TALK_NEAR: i64 = 89_999;
/// How far the direction to a thing may lie off the player's angle, to either side, for
/// the player to talk to it: about 36 degrees.
pub const TALK_FACED: i32 = 408;

/// Whether a thing is near enough for a talk (the distance test of script command `0x62`
/// without a region). The height counts.
pub fn talk_near(place: Position, thing: Position) -> bool {
    let squared: i64 = (0..3)
        .map(|axis| i64::from(place[axis]) - i64::from(thing[axis]))
        .map(|apart| apart * apart)
        .sum();
    squared <= TALK_NEAR
}

/// Whether the player faces a thing for a talk: the direction to it (`0x8003B9E4`) taken
/// from the player's angle and brought into -2048 to 2047 (`0x8003BA1C`) is at most
/// [`TALK_FACED`] to either side.
pub fn talk_faced(arc: &ArcTangent, place: Position, angle: i16, thing: Position) -> bool {
    let to = arc.angle(
        thing[0].wrapping_sub(place[0]),
        thing[2].wrapping_sub(place[2]),
    );
    let off = match i32::from(angle.wrapping_sub(to)) & (WHOLE - 1) {
        off if off < WHOLE / 2 => off,
        off => off - WHOLE,
    };
    (-TALK_FACED..=TALK_FACED).contains(&off)
}

/// The direction a view's camera looks in (`0x8003B9E4`), which the stick is turned by.
pub fn camera_direction(arc: &ArcTangent, camera: Position, look_at: Position) -> i16 {
    arc.angle(
        look_at[0].wrapping_sub(camera[0]),
        look_at[2].wrapping_sub(camera[2]),
    )
}

/// What the controller reads of one action of the player's file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Move {
    pub first: i16,
    pub loop_start: i16,
    pub hold: i16,
    pub end: i16,
    /// The first frame of the first of the action's ten pairs.
    pub window: i16,
    /// The byte at +3 of the long record: 1 for an action without the weapon in the hand.
    pub bare: u8,
    /// The extra word of the action's animation, 32 where the block has none.
    pub divisor: i32,
    /// The travel of every frame of the animation; empty while it is not in memory.
    pub travel: Vec<[i16; 3]>,
}

impl Move {
    /// An action with its animation, or without it while that is not in memory.
    pub fn of(action: &Action, animation: Option<&Animation>) -> Self {
        let frames = animation.map_or(0, |animation| animation.frames);
        let travel = |frame| animation.and_then(|animation| animation.travel(frame));
        Self {
            first: action.first,
            loop_start: action.loop_start,
            hold: action.hold,
            end: action.end,
            window: action.windows[0][0],
            bare: action.extra[1],
            divisor: (animation.and_then(|animation| animation.extra)).map_or(32, |by| by as i32),
            travel: (0..frames).map_while(travel).collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Orders {
    /// The order state: 0 while the player is free, 9 taken and ready for an order, 10
    /// to 12 while it goes, runs or turns to a place, 13 while an ordered action plays,
    /// 14 while a held action runs out.
    pub state: i16,
    /// A script has the player.
    pub taken: bool,
    /// The place of an order to go or turn somewhere (+164).
    pub place: Position,
    /// Flag 0x80: the script has given the player back, which ends such an order where
    /// the player is. "Go on" takes it off.
    pub ended: bool,
    /// The command word the order state has made for this game frame in place of the
    /// pad's ([`Orders::pass`]); none in the states that leave the pad's alone.
    pub command: Option<u32>,
    /// What a fight has written into the record.
    pub fight: Fought,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Fought {
    /// Bit 0: a fight holds the player, so order state 1 waits (`0x800A8A3C` sets it
    /// when the field engine starts a fight, the way back clears it).
    pub held: bool,
    /// Bit 1: a move was asked for and is not at its hold yet.
    pub asked: bool,
    /// Bit 2: "go" (`0x800A8CEC`).
    pub go: bool,
    /// Bit 3: the move is his own command and not a reaction.
    pub own: bool,
    /// Bit 4: order state 7 has kept his place.
    pub on: bool,
    /// The action (+204), the target mask (+52) and the parameter (+56) of the move.
    pub action: i16,
    pub targets: u32,
    pub parameter: u32,
}

/// The order states of [`Orders::state`].
pub mod order_state {
    /// A fight: waits while the fight holds him.
    pub const WAITS: i16 = 1;
    /// A fight: a move was asked for; the controller takes it when it can.
    pub const ASKED: i16 = 2;
    /// A fight: turns to the place until the move is at its hold.
    pub const TURNS_TO_HOLD: i16 = 3;
    /// A fight: ready, waits for "go".
    pub const READY: i16 = 4;
    /// A fight: turns to the place for a kind that plays nothing.
    pub const TURNS_FOR_NOTHING: i16 = 5;
    /// A fight: let go; his own command waits until it is played out.
    pub const WENT: i16 = 6;
    /// A fight starts: the place is kept.
    pub const KEEPS: i16 = 7;
    /// A fight: turns to the place before a command.
    pub const FACES: i16 = 8;
    pub const TAKEN: i16 = 9;
    pub const GOES: i16 = 10;
    pub const RUNS: i16 = 11;
    pub const TURNS: i16 = 12;
    pub const PLAYS: i16 = 13;
    pub const RUNS_OUT: i16 = 14;
}

/// The limit the order states hand the distance routine `0x8003B744`: further apart than
/// this on one axis the routine answers -1 for the squared distance.
pub const GO_LIMIT: i32 = 30000;
/// A player whose squared distance to the place of a go order is below this is there.
pub const GO_THERE: i32 = 400;
/// How far off the direction to the place the player's angle may be, to either side,
/// before an order state turns it.
pub const GO_FACED: i32 = 64;
/// The same for the order states 3 and 5 of a fight, which turn him while a move is
/// brought to its hold.
pub const HOLD_FACED: i32 = 96;

/// The bits of an action order that the player's side reads (the third argument of the
/// script commands `0x48` and `0x49`).
pub mod order_bit {
    /// Start at the first frame, not at the loop start.
    pub const FIRST_FRAME: u32 = 1;
    /// No fade from the action before.
    pub const NO_FADE: u32 = 4;
    /// The script is told at "go on", not when the action has turned back.
    pub const ENDS_AT_GO_ON: u32 = 8;
}

impl Orders {
    /// A script takes the player (`0x800A8EEC`).
    pub fn take(&mut self) {
        (self.state, self.taken) = (order_state::TAKEN, true);
    }

    /// A script gives the player back (`0x800A8F18` with `0x800ABCA4`): path, carrying
    /// and hold go, there is no step in the next game frame, and the player stands in
    /// mode 0. The order state stays 9 until [`Orders::pass`] sees the player stand.
    pub fn give_back(&mut self, player: &mut Player, moves: &[Move]) {
        (self.taken, self.ended) = (false, true);
        player.flags &= !(flag::PATH | flag::CARRIED | flag::HOLD);
        player.flags |= flag::NO_STEP;
        player.start(moves, state::STANDS, act::STAND, 0);
        player.mode = 0;
    }

    /// Free for an order and ready to go on are one test (`0x800A8F5C`, `0x800A90C0`).
    pub fn ready(&self) -> bool {
        self.state == order_state::TAKEN
    }

    /// An order is given (`0x800A8FA4`). Kinds 9 and 10, an action and two actions, are
    /// in code: the action is ordered of the controller ([`Player::order`]). The kinds
    /// that only count at "go on" (4, 6, 7, 8) do nothing here. A path (kind 11) is
    /// [`Orders::give_path`]; a place to go or turn to (kinds 0 to 2) is
    /// [`Orders::give_place`]. False for an order that cannot be given.
    pub fn give(
        &mut self,
        player: &mut Player,
        moves: &[Move],
        order: (u32, i16, i16, u32),
    ) -> bool {
        let (kind, action, second, bits) = order;
        match kind {
            9 | 10 => {
                player.from_loop_start(bits & order_bit::FIRST_FRAME == 0);
                let next = if kind == 10 { second } else { -1 };
                let given = player.order(moves, action, next, bits & order_bit::NO_FADE == 0);
                self.state = order_state::TAKEN;
                given
            }
            3..=8 | 12 | 13 => true,
            _ => false,
        }
    }

    /// An order of kind 0, 1 or 2 is given (`0x800A8FA4`, the entry at `0x800A8FF0`): the
    /// place is kept, and nothing else happens before "go on". The order's action is
    /// not read: the player goes with the pad's walk or run.
    pub fn give_place(&mut self, place: Position) {
        self.place = place;
    }

    /// An order of kind 11 is given (`0x800A8FA4`, the entry at `0x800A9068`): the player
    /// is carried from where it stands, gets the path, and the action is ordered from its
    /// loop start with a fade and with none to follow. Neither the order's bits nor its
    /// second action are read here: the end of the path makes the player stand. False,
    /// with nothing done, when the action's animation is not in memory.
    pub fn give_path(
        &mut self,
        player: &mut Player,
        moves: &[Move],
        action: i16,
        path: Path,
    ) -> bool {
        // The game sets the path up first and stops in the order when the animation is
        // missing; here nothing of the path is left behind then.
        if !player.order(moves, action, -1, true) {
            return false;
        }
        player.set_carried(true);
        player.set_path(path);
        player.from_loop_start(true);
        self.state = order_state::TAKEN;
        true
    }

    /// A fight starts (`0x800A8DEC` from the set-up of file 36, with the mean place of
    /// the opponents on the field; the hold is `0x800A8A3C`, which the field engine
    /// calls when it starts the fight): order state 7 with the place.
    pub fn start_fight(&mut self, place: Position) {
        (self.state, self.place) = (order_state::KEEPS, place);
        self.fight.held = true;
    }

    /// The runner turns the player to a place before a command (`0x800A8E28`): order
    /// state 8 with the place. The request that follows in the same game frame
    /// ([`Orders::ask`]) puts its own state over it, and the place stays.
    pub fn face(&mut self, place: Position) {
        (self.state, self.place) = (order_state::FACES, place);
    }

    /// The player is asked for the move of a command or a reaction (`0x800A8B10`): the
    /// action of the kind with the record's target mask and parameter, or none for a
    /// kind that plays nothing, which only turns him (order state 5) and leaves the
    /// bits of the order word alone.
    pub fn ask(&mut self, action: Option<i16>, own: bool, targets: u32, parameter: u32) {
        let Some(action) = action else {
            self.state = order_state::TURNS_FOR_NOTHING;
            return;
        };
        self.state = order_state::ASKED;
        let fight = &mut self.fight;
        (fight.action, fight.targets, fight.parameter) = (action, targets, parameter);
        (fight.asked, fight.go, fight.own) = (true, false, own);
    }

    /// The runner's second test for the player (`0x800A8CC4`): order state 4.
    pub fn at_the_ready(&self) -> bool {
        self.state == order_state::READY
    }

    /// "Go" (`0x800A8CEC`): bit 2 of the order word, which order state 4 waits for.
    pub fn go(&mut self) {
        self.fight.go = true;
    }

    /// The order record's half of the way back from a fight (`0x800A8A64`): the fight
    /// does not hold him any more, so order state 1 ends. The controller's half is
    /// [`Player::back_to_place`].
    pub fn end_fight(&mut self) {
        (self.fight.held, self.fight.on) = (false, false);
    }

    /// "Go on" (`0x800A90E8`) for an order of this kind, value and bits. True when the
    /// script is told at once that the move is over.
    pub fn go_on(&mut self, player: &mut Player, order: (u32, u32, u32)) -> bool {
        let (kind, value, bits) = order;
        self.ended = false;
        match kind {
            0 => self.state = order_state::GOES,
            1 => self.state = order_state::RUNS,
            2 => self.state = order_state::TURNS,
            4 => {
                self.state = order_state::RUNS_OUT;
                player.let_go();
            }
            6 => player.set_shown(value != 0),
            7 => player.set_walks(value != 0),
            8 => player.set_carried(value != 0),
            9..=11 => {
                let at_once = bits & order_bit::ENDS_AT_GO_ON != 0;
                self.state = match at_once {
                    true => order_state::TAKEN,
                    false => order_state::PLAYS,
                };
                player.go_on(true);
                return at_once;
            }
            _ => {}
        }
        false
    }

    pub fn pass(&mut self, player: &mut Player, arc: &ArcTangent) -> bool {
        self.command = None;
        let off = |player: &Player, place: Position| {
            let to = arc.angle(
                place[0].wrapping_sub(player.position[0]),
                place[2].wrapping_sub(player.position[2]),
            );
            match i32::from(player.angle.wrapping_sub(to)) & (WHOLE - 1) {
                off if off < WHOLE / 2 => off,
                off => off - WHOLE,
            }
        };
        // The side the place lies on: the angle falls towards it or rises.
        let turn_within = |off: i32, faced: i32| match off {
            off if (-faced..=faced).contains(&off) => 0,
            off if off >= 0 => command::FALLS,
            _ => command::RISES,
        };
        let turn = |off: i32| turn_within(off, GO_FACED);
        match self.state {
            0 if self.taken => {
                self.state = order_state::TAKEN;
                false
            }
            order_state::WAITS => {
                if !self.fight.held {
                    self.state = 0;
                }
                false
            }
            order_state::ASKED => {
                let fight = self.fight;
                if player.request(fight.action, fight.own, fight.targets, fight.parameter) {
                    self.state = order_state::TURNS_TO_HOLD;
                }
                false
            }
            order_state::TURNS_TO_HOLD | order_state::TURNS_FOR_NOTHING => {
                let turn = turn_within(off(player, self.place), HOLD_FACED);
                if turn == 0 {
                    if self.state == order_state::TURNS_FOR_NOTHING {
                        self.state = order_state::READY;
                    } else if player.at_the_hold() && self.fight.asked {
                        self.fight.asked = false;
                        self.state = order_state::READY;
                    }
                }
                self.command = Some(turn);
                false
            }
            order_state::READY => {
                if self.fight.go {
                    player.go_on(false);
                    self.fight.go = false;
                    self.state = order_state::WENT;
                }
                false
            }
            order_state::WENT => {
                if !self.fight.own {
                    self.state = order_state::WAITS;
                } else if player.played_out() {
                    self.fight.own = false;
                    self.state = order_state::WAITS;
                }
                false
            }
            order_state::KEEPS | order_state::FACES => {
                if self.state == order_state::KEEPS {
                    self.fight.on = true;
                    player.keep_place();
                }
                let turn = turn(off(player, self.place));
                // State 7 is over in its one game frame, faced or not.
                if turn == 0 || self.state == order_state::KEEPS {
                    self.state = order_state::WAITS;
                }
                self.command = Some(turn);
                false
            }
            order_state::GOES | order_state::RUNS => {
                let squared = squared_within(player.position, self.place, GO_LIMIT);
                let mut told = false;
                let movement = match squared < GO_THERE || self.ended {
                    true => {
                        if player.stands() {
                            self.state = order_state::TAKEN;
                            told = true;
                        }
                        0
                    }
                    false if self.state == order_state::GOES => command::WALK,
                    false => command::RUN,
                };
                self.command = Some(movement | turn(off(player, self.place)));
                told
            }
            order_state::TURNS => {
                let off = off(player, self.place);
                let faced = turn(off) == 0 || self.ended;
                if faced {
                    self.state = order_state::TAKEN;
                }
                self.command = Some(if faced { 0 } else { turn(off) });
                faced
            }
            order_state::TAKEN => {
                if !self.taken && player.stands() {
                    player.set_walks(true);
                    self.state = 0;
                }
                false
            }
            order_state::PLAYS if player.looped() => {
                self.state = order_state::TAKEN;
                true
            }
            order_state::RUNS_OUT if player.stands() => {
                self.state = order_state::TAKEN;
                true
            }
            _ => false,
        }
    }
}

fn squared_within(a: Position, b: Position, limit: i32) -> i32 {
    let apart = [0, 1, 2].map(|axis| a[axis].wrapping_sub(b[axis]).wrapping_abs());
    if apart.iter().any(|apart| *apart > limit) {
        return -1;
    }
    (apart.iter()).fold(0i32, |sum, apart| {
        sum.wrapping_add(apart.wrapping_mul(*apart))
    })
}

/// A frame number of an action as the controller compares it with its counter.
fn mark(frame: i16) -> u32 {
    u32::from(frame as u16)
}

/// The controller record of the player, with the place record.
///
/// Once per game frame: set [`Player::command`] (and the stick and the camera's direction
/// when the command has a stick bit), then [`Player::update`], then the pose and the
/// drawing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Player {
    /// The bits of [`flag`].
    pub flags: u32,
    /// The bits of [`command`] for this game frame. The turn takes its turn bits out.
    pub command: u32,
    pub state: i32,
    /// Which routine runs the state: 0 the free player, 1 the states of a fight
    /// ([`arms`]), 2 an ordered action; 3 is not in code.
    pub mode: i32,
    /// The frame counter of the action, and of the action before.
    pub counter: u32,
    pub counter_before: u32,
    /// The speed of the counter in 4096ths and its fraction.
    pub speed: u16,
    pub fraction: u16,
    pub action: i16,
    pub action_before: i16,
    /// The ordered action that is loaded and the one that is wanted. The first is also
    /// the wanted action of a fight ([`arms`]).
    pub ordered: [i16; 2],
    /// The action that follows the ordered one, below 0 for none.
    pub follows: i16,
    /// How far the fade from the action before has come, 0 to 4096.
    pub fade: i16,
    /// Game frames without a command.
    pub idle: i16,
    pub position: Position,
    /// The angle about the upright axis, 4096 to the full turn.
    pub angle: i16,
    /// The position before this game frame's step.
    pub before: Position,
    /// The base place of a carried player.
    pub base: Position,
    /// The path of a player on one, and the number of its next point.
    pub path: Option<Path>,
    pub path_at: usize,
    /// The travel of the frame last read, less the low bits the step keeps back.
    pub travel: [i32; 3],
    pub foothold: Foothold,
    /// The turn wanted in this game frame.
    pub wanted_turn: i32,
    pub filter: Filter,
    /// The two signed values of the stick, and the direction the camera looks in
    /// ([`camera_direction`]): read only while the command has a stick bit.
    pub stick: [i8; 2],
    pub camera: i16,
    /// Set when the walk over the ground came to a place where the game does not go on.
    pub fault: Option<Fault>,
    /// The group of the weapon in the hand: the actions 14 to 109 are looked up 16 rows
    /// further for each group ([`arms::GROUP_ROWS`]).
    pub group: usize,
    /// What the states of a fight keep ([`arms`]).
    pub fight: arms::Kept,
}

impl Player {
    pub fn new(place: Position, angle: i16, moves: &[Move]) -> Self {
        let mut player = Self {
            flags: flag::NO_STEP,
            command: 0,
            state: state::STANDS,
            mode: 0,
            counter: 0,
            counter_before: 0,
            speed: WHOLE as u16,
            fraction: 0,
            action: act::STAND,
            action_before: act::STAND,
            ordered: [-1; 2],
            follows: -1,
            fade: 0,
            idle: 0,
            position: place,
            angle,
            before: place,
            base: place,
            path: None,
            path_at: 0,
            travel: [0; 3],
            foothold: Foothold::default(),
            wanted_turn: 0,
            filter: Filter::new(vec![EVEN_WEIGHT; 4]),
            stick: [0; 2],
            camera: 0,
            fault: None,
            group: 0,
            fight: arms::Kept::default(),
        };
        player.read_travel(moves);
        player
    }

    pub fn stand_again(&mut self, moves: &[Move]) {
        self.filter = Filter::new(vec![EVEN_WEIGHT; 4]);
        self.start(moves, state::STANDS, act::STAND, 0);
        self.mode = 0;
        if !self.in_memory(moves, self.action_before) {
            self.flags &= !flag::FADE;
        }
        self.flags &= !(flag::PATH | flag::HOLD | flag::CARRIED);
        self.flags |= flag::NO_STEP;
        self.start(moves, state::STANDS, act::STAND, 0);
        self.read_travel(moves);
    }

    pub fn confirmed(&mut self) {
        self.flags |= flag::SENT_ON;
    }

    /// The room loader's placing (`0x80028B54`): the place record gets the arrival of an
    /// exit, and nothing of the controller record is written. The triangle under the player
    /// is looked for again by the next step.
    pub fn arrive(&mut self, place: Position, angle: i16) {
        (self.position, self.angle) = (place, angle);
    }

    /// The row of an action (`0x800AC964`): the actions 14 to 109 are those of the
    /// weapon's group.
    fn played<'a>(&self, moves: &'a [Move], action: i16) -> Option<&'a Move> {
        moves.get(arms::row(action, self.group)?)
    }

    fn current<'a>(&self, moves: &'a [Move]) -> Option<&'a Move> {
        self.played(moves, self.action)
    }

    /// Whether the animation of an action is in memory.
    fn in_memory(&self, moves: &[Move], action: i16) -> bool {
        (self.played(moves, action)).is_some_and(|played| !played.travel.is_empty())
    }

    /// The travel getter `0x800AA310`: the travel of the counter's frame is kept. A frame
    /// the animation does not have leaves the kept travel as it is.
    fn read_travel(&mut self, moves: &[Move]) {
        let Some(played) = self.current(moves) else {
            return;
        };
        let frame = self.counter % (mark(played.end) + 1);
        if let Some(travel) = played.travel.get(frame as usize) {
            self.travel = travel.map(i32::from);
        }
    }

    /// The counter goes to the loop start of the action, as at its end or its hold frame.
    fn turn_back(&mut self, moves: &[Move], played: &Move) {
        self.flags |= flag::LOOPED;
        self.counter = mark(played.loop_start);
        self.read_travel(moves);
    }

    /// Starting an action (`0x800AB1E0`): nothing when state and action are the ones that
    /// run. A frame of 0 is the action's first frame; a negative action changes only the
    /// state, a negative state only the action.
    pub fn start(&mut self, moves: &[Move], state: i32, action: i16, frame: u32) {
        let first = self.played(moves, action);
        let frame = match frame {
            0 => first.map_or(0, |played| played.first as i32 as u32),
            frame => frame,
        };
        if self.state == state && self.action == action {
            return;
        }
        self.idle = 0;
        if action < 0 {
            self.state = state;
        } else {
            if state >= 0 {
                self.state = state;
            }
            (self.action_before, self.counter_before) = (self.action, self.counter);
            (self.action, self.counter) = (action, frame);
            self.fade = 0;
        }
        if self.action_before != self.action {
            self.flags |= flag::NO_STEP;
            if self.in_memory(moves, self.action_before) {
                self.flags |= flag::FADE;
            }
        }
    }

    /// The free player (`0x800AA42C`, mode 0): the state and the action for the command
    /// word. Nothing is decided while a fade runs or the player was put on a base place.
    pub fn free(&mut self, moves: &[Move]) {
        if self.flags & flag::ARMS != 0 {
            self.start(moves, state::ARMS, act::STAND, 0);
            self.mode = 1;
            return;
        }
        if self.flags & (flag::PUT | flag::FADE) != 0 {
            return;
        }
        let turns = self.command & (command::FALLS | command::RISES) != 0;
        match self.command & command::MOVEMENT {
            command::RUN => self.start(moves, state::RUNS, act::RUN, 0),
            command::BACK => self.start(moves, state::BACKS, act::BACK, 0),
            command::WALK => {
                self.start(moves, state::WALKS, act::WALK, 0);
                self.flags &= !flag::TURN_ONLY;
            }
            // Turning on the spot plays the walk without a step.
            _ if turns => {
                self.start(moves, state::WALKS, act::WALK, 0);
                self.flags |= flag::TURN_ONLY;
            }
            command::ORDERED => {
                self.ordered[0] = self.ordered[1];
                self.start(moves, state::ORDERED, act::STAND, 0);
                self.mode = 3;
            }
            _ => {
                self.idle = self.idle.wrapping_add(1);
                if self.idle > IDLE {
                    self.start(moves, state::WAITS, act::WAIT, 0);
                    self.idle = IDLE;
                } else {
                    self.start(moves, state::STANDS, act::STAND, 0);
                }
            }
        }
    }

    pub fn order(&mut self, moves: &[Move], action: i16, next: i16, fade: bool) -> bool {
        if !self.in_memory(moves, action) || (next >= 0 && !self.in_memory(moves, next)) {
            return false;
        }
        self.start(moves, state::ORDERED, action, 0);
        self.mode = 2;
        // The routine has the last word on the fade: an order without one cuts.
        match fade {
            true => self.flags |= flag::FADE,
            false => self.flags &= !flag::FADE,
        }
        self.ordered = [action; 2];
        self.follows = next;
        self.flags &= !(flag::GO_ON | 1);
        true
    }

    /// "Go on" (`0x800AB374`): the ordered action that stands ready starts in the next
    /// update. With `held` (an argument of 1) the action is also held: it turns back at
    /// its hold frame, and the mark of having turned back is cleared.
    pub fn go_on(&mut self, held: bool) {
        self.flags |= flag::GO_ON;
        if held {
            self.flags |= flag::HOLD;
            self.flags &= !flag::LOOPED;
        }
    }

    /// The hold is taken off (`0x800ABAE0`): the action runs past its hold frame.
    pub fn let_go(&mut self) {
        self.flags &= !flag::HOLD;
    }

    /// Whether the action has turned back since "go on" (`0x800ABB08`).
    pub fn looped(&self) -> bool {
        self.flags & flag::LOOPED != 0
    }

    /// Whether the player stands or waits (`0x800ABB28`: a state below 2).
    pub fn stands(&self) -> bool {
        (0..2).contains(&self.state)
    }

    /// An ordered action starts at its loop start or at its first frame (`0x800ABD68`).
    pub fn from_loop_start(&mut self, on: bool) {
        match on {
            true => self.flags |= flag::FROM_LOOP_START,
            false => self.flags &= !flag::FROM_LOOP_START,
        }
    }

    /// Shown or hidden (`0x800ABDA8`). Showing also ends the black of a fight
    /// (`0x800AB320`): neither into it nor out of it, the share 0.
    pub fn set_shown(&mut self, shown: bool) {
        match shown {
            true => {
                self.flags &= !flag::HIDDEN;
                self.out_of_black();
            }
            false => self.flags |= flag::HIDDEN,
        }
    }

    /// The black of a fight is over (`0x800AB320`).
    pub fn out_of_black(&mut self) {
        self.flags &= !(flag::INTO_BLACK | flag::OUT_OF_BLACK);
        self.fight.black = 0;
    }

    /// Walks over the ground or not (`0x800ABE04`).
    pub fn set_walks(&mut self, walks: bool) {
        match walks {
            true => self.flags &= !flag::NO_GROUND,
            false => self.flags |= flag::NO_GROUND,
        }
    }

    /// Carried by its animation or not (`0x800ABE48`): the base place is where the player
    /// stands when the flag is set.
    pub fn set_carried(&mut self, carried: bool) {
        match carried {
            true => {
                self.flags |= flag::CARRIED;
                self.base = self.position;
            }
            false => self.flags &= !flag::CARRIED,
        }
    }

    /// The player is put on a path, at its first point (`0x800ABC74`). The step follows
    /// it only while the player is carried as well.
    pub fn set_path(&mut self, path: Path) {
        (self.path, self.path_at) = (Some(path), 0);
        self.flags |= flag::PATH;
    }

    /// The path rule of the step (`0x800AA08C`): the point of the path's counter becomes
    /// the kept travel, the place is the base place plus the point over the path's
    /// divisor, and the angle is the point's. Behind the last point the path and the
    /// carrying end and the player stands in mode 0, on that point.
    fn follow_path(&mut self, moves: &[Move]) {
        let path = self.path.as_ref();
        let point = path.and_then(|path| Some((*path.points.get(self.path_at)?, path.divisor)));
        // The game reads whatever lies there; a counter that names no point changes nothing.
        let Some((point, divisor)) = point else {
            return;
        };
        self.travel = point.place.map(i32::from);
        self.path_at += 1;
        if self.path_at >= self.path.as_ref().map_or(0, |path| path.points.len()) {
            self.flags &= !(flag::PATH | flag::CARRIED | flag::HOLD);
            self.flags |= flag::NO_STEP;
            self.start(moves, state::STANDS, act::STAND, 0);
            self.mode = 0;
        }
        for axis in 0..3 {
            self.position[axis] =
                self.base[axis].wrapping_add(quotient(self.travel[axis], divisor));
        }
        self.angle = point.turn;
    }

    /// The loop start of an action when ordered actions start there, else 0, which the
    /// start routine takes for the first frame.
    fn ordered_start(&self, moves: &[Move], action: i16) -> u32 {
        let played = self.played(moves, action);
        match (self.flags & flag::FROM_LOOP_START != 0, played) {
            (true, Some(played)) => mark(played.loop_start),
            _ => 0,
        }
    }

    /// Mode 2 (`0x800AAEC8`): an ordered action whose animation is in memory. State 23
    /// waits for "go on" and starts the action; state 24 plays it to its end, where the
    /// action that follows starts and is held, or without one the player stands again.
    fn ordered_action(&mut self, moves: &[Move]) {
        match self.state {
            state::ORDERED => {
                if self.flags & flag::GO_ON == 0 {
                    return;
                }
                let frame = self.ordered_start(moves, self.ordered[0]);
                self.start(moves, state::PLAYS, self.ordered[0], frame);
                self.flags &= !flag::LOOPED;
                // An action with one to follow is not held: it must come to its end.
                if self.follows >= 0 {
                    self.flags &= !flag::HOLD;
                }
            }
            state::PLAYS => {
                let Some(played) = self.current(moves) else {
                    return;
                };
                if self.flags & flag::SENT_ON != 0 {
                    self.flags &= !flag::SENT_ON;
                    if self.flags & flag::HOLD != 0 {
                        self.counter = mark(played.loop_start);
                        self.flags &= !flag::LOOPED;
                    } else {
                        self.counter = mark(played.end);
                    }
                }
                if self.counter == mark(played.end) {
                    if self.follows < 0 {
                        self.start(moves, state::STANDS, act::STAND, 0);
                        self.mode = 0;
                    } else {
                        let frame = self.ordered_start(moves, self.follows);
                        self.start(moves, -1, self.follows, frame);
                        self.follows = -1;
                        self.flags |= flag::HOLD;
                        self.ordered[0] = self.ordered[1];
                    }
                }
                // Another action was asked for meanwhile: back to waiting for "go on".
                if self.ordered[0] != self.ordered[1] {
                    self.state = state::ORDERED;
                    self.ordered[0] = self.ordered[1];
                }
            }
            _ => {}
        }
    }

    /// The update `0x800A9644` up to the turn: one game frame. What follows in the game is
    /// the pose, the drawing and the animation events. `counter` is the game's frame
    /// counter, which names the place of the turn filter.
    pub fn update(&mut self, moves: &[Move], ground: &Ground, arc: &ArcTangent, counter: u32) {
        self.wanted_turn = 0;
        self.before = self.position;
        if i32::from(self.fade) < WHOLE {
            self.fade = self.fade.wrapping_add(FADE_STEP);
        } else {
            self.flags &= !flag::FADE;
        }
        self.fraction = self.fraction.wrapping_add(self.speed);
        if i32::from(self.fraction) >= WHOLE {
            self.fraction -= WHOLE as u16;
            self.counter = self.counter.wrapping_add(1);
        }
        if self.flags & flag::HOLD != 0 {
            let held = (self.current(moves)).filter(|played| self.counter == mark(played.hold));
            if let Some(played) = held {
                self.turn_back(moves, played);
            }
        }
        self.fight.signs.clear();
        match self.mode {
            0 => self.free(moves),
            1 => self.arms(moves),
            2 => self.ordered_action(moves),
            _ => {}
        }
        self.step(moves, ground, arc);
        self.turn(arc, counter);
    }

    /// The step `0x800A9C58`: the difference of the travel becomes a step over the ground,
    /// or the travel puts a carried player.
    fn step(&mut self, moves: &[Move], ground: &Ground, arc: &ArcTangent) {
        let last = self.travel;
        self.read_travel(moves);
        let (mut side, mut forward) = (0i32, 0i32);
        if self.flags & flag::NO_STEP != 0 {
            self.flags &= !flag::NO_STEP;
        } else {
            // The low bits of both differences are kept back, from the forward value.
            side = self.travel[0].wrapping_sub(last[0]);
            self.travel[2] = self.travel[2].wrapping_sub((side & 31) * side.signum());
            forward = self.travel[2].wrapping_sub(last[2]);
            self.travel[2] = self.travel[2].wrapping_sub((forward & 31) * forward.signum());
        }
        let Some(played) = self.current(moves) else {
            return;
        };
        let held = self.flags & flag::HOLD != 0 && self.counter == mark(played.hold);
        if self.counter == mark(played.end) || held {
            self.turn_back(moves, played);
        }
        let on_the_spot = self.state == state::WALKS && self.flags & flag::TURN_ONLY != 0;
        if on_the_spot || self.action == act::STILL {
            (side, forward) = (0, 0);
        }

        let angle = i32::from(self.angle);
        let (sine, cosine) = (sine(angle), cosine(angle));
        if self.flags & (flag::CARRIED | flag::PUT) != 0 {
            if self.flags & flag::PATH != 0 {
                self.follow_path(moves);
                return;
            }
            self.read_travel(moves);
            let [x, y, z] = self.travel;
            let along = z.wrapping_mul(cosine).wrapping_sub(x.wrapping_mul(sine));
            let across = z.wrapping_mul(sine).wrapping_add(x.wrapping_mul(cosine));
            self.position = [
                (self.base[0]).wrapping_add(quotient(part(across), played.divisor)),
                self.base[1].wrapping_add(y / HEIGHT_DIVISOR),
                (self.base[2]).wrapping_add(quotient(part(along), played.divisor)),
            ];
            return;
        }

        // The slope share: what the triangle under the player leaves of a step up or down
        // it. Where no triangle holds the player the last one found counts; the game always
        // has one, and without any the ground counts as level here.
        ground.stand(&mut self.foothold, self.position);
        let region = ground.regions.get(self.foothold.region);
        let triangle = region.and_then(|region| region.triangles.get(self.foothold.triangle));
        let normal = triangle.map_or([0, -WHOLE, 0], |triangle| triangle.normal.map(i32::from));
        let lean = part(normal[0].wrapping_mul(sine)) + part(normal[2].wrapping_mul(cosine));
        let share = WHOLE - part(lean.wrapping_mul(lean));
        let shared = |value: i32| quotient(part(value.wrapping_mul(share)), played.divisor);
        let [x, _, z] = &mut self.position;
        *x = x.wrapping_add(shared(part(side.wrapping_mul(cosine))));
        *z = z.wrapping_add(shared(-part(side.wrapping_mul(sine))));
        *x = x.wrapping_add(shared(part(forward.wrapping_mul(sine))));
        *z = z.wrapping_add(shared(part(forward.wrapping_mul(cosine))));

        if self.flags & flag::NO_GROUND != 0 {
            return;
        }
        // Sliding along an edge turns the player along it.
        let position = &mut self.position;
        match ground.walk_player(&mut self.foothold, position, self.before, self.angle, arc) {
            Ok(answer) if answer < 0 => self.wanted_turn = -TURN,
            Ok(answer) if answer > 0 => self.wanted_turn = TURN,
            Ok(_) => {}
            Err(fault) => self.fault = Some(fault),
        }
    }

    /// The turn `0x800A9ABC`: to the stick's direction as the camera sees it, or by the
    /// average of the last four wanted turns.
    fn turn(&mut self, arc: &ArcTangent, counter: u32) {
        if self.command & command::STICK != 0 {
            let stick = arc.angle(i32::from(self.stick[0]), i32::from(self.stick[1]));
            let wanted = i32::from(stick) + i32::from(self.camera);
            // The short way round, a remainder towards zero brought into half a turn.
            let off = match (wanted - i32::from(self.angle)) % WHOLE {
                off if off < -WHOLE / 2 => off + WHOLE,
                off if off > WHOLE / 2 => off - WHOLE,
                off => off,
            };
            self.angle = (self.angle).wrapping_add(off.clamp(-STICK_TURN, STICK_TURN) as i16);
            self.command &= !command::STICK;
            return;
        }
        if self.command & command::FALLS != 0 {
            self.wanted_turn = -TURN;
        }
        if self.command & command::RISES != 0 {
            self.wanted_turn = TURN;
        }
        if self.command & command::HALF != 0 {
            self.wanted_turn /= 2;
        }
        self.command &= !(command::FALLS | command::RISES);
        let by = self.filter.run(self.wanted_turn, counter);
        self.angle = self.angle.wrapping_add(by as i16);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::{Region, Triangle};
    use std::f64::consts::TAU;

    /// A table like the game's, which the tests need only to be near the true angle.
    fn arc() -> ArcTangent {
        let step = |ratio: usize| ((ratio as f64 / 1024.0).atan() * 4096.0 / TAU).round() as i16;
        ArcTangent::of((0..1025).map(step).collect())
    }

    /// An action over the frames of this forward travel, with the hold frame at its end.
    fn played(loop_start: i16, forward: &[i16]) -> Move {
        Move {
            first: 0,
            loop_start,
            hold: forward.len() as i16 - 1,
            end: forward.len() as i16 - 1,
            window: 0,
            bare: 1,
            divisor: 32,
            travel: forward.iter().map(|z| [0, 0, *z]).collect(),
        }
    }

    /// Actions 0 to 4: walk, back, run, stand and wait.
    #[test]
    fn an_order_to_a_place_makes_the_pads_command_until_the_player_is_there() {
        let (arc, moves) = (arc(), moves());
        let mut player = Player::new([0, 0, 0], 0, &moves);
        let mut orders = Orders::default();
        orders.take();
        // The giving only keeps the place; "go on" starts the state of the kind.
        orders.give_place([0, 0, 500]);
        assert!(orders.ready());
        assert!(!orders.go_on(&mut player, (0, u32::MAX, 0)));
        assert_eq!(orders.state, order_state::GOES);
        // Straight ahead: a walk and no turn. The script is not told.
        assert!(!orders.pass(&mut player, &arc));
        assert_eq!(orders.command, Some(command::WALK));
        // The place to one side, more than 64 off: the turn bit of that side as well.
        player.angle = 1024;
        assert!(!orders.pass(&mut player, &arc));
        assert_eq!(orders.command, Some(command::WALK | command::FALLS));
        player.angle = -1024;
        assert!(!orders.pass(&mut player, &arc));
        assert_eq!(orders.command, Some(command::WALK | command::RISES));
        // Kind 1 is the same with a run.
        orders.go_on(&mut player, (1, u32::MAX, 0));
        player.angle = 0;
        assert!(!orders.pass(&mut player, &arc));
        assert_eq!(orders.command, Some(command::RUN));
        // Nearer than 20 units over all three axes: no movement, and the script is told
        // once the player stands.
        player.position = [0, 0, 485];
        player.state = state::WALKS;
        assert!(!orders.pass(&mut player, &arc));
        assert_eq!((orders.command, orders.state), (Some(0), order_state::RUNS));
        player.state = state::STANDS;
        assert!(orders.pass(&mut player, &arc));
        assert_eq!(
            (orders.command, orders.state),
            (Some(0), order_state::TAKEN)
        );
        // The height counts: the same place 20 higher is not there.
        orders.go_on(&mut player, (0, u32::MAX, 0));
        player.position = [0, -20, 485];
        assert!(!orders.pass(&mut player, &arc));
        assert_eq!(orders.command, Some(command::WALK));
        // A script that gives the player back ends the order where the player is.
        orders.give_back(&mut player, &moves);
        assert!(orders.pass(&mut player, &arc));
        assert_eq!(orders.state, order_state::TAKEN);
        // State 9 leaves the pad's command alone.
        orders.take();
        assert!(!orders.pass(&mut player, &arc));
        assert_eq!(orders.command, None);

        // Kind 2 turns: a turn bit until the angle is within 64 of the direction, then
        // the script is told at once.
        let mut player = Player::new([0, 0, 0], 100, &moves);
        orders.give_place([0, 0, 500]);
        orders.go_on(&mut player, (2, u32::MAX, 0));
        assert_eq!(orders.state, order_state::TURNS);
        assert!(!orders.pass(&mut player, &arc));
        assert_eq!(orders.command, Some(command::FALLS));
        player.angle = 64;
        assert!(orders.pass(&mut player, &arc));
        assert_eq!(
            (orders.command, orders.state),
            (Some(0), order_state::TAKEN)
        );

        // Further apart than the limit on one axis the distance routine answers -1,
        // which is "there" for the signed comparison of the order states.
        assert_eq!(squared_within([0, 0, 0], [3, 4, 12], GO_LIMIT), 169);
        assert_eq!(
            squared_within([0, 0, 0], [0, 0, GO_LIMIT + 1], GO_LIMIT),
            -1
        );
    }

    fn moves() -> Vec<Move> {
        vec![
            played(1, &[0, 48, 96, 144, 192]),
            played(0, &[0, -64, -128]),
            played(0, &[0, 640]),
            played(0, &[0, 0, 0]),
            played(0, &[0, 0]),
        ]
    }

    /// A player in the middle of an action, off the walk mesh.
    fn walking(moves: &[Move], action: i16, state: i32) -> Player {
        let mut player = Player::new([0, 0, 0], 0, moves);
        (player.action, player.action_before, player.state) = (action, action, state);
        player.flags = flag::NO_GROUND;
        player
    }

    fn floor(corners: [[i16; 2]; 3], normal: [i16; 3]) -> Triangle {
        Triangle {
            normal,
            corners: corners.map(|[x, z]| [x, -20, z]),
            inner_edges: [false; 3],
        }
    }

    fn ground(triangles: Vec<Triangle>) -> Ground {
        let mut region = Region { triangles };
        region.mark_inner_edges();
        Ground::new(&[region], &[])
    }

    #[test]
    fn the_pad_makes_a_command_word() {
        let stick = command::STICK_ONE;
        assert_eq!(stick_command([105, 0]), command::RUN | stick);
        assert_eq!(stick_command([104, 0]), command::WALK | stick);
        assert_eq!(stick_command([-52, -6]), command::WALK | stick);
        assert_eq!(stick_command([52, 5]), 0);
        assert_eq!(stick_command([-128, -128]), command::RUN | stick);

        let held = Held::default;
        let forward = Held {
            forward: true,
            ..held()
        };
        assert_eq!(button_command(held()), 0);
        assert_eq!(button_command(forward), command::WALK);
        let fast = Held {
            fast: true,
            ..forward
        };
        assert_eq!(button_command(fast), command::RUN);
        // Without forward the second button is nothing; back is only asked without both.
        let back = Held {
            back: true,
            fast: true,
            falls: true,
            rises: true,
            ..held()
        };
        assert_eq!(button_command(back), command::BACK | command::FALLS);
        let run = Held {
            run: true,
            back: true,
            rises: true,
            ..held()
        };
        assert_eq!(button_command(run), command::RUN | command::RISES);
    }

    #[test]
    fn a_near_thing_ahead_turns_the_player_away_and_one_behind_stops_the_step_back() {
        let arc = arc();
        let thing = |position| Thing {
            position,
            hidden: false,
        };
        let way = |angle, things: &[Thing]| InTheWay::of(&arc, [0, 0, 0], angle, things);
        let nothing = InTheWay::default();

        // Straight ahead, and near up to a squared distance of 39999 over all three axes.
        assert_eq!(way(0, &[thing([0, 0, 199])]).ahead, Some(0));
        assert_eq!(way(0, &[thing([0, 0, 200])]), nothing);
        assert_eq!(way(0, &[thing([0, 150, 150])]), nothing);
        let hidden = Thing {
            position: [0, 0, 100],
            hidden: true,
        };
        assert_eq!(way(0, &[hidden]), nothing);

        // To the side the angle rises to: the player turns the other way. The first near
        // thing ahead is the one that counts.
        let both = [thing([100, 0, 100]), thing([-100, 0, 100])];
        assert_eq!(way(0, &both).ahead, Some(-512));
        assert_eq!(way(0, &both[1..]).ahead, Some(512));
        // Exactly to one side is neither ahead nor behind; with the player's back to it, it
        // is behind.
        assert_eq!(way(0, &[thing([100, 0, 0])]), nothing);
        assert!(way(-1024, &[thing([100, 0, 0])]).behind);
        let behind = way(0, &[thing([0, 0, -100])]);
        assert_eq!((behind.ahead, behind.behind), (None, true));
        // The player's own angle counts: turned round, the thing behind is ahead.
        assert_eq!(way(2048, &[thing([0, 0, -100])]).ahead, Some(0));

        let ahead = |off| InTheWay {
            ahead: Some(off),
            behind: false,
        };
        let turn = command::HALF | command::RISES;
        assert_eq!(ahead(0).turned_away(command::RUN), turn);
        assert_eq!(ahead(512).turned_away(command::WALK), turn);
        let stick = command::STICK_ONE;
        let falls = command::HALF | command::FALLS;
        assert_eq!(ahead(-1).turned_away(command::WALK | stick), falls | stick);
        // A held turn button is added to the turn away.
        let held = command::RUN | command::RISES;
        assert_eq!(ahead(-512).turned_away(held), falls | command::RISES);
        assert_eq!(ahead(0).turned_away(command::BACK), command::BACK);
        assert_eq!(nothing.turned_away(held), held);
        let back = command::BACK | command::FALLS;
        assert_eq!(behind.turned_away(back), command::FALLS);
        assert_eq!(behind.turned_away(command::RUN), command::RUN);
    }

    #[test]
    fn a_talk_needs_a_thing_near_and_faced() {
        let arc = arc();
        // Near up to a squared distance of 89999 over all three axes.
        assert!(talk_near([0, 0, 0], [0, 0, 299]));
        assert!(!talk_near([0, 0, 0], [0, 0, 300]));
        assert!(talk_near([10, 20, 30], [10 + 200, 20 + 100, 30 + 199]));
        assert!(!talk_near([0, 0, 0], [200, 100, 201]));
        // A place far off does not come back near by the wrap of 32 bits.
        assert!(!talk_near([0, 0, 0], [i32::MAX, 0, 0]));
        // Faced: the thing straight ahead, and up to 408 of 4096 to either side.
        let faced = |angle| talk_faced(&arc, [0, 0, 0], angle, [0, 0, 100]);
        assert!(faced(0) && faced(408) && faced(-408));
        assert!(!faced(409) && !faced(-409) && !faced(2048));
        // The angle is asked about the upright axis alone.
        assert!(talk_faced(&arc, [0, 0, 0], 1024, [100, -500, 0]));
        assert!(!talk_faced(&arc, [0, 0, 0], 1024, [-100, 0, 0]));
    }

    #[test]
    fn a_new_player_stands_without_a_fade_and_makes_no_first_step() {
        let moves = moves();
        let mut player = Player::new([0, 0, 0], 0, &moves);
        assert_eq!((player.state, player.mode), (state::STANDS, 0));
        assert_eq!(
            (player.action, player.action_before),
            (act::STAND, act::STAND)
        );
        assert_eq!((player.counter, player.fade), (0, 0));
        assert_eq!(player.flags, flag::NO_STEP);
        // The fade counts up on its own; no action before is mixed in.
        let empty = Ground::default();
        player.update(&moves, &empty, &arc(), 0);
        assert_eq!((player.counter, player.fade, player.flags), (1, 1024, 0));
        assert_eq!((player.state, player.action), (state::STANDS, act::STAND));
    }

    #[test]
    fn the_free_player_picks_state_and_action_by_the_command() {
        let moves = moves();
        let mut player = Player::new([0, 0, 0], 0, &moves);
        player.command = command::WALK;
        player.free(&moves);
        assert_eq!((player.state, player.action), (state::WALKS, act::WALK));
        assert_eq!(player.flags, flag::NO_STEP | flag::FADE);
        // Nothing is decided while the fade runs.
        player.command = command::RUN;
        player.free(&moves);
        assert_eq!((player.state, player.action), (state::WALKS, act::WALK));
        player.flags = 0;
        player.free(&moves);
        assert_eq!((player.state, player.action), (state::RUNS, act::RUN));
        player.flags = 0;
        player.command = command::BACK | command::RISES;
        player.free(&moves);
        assert_eq!((player.state, player.action), (state::BACKS, act::BACK));
        // Only a turn bit: the walk, marked as a turn on the spot until a walk comes.
        player.flags = 0;
        player.command = command::FALLS;
        player.free(&moves);
        assert_eq!((player.state, player.action), (state::WALKS, act::WALK));
        assert_ne!(player.flags & flag::TURN_ONLY, 0);
        player.flags &= !flag::FADE;
        player.command = command::WALK;
        player.free(&moves);
        assert_eq!(player.flags & flag::TURN_ONLY, 0);
        // The ordered action is taken over and waited for in mode 3.
        player.ordered = [-1, 40];
        player.command = command::ORDERED;
        player.free(&moves);
        assert_eq!((player.state, player.action), (state::ORDERED, act::STAND));
        assert_eq!((player.mode, player.ordered), (3, [40, 40]));
    }

    #[test]
    fn a_standing_player_changes_its_action_after_ninety_game_frames() {
        let moves = moves();
        let mut player = Player::new([0, 0, 0], 0, &moves);
        for _ in 0..IDLE {
            player.free(&moves);
            assert_eq!((player.state, player.action), (state::STANDS, act::STAND));
        }
        player.free(&moves);
        assert_eq!((player.state, player.action), (state::WAITS, act::WAIT));
        assert_eq!(player.idle, IDLE);
        // A command starts the count again.
        player.flags = 0;
        player.command = command::WALK;
        player.free(&moves);
        assert_eq!(player.idle, 0);
        // With the weapon flag the free player leaves for mode 1.
        player.flags = flag::ARMS;
        player.free(&moves);
        assert_eq!((player.state, player.action), (state::ARMS, act::STAND));
        assert_eq!(player.mode, 1);
    }

    #[test]
    fn an_action_starts_from_its_first_frame_and_keeps_the_one_before() {
        let mut moves = moves();
        moves[act::RUN as usize].first = 1;
        let mut player = Player::new([0, 0, 0], 0, &moves);
        player.counter = 2;
        player.start(&moves, state::RUNS, act::RUN, 0);
        assert_eq!((player.action, player.counter), (act::RUN, 1));
        assert_eq!(
            (player.action_before, player.counter_before),
            (act::STAND, 2)
        );
        assert_eq!((player.fade, player.flags), (0, flag::NO_STEP | flag::FADE));
        // The ones that run: nothing happens, whatever the frame.
        player.flags = 0;
        player.start(&moves, state::RUNS, act::RUN, 7);
        assert_eq!((player.counter, player.flags), (1, 0));
        // Another state with the same action: no fade and no lost step, but the frame.
        player.start(&moves, state::WALKS, act::RUN, 7);
        assert_eq!((player.state, player.counter, player.flags), (2, 7, 0));
        assert_eq!(player.action_before, act::RUN);
        // No fade from an action whose animation is not in memory.
        moves[act::RUN as usize].travel.clear();
        player.start(&moves, state::STANDS, act::STAND, 0);
        assert_eq!(player.flags, flag::NO_STEP);
        // A negative action changes only the state, a negative state only the action.
        player.flags = 0;
        player.start(&moves, 9, -1, 0);
        assert_eq!((player.state, player.action), (9, act::STAND));
        player.start(&moves, -1, act::WAIT, 0);
        assert_eq!((player.state, player.action), (9, act::WAIT));
    }

    #[test]
    fn a_step_is_the_difference_of_the_travel_over_the_divisor() {
        let moves = moves();
        let empty = Ground::default();
        let mut player = walking(&moves, act::WALK, state::WALKS);
        let mut forward = Vec::new();
        for counter in 0..5 {
            player.command = command::WALK;
            player.update(&moves, &empty, &arc(), counter);
            forward.push(player.position[2]);
        }
        // 48 a frame over 32: the 16 that the division drops come back every other frame,
        // and are lost where the action turns back to its loop start.
        assert_eq!(forward, [1, 3, 4, 6, 7]);
        assert_eq!((player.counter, player.travel[2]), (2, 80));
        assert_ne!(player.flags & flag::LOOPED, 0);
        assert_eq!(player.before, [0, 0, 6]);

        // Turned a quarter, forward is x.
        let mut player = walking(&moves, act::RUN, state::RUNS);
        player.angle = 1024;
        player.command = command::RUN;
        player.update(&moves, &empty, &arc(), 0);
        assert_eq!(player.position, [20, 0, 0]);
        // A step back.
        let mut player = walking(&moves, act::BACK, state::BACKS);
        player.command = command::BACK;
        player.update(&moves, &empty, &arc(), 0);
        assert_eq!(player.position, [0, 0, -2]);
    }

    #[test]
    fn some_game_frames_make_no_step() {
        let moves = moves();
        let empty = Ground::default();
        // The mark of a started action: no step, and nothing is kept back of the travel.
        let mut player = walking(&moves, act::WALK, state::WALKS);
        player.flags |= flag::NO_STEP;
        player.command = command::WALK;
        player.update(&moves, &empty, &arc(), 0);
        assert_eq!((player.position, player.travel[2]), ([0, 0, 0], 48));
        assert_eq!(player.flags & flag::NO_STEP, 0);
        // A turn on the spot plays the walk and stays.
        let mut player = walking(&moves, act::WALK, state::WALKS);
        for counter in 0..3 {
            player.command = command::RISES;
            player.update(&moves, &empty, &arc(), counter);
        }
        assert_eq!((player.position, player.counter), ([0, 0, 0], 3));
        assert_eq!(player.angle, 16 + 32 + 48);
        // The whole way from standing: the frame an action starts in makes no step.
        let mut player = Player::new([0, 0, 0], 0, &moves);
        player.flags |= flag::NO_GROUND;
        player.command = command::RUN;
        player.update(&moves, &empty, &arc(), 0);
        assert_eq!((player.action, player.position), (act::RUN, [0, 0, 0]));
        assert_eq!((player.flags & flag::FADE, player.fade), (flag::FADE, 0));
        player.update(&moves, &empty, &arc(), 1);
        assert_eq!((player.position, player.fade), ([0, 0, 20], 1024));
        // The fade is through after four game frames and its flag goes in the fifth.
        for counter in 2..5 {
            player.update(&moves, &empty, &arc(), counter);
        }
        assert_eq!((player.flags & flag::FADE, player.fade), (flag::FADE, 4096));
        player.update(&moves, &empty, &arc(), 5);
        assert_eq!(player.flags & flag::FADE, 0);
    }

    #[test]
    fn a_held_action_turns_back_at_its_hold_frame() {
        let mut moves = moves();
        moves[act::WALK as usize].hold = 3;
        let empty = Ground::default();
        let mut player = walking(&moves, act::WALK, state::WALKS);
        (player.counter, player.travel) = (2, [0, 0, 96]);
        player.flags |= flag::HOLD;
        player.command = command::WALK;
        player.update(&moves, &empty, &arc(), 0);
        assert_eq!((player.counter, player.position), (1, [0, 0, 0]));
        assert_ne!(player.flags & flag::LOOPED, 0);
        // Without the flag the action runs past the hold frame.
        let mut player = walking(&moves, act::WALK, state::WALKS);
        (player.counter, player.travel) = (2, [0, 0, 96]);
        player.command = command::WALK;
        player.update(&moves, &empty, &arc(), 0);
        assert_eq!((player.counter, player.position), (3, [0, 0, 1]));
    }

    #[test]
    fn a_slope_shortens_a_step_up_or_down_it() {
        let moves = vec![played(0, &[0, 64])];
        // One large triangle that leans along z by half.
        let corners = [[-1000, -1000], [1000, 1000], [1000, -1000]];
        let slope = ground(vec![floor(corners, [0, -3547, 2048])]);
        let start = |angle: i16| {
            let mut player = walking(&moves, act::WALK, state::WALKS);
            (player.position, player.angle) = ([500, 0, -500], angle);
            player.command = command::WALK;
            player.update(&moves, &slope, &arc(), 0);
            player.position
        };
        // Straight up it: a quarter of the step is lost. Along it: the full step.
        assert_eq!(start(0), [500, 0, -499]);
        assert_eq!(start(1024), [502, 0, -500]);
        // Off every triangle the ground counts as level.
        let level = Ground::default();
        let mut player = walking(&moves, act::WALK, state::WALKS);
        player.command = command::WALK;
        player.update(&moves, &level, &arc(), 0);
        assert_eq!(player.position, [0, 0, 2]);
    }

    #[test]
    fn a_slide_along_an_edge_turns_the_player_along_it() {
        let moves = moves();
        let level = [0, -4096, 0];
        let square = ground(vec![
            floor([[0, 0], [100, 100], [100, 0]], level),
            floor([[0, 0], [0, 100], [100, 100]], level),
        ]);
        let mut player = Player::new([90, -20, 50], 1024, &moves);
        (player.action, player.action_before) = (act::RUN, act::RUN);
        (player.state, player.flags) = (state::RUNS, 0);
        player.command = command::RUN;
        player.update(&moves, &square, &arc(), 0);
        // Twenty units over the edge at x = 100: back onto it and four units inside.
        assert_eq!((player.position, player.fault), ([96, -20, 50], None));
        assert_eq!((player.wanted_turn, player.angle), (-TURN, 1024 - 16));
        // A held turn button counts for more than the slide.
        let mut player = Player::new([90, -20, 50], 1024, &moves);
        (player.action, player.action_before) = (act::RUN, act::RUN);
        (player.state, player.flags) = (state::RUNS, 0);
        player.command = command::RUN | command::RISES;
        player.update(&moves, &square, &arc(), 0);
        assert_eq!((player.wanted_turn, player.angle), (TURN, 1024 + 16));
        assert_eq!(player.command, command::RUN);
    }

    #[test]
    fn buttons_turn_through_the_filter_and_the_stick_by_the_picture() {
        let moves = moves();
        let arc = arc();
        let mut player = Player::new([0, 0, 0], 0, &moves);
        let angles: Vec<i16> = (0..6)
            .map(|counter| {
                player.command = command::RISES;
                player.turn(&arc, counter);
                player.angle
            })
            .collect();
        assert_eq!(angles, [16, 48, 96, 160, 224, 288]);
        let mut player = Player::new([0, 0, 0], 0, &moves);
        player.command = command::FALLS | command::HALF;
        player.turn(&arc, 0);
        assert_eq!((player.angle, player.command), (-8, command::HALF));

        // The stick points along the camera's direction, a quarter turn off the player's.
        let mut player = Player::new([0, 0, 0], 0, &moves);
        player.camera = camera_direction(&arc, [0, 0, 0], [500, -80, 0]);
        player.stick = [0, 100];
        let angles: Vec<i16> = (0..5)
            .map(|counter| {
                player.command = command::WALK | command::STICK_ONE;
                player.turn(&arc, counter);
                player.angle
            })
            .collect();
        assert_eq!(angles, [256, 512, 768, 1024, 1024]);
        assert_eq!(player.command, command::WALK);
        // The stick to the side, the short way round over the back.
        player.angle = 2000;
        player.camera = 0;
        player.stick = [-100, -100];
        player.command = command::STICK;
        player.turn(&arc, 0);
        assert_eq!(player.angle, 2000 + 256);
        player.angle = -1600;
        player.command = command::STICK;
        player.turn(&arc, 0);
        assert_eq!(player.angle, -1536);
    }

    #[test]
    fn a_carried_player_is_where_the_travel_says() {
        let mut moves = moves();
        moves[act::STAND as usize].travel = vec![[64, -96, 320]; 3];
        let mut player = Player::new([1000, 50, 2000], 0, &moves);
        player.flags |= flag::CARRIED;
        player.update(&moves, &Ground::default(), &arc(), 0);
        assert_eq!(player.position, [1002, 47, 2010]);
        player.angle = 1024;
        player.update(&moves, &Ground::default(), &arc(), 1);
        assert_eq!(player.position, [1010, 47, 1998]);
    }

    #[test]
    fn a_player_on_a_path_is_put_on_its_points_and_stands_at_its_end() {
        use crate::mover::PathPoint;
        let moves = moves();
        let mut player = Player::new([1000, 50, 2000], 77, &moves);
        let mut orders = Orders::default();
        orders.take();
        let point = |place, turn| PathPoint { place, turn };
        let points = vec![
            point([40, 8, -80], 100),
            point([80, 0, -160], 300),
            point([-7, 0, 7], 500),
        ];
        let path = Path { divisor: 4, points };
        // An action that is not in memory is not ordered, and nothing of the path stays.
        assert!(!orders.give_path(&mut player, &moves, 9, path.clone()));
        assert_eq!(player.flags & (flag::PATH | flag::CARRIED), 0);
        assert!(orders.give_path(&mut player, &moves, act::RUN, path));
        assert_eq!((player.mode, player.state), (2, state::ORDERED));
        assert_eq!((player.follows, player.base), (-1, [1000, 50, 2000]));
        let set = flag::PATH | flag::CARRIED | flag::FROM_LOOP_START | flag::FADE;
        assert_eq!(player.flags & set, set);
        assert!(orders.ready());

        // The path runs from the first game frame, also while the action waits for "go on".
        player.update(&moves, &Ground::default(), &arc(), 0);
        assert_eq!((player.position, player.angle), ([1010, 52, 1980], 100));
        assert_eq!(player.state, state::ORDERED);
        assert!(!orders.go_on(&mut player, (11, 0, 0)));
        assert_eq!(orders.state, order_state::PLAYS);
        player.update(&moves, &Ground::default(), &arc(), 1);
        assert_eq!((player.position, player.angle), ([1020, 50, 1960], 300));
        assert_eq!((player.state, player.action), (state::PLAYS, act::RUN));
        assert_eq!(player.travel, [80, 0, -160]);

        // The last point: the quotient goes towards zero, and the player stands free.
        player.update(&moves, &Ground::default(), &arc(), 2);
        assert_eq!((player.position, player.angle), ([999, 50, 2001], 500));
        assert_eq!(
            (player.state, player.action, player.mode),
            (state::STANDS, act::STAND, 0)
        );
        assert_eq!(player.flags & (flag::PATH | flag::CARRIED | flag::HOLD), 0);
        assert_ne!(player.flags & flag::NO_STEP, 0);
        // From here the free player's step: it stays where the path left it.
        player.flags |= flag::NO_GROUND;
        for counter in 3..9 {
            player.update(&moves, &Ground::default(), &arc(), counter);
        }
        assert_eq!(player.position, [999, 50, 2001]);
    }

    #[test]
    fn an_exit_can_make_the_player_stand_again_and_puts_only_the_place() {
        let moves = moves();
        let mut player = walking(&moves, act::RUN, state::RUNS);
        player.counter = 1;
        player.flags = flag::HOLD | flag::PATH | flag::CARRIED;
        player.mode = 2;
        player.stand_again(&moves);
        assert_eq!((player.state, player.action), (state::STANDS, act::STAND));
        assert_eq!((player.action_before, player.counter_before), (act::RUN, 1));
        assert_eq!((player.mode, player.counter, player.fade), (0, 0, 0));
        // The run is in memory, so it fades out; there is no step in the next game frame.
        assert_eq!(player.flags, flag::FADE | flag::NO_STEP);
        assert_eq!(player.travel, [0, 0, 0]);

        // The arrival writes the place record and nothing of the controller's.
        let mut arrived = player.clone();
        arrived.arrive([206, 0, -831], 193);
        assert_eq!((arrived.position, arrived.angle), ([206, 0, -831], 193));
        (arrived.position, arrived.angle) = (player.position, player.angle);
        assert_eq!(arrived, player);
    }

    #[test]
    fn a_move_takes_the_frames_of_its_action_and_the_travel_of_its_animation() {
        let action = Action {
            animation: Some(0),
            resident: 1,
            extra: [0; 2],
            first: 2,
            loop_start: 5,
            hold: 9,
            end: 12,
            windows: [[-1; 2]; crate::action::WINDOWS],
            tail: [-1; crate::action::TAIL],
        };
        let none = Move::of(&action, None);
        assert_eq!(
            (none.first, none.loop_start, none.hold, none.end),
            (2, 5, 9, 12)
        );
        assert_eq!((none.divisor, none.travel.len()), (32, 0));
    }
}
