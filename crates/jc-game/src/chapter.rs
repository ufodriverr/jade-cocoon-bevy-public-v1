
use bevy::prelude::*;
use jc_disc::{Archive, DiscImage, LocalConfig};
use jc_formats::chapters::{self, Chapter, ChapterTable};
use jc_formats::rooms::{self, RoomLocation, RoomRecord, RoomTable};

use crate::movie_view::{Failure, Playing, ScriptMovies, ShowMovie};
use crate::music::Music;

const PASSES_A_SECOND: f64 = 25.0;
/// The longest stretch of time one pass of the app may stand for.
const LONGEST_PASS: f64 = 0.1;

/// The field was entered in mode 0, or a script of a room has asked for it (`0x4F`): the
/// chapter screen of this chapter runs.
#[derive(Resource)]
pub struct StartChapter(pub usize);

/// The chapter screen while it runs.
#[derive(Resource)]
pub struct Screen {
    number: usize,
    chapter: Chapter,
    /// The records of the chapters up to this one.
    chapters: Vec<Chapter>,
    cleared: Vec<i32>,
    room: (RoomLocation, RoomRecord),
    /// The screen's counter once the movie is over, and the share of a pass that is owed.
    counter: Option<i32>,
    owed: f64,
}

/// Where the chapter screen sent the game: for whatever starts the room.
#[derive(Resource)]
pub struct Arrival {
    pub chapter: usize,
    /// The room, named as an exact record: the remap by slot is switched off.
    pub location: RoomLocation,
    pub record: RoomRecord,
}

/// Reads the chapter's record, the records of the chapters before it and its room.
fn open(number: usize) -> Result<Screen, Failure> {
    let config = LocalConfig::load()?;
    let mut disc = DiscImage::open(&config.disc_image)?;
    let archive = Archive::open(&mut disc)?;
    let engine = archive.read(&mut disc, rooms::FIELD_ENGINE_FILE)?;
    let file = archive.read(&mut disc, chapters::CHAPTER_FILE)?;
    let table = ChapterTable::from_files(&file, &engine)?;
    let chapter = table.get(number).ok_or("no such chapter")?.clone();
    let rooms = RoomTable::from_field_engine(&engine)?;
    let (location, record) = u16::try_from(chapter.start_room)
        .ok()
        .and_then(|room| rooms.by_number(room))
        .ok_or("the chapter starts in a room that is not in the room table")?;
    Ok(Screen {
        number,
        chapters: table.iter().take(number + 1).cloned().collect(),
        cleared: chapters::cleared_variables(&file)?,
        room: (location, *record),
        chapter,
        counter: None,
        owed: 0.0,
    })
}

/// The steps of the chapter screen: starts it when the field is entered in mode 0, plays
/// the chapter's movie, counts the wait behind it and leaves for the room.
#[allow(clippy::too_many_arguments)]
pub fn run_chapter(
    start: Option<Res<StartChapter>>,
    screen: Option<ResMut<Screen>>,
    playing: Option<Res<Playing>>,
    game: Option<Res<crate::Game>>,
    movies: Res<ScriptMovies>,
    mut music: ResMut<Music>,
    time: Res<Time<Virtual>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    if let Some(start) = start {
        commands.remove_resource::<StartChapter>();
        let opened = open(start.0).and_then(|screen| {
            let (asked, number) = (screen.chapter.music, screen.chapter.movie);
            info!(
                "the chapter screen of chapter {}: music {asked} is asked for, movie {number} plays",
                screen.number
            );
            // A music that cannot be read leaves the screen without it. The screen
            // notes its music as a room does.
            let asked = usize::try_from(asked)?;
            if let Err(err) = music.ask(asked) {
                error!("the chapter screen's music {asked}: {err}");
            }
            music.note(asked);
            let playing = Playing::start(&ShowMovie::chapter(usize::try_from(number)?, &movies))?;
            Ok((screen, playing))
        });
        match opened {
            Ok((screen, playing)) => {
                commands.insert_resource(screen);
                commands.insert_resource(playing);
            }
            Err(err) => {
                error!("the chapter screen: {err}");
                exit.write(AppExit::error());
            }
        }
        return;
    }
    let Some(mut screen) = screen else {
        return;
    };
    let Some(counter) = screen.counter else {
        if playing.is_some_and(|playing| playing.leads_to_the_room()) {
            commands.remove_resource::<Playing>();
            // A record that ends the music behind the movie fades the music record's
            // running sequence, as the title does, and sets the counter.
            let wait = match screen.chapter.ends_music {
                true => {
                    music.fade();
                    chapters::WAIT_BEHIND_MOVIE as i32
                }
                false => 0,
            };
            screen.counter = Some(wait);
        }
        return;
    };
    // The last state counts down once a pass and goes on when the counter is below zero.
    screen.owed += crate::pace::passed(&time, LONGEST_PASS) * PASSES_A_SECOND;
    let mut counter = counter;
    while screen.owed >= 1.0 && counter >= 0 {
        screen.owed -= 1.0;
        counter -= 1;
    }
    screen.counter = Some(counter);
    if counter >= 0 {
        return;
    }
    // The state the game carries on: a new game's when nothing has made one yet. The
    // lists of the chapters that are owed are applied, the others were before.
    let mut carried = game.map_or_else(Default::default, |game| game.0.clone());
    let owed = carried.owed;
    carried.start_chapter(screen.number, &screen.chapters, &screen.cleared);
    let (location, record) = screen.room;
    let arrival = Arrival {
        chapter: screen.number,
        location,
        record,
    };
    info!(
        "the chapter screen of chapter {}: story variables {:?} cleared, the lists of the chapters from {owed} on applied, {} variants in the table, state word {}",
        arrival.chapter,
        screen.cleared,
        carried.variants.entries().len(),
        carried.word
    );
    commands.insert_resource(crate::Game(carried));
    info!(
        "the chapter screen of chapter {}: the game goes to room {} (area {}, place {}, record {}: scene package {}, backgrounds {}), view 0, on the room's default position",
        arrival.chapter,
        screen.chapter.start_room,
        arrival.location.area,
        arrival.location.place,
        arrival.location.room,
        arrival.record.package_file,
        arrival.record.background_file
    );
    commands.insert_resource(arrival);
    commands.remove_resource::<Screen>();
}
