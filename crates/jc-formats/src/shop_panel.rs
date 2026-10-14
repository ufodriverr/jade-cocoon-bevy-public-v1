
use crate::carried::Carried;
use crate::equip_panel::colour;
use crate::items::{FREE, Kind};
use crate::menu::{Piece, Rect, Style, kind, piece, placed, sign};
use crate::player_menu::OPEN;
use crate::player_panel::{Drawn, five_numbers, four_lines, message, text_at};
use crate::rooms::Image;
use crate::shop_screen::{PUT_ON, PUT_ON_DONE, PUT_ON_LATER, QUESTION, ShopScreen, Wares};

/// The lists of the lines of the first menu and of the three kinds in the screen's
/// file, and the flag word of the menu record both are drawn with (`0x800BAFC0`): the
/// frame under the cursor pulses. The record's openness is 8, so nothing opens.
pub const FIRST_LINES: u32 = 0x800B_ECCC;
pub const KIND_LINES: u32 = 0x800B_ED84;
const MENU_FLAGS: u32 = 0x02;
/// The list of the four lines a list is drawn with, a row each, in the screen's file:
/// a small picture, a name and a number. The drawer writes the content of their pieces
/// and three of their flags in every pass.
pub const LIST_LINES: u32 = 0x800B_EC10;
pub const LIST_ROWS: usize = 4;

/// The bits of the panels (`+8` of the task's record) that are in code.
pub mod panel {
    pub const FIRST_OF_SHOP: u32 = 1 << 0;
    pub const HAS_OF_SHOP: u32 = 1 << 1;
    pub const MONEY_LEFT: u32 = 1 << 2;
    pub const MONEY_RIGHT: u32 = 1 << 3;
    pub const OFFER: u32 = 1 << 4;
    pub const ITEMS: u32 = 1 << 5;
    pub const FIRST_OF_BLACKSMITH: u32 = 1 << 6;
    pub const HAS_OF_BLACKSMITH: u32 = 1 << 7;
    pub const KINDS: u32 = 1 << 9;
    pub const OFFER_OF_KIND: u32 = 1 << 10;
    pub const LIST_OF_KIND: u32 = 1 << 11;
    /// The amount, to buy and to sell: at the blacksmith's and at the shop.
    pub const AMOUNT_OF_KIND: [u32; 2] = [1 << 13, 1 << 14];
    pub const AMOUNT: [u32; 2] = [1 << 17, 1 << 18];
    pub const SUM: u32 = 1 << 25;
}

/// The panels of the amount by their numbers, to buy and to sell, as its drawer
/// `0x800BC27C` is handed them: the blacksmith's (`0x800BD344`) and the shop's
/// (`0x800BCDE8`).
pub mod amount {
    pub const OF_KIND: [i16; 2] = [13, 14];
    pub const OF_SHOP: [i16; 2] = [17, 18];
}

/// The four panels of a list by their numbers, as the drawer `0x800BC544` is handed
/// them: the shop's offer and the player's items, the blacksmith's offer of a kind and
/// the player's list of it.
pub mod list {
    pub const OFFER: i16 = 4;
    pub const ITEMS: i16 = 5;
    pub const OFFER_OF_KIND: i16 = 10;
    pub const LIST_OF_KIND: i16 = 11;
}

/// The sprites of the menus' pool the screen draws.
pub mod sprite {
    /// The word in the box of the money and the sign behind its number.
    pub const MONEY: usize = 122;
    pub const UNIT: usize = 234;
    /// The word in the box of how many the player has.
    pub const HAS: usize = 147;
    /// The word in the box under the numbers of a list.
    pub const UNDER_NUMBERS: usize = 128;
    /// The heading of the shop's lists, and of the blacksmith's by the kind (the table
    /// at `0x800BEA44`): the words of the three kinds.
    pub const HEADING_OF_SHOP: usize = 88;
    pub const HEADING_OF_KIND: [usize; 3] = [22, 20, 3];
    /// The word of the mark of what he has on.
    pub const WORN: usize = 182;
    /// The words in the box of the amount and in the box of the sum.
    pub const AMOUNT: usize = 83;
    pub const SUM: usize = 206;
}

/// The whole screen, for what is cut to nothing.
const SCREEN: Rect = Rect {
    x: 0,
    y: 0,
    w: 320,
    h: 240,
};

/// The box of the money (`0x800BC0A0`): its size, and its corner for the panels 2 and 3
/// (the places at `0x800BED94` and `0x800BED98`).
pub const MONEY_SIZE: (i16, i16) = (121, 16);
pub const MONEY_AT: [(i16, i16); 2] = [(10, 77), (189, 77)];
/// Where the word, the end of the number and the sign stand from the box's corner.
const MONEY_WORD: (i16, i16) = (4, 3);
const MONEY_ENDS: (i16, i16) = (96, 6);
const MONEY_UNIT: (i16, i16) = (97, 4);

/// The keeper's box (the rectangle at `0x800BE994`) and where its three texts start
/// (the list at `0x800BE304`).
pub const KEEPER: Rect = Rect {
    x: 80,
    y: 20,
    w: 223,
    h: 54,
};
pub const KEEPER_TEXTS: [(i16, i16); 3] = [(87, 24), (87, 40), (87, 56)];
/// The box of the keeper's face and the face in it (the executable's `0x8002A54C` with
/// the rectangle at `0x80082708` and the place at `0x80082710`).
pub const FACE_BOX: Rect = Rect {
    x: 10,
    y: 9,
    w: 70,
    h: 66,
};
pub const FACE_AT: (i16, i16) = (13, 9);
/// The bar of the answer under the cursor (`0x800BC3E0`): on the second or the third
/// text.
const ANSWER: Rect = Rect {
    x: 86,
    y: 37,
    w: 30,
    h: 16,
};
const ANSWER_STEP: i16 = 16;

/// The lines a keeper has, and the tables of them in the screen's file by the argument
/// of the script's command (`0x800BD0A4`): for argument 2, for argument 3, for any
/// other. An entry is the address of a record of three texts.
pub const KEEPER_LINES: usize = 23;
const KEEPER_OF_2: u32 = 0x800B_E5A8;
const KEEPER_OF_3: u32 = 0x800B_E604;
const KEEPER_OF_OTHERS: u32 = 0x800B_E54C;

/// The executable's lists of the line of help (`0x800BD150`): on the four lists, where
/// the circle shows a description, and in every other state.
pub const HELP_OF_LISTS: u32 = 0x8008_25D4;
pub const HELP_PLAIN: u32 = 0x8008_25A4;

/// The list of help of the state the screen is in.
pub fn help(screen: &ShopScreen) -> u32 {
    match screen.state {
        2..=5 => HELP_OF_LISTS,
        _ => HELP_PLAIN,
    }
}

