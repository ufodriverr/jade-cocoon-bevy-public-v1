
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use jc_disc::{Archive, DiscImage, Executable, LocalConfig};
use jc_formats::battle::Tables as FightTables;
use jc_formats::battle::record::Record;
use jc_formats::carried::Carried;
use jc_formats::items::{Keeping, Kind};
use jc_formats::menu::{self, Frames, Menu, Piece, Rect, Style};
use jc_formats::name;
use jc_formats::notice;
use jc_formats::pad::{Pad, Repeat};
use jc_formats::player_menu::PASSES_A_SECOND;
use jc_formats::player_panel::{self, Drawn};
use jc_formats::rooms::{self, RoomTable};
use jc_formats::shop_panel;
use jc_formats::shop_screen::{self, BLACKSMITH, SHOP_FILE, SHOP_FILE_BASE, ShopScreen, Wares};
use jc_formats::text::{self, Font, Fonts};
use jc_formats::tim;
use jc_formats::window::{self, Faces};

use crate::music::Music;
use crate::name_entry::{Picture, SCREEN, WHOLE, blit};
use crate::options_view::{
    self, Parts, ThingLook, blit_lit, picture_in_row, scroll_signs, thing_looks,
};
use crate::place_map::picture;
use crate::pointer::{self, Aim, Pointer};
use crate::title::Press;

/// The longest stretch of time one pass of the app may stand for.
const LONGEST_PASS: f64 = 0.1;
/// Pictures of the screen in a game frame of the field engine.
const GAME_FRAME: u32 = 2;
/// The layer of what stands over a room, as the player's menu has it.
const OVER_THE_ROOM: usize = 4;
/// The room of chapter 0 whose keeper has the fifth stock: area 0, place 4.
const FIRST_ROOM: (usize, usize, usize) = (0, 0, 4);

type Failure = Box<dyn std::error::Error>;

/// `--shop=KIND[:ARGUMENT]`: the screen alone, kind 0 the blacksmith's and kind 1 the
/// shop's, with the argument of the script's command (0 without one).
#[derive(Resource)]
pub struct ShowShop {
    pub kind: i16,
    pub argument: i16,
    /// `--shop-money=N`: the money the player has for the look.
    pub money: Option<i32>,
    /// `--shop-items=ITEM[:COUNT],...` and `--shop-gear=KIND:THING[:COUNT],...`: items,
    /// and weapons (kind 0), armours (1) and third things (2), he is given for the
    /// look, so that he has something to sell.
    pub items: Vec<(i32, i32)>,
    pub gear: Vec<(usize, i32, i32)>,
}

impl ShowShop {
    /// The value of `--shop=`.
    pub fn of(value: &str) -> Option<(i16, i16)> {
        let (kind, argument) = match value.split_once(':') {
            Some((kind, argument)) => (kind, argument.parse().ok()?),
            None => (value, 0),
        };
        let kind = kind.parse().ok().filter(|kind| (0..=1).contains(kind))?;
        Some((kind, argument))
    }
}

/// `--shop-press=SECONDS:BUTTON,...`: the buttons a hidden run taps on the screen,
/// counted from the moment it opens.
#[derive(Resource, Default)]
pub struct ShopPresses(pub Vec<Press>);

/// What the screen puts over a room: its camera and its picture.
#[derive(Component)]
pub struct OverTheRoom;

/// The screen while it is up.
#[derive(Resource)]
pub struct ShopView {
    screen: ShopScreen,
    wares: Wares,
    /// The state of the game the screen deals on, and whether a deal has changed it.
    carried: Carried,
    changed: bool,
    pad: Option<Pad>,
    /// Passes of the screen's task so far, and the share of one that is owed.
    passes: u32,
    owed: f64,
    time: f64,
    presses: Vec<Press>,
    /// Whether a room stands behind the screen, to which it leaves.
    over_room: bool,
    /// The parts every screen of the menus is painted with, and the picture on screen.
    sprites: Vec<Picture>,
    frames: Frames,
    font: Font,
    font_picture: Picture,
    look: Look,
    /// The keeper's face: the face of thing 1 of the room behind the screen.
    face: Option<Picture>,
    shown: Entity,
    /// The row of the mouse and the wheel's notches on their way to the screen's
    /// cursor (`pointer.rs`).
    aim: Aim<shop_panel::Spot>,
}

