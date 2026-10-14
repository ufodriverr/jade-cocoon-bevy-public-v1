
use std::fmt;

/// The top bit of the two count words is a mark, not part of the count.
const MARK: u32 = 0x8000_0000;
/// Bytes of one entry: three signed 16-bit values.
const ENTRY: usize = 6;
/// Entries of a frame that come before the first bone's angles.
const FRAME_HEAD: usize = 2;
/// Bytes of one event.
const EVENT: usize = 8;

/// Why bytes are not an animation block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationError {
    /// The block needs more bytes than there are. Holds the offset that was wanted.
    Truncated(usize),
    /// A block has at least one frame and one bone.
    Empty,
    /// A list of resident animations names an offset outside its section.
    BadOffset(usize),
}

impl fmt::Display for AnimationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated(at) => write!(f, "animation block ends before offset {at:#x}"),
            Self::Empty => write!(f, "animation block without frames or bones"),
            Self::BadOffset(at) => write!(f, "animation list names the offset {at:#x}"),
        }
    }
}

impl std::error::Error for AnimationError {}

/// A record of the list behind an animation's frames: something that happens at one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
    /// The frame, in nearly every event; a few name a frame behind the last one or -1.
    pub frame: i16,
    /// The four 16-bit values of the record as they are; the first is `frame`.
    pub raw: [i16; 4],
}

/// Records of the executable's effect table, which an event of kind 0 names one of.
pub const EFFECTS: u16 = 110;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventAction {
    /// A record of the executable's effect table: a sound.
    Effect(u16),
    /// A mesh object shows the mesh of `source` from here on; its own number brings its own
    /// mesh back, a number past the model's mesh objects is one of the model's parts.
    Mesh { object: i16, source: i16 },
    /// A kind, or an effect outside the table, that the game does nothing for.
    Nothing,
}

impl Event {
    /// The second value is the kind; the third and fourth are what the kind reads.
    pub fn action(&self) -> EventAction {
        let [_, kind, first, second] = self.raw;
        match kind {
            0 if (first as u16) < EFFECTS => EventAction::Effect(first as u16),
            1 => EventAction::Mesh {
                object: first,
                source: second,
            },
            _ => EventAction::Nothing,
        }
    }
}

/// One animation of a model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Animation {
    pub frames: usize,
    /// Bones the block has values for, which can be more than the skeleton that plays it.
    pub bones: usize,
    /// The word some blocks carry behind the counts; the game uses 32 where there is none.
    pub extra: Option<u32>,
    rest: Vec<[i16; 3]>,
    keys: Vec<[i16; 3]>,
    pub events: Vec<Event>,
    /// Bytes of the block.
    pub len: usize,
}

fn word(data: &[u8], at: usize) -> Result<u32, AnimationError> {
    let bytes = data.get(at..at + 4).ok_or(AnimationError::Truncated(at))?;
    Ok(u32::from_le_bytes(bytes.try_into().expect("four bytes")))
}

/// `count` entries of three or four 16-bit values from `at`.
fn entries<const N: usize>(
    data: &[u8],
    at: usize,
    count: usize,
) -> Result<Vec<[i16; N]>, AnimationError> {
    let end = at + count * 2 * N;
    let bytes = data.get(at..end).ok_or(AnimationError::Truncated(end))?;
    Ok(bytes
        .chunks_exact(2 * N)
        .map(|entry| std::array::from_fn(|i| i16::from_le_bytes([entry[2 * i], entry[2 * i + 1]])))
        .collect())
}

