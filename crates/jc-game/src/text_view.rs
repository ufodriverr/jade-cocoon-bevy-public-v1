
use bevy::prelude::*;
use jc_disc::{Archive, DiscImage, Executable, LocalConfig};
use jc_formats::field::RoomData;
use jc_formats::menu::{Frames, Rect};
use jc_formats::message::{self, Group, Letter};
use jc_formats::model::ModelTable;
use jc_formats::notice;
use jc_formats::pad::{self, Pad, Repeat};
use jc_formats::rooms::{self, RoomTable};
use jc_formats::scene::ScenePackage;
use jc_formats::text::{self, Font, Fonts};
use jc_formats::tim;
use jc_formats::window::{self, Faces, Layout, Way, Where, Window};

use crate::name_entry::Named;
use crate::shown::Things;

/// The console's screen.
const SCREEN: (usize, usize) = (320, 240);
/// Where a text of `--text` starts: the place of the first line of the title's shorter
/// warning.
const START: (i16, i16) = (16, 90);
/// The pass of the sign's pulse a still is taken at: the picture as it is.
const MORE_PASS: u32 = 8;

type Failure = Box<dyn std::error::Error>;

#[derive(Resource)]
pub struct ShowText {
    /// The archive file, or `None` for the executable.
    file: Option<usize>,
    address: u32,
    font: usize,
    row: u8,
}

impl ShowText {
    pub fn parse(value: &str) -> Option<Self> {
        let mut parts = value.split(':');
        let file = match parts.next()? {
            "exe" => None,
            file => Some(file.parse().ok()?),
        };
        let address = parts.next()?;
        let address = address.strip_prefix("0x").unwrap_or(address);
        Some(Self {
            file,
            address: u32::from_str_radix(address, 16).ok()?,
            font: match parts.next() {
                Some(part) => part.parse().ok()?,
                None => text::TEXT_FONT,
            },
            row: match parts.next() {
                Some(part) => part.parse().ok()?,
                None => 0,
            },
        })
    }
}

/// `--says=ROOM:GROUP[:PAGE[:LETTERS[:NAME]]]`: page 0, all its letters and no name when
/// left out.
#[derive(Resource)]
pub struct ShowPage {
    room: u16,
    group: usize,
    page: usize,
    letters: u32,
    /// The units of the name that is put in for the player's.
    name: Vec<u16>,
    /// The model file whose face is in the picture's box.
    face: Option<usize>,
    /// How far the boxes are open, in passes of eight.
    open: i32,
    /// The window runs from its opening instead of standing with one page.
    runs: Option<Runs>,
}

/// `--talk[=PASS]` beside `--says`: the window runs as a script's message command has it
/// run, a pass a game frame, from its opening to its end; with a pass it stands as that
/// pass leaves it. The cross (the X key) turns a page and picks an answer, the triangle
/// (backspace) cancels, the arrow keys move the cursor. `--keys=PASS:BUTTON,...` holds a
/// button down in one pass in place of the keyboard, `--speaker=NAME` stands in for the
/// speaker's name, which the game takes from the room's actors, `--speed=N` is the speed
/// of the game's settings (0 to 3) in place of the one the game starts with and
/// `--answers` shows the group as a question: the window at the top with the answers'
/// box, as the commands `0x39` and `0x3A` open it.
pub struct Runs {
    stand: Option<u32>,
    keys: Vec<(u32, u16)>,
    speaker: Vec<u16>,
    speed: Option<u32>,
    answers: bool,
}

impl Runs {
    pub fn parse(
        stand: Option<&str>,
        keys: Option<&str>,
        speaker: Option<&str>,
        speed: Option<&str>,
        answers: bool,
    ) -> Option<Self> {
        let keys = match keys {
            Some(keys) => (keys.split(','))
                .map(|key| {
                    let (pass, button) = key.split_once(':')?;
                    let button = match button {
                        "cross" => pad::CROSS,
                        "triangle" => pad::TRIANGLE,
                        "up" => pad::UP,
                        "down" => pad::DOWN,
                        "left" => pad::LEFT,
                        "right" => pad::RIGHT,
                        _ => return None,
                    };
                    Some((pass.parse().ok()?, button))
                })
                .collect::<Option<_>>()?,
            None => Vec::new(),
        };
        Some(Self {
            stand: match stand {
                Some(pass) => Some(pass.parse().ok()?),
                None => None,
            },
            keys,
            speaker: match speaker {
                Some(name) => name_units(name)?,
                None => Vec::new(),
            },
            speed: match speed {
                Some(speed) => Some(speed.parse().ok().filter(|speed| *speed < 4)?),
                None => None,
            },
            answers,
        })
    }

    /// The game's setting word: the one the boot leaves, with the speed asked for.
    fn setting(&self) -> u32 {
        match self.speed {
            Some(speed) => window::SETTING_AT_START & !7 | speed,
            None => window::SETTING_AT_START,
        }
    }
}

#[derive(Resource)]
pub struct SaysInRoom {
    room: u16,
    group: usize,
    thing: usize,
    other: usize,
    runs: Runs,
}

impl SaysInRoom {
    pub fn parse(room: u16, value: &str, runs: Runs) -> Option<Self> {
        let mut parts = value.split(':');
        let mut number = |default: usize| match parts.next() {
            Some(part) => part.parse().ok(),
            None => Some(default),
        };
        let group = number(usize::MAX).filter(|group| *group != usize::MAX)?;
        Some(Self {
            room,
            group,
            thing: number(1)?,
            other: number(0)?,
            runs,
        })
    }
}

/// A name in plain letters as the units of a text: the full-width letters of the
/// encoding, whose first byte is the low half of a unit.
fn name_units(name: &str) -> Option<Vec<u16>> {
    (name.bytes())
        .map(|letter| {
            let second = match letter {
                b'A'..=b'Z' => 0x60 + (letter - b'A'),
                b'a'..=b'z' => 0x81 + (letter - b'a'),
                _ => return None,
            };
            Some(u16::from_le_bytes([0x82, second]))
        })
        .collect()
}

impl ShowPage {
    pub fn parse(value: &str) -> Option<Self> {
        let mut parts = value.split(':');
        Some(Self {
            room: parts.next()?.parse().ok()?,
            group: parts.next()?.parse().ok()?,
            page: match parts.next() {
                Some(part) => part.parse().ok()?,
                None => 0,
            },
            letters: match parts.next() {
                Some(part) => part.parse().ok()?,
                None => message::ALL,
            },
            name: match parts.next() {
                Some(part) => name_units(part)?,
                None => Vec::new(),
            },
            face: None,
            open: window::OPENING,
            runs: None,
        })
    }

    pub fn runs(mut self, runs: Runs) -> Self {
        self.runs = Some(runs);
        self
    }

