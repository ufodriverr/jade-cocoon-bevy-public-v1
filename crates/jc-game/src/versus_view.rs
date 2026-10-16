
use std::collections::BTreeSet;
use std::error::Error;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use jc_disc::{Archive, DiscImage, Executable, LocalConfig};
use jc_formats::battle::Tables;
use jc_formats::battle::duel::{Choice, Duel};
use jc_formats::carried::Carried;
use jc_formats::menu::{Frames, Rect, Style, frame_time};
use jc_formats::name;
use jc_formats::pad;
use jc_formats::text::{self, Font, Fonts};
use jc_formats::tim;
use jc_formats::versus::Series;
use jc_net::lan::{self, Discovery, HostInfo, Responder};
use jc_net::link::{CONNECT_TIMEOUT, Connecting, Link, Listener, Stream, Timing, host_address};
use jc_net::online::{Code, Hosting, Joining, Reach};
use jc_net::session::{End, Local as Answering, Notice, Role, Setup, VersusSession};
use jc_net::upnp::Forward;
use jc_net::wire::Terms;
use jc_net::{LAN_PORT, read_rules_hash};

use crate::actors::Figure;
use crate::name_entry::{Picture, SCREEN, WHOLE};
use crate::net::Versus;
use crate::options_view::Parts;
use crate::place_map::picture;
use crate::scripts::{FightPlace, VersusSetup, VersusShown, VersusSpot, dice};
use crate::shown::Things;
use crate::versus_check::{self, Bot, Mode};
use jc_formats::versus::Team;

const ARENA_ROOM: u16 = 345;
/// How long a host waits for its guest, and a guest for its host.
const WAIT_FOR_PEER: Duration = Duration::from_secs(90);
/// The same from the lobby: a host waits until the player cancels, a guest tries the
/// address this long.
const HOST_WAITS: Duration = Duration::from_secs(24 * 60 * 60);
const GUEST_TRIES: Duration = Duration::from_secs(20);
/// How long the leaving game waits for its last messages to go out.
const LAST_WORD: Duration = Duration::from_millis(300);
/// The frames the end of this side's own leave stands before the title.
const LEFT_FRAMES: u32 = 10;
/// The game frames the bot of a check waits with its command once the menu is up, and
/// with its answer once the end of a fight is on screen.
const BOT_THINKS: u32 = 30;
const BOT_ANSWERS: u32 = 60;
/// The frames a hidden check stays behind the end, for its last pictures.
const LAST_FRAMES: u32 = 90;
/// Where the build's boxes lie on the depth axis of the field's screen: in front of the
/// menu of the fight.
const IN_FRONT: f32 = 9.6;
/// The modes of a frame of the menus.
pub(crate) const PLAIN: u8 = 0;
pub(crate) const CURSOR: u8 = 1;

/// Whether the arguments ask for the view: `--versus=` with a window.
pub fn shown(args: &[String]) -> bool {
    args.iter().any(|arg| arg.starts_with("--versus="))
        && args.iter().any(|arg| {
            arg == "--versus-view" || arg.starts_with("--smoke") || arg.starts_with("--screenshot=")
        })
}

/// The minions' figures of the room of the arena, the local side's and the other's, as
/// the session's teams give them; read when the room is described (`describe_room`).
static ARENA: Mutex<Option<(Vec<Figure>, Vec<Figure>)>> = Mutex::new(None);

/// The figures of the arena, while one is asked for.
pub fn figures() -> Option<(Vec<Figure>, Vec<Figure>)> {
    ARENA.lock().ok()?.clone()
}

/// The connection while it is made.
enum Connect {
    Hosting {
        listener: Listener,
        _answering: Option<Responder>,
        until: Instant,
    },
    Joining {
        address: String,
        connecting: Option<Connecting>,
        again: Instant,
        until: Instant,
    },
    Asking {
        discovery: Discovery,
        until: Instant,
    },
    /// A host under a room code; `told` once it can be found.
    HostingOnline {
        hosting: Hosting,
        until: Instant,
        told: bool,
    },
    /// A guest of a room code: the code (none: a check's file, until it is there), then
    /// the search for its host.
    JoiningOnline {
        code: Option<String>,
        joining: Option<Joining>,
        until: Instant,
    },
    Done,
}

/// Where the room of the arena stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Room {
    None,
    Asked,
    Ready,
}

/// The fight of two players of this app: the session's resource is `net::Versus`, this
/// holds the rest.
#[derive(Resource)]
pub struct Arena {
    role: Role,
    setup: Option<Setup>,
    tables: Tables,
    exe: Executable,
    disc_image: PathBuf,
    bot: Bot,
    /// The bot answers in place of the pad.
    plays: bool,
    connect: Connect,
    /// The players' names, by side.
    names: [String; 2],
    choice: Option<Choice>,
    rematch: Option<bool>,
    /// The fight whose "Rematch?" the bot has answered.
    answered: u32,
    refused: bool,
    room: Room,
    /// A fight runs, and the fights that ended.
    runs: bool,
    round: u32,
    played: u32,
    end: Option<End>,
    end_frames: u32,
    /// A run nobody plays, which ends by itself.
    hidden: bool,
    shots: Option<PathBuf>,
    taken: BTreeSet<String>,
    /// The camera and the picture of the screen before the room, and what it says.
    camera: Option<Entity>,
    picture: Option<Entity>,
    status: String,
    /// The buttons held in the frame before, for a press on the end's message.
    held: u16,
    /// The arena came from the lobby: the lobby shows everything before the room, and
    /// answers the host's terms and the guest's accept.
    lobby: bool,
    terms_given: Option<Terms>,
    accepted: Option<bool>,
    /// The other side's hello passed.
    met: bool,
    /// The arena is over: [`leave_arena`] takes it away and opens the title.
    leaving: bool,
    /// A check from the lobby ends behind this many sessions (`--versus-sessions=K`).
    sessions: Option<u32>,
    /// A check quits the app at this turn of a fight (`--versus-quit-at=T`).
    quit_at: Option<u32>,
    /// The reach of a room code's endpoint, and a check's file of the code.
    reach: Reach,
    code_file: Option<PathBuf>,
    /// A host by address in real play asks the router to forward its port (UPnP), and
    /// the internet address once it does; taken back when the arena goes.
    forward: Option<Forward>,
    internet: Option<SocketAddr>,
}

impl Arena {
    /// A fight is on: the close button asks first (`guards.rs`).
    pub fn match_runs(&self) -> bool {
        self.runs && self.end.is_none()
    }

    /// The other player's name, once the hello passed.
    pub fn peer(&self) -> Option<&str> {
        self.met.then(|| self.other_name())
    }

    /// The local side's role.
    pub fn role(&self) -> Role {
        self.role
    }

    /// The internet address of a host by address, once the router forwards its port.
    pub fn internet(&self) -> Option<SocketAddr> {
        self.internet
    }