impl Animation {
    /// Reads the block at the start of `data`; bytes behind it are ignored.
    pub fn parse(data: &[u8]) -> Result<Self, AnimationError> {
        let (frames, bones) = (word(data, 0)?, word(data, 4)?);
        let (has_extra, has_events) = (frames & MARK != 0, bones & MARK != 0);
        let (frames, bones) = ((frames & !MARK) as usize, (bones & !MARK) as usize);
        if frames == 0 || bones == 0 {
            return Err(AnimationError::Empty);
        }
        let mut at = 8;
        let extra = if has_extra {
            at += 4;
            Some(word(data, 8)?)
        } else {
            None
        };
        // A size that cannot be told is the size of a block that is not there.
        let key_count = frames
            .checked_mul(bones + FRAME_HEAD)
            .filter(|count| *count <= data.len() / ENTRY)
            .ok_or(AnimationError::Truncated(data.len()))?;
        let rest = entries::<3>(data, at, bones)?;
        at += bones * ENTRY;
        let keys = entries::<3>(data, at, key_count)?;
        at = (at + key_count * ENTRY).next_multiple_of(4);
        let mut events = Vec::new();
        if has_events {
            let count = word(data, at)? as usize;
            if count > data.len() / EVENT {
                return Err(AnimationError::Truncated(data.len()));
            }
            events = entries::<4>(data, at + 4, count)?
                .into_iter()
                .map(|raw| Event { frame: raw[0], raw })
                .collect();
            at += 4 + count * EVENT;
        }
        Ok(Self {
            frames,
            bones,
            extra,
            rest,
            keys,
            events,
            len: at,
        })
    }

    /// The animations of a model file's resident section: a count, an offset per animation,
    /// the blocks. A section of zeros holds none.
    pub fn resident(section: &[u8]) -> Result<Vec<Self>, AnimationError> {
        let count = word(section, 0)? as usize;
        if count > section.len() / 4 {
            return Err(AnimationError::Truncated(section.len()));
        }
        (0..count)
            .map(|index| {
                let offset = word(section, 4 + 4 * index)? as usize;
                let block = section
                    .get(offset..)
                    .ok_or(AnimationError::BadOffset(offset))?;
                Self::parse(block)
            })
            .collect()
    }

    /// Where a bone sits against its parent while nothing moves it. None behind the last bone.
    pub fn rest(&self, bone: usize) -> Option<[i16; 3]> {
        self.rest.get(bone).copied()
    }

    fn entry(&self, frame: usize, entry: usize) -> Option<[i16; 3]> {
        (frame < self.frames && entry < self.bones + FRAME_HEAD)
            .then(|| self.keys[frame * (self.bones + FRAME_HEAD) + entry])
    }

    /// The three angles of a bone in a frame, 4096 to the full turn.
    pub fn angles(&self, frame: usize, bone: usize) -> Option<[i16; 3]> {
        self.entry(frame, bone + FRAME_HEAD)
    }

    pub fn travel(&self, frame: usize) -> Option<[i16; 3]> {
        self.entry(frame, 0)
    }

    /// The second entry of a frame: where the skeleton's second bone sits in that frame,
    /// in place of its rest offset.
    pub fn root(&self, frame: usize) -> Option<[i16; 3]> {
        self.entry(frame, 1)
    }
}

/// One in the fixed point of a playback's speed and of a fade's weight.
pub const WHOLE: i32 = 4096;
/// What the weight of a quick fade gains in a game frame.
const QUICK_FADE: i32 = 273;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub first: i16,
    pub loop_start: i16,
    pub end: i16,
}

impl Span {
    /// Every frame of an animation, round and round. No table of the game is behind this: it
    /// is what a debug view plays when it knows the animation and not the actor.
    pub fn whole(animation: &Animation) -> Self {
        Self {
            first: 0,
            loop_start: 0,
            end: (animation.frames - 1).min(i16::MAX as usize) as i16,
        }
    }

    /// The frame a start value of -3 picks (`0x8001D884`): the first frame plus a draw
    /// of the game's generator modulo the frames from there to the loop start. The game
    /// divides without a test; a span whose loop starts on its first frame has that
    /// frame here (HYPOTHESIS: what the console's division by zero leaves was not
    /// followed).
    pub fn random(&self, draw: u32) -> i32 {
        let (first, loop_start) = (i32::from(self.first), i32::from(self.loop_start));
        first + (draw as i32).checked_rem(loop_start - first).unwrap_or(0)
    }
}

/// Where in its span a started animation begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    First,
    LoopStart,
    /// This frame, wrapped at the span's end.
    At(i32),
}

/// The animation an actor played before, while the pose still mixes it in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fade {
    /// The animation that fades out and the frame counter it stopped at.
    pub animation: usize,
    pub counter: i32,
    /// How far the fade has come, from 0 to [`WHOLE`].
    pub weight: i32,
    /// Game frames the fade takes.
    pub length: i16,
    /// A fade that gains a fixed fifteenth per game frame whatever its length.
    pub quick: bool,
}

