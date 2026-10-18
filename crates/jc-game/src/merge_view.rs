
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use jc_disc::{Archive, DiscImage, Executable, LocalConfig};
use jc_formats::battle::Tables as FightTables;
use jc_formats::battle::record::Random;
use jc_formats::carried::Carried;
use jc_formats::creatures::{self, Listed};
use jc_formats::items;
use jc_formats::menu::{self, Frames, Menu, Piece, Style, sign};
use jc_formats::merge_panel;
use jc_formats::merge_screen::{MERGE_FILE, MERGE_FILE_BASE, MergeScreen, flag};
use jc_formats::name;
use jc_formats::notice;
use jc_formats::options_panel;
use jc_formats::options_screen;
use jc_formats::pad::{Pad, Repeat};
use jc_formats::player_menu::PASSES_A_SECOND;
use jc_formats::player_panel::{self, AttackRow, Drawn, Family, attacks, message, spells};
use jc_formats::text::{self, Font, Fonts};
use jc_formats::tim;
use jc_formats::window::{self, Faces};

use crate::music::Music;
use crate::name_entry::{self, NameEntry, Picture, SCREEN, WHOLE, blend, blit};
use crate::options_view::{
    self, Parts, ThingLook, blit_lit, inside, picture_in_row, scroll_signs, thing_looks,
};
use crate::place_map::picture;
use crate::player_menu::{Bars, bars, put_bar, triangle};
use crate::pointer::{self, Aim, Pointer};
use crate::title::Press;

/// The longest stretch of time one pass of the app may stand for.
const LONGEST_PASS: f64 = 0.1;
/// Pictures of the screen in a game frame of the field engine.
const GAME_FRAME: u32 = 2;
/// The layer of what stands over a room, as the player's menu has it.
const OVER_THE_ROOM: usize = 4;
/// The row of the small font's colours a marked place has its name in (`0x8003E698(1,
/// 3)`).
const MARKED_ROW: usize = 3;
/// The image of the fonts' file that is the arrow of a difference (`0x80032050`).
const ARROW: usize = 88;

type Failure = Box<dyn std::error::Error>;
/// A line of the buttons: the address of its list, and its pairs of a button's sign
/// and a word.
type Legend = (u32, Vec<(usize, Vec<u16>)>);

/// `--merge-press=SECONDS:BUTTON,...`: the buttons a hidden run taps on the screen,
/// counted from the moment it opens. The time stands while the screen sleeps for the
/// room's part behind a spin.
#[derive(Resource, Default)]
pub struct MergePresses(pub Vec<Press>);

/// What the screen puts over a room: its camera and its picture.
#[derive(Component)]
pub struct OverTheRoom;

/// The screen while it is up.
#[derive(Resource)]
pub struct MergeView {
    screen: MergeScreen,
    /// The state of the game the screen works on: its creatures are in the screen's
    /// list until it leaves.
    carried: Carried,
    pad: Option<Pad>,
    /// Passes of the screen's task so far, and the share of one that is owed.
    passes: u32,
    owed: f64,
    time: f64,
    presses: Vec<Press>,
    look: Look,
    /// The keeper's face: the face of thing 1 of the room behind the screen.
    face: Option<Picture>,
    shown: Entity,
    /// The name entry on a creature's name, while the screen's task sleeps for it.
    naming: Option<NameEntry>,
    /// The screen's task sleeps behind an answer to the room's script: the room is
    /// seen and runs. And the script has asked for the screen again, which the next
    /// pass of the view shows.
    asleep: bool,
    comes_back: bool,
    /// The spot of the mouse on its way to the screen's cursor (`pointer.rs`).
    aim: Aim<merge_panel::Spot>,
}

/// The parts every screen of the menus is painted with, and what the screen brings in
/// its own file.
struct Look {
    sprites: Vec<Picture>,
    frames: Frames,
    font: Font,
    font_picture: Picture,
    /// The font of the texts in its second row of colours, for a dim piece of a list.
    dim_font: Picture,
    /// The small font of the names of the lists, as it is and in the colours of a
    /// marked place.
    small: Font,
    small_picture: Picture,
    marked_picture: Picture,
    /// The lines of the main menu and of the two lists.
    main_lines: Vec<Vec<Piece>>,
    carried_lines: Vec<Vec<Piece>>,
    kept_lines: Vec<Vec<Piece>>,
    /// The lines of the list of the minion skins, and the small picture and the name
    /// of each skin, a thing of the player's kind 4.
    skin_lines: Vec<Vec<Piece>>,
    skins: Vec<ThingLook>,
    /// The sign between the frames of a merge by window colour, the colours of a
    /// number that is compared and the arrow of a difference.
    between: Vec<Picture>,
    digit_colours: [[u8; 3]; 3],
    arrow: Picture,
    /// The three texts of each of the keeper's lines.
    keeper: Vec<[Vec<u16>; 3]>,
    /// The eight pictures of a window's frame and the fill's colour by window colour.
    window: Vec<Picture>,
    fills: [[u8; 3]; window::COLOURS],
    /// The pairs of the lines of the buttons, by the address of their list.
    legends: Vec<Legend>,
    /// The scroll sign by window colour, the sign of "more", and the colours of the
    /// gauges' bars.
    signs: Vec<Picture>,
    more: Picture,
    bars: Bars,
    fight: FightTables,
    /// A creature's two lists: the executable's lines, sprites and names, and the
    /// texts of the screen's own file.
    attack_lines: Vec<Vec<Piece>>,
    spell_lines: Vec<Vec<Piece>>,
    attack_pictures: [[u8; 5]; attacks::ROWS],
    spell_pictures: [u8; spells::SPELLS],
    attack_names: Vec<Vec<u16>>,
    grade_names: Vec<Vec<u16>>,
    spell_names: Vec<Vec<u16>>,
    attack_texts: Vec<[Vec<u16>; message::LINES]>,
    spell_texts: Vec<[Vec<u16>; message::LINES]>,
}