    /// The room code of a host that hosts under one, and whether a guest can find it
    /// yet (the host reached its relay).
    pub fn room_code(&self) -> Option<(Code, bool)> {
        match &self.connect {
            Connect::HostingOnline { hosting, told, .. } => Some((hosting.code(), *told)),
            _ => None,
        }
    }

    /// How the session ended, when it did.
    pub fn ended(&self) -> Option<&End> {
        self.end.as_ref()
    }

    /// The room of the arena is asked for: the lobby is through.
    pub fn in_the_room(&self) -> bool {
        self.room != Room::None
    }

    /// The host's terms, given on the lobby's "Ready".
    pub fn give_terms(&mut self, terms: Terms) {
        self.terms_given = Some(terms);
    }

    /// The guest's answer to the host's terms: "Ready" or leave.
    pub fn accept(&mut self, yes: bool) {
        self.accepted = Some(yes);
    }

    fn side(&self) -> usize {
        self.role.side()
    }

    fn other_name(&self) -> &str {
        &self.names[1 - self.side()]
    }

    fn role_name(&self) -> &'static str {
        match self.role {
            Role::Host => "host",
            Role::Guest => "guest",
        }
    }

    /// The lines of the message of an end.
    fn end_lines(&self) -> Option<Vec<String>> {
        let end = self.end.as_ref()?;
        let other = self.other_name();
        let mut lines = match end {
            End::Left => vec!["You left the arena.".to_string()],
            End::PeerLeft => vec![format!("{other} left the arena.")],
            End::Disconnected(why) => vec!["The connection was lost.".into(), why.clone()],
            End::Refused(why) => vec!["The other game was refused.".into(), why.clone()],
            End::RefusedByPeer(why) => vec!["The other game refused this one.".into(), why.clone()],
            End::Desync { turn } => vec![
                format!("Desync at turn {turn}:"),
                "the two games went apart.".into(),
            ],
        };
        if !self.hidden {
            lines.push("Confirm returns to the title.".into());
        }
        Some(lines)
    }
}

/// The local side's answers to the session.
struct Answers<'a> {
    bot: &'a mut Bot,
    plays: bool,
    choice: &'a mut Option<Choice>,
    rematch: &'a mut Option<bool>,
    refused: &'a mut bool,
    ready: bool,
    /// The lobby's answers, when the arena came from it: the host's terms and the
    /// guest's accept.
    lobby: Option<(Option<Terms>, &'a mut Option<bool>)>,
}

impl Answering for Answers<'_> {
    fn terms(&mut self) -> Option<Terms> {
        match &self.lobby {
            Some((terms, _)) => *terms,
            None => Some(self.bot.terms),
        }
    }

    fn accept(&mut self, _: &Terms) -> Option<bool> {
        match &mut self.lobby {
            Some((_, accepted)) => accepted.take(),
            None => Some(true),
        }
    }

    fn team(&mut self) -> Option<jc_formats::versus::Team> {
        Some(self.bot.team.clone())
    }

    fn ready(&mut self, _: u32) -> bool {
        self.ready
    }

    fn choose(&mut self, tables: &Tables, duel: &Duel, side: usize) -> Option<Choice> {
        let choice = self.choice.take()?;
        match duel.check(tables, side, choice) {
            Ok(()) => Some(choice),
            Err(why) => {
                warn!("the arena: the duel refuses {choice:?}: {why:?}");
                *self.refused = true;
                None
            }
        }
    }

    fn rematch(&mut self, _: &Series) -> Option<bool> {
        self.rematch.take()
    }

    fn bits(&mut self) -> u32 {
        match self.plays {
            true => self.bot.half_seed(),
            false => jc_net::fresh_bits(),
        }
    }
}

/// What the arena is started with.
#[derive(Resource)]
pub struct Start {
    pub mode: Mode,
    pub args: Vec<String>,
    /// A check: the host binds this machine only, and a run from the command line ends
    /// by itself.
    pub hidden: bool,
    /// What the lobby gives: the player's name and team. None from the command line.
    pub lobby: Option<FromLobby>,
}

/// The player's name and team, as the lobby made them.
pub struct FromLobby {
    pub name: String,
    pub team: Team,
}

/// The arena of the lobby: what it needs is read and the connection starts.
pub fn open(start: &Start) -> Result<Arena, Box<dyn Error>> {
    Arena::start(start)
}

/// Reads what the arena needs and starts the connection, behind a screen that says so.
pub fn start_arena(start: Res<Start>, mut commands: Commands, mut exit: MessageWriter<AppExit>) {
    match Arena::start(&start) {
        Ok(mut arena) => {
            arena.camera = Some(commands.spawn(Camera2d).id());
            commands.insert_resource(arena);
        }
        Err(err) => {
            println!("versus: {err}");
            error!("the arena cannot start: {err}");
            exit.write(AppExit::error());
        }
    }
}

