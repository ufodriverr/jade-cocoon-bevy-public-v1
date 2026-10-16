
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use jc_disc::{Archive, DiscImage, Executable, LocalConfig};
use jc_formats::battle::Tables as FightTables;
use jc_formats::battle::record::Record;
use jc_formats::battle::results::{self, Passed, Results, Task};
use jc_formats::carried::Carried;
use jc_formats::menu::{Frames, Style, sign};
use jc_formats::message;
use jc_formats::name;
use jc_formats::notice;
use jc_formats::pad::{self, Pad, Repeat};
use jc_formats::player_menu::{MENU_FILE, MENU_FILE_BASE, PASSES_A_SECOND};
use jc_formats::player_panel::{self, Counted, Counts, Drawn};
use jc_formats::rooms;
use jc_formats::text::{self, Font, Fonts};
use jc_formats::tim;
use jc_formats::window;

use crate::music::Music;
use crate::name_entry::{Picture, SCREEN, WHOLE, blend, blit};
use crate::options_view::{self, Parts, ThingLook, blit_lit, inside};
use crate::place_map::picture;
use crate::player_menu::{Bars, bars, face, put_bar, triangle};
use crate::title::Press;

/// The longest stretch of time one pass of the app may stand for.
const LONGEST_PASS: f64 = 0.1;
/// Pictures of the screen in a game frame of the field engine.
const GAME_FRAME: u32 = 2;
/// The layer of what stands over a room, as the player's menu has it.
const OVER_THE_ROOM: usize = 4;
/// The music the counting asks for when a fighter gains a level (`0x8003013C(4, 1)`).
const LEVEL_MUSIC: usize = 4;

type Failure = Box<dyn std::error::Error>;

/// `--results-press=SECONDS:BUTTON,...`: the buttons a hidden run taps on the screen,
/// counted from the moment it opens.
#[derive(Resource, Default)]
pub struct ResultsPresses(pub Vec<Press>);

/// `--results-experience=HEX[,HEX...]`: a check option, the sums of experience the
/// screen counts for the player and for the creatures he carries, in their order, in
/// place of what the fight gave them (a level is `0x4000000`).
#[derive(Resource, Default)]
pub struct ResultsSum(pub Vec<u32>);

/// What the screen puts over a room: its camera and its picture.
#[derive(Component)]
pub struct OverTheRoom;

/// The screen while it is up.
#[derive(Resource)]
pub struct ResultsView {
    task: Task,
    /// The state of the game the screen works on: the notice gives to it, and the
    /// fighters' own records, which the counting's end writes.
    carried: Carried,
    real: Vec<Record>,
    tables: FightTables,
    pad: Option<Pad>,
    /// Passes of the screen's task so far, and the share of one that is owed.
    passes: u32,
    owed: f64,
    time: f64,
    presses: Vec<Press>,
    /// Nobody is at the pad: the confirm goes down in every second pass.
    confirms: bool,
    /// What the last pass drew.
    drawn: Passed,
    /// The circle has asked for the player's menu: the task sleeps and the screen
    /// waits to be taken away by the one who opens the menu.
    menu: bool,
    look: Look,
    shown: Entity,
}

/// The parts every screen of the menus is painted with, and what the notice and the
/// line of help are made of.
struct Look {
    sprites: Vec<Picture>,
    frames: Frames,
    font: Font,
    font_picture: Picture,
    small: Font,
    small_picture: Picture,
    /// The eight pictures of a window's frame and the fill's colour by window colour.
    window: Vec<Picture>,
    fills: [[u8; 3]; window::COLOURS],
    /// The small picture and the name of each thing of the notice, and the "X" in
    /// front of a count.
    things: [ThingLook; notice::THINGS],
    times: Vec<u16>,
    /// The pairs of the line of help.
    help: Vec<(usize, Vec<u16>)>,
    /// What the player's box is painted with: the colours of the gauges' bars, his
    /// face and his name.
    bars: Bars,
    face: Picture,
    name: Vec<u16>,
    /// The names of the ranks.
    ranks: Vec<Vec<u16>>,
    /// The colours of a number that rose or fell, and the arrow of a difference, an
    /// image of the fonts' file.
    digit_colours: [[u8; 3]; 3],
    arrow: Picture,
}

