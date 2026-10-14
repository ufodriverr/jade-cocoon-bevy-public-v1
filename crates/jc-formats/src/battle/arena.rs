
use crate::background::{PointLight, ViewHeader};
use crate::mover::ArcTangent;
use crate::rooms::{FIELD_ENGINE_FILE, Image, RoomTableError, load_address, step};
use crate::skeleton::{cosine, sine};

/// The archive file of the battle overlay, which holds the table of places.
pub const PLACES_FILE: usize = 36;
/// The table of places in the battle overlay: 16 bytes a place.
pub const PLACES: u32 = 0x800C_907C;
/// The archive file with the lists of rooms: the field engine.
pub const ROOMS_FILE: usize = FIELD_ENGINE_FILE;
/// A pointer for every place to its list of room numbers, in the field engine.
pub const ROOM_LISTS: u32 = 0x800A_E424;
/// The places of the disc.
pub const PLACE_COUNT: usize = 39;
/// The two opening views of the executable, 20 bytes each.
pub const OPENING: u32 = 0x8008_3B08;
/// The cameras of the field in the executable, 20 bytes each.
pub const CAMERAS: u32 = 0x8008_3B30;
/// The cameras `Pick` can name.
pub const CAMERA_COUNT: usize = 5;
pub const TARGETS: u32 = 0x8008_3B94;
pub const TARGET_COUNT: usize = 6;
pub const BY_TARGET: u32 = 0x8008_3C0C;
/// The column of [`BY_TARGET`] for a mark that names not exactly one opponent.
const NOT_ONE: usize = 3;

/// How far in front of the player the centre of a fight lies.
pub const TO_THE_CENTRE: i32 = 500;
/// How far the centre's height lies under the player's.
pub const UNDER_THE_PLAYER: i32 = 250;
/// The game frames the opening view stands.
pub const OPENING_FRAMES: u32 = 61;
/// The counter of a swing at its start, and what it falls by in a game frame.
pub const SWING_START: i32 = 4096;
pub const SWING_STEP: i32 = 256;
/// The two lists of the executable for a run of the camera along points, 176 bytes a
/// segment: the capture's and the one of the second request.
pub const CAPTURE_PATH: u32 = 0x8008_3C24;
pub const SECOND_PATH: u32 = 0x8008_4254;
pub const CAPTURE_SEGMENTS: usize = 9;
pub const SECOND_SEGMENTS: usize = 3;
/// How far behind the end of the player's draw the times of the capture's run lie; the
/// last two only count with the setting that makes the capture the longer one.
pub const CAPTURE_STEPS: [i32; CAPTURE_SEGMENTS] = [50, 70, 90, 110, 151, 200, 270, 361, 600];
pub const CAPTURE_PLAIN: usize = 7;
/// The times of the second request's run.
pub const SECOND_STEPS: [i32; SECOND_SEGMENTS] = [40, 80, 120];
pub const SHAKE: u32 = 0x8008_3AF8;
pub const SHAKE_FRAMES: usize = 15;
/// The times a run can have: the start, one for every segment and the end mark.
pub const TIMES: usize = CAPTURE_SEGMENTS + 2;
/// The time that ends a run.
pub const NO_TIME: i32 = -1;

const PLACE: usize = 16;
const CAMERA: usize = 20;
const SEGMENT: usize = 176;
/// The values a segment has a curve for.
const VALUES: usize = 11;
/// The pairs of further models a place can name.
const MODELS: usize = 3;
const NO_MODEL: u16 = 255;

/// One place: what its package holds for a fight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    /// The archive file of the six-section package.
    pub file: u16,
    /// The number of the ground model in the package.
    pub ground: u8,
    /// The numbers of the sprites whose quads stand round the centre.
    pub sprites: Vec<u16>,
    /// The index (not the number) of the first of the four sprite records of the strip
    /// of far background.
    pub strip: Option<u16>,
    /// The number of one more sprite, which moves.
    pub moving: Option<u16>,
    /// Further models: a number and a flag word each.
    pub models: Vec<(u16, u16)>,
}

/// The places of the disc and the rooms that pick each.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Places {
    places: Vec<Place>,
    rooms: Vec<Vec<i16>>,
}

/// A list of half words ended by -1.
fn numbers(image: &Image, start: u32) -> Result<Vec<i16>, RoomTableError> {
    let mut out = Vec::new();
    loop {
        let value = image.i16(step(start, out.len(), 2)?)?;
        if value == -1 {
            return Ok(out);
        }
        out.push(value);
    }
}

impl Places {
    /// The table of the battle overlay and the room lists of the field engine.
    pub fn from_files(battle: &[u8], field_engine: &[u8]) -> Result<Self, RoomTableError> {
        Self::parse(
            battle,
            load_address(PLACES_FILE),
            PLACES,
            field_engine,
            load_address(ROOMS_FILE),
            ROOM_LISTS,
            PLACE_COUNT,
        )
    }

    /// The same from any two images, for tests.
    pub fn parse(
        battle: &[u8],
        battle_base: u32,
        table: u32,
        field_engine: &[u8],
        engine_base: u32,
        lists: u32,
        count: usize,
    ) -> Result<Self, RoomTableError> {
        let image = Image {
            data: battle,
            base: battle_base,
        };
        let engine = Image {
            data: field_engine,
            base: engine_base,
        };
        let (mut places, mut rooms) = (Vec::new(), Vec::new());
        for index in 0..count {
            let at = step(table, index, PLACE)?;
            let sprites = numbers(&image, image.u32(at)?)?;
            let further = image.u32(at + 12)?;
            let mut models = Vec::new();
            if further != 0 {
                for pair in 0..MODELS {
                    let at = step(further, pair, 8)?;
                    let number = image.u16(at)?;
                    if number != NO_MODEL {
                        models.push((number, image.u16(at + 2)?));
                    }
                }
            }
            places.push(Place {
                file: image.u16(at + 4)?,
                ground: image.u8(at + 6)?,
                sprites: sprites.into_iter().map(|number| number as u16).collect(),
                strip: u16::try_from(image.i16(at + 8)?).ok(),
                moving: u16::try_from(image.i16(at + 10)?).ok(),
                models,
            });
            rooms.push(numbers(&engine, engine.u32(step(lists, index, 4)?)?)?);
        }
        Ok(Self { places, rooms })
    }

    pub fn len(&self) -> usize {
        self.places.len()
    }

    pub fn is_empty(&self) -> bool {
        self.places.is_empty()
    }

    pub fn get(&self, place: usize) -> Option<&Place> {
        self.places.get(place)
    }

    /// The rooms in the list of a place.
    pub fn rooms(&self, place: usize) -> &[i16] {
        self.rooms.get(place).map_or(&[], Vec::as_slice)
    }

    /// The place a room fights in: the first whose list holds the room, and place 0 for
    /// a room in no list.
    pub fn of_room(&self, room: u16) -> usize {
        self.rooms
            .iter()
            .position(|list| list.iter().any(|&listed| listed as u16 == room))
            .unwrap_or(0)
    }
}

/// The game's product of a table value and a length: over 4096, cut towards zero.
fn cut(product: i32) -> i32 {
    product / 4096
}

/// The middle of a fight, from where the player stands when it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Centre {
    pub place: [i32; 3],
    /// The way the player faces; a whole turn is 4096.
    pub angle: i32,
    /// The player's own height, which the cameras and the standing pictures count from.
    pub player_height: i32,
}

