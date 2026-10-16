
use crate::equip_screen::{EquipScreen, KIND, LIST, LIST_ROWS};
use crate::items::FREE;
use crate::menu::{Piece, Rect, Style, piece, sign};
use crate::player_menu::OPEN;
use crate::player_panel::{Drawn, detail, five_numbers, four_lines, message, text_at, worn_rows};

/// The list of the three lines the list is drawn with, in the screen's file. The
/// drawer writes the places of their pieces in every pass.
pub const LINES: u32 = 0x800C_5B74;
pub const ROWS: usize = LIST_ROWS as usize;
/// Where the three pieces of a row start (the picture, the name, the count), the first
/// row's y and the step of a row.
pub const PIECES: [i16; 3] = [83, 104, 216];
pub const FIRST_ROW: i16 = 137;
pub const STEP: i16 = 22;
/// The scroll signs above and below the list (the places at `0x800C5A1C`).
pub const SIGNS: [(i16, i16); 2] = [(160, 131), (159, 208)];
/// Where the first of the three lines of what he has on stands.
pub const WORN_AT: (i16, i16) = (10, 61);
/// The box of the five numbers and the box of what they would become (the rectangles
/// at `0x800C5B90` and `0x800C5B88`).
pub const NUMBERS: Rect = Rect {
    x: 194,
    y: 49,
    w: 75,
    h: 77,
};
pub const WOULD: Rect = Rect {
    x: 281,
    y: 49,
    w: 29,
    h: 77,
};
/// Where the numbers of what would be end, the first row's y and the step of a row
/// (`0x8002E1E4` with the place (281, 52) and the step 14).
pub const WOULD_ENDS: (i16, i16) = (303, 54);
pub const WOULD_STEP: i16 = 14;
/// The sign between the two columns.
pub const ARROW: (i16, i16) = (274, 87);
/// The box of the description of the list's thing: the box of a message with its
/// corner at the place of `0x800C5B84`.
pub const DESCRIBED: Rect = Rect {
    x: 10,
    y: 61,
    ..message::BOX
};
/// How many weapons, armours and third things have a description, and the tables of
/// them in the screen's file: the addresses of four lines for each thing.
pub const THINGS: [usize; 3] = [23, 15, 17];
pub const TEXTS: [u32; 3] = [0x800C_5BC8, 0x800C_5D38, 0x800C_5E28];
/// The text of the message: that the thing cannot be taken off.
pub const REFUSAL: u32 = 0x800C_4970;
/// The executable's list of the line of help, the same in every step.
pub const HELP: u32 = 0x8008_25D4;

/// The entries of the executable's colours of a number (`0x8008261C`) the numbers of
/// what would be are drawn under: one that stays, one that would rise, one that would
/// fall.
pub mod colour {
    pub const STAYS: usize = 0;
    pub const RISES: usize = 1;
    pub const FALLS: usize = 2;
}

/// A place of the list of a kind as a row shows it: the thing, [`FREE`] for none, and
/// how many of it lie there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GearRow {
    pub thing: u8,
    pub count: u8,
}

/// What the drawing takes from the game.
#[derive(Debug, Clone, Copy)]
pub struct Shown<'a> {
    /// The lines of what he has on (`crate::player_panel::detail::WORN_LINES`) and the
    /// lines of the list ([`LINES`]).
    pub worn_lines: &'a [Vec<Piece>],
    pub list_lines: &'a [Vec<Piece>],
    /// What he has on: the weapon, the armour and the third thing, [`FREE`] for none.
    pub worn: [u8; 3],
    /// The first five of the six values a fight counts with, as they are, and as they
    /// would be with the thing under the list's cursor on.
    pub values: [u8; 5],
    pub would: [u8; 5],
    /// The places of the list of the cursor's kind from its kept first line on, and
    /// the last place of that list in use.
    pub rows: [GearRow; ROWS],
    pub last_place: i16,
}