impl Arena {
    fn start(start: &Start) -> Result<Self, Box<dyn Error>> {
        let args = &start.args;
        let config = LocalConfig::load()?;
        let mut disc = DiscImage::open(&config.disc_image)?;
        let exe = Executable::read(&mut disc)?;
        let tables = Tables::read(&exe.image, exe.base)
            .ok_or("the fights' tables are not in the executable")?;
        let rules = read_rules_hash(&mut disc)?;
        let role = start.mode.role();
        let mut setup = versus_check::setup_of(args, role, rules)?;
        let mut bot = Bot::of(&tables, args, role.side())?;
        if let Some(lobby) = &start.lobby {
            setup.name = lobby.name.clone();
            bot.team = lobby.team.clone();
        }
        let from_lobby = start.lobby.is_some();
        let plays = args.iter().any(|arg| arg.starts_with("--versus-bot"));
        let lan_port = versus_check::number(args, "--versus-lan-port=", u32::from(LAN_PORT))?;
        let lan_port = u16::try_from(lan_port)?;
        // A check binds this machine only, which opens no firewall question.
        let only_here = start.hidden;
        let here = match only_here {
            true => IpAddr::V4(Ipv4Addr::LOCALHOST),
            false => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        };
        let waits = match (from_lobby, &start.mode) {
            (false, _) => WAIT_FOR_PEER,
            (true, Mode::Host { .. } | Mode::HostOnline) => HOST_WAITS,
            (true, _) => GUEST_TRIES,
        };
        let reach = versus_check::reach_of(args, only_here);
        let code_file = versus_check::code_file(args).filter(|_| start.hidden);
        let until = Instant::now() + waits;
        let mut forward = None;
        let (connect, status) = match &start.mode {
            Mode::Host { port } => {
                let listener = Listener::start(host_address(*port, only_here))?;
                // Never in a check: the router is the player's.
                let upnp = !args.iter().any(|arg| arg == "--versus-upnp=off");
                if !only_here && upnp {
                    forward = Some(Forward::start(listener.address().port()));
                }
                let info = HostInfo {
                    name: setup.name.clone(),
                    build: setup.build.clone(),
                    port: listener.address().port(),
                };
                let answering = Responder::start(here, lan_port, info)
                    .inspect_err(|err| warn!("the arena: not listed on the network: {err}"))
                    .ok();
                let status = format!("Hosting on port {}.", listener.address().port());
                println!("versus: hosting on {}", listener.address());
                (
                    Connect::Hosting {
                        listener,
                        _answering: answering,
                        until,
                    },
                    status,
                )
            }
            Mode::Join { address } => (
                Connect::Joining {
                    address: address.clone(),
                    connecting: None,
                    again: Instant::now(),
                    until,
                },
                format!("Connecting to {address}."),
            ),
            Mode::JoinLan => {
                let targets = match only_here {
                    true => vec![SocketAddr::new(here, lan_port)],
                    false => lan::broadcasts(lan_port),
                };
                (
                    Connect::Asking {
                        discovery: Discovery::start(targets, here)?,
                        until,
                    },
                    "Looking for a host on the network.".into(),
                )
            }
            Mode::HostOnline => {
                let hosting = Hosting::start(reach)?;
                let code = hosting.code();
                println!("versus: hosting under the room code {code}");
                (
                    Connect::HostingOnline {
                        hosting,
                        until,
                        told: false,
                    },
                    format!("Room code {code}. Going online."),
                )
            }
            Mode::JoinOnline { code } => (
                Connect::JoiningOnline {
                    code: code.clone(),
                    joining: None,
                    until,
                },
                "Looking for the host of the room code.".into(),
            ),
        };
        let name = setup.name.clone();
        println!(
            "versus: {name}, rules hash {rules:016x}, team hash {:016x}",
            bot.team.hash()
        );
        let shots = (args.iter())
            .find_map(|arg| arg.strip_prefix("--versus-shots="))
            .map(PathBuf::from);
        Ok(Self {
            role,
            setup: Some(setup),
            tables,
            exe,
            disc_image: config.disc_image,
            bot,
            plays,
            connect,
            names: match role {
                Role::Host => [name, "Guest".into()],
                Role::Guest => ["Host".into(), name],
            },
            choice: None,
            rematch: None,
            answered: 0,
            refused: false,
            room: Room::None,
            runs: false,
            round: 0,
            played: 0,
            end: None,
            end_frames: 0,
            // A check from the lobby goes back to the title as a play session does.
            hidden: start.hidden && !from_lobby,
            shots,
            taken: BTreeSet::new(),
            camera: None,
            picture: None,
            status,
            held: 0,
            lobby: from_lobby,
            terms_given: None,
            accepted: None,
            met: false,
            leaving: false,
            sessions: (args.iter())
                .find_map(|arg| arg.strip_prefix("--versus-sessions="))
                .and_then(|value| value.parse().ok())
                .filter(|_| start.hidden),
            quit_at: (args.iter())
                .find_map(|arg| arg.strip_prefix("--versus-quit-at="))
                .and_then(|value| value.parse().ok())
                .filter(|_| start.hidden),
            reach,
            code_file,
            forward,
            internet: None,
        })
    }

    /// A frame of the connection while it is made: the session once it is.
    fn connect(&mut self) -> Option<Result<VersusSession, String>> {
        let now = Instant::now();
        if self.internet.is_none()
            && let Some(answer) = self.forward.as_mut().and_then(Forward::poll)
        {
            match answer {
                Ok(internet) => {
                    println!("versus: the router forwards the port: {internet}");
                    self.internet = Some(internet);
                }
                Err(why) => {
                    println!("versus: no forward of the port by the router: {why}");
                    self.forward = None;
                }
            }
        }
        let stream: Result<Stream, String> = match &mut self.connect {
            Connect::Done => return None,
            Connect::Hosting {
                listener, until, ..
            } => match listener.poll() {
                Some(stream) => stream.map(Stream::from).map_err(|err| err.to_string()),
                None if now > *until => Err("no player came".into()),
                None => return None,
            },
            Connect::Joining {
                address,
                connecting,
                again,
                until,
            } => {
                if connecting.is_none() && now >= *again {
                    *connecting = Some(Connecting::start(address, CONNECT_TIMEOUT));
                }
                match connecting.as_ref().and_then(Connecting::poll) {
                    Some(Ok(stream)) => Ok(stream.into()),
                    Some(Err(err)) if now > *until => Err(format!("no host at {address}: {err}")),
                    // The host may not listen yet: once more in a moment.
                    Some(Err(_)) => {
                        *connecting = None;
                        *again = now + Duration::from_millis(250);
                        return None;
                    }
                    None => return None,
                }
            }
            Connect::Asking { discovery, until } => match discovery.hosts().into_iter().next() {
                Some(found) => {
                    println!(
                        "versus: heard {} (build {}) at {}",
                        found.info.name, found.info.build, found.address
                    );
                    self.status = format!("Connecting to {}.", found.info.name);
                    self.connect = Connect::Joining {
                        address: found.address.to_string(),
                        connecting: None,
                        again: now,
                        until: now + WAIT_FOR_PEER,
                    };
                    return None;
                }
                None if now > *until => Err("no host answered on the network".into()),
                None => return None,
            },
            Connect::HostingOnline {
                hosting,
                until,
                told,
            } => {
                if !*told && hosting.online() {
                    *told = true;
                    println!("versus: online: the code can be found");
                    self.status = format!("Room code {}. Waiting for a player.", hosting.code());
                    if let Some(file) = &self.code_file {
                        versus_check::write_code(file, hosting.code());
                    }
                }
                match hosting.poll() {
                    Some(stream) => stream.map(Stream::from).map_err(|err| err.to_string()),
                    None if now > *until => Err("no player came".into()),
                    None => return None,
                }
            }
            Connect::JoiningOnline {
                code,
                joining,
                until,
            } => {
                if joining.is_none() {
                    let text = code
                        .clone()
                        .or_else(|| (self.code_file.as_deref()).and_then(versus_check::read_code));
                    match text.map(|text| Code::parse(&text)) {
                        Some(Ok(code)) => {
                            println!("versus: joining the room code {code}");
                            match Joining::start(code, self.reach) {
                                Ok(started) => *joining = Some(started),
                                Err(err) => return Some(Err(err.to_string())),
                            }
                        }
                        Some(Err(err)) => {
                            return Some(Err(format!("No host found for this code ({err})")));
                        }
                        None if now > *until => {
                            return Some(Err("no room code came".into()));
                        }
                        None => return None,
                    }
                }
                let found = joining.as_ref().and_then(Joining::poll)?;
                found.map(Stream::from).map_err(|err| err.to_string())
            }
        };
        self.connect = Connect::Done;
        let stream = match stream {
            Ok(stream) => stream,
            Err(why) => return Some(Err(why)),
        };
        println!("versus: connected with {}", stream.describe());
        let link = match Link::new(stream, Timing::default()) {
            Ok(link) => link,
            Err(err) => return Some(Err(err.to_string())),
        };
        let setup = self.setup.take()?;
        self.status = "Connected. Waiting for the other player.".into();
        Some(Ok(VersusSession::new(self.role, Box::new(link), setup)))
    }
}

