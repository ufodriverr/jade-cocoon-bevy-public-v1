
use crate::battle::record::{LEVEL, Record};
use crate::menu::{Piece, Rect, Style, kind, piece, placed, sign};
use crate::player_menu::{
    CAUGHT, CAUGHT_ACROSS, CAUGHT_RECORDS, FIRST_LEVEL, ITEMS, LEAVE_FOREST, MAGIC, OF_CREATURE,
    OF_PLAYER, PlayerMenu, SPECIAL, STORY, STORY_THINGS, Spot, TARGET, caught_rows, flag,
};
use crate::rooms::Image;
use crate::window::{Way, opened};

/// The bit of the player's panel and of the carried creatures' panel.
pub const PLAYER_PANEL: u32 = 0x02;
pub const CREATURES_PANEL: u32 = 0x04;

/// The passes of a box's opening.
const OPENING: i32 = 8;

/// The sprites of the menus' pool the panel draws.
pub mod sprite {
    pub const NAME: usize = 17;
    pub const HP: usize = 18;
    pub const CAPTURE_LEVEL: usize = 112;
    pub const EXPERIENCE: usize = 226;
    pub const EMPTIES: usize = 130;
    pub const FIREFLIES: usize = 82;
    pub const CAPTURED: usize = 111;
    pub const STATUS: usize = 187;
    pub const OPTIONS: usize = 79;
    /// The word of the box of the corridor of the second round.
    pub const CORRIDOR: usize = 58;
    /// The stroke between the two numbers of a line.
    pub const STROKE: usize = 218;
    /// The frame of a gauge of kind 0, and of the kinds 1 and 2.
    pub const LONG_GAUGE: usize = 106;
    pub const SHORT_GAUGE: usize = 105;
    /// The digit 0 of a number and of a small number; the nine others follow.
    pub const DIGITS: usize = 132;
    pub const SMALL_DIGITS: usize = 207;
    /// The stone of the wall behind the boxes, 128 by 60.
    pub const WALL: usize = 149;
}

/// The wall behind the boxes (`0x8002BE6C`, the first call of every pass of the menu):
/// where the sprite [`sprite::WALL`] is put, four rows of three, every other row half
/// a stone to the left. It goes into the slot behind the frames' and is not opened
/// with the boxes: it is whole from the first drawing on.
pub fn wall() -> impl Iterator<Item = (i16, i16)> {
    (0..4i16).flat_map(|row| {
        let shift = if row & 1 != 0 { 64 } else { 0 };
        (0..3i16).map(move |column| (column * 128 - shift, row * 60))
    })
}

/// The images of the fonts' file a bar is made of: the usual one, the one of a value
/// under a quarter, and the one of the experience.
pub mod bar {
    pub const USUAL: usize = 2;
    pub const LOW: usize = 66;
    pub const EXPERIENCE: usize = 20;
    /// The rows of a bar: the first column of its image has a colour in three of them.
    pub const HIGH: usize = 3;
}

/// The two frames of the player and the corner his values are counted from.
pub const LEFT_FRAME: Rect = rect(10, 44, 68, 68);
pub const RIGHT_FRAME: Rect = rect(78, 44, 141, 68);
const CORNER: (i16, i16) = (78, 44);
/// The face, two pixels inside the left frame.
const FACE_AT: (i16, i16) = (12, 46);
/// The heading and the third row of screen 0.
pub const HEADING: Rect = rect(139, 18, 43, 19);
pub const THIRD_ROW: Rect = rect(125, 193, 70, 19);
/// The box of the corridor of the second round on screen 0 (the rectangle at
/// `0x800C2B7C` of the menu's file), where its word starts and its number ends.
pub const CORRIDOR_BOX: Rect = rect(10, 15, 79, 28);
const CORRIDOR_WORD: (i16, i16) = (15, 19);
const CORRIDOR_NUMBER: (i16, i16) = (84, 25);
/// The three small boxes, from the top: 91 wide at x 219, by y and height.
const SMALL_BOXES: [(i16, i16); 3] = [(44, 23), (67, 22), (89, 23)];
const SMALL_BOX: (i16, i16) = (219, 91);
/// Where a small box's word starts and its number ends, and how far below the box's
/// upper edge they stand.
const SMALL_WORD: (i16, i16) = (224, 5);
const SMALL_NUMBER: (i16, i16) = (306, 6);

/// The highest value a number and a small number show.
const HIGHEST: i32 = 9_999_999;
const HIGHEST_SMALL: i32 = 999;
const HIGHEST_LONG: i32 = 999_999_999;

const fn rect(x: i16, y: i16, w: i16, h: i16) -> Rect {
    Rect { x, y, w, h }
}

/// One thing of the panel, as a painter draws it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drawn {
    /// A frame of the menus in the window colour, half of what is under it and half its
    /// colour (`0x8002A3EC`): mode 0 the plain colour, mode 1 the colour that pulses by
    /// the time.
    Frame {
        rect: Rect,
        mode: u8,
        time: u32,
    },
    /// A sprite of the menus' pool, its upper left corner at the place.
    Sprite {
        number: usize,
        at: (i16, i16),
        clip: Rect,
    },
    /// A number whose last digit ends at the place ([`digits`]).
    Number {
        value: i32,
        ends: (i16, i16),
        small: bool,
        clip: Rect,
    },
    /// The bar of a gauge: an image of the fonts' file, `length` pixels long.
    Bar {
        picture: usize,
        length: i16,
        at: (i16, i16),
        clip: Rect,
    },
    /// The player's name in the font of the texts.
    Name {
        at: (i16, i16),
        clip: Rect,
    },
    /// The player's face, 64 by 64.
    Face {
        at: (i16, i16),
        clip: Rect,
    },
    /// The name of a carried creature in the small font, in capitals
    /// ([`in_capitals`]).
    CreatureName {
        creature: usize,
        at: (i16, i16),
        clip: Rect,
    },
    /// The name of the player's rank in the font of the texts.
    Rank {
        at: (i16, i16),
        clip: Rect,
    },
    /// The small picture of what he has on in a place (the weapon, the armour, the
    /// third thing), an image of the fonts' file, and its name in the font of the texts.
    WornPicture {
        place: usize,
        at: (i16, i16),
        clip: Rect,
    },
    WornName {
        place: usize,
        at: (i16, i16),
        clip: Rect,
    },
    /// The text of the message that is up, in the font of the texts ([`message_text`]).
    Message {
        at: (i16, i16),
    },
    /// The name of a carried creature in the font of the texts, as it is written.
    FullName {
        creature: usize,
        at: (i16, i16),
        clip: Rect,
    },
    /// A flat triangle in one colour, nothing blended.
    Triangle {
        corners: [(i16, i16); 3],
        colour: [u8; 3],
        clip: Rect,
    },
    /// A line in a grey, both of its ends drawn.
    Line {
        from: (i16, i16),
        to: (i16, i16),
        grey: u8,
        clip: Rect,
    },
    /// A scroll sign of a list (`crate::menu::sign`), turned by quarter turns about
    /// its place and pulsing by the time.
    Sign {
        turns: u16,
        at: (i16, i16),
        time: u32,
    },
    /// A sign of another family of the signs' images than a list's
    /// (`crate::menu::sign::image`), drawn the same way.
    SignOf {
        family: u16,
        turns: u16,
        at: (i16, i16),
        time: u32,
    },
    /// The small picture of an item, an image of the fonts' file, at half its
    /// brightness when dim; and its name in the font of the texts, in the font's second
    /// row of colours when dim.
    ItemPicture {
        item: u8,
        at: (i16, i16),
        dim: bool,
        clip: Rect,
    },
    ItemName {
        item: u8,
        at: (i16, i16),
        dim: bool,
        clip: Rect,
    },
    /// A number in the large digits as a line of a menu draws it, under the colour of
    /// a dim piece ([`dim_digits`]) when dim.
    Count {
        value: i32,
        ends: (i16, i16),
        dim: bool,
        clip: Rect,
    },
    /// The player's name in the small font, in capitals, as the head of a fighter's
    /// box has it.
    SmallName {
        at: (i16, i16),
        clip: Rect,
    },
    /// The name of a target of the picker, 0 the player and n a carried creature, in
    /// the small font and in capitals, about its middle (the name's width halved
    /// towards zero), in the font's second row of colours when dim.
    PickedName {
        target: usize,
        middle: (i16, i16),
        dim: bool,
        clip: Rect,
    },
    /// A line of an item's description in the font of the texts
    /// ([`description_text`]).
    Described {
        item: u8,
        line: usize,
        at: (i16, i16),
    },
    /// The small picture of a thing of the story and its name, as an item's.
    ThingPicture {
        thing: i16,
        at: (i16, i16),
        dim: bool,
        clip: Rect,
    },
    ThingName {
        thing: i16,
        at: (i16, i16),
        dim: bool,
        clip: Rect,
    },
    /// A line of the text of a thing of the story in the font of the texts
    /// ([`thing_text`]).
    ThingDescribed {
        thing: i16,
        line: usize,
        at: (i16, i16),
    },
    /// A text of the question whether to leave the forest in the font of the texts
    /// ([`question_texts`]): 0 the question, 1 its two answers.
    Asked {
        text: usize,
        at: (i16, i16),
    },
    /// A piece of one of a creature's lists as the shared drawer puts it into its
    /// room: at the left, at the right or in the middle by the piece's flags
    /// ([`crate::menu::placed`]), a picture at half its brightness and a text in the
    /// font's second row of colours when dim.
    Listed {
        what: Listed,
        room: Rect,
        flags: u16,
        dim: bool,
        clip: Rect,
    },
    /// A line of the text of a spell or of a body attack in the font of the texts
    /// ([`spell_text`], [`attack_text`]).
    ListDescribed {
        what: Listed,
        line: usize,
        at: (i16, i16),
    },
    /// The name of a caught creature that waits, by its place among the twelve, in the
    /// small font and in capitals.
    CaughtName {
        caught: usize,
        at: (i16, i16),
        clip: Rect,
    },
    /// The name of an ancestor of a carried creature ([`Family`]) in the small font,
    /// in capitals.
    AncestorName {
        creature: usize,
        ancestor: usize,
        at: (i16, i16),
    },
    /// The small picture of a weapon, an armour or a third thing (kind 0, 1 or 2), an
    /// image of the fonts' file, and its name in the font of the texts; dim as an
    /// item's.
    GearPicture {
        kind: usize,
        thing: u8,
        at: (i16, i16),
        dim: bool,
        clip: Rect,
    },
    GearName {
        kind: usize,
        thing: u8,
        at: (i16, i16),
        dim: bool,
        clip: Rect,
    },
    /// A line of the description of such a thing in the font of the texts
    /// (`crate::equip_panel::description_text`).
    GearDescribed {
        kind: usize,
        thing: u8,
        line: usize,
        at: (i16, i16),
    },
    /// The line that says a thing cannot be taken off, in the font of the texts
    /// (`crate::equip_panel::refusal_text`).
    GearRefused {
        at: (i16, i16),
    },
    /// A frame as [`Drawn::Frame`], in one of the four window colours instead of the
    /// one in use (`crate::options_panel`, the page of the colours).
    FrameOf {
        rect: Rect,
        colour: i16,
        mode: u8,
        time: u32,
    },
    /// A sprite of the menus' pool at half its brightness: the picture of a dim piece.
    DimSprite {
        number: usize,
        at: (i16, i16),
        clip: Rect,
    },
    /// The line of help of a line of a page of the options screen, and a line of one
    /// of its two refusals, in the font of the texts (`crate::options_panel::Texts`).
    OptionsHelp {
        page: usize,
        line: usize,
        at: (i16, i16),
    },
    OptionsRefused {
        which: usize,
        line: usize,
        at: (i16, i16),
    },
    /// The arrow of a difference (`0x80032050` with the number 88): image 88 of the
    /// fonts' file around the place, turned about it by quarter turns as a scroll sign
    /// is (`crate::menu::sign::texel`), in its own colours.
    Arrow {
        turns: u16,
        at: (i16, i16),
    },
    /// A number in the large digits under an entry of the executable's colours of a
    /// number ([`digit_colour`]); entry 0 leaves the digits as they are.
    Tinted {
        value: i32,
        ends: (i16, i16),
        colour: usize,
        clip: Rect,
    },
    /// The mark of a cell of the list of the caught kinds, a text of the menu's file in
    /// the small font (`crate::caught_list::mark_text`), and the name of a species
    /// there, in the small font and in capitals (`crate::caught_list::kind_name`).
    KindMark {
        at: (i16, i16),
    },
    KindName {
        species: u32,
        at: (i16, i16),
    },
    /// A number in the small digits with up to nine of them ([`long_digits`]).
    LongNumber {
        value: i32,
        ends: (i16, i16),
        clip: Rect,
    },
    /// The frame of a window of the texts about a rectangle (`crate::window::frame`),
    /// and its fill, half of what is under it and half the colour of the window colour
    /// (`crate::window::fill`). The screen of the blacksmith and the shop has them
    /// (`crate::shop_panel`).
    WindowFrame {
        rect: Rect,
    },
    WindowFill {
        rect: Rect,
    },
    /// The face of the keeper of that screen, 64 by 64: the face of thing 1 of the room.
    KeeperFace {
        at: (i16, i16),
    },
    /// One of the three texts of the keeper's line in the font of the texts
    /// (`crate::shop_panel::keeper_texts`).
    KeeperSaid {
        line: i16,
        text: usize,
        at: (i16, i16),
    },
    /// The name of the player's rank on his panel of a level up ([`rank_panel`]), in the
    /// font of the texts: the name of the rank before the counting or of the rank now,
    /// every colour of it at 128 less `dark` of 128.
    RankName {
        before: bool,
        dark: i16,
        at: (i16, i16),
    },
    /// The name of the creature of a place of the list of the screen of file 44
    /// (`crate::merge_panel`), in the small font and in capitals, about its middle (the
    /// name's width halved towards zero); in the font's fourth row of colours when the
    /// place is marked.
    PlaceName {
        place: usize,
        middle: (i16, i16),
        marked: bool,
    },
    /// The sign of "more" of a message window (`crate::window::more_brightness`), its
    /// upper left corner at the place.
    More {
        at: (i16, i16),
    },
}

/// What a piece of a creature's lists shows: a sprite of the menus' pool, or the name
/// of a spell, of a body attack or of an attack's grade in the font of the texts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Listed {
    Sprite(usize),
    Spell(usize),
    Attack(usize),
    Grade(usize),
}

/// The whole screen, for what is cut to nothing.
const SCREEN: Rect = rect(0, 0, 320, 240);

/// The box of a carried creature by how many are carried (the tables behind
/// `0x80082DB4`): its x. Every box is 100 by 77 at y 114.
pub const CREATURE_BOXES: [&[i16]; 3] = [&[110], &[60, 160], &[10, 110, 210]];
const CREATURE_BOX: (i16, i16, i16) = (114, 100, 77);

/// The sprites of a creature's box.
pub mod creature {
    pub const LEVEL: usize = 120;
    pub const MANA: usize = 113;
    pub const EXP: usize = 59;
    /// The sign of an element the creature has, of one it has not, and of no element
    /// at all, in the order of the element bits: fire, wind, earth, water.
    pub const HAS: [usize; 4] = [61, 231, 101, 222];
    pub const HAS_NOT: [usize; 4] = [80, 95, 204, 121];
    pub const NONE: usize = 129;
    /// Where the four signs stand from the sign's place.
    pub const SIGNS: [(i16, i16); 4] = [(14, 0), (14, 14), (0, 14), (0, 0)];
}

/// The executable's table of the species' names: the address of a text for each.
const SPECIES_NAMES: u32 = 0x8007_A094;
/// The letters a record's name holds.
const NAME_LETTERS: usize = 10;

/// The name of a species from the loaded executable, as far as a record's name can
/// hold it: the name a new creature's record gets (the set-up `0x8001900C` copies it
/// from this table through `0x8003E560`, which keeps ten letters).
pub fn species_name(executable: &[u8], base: u32, species: u32) -> Option<Vec<u16>> {
    let image = Image {
        data: executable,
        base,
    };
    let entry = crate::rooms::step(SPECIES_NAMES, species as usize, 4).ok()?;
    let offset = image.u32(entry).ok()?.checked_sub(base)? as usize;
    let mut units = crate::text::units(executable.get(offset..)?);
    units.truncate(NAME_LETTERS);
    Some(units)
}

/// A unit of a creature's name as its box shows it (`0x8003E634`): a small letter of
/// the texts becomes its capital.
pub fn in_capitals(unit: u16) -> u16 {
    match unit.to_le_bytes() {
        [0x82, second @ 0x81..=0x9A] => u16::from_le_bytes([0x82, second - 0x21]),
        _ => unit,
    }
}

/// The sign of a creature's element (`0x8002B284`): four sprites, the letter of the
/// element it has and a bent arrow for each it has not, or four dashes without one.
pub fn element_sign(bits: u16, at: (i16, i16), clip: Rect) -> [Drawn; 4] {
    std::array::from_fn(|element| {
        let number = match (bits, bits >> element & 1) {
            (0, _) => creature::NONE,
            (_, 1) => creature::HAS[element],
            _ => creature::HAS_NOT[element],
        };
        let (right, down) = creature::SIGNS[element];
        Drawn::Sprite {
            number,
            at: (at.0.wrapping_add(right), at.1.wrapping_add(down)),
            clip,
        }
    })
}

/// The line of a creature's HP or mana (`0x8002F020`): a short gauge and the two small
/// numbers 8 right of and 7 below its place.
pub fn line(values: (i16, i16), at: (i16, i16), clip: Rect) -> [Drawn; 5] {
    let [frame, bar] = gauge(1, values.0.into(), values.1.into(), at, clip);
    let [first, stroke, second] = pair(values, (at.0.wrapping_add(8), at.1.wrapping_add(7)), clip);
    [frame, bar, first, stroke, second]
}

/// What panel 2 draws in a pass (`0x800BF1A4` with `0x8002F940` and `0x8002ED3C`): a
/// box for every carried creature, from what lies underneath to what lies on top. In a
/// box, from the top: the name with the element's sign beside it, the HP, the level,
/// the mana and the experience. More than three creatures are not carried.
pub fn creature_boxes(menu: &PlayerMenu, creatures: &[Record], passes: u32) -> Vec<Drawn> {
    let openness = i32::from(menu.openness_of(CREATURES_PANEL));
    let in_row = menu.screen == FIRST_LEVEL && menu.flags & flag::CREATURES_ROW != 0;
    let under_cursor = |n: usize| in_row && usize::try_from(menu.cursor.at) == Ok(n);
    let creatures: Vec<&Record> = creatures.iter().collect();
    boxes_of_creatures(openness, under_cursor, &creatures, passes)
}

/// Where the boxes of this many carried creatures stand, from the left (`0x8002FA68`).
fn creature_places(carried: usize) -> &'static [i16] {
    let places = carried
        .checked_sub(1)
        .and_then(|count| CREATURE_BOXES.get(count));
    places.copied().unwrap_or(&[])
}

/// The boxes of the carried creatures (`0x8002F940` with `0x8002ED3C`), the frame of
/// the ones `lit` names pulsing by the pass counter.
fn boxes_of_creatures(
    openness: i32,
    lit: impl Fn(usize) -> bool,
    creatures: &[&Record],
    passes: u32,
) -> Vec<Drawn> {
    let mut drawn = Vec::new();
    for ((n, record), &x) in (creatures.iter().enumerate()).zip(creature_places(creatures.len())) {
        let (y, w, h) = CREATURE_BOX;
        let clip = opened(rect(x, y, w, h), openness, OPENING, Way::Middle);
        let under_cursor = lit(n);
        let from = |right: i16, down: i16| (x + right, y + down);
        let word = |number: usize, at: (i16, i16)| Drawn::Sprite { number, at, clip };
        drawn.extend([
            Drawn::Frame {
                rect: clip,
                mode: 1,
                time: if under_cursor { passes } else { 0 },
            },
            Drawn::CreatureName {
                creature: n,
                at: from(7, 3),
                clip,
            },
        ]);
        drawn.extend(line((record.hp, record.max_hp), from(3, 11), clip));
        drawn.extend(element_sign(record.element_bit, from(69, 4), clip));
        drawn.extend([
            word(creature::LEVEL, from(16, 27)),
            Drawn::Number {
                value: record.level.into(),
                ends: from(55, 29),
                small: true,
                clip,
            },
            word(creature::MANA, from(7, 40)),
        ]);
        drawn.extend(line((record.mp, record.max_mp), from(32, 39), clip));
        drawn.push(word(creature::EXP, from(7, 55)));
        let way = experience_shown(record.experience);
        drawn.extend(gauge(2, way, LEVEL as i32, from(32, 66), clip));
    }
    drawn
}

