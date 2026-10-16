//! The lockstep's tests: two sessions joined in memory, the messages going through their
//! bytes, on made-up tables like the duel's own tests.

use super::*;
use crate::wire::{decode, encode};
use crossbeam_channel::{Receiver, Sender, TryRecvError, unbounded};
use jc_formats::battle::record::{Draws, NOTHING, Random, Record};
use jc_formats::battle::{BASES, Base, Descriptor, Equipment, Item, Move, SPECIES, Spell, class};
use jc_formats::creatures::{Creature, state};

/// Made-up tables, as the duel's own tests have them: a plain attack, a heal and two
/// attack spells, a body attack, a liquor for a minion and a thrown item.
pub(crate) fn tables() -> Tables {
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
    let heal = [0, 0, 0xD8, 0xFF, 0xFF, 125, 125, 0, class::NONE, 2, 0, 0];
    let liquor = [0, 0, 0xCE, 0xFF, 0xFF, 125, 125, 0, class::NONE, 2, 0, 2];
    let spell = Spell {
        word: 0x80,
        cost: 3,
        shown: 0,
        need: 0,
        element: 0,
        effect: attack(115, 100),
    };
    let none = Equipment {
        stats: [0; 6],
        level: 0,
        modifiers: None,
        effect: None,
    };
    Tables {
        uniques: Vec::new(),
        bases: vec![
            Base {
                growth: [100; 6],
                element: -1,
                body: 0,
            };
            BASES
        ],
        modifiers: vec![[32; 12]; BASES],
        spell_bits: vec![0; SPECIES],
        moves: vec![[Move::default(); 5]; SPECIES],
        names: vec![[0; jc_formats::name::LETTERS]; SPECIES],
        relations: [[32; 4]; 4],
        growth_steps: (0..20).map(|class| 285 + 35 * class).collect(),
        body_flags: vec![0; SPECIES],
        capture_steps: [2, 3, 5, 10],
        capture: [[16; 4]; 5],
        plain_attack: attack(115, 100),
        spells: vec![
            Spell {
                word: 0x40,
                effect: Descriptor::parse(&heal).expect("a descriptor"),
                ..spell
            },
            spell,
            spell,
        ],
        body_attacks: vec![attack(115, 100)],
        items: vec![
            Item {
                flags: 0x53,
                effect: Descriptor::parse(&liquor).expect("a descriptor"),
            },
            Item {
                flags: 0x82,
                effect: attack(100, 50),
            },
        ],
        weapons: vec![none],
        armours: vec![none],
        thirds: vec![none],
    }
}

fn minion(speed: u8, hp: i16) -> Creature {
    let mut record = Record::head(&[0; Record::HEAD]).expect("a head");
    (record.hp, record.max_hp, record.mp, record.max_mp) = (hp, hp, 20, 20);
    record.stats = [30, 20, 30, 20, speed, 0];
    record.modifiers = [32; 12];
    record.strengths = [8; 4];
    record.level = 10;
    record.species = 4;
    record.equipment = [NOTHING; 3];
    record.spells = 0b11;
    record.moves[0] = Move {
        known: true,
        attack: 0,
        grade: 0,
        flag: 0,
    };
    Creature::of_a_caught(state::CARRIED, record)
}

pub(crate) fn team(side: usize) -> Team {
    match side {
        0 => Team {
            members: vec![minion(40, 120), minion(30, 90)],
            items: vec![(0, 2)],
        },
        _ => Team {
            members: vec![minion(40, 100), minion(20, 80), minion(10, 60)],
            items: vec![(0, 1)],
        },
    }
}

/// One end of a connection in memory; the messages go through their bytes.
pub(crate) struct Pipe {
    out: Option<Sender<Vec<u8>>>,
    incoming: Receiver<Vec<u8>>,
    lost: bool,
}

pub(crate) fn pipes() -> (Pipe, Pipe) {
    let (a_out, b_in) = unbounded();
    let (b_out, a_in) = unbounded();
    let pipe = |out, incoming| Pipe {
        out: Some(out),
        incoming,
        lost: false,
    };
    (pipe(a_out, a_in), pipe(b_out, b_in))
}

impl Pipe {
    /// The next message, for a side driven by hand.
    fn next(&mut self) -> Option<Message> {
        match self.receive()? {
            Incoming::Message(message) => Some(message),
            Incoming::Lost(_) => None,
        }
    }
}

