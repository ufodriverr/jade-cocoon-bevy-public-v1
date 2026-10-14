
use crate::battle::record::Draws;
use crate::command::quotient;
use crate::mover::{ArcTangent, Mover, flag};
use crate::rooms::{self, FIELD_ENGINE_BASE, Image, RoomTableError};

/// RAM address of the table of routines in file 27: two words for each species.
pub const ROUTINES: u32 = 0x800A_E088;
/// The species with an entry, and the first element variant.
pub const SPECIES: usize = 57;
/// How many levels the player must be above a creature for the second word.
pub const OUTGROWN_BY: i32 = 5;

/// The states the routines of the Beetle Forest use.
pub mod state {
    /// Nothing yet: left in the pass that finds it.
    pub const NONE: i32 = -1;
    pub const STANDS: i32 = 1;
    pub const FLEES: i32 = 5;
    pub const WALKS: i32 = 8;
    pub const HOLDS: i32 = 10;
    pub const RESTS: i32 = 13;
    pub const TURNS: i32 = 14;
    pub const COMES: i32 = 15;
    pub const WAITS: i32 = 16;
}

/// The actions a state plays.
pub mod plays {
    pub const STANDING: i32 = 1;
    pub const WALKING: i32 = 2;
    /// What the species 3 to 5 play between seeing the player and coming.
    pub const HOLDING: i32 = 4;
    pub const WAITING: i32 = 18;
    pub const RESTING: i32 = 23;
}

/// The values of the count that the items 41 and 42 start (`0x80011B6C`).
pub mod item {
    /// Item 41: every creature of the room rests.
    pub const REST: i32 = 0;
    /// Item 42: every creature of the room comes to the player.
    pub const CALL: i32 = 1;
    /// The value while neither works.
    pub const NONE: i32 = -1;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemCount {
    value: i32,
    left: i32,
}

impl Default for ItemCount {
    fn default() -> Self {
        Self {
            value: item::NONE,
            left: -1,
        }
    }
}

impl ItemCount {
    /// The passes the setter gives an item.
    pub const LASTS: i32 = 1000;
    /// The two items the setter knows.
    pub const RESTS: u8 = 41;
    pub const CALLS: u8 = 42;

    /// The setter `0x80011B24(item)`, called for every item that is used without a
    /// target: the two items start the count with their value, any other does nothing.
    pub fn start(&mut self, item: u8) {
        let value = match item {
            Self::RESTS => item::REST,
            Self::CALLS => item::CALL,
            _ => return,
        };
        (self.value, self.left) = (value, Self::LASTS);
    }

    /// One pass of the task `0x80011AF8`: it takes one from a count above 0; a pass
    /// that finds none left clears the count and makes the value -1.
    pub fn pass(&mut self) {
        if self.left > 0 {
            self.left -= 1;
        } else {
            (self.value, self.left) = (item::NONE, 0);
        }
    }

    /// The getter `0x80011B6C`.
    pub fn value(&self) -> i32 {
        self.value
    }

    pub fn left(&self) -> i32 {
        self.left
    }
}

/// Squared flat distances to the player: near is at most the first, far more than the
/// second, and the species 0 to 2 give up past the third.
pub const NEAR: u32 = 899_999;
pub const FAR: u32 = 1_400_000;
pub const FARTHER: u32 = 1_900_000;
/// How near another actor must be ahead to be gone round.
const AHEAD: i32 = 400;
/// The turn steps a game frame, and the angles a creature turns away by.
const SLOW: i32 = 32;
const TURN: i32 = 64;
const FAST: i32 = 128;
const QUICK: i32 = 341;
const ROUND_AN_ACTOR: i32 = 481;
const ALONG_AN_EDGE: i32 = 256;
const BACK: i32 = 1861;
/// The speeds of a creature that comes or flees, and of species 6 in flight.
const RUNS: i32 = 8192;
const BOLTS: i32 = 12288;
/// The divisor of a turn on the spot: up to half a circle, or up to a whole one.
const HALF: i32 = 2;
const WHOLE: i32 = 1;
/// The start of an action: its first frame, or its loop start.
pub const FROM_THE_START: i32 = -1;
pub const FROM_THE_LOOP: i32 = -2;

/// The routines of the Beetle Forest's species.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Routine {
    /// `0x800A2F48`: walks about, runs at a player who is near.
    Most,
    /// `0x800A32CC`: rests until the player runs near it.
    Rests,
    /// `0x800A316C`: watches the player and runs away when he is near.
    Watches,
    /// `0x800A3E18`: outgrown by the player, it waits and runs away.
    Outgrown,
    /// `0x800A3468`: walks about as most do and never comes (species 13 and 26).
    Wanders,
    /// `0x800A35FC`: the same with a turn of up to a whole circle (species 27).
    Wheels,
    /// `0x800A3794`: stands until the player is near, holds 40 game frames and comes
    /// (species 3 to 5).
    Lurks,
    /// `0x800A3F50`: walks 33 game frames, stands, turns and walks again, and never
    /// comes (species 7 and 8).
    Paces,
    /// `0x800A3CB4`: the same with a walk of 20 game frames (species 30).
    PacesShort,
    /// `0x800A3A1C`: walks for good (species 14).
    Marches,
    /// `0x800A38E4`: always comes to the player, and near him stands turned to him
    /// (species 16).
    Follows,
    /// `0x800A3B7C`: stands, and comes only while the player has his back to it
    /// (species 23).
    Stalks,
    /// `0x800A2E7C`: always runs from the player (species 33).
    Bolts,
}

impl Routine {
    /// The routine at an address of file 27, when it is one of those built.
    pub fn at(address: u32) -> Option<Self> {
        Some(match address {
            0x800A_2F48 => Self::Most,
            0x800A_32CC => Self::Rests,
            0x800A_316C => Self::Watches,
            0x800A_3E18 => Self::Outgrown,
            0x800A_3468 => Self::Wanders,
            0x800A_35FC => Self::Wheels,
            0x800A_3794 => Self::Lurks,
            0x800A_3F50 => Self::Paces,
            0x800A_3CB4 => Self::PacesShort,
            0x800A_3A1C => Self::Marches,
            0x800A_38E4 => Self::Follows,
            0x800A_3B7C => Self::Stalks,
            0x800A_2E7C => Self::Bolts,
            _ => return None,
        })
    }
}

/// The table `0x800AE088`: for each species the routine's address, and the one for a
/// player who has outgrown the creature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Routines(Vec<[u32; 2]>);

impl Routines {
    /// Reads the table from the field engine file as it is on the disc.
    pub fn from_field_engine(file: &[u8]) -> Result<Self, RoomTableError> {
        let image = Image {
            data: file,
            base: FIELD_ENGINE_BASE,
        };
        (0..SPECIES)
            .map(|species| {
                let at = rooms::step(ROUTINES, species, 8)?;
                Ok([image.u32(at)?, image.u32(at + 4)?])
            })
            .collect::<Result<_, _>>()
            .map(Self)
    }

    /// A table of these pairs.
    pub fn of(pairs: Vec<[u32; 2]>) -> Self {
        Self(pairs)
    }

