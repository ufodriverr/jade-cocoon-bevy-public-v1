
use std::ops::Range;

use super::{Move, Player, act, flag, mark};
use crate::action::Action;

/// The actions that are looked up in the group of the weapon in the hand (`0x800AC964`),
/// and how many rows a group is further on.
pub const WEAPON_ACTIONS: Range<usize> = 14..110;
pub const GROUP_ROWS: usize = 16;
/// File 27: the group of a weapon, a word for each of the 19 looks a weapon can have
/// (`0x800AC644`, by the half word `0x8008BD34`).
pub const GROUPS: u32 = 0x800B_13AC;

/// The groups of the weapons by their look, from the field engine file as it is on the
/// disc, whose first byte is at RAM address `base`.
pub fn weapon_groups(field_engine: &[u8], base: u32, looks: usize) -> Option<Vec<usize>> {
    let at = GROUPS.checked_sub(base)? as usize;
    let (words, _) = field_engine.get(at..at + 4 * looks)?.as_chunks::<4>();
    Some(
        words
            .iter()
            .map(|word| u32::from_le_bytes(*word) as usize)
            .collect(),
    )
}

/// The row of an action in the player's table for a weapon group (`0x800AC964`).
pub fn row(action: i16, group: usize) -> Option<usize> {
    let action = usize::try_from(action).ok()?;
    match WEAPON_ACTIONS.contains(&action) {
        true => Some(action + GROUP_ROWS * group),
        false => Some(action),
    }
}

pub fn read_with_weapon(actions: &[Action], group: usize) -> Vec<usize> {
    let first = WEAPON_ACTIONS.start + GROUP_ROWS * group;
    (actions.iter().skip(first).take(GROUP_ROWS))
        .filter(|action| action.resident != 0)
        .filter_map(|action| action.animation)
        .collect()
}

pub const CAPTURE_FX: u32 = 1 << 21;

/// File 27: the rows of the kinds of a command, 8 bytes each: the order state and the
/// action (`0x800A8B10`).
pub const KINDS: u32 = 0x800B_1254;

pub fn wanted(kind: u32, setting: u32) -> Option<i16> {
    const BY_KIND: [i16; 27] = [
        16, 25, 26, 28, 29, 8, 16, 13, 19, 17, 18, 19, 20, 21, 20, 23, 22, 21, 20, 24, -1, 6, 12,
        17, 4, -1, 8,
    ];
    let action = (BY_KIND.get(kind as usize).copied()).filter(|action| *action >= 0)?;
    match action == action::CAPTURE && setting & CAPTURE_FX != 0 {
        true => Some(action::CAPTURE_LONG),
        false => Some(action),
    }
}

pub fn times(moves: &[Move], group: usize, wanted: i16) -> Option<[i16; 2]> {
    let of = |action: i16| row(action, group).and_then(|row| moves.get(row));
    let played = of(wanted)?;
    let stretch = played.hold.wrapping_sub(played.loop_start);
    let (key, whole) = match wanted {
        action::SPELL | action::SUMMON | action::LEAVE => (2 * stretch, 2 * stretch),
        action::CAPTURE_LONG | action::CAPTURE => {
            let ready = of(action::DRAW)?.end;
            let more = if wanted == action::CAPTURE_LONG {
                76
            } else {
                0
            };
            (ready + more, ready.wrapping_add(30 * stretch))
        }
        _ => (0, 0),
    };
    Some([
        played.window.wrapping_add(key),
        played.end.wrapping_add(whole),
    ])
}

pub fn draw_ends(moves: &[Move], group: usize) -> Option<i16> {
    let played = row(action::DRAW, group).and_then(|row| moves.get(row))?;
    Some(played.end)
}

/// The state of the camera task in which the opening view of a fight runs.
pub const OPENING_VIEW: i32 = 3;
/// The share of black of a player that is not seen.
const BLACK: i32 = 4096;
/// What the share of black moves by in a game frame.
const BLACK_STEP: i32 = 128;
/// The bits of the word a model in the black is drawn with: every face blended, in
/// mode 1, the colours mixed towards black by the share in the upper half.
const IN_BLACK: u32 = 0xB0;

/// The states of mode 1.
pub mod state {
    /// Waits for the opening view.
    pub const WAITS_FOR_VIEW: i32 = 7;
    /// Waits for the loop start of the action.
    pub const SETTLES: i32 = 8;
    /// Draws the weapon.
    pub const DRAWS: i32 = 9;
    /// Ready: goes round between the loop start and the hold frame.
    pub const READY: i32 = 10;
    /// Asks for the wanted action's animation and its effect.
    pub const ASKS: i32 = 11;
    /// Waits for them.
    pub const LOADS: i32 = 12;
    /// At the hold: the wanted action is in memory and waits for "go".
    pub const AT_THE_HOLD: i32 = 13;
    /// Puts the weapon away in front of an action without it.
    pub const PUTS_AWAY: i32 = 14;
    /// Plays the wanted action.
    pub const PLAYS: i32 = 15;
    /// The game frame in which the wanted action's effects start.
    pub const STARTS: i32 = 16;
    /// Behind the fight: waits for the animation of the pose.
    pub const LOADS_POSE: i32 = 17;
    /// Behind the fight: puts the weapon away.
    pub const ENDS: i32 = 18;
    /// Plays an action to its end and is free.
    pub const POSES: i32 = 19;
    /// Lies where he fell until the hold is taken off.
    pub const LIES: i32 = 20;
}

/// The actions of the player's files that the states of a fight name.
pub mod action {
    /// The capture with the setting "Capture FX", which a new game has.
    pub const CAPTURE_LONG: i16 = 7;
    /// The capture without it.
    pub const CAPTURE: i16 = 8;
    pub const POSE: i16 = 10;
    /// Draws the weapon; its loop is the ready stance.
    pub const DRAW: i16 = 14;
    /// Puts the weapon away.
    pub const PUT_AWAY: i16 = 15;
    pub const ATTACK: i16 = 16;
    pub const FALLS: i16 = 19;
    /// The action of a defence, which is also its stance.
    pub const GUARD: i16 = 20;
    /// The action and stance of kind 16 of a command.
    pub const STANCE: i16 = 22;
    pub const FALLS_OTHER: i16 = 24;
    pub const SPELL: i16 = 25;
    pub const SUMMON: i16 = 26;
    pub const LEAVE: i16 = 28;
    pub const ITEM: i16 = 29;
}