/// The description of a thing in a box (`0x800C5628` through `0x8002AF70`): the box of
/// a message with its four lines. A thing past its kind's table has no box.
pub fn description(kind: i16, thing: u8, whole: Rect) -> Vec<Drawn> {
    let Some(kind) = usize::try_from(kind).ok().filter(|kind| *kind < 3) else {
        return Vec::new();
    };
    if usize::from(thing) >= THINGS[kind] {
        return Vec::new();
    }
    let mut drawn = vec![Drawn::Frame {
        rect: whole,
        mode: 0,
        time: 0,
    }];
    drawn.extend((0..message::LINES).map(|line| Drawn::GearDescribed {
        kind,
        thing,
        line,
        at: (
            whole.x + message::TEXT.0,
            whole.y + message::TEXT.1 + message::LINE * line as i16,
        ),
    }));
    drawn
}

/// Panel 1 (`0x800C0FFC`, `0x800C54FC`), from what lies underneath to what lies on
/// top: the box of the five numbers; with a thing under the list's cursor the box of
/// what they would become, the sign between the two and the new numbers, each under
/// the colour of its change; then the three lines of what he has on, the cursor's
/// line pulsing while a kind is picked and in the marked colour behind the list. The
/// description of the list's thing stands in place of all of it.
fn worn_panel(
    screen: &EquipScreen,
    shown: &Shown,
    time: u32,
    width: &impl Fn(usize) -> i16,
) -> Vec<Drawn> {
    if screen.describes_thing {
        return description(screen.kind, screen.thing, DESCRIBED);
    }
    let whole = Rect {
        x: 0,
        y: 0,
        w: 320,
        h: 240,
    };
    let mut drawn = five_numbers(NUMBERS, shown.values, OPEN.into());
    if screen.step == LIST && screen.thing != FREE {
        drawn.push(Drawn::Frame {
            rect: WOULD,
            mode: 0,
            time: 0,
        });
        drawn.push(Drawn::Sign {
            turns: 0,
            at: ARROW,
            time,
        });
        for (row, (now, then)) in (0i16..).zip(shown.values.into_iter().zip(shown.would)) {
            drawn.push(Drawn::Tinted {
                value: then.into(),
                ends: (WOULD_ENDS.0, WOULD_ENDS.1 + WOULD_STEP * row),
                colour: match then.cmp(&now) {
                    std::cmp::Ordering::Greater => colour::RISES,
                    std::cmp::Ordering::Less => colour::FALLS,
                    std::cmp::Ordering::Equal => colour::STAYS,
                },
                clip: whole,
            });
        }
    }
    let flags = if screen.step == KIND { 0x02 } else { 0x04 };
    let cursor = usize::try_from(screen.kind).ok();
    drawn.extend(worn_rows(
        WORN_AT,
        shown.worn_lines,
        shown.worn.map(|thing| thing != FREE),
        (flags, cursor, time),
        OPEN.into(),
        width,
    ));
    drawn
}

