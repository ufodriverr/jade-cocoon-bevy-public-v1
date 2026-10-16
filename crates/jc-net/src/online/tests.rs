//! The room code and the stream of `online.rs`, on this machine without the internet
//! ([`Reach::Offline`]: 127.0.0.1, no relay, no lookup; the guest is given the host's
//! address).

use std::thread;
use std::time::Instant;

use super::*;
use crate::link::{Incoming, Link, Lost, Timing, Transport};
use crate::wire::Message;

#[test]
fn a_code_is_read_with_or_without_dashes_spaces_and_capitals() {
    let code = Code::parse("k3fm-q8xa-7wpe").expect("a code");
    assert_eq!(code.to_string(), "k3fm-q8xa-7wpe");
    for text in [
        "k3fmq8xa7wpe",
        "K3FM-Q8XA-7WPE",
        "  k3fm q8xa 7wpe\r\n",
        "k3fm-q8xa-7wpe\n",
        "\"k3fm-q8xa-7wpe\"",
        "k3FM_q8xa.7wPE",
    ] {
        assert_eq!(Code::parse(text), Ok(code), "{text:?}");
    }
    // An O is a zero, an I or an L a one.
    assert_eq!(Code::parse("abcd-efgh-jkOI"), Code::parse("abcd-efgh-jk01"));
    assert_eq!(Code::parse("abcd-efgh-jk0L"), Code::parse("abcd-efgh-jk01"));
}

#[test]
fn what_is_no_code_says_why() {
    assert_eq!(Code::parse(""), Err(CodeError::Empty));
    assert_eq!(Code::parse(" - \n"), Err(CodeError::Empty));
    assert_eq!(Code::parse("k3fm-q8xa"), Err(CodeError::Short(8)));
    assert_eq!(Code::parse("k3fm-q8xa-7wpe-1"), Err(CodeError::Long(13)));
    assert_eq!(Code::parse("k3fm-q8xa-7wpu"), Err(CodeError::Letter('u')));
    assert_eq!(Code::parse("k3fm/q8xa-7wpe"), Err(CodeError::Letter('/')));
    assert_eq!(Code::parse("k3fm-q8xa-7wpé"), Err(CodeError::Letter('é')));
    assert!(CodeError::Short(8).to_string().contains("8 of the 12"));
}

#[test]
fn new_codes_differ_read_back_and_name_their_host() {
    let codes: Vec<Code> = (0..16).map(|_| Code::random()).collect();
    assert!(codes.iter().any(|code| *code != codes[0]));
    for code in &codes {
        let shown = code.to_string();
        assert_eq!(shown.len(), CODE_LETTERS + 2, "{shown}");
        assert_eq!(Code::parse(&shown).as_ref(), Ok(code));
        // The same code is the same host, on every machine.
        assert_eq!(code.host_id(), code.secret().public());
        assert_eq!(
            code.host_id(),
            Code::parse(&shown).expect("a code").host_id()
        );
    }
    assert_ne!(
        codes[0].host_id(),
        Code::parse("0000-0000-0000").expect("a code").host_id()
    );
}

/// Waits for what a host or a guest found.
fn found(poll: impl Fn() -> Option<io::Result<Stream>>, within: Duration) -> io::Result<Stream> {
    let deadline = Instant::now() + within;
    loop {
        if let Some(found) = poll() {
            return found;
        }
        assert!(Instant::now() < deadline, "nothing came in {within:?}");
        thread::sleep(Duration::from_millis(5));
    }
}

/// A host and a guest of one code on this machine, and the guest's stream and the host's.
fn pair() -> (Stream, Stream) {
    let code = Code::random();
    let hosting = Hosting::start_with(code, Reach::Offline).expect("hosting");
    let deadline = Instant::now() + Duration::from_secs(5);
    while hosting.bound().is_empty() {
        assert!(Instant::now() < deadline, "the host never bound");
        thread::sleep(Duration::from_millis(5));
    }
    let joining = Joining::start_at(
        code,
        Reach::Offline,
        hosting.bound(),
        Duration::from_secs(10),
    )
    .expect("joining");
    let guest = found(|| joining.poll(), Duration::from_secs(15)).expect("joined");
    let host = found(|| hosting.poll(), Duration::from_secs(5)).expect("a guest came");
    (host, guest)
}

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
fn two_ends_of_a_code_meet_and_a_link_runs_on_them() {
    let (host, guest) = pair();
    // Nothing but this machine: the straight way.
    assert_eq!(host.route(), Some(Route::Direct));
    assert_eq!(guest.route(), Some(Route::Direct));
    let mut host = Link::new(host, Timing::default()).expect("a link");
    let mut guest = Link::new(guest, Timing::default()).expect("a link");
    assert_eq!(Transport::route(&host), Some(Route::Direct));
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
    // Quiet for a while: the pings keep it.
    thread::sleep(Duration::from_millis(1500));
    assert_eq!(host.receive(), None);
    // A leave goes out before the close, and the other side sees both.
    host.send(Message::Leave);
    host.finish(Duration::from_millis(500));
    assert_eq!(wait(&mut guest), Incoming::Message(Message::Leave));
    assert_eq!(wait(&mut guest), Incoming::Lost(Lost::Closed));
}

#[test]
fn a_dropped_link_of_a_code_ends_at_once_and_the_other_side_sees_it() {
    let (host, guest) = pair();
    let host = Link::new(host, Timing::default()).expect("a link");
    let mut guest = Link::new(guest, Timing::default()).expect("a link");
    let dropped = Instant::now();
    drop(host);
    assert!(
        dropped.elapsed() < crate::link::ENDS_WITHIN,
        "{:?}",
        dropped.elapsed()
    );
    assert_eq!(wait(&mut guest), Incoming::Lost(Lost::Closed));
}

#[test]
fn a_wrong_code_finds_no_host_in_time_and_never_hangs() {
    // A host is up, but under another code: its key is not the one dialed.
    let hosting = Hosting::start(Reach::Offline).expect("hosting");
    let deadline = Instant::now() + Duration::from_secs(5);
    while hosting.bound().is_empty() {
        assert!(Instant::now() < deadline, "the host never bound");
        thread::sleep(Duration::from_millis(5));
    }
    let within = Duration::from_secs(3);
    for direct in [hosting.bound(), Vec::new()] {
        let started = Instant::now();
        let joining = Joining::start_at(Code::random(), Reach::Offline, direct.clone(), within)
            .expect("joining");
        let answer = found(|| joining.poll(), within + Duration::from_secs(5));
        let took = started.elapsed();
        let err = answer.err().expect("no host");
        assert_eq!(err.kind(), ErrorKind::NotFound, "{err}");
        assert!(
            err.to_string().starts_with("No host found for this code"),
            "{err}"
        );
        assert!(
            took >= within - Duration::from_millis(100),
            "{direct:?}: {took:?}"
        );
        assert!(
            took < within + Duration::from_secs(4),
            "{direct:?}: {took:?}"
        );
    }
    // Nobody came to the host.
    assert!(hosting.poll().is_none());
}