    /// `--face=FILE` and `--open=PASSES` of the command line.
    pub fn with(mut self, face: Option<usize>, open: Option<i32>) -> Self {
        self.face = face;
        self.open = open.unwrap_or(window::OPENING);
        self
    }
}

/// Writes a colour of five bits a part to a pixel of the screen, if it is on it.
fn put(screen: &mut [u8], x: i32, y: i32, five: [u8; 3]) {
    if !(0..SCREEN.0 as i32).contains(&x) || !(0..SCREEN.1 as i32).contains(&y) {
        return;
    }
    let pixel = (y as usize * SCREEN.0 + x as usize) * 4;
    for (part, value) in five.into_iter().enumerate() {
        screen[pixel + part] = value << 3 | value >> 2;
    }
    screen[pixel + 3] = 255;
}

/// Half of what is under a pixel and half of a colour, as the console blends in mode 0.
/// Where the window has drawn nothing yet the room is under it: the pixel then keeps the
/// share of the room that shows through as its alpha, and the graphics card mixes the
/// room in (in eight bits a part, where the console has five).
pub(crate) fn half(screen: &mut [u8], x: i32, y: i32, colour: [u8; 3]) {
    if !(0..SCREEN.0 as i32).contains(&x) || !(0..SCREEN.1 as i32).contains(&y) {
        return;
    }
    let pixel = (y as usize * SCREEN.0 + x as usize) * 4;
    if screen[pixel + 3] == 255 {
        let mut five = [0; 3];
        for (part, value) in five.iter_mut().enumerate() {
            *value = ((screen[pixel + part] >> 3) + (colour[part] >> 3)) >> 1;
        }
        return put(screen, x, y, five);
    }
    let under = f32::from(screen[pixel + 3]) / 255.0;
    let covered = 0.5 + 0.5 * under;
    for (part, value) in colour.into_iter().enumerate() {
        let five = value >> 3;
        let own = f32::from(five << 3 | five >> 2);
        let mixed = (0.5 * own + 0.5 * under * f32::from(screen[pixel + part])) / covered;
        screen[pixel + part] = mixed.round() as u8;
    }
    screen[pixel + 3] = (covered * 255.0).round() as u8;
}

/// The screen before a window is drawn: black, or clear over a room.
fn empty_screen(over_room: bool) -> Vec<u8> {
    let pixel = [0, 0, 0, if over_room { 0 } else { 255 }];
    pixel.repeat(SCREEN.0 * SCREEN.1)
}

/// Every place of a rectangle, row by row.
fn places(rect: Rect) -> impl Iterator<Item = (i16, i16)> {
    (rect.y..rect.y + rect.h.max(0))
        .flat_map(move |y| (rect.x..rect.x + rect.w.max(0)).map(move |x| (x, y)))
}

/// The pixels of a picture and its size.
struct Pixels {
    rgba: Vec<u8>,
    wide: usize,
    high: usize,
}

impl Pixels {
    /// The colour of a texel in five bits a part; `None` for a texel that is not drawn.
    fn texel(&self, x: i16, y: i16) -> Option<[u8; 3]> {
        let (x, y) = (usize::try_from(x).ok()?, usize::try_from(y).ok()?);
        if x >= self.wide || y >= self.high {
            return None;
        }
        let at = (y * self.wide + x) * 4;
        (self.rgba[at + 3] != 0).then(|| {
            [
                self.rgba[at] >> 3,
                self.rgba[at + 1] >> 3,
                self.rgba[at + 2] >> 3,
            ]
        })
    }

    fn rect(&self, at: (i16, i16)) -> Rect {
        Rect {
            x: at.0,
            y: at.1,
            w: self.wide as i16,
            h: self.high as i16,
        }
    }
}

/// The disc, the fonts and the fonts' images.
struct Source {
    disc: DiscImage<std::fs::File>,
    archive: Archive,
    exe: Executable,
    fonts: Fonts,
    set: Vec<tim::Tim>,
    /// The tables of the menus' frame drawer, which draws the bar of the answers.
    frames: Frames,
}

impl Source {
    fn open() -> Result<Self, Failure> {
        let config = LocalConfig::load()?;
        let mut disc = DiscImage::open(&config.disc_image)?;
        let archive = Archive::open(&mut disc)?;
        let exe = Executable::read(&mut disc)?;
        let fonts = Fonts::from_executable(&exe.image, exe.base)
            .ok_or("the executable does not hold the font tables")?;
        let set = tim::texture_set(&archive.read(&mut disc, text::FONT_SET)?)
            .ok_or("the fonts' file is not a texture set")?;
        let frames = Frames::from_executable(&exe.image, exe.base)
            .ok_or("the executable does not hold the tables of the frames")?;
        Ok(Self {
            disc,
            archive,
            exe,
            fonts,
            set,
            frames,
        })
    }

    /// An image of the fonts' file in its own colours.
    fn image(&self, number: usize) -> Result<Pixels, Failure> {
        let picture = self.set.get(number).ok_or("the set has no such image")?;
        Ok(Pixels {
            rgba: picture.to_rgba(picture.palette(0))?,
            wide: picture.width as usize,
            high: picture.height as usize,
        })
    }

    /// The face a model file brings: the picture of its texture set that lies where the
    /// window's face routine looks for one.
    fn face(&mut self, file: usize) -> Result<Pixels, Failure> {
        let bytes = self.archive.read(&mut self.disc, file)?;
        let table = ModelTable::from_executable(&self.exe.image, self.exe.base)?;
        let (_, record) = (u16::try_from(file).ok())
            .and_then(|file| table.by_file(file))
            .ok_or("that is not a model file")?;
        let images = record.textures(&bytes)?;
        let faces = Faces::from_executable(&self.exe.image, self.exe.base)
            .ok_or("the executable does not hold the places of the faces")?;
        let side = window::FACE as u32;
        let picture = (images.iter())
            .find(|image| {
                (image.width, image.height) == (side, side)
                    && faces.is_face((image.vram.x as i16, image.vram.y as i16))
            })
            .ok_or("the model file has no face")?;
        let palette = picture.palette(0).or_else(|| {
            let clut = picture.clut.as_ref()?;
            tim::shared_palette(&images, clut.rect, 0)
        });
        Ok(Pixels {
            rgba: picture.to_rgba(palette)?,
            wide: side as usize,
            high: side as usize,
        })
    }