/// The list of a kind (`0x800C56F8`), from what lies underneath to what lies on top:
/// the two scroll signs, then three rows through the shared menu drawer, each a
/// picture, a name and a count in a frame of their own. On the list the frames under
/// the cursor pulse, else all are plain. A place without a thing has its frames alone,
/// and no row is dim.
fn list(screen: &EquipScreen, shown: &Shown, time: u32) -> Vec<Drawn> {
    let flags = if screen.step == LIST { 0x02 } else { 0x01 };
    let (cursor, first) = screen.list_place(screen.kind);
    let kind = usize::try_from(screen.kind).unwrap_or(0);
    let mut drawn = Vec::new();
    // The signs: one line for every place up to the last in use, three at least. The
    // empty line behind the last third thing is not counted.
    let count = (i32::from(shown.last_place) + 1).max(ROWS as i32);
    if i32::from(first) < count - ROWS as i32 && count > ROWS as i32 {
        drawn.push(Drawn::Sign {
            turns: sign::BELOW,
            at: SIGNS[1],
            time,
        });
    }
    if first != 0 {
        drawn.push(Drawn::Sign {
            turns: sign::ABOVE,
            at: SIGNS[0],
            time,
        });
    }
    for (n, (pieces, row)) in shown.list_lines.iter().zip(shown.rows).enumerate() {
        let (style, pulses) = Style::of_line(flags, i32::from(cursor) == n as i32, 0);
        let y = FIRST_ROW + STEP * n as i16;
        for (k, (piece, x)) in pieces.iter().zip(PIECES).enumerate() {
            let clip = Rect { x, y, ..piece.rect };
            if piece.flags & piece::BARE == 0 {
                drawn.push(Drawn::Frame {
                    rect: clip,
                    mode: style.mode,
                    time: if pulses { time } else { 0 },
                });
            }
            if row.count == 0 || row.thing == FREE {
                continue;
            }
            let room = clip.inside(piece.margin);
            let thing = row.thing;
            drawn.push(match k {
                0 => Drawn::GearPicture {
                    kind,
                    thing,
                    at: (room.x, room.y),
                    dim: false,
                    clip,
                },
                1 => Drawn::GearName {
                    kind,
                    thing,
                    at: (room.x, room.y),
                    dim: false,
                    clip,
                },
                // A number ends 3 left of its room's right edge, 2 below its upper one.
                _ => Drawn::Count {
                    value: row.count.into(),
                    ends: (room.x + room.w - 3, room.y + 2),
                    dim: false,
                    clip,
                },
            });
        }
    }
    drawn
}

/// A place of the screen's cursor as a pointer of the build names it (the game has
/// none): the step and the row of the cursor.
pub type Spot = (i16, i16);

/// The spot the cursor is on.
pub fn here(screen: &EquipScreen) -> Spot {
    (screen.step, screen.cursor.at)
}

/// The directions that bring the cursor a step nearer to a spot of its step.
pub fn way(screen: &EquipScreen, (step, at): Spot) -> u16 {
    match step == screen.step {
        true => screen.cursor.way_to(at),
        false => 0,
    }
}

/// Where the spots of the step lie, each with a rectangle that shows it, for a pointer
/// of the build: the three rows of what he has on while a kind is picked, and the rows
/// of the list that hold a line. None under the message, nor behind the list, where
/// the putting on has a row alone. `worn_lines` and `list_lines` are as in [`Shown`].
pub fn spots(
    screen: &EquipScreen,
    worn_lines: &[Vec<Piece>],
    list_lines: &[Vec<Piece>],
) -> Vec<(Spot, Rect)> {
    if screen.message || !screen.up {
        return Vec::new();
    }
    let step = screen.step;
    let on = |rows: Vec<(i16, Rect)>| -> Vec<(Spot, Rect)> {
        rows.into_iter()
            .map(|(at, rect)| ((step, at), rect))
            .collect()
    };
    match step {
        KIND => {
            let starts = detail::WORN_PIECES.map(|x| WORN_AT.0 + x);
            let rows =
                crate::player_panel::row_rects(worn_lines, &starts, WORN_AT.1, detail::WORN_STEP);
            on(rows.into_iter().take(detail::WORN.len()).collect())
        }
        LIST => {
            let cursor = screen.cursor;
            let shown = i32::from(cursor.lines) - i32::from(cursor.first);
            let rows = crate::player_panel::row_rects(list_lines, &PIECES, FIRST_ROW, STEP);
            on(rows
                .into_iter()
                .filter(|(at, _)| i32::from(*at) < shown)
                .collect())
        }
        _ => Vec::new(),
    }
}

