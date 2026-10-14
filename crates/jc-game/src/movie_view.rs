
use std::fs::File;

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use jc_disc::{Archive, DiscImage, Executable, LocalConfig};
use jc_formats::movie::{
    AudioSector, CodeTable, Demuxer, Frame, LOUDEST, MovieRecord, MovieTable, Picture,
    PictureDecoder, Screening, Sector, SoundDecoder,
};
use jc_formats::pad::{self, Pad, Repeat};
use jc_formats::title;

use crate::WINDOW_SIZE;
use crate::movie_sound::{Line, MovieSound};

/// What `--movie=N[:FRAME]` asks for: the movie's number in the movie table and the number
/// of the frame to stand at, from 1. Without a frame the movie plays.
/// `--press-start=SECONDS` holds the Start button from that second of the movie on, in
/// place of the keyboard.
#[derive(Resource)]
pub struct ShowMovie {
    movie: usize,
    frame: Option<u32>,
    muted: bool,
    press_start: Option<f64>,
    leads: Leads,
    /// The second from which Start is held in each movie that follows this one.
    skip_later: Option<f64>,
}

/// What a movie is played for, by what follows it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Leads {
    /// A look at one movie.
    Nowhere,
    /// The start of the game: the title screen follows.
    ToTheTitle,
    /// The first movie of a new game: the name entry follows.
    ToTheNameEntry,
    /// The movie behind the name entry: the field follows.
    ToTheField,
    /// The movie of a chapter: the chapter's first room follows.
    ToTheRoom,
    /// A movie that a script of a room plays: the room, which stands still under it,
    /// goes on.
    BackToTheRoom,
}

/// How the movies of the chapter screen and the ones that the scripts of a room play are
/// heard and skipped in this run: a run that is a check plays them as silence, and
/// `--skip-movies=SECONDS` holds the Start button from that second of each.
#[derive(Resource, Clone, Copy)]
pub struct ScriptMovies {
    pub muted: bool,
    pub skip: Option<f64>,
}

/// The layer a movie over a room is drawn in: the room's camera does not draw it, and
/// the movie's camera draws nothing of the room.
const OVER_THE_ROOM: usize = 1;

/// Marks the camera of a movie that plays over a room.
#[derive(Component)]
pub struct OverTheRoom;

const OPENING: usize = 1;

impl ShowMovie {
    /// The start of the game as the app has it: the opening movie, without the screen of
    /// file 56 and the two logo movies before it.
    pub fn opening() -> Self {
        Self {
            movie: OPENING,
            frame: None,
            muted: false,
            press_start: None,
            leads: Leads::ToTheTitle,
            skip_later: None,
        }
    }

    pub fn new_game(&self) -> Self {
        Self {
            movie: title::NEW_GAME_MOVIE,
            frame: None,
            muted: self.muted,
            press_start: self.skip_later,
            leads: Leads::ToTheNameEntry,
            skip_later: self.skip_later,
        }
    }

    /// The movie the new-game task plays when the hero has his name, heard or not as
    /// this one is.
    pub fn named(&self) -> Self {
        Self {
            movie: title::NAMED_MOVIE,
            frame: None,
            muted: self.muted,
            press_start: self.skip_later,
            leads: Leads::ToTheField,
            skip_later: self.skip_later,
        }
    }

    pub fn second_round(&self) -> Self {
        Self {
            movie: title::SECOND_ROUND_MOVIE,
            ..self.new_game()
        }
    }

    pub fn named_again(&self) -> Self {
        Self {
            movie: title::NAMED_AGAIN_MOVIE,
            ..self.named()
        }
    }

    pub fn chapter(movie: usize, how: &ScriptMovies) -> Self {
        Self {
            movie,
            frame: None,
            muted: how.muted,
            press_start: how.skip,
            leads: Leads::ToTheRoom,
            skip_later: how.skip,
        }
    }

