
use crate::card_screen::{
    ASKS_DELETE, CardScreen, EXTRA_NAMES, HEAD, Routine, SLOTS, Shape, Tables, line, state,
};
use crate::menu::{Menu, Rect, Style, kind, piece, placed, sign};
use crate::text::{SMALL_FONT, TEXT_FONT};

/// The sprites the code names: the words for a slot whose head is not read yet and for
/// one without a file, the stroke between the two numbers of the HP, the words of the
/// card on the slots' heading and the digits of the two ports.
pub const LOADING: (usize, (i16, i16)) = (156, (125, 57));
pub const NO_SAVE: (usize, (i16, i16)) = (116, (139, 57));
pub const STROKE: usize = 218;
pub const CARD: (usize, (i16, i16)) = (39, (191, 21));
pub const PORT_DIGITS: ([usize; 2], (i16, i16)) = ([0, 2], (292, 21));
/// A row of a slot is this much lower than the one above.
pub const ROW_STEP: i16 = 33;
/// The rows on screen.
pub const ROWS: i16 = 4;
/// The two scroll signs of the slots.
pub const SIGNS: [(i16, i16); 2] = [(154, 44), (154, 183)];
/// Where the texts stand: the line of help and the question of state 4, the two lines
/// of state 2, the question of state 5, the words beside the free blocks.
pub const HELP_AT: (i16, i16) = (14, 196);
pub const ASKED_AT: (i16, i16) = (13, 196);
pub const FORMAT_AT: [(i16, i16); 2] = [(19, 73), (19, 87)];
pub const TITLE_AT: (i16, i16) = (102, 105);
pub const FREE_AT: (i16, i16) = (229, 35);
/// Where numbers end: the free blocks, the number of the slot and the count of slots;
/// the stroke between the last two starts where the first ends.
pub const FREE_ENDS: (i16, i16) = (224, 39);
pub const NUMBER_ENDS: [(i16, i16); 2] = [(28, 36), (50, 36)];
/// A slot's row: the number behind the text of the place ends this far right of the
/// text's start, a digit of the money is this wide, the 0 in front of a clock's number
/// below 10 stands this far left, and the two dots of the clock stand this far right
/// of the hours' end, the upper one this much higher.
pub const PLACE_NUMBER: i16 = 56;
pub const MONEY_DIGIT: i16 = 7;
pub const LEADING: i16 = 8;
pub const DOTS: (i16, i16) = (2, 4);
/// Where the name of a row of the build's above the slots starts in its row: the place
/// of a save's kind of place, a little further left for the longer word.
pub const EXTRA_NAME_AT: (i16, i16) = (252, 59);
/// A message: the screen it is centred on, the lines' height in its size and on
/// screen, and the frame's margins.
pub const MESSAGE_SCREEN: (i16, i16) = (314, 234);
pub const MESSAGE_LINE: (i16, i16) = (12, 13);
/// The busy sign (`0x80042000`): the executable's table of the images of the texts'
/// set it goes through, a word each, how many there are, and where the sign stands.
pub const BUSY_IMAGES: u32 = 0x8007_48EC;
pub const BUSY_STEPS: usize = 14;
pub const BUSY_AT: (i16, i16) = (248, 186);

/// The images of the busy sign, from the executable loaded at `base`.
pub fn busy_images(image: &[u8], base: u32) -> Option<[usize; BUSY_STEPS]> {
    let at = BUSY_IMAGES.checked_sub(base)? as usize;
    let words = image.get(at..)?.get(..4 * BUSY_STEPS)?;
    let mut images = [0; BUSY_STEPS];
    for (image, word) in images.iter_mut().zip(words.chunks_exact(4)) {
        *image = u32::from_le_bytes(word.try_into().ok()?) as usize;
    }
    Some(images)
}

/// The step of the busy sign in a pass, `None` in a pass that does not draw it: the
/// screen draws it, on top of everything, in every pass in which it writes, reads or
/// formats, which are the passes of the three messages of those. `passes` is the main
/// loop's count of passes.
pub fn busy(screen: &CardScreen, passes: u32) -> Option<usize> {
    let works = matches!(
        screen.routine,
        Routine::Saving | Routine::Loading | Routine::Formatting(_)
    );
    works.then_some(passes as usize % BUSY_STEPS)
}

/// The sizes the drawing asks the painter for.
pub trait Sizes {
    /// The width of a sprite of the menus' pool.
    fn sprite(&self, number: usize) -> i16;
    /// The length of a text in the font of the texts.
    fn text(&self, units: &[u16]) -> i16;
}

