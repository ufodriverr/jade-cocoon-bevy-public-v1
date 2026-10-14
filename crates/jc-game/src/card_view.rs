
use std::path::PathBuf;

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use jc_disc::{Archive, DiscImage, Executable, LocalConfig};
use jc_formats::card_panel::{self, Drawn, Sizes};
use jc_formats::card_screen::{
    self, Card, CardScreen, Cards, Game, Kind, SLOTS, Shape, Tables, state,
};
use jc_formats::menu::{self, Frames, Style};
use jc_formats::options_screen::Settings;
use jc_formats::pad::{Pad, Repeat};
use jc_formats::player_panel;
use jc_formats::text::{self, Font, Fonts};
use jc_formats::{name, save, tim};

use crate::music::Music;
use crate::name_entry::{Picture, SCREEN, WHOLE};
use crate::options_view::{self, Parts};
use crate::place_map::picture;
use crate::title::Press;

type Failure = Box<dyn std::error::Error>;

/// Passes of the screen's task in a second: a game frame of the field, two pictures of
/// the screen long.
const PASSES_A_SECOND: f64 = 25.0;
const GAME_FRAME: u32 = 2;
/// The longest stretch of time one pass of the app may stand for.
const LONGEST_PASS: f64 = 0.1;
/// The layer of what stands over a room, as the player's menu has it.
const OVER_THE_ROOM: usize = 4;
/// How far in front of the title's pictures the screen of "Continue" lies.
const IN_FRONT_OF_THE_TITLE: f32 = 5.0;
/// The unit of the digit 0 in a text.
const ZERO: u16 = 0x4F82;

/// `--card` (the screen of a room's object) or `--card=continue` (the title's), with
/// the buttons of `--press=SECONDS:BUTTON,...` and the folder of `--card-dir=DIR`.
#[derive(Resource)]
pub struct ShowCard {
    pub of_title: bool,
    pub presses: Vec<Press>,
    pub folder: PathBuf,
    pub told: Told,
    pub shape: Shape,
}

pub fn shape_of(args: &[String]) -> Shape {
    match args
        .iter()
        .find_map(|arg| arg.strip_prefix("--card-screen="))
    {
        None => Shape::Plain,
        Some("original") => Shape::Original,
        Some(other) => panic!("--card-screen={other}: takes original"),
    }
}

/// Where the cards of the build are kept (`--card-dir=DIR`, by default `saves`), and
/// the buttons that are tapped on a screen over a room in a hidden run, counted from the
/// moment it opens (`--card-press=SECONDS:BUTTON,...`).
#[derive(Resource)]
pub struct CardsKept {
    pub folder: PathBuf,
    pub presses: Vec<Press>,
    pub told: Told,
    pub shape: Shape,
}

/// What draws the screen over a room.
#[derive(Component)]
pub struct OverTheRoom;

/// The game of the check: it saves a new game and takes what the reader takes.
struct Checked {
    buffer: Vec<u8>,
    took: Option<save::Head>,
}

impl Game for Checked {
    fn write(&mut self) -> Vec<u8> {
        self.buffer.clone()
    }

    fn read(&mut self, buffer: &[u8]) -> bool {
        if !save::Memory::zeroed().read(buffer, false) {
            return false;
        }
        self.took = save::Head::read(buffer);
        true
    }
}

/// The game behind the screen: the check's, or the running game of a room.
enum Played {
    Checked(Checked),
    Running(Box<crate::saving::Running>),
}

impl Played {
    fn took(&self) -> Option<&save::Head> {
        match self {
            Self::Checked(game) => game.took.as_ref(),
            Self::Running(game) => game.took.as_ref(),
        }
    }
}

impl Game for Played {
    fn write(&mut self) -> Vec<u8> {
        match self {
            Self::Checked(game) => game.write(),
            Self::Running(game) => game.write(),
        }
    }

    fn read(&mut self, buffer: &[u8]) -> bool {
        match self {
            Self::Checked(game) => game.read(buffer),
            Self::Running(game) => game.read(buffer),
        }
    }
}

