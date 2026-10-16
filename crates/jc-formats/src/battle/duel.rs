
use super::arena::Pick;
use super::fight::{
    Block, Counted, Fall, Fight, Given, LAST_STONE, OPPONENTS, PLAYER, Party, Run,
    SHOWN_WHEN_GIVEN, Shown, Sounds, Ticked, UNABLE, counted, flashed, heard, kind, reaction,
    shaken, tick, took,
};
use super::menu::{Chosen, spell_needed};
use super::record::{Draws, Random, Record, Side, stat, status};
use super::resolve::{Command, NO_DEVA, Order, Resolved, outcome, resolve};
use super::{Tables, item, using};
use crate::player_panel::spells::SPELLS;

/// The two sides: 0 the Minions of the Dawn (player 1), 1 the Minions of the Sunset.
pub const SIDES: usize = 2;
/// The minions of a side.
pub const MOST: usize = 3;
/// A gauge that reaches this acts.
const FULL: i16 = 1000;
/// The head start of the first turn (`0x800A58EC`).
pub const HEAD_START: i16 = 100;
/// The gauge of a minion that came in by a change: this less a draw of so many
/// (`0x800A6DA4`).
const CHANGE_SPREAD: u32 = 200;
/// The faces of a die.
const FACES: u32 = 6;

/// The slot of the story's fight that shows a side's minion of a team place.
pub fn slot(side: usize, index: usize) -> usize {
    1 + side * MOST + index
}

/// The side and the team place of a slot of [`slot`]; `None` for slot 0 and past 6.
pub fn place_of(slot: usize) -> Option<(usize, usize)> {
    let n = slot.checked_sub(1).filter(|n| *n < SIDES * MOST)?;
    Some((n / MOST, n % MOST))
}

/// What both players agreed on before the duel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Each side may use the items of its save (the build's own rule; the original has
    /// none: "Rules: Original").
    pub items: bool,
    /// A side may change its minion for another (file 33's "Allow minion changing?").
    pub changing: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            items: true,
            changing: true,
        }
    }
}

/// What a side is told to do in its turn: file 29's five commands, the build's items and
/// "Give up" of the pause.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Choice {
    Attack,
    Defend,
    /// A body attack: the place of the move entry, 0 to 4.
    Body {
        slot: usize,
    },
    /// A spell, at the own fighter (`own`) or at the opponent's.
    Spell {
        spell: usize,
        own: bool,
    },
    /// Another minion of the team comes in (team place 0 to 2).
    Change {
        index: usize,
    },
    /// An item of the side's save on a minion of its team (team place 0 to 2).
    Item {
        item: usize,
        index: usize,
    },
    /// "Give up (results in a loss)".
    GiveUp,
}

impl Choice {
    /// What the story's command menu hands over, as a side's own menu has it
    /// ([`Duel::menu_fight`]: the own minions in the slots 1 to 3, the opponent in 4 to
    /// 6): an attack, a defence, a body attack, a spell (at the own fighter when the
    /// target mask names an own slot), a change and an item on an own minion. `None`
    /// for anything else, which the menu of a minion does not hand over.
    pub fn of_menu(chosen: &Chosen) -> Option<Self> {
        let own = |mask: u8| mask & 0b1110 != 0;
        let index = |slot: usize| slot.checked_sub(1).filter(|index| *index < MOST);
        let parameter = chosen.parameter as usize;
        Some(match chosen.kind {
            0 => Choice::Attack,
            18 => Choice::Defend,
            6 => Choice::Body { slot: parameter },
            1 => Choice::Spell {
                spell: parameter,
                own: own(chosen.targets),
            },
            8 => Choice::Change {
                index: index(chosen.picked?)?,
            },
            4 => Choice::Item {
                item: parameter,
                index: index(chosen.targets.trailing_zeros() as usize)?,
            },
            _ => return None,
        })
    }

    pub fn kind(&self) -> u32 {
        match self {
            Choice::Attack => kind::ATTACK,
            Choice::Spell { .. } => kind::SPELL,
            Choice::Item { .. } => kind::ITEM,
            Choice::Body { .. } => kind::BODY,
            Choice::Change { .. } => 8,
            Choice::Defend => u32::from(reaction::DEFENDS),
            Choice::GiveUp => kind::NOTHING,
        }
    }
}

/// Why a choice is not taken. The turn is not spent and nothing changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The duel is over, or it is not this side's turn.
    Over,
    NotYourTurn,
    /// The options of the duel close the command (changing, items).
    NotAllowed,
    /// The fighter does not know the body attack or the spell.
    NotKnown,
    NotEnoughMp,
    /// A team place without a minion, a fallen one, or the one that fights already.
    NotThatMinion,
    /// No piece of the item left, or an item that is not for a minion in a fight.
    NoPiece,
    /// The item or the spell is not for that target, or for the dead.
    NotForThat,
    /// It would do nothing (`0x80033E50` answer 3, `0x800340C4`).
    NoUse,
    /// The tables have no such row.
    NoSuchRow,
}

/// What the duel waits for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// This side chooses the command of its turn; `turn` counts the turns from 1.
    Choose {
        side: usize,
        turn: u32,
    },
    Over {
        winner: usize,
        gave_up: bool,
    },
}

/// The build's dice roll for the head start, shown at the start of every duel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Start {
    /// Every roll, the die of side 0 first: a draw is rolled again, so only the last
    /// one differs.
    pub rolls: Vec<[u8; 2]>,
    /// The side that got the head start.
    pub winner: usize,
    /// The two lead minions have the same step: "Equally fast!".
    pub equally_fast: bool,
}

/// What happened, in the order the duel decides it. Slots are those of [`slot`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// One roll of the two dice for the head start, and who won it.
    Rolled {
        dice: [u8; 2],
    },
    HeadStart {
        side: usize,
    },
    /// A minion stands on the field: at the start, by a change, or behind a fall.
    Entered {
        slot: usize,
    },
    Left {
        slot: usize,
    },
    /// The race gave the turn to this side's fighter.
    Turn {
        side: usize,
        turn: u32,
    },
    /// The start of the turn changed the fighter's status and shows a reaction.
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
    Chose {
        side: usize,
        choice: Choice,
    },
    /// One call of the resolver, with the changes cut to what the target has.
    Resolved {
        actor: usize,
        target: usize,
        result: Resolved,
        reaction: u8,
    },
    /// A piece of an item was spent.
    Used {
        side: usize,
        item: usize,
    },
    /// The second runner: a fighter shows the status it stands with.
    Shows {
        slot: usize,
        reaction: u8,
    },
    Fell {
        slot: usize,
    },
    GaveUp {
        side: usize,
    },
    Over {
        winner: usize,
        gave_up: bool,
    },
}

