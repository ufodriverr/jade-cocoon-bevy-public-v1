//! "Who hosts?" on the local network: a guest sends a short question by UDP broadcast to
//! [`LAN_PORT`](crate::LAN_PORT), and every host that hears it answers with its name, build and TCP port.
//! The guest lists the hosts it heard ([`Discovery`]), the host answers while it waits
//! ([`Responder`]). Only one host per machine can answer (the port is taken once).
//!
//! The question goes to 255.255.255.255 and to the broadcast address of each network of
//! the guest's machine ([`broadcasts`]): Windows sends the first out of one network card
//! only, so a PC with a second card or a VPN's adapter (Tailscale, ZeroTier, a virtual
//! machine's) would ask the wrong network. A VPN that carries no broadcast lists nobody;
//! its players type the address.
//!
//! Both threads end when their owner is dropped, before the drop returns, so the port is
//! free again at once.
//!
//! The datagrams: the question is the mark `JCVS?` and the protocol (two bytes); the
//! answer is the mark `JCVS!`, the protocol, the TCP port (two bytes each,
//! little-endian), then the build and the name, each as its length (a byte) and UTF-8.

use std::io::{self, ErrorKind};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, unbounded};

use crate::PROTOCOL;
use crate::wire::{MOST_BUILD, MOST_NAME};

const QUESTION: &[u8; 5] = b"JCVS?";
const ANSWER: &[u8; 5] = b"JCVS!";
/// How often a guest asks again, and how often the threads look at their stop flag.
const ASK_EVERY: Duration = Duration::from_secs(1);
const LOOK: Duration = Duration::from_millis(50);

/// What a host tells a guest that asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostInfo {
    pub name: String,
    pub build: String,
    pub port: u16,
}

/// A host a guest heard: where to connect and what it said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub address: SocketAddr,
    pub protocol: u16,
    pub info: HostInfo,
}

/// The question's datagram.
pub fn question() -> Vec<u8> {
    let mut bytes = QUESTION.to_vec();
    bytes.extend_from_slice(&PROTOCOL.to_le_bytes());
    bytes
}

/// Whether a datagram is the question (of any protocol: an old guest still hears a new
/// host, and is told by the build it shows).
pub fn is_question(bytes: &[u8]) -> bool {
    bytes.len() == QUESTION.len() + 2 && bytes.starts_with(QUESTION)
}

/// The answer's datagram.
pub fn answer(info: &HostInfo) -> Vec<u8> {
    let mut bytes = ANSWER.to_vec();
    bytes.extend_from_slice(&PROTOCOL.to_le_bytes());
    bytes.extend_from_slice(&info.port.to_le_bytes());
    for (text, most) in [(&info.build, MOST_BUILD), (&info.name, MOST_NAME)] {
        let mut end = text.len().min(most);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        bytes.push(end as u8);
        bytes.extend_from_slice(&text.as_bytes()[..end]);
    }
    bytes
}

/// The host an answer from `from` names: the sender's address with the port it gives.
/// `None` for anything that is not an answer.
pub fn read_answer(bytes: &[u8], from: SocketAddr) -> Option<Found> {
    let rest = bytes.strip_prefix(ANSWER)?;
    let (protocol, rest) = rest.split_first_chunk::<2>()?;
    let (port, mut rest) = rest.split_first_chunk::<2>()?;
    let mut text = |most: usize| -> Option<String> {
        let (&len, after) = rest.split_first()?;
        let len = usize::from(len);
        let (text, after) = after.split_at_checked(len).filter(|_| len <= most)?;
        rest = after;
        let text = std::str::from_utf8(text).ok()?;
        (!text.chars().any(char::is_control)).then(|| text.to_string())
    };
    let build = text(MOST_BUILD)?;
    let name = text(MOST_NAME)?;
    if !rest.is_empty() {
        return None;
    }
    let port = u16::from_le_bytes(*port);
    Some(Found {
        address: SocketAddr::new(from.ip(), port),
        protocol: u16::from_le_bytes(*protocol),
        info: HostInfo { name, build, port },
    })
}

/// The addresses the question goes to in real play, on `port`: 255.255.255.255 and the
/// broadcast address of each IPv4 network of this machine that has one.
pub fn broadcasts(port: u16) -> Vec<SocketAddr> {
    let networks = if_addrs::get_if_addrs().unwrap_or_default();
    let networks = (networks.iter()).filter_map(|network| match &network.addr {
        if_addrs::IfAddr::V4(four) if !network.is_loopback() => Some((four.ip, four.netmask)),
        _ => None,
    });
    broadcasts_of(networks, port)
}

