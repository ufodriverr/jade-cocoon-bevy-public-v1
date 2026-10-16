
use std::fmt;
use std::io::{self, ErrorKind};
use std::net::{IpAddr, Ipv4Addr, Shutdown, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, unbounded};
use iroh::dns::{DnsResolver, NameserverConfig};
use iroh::endpoint::{
    Connection, ConnectionError, PortmapperConfig, ReadError, RecvStream, SendStream, presets,
};
use iroh::{Endpoint, EndpointAddr, EndpointId, SecretKey, TransportAddr};
use tokio::runtime::{Handle, Runtime};
use tokio::sync::oneshot;

/// The protocol both ends speak (QUIC's ALPN): another game, or another form of this one,
/// is turned away in the handshake.
pub const ALPN: &[u8] = b"jade-cocoon-bevy/arena/1";
/// What a guest writes first on its stream, which the host reads before it takes the
/// guest (QUIC shows a new stream to the other side only with its first bytes).
const KNOCK: &[u8; 4] = b"JCAB";
/// The letters of a code, without the dashes.
pub const CODE_LETTERS: usize = 12;
/// The letters a code is written with: the digits and the small letters but i, l, o
/// and u (Crockford's base 32), so that nothing reads as something else.
const ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";
/// The context of the key made from a code; a change of it changes every key.
const KEY_CONTEXT: &str = "jade-cocoon-bevy 2026-10-16 arena room code";
/// How long a guest looks for the host of a code.
pub const FIND_WITHIN: Duration = Duration::from_secs(20);
/// How long a host waits for the guest's first bytes once it connected.
const KNOCK_WITHIN: Duration = Duration::from_secs(10);
/// How long a closing connection waits for its last bytes to be taken and for the other
/// side to close first, in the background.
const CLOSE_WITHIN: Duration = Duration::from_secs(2);
/// The public resolvers a check asks over TCP (no UDP socket of its own).
const CHECK_RESOLVERS: [Ipv4Addr; 2] = [Ipv4Addr::new(1, 1, 1, 1), Ipv4Addr::new(8, 8, 8, 8)];

/// A room code: what a host shows and a guest types or pastes.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Code([u8; CODE_LETTERS]);

/// Why a text is no code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeError {
    Empty,
    /// Fewer letters than a code has.
    Short(usize),
    /// More letters than a code has.
    Long(usize),
    /// A sign that no code holds.
    Letter(char),
}

impl fmt::Display for CodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodeError::Empty => write!(f, "no code"),
            CodeError::Short(count) => {
                write!(f, "{count} of the {CODE_LETTERS} letters of a code")
            }
            CodeError::Long(count) => write!(f, "{count} letters, a code has {CODE_LETTERS}"),
            CodeError::Letter(letter) => write!(f, "a code has no {letter:?}"),
        }
    }
}

impl std::error::Error for CodeError {}

impl Code {
    /// A new code from the system's secure randomness: 60 bits.
    pub fn random() -> Self {
        let bits = SecretKey::generate().to_bytes();
        let mut letters = [0; CODE_LETTERS];
        for (at, letter) in letters.iter_mut().enumerate() {
            // Five bits a letter, the low five of each byte of the random key.
            *letter = ALPHABET[usize::from(bits[at] & 31)];
        }
        Self(letters)
    }

    /// The code in a text as a player gives it: with or without the dashes, with spaces,
    /// a line's end or a quote around it, in small or capital letters; an O is read as a
    /// zero, an I or an L as a one.
    pub fn parse(text: &str) -> Result<Self, CodeError> {
        let mut letters = Vec::with_capacity(CODE_LETTERS);
        for sign in text.chars() {
            if sign.is_whitespace() || "-_.'\"`".contains(sign) {
                continue;
            }
            let letter = match sign.to_ascii_lowercase() {
                'o' => '0',
                'i' | 'l' => '1',
                letter => letter,
            };
            if !letter.is_ascii() || !ALPHABET.contains(&(letter as u8)) {
                return Err(CodeError::Letter(sign));
            }
            letters.push(letter as u8);
        }
        match letters.len() {
            0 => Err(CodeError::Empty),
            count if count < CODE_LETTERS => Err(CodeError::Short(count)),
            count if count > CODE_LETTERS => Err(CodeError::Long(count)),
            _ => {
                let mut code = [0; CODE_LETTERS];
                code.copy_from_slice(&letters);
                Ok(Self(code))
            }
        }
    }