/// The box of how many the player has (`0x800BC1A4`), where its word stands and where
/// its number ends.
pub const HAS_BOX: Rect = Rect {
    x: 119,
    y: 77,
    w: 69,
    h: 16,
};
const HAS_WORD: (i16, i16) = (123, 80);
const HAS_ENDS: (i16, i16) = (184, 82);
/// Round a list (`0x800BC544`): the box under the numbers with its word, the scroll
/// signs above and below, and the heading's box, from whose corner the word of the
/// shop's lists and the word of a kind stand apart.
pub const UNDER_NUMBERS: Rect = Rect {
    x: 138,
    y: 197,
    w: 55,
    h: 16,
};
const UNDER_NUMBERS_WORD: (i16, i16) = (155, 200);
pub const LIST_SIGNS: [(i16, i16); 2] = [(79, 104), (79, 201)];
pub const HEADING: Rect = Rect {
    x: 10,
    y: 77,
    w: 47,
    h: 18,
};
const HEADING_OF_SHOP: (i16, i16) = (19, 81);
const HEADING_OF_KIND: (i16, i16) = (14, 80);
/// The mark of what he has on (`0x800BCC8C`): its frame, which pulses, and its word.
pub const WORN_MARK: Rect = Rect {
    x: 80,
    y: 77,
    w: 38,
    h: 16,
};
const WORN_WORD: (i16, i16) = (84, 80);
/// The box of the amount (`0x800BC27C`), where its word stands and where its number
/// ends.
pub const AMOUNT_BOX: Rect = Rect {
    x: 119,
    y: 94,
    w: 69,
    h: 16,
};
const AMOUNT_WORD: (i16, i16) = (123, 97);
const AMOUNT_ENDS: (i16, i16) = (184, 99);
/// The box of the sum (`0x800BD19C`), where its word stands, where its number ends and
/// where the sign behind the number stands.
pub const SUM_BOX: Rect = Rect {
    x: 189,
    y: 94,
    w: 121,
    h: 16,
};
const SUM_WORD: (i16, i16) = (193, 96);
const SUM_ENDS: (i16, i16) = (285, 100);
const SUM_UNIT: (i16, i16) = (286, 98);
/// The jump table of the sum's routine in the screen's file, an entry for each of the
/// panels from 12 on that can have the cursor: which price counts.
pub const SUM_PRICES: u32 = 0x800B_AFDC;
pub const SUM_PANELS: usize = 8;
/// The box of a thing's description (`0x800BBB20` through `0x8002AF70`): the box of a
/// message with its corner where the keeper's face has its box.
pub const DESCRIBED: Rect = Rect {
    x: 10,
    y: 18,
    ..message::BOX
};
/// The tables of the descriptions in the screen's file, the addresses of four lines for
/// each thing, and how many things each has; both by the list of [`Kind`].
pub const TEXTS: [u32; 4] = [0x800B_ED9C, 0x800B_F04C, 0x800B_F1BC, 0x800B_F2AC];
pub const THINGS: [usize; 4] = [43, 23, 15, 17];
/// Right of the blacksmith's offer (`0x800BCEB8`): the box of the player's five numbers
/// and the box of what they would become, the sign between the two, and where the
/// numbers of what would be end, the first row's y and the step of a row (`0x8002E1E4`
/// with the place (281, 115) and the step 14).
pub const NUMBERS: Rect = Rect {
    x: 194,
    y: 112,
    w: 75,
    h: 77,
};
pub const WOULD: Rect = Rect {
    x: 281,
    y: 112,
    w: 29,
    h: 77,
};
pub const ARROW: (i16, i16) = (274, 149);
pub const WOULD_ENDS: (i16, i16) = (303, 117);
pub const WOULD_STEP: i16 = 14;

/// What the drawing takes from the game.
#[derive(Debug, Clone, Copy)]
pub struct Shown<'a> {
    /// The lines of the first menu ([`FIRST_LINES`]), of the kinds ([`KIND_LINES`]) and
    /// of a list ([`LIST_LINES`]).
    pub first_lines: &'a [Vec<Piece>],
    pub kind_lines: &'a [Vec<Piece>],
    pub list_lines: &'a [Vec<Piece>],
    /// What is for sale, and the state of the game the screen deals on.
    pub wares: &'a Wares,
    pub carried: &'a Carried,
    /// Whether the circle is held in the pass (`0x8003C218`).
    pub circle: bool,
    /// The first five of the six values a fight counts with, as they are, and as they
    /// would be with the thing of [`weighed`] on.
    pub values: [u8; 5],
    pub would: [u8; 5],
}

/// A row of a list: the thing, and the number beside it, which is the price in an
/// offer and what the thing brings in a list of the player's. In an offer a row whose
/// price is more than the money is dim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListRow {
    pub thing: u8,
    pub number: i32,
    pub dim: bool,
}

/// The list of the player's a panel of a list deals in, and whether the panel is an
/// offer. `kind` is the kind picked at the blacksmith's.
fn listed(panel: i16, kind: i16) -> Option<(Kind, bool)> {
    let of_kind = match kind {
        0 => Kind::Weapon,
        1 => Kind::Armour,
        _ => Kind::Ornament,
    };
    match panel {
        list::OFFER => Some((Kind::Item, true)),
        list::ITEMS => Some((Kind::Item, false)),
        list::OFFER_OF_KIND => Some((of_kind, true)),
        list::LIST_OF_KIND => Some((of_kind, false)),
        _ => None,
    }
}

/// The cursor and the first shown line of a list's panel (`0x800BE228`, `0x800BE1DC`):
/// the cursor record's while the panel has the cursor, the kept ones behind.
fn list_place(screen: &ShopScreen, panel: i16) -> (i16, i16) {
    if screen.panel == panel {
        (screen.cursor.at, screen.cursor.first)
    } else {
        screen.kept
    }
}

/// The thing at a place of a list's panel: of the offer as far as it goes, of the
/// player's list at any place; [`FREE`] for none.
fn thing_at(screen: &ShopScreen, shown: &Shown, panel: i16, place: i32) -> u8 {
    let Some((of, offer)) = listed(panel, screen.kind) else {
        return FREE;
    };
    if !offer {
        let place = usize::try_from(place).unwrap_or(usize::MAX);
        return shown.carried.keeping.number_at(place, of);
    }
    let on_offer = i32::from(shown.wares.offers(of, screen.stock));
    match i16::try_from(place) {
        Ok(n) if place < on_offer => shown.wares.nth(n, of, screen.stock),
        _ => FREE,
    }
}

/// The four rows of a list's panel from its first shown line on (the loop of
/// `0x800BC544`); `None` for a row past the offer and for an empty place.
pub fn list_rows(screen: &ShopScreen, shown: &Shown, panel: i16) -> [Option<ListRow>; LIST_ROWS] {
    let (_, first) = list_place(screen, panel);
    let money = shown.carried.keeping.money();
    std::array::from_fn(|row| {
        let (of, offer) = listed(panel, screen.kind)?;
        let thing = thing_at(screen, shown, panel, i32::from(first) + row as i32);
        if thing == FREE {
            return None;
        }
        let number = match offer {
            true => shown.wares.price(thing, of),
            false => shown.wares.worth(thing, of),
        };
        Some(ListRow {
            thing,
            number,
            dim: offer && money < number,
        })
    })
}