/// What the screen draws in a pass (`0x800C4C7C`, and `0x800C52D8` while the message
/// is up), from what lies underneath to what lies on top: the message, added last;
/// panel 1; panel 0, which is the list, or the description of what is on in the
/// cursor's place while the circle is held on the kinds, or nothing under the message.
/// The line of help ([`HELP`]) is the caller's. `time` is the menus' time and `width`
/// gives the width of a sprite of the pool.
pub fn drawn(
    screen: &EquipScreen,
    shown: &Shown,
    time: u32,
    width: impl Fn(usize) -> i16,
) -> Vec<Drawn> {
    let mut drawn = Vec::new();
    if screen.message {
        drawn.extend([
            Drawn::Frame {
                rect: message::BOX,
                mode: 0,
                time: 0,
            },
            Drawn::GearRefused {
                at: (
                    message::BOX.x + message::TEXT.0,
                    message::BOX.y + message::TEXT.1,
                ),
            },
        ]);
    }
    drawn.extend(worn_panel(screen, shown, time, &width));
    if screen.describes_worn {
        let on = usize::try_from(screen.kind).ok();
        let on = on.and_then(|kind| shown.worn.get(kind)).copied();
        drawn.extend(description(screen.kind, on.unwrap_or(FREE), message::BOX));
    } else if !screen.message {
        drawn.extend(list(screen, shown, time));
    }
    drawn
}

/// The four lines of a thing's description from the screen's file, whose first byte is
/// at `base`; a line without an address is empty.
pub fn description_text(
    file: &[u8],
    base: u32,
    kind: usize,
    thing: usize,
) -> Option<[Vec<u16>; message::LINES]> {
    if thing >= *THINGS.get(kind)? {
        return None;
    }
    four_lines(file, base, TEXTS[kind], thing)
}