/// One thing of the screen, as a painter draws it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drawn<'a> {
    /// A frame of the menus in the window colour: mode 0 the plain colour, 1 the
    /// colour that pulses by the time, 2 the marked colour.
    Frame { rect: Rect, mode: u8, time: u32 },
    /// A sprite of the menus' pool, its upper left corner at the place.
    Sprite {
        number: usize,
        at: (i16, i16),
        clip: Option<Rect>,
    },
    /// A number in the small digits of the pool, its last digit ending at the place.
    Digits {
        value: i32,
        ends: (i16, i16),
        clip: Option<Rect>,
    },
    /// A number in the small font, ending at the place.
    Number {
        value: i32,
        ends: (i16, i16),
        clip: Option<Rect>,
    },
    /// A text of the screen's file in a font.
    Text {
        units: &'a [u16],
        font: usize,
        at: (i16, i16),
        clip: Option<Rect>,
    },
    /// The hero's name of a head, in the small font and in capitals.
    Name {
        name: [u8; 12],
        at: (i16, i16),
        clip: Option<Rect>,
    },
    /// The text of the place of a head, in the small font.
    Place {
        text: [u8; 24],
        at: (i16, i16),
        clip: Option<Rect>,
    },
    /// A scroll sign of a list (`crate::menu::sign`).
    Sign {
        turns: u16,
        at: (i16, i16),
        time: u32,
    },
}

/// What is drawn, by the entry of the ordering table it is added to: a lower entry
/// lies nearer, and of one entry what is added later lies under what was added before.
#[derive(Default)]
struct Added<'a> {
    list: Vec<(i32, Drawn<'a>)>,
}

/// The entry of the frames that are no menu's.
const FRAMES: i32 = 14;

impl<'a> Added<'a> {
    fn add(&mut self, entry: i32, drawn: Drawn<'a>) {
        self.list.push((entry, drawn));
    }

    fn frame(&mut self, rect: Rect) {
        let (mode, time) = (0, 0);
        self.add(FRAMES, Drawn::Frame { rect, mode, time });
    }

    fn text(&mut self, entry: i32, units: &'a [u16], at: (i16, i16)) {
        let (font, clip) = (TEXT_FONT, None);
        let text = Drawn::Text {
            units,
            font,
            at,
            clip,
        };
        self.add(entry, text);
    }

    /// From what lies underneath to what lies on top.
    fn painted(mut self) -> Vec<Drawn<'a>> {
        self.list.reverse();
        self.list
            .sort_by_key(|(entry, _)| std::cmp::Reverse(*entry));
        self.list.into_iter().map(|(_, drawn)| drawn).collect()
    }
}

/// A menu through the shared menu drawer: of every piece its content and then its
/// frame in the style of the flag word. `words` are the texts of the lines whose
/// piece is a text.
fn menu<'a>(
    added: &mut Added<'a>,
    menu: &Menu,
    (flags, cursor, time): (u32, i16, u32),
    words: &'a [Vec<u16>],
    sizes: &dyn Sizes,
) {
    for (n, pieces) in menu.lines.iter().enumerate() {
        let (style, pulses) = Style::of_line(flags, usize::try_from(cursor) == Ok(n), 0);
        for part in pieces {
            let room = part.rect.inside(part.margin);
            let clip = Some(part.rect);
            let shows = part.flags & piece::EMPTY == 0;
            match part.kind {
                kind::PICTURE if shows && part.value != u32::MAX => {
                    let number = part.value as usize;
                    let at = placed(part.flags, room, sizes.sprite(number));
                    added.add(menu.slot, Drawn::Sprite { number, at, clip });
                }
                kind::TEXT if shows => {
                    if let Some(units) = words.get(n) {
                        let at = placed(part.flags, room, sizes.text(units));
                        let font = TEXT_FONT;
                        let text = Drawn::Text {
                            units,
                            font,
                            at,
                            clip,
                        };
                        added.add(menu.slot, text);
                    }
                }
                _ => {}
            }
            if part.flags & piece::BARE == 0 {
                let frame = Drawn::Frame {
                    rect: part.rect,
                    mode: style.mode,
                    time: if pulses { time } else { 0 },
                };
                added.add(menu.slot, frame);
            }
        }
    }
}