    pub fn of_script(movie: usize, how: &ScriptMovies) -> Self {
        Self {
            movie,
            frame: None,
            muted: how.muted,
            press_start: how.skip,
            leads: Leads::BackToTheRoom,
            skip_later: how.skip,
        }
    }

    /// `--skip-movies=SECONDS`: in the movies that follow this one in a run of the game
    /// the Start button goes down that many seconds into each and stays down.
    pub fn skip_later(self, seconds: Option<f64>) -> Self {
        Self {
            skip_later: seconds,
            ..self
        }
    }

    /// This movie played once more behind a game, as "End Game" plays the opening movie:
    /// it follows the first one of the run, so `--skip-movies=SECONDS` holds Start in it
    /// when the run pressed nothing in the first.
    pub fn behind_a_game(&self) -> Self {
        Self {
            movie: self.movie,
            frame: self.frame,
            muted: self.muted,
            press_start: self.press_start.or(self.skip_later),
            leads: self.leads,
            skip_later: self.skip_later,
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        let (movie, frame) = match value.split_once(':') {
            Some((movie, frame)) => (movie, Some(frame.parse().ok()?)),
            None => (value, None),
        };
        Some(Self {
            movie: movie.parse().ok()?,
            frame,
            muted: false,
            press_start: None,
            leads: Leads::Nowhere,
            skip_later: None,
        })
    }

    /// The Start button goes down that many seconds into the movie and stays down.
    pub fn press_start(self, seconds: f64) -> Self {
        Self {
            press_start: Some(seconds),
            ..self
        }
    }

    /// A muted movie sends its sound to the sound device as silence: a run that is a
    /// check must not be heard.
    pub fn muted(self, muted: bool) -> Self {
        Self { muted, ..self }
    }
}

/// The sound of a movie that plays.
struct Sound {
    decoder: SoundDecoder,
    muted: bool,
    /// The line to the sound device and what plays it, from the first audio sector on.
    line: Option<(Line, Entity)>,
    /// Audio sectors read, and the moments of sound they held.
    sectors: u32,
    moments: u64,
}

/// The longest stretch of time one pass of the app may stand for: a pass that took longer
/// (the first ones do) does not make the drive catch up with a burst of sectors.
const LONGEST_PASS: f64 = 0.1;

pub type Failure = Box<dyn std::error::Error>;

/// The disc and the tables a movie is read with.
struct Source {
    disc: DiscImage<File>,
    record: MovieRecord,
    /// The sector the record names, which is the second one of the file.
    start: u32,
    /// The sector behind the last one of the file.
    end: u32,
    table: CodeTable,
    decoder: PictureDecoder,
}

impl Source {
    fn open(movie: usize) -> Result<Self, Failure> {
        let config = LocalConfig::load()?;
        let mut disc = DiscImage::open(&config.disc_image)?;
        let archive = Archive::open(&mut disc)?;
        let exe = Executable::read(&mut disc)?;
        let movies = MovieTable::from_executable(&exe.image, exe.base)?;
        let record = *movies
            .get(movie)
            .ok_or_else(|| format!("the movie table has {} movies", movies.len()))?;
        let file = record.file as usize;
        let entry = archive
            .entry(file)
            .ok_or("no such record in the file table")?;
        let start = archive.sector(file).ok_or("no sector for the record")?;
        Ok(Self {
            disc,
            record,
            start,
            end: start - 1 + entry.sectors,
            table: CodeTable::from_executable(&exe.image, exe.base)?,
            decoder: PictureDecoder::from_executable(&exe.image, exe.base)?,
        })
    }

    /// The frame that the sector completes, if it is a video sector and does.
    fn frame(&mut self, sector: u32, demuxer: &mut Demuxer) -> Result<Option<Frame>, Failure> {
        let raw = self.disc.read_raw_sector(sector)?;
        Ok(match Sector::parse(&raw)? {
            Sector::Video(sector) => demuxer.push(&sector),
            _ => None,
        })
    }