/// What a checked choice becomes.
#[derive(Debug, Clone, Copy)]
enum Plan {
    GiveUp,
    Defend,
    Change(usize),
    Resolve { command: Command, target: usize },
}

/// A duel in progress.
#[derive(Debug, Clone)]
pub struct Duel {
    /// Each side's team, by team place.
    pub teams: [[Option<Record>; MOST]; SIDES],
    /// The team place of each side's fighter.
    pub fighter: [Option<usize>; SIDES],
    pub gauges: [i16; SIDES],
    pub options: Options,
    /// The items each side has left: number and pieces.
    pub items: [Vec<(usize, u32)>; SIDES],
    /// The deva on the field ([`NO_DEVA`] for none).
    pub deva: i16,
    pub start: Start,
    /// What happened so far, and the runs of the action runner for the picture.
    pub events: Vec<Event>,
    pub runs: Vec<Run>,
    seed: u32,
    random: Random,
    /// The turns given so far, and the commands taken.
    turn: u32,
    acts: u32,
    choosing: Option<usize>,
    outcome: Option<(usize, bool)>,
    /// The status words of turn starts that no run has taken yet ([`Run::ticked`]).
    ticked: Vec<(usize, u32)>,
    /// The director's game frames as the story's fight counts them ([`took`]). Stand-in:
    /// file 34's own director was not read for its frames.
    director: u32,
}

impl Duel {
    /// The duel before its first turn: both teams made whole (`0x80011CD0` of file 33),
    /// each side's first living minion on the field (`0x800A57E0`), and the dice rolled
    /// from the generator until they differ. Teams take their first three records;
    /// `items` is ignored when the options have none.
    pub fn new(
        tables: &Tables,
        teams: [Vec<Record>; SIDES],
        items: [Vec<(usize, u32)>; SIDES],
        options: Options,
        seed: u32,
    ) -> Self {
        let teams = teams.map(|team| {
            let mut records = team.into_iter().take(MOST).map(|mut record| {
                record.make_whole();
                Some(record)
            });
            std::array::from_fn(|_| records.next().flatten())
        });
        let items = items.map(|list| match options.items {
            true => list.into_iter().filter(|entry| entry.1 != 0).collect(),
            false => Vec::new(),
        });
        let mut duel = Self {
            teams,
            fighter: [None; SIDES],
            gauges: [0; SIDES],
            options,
            items,
            deva: NO_DEVA,
            start: Start {
                rolls: Vec::new(),
                winner: 0,
                equally_fast: false,
            },
            events: Vec::new(),
            runs: Vec::new(),
            seed,
            random: Random { seed },
            turn: 0,
            acts: 0,
            choosing: None,
            outcome: None,
            ticked: Vec::new(),
            director: 0,
        };
        for side in 0..SIDES {
            if let Some(index) = duel.next_living(side) {
                duel.fighter[side] = Some(index);
                duel.events.push(Event::Entered {
                    slot: slot(side, index),
                });
            }
        }
        // The build's dice: two dice from the shared generator, rolled again on a draw.
        loop {
            let dice = [0; SIDES].map(|_| (duel.random.draw() % FACES + 1) as u8);
            duel.start.rolls.push(dice);
            duel.events.push(Event::Rolled { dice });
            if dice[0] != dice[1] {
                duel.start.winner = usize::from(dice[1] > dice[0]);
                break;
            }
        }
        duel.events.push(Event::HeadStart {
            side: duel.start.winner,
        });
        let steps = [0, 1].map(|side| duel.step(tables, side));
        duel.start.equally_fast = steps[0] == steps[1];
        duel
    }

    pub fn seed(&self) -> u32 {
        self.seed
    }

    /// The turns given so far.
    pub fn turn(&self) -> u32 {
        self.turn
    }

    /// The commands taken so far.
    pub fn acts(&self) -> u32 {
        self.acts
    }

    pub fn outcome(&self) -> Option<(usize, bool)> {
        self.outcome
    }

    /// The record of a slot of [`slot`].
    pub fn record(&self, slot: usize) -> Option<&Record> {
        let (side, index) = place_of(slot)?;
        self.teams[side][index].as_ref()
    }

    /// The record of a side's fighter.
    pub fn fighting(&self, side: usize) -> Option<&Record> {
        self.teams[side][self.fighter[side]?].as_ref()
    }

    /// The slot of a side's fighter.
    pub fn fighter_slot(&self, side: usize) -> Option<usize> {
        self.fighter[side].map(|index| slot(side, index))
    }

    /// Whether a slot stands on the field.
    pub fn stands(&self, slot: usize) -> bool {
        place_of(slot).is_some_and(|(side, index)| self.fighter[side] == Some(index))
    }

    fn odd_frame(&self) -> bool {
        self.seed.wrapping_add(self.acts) & 1 == 1
    }

    /// Whether a side still has a minion that has not fallen (`0x800A4DC0`).
    fn alive(&self, side: usize) -> bool {
        self.teams[side]
            .iter()
            .flatten()
            .any(|record| !record.is_dead())
    }

    /// The next living minion of a side in team order, from the place behind its
    /// fighter round to it (`0x800A57E0`); from place 0 when none fights yet.
    fn next_living(&self, side: usize) -> Option<usize> {
        let from = self.fighter[side].map_or(0, |index| index + 1);
        (0..MOST)
            .map(|n| (from + n) % MOST)
            .find(|&index| (self.teams[side][index].as_ref()).is_some_and(|r| !r.is_dead()))
    }

    /// The step of a side's gauge (`0x800A5A68`): 10 and the fighter's speed with the
    /// equipment rows, which a minion does not have; 0 for a side without a fighter.
    pub fn step(&self, tables: &Tables, side: usize) -> i16 {
        match self.fighting(side) {
            Some(record) => 10 + i16::from(record.effective(tables).stats[stat::SPEED]),
            None => 0,
        }
    }

    /// The race of two (`0x800A5894`): on the first turn the head start goes to one
    /// gauge; then passes until a gauge is full. Side 0 steps first and, when it acts,
    /// side 1 gets no step in that pass; the winner's gauge loses 1000 and keeps the
    /// overshoot, and side 1 gets its step once more behind the subtraction.
    fn race(&mut self, tables: &Tables) -> usize {
        if self.gauges == [0, 0] {
            let winner = self.start.winner;
            self.gauges[winner] = self.gauges[winner].wrapping_add(HEAD_START);
        }
        let steps = [0, 1].map(|side| self.step(tables, side));
        loop {
            self.gauges[0] = self.gauges[0].wrapping_add(steps[0]);
            if self.gauges[0] >= FULL {
                self.gauges[0] -= FULL;
                return 0;
            }
            self.gauges[1] = self.gauges[1].wrapping_add(steps[1]);
            if self.gauges[1] >= FULL {
                self.gauges[1] = (self.gauges[1] - FULL).wrapping_add(steps[1]);
                return 1;
            }
        }
    }

