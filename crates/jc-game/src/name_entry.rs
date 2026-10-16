
use std::collections::VecDeque;

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::{ButtonState, InputSystems};
use bevy::prelude::*;
use jc_disc::{Archive, DiscImage, Executable, LocalConfig};
use jc_formats::menu::{self, Frames, Menu, Rect, Style};
use jc_formats::name::{self, Area, Entry, Framed, Page, Tables, Typed};
use jc_formats::pad::{Pad, Repeat};
use jc_formats::rooms;
use jc_formats::text::{self, Font, Fonts, Glyph};
use jc_formats::tim::{self, Tim};

use jc_formats::title;

use crate::movie_view::{Playing, ShowMovie};
use crate::music::Music;
use crate::pointer::{self, Aim, Mouse, Pointer};
use crate::prompts::{self, Laid};
use crate::title::Press;

/// The console's screen.
pub(crate) const SCREEN: (usize, usize) = (320, 240);
pub(crate) const WHOLE: Rect = Rect {
    x: 0,
    y: 0,
    w: SCREEN.0 as i16,
    h: SCREEN.1 as i16,
};
/// The longest stretch of time one pass of the app may stand for.
const LONGEST_PASS: f64 = 0.1;

type Failure = Box<dyn std::error::Error>;

/// `--name-entry`, with the buttons of `--press=SECONDS:BUTTON,...` for a hidden run. In
/// a run of the game the buttons are those of `--name-press=SECONDS:BUTTON,...`, counted
/// from the moment the screen opens.
#[derive(Resource)]
pub struct ShowNameEntry {
    pub presses: Vec<Press>,
}

/// The name the hero was given, for whatever shows it later.
#[derive(Resource)]
pub struct Named {
    /// The name as a text: what a message puts in for the name's unit.
    pub units: Vec<u16>,
    /// The name as the game's state keeps it.
    pub stored: [u8; name::LETTERS],
}

/// The last wait of the title's new-game task, behind its second movie: the screen is
/// black and the task counts its passes before it leaves for the field.
#[derive(Resource)]
pub struct LastWait {
    passes: u32,
    owed: f64,
}

/// An image in its own colours, four bytes a pixel.
pub(crate) struct Picture {
    pub(crate) rgba: Vec<u8>,
    pub(crate) wide: usize,
    pub(crate) high: usize,
}

impl Picture {
    pub(crate) fn of(image: &Tim) -> Result<Self, Failure> {
        Ok(Self {
            rgba: image.to_rgba(image.palette(0))?,
            wide: image.width as usize,
            high: image.height as usize,
        })
    }
}

/// The name entry while it is up.
#[derive(Resource)]
pub struct NameEntry {
    state: Entry,
    tables: Tables,
    /// The pages of the grid in codes of the font of the texts.
    pages: Vec<Page>,
    font: Font,
    small: Font,
    font_picture: Picture,
    small_picture: Picture,
    /// The menus' sprite pool.
    sprites: Vec<Picture>,
    frames: Frames,
    /// The picture behind the screen as the screen holds it.
    backdrop: Vec<u8>,
    pad: Option<Pad>,
    /// Passes of the screen's task so far, and the share of one that is owed.
    passes: u32,
    owed: f64,
    time: f64,
    presses: Vec<Press>,
    shown: Entity,
    /// The name was taken: the task sleeps and the screen stays as it was.
    over: bool,
    /// The window colour of the game's settings, in which the frames are drawn.
    pub(crate) colour: i16,
    /// The game frames a pass of the screen's task stands for, which the menus' time
    /// counts: 1 under the title, 2 over a room.
    game_frame: u32,
    /// The spot of the mouse on its way to the cursor (`pointer.rs`), the characters a
    /// keyboard types with what each one is, and the keys typed that wait for a pass.
    aim: Aim<name::Spot>,
    keyboard: Vec<(char, Typed)>,
    typed: VecDeque<Typed>,
}

/// Mixes a colour into a pixel of the screen as the console blends, in five bits a part.
pub(crate) fn blend(screen: &mut [u8], x: i16, y: i16, colour: [u8; 3], mode: Option<u8>) {
    let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) else {
        return;
    };
    if x >= SCREEN.0 || y >= SCREEN.1 {
        return;
    }
    let pixel = (y * SCREEN.0 + x) * 4;
    for (part, value) in colour.into_iter().enumerate() {
        let under = i32::from(screen[pixel + part] >> 3);
        let over = i32::from(value >> 3);
        let five = match mode {
            None => over,
            Some(0) => (under + over) >> 1,
            Some(1) => under + over,
            Some(2) => under - over,
            Some(_) => under + (over >> 2),
        }
        .clamp(0, 31) as u8;
        screen[pixel + part] = five << 3 | five >> 2;
    }
    screen[pixel + 3] = 255;
}