/// Where the spots of the cursor lie on the screen that is up, each with a rectangle
/// that shows it, for a pointer of the build (the game has none). `entries` are the
/// lines of the player's and of a creature's entries, `items` and `story` those of the
/// two lists, and `moves` those of a creature's spells and of its body attacks. A
/// screen not here has none: the caught creatures' screen, whose cursor is the first
/// row it shows, has nothing to point at.
pub fn spots(
    menu: &PlayerMenu,
    entries: [&[Vec<Piece>]; 2],
    (items, story): (&[Vec<Piece>], &[Vec<Piece>]),
    [spell_lines, attack_lines]: [&[Vec<Piece>]; 2],
) -> Vec<(Spot, Rect)> {
    let spot = |rows: Vec<(i16, Rect)>| -> Vec<(Spot, Rect)> {
        rows.into_iter().map(|(at, rect)| ((0, at), rect)).collect()
    };
    let lines = |lines: &[Vec<Piece>]| spot(line_rects(lines));
    let rows = |lines: &[Vec<Piece>], starts: &[i16], first_row: i16, step: i16| {
        spot(row_rects(lines, starts, first_row, step))
    };
    match menu.screen {
        FIRST_LEVEL => {
            let small = SMALL_BOXES.map(|(y, h)| rect(SMALL_BOX.0, y, SMALL_BOX.1, h));
            let player = [LEFT_FRAME, RIGHT_FRAME].into_iter().chain(small);
            let mut spots: Vec<(Spot, Rect)> = player.map(|rect| ((0, 0), rect)).collect();
            let carried = usize::try_from(menu.carried).unwrap_or(0);
            let (y, w, h) = CREATURE_BOX;
            let places = creature_places(carried).iter().zip(0..);
            spots.extend(places.map(|(&x, at)| ((1, at), rect(x, y, w, h))));
            spots.push(((2, 0), THIRD_ROW));
            spots
        }
        OF_PLAYER => lines(entries[0]),
        OF_CREATURE => lines(entries[1]),
        ITEMS => rows(items, &items::PIECES, items::FIRST_ROW, items::STEP),
        STORY => rows(story, &story::PIECES, story::FIRST_ROW, story::STEP),
        MAGIC => rows(
            spell_lines,
            &spells::PIECES,
            spells::FIRST_ROW,
            spells::STEP,
        ),
        SPECIAL => rows(
            attack_lines,
            &attacks::PIECES,
            attacks::FIRST_ROW,
            attacks::STEP,
        ),
        TARGET => {
            let carried = usize::try_from(menu.carried.clamp(0, 3)).unwrap_or(0);
            let rows: Vec<(usize, bool)> = (0..=carried).map(|target| (target, false)).collect();
            spot(picker_rects(&rows))
        }
        LEAVE_FOREST => (0..2)
            .map(|at| {
                let x = story::ANSWER.x + story::ANSWER_STEP * at;
                ((0, at), Rect { x, ..story::ANSWER })
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Each line of a menu with the rectangle around its pieces, for a pointer of the
/// build.
pub fn line_rects(lines: &[Vec<Piece>]) -> Vec<(i16, Rect)> {
    (lines.iter().zip(0..))
        .filter_map(|(line, at)| {
            crate::menu::bounds(line.iter().map(|piece| piece.rect)).map(|rect| (at, rect))
        })
        .collect()
}

/// Each row of a list the drawer places itself (the items, the things of the story, a
/// creature's two lists) with the rectangle around its pieces, which start at `starts`
/// across and `first_row` and `step` down: for a pointer of the build.
pub fn row_rects(
    lines: &[Vec<Piece>],
    starts: &[i16],
    first_row: i16,
    step: i16,
) -> Vec<(i16, Rect)> {
    (lines.iter().zip(0..))
        .filter_map(|(line, at)| {
            let y = first_row + step * at;
            let pieces = (line.iter().zip(starts)).map(|(piece, &x)| Rect { x, y, ..piece.rect });
            crate::menu::bounds(pieces).map(|rect| (at, rect))
        })
        .collect()
}

/// Each row of the picker of a target ([`picker_rows`]) with its frame, for a pointer
/// of the build.
pub fn picker_rects(rows: &[(usize, bool)]) -> Vec<(i16, Rect)> {
    let mut creatures = 0;
    (rows.iter().zip(0..))
        .map(|(&(target, _), at)| {
            let y = match target {
                0 => picker::NAME.y,
                _ => {
                    creatures += 1;
                    picker::FIRST + picker::STEP * (creatures - 1)
                }
            };
            (at, Rect { y, ..picker::NAME })
        })
        .collect()
}

/// The three counts of the small boxes: the empty cocoons, the caught creatures that
/// wait, and the kinds caught so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    pub cocoons: i32,
    pub waiting: i32,
    pub kinds: i32,
    /// The floor of the corridor of the second round while its generator lives (bit
    /// `0x00020000` of the engine's word with the getter `0x800B34DC` of scene
    /// package 114), for the box "Corr." of screen 0.
    pub floor: Option<i32>,
}

/// How long the bar of a gauge is (`0x8002BCBC`): the value, no more than the most, of
/// the most in pixels of the longest bar, and at least one pixel for a value that is not
/// zero.
pub fn bar_length(value: u32, most: u32, longest: u32) -> i16 {
    let value = value.min(most);
    if value == 0 {
        return 0;
    }
    (value.wrapping_mul(longest) / most).max(1) as i16
}

/// A gauge at a place (`0x8002BAB0`, without a change to come): its frame, and the bar
/// that lies on it 8 right and 2 below. Kind 0 is the long gauge, kind 1 the short one,
/// kind 2 the short one with the bar of the experience.
pub fn gauge(kind: u8, value: i32, most: i32, at: (i16, i16), clip: Rect) -> [Drawn; 2] {
    // A quarter as the routine takes it: towards zero.
    let low = value < most / 4;
    let (frame, longest, picture) = match (kind, low) {
        (2, _) => (sprite::SHORT_GAUGE, 47, bar::EXPERIENCE),
        (0, false) => (sprite::LONG_GAUGE, 87, bar::USUAL),
        (0, true) => (sprite::LONG_GAUGE, 87, bar::LOW),
        (_, false) => (sprite::SHORT_GAUGE, 47, bar::USUAL),
        (_, true) => (sprite::SHORT_GAUGE, 47, bar::LOW),
    };
    [
        Drawn::Sprite {
            number: frame,
            at,
            clip,
        },
        Drawn::Bar {
            picture,
            length: bar_length(value as u32, most as u32, longest),
            at: (at.0.wrapping_add(8), at.1.wrapping_add(2)),
            clip,
        },
    ]
}

/// The way of an experience word to its next level, as the gauge shows it: the levels
/// that wait are taken off.
pub fn experience_shown(experience: u32) -> i32 {
    let mut value = experience as i32;
    while value > LEVEL as i32 {
        value = value.wrapping_sub(LEVEL as i32);
    }
    value
}

/// The sprites of a number and where they go (`0x8002C15C`, `0x8002C2EC`): the digits
/// from the right, digit k standing k + 1 times "its sprite's width less one" left of
/// the place. `width` gives the width of a sprite of the pool. A value below zero does
/// not occur.
pub fn digits(
    value: i32,
    ends: (i16, i16),
    small: bool,
    width: impl Fn(usize) -> i16,
) -> Vec<(usize, (i16, i16))> {
    let (first, highest) = match small {
        true => (sprite::SMALL_DIGITS, HIGHEST_SMALL),
        false => (sprite::DIGITS, HIGHEST),
    };
    digits_from(first, highest, value, ends, width)
}

/// The sprites of a long number and where they go (`0x8002C470`): the small digits as
/// [`digits`] places them, up to nine of them.
pub fn long_digits(
    value: i32,
    ends: (i16, i16),
    width: impl Fn(usize) -> i16,
) -> Vec<(usize, (i16, i16))> {
    digits_from(sprite::SMALL_DIGITS, HIGHEST_LONG, value, ends, width)
}

fn digits_from(
    first: usize,
    highest: i32,
    value: i32,
    ends: (i16, i16),
    width: impl Fn(usize) -> i16,
) -> Vec<(usize, (i16, i16))> {
    let mut left = value.clamp(0, highest);
    let mut placed = Vec::new();
    loop {
        let number = first + (left % 10) as usize;
        let step = i32::from(width(number)) - 1;
        let x = i32::from(ends.0) - (placed.len() as i32 + 1) * step;
        placed.push((number, (x as i16, ends.1)));
        left /= 10;
        if left == 0 {
            break placed;
        }
    }
}

/// The two numbers of a line with the stroke between them (`0x8002EE84` with small
/// numbers): the first ends 21 right of the place, where the stroke starts, and the
/// second 48 right.
fn pair(values: (i16, i16), at: (i16, i16), clip: Rect) -> [Drawn; 3] {
    let right = |by: i16| (at.0.wrapping_add(by), at.1);
    [
        Drawn::Number {
            value: values.0.into(),
            ends: right(21),
            small: true,
            clip,
        },
        Drawn::Sprite {
            number: sprite::STROKE,
            at: right(21),
            clip,
        },
        Drawn::Number {
            value: values.1.into(),
            ends: right(48),
            small: true,
            clip,
        },
    ]
}

/// What panel 1 draws in a pass (`0x800BEEDC` with `0x8002E390` and `0x8002FD68`), from
/// what lies underneath to what lies on top. `passes` is the runner's pass counter, the
/// time the frames pulse by while the cursor is in their row.
pub fn player_panel(menu: &PlayerMenu, player: &Record, counts: Counts, passes: u32) -> Vec<Drawn> {
    let openness = i32::from(menu.openness_of(PLAYER_PANEL));
    let open = |whole: Rect| opened(whole, openness, OPENING, Way::Middle);
    let first_level = menu.screen == FIRST_LEVEL;
    let in_row = first_level && menu.flags & flag::PLAYER_ROW != 0;
    let time = if in_row { passes } else { 0 };
    let mut drawn = player_boxes(openness, 1, time, player, counts);

    // The heading and the third row, on the first level only.
    if first_level {
        let clip = open(HEADING);
        drawn.extend([
            Drawn::Frame {
                rect: clip,
                mode: 0,
                time: 0,
            },
            Drawn::Sprite {
                number: sprite::STATUS,
                at: (147, 21),
                clip,
            },
        ]);
        let clip = open(THIRD_ROW);
        let in_row = menu.flags & flag::THIRD_ROW != 0;
        drawn.extend([
            Drawn::Frame {
                rect: clip,
                mode: 1,
                time: if in_row { passes } else { 0 },
            },
            Drawn::Sprite {
                number: sprite::OPTIONS,
                at: (132, 196),
                clip,
            },
        ]);
        // The box of the corridor of the second round: a plain frame as the
        // heading's, its word and the floor's number in the large digits.
        if let Some(floor) = counts.floor {
            let clip = open(CORRIDOR_BOX);
            drawn.extend([
                Drawn::Frame {
                    rect: clip,
                    mode: 0,
                    time: 0,
                },
                Drawn::Sprite {
                    number: sprite::CORRIDOR,
                    at: CORRIDOR_WORD,
                    clip,
                },
                Drawn::Number {
                    value: floor,
                    ends: CORRIDOR_NUMBER,
                    small: false,
                    clip,
                },
            ]);
        }
    }
    drawn
}

/// The player as panel 1 and his "Detail" both draw him (`0x8002E390`, `0x8002FD68`):
/// the three small boxes, the left frame with the face and the right frame with his
/// values, every frame in the given mode and with the given time.
fn player_boxes(openness: i32, mode: u8, time: u32, player: &Record, counts: Counts) -> Vec<Drawn> {
    let open = |whole: Rect| opened(whole, openness, OPENING, Way::Middle);
    let from = |right: i16, down: i16| (CORNER.0 + right, CORNER.1 + down);
    let mut drawn = Vec::new();

    // The three small boxes.
    let small = [
        (sprite::EMPTIES, counts.cocoons),
        (sprite::FIREFLIES, counts.waiting),
        (sprite::CAPTURED, counts.kinds),
    ];
    for ((y, h), (word, value)) in SMALL_BOXES.into_iter().zip(small) {
        let clip = open(rect(SMALL_BOX.0, y, SMALL_BOX.1, h));
        drawn.extend([
            Drawn::Frame {
                rect: clip,
                mode,
                time,
            },
            Drawn::Sprite {
                number: word,
                at: (SMALL_WORD.0, y + SMALL_WORD.1),
                clip,
            },
            Drawn::Number {
                value,
                ends: (SMALL_NUMBER.0, y + SMALL_NUMBER.1),
                small: false,
                clip,
            },
        ]);
    }

    // The left frame with the face.
    let clip = open(LEFT_FRAME);
    drawn.extend([
        Drawn::Frame {
            rect: clip,
            mode,
            time,
        },
        Drawn::Face { at: FACE_AT, clip },
    ]);

    // The right frame with the name, the HP, the capture level and the experience.
    let clip = open(RIGHT_FRAME);
    let word = |number: usize, at: (i16, i16)| Drawn::Sprite { number, at, clip };
    drawn.extend([
        Drawn::Frame {
            rect: clip,
            mode,
            time,
        },
        word(sprite::NAME, from(4, 5)),
        Drawn::Name {
            at: from(37, 6),
            clip,
        },
        word(sprite::HP, from(4, 21)),
    ]);
    let (hp, most) = (player.hp, player.max_hp);
    drawn.extend(gauge(0, hp.into(), most.into(), from(34, 20), clip));
    drawn.extend(pair((hp, most), from(60, 28), clip));
    drawn.extend([
        word(sprite::CAPTURE_LEVEL, from(4, 36)),
        Drawn::Number {
            value: player.level.into(),
            ends: from(128, 37),
            small: false,
            clip,
        },
        word(sprite::EXPERIENCE, from(4, 52)),
    ]);
    let way = experience_shown(player.experience);
    drawn.extend(gauge(2, way, LEVEL as i32, from(75, 54), clip));
    drawn
}

/// The two small frames of the results screen with their sprites (`0x800BCA78`,
/// `0x800BCB04`): the player's above his right frame, a creature's below its box, this
/// far right of the box's left edge.
const GAINED: (usize, (i16, i16), Rect) = (228, (111, 28), rect(108, 26, 108, 18));
const CREATURE_GAINED: (usize, (i16, i16), (i16, i16, i16, i16)) =
    (93, (14, 193), (8, 191, 84, 18));

/// A fighter of the results screen: the record its box is drawn from (its working copy
/// until it is gone through, its own record behind that), and whether it has gained a
/// level and is not gone through yet.
#[derive(Debug, Clone, Copy)]
pub struct Counted<'a> {
    pub record: &'a Record,
    pub gained: bool,
}

pub fn results_boxes(
    player: Counted,
    creatures: &[Counted],
    counts: Counts,
    passes: u32,
) -> Vec<Drawn> {
    let mut drawn = Vec::new();
    let small = |sprite: usize, at: (i16, i16), frame: Rect| {
        [
            Drawn::Frame {
                rect: frame,
                mode: 1,
                time: passes,
            },
            Drawn::Sprite {
                number: sprite,
                at,
                clip: frame,
            },
        ]
    };
    // A box at an x of 0 has no small frame.
    let places = creature_places(creatures.len());
    for (creature, &x) in creatures.iter().zip(places).rev() {
        if creature.gained && x != 0 {
            let (sprite, at, (left, y, w, h)) = CREATURE_GAINED;
            drawn.extend(small(sprite, (x + at.0, at.1), rect(x + left, y, w, h)));
        }
    }
    if player.gained {
        let (sprite, at, frame) = GAINED;
        drawn.extend(small(sprite, at, frame));
    }
    let records: Vec<&Record> = creatures.iter().map(|creature| creature.record).collect();
    let lit = |n: usize| creatures[n].gained;
    drawn.extend(boxes_of_creatures(OPENING, lit, &records, passes));
    let time = if player.gained { passes } else { 0 };
    drawn.extend(player_boxes(OPENING, 1, time, player.record, counts));
    drawn
}

/// The places of the player's panel of a level up (`0x800BCD68`): the frame of his face
/// and the face, and the two rows, each a frame, where its word stands and where its
/// text starts.
const RANK_FACE: (Rect, (i16, i16)) = (rect(126, 52, 68, 68), (128, 54));
type RankRow = (Rect, (i16, i16), (i16, i16));
const RANK_NAME_ROW: RankRow = (rect(92, 123, 137, 21), (97, 128), (129, 129));
const RANK_RANK_ROW: RankRow = (rect(92, 147, 137, 21), (97, 153), (129, 153));

pub fn rank_panel(before: bool, dark: i16) -> Vec<Drawn> {
    let plain = |rect: Rect| Drawn::Frame {
        rect,
        mode: 0,
        time: 0,
    };
    let (name_frame, name_at, name) = RANK_NAME_ROW;
    let (rank_frame, rank_at, rank) = RANK_RANK_ROW;
    let (face_frame, face) = RANK_FACE;
    vec![
        plain(rank_frame),
        Drawn::Sprite {
            number: detail::RANK,
            at: rank_at,
            clip: SCREEN,
        },
        Drawn::RankName {
            before,
            dark,
            at: rank,
        },
        plain(name_frame),
        Drawn::Name {
            at: name,
            clip: SCREEN,
        },
        Drawn::Sprite {
            number: sprite::NAME,
            at: name_at,
            clip: SCREEN,
        },
        plain(face_frame),
        Drawn::Face {
            at: face,
            clip: SCREEN,
        },
    ]
}

/// The bits of the two rows of entries and of the player's "Detail".
pub const PLAYER_ENTRIES: u32 = 0x01;
pub const PLAYER_DETAIL: u32 = 0x20;
pub const CREATURE_ENTRIES: u32 = 0x80;

/// The lists of the lines of the two rows of entries in the menu's file, one line an
/// entry, and the flag word of the menu record they are drawn with (`0x800BBD4C`),
/// whose low three bits the panel sets itself.
pub const PLAYER_LINES: u32 = 0x800C_2858;
pub const CREATURE_LINES: u32 = 0x800C_2950;
const ENTRIES_FLAGS: u32 = 0x02;
/// The frame style of a row whose own screen is up, and of a row behind another screen.
const OWN_SCREEN: u32 = 0x02;
const BEHIND: u32 = 0x04;

/// A row of entries (`0x800C09E8` the player's, `0x800C0AB8` a creature's): the lines of
/// its list through the shared menu drawer, each a framed word. While the row's own
/// screen is up the frame under the cursor pulses; behind another screen that entry has
/// the marked colour and the others the plain one. The cursor is the one the screen
/// last kept. `width` gives the width of a sprite of the pool.
pub fn entries(
    menu: &PlayerMenu,
    of_creature: bool,
    lines: &[Vec<Piece>],
    time: u32,
    width: impl Fn(usize) -> i16,
) -> Vec<Drawn> {
    let (panel, own, cursor) = match of_creature {
        true => (CREATURE_ENTRIES, OF_CREATURE, menu.entry_of_creature()),
        false => (PLAYER_ENTRIES, OF_PLAYER, menu.entry_of_player()),
    };
    let openness = i32::from(menu.openness_of(panel));
    let state = if menu.screen == own {
        OWN_SCREEN
    } else {
        BEHIND
    };
    let flags = ENTRIES_FLAGS & !0x07 | state;
    let mut drawn = Vec::new();
    for (line, pieces) in lines.iter().enumerate() {
        let on_cursor = usize::try_from(cursor) == Ok(line);
        let (style, pulses) = Style::of_line(flags, on_cursor, 0);
        for piece in pieces {
            let clip = opened(piece.rect, openness, OPENING, Way::Middle);
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
                    clip,
                });
            }
        }
    }
    drawn
}

/// The sprites and places of the player's "Detail".
pub mod detail {
    use super::{Rect, rect};

    /// The five numbers (`0x8002F82C`): their words, and how far below the place each
    /// row stands (the bytes at `0x80082DE0` and `0x80082E28`).
    pub const WORDS: [usize; 5] = [98, 21, 109, 107, 184];
    pub const ROWS: [i16; 5] = [3, 17, 31, 45, 59];
    pub const NUMBERS: Rect = rect(209, 136, 75, 77);
    /// The words of what he has on, a row each (the words at `0x800C194C`), the list of
    /// their lines in the menu's file, and the place of the first row; the rows are 22
    /// apart, and behind the word's frame come the picture's 50 and the name's 71 right.
    pub const WORN: [usize; 3] = [22, 20, 3];
    pub const WORN_LINES: u32 = 0x800C_1AA8;
    pub const WORN_AT: (i16, i16) = (10, 136);
    pub const WORN_STEP: i16 = 22;
    pub const WORN_PIECES: [i16; 3] = [0, 50, 71];
    /// The rank and the money with their boxes.
    pub const RANK: usize = 194;
    pub const RANK_BOX: Rect = rect(10, 113, 137, 21);
    pub const MONEY: usize = 123;
    pub const YAN: usize = 233;
    pub const MONEY_BOX: Rect = rect(149, 113, 141, 21);
    /// The executable's table of the ranks' names: the address of a text for each.
    pub const RANK_NAMES: u32 = 0x8008_2DE8;
    pub const RANKS: usize = 9;
}

/// The player's rank by his level (`0x800121A8`): a quarter of a level under 20, then
/// 5 up to level 30, 6 up to 34, 7 up to 39 and 8 from 40.
pub fn rank(level: u8) -> usize {
    match level {
        0..=19 => usize::from(level / 4),
        20..=30 => 5,
        31..=34 => 6,
        35..=39 => 7,
        _ => 8,
    }
}

/// The name of a rank from the loaded executable (`0x8002FD34`): a rank past the table
/// has the last name.
pub fn rank_name(executable: &[u8], base: u32, rank: usize) -> Option<Vec<u16>> {
    let image = Image {
        data: executable,
        base,
    };
    let entry = crate::rooms::step(detail::RANK_NAMES, rank.min(detail::RANKS - 1), 4).ok()?;
    let offset = image.u32(entry).ok()?.checked_sub(base)? as usize;
    Some(crate::text::units(executable.get(offset..)?))
}

/// What the player's "Detail" shows beyond his record: the first five of the six values
/// a fight counts with (his own with what he has on added, `0x80033680`), whether a
/// thing is on in each of the three places, and his money.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Detail {
    pub values: [u8; 5],
    pub worn: [bool; 3],
    pub money: i32,
}

/// What panel 5 draws in a pass (`0x800BF2EC`), from what lies underneath to what lies
/// on top: the player as panel 1 draws him, in plain frames; the box of the five
/// numbers (`0x8002F82C`); the three lines of what he has on (`0x800C0FFC`, `lines`
/// being the lines of [`detail::WORN_LINES`]); the rank and the money in a box each.
/// The panel opens with the row of entries, whose openness it asks for.
pub fn player_detail(
    menu: &PlayerMenu,
    player: &Record,
    counts: Counts,
    shown: &Detail,
    lines: &[Vec<Piece>],
    width: impl Fn(usize) -> i16,
) -> Vec<Drawn> {
    let openness = i32::from(menu.openness_of(PLAYER_ENTRIES));
    let open = |whole: Rect| opened(whole, openness, OPENING, Way::Middle);
    let plain = |rect: Rect| Drawn::Frame {
        rect,
        mode: 0,
        time: 0,
    };
    let mut drawn = Vec::new();

    // The money.
    let clip = open(detail::MONEY_BOX);
    drawn.extend([
        plain(clip),
        Drawn::Sprite {
            number: detail::YAN,
            at: (260, 118),
            clip,
        },
        Drawn::Number {
            value: shown.money,
            ends: (260, 118),
            small: false,
            clip,
        },
        Drawn::Sprite {
            number: detail::MONEY,
            at: (154, 117),
            clip,
        },
    ]);

    // The rank.
    let clip = open(detail::RANK_BOX);
    drawn.extend([
        plain(clip),
        Drawn::Rank {
            at: (47, 119),
            clip,
        },
        Drawn::Sprite {
            number: detail::RANK,
            at: (15, 119),
            clip,
        },
    ]);

    // What he has on. The drawer's flag word is 1, the plain colour, and it has no cursor.
    drawn.extend(worn_rows(
        detail::WORN_AT,
        lines,
        shown.worn,
        (0x01, None, 0),
        openness,
        &width,
    ));

    drawn.extend(five_numbers(detail::NUMBERS, shown.values, openness));
    drawn.extend(player_boxes(openness, 0, 0, player, counts));
    drawn
}

/// The three lines of what the player has on (`0x800C0FFC`), the first at `at`: a
/// word, the thing's small picture and its name, each in a frame of its own, through
/// the shared menu drawer. `lines` are the lines of [`detail::WORN_LINES`], `worn` says
/// in which places a thing is on, and `state` is the drawer's flag word, the line its
/// cursor is on and the time a pulsing frame is drawn with.
pub(crate) fn worn_rows(
    at: (i16, i16),
    lines: &[Vec<Piece>],
    worn: [bool; 3],
    (flags, cursor, time): (u32, Option<usize>, u32),
    openness: i32,
    width: &impl Fn(usize) -> i16,
) -> Vec<Drawn> {
    let mut drawn = Vec::new();
    for (row, pieces) in lines.iter().enumerate().take(detail::WORN.len()) {
        let y = at.1 + detail::WORN_STEP * row as i16;
        let (style, pulses) = Style::of_line(flags, cursor == Some(row), 0);
        for (n, (piece, right)) in pieces.iter().zip(detail::WORN_PIECES).enumerate() {
            let whole = Rect {
                x: at.0 + right,
                y,
                ..piece.rect
            };
            let clip = opened(whole, openness, OPENING, Way::Middle);
            if piece.flags & piece::BARE == 0 {
                drawn.push(Drawn::Frame {
                    rect: clip,
                    mode: style.mode,
                    time: if pulses { time } else { 0 },
                });
            }
            let room = whole.inside(piece.margin);
            match n {
                0 => {
                    let number = detail::WORN[row];
                    drawn.push(Drawn::Sprite {
                        number,
                        at: placed(piece.flags, room, width(number)),
                        clip,
                    });
                }
                // A picture and a text start at the left edge of their room.
                1 if worn[row] => drawn.push(Drawn::WornPicture {
                    place: row,
                    at: (room.x, room.y),
                    clip,
                }),
                2 if worn[row] => drawn.push(Drawn::WornName {
                    place: row,
                    at: (room.x, room.y),
                    clip,
                }),
                _ => {}
            }
        }
    }
    drawn
}

/// The player's five numbers in their box (`0x8002F82C` in a box of `0x8002BF24`): a
/// plain frame as far as it stands open, and in it a word and a number a row.
pub(crate) fn five_numbers(whole: Rect, values: [u8; 5], openness: i32) -> Vec<Drawn> {
    let clip = opened(whole, openness, OPENING, Way::Middle);
    let mut drawn = vec![Drawn::Frame {
        rect: clip,
        mode: 0,
        time: 0,
    }];
    for ((word, down), value) in (detail::WORDS.into_iter()).zip(detail::ROWS).zip(values) {
        drawn.extend([
            Drawn::Sprite {
                number: word,
                at: (whole.x + 6, whole.y + down + 1),
                clip,
            },
            Drawn::Number {
                value: value.into(),
                ends: (whole.x + 69, whole.y + down + 2),
                small: false,
                clip,
            },
        ]);
    }
    drawn
}

/// The bit of the panel beside the player's entry "Equip" (panel 12), and its box
/// (the rectangle at `0x800C1ABC` of the menu's file).
pub const EQUIP_PANEL: u32 = 0x1000;
pub const EQUIP_BOX_AT: u32 = 0x800C_1ABC;
pub const EQUIP_BOX: Rect = rect(194, 49, 75, 77);

/// Where the three lines of what he has on stand beside the entry "Equip" (the two
/// half words panel 12 writes at `0x800BEA6C` and `0x800BEA74`): the place they have
/// on the screen behind the entry.
pub const EQUIP_WORN_AT: (i16, i16) = (10, 61);

/// What panel 12 draws in a pass (`0x800BEA58`), from what lies underneath to what
/// lies on top: the three lines of what he has on (`0x800C0FFC`, `lines` being the
/// lines of [`detail::WORN_LINES`]), plain and without a cursor, whole from the first
/// pass (the routine is handed the openness 8); and the player's five numbers
/// (`0x800C0770`), as his "Detail" has them, in a box right under the entries, which
/// opens with the row of entries, whose openness it asks for.
pub fn equip_panel(
    menu: &PlayerMenu,
    shown: &Detail,
    lines: &[Vec<Piece>],
    width: impl Fn(usize) -> i16,
) -> Vec<Drawn> {
    let openness = i32::from(menu.openness_of(PLAYER_ENTRIES));
    let mut drawn = worn_rows(
        EQUIP_WORN_AT,
        lines,
        shown.worn,
        (0x01, None, 0),
        crate::player_menu::OPEN.into(),
        &width,
    );
    drawn.extend(five_numbers(EQUIP_BOX, shown.values, openness));
    drawn
}

/// The bit of a creature's "Detail".
pub const CREATURE_DETAIL: u32 = 0x40;

/// The sprites, places and tables of a creature's "Detail" (`0x8002DBA4`). A place is
/// counted from the corner of the panel's box.
pub mod creature_detail {
    use super::{Rect, rect};

