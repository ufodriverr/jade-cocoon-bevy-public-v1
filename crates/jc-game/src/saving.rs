
use bevy::prelude::*;
use jc_disc::{Archive, DiscImage, Executable, LocalConfig};
use jc_formats::battle::record::Record;
use jc_formats::carried::{self, Beside, Carried, REFRESHES_A_SECOND, Stands};
use jc_formats::field::Position;
use jc_formats::model::PartTables;
use jc_formats::rooms::{self, RoomTable};
use jc_formats::{name, save};

use crate::name_entry::Named;
use crate::settings::Settings;
use crate::shown;

type Failure = Box<dyn std::error::Error>;

const FILE: usize = 8192;

#[derive(Resource, Default)]
pub struct PlayClock {
    pub refreshes: u32,
    owed: f64,
}

impl PlayClock {
    /// The reset of "Start Game".
    pub fn start(&mut self) {
        *self = Self::default();
    }

    /// A loaded game goes on with the clock of its save.
    pub fn set(&mut self, refreshes: u32) {
        *self = Self {
            refreshes,
            owed: 0.0,
        };
    }
}

#[derive(Resource)]
pub struct Ended;

#[derive(Resource)]
pub struct Loaded {
    pub carried: Carried,
    pub beside: Beside,
    pub stands: Stands,
    pub room: Option<u16>,
    pub name: Vec<u16>,
    /// He stands on the room's default place, not on the one of `stands`: the start
    /// of a second round.
    pub default_place: bool,
}

#[derive(Resource)]
pub struct SecondRound {
    pub carried: Carried,
    pub beside: Beside,
}

impl SecondRound {
    /// The game the field starts with: the state setup run over the loaded one, with
    /// the name the name entry left, in the first room of a second round.
    pub fn starts(&self, name: Vec<u16>, stored: [u8; name::LETTERS]) -> Loaded {
        let (mut carried, mut beside) = (self.carried.clone(), self.beside);
        carried.second_round();
        beside.second_round();
        beside.name = stored;
        Loaded {
            carried,
            beside,
            // Area 0, place 4, record 0, which the room's number stands for here.
            stands: Stands {
                area: 0,
                place: 4,
                room: 0,
                view: 0,
                position: [0; 3],
                angle: 0,
            },
            room: Some(jc_formats::title::SECOND_ROUND_ROOM),
            name,
            default_place: true,
        }
    }
}

/// Counts the refreshes of the time gone by.
pub fn count(time: Res<Time>, mut clock: ResMut<PlayClock>) {
    clock.owed += time.delta_secs_f64() * f64::from(REFRESHES_A_SECOND);
    let whole = clock.owed.floor();
    clock.owed -= whole;
    clock.refreshes = clock.refreshes.wrapping_add(whole as u32);
}

/// What a save needs of the disc: the template of the player's record, which stands for
/// a record no fight has changed, the value of every weapon for the player's model,
/// the table of the rooms and the two texts of the place that the card's file has.
struct Disc {
    template: Record,
    looks: Vec<usize>,
    rooms: RoomTable,
    fixed: [[u8; 24]; 2],
    /// The card's own head of a file: its title, its icon, the pocket console's picture.
    file_head: save::FileHead,
    /// The small font, whose table turns the stored name into a text.
    small: jc_formats::text::Font,
}

impl Disc {
    fn read() -> Result<Self, Failure> {
        let config = LocalConfig::load()?;
        let mut disc = DiscImage::open(&config.disc_image)?;
        let archive = Archive::open(&mut disc)?;
        let exe = Executable::read(&mut disc)?;
        let title = archive.read(&mut disc, name::DEFAULT_NAME.0)?;
        let engine = archive.read(&mut disc, rooms::FIELD_ENGINE_FILE)?;
        let parts = PartTables::read(&engine, &exe.image, exe.base)
            .map_err(|err| format!("the tables of the player's parts: {err:?}"))?;
        let card = archive.read(&mut disc, save::OVERLAY)?;
        let fonts = jc_formats::text::Fonts::from_executable(&exe.image, exe.base)
            .ok_or("the executable does not hold the font tables")?;
        let small = fonts.get(jc_formats::text::SMALL_FONT).cloned();
        let set =
            jc_formats::tim::texture_set(&archive.read(&mut disc, jc_formats::text::FONT_SET)?)
                .ok_or("the font file is not a texture set")?;
        Ok(Self {
            small: small.ok_or("there is no small font")?,
            template: Record::of_the_player(&title).ok_or("the title's file has no player")?,
            looks: parts.looks.to_vec(),
            rooms: RoomTable::from_field_engine(&engine)?,
            fixed: save::fixed_places(&card).ok_or("file 48 has no texts of the place")?,
            file_head: save::FileHead::read(&card, &set)
                .ok_or("file 48 has no head of a file on the card")?,
        })
    }

