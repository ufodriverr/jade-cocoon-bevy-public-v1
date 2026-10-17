
use std::error::Error;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::time::Duration;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use jc_disc::{DiscImage, Executable, LocalConfig};
use jc_formats::battle::Tables;
use jc_formats::battle::arena::PLACE_COUNT;
use jc_formats::battle::duel::Options;
use jc_formats::battle::record::{Draws, Random};
use jc_formats::carried::Carried;
use jc_formats::menu::{self, Rect};
use jc_formats::name;
use jc_formats::pad::{self, Pad, Repeat};
use jc_formats::player_menu::SOUND_REFUSED;
use jc_formats::save::Memory;
use jc_formats::text::{self, Font, Fonts};
use jc_formats::versus::{self, MOST, Team};
use jc_net::lan::{self, Discovery};
use jc_net::online::{Code, Route};
use jc_net::session::{End, Role};
use jc_net::wire::Terms;
use jc_net::{LAN_PORT, PORT};

use crate::card_view::{CardView, CardsKept, Picked};
use crate::name_entry::{NameEntry, SCREEN, Typing, WHOLE};
use crate::net::Versus;
use crate::pointer::{self, Aim, Pointer};
use crate::title::Press;
use crate::versus_check::Mode;
use crate::versus_view::{self, Arena, CURSOR, FromLobby, Look, PLAIN, Painter, Start};

type Failure = Box<dyn Error>;

/// Passes of the lobby in a second, as the menus over a room have them.
const PASSES_A_SECOND: f64 = 25.0;
const GAME_FRAME: u32 = 2;
/// The longest stretch of time one pass of the app may stand for.
const LONGEST_PASS: f64 = 0.1;
const LOBBY_MUSIC: usize = 142;
/// The rows of the list of minions on screen at once.
const SHOWN_MINIONS: usize = 7;
/// The height of a line of text and of a row.
const LINE: i16 = 14;
const ROW: i16 = 16;
/// How long the leaving lobby waits for its "leave" to go out.
const LAST_WORD: Duration = Duration::from_millis(300);
/// The longest address the player can type.
const ADDRESS_MOST: usize = 48;
/// The longest room code the player can type, with dashes and spaces.
const CODE_MOST: usize = 24;
/// The rows of the step of the code: "Paste" and "Join".
const CODE_PASTE: usize = 0;
const CODE_JOIN: usize = 1;
/// The keypad of the address, row by row: what each key types, then the key that takes
/// the last letter away and the key that joins.
const KEYPAD: [&str; 14] = [
    "1", "2", "3", "4", "5", "6", "7", "8", "9", "0", ".", ":", "Del", "OK",
];
const KEYPAD_COLUMNS: usize = 7;
const KEY_DELETE: usize = 12;
const KEY_OK: usize = 13;
/// The seconds behind which a check takes the picture of a step that opened.
const SHOT_AFTER: f64 = 0.8;

/// The signs of the line of help: the cross and the triangle of the menus' pool
/// (`prompts.rs` shows the keys or the pad's buttons in their place).
const CROSS_SIGN: usize = 16;
const TRIANGLE_SIGN: usize = 167;

/// The boxes of the lobby's screen, in console pixels.
mod boxes {
    use jc_formats::menu::Rect;

    pub const HEADER: Rect = Rect {
        x: 20,
        y: 10,
        w: 280,
        h: 24,
    };
    pub const BODY: Rect = Rect {
        x: 20,
        y: 40,
        w: 280,
        h: 168,
    };
}

/// The title asked for the lobby: it opens in the next frame.
#[derive(Resource)]
pub struct Wanted;

/// What the lobby puts on screen, its camera and its picture: the arena takes them away
/// with the lobby once both teams are in.
#[derive(Component)]
pub struct LobbyPart;

/// A step of the lobby.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// The list of saves is up.
    Save,
    Minions,
    Name,
    Way,
    /// How the host is joined: by room code, or on the network and by address.
    HostHow,
    /// How the guest joins: by room code, the network's list, or an address.
    JoinHow,
    Hosting,
    /// The host's room code, while it waits.
    HostingOnline,
    Joining,
    Address,
    /// The guest's room code, pasted or typed.
    Code,
    Connecting,
    /// The host sets the terms; the guest waits for them.
    Terms,
    /// The guest sees the host's terms.
    Accept,
    /// "Ready" was said: the other side or the arena is waited for.
    Waiting,
    /// What went wrong, until a confirm.
    Message,
}

impl Step {
    const ALL: [Step; 16] = [
        Step::Save,
        Step::Minions,
        Step::Name,
        Step::Way,
        Step::HostHow,
        Step::JoinHow,
        Step::Hosting,
        Step::HostingOnline,
        Step::Joining,
        Step::Address,
        Step::Code,
        Step::Connecting,
        Step::Terms,
        Step::Accept,
        Step::Waiting,
        Step::Message,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Step::Save => "save",
            Step::Minions => "minions",
            Step::Name => "name",
            Step::Way => "way",
            Step::HostHow => "host-how",
            Step::JoinHow => "join-how",
            Step::Hosting => "hosting",
            Step::HostingOnline => "hosting-online",
            Step::Joining => "joining",
            Step::Address => "address",
            Step::Code => "code",
            Step::Connecting => "connecting",
            Step::Terms => "terms",
            Step::Accept => "accept",
            Step::Waiting => "waiting",
            Step::Message => "message",
        }
    }

    fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|step| step.name() == name)
    }
}

/// `--lobby-press=STEP=SECONDS:BUTTON,...;STEP=...`: the buttons a hidden run taps on a
/// step of the lobby, counted from the moment the step opens. A step named more than
/// once takes its sets in turn, one each time it opens, in this run's lobbies; its last
/// set stays for every later time.
#[derive(Resource, Debug, Default, Clone)]
pub struct LobbyPresses(Vec<(Step, Vec<Press>)>);

impl LobbyPresses {
    pub fn of(value: &str) -> Option<Self> {
        (value.split(';'))
            .filter(|set| !set.trim().is_empty())
            .map(|set| {
                let (step, presses) = set.split_once('=')?;
                Some((Step::named(step.trim())?, crate::title::presses(presses)?))
            })
            .collect::<Option<Vec<_>>>()
            .map(Self)
    }

    /// The next set of a step that opens, taken: the last one of a step stays.
    fn take(&mut self, step: Step) -> Vec<Press> {
        let mut sets = (self.0.iter().enumerate()).filter(|(_, (named, _))| *named == step);
        match (sets.next(), sets.next()) {
            (Some((at, _)), Some(_)) => self.0.remove(at).1,
            (Some((_, (_, presses))), None) => presses.clone(),
            (None, _) => Vec::new(),
        }
    }
}

/// A minion of the save's list, as a row shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Minion {
    /// The number of its record among the save's fifteen.
    pub number: usize,
    pub name: String,
    pub level: u8,
    pub hp: i16,
}

/// The key of the keypad a direction leads to from `at`: left and right go round in a
/// row, up and down go to the other row.
fn keypad_step(at: usize, repeated: u16) -> usize {
    let (row, column) = (at / KEYPAD_COLUMNS, at % KEYPAD_COLUMNS);
    let rows = KEYPAD.len() / KEYPAD_COLUMNS;
    let (row, column) = if repeated & pad::LEFT != 0 {
        (row, (column + KEYPAD_COLUMNS - 1) % KEYPAD_COLUMNS)
    } else if repeated & pad::RIGHT != 0 {
        (row, (column + 1) % KEYPAD_COLUMNS)
    } else if repeated & (pad::UP | pad::DOWN) != 0 {
        ((row + 1) % rows, column)
    } else {
        (row, column)
    };
    row * KEYPAD_COLUMNS + column
}

/// The direction that brings the keypad's cursor from `at` a step nearer to `key`, for the
/// mouse.
fn keypad_way(at: usize, key: usize) -> u16 {
    match (at / KEYPAD_COLUMNS).cmp(&(key / KEYPAD_COLUMNS)) {
        std::cmp::Ordering::Less => pad::DOWN,
        std::cmp::Ordering::Greater => pad::UP,
        std::cmp::Ordering::Equal if key % KEYPAD_COLUMNS > at % KEYPAD_COLUMNS => pad::RIGHT,
        std::cmp::Ordering::Equal => pad::LEFT,
    }
}

/// What the session tells the lobby in a frame.
#[derive(Debug, Clone, Default)]
pub struct Net {
    pub role: Option<Role>,
    /// The other player's name, once the hello passed.
    pub peer: Option<String>,
    /// The host's terms, at the guest, once they came.
    pub terms: Option<Terms>,
    /// The host's room code, and whether a guest can find it yet.
    pub room: Option<(Code, bool)>,
    /// How the packets go, for a connection of a room code.
    pub route: Option<Route>,
    /// A host's internet address, once the router forwards its port.
    pub internet: Option<SocketAddr>,
}

/// What a pass asks of the lobby's system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Deed {
    /// The list of saves again.
    Saves,
    /// The team of the picks is made, and the name step comes.
    Team,
    NameEntry,
    /// The name is taken and kept.
    Named,
    Host,
    /// Hosting under a room code.
    HostOnline,
    /// The list of the network's hosts is asked for.
    Listen,
    Join(String),
    /// Joining by a room code.
    JoinOnline(Code),
    /// The host's room code goes to the clipboard.
    Copy,
    /// The clipboard comes into the field of the step (the code, or the address).
    Paste,
    /// The arena (and its session) goes, and the lobby is back at the step given.
    Leave,
    Terms(Terms),
    Accept(bool),
}

/// A row of a step: what it says, in columns, and whether it can be taken.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    columns: Vec<(i16, String)>,
    dim: bool,
}

impl Row {
    fn of(said: impl Into<String>) -> Self {
        Self {
            columns: vec![(0, said.into())],
            dim: false,
        }
    }
}