/// What a room is put on screen with.
type RoomAssets<'w> = (
    ResMut<'w, Assets<Image>>,
    ResMut<'w, Assets<Mesh>>,
    ResMut<'w, Assets<crate::field::AddedMaterial>>,
);

/// One frame of the arena: the connection, the session with the local side's answers,
/// the room of the arena and what the session tells the fight on screen.
#[allow(clippy::too_many_arguments)]
pub fn run_arena(
    arena: Option<ResMut<Arena>>,
    versus: Option<ResMut<Versus>>,
    mut things: Option<ResMut<Things>>,
    keys: Res<crate::keys::Pad>,
    (mut images, mut meshes, mut materials): RoomAssets,
    mut music: ResMut<crate::music::Music>,
    lobby: Query<Entity, With<crate::lobby::LobbyPart>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(mut arena) = arena else {
        return;
    };
    let arena = &mut *arena;
    let held = crate::keys::held(&keys);
    let pressed = held & !std::mem::replace(&mut arena.held, held);
    // The end: its message, then the title, or the end of a check.
    if arena.end.is_some() {
        arena.end_frames += 1;
        if let Some(mut versus) = versus {
            take_moments(arena, things.as_deref_mut(), &mut commands);
            let _ = versus.take_notices();
        }
        // Before the room the lobby says what ended and takes the arena away.
        if arena.lobby && arena.room == Room::None {
            return;
        }
        if !arena.hidden {
            // This side's own leave goes at once; any other end waits for a confirm on
            // its message (the bot of a check gives it a while later).
            let own = matches!(arena.end, Some(End::Left)) && arena.end_frames >= LEFT_FRAMES;
            let confirmed = arena.end_frames > 25 && pressed & pad::CROSS != 0;
            let bot = arena.plays && arena.end_frames >= BOT_ANSWERS;
            if !arena.leaving && (own || confirmed || bot) {
                info!(
                    "the arena: the session is over ({:?}), back to the title",
                    arena.end
                );
                arena.leaving = true;
            }
            return;
        }
        if arena.end_frames == LAST_FRAMES {
            let planned = matches!(arena.end, Some(End::Left | End::PeerLeft))
                && arena.played >= arena.bot.matches;
            println!(
                "versus: the view ends after {} of {} fights: {}",
                arena.played,
                arena.bot.matches,
                arena.end.as_ref().map_or(String::new(), End::to_string)
            );
            exit.write(match planned {
                true => AppExit::Success,
                false => AppExit::error(),
            });
        }
        return;
    }
    let Some(mut versus) = versus else {
        match arena.connect() {
            Some(Ok(session)) => commands.insert_resource(Versus(session)),
            Some(Err(why)) => {
                println!("versus: {why}");
                arena.end = Some(End::Disconnected(why));
            }
            None => {}
        }
        return;
    };
    let side = arena.side();
    // The room of the arena is ready once its scripts are stopped.
    if arena.room == Room::Asked
        && let Some(things) = things.as_mut()
        && things.for_versus()
    {
        info!("the arena: the room is ready");
        arena.room = Room::Ready;
    }
    if let Some(things) = things.as_mut() {
        // What the pad, the keys or the mouse gave comes first, the bot's of a check
        // only where they gave nothing.
        if let Some(choice) = things.take_versus_choice() {
            arena.choice = Some(choice);
        }
        if let Some(answer) = things.take_versus_rematch() {
            arena.rematch = Some(answer);
            arena.answered = arena.round;
        }
        // The bot of a check: a command a while after the menu came up, and the answer
        // to "Rematch?" a while after the end.
        if arena.plays {
            let menu = things.versus_menu_frames();
            if arena.choice.is_none()
                && menu.is_some_and(|frames| frames >= BOT_THINKS)
                && versus.local_turn().is_some()
                && let Some(duel) = versus.duel()
            {
                arena.choice = Some(arena.bot.pick(&arena.tables, duel, side));
            }
            let over = things.versus_over_frames();
            if over.is_some_and(|frames| frames >= BOT_ANSWERS) && arena.answered < arena.round {
                arena.answered = arena.round;
                arena.rematch = Some(arena.bot.again(versus.series()));
            }
        }
    }
    let mut answers = Answers {
        bot: &mut arena.bot,
        plays: arena.plays,
        choice: &mut arena.choice,
        rematch: &mut arena.rematch,
        refused: &mut arena.refused,
        ready: arena.room == Room::Ready,
        lobby: (arena.lobby).then_some((arena.terms_given, &mut arena.accepted)),
    };
    versus.update(&arena.tables, &mut answers);
    if std::mem::take(&mut arena.refused)
        && let Some(things) = things.as_mut()
    {
        things.versus_refused();
    }
    for notice in versus.take_notices() {
        versus_check::log(&versus, &notice);
        match notice {
            Notice::Met { name } => {
                arena.met = true;
                arena.names[1 - side] = name;
                arena.status = format!("{} is here. Waiting for the teams.", arena.other_name());
            }
            Notice::Teams => {
                let teams = versus.teams();
                let figures = |side: usize| {
                    (teams[side].as_ref()).map_or_else(Vec::new, |team| {
                        team.members.iter().map(Figure::of).collect::<Vec<_>>()
                    })
                };
                if let Ok(mut asked) = ARENA.lock() {
                    *asked = Some((figures(side), figures(1 - side)));
                }
                arena.status = "Loading the arena.".into();
                for entity in [arena.camera.take(), arena.picture.take()]
                    .into_iter()
                    .flatten()
                {
                    commands.entity(entity).despawn();
                }
                // The lobby is through: its screen goes, and the room comes.
                if arena.lobby {
                    for entity in &lobby {
                        commands.entity(entity).despawn();
                    }
                    commands.remove_resource::<crate::lobby::Lobby>();
                }
                let carried = Carried::new_game();
                let mut show = crate::ShowRoom::entered(ARENA_ROOM, &carried);
                crate::put_room(
                    &mut show,
                    &carried,
                    (&mut images, &mut meshes, &mut materials),
                    &mut commands,
                    &mut music,
                    &mut exit,
                );
                commands.insert_resource(crate::Game(carried));
                commands.insert_resource(show);
                arena.room = Room::Asked;
            }
            Notice::Started { round, place, .. } => {
                arena.round = round;
                arena.runs = true;
                let place = DiscImage::open(&arena.disc_image)
                    .ok()
                    .and_then(|mut disc| FightPlace::of_place(&mut disc, &arena.exe, place.into()));
                let (Some(place), Some(duel), Some(things)) =
                    (place, versus.duel(), things.as_mut())
                else {
                    error!("the arena: fight {round} cannot be shown (no place, duel or room)");
                    continue;
                };
                things.start_versus(VersusSetup {
                    side,
                    names: arena.names.clone(),
                    round,
                    series: *versus.series(),
                    duel: duel.clone(),
                    step: versus.step(),
                    place,
                });
            }
            Notice::Turn { .. } | Notice::Over { .. } => {
                if matches!(notice, Notice::Over { .. }) {
                    arena.runs = false;
                    arena.played += 1;
                }
                if let (Some(duel), Some(things)) = (versus.duel(), things.as_mut()) {
                    things.versus_update(duel, versus.step(), *versus.series());
                }
            }
            Notice::Agreed(_) => {}
            Notice::Ended(end) => {
                arena.runs = false;
                arena.end = Some(end);
            }
        }
    }
    if arena.end.is_none()
        && let Some(end) = versus.ended()
    {
        arena.end = Some(end.clone());
    }
    // A check's side quits in the middle of a fight, as the close button would.
    if arena.match_runs()
        && let Some(at) = arena.quit_at
        && versus.duel().is_some_and(|duel| duel.turn() >= at)
    {
        arena.quit_at = None;
        println!("versus: this side quits the app at turn {at} of a fight");
        exit.write(AppExit::Success);
    }
    take_moments(arena, things.as_deref_mut(), &mut commands);
}

