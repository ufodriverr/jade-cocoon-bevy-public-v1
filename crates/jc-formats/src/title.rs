
use crate::menu::{self, Cursor, Rect, Style};
use crate::name;
use crate::pad::{self, Pad};
use crate::text;

/// The archive file with the title's pictures, a texture set of nine images.
pub const PICTURES: usize = 54;

/// Passes of the title's task in a second: one for every picture of the screen.
pub const PASSES_A_SECOND: u32 = 50;

/// The screen the pictures are placed on.
pub const SCREEN: (i16, i16) = (320, 240);

/// An image of the texture set and where its upper left corner goes on the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placed {
    pub image: usize,
    pub x: i16,
    pub y: i16,
}

/// The two halves of the title picture, drawn in every pass of both screens.
pub const BACKGROUND: [Placed; 2] = [
    Placed {
        image: 7,
        x: 0,
        y: 0,
    },
    Placed {
        image: 8,
        x: 160,
        y: 0,
    },
];

/// The words of the "press Start" screen.
pub const PRESS_START: Placed = Placed {
    image: 2,
    x: 151,
    y: 116,
};

/// Passes without a button after which the title leaves: the count has to pass this.
pub const IDLE_PASSES: u32 = 6000;

/// Passes of the way out, behind which the title returns the state of the opening movie.
pub const WAY_OUT_PASSES: u32 = 91;

/// The frame around the words of the "press Start" screen. It always pulses.
pub const PRESS_START_FRAME: Rect = Rect {
    x: 144,
    y: 112,
    w: 166,
    h: 18,
};

/// How that frame is drawn: the same as the frame of the menu's line under the cursor.
pub const PRESS_START_STYLE: Style = Style {
    kind: 4,
    mode: 1,
    blend: 1,
};

/// A line of the menu: the image with its words and the frame around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Line {
    pub image: usize,
    pub frame: Rect,
}

const fn line(image: usize, y: i16) -> Line {
    Line {
        image,
        frame: Rect {
            x: 186,
            y,
            w: 108,
            h: 16,
        },
    }
}

/// The four lines of the menu, from the top: Start Game, Continue, Arena Battle Stage,
/// Options.
pub const LINES: [Line; 4] = [line(3, 84), line(4, 108), line(0, 132), line(1, 156)];

/// The margin between a line's frame and the room for its words, and the flags of the
/// piece that holds them (`menu::piece`).
pub const LINE_MARGIN: (i16, i16) = (0, 2);
pub const LINE_FLAGS: u16 = menu::piece::CENTRED;

/// The flag word of the menu's record: frames of kind 4, the one under the cursor
/// pulses (`menu::Style::of_line`).
pub const MENU_FLAGS: u32 = 0x2102;

/// The line the cursor is on when the console starts.
pub const FIRST_LINE: i16 = 1;

/// The music number the title's entry loads and starts. Nothing stops it until the
/// second movie of a new game is over, or the title goes out when it is left alone.
pub const MUSIC: usize = 27;

/// The movie "Start Game" plays first.
pub const NEW_GAME_MOVIE: usize = 6;

/// The movie the new-game task plays when the name entry is over.
pub const NAMED_MOVIE: usize = 7;
pub const SECOND_ROUND_MOVIE: usize = 8;
pub const NAMED_AGAIN_MOVIE: usize = 9;
/// The room the state setup of a second round names (`0x800A2A24`: area 0, place 4,
/// record 0), the first of the prologue rooms.
pub const SECOND_ROUND_ROOM: u16 = 339;

/// Passes the new-game task counts behind that movie before it leaves for the field.
pub const LAST_WAIT: u32 = 60;

/// The archive file of the title's code, which holds the texts of the warnings.
pub const OVERLAY: usize = 23;

/// Blocks of a memory card that files can take.
pub const CARD_BLOCKS: i32 = 15;
/// Bytes of a block.
const BLOCK: i32 = 8192;

