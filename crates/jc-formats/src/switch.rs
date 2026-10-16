
use crate::leave;
use crate::scene::ScenePackage;

/// The types of the two objects.
pub const KIND: u32 = 0x03;
pub const BARRIERS_KIND: u32 = 0x09;
/// The type of the picture that follows a story flag.
pub const FOLLOWER_KIND: u32 = 0x10;
/// The bytes of a switch's record and of one entry of a list of barriers.
const SIZE: usize = 24;
const ENTRY: usize = 8;
/// The bytes of a follower's record, and the value that stands for no sound in it.
const FOLLOWER_SIZE: usize = 16;
const NO_SOUND: u8 = 255;
/// The table value that ends a list of barriers.
const END: i16 = 2;
/// No list is longer than this; a list that does not end is cut here.
const LONGEST: usize = 16;

fn word(bytes: &[u8], at: usize) -> i32 {
    i32::from_le_bytes(bytes[at..at + 4].try_into().expect("4 bytes"))
}

/// The record of a switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Switch {
    /// RAM address of the record.
    pub record: u32,
    /// The views of the room in which the switch is active.
    pub view_mask: u32,
    /// The story flag a press sets.
    pub flag: i32,
    /// 1: a press of a switch whose flag is set clears the flag again.
    pub way: i32,
    /// The trigger region the player stands in to be at the switch, and the place the
    /// player then has to face.
    pub region: i32,
    pub place: [i32; 3],
}

impl Switch {
    /// The flag, the way, the region and the place of a record.
    pub fn parse(bytes: &[u8]) -> Option<(i32, i32, i32, [i32; 3])> {
        let bytes = bytes.get(..SIZE)?;
        let place = [word(bytes, 12), word(bytes, 16), word(bytes, 20)];
        Some((word(bytes, 0), word(bytes, 4), word(bytes, 8), place))
    }

    /// The switches in the object lists of a package, each once.
    pub fn of(package: &ScenePackage) -> Vec<Self> {
        let mut switches: Vec<Self> = Vec::new();
        let lists = package.object_lists();
        for object in (lists.iter().flat_map(|list| &list.objects))
            .filter(|object| object.kind == KIND && object.function != 0)
        {
            if switches.iter().any(|known| known.record == object.data) {
                continue;
            }
            let read = package.bytes_at(object.data, SIZE).and_then(Self::parse);
            switches.extend(read.map(|(flag, way, region, place)| Self {
                record: object.data,
                view_mask: object.view_mask,
                flag,
                way,
                region,
                place,
            }));
        }
        switches
    }
}

/// What a press did to the story flag of a switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Turned {
    Set,
    Cleared,
}

/// The words of a switch's task that its routine works on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pressed {
    /// The state a script or the confirm button told it (+124 of the task), -1 for
    /// none.
    pub told: i32,
    /// The word of the task's extra bytes: the flag as the routine last left it.
    pub on: bool,
    /// The mark (+120): the player stands at the switch.
    pub marked: bool,
    /// The task has ended (told state -2).
    pub ended: bool,
}

impl Pressed {
    /// The task behind its start routine, which reads the story flag.
    pub fn new(flag_set: bool) -> Self {
        Self {
            told: -1,
            on: flag_set,
            marked: false,
            ended: false,
        }
    }

    /// A script tells the switch a state (`0x2E`).
    pub fn tell(&mut self, state: i32) {
        self.told = state;
    }

    /// Whether a press of the confirm button would tell the object state 0, asked
    /// without telling it: the test of [`Self::press`] (the build's hint of what the
    /// press would do, `interaction_hint`).
    pub fn would_press(&self) -> bool {
        self.marked
    }

    /// The second half of the pad routine's call for a talk (`0x800A5BD0`): a marked
    /// object is told state 0. Answers whether this one was.
    pub fn press(&mut self) -> bool {
        let marked = self.would_press();
        if marked {
            self.told = 0;
        }
        marked
    }

    /// One game frame of the task: the wrapper of an object task, then the routine.
    /// `at`: the player stands in the region and faces the place. Answers what is to
    /// be done with the story flag.
    pub fn pass(&mut self, switch: &Switch, at: bool) -> Option<Turned> {
        self.marked = false;
        if self.ended {
            return None;
        }
        if !leave::runs(self.told) {
            self.ended = self.told == leave::ENDS;
            return None;
        }
        self.marked = at;
        if self.told != 0 {
            return None;
        }
        self.told = -1;
        if self.on && switch.way == 1 {
            self.on = false;
            Some(Turned::Cleared)
        } else {
            self.on = true;
            Some(Turned::Set)
        }
    }
}

