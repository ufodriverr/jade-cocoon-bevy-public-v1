
use std::collections::BTreeMap;
use std::fs::File;
use std::sync::Arc;

use jc_disc::{Archive, DiscImage, Executable};
use jc_formats::action::{Action, Point};
use jc_formats::animation::{Animation, EventAction, Playback, Span, WHOLE};
use jc_formats::command::Runner;
use jc_formats::field::{Ground, Region, RoomData};
use jc_formats::mesh::{Kept, MeshBlock, MeshObject};
use jc_formats::model::{self, ModelTable, PartTables, Worn};
use jc_formats::mover::{ArcTangent, Mover};
use jc_formats::player::arms::{self, Sign};
use jc_formats::player::{self, Held, InTheWay, Move, Player, Thing, flag};
use jc_formats::rooms::{FIELD_ENGINE_BASE, FIELD_ENGINE_FILE};
use jc_formats::scene::Exit;
use jc_formats::skeleton::{
    HUMAN_SKELETON, MODEL_LIST, MODEL_LIST_BASE, MODEL_LIST_FILE, ModelList, Scales, Skeleton,
    rotation,
};
use jc_formats::tim::Tim;

use crate::shown::Standing;

const FREE_ACTIONS: usize = 5;

/// What `--player[=FRAME]`, `--pad=...` and `--wears=WEAPON[:ARMOUR]` ask.
#[derive(Clone, Copy)]
pub struct Asked {
    /// The game frame to stand at when one was given.
    pub frame: Option<u32>,
    /// The buttons held in every game frame, in place of the keyboard.
    pub pad: Option<Held>,
    /// Other buttons from a game frame of the run on (`--pad=NAMES;FRAME:NAMES...`).
    pub later: Later,
    /// The game frames in which the confirm button is down (`--cross=FRAME,...`).
    pub crosses: Crosses,
    /// The weapon and the armour the player has.
    pub worn: Worn,
    pub stands: Option<([i32; 3], i16)>,
}

/// The buttons that take the place of the first ones from a game frame on, in order.
pub type Later = [Option<(u32, Held)>; 256];

/// No later set at all.
pub const NO_LATER: Later = [None; 256];

/// The game frames of a room in which the confirm button is down, one game frame each.
pub type Crosses = [Option<u32>; 64];

/// No press at all.
pub const NO_CROSSES: Crosses = [None; 64];

impl Asked {
    /// The value of `--wears=WEAPON[:ARMOUR]`: two numbers of the game's weapons and
    /// armours, no armour without the second.
    pub fn wears(value: &str) -> Option<Worn> {
        let (weapon, armour) = match value.split_once(':') {
            Some((weapon, armour)) => (weapon, armour.parse().ok()?),
            None => (value, Worn::NOTHING),
        };
        Some(Worn {
            weapon: weapon.parse().ok()?,
            armour,
        })
    }

    /// The value of `--pad=NAMES[;FRAME:NAMES...]`: the buttons held from the start and,
    /// behind semicolons, up to 256 more sets with the game frame each holds from.
    pub fn pads(value: &str) -> Option<(Held, Later)> {
        let mut parts = value.split(';');
        let first = Self::pad(parts.next()?)?;
        let mut later = NO_LATER;
        for (slot, part) in parts.enumerate() {
            let (frame, names) = part.split_once(':')?;
            *later.get_mut(slot)? = Some((frame.parse().ok()?, Self::pad(names)?));
        }
        Some((first, later))
    }

    /// The value of `--cross=FRAME[,FRAME...]`: up to 64 game frames of a room.
    pub fn crosses(value: &str) -> Option<Crosses> {
        let mut crosses = NO_CROSSES;
        for (slot, frame) in value.split(',').enumerate() {
            *crosses.get_mut(slot)? = Some(frame.parse().ok()?);
        }
        Some(crosses)
    }

    /// The value of `--pad=NAME[,NAME...]`: the buttons by what they do (`player::Held`).
    /// No name at all is no button.
    pub fn pad(value: &str) -> Option<Held> {
        let mut held = Held::default();
        for name in value.split(',').filter(|name| !name.is_empty()) {
            match name {
                "forward" => held.forward = true,
                "fast" => held.fast = true,
                "run" => held.run = true,
                "back" => held.back = true,
                "left" => held.falls = true,
                "right" => held.rises = true,
                _ => return None,
            }
        }
        Some(held)
    }
}

/// What comes into a room with a player that walked through an exit.
#[derive(Clone)]
pub struct Arrival {
    pub exit: Exit,
    /// The controller's record as the player left with it.
    pub player: Player,
    pub game_frame: u32,
    pub facing: Option<i16>,
}