    /// The picture of a frame, with the scale and the number of codes of its bitstream.
    fn picture(&self, frame: &Frame) -> Result<(Picture, u16, usize), Failure> {
        let codes = self.table.expand(&frame.bitstream)?;
        let picture = (self.decoder).decode(&codes, frame.width.into(), frame.height.into())?;
        Ok((picture, codes.header.scale, codes.used))
    }

    /// The picture as the app draws it.
    fn image(&self, picture: &Picture) -> Image {
        Image::new(
            Extent3d {
                width: picture.width as u32,
                height: picture.height as u32,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            picture.rgba(self.record.depth),
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        )
    }

    /// The sprite of a picture: the shown area of the frame buffer fills the window and
    /// the picture lies in it where the record draws it.
    fn sprite(&self, picture: &Picture, image: Handle<Image>) -> (Sprite, Transform) {
        let record = &self.record;
        let shown = Vec2::new(
            f32::from(record.display.width),
            f32::from(record.display.height),
        );
        let window = Vec2::new(WINDOW_SIZE.0 as f32, WINDOW_SIZE.1 as f32);
        let size = Vec2::new(picture.width as f32, picture.height as f32);
        let corner = Vec2::new(
            f32::from(record.picture.x - record.display.x),
            f32::from(record.picture.y - record.display.y),
        );
        let middle = (corner + size / 2.0 - shown / 2.0) * Vec2::new(1.0, -1.0);
        (
            Sprite {
                image,
                custom_size: Some(size * window / shown),
                ..default()
            },
            Transform::from_translation((middle * window / shown).extend(0.0)),
        )
    }
}

/// A movie that plays.
#[derive(Resource)]
pub struct Playing {
    movie: usize,
    source: Source,
    demuxer: Demuxer,
    screening: Screening,
    /// The sector the drive hands over next.
    sector: u32,
    /// Sectors the drive owes for the time gone by, a share of one among them.
    owed: f64,
    /// Seconds since the drive started.
    time: f64,
    /// The sprite of the picture on screen, once there is one.
    sprite: Option<Entity>,
    /// Number of the frame on screen and how many pictures were shown.
    on_screen: u32,
    pictures: u32,
    sound: Sound,
    /// The pad as it was last read, once it has been, and how often the player's loop
    /// has read it.
    pad: Option<Pad>,
    turns: u32,
    /// The second from which the Start button is held whatever the keyboard says.
    press_start: Option<f64>,
    leads: Leads,
    over: bool,
}

pub fn show_movie(
    show: Res<ShowMovie>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    commands.spawn(Camera2d);
    let Some(asked) = show.frame else {
        // The movie plays. The screen stays black until the first picture: the player
        // switches the console's display off before it starts the stream.
        match Playing::start(&show) {
            Ok(playing) => commands.insert_resource(playing),
            Err(err) => {
                error!("movie {}: {err}", show.movie);
                exit.write(AppExit::error());
            }
        }
        return;
    };
    let (source, picture, scale, codes) = match load(show.movie, asked) {
        Ok(loaded) => loaded,
        Err(err) => {
            error!("movie {} frame {asked}: {err}", show.movie);
            exit.write(AppExit::error());
            return;
        }
    };
    let pixels = (picture.width * picture.height).max(1) as u64;
    let mean = [0, 1, 2].map(|colour| {
        let sum: u64 = (picture.rgb.iter().skip(colour).step_by(3))
            .map(|&value| u64::from(value))
            .sum();
        sum / pixels
    });
    info!(
        "movie {} (file table record {}) frame {}: {}x{}, scale {}, {} codes, mean colour {mean:?}",
        show.movie, source.record.file, asked, picture.width, picture.height, scale, codes
    );
    commands.spawn(source.sprite(&picture, images.add(source.image(&picture))));
}

/// Hands the playing movie the sectors of the time gone by and shows the picture they
/// bring: once per pass of the app, which stands in for the console's wait for the next
/// picture of the screen.
#[allow(clippy::too_many_arguments)]
pub fn play_movie(
    playing: Option<ResMut<Playing>>,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut images: ResMut<Assets<Image>>,
    mut sounds: ResMut<Assets<MovieSound>>,
    mut sprites: Query<&mut Sprite>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(mut playing) = playing else {
        return;
    };
    if playing.over {
        return;
    }
    let passed = time.delta_secs_f64().min(LONGEST_PASS);
    playing.time += passed;
    playing.owed += passed * f64::from(playing.source.record.sectors_a_second());
    let start = match playing.press_start {
        Some(from) => playing.time >= from,
        None => crate::keys::held(&keys) & pad::START != 0,
    };
    let held = if start { pad::START } else { 0 };
    // The main loop's last call of the pad routine before the player's first: a button
    // that was down before the movie is not one that went down in it.
    playing.pad.get_or_insert_with(|| {
        let mut pad = Pad::default();
        pad.read(held, Repeat::of(1), 0);
        pad
    });
    if let Err(err) = playing.read(held, &mut images, &mut sounds, &mut sprites, &mut commands) {
        error!("movie {}: {err}", playing.movie);
        playing.over = true;
        playing.hush(&mut commands);
        exit.write(AppExit::error());
    }
}

pub fn room_runs(
    playing: Option<Res<Playing>>,
    menu: Option<Res<crate::player_menu::MenuScreen>>,
    shop: Option<Res<crate::shop_view::ShopView>>,
    merge: Option<Res<crate::merge_view::MergeView>>,
    results: Option<Res<crate::results_view::ResultsView>>,
    card: Option<Res<crate::card_view::CardView>>,
    loaded: Option<Res<crate::saving::Loaded>>,
    ended: Option<Res<crate::saving::Ended>>,
) -> bool {
    let merge = merge.is_some_and(|merge| !merge.asleep());
    // Behind a load or "End Game" the field's task ends: the room that goes has no game
    // frame more.
    let gone = loaded.is_some() || ended.is_some();
    let card = card.is_some_and(|card| card.holds_the_room()) || gone;
    let screen = menu.is_some() || shop.is_some() || merge || results.is_some() || card;
    !playing.is_some_and(|playing| playing.holds_the_room()) && !screen
}

/// The movie of a room's script is over: its camera goes, and the room, which is as the
/// script left it, runs its game frames again.
pub fn back_to_the_room(
    playing: Option<Res<Playing>>,
    cameras: Query<Entity, With<OverTheRoom>>,
    mut commands: Commands,
) {
    if !playing.is_some_and(|playing| playing.holds_the_room() && playing.over) {
        return;
    }
    for camera in &cameras {
        commands.entity(camera).despawn();
    }
    commands.remove_resource::<Playing>();
}

impl Playing {
    /// The movie that is asked for, at the start of its stream.
    pub fn start(show: &ShowMovie) -> Result<Self, Failure> {
        let source = Source::open(show.movie)?;
        let record = &source.record;
        info!(
            "movie {} (file table record {}) plays from sector {}: {} sectors a second, ends with frame {}",
            show.movie,
            record.file,
            source.start,
            record.sectors_a_second(),
            record.end_frame
        );
        Ok(Self {
            movie: show.movie,
            demuxer: Demuxer::new(),
            screening: Screening::new(record),
            sector: source.start,
            owed: 0.0,
            time: 0.0,
            sprite: None,
            on_screen: 0,
            pictures: 0,
            sound: Sound {
                decoder: SoundDecoder::new(),
                muted: show.muted,
                line: None,
                sectors: 0,
                moments: 0,
            },
            pad: None,
            turns: 0,
            press_start: show.press_start,
            leads: show.leads,
            over: false,
            source,
        })
    }