impl Look {
    fn read(argument: i16) -> Result<(Self, Faces), Failure> {
        let config = LocalConfig::load()?;
        let mut disc = DiscImage::open(&config.disc_image)?;
        let archive = Archive::open(&mut disc)?;
        let exe = Executable::read(&mut disc)?;
        let file = archive.read(&mut disc, MERGE_FILE)?;
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
        let lines = |list: u32| {
            Menu::lines_at(&file, MERGE_FILE_BASE, list)
                .ok_or("the file of the screen of the caught does not hold its lists of lines")
        };
        let window = (0..8)
            .map(|part| {
                let image = font_set.get(window::FRAME_IMAGE + part);
                Picture::of(image.ok_or("the fonts' file has no frame of a window")?)
            })
            .collect::<Result<Vec<_>, _>>()?;
        // A word of a line of the buttons lies in the screen's file or in the
        // executable, and so does the list itself.
        let legend = |list: u32| -> Result<Legend, Failure> {
            let pairs = match list.checked_sub(MERGE_FILE_BASE) {
                Some(_) => player_panel::help_list(&file, MERGE_FILE_BASE, list),
                None => player_panel::help_list(&exe.image, exe.base, list),
            };
            let pairs = pairs.ok_or("a line of the buttons is not where it should be")?;
            let pairs = (pairs.into_iter())
                .map(|(sprite, address)| {
                    let said = match address.checked_sub(MERGE_FILE_BASE) {
                        Some(offset) => file.get(offset as usize..),
                        None => exe.image.get(address.checked_sub(exe.base)? as usize..),
                    };
                    Some((sprite, text::units(said?)))
                })
                .collect::<Option<Vec<_>>>()
                .ok_or("a word of a line of the buttons is not to be found")?;
            Ok((list, pairs))
        };
        let names = |count: usize, name: &dyn Fn(usize) -> Option<Vec<u16>>| {
            (0..count)
                .map(name)
                .collect::<Option<Vec<_>>>()
                .ok_or("the executable does not hold the names of a creature's lists")
        };
        let things = notice::Tables::new(&exe.image, exe.base);
        let between = (0..options_panel::COLOURS)
            .map(|colour| {
                let family = merge_panel::BETWEEN_FAMILY;
                let image = sign::image(&exe.image, exe.base, colour, family)
                    .ok_or("the executable does not hold the signs of a merge")?;
                picture(&font_set, image)
            })
            .collect::<Result<Vec<_>, Failure>>()?;
        let mut digit_colours = [[128; 3]; 3];
        for (entry, colour) in digit_colours.iter_mut().enumerate() {
            *colour = player_panel::digit_colour(&exe.image, exe.base, entry)
                .ok_or("the executable does not hold the colours of the digits")?;
        }
        let look = Self {
            skin_lines: lines(merge_panel::SKIN_LINES)?,
            skins: thing_looks(&things, &font_set, 4, merge_panel::SKINS.into()),
            between,
            digit_colours,
            arrow: picture(&font_set, ARROW)?,
            attack_lines: Menu::lines_at(&exe.image, exe.base, attacks::LINES)
                .filter(|lines| lines.len() == attacks::ROWS)
                .ok_or("the executable does not hold the lines of the body attacks' list")?,
            spell_lines: Menu::lines_at(&exe.image, exe.base, spells::LINES)
                .filter(|lines| lines.len() == spells::ROWS)
                .ok_or("the executable does not hold the lines of the spells' list")?,
            attack_pictures: player_panel::attack_pictures(&exe.image, exe.base)
                .ok_or("the executable does not hold the sprites of the body attacks")?,
            spell_pictures: player_panel::spell_pictures(&exe.image, exe.base)
                .ok_or("the executable does not hold the sprites of the spells")?,
            attack_names: names(attacks::ATTACKS, &|attack| {
                player_panel::attack_name(&exe.image, exe.base, attack)
            })?,
            grade_names: names(attacks::GRADES, &|grade| {
                player_panel::grade_name(&exe.image, exe.base, grade)
            })?,
            spell_names: names(spells::SPELLS, &|spell| {
                player_panel::spell_name(&exe.image, exe.base, spell)
            })?,
            attack_texts: (0..attacks::ATTACKS)
                .map(|attack| merge_panel::attack_text(&file, MERGE_FILE_BASE, attack))
                .collect::<Option<Vec<_>>>()
                .ok_or("the file of the screen of the caught does not hold the attacks' texts")?,
            spell_texts: (0..spells::SPELLS)
                .map(|spell| merge_panel::spell_text(&file, MERGE_FILE_BASE, spell))
                .collect::<Option<Vec<_>>>()
                .ok_or("the file of the screen of the caught does not hold the spells' texts")?,
            dim_font: picture_in_row(&font_set, font.image, 1)?,
            font_picture: picture(&font_set, font.image)?,
            small_picture: picture(&font_set, small.image)?,
            marked_picture: picture_in_row(&font_set, small.image, MARKED_ROW)?,
            sprites,
            frames,
            font,
            small,
            main_lines: lines(merge_panel::MAIN_LINES)?,
            carried_lines: lines(merge_panel::CARRIED_LINES)?,
            kept_lines: lines(merge_panel::KEPT_LINES)?,
            keeper: merge_panel::keeper_texts(&file, MERGE_FILE_BASE, argument)
                .ok_or("the file of the screen of the caught does not hold the keeper's lines")?,
            window,
            fills: window::fill_colours(&exe.image, exe.base)
                .ok_or("the executable does not hold the colours of a window")?,
            legends: vec![
                legend(merge_panel::LEGEND_OF_A_PICK)?,
                legend(merge_panel::LEGEND_OF_THE_SPIN)?,
                legend(merge_panel::LEGEND_OF_A_SPIN)?,
                legend(merge_panel::LEGEND_OF_AN_EXCHANGE)?,
                legend(merge_panel::LEGEND_PLAIN)?,
                legend(merge_panel::LEGEND_OF_THE_VIEW)?,
                legend(merge_panel::LEGEND_OF_A_PAGE)?,
                legend(merge_panel::LEGEND_OF_ROWS)?,
                legend(merge_panel::LEGEND_OF_A_SECOND)?,
                legend(merge_panel::LEGEND_OF_THE_NEW)?,
                legend(merge_panel::LEGEND_OF_MORE)?,
                legend(merge_panel::LEGEND_OF_THE_FORM)?,
            ],
            signs: scroll_signs(&exe, &font_set)?,
            more: picture(&font_set, window::MORE_IMAGE)?,
            bars: bars(&font_set)?,
            fight: FightTables::read(&exe.image, exe.base)
                .ok_or("the executable does not hold the tables of the fights")?,
        };
        let faces = Faces::from_executable(&exe.image, exe.base)
            .ok_or("the executable does not hold the places of the faces")?;
        Ok((look, faces))
    }

