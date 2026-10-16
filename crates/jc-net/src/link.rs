//! One connection between the two players, and the ways to make it by address: a host
//! that waits for one peer ([`Listener`]) and a guest that connects ([`Connecting`]). The
//! connection is a [`Stream`]: a TCP stream (the same network, or an address), or a
//! stream of a room code (`online.rs`, through the internet); the link runs alike on
//! both.
//!
//! A [`Link`] runs two threads: one reads frames and hands the messages over a channel,
//! one takes messages from a channel and writes them, and a ping when nothing went out
//! for [`Timing::heartbeat`]. The game only looks into the channels, so no frame ever
//! waits on the network. A peer that sends nothing, not even a ping, for
//! [`Timing::silence`] is gone ([`Lost::Silent`]).
//!
//! Whatever goes, goes whole: a dropped [`Link`] closes its connection and its threads
//! end, a dropped [`Listener`] stops listening and its thread ends, each before the drop
//! returns (or within [`ENDS_WITHIN`] at the most), so that the port is free again for
//! the next host of the same process.

use std::fmt;
use std::io::{self, ErrorKind, Read, Write};
use std::net::{
    IpAddr, Ipv4Addr, Ipv6Addr, Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, unbounded};

use crate::PORT;
use crate::online::{self, Route};
#[cfg(test)]
use crate::wire::read_frame;
use crate::wire::{Message, take_frame, write_frame};

/// The time a guest waits for a host to answer.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// How often a waiting host looks for a peer.
const ACCEPT_POLL: Duration = Duration::from_millis(25);
/// The longest a drop waits for the threads of a link or a listener to end; a thread
/// stuck in the system longer than that ends by itself at its time-out.
pub const ENDS_WITHIN: Duration = Duration::from_millis(500);
/// The part of [`ENDS_WITHIN`] a dropped link gives its writer to send what is queued
/// before the connection is cut.
const LAST_WRITES: Duration = Duration::from_millis(200);
/// How long a reading thread waits for bytes before it looks at its stop flag and the
/// silence again. (A cut connection does not wake a thread that waits on it on Windows,
/// so the thread must look by itself.)
const READ_LOOK: Duration = Duration::from_millis(50);

/// The pace of the pings and how long a silent peer is waited for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    pub heartbeat: Duration,
    pub silence: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            heartbeat: Duration::from_secs(1),
            silence: Duration::from_secs(10),
        }
    }
}

/// How a connection ended without a message saying so.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lost {
    /// The peer closed the connection.
    Closed,
    /// Nothing came for longer than [`Timing::silence`].
    Silent,
    /// The connection broke (reset, aborted, ...).
    Broken(String),
    /// The peer sent bytes that are no message.
    Malformed(String),
}

impl fmt::Display for Lost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Lost::Closed => write!(f, "the other side closed the connection"),
            Lost::Silent => write!(f, "nothing came from the other side for too long"),
            Lost::Broken(why) => write!(f, "the connection broke ({why})"),
            Lost::Malformed(why) => write!(f, "the other side sent {why}"),
        }
    }
}

/// What the reading side hands over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incoming {
    Message(Message),
    /// The last thing that comes.
    Lost(Lost),
}

/// The way the session talks to the other side: a [`Link`], or a pair of channels in a
/// test. Nothing of it may block.
pub trait Transport: Send + Sync {
    /// Queues a message.
    fn send(&mut self, message: Message);
    /// The next thing that came, if any.
    fn receive(&mut self) -> Option<Incoming>;
    /// Sends what is queued and closes; whatever comes after is dropped.
    fn close(&mut self);
    /// How the packets go, for a connection of a room code; none for one by address.
    fn route(&self) -> Option<Route> {
        None
    }
    /// [`Self::close`], then waits up to `wait` for the queue to go out and the other side
    /// to close too. For the end of a process; a frame calls [`Self::close`].
    fn finish(&mut self, wait: Duration) {
        let _ = wait;
        self.close();
    }
}

/// The connection under a link: TCP, or a stream of a room code. Both read with a time
/// limit and close the same ways.
#[derive(Clone)]
pub enum Stream {
    Tcp(Arc<TcpStream>),
    Online(online::Stream),
}

impl From<TcpStream> for Stream {
    fn from(stream: TcpStream) -> Self {
        Stream::Tcp(Arc::new(stream))
    }
}