/// Puts a piece of a picture on the screen where it lies inside `clip`: every texel that
/// is drawn, as it is.
pub(crate) fn blit(
    screen: &mut [u8],
    picture: &Picture,
    from: (usize, usize),
    size: (usize, usize),
    at: (i16, i16),
    clip: Rect,
) {
    for (dx, dy) in (0..size.1).flat_map(|dy| (0..size.0).map(move |dx| (dx, dy))) {
        let (u, v) = (from.0 + dx, from.1 + dy);
        let (x, y) = (i32::from(at.0) + dx as i32, i32::from(at.1) + dy as i32);
        let inside = (i32::from(clip.x)..i32::from(clip.x) + i32::from(clip.w)).contains(&x)
            && (i32::from(clip.y)..i32::from(clip.y) + i32::from(clip.h)).contains(&y);
        if !inside || u >= picture.wide || v >= picture.high {
            continue;
        }
        let texel = (v * picture.wide + u) * 4;
        if picture.rgba[texel + 3] == 0 {
            continue;
        }
        let colour = [0, 1, 2].map(|part| picture.rgba[texel + part]);
        blend(screen, x as i16, y as i16, colour, None);
    }
}

/// The letter of a unit for the log: the capitals, small letters and digits of the
/// game's texts, a space, a sign a keyboard has, or a question mark for another sign.
pub(crate) fn letter(unit: u16) -> char {
    name::key_of(unit).unwrap_or('?')
}

impl NameEntry {
    /// The name being written, in letters.
    fn name(&self) -> String {
        (self.font.units(self.state.name()).into_iter())
            .map(letter)
            .collect()
    }

    fn sprite(&self, screen: &mut [u8], number: usize, at: (i16, i16), clip: Rect) {
        if let Some(sprite) = self.sprites.get(number) {
            blit(screen, sprite, (0, 0), (sprite.wide, sprite.high), at, clip);
        }
    }

    fn glyphs(&self, screen: &mut [u8], small: bool, glyphs: &[Glyph], clip: Rect) {
        let (font, picture) = match small {
            true => (&self.small, &self.small_picture),
            false => (&self.font, &self.font_picture),
        };
        let cell = (usize::from(font.cell.0), usize::from(font.cell.1));
        for glyph in glyphs {
            if let Some(from) = glyph.cell {
                let from = (usize::from(from.0), usize::from(from.1));
                blit(screen, picture, from, cell, glyph.at, clip);
            }
        }
    }

    /// A frame of the menus over what the screen holds.
    fn frame(&self, screen: &mut [u8], rect: Rect, style: Style, time: u32) {
        let Some(colour) = self.frames.colour(style, time) else {
            return;
        };
        let quads = self.frames.quads(rect, Some(colour));
        for y in rect.y..rect.y.saturating_add(rect.h.max(0)) {
            for x in rect.x..rect.x.saturating_add(rect.w.max(0)) {
                if let Some(quad) = quads.iter().find(|quad| quad.covers(x, y)) {
                    blend(screen, x, y, quad.colour, style.blend_mode());
                }
            }
        }
    }

    fn framed(&self, screen: &mut [u8], framed: Framed, time: u32) {
        let time = if framed.pulses { time } else { 0 };
        self.frame(screen, framed.rect, framed.style, time);
    }

    /// A menu as the shared drawer draws it: for every piece its frame, then its content
    /// cut off at the frame's rectangle.
    fn menu(&self, screen: &mut [u8], menu: &Menu, flags: u32, cursor: i16, time: u32) {
        for (line, pieces) in menu.lines.iter().enumerate() {
            let on_cursor = usize::try_from(cursor) == Ok(line);
            for piece in pieces {
                if piece.flags & menu::piece::BARE == 0 {
                    let (style, pulses) = Style::of_line(flags, on_cursor, self.colour);
                    self.frame(screen, piece.rect, style, if pulses { time } else { 0 });
                }
                if piece.flags & menu::piece::EMPTY != 0 {
                    continue;
                }
                let room = piece.rect.inside(piece.margin);
                match piece.kind {
                    menu::kind::PICTURE => {
                        let number = piece.value as usize;
                        let Some(sprite) = self.sprites.get(number) else {
                            continue;
                        };
                        let at = menu::placed(piece.flags, room, sprite.wide as i16);
                        self.sprite(screen, number, at, piece.rect);
                    }
                    menu::kind::SMALL_TEXT => {
                        let Some(units) = self.tables.text(piece.value) else {
                            continue;
                        };
                        let codes = self.small.codes(&units);
                        let (x, y) = menu::placed(piece.flags, room, self.small.length(&codes));
                        let glyphs = self.small.glyphs(&codes, (x, y.wrapping_add(2)));
                        self.glyphs(screen, true, &glyphs, piece.rect);
                    }
                    _ => {}
                }
            }
        }
    }