/// The lobby's screens without anything of the disc: the step and its cursor, the
/// team's picks, the name, the hosts heard, the address and the terms.
#[derive(Debug, Clone)]
pub struct Form {
    pub step: Step,
    pub cursor: usize,
    /// The first minion of the list on screen.
    first: usize,
    pub minions: Vec<Minion>,
    /// The picked minions, as places of `minions`, in fight order.
    pub picks: Vec<usize>,
    pub name: String,
    /// The hosts heard: name and address.
    pub hosts: Vec<(String, String)>,
    pub address: String,
    /// The room code typed or pasted.
    pub code: String,
    /// What the guest connects to, for the screen.
    pub joining: String,
    /// The guest connects by a room code: a message after it leads back to the code.
    pub online: bool,
    /// What the last copy or paste said, until the step changes.
    pub note: Option<String>,
    pub terms: Terms,
    pub message: Vec<String>,
    /// The step a message leads back to, and the one a cancel of "Connecting" does.
    pub back: Step,
    /// This PC's addresses and the port a host waits on, for the screen.
    pub addresses: Vec<String>,
    pub port: u16,
}

impl Default for Form {
    fn default() -> Self {
        Self {
            step: Step::Save,
            cursor: 0,
            first: 0,
            minions: Vec::new(),
            picks: Vec::new(),
            name: String::new(),
            hosts: Vec::new(),
            address: String::new(),
            code: String::new(),
            joining: String::new(),
            online: false,
            note: None,
            terms: Terms {
                options: Options {
                    items: true,
                    changing: true,
                },
                place: None,
            },
            message: Vec::new(),
            back: Step::Way,
            addresses: Vec::new(),
            port: PORT,
        }
    }
}

/// What a pass did: what it asks for, and its sounds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Passed {
    pub deed: Option<Deed>,
    pub sounds: Vec<u8>,
}

/// The words of the terms.
fn rules_word(items: bool) -> &'static str {
    match items {
        true => "Save items",
        false => "Original",
    }
}

fn on_off(on: bool) -> &'static str {
    match on {
        true => "On",
        false => "Off",
    }
}

fn place_word(place: Option<u8>) -> String {
    match place {
        None => "Random".into(),
        Some(place) => format!("{} of {PLACE_COUNT}", u32::from(place) + 1),
    }
}

/// The place after `place` (`forward`) or before it: at random, then the 39.
fn next_place(place: Option<u8>, forward: bool) -> Option<u8> {
    let last = (PLACE_COUNT - 1) as u8;
    match (place, forward) {
        (None, true) => Some(0),
        (None, false) => Some(last),
        (Some(place), true) if place >= last => None,
        (Some(place), true) => Some(place + 1),
        (Some(0), false) => None,
        (Some(place), false) => Some(place - 1),
    }
}

impl Form {
    /// The step that opens: its cursor on its first row.
    pub fn go(&mut self, step: Step) {
        info!("the lobby: the step {:?} opens", step);
        (self.step, self.cursor, self.first) = (step, 0, 0);
        self.note = None;
    }

    /// The room code of the field, if it is one.
    pub fn typed_code(&self) -> Option<Code> {
        Code::parse(&self.code).ok()
    }

    /// The heading of the step and the lines above its rows.
    fn page(&self, net: &Net) -> (String, Vec<String>) {
        let peer = net
            .peer
            .clone()
            .unwrap_or_else(|| "The other player".into());
        let host = net.role == Some(Role::Host);
        match self.step {
            Step::Save => (
                "Your team".into(),
                vec!["Pick the save of your team.".into()],
            ),
            Step::Minions => {
                let mut lines = vec!["Up to three minions, in fight order.".into()];
                if self.minions.len() > SHOWN_MINIONS {
                    let last = (self.first + SHOWN_MINIONS).min(self.minions.len());
                    lines.push(format!(
                        "Minions {} to {} of {}",
                        self.first + 1,
                        last,
                        self.minions.len()
                    ));
                }
                ("Your team".into(), lines)
            }
            Step::Name => (
                "Your name".into(),
                vec![
                    "The other player sees this name.".into(),
                    format!("Name: {}", self.name),
                ],
            ),
            Step::Way => (
                "Host or join".into(),
                vec![
                    format!("Team ready. Name: {}", self.name),
                    "One player hosts, the other joins.".into(),
                ],
            ),
            Step::HostHow => (
                "Host a match".into(),
                vec!["How will the other player join?".into()],
            ),
            Step::JoinHow => ("Join a match".into(), vec!["How do you join?".into()]),
            Step::HostingOnline => ("Hosting online".into(), vec!["Your room code:".into()]),
            Step::Code => ("Join online".into(), vec!["The host's room code:".into()]),
            Step::Hosting => {
                let mut lines = vec![
                    "Waiting for a player...".into(),
                    "The other player joins one of".into(),
                    "this PC's addresses:".into(),
                ];
                lines.extend(self.addresses.iter().cloned());
                lines.push(format!("Port {} (TCP)", self.port));
                if let Some(internet) = net.internet {
                    lines.push(format!("Your internet address: {internet}"));
                }
                ("Hosting".into(), lines)
            }
            Step::Joining => {
                let heard = match self.hosts.is_empty() {
                    true => "No host heard on this network yet.",
                    false => "Hosts on this network:",
                };
                ("Join a match".into(), vec![heard.into()])
            }
            Step::Address => (
                "Join by address".into(),
                vec![
                    "The host's address, as IP or IP:PORT".into(),
                    format!("(port {PORT} when none is given):"),
                ],
            ),
            Step::Connecting if self.online => (
                "Join online".into(),
                vec![
                    format!("Looking for the host of {}...", self.joining),
                    "This takes up to 20 seconds.".into(),
                ],
            ),
            Step::Connecting => (
                "Join a match".into(),
                vec![format!("Connecting to {}...", self.joining)],
            ),
            Step::Terms if host => (
                "The terms".into(),
                [format!("{peer} is here. Set the terms:")]
                    .into_iter()
                    .chain(route_line(net))
                    .collect(),
            ),
            Step::Terms => (
                "The terms".into(),
                [
                    format!("{peer} is here."),
                    format!("Waiting for {peer} to set the terms..."),
                ]
                .into_iter()
                .chain(route_line(net))
                .collect(),
            ),
            Step::Accept => {
                let terms = net.terms.unwrap_or(self.terms);
                (
                    "The terms".into(),
                    vec![
                        format!("{peer} sets the terms:"),
                        format!("Rules: {}", rules_word(terms.options.items)),
                        format!("Changing minions: {}", on_off(terms.options.changing)),
                        format!("Place: {}", place_word(terms.place)),
                    ]
                    .into_iter()
                    .chain(route_line(net))
                    .collect(),
                )
            }
            Step::Waiting => {
                let said = match host {
                    true => format!("Waiting for {peer} to say Ready..."),
                    false => "Ready. The arena opens...".into(),
                };
                (
                    "The terms".into(),
                    [said].into_iter().chain(route_line(net)).collect(),
                )
            }
            // The first line of a message is its heading.
            Step::Message => {
                let mut lines = self.message.clone();
                let heading = match lines.is_empty() {
                    true => String::new(),
                    false => lines.remove(0),
                };
                (heading, lines)
            }
        }
    }

    /// The rows of the step.
    fn rows(&self, net: &Net) -> Vec<Row> {
        let host = net.role == Some(Role::Host);
        match self.step {
            Step::Save => Vec::new(),
            Step::Minions => {
                let mut rows: Vec<Row> = (self.minions.iter().enumerate())
                    .map(|(place, minion)| {
                        let order = (self.picks.iter()).position(|&pick| pick == place);
                        Row {
                            columns: vec![
                                (0, order.map_or("-".into(), |order| (order + 1).to_string())),
                                (16, minion.name.clone()),
                                (132, format!("Lv {}", minion.level)),
                                (184, format!("HP {}", minion.hp)),
                            ],
                            dim: order.is_none() && self.picks.len() >= MOST,
                        }
                    })
                    .collect();
                rows.push(Row {
                    dim: self.picks.len() < 2,
                    ..Row::of("Shuffle")
                });
                rows.push(Row {
                    dim: self.picks.is_empty(),
                    ..Row::of("Done")
                });
                rows
            }
            Step::Name => vec![Row::of("Go on"), Row::of("Change the name")],
            Step::Way => vec![Row::of("Host a match"), Row::of("Join a match")],
            Step::HostHow => vec![
                Row::of("Online (room code)"),
                Row::of("Same network or address"),
            ],
            Step::JoinHow => vec![
                Row::of("Online (room code)"),
                Row::of("Same network"),
                Row::of("By address"),
            ],
            Step::HostingOnline => vec![Row::of("Copy the code"), Row::of("Cancel")],
            Step::Code => vec![
                Row::of("Paste"),
                Row {
                    dim: self.typed_code().is_none(),
                    ..Row::of("Join")
                },
            ],
            Step::Hosting | Step::Connecting => vec![Row::of("Cancel")],
            Step::Joining => {
                let mut rows = vec![Row::of("Enter address...")];
                rows.extend(self.hosts.iter().map(|(name, address)| Row {
                    columns: vec![(0, name.clone()), (120, address.clone())],
                    dim: false,
                }));
                rows
            }
            Step::Address => KEYPAD.into_iter().map(Row::of).collect(),
            Step::Terms if host => {
                let options = self.terms.options;
                let pair = |label: &str, value: String| Row {
                    columns: vec![(0, label.into()), (120, value)],
                    dim: false,
                };
                vec![
                    pair("Rules", rules_word(options.items).into()),
                    pair("Changing", on_off(options.changing).into()),
                    pair("Place", place_word(self.terms.place)),
                    Row::of("Ready"),
                ]
            }
            Step::Terms => vec![Row::of("Leave")],
            Step::Accept => vec![Row::of("Ready"), Row::of("Leave")],
            Step::Waiting => vec![Row::of("Leave")],
            Step::Message => vec![Row::of("OK")],
        }
    }