impl Transport for Pipe {
    fn send(&mut self, message: Message) {
        if let Some(out) = &self.out {
            let _ = out.send(encode(&message));
        }
    }

    fn receive(&mut self) -> Option<Incoming> {
        match self.incoming.try_recv() {
            Ok(bytes) => Some(match decode(&bytes) {
                Ok(message) => Incoming::Message(message),
                Err(err) => Incoming::Lost(Lost::Malformed(err.to_string())),
            }),
            Err(TryRecvError::Disconnected) if !self.lost => {
                self.lost = true;
                Some(Incoming::Lost(Lost::Closed))
            }
            Err(_) => None,
        }
    }

    fn close(&mut self) {
        self.out = None;
    }
}

/// A side that answers at once: random legal commands, `rounds` fights.
pub(crate) struct Bot {
    team: Team,
    picker: Random,
    rounds: u32,
}

impl Bot {
    pub(crate) fn new(side: usize, seed: u32, rounds: u32) -> Self {
        Self {
            team: team(side),
            picker: Random { seed },
            rounds,
        }
    }
}

impl Local for Bot {
    fn terms(&mut self) -> Option<Terms> {
        Some(Terms::default())
    }

    fn accept(&mut self, _: &Terms) -> Option<bool> {
        Some(true)
    }

    fn team(&mut self) -> Option<Team> {
        Some(self.team.clone())
    }

    fn ready(&mut self, _: u32) -> bool {
        true
    }

    fn choose(&mut self, tables: &Tables, duel: &Duel, side: usize) -> Option<Choice> {
        let legal = duel.legal(tables, side);
        match duel.turn() > 2000 || legal.is_empty() {
            true => Some(Choice::GiveUp),
            false => Some(legal[self.picker.draw() as usize % legal.len()]),
        }
    }

    fn rematch(&mut self, series: &Series) -> Option<bool> {
        let played: u32 = series.lost.iter().map(|lost| u32::from(*lost)).sum();
        Some(played < self.rounds)
    }

    fn bits(&mut self) -> u32 {
        self.picker.draw() << 16 | self.picker.draw()
    }
}

fn session(role: Role, pipe: Pipe, setup: Setup) -> VersusSession {
    VersusSession::new(role, Box::new(pipe), setup)
}

fn hello(rules: u64) -> Message {
    Message::Hello {
        protocol: PROTOCOL,
        build: BUILD.into(),
        rules,
        name: "Guest".into(),
    }
}

/// Runs both sessions until both ended; answers the notices of each.
fn run(
    sessions: &mut [VersusSession; 2],
    bots: &mut [Bot; 2],
    tables: &Tables,
) -> [Vec<Notice>; 2] {
    let mut notices = [Vec::new(), Vec::new()];
    for _ in 0..200_000 {
        for side in 0..2 {
            sessions[side].update(tables, &mut bots[side]);
            notices[side].extend(sessions[side].take_notices());
        }
        if sessions.iter().all(|session| session.ended().is_some()) {
            return notices;
        }
    }
    panic!("the sessions never end");
}

fn of_the_fights(notices: &[Notice]) -> Vec<Notice> {
    (notices.iter())
        .filter(|notice| {
            matches!(
                notice,
                Notice::Turn { .. } | Notice::Over { .. } | Notice::Started { .. }
            )
        })
        .cloned()
        .collect()
}

/// Both sessions up to their first fight.
fn playing(tables: &Tables) -> (VersusSession, VersusSession, [Bot; 2]) {
    let (a, b) = pipes();
    let mut host = session(Role::Host, a, Setup::new("Host", 7));
    let mut guest = session(Role::Guest, b, Setup::new("Guest", 7));
    let mut bots = [Bot::new(0, 5, 1), Bot::new(1, 6, 1)];
    while host.phase() != &Phase::Playing || guest.phase() != &Phase::Playing {
        host.update(tables, &mut bots[0]);
        guest.update(tables, &mut bots[1]);
    }
    (host, guest, bots)
}