    /// The movie was the start of the game and is over: the title screen is next.
    pub fn leads_to_the_title(&self) -> bool {
        self.leads == Leads::ToTheTitle && self.over
    }

    /// The movie was a chapter's and is over: the chapter screen goes on.
    pub fn leads_to_the_room(&self) -> bool {
        self.leads == Leads::ToTheRoom && self.over
    }

    /// The movie was the one behind the name entry and is over: the new game's last
    /// wait is next.
    pub fn leads_to_the_field(&self) -> bool {
        self.leads == Leads::ToTheField && self.over
    }

    /// The movie is one of a room's script, playing or over: the room stands still.
    pub fn holds_the_room(&self) -> bool {
        self.leads == Leads::BackToTheRoom
    }

    /// Puts the movie of a room's script over the room: its camera clears the screen to
    /// black, as the player clears the shown area before the first picture, and draws
    /// the movie's pictures alone.
    pub fn over_the_room(self, commands: &mut Commands) {
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
            RenderLayers::layer(OVER_THE_ROOM),
        ));
        commands.insert_resource(self);
    }

    /// The movie was the first one of a new game and is over: the name entry is next.
    pub fn leads_to_the_name_entry(&self) -> bool {
        self.leads == Leads::ToTheNameEntry && self.over
    }

