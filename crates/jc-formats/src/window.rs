
use crate::menu::Rect;
use crate::pad::{self, Pad};
use crate::rooms::Image;
use crate::vram::Vram;

/// Width and height of the text's box.
pub const TEXT_BOX: (i16, i16) = (219, 56);
/// Width and height of the picture's box.
pub const PICTURE_BOX: (i16, i16) = (70, 67);
/// The face is a quad of this many pixels, wide and high.
pub const FACE: i16 = 64;
/// The passes a box takes to open or to close.
pub const OPENING: i32 = 8;
/// The image of the sign of "more" in the fonts' file.
pub const MORE_IMAGE: usize = 60;
/// The first image of the frame in the fonts' file; see [`Part`].
pub const FRAME_IMAGE: usize = 79;
/// The table of the fill's colours in the executable, by the window colour.
const FILLS: u32 = 0x8008_6918;
/// The window colours the table has.
pub const COLOURS: usize = 5;

/// Where on the screen a window is: word 0 of the opener's record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Where {
    Top,
    Middle,
    Bottom,
}

/// The rectangles and places the opener sets for a window without a place of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    /// The text's box.
    pub text: Rect,
    /// The picture's box, which is drawn with or without a picture.
    pub picture: Rect,
    /// The upper left corner of the face.
    pub face: (i16, i16),
    /// Where the first line of the text starts.
    pub first_line: (i16, i16),
}

impl Layout {
    /// The window at one of the three places, with the picture's box on the left or on
    /// the right of the text.
    pub fn of(place: Where, picture_left: bool) -> Self {
        // The opener sets the picture's box, puts the text's box 13 below its upper
        // edge and moves the text's box up by 2 at the end.
        let (picture, text_x) = match (picture_left, place) {
            (true, Where::Top) => ((16, 16), 84),
            (true, Where::Middle) => ((16, 116), 84),
            (true, Where::Bottom) => ((16, 161), 84),
            (false, Where::Top) => ((234, 16), 16),
            (false, Where::Middle) => ((234, 96), 16),
            (false, Where::Bottom) => ((234, 161), 16),
        };
        let text_y = picture.1 + 13 - 2;
        Self {
            text: Rect {
                x: text_x,
                y: text_y,
                w: TEXT_BOX.0,
                h: TEXT_BOX.1,
            },
            picture: Rect {
                x: picture.0,
                y: picture.1,
                w: PICTURE_BOX.0,
                h: PICTURE_BOX.1,
            },
            face: (picture.0 + 3, picture.1 + 2),
            first_line: (text_x + 7, text_y + 3),
        }
    }

    /// The rectangle the text is cut off at (`0x80035888`), for the text's box as it
    /// stands or as it is while it closes.
    pub fn clip(text: Rect) -> Rect {
        Rect {
            x: text.x + 6,
            y: text.y + 2,
            w: text.w - 6,
            h: text.h - 3,
        }
    }
}

/// How a rectangle opens (`0x8003F208`): which part of it the share is taken of and what
/// stays in place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Way {
    /// Width and height about the middle: the boxes of a message window.
    Middle = 0,
    FromUpperLeft = 1,
    FromLowerLeft = 2,
    FromUpperRight = 3,
    FromLowerRight = 4,
    /// The height from the upper edge: the answers' box.
    Down = 5,
    Up = 6,
    FromLeft = 7,
    FromRight = 8,
    /// The height about the middle.
    MiddleHigh = 9,
    /// The width about the middle.
    MiddleWide = 10,
}

/// A value halved as the routine halves it: towards zero.
fn half(value: i16) -> i16 {
    value / 2
}

/// The rectangle at `count` of `of` passes of its opening. A count outside 0 to `of` is
/// the whole rectangle.
pub fn opened(whole: Rect, count: i32, of: i32, way: Way) -> Rect {
    let count = if count < 0 || count > of { of } else { count };
    let share = |length: i16| (i32::from(length) * count / of.max(1)) as i16;
    let (w, h) = (share(whole.w), share(whole.h));
    let (right, low) = (whole.x + whole.w, whole.y + whole.h);
    let middle_x = whole.x + half(whole.w) - half(w);
    let middle_y = whole.y + half(whole.h) - half(h);
    let (x, y, w, h) = match way {
        Way::Middle => (middle_x, middle_y, w, h),
        Way::FromUpperLeft => (whole.x, whole.y, w, h),
        Way::FromLowerLeft => (whole.x, low - h, w, h),
        Way::FromUpperRight => (right - w, whole.y, w, h),
        Way::FromLowerRight => (right - w, low - h, w, h),
        Way::Down => (whole.x, whole.y, whole.w, h),
        Way::Up => (whole.x, low - h, whole.w, h),
        Way::FromLeft => (whole.x, whole.y, w, whole.h),
        Way::FromRight => (right - w, whole.y, w, whole.h),
        Way::MiddleHigh => (whole.x, middle_y, whole.w, h),
        Way::MiddleWide => (middle_x, whole.y, w, whole.h),
    };
    Rect { x, y, w, h }
}

/// The eight pictures of a frame, in the order of their images from [`FRAME_IMAGE`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    UpperLeft = 0,
    LowerLeft = 1,
    LowerRight = 2,
    UpperRight = 3,
    LowerEdge = 4,
    LeftEdge = 5,
    RightEdge = 6,
    UpperEdge = 7,
}

/// The sizes of the eight images of a frame, by [`Part`].
pub type Sizes = [(i16, i16); 8];

/// One picture of a frame on the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece {
    pub part: Part,
    /// Where it is drawn and how large.
    pub on_screen: Rect,
    /// The size of its image. A corner is drawn as it is; an edge is a quad whose
    /// texture corners are the image's far edges less one.
    pub image: (i16, i16),
    pub stretched: bool,
}

impl Piece {
    /// The image number in the fonts' file.
    pub fn image_number(&self) -> usize {
        FRAME_IMAGE + self.part as usize
    }

    /// The texel of the image for a pixel of the piece, counted from its upper left
    /// corner. HYPOTHESIS for an edge: the texture coordinate grows evenly from 0 to the
    /// size less one over the piece's length and is rounded down, as the chip is publicly
    /// described.
    pub fn texel(&self, dx: i16, dy: i16) -> (i16, i16) {
        if !self.stretched {
            return (dx, dy);
        }
        let along = |d: i16, length: i16, size: i16| match length > 0 {
            true => (i32::from(d) * i32::from(size - 1) / i32::from(length)) as i16,
            false => 0,
        };
        (
            along(dx, self.on_screen.w, self.image.0),
            along(dy, self.on_screen.h, self.image.1),
        )
    }
}

/// Whether a rectangle is large enough for a frame and a fill (`0x80040244`): as wide as
/// the two upper corners and as high as the two.
pub fn is_drawn(rect: Rect, sizes: &Sizes) -> bool {
    let (left, right) = (
        sizes[Part::UpperLeft as usize],
        sizes[Part::UpperRight as usize],
    );
    left.0 + right.0 <= rect.w && left.1 + right.1 <= rect.h
}

