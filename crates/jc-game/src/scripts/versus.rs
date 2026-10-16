
use jc_formats::battle::duel::{self, Duel, MOST, Step as DuelStep};
use jc_formats::battle::fight::{Fight, reaction};
use jc_formats::battle::hud::{FIRST_SLOT, Hud, Marks, Seen};
use jc_formats::battle::runner::Showing;
use jc_formats::versus::Series;

use super::*;

/// The game frames of the dice of the head start (25 a second): a roll tumbles, lands and
/// stands; a draw says so and is rolled again; the last roll says who goes first.
pub mod dice {
    pub const TUMBLE: u32 = 28;
    pub const LANDED: u32 = 10;
    pub const DRAW: u32 = 20;
    pub const FIRST: u32 = 26;
    /// What is left of the dice behind a confirm.
    pub const QUICK: u32 = 8;
    /// A roll that is a draw, from its tumble to the next roll.
    pub const AGAIN: u32 = TUMBLE + LANDED + DRAW;

    /// The game frames of the dice with this many rolls.
    pub fn frames(rolls: usize) -> u32 {
        rolls.saturating_sub(1) as u32 * AGAIN + TUMBLE + LANDED + FIRST
    }

    /// The game frame the last roll lands in.
    pub fn landed(rolls: usize) -> u32 {
        rolls.saturating_sub(1) as u32 * AGAIN + TUMBLE
    }

    /// The roll on screen in game frame `at` of the dice, and the game frames into it.
    pub fn roll(rolls: usize, at: u32) -> (usize, u32) {
        let roll = ((at / AGAIN) as usize).min(rolls.saturating_sub(1));
        (roll, at - roll as u32 * AGAIN)
    }
}

/// The game frames the end of a fight shows "wins!" before its question takes a press.
const OVER_HOLDS: u32 = 20;
/// The game frames behind which a picture of a moment is noted for a check
/// ([`VersusMoment`]).
const MENU_SEEN: u32 = 12;
const CHOOSING_SEEN: u32 = 15;
const RESULT_SEEN: u32 = 30;

/// What the view hands a fight of two players at the start of each fight of a series.
pub struct VersusSetup {
    /// The local side of the duel.
    pub side: usize,
    /// The players' names, by side.
    pub names: [String; duel::SIDES],
    /// The fight's number in the series, from 1, and the tally in front of it.
    pub round: u32,
    pub series: Series,
    /// The duel as it starts (the dice rolled) and what it waits for.
    pub duel: Duel,
    pub step: Option<DuelStep>,
    pub place: FightPlace,
}

/// A moment of the fight a check takes a picture of (`--versus-shots=DIR`): the dice that
/// have landed, the local menu, the other side choosing, the key moment of an attack, a
/// fall, the entrance of the next minion behind a fall or a change, and the result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum VersusMoment {
    Dice,
    Menu,
    Choosing,
    Attack,
    Fall,
    Next,
    Result,
}

impl VersusMoment {
    pub fn name(self) -> &'static str {
        match self {
            VersusMoment::Dice => "dice",
            VersusMoment::Menu => "menu",
            VersusMoment::Choosing => "choosing",
            VersusMoment::Attack => "attack",
            VersusMoment::Fall => "fall",
            VersusMoment::Next => "next",
            VersusMoment::Result => "result",
        }
    }
}

/// A place of the build's own boxes over the fight that the mouse points at: the
/// answers of a question, and the sign of "Give up" beside the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersusSpot {
    Yes,
    No,
    GiveUp,
}

/// The dice as a game frame shows them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiceShown {
    /// Every roll, the local side's die first.
    pub rolls: Vec<[u8; 2]>,
    /// The roll on screen and the game frames into it ([`dice::roll`]).
    pub roll: usize,
    pub into: u32,
    /// Whether the local side won the head start, and "Equally fast!".
    pub won: bool,
    pub equally_fast: bool,
}

/// The end of a fight as a game frame shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverShown {
    /// Whether the local side won, and whether the loser gave up.
    pub won: bool,
    pub gave_up: bool,
    /// The tally after the fight: the local side's wins and losses.
    pub wins: u8,
    pub losses: u8,
    /// The game frames since it came, the cursor of "Rematch?" (on yes) and the answer.
    pub frames: u32,
    pub yes: bool,
    pub answered: Option<bool>,
}

/// What the build draws over a fight of two players in a game frame (`versus_view.rs`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersusShown {
    /// The local player's name and the other's.
    pub names: [String; 2],
    pub round: u32,
    pub game_frame: u32,
    pub dice: Option<DiceShown>,
    /// The game frames the other side has been choosing for.
    pub choosing: Option<u32>,
    /// The local menu is up, with the sign of "Give up"; the question of "Give up?"
    /// with its cursor on yes; a command went out and its turn is not back yet.
    pub menu: bool,
    pub give_up: Option<bool>,
    pub sent: bool,
    pub over: Option<OverShown>,
}