/// The effect the machine asks for with his own command (`0x800ABEC4`). Three of the
/// numbers come from routines of the executable that are not read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// `0x8001AC8C(spell)`.
    Spell(u32),
    /// Effect 37; the game also keeps `0x80013358(parameter)` for the capture.
    Capture(u32),
    /// Effect 57.
    Summon,
    /// Effect 58.
    Leave,
    /// `0x800189F8()`.
    Attack,
    /// `0x8001CE34(item)`.
    Item(u32),
}

impl Effect {
    /// The number of the effect, where the machine itself names it.
    pub fn number(self) -> Option<i32> {
        match self {
            Self::Capture(_) => Some(37),
            Self::Summon => Some(57),
            Self::Leave => Some(58),
            Self::Spell(_) | Self::Attack | Self::Item(_) => None,
        }
    }
}

/// What a game frame of the machine did besides the controller's record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sign {
    /// The weapon goes into the hand: mesh object 2 takes mesh 24, object 7 mesh 26.
    WeaponInHand,
    /// The weapon goes back: object 2 takes mesh 23, object 7 mesh 25.
    WeaponAway,
    /// The same, and object 12 gets its own mesh back.
    WeaponAwayWhole,
    /// The effects of the wanted action start (`0x800AA600`): the ten pairs of its row go
    /// to the effect sequencer, made later by [`later_windows`] with these values, with
    /// the player as the actor and the fighters of the target mask.
    Effects {
        effect: Effect,
        /// The loop start and the hold frame of the action that plays.
        loop_start: i16,
        hold: i16,
        loops: i16,
        targets: u32,
    },
    /// Sound 68, on the first frame of an item's first pair.
    Sound(u32),
    BeforeItem,
}

/// What the controller's record holds for a fight, and what the machine reads of the
/// world.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Kept {
    /// How often the stretch from the loop start to the hold frame is still played again.
    pub loops: i16,
    /// The effect of the wanted action; none for a reaction.
    pub effect: Option<Effect>,
    /// The parameter and the target mask of the command.
    pub parameter: u32,
    pub targets: u32,
    /// The share of black, 0 to 4096, as the two setters leave it. Its move by 128 a game
    /// frame belongs to the drawing ([`Player::draw_word`]).
    pub black: i32,
    /// The state of the camera task (`0x80037924`): state 7 waits for [`OPENING_VIEW`].
    pub camera_state: i32,
    /// The half word `0x8008D710` is 1: the loops of the capture end.
    pub loops_end: bool,
    /// What this game frame's update did. Cleared by every update.
    pub signs: Vec<Sign>,
}

/// The pairs of an action's row as the effect sequencer gets them (`0x80018200`): for
/// every loop, each frame that is not before the hold frame is made later by the stretch
/// from the loop start to the hold frame.
pub fn later_windows(
    mut windows: [[i16; 2]; 10],
    loop_start: i16,
    hold: i16,
    loops: i16,
) -> [[i16; 2]; 10] {
    let stretch = hold.wrapping_sub(loop_start);
    for _ in 0..loops.max(0) {
        for frame in windows.iter_mut().flatten() {
            // The hold frame is compared as an unsigned half word.
            if i32::from(*frame) >= i32::from(hold as u16) {
                *frame = frame.wrapping_add(stretch);
            }
        }
    }
    windows
}

impl Player {
    /// The start of a fight (`0x800AB5A4`, order state 7): the place is kept, he is
    /// carried about it by his animations from now on, and the free player leaves for
    /// the states of a fight in its next game frame. Nothing when a fight is on already.
    pub fn keep_place(&mut self) {
        if self.flags & flag::ARMS == 0 {
            self.flags |= flag::ARMS | flag::PUT;
            self.base = self.position;
        }
    }

    /// The way back (`0x800AB614`): the fight is over for the machine, with a pose behind
    /// the weapon put away or without, and he stands on the kept place. Nothing when no
    /// fight is on.
    pub fn back_to_place(&mut self, pose: bool) {
        if self.flags & flag::ARMS == 0 {
            return;
        }
        match pose {
            true => self.flags |= flag::POSE,
            false => self.flags &= !flag::POSE,
        }
        self.flags &= !flag::ARMS;
        (self.position, self.before) = (self.base, self.base);
    }

    /// The tear-down of a fight (`0x800ABA54`): he is not carried about the kept place
    /// any more.
    pub fn end_carrying(&mut self) {
        self.flags &= !flag::PUT;
    }

    /// A command or a reaction (`0x800AB3BC`, order state 2): the wanted action with the
    /// command's target mask and parameter. False, with nothing done, while the action
    /// wanted before is not played out.
    pub fn request(&mut self, action: i16, own: bool, targets: u32, parameter: u32) -> bool {
        if self.flags & flag::BUSY != 0 {
            return false;
        }
        match own {
            true => self.flags |= flag::OWN,
            false => self.flags &= !flag::OWN,
        }
        self.ordered[0] = action;
        (self.fight.targets, self.fight.parameter) = (targets, parameter);
        self.flags &= !(flag::THROUGH | flag::GO_ON);
        self.flags |= flag::BUSY;
        true
    }

    /// The stance of a defence stays behind the action or not (`0x800AB434`,
    /// `0x800AB45C`): set for kind 18 of a command, cleared for every other.
    pub fn set_guard(&mut self, on: bool) {
        match on {
            true => self.flags |= flag::GUARD,
            false => self.flags &= !flag::GUARD,
        }
    }

    /// The same for kind 16 (`0x800AB484`, `0x800AB4AC`).
    pub fn set_stance(&mut self, on: bool) {
        match on {
            true => self.flags |= flag::STANCE,
            false => self.flags &= !flag::STANCE,
        }
    }

    /// A pose follows the move in place of the weapon drawn again (`0x800AC510`): set for
    /// kind 26 of a command, cleared for every other.
    pub fn set_pose_behind(&mut self, on: bool) {
        match on {
            true => self.flags |= flag::POSE_BEHIND,
            false => self.flags &= !flag::POSE_BEHIND,
        }
    }