    /// One pass of the screen's task with the pad's words: its sounds are asked for,
    /// and once the name is taken it is answered as the game's state keeps it.
    pub(crate) fn pass(&mut self, pad: &Pad, music: &mut Music) -> Option<[u8; name::LETTERS]> {
        self.passes += 1;
        let passes = self.passes;
        let before = self.state.clone();
        // A key typed on the keyboard is the pass's input in place of the pad's; else
        // the mouse steers the cursor with the pad's own presses.
        let pass = match self.typed.pop_front() {
            Some(typed) => {
                self.aim.forget();
                info!("the name entry: pass {passes}: typed {typed:?}");
                self.state.typed(typed, &self.pages)
            }
            None => {
                let mut pad = *pad;
                let state = &self.state;
                (self.aim).steer(&mut pad, state.spot(), |spot| state.way_to(spot));
                self.state.pass(&pad, &self.pages)
            }
        };
        for &sound in &pass.sounds {
            music.sound(sound.into());
        }
        let state = &self.state;
        if *state != before {
            info!(
                "the name entry: pass {passes}: {:?} {} on page {}, the name {:?}, sounds {:?}",
                state.area,
                state.cursor,
                state.page,
                self.name(),
                pass.sounds
            );
        }
        if !pass.taken {
            return None;
        }
        let stored = state.stored(&self.font, &self.small);
        info!(
            "the name entry: the name {:?} is taken after {passes} passes, kept as {stored:?}",
            self.name()
        );
        self.over = true;
        Some(stored)
    }

    /// Once a frame: the mouse over the spots of the screen, at a pixel of it.
    pub(crate) fn look(&mut self, mouse: &Mouse, at: Option<(i16, i16)>) {
        let spots = name::spots(&self.state, &self.tables);
        self.aim.look(mouse, pointer::row_under(spots, at));
    }

    /// Keys typed on the keyboard, for the passes to come: a character the grid does
    /// not have is left out.
    pub(crate) fn type_keys(&mut self, keys: impl IntoIterator<Item = Typing>) {
        for key in keys {
            let typed = match key {
                Typing::Delete => Some(Typed::Delete),
                Typing::Done => Some(Typed::Done),
                Typing::Paste => None,
                Typing::Character(typed) => (self.keyboard.iter())
                    .find(|(key, _)| *key == typed)
                    .map(|(_, letter)| *letter),
            };
            match typed {
                Some(typed) => self.typed.push_back(typed),
                None => info!("the name entry: the grid has no {key:?}"),
            }
        }
    }

