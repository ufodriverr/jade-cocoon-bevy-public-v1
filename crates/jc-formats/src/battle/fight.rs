
use super::arena::Pick;
use super::lesson::{self, Who};
use super::record::{Draws, Record, Side, stat, status};
use super::resolve::{Command, Order, Resolved, outcome, resolve};
use super::{EMPTY, Tables, item};
use crate::mover::ArcTangent;
use crate::skeleton::cosine;
use crate::sound;

/// The slots: the player, his three carried creatures, three opponents.
pub const SLOTS: usize = 7;
pub const PLAYER: usize = 0;
pub const OPPONENTS: usize = 4;

/// The battle flags this keeps.
pub mod flag {
    /// The master waits to come back in: his creature fell.
    pub const MASTER_WAITS: u32 = 0x02;
    pub const PLAYER_FELL: u32 = 0x04;
    pub const MINION_FELL: u32 = 0x08;
    pub const OPPONENT_FELL: u32 = 0x40;
    pub const PLAYER_RAN: u32 = 0x80;
    /// A lesson ended the fight, and the lesson refused the player's choice.
    pub const LESSON_ENDED: u32 = 0x100;
    pub const REFUSED: u32 = 0x2000;
}

/// The outcome word.
pub mod ended {
    pub const WON: u32 = 0;
    /// The field is empty and nothing was beaten or caught.
    pub const EMPTY: u32 = 1;
    pub const RAN: u32 = 2;
    pub const LOST: u32 = 3;
    /// A lesson came to the end of its list, or refused what the player chose.
    pub const TAUGHT: u32 = 4;
    pub const REFUSED: u32 = 5;
}

/// The reactions a turn or a result shows.
pub mod reaction {
    pub const CAUGHT: u8 = 5;
    pub const HURT: u8 = 9;
    pub const HURT_BADLY: u8 = 10;
    pub const FALLS: u8 = 11;
    pub const DODGES: u8 = 12;
    pub const CANNOT_ACT: u8 = 14;
    pub const WAKES: u8 = 15;
    pub const SLEEPS: u8 = 16;
    pub const STOPS_DEFENDING: u8 = 17;
    pub const DEFENDS: u8 = 18;
    pub const STONE_DEATH: u8 = 19;
    pub const NONE: u8 = 20;
    pub const REGENERATES: u8 = 25;
}

/// A gauge that reaches this acts; the gauge of a fighter that just came in.
const FULL: i16 = 1000;
const CAME_IN: i16 = 800;
/// The third slot's thing that regenerates, and the status bits of a turn's start.
const REGENERATING_THING: u8 = 16;
pub(super) const LAST_STONE: u32 = 0x0200;
const REGENERATION: u32 = 0x4000;
pub(super) const UNABLE: u32 = 0x10000;
/// The bits that, newly given by an action, make the second runner show the status.
pub(super) const SHOWN_WHEN_GIVEN: u32 = 0x18202;
/// The bad statuses, and the one item that may be aimed at the dead.
const BAD: u32 = 0x07FB;
const REVIVES: usize = 13;
/// The command kinds the result records name.
pub(super) mod kind {
    pub const ATTACK: u32 = 0;
    pub const SPELL: u32 = 1;
    pub const SUMMON: u32 = 2;
    pub const LEAVE: u32 = 3;
    pub const ITEM: u32 = 4;
    pub const CAPTURE: u32 = 5;
    pub const BODY: u32 = 6;
    pub const NOTHING: u32 = 7;
    /// A master sends his creature in, and comes back when it fell.
    pub const SENDS_IN: u32 = 21;
    pub const COMES_BACK: u32 = 22;
    /// The capture that takes the last opponent on the field.
    pub const LAST_CAPTURE: u32 = 26;
}
pub mod took {
    /// From the last game frame of an ordinary action's runner to the test of the second
    /// runner (`0x800BBC40`): the wait sees the runner done, then the test. A second
    /// runner is made by the test, so its first state runs in that game frame.
    pub const TO_THE_SECOND: u32 = 2;
    /// From the last game frame of a runner to the hub's pass: behind an ordinary action
    /// the wait and the test of the second runner come first; behind a second runner
    /// and behind a capture only the wait.
    pub const TO_THE_HUB: u32 = 3;
    pub const STRAIGHT_TO_THE_HUB: u32 = 2;
    /// From the hub's pass to the start of the turn: the hub, the next actor.
    pub const TO_THE_TURN: u32 = 2;
    /// From the start of a turn to the runner of its reaction (the handler makes it),
    /// and to the hub's pass when the turn is lost without a picture.
    pub const TO_ITS_REACTION: u32 = 1;
    pub const LOST: u32 = 1;
    /// From the start of a turn to the choice: the state between (`0x800BBA30`). Behind
    /// the runner of a turn's reaction the same state follows the wait and the test.
    pub const TO_THE_CHOICE: u32 = 2;
    pub const BEHIND_ITS_REACTION: u32 = 4;
    /// From the choice to the runner: an opponent's decision is written in the state of
    /// the choice and the handler follows; on the player's side the state behind the
    /// menu comes between. Stand-in: the menu itself takes no game frame.
    pub const AN_OPPONENT_CHOOSES: u32 = 1;
    pub const THE_PLAYER_CHOOSES: u32 = 2;
    /// From the hub's pass that shows a message of a lesson to its next pass: the two
    /// states that wait for the window. The window's own game frames are not counted
    /// here: whoever shows the fight holds the director for them ([`super::Saying`]).
    pub const A_MESSAGE: u32 = 3;
}

pub(super) fn shaken(actor: usize, own: (i32, i32), decided: &[(usize, Resolved, u8)]) -> u8 {
    let mut shakes = u8::from(own.0 > 0 || own.1 > 0) << actor;
    for &(slot, result, _) in decided {
        let shaken = match result.outcome {
            outcome::HIT => result.hp > 0,
            outcome::MP_HIT => result.mp > 0,
            outcome::HP_AND_MP => result.hp > 0 || result.mp > 0,
            outcome::CRITICAL => true,
            _ => false,
        };
        shakes = shakes & !(1 << slot) | u8::from(shaken) << slot;
    }
    shakes
}

pub(super) fn flashed(decided: &[(usize, Resolved, u8)]) -> u8 {
    let mut flashes = 0;
    for &(slot, result, _) in decided {
        let critical = result.outcome == outcome::CRITICAL;
        flashes = flashes & !(1 << slot) | u8::from(critical) << slot;
    }
    flashes
}

/// The sound of a command's targets (`0x800BF108`, the cases of the result's kind): a
/// target that lost HP or MP by a hit, one that was hit critically and one that was
/// missed each write the half word, in slot order, so the last of them is heard.
pub(super) fn heard(decided: &[(usize, Resolved, u8)]) -> Option<u32> {
    let of = |&(_, result, _): &(usize, Resolved, u8)| match result.outcome {
        outcome::HIT | outcome::MP_HIT | outcome::HP_AND_MP if result.hp > 0 || result.mp > 0 => {
            Some(sound::HIT_SOUND)
        }
        outcome::CRITICAL => Some(sound::CRITICAL_SOUND),
        outcome::MISS => Some(sound::MISS_SOUND),
        _ => None,
    };
    decided.iter().rev().find_map(of)
}

/// The number tasks of the slots (`0x800BEA04` by the shown kind of a slot, as
/// [`shaken`] goes by it): a hit counts the HP change, an MP hit the MP change, both
/// count both, and a critical hit a task of way 2 and the HP change. The actor's kind
/// says which of its own changes of a drain are not zero.
pub(super) fn counted(
    actor: usize,
    own: (i32, i32),
    decided: &[(usize, Resolved, u8)],
) -> Vec<Counted> {
    let mut shown = [(outcome::NOTHING, 0, 0); SLOTS];
    shown[actor] = match own {
        (0, 0) => shown[actor],
        (hp, 0) => (outcome::HIT, hp, 0),
        (0, mp) => (outcome::MP_HIT, 0, mp),
        (hp, mp) => (outcome::HP_AND_MP, hp, mp),
    };
    for &(slot, result, _) in decided {
        shown[slot] = (result.outcome, result.hp, result.mp);
    }
    let mut counted = Vec::new();
    for (slot, (kind, hp, mp)) in shown.into_iter().enumerate() {
        let ways: &[(u8, i32)] = match kind {
            outcome::HIT => &[(0, hp)],
            outcome::MP_HIT => &[(1, mp)],
            outcome::HP_AND_MP => &[(0, hp), (1, mp)],
            outcome::CRITICAL => &[(2, 0), (0, hp)],
            _ => &[],
        };
        counted.extend(
            ways.iter()
                .map(|&(way, amount)| Counted { slot, way, amount }),
        );
    }
    counted
}

impl Run {
    /// The actor's own HP change is a loss, which shakes the picture at the actor's
    /// delay.
    pub fn own_loss(&self) -> bool {
        self.own.0 > 0
    }

    /// The runner of a reaction of a turn's start: the reaction is the command of the
    /// slot, without targets. The start of the turn has made the slot's shown kind of
    /// its own two words (`0x800BB5B0`), so the slot starts the tasks of that kind at
    /// its delay too, with the entries of the change tables, which are 0.
    pub(super) fn of_reaction(
        slot: usize,
        reaction: u8,
        (hp, mp): (i32, i32),
        player_fell: bool,
    ) -> Self {
        let empty = |way: u8| Counted {
            slot,
            way,
            amount: 0,
        };
        Self {
            kind: reaction.into(),
            parameter: 0,
            actors: 1 << slot,
            targets: 0,
            reactions: Vec::new(),
            own: (hp, mp),
            counted: (hp != 0)
                .then(|| empty(0))
                .into_iter()
                .chain((mp != 0).then(|| empty(1)))
                .collect(),
            given: Vec::new(),
            ticked: Vec::new(),
            shakes: 0,
            flashes: 0,
            effect_bytes: [0; SLOTS],
            player_fell,
            before: 0,
            camera: None,
            sounds: Sounds::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Ticked {
    /// The status word before and after.
    pub old: u32,
    pub new: u32,
    /// The change the tick brings, a loss above 0: what its reaction's runner applies.
    pub hp: i32,
    pub mp: i32,
    pub reaction: u8,
    /// The fighter falls: the last stone step, or a loss of all its HP.
    pub falls: bool,
    /// The turn is lost (still asleep, or unable) and the change is not applied.
    pub lost: bool,
}

/// The status tick of a turn's start on a record: the stone group moves up, poison,
/// regeneration and the end of a defence count, and one draw decides whether a sleeper
/// wakes. The new status word is written; the HP and MP change is only answered.
pub(super) fn tick(record: &mut Record, draws: &mut impl Draws) -> Ticked {
    let old = record.status;
    // The group moves up as a whole (`0x800BFC04`): the last step would become the
    // dead bit, but nobody starts a turn alive with the last step.
    let mut new = old & !status::STONE | (old & status::STONE) << 1;
    if record.equipment[2] == REGENERATING_THING {
        new |= REGENERATION;
    }
    let (mut hp, mut mp) = (0, 0);
    if old & status::POISON != 0 {
        hp = i32::from(record.max_hp) / 10;
        new &= !status::SLEEP;
    } else if new & REGENERATION != 0 {
        hp = -(i32::from(record.max_hp) / 20);
    }
    if old & status::DEFENDING != 0 {
        mp = -(i32::from(record.level) / 4 + 3);
        new &= !status::DEFENDING;
    }
    if new & LAST_STONE != 0 {
        hp = record.hp.into();
    }
    if old & status::SLEEP != 0 && draws.draw() % 100 >= 77 {
        new &= !status::SLEEP;
    }
    record.status = new;
    let (set, newly, gone) = (new, new & !old, old & !new);
    let reaction = if set & LAST_STONE != 0 {
        reaction::STONE_DEATH
    } else if newly & status::SLEEP != 0 {
        reaction::SLEEPS
    } else if newly & status::DEFENDING != 0 {
        reaction::DEFENDS
    } else if set & status::POISON != 0 {
        reaction::HURT
    } else if gone & status::SLEEP != 0 {
        reaction::WAKES
    } else if gone & status::DEFENDING != 0 {
        reaction::STOPS_DEFENDING
    } else if set & REGENERATION != 0 {
        reaction::REGENERATES
    } else {
        reaction::NONE
    };
    let cannot = |bit: u32| old & new & bit != 0;
    Ticked {
        old,
        new,
        hp,
        mp,
        reaction,
        falls: reaction == reaction::STONE_DEATH || hp >= i32::from(record.hp),
        lost: cannot(status::SLEEP) || cannot(UNABLE),
    }
}

/// The species whose presence closes the capture: bosses (`0x800BFB50`).
pub fn is_boss(species: u32) -> bool {
    (41..=56).contains(&species) || (201..=210).contains(&species)
}

/// One opponent of a fight block: what the set-up makes its record from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spec {
    pub species: u32,
    pub experience: u32,
    /// 0 is "by species".
    pub ai: u32,
}

/// The head of a fight block and its opponents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// 0 a master with his creature, 1 a script's single fight, 2 a roaming creature.
    pub mode: u32,
    /// How many opponents stand on the field at once.
    pub on_field: usize,
    pub retreat: bool,
    pub results: bool,
    /// Up to three; the list ends at the first species above 210.
    pub specs: Vec<Spec>,
}

/// What the player's side brings into a fight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Party {
    pub player: Record,
    /// The carried creatures in slot order, at most three.
    pub minions: Vec<Record>,
    pub cocoons: u32,
    /// The items he carries: number and pieces.
    pub items: Vec<(usize, u32)>,
    /// How many of the twelve records of caught creatures are free.
    pub room: usize,
}

/// The record of the results screen for one opponent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Beaten {
    /// The species when it stood on the field at the start or was beaten, else -1 (a
    /// capture does not write it).
    pub species: i32,
    /// The slot that did it, the opponent's level, and the kind of the command: 7 for
    /// "nothing happened to this one", 5 or 26 for a capture.
    pub by: usize,
    pub level: u8,
    pub kind: u32,
}

/// What a fighter is told to do, with the mask of its targets (1 << slot).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Attack {
        targets: u8,
    },
    Defend,
    Spell {
        spell: usize,
        targets: u8,
    },
    Body {
        slot: usize,
        targets: u8,
    },
    Flee,
    /// The player calls a carried creature (slot 1 to 3) in; a minion changes places
    /// with another one, or with the player (slot 0).
    Summon {
        slot: usize,
    },
    Change {
        slot: usize,
    },
    Capture {
        targets: u8,
    },
    Item {
        item: usize,
        targets: u8,
    },
    /// The master of a fight of mode 0 sends his creature in and steps out (kind 21).
    Swap,
    Left {
        kind: u8,
    },
}

impl Choice {
    /// The kind of the command the choice becomes ("The command record and its
    /// kinds"): what the judge of a lesson reads.
    pub fn kind(&self) -> i32 {
        match self {
            Choice::Attack { .. } => 0,
            Choice::Spell { .. } => 1,
            Choice::Summon { .. } => 2,
            Choice::Change { slot: PLAYER } => 3,
            Choice::Item { .. } => 4,
            Choice::Capture { .. } => 5,
            Choice::Body { .. } => 6,
            Choice::Flee => 7,
            Choice::Change { .. } => 8,
            Choice::Defend => 18,
            Choice::Swap => 21,
            Choice::Left { kind } => i32::from(*kind),
        }
    }
}

/// Why a choice is not taken. The turn is not spent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Nobody was asked, or the fight is over.
    NobodyChooses,
    /// The fighter does not have this command (the player has no spells and no body
    /// attacks, a minion no capture, no items and no flight, an opponent no summon).
    NotItsCommand,
    /// A closed root command: no carried creature, a boss or no cocoon, no retreat.
    Closed,
    NotEnoughMp,
    /// The picked creature is dead, is not there, or fights already.
    NotThatCreature,
    /// A target that is not on the field, or a row the tables do not have.
    NoSuchTarget,
    NoRoom,
    /// No piece of the item, or an item that cannot be used in a fight.
    NoPiece,
    NotInAFight,
    /// The item is not for this target: the player or a minion, or the dead.
    NotForThat,
    /// The item would do nothing to this target.
    NoUse,
    /// The routine of this opponent's AI type or species is not read.
    NotBuilt,
}

/// What happened, in the order the game decides it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The race gave the turn to this slot.
    Turn {
        slot: usize,
    },
    /// The start of a turn changed the fighter's status and shows a reaction.
    Tick {
        slot: usize,
        reaction: u8,
        hp: i32,
        mp: i32,
    },
    /// The turn was lost: asleep or unable.
    Lost {
        slot: usize,
    },
    /// One call of the resolver, with the changes cut to what the target has.
    Resolved {
        actor: usize,
        target: usize,
        result: Resolved,
        reaction: u8,
    },
    /// The second runner of an ordinary action: a target on the field shows the
    /// status it stands with. Nothing changes with it.
    Shows {
        slot: usize,
        reaction: u8,
    },
    /// A fighter fell, left the field, or came in.
    Fell {
        slot: usize,
    },
    Left {
        slot: usize,
    },
    Entered {
        slot: usize,
    },
    /// An opponent was caught: its record is stored as it stands.
    Caught {
        slot: usize,
    },
    /// A lesson shows a message: the number of a text of the room.
    Says {
        message: i32,
    },
    Over {
        outcome: u32,
    },
}

