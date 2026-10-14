
use bevy::prelude::*;
use jc_disc::{Archive, DiscImage, Executable, LocalConfig};
use jc_formats::carried::{self, Carried};
use jc_formats::chapters::{self, ChapterTable};
use jc_formats::map::{self, Choice, Chosen, Confirmed, Maps, Question, State};
use jc_formats::menu::{self, Cursor, Frames, Rect, Style, sign};
use jc_formats::name;
use jc_formats::options_panel;
use jc_formats::pad::{self, Pad, Repeat};
use jc_formats::rooms::{self, RoomTable};
use jc_formats::text::{self, Font, Fonts};
use jc_formats::tim::{self, Tim};

use crate::music::Music;
use crate::name_entry::{Picture, SCREEN, WHOLE, blend, blit, letter};
use crate::title::Press;

/// The longest stretch of time one pass of the app may stand for.
const LONGEST_PASS: f64 = 0.1;
/// Pictures of the screen in a game frame of the field engine.
const GAME_FRAME: u32 = 2;

type Failure = Box<dyn std::error::Error>;

/// `--map[=MAP] [--chapter=N] [--gate-flag]`, with the buttons of
/// `--press=SECONDS:BUTTON,...` for a hidden run. Without a number the map is the one
/// the chapter's first room leads to.
#[derive(Resource)]
pub struct ShowMap {
    pub map: Option<usize>,
    pub chapter: usize,
    pub gate_flag: bool,
    pub presses: Vec<Press>,
}

/// A script of the room on screen asked for the place map: the room is gone and the
/// map comes up for the place of that room.
#[derive(Resource)]
pub struct OpenMap {
    pub room: u16,
}

/// The buttons of `--map-press=SECONDS:BUTTON,...[;...]` for a hidden run of the game,
/// counted from the moment the map opens: a set for every time it opens, in order, and
/// the last set for every time after that.
#[derive(Resource, Default)]
pub struct MapPresses {
    sets: Vec<Vec<Press>>,
    opened: usize,
}

impl MapPresses {
    /// The value of `--map-press`: sets of `--press` with semicolons between them; a set
    /// may be empty.
    pub fn of(value: &str) -> Option<Self> {
        let sets = (value.split(';'))
            .map(|set| match set {
                "" => Some(Vec::new()),
                set => crate::title::presses(set),
            })
            .collect::<Option<_>>()?;
        Some(Self { sets, opened: 0 })
    }

    /// The presses of the map that opens now.
    fn next(&mut self) -> Vec<Press> {
        let set = self.sets.get(self.opened).or(self.sets.last());
        self.opened += 1;
        set.cloned().unwrap_or_default()
    }
}

/// A place was chosen on the map of a running game: the map is gone and the place's
/// room comes next.
#[derive(Resource)]
pub struct Leaves {
    pub choice: Choice,
}

/// What the screen shows: the map, or the question behind a confirm on a place.
enum Step {
    Map,
    Question {
        chosen: Chosen,
        cursor: Cursor,
        /// The place's picture as the screen holds it.
        backdrop: Vec<u8>,
    },
}

/// The pictures of one map.
struct Pictures {
    /// The map's picture as the screen holds it.
    backdrop: Vec<u8>,
    /// The small pictures and the markers.
    sprites: Vec<Picture>,
}