/// The image of the fonts' file that is the arrow of a difference (`0x80032050`).
const ARROW: usize = 88;

impl Look {
    /// `named` is the name the player was given; without one he has the name a new
    /// game starts with.
    fn read(
        noticed: &[notice::Thing; notice::THINGS],
        named: &[u16],
    ) -> Result<(Self, FightTables), Failure> {
        let config = LocalConfig::load()?;
        let mut disc = DiscImage::open(&config.disc_image)?;
        let archive = Archive::open(&mut disc)?;
        let exe = Executable::read(&mut disc)?;
        let file = archive.read(&mut disc, MENU_FILE)?;
        let fonts = Fonts::from_executable(&exe.image, exe.base)
            .ok_or("the executable does not hold the font tables")?;
        let frames = Frames::from_executable(&exe.image, exe.base)
            .ok_or("the executable does not hold the tables of the frames")?;
        let font = (fonts.get(text::TEXT_FONT).cloned()).ok_or("there is no font of the texts")?;
        let small = (fonts.get(text::SMALL_FONT).cloned()).ok_or("there is no small font")?;
        let font_set = tim::texture_set(&archive.read(&mut disc, text::FONT_SET)?)
            .ok_or("the font file is not a texture set")?;
        let sprite_set = tim::texture_set(&archive.read(&mut disc, name::SPRITES)?)
            .ok_or("the sprite file is not a texture set")?;
        let sprites = (sprite_set.iter())
            .map(Picture::of)
            .collect::<Result<Vec<_>, _>>()?;
        let window = (0..8)
            .map(|part| {
                let image = font_set.get(window::FRAME_IMAGE + part);
                Picture::of(image.ok_or("the fonts' file has no frame of a window")?)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let tables = notice::Tables::new(&exe.image, exe.base);
        let things = noticed.map(|thing| {
            let small = (tables.picture(thing.kind, thing.number))
                .and_then(|number| picture(&font_set, number).ok());
            let name = tables.name(thing.kind, thing.number);
            (small, name.map(|at| tables.text(at)).unwrap_or_default())
        });
        // A word of the line of help lies in the screen's file or in the executable.
        let pairs = player_panel::help_list(&file, MENU_FILE_BASE, results::HELP)
            .ok_or("the file of the results screen does not hold its line of help")?;
        let help = (pairs.into_iter())
            .map(|(sprite, address)| {
                let said = match address.checked_sub(MENU_FILE_BASE) {
                    Some(offset) => file.get(offset as usize..),
                    None => exe.image.get(address.checked_sub(exe.base)? as usize..),
                };
                Some((sprite, text::units(said?)))
            })
            .collect::<Option<Vec<_>>>()
            .ok_or("a word of the results screen's line of help is not to be found")?;
        let fight = FightTables::read(&exe.image, exe.base)
            .ok_or("the executable does not hold the tables of the fights")?;
        let name = match named.is_empty() {
            false => named.to_vec(),
            true => {
                let (name_file, address) = name::DEFAULT_NAME;
                let title = archive.read(&mut disc, name_file)?;
                (address.checked_sub(rooms::load_address(name_file)))
                    .and_then(|offset| title.get(offset as usize..))
                    .map(text::units)
                    .ok_or("the default name is not in its file")?
            }
        };
        let ranks = (0..player_panel::detail::RANKS)
            .map(|rank| player_panel::rank_name(&exe.image, exe.base, rank))
            .collect::<Option<Vec<_>>>()
            .ok_or("the executable does not hold the names of the ranks")?;
        let mut digit_colours = [[128; 3]; 3];
        for (entry, colour) in digit_colours.iter_mut().enumerate() {
            *colour = player_panel::digit_colour(&exe.image, exe.base, entry)
                .ok_or("the executable does not hold the colours of the digits")?;
        }
        let look = Self {
            digit_colours,
            arrow: picture(&font_set, ARROW)?,
            ranks,
            font_picture: picture(&font_set, font.image)?,
            small_picture: picture(&font_set, small.image)?,
            sprites,
            frames,
            font,
            small,
            window,
            fills: window::fill_colours(&exe.image, exe.base)
                .ok_or("the executable does not hold the colours of a window")?,
            things,
            times: tables.label(notice::Label::Times),
            help,
            bars: bars(&font_set)?,
            face: face(&mut disc, &archive, &exe)?,
            name,
        };
        Ok((look, fight))
    }

    fn parts(&self) -> Parts<'_> {
        Parts {
            sprites: &self.sprites,
            frames: &self.frames,
            font: &self.font,
            font_picture: &self.font_picture,
        }
    }

    fn notice(&self, screen: &mut [u8], things: &[notice::Thing; notice::THINGS], colour: i16) {
        let widths = [0, 1, 2].map(|slot: usize| {
            let small = self.things[slot].0.as_ref();
            small.map_or(0, |small| small.wide as i16)
        });
        let name = message::width(&self.font, &self.things[0].1, &[]);
        let laid = notice::laid_out(things, results::SPOILS_WAY, 0, 0, widths, name);
        options_view::window_frame(screen, &self.window, laid.rect);
        let fill = usize::try_from(colour)
            .ok()
            .and_then(|colour| self.fills.get(colour));
        if let Some(fill) = fill {
            options_view::window_fill(screen, &self.window, *fill, laid.rect);
        }
        let parts = self.parts();
        for piece in laid.pieces {
            match piece {
                notice::Piece::Picture { slot, at } => {
                    if let Some(small) = &self.things[slot].0 {
                        blit(screen, small, (0, 0), (small.wide, small.high), at, WHOLE);
                    }
                }
                notice::Piece::Name { slot, at } => {
                    parts.text(screen, &self.things[slot].1, at, WHOLE);
                }
                notice::Piece::Label {
                    label: notice::Label::Times,
                    at,
                } => {
                    let (font, small) = (&self.small, &self.small_picture);
                    options_view::glyphs(screen, font, small, &self.times, at, WHOLE);
                }
                notice::Piece::Count { value, at } => {
                    parts.text(screen, &notice::number_units(value), at, WHOLE);
                }
                // The spoils are things with a name: no money, no cocoons, no lines.
                _ => {}
            }
        }
    }
}

impl ResultsView {
    pub fn wants_menu(&self) -> bool {
        self.menu
    }