/// The duel as the session left it, kept for a fight still to come.
pub(super) struct Later(pub Duel, pub Option<DuelStep>, pub Series);

/// The end of a fight while it is on screen.
#[derive(Debug, Clone, Copy)]
struct Ending {
    winner: usize,
    gave_up: bool,
    frames: u32,
    yes: bool,
    answered: Option<bool>,
    /// The answer went to the view.
    handed: bool,
}

/// A fight of two players while it is on screen: the duel as the session last handed it
/// over and what the picture has of it.
pub(super) struct Match {
    side: usize,
    names: [String; duel::SIDES],
    round: u32,
    series: Series,
    duel: Duel,
    /// The duel's runs the picture has been given, and what the duel waits for.
    taken: usize,
    step: Option<DuelStep>,
    /// Game frames of the dice so far, and whether they are through.
    dice: u32,
    rolled: bool,
    /// The story's fight the menu reads its lists of, the side's items by place, and the
    /// menu's own generator for its preview.
    shadow: Option<Fight>,
    places: Vec<Option<usize>>,
    preview: Random,
    /// Game frames the menu has been up for; the command it handed over, until the view
    /// takes it, and whether one is out and its turn not back yet.
    menu_frames: u32,
    chosen: Option<duel::Choice>,
    sent: bool,
    /// The question of "Give up?", with the cursor on yes.
    give_up: Option<bool>,
    /// Game frames the other side has been choosing for.
    choosing: u32,
    over: Option<Ending>,
    /// The records of the HUD were set from the duel since the last run.
    synced: bool,
    moments: Vec<VersusMoment>,
    /// Moments noted that are pictured a few game frames later, when the HUD and the
    /// fighter show them.
    coming: Vec<(u32, VersusMoment)>,
    aim: Aim<VersusSpot>,
}

impl Match {
    fn new(setup: VersusSetup) -> (Self, FightPlace) {
        let VersusSetup {
            side,
            names,
            round,
            series,
            duel,
            step,
            place,
        } = setup;
        let preview = Random {
            seed: duel.seed() ^ 0x5EED,
        };
        let made = Self {
            side,
            names,
            round,
            series,
            taken: 0,
            step,
            dice: 0,
            rolled: false,
            shadow: None,
            places: Vec::new(),
            preview,
            menu_frames: 0,
            chosen: None,
            sent: false,
            give_up: None,
            choosing: 0,
            over: None,
            synced: false,
            moments: Vec::new(),
            coming: Vec::new(),
            aim: Aim::default(),
            duel,
        };
        (made, place)
    }

    /// The dice are still rolling: nothing else of the fight is shown meanwhile.
    pub(super) fn rolls(&self) -> bool {
        !self.rolled
    }

    /// The story's fight the menu reads, while it is up.
    pub(super) fn shadow(&self) -> Option<&Fight> {
        self.shadow.as_ref()
    }

    /// The items of the local side by place, for the menu's list.
    pub(super) fn places(&self) -> Vec<Option<usize>> {
        self.places.clone()
    }

    /// The local player's name and the other's.
    fn names(&self) -> [String; 2] {
        [
            self.names[self.side].clone(),
            self.names[1 - self.side].clone(),
        ]
    }

    /// A game frame of a run that is shown: the key moment of an attack, a fall and an
    /// entrance are moments a check takes pictures of.
    pub(super) fn note_run(&mut self, showing: &Showing) {
        if showing.clock() != showing.longest() || showing.longest() <= 0 {
            return;
        }
        let run = showing.run();
        let falls = (run.reactions.iter()).any(|shown| {
            shown.kind == u32::from(reaction::FALLS)
                || shown.kind == u32::from(reaction::STONE_DEATH)
        });
        let moment = match run.kind {
            _ if falls => VersusMoment::Fall,
            0 | 1 | 4 | 6 => VersusMoment::Attack,
            2 => VersusMoment::Next,
            _ => return,
        };
        // A fall is seen once the fighter lies and its HP have gone down; the next
        // minion once it stands.
        let later = match moment {
            VersusMoment::Fall => 24,
            VersusMoment::Next => 12,
            _ => 0,
        };
        self.coming.push((later, moment));
        self.synced = false;
    }
}

/// The opponents' mask the HUD of a fight of two players goes by: one opponent stands at
/// most, and its box is the first one's, whichever minion of the other side it is.
pub(super) fn hud_standing(standing: u8) -> u8 {
    match standing & 0b0111_0000 {
        0 => 0,
        _ => 1 << FIRST_SLOT,
    }
}