    /// The screen of this pass, four bytes a pixel. The slots of the ordering table are
    /// drawn from the last to the first, and in a slot what was added last comes first.
    pub(crate) fn paint(&self) -> Vec<u8> {
        let mut screen = self.backdrop.clone();
        let state = &self.state;
        let time = menu::frame_time(self.passes, self.game_frame);
        let name = state.name();
        let kind = self.colour;
        // Slot 14: the frames.
        self.framed(&mut screen, name::grid(kind), time);
        let next = name::next_letter(&self.font, name, kind);
        self.framed(&mut screen, next, time);
        self.framed(&mut screen, name::name_box(kind), time);
        if state.area == Area::Grid {
            self.framed(&mut screen, name::grid_cursor(state.cursor, kind), time);
        }
        // Slot 1: the legend on its frame, and what the name's box holds.
        self.framed(&mut screen, name::legend(kind), time);
        let sizes = |number: usize| {
            let sprite = self.sprites.get(number)?;
            Some((sprite.wide as i16, sprite.high as i16))
        };
        for piece in prompts::legend(&self.tables.legend, &self.font, sizes, true) {
            match piece {
                Laid::Sprite { sprite, at } => self.sprite(&mut screen, sprite, at, WHOLE),
                Laid::Glyph { glyph, at } => prompts::put(&mut screen, &glyph, at, WHOLE),
                Laid::Word { units, at } => {
                    let glyphs = self.font.glyphs(&self.font.codes(&units), at);
                    self.glyphs(&mut screen, false, &glyphs, WHOLE);
                }
            }
        }
        let dash = self.font.codes(&self.tables.dash);
        for at in name::dashes(&self.font, name, state.letters()) {
            self.glyphs(&mut screen, false, &self.font.glyphs(&dash, at), WHOLE);
        }
        let label = name::LABEL;
        self.sprite(&mut screen, label.image, (label.x, label.y), WHOLE);
        let glyphs = self.font.glyphs(name, name::NAME_AT);
        self.glyphs(&mut screen, false, &glyphs, WHOLE);
        // Slot 0: the two menus and the letters of the grid.
        let [pages, commands] = name::menu_looks(state.area);
        let runs = |runs: bool| if runs { time } else { 0 };
        let menu = &self.tables.page_menu;
        self.menu(&mut screen, menu, pages.0, state.page, runs(pages.1));
        let cursor = match state.area {
            Area::Commands => state.cursor,
            _ => -1,
        };
        let menu = &self.tables.command_menu;
        self.menu(&mut screen, menu, commands.0, cursor, runs(commands.1));
        let page = usize::try_from(state.page).ok();
        let rows =
            page.and_then(|page| Some((self.tables.pages.get(page)?, self.pages.get(page)?)));
        if let Some((rows, codes)) = rows {
            for (row, codes) in rows.iter().zip(codes) {
                let glyphs = name::row_glyphs(&self.font, codes, row.at);
                self.glyphs(&mut screen, false, &glyphs, WHOLE);
            }
        }
        screen
    }
}

/// Reads what the screen needs from the disc and puts it up as its start leaves it.
fn open(presses: Vec<Press>, shown: Entity) -> Result<NameEntry, Failure> {
    let config = LocalConfig::load()?;
    let mut disc = DiscImage::open(&config.disc_image)?;
    let archive = Archive::open(&mut disc)?;
    let exe = Executable::read(&mut disc)?;
    let fonts = Fonts::from_executable(&exe.image, exe.base)
        .ok_or("the executable does not hold the font tables")?;
    let frames = Frames::from_executable(&exe.image, exe.base)
        .ok_or("the executable does not hold the tables of the frames")?;
    let font = (fonts.get(text::TEXT_FONT).cloned()).ok_or("there is no font of the texts")?;
    let small = (fonts.get(text::SMALL_FONT).cloned()).ok_or("there is no small font")?;
    let mut set = |file: usize| -> Result<Vec<Tim>, Failure> {
        let bytes = archive.read(&mut disc, file)?;
        Ok(tim::texture_set(&bytes).ok_or("the file is not a texture set")?)
    };
    let font_set = set(text::FONT_SET)?;
    let font_picture = |font: &Font| -> Result<Picture, Failure> {
        Picture::of(
            font_set
                .get(font.image)
                .ok_or("the set has no such image")?,
        )
    };
    let (font_picture, small_picture) = (font_picture(&font)?, font_picture(&small)?);
    let sprites = (set(name::SPRITES)?.iter())
        .map(Picture::of)
        .collect::<Result<Vec<_>, _>>()?;
    let pictures = set(name::PICTURES)?;
    let mut backdrop = [0, 0, 0, 255].repeat(SCREEN.0 * SCREEN.1);
    for half in name::BACKGROUND {
        let image = pictures
            .get(half.image)
            .ok_or("the set has no such image")?;
        let picture = Picture::of(image)?;
        let size = (picture.wide, picture.high);
        blit(
            &mut backdrop,
            &picture,
            (0, 0),
            size,
            (half.x, half.y),
            WHOLE,
        );
    }
    let overlay = archive.read(&mut disc, name::OVERLAY)?;
    let tables = Tables::from_overlay(&overlay).ok_or("the file does not hold the records")?;
    let (file, address) = name::DEFAULT_NAME;
    let title = archive.read(&mut disc, file)?;
    let default = (address.checked_sub(rooms::load_address(file)))
        .and_then(|offset| title.get(offset as usize..))
        .map(text::units)
        .ok_or("the default name is not in its file")?;
    info!(
        "the name entry: {} pages in file {}, {} sprites in file {}",
        tables.pages.len(),
        name::OVERLAY,
        sprites.len(),
        name::SPRITES
    );
    Ok(NameEntry {
        colour: 0,
        state: Entry::new(&font.codes(&default), 0),
        pages: tables.letters(&font),
        keyboard: name::keyboard(&tables),
        typed: VecDeque::new(),
        aim: Aim::default(),
        tables,
        font,
        small,
        font_picture,
        small_picture,
        sprites,
        frames,
        backdrop,
        pad: None,
        passes: 0,
        owed: 0.0,
        time: 0.0,
        presses,
        shown,
        over: false,
        game_frame: 1,
    })
}

