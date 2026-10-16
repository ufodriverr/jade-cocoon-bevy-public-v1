
use super::menu::{Drew, Menu, leaves_out, level};
use crate::menu::{Menu as Lines, Piece, Rect, Style, kind, piece, placed};
use crate::player_panel::{self, AttackRow, ItemRow, SpellRows, attacks, items, message, spells};
use crate::rooms::{Image, step};
use crate::window::{OPENING, Way, opened};

/// The archive file of the fights' overlay, which holds the menu's tables, and the
/// address of its first byte.
pub const FILE: usize = 36;
pub const FILE_BASE: u32 = 0x800B_6890;

/// A word a side: the list of the lines of its root menu.
const LINES: u32 = 0x800C_97F8;
/// The menu's texts: ten lines of help and eleven messages of a refusal.
const TEXTS: u32 = 0x800C_9520;
pub const TEXT_COUNT: usize = 21;
/// The cells of a side's root menu, 8 bytes each: the place of the cell's line of help
/// in the table of the texts, and its refusal, which the maker writes.
const CELLS: [(u32, usize); 2] = [(0x800C_9574, 6), (0x800C_95A4, 5)];
/// The descriptions of the items: the addresses of four lines for each (`0x800C7F78`
/// hands 16 bytes an item to the menus' box of a message).
const DESCRIPTIONS: u32 = 0x800C_9898;
/// The descriptions of the spells and of the body attacks, four addresses each as the
/// items': the spells' table ends where the attacks' begins (`0x8002C6CC` is handed
/// the second), which has an entry for each attack with a name; the lines of the
/// attacks 0 and 1 are empty.
const SPELL_TEXTS: u32 = 0x800C_9B48;
const ATTACK_TEXTS: u32 = 0x800C_9D28;
pub const ATTACK_TEXT_COUNT: usize = 16;
/// The flag word the three lists are drawn with: the frames of the row under the
/// cursor pulse (the style 2 of the records `0x800C7F78` makes for `0x8002C784`,
/// `0x8002CE84` and `0x8002CB24`).
const LIST_FLAGS: u32 = 0x02;
/// The place `0x800C7F78` hands the list of the spells; the list of the body attacks
/// gets none and stands where the player's menu has it.
pub const SPELLS_AT: (i16, i16) = (96, 69);
/// A word a level: the list of its line of help, in the executable.
const LEGENDS: u32 = 0x800C_9844;
/// The levels that draw.
pub const LEVELS: usize = 5;
/// The flag word of the shared drawer's record: on the root the frame under the cursor
/// pulses, on the other levels it stands in the marked colour.
const ON_ROOT: u32 = 0x02;
const BEHIND_ROOT: u32 = 0x04;

/// The small box of a text on the root (`0x800C89F8`): one line, as wide as its text
/// and 4 more, with the text 2 right of and 4 below its corner.
pub const SMALL_BOX: (i16, i16) = (25, 138);
pub const SMALL_BOX_HIGH: i16 = 18;
pub const SMALL_BOX_SPARE: i16 = 4;
pub const SMALL_TEXT: (i16, i16) = (2, 4);

/// One thing of the menu, as a painter draws it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drawn {
    /// A frame of the menus in the window colour, half of what is under it and half
    /// its colour: mode 0 the plain colour, 1 the colour that pulses by the time, 2
    /// the marked colour.
    Frame { rect: Rect, mode: u8, time: u32 },
    /// A sprite of the menus' pool; a dim one at half its brightness.
    Sprite {
        number: usize,
        at: (i16, i16),
        dim: bool,
        clip: Rect,
    },
    /// A text of the menu's table in the font of the texts.
    Text { text: usize, at: (i16, i16) },
    /// A line of an item's description in the font of the texts
    /// ([`Tables::described`]).
    Described {
        item: u8,
        line: usize,
        at: (i16, i16),
    },
    /// A thing of the list of the items, which is the executable's own drawer: its
    /// frames, pictures, names, counts and scroll signs.
    Listed(player_panel::Drawn),
}

/// What the menu shows of the fight's list of items in a pass: the four places from
/// the list's first line on, and the last place in use (-1 for none).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Items {
    pub rows: [ItemRow; items::ROWS],
    pub last_place: i16,
}

/// What the two lists of a creature are drawn with: the executable's lines of each and
/// the sprites of their rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shapes {
    pub attacks: Vec<Vec<Piece>>,
    pub spells: Vec<Vec<Piece>>,
    pub attack_pictures: [[u8; 5]; attacks::ROWS],
    pub spell_pictures: [u8; spells::SPELLS],
}

impl Shapes {
    /// Reads them from the loaded executable, whose first byte is at `base`.
    pub fn from_executable(executable: &[u8], base: u32) -> Option<Self> {
        let lines = |list: u32, rows: usize| {
            Lines::lines_at(executable, base, list).filter(|lines| lines.len() == rows)
        };
        Some(Self {
            attacks: lines(attacks::LINES, attacks::ROWS)?,
            spells: lines(spells::LINES, spells::ROWS)?,
            attack_pictures: player_panel::attack_pictures(executable, base)?,
            spell_pictures: player_panel::spell_pictures(executable, base)?,
        })
    }
}

