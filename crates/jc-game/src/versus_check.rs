
use std::error::Error;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use jc_disc::{DiscImage, Executable, LocalConfig};
use jc_formats::battle::Tables;
use jc_formats::battle::duel::{Choice, Duel, Options};
use jc_formats::battle::record::{Draws, Random};
use jc_formats::versus::{Series, Team};
use jc_net::lan::{Discovery, HostInfo, Responder};
use jc_net::link::{CONNECT_TIMEOUT, Connecting, Link, Listener, Stream, Timing, host_address};
use jc_net::online::{self, Code, Hosting, Joining, Reach};
use jc_net::session::{End, Local, Notice, Role, Setup, VersusSession};
use jc_net::wire::Terms;
use jc_net::{LAN_PORT, PORT, read_rules_hash};

use crate::net::Versus;

/// How long a host waits for its guest, and a guest for its host.
const WAIT_FOR_PEER: Duration = Duration::from_secs(60);
/// The turns after which the bot gives up, so that a fight always ends.
const MOST_TURNS: u32 = 5000;
/// One command in so many is "Give up", so that the checks see that ending too.
const GIVE_UP_ONE_IN: u32 = 400;

/// Which end of the connection the arguments ask for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Host {
        port: u16,
    },
    Join {
        address: String,
    },
    JoinLan,
    /// Hosts under a new room code.
    HostOnline,
    /// Joins by a room code, or by the code of `--versus-code-file=` when none is given.
    JoinOnline {
        code: Option<String>,
    },
}

impl Mode {
    /// The end of the connection: a host or a guest.
    pub fn role(&self) -> Role {
        match self {
            Mode::Host { .. } | Mode::HostOnline => Role::Host,
            Mode::Join { .. } | Mode::JoinLan | Mode::JoinOnline { .. } => Role::Guest,
        }
    }
}

/// The reach of a room code's endpoint: every network in real play; 127.0.0.1 in a
/// check, or no UDP socket at all with `--versus-online=relay`.
pub(crate) fn reach_of(args: &[String], check: bool) -> Reach {
    let relay = args.iter().any(|arg| arg == "--versus-online=relay");
    match (relay, check) {
        (true, _) => Reach::RelayOnly,
        (false, true) => Reach::Here,
        (false, false) => Reach::Everywhere,
    }
}

/// `--versus-code-file=PATH`: where a check's host writes its room code and its guest
/// reads it.
pub(crate) fn code_file(args: &[String]) -> Option<PathBuf> {
    (args.iter())
        .find_map(|arg| arg.strip_prefix("--versus-code-file="))
        .map(PathBuf::from)
}

/// The code a check's host writes, for its guest.
pub(crate) fn write_code(path: &Path, code: Code) {
    match std::fs::write(path, format!("{code}\n")) {
        Ok(()) => println!("versus: the room code is in {}", path.display()),
        Err(err) => println!(
            "versus: the room code cannot go to {}: {err}",
            path.display()
        ),
    }
}

/// The code of a check's file, once its host wrote it.
pub(crate) fn read_code(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    Code::parse(&text).is_ok().then_some(text)
}

/// The mode, when the arguments ask for the check.
pub fn asked(args: &[String]) -> Option<Result<Mode, String>> {
    let value = args.iter().find_map(|arg| arg.strip_prefix("--versus="))?;
    Some(match value.split_once(':') {
        None if value == "host" => Ok(Mode::Host { port: PORT }),
        Some(("host", port)) => (port.parse())
            .map(|port| Mode::Host { port })
            .map_err(|_| format!("--versus=host:PORT takes a port, not {port}")),
        Some(("join", "lan")) => Ok(Mode::JoinLan),
        None if value == "online-host" => Ok(Mode::HostOnline),
        None if value == "online-join" => Ok(Mode::JoinOnline { code: None }),
        Some(("online-join", code)) if !code.trim().is_empty() => Ok(Mode::JoinOnline {
            code: Some(code.to_string()),
        }),
        Some(("join", address)) if !address.is_empty() => Ok(Mode::Join {
            address: address.to_string(),
        }),
        _ => Err(format!(
            "--versus= takes host, host:PORT, join:ADDR, join:lan, online-host or online-join[:CODE], not {value}"
        )),
    })
}

