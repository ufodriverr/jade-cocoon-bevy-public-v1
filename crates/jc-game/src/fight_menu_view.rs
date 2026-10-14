
use bevy::prelude::*;
use jc_disc::{Archive, DiscImage, Executable, LocalConfig};
use jc_formats::battle::hud;
use jc_formats::battle::menu::Menu;
use jc_formats::battle::menu_panel::{self, Drawn, LEVELS, Shapes, Tables};
use jc_formats::menu::{self, Frames, Piece, Rect, Style, brightness, frame_time, piece};
use jc_formats::message;
use jc_formats::name::{self, LegendPiece};
use jc_formats::notice;
use jc_formats::player_panel::{self, attacks, items, spells};
use jc_formats::rooms;
use jc_formats::text::{self, Font, Fonts};
use jc_formats::tim;

use crate::name_entry::{Picture, SCREEN, WHOLE, blend, blit};
use crate::options_view::{
    ThingLook, blit_lit, glyphs, picture_in_row, put_sign, scroll_signs, thing_looks,
};
use crate::place_map::picture;
use crate::player_menu::{Bars, bars, put_bar};
use crate::scripts::FightMenu;
use crate::shown::Things;
use crate::text_view::half;

type Failure = Box<dyn std::error::Error>;

/// Pictures of the screen in a game frame of the field engine.
const GAME_FRAME: u32 = 2;
/// Where the menu's picture lies on the depth axis of the field's screen: in front of
/// every layer and thing, and behind a message window.
const IN_FRONT: f32 = 9.5;

/// What the menu is painted with: the menus' sprite pool, the tables of the frames, the
/// font of the texts, the menu's own tables and the pairs of each level's line of help.
struct Look {
    sprites: Vec<Picture>,
    frames: Frames,
    font: Font,
    font_picture: Picture,
    tables: Tables,
    legends: [Vec<(usize, Vec<u16>)>; LEVELS],
    /// The executable's four lines of a list of items, the small picture and the name
    /// of every item, and what a dim row is painted with: the font's second row of
    /// colours and the colour of its digits; the scroll sign in every window colour.
    list: Vec<Vec<Piece>>,
    items: Vec<ThingLook>,
    dim_font: Picture,
    dim_digits: [u8; 3],
    signs: Vec<Picture>,
    /// The small font of a name in the picker of a target, with its second row of
    /// colours for a closed row, and the name a new game starts with.
    small: Font,
    small_picture: Picture,
    dim_small: Picture,
    name: Vec<u16>,
    /// The bars of the gauges.
    bars: Bars,
    /// The two lists of a creature: their lines and sprites, and the names of the
    /// spells, of the body attacks and of the grades.
    shapes: Shapes,
    spell_names: Vec<Vec<u16>>,
    attack_names: Vec<Vec<u16>>,
    grade_names: Vec<Vec<u16>>,
}