/// One barrier of a list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Barrier {
    /// The story flag that takes the barrier away.
    pub flag: i32,
    /// The barrier is one of table 4, the second mover's; of table 2, the player's,
    /// otherwise.
    pub second: bool,
    /// The region of that table.
    pub region: i16,
}

impl Barrier {
    /// The byte the routine writes for the barrier: 1, it stands, while the flag is not
    /// set.
    pub fn byte(&self, flag_set: bool) -> u8 {
        u8::from(!flag_set)
    }
}

/// The record of an object of type `0x09`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Barriers {
    /// RAM address of the record.
    pub record: u32,
    /// The views of the room in which the object is active.
    pub view_mask: u32,
    pub barriers: Vec<Barrier>,
}

impl Barriers {
    /// The entries of a record up to the one that ends it; none when the bytes end
    /// before it.
    pub fn parse(bytes: &[u8]) -> Option<Vec<Barrier>> {
        let mut barriers = Vec::new();
        for entry in bytes.chunks(ENTRY).take(LONGEST + 1) {
            let entry: &[u8; ENTRY] = entry.try_into().ok()?;
            let table = i16::from_le_bytes([entry[4], entry[5]]);
            if table == END {
                return Some(barriers);
            }
            barriers.push(Barrier {
                flag: word(entry, 0),
                second: table != 0,
                region: i16::from_le_bytes([entry[6], entry[7]]),
            });
        }
        None
    }

    /// The lists of barriers in the object lists of a package, each once.
    pub fn of(package: &ScenePackage) -> Vec<Self> {
        let mut found: Vec<Self> = Vec::new();
        let lists = package.object_lists();
        for object in (lists.iter().flat_map(|list| &list.objects))
            .filter(|object| object.kind == BARRIERS_KIND && object.function != 0)
        {
            if found.iter().any(|known| known.record == object.data) {
                continue;
            }
            // As many entries as the package holds behind the address, up to the longest.
            let read = (1..=LONGEST + 1)
                .rev()
                .find_map(|entries| package.bytes_at(object.data, entries * ENTRY))
                .and_then(Self::parse);
            found.extend(read.map(|barriers| Self {
                record: object.data,
                view_mask: object.view_mask,
                barriers,
            }));
        }
        found
    }
}

/// The frames of a follower for one state of its flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frames {
    /// The frame the state starts on and the one it comes to rest on.
    pub first: u8,
    pub rest: u8,
    /// At rest the frames go round: from the rest frame back to the first.
    pub round: bool,
    /// A sound for the first round after a change, if any.
    pub sound: Option<u8>,
}

/// The record of an object of type `0x10`: a picture that follows a story flag. The
/// picture is an animated group of the view's background, which the object holds on a
/// frame instead of letting it go round.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Follower {
    /// RAM address of the record.
    pub record: u32,
    /// The views of the room in which the object is active.
    pub view_mask: u32,
    /// The story flag the picture follows.
    pub flag: i32,
    /// The number that names the group (`background::group_named`).
    pub number: i16,
    /// The sound of a change, if any.
    pub sound: Option<i16>,
    /// The frames without the flag and with it.
    pub states: [Frames; 2],
}

impl Follower {
    /// The flag, the number, the sound and the frames of the two states of a record.
    pub fn parse(bytes: &[u8]) -> Option<(i32, i16, Option<i16>, [Frames; 2])> {
        let bytes = bytes.get(..FOLLOWER_SIZE)?;
        let half = |at: usize| i16::from_le_bytes([bytes[at], bytes[at + 1]]);
        let frames = |at: usize| Frames {
            first: bytes[at],
            rest: bytes[at + 1],
            round: bytes[at + 2] != 0,
            sound: Some(bytes[at + 3]).filter(|sound| *sound != NO_SOUND),
        };
        let sound = Some(half(6)).filter(|sound| *sound != i16::from(NO_SOUND));
        Some((word(bytes, 0), half(4), sound, [frames(8), frames(12)]))
    }

    /// The followers in the object lists of a package, each once.
    pub fn of(package: &ScenePackage) -> Vec<Self> {
        let mut found: Vec<Self> = Vec::new();
        let lists = package.object_lists();
        for object in (lists.iter().flat_map(|list| &list.objects))
            .filter(|object| object.kind == FOLLOWER_KIND && object.function != 0)
        {
            if found.iter().any(|known| known.record == object.data) {
                continue;
            }
            let read = (package.bytes_at(object.data, FOLLOWER_SIZE)).and_then(Self::parse);
            found.extend(read.map(|(flag, number, sound, states)| Self {
                record: object.data,
                view_mask: object.view_mask,
                flag,
                number,
                sound,
                states,
            }));
        }
        found
    }
}