impl From<online::Stream> for Stream {
    fn from(stream: online::Stream) -> Self {
        Stream::Online(stream)
    }
}

impl Stream {
    fn prepare(&self, timing: Timing) -> io::Result<()> {
        let read = Some(READ_LOOK.min(timing.silence));
        match self {
            Stream::Tcp(stream) => {
                stream.set_nodelay(true)?;
                stream.set_read_timeout(read)?;
                stream.set_write_timeout(Some(timing.silence))
            }
            Stream::Online(stream) => {
                stream.set_read_timeout(read);
                stream.set_write_timeout(Some(timing.silence));
                Ok(())
            }
        }
    }

    /// The other side's address, for a TCP stream.
    pub fn peer_addr(&self) -> Option<SocketAddr> {
        match self {
            Stream::Tcp(stream) => stream.peer_addr().ok(),
            Stream::Online(_) => None,
        }
    }

    /// The other side, for a log: its address, or its endpoint and the route.
    pub fn describe(&self) -> String {
        match self {
            Stream::Tcp(stream) => {
                (stream.peer_addr()).map_or_else(|_| "an address".into(), |peer| peer.to_string())
            }
            Stream::Online(stream) => match stream.route() {
                Some(route) => format!("endpoint {} ({route})", stream.peer()),
                None => format!("endpoint {}", stream.peer()),
            },
        }
    }

    pub fn route(&self) -> Option<Route> {
        match self {
            Stream::Tcp(_) => None,
            Stream::Online(stream) => stream.route(),
        }
    }

    fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        match self {
            Stream::Tcp(stream) => stream.shutdown(how),
            Stream::Online(stream) => stream.shutdown(how),
        }
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Stream::Tcp(stream) => (&**stream).read(buf),
            Stream::Online(stream) => stream.read(buf),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Stream::Tcp(stream) => (&**stream).write(buf),
            Stream::Online(stream) => stream.write_all(buf).map(|()| buf.len()),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Stream::Tcp(stream) => (&**stream).flush(),
            Stream::Online(_) => Ok(()),
        }
    }
}

/// A connection to the other player.
pub struct Link {
    out: Option<Sender<Message>>,
    incoming: Receiver<Incoming>,
    writer: Option<JoinHandle<()>>,
    reader: Option<JoinHandle<()>>,
    /// The connection itself, which a drop cuts.
    stream: Stream,
    /// Tells the reading thread to end.
    stop: Arc<AtomicBool>,
    peer: Option<SocketAddr>,
}

impl Link {
    /// Starts the two threads of a connected stream.
    pub fn new(stream: impl Into<Stream>, timing: Timing) -> io::Result<Self> {
        let stream = stream.into();
        stream.prepare(timing)?;
        let peer = stream.peer_addr();
        let (incoming_tx, incoming) = unbounded();
        let (out, out_rx) = unbounded::<Message>();
        let mut reading = stream.clone();
        let mut writing = stream.clone();
        let lost_tx = incoming_tx.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let reader = thread::Builder::new()
            .name("versus reader".into())
            .spawn(move || read_all(&mut reading, &incoming_tx, &stopped, timing.silence))?;
        let writer = thread::Builder::new()
            .name("versus writer".into())
            .spawn(move || write_all(&mut writing, &out_rx, &lost_tx, timing.heartbeat));
        let writer = match writer {
            Ok(writer) => writer,
            Err(err) => {
                stop.store(true, Ordering::Relaxed);
                let _ = reader.join();
                return Err(err);
            }
        };
        Ok(Self {
            out: Some(out),
            incoming,
            writer: Some(writer),
            reader: Some(reader),
            stream,
            stop,
            peer,
        })
    }

    /// The other side's address (a connection by address).
    pub fn peer(&self) -> Option<SocketAddr> {
        self.peer
    }

    /// Whether both threads have ended.
    pub fn threads_ended(&self) -> bool {
        [&self.reader, &self.writer]
            .into_iter()
            .all(|thread| thread.as_ref().is_none_or(JoinHandle::is_finished))
    }
}

/// Waits until `deadline` for a thread to end, and joins it when it did.
fn join_by(thread: &mut Option<JoinHandle<()>>, deadline: Instant) {
    let Some(handle) = thread.take() else {
        return;
    };
    while !handle.is_finished() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(2));
    }
    match handle.is_finished() {
        true => {
            let _ = handle.join();
        }
        // It ends by itself at its time-out; nothing waits for it any more.
        false => *thread = Some(handle),
    }
}

