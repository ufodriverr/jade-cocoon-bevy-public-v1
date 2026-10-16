//! The messages of the fight of two players and their bytes. A message travels as a
//! frame: its length (a little-endian word, at most [`MOST_FRAME`]) and then its bytes,
//! the kind first and then the fields, numbers little-endian, a text as its length (a
//! byte) and UTF-8. Reading checks every bound and wants every byte used, so whatever
//! the other side sends is either a message of this form or an error.
//!
//! The hello keeps its form in every version of the protocol (the mark, the protocol,
//! the build, the rules, the name), so that two builds that speak different protocols
//! still read each other's hello and can say why they refuse.

use std::fmt;
use std::io::{self, Read, Write};

use jc_formats::battle::arena::PLACE_COUNT;
use jc_formats::battle::duel::{Choice, MOST, Options};
use jc_formats::creatures::Creature;
use jc_formats::items::PAIRS;
use jc_formats::player_panel::spells::SPELLS;

/// The longest frame.
pub const MOST_FRAME: usize = 4096;
/// The mark a hello starts with.
pub const MARK: [u8; 4] = *b"JCVS";
/// The longest player name and build, in bytes.
pub const MOST_NAME: usize = 48;
pub const MOST_BUILD: usize = 32;
/// The longest reason of a refusal, in bytes.
pub const MOST_REASON: usize = 240;
/// The longest team (`Team::to_bytes`): the count, three records, the count of items and
/// two bytes an item.
pub const MOST_TEAM: usize = 1 + MOST * Creature::BYTES + 1 + 2 * PAIRS;
/// The place byte for "a place from the seed".
const RANDOM_PLACE: u8 = 0xFF;
/// The body attacks of a record: the places of its move entries.
const BODY_SLOTS: usize = 5;

/// What the host proposes in the lobby: the options of the duel and the place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Terms {
    pub options: Options,
    /// One of the 39 places, or `None` for one from the seed of each fight.
    pub place: Option<u8>,
}

/// A message, both ways unless it says otherwise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// The first message of each side: the protocol, the build's version, the hash of the
    /// rules (`crate::rules_hash`) and the player's name.
    Hello {
        protocol: u16,
        build: String,
        rules: u64,
        name: String,
    },
    /// This side will not go on, and why. The last message it sends.
    Refuse { reason: String },
    /// The host's terms and its half of the seed.
    Lobby { terms: Terms, bits: u32 },
    /// The guest takes the terms; its half of the seed.
    Accept { bits: u32 },
    /// The side's team as `Team::to_bytes` makes it, and `Team::hash` of it.
    Team { team: Vec<u8>, hash: u64 },
    /// The side is ready for the next fight.
    Ready,
    /// The command of the side whose turn it is, and the duel's state hash after it.
    Turn {
        turn: u32,
        choice: Choice,
        hash: u64,
    },
    /// The other side's state hash after it took a turn's command.
    Hash { turn: u32, hash: u64 },
    /// The hashes differ at this turn: the fight stops on both sides.
    Desync { turn: u32 },
    /// After a fight: one more, with the same teams; the side's half of the new seed.
    Rematch { bits: u32 },
    /// The side leaves (a loss when a fight is on). The last message it sends.
    Leave,
    /// Sent when nothing else was for a second, so that silence means a lost peer.
    Ping,
}

mod kind {
    pub const HELLO: u8 = 1;
    pub const REFUSE: u8 = 2;
    pub const LOBBY: u8 = 3;
    pub const ACCEPT: u8 = 4;
    pub const TEAM: u8 = 5;
    pub const READY: u8 = 6;
    pub const TURN: u8 = 7;
    pub const HASH: u8 = 8;
    pub const DESYNC: u8 = 9;
    pub const REMATCH: u8 = 10;
    pub const LEAVE: u8 = 11;
    pub const PING: u8 = 12;
}

impl Message {
    /// The name of the kind, for a log or a refusal.
    pub fn name(&self) -> &'static str {
        match self {
            Message::Hello { .. } => "hello",
            Message::Refuse { .. } => "refusal",
            Message::Lobby { .. } => "terms",
            Message::Accept { .. } => "accept",
            Message::Team { .. } => "team",
            Message::Ready => "ready",
            Message::Turn { .. } => "turn",
            Message::Hash { .. } => "hash",
            Message::Desync { .. } => "desync",
            Message::Rematch { .. } => "rematch",
            Message::Leave => "leave",
            Message::Ping => "ping",
        }
    }
}