/// What the two lists show of the creature that chooses in a pass
/// (`menu::Lists::attack_rows` and `spell_rows`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Moves {
    pub attacks: [AttackRow; attacks::ROWS],
    pub spells: SpellRows,
}

/// The tables of the menu's picture, from the file of the fights.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tables {
    /// The lines of the root menu by the side.
    pub lines: [Vec<Vec<Piece>>; 2],
    /// The line of help of each cell by the side: a place in the texts.
    pub help: [Vec<usize>; 2],
    pub texts: Vec<Vec<u16>>,
    /// The four lines of each item's description; a line without an address is empty.
    pub described: Vec<[Vec<u16>; message::LINES]>,
    /// The four lines of each spell's description and of each named body attack's.
    pub spell_texts: Vec<[Vec<u16>; message::LINES]>,
    pub attack_texts: Vec<[Vec<u16>; message::LINES]>,
    /// The address of the line of help's list by the level, in the executable.
    pub legends: [u32; LEVELS],
}

impl Tables {
    /// Reads the tables from the file, whose first byte is at `base`.
    pub fn from_file(file: &[u8], base: u32) -> Option<Self> {
        let image = Image { data: file, base };
        let word = |table: u32, n: usize| image.u32(step(table, n, 4).ok()?).ok();
        let lines = |side: usize| Lines::lines_at(file, base, word(LINES, side)?);
        let help = |side: usize| {
            let (cells, count) = CELLS[side];
            (0..count)
                .map(|cell| {
                    let entry = word(cells, 2 * cell)?.checked_sub(TEXTS)?;
                    let text = (entry / 4) as usize;
                    (entry % 4 == 0 && text < TEXT_COUNT).then_some(text)
                })
                .collect::<Option<Vec<_>>>()
        };
        let texts = (0..TEXT_COUNT)
            .map(|text| {
                let offset = word(TEXTS, text)?.checked_sub(base)? as usize;
                Some(crate::text::units(file.get(offset..)?))
            })
            .collect::<Option<Vec<_>>>()?;
        let described = (0..usize::from(items::ITEMS))
            .map(|item| {
                let mut lines: [Vec<u16>; message::LINES] = Default::default();
                for (line, units) in lines.iter_mut().enumerate() {
                    let address = word(DESCRIPTIONS, message::LINES * item + line)?;
                    if address != 0 {
                        let offset = address.checked_sub(base)? as usize;
                        *units = crate::text::units(file.get(offset..)?);
                    }
                }
                Some(lines)
            })
            .collect::<Option<Vec<_>>>()?;
        let four = |table: u32, count: usize| {
            (0..count)
                .map(|n| player_panel::four_lines(file, base, table, n))
                .collect::<Option<Vec<_>>>()
        };
        let mut legends = [0; LEVELS];
        for (level, list) in legends.iter_mut().enumerate() {
            *list = word(LEGENDS, level)?;
        }
        Some(Self {
            lines: [lines(0)?, lines(1)?],
            help: [help(0)?, help(1)?],
            texts,
            described,
            spell_texts: four(SPELL_TEXTS, spells::SPELLS)?,
            attack_texts: four(ATTACK_TEXTS, ATTACK_TEXT_COUNT)?,
            legends,
        })
    }

    /// The list of the line of help a pass draws last (`0x8002B064`), an address of the
    /// executable.
    pub fn legend(&self, drew: Drew) -> Option<u32> {
        self.legends.get(usize::from(drew.level)).copied()
    }

    /// The tables of a minion's menu in the build's fight of two players
    /// (`menu::DuelOffer`): the creature's five cells with "Change" as wide as one
    /// column, and a sixth cell "Item" beside it, the player's word of "Item" in the
    /// place of his sixth cell, with his line of help of it.
    pub fn for_duel(&self) -> Self {
        let mut tables = self.clone();
        let player = &self.lines[0];
        let (Some(item), Some(beside)) = (player.get(4), player.get(5)) else {
            return tables;
        };
        let rect = |line: &Vec<Piece>| line.first().map(|piece| piece.rect);
        if let (Some(change), Some(narrow)) = (tables.lines[1].get_mut(4), rect(item)) {
            for piece in change.iter_mut() {
                piece.rect.w = narrow.w;
            }
        }
        let cell = (item.iter())
            .map(|piece| Piece {
                rect: rect(beside).unwrap_or(piece.rect),
                ..*piece
            })
            .collect();
        tables.lines[1].push(cell);
        if let Some(&help) = self.help[0].get(4) {
            tables.help[1].push(help);
        }
        tables
    }
}

/// The text a pass draws in a box: the message of a refused confirm, or on the root the
/// line of help of the cell under the cursor while the circle is held.
fn said(menu: &Menu, tables: &Tables) -> Option<usize> {
    let drew = menu.drew();
    if let Some(message) = drew.message {
        return Some(usize::from(message));
    }
    let cell = usize::try_from(menu.root().at.wrapping_add(menu.root().first)).ok()?;
    let on_root = drew.level == level::ROOT && drew.describes;
    on_root.then(|| tables.help[menu.side()].get(cell).copied())?
}