/// How many the player has of the thing under the cursor of the list that is up
/// (`0x800BC4B4`, `0x800BCE1C`): of an offer the count of that thing in his list, of
/// his own list the count at the place.
pub fn has(screen: &ShopScreen, shown: &Shown) -> i32 {
    let of = screen.list();
    let offer = match of {
        Kind::Item => list::OFFER,
        _ => list::OFFER_OF_KIND,
    };
    // The panel of the player's list is the one behind the offer's.
    let panel = offer + i16::from(screen.sells != 0);
    let (cursor, first) = list_place(screen, panel);
    let place = i32::from(cursor) + i32::from(first);
    let keeping = &shown.carried.keeping;
    match screen.sells {
        0 => {
            // Past the offer the game reads on behind its table; the build has no thing.
            let n = i16::try_from(place).unwrap_or(i16::MAX);
            let thing = shown.wares.nth(n, of, screen.stock);
            keeping.count(thing.into(), of).into()
        }
        1 => {
            let place = usize::try_from(place).unwrap_or(usize::MAX);
            keeping.count_at(place, of).into()
        }
        _ => 0,
    }
}

/// The box of how many the player has (`0x800BC1A4`), from what lies underneath to
/// what lies on top: the plain frame, the number and the word.
fn has_box(count: i32) -> [Drawn; 3] {
    [
        Drawn::Frame {
            rect: HAS_BOX,
            mode: 0,
            time: 0,
        },
        Drawn::LongNumber {
            value: count,
            ends: HAS_ENDS,
            clip: SCREEN,
        },
        Drawn::Sprite {
            number: sprite::HAS,
            at: HAS_WORD,
            clip: SCREEN,
        },
    ]
}

/// The box of the amount (`0x800BC27C`), from what lies underneath to what lies on top:
/// the frame, the amount and the word. The frame pulses while the panel of the amount
/// has the cursor and has the marked colour behind, under the question. `panels` are
/// the panel to buy and the panel to sell of the routine that asks.
fn amount_box(screen: &ShopScreen, panels: [i16; 2], time: u32) -> [Drawn; 3] {
    let lit = screen.panel == panels[usize::from(screen.sells == 1)];
    [
        Drawn::Frame {
            rect: AMOUNT_BOX,
            mode: if lit { 1 } else { 2 },
            time: if lit { time } else { 0 },
        },
        Drawn::LongNumber {
            value: screen.amount,
            ends: AMOUNT_ENDS,
            clip: SCREEN,
        },
        Drawn::Sprite {
            number: sprite::AMOUNT,
            at: AMOUNT_WORD,
            clip: SCREEN,
        },
    ]
}

/// What the amount comes to (`0x800BD19C`): the price of the thing, or three quarters
/// of it when the panel with the cursor is one to sell, times the amount. A panel
/// outside the states of the amount and of the question counts a price of 0.
pub fn sum(screen: &ShopScreen, shown: &Shown) -> i32 {
    let of = screen.list();
    let each = match screen.panel {
        13 | 15 | 17 | 19 => shown.wares.price(screen.thing, of),
        12 | 14 | 16 | 18 => shown.wares.worth(screen.thing, of),
        _ => 0,
    };
    each.wrapping_mul(screen.amount)
}

/// The box of the sum (`0x800BD19C`), from what lies underneath to what lies on top:
/// the plain frame, the sign behind the number, the number and the word. Nothing
/// without a thing.
fn sum_box(screen: &ShopScreen, shown: &Shown) -> Vec<Drawn> {
    if screen.thing == FREE {
        return Vec::new();
    }
    vec![
        Drawn::Frame {
            rect: SUM_BOX,
            mode: 0,
            time: 0,
        },
        Drawn::Sprite {
            number: sprite::UNIT,
            at: SUM_UNIT,
            clip: SCREEN,
        },
        Drawn::LongNumber {
            value: sum(screen, shown),
            ends: SUM_ENDS,
            clip: SCREEN,
        },
        Drawn::Sprite {
            number: sprite::SUM,
            at: SUM_WORD,
            clip: SCREEN,
        },
    ]
}

/// A list with what stands round it (`0x800BC544`), from what lies underneath to what
/// lies on top: the heading's box and its word; at the blacksmith's, outside the
/// states of the putting on, the mark when the thing under the list's cursor is the one
/// the player has on; while the panel has the cursor the scroll signs; the box under
/// the numbers and its word; then four rows through the shared menu drawer, each a
/// picture, a name and a number in a frame of their own. While the panel has the cursor
/// the frames under it pulse; behind, the row of the kept place has the marked colour.
/// A row without a thing has its frames alone.
fn list(screen: &ShopScreen, shown: &Shown, panel: i16, time: u32) -> Vec<Drawn> {
    let Some((of, _)) = listed(panel, screen.kind) else {
        return Vec::new();
    };
    let gear = (of as usize).checked_sub(Kind::Weapon as usize);
    let has_cursor = screen.panel == panel;
    let (cursor, first) = list_place(screen, panel);
    let (word, word_at) = match gear {
        None => (sprite::HEADING_OF_SHOP, HEADING_OF_SHOP),
        Some(kind) => (sprite::HEADING_OF_KIND[kind.min(2)], HEADING_OF_KIND),
    };
    let mut drawn = vec![
        Drawn::Frame {
            rect: HEADING,
            mode: 0,
            time: 0,
        },
        Drawn::Sprite {
            number: word,
            at: word_at,
            clip: SCREEN,
        },
    ];
    if let Some(kind) = gear.filter(|_| !(PUT_ON..=PUT_ON_LATER).contains(&screen.state)) {
        let place = i32::from(cursor) + i32::from(first);
        let thing = thing_at(screen, shown, panel, place);
        if thing != FREE && shown.carried.worn.get(kind) == Some(&thing) {
            drawn.extend([
                Drawn::Frame {
                    rect: WORN_MARK,
                    mode: 1,
                    time,
                },
                Drawn::Sprite {
                    number: sprite::WORN,
                    at: WORN_WORD,
                    clip: SCREEN,
                },
            ]);
        }
    }
    if has_cursor {
        // The lower sign: with four lines or more, unless the last of them is shown.
        let lines = i32::from(screen.cursor.lines);
        if lines >= LIST_ROWS as i32 && i32::from(first) != lines - LIST_ROWS as i32 {
            drawn.push(Drawn::Sign {
                turns: sign::BELOW,
                at: LIST_SIGNS[1],
                time,
            });
        }
        if first != 0 {
            drawn.push(Drawn::Sign {
                turns: sign::ABOVE,
                at: LIST_SIGNS[0],
                time,
            });
        }
    }
    drawn.extend([
        Drawn::Frame {
            rect: UNDER_NUMBERS,
            mode: 0,
            time: 0,
        },
        Drawn::Sprite {
            number: sprite::UNDER_NUMBERS,
            at: UNDER_NUMBERS_WORD,
            clip: SCREEN,
        },
    ]);
    let flags = if has_cursor { 0x02 } else { 0x04 };
    let rows = list_rows(screen, shown, panel);
    for (n, (pieces, row)) in shown.list_lines.iter().zip(rows).enumerate() {
        let (style, pulses) = Style::of_line(flags, i32::from(cursor) == n as i32, 0);
        for (k, piece) in pieces.iter().enumerate() {
            let clip = piece.rect;
            if piece.flags & piece::BARE == 0 {
                drawn.push(Drawn::Frame {
                    rect: clip,
                    mode: style.mode,
                    time: if pulses { time } else { 0 },
                });
            }
            let Some(ListRow { thing, number, dim }) = row else {
                continue;
            };
            let room = clip.inside(piece.margin);
            let at = (room.x, room.y);
            drawn.push(match (k, gear) {
                (0, None) => Drawn::ItemPicture {
                    item: thing,
                    at,
                    dim,
                    clip,
                },
                (0, Some(kind)) => Drawn::GearPicture {
                    kind,
                    thing,
                    at,
                    dim,
                    clip,
                },
                (1, None) => Drawn::ItemName {
                    item: thing,
                    at,
                    dim,
                    clip,
                },
                (1, Some(kind)) => Drawn::GearName {
                    kind,
                    thing,
                    at,
                    dim,
                    clip,
                },
                // A number ends 3 left of its room's right edge, 2 below its upper one.
                _ => Drawn::Count {
                    value: number,
                    ends: (room.x + room.w - 3, room.y + 2),
                    dim,
                    clip,
                },
            });
        }
    }
    drawn
}