/// The bot of a check: answers everything at once. The view of a check
/// (`versus_view.rs`) asks it for the commands and the rematches too.
pub(crate) struct Bot {
    pub(crate) team: Team,
    pub(crate) terms: Terms,
    picker: Random,
    pub(crate) matches: u32,
    /// The fights that ended, as the log counts them.
    pub(crate) played: u32,
}

impl Bot {
    /// The bot of the arguments: the team of `--versus-save=N` (or a built-in one), the
    /// host's terms, the seed of `--versus-bot=SEED` (1 without it) and the count of
    /// `--versus-matches=K`.
    pub(crate) fn of(
        tables: &Tables,
        args: &[String],
        side: usize,
    ) -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            team: crate::duel_check::team_of(tables, args, "--versus-save=", side)?,
            terms: terms_of(args)?,
            picker: Random {
                seed: number(args, "--versus-bot=", 1)?,
            },
            matches: number(args, "--versus-matches=", 1)?.max(1),
            played: 0,
        })
    }
}

/// The value of a numbered option, or `default` without it.
pub(crate) fn number(args: &[String], name: &str, default: u32) -> Result<u32, String> {
    let option = args.iter().find_map(|arg| arg.strip_prefix(name));
    option.map_or(Ok(default), |value| {
        value
            .parse()
            .map_err(|_| format!("{name}N takes a number, not {value}"))
    })
}

/// The host's terms of the arguments: `--duel-rules=original` (no items),
/// `--duel-changing=off`, `--versus-place=N` (else a place from the seed).
pub(crate) fn terms_of(args: &[String]) -> Result<Terms, Box<dyn Error>> {
    let option = |name: &str| args.iter().find_map(|arg| arg.strip_prefix(name));
    let place = option("--versus-place=")
        .map(str::parse::<u8>)
        .transpose()?;
    Ok(Terms {
        options: Options {
            items: option("--duel-rules=") != Some("original"),
            changing: option("--duel-changing=") != Some("off"),
        },
        place,
    })
}

/// What this side says of itself: `--versus-name=NAME` (Host or Guest without it), and
/// for the checks that must fail `--versus-build=X` and `--versus-corrupt=TURN`.
pub(crate) fn setup_of(args: &[String], role: Role, rules: u64) -> Result<Setup, Box<dyn Error>> {
    let option = |name: &str| args.iter().find_map(|arg| arg.strip_prefix(name));
    let name = option("--versus-name=").unwrap_or(match role {
        Role::Host => "Host",
        Role::Guest => "Guest",
    });
    let mut setup = Setup::new(name, rules);
    if let Some(build) = option("--versus-build=") {
        setup.build = build.to_string();
    }
    setup.corrupt_at = option("--versus-corrupt=").map(str::parse).transpose()?;
    Ok(setup)
}

impl Bot {
    /// A random legal command of the side, "Give up" now and then and after too many
    /// turns.
    pub(crate) fn pick(&mut self, tables: &Tables, duel: &Duel, side: usize) -> Choice {
        let legal = duel.legal(tables, side);
        let give_up = self.picker.draw().is_multiple_of(GIVE_UP_ONE_IN);
        match duel.turn() > MOST_TURNS || give_up || legal.is_empty() {
            true => Choice::GiveUp,
            false => legal[self.picker.draw() as usize % legal.len()],
        }
    }

    /// One more while the tally counts fewer fights than asked for.
    pub(crate) fn again(&self, series: &Series) -> bool {
        let played: u32 = series.lost.iter().map(|&lost| u32::from(lost)).sum();
        played < self.matches
    }