/// The place map while it is up.
#[derive(Resource)]
pub struct PlaceMap {
    maps: Maps,
    question: Question,
    step: Step,
    /// The screen is a step of a running game: a choice leaves it.
    in_game: bool,
    /// The choice that ends the screen.
    left: Option<Choice>,
    /// The state of the game the map asks: the variant table, the state word and story
    /// flag 1.
    carried: Carried,
    /// The map and entry of this visit and of the last one.
    now: Chosen,
    last: Option<Chosen>,
    /// The cursor record of every map.
    cursors: Vec<Cursor>,
    font: Font,
    font_picture: Picture,
    /// The image of the scroll signs.
    signs: Vec<Option<Picture>>,
    /// The window colour of the game's settings, in which the frames are drawn.
    colour: i16,
    frames: Frames,
    pictures: Pictures,
    pad: Option<Pad>,
    /// Passes of the screen so far, and the share of one that is owed.
    passes: u32,
    owed: f64,
    time: f64,
    presses: Vec<Press>,
    shown: Entity,
    /// The musics the screen's starts have asked for and the sounds its passes have
    /// played, until the sound has taken them.
    asked: Vec<u32>,
    sounds: Vec<u8>,
}

/// An image of a set in its own colours, or in those of the image it shares them with.
pub(crate) fn picture(images: &[Tim], number: usize) -> Result<Picture, Failure> {
    let image = images.get(number).ok_or("the set has no such image")?;
    let palette = image.palette(0).or_else(|| {
        let clut = image.clut.as_ref()?;
        tim::shared_palette(images, clut.rect, 0)
    });
    Ok(Picture {
        rgba: image.to_rgba(palette)?,
        wide: image.width as usize,
        high: image.height as usize,
    })
}

/// A texel under a colour as the console draws a textured primitive: a part of 128
/// leaves the texel as it is.
pub(crate) fn lit(texel: [u8; 3], colour: [u8; 3]) -> [u8; 3] {
    [0, 1, 2].map(|part| {
        let lit = (u32::from(texel[part] >> 3) * u32::from(colour[part])) >> 7;
        let five = lit.min(31) as u8;
        five << 3 | five >> 2
    })
}

/// A picture file's first image as the screen holds it. The picture is moved to the
/// screen as a block: also its texels of no colour.
fn whole_screen(images: &[Tim]) -> Result<Vec<u8>, Failure> {
    let mut backdrop = [0, 0, 0, 255].repeat(SCREEN.0 * SCREEN.1);
    let whole = picture(images, 0)?;
    let size = (whole.wide, whole.high);
    blit(&mut backdrop, &whole, (0, 0), size, (0, 0), WHOLE);
    Ok(backdrop)
}

/// Reads a picture file of the map: a texture set.
fn picture_file(file: u32) -> Result<Vec<Tim>, Failure> {
    let config = LocalConfig::load()?;
    let mut disc = DiscImage::open(&config.disc_image)?;
    let archive = Archive::open(&mut disc)?;
    let bytes = archive.read(&mut disc, file as usize)?;
    Ok(tim::texture_set(&bytes).ok_or("the file is not a texture set")?)
}

/// Reads the two picture files of a map.
fn pictures(map: &map::Map) -> Result<Pictures, Failure> {
    let backdrop = whole_screen(&picture_file(map.background_file)?)?;
    let small = picture_file(map.pictures_file)?;
    let sprites = (0..small.len())
        .map(|number| picture(&small, number))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Pictures { backdrop, sprites })
}