/// The records and the marks the HUD of a fight of two players is drawn with: the
/// opponent on the field in the place of the first, and the marks of whichever opponent
/// the menu marked on the first box.
pub(super) fn hud_view(
    records: &[Option<Record>; SLOTS],
    standing: u8,
    mut marks: Marks,
) -> ([Option<Record>; SLOTS], Marks) {
    let mut shown = records.clone();
    let on_field = (OPPONENTS..SLOTS).find(|slot| standing >> slot & 1 != 0);
    if let Some(slot) = on_field {
        shown[FIRST_SLOT] = records[slot].clone();
        for record in shown.iter_mut().skip(FIRST_SLOT + 1) {
            *record = None;
        }
    }
    let time = marks.times.iter().copied().max().unwrap_or(0);
    marks.times = [time, 0, 0];
    (shown, marks)
}

/// The fighters of the duel as the local picture has them: the side's own minions in
/// the slots 1 to 3, the other side's in 4 to 6, and the other side's fighter on the
/// field.
fn view_records(duel: &Duel, side: usize) -> ([Option<Record>; SLOTS], u8) {
    let of = |slot: usize| match side {
        0 => slot,
        _ => duel::mirrored(slot),
    };
    let records = std::array::from_fn(|slot| match slot {
        0 => None,
        slot => duel.record(of(slot)).cloned(),
    });
    let standing = (duel.fighter_slot(1 - side)).map_or(0, |slot| 1 << of(slot));
    (records, standing)
}