/// The pictures of a check at the moments of a fight (`--versus-shots=DIR`): the first of
/// each moment in each fight.
fn take_moments(arena: &mut Arena, things: Option<&mut Things>, commands: &mut Commands) {
    let Some(things) = things else {
        return;
    };
    let moments = things.take_versus_moments();
    let Some(dir) = arena.shots.clone() else {
        return;
    };
    for moment in moments {
        let name = format!("{}-{}-{}", arena.role_name(), arena.round, moment.name());
        if !arena.taken.insert(name.clone()) {
            continue;
        }
        if let Err(err) = std::fs::create_dir_all(&dir) {
            warn!("the arena: no pictures in {}: {err}", dir.display());
            continue;
        }
        let path = dir.join(format!("{name}.png"));
        info!("the arena: a picture of {:?}: {}", moment, path.display());
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
    }
    // The last picture of a check comes a little after the end's message.
    if arena.end_frames == 20 {
        let name = format!("{}-end", arena.role_name());
        if arena.taken.insert(name.clone()) {
            let path = dir.join(format!("{name}.png"));
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path));
        }
    }
}

/// The arena is over ([`Arena`]'s `leaving`): the way back to the title as a change of
/// the game's state, as the field's end behind "End Game" is (`main.rs`). The session
/// says its last word and goes with its connection, its threads and the answering of
/// "who hosts?"; the room of the arena goes with everything it drew; the figures of the
/// arena are forgotten, so that no later room has them; and the title opens on its
/// "press Start" screen (`title::again`). A check from the lobby ends behind the
/// sessions it asked for.
#[allow(clippy::too_many_arguments)]
pub fn leave_arena(
    arena: Option<Res<Arena>>,
    versus: Option<ResMut<Versus>>,
    drawn: Query<Entity, crate::Drawn>,
    mut music: ResMut<crate::music::Music>,
    mut images: ResMut<Assets<Image>>,
    again: Option<Res<crate::title::BehindAMovie>>,
    mut sessions: Local<u32>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(arena) = arena.filter(|arena| arena.leaving) else {
        return;
    };
    if let Some(mut versus) = versus {
        versus.finish(LAST_WORD);
    }
    commands.remove_resource::<Versus>();
    commands.remove_resource::<Arena>();
    if let Ok(mut asked) = ARENA.lock() {
        *asked = None;
    }
    for entity in &drawn {
        commands.entity(entity).despawn();
    }
    commands.remove_resource::<Things>();
    commands.remove_resource::<crate::field::FieldScreen>();
    commands.remove_resource::<crate::field::RoomViews>();
    commands.remove_resource::<crate::field::WalkLines>();
    commands.remove_resource::<crate::ShowRoom>();
    commands.remove_resource::<crate::Game>();
    music.field_ends();
    *sessions += 1;
    println!("versus: back to the title after session {}", *sessions);
    info!(
        "the arena: session {} is over, the room, the connection and the arena's pictures \
         are gone; the title opens",
        *sessions
    );
    if arena.sessions.is_some_and(|wanted| *sessions >= wanted) {
        println!("versus: the check ends after {} sessions", *sessions);
        exit.write(AppExit::Success);
        return;
    }
    commands.spawn(Camera2d);
    let presses = again.map(|again| again.0.clone()).unwrap_or_default();
    if let Err(err) = crate::title::again(presses, &mut music, &mut images, &mut commands) {
        error!("the title behind the arena: {err}");
        exit.write(AppExit::error());
    }
}

/// The disc's parts the build's boxes are painted with.
pub(crate) struct Look {
    sprites: Vec<Picture>,
    frames: Frames,
    font: Font,
    font_picture: Picture,
}

impl Look {
    pub(crate) fn read() -> Result<Self, Box<dyn Error>> {
        let config = LocalConfig::load()?;
        let mut disc = DiscImage::open(&config.disc_image)?;
        let archive = Archive::open(&mut disc)?;
        let exe = Executable::read(&mut disc)?;
        let fonts = Fonts::from_executable(&exe.image, exe.base)
            .ok_or("the executable does not hold the font tables")?;
        let frames = Frames::from_executable(&exe.image, exe.base)
            .ok_or("the executable does not hold the tables of the frames")?;
        let font = (fonts.get(text::TEXT_FONT).cloned()).ok_or("there is no font of the texts")?;
        let font_set = tim::texture_set(&archive.read(&mut disc, text::FONT_SET)?)
            .ok_or("the font file is not a texture set")?;
        let sprite_set = tim::texture_set(&archive.read(&mut disc, name::SPRITES)?)
            .ok_or("the sprite file is not a texture set")?;
        let sprites = (sprite_set.iter())
            .map(Picture::of)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            font_picture: picture(&font_set, font.image)?,
            sprites,
            frames,
            font,
        })
    }

    pub(crate) fn parts(&self) -> Parts<'_> {
        Parts {
            sprites: &self.sprites,
            frames: &self.frames,
            font: &self.font,
            font_picture: &self.font_picture,
        }
    }
}

/// The boxes of the build over the fight, in console pixels.
mod boxes {
    use jc_formats::menu::Rect;