/// What the screen brings in its own file and what its boxes are made of.
struct Look {
    /// The lines of the first menu and of the blacksmith's three kinds.
    first_lines: Vec<Vec<Piece>>,
    kind_lines: Vec<Vec<Piece>>,
    /// The three texts of each of the keeper's lines.
    keeper: Vec<[Vec<u16>; 3]>,
    /// The eight pictures of a window's frame and the fill's colour by window colour.
    window: Vec<Picture>,
    fills: [[u8; 3]; window::COLOURS],
    /// The pairs of the two lines of help: on a list, and in every other state.
    help_of_lists: Vec<(usize, Vec<u16>)>,
    help_plain: Vec<(usize, Vec<u16>)>,
    /// The four lines of a list, and what a row of one is painted with: every item's
    /// and every weapon's, armour's and third thing's small picture and name, the font
    /// and the digits' colour of a dim row, and the scroll sign by window colour.
    list_lines: Vec<Vec<Piece>>,
    items: Vec<ThingLook>,
    gear: [Vec<ThingLook>; 3],
    dim_font: Picture,
    dim_digits: [u8; 3],
    signs: Vec<Picture>,
    /// The four lines of every thing's description, by the list of [`Kind`].
    described: [Vec<[Vec<u16>; 4]>; 4],
    /// The player's record and the tables his five numbers are counted with, and the
    /// colours of a number that would stay, rise or fall.
    player: Record,
    fight: FightTables,
    digit_colours: [[u8; 3]; 3],
}

/// What a screen is made from, read from the disc.
struct Read {
    wares: Wares,
    stock: u16,
    sprites: Vec<Picture>,
    frames: Frames,
    font: Font,
    font_picture: Picture,
    look: Look,
    faces: Faces,
}

/// The pairs of a list of help of the executable, with their words.
fn help(exe: &Executable, list: u32) -> Result<Vec<(usize, Vec<u16>)>, Failure> {
    let pairs = player_panel::help_list(&exe.image, exe.base, list)
        .ok_or("a list of help is not where it should be")?;
    let pairs = (pairs.into_iter())
        .map(|(sprite, address)| {
            let said = exe.image.get(address.checked_sub(exe.base)? as usize..)?;
            Some((sprite, text::units(said)))
        })
        .collect::<Option<Vec<_>>>()
        .ok_or("a word of a list of help is not in the executable")?;
    Ok(pairs)
}