/// Whether the card library's answer for a slot is a card the check goes on to read: no
/// fault (0), a card put in since the last look (3) or one that is not formatted (4). No
/// card (1), a fault of the line (2) and anything else is passed over.
pub fn card_answers(result: i32) -> bool {
    result == 0 || (3..5).contains(&result)
}

/// The free blocks of a card from the sizes of its files in bytes: a file takes a block
/// for every 8192 bytes and one more for a rest.
pub fn free_blocks(sizes: &[i32]) -> i32 {
    let used = (sizes.iter()).fold(0i32, |used, &size| {
        let rest = i32::from(size & (BLOCK - 1) != 0);
        used.wrapping_add(size.wrapping_div(BLOCK))
            .wrapping_add(rest)
    });
    CARD_BLOCKS.wrapping_sub(used)
}

/// A slot of the console as the card check of "Start Game" finds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Slot {
    /// No card that answers ([`card_answers`]).
    #[default]
    Empty,
    /// A card with its free blocks and the number of this game's files on it.
    Card { free: i32, saves: u32 },
}

/// The two warnings of "Start Game".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Warning {
    /// No slot holds a card: the game cannot be saved.
    NoCard,
    /// A card is there, but none with a free block or a file of this game.
    Full,
}

/// Where a line of a warning starts, and how far below it the next one.
pub const WARNING_LEFT: i16 = 16;
pub const WARNING_STEP: i16 = 16;
/// The font of the warnings.
pub const WARNING_FONT: usize = text::TEXT_FONT;

impl Warning {
    /// The addresses of the warning's lines in the title's code. Both start with the
    /// same line.
    pub fn lines(self) -> &'static [u32] {
        match self {
            Self::NoCard => &[0x800A_054C, 0x800A_0560, 0x800A_05AC, 0x800A_05E8],
            Self::Full => &[
                0x800A_054C,
                0x800A_0600,
                0x800A_062C,
                0x800A_0668,
                0x800A_06A8,
                0x800A_06E0,
            ],
        }
    }

    /// The upper edge of the first line. Both warnings end at the same height.
    pub fn top(self) -> i16 {
        match self {
            Self::NoCard => 90,
            Self::Full => 58,
        }
    }

    /// Every line's address and the place of its first glyph's cell.
    pub fn placed(self) -> impl Iterator<Item = (u32, (i16, i16))> {
        (self.lines().iter().zip(0i16..))
            .map(move |(&line, row)| (line, (WARNING_LEFT, self.top() + row * WARNING_STEP)))
    }
}

/// The card check of "Start Game": the slots are asked in their order. The first card
/// with a free block or a file of this game ends the check without a warning; a card
/// without either asks for the warning of full cards, and no card at all for the other.
pub fn card_check(slots: &[Slot; 2]) -> Option<Warning> {
    let mut warning = Warning::NoCard;
    for slot in slots {
        match *slot {
            Slot::Empty => {}
            Slot::Card { free, saves } if free != 0 || saves != 0 => return None,
            Slot::Card { .. } => warning = Warning::Full,
        }
    }
    Some(warning)
}

/// What a line of the menu starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    StartGame,
    Continue,
    ArenaBattleStage,
    Options,
}

impl Choice {
    /// The choice of a line of the menu.
    pub fn of_line(line: i16) -> Option<Self> {
        match line {
            0 => Some(Self::StartGame),
            1 => Some(Self::Continue),
            2 => Some(Self::ArenaBattleStage),
            3 => Some(Self::Options),
            _ => None,
        }
    }
}

/// What the title's task is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    PressStart,
    Menu,
    /// "Start Game" was confirmed: the pass of the memory card check. Nothing is drawn
    /// from here to the new game but a warning's lines.
    CardCheck,
    /// A warning stands until a confirm.
    Warning(Warning),
    /// The check found a card to save on: the pass that hands over to the new game.
    NoWarning,
    /// The music is stopped and the title counts its last passes.
    WayOut,
    OptionsWait,
    /// The options screen is up and draws itself: the task only asks whether it
    /// still is.
    Options,
    /// The options screen has ended: the pass that draws the menu again behind the
    /// held picture.
    OptionsBack,
    Card,
}