    fn parts(&self) -> Parts<'_> {
        Parts {
            sprites: &self.sprites,
            frames: &self.frames,
            font: &self.font,
            font_picture: &self.font_picture,
        }
    }
}

/// The creatures of the list in a line of the log: place, state, species and level.
fn listed(list: &Listed) -> Vec<(usize, i16, u32, u8)> {
    (list.in_use())
        .map(|(place, c)| (place, c.state, c.record.species, c.record.level))
        .collect()
}

/// Nobody is at the pad: the screen's start and its leaving are the game's, and
/// between the two the kept creatures go with the player in the order of the list
/// while fewer than three do. A list too long for the records loses its last places,
/// where the game's keeper would not let the screen leave. Answers what a line of the
/// log says of it.
pub fn nobody_chooses(carried: &mut Carried) -> String {
    let waited = carried.caught.waiting().count();
    let mut screen = MergeScreen::open(0, carried);
    let kept: Vec<usize> = (screen.listed.in_use())
        .filter(|(_, creature)| creature.state == creatures::state::KEPT)
        .map(|(place, _)| place)
        .collect();
    let taken = (kept.into_iter())
        .filter(|place| screen.listed.take_along(*place))
        .count();
    let mut list = screen.listed;
    let mut let_go = 0;
    while let Err(back) = list.close(&mut carried.creatures) {
        list = *back;
        let last = list.in_use().last().map(|(place, _)| place);
        list.free(last.expect("a list that cannot leave has places in use"));
        let_go += 1;
    }
    format!(
        "{waited} caught creatures are his own now, {taken} more go with him (nobody is at \
         the pad to choose), {let_go} are let go for want of room; he carries {} and keeps {}",
        carried.creatures.count(creatures::state::CARRIED),
        carried.creatures.count(creatures::state::KEPT)
    )
}