    /// Reads the sectors that are owed, puts the pictures they bring on screen and their
    /// sound on its line. `held` is the pad's word of buttons in this pass.
    fn read(
        &mut self,
        held: u16,
        images: &mut Assets<Image>,
        sounds: &mut Assets<MovieSound>,
        sprites: &mut Query<&mut Sprite>,
        commands: &mut Commands,
    ) -> Result<(), Failure> {
        while self.owed >= 1.0 && !self.over {
            self.owed -= 1.0;
            if self.sector >= self.source.end {
                self.over = true;
                info!(
                    "movie {}: the file is over after {:.2} s without the end frame, frame {} stays on screen, picture {}",
                    self.movie, self.time, self.on_screen, self.pictures
                );
                self.hush(commands);
                break;
            }
            let sector = self.sector;
            self.sector += 1;
            let raw = self.source.disc.read_raw_sector(sector)?;
            let frame = match Sector::parse(&raw)? {
                Sector::Video(sector) => self.demuxer.push(&sector),
                Sector::Audio(sector) => {
                    self.hear(&sector, sounds, commands);
                    None
                }
                Sector::Other => None,
            };
            let Some(frame) = frame else {
                continue;
            };
            let arrival = self.screening.arrived(frame);
            if let Some(frame) = arrival.show {
                let (picture, ..) = self.source.picture(&frame)?;
                let image = images.add(self.source.image(&picture));
                match self.sprite.and_then(|sprite| sprites.get_mut(sprite).ok()) {
                    Some(mut sprite) => sprite.image = image,
                    None => {
                        // The first picture, or a second one in the pass that made the
                        // sprite: the newer picture takes its place.
                        if let Some(sprite) = self.sprite.take() {
                            commands.entity(sprite).despawn();
                        }
                        let sprite = self.source.sprite(&picture, image);
                        self.sprite = Some(match self.holds_the_room() {
                            true => {
                                (commands.spawn((sprite, RenderLayers::layer(OVER_THE_ROOM)))).id()
                            }
                            false => commands.spawn(sprite).id(),
                        });
                    }
                }
                if self.pictures == 0 {
                    info!(
                        "movie {}: the first picture is frame {}, after {:.2} s",
                        self.movie, frame.number, self.time
                    );
                }
                self.on_screen = frame.number;
                self.pictures += 1;
            }
            // The top of the next turn of the player's loop: the end flag first, then the
            // pad, which is asked only for a movie that can be skipped.
            let ended = match arrival.ended {
                true => Some("over"),
                false => self
                    .skipped(held)
                    .then_some("skipped with the Start button"),
            };
            if let Some(ended) = ended {
                // The player clears the shown area when its loop is over.
                self.over = true;
                if let Some(sprite) = self.sprite.take() {
                    commands.entity(sprite).despawn();
                }
                info!(
                    "movie {}: {ended} after {:.2} s and {} pictures, the last one frame {}",
                    self.movie, self.time, self.pictures, self.on_screen
                );
                self.hush(commands);
                match self.leads {
                    // The screen is black as the player leaves it; `title.rs` takes over.
                    Leads::ToTheTitle => info!("the title screen comes next"),
                    // `name_entry.rs` takes over.
                    Leads::ToTheNameEntry => info!("the name entry comes next"),
                    // The new-game task counts its last wait on the black screen.
                    Leads::ToTheField => info!("the last wait of the new game comes next"),
                    // `chapter.rs` takes over.
                    Leads::ToTheRoom => info!("the chapter's first room comes next"),
                    // `back_to_the_room` takes the movie away.
                    Leads::BackToTheRoom => info!("the room goes on"),
                    Leads::Nowhere => {}
                }
            }
        }
        Ok(())
    }

