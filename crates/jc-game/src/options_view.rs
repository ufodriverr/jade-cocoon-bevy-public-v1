
use bevy::prelude::*;
use jc_disc::{Archive, DiscImage, Executable, LocalConfig};
use jc_formats::menu::{self, Frames, Rect, Style, sign};
use jc_formats::name;
use jc_formats::notice;
use jc_formats::options_panel;
use jc_formats::options_screen::{self, OptionsScreen, Passed, Settings};
use jc_formats::pad::Pad;
use jc_formats::player_menu::SOUND_CONFIRM;
use jc_formats::player_panel::{self, Drawn};
use jc_formats::text::{self, Font, Fonts};
use jc_formats::tim;
use jc_formats::window;

use crate::music::Music;
use crate::name_entry::{Picture, SCREEN, WHOLE, blend, blit};
use crate::pc_settings::{self, PcSettings};
use crate::place_map::{lit, picture};
use crate::pointer::{self, Aim, Mouse};
use crate::prefs::Prefs;
use crate::prompts::{self, Laid};

type Failure = Box<dyn std::error::Error>;

/// The line of the first list that is "PC Settings" in place of "Vibration".
pub(crate) const PC_LINE: i16 = options_screen::VIBRATION - 1;
/// Its words, and its line of help.
const PC_WORDS: &str = "PC Settings";
const PC_HELP: &str = "Screen, sound, game and keys of the PC.";

/// The PC Settings with what they show and the scroll signs to draw them with.
#[derive(Clone, Copy)]
pub(crate) struct Pc<'a> {
    pub(crate) settings: &'a PcSettings,
    pub(crate) prefs: &'a Prefs,
    /// The scroll sign in every window colour (`scroll_signs`).
    pub(crate) signs: &'a [Picture],
}

/// The colour of the words of "PC Settings", near the green of the game's pictures of
/// words on the menus' lines.
const PC_INK: [u8; 3] = [150, 200, 120];

/// Whether a pixel lies in a drawing area.
pub(crate) fn inside(clip: Rect, x: i16, y: i16) -> bool {
    (clip.x..clip.x.saturating_add(clip.w)).contains(&x)
        && (clip.y..clip.y.saturating_add(clip.h)).contains(&y)
}

/// Puts a picture on the screen where it lies inside `clip`, every texel that is drawn
/// under a colour; a part of 128 leaves a texel as it is.
pub(crate) fn blit_lit(
    screen: &mut [u8],
    picture: &Picture,
    at: (i16, i16),
    colour: [u8; 3],
    clip: Rect,
) {
    for (v, u) in (0..picture.high).flat_map(|v| (0..picture.wide).map(move |u| (v, u))) {
        let (x, y) = (at.0.wrapping_add(u as i16), at.1.wrapping_add(v as i16));
        let texel = (v * picture.wide + u) * 4;
        if picture.rgba[texel + 3] == 0 || !inside(clip, x, y) {
            continue;
        }
        let texel = [0, 1, 2].map(|part| picture.rgba[texel + part]);
        blend(screen, x, y, lit(texel, colour), None);
    }
}

/// A text in a font, from a picture of the font's image. The signs of the pad's buttons
/// in it are the keys or the pad's buttons of the prompts' style (`prompts.rs`).
pub(crate) fn glyphs(
    screen: &mut [u8],
    font: &Font,
    picture: &Picture,
    units: &[u16],
    at: (i16, i16),
    clip: Rect,
) {
    let cell = (usize::from(font.cell.0), usize::from(font.cell.1));
    let signs = prompts::now();
    for glyph in font.glyphs(&font.codes(units), at) {
        if let Some((sign, at)) = prompts::in_text(signs.as_deref(), font, glyph.code, glyph.at) {
            prompts::put(screen, &sign, at, clip);
            continue;
        }
        if let Some(from) = glyph.cell {
            let from = (usize::from(from.0), usize::from(from.1));
            blit(screen, picture, from, cell, glyph.at, clip);
        }
    }
}