impl Fade {
    /// The share of the old animation in the pose, [`WHOLE`] when the fade starts.
    pub fn old_share(&self) -> i32 {
        WHOLE - self.weight
    }
}

/// What a step of a playback did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stepped {
    /// The counter stands on the span's end.
    pub at_end: bool,
    /// The counter went past the end and is back at the loop start.
    pub looped: bool,
}

/// The animation state of an actor, stepped once per game frame as the game steps it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Playback {
    pub animation: usize,
    /// The frame counter. The frame that is shown is the counter modulo the animation's frames.
    pub counter: i32,
    /// Parts of a frame gathered so far, [`WHOLE`] to the frame.
    fraction: i16,
    /// What a game frame adds to `fraction`: [`WHOLE`] plays one frame per game frame.
    pub speed: i16,
    pub span: Span,
    pub paused: bool,
    pub fade: Option<Fade>,
}

impl Playback {
    /// An actor that starts in `animation` at the span's first frame.
    pub fn new(animation: usize, span: Span, speed: i16) -> Self {
        Self {
            animation,
            counter: span.first as i32,
            fraction: 0,
            speed,
            span,
            paused: false,
            fade: None,
        }
    }

    /// Starts another animation. With `fade` (game frames, more than none) the animation
    /// played so far is kept at its frame and mixed out of the pose over that time.
    pub fn start(&mut self, animation: usize, span: Span, start: Start, fade: Option<i16>) {
        let old = (self.animation, self.counter);
        self.animation = animation;
        self.span = span;
        self.counter = match start {
            Start::First => span.first as i32,
            Start::LoopStart => span.loop_start as i32,
            // The game divides by the end without a test; a span that ends on frame 0 has
            // only that frame.
            Start::At(frame) => frame.checked_rem(span.end as i32).unwrap_or(0),
        };
        self.fraction = 0;
        self.fade = fade.filter(|length| *length > 0).map(|length| Fade {
            animation: old.0,
            counter: old.1,
            weight: 0,
            length,
            quick: false,
        });
    }

    /// The counter is put on the end frame of the span and the gathered parts of a
    /// frame are dropped (`0x8001DA18`, what a taken confirm does to a busy actor).
    pub fn to_end(&mut self) {
        self.counter = self.span.end as i32;
        self.fraction = 0;
    }

    /// One game frame.
    pub fn step(&mut self) -> Stepped {
        let mut stepped = Stepped::default();
        if !self.paused {
            self.fraction = self.fraction.wrapping_add(self.speed);
            if self.fraction as i32 >= WHOLE {
                self.fraction = self.fraction.wrapping_sub(WHOLE as i16);
                self.counter += 1;
            }
            let end = self.span.end as i32;
            if self.counter == end {
                stepped.at_end = true;
            } else if self.counter > end {
                stepped.looped = true;
                self.counter = self.span.loop_start as i32;
            }
        }
        if let Some(fade) = &mut self.fade {
            fade.weight += match fade.quick {
                true => QUICK_FADE,
                false => WHOLE / fade.length as i32,
            };
            if fade.weight >= WHOLE {
                self.fade = None;
            }
        }
        stepped
    }

    /// The frame of an animation of `frames` frames that a counter shows.
    pub fn frame_of(counter: i32, frames: usize) -> usize {
        (counter as u32 % frames.max(1) as u32) as usize
    }