/// The frame of a rectangle (`0x800403D4`), in the order the pieces are drawn: the last
/// one added first. `None` for a rectangle that is too small.
pub fn frame(rect: Rect, sizes: &Sizes) -> Option<[Piece; 8]> {
    if !is_drawn(rect, sizes) {
        return None;
    }
    let size = |part: Part| sizes[part as usize];
    let (upper_left, upper_right) = (size(Part::UpperLeft), size(Part::UpperRight));
    let lower_left = size(Part::LowerLeft);
    let wide = rect.w - (upper_left.0 + upper_right.0);
    let high = rect.h - (upper_left.1 + lower_left.1);
    let low = rect.y + upper_left.1 + high;
    let corner = |part: Part, x: i16, y: i16| Piece {
        part,
        on_screen: Rect {
            x,
            y,
            w: size(part).0,
            h: size(part).1,
        },
        image: size(part),
        stretched: false,
    };
    let edge = |part: Part, x: i16, y: i16, w: i16, h: i16| Piece {
        part,
        on_screen: Rect { x, y, w, h },
        image: size(part),
        stretched: true,
    };
    let (left_edge, right_edge) = (size(Part::LeftEdge), size(Part::RightEdge));
    let (lower_edge, upper_edge) = (size(Part::LowerEdge), size(Part::UpperEdge));
    Some([
        edge(
            Part::RightEdge,
            rect.x + rect.w - right_edge.0,
            rect.y + upper_right.1,
            right_edge.0 - 1,
            high,
        ),
        edge(
            Part::LeftEdge,
            rect.x,
            rect.y + upper_left.1,
            left_edge.0 - 1,
            high,
        ),
        edge(
            Part::LowerEdge,
            rect.x + lower_left.0,
            rect.y + rect.h - lower_edge.1,
            wide,
            lower_edge.1,
        ),
        edge(
            Part::UpperEdge,
            rect.x + upper_left.0,
            rect.y,
            wide,
            upper_edge.1,
        ),
        corner(Part::LowerRight, rect.x + lower_left.0 + wide, low),
        corner(Part::LowerLeft, rect.x, low),
        corner(Part::UpperRight, rect.x + upper_left.0 + wide, rect.y),
        corner(Part::UpperLeft, rect.x, rect.y),
    ])
}

/// The rectangle of the fill of a box (`0x80040294`), which is drawn over the frame, half
/// of what is under it and half its colour.
pub fn fill(rect: Rect) -> Rect {
    Rect {
        x: rect.x + 3,
        y: rect.y + 2,
        w: rect.w - 6,
        h: rect.h - 4,
    }
}

/// The colours of the fill, by the window colour of the game's settings.
pub fn fill_colours(image: &[u8], base: u32) -> Option<[[u8; 3]; COLOURS]> {
    let image = Image { data: image, base };
    let mut colours = [[0; 3]; COLOURS];
    for (kind, colour) in colours.iter_mut().enumerate() {
        let address = image.u32(FILLS + 4 * kind as u32).ok()?;
        let word = image.u32(address).ok()?.to_le_bytes();
        *colour = [word[0], word[1], word[2]];
    }
    Some(colours)
}

/// The place of the player's face and of its palette, in the executable.
const PLAYER_FACE: u32 = 0x8007_DBC8;
/// The places of the faces of the field's model slots, and of their palettes.
const SLOT_FACES: u32 = 0x8007_DBD0;
const SLOT_PALETTES: u32 = 0x8007_DBE0;
/// The model slots of the field.
pub const FACE_SLOTS: usize = 4;

/// Where a face lies in video memory: a picture of [`FACE`] pixels in 256 colours that
/// came with the textures of its model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FacePlace {
    pub picture: (i16, i16),
    pub palette: (i16, i16),
}

impl FacePlace {
    /// The face as it lies in a video memory: [`FACE`] by [`FACE`] RGBA pixels, top row
    /// first. Two texels to a word, the low byte first; a texel whose colour is zero is
    /// not drawn and has alpha 0.
    pub fn rgba(&self, vram: &Vram) -> Vec<u8> {
        let side = FACE as usize;
        let at = |place: (i16, i16)| (place.0.max(0) as usize, place.1.max(0) as usize);
        let (picture, palette) = (at(self.picture), at(self.palette));
        let mut out = Vec::with_capacity(side * side * 4);
        for (y, x) in (0..side).flat_map(|y| (0..side).map(move |x| (y, x))) {
            let index = vram.word(picture.0 + x / 2, picture.1 + y) >> (8 * (x % 2)) & 0xFF;
            let colour = vram.word(palette.0 + index as usize, palette.1);
            out.extend_from_slice(&crate::tim::color_to_rgba(colour));
        }
        out
    }
}

/// The places the window's face routine (`0x800A6298` of file 27) knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Faces {
    /// Number 0: the player's.
    pub player: FacePlace,
    /// The face of the model in each slot of the field's loader.
    pub slots: [FacePlace; FACE_SLOTS],
}

impl Faces {
    pub fn from_executable(image: &[u8], base: u32) -> Option<Self> {
        let image = Image { data: image, base };
        let pair = |address: u32| -> Option<(i16, i16)> {
            Some((image.i16(address).ok()?, image.i16(address + 2).ok()?))
        };
        let mut slots = [FacePlace {
            picture: (0, 0),
            palette: (0, 0),
        }; FACE_SLOTS];
        for (slot, place) in slots.iter_mut().enumerate() {
            place.picture = pair(SLOT_FACES + 4 * slot as u32)?;
            place.palette = pair(SLOT_PALETTES + 4 * slot as u32)?;
        }
        Some(Self {
            player: FacePlace {
                picture: pair(PLAYER_FACE)?,
                palette: pair(PLAYER_FACE + 4)?,
            },
            slots,
        })
    }

    /// Whether a picture at this place of video memory is a face as a model file holds
    /// it: the player's, or the one of slot 0, where every other model's textures are
    /// stored for.
    pub fn is_face(&self, picture: (i16, i16)) -> bool {
        picture == self.player.picture || picture == self.slots[0].picture
    }
}

/// Where the sign of "more" stands for a text that starts at `first_line` (`0x8003D000`).
pub fn more_at(first_line: (i16, i16)) -> (i16, i16) {
    (first_line.0 + 195, first_line.1 + 35)
}

/// The brightness of the sign of "more" in a pass (`0x8003BD20`); 128 is the picture as
/// it is.
pub fn more_brightness(pass: u32) -> u8 {
    ((pass & 15) << 4) as u8
}

/// The sound of a turned page, of a moved cursor and of a picked answer.
pub const TURN_SOUND: u8 = 0x7F;
/// The sound of a cancel among the answers.
pub const CANCEL_SOUND: u8 = 0x7D;
/// The cues a page can have.
pub const CUES: usize = 48;
/// The height of the speaker's name box and of the bar under the answers' cursor.
pub const NAME_HIGH: i16 = 18;
pub const BAR_HIGH: i16 = 13;
/// The passes of one pulse of the bar, there and back.
const PULSE: i16 = 40;

impl Layout {
    /// The box of the speaker's name for a name of this width (the opener and
    /// `0x80035A74`). The name's first cell is 7 right of and 2 below its corner.
    pub fn name_box(place: Where, picture_left: bool, width: i16) -> Rect {
        let layout = Self::of(place, picture_left);
        // The right end of the name's row: the text's box, or under the picture's box
        // for a window at the top with the picture on the right.
        let right = |of: Rect| of.x + of.w - width - 16;
        let (x, y) = match (picture_left, place) {
            (false, Where::Top) => (
                right(layout.picture),
                layout.picture.y + layout.picture.h - 4,
            ),
            (false, _) => (layout.text.x, layout.text.y - 16),
            (true, Where::Top) => (right(layout.text), layout.text.y + layout.text.h - 2),
            (true, _) => (right(layout.text), layout.text.y - 16),
        };
        Rect {
            x,
            y,
            w: width + 17,
            h: NAME_HIGH,
        }
    }

    /// The answers' box for this many answers that can be picked, the widest of them
    /// this wide.
    pub fn answers_box(&self, widest: i16, lines: i16) -> Rect {
        let left = self.text.x.min(self.picture.x);
        Rect {
            x: left + 10,
            y: self.text.y + self.text.h + 5,
            w: widest + 17,
            h: crate::message::LINE * lines + 7,
        }
    }
}