/// The player in a room: the controller's record and what it is drawn with.
pub struct Walker {
    /// The model in the pose of this game frame, at the player's place.
    pub body: Standing,
    pub player: Player,
    /// What the controller reads of the actions whose animation is in memory.
    pub moves: Vec<Move>,
    /// Tables 1 and 2 of the room.
    ground: Ground,
    /// The images of the model and its part, as the file places them in video memory.
    pub images: Vec<Tim>,
    /// The buttons that are held whatever the keyboard says, and the ones that take
    /// their place from a game frame on.
    pub pad: Option<Held>,
    /// The command word an order state of a script has made for the next game frame, in
    /// place of the pad's (`shown.rs`, `step_player`).
    pub ordered: Option<u32>,
    later: Later,
    crosses: Crosses,
    /// Game frames since the first room started, which is the counter of the turn filter
    /// here.
    game_frame: u32,
    exits: Vec<(Exit, Region)>,
    pub resting: Vec<u32>,
    /// The room change the warp of a forest has written, which the next ask for an exit
    /// takes, with the upright angle it gives; and that angle once it is taken.
    warp: Option<(Exit, i16)>,
    warp_facing: Option<i16>,
    /// The room's list of things in the way: its actors, as the game frame before left
    /// them (the player's task runs before theirs).
    pub things: Vec<Thing>,
    /// What the list answered in the last game frame.
    way: InTheWay,
    /// Game frames in which a thing took the movement out of the command word.
    turned_away: u32,
    /// The view of the region of the walk mesh the player stands in, which is the view an
    /// object's mask is asked about.
    stands_in: usize,
    kept: Kept,
    rested: Vec<(usize, MeshObject)>,
    /// The animations a fight has read for his moves, which go back behind it, and
    /// whether a fight of a script is on screen.
    read_for_fight: Vec<usize>,
    in_fight: bool,
    /// The animations a task has read for an action it orders of him
    /// ([`Walker::read_action`]).
    read_for_task: Vec<usize>,
    /// The fight is gone from the screen and his states of a fight are not through:
    /// they go on in the room, and what the fight read goes back behind them.
    leaves_fight: bool,
    /// The action the last pose found no animation for, so that the log names it once.
    unposed: Option<i16>,
    effects: Vec<u16>,
    /// What his states of a fight have asked of the sound and nobody has taken: the
    /// sound of an item and the row in front of it ([`Walker::take_sounds`]).
    sounds: Vec<Sign>,
    /// The start of the effects of his own command, which his machine gave in the last
    /// game frame and the fight's picture has not taken yet ([`Walker::take_started`]).
    started: Option<Sign>,
    /// The weapon and the armour his model was read for, and how the room read it.
    worn: Worn,
    read: Read,
}

impl Walker {
    /// The buttons of `--pad` for this game frame: the last set whose game frame has
    /// come, the first set before that.
    pub fn held(&self) -> Option<Held> {
        let later = (self.later.iter().rev().flatten()).find(|(from, _)| *from <= self.game_frame);
        later.map(|(_, held)| *held).or(self.pad)
    }

    /// Whether `--cross` puts the confirm button down in this game frame.
    pub fn cross(&self) -> bool {
        self.crosses.contains(&Some(self.game_frame))
    }

    /// What his model was read for.
    pub fn worn(&self) -> Worn {
        self.worn
    }

    pub fn wear(
        &mut self,
        disc: &mut DiscImage<File>,
        archive: &Archive,
        worn: Worn,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let exe = Executable::read(disc)?;
        let mut lines = Vec::new();
        let dressed = dress(disc, archive, &exe, worn, self.read, &mut lines)?;
        self.worn = worn;
        self.player.group = dressed.group;
        self.moves = dressed.moves;
        self.images = dressed.images;
        self.kept = dressed.own_meshes;
        self.rested = dressed.rested;
        let body = &mut self.body;
        body.block = dressed.block;
        body.skeleton = dressed.skeleton;
        body.animations = dressed.animations;
        body.actions = dressed.actions;
        body.points = dressed.points;
        (body.shelf, body.sets, body.brought) = (dressed.shelf, dressed.sets, dressed.brought);
        for read in [&mut self.read_for_fight, &mut self.read_for_task] {
            read.retain(|number| body.read_animation(*number));
        }
        for (action, played) in body.actions.iter().zip(&mut self.moves) {
            let animation = (action.animation).and_then(|number| body.animations.get(&number));
            *played = Move::of(action, animation);
        }
        self.stand();
        Ok(dressed.line)
    }

    /// The loader reads an animation set of the player's model (`0x800AC8D8`, which is
    /// `0x800257E8` for that model): the set it holds is dropped and the new one read, and
    /// the controller finds the actions of the animations that are in memory now. Answers
    /// the animations the new set has brought.
    pub fn read_set(&mut self, set: usize) -> Vec<usize> {
        let brought = self.body.read_set(set).to_vec();
        for (action, played) in self.body.actions.iter().zip(&mut self.moves) {
            let animation = (action.animation).and_then(|number| self.body.animations.get(&number));
            *played = Move::of(action, animation);
        }
        brought
    }

    fn fire_events(&mut self) {
        let row = arms::row(self.player.action, self.player.group);
        let animation = (row.and_then(|row| self.body.actions.get(row)))
            .and_then(|action| self.body.animations.get(&action.animation?));
        let Some(animation) = animation else {
            return;
        };
        let counter = i64::from(self.player.counter);
        let fired = || (animation.events.iter()).filter(|event| i64::from(event.frame) == counter);
        let swaps: Vec<(i16, i16)> = fired()
            .filter_map(|event| match event.action() {
                EventAction::Mesh { object, source } => Some((object, source)),
                _ => None,
            })
            .collect();
        let effects: Vec<u16> = fired()
            .filter_map(|event| match event.action() {
                EventAction::Effect(number) => Some(number),
                _ => None,
            })
            .collect();
        self.effects.extend(effects);
        for (object, source) in swaps {
            if self.body.block.swap(&self.kept, object, source) {
                bevy::log::info!(
                    "player after {} game frames: an event of frame {counter} of action {} \
                     gives mesh object {object} the mesh of object {source}",
                    self.game_frame,
                    self.player.action
                );
            }
        }
    }