/// The reading thread: every message but the pings goes over, then how it ended. After the
/// game closed its side, it goes on reading until the peer closes too, so that the peer's
/// last messages are taken rather than reset; a dropped link stops it (`stop`), and it
/// tells nothing more then.
fn read_all(
    stream: &mut Stream,
    incoming: &Sender<Incoming>,
    stop: &AtomicBool,
    silence: Duration,
) {
    let mut frames = Vec::new();
    let mut chunk = [0; 1024];
    let mut heard = Instant::now();
    let lost = 'reading: loop {
        loop {
            match take_frame(&mut frames) {
                Ok(Some(Message::Ping)) => {}
                Ok(Some(message)) => {
                    let _ = incoming.send(Incoming::Message(message));
                }
                Ok(None) => break,
                Err(err) => break 'reading Lost::Malformed(err.to_string()),
            }
        }
        if stop.load(Ordering::Relaxed) {
            return;
        }
        match stream.read(&mut chunk) {
            Ok(0) => break Lost::Closed,
            Ok(read) => {
                frames.extend_from_slice(&chunk[..read]);
                heard = Instant::now();
            }
            Err(err) if matches!(err.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) => {
                if heard.elapsed() >= silence {
                    break Lost::Silent;
                }
            }
            Err(err) if err.kind() == ErrorKind::Interrupted => {}
            Err(err) => break Lost::Broken(err.to_string()),
        }
    };
    let _ = incoming.send(Incoming::Lost(lost));
    let _ = stream.shutdown(Shutdown::Both);
}