    pub const BOX: Rect = rect(51, 62, 218, 117);
    /// The words on the left (`0x8002DA98`): a sprite and its place.
    pub const WORDS: [(usize, (i16, i16)); 5] = [
        (17, (3, 4)),
        (92, (4, 17)),
        (60, (4, 30)),
        (84, (1, 55)),
        (114, (1, 86)),
    ];
    pub const NAME: (i16, i16) = (38, 5);
    pub const LEVEL_ENDS: (i16, i16) = (66, 18);
    /// The places of the differences of the level, of the most HP and of the most
    /// mana to what they were (`0x8002DBA4` with a record to compare with).
    pub const LEVEL_CHANGE: (i16, i16) = (67, 20);
    pub const HP_CHANGE: (i16, i16) = (47, 58);
    pub const MANA_CHANGE: (i16, i16) = (47, 89);
    pub const EXPERIENCE: (i16, i16) = (3, 43);
    pub const HP: (i16, i16) = (3, 69);
    pub const MANA: (i16, i16) = (3, 99);
    /// The five values: the rows of their words (the bytes at `0x80082E0C`), which
    /// start 78 right, and where the numbers end, a pixel below their row.
    pub const ROWS: [i16; 5] = [35, 49, 70, 84, 102];
    pub const VALUE_WORDS: i16 = 78;
    pub const VALUES_END: i16 = 140;

    /// The wheel of the elements (`0x8002E78C`): its corner, its picture, and for each
    /// quarter (upper left, upper right, lower right, lower left) the letter's sprite
    /// (`0x80082DC8`), the colour of the triangle (`0x80082DCC`) and the element whose
    /// strength it shows (`0x80082DDC`).
    pub const WHEEL: (i16, i16) = (162, 4);
    pub const WHEEL_PICTURE: usize = 11;
    pub const LETTERS: [usize; 4] = [222, 61, 231, 101];
    pub const COLOURS: [[u8; 3]; 4] = [[89, 82, 218], [159, 29, 29], [212, 142, 5], [59, 137, 63]];
    pub const ELEMENTS: [usize; 4] = [3, 0, 1, 2];
    /// A strength under this counts as none; what is left is halved.
    pub const LEAST: u8 = 8;
    /// The length of a triangle's arms by the halved strength (`0x80082E14`).
    pub const LENGTHS: [u8; 17] = [
        0, 6, 8, 10, 12, 13, 15, 16, 17, 18, 19, 20, 21, 22, 22, 23, 24,
    ];
    /// Half the wheel: where its quarters meet, and how far a letter of the near and
    /// of the far side stands from the wheel's corner.
    pub const HALF: i16 = 26;
    pub const LETTER_NEAR: i16 = 2;
    pub const LETTER_FAR: i16 = 37;

    /// The square of the build (`0x8002EB28`): its corner, the marker, the picture with
    /// the four words, the two outlines (how far inside, their side, their grey) and
    /// the grey of the cross.
    pub const BUILD: (i16, i16) = (162, 61);
    pub const MARKER: usize = 10;
    pub const BUILD_PICTURE: usize = 12;
    /// How far the marker can go (`0x8001419C`).
    pub const REACH: u32 = 47;
    pub const OUTLINES: [(i16, i16, u8); 2] = [(0, 50, 117), (1, 48, 189)];
    pub const CROSS: u8 = 96;
}

/// Where the marker of the build stands in its square (`0x8001419C`), from the growth
/// of the five values: right for speed against the two defenses, down for the magic
/// attack against the attack. A part whose growths are all zero is taken as 0 here;
/// what the console's division does with it was not read.
pub fn build_marker(growth: [u8; 6]) -> (i16, i16) {
    let [attack, defense, magic, magic_defense, speed, _] = growth.map(u32::from);
    let part = |of: u32, against: u32| {
        (creature_detail::REACH * of)
            .checked_div(of + against)
            .unwrap_or(0) as i16
    };
    (
        part(speed, (defense + magic_defense) >> 1),
        part(magic, attack),
    )
}

/// The length of the triangle of a strength in the wheel (`0x800338C0`, then the table
/// of `0x8002E9A0`): none under 8, else the table's entry for half of it. A strength
/// past the table has no triangle here; the console reads behind the table.
pub fn wheel_length(strength: u8) -> i16 {
    if strength < creature_detail::LEAST {
        return 0;
    }
    let lengths = creature_detail::LENGTHS;
    (lengths.get(usize::from(strength >> 1))).map_or(0, |&length| length.into())
}

/// The triangle of a quarter of the wheel (`0x8002E9A0`): its right angle where the
/// quarters meet and its two other corners `length` out on the quarter's two arms, the
/// arms that point up or left a pixel further, but for the left arm of the last.
pub fn wheel_triangle(quarter: usize, length: i16, wheel: (i16, i16)) -> [(i16, i16); 3] {
    let half = creature_detail::HALF;
    let (x, y) = (wheel.0 + half, wheel.1 + half);
    let (left, up) = (x - 1 - length, y - 1 - length);
    let (right, down) = (x + length, y + length);
    match quarter {
        0 => [(x, y), (x, up), (left, y)],
        1 => [(x, y), (x, up), (right, y)],
        2 => [(x, y), (x, down), (right, y)],
        _ => [(x, y), (x, down), (x - length, y)],
    }
}

/// What panel 6 draws in a pass (`0x800BE844`, `0x800BEDF4`, `0x8002DBA4`) for the
/// carried creature `creature`, from what lies underneath to what lies on top: the
/// box, the five values a fight counts with (`values`), the square of the build, the
/// wheel of the elements, the three gauges, the level, the name and the words.
pub fn creature_detail(
    menu: &PlayerMenu,
    creature: usize,
    record: &Record,
    values: [u8; 5],
) -> Vec<Drawn> {
    let openness = i32::from(menu.openness_of(CREATURE_DETAIL));
    let clip = opened(creature_detail::BOX, openness, OPENING, Way::Middle);
    detail(clip, clip, creature, (record, values), None)
}

/// The "Detail" of a creature in the box of another screen (`0x8002DBA4` with that
/// box's corner): the plain frame `frame`, and every place counted from its corner
/// and cut to `clip`. The screen of file 44 has it (`crate::merge_panel`), where
/// `creature` is the creature's place, and where the new creature of a merge is
/// compared with `before`, the first of the two and its values.
pub fn detail_in(
    frame: Rect,
    clip: Rect,
    creature: usize,
    (record, values): (&Record, [u8; 5]),
    before: Option<(&Record, [u8; 5])>,
) -> Vec<Drawn> {
    let by = (
        frame.x - creature_detail::BOX.x,
        frame.y - creature_detail::BOX.y,
    );
    let moved = |(x, y): (i16, i16)| (x + by.0, y + by.1);
    let back = Rect {
        x: clip.x - by.0,
        y: clip.y - by.1,
        ..clip
    };
    let drawn = detail(
        creature_detail::BOX,
        back,
        creature,
        (record, values),
        before,
    );
    let put = |rect: Rect| Rect {
        x: rect.x + by.0,
        y: rect.y + by.1,
        ..rect
    };
    (drawn.into_iter())
        .map(|piece| match piece {
            Drawn::Frame { rect, mode, time } => Drawn::Frame {
                rect: put(rect),
                mode,
                time,
            },
            Drawn::Sprite { number, at, clip } => Drawn::Sprite {
                number,
                at: moved(at),
                clip: put(clip),
            },
            Drawn::Number {
                value,
                ends,
                small,
                clip,
            } => Drawn::Number {
                value,
                ends: moved(ends),
                small,
                clip: put(clip),
            },
            Drawn::Bar {
                picture,
                length,
                at,
                clip,
            } => Drawn::Bar {
                picture,
                length,
                at: moved(at),
                clip: put(clip),
            },
            Drawn::FullName { creature, at, clip } => Drawn::FullName {
                creature,
                at: moved(at),
                clip: put(clip),
            },
            Drawn::Triangle {
                corners,
                colour,
                clip,
            } => Drawn::Triangle {
                corners: corners.map(moved),
                colour,
                clip: put(clip),
            },
            Drawn::Line {
                from,
                to,
                grey,
                clip,
            } => Drawn::Line {
                from: moved(from),
                to: moved(to),
                grey,
                clip: put(clip),
            },
            Drawn::Arrow { turns, at } => Drawn::Arrow {
                turns,
                at: moved(at),
            },
            Drawn::Tinted {
                value,
                ends,
                colour,
                clip,
            } => Drawn::Tinted {
                value,
                ends: moved(ends),
                colour,
                clip: put(clip),
            },
            other => other,
        })
        .collect()
}

/// A creature's panel of a level up on the results screen (`0x800BD070`): the "Detail"
/// of the carried creature `creature`, whole at once and cut by nothing, in a plain
/// frame, compared with the creature as it was before the counting. `now` and `before`
/// are the record and its five values as a fight counts them.
pub fn level_panel(
    creature: usize,
    now: (&Record, [u8; 5]),
    before: (&Record, [u8; 5]),
) -> Vec<Drawn> {
    detail(creature_detail::BOX, WHOLE, creature, now, Some(before))
}

/// The whole screen: the rectangle of what nothing cuts.
const WHOLE: Rect = rect(0, 0, 320, 240);

/// The difference of a value to what it was (`0x8002E060(place, was - is, slot)`):
/// nothing for none; else the arrow, 3 right of and 3 below the place (5 left of it
/// for a difference of 100 or more), turned down for a value that fell and up for one
/// that rose, and how much in the small digits, ending 20 right of the place.
fn difference(place: (i16, i16), was: i32, is: i32, clip: Rect) -> Vec<Drawn> {
    let change = was - is;
    if change == 0 {
        return Vec::new();
    }
    let x = if change.abs() < 100 {
        place.0 + 3
    } else {
        place.0 - 5
    };
    vec![
        Drawn::Arrow {
            turns: if change > 0 { sign::BELOW } else { sign::ABOVE },
            at: (x, place.1 + 3),
        },
        Drawn::Number {
            value: change.abs(),
            ends: (place.0 + 20, place.1),
            small: true,
            clip,
        },
    ]
}

/// A number of the "Detail" that is compared (`0x8002C62C` in front of it): under
/// entry 1 of the digits' colours when it rose, under entry 2 when it fell, else as
/// it is.
fn compared(value: i32, was: i32, ends: (i16, i16), clip: Rect) -> Drawn {
    Drawn::Tinted {
        value,
        ends,
        colour: match was.cmp(&value) {
            std::cmp::Ordering::Less => 1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 2,
        },
        clip,
    }
}

/// The "Detail" of a creature (`0x8002DBA4`) in the frame `frame`, cut to `clip`; with
/// `before`, the record and the values it is compared with, the level and the five
/// values are tinted by their change and the level, the most HP, the most mana and
/// the five values have their differences beside them.
fn detail(
    frame: Rect,
    clip: Rect,
    creature: usize,
    (record, values): (&Record, [u8; 5]),
    before: Option<(&Record, [u8; 5])>,
) -> Vec<Drawn> {
    use creature_detail as of;
    let from = |(right, down): (i16, i16)| (of::BOX.x + right, of::BOX.y + down);
    let word = |number: usize, at: (i16, i16)| Drawn::Sprite { number, at, clip };
    let mut drawn = vec![Drawn::Frame {
        rect: frame,
        mode: 0,
        time: 0,
    }];

    // The five numbers, each a pixel below the row of its word, and the difference
    // of each two more below.
    for (n, (row, value)) in of::ROWS.into_iter().zip(values).enumerate() {
        let ends = from((of::VALUES_END, row + 1));
        let value = i32::from(value);
        match before {
            None => drawn.push(Drawn::Number {
                value,
                ends,
                small: false,
                clip,
            }),
            Some((_, were)) => {
                let was = i32::from(were[n]);
                drawn.push(compared(value, was, ends, clip));
                let place = from((of::VALUES_END, row + 3));
                drawn.extend(difference(place, was, value, clip));
            }
        }
    }

    // The square of the build: the cross underneath, the outlines, the picture with
    // the four words and the marker on top.
    let build = from(of::BUILD);
    let stroke = |a: (i16, i16), b: (i16, i16), grey: u8| Drawn::Line {
        from: (build.0 + a.0, build.1 + a.1),
        to: (build.0 + b.0, build.1 + b.1),
        grey,
        clip,
    };
    drawn.extend([
        stroke((25, 2), (25, 50), of::CROSS),
        stroke((2, 26), (50, 26), of::CROSS),
    ]);
    for (inside, side, grey) in of::OUTLINES.into_iter().rev() {
        let (near, far) = (inside, inside + side);
        drawn.extend([
            stroke((near, near), (near, far), grey),
            stroke((near, near), (far, near), grey),
            stroke((far, near), (far, far), grey),
            stroke((far, far), (near, far), grey),
        ]);
    }
    let marker = build_marker(record.growth);
    drawn.extend([
        word(of::BUILD_PICTURE, (build.0 + 2, build.1 + 2)),
        word(of::MARKER, (build.0 + marker.0, build.1 + marker.1)),
    ]);

    // The wheel: its picture, and for each quarter from the last to the first the
    // triangle of the element's strength with the letter over it.
    let wheel = from(of::WHEEL);
    drawn.push(word(of::WHEEL_PICTURE, wheel));
    for quarter in (0..4).rev() {
        let length = wheel_length(record.strengths[of::ELEMENTS[quarter]]);
        if length != 0 {
            drawn.push(Drawn::Triangle {
                corners: wheel_triangle(quarter, length, wheel),
                colour: of::COLOURS[quarter],
                clip,
            });
        }
        let far = |is: bool| if is { of::LETTER_FAR } else { of::LETTER_NEAR };
        let at = (
            wheel.0 + far(quarter == 1 || quarter == 2),
            wheel.1 + far(quarter >= 2),
        );
        drawn.push(word(of::LETTERS[quarter], at));
    }

    // The experience, the mana and the HP.
    let way = experience_shown(record.experience);
    drawn.extend(gauge(2, way, LEVEL as i32, from(of::EXPERIENCE), clip));
    drawn.extend(line((record.mp, record.max_mp), from(of::MANA), clip));
    drawn.extend(line((record.hp, record.max_hp), from(of::HP), clip));
    let level = i32::from(record.level);
    if let Some((was, _)) = before {
        let pairs = [
            (of::MANA_CHANGE, was.max_mp, record.max_mp),
            (of::HP_CHANGE, was.max_hp, record.max_hp),
        ];
        for (place, was, is) in pairs {
            drawn.extend(difference(from(place), was.into(), is.into(), clip));
        }
        drawn.extend(difference(
            from(of::LEVEL_CHANGE),
            was.level.into(),
            level,
            clip,
        ));
    }
    drawn.extend([
        match before {
            None => Drawn::Number {
                value: level,
                ends: from(of::LEVEL_ENDS),
                small: false,
                clip,
            },
            Some((was, _)) => compared(level, was.level.into(), from(of::LEVEL_ENDS), clip),
        },
        Drawn::FullName {
            creature,
            at: from(of::NAME),
            clip,
        },
    ]);

    // The words: the five of the values, then the five on the left.
    for (number, row) in detail::WORDS.into_iter().zip(of::ROWS) {
        drawn.push(word(number, from((of::VALUE_WORDS, row))));
    }
    for (number, at) in of::WORDS {
        drawn.push(word(number, from(at)));
    }
    drawn
}

/// The message of a refused confirm (`0x800BF98C`, through the executable's
/// `0x8002AF70`).
pub mod message {
    use super::{Rect, rect};

    /// The box, and how far inside it the first of its four lines starts; the lines are
    /// 12 apart, and only the first has a text.
    pub const BOX: Rect = rect(10, 153, 300, 60);
    pub const TEXT: (i16, i16) = (4, 5);
    pub const LINE: i16 = 12;
    pub const LINES: usize = 4;
    /// The menu file's table of the results' texts: the address of a text for each.
    pub const TEXTS: u32 = 0x800C_1AF4;
    /// The results that have an entry in the table.
    pub const RESULTS: i16 = 14;
}

/// What the message of the pass draws, from what lies underneath to what lies on top:
/// a plain frame that does not open, and the text of the result. The message's routine
/// runs behind the drawing of the panels and so adds its things last: they lie under
/// everything else of the pass. Nothing for a pass without a message, and nothing for
/// the result that shows a thing's own text, which comes with the things of the story.
pub fn message(menu: &PlayerMenu) -> Option<[Drawn; 2]> {
    (1..message::RESULTS).contains(&menu.said).then_some([
        Drawn::Frame {
            rect: message::BOX,
            mode: 0,
            time: 0,
        },
        Drawn::Message {
            at: (
                message::BOX.x + message::TEXT.0,
                message::BOX.y + message::TEXT.1,
            ),
        },
    ])
}

/// The text of a result from the menu's file, whose first byte is at `base`.
pub fn message_text(file: &[u8], base: u32, result: i16) -> Option<Vec<u16>> {
    let result = usize::try_from(result).ok()?;
    if !(1..message::RESULTS as usize).contains(&result) {
        return None;
    }
    let image = Image { data: file, base };
    let entry = crate::rooms::step(message::TEXTS, result, 4).ok()?;
    let offset = image.u32(entry).ok()?.checked_sub(base)? as usize;
    Some(crate::text::units(file.get(offset..)?))
}

/// The bit of the list of the items (panel 11).
pub const ITEMS_PANEL: u32 = 0x800;

/// The places and tables of the list of the items (`0x8002C784`, `0x800C0620`).
pub mod items {
    use super::{Rect, rect};

    /// The executable's list of the four lines the list is drawn with, a row each. The
    /// drawer writes the places of their pieces in every pass.
    pub const LINES: u32 = 0x8008_290C;
    pub const ROWS: usize = 4;
    /// Where the three pieces of a row start (the picture, the name, the count), the
    /// first row's y and the step of a row.
    pub const PIECES: [i16; 3] = [83, 104, 216];
    pub const FIRST_ROW: i16 = 54;
    pub const STEP: i16 = 22;
    /// The items that have a name, a picture and a description.
    pub const ITEMS: u8 = 43;
    /// The scroll signs above and below the list (the places at `0x80082744`).
    pub const SIGNS: [(i16, i16); 2] = [(160, 47), (160, 148)];
    /// The executable's colours of the large digits, four bytes each (`0x8002C62C`),
    /// and the entry of a dim piece; entry 0 leaves them as they are.
    pub const DIGIT_COLOURS: u32 = 0x8008_261C;
    pub const DIM_DIGITS: usize = 3;
    /// The box of who gets the item (the rectangle at `0x800C1AEC` of the menu's file).
    pub const WHO: Rect = rect(110, 167, 100, 41);
    /// The word "Capture" in front of the player's "Level".
    pub const CAPTURE: usize = 225;
    /// The menu file's table of the descriptions: the addresses of four lines for
    /// each item.
    pub const DESCRIPTIONS: u32 = 0x800C_1DE0;
    /// The passes a gain is seen on a gauge and the passes it is not: bit `0x10` of
    /// the pass counter.
    pub const BLINK: u32 = 0x10;
}

/// A place of the player's list as a row of the list shows it: the item, how many of
/// it lie there, and whether the test of the row refuses it now (`0x80033B58`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ItemRow {
    pub item: u8,
    pub count: u8,
    pub refused: bool,
}

/// What the list of the items draws in a pass (`0x8002C784`), from what lies underneath
/// to what lies on top: the two scroll signs, then four rows through the shared menu
/// drawer, each a picture, a name and a count in a frame of their own. `rows` are the
/// places from the kept first line on, `last_place` the last place in use. The list
/// does not open: it is whole in its first pass. On its own screen the frames under
/// the cursor pulse, behind the player's entries all are plain, and behind another
/// screen the cursor's row has the marked colour. A refused row is dim, and an empty
/// place has its frames alone.
pub fn item_list(
    menu: &PlayerMenu,
    lines: &[Vec<Piece>],
    rows: [ItemRow; items::ROWS],
    last_place: i16,
    time: u32,
) -> Vec<Drawn> {
    let flags = match menu.screen {
        ITEMS => 0x02,
        OF_PLAYER => 0x01,
        _ => 0x04,
    };
    let first = menu.first_item();
    let cursor = menu.item_place().wrapping_sub(first);
    item_rows(flags, (cursor, first), lines, rows, last_place, time)
}

/// The list drawer itself (`0x8002C784`), which the menu of a fight calls too: `flags`
/// is the flag word of the shared drawer's record (the record's style), and `kept` the
/// cursor among the four rows and the first line.
pub fn item_rows(
    flags: u32,
    (cursor, first): (i16, i16),
    lines: &[Vec<Piece>],
    rows: [ItemRow; items::ROWS],
    last_place: i16,
    time: u32,
) -> Vec<Drawn> {
    let (cursor, first) = (i32::from(cursor), i32::from(first));
    let mut drawn = Vec::new();
    // The signs: one line for every place up to the last in use, four at least.
    let count = (i32::from(last_place) + 1).max(items::ROWS as i32);
    if first < count - items::ROWS as i32 && count > items::ROWS as i32 {
        drawn.push(Drawn::Sign {
            turns: sign::BELOW,
            at: items::SIGNS[1],
            time,
        });
    }
    if first != 0 {
        drawn.push(Drawn::Sign {
            turns: sign::ABOVE,
            at: items::SIGNS[0],
            time,
        });
    }
    for (n, (pieces, row)) in lines.iter().zip(rows).enumerate() {
        let (style, pulses) = Style::of_line(flags, cursor == n as i32, 0);
        let y = items::FIRST_ROW + items::STEP * n as i16;
        for (k, (piece, x)) in pieces.iter().zip(items::PIECES).enumerate() {
            let clip = Rect { x, y, ..piece.rect };
            if piece.flags & piece::BARE == 0 {
                drawn.push(Drawn::Frame {
                    rect: clip,
                    mode: style.mode,
                    time: if pulses { time } else { 0 },
                });
            }
            if row.count == 0 {
                continue;
            }
            let room = clip.inside(piece.margin);
            let (item, dim) = (row.item, row.refused);
            drawn.push(match k {
                0 => Drawn::ItemPicture {
                    item,
                    at: (room.x, room.y),
                    dim,
                    clip,
                },
                1 => Drawn::ItemName {
                    item,
                    at: (room.x, room.y),
                    dim,
                    clip,
                },
                // A number ends 3 left of its room's right edge, 2 below its upper one.
                _ => Drawn::Count {
                    value: row.count.into(),
                    ends: (room.x + room.w - 3, room.y + 2),
                    dim,
                    clip,
                },
            });
        }
    }
    drawn
}

/// The stretch a gauge shows for a change to come (`0x8002BAB0`): a bar of the
/// experience's picture behind the end of the gauge's own bar, or from its start for a
/// loss, as long as the change scaled as the bar is, in the passes with bit `0x10`.
/// A change that would pass the most is as many pixels long as value lacks of it.
pub fn gain_bar(
    kind: u8,
    (value, most, change): (i32, i32, i32),
    at: (i16, i16),
    passes: u32,
    clip: Rect,
) -> Option<Drawn> {
    if change == 0 || passes & items::BLINK == 0 {
        return None;
    }
    let longest = if kind == 0 { 87 } else { 47 };
    let own = bar_length(value as u32, most as u32, longest);
    let length = match most < value.wrapping_add(change) {
        true => most.wrapping_sub(value) as i16,
        false => bar_length(change.unsigned_abs(), most as u32, longest),
    };
    let right = if change < 0 {
        8
    } else {
        8i16.wrapping_add(own)
    };
    Some(Drawn::Bar {
        picture: bar::EXPERIENCE,
        length,
        at: (at.0.wrapping_add(right), at.1.wrapping_add(2)),
        clip,
    })
}

/// The bits of the last argument of the head of a fighter's box (`0x8002F298`), and
/// the sprite that stands in for a digit of a hidden number, 7 apart (`0x8002EFB8`).
pub mod head {
    /// Every number is hidden: the box of a boss in a fight.
    pub const HIDDEN: u32 = 1;
    /// "Capture" stands in front of "Level": the player.
    pub const CAPTURE: u32 = 2;
    pub const MARK: usize = 217;
    pub const MARK_STEP: i16 = 7;
}

/// The head of a fighter's box behind his name (`0x8002F298`), from the corner of the
/// box: the line of his HP with a change to come blinking on its gauge, the sign of his
/// element, and his level, by the flags with "Capture" in front of it and with marks in
/// place of every number ([`head`]).
pub fn head_of_box(
    record: &Record,
    corner: (i16, i16),
    gain: i32,
    passes: u32,
    flags: u32,
    clip: Rect,
) -> Vec<Drawn> {
    let from = |right: i16, down: i16| (corner.0.wrapping_add(right), corner.1.wrapping_add(down));
    let word = |number: usize, at: (i16, i16)| Drawn::Sprite { number, at, clip };
    let marks = |count: i16, at: (i16, i16)| {
        (0..count).map(move |n| Drawn::Sprite {
            number: head::MARK,
            at: (at.0.wrapping_add(head::MARK_STEP * n), at.1),
            clip,
        })
    };
    let hidden = flags & head::HIDDEN != 0;
    let mut drawn = Vec::new();
    let (hp, most) = (record.hp, record.max_hp);
    match hidden {
        false => drawn.extend(line((hp, most), from(3, 11), clip)),
        // The gauge, and three marks on either side of the stroke (`0x8002EE84`).
        true => {
            drawn.extend(gauge(1, hp.into(), most.into(), from(3, 11), clip));
            drawn.extend(marks(3, from(11, 18)));
            drawn.push(word(sprite::STROKE, from(32, 18)));
            drawn.extend(marks(3, from(38, 18)));
        }
    }
    drawn.extend(gain_bar(
        1,
        (hp.into(), most.into(), gain),
        from(3, 11),
        passes,
        clip,
    ));
    drawn.extend(element_sign(record.element_bit, from(69, 4), clip));
    let (level, number) = match flags & head::CAPTURE != 0 {
        true => {
            drawn.push(word(items::CAPTURE, from(4, 28)));
            (from(27, 27), 53)
        }
        false => (from(16, 27), 42),
    };
    drawn.push(word(creature::LEVEL, level));
    match hidden {
        false => drawn.push(Drawn::Number {
            value: record.level.into(),
            ends: from(number + 13, 29),
            small: true,
            clip,
        }),
        true => drawn.extend(marks(2, from(number, 29))),
    }
    drawn
}

