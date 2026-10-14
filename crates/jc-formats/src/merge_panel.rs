
use crate::battle::record::Record;
use crate::creatures::{self, MOST_CARRIED};
use crate::items;
use crate::menu::{Piece, Rect, Style, kind, piece, placed, sign};
use crate::merge_screen::{MergeScreen, ON_THE_NEW, SKIN_ROWS, Said, flag, state};
use crate::player_panel::{
    self, AttackRow, Drawn, Family, Listed, SpellRows, attacks, detail_in, message, spells, text_at,
};
use crate::rooms::Image;
use crate::window::{OPENING, Way, opened};

/// The lists of the lines in the screen's file: the main menu's five rows, a picture
/// each (`0x800BD0B8`), and the three rows of the carried and the four of the kept, a
/// frame each (`0x800BE718`, `0x800BE9CC`).
pub const MAIN_LINES: u32 = 0x800C_09B0;
pub const CARRIED_LINES: u32 = 0x800C_0000;
pub const KEPT_LINES: u32 = 0x800C_00F0;
/// The flag word the menu drawer is given for the main menu and for a list: the frame
/// under the cursor pulses.
const MENU_FLAGS: u32 = 0x02;

/// The panels in code, by their bits.
pub mod panel {
    pub const MAIN: u32 = 1 << 0;
    pub const LISTS: u32 = 1 << 1;
    pub const CREATURE: u32 = 1 << 2;
    pub const ANSWER: u32 = crate::merge_screen::ANSWER_PANEL;
    /// The first creature of a merge; the second and the new one; the minion skins.
    pub const FIRST: u32 = 1 << 4;
    pub const OTHERS: u32 = 1 << 5;
    pub const SKINS: u32 = 1 << 6;
}

/// The frames of the three creatures of a merge (the rectangles from `0x800BF98C`):
/// the first, the second and the new one. A name stands about the middle of its
/// frame, this far below its upper edge; the new one has a sprite instead, this far
/// from its frame's corner.
pub const OF_THREE: [Rect; 3] = [
    Rect {
        x: 92,
        y: 79,
        w: 60,
        h: 12,
    },
    Rect {
        x: 179,
        y: 79,
        w: 60,
        h: 12,
    },
    Rect {
        x: 261,
        y: 79,
        w: 49,
        h: 12,
    },
];
const THREE_NAME_DOWN: i16 = 2;
pub const NEW_SPRITE: usize = 66;
const NEW_SPRITE_AT: (i16, i16) = (7, 1);
/// The two signs between the three frames, of family 0 of the signs and not turned
/// (`0x8002B438(0, time, place, 14)`), while the cursor of the three is up.
pub const BETWEEN: [(i16, i16); 2] = [(165, 85), (250, 85)];
pub const BETWEEN_FAMILY: u16 = 0;

/// The list of the minion skins (`0x800BEF24`): its four lines in the screen's file,
/// where its drawer puts the three pieces of a row (the picture, the name, the
/// count), the first row's y and the step of a row, and the two scroll signs (the
/// places at `0x800C026C`).
pub const SKIN_LINES: u32 = 0x800C_0434;
pub const SKIN_PIECES: [i16; 3] = [123, 144, 251];
pub const SKIN_FIRST_ROW: i16 = 104;
pub const SKIN_STEP: i16 = 22;
pub const SKIN_SIGNS: [(i16, i16); 2] = [(200, 97), (200, 198)];
/// The things of the player's kind 4 that are skins with a name (the drawer's own
/// limit); any other line of the list is empty.
pub const SKINS: u8 = 11;

/// The bar under the cursor of an answer (`0x800BCAD4`): its place on row 0, its size
/// and how far apart the rows are. It lies on the keeper's second and third line.
pub const ANSWER_BAR: Rect = Rect {
    x: 86,
    y: 37,
    w: 30,
    h: 16,
};
const ANSWER_STEP: i16 = 16;

/// The keeper's box (the rectangle at `0x800BF974`) and where its three lines start
/// (the record at `0x800BF9A4`).
pub const KEEPER: Rect = Rect {
    x: 80,
    y: 20,
    w: 223,
    h: 54,
};
pub const KEEPER_TEXTS: [(i16, i16); 3] = [(87, 24), (87, 40), (87, 56)];
/// The box of the keeper's face and the face in it (the executable's `0x8002A54C`).
pub const FACE_BOX: Rect = Rect {
    x: 10,
    y: 9,
    w: 70,
    h: 66,
};
pub const FACE_AT: (i16, i16) = (13, 9);
/// The sign of "more" on the keeper's two last lines (`0x8003BD20`).
pub const MORE_AT: (i16, i16) = (286, 60);

/// The titles of the two lists: a frame (`0x800BF97C`, `0x800BF984`) and a sprite in it.
pub const TITLES: [(Rect, usize, (i16, i16)); 2] = [
    (
        Rect {
            x: 10,
            y: 77,
            w: 71,
            h: 15,
        },
        181,
        (13, 80),
    ),
    (
        Rect {
            x: 10,
            y: 138,
            w: 71,
            h: 15,
        },
        14,
        (14, 141),
    ),
];
/// The rows of the kept.
pub const KEPT_ROWS: usize = 4;
/// How far below a row's upper edge its name stands.
const NAME_DOWN: i16 = 3;
/// The scroll signs of the kept, below and above (`0x8002B438`).
pub const SIGNS: [(i16, i16); 2] = [(82, 203), (82, 162)];
/// The box of the creature under the cursor (`0x800BF96C`).
pub const CREATURE: Rect = Rect {
    x: 92,
    y: 94,
    w: 218,
    h: 117,
};

/// Where a page of the creature's box is handed to its routine (`0x800BC8C8` on): the
/// first piece of the first row of the body attacks and of the spells, and how far the
/// "Family" is moved from where the player's menu has it.
pub const ATTACKS_AT: (i16, i16) = (92, 94);
pub const SPELLS_AT: (i16, i16) = (126, 109);
pub const FAMILY_MOVED: (i16, i16) = (40, 30);
/// The frame style both lists are handed: the row under the cursor pulses.
const PAGE_FLAGS: u32 = 0x02;
/// The corner of the description of a body attack or of a spell, which stands in
/// place of the keeper's box (`0x800BC6D0`).
pub const DESCRIBED_AT: (i16, i16) = (10, 18);
/// The screen's own tables of the texts of the spells and of the body attacks: the
/// addresses of four lines each (`0x800BC740`, `0x800BC708`).
pub const SPELL_TEXTS: u32 = 0x800C_0448;
pub const ATTACK_TEXTS: u32 = 0x800C_0628;

