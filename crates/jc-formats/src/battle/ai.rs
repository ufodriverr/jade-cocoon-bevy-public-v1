
use super::fight::{Choice, Fight, OPPONENTS, Refusal};
use super::record::{Draws, Record, stat, status};
use super::resolve::NO_DEVA;
use super::{EARTH, FIRE, Tables, WATER, WIND};

/// The spells the routines name.
mod spell {
    pub const AGNI: usize = 0;
    /// The fixed types 2 and 3 order it without a test.
    pub const OF_THE_MINION: usize = 2;
    pub const MEDINA: usize = 13;
    pub const POISON: usize = 17;
    pub const SLEEP: usize = 18;
    pub const STONE: usize = 19;
    pub const DEATH: usize = 20;
    /// The call of the deva that beats the deva of each element: fire, wind, earth,
    /// water.
    pub const AGAINST: [usize; 4] = [22, 21, 24, 23];
    /// The last of the spells the common routine tries in order.
    pub const ELEMENTAL: usize = 12;
}

/// What an opponent sees when it decides.
struct Mind<'a> {
    tables: &'a Tables,
    fight: &'a Fight,
    me: &'a Record,
    foe: &'a Record,
    /// The mask of the one target.
    target: u8,
}

impl Mind<'_> {
    /// No command is written: the reaction of the turn's start, or nothing, is played
    /// once more and the turn is lost.
    fn left(&self) -> Choice {
        Choice::Left {
            kind: self.fight.began,
        }
    }

    /// Kind 0 on the target: always possible.
    fn attack(&self) -> Choice {
        Choice::Attack {
            targets: self.target,
        }
    }

    /// The first move slot that is known and that the MP pays.
    fn body(&self) -> Option<Choice> {
        let pays = |entry: &super::Move| entry.known && entry.cost() <= self.me.mp.into();
        let slot = self.me.moves.iter().position(pays)?;
        Some(Choice::Body {
            slot,
            targets: self.target,
        })
    }

    /// A spell the record knows and the MP pays, on `targets`.
    fn spell_on(&self, spell: usize, targets: u8) -> Option<Choice> {
        let known = spell < 32 && self.me.spells >> spell & 1 != 0;
        let cost = self.tables.spells.get(spell)?.cost;
        (known && i16::from(cost) <= self.me.mp).then_some(Choice::Spell { spell, targets })
    }

    fn spell(&self, spell: usize) -> Option<Choice> {
        self.spell_on(spell, self.target)
    }

    /// The first opponent on the field, itself among them, whose HP is not above a
    /// quarter of its highest gets the spell.
    fn heal(&self, spell: usize) -> Option<Choice> {
        let weak = |slot: &usize| {
            let record = self.fight.records[*slot].as_ref();
            self.fight.stands(*slot) && record.is_some_and(|r| r.hp <= r.max_hp / 4)
        };
        let friend = (OPPONENTS..OPPONENTS + 3).find(weak)?;
        self.spell_on(spell, 1 << friend)
    }

    fn foe_has(&self, bits: u32) -> bool {
        self.foe.status & bits != 0
    }

    /// The call of the deva that beats the deva of `element`, while that deva or none
    /// is on the field.
    fn against(&self, element: usize) -> Option<Choice> {
        let deva = self.fight.deva;
        let called = deva == element as i16 || deva == NO_DEVA;
        called
            .then(|| self.spell(spell::AGAINST[element]))
            .flatten()
    }

    /// Species 10: sleep, and the body attack when that cannot be paid, four times in
    /// ten on a target that is awake. The counter of the fixed types counts the tries
    /// that could not be paid, and from the second on the species only attacks.
    fn sleeper(&self, counter: &mut u8, draws: &mut impl Draws) -> Option<Choice> {
        let tries = draws.draw() % 100 < 40 && !self.foe_has(status::SLEEP);
        if *counter >= 2 || !tries {
            return None;
        }
        let chosen = self.spell(spell::SLEEP).or_else(|| {
            *counter += 1;
            self.body()
        });
        if chosen.is_none() {
            *counter += 1;
        }
        chosen
    }

    /// Type 0: by the species. `None` for a routine that is not read, and for the one
    /// way of species 27 that writes no command.
    fn by_species(&self, counter: &mut u8, draws: &mut impl Draws) -> Option<Choice> {
        let (hp, highest) = (self.me.hp, self.me.max_hp);
        let species = self.me.species;
        let chosen = match species {
            // "Marrdreg": the body attack when it is weak.
            0 | 1 if hp < highest / 4 => self.body(),
            0 | 1 => None,
            // A call of the deva first, each species its own.
            2 => self.against(WIND).or_else(|| self.body()),
            22 => self.against(FIRE).or_else(|| self.body()),
            25 => self.against(WATER).or_else(|| self.body()),
            29 => self.against(EARTH).or_else(|| self.body()),
            // A spell for a target whose magic defence is the lower one.
            3 if self.foe.stats[stat::MAGIC_DEFENCE] < self.foe.stats[stat::DEFENCE] => {
                self.spell(7).or_else(|| self.body())
            }
            3 | 4 | 8 | 19 | 31 => self.body(),
            5 => self.spell(11).or_else(|| self.body()),
            // "Mukhambu": heals a friend, or defends half the time when it is hurt.
            6 => self
                .heal(spell::MEDINA)
                .or_else(|| (hp < highest && draws.draw() & 1 != 0).then_some(Choice::Defend)),
            7 | 20 if in_ten(7, draws) => self.body(),
            7 | 20 => None,
            9 if draws.draw() % 100 < 50 => self.spell(9),
            9 => None,
            10 => self.sleeper(counter, draws),
            // It runs away when it cannot pay its spell; with it, the spell half the
            // time, else the body attack on a target that has MP left.
            11 => {
                let Some(spell) = self.spell(8) else {
                    return Some(Choice::Flee);
                };
                if odd(draws) {
                    Some(spell)
                } else {
                    (self.foe.mp != 0).then(|| self.body()).flatten()
                }
            }
            // "Pataimel": Agni three times in ten.
            12 if draws.draw() % 100 < 30 => self.spell(spell::AGNI),
            12 => None,
            13 if hp <= highest / 2 => self.body(),
            13 => None,
            14 if !self.foe_has(status::STONE) => self.body(),
            14 => None,
            // "Patalchu": below a third the body attack, else defending or attacking.
            15 | 17 if hp < highest / 3 => self
                .body()
                .or_else(|| (draws.draw() & 1 != 0).then_some(Choice::Defend)),
            15 | 17 => None,
            // Poison seven times in ten for a target without it, else the body attack
            // half the time.
            16 => {
                let poisons = !self.foe_has(status::POISON) && in_ten(7, draws);
                let curse = poisons.then(|| self.spell(spell::POISON)).flatten();
                curse.or_else(|| odd(draws).then(|| self.body()).flatten())
            }
            // "Nushab": its body attack poisons.
            18 if !self.foe_has(status::POISON) => self.body(),
            18 => None,
            21 | 28 if in_ten(6, draws) => self.spell(if species == 21 { 2 } else { 5 }),
            21 | 28 => None,
            // The deva call, a spell six times in ten, and when it is weak the flight
            // half the time; it never attacks.
            23 => {
                let spell = self
                    .against(WATER)
                    .or_else(|| in_ten(6, draws).then(|| self.spell(6)).flatten());
                return Some(spell.unwrap_or_else(|| {
                    if hp <= highest / 3 && odd(draws) {
                        Choice::Flee
                    } else {
                        Choice::Defend
                    }
                }));
            }
            // Stone, or it defends.
            24 => {
                let stone = (!self.foe_has(status::STONE)).then(|| self.spell(spell::STONE));
                return Some(stone.flatten().unwrap_or(Choice::Defend));
            }
            26 => self.spell(4),
            // On an odd draw the body attack and nothing else: with no slot it can
            // pay, what the start of its turn left in the command record stands.
            27 if odd(draws) => return Some(self.body().unwrap_or(self.left())),
            27 => None,
            30 => self.spell(spell::DEATH),
            // Species 189 to 192 run a routine first whose command the common routine
            // overwrites: all it leaves is one draw.
            189..=192 => {
                draws.draw();
                Some(self.common(draws))
            }
            32..=52 | 57..=200 | 209 | 210 => Some(self.common(draws)),
            201..=208 => self.boss(draws),
            _ => return None,
        };
        Some(chosen.unwrap_or_else(|| self.attack()))
    }

    /// Species 201 to 208: commands written without a test of what the record knows or
    /// can pay. `None` is the plain attack.
    fn boss(&self, draws: &mut impl Draws) -> Option<Choice> {
        let targets = self.target;
        let strikes = |slot: usize| Some(Choice::Body { slot, targets });
        let casts = |spell: usize| Some(Choice::Spell { spell, targets });
        match self.me.species {
            // Its first body attack for a target below a tenth of its HP.
            201 if self.foe.hp < self.foe.max_hp / 10 => strikes(0),
            201 | 206 if odd(draws) => strikes(1),
            201 => None,
            202 if odd(draws) => casts(8),
            202 => casts(6),
            203 if odd(draws) => strikes(2),
            203 => None,
            204 => casts(spell::DEATH),
            205 if draws.draw().is_multiple_of(3) => None,
            205 => strikes(3),
            206 => strikes(2),
            // A friend healed, else it defends half the time.
            207 => self
                .heal(spell::MEDINA)
                .or_else(|| odd(draws).then_some(Choice::Defend)),
            // The body attack that gives the status the target lacks, seven times in
            // ten each: stone, sleep, poison.
            _ => [status::STONE, status::SLEEP, status::POISON]
                .iter()
                .position(|bits| !self.foe_has(*bits) && in_ten(7, draws))
                .and_then(strikes),
        }
    }

    /// The routine of species 32 to 52 and of every element variant: the first try
    /// that succeeds is the command.
    fn common(&self, draws: &mut impl Draws) -> Choice {
        // 1. The deva call that beats the deva on the field, or any when there is none.
        if let Some(call) = (FIRE..=WATER).find_map(|element| self.against(element)) {
            return call;
        }
        // 2. to 4. A status the target does not have yet, seven times in ten.
        let statuses = [
            (status::STONE, spell::STONE),
            (status::SLEEP, spell::SLEEP),
            (status::POISON, spell::POISON),
        ];
        for (bits, spell) in statuses {
            if !self.foe_has(bits)
                && draws.draw() % 10 < 7
                && let Some(curse) = self.spell(spell)
            {
                return curse;
            }
        }
        // 5. Death half the time, and 6. a friend healed.
        if draws.draw() & 1 != 0
            && let Some(death) = self.spell(spell::DEATH)
        {
            return death;
        }
        if let Some(heals) = self.heal(spell::MEDINA) {
            return heals;
        }
        // 7. The body for the one whose defence is the higher, the spells for the other.
        if self.me.stats[stat::MAGIC_DEFENCE] < self.me.stats[stat::DEFENCE] {
            let body = (draws.draw() % 10 < 8).then(|| self.body()).flatten();
            return body.unwrap_or_else(|| self.attack());
        }
        let spell = (0..spell::ELEMENTAL).find_map(|spell| self.spell(spell));
        spell.unwrap_or_else(|| self.attack())
    }
}