    pub(crate) fn half_seed(&mut self) -> u32 {
        self.picker.draw() << 16 | self.picker.draw()
    }
}

impl Local for Bot {
    fn terms(&mut self) -> Option<Terms> {
        Some(self.terms)
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
        Some(self.pick(tables, duel, side))
    }

    /// One more while the tally counts fewer fights than asked for (the tally stops at
    /// 99 lost a side, so a larger count plays on until stopped).
    fn rematch(&mut self, series: &Series) -> Option<bool> {
        Some(self.again(series))
    }

    fn bits(&mut self) -> u32 {
        self.half_seed()
    }
}

/// Runs one side of the check; an error for any end but the planned one.
pub fn run(mode: Mode, args: &[String]) -> Result<(), Box<dyn Error>> {
    let config = LocalConfig::load()?;
    let mut disc = DiscImage::open(&config.disc_image)?;
    let exe = Executable::read(&mut disc)?;
    let tables =
        Tables::read(&exe.image, exe.base).ok_or("the fights' tables are not in the executable")?;
    let rules = read_rules_hash(&mut disc)?;
    let role = mode.role();
    let side = role.side();
    let setup = setup_of(args, role, rules)?;
    let name = setup.name.clone();
    let mut bot = Bot::of(&tables, args, side)?;
    let lan_port = u16::try_from(number(args, "--versus-lan-port=", u32::from(LAN_PORT))?)?;
    println!(
        "versus: {name}, rules hash {rules:016x}, team hash {:016x}",
        bot.team.hash()
    );
    let stream: Stream = match mode {
        Mode::Host { port } => host(port, lan_port, &setup)?.into(),
        Mode::Join { address } => join(&address)?.into(),
        Mode::JoinLan => join(&discover(lan_port)?.to_string())?.into(),
        Mode::HostOnline => host_online(args)?.into(),
        Mode::JoinOnline { code } => join_online(code, args)?.into(),
    };
    println!("versus: connected with {}", stream.describe());
    let link = Link::new(stream, Timing::default())?;
    let mut versus = Versus(VersusSession::new(role, Box::new(link), setup));
    let mut route = None;
    let end = loop {
        versus.update(&tables, &mut bot);
        if versus.route() != route {
            route = versus.route();
            if let Some(route) = route {
                println!("versus: route {route}");
            }
        }
        for notice in versus.take_notices() {
            if matches!(notice, Notice::Over { .. }) {
                bot.played += 1;
            }
            log(&versus, &notice);
        }
        if let Some(end) = versus.ended() {
            break end.clone();
        }
        thread::sleep(Duration::from_millis(1));
    };
    versus.finish(Duration::from_secs(2));
    match end {
        End::Left | End::PeerLeft if bot.played >= bot.matches => Ok(()),
        end => Err(format!(
            "ended after {} of {} fights: {end}",
            bot.played, bot.matches
        )
        .into()),
    }
}

/// Waits on 127.0.0.1 for the guest, answering "who hosts?" meanwhile.
fn host(port: u16, lan_port: u16, setup: &Setup) -> Result<TcpStream, Box<dyn Error>> {
    let listener = Listener::start(host_address(port, true))?;
    println!("versus: hosting on {}", listener.address());
    let info = HostInfo {
        name: setup.name.clone(),
        build: setup.build.clone(),
        port: listener.address().port(),
    };
    let _answering = match Responder::start(IpAddr::V4(Ipv4Addr::LOCALHOST), lan_port, info) {
        Ok(responder) => {
            println!("versus: answering who hosts on {}", responder.address());
            Some(responder)
        }
        Err(err) => {
            println!("versus: not answering who hosts on port {lan_port}: {err}");
            None
        }
    };
    let deadline = Instant::now() + WAIT_FOR_PEER;
    loop {
        if let Some(stream) = listener.poll() {
            return Ok(stream?);
        }
        if Instant::now() > deadline {
            return Err("no guest came".into());
        }
        thread::sleep(Duration::from_millis(5));
    }
}