impl MergeView {
    fn open(
        argument: i16,
        mut carried: Carried,
        face: impl FnOnce(&Faces) -> Option<Vec<u8>>,
        presses: Vec<Press>,
        shown: Entity,
    ) -> Result<Self, Failure> {
        let (look, faces) = Look::read(argument)?;
        let side = window::FACE as usize;
        let face = face(&faces).map(|rgba| Picture {
            rgba,
            wide: side,
            high: side,
        });
        let waited = carried.caught.waiting().count();
        let empties = carried.keeping.cocoons();
        let screen = MergeScreen::open(argument, &mut carried);
        info!(
            "the screen of the caught: argument {argument}; {waited} caught creatures are his \
             own now, everybody is made whole, the empty cocoons {empties} -> {}; the list \
             (place, state, species, level) {:?}",
            carried.keeping.cocoons(),
            listed(&screen.listed)
        );
        Ok(Self {
            screen,
            carried,
            pad: None,
            passes: 0,
            owed: 0.0,
            time: 0.0,
            presses,
            look,
            face,
            shown,
            naming: None,
            asleep: false,
            comes_back: false,
            aim: Aim::default(),
        })
    }

    /// Whether the screen's task sleeps while the room plays its part.
    pub fn asleep(&self) -> bool {
        self.asleep
    }

    /// Whether the name entry is up on a creature's name, and the entry itself, which
    /// takes the keyboard (`name_entry.rs`).
    pub fn names(&self) -> bool {
        self.naming.is_some() && !self.asleep
    }

    pub fn naming_mut(&mut self) -> Option<&mut NameEntry> {
        self.naming.as_mut().filter(|_| !self.asleep)
    }

    /// The script of the room asks for the sleeping screen again (`0x59` of kind 3,
    /// which file 43's watch takes as the step asked for). Behind a spin that is step
    /// 2 (`0x800BE258`): state 5 is up with the spun places gone. Behind a merge it is
    /// step 1: the name entry on the new creature's name. Either way the room stands,
    /// and a button that is down from the room did not go down on the screen.
    pub fn come_back(&mut self) {
        let names = self.screen.back();
        (self.asleep, self.comes_back) = (false, true);
        self.pad = None;
        info!(
            "the screen of the caught: the script asks for it again: it is back in state \
             {}, the list (place, state, species, level) {:?}",
            self.screen.state,
            listed(&self.screen.listed)
        );
        if let Some(place) = names {
            self.open_name_entry(place);
        }
    }

    /// The name entry comes up on the name of the creature of a place (file 43's step
    /// 1, `0x800B701C(name, 16)`), over the wall of the menus.
    fn open_name_entry(&mut self, place: usize) {
        let stored = (self.screen.creature(place)).map_or([0; name::LETTERS], |c| c.record.name);
        let mut wall = [0, 0, 0, 255].repeat(SCREEN.0 * SCREEN.1);
        self.look.parts().wall(&mut wall);
        match name_entry::open_on(&stored, wall, GAME_FRAME) {
            Ok(entry) => {
                info!(
                    "the screen of the caught: pass {}: the name entry opens on the name of \
                     the creature of place {place}, kept as {stored:?}",
                    self.passes
                );
                self.naming = Some(entry);
            }
            Err(err) => {
                error!("the screen of the caught: the name entry: {err}");
                self.screen.named(stored);
            }
        }
    }

    /// The name of the creature of a place: its record's.
    fn name(&self, place: usize) -> Option<Vec<u16>> {
        let record = &self.screen.creature(place)?.record;
        Some(name::units(&record.name, &self.look.small))
    }

    /// The five move entries of the creature of a place as the list of its body
    /// attacks shows them: outside a fight no row is refused for its cost.
    fn attack_rows(&self, place: usize) -> [AttackRow; attacks::ROWS] {
        let Some(creature) = self.screen.creature(place) else {
            return Default::default();
        };
        std::array::from_fn(|row| {
            let entry = creature.record.moves[row];
            let effect = (self.look.fight.body_attacks).get(usize::from(entry.attack));
            AttackRow {
                known: entry.known,
                attack: entry.attack,
                grade: entry.grade,
                // An attack past the table has no element.
                class: effect.map_or(u8::MAX, |effect| effect.class),
                refused: false,
            }
        })
    }

    /// A name in the small font and in capitals, as a box of the "Family" has it.
    fn small_name(&self, screen: &mut [u8], name: &[u16], at: (i16, i16), clip: menu::Rect) {
        let name: Vec<u16> = (name.iter().copied())
            .map(player_panel::in_capitals)
            .collect();
        let look = &self.look;
        options_view::glyphs(screen, &look.small, &look.small_picture, &name, at, clip);
    }