/// [`broadcasts`] of the networks given as address and mask: a network of one or two
/// addresses (a VPN's /32, a link's /31) has no broadcast.
pub fn broadcasts_of(
    networks: impl IntoIterator<Item = (Ipv4Addr, Ipv4Addr)>,
    port: u16,
) -> Vec<SocketAddr> {
    let mut found = vec![Ipv4Addr::BROADCAST];
    for (ip, mask) in networks {
        let (ip, mask) = (u32::from(ip), u32::from(mask));
        if mask.leading_ones() + mask.trailing_zeros() != 32 || mask.trailing_zeros() < 2 {
            continue;
        }
        let broadcast = Ipv4Addr::from(ip | !mask);
        if !found.contains(&broadcast) {
            found.push(broadcast);
        }
    }
    (found.into_iter())
        .map(|ip| SocketAddr::new(IpAddr::V4(ip), port))
        .collect()
}

/// Waits a moment at most for a thread that was told to stop, and joins it.
fn join_soon(thread: Option<JoinHandle<()>>) {
    let Some(thread) = thread else {
        return;
    };
    let deadline = Instant::now() + LOOK * 10;
    while !thread.is_finished() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(2));
    }
    if thread.is_finished() {
        let _ = thread.join();
    }
}

/// A host's answering of "who hosts?", while it lives.
pub struct Responder {
    stop: Arc<AtomicBool>,
    address: SocketAddr,
    thread: Option<JoinHandle<()>>,
}

impl Responder {
    /// Answers questions to `ip`:`port` (the host's [`crate::link::host_address`] IP: every
    /// network in real play, 127.0.0.1 in a check). A port another host of this machine
    /// holds is an error; the host still plays, only unlisted.
    pub fn start(ip: IpAddr, port: u16, info: HostInfo) -> io::Result<Self> {
        let socket = UdpSocket::bind(SocketAddr::new(ip, port))?;
        socket.set_read_timeout(Some(LOOK))?;
        let address = socket.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let answer = answer(&info);
        let thread = thread::Builder::new()
            .name("versus lan host".into())
            .spawn(move || {
                let mut buffer = [0; 64];
                while !stopped.load(Ordering::Relaxed) {
                    match socket.recv_from(&mut buffer) {
                        Ok((len, from)) if is_question(&buffer[..len]) => {
                            let _ = socket.send_to(&answer, from);
                        }
                        Ok(_) => {}
                        Err(err) if is_wait(&err) => {}
                        // Windows reports an earlier answer's unreachable guest here.
                        Err(err) if err.kind() == ErrorKind::ConnectionReset => {}
                        Err(_) => return,
                    }
                }
            })?;
        Ok(Self {
            stop,
            address,
            thread: Some(thread),
        })
    }

    /// The address it answers on.
    pub fn address(&self) -> SocketAddr {
        self.address
    }
}

impl Drop for Responder {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        join_soon(self.thread.take());
    }
}

/// A guest's list of the hosts that answered, asked again every second while it lives.
pub struct Discovery {
    found: Receiver<Found>,
    stop: Arc<AtomicBool>,
    heard: Vec<(Found, Instant)>,
    thread: Option<JoinHandle<()>>,
}

impl Discovery {
    /// Asks `targets` (in real play [`broadcasts`] from `0.0.0.0`; in a check the host's
    /// 127.0.0.1 port from 127.0.0.1).
    pub fn start(targets: Vec<SocketAddr>, from: IpAddr) -> io::Result<Self> {
        let socket = UdpSocket::bind(SocketAddr::new(from, 0))?;
        socket.set_broadcast(!from.is_loopback())?;
        socket.set_read_timeout(Some(LOOK))?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let (found_tx, found) = unbounded();
        let thread = thread::Builder::new()
            .name("versus lan guest".into())
            .spawn(move || {
                let question = question();
                let mut buffer = [0; 256];
                let mut asked: Option<Instant> = None;
                while !stopped.load(Ordering::Relaxed) {
                    if asked.is_none_or(|at| at.elapsed() >= ASK_EVERY) {
                        // A network that refuses one address does not stop the others.
                        for target in &targets {
                            let _ = socket.send_to(&question, target);
                        }
                        asked = Some(Instant::now());
                    }
                    match socket.recv_from(&mut buffer) {
                        Ok((len, from)) => {
                            if let Some(host) = read_answer(&buffer[..len], from)
                                && found_tx.send(host).is_err()
                            {
                                return;
                            }
                        }
                        Err(err) if is_wait(&err) || err.kind() == ErrorKind::ConnectionReset => {}
                        Err(_) => return,
                    }
                }
            })?;
        Ok(Self {
            found,
            stop,
            heard: Vec::new(),
            thread: Some(thread),
        })
    }