    /// The address the maker `0x800A2D20` picks for a roaming creature: the first word
    /// of its species, the second when the player's level is five or more above the
    /// creature's; an element variant takes the first word of its base species
    /// whatever the levels.
    pub fn address(&self, species: u32, level: u8, player_level: u8) -> Option<u32> {
        let species = species as usize;
        if species >= SPECIES {
            return Some(self.0.get((species - SPECIES) / 4)?[0]);
        }
        let outgrown = i32::from(player_level) - i32::from(level) >= OUTGROWN_BY;
        Some(self.0.get(species)?[usize::from(outgrown)])
    }
}

/// What a pass asks of the room.
#[derive(Debug, Clone, Copy)]
pub struct Senses<'a> {
    pub arc: &'a ArcTangent,
    /// The game's counter, for the turn filter.
    pub counter: u32,
    /// The player's place (`0x8008BCA8`) and his angle (+14 of that record).
    pub player: [i32; 3],
    pub player_turn: i16,
    /// The movement of his command word in this game frame is "run"
    /// (`player::command::RUN`).
    pub player_runs: bool,
    /// The value of the items' count ([`item`]).
    pub item: i32,
    /// The places of the other actors of the room that count, without the player.
    pub others: &'a [[i32; 3]],
}

/// An action a pass starts on the actor (`0x8001D7C8(actor, action, start, fade)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Start {
    pub action: i32,
    /// [`FROM_THE_START`] or [`FROM_THE_LOOP`].
    pub start: i32,
    /// Fades from the action before.
    pub fade: bool,
}

/// The record of a behaviour task, as far as its routines use it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Roamer {
    /// +12.
    pub state: i32,
    /// +32: the action the state plays, which a task that is let go on starts again.
    pub action: i32,
    /// +36: a count of game frames.
    pub count: i32,
    /// +40 and +44: the side of a turn (1 or 2) and the angle left of it.
    pub side: i32,
    pub left: i32,
}

impl Default for Roamer {
    fn default() -> Self {
        Self::new()
    }
}

/// What the helpers of a pass work with.
struct Pass<'a, D: Draws> {
    mover: &'a mut Mover,
    senses: &'a Senses<'a>,
    draws: &'a mut D,
    starts: Vec<Start>,
}

impl<D: Draws> Pass<'_, D> {
    /// The squared flat distance from the actor to the player (`0x8003B864`).
    fn apart(&self) -> u32 {
        let [x, _, z] = self.mover.position;
        let (x, z) = (
            x.wrapping_sub(self.senses.player[0]),
            z.wrapping_sub(self.senses.player[2]),
        );
        x.wrapping_mul(x).wrapping_add(z.wrapping_mul(z)) as u32
    }

    fn near(&self) -> bool {
        self.apart() <= NEAR
    }

    /// The player has his back to the creature (`0x800A4814` with the player first):
    /// his heading is within a quarter circle of the direction from the creature to
    /// him.
    fn unseen(&self) -> bool {
        let [x, _, z] = self.mover.position;
        let (x, z) = (
            self.senses.player[0].wrapping_sub(x),
            self.senses.player[2].wrapping_sub(z),
        );
        let to_him = i32::from(self.senses.arc.angle(x, z));
        within_a_quarter(to_him.wrapping_sub(i32::from(self.senses.player_turn)))
    }

    /// The point as far behind the creature as the player is in front of it
    /// (`0x800A4748`).
    fn behind(&self) -> [i32; 3] {
        let place = self.mover.position;
        std::array::from_fn(|axis| {
            (place[axis].wrapping_mul(2)).wrapping_sub(self.senses.player[axis])
        })
    }

    fn turn_by(&mut self, side: i32, left: i32, step: i32) -> i32 {
        (self.mover).turn_by(side, left, step, self.senses.counter)
    }

    fn turn_to(&mut self, place: [i32; 3], step: i32) -> bool {
        (self.mover).turn_to(self.senses.arc, place, step, self.senses.counter)
    }

    fn ahead(&self) -> i32 {
        (self.mover).ahead(self.senses.arc, self.senses.others, AHEAD)
    }
}

/// The answer of `0x800A4814` for the difference of its two angles: the game reads the
/// sign of its sine table at the difference plus 1024, which is not negative from 0 to
/// 2048, so within a quarter circle to either side.
fn within_a_quarter(turned: i32) -> bool {
    matches!(turned.wrapping_add(1024) & 0xFFF, 0..=2048)
}

/// The step of the turn away by three times the angle from the heading to the player
/// (`0x800A4884`).
fn pace(angle: i32) -> i32 {
    match angle {
        i32::MIN..=256 => 16,
        257..=512 => 32,
        513..=1024 => 64,
        1025..=2048 => 128,
        _ => 256,
    }
}

impl Roamer {
    /// The record as the maker `0x800A2D20` leaves it.
    pub fn new() -> Self {
        Self {
            state: state::NONE,
            action: -1,
            count: 0,
            side: 0,
            left: 0,
        }
    }

    /// One pass of a routine. The answer is what it starts on the actor, in order.
    pub fn pass(
        &mut self,
        routine: Routine,
        mover: &mut Mover,
        senses: &Senses,
        draws: &mut impl Draws,
    ) -> Vec<Start> {
        let mut pass = Pass {
            mover,
            senses,
            draws,
            starts: Vec::new(),
        };
        if !self.item(&mut pass) {
            match routine {
                Routine::Most => self.most(&mut pass),
                Routine::Rests => self.rests(&mut pass),
                Routine::Watches => self.watches(&mut pass),
                Routine::Outgrown => self.outgrown(&mut pass),
                Routine::Wanders => self.wanders(&mut pass, HALF),
                Routine::Wheels => self.wanders(&mut pass, WHOLE),
                Routine::Lurks => self.lurks(&mut pass),
                Routine::Paces => self.paces(&mut pass, 33),
                Routine::PacesShort => self.paces(&mut pass, 20),
                Routine::Marches => self.marches(&mut pass),
                Routine::Follows => self.follows(&mut pass),
                Routine::Stalks => self.stalks(&mut pass),
                Routine::Bolts => self.bolts(&mut pass),
            }
        }
        pass.starts
    }

    fn set<D: Draws>(
        &mut self,
        pass: &mut Pass<D>,
        (state, action, count): (i32, i32, i32),
        (start, fade): (i32, bool),
        divisor: i32,
    ) {
        (self.state, self.action, self.count) = (state, action, count);
        pass.starts.push(Start {
            action,
            start,
            fade,
        });
        if state == state::TURNS {
            let drawn = (pass.draws.draw() % 16) as i32;
            self.left = quotient(quotient(4096, drawn), divisor);
            self.side = if pass.draws.draw() & 1 != 0 { 1 } else { 2 };
        }
    }

    /// A state that starts its action from the first frame with a fade.
    fn go<D: Draws>(&mut self, pass: &mut Pass<D>, state: i32, action: i32, count: i32) {
        self.set(pass, (state, action, count), (FROM_THE_START, true), HALF);
    }

    /// The item's test (`0x800A4208`), the first call of every routine: true when an
    /// item holds the creature, and the routine does nothing else in this pass.
    fn item<D: Draws>(&mut self, pass: &mut Pass<D>) -> bool {
        match pass.senses.item {
            item::REST => {
                if self.state != state::RESTS {
                    self.go(pass, state::RESTS, plays::RESTING, 0);
                }
                true
            }
            item::CALL => {
                if self.state != state::WALKS {
                    self.go(pass, state::WALKS, plays::WALKING, 0);
                }
                self.towards_player(pass);
                true
            }
            _ => false,
        }
    }