    /// The frame and the fill of a box on the screen: the frame first, the fill over
    /// it, half of what is under it and half its colour. False for a box too small.
    fn boxed(&self, screen: &mut [u8], rect: Rect, colour: [u8; 3]) -> Result<bool, Failure> {
        let mut sizes: window::Sizes = [(0, 0); 8];
        let mut pictures = Vec::new();
        for (part, size) in sizes.iter_mut().enumerate() {
            let picture = self.image(window::FRAME_IMAGE + part)?;
            *size = (picture.wide as i16, picture.high as i16);
            pictures.push(picture);
        }
        let Some(pieces) = window::frame(rect, &sizes) else {
            return Ok(false);
        };
        for piece in pieces {
            let picture = &pictures[piece.part as usize];
            let on = piece.on_screen;
            for (x, y) in places(on) {
                let (u, v) = piece.texel(x - on.x, y - on.y);
                if let Some(five) = picture.texel(u, v) {
                    put(screen, i32::from(x), i32::from(y), five);
                }
            }
        }
        for (x, y) in places(window::fill(rect)) {
            half(screen, i32::from(x), i32::from(y), colour);
        }
        Ok(true)
    }

    /// The console's screen with the letters on it, four bytes a pixel.
    fn screen(&self, font: &Font, letters: &[Letter]) -> Result<Vec<u8>, Failure> {
        let picture = self
            .set
            .get(font.image)
            .ok_or("the set has no such image")?;
        let (wide, high) = (picture.width as usize, picture.height as usize);
        let (cell_w, cell_h) = (usize::from(font.cell.0), usize::from(font.cell.1));
        // The font's image in every palette row a letter asks for.
        let mut rows: Vec<Option<Vec<u8>>> = Vec::new();
        let mut screen = vec![0; SCREEN.0 * SCREEN.1 * 4];
        for letter in letters {
            let Some(cell) = letter.glyph.cell else {
                continue;
            };
            let row = usize::from(letter.row);
            if rows.len() <= row {
                rows.resize(row + 1, None);
            }
            if rows[row].is_none() {
                rows[row] = Some(picture.to_rgba(picture.palette(row))?);
            }
            let Some(rgba) = &rows[row] else { continue };
            for (dx, dy) in (0..cell_h).flat_map(|dy| (0..cell_w).map(move |dx| (dx, dy))) {
                let from = (usize::from(cell.0) + dx, usize::from(cell.1) + dy);
                let to = (
                    i32::from(letter.glyph.at.0) + dx as i32,
                    i32::from(letter.glyph.at.1) + dy as i32,
                );
                let inside =
                    (0..SCREEN.0 as i32).contains(&to.0) && (0..SCREEN.1 as i32).contains(&to.1);
                if !inside || from.0 >= wide || from.1 >= high {
                    continue;
                }
                let texel = (from.1 * wide + from.0) * 4;
                let pixel = (to.1 as usize * SCREEN.0 + to.0 as usize) * 4;
                // An earlier sprite lies over a later one, and a texel of colour 0 is
                // not drawn.
                if rgba[texel + 3] != 0 && screen[pixel + 3] == 0 {
                    screen[pixel..pixel + 4].copy_from_slice(&rgba[texel..texel + 4]);
                }
            }
        }
        Ok(screen)
    }
}

/// The console's screen, black, with texts of an archive file on it in one font: each
/// text is named by its address and the place of its first glyph's cell.
pub fn texts_on_black(
    file: usize,
    font: usize,
    texts: impl Iterator<Item = (u32, (i16, i16))>,
) -> Result<Vec<u8>, Failure> {
    let mut source = Source::open()?;
    let font = source.fonts.get(font).ok_or("there is no such font")?;
    let image = source.archive.read(&mut source.disc, file)?;
    let base = rooms::load_address(file);
    let mut letters = Vec::new();
    for (address, at) in texts {
        let bytes = (address.checked_sub(base))
            .and_then(|offset| image.get(offset as usize..))
            .ok_or("the address is not in the file")?;
        let codes = font.codes(&text::units(bytes));
        letters.extend((font.glyphs(&codes, at).into_iter()).map(|glyph| Letter { glyph, row: 0 }));
    }
    let mut screen = source.screen(font, &letters)?;
    for pixel in screen.as_chunks_mut::<4>().0 {
        pixel[3] = 255;
    }
    Ok(screen)
}

/// The screen with the text on it and the text's letters.
fn load_text(show: &ShowText) -> Result<(Vec<u8>, Vec<Letter>), Failure> {
    let mut source = Source::open()?;
    let font = source.fonts.get(show.font).ok_or("there is no such font")?;
    let file;
    let (image, base) = match show.file {
        Some(index) => {
            file = source.archive.read(&mut source.disc, index)?;
            (&file, rooms::load_address(index))
        }
        None => (&source.exe.image, source.exe.base),
    };
    let bytes = (show.address.checked_sub(base))
        .and_then(|offset| image.get(offset as usize..))
        .ok_or("the address is not in the image")?;
    let codes = font.codes(&text::units(bytes));
    let letters: Vec<Letter> = (font.glyphs(&codes, START).into_iter())
        .map(|glyph| Letter {
            glyph,
            row: show.row,
        })
        .collect();
    Ok((source.screen(font, &letters)?, letters))
}

pub fn show_text(
    show: Res<ShowText>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    let (screen, letters) = match load_text(&show) {
        Ok(text) => text,
        Err(err) => {
            error!("the text at {:08X}: {err}", show.address);
            exit.write(AppExit::error());
            return;
        }
    };
    let drawn = (letters.iter())
        .filter(|letter| letter.glyph.cell.is_some())
        .count();
    let codes: Vec<u16> = letters.iter().map(|letter| letter.glyph.code).collect();
    let ends = letters.last().map_or(START.0, |letter| letter.glyph.at.0);
    info!(
        "the text at {:08X} in font {}, palette row {}: {} units, {drawn} drawn, the last at x = {ends}, codes {codes:?}",
        show.address,
        show.font,
        show.row,
        letters.len()
    );
    commands.spawn(Camera2d);
    crate::spawn_picture(
        &mut commands,
        &mut images,
        SCREEN.0 as u32,
        SCREEN.1 as u32,
        screen,
    );
}

#[derive(Resource)]
pub struct ShowNotice {
    room: u16,
    nth: usize,
}

impl ShowNotice {
    pub fn parse(value: &str) -> Option<Self> {
        let (room, nth) = value.split_once(':').unwrap_or((value, "0"));
        Some(Self {
            room: room.parse().ok()?,
            nth: nth.parse().ok()?,
        })
    }
}