/// The screen with what it is painted by.
#[derive(Resource)]
pub struct CardView {
    tables: Tables,
    sprites: Vec<Picture>,
    frames: Frames,
    font: Font,
    font_picture: Picture,
    small: Font,
    small_picture: Picture,
    signs: Vec<Picture>,
    /// The pictures of the busy sign, one for each step of its table.
    busy: Vec<Picture>,
    legend: Vec<(usize, Vec<u16>)>,
    screen: CardScreen,
    cards: Held,
    game: Played,
    folder: PathBuf,
    presses: Vec<Press>,
    /// None until the pass in front of the screen's first has read the buttons.
    pad: Option<Pad>,
    /// The screen stands over a room, which goes on when it leaves.
    over_room: bool,
    passes: u32,
    owed: f64,
    time: f64,
    shown: Entity,
    over: bool,
}

/// The file of a slot in the folder of the cards.
fn file_of(folder: &std::path::Path, port: usize, slot: usize) -> PathBuf {
    folder
        .join(format!("card{}", port + 1))
        .join(format!("save{slot:02}.dat"))
}

/// The cards as the folder has them.
fn read_cards(folder: &std::path::Path) -> Cards {
    let mut cards = Cards::default();
    for (port, files) in cards.files.iter_mut().enumerate() {
        for (slot, file) in files.iter_mut().enumerate() {
            *file = std::fs::read(file_of(folder, port, slot)).ok();
        }
    }
    cards
}

/// The screen for a PC has one list, the first card's. The saves of the second card's
/// folder are moved into slots of the first that are free, each to the lowest one, so
/// that none is lost from sight; nothing is written over, and a save that finds no
/// free slot stays where it is (the original screen still shows it). Gives what was
/// moved, as the slots it came from and went to.
fn gather_saves(folder: &std::path::Path) -> std::io::Result<Vec<(usize, usize)>> {
    let mut moved = Vec::new();
    let mut free = (0..SLOTS).filter(|&slot| !file_of(folder, 0, slot).exists());
    for slot in 0..SLOTS {
        let from = file_of(folder, 1, slot);
        if !from.is_file() {
            continue;
        }
        let Some(to) = free.next() else {
            break;
        };
        let into = file_of(folder, 0, to);
        if let Some(parent) = into.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&from, &into)?;
        moved.push((slot, to));
    }
    Ok(moved)
}

const SQUARE_SIGN: usize = 94;

/// A deleted save is put away, not removed: its file is moved into the folder
/// `deleted` of the saves under a name that is not there yet. Gives where it is now.
fn put_away(folder: &std::path::Path, port: usize, slot: usize) -> std::io::Result<PathBuf> {
    let deleted = folder.join("deleted");
    std::fs::create_dir_all(&deleted)?;
    let named = |n: u32| deleted.join(format!("card{}-save{slot:02}-{n:03}.dat", port + 1));
    let to = (1..=999).map(named).find(|path| !path.exists());
    let to = to.ok_or_else(|| std::io::Error::other("the folder of deleted saves is full"))?;
    std::fs::rename(file_of(folder, port, slot), &to)?;
    Ok(to)
}

/// Writes the files that are not in the folder as the cards have them.
fn keep_cards(folder: &std::path::Path, cards: &Cards) -> std::io::Result<usize> {
    let mut written = 0;
    for (port, files) in cards.files.iter().enumerate() {
        for (slot, file) in files.iter().enumerate() {
            let path = file_of(folder, port, slot);
            let Some(file) = file else {
                continue;
            };
            if std::fs::read(&path).is_ok_and(|kept| kept == *file) {
                continue;
            }
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, file)?;
            written += 1;
        }
    }
    Ok(written)
}

/// What a check tells the screen of its cards in place of what the folder says
/// (`--card-told=gone|unformatted|format-fails|full|write-fails|read-fails`): the build's cards
/// are always there, formatted and never full of other games, so the messages for
/// those cases show only this way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Told {
    #[default]
    AsKept,
    /// No card in either port.
    Gone,
    /// A card that is not formatted, and a formatting that works.
    Unformatted,
    /// The same with a formatting that fails.
    FormatFails,
    /// Files of other games take every block that the saves leave.
    Full,
    /// Every write fails.
    WriteFails,
    /// The heads of the saves are read, and every read behind them fails.
    ReadFails,
}

impl Told {
    /// The value of `--card-told=` among the arguments.
    pub fn of(args: &[String]) -> Self {
        let told = args.iter().find_map(|arg| arg.strip_prefix("--card-told="));
        match told {
            None => Self::AsKept,
            Some("gone") => Self::Gone,
            Some("unformatted") => Self::Unformatted,
            Some("format-fails") => Self::FormatFails,
            Some("full") => Self::Full,
            Some("write-fails") => Self::WriteFails,
            Some("read-fails") => Self::ReadFails,
            Some(other) => panic!(
                "--card-told={other}: takes gone, unformatted, format-fails, full, write-fails or read-fails"
            ),
        }
    }
}