    /// The hub: goes on until a side has to choose or the duel is over.
    pub fn next(&mut self, tables: &Tables) -> Step {
        loop {
            if let Some((winner, gave_up)) = self.outcome {
                return Step::Over { winner, gave_up };
            }
            if let Some(side) = self.choosing {
                return Step::Choose {
                    side,
                    turn: self.turn,
                };
            }
            // A side without a living minion has lost; side 0 is asked first.
            if let Some(loser) = (0..SIDES).find(|&side| !self.alive(side)) {
                self.end(1 - loser, false);
                continue;
            }
            // A fallen fighter: the next living one comes in by itself, with the
            // gauge as it stands (`0x800A5530`, states 10 to 12).
            for side in 0..SIDES {
                if self.fighting(side).is_none_or(Record::is_dead) {
                    self.fighter[side] = self.next_living(side);
                    if let Some(index) = self.fighter[side] {
                        let comes = slot(side, index);
                        let before = self.director + took::TO_ITS_REACTION;
                        self.moves(kind::SUMMON, comes, before);
                        self.director = took::TO_THE_HUB;
                        self.events.push(Event::Entered { slot: comes });
                    }
                }
            }
            let side = self.race(tables);
            self.turn += 1;
            self.events.push(Event::Turn {
                side,
                turn: self.turn,
            });
            if self.start_turn(side) {
                self.choosing = Some(side);
            }
        }
    }

    fn end(&mut self, winner: usize, gave_up: bool) {
        self.outcome = Some((winner, gave_up));
        self.choosing = None;
        self.events.push(Event::Over { winner, gave_up });
    }

    /// The start of a turn (`0x800A5B5C` with file 36's `0x800BF994` and `0x800BFC1C`):
    /// the status tick and what follows from it, as in the story's fight. Answers
    /// whether the side chooses a command.
    fn start_turn(&mut self, side: usize) -> bool {
        let turn = self.director + took::TO_THE_TURN;
        let Some(index) = self.fighter[side] else {
            return false;
        };
        let at = slot(side, index);
        let Some(record) = self.teams[side][index].as_mut() else {
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
        } = tick(record, &mut self.random);
        let has = record.hp;
        if new != old {
            self.ticked.push((at, new));
        }
        if falls {
            let reaction = match reaction {
                reaction::STONE_DEATH => reaction,
                _ => reaction::FALLS,
            };
            let hp = has.into();
            self.events.push(Event::Tick {
                slot: at,
                reaction,
                hp,
                mp: 0,
            });
            self.push_run(Run {
                before: turn + took::TO_ITS_REACTION,
                ..Run::of_reaction(at, reaction, (hp, 0), false)
            });
            self.director = took::TO_THE_HUB;
            self.fell(at);
            if let Some(run) = self.runs.last_mut() {
                run.sounds.fall = Some(Fall::Sound);
            }
            return false;
        }
        if lost {
            self.events.push(Event::Lost { slot: at });
            self.director = turn + took::LOST;
            return false;
        }
        if reaction == reaction::NONE {
            self.director = turn + took::TO_THE_CHOICE;
            return true;
        }
        self.events.push(Event::Tick {
            slot: at,
            reaction,
            hp,
            mp,
        });
        self.push_run(Run {
            before: turn + took::TO_ITS_REACTION,
            ..Run::of_reaction(at, reaction, (hp, mp), false)
        });
        self.change(at, hp, mp);
        let after = self.record(at).map_or(status::DEAD, |r| r.status);
        let chooses = after & (status::DEAD | status::SLEEP | UNABLE) == 0;
        self.director = match chooses {
            true => took::BEHIND_ITS_REACTION,
            false => took::TO_THE_HUB,
        };
        chooses
    }

    fn record_mut(&mut self, slot: usize) -> Option<&mut Record> {
        let (side, index) = place_of(slot)?;
        self.teams[side][index].as_mut()
    }

    fn push_run(&mut self, mut run: Run) {
        run.ticked = std::mem::take(&mut self.ticked);
        self.runs.push(run);
    }

    /// A run of a handler that resolves nothing: a leave or an entrance.
    fn moves(&mut self, kind: u32, actor: usize, before: u32) {
        self.push_run(Run {
            kind,
            before,
            camera: Some(Pick::Plain),
            ..Run::of_reaction(actor, 0, (0, 0), false)
        });
    }

    /// Moves a change into a record, a loss counted positive, held between nothing and
    /// the highest; a record that comes to no HP is dead.
    fn change(&mut self, slot: usize, hp: i32, mp: i32) {
        let Some(record) = self.record_mut(slot) else {
            return;
        };
        record.hp = (i32::from(record.hp) - hp).clamp(0, record.max_hp.into()) as i16;
        record.mp = (i32::from(record.mp) - mp).clamp(0, record.max_mp.into()) as i16;
        if record.hp == 0 {
            record.status |= status::DEAD;
        }
    }

    /// A fighter fell. Its record stays as it is; the hub brings the next one in.
    fn fell(&mut self, slot: usize) {
        if let Some(record) = self.record_mut(slot) {
            record.hp = 0;
            record.status |= status::DEAD;
        }
        self.events.push(Event::Fell { slot });
    }

    pub fn menu_fight(&self, tables: &Tables, side: usize) -> Option<Fight> {
        let other = 1 - side;
        let index = self.fighter[side]?;
        let block = Block {
            mode: 1,
            on_field: 0,
            retreat: false,
            results: false,
            specs: Vec::new(),
        };
        let party = Party {
            player: self.teams[side][index].clone()?,
            minions: Vec::new(),
            cocoons: 0,
            items: self.items[side].clone(),
            room: 0,
        };
        let mut seen = Random { seed: 0 };
        let mut fight = Fight::new(tables, &block, party, 0, NO_DEVA, &mut seen)?;
        fight.records[PLAYER] = None;
        for place in 0..MOST {
            fight.records[1 + place] = self.teams[side][place].clone();
            fight.records[OPPONENTS + place] = self.teams[other][place].clone();
            fight.on_field[place] = self.fighter[other] == Some(place);
        }
        fight.fighter = 1 + index;
        fight.deva = self.deva;
        Some(fight)
    }