impl Running<'_> {
    /// Where the other side's minion stands: [`Lasting::TO_THE_OPPONENTS`] in front of
    /// the player's place, turned to him, as the spawner puts a single opponent.
    fn opponent_place(&self) -> ([i32; 3], i16) {
        let (place, angle) = (self.player.as_ref()).map_or(([0; 3], 0), |he| {
            (he.player.position, i32::from(he.player.angle))
        });
        let at = [
            place[0] + sine(angle) * Lasting::TO_THE_OPPONENTS / 4096,
            place[1],
            place[2] + cosine(angle) * Lasting::TO_THE_OPPONENTS / 4096,
        ];
        (at, (angle + 2048) as i16)
    }

    pub(super) fn enter_versus(&mut self, setup: VersusSetup) {
        let game_frame = self.room.game_frame;
        if let Some(before) = self.room.fight.take() {
            self.give_back_the_room(before.noted, before.stood);
        }
        let (versus, place) = Match::new(setup);
        let side = versus.side;
        let names = versus.names();
        info!(
            "game frame {game_frame}: fight {} of two players comes on screen in the place of \
             file {}: {} on this side, {} on the other",
            versus.round, place.place.file, names[0], names[1]
        );
        self.room.fight_place = Some(place);
        self.room.menu_kept = 0;
        let (here, angle) =
            (self.player.as_ref()).map_or(([0; 3], 0), |he| (he.player.position, he.player.angle));
        if let Some(he) = &mut self.player {
            he.player.set_shown(false);
        }
        let (there, turned) = self.opponent_place();
        self.room.orders.start_fight(there);
        let fighters = [versus.duel.fighter[side], versus.duel.fighter[1 - side]];
        let mut stood = Vec::new();
        for (index, actor) in self.actors.iter_mut().enumerate() {
            let (number, own) = match (actor.carried, actor.opponent) {
                (Some(number), _) => (number, true),
                (None, Some(number)) => (number, false),
                (None, None) => continue,
            };
            let fights = fighters[usize::from(!own)] == Some(number);
            stood.push(Stood {
                index,
                opponent: if own { usize::MAX } else { number },
                gone: own && !fights,
                waits: !own && !fights,
                mover: actor.mover.clone(),
                hidden: actor.runner.hidden,
                runner: Some(actor.runner.clone()),
                read: Vec::new(),
                carried: own.then_some(number),
                effect: None,
            });
            let (at, turn) = if own { (here, angle) } else { (there, turned) };
            actor.mover = mover::Mover::new(at, turn);
            (actor.place, actor.angle) = (at, turn);
            actor.runner.hidden = !fights;
        }
        let mut noted = Vec::new();
        for (index, actor) in self.actors.iter_mut().enumerate() {
            if actor.runner.hidden || stood.iter().any(|stands| stands.index == index) {
                continue;
            }
            actor.runner.hidden = true;
            noted.push(index);
        }
        let (records, standing) = view_records(&versus.duel, side);
        let fighter = fighters[0].map_or(PLAYER, |index| 1 + index);
        self.room.fight = Some(Lasting {
            script: Lasting::NO_SCRIPT,
            centre: Centre::of(here, angle.into()),
            frame: 0,
            ends: Lasting::CAMERA_STANDS,
            pause: 0,
            back: false,
            lead: 0,
            paused: 0,
            trail: None,
            played: None,
            runs: Vec::new(),
            at: 0,
            showing: None,
            between: 0,
            done: 0,
            told: Vec::new(),
            told_at: 0,
            saying: None,
            waited: 0,
            noted,
            stood,
            eye: arena::Eye::default(),
            species: None,
            roamer: None,
            results: None,
            bout: None,
            asks: None,
            fighter,
            menu: None,
            command: None,
            preview: (0, 0),
            at_once: 1,
            hud: None,
            seen: Some(Seen::new(records, standing)),
            master: 0,
            versus: Some(Box::new(versus)),
        });
    }

    /// One game frame of a fight of two players: its picture as the story's
    /// (`Running::show_fight`), which never ends by itself; the view ends it.
    pub(super) fn pass_versus(&mut self) {
        self.show_fight();
        let versus = (self.room.fight.as_mut()).and_then(|fight| fight.versus.as_deref_mut());
        if let Some(versus) = versus {
            for (later, moment) in std::mem::take(&mut versus.coming) {
                match later {
                    0 => versus.moments.push(moment),
                    later => versus.coming.push((later - 1, moment)),
                }
            }
        }
    }

    /// The other side's minion comes in (a run of kind 2 of an opponent's slot): it
    /// stands where the opponent stands, still hidden (its entrance shows it), the one
    /// that stood there leaves the field, and the HUD makes its box anew.
    pub(super) fn put_opponent(
        &mut self,
        (stood, seen, hud): (&mut Vec<Stood>, &mut Option<Seen>, &mut Option<Hud>),
        actors: u8,
    ) {
        let Some(slot) = (OPPONENTS..SLOTS).find(|slot| actors >> slot & 1 != 0) else {
            return;
        };
        let number = slot - OPPONENTS;
        let (there, turned) = self.opponent_place();
        for stands in stood.iter_mut().filter(|stands| stands.carried.is_none()) {
            let Some(actor) = self.actors.get_mut(stands.index) else {
                continue;
            };
            if stands.opponent == number {
                (stands.waits, stands.gone) = (false, false);
                actor.mover = mover::Mover::new(there, turned);
                (actor.place, actor.angle) = (there, turned);
                actor.runner.hidden = true;
            } else if stands.on_field() {
                stands.gone = true;
                actor.runner.hidden = true;
            }
        }
        if let Some(seen) = seen {
            seen.standing = 1 << slot;
        }
        if hud.is_some() {
            *hud = Some(Hud::new(1 << FIRST_SLOT, 0));
        }
        info!(
            "game frame {}: the other side's minion {number} comes onto the field in the \
             place of the opponent",
            self.room.game_frame
        );
    }

    /// The director of a fight of two players while every run it has is shown: the dice
    /// first, then the menu of the local side's turn, the sign of the other side's, or
    /// the end with its question.
    pub(super) fn versus_waits(&mut self, fight: &mut Lasting) {
        let game_frame = self.room.game_frame;
        fight.between = fight.between.saturating_add(1);
        let pad = self.room.pad;
        let Some(versus) = fight.versus.as_deref_mut() else {
            return;
        };
        if !versus.rolled {
            let rolls = versus.duel.start.rolls.len();
            let total = dice::frames(rolls);
            if versus.dice == 0 {
                let start = &versus.duel.start;
                info!(
                    "game frame {game_frame}: the dice of the head start roll: {:?}, side {} \
                     goes first{}",
                    start.rolls,
                    start.winner,
                    match start.equally_fast {
                        true => ", equally fast",
                        false => "",
                    }
                );
            }
            // A confirm takes the dice to their last line.
            if pad.pressed & pad::CROSS != 0 {
                versus.dice = versus.dice.max(total.saturating_sub(dice::QUICK));
            }
            versus.dice += 1;
            if versus.dice == dice::landed(rolls) + dice::LANDED + 6 {
                versus.moments.push(VersusMoment::Dice);
            }
            if versus.dice >= total {
                versus.rolled = true;
            }
            return;
        }
        // The picture has every run: its records are the duel's.
        if !versus.synced {
            versus.synced = true;
            if let Some(seen) = &mut fight.seen {
                let (records, _) = view_records(&versus.duel, versus.side);
                seen.records = records;
            }
        }
        match (versus.step, versus.side) {
            (Some(DuelStep::Over { winner, gave_up }), _) => {
                self.versus_over(fight, winner, gave_up);
            }
            (Some(DuelStep::Choose { side, .. }), local) if side == local => {
                self.versus_menu(fight);
            }
            (Some(DuelStep::Choose { .. }), _) => {
                versus.choosing += 1;
                if versus.choosing == CHOOSING_SEEN {
                    versus.moments.push(VersusMoment::Choosing);
                }
            }
            (None, _) => {}
        }
    }

    /// The end of a fight of two players: the victory piece, "wins!" and the question of
    /// a rematch, which the view takes the answer of.
    fn versus_over(&mut self, fight: &mut Lasting, winner: usize, gave_up: bool) {
        let game_frame = self.room.game_frame;
        let mut pad = self.room.pad;
        let Some(versus) = fight.versus.as_deref_mut() else {
            return;
        };
        let ending = versus.over.get_or_insert(Ending {
            winner,
            gave_up,
            frames: 0,
            yes: true,
            answered: None,
            handed: false,
        });
        if ending.frames == 0 {
            if let Some(place) = &self.room.fight_place {
                let (cameras, arc) = (&place.cameras, &place.arc);
                (fight.eye).pick(cameras, &fight.centre, arc, arena::Pick::Plain);
            }
            info!(
                "game frame {game_frame}: fight {} of two players is over: {} wins{}",
                versus.round,
                versus.names[winner],
                match gave_up {
                    true => ", the other gave up",
                    false => "",
                }
            );
            let asked = sound::versus_over().into_iter().map(Heard::Fight);
            (self.room.music).extend(asked.map(|asked| (game_frame, asked)));
        }
        ending.frames += 1;
        if ending.frames == RESULT_SEEN {
            versus.moments.push(VersusMoment::Result);
        }
        if ending.answered.is_some() || ending.frames < OVER_HOLDS {
            return;
        }
        let spot = if ending.yes {
            VersusSpot::Yes
        } else {
            VersusSpot::No
        };
        versus.aim.steer(&mut pad, spot, |spot| match spot {
            VersusSpot::Yes => pad::LEFT,
            VersusSpot::No => pad::RIGHT,
            VersusSpot::GiveUp => 0,
        });
        if pad.pressed & (pad::LEFT | pad::RIGHT) != 0 {
            ending.yes = pad.pressed & pad::LEFT != 0;
            self.room
                .music
                .push((game_frame, Heard::Sound(fight_menu::sound::MOVED)));
        }
        if pad.pressed & pad::CROSS != 0 {
            ending.answered = Some(ending.yes);
            self.room
                .music
                .push((game_frame, Heard::Sound(fight_menu::sound::CONFIRMED)));
            info!(
                "game frame {game_frame}: rematch? {}",
                if ending.yes { "yes" } else { "no" }
            );
        }
    }

    /// The local side's turn: the story's command menu of a minion over the duel's
    /// shadow, and the question of "Give up?" that a cancel on the root opens. The
    /// command a finished menu hands over is the view's to take, once the duel takes it.
    fn versus_menu(&mut self, fight: &mut Lasting) {
        let game_frame = self.room.game_frame;
        let Some(rules) = self.room.fights.as_ref() else {
            return;
        };
        let tables = &rules.tables;
        let Some(versus) = fight.versus.as_deref_mut() else {
            return;
        };
        if versus.sent {
            return;
        }
        let mut pad = self.room.pad;
        // The question of "Give up?".
        if let Some(yes) = versus.give_up {
            let spot = if yes { VersusSpot::Yes } else { VersusSpot::No };
            versus.aim.steer(&mut pad, spot, |spot| match spot {
                VersusSpot::Yes => pad::LEFT,
                VersusSpot::No => pad::RIGHT,
                VersusSpot::GiveUp => 0,
            });
            let sound = if pad.pressed & (pad::LEFT | pad::RIGHT) != 0 {
                versus.give_up = Some(pad.pressed & pad::LEFT != 0);
                Some(fight_menu::sound::MOVED)
            } else if jc_formats::menu::cancelled(&pad) {
                versus.give_up = None;
                Some(fight_menu::sound::CANCELLED)
            } else if pad.pressed & pad::CROSS != 0 {
                versus.give_up = None;
                if yes {
                    info!("game frame {game_frame}: the local side gives up");
                    versus.chosen = Some(duel::Choice::GiveUp);
                    versus.sent = true;
                    fight.menu = None;
                    fight.asks = None;
                    fight.preview = (0, 0);
                    // No run follows a giving up: the camera leaves the close-up of the
                    // menu's mark for the field's first camera, as at the end of a fight.
                    if let Some(place) = &self.room.fight_place {
                        let (cameras, arc) = (&place.cameras, &place.arc);
                        (fight.eye).pick(cameras, &fight.centre, arc, arena::Pick::Plain);
                    }
                }
                Some(fight_menu::sound::CONFIRMED)
            } else {
                None
            };
            self.room
                .music
                .extend(sound.map(|sound| (game_frame, Heard::Sound(sound))));
            return;
        }
        // The menu is made anew for each turn, as the story's is.
        if fight.menu.is_none() {
            let side = versus.side;
            let Some(shadow) = versus.duel.menu_fight(tables, side) else {
                return;
            };
            versus.places = (versus.duel.items[side].iter())
                .map(|&(item, _)| Some(item))
                .collect();
            let lists = fight_menu::Lists {
                fight: &shadow,
                tables,
                places: &versus.places,
            };
            let duel = &versus.duel;
            let index = duel.fighter[side];
            let others = (0..MOST).any(|place| {
                Some(place) != index
                    && (duel.teams[side][place].as_ref()).is_some_and(|record| !record.is_dead())
            });
            let mut offer = lists.offer(self.room.menu_kept);
            offer.duel = Some(fight_menu::DuelOffer {
                items: duel.options.items,
                no_change: match (duel.options.changing, others) {
                    (false, _) => Some(fight_menu::text::NOT_HERE),
                    (true, false) => Some(fight_menu::text::NO_CREATURE),
                    (true, true) => None,
                },
                no_item: (lists.last_place() < 0).then_some(fight_menu::text::NOT_HERE),
            });
            info!(
                "game frame {game_frame}: turn {} of fight {} of two players is the local \
                 side's: its menu is made",
                duel.turn(),
                versus.round
            );
            fight.menu = Some(fight_menu::Menu::new(offer));
            fight.asks = Some(fight.fighter);
            versus.shadow = Some(shadow);
            versus.menu_frames = 0;
            self.room.fight_aim.forget();
        }
        let (Some(menu), Some(shadow)) = (fight.menu.as_mut(), versus.shadow.as_ref()) else {
            return;
        };
        versus.menu_frames += 1;
        if versus.menu_frames == MENU_SEEN {
            versus.moments.push(VersusMoment::Menu);
        }
        // A cancel on the root, or a click on the sign beside it, asks "Give up?". The
        // click is the left button, which is the cross as well: the aim gives it back
        // only for the sign, and then the menu does not get it.
        let mut beside = pad;
        beside.pressed &= !pad::CROSS;
        beside.repeated &= !pad::CROSS;
        versus.aim.steer(&mut beside, VersusSpot::GiveUp, |_| 0);
        let on_root = menu.level() == fight_menu::level::ROOT;
        let clicked = beside.pressed & pad::CROSS != 0;
        if clicked {
            pad.pressed &= !pad::CROSS;
            pad.repeated &= !pad::CROSS;
        }
        if on_root && (jc_formats::menu::cancelled(&pad) || clicked) {
            info!(
                "game frame {game_frame}: \"Give up this fight?\" is asked ({})",
                if clicked {
                    "a click on the sign"
                } else {
                    "a cancel"
                }
            );
            versus.give_up = Some(false);
            self.room
                .music
                .push((game_frame, Heard::Sound(fight_menu::sound::CONFIRMED)));
            return;
        }
        let lists = fight_menu::Lists {
            fight: shadow,
            tables,
            places: &versus.places,
        };
        let here = fight_menu_panel::here(menu);
        (self.room.fight_aim).steer(&mut pad, here, |spot| fight_menu_panel::way(menu, spot));
        let pass = menu.pass(&pad, &lists);
        // The preview draws from a copy of the menu's own generator: the duel's draws
        // are the same on both machines and nothing here may take one.
        let mut draws = versus.preview;
        fight.preview = lists.preview(menu, game_frame % 2 == 1, &mut draws);
        fight.preview.1 -= lists.row_costs(menu);
        // The marked: the fighter turns to them and the camera is asked with them.
        if let Some(marked) = pass.marked {
            let named = |stands: &&Stood| marked >> (stands.opponent + OPPONENTS) & 1 != 0;
            let places: Vec<[i32; 3]> = (fight.stood.iter())
                .filter(|stands| stands.on_field())
                .filter(named)
                .filter_map(|stands| Some(self.actors.get(stands.index)?.mover.position))
                .collect();
            let figure = (fight.stood.iter())
                .find(|stands| stands.stands_for(fight.fighter))
                .map(|stands| stands.index);
            if let (Some(mean), Some(index)) = (runner::mean(&places), figure) {
                self.turn_figure(index, mean);
            }
            let lift = figure
                .and_then(|index| self.actors.get(index))
                .and_then(|actor| actor.pose.get(arena::LIFT_BONE)?.as_ref())
                .map_or(0, |bone| arena::CREATURE_LIFT + bone.position[1]);
            if let Some(place) = &self.room.fight_place {
                let asked = (fight.at_once, marked);
                (fight.eye).mark(&place.cameras, &fight.centre, &place.arc, asked, lift);
            }
        }
        let sounds = pass.sounds.iter();
        (self.room.music).extend(sounds.map(|&sound| (game_frame, Heard::Sound(sound))));
        let Some(chosen) = pass.chosen else {
            return;
        };
        self.room.menu_kept = menu.kept();
        fight.menu = None;
        fight.asks = None;
        fight.preview = (0, 0);
        let choice = duel::Choice::of_menu(&chosen);
        let Some(rules) = self.room.fights.as_ref() else {
            return;
        };
        let checked = |choice| versus.duel.check(&rules.tables, versus.side, choice);
        match choice.map(|choice| (choice, checked(choice))) {
            Some((choice, Ok(()))) => {
                info!("game frame {game_frame}: the menu hands over {choice:?}");
                versus.chosen = Some(choice);
                versus.sent = true;
            }
            refused => warn!(
                "game frame {game_frame}: the duel refuses {refused:?} of the menu (kind {} \
                 with parameter {} at the slots {:#04x}); the menu is made again",
                chosen.kind, chosen.parameter, chosen.targets
            ),
        }
    }
}