/// The cards of the folder with what a check tells of them.
struct Held {
    cards: Cards,
    folder: PathBuf,
    told: Told,
    formatted: bool,
    /// The reads so far.
    reads: usize,
}

impl Card for Held {
    fn present(&mut self, _port: u8) -> i32 {
        i32::from(self.told == Told::Gone)
    }

    fn check(&mut self, _port: u8) -> i32 {
        match self.told {
            Told::Gone => 1,
            Told::Unformatted | Told::FormatFails if !self.formatted => 4,
            _ => 0,
        }
    }

    fn format(&mut self, _port: u8) -> i32 {
        if self.told == Told::FormatFails {
            return 2;
        }
        self.formatted = true;
        0
    }

    fn blocks(&self, port: u8) -> i32 {
        match self.told {
            Told::Full => SLOTS as i32,
            _ => self.cards.blocks(port),
        }
    }

    fn has(&self, port: u8, slot: usize) -> bool {
        self.cards.has(port, slot)
    }

    fn read(&mut self, port: u8, slot: usize) -> Result<Vec<u8>, i32> {
        self.reads += 1;
        if self.told == Told::ReadFails && self.reads > self.cards.blocks(port) as usize {
            return Err(1);
        }
        self.cards.read(port, slot)
    }

    fn write(&mut self, port: u8, slot: usize, buffer: &[u8]) -> i32 {
        match self.told {
            Told::WriteFails => 1,
            _ => self.cards.write(port, slot, buffer),
        }
    }

    fn remove(&mut self, port: u8, slot: usize) -> i32 {
        match put_away(&self.folder, usize::from(port), slot) {
            Ok(to) => {
                info!(
                    "the save screen: the save of slot {slot} is deleted: its file is moved to {}",
                    to.display()
                );
                self.cards.remove(port, slot)
            }
            Err(err) => {
                error!("the save screen: the save of slot {slot} is not deleted: {err}");
                1
            }
        }
    }
}

/// The sizes the drawing asks for.
struct Sized<'a>(Parts<'a>);

impl Sizes for Sized<'_> {
    fn sprite(&self, number: usize) -> i16 {
        self.0.width(number)
    }

    fn text(&self, units: &[u16]) -> i16 {
        self.0.font.length(&self.0.font.codes(units))
    }
}

impl CardView {
    /// The screen of `kind` over the cards of `folder`, with `game` behind it; the
    /// game of the check when there is none.
    fn read(
        kind: Kind,
        (folder, presses, told, shape): (&std::path::Path, &[Press], Told, Shape),
        game: Option<crate::saving::Running>,
        settings: Settings,
        shown: Entity,
    ) -> Result<Self, Failure> {
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
        let font_set = tim::texture_set(&archive.read(&mut disc, text::FONT_SET)?)
            .ok_or("the font file is not a texture set")?;
        let (font_picture, small_picture) = (
            picture(&font_set, font.image)?,
            picture(&font_set, small.image)?,
        );
        let signs = options_view::scroll_signs(&exe, &font_set)?;
        let busy = card_panel::busy_images(&exe.image, exe.base)
            .ok_or("the executable does not hold the table of the busy sign")?;
        let busy = (busy.iter())
            .map(|&image| picture(&font_set, image))
            .collect::<Result<Vec<_>, _>>()?;
        let sprite_set = tim::texture_set(&archive.read(&mut disc, name::SPRITES)?)
            .ok_or("the sprite file is not a texture set")?;
        let sprites = (sprite_set.iter())
            .map(Picture::of)
            .collect::<Result<Vec<_>, _>>()?;
        let file = archive.read(&mut disc, save::OVERLAY)?;
        let tables = Tables::from_file(&file, save::BASE)
            .ok_or("file 48 does not hold the tables of the memory card screen")?;
        let legend = (tables.legend.iter())
            .map(|(sprite, word)| (*sprite as usize, word.clone()))
            .collect();
        let over_room = game.is_some();
        let game = match game {
            Some(game) => Played::Running(Box::new(game)),
            None => Played::Checked(Checked {
                buffer: crate::saving::new_game(settings)?,
                took: None,
            }),
        };
        if shape == Shape::Plain {
            match gather_saves(folder) {
                Ok(moved) if moved.is_empty() => {}
                Ok(moved) => info!(
                    "the save screen: the saves of card2 in {} are moved into free slots of card1 (from slot, to slot, counted from 0): {moved:?}",
                    folder.display()
                ),
                Err(err) => warn!("the save screen: a save of card2 is not moved: {err}"),
            }
        }
        let cards = Held {
            cards: read_cards(folder),
            folder: folder.to_path_buf(),
            told,
            formatted: false,
            reads: 0,
        };
        let screen = match shape {
            Shape::Original => CardScreen::open(kind, tables.cursors),
            Shape::Plain => CardScreen::open_plain(kind, tables.cursors, &cards),
        };
        Ok(Self {
            screen,
            cards,
            game,
            tables,
            sprites,
            frames,
            font,
            font_picture,
            small,
            small_picture,
            signs,
            busy,
            legend,
            folder: folder.to_path_buf(),
            presses: presses.to_vec(),
            pad: None,
            over_room,
            passes: 0,
            owed: 0.0,
            time: 0.0,
            shown,
            over: false,
        })
    }