    /// The effects the events of the player's frames have asked for since the last call.
    pub fn take_effects(&mut self) -> Vec<u16> {
        std::mem::take(&mut self.effects)
    }

    /// What his states of a fight have asked of the sound since the last call: the
    /// signs of the sound alone ([`Sign::Sound`], [`Sign::BeforeItem`]).
    pub fn take_sounds(&mut self) -> Vec<Sign> {
        std::mem::take(&mut self.sounds)
    }

    /// The start of the effects of his own command ([`Sign::Effects`]), when his machine
    /// gave it in the game frame that was stepped last.
    pub fn take_started(&mut self) -> Option<Sign> {
        self.started.take()
    }

    /// The game frames his controller has run, for the log.
    pub fn game_frames(&self) -> u32 {
        self.game_frame
    }

    pub fn fight(&mut self, camera_state: Option<i32>) {
        let fights = self.in_fight;
        match camera_state {
            Some(state) => {
                self.in_fight = true;
                if !fights {
                    // What the fight before read stays for this one.
                    self.leaves_fight = false;
                    // The place is kept by order state 7, in the pass that follows
                    // (`Orders::pass`), once: the way back ends the fight for him
                    // while it is still on screen.
                    bevy::log::info!(
                        "player after {} game frames: a fight starts, the place {:?} is kept",
                        self.game_frame,
                        self.player.position
                    );
                }
                self.player.fight.camera_state = state;
            }
            None if fights => {
                self.in_fight = false;
                let sent_back = self.player.flags & flag::ARMS == 0;
                let through = sent_back && self.player.through();
                if sent_back && !through && self.player.mode == 1 {
                    self.player.end_carrying();
                    self.leaves_fight = true;
                    bevy::log::info!(
                        "player after {} game frames: the fight is over, his states of a \
                         fight go on in the room from state {} of action {}",
                        self.game_frame,
                        self.player.state,
                        self.player.action
                    );
                    return;
                }
                self.player.back_to_place(false);
                self.player.end_carrying();
                self.player.flags &= !(flag::BUSY | flag::POSE);
                if !through {
                    self.rest_meshes();
                }
                self.give_back_fight();
                if self.player.mode == 1 {
                    self.player.stand_again(&self.moves);
                }
                self.stand();
                bevy::log::info!(
                    "player after {} game frames: the fight is over, he is free of it in \
                     action {}{}",
                    self.game_frame,
                    self.player.action,
                    match through {
                        true => "",
                        false => " (the weapon is not put away with its action: a stand-in)",
                    }
                );
            }
            None if self.leaves_fight && (self.player.through() || self.player.mode != 1) => {
                self.leaves_fight = false;
                self.give_back_fight();
                bevy::log::info!(
                    "player after {} game frames: his states of a fight are over in the \
                     room, in mode {} and action {}{}",
                    self.game_frame,
                    self.player.mode,
                    self.player.action,
                    match self.player.through() {
                        true => "",
                        false => " (taken out of them, not through)",
                    }
                );
            }
            None => {}
        }
    }

    /// The animations a fight has read for his moves go back.
    fn give_back_fight(&mut self) {
        for number in std::mem::take(&mut self.read_for_fight) {
            self.body.give_back(number);
            for (action, played) in self.body.actions.iter().zip(&mut self.moves) {
                if action.animation == Some(number) {
                    *played = Move::of(action, None);
                }
            }
        }
    }

    pub fn end_fight(&mut self, pose: bool) {
        for wanted in [arms::action::PUT_AWAY, arms::action::POSE] {
            self.read_for(wanted);
        }
        self.player.back_to_place(pose);
        let there = [arms::action::PUT_AWAY, arms::action::POSE].map(|action| {
            (arms::row(action, self.player.group).and_then(|row| self.moves.get(row)))
                .is_some_and(|played| !played.travel.is_empty())
        });
        bevy::log::info!(
            "player after {} game frames: the way back from the fight in state {} of action \
             {}, the animations of the actions 15 and 10 in memory: {there:?}, {} a pose",
            self.game_frame,
            self.player.state,
            self.player.action,
            match pose {
                true => "with",
                false => "without",
            }
        );
    }

    pub fn get_up(&mut self) {
        self.player.let_go();
    }

    /// A line for the log when an order state of a fight (1 to 8) begins or ends.
    pub fn note_order_state(&self, before: i16, after: i16) {
        let of_a_fight = [before, after].iter().any(|state| (1..=8).contains(state));
        if before != after && of_a_fight {
            bevy::log::info!(
                "player after {} game frames: the order state {before} becomes {after}; his \
                 angle is {}, the state of his machine {}",
                self.game_frame,
                self.player.angle,
                self.player.state
            );
        }
    }