/// What a pass changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Stays,
    /// Start went down on the "press Start" screen.
    ToMenu,
    /// A cancel in the menu: the dispatcher picks the "press Start" screen again.
    ToPressStart,
    /// A confirm in the menu on the cursor's line. What the line starts is the
    /// caller's, and the title stays in the menu, but for "Start Game", whose next
    /// pass is the memory card check, for "Options", whose next pass waits for
    /// the options screen's pictures, and for "Continue", which starts the memory
    /// card's screen: the caller runs it and says when it has ended
    /// ([`Title::card_ended`]).
    Chosen(Choice),
    /// The memory card's screen is gone and no game was loaded: the menu is the
    /// task's routine again.
    CardOver,
    /// The memory card's screen has loaded a game: the title is over and the field
    /// starts with that game (the title leaves with 7).
    Loaded,
    /// The task of the options screen is started; the caller runs it and says when it
    /// has ended ([`Title::options_ended`]).
    OptionsStart,
    /// The options screen is gone: the menu is drawn again, not yet seen.
    OptionsOver,
    /// The menu is seen and is the task's routine again from the next pass on.
    BackInMenu,
    /// The card check asks for a warning, which stands from the next pass on.
    Warns(Warning),
    /// The title's task hands over to the task of a new game. When it wakes again it is
    /// in the menu.
    NewGame,
    /// Left alone for too long: the way out begins.
    GoesOut,
    /// The title is over and the opening movie plays again.
    OpeningMovie,
}

/// The title's task between its start and its end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Title {
    pub screen: Screen,
    /// The menu's cursor. It keeps its line while the title is up.
    pub cursor: Cursor,
    /// The memory card slots as the card check will find them.
    pub slots: [Slot; 2],
    /// Passes since the last button that counts.
    idle: u32,
    /// Passes of the way out.
    way_out: u32,
    /// The options screen's own word says it is up (`0x800C47E0`).
    options_up: bool,
    /// The memory card screen's own word says it is up (`0x800B7B54`), and whether it
    /// has loaded a game (`0x800B7B2C`).
    card_up: bool,
    card_loaded: bool,
}

impl Default for Title {
    fn default() -> Self {
        Self::new()
    }
}

impl Title {
    /// The title as a start of the console shows it: the "press Start" screen.
    pub fn new() -> Self {
        Self {
            screen: Screen::PressStart,
            cursor: Cursor {
                columns: 1,
                rows: LINES.len() as i16,
                lines: LINES.len() as u16,
                flags: menu::cursor::WRAPS_DOWN,
                at: FIRST_LINE,
                first: 0,
            },
            slots: [Slot::Empty; 2],
            idle: 0,
            way_out: 0,
            options_up: false,
            card_up: false,
            card_loaded: false,
        }
    }

    /// The memory card screen's task has ended, with a loaded game or without one: the
    /// next pass of the title finds its word cleared.
    pub fn card_ended(&mut self, loaded: bool) {
        self.card_up = false;
        self.card_loaded = loaded;
    }

    /// The options screen's task has ended: the next pass of the title finds its word
    /// cleared.
    pub fn options_ended(&mut self) {
        self.options_up = false;
    }

    pub fn sounds(&self, pad: &Pad) -> Vec<u8> {
        let confirm = |confirmed: bool| confirmed.then_some(name::SOUND_CONFIRM);
        match self.screen {
            Screen::PressStart => confirm(pad.pressed & pad::START != 0).into_iter().collect(),
            Screen::Menu => {
                let mut cursor = self.cursor;
                let moved = cursor.step(pad.repeated).sounds.then_some(name::SOUND_MOVE);
                let cancel = menu::cancelled(pad).then_some(name::SOUND_CANCEL);
                let asked = confirm(menu::confirmed(pad)).or(cancel);
                moved.into_iter().chain(asked).collect()
            }
            Screen::Warning(_) => confirm(menu::confirmed(pad)).into_iter().collect(),
            _ => Vec::new(),
        }
    }