/// Where the first cell of answer `line` stands in the answers' box (`0x80035668`).
pub fn answer_at(answers: Rect, line: i16) -> (i16, i16) {
    (answers.x + 8, answers.y + 2 + crate::message::LINE * line)
}

/// The bar under the cursor on answer `line`, for the box as it stands in the pass.
pub fn bar(answers: Rect, line: i16) -> Rect {
    Rect {
        x: answers.x + 4,
        y: answers.y + crate::message::LINE * line + 3,
        w: answers.w - 8,
        h: BAR_HIGH,
    }
}

/// The slot of the window's table the bar is added to: the last one, so it is drawn
/// before everything else of the window and lies under the fill of the answers' box.
pub const BAR_SLOT: usize = 15;

/// How the bar is drawn (`0x8002F754`, `0x8002A3EC`): a frame of the menus in the kind
/// of the window colour, in the colour that pulses, half of what is under it and half
/// its colour. The time of its colour is the pulse of [`Answers`].
pub fn bar_style(window_colour: i16) -> crate::menu::Style {
    crate::menu::Style {
        kind: window_colour,
        mode: 1,
        blend: 1,
    }
}

pub const SETTING_AT_START: u32 = 0x01F0_283A;

/// Whether the picture of a script's message stands on the left: when the speaker is
/// seen left of the thing it speaks to, by the screen places the console's matrix unit
/// gives for the two (`0x800BC78C`).
pub fn picture_left(speaker_x: i32, other_x: i32) -> bool {
    speaker_x < other_x
}

impl Asked {
    /// What the scripts' builder `0x800BBFFC` asks the opener for when the script's
    /// task holds no flags and no place of its own: a message (commands `0x37` and
    /// `0x38`) at the bottom without answers, a question (`0x39` and `0x3A`) at the top
    /// with the group's answers less the ones the task has switched off. Typed unless
    /// the speed of the setting word is 3, hurried when its bits 3 and 4 are both set;
    /// a cancel picks the last answer. A group that has an entry in the room's table of
    /// cues is asked for as a window that is not drawn, which the opener overrules
    /// while bit 24 of the setting word is set.
    pub fn said(
        question: bool,
        setting: u32,
        pages: usize,
        answers: usize,
        switched_off: u32,
        picture_left: bool,
    ) -> Self {
        Self {
            place: if question { Where::Top } else { Where::Bottom },
            picture_left,
            pages,
            typed: setting & 7 != 3,
            hurried: setting & 0x18 == 0x18,
            turns: false,
            answers: if question { answers } else { 0 },
            open_answers: !switched_off,
            cancel_picks_last: true,
            hidden: false,
        }
    }

    pub fn taught(setting: u32, pages: usize) -> Self {
        Self {
            place: Where::Bottom,
            picture_left: true,
            pages,
            typed: setting & 7 != 3,
            hurried: setting & 0x18 == 0x18,
            turns: true,
            answers: 0,
            open_answers: 0,
            cancel_picks_last: false,
            hidden: false,
        }
    }
}

/// What the opener is told about a window, as far as its step reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Asked {
    pub place: Where,
    pub picture_left: bool,
    /// The pages of the group.
    pub pages: usize,
    /// The letters come one after the other.
    pub typed: bool,
    /// A held button brings all letters of a page at once.
    pub hurried: bool,
    /// A confirm turns a page without the owner's leave.
    pub turns: bool,
    /// The answers of the group, or 0 for a window without the answers' box.
    pub answers: usize,
    /// The answers that can be picked, one bit each.
    pub open_answers: u32,
    /// A cancel puts the cursor on the last answer.
    pub cancel_picks_last: bool,
    /// The window is not drawn: its pages go through whole.
    pub hidden: bool,
}

/// What the window waits for (+8 of its record).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wait {
    Nothing = 0,
    /// A page stands whole and another follows.
    More = 1,
    /// The last page stands whole and the group has no answers.
    Last = 2,
    /// The cursor is among the answers.
    Picking = 3,
    /// The last page stands whole and a confirm brings the answers.
    Asks = 4,
    Picked = 5,
    Closing = 6,
    Gone = 7,
}

/// What the two boxes do (+0xC).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Boxes {
    Away = 0,
    Opening = 1,
    Typing = 2,
    Whole = 3,
    Closing = 4,
}

/// What the answers' box does (+0x10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnswersBox {
    Away = 0,
    Opening = 1,
    Open = 2,
    Closing = 3,
}

/// What the text routine said of a page in a pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Read {
    pub whole: bool,
    /// The highest cue the page has reached.
    pub cue: Option<u8>,
}

/// The page a pass draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lines {
    pub page: usize,
    pub letters: u32,
    /// The rectangle the letters are cut off at.
    pub clip: Rect,
}

/// The answers' box of a pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Answers {
    /// The box as far as it is open; the lines are cut off at [`Layout::clip`] of it.
    pub rect: Rect,
    /// The line of the cursor among the answers that can be picked.
    pub cursor: i16,
    /// How far the bar's pulse is: 0 to 20.
    pub pulse: i16,
}

/// What a pass of the window draws and plays. A box too small for a frame
/// ([`is_drawn`]) is not drawn; the boxes are given in the order they are added, so
/// the last one lies under the others.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Pass {
    /// The speaker's name and the picture are drawn.
    pub furnished: bool,
    pub text_box: Option<Rect>,
    pub picture_box: Option<Rect>,
    pub lines: Option<Lines>,
    /// The sign of "more" is drawn.
    pub more: bool,
    pub answers: Option<Answers>,
    pub sounds: Vec<u8>,
    /// The pad's half asks the voice to end (`0x80040DC0`): in every pass in which
    /// the window waits for the confirm that brings the answers or for an answer.
    pub ends_voice: bool,
}

/// What the owner of a window gets once a confirm was taken (`0x80034A10`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Taken {
    /// The answer under the cursor, counted among all answers of the group.
    pub answer: i16,
    /// The confirms the window has taken.
    pub confirms: u16,
    /// An answer was picked.
    pub picked: bool,
    /// The window stands at its end: the last page of a group without answers, or the
    /// answers.
    pub at_end: bool,
}

const CONFIRM: u32 = 0x200;
const ALLOWED: u32 = 0x800;
const CLOSE: u32 = 0x4000;
const SIGNED: u32 = 0x8000;
const CANCELLED: u32 = 0x1_0000;
const SILENT: u32 = 0x2_0000;
const HIDDEN: u32 = 0x4_0000;
const SHOWN_AGAIN: u32 = 0x8_0000;

/// A message window from its opening to its end: the step `0x80034C74` and the pad's
/// half `0x800353FC`, one pass a game frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    asked: Asked,
    layout: Layout,
    answers_rect: Rect,
    flags: u32,
    pub wait: Wait,
    pub boxes: Boxes,
    pub answers: AnswersBox,
    /// How far the two boxes and the answers' box are open, of [`OPENING`].
    pub open: i32,
    pub answers_open: i32,
    /// The page that follows the one on screen, and the one on screen.
    next: usize,
    pub on_screen: usize,
    typing: crate::message::Typing,
    cues: [bool; CUES],
    cursor: i16,
    answer: i16,
    confirms: u16,
    picked: bool,
    at_end: bool,
    pulse: i16,
    /// The line of the page on screen is over ([`Window::hear`]).
    voice_over: bool,
}

/// Bits 22 and 23 of the setting word: "Speech" and "Auto Play".
const SPEECH_AND_AUTO_PLAY: u32 = 3 << 22;

