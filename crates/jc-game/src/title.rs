
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use jc_disc::{Archive, DiscImage, Executable, LocalConfig};
use jc_formats::menu::{self, Frames, Rect, Style};
use jc_formats::options_screen::{self, OptionsScreen};
use jc_formats::pad::{self, Pad, Repeat};
use jc_formats::tim::{self, Tim};
use jc_formats::title::{self, Choice, Placed, Screen, Step, Warning};

use crate::WINDOW_SIZE;
use crate::movie_view::{Failure, Playing, ShowMovie};
use crate::music::Music;
use crate::options_view::{self, View};
use crate::settings::Settings;
use crate::text_view;

/// `--title`: the title screen without the opening movie before it.
/// `--press-start=SECONDS` holds the Start button from that second of the title on, and
/// `--press=SECONDS:BUTTON,...` taps a button at a second of the title.
#[derive(Resource)]
pub struct ShowTitle {
    pub press_start: Option<f64>,
    pub presses: Vec<Press>,
}

/// `--title-again=SECONDS:BUTTON,...`: the buttons a hidden run taps on a title that
/// opens behind a movie (the one behind the opening movie of the app's start, of a title
/// left alone or of "End Game"), counted from the moment it opens. Start is one of them.
#[derive(Resource, Default)]
pub struct BehindAMovie(pub Vec<Press>);

/// A button that a hidden run holds for a moment.
#[derive(Clone, Copy)]
pub struct Press {
    from: f64,
    pub button: u16,
}

/// How long a button of `--press` stays down, in seconds.
const PRESS_LENGTH: f64 = 0.1;

impl Press {
    /// Whether the button is down at a second of the screen.
    pub fn holds(&self, time: f64) -> bool {
        (self.from..self.from + PRESS_LENGTH).contains(&time)
    }
}

/// The value of `--press`: `SECONDS:BUTTON` pairs with commas between them.
pub fn presses(value: &str) -> Option<Vec<Press>> {
    (value.split(','))
        .map(|press| {
            let (from, button) = press.split_once(':')?;
            let button = match button {
                "start" => pad::START,
                "cross" => pad::CROSS,
                "triangle" => pad::TRIANGLE,
                "square" => pad::SQUARE,
                "circle" => pad::CIRCLE,
                "up" => pad::UP,
                "down" => pad::DOWN,
                "left" => pad::LEFT,
                "right" => pad::RIGHT,
                "select" => pad::SELECT,
                "l1" => pad::L1,
                "l2" => pad::L2,
                "r1" => pad::R1,
                "r2" => pad::R2,
                _ => return None,
            };
            Some(Press {
                from: from.parse().ok()?,
                button,
            })
        })
        .collect()
}

/// The longest stretch of time one pass of the app may stand for.
const LONGEST_PASS: f64 = 0.1;

/// The kind of frame the game's state names for a menu that names none. The title's
/// menu names its own, so the value is never asked for here.
const OWN_KIND: i16 = 0;

/// A frame on screen and the colour its picture was last made with.
struct Framed {
    entity: Entity,
    rect: Rect,
    colour: Option<[u8; 3]>,
}

struct Options {
    screen: OptionsScreen,
    view: View,
    shown: Entity,
    stale: bool,
}

/// The title screen while it is up.
#[derive(Resource)]
pub struct Title {
    state: title::Title,
    pad: Option<Pad>,
    /// Passes of the title's task so far, and the share of one that is owed.
    passes: u32,
    owed: f64,
    time: f64,
    press_start: Option<f64>,
    presses: Vec<Press>,
    frames: Frames,
    /// The title picture as the screen holds it, four bytes a pixel.
    backdrop: Vec<u8>,
    background: Vec<Entity>,
    words: Entity,
    words_frame: Framed,
    /// The menu's lines: the frame and the words in it.
    lines: Vec<(Framed, Entity)>,
    /// The screen of a memory card warning, once the check has asked for one.
    warning: Option<Entity>,
    options: Option<Options>,
}

/// The images of the title's texture set and the frame drawer's tables.
fn load() -> Result<(Vec<Tim>, Frames), Failure> {
    let config = LocalConfig::load()?;
    let mut disc = DiscImage::open(&config.disc_image)?;
    let archive = Archive::open(&mut disc)?;
    let file = archive.read(&mut disc, title::PICTURES)?;
    let set = tim::texture_set(&file).ok_or("the title's pictures are not a texture set")?;
    let exe = Executable::read(&mut disc)?;
    let frames = Frames::from_executable(&exe.image, exe.base)
        .ok_or("the executable does not hold the tables of the frames")?;
    Ok((set, frames))
}