impl Look {
    fn read() -> Result<Self, Failure> {
        let config = LocalConfig::load()?;
        let mut disc = DiscImage::open(&config.disc_image)?;
        let archive = Archive::open(&mut disc)?;
        let exe = Executable::read(&mut disc)?;
        let file = archive.read(&mut disc, menu_panel::FILE)?;
        let tables = Tables::from_file(&file, menu_panel::FILE_BASE)
            .ok_or("the file of the fights does not hold the tables of its menu")?;
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
        // The lists of the lines of help and their words lie in the executable.
        let legend = |list: u32| {
            let pairs = player_panel::help_list(&exe.image, exe.base, list)?;
            (pairs.into_iter())
                .map(|(sprite, address)| {
                    let said = exe.image.get(address.checked_sub(exe.base)? as usize..)?;
                    Some((sprite, text::units(said)))
                })
                .collect::<Option<Vec<_>>>()
        };
        let mut legends: [Vec<(usize, Vec<u16>)>; LEVELS] = Default::default();
        for (pairs, &list) in legends.iter_mut().zip(&tables.legends) {
            *pairs = legend(list).ok_or("the executable does not hold a line of help")?;
        }
        let things = notice::Tables::new(&exe.image, exe.base);
        let (name_file, address) = name::DEFAULT_NAME;
        let title = archive.read(&mut disc, name_file)?;
        let named = (address.checked_sub(rooms::load_address(name_file)))
            .and_then(|offset| title.get(offset as usize..))
            .map(text::units)
            .ok_or("the default name is not in its file")?;
        let names = |count: usize, name: fn(&[u8], u32, usize) -> Option<Vec<u16>>| {
            (0..count)
                .map(|n| name(&exe.image, exe.base, n))
                .collect::<Option<Vec<_>>>()
                .ok_or("the executable does not hold the names of a creature's lists")
        };
        Ok(Self {
            shapes: Shapes::from_executable(&exe.image, exe.base)
                .ok_or("the executable does not hold the lists of a creature")?,
            spell_names: names(spells::SPELLS, player_panel::spell_name)?,
            attack_names: names(attacks::ATTACKS, player_panel::attack_name)?,
            grade_names: names(attacks::GRADES, player_panel::grade_name)?,
            small_picture: picture(&font_set, small.image)?,
            dim_small: picture_in_row(&font_set, small.image, 1)?,
            bars: bars(&font_set)?,
            small,
            name: named,
            list: menu::Menu::lines_at(&exe.image, exe.base, items::LINES)
                .ok_or("the executable does not hold the lines of a list of items")?,
            items: thing_looks(&things, &font_set, 0, items::ITEMS.into()),
            dim_font: picture_in_row(&font_set, font.image, 1)?,
            dim_digits: player_panel::dim_digits(&exe.image, exe.base)
                .ok_or("the executable does not hold the colours of the digits")?,
            signs: scroll_signs(&exe, &font_set)?,
            font_picture: picture(&font_set, font.image)?,
            sprites,
            frames,
            font,
            tables,
            legends,
        })
    }

    /// A frame of the menus over what the screen holds, or over the room where the
    /// screen holds nothing yet.
    fn frame(&self, screen: &mut [u8], rect: Rect, style: Style, time: u32) {
        let Some(colour) = self.frames.colour(style, time) else {
            return;
        };
        let quads = self.frames.quads(rect, Some(colour));
        for y in rect.y..rect.y.saturating_add(rect.h.max(0)) {
            for x in rect.x..rect.x.saturating_add(rect.w.max(0)) {
                let Some(quad) = quads.iter().find(|quad| quad.covers(x, y)) else {
                    continue;
                };
                match style.blend_mode() {
                    Some(0) => half(screen, x.into(), y.into(), quad.colour),
                    mode => blend(screen, x, y, quad.colour, mode),
                }
            }
        }
    }

    fn sprite(&self, screen: &mut [u8], number: usize, at: (i16, i16), clip: Rect) {
        if let Some(sprite) = self.sprites.get(number) {
            blit(screen, sprite, (0, 0), (sprite.wide, sprite.high), at, clip);
        }
    }

    /// The line of help of a level in a window colour (`0x8002B064`): its frame, added
    /// last, lies under the signs and the words.
    fn legend(&self, screen: &mut [u8], colour: i16, pairs: &[(usize, Vec<u16>)]) {
        let legend = name::legend(colour);
        self.frame(screen, legend.rect, legend.style, 0);
        let sizes = |number: usize| {
            let sprite = self.sprites.get(number)?;
            Some((sprite.wide as i16, sprite.high as i16))
        };
        for piece in name::legend_pieces(pairs, &self.font, sizes).unwrap_or_default() {
            match piece {
                LegendPiece::Sign { sprite, at } => self.sprite(screen, sprite, at, WHOLE),
                LegendPiece::Word { units, at } => {
                    glyphs(screen, &self.font, &self.font_picture, &units, at, WHOLE);
                }
            }
        }
    }