    /// Whether the states of a fight are through for him (`0x800A8E8C`).
    pub fn through(&self) -> bool {
        self.player.through()
    }

    /// The loader reads the animation of an action for a fight (`0x800AC6E8`, state 11
    /// of the machine), and the controller finds the action in memory. The build's
    /// loader takes no time.
    fn read_for(&mut self, action: i16) {
        let Some(row) = arms::row(action, self.player.group) else {
            return;
        };
        let number = (self.body.actions.get(row)).and_then(|action| action.animation);
        if let Some(number) = number
            && self.body.read_animation(number)
        {
            self.read_for_fight.push(number);
            for (action, played) in self.body.actions.iter().zip(&mut self.moves) {
                if action.animation == Some(number) {
                    *played = Move::of(action, self.body.animations.get(&number));
                }
            }
        }
    }

    pub fn read_action(&mut self, action: i16) -> bool {
        let number = (arms::row(action, self.player.group))
            .and_then(|row| self.body.actions.get(row)?.animation);
        let Some(number) = number else {
            return false;
        };
        if self.body.read_animation(number) {
            self.read_for_task.push(number);
            for (action, played) in self.body.actions.iter().zip(&mut self.moves) {
                if action.animation == Some(number) {
                    *played = Move::of(action, self.body.animations.get(&number));
                }
            }
        }
        self.body.animations.contains_key(&number)
    }

    /// The animation of such an action goes back to the loader (`0x800AC878`), when it
    /// was read for the task and not in memory before.
    pub fn give_back_action(&mut self, action: i16) {
        let number = (arms::row(action, self.player.group))
            .and_then(|row| self.body.actions.get(row)?.animation);
        let read = number.and_then(|number| {
            let at = (self.read_for_task.iter()).position(|&read| read == number)?;
            Some(self.read_for_task.swap_remove(at))
        });
        let Some(number) = read else {
            return;
        };
        self.body.give_back(number);
        for (action, played) in self.body.actions.iter().zip(&mut self.moves) {
            if action.animation == Some(number) {
                *played = Move::of(action, None);
            }
        }
    }

    pub fn asked(&mut self, kind: u32, reaction: bool, setting: u32) -> (Option<i16>, [i16; 2]) {
        let Some(action) = arms::wanted(kind, setting) else {
            return (None, [0, 0]);
        };
        for wanted in [action, arms::action::PUT_AWAY, arms::action::POSE] {
            self.read_for(wanted);
        }
        // The loops of the capture before are not ended any more. Stand-in: who clears
        // that half word in the game is not read.
        self.player.fight.loops_end = false;
        self.player.set_guard(kind == 18);
        self.player.set_stance(kind == 16);
        self.player.set_pose_behind(kind == 26);
        let times = arms::times(&self.moves, self.player.group, action).unwrap_or([0, 0]);
        bevy::log::info!(
            "player after {} game frames: the fight asks for action {action} (kind {kind}, {}): \
             the key moment comes {} game frames after the go, the end {}",
            self.game_frame,
            match reaction {
                true => "a reaction",
                false => "his own command",
            },
            times[0],
            times[1]
        );
        (Some(action), times)
    }

    fn follow_signs(&mut self) {
        self.started = (self.player.fight.signs.iter())
            .find(|sign| matches!(sign, Sign::Effects { .. }))
            .copied();
        for sign in &self.player.fight.signs {
            if matches!(sign, Sign::Sound(_) | Sign::BeforeItem) {
                bevy::log::info!(
                    "player after {} game frames: frame {} of action {} asks the sound for \
                     {sign:?}",
                    self.game_frame,
                    self.player.counter,
                    self.player.action
                );
                self.sounds.push(*sign);
            }
        }
        let swaps: Vec<(i16, i16)> = (self.player.fight.signs.iter())
            .flat_map(|sign| match sign {
                Sign::WeaponInHand => vec![(2, 24), (7, 26)],
                Sign::WeaponAway => vec![(2, 23), (7, 25)],
                Sign::WeaponAwayWhole => vec![(2, 23), (7, 25), (12, 12)],
                _ => Vec::new(),
            })
            .collect();
        for (object, source) in swaps {
            let given = self.body.block.swap(&self.kept, object, source);
            bevy::log::info!(
                "player after {} game frames: the fight gives mesh object {object} the mesh \
                 of object {source} at frame {} of action {}: {}",
                self.game_frame,
                self.player.counter,
                self.player.action,
                match given {
                    true => "done",
                    false => "no such mesh",
                }
            );
        }
    }

    /// One game frame of the controller's update with these buttons held, up to the pose.
    pub fn step(&mut self, held: Held) {
        // The events of the frame before come behind its drawing and before this update.
        self.fire_events();
        let (place, angle) = (self.player.position, self.player.angle);
        self.way = InTheWay::of(&self.body.arc, place, angle, &self.things);
        let command = player::button_command(held);
        self.player.command = self.way.turned_away(command);
        self.turned_away += u32::from(self.player.command != command);
        // The command word of an order state goes to the controller as it is.
        if let Some(ordered) = self.ordered.take() {
            self.player.command = ordered;
        }
        (self.player).update(&self.moves, &self.ground, &self.body.arc, self.game_frame);
        self.follow_signs();
        self.game_frame = self.game_frame.wrapping_add(1);
        self.stands_in = self.view(self.stands_in).unwrap_or(self.stands_in);
        self.stand();
        let word = self.player.draw_word();
        if word.is_none() != self.body.runner.hidden {
            bevy::log::info!(
                "player after {} game frames: {}",
                self.game_frame,
                match word {
                    None => "he is in the black and no longer drawn",
                    Some(_) => "he comes out of the black and is drawn again",
                }
            );
        }
        let tinted = self.body.runner.with_tint(word.unwrap_or(0));
        (self.body.runner.word, self.body.runner.hidden) = (tinted, word.is_none());
        if self.game_frame.is_multiple_of(25) {
            bevy::log::debug!("{}", self.line());
        }
    }