    /// The hosts heard in the last few seconds, one entry per address, the first heard
    /// first.
    pub fn hosts(&mut self) -> Vec<Found> {
        let now = Instant::now();
        while let Ok(host) = self.found.try_recv() {
            match self
                .heard
                .iter_mut()
                .find(|(seen, _)| seen.address == host.address)
            {
                Some(entry) => *entry = (host, now),
                None => self.heard.push((host, now)),
            }
        }
        self.heard
            .retain(|(_, at)| now.duration_since(*at) < ASK_EVERY * 4);
        self.heard.iter().map(|(host, _)| host.clone()).collect()
    }
}

impl Drop for Discovery {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        join_soon(self.thread.take());
    }
}

fn is_wait(err: &io::Error) -> bool {
    matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> HostInfo {
        HostInfo {
            name: "Levant".into(),
            build: "0.1.0".into(),
            port: 47820,
        }
    }

    #[test]
    fn an_answer_names_the_host() {
        assert!(is_question(&question()));
        assert!(!is_question(b"JCVS?"));
        assert!(!is_question(&answer(&info())));
        let from: SocketAddr = "192.168.1.7:51000".parse().expect("an address");
        let found = read_answer(&answer(&info()), from).expect("an answer");
        assert_eq!(found.address.to_string(), "192.168.1.7:47820");
        assert_eq!((found.protocol, found.info), (PROTOCOL, info()));
        // Anything else is no answer.
        let bytes = answer(&info());
        assert_eq!(read_answer(&question(), from), None);
        for end in 0..bytes.len() {
            assert_eq!(read_answer(&bytes[..end], from), None, "cut at {end}");
        }
        let mut longer = bytes.clone();
        longer.push(0);
        assert_eq!(read_answer(&longer, from), None);
    }

    #[test]
    fn a_guest_hears_a_host_on_this_machine() {
        let here = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let host = Responder::start(here, 0, info()).expect("a port");
        // The first address answers nothing: the next one is asked all the same.
        let nobody = SocketAddr::new(here, 9);
        let mut guest = Discovery::start(vec![nobody, host.address()], here).expect("a socket");
        let deadline = Instant::now() + Duration::from_secs(5);
        let found = loop {
            if let Some(found) = guest.hosts().into_iter().next() {
                break found;
            }
            assert!(Instant::now() < deadline, "no host heard");
            thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(found.info, info());
        assert_eq!(found.address.ip(), here);
        assert_eq!(found.address.port(), 47820);
    }

    #[test]
    fn a_host_answers_again_on_the_same_port_at_once() {
        let here = IpAddr::V4(Ipv4Addr::LOCALHOST);
        for time in 0..4 {
            match Responder::start(here, 47891, info()) {
                Ok(host) => drop(host),
                Err(err) => panic!("time {time}: {err}"),
            }
        }
        let guest = Discovery::start(vec![SocketAddr::new(here, 47891)], here);
        drop(guest.expect("a socket"));
    }

    #[test]
    fn the_question_goes_to_every_network_that_has_a_broadcast() {
        let mask = |bits: u32| Ipv4Addr::from(u32::MAX.checked_shl(32 - bits).unwrap_or(0));
        let networks = [
            (Ipv4Addr::new(192, 168, 1, 20), mask(24)),
            // Tailscale's address of one: no broadcast.
            (Ipv4Addr::new(100, 101, 102, 103), mask(32)),
            (Ipv4Addr::new(10, 147, 17, 9), mask(16)),
            (Ipv4Addr::new(192, 168, 1, 21), mask(24)),
            // ZeroTier's /24 has one, a link of two has none, a mask with holes is no mask.
            (Ipv4Addr::new(172, 27, 3, 4), mask(24)),
            (Ipv4Addr::new(10, 0, 0, 1), mask(31)),
            (Ipv4Addr::new(10, 9, 0, 1), Ipv4Addr::new(255, 0, 255, 0)),
        ];
        let found: Vec<String> = (broadcasts_of(networks, 47821).iter())
            .map(SocketAddr::to_string)
            .collect();
        assert_eq!(
            found,
            [
                "255.255.255.255:47821",
                "192.168.1.255:47821",
                "10.147.255.255:47821",
                "172.27.3.255:47821",
            ]
        );
        assert!(broadcasts(47821)[0].ip() == IpAddr::V4(Ipv4Addr::BROADCAST));
    }
}