/// What the lists are drawn with and show in a pass: the executable's four lines of a
/// list of items (`player_panel::items::LINES`) with what the fight's list holds, and
/// for a creature its two lists; the player has neither.
pub type Held<'a> = (&'a [Vec<Piece>], &'a Items, Option<(&'a Shapes, &'a Moves)>);

/// A place of the menu's cursors as a pointer of the build names it (the game has
/// none): the level and the place of its cursor.
pub type Spot = (u8, i16);

/// The spot the cursor is on.
pub fn here(menu: &Menu) -> Spot {
    (menu.level(), menu.cursor().at)
}

/// Where the spots of the level the last pass drew lie, each with a rectangle that
/// shows it: the cells of the root, the rows of the three lists and of the picker of a
/// target on the own side, and a target among the opponents where the fighter stands
/// on the fight's picture: `opponents` has the rectangle of each opponent's slot
/// there, which may reach beyond the console's 320 columns in a wide fight.
pub fn spots(
    menu: &Menu,
    tables: &Tables,
    (list, _, moves): Held,
    own: &[(usize, bool)],
    opponents: &[(usize, Rect)],
) -> Vec<(Spot, Rect)> {
    let drew = menu.drew();
    let level = menu.level();
    if drew.level >= level::DONE || drew.level != level {
        return Vec::new();
    }
    let on = |rows: Vec<(i16, Rect)>| -> Vec<(Spot, Rect)> {
        rows.into_iter()
            .map(|(at, rect)| ((level, at), rect))
            .collect()
    };
    match level {
        level::ROOT if drew.picks.is_none() => {
            let lines = tables.lines.get(menu.side()).map_or(&[][..], Vec::as_slice);
            let first = menu.root().first;
            let cells = player_panel::line_rects(lines).into_iter();
            on(cells.map(|(cell, rect)| (cell - first, rect)).collect())
        }
        level::ITEMS => on(player_panel::row_rects(
            list,
            &items::PIECES,
            items::FIRST_ROW,
            items::STEP,
        )),
        level::ATTACKS => match moves {
            Some((shapes, _)) => on(player_panel::row_rects(
                &shapes.attacks,
                &attacks::PIECES,
                attacks::FIRST_ROW,
                attacks::STEP,
            )),
            None => Vec::new(),
        },
        level::SPELLS => match moves {
            Some((shapes, _)) => {
                let moved = SPELLS_AT.0 - spells::PIECES[0];
                let starts = spells::PIECES.map(|x| x + moved);
                on(player_panel::row_rects(
                    &shapes.spells,
                    &starts,
                    SPELLS_AT.1,
                    spells::STEP,
                ))
            }
            None => Vec::new(),
        },
        level::TARGET if drew.picks.is_some() => on(player_panel::picker_rects(own)),
        level::TARGET => {
            let places = menu.opponent_places().unwrap_or_default();
            let at = |slot: &usize| opponents.iter().find(|(of, _)| of == slot);
            let found = (places.iter().zip(0..))
                .filter_map(|(slot, place)| at(slot).map(|(_, rect)| (place, *rect)));
            on(found.collect())
        }
        _ => Vec::new(),
    }
}

/// The directions that bring the cursor a step nearer to a spot of its level.
pub fn way(menu: &Menu, (level, at): Spot) -> u16 {
    match level == menu.level() {
        true => menu.cursor().way_to(at),
        false => 0,
    }
}