    pub const DICE: Rect = Rect {
        x: 60,
        y: 52,
        w: 200,
        h: 124,
    };
    pub const CHOOSING: Rect = Rect {
        x: 70,
        y: 148,
        w: 180,
        h: 22,
    };
    pub const GIVE_UP: Rect = Rect {
        x: 220,
        y: 164,
        w: 84,
        h: 18,
    };
    pub const QUESTION: Rect = Rect {
        x: 50,
        y: 76,
        w: 220,
        h: 70,
    };
    pub const RESULT: Rect = Rect {
        x: 40,
        y: 52,
        w: 240,
        h: 108,
    };
    pub const END: Rect = Rect {
        x: 24,
        y: 80,
        w: 272,
        h: 76,
    };
    pub const WAITING: Rect = Rect {
        x: 24,
        y: 84,
        w: 272,
        h: 72,
    };

    /// The frames of Yes and No at the foot of a box.
    pub fn answers(of: Rect) -> [Rect; 2] {
        let y = of.y + of.h - 24;
        let middle = of.x + of.w / 2;
        [
            Rect {
                x: middle - 56 - 8,
                y,
                w: 56,
                h: 18,
            },
            Rect {
                x: middle + 8,
                y,
                w: 56,
                h: 18,
            },
        ]
    }
}

/// What the boxes are painted with in a frame.
pub(crate) struct Painter<'a> {
    pub(crate) parts: Parts<'a>,
    pub(crate) colour: i16,
    pub(crate) time: u32,
}

impl Painter<'_> {
    /// A frame of the menus: plain, or the cursor's that pulses.
    pub(crate) fn frame(&self, screen: &mut [u8], rect: Rect, mode: u8) {
        let style = Style {
            kind: self.colour,
            mode,
            blend: 1,
        };
        let time = if mode == CURSOR { self.time } else { 0 };
        self.parts.frame(screen, rect, style, time);
    }

    /// A text of the build in the middle of a line of a box.
    pub(crate) fn centred(&self, screen: &mut [u8], said: &str, rect: Rect, y: i16) {
        let width = self.parts.width_of(said);
        let at = (rect.x + (rect.w - width) / 2, y);
        self.parts
            .text(screen, &self.parts.letters(said), at, WHOLE);
    }

    /// The words of a text cut into lines no wider than `wide`.
    pub(crate) fn lines(&self, said: &str, wide: i16) -> Vec<String> {
        let mut lines = Vec::new();
        let mut line = String::new();
        for word in said.split_whitespace() {
            let longer = match line.is_empty() {
                true => word.to_string(),
                false => format!("{line} {word}"),
            };
            if self.parts.width_of(&longer) > wide && !line.is_empty() {
                lines.push(std::mem::replace(&mut line, word.to_string()));
            } else {
                line = longer;
            }
        }
        if !line.is_empty() {
            lines.push(line);
        }
        lines
    }

    /// A box with lines of text, over a darker picture, as high as its lines (and the
    /// answers under them): the box it takes.
    fn said(&self, screen: &mut [u8], rect: Rect, said: &[String], answers: bool) -> Rect {
        let lines: Vec<String> = (said.iter())
            .flat_map(|said| self.lines(said, rect.w - 16))
            .collect();
        let rect = fitted(rect, lines.len(), answers);
        darken(screen, rect);
        self.frame(screen, rect, PLAIN);
        for (n, line) in lines.iter().enumerate() {
            self.centred(screen, line, rect, rect.y + 6 + 14 * n as i16);
        }
        rect
    }

    /// Yes and No at the foot of a box, the cursor on yes or no.
    fn answers(&self, screen: &mut [u8], rect: Rect, yes: bool) {
        let [on_yes, on_no] = boxes::answers(rect);
        self.frame(screen, on_yes, if yes { CURSOR } else { PLAIN });
        self.frame(screen, on_no, if yes { PLAIN } else { CURSOR });
        self.centred(screen, "Yes", on_yes, on_yes.y + 3);
        self.centred(screen, "No", on_no, on_no.y + 3);
    }

    /// The dice of the head start: the names, the dice that tumble and land, and what
    /// the roll says.
    fn dice(&self, screen: &mut [u8], shown: &VersusShown) {
        let Some(dice) = &shown.dice else {
            return;
        };
        let rect = boxes::DICE;
        darken(screen, rect);
        self.frame(screen, rect, PLAIN);
        self.centred(screen, "Who goes first?", rect, rect.y + 6);
        let columns = [rect.x + rect.w / 4 + 6, rect.x + rect.w * 3 / 4 - 6];
        let roll = dice.rolls.get(dice.roll).copied().unwrap_or([1, 1]);
        let last = dice.roll + 1 == dice.rolls.len();
        let landed = dice.into >= dice::TUMBLE;
        let ground = rect.y + 76;
        for (n, &x) in columns.iter().enumerate() {
            let name = Rect {
                x: x - 50,
                w: 100,
                ..rect
            };
            self.centred(screen, &shown.names[n], name, rect.y + 24);
            let face = match landed {
                true => roll[n],
                false => tumbling((dice.roll as u32) << 8 | (dice.into / 3) << 1 | n as u32),
            };
            // The die falls and bounces lower each time until it lands.
            let (lift, sway) = match landed {
                true => (0, 0),
                false => {
                    let share = dice.into as f32 / dice::TUMBLE as f32;
                    let bounce = (share * 3.0 * std::f32::consts::PI).cos().abs();
                    let lift = ((1.0 - share) * 34.0 * bounce) as i16;
                    let sway = ((1.0 - share) * 5.0 * (dice.into as f32 * 1.7).sin()) as i16;
                    (lift, sway)
                }
            };
            shadow(screen, (x, ground + 15), 14 - lift / 4);
            let won = last && landed && (n == 0) == dice.won;
            die(screen, (x + sway, ground - lift), face, won);
        }
        if landed && dice.into >= dice::TUMBLE + dice::LANDED {
            let y = rect.y + rect.h - 30;
            match last {
                false => self.centred(screen, "Draw! Again", rect, y),
                true => {
                    let first = &shown.names[usize::from(!dice.won)];
                    self.centred(screen, &format!("{first} goes first!"), rect, y);
                    if dice.equally_fast {
                        self.centred(screen, "Equally fast!", rect, y + 14);
                    }
                }
            }
        }
    }

    /// The small sign of the other side choosing, with a spinner.
    fn choosing(&self, screen: &mut [u8], shown: &VersusShown, frames: u32) {
        let rect = boxes::CHOOSING;
        darken(screen, rect);
        self.frame(screen, rect, PLAIN);
        let said = format!("{} is choosing...", shown.names[1]);
        let width = self.parts.width_of(&said);
        let x = rect.x + (rect.w - width + 18) / 2;
        spinner(screen, (x - 12, rect.y + rect.h / 2), frames);
        self.parts
            .text(screen, &self.parts.letters(&said), (x, rect.y + 5), WHOLE);
    }

    /// Everything the build draws over the fight in this game frame.
    fn over_the_fight(&self, screen: &mut [u8], shown: &VersusShown, end: Option<&[String]>) {
        self.dice(screen, shown);
        if let Some(frames) = shown.choosing {
            self.choosing(screen, shown, frames);
        }
        if shown.menu && shown.give_up.is_none() {
            let rect = boxes::GIVE_UP;
            self.frame(screen, rect, PLAIN);
            self.centred(screen, "Give up", rect, rect.y + 3);
        }
        if let Some(end) = end {
            self.said(screen, boxes::END, end, false);
            return;
        }
        if let Some(yes) = shown.give_up {
            let lines = ["Give up this fight?".into(), "It counts as a loss.".into()];
            let rect = self.said(screen, boxes::QUESTION, &lines, true);
            self.answers(screen, rect, yes);
        }
        if let Some(over) = &shown.over {
            let rect = boxes::RESULT;
            let [local, other] = &shown.names;
            let (winner, loser) = match over.won {
                true => (local, other),
                false => (other, local),
            };
            let mut lines = vec![format!("{winner} wins!")];
            if over.gave_up {
                lines.push(format!("{loser} gave up."));
            }
            lines.push(format!("{local} {} - {} {other}", over.wins, over.losses));
            if over.frames >= 20 {
                match over.answered {
                    None => lines.push("Rematch?".into()),
                    Some(true) => lines.push(format!("Waiting for {other}...")),
                    Some(false) => lines.push("Leaving the arena.".into()),
                }
            }
            let asks = over.frames >= 20 && over.answered.is_none();
            let rect = self.said(screen, rect, &lines, asks);
            if asks {
                self.answers(screen, rect, over.yes);
            }
        }
    }
}