impl Centre {
    /// The centre for a player at `place` who faces `angle`.
    pub fn of(place: [i32; 3], angle: i32) -> Self {
        Self {
            place: [
                place[0] + cut(sine(angle) * TO_THE_CENTRE),
                place[1] + UNDER_THE_PLAYER,
                place[2] + cut(cosine(angle) * TO_THE_CENTRE),
            ],
            angle,
            player_height: place[1],
        }
    }

    /// Where the player stood when the fight was asked for, to within a unit, and how
    /// he faced.
    pub fn stand(&self) -> Stand {
        Stand {
            place: [
                self.place[0] - cut(sine(self.angle) * TO_THE_CENTRE),
                self.player_height,
                self.place[2] - cut(cosine(self.angle) * TO_THE_CENTRE),
            ],
            angle: self.angle,
        }
    }

    /// A point given by its way to the side and forward of the centre and its height
    /// from the player's.
    fn point(&self, [side, height, forward]: [i16; 3]) -> [i32; 3] {
        let (sine, cosine) = (sine(self.angle), cosine(self.angle));
        let (side, forward) = (side as i32, forward as i32);
        [
            self.place[0] + cut(cosine * side) + cut(sine * forward),
            self.player_height + height as i32,
            self.place[2] + cut(-sine * side) + cut(cosine * forward),
        ]
    }
}

/// Where a camera stands and looks, in the room.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shot {
    pub at: [i32; 3],
    pub looks: [i32; 3],
    /// The projection distance.
    pub distance: i32,
}

/// A camera of a fight as the executable holds it: side, height and forward of where it
/// stands and of where it looks, and the projection distance.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CameraRecord {
    pub stands: [i16; 3],
    pub distance: i32,
    pub looks: [i16; 3],
}

impl CameraRecord {
    fn read(image: &Image, at: u32) -> Result<Self, RoomTableError> {
        let three = |at: u32| -> Result<[i16; 3], RoomTableError> {
            Ok([image.i16(at)?, image.i16(at + 2)?, image.i16(at + 4)?])
        };
        Ok(Self {
            stands: three(at)?,
            distance: image.i32(at + 8)?,
            looks: three(at + 12)?,
        })
    }

    /// The camera for a fight with this centre.
    pub fn shot(&self, centre: &Centre) -> Shot {
        Shot {
            at: centre.point(self.stands),
            looks: centre.point(self.looks),
            distance: self.distance,
        }
    }
}

/// The lift of the menu's camera for a creature of the player's (`0x80037B0C`): this
/// much, plus the height of the creature's bone of this place in its table, as the pose
/// routine leaves it about the actor's place.
pub const CREATURE_LIFT: i32 = 245;
pub const LIFT_BONE: usize = 10;

/// What asks for a camera of the field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    /// The end of the opening view, every command of a fight without a boss, the end of
    /// a fight and the end of a run along points.
    Plain,
    /// A command while a boss is on the field.
    Boss,
    /// The routine's pick by a random number, which nothing on the disc asks for.
    Random(u32),
}

/// Where the player stands and how he faces in the game frame at hand: a run of the
/// camera along points counts from him.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stand {
    pub place: [i32; 3],
    /// A whole turn is 4096.
    pub angle: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    pub curves: [[f32; 4]; VALUES],
}

impl Segment {
    fn read(image: &Image, at: u32) -> Result<Self, RoomTableError> {
        let mut curves = [[0.0; 4]; VALUES];
        for (index, number) in curves.iter_mut().flatten().enumerate() {
            *number = f32::from_bits(image.u32(step(at, index, 4)?)?);
        }
        Ok(Self { curves })
    }

    /// A value at a share of the segment's time, in the order of the routine: the cube
    /// and the square added first, then the plain share and the start, and the whole
    /// part of that.
    pub fn value(&self, value: usize, share: f32) -> i32 {
        let [cube, square, plain, start] = self.curves[value];
        let squared = share * share;
        let cubed = squared * share;
        (cube * cubed + square * squared + plain * share + start) as i32
    }

    /// The camera at a share of the segment's time. Values 0 to 2 are a place and value
    /// 3 an angle about it; the values 4 to 6 (where the camera stands) and 8 to 10
    /// (where it looks) are turned by that angle and added to the place, and value 7 is
    /// the projection distance. All of it is counted from the player as he stands now:
    /// to his side, in height and the way he faces.
    pub fn shot(&self, share: f32, he: &Stand) -> Shot {
        let values: [i32; VALUES] = std::array::from_fn(|value| self.value(value, share));
        let about = |angle: i32, from: [i32; 3], [x, y, z]: [i32; 3]| {
            let (sine, cosine) = (sine(angle), cosine(angle));
            [
                from[0] + cut(x * cosine) + cut(z * sine),
                from[1] + y,
                from[2] - cut(x * sine) + cut(z * cosine),
            ]
        };
        let three = |first: usize| [values[first], values[first + 1], values[first + 2]];
        let in_room = |point: [i32; 3]| {
            let beside_him = about(values[3], three(0), point);
            about(he.angle, he.place, beside_him)
        };
        Shot {
            at: in_room(three(4)),
            looks: in_room(three(8)),
            distance: values[7] as i16 as i32,
        }
    }
}

/// Which of the two runs along points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Path {
    /// The player's own capture.
    Capture,
    /// The summon, kind 2, of a slot of his creatures' side.
    Second,
}

impl Path {
    /// The run the letting go of a fighter's own command starts (`0x800377E0`), by the
    /// side of its slot (0 the player, 1 his creature, 2 an opponent) and the kind.
    pub fn asked(side: u32, kind: u32) -> Option<Self> {
        match (side, kind) {
            (0, 5 | 26) => Some(Self::Capture),
            (1, 2) => Some(Self::Second),
            _ => None,
        }
    }

    /// The times of the run (`0x80039710`, `0x80039840`): the capture's lie behind the
    /// end of the player's draw of the weapon, and `long` is the setting that adds two.
    pub fn times(self, draw_ends: i32, long: bool) -> [i32; TIMES] {
        let mut times = [NO_TIME; TIMES];
        times[0] = 0;
        let (steps, from): (&[i32], i32) = match (self, long) {
            (Self::Capture, true) => (&CAPTURE_STEPS, draw_ends),
            (Self::Capture, false) => (&CAPTURE_STEPS[..CAPTURE_PLAIN], draw_ends),
            (Self::Second, _) => (&SECOND_STEPS, 0),
        };
        for (time, step) in times[1..].iter_mut().zip(steps) {
            *time = from + step;
        }
        times
    }
}

/// The segments of both runs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Paths {
    pub capture: Vec<Segment>,
    pub second: Vec<Segment>,
}

impl Paths {
    fn read(image: &Image) -> Result<Self, RoomTableError> {
        let list = |at: u32, count: usize| {
            (0..count)
                .map(|index| Segment::read(image, step(at, index, SEGMENT)?))
                .collect::<Result<Vec<_>, _>>()
        };
        Ok(Self {
            capture: list(CAPTURE_PATH, CAPTURE_SEGMENTS)?,
            second: list(SECOND_PATH, SECOND_SEGMENTS)?,
        })
    }

    pub fn of(&self, path: Path) -> &[Segment] {
        match path {
            Path::Capture => &self.capture,
            Path::Second => &self.second,
        }
    }
}