/// The box of who gets the item (`0x800C0620` through `0x8002F224` and `0x8002F298`),
/// from what lies underneath to what lies on top: a plain frame and the head of a
/// fighter's box in it, his name in the small font, the line of his HP with the gain a
/// use would give blinking on its gauge, the sign of his element and his level, for
/// the player with "Capture" in front. `creature` is the carried creature under the
/// cursor of the targets, `None` for the player. Nothing is cut to the box.
pub fn who_gets(creature: Option<usize>, record: &Record, gain: i32, passes: u32) -> Vec<Drawn> {
    let clip = SCREEN;
    let from = |right: i16, down: i16| (items::WHO.x + right, items::WHO.y + down);
    let mut drawn = vec![
        Drawn::Frame {
            rect: items::WHO,
            mode: 0,
            time: 0,
        },
        match creature {
            Some(creature) => Drawn::CreatureName {
                creature,
                at: from(7, 3),
                clip,
            },
            None => Drawn::SmallName {
                at: from(7, 3),
                clip,
            },
        },
    ];
    let flags = match creature {
        Some(_) => 0,
        None => head::CAPTURE,
    };
    drawn.extend(head_of_box(record, from(0, 0), gain, passes, flags, clip));
    drawn
}

/// Whether the description of the item under the list's cursor stands in place of the
/// box of who gets it (`0x800C0620`): on the list's own screen, while the circle is
/// held, for an item that has one.
pub fn describes(menu: &PlayerMenu, item: u8, circle_held: bool) -> bool {
    circle_held && menu.screen == ITEMS && item < items::ITEMS
}

/// The description of an item (`0x8002AF70`): the box of a message with its four
/// lines, from what lies underneath to what lies on top.
pub fn description(item: u8) -> [Drawn; 1 + message::LINES] {
    std::array::from_fn(|n| match n.checked_sub(1) {
        None => Drawn::Frame {
            rect: message::BOX,
            mode: 0,
            time: 0,
        },
        Some(line) => Drawn::Described {
            item,
            line,
            at: (
                message::BOX.x + message::TEXT.0,
                message::BOX.y + message::TEXT.1 + message::LINE * line as i16,
            ),
        },
    })
}

/// The four lines of an item's description from the menu's file, whose first byte is
/// at `base`; a line without an address is empty.
pub fn description_text(file: &[u8], base: u32, item: u8) -> Option<[Vec<u16>; message::LINES]> {
    if item >= items::ITEMS {
        return None;
    }
    let image = Image { data: file, base };
    let mut lines: [Vec<u16>; message::LINES] = Default::default();
    for (n, line) in lines.iter_mut().enumerate() {
        let entry = crate::rooms::step(items::DESCRIPTIONS, usize::from(item) * 4 + n, 4).ok()?;
        let address = image.u32(entry).ok()?;
        if address != 0 {
            let offset = address.checked_sub(base)? as usize;
            *line = crate::text::units(file.get(offset..)?);
        }
    }
    Some(lines)
}

/// The colour the large digits of a dim piece are drawn under, from the loaded
/// executable; 128 in a part leaves a texel as it is.
pub fn dim_digits(executable: &[u8], base: u32) -> Option<[u8; 3]> {
    digit_colour(executable, base, items::DIM_DIGITS)
}

/// An entry of the executable's colours of the large digits.
pub fn digit_colour(executable: &[u8], base: u32, entry: usize) -> Option<[u8; 3]> {
    let image = Image {
        data: executable,
        base,
    };
    let entry = crate::rooms::step(items::DIGIT_COLOURS, entry, 4).ok()?;
    Some([
        image.u8(entry).ok()?,
        image.u8(entry + 1).ok()?,
        image.u8(entry + 2).ok()?,
    ])
}

/// The bits of the list of the things of the story (panel 13) and of the question
/// whether to leave the forest (panel 16).
pub const STORY_PANEL: u32 = 0x2000;
pub const QUESTION_PANEL: u32 = 0x1_0000;

/// The places and tables of the list of the things of the story (`0x800C0BA8`,
/// `0x800C07D8`) and of the question (`0x800BEBF4`).
pub mod story {
    use super::{Rect, rect};

    /// The menu file's list of the four lines the list is drawn with, a row each: a
    /// picture and a name.
    pub const LINES: u32 = 0x800C_2ABC;
    pub const ROWS: usize = 4;
    /// Where the two pieces of a row start, the first row's y and the step of a row,
    /// as the drawer writes them in every pass.
    pub const PIECES: [i16; 2] = [97, 118];
    pub const FIRST_ROW: i16 = 54;
    pub const STEP: i16 = 22;
    /// The scroll signs above and below the list (the places at `0x800C2964`).
    pub const SIGNS: [(i16, i16); 2] = [(160, 47), (160, 148)];
    /// The menu file's table of the things' texts: the addresses of four lines for
    /// each thing.
    pub const TEXTS: u32 = 0x800C_2090;
    /// The question and its two answers, texts of the menu's file, and the lines of
    /// the message's box they stand in.
    pub const QUESTION: [u32; 2] = [0x800B_BC14, 0x800B_BC38];
    pub const QUESTION_LINES: [i16; 2] = [0, 2];
    /// The frame of the cursor under the first answer, and the step to the second.
    pub const ANSWER: Rect = rect(14, 178, 28, 14);
    pub const ANSWER_STEP: i16 = 28;
}

/// A row of the list of the things of the story: the thing, [`STORY_THINGS`] for a
/// row past the last one held, and whether its row is open (`0x800C0E34`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoryRow {
    pub thing: i16,
    pub usable: bool,
}

/// What the list of the things of the story draws in a pass (`0x800C0BA8`), from what
/// lies underneath to what lies on top: the two scroll signs, then four rows through
/// the shared menu drawer, each a picture and a name in a frame of their own. `rows`
/// are the things held from the kept first line on, `held` how many are held. As the
/// items' list it does not open, pulses under the cursor on its own screen, is plain
/// behind the player's entries and has the cursor's row in the marked colour behind
/// the question. A row that is not open is dim, and a row without a thing has its
/// frames alone.
pub fn story_list(
    menu: &PlayerMenu,
    lines: &[Vec<Piece>],
    rows: [StoryRow; story::ROWS],
    held: i16,
    time: u32,
) -> Vec<Drawn> {
    let flags = match menu.screen {
        STORY => 0x02,
        OF_PLAYER => 0x01,
        _ => 0x04,
    };
    let first = i32::from(menu.first_thing());
    let cursor = i32::from(menu.thing_place()) - first;
    let mut drawn = Vec::new();
    let count = i32::from(held).max(story::ROWS as i32);
    if first < count - story::ROWS as i32 && count > story::ROWS as i32 {
        drawn.push(Drawn::Sign {
            turns: sign::BELOW,
            at: story::SIGNS[1],
            time,
        });
    }
    if first != 0 {
        drawn.push(Drawn::Sign {
            turns: sign::ABOVE,
            at: story::SIGNS[0],
            time,
        });
    }
    for (n, (pieces, row)) in lines.iter().zip(rows).enumerate() {
        let (style, pulses) = Style::of_line(flags, cursor == n as i32, 0);
        let y = story::FIRST_ROW + story::STEP * n as i16;
        for (k, (piece, x)) in pieces.iter().zip(story::PIECES).enumerate() {
            let clip = Rect { x, y, ..piece.rect };
            if piece.flags & piece::BARE == 0 {
                drawn.push(Drawn::Frame {
                    rect: clip,
                    mode: style.mode,
                    time: if pulses { time } else { 0 },
                });
            }
            if row.thing >= STORY_THINGS {
                continue;
            }
            let room = clip.inside(piece.margin);
            let (thing, at, dim) = (row.thing, (room.x, room.y), !row.usable);
            drawn.push(match k {
                0 => Drawn::ThingPicture {
                    thing,
                    at,
                    dim,
                    clip,
                },
                _ => Drawn::ThingName {
                    thing,
                    at,
                    dim,
                    clip,
                },
            });
        }
    }
    drawn
}

/// Whether the text of the thing under the list's cursor stands in place of the box
/// with the player's head (`0x800C07D8`): on the list's own screen, while the circle
/// is held, for a thing that has one.
pub fn thing_describes(menu: &PlayerMenu, thing: i16, circle_held: bool) -> bool {
    circle_held && menu.screen == STORY && (0..STORY_THINGS).contains(&thing)
}

/// The text of a thing of the story (`0x8002AF70`): the box of a message with the
/// thing's four lines, from what lies underneath to what lies on top. It is what the
/// held circle shows on the list and what the use of a thing with a text of its own
/// shows as its message.
pub fn thing_description(thing: i16) -> [Drawn; 1 + message::LINES] {
    std::array::from_fn(|n| match n.checked_sub(1) {
        None => Drawn::Frame {
            rect: message::BOX,
            mode: 0,
            time: 0,
        },
        Some(line) => Drawn::ThingDescribed {
            thing,
            line,
            at: (
                message::BOX.x + message::TEXT.0,
                message::BOX.y + message::TEXT.1 + message::LINE * line as i16,
            ),
        },
    })
}

/// The four lines of the text of a thing of the story from the menu's file, whose
/// first byte is at `base`; a line without an address is empty.
pub fn thing_text(file: &[u8], base: u32, thing: i16) -> Option<[Vec<u16>; message::LINES]> {
    let thing = usize::try_from(thing).ok()?;
    if thing >= STORY_THINGS as usize {
        return None;
    }
    let image = Image { data: file, base };
    let mut lines: [Vec<u16>; message::LINES] = Default::default();
    for (n, line) in lines.iter_mut().enumerate() {
        let entry = crate::rooms::step(story::TEXTS, thing * 4 + n, 4).ok()?;
        let address = image.u32(entry).ok()?;
        if address != 0 {
            let offset = address.checked_sub(base)? as usize;
            *line = crate::text::units(file.get(offset..)?);
        }
    }
    Some(lines)
}

/// What the question whether to leave the forest draws (`0x800BEBF4`), from what lies
/// underneath to what lies on top: the frame of the cursor under the answer it is on,
/// pulsing, which is added last and so lies under the rest; the box of a message; the
/// question in the box's first line and the two answers in its third.
pub fn question(menu: &PlayerMenu, time: u32) -> [Drawn; 4] {
    let answer = Rect {
        x: story::ANSWER.x + story::ANSWER_STEP * menu.cursor.at,
        ..story::ANSWER
    };
    let text = |text: usize| Drawn::Asked {
        text,
        at: (
            message::BOX.x + message::TEXT.0,
            message::BOX.y + message::TEXT.1 + message::LINE * story::QUESTION_LINES[text],
        ),
    };
    [
        Drawn::Frame {
            rect: answer,
            mode: 1,
            time,
        },
        Drawn::Frame {
            rect: message::BOX,
            mode: 0,
            time: 0,
        },
        text(0),
        text(1),
    ]
}

/// The question and its answers from the menu's file, whose first byte is at `base`.
pub fn question_texts(file: &[u8], base: u32) -> Option<[Vec<u16>; 2]> {
    let text = |address: u32| {
        let offset = address.checked_sub(base)? as usize;
        file.get(offset..).map(crate::text::units)
    };
    Some([text(story::QUESTION[0])?, text(story::QUESTION[1])?])
}

/// The bit of what the choice of who gets an item adds to the list (panel 15).
pub const TARGET_PANEL: u32 = 0x8000;

/// The places of the picker of a target (`0x8002D2AC`) and of the line of mana.
pub mod picker {
    use super::{Rect, rect};

    /// The executable's line of the player, a name between two small frames without
    /// content, and the lines of the three carried creatures, a name each.
    pub const PLAYER_LINE: u32 = 0x8007_4114;
    pub const CREATURE_LINES: [u32; 3] = [0x8007_4184, 0x8007_41BC, 0x8007_41F4];
    /// The frame of the player's name, the margin of a name in it, and where the two
    /// small frames beside it start, 6 wide each.
    pub const NAME: Rect = rect(46, 152, 60, 12);
    pub const MARGIN: (i16, i16) = (2, 2);
    pub const SIDES: [i16; 2] = [38, 108];
    pub const SIDE: i16 = 6;
    /// The y of the first creature's row and the step of the rows that are shown.
    pub const FIRST: i16 = 166;
    pub const STEP: i16 = 13;
    /// The frame of the line of mana of a carried creature under the cursor.
    pub const MANA: Rect = rect(210, 187, 100, 21);
}

/// Whether the picker shows the row of a target closed, by the answer of the test of
/// the item's target (`0x80033E50`): the player's row is open for an answer below 2,
/// a creature's for 0 and 2.
pub fn picker_closes(target: usize, answer: i16) -> bool {
    match target {
        0 => !(0..2).contains(&answer),
        _ => answer != 0 && answer != 2,
    }
}

/// What panel 15 draws in a pass without a message (`0x800BEB18`), from what lies
/// underneath to what lies on top: for a carried creature under the cursor the line of
/// its mana in a plain frame, with the mana a use would give blinking on the gauge;
/// then the picker, a framed name for the player and for each carried creature, in
/// the middle of its frame, the row under the cursor pulsing and a closed row dim.
/// `closed` goes by target, 0 the player; `creature` is the record under the cursor.
pub fn target_panel(
    menu: &PlayerMenu,
    closed: [bool; 4],
    creature: Option<&Record>,
    gain: i32,
    time: u32,
    passes: u32,
) -> Vec<Drawn> {
    let mut drawn = Vec::new();
    if let Some(record) = creature {
        let clip = SCREEN;
        let from = |right: i16, down: i16| (picker::MANA.x + right, picker::MANA.y + down);
        drawn.extend([
            Drawn::Frame {
                rect: picker::MANA,
                mode: 0,
                time: 0,
            },
            Drawn::Sprite {
                number: creature::MANA,
                at: from(7, 4),
                clip,
            },
        ]);
        let (mp, most) = (record.mp, record.max_mp);
        drawn.extend(line((mp, most), from(32, 3), clip));
        drawn.extend(gain_bar(
            1,
            (mp.into(), most.into(), gain),
            from(32, 3),
            passes,
            clip,
        ));
    }
    let cursor = usize::try_from(menu.target()).ok();
    let carried = usize::try_from(menu.carried).unwrap_or(0).min(3);
    // Mode 0 of the picker: the player and every carried creature.
    let rows: Vec<(usize, bool)> = (closed.into_iter().enumerate()).take(carried + 1).collect();
    drawn.extend(picker_rows(&rows, cursor, time));
    drawn
}

/// What the picker of a target draws (`0x8002D2AC`), row by row: a framed name, the
/// player's between two small frames at the top and those of the carried creatures
/// below it, 13 apart from the first that is shown, each name in the middle of its
/// frame; the row under the cursor pulses and a closed row is dim. A row is its target,
/// 0 the player and n a carried creature, and whether it is closed; `cursor` is a row,
/// and none when the picker was handed -1.
pub fn picker_rows(rows: &[(usize, bool)], cursor: Option<usize>, time: u32) -> Vec<Drawn> {
    let mut drawn = Vec::new();
    let mut creatures = 0;
    for (row, &(target, dim)) in rows.iter().enumerate() {
        let (style, pulses) = Style::of_line(0x02, cursor == Some(row), 0);
        let frame = |rect: Rect| Drawn::Frame {
            rect,
            mode: style.mode,
            time: if pulses { time } else { 0 },
        };
        let y = match target {
            0 => picker::NAME.y,
            _ => {
                creatures += 1;
                picker::FIRST + picker::STEP * (creatures - 1)
            }
        };
        let clip = Rect { y, ..picker::NAME };
        let room = clip.inside(picker::MARGIN);
        drawn.extend([
            frame(clip),
            Drawn::PickedName {
                target,
                // Half the room and half the name, both halved towards zero.
                middle: (room.x + room.w / 2, room.y),
                dim,
                clip,
            },
        ]);
        if target == 0 {
            drawn.extend(picker::SIDES.map(|x| frame(rect(x, y, picker::SIDE, clip.h))));
        }
    }
    drawn
}

/// The bits of the list of a creature's spells (panel 4) and of its body attacks
/// (panel 8).
pub const SPELLS_PANEL: u32 = 0x10;
pub const ATTACKS_PANEL: u32 = 0x100;

/// The places and tables of the list of a creature's spells (`0x8002CB24`,
/// `0x800C08E0`).
pub mod spells {
    /// The executable's list of the four lines the list is drawn with, a row each: a
    /// picture and a name.
    pub const LINES: u32 = 0x8008_2A78;
    pub const ROWS: usize = 4;
    /// Where the two pieces of a row start, the first row's y and the step of a row,
    /// as the drawer writes them in every pass.
    pub const PIECES: [i16; 2] = [96, 117];
    pub const FIRST_ROW: i16 = 54;
    pub const STEP: i16 = 22;
    /// The scroll signs above and below the list (the places at `0x80082920`).
    pub const SIGNS: [(i16, i16); 2] = [(160, 47), (160, 148)];
    /// The spells of the game; the answer of "the n-th it knows" when there is none.
    pub const SPELLS: usize = 30;
    /// The executable's names, eight bytes a spell with the address first, and the
    /// sprite of each spell, a byte.
    pub const NAMES: u32 = 0x8007_C26C;
    pub const PICTURES: u32 = 0x8007_C62C;
    /// The menu file's table of the spells' texts: the addresses of four lines each.
    pub const TEXTS: u32 = 0x800C_22F0;
}

/// The places and tables of the list of a creature's body attacks (`0x8002CE84`,
/// panel 8 `0x800BE8C8`).
pub mod attacks {
    /// The executable's list of the five lines, a row each: a picture, the attack's
    /// name and the name of its grade, all three in the middle of their rooms.
    pub const LINES: u32 = 0x8008_2CBC;
    pub const ROWS: usize = 5;
    pub const PIECES: [i16; 3] = [56, 94, 187];
    pub const FIRST_ROW: i16 = 44;
    pub const STEP: i16 = 22;
    /// The body attacks of the game and the grades of a move entry.
    pub const ATTACKS: usize = 39;
    pub const GRADES: usize = 5;
    /// The executable's names: an address for each attack (none from attack 16 on)
    /// and for each grade.
    pub const NAMES: u32 = 0x8008_3408;
    pub const GRADE_NAMES: u32 = 0x8008_34A4;
    /// The sprites of a row (`0x8002D200`): for each row the address of four, by the
    /// place of the attack's element, and a fifth for an attack without one.
    pub const PICTURES: u32 = 0x8008_2730;
    pub const PLAIN: u32 = 0x8008_2714;
    /// The place of an element's sprite by the class of the attack (fire, wind,
    /// earth, water).
    pub const ELEMENT_PLACES: [usize; 4] = [0, 2, 3, 1];
    pub const NO_ELEMENT: usize = 4;
    /// The menu file's table of the attacks' texts: the addresses of four lines each.
    pub const TEXTS: u32 = 0x800C_24D0;
}

/// How many spells the word of a record names (`0x8001AE54`): its bits 0 to 29.
pub fn spells_known(bits: u32) -> i16 {
    (bits & ((1 << spells::SPELLS) - 1)).count_ones() as i16
}

/// The n-th spell the word of a record names, counted from 0 (`0x8001AEC0`);
/// [`spells::SPELLS`] when it names fewer.
pub fn spell_at(bits: u32, n: i16) -> usize {
    let known = (0..spells::SPELLS).filter(|spell| bits >> spell & 1 != 0);
    let n = usize::try_from(n).ok();
    (n.and_then(|n| known.clone().nth(n))).unwrap_or(spells::SPELLS)
}

/// The two scroll signs of a list of four rows over `count` lines.
fn signs(first: i32, count: i32, places: [(i16, i16); 2], time: u32) -> Vec<Drawn> {
    let mut drawn = Vec::new();
    if first < count - 4 && count > 4 {
        drawn.push(Drawn::Sign {
            turns: sign::BELOW,
            at: places[1],
            time,
        });
    }
    if first != 0 {
        drawn.push(Drawn::Sign {
            turns: sign::ABOVE,
            at: places[0],
            time,
        });
    }
    drawn
}

/// What the list of a creature's spells draws in a pass (`0x8002CB24`), from what lies
/// underneath to what lies on top: the two scroll signs, then four rows through the
/// shared menu drawer, each the spell's sprite and its name in a frame of their own.
/// `rows` are the spells the creature knows from the kept first line on
/// ([`spell_at`]), `known` how many it knows and `pictures` the sprite of each spell.
/// As the items' list it does not open, pulses under the cursor on its own screen, is
/// plain behind the creature's entries and has the cursor's row in the marked colour
/// elsewhere. A row past the last spell has its frames alone. Outside a fight no row
/// is dim for the mana its spell costs (`0x80033C64` with the list's word at +10).
pub fn spell_list(
    menu: &PlayerMenu,
    lines: &[Vec<Piece>],
    rows: [usize; spells::ROWS],
    known: i16,
    pictures: &[u8],
    time: u32,
) -> Vec<Drawn> {
    let flags = match menu.screen {
        MAGIC => 0x02,
        OF_CREATURE => 0x01,
        _ => 0x04,
    };
    let first = menu.first_spell();
    let kept = (menu.spell_place().wrapping_sub(first), first);
    let shown = SpellRows {
        rows,
        short: [false; spells::ROWS],
        known,
    };
    let place = (spells::PIECES[0], spells::FIRST_ROW);
    spell_rows(flags, kept, place, lines, shown, pictures, time)
}

/// What a list of spells shows in a pass: the spells of its four rows ([`spell_at`]
/// from the first line on), the rows whose spell costs more mana than the creature
/// has, which only a fight shows dim, and how many spells the creature knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpellRows {
    pub rows: [usize; spells::ROWS],
    pub short: [bool; spells::ROWS],
    pub known: i16,
}

/// The list of the spells for any caller (`0x8002CB24`): the flag word of the shared
/// drawer, the cursor's row with the first line, and the place of the first row's first
/// piece, which the player's menu leaves at (96, 54) and the menu of a fight puts at
/// (96, 69); the scroll signs move with it.
pub fn spell_rows(
    flags: u32,
    (cursor, first): (i16, i16),
    place: (i16, i16),
    lines: &[Vec<Piece>],
    shown: SpellRows,
    pictures: &[u8],
    time: u32,
) -> Vec<Drawn> {
    let moved = (place.0 - spells::PIECES[0], place.1 - spells::FIRST_ROW);
    let count = i32::from(shown.known).max(spells::ROWS as i32);
    let places = spells::SIGNS.map(|(x, y)| (x + moved.0, y + moved.1));
    let mut drawn = signs(first.into(), count, places, time);
    for (n, (pieces, spell)) in lines.iter().zip(shown.rows).enumerate() {
        let (style, pulses) = Style::of_line(flags, usize::try_from(cursor) == Ok(n), 0);
        let y = place.1 + spells::STEP * n as i16;
        for (k, (piece, x)) in pieces.iter().zip(spells::PIECES).enumerate() {
            let x = x + moved.0;
            let clip = Rect { x, y, ..piece.rect };
            if piece.flags & piece::BARE == 0 {
                drawn.push(Drawn::Frame {
                    rect: clip,
                    mode: style.mode,
                    time: if pulses { time } else { 0 },
                });
            }
            let what = match (k, pictures.get(spell)) {
                (0, Some(&picture)) => Listed::Sprite(picture.into()),
                (_, Some(_)) => Listed::Spell(spell),
                (_, None) => continue,
            };
            drawn.push(Drawn::Listed {
                what,
                room: clip.inside(piece.margin),
                flags: piece.flags,
                dim: shown.short[n],
                clip,
            });
        }
    }
    drawn
}

/// A move entry of a creature as a row of the list of its body attacks shows it,
/// with the class of the attack's effect, and whether the row's test refuses a known
/// entry for the mana it costs, which it does in a fight alone (`0x80033CF4`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AttackRow {
    pub known: bool,
    pub attack: u8,
    pub grade: u8,
    pub class: u8,
    pub refused: bool,
}

/// The sprite of a row of the body attacks (`0x8002D200`): by the row and the element
/// of the attack's class, the row's fifth for a class that is no element. `pictures`
/// are the five sprites of each row ([`attack_pictures`]).
pub fn attack_picture(pictures: &[[u8; 5]; attacks::ROWS], row: usize, class: u8) -> usize {
    let place =
        (attacks::ELEMENT_PLACES.get(usize::from(class)).copied()).unwrap_or(attacks::NO_ELEMENT);
    pictures.get(row).map_or(0, |row| row[place].into())
}