/// The rows of the slots (`0x800B8EC0`), every piece cut to the rows' area. The game
/// adds a row above the first and one below the last too, which the area cuts away
/// whole; they are left out here. A row of the build's above the slots
/// ([`CardScreen::extra`]) has its name where a save's kind of place stands.
fn rows<'a>(added: &mut Added<'a>, tables: &'a Tables, screen: &CardScreen) {
    const ENTRY: i32 = 1;
    let clip = Some(tables.area);
    let first = usize::try_from(screen.cursors[3].first).unwrap_or(0);
    for row in 0..ROWS {
        let slot = first + row as usize;
        if slot >= screen.rows() {
            continue;
        }
        let low = ROW_STEP * row;
        let lower = |at: (i16, i16)| (at.0, at.1 + low);
        // A row of the build's has its name, filled or not, so that an empty one is
        // not taken for a slot without a save.
        let extra_name = (slot < screen.extra)
            .then(|| EXTRA_NAMES.get(slot))
            .flatten();
        let named = |added: &mut Added<'a>, units: &'a [u16]| {
            let at = lower(EXTRA_NAME_AT);
            let font = TEXT_FONT;
            let text = Drawn::Text {
                units,
                font,
                at,
                clip,
            };
            added.add(ENTRY, text);
        };
        if !screen.is_read(slot) || !screen.filled[slot] {
            let (number, at) = match screen.is_read(slot) {
                true => NO_SAVE,
                false => LOADING,
            };
            let at = lower(at);
            added.add(ENTRY, Drawn::Sprite { number, at, clip });
            if let Some(units) = extra_name {
                named(added, units);
            }
            continue;
        }
        let mut sprite = |number: usize, at: (i16, i16)| {
            let at = lower(at);
            added.add(ENTRY, Drawn::Sprite { number, at, clip });
        };
        for label in &tables.labels {
            sprite(label.number as usize, label.at);
        }
        let head: &[u8; HEAD] = &screen.heads[slot];
        let word =
            |at: usize| i32::from_le_bytes([head[at], head[at + 1], head[at + 2], head[at + 3]]);
        let half = |at: usize| i16::from_le_bytes([head[at], head[at + 1]]);
        let mut digits = |value: i32, ends: (i16, i16)| {
            let ends = lower(ends);
            added.add(ENTRY, Drawn::Digits { value, ends, clip });
        };
        // The level, the HP and the largest HP, the hours and the minutes.
        for n in 0..5 {
            let value = word(40 + 4 * n);
            digits(value, tables.row[n]);
            if n >= 3 && value < 10 {
                digits(0, (tables.row[n].0 - LEADING, tables.row[n].1));
            }
        }
        // The money starts at its place: the drawer is handed where it ends.
        let money = word(60);
        let mut count = 1;
        let mut rest = money / 10;
        while rest != 0 {
            count += 1;
            rest /= 10;
        }
        digits(
            money,
            (tables.row[5].0 + MONEY_DIGIT * count, tables.row[5].1),
        );
        let mut name = [0; 12];
        name.copy_from_slice(&head[..12]);
        let at = lower(tables.row[6]);
        added.add(ENTRY, Drawn::Name { name, at, clip });
        let mut text = [0; 24];
        text.copy_from_slice(&head[12..36]);
        let at = lower(tables.row[7]);
        added.add(ENTRY, Drawn::Place { text, at, clip });
        if half(38) >= 0 {
            let value = i32::from(half(38));
            let ends = (at.0 + PLACE_NUMBER, at.1);
            added.add(ENTRY, Drawn::Number { value, ends, clip });
        }
        let kind = usize::try_from(half(36))
            .ok()
            .and_then(|k| tables.place_kinds.get(k));
        match (extra_name, kind) {
            (Some(units), _) => named(added, units),
            (None, Some(kind)) => {
                let at = lower(kind.at);
                let number = kind.number as usize;
                added.add(ENTRY, Drawn::Sprite { number, at, clip });
            }
            (None, None) => {}
        }
        let at = lower(tables.row[1]);
        let number = STROKE;
        added.add(ENTRY, Drawn::Sprite { number, at, clip });
        let dot = lower((tables.row[3].0 + DOTS.0, tables.row[3].1));
        for at in [(dot.0, dot.1 - DOTS.1), dot] {
            let (units, font) = (&tables.texts[8][..], TEXT_FONT);
            let text = Drawn::Text {
                units,
                font,
                at,
                clip,
            };
            added.add(ENTRY, text);
        }
    }
}

/// What stands round the slots: the scroll signs, the word of the first list's line,
/// the port with the free blocks of a save, and the number of the slot.
fn round_the_slots<'a>(added: &mut Added<'a>, tables: &'a Tables, screen: &CardScreen, time: u32) {
    let (above, below) = screen.cursors[3].hides();
    for (hidden, turns, at) in [
        (above, sign::ABOVE, SIGNS[0]),
        (below, sign::BELOW, SIGNS[1]),
    ] {
        if hidden {
            added.add(1, Drawn::Sign { turns, at, time });
        }
    }
    let clip = None;
    if let Some(heading) = usize::try_from(screen.line())
        .ok()
        .and_then(|n| tables.headings.get(n))
    {
        let (number, at) = (heading.number as usize, heading.at);
        added.add(i32::from(heading.slot), Drawn::Sprite { number, at, clip });
    }
    added.frame(tables.frames[5]);
    // The screen for a PC names no card and counts no blocks.
    let of_a_card = screen.shape == Shape::Original;
    if of_a_card {
        let (number, at) = CARD;
        added.add(1, Drawn::Sprite { number, at, clip });
        let number = PORT_DIGITS.0[usize::from(screen.port() == 1)];
        let at = PORT_DIGITS.1;
        added.add(1, Drawn::Sprite { number, at, clip });
        added.frame(tables.frames[6]);
    }
    if of_a_card && screen.line() == line::SAVE {
        added.text(0, &tables.texts[6], FREE_AT);
        let (value, ends) = (screen.free, FREE_ENDS);
        added.add(0, Drawn::Digits { value, ends, clip });
        added.frame(tables.frames[7]);
    }
    // The number of the slot of the fifteen; a row of the build's above them has none.
    let number = |value: i32, ends: (i16, i16)| Drawn::Number { value, ends, clip };
    if screen.slot() < SLOTS {
        added.add(5, number(screen.slot() as i32 + 1, NUMBER_ENDS[0]));
        let (units, font, at) = (&tables.texts[7][..], SMALL_FONT, NUMBER_ENDS[0]);
        let stroke = Drawn::Text {
            units,
            font,
            at,
            clip,
        };
        added.add(5, stroke);
        added.add(5, number(SLOTS as i32, NUMBER_ENDS[1]));
    }
    added.frame(tables.frames[8]);
}