/// The cameras of a fight.
#[derive(Debug, Clone, PartialEq)]
pub struct Cameras {
    /// The view of the first game frames: from the centre onto the player.
    pub opening: CameraRecord,
    pub field: [CameraRecord; CAMERA_COUNT],
    /// The runs along points.
    pub paths: Paths,
    /// How far the picture is moved in each game frame of a shake.
    pub shake: [i8; SHAKE_FRAMES],
    /// The cameras of the command menu's targets: they stand behind the player and
    /// look over him at the opponents. Which one is taken, by the opponents on the
    /// field at once less one and by the number of the one marked opponent, the last
    /// column for any other mark.
    pub targets: [CameraRecord; TARGET_COUNT],
    pub by_target: [[u16; 4]; 3],
}

impl Cameras {
    pub fn from_executable(image: &[u8], base: u32) -> Result<Self, RoomTableError> {
        let image = Image { data: image, base };
        let mut field = [CameraRecord::read(&image, CAMERAS)?; CAMERA_COUNT];
        for (index, camera) in field.iter_mut().enumerate().skip(1) {
            *camera = CameraRecord::read(&image, step(CAMERAS, index, CAMERA)?)?;
        }
        let mut shake = [0; SHAKE_FRAMES];
        for (index, moved) in shake.iter_mut().enumerate() {
            *moved = image.u8(step(SHAKE, index, 1)?)? as i8;
        }
        let mut targets = [CameraRecord::default(); TARGET_COUNT];
        for (index, camera) in targets.iter_mut().enumerate() {
            *camera = CameraRecord::read(&image, step(TARGETS, index, CAMERA)?)?;
        }
        let mut by_target = [[0; 4]; 3];
        for (index, camera) in by_target.iter_mut().flatten().enumerate() {
            *camera = image.i16(step(BY_TARGET, index, 2)?)? as u16;
        }
        Ok(Self {
            opening: CameraRecord::read(&image, OPENING)?,
            field,
            paths: Paths::read(&image)?,
            shake,
            targets,
            by_target,
        })
    }

    /// The camera the command menu asks for with the mask of its marked slots
    /// (`0x80037B0C`), in a fight with `at_once` opponents on the field at once: by
    /// the number of the opponent when the mask names exactly one, else by the last
    /// column. The highest opponent of the mask counts, as the routine's loop leaves it.
    pub fn marked(&self, at_once: usize, mask: u8) -> Option<&CameraRecord> {
        let named = |opponent: &usize| mask >> (opponent + 4) & 1 != 0;
        let column = match (0..3).filter(named).count() {
            1 => (0..3).rfind(named)?,
            _ => NOT_ONE,
        };
        let camera = self.by_target.get(at_once.checked_sub(1)?)?[column];
        self.targets.get(usize::from(camera))
    }

    /// The camera a request picks.
    pub fn picked(&self, pick: Pick) -> &CameraRecord {
        &self.field[match pick {
            Pick::Plain => 0,
            Pick::Boss => 3,
            Pick::Random(drawn) => (drawn & 3) as usize,
        }]
    }

    pub fn at_frame(&self, centre: &Centre, arc: &ArcTangent, frame: u32) -> Shot {
        let from = self.opening.shot(centre);
        let Some(eased) = frame.checked_sub(OPENING_FRAMES + 1) else {
            return from;
        };
        let to = self.picked(Pick::Plain).shot(centre);
        let mut swing = Swing::between(centre, from, to, arc);
        for _ in 0..eased.min((SWING_START / SWING_STEP) as u32) {
            swing.step();
        }
        swing.shot(centre)
    }
}

/// The camera on its way from one shot to the next: round the centre on an arc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Swing {
    pub from: Shot,
    pub to: Shot,
    /// The angle and the distance of both places about the centre, as the game notes
    /// them when the swing starts; the target's angle is not more than half a turn from
    /// the start's.
    pub from_angle: i32,
    pub from_distance: i32,
    pub to_angle: i32,
    pub to_distance: i32,
    /// Falls from [`SWING_START`] to 0 by [`SWING_STEP`] a game frame.
    pub count: i32,
}

impl Swing {
    /// A swing at its start, as the pick of a camera notes it (`0x800390F8` with
    /// `0x80038D68`): the angle of each place about the centre from the library's arc
    /// tangent and its distance as the whole part of the root, flat in x and z.
    pub fn between(centre: &Centre, from: Shot, to: Shot, arc: &ArcTangent) -> Self {
        let about = |at: [i32; 3]| {
            let (x, z) = (at[0] - centre.place[0], at[2] - centre.place[2]);
            let squares = x.wrapping_mul(x).wrapping_add(z.wrapping_mul(z));
            (
                i32::from(arc.angle(x, z)) & 0xFFF,
                (squares as u32).isqrt() as i32,
            )
        };
        let ((from_angle, from_distance), (mut to_angle, to_distance)) =
            (about(from.at), about(to.at));
        // The target is not more than half a turn from the start.
        if to_angle - from_angle > 2048 {
            to_angle -= 4096;
        } else if to_angle - from_angle < -2048 {
            to_angle += 4096;
        }
        Self {
            from,
            to,
            from_angle,
            from_distance,
            to_angle,
            to_distance,
            count: SWING_START,
        }
    }

    /// The share of the start that is left: 4096 at the start, 0 at the end.
    pub fn share(&self) -> i32 {
        4096 - cosine(self.count / 4)
    }

    /// The camera of this game frame.
    pub fn shot(&self, centre: &Centre) -> Shot {
        let share = self.share();
        let between = |from: i32, to: i32| to + cut((from - to) * share);
        let angle = between(self.from_angle, self.to_angle);
        let distance = between(self.from_distance, self.to_distance) as i16 as i32;
        Shot {
            at: [
                centre.place[0] + cut(sine(angle) * distance),
                between(self.from.at[1], self.to.at[1]),
                centre.place[2] + cut(cosine(angle) * distance),
            ],
            looks: [0, 1, 2].map(|axis| between(self.from.looks[axis], self.to.looks[axis])),
            distance: between(self.from.distance, self.to.distance),
        }
    }