/// Where a lesson stands in its list.
#[derive(Debug, Clone)]
struct Lesson {
    steps: Vec<lesson::Step>,
    /// The step counter; `None` in front of the first step.
    at: Option<usize>,
    /// The message of a refusal that the hub has still to show.
    pending: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub kind: u32,
    pub parameter: u32,
    /// The masks of the actors and the targets (1 << slot).
    pub actors: u8,
    pub targets: u8,
    pub reactions: Vec<Shown>,
    pub own: (i32, i32),
    /// The number tasks the slots start at their delays, by their shown kinds.
    pub counted: Vec<Counted>,
    /// The status bits the run clears and adds, each at the delay of its slot.
    pub given: Vec<Given>,
    pub ticked: Vec<(usize, u32)>,
    pub shakes: u8,
    /// The slots whose shown change is a critical hit, which makes the screen white
    /// at the delay of the slot, in front of the shake (1 << slot).
    pub flashes: u8,
    pub effect_bytes: [u8; SLOTS],
    /// The player has fallen when the run is shown (flag 4), which makes its end wait
    /// for everybody.
    pub player_fell: bool,
    /// The director's game frames in front of the run ([`took`]): its first state runs
    /// this many game frames behind the last game frame of the run before it, and for
    /// the first run of a fight behind the hub's first pass.
    pub before: u32,
    pub camera: Option<Pick>,
    /// What the run plays at its key moment.
    pub sounds: Sounds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Sounds {
    /// The sound of the last target, in slot order, that lost HP or MP, was hit
    /// critically or was missed.
    pub hit: Option<u32>,
    /// Somebody fell.
    pub fall: Option<Fall>,
    /// A row of the fight's sequences that the command's handler starts in front of
    /// the run: the change of a creature for another (`0x800BBE14`).
    pub start: Option<usize>,
}

/// What is heard of a fall: the row of the fight's sequences or the sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fall {
    Row,
    Sound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counted {
    pub slot: usize,
    /// 0 the HP, 1 the MP, 2 neither (the task of a critical hit that changes nothing).
    pub way: u8,
    /// The change, a loss above 0.
    pub amount: i32,
}

/// The status bits a slot loses and gets at its delay (`0x800BE90C`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Given {
    pub slot: usize,
    pub clear: u32,
    pub add: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Saying {
    /// The number of a text of the room.
    pub message: i32,
    /// The run the message stands in front of: the count of the runs when the hub shows
    /// it, so the count of all runs for a message behind the last one.
    pub run: usize,
    /// The hub's pass that shows it, in the director's game frames as [`Run::before`]
    /// counts them. The window's own game frames are not in any count of the rules.
    pub at: u32,
}

/// The reaction record of a slot in a [`Run`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shown {
    pub slot: usize,
    pub kind: u32,
    pub parameter: u32,
}

/// What the fight waits for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// This slot chooses a command.
    Choose {
        slot: usize,
    },
    Over {
        outcome: u32,
    },
}

/// The closed root commands of the player's menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Closed {
    pub summon: bool,
    pub capture: bool,
    pub flee: bool,
}

/// A fight in progress.
#[derive(Debug, Clone)]
pub struct Fight {
    /// The record of each slot; an opponent's stays when it has left the field.
    pub records: [Option<Record>; SLOTS],
    pub gauges: [i16; SLOTS],
    /// The slot of the player's side that fights: 0 or the minion.
    pub fighter: usize,
    pub flags: u32,
    /// Which opponents stand on the field, and their AI types.
    pub on_field: [bool; 3],
    pub ai: [u32; 3],
    /// The deva in force and the fixed deva of the place ([`NO_DEVA`] for none).
    pub deva: i16,
    pub place_deva: i16,
    pub retreat: bool,
    pub results: bool,
    pub cocoons: u32,
    /// The items of the player (number and pieces), the caught creatures as they stood
    /// when they were caught, and the free records left for more.
    pub items: Vec<(usize, u32)>,
    pub caught: Vec<Record>,
    pub room: usize,
    pub beaten: [Beaten; 3],
    pub events: Vec<Event>,
    /// The runs of the action runner so far, for the picture.
    pub runs: Vec<Run>,
    /// The messages of a lesson so far, for the picture.
    pub said: Vec<Saying>,
    /// The status words of turn starts that no run has taken yet ([`Run::ticked`]).
    ticked: Vec<(usize, u32)>,
    /// The game frames from the last game frame of the last run (from the hub's first
    /// pass, when there is none) to the director's own end ([`took`]), once the fight
    /// is over.
    pub done: u32,
    /// Where the director stands: the game frames from the last game frame of the last
    /// run to the hub's next pass, or to the state of the choice while somebody chooses.
    director: u32,
    /// A fight of mode 0 is a master (opponent 0) with his creature (opponent 1);
    /// `pair` says that the creature is in front.
    pub mode: u32,
    pub pair: bool,
    /// What the enemy's decision keeps between turns: the master has sent his creature
    /// in, the counter of the fixed types, and the creature of type 13 has made ready
    /// for its strike.
    pub(super) sent: bool,
    pub(super) counter: u8,
    pub(super) charged: bool,
    /// The reaction of the status tick of the turn that runs: what the command record
    /// holds until somebody writes a command.
    pub(super) began: u8,
    /// Who was asked to choose, and the outcome once the fight is over.
    pub(super) choosing: Option<usize>,
    outcome: Option<u32>,
    lesson: Option<Lesson>,
    /// Battle flag `0x100000`: the last opponent that fell is of a species whose fall
    /// is heard as a row of the fight's sequences.
    falls_with_the_row: bool,
}

pub fn surprise(arc: &ArcTangent, player: ([i32; 3], i16), opponent: ([i32; 3], i16)) -> u32 {
    let (at, facing) = (player.0, i32::from(player.1));
    if cosine(facing - i32::from(opponent.1)) < 0 {
        return 0;
    }
    let towards = arc.angle(opponent.0[0] - at[0], opponent.0[2] - at[2]);
    match cosine(i32::from(towards) - facing) >= 0 {
        true => 1,
        false => 2,
    }
}

impl Fight {
    /// The set-up: the opponents' records (six draws each, in order), the start gauges
    /// by the surprise number (0 none, 1 the player surprises, 2 the opponent does).
    /// `None` for a species the set-up of a record does not make.
    pub fn new(
        tables: &Tables,
        block: &Block,
        party: Party,
        surprise: u32,
        place_deva: i16,
        draws: &mut impl Draws,
    ) -> Option<Self> {
        let mut records: [Option<Record>; SLOTS] = Default::default();
        let (mut on_field, mut ai) = ([false; 3], [0; 3]);
        let mut beaten = [Beaten {
            species: -1,
            by: 0,
            level: 0,
            kind: 7,
        }; 3];
        let specs = block.specs.iter().take_while(|spec| spec.species < EMPTY);
        for (n, spec) in specs.take(3).enumerate() {
            records[OPPONENTS + n] =
                Some(Record::make(tables, spec.species, spec.experience, draws)?);
            ai[n] = spec.ai;
            on_field[n] = n < block.on_field;
            if on_field[n] {
                beaten[n].species = spec.species as i32;
            }
        }
        records[PLAYER] = Some(party.player);
        for (n, minion) in party.minions.into_iter().take(3).enumerate() {
            records[1 + n] = Some(minion);
        }
        let gauges = match surprise {
            1 => [1000, 0, 0, 0, 0, -700, -1400],
            2 => [0, 0, 0, 0, 1000, 500, 333],
            _ => [500, 0, 0, 0, 0, -700, -1400],
        };
        Some(Self {
            records,
            gauges,
            fighter: PLAYER,
            flags: 0,
            on_field,
            ai,
            deva: place_deva,
            place_deva,
            retreat: block.retreat,
            results: block.results,
            cocoons: party.cocoons,
            items: party.items,
            caught: Vec::new(),
            room: party.room,
            beaten,
            events: Vec::new(),
            runs: Vec::new(),
            said: Vec::new(),
            ticked: Vec::new(),
            done: 0,
            director: 0,
            mode: block.mode,
            pair: false,
            sent: false,
            counter: 0,
            charged: false,
            began: reaction::NONE,
            choosing: None,
            outcome: None,
            lesson: None,
            falls_with_the_row: false,
        })
    }

    /// Makes the fight a lesson (`0x800A166C` of file 27 fills the lesson's block): it
    /// goes by these steps from the first one, and no message is pending.
    pub fn teach(&mut self, steps: Vec<lesson::Step>) {
        self.lesson = Some(Lesson {
            steps,
            at: None,
            pending: None,
        });
    }

    /// The step of the lesson that is in turn: on the player's step what the judge
    /// asks of him, on the opponent's what it does.
    pub fn taught(&self) -> Option<&lesson::Step> {
        let lesson = self.lesson.as_ref()?;
        lesson.steps.get(lesson.at?)
    }

    /// Whether a slot stands on the field.
    pub fn stands(&self, slot: usize) -> bool {
        match slot {
            0..OPPONENTS => slot == self.fighter && self.records[slot].is_some(),
            OPPONENTS..SLOTS => self.on_field[slot - OPPONENTS],
            _ => false,
        }
    }

    /// The mask of the opponents on the field.
    pub fn opponents(&self) -> u8 {
        (OPPONENTS..SLOTS)
            .filter(|slot| self.stands(*slot))
            .fold(0, |mask, slot| mask | 1 << slot)
    }

    pub fn outcome(&self) -> Option<u32> {
        self.outcome
    }

    /// Whether an opponent on the field is of a boss species (`0x800BFB50`).
    pub(super) fn boss_stands(&self) -> bool {
        let species = |slot: usize| self.records[slot].as_ref().map_or(EMPTY, |r| r.species);
        (OPPONENTS..SLOTS).any(|slot| self.stands(slot) && is_boss(species(slot)))
    }

    fn camera_of(&self, actor: usize, kind: u32) -> Option<Pick> {
        if self.boss_stands() {
            return Some(Pick::Boss);
        }
        let none = match actor {
            PLAYER => matches!(kind, 17 | 18 | 20 | 25),
            1..OPPONENTS => matches!(kind, 17 | 18),
            _ => false,
        };
        (!none).then_some(Pick::Plain)
    }

    pub fn asked_at(&self) -> Option<u32> {
        self.choosing.map(|_| self.director)
    }

    /// The root commands the player's menu shows closed.
    pub fn closed(&self) -> Closed {
        let boss = self.boss_stands();
        Closed {
            summon: self.records[1..OPPONENTS].iter().all(Option::is_none),
            capture: boss || self.cocoons == 0,
            flee: !self.retreat,
        }
    }

    /// The turn hub: goes on until somebody has to choose or the fight is over.
    pub fn next(&mut self, tables: &Tables, draws: &mut impl Draws) -> Step {
        loop {
            if let Some(outcome) = self.outcome {
                return Step::Over { outcome };
            }
            if let Some(slot) = self.choosing {
                return Step::Choose { slot };
            }
            // A lesson has its own hub: none of the tests below runs.
            if self.lesson.is_some() {
                self.lesson_goes_on(draws);
                continue;
            }
            // 1. The fight is over.
            let fled_or_fell = self.flags & (flag::PLAYER_FELL | flag::PLAYER_RAN) != 0;
            let waits = self.flags & flag::MASTER_WAITS != 0;
            if fled_or_fell || !waits && self.opponents() == 0 {
                let outcome = if self.flags & flag::PLAYER_FELL != 0 {
                    ended::LOST
                } else if self.flags & flag::PLAYER_RAN != 0 {
                    ended::RAN
                } else if self.flags & flag::OPPONENT_FELL != 0 {
                    ended::WON
                } else {
                    ended::EMPTY
                };
                self.outcome = Some(outcome);
                self.done = self.director;
                self.events.push(Event::Over { outcome });
                continue;
            }
            if self.flags & flag::MINION_FELL != 0 {
                self.flags &= !flag::MINION_FELL;
                let handler = self.director + took::TO_ITS_REACTION;
                self.moves(kind::LEAVE, PLAYER, 0, handler);
                self.director = took::TO_THE_HUB;
                self.player_returns();
                continue;
            }
            // 3. The creature of a pair fell: the master comes back (kind 22).
            // The hub names the second step of that handler (`0x800BC22C`): the swap,
            // and one runner, the master's own move of kind 22.
            if waits {
                self.flags &= !flag::MASTER_WAITS;
                self.swap();
                let handler = self.director + took::TO_ITS_REACTION;
                self.moves(kind::COMES_BACK, OPPONENTS, 0, handler);
                self.director = took::TO_THE_HUB;
                continue;
            }
            // 4. The race picks the next actor, and its turn starts.
            let slot = self.race(tables);
            self.events.push(Event::Turn { slot });
            if self.start_turn(slot, draws) {
                self.choosing = Some(slot);
            }
        }
    }

    /// One pass of the hub under a lesson (the first branch of `0x800BB044`): a pending
    /// message is shown; else a refusal ends the fight; else the step counter goes up,
    /// and the step is a message, the turn of the slot it names, or the end of the
    /// list. The gauges are not used; the turn starts as every turn does.
    fn lesson_goes_on(&mut self, draws: &mut impl Draws) {
        let Some(lesson) = self.lesson.as_mut() else {
            return;
        };
        if let Some(message) = lesson.pending.take() {
            return self.says(message);
        }
        let mut step = None;
        if self.flags & flag::REFUSED == 0 {
            let at = lesson.at.map_or(0, |at| at + 1);
            lesson.at = Some(at);
            step = lesson.steps.get(at).copied();
        }
        let slot = match step.map(|step| (step.who, step.message)) {
            Some((Who::Message, message)) => return self.says(message),
            Some((Who::Player, _)) => self.fighter,
            Some((Who::Opponent, _)) => OPPONENTS,
            None => return self.lesson_ends(),
        };
        self.events.push(Event::Turn { slot });
        if self.start_turn(slot, draws) {
            self.choosing = Some(slot);
        }
    }

    /// The hub shows a message of a lesson (`0x800B43D0` of the gates' package) and the
    /// two states behind it wait for its window.
    fn says(&mut self, message: i32) {
        self.events.push(Event::Says { message });
        self.said.push(Saying {
            message,
            run: self.runs.len(),
            at: self.director,
        });
        self.director += took::A_MESSAGE;
    }

    /// The end of a lesson (flag `0x100` and the end routine `0x800BB380`): a fall or
    /// a flight of the player still comes first, a minion that is out is sent off, and
    /// the outcome says whether the lesson refused a choice.
    fn lesson_ends(&mut self) {
        self.flags |= flag::LESSON_ENDED;
        let outcome = if self.flags & flag::PLAYER_FELL != 0 {
            ended::LOST
        } else if self.flags & flag::PLAYER_RAN != 0 {
            ended::RAN
        } else {
            if self.fighter != PLAYER {
                self.player_returns();
            }
            match self.flags & flag::REFUSED {
                0 => ended::TAUGHT,
                _ => ended::REFUSED,
            }
        };
        self.outcome = Some(outcome);
        self.done = self.director;
        self.events.push(Event::Over { outcome });
    }

    /// The judge of a lesson on what the menu hands over (`0x800BBB10`, with
    /// `0x800B44E8` of the gates' package): a choice of the player's side that the step
    /// refuses is not run. Its message is left for the hub, flag `0x2000` is set, and
    /// the answer is `true`: the fight ends with outcome 5.
    fn judged(&mut self, actor: usize, kind: i32) -> bool {
        let refusal = self.taught().and_then(|step| step.refuses(kind));
        let (Some(message), Some(lesson)) = (refusal, self.lesson.as_mut()) else {
            return false;
        };
        if actor >= OPPONENTS {
            return false;
        }
        lesson.pending = Some(message);
        self.flags |= flag::REFUSED;
        self.choosing = None;
        // The state behind the menu names the hub.
        self.director += took::THE_PLAYER_CHOOSES;
        true
    }

    /// The step of a slot's gauge: 10 and its speed with the equipment, or nothing for
    /// a slot that does not stand on the field.
    fn step(&self, tables: &Tables, slot: usize) -> i16 {
        match &self.records[slot] {
            Some(record) if self.stands(slot) => {
                10 + i16::from(record.effective(tables).stats[stat::SPEED])
            }
            _ => 0,
        }
    }

    /// The counter race: passes over the slots in rising order until a gauge is full.
    /// The winner's gauge is emptied and the slots behind it get no step in that pass.
    fn race(&mut self, tables: &Tables) -> usize {
        let steps: [i16; SLOTS] = std::array::from_fn(|slot| self.step(tables, slot));
        loop {
            for (slot, step) in steps.into_iter().enumerate() {
                if slot < OPPONENTS && slot != self.fighter {
                    continue;
                }
                self.gauges[slot] = self.gauges[slot].wrapping_add(step);
                if self.gauges[slot] >= FULL {
                    self.gauges[slot] = 0;
                    return slot;
                }
            }
        }
    }

    /// The start of a turn (`0x800BB550`): the status tick and what follows from it.
    /// Answers whether the fighter chooses a command.
    fn start_turn(&mut self, slot: usize, draws: &mut impl Draws) -> bool {
        // The game frame of this state, counted as the director's are ([`took`]).
        let turn = self.director + took::TO_THE_TURN;
        let Some(record) = self.records[slot].as_mut() else {
            return false;
        };
        let Ticked {
            old,
            new,
            hp,
            mp,
            reaction,
            falls,
            lost,
        } = tick(record, draws);
        if new != old {
            self.ticked.push((slot, new));
        }
        self.began = reaction;
        if falls {
            let reaction = match reaction {
                reaction::STONE_DEATH => reaction,
                _ => reaction::FALLS,
            };
            let (hp, mp) = (record.hp.into(), 0);
            self.events.push(Event::Tick {
                slot,
                reaction,
                hp,
                mp,
            });
            let fell = slot == PLAYER || self.flags & flag::PLAYER_FELL != 0;
            self.push_run(Run {
                before: turn + took::TO_ITS_REACTION,
                ..Run::of_reaction(slot, reaction, (hp, mp), fell)
            });
            self.director = took::TO_THE_HUB;
            self.fell(slot);
            let fall = Some(self.fall());
            if let Some(run) = self.runs.last_mut() {
                run.sounds.fall = fall;
            }
            self.leave_if_fallen(slot);
            return false;
        }
        if lost {
            // The turn is lost and the tick's HP and MP change is not applied.
            self.events.push(Event::Lost { slot });
            self.director = turn + took::LOST;
            return false;
        }
        if reaction == reaction::NONE {
            self.director = turn + took::TO_THE_CHOICE;
            return true;
        }
        // The reaction is shown by a runner, which applies the change of the turn.
        self.events.push(Event::Tick {
            slot,
            reaction,
            hp,
            mp,
        });
        let fell = self.flags & flag::PLAYER_FELL != 0;
        self.push_run(Run {
            before: turn + took::TO_ITS_REACTION,
            ..Run::of_reaction(slot, reaction, (hp, mp), fell)
        });
        self.change(slot, hp, mp);
        let after = self.records[slot]
            .as_ref()
            .map_or(status::DEAD, |r| r.status);
        let chooses = after & (status::DEAD | status::SLEEP | UNABLE) == 0;
        self.director = match chooses {
            true => took::BEHIND_ITS_REACTION,
            false => took::TO_THE_HUB,
        };
        chooses
    }