/// Why bytes are no message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireError {
    Empty,
    UnknownKind(u8),
    /// The bytes end inside a field.
    Short,
    /// Bytes are left after the message.
    Trailing,
    /// A field is out of its bounds.
    Bad(&'static str),
}

impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WireError::Empty => write!(f, "an empty message"),
            WireError::UnknownKind(kind) => write!(f, "a message of unknown kind {kind}"),
            WireError::Short => write!(f, "a message cut short"),
            WireError::Trailing => write!(f, "a message with bytes after its end"),
            WireError::Bad(what) => write!(f, "a message with a bad {what}"),
        }
    }
}

impl std::error::Error for WireError {}

/// The bytes of a message, without the frame's length.
pub fn encode(message: &Message) -> Vec<u8> {
    let mut out = Out(Vec::with_capacity(16));
    match message {
        Message::Hello {
            protocol,
            build,
            rules,
            name,
        } => {
            out.byte(kind::HELLO);
            out.0.extend_from_slice(&MARK);
            out.half(*protocol);
            out.text(build, MOST_BUILD);
            out.long(*rules);
            out.text(name, MOST_NAME);
        }
        Message::Refuse { reason } => {
            out.byte(kind::REFUSE);
            out.text(reason, MOST_REASON);
        }
        Message::Lobby { terms, bits } => {
            out.byte(kind::LOBBY);
            out.byte(u8::from(terms.options.items));
            out.byte(u8::from(terms.options.changing));
            out.byte(terms.place.unwrap_or(RANDOM_PLACE));
            out.word(*bits);
        }
        Message::Accept { bits } => {
            out.byte(kind::ACCEPT);
            out.word(*bits);
        }
        Message::Team { team, hash } => {
            out.byte(kind::TEAM);
            let team = &team[..team.len().min(MOST_TEAM)];
            out.half(team.len() as u16);
            out.0.extend_from_slice(team);
            out.long(*hash);
        }
        Message::Ready => out.byte(kind::READY),
        Message::Turn { turn, choice, hash } => {
            out.byte(kind::TURN);
            out.word(*turn);
            out.0.extend_from_slice(&choice_bytes(choice));
            out.long(*hash);
        }
        Message::Hash { turn, hash } => {
            out.byte(kind::HASH);
            out.word(*turn);
            out.long(*hash);
        }
        Message::Desync { turn } => {
            out.byte(kind::DESYNC);
            out.word(*turn);
        }
        Message::Rematch { bits } => {
            out.byte(kind::REMATCH);
            out.word(*bits);
        }
        Message::Leave => out.byte(kind::LEAVE),
        Message::Ping => out.byte(kind::PING),
    }
    out.0
}

/// The message of some bytes; every field in its bounds and every byte used.
pub fn decode(bytes: &[u8]) -> Result<Message, WireError> {
    let mut input = In { bytes, at: 0 };
    let kind = input.byte().map_err(|_| WireError::Empty)?;
    let message = match kind {
        kind::HELLO => {
            if input.take(MARK.len())? != MARK {
                return Err(WireError::Bad("mark"));
            }
            Message::Hello {
                protocol: input.half()?,
                build: input.text(MOST_BUILD, "build")?,
                rules: input.long()?,
                name: input.text(MOST_NAME, "name")?,
            }
        }
        kind::REFUSE => Message::Refuse {
            reason: input.text(MOST_REASON, "reason")?,
        },
        kind::LOBBY => {
            let items = input.flag("items option")?;
            let changing = input.flag("changing option")?;
            let place = match input.byte()? {
                RANDOM_PLACE => None,
                place if usize::from(place) < PLACE_COUNT => Some(place),
                _ => return Err(WireError::Bad("place")),
            };
            Message::Lobby {
                terms: Terms {
                    options: Options { items, changing },
                    place,
                },
                bits: input.word()?,
            }
        }
        kind::ACCEPT => Message::Accept {
            bits: input.word()?,
        },
        kind::TEAM => {
            let len = usize::from(input.half()?);
            if len > MOST_TEAM {
                return Err(WireError::Bad("team length"));
            }
            Message::Team {
                team: input.take(len)?.to_vec(),
                hash: input.long()?,
            }
        }
        kind::READY => Message::Ready,
        kind::TURN => Message::Turn {
            turn: input.word()?,
            choice: choice_of(input.take(3)?)?,
            hash: input.long()?,
        },
        kind::HASH => Message::Hash {
            turn: input.word()?,
            hash: input.long()?,
        },
        kind::DESYNC => Message::Desync {
            turn: input.word()?,
        },
        kind::REMATCH => Message::Rematch {
            bits: input.word()?,
        },
        kind::LEAVE => Message::Leave,
        kind::PING => Message::Ping,
        other => return Err(WireError::UnknownKind(other)),
    };
    match input.at == bytes.len() {
        true => Ok(message),
        false => Err(WireError::Trailing),
    }
}

