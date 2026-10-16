//! The lockstep of the fight of two players, without a screen: [`VersusSession`] owns the
//! duel and talks to the other side through a [`Transport`]; the local side's answers
//! come from a [`Local`] (the game's menus, or a bot in a check).
//!
//! The order, both sides alike:
//!
//! 1. **Hello**: the protocol, the build, the rules hash, the name. Any difference ends
//!    it with a refusal that says which.
//! 2. **Lobby**: the host sends its [`Terms`] and 32 bits, the guest accepts with its 32
//!    bits. The seed of the first fight is the XOR of the two.
//! 3. **Teams**: each side sends its team (`Team::to_bytes`) and its hash; the other
//!    reads it, tests the hash and `Team::plausible`, or refuses.
//! 4. **Ready**, from both, and the fight starts: `versus::start` of the host's team
//!    (side 0) and the guest's (side 1), with the terms' options and the seed.
//! 5. **Turns**: only the side whose turn it is sends its command, with the turn number
//!    and the duel's state hash after taking it. The other side tests the turn number and
//!    the command (`Duel::check`: legal for that side now), takes it, compares the hash and
//!    answers with its own. A hash that differs stops the fight on both sides (desync).
//! 6. After a fight: a rematch from both (32 new bits each, the same teams, the tally of
//!    [`Series`] goes on) or a leave.
//!
//! Nothing the other side sends is trusted: a message out of its order, a turn out of
//! turn, a command the duel refuses, a team that is no team, all end the session with a
//! refusal that says why. A leave during a fight, or a lost connection, is a fight given
//! up by that side.

use std::collections::VecDeque;
use std::fmt;
use std::time::Duration;

use jc_formats::battle::Tables;
use jc_formats::battle::arena::PLACE_COUNT;
use jc_formats::battle::duel::{Choice, Duel, Hash, SIDES, Step};
use jc_formats::versus::{self, Series, Team};

use crate::link::{Incoming, Lost, Transport};
use crate::online::Route;
use crate::wire::{Message, Terms};
use crate::{BUILD, PROTOCOL};

/// Which end of the connection this is. The host's team is side 0 (the Minions of the
/// Dawn, player 1), the guest's side 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Host,
    Guest,
}

impl Role {
    pub fn side(self) -> usize {
        match self {
            Role::Host => 0,
            Role::Guest => 1,
        }
    }
}

/// What this side says of itself in its hello.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setup {
    pub name: String,
    /// `crate::rules_hash` of this machine's disc.
    pub rules: u64,
    /// [`BUILD`], or another for a check that wants to be refused.
    pub build: String,
    /// A check's flag: the hashes this side sends from this turn on are wrong on purpose
    /// (from, not at: a turn lost asleep sends none).
    pub corrupt_at: Option<u32>,
}

impl Setup {
    pub fn new(name: &str, rules: u64) -> Self {
        Self {
            name: name.to_string(),
            rules,
            build: BUILD.to_string(),
            corrupt_at: None,
        }
    }
}

/// The local side's answers, asked again every update until given (`None`: not yet).
pub trait Local {
    /// The host's terms for the lobby.
    fn terms(&mut self) -> Option<Terms>;
    /// The guest's answer to the host's terms: take them, or leave.
    fn accept(&mut self, terms: &Terms) -> Option<bool>;
    /// This side's team.
    fn team(&mut self) -> Option<Team>;
    /// Ready for fight `round` (counted from 1).
    fn ready(&mut self, round: u32) -> bool;
    /// The command of this side's turn; it must pass `duel.check`, or it is asked again.
    fn choose(&mut self, tables: &Tables, duel: &Duel, side: usize) -> Option<Choice>;
    /// After a fight: one more, or leave.
    fn rematch(&mut self, series: &Series) -> Option<bool>;
    /// This side's half of a seed.
    fn bits(&mut self) -> u32 {
        crate::fresh_bits()
    }
}

/// Where the session stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    /// Waiting for the other side's hello.
    Hello,
    /// The host's terms and the guest's accept.
    Lobby,
    Teams,
    /// Both teams are in; waiting for both sides to be ready for the next fight.
    Ready,
    Playing,
    /// A fight is over; waiting for both sides to ask for a rematch.
    Over,
    Ended(End),
}

/// How a session ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum End {
    /// This side left.
    Left,
    /// The other side left.
    PeerLeft,
    /// The connection was lost (or the other side was silent too long).
    Disconnected(String),
    /// This side refused the other: a different build or disc, a bad team, a message out
    /// of order, an illegal command.
    Refused(String),
    /// The other side refused this one.
    RefusedByPeer(String),
    /// The two duels went apart at this turn.
    Desync { turn: u32 },
}