/// The tables of the keeper's lines in the screen's file, an entry the address of a
/// record of three texts: by the row of the main menu, by the state, and by the
/// refusal; each for the argument 4 and for any other (`0x800BBD24`, `0x800BBC7C`).
const ROW_LINES: [u32; 2] = [0x800B_FEE4, 0x800B_FED0];
const STATE_LINES: [u32; 2] = [0x800B_FCD0, 0x800B_FC90];
const REFUSAL_LINES: [u32; 2] = [0x800B_FE44, 0x800B_FDE0];
/// The second lines by the state (kind 3 of `0x800BBD24`): the states 3, 5 and 13
/// have a record, the others none.
const SECOND_LINES: [u32; 2] = [0x800B_FD50, 0x800B_FD10];
/// The three fixed lines while a creature's form is looked at (kind 2): one record of
/// three texts for each set.
const FORM_LINES: [u32; 2] = [0x800B_FC84, 0x800B_FB94];
pub const ROWS: usize = 5;
pub const REFUSALS: usize = 5;
/// The argument of the script's command that picks the second set.
pub const SECOND_SET: i16 = 4;

/// The lists of the line of the buttons (`0x800BCEB4`): of a first pick, of the place
/// it changes with, and the executable's plain pair.
pub const LEGEND_OF_A_PICK: u32 = 0x800C_0224;
pub const LEGEND_OF_AN_EXCHANGE: u32 = 0x800C_024C;
/// Of the second pick of a merge; of the new creature beside its two parents, where
/// the confirm and the cancel both lead on to "Combine"; and of any other state whose
/// second lines wait for a button.
pub const LEGEND_OF_A_SECOND: u32 = 0x800C_014C;
pub const LEGEND_OF_THE_NEW: u32 = 0x800C_0194;
pub const LEGEND_OF_MORE: u32 = 0x800C_0214;
/// State 5 (`0x800BCF80`): with nothing marked, and with a place marked, where the
/// circle is "Spin".
pub const LEGEND_OF_THE_SPIN: u32 = 0x800C_0124;
pub const LEGEND_OF_A_SPIN: u32 = 0x800C_0104;
pub const LEGEND_PLAIN: u32 = 0x8008_25A4;
/// And of the view, of a page of the creature's box, and of a page with rows, which
/// has the button of a description too.
pub const LEGEND_OF_THE_VIEW: u32 = 0x800C_016C;
pub const LEGEND_OF_A_PAGE: u32 = 0x800C_01DC;
pub const LEGEND_OF_ROWS: u32 = 0x800C_01F4;
/// While a creature's form is looked at, in any state: "Quit" and "Zoom".
pub const LEGEND_OF_THE_FORM: u32 = 0x800C_01BC;

/// The list of the line of the buttons of the state the screen is in, none on the
/// keeper's last line.
pub fn legend(screen: &MergeScreen) -> Option<u32> {
    match screen.state {
        state::DONE => None,
        _ if screen.flags & flag::FORM != 0 => Some(LEGEND_OF_THE_FORM),
        state::SPIN => match screen.marks {
            0 => Some(LEGEND_OF_THE_SPIN),
            _ => Some(LEGEND_OF_A_SPIN),
        },
        state::MERGE_FIRST | state::TAKE => Some(LEGEND_OF_A_PICK),
        state::MERGE_SECOND => Some(LEGEND_OF_A_SECOND),
        state::MERGE_ASKED if screen.flags & flag::MORE != 0 => Some(LEGEND_OF_THE_NEW),
        state::MERGE_ASKED => Some(LEGEND_PLAIN),
        state::EXCHANGE => Some(LEGEND_OF_AN_EXCHANGE),
        state::VIEW => Some(LEGEND_OF_THE_VIEW),
        state::ABILITIES => match screen.flags & (flag::ATTACKS | flag::SPELLS) {
            0 => Some(LEGEND_OF_A_PAGE),
            _ => Some(LEGEND_OF_ROWS),
        },
        _ if screen.flags & flag::MORE != 0 => Some(LEGEND_OF_MORE),
        _ => Some(LEGEND_PLAIN),
    }
}

/// The four lines of a spell's text and of a body attack's from the screen's file,
/// whose first byte is at `base`.
pub fn spell_text(file: &[u8], base: u32, spell: usize) -> Option<[Vec<u16>; message::LINES]> {
    (spell < spells::SPELLS).then(|| player_panel::four_lines(file, base, SPELL_TEXTS, spell))?
}

pub fn attack_text(file: &[u8], base: u32, attack: usize) -> Option<[Vec<u16>; message::LINES]> {
    (attack < attacks::ATTACKS)
        .then(|| player_panel::four_lines(file, base, ATTACK_TEXTS, attack))?
}

/// The three texts of every line the keeper has, from the screen's file, whose first
/// byte is at `base`: the rows of the main menu, the states, the refusals and the
/// second lines of the states (empty for a state without them), in the order of
/// [`line_of`].
pub fn keeper_texts(file: &[u8], base: u32, argument: i16) -> Option<Vec<[Vec<u16>; 3]>> {
    let set = usize::from(argument == SECOND_SET);
    let image = Image { data: file, base };
    let tables = [
        (ROW_LINES[set], ROWS),
        (STATE_LINES[set], crate::merge_screen::STATES),
        (REFUSAL_LINES[set], REFUSALS),
        (SECOND_LINES[set], crate::merge_screen::STATES),
    ];
    let mut lines = Vec::new();
    for (table, count) in tables {
        for line in 0..count {
            let record = image.u32(crate::rooms::step(table, line, 4).ok()?).ok()?;
            let mut texts: [Vec<u16>; 3] = Default::default();
            if record == 0 {
                lines.push(texts);
                continue;
            }
            for (n, text) in texts.iter_mut().enumerate() {
                let address = image.u32(crate::rooms::step(record, n, 4).ok()?).ok()?;
                *text = text_at(file, base, address)?;
            }
            lines.push(texts);
        }
    }
    let mut texts: [Vec<u16>; 3] = Default::default();
    for (n, text) in texts.iter_mut().enumerate() {
        let address = image
            .u32(crate::rooms::step(FORM_LINES[set], n, 4).ok()?)
            .ok()?;
        *text = text_at(file, base, address)?;
    }
    lines.push(texts);
    Some(lines)
}