impl Scripts {
    /// The room is the arena of a fight of two players: none of its scripts runs, and
    /// its fights are the view's (`versus_view.rs`).
    pub fn for_versus(&mut self) {
        self.tasks.clear();
        self.room.by_view.clear();
        self.room.fight_menu = true;
        // The script's first game frame has run before it stopped: the greeting of room
        // 345 shuts the picture between two bars (`0x5E`), which no fight of the arena
        // wants. The whole screen is drawn, as in the story's fights.
        if self.room.area != DrawArea::WHOLE {
            info!(
                "game frame {}: the arena draws the whole screen again in place of {:?}",
                self.room.game_frame, self.room.area
            );
            self.room.area = DrawArea::WHOLE;
        }
    }

    /// A fight of two players comes on screen in the next game frame, in place of the
    /// one that is there.
    pub fn start_versus(&mut self, setup: VersusSetup) {
        self.room.versus_start = Some(Box::new(setup));
    }

    /// Whether a fight of two players is on screen or about to come.
    pub fn versus_on(&self) -> bool {
        self.room.versus_start.is_some() || self.versus().is_some()
    }

    fn versus(&self) -> Option<&Match> {
        self.room.fight.as_ref()?.versus.as_deref()
    }

    fn versus_mut(&mut self) -> Option<&mut Match> {
        self.room.fight.as_mut()?.versus.as_deref_mut()
    }