/// The writing thread: the messages in order, a ping after each quiet heartbeat, and the
/// end of the writing side once the game closed.
fn write_all(
    stream: &mut Stream,
    out: &Receiver<Message>,
    incoming: &Sender<Incoming>,
    heartbeat: Duration,
) {
    loop {
        let message = match out.recv_timeout(heartbeat) {
            Ok(message) => message,
            Err(RecvTimeoutError::Timeout) => Message::Ping,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        if let Err(err) = write_frame(stream, &message) {
            let _ = incoming.send(Incoming::Lost(Lost::Broken(err.to_string())));
            break;
        }
    }
    let _ = stream.shutdown(Shutdown::Write);
}

impl Transport for Link {
    fn send(&mut self, message: Message) {
        if let Some(out) = &self.out {
            let _ = out.send(message);
        }
    }

    fn receive(&mut self) -> Option<Incoming> {
        self.out.as_ref()?;
        self.incoming.try_recv().ok()
    }

    fn close(&mut self) {
        self.out = None;
    }

    fn route(&self) -> Option<Route> {
        self.stream.route()
    }

    fn finish(&mut self, wait: Duration) {
        self.close();
        let deadline = Instant::now() + wait;
        join_by(&mut self.writer, deadline);
        while let Ok(incoming) = self.incoming.recv_deadline(deadline) {
            if matches!(incoming, Incoming::Lost(_)) {
                break;
            }
        }
    }
}

/// The end of a link: what is queued goes out (for a moment at most), then the
/// connection is cut both ways, the reading thread stops even when the other side never
/// closes, and both threads are joined.
impl Drop for Link {
    fn drop(&mut self) {
        self.close();
        self.stop.store(true, Ordering::Relaxed);
        let start = Instant::now();
        join_by(&mut self.writer, start + LAST_WRITES);
        let _ = self.stream.shutdown(Shutdown::Both);
        join_by(&mut self.writer, start + ENDS_WITHIN);
        join_by(&mut self.reader, start + ENDS_WITHIN);
    }
}

/// The address a host listens on: every network of the machine in real play, only this
/// machine (127.0.0.1, which opens no firewall question) in a check.
pub fn host_address(port: u16, only_here: bool) -> SocketAddr {
    let ip = match only_here {
        true => Ipv4Addr::LOCALHOST,
        false => Ipv4Addr::UNSPECIFIED,
    };
    SocketAddr::new(IpAddr::V4(ip), port)
}

/// An address the player typed, with [`PORT`] when it names none: `192.168.1.20`,
/// `192.168.1.20:5000`, `[::1]:47820`, `::1`, `my-pc`, `my-pc:5000`.
pub fn with_port(address: &str) -> String {
    let address = address.trim();
    if address.parse::<SocketAddr>().is_ok() {
        return address.to_string();
    }
    if let Ok(ip) = address.parse::<IpAddr>() {
        return SocketAddr::new(ip, PORT).to_string();
    }
    match address.rsplit_once(':') {
        Some((_, port)) if port.parse::<u16>().is_ok() => address.to_string(),
        _ => format!("{address}:{PORT}"),
    }
}

/// A host waiting for its one peer. It listens from [`Listener::start`] on and stops
/// listening when the peer came or the listener is dropped. A host listens on IPv6 too,
/// on the same port (every network: 0.0.0.0 and [::]; this machine only: 127.0.0.1 and
/// [::1]), so that a guest may type either kind of address.
pub struct Listener {
    found: Receiver<io::Result<TcpStream>>,
    stop: Arc<AtomicBool>,
    address: SocketAddr,
    thread: Option<JoinHandle<()>>,
}

impl Listener {
    /// Listens on `address` (a port in use is an error here, not later).
    pub fn start(address: SocketAddr) -> io::Result<Self> {
        let first = TcpListener::bind(address)?;
        first.set_nonblocking(true)?;
        let address = first.local_addr()?;
        let mut listeners = vec![first];
        // Without IPv6 on the machine, or with a system that gives one socket both kinds,
        // the first one is enough.
        let six = match address.ip() {
            IpAddr::V4(Ipv4Addr::UNSPECIFIED) => Some(Ipv6Addr::UNSPECIFIED),
            IpAddr::V4(Ipv4Addr::LOCALHOST) => Some(Ipv6Addr::LOCALHOST),
            _ => None,
        };
        if let Some(six) = six
            && let Ok(listener) = TcpListener::bind(SocketAddr::new(six.into(), address.port()))
            && listener.set_nonblocking(true).is_ok()
        {
            listeners.push(listener);
        }
        let stop = Arc::new(AtomicBool::new(false));
        let (found_tx, found) = unbounded();
        let stopped = stop.clone();
        let thread = thread::Builder::new()
            .name("versus host".into())
            .spawn(move || {
                while !stopped.load(Ordering::Relaxed) {
                    for listener in &listeners {
                        match listener.accept() {
                            Ok((stream, _)) => {
                                let stream = stream.set_nonblocking(false).map(|()| stream);
                                let _ = found_tx.send(stream);
                                return;
                            }
                            Err(err) if err.kind() == ErrorKind::WouldBlock => {}
                            Err(err) => {
                                let _ = found_tx.send(Err(err));
                                return;
                            }
                        }
                    }
                    thread::sleep(ACCEPT_POLL);
                }
            })?;
        Ok(Self {
            found,
            stop,
            address,
            thread: Some(thread),
        })
    }

    /// The address it listens on (with the port the system gave for port 0).
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// The peer's stream once one connected.
    pub fn poll(&self) -> Option<io::Result<TcpStream>> {
        self.found.try_recv().ok()
    }
}

/// The listening thread ends within one look ([`ACCEPT_POLL`]) and takes its sockets
/// with it: the port is free when the drop returns.
impl Drop for Listener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        join_by(&mut self.thread, Instant::now() + ENDS_WITHIN);
    }
}

/// A guest connecting to a host, in the background.
pub struct Connecting {
    found: Receiver<io::Result<TcpStream>>,
}

impl Connecting {
    /// Connects to `address` ([`with_port`] is applied), giving up after `timeout` for each
    /// address the name stands for.
    pub fn start(address: &str, timeout: Duration) -> Self {
        let address = with_port(address);
        let (found_tx, found) = unbounded();
        let spawned = thread::Builder::new().name("versus guest".into()).spawn({
            let found_tx = found_tx.clone();
            move || {
                let _ = found_tx.send(connect(&address, timeout));
            }
        });
        if let Err(err) = spawned {
            let _ = found_tx.send(Err(err));
        }
        Self { found }
    }

    /// The stream once connected, or why not.
    pub fn poll(&self) -> Option<io::Result<TcpStream>> {
        self.found.try_recv().ok()
    }
}

fn connect(address: &str, timeout: Duration) -> io::Result<TcpStream> {
    let mut last = io::Error::new(ErrorKind::NotFound, format!("no address for {address}"));
    for address in address.to_socket_addrs()? {
        match TcpStream::connect_timeout(&address, timeout) {
            Ok(stream) => return Ok(stream),
            Err(err) => last = err,
        }
    }
    Err(last)
}