/// "p in ten": one draw.
fn in_ten(p: u32, draws: &mut impl Draws) -> bool {
    draws.draw() % 10 < p
}

/// One draw, odd.
fn odd(draws: &mut impl Draws) -> bool {
    draws.draw() & 1 != 0
}

/// The creature of type 14 takes a form: the element that beats the deva on the field.
/// The body attacks from this number on call the deva of fire, wind, earth and water.
const CALLS_THE_DEVA: u8 = 34;
/// The defensive modifiers of a form, in 32nds: of its four elements, and of the one
/// that beats it. The master's own are all the third.
const FORM_TAKES: (i8, i8, i8) = (12, 24, 48);
/// The body attack of the forms of fire, wind and earth in slot 1, and their spell.
const OF_THE_FORM: [(u8, usize); 3] = [(4, 25), (5, 28), (2, 27)];
/// The spell the form of water casts on itself.
const ON_ITSELF: usize = 29;

/// The element whose form the creature of type 14 takes now, if it changes: under the
/// deva of an element the one that beats it, when it is not in that deva's own form or
/// has counted to 3 (to 4 under the deva of water); with no deva the form of earth.
fn new_form(deva: i16, element_bit: u16, counter: u8) -> Option<usize> {
    let Ok(deva) = usize::try_from(deva) else {
        return Some(EARTH);
    };
    let waited = if deva == WATER { 4 } else { 3 };
    let changes = deva <= WATER && (element_bit & 1 << deva == 0 || counter >= waited);
    changes.then_some((deva + 3) % 4)
}