/// What the last pass of the menu drew (`0x800C7F78`, and the message of the state
/// `0x800C8AB4`), from what lies underneath to what lies on top; the line of help
/// ([`Tables::legend`]) is the caller's and lies under all of it. `time` is the menus'
/// time, `width` the width of a sprite of the pool and `text_width` the width of a
/// text in the font of the texts; the lists are [`Held`], and `own` the rows of the picker of a target on the own side, each with
/// its slot and whether it is shown closed (`menu::Lists::picker`).
pub fn drawn(
    menu: &Menu,
    tables: &Tables,
    (list, held, moves): Held,
    own: &[(usize, bool)],
    time: u32,
    width: impl Fn(usize) -> i16,
    text_width: impl Fn(&[u16]) -> i16,
) -> Vec<Drawn> {
    let drew = menu.drew();
    let mut drawn = Vec::new();
    let on_root = drew.level == level::ROOT;
    // A pass that draws the picker of the own side leaves the root out for it.
    let picks = drew.picks.filter(|_| drew.level < level::DONE);
    if drew.level < level::DONE && drew.leaves_out & leaves_out::ROOT == 0 && picks.is_none() {
        let flags = if on_root { ON_ROOT } else { BEHIND_ROOT };
        let cursor = menu.root().at.wrapping_add(menu.root().first);
        for (cell, pieces) in tables.lines[menu.side()].iter().enumerate() {
            let (style, pulses) = Style::of_line(flags, usize::try_from(cursor) == Ok(cell), 0);
            // A closed command has the dim bit in the flag word of its line.
            let closed =
                (menu.command_of(cell)).is_some_and(|command| menu.closed(command).is_some());
            for piece in pieces {
                // The frame grows about its middle, and the content is cut to it.
                let clip = opened(piece.rect, menu.openness().into(), OPENING, Way::Middle);
                if piece.flags & piece::BARE == 0 {
                    drawn.push(Drawn::Frame {
                        rect: clip,
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
                        dim: closed || piece.flags & piece::DIM != 0,
                        clip,
                    });
                }
            }
        }
    }
    // The list of the items stands above the root, whole at once.
    if drew.level < level::DONE && drew.leaves_out & leaves_out::ITEMS == 0 {
        let kept = (menu.items().at, menu.items().first);
        let rows =
            player_panel::item_rows(LIST_FLAGS, kept, list, held.rows, held.last_place, time);
        drawn.extend(rows.into_iter().map(Drawn::Listed));
    }
    // The two lists of a creature stand beside the root as the list of the items does,
    // and the text of the row under the cursor lies on them in the menus' box of a
    // message: a body attack's for an entry that is known and whose attack is not 0
    // (`0x8002C6CC`; the root is left out for the held circle alone), a spell's
    // whenever the pass describes.
    let mut text = None;
    if let Some((shapes, moves)) = moves.filter(|_| drew.level < level::DONE) {
        if drew.leaves_out & leaves_out::ATTACKS == 0 {
            let cursor = menu.attacks().at;
            let (lines, pictures) = (&shapes.attacks, &shapes.attack_pictures);
            let rows =
                player_panel::attack_rows(LIST_FLAGS, cursor, lines, moves.attacks, pictures, time);
            drawn.extend(rows.into_iter().map(Drawn::Listed));
            let row = usize::try_from(cursor.wrapping_add(menu.attacks().first)).ok();
            let row = row.and_then(|row| moves.attacks.get(row));
            text = row
                .filter(|row| drew.level == level::ATTACKS && row.known && row.attack != 0)
                .map(|row| player_panel::Listed::Attack(row.attack.into()));
        }
        if drew.leaves_out & leaves_out::SPELLS == 0 {
            let kept = (menu.spells().at, menu.spells().first);
            let (lines, pictures) = (&shapes.spells, &shapes.spell_pictures[..]);
            let rows = player_panel::spell_rows(
                LIST_FLAGS,
                kept,
                SPELLS_AT,
                lines,
                moves.spells,
                pictures,
                time,
            );
            drawn.extend(rows.into_iter().map(Drawn::Listed));
            let row = usize::try_from(kept.0).ok();
            let spell = row.and_then(|row| moves.spells.rows.get(row)).copied();
            text = spell
                .filter(|spell| drew.level == level::SPELLS && *spell < spells::SPELLS)
                .map(player_panel::Listed::Spell);
        }
    }
    if let Some(what) = text.filter(|_| drew.describes && drew.message.is_none()) {
        let described = player_panel::list_description(what);
        drawn.extend(described.into_iter().map(Drawn::Listed));
    }
    // The picker of a target on the own side, with the cursor it was handed.
    if let Some(row) = picks {
        let rows = player_panel::picker_rows(own, usize::try_from(row).ok(), time);
        drawn.extend(rows.into_iter().map(Drawn::Listed));
    }
    // The description of the item under the list's cursor: the menus' box of a
    // message with its four lines.
    if drew.level == level::ITEMS && drew.describes && drew.message.is_none() {
        let row = usize::try_from(menu.items().at).ok();
        let item = row.and_then(|row| held.rows.get(row)).map(|row| row.item);
        if let Some(item) = item.filter(|item| *item < items::ITEMS) {
            let (x, y) = (
                message::BOX.x + message::TEXT.0,
                message::BOX.y + message::TEXT.1,
            );
            drawn.push(Drawn::Frame {
                rect: message::BOX,
                mode: 0,
                time: 0,
            });
            drawn.extend((0..message::LINES).map(|line| Drawn::Described {
                item,
                line,
                at: (x, y + message::LINE * line as i16),
            }));
        }
    }
    let Some(text) = said(menu, tables) else {
        return drawn;
    };
    // On the root a small box of one line, on the other levels the menus' box of a
    // message; both are plain frames that do not open.
    let (rect, at) = match on_root {
        true => {
            let wide = (tables.texts.get(text)).map_or(0, |units| text_width(units));
            let rect = Rect {
                x: SMALL_BOX.0,
                y: SMALL_BOX.1,
                w: wide.wrapping_add(SMALL_BOX_SPARE),
                h: SMALL_BOX_HIGH,
            };
            (rect, (rect.x + SMALL_TEXT.0, rect.y + SMALL_TEXT.1))
        }
        false => {
            let rect = message::BOX;
            (rect, (rect.x + message::TEXT.0, rect.y + message::TEXT.1))
        }
    };
    drawn.extend([
        Drawn::Frame {
            rect,
            mode: 0,
            time: 0,
        },
        Drawn::Text { text, at },
    ]);
    drawn
}