/// A choice as three bytes: the kind and two values (0 where the kind has none).
fn choice_bytes(choice: &Choice) -> [u8; 3] {
    let small = |n: usize| n.min(255) as u8;
    match *choice {
        Choice::Attack => [0, 0, 0],
        Choice::Defend => [1, 0, 0],
        Choice::Body { slot } => [2, small(slot), 0],
        Choice::Spell { spell, own } => [3, small(spell), u8::from(own)],
        Choice::Change { index } => [4, small(index), 0],
        Choice::Item { item, index } => [5, small(item), small(index)],
        Choice::GiveUp => [6, 0, 0],
    }
}

/// The way back of [`choice_bytes`], with the bounds a choice can have at all; whether it
/// is legal now is the duel's test.
fn choice_of(bytes: &[u8]) -> Result<Choice, WireError> {
    let [kind, a, b] = [bytes[0], bytes[1], bytes[2]];
    let (a, b_zero) = (usize::from(a), b == 0);
    let choice = match kind {
        0 if a == 0 && b_zero => Choice::Attack,
        1 if a == 0 && b_zero => Choice::Defend,
        2 if a < BODY_SLOTS && b_zero => Choice::Body { slot: a },
        3 if a < SPELLS && b < 2 => Choice::Spell {
            spell: a,
            own: b == 1,
        },
        4 if a < MOST && b_zero => Choice::Change { index: a },
        5 if usize::from(b) < MOST => Choice::Item {
            item: a,
            index: usize::from(b),
        },
        6 if a == 0 && b_zero => Choice::GiveUp,
        _ => return Err(WireError::Bad("choice")),
    };
    Ok(choice)
}

/// Why no message came out of a stream.
#[derive(Debug)]
pub enum FrameError {
    Io(io::Error),
    /// The length of a frame is 0 or past [`MOST_FRAME`].
    Length(u32),
    Wire(WireError),
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FrameError::Io(err) => write!(f, "{err}"),
            FrameError::Length(len) => write!(f, "a frame of {len} bytes"),
            FrameError::Wire(err) => write!(f, "{err}"),
        }
    }
}

/// Writes a message as one frame.
pub fn write_frame(stream: &mut impl Write, message: &Message) -> io::Result<()> {
    let bytes = encode(message);
    let mut frame = Vec::with_capacity(4 + bytes.len());
    frame.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    frame.extend_from_slice(&bytes);
    stream.write_all(&frame)?;
    stream.flush()
}

/// Reads one frame and its message.
pub fn read_frame(stream: &mut impl Read) -> Result<Message, FrameError> {
    let mut len = [0; 4];
    stream.read_exact(&mut len).map_err(FrameError::Io)?;
    let len = u32::from_le_bytes(len);
    if len == 0 || len as usize > MOST_FRAME {
        return Err(FrameError::Length(len));
    }
    let mut bytes = vec![0; len as usize];
    stream.read_exact(&mut bytes).map_err(FrameError::Io)?;
    decode(&bytes).map_err(FrameError::Wire)
}

/// The first whole frame of the bytes read so far, taken out of them: `None` while the
/// frame is not whole yet.
pub fn take_frame(bytes: &mut Vec<u8>) -> Result<Option<Message>, FrameError> {
    let Some(len) = bytes.first_chunk::<4>().map(|len| u32::from_le_bytes(*len)) else {
        return Ok(None);
    };
    if len == 0 || len as usize > MOST_FRAME {
        return Err(FrameError::Length(len));
    }
    let end = 4 + len as usize;
    if bytes.len() < end {
        return Ok(None);
    }
    let message = decode(&bytes[4..end]).map_err(FrameError::Wire);
    bytes.drain(..end);
    message.map(Some)
}

struct Out(Vec<u8>);

impl Out {
    fn byte(&mut self, byte: u8) {
        self.0.push(byte);
    }