/// The screen with a notice on it and a line that tells what it holds.
fn load_notice(show: &ShowNotice) -> Result<(Vec<u8>, String), Failure> {
    let mut source = Source::open()?;
    let engine = (source.archive).read(&mut source.disc, rooms::FIELD_ENGINE_FILE)?;
    let table = RoomTable::from_field_engine(&engine)?;
    let (_, record) = table.by_number(show.room).ok_or("there is no such room")?;
    let file = (source.archive).read(&mut source.disc, record.package_file as usize)?;
    let package = ScenePackage::parse(&file)?;
    let notices: Vec<_> = (package.script_commands().iter())
        .filter_map(|command| {
            let notice = package.notice(command)?;
            let things = notice.things.filter(|things| !things.is_empty())?;
            Some((command.address, notice.way, things))
        })
        .collect();
    let count = notices.len();
    let (address, way, listed) = (notices.into_iter().nth(show.nth))
        .ok_or_else(|| format!("the scripts of the room have {count} notices with things"))?;
    let mut things = [notice::Thing::NONE; notice::THINGS];
    for (thing, listed) in things.iter_mut().zip(&listed) {
        *thing = notice::Thing {
            kind: listed.kind,
            number: listed.number,
            count: listed.count,
        };
    }

    // The view shows the notices with things, which have no lines.
    let colours = window::fill_colours(&source.exe.image, source.exe.base)
        .ok_or("the executable does not hold the fill's colours")?;
    let mut screen = empty_screen(false);
    let painted = source.notice(&mut screen, &things, way, &[], colours[0])?;
    let told = format!(
        "the notice of room {} at {address:#010x}, {} of {count}: way {way}, things {:?}, \
         {painted}",
        show.room,
        show.nth,
        listed
            .iter()
            .map(|thing| (thing.kind, thing.number, thing.count))
            .collect::<Vec<_>>(),
    );
    Ok((screen, told))
}

impl Source {
    fn notice(
        &self,
        screen: &mut [u8],
        things: &[notice::Thing; notice::THINGS],
        way: i32,
        lines: &[Vec<u16>],
        colour: [u8; 3],
    ) -> Result<String, Failure> {
        let tables = notice::Tables::new(&self.exe.image, self.exe.base);
        let font = (self.fonts.get(text::TEXT_FONT)).ok_or("there is no text font")?;
        let small = (self.fonts.get(text::SMALL_FONT)).ok_or("there is no small font")?;
        let name = |thing: &notice::Thing| {
            let name = tables.name(thing.kind, thing.number);
            name.map(|address| tables.text(address)).unwrap_or_default()
        };
        let mut pictures = Vec::new();
        for thing in things {
            pictures.push(match tables.picture(thing.kind, thing.number) {
                Some(number) => Some((number, self.image(number)?)),
                None => None,
            });
        }
        let widths = [0, 1, 2].map(|slot| {
            let picture = pictures[slot].as_ref();
            picture.map_or(0, |(_, picture)| picture.wide as i16)
        });
        let widest = (lines.iter())
            .map(|line| message::width(font, line, &[]))
            .max();
        let laid = notice::laid_out(
            things,
            way,
            lines.len(),
            widest.unwrap_or(0),
            widths,
            message::width(font, &name(&things[0]), &[]),
        );

        let boxed = self.boxed(screen, laid.rect, colour)?;
        let (mut letters, mut small_letters) = (Vec::new(), Vec::new());
        let row =
            |glyphs: Vec<text::Glyph>| glyphs.into_iter().map(|glyph| Letter { glyph, row: 0 });
        for piece in &laid.pieces {
            match *piece {
                notice::Piece::Picture { slot, at } => {
                    let Some((_, picture)) = &pictures[slot] else {
                        continue;
                    };
                    let on = picture.rect(at);
                    for (x, y) in places(on) {
                        if let Some(five) = picture.texel(x - on.x, y - on.y) {
                            put(screen, i32::from(x), i32::from(y), five);
                        }
                    }
                }
                notice::Piece::Name { slot, at } => {
                    letters.extend(row(font.glyphs(&font.codes(&name(&things[slot])), at)));
                }
                notice::Piece::Label { label, at } => {
                    let units = tables.label(label);
                    match label {
                        notice::Label::Times => {
                            small_letters.extend(row(small.glyphs(&small.codes(&units), at)));
                        }
                        _ => letters.extend(row(font.glyphs(&font.codes(&units), at))),
                    }
                }
                notice::Piece::Count { value, at } => {
                    let codes = font.codes(&notice::number_units(value));
                    letters.extend(row(font.glyphs(&codes, at)));
                }
                notice::Piece::Amount { value, right } => {
                    small_letters.extend(row(notice::amount_glyphs(small, value, right)));
                }
                notice::Piece::Way { way, at } => {
                    let units = tables.way(way).unwrap_or_default();
                    letters.extend(row(font.glyphs(&font.codes(&units), at)));
                }
                // The plain text drawer: no typing, no control units.
                notice::Piece::Line { line, at } => {
                    letters.extend(row(font.glyphs(&font.codes(&lines[line]), at)));
                }
            }
        }
        self.letters(screen, font, &letters, WHOLE)?;
        self.letters(screen, small, &small_letters, WHOLE)?;

        let drawn = |letters: &[Letter]| {
            (letters.iter())
                .filter(|letter| letter.glyph.cell.is_some())
                .count()
        };
        let numbers: Vec<Option<usize>> = (pictures.iter())
            .map(|picture| picture.as_ref().map(|(number, _)| *number))
            .collect();
        Ok(format!(
            "the box {:?}{}, counted {}, pictures {numbers:?} of widths {widths:?}, {} lines, \
             {} letters and {} small ones drawn",
            laid.rect,
            if boxed { "" } else { " (too small to draw)" },
            laid.counted,
            lines.len(),
            drawn(&letters),
            drawn(&small_letters),
        ))
    }
}

pub fn show_notice(
    show: Res<ShowNotice>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    let (screen, told) = match load_notice(&show) {
        Ok(notice) => notice,
        Err(err) => {
            error!("the notice of room {}: {err}", show.room);
            exit.write(AppExit::error());
            return;
        }
    };
    info!("{told}");
    commands.spawn(Camera2d);
    crate::spawn_picture(
        &mut commands,
        &mut images,
        SCREEN.0 as u32,
        SCREEN.1 as u32,
        screen,
    );
}

/// The screen with a page on it, what the text routine said, and how many groups the
/// room and pages the group has.
struct Page {
    screen: Vec<u8>,
    shown: message::Shown,
    /// Whether the picture's and the text's box were large enough to be drawn.
    boxes: [bool; 2],
    face: bool,
    groups: usize,
    pages: usize,
}