/// A box as high as its lines of text and the answers under them.
fn fitted(rect: Rect, lines: usize, answers: bool) -> Rect {
    let high = 10 + 14 * lines as i16 + if answers { 26 } else { 0 };
    Rect { h: high, ..rect }
}

/// A face while the die tumbles, from the game frame alone (never the duel's draws).
fn tumbling(seed: u32) -> u8 {
    let mixed = seed.wrapping_mul(0x9E37_79B1) ^ seed.wrapping_mul(0x85EB_CA6B) >> 13;
    (mixed >> 9) as u8 % 6 + 1
}

/// The screen at a third of its brightness inside a rectangle, half see-through where
/// the fight shows.
pub(crate) fn darken(screen: &mut [u8], rect: Rect) {
    for y in rect.y.max(0)..(rect.y + rect.h).min(WHOLE.h) {
        for x in rect.x.max(0)..(rect.x + rect.w).min(WHOLE.w) {
            let at = (y as usize * SCREEN.0 + x as usize) * 4;
            let pixel = &mut screen[at..at + 4];
            if pixel[3] == 0 {
                pixel.copy_from_slice(&[0, 0, 0, 144]);
            } else {
                for part in &mut pixel[..3] {
                    *part /= 3;
                }
            }
        }
    }
}

fn put(screen: &mut [u8], x: i16, y: i16, colour: [u8; 4]) {
    if !(0..WHOLE.w).contains(&x) || !(0..WHOLE.h).contains(&y) {
        return;
    }
    let at = (y as usize * SCREEN.0 + x as usize) * 4;
    screen[at..at + 4].copy_from_slice(&colour);
}

/// A die of the build's own drawing: an ivory cube with dark pips (a red one for a 1),
/// its edge gold on the side that goes first.
fn die(screen: &mut [u8], (cx, cy): (i16, i16), face: u8, won: bool) {
    const SIZE: i16 = 28;
    let (left, top) = (cx - SIZE / 2, cy - SIZE / 2);
    let edge = match won {
        true => [232, 184, 64, 255],
        false => [60, 40, 30, 255],
    };
    for y in 0..SIZE {
        for x in 0..SIZE {
            // Round corners of three pixels.
            let (dx, dy) = ((x.min(SIZE - 1 - x)), (y.min(SIZE - 1 - y)));
            if dx < 3 && dy < 3 && (3 - dx) * (3 - dx) + (3 - dy) * (3 - dy) > 10 {
                continue;
            }
            let colour = match () {
                _ if dx == 0 || dy == 0 => edge,
                _ if won && (dx == 1 || dy == 1) => edge,
                _ if x >= SIZE - 3 || y >= SIZE - 3 => [196, 186, 160, 255],
                _ => [238, 230, 206, 255],
            };
            put(screen, left + x, top + y, colour);
        }
    }
    let q = SIZE / 4 + 1;
    let pips: &[(i16, i16)] = match face {
        1 => &[(0, 0)],
        2 => &[(-q, -q), (q, q)],
        3 => &[(-q, -q), (0, 0), (q, q)],
        4 => &[(-q, -q), (q, -q), (-q, q), (q, q)],
        5 => &[(-q, -q), (q, -q), (0, 0), (-q, q), (q, q)],
        _ => &[(-q, -q), (q, -q), (-q, 0), (q, 0), (-q, q), (q, q)],
    };
    let (pip, radius) = match face {
        1 => ([176, 32, 32, 255], 3),
        _ => ([44, 28, 24, 255], 2),
    };
    for &(px, py) in pips {
        for y in -radius..=radius {
            for x in -radius..=radius {
                if x * x + y * y <= radius * radius + 1 {
                    put(screen, cx + px + x, cy + py + y, pip);
                }
            }
        }
    }
}

/// The shadow of a die on the ground, smaller the higher it is.
fn shadow(screen: &mut [u8], (cx, cy): (i16, i16), half: i16) {
    let half = half.max(4);
    for y in -2..=2i16 {
        for x in -half..=half {
            if x * x * 4 / (half * half).max(1) + y * y <= 4 {
                put(screen, cx + x, cy + y, [0, 0, 0, 120]);
            }
        }
    }
}

/// Eight dots round a point, one of them lit, turning.
fn spinner(screen: &mut [u8], (cx, cy): (i16, i16), frames: u32) {
    const AROUND: [(i16, i16); 8] = [
        (0, -5),
        (4, -4),
        (5, 0),
        (4, 4),
        (0, 5),
        (-4, 4),
        (-5, 0),
        (-4, -4),
    ];
    let lit = (frames / 2) as usize % AROUND.len();
    for (n, &(x, y)) in AROUND.iter().enumerate() {
        let colour = match (n + AROUND.len() - lit) % AROUND.len() {
            0 => [248, 232, 160, 255],
            1 | 7 => [176, 160, 112, 255],
            _ => [88, 80, 64, 255],
        };
        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            put(screen, cx + x + dx, cy + y + dy, colour);
        }
    }
}