    fn half(&mut self, half: u16) {
        self.0.extend_from_slice(&half.to_le_bytes());
    }

    fn word(&mut self, word: u32) {
        self.0.extend_from_slice(&word.to_le_bytes());
    }

    fn long(&mut self, long: u64) {
        self.0.extend_from_slice(&long.to_le_bytes());
    }

    /// A text cut to `most` bytes at a character's edge.
    fn text(&mut self, text: &str, most: usize) {
        let mut end = text.len().min(most);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        self.byte(end as u8);
        self.0.extend_from_slice(&text.as_bytes()[..end]);
    }
}

struct In<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> In<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], WireError> {
        let end = self.at.checked_add(len).ok_or(WireError::Short)?;
        let taken = self.bytes.get(self.at..end).ok_or(WireError::Short)?;
        self.at = end;
        Ok(taken)
    }

    fn byte(&mut self) -> Result<u8, WireError> {
        Ok(self.take(1)?[0])
    }

    fn flag(&mut self, what: &'static str) -> Result<bool, WireError> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(WireError::Bad(what)),
        }
    }

    fn half(&mut self) -> Result<u16, WireError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().expect("two")))
    }

    fn word(&mut self) -> Result<u32, WireError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("four")))
    }

    fn long(&mut self) -> Result<u64, WireError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().expect("eight")))
    }

    /// A text of at most `most` bytes, UTF-8, without control characters.
    fn text(&mut self, most: usize, what: &'static str) -> Result<String, WireError> {
        let len = usize::from(self.byte()?);
        if len > most {
            return Err(WireError::Bad(what));
        }
        let text = std::str::from_utf8(self.take(len)?).map_err(|_| WireError::Bad(what))?;
        if text.chars().any(char::is_control) {
            return Err(WireError::Bad(what));
        }
        Ok(text.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_kind() -> Vec<Message> {
        vec![
            Message::Hello {
                protocol: 1,
                build: "0.1.0".into(),
                rules: 0x0123_4567_89AB_CDEF,
                name: "Levant".into(),
            },
            Message::Refuse {
                reason: "the other player runs build 0.2.0".into(),
            },
            Message::Lobby {
                terms: Terms {
                    options: Options {
                        items: false,
                        changing: true,
                    },
                    place: Some(38),
                },
                bits: 0xDEAD_BEEF,
            },
            Message::Lobby {
                terms: Terms::default(),
                bits: 7,
            },
            Message::Accept { bits: 1 },
            Message::Team {
                team: vec![1; 250],
                hash: 99,
            },
            Message::Ready,
            Message::Turn {
                turn: 12,
                choice: Choice::Spell {
                    spell: SPELLS - 1,
                    own: true,
                },
                hash: u64::MAX,
            },
            Message::Turn {
                turn: 1,
                choice: Choice::Item { item: 14, index: 2 },
                hash: 0,
            },
            Message::Hash { turn: 3, hash: 4 },
            Message::Desync { turn: 5 },
            Message::Rematch { bits: 6 },
            Message::Leave,
            Message::Ping,
        ]
    }

    #[test]
    fn every_message_comes_back_as_it_went() {
        for message in every_kind() {
            let bytes = encode(&message);
            assert_eq!(decode(&bytes), Ok(message.clone()), "{message:?}");
            // Cut short anywhere, it is no message.
            for end in 0..bytes.len() {
                assert!(decode(&bytes[..end]).is_err(), "{message:?} cut at {end}");
            }
            let mut longer = bytes.clone();
            longer.push(0);
            assert_eq!(decode(&longer), Err(WireError::Trailing));
        }
        let choices = [
            Choice::Attack,
            Choice::Defend,
            Choice::Body { slot: 4 },
            Choice::Spell {
                spell: 0,
                own: false,
            },
            Choice::Change { index: 2 },
            Choice::Item {
                item: 255,
                index: 0,
            },
            Choice::GiveUp,
        ];
        for choice in choices {
            assert_eq!(choice_of(&choice_bytes(&choice)), Ok(choice));
        }
    }

    #[test]
    fn frames_come_back_from_a_stream() {
        let mut stream = Vec::new();
        for message in every_kind() {
            write_frame(&mut stream, &message).expect("written");
        }
        let mut reading = stream.as_slice();
        for message in every_kind() {
            assert_eq!(read_frame(&mut reading).expect("a message"), message);
        }
        assert!(matches!(read_frame(&mut reading), Err(FrameError::Io(_))));
        let mut huge = (MOST_FRAME as u32 + 1).to_le_bytes().to_vec();
        huge.extend_from_slice(&[0; 8]);
        assert!(matches!(
            read_frame(&mut huge.as_slice()),
            Err(FrameError::Length(_))
        ));
        assert!(matches!(
            read_frame(&mut [0, 0, 0, 0].as_slice()),
            Err(FrameError::Length(0))
        ));
    }

    #[test]
    fn frames_come_out_of_bytes_that_come_in_pieces() {
        let mut stream = Vec::new();
        for message in every_kind() {
            write_frame(&mut stream, &message).expect("written");
        }
        // A byte at a time: a frame comes out once it is whole, and only then.
        let mut bytes = Vec::new();
        let mut taken = Vec::new();
        for &byte in &stream {
            bytes.push(byte);
            while let Some(message) = take_frame(&mut bytes).expect("a frame") {
                taken.push(message);
            }
        }
        assert_eq!(taken, every_kind());
        assert!(bytes.is_empty());
        // A length past the longest is an error before its bytes came.
        let mut huge = (MOST_FRAME as u32 + 1).to_le_bytes().to_vec();
        assert!(matches!(take_frame(&mut huge), Err(FrameError::Length(_))));
        let mut bad = vec![1, 0, 0, 0, 99];
        assert!(matches!(take_frame(&mut bad), Err(FrameError::Wire(_))));
    }

    #[test]
    fn fields_out_of_their_bounds_are_refused() {
        let bad = |bytes: &[u8]| decode(bytes).expect_err("refused");
        assert_eq!(bad(&[]), WireError::Empty);
        assert_eq!(bad(&[0]), WireError::UnknownKind(0));
        assert_eq!(bad(&[200]), WireError::UnknownKind(200));
        // A hello without the mark.
        let mut hello = encode(&every_kind()[0]);
        hello[1] = b'X';
        assert_eq!(bad(&hello), WireError::Bad("mark"));
        // Options are 0 or 1; a place is below 39 or 255.
        assert_eq!(
            bad(&[kind::LOBBY, 2, 0, 0, 0, 0, 0, 0]),
            WireError::Bad("items option")
        );
        assert_eq!(
            bad(&[kind::LOBBY, 1, 1, 39, 0, 0, 0, 0]),
            WireError::Bad("place")
        );
        // Choices: an unknown kind, a body slot past 4, a spell past the list, a team
        // place past 2, a value where the kind has none, a target that is not 0 or 1.
        let turn = |choice: [u8; 3]| {
            let mut bytes = vec![kind::TURN, 1, 0, 0, 0];
            bytes.extend_from_slice(&choice);
            bytes.extend_from_slice(&[0; 8]);
            bytes
        };
        for choice in [
            [7, 0, 0],
            [2, 5, 0],
            [3, SPELLS as u8, 0],
            [3, 0, 2],
            [4, 3, 0],
            [5, 9, 3],
            [0, 1, 0],
            [6, 0, 1],
        ] {
            assert_eq!(bad(&turn(choice)), WireError::Bad("choice"), "{choice:?}");
        }
        assert!(decode(&turn([5, 9, 2])).is_ok());
        // A team longer than three records and the items.
        let mut team = vec![kind::TEAM];
        team.extend_from_slice(&(MOST_TEAM as u16 + 1).to_le_bytes());
        assert_eq!(bad(&team), WireError::Bad("team length"));
        // A name too long, not UTF-8, or with a control character.
        let name = |name: &[u8]| {
            let mut bytes = vec![kind::REFUSE, name.len() as u8];
            bytes.extend_from_slice(name);
            bytes
        };
        assert_eq!(
            bad(&name(&[b'a'; MOST_REASON + 1])),
            WireError::Bad("reason")
        );
        assert_eq!(bad(&name(&[0xFF, 0xFE])), WireError::Bad("reason"));
        assert_eq!(bad(&name(b"a\nb")), WireError::Bad("reason"));
    }

    #[test]
    fn a_long_text_is_cut_at_a_character() {
        let name = "é".repeat(40);
        let bytes = encode(&Message::Hello {
            protocol: 1,
            build: "x".into(),
            rules: 0,
            name,
        });
        let Ok(Message::Hello { name, .. }) = decode(&bytes) else {
            panic!("a hello");
        };
        assert_eq!(name, "é".repeat(MOST_NAME / 2));
    }
}