fn load_page(show: &ShowPage) -> Result<Page, Failure> {
    let mut source = Source::open()?;
    let face = match show.face {
        Some(file) => Some(source.face(file)?),
        None => None,
    };
    let (group, groups) = source.group(show.room, show.group)?;
    let font = (source.fonts.get(text::TEXT_FONT)).ok_or("there is no text font")?;
    let page = (group.pages.get(show.page))
        .ok_or_else(|| format!("the group has {} pages", group.pages.len()))?;
    let layout = Layout::of(Where::Bottom, false);
    let shown = message::shown(font, page, layout.first_line, show.letters, &show.name);
    let colours = window::fill_colours(&source.exe.image, source.exe.base)
        .ok_or("the executable does not hold the fill's colours")?;
    // A pass of the window as it stands with the page, or while its boxes open.
    let open = show.open >= window::OPENING;
    let part = |rect: Rect| window::opened(rect, show.open, window::OPENING, Way::Middle);
    let pass = window::Pass {
        furnished: open,
        text_box: Some(part(layout.text)),
        picture_box: Some(part(layout.picture)),
        lines: open.then_some(window::Lines {
            page: show.page,
            letters: show.letters,
            clip: Layout::clip(layout.text),
        }),
        more: open && shown.whole,
        ..Default::default()
    };
    let told = Told {
        place: Where::Bottom,
        picture_left: false,
        group: &group,
        open_answers: 0,
        speaker: &[],
        name: &show.name,
        face: face.as_ref(),
        mirrored: false,
        over_room: false,
        colour: colours[0],
        kind: 0,
    };
    let (screen, boxes) = source.paint(font, &told, &pass, MORE_PASS)?;
    Ok(Page {
        screen,
        shown,
        boxes,
        face: face.is_some(),
        groups,
        pages: group.pages.len(),
    })
}

/// The whole screen as a rectangle.
const WHOLE: Rect = Rect {
    x: 0,
    y: 0,
    w: SCREEN.0 as i16,
    h: SCREEN.1 as i16,
};

/// What a window shows besides what a pass says of it.
struct Told<'a> {
    place: Where,
    picture_left: bool,
    group: &'a Group,
    /// The answers that can be picked, one bit each.
    open_answers: u32,
    /// The speaker's name, and the name that is put in for the player's.
    speaker: &'a [u16],
    name: &'a [u16],
    face: Option<&'a Pixels>,
    /// The face is drawn left to right as it lies right to left.
    mirrored: bool,
    /// A room is under the window, not black.
    over_room: bool,
    /// The colour of the fills.
    colour: [u8; 3],
    /// The window colour, for the bar of the answers' cursor.
    kind: i16,
}

impl Source {
    /// One group of a room's text, and how many groups the room has.
    fn group(&mut self, room: u16, group: usize) -> Result<(Group, usize), Failure> {
        let engine = (self.archive).read(&mut self.disc, rooms::FIELD_ENGINE_FILE)?;
        let table = RoomTable::from_field_engine(&engine)?;
        let (_, record) = table.by_number(room).ok_or("there is no such room")?;
        let file = (self.archive).read(&mut self.disc, record.package_file as usize)?;
        let package = ScenePackage::parse(&file)?;
        let data = RoomData::parse(package.room_data())?;
        let resource = (data.resources.iter())
            .find(|resource| resource.kind == 1)
            .ok_or("the room has no text")?;
        let mut groups = message::groups(resource.data).ok_or("the room's text does not read")?;
        let count = groups.len();
        if group >= count {
            return Err(format!("the room has {count} groups").into());
        }
        Ok((groups.swap_remove(group), count))
    }

    /// Letters on the screen, cut off at a rectangle.
    fn letters(
        &self,
        screen: &mut [u8],
        font: &Font,
        letters: &[Letter],
        clip: Rect,
    ) -> Result<(), Failure> {
        let layer = self.screen(font, letters)?;
        for (x, y) in places(clip) {
            if !(0..SCREEN.0 as i16).contains(&x) || !(0..SCREEN.1 as i16).contains(&y) {
                continue;
            }
            let pixel = (y as usize * SCREEN.0 + x as usize) * 4;
            if layer[pixel + 3] != 0 {
                screen[pixel..pixel + 4].copy_from_slice(&layer[pixel..pixel + 4]);
            }
        }
        Ok(())
    }

    /// The screen of one pass of a window, and whether the picture's and the text's box
    /// were large enough to be drawn. `counter` is the pass of the sign's pulse.
    fn paint(
        &self,
        font: &Font,
        told: &Told,
        pass: &window::Pass,
        counter: u32,
    ) -> Result<(Vec<u8>, [bool; 2]), Failure> {
        let layout = Layout::of(told.place, told.picture_left);
        let mut screen = empty_screen(told.over_room);
        // The bar of the answers' cursor lies in the last slot, under everything else:
        // a frame of the menus in the window colour, blended with what is under it.
        let bar = (pass.answers.as_ref()).and_then(|answers| {
            let colour =
                (self.frames).colour(window::bar_style(told.kind), answers.pulse as u32)?;
            Some((window::bar(answers.rect, answers.cursor), colour))
        });
        if let Some((rect, colour)) = bar {
            let quads = self.frames.quads(rect, Some(colour));
            for (x, y) in places(rect) {
                let on_screen =
                    (0..SCREEN.0 as i16).contains(&x) && (0..SCREEN.1 as i16).contains(&y);
                let Some(quad) = quads.iter().find(|quad| on_screen && quad.covers(x, y)) else {
                    continue;
                };
                half(&mut screen, i32::from(x), i32::from(y), quad.colour);
            }
        }
        // The boxes lie in one slot, where the one added last is drawn first: the step
        // adds the name's box, the text's, the picture's and the answers'.
        if let Some(answers) = &pass.answers {
            self.boxed(&mut screen, answers.rect, told.colour)?;
        }
        let mut boxes = [false; 2];
        for (drawn, rect) in boxes.iter_mut().zip([pass.picture_box, pass.text_box]) {
            if let Some(rect) = rect {
                *drawn = self.boxed(&mut screen, rect, told.colour)?;
            }
        }
        let name_box = (pass.furnished && !told.speaker.is_empty()).then(|| {
            let width = message::width(font, told.speaker, told.name);
            Layout::name_box(told.place, told.picture_left, width)
        });
        if let Some(rect) = name_box {
            self.boxed(&mut screen, rect, told.colour)?;
        }
        if let Some(face) = told.face.filter(|_| pass.furnished) {
            // A quad from the face's place to 63 further: one pixel short of the picture.
            let side = window::FACE - 1;
            let quad = Rect {
                x: layout.face.0,
                y: layout.face.1,
                w: side,
                h: side,
            };
            for (x, y) in places(quad) {
                // A mirrored face has its two left corners where the right ones were.
                let across = match told.mirrored {
                    true => side - (x - quad.x),
                    false => x - quad.x,
                };
                if let Some(five) = face.texel(across, y - quad.y) {
                    put(&mut screen, i32::from(x), i32::from(y), five);
                }
            }
        }
        let page = (pass.lines).and_then(|lines| Some((lines, told.group.pages.get(lines.page)?)));
        if let Some((lines, page)) = page {
            let shown = message::shown(font, page, layout.first_line, lines.letters, told.name);
            self.letters(&mut screen, font, &shown.letters, lines.clip)?;
        }
        if let Some(rect) = name_box {
            let glyphs = font.glyphs(&font.codes(told.speaker), (rect.x + 7, rect.y + 2));
            let letters: Vec<Letter> = (glyphs.into_iter())
                .map(|glyph| Letter { glyph, row: 0 })
                .collect();
            self.letters(&mut screen, font, &letters, WHOLE)?;
        }
        if let Some(answers) = &pass.answers {
            let open = (told.group.answers.iter().enumerate())
                .filter(|(answer, _)| *answer < 32 && told.open_answers & (1 << answer) != 0);
            let mut letters = Vec::new();
            for (line, (_, answer)) in open.enumerate() {
                let at = window::answer_at(answers.rect, line as i16);
                letters.extend(message::line(font, answer, at));
            }
            self.letters(&mut screen, font, &letters, Layout::clip(answers.rect))?;
        }
        if pass.more {
            let sign = self.image(window::MORE_IMAGE)?;
            let on = sign.rect(window::more_at(layout.first_line));
            let bright = u16::from(window::more_brightness(counter));
            for (x, y) in places(on) {
                if let Some(five) = sign.texel(x - on.x, y - on.y) {
                    let five = five.map(|value| ((u16::from(value) * bright) >> 7).min(31) as u8);
                    put(&mut screen, i32::from(x), i32::from(y), five);
                }
            }
        }
        Ok((screen, boxes))
    }
}