impl Fight {
    /// Type 14 (`0x800C63CC`): the master sends his creature in, and on a later turn
    /// he loses his element and strikes with slot 1. The creature takes the form that
    /// beats the deva on the field and calls its own deva with slot 2; between the
    /// changes it strikes or casts by its form. Every call ends with one draw that
    /// counts 1 or 2 on. A creature without an element bit of the four that does not
    /// change writes nothing ([`Choice::Left`]); no way leads there.
    fn shifter(&mut self, slot: usize, draws: &mut impl Draws) -> Option<Choice> {
        let targets = 1 << self.fighter;
        let deva = self.deva;
        let it = self.records[slot].as_mut()?;
        let body = |slot: usize| Some(Choice::Body { slot, targets });
        let cast = |spell: usize| Some(Choice::Spell { spell, targets });
        let (takes, weak, masters) = FORM_TAKES;
        let choice = if slot == OPPONENTS {
            if self.sent {
                it.element_bit = 0;
                it.modifiers[..4].fill(masters);
                body(1)
            } else {
                self.sent = true;
                Some(Choice::Swap)
            }
        } else if let Some(form) = new_form(deva, it.element_bit, self.counter) {
            it.moves[2].attack = CALLS_THE_DEVA + form as u8;
            it.element_bit = 1 << form;
            it.modifiers[..4].fill(takes);
            it.modifiers[(form + 3) % 4] = weak;
            self.counter = 0;
            body(2)
        } else if it.element_bit & 1 << WATER != 0 {
            if self.counter == 1 {
                Some(Choice::Spell {
                    spell: ON_ITSELF,
                    targets: 1 << slot,
                })
            } else {
                body(0)
            }
        } else if let Some(form) = (FIRE..=EARTH).find(|form| it.element_bit & 1 << form != 0) {
            let (attack, spell) = OF_THE_FORM[form];
            // Fire strikes on an odd draw, wind on one of three, earth on two of three.
            let draw = draws.draw();
            let strikes = match form {
                FIRE => draw & 1 != 0,
                WIND => draw.is_multiple_of(3),
                _ => !draw.is_multiple_of(3),
            };
            if strikes {
                it.moves[1].attack = attack;
                body(1)
            } else {
                cast(spell)
            }
        } else {
            Some(Choice::Left { kind: self.began })
        };
        let counts = ((draws.draw() & 3) >> 1) as u8 + 1;
        self.counter = self.counter.wrapping_add(counts);
        choice
    }