    fn parts(&self) -> Parts<'_> {
        Parts {
            sprites: &self.sprites,
            frames: &self.frames,
            font: &self.font,
            font_picture: &self.font_picture,
        }
    }

    /// A text in the small font.
    fn small(&self, screen: &mut [u8], units: &[u16], at: (i16, i16), clip: menu::Rect) {
        options_view::glyphs(screen, &self.small, &self.small_picture, units, at, clip);
    }

    /// One piece of the screen; `colour` is the window colour of its frames.
    fn put(&self, screen: &mut [u8], colour: i16, drawn: Drawn) {
        let parts = self.parts();
        let whole = |clip: Option<menu::Rect>| clip.unwrap_or(WHOLE);
        match drawn {
            Drawn::Frame { rect, mode, time } => {
                let style = Style {
                    kind: colour,
                    mode,
                    blend: 1,
                };
                parts.frame(screen, rect, style, time);
            }
            Drawn::Sprite { number, at, clip } => parts.sprite(screen, number, at, whole(clip)),
            Drawn::Digits { value, ends, clip } => {
                let width = |number: usize| parts.width(number);
                for (number, at) in player_panel::long_digits(value, ends, width) {
                    parts.sprite(screen, number, at, whole(clip));
                }
            }
            Drawn::Number { value, ends, clip } => {
                // The digits of the small font, so that the last one ends at the place.
                let units: Vec<u16> = (value.max(0).to_string().bytes())
                    .map(|digit| ZERO + (u16::from(digit - b'0') << 8))
                    .collect();
                let long = self.small.length(&self.small.codes(&units));
                self.small(screen, &units, (ends.0 - long, ends.1), whole(clip));
            }
            Drawn::Text {
                units,
                font,
                at,
                clip,
            } => match font {
                text::SMALL_FONT => self.small(screen, units, at, whole(clip)),
                _ => parts.text(screen, units, at, whole(clip)),
            },
            Drawn::Name { name, at, clip } => {
                let mut stored = [0; name::LETTERS];
                stored.copy_from_slice(&name[..name::LETTERS]);
                let units: Vec<u16> = (name::units(&stored, &self.small).into_iter())
                    .map(player_panel::in_capitals)
                    .collect();
                self.small(screen, &units, at, whole(clip));
            }
            Drawn::Place { text, at, clip } => {
                self.small(screen, &text::units(&text), at, whole(clip));
            }
            Drawn::Sign { turns, at, time } => {
                if let Some(sign) = self.signs.get(colour.max(0) as usize) {
                    options_view::put_sign(screen, sign, turns, at, time);
                }
            }
        }
    }

    /// The screen of a pass, four bytes a pixel: the wall, the pieces and the legend.
    fn paint(&self, colour: i16) -> Vec<u8> {
        let mut screen = [0, 0, 0, 255].repeat(SCREEN.0 * SCREEN.1);
        let parts = self.parts();
        parts.wall(&mut screen);
        let time = menu::frame_time(self.passes, GAME_FRAME);
        for drawn in card_panel::drawn(&self.tables, &self.screen, time, &Sized(parts)) {
            self.put(&mut screen, colour, drawn);
        }
        // On the saves of the screen for a PC the legend names the square too.
        let on_saves = self.screen.shape == Shape::Plain && self.screen.state == state::SLOTS;
        match on_saves {
            true => {
                let mut legend = self.legend.clone();
                legend.push((SQUARE_SIGN, card_screen::DELETE_WORD.to_vec()));
                parts.legend(&mut screen, colour, &legend);
            }
            false => parts.legend(&mut screen, colour, &self.legend),
        }
        // The busy sign lies in slot 0, on top of everything. Stand-in: its step goes
        // by the screen's own passes, the game's by the main loop's count.
        let step = card_panel::busy(&self.screen, self.passes);
        if let Some(sign) = step.and_then(|step| self.busy.get(step)) {
            let size = (sign.wide, sign.high);
            crate::name_entry::blit(&mut screen, sign, (0, 0), size, card_panel::BUSY_AT, WHOLE);
        }
        screen
    }
}