    /// A run for the picture, with the status words of the turn starts in front of it.
    fn push_run(&mut self, mut run: Run) {
        run.ticked = std::mem::take(&mut self.ticked);
        self.runs.push(run);
    }

    /// Moves a change into a record, a loss counted positive, held between nothing and
    /// the highest; a record that comes to no HP is dead.
    fn change(&mut self, slot: usize, hp: i32, mp: i32) {
        let Some(record) = self.records[slot].as_mut() else {
            return;
        };
        record.hp = (i32::from(record.hp) - hp).clamp(0, record.max_hp.into()) as i16;
        record.mp = (i32::from(record.mp) - mp).clamp(0, record.max_mp.into()) as i16;
        if record.hp == 0 {
            record.status |= status::DEAD;
            record.experience = 0;
        }
    }

    /// A fighter fell: it is dead, and its side's flag is set.
    fn fell(&mut self, slot: usize) {
        if let Some(record) = self.records[slot].as_mut() {
            record.hp = 0;
            record.status |= status::DEAD;
            record.experience = 0;
        }
        self.flags |= match slot {
            PLAYER => flag::PLAYER_FELL,
            1..OPPONENTS => flag::MINION_FELL,
            // The creature of a pair fell: its master has to come back.
            _ if self.mode == 0 && self.pair => flag::OPPONENT_FELL | flag::MASTER_WAITS,
            _ => flag::OPPONENT_FELL,
        };
        // An opponent's fall sets or clears battle flag `0x100000` by its species.
        if let (Some(record), true) = (&self.records[slot], slot >= OPPONENTS) {
            self.falls_with_the_row = sound::falls_with_the_row(record.species);
        }
        self.events.push(Event::Fell { slot });
    }

    /// What is heard of a fall at the key moment of the run that shows it (`0x800BE7CC`,
    /// by the battle flags as they stand then): the row when the player has fallen,
    /// the sound when his creature has, and for an opponent the row or the sound by
    /// the species of the last one that fell.
    fn fall(&self) -> Fall {
        if self.flags & flag::PLAYER_FELL != 0 {
            Fall::Row
        } else if self.flags & flag::MINION_FELL != 0 || !self.falls_with_the_row {
            Fall::Sound
        } else {
            Fall::Row
        }
    }

    /// Master and creature change places (`0x800A2A78` of file 27): the one in front
    /// waits, the other stands on the field with a gauge of 800.
    fn swap(&mut self) {
        self.pair = !self.pair;
        let (comes, goes) = match self.pair {
            true => (1, 0),
            false => (0, 1),
        };
        if self.on_field[goes] {
            self.events.push(Event::Left {
                slot: OPPONENTS + goes,
            });
        }
        (self.on_field[comes], self.on_field[goes]) = (true, false);
        self.gauges[OPPONENTS + comes] = CAME_IN;
        self.events.push(Event::Entered {
            slot: OPPONENTS + comes,
        });
    }

    /// What every way out of a fight does to the player's side (`0x800BAB4C`, and
    /// `0x80011CF4` before it after a loss): the status goes but for the dead bit, and a
    /// carried creature that lives gets a quarter of its HP and MP back. A player that
    /// fell stands up again with one HP.
    pub fn way_out(&mut self) {
        let lost = self.outcome == Some(ended::LOST);
        for (slot, record) in self.records[..OPPONENTS].iter_mut().enumerate() {
            let Some(record) = record else {
                continue;
            };
            if slot == PLAYER && lost {
                (record.status, record.experience) = (0, 0);
                record.hp = (record.hp + 1).min(record.max_hp);
            }
            record.status &= status::DEAD;
            if slot != PLAYER && !record.is_dead() {
                record.hp = (record.hp + record.max_hp / 4).min(record.max_hp);
                record.mp = (record.mp + record.max_mp / 4).min(record.max_mp);
            }
        }
    }

    /// The end of a runner: an opponent that fell leaves the field.
    fn leave_if_fallen(&mut self, slot: usize) {
        let dead = self.records[slot].as_ref().is_some_and(Record::is_dead);
        if slot >= OPPONENTS && dead && self.on_field[slot - OPPONENTS] {
            self.on_field[slot - OPPONENTS] = false;
            self.events.push(Event::Left { slot });
        }
    }

    /// The player steps in for a minion that left or fell.
    fn player_returns(&mut self) {
        let left = self.fighter;
        self.fighter = PLAYER;
        if left != PLAYER {
            self.events.push(Event::Left { slot: left });
        }
        self.events.push(Event::Entered { slot: PLAYER });
        if self.opponents() != 0 {
            self.gauges[PLAYER] = CAME_IN;
        }
    }

    fn moves(&mut self, kind: u32, actor: usize, targets: u8, before: u32) {
        let fell = self.flags & flag::PLAYER_FELL != 0;
        self.push_run(Run {
            kind,
            targets,
            before,
            camera: self.camera_of(actor, kind),
            ..Run::of_reaction(actor, 0, (0, 0), fell)
        });
    }

    /// A carried creature comes in and fights; in a lesson it gets no gauge.
    fn comes_in(&mut self, slot: usize) {
        self.fighter = slot;
        if self.lesson.is_none() {
            self.gauges[slot] = CAME_IN;
        }
        self.events.push(Event::Entered { slot });
    }

    /// The menu's tests of an item and of its target (`0x80033B58`, `0x80033E50`).
    /// Answers the mask the command gets: the item's flags name the side, and an item
    /// for all of a side takes every opponent on the field without picking.
    fn item_targets(&self, tables: &Tables, number: usize, targets: u8) -> Result<u8, Refusal> {
        let pieces = self.items.iter().find(|entry| entry.0 == number);
        if pieces.is_none_or(|entry| entry.1 == 0) {
            return Err(Refusal::NoPiece);
        }
        let flags = tables.items.get(number).ok_or(Refusal::NoPiece)?.flags;
        if flags & item::IN_A_FIGHT == 0 {
            return Err(Refusal::NotInAFight);
        }
        let one = targets.count_ones() == 1;
        if flags & item::AT_THE_OPPONENTS != 0 {
            let all = self.opponents();
            return if flags & item::ALL_OF_A_SIDE != 0 {
                Ok(all)
            } else if one && targets & all != 0 {
                Ok(targets)
            } else {
                Err(Refusal::NoSuchTarget)
            };
        }
        let slot = targets.trailing_zeros() as usize;
        let record = self.records.get(slot).and_then(Option::as_ref);
        let Some(record) = record.filter(|_| one && slot < OPPONENTS) else {
            return Err(Refusal::NoSuchTarget);
        };
        let allowed = match slot {
            PLAYER => item::ON_THE_PLAYER,
            _ => item::ON_A_MINION,
        };
        if flags & allowed == 0 || record.is_dead() && number != REVIVES {
            return Err(Refusal::NotForThat);
        }
        // Items 0 to 15 are refused where they would do nothing.
        let (hp_full, mp_full) = (record.hp >= record.max_hp, record.mp >= record.max_mp);
        let nothing = match number {
            0..=2 => hp_full,
            9..=12 => hp_full && mp_full,
            REVIVES => record.status & BAD == 0 && hp_full && mp_full,
            14 => record.status & status::POISON == 0,
            15 => record.status & status::STONE == 0,
            _ => false,
        };
        if nothing {
            return Err(Refusal::NoUse);
        }
        Ok(targets)
    }