    /// One thing of the boxes; `colour` is the window colour of the settings. Stand-in:
    /// a carried creature's name is not drawn, the build's player carries none.
    fn put(&self, screen: &mut [u8], colour: i16, drawn: Drawn) {
        let parts = self.look.parts();
        match drawn {
            Drawn::Frame { rect, mode, time } => {
                let style = Style {
                    kind: colour,
                    mode,
                    blend: 1,
                };
                parts.frame(screen, rect, style, time);
            }
            Drawn::Sprite { number, at, clip } => parts.sprite(screen, number, at, clip),
            Drawn::Number {
                value,
                ends,
                small,
                clip,
            } => {
                let width = |number: usize| parts.width(number);
                for (number, at) in player_panel::digits(value, ends, small, width) {
                    parts.sprite(screen, number, at, clip);
                }
            }
            Drawn::Bar {
                picture,
                length,
                at,
                clip,
            } => {
                if let Some(rows) = self.look.bars.get(&picture) {
                    put_bar(screen, rows, length, at, clip);
                }
            }
            Drawn::Name { at, clip } => parts.text(screen, &self.look.name, at, clip),
            Drawn::Face { at, clip } => {
                let face = &self.look.face;
                blit(screen, face, (0, 0), (face.wide, face.high), at, clip);
            }
            // The rank before is the one of his level before the counting, the rank
            // now the one of his own record.
            Drawn::RankName { before, dark, at } => {
                let results = &self.task.results;
                let record = match before {
                    true => results.before.first(),
                    false => self.real.first(),
                };
                let name = record.and_then(|he| self.look.ranks.get(player_panel::rank(he.level)));
                if let Some(name) = name {
                    let part = (results::DARK - dark).clamp(0, 255) as u8;
                    let (font, picture) = (parts.font, parts.font_picture);
                    options_view::glyphs_lit(screen, font, picture, name, at, [part; 3]);
                }
            }
            // The name of a carried creature: in its box in the small font and in
            // capitals, on its panel in the font of the texts.
            Drawn::CreatureName { creature, at, clip } => {
                if let Some(name) = self.creature_name(creature) {
                    let name: Vec<u16> = (name.iter().copied())
                        .map(player_panel::in_capitals)
                        .collect();
                    let (small, picture) = (&self.look.small, &self.look.small_picture);
                    options_view::glyphs(screen, small, picture, &name, at, clip);
                }
            }
            Drawn::FullName { creature, at, clip } => {
                if let Some(name) = self.creature_name(creature) {
                    parts.text(screen, &name, at, clip);
                }
            }
            // The pieces of a creature's panel of a level up, as the player's menu
            // paints its "Detail".
            Drawn::Tinted {
                value,
                ends,
                colour,
                clip,
            } => {
                let colours = &self.look.digit_colours;
                let colour = colours.get(colour).copied().unwrap_or([128; 3]);
                let width = |number: usize| parts.width(number);
                for (number, at) in player_panel::digits(value, ends, false, width) {
                    if let Some(sprite) = self.look.sprites.get(number) {
                        blit_lit(screen, sprite, at, colour, clip);
                    }
                }
            }
            Drawn::Arrow { turns, at } => {
                let image = &self.look.arrow;
                let size = (image.wide as i16, image.high as i16);
                for (u, v) in (0..size.1).flat_map(|v| (0..size.0).map(move |u| (u, v))) {
                    let texel = (v as usize * image.wide + u as usize) * 4;
                    if image.rgba[texel + 3] != 0 {
                        let (dx, dy) = sign::texel(turns, size, (u, v));
                        let colour = [0, 1, 2].map(|part| image.rgba[texel + part]);
                        blend(screen, at.0 + dx, at.1 + dy, colour, None);
                    }
                }
            }
            Drawn::Triangle {
                corners,
                colour,
                clip,
            } => triangle(screen, corners, colour, clip),
            Drawn::Line {
                from,
                to,
                grey,
                clip,
            } => {
                let (xs, ys) = (
                    from.0.min(to.0)..=from.0.max(to.0),
                    from.1.min(to.1)..=from.1.max(to.1),
                );
                for (x, y) in ys.flat_map(|y| xs.clone().map(move |x| (x, y))) {
                    if inside(clip, x, y) {
                        blend(screen, x, y, [grey; 3], None);
                    }
                }
            }
            _ => {}
        }
    }