/// What a game frame of a follower asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Followed {
    /// The frame the group is put on, which starts the frame's time over.
    pub frame: u8,
    /// A sound to play.
    pub sound: Option<i16>,
}

/// The words of a follower's task that its routine works on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Following {
    /// The state the picture shows or is on its way to.
    pub on: bool,
    /// The frame the group was last put on.
    pub frame: u8,
    /// A change has played its sound and the sound of the round is still to come.
    sounded: bool,
}

impl Following {
    /// The task behind its start routine (`0x800B2E64`), which reads the story flag: the
    /// picture rests in the flag's state. The routine puts the group on [`Self::frame`]
    /// and runs a first game frame at once.
    pub fn new(follower: &Follower, flag_set: bool) -> Self {
        Self {
            on: flag_set,
            frame: follower.states[usize::from(flag_set)].rest,
            sounded: false,
        }
    }

    /// One game frame (`0x800B26C4`). `on`: the story flag is set, or the task was told
    /// state 0. `through`: the group's frame is through
    /// (`background::GroupClock::through` with [`Self::frame`]); until then nothing
    /// happens. A frame on its way goes on by one; at rest a change of `on` starts the
    /// frames of the other state, and a state whose frames go round starts over.
    pub fn pass(&mut self, follower: &Follower, on: bool, through: bool) -> Option<Followed> {
        if !through {
            return None;
        }
        let state = follower.states[usize::from(self.on)];
        let mut sound = None;
        if self.frame != state.rest {
            self.frame = self.frame.wrapping_add(1);
        } else if self.on != on {
            sound = follower.sound;
            self.sounded |= sound.is_some();
            self.on = on;
            self.frame = follower.states[usize::from(on)].first;
        } else if state.round {
            if self.sounded {
                sound = state.sound.map(i16::from);
                self.sounded &= sound.is_none();
            }
            self.frame = state.first;
        }
        Some(Followed {
            frame: self.frame,
            sound,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The follower of room 107's shape: at rest on frame 0 without the flag, the frames
    /// 1 to 7 with it.
    fn follower(round: bool) -> Follower {
        let frames = |first, rest, round, sound| Frames {
            first,
            rest,
            round,
            sound,
        };
        Follower {
            record: 0x800B_39B4,
            view_mask: 1,
            flag: 108,
            number: 200,
            sound: Some(23),
            states: [frames(0, 0, false, None), frames(1, 7, round, Some(25))],
        }
    }

    #[test]
    fn a_record_of_a_follower_reads_its_frames() {
        let mut bytes = 108i32.to_le_bytes().to_vec();
        bytes.extend(200i16.to_le_bytes());
        bytes.extend(23i16.to_le_bytes());
        bytes.extend([0, 0, 0, 255, 1, 7, 1, 25]);
        let (flag, number, sound, states) = Follower::parse(&bytes).expect("a record");
        assert_eq!((flag, number, sound), (108, 200, Some(23)));
        assert_eq!(states, follower(true).states);
        bytes[6] = 255;
        assert_eq!(Follower::parse(&bytes).expect("a record").2, None);
        assert_eq!(Follower::parse(&bytes[..15]), None);
    }

    #[test]
    fn a_follower_rests_until_its_flag_changes_and_plays_the_way_once() {
        let follower = follower(false);
        let mut following = Following::new(&follower, false);
        assert_eq!((following.on, following.frame), (false, 0));
        // Nothing while the frame is not through; at rest the frame is put again.
        assert_eq!(following.pass(&follower, false, false), None);
        let rests = Followed {
            frame: 0,
            sound: None,
        };
        assert_eq!(following.pass(&follower, false, true), Some(rests));
        // The flag is set: the sound of the change, then a frame at a time to the rest.
        let frames: Vec<Followed> = (0..9)
            .filter_map(|_| following.pass(&follower, true, true))
            .collect();
        assert_eq!(frames[0].sound, Some(23));
        assert!(frames[1..].iter().all(|followed| followed.sound.is_none()));
        let shown: Vec<u8> = frames.iter().map(|followed| followed.frame).collect();
        assert_eq!(shown, [1, 2, 3, 4, 5, 6, 7, 7, 7]);
        // A room entered with the flag set starts at rest in that state.
        assert_eq!(Following::new(&follower, true).frame, 7);
    }

    #[test]
    fn a_state_that_goes_round_starts_over_and_sounds_once() {
        let follower = follower(true);
        let mut following = Following::new(&follower, false);
        let frames: Vec<Followed> = (0..16)
            .filter_map(|_| following.pass(&follower, true, true))
            .collect();
        let shown: Vec<u8> = frames.iter().map(|followed| followed.frame).collect();
        assert_eq!(shown, [1, 2, 3, 4, 5, 6, 7, 1, 2, 3, 4, 5, 6, 7, 1, 2]);
        // The sound of the round comes with the first round only.
        let sounds: Vec<Option<i16>> = frames.iter().map(|followed| followed.sound).collect();
        assert_eq!(sounds[0], Some(23));
        assert_eq!(sounds[7], Some(25));
        assert_eq!(sounds.iter().flatten().count(), 2);
    }

    fn switch(way: i32) -> Switch {
        Switch {
            record: 0x800B_399C,
            view_mask: u32::MAX,
            flag: 108,
            way,
            region: 1,
            place: [1674, 0, -3655],
        }
    }

    #[test]
    fn a_record_of_a_switch_reads_its_words() {
        let mut bytes = Vec::new();
        for word in [108i32, 0, 1, 1674, 0, -3655] {
            bytes.extend(word.to_le_bytes());
        }
        assert_eq!(Switch::parse(&bytes), Some((108, 0, 1, [1674, 0, -3655])));
        assert_eq!(Switch::parse(&bytes[..23]), None);
    }

    #[test]
    fn a_press_sets_the_flag_of_a_marked_switch() {
        let (switch, mut pressed) = (switch(0), Pressed::new(false));
        assert_eq!(pressed.pass(&switch, false), None);
        assert!(!pressed.press());
        assert_eq!(pressed.pass(&switch, true), None);
        assert!(pressed.marked && pressed.press());
        assert_eq!(pressed.pass(&switch, true), Some(Turned::Set));
        assert!(pressed.on);
        assert_eq!(pressed.told, -1);
        assert_eq!(pressed.pass(&switch, true), None);
        // A second press sets it once more: this way never clears.
        assert!(pressed.press());
        assert_eq!(pressed.pass(&switch, true), Some(Turned::Set));
    }

    #[test]
    fn a_switch_of_way_one_goes_both_ways() {
        let (switch, mut pressed) = (switch(1), Pressed::new(true));
        pressed.pass(&switch, true);
        pressed.press();
        assert_eq!(pressed.pass(&switch, true), Some(Turned::Cleared));
        assert!(!pressed.on);
        pressed.press();
        assert_eq!(pressed.pass(&switch, true), Some(Turned::Set));
    }

    #[test]
    fn a_resting_switch_is_not_marked_and_an_ended_one_stays_ended() {
        let (switch, mut pressed) = (switch(0), Pressed::new(false));
        pressed.tell(leave::RESTS);
        assert_eq!(pressed.pass(&switch, true), None);
        assert!(!pressed.marked && !pressed.press());
        pressed.tell(leave::ENDS);
        pressed.pass(&switch, true);
        assert!(pressed.ended);
        pressed.tell(0);
        assert_eq!(pressed.pass(&switch, true), None);
    }

    #[test]
    fn a_list_of_barriers_ends_at_the_table_value_two() {
        let mut bytes = Vec::new();
        for (flag, table, region) in [(109i32, 0i16, 0i16), (109, 1, 0), (110, 0, 1), (0, 2, 0)] {
            bytes.extend(flag.to_le_bytes());
            bytes.extend(table.to_le_bytes());
            bytes.extend(region.to_le_bytes());
        }
        let barriers = Barriers::parse(&bytes).expect("a list");
        assert_eq!(barriers.len(), 3);
        assert_eq!(
            barriers[1],
            Barrier {
                flag: 109,
                second: true,
                region: 0
            }
        );
        assert_eq!((barriers[2].flag, barriers[2].region), (110, 1));
        // Bytes that end before the list does are no list.
        assert_eq!(Barriers::parse(&bytes[..24]), None);
        assert_eq!(Barriers::parse(&bytes[24..]), Some(Vec::new()));
        // The barrier stands while its flag is not set.
        assert_eq!((barriers[0].byte(false), barriers[0].byte(true)), (1, 0));
    }
}