/// A menu of three framed words through the shared menu drawer (`0x800BC364`,
/// `0x800BC438`): the frame under the cursor pulses, the others are plain.
fn menu(lines: &[Vec<Piece>], cursor: i16, time: u32, width: &impl Fn(usize) -> i16) -> Vec<Drawn> {
    let mut drawn = Vec::new();
    for (line, pieces) in lines.iter().enumerate() {
        let on_cursor = usize::try_from(cursor) == Ok(line);
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

/// The box of the money (`0x800BC0A0`), from what lies underneath to what lies on top:
/// the plain frame, the sign behind the number, the number and the word.
fn money(at: (i16, i16), money: i32) -> [Drawn; 4] {
    let from = |by: (i16, i16)| (at.0 + by.0, at.1 + by.1);
    [
        Drawn::Frame {
            rect: Rect {
                x: at.0,
                y: at.1,
                w: MONEY_SIZE.0,
                h: MONEY_SIZE.1,
            },
            mode: 0,
            time: 0,
        },
        Drawn::Sprite {
            number: sprite::UNIT,
            at: from(MONEY_UNIT),
            clip: SCREEN,
        },
        Drawn::LongNumber {
            value: money,
            ends: from(MONEY_ENDS),
            clip: SCREEN,
        },
        Drawn::Sprite {
            number: sprite::MONEY,
            at: from(MONEY_WORD),
            clip: SCREEN,
        },
    ]
}

/// The keeper's box (`0x800BBA9C`), from what lies underneath to what lies on top: in
/// the states of a question the bar of the answer under the cursor, which pulses; the
/// frame and over it the fill; the three texts of the keeper's line; then the box of
/// the face, its fill under the face and its frame over it.
fn keeper(screen: &ShopScreen, time: u32) -> Vec<Drawn> {
    let mut drawn = Vec::new();
    if (QUESTION..=PUT_ON).contains(&screen.state) {
        drawn.push(Drawn::Frame {
            rect: Rect {
                y: ANSWER.y + ANSWER_STEP * screen.cursor.at,
                ..ANSWER
            },
            mode: 1,
            time,
        });
    }
    drawn.push(Drawn::WindowFrame { rect: KEEPER });
    drawn.push(Drawn::WindowFill { rect: KEEPER });
    for (text, at) in KEEPER_TEXTS.into_iter().enumerate() {
        drawn.push(Drawn::KeeperSaid {
            line: screen.line,
            text,
            at,
        });
    }
    drawn.extend([
        Drawn::WindowFill { rect: FACE_BOX },
        Drawn::KeeperFace { at: FACE_AT },
        Drawn::WindowFrame { rect: FACE_BOX },
    ]);
    drawn
}

/// The thing whose change of the player's numbers the pass shows (`0x800BCEB8`), with
/// its kind: the thing of the blacksmith's offer under the list's cursor, the kept
/// place behind. None past the offer, and none while the state or the state asked for
/// is the one behind the putting on.
pub fn weighed(screen: &ShopScreen, wares: &Wares) -> Option<(usize, u8)> {
    if screen.state == PUT_ON_DONE || screen.asked == PUT_ON_DONE {
        return None;
    }
    let kind = usize::try_from(screen.kind).ok().filter(|kind| *kind < 3)?;
    let (of, _) = listed(list::OFFER_OF_KIND, screen.kind)?;
    let (cursor, first) = list_place(screen, list::OFFER_OF_KIND);
    let place = i16::try_from(i32::from(cursor) + i32::from(first)).ok()?;
    let thing = wares.nth(place, of, screen.stock);
    (thing != FREE).then_some((kind, thing))
}

/// What a thing would change (`0x800BCEB8`), from what lies underneath to what lies on
/// top: outside the state behind the putting on the box of what the numbers would
/// become and the sign between the two boxes; the box of the five numbers with them;
/// and for the thing of [`weighed`] the new numbers, each under the colour of its
/// change.
fn change(screen: &ShopScreen, shown: &Shown, time: u32) -> Vec<Drawn> {
    let mut drawn = Vec::new();
    if screen.state != PUT_ON_DONE {
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
    }
    drawn.extend(five_numbers(NUMBERS, shown.values, OPEN.into()));
    if weighed(screen, shown.wares).is_some() {
        for (row, (now, then)) in (0i16..).zip(shown.values.into_iter().zip(shown.would)) {
            drawn.push(Drawn::Tinted {
                value: then.into(),
                ends: (WOULD_ENDS.0, WOULD_ENDS.1 + WOULD_STEP * row),
                colour: match then.cmp(&now) {
                    std::cmp::Ordering::Greater => colour::RISES,
                    std::cmp::Ordering::Less => colour::FALLS,
                    std::cmp::Ordering::Equal => colour::STAYS,
                },
                clip: SCREEN,
            });
        }
    }
    drawn
}

/// The thing whose description the pass shows (`0x800BBB20`): on the four lists, while
/// the circle is held, the thing under the cursor, with the list of the player's it is
/// of; none for a row past the offer and for an empty place.
pub fn described(screen: &ShopScreen, shown: &Shown) -> Option<(Kind, u8)> {
    if !shown.circle || !(2..=5).contains(&screen.state) {
        return None;
    }
    let (of, _) = listed(screen.panel, screen.kind)?;
    let place = i32::from(screen.cursor.at) + i32::from(screen.cursor.first);
    let thing = thing_at(screen, shown, screen.panel, place);
    (thing != FREE).then_some((of, thing))
}

/// The description of a thing (`0x8002AF70` at the place (10, 18)), from what lies
/// underneath to what lies on top: the box of a message and its four lines.
fn description(of: Kind, thing: u8) -> Vec<Drawn> {
    let mut drawn = vec![Drawn::Frame {
        rect: DESCRIBED,
        mode: 0,
        time: 0,
    }];
    let gear = (of as usize).checked_sub(Kind::Weapon as usize);
    drawn.extend((0..message::LINES).map(|line| {
        let at = (
            DESCRIBED.x + message::TEXT.0,
            DESCRIBED.y + message::TEXT.1 + message::LINE * line as i16,
        );
        match gear {
            None => Drawn::Described {
                item: thing,
                line,
                at,
            },
            Some(kind) => Drawn::GearDescribed {
                kind,
                thing,
                line,
                at,
            },
        }
    }));
    drawn
}

/// The four lines of a thing's description from the screen's file, whose first byte is
/// at `base`; `list` is the list of [`Kind`] the thing is of.
pub fn description_text(
    file: &[u8],
    base: u32,
    list: usize,
    thing: usize,
) -> Option<[Vec<u16>; message::LINES]> {
    if thing >= *THINGS.get(list)? {
        return None;
    }
    four_lines(file, base, TEXTS[list], thing)
}

/// What the screen draws in a pass (`0x800BBB20`), from what lies underneath to what
/// lies on top: the keeper's box, which is added last, or in its place the description
/// of the thing under the cursor while the circle is held on a list; then the panels
/// asked for from the last that is added to the first. The line of help ([`help`]) and the wall are
/// the caller's. `time` is the menus' time and `width` gives the width of a sprite of
/// the pool.
pub fn drawn(
    screen: &ShopScreen,
    shown: &Shown,
    time: u32,
    width: impl Fn(usize) -> i16,
) -> Vec<Drawn> {
    let asked = |panel: u32| screen.shown & panel != 0;
    let has_money = shown.carried.keeping.money();
    let mut drawn = match described(screen, shown) {
        Some((of, thing)) => description(of, thing),
        None => keeper(screen, time),
    };
    if asked(panel::SUM) {
        drawn.extend(sum_box(screen, shown));
    }
    if asked(panel::KINDS) {
        drawn.extend(menu(shown.kind_lines, screen.cursor.at, time, &width));
    }
    // Each of the two routines of the amount runs once for each of its bits.
    for bit in panel::AMOUNT.into_iter().rev() {
        if asked(bit) {
            drawn.extend(amount_box(screen, amount::OF_SHOP, time));
        }
    }
    for bit in panel::AMOUNT_OF_KIND.into_iter().rev() {
        if asked(bit) {
            drawn.extend(amount_box(screen, amount::OF_KIND, time));
        }
    }
    if asked(panel::FIRST_OF_BLACKSMITH) {
        drawn.extend(menu(shown.first_lines, screen.cursor.at, time, &width));
    }
    if asked(panel::LIST_OF_KIND) {
        drawn.extend(list(screen, shown, list::LIST_OF_KIND, time));
    }
    if asked(panel::OFFER_OF_KIND) {
        drawn.extend(list(screen, shown, list::OFFER_OF_KIND, time));
        drawn.extend(change(screen, shown, time));
    }
    if asked(panel::HAS_OF_BLACKSMITH) {
        drawn.extend(has_box(has(screen, shown)));
    }
    if asked(panel::ITEMS) {
        drawn.extend(list(screen, shown, list::ITEMS, time));
    }
    if asked(panel::OFFER) {
        drawn.extend(list(screen, shown, list::OFFER, time));
    }
    if asked(panel::MONEY_RIGHT) {
        drawn.extend(money(MONEY_AT[1], has_money));
    }
    if asked(panel::MONEY_LEFT) {
        drawn.extend(money(MONEY_AT[0], has_money));
    }
    if asked(panel::HAS_OF_SHOP) {
        drawn.extend(has_box(has(screen, shown)));
    }
    if asked(panel::FIRST_OF_SHOP) {
        drawn.extend(menu(shown.first_lines, screen.cursor.at, time, &width));
    }
    drawn
}

/// The three texts of every line of the keeper a script's argument picks, from the
/// screen's file, whose first byte is at `base`.
pub fn keeper_texts(file: &[u8], base: u32, argument: i16) -> Option<Vec<[Vec<u16>; 3]>> {
    let table = match argument {
        2 => KEEPER_OF_2,
        3 => KEEPER_OF_3,
        _ => KEEPER_OF_OTHERS,
    };
    let image = Image { data: file, base };
    (0..KEEPER_LINES)
        .map(|line| {
            let record = image.u32(crate::rooms::step(table, line, 4).ok()?).ok()?;
            let mut texts: [Vec<u16>; 3] = Default::default();
            for (n, text) in texts.iter_mut().enumerate() {
                let address = image.u32(crate::rooms::step(record, n, 4).ok()?).ok()?;
                *text = text_at(file, base, address)?;
            }
            Some(texts)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shop_screen::{
        AMOUNT, BLACKSMITH, ITEMS, KINDS_TO_BUY, OFFER, PANEL_OF, PUT_ON_DONE, SHOP, SHOWN,
    };

    /// The first stock: the items 0, 9 and 14 and the weapons 0, 1 and 3, with a price.
    fn wares() -> Wares {
        let mut wares = Wares::default();
        for (list, things) in [
            (0, [(0, 15), (9, 50), (14, 10)]),
            (1, [(0, 50), (1, 100), (3, 500)]),
        ] {
            wares.offered[list] = vec![0; 16];
            wares.prices[list] = vec![0; 16];
            for (thing, price) in things {
                wares.offered[list][thing] = 1;
                wares.prices[list][thing] = price;
            }
        }
        wares
    }

    /// A game whose player has this much money.
    fn carried(money: i32) -> Carried {
        let mut carried = Carried::new_game();
        let has = carried.keeping.money();
        carried.keeping.take_money(has);
        carried.keeping.give_money(money);
        carried
    }

    /// The four rows of a list as the screen's file has them.
    fn rows() -> Vec<Vec<Piece>> {
        (0..LIST_ROWS as i16)
            .map(|row| {
                let piece = |flags, kind, x, w, margin| Piece {
                    flags,
                    kind,
                    value: 0,
                    rect: Rect {
                        x,
                        y: 112 + 21 * row,
                        w,
                        h: 21,
                    },
                    margin,
                };
                vec![
                    piece(0x10, kind::PICTURE, 10, 21, (3, 3)),
                    piece(0x10, kind::TEXT, 31, 111, (4, 4)),
                    piece(piece::RIGHT, 8, 138, 51, (3, 4)),
                ]
            })
            .collect()
    }

    /// The screen in a state as the change of state leaves it, with a cursor of rows.
    fn in_state(way: i16, state: i16, lines: u16, at: i16, first: i16) -> ShopScreen {
        let mut screen = ShopScreen::open(way, 0, 1);
        (screen.state, screen.shown) = (state, SHOWN[state as usize]);
        screen.panel = PANEL_OF[state as usize];
        screen.cursor.rows = LIST_ROWS as i16;
        screen.cursor.lines = lines;
        (screen.cursor.at, screen.cursor.first) = (at, first);
        screen
    }

    fn lines(sprites: [u32; 3]) -> Vec<Vec<Piece>> {
        (sprites.into_iter().zip([94, 113, 132]))
            .map(|(value, y)| {
                vec![Piece {
                    flags: piece::CENTRED,
                    kind: kind::PICTURE,
                    value,
                    rect: Rect {
                        x: 10,
                        y,
                        w: 47,
                        h: 18,
                    },
                    margin: (0, 3),
                }]
            })
            .collect()
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
    fn the_shop_starts_with_its_menu_the_money_and_the_keeper() {
        let (first, kinds) = (lines([23, 170, 53]), lines([22, 20, 3]));
        let (wares, carried) = (wares(), carried(200));
        let shown = Shown {
            first_lines: &first,
            kind_lines: &kinds,
            list_lines: &rows(),
            wares: &wares,
            carried: &carried,
            circle: false,
            values: [10, 20, 30, 40, 50],
            would: [12, 20, 29, 40, 50],
        };
        let mut screen = ShopScreen::open(SHOP, 1, 1);
        screen.cursor.at = 1;
        let drawn = drawn(&screen, &shown, 9, |_| 31);
        // Underneath: the keeper's box with the three texts of line 9, then the face.
        assert_eq!(drawn[0], Drawn::WindowFrame { rect: KEEPER });
        assert_eq!(drawn[1], Drawn::WindowFill { rect: KEEPER });
        assert_eq!(
            drawn[3],
            Drawn::KeeperSaid {
                line: 9,
                text: 1,
                at: (87, 40)
            }
        );
        assert_eq!(drawn[6], Drawn::KeeperFace { at: (13, 9) });
        assert_eq!(drawn[7], Drawn::WindowFrame { rect: FACE_BOX });
        // The money on the left, then the three words, the second one pulsing.
        let frames = frames(&drawn);
        assert_eq!(frames.len(), 1 + 3);
        let money = Rect {
            x: 10,
            y: 77,
            w: 121,
            h: 16,
        };
        assert_eq!(frames[0], (money, 0, 0));
        assert_eq!(
            frames.iter().map(|frame| frame.2).collect::<Vec<_>>(),
            [0, 0, 9, 0]
        );
        assert!(drawn.contains(&Drawn::LongNumber {
            value: 200,
            ends: (106, 83),
            clip: SCREEN
        }));
        // A word of 31 lies in the middle of its piece of 47, 3 below its upper edge.
        let words: Vec<(usize, (i16, i16))> = (drawn.iter())
            .filter_map(|drawn| match *drawn {
                Drawn::Sprite { number, at, .. } => Some((number, at)),
                _ => None,
            })
            .collect();
        assert_eq!(
            words,
            [
                (sprite::UNIT, (107, 81)),
                (sprite::MONEY, (14, 80)),
                (23, (18, 97)),
                (170, (18, 116)),
                (53, (18, 135)),
            ]
        );
    }

    #[test]
    fn the_blacksmiths_kinds_and_the_answer_of_a_question() {
        let (first, kinds) = (lines([23, 170, 53]), lines([22, 20, 3]));
        let (wares, carried) = (wares(), carried(0));
        let shown = Shown {
            first_lines: &first,
            kind_lines: &kinds,
            list_lines: &rows(),
            wares: &wares,
            carried: &carried,
            circle: false,
            values: [10, 20, 30, 40, 50],
            would: [12, 20, 29, 40, 50],
        };
        let mut screen = ShopScreen::open(BLACKSMITH, 0, 1);
        let words = |screen: &ShopScreen| -> Vec<usize> {
            (drawn(screen, &shown, 0, |_| 31).iter())
                .filter_map(|drawn| match *drawn {
                    Drawn::Sprite { number, .. } if number < 100 || number == 170 => Some(number),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(words(&screen), [23, 170, 53]);
        (screen.state, screen.shown) = (KINDS_TO_BUY, SHOWN[KINDS_TO_BUY as usize]);
        assert_eq!(words(&screen), [22, 20, 3]);
        // A question: the bar lies under the box, on the no and then on the yes.
        (screen.state, screen.shown) = (QUESTION, SHOWN[QUESTION as usize]);
        screen.cursor.at = 1;
        let bar = |screen: &ShopScreen| super::drawn(screen, &shown, 5, |_| 31)[0];
        let on = |y: i16| Drawn::Frame {
            rect: Rect {
                x: 86,
                y,
                w: 30,
                h: 16,
            },
            mode: 1,
            time: 5,
        };
        assert_eq!(bar(&screen), on(53));
        screen.cursor.at = 0;
        assert_eq!(bar(&screen), on(37));
        assert_eq!(help(&screen), HELP_PLAIN);
        screen.state = 4;
        assert_eq!(help(&screen), HELP_OF_LISTS);
    }

    #[test]
    fn the_shops_offer_has_its_prices_and_a_dim_row() {
        let (first, kinds, rows) = (lines([23, 170, 53]), lines([22, 20, 3]), rows());
        let (wares, carried) = (wares(), carried(40));
        let shown = Shown {
            first_lines: &first,
            kind_lines: &kinds,
            list_lines: &rows,
            wares: &wares,
            carried: &carried,
            circle: false,
            values: [10, 20, 30, 40, 50],
            would: [12, 20, 29, 40, 50],
        };
        let screen = in_state(SHOP, OFFER, 3, 1, 0);
        assert_eq!(
            list_rows(&screen, &shown, list::OFFER).map(|row| row.map(|row| (row.thing, row.dim))),
            [Some((0, false)), Some((9, true)), Some((14, false)), None]
        );
        let drawn = drawn(&screen, &shown, 7, |_| 31);
        // The pictures of the three items, the second dim; the fourth row is bare.
        let pictures: Vec<(u8, (i16, i16), bool)> = (drawn.iter())
            .filter_map(|drawn| match *drawn {
                Drawn::ItemPicture { item, at, dim, .. } => Some((item, at, dim)),
                _ => None,
            })
            .collect();
        assert_eq!(
            pictures,
            [
                (0, (13, 115), false),
                (9, (13, 136), true),
                (14, (13, 157), false)
            ]
        );
        // The prices end 3 left of their room's right edge.
        let prices: Vec<(i32, (i16, i16), bool)> = (drawn.iter())
            .filter_map(|drawn| match *drawn {
                Drawn::Count {
                    value, ends, dim, ..
                } => Some((value, ends, dim)),
                _ => None,
            })
            .collect();
        assert_eq!(
            prices,
            [
                (15, (183, 118), false),
                (50, (183, 139), true),
                (10, (183, 160), false)
            ]
        );
        // The frames from underneath: the keeper's none; the heading, the box under the
        // numbers, four rows of three with the cursor's pulsing, the money, the count.
        let frames = frames(&drawn);
        assert_eq!(frames.len(), 2 + 12 + 1 + 1);
        assert_eq!(frames[0], (HEADING, 0, 0));
        assert_eq!(frames[1], (UNDER_NUMBERS, 0, 0));
        let times: Vec<u32> = frames[2..14].iter().map(|frame| frame.2).collect();
        assert_eq!(times, [0, 0, 0, 7, 7, 7, 0, 0, 0, 0, 0, 0]);
        assert_eq!(frames[15], (HAS_BOX, 0, 0));
        assert!(drawn.contains(&Drawn::Sprite {
            number: sprite::HEADING_OF_SHOP,
            at: (19, 81),
            clip: SCREEN
        }));
        // With the circle held the description of the item under the cursor stands in
        // place of the keeper's box and of the face; the rest is as it was.
        let held = Shown {
            circle: true,
            ..shown
        };
        assert_eq!(described(&screen, &shown), None);
        assert_eq!(described(&screen, &held), Some((Kind::Item, 9)));
        let with = super::drawn(&screen, &held, 7, |_| 31);
        assert_eq!(with.len(), drawn.len() - 8 + 5);
        assert_eq!(
            with[..2],
            [
                Drawn::Frame {
                    rect: DESCRIBED,
                    mode: 0,
                    time: 0
                },
                Drawn::Described {
                    item: 9,
                    line: 0,
                    at: (14, 23)
                }
            ]
        );
        assert_eq!(
            with[4],
            Drawn::Described {
                item: 9,
                line: 3,
                at: (14, 59)
            }
        );
        assert_eq!(with[5..], drawn[8..]);
        // Not on the row past the offer, and not behind the lists.
        let mut past = screen.clone();
        past.cursor.at = 3;
        assert_eq!(described(&past, &held), None);
        past = in_state(SHOP, QUESTION + 2, 2, 1, 0);
        assert_eq!(described(&past, &held), None);
        // Three lines: no sign. He has none of the item under the cursor.
        assert!(!drawn.iter().any(|d| matches!(d, Drawn::Sign { .. })));
        assert!(drawn.contains(&Drawn::LongNumber {
            value: carried.keeping.count(9, Kind::Item).into(),
            ends: (184, 82),
            clip: SCREEN
        }));
        assert_eq!(help(&screen), HELP_OF_LISTS);
    }

    #[test]
    fn the_players_list_scrolls_and_shows_what_a_thing_brings() {
        let (first, kinds, rows) = (lines([23, 170, 53]), lines([22, 20, 3]), rows());
        let (wares, mut carried) = (wares(), carried(0));
        for item in [0, 9, 14] {
            assert!(carried.keeping.give(item, 2, Kind::Item, carried.worn));
        }
        let at = |place| carried.keeping.number_at(place, Kind::Item);
        let last = carried.keeping.last_place(Kind::Item);
        let shown = Shown {
            first_lines: &first,
            kind_lines: &kinds,
            list_lines: &rows,
            wares: &wares,
            carried: &carried,
            circle: false,
            values: [10, 20, 30, 40, 50],
            would: [12, 20, 29, 40, 50],
        };
        // Six lines from the second on: a sign above and one below.
        let mut screen = in_state(SHOP, ITEMS, 6, 0, 1);
        let signs = |screen: &ShopScreen| -> Vec<(u16, (i16, i16))> {
            (drawn(screen, &shown, 0, |_| 31).iter())
                .filter_map(|drawn| match *drawn {
                    Drawn::Sign { turns, at, .. } => Some((turns, at)),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(
            signs(&screen),
            [(sign::BELOW, (79, 201)), (sign::ABOVE, (79, 104))]
        );
        screen.cursor.first = 2;
        assert_eq!(signs(&screen), [(sign::ABOVE, (79, 104))]);
        // A row is the place behind the first shown line; nothing of his own is dim, and
        // the number is three quarters of the price.
        screen.cursor.first = 0;
        let listed = list_rows(&screen, &shown, list::ITEMS);
        for (place, row) in listed.into_iter().enumerate() {
            let thing = at(place);
            let row = row.map(|row| (row.thing, row.number, row.dim));
            let worth = wares.worth(thing, Kind::Item);
            assert_eq!(row, (thing != FREE).then_some((thing, worth, false)));
        }
        assert_eq!(wares.worth(0, Kind::Item), 11);
        // The count is the one at the cursor's place.
        screen.sells = 1;
        screen.cursor.at = last as i16;
        let there = carried.keeping.count_at(last, Kind::Item);
        assert!(there >= 2);
        assert_eq!(has(&screen, &shown), there.into());
        screen.cursor.at = last as i16 + 1;
        assert_eq!(has(&screen, &shown), 0);
    }

    #[test]
    fn the_blacksmiths_list_marks_what_he_has_on() {
        let (first, kinds, rows) = (lines([23, 170, 53]), lines([22, 20, 3]), rows());
        let (wares, mut carried) = (wares(), carried(120));
        carried.worn = [3, FREE, FREE];
        let shown = Shown {
            first_lines: &first,
            kind_lines: &kinds,
            list_lines: &rows,
            wares: &wares,
            carried: &carried,
            circle: false,
            values: [10, 20, 30, 40, 50],
            would: [12, 20, 29, 40, 50],
        };
        // The offer of the weapons with the cursor on the third, the one he has on.
        let mut screen = in_state(BLACKSMITH, 4, 3, 2, 0);
        let all = drawn(&screen, &shown, 3, |_| 31);
        let weapons: Vec<(usize, u8, bool)> = (all.iter())
            .filter_map(|drawn| match *drawn {
                Drawn::GearName {
                    kind, thing, dim, ..
                } => Some((kind, thing, dim)),
                _ => None,
            })
            .collect();
        assert_eq!(weapons, [(0, 0, false), (0, 1, false), (0, 3, true)]);
        let mark = Drawn::Frame {
            rect: WORN_MARK,
            mode: 1,
            time: 3,
        };
        assert!(all.contains(&mark));
        assert!(all.contains(&Drawn::Sprite {
            number: sprite::HEADING_OF_KIND[0],
            at: (14, 80),
            clip: SCREEN
        }));
        screen.cursor.at = 1;
        assert!(!drawn(&screen, &shown, 3, |_| 31).contains(&mark));
        // Right of the offer: the two boxes with the sign between them, and what the
        // dagger under the cursor would make of his numbers, each in its colour.
        assert_eq!(weighed(&screen, &wares), Some((0, 1)));
        let all = drawn(&screen, &shown, 3, |_| 31);
        let boxes = frames(&all);
        let at = |rect: Rect| boxes.iter().position(|frame| frame.0 == rect);
        assert!(at(WOULD) < at(NUMBERS), "the second box is added last");
        assert!(all.contains(&Drawn::Sign {
            turns: 0,
            at: ARROW,
            time: 3
        }));
        let would = |all: &[Drawn]| -> Vec<(i32, (i16, i16), usize)> {
            (all.iter())
                .filter_map(|drawn| match *drawn {
                    Drawn::Tinted {
                        value,
                        ends,
                        colour,
                        ..
                    } => Some((value, ends, colour)),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(
            would(&all),
            [
                (12, (303, 117), colour::RISES),
                (20, (303, 131), colour::STAYS),
                (29, (303, 145), colour::FALLS),
                (40, (303, 159), colour::STAYS),
                (50, (303, 173), colour::STAYS),
            ]
        );
        // Behind the putting on only the box of the numbers is left; on the way there
        // the new numbers go first.
        let mut done = in_state(BLACKSMITH, PUT_ON_DONE, 1, 0, 0);
        done.kept = (1, 0);
        let all = drawn(&done, &shown, 3, |_| 31);
        assert!(would(&all).is_empty());
        assert!(frames(&all).iter().any(|frame| frame.0 == NUMBERS));
        assert!(!frames(&all).iter().any(|frame| frame.0 == WOULD));
        assert!(!all.iter().any(|d| matches!(d, Drawn::Sign { .. })));
        done = in_state(BLACKSMITH, PUT_ON, 2, 0, 0);
        (done.kept, done.asked) = ((1, 0), PUT_ON_DONE);
        let all = drawn(&done, &shown, 3, |_| 31);
        assert!(would(&all).is_empty());
        assert!(frames(&all).iter().any(|frame| frame.0 == WOULD));
        done.asked = -1;
        assert_eq!(would(&drawn(&done, &shown, 3, |_| 31)).len(), 5);
        // The circle shows the weapon's description, a thing of kind 0.
        let held = Shown {
            circle: true,
            ..shown
        };
        assert_eq!(described(&screen, &held), Some((Kind::Weapon, 1)));
        assert_eq!(
            drawn(&screen, &held, 3, |_| 31)[2],
            Drawn::GearDescribed {
                kind: 0,
                thing: 1,
                line: 1,
                at: (14, 35)
            }
        );
        // Behind the question the list shows the kept place in the marked colour, has
        // no signs, and the mark goes by the kept place.
        screen = in_state(BLACKSMITH, QUESTION, 2, 1, 0);
        screen.cursor.rows = 2;
        screen.kept = (2, 0);
        let all = drawn(&screen, &shown, 3, |_| 31);
        assert!(all.contains(&mark));
        let row = |y: i16| Rect {
            x: 10,
            y,
            w: 21,
            h: 21,
        };
        assert!(all.contains(&Drawn::Frame {
            rect: row(154),
            mode: 2,
            time: 0
        }));
        assert!(all.contains(&Drawn::Frame {
            rect: row(133),
            mode: 0,
            time: 0
        }));
        // In the states of the putting on there is no mark.
        (screen.state, screen.shown) = (PUT_ON, SHOWN[PUT_ON as usize]);
        assert!(!drawn(&screen, &shown, 3, |_| 31).contains(&mark));
    }

    #[test]
    fn the_amount_and_the_sum_stand_beside_the_list() {
        let (first, kinds, rows) = (lines([23, 170, 53]), lines([22, 20, 3]), rows());
        let (wares, carried) = (wares(), carried(200));
        let shown = Shown {
            first_lines: &first,
            kind_lines: &kinds,
            list_lines: &rows,
            wares: &wares,
            carried: &carried,
            circle: false,
            values: [10, 20, 30, 40, 50],
            would: [12, 20, 29, 40, 50],
        };
        let boxes = |screen: &ShopScreen| -> Vec<(Rect, u8, u32)> {
            (frames(&drawn(screen, &shown, 6, |_| 31)).into_iter())
                .filter(|frame| [AMOUNT_BOX, SUM_BOX].contains(&frame.0))
                .collect()
        };
        let numbers = |screen: &ShopScreen| -> Vec<(i32, (i16, i16))> {
            (drawn(screen, &shown, 6, |_| 31).iter())
                .filter_map(|drawn| match *drawn {
                    Drawn::LongNumber { value, ends, .. } => Some((value, ends)),
                    _ => None,
                })
                .collect()
        };
        // Three of item 9 at the shop: the amount's box pulses, the sum is the price
        // times three; then the money and how many he has.
        let mut screen = in_state(SHOP, AMOUNT + 2, 0, 0, 0);
        (screen.thing, screen.amount, screen.kept) = (9, 3, (1, 0));
        assert_eq!(boxes(&screen), [(SUM_BOX, 0, 0), (AMOUNT_BOX, 1, 6)]);
        assert_eq!(
            numbers(&screen),
            [
                (150, (285, 100)),
                (3, (184, 99)),
                (200, (285, 83)),
                (carried.keeping.count(9, Kind::Item).into(), (184, 82))
            ]
        );
        let all = drawn(&screen, &shown, 6, |_| 31);
        for (number, at) in [
            (sprite::SUM, (193, 96)),
            (sprite::UNIT, (286, 98)),
            (sprite::AMOUNT, (123, 97)),
        ] {
            assert!(all.contains(&Drawn::Sprite {
                number,
                at,
                clip: SCREEN
            }));
        }
        // Behind the question the amount's box has the marked colour.
        screen = in_state(SHOP, QUESTION + 2, 2, 1, 0);
        (screen.thing, screen.amount, screen.kept) = (9, 3, (1, 0));
        assert_eq!(boxes(&screen), [(SUM_BOX, 0, 0), (AMOUNT_BOX, 2, 0)]);
        assert_eq!(sum(&screen, &shown), 150);
        // To sell, the sum is three quarters of the price, rounded down, times the
        // amount; at the blacksmith's the price is the weapon's.
        screen = in_state(SHOP, AMOUNT + 3, 0, 0, 0);
        (screen.sells, screen.thing, screen.amount) = (1, 0, 4);
        assert_eq!(boxes(&screen), [(SUM_BOX, 0, 0), (AMOUNT_BOX, 1, 6)]);
        assert_eq!(sum(&screen, &shown), 44);
        screen = in_state(BLACKSMITH, AMOUNT, 0, 0, 0);
        (screen.thing, screen.amount) = (1, 2);
        assert_eq!(boxes(&screen), [(SUM_BOX, 0, 0), (AMOUNT_BOX, 1, 6)]);
        assert_eq!(sum(&screen, &shown), 200);
        screen = in_state(BLACKSMITH, QUESTION + 1, 2, 1, 0);
        (screen.sells, screen.thing, screen.amount) = (1, 1, 2);
        assert_eq!(boxes(&screen), [(SUM_BOX, 0, 0), (AMOUNT_BOX, 2, 0)]);
        assert_eq!(sum(&screen, &shown), 150);
        // Without a thing there is no box of the sum.
        screen.thing = FREE;
        assert_eq!(boxes(&screen), [(AMOUNT_BOX, 2, 0)]);
    }

    #[test]
    fn the_keepers_texts_are_read_by_the_argument() {
        let base = 0x800B_6890;
        let mut file = vec![0u8; (KEEPER_OF_3 - base) as usize + 4 * KEEPER_LINES];
        let mut put = |address: u32, word: u32| {
            let at = (address - base) as usize;
            file[at..at + 4].copy_from_slice(&word.to_le_bytes());
        };
        // Every line of the three tables names the record at +0x100, whose second text
        // is the letter at +0x200; line 4 of the table for argument 2 has its own.
        for table in [KEEPER_OF_OTHERS, KEEPER_OF_2, KEEPER_OF_3] {
            for line in 0..KEEPER_LINES as u32 {
                put(table + 4 * line, base + 0x100);
            }
        }
        put(KEEPER_OF_2 + 4 * 4, base + 0x110);
        put(base + 0x104, base + 0x200);
        put(base + 0x110, base + 0x200);
        put(base + 0x200, 0x6082);
        let texts = keeper_texts(&file, base, 0).expect("the first table");
        assert_eq!(texts.len(), KEEPER_LINES);
        assert_eq!(
            texts[4].each_ref().map(|text| text.len()),
            [0, 1, 0],
            "an address of 0 is the empty text"
        );
        let texts = keeper_texts(&file, base, 2).expect("the table of argument 2");
        assert_eq!(texts[4].each_ref().map(|text| text.len()), [1, 0, 0]);
        assert_eq!(texts[5].each_ref().map(|text| text.len()), [0, 1, 0]);
        assert_eq!(keeper_texts(&file[..0x100], base, 3), None);
    }
}