#[cfg(test)]
mod tests {
    use super::super::menu::{Asked, Listed, Offer, Refused, text};
    use super::*;
    use crate::pad::{self, Pad, Repeat};

    /// A fight without lists.
    struct Nothing;

    impl Asked for Nothing {
        fn item(&self, _: usize) -> Result<Listed, Refused> {
            Err(None)
        }
        fn attack(&self, _: usize) -> Result<Listed, Refused> {
            Err(None)
        }
        fn spell(&self, _: usize) -> Result<Listed, Refused> {
            Err(None)
        }
        fn own(&self, _: u8, _: u32, _: i16) -> usize {
            0
        }
        fn target(&self, _: u8, _: u32, _: usize) -> Result<(), Refused> {
            Ok(())
        }
        fn described(&self, level: u8, _: usize) -> bool {
            level == level::ITEMS
        }
    }

    /// The player's root menu as the file has it: six framed words in two columns.
    fn tables() -> Tables {
        let line = |sprite: u32, x: i16, y: i16| {
            vec![Piece {
                flags: piece::CENTRED,
                kind: kind::PICTURE,
                value: sprite,
                rect: Rect { x, y, w: 38, h: 18 },
                margin: (2, 2),
            }]
        };
        let rows = [157, 176, 195];
        let player = (0..6)
            .map(|cell| line(100 + cell as u32, [22, 60][cell % 2], rows[cell / 2]))
            .collect();
        Tables {
            lines: [player, Vec::new()],
            help: [(0..6).collect(), Vec::new()],
            texts: (0..TEXT_COUNT).map(|text| vec![0; text]).collect(),
            described: Vec::new(),
            spell_texts: Vec::new(),
            attack_texts: Vec::new(),
            legends: [10, 11, 12, 13, 14],
        }
    }

    #[test]
    fn a_minion_of_the_fight_of_two_players_has_the_item_beside_the_change() {
        let mut tables = tables();
        tables.lines[1] = tables.lines[0][..5].to_vec();
        tables.lines[1][4][0].rect.w = 76;
        tables.help[1] = vec![6, 1, 7, 8, 9];
        let duel = tables.for_duel();
        assert_eq!(duel.lines[1].len(), 6);
        assert_eq!(duel.lines[1][4][0].rect.w, 38);
        let item = duel.lines[1][5][0];
        assert_eq!(item.value, 104);
        assert_eq!((item.rect.x, item.rect.y), (60, 195));
        assert_eq!(duel.help[1], [6, 1, 7, 8, 9, 4]);
        // The story's tables are as they were.
        assert_eq!(tables.lines[1].len(), 5);
    }

    /// A player alone against one opponent, with no empty cocoon.
    fn menu() -> Menu {
        Menu::new(Offer {
            fighter: 0,
            carried: 0,
            opponents: 0b0001_0000,
            boss: false,
            no_cocoon: true,
            no_retreat: false,
            places: 0,
            spells: 0,
            kept: 0,
            duel: None,
        })
    }

    fn go(menu: &mut Menu, buttons: u16) {
        let mut pad = Pad::default();
        pad.read(buttons, Repeat::of(2), 0);
        menu.pass(&pad, &Nothing);
    }

    fn picture(menu: &Menu) -> Vec<Drawn> {
        let items = Items::default();
        let width = |units: &[u16]| 6 * units.len() as i16;
        drawn(menu, &tables(), (&[], &items, None), &[], 7, |_| 30, width)
    }

    #[test]
    fn a_target_among_the_opponents_is_found_where_the_fighter_stands() {
        let mut menu = Menu::new(Offer {
            fighter: 0,
            carried: 0,
            opponents: 0b0101_0000,
            boss: false,
            no_cocoon: true,
            no_retreat: false,
            places: 0,
            spells: 0,
            kept: 0,
            duel: None,
        });
        let tables = tables();
        let items = Items::default();
        let rect = |x| Rect {
            x,
            y: 90,
            w: 50,
            h: 60,
        };
        // Slot 5 is not on the field; a place of the wide picture left of the console's.
        let opponents = [(6, rect(-40)), (4, rect(130)), (5, rect(200))];
        let found = |menu: &Menu| spots(menu, &tables, (&[], &items, None), &[], &opponents);
        go(&mut menu, 0);
        assert!(
            found(&menu)
                .iter()
                .all(|((level, _), _)| *level == level::ROOT)
        );
        // "Attack" leads to a target among the opponents: a place for each that stands,
        // in the order of their slots, at the fighter's rectangle.
        go(&mut menu, pad::CROSS);
        go(&mut menu, 0);
        assert_eq!(menu.opponent_places(), Some(vec![4, 6]));
        assert_eq!(
            found(&menu),
            [
                ((level::TARGET, 0), rect(130)),
                ((level::TARGET, 1), rect(-40))
            ]
        );
        assert_eq!(way(&menu, (level::TARGET, 1)), pad::RIGHT);
        // A target on the own side is picked in the picker's rows instead.
        assert_eq!(self::menu().opponent_places(), None);
    }