    /// The lines under the rows: what the row under the cursor of the terms does.
    fn below(&self, net: &Net) -> Vec<String> {
        let mut lines = self.below_rows(net);
        lines.extend(self.note.clone());
        lines
    }

    /// What the row under the cursor does, or what the step waits for.
    fn below_rows(&self, net: &Net) -> Vec<String> {
        match self.step {
            Step::HostHow => {
                return vec![
                    match self.cursor {
                        0 => "Recommended: over the internet, no router setup.",
                        _ => "On this network, or by your IP address and port.",
                    }
                    .into(),
                ];
            }
            Step::JoinHow => {
                return vec![
                    match self.cursor {
                        0 => "Recommended: the code the host sent you.",
                        1 => "The hosts of this network are listed.",
                        _ => "Type the host's IP address.",
                    }
                    .into(),
                ];
            }
            Step::HostingOnline => {
                return match net.room {
                    Some((_, true)) => vec![
                        "Send it to the other player in any chat.".into(),
                        "Waiting for them to join...".into(),
                    ],
                    _ => vec!["Going online...".into()],
                };
            }
            Step::Code => {
                return vec![match Code::parse(&self.code) {
                    Ok(code) => format!("Room {code}: Join."),
                    Err(jc_net::online::CodeError::Empty) => "Paste it (Ctrl+V) or type it.".into(),
                    Err(err) => format!("Not a code yet: {err}."),
                }];
            }
            _ => {}
        }
        if self.step != Step::Terms || net.role != Some(Role::Host) {
            return Vec::new();
        }
        let said = match self.cursor {
            0 if self.terms.options.items => "Each side may use the items of its save.",
            0 => "No items, as in the original.",
            1 => "A side may change its minion as a turn.",
            2 => "Where the fights are: one of the story's.",
            _ => "The terms go to the other player.",
        };
        vec![said.into()]
    }

    /// The places of the rows on screen, as places of [`Form::rows`]: the list of
    /// minions shows a part of itself.
    fn shown(&self, rows: usize) -> Vec<usize> {
        match self.step {
            Step::Minions => {
                let minions = self.minions.len();
                let last = (self.first + SHOWN_MINIONS).min(minions);
                (self.first..last).chain(minions..rows).collect()
            }
            _ => (0..rows).collect(),
        }
    }

    /// One pass with the pad's words.
    pub fn pass(&mut self, pad: &Pad, net: &Net) -> Passed {
        let mut passed = Passed::default();
        let rows = self.rows(net);
        let count = rows.len();
        let host = net.role == Some(Role::Host);
        // Up and down move the cursor, without going round; on the keypad of the address
        // all four directions do.
        if self.step == Step::Address {
            let at = self.cursor;
            self.cursor = keypad_step(at, pad.repeated);
            if self.cursor != at {
                passed.sounds.push(name::SOUND_MOVE);
            }
        } else if count > 0 {
            let at = self.cursor;
            if pad.repeated & pad::UP != 0 {
                self.cursor = self.cursor.saturating_sub(1);
            } else if pad.repeated & pad::DOWN != 0 {
                self.cursor = (self.cursor + 1).min(count - 1);
            }
            if self.cursor != at {
                passed.sounds.push(name::SOUND_MOVE);
            }
            self.cursor = self.cursor.min(count - 1);
        }
        // The list of minions keeps its cursor on screen.
        if self.step == Step::Minions && self.cursor < self.minions.len() {
            if self.cursor < self.first {
                self.first = self.cursor;
            } else if self.cursor >= self.first + SHOWN_MINIONS {
                self.first = self.cursor + 1 - SHOWN_MINIONS;
            }
        }
        // Left and right turn the terms of the host.
        let turned = pad.repeated & (pad::LEFT | pad::RIGHT);
        if self.step == Step::Terms && host && turned != 0 && self.cursor < 3 {
            self.turn(turned & pad::RIGHT != 0);
            passed.sounds.push(name::SOUND_MOVE);
        }
        let confirmed = menu::confirmed(pad);
        let cancelled = !confirmed && menu::cancelled(pad);
        if cancelled {
            passed.sounds.push(name::SOUND_CANCEL);
            passed.deed = self.cancel();
            return passed;
        }
        if !confirmed || count == 0 {
            return passed;
        }
        if rows.get(self.cursor).is_some_and(|row| row.dim) {
            passed.sounds.push(SOUND_REFUSED);
            return passed;
        }
        passed.sounds.push(name::SOUND_CONFIRM);
        passed.deed = self.confirm(host);
        passed
    }

    /// A turn of the term under the cursor.
    fn turn(&mut self, forward: bool) {
        let options = &mut self.terms.options;
        match self.cursor {
            0 => options.items = !options.items,
            1 => options.changing = !options.changing,
            _ => self.terms.place = next_place(self.terms.place, forward),
        }
    }

    /// A confirm on the row under the cursor.
    fn confirm(&mut self, host: bool) -> Option<Deed> {
        let at = self.cursor;
        match self.step {
            Step::Save => None,
            Step::Minions => {
                let minions = self.minions.len();
                if at < minions {
                    match self.picks.iter().position(|&pick| pick == at) {
                        Some(order) => {
                            self.picks.remove(order);
                        }
                        None => self.picks.push(at),
                    }
                    None
                } else if at == minions {
                    self.shuffle(jc_net::fresh_bits());
                    None
                } else {
                    Some(Deed::Team)
                }
            }
            Step::Name => Some(match at {
                0 => Deed::Named,
                _ => Deed::NameEntry,
            }),
            Step::Way => {
                self.go(match at {
                    0 => Step::HostHow,
                    _ => Step::JoinHow,
                });
                None
            }
            Step::HostHow => match at {
                0 => Some(Deed::HostOnline),
                _ => Some(Deed::Host),
            },
            Step::JoinHow => match at {
                0 => {
                    self.go(Step::Code);
                    None
                }
                1 => Some(Deed::Listen),
                _ => {
                    self.go(Step::Address);
                    None
                }
            },
            Step::HostingOnline => match at {
                0 => Some(Deed::Copy),
                _ => Some(Deed::Leave),
            },
            Step::Code => match at {
                CODE_PASTE => Some(Deed::Paste),
                _ => self.typed_code().map(Deed::JoinOnline),
            },
            Step::Hosting | Step::Waiting | Step::Connecting => Some(Deed::Leave),
            Step::Joining => match at {
                0 => {
                    self.go(Step::Address);
                    None
                }
                at => (self.hosts.get(at - 1)).map(|(_, address)| Deed::Join(address.clone())),
            },
            Step::Address => match at {
                KEY_DELETE => {
                    self.address.pop();
                    None
                }
                KEY_OK => self.join_typed(),
                key => {
                    if self.address.len() < ADDRESS_MOST {
                        self.address.push_str(KEYPAD[key]);
                    }
                    None
                }
            },
            Step::Terms if host => match at {
                0..=2 => {
                    self.turn(true);
                    None
                }
                _ => Some(Deed::Terms(self.terms)),
            },
            Step::Terms => Some(Deed::Leave),
            Step::Accept => Some(Deed::Accept(at == 0)),
            Step::Message => {
                self.go(self.back);
                None
            }
        }
    }

    /// A cancel: the step before.
    fn cancel(&mut self) -> Option<Deed> {
        match self.step {
            Step::Save => None,
            Step::Minions => Some(Deed::Saves),
            Step::Name => {
                self.go(Step::Minions);
                None
            }
            Step::Way => {
                self.go(Step::Name);
                None
            }
            Step::HostHow | Step::JoinHow => {
                self.go(Step::Way);
                None
            }
            Step::Joining | Step::Address | Step::Code => {
                self.go(Step::JoinHow);
                None
            }
            Step::Accept => Some(Deed::Accept(false)),
            Step::Message => {
                self.go(self.back);
                None
            }
            Step::Hosting
            | Step::HostingOnline
            | Step::Connecting
            | Step::Terms
            | Step::Waiting => Some(Deed::Leave),
        }
    }

    /// "Shuffle": the picks in another order.
    pub fn shuffle(&mut self, seed: u32) {
        let mut draws = Random { seed };
        for at in (1..self.picks.len()).rev() {
            let other = draws.draw() as usize % (at + 1);
            self.picks.swap(at, other);
        }
    }

    /// The typed address is joined, once there is one.
    fn join_typed(&mut self) -> Option<Deed> {
        let address = self.address.trim().to_string();
        (!address.is_empty()).then_some(Deed::Join(address))
    }

    /// A key typed on the step of the address: what an address has (the digits and the
    /// letters of an IPv4 or IPv6 address or of a name of a PC, the dot, the colon, the
    /// dash and the brackets) is written, Backspace takes the last one away. Either puts
    /// the keypad's cursor on "OK": Enter is the confirm, which joins there. (Enter's
    /// own typing is left alone, else it would join and then confirm a second time.)
    pub fn type_address(&mut self, typed: Typing) {
        if self.step != Step::Address {
            return;
        }
        match typed {
            Typing::Delete => {
                self.address.pop();
            }
            Typing::Done | Typing::Paste => return,
            Typing::Character(letter)
                if (letter.is_ascii_alphanumeric() || ".:-[]".contains(letter))
                    && self.address.len() < ADDRESS_MOST =>
            {
                self.address.push(letter);
            }
            Typing::Character(_) => return,
        }
        self.cursor = KEY_OK;
    }

    /// A key typed on the step of the code: the letters and digits of a code, the dash
    /// and the space are written, Backspace takes the last one away; either puts the
    /// cursor on "Join", so that Enter joins. Ctrl+V asks for the clipboard (`true`).
    pub fn type_code(&mut self, typed: Typing) -> bool {
        if self.step != Step::Code {
            return false;
        }
        match typed {
            Typing::Delete => {
                self.code.pop();
            }
            Typing::Done => return false,
            Typing::Paste => return true,
            Typing::Character(letter)
                if (letter.is_ascii_alphanumeric() || "- ".contains(letter))
                    && self.code.chars().count() < CODE_MOST =>
            {
                self.code.push(letter);
            }
            Typing::Character(_) => return false,
        }
        self.cursor = CODE_JOIN;
        false
    }