impl fmt::Display for End {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            End::Left => write!(f, "left"),
            End::PeerLeft => write!(f, "the other player left"),
            End::Disconnected(why) => write!(f, "disconnected: {why}"),
            End::Refused(why) => write!(f, "refused: {why}"),
            End::RefusedByPeer(why) => write!(f, "refused by the other player: {why}"),
            End::Desync { turn } => {
                write!(f, "desync at turn {turn}: the two games went apart")
            }
        }
    }
}

/// What happened, for the game to show; taken with [`VersusSession::take_notices`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    /// The other side's hello passed.
    Met {
        name: String,
    },
    /// The lobby is through: the terms of every fight.
    Agreed(Terms),
    /// Both teams are in.
    Teams,
    /// A fight started: its number from 1, its seed and place. The duel's first events
    /// (the dice) are in [`VersusSession::duel`].
    Started {
        round: u32,
        seed: u32,
        place: u8,
    },
    /// A command was taken, from either side, in fight `round`; the duel's events behind
    /// it are new.
    Turn {
        round: u32,
        side: usize,
        turn: u32,
        choice: Choice,
        hash: u64,
    },
    /// Fight `round` ended (beaten, given up, left or lost connection), and the tally
    /// after it.
    Over {
        round: u32,
        winner: usize,
        gave_up: bool,
        series: Series,
    },
    Ended(End),
}

/// One side of the fight of two players over the network. A Bevy resource wraps it in
/// the game; a check drives it in a loop.
pub struct VersusSession {
    role: Role,
    setup: Setup,
    link: Box<dyn Transport>,
    phase: Phase,
    hello_sent: bool,
    peer_name: Option<String>,
    terms: Option<Terms>,
    /// The halves of the seed of the next fight, by side.
    bits: [Option<u32>; SIDES],
    seed: u32,
    teams: [Option<Team>; SIDES],
    round: u32,
    ready: [bool; SIDES],
    duel: Option<Duel>,
    /// What the duel waited for when last asked.
    step: Option<Step>,
    /// This side's turns sent whose hash the other side has not answered yet.
    unanswered: VecDeque<(u32, u64)>,
    series: Series,
    notices: Vec<Notice>,
}

impl VersusSession {
    /// A session on a connected link; the hello goes out with the first update.
    pub fn new(role: Role, link: Box<dyn Transport>, setup: Setup) -> Self {
        Self {
            role,
            setup,
            link,
            phase: Phase::Hello,
            hello_sent: false,
            peer_name: None,
            terms: None,
            bits: [None; SIDES],
            seed: 0,
            teams: [None, None],
            round: 0,
            ready: [false; SIDES],
            duel: None,
            step: None,
            unanswered: VecDeque::new(),
            series: Series::default(),
            notices: Vec::new(),
        }
    }

    pub fn role(&self) -> Role {
        self.role
    }

    /// This side's side of the duel.
    pub fn side(&self) -> usize {
        self.role.side()
    }

    pub fn phase(&self) -> &Phase {
        &self.phase
    }

    pub fn ended(&self) -> Option<&End> {
        match &self.phase {
            Phase::Ended(end) => Some(end),
            _ => None,
        }
    }

    pub fn peer_name(&self) -> Option<&str> {
        self.peer_name.as_deref()
    }

    /// How the packets go, for a connection of a room code: straight or through a relay.
    pub fn route(&self) -> Option<Route> {
        self.link.route()
    }

    pub fn terms(&self) -> Option<Terms> {
        self.terms
    }

    /// The fight in progress or the last one.
    pub fn duel(&self) -> Option<&Duel> {
        self.duel.as_ref()
    }

    /// The number of the fight in progress or the last one, from 1.
    pub fn round(&self) -> u32 {
        self.round
    }

    pub fn seed(&self) -> u32 {
        self.seed
    }

    /// The place of the fight: the host's pick, or one from the seed.
    pub fn place(&self) -> u8 {
        let terms = self.terms.unwrap_or_default();
        terms
            .place
            .unwrap_or((self.seed % PLACE_COUNT as u32) as u8)
    }

    pub fn series(&self) -> &Series {
        &self.series
    }

    /// The two teams, once in.
    pub fn teams(&self) -> &[Option<Team>; SIDES] {
        &self.teams
    }

    /// What the fight waits for: a side's command of a turn, or nothing more.
    pub fn step(&self) -> Option<Step> {
        self.step
    }