    #[test]
    fn the_root_opens_with_a_pulsing_frame_under_the_cursor_and_dim_closed_commands() {
        let mut menu = menu();
        go(&mut menu, 0);
        // The first pass: an eighth of every frame, about its middle.
        let first = picture(&menu);
        assert_eq!(first.len(), 12);
        let small = Rect {
            x: 22 + 19 - 2,
            y: 157 + 9 - 1,
            w: 4,
            h: 2,
        };
        assert_eq!(
            first[0],
            Drawn::Frame {
                rect: small,
                mode: 1,
                time: 7
            }
        );
        for _ in 0..7 {
            go(&mut menu, 0);
        }
        let whole = picture(&menu);
        let frame = |cell: usize| whole[2 * cell];
        let word = |cell: usize| whole[2 * cell + 1];
        let rect = |x: i16, y: i16| Rect { x, y, w: 38, h: 18 };
        // The frame under the cursor pulses by the time, the others are plain.
        assert_eq!(
            frame(0),
            Drawn::Frame {
                rect: rect(22, 157),
                mode: 1,
                time: 7
            }
        );
        assert_eq!(
            frame(3),
            Drawn::Frame {
                rect: rect(60, 176),
                mode: 1,
                time: 0
            }
        );
        // A word of 30 lies in the middle of a room of 34 at (24, 159).
        assert_eq!(
            word(0),
            Drawn::Sprite {
                number: 100,
                at: (26, 159),
                dim: false,
                clip: rect(22, 157)
            }
        );
        // Nobody to summon and no empty cocoon: the cells 2 and 3 are dim.
        let dim = |drawn: Drawn| matches!(drawn, Drawn::Sprite { dim: true, .. });
        let dims: Vec<bool> = (0..6).map(|cell| dim(word(cell))).collect();
        assert_eq!(dims, [false, false, true, true, false, false]);
    }

    #[test]
    fn the_circle_and_a_refusal_put_a_text_in_the_small_box() {
        let mut menu = menu();
        go(&mut menu, pad::CIRCLE);
        let described = picture(&menu);
        assert_eq!(described.len(), 14);
        // The line of help of the first cell is text 0, which is empty here.
        let small = |wide: i16| Drawn::Frame {
            rect: Rect {
                x: 25,
                y: 138,
                w: wide + 4,
                h: 18,
            },
            mode: 0,
            time: 0,
        };
        assert_eq!(described[12], small(0));
        assert_eq!(
            described[13],
            Drawn::Text {
                text: 0,
                at: (27, 142)
            }
        );
        // Down to "summon" and a confirm: refused, and the message is drawn from the
        // pass behind on.
        go(&mut menu, pad::DOWN);
        go(&mut menu, pad::CROSS);
        assert_eq!(picture(&menu).len(), 12);
        go(&mut menu, 0);
        let refused = picture(&menu);
        let message = usize::from(text::NO_CREATURE);
        assert_eq!(refused[12], small(6 * message as i16));
        assert_eq!(
            refused[13],
            Drawn::Text {
                text: message,
                at: (27, 142)
            }
        );
    }

    #[test]
    fn behind_the_root_the_cursors_frame_is_marked_and_the_help_is_the_levels() {
        let mut menu = menu();
        let tables = tables();
        go(&mut menu, 0);
        assert_eq!(tables.legend(menu.drew()), Some(10));
        // The confirm on "attack": the pass still drew the root as the root.
        go(&mut menu, pad::CROSS);
        assert_eq!(tables.legend(menu.drew()), Some(10));
        go(&mut menu, 0);
        assert_eq!(tables.legend(menu.drew()), Some(14));
        let frames: Vec<u8> = (picture(&menu).into_iter())
            .filter_map(|drawn| match drawn {
                Drawn::Frame { mode, time: 0, .. } => Some(mode),
                _ => None,
            })
            .collect();
        assert_eq!(frames, [2, 0, 0, 0, 0, 0]);
    }

    /// A fight whose first item is for one of the own side.
    struct Healing;

    impl Asked for Healing {
        fn item(&self, _: usize) -> Result<Listed, Refused> {
            Ok(Listed {
                parameter: 0,
                class: super::super::menu::class::OWN,
            })
        }
        fn attack(&self, _: usize) -> Result<Listed, Refused> {
            Err(None)
        }
        fn spell(&self, _: usize) -> Result<Listed, Refused> {
            Err(None)
        }
        fn own(&self, _: u8, _: u32, row: i16) -> usize {
            row.max(0) as usize
        }
        fn target(&self, _: u8, _: u32, _: usize) -> Result<(), Refused> {
            Err(Some(text::NO_NEED))
        }
        fn described(&self, _: u8, _: usize) -> bool {
            false
        }
    }