    /// One game frame on; `false` when the swing was already through.
    pub fn step(&mut self) -> bool {
        let moved = self.count > 0;
        self.count = (self.count - SWING_STEP).max(0);
        moved
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Eye {
    /// The passes made: the counter of the opening view.
    passes: u32,
    /// Where the camera stood in the last pass; none in front of the first.
    shot: Option<Shot>,
    /// The swing since the first pick; none while the opening view runs.
    swing: Option<Swing>,
    /// The run along points that is asked for, with its times: flag `0x008`.
    asked: Option<(Path, [i32; TIMES])>,
    /// State 5: the segment and the counter of the run.
    along: Option<(usize, i32)>,
    /// The counter of a shake (flags `0x040` and `0x080`), and how far below its place
    /// the picture is drawn in this game frame.
    shake: Option<usize>,
    scroll: i32,
}

impl Eye {
    /// The camera of the game frame: the opening view until a pass has moved it.
    pub fn shot(&self, cameras: &Cameras, centre: &Centre) -> Shot {
        (self.shot).unwrap_or_else(|| cameras.opening.shot(centre))
    }

    /// The segment and the counter of a run along points, while one has the camera.
    pub fn along(&self) -> Option<(usize, i32)> {
        self.along
    }

    /// The camera task's pass of a game frame. The opening view stands for
    /// [`OPENING_FRAMES`] passes, and the last of them picks the first camera of the
    /// field; from then on the easing is worked out with the counter as it stands and
    /// the counter falls behind it, and a run that is asked for starts behind that. A
    /// run counts up first and goes on to the next segment at its time; when the time
    /// behind that is the end mark, or the asking is gone, the first camera of the
    /// field is picked and the picture stays where it was for this pass.
    pub fn pass(&mut self, cameras: &Cameras, centre: &Centre, arc: &ArcTangent, he: &Stand) {
        self.passes += 1;
        // The scroll routine of the pass (`0x800398B8` with flag `0x004`): the byte of
        // the counter is the offset, and with the last one the shake is over.
        if let Some(count) = self.shake {
            self.scroll = cameras.shake[count].into();
            self.shake = Some(count + 1).filter(|count| *count < SHAKE_FRAMES);
            if self.shake.is_none() {
                self.scroll = 0;
            }
        }
        if let Some((mut segment, mut count)) = self.along {
            count += 1;
            let shot = self.asked.and_then(|(path, times)| {
                if count >= *times.get(segment + 1)? {
                    segment += 1;
                }
                let (from, to) = (times[segment], *times.get(segment + 1)?);
                let curves = cameras.paths.of(path).get(segment)?;
                let share = (count - from) as f32 / (to - from) as f32;
                (to != NO_TIME).then(|| curves.shot(share, he))
            });
            match shot {
                Some(shot) => {
                    self.shot = Some(shot);
                    self.along = Some((segment, count));
                }
                None => {
                    self.along = None;
                    self.pick(cameras, centre, arc, Pick::Plain);
                }
            }
            return;
        }
        match &mut self.swing {
            Some(swing) => {
                self.shot = Some(swing.shot(centre));
                swing.step();
                if self.asked.is_some() {
                    self.along = Some((0, 0));
                }
            }
            None => {
                self.shot = Some(cameras.opening.shot(centre));
                if self.passes >= OPENING_FRAMES {
                    self.pick(cameras, centre, arc, Pick::Plain);
                }
            }
        }
    }

    /// A camera of the field is picked (`0x800390F8`): the swing starts anew, from where
    /// the camera stands now to the picked record, and a run along points is no longer
    /// asked for.
    pub fn pick(&mut self, cameras: &Cameras, centre: &Centre, arc: &ArcTangent, pick: Pick) {
        let (from, to) = (
            self.shot(cameras, centre),
            cameras.picked(pick).shot(centre),
        );
        self.swing = Some(Swing::between(centre, from, to, arc));
        self.asked = None;
    }

    /// The command menu marks slots (`0x80037B0C` with `0x80038E30`, a twin of the
    /// pick of a camera of the field): while the camera task is in its state 4 or 5,
    /// the swing starts anew from where the camera stands to the menu's camera of
    /// those slots, and a run along points is over; during the opening view nothing
    /// is done. `lift` is added to the height the camera stands at: 0 for the player,
    /// and for his creature [CREATURE_LIFT] plus the height of its bone [LIFT_BONE]
    /// about its place (downwards is positive).
    pub fn mark(
        &mut self,
        cameras: &Cameras,
        centre: &Centre,
        arc: &ArcTangent,
        (at_once, mask): (usize, u8),
        lift: i32,
    ) -> bool {
        if self.swing.is_none() && self.along.is_none() {
            return false;
        }
        let Some(record) = cameras.marked(at_once, mask) else {
            return false;
        };
        let (from, mut to) = (self.shot(cameras, centre), record.shot(centre));
        to.at[1] += lift;
        self.swing = Some(Swing::between(centre, from, to, arc));
        (self.asked, self.along) = (None, None);
        true
    }

    /// The picture is shaken up and down (`0x80037C28`): the passes behind this move it
    /// by the table, and a shake that runs starts anew.
    pub fn shake(&mut self) {
        self.shake = Some(0);
    }

    /// How far the picture is moved down in this game frame: the vertical scroll
    /// offset `0x8008D406`, which is nothing but the shake during a fight.
    pub fn scroll(&self) -> i32 {
        self.scroll
    }

    /// A run along points is asked for (`0x80039710`, `0x80039840`): the pass behind
    /// this still eases the camera, the one behind that is the run's first.
    pub fn fly(&mut self, path: Path, times: [i32; TIMES]) {
        self.asked = Some((path, times));
    }
}

pub const AMBIENT: u8 = 64;
pub const LIGHT_COLOUR: i16 = 2048;
pub const LIGHT_RANGE: i32 = 3750;
pub const LIGHT_ABOVE: i32 = 1000;
pub const SUN: [i16; 3] = [2044, -2726, 2270];
pub const SUN_COLOUR: i16 = 4096;

impl Centre {
    /// The live view header of a fight seen through `shot`: the room's own with the
    /// camera, the light and the colours the camera task puts into it. No point light
    /// of the room keeps its range.
    pub fn header(&self, room: &ViewHeader, shot: &Shot) -> ViewHeader {
        let mut header = *room;
        for light in &mut header.point_lights {
            light.range = 0;
        }
        header.point_lights[0] = PointLight {
            position: [self.place[0], self.place[1] - LIGHT_ABOVE, self.place[2]],
            color: [LIGHT_COLOUR; 3],
            range: LIGHT_RANGE,
        };
        header.camera = shot.at;
        header.look_at = shot.looks;
        header.roll = 0;
        header.projection_distance = shot.distance as i16;
        header.color = [AMBIENT; 3];
        header.light_direction = SUN;
        header.light_color = [SUN_COLOUR; 3];
        header
    }

    /// Where a quad of a standing picture stands: its place from the centre, its height
    /// from the player's.
    pub fn stands(&self, [x, y, z]: [i16; 3]) -> [i32; 3] {
        [
            self.place[0] + i32::from(x),
            self.player_height + i32::from(y),
            self.place[2] + i32::from(z),
        ]
    }

    /// The point that gives the strip of far background its height on screen: 4096 from
    /// the centre the way the camera looks, 850 over the player's height. `turn` is the
    /// camera's turn word ([`Shot::turn`]).
    pub fn strip_point(&self, turn: i32) -> [i32; 3] {
        [
            self.place[0] + sine(-turn),
            self.player_height - STRIP_ABOVE,
            self.place[2] + cosine(-turn),
        ]
    }
}

impl Shot {
    pub fn turn(&self) -> i32 {
        let toward = |axis: usize| f64::from(self.looks[axis] - self.at[axis]);
        let turn = -toward(0).atan2(toward(2)) * 4096.0 / std::f64::consts::TAU;
        turn.round() as i32 & 0xFFF
    }
}

/// The colour a standing picture is drawn in: the ambient colour of the live header plus
/// the quad's own times the colour of point light 0, each sum kept as a byte.
pub fn picture_colour(colour: [u8; 3], header: &ViewHeader) -> [u8; 3] {
    std::array::from_fn(|part| {
        let lit = i32::from(colour[part]) * i32::from(header.point_lights[0].color[part]) / 4096;
        header.color[part].wrapping_add(lit as u8)
    })
}

/// The pieces of the strip of far background, side by side, and the sprite records they
/// go through in turn.
pub const STRIP_PIECES: usize = 32;
pub const STRIP_PICTURES: usize = 4;
/// How far over the player's height the strip's point lies.
pub const STRIP_ABOVE: i32 = 850;
/// The slot of the ordering table the strip is linked into: the farthest.
pub const STRIP_SLOT: u32 = 1023;
/// A piece's width at the projection distance [`STRIP_AT`].
const STRIP_STEP: i32 = 96;
const STRIP_AT: i32 = 240;
const SCREEN_WIDTH: i32 = 320;

/// Piece `piece` of the strip for a camera turned by `turn`: its left edge in pixels from
/// the left edge of the screen, and its width. `None` for a piece that is not drawn. The
/// piece reaches one pixel further right than its width, and from its width over the
/// strip's point to one pixel under it.
pub fn strip_piece(turn: i32, piece: usize, distance: i32) -> Option<(i32, i32)> {
    let left = ((turn & 0xFFF) / 4 + STRIP_STEP * piece as i32 - 1024) * distance / STRIP_AT;
    (-STRIP_STEP..SCREEN_WIDTH)
        .contains(&left)
        .then_some((left, STRIP_STEP * distance / STRIP_AT))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: u32 = 0x8000_0000;

    fn put(image: &mut [u8], at: usize, bytes: &[u8]) {
        image[at..at + bytes.len()].copy_from_slice(bytes);
    }

    /// Two places: the first with two sprites, a strip and no more, the second with no
    /// sprite, a moving one and one further model.
    fn images() -> (Vec<u8>, Vec<u8>) {
        let mut battle = vec![0u8; 0x80];
        for (at, value) in [(0x40, 40i16), (0x42, 41), (0x44, -1), (0x46, -1)] {
            put(&mut battle, at, &value.to_le_bytes());
        }
        // The pairs of further models: one, none, none.
        for (at, value) in [(0x50, 7u16), (0x52, 3), (0x58, 255), (0x60, 255)] {
            put(&mut battle, at, &value.to_le_bytes());
        }
        put(&mut battle, 0, &(BASE + 0x40).to_le_bytes());
        put(&mut battle, 4, &934u16.to_le_bytes());
        battle[6] = 37;
        put(&mut battle, 8, &7i16.to_le_bytes());
        put(&mut battle, 10, &(-1i16).to_le_bytes());
        put(&mut battle, 16, &(BASE + 0x46).to_le_bytes());
        put(&mut battle, 20, &965u16.to_le_bytes());
        battle[22] = 42;
        put(&mut battle, 24, &(-1i16).to_le_bytes());
        put(&mut battle, 26, &426i16.to_le_bytes());
        put(&mut battle, 28, &(BASE + 0x50).to_le_bytes());

        let mut engine = vec![0u8; 0x40];
        put(&mut engine, 0, &(BASE + 0x10).to_le_bytes());
        put(&mut engine, 4, &(BASE + 0x18).to_le_bytes());
        for (at, value) in [
            (0x10, 106i16),
            (0x12, 107),
            (0x14, -1),
            (0x18, 110),
            (0x1A, 107),
            (0x1C, -1),
        ] {
            put(&mut engine, at, &value.to_le_bytes());
        }
        (battle, engine)
    }

    #[test]
    fn a_table_of_places_is_read_with_its_rooms() {
        let (battle, engine) = images();
        let places = Places::parse(&battle, BASE, BASE, &engine, BASE, BASE, 2).expect("places");
        assert_eq!(places.len(), 2);
        let first = places.get(0).expect("place 0");
        assert_eq!((first.file, first.ground), (934, 37));
        assert_eq!(first.sprites, [40, 41]);
        assert_eq!((first.strip, first.moving), (Some(7), None));
        assert!(first.models.is_empty());
        let second = places.get(1).expect("place 1");
        assert_eq!((second.file, second.ground), (965, 42));
        assert!(second.sprites.is_empty());
        assert_eq!((second.strip, second.moving), (None, Some(426)));
        assert_eq!(second.models, [(7, 3)]);
        assert_eq!(places.rooms(1), [110, 107]);
    }

    #[test]
    fn a_room_fights_in_the_first_place_that_lists_it_and_else_in_place_0() {
        let (battle, engine) = images();
        let places = Places::parse(&battle, BASE, BASE, &engine, BASE, BASE, 2).expect("places");
        assert_eq!(places.of_room(110), 1);
        // In both lists: the first wins.
        assert_eq!(places.of_room(107), 0);
        // In no list.
        assert_eq!(places.of_room(1), 0);
    }

    #[test]
    fn a_list_that_leaves_the_image_is_an_error() {
        let (mut battle, engine) = images();
        put(&mut battle, 0, &(BASE + 0x1000).to_le_bytes());
        let read = Places::parse(&battle, BASE, BASE, &engine, BASE, BASE, 2);
        assert_eq!(read, Err(RoomTableError::OutOfImage(BASE + 0x1000)));
    }

    #[test]
    fn the_centre_lies_in_front_of_the_player_and_under_him() {
        // Angle 0 faces along z, a quarter turn along x.
        let centre = Centre::of([100, -40, 200], 0);
        assert_eq!(centre.place, [100, 210, 700]);
        assert_eq!(centre.player_height, -40);
        assert_eq!(Centre::of([100, -40, 200], 1024).place, [600, 210, 200]);
        assert_eq!(Centre::of([100, -40, 200], 2048).place, [100, 210, -300]);
    }

    #[test]
    fn a_camera_is_placed_by_the_side_and_the_way_the_player_faces() {
        let record = CameraRecord {
            stands: [1381, -795, -507],
            distance: 300,
            looks: [137, -102, 11],
        };
        // The player faces along z: the side is x, forward is z.
        let centre = Centre::of([0, 0, 0], 0);
        let shot = record.shot(&centre);
        assert_eq!(shot.at, [1381, -795, 500 - 507]);
        assert_eq!(shot.looks, [137, -102, 511]);
        assert_eq!(shot.distance, 300);
        // A quarter turn on: forward is x, the side is against z.
        let centre = Centre::of([0, 0, 0], 1024);
        assert_eq!(record.shot(&centre).at, [500 - 507, -795, -1381]);
    }

    #[test]
    fn the_requests_pick_their_cameras() {
        let camera = |side: i16| CameraRecord {
            stands: [side, 0, 0],
            distance: 300,
            looks: [0; 3],
        };
        let cameras = Cameras {
            opening: camera(9),
            field: [camera(0), camera(1), camera(2), camera(3), camera(4)],
            paths: Paths::default(),
            shake: [0; SHAKE_FRAMES],
            targets: [CameraRecord::default(); TARGET_COUNT],
            by_target: [[0; 4]; 3],
        };
        assert_eq!(cameras.picked(Pick::Plain).stands[0], 0);
        assert_eq!(cameras.picked(Pick::Boss).stands[0], 3);
        assert_eq!(cameras.picked(Pick::Random(0x1236)).stands[0], 2);
    }

    /// The command menu's cameras: one marked opponent names its column, any other
    /// mark the last one, and the mark only moves a camera that has left the opening
    /// view.
    #[test]
    fn the_menu_marks_and_the_camera_swings_to_its_target() {
        let camera = |stands: [i16; 3]| CameraRecord {
            stands,
            distance: 300,
            looks: [0; 3],
        };
        let mut targets = [CameraRecord::default(); TARGET_COUNT];
        for (number, target) in targets.iter_mut().enumerate() {
            *target = camera([number as i16 * 10, -300, -700]);
        }
        let cameras = Cameras {
            opening: camera([0, -200, 300]),
            field: [camera([1000, -800, 0]); CAMERA_COUNT],
            paths: Paths::default(),
            shake: [0; SHAKE_FRAMES],
            targets,
            by_target: [[0, 0, 0, 5], [3, 4, 0, 5], [1, 0, 2, 5]],
        };
        let side = |at_once: usize, mask: u8| cameras.marked(at_once, mask).map(|c| c.stands[0]);
        assert_eq!(side(1, 1 << 4), Some(0));
        assert_eq!(side(2, 1 << 5), Some(40));
        assert_eq!(side(3, 1 << 6), Some(20));
        // Two marked, nobody marked, and a slot of the own side beside one opponent.
        assert_eq!(side(3, 0x30), Some(50));
        assert_eq!(side(2, 0), Some(50));
        assert_eq!(side(2, 0x21), Some(40));
        assert_eq!((side(0, 1 << 4), side(4, 1 << 4)), (None, None));
        let centre = Centre::of([0, 0, -500], 0);
        let arc = arc();
        // During the opening view the mark does nothing.
        let mut eye = Eye::default();
        eye.pass(&cameras, &centre, &arc, &centre.stand());
        assert!(!eye.mark(&cameras, &centre, &arc, (1, 1 << 4), 0));
        for _ in 0..100 {
            eye.pass(&cameras, &centre, &arc, &centre.stand());
        }
        assert_eq!(eye.shot(&cameras, &centre), cameras.field[0].shot(&centre));
        // From the side of the field it swings behind the player in sixteen passes
        // (one more shows where it was), and a pick takes it back.
        assert!(eye.mark(&cameras, &centre, &arc, (1, 1 << 4), 0));
        eye.pass(&cameras, &centre, &arc, &centre.stand());
        assert_eq!(eye.shot(&cameras, &centre), cameras.field[0].shot(&centre));
        for _ in 0..16 {
            eye.pass(&cameras, &centre, &arc, &centre.stand());
        }
        let stands = eye.shot(&cameras, &centre);
        let wanted = cameras.targets[0].shot(&centre);
        assert_eq!(
            (stands.looks, stands.distance),
            (wanted.looks, wanted.distance)
        );
        assert!(
            (stands.at[0] - wanted.at[0]).abs() <= 1 && (stands.at[2] - wanted.at[2]).abs() <= 1
        );
        eye.pick(&cameras, &centre, &arc, Pick::Plain);
        for _ in 0..17 {
            eye.pass(&cameras, &centre, &arc, &centre.stand());
        }
        let back = eye.shot(&cameras, &centre);
        assert_eq!(back.looks, cameras.field[0].shot(&centre).looks);
    }

    #[test]
    fn a_swing_goes_round_the_centre_and_ends_on_its_target() {
        let centre = Centre::of([0, 0, -500], 0);
        assert_eq!(centre.place, [0, 250, 0]);
        let shot = |at: [i32; 3], looks: [i32; 3], distance: i32| Shot {
            at,
            looks,
            distance,
        };
        let mut swing = Swing {
            from: shot([0, -200, 1000], [0, -100, 0], 300),
            to: shot([2000, -800, 0], [100, -300, 0], 160),
            from_angle: 0,
            from_distance: 1000,
            to_angle: 1024,
            to_distance: 2000,
            count: SWING_START,
        };
        // At the start the whole of the start is left.
        assert_eq!(swing.share(), 4096);
        assert_eq!(swing.shot(&centre), swing.from);
        // Half way of the counter: 4096 less the cosine of an eighth of a turn.
        swing.count = 2048;
        assert_eq!(swing.share(), 4096 - 2896);
        let middle = swing.shot(&centre);
        // Neither on the straight line between the two places nor at either.
        assert!(middle.at[0] > 0 && middle.at[2] > 0);
        assert!(middle.at[0] * middle.at[0] + middle.at[2] * middle.at[2] > 1000 * 1000);
        // Sixteen steps bring it to the target, and a seventeenth changes nothing.
        swing.count = SWING_START;
        let mut steps = 0;
        while swing.step() {
            steps += 1;
        }
        assert_eq!(steps, 16);
        assert_eq!(swing.shot(&centre), swing.to);
        assert!(!swing.step());
    }

    /// A table like the game's, which the tests need only to be near the true angle.
    fn arc() -> ArcTangent {
        let step = |ratio: usize| {
            ((ratio as f64 / 1024.0).atan() * 4096.0 / std::f64::consts::TAU).round() as i16
        };
        ArcTangent::of((0..1025).map(step).collect())
    }

    #[test]
    fn a_swing_is_noted_by_angle_and_distance_about_the_centre() {
        let centre = Centre::of([0, 0, -500], 0);
        let shot = |at: [i32; 3]| Shot {
            at,
            looks: [0; 3],
            distance: 300,
        };
        // Along z is the angle 0, along x a quarter turn; the height does not count.
        let swing = Swing::between(
            &centre,
            shot([0, -200, 1000]),
            shot([300, -800, -400]),
            &arc(),
        );
        assert_eq!((swing.from_angle, swing.from_distance), (0, 1000));
        // The root of 300 * 300 + 400 * 400, and an angle behind the quarter turn.
        assert_eq!(swing.to_distance, 500);
        assert!((1024..2048).contains(&swing.to_angle));
        assert_eq!(swing.count, SWING_START);
        // The whole part of a root: 100 * 100 + 100 * 100 is 141.4 squared.
        assert_eq!(
            Swing::between(&centre, shot([100, 0, 100]), shot([0, 0, 1]), &arc()).from_distance,
            141
        );
        // The short way round: from just left of z to just right of it is not a whole turn.
        let swing = Swing::between(&centre, shot([-10, 0, 1000]), shot([10, 0, 1000]), &arc());
        assert!(swing.from_angle > 4000 && swing.to_angle > 4096);
        let swing = Swing::between(&centre, shot([10, 0, 1000]), shot([-10, 0, 1000]), &arc());
        assert!(swing.from_angle < 96 && swing.to_angle < 0);
    }

    #[test]
    fn the_opening_view_stands_and_then_swings_to_the_first_camera() {
        let camera = |stands: [i16; 3]| CameraRecord {
            stands,
            distance: 300,
            looks: [0; 3],
        };
        let cameras = Cameras {
            opening: camera([0, -200, 300]),
            field: [camera([1000, -800, 0]); CAMERA_COUNT],
            paths: Paths::default(),
            shake: [0; SHAKE_FRAMES],
            targets: [CameraRecord::default(); TARGET_COUNT],
            by_target: [[0; 4]; 3],
        };
        let centre = Centre::of([0, 0, -500], 0);
        let opening = cameras.opening.shot(&centre);
        let field = cameras.field[0].shot(&centre);
        assert_eq!((opening.at, field.at), ([0, -200, 300], [1000, -800, 0]));
        // 61 game frames of the opening view, and the first of the swing still shows it.
        for frame in [0, 1, 61, 62] {
            assert_eq!(cameras.at_frame(&centre, &arc(), frame), opening);
        }
        // Then it moves, and from the seventeenth game frame of the swing on it stands.
        let moving = cameras.at_frame(&centre, &arc(), 63);
        assert!(moving != opening && moving.at[0] > 0);
        assert!(cameras.at_frame(&centre, &arc(), 77) != field);
        assert_eq!(cameras.at_frame(&centre, &arc(), 78), field);
        assert_eq!(cameras.at_frame(&centre, &arc(), 5000), field);
    }

    #[test]
    fn a_pick_starts_the_swing_anew_from_where_the_camera_is() {
        let camera = |stands: [i16; 3]| CameraRecord {
            stands,
            distance: 300,
            looks: [0; 3],
        };
        let mut field = [camera([1000, -800, 0]); CAMERA_COUNT];
        field[3] = camera([1000, -200, -400]);
        let cameras = Cameras {
            opening: camera([0, -200, 300]),
            field,
            paths: Paths::default(),
            shake: [0; SHAKE_FRAMES],
            targets: [CameraRecord::default(); TARGET_COUNT],
            by_target: [[0; 4]; 3],
        };
        let centre = Centre::of([0, 0, -500], 0);
        let arc = arc();
        // Without a request the passes are the opening by game frames.
        let mut eye = Eye::default();
        assert_eq!(eye.shot(&cameras, &centre), cameras.opening.shot(&centre));
        for frame in 1..100 {
            eye.pass(&cameras, &centre, &arc, &centre.stand());
            let shot = eye.shot(&cameras, &centre);
            assert_eq!(shot, cameras.at_frame(&centre, &arc, frame), "{frame}");
        }
        // A runner asks in game frame 68, behind the camera's pass: the next pass shows
        // the camera where it was, then it swings from there, and it stands on the
        // picked camera sixteen passes later.
        let mut eye = Eye::default();
        for _ in 1..=68 {
            eye.pass(&cameras, &centre, &arc, &centre.stand());
        }
        let was = eye.shot(&cameras, &centre);
        assert_eq!(was, cameras.at_frame(&centre, &arc, 68));
        eye.pick(&cameras, &centre, &arc, Pick::Boss);
        eye.pass(&cameras, &centre, &arc, &centre.stand());
        let held = eye.shot(&cameras, &centre);
        assert_eq!(
            (held.at[1], held.looks, held.distance),
            (was.at[1], was.looks, was.distance)
        );
        assert!((held.at[0] - was.at[0]).abs() <= 1 && (held.at[2] - was.at[2]).abs() <= 1);
        let boss = cameras.field[3].shot(&centre);
        for _ in 0..15 {
            eye.pass(&cameras, &centre, &arc, &centre.stand());
            assert!(eye.shot(&cameras, &centre) != boss);
        }
        eye.pass(&cameras, &centre, &arc, &centre.stand());
        let stands = eye.shot(&cameras, &centre);
        assert_eq!((stands.at[1], stands.looks), (boss.at[1], boss.looks));
        assert!((stands.at[0] - boss.at[0]).abs() <= 1 && (stands.at[2] - boss.at[2]).abs() <= 1);
        // The same camera asked for again all but leaves the picture where it is, and
        // ends on the same place.
        eye.pick(&cameras, &centre, &arc, Pick::Boss);
        for _ in 0..20 {
            eye.pass(&cameras, &centre, &arc, &centre.stand());
            let still = eye.shot(&cameras, &centre);
            assert_eq!((still.at[1], still.looks), (stands.at[1], stands.looks));
            // The place is made anew from its angle about the centre, a step of which is
            // more than a unit at this distance.
            assert!((still.at[0] - stands.at[0]).abs() <= 3);
            assert!((still.at[2] - stands.at[2]).abs() <= 3);
        }
        assert_eq!(eye.shot(&cameras, &centre), stands);
    }

    #[test]
    fn a_segment_turns_its_points_about_a_place_beside_the_player() {
        let mut curves = [[0.0; 4]; VALUES];
        // A curve is worked out from the cube down and cut to its whole part.
        curves[0] = [8.0, 4.0, 2.0, 1.0];
        curves[1] = [0.0, 0.0, -3.0, 0.5];
        curves[2] = [0.0, 0.0, 0.0, -0.9];
        let segment = Segment { curves };
        assert_eq!(segment.value(0, 0.5), 4);
        assert_eq!(segment.value(1, 0.5), -1);
        assert_eq!(segment.value(2, 0.5), 0);
        // A place 1000 in front of him and 100 up, the camera 200 and 50 more up from
        // it at a quarter turn, looking at the place.
        let mut curves = [[0.0; 4]; VALUES];
        for (value, start) in [
            (1, -100.0),
            (2, 1000.0),
            (3, 1024.0),
            (5, -50.0),
            (6, 200.0),
        ] {
            curves[value][3] = start;
        }
        curves[7][3] = 160.0;
        let segment = Segment { curves };
        let mut he = Stand {
            place: [10, 20, 30],
            angle: 0,
        };
        let shot = segment.shot(0.3, &he);
        assert_eq!(shot.at, [210, -130, 1030]);
        assert_eq!(shot.looks, [10, -80, 1030]);
        assert_eq!(shot.distance, 160);
        // He turns a quarter: what was in front of him is now along x.
        he.angle = 1024;
        let shot = segment.shot(0.3, &he);
        assert_eq!(shot.at, [1010, -130, -170]);
        assert_eq!(shot.looks, [1010, -80, 30]);
    }

    #[test]
    fn the_times_of_a_run_lie_behind_the_draw() {
        assert_eq!(
            Path::Capture.times(81, false),
            [0, 131, 151, 171, 191, 232, 281, 351, -1, -1, -1]
        );
        assert_eq!(
            Path::Capture.times(81, true),
            [0, 131, 151, 171, 191, 232, 281, 351, 442, 681, -1]
        );
        assert_eq!(
            Path::Second.times(81, true),
            [0, 40, 80, 120, -1, -1, -1, -1, -1, -1, -1]
        );
        assert_eq!(Path::asked(0, 5), Some(Path::Capture));
        assert_eq!(Path::asked(0, 26), Some(Path::Capture));
        assert_eq!(Path::asked(1, 2), Some(Path::Second));
        assert_eq!((Path::asked(0, 2), Path::asked(2, 5)), (None, None));
    }

    #[test]
    fn a_run_along_points_takes_the_camera_until_its_times_are_through() {
        let camera = |stands: [i16; 3]| CameraRecord {
            stands,
            distance: 300,
            looks: [0; 3],
        };
        // Segment n lies n hundred in front of the player, and the camera goes ten to
        // the side in each.
        let segment = |number: usize| {
            let mut curves = [[0.0; 4]; VALUES];
            curves[2][3] = 100.0 * number as f32;
            curves[4][2] = 10.0;
            curves[7][3] = 160.0;
            Segment { curves }
        };
        let mut field = [camera([1000, -800, 0]); CAMERA_COUNT];
        field[3] = camera([1000, -200, -400]);
        let cameras = Cameras {
            opening: camera([0, -200, 300]),
            field,
            paths: Paths {
                capture: (0..CAPTURE_SEGMENTS).map(segment).collect(),
                second: Vec::new(),
            },
            shake: [0; SHAKE_FRAMES],
            targets: [CameraRecord::default(); TARGET_COUNT],
            by_target: [[0; 4]; 3],
        };
        let centre = Centre::of([0, 0, 0], 0);
        let (arc, he) = (arc(), centre.stand());
        let mut times = [NO_TIME; TIMES];
        times[..3].copy_from_slice(&[0, 4, 6]);
        let flown = |side: i32, forward: i32| Shot {
            at: [side, 0, forward],
            looks: [0, 0, forward],
            distance: 160,
        };
        let mut eye = Eye::default();
        for _ in 0..90 {
            eye.pass(&cameras, &centre, &arc, &he);
        }
        let stood = eye.shot(&cameras, &centre);
        // The pass behind the asking still shows the camera of the field.
        eye.fly(Path::Capture, times);
        eye.pass(&cameras, &centre, &arc, &he);
        assert_eq!(
            (eye.shot(&cameras, &centre), eye.along()),
            (stood, Some((0, 0)))
        );
        // Then the run: a quarter of the first segment a pass, the second from its time
        // on, at its start.
        let mut seen = Vec::new();
        for _ in 0..5 {
            eye.pass(&cameras, &centre, &arc, &he);
            seen.push(eye.shot(&cameras, &centre));
        }
        assert_eq!(
            seen,
            [
                flown(2, 0),
                flown(5, 0),
                flown(7, 0),
                flown(0, 100),
                flown(5, 100)
            ]
        );
        assert_eq!(eye.along(), Some((1, 5)));
        // The time behind the second is the end mark: the picture stays for one pass
        // and the camera swings back to the first of the field.
        eye.pass(&cameras, &centre, &arc, &he);
        assert_eq!(
            (eye.shot(&cameras, &centre), eye.along()),
            (flown(5, 100), None)
        );
        for _ in 0..17 {
            eye.pass(&cameras, &centre, &arc, &he);
        }
        let back = eye.shot(&cameras, &centre);
        assert_eq!(
            (back.at[1], back.looks, back.distance),
            (-800, [0, 0, 500], 300)
        );
        // A camera that is picked in the middle of a run ends it, and the run's end
        // picks the first camera of the field over it.
        eye.fly(Path::Capture, times);
        for _ in 0..3 {
            eye.pass(&cameras, &centre, &arc, &he);
        }
        assert_eq!(eye.along(), Some((0, 2)));
        eye.pick(&cameras, &centre, &arc, Pick::Boss);
        eye.pass(&cameras, &centre, &arc, &he);
        assert_eq!(
            (eye.shot(&cameras, &centre), eye.along()),
            (flown(5, 0), None)
        );
        for _ in 0..17 {
            eye.pass(&cameras, &centre, &arc, &he);
        }
        assert_eq!(eye.shot(&cameras, &centre).at[1], -800);
        // A run that is asked for while the opening view stands is lost: the pick at
        // the view's end takes the asking away.
        let mut eye = Eye::default();
        eye.fly(Path::Capture, times);
        for _ in 0..OPENING_FRAMES + 5 {
            eye.pass(&cameras, &centre, &arc, &he);
            assert_eq!(eye.along(), None);
        }
    }

    #[test]
    fn a_shake_moves_the_picture_by_the_table_and_is_over_with_its_last_byte() {
        let camera = CameraRecord {
            stands: [1000, -800, 0],
            distance: 300,
            looks: [0; 3],
        };
        let mut shake = [0; SHAKE_FRAMES];
        for (index, moved) in shake.iter_mut().enumerate() {
            *moved = 20 - index as i8;
        }
        let cameras = Cameras {
            opening: camera,
            field: [camera; CAMERA_COUNT],
            paths: Paths::default(),
            shake,
            targets: [CameraRecord::default(); TARGET_COUNT],
            by_target: [[0; 4]; 3],
        };
        let centre = Centre::of([0, 0, 0], 0);
        let (arc, he) = (arc(), centre.stand());
        let mut eye = Eye::default();
        let pass = |eye: &mut Eye| {
            eye.pass(&cameras, &centre, &arc, &he);
            eye.scroll()
        };
        // Without a shake the picture stands.
        assert_eq!([pass(&mut eye), pass(&mut eye)], [0, 0]);
        // The asking moves nothing; the passes behind it do, one byte each, and the
        // pass of the last byte puts the picture back.
        eye.shake();
        assert_eq!(eye.scroll(), 0);
        let moved: Vec<i32> = (0..SHAKE_FRAMES + 2).map(|_| pass(&mut eye)).collect();
        let mut table: Vec<i32> = (0..SHAKE_FRAMES as i32 - 1)
            .map(|index| 20 - index)
            .collect();
        table.extend([0, 0, 0]);
        assert_eq!(moved, table);
        // A shake that is asked for again starts anew.
        eye.shake();
        assert_eq!([pass(&mut eye), pass(&mut eye)], [20, 19]);
        eye.shake();
        assert_eq!(pass(&mut eye), 20);
    }

    #[test]
    fn the_header_of_a_fight_keeps_one_light_over_the_centre() {
        let mut room = ViewHeader::parse(&[0u8; crate::background::HEADER_SIZE]).expect("header");
        room.roll = 47;
        room.point_lights[0].range = 500;
        room.point_lights[3].range = 900;
        let centre = Centre::of([100, -40, 200], 0);
        let shot = Shot {
            at: [1481, -835, 193],
            looks: [237, -142, 711],
            distance: 300,
        };
        let header = centre.header(&room, &shot);
        assert_eq!((header.camera, header.look_at), (shot.at, shot.looks));
        assert_eq!((header.roll, header.projection_distance), (0, 300));
        assert_eq!(header.color, [64; 3]);
        assert_eq!(header.light_direction, [2044, -2726, 2270]);
        let light = header.point_lights[0];
        assert_eq!(light.position, [100, 210 - 1000, 700]);
        assert_eq!((light.color, light.range), ([2048; 3], 3750));
        assert!(
            header.point_lights[1..]
                .iter()
                .all(|light| light.range == 0)
        );
        // A picture is drawn at the ambient colour plus half its own.
        assert_eq!(picture_colour([128, 108, 89], &header), [128, 118, 108]);
    }

    #[test]
    fn a_standing_picture_counts_its_height_from_the_player() {
        let centre = Centre::of([100, -40, 200], 0);
        assert_eq!(centre.stands([-614, -300, 825]), [-514, -340, 1525]);
    }

    #[test]
    fn the_strip_slides_with_the_turn_of_the_camera() {
        // Looking along z the turn is 0, looking along x a quarter turn the other way.
        let shot = |looks: [i32; 3]| Shot {
            at: [0; 3],
            looks,
            distance: 300,
        };
        assert_eq!(shot([0, 0, 500]).turn(), 0);
        assert_eq!(shot([500, 0, 0]).turn(), 3072);
        // The point lies the way the camera looks.
        let centre = Centre::of([0, 0, -500], 0);
        assert_eq!(centre.strip_point(3072), [4096, -850, 0]);
        assert_eq!(centre.strip_point(0), [0, -850, 4096]);
        // At the distance 300 a piece is 120 pixels wide and a turn of 4 moves it by 1.25.
        assert_eq!(strip_piece(0, 10, 300), Some((-80, 120)));
        assert_eq!(strip_piece(0, 11, 300), Some((40, 120)));
        assert_eq!(strip_piece(4, 11, 300), Some((41, 120)));
        // Beyond either edge of the screen a piece is left out.
        assert_eq!(strip_piece(0, 9, 300), None);
        assert_eq!(strip_piece(0, 13, 300), Some((280, 120)));
        assert_eq!(strip_piece(0, 14, 300), None);
        // The turn counts by its low twelve bits.
        assert_eq!(strip_piece(4096 + 4, 11, 300), strip_piece(4, 11, 300));
    }
}