impl CardView {
    /// Whether the screen stands over a room, whose game frames do not run then: none
    /// of the room's tasks is passed while the card's task has the field
    /// (`movie_view::room_runs`).
    pub fn holds_the_room(&self) -> bool {
        self.over_room
    }
}

pub fn over_the_room(
    kind: Kind,
    game: crate::saving::Running,
    kept: &CardsKept,
    settings: Settings,
    images: &mut Assets<Image>,
    commands: &mut Commands,
) -> Result<(), Failure> {
    let (wide, high) = (SCREEN.0 as u32, SCREEN.1 as u32);
    let black = [0, 0, 0, 255].repeat(SCREEN.0 * SCREEN.1);
    let shown = crate::spawn_picture(commands, images, wide, high, black);
    let cards = (
        kept.folder.as_path(),
        &kept.presses[..],
        kept.told,
        kept.shape,
    );
    let view = match CardView::read(kind, cards, Some(game), settings, shown) {
        Ok(view) => view,
        Err(err) => {
            commands.entity(shown).despawn();
            return Err(err);
        }
    };
    if matches!(kind, Kind::Continue) {
        // The title's own camera draws the screen, in front of the title's pictures.
        let factor = (crate::WINDOW_SIZE.0 / wide)
            .min(crate::WINDOW_SIZE.1 / high)
            .max(1);
        let in_front = Transform::from_scale(Vec3::splat(factor as f32))
            .with_translation(Vec3::new(0.0, 0.0, IN_FRONT_OF_THE_TITLE));
        commands.entity(shown).insert((OverTheRoom, in_front));
        commands.insert_resource(view);
        return Ok(());
    }
    let layer = RenderLayers::layer(OVER_THE_ROOM);
    commands.entity(shown).insert((OverTheRoom, layer.clone()));
    commands.spawn((
        OverTheRoom,
        Camera2d,
        Camera {
            order: 1,
            clear_color: ClearColorConfig::Custom(Color::BLACK),
            ..default()
        },
        // Two cameras of one window must agree on this, and the room's has none.
        Msaa::Off,
        layer,
    ));
    commands.insert_resource(view);
    Ok(())
}

/// `--card`: the screen alone.
pub fn show_card(
    show: Res<ShowCard>,
    settings: Res<crate::settings::Settings>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    commands.spawn(Camera2d);
    let (wide, high) = (SCREEN.0 as u32, SCREEN.1 as u32);
    let black = [0, 0, 0, 255].repeat(SCREEN.0 * SCREEN.1);
    let shown = crate::spawn_picture(&mut commands, &mut images, wide, high, black);
    let kind = match show.of_title {
        true => Kind::Continue,
        false => Kind::Room,
    };
    let kept = (
        show.folder.as_path(),
        &show.presses[..],
        show.told,
        show.shape,
    );
    match CardView::read(kind, kept, None, settings.0, shown) {
        Ok(view) => {
            let saves = view
                .cards
                .cards
                .files
                .each_ref()
                .map(|files| files.iter().flatten().count());
            info!(
                "the memory card screen: starts in state {} on line {} of its first list, the folder {} holds {saves:?} saves of {SLOTS} a card",
                view.screen.state,
                view.screen.line(),
                view.folder.display()
            );
            commands.insert_resource(view);
        }
        Err(err) => {
            error!("the memory card screen: {err}");
            exit.write(AppExit::error());
        }
    }
}