    /// Where the hero stands in room `number` with view `view` on screen.
    fn stands(&self, number: u16, view: usize, at: (Position, i16)) -> Option<Stands> {
        let (location, _) = self.rooms.by_number(number)?;
        Some(Stands {
            area: i16::try_from(location.area).ok()?,
            place: i16::try_from(location.place).ok()?,
            room: i16::try_from(location.room).ok()?,
            view: i16::try_from(view).ok()?,
            position: at.0,
            angle: at.1,
        })
    }
}

fn memory(
    carried: &Carried,
    beside: &Beside,
    stands: Option<&Stands>,
    disc: &Disc,
) -> Result<save::Memory, Failure> {
    let mut memory = save::Memory::zeroed();
    if !carried.write_memory(&mut memory, &disc.template) {
        return Err("the variant table has no room in a save".into());
    }
    beside.write_block(&mut memory.block);
    if let Some(stands) = stands {
        stands.write_block(&mut memory.block);
    }
    if !carried::write_model(&mut memory.block, carried.worn, &disc.looks) {
        return Err("the weapon he has on has no record".into());
    }
    Ok(memory)
}

/// The buffer of a save of a new game that was given no name, with a place without
/// letters: what the check of the memory card screen saves (`--card`).
pub(crate) fn new_game(settings: jc_formats::options_screen::Settings) -> Result<Vec<u8>, Failure> {
    let disc = Disc::read()?;
    let beside = Beside::new_game(settings, [0; name::LETTERS]);
    let mut buffer = vec![0; jc_formats::card_screen::BUFFER];
    let made = memory(&Carried::new_game(), &beside, None, &disc)?;
    if !made.write(&mut buffer, &[0; 24], &disc.fixed) || !disc.file_head.write(&mut buffer) {
        return Err("the buffer is too short for a save".into());
    }
    Ok(buffer)
}

pub struct Running {
    carried: Carried,
    beside: Beside,
    stands: Option<Stands>,
    place: [u8; 24],
    disc: Disc,
    /// The head of the save the reader took last.
    pub took: Option<save::Head>,
    /// The game the reader took last, until the field takes it.
    pub loaded: Option<Loaded>,
}

impl Running {
    /// The game of this moment; `room` is the room on screen with its view and the
    /// player's place and angle in it.
    pub fn of(
        carried: Carried,
        (settings, named, clock): (&Settings, Option<&Named>, &PlayClock),
        room: Option<Room>,
        place: [u8; 24],
    ) -> Result<Self, Failure> {
        let disc = Disc::read()?;
        let stands = match room {
            Some((number, view, at)) => Some(
                disc.stands(number, view, at)
                    .ok_or("the room on screen has no record")?,
            ),
            None => None,
        };
        Ok(Self {
            beside: beside(&carried, settings, named, clock),
            carried,
            stands,
            place,
            disc,
            took: None,
            loaded: None,
        })
    }

    /// The story is over: what the field's hand-over does to the game in front of the
    /// save behind the end (`Beside::finish`).
    pub fn finish(&mut self) {
        self.beside.finish();
    }

    /// The play clock counts on while the screen is up.
    pub fn at(&mut self, clock: &PlayClock) {
        self.beside.clock = clock.refreshes;
    }

    fn memory(&self) -> Result<save::Memory, Failure> {
        memory(
            &self.carried,
            &self.beside,
            self.stands.as_ref(),
            &self.disc,
        )
    }
}

impl jc_formats::card_screen::Game for Running {
    fn write(&mut self) -> Vec<u8> {
        let mut buffer = vec![0; jc_formats::card_screen::BUFFER];
        match self.memory() {
            Ok(memory) => {
                memory.write(&mut buffer, &self.place, &self.disc.fixed);
                self.disc.file_head.write(&mut buffer);
            }
            Err(err) => error!("the memory card screen: the game is not saved: {err}"),
        }
        buffer
    }