    /// The duel as the session left it behind a command, what it waits for and the
    /// tally: the runs the picture does not have yet are added, turned round for the
    /// player of side 1. A menu or a sign of a turn that is over goes.
    pub fn versus_update(&mut self, duel: &Duel, step: Option<DuelStep>, series: Series) {
        update(&mut self.room, duel, step, series);
    }

    /// The command the local menu handed over, for the session.
    pub fn take_versus_choice(&mut self) -> Option<duel::Choice> {
        self.versus_mut()?.chosen.take()
    }

    /// The session did not take the command: the menu is made again.
    pub fn versus_refused(&mut self) {
        if let Some(versus) = self.versus_mut() {
            versus.sent = false;
        }
    }

    /// The answer to "Rematch?", once it is given (taken once).
    pub fn take_versus_rematch(&mut self) -> Option<bool> {
        let ending = self.versus_mut()?.over.as_mut()?;
        let answer = ending.answered.filter(|_| !ending.handed)?;
        ending.handed = true;
        Some(answer)
    }

    /// The game frames the local menu has been up for, while it is.
    pub fn versus_menu_frames(&self) -> Option<u32> {
        let versus = self.versus()?;
        let up = self.room.fight.as_ref()?.menu.is_some() && !versus.sent;
        (up && versus.give_up.is_none()).then_some(versus.menu_frames)
    }