    #[test]
    fn the_picker_of_the_own_side_stands_in_place_of_the_root() {
        let mut menu = menu();
        let healing = |menu: &mut Menu, buttons: u16| {
            let mut pad = Pad::default();
            pad.read(buttons, Repeat::of(2), 0);
            menu.pass(&pad, &Healing);
        };
        // Down twice to "use", a confirm, a confirm on the first item, and a pass on
        // the target.
        for buttons in [pad::DOWN, 0, pad::DOWN, pad::CROSS, 0, pad::CROSS, 0] {
            healing(&mut menu, buttons);
        }
        assert_eq!(menu.drew().level, level::TARGET);
        assert_eq!(menu.drew().picks, Some(0));
        let items = Items::default();
        let width = |units: &[u16]| 6 * units.len() as i16;
        let picture = |menu: &Menu| {
            drawn(
                menu,
                &tables(),
                (&[], &items, None),
                &[(0, true)],
                7,
                |_| 30,
                width,
            )
        };
        // The player alone: his name's frame, his name, and the two small frames; no
        // root and no list.
        let alone = picture(&menu);
        let frame = |x: i16, w: i16| {
            Drawn::Listed(player_panel::Drawn::Frame {
                rect: Rect {
                    x,
                    y: 152,
                    w,
                    h: 12,
                },
                mode: 1,
                time: 7,
            })
        };
        assert_eq!(alone.len(), 4);
        assert_eq!(alone[0], frame(46, 60));
        assert!(matches!(
            alone[1],
            Drawn::Listed(player_panel::Drawn::PickedName {
                target: 0,
                dim: true,
                ..
            })
        ));
        assert_eq!(alone[2..], [frame(38, 6), frame(108, 6)]);
        // A refused confirm: from the pass behind it the message stands in the menus'
        // box and the picker is left out with the root.
        healing(&mut menu, pad::CROSS);
        assert_eq!(picture(&menu).len(), 4);
        healing(&mut menu, 0);
        assert_eq!(menu.drew().picks, None);
        let refused = picture(&menu);
        assert_eq!(
            refused,
            [
                Drawn::Frame {
                    rect: message::BOX,
                    mode: 0,
                    time: 0
                },
                Drawn::Text {
                    text: usize::from(text::NO_NEED),
                    at: (14, 158)
                }
            ]
        );
    }

    /// A creature whose rows all pass.
    struct Creature;

    impl Asked for Creature {
        fn item(&self, _: usize) -> Result<Listed, Refused> {
            Err(None)
        }
        fn attack(&self, row: usize) -> Result<Listed, Refused> {
            Ok(Listed {
                parameter: row as u32,
                class: super::super::menu::class::OPPONENTS,
            })
        }
        fn spell(&self, _: usize) -> Result<Listed, Refused> {
            Ok(Listed {
                parameter: 11,
                class: super::super::menu::class::OPPONENTS,
            })
        }
        fn own(&self, _: u8, _: u32, _: i16) -> usize {
            1
        }
        fn target(&self, _: u8, _: u32, _: usize) -> Result<(), Refused> {
            Ok(())
        }
        fn described(&self, level: u8, line: usize) -> bool {
            level == level::SPELLS && line == 0
        }
    }