/// The line of help (`0x800B8994`): its frame, and the text when the state has one.
fn help<'a>(added: &mut Added<'a>, tables: &'a Tables, text: Option<&'a [u16]>) {
    if let Some(units) = text {
        added.text(5, units, HELP_AT);
    }
    added.frame(tables.frames[4]);
}

/// The two ports with what stands on them (`0x800B8514`, `0x800B8694`): while the
/// player picks, the frame under the cursor pulses; else the port in use is marked.
fn ports<'a>(
    added: &mut Added<'a>,
    tables: &'a Tables,
    screen: &CardScreen,
    time: u32,
    sizes: &dyn Sizes,
) {
    for (port, record) in tables.ports.iter().enumerate() {
        let on = usize::from(screen.port()) == port;
        let how = match (screen.state == state::PORT, on) {
            (true, true) => (2, 0, time),
            (true, false) => (2, 0, 0),
            (false, true) => (4, 0, 0),
            (false, false) => (1, 0, 0),
        };
        menu(added, record, how, &[], sizes);
    }
    for sprite in &tables.on_ports {
        let (number, at, clip) = (sprite.number as usize, sprite.at, None);
        added.add(i32::from(sprite.slot), Drawn::Sprite { number, at, clip });
    }
    added.frame(tables.frames[3]);
}

/// For a pointer of the build (the game has none): the cursor record the state moves,
/// and where its places lie, each with a rectangle that shows it. A port is the whole
/// of its menu. None while a message is up or the screen does not take the pad.
pub fn spots(tables: &Tables, screen: &CardScreen) -> Option<(usize, Vec<(i16, Rect)>)> {
    if screen.routine != Routine::States || screen.message().is_some() {
        return None;
    }
    let around =
        |pieces: &[crate::menu::Piece]| crate::menu::bounds(pieces.iter().map(|piece| piece.rect));
    let lines = |menu: &Menu| -> Vec<(i16, Rect)> {
        (menu.lines.iter().zip(0..))
            .filter_map(|(line, at)| around(line).map(|rect| (at, rect)))
            .collect()
    };
    Some(match screen.state {
        state::LIST => (0, lines(&tables.list)),
        state::PORT => {
            let ports = (tables.ports.iter().zip(0..)).filter_map(|(menu, port)| {
                crate::menu::bounds(menu.lines.iter().filter_map(|line| around(line)))
                    .map(|rect| (port, rect))
            });
            (1, ports.collect())
        }
        state::FORMAT => (2, lines(&tables.answers[0])),
        state::SLOTS => {
            // Only the rows that show a slot.
            let first = screen.cursors[3].first;
            let rows = lines(&tables.slots).into_iter();
            (
                3,
                rows.filter(|(at, _)| {
                    usize::try_from(first + at).is_ok_and(|row| row < screen.rows())
                })
                .collect(),
            )
        }
        state::ASK | state::DELETE => (4, lines(&tables.answers[1])),
        state::TITLE => (5, lines(&tables.answers[2])),
        _ => return None,
    })
}

/// A message (`0x800BAFA8`): its lines about the middle of the screen, in a frame.
pub fn message<'a>(lines: &'a [Vec<u16>], sizes: &dyn Sizes) -> Vec<Drawn<'a>> {
    let wide = lines.iter().map(|line| sizes.text(line)).max().unwrap_or(0);
    let high = MESSAGE_LINE.0 * lines.len() as i16;
    let (x, y) = ((MESSAGE_SCREEN.0 - wide) / 2, (MESSAGE_SCREEN.1 - high) / 2);
    let rect = Rect {
        x,
        y,
        w: wide + 6,
        h: high + if high < 16 { 3 } else { 6 },
    };
    let mut added = Added::default();
    for (n, line) in lines.iter().enumerate() {
        added.text(5, line, (x + 3, y + 2 + MESSAGE_LINE.1 * n as i16));
    }
    added.frame(rect);
    added.painted()
}