/// The text of the message from the screen's file.
pub fn refusal_text(file: &[u8], base: u32) -> Option<Vec<u16>> {
    text_at(file, base, REFUSAL)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::kind;

    fn lines(pieces: &[(u16, i16, i16)], rows: usize) -> Vec<Vec<Piece>> {
        let line: Vec<Piece> = (pieces.iter())
            .map(|&(kind, x, w)| Piece {
                flags: 0x10,
                kind,
                value: 0,
                rect: Rect {
                    x,
                    y: 137,
                    w,
                    h: 21,
                },
                margin: (3, 5),
            })
            .collect();
        vec![line; rows]
    }

    fn shown<'a>(worn_lines: &'a [Vec<Piece>], list_lines: &'a [Vec<Piece>]) -> Shown<'a> {
        Shown {
            worn_lines,
            list_lines,
            worn: [0, FREE, 4],
            values: [10, 20, 30, 40, 50],
            would: [12, 20, 29, 40, 50],
            rows: [
                GearRow { thing: 2, count: 1 },
                GearRow { thing: 1, count: 3 },
                GearRow {
                    thing: FREE,
                    count: 0,
                },
            ],
            last_place: 1,
        }
    }

    #[test]
    fn a_pointer_finds_the_rows_of_what_he_has_on_and_of_the_list() {
        use crate::carried::Carried;
        let worn = lines(&[(kind::PICTURE, 0, 50), (kind::PICTURE, 0, 21)], 3);
        let list = lines(&[(kind::PICTURE, 83, 21), (kind::TEXT, 104, 112)], 3);
        let mut screen = EquipScreen::open();
        // The kinds: the three rows of what he has on, 22 apart from y 61.
        let found = spots(&screen, &worn, &list);
        let rect = |x, y, w, h| Rect { x, y, w, h };
        assert_eq!(found.len(), 3);
        assert_eq!(found[2], ((KIND, 2), rect(10, 105, 71, 21)));
        assert_eq!(here(&screen), (KIND, 0));
        assert_eq!(way(&screen, (KIND, 2)), crate::pad::DOWN);
        assert_eq!(way(&screen, (LIST, 2)), 0);
        // The list of a kind: a row for each line it shows.
        let mut carried = Carried::new_game();
        let mut pad = crate::pad::Pad::default();
        pad.read(crate::pad::CROSS, crate::pad::Repeat::of(1), 1);
        screen.openness = OPEN;
        screen.pass(&mut carried, &pad);
        assert_eq!(screen.step, LIST);
        let found = spots(&screen, &worn, &list);
        assert_eq!(found.len(), 3);
        assert_eq!(found[1], ((LIST, 1), rect(83, 159, 133, 21)));
        // Nothing under the message.
        screen.message = true;
        assert!(spots(&screen, &worn, &list).is_empty());
    }

    fn frames(drawn: &[Drawn]) -> Vec<(Rect, u8, u32)> {
        (drawn.iter())
            .filter_map(|drawn| match *drawn {
                Drawn::Frame { rect, mode, time } => Some((rect, mode, time)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn on_the_kinds_the_cursors_line_pulses_and_the_list_is_plain() {
        let worn = lines(&[(kind::PICTURE, 0, 50), (kind::PICTURE, 0, 21)], 3);
        let list = lines(&[(kind::PICTURE, 83, 21), (kind::TEXT, 104, 112)], 3);
        let shown = shown(&worn, &list);
        let mut screen = EquipScreen::open();
        screen.kind = 2;
        let drawn = drawn(&screen, &shown, 7, |_| 10);
        let frames = frames(&drawn);
        // The box of the numbers, six frames of the lines, six of the list.
        assert_eq!(frames.len(), 1 + 6 + 6);
        assert_eq!(frames[0], (NUMBERS, 0, 0));
        // Only the third line of what he has on pulses, 44 below the first.
        let pulsing: Vec<_> = frames.iter().filter(|frame| frame.2 != 0).collect();
        assert_eq!(pulsing.len(), 2);
        assert!(pulsing.iter().all(|frame| frame.0.y == 61 + 44));
        // Nothing of what would be is there before a thing is under the cursor.
        assert!(!drawn.iter().any(|d| matches!(d, Drawn::Tinted { .. })));
        // The armour's place is empty: its picture is not asked for.
        let places: Vec<usize> = (drawn.iter())
            .filter_map(|drawn| match *drawn {
                Drawn::WornPicture { place, .. } => Some(place),
                _ => None,
            })
            .collect();
        assert_eq!(places, [0, 2]);
        // The rows of the list: two things of the third kind, the empty row bare.
        let things: Vec<(usize, u8, (i16, i16))> = (drawn.iter())
            .filter_map(|drawn| match *drawn {
                Drawn::GearPicture {
                    kind, thing, at, ..
                } => Some((kind, thing, at)),
                _ => None,
            })
            .collect();
        assert_eq!(things, [(2, 2, (86, 142)), (2, 1, (86, 164))]);
    }

    #[test]
    fn on_the_list_the_numbers_of_what_would_be_stand_in_their_colours() {
        let worn = lines(&[(kind::PICTURE, 0, 50)], 3);
        let list = lines(&[(kind::PICTURE, 83, 21)], 3);
        let mut shown = shown(&worn, &list);
        shown.last_place = 5;
        let mut screen = EquipScreen::open();
        (screen.step, screen.thing) = (LIST, 2);
        (screen.kept[1], screen.kept_first[1]) = (1, 2);
        let drawn = drawn(&screen, &shown, 7, |_| 10);
        let would: Vec<(i32, (i16, i16), usize)> = (drawn.iter())
            .filter_map(|drawn| match *drawn {
                Drawn::Tinted {
                    value,
                    ends,
                    colour,
                    ..
                } => Some((value, ends, colour)),
                _ => None,
            })
            .collect();
        assert_eq!(
            would,
            [
                (12, (303, 54), colour::RISES),
                (20, (303, 68), colour::STAYS),
                (29, (303, 82), colour::FALLS),
                (40, (303, 96), colour::STAYS),
                (50, (303, 110), colour::STAYS),
            ]
        );
        let signs: Vec<(u16, (i16, i16))> = (drawn.iter())
            .filter_map(|drawn| match *drawn {
                Drawn::Sign { turns, at, .. } => Some((turns, at)),
                _ => None,
            })
            .collect();
        // Between the columns, and both of the list: lines above and below.
        assert_eq!(
            signs,
            [(0, ARROW), (sign::BELOW, SIGNS[1]), (sign::ABOVE, SIGNS[0])]
        );
        let frames = frames(&drawn);
        assert_eq!(frames[1], (WOULD, 0, 0));
        // The cursor's line of what he has on is marked, the list's second row pulses.
        assert!(frames.contains(&(
            Rect {
                x: 10,
                y: 61,
                w: 50,
                h: 21
            },
            2,
            0
        )));
        assert!(frames.contains(&(
            Rect {
                x: 83,
                y: 159,
                w: 21,
                h: 21
            },
            1,
            7
        )));
        // Without a thing under the cursor nothing would change.
        screen.thing = FREE;
        let bare = super::drawn(&screen, &shown, 7, |_| 10);
        assert!(!bare.iter().any(|d| matches!(d, Drawn::Tinted { .. })));
    }

    #[test]
    fn a_description_and_the_message_stand_in_place_of_a_panel() {
        let worn = lines(&[(kind::PICTURE, 0, 50), (kind::PICTURE, 0, 21)], 3);
        let list = lines(&[(kind::PICTURE, 83, 21)], 3);
        let shown = shown(&worn, &list);
        // The list's thing: in place of what he has on and the numbers.
        let mut screen = EquipScreen::open();
        (screen.step, screen.thing, screen.describes_thing) = (LIST, 2, true);
        let drawn = drawn(&screen, &shown, 0, |_| 10);
        assert_eq!(frames(&drawn)[0], (DESCRIBED, 0, 0));
        assert_eq!(
            drawn[2],
            Drawn::GearDescribed {
                kind: 0,
                thing: 2,
                line: 1,
                at: (14, 78)
            }
        );
        assert!(!drawn.iter().any(|d| matches!(d, Drawn::WornPicture { .. })));
        assert!(drawn.iter().any(|d| matches!(d, Drawn::GearPicture { .. })));
        // What is on: in place of the list, where a message stands.
        let mut screen = EquipScreen::open();
        (screen.kind, screen.describes_worn) = (2, true);
        let drawn = super::drawn(&screen, &shown, 0, |_| 10);
        assert!(frames(&drawn).contains(&(message::BOX, 0, 0)));
        assert!(drawn.contains(&Drawn::GearDescribed {
            kind: 2,
            thing: 4,
            line: 0,
            at: (14, 158)
        }));
        assert!(!drawn.iter().any(|d| matches!(d, Drawn::GearPicture { .. })));
        // A thing past its table has no box.
        assert!(description(1, 15, DESCRIBED).is_empty());
        assert!(description(3, 0, DESCRIBED).is_empty());
        // The message: its box and its line, and no list.
        let mut screen = EquipScreen::open();
        screen.message = true;
        let drawn = super::drawn(&screen, &shown, 0, |_| 10);
        assert_eq!(drawn[1], Drawn::GearRefused { at: (14, 158) });
        assert!(!drawn.iter().any(|d| matches!(d, Drawn::GearPicture { .. })));
        assert!(drawn.iter().any(|d| matches!(d, Drawn::WornPicture { .. })));
    }

    #[test]
    fn the_texts_are_read_by_their_tables() {
        let base = 0x800C_2BB8;
        let mut file = vec![0u8; (TEXTS[2] - base) as usize + 16 * THINGS[2]];
        let entry = (TEXTS[1] - base) as usize + 16 * 3 + 4;
        file[entry..entry + 4].copy_from_slice(&REFUSAL.to_le_bytes());
        let at = (REFUSAL - base) as usize;
        file[at..at + 2].copy_from_slice(&[0x82, 0x60]);
        let lines = description_text(&file, base, 1, 3).expect("armour 3");
        assert_eq!((lines[0].len(), lines[1].len()), (0, 1));
        assert_eq!(refusal_text(&file, base), Some(lines[1].clone()));
        assert_eq!(description_text(&file, base, 1, 15), None);
        assert_eq!(description_text(&file, base, 3, 0), None);
    }
}