pub(crate) fn open_on(
    stored: &[u8; name::LETTERS],
    wall: Vec<u8>,
    game_frame: u32,
) -> Result<NameEntry, Failure> {
    let mut entry = open(Vec::new(), Entity::PLACEHOLDER)?;
    let codes = entry.font.codes(&name::units(stored, &entry.small));
    entry.state = Entry::new(&codes, name::mode::OVER_THE_WALL);
    entry.backdrop = wall;
    entry.game_frame = game_frame;
    Ok(entry)
}

/// A key typed on a name entry, before the screen has made a letter of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Typing {
    Character(char),
    /// Backspace.
    Delete,
    /// Enter.
    Done,
    /// Ctrl+V: the lobby's field of the room code takes the clipboard.
    Paste,
}

/// What a key that went down types: Backspace and Enter (not with Alt, which is the
/// switch of `display.rs`), else the characters of its text.
pub fn typing(key: &Key, text: Option<&str>, alt: bool) -> Vec<Typing> {
    match key {
        Key::Backspace => vec![Typing::Delete],
        Key::Enter if alt => Vec::new(),
        Key::Enter => vec![Typing::Done],
        Key::Character(typed) => {
            let text = text.unwrap_or(typed.as_str());
            (text.chars())
                .filter(|letter| !letter.is_control())
                .map(Typing::Character)
                .collect()
        }
        Key::Space => vec![Typing::Character(' ')],
        _ => Vec::new(),
    }
}

/// Whether a key types on a name entry, so that it is not a button of the pad there:
/// the letters, the digits, Space, the keys of signs and Backspace. The arrow keys,
/// Enter, Esc, Tab and the keys that change nothing are not.
pub fn typing_keys(key: KeyCode) -> bool {
    use KeyCode::*;
    let letter = matches!(
        key,
        KeyA | KeyB
            | KeyC
            | KeyD
            | KeyE
            | KeyF
            | KeyG
            | KeyH
            | KeyI
            | KeyJ
            | KeyK
            | KeyL
            | KeyM
            | KeyN
            | KeyO
            | KeyP
            | KeyQ
            | KeyR
            | KeyS
            | KeyT
            | KeyU
            | KeyV
            | KeyW
            | KeyX
            | KeyY
            | KeyZ
    );
    let digit = matches!(
        key,
        Digit0
            | Digit1
            | Digit2
            | Digit3
            | Digit4
            | Digit5
            | Digit6
            | Digit7
            | Digit8
            | Digit9
            | Numpad0
            | Numpad1
            | Numpad2
            | Numpad3
            | Numpad4
            | Numpad5
            | Numpad6
            | Numpad7
            | Numpad8
            | Numpad9
    );
    let sign = matches!(
        key,
        Space
            | Backspace
            | Backquote
            | Backslash
            | BracketLeft
            | BracketRight
            | Comma
            | Equal
            | IntlBackslash
            | IntlRo
            | IntlYen
            | Minus
            | Period
            | Quote
            | Semicolon
            | Slash
            | NumpadAdd
            | NumpadComma
            | NumpadDecimal
            | NumpadDivide
            | NumpadMultiply
            | NumpadSubtract
    );
    letter || digit || sign
}

/// Whether a name entry is up and takes the keyboard: the one of a new game, the one of
/// the screen of the caught, or the lobby's (its name entry and its field of the
/// address).
pub fn naming(
    entry: Option<&NameEntry>,
    merge: Option<&crate::merge_view::MergeView>,
    lobby: Option<&crate::lobby::Lobby>,
) -> bool {
    entry.is_some_and(|entry| !entry.over)
        || merge.is_some_and(|merge| merge.names())
        || lobby.is_some_and(|lobby| lobby.types())
}

/// The keyboard on a name entry, and the typing of a check run.
pub struct TypingPlugin;