    /// The game frames the end of the fight has been on screen for, until "Rematch?" is
    /// answered.
    pub fn versus_over_frames(&self) -> Option<u32> {
        let ending = self.versus()?.over?;
        ending.answered.is_none().then_some(ending.frames)
    }

    /// The moments since the last call, for a check's pictures.
    pub fn take_versus_moments(&mut self) -> Vec<VersusMoment> {
        (self.versus_mut()).map_or_else(Vec::new, |versus| std::mem::take(&mut versus.moments))
    }

    /// Once a frame: the mouse over a spot of the build's boxes over the fight, while
    /// the boxes have spots (`aimed`); without any the aim is forgotten.
    pub fn versus_point(
        &mut self,
        mouse: &crate::pointer::Mouse,
        under: Option<VersusSpot>,
        aimed: bool,
    ) {
        if let Some(versus) = self.versus_mut() {
            match aimed {
                true => versus.aim.look(mouse, under),
                false => versus.aim.forget(),
            }
        }
    }

    /// What the build draws over the fight of two players in this game frame.
    pub fn versus_shown(&self) -> Option<VersusShown> {
        let fight = self.room.fight.as_ref()?;
        let versus = fight.versus.as_deref()?;
        let start = &versus.duel.start;
        // The dice once the opening has put the camera in place, until they are through.
        let rolling = !versus.rolled && versus.dice > 0;
        let dice = rolling.then(|| {
            let rolls: Vec<[u8; 2]> = (start.rolls.iter())
                .map(|&[first, second]| match versus.side {
                    0 => [first, second],
                    _ => [second, first],
                })
                .collect();
            let (roll, into) = dice::roll(rolls.len(), versus.dice.saturating_sub(1));
            DiceShown {
                roll,
                into,
                won: start.winner == versus.side,
                equally_fast: start.equally_fast,
                rolls,
            }
        });
        let all_shown = fight.showing.is_none() && fight.at == fight.runs.len();
        let waits = versus.rolled && all_shown;
        let choosing = match versus.step {
            Some(DuelStep::Choose { side, .. }) if waits && side != versus.side => {
                Some(versus.choosing)
            }
            _ => None,
        };
        let over = (versus.over.filter(|_| waits)).map(|ending| {
            let won = versus.series.won();
            OverShown {
                won: ending.winner == versus.side,
                gave_up: ending.gave_up,
                wins: won[versus.side],
                losses: won[1 - versus.side],
                frames: ending.frames,
                yes: ending.yes,
                answered: ending.answered,
            }
        });
        Some(VersusShown {
            names: versus.names(),
            round: versus.round,
            game_frame: self.room.game_frame,
            dice,
            choosing,
            menu: fight.menu.is_some() && !versus.sent,
            give_up: versus.give_up,
            sent: versus.sent,
            over,
        })
    }
}