    fn read(&mut self, buffer: &[u8]) -> bool {
        let Ok(mut memory) = self.memory() else {
            return false;
        };
        if !memory.read(buffer, false) {
            return false;
        }
        let mut carried = self.carried.clone();
        if !carried.read_memory(&memory, &self.disc.template) {
            error!("the memory card screen: the save has no variant table, it is not taken");
            return false;
        }
        let beside = Beside::read_block(&memory.block);
        // The word of the place is the loaded game's from here on.
        carried.place_word = beside.place;
        if beside.finished {
            info!("the memory card screen: this is the save of a finished game");
        }
        let stands = Stands::read_block(&memory.block);
        let records = (usize::try_from(stands.area).ok())
            .zip(usize::try_from(stands.place).ok())
            .map(|(area, place)| self.disc.rooms.rooms(area, place));
        let room = (records.zip(usize::try_from(stands.room).ok()))
            .and_then(|(records, room)| records.get(room)?.number);
        self.took = save::Head::read(buffer);
        self.loaded = Some(Loaded {
            name: name::units(&beside.name, &self.disc.small),
            carried,
            beside,
            stands,
            room,
            default_place: false,
        });
        true
    }
}

fn beside(
    carried: &Carried,
    settings: &Settings,
    named: Option<&Named>,
    clock: &PlayClock,
) -> Beside {
    let name = named.map_or([0; name::LETTERS], |named| named.stored);
    Beside {
        clock: clock.refreshes,
        place: carried.place_word,
        ..Beside::new_game(settings.0, name)
    }
}

/// The room on screen with its view and the player's place and angle in it.
pub type Room = (u16, usize, (Position, i16));

/// The file of a save of this state, the head the list of saves would show for it, and
/// whether the file that is read back and written again is the same file.
fn said(carried: &Carried, beside: &Beside, room: Option<Room>) -> Result<String, Failure> {
    let disc = Disc::read()?;
    let stands = match room {
        Some((number, view, at)) => Some(
            disc.stands(number, view, at)
                .ok_or("the room on screen has no record")?,
        ),
        None => None,
    };
    // No object of the memory card hands a text of the place on: one without letters.
    let (place, fixed) = ([0; 24], disc.fixed);
    let mut file = vec![0; FILE];
    let made = memory(carried, beside, stands.as_ref(), &disc)?;
    if !made.write(&mut file, &place, &fixed) {
        return Err("the file is too short for a save".into());
    }
    let head = save::Head::read(&file).ok_or("the file has no head")?;

    let mut read = save::Memory::zeroed();
    if !read.read(&file, false) {
        return Err("the reader refuses the save".into());
    }
    let mut loaded = Carried::new_game();
    if !loaded.read_memory(&read, &disc.template) {
        return Err("the block of the save has no variant table".into());
    }
    let again = Beside::read_block(&read.block);
    let stood = stands.map(|_| Stands::read_block(&read.block));
    let mut second = vec![0; FILE];
    let same = memory(&loaded, &again, stood.as_ref(), &disc)?.write(&mut second, &place, &fixed)
        && second == file
        && again == *beside
        && stood == stands;
    let model = &made.block[2096..2102];
    Ok(format!(
        "the hero's name {:?}, level {}, HP {} of {}, {} h {} min on the clock of {} refreshes, \
         {} money, the place of kind {} with the number {}, he stands {stands:?}, model {} with \
         the weapon's value {}, the check sum \
         {:#X}; read back and written again it is the same file: {same}",
        &head.name[..name::LETTERS],
        head.level,
        head.hp,
        head.most_hp,
        head.hours,
        head.minutes,
        beside.clock,
        head.money,
        head.place_kind,
        head.place_word as i16,
        model[0],
        model[4],
        head.sum
    ))
}
/// `--save-check`: when a hidden run ends, the save the game would write of this moment
/// is made, and the log says what its head holds and whether it comes back the same.
pub fn check(
    mut done: Local<bool>,
    game: Option<Res<crate::Game>>,
    things: Option<Res<shown::Things>>,
    settings: Res<Settings>,
    named: Option<Res<Named>>,
    clock: Res<PlayClock>,
    room: Option<Res<crate::ShowRoom>>,
) {
    if std::mem::replace(&mut *done, true) {
        return;
    }
    let things = things.as_deref();
    let room = (room.as_deref().zip(things))
        .and_then(|(room, things)| Some((room.number, things.view_shown(), things.stands()?)));
    let carried = (things.and_then(|things| things.carried()))
        .or_else(|| game.map(|game| game.0.clone()))
        .unwrap_or_default();
    let beside = beside(&carried, &settings, named.as_deref(), &clock);
    match said(&carried, &beside, room) {
        Ok(said) => info!("a save of this moment: {said}"),
        Err(err) => error!("a save of this moment: {err}"),
    }
}