    /// The turn whose command the fight waits for from this side.
    pub fn local_turn(&self) -> Option<u32> {
        match (&self.phase, self.step) {
            (Phase::Playing, Some(Step::Choose { side, turn })) if side == self.side() => {
                Some(turn)
            }
            _ => None,
        }
    }

    pub fn take_notices(&mut self) -> Vec<Notice> {
        std::mem::take(&mut self.notices)
    }

    /// Takes what came from the other side and asks the local side for what the session
    /// waits for. Never blocks.
    pub fn update(&mut self, tables: &Tables, local: &mut dyn Local) {
        if self.ended().is_some() {
            return;
        }
        if !self.hello_sent {
            self.hello_sent = true;
            self.link.send(Message::Hello {
                protocol: PROTOCOL,
                build: self.setup.build.clone(),
                rules: self.setup.rules,
                name: self.setup.name.clone(),
            });
        }
        while let Some(incoming) = self.link.receive() {
            match incoming {
                Incoming::Message(message) => {
                    if let Err(fault) = self.take(tables, message) {
                        self.fault(fault);
                    }
                }
                Incoming::Lost(lost) => self.lost(lost),
            }
            if self.ended().is_some() {
                return;
            }
        }
        self.drive(tables, local);
    }

    /// This side leaves: a fight in progress is lost by giving up.
    pub fn leave(&mut self) {
        if self.ended().is_some() {
            return;
        }
        if self.phase == Phase::Playing {
            self.count(1 - self.side(), true);
        }
        self.link.send(Message::Leave);
        self.end(End::Left);
    }

    /// For the end of a process: waits up to `wait` for the last messages to go out.
    pub fn finish(&mut self, wait: Duration) {
        self.leave();
        self.link.finish(wait);
    }

    /// Asks the local side for what the phase waits for, as long as it answers.
    fn drive(&mut self, tables: &Tables, local: &mut dyn Local) {
        let side = self.side();
        loop {
            match self.phase {
                Phase::Lobby => match self.role {
                    Role::Host => {
                        if self.terms.is_none()
                            && let Some(terms) = local.terms()
                        {
                            let bits = local.bits();
                            self.terms = Some(terms);
                            self.bits[side] = Some(bits);
                            self.link.send(Message::Lobby { terms, bits });
                        }
                        return;
                    }
                    Role::Guest => {
                        let Some(terms) = self.terms else { return };
                        match local.accept(&terms) {
                            Some(true) => {
                                let bits = local.bits();
                                self.bits[side] = Some(bits);
                                self.link.send(Message::Accept { bits });
                                self.agreed();
                            }
                            Some(false) => return self.leave(),
                            None => return,
                        }
                    }
                },
                Phase::Teams => {
                    if self.teams[side].is_some() {
                        return;
                    }
                    let Some(team) = local.team() else { return };
                    self.link.send(Message::Team {
                        team: team.to_bytes(),
                        hash: team.hash(),
                    });
                    self.teams[side] = Some(team);
                    self.teams_in();
                }
                Phase::Ready => {
                    if self.ready[side] || !local.ready(self.round) {
                        return;
                    }
                    self.ready[side] = true;
                    self.link.send(Message::Ready);
                    self.start(tables);
                }
                Phase::Playing => {
                    let Some(duel) = self.duel.as_mut() else {
                        return;
                    };
                    match duel.next(tables) {
                        Step::Over { .. } => self.settle(tables),
                        Step::Choose {
                            side: chooser,
                            turn,
                        } if chooser == side => {
                            let Some(choice) = local.choose(tables, duel, side) else {
                                return;
                            };
                            if duel.act(tables, side, choice).is_err() {
                                return;
                            }
                            let hash = duel.state_hash();
                            self.unanswered.push_back((turn, hash));
                            self.link.send(Message::Turn {
                                turn,
                                choice,
                                hash: self.sent_hash(turn, hash),
                            });
                            self.notices.push(Notice::Turn {
                                round: self.round,
                                side,
                                turn,
                                choice,
                                hash,
                            });
                            self.settle(tables);
                        }
                        Step::Choose { .. } => return,
                    }
                }
                Phase::Over => {
                    if self.bits[side].is_some() {
                        return;
                    }
                    match local.rematch(&self.series) {
                        Some(true) => {
                            let bits = local.bits();
                            self.bits[side] = Some(bits);
                            self.link.send(Message::Rematch { bits });
                            self.again();
                        }
                        Some(false) => return self.leave(),
                        None => return,
                    }
                }
                Phase::Hello | Phase::Ended(_) => return,
            }
        }
    }