    /// A text of the clipboard into the field of the step: a code (written as the host
    /// shows it when it is one), or an address.
    pub fn paste(&mut self, text: &str) {
        let text: String = text.chars().filter(|letter| !letter.is_control()).collect();
        let text = text.trim();
        match self.step {
            Step::Code => {
                self.code = match Code::parse(text) {
                    Ok(code) => code.to_string(),
                    Err(_) => text.chars().take(CODE_MOST).collect(),
                };
                self.cursor = match self.typed_code() {
                    Some(_) => CODE_JOIN,
                    None => CODE_PASTE,
                };
            }
            Step::Address => {
                self.address = (text.chars())
                    .filter(|letter| letter.is_ascii_alphanumeric() || ".:-[]".contains(*letter))
                    .take(ADDRESS_MOST)
                    .collect();
                self.cursor = KEY_OK;
            }
            _ => {}
        }
    }
}

/// The line of the terms that says how the packets of a room code go.
fn route_line(net: &Net) -> Option<String> {
    Some(match net.route? {
        Route::Direct => "Online: direct.".into(),
        Route::Relayed => "Online: through a relay (fine for this fight).".into(),
    })
}

/// The lobby while it is up: the form, what it is painted with, its picture, the name
/// entry while the name is changed, the list of the network's hosts while it is asked
/// for, and the passes.
#[derive(Resource)]
pub struct Lobby {
    pub form: Form,
    look: Look,
    small: Font,
    tables: Tables,
    picture: Entity,
    /// The save of the team: its memory and the hero's name.
    memory: Option<Memory>,
    hero: String,
    team: Option<Team>,
    naming: Option<NameEntry>,
    discovery: Option<Discovery>,
    lan_port: u16,
    /// A check: this machine only (127.0.0.1).
    only_here: bool,
    args: Vec<String>,
    pad: Option<Pad>,
    passes: u32,
    owed: f64,
    /// Seconds since the step opened, and the buttons a check taps on it.
    time: f64,
    presses: Vec<Press>,
    aim: Aim<usize>,
    /// The rows on screen and their places, for the mouse.
    spots: Vec<(usize, Rect)>,
    /// A check's pictures: where, and whether this step's was taken.
    shots: Option<PathBuf>,
    shot_taken: bool,
    /// Keys typed on the step of the address, for the next pass.
    typed: Vec<Typing>,
}

impl Lobby {
    /// The keyboard types here: on the name entry, and on the step of the address.
    pub fn types(&self) -> bool {
        self.naming.is_some() || matches!(self.form.step, Step::Address | Step::Code)
    }

    /// The name entry while it is up, which takes the keys typed (`name_entry.rs`).
    pub fn naming_mut(&mut self) -> Option<&mut NameEntry> {
        self.naming.as_mut()
    }

    /// Keys typed on the step of the address, for the next pass.
    pub fn type_keys(&mut self, keys: impl IntoIterator<Item = Typing>) {
        self.typed.extend(keys);
    }

    /// A name of the game's state in letters.
    fn letters_of(&self, stored: &[u8; name::LETTERS]) -> String {
        (name::units(stored, &self.small).into_iter())
            .map(crate::name_entry::letter)
            .collect::<String>()
            .trim()
            .to_string()
    }

    /// The step that opens, with the buttons a check taps on it.
    fn go(&mut self, step: Step, presses: &mut LobbyPresses) {
        self.form.go(step);
        self.opened(presses);
    }

    /// The step of the form opened: its buttons and its picture start.
    fn opened(&mut self, presses: &mut LobbyPresses) {
        self.time = 0.0;
        self.presses = presses.take(self.form.step);
        self.shot_taken = false;
        self.aim.forget();
    }

    /// The minions of the save the list took.
    fn take_save(&mut self, picked: &Picked, prefs: &crate::prefs::Prefs) {
        let loaded = &picked.0;
        let mut memory = Memory::zeroed();
        loaded.carried.creatures.write(&mut memory.creatures);
        let kept = loaded.carried.keeping.to_bytes();
        memory.block[36..36 + kept.len()].copy_from_slice(&kept);
        self.hero = (loaded.name.iter().copied())
            .map(crate::name_entry::letter)
            .collect::<String>()
            .trim()
            .to_string();
        let listed = versus::listed(&memory);
        self.form.minions = (listed.iter())
            .map(|(number, creature)| Minion {
                number: *number,
                name: self.letters_of(&creature.record.name),
                level: creature.record.level,
                hp: creature.record.max_hp,
            })
            .collect();
        let first = versus::first_three(&memory);
        self.form.picks = (first.iter())
            .filter_map(|number| (listed.iter()).position(|(listed, _)| listed == number))
            .collect();
        self.memory = Some(memory);
        if self.form.name.is_empty() {
            self.form.name = match (prefs.versus.name.trim(), self.hero.as_str()) {
                ("", "") => "Player".into(),
                ("", hero) => hero.to_string(),
                (kept, _) => kept.to_string(),
            };
        }
        info!(
            "the lobby: the save of {} with {} minions, {:?} picked",
            self.hero,
            self.form.minions.len(),
            self.form.picks
        );
    }

    /// The team of the picks.
    fn make_team(&mut self) -> Result<(), String> {
        let memory = self.memory.as_ref().ok_or("no save was picked")?;
        let numbers: Vec<usize> = (self.form.picks.iter())
            .filter_map(|&pick| self.form.minions.get(pick).map(|minion| minion.number))
            .collect();
        let team = Team::from_memory(&self.tables, memory, &numbers)
            .map_err(|err| format!("the team cannot be made: {err:?}"))?;
        info!(
            "the lobby: the team is the records {numbers:?} with {} kinds of items, hash {:016x}",
            team.items.len(),
            team.hash()
        );
        self.team = Some(team);
        Ok(())
    }

    /// The arena of a way in.
    fn arena(&self, mode: Mode) -> Result<Arena, Failure> {
        let team = self.team.clone().ok_or("no team")?;
        versus_view::open(&Start {
            mode,
            args: self.args.clone(),
            hidden: self.only_here,
            lobby: Some(FromLobby {
                name: self.form.name.clone(),
                team,
            }),
        })
    }

    /// The host's room code goes to the clipboard; a check only says so.
    fn copy(&mut self, code: Code) {
        let said = match self.only_here {
            true => {
                println!("versus: the room code {code} is copied (a check leaves the clipboard)");
                Ok(())
            }
            false => crate::clipboard::set(&code.to_string()),
        };
        self.form.note = Some(match said {
            Ok(()) => "Copied: paste it into the chat.".into(),
            Err(err) => {
                warn!("the lobby: the clipboard: {err}");
                format!("The clipboard cannot be used ({err}).")
            }
        });
    }

    /// The clipboard comes into the field of the step; a check's comes from the file of
    /// `--versus-code-file=`.
    fn paste(&mut self) {
        let text = match self.only_here {
            true => crate::versus_check::code_file(&self.args)
                .and_then(|file| std::fs::read_to_string(file).ok())
                .ok_or_else(|| "a check has no code yet".to_string()),
            false => crate::clipboard::get(),
        };
        match text {
            Ok(text) if !text.trim().is_empty() => {
                self.form.paste(&text);
                info!("the lobby: pasted {:?}", text.trim());
                self.form.note = None;
            }
            Ok(_) => self.form.note = Some("The clipboard is empty.".into()),
            Err(err) => {
                info!("the lobby: nothing to paste: {err}");
                self.form.note = Some("Nothing to paste: copy the code first.".into());
            }
        }
    }

    /// The list of the network's hosts is asked for.
    fn listen(&mut self) {
        let here = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let (targets, from) = match self.only_here {
            true => (vec![SocketAddr::new(here, self.lan_port)], here),
            false => (
                lan::broadcasts(self.lan_port),
                IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            ),
        };
        match Discovery::start(targets, from) {
            Ok(discovery) => self.discovery = Some(discovery),
            Err(err) => warn!("the lobby: no list of the network's hosts: {err}"),
        }
        self.form.hosts.clear();
    }

    /// The hosts heard, as the list shows them.
    fn hear(&mut self) {
        let Some(discovery) = self.discovery.as_mut() else {
            return;
        };
        let hosts: Vec<(String, String)> = (discovery.hosts().into_iter())
            .map(|found| {
                let address = SocketAddr::new(found.address.ip(), found.info.port);
                (found.info.name, address.to_string())
            })
            .collect();
        if hosts != self.form.hosts {
            for (name, address) in hosts.iter().filter(|host| !self.form.hosts.contains(host)) {
                println!("versus: heard {name} at {address}");
            }
            self.form.hosts = hosts;
        }
    }