    /// Up to `frames` game frames with the pad's buttons held, ended by the game frame in
    /// which the player stands in the trigger region of an exit: that exit, and the game
    /// frames that are left.
    pub fn run(&mut self, frames: u32) -> Option<(Exit, u32)> {
        let held = self.pad.unwrap_or_default();
        (1..=frames).find_map(|frame| {
            self.step(held);
            Some((self.leaves()?, frames - frame))
        })
    }

    pub fn leaves(&mut self) -> Option<Exit> {
        if let Some((exit, angle)) = self.warp.take() {
            self.warp_facing = Some(angle);
            return Some(exit);
        }
        self.warp_facing = None;
        let place = self.player.position;
        let view = self.stands_in;
        let (exit, _) = (self.exits.iter()).find(|(exit, region)| {
            let masked = view < 32 && exit.view_mask >> view & 1 == 0;
            let rests = self.resting.contains(&exit.record);
            !masked && !rests && region.triangle_at(place).is_some()
        })?;
        let exit = *exit;
        if exit.stands_again() {
            self.stand_again();
        }
        Some(exit)
    }

    pub fn set_barrier(&mut self, region: usize, byte: u8) -> bool {
        match self.ground.flags.get_mut(region) {
            Some(flag) if *flag != byte => {
                *flag = byte;
                true
            }
            _ => false,
        }
    }

    pub fn stand_again(&mut self) {
        self.rest_meshes();
        self.player.stand_again(&self.moves);
        self.stand();
    }

    /// The mesh objects an event or a fight can change show what they show at rest.
    fn rest_meshes(&mut self) {
        for (object, mesh) in &self.rested {
            if let Some(shown) = self.body.block.objects.get_mut(*object) {
                *shown = mesh.clone();
            }
        }
    }

    pub fn arrive(&mut self, mut from: Arrival) {
        let angle = (from.facing).unwrap_or_else(|| from.exit.facing(from.player.angle));
        from.player.arrive(from.exit.arrival, angle);
        self.player = from.player;
        self.game_frame = from.game_frame;
        self.stands_in = self.view(0).unwrap_or(0);
        self.stand();
    }

    pub fn come_out(&mut self, place: [i32; 3], angle: i16) {
        self.player.arrive(place, angle);
        self.player.set_shown(false);
        self.stands_in = self.view(0).unwrap_or(0);
        self.stand();
    }

    pub fn stand_at(&mut self, place: [i32; 3], angle: i16) {
        self.player.arrive(place, angle);
        self.stands_in = self.view(0).unwrap_or(0);
        self.stand();
    }

    /// What the player takes through this exit into the next room.
    pub fn leaving(&self, exit: Exit) -> Arrival {
        Arrival {
            exit,
            player: self.player.clone(),
            game_frame: self.game_frame,
            facing: self.warp_facing,
        }
    }

    pub fn warp_out(&mut self, exit: Exit, angle: i16) -> bool {
        let new = self.warp.is_none();
        self.warp = Some((exit, angle));
        new
    }

    /// The view of the region the player stands in, as the camera's lookup followed it.
    pub fn stands_in(&self) -> usize {
        self.stands_in
    }

    /// The exits of the room that nothing can be loaded behind are no longer asked.
    pub fn forget_exits(&mut self) {
        self.exits.clear();
    }

    fn stand(&mut self) {
        let player = &self.player;
        let body = &self.body;
        let animation = |action: i16| -> Option<&Animation> {
            let action = body.actions.get(arms::row(action, player.group)?)?;
            body.animations.get(&action.animation?)
        };
        let frame = |animation: &Animation, counter: u32| {
            (counter as usize).min(animation.frames.saturating_sub(1))
        };
        let Some(new) = animation(player.action) else {
            // The pose of the frame before stays: he stands frozen in it.
            let action = player.action;
            if self.unposed != Some(action) {
                bevy::log::warn!(
                    "player after {} game frames: action {action} has no animation in memory, \
                     the pose before stays",
                    self.game_frame
                );
                self.unposed = Some(action);
            }
            return;
        };
        self.unposed = None;
        let turn = rotation([0, player.angle, 0]);
        // Nothing read so far gives the player a scale of its own.
        let scales = Scales {
            model: None,
            bones: &[],
        };
        let old = (player.flags & flag::FADE != 0)
            .then(|| animation(player.action_before))
            .flatten();
        let now = (new, frame(new, player.counter));
        let pose = match old {
            Some(old) => body.skeleton.pose_faded_from(
                turn,
                now,
                (old, frame(old, player.counter_before)),
                WHOLE - i32::from(player.fade),
                &scales,
            ),
            None => body.skeleton.pose_from(turn, now.0, now.1, &scales),
        };
        (self.body.pose, self.body.place, self.body.angle) = (pose, player.position, player.angle);
    }