/// Runs the passes of the screen for the time gone by and paints the last one.
#[allow(clippy::too_many_arguments)]
pub fn run_card(
    view: Option<ResMut<CardView>>,
    mut music: ResMut<Music>,
    settings: Res<crate::settings::Settings>,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut images: ResMut<Assets<Image>>,
    mut sprites: Query<&mut Sprite>,
    clock: Res<crate::saving::PlayClock>,
    over_room: Query<Entity, With<OverTheRoom>>,
    mut commands: Commands,
) {
    let Some(mut view) = view else {
        return;
    };
    if view.over {
        return;
    }
    let passed = time.delta_secs_f64().min(LONGEST_PASS);
    view.time += passed;
    view.owed += passed * PASSES_A_SECOND;
    let mut held = crate::keys::held(&keys);
    for press in &view.presses {
        if press.holds(view.time) {
            held |= press.button;
        }
    }
    // The pad routine has run in every pass before the screen's first: a button that
    // is still down from the room did not go down on the screen.
    view.pad.get_or_insert_with(|| {
        let mut pad = Pad::default();
        pad.read(held, Repeat::of(GAME_FRAME), 0);
        pad
    });
    // The play clock counts on while the screen is up.
    if let Played::Running(game) = &mut view.game {
        game.at(&clock);
    }
    let mut painted = view.passes == 0;
    while view.owed >= 1.0 && !view.over {
        view.owed -= 1.0;
        painted = true;
        view.passes += 1;
        let view = &mut *view;
        let mut pad = view.pad.unwrap_or_default();
        pad.read(held, Repeat::of(GAME_FRAME), view.passes);
        view.pad = Some(pad);
        let before = (view.screen.state, view.screen.routine, view.screen.slot());
        let pass = view.screen.pass(&pad, &mut view.cards, &mut view.game);
        for &sound in &pass.sounds {
            music.sound(sound.into());
        }
        let now = (view.screen.state, view.screen.routine, view.screen.slot());
        if now != before || !pass.sounds.is_empty() {
            info!(
                "the memory card screen: pass {}: state {}, {:?}, line {} of the first list, port {}, slot {}, {} heads read, {} free blocks, {} saves, message {:?}, sounds {:?}",
                view.passes,
                view.screen.state,
                view.screen.routine,
                view.screen.line(),
                view.screen.port(),
                view.screen.slot(),
                view.screen.count,
                view.screen.free,
                view.screen.saves,
                view.screen.message(),
                pass.sounds
            );
        }
        match keep_cards(&view.folder, &view.cards.cards) {
            Ok(0) => {}
            Ok(written) => info!(
                "the memory card screen: pass {}: {written} file written to {}",
                view.passes,
                view.folder.display()
            ),
            Err(err) => error!("the memory card screen: a save is not kept: {err}"),
        }
        if let Some(word) = pass.ended {
            let took = view.game.took().map(|head| {
                format!(
                    "level {}, HP {} of {}, {} h {} min, {} money",
                    head.level, head.hp, head.most_hp, head.hours, head.minutes, head.money
                )
            });
            info!(
                "the memory card screen: pass {}: the task ends with the word {word:#X}; the game the reader took: {took:?}",
                view.passes
            );
            view.over = true;
            if !view.over_room {
                continue;
            }
            let mut next = "what stands under it goes on";
            if word & card_screen::word::LOADED != 0
                && let Played::Running(game) = &mut view.game
                && let Some(loaded) = game.loaded.take()
            {
                commands.insert_resource(loaded);
                next = "the field starts anew with the loaded game";
            }
            // Behind "End Game" the field's task ends and the opening movie plays
            // (`end_game` in `main.rs`).
            if word & card_screen::word::TO_TITLE != 0 {
                commands.insert_resource(crate::saving::Ended);
                next = "the field ends by \"End Game\"";
            }
            info!(
                "the memory card screen: pass {}: the screen leaves, {next}",
                view.passes
            );
            for entity in &over_room {
                commands.entity(entity).despawn();
            }
            commands.remove_resource::<CardView>();
            return;
        }
    }
    if !painted {
        return;
    }
    if let Ok(mut sprite) = sprites.get_mut(view.shown) {
        let (wide, high) = (SCREEN.0 as u32, SCREEN.1 as u32);
        let screen = view.paint(settings.0.colour as i16);
        sprite.image = images.add(crate::picture_image(wide, high, screen));
    }
}