impl Plugin for TypingPlugin {
    fn build(&self, app: &mut App) {
        // `--type=SECONDS:TEXT;...`: `<` is Backspace, `>` Enter and `^` Ctrl+V.
        let checked = (std::env::args())
            .find_map(|arg| arg.strip_prefix("--type=").map(TypedForACheck::of))
            .unwrap_or_default();
        app.insert_resource(checked)
            .add_systems(PreUpdate, type_name.after(InputSystems));
    }
}

/// What a check run types, by the second of the run from which on it is typed.
#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct TypedForACheck(Vec<(f32, Vec<Typing>)>);

impl TypedForACheck {
    fn of(value: &str) -> Self {
        let typed = (value.split(';'))
            .filter_map(|part| {
                let (second, text) = part.split_once(':')?;
                let Ok(second) = second.trim().parse::<f32>() else {
                    warn!("--type: {part:?} is not SECONDS:TEXT");
                    return None;
                };
                let keys = (text.chars())
                    .map(|letter| match letter {
                        '<' => Typing::Delete,
                        '>' => Typing::Done,
                        '^' => Typing::Paste,
                        letter => Typing::Character(letter),
                    })
                    .collect();
                Some((second, keys))
            })
            .collect();
        Self(typed)
    }
}

/// The keys typed in this frame go to the name entry that is up.
fn type_name(
    mut keys: MessageReader<KeyboardInput>,
    held: Res<ButtonInput<KeyCode>>,
    time: Res<Time<Real>>,
    mut checked: ResMut<TypedForACheck>,
    entry: Option<ResMut<NameEntry>>,
    merge: Option<ResMut<crate::merge_view::MergeView>>,
    lobby: Option<ResMut<crate::lobby::Lobby>>,
) {
    let alt = held.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);
    let ctrl = held.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let mut typed: Vec<Typing> = (keys.read())
        .filter(|key| key.state == ButtonState::Pressed)
        .flat_map(|key| match ctrl && key.key_code == KeyCode::KeyV {
            true => vec![Typing::Paste],
            false => typing(&key.logical_key, key.text.as_deref(), alt),
        })
        .collect();
    let (mut entry, mut merge, mut lobby) = (entry, merge, lobby);
    // A check's text is typed once a name entry is up, from its second on.
    let mut due = |typed: &mut Vec<Typing>| {
        let second = time.elapsed_secs();
        let due = (checked.0.iter()).position(|(from, _)| *from <= second);
        if let Some(due) = due {
            typed.extend(checked.0.remove(due).1);
        }
    };
    if let Some(entry) = entry.as_deref_mut().filter(|entry| !entry.over) {
        due(&mut typed);
        if !typed.is_empty() {
            entry.type_keys(typed);
        }
        return;
    }
    if let Some(naming) = (merge.as_deref_mut()).and_then(crate::merge_view::MergeView::naming_mut)
    {
        due(&mut typed);
        if !typed.is_empty() {
            naming.type_keys(typed);
        }
        return;
    }
    // The lobby's name entry, or its field of the address.
    if let Some(lobby) = (lobby.as_deref_mut()).filter(|lobby| lobby.types()) {
        due(&mut typed);
        if typed.is_empty() {
            return;
        }
        match lobby.naming_mut() {
            Some(naming) => naming.type_keys(typed),
            None => lobby.type_keys(typed),
        }
    }
}

/// `--name-entry`: the screen alone.
pub fn show_name_entry(
    show: Res<ShowNameEntry>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    commands.spawn(Camera2d);
    let (wide, high) = (SCREEN.0 as u32, SCREEN.1 as u32);
    let black = [0, 0, 0, 255].repeat(SCREEN.0 * SCREEN.1);
    let shown = crate::spawn_picture(&mut commands, &mut images, wide, high, black);
    match open(show.presses.clone(), shown) {
        Ok(entry) => {
            info!(
                "the name entry: starts on the name {:?}, {} letters",
                entry.name(),
                entry.state.letters()
            );
            commands.insert_resource(entry);
        }
        Err(err) => {
            error!("the name entry: {err}");
            exit.write(AppExit::error());
        }
    }
}

/// The pad's word of held buttons from the keyboard, or from the presses of a hidden run.
fn buttons(current: &NameEntry, keys: &crate::keys::Pad) -> u16 {
    let mut held = crate::keys::held(keys);
    for press in &current.presses {
        if press.holds(current.time) {
            held |= press.button;
        }
    }
    held
}