/// The line of [`keeper_texts`] that stands in the keeper's box.
pub fn line_of(said: Said) -> i16 {
    match said {
        Said::Row(row) => row,
        Said::State(state) => ROWS as i16 + state,
        Said::Refusal(refusal) => (ROWS + crate::merge_screen::STATES) as i16 + refusal,
        Said::Second(state) => (ROWS + crate::merge_screen::STATES + REFUSALS) as i16 + state,
        Said::Form => (ROWS + 2 * crate::merge_screen::STATES + REFUSALS) as i16,
    }
}

/// What the drawing takes from the game.
#[derive(Debug, Clone, Copy)]
pub struct Shown<'a> {
    /// The lines of the main menu ([`MAIN_LINES`]) and of the two lists
    /// ([`CARRIED_LINES`], [`KEPT_LINES`]).
    pub main_lines: &'a [Vec<Piece>],
    pub carried_lines: &'a [Vec<Piece>],
    pub kept_lines: &'a [Vec<Piece>],
    /// The four lines of the list of the minion skins ([`SKIN_LINES`]).
    pub skin_lines: &'a [Vec<Piece>],
    /// The first five of the values a fight counts with, of the creature of
    /// [`MergeScreen::shown_place`], and of the one it is compared with
    /// ([`MergeScreen::compared`]).
    pub values: [u8; 5],
    pub compared_values: [u8; 5],
    /// The executable's lines of the lists of the body attacks and of the spells
    /// (`player_panel::attacks::LINES`, `player_panel::spells::LINES`), the sprites of
    /// both, and the five move entries of that creature as a row shows them.
    pub attack_lines: &'a [Vec<Piece>],
    pub spell_lines: &'a [Vec<Piece>],
    pub attack_pictures: &'a [[u8; 5]; attacks::ROWS],
    pub spell_pictures: &'a [u8],
    pub attack_rows: [AttackRow; attacks::ROWS],
    /// Where that creature comes from, for its "Family".
    pub family: Option<&'a Family>,
    /// Whether the circle is held.
    pub circle: bool,
}

/// The body attack or the spell whose text the pass shows in place of the keeper's box
/// (the flag `0x1000` of `0x800BC580`): on the page of the body attacks, while the
/// circle is held, the entry under the second cursor when it is known and has an
/// attack; on the page of the spells the spell under it, when the row has one.
pub fn described(screen: &MergeScreen, shown: &Shown) -> Option<Listed> {
    if !shown.circle {
        return None;
    }
    // Beside the new creature of a merge nothing is described (`0x800BC650`).
    if screen.state == state::MERGE_ASKED && screen.flags & flag::MORE != 0 {
        return None;
    }
    let cursor = screen.page_cursor;
    match screen.drawn_page() {
        flag::ATTACKS => {
            let row = shown.attack_rows.get(usize::try_from(cursor.at).ok()?)?;
            (row.known && row.attack != 0).then_some(Listed::Attack(row.attack.into()))
        }
        flag::SPELLS => {
            let record = &screen.creature(screen.shown_place())?.record;
            let spell = player_panel::spell_at(record.spells, cursor.at.wrapping_add(cursor.first));
            (spell < spells::SPELLS).then_some(Listed::Spell(spell))
        }
        _ => None,
    }
}

/// A menu through the shared menu drawer (`0x8002A60C`): the frame of every piece, the
/// one under the cursor pulsing, and the picture of a piece that has one.
fn menu(
    lines: &[Vec<Piece>],
    cursor: Option<i16>,
    time: u32,
    width: &impl Fn(usize) -> i16,
) -> Vec<Drawn> {
    let mut drawn = Vec::new();
    for (line, pieces) in lines.iter().enumerate() {
        let on_cursor = cursor.and_then(|at| usize::try_from(at).ok()) == Some(line);
        let (style, pulses) = Style::of_line(MENU_FLAGS, on_cursor, 0);
        for piece in pieces {
            if piece.flags & piece::BARE == 0 {
                drawn.push(Drawn::Frame {
                    rect: piece.rect,
                    mode: style.mode,
                    time: if pulses { time } else { 0 },
                });
            }
            let shows = piece.flags & piece::EMPTY == 0 && piece.kind == kind::PICTURE;
            // A picture of -1 is none.
            if let Some(number) = (shows && piece.value != u32::MAX).then_some(piece.value) {
                let number = number as usize;
                let room = piece.rect.inside(piece.margin);
                drawn.push(Drawn::Sprite {
                    number,
                    at: placed(piece.flags, room, width(number)),
                    clip: piece.rect,
                });
            }
        }
    }
    drawn
}

/// The names of the rows of a list over their frames: the creature of each row whose
/// place has the list's own state, about the middle of the row's frame.
fn names(
    screen: &MergeScreen,
    lines: &[Vec<Piece>],
    first: usize,
    own: i16,
    marked: impl Fn(usize) -> bool,
) -> Vec<Drawn> {
    let rows = lines.iter().enumerate();
    rows.filter_map(|(row, pieces)| {
        let place = first + row;
        let rect = pieces.first()?.rect;
        (screen.listed.state(place) == own).then_some(Drawn::PlaceName {
            place,
            middle: (rect.x + rect.w / 2, rect.y + NAME_DOWN),
            marked: marked(row),
        })
    })
    .collect()
}

/// The two lists (`0x800BE9CC`, then `0x800BE718`, then their titles), from what lies
/// underneath to what lies on top.
fn lists(
    screen: &MergeScreen,
    shown: &Shown,
    time: u32,
    width: &impl Fn(usize) -> i16,
) -> Vec<Drawn> {
    let (in_carried, in_kept) = screen.cursor_rows();
    let scroll = screen.scroll().max(0);
    let mut drawn = Vec::new();
    // While the cursor is in the kept, the signs of what lies below and above.
    if screen.flags & flag::IN_CARRIED == 0 {
        let last = (creatures::PLACES - MOST_CARRIED - KEPT_ROWS) as i16;
        if scroll != 0 {
            drawn.push(Drawn::Sign {
                turns: sign::ABOVE,
                at: SIGNS[1],
                time,
            });
        }
        if scroll < last {
            drawn.push(Drawn::Sign {
                turns: sign::BELOW,
                at: SIGNS[0],
                time,
            });
        }
    }
    let bit = |word: u32, n: usize| word & 1u32.wrapping_shl(n as u32) != 0;
    let first = MOST_CARRIED + scroll as usize;
    drawn.extend(menu(shown.kept_lines, in_kept, time, width));
    drawn.extend(names(
        screen,
        shown.kept_lines,
        first,
        creatures::state::KEPT,
        |row| bit(screen.marks, first + row) && bit(screen.kept_marks, first + row - MOST_CARRIED),
    ));
    drawn.extend(menu(shown.carried_lines, in_carried, time, width));
    drawn.extend(names(
        screen,
        shown.carried_lines,
        0,
        creatures::state::CARRIED,
        |row| bit(screen.marks, row) && bit(screen.carried_marks, row),
    ));
    for (rect, number, at) in TITLES.into_iter().rev() {
        drawn.push(Drawn::Frame {
            rect,
            mode: 0,
            time: 0,
        });
        drawn.push(Drawn::Sprite {
            number,
            at,
            clip: rect,
        });
    }
    drawn
}