    /// The screen of this pass, four bytes a pixel.
    fn paint(&mut self, net: &Net, colour: i16) -> Vec<u8> {
        if let Some(entry) = self.naming.as_mut() {
            entry.colour = colour;
            return entry.paint();
        }
        let mut screen = [0, 0, 0, 255].repeat(SCREEN.0 * SCREEN.1);
        let paint = Painter {
            parts: self.look.parts(),
            colour,
            time: menu::frame_time(self.passes, GAME_FRAME),
        };
        let parts = paint.parts;
        parts.wall(&mut screen);
        let header = boxes::HEADER;
        paint.frame(&mut screen, header, PLAIN);
        paint.centred(&mut screen, "Arena Battle Stage", header, header.y + 5);
        let body = boxes::BODY;
        paint.frame(&mut screen, body, PLAIN);
        let (heading, lines) = self.form.page(net);
        paint.centred(&mut screen, &heading, body, body.y + 6);
        let left = body.x + 14;
        let mut y = body.y + 26;
        let wide = body.w - 28;
        let text = |screen: &mut [u8], said: &str, at: (i16, i16)| {
            parts.text(screen, &parts.letters(said), at, WHOLE);
        };
        for line in lines.iter().flat_map(|line| paint.lines(line, wide)) {
            text(&mut screen, &line, (left, y));
            y += LINE;
        }
        // The typed address or code in a field of its own, with the place of the next
        // letter; the host's room code in one too.
        let caret = match (self.passes / 8).is_multiple_of(2) {
            true => "_",
            false => " ",
        };
        let field = match self.form.step {
            Step::Address => Some(format!("{}{caret}", self.form.address)),
            Step::Code => Some(format!("{}{caret}", self.form.code)),
            Step::HostingOnline => {
                Some(net.room.map_or(String::new(), |(code, _)| code.to_string()))
            }
            _ => None,
        };
        if let Some(said) = field {
            let field = Rect {
                x: left - 4,
                y: y + 2,
                w: wide + 8,
                h: 18,
            };
            paint.frame(&mut screen, field, PLAIN);
            match self.form.step {
                Step::HostingOnline => paint.centred(&mut screen, &said, field, field.y + 3),
                _ => text(&mut screen, &said, (left, field.y + 3)),
            }
            y = field.y + field.h + 2;
        }
        y += 4;
        let rows = self.form.rows(net);
        self.spots.clear();
        if self.form.step == Step::Address {
            let key_wide = (wide + 12) / KEYPAD_COLUMNS as i16;
            for (key, row) in rows.iter().enumerate() {
                let (row_at, column) = (key / KEYPAD_COLUMNS, key % KEYPAD_COLUMNS);
                let rect = Rect {
                    x: left - 6 + column as i16 * key_wide,
                    y: y + row_at as i16 * (ROW + 2),
                    w: key_wide,
                    h: ROW,
                };
                paint.frame(&mut screen, rect, PLAIN);
                if key == self.form.cursor {
                    paint.frame(&mut screen, rect, CURSOR);
                }
                for (_, said) in &row.columns {
                    paint.centred(&mut screen, said, rect, rect.y + 2);
                }
                self.spots.push((key, rect));
            }
            y += (KEYPAD.len() / KEYPAD_COLUMNS) as i16 * (ROW + 2);
        }
        for place in self.form.shown(rows.len()) {
            if self.form.step == Step::Address {
                break;
            }
            let Some(row) = rows.get(place) else { continue };
            let rect = Rect {
                x: left - 6,
                y,
                w: wide + 12,
                h: ROW,
            };
            if place == self.form.cursor {
                paint.frame(&mut screen, rect, CURSOR);
            }
            for (offset, said) in &row.columns {
                let units = parts.letters(said);
                parts.text(&mut screen, &units, (left + offset, y + 2), WHOLE);
            }
            if row.dim {
                versus_view::darken(&mut screen, rect);
            }
            self.spots.push((place, rect));
            y += ROW;
        }
        y += 4;
        for line in (self.form.below(net).iter()).flat_map(|line| paint.lines(line, wide)) {
            text(&mut screen, &line, (left, y));
            y += LINE;
        }
        let legend: Vec<(usize, Vec<u16>)> = [
            (CROSS_SIGN, "Choose"),
            (
                TRIANGLE_SIGN,
                if self.form.step == Step::Message {
                    "OK"
                } else {
                    "Back"
                },
            ),
        ]
        .into_iter()
        .map(|(sign, word)| (sign, parts.letters(word)))
        .collect();
        parts.legend(&mut screen, colour, &legend);
        screen
    }
}

/// The parts of the disc the lobby needs besides the arena's: the fights' tables and the
/// small font of the names.
fn read_rules() -> Result<(Tables, Font), Failure> {
    let config = LocalConfig::load()?;
    let mut disc = DiscImage::open(&config.disc_image)?;
    let exe = Executable::read(&mut disc)?;
    let tables =
        Tables::read(&exe.image, exe.base).ok_or("the fights' tables are not in the executable")?;
    let fonts = Fonts::from_executable(&exe.image, exe.base)
        .ok_or("the executable does not hold the font tables")?;
    let small = (fonts.get(text::SMALL_FONT).cloned()).ok_or("there is no small font")?;
    Ok((tables, small))
}

/// This PC's addresses another player can join: the ones the system would send from
/// towards the internet, the private networks and a virtual one such as Tailscale's
/// (a socket that is only connected sends nothing). A check has 127.0.0.1.
fn addresses(only_here: bool) -> Vec<String> {
    if only_here {
        return vec![Ipv4Addr::LOCALHOST.to_string()];
    }
    let probes = [
        "8.8.8.8:80",
        "10.255.255.254:9",
        "172.31.255.254:9",
        "192.168.255.254:9",
        "100.100.100.100:53",
    ];
    let mut found: Vec<String> = Vec::new();
    for probe in probes {
        let Ok(socket) = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)) else {
            continue;
        };
        if socket.connect(probe).is_err() {
            continue;
        }
        let Ok(local) = socket.local_addr() else {
            continue;
        };
        let ip = local.ip();
        let text = ip.to_string();
        if !ip.is_unspecified() && !ip.is_loopback() && !found.contains(&text) {
            found.push(text);
        }
    }
    if found.is_empty() {
        found.push("none found: is the network up?".into());
    }
    found
}

/// The value of a numbered option of a check.
fn number(args: &[String], name: &str) -> Option<u16> {
    (args.iter())
        .find_map(|arg| arg.strip_prefix(name))
        .and_then(|value| value.parse().ok())
}

/// The lobby opens behind the title's "Arena Battle Stage": everything drawn so far goes,
/// its camera and picture come, the music of the original's entry screens plays and the
/// list of saves comes up.
fn open(
    args: Vec<String>,
    only_here: bool,
    prefs: &crate::prefs::Prefs,
    drawn: &Query<Entity, crate::Drawn>,
    (images, commands): (&mut Assets<Image>, &mut Commands),
    music: &mut crate::music::Music,
) -> Result<Lobby, Failure> {
    let look = Look::read()?;
    let (tables, small) = read_rules()?;
    for entity in drawn {
        commands.entity(entity).despawn();
    }
    commands.spawn((Camera2d, LobbyPart));
    let (wide, high) = (SCREEN.0 as u32, SCREEN.1 as u32);
    let black = [0, 0, 0, 255].repeat(SCREEN.0 * SCREEN.1);
    let picture = crate::spawn_picture(commands, images, wide, high, black);
    commands.entity(picture).insert(LobbyPart);
    if let Err(err) = music.start(LOBBY_MUSIC) {
        warn!("the lobby's music: {err}");
    }
    let port = number(&args, "--versus-port=").unwrap_or(prefs.versus.port);
    let lan_port = number(&args, "--versus-lan-port=").unwrap_or(LAN_PORT);
    let shots = (args.iter())
        .find_map(|arg| arg.strip_prefix("--versus-shots="))
        .map(PathBuf::from);
    let form = Form {
        address: prefs.versus.last_address.clone(),
        port,
        ..Form::default()
    };
    Ok(Lobby {
        form,
        look,
        small,
        tables,
        picture,
        memory: None,
        hero: String::new(),
        team: None,
        naming: None,
        discovery: None,
        lan_port,
        only_here,
        args,
        pad: None,
        passes: 0,
        owed: 0.0,
        time: 0.0,
        presses: Vec::new(),
        aim: Aim::default(),
        spots: Vec::new(),
        shots,
        shot_taken: false,
        typed: Vec::new(),
    })
}

/// The list of saves comes up over the lobby, on a new game that nothing writes.
fn open_saves(
    lobby: &mut Lobby,
    (kept, settings, clock): (
        &CardsKept,
        &crate::settings::Settings,
        &crate::saving::PlayClock,
    ),
    (images, commands): (&mut Assets<Image>, &mut Commands),
    presses: &mut LobbyPresses,
) -> Result<(), Failure> {
    let beside = (settings, None, clock);
    let game = crate::saving::Running::of(Carried::new_game(), beside, None, [0; 24])?;
    crate::card_view::for_the_arena(game, kept, settings.0, images, commands)?;
    lobby.go(Step::Save, presses);
    Ok(())
}

/// The way back to the title from the lobby's first step: the lobby's camera stays for
/// the title.
fn to_title(
    lobby: &Lobby,
    music: &mut crate::music::Music,
    again: Vec<Press>,
    (images, commands): (&mut Assets<Image>, &mut Commands),
) -> Result<(), Failure> {
    commands.entity(lobby.picture).despawn();
    commands.remove_resource::<Lobby>();
    music.give_back();
    info!("the lobby: back to the title");
    crate::title::again(again, music, images, commands)
}

/// The arena of the lobby goes, with its session: the other side is told this one
/// leaves.
fn drop_arena(versus: Option<&mut Versus>, commands: &mut Commands) {
    if let Some(versus) = versus {
        versus.finish(LAST_WORD);
    }
    commands.remove_resource::<Versus>();
    commands.remove_resource::<Arena>();
}

/// What the end of a session before the arena says, and nothing for this side's own
/// leave.
fn said_of(end: &End, peer: Option<&str>, joining: &str, host: bool) -> Option<Vec<String>> {
    let other = peer.unwrap_or("The other player");
    Some(match end {
        End::Left => return None,
        End::PeerLeft => vec![format!("{other} left.")],
        End::Disconnected(why) if peer.is_none() && host => {
            vec!["No player came.".into(), why.clone()]
        }
        End::Disconnected(why)
            if peer.is_none() && why.starts_with("No host found for this code") =>
        {
            vec![
                "No host found for this code.".into(),
                "Check the code, and that the other player still hosts.".into(),
            ]
        }
        End::Disconnected(why) if peer.is_none() => {
            // The arena's reason names the address too.
            let why = why.split_once(": ").map_or(why.as_str(), |(_, why)| why);
            vec![format!("No host answered at {joining}."), why.to_string()]
        }
        End::Disconnected(why) => vec!["The connection was lost.".into(), why.clone()],
        End::Refused(why) => vec!["The two games cannot play together:".into(), why.clone()],
        End::RefusedByPeer(why) => vec!["The other game refused this one:".into(), why.clone()],
        End::Desync { turn } => vec![format!("The two games went apart at turn {turn}.")],
    })
}