/// What is for sale, the bit of the stock for a room and the parts of the menus' screens.
fn read(carried: &Carried, argument: i16, room: Option<u16>) -> Result<Read, Failure> {
    let config = LocalConfig::load()?;
    let mut disc = DiscImage::open(&config.disc_image)?;
    let archive = Archive::open(&mut disc)?;
    let exe = Executable::read(&mut disc)?;
    let file = archive.read(&mut disc, SHOP_FILE)?;
    let wares = Wares::read(&file, &exe.image, exe.base).ok_or("the tables of the wares")?;
    let first = match room {
        Some(room) => {
            let engine = archive.read(&mut disc, rooms::FIELD_ENGINE_FILE)?;
            let table = RoomTable::from_field_engine(&engine)?;
            let at = table.by_number(room).map(|(at, _)| (at.area, at.place));
            at.is_some_and(|(area, place)| (carried.chapter, area, place) == FIRST_ROOM)
        }
        None => false,
    };
    let stock = shop_screen::stock(&carried.keeping, argument, first);
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
    let lines = |list: u32| {
        Menu::lines_at(&file, SHOP_FILE_BASE, list)
            .ok_or("the file of the shop's screen does not hold its lists of lines")
    };
    let things = notice::Tables::new(&exe.image, exe.base);
    let window = (0..8)
        .map(|part| {
            let image = font_set.get(window::FRAME_IMAGE + part);
            Picture::of(image.ok_or("the fonts' file has no frame of a window")?)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut described: [Vec<[Vec<u16>; 4]>; 4] = Default::default();
    for (list, texts) in described.iter_mut().enumerate() {
        *texts = (0..shop_panel::THINGS[list])
            .map(|thing| shop_panel::description_text(&file, SHOP_FILE_BASE, list, thing))
            .collect::<Option<Vec<_>>>()
            .ok_or("the file of the shop's screen does not hold its descriptions")?;
    }
    // The player as the game has him, or as a new game makes him.
    let player = match carried.player.clone() {
        Some(player) => player,
        None => {
            let title = archive.read(&mut disc, name::DEFAULT_NAME.0)?;
            Record::of_the_player(&title).ok_or("the title's file has no player")?
        }
    };
    let mut digit_colours = [[128; 3]; 3];
    for (entry, colour) in digit_colours.iter_mut().enumerate() {
        *colour = player_panel::digit_colour(&exe.image, exe.base, entry)
            .ok_or("the executable does not hold the colours of the digits")?;
    }
    let look = Look {
        player,
        fight: FightTables::read(&exe.image, exe.base)
            .ok_or("the executable does not hold the tables of the fights")?,
        digit_colours,
        described,
        first_lines: lines(shop_panel::FIRST_LINES)?,
        kind_lines: lines(shop_panel::KIND_LINES)?,
        keeper: shop_panel::keeper_texts(&file, SHOP_FILE_BASE, argument)
            .ok_or("the file of the shop's screen does not hold the keeper's lines")?,
        window,
        fills: window::fill_colours(&exe.image, exe.base)
            .ok_or("the executable does not hold the colours of a window")?,
        help_of_lists: help(&exe, shop_panel::HELP_OF_LISTS)?,
        help_plain: help(&exe, shop_panel::HELP_PLAIN)?,
        list_lines: lines(shop_panel::LIST_LINES)?,
        items: thing_looks(&things, &font_set, 0, wares.prices[0].len() as i32),
        gear: std::array::from_fn(|kind| {
            let count = wares.prices[kind + 1].len() as i32;
            thing_looks(&things, &font_set, kind as i32 + 1, count)
        }),
        dim_font: picture_in_row(&font_set, font.image, 1)?,
        dim_digits: player_panel::dim_digits(&exe.image, exe.base)
            .ok_or("the executable does not hold the colours of the digits")?,
        signs: scroll_signs(&exe, &font_set)?,
    };
    let faces = Faces::from_executable(&exe.image, exe.base)
        .ok_or("the executable does not hold the places of the faces")?;
    Ok(Read {
        wares,
        stock,
        sprites,
        frames,
        font,
        font_picture,
        look,
        faces,
    })
}

impl ShopView {
    fn open(
        (kind, argument): (i16, i16),
        carried: Carried,
        room: Option<u16>,
        face: impl FnOnce(&Faces) -> Option<Vec<u8>>,
        presses: Vec<Press>,
        shown: Entity,
    ) -> Result<Self, Failure> {
        let Read {
            wares,
            stock,
            sprites,
            frames,
            font,
            font_picture,
            look,
            faces,
        } = read(&carried, argument, room)?;
        let side = window::FACE as usize;
        let face = face(&faces).map(|rgba| Picture {
            rgba,
            wide: side,
            high: side,
        });
        let kinds = [Kind::Item, Kind::Weapon, Kind::Armour, Kind::Ornament];
        let offer = kinds.map(|list| {
            (0..wares.offers(list, stock))
                .map(|n| {
                    let thing = wares.nth(n, list, stock);
                    (thing, wares.price(thing, list))
                })
                .collect::<Vec<_>>()
        });
        info!(
            "the shop's screen: {} with argument {argument}, the stock word {stock}; on \
             offer (thing, price): items {:?}, weapons {:?}, armours {:?}, third things {:?}; \
             the player has {} money",
            if kind == BLACKSMITH {
                "the blacksmith's"
            } else {
                "the town shop's"
            },
            offer[0],
            offer[1],
            offer[2],
            offer[3],
            carried.keeping.money()
        );
        Ok(Self {
            screen: ShopScreen::open(kind, argument, stock),
            wares,
            carried,
            changed: false,
            pad: None,
            passes: 0,
            owed: 0.0,
            time: 0.0,
            presses,
            over_room: room.is_some(),
            sprites,
            frames,
            font,
            font_picture,
            look,
            face,
            shown,
            aim: Aim::default(),
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

    fn gear_look(&self, kind: usize, thing: u8) -> Option<&ThingLook> {
        self.look.gear.get(kind)?.get(usize::from(thing))
    }

    /// The small picture of a thing of a list, at half its brightness when dim.
    fn small_picture(
        &self,
        screen: &mut [u8],
        look: Option<&ThingLook>,
        at: (i16, i16),
        dim: bool,
        clip: Rect,
    ) {
        if let Some((Some(picture), _)) = look {
            let colour = [menu::brightness(if dim { menu::piece::DIM } else { 0 }); 3];
            blit_lit(screen, picture, at, colour, clip);
        }
    }

    /// The name of a thing of a list, in the font's second row of colours when dim.
    fn name(
        &self,
        screen: &mut [u8],
        look: Option<&ThingLook>,
        at: (i16, i16),
        dim: bool,
        clip: Rect,
    ) {
        if let Some((_, name)) = look {
            let picture = match dim {
                true => &self.look.dim_font,
                false => &self.font_picture,
            };
            options_view::glyphs(screen, &self.font, picture, name, at, clip);
        }
    }

    /// A line of the description of a thing of a list of the player's.
    fn described(&self, screen: &mut [u8], list: usize, thing: u8, line: usize, at: (i16, i16)) {
        let texts = self.look.described.get(list);
        let lines = texts.and_then(|texts| texts.get(usize::from(thing)));
        if let Some(said) = lines.and_then(|lines| lines.get(line)) {
            self.parts().text(screen, said, at, WHOLE);
        }
    }

    /// The first five of the values a fight counts with, with these things on.
    fn values_with(&self, worn: [u8; 3]) -> [u8; 5] {
        let mut dressed = self.look.player.clone();
        dressed.equipment = worn;
        let counted = dressed.effective(&self.look.fight).stats;
        std::array::from_fn(|n| counted[n])
    }

    /// One thing of a panel; `colour` is the window colour of the settings.
    fn put(&self, screen: &mut [u8], colour: i16, drawn: Drawn) {
        let parts = self.parts();
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
            Drawn::LongNumber { value, ends, clip } => {
                let width = |number: usize| parts.width(number);
                for (number, at) in player_panel::long_digits(value, ends, width) {
                    parts.sprite(screen, number, at, clip);
                }
            }
            Drawn::WindowFrame { rect } => {
                options_view::window_frame(screen, &self.look.window, rect);
            }
            Drawn::WindowFill { rect } => {
                let Some(fill) = usize::try_from(colour)
                    .ok()
                    .and_then(|colour| self.look.fills.get(colour))
                else {
                    return;
                };
                options_view::window_fill(screen, &self.look.window, *fill, rect);
            }
            Drawn::KeeperFace { at } => {
                if let Some(face) = &self.face {
                    blit(screen, face, (0, 0), (face.wide, face.high), at, WHOLE);
                }
            }
            Drawn::KeeperSaid { line, text, at } => {
                let texts = usize::try_from(line)
                    .ok()
                    .and_then(|line| self.look.keeper.get(line));
                if let Some(said) = texts.and_then(|texts| texts.get(text)) {
                    parts.text(screen, said, at, WHOLE);
                }
            }
            Drawn::Sign { turns, at, time } => {
                if let Some(image) = usize::try_from(colour)
                    .ok()
                    .and_then(|colour| self.look.signs.get(colour))
                {
                    options_view::put_sign(screen, image, turns, at, time);
                }
            }
            Drawn::ItemPicture {
                item,
                at,
                dim,
                clip,
            } => self.small_picture(
                screen,
                self.look.items.get(usize::from(item)),
                at,
                dim,
                clip,
            ),
            Drawn::GearPicture {
                kind,
                thing,
                at,
                dim,
                clip,
            } => self.small_picture(screen, self.gear_look(kind, thing), at, dim, clip),
            Drawn::ItemName {
                item,
                at,
                dim,
                clip,
            } => self.name(
                screen,
                self.look.items.get(usize::from(item)),
                at,
                dim,
                clip,
            ),
            Drawn::GearName {
                kind,
                thing,
                at,
                dim,
                clip,
            } => self.name(screen, self.gear_look(kind, thing), at, dim, clip),
            Drawn::Count {
                value,
                ends,
                dim,
                clip,
            } => {
                let colour = if dim { self.look.dim_digits } else { [128; 3] };
                let width = |number: usize| parts.width(number);
                for (number, at) in player_panel::digits(value, ends, false, width) {
                    if let Some(sprite) = self.sprites.get(number) {
                        blit_lit(screen, sprite, at, colour, clip);
                    }
                }
            }
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
            Drawn::Tinted {
                value,
                ends,
                colour,
                clip,
            } => {
                let colour = self.look.digit_colours.get(colour).copied();
                let width = |number: usize| parts.width(number);
                for (number, at) in player_panel::digits(value, ends, false, width) {
                    if let Some(sprite) = self.sprites.get(number) {
                        blit_lit(screen, sprite, at, colour.unwrap_or([128; 3]), clip);
                    }
                }
            }
            Drawn::Described { item, line, at } => self.described(screen, 0, item, line, at),
            Drawn::GearDescribed {
                kind,
                thing,
                line,
                at,
            } => self.described(screen, kind + 1, thing, line, at),
            // The screen draws no other kind of piece yet.
            _ => {}
        }
    }

    /// The screen of a pass, four bytes a pixel: the wall, which every routine of the
    /// task draws first (`0x8002BE6C`), the panels that are in code over it, and the
    /// line of help of the state. `colour` is the window colour of the settings.
    fn paint(&self, colour: i16) -> Vec<u8> {
        let mut screen = [0, 0, 0, 255].repeat(SCREEN.0 * SCREEN.1);
        let parts = self.parts();
        parts.wall(&mut screen);
        // What the thing under the cursor of the blacksmith's offer would make of his
        // numbers (`0x8002E2E4`).
        let values = self.values_with(self.carried.worn);
        let would = match shop_panel::weighed(&self.screen, &self.wares) {
            Some((kind, thing)) => {
                let mut worn = self.carried.worn;
                worn[kind] = thing;
                self.values_with(worn)
            }
            None => values,
        };
        let shown = shop_panel::Shown {
            values,
            would,
            first_lines: &self.look.first_lines,
            kind_lines: &self.look.kind_lines,
            list_lines: &self.look.list_lines,
            wares: &self.wares,
            carried: &self.carried,
            circle: (self.pad).is_some_and(|pad| pad.held & jc_formats::pad::CIRCLE != 0),
        };
        let time = menu::frame_time(self.passes, GAME_FRAME);
        let width = |number: usize| parts.width(number);
        for drawn in shop_panel::drawn(&self.screen, &shown, time, width) {
            self.put(&mut screen, colour, drawn);
        }
        let help = match shop_panel::help(&self.screen) {
            shop_panel::HELP_OF_LISTS => &self.look.help_of_lists,
            _ => &self.look.help_plain,
        };
        parts.legend(&mut screen, colour, help);
        screen
    }

    /// What a line of the log says of where the screen stands.
    fn place(&self) -> (i16, i16, i16, u8, i32, i16, i16) {
        let screen = &self.screen;
        (
            screen.state,
            screen.cursor.at,
            screen.cursor.first,
            screen.thing,
            screen.amount,
            screen.refusal,
            screen.line,
        )
    }

    /// The player's list of a kind as (thing, count) pairs.
    fn listed(keeping: &Keeping, list: Kind) -> Vec<(u8, u8)> {
        (0..=keeping.last_place(list))
            .map(|place| {
                (
                    keeping.number_at(place, list),
                    keeping.count_at(place, list),
                )
            })
            .collect()
    }
}

pub fn over_the_room(
    asked: (i16, i16),
    carried: Carried,
    room: u16,
    face: impl FnOnce(&Faces) -> Option<Vec<u8>>,
    presses: &ShopPresses,
    images: &mut Assets<Image>,
    commands: &mut Commands,
) -> Result<(), Failure> {
    let (wide, high) = (SCREEN.0 as u32, SCREEN.1 as u32);
    let black = [0, 0, 0, 255].repeat(SCREEN.0 * SCREEN.1);
    let shown = crate::spawn_picture(commands, images, wide, high, black);
    let presses = presses.0.clone();
    let view = match ShopView::open(asked, carried, Some(room), face, presses, shown) {
        Ok(view) => view,
        Err(err) => {
            commands.entity(shown).despawn();
            return Err(err);
        }
    };
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

/// `--shop`: the screen alone.
pub fn show_shop(
    show: Res<ShowShop>,
    presses: Res<ShopPresses>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    commands.spawn(Camera2d);
    let (wide, high) = (SCREEN.0 as u32, SCREEN.1 as u32);
    let black = [0, 0, 0, 255].repeat(SCREEN.0 * SCREEN.1);
    let shown = crate::spawn_picture(&mut commands, &mut images, wide, high, black);
    let mut carried = Carried::new_game();
    if let Some(money) = show.money {
        let has = carried.keeping.money();
        carried.keeping.take_money(has);
        carried.keeping.give_money(money);
    }
    let lists = [Kind::Weapon, Kind::Armour, Kind::Ornament];
    let items = show
        .items
        .iter()
        .map(|&(item, count)| (item, count, Kind::Item));
    let gear = (show.gear.iter()).map(|&(kind, thing, count)| (thing, count, lists[kind]));
    for (thing, count, list) in items.chain(gear) {
        let worn = carried.worn;
        if !carried.keeping.give(thing, count, list, worn) {
            error!("the shop's screen: {count} of thing {thing} do not fit the list {list:?}");
            exit.write(AppExit::error());
            return;
        }
    }
    let asked = (show.kind, show.argument);
    match ShopView::open(asked, carried, None, |_| None, presses.0.clone(), shown) {
        Ok(view) => commands.insert_resource(view),
        Err(err) => {
            error!("the shop's screen: {err}");
            exit.write(AppExit::error());
        }
    }
}

/// Runs the passes of the screen for the time gone by.
#[allow(clippy::too_many_arguments)]
pub fn run_shop(
    current: Option<ResMut<ShopView>>,
    mut music: ResMut<Music>,
    settings: Res<crate::settings::Settings>,
    time: Res<Time<Virtual>>,
    keys: Res<crate::keys::Pad>,
    mut images: ResMut<Assets<Image>>,
    mut sprites: Query<&mut Sprite>,
    over_room: Query<Entity, With<OverTheRoom>>,
    mut things: Option<ResMut<crate::shown::Things>>,
    mut commands: Commands,
    pointer: Pointer,
) {
    let Some(mut current) = current else {
        return;
    };
    // The mouse over the rows of the state, and the wheel a row a pass.
    let under = pointer.on_picture(current.shown, SCREEN);
    let look = &current.look;
    let lines = [
        &look.first_lines[..],
        &look.kind_lines[..],
        &look.list_lines[..],
    ];
    let spots = shop_panel::spots(&current.screen, lines);
    let under = pointer::row_under(spots, under);
    current.aim.look(pointer.mouse(), under);
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
    // is still down from the room did not go down on the screen.
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
        let mut pad = current.pad.unwrap_or_default();
        pad.read(held, Repeat::of(GAME_FRAME), passes);
        let view = &mut *current;
        let here = shop_panel::here(&view.screen);
        (view.aim).steer(&mut pad, here, |spot| shop_panel::way(&view.screen, spot));
        current.pad = Some(pad);
        let before = current.place();
        let current = &mut *current;
        let money = current.carried.keeping.money();
        let pass = current
            .screen
            .pass(&current.wares, &mut current.carried, &pad);
        // `money_multiplier` (`assist.rs`): a sale brings more or less, and never more
        // than the things cost to buy.
        if let Some((thing, list, amount)) = pass.sold {
            let cost = current.wares.price(thing, list).saturating_mul(amount);
            let times = crate::assist::now().money;
            crate::assist::given_money(&mut current.carried.keeping, money, times, Some(cost));
        }
        for &sound in &pass.sounds {
            music.sound(sound.into());
        }
        let keeping = &current.carried.keeping;
        if let Some((thing, list, amount)) = pass.bought {
            current.changed = true;
            info!(
                "the shop's screen: pass {passes}: {amount} of thing {thing} of the list \
                 {list:?} bought for {} each: {} money left, the list (thing, count) {:?}",
                current.wares.price(thing, list),
                keeping.money(),
                ShopView::listed(keeping, list)
            );
        }
        if let Some((thing, list, amount)) = pass.sold {
            current.changed = true;
            info!(
                "the shop's screen: pass {passes}: {amount} of thing {thing} of the list \
                 {list:?} sold for {} each: {} money now, the list (thing, count) {:?}",
                current.wares.worth(thing, list),
                keeping.money(),
                ShopView::listed(keeping, list)
            );
        }
        if let Some((thing, kind)) = pass.put_on {
            current.changed = true;
            info!(
                "the shop's screen: pass {passes}: thing {thing} of kind {kind} is put on: \
                 he has on {:?}",
                current.carried.worn
            );
        }
        let (state, at, first, thing, amount, refusal, line) = current.place();
        if current.place() != before || !pass.sounds.is_empty() {
            info!(
                "the shop's screen: pass {passes}: state {state}, the cursor on {at} from \
                 line {first}, thing {thing}, amount {amount}, refusal {refusal}, the \
                 keeper's line {line}, panels {:#x}, sounds {:?}",
                current.screen.shown, pass.sounds
            );
        }
        if !pass.ended {
            continue;
        }
        if !current.over_room {
            // Alone, the view has nowhere to go: the screen's task starts again.
            info!("the shop's screen: pass {passes}: the screen leaves, and comes up again");
            let screen = &current.screen;
            current.screen = ShopScreen::open(screen.way, screen.argument, screen.stock);
            continue;
        }
        info!(
            "the shop's screen: pass {passes}: the screen leaves with {} money, the room \
             runs on",
            current.carried.keeping.money()
        );
        if let Some(things) = things.as_mut() {
            if current.changed {
                let carried = &current.carried;
                things.kept(carried.keeping.clone(), None, carried.worn, None);
            }
            things.screen_left();
        }
        for entity in &over_room {
            commands.entity(entity).despawn();
        }
        commands.remove_resource::<ShopView>();
        return;
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