    /// Whether a choice would be taken now, without taking it.
    pub fn check(&self, tables: &Tables, side: usize, choice: Choice) -> Result<(), Refusal> {
        self.plan(tables, side, choice).map(|_| ())
    }

    /// Every choice the side could make now but "Give up": what a menu offers, and what
    /// a check run picks from.
    pub fn legal(&self, tables: &Tables, side: usize) -> Vec<Choice> {
        let mut choices = vec![Choice::Attack, Choice::Defend];
        choices.extend((0..5).map(|slot| Choice::Body { slot }));
        for spell in 0..SPELLS {
            choices.extend([false, true].map(|own| Choice::Spell { spell, own }));
        }
        choices.extend((0..MOST).map(|index| Choice::Change { index }));
        for &(item, _) in &self.items[side] {
            choices.extend((0..MOST).map(|index| Choice::Item { item, index }));
        }
        choices.retain(|choice| self.check(tables, side, *choice).is_ok());
        choices
    }

    /// The tests of a choice: the menu's of file 29 and of the story's lists, and the
    /// build's for the items.
    fn plan(&self, tables: &Tables, side: usize, choice: Choice) -> Result<Plan, Refusal> {
        if self.outcome.is_some() {
            return Err(Refusal::Over);
        }
        if self.choosing != Some(side) {
            return Err(Refusal::NotYourTurn);
        }
        let other = 1 - side;
        let (Some(index), Some(record)) = (self.fighter[side], self.fighting(side)) else {
            return Err(Refusal::NotYourTurn);
        };
        let against = self.fighter_slot(other).ok_or(Refusal::Over)?;
        Ok(match choice {
            Choice::GiveUp => Plan::GiveUp,
            Choice::Defend => Plan::Defend,
            Choice::Attack => Plan::Resolve {
                command: Command::Attack,
                target: against,
            },
            Choice::Body { slot } => {
                let entry = record.moves.get(slot).ok_or(Refusal::NoSuchRow)?;
                if !entry.known {
                    return Err(Refusal::NotKnown);
                }
                if entry.cost() > record.mp.into() {
                    return Err(Refusal::NotEnoughMp);
                }
                Plan::Resolve {
                    command: Command::Body(slot),
                    target: against,
                }
            }
            Choice::Spell { spell, own } => {
                if spell >= SPELLS || record.spells >> spell & 1 == 0 {
                    return Err(Refusal::NotKnown);
                }
                let row = tables.spells.get(spell).ok_or(Refusal::NoSuchRow)?;
                if i16::from(row.cost) > record.mp {
                    return Err(Refusal::NotEnoughMp);
                }
                // Bit 6 of the spell's word is the own side, bit 7 the opponents
                // (`0x8001ACE4`); a spell of neither goes to the opponent.
                let (for_own, for_them) = (row.word >> 6 & 1 != 0, row.word >> 7 & 1 != 0);
                if own && !for_own || !own && for_own && !for_them {
                    return Err(Refusal::NotForThat);
                }
                if own && !spell_needed(tables, spell, record) {
                    return Err(Refusal::NoUse);
                }
                Plan::Resolve {
                    command: Command::Spell(spell),
                    target: match own {
                        true => slot(side, index),
                        false => against,
                    },
                }
            }
            Choice::Change { index: to } => {
                if !self.options.changing {
                    return Err(Refusal::NotAllowed);
                }
                let comes = self.teams[side].get(to).and_then(Option::as_ref);
                if to == index || comes.is_none_or(Record::is_dead) {
                    return Err(Refusal::NotThatMinion);
                }
                Plan::Change(to)
            }
            Choice::Item { item, index: to } => {
                if !self.options.items {
                    return Err(Refusal::NotAllowed);
                }
                let pieces = self.items[side].iter().find(|entry| entry.0 == item);
                if pieces.is_none_or(|entry| entry.1 == 0) {
                    return Err(Refusal::NoPiece);
                }
                let flags = tables.items.get(item).ok_or(Refusal::NoSuchRow)?.flags;
                if !for_a_minion(flags) {
                    return Err(Refusal::NoPiece);
                }
                let target = self.teams[side].get(to).and_then(Option::as_ref);
                let target = target.ok_or(Refusal::NotThatMinion)?;
                match using::aim_test(item, flags, 1 + to, target) {
                    using::aim::FINE => {}
                    using::aim::NO_NEED => return Err(Refusal::NoUse),
                    _ => return Err(Refusal::NotForThat),
                }
                Plan::Resolve {
                    command: Command::Item(item),
                    target: slot(side, to),
                }
            }
        })
    }