    /// Whether the wanted action is in memory and waits for "go" (`0x800AB4FC`).
    pub fn at_the_hold(&self) -> bool {
        self.state == state::AT_THE_HOLD
    }

    /// Whether the wanted action is played out (`0x800AB4D4`).
    pub fn played_out(&self) -> bool {
        self.flags & flag::BUSY == 0
    }

    /// Whether the states of a fight are through (`0x800ABA2C`).
    pub fn through(&self) -> bool {
        self.flags & flag::THROUGH != 0
    }

    /// Free again (`0x800AA5C0`): he stands, in mode 0.
    fn leave(&mut self, moves: &[Move]) {
        self.start(moves, super::state::STANDS, act::STAND, 0);
        self.mode = 0;
        self.flags |= flag::THROUGH;
    }

    fn sign(&mut self, sign: Sign) {
        self.fight.signs.push(sign);
    }

    /// The effect of his own command by the wanted action (`0x800ABEC4`).
    fn pick_effect(&self) -> Option<Effect> {
        let parameter = self.fight.parameter;
        match self.ordered[0] {
            action::SPELL => Some(Effect::Spell(parameter)),
            action::CAPTURE_LONG | action::CAPTURE => Some(Effect::Capture(parameter)),
            action::SUMMON => Some(Effect::Summon),
            action::LEAVE => Some(Effect::Leave),
            action::ATTACK => Some(Effect::Attack),
            action::ITEM => Some(Effect::Item(parameter)),
            _ => None,
        }
    }

    /// What single actions do on single frames (`0x800ABFC0`): a summon goes into the
    /// black from its loop start, a leave comes out of it from its frame 2, an item has
    /// its sound.
    fn single_frames(&mut self, played: &Move) {
        match self.action {
            action::SUMMON if self.counter == mark(played.loop_start) => {
                self.flags |= flag::INTO_BLACK;
                self.flags &= !flag::OUT_OF_BLACK;
                self.fight.black = 0;
            }
            action::LEAVE if self.counter == 2 => {
                self.flags |= flag::OUT_OF_BLACK;
                self.flags &= !flag::INTO_BLACK;
                self.fight.black = BLACK;
            }
            action::ITEM => {
                let window = mark(played.window);
                if self.counter == window {
                    self.sign(Sign::Sound(crate::sound::ITEM_SOUND));
                }
                // The game compares with the difference as it is: a pair before frame 10
                // names no frame.
                if window >= 10 && self.counter == window - 10 {
                    self.sign(Sign::BeforeItem);
                }
            }
            _ => {}
        }
    }

    /// The effects of the wanted action start (`0x800AA600`): nothing for a reaction.
    fn start_effects(&mut self, played: &Move) {
        let Some(effect) = self.fight.effect else {
            return;
        };
        let capture = matches!(self.action, action::CAPTURE_LONG | action::CAPTURE);
        self.sign(Sign::Effects {
            effect,
            loop_start: played.loop_start,
            hold: played.hold,
            loops: if capture { 4 } else { self.fight.loops },
            targets: self.fight.targets,
        });
    }

    /// The black of the player's draw routine (`0x800A9894` to `0x800A9910`), once a
    /// game frame: the word his model is drawn with, `None` when it is not drawn at all
    /// (the share is whole). While he goes into the black or comes out of it the model
    /// is drawn blended in mode 1 with the share of black as it stood, and the share
    /// then moves by [`BLACK_STEP`]: up to the whole, or down to 0, where the way out is
    /// over (`0x800AB320`).
    pub fn draw_word(&mut self) -> Option<u32> {
        let mut word = 0;
        if self.flags & (flag::INTO_BLACK | flag::OUT_OF_BLACK) != 0 {
            word = IN_BLACK | (self.fight.black as u32) << 16;
        }
        if self.flags & flag::INTO_BLACK != 0 {
            self.fight.black = (self.fight.black + BLACK_STEP).min(BLACK);
        } else if self.flags & flag::OUT_OF_BLACK != 0 {
            self.fight.black -= BLACK_STEP;
            if self.fight.black < 0 {
                self.out_of_black();
            }
        }
        (self.fight.black != BLACK).then_some(word)
    }

    /// The weapon is put away behind the fight: his standing action when he is in the
    /// black, else the action that puts it away.
    fn put_away_behind(&mut self, moves: &[Move]) {
        let action = match self.flags & flag::INTO_BLACK != 0 {
            true => act::STAND,
            false => action::PUT_AWAY,
        };
        self.start(moves, state::ENDS, action, 0);
        self.flags &= !flag::HOLD;
    }