    /// Takes a message of the other side, or says why it is refused.
    fn take(&mut self, tables: &Tables, message: Message) -> Result<(), Fault> {
        let other = 1 - self.side();
        let wrong = |phase: &Phase, message: &Message| {
            Fault::Refuse(format!(
                "a {} message came while {}",
                message.name(),
                phase_name(phase)
            ))
        };
        match (self.phase.clone(), message) {
            (_, Message::Refuse { reason }) => {
                self.lost_fight();
                self.end(End::RefusedByPeer(reason));
            }
            (_, Message::Leave) => {
                if self.phase == Phase::Playing {
                    self.count(self.side(), true);
                }
                self.end(End::PeerLeft);
            }
            (_, Message::Desync { turn }) => {
                self.end(End::Desync { turn });
            }
            (
                Phase::Hello,
                Message::Hello {
                    protocol,
                    build,
                    rules,
                    name,
                },
            ) => {
                if protocol != PROTOCOL {
                    return Err(Fault::Refuse(format!(
                        "the other player's game speaks protocol {protocol}, this one {PROTOCOL}: both need the same build"
                    )));
                }
                if build != self.setup.build {
                    return Err(Fault::Refuse(format!(
                        "the other player runs build {build}, this one is build {}: both need the same build",
                        self.setup.build
                    )));
                }
                if rules != self.setup.rules {
                    return Err(Fault::Refuse(format!(
                        "the other player's disc has other rules (rules hash {rules:016x}, here {:016x}): both need the same disc",
                        self.setup.rules
                    )));
                }
                self.peer_name = Some(name.clone());
                self.notices.push(Notice::Met { name });
                self.phase = Phase::Lobby;
            }
            (Phase::Lobby, Message::Lobby { terms, bits })
                if self.role == Role::Guest && self.terms.is_none() =>
            {
                self.terms = Some(terms);
                self.bits[other] = Some(bits);
            }
            (Phase::Lobby, Message::Accept { bits })
                if self.role == Role::Host && self.terms.is_some() =>
            {
                self.bits[other] = Some(bits);
                self.agreed();
            }
            (Phase::Teams, Message::Team { team, hash }) if self.teams[other].is_none() => {
                // The hash is of the bytes as sent (`Team::hash` of the sender's team).
                let mut sent = Hash::new();
                sent.bytes(&team);
                let team = Team::from_bytes(&team).ok_or_else(|| {
                    Fault::Refuse("the other player's team cannot be read".into())
                })?;
                if sent.0 != hash {
                    return Err(Fault::Refuse(
                        "the other player's team does not match its hash".into(),
                    ));
                }
                if !team.plausible(tables) {
                    return Err(Fault::Refuse(
                        "the other player's team is not one a save can hold".into(),
                    ));
                }
                self.teams[other] = Some(team);
                self.teams_in();
            }
            (Phase::Ready, Message::Ready) if !self.ready[other] => {
                self.ready[other] = true;
                self.start(tables);
            }
            (Phase::Playing, Message::Turn { turn, choice, hash }) => {
                let duel = self.duel.as_mut().ok_or(Fault::Refuse("no fight".into()))?;
                match duel.next(tables) {
                    Step::Choose { side, turn: now } if side == other && now == turn => {}
                    Step::Choose { side, turn: now } => {
                        return Err(Fault::Refuse(format!(
                            "a command for turn {turn} came, but turn {now} is side {side}'s"
                        )));
                    }
                    Step::Over { .. } => {
                        return Err(Fault::Refuse(format!(
                            "a command for turn {turn} came after the fight"
                        )));
                    }
                }
                duel.act(tables, other, choice).map_err(|refusal| {
                    Fault::Refuse(format!(
                        "the other player's command {choice:?} for turn {turn} is not allowed ({refusal:?})"
                    ))
                })?;
                let mine = duel.state_hash();
                self.notices.push(Notice::Turn {
                    round: self.round,
                    side: other,
                    turn,
                    choice,
                    hash: mine,
                });
                if mine != hash {
                    return Err(Fault::Desync(turn));
                }
                self.link.send(Message::Hash {
                    turn,
                    hash: self.sent_hash(turn, mine),
                });
                self.settle(tables);
            }
            (Phase::Playing | Phase::Over, Message::Hash { turn, hash }) => {
                let Some((asked, mine)) = self.unanswered.pop_front() else {
                    return Err(Fault::Refuse(format!(
                        "a hash for turn {turn} came, but none was asked"
                    )));
                };
                if asked != turn {
                    return Err(Fault::Refuse(format!(
                        "a hash for turn {turn} came, but turn {asked}'s was asked"
                    )));
                }
                if hash != mine {
                    return Err(Fault::Desync(turn));
                }
            }
            (Phase::Over, Message::Rematch { bits }) if self.bits[other].is_none() => {
                self.bits[other] = Some(bits);
                self.again();
            }
            (phase, message) => return Err(wrong(&phase, &message)),
        }
        Ok(())
    }