    /// The pad test at the top of a turn of the player's loop: the pad routine is called
    /// and the buttons that went down since its last call are held against the record's
    /// mask. A game frame is one picture of the screen in a movie; the count that paces
    /// the repeat is not needed for the buttons that went down.
    fn skipped(&mut self, held: u16) -> bool {
        if !self.source.record.can_be_skipped() {
            return false;
        }
        self.turns += 1;
        let pad = self.pad.get_or_insert_default();
        pad.read(held, Repeat::of(1), self.turns);
        self.source.record.skipped_by(pad.pressed)
    }

    /// Puts the sound of an audio sector on the line to the sound device. The first one
    /// makes the line: only an audio sector says how many samples a second it holds.
    fn hear(
        &mut self,
        sector: &AudioSector,
        sounds: &mut Assets<MovieSound>,
        commands: &mut Commands,
    ) {
        let samples = self.sound.decoder.decode(sector);
        let (left, right) = self.source.record.sound_volume();
        let (line, _) = self.sound.line.get_or_insert_with(|| {
            info!(
                "movie {}: the sound starts after {:.2} s, {} at {} samples a second, volume {left} and {right} of {LOUDEST}{}",
                self.movie,
                self.time,
                if sector.stereo { "stereo" } else { "mono" },
                sector.rate(),
                if self.sound.muted { ", muted for this run" } else { "" }
            );
            let line = Line::new(sector.rate());
            let player = AudioPlayer(sounds.add(MovieSound(line.clone())));
            (line, commands.spawn(player).id())
        });
        let share = |volume: u16| match self.sound.muted {
            true => 0.0,
            false => f32::from(volume) / f32::from(LOUDEST),
        };
        line.push(&samples, sector.stereo, (share(left), share(right)));
        self.sound.sectors += 1;
        self.sound.moments += (samples.len() / if sector.stereo { 2 } else { 1 }) as u64;
    }

    /// Ends the sound, as the player mutes the drive's sound when its loop is over.
    fn hush(&mut self, commands: &mut Commands) {
        let Some((line, player)) = self.sound.line.take() else {
            return;
        };
        line.close();
        commands.entity(player).despawn();
        info!(
            "movie {}: {} audio sectors with {:.2} s of sound, of which the sound device took {:.2} s",
            self.movie,
            self.sound.sectors,
            self.sound.moments as f64 / f64::from(line.rate()),
            line.taken() as f64 / f64::from(line.rate())
        );
    }
}

/// Reads the stream of the movie from the file's first sector up to the frame asked for
/// and decodes that frame: the picture, the scale and the number of codes.
fn load(movie: usize, asked: u32) -> Result<(Source, Picture, u16, usize), Failure> {
    let mut source = Source::open(movie)?;
    // The first frame is whole only from the sector before the one the record names.
    let mut demuxer = Demuxer::new();
    for sector in source.start - 1..source.end {
        let Some(frame) = source.frame(sector, &mut demuxer)? else {
            continue;
        };
        if frame.number != asked {
            continue;
        }
        let (picture, scale, codes) = source.picture(&frame)?;
        return Ok((source, picture, scale, codes));
    }
    Err("the movie has no such frame".into())
}