    pub fn view(&self, current: usize) -> Option<usize> {
        self.ground.view_from(current, self.player.position)
    }

    pub fn view_of(&self, current: usize, place: [i32; 3]) -> Option<usize> {
        self.ground.view_from(current, place)
    }

    /// The state of the controller in words, for the log.
    pub fn line(&self) -> String {
        let player = &self.player;
        format!(
            "player after {} game frames: at {:?} angle {} ({} of the turn), state {} action \
             {} frame {}, flags {:#x}, on triangle {} of region {}, {} things in the way list \
             ({}), turned away in {} game frames{}",
            self.game_frame,
            player.position,
            player.angle,
            // The half word counts on over whole turns, as the game's does.
            i32::from(player.angle) & 4095,
            player.state,
            player.action,
            player.counter,
            player.flags,
            player.foothold.triangle,
            player.foothold.region,
            self.things.len(),
            match self.way {
                InTheWay {
                    ahead: Some(off), ..
                } => format!("one ahead, {off} off"),
                InTheWay { behind: true, .. } => "one behind".to_string(),
                _ => "none near ahead or behind".to_string(),
            },
            self.turned_away,
            match player.fault {
                Some(fault) => format!(", the walk stopped: {fault}"),
                None => String::new(),
            }
        )
    }
}

struct Dressed {
    /// The group of the weapon's actions, by the look of the weapon.
    group: usize,
    block: MeshBlock,
    /// The copies of its own meshes that the model keeps, and what the same objects show
    /// once the part is put on or kept off.
    own_meshes: Kept,
    rested: Vec<(usize, MeshObject)>,
    images: Vec<Tim>,
    animations: BTreeMap<usize, Animation>,
    moves: Vec<Move>,
    actions: Vec<Action>,
    points: Vec<Point>,
    shelf: BTreeMap<usize, Animation>,
    sets: Vec<Vec<usize>>,
    brought: Vec<usize>,
    /// The animation and the span of the standing action.
    stand: (usize, Span),
    skeleton: Skeleton,
    /// The start of the line about him.
    line: String,
}

/// What a room keeps of how its player was read, for reading him again.
#[derive(Clone, Copy)]
struct Read {
    area: usize,
    scripted: Option<Option<usize>>,
    reads: bool,
}