/// The box of the creature under the cursor (panel 2): a plain frame, and for a place
/// in use the creature's "Detail" (`0x8002DBA4` at the box's corner), cut to the box
/// as it opens. With another page up neither is drawn: the list of its body attacks
/// (`0x8002CE84`), the list of its spells (`0x8002CB24`) or its "Family"
/// (`0x8002D82C`) stands there, none of which opens.
fn creature(screen: &MergeScreen, record: Option<&Record>, shown: &Shown, time: u32) -> Vec<Drawn> {
    // While its form is looked at the box is the frame alone, as for a free place
    // (`0x800BC8B0`): the creature's actor is what is seen there.
    let Some(record) = record.filter(|_| screen.flags & flag::FORM == 0) else {
        return vec![Drawn::Frame {
            rect: CREATURE,
            mode: 0,
            time: 0,
        }];
    };
    let place = screen.shown_place();
    let cursor = screen.page_cursor;
    match screen.drawn_page() {
        flag::ATTACKS => player_panel::attack_rows_at(
            PAGE_FLAGS,
            cursor.at,
            ATTACKS_AT,
            shown.attack_lines,
            shown.attack_rows,
            shown.attack_pictures,
            time,
        ),
        flag::SPELLS => {
            let rows = SpellRows {
                rows: std::array::from_fn(|row| {
                    player_panel::spell_at(record.spells, cursor.first.wrapping_add(row as i16))
                }),
                // The screen hands the list a 1 where a fight hands 0: no row is dim
                // for the mana its spell costs.
                short: [false; spells::ROWS],
                known: player_panel::spells_known(record.spells),
            };
            player_panel::spell_rows(
                PAGE_FLAGS,
                (cursor.at, cursor.first),
                SPELLS_AT,
                shown.spell_lines,
                rows,
                shown.spell_pictures,
                time,
            )
        }
        flag::FAMILY => match shown.family {
            Some(family) => player_panel::family_panel_at(FAMILY_MOVED, place, record, family),
            None => Vec::new(),
        },
        _ => {
            let openness = i32::from(screen.openness(panel::CREATURE));
            let clip = opened(CREATURE, openness, OPENING, Way::Middle);
            // The new creature of a merge is compared with the first of the two.
            let before = (screen.compared()).map(|first| (&first.record, shown.compared_values));
            detail_in(CREATURE, clip, place, (record, shown.values), before)
        }
    }
}

/// The keeper's box, from what lies underneath to what lies on top: the frame and over
/// it the fill, the three texts of the keeper's line, then the box of the face, its
/// fill under the face and its frame over it; and on her two last lines the sign of
/// "more".
fn keeper(screen: &MergeScreen) -> Vec<Drawn> {
    let mut drawn = vec![
        Drawn::WindowFrame { rect: KEEPER },
        Drawn::WindowFill { rect: KEEPER },
    ];
    for (text, at) in KEEPER_TEXTS.into_iter().enumerate() {
        drawn.push(Drawn::KeeperSaid {
            line: line_of(screen.said),
            text,
            at,
        });
    }
    drawn.extend([
        Drawn::WindowFill { rect: FACE_BOX },
        Drawn::KeeperFace { at: FACE_AT },
        Drawn::WindowFrame { rect: FACE_BOX },
    ]);
    if matches!(screen.state, state::DONE | state::FULL) || screen.flags & flag::MORE != 0 {
        drawn.push(Drawn::More { at: MORE_AT });
    }
    drawn
}

/// A frame of the three of a merge (`0x800BCB44`, `0x800BCC34`): in the cursor's
/// colour, which pulses for the one the cursor of the three is on while that cursor
/// is up, with the name of a picked creature or the new one's sprite over it.
fn of_three(screen: &MergeScreen, which: usize, time: u32) -> Vec<Drawn> {
    let rect = OF_THREE[which];
    let up = screen.state == state::MERGE_ASKED && screen.flags & flag::MORE != 0;
    let on = up && usize::try_from(screen.which.at) == Ok(which);
    let frame = Drawn::Frame {
        rect,
        mode: 1,
        time: if on { time } else { 0 },
    };
    let name = |place: usize| Drawn::PlaceName {
        place,
        middle: (rect.x + rect.w / 2, rect.y + THREE_NAME_DOWN),
        marked: false,
    };
    let content = match which {
        0 => name(screen.first),
        1 => name(screen.second),
        _ => Drawn::Sprite {
            number: NEW_SPRITE,
            at: (rect.x + NEW_SPRITE_AT.0, rect.y + NEW_SPRITE_AT.1),
            clip: rect,
        },
    };
    vec![frame, content]
}

/// The list of the minion skins (`0x800BEF24`), from what lies underneath to what
/// lies on top: the two scroll signs, then four rows through the shared menu drawer,
/// each the skin's picture, its name and how many of it he has, in a frame of their
/// own; the frames of the row under the cursor pulse. A line whose thing is no skin,
/// or of which he has none, has its frames alone. As the list of the items, it does
/// not open.
fn skins(screen: &MergeScreen, lines: &[Vec<Piece>], time: u32) -> Vec<Drawn> {
    let (cursor, first) = screen.skin_cursor();
    let kind = items::Kind::Creature;
    let mut drawn = Vec::new();
    // A line for every place up to the last in use, four at least.
    let count = (screen.things.last_place(kind) as i32 + 1).max(SKIN_ROWS as i32);
    if i32::from(first) < count - SKIN_ROWS as i32 && count > SKIN_ROWS as i32 {
        drawn.push(Drawn::Sign {
            turns: sign::BELOW,
            at: SKIN_SIGNS[1],
            time,
        });
    }
    if first != 0 {
        drawn.push(Drawn::Sign {
            turns: sign::ABOVE,
            at: SKIN_SIGNS[0],
            time,
        });
    }
    for (row, pieces) in lines.iter().enumerate().take(SKIN_ROWS) {
        let line = usize::try_from(i32::from(first) + row as i32).ok();
        let thing = line.map_or(items::FREE, |line| screen.things.number_at(line, kind));
        let count = match (line, thing < SKINS) {
            (Some(line), true) => screen.things.count_at(line, kind),
            _ => 0,
        };
        let (style, pulses) = Style::of_line(MENU_FLAGS, usize::try_from(cursor) == Ok(row), 0);
        let y = SKIN_FIRST_ROW + SKIN_STEP * row as i16;
        for (k, (piece, x)) in pieces.iter().zip(SKIN_PIECES).enumerate() {
            let clip = Rect { x, y, ..piece.rect };
            if piece.flags & piece::BARE == 0 {
                drawn.push(Drawn::Frame {
                    rect: clip,
                    mode: style.mode,
                    time: if pulses { time } else { 0 },
                });
            }
            if count == 0 {
                continue;
            }
            let room = clip.inside(piece.margin);
            drawn.push(match k {
                0 => Drawn::ItemPicture {
                    item: thing,
                    at: (room.x, room.y),
                    dim: false,
                    clip,
                },
                1 => Drawn::ItemName {
                    item: thing,
                    at: (room.x, room.y),
                    dim: false,
                    clip,
                },
                // A number ends 3 left of its room's right edge, 2 below its upper one.
                _ => Drawn::Count {
                    value: count.into(),
                    ends: (room.x + room.w - 3, room.y + 2),
                    dim: false,
                    clip,
                },
            });
        }
    }
    drawn
}