    /// The name of a slot's fighter in capitals: `name` for the player, the name of
    /// its record for an opponent and a carried creature.
    fn slot_name(&self, name: &[u16], shown: &FightMenu, slot: usize) -> Option<Vec<u16>> {
        let units = match slot {
            0 => name.to_vec(),
            _ => name::units(shown.names.get(slot)?.as_ref()?, &self.small),
        };
        Some(units.into_iter().map(player_panel::in_capitals).collect())
    }

    fn boxed(
        &self,
        screen: &mut [u8],
        colour: i16,
        name: &[u16],
        shown: &FightMenu,
        drawn: hud::Drawn,
    ) {
        use player_panel::Drawn as Boxed;
        let width =
            |number: usize| (self.sprites.get(number)).map_or(0, |sprite| sprite.wide as i16);
        match drawn {
            hud::Drawn::Name { slot, at, clip } => {
                let Some(units) = self.slot_name(name, shown, slot) else {
                    return;
                };
                glyphs(screen, &self.small, &self.small_picture, &units, at, clip);
            }
            hud::Drawn::Boxed(Boxed::Frame { rect, mode, time }) => {
                let style = Style {
                    kind: colour,
                    mode,
                    blend: 1,
                };
                self.frame(screen, rect, style, time);
            }
            hud::Drawn::Boxed(Boxed::Sprite { number, at, clip }) => {
                self.sprite(screen, number, at, clip);
            }
            hud::Drawn::Boxed(Boxed::Number {
                value,
                ends,
                small,
                clip,
            }) => {
                for (number, at) in player_panel::digits(value, ends, small, width) {
                    self.sprite(screen, number, at, clip);
                }
            }
            hud::Drawn::Boxed(Boxed::Bar {
                picture,
                length,
                at,
                clip,
            }) => {
                if let Some(rows) = self.bars.get(&picture) {
                    put_bar(screen, rows, length, at, clip);
                }
            }
            // A box draws nothing else.
            hud::Drawn::Boxed(_) => {}
        }
    }