    /// One thing of a panel; `colour` is the window colour of the settings.
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
            Drawn::WindowFrame { rect } => {
                options_view::window_frame(screen, &self.look.window, rect);
            }
            Drawn::WindowFill { rect } => {
                let fill = usize::try_from(colour)
                    .ok()
                    .and_then(|colour| self.look.fills.get(colour));
                if let Some(fill) = fill {
                    options_view::window_fill(screen, &self.look.window, *fill, rect);
                }
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
            // The name of a place's creature: in a list in the small font and in
            // capitals, in its box in the font of the texts.
            Drawn::PlaceName {
                place,
                middle,
                marked,
            } => {
                let Some(name) = self.name(place) else {
                    return;
                };
                let name: Vec<u16> = (name.iter().copied())
                    .map(player_panel::in_capitals)
                    .collect();
                let small = &self.look.small;
                let width = small.length(&small.codes(&name));
                let picture = match marked {
                    true => &self.look.marked_picture,
                    false => &self.look.small_picture,
                };
                let at = (middle.0.wrapping_sub(width / 2), middle.1);
                options_view::glyphs(screen, small, picture, &name, at, WHOLE);
            }
            Drawn::FullName { creature, at, clip } => {
                if let Some(name) = self.name(creature) {
                    parts.text(screen, &name, at, clip);
                }
            }
            Drawn::More { at } => {
                let bright = window::more_brightness(self.passes);
                blit_lit(screen, &self.look.more, at, [bright; 3], WHOLE);
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
            Drawn::Listed {
                what: player_panel::Listed::Sprite(number),
                room,
                flags,
                dim,
                clip,
            } => {
                if let Some(sprite) = self.look.sprites.get(number) {
                    let at = menu::placed(flags, room, sprite.wide as i16);
                    let lit = [menu::brightness(if dim { menu::piece::DIM } else { 0 }); 3];
                    blit_lit(screen, sprite, at, lit, clip);
                }
            }
            Drawn::Listed {
                what,
                room,
                flags,
                dim,
                clip,
            } => {
                let look = &self.look;
                let name = match what {
                    player_panel::Listed::Spell(spell) => look.spell_names.get(spell),
                    player_panel::Listed::Attack(attack) => look.attack_names.get(attack),
                    player_panel::Listed::Grade(grade) => look.grade_names.get(grade),
                    player_panel::Listed::Sprite(_) => None,
                };
                if let Some(name) = name {
                    let width = look.font.length(&look.font.codes(name));
                    let picture = match dim {
                        true => &look.dim_font,
                        false => &look.font_picture,
                    };
                    let at = menu::placed(flags, room, width);
                    options_view::glyphs(screen, &look.font, picture, name, at, clip);
                }
            }
            Drawn::ListDescribed { what, line, at } => {
                let lines = match what {
                    player_panel::Listed::Spell(spell) => self.look.spell_texts.get(spell),
                    player_panel::Listed::Attack(attack) => self.look.attack_texts.get(attack),
                    _ => None,
                };
                if let Some(said) = lines.and_then(|lines| lines.get(line)) {
                    parts.text(screen, said, at, WHOLE);
                }
            }
            // The list of the minion skins: a skin's small picture, its name and how
            // many of it he has.
            Drawn::ItemPicture { item, at, clip, .. } => {
                if let Some((Some(picture), _)) = self.look.skins.get(usize::from(item)) {
                    blit_lit(screen, picture, at, [menu::brightness(0); 3], clip);
                }
            }
            Drawn::ItemName { item, at, clip, .. } => {
                if let Some((_, name)) = self.look.skins.get(usize::from(item)) {
                    parts.text(screen, name, at, clip);
                }
            }
            Drawn::Count {
                value, ends, clip, ..
            } => {
                let width = |number: usize| parts.width(number);
                for (number, at) in player_panel::digits(value, ends, false, width) {
                    parts.sprite(screen, number, at, clip);
                }
            }
            Drawn::SignOf {
                turns, at, time, ..
            } => {
                if let Some(image) = usize::try_from(colour)
                    .ok()
                    .and_then(|colour| self.look.between.get(colour))
                {
                    options_view::put_sign(screen, image, turns, at, time);
                }
            }
            // The new creature of a merge compared with the first of the two: a
            // number under the colour of its change, and the arrow of a difference.
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
            // The boxes of the "Family": the creature's own name and the six names its
            // record keeps.
            Drawn::CreatureName { creature, at, clip } => {
                if let Some(name) = self.name(creature) {
                    self.small_name(screen, &name, at, clip);
                }
            }
            Drawn::AncestorName {
                creature,
                ancestor,
                at,
            } => {
                let line = self
                    .screen
                    .creature(creature)
                    .map(|creature| &creature.line);
                if let Some(name) = line.and_then(|line| line.names.get(ancestor)) {
                    let name = name::units(name, &self.look.small);
                    self.small_name(screen, &name, at, WHOLE);
                }
            }
            // The screen draws no other kind of piece yet.
            _ => {}
        }
    }