/// A text in a font under a colour, as the text drawer draws it when its sprite is given
/// one: a part of 128 leaves a letter as it is, less darkens it.
pub(crate) fn glyphs_lit(
    screen: &mut [u8],
    font: &Font,
    picture: &Picture,
    units: &[u16],
    at: (i16, i16),
    colour: [u8; 3],
) {
    let cell = (usize::from(font.cell.0), usize::from(font.cell.1));
    let signs = prompts::now();
    for glyph in font.glyphs(&font.codes(units), at) {
        if let Some((sign, at)) = prompts::in_text(signs.as_deref(), font, glyph.code, glyph.at) {
            prompts::put(screen, &sign, at, WHOLE);
            continue;
        }
        let Some(from) = glyph.cell else {
            continue;
        };
        let from = (usize::from(from.0), usize::from(from.1));
        for (dx, dy) in (0..cell.1).flat_map(|dy| (0..cell.0).map(move |dx| (dx, dy))) {
            let (u, v) = (from.0 + dx, from.1 + dy);
            let texel = (v * picture.wide + u) * 4;
            if u >= picture.wide || picture.rgba.get(texel + 3).is_none_or(|alpha| *alpha == 0) {
                continue;
            }
            let texel = [0, 1, 2].map(|part| picture.rgba[texel + part]);
            let (x, y) = (glyph.at.0 + dx as i16, glyph.at.1 + dy as i16);
            blend(screen, x, y, lit(texel, colour), None);
        }
    }
}

/// The small picture of a thing, an image of the fonts' file, and its name.
pub(crate) type ThingLook = (Option<Picture>, Vec<u16>);

/// The looks of the first `count` things of a kind of the executable's tables: 0 the
/// items, 1 to 3 the weapons, the armours and the third things, 5 the things of the
/// story.
pub(crate) fn thing_looks(
    things: &notice::Tables,
    font_set: &[tim::Tim],
    kind: i32,
    count: i32,
) -> Vec<ThingLook> {
    (0..count)
        .map(|thing| {
            let small =
                (things.picture(kind, thing)).and_then(|number| picture(font_set, number).ok());
            let name = things.name(kind, thing);
            (small, name.map(|at| things.text(at)).unwrap_or_default())
        })
        .collect()
}

/// The sizes of the eight pictures of a window's frame.
pub(crate) fn window_sizes(window: &[Picture]) -> window::Sizes {
    std::array::from_fn(|part| {
        let picture = window.get(part);
        picture.map_or((0, 0), |picture| (picture.wide as i16, picture.high as i16))
    })
}

/// The frame of a window of the messages round a box, from its eight pictures.
pub(crate) fn window_frame(screen: &mut [u8], window: &[Picture], rect: Rect) {
    let Some(pieces) = window::frame(rect, &window_sizes(window)) else {
        return;
    };
    for piece in pieces {
        let picture = &window[piece.part as usize];
        let on = piece.on_screen;
        for (dx, dy) in (0..on.h).flat_map(|dy| (0..on.w).map(move |dx| (dx, dy))) {
            let (u, v) = piece.texel(dx, dy);
            let texel = (v.max(0) as usize * picture.wide + u.max(0) as usize) * 4;
            if picture.rgba.get(texel + 3).is_none_or(|alpha| *alpha == 0) {
                continue;
            }
            let texel = [0, 1, 2].map(|part| picture.rgba[texel + part]);
            blend(screen, on.x + dx, on.y + dy, texel, None);
        }
    }
}

/// The fill of a window of the messages: half of what is under it and half its colour.
pub(crate) fn window_fill(screen: &mut [u8], window: &[Picture], fill: [u8; 3], rect: Rect) {
    if !window::is_drawn(rect, &window_sizes(window)) {
        return;
    }
    let inside = window::fill(rect);
    for y in inside.y..inside.y + inside.h {
        for x in inside.x..inside.x + inside.w {
            blend(screen, x, y, fill, Some(0));
        }
    }
}