/// A picture of the console's screen where the window shows it: the screen fills the
/// window.
fn on_screen(
    image: Handle<Image>,
    corner: (i16, i16),
    size: (u32, u32),
    depth: f32,
) -> impl Bundle {
    let screen = Vec2::new(f32::from(title::SCREEN.0), f32::from(title::SCREEN.1));
    let window = Vec2::new(WINDOW_SIZE.0 as f32, WINDOW_SIZE.1 as f32);
    let size = Vec2::new(size.0 as f32, size.1 as f32);
    let corner = Vec2::new(f32::from(corner.0), f32::from(corner.1));
    let middle = (corner + size / 2.0 - screen / 2.0) * Vec2::new(1.0, -1.0);
    (
        Sprite {
            image,
            custom_size: Some(size * window / screen),
            ..default()
        },
        Transform::from_translation((middle * window / screen).extend(depth)),
    )
}

fn picture(width: u32, height: u32, rgba: Vec<u8>) -> Image {
    Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// The pixels of an image of the set.
fn pixels(set: &[Tim], image: usize) -> Result<(&Tim, Vec<u8>), Failure> {
    let picture = set.get(image).ok_or("the set has no such image")?;
    Ok((picture, picture.to_rgba(picture.palette(0))?))
}

/// The picture of a frame over what the screen holds under it. The console keeps five
/// bits of a colour and blends in them; how its chip rounds is not on the disc.
fn frame_picture(
    frames: &Frames,
    rect: Rect,
    colour: [u8; 3],
    blend: Option<u8>,
    backdrop: &[u8],
) -> Image {
    let quads = frames.quads(rect, Some(colour));
    let (width, height) = (rect.w.max(0) as u32, rect.h.max(0) as u32);
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in rect.y..rect.y.saturating_add(rect.h.max(0)) {
        for x in rect.x..rect.x.saturating_add(rect.w.max(0)) {
            let Some(quad) = quads.iter().find(|quad| quad.covers(x, y)) else {
                rgba.extend([0; 4]);
                continue;
            };
            let inside = (0..title::SCREEN.0).contains(&x) && (0..title::SCREEN.1).contains(&y);
            let under = match inside {
                true => y as usize * title::SCREEN.0 as usize + x as usize,
                false => usize::MAX,
            };
            for part in 0..3 {
                let under = under
                    .checked_mul(4)
                    .and_then(|at| backdrop.get(at + part))
                    .map_or(0, |&value| i32::from(value >> 3));
                let over = i32::from(quad.colour[part] >> 3);
                let five = match blend {
                    None => over,
                    Some(0) => (under + over) >> 1,
                    Some(1) => under + over,
                    Some(2) => under - over,
                    Some(_) => under + (over >> 2),
                }
                .clamp(0, 31) as u8;
                rgba.push(five << 3 | five >> 2);
            }
            rgba.push(255);
        }
    }
    picture(width, height, rgba)
}

/// Puts the title on screen as its task's start routine leaves it.
fn open(
    press_start: Option<f64>,
    presses: Vec<Press>,
    images: &mut Assets<Image>,
    commands: &mut Commands,
) -> Result<Title, Failure> {
    let (set, frames) = load()?;
    let (wide, high) = (title::SCREEN.0 as usize, title::SCREEN.1 as usize);
    let mut backdrop = vec![0; wide * high * 4];
    let mut background = Vec::new();
    let mut spawn = |placed: Placed, depth: f32, look: Visibility, commands: &mut Commands| {
        let (image, rgba) = pixels(&set, placed.image)?;
        let size = (image.width, image.height);
        let handle = images.add(picture(size.0, size.1, rgba));
        let bundle = on_screen(handle, (placed.x, placed.y), size, depth);
        Ok::<Entity, Failure>(commands.spawn((bundle, look)).id())
    };
    for half in title::BACKGROUND {
        background.push(spawn(half, 0.0, Visibility::Inherited, commands)?);
        let (image, rgba) = pixels(&set, half.image)?;
        let row = image.width as usize * 4;
        for (y, pixels) in rgba.chunks_exact(row.max(1)).enumerate() {
            let (x, y) = (half.x as usize, half.y as usize + y);
            let from = (y * wide + x) * 4;
            if x * 4 + row <= wide * 4 && y < high {
                backdrop[from..from + row].copy_from_slice(pixels);
            }
        }
    }
    // Words are added to the ordering table before their frame and the frame before the
    // picture: each lies over the next.
    let words = spawn(title::PRESS_START, 2.0, Visibility::Inherited, commands)?;
    let mut lines = Vec::new();
    for line in title::LINES {
        let width = set.get(line.image).map_or(0, |image| image.width);
        let room = line.frame.inside(title::LINE_MARGIN);
        let (x, y) = menu::placed(title::LINE_FLAGS, room, width as i16);
        let placed = Placed {
            image: line.image,
            x,
            y,
        };
        let words = spawn(placed, 2.0, Visibility::Hidden, commands)?;
        lines.push((framed(line.frame, commands), words));
    }
    info!(
        "the title screen: {} images in file {}, the press Start screen",
        set.len(),
        title::PICTURES
    );
    Ok(Title {
        state: title::Title::new(),
        pad: None,
        passes: 0,
        owed: 0.0,
        time: 0.0,
        press_start,
        presses,
        frames,
        backdrop,
        background,
        words,
        words_frame: framed(title::PRESS_START_FRAME, commands),
        lines,
        warning: None,
        options: None,
    })
}

/// The screen of a warning: its lines in the font of the texts and nothing else. The
/// title's task does not draw its picture while a warning stands, and the screen is
/// cleared to black.
fn warn(
    warning: Warning,
    images: &mut Assets<Image>,
    commands: &mut Commands,
) -> Result<Entity, Failure> {
    let rgba = text_view::texts_on_black(title::OVERLAY, title::WARNING_FONT, warning.placed())?;
    let size = (title::SCREEN.0 as u32, title::SCREEN.1 as u32);
    let handle = images.add(picture(size.0, size.1, rgba));
    Ok(commands.spawn(on_screen(handle, (0, 0), size, 3.0)).id())
}

/// The start of the options screen's task (`0x800C309C`): its record is set up from the
/// settings and it draws once, over everything of the title.
fn open_options(
    settings: &options_screen::Settings,
    time: u32,
    images: &mut Assets<Image>,
    commands: &mut Commands,
) -> Result<Options, Failure> {
    let view = View::read()?;
    let screen = OptionsScreen::open(settings);
    let size = (title::SCREEN.0 as u32, title::SCREEN.1 as u32);
    let rgba = view.screen(&screen, settings, time);
    let handle = images.add(picture(size.0, size.1, rgba));
    Ok(Options {
        screen,
        view,
        shown: commands.spawn(on_screen(handle, (0, 0), size, 3.0)).id(),
        stale: false,
    })
}

/// A frame that is not shown yet: `draw` gives it its picture.
fn framed(rect: Rect, commands: &mut Commands) -> Framed {
    let size = (rect.w.max(0) as u32, rect.h.max(0) as u32);
    let bundle = on_screen(Handle::default(), (rect.x, rect.y), size, 1.0);
    Framed {
        entity: commands.spawn((bundle, Visibility::Hidden)).id(),
        rect,
        colour: None,
    }
}

/// Shows what the title's screen holds in this pass: the words or the lines, and the
/// frames in the colours of the time.
fn draw(
    current: &mut Title,
    images: &mut Assets<Image>,
    sprites: &mut Query<&mut Sprite>,
    looks: &mut Query<&mut Visibility>,
) {
    let time = menu::frame_time(current.passes, 1);
    let screen = current.state.screen;
    let cursor = current.state.cursor.at;
    let Title {
        frames,
        backdrop,
        background,
        words,
        words_frame,
        lines,
        ..
    } = current;
    let mut show = |entity: Entity, shown: bool| {
        if let Ok(mut look) = looks.get_mut(entity) {
            *look = match shown {
                true => Visibility::Inherited,
                false => Visibility::Hidden,
            };
        }
    };
    let mut frame = |around: &mut Framed, style: Style, time: u32| {
        let Some(colour) = frames.colour(style, time) else {
            return;
        };
        if around.colour == Some(colour) {
            return;
        }
        if let Ok(mut sprite) = sprites.get_mut(around.entity) {
            let image = frame_picture(frames, around.rect, colour, style.blend_mode(), backdrop);
            sprite.image = images.add(image);
            around.colour = Some(colour);
        }
    };
    // From the card check to the new game the task draws no picture. Around the
    // options screen the menu's picture is the one the screen keeps, and under the
    // options' own picture it is not seen.
    let in_menu = matches!(
        screen,
        Screen::Menu | Screen::OptionsWait | Screen::Options | Screen::OptionsBack
    );
    let pictured = in_menu || matches!(screen, Screen::PressStart | Screen::WayOut);
    for half in background.iter() {
        show(*half, pictured);
    }
    show(*words, screen == Screen::PressStart);
    show(words_frame.entity, screen == Screen::PressStart);
    if screen == Screen::PressStart {
        frame(words_frame, title::PRESS_START_STYLE, time);
    }
    for (line, (around, words)) in lines.iter_mut().enumerate() {
        show(*words, in_menu);
        show(around.entity, in_menu);
        if in_menu {
            let on_cursor = usize::try_from(cursor) == Ok(line);
            let (style, pulses) = Style::of_line(title::MENU_FLAGS, on_cursor, OWN_KIND);
            frame(around, style, if pulses { time } else { 0 });
        }
    }
}

/// Takes the title off the screen.
fn close(current: &mut Title, commands: &mut Commands) {
    for entity in current.background.drain(..) {
        commands.entity(entity).despawn();
    }
    commands.entity(current.words).despawn();
    commands.entity(current.words_frame.entity).despawn();
    for (around, words) in current.lines.drain(..) {
        commands.entity(around.entity).despawn();
        commands.entity(words).despawn();
    }
    if let Some(warning) = current.warning.take() {
        commands.entity(warning).despawn();
    }
    if let Some(options) = current.options.take() {
        commands.entity(options.shown).despawn();
    }
    commands.remove_resource::<Title>();
}

fn start_music(music: &mut Music) {
    if let Err(err) = music.start(title::MUSIC) {
        error!("the title's music: {err}");
    }
}

/// `--title`: the screen alone.
pub fn show_title(
    show: Res<ShowTitle>,
    mut music: ResMut<Music>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    commands.spawn(Camera2d);
    match open(
        show.press_start,
        show.presses.clone(),
        &mut images,
        &mut commands,
    ) {
        Ok(title) => {
            start_music(&mut music);
            commands.insert_resource(title);
        }
        Err(err) => {
            error!("the title screen: {err}");
            exit.write(AppExit::error());
        }
    }
}

/// When the app started, for the count of refreshes the game seeds its generator with.
pub static STARTED: std::sync::LazyLock<std::time::Instant> =
    std::sync::LazyLock::new(std::time::Instant::now);

pub static SEED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// The refreshes of the disc's picture in a second.
const REFRESHES: f64 = 50.0;

/// The pad's word of held buttons from the keyboard, or from the presses of a hidden run.
fn buttons(current: &Title, keys: &ButtonInput<KeyCode>) -> u16 {
    // A hidden run that presses Start at a set time takes it from nowhere else.
    let mut held = match current.press_start {
        Some(from) if current.time >= from => pad::START,
        Some(_) => 0,
        None => crate::keys::held(keys) & pad::START,
    };
    // Left and right are the options screen's.
    let own = pad::CROSS | pad::TRIANGLE | pad::UP | pad::DOWN | pad::LEFT | pad::RIGHT;
    held |= crate::keys::held(keys) & own;
    for press in &current.presses {
        if press.holds(current.time) {
            held |= press.button;
        }
    }
    held
}

/// One pass of the options screen's task while the title waits for it, on the settings
/// of the app. The pass in which it ends tells the title, whose next pass finds it gone.
fn run_options(current: &mut Title, settings: &mut ResMut<Settings>, pad: &Pad, music: &mut Music) {
    let Some(options) = current.options.as_mut() else {
        return;
    };
    // Only a pass that changes a setting marks the resource as changed.
    let mut set = settings.0;
    let who = "the title screen";
    let pass = options_view::pass(
        who,
        current.passes,
        &mut options.screen,
        &mut set,
        pad,
        music,
    );
    if set != settings.0 {
        settings.0 = set;
    }
    options.stale = true;
    if pass.ended {
        current.state.options_ended();
        info!(
            "the title screen: pass {}: the options screen has ended",
            current.passes
        );
    }
}

/// Opens the title when the opening movie is over, runs its passes for the time gone by,
/// starts the opening movie again when the title has been left alone and the first movie
/// of a new game on "Start Game".
#[allow(clippy::too_many_arguments)]
pub fn run_title(
    current: Option<ResMut<Title>>,
    playing: Option<Res<Playing>>,
    movie: Res<ShowMovie>,
    mut music: ResMut<Music>,
    mut settings: ResMut<Settings>,
    mut clock: ResMut<crate::saving::PlayClock>,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut images: ResMut<Assets<Image>>,
    mut sprites: Query<&mut Sprite>,
    mut looks: Query<&mut Visibility>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
    (card, loaded, kept, game, again, mut card_seen): (
        Option<Res<crate::card_view::CardView>>,
        Option<Res<crate::saving::Loaded>>,
        Res<crate::card_view::CardsKept>,
        Option<Res<crate::Game>>,
        Res<BehindAMovie>,
        Local<bool>,
    ),
) {
    let Some(mut current) = current else {
        if playing.is_some_and(|playing| playing.leads_to_the_title()) {
            commands.remove_resource::<Playing>();
            match open(None, again.0.clone(), &mut images, &mut commands) {
                Ok(title) => {
                    start_music(&mut music);
                    commands.insert_resource(title);
                }
                Err(err) => {
                    error!("the title screen: {err}");
                    exit.write(AppExit::error());
                }
            }
        }
        return;
    };
    let passed = time.delta_secs_f64().min(LONGEST_PASS);
    current.time += passed;
    current.owed += passed * f64::from(title::PASSES_A_SECOND);
    let held = buttons(&current, &keys);
    // The memory card's screen of "Continue" was up and is gone: the title's next pass
    // finds its word cleared, and a game it loaded waits for the field.
    if current.state.screen == Screen::Card {
        match (card.is_some(), *card_seen) {
            (true, _) => *card_seen = true,
            (false, true) => {
                *card_seen = false;
                current.state.card_ended(loaded.is_some());
            }
            (false, false) => {}
        }
    }
    // The pad routine's call before the title's first pass: a button that is still down
    // from the movie did not go down in the title.
    current.pad.get_or_insert_with(|| {
        let mut pad = Pad::default();
        pad.read(held, Repeat::of(1), 0);
        pad
    });
    while current.owed >= 1.0 {
        current.owed -= 1.0;
        current.passes += 1;
        let passes = current.passes;
        let mut pad = current.pad.unwrap_or_default();
        pad.read(held, Repeat::of(1), passes);
        current.pad = Some(pad);
        let line = current.state.cursor.at;
        for sound in current.state.sounds(&pad) {
            music.sound(sound.into());
        }
        let waits = current.state.screen == Screen::Options;
        let step = current.state.pass(&pad);
        if waits && step == Step::Stays {
            run_options(&mut current, &mut settings, &pad, &mut music);
        }
        let seconds = current.time;
        if current.state.cursor.at != line {
            info!(
                "the title screen: the cursor goes from line {line} to line {} after {passes} passes",
                current.state.cursor.at
            );
        }
        // The movie a step starts in place of the title.
        let new_game;
        let next: &ShowMovie = match step {
            Step::Stays => continue,
            Step::ToMenu => {
                // The count of refreshes since the start; a hidden run that presses
                // its buttons at set times counts the title's own time, so it repeats.
                let scripted = current.press_start.is_some() || !current.presses.is_empty();
                let since = match scripted {
                    true => seconds,
                    false => STARTED.elapsed().as_secs_f64(),
                };
                let seed = (since * REFRESHES) as u32;
                SEED.store(seed, std::sync::atomic::Ordering::Relaxed);
                info!(
                    "the title screen: Start after {seconds:.2} s and {passes} passes, the menu with the cursor on line {line}; the generator is seeded with {seed}"
                );
                continue;
            }
            Step::ToPressStart => {
                info!("the title screen: back to the press Start screen after {passes} passes");
                continue;
            }
            Step::GoesOut => {
                info!(
                    "the title screen: left alone, it goes out after {seconds:.2} s and {passes} passes"
                );
                // The screen fades the music there.
                music.fade();
                continue;
            }
            Step::Chosen(Choice::Options) => {
                info!(
                    "the title screen: Options after {passes} passes, the options screen comes next"
                );
                continue;
            }
            Step::OptionsStart => {
                let time = menu::frame_time(passes, 1);
                match open_options(&settings.0, time, &mut images, &mut commands) {
                    Ok(options) => current.options = Some(options),
                    Err(err) => {
                        error!("the options screen: {err}");
                        exit.write(AppExit::error());
                        return;
                    }
                }
                info!("the title screen: the options screen starts after {passes} passes");
                continue;
            }
            Step::OptionsOver => {
                info!(
                    "the title screen: pass {passes}: the menu is drawn again behind the options' last picture"
                );
                continue;
            }
            Step::BackInMenu => {
                if let Some(options) = current.options.take() {
                    commands.entity(options.shown).despawn();
                }
                info!(
                    "the title screen: back in the menu after {passes} passes, the cursor on line {line}"
                );
                continue;
            }
            Step::Chosen(choice) if current.state.screen == Screen::CardCheck => {
                info!(
                    "the title screen: {choice:?} after {passes} passes, the memory card check comes next"
                );
                continue;
            }
            Step::Chosen(Choice::Continue) => {
                let carried = game
                    .as_ref()
                    .map_or_else(Default::default, |game| game.0.clone());
                let beside = (&*settings, None, &*clock);
                let opened =
                    crate::saving::Running::of(carried, beside, None, [0; 24]).and_then(|game| {
                        let kind = jc_formats::card_screen::Kind::Continue;
                        let (images, commands) = (&mut images, &mut commands);
                        crate::card_view::over_the_room(
                            kind, game, &kept, settings.0, images, commands,
                        )
                    });
                match opened {
                    Ok(()) => info!(
                        "the title screen: Continue after {passes} passes, the memory card's screen comes up"
                    ),
                    Err(err) => {
                        error!("the memory card screen: {err}");
                        current.state.card_ended(false);
                    }
                }
                continue;
            }
            Step::Chosen(choice) => {
                info!(
                    "the title screen: {choice:?} after {passes} passes, which the app does not have yet"
                );
                continue;
            }
            Step::CardOver => {
                info!(
                    "the title screen: the memory card's screen is gone after {passes} passes, back in the menu"
                );
                continue;
            }
            Step::Loaded => {
                info!(
                    "the title screen: a game was loaded, the title is over after {passes} passes and the field starts"
                );
                // The title's entry gives its music back behind the task runner. With a
                // finished game the new-game task comes in place of the leaving, and
                // the music plays on as under a new game.
                if !loaded.as_ref().is_some_and(|loaded| loaded.beside.finished) {
                    music.give_back();
                }
                close(&mut current, &mut commands);
                return;
            }
            Step::Warns(warning) => {
                info!(
                    "the title screen: slots {:?}, the warning {warning:?} of {} lines after {passes} passes",
                    current.state.slots,
                    warning.lines().len()
                );
                match warn(warning, &mut images, &mut commands) {
                    Ok(entity) => current.warning = Some(entity),
                    Err(err) => {
                        error!("the memory card warning: {err}");
                        exit.write(AppExit::error());
                        return;
                    }
                }
                continue;
            }
            Step::NewGame => {
                info!(
                    "the title screen: the new game after {seconds:.2} s and {passes} passes, movie {} plays",
                    title::NEW_GAME_MOVIE
                );
                // The reset of "Start Game" sets the play clock to 0.
                clock.start();
                new_game = movie.new_game();
                &new_game
            }
            Step::OpeningMovie => {
                info!(
                    "the title screen: over after {passes} passes, the opening movie plays again"
                );
                // The title's entry gives its music back behind the task runner.
                music.give_back();
                &movie
            }
        };
        close(&mut current, &mut commands);
        match Playing::start(next) {
            Ok(playing) => commands.insert_resource(playing),
            Err(err) => {
                error!("the movie behind the title: {err}");
                exit.write(AppExit::error());
            }
        }
        return;
    }
    // The options screen paints itself anew when a pass of it has run.
    let time = menu::frame_time(current.passes, 1);
    if let Some(options) = current.options.as_mut().filter(|options| options.stale) {
        options.stale = false;
        if let Ok(mut sprite) = sprites.get_mut(options.shown) {
            let size = (title::SCREEN.0 as u32, title::SCREEN.1 as u32);
            let rgba = options.view.screen(&options.screen, &settings.0, time);
            sprite.image = images.add(picture(size.0, size.1, rgba));
        }
    }
    draw(&mut current, &mut images, &mut sprites, &mut looks);
}