    /// Takes the command of the fighter that was asked, as the menu would hand it over
    /// (or the enemy's decision), and plays it to the next pass of the hub.
    pub fn act(
        &mut self,
        tables: &Tables,
        choice: Choice,
        odd_frame: bool,
        draws: &mut impl Draws,
    ) -> Result<(), Refusal> {
        let actor = self.choosing.ok_or(Refusal::NobodyChooses)?;
        let chosen = choice.kind();
        let record = self.records[actor].as_ref().ok_or(Refusal::NobodyChooses)?;
        let (player, minion) = (actor == PLAYER, (1..OPPONENTS).contains(&actor));
        let carried = |slot: usize| match self.records.get(slot) {
            Some(Some(record)) if (1..OPPONENTS).contains(&slot) && slot != self.fighter => {
                (!record.is_dead()).then_some(slot)
            }
            _ => None,
        };
        // The root commands of each side, the closed ones, and the lower levels.
        let (command, targets) = match choice {
            Choice::Attack { targets } => (Command::Attack, targets),
            Choice::Defend => (Command::Other, self.opponents()),
            Choice::Spell { spell, targets } if !player => {
                // The MP is the menu's test: an opponent's routine tests for itself, or
                // not at all, and pays what it has.
                let cost = tables.spells.get(spell).ok_or(Refusal::NoSuchTarget)?.cost;
                if minion && i16::from(cost) > record.mp {
                    return Err(Refusal::NotEnoughMp);
                }
                (Command::Spell(spell), targets)
            }
            Choice::Body { slot, targets } if !player => {
                let entry = record.moves.get(slot).ok_or(Refusal::NoSuchTarget)?;
                if minion && entry.cost() > record.mp.into() {
                    return Err(Refusal::NotEnoughMp);
                }
                (Command::Body(slot), targets)
            }
            Choice::Flee if !minion => {
                if player && self.closed().flee {
                    return Err(Refusal::Closed);
                }
                let against = if player {
                    self.opponents()
                } else {
                    1 << self.fighter
                };
                (Command::Flee, against)
            }
            Choice::Summon { slot } if player => {
                if self.closed().summon {
                    return Err(Refusal::Closed);
                }
                let slot = carried(slot).ok_or(Refusal::NotThatCreature)?;
                if self.judged(actor, chosen) {
                    return Ok(());
                }
                self.choosing = None;
                // The player's own run, with the picked creature as its target, and
                // the creature's entrance behind the making of its figure.
                let handler = self.director + took::THE_PLAYER_CHOOSES;
                self.moves(kind::SUMMON, actor, 1 << slot, handler);
                self.moves(kind::SUMMON, slot, 0, took::TO_THE_SECOND);
                self.director = took::TO_THE_HUB;
                self.comes_in(slot);
                return Ok(());
            }
            Choice::Change { slot } if minion => {
                if slot == PLAYER {
                    if self.judged(actor, chosen) {
                        return Ok(());
                    }
                    self.choosing = None;
                    // The creature's leave, and the player who comes back in.
                    let handler = self.director + took::THE_PLAYER_CHOOSES;
                    self.moves(kind::LEAVE, actor, 0, handler);
                    self.moves(kind::LEAVE, PLAYER, 0, took::TO_THE_SECOND);
                    self.director = took::TO_THE_HUB;
                    self.player_returns();
                    return Ok(());
                }
                let slot = carried(slot).ok_or(Refusal::NotThatCreature)?;
                if self.judged(actor, chosen) {
                    return Ok(());
                }
                self.choosing = None;
                // The creature's leave, and the other one's entrance, which is the
                // second half of a summon.
                let handler = self.director + took::THE_PLAYER_CHOOSES;
                self.moves(kind::LEAVE, actor, 0, handler);
                // The handler starts a row for a creature that goes for another.
                if let Some(leave) = self.runs.last_mut() {
                    leave.sounds.start = Some(crate::sound::CHANGE_ROW);
                }
                self.moves(kind::SUMMON, slot, 0, took::TO_THE_SECOND);
                self.director = took::TO_THE_HUB;
                self.events.push(Event::Left { slot: actor });
                self.comes_in(slot);
                return Ok(());
            }
            Choice::Capture { targets } if player => {
                if self.closed().capture {
                    return Err(Refusal::Closed);
                }
                if self.room == 0 {
                    return Err(Refusal::NoRoom);
                }
                if targets.count_ones() != 1 || targets & self.opponents() == 0 {
                    return Err(Refusal::NoSuchTarget);
                }
                (Command::Capture, targets)
            }
            Choice::Item { item, targets } if player => (
                Command::Item(item),
                self.item_targets(tables, item, targets)?,
            ),
            Choice::Swap if actor == OPPONENTS && self.mode == 0 && !self.pair => {
                let creature = self.records[OPPONENTS + 1].as_ref();
                if creature.is_none_or(Record::is_dead) {
                    return Err(Refusal::NotThatCreature);
                }
                self.choosing = None;
                // The handler's two runners (`0x800BC0CC`, `0x800BC134`): the master's
                // own move, then the swap, and the move of the creature that comes in,
                // each the command of kind 21 of its actor with no target.
                let handler = self.director + took::AN_OPPONENT_CHOOSES;
                self.moves(kind::SENDS_IN, actor, 0, handler);
                self.director = took::TO_THE_HUB;
                self.swap();
                self.moves(kind::SENDS_IN, OPPONENTS + 1, 0, took::TO_THE_SECOND);
                return Ok(());
            }
            // Nothing is resolved and no record changes: the turn is lost.
            Choice::Left { kind } if actor >= OPPONENTS => {
                self.choosing = None;
                let before = self.director + took::AN_OPPONENT_CHOOSES;
                self.director = took::TO_THE_HUB;
                let fell = self.flags & flag::PLAYER_FELL != 0;
                self.push_run(Run {
                    before,
                    camera: self.camera_of(actor, kind.into()),
                    ..Run::of_reaction(actor, kind, (0, 0), fell)
                });
                return Ok(());
            }
            _ => return Err(Refusal::NotItsCommand),
        };
        // An item for the own side may go to a carried creature that does not fight.
        let slots: Vec<usize> = (0..SLOTS).filter(|slot| targets >> slot & 1 != 0).collect();
        let carried_too = matches!(command, Command::Item(_)) && targets & 0x0F != 0;
        if slots.is_empty() || !carried_too && !slots.iter().all(|slot| self.stands(*slot)) {
            return Err(Refusal::NoSuchTarget);
        }
        // What the menu hands over goes to the judge of a lesson first.
        if self.judged(actor, chosen) {
            return Ok(());
        }
        let taught = self.taught().copied();
        let side = |slot: usize| match slot {
            PLAYER => Side::Player,
            _ => Side::Creature,
        };

        // Resolution comes first and all at once, in slot order.
        let defends = choice == Choice::Defend;
        let mut decided = Vec::with_capacity(slots.len());
        let (mut deva, mut cost, mut own) = (self.deva, 0, (0, 0));
        for &target in &slots {
            let records = (self.records[actor].as_ref(), self.records[target].as_ref());
            let (Some(acting), Some(hit)) = records else {
                return Err(Refusal::NoSuchTarget);
            };
            let order = Order {
                command,
                actor: acting,
                actor_side: side(actor),
                target: hit,
                target_side: side(target),
                deva,
                lock: self.place_deva,
                odd_frame,
            };
            let mut result = resolve(tables, &order, draws).ok_or(Refusal::NoSuchTarget)?;
            // A lesson rewrites the result (`0x800B4560` of the gates' package): an
            // attack, a spell or a body attack hits for the step's share of the
            // target's HP with one more draw, a capture catches.
            match (taught, chosen) {
                (Some(step), 0 | 1 | 6) => {
                    let forced = step.forced(chosen, hit.hp, draws.draw());
                    (result.outcome, result.hp) = (outcome::HIT, forced.unwrap_or(result.hp));
                }
                (Some(_), 5) => (result.outcome, result.grade) = (outcome::CAPTURE, 0),
                _ => {}
            }
            deva = result.deva;
            // A loss is cut to what the target has, a gain to what it lacks.
            let cut = |change: i32, has: i16, highest: i16| {
                change.clamp(i32::from(has) - i32::from(highest), has.into())
            };
            result.hp = cut(result.hp, hit.hp, hit.max_hp);
            result.mp = cut(result.mp, hit.mp, hit.max_mp);
            if target != actor {
                own = (own.0 + result.actor_hp, own.1 + result.actor_mp);
            }
            if result.cost != 0 {
                cost = result.cost.min(acting.mp.into());
            }
            let guards = hit.status & status::DEFENDING != 0;
            let mut reaction = match result.outcome {
                outcome::HIT | outcome::MP_HIT | outcome::HP_AND_MP
                    if result.hp > 0 || result.mp > 0 =>
                {
                    reaction::HURT
                }
                outcome::CRITICAL => reaction::HURT_BADLY,
                outcome::MISS if !guards => reaction::DODGES,
                _ => reaction::NONE,
            };
            let dies =
                result.hp >= i32::from(hit.hp) || result.add & (LAST_STONE | status::DEAD) != 0;
            if dies && !defends && command != Command::Flee {
                result.hp = hit.hp.into();
                reaction = reaction::FALLS;
            }
            // Grades 0 and 2 of a capture catch.
            if result.outcome == outcome::CAPTURE && matches!(result.grade, 0 | 2) {
                reaction = reaction::CAUGHT;
            }
            decided.push((target, result, reaction));
        }
        // From here on the turn is spent. The handler makes the runner ([`took`]).
        self.choosing = None;
        let handler = self.director
            + match actor {
                0..OPPONENTS => took::THE_PLAYER_CHOOSES,
                _ => took::AN_OPPONENT_CHOOSES,
            };
        self.director = match command {
            Command::Capture => took::STRAIGHT_TO_THE_HUB,
            _ => took::TO_THE_HUB,
        };
        if defends {
            if let Some(record) = self.records[actor].as_mut() {
                record.status |= status::DEFENDING;
            }
            self.push_run(Run {
                kind: chosen as u32,
                parameter: 0,
                actors: 1 << actor,
                targets,
                reactions: Vec::new(),
                own: (0, 0),
                counted: Vec::new(),
                given: Vec::new(),
                ticked: Vec::new(),
                shakes: 0,
                flashes: 0,
                effect_bytes: [0; SLOTS],
                player_fell: self.flags & flag::PLAYER_FELL != 0,
                before: handler,
                camera: self.camera_of(actor, chosen as u32),
                sounds: Sounds::default(),
            });
            return Ok(());
        }
        // The run of the command for the picture. A capture shows its reaction caught or
        // not, and is kind 26 when it takes the last opponent.
        let takes_last = |target: usize, reaction: u8| {
            reaction == reaction::CAUGHT && self.opponents() == 1 << target
        };
        let capture = (decided.first())
            .filter(|_| command == Command::Capture)
            .map(|&(target, result, reaction)| (takes_last(target, reaction), result.grade));
        // The numbers that count: the change of each target and the actor's gain, at
        // the delays of their slots, with the status bits of each target. A flight
        // changes nothing.
        let (numbers, given) = match command {
            Command::Flee => (Vec::new(), Vec::new()),
            _ => (
                counted(actor, own, &decided),
                (decided.iter())
                    .filter(|(_, result, _)| result.clear != 0 || result.add != 0)
                    .map(|&(slot, result, _)| Given {
                        slot,
                        clear: result.clear,
                        add: result.add,
                    })
                    .collect(),
            ),
        };
        let shakes = match command {
            Command::Flee => 0,
            _ => shaken(actor, own, &decided),
        };
        let hit = match command {
            Command::Flee => None,
            _ => heard(&decided),
        };
        let flashes = match command {
            Command::Flee => 0,
            _ => flashed(&decided),
        };
        let mut effect_bytes = [0; SLOTS];
        for &(slot, result, _) in &decided {
            effect_bytes[slot] = result.effects;
        }
        let shown = self.runs.len();
        let camera = self.camera_of(actor, chosen as u32);
        self.push_run(Run {
            kind: match capture {
                Some((true, _)) => kind::LAST_CAPTURE,
                _ => chosen as u32,
            },
            parameter: match command {
                Command::Spell(number) | Command::Item(number) | Command::Body(number) => {
                    number as u32
                }
                _ => capture.map_or(0, |(_, grade)| grade.into()),
            },
            actors: 1 << actor,
            targets,
            reactions: (decided.iter())
                .filter_map(|&(slot, result, reaction)| {
                    let (kind, parameter) = match (result.outcome, reaction) {
                        (outcome::CAPTURE, _) => (reaction::CAUGHT, result.grade.into()),
                        (_, reaction::NONE) => return None,
                        (_, reaction) => (reaction, 0),
                    };
                    Some(Shown {
                        slot,
                        kind: kind.into(),
                        parameter,
                    })
                })
                .filter(|_| command != Command::Flee)
                .collect(),
            // The mana the command costs is the actor's own MP word.
            own: (0, cost),
            counted: numbers,
            given,
            ticked: Vec::new(),
            shakes,
            flashes,
            effect_bytes,
            player_fell: false,
            before: handler,
            camera,
            sounds: Sounds {
                hit,
                fall: None,
                start: None,
            },
        });

        // The actor starts: its cost, and a cocoon or a piece of the item is spent,
        // caught or not. Then the impact on each target in slot order: status cleared,
        // status added, the change; then the actor's own gain.
        self.change(actor, 0, cost);
        match command {
            Command::Capture => self.cocoons = self.cocoons.saturating_sub(1),
            Command::Item(number) => {
                for entry in self.items.iter_mut().filter(|entry| entry.0 == number) {
                    entry.1 = entry.1.saturating_sub(1);
                }
                self.items.retain(|entry| entry.1 != 0);
            }
            _ => {}
        }
        let kind = match command {
            Command::Attack => kind::ATTACK,
            Command::Spell(_) => kind::SPELL,
            Command::Item(_) => kind::ITEM,
            Command::Body(_) => kind::BODY,
            _ => kind::NOTHING,
        };
        let mut stopped = false;
        let somebody_fell = command != Command::Flee
            && (decided.iter()).any(|&(.., reaction)| reaction == reaction::FALLS);
        // What the test of the second runner reads of each target: the outcome and the
        // bits the action gave.
        let given: Vec<_> = decided
            .iter()
            .map(|d| (d.0, d.1.outcome, d.1.add))
            .collect();
        for (target, result, reaction) in decided {
            self.events.push(Event::Resolved {
                actor,
                target,
                result,
                reaction,
            });
            if command == Command::Flee {
                stopped |= result.grade != 0;
                continue;
            }
            if reaction == reaction::CAUGHT {
                // The record is stored as it stands (as hurt as it was, the sleep bit
                // still set), and the opponent leaves the field like a beaten one.
                let last = self.opponents() == 1 << target;
                if let Some(record) = &self.records[target] {
                    self.caught.push(record.clone());
                    self.room = self.room.saturating_sub(1);
                    self.beaten[target - OPPONENTS] = Beaten {
                        by: actor,
                        level: record.level,
                        kind: match last {
                            true => kind::LAST_CAPTURE,
                            false => kind::CAPTURE,
                        },
                        ..self.beaten[target - OPPONENTS]
                    };
                }
                self.flags |= flag::OPPONENT_FELL;
                self.on_field[target - OPPONENTS] = false;
                self.events.push(Event::Caught { slot: target });
                self.events.push(Event::Left { slot: target });
            }
            if let Some(record) = self.records[target].as_mut() {
                record.status = record.status & !result.clear | result.add;
            }
            self.change(target, result.hp, result.mp);
            if reaction == reaction::FALLS {
                if let (Some(record), true) = (&self.records[target], target >= OPPONENTS) {
                    self.beaten[target - OPPONENTS] = Beaten {
                        species: record.species as i32,
                        by: actor,
                        level: record.level,
                        kind,
                    };
                }
                self.fell(target);
            }
        }
        self.deva = deva;
        self.change(actor, own.0, own.1);
        let fell = self.flags & flag::PLAYER_FELL != 0;
        let fall = somebody_fell.then(|| self.fall());
        if let Some(run) = self.runs.get_mut(shown) {
            run.player_fell = fell;
            run.sounds.fall = fall;
        }
        if command == Command::Flee && !stopped {
            if player {
                self.flags |= flag::PLAYER_RAN;
            } else {
                // An opponent that got away leaves the field, and no flag is set.
                self.on_field[actor - OPPONENTS] = false;
                self.events.push(Event::Left { slot: actor });
            }
        }
        // The runner ends: the fallen opponents leave the field.
        for slot in OPPONENTS..SLOTS {
            self.leave_if_fallen(slot);
        }
        // The second runner (`0x800BC378`, not after a capture): when a target that
        // was really hit still lives and either got one of the bits or defends, every
        // target on the field shows the status it stands with.
        let live = |slot: usize| self.records[slot].as_ref().filter(|record| record.hp > 0);
        let asks = |&(target, shown, add): &(usize, u16, u32)| {
            let hit = !matches!(shown, outcome::NOTHING | outcome::MISS | 6);
            let defends = |record: &Record| record.status & status::DEFENDING != 0;
            hit && live(target).is_some_and(|record| add & SHOWN_WHEN_GIVEN != 0 || defends(record))
        };
        if command != Command::Capture && given.iter().any(asks) {
            let mut second = Run {
                kind: reaction::NONE.into(),
                parameter: 0,
                actors: 0,
                targets,
                reactions: Vec::new(),
                own: (0, 0),
                counted: Vec::new(),
                given: Vec::new(),
                ticked: Vec::new(),
                shakes: 0,
                flashes: 0,
                effect_bytes: [0; SLOTS],
                player_fell: fell,
                before: took::TO_THE_SECOND,
                camera: None,
                sounds: Sounds::default(),
            };
            for (slot, ..) in given {
                let status = self.records[slot]
                    .as_ref()
                    .map_or(0, |record| record.status);
                let reaction = [
                    (LAST_STONE, reaction::STONE_DEATH),
                    (UNABLE, reaction::CANNOT_ACT),
                    (status::SLEEP, reaction::SLEEPS),
                    (status::DEFENDING, reaction::DEFENDS),
                ]
                .into_iter()
                .find(|(bit, _)| status & bit != 0);
                if let (Some((_, reaction)), true) = (reaction, self.stands(slot)) {
                    self.events.push(Event::Shows { slot, reaction });
                    second.reactions.push(Shown {
                        slot,
                        kind: reaction.into(),
                        parameter: 0,
                    });
                }
            }
            self.push_run(second);
            self.director = took::STRAIGHT_TO_THE_HUB;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::record::NOTHING;
    use super::super::record::tests::{head, tables as made_up};
    use super::super::resolve::NO_DEVA;
    use super::super::{Descriptor, EARTH, FIRE, Item, Move, Spell, WATER, WIND, class};
    use super::*;

    #[test]
    fn who_is_surprised_goes_by_the_two_facings_and_the_side_the_opponent_is_on() {
        use std::f64::consts::TAU;
        // A table like the game's: the angle of a ratio in 1024ths.
        let step = |ratio: usize| ((ratio as f64 / 1024.0).atan() * 4096.0 / TAU).round() as i16;
        let arc = ArcTangent::of((0..=1024).map(step).collect());
        let he = ([100, 0, 200], 0);
        let (ahead, behind, beside) = ([100, 0, 1200], [100, 50, -800], [1100, 0, 200]);
        // Both look the same way: the one who has the other in front of him surprises.
        assert_eq!(surprise(&arc, he, (ahead, 0)), 1);
        assert_eq!(surprise(&arc, he, (behind, 0)), 2);
        assert_eq!(surprise(&arc, he, (behind, 1000)), 2);
        // They look at each other, or more than a quarter turn apart: nobody.
        assert_eq!(surprise(&arc, he, (ahead, 2048)), 0);
        assert_eq!(surprise(&arc, he, (behind, -1025)), 0);
        // A cosine of zero is not below zero: a quarter turn apart still counts, and an
        // opponent straight to the side is in front.
        assert_eq!(surprise(&arc, he, (ahead, 1024)), 1);
        assert_eq!(surprise(&arc, he, (beside, -1024)), 1);
        // The player's own facing turns both tests with him.
        assert_eq!(surprise(&arc, ([100, 0, 200], 2048), (ahead, 2048)), 2);
    }

    /// The made-up tables with the game's plain attack and a weapon that has an attack.
    fn tables() -> Tables {
        let attack = |accuracy: u8, power: u8| {
            let bytes = [
                0,
                0,
                power,
                0,
                0xFF,
                accuracy,
                0,
                0,
                class::PHYSICAL,
                0,
                3,
                0,
            ];
            Descriptor::parse(&bytes).expect("a descriptor")
        };
        let mut tables = made_up();
        tables.plain_attack = attack(115, 100);
        tables.weapons[0].effect = Some(attack(120, 90));
        // Three spells of 3 MP and a body attack that hit like the plain attack, and
        // the first herb of the game: 40 HP for the player.
        let spell = Spell {
            word: 0,
            cost: 3,
            shown: 0,
            need: 0,
            element: 0,
            effect: attack(115, 100),
        };
        tables.spells = vec![spell; 3];
        tables.body_attacks = vec![attack(115, 100)];
        let herb = [0, 0, 0xD8, 0xFF, 0xFF, 125, 125, 0, class::NONE, 2, 0, 0];
        tables.items = vec![Item {
            flags: 0x4B,
            effect: Descriptor::parse(&herb).expect("a descriptor"),
        }];
        tables
    }

    /// The player of a new game, as the template of the disc has him.
    fn player() -> Record {
        let mut player = head(&"00".repeat(72));
        (player.hp, player.max_hp) = (100, 100);
        player.stats = [20, 23, 20, 23, 20, 0];
        player.modifiers = [56, 56, 56, 56, 32, 32, 32, 16, 32, -32, 32, 32];
        player.strengths = [8; 4];
        (player.level, player.fixed) = (1, true);
        player.equipment = [0, NOTHING, NOTHING];
        player
    }

    fn block(species: &[u32]) -> Block {
        let spec = |species: &u32| Spec {
            species: *species,
            experience: 0,
            ai: 0,
        };
        Block {
            mode: 1,
            on_field: species.len(),
            retreat: true,
            results: true,
            specs: species.iter().map(spec).collect(),
        }
    }

    fn party() -> Party {
        Party {
            player: player(),
            minions: vec![],
            cocoons: 1,
            items: vec![(0, 2)],
            room: 12,
        }
    }

    /// Plays the fight on with the player's fighter choosing `choice` at the opponents
    /// on the field and the opponents deciding for themselves; answers what they chose.
    fn play(
        fight: &mut Fight,
        tables: &Tables,
        draws: &mut impl Draws,
        turns: usize,
    ) -> Vec<(usize, Choice)> {
        let mut chosen = Vec::new();
        while chosen.len() < turns {
            match fight.next(tables, draws) {
                Step::Over { .. } => break,
                Step::Choose { slot } if slot < OPPONENTS => {
                    let targets = fight.opponents();
                    let attack = Choice::Attack { targets };
                    fight.act(tables, attack, false, draws).expect("taken");
                }
                Step::Choose { slot } => {
                    let choice = fight.decide(tables, draws).expect("a routine that is read");
                    fight.act(tables, choice, false, draws).expect("taken");
                    chosen.push((slot, choice));
                }
            }
        }
        chosen
    }

    /// A step of a lesson: a message, or a command of the player (at opponent 0, an
    /// item at himself) or of opponent 0 (at the player) with its forced percent.
    fn told(who: Who, kind: i32, number: i32, percent: i32) -> lesson::Step {
        let (actor, targets) = match (who, kind) {
            (Who::Message, _) => (0, 0),
            (Who::Player, 4) => (0x01, 0x01),
            (Who::Player, _) => (0x01, 0x10),
            (Who::Opponent, _) => (0x10, 0x01),
        };
        let message = match who {
            Who::Message => number,
            _ => 5,
        };
        lesson::Step {
            who,
            kind,
            parameter: 0,
            actor: Some(actor),
            targets,
            message,
            must: true,
            percent,
        }
    }

    fn lessons() -> [Vec<lesson::Step>; 2] {
        let says = |message: i32| told(Who::Message, 24, message, 0);
        let (player, opponent) = (Who::Player, Who::Opponent);
        let first = vec![
            says(11),
            told(player, 0, 0, 90),
            told(opponent, 0, 0, 36),
            says(12),
            told(player, 18, 0, 0),
            told(opponent, 0, 0, 19),
            says(13),
            told(player, 4, 0, 0),
            says(14),
        ];
        let second = vec![
            says(17),
            told(player, 0, 0, 90),
            says(18),
            told(player, 5, 0, 0),
        ];
        [first, second]
    }

    /// A lesson against one opponent of 30 HP.
    fn lesson_fight(tables: &Tables, steps: Vec<lesson::Step>) -> Fight {
        let mut fight =
            Fight::new(tables, &block(&[12]), party(), 0, NO_DEVA, &mut || 0u32).expect("it");
        let it = fight.records[OPPONENTS].as_mut().expect("it");
        (it.hp, it.max_hp) = (30, 30);
        fight.teach(steps);
        fight
    }

    /// Plays a lesson to its end with everybody doing what the step in turn says.
    fn learn(fight: &mut Fight, tables: &Tables, draws: &mut impl Draws) -> u32 {
        loop {
            match fight.next(tables, draws) {
                Step::Over { outcome } => return outcome,
                Step::Choose { slot } if slot < OPPONENTS => {
                    let step = fight.taught().expect("a step in turn");
                    let choice = step.choice().expect("a command");
                    fight.act(tables, choice, false, draws).expect("taken");
                }
                Step::Choose { .. } => fight.opponent_acts(tables, false, draws).expect("taken"),
            }
        }
    }

    fn said(fight: &Fight) -> Vec<i32> {
        let message = |event: &Event| match event {
            Event::Says { message } => Some(*message),
            _ => None,
        };
        fight.events.iter().filter_map(message).collect()
    }

    #[test]
    fn a_lesson_goes_by_its_steps_to_the_end_of_the_list() {
        let tables = tables();
        let [first, second] = lessons();
        // Lesson 0, every draw even: 90 in 100 of the opponent's 30 HP; 36 in 100 of
        // the player's 100, then 19 in 100 of the 64 that are left while he defends;
        // the herb gives 40 back.
        let mut fight = lesson_fight(&tables, first.clone());
        assert_eq!(fight.taught(), None);
        assert_eq!(learn(&mut fight, &tables, &mut || 0u32), ended::TAUGHT);
        assert_eq!(said(&fight), [11, 12, 13, 14]);
        // Where the messages stand for the picture: the first at the hub's first pass
        // in front of the first run, the last behind the last run, and the director's
        // three game frames of each lie in front of the run's first state or the end.
        let shown: Vec<i32> = fight.said.iter().map(|saying| saying.message).collect();
        assert_eq!(shown, [11, 12, 13, 14]);
        assert_eq!((fight.said[0].run, fight.said[0].at), (0, 0));
        assert_eq!(fight.said[3].run, fight.runs.len());
        for saying in &fight.said {
            let room = (fight.runs.get(saying.run)).map_or(fight.done, |run| run.before);
            assert!(saying.at + took::A_MESSAGE <= room, "{saying:?}");
        }
        let hp = |fight: &Fight, slot: usize| fight.records[slot].as_ref().expect("it").hp;
        assert_eq!((hp(&fight, OPPONENTS), hp(&fight, PLAYER)), (3, 92));
        assert_eq!((fight.flags, fight.items.clone()), (0x100, vec![(0, 1)]));
        // The turns are the steps' and not the gauges': the player, the opponent, the
        // player, the opponent, the player. Nobody's gauge moved.
        let turn = |event: &Event| match event {
            Event::Turn { slot } => Some(*slot),
            _ => None,
        };
        let turns: Vec<usize> = fight.events.iter().filter_map(turn).collect();
        assert_eq!(turns, [PLAYER, OPPONENTS, PLAYER, OPPONENTS, PLAYER]);
        assert_eq!(fight.gauges, [500, 0, 0, 0, 0, -700, -1400]);
        assert_eq!(
            fight.events.last(),
            Some(&Event::Over {
                outcome: ended::TAUGHT
            })
        );
        // The lowest bit of the one more draw goes on top: 28 of 30, 37 of 100, and 11
        // and one of 63.
        let mut fight = lesson_fight(&tables, first.clone());
        assert_eq!(learn(&mut fight, &tables, &mut || 1u32), ended::TAUGHT);
        assert_eq!(hp(&fight, OPPONENTS), 2);
        let lost = |event: &Event| match event {
            Event::Resolved { target, result, .. } => Some((*target, result.outcome, result.hp)),
            _ => None,
        };
        let hits: Vec<_> = fight.events.iter().filter_map(lost).take(3).collect();
        let hit = outcome::HIT;
        assert_eq!(
            hits,
            [(OPPONENTS, hit, 28), (PLAYER, hit, 37), (PLAYER, hit, 12)]
        );
        // On its step the opponent is told its command; it does not decide.
        let mut fight = lesson_fight(&tables, first);
        fight.next(&tables, &mut || 0u32);
        let attack = Choice::Attack { targets: 0x10 };
        fight.act(&tables, attack, false, &mut || 0u32).expect("it");
        assert_eq!(
            fight.next(&tables, &mut || 0u32),
            Step::Choose { slot: OPPONENTS }
        );
        let told = Some(Choice::Attack { targets: 0x01 });
        assert_eq!(fight.decide(&tables, &mut || 29u32), told);

        // Lesson 1: the attack, and the capture catches whatever the draws say (a
        // draw of 99 is no catch of an unhurt creature by the table).
        let mut fight = lesson_fight(&tables, second);
        assert_eq!(learn(&mut fight, &tables, &mut || 99u32), ended::TAUGHT);
        assert_eq!(said(&fight), [17, 18]);
        assert_eq!((fight.caught.len(), fight.cocoons), (1, 0));
        assert_eq!((fight.caught[0].species, fight.caught[0].hp), (12, 2));
        assert_eq!(fight.flags, flag::OPPONENT_FELL | flag::LESSON_ENDED);
        assert_eq!(fight.beaten[0].kind, 26);
    }

    #[test]
    fn a_lesson_refuses_another_choice_and_ends_with_outcome_5() {
        let tables = tables();
        let mut draws = || 0u32;
        let [first, _] = lessons();
        let mut fight = lesson_fight(&tables, first);
        assert_eq!(
            fight.next(&tables, &mut draws),
            Step::Choose { slot: PLAYER }
        );
        // What the menu would not hand over is not judged: the turn is not spent.
        let nobody = Choice::Capture { targets: 0x40 };
        let refused = fight.act(&tables, nobody, false, &mut draws);
        assert_eq!((refused, fight.flags), (Err(Refusal::NoSuchTarget), 0));
        // He defends where he is to attack: the command is not run, the refusal's
        // message is shown and the fight is over.
        fight
            .act(&tables, Choice::Defend, false, &mut draws)
            .expect("taken");
        assert_eq!(fight.flags, flag::REFUSED);
        assert_eq!(
            fight.next(&tables, &mut draws),
            Step::Over {
                outcome: ended::REFUSED
            }
        );
        assert_eq!(said(&fight), [11, 5]);
        assert_eq!(fight.flags, flag::REFUSED | flag::LESSON_ENDED);
        let (him, it) = (&fight.records[PLAYER], &fight.records[OPPONENTS]);
        assert_eq!(him.as_ref().expect("him").status & status::DEFENDING, 0);
        assert_eq!(it.as_ref().expect("it").hp, 30);
        // A step that forbids its kind refuses exactly that one.
        let mut forbidden = lessons()[0].clone();
        forbidden[1].must = false;
        let mut fight = lesson_fight(&tables, forbidden);
        fight.next(&tables, &mut draws);
        let attack = Choice::Attack { targets: 0x10 };
        fight.act(&tables, attack, false, &mut draws).expect("it");
        assert_eq!(fight.flags, flag::REFUSED);
        // A player that fell comes before the lesson's own outcomes.
        let mut fight = lesson_fight(&tables, vec![]);
        fight.flags = flag::PLAYER_FELL;
        assert_eq!(
            fight.next(&tables, &mut draws),
            Step::Over {
                outcome: ended::LOST
            }
        );
    }

    #[test]
    fn a_capture_spends_a_cocoon_and_stores_the_creature_as_it_stands() {
        let tables = tables();
        let mut draws = || 0u32;
        let mut two = party();
        two.cocoons = 2;
        let mut fight =
            Fight::new(&tables, &block(&[12, 12]), two, 1, NO_DEVA, &mut draws).expect("a fight");
        let catch = Choice::Capture { targets: 0x10 };
        assert_eq!(
            fight.next(&tables, &mut draws),
            Step::Choose { slot: PLAYER }
        );
        // Not at two at once, and not at one that is not there.
        let both = fight.act(
            &tables,
            Choice::Capture { targets: 0x30 },
            false,
            &mut draws,
        );
        let nobody = fight.act(
            &tables,
            Choice::Capture { targets: 0x40 },
            false,
            &mut draws,
        );
        let no_target = Err(Refusal::NoSuchTarget);
        assert_eq!((both, nobody), (no_target, no_target));
        // An unhurt sleeper of his own level: the score is 8, the tier 3, and a draw
        // of 14 is grade 1. It breaks free, awake, and the cocoon is gone.
        fight.records[OPPONENTS].as_mut().expect("it").status = status::SLEEP;
        fight
            .act(&tables, catch, false, &mut || 14u32)
            .expect("taken");
        let it = fight.records[OPPONENTS].as_ref().expect("it");
        assert_eq!((fight.cocoons, it.status, fight.flags), (1, 0, 0));
        assert!(fight.caught.is_empty() && fight.stands(OPPONENTS));
        // Asleep again, a draw of 13 is grade 0: caught.
        fight.gauges[PLAYER] = 1000;
        assert_eq!(
            fight.next(&tables, &mut draws),
            Step::Choose { slot: PLAYER }
        );
        fight.records[OPPONENTS].as_mut().expect("it").status = status::SLEEP;
        fight
            .act(&tables, catch, false, &mut || 13u32)
            .expect("taken");
        assert_eq!((fight.cocoons, fight.room), (0, 11));
        assert_eq!(
            (fight.flags, fight.opponents()),
            (flag::OPPONENT_FELL, 0x20)
        );
        // The stored record still sleeps; the one of the fight was woken behind it.
        assert_eq!(fight.caught.len(), 1);
        assert_eq!((fight.caught[0].species, fight.caught[0].status), (12, 2));
        assert_eq!(fight.records[OPPONENTS].as_ref().expect("it").status, 0);
        let caught = Beaten {
            species: 12,
            by: PLAYER,
            level: 1,
            kind: 5,
        };
        assert_eq!(fight.beaten[0], caught);
        assert!(fight.events.contains(&Event::Caught { slot: OPPONENTS }));
        // No cocoon is left: the command is closed.
        fight.gauges[PLAYER] = 1000;
        fight.next(&tables, &mut draws);
        let closed = fight.act(
            &tables,
            Choice::Capture { targets: 0x20 },
            false,
            &mut draws,
        );
        assert_eq!(closed, Err(Refusal::Closed));
        // The last opponent on the field is caught with kind 26, and the fight is won.
        (fight.cocoons, fight.room) = (1, 1);
        let last = Choice::Capture { targets: 0x20 };
        fight
            .act(&tables, last, false, &mut || 3u32)
            .expect("taken");
        assert_eq!((fight.beaten[1].kind, fight.room), (26, 0));
        // The picture gets the same kind, with the reaction "caught" of the target; both
        // carry the grade.
        let run = fight.runs.last().expect("the run of the capture");
        assert_eq!((run.kind, run.actors, run.targets), (26, 0x01, 0x20));
        let caught = Shown {
            slot: OPPONENTS + 1,
            kind: 5,
            parameter: run.parameter,
        };
        assert_eq!(run.reactions, [caught]);
        assert!(matches!(run.parameter, 0 | 2), "a grade that catches");
        assert_eq!(
            fight.next(&tables, &mut draws),
            Step::Over {
                outcome: ended::WON
            }
        );
        // Without a free record the capture is refused.
        let mut full = party();
        full.room = 0;
        let mut fight =
            Fight::new(&tables, &block(&[12]), full, 1, NO_DEVA, &mut draws).expect("a fight");
        fight.next(&tables, &mut draws);
        let refused = fight.act(&tables, catch, false, &mut draws);
        assert_eq!(refused, Err(Refusal::NoRoom));
    }

    #[test]
    fn an_item_is_tested_as_the_menu_tests_it_and_costs_a_piece() {
        let tables = tables();
        let mut draws = || 0u32;
        let mut both = party();
        both.minions = vec![player()];
        let mut fight =
            Fight::new(&tables, &block(&[12]), both, 1, NO_DEVA, &mut draws).expect("a fight");
        fight.next(&tables, &mut draws);
        let herb = |targets: u8| Choice::Item { item: 0, targets };
        let used =
            |fight: &mut Fight, choice: Choice| fight.act(&tables, choice, false, &mut || 0u32);
        // Full HP: it would do nothing. Not for a minion, not at an opponent, and no
        // piece of another item.
        assert_eq!(used(&mut fight, herb(0x01)), Err(Refusal::NoUse));
        assert_eq!(used(&mut fight, herb(0x02)), Err(Refusal::NotForThat));
        assert_eq!(used(&mut fight, herb(0x10)), Err(Refusal::NoSuchTarget));
        let other = Choice::Item {
            item: 1,
            targets: 1,
        };
        assert_eq!(used(&mut fight, other), Err(Refusal::NoPiece));
        // With 61 of 100 HP the 40 of the herb are cut to 39.
        fight.records[PLAYER].as_mut().expect("him").hp = 61;
        used(&mut fight, herb(0x01)).expect("taken");
        assert_eq!(fight.records[PLAYER].as_ref().expect("him").hp, 100);
        assert_eq!(fight.items, [(0, 1)]);
        let healed = |event: &Event| match event {
            Event::Resolved { target, result, .. } => Some((*target, result.hp)),
            _ => None,
        };
        assert_eq!(fight.events.iter().find_map(healed), Some((PLAYER, -39)));
        // The turn was spent, and the last piece takes the item off the list.
        fight.gauges[PLAYER] = 1000;
        assert_eq!(
            fight.next(&tables, &mut draws),
            Step::Choose { slot: PLAYER }
        );
        fight.records[PLAYER].as_mut().expect("him").hp = 10;
        used(&mut fight, herb(0x01)).expect("taken");
        assert_eq!(fight.records[PLAYER].as_ref().expect("him").hp, 50);
        assert!(fight.items.is_empty());
        // An item that cannot be used in a fight.
        let mut tables = tables.clone();
        tables.items[0].flags = 0x49;
        fight.items = vec![(0, 1)];
        fight.gauges[PLAYER] = 1000;
        fight.next(&tables, &mut draws);
        let refused = fight.act(&tables, herb(0x01), false, &mut draws);
        assert_eq!(refused, Err(Refusal::NotInAFight));
    }

    /// The menu of a fight of the rules: what its maker notes, and an item from the
    /// list to the command the rules take.
    #[test]
    fn the_menu_of_a_fight_hands_the_rules_an_item() {
        use super::super::menu::{Lists, Menu, Pass, command, level, sound as heard, text};
        use crate::pad::{self, Pad, Repeat};
        let tables = tables();
        let mut draws = || 0u32;
        let mut fight =
            Fight::new(&tables, &block(&[12]), party(), 1, NO_DEVA, &mut draws).expect("a fight");
        assert_eq!(
            fight.next(&tables, &mut draws),
            Step::Choose { slot: PLAYER }
        );
        // The herb lies at place 2 of his list, behind a free place and a thing the
        // tables do not have.
        let places = [None, Some(7), Some(0)];
        let go = |fight: &Fight, menu: &mut Menu, buttons: u16| -> Pass {
            let lists = Lists {
                fight,
                tables: &tables,
                places: &places,
            };
            let mut pad = Pad::default();
            pad.read(buttons, Repeat::of(2), 0);
            menu.pass(&pad, &lists)
        };
        let lists = Lists {
            fight: &fight,
            tables: &tables,
            places: &places,
        };
        let offer = lists.offer(0);
        assert_eq!(
            (offer.fighter, offer.carried, offer.opponents),
            (0, 0, 0x10)
        );
        assert_eq!(
            (offer.boss, offer.no_cocoon, offer.no_retreat),
            (false, false, false)
        );
        assert_eq!((offer.places, offer.spells), (3, 0));
        let mut menu = Menu::new(offer);
        // Nobody is carried: "summon" is closed. Down twice is "item".
        assert_eq!(menu.closed(command::SUMMON), Some(text::NO_CREATURE));
        go(&fight, &mut menu, pad::DOWN);
        go(&fight, &mut menu, pad::DOWN);
        assert_eq!(menu.command(), command::ITEM);
        go(&fight, &mut menu, pad::CROSS);
        assert_eq!(menu.level(), level::ITEMS);
        // A free place and a thing without a row refuse without a message.
        let free = go(&fight, &mut menu, pad::CROSS);
        assert_eq!((free.sounds, free.message), (vec![heard::REFUSED], None));
        go(&fight, &mut menu, pad::DOWN);
        assert_eq!(go(&fight, &mut menu, pad::CROSS).sounds, [heard::REFUSED]);
        go(&fight, &mut menu, pad::DOWN);
        go(&fight, &mut menu, pad::CROSS);
        assert_eq!(menu.level(), level::TARGET);
        // The herb is for him, and with all his HP he has no need of it.
        let full = go(&fight, &mut menu, pad::CROSS);
        assert_eq!(full.sounds, [heard::REFUSED]);
        assert_eq!(go(&fight, &mut menu, 0).message, Some(text::NO_NEED));
        // A pass on the target marks the own side for the HUD and previews for him:
        // nothing, where the test of the target refuses.
        let drew = menu.drew();
        assert_eq!(
            (drew.marks, drew.subject, drew.previews),
            ((0, true), PLAYER, true)
        );
        let preview = |fight: &Fight, menu: &Menu| {
            let lists = Lists {
                fight,
                tables: &tables,
                places: &places,
            };
            let mut count = 0;
            let mut draws = || {
                count += 1;
                0u32
            };
            (lists.preview(menu, false, &mut draws), count)
        };
        assert_eq!(preview(&fight, &menu).0, (0, 0));
        fight.records[PLAYER].as_mut().expect("him").hp = 61;
        // A button takes the message away, and the next confirm is taken. With 61 of
        // 100 HP the preview is the 39 he lacks, of the herb's 40, and the resolver has
        // made its draws for it.
        go(&fight, &mut menu, pad::TRIANGLE);
        let (would, drawn) = preview(&fight, &menu);
        assert_eq!(would, (39, 0));
        assert!(drawn > 0);
        let chosen = go(&fight, &mut menu, pad::CROSS).chosen.expect("a command");
        // The pass that hands the command over clears the marks and the preview.
        assert_eq!(
            (menu.drew().marks, menu.drew().previews),
            ((0, false), false)
        );
        assert_eq!(preview(&fight, &menu), ((0, 0), 0));
        let choice = chosen.choice().expect("a choice");
        assert_eq!(
            choice,
            Choice::Item {
                item: 0,
                targets: 1
            }
        );
        assert_eq!(fight.act(&tables, choice, false, &mut draws), Ok(()));
        assert_eq!(fight.items, [(0, 1)]);
        // The last piece frees the place: the list of the next menu ends in front of it.
        let listed = |fight: &Fight| {
            let lists = Lists {
                fight,
                tables: &tables,
                places: &places,
            };
            (
                lists.row(2).count,
                lists.last_place(),
                lists.offer(0).places,
            )
        };
        assert_eq!(listed(&fight), (1, 2, 3));
        fight.items.clear();
        assert_eq!(listed(&fight), (0, -1, 0));
    }

    #[test]
    fn a_master_sends_his_creature_in_and_comes_back_when_it_falls() {
        let tables = tables();
        let mut draws = || 50u32;
        let pair = Block {
            mode: 0,
            on_field: 1,
            specs: vec![
                Spec {
                    species: 41,
                    experience: 0,
                    ai: 2,
                },
                Spec {
                    species: 42,
                    experience: 0,
                    ai: 2,
                },
            ],
            ..block(&[])
        };
        let mut fight = Fight::new(&tables, &pair, party(), 0, NO_DEVA, &mut draws).expect("it");
        assert_eq!((fight.opponents(), fight.beaten[1].species), (0x10, -1));
        // The master's first turn: his creature comes in with a gauge of 800.
        let first = play(&mut fight, &tables, &mut draws, 1);
        assert_eq!(first, [(OPPONENTS, Choice::Swap)]);
        assert_eq!((fight.opponents(), fight.pair), (0x20, true));
        assert_eq!(fight.gauges[OPPONENTS + 1], 800);
        // The picture gets the two runners of the handler: his move, then the
        // creature's, each of kind 21 with no target.
        let shown = |fight: &Fight, last: usize| -> Vec<(u32, u8, u8)> {
            let runs = &fight.runs[fight.runs.len() - last..];
            (runs.iter().map(|run| (run.kind, run.actors, run.targets))).collect()
        };
        assert_eq!(shown(&fight, 2), [(21, 0x10, 0), (21, 0x20, 0)]);
        assert_eq!(
            fight.runs.last().map(|run| run.before),
            Some(took::TO_THE_SECOND)
        );
        // The creature goes round: attack, body attack 0, spell 2, attack.
        fight.records[PLAYER].as_mut().expect("him").max_hp = 30000;
        fight.records[PLAYER].as_mut().expect("him").hp = 30000;
        fight.records[OPPONENTS + 1].as_mut().expect("it").max_hp = 30000;
        fight.records[OPPONENTS + 1].as_mut().expect("it").hp = 30000;
        let round = play(&mut fight, &tables, &mut draws, 4);
        let kinds: Vec<Choice> = round.iter().map(|(_, choice)| *choice).collect();
        let targets = 0x01;
        let spell = Choice::Spell { spell: 2, targets };
        let body = Choice::Body { slot: 0, targets };
        let attack = Choice::Attack { targets };
        assert_eq!(kinds, [attack, body, spell, attack]);
        assert!(round.iter().all(|(slot, _)| *slot == OPPONENTS + 1));
        // It falls: the fight is not over, the master comes back with a gauge of 800.
        fight.records[OPPONENTS + 1].as_mut().expect("it").hp = 1;
        fight.gauges[PLAYER] = 1000;
        fight.next(&tables, &mut draws);
        let hit = Choice::Attack { targets: 0x20 };
        fight.act(&tables, hit, false, &mut draws).expect("taken");
        let fell = flag::OPPONENT_FELL | flag::MASTER_WAITS;
        assert_eq!((fight.flags, fight.opponents()), (fell, 0));
        fight.gauges[PLAYER] = 1000;
        assert_eq!(
            fight.next(&tables, &mut draws),
            Step::Choose { slot: PLAYER }
        );
        assert_eq!(
            (fight.flags, fight.opponents()),
            (flag::OPPONENT_FELL, 0x10)
        );
        assert_eq!((fight.pair, fight.gauges[OPPONENTS]), (false, 800));
        assert_eq!(fight.beaten[1].species, 42);
        // One runner for it: the master's move of kind 22.
        assert_eq!(shown(&fight, 1), [(22, 0x10, 0)]);
        // On his later turns the master attacks, and when he falls the fight is won.
        fight
            .act(&tables, Choice::Defend, false, &mut draws)
            .expect("taken");
        let later = play(&mut fight, &tables, &mut draws, 1);
        assert_eq!(later, [(OPPONENTS, attack)]);
        fight.records[OPPONENTS].as_mut().expect("him").hp = 1;
        play(&mut fight, &tables, &mut draws, 9);
        assert_eq!(fight.outcome(), Some(ended::WON));
    }

    #[test]
    fn the_opponents_decide_by_type_and_species() {
        let mut tables = tables();
        let mut draws = || 0u32;
        let mut fight =
            Fight::new(&tables, &block(&[12, 15]), party(), 2, NO_DEVA, &mut draws).expect("it");
        let targets = 0x01;
        let attack = Choice::Attack { targets };
        // Nobody chooses, or the player does: no decision.
        assert_eq!(fight.decide(&tables, &mut draws), None);
        fight.choosing = Some(PLAYER);
        assert_eq!(fight.decide(&tables, &mut draws), None);
        // Species 12: the spell on a draw below 30 when it knows it and can pay.
        fight.choosing = Some(OPPONENTS);
        assert_eq!(fight.decide(&tables, &mut || 29u32), Some(attack));
        let it = fight.records[OPPONENTS].as_mut().expect("it");
        (it.spells, it.mp) = (1, 3);
        let agni = Choice::Spell { spell: 0, targets };
        assert_eq!(fight.decide(&tables, &mut || 29u32), Some(agni));
        assert_eq!(fight.decide(&tables, &mut || 30u32), Some(attack));
        fight.records[OPPONENTS].as_mut().expect("it").mp = 2;
        assert_eq!(fight.decide(&tables, &mut || 29u32), Some(attack));
        // Species 15: unhurt it attacks; below a third the body attack it knows and
        // can pay, else it defends on an odd draw.
        fight.choosing = Some(OPPONENTS + 1);
        assert_eq!(fight.decide(&tables, &mut || 1u32), Some(attack));
        let it = fight.records[OPPONENTS + 1].as_mut().expect("it");
        (it.hp, it.max_hp) = (9, 30);
        assert_eq!(fight.decide(&tables, &mut || 1u32), Some(Choice::Defend));
        assert_eq!(fight.decide(&tables, &mut || 2u32), Some(attack));
        let it = fight.records[OPPONENTS + 1].as_mut().expect("it");
        it.moves[3] = Move {
            known: true,
            attack: 4,
            grade: 0,
            flag: 0,
        };
        it.mp = 9;
        assert_eq!(fight.decide(&tables, &mut || 1u32), Some(Choice::Defend));
        fight.records[OPPONENTS + 1].as_mut().expect("it").mp = 10;
        let body = |slot: usize| Choice::Body { slot, targets };
        assert_eq!(fight.decide(&tables, &mut || 1u32), Some(body(3)));
        // An element variant with the common routine: the deva call first, then a
        // status the target lacks, and with nothing known the plain attack.
        fight.records[OPPONENTS].as_mut().expect("it").species = 106;
        fight.choosing = Some(OPPONENTS);
        tables.spells = vec![tables.spells[0]; 25];
        let knows = |fight: &mut Fight, spells: u32| {
            let it = fight.records[OPPONENTS].as_mut().expect("it");
            (it.spells, it.mp) = (spells, 3);
        };
        let spell = |spell: usize| Some(Choice::Spell { spell, targets });
        knows(&mut fight, 1 << 21 | 1 << 24 | 1 << 18);
        assert_eq!(fight.decide(&tables, &mut draws), spell(21));
        fight.deva = 2;
        assert_eq!(fight.decide(&tables, &mut draws), spell(24));
        fight.deva = 0;
        assert_eq!(fight.decide(&tables, &mut draws), spell(18));
        assert_eq!(fight.decide(&tables, &mut || 7u32), Some(attack));
        fight.records[PLAYER].as_mut().expect("him").status = status::SLEEP;
        knows(&mut fight, 1 << 5);
        let it = fight.records[OPPONENTS].as_mut().expect("it");
        it.stats[stat::MAGIC_DEFENCE] = it.stats[stat::DEFENCE];
        assert_eq!(fight.decide(&tables, &mut draws), spell(5));
        fight.records[OPPONENTS].as_mut().expect("it").stats[stat::MAGIC_DEFENCE] = 0;
        assert_eq!(fight.decide(&tables, &mut draws), Some(attack));
        // The fixed types: the Masked Boy goes round six commands; types that only
        // defend; a type that is not read.
        fight.ai[0] = 10;
        let six: Vec<_> = (0..7).map(|_| fight.decide(&tables, &mut draws)).collect();
        let round = [body(2), attack, body(3), attack, body(1), attack, body(2)];
        assert_eq!(six, round.map(Some));
        // Type 4 goes round three body attacks on the same counter (it stands at 7).
        fight.ai[0] = 4;
        let three: Vec<_> = (0..4).map(|_| fight.decide(&tables, &mut draws)).collect();
        assert_eq!(three, [body(2), body(1), body(3), body(2)].map(Some));
        fight.ai[0] = 5;
        assert_eq!(fight.decide(&tables, &mut draws), Some(Choice::Defend));
        // A species the table of routines has no word for.
        fight.ai[0] = 0;
        fight.records[OPPONENTS].as_mut().expect("it").species = 53;
        assert_eq!(fight.decide(&tables, &mut draws), None);
        assert_eq!(
            fight.opponent_acts(&tables, false, &mut draws),
            Err(Refusal::NotBuilt)
        );
    }

    #[test]
    fn the_creature_of_type_14_changes_its_form() {
        let tables = tables();
        let mut fight = Fight::new(&tables, &block(&[12, 15]), party(), 2, NO_DEVA, &mut || {
            0u32
        })
        .expect("it");
        fight.ai = [14, 14, 0];
        let targets = 0x01;
        let strikes = |slot: usize| Some(Choice::Body { slot, targets });
        let spell = |spell: usize| Some(Choice::Spell { spell, targets });
        let decides = |fight: &mut Fight, draws: &[u32]| {
            let mut left = draws.iter().copied();
            let choice = fight.decide(&tables, &mut || left.next().expect("a draw"));
            assert_eq!(left.next(), None, "a draw is left over");
            choice
        };
        let form = |fight: &Fight| {
            let it = fight.records[OPPONENTS + 1].as_ref().expect("it");
            let taken = [it.modifiers[0], it.modifiers[1], it.modifiers[2]];
            (it.element_bit, it.moves[2].attack, taken, it.modifiers[3])
        };
        // The master: his creature in, and the last draw counts 1 or 2 on.
        fight.choosing = Some(OPPONENTS);
        assert_eq!(decides(&mut fight, &[1]), Some(Choice::Swap));
        assert_eq!(fight.counter, 1);
        // No deva: the form of earth, which the wind beats, and the call of its deva.
        fight.choosing = Some(OPPONENTS + 1);
        fight.records[OPPONENTS + 1]
            .as_mut()
            .expect("it")
            .element_bit = 0;
        assert_eq!(decides(&mut fight, &[2]), strikes(2));
        assert_eq!(form(&fight), (4, 36, [12, 24, 12], 12));
        assert_eq!(fight.counter, 2);
        // Under its own deva it strikes on two draws of three, else its spell.
        fight.deva = EARTH as i16;
        assert_eq!(decides(&mut fight, &[1, 0]), strikes(1));
        assert_eq!(
            fight.records[OPPONENTS + 1].as_ref().expect("it").moves[1].attack,
            2
        );
        assert_eq!(fight.counter, 3);
        // Counted to 3: the form of wind, and the counter starts anew.
        assert_eq!(decides(&mut fight, &[0]), strikes(2));
        assert_eq!(form(&fight), (2, 35, [24, 12, 12], 12));
        assert_eq!(fight.counter, 1);
        fight.deva = WIND as i16;
        assert_eq!(decides(&mut fight, &[3, 0]), strikes(1));
        assert_eq!(
            fight.records[OPPONENTS + 1].as_ref().expect("it").moves[1].attack,
            5
        );
        assert_eq!(decides(&mut fight, &[4, 3]), spell(28));
        assert_eq!(fight.counter, 4);
        // The form of fire, which the water beats.
        assert_eq!(decides(&mut fight, &[0]), strikes(2));
        assert_eq!(form(&fight), (1, 34, [12, 12, 12], 24));
        fight.deva = FIRE as i16;
        assert_eq!(decides(&mut fight, &[1, 0]), strikes(1));
        assert_eq!(
            fight.records[OPPONENTS + 1].as_ref().expect("it").moves[1].attack,
            4
        );
        assert_eq!(decides(&mut fight, &[0, 0]), spell(25));
        // Another deva than its own: it changes at once, whatever the counter.
        fight.counter = 0;
        fight.deva = WIND as i16;
        assert_eq!(decides(&mut fight, &[0]), strikes(2));
        assert_eq!(form(&fight).0, 1);
        fight.deva = FIRE as i16;
        fight.counter = 3;
        assert_eq!(decides(&mut fight, &[0]), strikes(2));
        assert_eq!(form(&fight), (8, 37, [12, 12, 24], 12));
        // The form of water: a spell on itself when the counter stands at 1, else slot
        // 0, with no draw but the last; it waits to 4.
        fight.deva = WATER as i16;
        let on_itself = Choice::Spell {
            spell: 29,
            targets: 1 << (OPPONENTS + 1),
        };
        assert_eq!(decides(&mut fight, &[2]), Some(on_itself));
        assert_eq!(fight.counter, 3);
        assert_eq!(decides(&mut fight, &[0]), strikes(0));
        assert_eq!(decides(&mut fight, &[0]), strikes(2));
        assert_eq!(form(&fight).0, 4);
        // The master's later turns: no element, every modifier of the four at 48, and
        // slot 1.
        fight.choosing = Some(OPPONENTS);
        assert_eq!(decides(&mut fight, &[3]), strikes(1));
        let he = fight.records[OPPONENTS].as_ref().expect("him");
        assert_eq!(
            (he.element_bit, he.modifiers[..4].to_vec()),
            (0, vec![48; 4])
        );
        assert_eq!(fight.counter, 3);
    }

    #[test]
    fn the_other_species_decide_with_their_draws() {
        let mut tables = tables();
        tables.spells = vec![tables.spells[0]; 25];
        let mut fight = Fight::new(&tables, &block(&[12, 15]), party(), 2, NO_DEVA, &mut || {
            0u32
        })
        .expect("it");
        fight.choosing = Some(OPPONENTS);
        let targets = 0x01;
        let attack = Some(Choice::Attack { targets });
        let spell = |spell: usize| Some(Choice::Spell { spell, targets });
        let body = Some(Choice::Body { slot: 0, targets });
        let decides = |fight: &mut Fight, draws: &[u32]| {
            let mut left = draws.iter().copied();
            let choice = fight.decide(&tables, &mut || left.next().expect("a draw"));
            assert_eq!(left.next(), None, "a draw is left over");
            choice
        };
        // The opponent as a species that knows the spells of `spells` and, with
        // `strikes`, a body attack it can pay.
        let becomes = |fight: &mut Fight, species: u32, spells: u32, strikes: bool| {
            let it = fight.records[OPPONENTS].as_mut().expect("it");
            (it.species, it.spells, it.mp) = (species, spells, 10);
            it.moves[0].known = strikes;
            (it.hp, it.max_hp) = (30, 30);
        };
        let him = |fight: &mut Fight, status: u32, mp: i16| {
            let him = fight.records[PLAYER].as_mut().expect("him");
            (him.status, him.mp) = (status, mp);
        };
        fight.records[OPPONENTS].as_mut().expect("it").moves[0] = Move {
            known: true,
            attack: 4,
            grade: 0,
            flag: 0,
        };
        him(&mut fight, 0, 5);

        // One draw in every call: species 9 and 7.
        becomes(&mut fight, 9, 1 << 9, false);
        assert_eq!(decides(&mut fight, &[49]), spell(9));
        assert_eq!(decides(&mut fight, &[50]), attack);
        becomes(&mut fight, 7, 0, true);
        assert_eq!(decides(&mut fight, &[6]), body);
        assert_eq!(decides(&mut fight, &[7]), attack);
        // The deva call of species 2 under no deva or its own, else the body attack.
        becomes(&mut fight, 2, 1 << 21, true);
        assert_eq!(decides(&mut fight, &[]), spell(21));
        fight.deva = 0;
        assert_eq!(decides(&mut fight, &[]), body);
        fight.deva = 1;
        assert_eq!(decides(&mut fight, &[]), spell(21));
        fight.deva = NO_DEVA;
        // Species 3 looks at the target's two defences.
        becomes(&mut fight, 3, 1 << 7, true);
        let foe = fight.records[PLAYER].as_mut().expect("him");
        (foe.stats[stat::MAGIC_DEFENCE], foe.stats[stat::DEFENCE]) = (4, 5);
        assert_eq!(decides(&mut fight, &[]), spell(7));
        fight.records[PLAYER].as_mut().expect("him").stats[stat::MAGIC_DEFENCE] = 5;
        assert_eq!(decides(&mut fight, &[]), body);
        // Species 10 counts the tries it could not pay and then only attacks.
        becomes(&mut fight, 10, 0, false);
        assert_eq!(decides(&mut fight, &[39]), attack);
        assert_eq!(fight.counter, 2);
        becomes(&mut fight, 10, 1 << 18, true);
        assert_eq!(decides(&mut fight, &[39]), attack);
        fight.counter = 0;
        assert_eq!(decides(&mut fight, &[39]), spell(18));
        assert_eq!(decides(&mut fight, &[40]), attack);
        him(&mut fight, status::SLEEP, 5);
        assert_eq!(decides(&mut fight, &[0]), attack);
        assert_eq!(fight.counter, 0);
        him(&mut fight, 0, 5);
        // Species 11 runs away without its spell.
        becomes(&mut fight, 11, 0, true);
        assert_eq!(decides(&mut fight, &[]), Some(Choice::Flee));
        becomes(&mut fight, 11, 1 << 8, true);
        assert_eq!(decides(&mut fight, &[1]), spell(8));
        assert_eq!(decides(&mut fight, &[0]), body);
        him(&mut fight, 0, 0);
        assert_eq!(decides(&mut fight, &[0]), attack);
        // Species 13 strikes at half its HP, not above.
        becomes(&mut fight, 13, 0, true);
        assert_eq!(decides(&mut fight, &[]), attack);
        fight.records[OPPONENTS].as_mut().expect("it").hp = 15;
        assert_eq!(decides(&mut fight, &[]), body);
        // Species 16: the second draw only when the spell was not cast.
        becomes(&mut fight, 16, 1 << 17, true);
        assert_eq!(decides(&mut fight, &[6]), spell(17));
        assert_eq!(decides(&mut fight, &[7, 1]), body);
        assert_eq!(decides(&mut fight, &[7, 0]), attack);
        him(&mut fight, status::POISON, 0);
        assert_eq!(decides(&mut fight, &[1]), body);
        him(&mut fight, 0, 0);
        // Species 23 never attacks, and draws a third time only when it is weak.
        becomes(&mut fight, 23, 0, true);
        assert_eq!(decides(&mut fight, &[9]), Some(Choice::Defend));
        fight.records[OPPONENTS].as_mut().expect("it").hp = 10;
        assert_eq!(decides(&mut fight, &[0, 1]), Some(Choice::Flee));
        assert_eq!(decides(&mut fight, &[6, 0]), Some(Choice::Defend));
        // Species 24: stone or defending.
        becomes(&mut fight, 24, 1 << 19, true);
        assert_eq!(decides(&mut fight, &[]), spell(19));
        him(&mut fight, 0x08, 0);
        assert_eq!(decides(&mut fight, &[]), Some(Choice::Defend));
        him(&mut fight, 0, 0);
        // Species 27 writes nothing when it cannot pay on an odd draw.
        becomes(&mut fight, 27, 0, true);
        assert_eq!(decides(&mut fight, &[0]), attack);
        assert_eq!(decides(&mut fight, &[1]), body);
        becomes(&mut fight, 27, 0, false);
        let left = |kind: u8| Some(Choice::Left { kind });
        assert_eq!(decides(&mut fight, &[1]), left(reaction::NONE));
        // It plays what its turn began with and loses the turn: nothing is resolved.
        fight.began = reaction::WAKES;
        assert_eq!(decides(&mut fight, &[1]), left(reaction::WAKES));
        let mut played = fight.clone();
        let (runs, before) = (played.runs.len(), played.records.clone());
        let acted = played.act(&tables, Choice::Left { kind: 15 }, false, &mut || 0u32);
        assert_eq!((acted, played.choosing), (Ok(()), None));
        assert_eq!(played.records, before);
        let run = played.runs.last().expect("its run");
        assert_eq!(played.runs.len(), runs + 1);
        assert_eq!(
            (run.kind, run.actors, run.targets, run.counted.len()),
            (15, 0x10, 0, 0)
        );
        fight.began = reaction::NONE;
        // Only an opponent leaves its command standing.
        played.choosing = Some(PLAYER);
        let refused = played.act(&tables, Choice::Left { kind: 20 }, false, &mut || 0u32);
        assert_eq!(refused, Err(Refusal::NotItsCommand));
        // Species 189 to 192 draw once more than the common routine does.
        becomes(&mut fight, 106, 0, false);
        let it = fight.records[OPPONENTS].as_mut().expect("it");
        (it.stats[stat::MAGIC_DEFENCE], it.stats[stat::DEFENCE]) = (0, 1);
        assert_eq!(decides(&mut fight, &[9, 9, 9, 0, 9]), attack);
        fight.records[OPPONENTS].as_mut().expect("it").species = 190;
        assert_eq!(decides(&mut fight, &[1, 9, 9, 9, 0, 9]), attack);
        // Species 209 and 210 are the common routine alone.
        fight.records[OPPONENTS].as_mut().expect("it").species = 209;
        assert_eq!(decides(&mut fight, &[9, 9, 9, 0, 9]), attack);

        // Species 201 to 208 order what they neither know nor can pay.
        let strikes = |slot: usize| Some(Choice::Body { slot, targets });
        let is = |fight: &mut Fight, species: u32| {
            let it = fight.records[OPPONENTS].as_mut().expect("it");
            (it.species, it.spells, it.mp) = (species, 0, 0);
        };
        let hurt = |fight: &mut Fight, hp: i16| {
            let him = fight.records[PLAYER].as_mut().expect("him");
            (him.hp, him.max_hp) = (hp, 100);
        };
        is(&mut fight, 201);
        hurt(&mut fight, 9);
        assert_eq!(decides(&mut fight, &[]), strikes(0));
        hurt(&mut fight, 10);
        assert_eq!(decides(&mut fight, &[1]), strikes(1));
        assert_eq!(decides(&mut fight, &[0]), attack);
        is(&mut fight, 202);
        assert_eq!(decides(&mut fight, &[1]), spell(8));
        assert_eq!(decides(&mut fight, &[0]), spell(6));
        is(&mut fight, 203);
        assert_eq!(decides(&mut fight, &[1]), strikes(2));
        assert_eq!(decides(&mut fight, &[0]), attack);
        is(&mut fight, 204);
        assert_eq!(decides(&mut fight, &[]), spell(20));
        is(&mut fight, 205);
        assert_eq!(decides(&mut fight, &[3]), attack);
        assert_eq!(decides(&mut fight, &[4]), strikes(3));
        is(&mut fight, 206);
        assert_eq!(decides(&mut fight, &[1]), strikes(1));
        assert_eq!(decides(&mut fight, &[0]), strikes(2));
        is(&mut fight, 207);
        assert_eq!(decides(&mut fight, &[1]), Some(Choice::Defend));
        assert_eq!(decides(&mut fight, &[0]), attack);
        let it = fight.records[OPPONENTS].as_mut().expect("it");
        (it.spells, it.mp, it.hp) = (1 << 13, 3, 7);
        let heals = Choice::Spell {
            spell: 13,
            targets: 1 << OPPONENTS,
        };
        assert_eq!(decides(&mut fight, &[]), Some(heals));
        // Species 208: a draw for each status the target lacks, until one is below 7.
        is(&mut fight, 208);
        assert_eq!(decides(&mut fight, &[6]), strikes(0));
        assert_eq!(decides(&mut fight, &[7, 6]), strikes(1));
        assert_eq!(decides(&mut fight, &[7, 7, 6]), strikes(2));
        assert_eq!(decides(&mut fight, &[7, 7, 7]), attack);
        him(&mut fight, status::SLEEP | 0x08, 0);
        assert_eq!(decides(&mut fight, &[6]), strikes(2));

        // The types 15 to 17: the master sends his creature in once and attacks
        // afterwards; the creature has its own tests.
        him(&mut fight, 0, 0);
        fight.ai = [15, 15, 0];
        assert_eq!(decides(&mut fight, &[]), Some(Choice::Swap));
        assert_eq!(decides(&mut fight, &[]), attack);
        fight.choosing = Some(OPPONENTS + 1);
        let creature = |fight: &mut Fight, hp: i16, spells: u32| {
            let it = fight.records[OPPONENTS + 1].as_mut().expect("it");
            (it.hp, it.max_hp, it.spells, it.mp) = (hp, 30, spells, 10);
            it.moves[0] = Move {
                known: true,
                attack: 4,
                grade: 0,
                flag: 0,
            };
        };
        creature(&mut fight, 30, 0);
        assert_eq!(decides(&mut fight, &[1]), body);
        assert_eq!(decides(&mut fight, &[0]), attack);
        fight.records[PLAYER].as_mut().expect("him").element_bit = 2;
        assert_eq!(decides(&mut fight, &[0]), body);
        fight.records[PLAYER].as_mut().expect("him").element_bit = 0;
        creature(&mut fight, 9, 0);
        assert_eq!(decides(&mut fight, &[0]), body);
        fight.ai[1] = 16;
        creature(&mut fight, 7, 1 << 9);
        assert_eq!(decides(&mut fight, &[]), spell(9));
        creature(&mut fight, 6, 1 << 9);
        assert_eq!(decides(&mut fight, &[]), body);
        fight.ai[1] = 17;
        creature(&mut fight, 14, 1 << 6);
        assert_eq!(decides(&mut fight, &[]), body);
        creature(&mut fight, 15, 1 << 6);
        assert_eq!(decides(&mut fight, &[1]), spell(6));
        assert_eq!(decides(&mut fight, &[0]), attack);

        // The types 11 to 13: the creature orders by a draw of three, with no test.
        let on_itself = Some(Choice::Body {
            slot: 0,
            targets: 1 << (OPPONENTS + 1),
        });
        creature(&mut fight, 30, 0);
        fight.ai = [11, 11, 0];
        fight.counter = 0;
        him(&mut fight, 0, 5);
        assert_eq!(decides(&mut fight, &[0]), strikes(1));
        assert_eq!(decides(&mut fight, &[1]), spell(25));
        assert_eq!(decides(&mut fight, &[2]), strikes(3));
        him(&mut fight, 0, 0);
        assert_eq!(decides(&mut fight, &[0]), spell(25));
        assert_eq!(fight.counter, 4);
        // The master of the types 11 and 13 counts as well; the one of type 12 not.
        fight.choosing = Some(OPPONENTS);
        assert_eq!(decides(&mut fight, &[]), attack);
        assert_eq!(fight.counter, 5);
        fight.ai = [12, 12, 0];
        assert_eq!(decides(&mut fight, &[]), attack);
        assert_eq!(fight.counter, 5);
        // Type 12: on itself once the target sleeps, and the counter starts anew.
        fight.choosing = Some(OPPONENTS + 1);
        assert_eq!(decides(&mut fight, &[0]), spell(28));
        assert_eq!(decides(&mut fight, &[1]), strikes(1));
        assert_eq!(decides(&mut fight, &[2]), strikes(3));
        him(&mut fight, status::SLEEP, 0);
        assert_eq!(decides(&mut fight, &[]), on_itself);
        assert_eq!(fight.counter, 0);
        assert_eq!(decides(&mut fight, &[0]), spell(28));
        assert_eq!(fight.counter, 1);
        // Type 13: ready on a draw of 2, and the strike on its next turn with no draw.
        fight.ai = [13, 13, 0];
        assert_eq!(decides(&mut fight, &[0]), spell(27));
        assert_eq!(decides(&mut fight, &[1]), strikes(1));
        assert_eq!(decides(&mut fight, &[2]), on_itself);
        assert_eq!(decides(&mut fight, &[]), strikes(2));
        assert_eq!(decides(&mut fight, &[2]), on_itself);
        assert_eq!(fight.counter, 6);
    }

    #[test]
    fn the_way_out_takes_the_status_and_gives_the_carried_a_quarter_back() {
        let tables = tables();
        let mut draws = || 0u32;
        let mut three = party();
        three.minions = vec![player(), player()];
        let mut fight =
            Fight::new(&tables, &block(&[12]), three, 0, NO_DEVA, &mut draws).expect("a fight");
        let set = |fight: &mut Fight, slot: usize, status: u32, hp: i16, mp: (i16, i16)| {
            let record = fight.records[slot].as_mut().expect("a record");
            (record.status, record.hp) = (status, hp);
            (record.mp, record.max_mp) = mp;
        };
        set(&mut fight, PLAYER, status::POISON | status::DEAD, 0, (0, 0));
        set(
            &mut fight,
            1,
            status::SLEEP | status::DEFENDING,
            80,
            (1, 42),
        );
        set(&mut fight, 2, status::DEAD | status::POISON, 0, (5, 40));
        fight.flags = flag::PLAYER_FELL;
        assert_eq!(
            fight.next(&tables, &mut draws),
            Step::Over {
                outcome: ended::LOST
            }
        );
        fight.way_out();
        let of = |slot: usize| {
            let record = fight.records[slot].as_ref().expect("a record");
            (record.status, record.hp, record.mp)
        };
        // The player that fell stands up with one HP; the living creature gets 25 HP
        // (cut at its highest) and 10 MP; the dead one stays as it is.
        assert_eq!(of(PLAYER), (0, 1, 0));
        assert_eq!(of(1), (0, 100, 11));
        assert_eq!(of(2), (status::DEAD, 0, 5));
    }

    /// A script's single fight against one creature of level 1 (as the first fight of
    /// room 1 is), every draw 50, both sides attacking, played to its end.
    #[test]
    fn a_loss_that_is_shown_shakes_the_picture() {
        let shown = |outcome: u16, hp: i32, mp: i32| Resolved {
            outcome,
            hp,
            mp,
            ..Resolved::default()
        };
        let of = |own: (i32, i32), results: &[(usize, Resolved)]| {
            let decided: Vec<_> = (results.iter())
                .map(|&(slot, result)| (slot, result, reaction::NONE))
                .collect();
            shaken(4, own, &decided)
        };
        // A hit by its HP, an MP hit by its MP, both by either, a critical hit always.
        assert_eq!(of((0, 0), &[(0, shown(outcome::HIT, 5, 0))]), 1);
        assert_eq!(of((0, 0), &[(0, shown(outcome::HIT, 0, 5))]), 0);
        assert_eq!(of((0, 0), &[(0, shown(outcome::HIT, -5, 0))]), 0);
        assert_eq!(of((0, 0), &[(1, shown(outcome::MP_HIT, 0, 5))]), 2);
        assert_eq!(of((0, 0), &[(1, shown(outcome::MP_HIT, 5, 0))]), 0);
        assert_eq!(of((0, 0), &[(0, shown(outcome::HP_AND_MP, 0, 5))]), 1);
        assert_eq!(of((0, 0), &[(0, shown(outcome::CRITICAL, 0, 0))]), 1);
        // Nothing for a miss, a capture or what shows nothing.
        for kind in [outcome::MISS, outcome::CAPTURE, outcome::NOTHING] {
            assert_eq!(of((0, 0), &[(0, shown(kind, 5, 5))]), 0);
        }
        // Every target for itself; the actor by its own changes, a gain of a drain not.
        let both = [
            (0, shown(outcome::HIT, 5, 0)),
            (2, shown(outcome::MISS, 0, 0)),
            (3, shown(outcome::CRITICAL, 9, 0)),
        ];
        assert_eq!(of((0, 0), &both), 0b1001);
        // Of these only the critical hit makes the screen white.
        let decided: Vec<_> = (both.iter())
            .map(|&(slot, result)| (slot, result, reaction::NONE))
            .collect();
        assert_eq!(flashed(&decided), 0b1000);
        assert_eq!(of((-5, 0), &both), 0b1001);
        assert_eq!(of((3, 0), &both), 0b1_1001);
        assert_eq!(of((0, 2), &both), 0b1_1001);
        // A target that is the actor has the kind of its result.
        assert_eq!(of((3, 0), &[(4, shown(outcome::MISS, 0, 0))]), 0);
    }

    #[test]
    fn a_single_fight_is_won_by_attacking() {
        let tables = tables();
        let mut draws = || 50u32;
        let mut fight =
            Fight::new(&tables, &block(&[12]), party(), 0, NO_DEVA, &mut draws).expect("a fight");
        assert_eq!(fight.gauges, [500, 0, 0, 0, 0, -700, -1400]);
        assert_eq!((fight.opponents(), fight.beaten[0].species), (0x10, 12));
        assert_eq!((fight.beaten[1].species, fight.beaten[0].kind), (-1, 7));
        let mut turns = Vec::new();
        let outcome = loop {
            match fight.next(&tables, &mut draws) {
                Step::Over { outcome } => break outcome,
                Step::Choose { slot } => {
                    turns.push(slot);
                    let targets = if slot == PLAYER { 0x10 } else { 0x01 };
                    let attack = Choice::Attack { targets };
                    fight
                        .act(&tables, attack, false, &mut draws)
                        .expect("taken");
                }
            }
        };
        // Steps of 30 and 31 from gauges of 500 and 0: the player first, then in turn.
        assert_eq!(turns, [0, 4, 0, 4, 0, 4, 0]);
        assert_eq!((outcome, fight.outcome()), (ended::WON, Some(ended::WON)));
        assert_eq!(fight.flags, flag::OPPONENT_FELL);
        let beaten = Beaten {
            species: 12,
            by: PLAYER,
            level: 1,
            kind: 0,
        };
        assert_eq!((fight.beaten[0], fight.opponents()), (beaten, 0));
        let opponent = fight.records[OPPONENTS].as_ref().expect("its record");
        assert!(opponent.is_dead() && opponent.hp == 0 && opponent.experience == 0);
        // What the events say is what the records hold.
        let hits = |on: usize| -> Vec<i32> {
            let hit = |event: &Event| match event {
                Event::Resolved { target, result, .. } if *target == on => Some(result.hp),
                _ => None,
            };
            fight.events.iter().filter_map(hit).collect()
        };
        assert_eq!(hits(OPPONENTS), [8, 8, 8, 5]);
        let taken: i32 = hits(PLAYER).iter().sum();
        assert_eq!((hits(PLAYER).len(), taken > 0), (3, true));
        let left = fight.records[PLAYER].as_ref().expect("the player").hp;
        assert_eq!(i32::from(left), 100 - taken);
        assert_eq!(
            fight.events.last(),
            Some(&Event::Over {
                outcome: ended::WON
            })
        );
        assert!(fight.events.contains(&Event::Left { slot: OPPONENTS }));
        // Nobody chooses once it is over.
        let late = fight.act(&tables, Choice::Defend, false, &mut draws);
        assert_eq!(late, Err(Refusal::NobodyChooses));
    }

    /// The second runner: a target that was hit, lives and defends shows its defence
    /// again; a plain hit shows nothing more, and neither does a target that fell.
    #[test]
    fn a_hit_on_a_defender_is_followed_by_the_second_runner() {
        let tables = tables();
        let mut draws = || 50u32;
        let mut fight =
            Fight::new(&tables, &block(&[12]), party(), 0, NO_DEVA, &mut draws).expect("a fight");
        let attack = Choice::Attack { targets: 0x10 };
        let shown = |fight: &Fight| {
            let shows = |event: &&Event| matches!(event, Event::Shows { .. });
            fight
                .events
                .iter()
                .filter(shows)
                .cloned()
                .collect::<Vec<_>>()
        };
        let player = Step::Choose { slot: PLAYER };
        let opponent = Step::Choose { slot: OPPONENTS };
        assert_eq!(fight.next(&tables, &mut draws), player);
        // The state of the choice, two game frames in front of the handler's: four
        // behind the hub's first pass, and seven behind an ordinary runner.
        assert_eq!(fight.asked_at(), Some(4));
        fight
            .act(&tables, attack, false, &mut draws)
            .expect("taken");
        assert!(shown(&fight).is_empty());
        assert_eq!(fight.asked_at(), None);
        assert_eq!(fight.next(&tables, &mut draws), opponent);
        assert_eq!(fight.asked_at(), Some(7));
        fight
            .act(&tables, Choice::Defend, false, &mut draws)
            .expect("taken");
        assert_eq!(fight.next(&tables, &mut draws), player);
        fight
            .act(&tables, attack, false, &mut draws)
            .expect("taken");
        let defends = vec![Event::Shows {
            slot: OPPONENTS,
            reaction: reaction::DEFENDS,
        }];
        assert_eq!(shown(&fight), defends);
        // It defends once more and falls to the next hit: nothing is shown of it.
        assert_eq!(fight.next(&tables, &mut draws), opponent);
        fight
            .act(&tables, Choice::Defend, false, &mut draws)
            .expect("taken");
        fight.records[OPPONENTS].as_mut().expect("it").hp = 1;
        assert_eq!(fight.next(&tables, &mut draws), player);
        fight
            .act(&tables, attack, false, &mut draws)
            .expect("taken");
        assert_eq!(shown(&fight), defends);
        assert!(fight.events.contains(&Event::Fell { slot: OPPONENTS }));
        // The runs for the picture: kind, actors, targets and the reactions by slot. The
        // second runner is the command "nothing" of nobody; to stop defending is the
        // reaction of the opponent's next turn, a run of its own without targets.
        type Seen = (u32, u8, u8, Vec<(usize, u32)>);
        let runs: Vec<Seen> = (fight.runs.iter())
            .map(|run| {
                let reactions = run.reactions.iter().map(|shown| (shown.slot, shown.kind));
                (run.kind, run.actors, run.targets, reactions.collect())
            })
            .collect();
        let hit = |reaction: u8| (0, 0x01, 0x10, vec![(OPPONENTS, u32::from(reaction))]);
        let defend = (18, 0x10, 0x10, Vec::new());
        assert_eq!(
            runs,
            [
                hit(reaction::HURT),
                defend.clone(),
                hit(reaction::HURT),
                (20, 0, 0x10, vec![(OPPONENTS, 18)]),
                (17, 0x10, 0, Vec::new()),
                defend,
                hit(reaction::FALLS),
            ]
        );
        assert!(fight.runs.iter().all(|run| run.parameter == 0));
        // What each run plays at its key moment: a hit that takes HP its sound, the
        // last one the sound of a fall too (the opponent is of no species that falls
        // with the row); a defence, the second runner and the reaction of a turn's
        // start nothing.
        let hit = Some(sound::HIT_SOUND);
        let sounds: Vec<_> = (fight.runs.iter())
            .map(|run| (run.sounds.hit, run.sounds.fall))
            .collect();
        let none = (None, None);
        let fell = (hit, Some(Fall::Sound));
        assert_eq!(
            sounds,
            [(hit, None), none, (hit, None), none, none, none, fell]
        );
        // The director's game frames in front of each run: the player's first choice
        // from the hub's first pass (hub, next actor, turn, the state between, choice,
        // the state behind the menu, handler), an opponent's command and the player's
        // behind a runner (the wait and the test of the second runner come first), the
        // second runner from that test, the reaction of a turn's start behind a second
        // runner, the choice behind that reaction; and the hub ends the fight three
        // game frames behind the last runner.
        let before: Vec<u32> = fight.runs.iter().map(|run| run.before).collect();
        assert_eq!(before, [6, 8, 9, 2, 5, 5, 9]);
        // The camera each run asks for: a command asks for the first camera of the
        // field, an opponent's defence too; the second runner and the reaction of a
        // turn's start only show and ask for none.
        let cameras: Vec<Option<Pick>> = fight.runs.iter().map(|run| run.camera).collect();
        let plain = Some(Pick::Plain);
        assert_eq!(cameras, [plain, plain, plain, None, None, plain, plain]);
        // The player's own defence asks for none, his creature's neither; while a boss
        // stands on the field every command asks for the boss camera.
        assert_eq!(fight.camera_of(PLAYER, 18), None);
        assert_eq!(fight.camera_of(1, 17), None);
        assert_eq!(fight.camera_of(1, 20), plain);
        assert_eq!(fight.camera_of(OPPONENTS, 18), plain);
        fight.on_field[0] = true;
        fight.records[OPPONENTS].as_mut().expect("it").species = 50;
        assert_eq!(fight.camera_of(PLAYER, 18), Some(Pick::Boss));
        fight.on_field[0] = false;
        let over = Step::Over {
            outcome: ended::WON,
        };
        assert_eq!((fight.next(&tables, &mut draws), fight.done), (over, 3));
    }

    #[test]
    fn the_race_gives_a_tie_to_the_lower_slot_and_skips_who_does_not_fight() {
        let tables = tables();
        let mut draws = || 0u32;
        let mut both = party();
        both.minions = vec![player()];
        let mut fight =
            Fight::new(&tables, &block(&[12, 12]), both, 2, NO_DEVA, &mut draws).expect("a fight");
        // The opponents surprise: 1000, 500 and 333 against nothing.
        assert_eq!(fight.gauges, [0, 0, 0, 0, 1000, 500, 333]);
        assert_eq!(fight.race(&tables), OPPONENTS);
        // The winner's gauge is empty and the slot behind it got no step; the carried
        // creature that does not fight got none either.
        assert_eq!(fight.gauges, [30, 0, 0, 0, 0, 500, 333]);
        (fight.gauges[PLAYER], fight.gauges[OPPONENTS]) = (970, 970);
        assert_eq!(fight.race(&tables), PLAYER);
        assert_eq!((fight.gauges[PLAYER], fight.gauges[OPPONENTS]), (0, 970));
        // A fighter that just came in has a gauge of 800, and the player's stands still.
        fight.choosing = Some(PLAYER);
        assert_eq!(
            fight.act(&tables, Choice::Summon { slot: 2 }, false, &mut draws),
            Err(Refusal::NotThatCreature)
        );
        fight
            .act(&tables, Choice::Summon { slot: 1 }, false, &mut draws)
            .expect("taken");
        assert_eq!(
            (fight.fighter, fight.gauges[1], fight.gauges[PLAYER]),
            (1, 800, 0)
        );
        assert!(fight.stands(1) && !fight.stands(PLAYER));
        // The summon is two runs: the player's own, at the creature, and its entrance.
        let shown = |fight: &Fight| {
            let runs = fight.runs.iter().rev().take(2).rev();
            runs.map(|run| (run.kind, run.actors, run.targets))
                .collect::<Vec<_>>()
        };
        assert_eq!(shown(&fight), [(2, 1 << 0, 1 << 1), (2, 1 << 1, 0)]);
        // The creature's change for the player: its leave, and he comes back in.
        fight.choosing = Some(1);
        fight
            .act(&tables, Choice::Change { slot: PLAYER }, false, &mut draws)
            .expect("taken");
        assert_eq!(fight.fighter, PLAYER);
        assert_eq!(shown(&fight), [(3, 1 << 1, 0), (3, 1 << 0, 0)]);
        // No row is started for it: that is for a creature that goes for another.
        assert!(fight.runs.iter().all(|run| run.sounds.start.is_none()));
        // A creature that falls: the hub sends the player in with the second half of
        // the leave alone.
        fight.choosing = Some(PLAYER);
        fight
            .act(&tables, Choice::Summon { slot: 1 }, false, &mut draws)
            .expect("taken");
        let runs = fight.runs.len();
        fight.fell(1);
        fight.next(&tables, &mut draws);
        assert_eq!(fight.fighter, PLAYER);
        let came = &fight.runs[runs];
        assert_eq!((came.kind, came.actors, came.targets), (3, 1 << 0, 0));
        assert_eq!(fight.flags & flag::MINION_FELL, 0);
    }

    #[test]
    fn the_lists_of_a_creature_answer_by_its_record() {
        use super::super::menu::{Asked, Listed, Lists, class, level, text};
        let tables = tables();
        let mut draws = || 0u32;
        let mut both = party();
        both.minions = vec![player()];
        let mut fight =
            Fight::new(&tables, &block(&[12]), both, 1, NO_DEVA, &mut draws).expect("a fight");
        fight.fighter = 1;
        let known = |attack: u8| crate::battle::Move {
            known: true,
            attack,
            ..Default::default()
        };
        let it = fight.records[1].as_mut().expect("the creature");
        it.moves = Default::default();
        // The first costs 10 MP and goes to one opponent, the second 15 and to all.
        (it.moves[0], it.moves[1]) = (known(2), known(12));
        (it.spells, it.mp) = (0b101, 12);
        fn of<'a>(fight: &'a Fight, tables: &'a Tables) -> Lists<'a> {
            Lists {
                fight,
                tables,
                places: &[],
            }
        }
        assert_eq!(of(&fight, &tables).offer(0).spells, 2);
        let one = Listed {
            parameter: 0,
            class: class::OPPONENTS,
        };
        assert_eq!(of(&fight, &tables).attack(0), Ok(one));
        assert_eq!(of(&fight, &tables).attack(1), Err(Some(text::NO_MANA)));
        assert_eq!(of(&fight, &tables).attack(2), Err(None));
        fight.records[1].as_mut().expect("the creature").mp = 15;
        let all = of(&fight, &tables).attack(1).expect("enough MP now");
        assert_eq!(
            (all.parameter, all.class),
            (1, class::OPPONENTS | class::EVERYBODY)
        );
        // The second row of the spells is spell 2, and there is no third.
        let word = tables.spells[2].word;
        fight.records[1].as_mut().expect("the creature").mp = i16::MAX;
        let spell = of(&fight, &tables).spell(1).expect("a spell it knows");
        assert_eq!(spell.parameter, 2);
        assert_eq!(spell.class & class::OWN != 0, word & 0x40 != 0);
        assert_eq!(spell.class & class::OPPONENTS != 0, word & 0x80 != 0);
        assert_eq!(spell.class & class::EVERYBODY != 0, word & 0x04 != 0);
        assert_eq!(of(&fight, &tables).spell(2), Err(None));
        fight.records[1].as_mut().expect("the creature").mp = -1;
        assert_eq!(of(&fight, &tables).spell(1), Err(Some(text::NO_MANA)));
        // The target of a spell for the own side: one that heals is refused while the
        // creature has all its HP, one that cures while it has no status; a spell for
        // the opponents is never refused.
        let mut own = tables.clone();
        own.spells[2].word |= 0x40;
        let aimed = |fight: &Fight, tables: &Tables| {
            of(fight, tables).target(super::super::menu::command::SPELL, 2, 1)
        };
        let it = fight.records[1].as_mut().expect("the creature");
        (it.hp, it.max_hp, it.status) = (50, 50, 0);
        assert_eq!(aimed(&fight, &own), Err(Some(text::NO_NEED)));
        assert_eq!(aimed(&fight, &tables), Ok(()));
        fight.records[1].as_mut().expect("the creature").hp = 49;
        assert_eq!(aimed(&fight, &own), Ok(()));
        own.spells[2].need = 1;
        assert_eq!(aimed(&fight, &own), Err(Some(text::NO_NEED)));
        fight.records[1].as_mut().expect("the creature").status = 0x008;
        assert_eq!(aimed(&fight, &own), Ok(()));
        fight.records[1].as_mut().expect("the creature").status = 0x004;
        assert_eq!(aimed(&fight, &own), Err(Some(text::NO_NEED)));
        fight.records[1].as_mut().expect("the creature").status = 0;
        // What the rows show: without mana every known entry and every spell is
        // refused for its cost, and the circle describes a row that has a spell.
        let rows = of(&fight, &tables).attack_rows();
        assert_eq!(
            (rows[0].known, rows[0].attack, rows[0].refused),
            (true, 2, true)
        );
        assert_eq!((rows[2].known, rows[2].refused), (false, false));
        let shown = of(&fight, &tables).spell_rows(0);
        assert_eq!(shown.rows, [0, 2, 30, 30]);
        assert_eq!((shown.short, shown.known), ([true, true, false, false], 2));
        assert_eq!(of(&fight, &tables).spell_rows(1).rows, [2, 30, 30, 30]);
        assert!(of(&fight, &tables).described(level::SPELLS, 1));
        assert!(!of(&fight, &tables).described(level::SPELLS, 2));
        // The player has no lists.
        fight.fighter = PLAYER;
        assert_eq!(of(&fight, &tables).attack(0), Err(None));
        assert_eq!(of(&fight, &tables).offer(0).spells, 0);
    }

    #[test]
    fn the_start_of_a_turn_ticks_the_status() {
        let tables = tables();
        let mut draws = || 0u32;
        let mut fight =
            Fight::new(&tables, &block(&[12]), party(), 0, NO_DEVA, &mut draws).expect("a fight");
        let status_of = |fight: &Fight| fight.records[PLAYER].as_ref().expect("him").status;
        let set = |fight: &mut Fight, status: u32| {
            fight.records[PLAYER].as_mut().expect("him").status = status;
        };
        // A sleeper whose draw is below 77 loses the turn in silence.
        set(&mut fight, status::SLEEP);
        assert!(!fight.start_turn(PLAYER, &mut || 76u32));
        assert_eq!(fight.events.last(), Some(&Event::Lost { slot: PLAYER }));
        // From 77 on he wakes, shows it and chooses.
        assert!(fight.start_turn(PLAYER, &mut || 77u32));
        assert_eq!(status_of(&fight), 0);
        let woke = Event::Tick {
            slot: PLAYER,
            reaction: reaction::WAKES,
            hp: 0,
            mp: 0,
        };
        assert_eq!(fight.events.last(), Some(&woke));
        // Poison takes a tenth of the highest HP and wakes without a draw's help.
        set(&mut fight, status::POISON | status::SLEEP);
        assert!(fight.start_turn(PLAYER, &mut || 0u32));
        let player = fight.records[PLAYER].as_ref().expect("him");
        assert_eq!((player.hp, player.status), (90, status::POISON));
        // Who defended gets MP back (level / 4 + 3, cut to what he lacks) and stops.
        set(&mut fight, status::DEFENDING);
        assert!(fight.start_turn(PLAYER, &mut || 0u32));
        assert_eq!(status_of(&fight), 0);
        // Stone moves one step up at each turn; at the last one the fighter falls.
        set(&mut fight, 0x0008);
        assert!(fight.start_turn(PLAYER, &mut || 0u32));
        assert_eq!(status_of(&fight), 0x0010);
        // That step has no reaction and no run of its own: the word waits for the next
        // run, which hands it to the picture.
        assert_eq!(fight.ticked, [(PLAYER, 0x0010)]);
        // The group is shifted as a whole: a last step that stood would become the
        // dead bit (no fighter starts a turn alive with it).
        set(&mut fight, 0x0200);
        assert!(fight.start_turn(PLAYER, &mut || 0u32));
        assert_eq!(status_of(&fight), status::DEAD);
        set(&mut fight, 0x0100);
        assert!(!fight.start_turn(PLAYER, &mut || 0u32));
        assert_eq!(fight.flags, flag::PLAYER_FELL);
        // His fall is heard as the row of the fight's sequences, an opponent's by its
        // species, and a creature of his as the sound.
        let last = fight.runs.last().expect("the run of his fall");
        assert_eq!(
            last.ticked,
            [(PLAYER, 0x0010), (PLAYER, status::DEAD), (PLAYER, 0x0200)]
        );
        assert!(fight.ticked.is_empty());
        assert_eq!(
            last.sounds,
            Sounds {
                hit: None,
                fall: Some(Fall::Row),
                start: None,
            }
        );
        fight.flags = 0;
        assert_eq!(fight.fall(), Fall::Sound);
        fight.falls_with_the_row = true;
        assert_eq!(fight.fall(), Fall::Row);
        fight.flags = flag::MINION_FELL;
        assert_eq!(fight.fall(), Fall::Sound);
        fight.flags = flag::PLAYER_FELL;
        assert_eq!(
            fight.next(&tables, &mut draws),
            Step::Over {
                outcome: ended::LOST
            }
        );
    }

    #[test]
    fn the_menu_s_refusals_and_the_flight() {
        let tables = tables();
        let mut draws = || 0u32;
        let closed = Block {
            retreat: false,
            ..block(&[41])
        };
        let mut fight =
            Fight::new(&tables, &closed, party(), 1, NO_DEVA, &mut draws).expect("a fight");
        // The player surprises: his turn at once. A boss, no carried creature, no retreat.
        assert_eq!(
            fight.next(&tables, &mut draws),
            Step::Choose { slot: PLAYER }
        );
        let all = Closed {
            summon: true,
            capture: true,
            flee: true,
        };
        assert_eq!(fight.closed(), all);
        let mut refused = |choice: Choice| fight.act(&tables, choice, false, &mut || 0u32);
        assert_eq!(refused(Choice::Flee), Err(Refusal::Closed));
        assert_eq!(refused(Choice::Summon { slot: 1 }), Err(Refusal::Closed));
        assert_eq!(
            refused(Choice::Capture { targets: 0x10 }),
            Err(Refusal::Closed)
        );
        let spell = Choice::Spell {
            spell: 0,
            targets: 0x10,
        };
        assert_eq!(refused(spell), Err(Refusal::NotItsCommand));
        assert_eq!(
            refused(Choice::Change { slot: 0 }),
            Err(Refusal::NotItsCommand)
        );
        assert_eq!(
            refused(Choice::Attack { targets: 0x20 }),
            Err(Refusal::NoSuchTarget)
        );
        assert_eq!(
            refused(Choice::Item {
                item: 0,
                targets: 1
            }),
            Err(Refusal::NoUse)
        );
        // A refusal does not spend the turn.
        assert_eq!(
            fight.next(&tables, &mut draws),
            Step::Choose { slot: PLAYER }
        );

        // Where a retreat is allowed: a draw of 81 stops the flight, one below does not.
        let mut fight =
            Fight::new(&tables, &block(&[12]), party(), 1, NO_DEVA, &mut draws).expect("a fight");
        assert_eq!(
            fight.next(&tables, &mut draws),
            Step::Choose { slot: PLAYER }
        );
        assert!(!fight.closed().flee && !fight.closed().capture);
        fight
            .act(&tables, Choice::Flee, false, &mut || 81u32)
            .expect("taken");
        assert_eq!(fight.flags, 0);
        // Defending: the bit stands at once and the turn is spent.
        fight.gauges[PLAYER] = 1000;
        assert_eq!(
            fight.next(&tables, &mut draws),
            Step::Choose { slot: PLAYER }
        );
        fight
            .act(&tables, Choice::Defend, false, &mut draws)
            .expect("taken");
        let status = fight.records[PLAYER].as_ref().expect("him").status;
        assert_eq!(status, status::DEFENDING);
        fight.gauges[PLAYER] = 1000;
        assert_eq!(
            fight.next(&tables, &mut draws),
            Step::Choose { slot: PLAYER }
        );
        fight
            .act(&tables, Choice::Flee, false, &mut || 80u32)
            .expect("taken");
        assert_eq!(fight.flags, flag::PLAYER_RAN);
        assert_eq!(
            fight.next(&tables, &mut draws),
            Step::Over {
                outcome: ended::RAN
            }
        );
    }
}