    #[test]
    fn the_lists_of_a_creature_stand_beside_its_root_with_the_text_of_a_row() {
        let piece = |x: i16, w: i16| Piece {
            flags: 0,
            kind: kind::PICTURE,
            value: 0,
            rect: Rect { x, y: 0, w, h: 21 },
            margin: (0, 0),
        };
        let shapes = Shapes {
            attacks: vec![vec![piece(56, 38), piece(94, 93), piece(187, 78)]; attacks::ROWS],
            spells: vec![vec![piece(96, 21), piece(117, 107)]; spells::ROWS],
            attack_pictures: [[1; 5]; attacks::ROWS],
            spell_pictures: [2; spells::SPELLS],
        };
        let mut rows = [AttackRow::default(); attacks::ROWS];
        rows[1] = AttackRow {
            known: true,
            attack: 5,
            ..AttackRow::default()
        };
        let none = spells::SPELLS;
        let moves = Moves {
            attacks: rows,
            spells: SpellRows {
                rows: [11, none, none, none],
                short: [false; spells::ROWS],
                known: 1,
            },
        };
        let creature = || {
            let mut tables = tables();
            tables.lines[1] = tables.lines[0][..5].to_vec();
            tables.help[1] = (0..5).collect();
            let menu = Menu::new(Offer {
                fighter: 1,
                carried: 0b10,
                opponents: 0b0001_0000,
                boss: false,
                no_cocoon: true,
                no_retreat: false,
                places: 0,
                spells: 1,
                kept: 0,
                duel: None,
            });
            (tables, menu)
        };
        let go = |menu: &mut Menu, buttons: u16| {
            let mut pad = Pad::default();
            pad.read(buttons, Repeat::of(2), 0);
            menu.pass(&pad, &Creature);
        };
        let picture = |menu: &Menu, tables: &Tables| {
            let (items, width) = (Items::default(), |units: &[u16]| 6 * units.len() as i16);
            let held = (&[][..], &items, Some((&shapes, &moves)));
            drawn(menu, tables, held, &[], 7, |_| 30, width)
        };
        let listed = |drawn: &[Drawn]| {
            let listed = drawn.iter().filter_map(|drawn| match drawn {
                Drawn::Listed(listed) => Some(*listed),
                _ => None,
            });
            listed.collect::<Vec<_>>()
        };
        let frames = |listed: &[player_panel::Drawn]| {
            let frames = listed.iter().filter_map(|listed| match *listed {
                player_panel::Drawn::Frame { rect, time, .. } => Some((rect.x, rect.y, time)),
                _ => None,
            });
            frames.collect::<Vec<_>>()
        };
        // "Special" is one down: the root's five lines and the five rows of the body
        // attacks where the player's menu has them, the row under the cursor pulsing.
        let (tables, mut menu) = creature();
        for buttons in [pad::DOWN, pad::CROSS, 0] {
            go(&mut menu, buttons);
        }
        assert_eq!(menu.drew().level, level::ATTACKS);
        let special = picture(&menu, &tables);
        assert_eq!(special.len(), 10 + 30);
        assert_eq!(frames(&listed(&special))[0], (56, 44, 7));
        // The circle on a row that is not known: the root goes and no text comes; on
        // the known row below it the text of attack 5 lies on the list.
        go(&mut menu, pad::CIRCLE);
        assert_eq!(picture(&menu, &tables).len(), 30);
        go(&mut menu, pad::DOWN | pad::CIRCLE);
        go(&mut menu, pad::CIRCLE);
        let described = listed(&picture(&menu, &tables));
        assert_eq!(described.len(), 30 + 5);
        assert_eq!(frames(&described)[3], (56, 66, 7));
        assert_eq!(
            described[32],
            player_panel::Drawn::ListDescribed {
                what: player_panel::Listed::Attack(5),
                line: 1,
                at: (14, 170)
            }
        );
        // "Magic" is one down and one right: the four rows of the spells from y 69.
        let (tables, mut menu) = creature();
        for buttons in [pad::DOWN, 0, pad::RIGHT, pad::CROSS, 0] {
            go(&mut menu, buttons);
        }
        assert_eq!(menu.drew().level, level::SPELLS);
        let magic = picture(&menu, &tables);
        assert_eq!(magic.len(), 10 + 8 + 2);
        assert_eq!(frames(&listed(&magic))[..2], [(96, 69, 7), (117, 69, 7)]);
        go(&mut menu, pad::CIRCLE);
        let described = listed(&picture(&menu, &tables));
        assert_eq!(described.len(), 10 + 5);
        assert_eq!(
            described[11],
            player_panel::Drawn::ListDescribed {
                what: player_panel::Listed::Spell(11),
                line: 0,
                at: (14, 158)
            }
        );
    }

    #[test]
    fn the_list_of_the_items_stands_with_the_root_and_a_description_takes_the_root_away() {
        let mut menu = menu();
        // Down twice to "use" and a confirm, then a pass on the list.
        for buttons in [pad::DOWN, 0, pad::DOWN, pad::CROSS, 0] {
            go(&mut menu, buttons);
        }
        assert_eq!(menu.drew().level, level::ITEMS);
        // A list of four rows of three pieces, with two items in it.
        let piece = |w: i16| Piece {
            flags: 0,
            kind: kind::PICTURE,
            value: 0,
            rect: Rect {
                x: 0,
                y: 0,
                w,
                h: 21,
            },
            margin: (0, 0),
        };
        let list = vec![vec![piece(21), piece(112), piece(23)]; items::ROWS];
        let row = |item: u8| ItemRow {
            item,
            count: 1,
            refused: false,
        };
        let held = Items {
            rows: [row(5), row(7), ItemRow::default(), ItemRow::default()],
            last_place: 1,
        };
        let width = |units: &[u16]| 6 * units.len() as i16;
        let picture =
            |menu: &Menu| drawn(menu, &tables(), (&list, &held, None), &[], 7, |_| 30, width);
        let listed = |drawn: &[Drawn]| {
            let listed = drawn.iter().filter_map(|drawn| match drawn {
                Drawn::Listed(listed) => Some(*listed),
                _ => None,
            });
            listed.collect::<Vec<_>>()
        };
        let plain = picture(&menu);
        // The root's six lines, then the list: twelve frames and the content of two rows.
        assert_eq!(plain.len(), 12 + 12 + 6);
        let rows = listed(&plain);
        assert_eq!(
            rows[0],
            player_panel::Drawn::Frame {
                rect: Rect {
                    x: 83,
                    y: 54,
                    w: 21,
                    h: 21
                },
                mode: 1,
                time: 7
            }
        );
        // The circle: the root is left out, the list stays, and the box of the item's
        // four lines lies on top.
        go(&mut menu, pad::CIRCLE);
        let described = picture(&menu);
        assert_eq!(listed(&described).len(), 18);
        assert_eq!(described.len(), 18 + 5);
        assert_eq!(
            described[18],
            Drawn::Frame {
                rect: message::BOX,
                mode: 0,
                time: 0
            }
        );
        assert_eq!(
            described[20],
            Drawn::Described {
                item: 5,
                line: 1,
                at: (14, 170)
            }
        );
    }
}