/// Connects to the host, trying again while it does not listen yet.
fn join(address: &str) -> Result<TcpStream, Box<dyn Error>> {
    let deadline = Instant::now() + WAIT_FOR_PEER;
    loop {
        let connecting = Connecting::start(address, CONNECT_TIMEOUT);
        let answer = loop {
            if let Some(answer) = connecting.poll() {
                break answer;
            }
            thread::sleep(Duration::from_millis(5));
        };
        match answer {
            Ok(stream) => {
                println!("versus: joined {address}");
                return Ok(stream);
            }
            Err(err) if Instant::now() > deadline => {
                return Err(format!("no host at {address}: {err}").into());
            }
            Err(_) => thread::sleep(Duration::from_millis(200)),
        }
    }
}

/// Hosts under a new room code, from 127.0.0.1 (or the relays only), and writes the
/// code to the file of `--versus-code-file=` once a guest can find it.
fn host_online(args: &[String]) -> Result<online::Stream, Box<dyn Error>> {
    let hosting = Hosting::start(reach_of(args, true))?;
    let code = hosting.code();
    println!("versus: hosting under the room code {code}");
    let file = code_file(args);
    let started = Instant::now();
    let deadline = started + WAIT_FOR_PEER;
    let mut told = false;
    loop {
        if !told && hosting.online() {
            told = true;
            println!(
                "versus: online after {:.1} s: the code can be found",
                started.elapsed().as_secs_f32()
            );
            if let Some(file) = &file {
                write_code(file, code);
            }
        }
        if let Some(stream) = hosting.poll() {
            return Ok(stream?);
        }
        if Instant::now() > deadline {
            return Err("no guest came".into());
        }
        thread::sleep(Duration::from_millis(5));
    }
}

/// Joins by a room code, or by the code of the file of `--versus-code-file=` once its
/// host wrote it; "No host found for this code" after `online::FIND_WITHIN`.
fn join_online(code: Option<String>, args: &[String]) -> Result<online::Stream, Box<dyn Error>> {
    let deadline = Instant::now() + WAIT_FOR_PEER;
    let text = match code {
        Some(code) => code,
        None => {
            let file = code_file(args)
                .ok_or("--versus=online-join needs a code or --versus-code-file=")?;
            loop {
                if let Some(text) = read_code(&file) {
                    break text;
                }
                if Instant::now() > deadline {
                    return Err(format!("no room code came to {}", file.display()).into());
                }
                thread::sleep(Duration::from_millis(50));
            }
        }
    };
    let code = Code::parse(&text)?;
    println!("versus: joining the room code {code}");
    let started = Instant::now();
    let joining = Joining::start(code, reach_of(args, true))?;
    loop {
        if let Some(found) = joining.poll() {
            let took = started.elapsed().as_secs_f32();
            return match found {
                Ok(stream) => {
                    println!("versus: joined the room code {code} after {took:.1} s");
                    Ok(stream)
                }
                Err(err) => Err(format!("{err} after {took:.1} s").into()),
            };
        }
        thread::sleep(Duration::from_millis(5));
    }
}

/// Asks 127.0.0.1 who hosts, and takes the first that answers.
fn discover(lan_port: u16) -> Result<SocketAddr, Box<dyn Error>> {
    let here = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let mut discovery = Discovery::start(vec![SocketAddr::new(here, lan_port)], here)?;
    let deadline = Instant::now() + WAIT_FOR_PEER;
    loop {
        if let Some(found) = discovery.hosts().into_iter().next() {
            println!(
                "versus: heard {} (build {}) at {}",
                found.info.name, found.info.build, found.address
            );
            return Ok(found.address);
        }
        if Instant::now() > deadline {
            return Err("no host answered who hosts".into());
        }
        thread::sleep(Duration::from_millis(20));
    }
}