/// The scroll sign of a list in every window colour.
pub(crate) fn scroll_signs(
    exe: &Executable,
    font_set: &[tim::Tim],
) -> Result<Vec<Picture>, Failure> {
    (0..options_panel::COLOURS)
        .map(|colour| {
            let sign = sign::image(&exe.image, exe.base, colour, sign::OF_LIST)
                .ok_or("the executable does not hold the scroll signs")?;
            picture(font_set, sign)
        })
        .collect()
}

/// An image of a set in a row of its colours, or of those of the image it shares them
/// with.
pub(crate) fn picture_in_row(
    images: &[tim::Tim],
    number: usize,
    row: usize,
) -> Result<Picture, Failure> {
    let image = images.get(number).ok_or("the set has no such image")?;
    let palette = image.palette(row).or_else(|| {
        let clut = image.clut.as_ref()?;
        tim::shared_palette(images, clut.rect, row)
    });
    Ok(Picture {
        rgba: image.to_rgba(palette)?,
        wide: image.width as usize,
        high: image.height as usize,
    })
}

/// A scroll sign turned about its place, pulsing by the time.
pub(crate) fn put_sign(screen: &mut [u8], image: &Picture, turns: u16, at: (i16, i16), time: u32) {
    let colour = menu::pulse(sign::DIM, sign::BRIGHT, time);
    let size = (image.wide as i16, image.high as i16);
    for (u, v) in (0..size.1).flat_map(|v| (0..size.0).map(move |u| (u, v))) {
        let texel = (v as usize * image.wide + u as usize) * 4;
        if image.rgba[texel + 3] == 0 {
            continue;
        }
        let (dx, dy) = sign::texel(turns, size, (u, v));
        let texel = [0, 1, 2].map(|part| image.rgba[texel + part]);
        blend(screen, at.0 + dx, at.1 + dy, lit(texel, colour), None);
    }
}

/// What the options screen brings in its own file: its lists of lines, the texts of its
/// line of help and of its two refusals, and the pairs of its legend.
pub(crate) struct Look {
    lines: options_panel::Lines,
    texts: options_panel::Texts,
    legend: Vec<(usize, Vec<u16>)>,
}

impl Look {
    pub(crate) fn lines(&self) -> &options_panel::Lines {
        &self.lines
    }

    /// Reads them from the screen's file. A word of the legend lies in that file or in
    /// the executable.
    pub(crate) fn read(file: &[u8], exe: &Executable) -> Result<Self, Failure> {
        let base = options_screen::OPTIONS_FILE_BASE;
        let lines = options_panel::Lines::from_file(file, base)
            .ok_or("the file of the options screen does not hold its lists of lines")?;
        let texts = options_panel::Texts::from_file(file, base)
            .ok_or("the file of the options screen does not hold its texts")?;
        let pairs = player_panel::help_list(file, base, options_panel::LEGEND)
            .ok_or("the file of the options screen does not hold its legend")?;
        let legend = (pairs.into_iter())
            .map(|(sprite, address)| {
                let said = match address.checked_sub(base) {
                    Some(offset) => file.get(offset as usize..),
                    None => exe.image.get(address.checked_sub(exe.base)? as usize..),
                };
                Some((sprite, text::units(said?)))
            })
            .collect::<Option<Vec<_>>>()
            .ok_or("a word of the options screen's legend is not in its file")?;
        Ok(Self {
            lines,
            texts,
            legend,
        })
    }
}

/// The parts every screen of the menus is painted with: the menus' sprite pool, the
/// tables of the frames, and the font of the texts with its picture.
#[derive(Clone, Copy)]
pub(crate) struct Parts<'a> {
    pub(crate) sprites: &'a [Picture],
    pub(crate) frames: &'a Frames,
    pub(crate) font: &'a Font,
    pub(crate) font_picture: &'a Picture,
}