/// Reads the player's model, its part, its images and its animations for what he has on,
/// in a room of `area`. `scripted` and `reads` are those of [`load`].
fn dress(
    disc: &mut DiscImage<File>,
    archive: &Archive,
    exe: &Executable,
    worn: Worn,
    Read {
        area,
        scripted,
        reads,
    }: Read,
    lines: &mut Vec<String>,
) -> Result<Dressed, Box<dyn std::error::Error>> {
    let models = ModelTable::from_executable(&exe.image, exe.base)?;
    // The model and the part of what the player has on; the part is put on (the weapon on
    // the back) outside the areas whose rooms set bit 24 of the engine's flag word.
    let field_engine = archive.read(disc, FIELD_ENGINE_FILE)?;
    let tables = PartTables::read(&field_engine, &exe.image, exe.base)?;
    let (model, number) =
        (tables.worn(worn)).ok_or("--wears names a weapon or an armour the game does not have")?;
    // The group of the weapon's actions, by the look of the weapon.
    let groups = arms::weapon_groups(&field_engine, FIELD_ENGINE_BASE, model::WEAPONS);
    let group = (tables.looks.get(usize::from(worn.weapon)))
        .and_then(|look| groups.as_ref()?.get(*look).copied())
        .ok_or("the field engine has no group for the weapon")?;
    let record = models.get(model).ok_or("the model table has no player")?;
    let player_file = record.file;
    let file = archive.read(disc, player_file as usize)?;
    let head = record.head(&file)?;
    let part = record.part(&file, number)?;
    let put_on = model::part_is_put_on(area);
    let own = record.mesh(&file)?;
    let block = match put_on {
        true => part.put_on(&own),
        false => part.kept_off(&own),
    };
    // The copies of its own meshes that the model keeps, and what the same objects show
    // once the part is put on or kept off.
    let own_meshes = Kept::of_player(&field_engine, &own)
        .ok_or("the field engine has no list of the player's kept mesh objects")?;
    let rested: Vec<(usize, MeshObject)> = (own_meshes.objects())
        .filter_map(|object| Some((object, block.objects.get(object)?.clone())))
        .collect();
    let mut images = record.textures(&file)?;
    images.extend(part.images);

    let mut animations = BTreeMap::new();
    let mut moves = Vec::new();
    let (whole, kept) = match scripted {
        Some(None) => (true, head.actions.len()),
        Some(Some(_)) => (false, head.actions.len()),
        None => (false, FREE_ACTIONS),
    };
    let set: Vec<usize> = match scripted.flatten() {
        Some(set) => (record.set(&file, set)?.iter())
            .map(|entry| entry.animation)
            .collect(),
        None => Vec::new(),
    };
    if let Some(Some(number)) = scripted {
        lines.push(format!(
            "the player has animation set {number} of its model: {} animations",
            set.len()
        ));
    }
    for action in head.actions.iter().take(kept) {
        let resident = (action.animation).filter(|number| {
            let resident = record.animations.get(*number).is_some_and(|a| a.resident);
            whole || resident || set.contains(number)
        });
        let animation = match resident {
            Some(number) if scripted.is_some() => record.animation(&file, number).ok(),
            Some(number) => Some(record.animation(&file, number)?),
            None => None,
        };
        let resident = resident.filter(|_| animation.is_some());
        moves.push(Move::of(action, animation.as_ref()));
        if let (Some(number), Some(animation)) = (resident, animation) {
            animations.insert(number, animation);
        }
    }
    for number in arms::read_with_weapon(&head.actions, group) {
        if animations.contains_key(&number) {
            continue;
        }
        let Ok(animation) = record.animation(&file, number) else {
            continue;
        };
        for (action, played) in head.actions.iter().zip(&mut moves) {
            if action.animation == Some(number) {
                *played = Move::of(action, Some(&animation));
            }
        }
        animations.insert(number, animation);
    }
    let (mut shelf, mut sets, mut brought) = (BTreeMap::new(), Vec::new(), Vec::new());
    if reads && scripted.is_some() {
        for number in 0..record.animations.len() {
            if !animations.contains_key(&number)
                && let Ok(animation) = record.animation(&file, number)
            {
                shelf.insert(number, animation);
            }
        }
        for number in 0..record.sets.len() {
            let entries = record.set(&file, number)?;
            sets.push(entries.iter().map(|entry| entry.animation).collect());
        }
        let stays = |number: &usize| record.animations[*number].resident;
        brought = (set.iter().copied())
            .filter(|number| animations.contains_key(number) && !stays(number))
            .collect();
    }
    let stand = (head.actions.get(player::act::STAND as usize))
        .and_then(|action| Some((action.animation?, action.span())))
        .ok_or("the player has no standing action")?;

    // The bone table the model list gives the model, else the people's table.
    let overlay = archive.read(disc, MODEL_LIST_FILE)?;
    let listed = ModelList::at(&overlay, MODEL_LIST_BASE, MODEL_LIST)?;
    let skeleton = match listed.get(model) {
        Some(entry) => entry.skeleton.clone(),
        None => Skeleton::at(&field_engine, FIELD_ENGINE_BASE, HUMAN_SKELETON)?,
    };
    let line = format!(
        "player: weapon {} (group {group}) and {}, which is model {model} (file {player_file}) with part \
         {number} of {} {}, {} mesh objects, {} images, {} bones, {} of {kept} actions \
         in memory",
        worn.weapon,
        match worn.armour {
            Worn::NOTHING => "no armour".to_string(),
            armour => format!("armour {armour}"),
        },
        record.parts.len(),
        match put_on {
            true => "put on",
            false => "kept off (area 1 or 2)",
        },
        block.objects.len(),
        images.len(),
        skeleton.bones.len(),
        animations.len()
    );
    Ok(Dressed {
        group,
        block,
        own_meshes,
        rested,
        images,
        animations,
        moves,
        actions: head.actions.clone(),
        points: head.points().unwrap_or_default(),
        shelf,
        sets,
        brought,
        stand,
        skeleton,
        line,
    })
}