    /// The name of the carried creature `creature`, counted from 0: its record's.
    fn creature_name(&self, creature: usize) -> Option<Vec<u16>> {
        let record = self.real.get(creature + 1)?;
        Some(name::units(&record.name, &self.look.small))
    }

    /// The screen of a pass, four bytes a pixel: the wall, which every state of the
    /// task draws first (`0x8002BE6C`), the line of help once all are gone through,
    /// and the notice of the spoils while its window is up. `colour` is the window
    /// colour of the settings.
    fn paint(&self, colour: i16) -> Vec<u8> {
        let mut screen = [0, 0, 0, 255].repeat(SCREEN.0 * SCREEN.1);
        let parts = self.look.parts();
        parts.wall(&mut screen);
        let results = &self.task.results;
        // A creature's panel of a level up, in place of the boxes: its "Detail" as it
        // is now, compared with the creature before the counting.
        let creature = results.panel().filter(|n| *n != 0 && self.drawn.screen);
        let pair = creature.and_then(|n| Some((n, self.real.get(n)?, results.before.get(n)?)));
        if let Some((n, now, before)) = pair {
            let values = |record: &Record| -> [u8; 5] {
                let counted = record.effective(&self.tables).stats;
                std::array::from_fn(|value| counted[value])
            };
            let panel =
                player_panel::level_panel(n - 1, (now, values(now)), (before, values(before)));
            for drawn in panel {
                self.put(&mut screen, colour, drawn);
            }
        }
        // The player's panel of a level up, in place of the boxes.
        if let (true, Some(0), Some((before, dark))) =
            (self.drawn.screen, results.panel(), self.drawn.pass.name)
        {
            for drawn in player_panel::rank_panel(before, dark) {
                self.put(&mut screen, colour, drawn);
            }
        }
        if self.drawn.screen && results.panel().is_none() {
            let counted = |n: usize| {
                let through = results.is_through(n);
                let record = match through {
                    true => self.real.get(n),
                    false => results.copies.get(n),
                };
                record.map(|record| Counted {
                    record,
                    gained: results.gained[n] && !through,
                })
            };
            let creatures: Vec<Counted> = (1..results.copies.len()).filter_map(counted).collect();
            // The caught creatures that wait and the kinds that count as caught, as
            // the player's menu counts them.
            let counts = Counts {
                cocoons: self.carried.keeping.cocoons(),
                waiting: self.carried.caught.waiting().count() as i32,
                kinds: self.carried.kinds.count() as i32,
                floor: (self.carried.corridor.as_ref()).map(|corridor| corridor.floor as i32),
            };
            let time = jc_formats::menu::frame_time(self.passes, GAME_FRAME);
            if let Some(player) = counted(0) {
                for drawn in player_panel::results_boxes(player, &creatures, counts, time) {
                    self.put(&mut screen, colour, drawn);
                }
            }
        }
        if self.drawn.screen && results.all_through() {
            parts.legend(&mut screen, colour, &self.look.help);
        }
        if self.drawn.notice {
            (self.look).notice(&mut screen, &self.task.noticed(), colour);
        }
        screen
    }
}

pub fn over_the_room(
    results: Results,
    carried: Carried,
    name: &[u16],
    confirms: bool,
    presses: &ResultsPresses,
    images: &mut Assets<Image>,
    commands: &mut Commands,
) -> Result<(), Box<(Results, Failure)>> {
    let Some(player) = carried.player.clone() else {
        return Err(Box::new((
            results,
            "the game has no record of the player".into(),
        )));
    };
    // The real records: the player's and those of the creatures he carries.
    let mut real = vec![player.clone()];
    real.extend(carried.creatures.carried().cloned());
    let task = Task::new(results);
    let (look, tables) = match Look::read(&task.noticed(), name) {
        Ok(read) => read,
        Err(err) => return Err(Box::new((task.results, err))),
    };
    info!(
        "the results screen: the opponents leave {:?}, the sums of experience are {:x?}; the \
         player has level {} and the experience word {:#x}",
        task.results.won, task.results.sums, player.level, player.experience
    );
    let (wide, high) = (SCREEN.0 as u32, SCREEN.1 as u32);
    let black = [0, 0, 0, 255].repeat(SCREEN.0 * SCREEN.1);
    let shown = crate::spawn_picture(commands, images, wide, high, black);
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
    commands.insert_resource(ResultsView {
        task,
        carried,
        real,
        tables,
        pad: None,
        passes: 0,
        owed: 0.0,
        time: 0.0,
        presses: presses.0.clone(),
        confirms,
        drawn: Passed::default(),
        menu: false,
        look,
        shown,
    });
    Ok(())
}

/// Runs the passes of the screen for the time gone by.
#[allow(clippy::too_many_arguments)]
pub fn run_results(
    current: Option<ResMut<ResultsView>>,
    mut music: ResMut<Music>,
    settings: Res<crate::settings::Settings>,
    time: Res<Time<Virtual>>,
    keys: Res<crate::keys::Pad>,
    mut images: ResMut<Assets<Image>>,
    mut sprites: Query<&mut Sprite>,
    over_room: Query<Entity, With<OverTheRoom>>,
    mut things: Option<ResMut<crate::shown::Things>>,
    mut commands: Commands,
) {
    let Some(mut current) = current else {
        return;
    };
    if current.menu {
        return;
    }
    let passed = crate::pace::passed(&time, LONGEST_PASS);
    current.time += passed;
    current.owed += passed * f64::from(PASSES_A_SECOND);
    let mut held = crate::keys::held(&keys);
    for press in &current.presses {
        if press.holds(current.time) {
            held |= press.button;
        }
    }
    // The pad routine has run in every pass before the screen's first: a button that
    // is still down from the fight did not go down on the screen.
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
        let passes = current.passes;
        let alone = current.confirms && current.presses.is_empty();
        let held = match alone && passes % 2 == 0 {
            true => held | pad::CROSS,
            false => held,
        };
        let mut pad = current.pad.unwrap_or_default();
        pad.read(held, Repeat::of(GAME_FRAME), passes);
        current.pad = Some(pad);
        let current = &mut *current;
        let before = (current.task.results.flags, current.task.results.counted);
        let worn = current.carried.worn;
        let keeping = &mut current.carried.keeping;
        let Some(drawn) =
            (current.task).pass(&current.tables, &pad, &mut current.real, keeping, worn)
        else {
            // The growth has no steps for a fighter: the screen cannot go on.
            error!("the results screen: pass {passes}: a fighter cannot grow; the screen leaves");
            leave(current, things.as_deref_mut(), &over_room, &mut commands);
            return;
        };
        for &sound in &drawn.pass.sounds {
            music.sound(sound.into());
        }
        for _ in 0..drawn.pass.jingles {
            if let Err(err) = music.ask(LEVEL_MUSIC) {
                error!("music {LEVEL_MUSIC} of a level gained: {err}");
            }
        }
        let results = &current.task.results;
        if drawn.given {
            info!(
                "the results screen: pass {passes}: the notice ends and hands over {:?}",
                results.won.iter().flatten().collect::<Vec<_>>()
            );
        }
        if (results.flags, results.counted) != before || drawn.notice != current.drawn.notice {
            info!(
                "the results screen: pass {passes}: the notice is {}, the flag word {:#x}, \
                 counted {:x?} of {:x?}, sounds {:?}, the music of a level {} times",
                if drawn.notice { "up" } else { "not up" },
                results.flags,
                results.counted,
                results.sums,
                drawn.pass.sounds,
                drawn.pass.jingles
            );
        }
        let (leaves, menu) = (drawn.pass.leaves, drawn.pass.menu);
        current.drawn = drawn;
        // The circle: the task sleeps and the menu's is woken. The real records were
        // written in every pass since the counting ended, so the room has them now.
        if menu {
            info!(
                "the results screen: pass {passes}: the circle opens the player's menu; the \
                 screen's task sleeps"
            );
            if let Some(things) = things.as_deref_mut() {
                let carried = &current.carried;
                things.kept(
                    carried.keeping.clone(),
                    current.real.first().cloned(),
                    carried.worn,
                    current.real.get(1..).map(<[Record]>::to_vec),
                );
            }
            current.menu = true;
            return;
        }
        if leaves {
            leave(current, things.as_deref_mut(), &over_room, &mut commands);
            return;
        }
    }
    if !painted {
        return;
    }
    if let Ok(mut sprite) = sprites.get_mut(current.shown) {
        let (wide, high) = (SCREEN.0 as u32, SCREEN.1 as u32);
        let colour = settings.0.colour as i16;
        sprite.image = images.add(crate::picture_image(wide, high, current.paint(colour)));
    }
}

fn leave(
    current: &mut ResultsView,
    things: Option<&mut crate::shown::Things>,
    over_room: &Query<Entity, With<OverTheRoom>>,
    commands: &mut Commands,
) {
    let player = current.real.first().cloned();
    if let Some(he) = &player {
        info!(
            "the results screen: pass {}: the screen leaves; the player has level {} and the \
             experience word {:#x}, the room runs on",
            current.passes, he.level, he.experience
        );
    }
    if let Some(things) = things {
        let carried = &current.carried;
        let creatures = current.real.get(1..).map(<[Record]>::to_vec);
        things.kept(carried.keeping.clone(), player, carried.worn, creatures);
    }
    for entity in over_room {
        commands.entity(entity).despawn();
    }
    commands.remove_resource::<ResultsView>();
}