/// The resources the lobby reads beside its own.
type Kept<'w> = (
    Res<'w, CardsKept>,
    Res<'w, crate::settings::Settings>,
    Res<'w, crate::saving::PlayClock>,
    Option<Res<'w, crate::title::BehindAMovie>>,
);

/// Opens the lobby when the title asks for it, and runs its passes for the time gone by:
/// the list of saves while it is up, the steps, what the session tells, the name entry,
/// and the picture.
#[allow(clippy::too_many_arguments)]
pub fn run_lobby(
    lobby: Option<ResMut<Lobby>>,
    wanted: Option<Res<Wanted>>,
    (mut arena, mut versus): (Option<ResMut<Arena>>, Option<ResMut<Versus>>),
    (card, picked): (Option<Res<CardView>>, Option<Res<Picked>>),
    (kept, settings, clock, again): Kept,
    (mut prefs, prefs_file, mut presses): (
        ResMut<crate::prefs::Prefs>,
        Res<crate::prefs::PrefsFile>,
        ResMut<LobbyPresses>,
    ),
    (keys, time): (Res<crate::keys::Pad>, Res<Time<Virtual>>),
    mut music: ResMut<crate::music::Music>,
    mut images: ResMut<Assets<Image>>,
    mut sprites: Query<&mut Sprite>,
    drawn: Query<Entity, crate::Drawn>,
    pointer: Pointer,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    let again = || {
        again
            .as_ref()
            .map(|again| again.0.clone())
            .unwrap_or_default()
    };
    let Some(mut lobby) = lobby else {
        if wanted.is_some() {
            commands.remove_resource::<Wanted>();
            let args: Vec<String> = std::env::args().collect();
            let only_here = !prefs_file.live();
            let opened = open(
                args,
                only_here,
                &prefs,
                &drawn,
                (&mut images, &mut commands),
                &mut music,
            )
            .and_then(|mut lobby| {
                let beside = (&*kept, &*settings, &*clock);
                open_saves(
                    &mut lobby,
                    beside,
                    (&mut images, &mut commands),
                    &mut presses,
                )?;
                Ok(lobby)
            });
            match opened {
                Ok(lobby) => {
                    info!("the lobby: opens on the list of saves");
                    commands.insert_resource(lobby);
                }
                Err(err) => {
                    error!("the lobby: {err}");
                    exit.write(AppExit::error());
                }
            }
        }
        return;
    };
    let lobby = &mut *lobby;
    let colour = settings.0.colour as i16;
    let passed = crate::pace::passed(&time, LONGEST_PASS);
    lobby.time += passed;
    take_shot(lobby, &mut commands);
    // The list of saves is up: it runs by itself.
    if card.is_some() {
        return;
    }
    if lobby.form.step == Step::Save {
        match &picked {
            Some(picked) => {
                lobby.take_save(picked, &prefs);
                commands.remove_resource::<Picked>();
                lobby.go(Step::Minions, &mut presses);
            }
            // No save was taken: the step before, or the title.
            None if lobby.memory.is_some() => lobby.go(Step::Minions, &mut presses),
            None => {
                if let Err(err) = to_title(lobby, &mut music, again(), (&mut images, &mut commands))
                {
                    error!("the title behind the lobby: {err}");
                    exit.write(AppExit::error());
                }
                return;
            }
        }
    }
    // What the session tells.
    let mut net = Net::default();
    if let Some(arena) = arena.as_deref() {
        net.role = Some(arena.role());
        net.peer = arena.peer().map(str::to_string);
        net.terms = (versus.as_deref()).and_then(|versus| versus.terms());
        net.room = arena.room_code();
        net.internet = arena.internet();
        net.route = (versus.as_deref()).and_then(|versus| versus.route());
        if let Some(end) = arena.ended().filter(|_| !arena.in_the_room()) {
            let host = arena.role() == Role::Host;
            let said = said_of(end, arena.peer(), &lobby.form.joining, host);
            info!("the lobby: the session ends before the arena: {end}");
            println!("versus: ended: {end}");
            drop_arena(versus.as_deref_mut(), &mut commands);
            match said {
                Some(lines) => {
                    lobby.form.message = lines;
                    lobby.form.back = match lobby.form.online && !host {
                        true => Step::Code,
                        false => Step::Way,
                    };
                    lobby.go(Step::Message, &mut presses);
                }
                None => lobby.go(Step::Way, &mut presses),
            }
            return;
        }
    }
    match lobby.form.step {
        Step::Hosting | Step::HostingOnline | Step::Connecting if net.peer.is_some() => {
            info!(
                "the lobby: {} is here",
                net.peer.as_deref().unwrap_or_default()
            );
            lobby.go(Step::Terms, &mut presses);
        }
        Step::Terms if net.role == Some(Role::Guest) && net.terms.is_some() => {
            lobby.go(Step::Accept, &mut presses);
        }
        Step::Joining | Step::Address => lobby.hear(),
        // The list of the network's hosts is only heard on its steps.
        _ => lobby.discovery = None,
    }
    // The mouse over the rows, or over the name entry.
    let under = pointer.on_picture(lobby.picture, SCREEN);
    match lobby.naming.as_mut() {
        Some(entry) => {
            lobby.aim.forget();
            entry.look(pointer.mouse(), under);
        }
        None => {
            let spots = lobby.spots.clone();
            (lobby.aim).look(pointer.mouse(), pointer::row_under(spots, under));
        }
    }
    // The keys typed on the step of the address or the code; Ctrl+V pastes.
    let mut deeds = Vec::new();
    for typed in std::mem::take(&mut lobby.typed) {
        let pasted = typed == Typing::Paste && lobby.form.step == Step::Address;
        if lobby.form.type_code(typed) || pasted {
            lobby.paste();
        }
        lobby.form.type_address(typed);
    }
    let mut held = crate::keys::held(&keys);
    for press in &lobby.presses {
        if press.holds(lobby.time) {
            held |= press.button;
        }
    }
    lobby.pad.get_or_insert_with(|| {
        let mut pad = Pad::default();
        pad.read(held, Repeat::of(GAME_FRAME), 0);
        pad
    });
    lobby.owed += passed * PASSES_A_SECOND;
    let mut painted = lobby.passes == 0;
    while lobby.owed >= 1.0 && deeds.is_empty() {
        lobby.owed -= 1.0;
        lobby.passes += 1;
        painted = true;
        let mut pad = lobby.pad.unwrap_or_default();
        pad.read(held, Repeat::of(GAME_FRAME), lobby.passes);
        lobby.pad = Some(pad);
        if let Some(entry) = lobby.naming.as_mut() {
            if let Some(stored) = entry.pass(&pad, &mut music) {
                let named = lobby.letters_of(&stored);
                if !named.is_empty() {
                    lobby.form.name = named;
                }
                lobby.naming = None;
                lobby.form.cursor = 0;
                info!("the lobby: the name is {:?}", lobby.form.name);
                lobby.opened(&mut presses);
            }
            continue;
        }
        let at = lobby.form.cursor;
        let keypad = lobby.form.step == Step::Address;
        let way = |row: usize| match (keypad, row > at) {
            (true, _) => keypad_way(at, row),
            (false, true) => pad::DOWN,
            (false, false) => pad::UP,
        };
        lobby.aim.steer(&mut pad, at, way);
        let before = lobby.form.step;
        let pass = lobby.form.pass(&pad, &net);
        for &sound in &pass.sounds {
            music.sound(sound.into());
        }
        deeds.extend(pass.deed);
        if lobby.form.step != before {
            lobby.opened(&mut presses);
        }
    }
    for deed in deeds {
        info!("the lobby: {deed:?}");
        let mut arena = arena.as_deref_mut();
        let versus = versus.as_deref_mut();
        match deed {
            Deed::Saves => {
                let beside = (&*kept, &*settings, &*clock);
                if let Err(err) =
                    open_saves(lobby, beside, (&mut images, &mut commands), &mut presses)
                {
                    error!("the lobby's list of saves: {err}");
                }
            }
            Deed::Team => match lobby.make_team() {
                Ok(()) => lobby.go(Step::Name, &mut presses),
                Err(err) => {
                    error!("the lobby: {err}");
                    lobby.form.message = vec![err];
                    lobby.form.back = Step::Minions;
                    lobby.go(Step::Message, &mut presses);
                }
            },
            Deed::NameEntry => {
                let mut wall = [0, 0, 0, 255].repeat(SCREEN.0 * SCREEN.1);
                lobby.look.parts().wall(&mut wall);
                let stored =
                    name::stored(&lobby.look.parts().letters(&lobby.form.name), &lobby.small);
                match crate::name_entry::open_on(&stored, wall, GAME_FRAME) {
                    Ok(entry) => {
                        lobby.naming = Some(entry);
                        lobby.presses = presses.take(Step::Name);
                        lobby.time = 0.0;
                        lobby.shot_taken = false;
                    }
                    Err(err) => error!("the lobby's name entry: {err}"),
                }
            }
            Deed::Named => {
                if prefs.versus.name != lobby.form.name {
                    prefs.versus.name = lobby.form.name.clone();
                }
                lobby.go(Step::Way, &mut presses);
            }
            Deed::Host => match lobby.arena(Mode::Host {
                port: lobby.form.port,
            }) {
                Ok(arena) => {
                    lobby.form.addresses = addresses(lobby.only_here);
                    commands.insert_resource(arena);
                    lobby.go(Step::Hosting, &mut presses);
                }
                Err(err) => {
                    lobby.form.message = vec![
                        format!("This PC cannot host on port {}:", lobby.form.port),
                        err.to_string(),
                    ];
                    lobby.form.back = Step::HostHow;
                    lobby.go(Step::Message, &mut presses);
                }
            },
            Deed::HostOnline => match lobby.arena(Mode::HostOnline) {
                Ok(arena) => {
                    commands.insert_resource(arena);
                    lobby.go(Step::HostingOnline, &mut presses);
                }
                Err(err) => {
                    lobby.form.message =
                        vec!["This PC cannot host online:".into(), err.to_string()];
                    lobby.form.back = Step::HostHow;
                    lobby.go(Step::Message, &mut presses);
                }
            },
            Deed::Copy => {
                if let Some((code, _)) = arena.as_deref().and_then(Arena::room_code) {
                    lobby.copy(code);
                }
            }
            Deed::Paste => lobby.paste(),
            Deed::JoinOnline(code) => {
                lobby.form.back = Step::Code;
                lobby.form.joining = format!("room {code}");
                lobby.form.online = true;
                lobby.discovery = None;
                match lobby.arena(Mode::JoinOnline {
                    code: Some(code.to_string()),
                }) {
                    Ok(arena) => {
                        commands.insert_resource(arena);
                        lobby.go(Step::Connecting, &mut presses);
                    }
                    Err(err) => {
                        lobby.form.message = vec!["The arena cannot open:".into(), err.to_string()];
                        lobby.go(Step::Message, &mut presses);
                    }
                }
            }
            Deed::Listen => {
                lobby.listen();
                lobby.go(Step::Joining, &mut presses);
            }
            Deed::Join(address) => {
                lobby.form.online = false;
                let address = jc_net::link::with_port(&address);
                if lobby.form.step == Step::Address && prefs.versus.last_address != address {
                    prefs.versus.last_address = address.clone();
                }
                lobby.form.back = lobby.form.step;
                lobby.form.joining = address.clone();
                lobby.discovery = None;
                match lobby.arena(Mode::Join { address }) {
                    Ok(arena) => {
                        commands.insert_resource(arena);
                        lobby.go(Step::Connecting, &mut presses);
                    }
                    Err(err) => {
                        lobby.form.message = vec!["The arena cannot open:".into(), err.to_string()];
                        lobby.form.back = Step::Way;
                        lobby.go(Step::Message, &mut presses);
                    }
                }
            }
            Deed::Leave => {
                drop_arena(versus, &mut commands);
                let back = match lobby.form.step {
                    Step::Connecting if lobby.form.online => Step::Code,
                    Step::Connecting => Step::Joining,
                    Step::Hosting | Step::HostingOnline => Step::HostHow,
                    _ => Step::Way,
                };
                if back == Step::Joining {
                    lobby.listen();
                }
                lobby.go(back, &mut presses);
            }
            Deed::Terms(terms) => {
                if let Some(arena) = arena.as_mut() {
                    arena.give_terms(terms);
                }
                println!(
                    "versus: the lobby's terms {:?}, place {:?}",
                    terms.options, terms.place
                );
                lobby.go(Step::Waiting, &mut presses);
            }
            Deed::Accept(yes) => {
                if let Some(arena) = arena.as_mut() {
                    arena.accept(yes);
                }
                match yes {
                    true => lobby.go(Step::Waiting, &mut presses),
                    false => lobby.go(Step::Way, &mut presses),
                }
            }
        }
    }
    if !painted && lobby.naming.is_none() {
        return;
    }
    let screen = lobby.paint(&net, colour);
    if let Ok(mut sprite) = sprites.get_mut(lobby.picture) {
        let (wide, high) = (SCREEN.0 as u32, SCREEN.1 as u32);
        sprite.image = images.add(crate::picture_image(wide, high, screen));
    }
}