/// The steps of the title's new-game task around the screen, in a run of the game: opens
/// the screen when the first movie of a new game is over, and counts the task's last
/// wait when the second one is.
#[allow(clippy::too_many_arguments)]
fn new_game(
    playing: Option<Res<Playing>>,
    wait: Option<ResMut<LastWait>>,
    show: Option<Res<ShowNameEntry>>,
    named: Option<Res<Named>>,
    second: Option<&crate::saving::SecondRound>,
    music: &mut Music,
    time: &Time<Virtual>,
    images: &mut Assets<Image>,
    commands: &mut Commands,
    exit: &mut MessageWriter<AppExit>,
) {
    if let Some(mut wait) = wait {
        wait.owed += crate::pace::passed(time, LONGEST_PASS) * f64::from(title::PASSES_A_SECOND);
        while wait.owed >= 1.0 && wait.passes < title::LAST_WAIT {
            wait.owed -= 1.0;
            wait.passes += 1;
        }
        if wait.passes == title::LAST_WAIT {
            let named_of = named.map(|named| (named.units.clone(), named.stored));
            let name = (named_of.as_ref()).map(|(units, stored)| (units.len(), *stored));
            info!(
                "the new game: the title's task leaves for the field after its wait of {} passes, with the name (letters, as the state keeps it) {name:?}; the field starts with the chapter screen",
                title::LAST_WAIT
            );
            music.give_back();
            commands.remove_resource::<LastWait>();
            if let Some(second) = second {
                let (units, stored) = named_of.unwrap_or((Vec::new(), second.beside.name));
                info!(
                    "the second round: the field starts in room {}",
                    title::SECOND_ROUND_ROOM
                );
                commands.insert_resource(second.starts(units, stored));
                commands.remove_resource::<crate::saving::SecondRound>();
                return;
            }
            commands.insert_resource(crate::Game::default());
            commands.insert_resource(crate::chapter::StartChapter(0));
        }
        return;
    }
    let Some(playing) = playing else {
        return;
    };
    if playing.leads_to_the_field() {
        // Step 6 of the new-game task fades the title's music behind its movie, and
        // step 7 gives it back behind the last wait.
        music.fade();
        commands.remove_resource::<Playing>();
        commands.insert_resource(LastWait {
            passes: 0,
            owed: 0.0,
        });
    } else if playing.leads_to_the_name_entry() {
        commands.remove_resource::<Playing>();
        let presses = show.map(|show| show.presses.clone()).unwrap_or_default();
        let (wide, high) = (SCREEN.0 as u32, SCREEN.1 as u32);
        let black = [0, 0, 0, 255].repeat(SCREEN.0 * SCREEN.1);
        let shown = crate::spawn_picture(commands, images, wide, high, black);
        // A second round keeps the name the game has: the default one is not copied.
        let opened = open(presses, shown).map(|mut entry| {
            if let Some(second) = second {
                let kept = name::units(&second.beside.name, &entry.small);
                entry.state = Entry::new(&entry.font.codes(&kept), 0);
            }
            entry
        });
        match opened {
            Ok(entry) => {
                info!(
                    "the name entry: opens behind the movie on the name {:?}",
                    entry.name()
                );
                commands.insert_resource(entry);
            }
            Err(err) => {
                error!("the name entry: {err}");
                exit.write(AppExit::error());
            }
        }
    }
}