/// The number of the `line`-th answer that can be picked (`0x80034BE4`).
fn answer_of(line: i16, open: u32) -> i16 {
    let mut left = i32::from(line);
    let mut answer: u32 = 0;
    // The routine has no end for a line that is not there; 32 bits are all there are.
    while left >= 0 && answer < 32 {
        if open & (1 << answer) != 0 {
            left -= 1;
        }
        answer += 1;
    }
    answer as i16 - 1
}

impl Window {
    /// The window as the opener leaves it. `widest` is the width of the widest answer
    /// that can be picked.
    pub fn open(asked: Asked, widest: i16) -> Self {
        let layout = Layout::of(asked.place, asked.picture_left);
        let open_answers = match asked.answers {
            0 => 0,
            count => asked.open_answers & (u32::MAX >> (32 - count.min(32))),
        };
        let lines = open_answers.count_ones() as i16;
        Self {
            asked: Asked {
                open_answers,
                ..asked
            },
            layout,
            answers_rect: layout.answers_box(widest, lines),
            flags: if asked.hidden { HIDDEN } else { 0 },
            wait: Wait::Nothing,
            boxes: Boxes::Opening,
            answers: AnswersBox::Away,
            open: 0,
            answers_open: 0,
            next: 0,
            on_screen: 0,
            typing: crate::message::Typing::default(),
            cues: [false; CUES],
            cursor: 0,
            answer: 0,
            confirms: 0,
            picked: false,
            at_end: false,
            pulse: 0,
            voice_over: false,
        }
    }

    pub fn hear(&mut self, over: bool) {
        self.voice_over = over;
    }

    pub fn layout(&self) -> Layout {
        self.layout
    }

    /// The owner lets the window take a confirm (`0x80034AC4`).
    pub fn allow(&mut self) {
        self.flags |= ALLOWED;
    }

    /// The owner closes the window (`0x80034AE0`).
    pub fn close(&mut self) {
        self.flags |= CLOSE;
    }

    /// The answer under the cursor while the cursor is among the answers and their box
    /// is open, counted among all answers of the group.
    pub fn pointed(&self) -> Option<i16> {
        (self.wait == Wait::Picking && self.answers == AnswersBox::Open).then_some(self.answer)
    }

    /// Whether a page or the answers stand whole (`0x80034A8C`).
    pub fn stands(&self) -> bool {
        self.boxes == Boxes::Whole || self.answers == AnswersBox::Open
    }

    /// What a taken confirm hands out, until the next pass has seen it.
    pub fn taken(&self) -> Option<Taken> {
        (self.flags & CONFIRM != 0).then_some(Taken {
            answer: self.answer,
            confirms: self.confirms,
            picked: self.picked,
            at_end: self.at_end,
        })
    }

    /// Whether the page on screen has reached a cue (`0x80034B38`).
    pub fn cue(&self, cue: usize) -> bool {
        self.cues.get(cue).copied().unwrap_or(false)
    }

    /// The task has ended.
    pub fn is_gone(&self) -> bool {
        self.wait == Wait::Gone
    }

    fn sound(&self, pass: &mut Pass, sound: u8) {
        if self.flags & SILENT == 0 {
            pass.sounds.push(sound);
        }
    }

    /// The page on screen from its first letter.
    fn begin_page(&mut self, page: usize) {
        self.on_screen = page;
        self.typing = crate::message::Typing::default();
        self.cues = [false; CUES];
    }

    /// The text routine of a pass (`0x80035888`) for the text's box as it stands.
    fn text(
        &mut self,
        pad: &Pad,
        setting: u32,
        text: Rect,
        pass: &mut Pass,
        read: &mut dyn FnMut(usize, u32) -> Read,
    ) -> bool {
        let clip = Layout::clip(text);
        if clip.w <= 0 || clip.h <= 0 {
            return true;
        }
        let letters = self.typing.begin(setting, self.asked.typed);
        let hidden = self.flags & HIDDEN != 0;
        // A page that is not drawn counts no letters and goes through whole.
        let told = read(
            self.on_screen,
            if hidden { crate::message::ALL } else { letters },
        );
        if let Some(cue) = told.cue {
            let reached = (usize::from(cue) + 1).min(CUES);
            self.cues[..reached].fill(true);
        }
        (self.typing).end(
            told.whole,
            self.asked.hurried && crate::message::hurries(pad),
        );
        if !hidden {
            pass.lines = Some(Lines {
                page: self.on_screen,
                letters,
                clip,
            });
        }
        told.whole
    }

    /// The sign of "more" while a confirm would count.
    fn sign(&mut self, pass: &mut Pass) {
        if self.asked.turns || self.flags & ALLOWED != 0 {
            self.flags |= SIGNED;
            pass.more = self.flags & HIDDEN == 0;
        }
    }

    /// One pass: what stands on the screen, then the pad. `setting` is the game's
    /// setting word, `voice_waits` whether a voice was asked for and has not been
    /// started (`0x80040E40` answers 0), and `read` the text routine for a page drawn
    /// with a number of letters.
    pub fn step(
        &mut self,
        pad: &Pad,
        setting: u32,
        voice_waits: bool,
        read: &mut dyn FnMut(usize, u32) -> Read,
    ) -> Pass {
        let mut pass = Pass::default();
        if self.flags & SHOWN_AGAIN != 0 {
            self.flags &= !(HIDDEN | SHOWN_AGAIN);
        }
        match self.wait {
            Wait::More => {
                self.sign(&mut pass);
                if self.flags & CONFIRM != 0 {
                    self.sound(&mut pass, TURN_SOUND);
                    self.flags &= !(SILENT | SIGNED | CONFIRM);
                    self.begin_page(self.next);
                    self.wait = Wait::Nothing;
                    self.boxes = Boxes::Typing;
                }
            }
            Wait::Last => {
                self.sign(&mut pass);
                if self.flags & CONFIRM != 0 {
                    self.sound(&mut pass, TURN_SOUND);
                    self.flags &= !(SILENT | SIGNED | CONFIRM);
                }
                if self.flags & CLOSE != 0 {
                    self.flags &= !(SIGNED | CLOSE);
                    self.wait = Wait::Closing;
                }
            }
            Wait::Picking => {
                if self.flags & CONFIRM != 0 {
                    self.sound(&mut pass, TURN_SOUND);
                    self.wait = Wait::Picked;
                    self.flags &= !(SILENT | CONFIRM);
                } else if self.flags & CANCELLED != 0 {
                    self.sound(&mut pass, CANCEL_SOUND);
                    self.flags &= !(SILENT | CANCELLED);
                }
                if self.flags & CLOSE != 0 {
                    self.flags &= !CLOSE;
                    self.wait = Wait::Closing;
                }
            }
            Wait::Asks => {
                if self.flags & CONFIRM != 0 {
                    self.sound(&mut pass, TURN_SOUND);
                    self.answers = AnswersBox::Opening;
                    self.wait = Wait::Nothing;
                    self.flags &= !(SILENT | CONFIRM);
                }
                pass.more = self.flags & HIDDEN == 0;
            }
            Wait::Picked => self.answers = AnswersBox::Closing,
            Wait::Closing => self.boxes = Boxes::Closing,
            Wait::Nothing | Wait::Gone => {}
        }

        let (text, picture) = (self.layout.text, self.layout.picture);
        let part = |rect: Rect, open: i32| opened(rect, open, OPENING, Way::Middle);
        match self.boxes {
            Boxes::Away => {}
            Boxes::Opening => {
                self.open += 1;
                if self.open >= OPENING {
                    self.open = OPENING;
                    self.boxes = Boxes::Typing;
                }
                pass.text_box = Some(part(text, self.open));
                pass.picture_box = Some(part(picture, self.open));
            }
            Boxes::Typing => {
                pass.furnished = true;
                if self.text(pad, setting, text, &mut pass, read) {
                    self.boxes = Boxes::Whole;
                    self.next += 1;
                    let more = self.next < self.asked.pages;
                    self.wait = match (more, self.asked.answers) {
                        (true, _) => Wait::More,
                        (false, 0) => {
                            self.at_end = true;
                            Wait::Last
                        }
                        (false, _) => Wait::Asks,
                    };
                    // A window that was not drawn comes up for the page before its
                    // answers.
                    let last = self.next + 1 >= self.asked.pages;
                    if more && last && self.flags & HIDDEN != 0 && self.asked.answers != 0 {
                        self.begin_page(self.next);
                        self.boxes = Boxes::Opening;
                        self.open = 0;
                        self.wait = Wait::Nothing;
                        self.answers = AnswersBox::Away;
                        self.flags |= SHOWN_AGAIN;
                    }
                }
                pass.text_box = Some(text);
                pass.picture_box = Some(picture);
            }
            Boxes::Whole => {
                pass.furnished = true;
                self.text(pad, setting, text, &mut pass, read);
                pass.text_box = Some(text);
                pass.picture_box = Some(picture);
            }
            Boxes::Closing => {
                self.open -= 1;
                if self.open < 1 {
                    self.open = 0;
                    self.boxes = Boxes::Away;
                    self.wait = Wait::Gone;
                }
                let rect = part(text, self.open);
                // The frame's own test of the size, with the sizes of its corners.
                if rect.w >= 16 && rect.h >= 12 {
                    self.text(pad, setting, rect, &mut pass, read);
                    pass.text_box = Some(rect);
                }
                pass.picture_box = Some(part(picture, self.open));
            }
        }

        let whole = match self.answers {
            AnswersBox::Away => None,
            AnswersBox::Opening => {
                self.answers_open += 1;
                if self.answers_open >= OPENING {
                    self.answers_open = OPENING;
                    self.answers = AnswersBox::Open;
                    self.wait = Wait::Picking;
                    self.at_end = true;
                }
                Some(false)
            }
            AnswersBox::Open => Some(true),
            AnswersBox::Closing => {
                self.answers_open -= 1;
                if self.answers_open < 1 {
                    self.answers_open = 0;
                    self.answers = AnswersBox::Away;
                    self.wait = Wait::Closing;
                }
                Some(false)
            }
        };
        if let Some(whole) = whole {
            let rect = match whole {
                true => self.answers_rect,
                false => opened(self.answers_rect, self.answers_open, OPENING, Way::Down),
            };
            if whole || (rect.w >= 16 && rect.h >= 12) {
                self.answers_pass(pad, rect, &mut pass);
            }
        }

        if self.flags & HIDDEN != 0 {
            pass = Pass {
                sounds: pass.sounds,
                ..Pass::default()
            };
        }
        pass.ends_voice = self.pad(pad, setting, voice_waits);
        pass
    }