/// A window that runs: `--says` with `--talk`.
#[derive(Resource)]
pub struct Talk {
    source: Source,
    font: Font,
    group: Group,
    place: Where,
    picture_left: bool,
    mirrored: bool,
    over_room: bool,
    name: Vec<u16>,
    speaker: Vec<u16>,
    face: Option<Pixels>,
    colour: [u8; 3],
    open_answers: u32,
    window: Window,
    /// The game's setting word.
    setting: u32,
    pad: Pad,
    passes: u32,
    stand: Option<u32>,
    keys: Vec<(u32, u16)>,
    /// The owner has let the window take a confirm and waits for it.
    waits: bool,
    sprite: Option<Entity>,
}

impl Talk {
    /// The window as its opener leaves it, and the screen before its first pass or, for
    /// a window that stands, after the pass it stands at.
    fn open(show: &ShowPage, runs: &Runs) -> Result<(Self, Vec<u8>), Failure> {
        let mut source = Source::open()?;
        let face = match show.face {
            Some(file) => Some(source.face(file)?),
            None => None,
        };
        let said = Said {
            room: show.room,
            group: show.group,
            name: show.name.clone(),
            speaker: runs.speaker.clone(),
            face,
            picture_left: false,
            mirrored: false,
            over_room: false,
        };
        Self::begin(source, said, runs)
    }

    /// The window of a message command in a room, with what the room says of the
    /// speaker.
    fn open_in_room(says: &SaysInRoom, things: &Things) -> Result<(Self, Vec<u8>), Failure> {
        let source = Source::open()?;
        let faces = Faces::from_executable(&source.exe.image, source.exe.base)
            .ok_or("the executable does not hold the places of the faces")?;
        let speaker = things.speaker(&faces, says.thing, says.other)?;
        info!(
            "room {} group {}: thing {} speaks to thing {}, seen at x = {} and {}: the picture on the {}{}",
            says.room,
            says.group,
            says.thing,
            says.other,
            speaker.seen[0],
            speaker.seen[1],
            if speaker.picture_left {
                "left"
            } else {
                "right"
            },
            match (speaker.mirrored, speaker.picture_left) {
                (true, false) => ", the face mirrored",
                (true, true) => ", the face as it is (its mirror word counts on the right only)",
                (false, _) => "",
            }
        );
        let side = window::FACE as usize;
        let said = Said {
            room: says.room,
            group: says.group,
            name: Vec::new(),
            speaker: speaker.name,
            face: Some(Pixels {
                rgba: speaker.face,
                wide: side,
                high: side,
            }),
            picture_left: speaker.picture_left,
            mirrored: speaker.mirrored && !speaker.picture_left,
            over_room: true,
        };
        Self::begin(source, said, &says.runs)
    }

    fn begin(mut source: Source, said: Said, runs: &Runs) -> Result<(Self, Vec<u8>), Failure> {
        let show = &said;
        let (group, _) = source.group(show.room, show.group)?;
        let font = (source.fonts.get(text::TEXT_FONT))
            .ok_or("there is no text font")?
            .clone();
        let colours = window::fill_colours(&source.exe.image, source.exe.base)
            .ok_or("the executable does not hold the fill's colours")?;
        if runs.answers && group.answers.is_empty() {
            return Err("the group has no answers".into());
        }
        let widest = (group.answers.iter())
            .map(|answer| message::width(&font, answer, &show.name))
            .max()
            .unwrap_or(0);
        // What the scripts' builder asks for: a question at the top, any other message
        // at the bottom, no answer switched off.
        let setting = runs.setting();
        let asked = window::Asked::said(
            runs.answers,
            setting,
            group.pages.len(),
            group.answers.len(),
            0,
            said.picture_left,
        );
        let (place, open_answers) = (asked.place, asked.open_answers);
        let over_room = said.over_room;
        let mut talk = Self {
            source,
            font,
            group,
            place,
            picture_left: said.picture_left,
            mirrored: said.mirrored,
            over_room,
            name: said.name,
            speaker: said.speaker,
            face: said.face,
            colour: colours[0],
            open_answers,
            window: Window::open(asked, widest),
            setting,
            pad: Pad::default(),
            passes: 0,
            stand: runs.stand,
            keys: runs.keys.clone(),
            waits: false,
            sprite: None,
        };
        let mut screen = empty_screen(over_room);
        for _ in 0..runs.stand.unwrap_or(0) {
            if talk.window.is_gone() {
                break;
            }
            screen = talk.pass(0)?;
        }
        Ok((talk, screen))
    }