    /// One pass of the task with the pad's words of that pass.
    pub fn pass(&mut self, pad: &Pad) -> Step {
        let step = match self.screen {
            Screen::PressStart if pad.pressed & pad::START != 0 => {
                self.screen = Screen::Menu;
                self.idle = 0;
                Step::ToMenu
            }
            Screen::PressStart => Step::Stays,
            Screen::Menu => {
                self.cursor.step(pad.repeated);
                if pad.repeated & (pad::UP | pad::DOWN) != 0 {
                    self.idle = 0;
                }
                // The confirm is asked first; a cancel only counts without one.
                match (menu::confirmed(pad), Choice::of_line(self.cursor.at)) {
                    (true, Some(choice)) => {
                        self.idle = 0;
                        match choice {
                            Choice::StartGame => self.screen = Screen::CardCheck,
                            Choice::Options => self.screen = Screen::OptionsWait,
                            Choice::Continue => {
                                self.screen = Screen::Card;
                                (self.card_up, self.card_loaded) = (true, false);
                            }
                            Choice::ArenaBattleStage => {}
                        }
                        Step::Chosen(choice)
                    }
                    (false, _) if menu::cancelled(pad) => {
                        self.screen = Screen::PressStart;
                        self.idle = 0;
                        Step::ToPressStart
                    }
                    _ => Step::Stays,
                }
            }
            // The check and what follows it do not count the pass.
            Screen::CardCheck => {
                return match card_check(&self.slots) {
                    Some(warning) => {
                        self.screen = Screen::Warning(warning);
                        Step::Warns(warning)
                    }
                    None => {
                        self.screen = Screen::NoWarning;
                        Step::Stays
                    }
                };
            }
            Screen::Warning(_) => {
                return match menu::confirmed(pad) {
                    true => {
                        self.screen = Screen::Menu;
                        Step::NewGame
                    }
                    false => Step::Stays,
                };
            }
            Screen::NoWarning => {
                self.screen = Screen::Menu;
                return Step::NewGame;
            }
            Screen::WayOut => {
                self.way_out += 1;
                return match self.way_out >= WAY_OUT_PASSES {
                    true => Step::OpeningMovie,
                    false => Step::Stays,
                };
            }
            // None of the passes of "Options" asks a button or counts the pass. The
            // console waits here until the sprite pool is read; the build has it at
            // once.
            Screen::OptionsWait => {
                self.screen = Screen::Options;
                self.options_up = true;
                return Step::OptionsStart;
            }
            Screen::Options if self.options_up => return Step::Stays,
            Screen::Options => {
                self.screen = Screen::OptionsBack;
                return Step::OptionsOver;
            }
            Screen::OptionsBack => {
                self.screen = Screen::Menu;
                return Step::BackInMenu;
            }
            // The same routine steps the memory card's screen until it is gone: with a
            // loaded game the title leaves, with anything else the menu is back.
            Screen::Card if self.card_up => return Step::Stays,
            Screen::Card => {
                self.screen = Screen::Menu;
                return match self.card_loaded {
                    true => Step::Loaded,
                    false => Step::CardOver,
                };
            }
        };
        // Both screens count the pass behind whatever the buttons did.
        self.idle += 1;
        if self.idle > IDLE_PASSES {
            self.screen = Screen::WayOut;
            self.way_out = 0;
            return Step::GoesOut;
        }
        step
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pad::Repeat;

    /// A pad read once with nothing down, then with `held`.
    fn pad_with(held: u16) -> Pad {
        let mut pad = Pad::default();
        pad.read(0, Repeat::of(1), 0);
        pad.read(held, Repeat::of(1), 1);
        pad
    }

    /// The passes from a confirm on "Options" back to the menu, with an options screen
    /// that ends at once.
    fn through_the_options(title: &mut Title) {
        assert_eq!(title.screen, Screen::OptionsWait);
        assert_eq!(title.pass(&pad_with(0)), Step::OptionsStart);
        title.options_ended();
        assert_eq!(title.pass(&pad_with(0)), Step::OptionsOver);
        assert_eq!(title.pass(&pad_with(0)), Step::BackInMenu);
        assert_eq!(title.screen, Screen::Menu);
    }

    #[test]
    fn the_title_waits_for_the_options_screen_and_asks_no_button_meanwhile() {
        let mut title = Title::new();
        title.pass(&pad_with(pad::START));
        title.pass(&pad_with(pad::DOWN));
        assert_eq!(
            title.pass(&pad_with(pad::DOWN | pad::CROSS)),
            Step::Chosen(Choice::Options)
        );
        assert_eq!(title.screen, Screen::OptionsWait);
        // The pass behind the confirm starts the screen whatever is held.
        assert_eq!(title.pass(&pad_with(pad::TRIANGLE)), Step::OptionsStart);
        assert_eq!(title.screen, Screen::Options);
        // While it is up neither the cursor, a confirm nor a cancel is the title's,
        // no sound is, and the passes are not counted as idle ones.
        let buttons = pad_with(pad::UP | pad::CROSS | pad::TRIANGLE);
        assert_eq!(title.sounds(&buttons), []);
        for _ in 0..IDLE_PASSES + 10 {
            assert_eq!(title.pass(&buttons), Step::Stays);
        }
        assert_eq!((title.screen, title.cursor.at), (Screen::Options, 3));
        // The pass behind its end draws the menu, the next one shows it, and from the
        // third the menu listens, the cursor still on its line.
        title.options_ended();
        assert_eq!(title.pass(&buttons), Step::OptionsOver);
        assert_eq!(title.pass(&buttons), Step::BackInMenu);
        assert_eq!((title.screen, title.cursor.at), (Screen::Menu, 3));
        assert_eq!(title.sounds(&pad_with(pad::TRIANGLE)), [name::SOUND_CANCEL]);
        assert_eq!(title.pass(&pad_with(pad::TRIANGLE)), Step::ToPressStart);
    }

    #[test]
    fn start_leads_from_the_first_screen_to_the_menu() {
        let mut title = Title::new();
        assert_eq!(title.pass(&pad_with(0)), Step::Stays);
        assert_eq!(title.pass(&pad_with(pad::CROSS)), Step::Stays);
        assert_eq!(title.pass(&pad_with(pad::START)), Step::ToMenu);
        assert_eq!(title.screen, Screen::Menu);
        // In the menu Start is no button of the title's own.
        assert_eq!(title.pass(&pad_with(pad::START)), Step::Stays);
    }

    #[test]
    fn a_start_that_was_down_before_is_not_pressed() {
        let mut pad = Pad::default();
        pad.read(pad::START, Repeat::of(1), 0);
        pad.read(pad::START, Repeat::of(1), 1);
        let mut title = Title::new();
        assert_eq!(title.pass(&pad), Step::Stays);
        assert_eq!(title.screen, Screen::PressStart);
    }

    #[test]
    fn left_alone_the_title_goes_out_and_the_movie_plays_again() {
        let mut title = Title::new();
        let nothing = pad_with(0);
        for _ in 0..IDLE_PASSES {
            assert_eq!(title.pass(&nothing), Step::Stays);
        }
        assert_eq!(title.pass(&nothing), Step::GoesOut);
        assert_eq!(title.screen, Screen::WayOut);
        for _ in 1..WAY_OUT_PASSES {
            // No button stops the way out.
            assert_eq!(title.pass(&pad_with(pad::START)), Step::Stays);
        }
        assert_eq!(title.pass(&nothing), Step::OpeningMovie);
    }

    #[test]
    fn the_menu_counts_from_start_and_from_every_move_of_the_cursor() {
        let mut title = Title::new();
        let nothing = pad_with(0);
        for _ in 0..IDLE_PASSES - 1 {
            title.pass(&nothing);
        }
        // Start clears the count, and its own pass is the first one counted again.
        assert_eq!(title.pass(&pad_with(pad::START)), Step::ToMenu);
        for _ in 0..IDLE_PASSES - 2 {
            assert_eq!(title.pass(&nothing), Step::Stays);
        }
        assert_eq!(title.pass(&pad_with(pad::DOWN)), Step::Stays);
        for _ in 0..IDLE_PASSES - 1 {
            assert_eq!(title.pass(&nothing), Step::Stays);
        }
        assert_eq!(title.pass(&nothing), Step::GoesOut);
    }

    #[test]
    fn the_cursor_starts_on_the_second_line_and_a_confirm_chooses_its_line() {
        let mut title = Title::new();
        // On the first screen neither the cursor nor a confirm is asked.
        assert_eq!(title.pass(&pad_with(pad::DOWN | pad::CROSS)), Step::Stays);
        assert_eq!(title.cursor.at, FIRST_LINE);
        title.pass(&pad_with(pad::START));
        assert_eq!(
            title.pass(&pad_with(pad::CROSS)),
            Step::Chosen(Choice::Continue)
        );
        // The memory card's screen is up: no button is the title's until it has ended.
        assert_eq!(title.screen, Screen::Card);
        assert_eq!(title.pass(&pad_with(pad::UP | pad::CROSS)), Step::Stays);
        assert_eq!(title.cursor.at, FIRST_LINE);
        title.card_ended(false);
        assert_eq!(title.pass(&pad_with(pad::CROSS)), Step::CardOver);
        assert_eq!(title.screen, Screen::Menu);
        assert_eq!(title.pass(&pad_with(pad::UP)), Step::Stays);
        assert_eq!(title.cursor.at, 0);
        // Up from the first line is the last one; the cursor moves before the confirm.
        assert_eq!(
            title.pass(&pad_with(pad::UP | pad::CROSS)),
            Step::Chosen(Choice::Options)
        );
        through_the_options(&mut title);
        assert_eq!(title.pass(&pad_with(pad::UP)), Step::Stays);
        assert_eq!(
            title.pass(&pad_with(pad::L1)),
            Step::Chosen(Choice::ArenaBattleStage)
        );
        // A confirm wins over a cancel of the same pass, and the cursor keeps its line
        // over a visit to the first screen.
        assert_eq!(
            title.pass(&pad_with(pad::CROSS | pad::TRIANGLE)),
            Step::Chosen(Choice::ArenaBattleStage)
        );
        assert_eq!(title.pass(&pad_with(pad::TRIANGLE)), Step::ToPressStart);
        title.pass(&pad_with(pad::START));
        assert_eq!(title.cursor.at, 2);
    }

    #[test]
    fn start_a_confirm_a_move_and_a_cancel_have_their_sounds() {
        let (confirm, cancel, moved) = (name::SOUND_CONFIRM, name::SOUND_CANCEL, name::SOUND_MOVE);
        let mut title = Title::new();
        // On the first screen only Start sounds.
        assert_eq!(title.sounds(&pad_with(pad::DOWN | pad::CROSS)), []);
        assert_eq!(title.sounds(&pad_with(pad::START)), [confirm]);
        title.pass(&pad_with(pad::START));
        // In the menu the cursor comes first, and a cancel is silent beside a confirm.
        assert_eq!(title.sounds(&pad_with(pad::START)), []);
        assert_eq!(title.sounds(&pad_with(pad::UP)), [moved]);
        assert_eq!(title.sounds(&pad_with(pad::UP | pad::L1)), [moved, confirm]);
        assert_eq!(
            title.sounds(&pad_with(pad::CROSS | pad::TRIANGLE)),
            [confirm]
        );
        assert_eq!(title.sounds(&pad_with(pad::TRIANGLE)), [cancel]);
        assert_eq!(
            title.sounds(&pad_with(pad::DOWN | pad::L2)),
            [moved, cancel]
        );
        // Asking for the sounds changes nothing.
        assert_eq!(title.cursor.at, FIRST_LINE);
        // A warning waits for a confirm, which sounds; nothing else does there.
        title.screen = Screen::Warning(Warning::NoCard);
        assert_eq!(title.sounds(&pad_with(pad::CROSS)), [confirm]);
        assert_eq!(title.sounds(&pad_with(pad::DOWN | pad::TRIANGLE)), []);
        title.screen = Screen::CardCheck;
        assert_eq!(title.sounds(&pad_with(pad::CROSS)), []);
    }

    #[test]
    fn a_confirm_clears_the_count_of_idle_passes() {
        let mut title = Title::new();
        let nothing = pad_with(0);
        title.pass(&pad_with(pad::START));
        for _ in 0..IDLE_PASSES - 2 {
            assert_eq!(title.pass(&nothing), Step::Stays);
        }
        assert_eq!(
            title.pass(&pad_with(pad::CROSS)),
            Step::Chosen(Choice::Continue)
        );
        // The passes of the memory card's screen do not count, and a game it loaded
        // ends the title.
        assert_eq!(title.pass(&nothing), Step::Stays);
        let mut loads = title;
        loads.card_ended(true);
        assert_eq!(loads.pass(&nothing), Step::Loaded);
        title.card_ended(false);
        assert_eq!(title.pass(&nothing), Step::CardOver);
        for _ in 0..IDLE_PASSES - 1 {
            assert_eq!(title.pass(&nothing), Step::Stays);
        }
        assert_eq!(title.pass(&nothing), Step::GoesOut);
    }

    #[test]
    fn the_lines_of_the_menu_lie_under_each_other() {
        let tops: Vec<i16> = LINES.iter().map(|line| line.frame.y).collect();
        assert_eq!(tops, [84, 108, 132, 156]);
        assert!(LINES.iter().all(|line| line.frame.x == 186));
        let room = LINES[0].frame.inside(LINE_MARGIN);
        assert_eq!((room.y, room.h), (86, 12));
        // The flag word names kind 4, and only the cursor's frame pulses.
        let (style, pulses) = Style::of_line(MENU_FLAGS, true, 0);
        assert_eq!(style, PRESS_START_STYLE);
        assert!(pulses && !Style::of_line(MENU_FLAGS, false, 0).1);
    }

    /// A title in its menu with the cursor on "Start Game".
    fn on_start_game(slots: [Slot; 2]) -> Title {
        let mut title = Title::new();
        title.slots = slots;
        title.pass(&pad_with(pad::START));
        assert_eq!(title.pass(&pad_with(pad::UP)), Step::Stays);
        title
    }

    #[test]
    fn a_console_without_a_card_is_warned_before_the_new_game() {
        let mut title = on_start_game([Slot::Empty; 2]);
        assert_eq!(
            title.pass(&pad_with(pad::CROSS)),
            Step::Chosen(Choice::StartGame)
        );
        assert_eq!(title.screen, Screen::CardCheck);
        // The cross is still down in the pass of the check and does not end the warning.
        let mut held = pad_with(pad::CROSS);
        held.read(pad::CROSS, Repeat::of(1), 2);
        assert_eq!(title.pass(&held), Step::Warns(Warning::NoCard));
        assert_eq!(title.pass(&held), Step::Stays);
        // Neither the cursor nor a cancel is asked while the warning stands.
        assert_eq!(
            title.pass(&pad_with(pad::DOWN | pad::TRIANGLE)),
            Step::Stays
        );
        assert_eq!(title.screen, Screen::Warning(Warning::NoCard));
        assert_eq!(title.cursor.at, 0);
        assert_eq!(title.pass(&pad_with(pad::L1)), Step::NewGame);
        assert_eq!(title.screen, Screen::Menu);
    }

    #[test]
    fn a_warning_does_not_count_idle_passes() {
        let mut title = on_start_game([Slot::Empty; 2]);
        title.pass(&pad_with(pad::CROSS));
        let nothing = pad_with(0);
        assert_eq!(title.pass(&nothing), Step::Warns(Warning::NoCard));
        for _ in 0..IDLE_PASSES * 2 {
            assert_eq!(title.pass(&nothing), Step::Stays);
        }
        assert_eq!(title.screen, Screen::Warning(Warning::NoCard));
    }

    #[test]
    fn a_card_to_save_on_leads_to_the_new_game_without_a_warning() {
        let room = Slot::Card { free: 1, saves: 0 };
        let saved = Slot::Card { free: 0, saves: 2 };
        let full = Slot::Card { free: 0, saves: 0 };
        assert_eq!(card_check(&[room, Slot::Empty]), None);
        assert_eq!(card_check(&[Slot::Empty, saved]), None);
        assert_eq!(card_check(&[full, room]), None);
        assert_eq!(card_check(&[full, Slot::Empty]), Some(Warning::Full));
        assert_eq!(card_check(&[Slot::Empty, full]), Some(Warning::Full));
        assert_eq!(card_check(&[full, full]), Some(Warning::Full));
        assert_eq!(card_check(&[Slot::Empty; 2]), Some(Warning::NoCard));

        let mut title = on_start_game([Slot::Empty, saved]);
        let nothing = pad_with(0);
        title.pass(&pad_with(pad::CROSS));
        assert_eq!(title.pass(&nothing), Step::Stays);
        assert_eq!(title.screen, Screen::NoWarning);
        assert_eq!(title.pass(&nothing), Step::NewGame);

        let mut title = on_start_game([full, Slot::Empty]);
        title.pass(&pad_with(pad::CROSS));
        assert_eq!(title.pass(&nothing), Step::Warns(Warning::Full));
        assert_eq!(title.pass(&pad_with(pad::CROSS)), Step::NewGame);
    }

    #[test]
    fn blocks_of_a_card_are_counted_by_the_sizes_of_its_files() {
        assert_eq!(free_blocks(&[]), 15);
        assert_eq!(free_blocks(&[8192, 8193, 1, 0]), 15 - 1 - 2 - 1);
        assert_eq!(free_blocks(&[8192 * 15]), 0);
        // A card the library calls new or unformatted is read like any other.
        let answers: Vec<i32> = (-1..7).filter(|&result| card_answers(result)).collect();
        assert_eq!(answers, [0, 3, 4]);
    }

    #[test]
    fn the_lines_of_a_warning_end_at_the_same_height() {
        let last = |warning: Warning| warning.placed().last().map(|(_, at)| at);
        assert_eq!(last(Warning::NoCard), Some((16, 138)));
        assert_eq!(last(Warning::Full), Some((16, 138)));
        assert_eq!(Warning::NoCard.placed().count(), 4);
        assert_eq!(Warning::Full.placed().count(), 6);
        assert_eq!(Warning::Full.placed().next(), Some((0x800A_054C, (16, 58))));
    }

    #[test]
    fn a_cancel_in_the_menu_leads_back_to_the_first_screen() {
        let mut title = Title::new();
        title.pass(&pad_with(pad::START));
        assert_eq!(title.pass(&pad_with(pad::TRIANGLE)), Step::ToPressStart);
        assert_eq!(title.screen, Screen::PressStart);
        assert_eq!(title.pass(&pad_with(pad::START)), Step::ToMenu);
    }
}