/// [`Scripts::versus_update`] on the room.
pub(super) fn update(room: &mut Room, duel: &Duel, step: Option<DuelStep>, series: Series) {
    let game_frame = room.game_frame;
    // A fight still to come takes it once it is on screen.
    if room.versus_start.is_some() {
        room.versus_later = Some(Box::new(Later(duel.clone(), step, series)));
        return;
    }
    let Some(fight) = room.fight.as_mut() else {
        return;
    };
    let Some(versus) = fight.versus.as_deref_mut() else {
        return;
    };
    let new = duel.runs.get(versus.taken..).unwrap_or_default();
    if !new.is_empty() {
        info!(
            "game frame {game_frame}: the duel hands the picture {} runs of turn {}",
            new.len(),
            duel.turn()
        );
    }
    (fight.runs).extend(new.iter().map(|run| match versus.side {
        0 => run.clone(),
        _ => duel::mirror(run),
    }));
    versus.taken = duel.runs.len();
    let waited = versus.step;
    let local = |step: Option<DuelStep>| matches!(step, Some(DuelStep::Choose { side, .. }) if side == versus.side);
    if waited != step {
        if local(waited) {
            fight.menu = None;
            fight.asks = None;
            fight.preview = (0, 0);
            versus.shadow = None;
            versus.give_up = None;
        }
        versus.sent = false;
        versus.choosing = 0;
        versus.synced = false;
    }
    (versus.duel, versus.step, versus.series) = (duel.clone(), step, series);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dice_tumble_land_and_say_who_goes_first_in_about_two_and_a_half_seconds() {
        // One roll: 64 game frames, two and a half seconds at 25 a second.
        assert_eq!(dice::frames(1), 64);
        assert_eq!(dice::landed(1), dice::TUMBLE);
        assert_eq!(dice::roll(1, 0), (0, 0));
        assert_eq!(dice::roll(1, 63), (0, 63));
        // A draw: the second roll starts behind the first one's line.
        assert_eq!(dice::frames(2), dice::AGAIN + 64);
        assert_eq!(dice::roll(2, dice::AGAIN - 1), (0, dice::AGAIN - 1));
        assert_eq!(dice::roll(2, dice::AGAIN), (1, 0));
        assert_eq!(dice::roll(2, 1000).0, 1);
    }

    #[test]
    fn the_hud_of_two_players_shows_the_one_opponent_in_the_first_box() {
        assert_eq!(hud_standing(0), 0);
        assert_eq!(hud_standing(1 << 6), 1 << FIRST_SLOT);
        let record = |hp: i16| {
            let mut bytes = [0u8; Record::HEAD];
            bytes[8..10].copy_from_slice(&hp.to_le_bytes());
            Record::head(&bytes)
        };
        let mut records: [Option<Record>; SLOTS] = Default::default();
        records[2] = record(30);
        records[4] = record(10);
        records[5] = record(20);
        let marks = Marks {
            times: [0, 7, 0],
            ..Marks::default()
        };
        let (shown, marks) = hud_view(&records, 1 << 5, marks);
        assert_eq!(shown[FIRST_SLOT].as_ref().map(|record| record.hp), Some(20));
        assert!(shown[5].is_none() && shown[6].is_none());
        assert_eq!(shown[2].as_ref().map(|record| record.hp), Some(30));
        assert_eq!(marks.times, [7, 0, 0]);
    }
}