    /// One pass with the keyboard's buttons: the owner, the window, and the screen.
    fn pass(&mut self, keyboard: u16) -> Result<Vec<u8>, Failure> {
        self.passes += 1;
        let passes = self.passes;
        let held = (self.keys.iter())
            .filter(|(pass, _)| *pass == passes)
            .fold(keyboard, |held, (_, button)| held | button);
        self.pad.read(held, Repeat::of(2), passes);
        // The owner as the scripts' waiter `0x800BC47C` is one: it lets the window take
        // a confirm when a page or the answers stand whole, and closes the window when
        // the confirm was taken at its end. A taken confirm can only be asked for
        // between the pass that took it and the next, so the owner comes in between.
        match (self.waits, self.window.taken()) {
            (true, Some(taken)) => {
                self.waits = false;
                if taken.at_end {
                    self.window.close();
                    info!(
                        "pass {passes}: the confirm at the end ({} taken), {}, the window closes",
                        taken.confirms,
                        match taken.picked {
                            true => format!("answer {} picked", taken.answer),
                            false => "no answer".into(),
                        }
                    );
                }
            }
            (false, _) if self.window.stands() => {
                self.window.allow();
                self.waits = true;
            }
            _ => {}
        }
        let before = (self.window.wait, self.window.boxes, self.window.answers);
        let (font, group, name) = (&self.font, &self.group, &self.name);
        let first_line = self.window.layout().first_line;
        let mut read = |page: usize, letters: u32| match group.pages.get(page) {
            Some(page) => {
                let shown = message::shown(font, page, first_line, letters, name);
                window::Read {
                    whole: shown.whole,
                    cue: shown.cues.iter().copied().max(),
                }
            }
            None => window::Read {
                whole: true,
                cue: None,
            },
        };
        let pass = (self.window).step(&self.pad, self.setting, false, &mut read);
        let after = (self.window.wait, self.window.boxes, self.window.answers);
        if before != after || !pass.sounds.is_empty() {
            info!(
                "pass {passes}: {:?}, the boxes {:?}, the answers {:?}, page {}, sounds {:02X?}{}",
                after.0,
                after.1,
                after.2,
                self.window.on_screen,
                pass.sounds,
                match &pass.answers {
                    Some(answers) => format!(", the cursor on line {}", answers.cursor),
                    None => String::new(),
                }
            );
        }
        let told = Told {
            place: self.place,
            picture_left: self.picture_left,
            group,
            open_answers: self.open_answers,
            speaker: &self.speaker,
            name,
            face: self.face.as_ref(),
            mirrored: self.mirrored,
            over_room: self.over_room,
            colour: self.colour,
            kind: 0,
        };
        Ok(self.source.paint(font, &told, &pass, passes)?.0)
    }
}

/// What a talk is opened with.
struct Said {
    room: u16,
    group: usize,
    /// The name that is put in for the player's, and the speaker's.
    name: Vec<u16>,
    speaker: Vec<u16>,
    face: Option<Pixels>,
    picture_left: bool,
    mirrored: bool,
    over_room: bool,
}

pub fn talk_in_room(
    says: Res<SaysInRoom>,
    things: Option<Res<Things>>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    let opened = match &things {
        Some(things) => Talk::open_in_room(&says, things),
        None => Err("the room is not shown with its things: ask for --actors".into()),
    };
    let (mut talk, screen) = match opened {
        Ok(opened) => opened,
        Err(err) => {
            error!("room {} group {}: {err}", says.room, says.group);
            exit.write(AppExit::error());
            return;
        }
    };
    info!(
        "room {} group {}: the window runs over the room, {} pages and {} answers, {}",
        says.room,
        says.group,
        talk.group.pages.len(),
        talk.group.answers.len(),
        match talk.stand {
            Some(pass) => format!("standing after pass {pass}"),
            None => "a pass a game frame".into(),
        }
    );
    let image = crate::picture_image(SCREEN.0 as u32, SCREEN.1 as u32, screen);
    let sprite = (
        Sprite::from_image(images.add(image)),
        Transform::from_xyz(0.0, 0.0, IN_FRONT),
    );
    talk.sprite = Some(commands.spawn(sprite).id());
    commands.insert_resource(talk);
}

/// The speaker of a script's message: the room's package and the thing that speaks, its
/// name, its face and whether the face has the mirror word.
type Speaker = ((usize, usize), Vec<u16>, Pixels, bool);

/// What the windows of a room's scripts are painted from: the disc's pictures, the
/// font of the texts, the fill of every window colour and the places of the faces.
type Painter = (Source, Font, [[u8; 3]; window::COLOURS], Faces);

/// What paints the message windows of a room's scripts: the disc's pictures, read when
/// the first window comes up, the sprite, and the speaker of the message on screen.
#[derive(Default)]
pub struct ScriptTalk {
    source: Option<Painter>,
    /// The disc could not be read: said once.
    failed: bool,
    sprite: Option<Entity>,
    speaker: Option<Speaker>,
    /// The game frame of the scripts that is on screen; none without a window.
    painted: Option<u32>,
    /// A notice is on screen.
    noticed: bool,
}

impl ScriptTalk {
    fn open() -> Result<Painter, Failure> {
        let source = Source::open()?;
        let font = (source.fonts.get(text::TEXT_FONT))
            .ok_or("there is no text font")?
            .clone();
        let colours = window::fill_colours(&source.exe.image, source.exe.base)
            .ok_or("the executable does not hold the fill's colours")?;
        let faces = Faces::from_executable(&source.exe.image, source.exe.base)
            .ok_or("the executable does not hold the places of the faces")?;
        Ok((source, font, colours, faces))
    }

    /// Reads the disc's pictures when the first window comes up. False when they cannot
    /// be read, which is said once.
    fn ready(&mut self) -> bool {
        if self.source.is_none() && !self.failed {
            match Self::open() {
                Ok(source) => self.source = Some(source),
                Err(err) => {
                    error!("the window of a script cannot be painted: {err}");
                    self.failed = true;
                }
            }
        }
        self.source.is_some()
    }
}