    /// A turn on the spot (`0x800A4400`): by 32 a game frame while an angle is left.
    fn turn_on_the_spot<D: Draws>(&mut self, pass: &mut Pass<D>) -> bool {
        if self.left == 0 {
            return false;
        }
        self.left = pass.turn_by(self.side, self.left, SLOW);
        pass.mover.flags |= flag::NO_STEP;
        true
    }

    /// What is in the way, when nobody is ahead: the answer of the walk over the
    /// ground of the game frame before. Stopped: about, to side 2.
    fn ground<D: Draws>(&mut self, pass: &Pass<D>) -> i32 {
        let answer = pass.mover.answers[0];
        self.side = answer;
        if answer == 3 {
            (self.side, self.left) = (2, BACK);
        }
        answer
    }

    /// Round what is in the way (`0x800A468C`).
    fn round<D: Draws>(&mut self, pass: &mut Pass<D>) {
        if self.left != 0 {
            self.left = pass.turn_by(self.side, self.left, SLOW);
            return;
        }
        self.side = pass.ahead();
        match self.side {
            3 => (self.side, self.left) = (2, BACK),
            1 | 2 => self.left = ROUND_AN_ACTOR,
            _ => {
                if let 1 | 2 = self.ground(pass) {
                    self.left = ALONG_AN_EDGE;
                }
            }
        }
    }

    /// To the player (`0x800A4588`).
    fn towards_player<D: Draws>(&mut self, pass: &mut Pass<D>) {
        if self.left != 0 {
            self.left = pass.turn_by(self.side, self.left, FAST);
        }
        self.side = pass.ahead();
        match self.side {
            3 => (self.side, self.left) = (2, BACK),
            0 => {
                self.ground(pass);
            }
            _ => {}
        }
        if self.side != 0 {
            pass.turn_by(self.side, 2048, TURN);
            return;
        }
        // `0x800A4814`: faster while its heading is within a quarter circle of the
        // direction from the player to it.
        let [x, _, z] = pass.mover.position;
        let (x, z) = (
            x.wrapping_sub(pass.senses.player[0]),
            z.wrapping_sub(pass.senses.player[2]),
        );
        let from_him = i32::from(pass.senses.arc.angle(x, z));
        let turned = from_him.wrapping_sub(i32::from(pass.mover.turn));
        let step = match within_a_quarter(turned) {
            true => FAST,
            false => TURN,
        };
        pass.turn_to(pass.senses.player, step);
    }

    /// Away from the player (`0x800A452C`).
    fn away<D: Draws>(&mut self, pass: &mut Pass<D>) {
        if !pass.turn_to(pass.behind(), QUICK) {
            pass.mover.flags |= flag::NO_STEP;
        }
    }

    /// Away from the player and round what is in the way (`0x800A4450`): slow while
    /// the creature still looks at him.
    fn away_and_round<D: Draws>(&mut self, pass: &mut Pass<D>) {
        self.round(pass);
        let [x, _, z] = pass.mover.position;
        let (x, z) = (
            pass.senses.player[0].wrapping_sub(x),
            pass.senses.player[2].wrapping_sub(z),
        );
        let to_him = i32::from(pass.senses.arc.angle(x, z));
        let turned = to_him.wrapping_sub(i32::from(pass.mover.turn)) & 0xFFF;
        if !pass.turn_to(pass.behind(), pace(3 * turned)) {
            pass.mover.flags |= flag::NO_STEP;
        }
    }

    /// 100 and ten times a draw of 16: the game frames a creature walks or stands.
    fn while_<D: Draws>(pass: &mut Pass<D>) -> i32 {
        100 + 10 * (pass.draws.draw() % 16) as i32
    }

    /// `0x800A2F48`.
    fn most<D: Draws>(&mut self, pass: &mut Pass<D>) {
        if pass.near() && self.state != state::COMES {
            self.go(pass, state::COMES, plays::WALKING, 0);
            pass.mover.set_speed(RUNS, true);
        }
        if self.state == state::COMES {
            self.towards_player(pass);
            if pass.apart() > FAR {
                self.state = state::NONE;
                pass.mover.set_speed(4096, true);
            }
        } else {
            self.wanders(pass, HALF);
        }
    }

    /// `0x800A3468` with the divisor 2 and `0x800A35FC` with the divisor 1, and the
    /// four states `0x800A2F48` has beside its coming: it walks, stands, turns on the
    /// spot by the turn the divisor picks, and walks again.
    fn wanders<D: Draws>(&mut self, pass: &mut Pass<D>, divisor: i32) {
        match self.state {
            state::NONE => {
                let count = Self::while_(pass);
                self.go(pass, state::WALKS, plays::WALKING, count);
            }
            state::WALKS => {
                self.round(pass);
                self.count -= 1;
                if self.count < 0 {
                    let count = Self::while_(pass);
                    self.go(pass, state::STANDS, plays::STANDING, count);
                }
            }
            state::STANDS => {
                self.count -= 1;
                if self.count < 0 {
                    let turn = (state::TURNS, plays::WALKING, 0);
                    self.set(pass, turn, (FROM_THE_START, true), divisor);
                }
            }
            state::TURNS => {
                if !self.turn_on_the_spot(pass) {
                    self.state = state::NONE;
                }
            }
            _ => {}
        }
    }

    /// `0x800A32CC`.
    fn rests<D: Draws>(&mut self, pass: &mut Pass<D>) {
        if self.state == state::NONE {
            // The routine goes on into the state it has just set.
            let rest = (state::RESTS, plays::RESTING, 0);
            self.set(pass, rest, (FROM_THE_LOOP, false), HALF);
        }
        match self.state {
            state::RESTS => {
                if pass.near() && pass.senses.player_runs {
                    self.go(pass, state::COMES, plays::WALKING, 0);
                }
            }
            state::COMES => {
                self.towards_player(pass);
                if pass.apart() > FARTHER {
                    self.go(pass, state::STANDS, plays::STANDING, 50);
                }
            }
            state::STANDS => {
                self.count -= 1;
                if self.count < 0 {
                    self.go(pass, state::RESTS, plays::RESTING, 0);
                }
            }
            _ => {}
        }
    }

    /// `0x800A316C`.
    fn watches<D: Draws>(&mut self, pass: &mut Pass<D>) {
        match self.state {
            state::NONE => {
                self.go(pass, state::STANDS, plays::STANDING, 0);
                pass.mover.set_speed(4096, false);
            }
            state::STANDS => {
                pass.turn_to(pass.senses.player, QUICK);
                if pass.near() {
                    self.go(pass, state::FLEES, plays::WALKING, 0);
                    pass.mover.set_speed(BOLTS, true);
                }
            }
            state::FLEES => {
                self.away(pass);
                if pass.apart() > FAR {
                    self.state = state::NONE;
                }
            }
            _ => {}
        }
    }

    /// `0x800A3E18`.
    fn outgrown<D: Draws>(&mut self, pass: &mut Pass<D>) {
        if self.state == state::NONE {
            // The routine goes on into the state it has just set.
            self.go(pass, state::WAITS, plays::WAITING, 0);
        }
        match self.state {
            state::WAITS => {
                if pass.near() {
                    self.go(pass, state::FLEES, plays::WALKING, 0);
                    pass.mover.set_speed(RUNS, true);
                }
            }
            state::FLEES => {
                self.away_and_round(pass);
                if pass.apart() > FAR {
                    self.state = state::NONE;
                }
            }
            _ => {}
        }
    }

