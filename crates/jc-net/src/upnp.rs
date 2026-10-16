//! A host by address asks the router to forward its TCP port (UPnP IGD), so that a
//! guest on the internet may join by the host's internet address without any setup of
//! the router by hand. It is a try and nothing more: a router without UPnP, or one
//! behind a carrier's NAT, says nothing alarming, and the room code (`online.rs`) is the
//! way recommended anyway. The forward lasts [`LEASE`] at the most and is taken back when
//! hosting stops ([`Forward`]'s drop).
//!
//! A check never asks: the router is the player's (`versus_view.rs` asks only in real
//! play).

use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crossbeam_channel::{Receiver, unbounded};
use igd_next::{Gateway, PortMappingProtocol, SearchOptions};

/// How long a forward lasts if it is never taken back (a game that crashed).
pub const LEASE: Duration = Duration::from_secs(2 * 60 * 60);
/// How long the router is looked for.
const SEARCH_WITHIN: Duration = Duration::from_secs(4);
/// What the router's list of forwards says of this one.
const DESCRIPTION: &str = "Jade Cocoon on Bevy, Arena";

/// The forward of a host's TCP port, asked for in the background.
pub struct Forward {
    port: u16,
    answer: Receiver<Result<SocketAddr, String>>,
    /// The internet address and port, once the router forwards.
    forwarded: Option<SocketAddr>,
    /// The router that forwards, for the end.
    gateway: Arc<Mutex<Option<Gateway>>>,
}

impl Forward {
    /// Asks the router of this network to forward TCP `port` to this PC.
    pub fn start(port: u16) -> Self {
        let (answer_tx, answer) = unbounded();
        let gateway = Arc::new(Mutex::new(None));
        let kept = gateway.clone();
        let spawned = thread::Builder::new()
            .name("versus upnp".into())
            .spawn(move || {
                let _ = answer_tx.send(forward(port, &kept));
            });
        if let Err(err) = spawned {
            let (failed_tx, failed) = unbounded();
            let _ = failed_tx.send(Err(err.to_string()));
            return Self {
                port,
                answer: failed,
                forwarded: None,
                gateway,
            };
        }
        Self {
            port,
            answer,
            forwarded: None,
            gateway,
        }
    }

    /// The host's internet address and port, once the router forwards; `Err` once it
    /// said no (asked again, it says nothing more).
    pub fn poll(&mut self) -> Option<Result<SocketAddr, String>> {
        if let Some(forwarded) = self.forwarded {
            return Some(Ok(forwarded));
        }
        let answer = self.answer.try_recv().ok()?;
        if let Ok(forwarded) = answer {
            self.forwarded = Some(forwarded);
        }
        Some(answer)
    }
}

/// The forward is taken back, in the background.
impl Drop for Forward {
    fn drop(&mut self) {
        let Some(gateway) = self.gateway.lock().ok().and_then(|mut kept| kept.take()) else {
            return;
        };
        let port = self.port;
        let _ = thread::Builder::new()
            .name("versus upnp end".into())
            .spawn(move || {
                let _ = gateway.remove_port(PortMappingProtocol::TCP, port);
            });
    }
}

/// Whether an address is one of the internet: a router behind a carrier's NAT reports a
/// private one, which nobody outside can join.
pub fn public(ip: Ipv4Addr) -> bool {
    let shared = ip.octets()[0] == 100 && (ip.octets()[1] & 0xC0) == 64;
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_documentation()
        || shared)
}

/// Finds the router, forwards the port to this PC's address on the router's network and
/// asks for the internet address.
fn forward(port: u16, kept: &Mutex<Option<Gateway>>) -> Result<SocketAddr, String> {
    let options = SearchOptions {
        timeout: Some(SEARCH_WITHIN),
        single_search_timeout: Some(SEARCH_WITHIN),
        ..SearchOptions::default()
    };
    let gateway = igd_next::search_gateway(options).map_err(|err| err.to_string())?;
    // This PC's address towards the router: the one a socket aimed at it sends from.
    let probe = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).map_err(|err| err.to_string())?;
    probe.connect(gateway.addr).map_err(|err| err.to_string())?;
    let IpAddr::V4(local) = probe.local_addr().map_err(|err| err.to_string())?.ip() else {
        return Err("the router is not on IPv4".into());
    };
    let external = match gateway.get_external_ip().map_err(|err| err.to_string())? {
        IpAddr::V4(ip) if public(ip) => ip,
        ip => {
            return Err(format!(
                "the router's own address {ip} is not on the internet"
            ));
        }
    };
    gateway
        .add_port(
            PortMappingProtocol::TCP,
            port,
            SocketAddr::V4(SocketAddrV4::new(local, port)),
            LEASE.as_secs() as u32,
            DESCRIPTION,
        )
        .map_err(|err| err.to_string())?;
    if let Ok(mut kept) = kept.lock() {
        *kept = Some(gateway);
    }
    Ok(SocketAddr::V4(SocketAddrV4::new(external, port)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_address_of_the_internet_is_shown() {
        assert!(public(Ipv4Addr::new(84, 10, 20, 30)));
        for ip in [
            Ipv4Addr::new(192, 168, 1, 1),
            Ipv4Addr::new(10, 0, 0, 1),
            Ipv4Addr::new(172, 16, 5, 5),
            Ipv4Addr::new(100, 64, 0, 1),
            Ipv4Addr::new(100, 127, 255, 255),
            Ipv4Addr::new(127, 0, 0, 1),
            Ipv4Addr::new(169, 254, 1, 1),
            Ipv4Addr::UNSPECIFIED,
        ] {
            assert!(!public(ip), "{ip}");
        }
        assert!(public(Ipv4Addr::new(100, 128, 0, 1)));
    }
}