/// Two links joined on this machine, for tests.
#[cfg(test)]
pub(crate) fn joined(timing: Timing) -> (Link, Link) {
    let listener = Listener::start(host_address(0, true)).expect("a port");
    let guest = TcpStream::connect(listener.address()).expect("connected");
    let host = loop {
        if let Some(stream) = listener.poll() {
            break stream.expect("accepted");
        }
        thread::sleep(Duration::from_millis(2));
    };
    (
        Link::new(host, timing).expect("a link"),
        Link::new(guest, timing).expect("a link"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait(link: &mut Link) -> Incoming {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(incoming) = link.receive() {
                return incoming;
            }
            assert!(Instant::now() < deadline, "nothing came");
            thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn messages_go_both_ways_and_a_close_is_seen() {
        let (mut host, mut guest) = joined(Timing::default());
        host.send(Message::Ready);
        host.send(Message::Rematch { bits: 5 });
        guest.send(Message::Desync { turn: 2 });
        assert_eq!(wait(&mut guest), Incoming::Message(Message::Ready));
        assert_eq!(
            wait(&mut guest),
            Incoming::Message(Message::Rematch { bits: 5 })
        );
        assert_eq!(
            wait(&mut host),
            Incoming::Message(Message::Desync { turn: 2 })
        );
        // A leave goes out before the close, and the other side sees both.
        host.send(Message::Leave);
        host.finish(Duration::from_millis(200));
        assert_eq!(wait(&mut guest), Incoming::Message(Message::Leave));
        assert_eq!(wait(&mut guest), Incoming::Lost(Lost::Closed));
    }

    #[test]
    fn pings_keep_a_quiet_link_and_silence_loses_it() {
        let timing = Timing {
            heartbeat: Duration::from_millis(30),
            silence: Duration::from_millis(300),
        };
        let (mut host, guest) = joined(timing);
        // Quiet for longer than the silence, but pinged: nothing is lost.
        thread::sleep(Duration::from_millis(600));
        assert_eq!(host.receive(), None);
        drop(guest);
        assert_eq!(wait(&mut host), Incoming::Lost(Lost::Closed));
        // A peer that connects and never sends anything is silent.
        let listener = Listener::start(host_address(0, true)).expect("a port");
        let mute = TcpStream::connect(listener.address()).expect("connected");
        let stream = loop {
            if let Some(stream) = listener.poll() {
                break stream.expect("accepted");
            }
            thread::sleep(Duration::from_millis(2));
        };
        let started = Instant::now();
        let mut link = Link::new(stream, timing).expect("a link");
        assert_eq!(wait(&mut link), Incoming::Lost(Lost::Silent));
        assert!(started.elapsed() >= Duration::from_millis(250));
        drop(mute);
    }

    #[test]
    fn bytes_that_are_no_message_end_the_link() {
        let listener = Listener::start(host_address(0, true)).expect("a port");
        let mut raw = TcpStream::connect(listener.address()).expect("connected");
        let stream = loop {
            if let Some(stream) = listener.poll() {
                break stream.expect("accepted");
            }
            thread::sleep(Duration::from_millis(2));
        };
        let mut link = Link::new(stream, Timing::default()).expect("a link");
        io::Write::write_all(&mut raw, &[1, 0, 0, 0, 99]).expect("written");
        assert!(matches!(
            wait(&mut link),
            Incoming::Lost(Lost::Malformed(_))
        ));
    }

    #[test]
    fn a_guest_connects_in_the_background() {
        let listener = Listener::start(host_address(0, true)).expect("a port");
        let connecting = Connecting::start(&listener.address().to_string(), CONNECT_TIMEOUT);
        let deadline = Instant::now() + Duration::from_secs(5);
        let (mut host, mut guest) = (None, None);
        while host.is_none() || guest.is_none() {
            host = host.or_else(|| listener.poll());
            guest = guest.or_else(|| connecting.poll());
            assert!(Instant::now() < deadline, "no connection");
            thread::sleep(Duration::from_millis(2));
        }
        assert!(host.expect("host").is_ok() && guest.expect("guest").is_ok());
        // Nobody listens on a port that was just freed.
        let address = {
            let gone = Listener::start(host_address(0, true)).expect("a port");
            gone.address()
        };
        thread::sleep(Duration::from_millis(100));
        let refused = Connecting::start(&address.to_string(), Duration::from_secs(1));
        let deadline = Instant::now() + Duration::from_secs(5);
        let answer = loop {
            if let Some(answer) = refused.poll() {
                break answer;
            }
            assert!(Instant::now() < deadline, "no answer");
            thread::sleep(Duration::from_millis(2));
        };
        assert!(answer.is_err());
    }

    /// Waits for the listener's stream.
    fn accepted(listener: &Listener) -> TcpStream {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(stream) = listener.poll() {
                break stream.expect("accepted");
            }
            assert!(Instant::now() < deadline, "nobody came");
            thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn a_dropped_link_cuts_its_connection_and_ends_its_threads_at_once() {
        // The other side stays and never closes: before, the reading thread of the
        // dropped link waited for it until the silence.
        let listener = Listener::start(host_address(0, true)).expect("a port");
        let mut stays = TcpStream::connect(listener.address()).expect("connected");
        let mut link = Link::new(accepted(&listener), Timing::default()).expect("a link");
        let incoming = link.incoming.clone();
        link.send(Message::Leave);
        let dropped = Instant::now();
        drop(link);
        assert!(dropped.elapsed() < ENDS_WITHIN, "{:?}", dropped.elapsed());
        // Both threads ended: nothing holds the sending end of the channel any more.
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            match incoming.recv_deadline(deadline) {
                Ok(_) => {}
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => panic!("a thread lives on"),
            }
        }
        // The other side got the last message, then the end of the connection.
        assert_eq!(read_frame(&mut stays).expect("a frame"), Message::Leave);
        assert!(read_frame(&mut stays).is_err());
    }

    #[test]
    fn a_host_hosts_again_on_the_same_port_at_once() {
        // Title, arena, title, arena in one process: every time the same port, and the
        // guest of the first time still connected while the second listens.
        let port = 47890;
        let mut kept = Vec::new();
        for time in 0..4 {
            let listener = match Listener::start(host_address(port, true)) {
                Ok(listener) => listener,
                Err(err) => panic!("time {time}: {err}"),
            };
            let guest = Connecting::start(&format!("127.0.0.1:{port}"), CONNECT_TIMEOUT);
            let host = Link::new(accepted(&listener), Timing::default()).expect("a link");
            let guest = loop {
                if let Some(stream) = guest.poll() {
                    break stream.expect("joined");
                }
                thread::sleep(Duration::from_millis(2));
            };
            let guest = Link::new(guest, Timing::default()).expect("a link");
            drop(listener);
            drop(host);
            // The odd times the guest goes too; the even ones it stays connected.
            match time % 2 {
                0 => kept.push(guest),
                _ => drop(guest),
            }
        }
        // A listener that saw nobody frees its port too.
        for _ in 0..3 {
            drop(Listener::start(host_address(port, true)).expect("the port again"));
        }
    }

    #[test]
    fn a_host_takes_a_guest_of_ipv6_too() {
        let listener = Listener::start(host_address(0, true)).expect("a port");
        let address = format!("[::1]:{}", listener.address().port());
        let Ok(mut guest) = TcpStream::connect(&address) else {
            println!("this machine has no IPv6: {address} not tried");
            return;
        };
        let mut link = Link::new(accepted(&listener), Timing::default()).expect("a link");
        write_frame(&mut guest, &Message::Ready).expect("written");
        assert_eq!(wait(&mut link), Incoming::Message(Message::Ready));
        assert!(link.peer().is_some_and(|peer| peer.is_ipv6()));
    }

    #[test]
    fn an_address_gets_the_default_port() {
        assert_eq!(with_port("192.168.1.20"), "192.168.1.20:47820");
        assert_eq!(with_port(" 10.0.0.2:5000 "), "10.0.0.2:5000");
        assert_eq!(with_port("::1"), "[::1]:47820");
        assert_eq!(with_port("[::1]:9"), "[::1]:9");
        assert_eq!(with_port("my-pc"), "my-pc:47820");
        assert_eq!(with_port("my-pc:5000"), "my-pc:5000");
        assert_eq!(host_address(47820, true).to_string(), "127.0.0.1:47820");
        assert_eq!(host_address(47820, false).to_string(), "0.0.0.0:47820");
    }
}