#[test]
fn two_sides_play_the_same_fights_and_rematches() {
    let tables = tables();
    let (a, b) = pipes();
    let mut sessions = [
        session(Role::Host, a, Setup::new("Host", 7)),
        session(Role::Guest, b, Setup::new("Guest", 7)),
    ];
    let mut bots = [Bot::new(0, 11, 3), Bot::new(1, 12, 3)];
    let [host, guest] = run(&mut sessions, &mut bots, &tables);
    assert!(host.contains(&Notice::Met {
        name: "Guest".into()
    }));
    assert!(guest.contains(&Notice::Met {
        name: "Host".into()
    }));
    let fights = of_the_fights(&host);
    assert_eq!(fights, of_the_fights(&guest));
    let count = |what: fn(&Notice) -> bool| fights.iter().filter(|n| what(n)).count();
    assert_eq!(count(|n| matches!(n, Notice::Started { .. })), 3);
    assert_eq!(count(|n| matches!(n, Notice::Over { .. })), 3);
    assert!(count(|n| matches!(n, Notice::Turn { .. })) > 3);
    assert_eq!(sessions[0].series(), sessions[1].series());
    assert_eq!(sessions[0].round(), 3);
    for session in &sessions {
        assert!(matches!(session.ended(), Some(End::Left | End::PeerLeft)));
    }
}

#[test]
fn another_build_or_disc_is_refused_with_its_reason() {
    let tables = tables();
    for (build, rules, says) in [("9.9.9", 7, "build 9.9.9"), (BUILD, 8, "other rules")] {
        let (a, b) = pipes();
        let mut guest_setup = Setup::new("Guest", rules);
        guest_setup.build = build.into();
        let mut sessions = [
            session(Role::Host, a, Setup::new("Host", 7)),
            session(Role::Guest, b, guest_setup),
        ];
        let mut bots = [Bot::new(0, 1, 1), Bot::new(1, 2, 1)];
        run(&mut sessions, &mut bots, &tables);
        for session in &sessions {
            let Some(End::Refused(reason)) = session.ended() else {
                panic!("not refused: {:?}", session.ended());
            };
            assert!(reason.contains(says), "{reason}");
        }
    }
}

#[test]
fn a_wrong_hash_stops_both_sides_with_a_desync() {
    let tables = tables();
    for corrupt_at in [1, 2, 3, 4] {
        let (a, b) = pipes();
        let mut guest_setup = Setup::new("Guest", 7);
        guest_setup.corrupt_at = Some(corrupt_at);
        let mut sessions = [
            session(Role::Host, a, Setup::new("Host", 7)),
            session(Role::Guest, b, guest_setup),
        ];
        let mut bots = [Bot::new(0, 5, 1), Bot::new(1, 6, 1)];
        run(&mut sessions, &mut bots, &tables);
        for session in &sessions {
            assert_eq!(
                session.ended(),
                Some(&End::Desync { turn: corrupt_at }),
                "corrupt at {corrupt_at}"
            );
        }
    }
}

#[test]
fn a_side_that_leaves_loses_the_fight() {
    let tables = tables();
    for by_drop in [false, true] {
        let (mut host, mut guest, mut bots) = playing(&tables);
        match by_drop {
            false => guest.leave(),
            // The session's own drop leaves too (a window closed).
            true => drop(guest),
        }
        host.take_notices();
        host.update(&tables, &mut bots[0]);
        let notices = host.take_notices();
        assert!(notices.iter().any(|notice| matches!(
            notice,
            Notice::Over {
                winner: 0,
                gave_up: true,
                ..
            }
        )));
        assert_eq!(host.ended(), Some(&End::PeerLeft));
        assert_eq!(host.series().won(), [1, 0]);
    }
}

#[test]
fn a_lost_link_in_a_fight_is_a_disconnection_and_a_loss_of_the_other_side() {
    let tables = tables();
    let (mut host, mut guest, mut bots) = playing(&tables);
    // The guest's machine goes away without a word.
    guest.link.close();
    std::mem::forget(guest);
    host.update(&tables, &mut bots[0]);
    assert!(matches!(host.ended(), Some(End::Disconnected(_))));
    assert_eq!(host.series().won(), [1, 0]);
}

/// A host and a guest driven by hand, up to the guest's first turn.
fn at_the_guest_s_turn(tables: &Tables) -> (VersusSession, Pipe, Bot) {
    for seed in 0..50 {
        let (a, mut guest) = pipes();
        let mut host = session(Role::Host, a, Setup::new("Host", 7));
        let mut bot = Bot::new(0, seed, 1);
        let team = team(1);
        for message in [
            hello(7),
            Message::Accept { bits: 3 },
            Message::Team {
                team: team.to_bytes(),
                hash: team.hash(),
            },
            Message::Ready,
        ] {
            guest.send(message);
            host.update(tables, &mut bot);
        }
        while guest.next().is_some() {}
        if let Some(Step::Choose { side: 1, turn: 1 }) = host.step() {
            return (host, guest, bot);
        }
    }
    panic!("the guest never gets a turn");
}