    /// The hash this side sends for a turn: its own, or a wrong one for a check.
    fn sent_hash(&self, turn: u32, hash: u64) -> u64 {
        match self.setup.corrupt_at.is_some_and(|at| turn >= at) {
            true => hash ^ 1,
            false => hash,
        }
    }

    /// The lobby is through: the seed of the first fight.
    fn agreed(&mut self) {
        let (Some(terms), [Some(host), Some(guest)]) = (self.terms, self.bits) else {
            return;
        };
        self.seed = host ^ guest;
        self.bits = [None; SIDES];
        self.notices.push(Notice::Agreed(terms));
        self.phase = Phase::Teams;
    }

    /// Both teams in: on to the first fight.
    fn teams_in(&mut self) {
        if self.teams.iter().all(Option::is_some) {
            self.notices.push(Notice::Teams);
            self.round = 1;
            self.ready = [false; SIDES];
            self.phase = Phase::Ready;
        }
    }

    /// Both sides ready: the fight starts.
    fn start(&mut self, tables: &Tables) {
        let ([true, true], [Some(host), Some(guest)]) = (self.ready, &self.teams) else {
            return;
        };
        let options = self.terms.unwrap_or_default().options;
        self.duel = Some(versus::start(tables, [host, guest], options, self.seed));
        self.unanswered.clear();
        self.notices.push(Notice::Started {
            round: self.round,
            seed: self.seed,
            place: self.place(),
        });
        self.phase = Phase::Playing;
        self.settle(tables);
    }

    /// After a command: the fight goes on to the next choice, or is over.
    fn settle(&mut self, tables: &Tables) {
        let Some(duel) = self.duel.as_mut() else {
            return;
        };
        let step = duel.next(tables);
        self.step = Some(step);
        if let Step::Over { winner, gave_up } = step {
            self.count(winner, gave_up);
            self.bits = [None; SIDES];
            self.phase = Phase::Over;
        }
    }

    /// Both sides asked for a rematch: the next fight's seed, the same teams.
    fn again(&mut self) {
        let [Some(host), Some(guest)] = self.bits else {
            return;
        };
        if !self.unanswered.is_empty() {
            return self.fault(Fault::Refuse(
                "a rematch came before the hash of the last turn".into(),
            ));
        }
        self.seed = host ^ guest;
        self.bits = [None; SIDES];
        self.round += 1;
        self.ready = [false; SIDES];
        self.phase = Phase::Ready;
    }

    /// The end of a fight, counted in the tally.
    fn count(&mut self, winner: usize, gave_up: bool) {
        self.series.count(winner, gave_up);
        self.notices.push(Notice::Over {
            round: self.round,
            winner,
            gave_up,
            series: self.series,
        });
    }

    /// A fight in progress when the other side goes away is the other side's loss.
    fn lost_fight(&mut self) {
        if self.phase == Phase::Playing {
            self.count(self.side(), true);
        }
    }

    fn lost(&mut self, lost: Lost) {
        self.lost_fight();
        self.end(End::Disconnected(lost.to_string()));
    }

    fn fault(&mut self, fault: Fault) {
        match fault {
            Fault::Refuse(reason) => {
                self.link.send(Message::Refuse {
                    reason: reason.clone(),
                });
                self.end(End::Refused(reason));
            }
            Fault::Desync(turn) => {
                self.link.send(Message::Desync { turn });
                self.end(End::Desync { turn });
            }
        }
    }

    fn end(&mut self, end: End) {
        self.notices.push(Notice::Ended(end.clone()));
        self.phase = Phase::Ended(end);
        self.link.close();
    }
}

impl Drop for VersusSession {
    fn drop(&mut self) {
        self.leave();
    }
}

/// Why a message of the other side ends the session.
enum Fault {
    Refuse(String),
    Desync(u32),
}

fn phase_name(phase: &Phase) -> &'static str {
    match phase {
        Phase::Hello => "waiting for the hello",
        Phase::Lobby => "in the lobby",
        Phase::Teams => "waiting for the teams",
        Phase::Ready => "waiting for both to be ready",
        Phase::Playing => "fighting",
        Phase::Over => "between fights",
        Phase::Ended(_) => "after the end",
    }
}

#[cfg(test)]
pub(crate) mod tests;