    /// Mode 1 (`0x800AA6EC`): one game frame of the states of a fight.
    pub(super) fn arms(&mut self, moves: &[Move]) {
        let Some(played) = self.current(moves) else {
            return;
        };
        match self.state {
            state::WAITS_FOR_VIEW => {
                if self.fight.camera_state == OPENING_VIEW {
                    self.start(moves, state::DRAWS, action::DRAW, 0);
                }
            }
            state::DRAWS | state::SETTLES => {
                if self.state == state::DRAWS {
                    // The fight was over before the weapon was out.
                    if self.flags & (flag::ARMS | flag::POSE) == 0 {
                        self.sign(Sign::WeaponAway);
                        self.leave(moves);
                        return;
                    }
                    if self.counter == mark(played.window) {
                        self.sign(Sign::WeaponInHand);
                    }
                }
                if self.counter >= mark(played.loop_start) {
                    self.state = state::READY;
                }
            }
            state::READY => {
                self.flags |= flag::HOLD;
                self.flags &= !flag::LOOPED;
                if self.flags & flag::ARMS != 0 {
                    if self.flags & flag::BUSY != 0 {
                        self.state = state::ASKS;
                    }
                    return;
                }
                // The fight is over.
                if self.flags & flag::INTO_BLACK != 0 {
                    self.sign(Sign::WeaponAwayWhole);
                }
                match self.flags & flag::POSE != 0 {
                    true => self.start(moves, state::LOADS_POSE, -1, 0),
                    false => self.put_away_behind(moves),
                }
            }
            state::ASKS => {
                self.state = state::LOADS;
                self.fight.effect = match self.flags & flag::OWN != 0 {
                    true => self.pick_effect(),
                    false => None,
                };
            }
            state::LOADS => {
                let pose = self.flags & flag::POSE_BEHIND != 0;
                let there = self.in_memory(moves, self.ordered[0])
                    && (!pose || self.in_memory(moves, action::POSE));
                if there {
                    self.state = state::AT_THE_HOLD;
                }
            }
            state::AT_THE_HOLD => {
                let wanted = self.ordered[0];
                if self.flags & flag::GO_ON != 0 {
                    let Some(row) = self.played(moves, wanted) else {
                        return;
                    };
                    if row.bare & 1 != 0 {
                        self.start(moves, state::PUTS_AWAY, action::PUT_AWAY, 0);
                        self.flags &= !flag::HOLD;
                    } else if wanted == self.action {
                        self.start(moves, state::STARTS, -1, 0);
                    } else {
                        self.start(moves, state::STARTS, wanted, 0);
                    }
                    self.flags &= !flag::GO_ON;
                }
                self.fight.loops = match wanted {
                    action::SPELL | action::SUMMON | action::LEAVE => 2,
                    action::CAPTURE_LONG | action::CAPTURE => 30,
                    _ => 0,
                };
            }
            state::PUTS_AWAY => {
                if self.counter == mark(played.window) {
                    self.sign(Sign::WeaponAway);
                }
                if self.counter == mark(played.end) {
                    self.start(moves, state::STARTS, self.ordered[0], 0);
                }
            }
            state::STARTS => {
                self.single_frames(played);
                if matches!(self.action, action::FALLS | action::FALLS_OTHER) {
                    self.flags &= !flag::BUSY;
                    self.flags |= flag::HOLD;
                    self.start(moves, state::LIES, self.action, 0);
                } else {
                    self.state = state::PLAYS;
                }
                self.start_effects(played);
            }
            state::PLAYS => {
                self.flags &= !flag::HOLD;
                self.single_frames(played);
                let looped = matches!(
                    self.action,
                    action::CAPTURE_LONG
                        | action::CAPTURE
                        | action::SPELL
                        | action::SUMMON
                        | action::LEAVE
                );
                if looped && self.counter >= mark(played.hold) {
                    if self.fight.loops_end {
                        self.fight.loops = -1;
                    } else {
                        self.fight.loops = self.fight.loops.wrapping_sub(1);
                        if self.fight.loops >= 0 {
                            self.counter = mark(played.loop_start);
                        }
                    }
                }
                if self.counter == mark(played.end) {
                    self.played_to_the_end(moves);
                }
            }
            state::LOADS_POSE => {
                if self.in_memory(moves, action::POSE) {
                    self.put_away_behind(moves);
                }
            }
            state::ENDS => {
                if self.counter == mark(played.window) {
                    self.sign(Sign::WeaponAwayWhole);
                }
                if self.counter >= mark(played.end) {
                    self.flags &= !flag::HOLD;
                    match self.flags & flag::POSE != 0 {
                        true => self.start(moves, state::POSES, action::POSE, 0),
                        false => self.leave(moves),
                    }
                }
            }
            state::POSES => {
                if self.counter >= mark(played.end) {
                    self.leave(moves);
                }
            }
            state::LIES if self.flags & flag::HOLD == 0 => self.leave(moves),
            _ => {}
        }
    }