    /// `0x800A3794`. Its speed is never changed.
    fn lurks<D: Draws>(&mut self, pass: &mut Pass<D>) {
        match self.state {
            state::NONE => self.go(pass, state::STANDS, plays::STANDING, 0),
            state::STANDS => {
                if pass.near() {
                    self.go(pass, state::HOLDS, plays::HOLDING, 40);
                }
            }
            state::HOLDS => {
                self.count -= 1;
                if self.count < 0 {
                    self.go(pass, state::COMES, plays::WALKING, 0);
                }
            }
            state::COMES => {
                self.towards_player(pass);
                if pass.apart() > FARTHER {
                    self.state = state::NONE;
                }
            }
            _ => {}
        }
    }

    /// `0x800A3F50` with a walk of 33 game frames and `0x800A3CB4` with one of 20.
    fn paces<D: Draws>(&mut self, pass: &mut Pass<D>, walk: i32) {
        match self.state {
            state::NONE => self.go(pass, state::WALKS, plays::WALKING, walk),
            state::WALKS => {
                self.round(pass);
                self.count -= 1;
                if self.count < 0 {
                    let count = 50 + 8 * (pass.draws.draw() % 16) as i32;
                    self.go(pass, state::STANDS, plays::STANDING, count);
                }
            }
            state::STANDS => {
                self.count -= 1;
                if self.count < 0 {
                    self.go(pass, state::TURNS, plays::WALKING, 0);
                    // `0x800A438C` picks the turn a second time, by two more draws:
                    // what the setter picked is written over.
                    let drawn = (pass.draws.draw() % 16) as i32;
                    self.left = quotient(quotient(4096, drawn), HALF);
                    self.side = if pass.draws.draw() & 1 != 0 { 1 } else { 2 };
                }
            }
            state::TURNS => {
                if !self.turn_on_the_spot(pass) {
                    self.state = state::NONE;
                }
            }
            _ => {}
        }
    }

    /// `0x800A3A1C`: the test `0x800A4794` behind the walk always answers 0, so the
    /// states 1 and 14 the routine has behind it are never reached.
    fn marches<D: Draws>(&mut self, pass: &mut Pass<D>) {
        match self.state {
            state::NONE => self.go(pass, state::WALKS, plays::WALKING, 0),
            state::WALKS => {
                self.round(pass);
                // The test asks who is ahead and the ground once more and, for an
                // answer, writes the side and the COUNT (+36, where a turn's angle
                // would be +44): the side of a turn that is under way can change.
                let found = match pass.ahead() {
                    0 => pass.mover.answers[0],
                    ahead => ahead,
                };
                if found != 0 {
                    self.side = if found == 1 { 1 } else { 2 };
                    self.count = if found == 3 { BACK } else { ROUND_AN_ACTOR };
                }
            }
            _ => {}
        }
    }

    /// `0x800A38E4`. The count of 70 it sets with the hold is never counted, and its
    /// speed is never changed.
    fn follows<D: Draws>(&mut self, pass: &mut Pass<D>) {
        match self.state {
            state::NONE => self.go(pass, state::COMES, plays::WALKING, 0),
            state::COMES => {
                self.towards_player(pass);
                if pass.near() {
                    self.go(pass, state::HOLDS, plays::HOLDING, 70);
                }
            }
            state::HOLDS => {
                pass.turn_to(pass.senses.player, QUICK);
                if pass.apart() > NEAR + 1 {
                    self.state = state::NONE;
                }
            }
            _ => {}
        }
    }

    /// `0x800A3B7C`. No distance is asked, the count it sets with the stand is never
    /// counted, and its speed is never changed.
    fn stalks<D: Draws>(&mut self, pass: &mut Pass<D>) {
        match self.state {
            state::NONE => {
                let count = 50 + 8 * (pass.draws.draw() % 16) as i32;
                self.go(pass, state::STANDS, plays::STANDING, count);
            }
            state::STANDS => {
                if pass.unseen() {
                    self.go(pass, state::COMES, plays::WALKING, 0);
                }
            }
            state::COMES => {
                self.towards_player(pass);
                if !pass.unseen() {
                    self.state = state::NONE;
                }
            }
            _ => {}
        }
    }