/// The pictures a check took of the lobby's steps in this run.
static SHOTS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// A check's picture of each step (`--versus-shots=DIR`), a moment after it opened.
fn take_shot(lobby: &mut Lobby, commands: &mut Commands) {
    let Some(dir) = lobby.shots.clone() else {
        return;
    };
    let due = match lobby.form.step {
        Step::Save => SHOT_AFTER * 2.0,
        _ => SHOT_AFTER,
    };
    if lobby.shot_taken || lobby.time < due {
        return;
    }
    lobby.shot_taken = true;
    let named = match lobby.naming.is_some() {
        true => "name-entry",
        false => lobby.form.step.name(),
    };
    if let Err(err) = std::fs::create_dir_all(&dir) {
        warn!("the lobby: no pictures in {}: {err}", dir.display());
        return;
    }
    // Counted over the run's lobbies, so that a second session keeps the first's.
    let count = SHOTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    let path = dir.join(format!("lobby-{count:02}-{named}.png"));
    info!(
        "the lobby: a picture of the step {named}: {}",
        path.display()
    );
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path));
}

#[cfg(test)]
mod tests {
    use super::*;
    use jc_formats::pad::Repeat;

    /// A pad on which `held` went down in this pass.
    fn pressed(held: u16) -> Pad {
        let mut pad = Pad::default();
        pad.read(0, Repeat::of(GAME_FRAME), 0);
        pad.read(held, Repeat::of(GAME_FRAME), 1);
        pad
    }

    fn minions(count: usize) -> Vec<Minion> {
        (0..count)
            .map(|number| Minion {
                number: number * 2,
                name: format!("M{number}"),
                level: 10,
                hp: 100,
            })
            .collect()
    }

    #[test]
    fn the_presses_of_a_check_name_their_steps_and_come_in_turn() {
        let mut presses =
            LobbyPresses::of("minions=0.5:cross;joining=1:down,1.2:cross;joining=0.5:cross")
                .expect("presses");
        assert_eq!(presses.take(Step::Joining).len(), 2);
        assert_eq!(presses.take(Step::Joining).len(), 1);
        assert_eq!(presses.take(Step::Joining).len(), 1);
        assert_eq!(presses.take(Step::Minions).len(), 1);
        assert!(presses.take(Step::Way).is_empty());
        assert!(LobbyPresses::of("nowhere=1:cross").is_none());
        assert!(LobbyPresses::of("way=1:jump").is_none());
    }

    #[test]
    fn up_to_three_minions_are_picked_in_order_and_done_needs_one() {
        let net = Net::default();
        let mut form = Form {
            step: Step::Minions,
            minions: minions(5),
            ..Form::default()
        };
        // Done with no pick is refused.
        form.cursor = 6;
        let passed = form.pass(&pressed(pad::CROSS), &net);
        assert_eq!((passed.deed, passed.sounds), (None, vec![SOUND_REFUSED]));
        for place in [3, 0, 4] {
            form.cursor = place;
            form.pass(&pressed(pad::CROSS), &net);
        }
        assert_eq!(form.picks, [3, 0, 4]);
        // A fourth is refused; a picked one is taken out again.
        form.cursor = 1;
        assert_eq!(
            form.pass(&pressed(pad::CROSS), &net).sounds,
            [SOUND_REFUSED]
        );
        form.cursor = 0;
        form.pass(&pressed(pad::CROSS), &net);
        assert_eq!(form.picks, [3, 4]);
        form.cursor = 6;
        assert_eq!(form.pass(&pressed(pad::CROSS), &net).deed, Some(Deed::Team));
        // A cancel goes back to the list of saves.
        assert_eq!(
            form.pass(&pressed(pad::TRIANGLE), &net).deed,
            Some(Deed::Saves)
        );
    }

    #[test]
    fn a_shuffle_keeps_the_picks_and_the_long_list_keeps_its_cursor_on_screen() {
        let net = Net::default();
        let mut form = Form {
            step: Step::Minions,
            minions: minions(12),
            picks: vec![0, 1, 2],
            ..Form::default()
        };
        form.shuffle(7);
        let mut sorted = form.picks.clone();
        sorted.sort();
        assert_eq!(sorted, [0, 1, 2]);
        for _ in 0..9 {
            form.pass(&pressed(pad::DOWN), &net);
        }
        assert_eq!(form.cursor, 9);
        assert_eq!(form.first, 9 + 1 - SHOWN_MINIONS);
        let shown = form.shown(form.rows(&net).len());
        assert!(shown.contains(&9) && shown.contains(&12) && shown.contains(&13));
        assert!(!shown.contains(&0));
    }

    #[test]
    fn the_host_turns_the_terms_and_ready_gives_them() {
        let net = Net {
            role: Some(Role::Host),
            peer: Some("Koris".into()),
            ..Net::default()
        };
        let mut form = Form {
            step: Step::Terms,
            ..Form::default()
        };
        form.pass(&pressed(pad::RIGHT), &net);
        assert!(!form.terms.options.items);
        form.pass(&pressed(pad::DOWN), &net);
        form.pass(&pressed(pad::CROSS), &net);
        assert!(!form.terms.options.changing);
        form.pass(&pressed(pad::DOWN), &net);
        form.pass(&pressed(pad::LEFT), &net);
        assert_eq!(form.terms.place, Some(38));
        form.pass(&pressed(pad::RIGHT), &net);
        form.pass(&pressed(pad::RIGHT), &net);
        assert_eq!(form.terms.place, Some(0));
        form.pass(&pressed(pad::DOWN), &net);
        let terms = form.terms;
        assert_eq!(
            form.pass(&pressed(pad::CROSS), &net).deed,
            Some(Deed::Terms(terms))
        );
        // A cancel leaves the session.
        assert_eq!(
            form.pass(&pressed(pad::TRIANGLE), &net).deed,
            Some(Deed::Leave)
        );
    }

    #[test]
    fn the_places_go_round_through_random() {
        assert_eq!(next_place(None, true), Some(0));
        assert_eq!(next_place(Some(38), true), None);
        assert_eq!(next_place(Some(0), false), None);
        assert_eq!(next_place(None, false), Some(38));
        assert_eq!(place_word(Some(11)), "12 of 39");
    }