    /// Takes the command of the side whose turn it is and plays it to the next pass of
    /// the hub ([`Duel::next`]). A refused choice changes nothing.
    pub fn act(&mut self, tables: &Tables, side: usize, choice: Choice) -> Result<(), Refusal> {
        let plan = self.plan(tables, side, choice)?;
        let odd_frame = self.odd_frame();
        let actor = self.fighter_slot(side).ok_or(Refusal::NotYourTurn)?;
        let handler = self.director + took::THE_PLAYER_CHOOSES;
        let (command, target) = match plan {
            Plan::GiveUp => {
                self.acts += 1;
                self.events.push(Event::Chose { side, choice });
                self.events.push(Event::GaveUp { side });
                self.end(1 - side, true);
                return Ok(());
            }
            Plan::Change(to) => {
                // The leave of kind 3, then the other one's entrance of kind 2 with the
                // gauge 1000 - rand() % 200 (`0x800A5F9C`, `0x800A67E0` case 5).
                self.acts += 1;
                self.events.push(Event::Chose { side, choice });
                self.choosing = None;
                self.moves(kind::LEAVE, actor, handler);
                if let Some(leave) = self.runs.last_mut() {
                    leave.sounds.start = Some(crate::sound::CHANGE_ROW);
                }
                self.events.push(Event::Left { slot: actor });
                self.fighter[side] = Some(to);
                let comes = slot(side, to);
                self.gauges[side] = FULL - (self.random.draw() % CHANGE_SPREAD) as i16;
                self.moves(kind::SUMMON, comes, took::TO_THE_SECOND);
                self.director = took::TO_THE_HUB;
                self.events.push(Event::Entered { slot: comes });
                return Ok(());
            }
            Plan::Defend => {
                self.acts += 1;
                self.events.push(Event::Chose { side, choice });
                self.choosing = None;
                if let Some(record) = self.record_mut(actor) {
                    record.status |= status::DEFENDING;
                }
                let against = self.fighter_slot(1 - side).map_or(0, |slot| 1 << slot);
                self.push_run(Run {
                    targets: against,
                    actors: 1 << actor,
                    before: handler,
                    camera: None,
                    ..Run::of_reaction(actor, reaction::DEFENDS, (0, 0), false)
                });
                self.director = took::TO_THE_HUB;
                return Ok(());
            }
            Plan::Resolve { command, target } => (command, target),
        };

        // Resolution comes first and all at once (`0x800A6064`).
        let (Some(acting), Some(hit)) = (self.record(actor).cloned(), self.record(target).cloned())
        else {
            return Err(Refusal::NotThatMinion);
        };
        let order = Order {
            command,
            actor: &acting,
            actor_side: Side::Creature,
            target: &hit,
            target_side: Side::Creature,
            deva: self.deva,
            lock: NO_DEVA,
            odd_frame,
        };
        // Nothing has changed before the resolver: a row it does not have is refused.
        let mut draws = self.random;
        let mut result = resolve(tables, &order, &mut draws).ok_or(Refusal::NoSuchRow)?;
        self.random = draws;
        self.acts += 1;
        self.events.push(Event::Chose { side, choice });
        self.choosing = None;
        let cut = |change: i32, has: i16, highest: i16| {
            change.clamp(i32::from(has) - i32::from(highest), has.into())
        };
        result.hp = cut(result.hp, hit.hp, hit.max_hp);
        result.mp = cut(result.mp, hit.mp, hit.max_mp);
        let own = match target != actor {
            true => (result.actor_hp, result.actor_mp),
            false => (0, 0),
        };
        let cost = match result.cost {
            0 => 0,
            cost => cost.min(acting.mp.into()),
        };
        let guards = hit.status & status::DEFENDING != 0;
        let mut shown = match result.outcome {
            outcome::HIT | outcome::MP_HIT | outcome::HP_AND_MP
                if result.hp > 0 || result.mp > 0 =>
            {
                reaction::HURT
            }
            outcome::CRITICAL => reaction::HURT_BADLY,
            outcome::MISS if !guards => reaction::DODGES,
            _ => reaction::NONE,
        };
        let dies = result.hp >= i32::from(hit.hp) || result.add & (LAST_STONE | status::DEAD) != 0;
        if dies {
            result.hp = hit.hp.into();
            shown = reaction::FALLS;
        }
        let decided = [(target, result, shown)];

        // The run of the command for the picture.
        let mut effect_bytes = [0; super::fight::SLOTS];
        effect_bytes[target] = result.effects;
        let given: Vec<Given> = (result.clear != 0 || result.add != 0)
            .then_some(Given {
                slot: target,
                clear: result.clear,
                add: result.add,
            })
            .into_iter()
            .collect();
        let numbers: Vec<Counted> = counted(actor, own, &decided);
        let run = self.runs.len();
        self.push_run(Run {
            kind: choice.kind(),
            parameter: match command {
                Command::Spell(number) | Command::Item(number) | Command::Body(number) => {
                    number as u32
                }
                _ => 0,
            },
            actors: 1 << actor,
            targets: 1 << target,
            reactions: (shown != reaction::NONE)
                .then_some(Shown {
                    slot: target,
                    kind: shown.into(),
                    parameter: 0,
                })
                .into_iter()
                .collect(),
            own: (0, cost),
            counted: numbers,
            given,
            ticked: Vec::new(),
            shakes: shaken(actor, own, &decided),
            flashes: flashed(&decided),
            effect_bytes,
            player_fell: false,
            before: handler,
            camera: Some(Pick::Plain),
            sounds: Sounds {
                hit: heard(&decided),
                fall: (shown == reaction::FALLS).then_some(Fall::Sound),
                start: None,
            },
        });
        self.director = took::TO_THE_HUB;

        // The actor starts: its cost, and a piece of the item is spent. Then the impact
        // on the target: status cleared, status added, the change; then the actor's own.
        self.change(actor, 0, cost);
        if let Command::Item(number) = command {
            for entry in self.items[side]
                .iter_mut()
                .filter(|entry| entry.0 == number)
            {
                entry.1 = entry.1.saturating_sub(1);
            }
            self.items[side].retain(|entry| entry.1 != 0);
            self.events.push(Event::Used { side, item: number });
        }
        self.events.push(Event::Resolved {
            actor,
            target,
            result,
            reaction: shown,
        });
        if let Some(record) = self.record_mut(target) {
            record.status = record.status & !result.clear | result.add;
        }
        self.change(target, result.hp, result.mp);
        if shown == reaction::FALLS {
            self.fell(target);
        }
        self.deva = result.deva;
        self.change(actor, own.0, own.1);

        // The second runner (`0x800BC378`): a target that was really hit, lives, and got
        // one of the bits or defends shows the status it stands with.
        let hit_really = !matches!(result.outcome, outcome::NOTHING | outcome::MISS | 6);
        let live = self.record(target).filter(|record| record.hp > 0);
        let asks = hit_really
            && live.is_some_and(|record| {
                result.add & SHOWN_WHEN_GIVEN != 0 || record.status & status::DEFENDING != 0
            });
        if asks {
            let standing = self.record(target).map_or(0, |record| record.status);
            let mut second = Run {
                kind: reaction::NONE.into(),
                actors: 0,
                targets: 1 << target,
                before: took::TO_THE_SECOND,
                camera: None,
                ..Run::of_reaction(actor, reaction::NONE, (0, 0), false)
            };
            let shows = [
                (LAST_STONE, reaction::STONE_DEATH),
                (UNABLE, reaction::CANNOT_ACT),
                (status::SLEEP, reaction::SLEEPS),
                (status::DEFENDING, reaction::DEFENDS),
            ]
            .into_iter()
            .find(|(bit, _)| standing & bit != 0);
            if let (Some((_, reaction)), true) = (shows, self.stands(target)) {
                self.events.push(Event::Shows {
                    slot: target,
                    reaction,
                });
                second.reactions.push(Shown {
                    slot: target,
                    kind: reaction.into(),
                    parameter: 0,
                });
            }
            self.push_run(second);
            self.director = took::STRAIGHT_TO_THE_HUB;
        }
        if let Some(run) = self.runs.get_mut(run) {
            run.player_fell = false;
        }
        Ok(())
    }