    /// The screen of a pass, four bytes a pixel: the wall, which the task's step draws
    /// first (`0x8002BE6C`), the line of the buttons of the state, and over it the
    /// panels. `colour` is the window colour of the settings.
    fn paint(&self, colour: i16) -> Vec<u8> {
        let mut screen = [0, 0, 0, 255].repeat(SCREEN.0 * SCREEN.1);
        let parts = self.look.parts();
        parts.wall(&mut screen);
        let legend = merge_panel::legend(&self.screen);
        let pairs = legend.and_then(|list| self.look.legends.iter().find(|(of, _)| *of == list));
        if let Some((_, pairs)) = pairs {
            parts.legend(&mut screen, colour, pairs);
        }
        // The five values of the creature in the box, as a fight counts them.
        let place = self.screen.shown_place();
        let shown = self.screen.creature(place);
        let counted = |creature: Option<&creatures::Creature>| {
            creature.map_or([0; 5], |creature| {
                let counted = creature.record.effective(&self.look.fight).stats;
                std::array::from_fn(|n| counted[n])
            })
        };
        let values = counted(shown);
        // Where it comes from, as its record keeps it.
        let family = shown.map(|creature| {
            Family::of_a_line(&creature.line, |name| name::units(name, &self.look.small))
        });
        let shown = merge_panel::Shown {
            main_lines: &self.look.main_lines,
            carried_lines: &self.look.carried_lines,
            kept_lines: &self.look.kept_lines,
            skin_lines: &self.look.skin_lines,
            values,
            compared_values: counted(self.screen.compared()),
            attack_lines: &self.look.attack_lines,
            spell_lines: &self.look.spell_lines,
            attack_pictures: &self.look.attack_pictures,
            spell_pictures: &self.look.spell_pictures,
            attack_rows: self.attack_rows(place),
            family: family.as_ref(),
            circle: (self.pad).is_some_and(|pad| pad.held & jc_formats::pad::CIRCLE != 0),
        };
        let time = menu::frame_time(self.passes, GAME_FRAME);
        let width = |number: usize| parts.width(number);
        for drawn in merge_panel::drawn(&self.screen, &shown, time, width) {
            self.put(&mut screen, colour, drawn);
        }
        // The white behind a yes with "Magic FX" off: a rectangle that is added to
        // every colour. HYPOTHESIS: it lies over the screen's panels too.
        let white = self.screen.white();
        if white > 0 {
            for (_, part) in (screen.iter_mut().enumerate()).filter(|(byte, _)| byte % 4 != 3) {
                *part = part.saturating_add(white);
            }
        }
        screen
    }

    /// What a line of the log says of where the screen stands.
    fn place(&self) -> (i16, i16, i16, i16, i16, u32, (u32, i16, i16)) {
        let screen = &self.screen;
        (
            screen.state,
            screen.list,
            screen.cursor.at,
            screen.cursor.first,
            screen.refusal,
            screen.marks,
            (
                screen.page(),
                screen.page_cursor.at,
                screen.page_cursor.first,
            ),
        )
    }
}

pub fn over_the_room(
    argument: i16,
    carried: Carried,
    face: impl FnOnce(&Faces) -> Option<Vec<u8>>,
    presses: &MergePresses,
    images: &mut Assets<Image>,
    commands: &mut Commands,
) -> Result<(), Failure> {
    let (wide, high) = (SCREEN.0 as u32, SCREEN.1 as u32);
    let black = [0, 0, 0, 255].repeat(SCREEN.0 * SCREEN.1);
    let shown = crate::spawn_picture(commands, images, wide, high, black);
    let view = match MergeView::open(argument, carried, face, presses.0.clone(), shown) {
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
        // The creature of "View form" is drawn in the console's own colour values, as
        // the room's camera composes them (`thing.wgsl`).
        bevy::camera::CompositingSpace::Srgb,
        // Two cameras of one window must agree on this, and the room's has none.
        Msaa::Off,
        layer,
    ));
    commands.insert_resource(view);
    Ok(())
}

/// Once a frame in front of the passes, while the screen is awake: the mouse over the
/// spots of its state, or over the name entry's.
pub fn point_merge(current: Option<ResMut<MergeView>>, pointer: Pointer) {
    let Some(mut current) = current.filter(|current| !current.asleep) else {
        return;
    };
    let under = pointer.on_picture(current.shown, SCREEN);
    let view = &mut *current;
    match view.naming.as_mut() {
        Some(entry) => {
            view.aim.forget();
            entry.look(pointer.mouse(), under);
        }
        None => {
            let look = &view.look;
            let lines = [
                &look.main_lines[..],
                &look.carried_lines[..],
                &look.kept_lines[..],
                &look.skin_lines[..],
            ];
            let spots = merge_panel::spots(&view.screen, lines);
            (view.aim).look(pointer.mouse(), pointer::row_under(spots, under));
        }
    }
}