    #[test]
    fn an_address_is_typed_on_the_keyboard_with_ipv6_and_names() {
        let net = Net::default();
        let mut form = Form {
            step: Step::Address,
            ..Form::default()
        };
        for letter in "192.168.1.5:4782 0/".chars() {
            form.type_address(Typing::Character(letter));
        }
        assert_eq!(form.address, "192.168.1.5:47820");
        form.type_address(Typing::Delete);
        // Enter types nothing: it is the confirm, and the keyboard put the cursor on OK.
        form.type_address(Typing::Done);
        assert_eq!(form.cursor, KEY_OK);
        assert_eq!(
            form.pass(&pressed(pad::CROSS), &net).deed,
            Some(Deed::Join("192.168.1.5:4782".into()))
        );
        form.address.clear();
        for letter in "[fe80::1a2B]:5000 my-pc".chars() {
            form.type_address(Typing::Character(letter));
        }
        assert_eq!(form.address, "[fe80::1a2B]:5000my-pc");
        // Nothing is typed on another step.
        form.go(Step::Way);
        form.type_address(Typing::Character('9'));
        assert_eq!(form.address, "[fe80::1a2B]:5000my-pc");
    }

    #[test]
    fn the_keypad_types_an_address_with_the_pad() {
        let net = Net::default();
        let mut form = Form {
            step: Step::Address,
            ..Form::default()
        };
        // "1", right to "2", down to "9", right twice to the dot, right to the colon, left
        // twice to "0".
        let presses = [
            pad::CROSS,
            pad::RIGHT,
            pad::CROSS,
            pad::DOWN,
            pad::CROSS,
            pad::RIGHT,
            pad::RIGHT,
            pad::CROSS,
            pad::RIGHT,
            pad::CROSS,
            pad::LEFT,
            pad::LEFT,
            pad::CROSS,
        ];
        for press in presses {
            assert_eq!(form.pass(&pressed(press), &net).deed, None);
        }
        assert_eq!(form.address, "129.:0");
        // Del takes the last one away, OK joins; left from the first key goes round.
        form.cursor = 0;
        form.pass(&pressed(pad::LEFT), &net);
        assert_eq!(form.cursor, 6);
        form.cursor = KEY_DELETE;
        form.pass(&pressed(pad::CROSS), &net);
        form.pass(&pressed(pad::RIGHT), &net);
        assert_eq!(
            form.pass(&pressed(pad::CROSS), &net).deed,
            Some(Deed::Join("129.:".into()))
        );
        // OK with nothing typed joins nothing, and a cancel goes back to the ways to join.
        form.address.clear();
        assert_eq!(form.pass(&pressed(pad::CROSS), &net).deed, None);
        assert_eq!(form.pass(&pressed(pad::TRIANGLE), &net).deed, None);
        assert_eq!(form.step, Step::JoinHow);
        // The mouse steers along a row first, then across.
        assert_eq!(keypad_way(0, 13), pad::DOWN);
        assert_eq!(keypad_way(7, 13), pad::RIGHT);
        assert_eq!(keypad_way(13, 8), pad::LEFT);
        assert_eq!(keypad_way(9, 2), pad::UP);
    }

    #[test]
    fn the_guest_sees_the_terms_and_says_ready_or_leaves() {
        let net = Net {
            role: Some(Role::Guest),
            peer: Some("Levant".into()),
            terms: Some(Terms::default()),
            route: Some(Route::Relayed),
            ..Net::default()
        };
        let mut form = Form {
            step: Step::Accept,
            ..Form::default()
        };
        let (_, lines) = form.page(&net);
        assert!(lines.iter().any(|line| line.contains("Rules: Save items")));
        assert!(lines.iter().any(|line| line.contains("through a relay")));
        assert_eq!(
            form.pass(&pressed(pad::CROSS), &net).deed,
            Some(Deed::Accept(true))
        );
        form.pass(&pressed(pad::DOWN), &net);
        assert_eq!(
            form.pass(&pressed(pad::CROSS), &net).deed,
            Some(Deed::Accept(false))
        );
    }

    #[test]
    fn an_end_before_the_arena_says_why_but_not_for_a_leave_of_ones_own() {
        assert_eq!(said_of(&End::Left, None, "", true), None);
        let refused = said_of(&End::RefusedByPeer("build 0.0.0".into()), None, "", false);
        assert!(refused.is_some_and(|lines| lines[1].contains("0.0.0")));
        let none = said_of(
            &End::Disconnected("timed out".into()),
            None,
            "10.0.0.2:47820",
            false,
        );
        assert!(none.is_some_and(|lines| lines[0].contains("10.0.0.2:47820")));
        let gone = said_of(&End::PeerLeft, Some("Koris"), "", true);
        assert_eq!(gone, Some(vec!["Koris left.".to_string()]));
        let nobody = said_of(
            &End::Disconnected("No host found for this code (timed out)".into()),
            None,
            "room k3fm-q8xa-7wpe",
            false,
        );
        assert_eq!(
            nobody.map(|lines| lines[0].clone()).as_deref(),
            Some("No host found for this code.")
        );
    }

    #[test]
    fn host_and_join_put_the_room_code_first_and_lead_to_their_steps() {
        let net = Net::default();
        let mut form = Form {
            step: Step::Way,
            ..Form::default()
        };
        // Host: the room code first, then the network and the address.
        assert_eq!(form.pass(&pressed(pad::CROSS), &net).deed, None);
        assert_eq!(form.step, Step::HostHow);
        let rows = form.rows(&net);
        assert_eq!(rows[0], Row::of("Online (room code)"));
        assert_eq!(
            form.pass(&pressed(pad::CROSS), &net).deed,
            Some(Deed::HostOnline)
        );
        form.pass(&pressed(pad::DOWN), &net);
        assert_eq!(form.pass(&pressed(pad::CROSS), &net).deed, Some(Deed::Host));
        // Back, then join: the code, the network's list, the address.
        form.pass(&pressed(pad::TRIANGLE), &net);
        assert_eq!(form.step, Step::Way);
        form.pass(&pressed(pad::DOWN), &net);
        form.pass(&pressed(pad::CROSS), &net);
        assert_eq!(form.step, Step::JoinHow);
        let rows: Vec<Row> = form.rows(&net);
        assert_eq!(
            rows,
            [
                Row::of("Online (room code)"),
                Row::of("Same network"),
                Row::of("By address")
            ]
        );
        form.pass(&pressed(pad::DOWN), &net);
        assert_eq!(
            form.pass(&pressed(pad::CROSS), &net).deed,
            Some(Deed::Listen)
        );
        form.go(Step::JoinHow);
        form.pass(&pressed(pad::DOWN), &net);
        form.pass(&pressed(pad::DOWN), &net);
        form.pass(&pressed(pad::CROSS), &net);
        assert_eq!(form.step, Step::Address);
        form.pass(&pressed(pad::TRIANGLE), &net);
        assert_eq!(form.step, Step::JoinHow);
        form.pass(&pressed(pad::CROSS), &net);
        assert_eq!(form.step, Step::Code);
    }

    #[test]
    fn a_room_code_is_pasted_with_the_pad_and_joined() {
        let net = Net::default();
        let mut form = Form {
            step: Step::Code,
            ..Form::default()
        };
        // "Join" is dim without a code.
        form.pass(&pressed(pad::DOWN), &net);
        assert_eq!(
            form.pass(&pressed(pad::CROSS), &net).sounds,
            [SOUND_REFUSED]
        );
        form.pass(&pressed(pad::UP), &net);
        assert_eq!(
            form.pass(&pressed(pad::CROSS), &net).deed,
            Some(Deed::Paste)
        );
        // The clipboard as a chat gives it: capitals, no dashes, a line's end.
        form.paste(
            "K3FMQ8XA7WPE
",
        );
        assert_eq!(form.code, "k3fm-q8xa-7wpe");
        assert_eq!(form.cursor, CODE_JOIN);
        let code = Code::parse("k3fm-q8xa-7wpe").expect("a code");
        assert_eq!(
            form.pass(&pressed(pad::CROSS), &net).deed,
            Some(Deed::JoinOnline(code))
        );
        // Something that is no code stays as it came, and the cursor on "Paste".
        form.paste("hello there");
        assert_eq!(
            (form.code.as_str(), form.cursor),
            ("hello there", CODE_PASTE)
        );
        assert!(form.below(&net)[0].starts_with("Not a code yet"));
    }

    #[test]
    fn a_room_code_is_typed_on_the_keyboard_and_ctrl_v_asks_for_the_clipboard() {
        let net = Net::default();
        let mut form = Form {
            step: Step::Code,
            ..Form::default()
        };
        for letter in "k3fm q8xa/7wpe".chars() {
            assert!(!form.type_code(Typing::Character(letter)));
        }
        assert_eq!(form.code, "k3fm q8xa7wpe");
        assert_eq!(form.cursor, CODE_JOIN);
        assert_eq!(form.below(&net), ["Room k3fm-q8xa-7wpe: Join."]);
        form.type_code(Typing::Delete);
        assert!(form.typed_code().is_none());
        assert!(form.type_code(Typing::Paste));
        // Nothing is typed on another step.
        form.go(Step::Way);
        assert!(!form.type_code(Typing::Character('x')));
        assert!(!form.type_code(Typing::Paste));
        assert_eq!(form.code, "k3fm q8xa7wp");
    }

    #[test]
    fn the_host_shows_its_code_and_says_when_it_can_be_found() {
        let code = Code::parse("k3fm-q8xa-7wpe").expect("a code");
        let mut net = Net {
            role: Some(Role::Host),
            room: Some((code, false)),
            ..Net::default()
        };
        let mut form = Form {
            step: Step::HostingOnline,
            ..Form::default()
        };
        assert_eq!(form.below(&net), ["Going online..."]);
        net.room = Some((code, true));
        assert!(form.below(&net)[0].contains("any chat"));
        assert_eq!(form.pass(&pressed(pad::CROSS), &net).deed, Some(Deed::Copy));
        assert_eq!(
            form.pass(&pressed(pad::TRIANGLE), &net).deed,
            Some(Deed::Leave)
        );
    }
}