    /// `0x800A2E7C`: the flight of species 6 without its watching.
    fn bolts<D: Draws>(&mut self, pass: &mut Pass<D>) {
        match self.state {
            state::NONE => {
                self.go(pass, state::FLEES, plays::WALKING, 0);
                pass.mover.set_speed(BOLTS, true);
            }
            state::FLEES => {
                self.away(pass);
                if pass.apart() > FAR {
                    self.state = state::NONE;
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::TAU;

    #[test]
    fn the_count_of_the_two_items_lasts_a_thousand_passes_and_one() {
        let mut count = ItemCount::default();
        assert_eq!((count.value(), count.left()), (item::NONE, -1));
        // The first pass of the game clears the count; any other item starts nothing.
        count.pass();
        count.start(0);
        assert_eq!((count.value(), count.left()), (item::NONE, 0));
        count.start(ItemCount::CALLS);
        assert_eq!((count.value(), count.left()), (item::CALL, 1000));
        (0..1000).for_each(|_| count.pass());
        assert_eq!((count.value(), count.left()), (item::CALL, 0));
        // The pass that finds nothing left ends it.
        count.pass();
        assert_eq!(count.value(), item::NONE);
        count.start(ItemCount::RESTS);
        assert_eq!(count.value(), item::REST);
    }

    /// A table like the game's, which the tests need only to be near the true angle.
    fn arc() -> ArcTangent {
        let step = |ratio: usize| ((ratio as f64 / 1024.0).atan() * 4096.0 / TAU).round() as i16;
        ArcTangent::of((0..1025).map(step).collect())
    }

    /// Draws handed in, in their order.
    fn draws(numbers: &[u32]) -> impl Draws + use<> {
        let mut numbers: std::collections::VecDeque<u32> = numbers.iter().copied().collect();
        move || numbers.pop_front().expect("a draw too many")
    }

    /// The room as a creature at the origin finds it: the player `far` along z.
    fn senses(arc: &ArcTangent, far: i32) -> Senses<'_> {
        Senses {
            arc,
            counter: 0,
            player: [0, 0, far],
            player_turn: 0,
            player_runs: false,
            item: item::NONE,
            others: &[],
        }
    }

    const FAR_OFF: i32 = 3000;
    const CLOSE: i32 = 900;

    fn started(action: i32) -> Vec<Start> {
        vec![Start {
            action,
            start: FROM_THE_START,
            fade: true,
        }]
    }

    #[test]
    fn the_maker_picks_the_routine_by_species_and_levels() {
        let pairs: Vec<[u32; 2]> = (0..SPECIES as u32)
            .map(|species| [0x800A_0000 + species, 0x800B_0000 + species])
            .collect();
        let routines = Routines::of(pairs);
        assert_eq!(routines.address(12, 1, 1), Some(0x800A_000C));
        assert_eq!(routines.address(12, 1, 5), Some(0x800A_000C));
        assert_eq!(routines.address(12, 1, 6), Some(0x800B_000C));
        assert_eq!(routines.address(12, 9, 6), Some(0x800A_000C));
        // A variant has the first word of its base species whatever the levels.
        assert_eq!(routines.address(57 + 4 * 12 + 1, 1, 60), Some(0x800A_000C));
        assert_eq!(routines.address(57 + 4 * 57, 1, 1), None);
        assert_eq!(Routine::at(0x800A_2F48), Some(Routine::Most));
        assert_eq!(Routine::at(0x800A_32CC), Some(Routine::Rests));
        assert_eq!(Routine::at(0x800A_316C), Some(Routine::Watches));
        assert_eq!(Routine::at(0x800A_3E18), Some(Routine::Outgrown));
        assert_eq!(Routine::at(0x800A_3468), Some(Routine::Wanders));
        assert_eq!(Routine::at(0x800A_35FC), Some(Routine::Wheels));
        assert_eq!(Routine::at(0x800A_3794), Some(Routine::Lurks));
        assert_eq!(Routine::at(0x800A_3F50), Some(Routine::Paces));
        assert_eq!(Routine::at(0x800A_3CB4), Some(Routine::PacesShort));
        assert_eq!(Routine::at(0x800A_3A1C), Some(Routine::Marches));
        assert_eq!(Routine::at(0x800A_38E4), Some(Routine::Follows));
        assert_eq!(Routine::at(0x800A_3B7C), Some(Routine::Stalks));
        assert_eq!(Routine::at(0x800A_2E7C), Some(Routine::Bolts));
        //  x800A40B4, the thirteenth routine, which the table does not name.
        assert_eq!(Routine::at(0x800A_40B4), None);
        assert!(Routines::from_field_engine(&[]).is_err());
    }

    #[test]
    fn the_species_3_to_5_stand_hold_and_come() {
        let arc = arc();
        let (far, near) = (senses(&arc, FAR_OFF), senses(&arc, CLOSE));
        let mut mover = Mover::new([0; 3], 0);
        let mut roamer = Roamer::new();
        let routine = Routine::Lurks;
        let starts = roamer.pass(routine, &mut mover, &far, &mut draws(&[]));
        assert_eq!(starts, started(plays::STANDING));
        // It stands for as long as the player is not near.
        for _ in 0..300 {
            assert!(
                roamer
                    .pass(routine, &mut mover, &far, &mut draws(&[]))
                    .is_empty()
            );
        }
        assert_eq!(roamer.state, state::STANDS);
        let starts = roamer.pass(routine, &mut mover, &near, &mut draws(&[]));
        assert_eq!(starts, started(plays::HOLDING));
        assert_eq!((roamer.state, roamer.count), (state::HOLDS, 40));
        // The count runs down to below 0 wherever he is; then it comes.
        for _ in 0..40 {
            assert!(
                roamer
                    .pass(routine, &mut mover, &far, &mut draws(&[]))
                    .is_empty()
            );
        }
        let starts = roamer.pass(routine, &mut mover, &far, &mut draws(&[]));
        assert_eq!(starts, started(plays::WALKING));
        assert_eq!(roamer.state, state::COMES);
        // It gives up past the farther distance only, and is nothing again.
        roamer.pass(routine, &mut mover, &near, &mut draws(&[]));
        assert_eq!(roamer.state, state::COMES);
        roamer.pass(routine, &mut mover, &far, &mut draws(&[]));
        assert_eq!(roamer.state, state::NONE);
    }

    #[test]
    fn the_species_7_8_and_30_pace_and_pick_their_turn_twice() {
        let arc = arc();
        let near = senses(&arc, CLOSE);
        for (routine, walk) in [(Routine::Paces, 33), (Routine::PacesShort, 20)] {
            let mut mover = Mover::new([0; 3], 0);
            let mut roamer = Roamer::new();
            let starts = roamer.pass(routine, &mut mover, &near, &mut draws(&[]));
            assert_eq!(starts, started(plays::WALKING));
            assert_eq!((roamer.state, roamer.count), (state::WALKS, walk));
            for _ in 0..walk {
                assert!(
                    roamer
                        .pass(routine, &mut mover, &near, &mut draws(&[]))
                        .is_empty()
                );
            }
            // It stands for 50 and eight times a draw of 16 game frames.
            let starts = roamer.pass(routine, &mut mover, &near, &mut draws(&[16 + 3]));
            assert_eq!(starts, started(plays::STANDING));
            assert_eq!((roamer.state, roamer.count), (state::STANDS, 74));
            for _ in 0..74 {
                roamer.pass(routine, &mut mover, &near, &mut draws(&[]));
            }
            // Four draws: the setter's turn is written over by the second pick.
            let starts = roamer.pass(routine, &mut mover, &near, &mut draws(&[1, 1, 4, 2]));
            assert_eq!(starts, started(plays::WALKING));
            assert_eq!(
                (roamer.state, roamer.left, roamer.side),
                (state::TURNS, 512, 2)
            );
            while roamer.state == state::TURNS {
                roamer.pass(routine, &mut mover, &near, &mut draws(&[]));
            }
            // The player was near all the while: it never comes.
            assert_eq!((roamer.state, roamer.left), (state::NONE, 0));
        }
    }

    #[test]
    fn species_14_walks_for_good() {
        let arc = arc();
        let near = senses(&arc, CLOSE);
        let mut mover = Mover::new([0; 3], 0);
        let mut roamer = Roamer::new();
        let routine = Routine::Marches;
        let starts = roamer.pass(routine, &mut mover, &near, &mut draws(&[]));
        assert_eq!(starts, started(plays::WALKING));
        assert_eq!((roamer.state, roamer.count), (state::WALKS, 0));
        for _ in 0..500 {
            assert!(
                roamer
                    .pass(routine, &mut mover, &near, &mut draws(&[]))
                    .is_empty()
            );
        }
        assert_eq!((roamer.state, roamer.count), (state::WALKS, 0));
        // The ground stops it: about, to side 2, and the test behind the walk writes
        // the angle into the count as well.
        mover.answers[0] = 3;
        roamer.pass(routine, &mut mover, &near, &mut draws(&[]));
        assert_eq!(
            (roamer.state, roamer.side, roamer.left, roamer.count),
            (state::WALKS, 2, BACK, BACK)
        );
    }

    #[test]
    fn most_species_walk_stand_turn_and_walk_again() {
        let arc = arc();
        let senses = senses(&arc, FAR_OFF);
        let mut mover = Mover::new([0; 3], 0);
        let mut roamer = Roamer::new();
        assert_eq!(roamer, Roamer::default());
        // Nothing yet: it walks for 100 and ten times a draw of 16 game frames.
        let starts = roamer.pass(Routine::Most, &mut mover, &senses, &mut draws(&[16 + 2]));
        assert_eq!(starts, started(plays::WALKING));
        assert_eq!((roamer.state, roamer.count), (state::WALKS, 120));
        // The count runs down to below 0; then it stands.
        for _ in 0..120 {
            let starts = roamer.pass(Routine::Most, &mut mover, &senses, &mut draws(&[]));
            assert!(starts.is_empty());
        }
        let starts = roamer.pass(Routine::Most, &mut mover, &senses, &mut draws(&[0]));
        assert_eq!(starts, started(plays::STANDING));
        assert_eq!((roamer.state, roamer.count), (state::STANDS, 100));
        for _ in 0..100 {
            roamer.pass(Routine::Most, &mut mover, &senses, &mut draws(&[]));
        }
        // Then a turn is picked: 4096 over the draw over 2, to side 1 for an odd draw.
        let starts = roamer.pass(Routine::Most, &mut mover, &senses, &mut draws(&[15, 3]));
        assert_eq!(starts, started(plays::WALKING));
        assert_eq!(
            (roamer.state, roamer.left, roamer.side),
            (state::TURNS, 136, 1)
        );
        // 32 a game frame, with no step, until none is left; then nothing again.
        let mut passes = 0;
        while roamer.state == state::TURNS {
            mover.flags = 0;
            roamer.pass(Routine::Most, &mut mover, &senses, &mut draws(&[]));
            passes += 1;
            assert_eq!(mover.flags == flag::NO_STEP, roamer.state == state::TURNS);
        }
        assert_eq!((passes, roamer.state, roamer.left), (6, state::NONE, 0));
        assert!(mover.turn > 0);
    }

    #[test]
    fn a_turn_is_picked_by_two_draws() {
        let arc = arc();
        let senses = senses(&arc, FAR_OFF);
        let picked = |numbers: &[u32]| {
            let mut roamer = Roamer {
                state: state::STANDS,
                ..Roamer::new()
            };
            roamer.pass(
                Routine::Most,
                &mut Mover::new([0; 3], 0),
                &senses,
                &mut draws(numbers),
            );
            (roamer.left, roamer.side)
        };
        assert_eq!(picked(&[1, 0]), (2048, 2));
        assert_eq!(picked(&[16 + 3, 5]), (682, 1));
        // A draw of 0: the division by zero answers -1, half of which is no turn.
        assert_eq!(picked(&[0, 0]), (0, 2));
    }

    #[test]
    fn the_species_26_and_27_walk_about_and_never_come() {
        let arc = arc();
        let near = senses(&arc, CLOSE);
        for (routine, turn) in [(Routine::Wanders, 2048), (Routine::Wheels, 4096)] {
            let mut mover = Mover::new([0; 3], 1024);
            let mut roamer = Roamer::new();
            // The player is near all the while, and nothing of it is asked.
            let starts = roamer.pass(routine, &mut mover, &near, &mut draws(&[3]));
            assert_eq!(starts, started(plays::WALKING));
            assert_eq!((roamer.state, roamer.count), (state::WALKS, 130));
            for _ in 0..130 {
                assert!(
                    roamer
                        .pass(routine, &mut mover, &near, &mut draws(&[]))
                        .is_empty()
                );
            }
            let starts = roamer.pass(routine, &mut mover, &near, &mut draws(&[1]));
            assert_eq!(starts, started(plays::STANDING));
            assert_eq!((roamer.state, roamer.count), (state::STANDS, 110));
            for _ in 0..110 {
                roamer.pass(routine, &mut mover, &near, &mut draws(&[]));
            }
            // The turn: 4096 over the draw, halved by the one routine alone.
            let starts = roamer.pass(routine, &mut mover, &near, &mut draws(&[1, 1]));
            assert_eq!(starts, started(plays::WALKING));
            assert_eq!(
                (roamer.state, roamer.left, roamer.side),
                (state::TURNS, turn, 1)
            );
            for _ in 0..turn / 32 {
                roamer.pass(routine, &mut mover, &near, &mut draws(&[]));
                assert_eq!(roamer.state, state::TURNS);
            }
            roamer.pass(routine, &mut mover, &near, &mut draws(&[]));
            assert_eq!((roamer.state, roamer.left), (state::NONE, 0));
            assert_eq!((mover.speed, mover.flags & flag::SPEED), (4096, 0));
        }
        // A draw of 0 with the divisor 1: a turn of -1, which one pass ends.
        let mut roamer = Roamer {
            state: state::STANDS,
            ..Roamer::new()
        };
        let mut mover = Mover::new([0; 3], 0);
        roamer.pass(Routine::Wheels, &mut mover, &near, &mut draws(&[0, 0]));
        assert_eq!((roamer.left, roamer.side), (-1, 2));
        roamer.pass(Routine::Wheels, &mut mover, &near, &mut draws(&[]));
        assert_eq!((roamer.state, roamer.left), (state::TURNS, 0));
    }

    #[test]
    fn most_species_run_at_a_player_who_is_near() {
        let arc = arc();
        let mut mover = Mover::new([0; 3], 1024);
        let mut roamer = Roamer::new();
        // Near in any state: it comes, at twice the speed, and turns to him at once.
        let near = senses(&arc, CLOSE);
        let starts = roamer.pass(Routine::Most, &mut mover, &near, &mut draws(&[]));
        assert_eq!(starts, started(plays::WALKING));
        assert_eq!(roamer.state, state::COMES);
        assert_eq!((mover.speed, mover.flags), (8192, flag::SPEED));
        assert!(mover.turn < 1024);
        // 948 is near, 949 is not; it follows until he is more than 1400000 away.
        let follows = senses(&arc, 1183);
        assert!(
            roamer
                .pass(Routine::Most, &mut mover, &follows, &mut draws(&[]))
                .is_empty()
        );
        assert_eq!(roamer.state, state::COMES);
        let lost = senses(&arc, 1184);
        roamer.pass(Routine::Most, &mut mover, &lost, &mut draws(&[]));
        assert_eq!(roamer.state, state::NONE);
        assert_eq!((mover.speed, mover.flags), (4096, flag::SPEED));
    }

    #[test]
    fn a_creature_goes_round_what_is_in_its_way() {
        let arc = arc();
        let far = senses(&arc, FAR_OFF);
        let walking = || Roamer {
            state: state::WALKS,
            count: 50,
            ..Roamer::new()
        };
        // Another actor ahead to the one side: a turn of 481 to that side.
        let others = [[100, 0, 300]];
        let crowded = Senses {
            others: &others,
            ..far
        };
        let (mut roamer, mut mover) = (walking(), Mover::new([0; 3], 0));
        roamer.pass(Routine::Most, &mut mover, &crowded, &mut draws(&[]));
        assert_eq!((roamer.side, roamer.left), (2, 481));
        // The turn is made 32 a game frame, and nothing else is asked meanwhile.
        roamer.pass(Routine::Most, &mut mover, &far, &mut draws(&[]));
        assert_eq!((roamer.side, roamer.left), (2, 449));
        assert_eq!(mover.flags, flag::NO_STEP);
        // On the other side, and behind or too far: nobody.
        let others = [[0, 0, -100], [0, 0, 500], [-100, 0, 300]];
        let crowded = Senses {
            others: &others,
            ..far
        };
        let (mut roamer, mut mover) = (walking(), Mover::new([0; 3], 0));
        roamer.pass(Routine::Most, &mut mover, &crowded, &mut draws(&[]));
        assert_eq!((roamer.side, roamer.left), (1, 481));
        // Nobody ahead: the walk's answer of the game frame before. Pushed along an
        // edge: 256 to that side; stopped: about, to side 2; free: straight on.
        for (answer, expected) in [(1, (1, 256)), (2, (2, 256)), (3, (2, 1861)), (0, (0, 0))] {
            let (mut roamer, mut mover) = (walking(), Mover::new([0; 3], 0));
            mover.answers = [answer, 3];
            roamer.pass(Routine::Most, &mut mover, &far, &mut draws(&[]));
            assert_eq!((roamer.side, roamer.left), expected, "answer {answer}");
        }
    }

    #[test]
    fn a_creature_that_comes_goes_round_too() {
        let arc = arc();
        let near = senses(&arc, CLOSE);
        let coming = || Roamer {
            state: state::COMES,
            ..Roamer::new()
        };
        // Somebody ahead: 64 to the side answered, and no turn to the player.
        let others = [[-100, 0, 300]];
        let crowded = Senses {
            others: &others,
            ..near
        };
        let (mut roamer, mut mover) = (coming(), Mover::new([0; 3], 0));
        roamer.pass(Routine::Most, &mut mover, &crowded, &mut draws(&[]));
        assert_eq!((roamer.side, roamer.left, mover.turn), (1, 0, 8));
        assert_eq!(mover.flags, flag::NO_STEP);
        // Stopped by the ground: about, to side 2, 128 a game frame from the next pass.
        let (mut roamer, mut mover) = (coming(), Mover::new([0; 3], 0));
        mover.answers = [3, 0];
        roamer.pass(Routine::Most, &mut mover, &near, &mut draws(&[]));
        assert_eq!((roamer.side, roamer.left, mover.turn), (2, 1861, -8));
        // Nothing in the way: to the player, by 64 while it looks his way and by 128
        // while its heading is within a quarter circle of the way from him.
        let turned = |turn: i16| {
            let (mut roamer, mut mover) = (coming(), Mover::new([0; 3], turn));
            roamer.pass(Routine::Most, &mut mover, &near, &mut draws(&[]));
            assert_eq!(mover.flags, 0);
            i32::from(mover.turn) - i32::from(turn)
        };
        assert_eq!(turned(1000), -8);
        assert_eq!(turned(1100), -16);
        assert_eq!(turned(-1100), 16);
    }

    #[test]
    fn species_23_comes_only_while_the_player_has_his_back_to_it() {
        let arc = arc();
        let mut mover = Mover::new([0; 3], 0);
        let mut roamer = Roamer::new();
        // The player stands along +z of the creature. Angle 0 looks along +z: his back
        // is to it. Angle 2048 looks at it.
        let looks_at = Senses {
            player_turn: 2048,
            ..senses(&arc, FAR_OFF)
        };
        let starts = roamer.pass(
            Routine::Stalks,
            &mut mover,
            &looks_at,
            &mut draws(&[16 + 3]),
        );
        assert_eq!(starts, started(plays::STANDING));
        assert_eq!((roamer.state, roamer.count), (state::STANDS, 50 + 8 * 3));
        // Watched it stands for good, and the count is not counted.
        for _ in 0..200 {
            let starts = roamer.pass(Routine::Stalks, &mut mover, &looks_at, &mut draws(&[]));
            assert!(starts.is_empty());
        }
        assert_eq!((roamer.state, roamer.count), (state::STANDS, 74));
        // He turns his back, however far he is: it comes, at the animation's speed.
        let back = senses(&arc, FAR_OFF);
        let starts = roamer.pass(Routine::Stalks, &mut mover, &back, &mut draws(&[]));
        assert_eq!(
            (starts, roamer.state),
            (started(plays::WALKING), state::COMES)
        );
        roamer.pass(Routine::Stalks, &mut mover, &back, &mut draws(&[]));
        assert_eq!((roamer.state, mover.speed), (state::COMES, 4096));
        // A quarter circle to either side still counts as his back.
        let side = Senses {
            player_turn: 1024,
            ..back
        };
        roamer.pass(Routine::Stalks, &mut mover, &side, &mut draws(&[]));
        assert_eq!(roamer.state, state::COMES);
        // He looks round: it is in no state, and stands again in the next pass.
        let round = Senses {
            player_turn: 1025,
            ..back
        };
        roamer.pass(Routine::Stalks, &mut mover, &round, &mut draws(&[]));
        assert_eq!(roamer.state, state::NONE);
        let starts = roamer.pass(Routine::Stalks, &mut mover, &round, &mut draws(&[0]));
        assert_eq!((starts, roamer.count), (started(plays::STANDING), 50));
    }

    #[test]
    fn species_16_comes_from_any_distance_and_holds_near_the_player() {
        let arc = arc();
        let mut mover = Mover::new([0; 3], 0);
        let mut roamer = Roamer::new();
        let far = senses(&arc, FAR_OFF);
        let starts = roamer.pass(Routine::Follows, &mut mover, &far, &mut draws(&[]));
        assert_eq!(
            (starts, roamer.state),
            (started(plays::WALKING), state::COMES)
        );
        roamer.pass(Routine::Follows, &mut mover, &far, &mut draws(&[]));
        assert_eq!(roamer.state, state::COMES);
        // Near is at most 899999: 948 away, not 949.
        roamer.pass(
            Routine::Follows,
            &mut mover,
            &senses(&arc, 949),
            &mut draws(&[]),
        );
        assert_eq!(roamer.state, state::COMES);
        let near = senses(&arc, 948);
        let starts = roamer.pass(Routine::Follows, &mut mover, &near, &mut draws(&[]));
        assert_eq!(starts, started(plays::HOLDING));
        assert_eq!((roamer.state, roamer.count), (state::HOLDS, 70));
        // The hold is not counted: it ends when he is more than 900000 away.
        for _ in 0..100 {
            roamer.pass(Routine::Follows, &mut mover, &near, &mut draws(&[]));
        }
        assert_eq!((roamer.state, roamer.count), (state::HOLDS, 70));
        roamer.pass(
            Routine::Follows,
            &mut mover,
            &senses(&arc, 949),
            &mut draws(&[]),
        );
        assert_eq!(roamer.state, state::NONE);
    }

    #[test]
    fn species_33_runs_from_the_player_whatever_the_distance() {
        let arc = arc();
        let mut mover = Mover::new([0; 3], 0);
        let mut roamer = Roamer::new();
        let far = senses(&arc, FAR_OFF);
        let starts = roamer.pass(Routine::Bolts, &mut mover, &far, &mut draws(&[]));
        assert_eq!(
            (starts, roamer.state),
            (started(plays::WALKING), state::FLEES)
        );
        assert_eq!(mover.speed, 12288);
        // Farther than 1400000 it is in no state, and flees again in the next pass.
        roamer.pass(Routine::Bolts, &mut mover, &far, &mut draws(&[]));
        assert_eq!(roamer.state, state::NONE);
        roamer.pass(Routine::Bolts, &mut mover, &far, &mut draws(&[]));
        assert_eq!(roamer.state, state::FLEES);
        let close = senses(&arc, CLOSE);
        roamer.pass(Routine::Bolts, &mut mover, &close, &mut draws(&[]));
        assert_eq!(roamer.state, state::FLEES);
    }

    #[test]
    fn the_species_that_rest_wake_when_the_player_runs_near() {
        let arc = arc();
        let mut mover = Mover::new([0; 3], 0);
        let mut roamer = Roamer::new();
        let rest = vec![Start {
            action: plays::RESTING,
            start: FROM_THE_LOOP,
            fade: false,
        }];
        // A player who walks past does not wake it.
        let walks = senses(&arc, CLOSE);
        let starts = roamer.pass(Routine::Rests, &mut mover, &walks, &mut draws(&[]));
        assert_eq!((starts, roamer.state), (rest.clone(), state::RESTS));
        assert!(
            roamer
                .pass(Routine::Rests, &mut mover, &walks, &mut draws(&[]))
                .is_empty()
        );
        // One who runs far off does not either; one who runs near does.
        let runs_far = Senses {
            player_runs: true,
            ..senses(&arc, FAR_OFF)
        };
        roamer.pass(Routine::Rests, &mut mover, &runs_far, &mut draws(&[]));
        assert_eq!(roamer.state, state::RESTS);
        let runs = Senses {
            player_runs: true,
            ..walks
        };
        let starts = roamer.pass(Routine::Rests, &mut mover, &runs, &mut draws(&[]));
        assert_eq!(
            (starts, roamer.state),
            (started(plays::WALKING), state::COMES)
        );
        // No speed is set: it comes at the animation's own.
        assert_eq!((mover.speed, mover.flags), (4096, 0));
        // It follows until he is more than 1900000 away, stands for 50 game frames
        // and rests again.
        let follows = senses(&arc, 1378);
        roamer.pass(Routine::Rests, &mut mover, &follows, &mut draws(&[]));
        assert_eq!(roamer.state, state::COMES);
        let lost = senses(&arc, 1379);
        let starts = roamer.pass(Routine::Rests, &mut mover, &lost, &mut draws(&[]));
        assert_eq!(starts, started(plays::STANDING));
        assert_eq!((roamer.state, roamer.count), (state::STANDS, 50));
        for _ in 0..50 {
            roamer.pass(Routine::Rests, &mut mover, &lost, &mut draws(&[]));
        }
        let starts = roamer.pass(Routine::Rests, &mut mover, &lost, &mut draws(&[]));
        assert_eq!(
            (starts, roamer.state),
            (started(plays::RESTING), state::RESTS)
        );
        // The first pass goes on into the rest it has just set: two starts.
        let mut roamer = Roamer::new();
        let starts = roamer.pass(Routine::Rests, &mut mover, &runs, &mut draws(&[]));
        assert_eq!(starts, [rest, started(plays::WALKING)].concat());
        assert_eq!(roamer.state, state::COMES);
    }

    #[test]
    fn species_6_watches_the_player_and_bolts_when_he_is_near() {
        let arc = arc();
        let mut mover = Mover::new([0; 3], 1024);
        mover.set_speed(8192, true);
        let mut roamer = Roamer::new();
        let far = senses(&arc, FAR_OFF);
        let starts = roamer.pass(Routine::Watches, &mut mover, &far, &mut draws(&[]));
        assert_eq!(
            (starts, roamer.state),
            (started(plays::STANDING), state::STANDS)
        );
        assert_eq!((mover.speed, mover.flags), (4096, 0));
        // It turns to him by 341 a game frame.
        roamer.pass(Routine::Watches, &mut mover, &far, &mut draws(&[]));
        assert_eq!((roamer.state, mover.turn), (state::STANDS, 1024 - 43));
        // Near: away at three times the speed.
        let near = senses(&arc, CLOSE);
        let starts = roamer.pass(Routine::Watches, &mut mover, &near, &mut draws(&[]));
        assert_eq!(
            (starts, roamer.state),
            (started(plays::WALKING), state::FLEES)
        );
        assert_eq!((mover.speed, mover.flags), (12288, flag::SPEED));
        // It turns to the point behind it and makes no step until it faces that point.
        let before = mover.turn;
        roamer.pass(Routine::Watches, &mut mover, &near, &mut draws(&[]));
        assert!(mover.turn > before);
        assert_eq!(mover.flags, flag::SPEED | flag::NO_STEP);
        mover.turn = 2048;
        mover.flags = flag::SPEED;
        roamer.pass(Routine::Watches, &mut mover, &near, &mut draws(&[]));
        assert_eq!((roamer.state, mover.flags), (state::FLEES, flag::SPEED));
        // More than 1400000 away: nothing again, and it stands and watches.
        roamer.pass(
            Routine::Watches,
            &mut mover,
            &senses(&arc, 1184),
            &mut draws(&[]),
        );
        assert_eq!(roamer.state, state::NONE);
    }

    #[test]
    fn an_outgrown_creature_waits_and_runs_away() {
        let arc = arc();
        let mut mover = Mover::new([0; 3], 0);
        let mut roamer = Roamer::new();
        let far = senses(&arc, FAR_OFF);
        let starts = roamer.pass(Routine::Outgrown, &mut mover, &far, &mut draws(&[]));
        assert_eq!(
            (starts, roamer.state),
            (started(plays::WAITING), state::WAITS)
        );
        let near = senses(&arc, CLOSE);
        let starts = roamer.pass(Routine::Outgrown, &mut mover, &near, &mut draws(&[]));
        assert_eq!(
            (starts, roamer.state),
            (started(plays::WALKING), state::FLEES)
        );
        assert_eq!((mover.speed, mover.flags), (8192, flag::SPEED));
        // It looks at him: the turn away starts slowly, 16 a game frame.
        roamer.pass(Routine::Outgrown, &mut mover, &near, &mut draws(&[]));
        assert_eq!(
            (mover.turn.abs(), mover.flags),
            (2, flag::SPEED | flag::NO_STEP)
        );
        // With its back half to him the step is the largest.
        assert_eq!(
            [0, 256, 257, 512, 513, 1024, 1025, 2048, 2049].map(pace),
            [16, 16, 32, 32, 64, 64, 128, 128, 256]
        );
        // Something in the way is gone round first, in the same pass.
        let mut blocked = Mover::new([0; 3], 2048);
        blocked.answers = [1, 0];
        let mut fleeing = roamer;
        fleeing.pass(Routine::Outgrown, &mut blocked, &near, &mut draws(&[]));
        assert_eq!((fleeing.side, fleeing.left), (1, 256));
        roamer.pass(
            Routine::Outgrown,
            &mut mover,
            &senses(&arc, 1184),
            &mut draws(&[]),
        );
        assert_eq!(roamer.state, state::NONE);
        // The first pass goes on into the wait it has just set.
        let mut roamer = Roamer::new();
        let starts = roamer.pass(Routine::Outgrown, &mut mover, &near, &mut draws(&[]));
        assert_eq!(
            starts,
            [started(plays::WAITING), started(plays::WALKING)].concat()
        );
    }

    #[test]
    fn the_items_come_in_front_of_every_routine() {
        let arc = arc();
        let near = senses(&arc, CLOSE);
        for routine in [
            Routine::Most,
            Routine::Rests,
            Routine::Watches,
            Routine::Outgrown,
            Routine::Wanders,
            Routine::Wheels,
        ] {
            // Item 41: the creature rests and stays so, however near the player is.
            let rests = Senses {
                item: item::REST,
                ..near
            };
            let (mut roamer, mut mover) = (Roamer::new(), Mover::new([0; 3], 1024));
            let starts = roamer.pass(routine, &mut mover, &rests, &mut draws(&[]));
            assert_eq!(
                (starts, roamer.state),
                (started(plays::RESTING), state::RESTS)
            );
            assert!(
                roamer
                    .pass(routine, &mut mover, &rests, &mut draws(&[]))
                    .is_empty()
            );
            assert_eq!((roamer.state, mover.turn), (state::RESTS, 1024));
            // Item 42: it walks and is steered to the player in every pass.
            let calls = Senses {
                item: item::CALL,
                ..near
            };
            let (mut roamer, mut mover) = (Roamer::new(), Mover::new([0; 3], 1024));
            let starts = roamer.pass(routine, &mut mover, &calls, &mut draws(&[]));
            assert_eq!(
                (starts, roamer.state),
                (started(plays::WALKING), state::WALKS)
            );
            assert!(mover.turn < 1024);
            assert!(
                roamer
                    .pass(routine, &mut mover, &calls, &mut draws(&[]))
                    .is_empty()
            );
            assert_eq!(roamer.state, state::WALKS);
        }
    }
}