/// The log line of a notice. The lines of the fights ("versus: match", "match") are the
/// same on both sides; the others say what this side saw.
pub(crate) fn log(versus: &VersusSession, notice: &Notice) {
    match notice {
        Notice::Met { name } => println!("versus: met {name}"),
        Notice::Agreed(terms) => {
            println!("versus: terms {:?}, place {:?}", terms.options, terms.place)
        }
        Notice::Teams => {
            let hashes = versus
                .teams()
                .each_ref()
                .map(|team| team.as_ref().map(Team::hash));
            println!("versus: teams in, hashes {hashes:x?}");
        }
        Notice::Started { round, seed, place } => {
            let start = versus.duel().map(|duel| duel.start.clone());
            let (rolls, winner, equally) = start.map_or((Vec::new(), 0, false), |start| {
                (start.rolls, start.winner, start.equally_fast)
            });
            println!(
                "versus: match {round} seed {seed:08x} place {place} dice {rolls:?} head start side {winner}{}",
                match equally {
                    true => ", equally fast",
                    false => "",
                }
            );
        }
        Notice::Turn {
            round,
            side,
            turn,
            choice,
            hash,
        } => println!("match {round} turn {turn}: side {side} {choice:?} hash {hash:016x}"),
        Notice::Over {
            round,
            winner,
            gave_up,
            series,
        } => println!(
            "versus: match {round} over: side {winner} won{}; wins so far {:?}",
            match gave_up {
                true => " (the other gave up)",
                false => "",
            },
            series.won()
        ),
        Notice::Ended(end) => println!("versus: ended: {end}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_check_is_asked_with_a_mode() {
        let args = |list: &[&str]| list.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
        assert_eq!(asked(&args(&["jc-game", "--smoke=5"])), None);
        assert_eq!(
            asked(&args(&["jc-game", "--versus=host"])),
            Some(Ok(Mode::Host { port: PORT }))
        );
        assert_eq!(
            asked(&args(&["jc-game", "--versus=host:5000"])),
            Some(Ok(Mode::Host { port: 5000 }))
        );
        assert_eq!(
            asked(&args(&["jc-game", "--versus=join:127.0.0.1:5000"])),
            Some(Ok(Mode::Join {
                address: "127.0.0.1:5000".into()
            }))
        );
        assert_eq!(
            asked(&args(&["jc-game", "--versus=join:lan"])),
            Some(Ok(Mode::JoinLan))
        );
        assert_eq!(
            asked(&args(&["jc-game", "--versus=online-host"])),
            Some(Ok(Mode::HostOnline))
        );
        assert_eq!(
            asked(&args(&["jc-game", "--versus=online-join"])),
            Some(Ok(Mode::JoinOnline { code: None }))
        );
        assert_eq!(
            asked(&args(&["jc-game", "--versus=online-join:k3fm-q8xa-7wpe"])),
            Some(Ok(Mode::JoinOnline {
                code: Some("k3fm-q8xa-7wpe".into())
            }))
        );
        assert_eq!(Mode::HostOnline.role(), Role::Host);
        assert_eq!(Mode::JoinOnline { code: None }.role(), Role::Guest);
        for bad in [
            "--versus=host:x",
            "--versus=join:",
            "--versus=watch",
            "--versus=online-join: ",
        ] {
            assert!(
                matches!(asked(&args(&["jc-game", bad])), Some(Err(_))),
                "{bad}"
            );
        }
    }

    #[test]
    fn a_check_keeps_its_endpoint_here_and_real_play_reaches_everywhere() {
        let args = |list: &[&str]| list.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
        assert_eq!(reach_of(&args(&[]), true), Reach::Here);
        assert_eq!(reach_of(&args(&[]), false), Reach::Everywhere);
        let relay = args(&["--versus-online=relay"]);
        assert_eq!(reach_of(&relay, true), Reach::RelayOnly);
        let file = args(&["--versus-code-file=target/code.txt"]);
        assert_eq!(code_file(&file), Some(PathBuf::from("target/code.txt")));
    }
}