    /// The frame of the playing animation, which has `frames` frames.
    pub fn frame(&self, frames: usize) -> usize {
        Self::frame_of(self.counter, frames)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_random_start_lies_between_the_first_frame_and_the_loop_start() {
        let span = Span {
            first: 2,
            loop_start: 12,
            end: 30,
        };
        assert_eq!([0, 7, 10, 23].map(|draw| span.random(draw)), [2, 9, 2, 5]);
        // A loop that starts on the first frame leaves that frame.
        let still = Span {
            loop_start: 2,
            ..span
        };
        assert_eq!(still.random(7), 2);
    }

    #[test]
    fn a_playback_runs_to_the_end_and_loops() {
        let span = Span {
            first: 2,
            loop_start: 4,
            end: 6,
        };
        let mut playback = Playback::new(3, span, WHOLE as i16);
        let mut seen = Vec::new();
        for _ in 0..8 {
            let stepped = playback.step();
            seen.push((playback.counter, stepped.at_end, stepped.looped));
        }
        assert_eq!(
            seen,
            [
                (3, false, false),
                (4, false, false),
                (5, false, false),
                (6, true, false),
                (4, false, true),
                (5, false, false),
                (6, true, false),
                (4, false, true),
            ]
        );
        assert_eq!(playback.frame(5), 4);
        assert_eq!(Playback::frame_of(7, 5), 2);
    }

    #[test]
    fn the_speed_is_parts_of_a_frame_and_a_pause_holds_the_counter() {
        let span = Span {
            first: 0,
            loop_start: 0,
            end: 100,
        };
        // Three quarters of a frame per game frame: the counter moves in three steps of four.
        let mut playback = Playback::new(0, span, 3072);
        let counters: Vec<i32> = (0..8)
            .map(|_| {
                playback.step();
                playback.counter
            })
            .collect();
        assert_eq!(counters, [0, 1, 2, 3, 3, 4, 5, 6]);
        playback.paused = true;
        playback.step();
        assert_eq!(playback.counter, 6);
    }

    #[test]
    fn a_started_animation_fades_the_old_one_out() {
        let span = Span {
            first: 1,
            loop_start: 3,
            end: 9,
        };
        let mut playback = Playback::new(0, span, WHOLE as i16);
        playback.step();
        playback.start(5, span, Start::LoopStart, Some(4));
        assert_eq!((playback.animation, playback.counter), (5, 3));
        let fade = playback.fade.unwrap();
        assert_eq!((fade.animation, fade.counter), (0, 2));
        assert_eq!(fade.old_share(), WHOLE);
        let shares: Vec<Option<i32>> = (0..4)
            .map(|_| {
                playback.step();
                playback.fade.map(|fade| fade.old_share())
            })
            .collect();
        assert_eq!(shares, [Some(3072), Some(2048), Some(1024), None]);
        // The old animation stood still all the while; the new one went on.
        assert_eq!(playback.counter, 7);

        // A quick fade takes fifteen game frames and a bit, whatever its length.
        playback.start(1, span, Start::At(12), Some(4));
        assert_eq!(playback.counter, 3);
        playback.fade.as_mut().unwrap().quick = true;
        let steps = (0..).take_while(|_| {
            playback.step();
            playback.fade.is_some()
        });
        assert_eq!(steps.count(), 15);

        playback.start(2, span, Start::First, None);
        assert_eq!((playback.counter, playback.fade), (1, None));
        playback.start(2, span, Start::First, Some(0));
        assert_eq!(playback.fade, None);
    }

    /// A block of `frames` frames and `bones` bones. Rest offset of bone b is (b, 10 b, -b);
    /// entry e of frame f is (100 f + e, f, e).
    fn block(frames: u32, bones: u32, extra: Option<u32>, events: Option<&[[i16; 4]]>) -> Vec<u8> {
        let mut data = Vec::new();
        let mark = |on: bool| if on { MARK } else { 0 };
        data.extend((frames | mark(extra.is_some())).to_le_bytes());
        data.extend((bones | mark(events.is_some())).to_le_bytes());
        if let Some(extra) = extra {
            data.extend(extra.to_le_bytes());
        }
        let mut put = |values: [i16; 3]| {
            for value in values {
                data.extend(value.to_le_bytes());
            }
        };
        for bone in 0..bones as i16 {
            put([bone, 10 * bone, -bone]);
        }
        for frame in 0..frames as i16 {
            for entry in 0..bones as i16 + 2 {
                put([100 * frame + entry, frame, entry]);
            }
        }
        data.resize(data.len().next_multiple_of(4), 0);
        if let Some(events) = events {
            data.extend((events.len() as u32).to_le_bytes());
            for value in events.iter().flatten() {
                data.extend(value.to_le_bytes());
            }
        }
        data
    }

    #[test]
    fn reads_rest_offsets_and_frames() {
        let data = block(3, 4, None, None);
        let animation = Animation::parse(&data).unwrap();
        assert_eq!((animation.frames, animation.bones), (3, 4));
        assert_eq!(animation.extra, None);
        assert!(animation.events.is_empty());
        assert_eq!(animation.len, data.len());
        assert_eq!(animation.rest(3), Some([3, 30, -3]));
        assert_eq!(animation.rest(4), None);
        // Bone 0 is the third entry of a frame.
        assert_eq!(animation.angles(2, 0), Some([202, 2, 2]));
        assert_eq!(animation.angles(2, 3), Some([205, 2, 5]));
        assert_eq!(animation.angles(2, 4), None);
        assert_eq!(animation.angles(3, 0), None);
        assert_eq!(animation.travel(1), Some([100, 1, 0]));
        assert_eq!(animation.root(1), Some([101, 1, 1]));
    }

    #[test]
    fn the_marks_add_a_word_and_events() {
        let events = [[7, 0, 22, 0], [21, 0, 21, 0]];
        let data = block(1, 3, Some(4), Some(&events));
        let animation = Animation::parse(&data).unwrap();
        assert_eq!(animation.extra, Some(4));
        assert_eq!(animation.rest(1), Some([1, 10, -1]));
        assert_eq!(animation.angles(0, 2), Some([4, 0, 4]));
        assert_eq!(animation.events.len(), 2);
        assert_eq!(animation.events[1].frame, 21);
        assert_eq!(animation.events[1].raw, events[1]);
        assert_eq!(animation.events[1].action(), EventAction::Effect(21));
        assert_eq!(animation.len, data.len());
        assert_eq!(data.len(), 12 + 18 + 30 + 4 + 16);

        // Seven entries behind the counts end on 50 bytes: the events start two bytes later.
        let data = block(2, 1, None, Some(&events[..1]));
        let animation = Animation::parse(&data).unwrap();
        assert_eq!(data.len(), 8 + 6 + 36 + 2 + 4 + 8);
        assert_eq!(animation.events[0].raw, events[0]);
        assert_eq!(animation.len, data.len());
    }

    #[test]
    fn an_event_is_an_effect_or_a_change_of_mesh() {
        let event = |raw: [i16; 4]| Event { frame: raw[0], raw }.action();
        assert_eq!(event([7, 0, 109, 0]), EventAction::Effect(109));
        assert_eq!(event([7, 0, 110, 0]), EventAction::Nothing);
        assert_eq!(event([7, 0, -1, 0]), EventAction::Nothing);
        assert_eq!(
            event([9, 1, 12, 22]),
            EventAction::Mesh {
                object: 12,
                source: 22
            }
        );
        assert_eq!(event([9, 2, 12, 22]), EventAction::Nothing);
    }

    #[test]
    fn bytes_behind_the_block_are_ignored_and_missing_ones_are_an_error() {
        let mut data = block(2, 2, None, None);
        let len = data.len();
        data.extend([0xFF; 8]);
        assert_eq!(Animation::parse(&data).unwrap().len, len);
        assert_eq!(
            Animation::parse(&data[..len - 1]),
            Err(AnimationError::Truncated(len))
        );
        assert_eq!(Animation::parse(&[0; 8]), Err(AnimationError::Empty));
        assert_eq!(Animation::parse(&[0; 4]), Err(AnimationError::Truncated(4)));
        // A count that no file could hold.
        let mut huge = data.clone();
        huge[..4].copy_from_slice(&0x7FFF_FFFFu32.to_le_bytes());
        assert!(matches!(
            Animation::parse(&huge),
            Err(AnimationError::Truncated(_))
        ));
    }

    #[test]
    fn a_resident_section_lists_its_blocks() {
        let (first, second) = (block(1, 2, None, None), block(2, 3, None, None));
        let mut section = Vec::new();
        section.extend(2u32.to_le_bytes());
        section.extend(12u32.to_le_bytes());
        section.extend((12 + first.len() as u32).to_le_bytes());
        section.extend(&first);
        section.extend(&second);
        section.resize(2048, 0);
        let animations = Animation::resident(&section).unwrap();
        assert_eq!(animations.len(), 2);
        assert_eq!((animations[1].frames, animations[1].bones), (2, 3));
        assert_eq!(Animation::resident(&[0; 2048]).unwrap(), []);
        section[4..8].copy_from_slice(&4096u32.to_le_bytes());
        assert_eq!(
            Animation::resident(&section),
            Err(AnimationError::BadOffset(4096))
        );
    }
}