    /// The host's secret key, made from the code: whoever knows the code knows the key,
    /// so the code goes only to the friend who joins, and a new one comes each hosting.
    pub fn secret(&self) -> SecretKey {
        SecretKey::from_bytes(&blake3::derive_key(KEY_CONTEXT, &self.0))
    }

    /// The host's endpoint, as the guest dials it.
    pub fn host_id(&self) -> EndpointId {
        self.secret().public()
    }
}

/// In groups of four for reading: `k3fm-q8xa-7wpe`.
impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (at, group) in self.0.chunks(4).enumerate() {
            if at > 0 {
                f.write_str("-")?;
            }
            f.write_str(std::str::from_utf8(group).map_err(|_| fmt::Error)?)?;
        }
        Ok(())
    }
}

impl fmt::Debug for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Code({self})")
    }
}

/// Which ways an endpoint may take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// Real play: UDP on every network of the PC, the router asked for a port (UPnP,
    /// PCP, NAT-PMP), the relays as the fallback.
    Everywhere,
    /// A check: the UDP socket on 127.0.0.1 only, no port mapping, the names looked up
    /// over TCP; the relays and n0's DNS as in real play.
    Here,
    /// A check of the way over the internet: no UDP socket of its own at all, so the two
    /// sides can only meet through a relay.
    RelayOnly,
    /// A test without the internet: 127.0.0.1 only, no relay, no lookup; the guest must
    /// be given the host's address.
    Offline,
}

/// How the packets of a connection go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// Straight from one PC to the other.
    Direct,
    /// Through one of n0's relays (still encrypted end to end).
    Relayed,
}

impl fmt::Display for Route {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Route::Direct => write!(f, "Direct"),
            Route::Relayed => write!(f, "Relayed"),
        }
    }
}

/// The runtime of the online play: two worker threads, made the first time it is asked
/// for and kept until the process ends.
pub fn runtime() -> io::Result<&'static Runtime> {
    static RUNTIME: OnceLock<Result<Runtime, String>> = OnceLock::new();
    let made = RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("online")
            .enable_all()
            .build()
            .map_err(|err| err.to_string())
    });
    made.as_ref()
        .map_err(|err| io::Error::other(format!("no runtime for the online play: {err}")))
}

fn other(err: impl fmt::Display) -> io::Error {
    io::Error::other(err.to_string())
}