    /// The answers' box of a pass (`0x80035668`): the cursor moves while an answer is
    /// picked, and the bar pulses.
    fn answers_pass(&mut self, pad: &Pad, rect: Rect, pass: &mut Pass) {
        if self.wait == Wait::Picking {
            let before = self.cursor;
            let lines = self.asked.open_answers.count_ones() as i16;
            // The menus' mover `0x8003C3C8` with one column: a step to the side ends
            // it before it looks up or down.
            let sideways = pad.repeated & (pad::RIGHT | pad::LEFT) != 0;
            self.cursor = self.cursor.max(0);
            if !sideways {
                let up = pad.repeated & pad::UP != 0;
                if up && self.cursor >= 1 {
                    self.cursor -= 1;
                }
                let stays = up && before.max(0) < 1;
                if !stays && pad.repeated & pad::DOWN != 0 && self.cursor <= lines - 2 {
                    self.cursor += 1;
                }
            }
            if self.cursor != before {
                pass.sounds.push(TURN_SOUND);
            }
        }
        self.pulse += 1;
        if self.pulse == PULSE {
            self.pulse = 0;
        }
        pass.answers = Some(Answers {
            rect,
            cursor: self.cursor,
            pulse: match self.pulse > PULSE / 2 {
                true => PULSE - self.pulse,
                false => self.pulse,
            },
        });
        self.answer = answer_of(self.cursor, self.asked.open_answers);
    }