/// Where the mouse finds the build's boxes in this frame.
fn spots(shown: &VersusShown) -> Vec<(VersusSpot, Rect)> {
    let answers = |rect: Rect, lines: usize| {
        let [yes, no] = boxes::answers(fitted(rect, lines, true));
        vec![(VersusSpot::Yes, yes), (VersusSpot::No, no)]
    };
    if shown.give_up.is_some() {
        return answers(boxes::QUESTION, 2);
    }
    if let Some(over) = &shown.over
        && over.frames >= 20
        && over.answered.is_none()
    {
        return answers(boxes::RESULT, 3 + usize::from(over.gave_up));
    }
    match shown.menu {
        true => vec![(VersusSpot::GiveUp, boxes::GIVE_UP)],
        false => Vec::new(),
    }
}

/// What paints the arena's boxes: the disc's parts, the picture over the fight and the
/// one of the screen before the room, and what was last painted.
#[derive(Default)]
pub struct ArenaPainter {
    look: Option<Look>,
    failed: bool,
    sprite: Option<Entity>,
    painted: Option<(Option<VersusShown>, Vec<String>, u32)>,
}

/// Paints the screen before the room, or the build's boxes over the fight, in the window
/// colour of the game's settings.
#[allow(clippy::too_many_arguments)]
pub fn paint_arena(
    arena: Option<ResMut<Arena>>,
    mut things: Option<ResMut<Things>>,
    settings: Res<crate::settings::Settings>,
    mut painter: Local<ArenaPainter>,
    mut images: ResMut<Assets<Image>>,
    mut sprites: Query<&mut Sprite>,
    mut commands: Commands,
    pointer: crate::pointer::Pointer,
    real: Res<Time<Real>>,
) {
    let Some(mut arena) = arena else {
        return;
    };
    let painter = &mut *painter;
    // A new session paints anew: the pictures of the last one went with its room.
    if arena.is_added() {
        (painter.sprite, painter.painted) = (None, None);
    }
    if painter.failed {
        return;
    }
    if painter.look.is_none() {
        match Look::read() {
            Ok(look) => painter.look = Some(look),
            Err(err) => {
                error!("the arena's boxes cannot be painted: {err}");
                painter.failed = true;
                return;
            }
        }
    }
    let Some(look) = &painter.look else {
        return;
    };
    let colour = settings.0.colour as i16;
    let spinning = (real.elapsed_secs() * 25.0) as u32;
    let paint = Painter {
        parts: look.parts(),
        colour,
        time: frame_time(spinning, 2),
    };
    let end = arena.end_lines();
    let on = things.as_ref().is_some_and(|things| things.versus_on());
    // Before the fight: a screen of its own that says what the arena waits for; the
    // lobby's, when the arena came from it.
    if !on && arena.lobby && arena.room == Room::None {
        return;
    }
    if !on {
        let lines = end.unwrap_or_else(|| {
            vec![
                "Arena Battle".into(),
                arena.status.clone(),
                format!(
                    "{}{}",
                    "Please wait",
                    ".".repeat((spinning / 12 % 4) as usize)
                ),
            ]
        });
        let key = (None, lines.clone(), spinning / 12);
        if painter.painted.as_ref() == Some(&key) {
            return;
        }
        let mut screen = [0, 0, 0, 255].repeat(SCREEN.0 * SCREEN.1);
        paint.said(&mut screen, boxes::WAITING, &lines, false);
        let image = images.add(crate::picture_image(
            SCREEN.0 as u32,
            SCREEN.1 as u32,
            screen,
        ));
        match arena
            .picture
            .and_then(|entity| sprites.get_mut(entity).ok())
        {
            Some(mut sprite) => sprite.image = image,
            None if arena.room == Room::None => {
                let wide = SCREEN.0 as u32;
                let high = SCREEN.1 as u32;
                let blank = [0, 0, 0, 255].repeat(SCREEN.0 * SCREEN.1);
                let entity = crate::spawn_picture(&mut commands, &mut images, wide, high, blank);
                arena.picture = Some(entity);
                painter.painted = None;
                return;
            }
            None => {}
        }
        painter.painted = Some(key);
        return;
    }
    let Some(things) = things.as_mut() else {
        return;
    };
    let shown = things.versus_shown();
    // The mouse over the boxes, once a frame, while there are any: a check's mouse
    // (`--point-from-menu`) counts from the moment they come up.
    if let Some(shown) = &shown {
        let spots = spots(shown);
        let things = things.bypass_change_detection();
        match spots.is_empty() {
            true => things.versus_point(pointer.mouse(), None, false),
            false => {
                let under = crate::pointer::row_under(spots, pointer.on_fight());
                things.versus_point(pointer.mouse(), under, true);
            }
        }
    }
    let lines = end.unwrap_or_default();
    let key = (shown.clone(), lines.clone(), spinning);
    if painter.painted.as_ref().is_some_and(|painted| {
        painted.0 == key.0 && painted.1 == key.1 && (lines.is_empty() || painted.2 == key.2)
    }) {
        return;
    }
    let mut screen = [0, 0, 0, 0].repeat(SCREEN.0 * SCREEN.1);
    let paint = Painter {
        time: frame_time(shown.as_ref().map_or(0, |shown| shown.game_frame), 2),
        ..paint
    };
    if let Some(shown) = &shown {
        let end = (!lines.is_empty()).then_some(&lines[..]);
        paint.over_the_fight(&mut screen, shown, end);
    } else if !lines.is_empty() {
        paint.said(&mut screen, boxes::END, &lines, false);
    }
    let image = images.add(crate::picture_image(
        SCREEN.0 as u32,
        SCREEN.1 as u32,
        screen,
    ));
    match painter
        .sprite
        .and_then(|sprite| sprites.get_mut(sprite).ok())
    {
        Some(mut sprite) => sprite.image = image,
        None => {
            let picture = (
                Sprite::from_image(image),
                Transform::from_xyz(0.0, 0.0, IN_FRONT),
            );
            painter.sprite = Some(commands.spawn(picture).id());
        }
    }
    painter.painted = Some(key);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| arg.to_string()).collect()
    }

    #[test]
    fn the_view_is_the_versus_option_with_a_window() {
        assert!(!shown(&args(&["jc-game", "--versus=host"])));
        assert!(shown(&args(&["jc-game", "--versus=host", "--smoke=9000"])));
        assert!(shown(&args(&[
            "jc-game",
            "--versus=join:lan",
            "--versus-view"
        ])));
        assert!(!shown(&args(&["jc-game", "--smoke=9000"])));
    }

    #[test]
    fn a_tumbling_die_shows_every_face_and_no_other() {
        let faces: BTreeSet<u8> = (0..200).map(tumbling).collect();
        assert_eq!(faces, (1..=6).collect());
    }

    #[test]
    fn a_die_is_drawn_inside_the_screen_only() {
        let mut screen = [0, 0, 0, 0].repeat(SCREEN.0 * SCREEN.1);
        die(&mut screen, (2, 2), 6, true);
        die(&mut screen, (160, 120), 1, false);
        // The red pip of a 1 in the middle.
        let at = (120 * SCREEN.0 + 160) * 4;
        assert_eq!(screen[at..at + 4], [176, 32, 32, 255]);
    }
}