/// What a message to a host at the guest's turn ends with; `None` for a turn number
/// means the turn in progress, `Some(n)` that turn plus n.
fn refused_with(turn: Option<u32>, message: Message) -> String {
    let tables = tables();
    let (mut host, mut guest, mut bot) = at_the_guest_s_turn(&tables);
    let Some(Step::Choose { turn: now, .. }) = host.step() else {
        unreachable!("the guest's turn")
    };
    let message = match message {
        Message::Turn { choice, hash, .. } => Message::Turn {
            turn: now + turn.unwrap_or(0),
            choice,
            hash,
        },
        other => other,
    };
    guest.send(message);
    host.update(&tables, &mut bot);
    let Some(End::Refused(reason)) = host.ended() else {
        panic!("not refused: {:?}", host.ended());
    };
    // The guest is told why.
    let mut told = None;
    while let Some(message) = guest.next() {
        told = Some(message);
    }
    assert_eq!(
        told,
        Some(Message::Refuse {
            reason: reason.clone()
        })
    );
    reason.clone()
}

#[test]
fn a_command_out_of_turn_or_not_allowed_is_refused() {
    let turn = |choice| Message::Turn {
        turn: 0,
        choice,
        hash: 0,
    };
    // The turn after the one in progress.
    let reason = refused_with(Some(1), turn(Choice::Attack));
    assert!(reason.contains("is side 1's"), "{reason}");
    // The minion that fights already cannot come in.
    let reason = refused_with(None, turn(Choice::Change { index: 0 }));
    assert!(reason.contains("NotThatMinion"), "{reason}");
    // An item the guest does not have.
    let reason = refused_with(None, turn(Choice::Item { item: 1, index: 0 }));
    assert!(reason.contains("NoPiece"), "{reason}");
    // A message of another phase.
    let reason = refused_with(None, Message::Ready);
    assert!(
        reason.contains("a ready message came while fighting"),
        "{reason}"
    );
    let reason = refused_with(None, Message::Hash { turn: 1, hash: 0 });
    assert!(reason.contains("none was asked"), "{reason}");
}

#[test]
fn a_team_that_is_no_team_is_refused() {
    let tables = tables();
    let mut forged = team(1);
    forged.members[0].record.max_hp = 5000;
    let honest = team(1);
    for (bytes, hash, says) in [
        (forged.to_bytes(), forged.hash(), "not one a save can hold"),
        (
            honest.to_bytes(),
            honest.hash() ^ 1,
            "does not match its hash",
        ),
        (vec![9, 9], 0, "cannot be read"),
    ] {
        let (a, mut guest) = pipes();
        let mut host = session(Role::Host, a, Setup::new("Host", 7));
        let mut bot = Bot::new(0, 1, 1);
        for message in [
            hello(7),
            Message::Accept { bits: 3 },
            Message::Team { team: bytes, hash },
        ] {
            guest.send(message);
            host.update(&tables, &mut bot);
        }
        let Some(End::Refused(reason)) = host.ended() else {
            panic!("not refused: {:?}", host.ended());
        };
        assert!(reason.contains(says), "{reason}");
    }
}

#[test]
fn the_seed_is_both_sides_bits_and_the_place_comes_from_it() {
    let tables = tables();
    let (a, mut guest) = pipes();
    let mut host = session(Role::Host, a, Setup::new("Host", 7));
    let mut bot = Bot::new(0, 1, 1);
    let mut copy = Random { seed: 1 };
    let host_bits = copy.draw() << 16 | copy.draw();
    guest.send(hello(7));
    host.update(&tables, &mut bot);
    guest.send(Message::Accept { bits: 0x1234_5678 });
    host.update(&tables, &mut bot);
    assert_eq!(host.seed(), host_bits ^ 0x1234_5678);
    assert_eq!(u32::from(host.place()), host.seed() % 39);
    assert_eq!(host.phase(), &Phase::Teams);
    let mut sent = Vec::new();
    while let Some(message) = guest.next() {
        sent.push(message);
    }
    assert!(sent.contains(&Message::Lobby {
        terms: Terms::default(),
        bits: host_bits
    }));
    // A second accept is out of order.
    guest.send(Message::Accept { bits: 1 });
    host.update(&tables, &mut bot);
    assert!(matches!(host.ended(), Some(End::Refused(_))));
}