    /// A hash of everything the duel goes on from: the generator's word, the gauges,
    /// who fights, the six records' HP, MP and status, the items left, the deva, the
    /// counts, who chooses and the outcome. FNV-1a over little-endian bytes, the same
    /// on every machine; two peers compare it with every command.
    pub fn state_hash(&self) -> u64 {
        let mut hash = Hash::new();
        hash.word(self.seed);
        hash.word(self.random.seed);
        for gauge in self.gauges {
            hash.half(gauge as u16);
        }
        for fighter in self.fighter {
            hash.byte(fighter.map_or(0xFF, |index| index as u8));
        }
        for record in self.teams.iter().flatten() {
            match record {
                Some(record) => {
                    hash.byte(1);
                    hash.word(record.species);
                    hash.byte(record.level);
                    for half in [record.hp, record.max_hp, record.mp, record.max_mp] {
                        hash.half(half as u16);
                    }
                    hash.word(record.status);
                }
                None => hash.byte(0),
            }
        }
        for list in &self.items {
            hash.word(list.len() as u32);
            for &(number, pieces) in list {
                hash.word(number as u32);
                hash.word(pieces);
            }
        }
        hash.half(self.deva as u16);
        hash.word(self.turn);
        hash.word(self.acts);
        hash.byte(self.choosing.map_or(0xFF, |side| side as u8));
        match self.outcome {
            Some((winner, gave_up)) => {
                hash.byte(winner as u8);
                hash.byte(u8::from(gave_up));
            }
            None => hash.byte(0xFF),
        }
        hash.0
    }
}

pub fn for_a_minion(flags: u16) -> bool {
    flags & item::IN_A_FIGHT != 0
        && flags & item::ON_A_MINION != 0
        && flags & item::AT_THE_OPPONENTS == 0
}

/// FNV-1a, 64 bits.
#[derive(Debug, Clone, Copy)]
pub struct Hash(pub u64);

impl Hash {
    pub fn new() -> Self {
        Self(0xCBF2_9CE4_8422_2325)
    }

    pub fn byte(&mut self, byte: u8) {
        self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(0x0100_0000_01B3);
    }

    pub fn bytes(&mut self, bytes: &[u8]) {
        bytes.iter().for_each(|byte| self.byte(*byte));
    }

    pub fn half(&mut self, half: u16) {
        self.bytes(&half.to_le_bytes());
    }

    pub fn word(&mut self, word: u32) {
        self.bytes(&word.to_le_bytes());
    }
}

impl Default for Hash {
    fn default() -> Self {
        Self::new()
    }
}

/// The slot of the other side's view: side 0's minions become the opponents and side
/// 1's the carried creatures. Slot 0 and others stay.
pub fn mirrored(slot: usize) -> usize {
    match place_of(slot) {
        Some((side, index)) => self::slot(1 - side, index),
        None => slot,
    }
}