pub fn paint_script_talk(
    things: Option<Res<Things>>,
    named: Option<Res<Named>>,
    settings: Res<crate::settings::Settings>,
    mut talk: Local<ScriptTalk>,
    mut images: ResMut<Assets<Image>>,
    mut sprites: Query<&mut Sprite>,
    mut commands: Commands,
) {
    let talk = &mut *talk;
    let kind = usize::from(settings.0.colour);
    let mut show = |screen: Vec<u8>, sprite: &mut Option<Entity>| {
        let image = images.add(crate::picture_image(
            SCREEN.0 as u32,
            SCREEN.1 as u32,
            screen,
        ));
        match sprite.and_then(|sprite| sprites.get_mut(sprite).ok()) {
            Some(mut sprite) => sprite.image = image,
            None => {
                let picture = (
                    Sprite::from_image(image),
                    Transform::from_xyz(0.0, 0.0, IN_FRONT),
                );
                *sprite = Some(commands.spawn(picture).id());
            }
        }
    };
    // A notice stands as it came up until it goes: one picture.
    if let Some(noticed) = things.as_ref().and_then(|things| things.noticed()) {
        if talk.noticed || !talk.ready() {
            return;
        }
        let Some((source, _, colours, _)) = &talk.source else {
            return;
        };
        let colour = colours.get(kind).unwrap_or(&colours[0]);
        let mut screen = empty_screen(true);
        let (things, lines) = (&noticed.things, &noticed.lines);
        match source.notice(&mut screen, things, noticed.way, lines, *colour) {
            Ok(painted) => {
                info!("the notice of a script: way {}, {painted}", noticed.way);
                show(screen, &mut talk.sprite);
                (talk.noticed, talk.painted) = (true, None);
            }
            Err(err) => {
                error!("the notice of a script cannot be painted: {err}");
                talk.failed = true;
            }
        }
        return;
    }
    if std::mem::take(&mut talk.noticed) {
        show(empty_screen(true), &mut talk.sprite);
    }
    let Some(spoken) = things.as_ref().and_then(|things| things.spoken()) else {
        // The window has gone: its last picture goes with it.
        if talk.painted.take().is_some() {
            show(empty_screen(true), &mut talk.sprite);
        }
        return;
    };
    if talk.painted == Some(spoken.game_frame) || talk.failed {
        return;
    }
    if !talk.ready() {
        return;
    }
    let Some((source, font, colours, faces)) = &talk.source else {
        return;
    };
    let colour = colours.get(kind).unwrap_or(&colours[0]);
    // The speaker is kept by the room's package and the thing: thing 1 of the next room
    // is somebody else.
    let speaks = (spoken.file, spoken.thing);
    if (talk.speaker.as_ref()).is_none_or(|(kept, ..)| *kept != speaks) {
        let speaker = (things.as_ref())
            .ok_or_else(|| "no room".to_string())
            .and_then(|things| things.speaker(faces, spoken.thing, spoken.other));
        match speaker {
            Ok(speaker) => {
                let side = window::FACE as usize;
                let face = Pixels {
                    rgba: speaker.face,
                    wide: side,
                    high: side,
                };
                talk.speaker = Some((speaks, speaker.name, face, speaker.mirrored));
            }
            Err(err) => {
                error!("thing {} cannot speak: {err}", spoken.thing);
                talk.failed = true;
                return;
            }
        }
    }
    let Some((_, speaker, face, mirror_word)) = &talk.speaker else {
        return;
    };
    let name = named.as_ref().map_or(&[][..], |named| &named.units[..]);
    let told = Told {
        place: spoken.place,
        picture_left: spoken.picture_left,
        group: spoken.group,
        open_answers: spoken.open_answers,
        speaker,
        name,
        face: Some(face),
        // The mirror word counts for a face on the right only.
        mirrored: *mirror_word && !spoken.picture_left,
        over_room: true,
        colour: *colour,
        kind: kind as i16,
    };
    match source.paint(font, &told, spoken.pass, spoken.passes) {
        Ok((screen, _)) => {
            show(screen, &mut talk.sprite);
            talk.painted = Some(spoken.game_frame);
        }
        Err(err) => {
            error!("the window of a script cannot be painted: {err}");
            talk.failed = true;
        }
    }
}

/// Where the window's picture lies on the depth axis of the field's screen: in front of
/// every layer and thing, which lie at zero and behind.
const IN_FRONT: f32 = 10.0;

/// The keyboard as the pad's word of held buttons.
fn keyboard(keys: &crate::keys::Pad) -> u16 {
    let window = pad::CROSS | pad::TRIANGLE | pad::UP | pad::DOWN | pad::LEFT | pad::RIGHT;
    crate::keys::held(keys) & window
}

/// One pass of a window that runs, once a game frame.
pub fn run_talk(
    talk: Option<ResMut<Talk>>,
    keys: Res<crate::keys::Pad>,
    mut images: ResMut<Assets<Image>>,
    mut sprites: Query<&mut Sprite>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(mut talk) = talk else { return };
    if talk.stand.is_some() || talk.window.is_gone() {
        return;
    }
    let screen = match talk.pass(keyboard(&keys)) {
        Ok(screen) => screen,
        Err(err) => {
            error!("the window: {err}");
            exit.write(AppExit::error());
            return;
        }
    };
    if talk.window.is_gone() {
        info!("pass {}: the window is gone", talk.passes);
    }
    let sprite = talk.sprite.and_then(|sprite| sprites.get_mut(sprite).ok());
    if let Some(mut sprite) = sprite {
        let image = crate::picture_image(SCREEN.0 as u32, SCREEN.1 as u32, screen);
        sprite.image = images.add(image);
    }
}

pub fn show_page(
    show: Res<ShowPage>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    if let Some(runs) = &show.runs {
        match Talk::open(&show, runs) {
            Ok((mut talk, screen)) => {
                info!(
                    "room {} group {}: the window runs, {} pages and {} answers, {}",
                    show.room,
                    show.group,
                    talk.group.pages.len(),
                    talk.group.answers.len(),
                    match talk.stand {
                        Some(pass) => format!("standing after pass {pass}"),
                        None => "a pass a game frame".into(),
                    }
                );
                commands.spawn(Camera2d);
                talk.sprite = Some(crate::spawn_picture(
                    &mut commands,
                    &mut images,
                    SCREEN.0 as u32,
                    SCREEN.1 as u32,
                    screen,
                ));
                commands.insert_resource(talk);
            }
            Err(err) => {
                error!("room {} group {}: {err}", show.room, show.group);
                exit.write(AppExit::error());
            }
        }
        return;
    }
    let Page {
        screen,
        shown,
        boxes,
        face,
        groups,
        pages,
    } = match load_page(&show) {
        Ok(page) => page,
        Err(err) => {
            error!("room {} group {}: {err}", show.room, show.group);
            exit.write(AppExit::error());
            return;
        }
    };
    let lowest = shown.letters.iter().map(|letter| letter.glyph.at.1).max();
    let marked = shown
        .letters
        .iter()
        .filter(|letter| letter.row != 0)
        .count();
    info!(
        "room {} group {} of {groups}, page {} of {pages}: {} letters ({marked} in another colour), cues {:?}, {}, the lowest line at y = {lowest:?}",
        show.room,
        show.group,
        show.page,
        shown.letters.len(),
        shown.cues,
        if shown.whole { "whole" } else { "not whole" },
    );
    let drawn = |drawn: bool| if drawn { "drawn" } else { "too small to draw" };
    info!(
        "the window at the bottom, open {} of {}: the picture's box {}, the text's box {}, {}, {}",
        show.open,
        window::OPENING,
        drawn(boxes[0]),
        drawn(boxes[1]),
        if face {
            "with a face"
        } else {
            "without a face"
        },
        match shown.whole && show.open >= window::OPENING {
            true => "the sign of more shown",
            false => "no sign of more",
        },
    );
    commands.spawn(Camera2d);
    crate::spawn_picture(
        &mut commands,
        &mut images,
        SCREEN.0 as u32,
        SCREEN.1 as u32,
        screen,
    );
}