/// What the screen draws in a pass, from what lies underneath to what lies on top.
/// `time` is the time of the menus.
pub fn drawn<'a>(
    tables: &'a Tables,
    screen: &CardScreen,
    time: u32,
    sizes: &dyn Sizes,
) -> Vec<Drawn<'a>> {
    if screen.routine == Routine::Ending {
        return Vec::new();
    }
    if let Some(number) = screen.message() {
        let lines = tables.messages.get(number);
        return lines.map_or_else(Vec::new, |lines| message(lines, sizes));
    }
    let mut added = Added::default();
    let cursor = |state: usize| screen.cursors[state].at;
    let slots = |added: &mut Added<'a>, how: (u32, i16, u32)| {
        menu(added, &tables.slots, how, &[], sizes);
        rows(added, tables, screen);
        round_the_slots(added, tables, screen, time);
    };
    match screen.state {
        state::LIST => {
            menu(&mut added, &tables.list, (2, cursor(0), time), &[], sizes);
            help(&mut added, tables, None);
        }
        state::PORT | state::NO_CARD | state::UNFORMATTED | state::CHECK => {
            ports(&mut added, tables, screen, time, sizes);
            let text = match screen.state {
                state::PORT => 0,
                state::NO_CARD => 1,
                state::UNFORMATTED => 4,
                _ => 5,
            };
            help(&mut added, tables, Some(&tables.help[text]));
        }
        state::FORMAT => {
            for (text, at) in tables.texts[4..6].iter().zip(FORMAT_AT) {
                added.text(5, text, at);
            }
            added.frame(tables.frames[2]);
            let how = (2, cursor(2), time);
            menu(
                &mut added,
                &tables.answers[0],
                how,
                &tables.answers_words,
                sizes,
            );
        }
        state::SLOTS => {
            slots(&mut added, (2, cursor(3), time));
            let text = usize::try_from(screen.line()).ok();
            let text = text.and_then(|line| tables.help_of_slots.get(line));
            help(&mut added, tables, text.map(Vec::as_slice));
        }
        state::NO_ROOM | state::DAMAGED => {
            slots(&mut added, (1, cursor(3), 0));
            let text = if screen.state == state::NO_ROOM { 2 } else { 3 };
            help(&mut added, tables, Some(&tables.help[text]));
        }
        state::ASK => {
            slots(&mut added, (4, cursor(3), 0));
            let how = (2, cursor(4), time);
            menu(
                &mut added,
                &tables.answers[1],
                how,
                &tables.answers_words,
                sizes,
            );
            let filled = screen.filled.get(screen.row()).copied().unwrap_or(false);
            let text = match (screen.line(), filled) {
                (line::LOAD, _) => Some(0),
                (line::SAVE, true) => Some(1),
                (line::SAVE, false) => Some(2),
                _ => None,
            };
            if let Some(text) = text {
                added.text(5, &tables.texts[text], ASKED_AT);
            }
            added.frame(tables.frames[0]);
        }
        state::DELETE => {
            // The build's question, laid out as the game's question in front of a save.
            slots(&mut added, (4, cursor(3), 0));
            let how = (2, cursor(4), time);
            menu(
                &mut added,
                &tables.answers[1],
                how,
                &tables.answers_words,
                sizes,
            );
            added.text(5, &ASKS_DELETE, ASKED_AT);
            added.frame(tables.frames[0]);
        }
        state::TITLE => {
            added.text(5, &tables.texts[3], TITLE_AT);
            added.frame(tables.frames[1]);
            let how = (2, cursor(5), time);
            menu(
                &mut added,
                &tables.answers[2],
                how,
                &tables.answers_words,
                sizes,
            );
        }
        _ => {}
    }
    added.painted()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card_screen::{Kind, Sprite};
    use crate::menu::{Cursor, Piece, cursor};

    /// Every sprite 20 wide, every letter 8.
    struct Even;

    impl Sizes for Even {
        fn sprite(&self, _number: usize) -> i16 {
            20
        }
        fn text(&self, units: &[u16]) -> i16 {
            8 * units.len() as i16
        }
    }

    fn rect(x: i16, y: i16, w: i16, h: i16) -> Rect {
        Rect { x, y, w, h }
    }

    /// A menu of lines of one piece each.
    fn lines(slot: i32, pieces: &[(u16, u16, u32, Rect, (i16, i16))]) -> Menu {
        let line = |&(flags, kind, value, rect, margin)| {
            vec![Piece {
                flags,
                kind,
                value,
                rect,
                margin,
            }]
        };
        Menu {
            flags: 2,
            openness: 8,
            cursor: 0,
            slot,
            lines: pieces.iter().map(line).collect(),
        }
    }

    fn sprite(x: i16, y: i16, number: i16) -> Sprite {
        Sprite {
            at: (x, y),
            number,
            slot: 1,
        }
    }

    /// Tables in the shape of the file's, with the document's places.
    fn tables() -> Tables {
        let record = |columns, rows, lines, flags, at| Cursor {
            columns,
            rows,
            lines,
            flags,
            at,
            first: 0,
        };
        let empty = |x, y, w, h| (piece::EMPTY, 0, 0, rect(x, y, w, h), (0, 0));
        let word = |n, y| {
            (
                piece::CENTRED,
                kind::PICTURE,
                n,
                rect(106, y, 108, 19),
                (0, 4),
            )
        };
        let answers = |slot, a: Rect, b: Rect| {
            let text = |rect| (piece::CENTRED, kind::TEXT, 0, rect, (2, 2));
            lines(slot, &[text(a), text(b)])
        };
        let text = |letters: usize| vec![0x8260; letters];
        Tables {
            cursors: [
                record(1, 4, 4, 0, 0),
                record(2, 1, 2, 0, 0),
                record(1, 2, 2, 0, 1),
                record(1, 4, 15, cursor::SCROLLS, 0),
                record(2, 1, 2, cursor::WRAPS_ACROSS, 0),
                record(2, 1, 2, 0, 1),
            ],
            slots: lines(
                5,
                &[
                    empty(10, 49, 300, 31),
                    empty(10, 82, 300, 31),
                    empty(10, 115, 300, 31),
                    empty(10, 148, 300, 31),
                ],
            ),
            ports: [
                lines(5, &[empty(33, 90, 105, 36)]),
                lines(5, &[empty(182, 90, 105, 36)]),
            ],
            list: lines(
                5,
                &[word(190, 69), word(197, 90), word(198, 111), word(196, 132)],
            ),
            answers: [
                answers(0, rect(126, 106, 56, 18), rect(126, 126, 56, 18)),
                answers(0, rect(215, 193, 46, 19), rect(261, 193, 48, 19)),
                answers(0, rect(106, 121, 52, 19), rect(163, 121, 52, 19)),
            ],
            answers_words: [text(3), text(3)],
            headings: [sprite(80, 21, 190), sprite(69, 21, 197)],
            on_ports: vec![sprite(108, 21, 39), sprite(39, 95, 39)],
            place_kinds: [
                sprite(272, 52, 78),
                sprite(272, 52, 52),
                sprite(272, 52, 58),
            ],
            row: [
                (170, 54),
                (144, 68),
                (172, 68),
                (222, 68),
                (244, 68),
                (225, 54),
                (39, 55),
                (39, 68),
                (290, 52),
            ],
            labels: vec![sprite(13, 52, 119), sprite(13, 66, 150)],
            area: rect(10, 49, 300, 129),
            frames: [
                rect(10, 193, 204, 20),
                rect(92, 100, 140, 19),
                rect(16, 70, 288, 34),
                rect(93, 18, 124, 18),
                rect(10, 193, 300, 20),
                rect(63, 18, 108, 18),
                rect(187, 18, 122, 18),
                rect(187, 36, 122, 12),
                rect(10, 33, 44, 14),
            ],
            texts: std::array::from_fn(|n| text(n + 1)),
            help: std::array::from_fn(|n| text(n + 10)),
            help_of_slots: [text(20), text(21)],
            legend: Vec::new(),
            messages: vec![vec![text(10)], vec![text(10), text(20)]],
        }
    }

    /// The screen of a room on the slots, with every head in and slot 1 filled.
    fn on_slots(tables: &Tables) -> CardScreen {
        let mut screen = CardScreen::open(Kind::Room, tables.cursors);
        screen.state = state::SLOTS;
        screen.count = 16;
        screen.free = 14;
        screen.filled[1] = true;
        let head = &mut screen.heads[1];
        head[0] = 0x4C;
        head[36..38].copy_from_slice(&0i16.to_le_bytes());
        head[38..40].copy_from_slice(&(-2i16).to_le_bytes());
        for (n, value) in [12, 95, 120, 3, 47, 1250].into_iter().enumerate() {
            head[40 + 4 * n..44 + 4 * n].copy_from_slice(&i32::to_le_bytes(value));
        }
        screen
    }

    #[test]
    fn the_busy_sign_is_drawn_while_the_card_works_and_goes_through_its_table() {
        let tables = tables();
        let mut screen = CardScreen::open(Kind::Room, tables.cursors);
        assert_eq!(busy(&screen, 5), None);
        for routine in [
            Routine::Saving,
            Routine::Loading,
            Routine::Formatting(0),
            Routine::Formatting(2),
        ] {
            screen.routine = routine;
            assert_eq!(busy(&screen, 5), Some(5));
            assert_eq!(busy(&screen, 14), Some(0));
            assert_eq!(busy(&screen, 27), Some(13));
        }
        screen.routine = Routine::Ending;
        assert_eq!(busy(&screen, 5), None);

        // The table is fourteen words of the executable.
        let base = BUSY_IMAGES - 8;
        let mut image = vec![0xEE; 8];
        for word in 0..BUSY_STEPS as u32 {
            image.extend((100 + word).to_le_bytes());
        }
        let read = busy_images(&image, base).expect("the table");
        assert_eq!((read[0], read[13]), (100, 113));
        assert_eq!(busy_images(&image[..image.len() - 1], base), None);
        assert_eq!(busy_images(&image, BUSY_IMAGES + 4), None);
    }

    #[test]
    fn the_first_list_is_four_words_in_frames_and_the_cursors_pulses() {
        let tables = tables();
        let mut screen = CardScreen::open(Kind::Room, tables.cursors);
        screen.cursors[0].at = 2;
        let drawn = drawn(&tables, &screen, 9, &Even);
        // The empty line of help lies underneath, then of every line its frame and
        // its word, the last line first.
        assert_eq!(drawn.len(), 9);
        assert_eq!(
            drawn[0],
            Drawn::Frame {
                rect: tables.frames[4],
                mode: 0,
                time: 0
            }
        );
        let frame = |y, time| Drawn::Frame {
            rect: rect(106, y, 108, 19),
            mode: 1,
            time,
        };
        assert_eq!(
            (drawn[1], drawn[3], drawn[7]),
            (frame(132, 0), frame(111, 9), frame(69, 0))
        );
        // A word stands in the middle of its room, 4 below the frame's edge.
        assert_eq!(
            drawn[8],
            Drawn::Sprite {
                number: 190,
                at: (106 + 44, 73),
                clip: Some(rect(106, 69, 108, 19))
            }
        );
    }

    #[test]
    fn a_row_shows_its_head_or_that_there_is_none_or_that_it_is_read() {
        let tables = tables();
        let mut screen = on_slots(&tables);
        screen.count = 4;
        let drawn = drawn(&tables, &screen, 0, &Even);
        let clip = Some(tables.area);
        let has = |wanted: Drawn| drawn.contains(&wanted);
        let sprite = |number, at| Drawn::Sprite { number, at, clip };
        // Slot 0 is empty, the slots 3 and on are not read yet.
        assert!(has(sprite(NO_SAVE.0, (139, 57))));
        assert!(has(sprite(LOADING.0, (125, 57 + 99))));
        assert!(!has(sprite(LOADING.0, (125, 57 + 66))));
        // Slot 1, a row lower: the labels, the numbers, the kind of the place.
        assert!(has(sprite(119, (13, 85))) && has(sprite(78, (272, 85))));
        assert!(has(sprite(STROKE, (144, 101))));
        let digits = |value, ends| Drawn::Digits { value, ends, clip };
        assert!(has(digits(12, (170, 87))) && has(digits(95, (144, 101))));
        assert!(has(digits(120, (172, 101))));
        // The hours below 10 get a 0 in front, the minutes do not.
        assert!(has(digits(3, (222, 101))) && has(digits(0, (214, 101))));
        assert!(has(digits(47, (244, 101))) && !has(digits(0, (236, 101))));
        // Four digits of money end 28 right of its place.
        assert!(has(digits(1250, (253, 87))));
        let mut name = [0; 12];
        name[0] = 0x4C;
        assert!(has(Drawn::Name {
            name,
            at: (39, 88),
            clip
        }));
        // The place has no number behind it: its half word is below 0.
        assert!(
            !drawn
                .iter()
                .any(|d| matches!(d, Drawn::Number { clip: Some(_), .. }))
        );
        let dots: Vec<_> = (drawn.iter())
            .filter_map(|d| match d {
                Drawn::Text {
                    at, clip: Some(_), ..
                } => Some(*at),
                _ => None,
            })
            .collect();
        assert_eq!(dots, [(224, 101), (224, 97)]);
        // Round the rows: the sign below alone, "save" with the free blocks, slot 1 of 15.
        let signs: Vec<_> = drawn
            .iter()
            .filter(|d| matches!(d, Drawn::Sign { .. }))
            .collect();
        assert_eq!(
            signs,
            [&Drawn::Sign {
                turns: sign::BELOW,
                at: SIGNS[1],
                time: 0
            }]
        );
        let free = Drawn::Digits {
            value: 14,
            ends: FREE_ENDS,
            clip: None,
        };
        assert!(
            has(free)
                && has(Drawn::Sprite {
                    number: 190,
                    at: (80, 21),
                    clip: None
                })
        );
        let number = |value, ends| Drawn::Number {
            value,
            ends,
            clip: None,
        };
        assert!(has(number(1, (28, 36))) && has(number(15, (50, 36))));
        // A load shows no free blocks, and the second port its digit.
        screen.cursors[0].at = line::LOAD;
        screen.cursors[1].at = 1;
        let drawn = super::drawn(&tables, &screen, 0, &Even);
        assert!(!drawn.contains(&free));
        let digit = Drawn::Sprite {
            number: 2,
            at: (292, 21),
            clip: None,
        };
        assert!(drawn.contains(&digit));
        assert!(drawn.contains(&Drawn::Sprite {
            number: 197,
            at: (69, 21),
            clip: None
        }));
        // The screen for a PC keeps the heading and the number of the slot, and has
        // neither the card with its digit nor the free blocks.
        screen.shape = Shape::Plain;
        screen.cursors[0].at = line::SAVE;
        let plain = super::drawn(&tables, &screen, 0, &Even);
        assert!(!plain.contains(&free) && !plain.contains(&digit));
        let card = Drawn::Sprite {
            number: CARD.0,
            at: CARD.1,
            clip: None,
        };
        assert!(!plain.contains(&card) && drawn.contains(&card));
        assert!(plain.contains(&Drawn::Sprite {
            number: 190,
            at: (80, 21),
            clip: None
        }));
        assert!(plain.contains(&number(1, (28, 36))) && plain.contains(&number(15, (50, 36))));
    }

    #[test]
    fn the_question_names_the_deed_and_marks_the_slot() {
        let tables = tables();
        let mut screen = on_slots(&tables);
        screen.state = state::ASK;
        screen.cursors[3].at = 1;
        screen.cursors[4].at = 1;
        let asked = |screen: &CardScreen| {
            let drawn = drawn(&tables, screen, 5, &Even);
            let text = drawn.iter().find_map(|d| match d {
                Drawn::Text { units, at, .. } if *at == ASKED_AT => Some(units.len()),
                _ => None,
            });
            (drawn, text)
        };
        // A save over a file, to a free slot, and a load: the texts 1, 2 and 0.
        let (drawn, text) = asked(&screen);
        assert_eq!(text, Some(2));
        // The slot's frame has the marked look and does not pulse; the answer's does.
        let frame = |rect, mode, time| Drawn::Frame { rect, mode, time };
        assert!(drawn.contains(&frame(rect(10, 82, 300, 31), 2, 0)));
        assert!(drawn.contains(&frame(rect(10, 49, 300, 31), 0, 0)));
        assert!(drawn.contains(&frame(rect(261, 193, 48, 19), 1, 5)));
        assert!(drawn.contains(&frame(rect(215, 193, 46, 19), 1, 0)));
        assert!(drawn.contains(&frame(tables.frames[0], 0, 0)));
        // An answer of three letters in the middle of a room of 42 by 15.
        let yes = Drawn::Text {
            units: &tables.answers_words[0],
            font: TEXT_FONT,
            at: (217 + 9, 195),
            clip: Some(rect(215, 193, 46, 19)),
        };
        assert!(drawn.contains(&yes));
        screen.cursors[3].at = 0;
        assert_eq!(asked(&screen).1, Some(3));
        screen.cursors[0].at = line::LOAD;
        assert_eq!(asked(&screen).1, Some(1));
    }

    #[test]
    fn a_message_stands_about_the_middle_in_a_frame_of_its_size() {
        let tables = tables();
        // One line of ten letters: 80 by 12, the frame 3 higher.
        let one = message(&tables.messages[0], &Even);
        assert_eq!(
            one[0],
            Drawn::Frame {
                rect: rect(117, 111, 86, 15),
                mode: 0,
                time: 0
            }
        );
        assert!(matches!(one[1], Drawn::Text { at: (120, 113), .. }));
        // Two lines, the wider one of twenty letters: 160 by 24, the frame 6 higher,
        // the lines 13 apart.
        let two = message(&tables.messages[1], &Even);
        let frame = rect(77, 105, 166, 30);
        assert_eq!(
            two[0],
            Drawn::Frame {
                rect: frame,
                mode: 0,
                time: 0
            }
        );
        let at: Vec<_> = (two.iter())
            .filter_map(|d| match d {
                Drawn::Text { at, .. } => Some(*at),
                _ => None,
            })
            .collect();
        assert_eq!(at, [(80, 120), (80, 107)]);
        // A deed that takes passes shows its message and nothing else.
        let mut screen = on_slots(&tables);
        screen.routine = Routine::Loading;
        assert!(drawn(&tables, &screen, 0, &Even).is_empty());
        screen.routine = Routine::Formatting(0);
        assert_eq!(drawn(&tables, &screen, 0, &Even), two);
        screen.routine = Routine::Ending;
        assert!(drawn(&tables, &screen, 0, &Even).is_empty());
    }

    #[test]
    fn the_port_under_the_cursor_pulses_and_the_port_in_use_is_marked() {
        let tables = tables();
        let mut screen = CardScreen::open(Kind::Continue, tables.cursors);
        screen.cursors[1].at = 1;
        let frames = |screen: &CardScreen| -> Vec<(i16, u8, u32)> {
            (drawn(&tables, screen, 7, &Even).iter())
                .filter_map(|d| match d {
                    Drawn::Frame { rect, mode, time } if rect.h == 36 => {
                        Some((rect.x, *mode, *time))
                    }
                    _ => None,
                })
                .collect()
        };
        assert_eq!(frames(&screen), [(182, 1, 7), (33, 1, 0)]);
        screen.state = state::CHECK;
        assert_eq!(frames(&screen), [(182, 2, 0), (33, 0, 0)]);
        // The line of help is the state's: the texts 0, 5 and 1 of the six.
        let help = |screen: &CardScreen| {
            let drawn = drawn(&tables, screen, 0, &Even);
            drawn.iter().find_map(|d| match d {
                Drawn::Text { units, at, .. } if *at == HELP_AT => Some(units.len()),
                _ => None,
            })
        };
        assert_eq!(help(&screen), Some(15));
        screen.state = state::PORT;
        assert_eq!(help(&screen), Some(10));
        screen.state = state::NO_CARD;
        assert_eq!(help(&screen), Some(11));
    }
}