/// Reads the player's model and makes the player on the default place of a room of `area`.
/// Adds a line about it to `lines`. `scripted` is for a room whose scripts run: the
/// animation set of the player that the room's package asks for, whose animations are in
/// memory beside the resident ones for the actions a script orders; without a number
/// (a package that reads back the set of the scene before) every animation of the file is
/// read, a stand-in until the scene before is known. `reads` is for a room whose scripts
/// name another set of the player on the way ([`Walker::read_set`]) or start a fight, whose
/// actions are read when the fight asks for them (`Walker::asked`).
pub fn load(
    disc: &mut DiscImage<File>,
    archive: &Archive,
    (data, area): (&RoomData, usize),
    exits: &[Exit],
    asked: Asked,
    (scripted, reads): (Option<Option<usize>>, bool),
    lines: &mut Vec<String>,
) -> Result<Walker, Box<dyn std::error::Error>> {
    let exe = Executable::read(disc)?;
    let read = Read {
        area,
        scripted,
        reads,
    };
    let Dressed {
        group,
        block,
        own_meshes,
        rested,
        images,
        animations,
        moves,
        actions,
        points,
        shelf,
        sets,
        brought,
        stand,
        skeleton,
        line,
    } = dress(disc, archive, &exe, asked.worn, read, lines)?;
    let arc = ArcTangent::from_image(&exe.image, exe.base)
        .ok_or("the executable has no arc tangent table")?;

    let place = data.default_position();
    let angle = data.default_angles()[1];
    let mut player = Player::new(place, angle, &moves);
    player.group = group;
    lines.push(format!("{line}, at {place:?} angle {angle}"));
    let body = Standing {
        block,
        // The player's images lie in every video memory of the room.
        memory: 0,
        skeleton,
        pose: Vec::new(),
        model_scale: None,
        bone_scales: Vec::new(),
        place,
        angle,
        animations,
        actions,
        points,
        shape: None,
        // The player's sets are read by the room's loader (`walker::load`), and again
        // behind a fight of a script ([`Walker::read_set`]).
        shelf,
        sets,
        brought,
        // The command list and the mover are an actor's: the player has the controller.
        runner: Runner::new(
            Playback::new(stand.0, stand.1, WHOLE as i16),
            player::act::STAND,
            0,
        ),
        mover: Mover::new(place, angle),
        arc: Arc::new(arc),
        ground: None,
        game_frame: 0,
        // The player is never the speaker of a script's message.
        name: Vec::new(),
        mirrored: false,
        slot: 0,
        moves: None,
        // The controller keeps the player's copies ([`Walker::fire_events`]).
        kept: Kept::default(),
        roams: None,
        mate: None,
        carried: None,
        table: 0,
    };
    let mut walker = Walker {
        body,
        player,
        moves,
        ground: data.ground(),
        images,
        pad: asked.pad,
        ordered: None,
        later: asked.later,
        crosses: asked.crosses,
        game_frame: 0,
        // An exit that names a trigger region the room does not have is never set off.
        exits: (exits.iter())
            .filter_map(|exit| {
                let region = data.triggers().get(usize::try_from(exit.region).ok()?)?;
                Some((*exit, region.clone()))
            })
            .collect(),
        resting: Vec::new(),
        warp: None,
        warp_facing: None,
        things: Vec::new(),
        way: InTheWay::default(),
        turned_away: 0,
        stands_in: data.view_at(place).unwrap_or(0),
        kept: own_meshes,
        rested,
        read_for_fight: Vec::new(),
        read_for_task: Vec::new(),
        in_fight: false,
        leaves_fight: false,
        unposed: None,
        effects: Vec::new(),
        sounds: Vec::new(),
        started: None,
        worn: asked.worn,
        read,
    };
    walker.stand();
    Ok(walker)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "needs the disc image named in jc.local.toml"]
    fn what_he_has_on_picks_the_part_the_group_and_the_model() {
        let config = jc_disc::LocalConfig::load().unwrap();
        let mut disc = DiscImage::open(&config.disc_image).unwrap();
        let archive = Archive::open(&mut disc).unwrap();
        let exe = Executable::read(&mut disc).unwrap();
        let read = Read {
            area: 0,
            scripted: None,
            reads: false,
        };
        let mut dressed = |weapon, armour| {
            let worn = Worn { weapon, armour };
            dress(&mut disc, &archive, &exe, worn, read, &mut Vec::new()).unwrap()
        };
        let knife = dressed(0, Worn::NOTHING);
        let sword = dressed(5, Worn::NOTHING);
        let armoured = dressed(0, 0);
        assert!(knife.line.contains("model 40") && knife.line.contains("part 5 of"));
        assert!(sword.line.contains("model 40") && sword.line.contains("part 12 of"));
        assert!(armoured.line.contains("model 39"));
        assert_ne!(knife.group, sword.group);
        assert_eq!(knife.block.objects.len(), sword.block.objects.len());
        // The part's pair list gives the back another mesh, and the rest stays.
        assert_ne!(knife.rested, sword.rested);
        assert_eq!(knife.moves.len(), FREE_ACTIONS);
        assert_eq!(knife.stand.0, sword.stand.0);
    }

    #[test]
    fn a_pad_is_named_by_what_its_buttons_do() {
        let held = Asked::pad("forward,fast,right").expect("three buttons");
        let expected = Held {
            forward: true,
            fast: true,
            rises: true,
            ..Held::default()
        };
        assert_eq!(held, expected);
        assert_eq!(
            player::button_command(held),
            player::command::RUN | player::command::RISES
        );
        assert_eq!(Asked::pad("forward,jump"), None);
        // Other buttons from a game frame on.
        let (first, later) = Asked::pads("left;300:forward;340:").expect("three sets");
        assert!(first.falls && !first.forward);
        assert_eq!(
            later[0],
            Some((300, Asked::pad("forward").expect("a button")))
        );
        assert_eq!(later[1], Some((340, Held::default())));
        assert_eq!(later[2], None);
        assert_eq!(Asked::pads("left;soon:forward"), None);
        assert!(Asked::pads(&format!("left{}", ";1:".repeat(256))).is_some());
        assert_eq!(Asked::pads(&format!("left{}", ";1:".repeat(257))), None);
        // The confirm button by game frames.
        let crosses = Asked::crosses("300,420").expect("two game frames");
        assert_eq!(crosses[..3], [Some(300), Some(420), None]);
        assert_eq!(Asked::crosses("300,soon"), None);
        assert!(Asked::crosses(&["1"; 64].join(",")).is_some());
        assert_eq!(Asked::crosses(&["1"; 65].join(",")), None);
    }

    #[test]
    fn what_the_player_wears_is_a_weapon_and_maybe_an_armour() {
        let worn = |weapon, armour| Some(Worn { weapon, armour });
        assert_eq!(Asked::wears("0"), worn(0, Worn::NOTHING));
        assert_eq!(Asked::wears("22:15"), worn(22, 15));
        assert_eq!(Asked::wears("sword"), None);
        assert_eq!(Asked::wears("1:"), None);
    }
}