    fn pad(&mut self, pad: &Pad, setting: u32, voice_waits: bool) -> bool {
        let confirmed = crate::menu::confirmed(pad);
        let allowed = self.flags & ALLOWED != 0;
        match self.wait {
            Wait::Picking => {
                if confirmed && allowed {
                    self.flags = self.flags & !ALLOWED | CONFIRM;
                    self.picked = true;
                    self.confirms = self.confirms.wrapping_add(1);
                } else if self.asked.cancel_picks_last && crate::menu::cancelled(pad) && allowed {
                    self.flags |= CANCELLED;
                    self.cursor = self.asked.open_answers.count_ones() as i16 - 1;
                    self.answer = answer_of(self.cursor, self.asked.open_answers);
                }
            }
            Wait::More | Wait::Last => {
                let may = allowed || self.asked.turns;
                if confirmed && !voice_waits {
                    if may {
                        self.flags = self.flags & !ALLOWED | CONFIRM;
                        self.confirms = self.confirms.wrapping_add(1);
                    }
                } else if setting & SPEECH_AND_AUTO_PLAY == SPEECH_AND_AUTO_PLAY
                    && self.voice_over
                    && !voice_waits
                    && may
                {
                    // "Auto Play": the page turns by itself behind its line, without
                    // the sound of a turn.
                    self.flags = self.flags & !ALLOWED | CONFIRM | SILENT;
                    self.confirms = self.confirms.wrapping_add(1);
                }
            }
            Wait::Asks if confirmed => self.flags |= CONFIRM,
            _ => {}
        }
        matches!(self.wait, Wait::Picking | Wait::Asks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZES: Sizes = [
        (8, 6),
        (8, 6),
        (8, 6),
        (8, 6),
        (4, 2),
        (4, 2),
        (4, 2),
        (4, 2),
    ];

    fn rect(x: i16, y: i16, w: i16, h: i16) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn the_bottom_window_stands_where_the_opener_puts_it() {
        let right = Layout::of(Where::Bottom, false);
        assert_eq!(right.picture, rect(234, 161, 70, 67));
        assert_eq!(right.text, rect(16, 172, 219, 56));
        assert_eq!(right.face, (237, 163));
        assert_eq!(right.first_line, crate::message::BOTTOM_LINE);
        let left = Layout::of(Where::Middle, true);
        assert_eq!(left.picture, rect(16, 116, 70, 67));
        assert_eq!(left.text, rect(84, 127, 219, 56));
        assert_eq!(Layout::clip(left.text), rect(90, 129, 213, 53));
    }

    #[test]
    fn a_box_opens_about_its_middle_and_the_answers_downwards() {
        let whole = rect(16, 172, 219, 56);
        assert_eq!(
            opened(whole, 2, OPENING, Way::Middle),
            rect(16 + 109 - 27, 172 + 28 - 7, 54, 14)
        );
        assert_eq!(opened(whole, OPENING, OPENING, Way::Middle), whole);
        assert_eq!(opened(whole, 9, OPENING, Way::Middle), whole);
        assert_eq!(opened(whole, 0, OPENING, Way::Middle).w, 0);
        assert_eq!(opened(whole, 4, OPENING, Way::Down), rect(16, 172, 219, 28));
        assert_eq!(
            opened(whole, 4, OPENING, Way::FromLowerRight),
            rect(16 + 219 - 109, 172 + 28, 109, 28)
        );
    }

    #[test]
    fn a_small_rectangle_has_no_frame() {
        assert!(frame(rect(0, 0, 15, 40), &SIZES).is_none());
        assert!(frame(rect(0, 0, 40, 11), &SIZES).is_none());
        assert!(is_drawn(rect(0, 0, 16, 12), &SIZES));
        // The first pass of an opening text box is too low, the second is not.
        let whole = Layout::of(Where::Bottom, false).text;
        assert!(!is_drawn(opened(whole, 1, OPENING, Way::Middle), &SIZES));
        assert!(is_drawn(opened(whole, 2, OPENING, Way::Middle), &SIZES));
    }

    #[test]
    fn the_pieces_of_a_frame_lie_round_the_rectangle() {
        let pieces = frame(rect(10, 20, 100, 50), &SIZES).expect("a frame");
        let of = |part: Part| {
            *pieces
                .iter()
                .find(|piece| piece.part == part)
                .expect("part")
        };
        assert_eq!(of(Part::UpperLeft).on_screen, rect(10, 20, 8, 6));
        assert_eq!(of(Part::UpperRight).on_screen, rect(102, 20, 8, 6));
        assert_eq!(of(Part::LowerLeft).on_screen, rect(10, 64, 8, 6));
        assert_eq!(of(Part::LowerRight).on_screen, rect(102, 64, 8, 6));
        assert_eq!(of(Part::UpperEdge).on_screen, rect(18, 20, 84, 2));
        assert_eq!(of(Part::LowerEdge).on_screen, rect(18, 68, 84, 2));
        assert_eq!(of(Part::LeftEdge).on_screen, rect(10, 26, 3, 38));
        assert_eq!(of(Part::RightEdge).on_screen, rect(106, 26, 3, 38));
        // The corner that was added first is drawn last.
        assert_eq!(pieces[7].part, Part::UpperLeft);
        assert_eq!(of(Part::UpperLeft).image_number(), 79);
        assert_eq!(of(Part::UpperEdge).image_number(), 86);
        // An upright edge shows its first three columns, one a pixel.
        let left = of(Part::LeftEdge);
        assert_eq!(left.texel(0, 0), (0, 0));
        assert_eq!(left.texel(2, 37), (2, 0));
        assert_eq!(of(Part::UpperLeft).texel(5, 3), (5, 3));
    }

    #[test]
    fn the_fill_lies_inside_and_the_sign_pulses() {
        assert_eq!(fill(rect(16, 172, 219, 56)), rect(19, 174, 213, 52));
        assert_eq!(more_at((23, 175)), (218, 210));
        assert_eq!(more_brightness(0), 0);
        assert_eq!(more_brightness(8), 128);
        assert_eq!(more_brightness(15), 240);
        assert_eq!(more_brightness(16), 0);
    }

    #[test]
    fn the_fill_colours_are_read_through_their_addresses() {
        let base = FILLS - 8;
        let mut image = vec![0u8; 8 + 4 * COLOURS + 4 * COLOURS];
        for kind in 0..COLOURS {
            let address = FILLS + 4 * (COLOURS + kind) as u32;
            image[8 + 4 * kind..12 + 4 * kind].copy_from_slice(&address.to_le_bytes());
            let colour = [kind as u8 + 1, 20, 30, 0];
            let at = (address - base) as usize;
            image[at..at + 4].copy_from_slice(&colour);
        }
        let colours = fill_colours(&image, base).expect("colours");
        assert_eq!(colours[0], [1, 20, 30]);
        assert_eq!(colours[4], [5, 20, 30]);
        assert!(fill_colours(&image[..20], base).is_none());
    }

    #[test]
    fn the_name_and_the_answers_stand_by_the_text() {
        assert_eq!(
            Layout::name_box(Where::Bottom, false, 40),
            rect(16, 156, 57, 18)
        );
        assert_eq!(
            Layout::name_box(Where::Top, false, 40),
            rect(248, 79, 57, 18)
        );
        assert_eq!(
            Layout::name_box(Where::Top, true, 40),
            rect(247, 81, 57, 18)
        );
        assert_eq!(
            Layout::name_box(Where::Middle, true, 40),
            rect(247, 111, 57, 18)
        );
        let answers = Layout::of(Where::Top, false).answers_box(40, 2);
        assert_eq!(answers, rect(26, 88, 57, 31));
        assert_eq!(Layout::of(Where::Top, true).answers_box(40, 2), answers);
        assert_eq!(answer_at(answers, 1), (34, 102));
        assert_eq!(bar(answers, 1), rect(30, 103, 49, 13));
        let style = bar_style(3);
        assert_eq!(
            (style.kind, style.mode, style.blend_mode()),
            (3, 1, Some(0))
        );
        assert_eq!(answer_of(0, 0b101), 0);
        assert_eq!(answer_of(1, 0b101), 2);
        assert_eq!(answer_of(1, 0b110), 2);
    }

    /// The setting with a letter in every pass.
    const SPEED: u32 = 2;

    fn asked(pages: usize, answers: usize) -> Asked {
        Asked {
            place: Where::Bottom,
            picture_left: false,
            pages,
            typed: true,
            hurried: false,
            turns: false,
            answers,
            open_answers: u32::MAX,
            cancel_picks_last: true,
            hidden: false,
        }
    }

    /// A text routine whose pages have three letters and reach the cue of their number
    /// with the first.
    fn read(page: usize, letters: u32) -> Read {
        Read {
            whole: letters >= 3,
            cue: (letters > 0).then_some(page as u8),
        }
    }

    fn press(buttons: u16) -> Pad {
        let mut pad = Pad::default();
        pad.read(buttons, pad::Repeat::of(2), 0);
        pad
    }

    #[test]
    fn a_page_turns_by_itself_behind_its_line_without_a_sound() {
        let quiet = Pad::default();
        // "Speech" and "Auto Play" with the speed of the other tests, and each alone.
        let both = SPEED | SPEECH_AND_AUTO_PLAY;
        let stood = || {
            let mut window = Window::open(asked(2, 0), 0);
            while window.wait != Wait::More {
                window.step(&quiet, both, false, &mut read);
            }
            window
        };
        // Nothing turns while the line plays, without the owner's leave, while a line
        // is asked for, or with one of the two settings off.
        let mut window = stood();
        window.allow();
        window.step(&quiet, both, false, &mut read);
        assert!(window.taken().is_none());
        let mut window = stood();
        window.hear(true);
        window.step(&quiet, both, false, &mut read);
        assert!(window.taken().is_none());
        window.allow();
        window.step(&quiet, both, true, &mut read);
        assert!(window.taken().is_none());
        for one in [1 << 22, 1 << 23] {
            window.step(&quiet, SPEED | one, false, &mut read);
            assert!(window.taken().is_none());
        }
        // The line is over: the page turns as by a confirm, and no sound is played.
        window.step(&quiet, both, false, &mut read);
        let taken = window.taken().expect("a turn");
        assert!(taken.confirms == 1 && !taken.at_end);
        let pass = window.step(&quiet, both, false, &mut read);
        assert!(pass.sounds.is_empty());
        assert_eq!(pass.lines.map(|l| (l.page, l.letters)), Some((1, 0)));
        // A confirm of the player on the next page sounds again.
        window.hear(false);
        while window.wait != Wait::Last {
            window.step(&quiet, both, false, &mut read);
        }
        window.allow();
        window.step(&press(pad::CROSS), both, false, &mut read);
        window.close();
        let pass = window.step(&quiet, both, false, &mut read);
        assert_eq!(pass.sounds, [TURN_SOUND]);
    }

    #[test]
    fn a_window_opens_types_turns_and_closes() {
        let (quiet, cross) = (Pad::default(), press(pad::CROSS));
        let mut window = Window::open(asked(2, 0), 0);
        let layout = window.layout();
        for count in 1..=OPENING {
            assert_eq!(window.boxes, Boxes::Opening);
            let pass = window.step(&quiet, SPEED, false, &mut read);
            let text = opened(layout.text, count, OPENING, Way::Middle);
            assert_eq!(pass.text_box, Some(text));
            assert!(!pass.furnished && pass.lines.is_none());
        }
        assert_eq!(window.boxes, Boxes::Typing);
        for letters in 0..=3 {
            let pass = window.step(&quiet, SPEED, false, &mut read);
            let lines = Lines {
                page: 0,
                letters,
                clip: Layout::clip(layout.text),
            };
            assert_eq!(pass.lines, Some(lines));
            assert!(pass.furnished);
            assert_eq!(window.stands(), letters == 3);
        }
        assert_eq!(window.wait, Wait::More);
        assert!(window.cue(0) && !window.cue(1));
        // A confirm does not count before the owner allows it, nor while a voice waits.
        let pass = window.step(&cross, SPEED, false, &mut read);
        assert!(!pass.more && window.taken().is_none());
        window.allow();
        let pass = window.step(&cross, SPEED, true, &mut read);
        assert!(pass.more && window.taken().is_none());
        window.step(&cross, SPEED, false, &mut read);
        let taken = Taken {
            answer: 0,
            confirms: 1,
            picked: false,
            at_end: false,
        };
        assert_eq!(window.taken(), Some(taken));
        // The next pass turns the page.
        let pass = window.step(&quiet, SPEED, false, &mut read);
        assert_eq!(pass.sounds, [TURN_SOUND]);
        assert_eq!(pass.lines.map(|l| (l.page, l.letters)), Some((1, 0)));
        assert!(window.taken().is_none() && !window.cue(0) && !pass.more);
        for letters in 1..=3 {
            let pass = window.step(&cross, SPEED, false, &mut read);
            assert_eq!(pass.lines.map(|l| (l.page, l.letters)), Some((1, letters)));
        }
        assert_eq!(window.wait, Wait::Last);
        assert!(window.cue(1) && window.taken().is_none());
        window.allow();
        window.step(&cross, SPEED, false, &mut read);
        let taken = window.taken().expect("a confirm");
        assert!(taken.at_end && taken.confirms == 2 && !taken.picked);
        window.close();
        let pass = window.step(&quiet, SPEED, false, &mut read);
        assert_eq!(pass.sounds, [TURN_SOUND]);
        assert_eq!(window.wait, Wait::Closing);
        assert_eq!(pass.text_box, Some(layout.text));
        for count in (0..OPENING).rev() {
            let pass = window.step(&quiet, SPEED, false, &mut read);
            let picture = opened(layout.picture, count, OPENING, Way::Middle);
            assert_eq!(pass.picture_box, Some(picture));
            // The text is drawn while its box is large enough for a frame.
            assert_eq!(pass.lines.is_some(), count >= 2);
            assert!(!pass.furnished);
            assert_eq!(window.is_gone(), count == 0);
        }
    }

    #[test]
    fn a_page_that_turns_by_itself_needs_no_leave() {
        let mut asked = asked(2, 0);
        (asked.turns, asked.typed) = (true, false);
        let mut window = Window::open(asked, 0);
        let quiet = Pad::default();
        for _ in 0..OPENING {
            window.step(&quiet, SPEED, false, &mut read);
        }
        // Not typed: the page is whole in its first pass.
        let pass = window.step(&quiet, SPEED, false, &mut read);
        assert_eq!(pass.lines.map(|l| l.letters), Some(crate::message::ALL));
        assert_eq!(window.wait, Wait::More);
        let pass = window.step(&press(pad::CROSS), SPEED, false, &mut read);
        assert!(pass.more && window.taken().is_some());
    }

    #[test]
    fn the_answers_open_under_the_last_page() {
        let (quiet, cross) = (Pad::default(), press(pad::CROSS));
        let mut asked = asked(1, 3);
        asked.open_answers = 0b101;
        let mut window = Window::open(asked, 40);
        let whole = window.layout().answers_box(40, 2);
        for _ in 0..OPENING + 4 {
            window.step(&quiet, SPEED, false, &mut read);
        }
        assert_eq!(window.wait, Wait::Asks);
        // The sign is there without the owner's leave, and so is the confirm.
        let pass = window.step(&cross, SPEED, false, &mut read);
        assert!(pass.more && pass.answers.is_none());
        let taken = window.taken().expect("a confirm");
        assert!(!taken.at_end && taken.confirms == 0);
        for count in 1..=OPENING {
            let pass = window.step(&quiet, SPEED, false, &mut read);
            assert_eq!(pass.sounds.len(), usize::from(count == 1));
            let rect = opened(whole, count, OPENING, Way::Down);
            assert_eq!(
                pass.answers.map(|answers| answers.rect),
                (count >= 4).then_some(rect)
            );
            if count == 4 {
                assert_eq!(pass.answers.map(|answers| answers.pulse), Some(1));
            }
        }
        assert_eq!(
            (window.wait, window.answers),
            (Wait::Picking, AnswersBox::Open)
        );
        assert!(window.stands());
        // Down to the second line, which is the third answer; no further.
        let down = press(pad::DOWN);
        let pass = window.step(&down, SPEED, false, &mut read);
        assert_eq!(pass.sounds, [TURN_SOUND]);
        assert_eq!(pass.answers.map(|answers| answers.cursor), Some(1));
        let pass = window.step(&down, SPEED, false, &mut read);
        assert!(pass.sounds.is_empty());
        // A step to the side keeps the cursor from going up.
        let pass = window.step(&press(pad::UP | pad::LEFT), SPEED, false, &mut read);
        assert_eq!(pass.answers.map(|answers| answers.cursor), Some(1));
        let pass = window.step(&press(pad::UP), SPEED, false, &mut read);
        assert_eq!(pass.answers.map(|answers| answers.cursor), Some(0));
        // Neither a cancel nor a confirm counts without the owner's leave.
        window.step(&press(pad::TRIANGLE), SPEED, false, &mut read);
        window.step(&cross, SPEED, false, &mut read);
        assert!(window.taken().is_none());
        window.allow();
        window.step(&press(pad::TRIANGLE), SPEED, false, &mut read);
        let pass = window.step(&quiet, SPEED, false, &mut read);
        assert_eq!(pass.sounds, [CANCEL_SOUND]);
        assert_eq!(pass.answers.map(|answers| answers.cursor), Some(1));
        window.step(&cross, SPEED, false, &mut read);
        let taken = Taken {
            answer: 2,
            confirms: 1,
            picked: true,
            at_end: true,
        };
        assert_eq!(window.taken(), Some(taken));
        window.close();
        let pass = window.step(&quiet, SPEED, false, &mut read);
        assert_eq!(pass.sounds, [TURN_SOUND]);
        assert_eq!(window.wait, Wait::Closing);
        // Closed by its owner before the next pass, the window shrinks with the
        // answers' box still whole; left alone the answers' box shrinks first.
        let pass = window.step(&quiet, SPEED, false, &mut read);
        assert_eq!(pass.answers.map(|answers| answers.rect), Some(whole));
        assert_eq!((window.boxes, window.open), (Boxes::Closing, OPENING - 1));
        let mut alone = Window::open(asked, 40);
        for _ in 0..OPENING + 4 {
            alone.step(&quiet, SPEED, false, &mut read);
        }
        alone.step(&cross, SPEED, false, &mut read);
        for _ in 0..OPENING {
            alone.step(&quiet, SPEED, false, &mut read);
        }
        alone.allow();
        alone.step(&cross, SPEED, false, &mut read);
        alone.step(&quiet, SPEED, false, &mut read);
        assert_eq!(alone.wait, Wait::Picked);
        for count in (0..OPENING).rev() {
            assert_eq!(alone.wait, Wait::Picked);
            let pass = alone.step(&quiet, SPEED, false, &mut read);
            assert_eq!(pass.answers.is_some(), count >= 4);
            assert!(pass.text_box.is_some());
        }
        assert_eq!(
            (alone.wait, alone.answers),
            (Wait::Closing, AnswersBox::Away)
        );
        for _ in 0..OPENING {
            assert!(!alone.is_gone());
            alone.step(&quiet, SPEED, false, &mut read);
        }
        assert!(alone.is_gone());
    }

    #[test]
    fn the_voice_is_asked_to_end_while_the_answers_are_waited_for() {
        let (quiet, cross) = (Pad::default(), press(pad::CROSS));
        let mut window = Window::open(asked(1, 2), 40);
        // Not while the page opens and is typed, from the pass it stands whole in on.
        let mut ends = Vec::new();
        while window.wait != Wait::Asks {
            ends.push(window.step(&quiet, SPEED, false, &mut read).ends_voice);
        }
        let last = ends.pop();
        assert!(last == Some(true) && ends.iter().all(|&ends| !ends));
        // The confirm that brings the answers; not while their box opens.
        assert!(window.step(&cross, SPEED, false, &mut read).ends_voice);
        for count in 1..=OPENING {
            let pass = window.step(&quiet, SPEED, false, &mut read);
            assert_eq!(pass.ends_voice, count == OPENING);
        }
        // In every pass of the picking, and no more once an answer is picked.
        assert_eq!(window.wait, Wait::Picking);
        assert!(window.step(&quiet, SPEED, false, &mut read).ends_voice);
        window.allow();
        assert!(window.step(&cross, SPEED, false, &mut read).ends_voice);
        assert!(!window.step(&quiet, SPEED, false, &mut read).ends_voice);
        // A group without answers never asks.
        let mut plain = Window::open(asked(2, 0), 0);
        for _ in 0..OPENING + 40 {
            assert!(!plain.step(&quiet, SPEED, false, &mut read).ends_voice);
        }
    }

    #[test]
    fn a_window_that_is_not_drawn_comes_up_for_its_answers() {
        let quiet = Pad::default();
        let mut hidden = asked(2, 2);
        hidden.hidden = true;
        let mut window = Window::open(hidden, 40);
        for _ in 0..OPENING {
            let pass = window.step(&quiet, SPEED, false, &mut read);
            assert_eq!(pass, Pass::default());
        }
        // The first page goes through whole in one pass.
        let pass = window.step(&quiet, SPEED, false, &mut read);
        assert_eq!(pass, Pass::default());
        assert_eq!((window.boxes, window.on_screen), (Boxes::Opening, 1));
        let pass = window.step(&quiet, SPEED, false, &mut read);
        assert!(pass.text_box.is_some());
        // Without answers it stays away and waits on its pages.
        hidden.answers = 0;
        let mut window = Window::open(hidden, 0);
        for _ in 0..OPENING + 1 {
            window.step(&quiet, SPEED, false, &mut read);
        }
        assert_eq!(window.wait, Wait::More);
        window.allow();
        let pass = window.step(&quiet, SPEED, false, &mut read);
        assert_eq!(pass, Pass::default());
    }

    #[test]
    fn the_places_of_the_faces_are_pairs_of_the_executable() {
        let base = PLAYER_FACE;
        let mut image = Vec::new();
        for value in 1..=20i16 {
            image.extend(value.to_le_bytes());
        }
        let faces = Faces::from_executable(&image, base).expect("faces");
        assert_eq!(faces.player.picture, (1, 2));
        assert_eq!(faces.player.palette, (3, 4));
        assert_eq!(faces.slots[0].picture, (5, 6));
        assert_eq!(faces.slots[3].picture, (11, 12));
        assert_eq!(faces.slots[0].palette, (13, 14));
        assert_eq!(faces.slots[3].palette, (19, 20));
        assert!(faces.is_face((1, 2)) && faces.is_face((5, 6)) && !faces.is_face((7, 8)));
        assert!(Faces::from_executable(&image[..38], base).is_none());
    }

    #[test]
    fn the_builder_asks_as_the_script_does() {
        let message = Asked::said(false, SETTING_AT_START, 3, 2, 0, true);
        assert_eq!((message.place, message.picture_left), (Where::Bottom, true));
        assert_eq!(
            (message.pages, message.answers),
            (3, 0),
            "a message has no answers"
        );
        assert!(message.typed && message.hurried && message.cancel_picks_last);
        assert!(!message.turns && !message.hidden);
        let question = Asked::said(true, SETTING_AT_START & !0x1F | 3, 1, 4, 0b10, false);
        assert_eq!((question.place, question.answers), (Where::Top, 4));
        assert_eq!(
            question.open_answers & 0b1111,
            0b1101,
            "an answer switched off"
        );
        assert!(!question.typed, "speed 3 is not typed");
        assert!(!question.hurried, "both hurry bits are needed");
        assert!(!Asked::said(false, 0x08, 1, 0, 0, false).hurried);
        // The boot leaves speed 2 and both hurry bits.
        assert_eq!(SETTING_AT_START & 7, 2);
        assert_eq!(SETTING_AT_START & 0x18, 0x18);
        assert_ne!(SETTING_AT_START & 1 << 24, 0);
    }

    #[test]
    fn the_picture_stands_left_of_a_speaker_seen_on_the_left() {
        assert!(picture_left(-43, 71));
        assert!(!picture_left(71, -43));
        assert!(!picture_left(5, 5));
    }

    #[test]
    fn a_face_is_read_from_video_memory_through_its_palette() {
        // A picture of 64 by 64 at (352, 448) whose texel is its column, and a palette
        // at (768, 386) whose colour is its number: colour 0 is not drawn.
        let mut data = Vec::new();
        data.extend_from_slice(&0x10u32.to_le_bytes());
        data.extend_from_slice(&9u32.to_le_bytes());
        let colours: Vec<u16> = (0..256).collect();
        let row: Vec<u16> = (0..32)
            .map(|pair| (2 * pair) | ((2 * pair + 1) << 8))
            .collect();
        let pixels: Vec<u16> = (0..64).flat_map(|_| row.clone()).collect();
        for ((x, y, w, h), words) in [
            ((768u16, 386u16, 256u16, 1u16), &colours),
            ((352, 448, 32, 64), &pixels),
        ] {
            data.extend_from_slice(&(12 + 2 * words.len() as u32).to_le_bytes());
            for value in [x, y, w, h] {
                data.extend_from_slice(&value.to_le_bytes());
            }
            for word in words {
                data.extend_from_slice(&word.to_le_bytes());
            }
        }
        let image = crate::tim::Tim::parse(&data, 0).unwrap();
        let vram = Vram::with([&image]);
        let place = FacePlace {
            picture: (352, 448),
            palette: (768, 386),
        };
        let rgba = place.rgba(&vram);
        assert_eq!(rgba.len(), 64 * 64 * 4);
        assert_eq!(rgba[..4], [0, 0, 0, 0], "colour zero is not drawn");
        // Column 5 of the last row: red 5 of 31.
        let at = (63 * 64 + 5) * 4;
        assert_eq!(rgba[at..at + 4], [5 << 3 | 5 >> 2, 0, 0, 255]);
        // Column 37: red 5, green 1.
        let at = 37 * 4;
        assert_eq!(rgba[at..at + 4], [5 << 3 | 5 >> 2, 1 << 3, 0, 255]);
    }
}