impl Parts<'_> {
    /// The width of a sprite of the pool, as the drawers place by it.
    pub(crate) fn width(&self, number: usize) -> i16 {
        (self.sprites.get(number)).map_or(0, |sprite| sprite.wide as i16)
    }

    pub(crate) fn sprite(&self, screen: &mut [u8], number: usize, at: (i16, i16), clip: Rect) {
        if let Some(sprite) = self.sprites.get(number) {
            blit(screen, sprite, (0, 0), (sprite.wide, sprite.high), at, clip);
        }
    }

    /// A frame of the menus over what the screen holds.
    pub(crate) fn frame(&self, screen: &mut [u8], rect: Rect, style: Style, time: u32) {
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

    /// A text in the font of the texts.
    pub(crate) fn text(&self, screen: &mut [u8], units: &[u16], at: (i16, i16), clip: Rect) {
        glyphs(screen, self.font, self.font_picture, units, at, clip);
    }

    /// A text of the build as units of the font of the texts.
    pub(crate) fn letters(&self, said: &str) -> Vec<u16> {
        pc_settings::letters(self.font, said)
    }

    /// How wide a text of the build is in the font of the texts.
    pub(crate) fn width_of(&self, said: &str) -> i16 {
        self.font.length(&self.font.codes(&self.letters(said)))
    }

    /// The line of help of a list in a window colour: its frame, added last, lies under
    /// the signs and the words. The signs are the keys or the pad's buttons of the
    /// prompts' style (`prompts.rs`).
    pub(crate) fn legend(&self, screen: &mut [u8], colour: i16, pairs: &[(usize, Vec<u16>)]) {
        let legend = name::legend(colour);
        self.frame(screen, legend.rect, legend.style, 0);
        let sizes = |number: usize| {
            let sprite = self.sprites.get(number)?;
            Some((sprite.wide as i16, sprite.high as i16))
        };
        for piece in prompts::legend(pairs, self.font, sizes, false) {
            match piece {
                Laid::Sprite { sprite, at } => self.sprite(screen, sprite, at, WHOLE),
                Laid::Glyph { glyph, at } => prompts::put(screen, &glyph, at, WHOLE),
                Laid::Word { units, at } => self.text(screen, &units, at, WHOLE),
            }
        }
    }

    /// The wall behind the boxes of a screen of the menus (`0x8002BE6C`).
    pub(crate) fn wall(&self, screen: &mut [u8]) {
        for at in player_panel::wall() {
            self.sprite(screen, player_panel::sprite::WALL, at, WHOLE);
        }
    }

    /// One piece of the options screen; `colour` is the window colour of its frames.
    /// The screen draws no other kind of piece.
    pub(crate) fn put(&self, screen: &mut [u8], look: &Look, colour: i16, drawn: Drawn) {
        match drawn {
            Drawn::Frame { rect, mode, time } => {
                let style = Style {
                    kind: colour,
                    mode,
                    blend: 1,
                };
                self.frame(screen, rect, style, time);
            }
            Drawn::FrameOf {
                rect,
                colour,
                mode,
                time,
            } => {
                let style = Style {
                    kind: colour,
                    mode,
                    blend: 1,
                };
                self.frame(screen, rect, style, time);
            }
            Drawn::Sprite { number, at, clip } => self.sprite(screen, number, at, clip),
            Drawn::DimSprite { number, at, clip } => {
                if let Some(sprite) = self.sprites.get(number) {
                    let colour = [menu::brightness(menu::piece::DIM); 3];
                    blit_lit(screen, sprite, at, colour, clip);
                }
            }
            Drawn::OptionsHelp { page, line, at } => {
                let texts = look.texts.help.get(page);
                if let Some(said) = texts.and_then(|texts| texts.get(line)) {
                    self.text(screen, said, at, WHOLE);
                }
            }
            Drawn::OptionsRefused { which, line, at } => {
                let texts = look.texts.refusals.get(which);
                if let Some(said) = texts.and_then(|texts| texts.get(line)) {
                    self.text(screen, said, at, WHOLE);
                }
            }
            _ => {}
        }
    }

    /// What the options screen draws in a pass, over the wall: its pieces from
    /// underneath, and its legend unless a refusal is up. The first list has "PC
    /// Settings" on the line of "Vibration"; while the PC Settings are up they draw
    /// themselves in place of the options.
    pub(crate) fn options(
        &self,
        screen: &mut [u8],
        look: &Look,
        (options, settings): (&OptionsScreen, &Settings),
        time: u32,
        pc: Pc,
    ) {
        let colour = settings.colour as i16;
        if pc.settings.is_open() {
            let shown = (colour, pc.signs, time);
            pc.settings.paint(self, screen, pc.prefs, shown);
            return;
        }
        let width = |number: usize| self.width(number);
        let first = options.page == options_screen::FIRST;
        let pc_line = (look.lines.first.get(PC_LINE as usize))
            .and_then(|line| menu::bounds(line.iter().map(|piece| piece.rect)));
        for drawn in options_panel::drawn(options, settings, &look.lines, time, width) {
            match drawn {
                // The words of the line of "PC Settings" in place of the picture of
                // "Vibration", and its own line of help.
                Drawn::Sprite { at, .. }
                    if first && pc_line.is_some_and(|rect| pointer::inside(rect, at)) =>
                {
                    let Some(rect) = pc_line else { continue };
                    let said = self.letters(PC_WORDS);
                    let wide = self.width_of(PC_WORDS);
                    let place = (rect.x + (rect.w - wide) / 2, rect.y + (rect.h - 12) / 2);
                    glyphs_lit(screen, self.font, self.font_picture, &said, place, PC_INK);
                }
                Drawn::OptionsHelp { page: 0, line, at } if line == PC_LINE as usize => {
                    self.text(screen, &self.letters(PC_HELP), at, WHOLE);
                }
                drawn => self.put(screen, look, colour, drawn),
            }
        }
        if options.up && options.refusal == 0 {
            self.legend(screen, colour, &look.legend);
        }
    }
}