    /// A thing of a list or of the picker of a target, as the screens of the menus paint
    /// it. `names` are the player's name and the species of the fighter of each slot.
    fn listed(
        &self,
        screen: &mut [u8],
        colour: i16,
        names: (&[u16], &FightMenu),
        drawn: player_panel::Drawn,
    ) {
        let (name, shown) = names;
        use player_panel::Drawn as Listed;
        use player_panel::Listed as Row;
        let look = |item: u8| self.items.get(usize::from(item));
        match drawn {
            Listed::Frame { rect, mode, time } => {
                let style = Style {
                    kind: colour,
                    mode,
                    blend: 1,
                };
                self.frame(screen, rect, style, time);
            }
            Listed::Sign { turns, at, time } => {
                let sign = usize::try_from(colour).ok();
                if let Some(image) = sign.and_then(|colour| self.signs.get(colour)) {
                    put_sign(screen, image, turns, at, time);
                }
            }
            Listed::ItemPicture {
                item,
                at,
                dim,
                clip,
            } => {
                if let Some((Some(picture), _)) = look(item) {
                    let colour = [brightness(if dim { piece::DIM } else { 0 }); 3];
                    blit_lit(screen, picture, at, colour, clip);
                }
            }
            Listed::ItemName {
                item,
                at,
                dim,
                clip,
            } => {
                if let Some((_, name)) = look(item) {
                    let picture = match dim {
                        true => &self.dim_font,
                        false => &self.font_picture,
                    };
                    glyphs(screen, &self.font, picture, name, at, clip);
                }
            }
            Listed::Count {
                value,
                ends,
                dim,
                clip,
            } => {
                let colour = if dim { self.dim_digits } else { [128; 3] };
                let width = |number: usize| {
                    (self.sprites.get(number)).map_or(0, |sprite| sprite.wide as i16)
                };
                for (number, at) in player_panel::digits(value, ends, false, width) {
                    if let Some(sprite) = self.sprites.get(number) {
                        blit_lit(screen, sprite, at, colour, clip);
                    }
                }
            }
            // The name of the player or of the carried creature of a slot, in capitals
            // about the middle of its frame.
            Listed::PickedName {
                target,
                middle,
                dim,
                clip,
            } => {
                let Some(name) = self.slot_name(name, shown, target) else {
                    return;
                };
                let width = self.small.length(&self.small.codes(&name));
                let picture = match dim {
                    true => &self.dim_small,
                    false => &self.small_picture,
                };
                let at = (middle.0.wrapping_sub(width / 2), middle.1);
                glyphs(screen, &self.small, picture, &name, at, clip);
            }
            // A piece of a creature's lists: a sprite of the pool or a name in the font
            // of the texts, each placed in its room by the piece's flags.
            Listed::Listed {
                what: Row::Sprite(number),
                room,
                flags,
                dim,
                clip,
            } => {
                if let Some(sprite) = self.sprites.get(number) {
                    let at = menu::placed(flags, room, sprite.wide as i16);
                    let colour = [brightness(if dim { piece::DIM } else { 0 }); 3];
                    blit_lit(screen, sprite, at, colour, clip);
                }
            }
            Listed::Listed {
                what,
                room,
                flags,
                dim,
                clip,
            } => {
                let name = match what {
                    Row::Spell(spell) => self.spell_names.get(spell),
                    Row::Attack(attack) => self.attack_names.get(attack),
                    Row::Grade(grade) => self.grade_names.get(grade),
                    Row::Sprite(_) => None,
                };
                if let Some(name) = name {
                    let width = self.font.length(&self.font.codes(name));
                    let picture = match dim {
                        true => &self.dim_font,
                        false => &self.font_picture,
                    };
                    let at = menu::placed(flags, room, width);
                    glyphs(screen, &self.font, picture, name, at, clip);
                }
            }
            Listed::ListDescribed { what, line, at } => {
                let lines = match what {
                    Row::Spell(spell) => self.tables.spell_texts.get(spell),
                    Row::Attack(attack) => self.tables.attack_texts.get(attack),
                    _ => None,
                };
                if let Some(units) = lines.and_then(|lines| lines.get(line)) {
                    glyphs(screen, &self.font, &self.font_picture, units, at, WHOLE);
                }
            }
            // The lists and the picker draw nothing else.
            _ => {}
        }
    }

    /// The screen of the menu's last pass, four bytes a pixel, clear where the fight
    /// shows through. `colour` is the window colour of the settings and `named` the name
    /// the player was given, empty for the one a new game starts with.
    fn paint(&self, shown: &FightMenu, colour: i16, named: &[u16]) -> Vec<u8> {
        let name = if named.is_empty() { &self.name } else { named };
        let (menu, game_frame): (Option<&Menu>, u32) = (shown.menu, shown.game_frame);
        let mut screen = [0, 0, 0, 0].repeat(SCREEN.0 * SCREEN.1);
        let level = menu.map(|menu| usize::from(menu.drew().level));
        if let Some(pairs) = level.and_then(|level| self.legends.get(level)) {
            self.legend(&mut screen, colour, pairs);
        }
        let width =
            |number: usize| (self.sprites.get(number)).map_or(0, |sprite| sprite.wide as i16);
        let text_width = |units: &[u16]| message::width(&self.font, units, &[]);
        let time = frame_time(game_frame, GAME_FRAME);
        // The boxes of the HUD; a list or a text of the menu has them left out, so the
        // two never lie on one another.
        for drawn in &shown.hud {
            self.boxed(&mut screen, colour, name, shown, *drawn);
        }
        // While a command plays there is no menu: the HUD alone.
        let Some(menu) = menu else {
            return screen;
        };
        let moves = shown.moves.as_ref().map(|moves| (&self.shapes, moves));
        let list = (&self.list[..], &shown.items, moves);
        let panel = menu_panel::drawn(
            menu,
            &self.tables,
            list,
            &shown.own,
            time,
            width,
            text_width,
        );
        for drawn in panel {
            match drawn {
                Drawn::Frame { rect, mode, time } => {
                    let style = Style {
                        kind: colour,
                        mode,
                        blend: 1,
                    };
                    self.frame(&mut screen, rect, style, time);
                }
                Drawn::Sprite {
                    number,
                    at,
                    dim: false,
                    clip,
                } => self.sprite(&mut screen, number, at, clip),
                Drawn::Sprite {
                    number, at, clip, ..
                } => {
                    if let Some(sprite) = self.sprites.get(number) {
                        let colour = [brightness(piece::DIM); 3];
                        blit_lit(&mut screen, sprite, at, colour, clip);
                    }
                }
                Drawn::Text { text, at } => {
                    if let Some(units) = self.tables.texts.get(text) {
                        glyphs(
                            &mut screen,
                            &self.font,
                            &self.font_picture,
                            units,
                            at,
                            WHOLE,
                        );
                    }
                }
                Drawn::Described { item, line, at } => {
                    let lines = self.tables.described.get(usize::from(item));
                    if let Some(units) = lines.and_then(|lines| lines.get(line)) {
                        glyphs(
                            &mut screen,
                            &self.font,
                            &self.font_picture,
                            units,
                            at,
                            WHOLE,
                        );
                    }
                }
                Drawn::Listed(listed) => self.listed(&mut screen, colour, (name, shown), listed),
            }
        }
        screen
    }
}