    /// The end of the wanted action in state 15: back into a stance or the ready stance,
    /// or the weapon drawn again behind an action without it.
    fn played_to_the_end(&mut self, moves: &[Move]) {
        // The game looks this byte up with the lowest bit of the weapon's group in place
        // of the group; no action's byte differs between the two rows.
        let bare = (self.played(moves, self.ordered[0])).map_or(0, |row| row.bare);
        if bare == 0 {
            let stance = match self.flags {
                flags if flags & flag::GUARD != 0 => Some(action::GUARD),
                flags if flags & flag::STANCE != 0 => Some(action::STANCE),
                _ => None,
            };
            match stance {
                Some(stance) if self.action == stance => self.start(moves, state::READY, -1, 0),
                Some(stance) => self.start(moves, state::SETTLES, stance, 0),
                None => {
                    let ready = self.played(moves, action::DRAW);
                    let hold = ready.map_or(0, |ready| mark(ready.hold));
                    self.start(moves, state::READY, action::DRAW, hold);
                }
            }
        } else if self.flags & flag::POSE_BEHIND != 0 {
            self.start(moves, state::POSES, action::POSE, 0);
        } else {
            self.start(moves, state::DRAWS, action::DRAW, 0);
        }
        self.flags &= !flag::BUSY;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::Ground;
    use crate::mover::ArcTangent;
    use crate::player::{Orders, order_state};

    /// A row: its loop start, hold frame, end, the first frame of its first pair, the
    /// byte at +3 and the forward travel of every frame up to the end.
    fn row(loop_start: i16, hold: i16, end: i16, window: i16, bare: u8, far: i16) -> Move {
        // Forward and back again: `far` at the frame in the middle.
        let middle = end / 2;
        let forward = |frame: i16| match middle {
            0 => 0,
            middle => far - (far / middle) * (frame - middle).abs(),
        };
        Move {
            first: 0,
            loop_start,
            hold,
            end,
            window,
            bare,
            divisor: 32,
            travel: (0..=end).map(|frame| [0, 0, forward(frame)]).collect(),
        }
    }

    /// Thirty rows of the first weapon group, the ones a test needs with frames of
    /// their own.
    fn moves() -> Vec<Move> {
        let mut moves = vec![row(0, 5, 5, 0, 1, 0); 30];
        moves[action::CAPTURE as usize] = row(2, 4, 6, 4, 1, 0);
        moves[action::POSE as usize] = row(0, 4, 4, 0, 1, 0);
        moves[13] = row(0, 4, 4, 4, 1, 0);
        moves[action::DRAW as usize] = row(4, 7, 7, 2, 1, 0);
        moves[action::PUT_AWAY as usize] = row(0, 5, 5, 3, 0, 0);
        moves[action::ATTACK as usize] = row(0, 6, 6, 2, 0, 960);
        moves[17] = row(0, 3, 3, 0, 0, 0);
        moves[action::FALLS as usize] = row(3, 4, 4, 0, 0, 0);
        moves[action::GUARD as usize] = row(2, 4, 4, 2, 0, 0);
        moves[action::SPELL as usize] = row(2, 4, 7, 5, 0, 0);
        moves
    }

    #[test]
    fn the_weapon_brings_the_animations_its_group_keeps_in_memory() {
        use crate::action::{TAIL, WINDOWS};
        let plain = |animation: usize, resident: u8| Action {
            animation: Some(animation),
            resident,
            extra: [0; 2],
            first: 0,
            loop_start: 0,
            hold: 0,
            end: 0,
            windows: [[-1; 2]; WINDOWS],
            tail: [-1; TAIL],
        };
        // Row n plays animation n; no row asks, but for the ones set below.
        let mut actions: Vec<Action> = (0..14 + 2 * GROUP_ROWS).map(|row| plain(row, 0)).collect();
        // A free action, the draw and the guard of group 0, and the draw of group 1.
        for row in [3, 14, 20, 30] {
            actions[row].resident = 1;
        }
        // A row that asks and has no animation brings none.
        actions[31] = Action {
            animation: None,
            ..plain(0, 1)
        };
        assert_eq!(read_with_weapon(&actions, 0), [14, 20]);
        assert_eq!(read_with_weapon(&actions, 1), [30]);
        // A group the table does not have.
        assert!(read_with_weapon(&actions, 2).is_empty());
    }

    #[test]
    fn the_two_times_come_from_the_row_of_the_action() {
        let moves = moves();
        assert_eq!(wanted(0, 0), Some(action::ATTACK));
        assert_eq!(wanted(11, 0), Some(action::FALLS));
        assert_eq!([wanted(20, 0), wanted(25, 0), wanted(27, 0)], [None; 3]);
        assert_eq!([wanted(5, 0), wanted(26, 0)], [Some(action::CAPTURE); 2]);
        // With "Capture FX", the setting of a new game, the capture is the long one,
        // and no other kind changes.
        let on = crate::window::SETTING_AT_START;
        assert_ne!(on & CAPTURE_FX, 0);
        assert_eq!(
            [wanted(5, on), wanted(26, on)],
            [Some(action::CAPTURE_LONG); 2]
        );
        assert_eq!(wanted(5, on & !CAPTURE_FX), Some(action::CAPTURE));
        for kind in (0..27).filter(|kind| ![5, 26].contains(kind)) {
            assert_eq!(wanted(kind, on), wanted(kind, 0), "kind {kind}");
        }
        // The first frame of the first pair and the end.
        assert_eq!(times(&moves, 0, action::ATTACK), Some([2, 6]));
        // A spell: both get twice the stretch from the loop start to the hold frame.
        assert_eq!(times(&moves, 0, action::SPELL), Some([5 + 4, 7 + 4]));
        // The capture: the end of action 14 in front, and thirty loops in E.
        assert_eq!(times(&moves, 0, action::CAPTURE), Some([7 + 4, 7 + 6 + 60]));
        assert_eq!(times(&moves, 0, 30), None);
    }

    fn pass(player: &mut Player, moves: &[Move]) {
        let arc = ArcTangent::of(vec![0; 1025]);
        player.update(moves, &Ground::default(), &arc, 0);
    }

    /// Game frames until the state is the wanted one, with every sign on the way.
    fn until(player: &mut Player, moves: &[Move], state: i32) -> (u32, Vec<Sign>) {
        let mut signs = Vec::new();
        for frames in 1..200 {
            pass(player, moves);
            signs.extend(player.fight.signs.iter().copied());
            if player.state == state {
                return (frames, signs);
            }
        }
        panic!("state {state} never came: {player:?}");
    }

    /// A player that stands ready in a fight, at a place.
    fn ready(moves: &[Move]) -> Player {
        let mut player = Player::new([100, 0, 200], 0, moves);
        player.fight.camera_state = OPENING_VIEW;
        player.keep_place();
        until(&mut player, moves, state::READY);
        player
    }

    #[test]
    fn he_draws_the_weapon_in_the_opening_view_and_stands_ready() {
        let moves = moves();
        let mut player = Player::new([100, 0, 200], 0, &moves);
        player.keep_place();
        // The free player leaves for state 7, which waits for the camera.
        pass(&mut player, &moves);
        assert_eq!((player.mode, player.state), (1, state::WAITS_FOR_VIEW));
        pass(&mut player, &moves);
        assert_eq!((player.state, player.action), (7, act::STAND));
        player.fight.camera_state = OPENING_VIEW;
        pass(&mut player, &moves);
        assert_eq!((player.state, player.action, player.counter), (9, 14, 0));
        // The weapon goes into the hand on the pair's frame, the loop start is "ready".
        let (frames, signs) = until(&mut player, &moves, state::READY);
        assert_eq!((frames, player.counter), (4, 4));
        assert_eq!(signs, [Sign::WeaponInHand]);
        // Ready is the loop of the action between its loop start and its hold frame.
        let seen: Vec<u32> = (0..8)
            .map(|_| {
                pass(&mut player, &moves);
                player.counter
            })
            .collect();
        assert_eq!(seen, [5, 6, 4, 5, 6, 4, 5, 6]);
        assert_eq!((player.state, player.action), (state::READY, 14));
        assert!(player.flags & flag::HOLD != 0);
        assert!(player.played_out() && !player.at_the_hold() && !player.through());
    }

    /// A table like the game's, which the test needs only to be near the true angle.
    fn arc() -> ArcTangent {
        use std::f64::consts::TAU;
        let step = |ratio: usize| ((ratio as f64 / 1024.0).atan() * 4096.0 / TAU).round() as i16;
        ArcTangent::of((0..1025).map(step).collect())
    }

    /// Game frames as a fight plays them, the order states and then the controller with
    /// the command word they made, until the order state is the wanted one.
    fn ordered(
        orders: &mut Orders,
        player: &mut Player,
        moves: &[Move],
        state: i16,
    ) -> (u32, Vec<Option<u32>>) {
        let (arc, mut commands) = (arc(), Vec::new());
        for frames in 1..400 {
            orders.pass(player, &arc);
            commands.push(orders.command);
            if let Some(command) = orders.command {
                player.command = command;
            }
            player.update(moves, &Ground::default(), &arc, frames);
            if orders.state == state {
                return (frames, commands);
            }
        }
        panic!("order state {state} never came: {orders:?} {player:?}");
    }

    #[test]
    fn the_order_states_of_a_fight_hand_a_move_to_the_machine() {
        let moves = moves();
        let mut player = Player::new([100, 0, 200], 0, &moves);
        player.fight.camera_state = OPENING_VIEW;
        let mut orders = Orders::default();
        // The set-up: state 7 keeps the place and is state 1 behind its one game frame.
        orders.start_fight([100, 0, 1200]);
        let (frames, commands) = ordered(&mut orders, &mut player, &moves, order_state::WAITS);
        assert_eq!((frames, commands), (1, vec![Some(0)]));
        assert!(orders.fight.held && orders.fight.on);
        assert!(player.flags & flag::ARMS != 0);
        assert_eq!(player.base, [100, 0, 200]);
        until(&mut player, &moves, state::READY);

        // His own command: state 2 hands it over, state 3 waits for the hold, and the
        // runner's test sees state 4 one game frame behind the hold.
        orders.face([100, 0, 1200]);
        orders.ask(Some(action::ATTACK), true, 0x10, 0);
        assert_eq!(orders.state, order_state::ASKED);
        assert!(!orders.at_the_ready());
        let (frames, commands) = ordered(&mut orders, &mut player, &moves, order_state::READY);
        // The states 1, 2, 4 and 6 leave the command word alone, state 3 makes one.
        assert_eq!(
            (frames, commands),
            (4, vec![None, Some(0), Some(0), Some(0)])
        );
        assert!(player.at_the_hold() && orders.at_the_ready() && !orders.fight.asked);
        assert_eq!(
            (player.fight.targets, player.flags & flag::OWN),
            (0x10, flag::OWN)
        );
        // Without "go" nothing moves on.
        orders.pass(&mut player, &arc());
        assert_eq!(
            (orders.state, player.flags & flag::GO_ON),
            (order_state::READY, 0)
        );
        orders.go();
        ordered(&mut orders, &mut player, &moves, order_state::WENT);
        assert_eq!(
            (player.state, player.action),
            (state::STARTS, action::ATTACK)
        );
        assert!(!orders.fight.go);
        // State 6 keeps his own command until it is played out.
        ordered(&mut orders, &mut player, &moves, order_state::WAITS);
        assert!(player.played_out() && !orders.fight.own);
        assert_eq!((player.state, player.action), (state::READY, action::DRAW));

        // A reaction is state 1 again right behind its "go".
        orders.ask(Some(17), false, 0, 0);
        ordered(&mut orders, &mut player, &moves, order_state::READY);
        orders.go();
        let (frames, _) = ordered(&mut orders, &mut player, &moves, order_state::WAITS);
        assert_eq!(frames, 2);
        assert!(!player.played_out());
        until(&mut player, &moves, state::READY);

        // A kind that plays nothing only makes him ready (state 5), and its "go" brings
        // him back to state 1 with nothing for the machine to play.
        orders.ask(None, false, 0, 0);
        let (frames, commands) = ordered(&mut orders, &mut player, &moves, order_state::READY);
        assert_eq!((frames, commands), (1, vec![Some(0)]));
        assert!(player.played_out());
        orders.go();
        let (frames, _) = ordered(&mut orders, &mut player, &moves, order_state::WAITS);
        assert_eq!(frames, 2);
        assert_eq!((player.state, player.action), (state::READY, action::DRAW));

        // The way back: the fight holds him no more, and a script that has him has
        // him again.
        orders.end_fight();
        orders.taken = true;
        ordered(&mut orders, &mut player, &moves, 0);
        let (frames, _) = ordered(&mut orders, &mut player, &moves, order_state::TAKEN);
        assert_eq!(frames, 1);
    }

    #[test]
    fn the_order_states_of_a_fight_turn_him_to_the_place() {
        let moves = moves();
        let mut player = ready(&moves);
        let mut orders = Orders::default();
        let off = |player: &Player| i32::from(player.angle) - 1024;
        // A place a quarter turn to his side. State 8 turns him to within 64 of it.
        orders.face([1100, 0, 200]);
        let (frames, commands) = ordered(&mut orders, &mut player, &moves, order_state::WAITS);
        assert!(frames > 2, "{frames}");
        assert_eq!(commands[0], Some(crate::player::command::RISES));
        assert_eq!(commands.last(), Some(&Some(0)));
        assert!(off(&player).abs() < 200, "{}", player.angle);
        assert_eq!(player.position, [100, 0, 200]);
        // From the other side the angle falls.
        player.angle = 2000;
        orders.face([1100, 0, 200]);
        let (_, commands) = ordered(&mut orders, &mut player, &moves, order_state::WAITS);
        assert_eq!(commands[0], Some(crate::player::command::FALLS));
        // State 5 turns to within 96, and is ready there; state 3 the same, with the
        // move at its hold.
        player.angle = 0;
        orders.ask(None, false, 0, 0);
        let (frames, commands) = ordered(&mut orders, &mut player, &moves, order_state::READY);
        assert!(frames > 2 && commands[0] == Some(crate::player::command::RISES));
        player.angle = 0;
        orders.ask(Some(action::ATTACK), true, 0x10, 0);
        let (frames, _) = ordered(&mut orders, &mut player, &moves, order_state::READY);
        assert!(frames > 4, "{frames}");
        assert!(player.at_the_hold() && off(&player).abs() < 200);
    }

    #[test]
    fn an_attack_is_asked_held_let_go_and_carries_him_there_and_back() {
        let moves = moves();
        let mut player = ready(&moves);
        assert!(player.request(action::ATTACK, true, 0x10, 0));
        // A second request is refused while the first is not played out.
        assert!(!player.request(17, false, 0, 0));
        let (frames, _) = until(&mut player, &moves, state::AT_THE_HOLD);
        assert_eq!(frames, 3);
        assert!(player.at_the_hold() && !player.played_out());
        // Without "go" he goes round in the ready stance.
        pass(&mut player, &moves);
        assert_eq!((player.state, player.action), (state::AT_THE_HOLD, 14));
        player.go_on(false);
        pass(&mut player, &moves);
        assert_eq!((player.state, player.action, player.counter), (16, 16, 0));
        assert_eq!(player.flags & flag::GO_ON, 0);
        assert_eq!(player.position, [100, 0, 200]);
        // The effects start in the game frame behind.
        pass(&mut player, &moves);
        assert_eq!(player.state, state::PLAYS);
        let effects = Sign::Effects {
            effect: Effect::Attack,
            loop_start: 0,
            hold: 6,
            loops: 0,
            targets: 0x10,
        };
        assert_eq!(player.fight.signs, [effects]);
        // The travel of the animation carries him: 960 over the divisor at frame 3.
        let mut furthest = (0, 0);
        while player.state == state::PLAYS {
            furthest = furthest.max((player.position[2], player.counter));
            pass(&mut player, &moves);
        }
        assert_eq!(furthest, (230, 3));
        // At the end: the ready stance at its hold frame, on the kept place.
        assert_eq!((player.state, player.action), (state::READY, 14));
        assert_eq!(player.position, [100, 0, 200]);
        assert!(player.played_out());
    }

    #[test]
    fn an_action_without_the_weapon_puts_it_away_first_and_draws_it_again() {
        let moves = moves();
        let mut player = ready(&moves);
        assert!(player.request(13, true, 0, 0));
        until(&mut player, &moves, state::AT_THE_HOLD);
        player.go_on(false);
        pass(&mut player, &moves);
        assert_eq!((player.state, player.action), (state::PUTS_AWAY, 15));
        assert_eq!(player.flags & flag::HOLD, 0);
        let (frames, signs) = until(&mut player, &moves, state::STARTS);
        assert_eq!((frames, player.action, player.counter), (5, 13, 0));
        assert_eq!(signs, [Sign::WeaponAway]);
        // Behind it he draws again; a command without an effect of its own starts none.
        let (frames, signs) = until(&mut player, &moves, state::DRAWS);
        assert_eq!((frames, player.action, signs), (4, 14, vec![]));
        assert!(player.played_out());
        let (_, signs) = until(&mut player, &moves, state::READY);
        assert_eq!(signs, [Sign::WeaponInHand]);

        // With the pose behind the move: action 10 in place of the weapon, then free.
        player.set_pose_behind(true);
        assert!(player.request(action::CAPTURE, true, 0x20, 3));
        until(&mut player, &moves, state::AT_THE_HOLD);
        pass(&mut player, &moves);
        assert_eq!(player.fight.loops, 30);
        player.go_on(false);
        let (_, signs) = until(&mut player, &moves, state::PLAYS);
        let effects = Sign::Effects {
            effect: Effect::Capture(3),
            loop_start: 2,
            hold: 4,
            loops: 4,
            targets: 0x20,
        };
        assert_eq!(signs, [Sign::WeaponAway, effects]);
        // The capture goes round between its loop start and its hold frame.
        for _ in 0..20 {
            pass(&mut player, &moves);
            assert!((2..4).contains(&player.counter), "{}", player.counter);
        }
        assert_eq!(player.fight.loops, 21);
        // Until the word that ends the loops stands.
        player.fight.loops_end = true;
        let (frames, _) = until(&mut player, &moves, state::POSES);
        assert!(frames <= 5, "{frames}");
        assert_eq!(player.action, action::POSE);
        let (frames, _) = until(&mut player, &moves, super::super::state::STANDS);
        assert_eq!(frames, 4);
        assert!(player.through());
        assert_eq!((player.mode, player.action), (0, act::STAND));
    }

    #[test]
    fn a_spell_plays_its_loop_three_times() {
        let moves = moves();
        let mut player = ready(&moves);
        assert!(player.request(action::SPELL, true, 0x10, 9));
        until(&mut player, &moves, state::AT_THE_HOLD);
        pass(&mut player, &moves);
        assert_eq!(player.fight.loops, 2);
        player.go_on(false);
        pass(&mut player, &moves);
        assert_eq!((player.state, player.counter), (state::STARTS, 0));
        // The end 7 and twice the stretch from 2 to 4.
        let (frames, signs) = until(&mut player, &moves, state::READY);
        assert_eq!(frames, 11);
        let effects = Sign::Effects {
            effect: Effect::Spell(9),
            loop_start: 2,
            hold: 4,
            loops: 2,
            targets: 0x10,
        };
        assert_eq!(signs, [effects]);
        // The pairs as the sequencer gets them: a frame from the hold frame on is later.
        let mut windows = [[0; 2]; 10];
        windows[0] = [3, 5];
        let later = later_windows(windows, 2, 4, 2);
        assert_eq!(later[0], [3, 9]);
        assert_eq!(later[1], [0, 0]);
        // Group 0 of the disc: the spell's pair at 49, loop 29 to 44, is K 79.
        windows[0] = [49, 43];
        assert_eq!(later_windows(windows, 29, 44, 2)[0], [79, 43]);
        assert_eq!(later_windows(windows, 29, 44, 0)[0], [49, 43]);
    }

    #[test]
    fn a_defence_stays_in_its_action_until_the_next_request() {
        let moves = moves();
        let mut player = ready(&moves);
        player.set_guard(true);
        assert!(player.request(action::GUARD, true, 0, 0));
        until(&mut player, &moves, state::AT_THE_HOLD);
        player.go_on(false);
        let (_, signs) = until(&mut player, &moves, state::READY);
        assert_eq!((player.action, signs), (action::GUARD, vec![]));
        for _ in 0..6 {
            pass(&mut player, &moves);
            assert_eq!((player.state, player.action), (state::READY, 20));
            assert!((2..4).contains(&player.counter), "{}", player.counter);
        }
        // Hurt while he guards, the flag still standing: back into the stance.
        assert!(player.request(17, false, 0, 0));
        until(&mut player, &moves, state::AT_THE_HOLD);
        player.go_on(false);
        pass(&mut player, &moves);
        assert_eq!((player.state, player.action), (state::STARTS, 17));
        let (_, signs) = until(&mut player, &moves, state::SETTLES);
        assert_eq!((player.action, signs), (action::GUARD, vec![]));
        until(&mut player, &moves, state::READY);
        // The same without the flag: the ready stance at its hold frame.
        player.set_guard(false);
        assert!(player.request(17, false, 0, 0));
        until(&mut player, &moves, state::AT_THE_HOLD);
        player.go_on(false);
        until(&mut player, &moves, state::READY);
        assert_eq!(player.action, action::DRAW);
        // An action he is in already only goes on.
        player.set_guard(true);
        player.request(action::GUARD, true, 0, 0);
        until(&mut player, &moves, state::AT_THE_HOLD);
        player.go_on(false);
        until(&mut player, &moves, state::READY);
        player.request(action::GUARD, true, 0, 0);
        until(&mut player, &moves, state::AT_THE_HOLD);
        let counter = player.counter;
        player.go_on(false);
        pass(&mut player, &moves);
        assert_eq!((player.state, player.action), (state::STARTS, 20));
        assert_ne!(player.counter, 0, "from {counter}");
    }

    #[test]
    fn a_fall_lies_until_the_hold_is_taken_off() {
        let moves = moves();
        let mut player = ready(&moves);
        assert!(player.request(action::FALLS, false, 0, 0));
        until(&mut player, &moves, state::AT_THE_HOLD);
        player.go_on(false);
        until(&mut player, &moves, state::LIES);
        // Played out at once, and held on the loop of its last frames.
        assert!(player.played_out());
        for _ in 0..10 {
            pass(&mut player, &moves);
            assert_eq!((player.state, player.action), (state::LIES, 19));
        }
        assert!((3..=4).contains(&player.counter));
        player.back_to_place(false);
        player.let_go();
        pass(&mut player, &moves);
        assert!(player.through());
        assert_eq!((player.mode, player.action), (0, act::STAND));
    }

    #[test]
    fn behind_the_fight_the_weapon_is_put_away_with_a_pose_or_without() {
        let moves = moves();
        for pose in [false, true] {
            let mut player = ready(&moves);
            player.position = [130, 0, 260];
            player.back_to_place(pose);
            assert_eq!(player.position, [100, 0, 200]);
            let (_, signs) = until(&mut player, &moves, state::ENDS);
            assert_eq!((player.action, signs), (action::PUT_AWAY, vec![]));
            let after = match pose {
                true => state::POSES,
                false => super::super::state::STANDS,
            };
            let (frames, signs) = until(&mut player, &moves, after);
            assert_eq!((frames, signs), (5, vec![Sign::WeaponAwayWhole]));
            if pose {
                assert_eq!(player.action, action::POSE);
                assert!(!player.through());
                let (frames, _) = until(&mut player, &moves, super::super::state::STANDS);
                assert_eq!(frames, 4);
            }
            assert!(player.through());
            assert_eq!((player.mode, player.action), (0, act::STAND));
            // Still carried until the tear-down: the free player decides nothing.
            player.command = super::super::command::RUN;
            for _ in 0..6 {
                pass(&mut player, &moves);
                assert_eq!(player.state, super::super::state::STANDS);
            }
            player.end_carrying();
            pass(&mut player, &moves);
            assert_eq!(player.state, super::super::state::RUNS);
            // A new request takes the "through" mark off.
            player.request(action::ATTACK, true, 0, 0);
            assert!(!player.through());
        }
    }

    #[test]
    fn a_summon_goes_into_the_black_and_ends_the_fight_without_the_action() {
        let mut moves = moves();
        moves[action::SUMMON as usize] = row(2, 4, 4, 4, 0, 0);
        let mut player = ready(&moves);
        assert!(player.request(action::SUMMON, true, 0, 0));
        until(&mut player, &moves, state::AT_THE_HOLD);
        player.go_on(false);
        until(&mut player, &moves, state::READY);
        assert!(player.flags & flag::INTO_BLACK != 0);
        assert_eq!(player.fight.black, 0);
        // In the black the meshes go back at once and he stands in place of the action.
        player.back_to_place(false);
        pass(&mut player, &moves);
        assert_eq!(player.fight.signs, [Sign::WeaponAwayWhole]);
        assert_eq!((player.state, player.action), (state::ENDS, act::STAND));
    }

    #[test]
    fn the_black_moves_by_128_a_game_frame_and_ends_the_drawing() {
        let moves = moves();
        let mut player = ready(&moves);
        // Outside the black the word is plain and nothing moves.
        assert_eq!((player.draw_word(), player.fight.black), (Some(0), 0));
        // Into the black: drawn blended with the share as it stood, 31 times, and not
        // in the game frame the share is whole.
        player.flags |= flag::INTO_BLACK;
        assert_eq!(player.draw_word(), Some(0xB0));
        assert_eq!(player.draw_word(), Some(0xB0 | 128 << 16));
        for _ in 2..31 {
            assert!(player.draw_word().is_some());
        }
        assert_eq!((player.draw_word(), player.fight.black), (None, 4096));
        assert_eq!(player.draw_word(), None);
        // Out of it, as a leave sets it: the share falls, and below 0 the black is over.
        player.flags = player.flags & !flag::INTO_BLACK | flag::OUT_OF_BLACK;
        assert_eq!(player.draw_word(), Some(0xB0 | 4096 << 16));
        for _ in 1..32 {
            assert!(player.flags & flag::OUT_OF_BLACK != 0);
            player.draw_word();
        }
        assert_eq!(player.fight.black, 0);
        assert_eq!(player.draw_word(), Some(0xB0));
        assert_eq!(player.flags & (flag::INTO_BLACK | flag::OUT_OF_BLACK), 0);
        assert_eq!(player.draw_word(), Some(0));
        // Showing him ends the black wherever it stands.
        player.flags |= flag::INTO_BLACK;
        player.fight.black = 4096;
        player.set_shown(true);
        assert_eq!(player.draw_word(), Some(0));
    }
}