    /// The command of the opponent that was asked to choose, by its AI type. `None`
    /// when no opponent chooses, or for a species or a type without a routine.
    pub fn decide(&mut self, tables: &Tables, draws: &mut impl Draws) -> Option<Choice> {
        let slot = self.choosing.filter(|slot| *slot >= OPPONENTS)?;
        // In a lesson the step's command stands in place of the decision (`0x800B4478`
        // of the gates' package).
        if let Some(step) = self.taught() {
            return step.choice();
        }
        let n = slot - OPPONENTS;
        if self.ai[n] == 14 {
            return self.shifter(slot, draws);
        }
        let mind = Mind {
            tables,
            fight: self,
            me: self.records[slot].as_ref()?,
            foe: self.records[self.fighter].as_ref()?,
            target: 1 << self.fighter,
        };
        let body = |slot: usize| Choice::Body {
            slot,
            targets: mind.target,
        };
        let cast = |spell: usize| Choice::Spell {
            spell,
            targets: mind.target,
        };
        // A body attack of the creature on itself.
        let on_itself = Choice::Body {
            slot: 0,
            targets: 1 << slot,
        };
        let (mut sent, mut counter, mut charged) = (self.sent, self.counter, self.charged);
        let choice = match (self.ai[n], n) {
            (0, _) => mind.by_species(&mut counter, draws)?,
            // The master sends his creature in on his first turn and attacks on every
            // later one. The types 11 and 13 count his turns too.
            (2 | 3 | 11..=13 | 15..=17, 0) => {
                if matches!(self.ai[n], 11 | 13) {
                    counter = counter.wrapping_add(1);
                }
                if sent {
                    mind.attack()
                } else {
                    sent = true;
                    Choice::Swap
                }
            }
            // The creatures of the types 11 to 13 order what they neither know nor can
            // pay, by a draw of three. Type 11: its spell, or the body attack on a
            // target that has MP left.
            (11, _) => {
                counter = counter.wrapping_add(1);
                match draws.draw() % 3 {
                    0 if mind.foe.mp != 0 => body(1),
                    0 | 1 => cast(25),
                    _ => body(3),
                }
            }
            // Type 12: once the target sleeps, a body attack on itself.
            (12, _) if counter != 0 && mind.foe_has(status::SLEEP) => {
                counter = 0;
                on_itself
            }
            (12, _) => {
                counter = counter.wrapping_add(1);
                match draws.draw() % 3 {
                    0 => cast(28),
                    1 => body(1),
                    _ => body(3),
                }
            }
            // Type 13: a body attack on itself makes it ready for the strike of its
            // next turn.
            (13, _) if charged => {
                counter = counter.wrapping_add(1);
                charged = false;
                body(2)
            }
            (13, _) => {
                counter = counter.wrapping_add(1);
                match draws.draw() % 3 {
                    0 => cast(27),
                    1 => body(1),
                    _ => {
                        charged = true;
                        on_itself
                    }
                }
            }
            // The creatures of three later masters. The body attack on an odd draw,
            // against a target of the wind, or below a third of its HP.
            (15, 1) => {
                let strikes = odd(draws)
                    || mind.foe.element_bit & 1 << WIND != 0
                    || mind.me.hp < mind.me.max_hp / 3;
                let body = strikes.then(|| mind.body()).flatten();
                body.unwrap_or_else(|| mind.attack())
            }
            // The body attack below a quarter, else its spell.
            (16, 1) => {
                let weak = mind.me.hp < mind.me.max_hp / 4;
                let body = weak.then(|| mind.body()).flatten();
                let chosen = body.or_else(|| mind.spell(9));
                chosen.unwrap_or_else(|| mind.attack())
            }
            // The body attack below a half, else its spell half the time.
            (17, 1) => {
                let weak = mind.me.hp < mind.me.max_hp / 2;
                let body = weak.then(|| mind.body()).flatten();
                let chosen = body.or_else(|| odd(draws).then(|| mind.spell(6)).flatten());
                chosen.unwrap_or_else(|| mind.attack())
            }
            (2 | 3, 1) => {
                counter = counter.wrapping_add(1);
                match self.counter % 3 {
                    0 => mind.attack(),
                    1 => body(0),
                    _ => Choice::Spell {
                        spell: spell::OF_THE_MINION,
                        targets: mind.target,
                    },
                }
            }
            // A later boss: its body attacks 3, 2, 1 in turn, on the same counter.
            (4, _) => {
                counter = counter.wrapping_add(1);
                body(3 - usize::from(self.counter % 3))
            }
            (7..=9, _) => body(0),
            // The Masked Boy.
            (10, _) => {
                counter = counter.wrapping_add(1);
                match self.counter % 6 {
                    0 => body(2),
                    2 => body(3),
                    4 => body(1),
                    _ => mind.attack(),
                }
            }
            // An opponent 2 of the types 15 to 17 writes nothing. Type 14 was answered
            // above.
            (15..=17, _) => mind.left(),
            (1 | 5 | 6 | 18.., _) => Choice::Defend,
            _ => return None,
        };
        (self.sent, self.counter, self.charged) = (sent, counter, charged);
        Some(choice)
    }

    /// The opponent that was asked decides and acts: [`Fight::decide`], then
    /// [`Fight::act`]. [`Refusal::NotBuilt`] when its routine is not read.
    pub fn opponent_acts(
        &mut self,
        tables: &Tables,
        odd_frame: bool,
        draws: &mut impl Draws,
    ) -> Result<(), Refusal> {
        let choice = self.decide(tables, draws).ok_or(Refusal::NotBuilt)?;
        self.act(tables, choice, odd_frame, draws)
    }
}