/// What the list of a creature's body attacks draws in a pass (`0x8002CE84`): five
/// rows through the shared menu drawer, each a sprite, the attack's name and the name
/// of its grade, every one in the middle of its room in a frame of its own. A row
/// whose entry is not known is dim and shows the names of attack 0 and grade 0, two
/// strokes; so does the name of a known entry whose attack or grade is 0. The sprite
/// goes by the entry's attack whether it is known or not. The frames pulse under the
/// cursor on the list's own screen and are plain behind the creature's entries.
pub fn attack_list(
    menu: &PlayerMenu,
    lines: &[Vec<Piece>],
    rows: [AttackRow; attacks::ROWS],
    pictures: &[[u8; 5]; attacks::ROWS],
    time: u32,
) -> Vec<Drawn> {
    let flags = if menu.screen == SPECIAL { 0x02 } else { 0x01 };
    attack_rows(flags, menu.attack_row(), lines, rows, pictures, time)
}

/// The list of the body attacks for any caller (`0x8002CE84`): the flag word of the
/// shared drawer and the cursor's row. The menu of a fight leaves the list where the
/// player's menu has it, and there a known entry that costs more mana than the
/// creature has is dim too, with its own names.
pub fn attack_rows(
    flags: u32,
    cursor: i16,
    lines: &[Vec<Piece>],
    rows: [AttackRow; attacks::ROWS],
    pictures: &[[u8; 5]; attacks::ROWS],
    time: u32,
) -> Vec<Drawn> {
    let place = (attacks::PIECES[0], attacks::FIRST_ROW);
    attack_rows_at(flags, cursor, place, lines, rows, pictures, time)
}

/// The same list with the first row's first piece at `place`, which the routine is
/// handed by the screen of file 44 (`crate::merge_panel`): its three pieces keep their
/// distances.
pub fn attack_rows_at(
    flags: u32,
    cursor: i16,
    place: (i16, i16),
    lines: &[Vec<Piece>],
    rows: [AttackRow; attacks::ROWS],
    pictures: &[[u8; 5]; attacks::ROWS],
    time: u32,
) -> Vec<Drawn> {
    let mut drawn = Vec::new();
    for (n, (pieces, row)) in lines.iter().zip(rows).enumerate() {
        let (style, pulses) = Style::of_line(flags, usize::try_from(cursor) == Ok(n), 0);
        let y = place.1 + attacks::STEP * n as i16;
        let shown = |number: u8| if row.known { number.into() } else { 0 };
        for (k, (piece, x)) in pieces.iter().zip(attacks::PIECES).enumerate() {
            let x = x - attacks::PIECES[0] + place.0;
            let clip = Rect { x, y, ..piece.rect };
            if piece.flags & piece::BARE == 0 {
                drawn.push(Drawn::Frame {
                    rect: clip,
                    mode: style.mode,
                    time: if pulses { time } else { 0 },
                });
            }
            drawn.push(Drawn::Listed {
                what: match k {
                    0 => Listed::Sprite(attack_picture(pictures, n, row.class)),
                    1 => Listed::Attack(shown(row.attack)),
                    _ => Listed::Grade(shown(row.grade)),
                },
                room: clip.inside(piece.margin),
                flags: piece.flags,
                dim: !row.known || row.refused,
                clip,
            });
        }
    }
    drawn
}

/// Whether the text of the spell under the list's cursor stands in place of the head
/// of the creature's box (`0x800C08E0`): on the list's own screen, while the circle is
/// held, for a row that has a spell.
pub fn spell_describes(menu: &PlayerMenu, spell: usize, circle_held: bool) -> bool {
    circle_held && menu.screen == MAGIC && spell < spells::SPELLS
}

/// The same for the body attack under the cursor (`0x8002C6CC`): an entry that is
/// known and whose attack is not 0.
pub fn attack_describes(menu: &PlayerMenu, row: AttackRow, circle_held: bool) -> bool {
    circle_held && menu.screen == SPECIAL && row.known && row.attack != 0
}

/// The text of a spell or of a body attack (`0x8002AF70`): the box of a message with
/// its four lines, from what lies underneath to what lies on top.
pub fn list_description(what: Listed) -> [Drawn; 1 + message::LINES] {
    list_description_at(what, (message::BOX.x, message::BOX.y))
}

/// The same box with its corner at a place the routine is handed.
pub fn list_description_at(what: Listed, corner: (i16, i16)) -> [Drawn; 1 + message::LINES] {
    let (x, y) = corner;
    std::array::from_fn(|n| match n.checked_sub(1) {
        None => Drawn::Frame {
            rect: Rect {
                x,
                y,
                ..message::BOX
            },
            mode: 0,
            time: 0,
        },
        Some(line) => Drawn::ListDescribed {
            what,
            line,
            at: (
                x + message::TEXT.0,
                y + message::TEXT.1 + message::LINE * line as i16,
            ),
        },
    })
}

/// A text at an address of a memory image whose first byte is at `base`; an address
/// of 0 is the empty text.
pub(crate) fn text_at(image: &[u8], base: u32, address: u32) -> Option<Vec<u16>> {
    if address == 0 {
        return Some(Vec::new());
    }
    let offset = address.checked_sub(base)? as usize;
    image.get(offset..).map(crate::text::units)
}

/// Entry `n` of a table of four addresses each, as four lines.
pub(crate) fn four_lines(
    file: &[u8],
    base: u32,
    table: u32,
    n: usize,
) -> Option<[Vec<u16>; message::LINES]> {
    let image = Image { data: file, base };
    let mut lines: [Vec<u16>; message::LINES] = Default::default();
    for (k, line) in lines.iter_mut().enumerate() {
        let entry = crate::rooms::step(table, n * 4 + k, 4).ok()?;
        *line = text_at(file, base, image.u32(entry).ok()?)?;
    }
    Some(lines)
}

/// The four lines of a spell's text and of a body attack's from the menu's file,
/// whose first byte is at `base`. The attacks from 16 on have four empty lines.
pub fn spell_text(file: &[u8], base: u32, spell: usize) -> Option<[Vec<u16>; message::LINES]> {
    (spell < spells::SPELLS).then(|| four_lines(file, base, spells::TEXTS, spell))?
}

pub fn attack_text(file: &[u8], base: u32, attack: usize) -> Option<[Vec<u16>; message::LINES]> {
    (attack < attacks::ATTACKS).then(|| four_lines(file, base, attacks::TEXTS, attack))?
}

/// The name of a spell, of a body attack and of a grade from the loaded executable;
/// an attack without a name has the empty text.
pub fn spell_name(executable: &[u8], base: u32, spell: usize) -> Option<Vec<u16>> {
    let image = Image {
        data: executable,
        base,
    };
    let entry = crate::rooms::step(spells::NAMES, spell, 8).ok()?;
    (spell < spells::SPELLS).then(|| text_at(executable, base, image.u32(entry).ok()?))?
}

pub fn attack_name(executable: &[u8], base: u32, attack: usize) -> Option<Vec<u16>> {
    let image = Image {
        data: executable,
        base,
    };
    let entry = crate::rooms::step(attacks::NAMES, attack, 4).ok()?;
    (attack < attacks::ATTACKS).then(|| text_at(executable, base, image.u32(entry).ok()?))?
}

pub fn grade_name(executable: &[u8], base: u32, grade: usize) -> Option<Vec<u16>> {
    let image = Image {
        data: executable,
        base,
    };
    let entry = crate::rooms::step(attacks::GRADE_NAMES, grade, 4).ok()?;
    (grade < attacks::GRADES).then(|| text_at(executable, base, image.u32(entry).ok()?))?
}

/// The sprite of every spell, and the five sprites of every row of the body attacks,
/// from the loaded executable.
pub fn spell_pictures(executable: &[u8], base: u32) -> Option<[u8; spells::SPELLS]> {
    let start = spells::PICTURES.checked_sub(base)? as usize;
    executable
        .get(start..start + spells::SPELLS)?
        .try_into()
        .ok()
}

pub fn attack_pictures(executable: &[u8], base: u32) -> Option<[[u8; 5]; attacks::ROWS]> {
    let image = Image {
        data: executable,
        base,
    };
    let mut pictures = [[0; 5]; attacks::ROWS];
    for (row, five) in pictures.iter_mut().enumerate() {
        let of_row = image
            .u32(crate::rooms::step(attacks::PICTURES, row, 4).ok()?)
            .ok()?;
        for (place, picture) in five.iter_mut().enumerate() {
            *picture = match place {
                attacks::NO_ELEMENT => image.u8(attacks::PLAIN + row as u32).ok()?,
                place => image.u8(of_row.checked_add(place as u32)?).ok()?,
            };
        }
    }
    Some(pictures)
}

/// The lists of the line of help (`0x800BECD8`): the one of the player's and the
/// creatures' row in file 42, the plain pair of the executable (a button to pick, a
/// button to leave), the lone button to leave and the list of the items' screen in
/// file 42.
pub const HELP_OF_A_ROW: u32 = 0x800C_2AD0;
pub const HELP_PLAIN: u32 = 0x8008_25A4;
pub const HELP_LEAVE: u32 = 0x800C_2B50;
pub const HELP_OF_ITEMS: u32 = 0x800C_2B28;
/// The list of the screen of the things of the story, in the executable: the plain
/// pair and a third button for the thing's text.
pub const HELP_OF_STORY: u32 = 0x8008_25D4;
/// The list of a creature's two lists, in file 42: a button to leave and one for the
/// text of the row.
pub const HELP_OF_LISTS: u32 = 0x800C_2B60;

/// The lists of screen 0 where the room is in a forest, in file 42: the one of the
/// player's and the creatures' row and the one of the third row. Both name the square
/// for the list of the caught kinds.
pub const HELP_OF_A_ROW_IN_A_FOREST: u32 = 0x800C_2AE8;
pub const HELP_OF_THIRD_ROW_IN_A_FOREST: u32 = 0x800C_2B08;

/// The list of the line of help by the screen and the cursor (`0x800BECD8`), where
/// `forest` says whether the room is in a forest (`0x8003419C`): screen 0 has lists
/// of its own there.
pub fn help_where(menu: &PlayerMenu, forest: bool) -> u32 {
    let rows = flag::PLAYER_ROW | flag::CREATURES_ROW;
    match (forest, menu.screen) {
        (true, FIRST_LEVEL) if menu.flags & rows != 0 => HELP_OF_A_ROW_IN_A_FOREST,
        (true, FIRST_LEVEL) => HELP_OF_THIRD_ROW_IN_A_FOREST,
        _ => help(menu),
    }
}

/// The list of the line of help by the screen and the cursor, outside a forest. The
/// screens behind the entries have lists of their own, which come with those screens.
pub fn help(menu: &PlayerMenu) -> u32 {
    let rows = flag::PLAYER_ROW | flag::CREATURES_ROW;
    match (menu.screen, menu.cursor.at) {
        (FIRST_LEVEL, _) if menu.flags & rows != 0 => HELP_OF_A_ROW,
        // The entries that are nothing but a panel.
        (OF_PLAYER, 2) | (OF_CREATURE, 2 | 3) => HELP_LEAVE,
        (ITEMS, _) => HELP_OF_ITEMS,
        (STORY, _) => HELP_OF_STORY,
        (MAGIC | SPECIAL, _) => HELP_OF_LISTS,
        (CAUGHT, _) => HELP_LEAVE,
        _ => HELP_PLAIN,
    }
}

/// The pairs of a list of help at an address of a memory image whose first byte is at
/// `base`: a sprite of the pool and the address of its word, up to a sprite below zero.
pub fn help_list(image: &[u8], base: u32, address: u32) -> Option<Vec<(usize, u32)>> {
    /// No list of the disc comes near this many pairs.
    const MOST: usize = 16;
    let image = Image { data: image, base };
    let mut pairs = Vec::new();
    loop {
        let pair = crate::rooms::step(address, pairs.len(), 8).ok()?;
        let Ok(sprite) = usize::try_from(image.i32(pair).ok()?) else {
            break Some(pairs);
        };
        pairs.push((sprite, image.u32(pair + 4).ok()?));
        if pairs.len() > MOST {
            return None;
        }
    }
}

/// The bit of the caught creatures that wait (panel 10).
pub const CAUGHT_PANEL: u32 = 0x400;

/// The places of the caught creatures' screen (`0x800C03EC` of the menu's file).
pub mod caught {
    use super::{Rect, rect};

    /// The first box (the rectangle at `0x800C1AE4`); the others stand its width and
    /// its height further, three across and three down.
    pub const FIRST_AT: u32 = 0x800C_1AE4;
    pub const FIRST: Rect = rect(10, 59, 100, 41);
    pub const DOWN: i32 = 3;
    /// The scroll signs above and below the boxes.
    pub const SIGNS: [(i16, i16); 2] = [(160, 45), (160, 196)];
}

/// What panel 10 draws in a pass (`0x800C03EC`): the caught creatures that wait, three
/// rows of three boxes from the kept first row on, each the head of a creature's box
/// (the name, the HP, the element's sign and the level) in a plain frame, and a
/// scroll sign where rows lie above or below. `waiting` are the twelve places of the
/// caught, an empty one `None`. Nothing is drawn while none waits, and nothing of it
/// opens.
pub fn caught_grid(menu: &PlayerMenu, waiting: &[Option<Record>], time: u32) -> Vec<Drawn> {
    let mut drawn = Vec::new();
    if menu.caught == 0 {
        return drawn;
    }
    let first = i32::from(menu.caught_row());
    // More than nine scroll: the first row goes up to the third from the last.
    let last_first = match menu.caught > 9 {
        true => caught_rows(menu.caught) - caught::DOWN,
        false => 0,
    };
    if first < last_first {
        drawn.push(Drawn::Sign {
            turns: sign::BELOW,
            at: caught::SIGNS[1],
            time,
        });
    }
    if first != 0 {
        drawn.push(Drawn::Sign {
            turns: sign::ABOVE,
            at: caught::SIGNS[0],
            time,
        });
    }
    let clip = SCREEN;
    for cell in (0..CAUGHT_ACROSS * caught::DOWN).rev() {
        let place = first * CAUGHT_ACROSS + cell;
        if !(0..CAUGHT_RECORDS).contains(&place) {
            continue;
        }
        let Some(Some(record)) = waiting.get(place as usize) else {
            continue;
        };
        let (x, y) = (
            caught::FIRST.x + caught::FIRST.w * (cell % CAUGHT_ACROSS) as i16,
            caught::FIRST.y + caught::FIRST.h * (cell / CAUGHT_ACROSS) as i16,
        );
        let from = |right: i16, down: i16| (x + right, y + down);
        drawn.extend([
            Drawn::Frame {
                rect: rect(x, y, caught::FIRST.w, caught::FIRST.h),
                mode: 0,
                time: 0,
            },
            Drawn::CaughtName {
                caught: place as usize,
                at: from(7, 3),
                clip,
            },
        ]);
        drawn.extend(line((record.hp, record.max_hp), from(3, 11), clip));
        drawn.extend(element_sign(record.element_bit, from(69, 4), clip));
        drawn.extend([
            Drawn::Sprite {
                number: creature::LEVEL,
                at: from(16, 27),
                clip,
            },
            Drawn::Number {
                value: record.level.into(),
                ends: from(55, 29),
                small: true,
                clip,
            },
        ]);
    }
    drawn
}

/// The bit of a creature's "Family" (panel 9).
pub const FAMILY_PANEL: u32 = 0x200;

/// The places and tables of a creature's "Family" (`0x8002D82C`), on the screen as
/// the menu calls it, with a place of nothing.
pub mod family {
    /// The ancestors a record keeps: its two parents, then the two parents of each.
    pub const ANCESTORS: usize = 6;
    /// The seven boxes, the creature's own first (the executable's seven addresses at
    /// `0x80082CF0`, each of a pair of half words), and the size of each.
    pub const PLACES_AT: u32 = 0x8008_2CF0;
    pub const PLACES: [(i16, i16); ANCESTORS + 1] = [
        (40, 107),
        (117, 77),
        (117, 138),
        (194, 62),
        (194, 92),
        (194, 122),
        (194, 152),
    ];
    pub const BOX: (i16, i16) = (64, 15);
    /// Where the name starts in a box, and the row of signs under it.
    pub const NAME: (i16, i16) = (3, 4);
    pub const SIGN: (i16, i16) = (4, 16);
    /// Where the four signs of a row stand (`0x8002B20C`, the pairs at `0x800740E8`).
    pub const SIGNS_AT: u32 = 0x8007_40E8;
    pub const SIGNS: [(i16, i16); 4] = [(0, 0), (14, 0), (28, 0), (42, 0)];
    /// The twelve lines between the boxes: the two ends of each (the pairs at
    /// `0x80082D0C` and `0x80082D3C`) and their grey (the bytes at `0x800742F0`).
    pub const FROM_AT: u32 = 0x8008_2D0C;
    pub const TO_AT: u32 = 0x8008_2D3C;
    pub const LINES: [((i16, i16), (i16, i16)); 12] = [
        ((104, 114), (110, 114)),
        ((111, 84), (117, 84)),
        ((111, 145), (117, 145)),
        ((181, 84), (187, 84)),
        ((188, 69), (194, 69)),
        ((188, 99), (194, 99)),
        ((181, 145), (187, 145)),
        ((188, 129), (194, 129)),
        ((188, 159), (194, 159)),
        ((111, 84), (111, 145)),
        ((188, 69), (188, 99)),
        ((188, 129), (188, 159)),
    ];
    pub const GREY_AT: u32 = 0x8007_42F0;
    pub const GREY: u8 = 128;
}

/// What a carried creature's record keeps of where it comes from (+4 and +124 of its
/// 248 bytes): the names of six ancestors and the element bits of each, the low byte
/// of the word a head has at +66. The first two are its parents, the next two the
/// parents of the first and the last two the parents of the second.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Family {
    pub names: [Vec<u16>; family::ANCESTORS],
    pub signs: [u8; family::ANCESTORS],
}

impl Family {
    /// The family of a creature that was caught and never merged (`0x800BA064` of
    /// file 44, which makes a carried creature's record of a caught one's): all six
    /// have its own name and its own element.
    pub fn of_a_caught(name: &[u16], element_bit: u16) -> Self {
        Self {
            names: std::array::from_fn(|_| name.to_vec()),
            signs: [element_bit as u8; family::ANCESTORS],
        }
    }

    /// The family as a creature's record keeps it ([`crate::creatures::Line`], which a
    /// merge writes): the six names, each made letters of by `units`, and the six
    /// bytes of bits.
    pub fn of_a_line(
        line: &crate::creatures::Line,
        units: impl Fn(&[u8; crate::name::LETTERS]) -> Vec<u16>,
    ) -> Self {
        Self {
            names: std::array::from_fn(|ancestor| units(&line.names[ancestor])),
            signs: line.signs,
        }
    }

    /// The family of the creature a merge leaves (`0x800BAD08` of file 44): the two
    /// that were merged, each by its name and element bits as they were, become the
    /// parents, and the parents of each the four behind them. `self` is the family of
    /// the first, whose record the merge goes on in.
    pub fn merged(&self, own: (&[u16], u16), other: &Self, others: (&[u16], u16)) -> Self {
        Self {
            names: [
                own.0.to_vec(),
                others.0.to_vec(),
                self.names[0].clone(),
                self.names[1].clone(),
                other.names[0].clone(),
                other.names[1].clone(),
            ],
            signs: [
                own.1 as u8,
                others.1 as u8,
                self.signs[0],
                self.signs[1],
                other.signs[0],
                other.signs[1],
            ],
        }
    }
}

/// The element bits the creature's own box of the "Family" shows (`0x800338C0`, then
/// `0x8002F480`): a bit for every element its strength is 8 or more in.
pub fn strength_signs(strengths: [u8; 4]) -> u16 {
    (strengths.iter().enumerate())
        .filter(|&(_, &strength)| strength >= creature_detail::LEAST)
        .fold(0, |bits, (element, _)| bits | 1 << element)
}

/// A row of four signs (`0x8002B20C`): the letter of each element among the bits and
/// a dash for each that is not.
pub fn sign_row(bits: u16, at: (i16, i16)) -> [Drawn; 4] {
    std::array::from_fn(|element| {
        let (right, down) = family::SIGNS[element];
        Drawn::Sprite {
            number: match bits >> element & 1 {
                1 => creature::HAS[element],
                _ => creature::NONE,
            },
            at: (at.0.wrapping_add(right), at.1.wrapping_add(down)),
            clip: SCREEN,
        }
    })
}

/// What panel 9 draws in a pass (`0x800BE9B8`, `0x8002D82C`) for the carried creature
/// `creature`, from what lies underneath to what lies on top: the twelve lines, then
/// a box for each of the six ancestors and for the creature itself, with a name and a
/// row of signs under the box. Nothing of it opens: the boxes are whole from the
/// first pass on.
pub fn family_panel(creature: usize, record: &Record, family: &Family) -> Vec<Drawn> {
    family_panel_at((0, 0), creature, record, family)
}