/// Once a frame: the mouse over the options screen, at a pixel of the screen
/// (`pointer.rs`), or over the PC Settings while they are up.
pub(crate) fn look(
    (aim, pc): (&mut Aim<options_panel::Spot>, &mut PcSettings),
    mouse: &Mouse,
    at: Option<(i16, i16)>,
    options: &OptionsScreen,
    lines: &options_panel::Lines,
) {
    if pc.is_open() {
        aim.forget();
        pc.look(mouse, at);
        return;
    }
    let spots = options_panel::spots(options, lines);
    aim.look(mouse, pointer::row_under(spots, at));
}

/// One pass of the options screen's task on the settings: the mouse steers its cursor,
/// its sounds are played and what it changed is logged under the caller's name. While
/// the PC Settings are up the pass is theirs, on the player's settings; a confirm on the
/// line of "PC Settings" opens them.
pub(crate) fn pass(
    who: &str,
    passes: u32,
    (options, aim): (&mut OptionsScreen, &mut Aim<options_panel::Spot>),
    settings: &mut Settings,
    pad: &Pad,
    (music, pc, prefs): (&mut Music, &mut PcSettings, &mut Prefs),
) -> Passed {
    if pc.is_open() {
        let passed = pc.pass(pad, prefs);
        for &sound in &passed.sounds {
            music.sound(sound.into());
        }
        if passed.closed {
            info!("{who}: pass {passes}: the PC Settings are closed, the options are back");
        }
        return Passed::default();
    }
    let mut pad = *pad;
    match options.up && options.refusal == 0 {
        true => {
            let here = options_panel::here(options, settings);
            aim.steer(&mut pad, here, |to| options_panel::way(options, here, to));
        }
        false => aim.forget(),
    }
    let pad = &pad;
    let on_pc_line = options.up
        && options.refusal == 0
        && !options.picking
        && options.page == options_screen::FIRST
        && options.cursor.at == PC_LINE
        && options.cursor.columns == 1;
    if on_pc_line && menu::confirmed(pad) {
        music.sound(SOUND_CONFIRM.into());
        info!("{who}: pass {passes}: the options: PC Settings");
        pc.open(pc_settings::Page::default());
        return Passed {
            sounds: vec![SOUND_CONFIRM],
            ..Passed::default()
        };
    }
    let place = |options: &OptionsScreen| {
        let at = (options.page, options.cursor.at);
        (at, options.picking, options.refusal, options.bytes)
    };
    let (before, set) = (place(options), *settings);
    // The page is drawn in front of the input, and the page of the sound gives the
    // running music the volume of the setting word as it is by then (`0x80030A34`).
    if options.up && options.page == options_screen::AUDIO {
        music.audio_page(settings.word);
    }
    let pass = options.pass(settings, pad);
    for &sound in &pass.sounds {
        music.sound(sound.into());
    }
    if place(options) != before || *settings != set || !pass.sounds.is_empty() {
        info!(
            "{who}: pass {passes}: the options: page {}, the cursor on {}{}{}, the setting word {:#010X}, colour {}, vibration {}, sounds {:?}",
            options.page,
            options.cursor.at,
            if options.picking {
                ", an answer is picked"
            } else {
                ""
            },
            match options.refusal {
                0 => String::new(),
                which => format!(", refusal {which} is up"),
            },
            settings.word,
            settings.colour,
            if settings.vibration_off { "off" } else { "on" },
            pass.sounds
        );
    }
    if pass.output_told {
        // The sound library is told one or two loudspeakers (`0x800309E4`).
        info!("{who}: pass {passes}: the options: the output format is written");
        music.tell_output(settings.word);
    }
    pass
}