/// Runs the passes of the screen for the time gone by and paints the last one. In a run
/// of the game (there is a movie to play) the screen opens behind the first movie of a
/// new game and the second one plays when the name is taken.
#[allow(clippy::too_many_arguments)]
pub fn run_name_entry(
    current: Option<ResMut<NameEntry>>,
    playing: Option<Res<Playing>>,
    movie: Option<Res<ShowMovie>>,
    wait: Option<ResMut<LastWait>>,
    show: Option<Res<ShowNameEntry>>,
    named: Option<Res<Named>>,
    second: Option<Res<crate::saving::SecondRound>>,
    mut music: ResMut<Music>,
    settings: Res<crate::settings::Settings>,
    time: Res<Time<Virtual>>,
    keys: Res<crate::keys::Pad>,
    mut images: ResMut<Assets<Image>>,
    mut sprites: Query<&mut Sprite>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
    pointer: Pointer,
) {
    let Some(mut current) = current else {
        new_game(
            playing,
            wait,
            show,
            named,
            second.as_deref(),
            &mut music,
            &time,
            &mut images,
            &mut commands,
            &mut exit,
        );
        return;
    };
    let colour = settings.0.colour as i16;
    if current.colour != colour {
        current.colour = colour;
    }
    if current.over {
        return;
    }
    let passed = crate::pace::passed(&time, LONGEST_PASS);
    current.time += passed;
    current.owed += passed * f64::from(name::PASSES_A_SECOND);
    let held = buttons(&current, &keys);
    let under = pointer.on_picture(current.shown, SCREEN);
    current.look(pointer.mouse(), under);
    // The title's task clears the word of pressed buttons before it starts the screen,
    // and the pad routine has run in every pass before: a button that is still down from
    // the movie did not go down on the screen.
    current.pad.get_or_insert_with(|| {
        let mut pad = Pad::default();
        pad.read(held, Repeat::of(1), 0);
        pad
    });
    let mut painted = current.passes == 0;
    while current.owed >= 1.0 && !current.over {
        current.owed -= 1.0;
        painted = true;
        let mut pad = current.pad.unwrap_or_default();
        pad.read(held, Repeat::of(1), current.passes + 1);
        current.pad = Some(pad);
        current.pass(&pad, &mut music);
    }
    if current.over {
        commands.insert_resource(Named {
            units: current.font.units(current.state.name()),
            stored: current.state.stored(&current.font, &current.small),
        });
        if let Some(movie) = movie {
            // The second movie of the new game: its player clears the screen, and the
            // task frees the pictures behind it.
            let (next, number) = match second {
                Some(_) => (movie.named_again(), title::NAMED_AGAIN_MOVIE),
                None => (movie.named(), title::NAMED_MOVIE),
            };
            info!("the new game: the hero has his name, movie {number} plays");
            commands.entity(current.shown).despawn();
            commands.remove_resource::<NameEntry>();
            match Playing::start(&next) {
                Ok(playing) => commands.insert_resource(playing),
                Err(err) => {
                    error!("the movie behind the name entry: {err}");
                    exit.write(AppExit::error());
                }
            }
            return;
        }
    }
    if !painted {
        return;
    }
    if let Ok(mut sprite) = sprites.get_mut(current.shown) {
        let (wide, high) = (SCREEN.0 as u32, SCREEN.1 as u32);
        sprite.image = images.add(crate::picture_image(wide, high, current.paint()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_types_its_characters_backspace_deletes_and_enter_takes_the_name() {
        let letter = Key::Character("a".into());
        assert_eq!(typing(&letter, Some("A"), false), [Typing::Character('A')]);
        assert_eq!(typing(&letter, None, false), [Typing::Character('a')]);
        assert_eq!(
            typing(&Key::Backspace, Some("\u{8}"), false),
            [Typing::Delete]
        );
        assert_eq!(typing(&Key::Enter, Some("\r"), false), [Typing::Done]);
        // Alt and Enter is the switch of the full screen.
        assert!(typing(&Key::Enter, Some("\r"), true).is_empty());
        assert_eq!(
            typing(&Key::Space, Some(" "), false),
            [Typing::Character(' ')]
        );
        assert!(typing(&Key::ArrowLeft, None, false).is_empty());
        assert!(typing(&Key::Character("\t".into()), Some("\t"), false).is_empty());
    }

    #[test]
    fn the_keys_that_type_are_no_buttons_on_a_name_entry() {
        use KeyCode::*;
        let typing = [
            KeyW, KeyA, KeyS, KeyD, KeyF, KeyK, KeyX, KeyI, KeyJ, KeyL, KeyR, KeyC, KeyQ, KeyE,
            KeyZ, KeyV, KeyU, KeyO, Space, Backspace, Digit7, Numpad3, Period, Comma, Minus,
        ];
        assert!(typing.into_iter().all(typing_keys));
        let buttons = [
            ArrowUp,
            ArrowDown,
            ArrowLeft,
            ArrowRight,
            Enter,
            NumpadEnter,
            Escape,
            Tab,
            ShiftLeft,
            ControlLeft,
            CapsLock,
            F11,
        ];
        assert!(!buttons.into_iter().any(typing_keys));
        // Only while a name entry is up.
        assert!(!naming(None, None, None));
    }

    #[test]
    fn a_check_types_its_text_with_backspace_and_enter() {
        let typed = TypedForACheck::of("1.5:Ab<;3:>;x:y");
        assert_eq!(
            typed.0,
            [
                (
                    1.5,
                    vec![
                        Typing::Character('A'),
                        Typing::Character('b'),
                        Typing::Delete
                    ]
                ),
                (3.0, vec![Typing::Done]),
            ]
        );
    }
}