/// What the screen draws in a pass (`0x800BC550`), from what lies underneath to what
/// lies on top: the panels asked for from the last that is added to the first, then
/// the keeper's box, which is added first, or in its place the description of the
/// body attack or the spell under the cursor of a page ([`described`]). The line of the buttons ([`legend`]), which
/// lies under all of it, and the wall are the caller's. `time` is the menus' time and
/// `width` gives the width of a sprite of the pool.
pub fn drawn(
    screen: &MergeScreen,
    shown: &Shown,
    time: u32,
    width: impl Fn(usize) -> i16,
) -> Vec<Drawn> {
    let asked = |panel: u32| screen.panels & panel != 0;
    let mut drawn = Vec::new();
    if asked(panel::SKINS) {
        drawn.extend(skins(screen, shown.skin_lines, time));
    }
    if asked(panel::OTHERS) {
        // While the cursor of the three is up, the two signs between the frames.
        if screen.state == state::MERGE_ASKED && screen.flags & flag::MORE != 0 {
            drawn.extend(BETWEEN.map(|at| Drawn::SignOf {
                family: BETWEEN_FAMILY,
                turns: 0,
                at,
                time,
            }));
        }
        drawn.extend(of_three(screen, ON_THE_NEW as usize, time));
        drawn.extend(of_three(screen, 1, time));
    }
    if asked(panel::FIRST) {
        drawn.extend(of_three(screen, 0, time));
    }
    if asked(panel::MAIN) {
        drawn.extend(menu(shown.main_lines, Some(screen.cursor.at), time, &width));
    }
    if asked(panel::CREATURE) {
        let place = screen.creature(screen.shown_place());
        drawn.extend(creature(screen, place.map(|c| &c.record), shown, time));
    }
    if asked(panel::LISTS) {
        drawn.extend(lists(screen, shown, time, &width));
    }
    // The bar of an answer, a frame that pulses, unless a refusal is up, the keeper's
    // second lines wait for a button or the white behind a yes is over the screen
    // (`0x800BCA9C`). The keeper's box is drawn over it.
    let bar = screen.refusal == 0 && screen.flags & (flag::WHITE | flag::MORE) == 0;
    if asked(panel::ANSWER) && bar {
        drawn.push(Drawn::Frame {
            rect: Rect {
                y: ANSWER_BAR
                    .y
                    .wrapping_add(screen.cursor.at.wrapping_mul(ANSWER_STEP)),
                ..ANSWER_BAR
            },
            mode: 1,
            time,
        });
    }
    match described(screen, shown) {
        Some(what) => drawn.extend(player_panel::list_description_at(what, DESCRIBED_AT)),
        None => drawn.extend(keeper(screen)),
    }
    drawn
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::carried::Carried;
    use crate::creatures::state::CARRIED;
    use crate::pad::{self, Pad, Repeat};

    fn record(species: u32) -> Record {
        let mut record = Record::head(&[0u8; Record::HEAD]).expect("a head");
        record.species = species;
        record
    }

    fn lines(places: &[(i16, i16)], value: u32) -> Vec<Vec<Piece>> {
        (places.iter())
            .map(|&(x, y)| {
                vec![Piece {
                    flags: piece::CENTRED,
                    kind: if value == 0 { 0 } else { kind::PICTURE },
                    value,
                    rect: Rect { x, y, w: 64, h: 14 },
                    margin: (0, 0),
                }]
            })
            .collect()
    }

    fn press(buttons: u16) -> Pad {
        let mut pad = Pad::default();
        pad.read(0, Repeat::of(1), 0);
        pad.read(buttons, Repeat::of(1), 1);
        pad
    }

    fn go(screen: &mut MergeScreen, buttons: u16) {
        screen.pass(&press(buttons));
        screen.pass(&Pad::default());
    }

    #[test]
    fn the_main_menu_is_drawn_under_the_keeper_and_the_lists_with_their_names() {
        let mut game = Carried::new_game();
        game.creatures.put(CARRIED, record(5));
        game.caught.put(record(6));
        let mut screen = MergeScreen::open(SECOND_SET, &mut game);
        let main = lines(&[(15, 77), (15, 100), (15, 123), (15, 146), (15, 169)], 43);
        let carried = lines(&[(13, 93), (13, 108), (13, 123)], 0);
        let kept = lines(&[(13, 154), (13, 169), (13, 184), (13, 199)], 0);
        let shown = Shown {
            main_lines: &main,
            carried_lines: &carried,
            kept_lines: &kept,
            skin_lines: &[],
            values: [1, 2, 3, 4, 5],
            compared_values: [0; 5],
            attack_lines: &[],
            spell_lines: &[],
            attack_pictures: &[[0; 5]; attacks::ROWS],
            spell_pictures: &[],
            attack_rows: [AttackRow::default(); attacks::ROWS],
            family: None,
            circle: false,
        };
        let width = |_: usize| 40;
        let first = drawn(&screen, &shown, 7, width);
        // Five framed pictures, the first under the cursor, then the keeper's box.
        assert_eq!(
            first[0],
            Drawn::Frame {
                rect: main[0][0].rect,
                mode: 1,
                time: 7
            }
        );
        assert!(first.contains(&Drawn::Sprite {
            number: 43,
            at: (27, 77),
            clip: main[0][0].rect
        }));
        assert_eq!(first[10], Drawn::WindowFrame { rect: KEEPER });
        assert!(first.contains(&Drawn::KeeperSaid {
            line: 0,
            text: 2,
            at: (87, 56)
        }));
        assert_eq!(legend(&screen), Some(LEGEND_PLAIN));
        // The choice of who goes with him: no main menu, the two lists, the box.
        go(&mut screen, pad::DOWN);
        go(&mut screen, pad::DOWN);
        go(&mut screen, pad::CROSS);
        let lists = drawn(&screen, &shown, 7, width);
        assert_eq!(legend(&screen), Some(LEGEND_OF_A_PICK));
        assert!(
            !lists
                .iter()
                .any(|d| matches!(d, Drawn::Sprite { number: 43, .. }))
        );
        assert!(lists.contains(&Drawn::PlaceName {
            place: 0,
            middle: (45, 96),
            marked: false
        }));
        assert!(lists.contains(&Drawn::PlaceName {
            place: 3,
            middle: (45, 157),
            marked: false
        }));
        // The cursor is among the carried: its row pulses, and no scroll sign is up.
        assert!(lists.contains(&Drawn::Frame {
            rect: carried[0][0].rect,
            mode: 1,
            time: 7
        }));
        assert!(!lists.iter().any(|d| matches!(d, Drawn::Sign { .. })));
        assert!(lists.contains(&Drawn::Frame {
            rect: CREATURE,
            mode: 0,
            time: 0
        }));
        assert!(lists.contains(&Drawn::FullName {
            creature: 0,
            at: (CREATURE.x + 38, CREATURE.y + 5),
            clip: opened(CREATURE, 1, OPENING, Way::Middle)
        }));
        assert!(lists.contains(&Drawn::KeeperSaid {
            line: ROWS as i16 + state::TAKE,
            text: 0,
            at: (87, 24)
        }));
        // A picked place is marked, and in the kept the sign of more below is up.
        go(&mut screen, pad::CROSS);
        for _ in 0..3 {
            go(&mut screen, pad::DOWN);
        }
        let picked = drawn(&screen, &shown, 7, width);
        assert_eq!(legend(&screen), Some(LEGEND_OF_AN_EXCHANGE));
        assert!(picked.contains(&Drawn::PlaceName {
            place: 0,
            middle: (45, 96),
            marked: true
        }));
        assert!(picked.contains(&Drawn::Sign {
            turns: sign::BELOW,
            at: SIGNS[0],
            time: 7
        }));
        assert!(picked.contains(&Drawn::Frame {
            rect: kept[0][0].rect,
            mode: 1,
            time: 7
        }));
        // The keeper's last line has the sign of "more" and no line of the buttons.
        go(&mut screen, pad::TRIANGLE);
        go(&mut screen, pad::TRIANGLE);
        go(&mut screen, pad::TRIANGLE);
        go(&mut screen, pad::CROSS);
        assert_eq!((screen.state, legend(&screen)), (state::DONE, None));
        let last = drawn(&screen, &shown, 7, width);
        assert_eq!(last.last(), Some(&Drawn::More { at: MORE_AT }));
        assert_eq!(last[0], Drawn::WindowFrame { rect: KEEPER });
    }

    #[test]
    fn a_merge_has_its_three_frames_and_the_list_of_the_skins() {
        let mut game = Carried::new_game();
        game.creatures.put(CARRIED, record(5));
        game.creatures.put(CARRIED, record(6));
        let worn = game.worn;
        game.keeping.give(3, 2, items::Kind::Creature, worn);
        let mut screen = MergeScreen::open(SECOND_SET, &mut game);
        let skin_lines = rows_of(SKIN_ROWS, &[21, 107, 28]);
        let shown = Shown {
            main_lines: &[],
            carried_lines: &[],
            kept_lines: &[],
            skin_lines: &skin_lines,
            values: [9, 9, 9, 9, 9],
            compared_values: [9, 8, 9, 9, 9],
            attack_lines: &[],
            spell_lines: &[],
            attack_pictures: &[[0; 5]; attacks::ROWS],
            spell_pictures: &[],
            attack_rows: [AttackRow::default(); attacks::ROWS],
            family: None,
            circle: false,
        };
        let width = |_: usize| 40;
        let frame = |which: usize, time: u32| Drawn::Frame {
            rect: OF_THREE[which],
            mode: 1,
            time,
        };
        // The second pick: the first one's name in its frame, which does not pulse.
        go(&mut screen, pad::CROSS);
        go(&mut screen, pad::CROSS);
        go(&mut screen, pad::DOWN);
        assert_eq!(legend(&screen), Some(LEGEND_OF_A_SECOND));
        let second = drawn(&screen, &shown, 7, width);
        assert_eq!(
            second[..2],
            [
                frame(0, 0),
                Drawn::PlaceName {
                    place: 0,
                    middle: (122, 81),
                    marked: false
                }
            ]
        );
        assert!(!second.contains(&frame(1, 0)));
        // He has a skin: the keeper's second lines with the sign of "more", no bar,
        // and all three frames.
        go(&mut screen, pad::CROSS);
        assert_eq!(
            (screen.state, legend(&screen)),
            (state::SKINS_ASKED, Some(LEGEND_OF_MORE))
        );
        let told = drawn(&screen, &shown, 7, width);
        assert!(told.contains(&Drawn::KeeperSaid {
            line: (ROWS + crate::merge_screen::STATES + REFUSALS) as i16 + state::SKINS_ASKED,
            text: 1,
            at: (87, 40)
        }));
        assert_eq!(told.last(), Some(&Drawn::More { at: MORE_AT }));
        let bar = |d: &Drawn| matches!(d, Drawn::Frame { rect, .. } if rect.x == ANSWER_BAR.x);
        assert!(!told.iter().any(bar));
        assert!(told.contains(&frame(2, 0)) && told.contains(&frame(1, 0)));
        assert!(told.contains(&Drawn::Sprite {
            number: NEW_SPRITE,
            at: (268, 80),
            clip: OF_THREE[2]
        }));
        assert!(told.contains(&Drawn::PlaceName {
            place: 1,
            middle: (209, 81),
            marked: false
        }));
        // Her question has the bar, on the "No".
        go(&mut screen, pad::CROSS);
        assert_eq!(legend(&screen), Some(LEGEND_PLAIN));
        let asked = drawn(&screen, &shown, 7, width);
        assert!(asked.iter().any(bar));
        assert!(!asked.iter().any(|d| matches!(d, Drawn::More { .. })));
        // The list of the skins: his one skin in the first row, which pulses, and
        // three rows of frames alone.
        go(&mut screen, pad::UP);
        go(&mut screen, pad::CROSS);
        assert_eq!(screen.state, state::SKINS);
        go(&mut screen, pad::DOWN);
        go(&mut screen, pad::UP);
        let list = drawn(&screen, &shown, 7, width);
        assert_eq!(
            list[..6],
            [
                Drawn::Frame {
                    rect: Rect {
                        x: 123,
                        y: 104,
                        w: 21,
                        h: 21
                    },
                    mode: 1,
                    time: 7
                },
                Drawn::ItemPicture {
                    item: 3,
                    at: (123, 108),
                    dim: false,
                    clip: Rect {
                        x: 123,
                        y: 104,
                        w: 21,
                        h: 21
                    }
                },
                Drawn::Frame {
                    rect: Rect {
                        x: 144,
                        y: 104,
                        w: 107,
                        h: 21
                    },
                    mode: 1,
                    time: 7
                },
                Drawn::ItemName {
                    item: 3,
                    at: (144, 108),
                    dim: false,
                    clip: Rect {
                        x: 144,
                        y: 104,
                        w: 107,
                        h: 21
                    }
                },
                Drawn::Frame {
                    rect: Rect {
                        x: 251,
                        y: 104,
                        w: 28,
                        h: 21
                    },
                    mode: 1,
                    time: 7
                },
                Drawn::Count {
                    value: 2,
                    ends: (276, 110),
                    dim: false,
                    clip: Rect {
                        x: 251,
                        y: 104,
                        w: 28,
                        h: 21
                    }
                },
            ]
        );
        let pictures = |d: &&Drawn| matches!(d, Drawn::ItemPicture { .. });
        assert_eq!(list.iter().filter(pictures).count(), 1);
        assert!(!list.iter().any(|d| matches!(d, Drawn::Sign { .. })));
        // The skin is taken: the new creature beside the two, the cursor of the three
        // on it, its frame pulsing between the two signs, and its box compared with
        // the first one's values.
        go(&mut screen, pad::CROSS);
        assert_eq!(
            (screen.state, legend(&screen)),
            (state::MERGE_ASKED, Some(LEGEND_OF_THE_NEW))
        );
        let new = drawn(&screen, &shown, 7, width);
        assert_eq!(
            new[..3],
            [
                Drawn::SignOf {
                    family: BETWEEN_FAMILY,
                    turns: 0,
                    at: BETWEEN[0],
                    time: 7
                },
                Drawn::SignOf {
                    family: BETWEEN_FAMILY,
                    turns: 0,
                    at: BETWEEN[1],
                    time: 7
                },
                frame(2, 7)
            ]
        );
        assert!(new.contains(&frame(0, 0)) && new.contains(&frame(1, 0)));
        assert!(
            new.iter()
                .any(|d| matches!(d, Drawn::Tinted { colour: 1, .. }))
        );
        assert!(new.iter().any(|d| matches!(d, Drawn::Arrow { .. })));
        assert!(!new.iter().any(bar));
        // On the first of the two nothing is compared and its frame pulses.
        go(&mut screen, pad::RIGHT);
        let first = drawn(&screen, &shown, 7, width);
        assert!(first.contains(&frame(0, 7)) && first.contains(&frame(2, 0)));
        assert!(!first.iter().any(|d| matches!(d, Drawn::Tinted { .. })));
        // Her question: no cursor of the three any more.
        go(&mut screen, pad::CROSS);
        let asked = drawn(&screen, &shown, 7, width);
        assert!(asked.iter().any(bar) && asked.contains(&frame(0, 0)));
        assert!(!asked.iter().any(|d| matches!(d, Drawn::SignOf { .. })));
    }

    /// The lines of a list: a piece of each width in every row.
    fn rows_of(rows: usize, widths: &[i16]) -> Vec<Vec<Piece>> {
        let line: Vec<Piece> = (widths.iter())
            .map(|&w| Piece {
                flags: 0,
                kind: kind::PICTURE,
                value: 0,
                rect: Rect {
                    x: 0,
                    y: 0,
                    w,
                    h: 21,
                },
                margin: (0, 4),
            })
            .collect();
        vec![line; rows]
    }

    #[test]
    fn the_box_of_a_creature_has_four_pages_and_a_description_on_the_circle() {
        let mut game = Carried::new_game();
        let mut knows = record(5);
        knows.spells = 0b0110;
        game.creatures.put(CARRIED, knows);
        let mut screen = MergeScreen::open(SECOND_SET, &mut game);
        let attack_lines = rows_of(attacks::ROWS, &[38, 93, 78]);
        let spell_lines = rows_of(spells::ROWS, &[21, 107]);
        let pictures = [[9; 5]; attacks::ROWS];
        let spell_pictures = [33u8; spells::SPELLS];
        let family = Family::of_a_caught(&[65], 1);
        let mut attack_rows = [AttackRow::default(); attacks::ROWS];
        attack_rows[0] = AttackRow {
            known: true,
            attack: 3,
            grade: 2,
            class: 0,
            refused: false,
        };
        let mut shown = Shown {
            main_lines: &[],
            carried_lines: &[],
            kept_lines: &[],
            skin_lines: &[],
            values: [1, 2, 3, 4, 5],
            compared_values: [0; 5],
            attack_lines: &attack_lines,
            spell_lines: &spell_lines,
            attack_pictures: &pictures,
            spell_pictures: &spell_pictures,
            attack_rows,
            family: Some(&family),
            circle: false,
        };
        let width = |_: usize| 40;
        for _ in 0..3 {
            go(&mut screen, pad::DOWN);
        }
        go(&mut screen, pad::CROSS);
        assert_eq!(legend(&screen), Some(LEGEND_OF_THE_VIEW));
        // The square: the pages, the "Detail" first.
        go(&mut screen, pad::SQUARE);
        assert_eq!(
            (screen.state, legend(&screen)),
            (state::ABILITIES, Some(LEGEND_OF_A_PAGE))
        );
        let detail = drawn(&screen, &shown, 7, width);
        assert!(detail.contains(&Drawn::Frame {
            rect: CREATURE,
            mode: 0,
            time: 0
        }));
        assert!(detail.contains(&Drawn::KeeperSaid {
            line: ROWS as i16 + state::ABILITIES,
            text: 0,
            at: (87, 24)
        }));
        // The body attacks at (92, 94), the row under the second cursor pulsing; no
        // frame of the box and no "Detail".
        go(&mut screen, pad::SQUARE);
        assert_eq!(legend(&screen), Some(LEGEND_OF_ROWS));
        let attacks_page = drawn(&screen, &shown, 7, width);
        assert_eq!(
            attacks_page[0],
            Drawn::Frame {
                rect: Rect {
                    x: 92,
                    y: 94,
                    w: 38,
                    h: 21
                },
                mode: 1,
                time: 7
            }
        );
        assert!(attacks_page.contains(&Drawn::Listed {
            what: Listed::Attack(3),
            room: Rect {
                x: 130,
                y: 98,
                w: 93,
                h: 13
            },
            flags: 0,
            dim: false,
            clip: Rect {
                x: 130,
                y: 94,
                w: 93,
                h: 21
            }
        }));
        assert!(attacks_page.contains(&Drawn::Frame {
            rect: Rect {
                x: 223,
                y: 94 + 4 * 22,
                w: 78,
                h: 21
            },
            mode: 1,
            time: 0
        }));
        assert!(!attacks_page.iter().any(|d| matches!(
            d,
            Drawn::Frame { rect, .. } if *rect == CREATURE
        )));
        // The circle on a known entry: its text in place of the keeper's box.
        assert_eq!(described(&screen, &shown), None);
        shown.circle = true;
        assert_eq!(described(&screen, &shown), Some(Listed::Attack(3)));
        let told = drawn(&screen, &shown, 7, width);
        assert!(!told.iter().any(|d| matches!(
            d,
            Drawn::KeeperSaid { .. } | Drawn::KeeperFace { .. } | Drawn::WindowFrame { .. }
        )));
        assert_eq!(
            told[told.len() - 5..told.len() - 3],
            [
                Drawn::Frame {
                    rect: Rect {
                        x: 10,
                        y: 18,
                        ..message::BOX
                    },
                    mode: 0,
                    time: 0
                },
                Drawn::ListDescribed {
                    what: Listed::Attack(3),
                    line: 0,
                    at: (14, 23)
                }
            ]
        );
        // Not on an entry that is not known.
        go(&mut screen, pad::DOWN);
        assert_eq!(described(&screen, &shown), None);
        // The spells at (126, 109): the two it knows, then rows of frames alone.
        go(&mut screen, pad::SQUARE);
        let spells_page = drawn(&screen, &shown, 7, width);
        assert_eq!(
            spells_page[0],
            Drawn::Frame {
                rect: Rect {
                    x: 126,
                    y: 109,
                    w: 21,
                    h: 21
                },
                mode: 1,
                time: 7
            }
        );
        let named: Vec<Listed> = (spells_page.iter())
            .filter_map(|d| match d {
                Drawn::Listed { what, .. } => Some(*what),
                _ => None,
            })
            .collect();
        assert_eq!(
            named,
            [
                Listed::Sprite(33),
                Listed::Spell(1),
                Listed::Sprite(33),
                Listed::Spell(2)
            ]
        );
        assert_eq!(described(&screen, &shown), Some(Listed::Spell(1)));
        go(&mut screen, pad::DOWN);
        go(&mut screen, pad::DOWN);
        assert_eq!(described(&screen, &shown), None);
        // The "Family", every place of it moved by (40, 30); nothing to describe.
        go(&mut screen, pad::SQUARE);
        assert_eq!(legend(&screen), Some(LEGEND_OF_A_PAGE));
        let tree = drawn(&screen, &shown, 7, width);
        assert!(tree.contains(&Drawn::Frame {
            rect: Rect {
                x: 80,
                y: 137,
                w: 64,
                h: 15
            },
            mode: 0,
            time: 0
        }));
        assert!(tree.contains(&Drawn::AncestorName {
            creature: 0,
            ancestor: 5,
            at: (237, 186)
        }));
        assert!(tree.contains(&Drawn::Line {
            from: (144, 144),
            to: (150, 144),
            grey: 128,
            clip: Rect {
                x: 0,
                y: 0,
                w: 320,
                h: 240
            }
        }));
        assert_eq!(described(&screen, &shown), None);
        // A cancel: the view again, with the "Detail" at once.
        go(&mut screen, pad::TRIANGLE);
        assert_eq!((screen.state, screen.drawn_page()), (state::VIEW, 0));
    }

    #[test]
    fn a_form_that_is_looked_at_leaves_the_box_its_frame_alone() {
        let mut game = Carried::new_game();
        game.creatures.put(CARRIED, record(5));
        let mut screen = MergeScreen::open(SECOND_SET, &mut game);
        let pictures = [[9; 5]; attacks::ROWS];
        let spell_pictures = [33u8; spells::SPELLS];
        let shown = Shown {
            main_lines: &[],
            carried_lines: &[],
            kept_lines: &[],
            skin_lines: &[],
            values: [1, 2, 3, 4, 5],
            compared_values: [0; 5],
            attack_lines: &[],
            spell_lines: &[],
            attack_pictures: &pictures,
            spell_pictures: &spell_pictures,
            attack_rows: [AttackRow::default(); attacks::ROWS],
            family: None,
            circle: false,
        };
        let width = |_: usize| 40;
        for _ in 0..3 {
            go(&mut screen, pad::DOWN);
        }
        go(&mut screen, pad::CROSS);
        let frame = Drawn::Frame {
            rect: CREATURE,
            mode: 0,
            time: 0,
        };
        let frames = |screen: &MergeScreen| {
            let drawn = drawn(screen, &shown, 7, width);
            let own = (drawn.iter())
                .filter(|part| matches!(part, Drawn::Frame { rect, .. } if *rect == CREATURE));
            (drawn.contains(&frame), own.count(), drawn.len())
        };
        let (_, _, with_the_detail) = frames(&screen);
        // The circle: the line of the buttons at once, the keeper's lines a pass later.
        go(&mut screen, pad::CIRCLE);
        assert_eq!(legend(&screen), Some(LEGEND_OF_THE_FORM));
        assert_eq!(screen.said, Said::Form);
        let (has, own, parts) = frames(&screen);
        assert!(has && own == 1 && parts < with_the_detail);
        assert!(
            drawn(&screen, &shown, 7, width).contains(&Drawn::KeeperSaid {
                line: line_of(Said::Form),
                text: 0,
                at: (87, 24)
            })
        );
        go(&mut screen, pad::TRIANGLE);
        assert_eq!(legend(&screen), Some(LEGEND_OF_THE_VIEW));
        assert_eq!(frames(&screen).2, with_the_detail);
    }

    #[test]
    fn the_keeper_has_a_line_for_every_row_state_and_refusal() {
        assert_eq!(line_of(Said::Form), 42);
        assert_eq!(line_of(Said::Row(4)), 4);
        assert_eq!(line_of(Said::State(state::DONE)), 16);
        assert_eq!(line_of(Said::Refusal(1)), 22);
    }
}