/// The options screen for a caller without a screen of the menus: the parts, the look
/// and the scroll signs of the PC Settings, read from the disc.
pub(crate) struct View {
    sprites: Vec<Picture>,
    frames: Frames,
    font: Font,
    font_picture: Picture,
    look: Look,
    signs: Vec<Picture>,
}

impl View {
    pub(crate) fn lines(&self) -> &options_panel::Lines {
        self.look.lines()
    }

    pub(crate) fn read() -> Result<Self, Failure> {
        let config = LocalConfig::load()?;
        let mut disc = DiscImage::open(&config.disc_image)?;
        let archive = Archive::open(&mut disc)?;
        let exe = Executable::read(&mut disc)?;
        let fonts = Fonts::from_executable(&exe.image, exe.base)
            .ok_or("the executable does not hold the font tables")?;
        let frames = Frames::from_executable(&exe.image, exe.base)
            .ok_or("the executable does not hold the tables of the frames")?;
        let font = (fonts.get(text::TEXT_FONT).cloned()).ok_or("there is no font of the texts")?;
        let font_set = tim::texture_set(&archive.read(&mut disc, text::FONT_SET)?)
            .ok_or("the font file is not a texture set")?;
        let font_picture = picture(&font_set, font.image)?;
        let sprite_set = tim::texture_set(&archive.read(&mut disc, name::SPRITES)?)
            .ok_or("the sprite file is not a texture set")?;
        let sprites = (sprite_set.iter())
            .map(Picture::of)
            .collect::<Result<Vec<_>, _>>()?;
        let file = archive.read(&mut disc, options_screen::OPTIONS_FILE)?;
        let look = Look::read(&file, &exe)?;
        let signs = scroll_signs(&exe, &font_set)?;
        Ok(Self {
            sprites,
            frames,
            font,
            font_picture,
            look,
            signs,
        })
    }

    /// The screen of a pass, four bytes a pixel: the wall and the options over it, or
    /// the PC Settings while they are up.
    pub(crate) fn screen(
        &self,
        options: &OptionsScreen,
        settings: &Settings,
        time: u32,
        (pc, prefs): (&PcSettings, &Prefs),
    ) -> Vec<u8> {
        let mut screen = [0, 0, 0, 255].repeat(SCREEN.0 * SCREEN.1);
        let parts = Parts {
            sprites: &self.sprites,
            frames: &self.frames,
            font: &self.font,
            font_picture: &self.font_picture,
        };
        parts.wall(&mut screen);
        let pc = Pc {
            settings: pc,
            prefs,
            signs: &self.signs,
        };
        parts.options(&mut screen, &self.look, (options, settings), time, pc);
        screen
    }
}