/// Runs the passes of the screen for the time gone by.
#[allow(clippy::too_many_arguments)]
pub fn run_merge(
    current: Option<ResMut<MergeView>>,
    mut music: ResMut<Music>,
    settings: Res<crate::settings::Settings>,
    time: Res<Time<Virtual>>,
    keys: Res<crate::keys::Pad>,
    mut images: ResMut<Assets<Image>>,
    mut sprites: Query<(&mut Sprite, &Transform)>,
    mut over_room: Query<(Entity, Option<&mut Camera>), With<OverTheRoom>>,
    mut things: Option<ResMut<crate::shown::Things>>,
    mut commands: Commands,
) {
    let Some(mut current) = current else {
        return;
    };
    // Asleep the screen is not passed and not seen; back, it is over the room again.
    if current.asleep {
        return;
    }
    if std::mem::take(&mut current.comes_back) {
        for (_, camera) in &mut over_room {
            if let Some(mut camera) = camera {
                camera.is_active = true;
            }
        }
    }
    current.screen.magic_fx = settings.0.word & (1 << options_screen::MAGIC_FX) != 0;
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
        current.pad = Some(pad);
        // The name entry has the passes while the screen's task sleeps; with the name
        // taken the task is woken and goes on in the view (`0x800BE304`).
        let view = &mut *current;
        if let Some(entry) = view.naming.as_mut() {
            if let Some(name) = entry.pass(&pad, &mut music) {
                view.naming = None;
                view.screen.named(name);
                info!(
                    "the screen of the caught: pass {passes}: the creature has its name, \
                     kept as {name:?}; the screen is back in state {}",
                    view.screen.state
                );
            }
            continue;
        }
        let before = current.place();
        let white = current.screen.flags & flag::WHITE != 0;
        // The list of the minion skins is his things of kind 4 as they are now.
        current.screen.things = current.carried.keeping.clone();
        // The mouse steers the cursor with the pad's own presses.
        let view = &mut *current;
        let here = merge_panel::here(&view.screen);
        (view.aim).steer(&mut pad, here, |spot| merge_panel::way(&view.screen, spot));
        let pass = current.screen.pass(&pad);
        if let Some(merging) = pass.merges {
            // STAND-IN: the seed of the six draws of a new species is the pass, where
            // the game has its generator's word; no line of the slice makes one.
            let view = &mut *current;
            let mut draws = Random { seed: passes };
            view.screen.merge(merging, &view.look.fight, &mut draws);
            let made = view.screen.made.as_ref().map(|made| {
                let record = &made.record;
                let (level, hp, mp) = (record.level, record.max_hp, record.max_mp);
                (
                    record.species,
                    made.line.look,
                    level,
                    hp,
                    mp,
                    record.strengths,
                )
            });
            info!(
                "the screen of the caught: pass {passes}: the places {} and {} are merged, \
                 with the skin {:?}: the new creature lies aside (species, look, level, HP, \
                 mana, strengths) {made:?}",
                merging.first, merging.second, merging.skin
            );
        }
        if white != (current.screen.flags & flag::WHITE != 0) {
            info!(
                "the screen of the caught: pass {passes}: the white over the screen {} \
                 (the row \"Magic FX\" of the options is off)",
                if white { "has gone" } else { "begins to grow" }
            );
        }
        for &sound in &pass.sounds {
            music.sound(sound.into());
        }
        if !pass.spun.is_empty() {
            let view = &mut *current;
            let worn = view.carried.worn;
            let given: Vec<(u8, bool)> = (pass.spun.iter())
                .map(|&item| {
                    let keeping = &mut view.carried.keeping;
                    (item, keeping.give(item.into(), 1, items::Kind::Item, worn))
                })
                .collect();
            info!(
                "the screen of the caught: pass {passes}: spun into silk (item, whether it \
                 fitted) {given:?}: the list (place, state, species, level) {:?}",
                listed(&view.screen.listed)
            );
        }
        if let Some(merged) = pass.merged {
            // The skin is taken from the player and his skins are put in order
            // (`0x800AD3B0(thing, 1, 4)`, `0x800AD5C0(4)`); a new species counts as a
            // caught kind from now on.
            let view = &mut *current;
            let taken = merged.skin.map(|skin| {
                let keeping = &mut view.carried.keeping;
                let taken = keeping.take(skin.into(), 1, items::Kind::Creature);
                keeping.tidy(items::Kind::Creature);
                (skin, taken)
            });
            if let Some(kind) = merged.new_species {
                view.carried.kinds.mark(kind.into());
            }
            info!(
                "the screen of the caught: pass {passes}: the yes of the merge: the new \
                 creature is in place {}, place {} is free, the skin (thing, whether he had \
                 it) {taken:?}, a new kind {:?}: the list (place, state, species, level) {:?}",
                merged.first,
                merged.second,
                merged.new_species,
                listed(&view.screen.listed)
            );
        }
        if let Some(place) = pass.names {
            current.open_name_entry(place);
        }
        if let Some(place) = pass.looks {
            // The model job: the creature's model is read and its actor made at the
            // room's origin, hidden until the screen's wait is over. The scale of its
            // stage is the one R1 and R2 change.
            let of = (current.screen.creature(place)).map(crate::actors::Figure::of);
            let factor = (sprites.get(current.shown)).map_or(1.0, |(_, placed)| placed.scale.x);
            let looked = match (of, things.as_mut()) {
                (Some(of), Some(things)) => {
                    (things.look_at(&of, factor)).map_err(|err| err.to_string())
                }
                (None, _) => Err("the place has no creature".to_string()),
                (_, None) => Err("there is no room behind the screen".to_string()),
            };
            let of = of.map(|of| (of.species, of.level));
            match looked {
                Ok((scale, line)) => {
                    current.screen.form_stands(scale);
                    info!(
                        "the screen of the caught: pass {passes}: \"View form\" of place \
                         {place} (species, level) {of:?}: the model job is asked for; the \
                         scale of its stage {scale:?}; {line}"
                    );
                }
                Err(err) => error!(
                    "the screen of the caught: pass {passes}: \"View form\" of place {place} \
                     (species, level) {of:?}: no creature is shown: {err}"
                ),
            }
        }
        if pass.looked {
            if let Some(things) = things.as_mut() {
                things.end_look();
            }
            info!(
                "the screen of the caught: pass {passes}: the look at the form is over, the \
                 screen is back in state {}",
                current.screen.state
            );
        }
        if let Some(form) = current.screen.form.filter(|form| form.made) {
            // The actor's game frame, with the turn and the scale of this pass.
            if let (Some(things), Some(scale)) = (things.as_mut(), form.scale) {
                things.pass_form(form.turn, scale);
            }
            let pad = current.pad.unwrap_or_default();
            if form.turn != 0 && pad.held != 0 && current.passes.is_multiple_of(8) {
                info!(
                    "the screen of the caught: pass {passes}: the form is turned to {} of \
                     4096, its scale {:?}",
                    form.turn, form.scale
                );
            }
        }
        if let Some((first, second)) = pass.exchanged {
            info!(
                "the screen of the caught: pass {passes}: the places {first} and {second} \
                 change with each other: the list (place, state, species, level) {:?}",
                listed(&current.screen.listed)
            );
        }
        if pass.rearranged {
            info!(
                "the screen of the caught: pass {passes}: the gaps of the kept are closed: \
                 the list (place, state, species, level) {:?}",
                listed(&current.screen.listed)
            );
        }
        let (state, list, at, first, refusal, marks, (page, row, line)) = current.place();
        if current.place() != before || !pass.sounds.is_empty() {
            info!(
                "the screen of the caught: pass {passes}: state {state}, the cursor on {at} \
                 from line {first} of list {list}, the place shown {}, refusal {refusal}, \
                 marks {marks:#x}, panels {:#x}, page {page:#x} with its cursor on {row} from \
                 line {line}, sounds {:?}",
                current.screen.shown_place(),
                current.screen.panels,
                pass.sounds
            );
        }
        if let Some(answer) = pass.answers {
            // The task sleeps and the field has its mode 1 again (`0x800BDF50`): the
            // room is seen and runs, and its script goes on with the answer.
            info!(
                "the screen of the caught: pass {passes}: the screen answers the script \
                 {answer} and sleeps, the room runs on"
            );
            current.asleep = true;
            for (_, camera) in &mut over_room {
                if let Some(mut camera) = camera {
                    camera.is_active = false;
                }
            }
            if let Some(things) = things.as_mut() {
                things.screen_answered(answer);
            }
            return;
        }
        if !pass.ended {
            continue;
        }
        let mut carried = current.carried.clone();
        if let Err(list) = current.screen.clone().close(&mut carried) {
            // The keeper lets nobody leave with more than the records hold.
            error!(
                "the screen of the caught: pass {passes}: the list does not fit the game's \
                 records: {:?}",
                listed(&list)
            );
        }
        info!(
            "the screen of the caught: pass {passes}: the screen leaves: he carries {} \
             creatures and keeps {}, the room runs on",
            carried.creatures.count(creatures::state::CARRIED),
            carried.creatures.count(creatures::state::KEPT)
        );
        if let Some(things) = things.as_mut() {
            things.listed(&carried);
            things.screen_left();
        }
        for (entity, _) in &over_room {
            commands.entity(entity).despawn();
        }
        commands.remove_resource::<MergeView>();
        return;
    }
    if !painted {
        return;
    }
    if let Ok((mut sprite, _)) = sprites.get_mut(current.shown) {
        let (wide, high) = (SCREEN.0 as u32, SCREEN.1 as u32);
        let colour = settings.0.colour as i16;
        let picture = match current.naming.as_mut() {
            Some(entry) => {
                entry.colour = colour;
                entry.paint()
            }
            None => current.paint(colour),
        };
        sprite.image = images.add(crate::picture_image(wide, high, picture));
    }
}