/// What paints the menu of a room's fights: the disc's pictures, read when the first
/// menu comes up, the sprite, and the game frame that is on screen.
#[derive(Default)]
pub struct Painter {
    look: Option<Look>,
    /// The disc could not be read: said once.
    failed: bool,
    sprite: Option<Entity>,
    /// The game frame of the scripts that is on screen; none without a fight's HUD or
    /// menu. And whether the menu was in the last picture, for the log.
    painted: Option<u32>,
    menu: bool,
}

/// Paints the command menu of the room's fight over the room, as the last game frame
/// left it, in the window colour of the game's settings.
pub fn paint_fight_menu(
    things: Option<Res<Things>>,
    settings: Res<crate::settings::Settings>,
    named: Option<Res<crate::name_entry::Named>>,
    mut painter: Local<Painter>,
    mut images: ResMut<Assets<Image>>,
    mut sprites: Query<&mut Sprite>,
    mut commands: Commands,
) {
    let painter = &mut *painter;
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
    let Some(shown) = things.as_ref().and_then(|things| things.fight_menu()) else {
        // The HUD has gone, and the menu with it: the last picture goes too.
        painter.menu = false;
        if painter.painted.take().is_some() {
            show(
                [0, 0, 0, 0].repeat(SCREEN.0 * SCREEN.1),
                &mut painter.sprite,
            );
        }
        return;
    };
    let game_frame = shown.game_frame;
    if painter.painted == Some(game_frame) || painter.failed {
        return;
    }
    if painter.look.is_none() {
        match Look::read() {
            Ok(look) => painter.look = Some(look),
            Err(err) => {
                error!("the menu of a fight cannot be painted: {err}");
                painter.failed = true;
                return;
            }
        }
    }
    let Some(look) = &painter.look else {
        return;
    };
    if painter.painted.is_none() {
        info!("game frame {game_frame}: the HUD of the fight is painted");
    }
    if shown.menu.is_some() != painter.menu {
        painter.menu = shown.menu.is_some();
        match painter.menu {
            true => info!("game frame {game_frame}: the menu of the fight is painted"),
            false => info!("game frame {game_frame}: the menu of the fight is gone"),
        }
    }
    let colour = settings.0.colour as i16;
    let named = named.as_ref().map_or(&[][..], |named| &named.units);
    show(look.paint(&shown, colour, named), &mut painter.sprite);
    painter.painted = Some(game_frame);
}