impl PlaceMap {
    fn state(&self) -> State<'_> {
        self.carried.map_state()
    }

    fn cursor(&self) -> Cursor {
        self.cursors[self.now.map]
    }

    /// The line under the cursor, its entry and the entry's name in letters.
    fn under_cursor(&self) -> (usize, usize, String) {
        let cursor = self.cursor();
        let line = (i32::from(cursor.at) + i32::from(cursor.first)).max(0) as usize;
        let entry = (self.maps).entry_of_line(self.now.map, line, &self.state());
        (line, entry, self.name(self.now.map, entry))
    }

    fn name(&self, map: usize, entry: usize) -> String {
        let entry = self.maps.get(map).and_then(|map| map.entries.get(entry));
        entry.map_or(String::new(), |entry| {
            entry.name.iter().map(|&unit| letter(unit)).collect()
        })
    }

    /// Sets the cursor record of this visit's map up as the screen does when it starts.
    fn set_up(&mut self) {
        let kept = self.cursor();
        let cursor = (self.maps).cursor(self.now, self.last, kept, &self.state());
        self.cursors[self.now.map] = cursor;
        // The start of the screen asks for the map's music (`0x800B6E6C`).
        if let Some(map) = self.maps.get(self.now.map) {
            let music = map::music_asked(map.music, self.carried.variable(12));
            self.asked.push(music);
        }
        let lines = self.maps.lines(self.now.map, &self.state());
        let names: Vec<String> = (lines.iter())
            .map(|&entry| self.name(self.now.map, entry))
            .collect();
        info!(
            "the place map: map {} with the player at entry {}, {} lines {names:?}, the cursor on row {} with line {} first",
            self.now.map,
            self.now.entry,
            lines.len(),
            cursor.at,
            cursor.first
        );
    }

    fn text(&self, screen: &mut [u8], units: &[u16], at: (i16, i16), clip: Rect) {
        let cell = (usize::from(self.font.cell.0), usize::from(self.font.cell.1));
        for glyph in self.font.glyphs(&self.font.codes(units), at) {
            if let Some(from) = glyph.cell {
                let from = (usize::from(from.0), usize::from(from.1));
                blit(screen, &self.font_picture, from, cell, glyph.at, clip);
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

    /// A sprite of the map's second file under a colour, solid or in a blend mode.
    fn sprite(
        &self,
        screen: &mut [u8],
        number: u32,
        at: (i16, i16),
        colour: [u8; 3],
        mode: Option<u8>,
    ) {
        let Some(sprite) = self.pictures.sprites.get(number as usize) else {
            return;
        };
        for (u, v) in (0..sprite.high).flat_map(|v| (0..sprite.wide).map(move |u| (u, v))) {
            let texel = (v * sprite.wide + u) * 4;
            if sprite.rgba[texel + 3] == 0 {
                continue;
            }
            let (x, y) = (at.0.wrapping_add(u as i16), at.1.wrapping_add(v as i16));
            let texel = [0, 1, 2].map(|part| sprite.rgba[texel + part]);
            blend(screen, x, y, lit(texel, colour), mode);
        }
    }

    /// A scroll sign: its image around a place, turned and under the pulse's colour.
    fn sign(&self, screen: &mut [u8], turns: u16, at: (i16, i16), time: u32) {
        let sign = usize::try_from(self.colour).ok();
        let Some(Some(image)) = sign.and_then(|colour| self.signs.get(colour)) else {
            return;
        };
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

    /// A text in a frame of the window colour that does not pulse.
    fn framed(&self, screen: &mut [u8], units: &[u16], at: (i16, i16), frame: Rect) {
        let style = Style {
            kind: self.colour,
            mode: 0,
            blend: 1,
        };
        self.frame(screen, frame, style, 0);
        self.text(screen, units, at, WHOLE);
    }

    /// A line of a menu of the screen: its frame, pulsing under the cursor, and its
    /// text in the middle of it.
    fn line(&self, screen: &mut [u8], units: &[u16], rect: Rect, on_cursor: bool, time: u32) {
        let (style, pulses) = Style::of_line(map::LIST_FLAGS, on_cursor, self.colour);
        self.frame(screen, rect, style, if pulses { time } else { 0 });
        let room = rect.inside(map::ROW_MARGIN);
        let width = self.font.length(&self.font.codes(units));
        let at = menu::placed(map::ROW_FLAGS, room, width);
        self.text(screen, units, at, rect);
    }

    /// The screen of the question: the place's picture, its name, the question and the
    /// two answers.
    fn paint_question(&self, chosen: Chosen, cursor: Cursor, backdrop: &[u8]) -> Vec<u8> {
        let mut screen = backdrop.to_vec();
        let time = menu::frame_time(self.passes, GAME_FRAME);
        for (answer, units) in self.question.answers.iter().enumerate() {
            let on_cursor = usize::try_from(cursor.at) == Ok(answer);
            self.line(&mut screen, units, map::answer(answer), on_cursor, time);
        }
        let text = &self.question.text;
        self.framed(&mut screen, text, map::QUESTION_AT, map::QUESTION_FRAME);
        let entry = self.maps.get(chosen.map);
        if let Some(entry) = entry.and_then(|map| map.entries.get(chosen.entry)) {
            let width = self.font.length(&self.font.codes(&entry.name));
            let (at, frame) = map::title(map::QUESTION_NAME, width);
            self.framed(&mut screen, &entry.name, at, frame);
        }
        screen
    }

    /// The screen of this pass, four bytes a pixel. The entries of the windows' table
    /// are drawn from the last to the first, and in one what was added last comes first.
    fn paint(&self) -> Vec<u8> {
        if let Step::Question {
            chosen,
            cursor,
            backdrop,
        } = &self.step
        {
            return self.paint_question(*chosen, *cursor, backdrop);
        }
        let mut screen = self.pictures.backdrop.clone();
        let Some(map) = self.maps.get(self.now.map) else {
            return screen;
        };
        let time = menu::frame_time(self.passes, GAME_FRAME);
        let cursor = self.cursor();
        if cursor.lines > 0 {
            let (_, entry, _) = self.under_cursor();
            if let Some(entry) = map.entries.get(entry) {
                // Entry 10: the small picture. Entry 9: the marker.
                self.sprite(&mut screen, entry.picture, entry.picture_at, [128; 3], None);
                let [from, to] = entry.colours.map(|colour| {
                    let [red, green, blue, _] = colour.to_le_bytes();
                    [red, green, blue]
                });
                let colour = menu::pulse(from, to, time);
                // The marker of a forest map is added to the picture under it.
                let mode = map::marker_blend(self.now.map);
                self.sprite(&mut screen, entry.marker, entry.marker_at, colour, mode);
            }
            // Entry 0: the scroll signs, the list, then the title.
            let (above, below) = cursor.hides();
            let places = map::signs(cursor.rows);
            if below {
                self.sign(&mut screen, sign::BELOW, places[1], time);
            }
            if above {
                self.sign(&mut screen, sign::ABOVE, places[0], time);
            }
            for row in 0..cursor.rows {
                let line = (i32::from(cursor.first) + i32::from(row)).max(0) as usize;
                let entry = (self.maps).entry_of_line(self.now.map, line, &self.state());
                let Some(entry) = map.entries.get(entry) else {
                    continue;
                };
                let on_cursor = row == cursor.at;
                self.line(&mut screen, &entry.name, map::row(row), on_cursor, time);
            }
        }
        let width = self.font.length(&self.font.codes(&map.title));
        let (at, frame) = map::title(map.title_at, width);
        self.framed(&mut screen, &map.title, at, frame);
        screen
    }

    /// A place is chosen: what the choice writes, and the screen leaves. Alone, the
    /// view has nowhere to go and shows the map again.
    fn choose(&mut self, chosen: Chosen, how: &str) {
        let choice = self.maps.choice(chosen);
        info!(
            "the place map: pass {}: {} of map {} {how}: {choice:?}",
            self.passes,
            self.name(chosen.map, chosen.entry),
            chosen.map
        );
        self.step = Step::Map;
        match self.in_game {
            true => self.left = choice,
            false => self.set_up(),
        }
    }

    /// A confirm on the cursor's line: another map comes up, or a place is picked.
    fn confirm(&mut self, at_once: bool) -> Result<(), Failure> {
        let (line, entry, name) = self.under_cursor();
        let map = self.now.map;
        let stored = Chosen { map, entry };
        self.last = Some(stored);
        self.now = stored;
        match self.maps.confirm(map, line, &self.state()) {
            Confirmed::Map(chosen) => {
                info!(
                    "the place map: pass {}: line {line} ({name}) brings up map {}",
                    self.passes, chosen.map
                );
                self.now = chosen;
                let map = self.maps.get(chosen.map).ok_or("there is no such map")?;
                self.pictures = pictures(map)?;
                self.set_up();
            }
            Confirmed::Place(chosen) if at_once => self.choose(chosen, "is chosen at once"),
            Confirmed::Place(chosen) => {
                let entry = self.maps.get(chosen.map);
                let entry = entry.and_then(|map| map.entries.get(chosen.entry));
                let file = entry.ok_or("there is no such entry")?.large_file;
                info!(
                    "the place map: pass {}: line {line} ({name}) of map {} is asked about, with the picture of file {file}",
                    self.passes, chosen.map
                );
                self.step = Step::Question {
                    chosen,
                    cursor: map::QUESTION_CURSOR,
                    backdrop: whole_screen(&picture_file(file)?)?,
                };
            }
        }
        Ok(())
    }

    /// One pass of the question with the pad's words.
    fn pass_question(&mut self, pad: &Pad) {
        let Step::Question { chosen, cursor, .. } = &mut self.step else {
            return;
        };
        let chosen = *chosen;
        let before = *cursor;
        let moved = cursor.step_with(pad.repeated, pad.held);
        let at = cursor.at;
        // The cursor routine, the confirm and the cancel play their sounds; the cancel
        // is only asked without a confirm.
        let asked = match (menu::confirmed(pad), menu::cancelled(pad)) {
            (true, _) => Some(name::SOUND_CONFIRM),
            (false, true) => Some(name::SOUND_CANCEL),
            _ => None,
        };
        (self.sounds).extend(
            moved
                .sounds
                .then_some(name::SOUND_MOVE)
                .into_iter()
                .chain(asked),
        );
        if at != before.at {
            info!(
                "the place map: pass {}: the question's cursor on answer {at}",
                self.passes
            );
        }
        let confirmed = menu::confirmed(pad);
        if confirmed && at == map::ANSWER_YES {
            self.choose(chosen, "is gone to");
        } else if confirmed || menu::cancelled(pad) {
            // The screen starts again with the map and entry the confirm stored.
            info!(
                "the place map: pass {}: the answer is no, the map comes back",
                self.passes
            );
            self.step = Step::Map;
            self.set_up();
        }
    }

    /// One pass of the screen with the pad's words.
    fn pass(&mut self, pad: &Pad) -> Result<(), Failure> {
        if matches!(self.step, Step::Question { .. }) {
            self.pass_question(pad);
            return Ok(());
        }
        let map = self.now.map;
        if self.cursors[map].lines == 0 {
            return Ok(());
        }
        let before = self.cursors[map];
        let moved = self.cursors[map].step_with(pad.repeated, pad.held);
        if self.cursors[map] != before {
            let (line, entry, name) = self.under_cursor();
            info!(
                "the place map: pass {}: the cursor on row {} with line {} first: line {line}, entry {entry} ({name}), answer {}, sound {}",
                self.passes,
                self.cursors[map].at,
                self.cursors[map].first,
                moved.answer,
                moved.sounds
            );
        }
        self.sounds.extend(moved.sounds.then_some(name::SOUND_MOVE));
        if menu::confirmed(pad) {
            self.sounds.push(name::SOUND_CONFIRM);
            self.confirm(false)?;
        } else if pad.pressed & pad::SQUARE != 0 {
            self.confirm(true)?;
        }
        Ok(())
    }
}

/// Reads what the screen needs from the disc and sets it up as its start leaves it:
/// for the map that is asked for, or else for the place of `room`, or else for the place
/// of the chapter's first room. `carried` is the state of a running game; without one
/// the options stand in for the story so far.
fn open(
    show: &ShowMap,
    room: Option<u16>,
    shown: Entity,
    carried: Option<&Carried>,
) -> Result<PlaceMap, Failure> {
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
    // The scroll sign of every window colour.
    let signs = (0..options_panel::COLOURS)
        .map(|colour| {
            sign::image(&exe.image, exe.base, colour, sign::OF_LIST)
                .and_then(|number| picture(&font_set, number).ok())
        })
        .collect();
    let file = archive.read(&mut disc, map::MAP_FILE)?;
    let maps = Maps::from_file(&file)?;
    let question = Question::from_file(&file)?;
    // The story so far, where no game runs: the variant lists of the chapters up to this
    // one and the state word of the chapter's record, with story flag 1 as asked.
    let engine = archive.read(&mut disc, rooms::FIELD_ENGINE_FILE)?;
    let chapter_file = archive.read(&mut disc, chapters::CHAPTER_FILE)?;
    let table = ChapterTable::from_files(&chapter_file, &engine)?;
    let carried = match carried {
        Some(carried) => carried.clone(),
        None => {
            let mut carried =
                Carried::at_chapter(show.chapter, table.iter()).ok_or("no such chapter")?;
            if show.gate_flag {
                carried.keeping.set_flag(carried::GATE_FLAG);
            }
            carried
        }
    };
    let chapter = table.get(carried.chapter).ok_or("no such chapter")?;
    let state = carried.map_state();
    let now = match show.map {
        Some(map) => Chosen { map, entry: 0 },
        None => {
            let rooms = RoomTable::from_field_engine(&engine)?;
            let (location, _) = room
                .or_else(|| u16::try_from(chapter.start_room).ok())
                .and_then(|room| rooms.by_number(room))
                .ok_or("the room is not in the room table")?;
            maps.of_location(location.area as i16, location.place as i16, &state)
        }
    };
    let pictures = pictures(maps.get(now.map).ok_or("there is no such map")?)?;
    // A map's cursor record as the field engine's file holds it.
    let kept = Cursor {
        columns: 1,
        rows: map::ROWS as i16,
        lines: 0,
        flags: menu::cursor::SCROLLS,
        at: 0,
        first: 0,
    };
    let mut current = PlaceMap {
        cursors: vec![kept; maps.maps.len()],
        maps,
        question,
        step: Step::Map,
        in_game: room.is_some(),
        left: None,
        carried,
        now,
        last: None,
        font,
        font_picture,
        signs,
        colour: 0,
        frames,
        pictures,
        pad: None,
        passes: 0,
        owed: 0.0,
        time: 0.0,
        presses: show.presses.clone(),
        shown,
        asked: Vec::new(),
        sounds: Vec::new(),
    };
    current.set_up();
    Ok(current)
}

/// `--map`: the screen alone.
pub fn show_place_map(
    show: Res<ShowMap>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    commands.spawn(Camera2d);
    let (wide, high) = (SCREEN.0 as u32, SCREEN.1 as u32);
    let black = [0, 0, 0, 255].repeat(SCREEN.0 * SCREEN.1);
    let shown = crate::spawn_picture(&mut commands, &mut images, wide, high, black);
    match open(&show, None, shown, None) {
        Ok(current) => commands.insert_resource(current),
        Err(err) => {
            error!("the place map: {err}");
            exit.write(AppExit::error());
        }
    }
}

/// The pad's word of held buttons from the keyboard, or from the presses of a hidden run.
fn buttons(current: &PlaceMap, keys: &ButtonInput<KeyCode>) -> u16 {
    let mut held = crate::keys::held(keys);
    for press in &current.presses {
        if press.holds(current.time) {
            held |= press.button;
        }
    }
    held
}

/// The map comes up in a run of the game, for the place of the room a script asked in.
fn open_in_game(
    asked: &OpenMap,
    carried: &Carried,
    presses: Option<&mut MapPresses>,
    (images, commands, exit): (
        &mut Assets<Image>,
        &mut Commands,
        &mut MessageWriter<AppExit>,
    ),
) {
    commands.remove_resource::<OpenMap>();
    commands.spawn(Camera2d);
    let (wide, high) = (SCREEN.0 as u32, SCREEN.1 as u32);
    let black = [0, 0, 0, 255].repeat(SCREEN.0 * SCREEN.1);
    let shown = crate::spawn_picture(commands, images, wide, high, black);
    // The chapter and story flag 1 are the running game's.
    let show = ShowMap {
        map: None,
        chapter: carried.chapter,
        gate_flag: false,
        presses: presses.map(MapPresses::next).unwrap_or_default(),
    };
    match open(&show, Some(asked.room), shown, Some(carried)) {
        Ok(current) => {
            let state = current.state();
            info!(
                "the place map: map {} for room {}, with the state word {}, {} variants and story flag 1 {}",
                current.now.map,
                asked.room,
                state.word,
                state.variants.entries().len(),
                if state.gate_flag { "set" } else { "not set" }
            );
            commands.insert_resource(current)
        }
        Err(err) => {
            error!("the place map: {err}");
            exit.write(AppExit::error());
        }
    }
}

fn hear(current: &mut PlaceMap, music: &mut Music) {
    for number in current.asked.drain(..) {
        if let Err(err) = music.ask(number as usize) {
            error!("the place map: music {number}: {err}");
        }
    }
    for sound in current.sounds.drain(..) {
        music.sound(sound.into());
    }
}

/// Runs the passes of the screen for the time gone by and paints the last one. In a run
/// of the game the screen opens when a room's script has asked for it, and a choice
/// takes it away again.
#[allow(clippy::too_many_arguments)]
pub fn run_place_map(
    current: Option<ResMut<PlaceMap>>,
    asked: Option<Res<OpenMap>>,
    game: Option<Res<crate::Game>>,
    mut presses: Option<ResMut<MapPresses>>,
    mut music: ResMut<Music>,
    settings: Res<crate::settings::Settings>,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut images: ResMut<Assets<Image>>,
    mut sprites: Query<&mut Sprite>,
    cameras: Query<Entity, With<Camera2d>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(mut current) = current else {
        if let Some(asked) = asked {
            let presses = presses.as_deref_mut();
            let carried = game.map_or_else(Carried::new_game, |game| game.0.clone());
            let drawing = (&mut *images, &mut commands, &mut exit);
            open_in_game(&asked, &carried, presses, drawing);
        }
        return;
    };
    hear(&mut current, &mut music);
    let colour = settings.0.colour as i16;
    if current.colour != colour {
        current.colour = colour;
    }
    let passed = time.delta_secs_f64().min(LONGEST_PASS);
    current.time += passed;
    current.owed += passed * f64::from(map::PASSES_A_SECOND);
    let held = buttons(&current, &keys);
    // The screen clears the pad's repeat counters before its first pass, and the pad
    // routine has run in every pass before: a button that is still down from the room
    // did not go down on the screen.
    current.pad.get_or_insert_with(|| {
        let mut pad = Pad::default();
        pad.read(held, Repeat::of(GAME_FRAME), 0);
        pad
    });
    let mut painted = current.passes == 0;
    while current.owed >= 1.0 {
        current.owed -= 1.0;
        current.passes += 1;
        painted = true;
        let mut pad = current.pad.unwrap_or_default();
        pad.read(held, Repeat::of(GAME_FRAME), current.passes);
        current.pad = Some(pad);
        if let Err(err) = current.pass(&pad) {
            error!("the place map: {err}");
            exit.write(AppExit::error());
            return;
        }
        hear(&mut current, &mut music);
        if let Some(choice) = current.left {
            // The picture is switched off and the field goes back to its rooms.
            commands.entity(current.shown).despawn();
            for camera in &cameras {
                commands.entity(camera).despawn();
            }
            commands.remove_resource::<PlaceMap>();
            commands.insert_resource(Leaves { choice });
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