/// A run as the player of side 1 sees it, his own minion where the story's carried
/// creature stands.
pub fn mirror(run: &Run) -> Run {
    let mask = |mask: u8| {
        (0..8)
            .filter(|slot| mask >> slot & 1 != 0)
            .fold(0, |turned, slot| turned | 1 << mirrored(slot))
    };
    let mut effect_bytes = run.effect_bytes;
    for (slot, byte) in run.effect_bytes.iter().enumerate() {
        effect_bytes[mirrored(slot)] = *byte;
    }
    Run {
        actors: mask(run.actors),
        targets: mask(run.targets),
        reactions: (run.reactions.iter())
            .map(|shown| Shown {
                slot: mirrored(shown.slot),
                ..*shown
            })
            .collect(),
        counted: (run.counted.iter())
            .map(|counted| Counted {
                slot: mirrored(counted.slot),
                ..*counted
            })
            .collect(),
        given: (run.given.iter())
            .map(|given| Given {
                slot: mirrored(given.slot),
                ..*given
            })
            .collect(),
        ticked: (run.ticked.iter())
            .map(|&(slot, word)| (mirrored(slot), word))
            .collect(),
        shakes: mask(run.shakes),
        flashes: mask(run.flashes),
        effect_bytes,
        ..run.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::super::record::tests::{head, tables as made_up};
    use super::super::{Descriptor, Item, Move, Spell, class};
    use super::*;

    /// The made-up tables with a plain attack, three spells (the first for the own
    /// side, a heal), a body attack and two items: a liquor for a minion and one that
    /// is thrown at the opponent.
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
        let heal = [0, 0, 0xD8, 0xFF, 0xFF, 125, 125, 0, class::NONE, 2, 0, 0];
        let heal = Descriptor::parse(&heal).expect("a descriptor");
        let spell = Spell {
            word: 0x80,
            cost: 3,
            shown: 0,
            need: 0,
            element: 0,
            effect: attack(115, 100),
        };
        tables.spells = vec![
            Spell {
                word: 0x40,
                effect: heal,
                ..spell
            },
            spell,
            spell,
        ];
        tables.body_attacks = vec![attack(115, 100)];
        let liquor = [0, 0, 0xCE, 0xFF, 0xFF, 125, 125, 0, class::NONE, 2, 0, 2];
        tables.items = vec![
            Item {
                flags: 0x53,
                effect: Descriptor::parse(&liquor).expect("a descriptor"),
            },
            Item {
                flags: 0x82,
                effect: attack(100, 50),
            },
        ];
        tables
    }

    /// A minion of a level with some speed.
    fn minion(speed: u8, hp: i16) -> Record {
        let mut record = head(&"00".repeat(72));
        (record.hp, record.max_hp, record.mp, record.max_mp) = (hp, hp, 20, 20);
        record.stats = [30, 20, 30, 20, speed, 0];
        record.modifiers = [32; 12];
        record.strengths = [8; 4];
        record.level = 10;
        record.species = 4;
        record.equipment = [0xFF; 3];
        record.spells = 0b11;
        record.moves[0] = Move {
            known: true,
            attack: 0,
            grade: 0,
            flag: 0,
        };
        record
    }

    fn duel(seed: u32, options: Options) -> Duel {
        let teams = [
            vec![minion(40, 120), minion(30, 90)],
            vec![minion(40, 100), minion(20, 80), minion(10, 60)],
        ];
        let items = [vec![(0, 2), (1, 3)], vec![(0, 1)]];
        Duel::new(&tables(), teams, items, options, seed)
    }

    /// Plays a duel to its end with commands picked by a second generator; answers the
    /// commands taken.
    fn play(duel: &mut Duel, picks: u32) -> Vec<(usize, Choice)> {
        let tables = tables();
        let mut picker = Random { seed: picks };
        let mut log = Vec::new();
        for _ in 0..2000 {
            match duel.next(&tables) {
                Step::Over { .. } => return log,
                Step::Choose { side, .. } => {
                    let legal = duel.legal(&tables, side);
                    let choice = legal[picker.draw() as usize % legal.len()];
                    duel.act(&tables, side, choice).expect("a legal choice");
                    log.push((side, choice));
                }
            }
        }
        let side = duel.choosing.expect("somebody chooses");
        duel.act(&tables, side, Choice::GiveUp).expect("giving up");
        log.push((side, Choice::GiveUp));
        log
    }

    #[test]
    fn the_same_teams_seed_and_commands_make_the_same_duel() {
        let mut first = duel(77, Options::default());
        let log = play(&mut first, 5);
        let mut second = duel(77, Options::default());
        assert_eq!(play(&mut second, 5), log);
        assert_eq!(first.events, second.events);
        assert_eq!(first.runs, second.runs);
        assert_eq!(first.state_hash(), second.state_hash());
        assert!(first.outcome().is_some());

        // A replay from the log of commands alone reaches the same end and hash, and
        // the hash agrees after every command.
        let tables = tables();
        let mut replay = duel(77, Options::default());
        let mut hashes = Vec::new();
        for &(side, choice) in &log {
            assert_eq!(
                replay.next(&tables),
                Step::Choose {
                    side,
                    turn: replay.turn()
                }
            );
            replay.act(&tables, side, choice).expect("the same choice");
            hashes.push(replay.state_hash());
        }
        assert!(matches!(replay.next(&tables), Step::Over { .. }));
        assert_eq!(replay.state_hash(), first.state_hash());
        assert_eq!(replay.events, first.events);
        // Another seed makes another duel.
        let mut other = duel(78, Options::default());
        play(&mut other, 5);
        assert_ne!(other.state_hash(), first.state_hash());
        // Every hash of the replay differs from the one before it.
        assert!(hashes.windows(2).all(|pair| pair[0] != pair[1]));
    }

    #[test]
    fn the_dice_are_rolled_again_on_a_draw_and_the_winner_gets_the_head_start() {
        let mut seen_a_draw = false;
        for seed in 0..200 {
            let duel = duel(seed, Options::default());
            let rolls = &duel.start.rolls;
            let last = rolls.last().expect("a roll");
            assert_ne!(last[0], last[1]);
            assert!(
                rolls[..rolls.len() - 1]
                    .iter()
                    .all(|roll| roll[0] == roll[1])
            );
            assert!(rolls.iter().flatten().all(|die| (1..=6).contains(die)));
            assert_eq!(duel.start.winner, usize::from(last[1] > last[0]));
            seen_a_draw |= rolls.len() > 1;
            // The dice are the first two draws of the generator, and so on.
            let mut random = Random { seed };
            for roll in rolls {
                let dice = [0; 2].map(|_| (random.draw() % 6 + 1) as u8);
                assert_eq!(&dice, roll);
            }
        }
        assert!(seen_a_draw);
        // Equal speeds are said; the winner's gauge gets 100 in front of the first
        // pass.
        let tables = tables();
        for seed in [3, 5, 8, 13] {
            let mut duel = duel(seed, Options::default());
            assert!(duel.start.equally_fast);
            let winner = duel.start.winner;
            // Both step 50: the head start decides who acts first, in the 18th pass,
            // and side 1 steps once more behind its turn.
            assert_eq!(duel.race(&tables), winner);
            assert_eq!(duel.gauges[winner], [0, 50][winner]);
            assert_eq!(duel.gauges[1 - winner], [850, 900][winner]);
        }
    }

    #[test]
    fn the_race_keeps_the_overshoot_and_gives_side_one_its_extra_step() {
        let tables = tables();
        let mut duel = duel(1, Options::default());
        // Side 0 steps 47, side 1 steps 33.
        (duel.teams[0][0].as_mut().expect("a minion")).stats[stat::SPEED] = 37;
        (duel.teams[1][0].as_mut().expect("a minion")).stats[stat::SPEED] = 23;
        duel.gauges = [990, 980];
        // Side 0 is full first: 1037 less 1000, and side 1 gets no step in that pass.
        assert_eq!(duel.race(&tables), 0);
        assert_eq!(duel.gauges, [37, 980]);
        // Next pass side 0 goes to 84, side 1 to 1013: it acts, keeps 13 and steps
        // once more.
        assert_eq!(duel.race(&tables), 1);
        assert_eq!(duel.gauges, [84, 13 + 33]);
        // A tie in the same pass goes to side 0.
        duel.gauges = [953, 967];
        assert_eq!(duel.race(&tables), 0);
        assert_eq!(duel.gauges, [0, 967]);
    }

    #[test]
    fn a_change_brings_the_other_minion_in_with_a_drawn_gauge() {
        let tables = tables();
        let mut duel = duel(9, Options::default());
        let Step::Choose { side, .. } = duel.next(&tables) else {
            panic!("a choice");
        };
        let mut random = duel.random;
        let gauge = 1000 - (random.draw() % 200) as i16;
        duel.act(&tables, side, Choice::Change { index: 1 })
            .expect("a change");
        assert_eq!(duel.fighter[side], Some(1));
        assert_eq!(duel.gauges[side], gauge);
        assert!((801..=1000).contains(&gauge));
        assert_eq!(
            duel.runs.iter().map(|run| run.kind).collect::<Vec<_>>(),
            [kind::LEAVE, kind::SUMMON]
        );
        // The one that fights cannot come in again, and without the option nobody
        // changes.
        let Step::Choose { side, .. } = duel.next(&tables) else {
            panic!("a choice");
        };
        let fighting = duel.fighter[side].expect("a fighter");
        assert_eq!(
            duel.check(&tables, side, Choice::Change { index: fighting }),
            Err(Refusal::NotThatMinion)
        );
        let mut closed = self::duel(
            9,
            Options {
                changing: false,
                ..Options::default()
            },
        );
        let Step::Choose { side, .. } = closed.next(&tables) else {
            panic!("a choice");
        };
        assert_eq!(
            closed.check(&tables, side, Choice::Change { index: 1 }),
            Err(Refusal::NotAllowed)
        );
        assert!(
            closed
                .legal(&tables, side)
                .iter()
                .all(|choice| !matches!(choice, Choice::Change { .. }))
        );
    }

    #[test]
    fn items_are_used_up_and_only_those_for_a_minion_count() {
        let tables = tables();
        let mut duel = duel(4, Options::default());
        let Step::Choose { side, .. } = duel.next(&tables) else {
            panic!("a choice");
        };
        // Nobody is hurt yet: the liquor would do nothing.
        assert_eq!(
            duel.check(&tables, side, Choice::Item { item: 0, index: 0 }),
            Err(Refusal::NoUse)
        );
        // The item thrown at the opponent is not one of the duel's.
        if side == 0 {
            assert_eq!(
                duel.check(&tables, 0, Choice::Item { item: 1, index: 0 }),
                Err(Refusal::NoPiece)
            );
        }
        assert!(!for_a_minion(0x82) && for_a_minion(0x53) && !for_a_minion(0x4B));
        // A hurt minion takes it; a piece is spent, and the last piece empties the list.
        let index = duel.fighter[side].expect("a fighter");
        (duel.teams[side][index].as_mut().expect("a minion")).hp = 10;
        let pieces = |duel: &Duel| duel.items[side].iter().find(|e| e.0 == 0).map(|e| e.1);
        let had = pieces(&duel).expect("liquors");
        duel.act(&tables, side, Choice::Item { item: 0, index })
            .expect("the liquor");
        assert_eq!(pieces(&duel), (had > 1).then_some(had - 1));
        assert!(duel.fighting(side).expect("a fighter").hp > 10);
        assert!(duel.events.contains(&Event::Used { side, item: 0 }));
        // Without the option nobody has items.
        let closed = self::duel(
            4,
            Options {
                items: false,
                ..Options::default()
            },
        );
        assert!(closed.items.iter().all(Vec::is_empty));
    }

    #[test]
    fn giving_up_loses_and_the_next_minion_comes_in_after_a_fall() {
        let tables = tables();
        let mut duel = duel(12, Options::default());
        let Step::Choose { side, .. } = duel.next(&tables) else {
            panic!("a choice");
        };
        duel.act(&tables, side, Choice::GiveUp).expect("giving up");
        assert_eq!(
            duel.next(&tables),
            Step::Over {
                winner: 1 - side,
                gave_up: true
            }
        );
        assert_eq!(
            duel.act(&tables, 1 - side, Choice::Attack),
            Err(Refusal::Over)
        );

        // A fighter at 1 HP falls to the first hit, and the next one in team order
        // comes in by itself with its side's gauge as it stood.
        let mut duel = self::duel(12, Options::default());
        (duel.teams[1][0].as_mut().expect("a minion")).hp = 1;
        for _ in 0..200 {
            let Step::Choose { side, .. } = duel.next(&tables) else {
                panic!("the duel goes on");
            };
            if side == 1 {
                duel.act(&tables, 1, Choice::Defend).expect("a defence");
                continue;
            }
            duel.act(&tables, 0, Choice::Attack).expect("an attack");
            if duel.teams[1][0].as_ref().is_some_and(Record::is_dead) {
                break;
            }
        }
        assert!(duel.teams[1][0].as_ref().is_some_and(Record::is_dead));
        let Step::Choose { .. } = duel.next(&tables) else {
            panic!("the duel goes on");
        };
        assert_eq!(duel.fighter[1], Some(1));
        assert!(duel.events.contains(&Event::Fell { slot: slot(1, 0) }));
        assert!(duel.events.contains(&Event::Entered { slot: slot(1, 1) }));
        // A wrong side is refused.
        let Step::Choose { side, .. } = duel.next(&tables) else {
            panic!("the duel goes on");
        };
        assert_eq!(
            duel.act(&tables, 1 - side, Choice::Attack),
            Err(Refusal::NotYourTurn)
        );
    }

    #[test]
    fn a_side_without_a_living_minion_loses() {
        let tables = tables();
        let mut duel = duel(2, Options::default());
        for record in duel.teams[1].iter_mut().flatten() {
            record.status |= status::DEAD;
        }
        assert_eq!(
            duel.next(&tables),
            Step::Over {
                winner: 0,
                gave_up: false
            }
        );
    }

    #[test]
    fn a_mirrored_run_swaps_the_sides() {
        assert_eq!((slot(0, 0), slot(1, 2)), (1, 6));
        assert_eq!((mirrored(1), mirrored(6), mirrored(0)), (4, 3, 0));
        let tables = tables();
        let mut duel = duel(21, Options::default());
        while duel.runs.is_empty() {
            let Step::Choose { side, .. } = duel.next(&tables) else {
                panic!("the duel goes on");
            };
            duel.act(&tables, side, Choice::Attack).expect("an attack");
        }
        let run = &duel.runs[0];
        let back = mirror(&mirror(run));
        assert_eq!(&back, run);
        assert_eq!(mirror(run).actors.count_ones(), run.actors.count_ones());
    }

    #[test]
    fn a_side_s_menu_sees_its_own_minions_first_and_its_commands_come_back() {
        let tables = tables();
        let mut duel = duel(5, Options::default());
        let Step::Choose { side, .. } = duel.next(&tables) else {
            panic!("the duel goes on");
        };
        let fight = duel.menu_fight(&tables, side).expect("a fighter");
        let other = 1 - side;
        assert!(fight.records[PLAYER].is_none());
        assert_eq!(fight.fighter, 1 + duel.fighter[side].unwrap());
        assert_eq!(fight.records[1], duel.teams[side][0]);
        assert_eq!(fight.records[OPPONENTS], duel.teams[other][0]);
        let standing = 1 << (OPPONENTS + duel.fighter[other].unwrap());
        assert_eq!(fight.opponents(), standing);
        assert_eq!(fight.items, duel.items[side]);
        let chosen = |kind, parameter, targets, picked| Chosen {
            kind,
            parameter,
            actors: 0,
            targets,
            picked,
        };
        let of = |kind, parameter, targets, picked| {
            Choice::of_menu(&chosen(kind, parameter, targets, picked))
        };
        assert_eq!(of(0, 0, 1 << 4, Some(4)), Some(Choice::Attack));
        assert_eq!(of(18, 0, standing, None), Some(Choice::Defend));
        assert_eq!(of(6, 3, 1 << 4, Some(4)), Some(Choice::Body { slot: 3 }));
        let spell = |own| Some(Choice::Spell { spell: 2, own });
        assert_eq!(of(1, 2, 1 << 1, Some(1)), spell(true));
        assert_eq!(of(1, 2, 1 << 4, Some(4)), spell(false));
        assert_eq!(of(8, 0, 0, Some(2)), Some(Choice::Change { index: 1 }));
        let item = Some(Choice::Item { item: 9, index: 2 });
        assert_eq!(of(4, 9, 1 << 3, Some(3)), item);
        // A flight and a capture are no commands of a minion.
        assert_eq!(of(7, 0, standing, None), None);
        assert_eq!(of(5, 0, 1 << 4, Some(4)), None);
    }
}