/// The same tree with every place of it moved by the place the routine is handed
/// (`0x8002D82C` adds its third argument to each box and to both ends of each line):
/// (0, 0) in the player's menu, (40, 30) on the screen of file 44.
pub fn family_panel_at(
    moved: (i16, i16),
    creature: usize,
    record: &Record,
    family: &Family,
) -> Vec<Drawn> {
    let at = |(x, y): (i16, i16)| (x + moved.0, y + moved.1);
    let mut drawn: Vec<Drawn> = (family::LINES.into_iter().rev())
        .map(|(from, to)| Drawn::Line {
            from: at(from),
            to: at(to),
            grey: family::GREY,
            clip: SCREEN,
        })
        .collect();
    for (n, place) in family::PLACES.into_iter().enumerate().rev() {
        let (x, y) = at(place);
        let from = |(right, down): (i16, i16)| (x + right, y + down);
        let (bits, name) = match n.checked_sub(1) {
            None => (
                strength_signs(record.strengths),
                Drawn::CreatureName {
                    creature,
                    at: from(family::NAME),
                    clip: SCREEN,
                },
            ),
            Some(ancestor) => (
                family.signs[ancestor].into(),
                Drawn::AncestorName {
                    creature,
                    ancestor,
                    at: from(family::NAME),
                },
            ),
        };
        drawn.push(Drawn::Frame {
            rect: rect(x, y, family::BOX.0, family::BOX.1),
            mode: 0,
            time: 0,
        });
        drawn.extend(sign_row(bits, from(family::SIGN)));
        drawn.push(name);
    }
    drawn
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pad::Pad;
    use crate::player_menu::Nothing;
    use crate::player_menu::{OPEN, SCREENS, Tables};

    fn tables() -> Tables {
        let mut tables = Tables {
            cursors: [[1, 1, 1, 0]; SCREENS],
            confirm: [-1; SCREENS],
            cancel: [-1; SCREENS],
            panels: [0; SCREENS],
            kept: [-1; SCREENS],
            back: [-1; SCREENS],
            of_player: [13, 1, 6, 10, 14],
            of_creature: [11, 4, 7, 7],
            player_panels: [0x421, 0x821, 0x21, 0x1021, 0x2021],
            creature_panels: [0x1C0, 0xD0, 0xC0, 0x2C0],
        };
        tables.cursors[0] = [1, 1, 1, 0x20];
        tables.panels[0] = 0x06;
        tables
    }

    fn player() -> Record {
        let mut head = [0; Record::HEAD];
        head[4..8].copy_from_slice(&(LEVEL / 2).to_le_bytes());
        head[8..10].copy_from_slice(&30i16.to_le_bytes());
        head[10..12].copy_from_slice(&40i16.to_le_bytes());
        head[0x44] = 3;
        Record::head(&head).expect("a head")
    }

    const COUNTS: Counts = Counts {
        cocoons: 5,
        waiting: 0,
        kinds: 12,
        floor: None,
    };

    #[test]
    fn the_wall_covers_the_screen_with_every_other_row_shifted() {
        let stones: Vec<_> = wall().collect();
        assert_eq!(stones.len(), 12);
        assert_eq!(stones[..3], [(0, 0), (128, 0), (256, 0)]);
        assert_eq!(stones[3..6], [(-64, 60), (64, 60), (192, 60)]);
        assert_eq!(stones[11], (192, 180));
        // No pixel of the 320 by 240 of the screen is left out.
        for (x, y) in (0..240i16).flat_map(|y| (0..320i16).map(move |x| (x, y))) {
            let covered = (stones.iter()).any(|&(left, top)| {
                (left..left + 128).contains(&x) && (top..top + 60).contains(&y)
            });
            assert!(covered, "({x}, {y})");
        }
    }

    #[test]
    fn a_bar_is_never_longer_than_its_gauge_and_shows_any_value() {
        assert_eq!(bar_length(0, 40, 87), 0);
        assert_eq!(bar_length(40, 40, 87), 87);
        assert_eq!(bar_length(90, 40, 87), 87);
        assert_eq!(bar_length(20, 40, 87), 43);
        // A value too small for a pixel still shows one.
        assert_eq!(bar_length(1, 999, 47), 1);
        assert_eq!(bar_length(3, 0, 47), 0);
    }

    #[test]
    fn a_gauge_picks_its_frame_and_its_bar_by_kind_and_value() {
        let clip = rect(0, 0, 320, 240);
        let parts = |kind: u8, value: i32, most: i32| {
            let [frame, bar] = gauge(kind, value, most, (100, 50), clip);
            let Drawn::Sprite { number, at, .. } = frame else {
                panic!("the frame is a sprite");
            };
            assert_eq!(at, (100, 50));
            let Drawn::Bar {
                picture,
                length,
                at,
                ..
            } = bar
            else {
                panic!("the bar is a bar");
            };
            assert_eq!(at, (108, 52));
            (number, picture, length)
        };
        assert_eq!(parts(0, 40, 40), (sprite::LONG_GAUGE, bar::USUAL, 87));
        // Under a quarter the bar changes its picture; a quarter itself does not.
        assert_eq!(parts(0, 10, 40), (sprite::LONG_GAUGE, bar::USUAL, 21));
        assert_eq!(parts(0, 9, 40), (sprite::LONG_GAUGE, bar::LOW, 19));
        assert_eq!(parts(1, 9, 40), (sprite::SHORT_GAUGE, bar::LOW, 10));
        assert_eq!(parts(1, 40, 40), (sprite::SHORT_GAUGE, bar::USUAL, 47));
        assert_eq!(parts(2, 1, 400), (sprite::SHORT_GAUGE, bar::EXPERIENCE, 1));
    }

    #[test]
    fn the_results_screen_draws_the_boxes_whole_and_marks_a_gained_level() {
        let he = player();
        let frames = |drawn: &[Drawn]| -> Vec<(Rect, u32)> {
            (drawn.iter())
                .filter_map(|drawn| match *drawn {
                    Drawn::Frame { rect, time, .. } => Some((rect, time)),
                    _ => None,
                })
                .collect()
        };
        // Nothing gained: his boxes alone, whole and plain, as the menu draws them.
        let plain = Counted {
            record: &he,
            gained: false,
        };
        let drawn = results_boxes(plain, &[], Counts::default(), 7);
        assert_eq!(drawn, player_boxes(OPENING, 1, 0, &he, Counts::default()));
        assert!(frames(&drawn).contains(&(RIGHT_FRAME, 0)));
        // A level gained: the small frame with its sprite underneath, and his frames
        // pulse by the pass counter.
        let gained = Counted {
            record: &he,
            gained: true,
        };
        let drawn = results_boxes(gained, &[], Counts::default(), 7);
        let small = rect(108, 26, 108, 18);
        assert_eq!(frames(&drawn)[0], (small, 7));
        assert_eq!(
            drawn[1],
            Drawn::Sprite {
                number: 228,
                at: (111, 28),
                clip: small
            }
        );
        assert!(frames(&drawn).contains(&(RIGHT_FRAME, 7)));
        // Two carried creatures, the second with a level: its small frame stands below
        // its box, and only its box pulses.
        let two = [plain, gained];
        let drawn = results_boxes(plain, &two, Counts::default(), 7);
        let (y, w, h) = CREATURE_BOX;
        let framed = frames(&drawn);
        assert_eq!(framed[0], (rect(168, 191, 84, 18), 7));
        assert!(framed.contains(&(rect(60, y, w, h), 0)));
        assert!(framed.contains(&(rect(160, y, w, h), 7)));
        assert!(drawn.contains(&Drawn::Sprite {
            number: 93,
            at: (174, 193),
            clip: rect(168, 191, 84, 18)
        }));
    }

    #[test]
    fn the_panel_of_a_new_rank_is_a_face_and_two_rows() {
        let drawn = rank_panel(true, 40);
        // Every frame lies under what stands in it, plain and without a time.
        let frames: Vec<(usize, Rect)> = (drawn.iter().enumerate())
            .filter_map(|(place, drawn)| match *drawn {
                Drawn::Frame {
                    rect,
                    mode: 0,
                    time: 0,
                } => Some((place, rect)),
                _ => None,
            })
            .collect();
        assert_eq!(
            frames,
            [
                (0, rect(92, 147, 137, 21)),
                (3, rect(92, 123, 137, 21)),
                (6, rect(126, 52, 68, 68))
            ]
        );
        assert_eq!(
            drawn[1],
            Drawn::Sprite {
                number: 194,
                at: (97, 153),
                clip: SCREEN
            }
        );
        assert_eq!(
            drawn[2],
            Drawn::RankName {
                before: true,
                dark: 40,
                at: (129, 153)
            }
        );
        assert!(drawn.contains(&Drawn::Sprite {
            number: 17,
            at: (97, 128),
            clip: SCREEN
        }));
        assert!(drawn.contains(&Drawn::Name {
            at: (129, 129),
            clip: SCREEN
        }));
        assert_eq!(
            drawn[7],
            Drawn::Face {
                at: (128, 54),
                clip: SCREEN
            }
        );
        // What the pass says of the name is handed on as it is.
        assert!(rank_panel(false, 4).contains(&Drawn::RankName {
            before: false,
            dark: 4,
            at: (129, 153)
        }));
    }

    #[test]
    fn the_levels_that_wait_are_not_part_of_the_way() {
        assert_eq!(experience_shown(0), 0);
        assert_eq!(experience_shown(LEVEL), LEVEL as i32);
        assert_eq!(experience_shown(LEVEL + 7), 7);
        assert_eq!(experience_shown(3 * LEVEL + 7), 7);
    }

    #[test]
    fn the_digits_of_a_number_stand_from_the_right() {
        // Every digit 8 wide but the 1, which is 5.
        let width = |number: usize| match number - sprite::DIGITS {
            1 => 5,
            _ => 8,
        };
        assert_eq!(
            digits(0, (306, 50), false, width),
            [(sprite::DIGITS, (299, 50))]
        );
        assert_eq!(
            digits(412, (306, 50), false, width),
            [
                (sprite::DIGITS + 2, (299, 50)),
                (sprite::DIGITS + 1, (298, 50)),
                (sprite::DIGITS + 4, (285, 50)),
            ]
        );
        assert_eq!(digits(123_456_789, (0, 0), false, width).len(), 7);
        let small = digits(1234, (0, 0), true, |_| 6);
        assert_eq!(small.len(), 3);
        assert_eq!(small[0], (sprite::SMALL_DIGITS + 9, (-5, 0)));
    }

    #[test]
    fn the_panel_opens_about_the_middle_of_its_boxes() {
        let tables = tables();
        let mut menu = PlayerMenu::open(&tables, 0, 0);
        let frames = |menu: &PlayerMenu, passes: u32| -> Vec<(Rect, u8, u32)> {
            (player_panel(menu, &player(), COUNTS, passes).into_iter())
                .filter_map(|drawn| match drawn {
                    Drawn::Frame { rect, mode, time } => Some((rect, mode, time)),
                    _ => None,
                })
                .collect()
        };
        // Closed, every box is empty and nothing of it shows.
        assert!(frames(&menu, 5).iter().all(|(rect, ..)| rect.w == 0));
        for _ in 0..4 {
            menu.pass(&tables, &mut Nothing, &Pad::default());
        }
        let half = frames(&menu, 5);
        assert_eq!(half[3].0, rect(27, 61, 34, 34));
        for _ in 0..OPEN {
            menu.pass(&tables, &mut Nothing, &Pad::default());
        }
        let whole = frames(&menu, 5);
        assert_eq!(
            whole,
            [
                (rect(219, 44, 91, 23), 1, 5),
                (rect(219, 67, 91, 22), 1, 5),
                (rect(219, 89, 91, 23), 1, 5),
                (LEFT_FRAME, 1, 5),
                (RIGHT_FRAME, 1, 5),
                (HEADING, 0, 0),
                (THIRD_ROW, 1, 0),
            ]
        );
        // In the corridor of the second round screen 0 has one box more, a plain one.
        let floor = Counts {
            floor: Some(17),
            ..COUNTS
        };
        let there = player_panel(&menu, &player(), floor, 5);
        assert_eq!(
            there.len(),
            player_panel(&menu, &player(), COUNTS, 5).len() + 3
        );
        assert_eq!(
            there[there.len() - 3..],
            [
                Drawn::Frame {
                    rect: CORRIDOR_BOX,
                    mode: 0,
                    time: 0
                },
                Drawn::Sprite {
                    number: sprite::CORRIDOR,
                    at: (15, 19),
                    clip: CORRIDOR_BOX
                },
                Drawn::Number {
                    value: 17,
                    ends: (84, 25),
                    small: false,
                    clip: CORRIDOR_BOX
                },
            ]
        );
        // The time runs only for the row the cursor is in.
        menu.pass(&tables, &mut Nothing, &{
            let mut pad = Pad::default();
            pad.read(0, crate::pad::Repeat::of(1), 0);
            pad.read(crate::pad::DOWN, crate::pad::Repeat::of(1), 1);
            pad
        });
        assert_eq!(menu.flags, flag::THIRD_ROW);
        let below = frames(&menu, 9);
        assert_eq!((below[0].2, below[4].2), (0, 0));
        assert_eq!(below[6], (THIRD_ROW, 1, 9));
        assert_eq!(help(&menu), HELP_PLAIN);
    }

    #[test]
    fn the_values_of_the_player_stand_where_the_game_puts_them() {
        let tables = tables();
        let mut menu = PlayerMenu::open(&tables, 0, 0);
        for _ in 0..=OPEN {
            menu.pass(&tables, &mut Nothing, &Pad::default());
        }
        assert_eq!(help(&menu), HELP_OF_A_ROW);
        let drawn = player_panel(&menu, &player(), COUNTS, 0);
        let sprite_at = |wanted: usize| {
            drawn.iter().find_map(|drawn| match *drawn {
                Drawn::Sprite { number, at, .. } if number == wanted => Some(at),
                _ => None,
            })
        };
        assert_eq!(sprite_at(sprite::EMPTIES), Some((224, 49)));
        assert_eq!(sprite_at(sprite::FIREFLIES), Some((224, 72)));
        assert_eq!(sprite_at(sprite::CAPTURED), Some((224, 94)));
        assert_eq!(sprite_at(sprite::NAME), Some((82, 49)));
        assert_eq!(sprite_at(sprite::HP), Some((82, 65)));
        assert_eq!(sprite_at(sprite::LONG_GAUGE), Some((112, 64)));
        assert_eq!(sprite_at(sprite::STROKE), Some((159, 72)));
        assert_eq!(sprite_at(sprite::CAPTURE_LEVEL), Some((82, 80)));
        assert_eq!(sprite_at(sprite::EXPERIENCE), Some((82, 96)));
        assert_eq!(sprite_at(sprite::SHORT_GAUGE), Some((153, 98)));
        assert_eq!(sprite_at(sprite::STATUS), Some((147, 21)));
        assert_eq!(sprite_at(sprite::OPTIONS), Some((132, 196)));
        let numbers: Vec<_> = (drawn.iter())
            .filter_map(|drawn| match *drawn {
                Drawn::Number {
                    value, ends, small, ..
                } => Some((value, ends, small)),
                _ => None,
            })
            .collect();
        assert_eq!(
            numbers,
            [
                (5, (306, 50), false),
                (0, (306, 73), false),
                (12, (306, 95), false),
                (30, (159, 72), true),
                (40, (186, 72), true),
                (3, (206, 81), false),
            ]
        );
        let bars: Vec<_> = (drawn.iter())
            .filter_map(|drawn| match *drawn {
                Drawn::Bar {
                    picture,
                    length,
                    at,
                    ..
                } => Some((picture, length, at)),
                _ => None,
            })
            .collect();
        assert_eq!(
            bars,
            [
                (bar::USUAL, 65, (120, 66)),
                (bar::EXPERIENCE, 23, (161, 100))
            ]
        );
        assert!(drawn.contains(&Drawn::Name {
            at: (115, 50),
            clip: RIGHT_FRAME
        }));
        assert!(drawn.contains(&Drawn::Face {
            at: (12, 46),
            clip: LEFT_FRAME
        }));
    }

    #[test]
    fn a_carried_creature_has_its_box_by_how_many_are_carried() {
        let tables = tables();
        let mut creature = player();
        (creature.mp, creature.max_mp) = (5, 26);
        creature.element_bit = 2;
        let boxes = |carried: usize, passes: u32| {
            let mut menu = PlayerMenu::open(&tables, carried as i16, 0);
            for _ in 0..=OPEN {
                menu.pass(&tables, &mut Nothing, &Pad::default());
            }
            let mut pad = Pad::default();
            pad.read(0, crate::pad::Repeat::of(1), 0);
            pad.read(crate::pad::DOWN, crate::pad::Repeat::of(1), 1);
            menu.pass(&tables, &mut Nothing, &pad);
            creature_boxes(&menu, &vec![creature.clone(); carried], passes)
        };
        let frames = |drawn: &[Drawn]| -> Vec<(i16, u32)> {
            (drawn.iter())
                .filter_map(|drawn| match *drawn {
                    Drawn::Frame {
                        rect,
                        mode: 1,
                        time,
                    } => {
                        assert_eq!((rect.y, rect.w, rect.h), (114, 100, 77));
                        Some((rect.x, time))
                    }
                    _ => None,
                })
                .collect()
        };
        assert!(boxes(0, 7).is_empty());
        assert_eq!(frames(&boxes(1, 7)), [(110, 7)]);
        assert_eq!(frames(&boxes(2, 7)), [(60, 7), (160, 0)]);
        // Only the box under the cursor has a time.
        let three = boxes(3, 7);
        assert_eq!(frames(&three), [(10, 7), (110, 0), (210, 0)]);

        // The inside of the first of three, from its corner (10, 114).
        let first: Vec<Drawn> = three[..three.len() / 3].to_vec();
        let clip = rect(10, 114, 100, 77);
        let sprite_at = |wanted: usize| {
            first.iter().find_map(|drawn| match *drawn {
                Drawn::Sprite { number, at, .. } if number == wanted => Some(at),
                _ => None,
            })
        };
        assert!(first.contains(&Drawn::CreatureName {
            creature: 0,
            at: (17, 117),
            clip
        }));
        assert_eq!(sprite_at(creature::LEVEL), Some((26, 141)));
        assert_eq!(sprite_at(creature::MANA), Some((17, 154)));
        assert_eq!(sprite_at(creature::EXP), Some((17, 169)));
        // Wind is the creature's element: its letter, and an arrow for the three others.
        assert_eq!(sprite_at(creature::HAS[1]), Some((93, 132)));
        assert_eq!(sprite_at(creature::HAS_NOT[0]), Some((93, 118)));
        assert_eq!(sprite_at(creature::HAS_NOT[2]), Some((79, 132)));
        assert_eq!(sprite_at(creature::HAS_NOT[3]), Some((79, 118)));
        let numbers: Vec<_> = (first.iter())
            .filter_map(|drawn| match *drawn {
                Drawn::Number {
                    value, ends, small, ..
                } => Some((value, ends, small)),
                _ => None,
            })
            .collect();
        assert_eq!(
            numbers,
            [
                (30, (42, 132), true),
                (40, (69, 132), true),
                (3, (65, 143), true),
                (5, (71, 160), true),
                (26, (98, 160), true),
            ]
        );
        let bars: Vec<_> = (first.iter())
            .filter_map(|drawn| match *drawn {
                Drawn::Bar {
                    picture,
                    length,
                    at,
                    ..
                } => Some((picture, length, at)),
                _ => None,
            })
            .collect();
        assert_eq!(
            bars,
            [
                (bar::USUAL, 35, (21, 127)),
                (bar::LOW, 9, (50, 155)),
                (bar::EXPERIENCE, 23, (50, 182)),
            ]
        );
        // Without an element all four signs are the dash.
        let dashes = element_sign(0, (0, 0), clip);
        assert!(dashes.iter().all(
            |drawn| matches!(drawn, Drawn::Sprite { number, .. } if *number == creature::NONE)
        ));
    }

    /// The menu on the player's row of entries, whole, with the tables of that screen.
    fn on_the_entries() -> (Tables, PlayerMenu) {
        let mut tables = tables();
        tables.cursors[6] = [5, 1, 5, 0x01];
        (tables.cancel[0], tables.cancel[6]) = (-2, 0);
        (tables.panels[6], tables.kept[6]) = (0x21, 0);
        let mut menu = PlayerMenu::open(&tables, 0, 0);
        for _ in 0..=OPEN {
            menu.pass(&tables, &mut Nothing, &Pad::default());
        }
        menu.pass(&tables, &mut Nothing, &press(crate::pad::CROSS));
        for _ in 0..=OPEN {
            menu.pass(&tables, &mut Nothing, &Pad::default());
        }
        assert_eq!((menu.screen, menu.cursor.at), (OF_PLAYER, 2));
        (tables, menu)
    }

    fn press(buttons: u16) -> Pad {
        let mut pad = Pad::default();
        pad.read(0, crate::pad::Repeat::of(1), 0);
        pad.read(buttons, crate::pad::Repeat::of(1), 1);
        pad
    }

    /// Three entries of one piece each: a word in the middle of its frame.
    fn entry_lines() -> Vec<Vec<Piece>> {
        [(32, 66, 5), (100, 43, 7), (145, 43, 7)]
            .into_iter()
            .zip([82u32, 88, 49])
            .map(|((x, w, margin), value)| {
                vec![Piece {
                    flags: piece::CENTRED,
                    kind: kind::PICTURE,
                    value,
                    rect: rect(x, 18, w, 19),
                    margin: (margin, 3),
                }]
            })
            .collect()
    }

    #[test]
    fn a_row_of_entries_pulses_under_the_cursor_of_its_own_screen() {
        let (tables, mut menu) = on_the_entries();
        let lines = entry_lines();
        let row = entries(&menu, false, &lines, 9, |_| 30);
        let frames: Vec<_> = (row.iter())
            .filter_map(|drawn| match *drawn {
                Drawn::Frame { rect, mode, time } => Some((rect.x, mode, time)),
                _ => None,
            })
            .collect();
        // The cursor starts on the third entry: its frame alone has the time.
        assert_eq!(frames, [(32, 1, 0), (100, 1, 0), (145, 1, 9)]);
        // A word lies in the middle of its room, the frame less its margin: half of
        // the 26 that 56 leaves a word of 30.
        assert!(row.contains(&Drawn::Sprite {
            number: 82,
            at: (37 + 13, 21),
            clip: rect(32, 18, 66, 19)
        }));
        // The row is drawn with the cursor the screen kept, a pass behind the cursor.
        menu.pass(&tables, &mut Nothing, &press(crate::pad::LEFT));
        assert_eq!((menu.cursor.at, menu.entry_of_player()), (1, 2));
        menu.pass(&tables, &mut Nothing, &Pad::default());
        assert_eq!(menu.entry_of_player(), 1);
        // Behind another screen the entry has the marked colour and nothing pulses.
        menu.screen = 1;
        let behind = entries(&menu, false, &lines, 9, |_| 30);
        let modes: Vec<_> = (behind.iter())
            .filter_map(|drawn| match *drawn {
                Drawn::Frame { mode, time, .. } => Some((mode, time)),
                _ => None,
            })
            .collect();
        assert_eq!(modes, [(0, 0), (2, 0), (0, 0)]);
    }

    #[test]
    fn the_rank_follows_the_level() {
        let ranks: Vec<_> = [1, 3, 4, 19, 20, 30, 31, 34, 35, 39, 40, 99]
            .into_iter()
            .map(rank)
            .collect();
        assert_eq!(ranks, [0, 0, 1, 4, 5, 5, 6, 6, 7, 7, 8, 8]);
    }

    #[test]
    fn the_detail_of_the_player_stands_where_the_game_puts_it() {
        let (_, menu) = on_the_entries();
        let piece = |flags: u16, kind: u16, w: i16, margin: (i16, i16)| Piece {
            flags,
            kind,
            value: 0,
            rect: rect(0, 0, w, 21),
            margin,
        };
        let line = |margin: i16| {
            vec![
                piece(piece::CENTRED, kind::PICTURE, 50, (margin, 5)),
                piece(0x10, kind::PICTURE, 21, (3, 5)),
                piece(0x10, kind::TEXT, 112, (4, 4)),
            ]
        };
        let lines = [line(10), line(11), line(5)];
        let shown = Detail {
            values: [12, 8, 5, 6, 9],
            worn: [true, false, false],
            money: 1234,
        };
        let drawn = player_detail(&menu, &player(), COUNTS, &shown, &lines, |_| 30);
        // Every frame is a plain one without a time, the player's too.
        let frames: Vec<Rect> = (drawn.iter())
            .filter_map(|drawn| match *drawn {
                Drawn::Frame { rect, mode, time } => {
                    assert_eq!((mode, time), (0, 0));
                    Some(rect)
                }
                _ => None,
            })
            .collect();
        assert_eq!(frames.len(), 2 + 9 + 1 + 5);
        assert_eq!(frames[..2], [detail::MONEY_BOX, detail::RANK_BOX]);
        assert_eq!(
            frames[2..5],
            [
                rect(10, 136, 50, 21),
                rect(60, 136, 21, 21),
                rect(81, 136, 112, 21)
            ]
        );
        assert_eq!(frames[8], rect(10, 180, 50, 21));
        assert_eq!(frames[11], detail::NUMBERS);
        assert_eq!(frames[15..], [LEFT_FRAME, RIGHT_FRAME]);
        // Only the place with a thing on has a picture and a name.
        let worn: Vec<_> = (drawn.iter())
            .filter(|drawn| matches!(drawn, Drawn::WornPicture { .. } | Drawn::WornName { .. }))
            .collect();
        assert_eq!(
            worn,
            [
                &Drawn::WornPicture {
                    place: 0,
                    at: (63, 141),
                    clip: rect(60, 136, 21, 21)
                },
                &Drawn::WornName {
                    place: 0,
                    at: (85, 140),
                    clip: rect(81, 136, 112, 21)
                }
            ]
        );
        assert!(drawn.contains(&Drawn::Rank {
            at: (47, 119),
            clip: detail::RANK_BOX
        }));
        let numbers: Vec<_> = (drawn.iter())
            .filter_map(|drawn| match *drawn {
                Drawn::Number {
                    value, ends, small, ..
                } => Some((value, ends, small)),
                _ => None,
            })
            .collect();
        assert_eq!(numbers[0], (1234, (260, 118), false));
        assert_eq!(
            numbers[1..6],
            [
                (12, (278, 141), false),
                (8, (278, 155), false),
                (5, (278, 169), false),
                (6, (278, 183), false),
                (9, (278, 197), false),
            ]
        );
        let sprite_at = |wanted: usize| {
            drawn.iter().find_map(|drawn| match *drawn {
                Drawn::Sprite { number, at, .. } if number == wanted => Some(at),
                _ => None,
            })
        };
        assert_eq!(sprite_at(detail::WORDS[0]), Some((215, 140)));
        assert_eq!(sprite_at(detail::WORDS[4]), Some((215, 196)));
        assert_eq!(sprite_at(detail::MONEY), Some((154, 117)));
        assert_eq!(sprite_at(detail::YAN), Some((260, 118)));
        assert_eq!(sprite_at(detail::RANK), Some((15, 119)));
        // The word of a row lies in the middle of its room of 50 less twice the margin.
        assert_eq!(sprite_at(detail::WORN[0]), Some((20, 141)));
        assert_eq!(sprite_at(detail::WORN[2]), Some((20, 185)));
        // The heading and the third row are the first level's alone.
        assert_eq!(sprite_at(sprite::STATUS), None);
        assert_eq!(sprite_at(sprite::NAME), Some((82, 49)));
    }

    #[test]
    fn a_name_in_a_box_is_in_capitals() {
        let unit = |second: u8| u16::from_le_bytes([0x82, second]);
        assert_eq!(in_capitals(unit(0x81)), unit(0x60));
        assert_eq!(in_capitals(unit(0x9A)), unit(0x79));
        // Capitals, digits and the space stay.
        assert_eq!(in_capitals(unit(0x60)), unit(0x60));
        assert_eq!(in_capitals(unit(0x4F)), unit(0x4F));
        assert_eq!(in_capitals(u16::from_le_bytes([0x81, 0x40])), 0x4081);
    }

    #[test]
    fn the_marker_of_the_build_goes_by_the_growth() {
        // Even growth stands in the middle, 23 of 47.
        assert_eq!(build_marker([5, 5, 5, 5, 5, 0]), (23, 23));
        // All speed and all magic: the far corner; none of either: the near one.
        assert_eq!(build_marker([0, 0, 9, 0, 9, 0]), (47, 47));
        assert_eq!(build_marker([9, 4, 0, 5, 0, 0]), (0, 0));
        // The two defenses count as their half sum, rounded down.
        assert_eq!(build_marker([1, 3, 3, 4, 6, 0]), (47 * 6 / 9, 47 * 3 / 4));
        assert_eq!(build_marker([0; 6]), (0, 0));
    }

    #[test]
    fn a_triangle_of_the_wheel_grows_from_the_middle_into_its_quarter() {
        assert_eq!(
            [0, 7, 8, 9, 16, 32, 33].map(wheel_length),
            [0, 0, 12, 12, 17, 24, 24]
        );
        assert_eq!(wheel_length(34), 0);
        let wheel = (100, 50);
        assert_eq!(
            [0, 1, 2, 3].map(|quarter| wheel_triangle(quarter, 10, wheel)),
            [
                [(126, 76), (126, 65), (115, 76)],
                [(126, 76), (126, 65), (136, 76)],
                [(126, 76), (126, 86), (136, 76)],
                [(126, 76), (126, 86), (116, 76)]
            ]
        );
    }

    #[test]
    fn a_panel_of_a_level_up_tints_what_changed_and_puts_the_difference_beside_it() {
        let mut was = player();
        (was.level, was.max_hp, was.max_mp) = (7, 60, 10);
        let mut now = was.clone();
        (now.level, now.max_hp, now.max_mp) = (9, 172, 10);
        let panel = level_panel(1, (&now, [11, 12, 13, 14, 15]), (&was, [9, 12, 20, 14, 15]));
        let has = |wanted: Drawn| panel.contains(&wanted);
        // A plain frame, whole at once, and nothing is cut.
        let clip = rect(0, 0, 320, 240);
        let frame = Drawn::Frame {
            rect: rect(51, 62, 218, 117),
            mode: 0,
            time: 0,
        };
        assert_eq!(panel[0], frame);
        let tinted = |value: i32, ends: (i16, i16), colour: usize| Drawn::Tinted {
            value,
            ends,
            colour,
            clip,
        };
        let small = |value: i32, ends: (i16, i16)| Drawn::Number {
            value,
            ends,
            small: true,
            clip,
        };
        let arrow = |turns: u16, at: (i16, i16)| Drawn::Arrow { turns, at };
        // The level rose by 2: under colour 1, with an arrow up and the 2 beside it.
        assert!(has(tinted(9, (117, 80), 1)));
        assert!(has(arrow(sign::ABOVE, (121, 85))));
        assert!(has(small(2, (138, 82))));
        // The most HP rose by 112: the arrow stands 8 further left. The mana stays.
        assert!(has(arrow(sign::ABOVE, (93, 123))));
        assert!(has(small(112, (118, 120))));
        let arrows = panel
            .iter()
            .filter(|drawn| matches!(drawn, Drawn::Arrow { .. }));
        assert_eq!(arrows.count(), 4);
        // The first value rose by 2, the third fell by 7, the others stand as they are.
        assert!(has(tinted(11, (191, 98), 1)));
        assert!(has(small(2, (211, 100))));
        assert!(has(tinted(12, (191, 112), 0)));
        assert!(has(tinted(13, (191, 133), 2)));
        assert!(has(arrow(sign::BELOW, (194, 138))));
        assert!(has(small(7, (211, 135))));
        // The menu's own "Detail" compares nothing.
        let (_, menu) = on_the_entries();
        let plain = creature_detail(&menu, 1, &now, [11, 12, 13, 14, 15]);
        let compares = |drawn: &Drawn| matches!(drawn, Drawn::Arrow { .. } | Drawn::Tinted { .. });
        assert!(!plain.iter().any(compares));
    }

    #[test]
    fn the_detail_of_a_creature_stands_where_the_game_puts_it() {
        let (_, menu) = on_the_entries();
        let mut record = player();
        record.strengths = [20, 0, 7, 32];
        record.growth = [5, 5, 5, 5, 5, 0];
        (record.hp, record.max_hp, record.mp, record.max_mp) = (30, 60, 5, 10);
        record.level = 7;
        let drawn = creature_detail(&menu, 1, &record, [11, 12, 13, 14, 15]);
        // The box comes first: everything else lies on it and is cut to it.
        let Drawn::Frame {
            rect: clip,
            mode: 0,
            time: 0,
        } = drawn[0]
        else {
            panic!("the box is not first");
        };
        let has = |wanted: Drawn| drawn.contains(&wanted);
        let at = |wanted: Drawn| drawn.iter().position(|&drawn| drawn == wanted);
        let sprite = |number: usize, at: (i16, i16)| Drawn::Sprite { number, at, clip };
        // The words on the left and the name behind the first.
        for (number, at) in [
            (17, (54, 66)),
            (92, (55, 79)),
            (60, (55, 92)),
            (84, (52, 117)),
            (114, (52, 148)),
        ] {
            assert!(has(sprite(number, at)), "{number}");
        }
        assert!(has(Drawn::FullName {
            creature: 1,
            at: (89, 67),
            clip
        }));
        let number = |value: i32, ends: (i16, i16), small: bool| Drawn::Number {
            value,
            ends,
            small,
            clip,
        };
        assert!(has(number(7, (117, 80), false)));
        // The gauges: the experience, the HP and the mana with their small numbers.
        for y in [105, 131, 161] {
            assert!(has(sprite(sprite::SHORT_GAUGE, (54, y))), "{y}");
        }
        assert!(has(number(30, (54 + 8 + 21, 131 + 7), true)));
        assert!(has(number(10, (54 + 8 + 48, 161 + 7), true)));
        // The five values with their words.
        for ((word, row), value) in [98, 21, 109, 107, 184]
            .into_iter()
            .zip([35, 49, 70, 84, 102])
            .zip(11..)
        {
            assert!(has(sprite(word, (51 + 78, 62 + row))));
            assert!(has(number(value, (51 + 140, 62 + row + 1), false)));
        }
        // The wheel: its picture under two triangles (fire 20, water 32; wind has none
        // and earth is under 8), and the four letters on top.
        let wheel = (51 + 162, 62 + 4);
        let triangles: Vec<_> = (drawn.iter())
            .filter_map(|drawn| match drawn {
                Drawn::Triangle {
                    corners, colour, ..
                } => Some((*corners, *colour)),
                _ => None,
            })
            .collect();
        assert_eq!(
            triangles,
            [
                (wheel_triangle(1, 19, wheel), [159, 29, 29]),
                (wheel_triangle(0, 24, wheel), [89, 82, 218])
            ]
        );
        let picture = at(sprite(11, wheel)).expect("the wheel");
        let last = (drawn.iter()).rposition(|drawn| matches!(drawn, Drawn::Triangle { .. }));
        let first = (drawn.iter()).position(|drawn| matches!(drawn, Drawn::Triangle { .. }));
        assert!(Some(picture) < first);
        // A letter lies on the triangle of its own quarter.
        let letter = |number: usize, right: i16, down: i16| {
            at(sprite(number, (wheel.0 + right, wheel.1 + down)))
        };
        assert!(letter(61, 37, 2) > first && letter(222, 2, 2) > last);
        assert!(letter(231, 37, 37).is_some() && letter(101, 2, 37).is_some());
        // The square: the marker of an even growth on the picture, on the cross.
        let build = (51 + 162, 62 + 61);
        let marker = at(sprite(10, (build.0 + 23, build.1 + 23))).expect("the marker");
        let words = at(sprite(12, (build.0 + 2, build.1 + 2))).expect("the picture");
        let cross = at(Drawn::Line {
            from: (build.0 + 2, build.1 + 26),
            to: (build.0 + 50, build.1 + 26),
            grey: 96,
            clip,
        });
        assert!(cross.is_some() && cross < Some(words) && words < marker);
        let lines = (drawn.iter()).filter(|drawn| matches!(drawn, Drawn::Line { .. }));
        assert_eq!(lines.count(), 2 + 4 + 4);
        assert!(has(Drawn::Line {
            from: (build.0 + 50, build.1),
            to: (build.0 + 50, build.1 + 50),
            grey: 117,
            clip
        }));
        assert!(has(Drawn::Line {
            from: (build.0 + 1, build.1 + 1),
            to: (build.0 + 49, build.1 + 1),
            grey: 189,
            clip
        }));
    }

    #[test]
    fn a_message_is_a_plain_box_with_its_text_inside() {
        let (_, mut menu) = on_the_entries();
        assert_eq!(message(&menu), None);
        menu.said = crate::player_menu::NOTHING_CAUGHT;
        assert_eq!(
            message(&menu),
            Some([
                Drawn::Frame {
                    rect: rect(10, 153, 300, 60),
                    mode: 0,
                    time: 0
                },
                Drawn::Message { at: (14, 158) }
            ])
        );
        // The four lines fill the box, and it ends where the line of help begins.
        let lines = message::LINE * message::LINES as i16;
        assert!(message::TEXT.1 + lines <= message::BOX.h);
        assert_eq!(message::BOX.y + message::BOX.h, 213);
        // A thing's own text is not one of the table's.
        menu.said = message::RESULTS;
        assert_eq!(message(&menu), None);
    }

    #[test]
    fn the_text_of_a_result_is_the_one_the_files_table_names() {
        let base = message::TEXTS - 8;
        let table = 8 + 4 * message::RESULTS as usize;
        let mut file = vec![0u8; table + 8];
        let text = base + table as u32;
        file[8 + 4 * 9..][..4].copy_from_slice(&text.to_le_bytes());
        file[table..table + 4].copy_from_slice(&[0x82, 0x6D, 0x82, 0x8F]);
        assert_eq!(message_text(&file, base, 9), Some(vec![0x6D82, 0x8F82]));
        // A result without a text of its own, and what is not a result of the table.
        assert_eq!(message_text(&file, base, 8), None);
        assert_eq!(message_text(&file, base, 0), None);
        assert_eq!(message_text(&file, base, message::RESULTS), None);
        assert_eq!(message_text(&file[..16], base, 9), None);
    }

    #[test]
    fn a_list_of_help_ends_at_a_sprite_below_zero() {
        let mut data = Vec::new();
        for (sprite, word) in [(16i32, 0x8000_0100u32), (167, 0x8000_0200), (-1, 0)] {
            data.extend_from_slice(&sprite.to_le_bytes());
            data.extend_from_slice(&word.to_le_bytes());
        }
        let base = 0x8000_0000;
        assert_eq!(
            help_list(&data, base, base),
            Some(vec![(16, 0x8000_0100), (167, 0x8000_0200)])
        );
        assert_eq!(
            help_list(&data, base, base + 8).map(|list| list.len()),
            Some(1)
        );
        // A list that runs out of the image is none.
        assert_eq!(help_list(&data, base, base + 0x20), None);
    }

    /// The four lines of the list as the executable holds them: a picture, a name and
    /// a count, all at the first row's place until the drawer writes theirs.
    fn item_lines() -> Vec<Vec<Piece>> {
        let piece = |flags: u16, kind: u16, x: i16, w: i16, margin: (i16, i16)| Piece {
            flags,
            kind,
            value: 0,
            rect: rect(x, 54, w, 21),
            margin,
        };
        let line = vec![
            piece(0x10, kind::PICTURE, 83, 21, (3, 5)),
            piece(0x10, kind::TEXT, 104, 112, (4, 4)),
            piece(piece::RIGHT, 8, 216, 23, (0, 3)),
        ];
        vec![line; items::ROWS]
    }

    fn frames_of(drawn: &[Drawn]) -> Vec<(i16, i16, u8, u32)> {
        (drawn.iter())
            .filter_map(|drawn| match *drawn {
                Drawn::Frame { rect, mode, time } => Some((rect.x, rect.y, mode, time)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_list_of_the_items_has_four_rows_of_three_framed_pieces() {
        let mut menu = PlayerMenu::open(&tables(), 0, 0);
        menu.screen = ITEMS;
        // The cursor on the second row, the list one line down.
        (menu.kept[3], menu.kept_first[0]) = (1, 1);
        let row = |item: u8, count: u8, refused: bool| ItemRow {
            item,
            count,
            refused,
        };
        let rows = [
            row(9, 2, false),
            row(14, 1, true),
            row(1, 12, false),
            row(crate::items::FREE, 0, true),
        ];
        let drawn = item_list(&menu, &item_lines(), rows, 5, 7);

        // Twelve frames, the rows 22 apart; the three under the cursor pulse.
        let frames = frames_of(&drawn);
        assert_eq!(frames.len(), 12);
        assert_eq!(frames[0], (83, 54, 1, 0));
        assert_eq!(
            frames[3..6],
            [(83, 76, 1, 7), (104, 76, 1, 7), (216, 76, 1, 7)]
        );
        assert_eq!(frames[11], (216, 120, 1, 0));

        // A row's content stands in its room: the frame less the piece's margin, and
        // the count ends 3 left of the room's right edge, 2 below its upper one.
        let clip = rect(83, 54, 21, 21);
        assert!(drawn.contains(&Drawn::ItemPicture {
            item: 9,
            at: (86, 59),
            dim: false,
            clip
        }));
        let clip = rect(104, 76, 112, 21);
        assert!(drawn.contains(&Drawn::ItemName {
            item: 14,
            at: (108, 80),
            dim: true,
            clip
        }));
        let clip = rect(216, 98, 23, 21);
        assert!(drawn.contains(&Drawn::Count {
            value: 12,
            ends: (236, 103),
            dim: false,
            clip
        }));
        // An empty place has its frames alone.
        let contents = (drawn.iter()).filter(|drawn| {
            matches!(
                drawn,
                Drawn::ItemPicture { .. } | Drawn::ItemName { .. } | Drawn::Count { .. }
            )
        });
        assert_eq!(contents.count(), 9);

        // Six lines under four rows, the first of them line 1: a sign on both sides,
        // drawn before the rows.
        let below = Drawn::Sign {
            turns: sign::BELOW,
            at: (160, 148),
            time: 7,
        };
        let above = Drawn::Sign {
            turns: sign::ABOVE,
            at: (160, 47),
            time: 7,
        };
        assert_eq!(drawn[..2], [below, above]);
        // At the list's end there is none below, at its start none above, and a list
        // of four lines has none at all.
        menu.kept_first[0] = 2;
        assert_eq!(item_list(&menu, &item_lines(), rows, 5, 7)[0], above);
        menu.kept_first[0] = 0;
        assert_eq!(item_list(&menu, &item_lines(), rows, 5, 7)[0], below);
        let short = item_list(&menu, &item_lines(), rows, 3, 7);
        assert!(
            !short
                .iter()
                .any(|drawn| matches!(drawn, Drawn::Sign { .. }))
        );
    }

    #[test]
    fn the_frames_of_the_list_go_by_the_screen() {
        let mut menu = PlayerMenu::open(&tables(), 0, 0);
        menu.kept[3] = 2;
        let rows = [ItemRow::default(); items::ROWS];
        let modes = |menu: &PlayerMenu| -> Vec<(u8, u32)> {
            (frames_of(&item_list(menu, &item_lines(), rows, 0, 9)).into_iter())
                .step_by(3)
                .map(|(_, _, mode, time)| (mode, time))
                .collect()
        };
        // Behind the player's entries every frame is plain.
        menu.screen = OF_PLAYER;
        assert_eq!(modes(&menu), [(0, 0); 4]);
        // On the list's own screen the cursor's row pulses.
        menu.screen = ITEMS;
        assert_eq!(modes(&menu), [(1, 0), (1, 0), (1, 9), (1, 0)]);
        // Behind the choice of who gets it that row has the marked colour.
        menu.screen = crate::player_menu::TARGET;
        assert_eq!(modes(&menu), [(0, 0), (0, 0), (2, 0), (0, 0)]);
    }

    #[test]
    fn a_gain_blinks_behind_the_bar_of_its_gauge() {
        let clip = rect(0, 0, 320, 240);
        let bar = |values: (i32, i32, i32), passes: u32| {
            gain_bar(1, values, (100, 50), passes, clip).map(|drawn| match drawn {
                Drawn::Bar {
                    picture,
                    length,
                    at,
                    ..
                } => (picture, length, at),
                other => panic!("{other:?}"),
            })
        };
        // Sixteen passes off and sixteen on, and nothing without a change.
        assert_eq!(bar((5, 100, 40), 0x0F), None);
        assert_eq!(bar((5, 100, 0), 0x10), None);
        // Behind the bar of the value, 5 of 100 on 47 pixels being 2, as long as the
        // change is on the gauge.
        assert_eq!(
            bar((5, 100, 40), 0x10),
            Some((bar::EXPERIENCE, 18, (110, 52)))
        );
        // A loss starts where the bar does.
        assert_eq!(
            bar((50, 100, -20), 0x1F),
            Some((bar::EXPERIENCE, 9, (108, 52)))
        );
        // A change past the most is as many pixels long as the value lacks.
        assert_eq!(
            bar((90, 100, 40), 0x10),
            Some((bar::EXPERIENCE, 10, (150, 52)))
        );
    }

    #[test]
    fn the_box_of_who_gets_an_item_is_the_head_of_his_box() {
        let record = player();
        let of_player = who_gets(None, &record, 0, 0x10);
        assert_eq!(
            of_player[0],
            Drawn::Frame {
                rect: rect(110, 167, 100, 41),
                mode: 0,
                time: 0
            }
        );
        let clip = SCREEN;
        assert_eq!(
            of_player[1],
            Drawn::SmallName {
                at: (117, 170),
                clip
            }
        );
        // The player has "Capture" in front of "Level" and his level further right.
        let sprite = |number: usize, at: (i16, i16)| Drawn::Sprite { number, at, clip };
        assert!(of_player.contains(&sprite(items::CAPTURE, (114, 195))));
        assert!(of_player.contains(&sprite(creature::LEVEL, (137, 194))));
        assert!(of_player.contains(&Drawn::Number {
            value: 3,
            ends: (176, 196),
            small: true,
            clip
        }));
        // Without a gain there is the gauge's own bar alone.
        let bars = |drawn: &[Drawn]| {
            (drawn.iter())
                .filter(|drawn| matches!(drawn, Drawn::Bar { .. }))
                .count()
        };
        assert_eq!(bars(&of_player), 1);

        // A carried creature has its name, the plain "Level" and the gain on its gauge.
        let of_creature = who_gets(Some(1), &record, 10, 0x10);
        assert_eq!(
            of_creature[1],
            Drawn::CreatureName {
                creature: 1,
                at: (117, 170),
                clip
            }
        );
        assert!(of_creature.contains(&sprite(creature::LEVEL, (126, 194))));
        assert!(!of_creature.contains(&sprite(items::CAPTURE, (114, 195))));
        assert_eq!(bars(&of_creature), 2);
        assert_eq!(bars(&who_gets(Some(1), &record, 10, 0)), 1);
    }

    #[test]
    fn an_item_is_described_while_the_circle_is_held_on_its_list() {
        let mut menu = PlayerMenu::open(&tables(), 0, 0);
        menu.screen = ITEMS;
        assert!(describes(&menu, 0, true));
        assert!(!describes(&menu, 0, false));
        assert!(!describes(&menu, crate::items::FREE, true));
        menu.screen = OF_PLAYER;
        assert!(!describes(&menu, 0, true));

        // The box of a message with four lines, 12 apart.
        let drawn = description(7);
        assert_eq!(
            drawn[0],
            Drawn::Frame {
                rect: message::BOX,
                mode: 0,
                time: 0
            }
        );
        assert_eq!(
            drawn[4],
            Drawn::Described {
                item: 7,
                line: 3,
                at: (14, 158 + 36)
            }
        );

        // The lines are four addresses an item; one without an address is empty.
        let base = items::DESCRIPTIONS - 0x20;
        let mut file = vec![0u8; 0x20 + 2 * 16];
        file[..2].copy_from_slice(&[0x82, 0x60]);
        file[0x20 + 16..0x20 + 20].copy_from_slice(&base.to_le_bytes());
        let lines = description_text(&file, base, 1).expect("the second item");
        assert_eq!(lines[0], [0x6082]);
        assert!(lines[1..].iter().all(Vec::is_empty));
        // An item past the file, or past the items, has none.
        assert_eq!(description_text(&file, base, 2), None);
        assert_eq!(description_text(&file, base, items::ITEMS), None);
    }

    #[test]
    fn the_colour_of_dim_digits_is_the_fourth_of_the_table() {
        let base = items::DIGIT_COLOURS;
        let table: Vec<u8> = (0..16).collect();
        assert_eq!(dim_digits(&table, base), Some([12, 13, 14]));
        assert_eq!(dim_digits(&table[..13], base), None);
    }

    #[test]
    fn the_list_of_the_items_has_a_line_of_help_of_its_own() {
        let mut menu = PlayerMenu::open(&tables(), 0, 0);
        menu.screen = ITEMS;
        assert_eq!(help(&menu), HELP_OF_ITEMS);
        menu.screen = crate::player_menu::TARGET;
        assert_eq!(help(&menu), HELP_PLAIN);
    }

    #[test]
    fn the_picker_of_a_target_is_a_name_for_each_who_may_get_the_item() {
        // A row is open exactly where the confirm would be taken.
        assert!(!picker_closes(0, 0) && !picker_closes(0, 1));
        assert!(picker_closes(0, 2) && picker_closes(0, 3) && picker_closes(0, 4));
        assert!(!picker_closes(2, 0) && !picker_closes(2, 2));
        assert!(picker_closes(2, 1) && picker_closes(2, 3) && picker_closes(2, 4));

        let mut menu = PlayerMenu::open(&tables(), 2, 0);
        menu.screen = crate::player_menu::TARGET;
        // The cursor on the first carried creature.
        menu.kept[5] = 1;
        let closed = [true, false, false, false];
        let drawn = target_panel(&menu, closed, None, 0, 9, 0x10);
        // The player's name between two small frames, then a row for each creature,
        // 13 apart; the row under the cursor pulses.
        assert_eq!(
            frames_of(&drawn),
            [
                (46, 152, 1, 0),
                (38, 152, 1, 0),
                (108, 152, 1, 0),
                (46, 166, 1, 9),
                (46, 179, 1, 0)
            ]
        );
        // A name stands about the middle of its frame's room, a closed row's dim.
        let names: Vec<_> = (drawn.iter())
            .filter_map(|drawn| match *drawn {
                Drawn::PickedName {
                    target,
                    middle,
                    dim,
                    ..
                } => Some((target, middle, dim)),
                _ => None,
            })
            .collect();
        assert_eq!(
            names,
            [
                (0, (76, 154), true),
                (1, (76, 168), false),
                (2, (76, 181), false)
            ]
        );

        // A carried creature under the cursor has the line of its mana under the
        // picker: a plain frame, the word, the gauge with what a use would give.
        let mut record = player();
        (record.mp, record.max_mp) = (10, 20);
        let drawn = target_panel(&menu, closed, Some(&record), 5, 9, 0x10);
        assert_eq!(
            drawn[..2],
            [
                Drawn::Frame {
                    rect: rect(210, 187, 100, 21),
                    mode: 0,
                    time: 0
                },
                Drawn::Sprite {
                    number: creature::MANA,
                    at: (217, 191),
                    clip: SCREEN
                }
            ]
        );
        // Ten of twenty on 47 pixels are 23, and five more are 11 behind them.
        assert!(drawn.contains(&Drawn::Bar {
            picture: bar::EXPERIENCE,
            length: 11,
            at: (242 + 8 + 23, 192),
            clip: SCREEN
        }));
    }

    fn story_lines() -> Vec<Vec<Piece>> {
        let piece = |flags: u16, kind: u16, x: i16, w: i16, margin: (i16, i16)| Piece {
            flags,
            kind,
            value: 0,
            rect: rect(x, 54, w, 21),
            margin,
        };
        let line = vec![
            piece(0x10, kind::PICTURE, 97, 21, (3, 4)),
            piece(0x10, kind::TEXT, 118, 107, (4, 4)),
        ];
        vec![line; story::ROWS]
    }

    #[test]
    fn the_list_of_the_things_has_four_rows_of_two_framed_pieces() {
        let mut menu = PlayerMenu::open(&tables(), 0, 0);
        menu.screen = STORY;
        // The cursor on the third row, the list one line down.
        (menu.kept[4], menu.kept_first[1]) = (2, 1);
        let row = |thing: i16, usable: bool| StoryRow { thing, usable };
        let rows = [
            row(1, false),
            row(3, false),
            row(26, true),
            row(STORY_THINGS, false),
        ];
        let drawn = story_list(&menu, &story_lines(), rows, 6, 7);

        // Eight frames, the rows 22 apart; the two under the cursor pulse.
        let frames = frames_of(&drawn);
        assert_eq!(frames.len(), 8);
        assert_eq!(frames[0], (97, 54, 1, 0));
        assert_eq!(frames[4..6], [(97, 98, 1, 7), (118, 98, 1, 7)]);
        assert_eq!(frames[7], (118, 120, 1, 0));

        // A row's content stands in its room, dim while the row is not open, and a
        // row past the last thing has its frames alone.
        assert!(drawn.contains(&Drawn::ThingPicture {
            thing: 1,
            at: (100, 58),
            dim: true,
            clip: rect(97, 54, 21, 21)
        }));
        assert!(drawn.contains(&Drawn::ThingName {
            thing: 26,
            at: (122, 102),
            dim: false,
            clip: rect(118, 98, 107, 21)
        }));
        let pictures = (drawn.iter()).filter(|drawn| matches!(drawn, Drawn::ThingPicture { .. }));
        assert_eq!(pictures.count(), 3);

        // The signs lie under the rows: one below while things are left, one above
        // while the list is not at its start.
        let sign = |turns: u16, at: (i16, i16)| Drawn::Sign { turns, at, time: 7 };
        assert_eq!(drawn[0], sign(sign::BELOW, (160, 148)));
        assert_eq!(drawn[1], sign(sign::ABOVE, (160, 47)));
        let last = story_list(&menu, &story_lines(), rows, 5, 7);
        assert_eq!(last[0], sign(sign::ABOVE, (160, 47)));
        assert!(matches!(last[1], Drawn::Frame { .. }));

        // Behind the entries and behind the question nothing pulses.
        for screen in [OF_PLAYER, crate::player_menu::LEAVE_FOREST] {
            menu.screen = screen;
            let drawn = story_list(&menu, &story_lines(), rows, 4, 7);
            assert!(frames_of(&drawn).iter().all(|frame| frame.3 == 0));
        }
    }

    #[test]
    fn the_text_of_a_thing_stands_in_the_box_of_a_message() {
        let mut menu = PlayerMenu::open(&tables(), 0, 0);
        menu.screen = STORY;
        assert!(thing_describes(&menu, 3, true));
        assert!(!thing_describes(&menu, 3, false));
        assert!(!thing_describes(&menu, STORY_THINGS, true));
        assert_eq!(help(&menu), HELP_OF_STORY);
        menu.screen = OF_PLAYER;
        assert!(!thing_describes(&menu, 3, true));

        let drawn = thing_description(3);
        assert_eq!(
            drawn[0],
            Drawn::Frame {
                rect: message::BOX,
                mode: 0,
                time: 0
            }
        );
        assert_eq!(
            drawn[2],
            Drawn::ThingDescribed {
                thing: 3,
                line: 1,
                at: (14, 170)
            }
        );
        assert_eq!(thing_text(&[0; 64], 0x800C_2090, STORY_THINGS), None);
        assert_eq!(thing_text(&[0; 64], 0x800C_2090, -1), None);
    }

    #[test]
    fn the_question_has_a_pulsing_frame_under_the_answer_of_its_cursor() {
        let mut menu = PlayerMenu::open(&tables(), 0, 0);
        menu.screen = crate::player_menu::LEAVE_FOREST;
        menu.cursor.at = 1;
        assert_eq!(help(&menu), HELP_PLAIN);
        let drawn = question(&menu, 9);
        let frame = |rect: Rect, mode: u8, time: u32| Drawn::Frame { rect, mode, time };
        assert_eq!(drawn[0], frame(rect(42, 178, 28, 14), 1, 9));
        assert_eq!(drawn[1], frame(message::BOX, 0, 0));
        assert_eq!(
            drawn[2..],
            [
                Drawn::Asked {
                    text: 0,
                    at: (14, 158)
                },
                Drawn::Asked {
                    text: 1,
                    at: (14, 182)
                }
            ]
        );
        menu.cursor.at = 0;
        assert_eq!(question(&menu, 9)[0], frame(rect(14, 178, 28, 14), 1, 9));
    }

    fn spell_lines() -> Vec<Vec<Piece>> {
        let piece = |kind: u16, x: i16, w: i16, margin: (i16, i16)| Piece {
            flags: 0x10,
            kind,
            value: 0,
            rect: rect(x, 54, w, 21),
            margin,
        };
        let line = vec![
            piece(kind::PICTURE, 96, 21, (4, 4)),
            piece(kind::TEXT, 117, 107, (3, 4)),
        ];
        vec![line; spells::ROWS]
    }

    #[test]
    fn the_spells_a_record_names_are_counted_and_found_in_their_order() {
        let bits = 1 << 3 | 1 << 11 | 1 << 29 | 1 << 31;
        assert_eq!(spells_known(bits), 3);
        assert_eq!([0, 1, 2].map(|n| spell_at(bits, n)), [3, 11, 29]);
        assert_eq!(spell_at(bits, 3), spells::SPELLS);
        assert_eq!(spell_at(bits, -1), spells::SPELLS);
        assert_eq!((spells_known(0), spell_at(0, 0)), (0, spells::SPELLS));
    }

    #[test]
    fn a_pointer_finds_the_rows_of_a_creatures_spells_and_body_attacks() {
        let line = |pieces: &[(i16, i16)]| -> Vec<Piece> {
            (pieces.iter())
                .map(|&(x, w)| Piece {
                    flags: 0,
                    kind: kind::TEXT,
                    value: 0,
                    rect: rect(x, 0, w, 21),
                    margin: (0, 4),
                })
                .collect()
        };
        let spells = vec![line(&[(0, 21), (0, 112)]); spells::ROWS];
        let attacks = vec![line(&[(0, 38), (0, 93), (0, 78)]); attacks::ROWS];
        let mut menu = PlayerMenu::open(&tables(), 1, 0);
        let moves = [&spells[..], &attacks[..]];
        menu.screen = MAGIC;
        let found = spots(&menu, [&[], &[]], (&[], &[]), moves);
        // Four rows of the spell's picture and its name, 22 apart from y 54.
        assert_eq!(found.len(), spells::ROWS);
        assert_eq!(found[1], ((0, 1), rect(96, 76, 133, 21)));
        menu.screen = SPECIAL;
        let found = spots(&menu, [&[], &[]], (&[], &[]), moves);
        assert_eq!(found.len(), attacks::ROWS);
        assert_eq!(found[4], ((0, 4), rect(56, 132, 209, 21)));
        // The caught creatures' screen has nothing to point at.
        menu.screen = CAUGHT;
        assert!(spots(&menu, [&[], &[]], (&[], &[]), moves).is_empty());
    }

    #[test]
    fn the_list_of_the_spells_has_four_rows_of_two_framed_pieces() {
        let mut menu = PlayerMenu::open(&tables(), 1, 0);
        menu.screen = MAGIC;
        // The cursor on the second row, the list one line down.
        (menu.kept[7], menu.kept_first[2]) = (1, 1);
        let mut pictures = [0; spells::SPELLS];
        (pictures[3], pictures[11]) = (34, 243);
        let rows = [3, 11, spells::SPELLS, spells::SPELLS];
        let drawn = spell_list(&menu, &spell_lines(), rows, 6, &pictures, 7);

        let frames = frames_of(&drawn);
        assert_eq!(frames.len(), 8);
        assert_eq!(frames[0], (96, 54, 1, 0));
        assert_eq!(frames[2..4], [(96, 76, 1, 7), (117, 76, 1, 7)]);
        assert_eq!(frames[7], (117, 120, 1, 0));

        // A row is the spell's sprite and its name, each in its room and never dim;
        // a row past the last spell has its frames alone.
        assert!(drawn.contains(&Drawn::Listed {
            what: Listed::Sprite(243),
            room: rect(100, 80, 13, 13),
            flags: 0x10,
            dim: false,
            clip: rect(96, 76, 21, 21)
        }));
        assert!(drawn.contains(&Drawn::Listed {
            what: Listed::Spell(3),
            room: rect(120, 58, 101, 13),
            flags: 0x10,
            dim: false,
            clip: rect(117, 54, 107, 21)
        }));
        let listed = (drawn.iter()).filter(|drawn| matches!(drawn, Drawn::Listed { .. }));
        assert_eq!(listed.count(), 4);

        // The signs as on the other lists, by the spells that are known.
        let sign = |turns: u16, at: (i16, i16)| Drawn::Sign { turns, at, time: 7 };
        assert_eq!(drawn[0], sign(sign::BELOW, (160, 148)));
        assert_eq!(drawn[1], sign(sign::ABOVE, (160, 47)));
        let last = spell_list(&menu, &spell_lines(), rows, 5, &pictures, 7);
        assert_eq!(last[0], sign(sign::ABOVE, (160, 47)));
        assert!(matches!(last[1], Drawn::Frame { .. }));

        // Behind the creature's entries nothing pulses.
        menu.screen = OF_CREATURE;
        let drawn = spell_list(&menu, &spell_lines(), rows, 2, &pictures, 7);
        assert!(frames_of(&drawn).iter().all(|frame| frame.3 == 0));

        // The text of a spell: on the list's screen, on the held circle.
        assert!(!spell_describes(&menu, 3, true));
        menu.screen = MAGIC;
        assert!(spell_describes(&menu, 3, true) && !spell_describes(&menu, 3, false));
        assert!(!spell_describes(&menu, spells::SPELLS, true));
        let said = list_description(Listed::Spell(3));
        assert!(matches!(said[0], Drawn::Frame { rect, mode: 0, time: 0 } if rect == message::BOX));
        assert_eq!(
            said[2],
            Drawn::ListDescribed {
                what: Listed::Spell(3),
                line: 1,
                at: (14, 170)
            }
        );
        assert_eq!(help(&menu), HELP_OF_LISTS);
    }

    #[test]
    fn the_list_of_the_spells_moves_to_a_place_and_dims_what_costs_too_much() {
        // As the menu of a fight calls it: at (96, 69), the cursor on the first row,
        // the list one line down, and the first row's spell too dear.
        let mut pictures = [0; spells::SPELLS];
        pictures[3] = 34;
        let none = spells::SPELLS;
        let shown = SpellRows {
            rows: [3, none, none, none],
            short: [true, false, false, false],
            known: 6,
        };
        let drawn = spell_rows(0x02, (0, 1), (96, 69), &spell_lines(), shown, &pictures, 7);
        let frames = frames_of(&drawn);
        assert_eq!(frames.len(), 8);
        assert_eq!(frames[0], (96, 69, 1, 7));
        assert_eq!(frames[7], (117, 135, 1, 0));
        assert!(drawn.contains(&Drawn::Listed {
            what: Listed::Spell(3),
            room: rect(120, 73, 101, 13),
            flags: 0x10,
            dim: true,
            clip: rect(117, 69, 107, 21)
        }));
        // The scroll signs go with the list, 15 lower.
        for (turns, at) in [(sign::ABOVE, (160, 62)), (sign::BELOW, (160, 163))] {
            assert!(drawn.contains(&Drawn::Sign { turns, at, time: 7 }));
        }
    }

    #[test]
    fn the_list_of_the_body_attacks_has_five_rows_of_three_framed_pieces() {
        let piece = |kind: u16, x: i16, w: i16| Piece {
            flags: 0x08,
            kind,
            value: 0,
            rect: rect(x, 44, w, 21),
            margin: (0, 4),
        };
        let line = vec![
            piece(kind::PICTURE, 56, 38),
            piece(kind::TEXT, 94, 93),
            piece(kind::TEXT, 187, 78),
        ];
        let lines = vec![line; attacks::ROWS];
        let mut menu = PlayerMenu::open(&tables(), 1, 0);
        menu.screen = SPECIAL;
        menu.kept[10] = 3;
        let pictures: [[u8; 5]; attacks::ROWS] =
            std::array::from_fn(|row| std::array::from_fn(|place| (10 * row + place) as u8));
        // Wind is the element of class 1, whose sprite is the third of its row.
        assert_eq!(attack_picture(&pictures, 2, 1), 22);
        assert_eq!(
            [0, 2, 3].map(|class| attack_picture(&pictures, 0, class)),
            [0, 3, 1]
        );
        assert_eq!(attack_picture(&pictures, 4, 10), 44);
        let mut rows = [AttackRow::default(); attacks::ROWS];
        rows[1] = AttackRow {
            known: true,
            attack: 5,
            grade: 3,
            class: 1,
            refused: false,
        };
        // An entry that is not known keeps its attack for the sprite alone.
        rows[3] = AttackRow {
            known: false,
            attack: 6,
            grade: 2,
            class: 4,
            refused: false,
        };
        let drawn = attack_list(&menu, &lines, rows, &pictures, 9);

        let frames = frames_of(&drawn);
        assert_eq!(frames.len(), 15);
        assert_eq!(frames[0], (56, 44, 1, 0));
        assert_eq!(
            frames[9..12],
            [(56, 110, 1, 9), (94, 110, 1, 9), (187, 110, 1, 9)]
        );
        let listed = |what: Listed, x: i16, w: i16, y: i16, dim: bool| Drawn::Listed {
            what,
            room: rect(x, y + 4, w, 13),
            flags: 0x08,
            dim,
            clip: rect(x, y, w, 21),
        };
        assert!(drawn.contains(&listed(Listed::Sprite(12), 56, 38, 66, false)));
        assert!(drawn.contains(&listed(Listed::Attack(5), 94, 93, 66, false)));
        assert!(drawn.contains(&listed(Listed::Grade(3), 187, 78, 66, false)));
        assert!(drawn.contains(&listed(Listed::Sprite(34), 56, 38, 110, true)));
        assert!(drawn.contains(&listed(Listed::Attack(0), 94, 93, 110, true)));
        assert!(drawn.contains(&listed(Listed::Grade(0), 187, 78, 110, true)));

        // Behind the entries the frames are plain.
        menu.screen = OF_CREATURE;
        let behind = attack_list(&menu, &lines, rows, &pictures, 9);
        assert!(
            frames_of(&behind)
                .iter()
                .all(|frame| frame.2 == 0 && frame.3 == 0)
        );

        // The text of an attack: a known entry whose attack is not 0.
        assert!(!attack_describes(&menu, rows[1], true));
        menu.screen = SPECIAL;
        // In a fight a known entry that costs too much is dim with its own names.
        rows[1].refused = true;
        let dear = attack_rows(0x02, 1, &lines, rows, &pictures, 9);
        assert!(dear.contains(&listed(Listed::Attack(5), 94, 93, 66, true)));
        assert!(dear.contains(&listed(Listed::Grade(3), 187, 78, 66, true)));
        assert_eq!(frames_of(&dear)[3], (56, 66, 1, 9));
        rows[1].refused = false;

        assert!(attack_describes(&menu, rows[1], true));
        assert!(!attack_describes(&menu, rows[1], false));
        assert!(!attack_describes(&menu, rows[3], true));
        let plain = AttackRow {
            known: true,
            ..AttackRow::default()
        };
        assert!(!attack_describes(&menu, plain, true));
    }

    #[test]
    fn the_names_and_texts_of_the_lists_are_read_through_their_tables() {
        // A file at the menu's base: the table of the spells' texts with spell 1's
        // second line, and the text itself.
        let base = 0x800C_2000;
        let mut file = vec![0; 0x800];
        let text_at = 0x700usize;
        file[text_at..text_at + 4].copy_from_slice(&[0x82, 0x60, 0x82, 0x61]);
        let entry = (spells::TEXTS - base) as usize + 16 + 4;
        file[entry..entry + 4].copy_from_slice(&(base + text_at as u32).to_le_bytes());
        let lines = spell_text(&file, base, 1).expect("the lines");
        assert!(lines[0].is_empty() && lines[2].is_empty());
        assert_eq!(lines[1], crate::text::units(&file[text_at..]));
        assert_eq!(lines[1].len(), 2);
        assert_eq!(spell_text(&file, base, spells::SPELLS), None);
        assert_eq!(attack_text(&file, base, attacks::ATTACKS), None);
        // A table that leaves the file is no text.
        assert_eq!(spell_text(&file[..0x300], base, 1), None);
        assert_eq!(spell_pictures(&file, base), None);
        assert_eq!(attack_pictures(&file, base), None);
        assert_eq!(spell_name(&file, base, 0), None);
    }

    #[test]
    fn the_panel_beside_equip_has_what_he_has_on_and_the_five_numbers() {
        let tables = tables();
        let mut menu = PlayerMenu::open(&tables, 0, 0);
        let piece = |flags: u16, kind: u16, w: i16, margin: (i16, i16)| Piece {
            flags,
            kind,
            value: 0,
            rect: rect(0, 0, w, 21),
            margin,
        };
        let line = |margin: i16| {
            vec![
                piece(piece::CENTRED, kind::PICTURE, 50, (margin, 5)),
                piece(0x10, kind::PICTURE, 21, (3, 5)),
                piece(0x10, kind::TEXT, 112, (4, 4)),
            ]
        };
        let lines = [line(10), line(11), line(5)];
        let shown = Detail {
            values: [1, 2, 3, 4, 5],
            worn: [true, true, false],
            money: 0,
        };
        // The three lines come first: nine frames, three words, and a picture and a
        // name for each place with a thing on.
        let rows = 9 + 3 + 2 * 2;
        // While the row of entries opens, the box of the numbers is cut to as far as it
        // stands open, and the three lines are whole.
        menu.openness = 4;
        let half = equip_panel(&menu, &shown, &lines, |_| 30);
        assert_eq!(half.len(), rows + 1 + 5 * 2);
        let frames = |drawn: &[Drawn]| -> Vec<Rect> {
            (drawn.iter())
                .filter_map(|drawn| match *drawn {
                    Drawn::Frame { rect, mode, time } => {
                        // Plain, and no line has the cursor.
                        assert_eq!((mode, time), (0, 0));
                        Some(rect)
                    }
                    _ => None,
                })
                .collect()
        };
        assert_eq!(
            frames(&half[..rows]),
            [
                rect(10, 61, 50, 21),
                rect(60, 61, 21, 21),
                rect(81, 61, 112, 21),
                rect(10, 83, 50, 21),
                rect(60, 83, 21, 21),
                rect(81, 83, 112, 21),
                rect(10, 105, 50, 21),
                rect(60, 105, 21, 21),
                rect(81, 105, 112, 21),
            ]
        );
        let worn: Vec<usize> = (half.iter())
            .filter_map(|drawn| match *drawn {
                Drawn::WornName { place, .. } => Some(place),
                _ => None,
            })
            .collect();
        assert_eq!(worn, [0, 1]);
        let Drawn::Frame {
            rect: cut, mode, ..
        } = half[rows]
        else {
            panic!("the frame of the numbers comes behind the lines");
        };
        assert!(cut.w < EQUIP_BOX.w && cut.h < EQUIP_BOX.h && mode == 0);

        menu.openness = OPEN;
        let whole = equip_panel(&menu, &shown, &lines, |_| 30);
        let whole = &whole[rows..];
        assert_eq!(
            whole[0],
            Drawn::Frame {
                rect: EQUIP_BOX,
                mode: 0,
                time: 0,
            }
        );
        // The first row: "Attack" and its number; the last: "Speed".
        assert_eq!(
            whole[1..3],
            [
                Drawn::Sprite {
                    number: 98,
                    at: (200, 53),
                    clip: EQUIP_BOX,
                },
                Drawn::Number {
                    value: 1,
                    ends: (263, 54),
                    small: false,
                    clip: EQUIP_BOX,
                },
            ]
        );
        assert_eq!(
            whole[10],
            Drawn::Number {
                value: 5,
                ends: (263, 110),
                small: false,
                clip: EQUIP_BOX,
            }
        );
    }

    #[test]
    fn the_caught_creatures_stand_three_across_from_the_kept_row() {
        let tables = tables();
        let record = player();
        // Ten wait, in the first ten places; the twelfth place holds one more that the
        // count does not cover, the eleventh is empty.
        let mut waiting = vec![Some(record.clone()); 10];
        waiting.extend([None, Some(record.clone())]);
        let mut menu = PlayerMenu::open(&tables, 0, 0);
        assert!(caught_grid(&menu, &waiting, 0).is_empty());

        menu.caught = 10;
        let boxes = |drawn: &[Drawn]| -> Vec<(usize, (i16, i16))> {
            (drawn.iter())
                .filter_map(|drawn| match *drawn {
                    Drawn::CaughtName { caught, at, .. } => Some((caught, at)),
                    _ => None,
                })
                .collect()
        };
        let signs = |drawn: &[Drawn]| -> Vec<(u16, (i16, i16))> {
            (drawn.iter())
                .filter_map(|drawn| match *drawn {
                    Drawn::Sign { turns, at, .. } => Some((turns, at)),
                    _ => None,
                })
                .collect()
        };
        // From the first row: nine boxes, the last added first, and the sign below.
        let drawn = caught_grid(&menu, &waiting, 5);
        let names = boxes(&drawn);
        assert_eq!(names.len(), 9);
        assert_eq!(names[8], (0, (17, 62)));
        assert_eq!(names[7], (1, (117, 62)));
        assert_eq!(names[5], (3, (17, 103)));
        assert_eq!(names[0], (8, (217, 144)));
        assert_eq!(signs(&drawn), [(sign::BELOW, (160, 196))]);
        assert!(drawn.contains(&Drawn::Frame {
            rect: rect(210, 141, 100, 41),
            mode: 0,
            time: 0,
        }));
        // A box is the head of a creature's box: frame, name, the line of the HP (five
        // things), the four signs, "Level" and its number.
        assert_eq!(drawn.len(), 1 + 9 * 13);

        // From the second row: the places 3 to 11, of which one is empty, and the sign
        // above alone.
        menu.kept[9] = 1;
        let drawn = caught_grid(&menu, &waiting, 5);
        let names = boxes(&drawn);
        assert_eq!(names.len(), 8);
        assert_eq!(names[7], (3, (17, 62)));
        assert_eq!(names[0], (11, (217, 144)));
        assert_eq!(signs(&drawn), [(sign::ABOVE, (160, 45))]);
        // Nine or fewer never scroll.
        menu.caught = 9;
        menu.kept[9] = 0;
        assert!(signs(&caught_grid(&menu, &waiting, 5)).is_empty());
    }

    #[test]
    fn a_caught_creature_is_its_own_family_and_a_merge_moves_the_parents_back() {
        let (a, b) = ([0x8260u16, 0x8261], [0x8262u16]);
        let first = Family::of_a_caught(&a, 1);
        let second = Family::of_a_caught(&b, 8);
        assert!(first.names.iter().all(|name| name == &a));
        assert_eq!(first.signs, [1; 6]);

        // The child of the two: its parents, then the parents of each, which a caught
        // creature is itself.
        let child = first.merged((&a, 1), &second, (&b, 8));
        assert_eq!(child.signs, [1, 8, 1, 1, 8, 8]);
        assert_eq!(child.names[1], b);
        assert_eq!(child.names[4], b);

        // Merged once more under another name with the second: the first two of each
        // family go back a row, the last four of each are forgotten.
        let c = [0x8263u16];
        let grandchild = child.merged((&c, 3), &second, (&b, 8));
        assert_eq!(grandchild.signs, [3, 8, 1, 8, 8, 8]);
        assert_eq!(grandchild.names[0], c);
        assert_eq!(grandchild.names[2], a);
        assert_eq!(grandchild.names[3], b);
    }

    #[test]
    fn the_family_is_a_tree_of_seven_boxes_over_twelve_lines() {
        let mut record = player();
        record.strengths = [32, 7, 8, 0];
        assert_eq!(strength_signs(record.strengths), 0b0101);
        assert_eq!(strength_signs([8; 4]), 0b1111);
        let family = Family {
            names: Default::default(),
            signs: [2, 0, 1, 1, 8, 8],
        };
        let drawn = family_panel(1, &record, &family);
        // Twelve lines underneath, then six things a box.
        assert_eq!(drawn.len(), 12 + 7 * 6);
        assert!(
            drawn[..12]
                .iter()
                .all(|drawn| matches!(drawn, Drawn::Line { grey: 128, .. }))
        );
        // The line added last lies lowest, the one from the creature's box on top.
        assert_eq!(
            drawn[11],
            Drawn::Line {
                from: (104, 114),
                to: (110, 114),
                grey: 128,
                clip: SCREEN,
            }
        );
        // Every line runs along a row or a column of the screen.
        assert!(
            family::LINES
                .iter()
                .all(|(from, to)| from.0 == to.0 || from.1 == to.1)
        );

        // The creature's own box is added first and so painted last: its frame, the
        // signs of its strengths and its own name.
        let own = &drawn[12 + 6 * 6..];
        assert_eq!(
            own[0],
            Drawn::Frame {
                rect: rect(40, 107, 64, 15),
                mode: 0,
                time: 0,
            }
        );
        let sprite = |number: usize, at: (i16, i16)| Drawn::Sprite {
            number,
            at,
            clip: SCREEN,
        };
        assert_eq!(own[1], sprite(creature::HAS[0], (44, 123)));
        assert_eq!(own[2], sprite(creature::NONE, (58, 123)));
        assert_eq!(own[3], sprite(creature::HAS[2], (72, 123)));
        assert_eq!(own[4], sprite(creature::NONE, (86, 123)));
        assert_eq!(
            own[5],
            Drawn::CreatureName {
                creature: 1,
                at: (43, 111),
                clip: SCREEN,
            }
        );

        // The first parent stands right of it, higher, with the bits its record keeps.
        let parent = &drawn[12 + 5 * 6..12 + 6 * 6];
        assert_eq!(
            parent[0],
            Drawn::Frame {
                rect: rect(117, 77, 64, 15),
                mode: 0,
                time: 0,
            }
        );
        assert_eq!(parent[2], sprite(creature::HAS[1], (135, 93)));
        assert_eq!(
            parent[5],
            Drawn::AncestorName {
                creature: 1,
                ancestor: 0,
                at: (120, 81),
            }
        );
        // An ancestor without an element has four dashes; the last box is the first
        // painted.
        let none = &drawn[12 + 4 * 6..12 + 5 * 6];
        assert!((none[1..5].iter()).all(|drawn| matches!(
            drawn,
            Drawn::Sprite {
                number: creature::NONE,
                ..
            }
        )));
        assert_eq!(
            drawn[12],
            Drawn::Frame {
                rect: rect(194, 152, 64, 15),
                mode: 0,
                time: 0,
            }
        );
        // A line ends where a box begins or a pixel in front of it.
        let (own_box, parent_box) = (family::PLACES[0], family::PLACES[1]);
        assert_eq!(
            family::LINES[0].0,
            (own_box.0 + family::BOX.0, own_box.1 + 7)
        );
        assert_eq!(family::LINES[1].1, (parent_box.0, parent_box.1 + 7));
    }
}