/// An endpoint of `reach`, with the host's key of a code or a key of its own.
async fn bind(reach: Reach, secret: Option<SecretKey>) -> io::Result<Endpoint> {
    let mut builder = match reach {
        Reach::Offline => Endpoint::builder(presets::Minimal),
        _ => Endpoint::builder(presets::N0),
    };
    builder = builder.alpns(vec![ALPN.to_vec()]);
    if let Some(secret) = secret {
        builder = builder.secret_key(secret);
    }
    if reach != Reach::Everywhere {
        builder = builder.portmapper_config(PortmapperConfig::Disabled);
    }
    builder = match reach {
        Reach::Everywhere => builder,
        Reach::RelayOnly => builder.clear_ip_transports(),
        Reach::Here | Reach::Offline => builder
            .clear_ip_transports()
            .bind_addr(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .map_err(other)?,
    };
    if matches!(reach, Reach::Here | Reach::RelayOnly) {
        let resolver = CHECK_RESOLVERS
            .into_iter()
            .fold(DnsResolver::builder(), |builder, ip| {
                builder.add_nameserver_config(NameserverConfig::tcp(IpAddr::V4(ip)))
            })
            .disable_fallback()
            .build();
        builder = builder.dns_resolver(resolver);
    }
    builder.bind().await.map_err(other)
}

/// A host waiting for its one guest, from [`Hosting::start`] on, in the background.
pub struct Hosting {
    code: Code,
    found: Receiver<io::Result<Stream>>,
    /// The endpoint reached its relay: a guest can find it from anywhere.
    online: Arc<AtomicBool>,
    /// The addresses its socket is bound to, once bound.
    bound: Arc<Mutex<Vec<SocketAddr>>>,
    stop: Option<oneshot::Sender<()>>,
}

impl Hosting {
    /// Hosts under a new code.
    pub fn start(reach: Reach) -> io::Result<Self> {
        Self::start_with(Code::random(), reach)
    }

    /// Hosts under `code`.
    pub fn start_with(code: Code, reach: Reach) -> io::Result<Self> {
        let runtime = runtime()?;
        let (found_tx, found) = unbounded();
        let online = Arc::new(AtomicBool::new(false));
        let bound = Arc::new(Mutex::new(Vec::new()));
        let (stop, stopped) = oneshot::channel();
        runtime.spawn(host(
            code,
            reach,
            found_tx,
            (online.clone(), bound.clone()),
            stopped,
        ));
        Ok(Self {
            code,
            found,
            online,
            bound,
            stop: Some(stop),
        })
    }

    pub fn code(&self) -> Code {
        self.code
    }

    /// Whether a guest can find this host now: it reached its relay (at once without
    /// relays).
    pub fn online(&self) -> bool {
        self.online.load(Ordering::Relaxed)
    }

    /// The addresses the endpoint's socket is bound to (a test dials them).
    pub fn bound(&self) -> Vec<SocketAddr> {
        self.bound
            .lock()
            .map(|bound| bound.clone())
            .unwrap_or_default()
    }

    /// The guest's stream once one came, or why hosting ended.
    pub fn poll(&self) -> Option<io::Result<Stream>> {
        self.found.try_recv().ok()
    }
}

/// A host that goes before a guest came closes its endpoint, in the background.
impl Drop for Hosting {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}

/// The host's task: binds, says when it is online, takes the first guest that knocks
/// and turns every later one away.
async fn host(
    code: Code,
    reach: Reach,
    found: Sender<io::Result<Stream>>,
    (online, bound): (Arc<AtomicBool>, Arc<Mutex<Vec<SocketAddr>>>),
    mut stop: oneshot::Receiver<()>,
) {
    let endpoint = tokio::select! {
        _ = &mut stop => return,
        endpoint = bind(reach, Some(code.secret())) => match endpoint {
            Ok(endpoint) => endpoint,
            Err(err) => {
                let _ = found.send(Err(err));
                return;
            }
        },
    };
    if let Ok(mut bound) = bound.lock() {
        *bound = endpoint.bound_sockets();
    }
    let reached = async {
        if reach != Reach::Offline {
            endpoint.online().await;
        }
        online.store(true, Ordering::Relaxed);
        std::future::pending::<()>().await
    };
    let first = async {
        loop {
            let Some(incoming) = endpoint.accept().await else {
                return Err(io::Error::new(
                    ErrorKind::ConnectionAborted,
                    "the endpoint closed",
                ));
            };
            match take(&endpoint, incoming).await {
                Ok(stream) => return Ok(stream),
                // A stranger, or a guest that went at once: the next one.
                Err(_) => continue,
            }
        }
    };
    tokio::select! {
        _ = &mut stop => endpoint.close().await,
        _ = reached => {}
        taken = first => {
            let came = taken.is_ok();
            let _ = found.send(taken);
            if came {
                // Nobody else comes in; the endpoint lives on with the guest's stream,
                // whose end closes it.
                while let Some(incoming) = endpoint.accept().await {
                    incoming.refuse();
                }
            }
        }
    }
}

/// A guest's connection, taken once it knocked on its stream.
async fn take(endpoint: &Endpoint, incoming: iroh::endpoint::Incoming) -> io::Result<Stream> {
    let connection = incoming.accept().map_err(other)?.await.map_err(other)?;
    let (send, mut recv) = tokio::time::timeout(KNOCK_WITHIN, connection.accept_bi())
        .await
        .map_err(|_| io::Error::new(ErrorKind::TimedOut, "the guest sent nothing"))?
        .map_err(other)?;
    let mut knock = [0; KNOCK.len()];
    tokio::time::timeout(KNOCK_WITHIN, recv.read_exact(&mut knock))
        .await
        .map_err(|_| io::Error::new(ErrorKind::TimedOut, "the guest sent nothing"))?
        .map_err(other)?;
    if &knock != KNOCK {
        connection.close(1u32.into(), b"not a guest of the arena");
        return Err(io::Error::new(ErrorKind::InvalidData, "not a guest"));
    }
    Stream::new(endpoint.clone(), connection, send, recv)
}

/// A guest looking for the host of a code, in the background.
pub struct Joining {
    found: Receiver<io::Result<Stream>>,
    stop: Option<oneshot::Sender<()>>,
}

impl Joining {
    /// Looks for the host of `code` for [`FIND_WITHIN`].
    pub fn start(code: Code, reach: Reach) -> io::Result<Self> {
        Self::start_at(code, reach, Vec::new(), FIND_WITHIN)
    }

    /// Looks for the host of `code` for `within`, at the direct addresses given first (a
    /// test without the internet).
    pub fn start_at(
        code: Code,
        reach: Reach,
        direct: Vec<SocketAddr>,
        within: Duration,
    ) -> io::Result<Self> {
        let runtime = runtime()?;
        let (found_tx, found) = unbounded();
        let (stop, stopped) = oneshot::channel();
        runtime.spawn(async move {
            let deadline = tokio::time::Instant::now() + within;
            tokio::select! {
                _ = stopped => {}
                joined = join(code, reach, direct, deadline) => {
                    let _ = found_tx.send(joined);
                }
            }
        });
        Ok(Self {
            found,
            stop: Some(stop),
        })
    }

    /// The stream once connected, or "No host found for this code".
    pub fn poll(&self) -> Option<io::Result<Stream>> {
        self.found.try_recv().ok()
    }
}

impl Drop for Joining {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}

/// The error of a code nobody hosts under (or a host that cannot be reached).
pub fn not_found(why: &str) -> io::Error {
    io::Error::new(
        ErrorKind::NotFound,
        format!("No host found for this code ({why})"),
    )
}

/// The guest's task: dials the host of the code again and again until it answers or the
/// time is up, then knocks.
async fn join(
    code: Code,
    reach: Reach,
    direct: Vec<SocketAddr>,
    deadline: tokio::time::Instant,
) -> io::Result<Stream> {
    let endpoint = tokio::time::timeout_at(deadline, bind(reach, None))
        .await
        .map_err(|_| not_found("no network"))??;
    let address =
        EndpointAddr::from_parts(code.host_id(), direct.into_iter().map(TransportAddr::Ip));
    let mut last = String::from("nobody answered");
    let connection = loop {
        match tokio::time::timeout_at(deadline, endpoint.connect(address.clone(), ALPN)).await {
            Ok(Ok(connection)) => break connection,
            Ok(Err(err)) => last = err.to_string(),
            Err(_) => {
                close_later(endpoint);
                return Err(not_found(&last));
            }
        }
        // The host may not have published its address yet: once more in a moment (at the
        // deadline the next try ends at once).
        let again = tokio::time::Instant::now() + Duration::from_millis(500);
        tokio::time::sleep_until(again.min(deadline)).await;
    };
    let opened = async {
        let (mut send, recv) = connection.open_bi().await.map_err(other)?;
        send.write_all(KNOCK).await.map_err(other)?;
        io::Result::Ok((send, recv))
    };
    match tokio::time::timeout_at(deadline + KNOCK_WITHIN, opened).await {
        Ok(Ok((send, recv))) => Stream::new(endpoint, connection, send, recv),
        Ok(Err(err)) => {
            close_later(endpoint);
            Err(err)
        }
        Err(_) => {
            close_later(endpoint);
            Err(not_found("the host did not answer"))
        }
    }
}

/// An endpoint that is not needed any more closes in the background: a close waits up
/// to some seconds for the other side, which nobody should wait for.
fn close_later(endpoint: Endpoint) {
    tokio::spawn(async move { endpoint.close().await });
}

/// One bidirectional stream of a connection, read and written by blocking calls with a
/// time limit, as a `TcpStream` is; clones share it (one thread reads, one writes).
#[derive(Clone)]
pub struct Stream {
    shared: Arc<Shared>,
}

struct Shared {
    handle: Handle,
    endpoint: Endpoint,
    connection: Connection,
    send: Mutex<SendStream>,
    recv: Mutex<RecvStream>,
    read_timeout: Mutex<Option<Duration>>,
    write_timeout: Mutex<Option<Duration>>,
    /// The connection is being closed.
    cut: AtomicBool,
}

impl Stream {
    fn new(
        endpoint: Endpoint,
        connection: Connection,
        send: SendStream,
        recv: RecvStream,
    ) -> io::Result<Self> {
        Ok(Self {
            shared: Arc::new(Shared {
                handle: runtime()?.handle().clone(),
                endpoint,
                connection,
                send: Mutex::new(send),
                recv: Mutex::new(recv),
                read_timeout: Mutex::new(None),
                write_timeout: Mutex::new(None),
                cut: AtomicBool::new(false),
            }),
        })
    }

    /// How the packets go now: straight, or through a relay. It may change while the
    /// connection lasts (a relay first, then straight once the holes are punched).
    pub fn route(&self) -> Option<Route> {
        let paths = self.shared.connection.paths();
        let selected = paths.iter().find(|path| path.is_selected());
        selected.map(|path| match path.is_relay() {
            true => Route::Relayed,
            false => Route::Direct,
        })
    }

    /// The other side's endpoint, short.
    pub fn peer(&self) -> String {
        self.shared.connection.remote_id().fmt_short().to_string()
    }

    pub fn set_read_timeout(&self, timeout: Option<Duration>) {
        if let Ok(mut set) = self.shared.read_timeout.lock() {
            *set = timeout;
        }
    }

    pub fn set_write_timeout(&self, timeout: Option<Duration>) {
        if let Ok(mut set) = self.shared.write_timeout.lock() {
            *set = timeout;
        }
    }

    fn timeout(of: &Mutex<Option<Duration>>) -> Option<Duration> {
        of.lock().ok().and_then(|timeout| *timeout)
    }

    /// Reads what came, waiting at most the read time-out (then an error of kind
    /// `TimedOut`); 0 once the other side finished its stream or closed.
    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        let shared = &self.shared;
        let mut recv = shared
            .recv
            .lock()
            .map_err(|_| other("the reading half is poisoned"))?;
        // `RecvStream::read` may be dropped unfinished: nothing is lost then.
        let read = recv.read(buf);
        let read = match Self::timeout(&shared.read_timeout) {
            Some(timeout) => shared
                .handle
                .block_on(async { tokio::time::timeout(timeout, read).await })
                .map_err(|_| io::Error::new(ErrorKind::TimedOut, "nothing came yet"))?,
            None => shared.handle.block_on(read),
        };
        match read {
            Ok(Some(count)) => Ok(count),
            Ok(None) => Ok(0),
            Err(ReadError::ConnectionLost(
                ConnectionError::ApplicationClosed(_) | ConnectionError::LocallyClosed,
            )) => Ok(0),
            Err(ReadError::Reset(code)) => Err(io::Error::new(
                ErrorKind::ConnectionReset,
                format!("the other side reset its stream ({code})"),
            )),
            Err(err) => Err(io::Error::new(
                ErrorKind::ConnectionAborted,
                err.to_string(),
            )),
        }
    }

    /// Writes all of `buf`, waiting at most the write time-out.
    pub fn write_all(&self, buf: &[u8]) -> io::Result<()> {
        let shared = &self.shared;
        let mut send = shared
            .send
            .lock()
            .map_err(|_| other("the writing half is poisoned"))?;
        let write = send.write_all(buf);
        let written = match Self::timeout(&shared.write_timeout) {
            Some(timeout) => shared
                .handle
                .block_on(async { tokio::time::timeout(timeout, write).await })
                .map_err(|_| io::Error::new(ErrorKind::TimedOut, "the other side takes nothing"))?,
            None => shared.handle.block_on(write),
        };
        written.map_err(|err| io::Error::new(ErrorKind::ConnectionAborted, err.to_string()))
    }

    /// `Write`: the end of this side's stream (what is written still goes out). `Read`
    /// or `Both`: the connection closes, in the background, once the last bytes were
    /// taken and the other side closed too, or after [`CLOSE_WITHIN`] at the most each.
    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        let shared = &self.shared;
        // A writing thread stuck in a write holds the half: its own end finishes it.
        let stopped = shared.send.try_lock().ok().map(|mut send| {
            let _ = send.finish();
            send.stopped()
        });
        if how == Shutdown::Write || shared.cut.swap(true, Ordering::Relaxed) {
            return Ok(());
        }
        let connection = shared.connection.clone();
        let endpoint = shared.endpoint.clone();
        shared.handle.spawn(async move {
            if let Some(stopped) = stopped {
                let _ = tokio::time::timeout(CLOSE_WITHIN, stopped).await;
            }
            let _ = tokio::time::timeout(CLOSE_WITHIN, connection.closed()).await;
            connection.close(0u32.into(), b"bye");
            endpoint.close().await;
        });
        Ok(())
    }
}

/// The last clone of a stream closes its connection (gently, in the background).
impl Drop for Shared {
    fn drop(&mut self) {
        if self.cut.swap(true, Ordering::Relaxed) {
            return;
        }
        let stopped = self.send.get_mut().ok().map(|send| {
            let _ = send.finish();
            send.stopped()
        });
        let connection = self.connection.clone();
        let endpoint = self.endpoint.clone();
        self.handle.spawn(async move {
            if let Some(stopped) = stopped {
                let _ = tokio::time::timeout(CLOSE_WITHIN, stopped).await;
            }
            connection.close(0u32.into(), b"bye");
            endpoint.close().await;
        });
    }
}

#[cfg(test)]
mod tests;
