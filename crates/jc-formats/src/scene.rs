
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::battle;
use crate::battle::fight::{Block, Spec};
use crate::battle::lesson;
use crate::items::{Keeping, Kind, Worn};
use crate::message::{self, Group};
use crate::skeleton::BoneScale;

/// RAM address that offset 0 of a scene package file corresponds to. The code image starts
/// four bytes later, after the size word.
pub const PACKAGE_BASE: u32 = 0x800B_248C;
/// Bytes of the size word in front of the code image.
const SIZE_WORD: usize = 4;
/// Bytes of one object record.
const OBJECT_SIZE: usize = 20;
/// Bytes of one exit record.
const EXIT_SIZE: usize = 24;
/// Object type that ends an object list.
pub const OBJECT_END: u32 = 0x12;
/// Object type of an exit.
pub const OBJECT_EXIT: u32 = 0x01;
/// Object type of a script.
pub const OBJECT_SCRIPT: u32 = 0x08;
/// Bytes of one script command.
const COMMAND_SIZE: usize = 16;
/// Number of script commands the interpreter has a handler for: codes 0 to `0x68`.
pub const COMMAND_COUNT: u32 = 105;
/// Command that ends a script. It also closes a segment: the label search of
/// [`COMMAND_GOTO`] stops at it.
pub const COMMAND_END: u32 = 0x03;
/// Command that does nothing; its `b` is a label number for [`COMMAND_GOTO`].
pub const COMMAND_LABEL: u32 = 0x09;
/// Command that goes to the label `b` of the current segment.
pub const COMMAND_GOTO: u32 = 0x0A;
/// Command that goes to the RAM address `c`, where another segment starts.
pub const COMMAND_JUMP: u32 = 0x0B;
/// Object type whose data field is a small number instead of a pointer.
pub const OBJECT_NUMBERED: u32 = 0x0D;
/// The data number of an [`OBJECT_NUMBERED`] record is below this.
const MAX_NUMBER: u32 = 0x100;
/// An object record with more extra bytes than this is not taken for one.
const MAX_EXTRA: u32 = 0x1000;
/// Arrival angle that means "keep the player's angles".
pub const KEEP_ANGLES: i16 = 0x4000;

/// Why a file is not a scene package.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneError {
    /// The file is shorter than its size word, or than the code image the word announces.
    Truncated,
}

impl fmt::Display for SceneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => write!(f, "scene package is shorter than its code image"),
        }
    }
}

impl std::error::Error for SceneError {}

/// One record of an object list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectRecord {
    /// RAM address of the record itself.
    pub address: u32,
    /// The views of the room the object belongs to, one bit each.
    pub view_mask: u32,
    /// Object type, below [`OBJECT_END`].
    pub kind: u32,
    /// RAM address of the object's data, 0 for none. What it points at depends on the type.
    pub data: u32,
    /// Extra bytes the object's task gets.
    pub extra: u32,
    /// RAM address of the object's function, 0 for none.
    pub function: u32,
}

/// An object list: the records up to, not including, the end record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectList {
    /// RAM address of the first record.
    pub address: u32,
    pub objects: Vec<ObjectRecord>,
}

impl ObjectList {
    /// Whether a script object of the list names the script at an address.
    pub fn has_script(&self, address: u32) -> bool {
        (self.objects.iter()).any(|object| object.kind == OBJECT_SCRIPT && object.data == address)
    }

    /// Whether an actor object of the list names the instance at an address.
    pub fn has_actor(&self, instance: u32) -> bool {
        (self.objects.iter()).any(|object| object.kind == OBJECT_ACTOR && object.data == instance)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListIndex {
    /// A story variable (`0x800AD324`).
    StoryVariable(u32),
    /// A bit of the global flags `0x8008D608`: set picks the second list.
    GlobalFlag(u32),
    /// A story flag (`0x800AD2A4`): set picks the second list.
    StoryFlag(u32),
}

impl ListIndex {
    /// The number of the list the entry routine picks: a story variable as it is, a flag
    /// as 0 or 1. `variable` and `flag` answer for the story variables and story flags
    /// of the running game, `global_flags` is the word `0x8008D608`.
    pub fn pick(
        self,
        variable: impl Fn(u32) -> i32,
        global_flags: u32,
        flag: impl Fn(u32) -> bool,
    ) -> usize {
        match self {
            // A story variable is a byte (`lbu` in `0x800AD324`).
            Self::StoryVariable(number) => (variable(number) & 0xFF) as usize,
            Self::GlobalFlag(bit) => usize::from(global_flags & bit != 0),
            Self::StoryFlag(number) => usize::from(flag(number)),
        }
    }
}

/// The index of the lists of an archive file that is a package with several object lists,
/// `None` for every other file. The index counts the lists in file order, which is the
/// order of [`ScenePackage::object_lists`]: where the entry routine's code names them
/// (files 111, 189 and 328) and in the four tables of pointers alike.
pub fn list_index(file: usize) -> Option<ListIndex> {
    Some(match file {
        ROOM_0_PACKAGE => ListIndex::StoryVariable(0),
        189 => ListIndex::GlobalFlag(0x4000_0000),
        328 => ListIndex::StoryFlag(118),
        422 | 433 | 467 | 470 => ListIndex::StoryVariable(11),
        _ => return None,
    })
}

/// The package of room 0, which holds two scenes as two object lists.
pub const ROOM_0_PACKAGE: usize = 111;
pub const ROOM_0_AGAIN: u32 = 0x800B_2B28;

/// The native routine of a package with several lists that a script ends a scene with
/// (`0x4D` behind a `0x4C`), as (routine, story variable): it writes the first word of
/// the argument record to the story variable its entry routine picks the list with and
/// the second to the force flag of the room change, so that the room is read again with
/// the next list. Room 0's package and the four of area 5 have one; in files 189 and
/// 328 a script writes the flag and nothing reads the room again.
pub fn again_routine(file: usize) -> Option<(u32, u32)> {
    Some(match file {
        ROOM_0_PACKAGE => (ROOM_0_AGAIN, 0),
        422 => (0x800B_2FC4, 11),
        433 => (0x800B_34CC, 11),
        467 => (0x800B_2F50, 11),
        470 => (0x800B_3128, 11),
        _ => return None,
    })
}

/// The view the entry routine of a package with several lists asks for (the word
/// `0x80091C14`) when it picks list `index`: `None` where it asks for none, and the
/// room's view is the one the player stands in. File 189 asks for the list's number,
/// files 433, 467 and 470 read a table of words of their own (-1 asks for none), files
/// 111, 328 and 422 ask for nothing. `package` is the parsed file.
pub fn list_view(file: usize, index: usize, package: &ScenePackage) -> Option<u16> {
    let (table, lists) = match file {
        189 => return u16::try_from(index).ok().filter(|&view| view < 2),
        433 => (0x800B_5868, 4),
        467 => (0x800B_24FC, 4),
        470 => (0x800B_24E8, 3),
        _ => return None,
    };
    if index >= lists {
        return None;
    }
    let bytes = package.bytes_at(table + 4 * index as u32, 4)?;
    let view = i32::from_le_bytes(bytes.try_into().ok()?);
    u16::try_from(view).ok()
}

/// The set of the player that the entry routine of a package with several lists keeps
/// for list `index` (the word `0x80091C04`, loaded when the room is in): room 0's
/// package keeps set 3 for its first scene and set 4 for its second.
pub fn list_player_set(file: usize, index: usize) -> Option<u16> {
    match (file, index) {
        (ROOM_0_PACKAGE, 0) => Some(3),
        (ROOM_0_PACKAGE, 1) => Some(4),
        _ => None,
    }
}

/// An exit of a room: where the player has to stand and where that leads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exit {
    /// RAM address of the exit record, by which a script names the exit (`0x2E`).
    pub record: u32,
    /// The views of the room in which the exit is active.
    pub view_mask: u32,
    /// Position the player arrives at in the target room.
    pub arrival: [i32; 3],
    /// Arrival angle; [`KEEP_ANGLES`] keeps the player's angles.
    pub angle: i16,
    /// Non-zero makes the player stand again on the way out ([`Exit::stands_again`]).
    pub leave_flag: i16,
    /// Number of the trigger region of this room that sets the exit off.
    pub region: i16,
    /// Target area.
    pub area: i16,
    /// Target place.
    pub place: i16,
    /// Target room slot (not a record index, see `rooms::RoomTable::by_slot`).
    pub slot: i16,
}

impl Exit {
    pub fn facing(&self, angle: i16) -> i16 {
        match self.angle {
            KEEP_ANGLES => angle,
            turn => angle.wrapping_add(turn),
        }
    }

    /// The exit routine calls the player's "stand again" (`0x800A8ECC`) on the way out.
    pub fn stands_again(&self) -> bool {
        self.leave_flag != 0
    }
}

/// One 16-byte command of a script. What the three arguments mean depends on the command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Command {
    /// RAM address of the command.
    pub address: u32,
    /// Command number, below [`COMMAND_COUNT`].
    pub code: u32,
    pub a: i32,
    pub b: u32,
    pub c: u32,
}

/// The commands that deal with where the player is, in typed form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocationCommand {
    /// `0x4F`: start a chapter, by way of the chapter screen.
    StartChapter { chapter: u32 },
    SetStateWord { value: u32 },
    /// `0x51`: make a slot lead to a room. `room` is a room number; -1 closes the entry.
    SetVariant {
        area: i32,
        place: u16,
        slot: u16,
        room: i32,
    },
    /// `0x52`: go to a slot. `arrival` is the RAM address of an arrival position (three words
    /// and an angle), `None` for the room's default position.
    GoToSlot {
        area: i32,
        place: u16,
        slot: u16,
        arrival: Option<u32>,
    },
    /// `0x53`: go to the room with this room number.
    GoToRoom { room: u32 },
    /// `0x58`: open the place map.
    OpenPlaceMap,
}

impl Command {
    /// The typed form of a location command, `None` for every other command.
    pub fn location(&self) -> Option<LocationCommand> {
        let (place, slot) = ((self.b >> 16) as u16, self.b as u16);
        Some(match self.code {
            0x4F => LocationCommand::StartChapter { chapter: self.b },
            0x50 => LocationCommand::SetStateWord { value: self.b },
            0x51 => LocationCommand::SetVariant {
                area: self.a,
                place,
                slot,
                room: self.c as i32,
            },
            0x52 => LocationCommand::GoToSlot {
                area: self.a,
                place,
                slot,
                arrival: (self.c != 0).then_some(self.c),
            },
            0x53 => LocationCommand::GoToRoom { room: self.b },
            0x58 => LocationCommand::OpenPlaceMap,
            _ => return None,
        })
    }

    /// The typed form of a command that deals with what the player has, `None` for every
    /// other command.
    pub fn keeping(&self) -> Option<KeepingCommand> {
        let (number, count) = (self.a, self.b as i32);
        Some(match self.code {
            0x1F if self.a == 0 => KeepingCommand::WaitForFlag { flag: self.b },
            0x1F => KeepingCommand::AskFlag { flag: self.b },
            0x20 => KeepingCommand::SetFlag { flag: self.b },
            0x21 => KeepingCommand::ClearFlag { flag: self.b },
            0x25 | 0x27 => KeepingCommand::Give {
                kind: [Kind::Item, Kind::Weapon][usize::from(self.code == 0x27)],
                number,
                count,
            },
            0x26 | 0x28 => KeepingCommand::Take {
                kind: [Kind::Item, Kind::Weapon][usize::from(self.code == 0x28)],
                number,
                count,
            },
            0x29 if self.a == 0 => KeepingCommand::StoryThingUnread { value: self.b },
            0x29 => KeepingCommand::AskStoryThing { number: count },
            0x2A => KeepingCommand::GiveMoney { amount: count },
            0x2B => KeepingCommand::GiveCocoons { count },
            0x2D if self.a == 0 => KeepingCommand::TakeStoryThing { number: count },
            0x2D => KeepingCommand::GiveStoryThing { number: count },
            _ => return None,
        })
    }

    /// The typed form of a command that deals with the bits of the room, `None` for
    /// every other command.
    pub fn room_bits(&self) -> Option<RoomBits> {
        Some(match self.code {
            0x22 if self.a == 0 => RoomBits::WaitFor { bits: self.b },
            0x22 => RoomBits::Ask { bits: self.b },
            0x23 => RoomBits::Set { bits: self.b },
            0x24 => RoomBits::Clear { bits: self.b },
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomBits {
    /// `0x22` with `a` of 0: the script waits until every bit of the mask is set.
    WaitFor { bits: u32 },
    /// `0x22`: the task's choice becomes 0 when every bit of the mask is set, else 1.
    Ask { bits: u32 },
    /// `0x23`.
    Set { bits: u32 },
    /// `0x24`.
    Clear { bits: u32 },
}

impl RoomBits {
    /// Whether every bit of a mask is set in the word.
    pub fn all_set(word: u32, bits: u32) -> bool {
        !word & bits == 0
    }

    /// Does to the word what the command's handler does, and answers the choice of the
    /// task when the command sets it. The waiting form changes nothing.
    pub fn run(&self, word: &mut u32) -> Option<u32> {
        match *self {
            Self::WaitFor { .. } => {}
            Self::Ask { bits } => return Some(u32::from(!Self::all_set(*word, bits))),
            Self::Set { bits } => *word |= bits,
            Self::Clear { bits } => *word &= !bits,
        }
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeepingCommand {
    /// `0x1F` with `a` of 0: the script waits until the flag is set.
    WaitForFlag { flag: u32 },
    /// `0x1F`: the task's choice becomes 1 when the flag is clear, 0 when it is set.
    AskFlag { flag: u32 },
    /// `0x20`.
    SetFlag { flag: u32 },
    /// `0x21`.
    ClearFlag { flag: u32 },
    /// `0x25` an item, `0x27` a weapon.
    Give { kind: Kind, number: i32, count: i32 },
    /// `0x26` an item, `0x28` a weapon.
    Take { kind: Kind, number: i32, count: i32 },
    /// `0x29`: the task's choice becomes 1 when the player lacks the thing of the story, 0
    /// when it is there.
    AskStoryThing { number: i32 },
    /// `0x29` with `a` of 0, which was not read: it keeps `b` in the task and waits.
    StoryThingUnread { value: u32 },
    /// `0x2A`.
    GiveMoney { amount: i32 },
    /// `0x2B`.
    GiveCocoons { count: i32 },
    /// `0x2D`.
    GiveStoryThing { number: i32 },
    /// `0x2D` with `a` of 0.
    TakeStoryThing { number: i32 },
}

impl KeepingCommand {
    /// Does to the player's things what the command's handler does, and answers the
    /// choice of the task when the command sets it. A waiting form changes nothing.
    pub fn run(&self, keeping: &mut Keeping, worn: Worn) -> Option<u32> {
        match *self {
            Self::AskFlag { flag } => return Some(u32::from(!keeping.flag(flag))),
            Self::AskStoryThing { number } => {
                return Some(u32::from(!keeping.has_story_thing(number)));
            }
            Self::SetFlag { flag } => keeping.set_flag(flag),
            Self::ClearFlag { flag } => keeping.clear_flag(flag),
            Self::Give {
                kind,
                number,
                count,
            } => {
                keeping.give(number, count, kind, worn);
            }
            Self::Take {
                kind,
                number,
                count,
            } => {
                keeping.take(number, count, kind);
            }
            Self::GiveMoney { amount } => {
                keeping.give_money(amount);
            }
            Self::GiveCocoons { count } => {
                keeping.give_cocoons(count);
            }
            Self::GiveStoryThing { number } => {
                keeping.give_story_thing(number);
            }
            Self::TakeStoryThing { number } => keeping.take_story_thing(number),
            Self::WaitForFlag { .. } | Self::StoryThingUnread { .. } => {}
        }
        None
    }
}

/// One thing of a notice: the notice's number of its kind ([`Kind::of`]), its number and
/// how many.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoticeThing {
    pub kind: i32,
    pub number: i32,
    pub count: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoticeCommand {
    /// The way, -1 to 11: what the notice says was done with the things.
    pub way: i32,
    /// Up to three things; `None` when they are those of the thing the script's task
    /// belongs to, which the command does not name.
    pub things: Option<Vec<NoticeThing>>,
}

/// More lines than a list of a notice's lines has (the lines of the things to be
/// searched are one to three).
const NOTICE_LINES_MOST: u32 = 8;
/// Bytes of one thing in the list a notice command names.
const NOTICE_THING_SIZE: usize = 8;
/// Things a notice has room for.
const NOTICE_THINGS: usize = 3;
/// The way of a notice that is only lines of text.
const NOTICE_LINES_ONLY: i32 = 11;

/// A script of an object list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Script {
    /// RAM address of the first command.
    pub address: u32,
    /// The views of the room in which the script's object exists.
    pub view_mask: u32,
    /// The script's segments one after the other: the one at `address` first, then every
    /// segment a [`COMMAND_JUMP`] leads to, in the order the jumps are met.
    pub commands: Vec<Command>,
}

/// The view mask of an object that is in every view of its room.
pub const EVERY_VIEW: u32 = u32::MAX;

pub fn view_ends(view_mask: u32, view: u32) -> bool {
    view_mask != EVERY_VIEW && view_mask & 1u32.wrapping_shl(view) == 0
}

/// Whether a view that comes on screen makes an object with this view mask (the maker
/// `0x800B3CB0`): one that is in the new view and was not in the view `before`, none at
/// the room's start.
pub fn view_makes(view_mask: u32, view: u32, before: Option<u32>) -> bool {
    before.is_none_or(|before| view_mask & 1u32.wrapping_shl(before) == 0)
        && view_mask & 1u32.wrapping_shl(view) != 0
}

pub fn asked(mask: u32, bits: u32) -> i32 {
    let mut tested = 1;
    while tested < u32::BITS && mask >> tested & 1 != 0 {
        tested += 1;
    }
    let all = 1u32.checked_shl(tested).map_or(u32::MAX, |next| next - 1);
    match (bits & all).count_ones() {
        0 => 2,
        set => i32::from(set != tested),
    }
}

/// A scene package file, split into its two parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScenePackage<'a> {
    /// The size word and the code image: offset `o` of it is RAM address
    /// [`PACKAGE_BASE`]` + o`.
    image: &'a [u8],
    /// Everything after the code image.
    room_data: &'a [u8],
}

impl<'a> ScenePackage<'a> {
    /// Splits a scene package file. The first word is the size of the code image.
    pub fn parse(file: &'a [u8]) -> Result<Self, SceneError> {
        let size = file
            .get(..SIZE_WORD)
            .map(|word| u32::from_le_bytes(word.try_into().expect("four bytes")) as usize)
            .ok_or(SceneError::Truncated)?;
        let end = SIZE_WORD
            .checked_add(size)
            .filter(|end| *end <= file.len())
            .ok_or(SceneError::Truncated)?;
        Ok(Self {
            image: &file[..end],
            room_data: &file[end..],
        })
    }

    /// The code image, which the game copies to [`Self::code_address`].
    pub fn code(&self) -> &'a [u8] {
        &self.image[SIZE_WORD..]
    }

    /// RAM address of the first byte of the code image.
    pub fn code_address(&self) -> u32 {
        PACKAGE_BASE + SIZE_WORD as u32
    }

    /// The room data that follows the code image: resource blocks, views, regions.
    pub fn room_data(&self) -> &'a [u8] {
        self.room_data
    }

    /// The bytes at a RAM address, `None` unless all of them lie in the code image.
    pub fn bytes_at(&self, address: u32, len: usize) -> Option<&'a [u8]> {
        let start = address.checked_sub(PACKAGE_BASE)? as usize;
        if start < SIZE_WORD {
            return None;
        }
        self.image.get(start..)?.get(..len)
    }

    pub fn said_at(&self, list: u32, question: bool) -> Option<Group> {
        message::listed(list, question, |address| {
            let start = address.checked_sub(PACKAGE_BASE)? as usize;
            if start < SIZE_WORD {
                return None;
            }
            self.image.get(start..)
        })
    }

    pub fn fight_things(&self, record: u32) -> Option<[Option<u32>; FIGHT_ENTRIES]> {
        let words = self.bytes_at(record.checked_add(FIGHT_THINGS)?, 4 * FIGHT_ENTRIES)?;
        let mut things = [None; FIGHT_ENTRIES];
        for (thing, word) in things.iter_mut().zip(words.as_chunks::<4>().0) {
            let number = u32::from_le_bytes(*word);
            *thing = (number != FIGHT_NO_THING).then_some(number);
        }
        Some(things)
    }

    pub fn fight_record(&self, record: u32) -> Option<FightRecord> {
        let bytes = self.bytes_at(record, 4 * FIGHT_WORDS)?;
        let words: Vec<u32> = (bytes.as_chunks::<4>().0.iter())
            .map(|word| u32::from_le_bytes(*word))
            .collect();
        let entries = (words[9..].as_chunks::<3>().0.iter())
            .take_while(|entry| entry[0] < battle::EMPTY)
            .map(|&[species, experience, ai]| Spec {
                species,
                experience,
                ai,
            });
        Some(FightRecord {
            block: Block {
                mode: words[0],
                on_field: words[1] as usize,
                retreat: words[2] != 0,
                results: words[3] != 0,
                specs: entries.collect(),
            },
            loss_is_final: words[5] != 0,
        })
    }

    pub fn lesson(&self, number: u32) -> Option<Vec<lesson::Step>> {
        let entry = self.bytes_at(LESSON_TABLE.checked_add(number.checked_mul(4)?)?, 4)?;
        let list = u32::from_le_bytes(entry.try_into().ok()?);
        let mut steps = Vec::new();
        loop {
            let at = list.checked_add((steps.len() * lesson::STEP) as u32)?;
            let bytes = self.bytes_at(at, lesson::STEP)?;
            match lesson::Step::parse(bytes.try_into().ok()?) {
                Some(step) => steps.push(step),
                None => return Some(steps),
            }
        }
    }

    pub fn is_outcome_test(&self, routine: u32) -> bool {
        (self.bytes_at(routine, 4 * OUTCOME_TEST.len())).is_some_and(|code| {
            (code.as_chunks::<4>().0.iter())
                .map(|word| u32::from_le_bytes(*word))
                .eq(OUTCOME_TEST)
        })
    }

    pub fn is_way_back(&self, routine: u32) -> bool {
        (self.bytes_at(routine, 4 * WAY_BACK.len())).is_some_and(|code| {
            (code.as_chunks::<4>().0.iter())
                .map(|word| u32::from_le_bytes(*word))
                .eq(WAY_BACK)
        })
    }

    pub fn view_test(&self, routine: u32) -> Option<u16> {
        let code = self.bytes_at(routine, 4 * VIEW_TEST.len())?;
        let mut words = (code.as_chunks::<4>().0.iter()).map(|word| u32::from_le_bytes(*word));
        let mut view = None;
        for shape in VIEW_TEST {
            match (shape, words.next()?) {
                (Some(shape), word) if shape == word => {}
                // `xori v0,v0,N`: the wanted view turned to zero for view N.
                (None, word) if word >> 16 == 0x3842 => view = Some((word & 0xFFFF) as u16),
                _ => return None,
            }
        }
        view
    }

    pub fn frees_models(&self, routine: u32) -> bool {
        (self.bytes_at(routine, 4 * FREES_MODELS.len())).is_some_and(|code| {
            (code.as_chunks::<4>().0.iter())
                .map(|word| u32::from_le_bytes(*word))
                .eq(FREES_MODELS)
        })
    }

    pub fn screen_fade(&self, routine: u32) -> Option<u32> {
        let code = self.bytes_at(routine, 4 * (2 + SCREEN_FADE.len()))?;
        let mut words = (code.as_chunks::<4>().0.iter()).map(|word| u32::from_le_bytes(*word));
        let (frame, kind) = (words.next()?, words.next()?);
        (frame == 0x27BD_FFE8 && kind >> 16 == 0x2404 && words.eq(SCREEN_FADE))
            .then_some(kind & 0xFFFF)
    }

    pub fn shakes(&self, routine: u32) -> bool {
        self.bytes_at(routine, 4 * SHAKES.len())
            .is_some_and(|code| {
                (code.as_chunks::<4>().0.iter())
                    .map(|word| u32::from_le_bytes(*word))
                    .eq(SHAKES)
            })
    }

    pub fn asked_test(&self, routine: u32) -> Option<u32> {
        let length = ASKED_HEAD.len() + 1 + ASKED_BODY.len();
        let code = self.bytes_at(routine, 4 * length)?;
        let words: Vec<u32> = (code.as_chunks::<4>().0.iter())
            .map(|word| u32::from_le_bytes(*word))
            .collect();
        let (head, rest) = words.split_at(ASKED_HEAD.len());
        (head == ASKED_HEAD && rest[0] >> 16 == 0x2412 && rest[1..] == ASKED_BODY)
            .then_some(rest[0] & 0xFFFF)
    }

    pub fn plain_answer(&self, routine: u32) -> Option<i32> {
        let code = self.bytes_at(routine, 8)?;
        let mut words = (code.as_chunks::<4>().0.iter()).map(|word| u32::from_le_bytes(*word));
        // `jr ra`, with `addu v0, zero, zero` or `addiu v0, zero, N` in its delay slot.
        match (words.next()?, words.next()?) {
            (0x03E0_0008, 0x0000_1021) => Some(0),
            (0x03E0_0008, set) if set >> 15 == 0x2402 << 1 => Some((set & 0x7FFF) as i32),
            _ => None,
        }
    }

    pub fn is_list_test(&self, routine: u32) -> bool {
        (self.bytes_at(routine, 4 * LIST_TEST.len())).is_some_and(|code| {
            (code.as_chunks::<4>().0.iter())
                .map(|word| u32::from_le_bytes(*word))
                .eq(LIST_TEST)
        })
    }

    pub fn is_refusal_test(&self, routine: u32) -> bool {
        (self.bytes_at(routine, 4 * REFUSAL_TEST.len())).is_some_and(|code| {
            (code.as_chunks::<4>().0.iter())
                .map(|word| u32::from_le_bytes(*word))
                .eq(REFUSAL_TEST)
        })
    }

    pub fn does_nothing(&self, start: u32) -> bool {
        const JAL: u32 = 0x0C00_0000;
        const RETURN: [u32; 2] = [0x03E0_0008, 0];
        let word = |address: u32| {
            let bytes = self.bytes_at(address, 4)?;
            Some(u32::from_le_bytes(bytes.try_into().ok()?))
        };
        let called = (0..10u32)
            .filter_map(|n| word(start.checked_add(4 * n)?))
            .find(|code| code & 0xFC00_0000 == JAL)
            .map(|code| 0x8000_0000 | (code & 0x03FF_FFFF) << 2);
        called.is_some_and(|routine| {
            start != 0 && [word(routine), word(routine.wrapping_add(4))] == RETURN.map(Some)
        })
    }

    fn u32_at(&self, offset: usize) -> Option<u32> {
        let bytes = self.image.get(offset..)?.get(..4)?;
        Some(u32::from_le_bytes(bytes.try_into().ok()?))
    }

    /// True for 0 and for an address inside the code image.
    fn null_or_inside(&self, address: u32) -> bool {
        address == 0 || self.bytes_at(address, 1).is_some()
    }

    /// Reads what could be an object record at an offset of the image. `None` when the
    /// bytes cannot be one.
    fn object_at(&self, offset: usize) -> Option<ObjectRecord> {
        let record = ObjectRecord {
            address: PACKAGE_BASE + offset as u32,
            view_mask: self.u32_at(offset)?,
            kind: self.u32_at(offset + 4)?,
            data: self.u32_at(offset + 8)?,
            extra: self.u32_at(offset + 12)?,
            function: self.u32_at(offset + 16)?,
        };
        // The data of a numbered object is a small number, of any other a pointer.
        let data_fits = self.null_or_inside(record.data)
            || (record.kind == OBJECT_NUMBERED && record.data < MAX_NUMBER);
        let plausible = record.kind == OBJECT_END
            || (record.kind < OBJECT_END
                && data_fits
                && self.null_or_inside(record.function)
                && record.extra < MAX_EXTRA
                && (record.data != 0 || record.function != 0));
        plausible.then_some(record)
    }

    /// Offsets in the image that the image itself names: the word-aligned addresses that a
    /// `lui` with a following `addiu` or `ori` builds, and those stored as a data word.
    /// Sorted, each once.
    fn named_offsets(&self) -> Vec<usize> {
        const WINDOW: usize = 0x40;
        let mut named = BTreeSet::new();
        let mut note = |address: u32| {
            if address.is_multiple_of(4) && self.bytes_at(address, 4).is_some() {
                named.insert((address - PACKAGE_BASE) as usize);
            }
        };
        // Per register: the value of the last `lui` and where it was.
        let mut upper = [None::<(u32, usize)>; 32];
        for offset in (SIZE_WORD..self.image.len().saturating_sub(3)).step_by(4) {
            let Some(word) = self.u32_at(offset) else {
                break;
            };
            note(word);
            let source = (word >> 21) as usize & 31;
            let target = (word >> 16) as usize & 31;
            let low = word & 0xFFFF;
            match word >> 26 {
                0x0F => upper[target] = Some((low << 16, offset)),
                opcode @ (0x09 | 0x0D) => {
                    if let Some((high, _)) = upper[source].filter(|(_, at)| offset - at <= WINDOW) {
                        // `addiu` sign-extends its 16 bits, `ori` does not.
                        let low = if opcode == 0x09 {
                            low as u16 as i16 as u32
                        } else {
                            low
                        };
                        note(high.wrapping_add(low));
                    }
                }
                _ => {}
            }
        }
        named.into_iter().collect()
    }

    /// Every object list of the package, in file order.
    ///
    /// The package's own code decides which list a room uses and there is no directory of
    /// them, so they are found by two things. The package names the list's address, in the
    /// code of its entry function or in a table of list pointers; and at that address is a
    /// run of at least one plausible record (a known type, pointers that are 0 or lead into
    /// the code image, a small extra size) closed by an end record.
    pub fn object_lists(&self) -> Vec<ObjectList> {
        let mut lists = Vec::new();
        // Offset after the last list found: a list never starts inside another.
        let mut free_from = 0;
        for start in self.named_offsets() {
            if start < free_from {
                continue;
            }
            let mut objects = Vec::new();
            let mut offset = start;
            let closed = loop {
                match self.object_at(offset) {
                    Some(record) if record.kind == OBJECT_END => break true,
                    Some(record) => objects.push(record),
                    None => break false,
                }
                offset += OBJECT_SIZE;
            };
            if closed && !objects.is_empty() {
                free_from = offset + OBJECT_SIZE;
                lists.push(ObjectList {
                    address: PACKAGE_BASE + start as u32,
                    objects,
                });
            }
        }
        lists
    }

    /// Decodes the exit record an object of type [`OBJECT_EXIT`] points at.
    pub fn exit(&self, object: &ObjectRecord) -> Option<Exit> {
        if object.kind != OBJECT_EXIT {
            return None;
        }
        let bytes = self.bytes_at(object.data, EXIT_SIZE)?;
        let word = |at: usize| i32::from_le_bytes(bytes[at..at + 4].try_into().expect("4 bytes"));
        let half = |at: usize| i16::from_le_bytes(bytes[at..at + 2].try_into().expect("2 bytes"));
        Some(Exit {
            record: object.data,
            view_mask: object.view_mask,
            arrival: [word(0), word(4), word(8)],
            angle: half(12),
            leave_flag: half(14),
            region: half(16),
            area: half(18),
            place: half(20),
            slot: half(22),
        })
    }

    /// The segment that starts at a RAM address: the commands up to and including the first
    /// [`COMMAND_END`].
    ///
    /// A script has no length field; the end command is what the game itself stops at when
    /// it searches a segment for a label. A segment that is cut short (16 bytes that do not
    /// begin with a command number, or the end of the code image) comes back without one.
    pub fn commands_at(&self, address: u32) -> Vec<Command> {
        let mut commands = Vec::new();
        let mut at = address;
        while let Some(command) = self.command_at(at) {
            commands.push(command);
            if command.code == COMMAND_END {
                break;
            }
            at += COMMAND_SIZE as u32;
        }
        commands
    }

    /// The command at a RAM address: 16 bytes of the code image that begin with a command
    /// number.
    pub fn command_at(&self, address: u32) -> Option<Command> {
        let bytes = self.bytes_at(address, COMMAND_SIZE)?;
        let word = |i: usize| u32::from_le_bytes(bytes[i..i + 4].try_into().expect("4 bytes"));
        let code = word(0);
        (code < COMMAND_COUNT).then(|| Command {
            address,
            code,
            a: word(4) as i32,
            b: word(8),
            c: word(12),
        })
    }

    /// The segment at a RAM address followed by every segment its [`COMMAND_JUMP`]s lead to,
    /// each once, in the order the jumps are met.
    pub fn commands_from(&self, address: u32) -> Vec<Command> {
        let mut commands = Vec::new();
        let mut starts = vec![address];
        let mut next = 0;
        while let Some(&start) = starts.get(next) {
            next += 1;
            let segment = self.commands_at(start);
            for command in &segment {
                let target = command.c;
                if command.code == COMMAND_JUMP
                    && self.bytes_at(target, COMMAND_SIZE).is_some()
                    && !starts.contains(&target)
                {
                    starts.push(target);
                }
            }
            commands.extend(segment);
        }
        commands
    }

    /// Decodes the script an object of type [`OBJECT_SCRIPT`] points at.
    pub fn script(&self, object: &ObjectRecord) -> Option<Script> {
        if object.kind != OBJECT_SCRIPT || object.data == 0 {
            return None;
        }
        Some(Script {
            address: object.data,
            view_mask: object.view_mask,
            commands: self.commands_from(object.data),
        })
    }

    /// The scripts of every object list of the package, in file order. Scripts that the
    /// package starts from its native code (a trigger's, for one) are not among them.
    pub fn scripts(&self) -> Vec<Script> {
        self.object_lists()
            .iter()
            .flat_map(|list| &list.objects)
            .filter_map(|object| self.script(object))
            .collect()
    }

    /// The animation sets of the player that the package's code asks for, one entry for
    /// every call of [`PLAYER_SET_LOADER`] in the order of the code: the number that one
    /// of the four instructions before the call puts into the argument register (or into
    /// `v0`, which the package also stores in the kept word), a zero stored to the kept
    /// word, or the kept word read back. `None` for a call whose argument comes another
    /// way.
    pub fn player_sets(&self) -> Vec<Option<PlayerSet>> {
        let call = jump_and_link(PLAYER_SET_LOADER);
        let kept = KEPT_PLAYER_SET & 0xFFFF;
        let words = self.image.len() / 4;
        let mut sets = Vec::new();
        for at in (0..words).filter(|at| self.u32_at(4 * at) == Some(call)) {
            let found = (1..=4usize).find_map(|back| {
                let word = self.u32_at(4 * at.checked_sub(back)?)?;
                // addiu v0 or a0, zero, N
                if matches!(word >> 16, 0x2402 | 0x2404) {
                    Some(PlayerSet::Number(word as u16))
                } else if word & 0xFC1F_FFFF == 0x8C04_0000 | kept {
                    // lw a0, KEPT(reg)
                    Some(PlayerSet::Kept)
                } else if word & 0xFC1F_FFFF == 0xAC00_0000 | kept {
                    // sw zero, KEPT(reg)
                    Some(PlayerSet::Number(0))
                } else {
                    None
                }
            });
            sets.push(found);
        }
        sets
    }

    pub fn kept_set_routines(&self) -> Vec<(u32, i16)> {
        let end = self.image.len().saturating_sub(4 * SIZE_WORD + 3);
        (SIZE_WORD..end)
            .step_by(4)
            .filter_map(|start| {
                let word = |at: usize| self.u32_at(start + 4 * at);
                let number = word(1).filter(|word| word >> 16 == 0x2402)?;
                let shape = word(0) == Some(KEPT_UPPER)
                    && word(2) == Some(KEPT_STORE)
                    && word(3) == Some(RETURN)
                    && word(4) == Some(ANSWER_ONE);
                shape.then_some((PACKAGE_BASE + start as u32, number as i16))
            })
            .collect()
    }

    pub fn equip_routines(&self) -> Vec<EquipRoutine> {
        let end = self.image.len().saturating_sub(4 * EQUIP_SHAPE.len() - 1);
        (SIZE_WORD..end)
            .step_by(4)
            .filter_map(|start| {
                let word = |at: usize| self.u32_at(start + 4 * at);
                let shape = (EQUIP_SHAPE.iter().enumerate())
                    .all(|(at, wanted)| wanted.is_none_or(|wanted| word(at) == Some(wanted)));
                // addu a0,zero,zero or addiu a0,zero,N, and addiu a1,zero,N.
                let item = match word(1)? {
                    0x0000_2021 => 0,
                    word if word >> 16 == 0x2404 => i32::from(word as i16),
                    _ => return None,
                };
                let kind = word(4).filter(|word| word >> 16 == 0x2405)? as i16;
                shape.then_some(EquipRoutine {
                    address: PACKAGE_BASE + start as u32,
                    item,
                    kind: i32::from(kind),
                })
            })
            .collect()
    }

    pub fn fade_routines(&self) -> Vec<u32> {
        let end = self.image.len().saturating_sub(4 * FADE_LENGTH - 1);
        (SIZE_WORD..end)
            .step_by(4)
            .filter(|&start| {
                let word = |at: usize| self.u32_at(start + 4 * at);
                let has = |wanted: u32| (1..FADE_LENGTH).any(|at| word(at) == Some(wanted));
                word(0) == Some(FADE_STACK)
                    && word(FADE_LENGTH - 2) == Some(RETURN)
                    && FADE_TELLS.iter().all(|&wanted| has(wanted))
            })
            .map(|start| PACKAGE_BASE + start as u32)
            .collect()
    }

    pub fn view_musics(&self, entry: u32) -> Option<[i32; VIEW_MUSICS]> {
        let from = entry.checked_sub(PACKAGE_BASE)? as usize;
        let word = |at: usize| self.u32_at(from + 4 * at);
        let call = (0..VIEW_MUSIC_SEARCH).find(|&at| {
            word(at) == Some(jump_and_link(ASK_MUSIC))
                && word(at + 2) == Some(jump_and_link(NOTE_MUSIC))
        })?;
        (0..call).find(|&at| word(at) == Some(WANTED_VIEW_READ))?;
        // `lui r,high`, `addiu r,r,low` and `addiu a0,r,32`: the table and its end.
        let table = (0..call).find_map(|at| {
            let (upper, lower, end) = (word(at)?, word(at + 1)?, word(at + 2)?);
            let shape = upper >> 26 == 0x0F && lower >> 26 == 0x09 && end == VIEW_MUSIC_END;
            shape.then(|| (upper << 16).wrapping_add(lower as u16 as i16 as u32))
        })?;
        let bytes = self.bytes_at(table, 4 * VIEW_MUSICS)?;
        let mut musics = [0; VIEW_MUSICS];
        for (music, bytes) in musics.iter_mut().zip(bytes.as_chunks::<4>().0) {
            *music = i32::from_le_bytes(*bytes);
        }
        Some(musics)
    }

    pub fn entry_stored_room(&self, entry: u32) -> Option<u16> {
        let from = entry.checked_sub(PACKAGE_BASE)? as usize;
        let word = |at: usize| self.u32_at(from + 4 * at);
        let call = (0..VIEW_MUSIC_SEARCH).find(|&at| {
            word(at) == Some(jump_and_link(ASK_MUSIC))
                && word(at + 2) == Some(jump_and_link(NOTE_MUSIC))
        })?;
        // `jal` to the routine with `addiu a0,zero,room` in its delay slot.
        let room = word(call + 5)?;
        (word(call + 4) == Some(jump_and_link(STORE_ROOM)) && room >> 16 == 0x2404)
            .then_some(room as u16)
    }

    /// Every command of the scripts of the object lists, each once, in the order of their
    /// addresses: two scripts can lead to one segment.
    pub fn script_commands(&self) -> Vec<Command> {
        let commands: BTreeMap<u32, Command> = (self.scripts().into_iter())
            .flat_map(|script| script.commands)
            .map(|command| (command.address, command))
            .collect();
        commands.into_values().collect()
    }

    /// The typed form of a command that opens the notice window, `None` for every other
    /// command and for one whose list of things is not in the package.
    pub fn notice(&self, command: &Command) -> Option<NoticeCommand> {
        let way = command.b as i32;
        let things = match command.code {
            0x3B => {
                return Some(NoticeCommand {
                    way: NOTICE_LINES_ONLY,
                    things: Some(Vec::new()),
                });
            }
            0x3C | 0x3D if command.c == 0 => None,
            0x3C | 0x3D => {
                let count = usize::try_from(command.a).unwrap_or(0).min(NOTICE_THINGS);
                let list = self.bytes_at(command.c, count * NOTICE_THING_SIZE)?;
                let things = list.as_chunks::<NOTICE_THING_SIZE>().0.iter();
                Some(
                    things
                        .map(|thing| NoticeThing {
                            kind: i16::from_le_bytes([thing[0], thing[1]]).into(),
                            number: i16::from_le_bytes([thing[2], thing[3]]).into(),
                            count: i32::from_le_bytes([thing[4], thing[5], thing[6], thing[7]]),
                        })
                        .collect(),
                )
            }
            0x3E if command.a == -1 => None,
            0x3E => Some(vec![NoticeThing {
                kind: command.c as i32,
                number: command.a,
                count: 1,
            }]),
            _ => return None,
        };
        Some(NoticeCommand { way, things })
    }

    pub fn lines_at(&self, address: u32) -> Option<Vec<Vec<u16>>> {
        let mut lines = Vec::new();
        for n in 0..NOTICE_LINES_MOST {
            let line = self.bytes_at(address.checked_add(4 * n)?, 4)?;
            match u32::from_le_bytes(line.try_into().ok()?) {
                0 => return Some(lines),
                line => {
                    let at = line.checked_sub(PACKAGE_BASE)? as usize;
                    lines.push(crate::text::units(self.image.get(at..)?));
                }
            }
        }
        None
    }

    /// The exits of every object list of the package, in file order.
    pub fn exits(&self) -> Vec<Exit> {
        self.object_lists()
            .iter()
            .flat_map(|list| &list.objects)
            .filter_map(|object| self.exit(object))
            .collect()
    }

    /// Offset of the routine that holds an offset of the image: the nearest instruction
    /// before it that makes room on the stack.
    fn routine_start(&self, mut offset: usize) -> Option<usize> {
        while offset >= SIZE_WORD {
            let word = self.u32_at(offset)?;
            // `addiu sp,sp,-N`
            if word >> 16 == 0x27BD && word & 0x8000 != 0 {
                return Some(offset);
            }
            offset -= 4;
        }
        None
    }

    /// The addresses that the instructions of a stretch of the image build with a `lui` and
    /// a following `addiu`, with the offset of the `addiu`, in file order.
    fn built_addresses(&self, from: usize, to: usize) -> Vec<(usize, u32)> {
        let mut built = Vec::new();
        let mut upper = [None::<u32>; 32];
        for offset in (from..to).step_by(4) {
            let Some(word) = self.u32_at(offset) else {
                break;
            };
            let source = (word >> 21) as usize & 31;
            let target = (word >> 16) as usize & 31;
            let low = word & 0xFFFF;
            match word >> 26 {
                0x0F => upper[target] = Some(low << 16),
                0x09 => {
                    if let Some(high) = upper[source] {
                        built.push((offset, high.wrapping_add(low as u16 as i16 as u32)));
                    }
                }
                _ => {}
            }
        }
        built
    }

    fn stack_table(&self, start: usize, call: usize) -> Option<[u32; SHOW_WAYS]> {
        let words: Vec<(usize, u32)> = (start..=call + 4)
            .step_by(4)
            .map_while(|offset| Some((offset, self.u32_at(offset)?)))
            .collect();
        let fill = (words.iter()).position(|(_, word)| *word == jump_and_link(FILL))?;
        // The arguments are set before the call or in its delay slot.
        let before = &words[..(fill + 2).min(words.len())];
        before.iter().find(|(_, word)| *word == FILL_TABLE)?;
        // `addiu a0,sp,N`: where the table lies in the frame.
        let base = (before.iter().rev())
            .find(|(_, word)| word >> 16 == 0x27A4)
            .map(|(_, word)| word & 0xFFFF)?;
        let built = self.built_addresses(start, call + 8);
        let mut entries = [0u32; SHOW_WAYS];
        for (offset, word) in &words {
            // `sw register,N(sp)` of the register the instruction before has built.
            if word >> 21 != (0x2B << 5 | 29) {
                continue;
            }
            let register = word >> 16 & 31;
            let address = built
                .iter()
                .find(|(at, _)| at + 4 == *offset)
                .filter(|(at, _)| {
                    (self.u32_at(*at)).is_some_and(|addiu| addiu >> 16 & 31 == register)
                });
            let way = (word & 0xFFFF)
                .checked_sub(base)
                .map(|bytes| bytes as usize);
            if let (Some((_, address)), Some(way)) = (address, way)
                && way.is_multiple_of(4)
                && way / 4 < SHOW_WAYS
            {
                entries[way / 4] = *address;
            }
        }
        Some(entries)
    }

    pub fn show_routines(&self) -> Option<ShowRoutines> {
        let end = self.image.len().saturating_sub(3);
        let getter = jump_and_link(ROOM_RESOURCE);
        let inside =
            |address: u32| address.is_multiple_of(4) && self.bytes_at(address, 4).is_some();
        let fits = |entries: &[u32; SHOW_WAYS]| {
            entries.iter().any(|entry| *entry != 0)
                && entries.iter().all(|entry| *entry == 0 || inside(*entry))
        };
        let (starter, table, entries) = (SIZE_WORD..end)
            .step_by(4)
            .filter(|offset| self.u32_at(*offset) == Some(getter))
            .find_map(|call| {
                let start = self.routine_start(call)?;
                if let Some(entries) = self.stack_table(start, call).filter(fits) {
                    return Some((start, None, entries));
                }
                self.built_addresses(start, call)
                    .into_iter()
                    .find_map(|(_, address)| {
                        self.bytes_at(address, 4 * SHOW_WAYS)?;
                        let offset = (address - PACKAGE_BASE) as usize;
                        let mut entries = [0u32; SHOW_WAYS];
                        for (way, entry) in entries.iter_mut().enumerate() {
                            *entry = self.u32_at(offset + 4 * way)?;
                        }
                        fits(&entries).then_some((start, Some(address), entries))
                    })
            })?;
        let starter = PACKAGE_BASE + starter as u32;
        let spawner = self
            .built_addresses(SIZE_WORD, end)
            .into_iter()
            .find(|(_, address)| *address == starter)
            .and_then(|(offset, _)| self.routine_start(offset))?;
        Some(ShowRoutines {
            starter,
            table,
            entries,
            spawner: PACKAGE_BASE + spawner as u32,
        })
    }

    /// The show records at a RAM address: 44-byte records up to one whose first value is
    /// [`SHOW_END`].
    pub fn shows_at(&self, address: u32) -> Vec<Show> {
        let mut shows = Vec::new();
        let mut at = address;
        while let Some(bytes) = self.bytes_at(at, SHOW_SIZE) {
            let bytes: &[u8; SHOW_SIZE] = bytes.try_into().expect("the size asked for");
            let show = Show::parse(bytes);
            if show.way == SHOW_END || shows.len() == MAX_SHOWS {
                break;
            }
            shows.push(show);
            at += SHOW_SIZE as u32;
        }
        shows
    }

    /// The lists of show records that the package's scripts hand its spawner, each once, in
    /// file order of the commands: a [`COMMAND_ARGUMENT`] with the list's address followed by
    /// a [`COMMAND_NATIVE`] with the spawner's.
    ///
    /// The commands are searched for in the whole code image, not only in the scripts of the
    /// object lists, because most of them lie in scripts that native code starts. The few
    /// lists that a package's own code hands the spawner are not found: their addresses are
    /// not constants.
    pub fn show_lists(&self) -> Vec<ShowList> {
        let Some(routines) = self.show_routines() else {
            return Vec::new();
        };
        let mut lists: Vec<ShowList> = Vec::new();
        let end = self.image.len().saturating_sub(2 * COMMAND_SIZE - 1);
        for offset in (SIZE_WORD..end).step_by(4) {
            let word = |at: usize| self.u32_at(offset + at);
            let pair = word(0) == Some(COMMAND_ARGUMENT)
                && word(COMMAND_SIZE) == Some(COMMAND_NATIVE)
                && word(2 * COMMAND_SIZE - 4) == Some(routines.spawner);
            let Some(address) = word(COMMAND_SIZE - 4).filter(|_| pair) else {
                continue;
            };
            if self.bytes_at(address, SHOW_SIZE).is_some()
                && lists.iter().all(|list| list.address != address)
            {
                lists.push(ShowList {
                    address,
                    shows: self.shows_at(address),
                });
            }
        }
        lists
    }

    pub fn movie_routine(&self) -> Option<u32> {
        let end = self.image.len().saturating_sub(3);
        let player = jump_and_link(MOVIE_PLAYER);
        (2 * SIZE_WORD..end)
            .step_by(4)
            .filter(|call| self.u32_at(*call) == Some(player))
            // `lw a0,0(register)` right before the call, with a register that is kept over
            // calls (`s0` to `s7`): the movie's number is the first word of the record the
            // routine was handed. The object routine of files 245 to 248, which is no
            // script's, takes the number from a table through `v1`.
            .filter(|call| {
                (self.u32_at(call - SIZE_WORD)).is_some_and(|word| {
                    word & 0xFC1F_FFFF == 0x8C04_0000 && (16..24).contains(&(word >> 21 & 31))
                })
            })
            .find_map(|call| self.routine_start(call))
            .map(|start| PACKAGE_BASE + start as u32)
    }

    /// The movies that the package's scripts play, in file order of the commands: a
    /// [`COMMAND_RECORD`] with the movie's number followed by a [`COMMAND_NATIVE`] with the
    /// movie routine's address.
    pub fn script_movies(&self) -> Vec<u32> {
        let Some(routine) = self.movie_routine() else {
            return Vec::new();
        };
        let end = self.image.len().saturating_sub(2 * COMMAND_SIZE - 1);
        (SIZE_WORD..end)
            .step_by(4)
            .filter_map(|offset| {
                let word = |at: usize| self.u32_at(offset + at);
                let pair = word(0) == Some(COMMAND_RECORD)
                    && word(COMMAND_SIZE) == Some(COMMAND_NATIVE)
                    && word(2 * COMMAND_SIZE - 4) == Some(routine);
                word(SIZE_WORD).filter(|_| pair)
            })
            .collect()
    }

    pub fn read_routines(&self) -> Vec<ReadRoutine> {
        let end = self.image.len().saturating_sub(3);
        let (one, set) = (jump_and_link(ANIMATION_LOADER), jump_and_link(SET_LOADER));
        // The last `addiu register,zero,N` of a stretch of words.
        let constant = |words: &[u32], register: u32| {
            (words.iter().rev())
                .find(|word| *word >> 16 == 0x2400 | register)
                .map(|word| word & 0xFFFF)
        };
        (SIZE_WORD..end)
            .step_by(4)
            .filter(|call| [Some(one), Some(set)].contains(&self.u32_at(*call)))
            .filter_map(|call| {
                let start = self.routine_start(call)?;
                // Up to the delay slot of the call.
                let words: Vec<u32> = (start..=call + 4)
                    .step_by(4)
                    .map_while(|offset| self.u32_at(offset))
                    .collect();
                let getter = (words.iter()).position(|word| *word == jump_and_link(ACTOR_MODEL))?;
                let actor = constant(&words[..(getter + 2).min(words.len())], 4)?;
                let reads = match self.u32_at(call) == Some(one) {
                    true => Reads::Animation(constant(&words, 5)?),
                    false => Reads::Set,
                };
                Some(ReadRoutine {
                    address: PACKAGE_BASE + start as u32,
                    actor,
                    reads,
                })
            })
            .collect()
    }

    pub fn clear_routine(&self) -> Option<u32> {
        let end = self.image.len().saturating_sub(3);
        (SIZE_WORD..end)
            .step_by(4)
            .filter(|start| {
                self.u32_at(*start) == Some(POOL_UPPER) && self.u32_at(start + 4) == Some(POOL_LOAD)
            })
            .find(|start| {
                (2..CLEAR_LENGTH)
                    .map_while(|word| self.u32_at(start + 4 * word))
                    .take_while(|word| word >> 26 != 3 && *word != RETURN)
                    .any(|word| word == LIGHT_CLEAR)
            })
            .map(|start| PACKAGE_BASE + start as u32)
    }

    /// How many commands of the code image call the routine that takes the shown things
    /// away: a [`COMMAND_NATIVE`] with its address.
    pub fn clear_calls(&self) -> usize {
        let Some(routine) = self.clear_routine() else {
            return 0;
        };
        let end = self.image.len().saturating_sub(COMMAND_SIZE - 1);
        (SIZE_WORD..end)
            .step_by(4)
            .filter(|offset| {
                self.u32_at(*offset) == Some(COMMAND_NATIVE)
                    && self.u32_at(offset + COMMAND_SIZE - 4) == Some(routine)
            })
            .count()
    }
}

/// The getter of file 27 that gives the model of an actor of the room by its number in
/// the field's list of things, which holds the actors in the order of the object list.
const ACTOR_MODEL: u32 = 0x800A_79AC;
const ANIMATION_LOADER: u32 = 0x8002_54A8;
const SET_LOADER: u32 = 0x8002_57E8;

/// A routine of a package that reads animations for the model of an actor when a script
/// calls it. It has two states, kept in the first word of the script's argument record:
/// the first call asks the loader and answers 0, every later one answers whether the
/// loader is through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadRoutine {
    /// RAM address of the routine.
    pub address: u32,
    /// The actor whose model it is: its number among the actors of the room, from 0.
    pub actor: u32,
    pub reads: Reads,
}

/// What a [`ReadRoutine`] asks the loader for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reads {
    /// One animation of the model, by its number.
    Animation(u32),
    /// The animation set that the second word of the argument record names, in place of
    /// the set the model holds.
    Set,
}

const POOL_UPPER: u32 = 0x3C02_8009;
const POOL_LOAD: u32 = 0x8C44_D390;
/// `sw zero,-0x2C80(v0)`: the range of the fourth light, `0x8008D380`, is cleared.
const LIGHT_CLEAR: u32 = 0xAC40_D380;
/// Instructions of the routine that takes the shown things away.
const CLEAR_LENGTH: usize = 32;
const MOVIE_PLAYER: u32 = 0x8004_2B00;
/// Command that fills the argument record the next native call gets, with `a`, `b` and `c`.
pub const COMMAND_RECORD: u32 = 0x4C;
/// RAM address of the field engine's routine that gives a resource block of the room.
const ROOM_RESOURCE: u32 = 0x800A_5540;
/// RAM address of the executable's fill routine (address, value, bytes).
const FILL: u32 = 0x8004_E28C;
/// `addiu a2,zero,20`: the fill routine is asked for the five words of a starter's table.
const FILL_TABLE: u32 = 0x2406_0014;
/// Command that sets the argument the next native call gets, to the address `c`.
pub const COMMAND_ARGUMENT: u32 = 0x4B;
/// Command that calls the native routine at the address `c`.
pub const COMMAND_NATIVE: u32 = 0x4D;
/// Bytes of one show record.
pub const SHOW_SIZE: usize = 44;
/// First value of the record that ends a list of show records.
pub const SHOW_END: i16 = 5;
/// Number of ways to show a thing: the entries of the starter's table.
pub const SHOW_WAYS: usize = 5;
/// A list of show records longer than this is not taken for one.
const MAX_SHOWS: usize = 64;

pub const PLAYER_SET_LOADER: u32 = 0x800A_C8D8;
/// The word that keeps the set of the player the last scene asked for.
pub const KEPT_PLAYER_SET: u32 = 0x8009_1C04;

pub const COMMAND_FIGHT: u32 = 0x65;
/// The opponents a fight's record has room for.
pub const FIGHT_ENTRIES: usize = 3;
/// Where the record names the things of its entries, a word each.
const FIGHT_THINGS: u32 = 24;
/// The thing of an entry without an opponent.
const FIGHT_NO_THING: u32 = 5;
/// The words of a fight's record: six of the head, three things, three opponents of
/// three words.
const FIGHT_WORDS: usize = 18;
pub const COMMAND_FIGHT_ENDED: u32 = 0x66;
pub const COMMAND_MAKE_WHOLE: u32 = 0x2C;
pub const LESSONS_FILE: usize = 305;
/// The table of that package with the address of each lesson's list of steps.
pub const LESSON_TABLE: u32 = 0x800B_6884;
/// The lessons of that package.
pub const LESSONS: u32 = 2;
/// Its native routine that marks the fight behind it as a lesson: a script calls it
/// (`0x4D`) in front of the `0x65`, and the lesson's number is the first word of the
/// argument record, the `a` of the `0x4C` in front of the call.
pub const LESSON_ROUTINE: u32 = 0x800B_398C;
/// The code of a native routine that answers whether the outcome word is not zero: the
/// word's address in two instructions, the return, and the test in its delay slot.
const OUTCOME_TEST: [u32; 4] = [0x3C02_800B, 0x8C42_1400, 0x03E0_0008, 0x0002_102B];
/// The code of a native routine that takes the way back of a lost fight: it calls
/// `0x800A5F3C` of the field engine and answers 1.
const WAY_BACK: [u32; 8] = [
    0x27BD_FFE8,
    0xAFBF_0010,
    0x0C02_97CF,
    0,
    0x8FBF_0010,
    0x2402_0001,
    0x03E0_0008,
    0x27BD_0018,
];
/// The code of a native routine that answers whether the wanted view is one view: the
/// word's address and its read, an empty slot, the word turned to zero for the view
/// (left open), the return, and "is zero" in its delay slot.
const VIEW_TEST: [Option<u32>; 6] = [
    Some(0x3C02_8009),
    Some(WANTED_VIEW_READ),
    Some(0),
    None,
    Some(0x03E0_0008),
    Some(0x2C42_0001),
];
/// The code of a native routine that frees the room's package of models and answers 1:
/// it calls `0x800A564C` of the field engine with kind 0.
const FREES_MODELS: [u32; 8] = [
    0x27BD_FFE8,
    0xAFBF_0010,
    0x0C02_9593,
    0x0000_2021,
    0x8FBF_0010,
    0x2402_0001,
    0x03E0_0008,
    0x27BD_0018,
];
/// The code of a native routine that asks for the screen's fade and answers 1, behind
/// its first word and the one that loads the kind: it calls `0x8001C1A8(kind, 32)`.
const SCREEN_FADE: [u32; 7] = [
    0xAFBF_0010,
    0x0C00_706A,
    0x2405_0020,
    0x8FBF_0010,
    0x2402_0001,
    0x03E0_0008,
    0x27BD_0018,
];
/// The code of a native routine that asks for the shake of the picture and answers 1:
/// it calls `0x80037C28` with 0, which the routine does not look at.
const SHAKES: [u32; 8] = [
    0x27BD_FFE8,
    0xAFBF_0010,
    0x0C00_DF0A,
    0x0000_2021,
    0x8FBF_0010,
    0x2402_0001,
    0x03E0_0008,
    0x27BD_0018,
];
/// The code of a native routine that counts the room bits of a mask that are set
/// (`0x800B284C` of room 36), in front of the word that loads the mask and behind it;
/// the jump behind these words leads to the routine's own end.
const ASKED_HEAD: [u32; 8] = [
    0x27BD_FFD8,
    0xAFB0_0010,
    0x0000_8021,
    0xAFB1_0014,
    0x0200_8821,
    0xAFB3_001C,
    0x2413_0001,
    0xAFB2_0018,
];
const ASKED_BODY: [u32; 13] = [
    0xAFBF_0020,
    0x0C02_EFE6,
    0x0213_2004,
    0x1040_0002,
    0,
    0x2631_0001,
    0x2610_0001,
    0x0212_1007,
    0x3042_0001,
    0x1440_FFF7,
    0,
    0x1220_0003,
    0x0211_1026,
];
/// The code of a native routine that answers whether the outcome word is 5: the word,
/// an empty slot, the word turned to zero for a 5, the return, and "is zero" in its
/// delay slot.
/// The start of the test in front of the screen of file 44: room bit `0x20000000` is
/// asked for (`0x800BBF98`) and set (`0x800BBF60`), then the caught creatures are
/// counted (`0x800106B8`).
const LIST_TEST: [u32; 12] = [
    0x27BD_FFE0,
    0x3C04_2000,
    0xAFBF_0018,
    0xAFB1_0014,
    0x0C02_EFE6,
    0xAFB0_0010,
    0x1440_002C,
    0x0000_1021,
    0x3C04_2000,
    0x0C02_EFD8,
    0x2405_0001,
    0x0C00_41AE,
];

const REFUSAL_TEST: [u32; 6] = [
    0x3C02_800B,
    0x8C42_1400,
    0,
    0x3842_0005,
    0x03E0_0008,
    0x2C42_0001,
];

/// What a script's `0x65` hands the field engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FightRecord {
    pub block: Block,
    /// A lost fight sends the player away; when not, the script goes on behind it.
    pub loss_is_final: bool,
}
/// `lui v1,0x8009`, `sw v0,0x1C04(v1)` and `addiu v0,zero,1`: the kept word is written and
/// the routine answers 1.
const KEPT_UPPER: u32 = 0x3C03_8009;
const KEPT_STORE: u32 = 0xAC62_0000 | (KEPT_PLAYER_SET & 0xFFFF);
const ANSWER_ONE: u32 = 0x2402_0001;

pub const ASK_MUSIC: u32 = 0x8003_013C;
pub const NOTE_MUSIC: u32 = 0x8003_1634;
pub const STORE_ROOM: u32 = 0x800A_67CC;
pub const VIEW_MUSICS: usize = 9;
const VIEW_MUSIC_SEARCH: usize = 128;
const WANTED_VIEW_READ: u32 = 0x8442_B7F0;
const VIEW_MUSIC_END: u32 = 0x2444_0020;

pub const SEQUENCE_FADE: u32 = 0x8006_2740;
pub const KEPT_SOUND_RECORD: u32 = 0x8009_1CD0;
/// A routine that fades that record's running sequence: how many instructions it has,
/// the one that makes room on the stack, and the ones that tell it from any other
/// routine: `addiu s0,v0,low` of the record's address, `lh a1,4(s0)`, the call of the
/// fade with `addiu a2,zero,127` and `addiu a3,zero,120`, `sh v0,4(s0)` and the answer 1.
const FADE_LENGTH: usize = 19;
const FADE_STACK: u32 = 0x27BD_FFE8;
const FADE_TELLS: [u32; 7] = [
    0x2450_0000 | (KEPT_SOUND_RECORD & 0xFFFF),
    0x8605_0004,
    0x0C00_0000 | ((SEQUENCE_FADE >> 2) & 0x03FF_FFFF),
    0x2406_007F,
    0x2407_0078,
    0xA602_0004,
    ANSWER_ONE,
];

pub const EQUIP: u32 = 0x800A_DBBC;
/// A routine that calls it and answers 1; the two numbers are in the words left open.
const EQUIP_SHAPE: [Option<u32>; 9] = [
    Some(0x27BD_FFE8),
    None,
    Some(0xAFBF_0010),
    Some(0x0C00_0000 | ((EQUIP & 0x03FF_FFFF) >> 2)),
    None,
    Some(0x8FBF_0010),
    Some(ANSWER_ONE),
    Some(RETURN),
    Some(0x27BD_0018),
];

/// A routine of a scene package that puts something on the player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EquipRoutine {
    /// RAM address of the routine, which a `0x4D` command names.
    pub address: u32,
    /// The thing's number and the kind as the equip routine counts it: 0 a weapon, 1 an
    /// armour, 2 the third thing.
    pub item: i32,
    pub kind: i32,
}

/// What a scene package hands the loader of the player's animation sets in one call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerSet {
    /// A set of the player's model by its number.
    Number(u16),
    /// The set the last scene asked for, read back from the kept word.
    Kept,
}

/// The instruction that calls a routine.
fn jump_and_link(address: u32) -> u32 {
    0x0C00_0000 | ((address & 0x03FF_FFFF) >> 2)
}

/// Where a package keeps the routines that show pieces of the room's six-section package.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShowRoutines {
    /// RAM address of the routine that starts the task of one show record.
    pub starter: u32,
    /// RAM address of the table of routines, one per way of showing; `None` when the
    /// starter fills the table on the stack.
    pub table: Option<u32>,
    /// The table: the routine of each way, 0 for a way the package does not use.
    pub entries: [u32; SHOW_WAYS],
    /// RAM address of the routine that is handed a list of show records.
    pub spawner: u32,
}

/// How a show record shows its thing: the entry of the starter's table it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShowWay {
    /// Entry 0: a sprite of the room's package, every quad of the frame.
    Sprite,
    /// Entry 1: a model of the room's package, the placements of the frame.
    Model,
    /// Entry 3: a clip of the room's package, unpacked into video memory.
    Clip,
    /// Entry 4: the movable fourth light of the room.
    Light,
}

/// One thing a scene shows from the room's six-section package, or its fourth light.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Show {
    /// Entry of the starter's table; see [`Show::shown`].
    pub way: i16,
    /// The number of the sprite, model or clip in the room's package.
    pub number: i16,
    /// Position, added to what the mover gives.
    pub place: [i32; 3],
    /// Angles, 4096 to the circle, added to what the mover gives. For a light: its colour.
    pub angles: [i16; 3],
    /// Game frames one run takes.
    pub length: i16,
    /// Game frames to wait before the start.
    pub wait: i16,
    /// Runs after the first one; below zero: without end.
    pub runs: i16,
    /// The first frame of a run.
    pub first: i16,
    /// The value the counter restarts from after a run.
    pub restart: i16,
    /// The last frame of a run; below zero: the last there is.
    pub last: i16,
    /// A word whose meaning depends on the way: the blend word of a sprite with bit 3 for
    /// "in front of everything", the mode of a clip, the range of a light.
    pub mode: i16,
    /// RAM address of a routine of the package that gives a position and angles each frame,
    /// 0 for none.
    pub mover: u32,
}

impl Show {
    pub fn parse(bytes: &[u8; SHOW_SIZE]) -> Self {
        let half = |at: usize| i16::from_le_bytes(bytes[at..at + 2].try_into().expect("2 bytes"));
        let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().expect("4 bytes"));
        Self {
            way: half(0),
            number: half(2),
            place: [word(4) as i32, word(8) as i32, word(12) as i32],
            angles: [half(16), half(18), half(20)],
            length: half(24),
            wait: half(26),
            runs: half(28),
            first: half(30),
            restart: half(32),
            last: half(34),
            mode: half(36),
            mover: word(40),
        }
    }

    /// The way of showing, `None` for the empty entry 2 and for a value outside the table.
    pub fn shown(&self) -> Option<ShowWay> {
        match self.way {
            0 => Some(ShowWay::Sprite),
            1 => Some(ShowWay::Model),
            3 => Some(ShowWay::Clip),
            4 => Some(ShowWay::Light),
            _ => None,
        }
    }

    /// The frame of a thing with `frames` frames that is shown when the counter of the run
    /// stands at `counter` game frames: the stretch from the first to the last frame spread
    /// over the length of a run, which is the number of frames when the record's is below
    /// zero. `None` for a thing without frames or a run without length.
    pub fn frame(&self, counter: i32, frames: usize) -> Option<usize> {
        let frames = i32::try_from(frames).ok().filter(|frames| *frames > 0)?;
        let length = match i32::from(self.length) {
            0 => return None,
            length if length < 0 => frames,
            length => length,
        };
        let first = i32::from(self.first);
        let last = match self.last {
            last if last < 0 => frames - 1,
            last => i32::from(last),
        };
        let frame = first + (last + 1 - first) * counter / length;
        Some(frame.rem_euclid(frames) as usize)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ShowClock {
    /// Game frames waited so far, up to the record's wait.
    pub waited: i16,
    /// Game frames into the run.
    pub counter: i16,
    /// Runs that have ended.
    pub runs: i16,
    /// The last run is over: the game ends the task.
    pub ended: bool,
}

impl ShowClock {
    /// The frame of a thing with `frames` frames that this game frame shows; `None` while the
    /// record waits and after its last run.
    pub fn frame(&self, show: &Show, frames: usize) -> Option<usize> {
        if self.ended || self.waited < show.wait {
            return None;
        }
        show.frame(i32::from(self.counter), frames)
    }

    /// One game frame, after the frame was shown: the wait goes on, or the counter goes up
    /// and a run that reaches the record's length ends. The counter then restarts from the
    /// record's value, and the run was the last one when as many have ended as the record
    /// names (never when that is below zero).
    pub fn step(&mut self, show: &Show) {
        if self.ended {
            return;
        }
        if self.waited < show.wait {
            self.waited = self.waited.saturating_add(1).clamp(0, show.wait);
            return;
        }
        self.counter = self.counter.wrapping_add(1);
        if self.counter < show.length {
            return;
        }
        self.ended = show.runs >= 0 && self.runs >= show.runs;
        self.runs = self.runs.wrapping_add(1);
        self.counter = show.restart;
    }
}

/// A list of show records that a script hands the spawner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShowList {
    /// RAM address of the first record.
    pub address: u32,
    pub shows: Vec<Show>,
}

/// Object type of an actor: its data is an instance, its function the package's spawner.
pub const OBJECT_ACTOR: u32 = 0x0F;
/// Models the field's loader holds at once: the notes of a package's entry routine.
pub const MODEL_SLOTS: usize = 4;
/// Bytes of an actor's instance that the spawner reads.
const INSTANCE_SIZE: usize = 36;
/// Bytes of an actor record.
const ACTOR_RECORD_SIZE: usize = 32;
/// Where the turn of the colours lies from the start of an actor record.
const ACTOR_TURN: u32 = 0x28;
/// Where the word lies that mirrors the actor's face in a message window.
const ACTOR_MIRROR: u32 = 0x2A;
/// Where the default actions lie from the start of an actor record.
const ACTOR_MOVES: u32 = 0x20;
/// The instruction that ends a routine.
const RETURN: u32 = 0x03E0_0008;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActorRecord {
    /// RAM address of the record.
    pub address: u32,
    /// RAM address of the actor's name in the game's two-byte letters.
    pub name: u32,
    /// The model number. A creature's comes from the species table instead.
    pub model: u32,
    /// RAM address of the bone table, 0 for the package's default
    /// ([`ActorDefaults::bone_table`]). It lies in the package or is the creatures' table of
    /// the executable.
    pub bone_table: u32,
    /// RAM address of the scale of the whole model (three words, 4096 is one), 0 for the
    /// package's default.
    pub model_scale: u32,
    /// RAM address of the bone scales, 0 for none.
    pub bone_scales: u32,
    /// RAM address of a list of numbers ended by a negative one that the spawner hands the
    /// model, 0 for none: the mesh objects it keeps a copy of
    /// ([`ScenePackage::actor_kept`]).
    pub list: u32,
    /// Game frames over which the first action fades in.
    pub fade: u32,
    /// RAM address of the actor's move table (`command::move_table`), which the spawner
    /// keeps with the actor's task; 0 for none. Most records name a place without a list.
    pub word: u32,
    pub turn: i16,
    pub mirror: i16,
}

/// The default actions of an actor record, at +0x20 of it: what the actor moves with when a
/// script orders it to go or turn somewhere and names no action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActorMoves {
    /// The action to go to a place with, for the two kinds of that order.
    pub go: [i16; 2],
    /// The action to turn to a place with.
    pub turn: i16,
    /// The turn per pass of the actor's command list, 4096 to the circle.
    pub turn_step: i16,
}

/// The creature an actor is, for the few actors that are creatures of the story.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Creature {
    /// Number of the species: the species table of the executable gives the model
    /// (`model::species_model`).
    pub species: u32,
    /// The second word of the pair the creature record is set up from: its top six bits are
    /// levels the creature is given on top of its first ([`Self::level`]). The 26 bits
    /// below are added to the same word of the record and not read further.
    pub word: u32,
}

/// What the set-up of a creature record puts into the word the levels owed are counted in
/// (the top six bits): one level.
const ONE_LEVEL_OWED: u32 = 1 << LEVELS_OWED_SHIFT;
const LEVELS_OWED_SHIFT: u32 = 26;

impl Creature {
    pub fn level(&self) -> u8 {
        (ONE_LEVEL_OWED.wrapping_add(self.word) >> LEVELS_OWED_SHIFT) as u8
    }
}

/// One character a room places: the instance of an actor object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Actor {
    /// The views of the room the actor's object belongs to.
    pub view_mask: u32,
    /// RAM address of the instance.
    pub instance: u32,
    /// RAM address of the package's spawner: the function of the actor's object.
    pub spawner: u32,
    pub record: ActorRecord,
    /// Set for a creature: its model, bone table and scales then do not come from the record.
    pub creature: Option<Creature>,
    /// Where the actor stands.
    pub place: [i32; 3],
    /// The turn about the upright axis, 4096 to the circle.
    pub angle: i16,
    /// The action of the model's action table the actor starts in.
    pub action: i32,
    /// The animation set of the model the scene needs, `None` for none.
    pub set: Option<u32>,
    /// Bits 0, 1 and 3 change the flag word of the new actor (bit 0 sets `0x2000`, which
    /// hides it until a script shows it; bit 1 sets `0x0100_0000`, bit 3 clears `0x4_0000`),
    /// bit 2 is handed to the actor maker. What the other two flags do is not read.
    pub flags: u32,
}

/// What a package's spawner takes for an actor record that leaves a field empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActorDefaults {
    /// RAM address of the package's own bone table: the people's.
    pub bone_table: u32,
    /// RAM address of the scale of the model that goes with it.
    pub model_scale: u32,
}

impl<'a> ScenePackage<'a> {
    /// The size word and the code image: offset `o` is RAM address [`PACKAGE_BASE`]` + o`,
    /// which is the form the readers of other modules take an image in.
    pub fn image(&self) -> &'a [u8] {
        self.image
    }

    /// The actor an object of type [`OBJECT_ACTOR`] stands for.
    pub fn actor(&self, object: &ObjectRecord) -> Option<Actor> {
        if object.kind != OBJECT_ACTOR {
            return None;
        }
        let bytes = self.bytes_at(object.data, INSTANCE_SIZE)?;
        let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().expect("4 bytes"));
        let record = self.actor_record(word(0))?;
        let creature = self.bytes_at(word(4), 8).map(|pair| Creature {
            species: u32::from_le_bytes(pair[..4].try_into().expect("4 bytes")),
            word: u32::from_le_bytes(pair[4..].try_into().expect("4 bytes")),
        });
        Some(Actor {
            view_mask: object.view_mask,
            instance: object.data,
            spawner: object.function,
            record,
            creature,
            place: [word(8) as i32, word(12) as i32, word(16) as i32],
            angle: word(20) as u16 as i16,
            action: word(24) as i32,
            set: u32::try_from(word(28) as i32).ok(),
            flags: word(32),
        })
    }

    /// The actor record at a RAM address of the package.
    pub fn actor_record(&self, address: u32) -> Option<ActorRecord> {
        let bytes = self.bytes_at(address, ACTOR_RECORD_SIZE)?;
        let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().expect("4 bytes"));
        Some(ActorRecord {
            address,
            name: word(0),
            model: word(4),
            bone_table: word(8),
            model_scale: word(12),
            bone_scales: word(16),
            list: word(20),
            fade: word(24),
            word: word(28),
            turn: (self.bytes_at(address.checked_add(ACTOR_TURN)?, 2))
                .map_or(0, |turn| i16::from_le_bytes([turn[0], turn[1]])),
            mirror: (self.bytes_at(address.checked_add(ACTOR_MIRROR)?, 2))
                .map_or(0, |word| i16::from_le_bytes([word[0], word[1]])),
        })
    }

    /// The name of an actor record as the units of a text: what a message window shows
    /// as the speaker's name. `None` for a name that does not lie in the package.
    pub fn actor_name(&self, record: &ActorRecord) -> Option<Vec<u16>> {
        let start = record.name.checked_sub(PACKAGE_BASE)? as usize;
        (start >= SIZE_WORD).then_some(())?;
        Some(crate::text::units(self.image.get(start..)?))
    }

    pub fn actor_moves(&self, record: &ActorRecord) -> Option<ActorMoves> {
        let bytes = self.bytes_at(record.address.checked_add(ACTOR_MOVES)?, 8)?;
        let value = |at: usize| i16::from_le_bytes([bytes[at], bytes[at + 1]]);
        Some(ActorMoves {
            go: [value(0), value(2)],
            turn: value(4),
            turn_step: value(6),
        })
    }

    pub fn actor_kept(&self, record: &ActorRecord) -> Vec<usize> {
        let number = |entry: u32| {
            let bytes = self.bytes_at(record.list.checked_add(4 * entry)?, 4)?;
            usize::try_from(i32::from_le_bytes(bytes.try_into().expect("4 bytes"))).ok()
        };
        match record.list {
            0 => Vec::new(),
            _ => (0..).map_while(number).collect(),
        }
    }

    /// The actors of every object list of the package, in file order, each instance once.
    /// A package with several lists picks one by story state, so not all of them need stand
    /// in the room at once.
    pub fn actors(&self) -> Vec<Actor> {
        let mut actors: Vec<Actor> = Vec::new();
        for object in self.object_lists().iter().flat_map(|list| &list.objects) {
            if let Some(actor) = self.actor(object)
                && actors.iter().all(|known| known.instance != actor.instance)
            {
                actors.push(actor);
            }
        }
        actors
    }

    pub fn actors_of(&self, list: &ObjectList) -> Vec<Actor> {
        (list.objects.iter())
            .filter_map(|object| self.actor(object))
            .collect()
    }

    pub fn noted_models(
        &self,
        list: &ObjectList,
        model: impl Fn(&Actor) -> Option<u32>,
    ) -> Vec<u32> {
        let noted = self.noted_actors(list, model);
        noted.into_iter().map(|(model, _)| model).collect()
    }

    /// [`ScenePackage::noted_models`] with the actor each model is noted for: the first of
    /// the list that has it. What the entry routine notes with a model (the turn of its
    /// colours, `model::actor_turn`) is that actor's; a later actor of the same model
    /// changes nothing.
    pub fn noted_actors(
        &self,
        list: &ObjectList,
        model: impl Fn(&Actor) -> Option<u32>,
    ) -> Vec<(u32, Actor)> {
        let mut noted: Vec<(u32, Actor)> = Vec::new();
        for object in &list.objects {
            let Some((model, actor)) =
                (self.actor(object)).and_then(|actor| model(&actor).map(|model| (model, actor)))
            else {
                continue;
            };
            if noted.iter().any(|(known, _)| *known == model) {
                continue;
            }
            if noted.len() == MODEL_SLOTS {
                break;
            }
            noted.push((model, actor));
        }
        noted
    }

    /// The defaults of the spawner at a RAM address: the first two addresses of the package
    /// that its code builds, which are the bone table and the scale of the model in every
    /// package (the spawner is the same code in all of them).
    pub fn actor_defaults(&self, spawner: u32) -> Option<ActorDefaults> {
        self.bytes_at(spawner, 4)?;
        let start = (spawner - PACKAGE_BASE) as usize;
        let end = (start..self.image.len().saturating_sub(3))
            .step_by(4)
            .find(|offset| self.u32_at(*offset) == Some(RETURN))?;
        let mut inside = self
            .built_addresses(start, end)
            .into_iter()
            .map(|(_, address)| address)
            .filter(|address| self.bytes_at(*address, 4).is_some());
        Some(ActorDefaults {
            bone_table: inside.next()?,
            model_scale: inside.next()?,
        })
    }

    /// The three words of a scale of the whole model at a RAM address of the package.
    pub fn model_scale(&self, address: u32) -> Option<[i32; 3]> {
        let bytes = self.bytes_at(address, 12)?;
        let word = |at: usize| i32::from_le_bytes(bytes[at..at + 4].try_into().expect("4 bytes"));
        Some([word(0), word(4), word(8)])
    }

    pub fn bone_scales(&self, address: u32, count: usize) -> Option<Vec<BoneScale>> {
        let bytes = self.bytes_at(address, 8 * count)?;
        let records = bytes.as_chunks::<8>().0.iter().map(|record| {
            let word = |at: usize| i16::from_le_bytes([record[at], record[at + 1]]);
            BoneScale {
                scale: [word(0), word(2), word(4)],
                used: word(6),
            }
        });
        Some(records.collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bits_of_a_room_are_asked_set_and_cleared_by_mask() {
        let command = |code, a, b| Command {
            address: 0,
            code,
            a,
            b,
            c: 0,
        };
        assert_eq!(
            command(0x22, 0, 2).room_bits(),
            Some(RoomBits::WaitFor { bits: 2 })
        );
        assert_eq!(command(0x21, 0, 2).room_bits(), None);
        let mut word = 0;
        let ask = command(0x22, 1, 3).room_bits().expect("an ask");
        assert_eq!(ask.run(&mut word), Some(1));
        let set = command(0x23, 0, 1).room_bits().expect("a set");
        assert_eq!(set.run(&mut word), None);
        // One of the two bits is not all of the mask.
        assert_eq!((word, ask.run(&mut word)), (1, Some(1)));
        assert!(!RoomBits::all_set(word, 3));
        RoomBits::Set { bits: 6 }.run(&mut word);
        assert_eq!((word, ask.run(&mut word)), (7, Some(0)));
        let clear = command(0x24, 0, 2).room_bits().expect("a clear");
        clear.run(&mut word);
        assert_eq!((word, ask.run(&mut word)), (5, Some(1)));
        assert_eq!(RoomBits::WaitFor { bits: 4 }.run(&mut word), None);
        assert!(RoomBits::all_set(word, 4));
    }

    #[test]
    fn the_test_of_the_asked_bits_answers_none_all_or_some() {
        // Room 36: three questions, the bits 1, 2 and 4.
        assert_eq!(asked(7, 0), 2);
        assert_eq!(asked(7, 7), 0);
        assert_eq!((asked(7, 1), asked(7, 6)), (1, 1));
        // A bit past the mask is not counted, and the first bit is tested without one.
        assert_eq!((asked(7, 8), asked(7, 15)), (2, 0));
        assert_eq!((asked(0, 0), asked(0, 1), asked(1, 3)), (2, 0, 0));
        assert_eq!(asked(u32::MAX, u32::MAX), 0);
    }

    /// Builds a package file from 32-bit words of code image and bytes of room data.
    fn package(code: &[u32], room_data: &[u8]) -> Vec<u8> {
        let mut file = (code.len() as u32 * 4).to_le_bytes().to_vec();
        for word in code {
            file.extend_from_slice(&word.to_le_bytes());
        }
        file.extend_from_slice(room_data);
        file
    }

    /// RAM address of word `index` of the code image.
    fn word_address(index: u32) -> u32 {
        PACKAGE_BASE + 4 + index * 4
    }

    /// The two 16-bit halves of a word, low half first.
    fn halves(low: i16, high: i16) -> u32 {
        (low as u16 as u32) | ((high as u16 as u32) << 16)
    }

    /// The `lui a0` and `addiu a0,a0` that build an address.
    fn names(address: u32) -> [u32; 2] {
        let low = address & 0xFFFF;
        // `addiu` sign-extends, so a low half with the top bit set borrows from the high.
        let high = (address >> 16) + (low >> 15);
        [0x3C04_0000 | (high & 0xFFFF), 0x2484_0000 | low]
    }

    /// A code image with one exit record at word 2 and one object list at word 10.
    fn sample() -> Vec<u32> {
        let function = word_address(0);
        let mut code = vec![0x27BD_FFE8, 0x03E0_0008];
        // Words 2 to 7: the exit record.
        code.extend([100, 0, (-250i32) as u32]);
        code.extend([halves(KEEP_ANGLES, 1), halves(3, 4), halves(1, 10)]);
        // Words 8 and 9: the code that names the list.
        code.extend(names(word_address(10)));
        // Words 10 to 24: an exit, a script without a function, the end record.
        code.extend([0xFFFF_FFFF, OBJECT_EXIT, word_address(2), 0, function]);
        code.extend([0x3, 0x08, word_address(0), 0x20, 0]);
        code.extend([0, OBJECT_END, 0, 0, 0]);
        code
    }

    #[test]
    fn a_list_is_picked_by_the_state_of_the_story() {
        let variable = |number: u32| if number == 11 { 3 } else { 1 };
        let flag = |number: u32| number == 118;
        let pick =
            |file: usize, flags: u32| list_index(file).map(|by| by.pick(variable, flags, flag));
        assert_eq!(pick(ROOM_0_PACKAGE, 0), Some(1));
        assert_eq!(pick(470, 0), Some(3));
        assert_eq!(pick(328, 0), Some(1));
        // File 189 goes by the arena's bit of the global flags alone.
        assert_eq!(pick(189, 0x0800_0000), Some(0));
        assert_eq!(pick(189, 0x4800_0000), Some(1));
        assert_eq!(pick(308, !0), None);
        // A story variable is a byte.
        let by = ListIndex::StoryVariable(0);
        assert_eq!(by.pick(|_| 0x102, 0, |_| false), 2);
    }

    #[test]
    fn what_goes_with_a_list() {
        assert_eq!(again_routine(ROOM_0_PACKAGE), Some((ROOM_0_AGAIN, 0)));
        assert_eq!(again_routine(467).map(|(_, variable)| variable), Some(11));
        assert_eq!(again_routine(189), None);
        assert_eq!(list_player_set(ROOM_0_PACKAGE, 0), Some(3));
        assert_eq!(list_player_set(ROOM_0_PACKAGE, 1), Some(4));
        assert_eq!(list_player_set(189, 0), None);
        // File 189 asks for the view of the list's number without a table.
        let file = package(&[1, 2, 3], &[]);
        let scene = ScenePackage::parse(&file).unwrap();
        assert_eq!(list_view(189, 1, &scene), Some(1));
        assert_eq!(list_view(189, 2, &scene), None);
        assert_eq!(list_view(ROOM_0_PACKAGE, 0, &scene), None);
        // A table that the package does not hold answers nothing.
        assert_eq!(list_view(433, 1, &scene), None);
    }

    #[test]
    fn splits_code_and_room_data() {
        let file = package(&[1, 2, 3], &[9, 8, 7]);
        let scene = ScenePackage::parse(&file).unwrap();
        assert_eq!(scene.code().len(), 12);
        assert_eq!(scene.code_address(), 0x800B_2490);
        assert_eq!(scene.room_data(), [9, 8, 7]);
        assert_eq!(scene.bytes_at(word_address(1), 4), Some(&[2, 0, 0, 0][..]));
        // Not the size word, not the room data.
        assert_eq!(scene.bytes_at(PACKAGE_BASE, 4), None);
        assert_eq!(scene.bytes_at(word_address(2), 5), None);
        assert_eq!(scene.bytes_at(PACKAGE_BASE - 4, 4), None);
    }

    #[test]
    fn rejects_a_short_file() {
        assert_eq!(ScenePackage::parse(&[1, 0]), Err(SceneError::Truncated));
        let mut file = package(&[1, 2], &[]);
        file.pop();
        assert_eq!(ScenePackage::parse(&file), Err(SceneError::Truncated));
        let huge = u32::MAX.to_le_bytes();
        assert_eq!(ScenePackage::parse(&huge), Err(SceneError::Truncated));
    }

    #[test]
    fn finds_the_object_list() {
        let file = package(&sample(), &[]);
        let scene = ScenePackage::parse(&file).unwrap();
        let lists = scene.object_lists();
        assert_eq!(lists.len(), 1);
        assert_eq!(lists[0].address, word_address(10));
        assert_eq!(lists[0].objects.len(), 2);
        assert_eq!(
            lists[0].objects[1],
            ObjectRecord {
                address: word_address(15),
                view_mask: 3,
                kind: 0x08,
                data: word_address(0),
                extra: 0x20,
                function: 0,
            }
        );
    }

    #[test]
    fn a_run_without_an_end_record_is_not_a_list() {
        let mut code = sample();
        code.truncate(20);
        let file = package(&code, &[]);
        assert!(
            ScenePackage::parse(&file)
                .unwrap()
                .object_lists()
                .is_empty()
        );

        // Neither is an end record on its own.
        let file = package(&[0, OBJECT_END, 0, 0, 0], &[]);
        assert!(
            ScenePackage::parse(&file)
                .unwrap()
                .object_lists()
                .is_empty()
        );
    }

    #[test]
    fn records_with_outside_pointers_are_not_objects() {
        for (word, value) in [(12, PACKAGE_BASE), (14, 0x8001_0000), (13, MAX_EXTRA)] {
            let mut code = sample();
            code[word] = value;
            let file = package(&code, &[]);
            let lists = ScenePackage::parse(&file).unwrap().object_lists();
            // The run from the second record on has the shape, but nothing names it.
            assert!(lists.is_empty(), "word {word}");
        }
    }

    #[test]
    fn a_list_is_found_only_where_the_package_names_it() {
        // Not named at all.
        let mut code = sample();
        code[8] = 0xDEAD_BEEF;
        let file = package(&code, &[]);
        assert!(
            ScenePackage::parse(&file)
                .unwrap()
                .object_lists()
                .is_empty()
        );

        // Named from its second record: by code with a negative low half, and by a word.
        let high_address = PACKAGE_BASE + 0x8000;
        assert_eq!(names(high_address), [0x3C04_800C, 0x2484_A48C]);
        for naming in [[0x3C04_800B, 0x3484_24CC], [0, word_address(15)]] {
            let mut code = sample();
            code[8..10].copy_from_slice(&naming);
            let file = package(&code, &[]);
            let lists = ScenePackage::parse(&file).unwrap().object_lists();
            assert_eq!(lists.len(), 1, "{naming:x?}");
            assert_eq!(lists[0].address, word_address(15), "{naming:x?}");
        }
    }

    #[test]
    fn a_numbered_object_holds_a_number_as_data() {
        let mut code = sample();
        // The second record becomes a numbered object; any other type needs a pointer.
        code[16] = OBJECT_NUMBERED;
        code[17] = 2;
        code[19] = word_address(0);
        let file = package(&code, &[]);
        let lists = ScenePackage::parse(&file).unwrap().object_lists();
        assert_eq!(lists[0].objects.len(), 2);
        assert_eq!(lists[0].objects[1].data, 2);

        code[16] = 0x0C;
        let file = package(&code, &[]);
        assert!(
            ScenePackage::parse(&file)
                .unwrap()
                .object_lists()
                .is_empty()
        );
    }

    #[test]
    fn a_view_on_screen_ends_and_makes_the_objects_of_single_views() {
        // An object of every view is made with the room and never again or ended.
        assert!(view_makes(EVERY_VIEW, 0, None));
        assert!(!view_makes(EVERY_VIEW, 5, Some(0)));
        assert!(!view_ends(EVERY_VIEW, 5));
        // An object of view 5 alone: not made in view 0, made when view 5 comes, ended
        // when another view does, and made anew when view 5 comes back.
        let island = 1 << 5;
        assert!(!view_makes(island, 0, None));
        assert!(view_ends(island, 0));
        assert!(view_makes(island, 5, Some(0)));
        assert!(!view_ends(island, 5));
        assert!(view_ends(island, 1));
        assert!(view_makes(island, 5, Some(1)));
        // An object of two views stays through a change between them.
        let both = 1 << 2 | 1 << 3;
        assert!(view_makes(both, 2, None));
        assert!(!view_ends(both, 3) && !view_makes(both, 3, Some(2)));
    }

    #[test]
    fn decodes_the_exit_record() {
        let file = package(&sample(), &[]);
        let scene = ScenePackage::parse(&file).unwrap();
        assert_eq!(
            scene.exits(),
            [Exit {
                record: scene.object_lists()[0].objects[0].data,
                view_mask: 0xFFFF_FFFF,
                arrival: [100, 0, -250],
                angle: KEEP_ANGLES,
                leave_flag: 1,
                region: 3,
                area: 4,
                place: 1,
                slot: 10,
            }]
        );
        // The script object is not an exit.
        let script = scene.object_lists()[0].objects[1];
        assert_eq!(scene.exit(&script), None);
    }

    #[test]
    fn an_exit_turns_the_player_by_its_angle() {
        let file = package(&sample(), &[]);
        let mut exit = ScenePackage::parse(&file).unwrap().exits()[0];
        assert!(exit.stands_again());
        assert_eq!(exit.facing(1000), 1000);
        exit.angle = 1672;
        exit.leave_flag = 0;
        assert!(!exit.stands_again());
        assert_eq!(exit.facing(1000), 2672);
        // The sum is a 16-bit one, as the game stores it.
        assert_eq!(exit.facing(0x7000), (0x7000u16 + 1672) as i16);
    }

    /// A code image with a three-command script at word 0, a word that is no command, an
    /// object list with the script at word 13, and a one-entry table of list pointers.
    fn script_sample() -> Vec<u32> {
        let mut code = vec![0x53, 0, 340, 0];
        code.extend([0x51, 4, halves(7, 2), (-1i32) as u32]);
        code.extend([COMMAND_END, 0, 0, 0]);
        code.push(COMMAND_COUNT);
        code.extend([0x5, OBJECT_SCRIPT, word_address(0), 0, 0]);
        code.extend([0, OBJECT_END, 0, 0, 0]);
        code.push(word_address(13));
        code
    }

    #[test]
    fn decodes_the_script_of_an_object_list() {
        let file = package(&script_sample(), &[]);
        let scene = ScenePackage::parse(&file).unwrap();
        let scripts = scene.scripts();
        assert_eq!(scripts.len(), 1);
        assert_eq!(scripts[0].address, word_address(0));
        assert_eq!(scripts[0].view_mask, 5);
        let commands = &scripts[0].commands;
        assert_eq!(commands.len(), 3);
        assert_eq!(
            commands[1],
            Command {
                address: word_address(4),
                code: 0x51,
                a: 4,
                b: 0x0002_0007,
                c: 0xFFFF_FFFF,
            }
        );
        let typed: Vec<_> = commands.iter().map(Command::location).collect();
        assert_eq!(
            typed,
            [
                Some(LocationCommand::GoToRoom { room: 340 }),
                Some(LocationCommand::SetVariant {
                    area: 4,
                    place: 2,
                    slot: 7,
                    room: -1,
                }),
                None,
            ]
        );
        // An exit object is not a script.
        let file = package(&sample(), &[]);
        let scene = ScenePackage::parse(&file).unwrap();
        assert_eq!(scene.script(&scene.object_lists()[0].objects[0]), None);
    }

    #[test]
    fn a_script_stops_at_the_image_end() {
        // Two whole commands and half of a third.
        let code = [0x58, 0, 0, 0, 0x4F, 0, 2, 0, 0x50, 0];
        let file = package(&code, &[0; 64]);
        let scene = ScenePackage::parse(&file).unwrap();
        let commands = scene.commands_at(word_address(0));
        let typed: Vec<_> = commands.iter().map(Command::location).collect();
        assert_eq!(
            typed,
            [
                Some(LocationCommand::OpenPlaceMap),
                Some(LocationCommand::StartChapter { chapter: 2 }),
            ]
        );
        assert!(scene.commands_at(PACKAGE_BASE - 16).is_empty());
    }

    #[test]
    fn a_segment_stops_at_the_end_command_and_jumps_are_followed() {
        // Word 0: jump to word 16, end, then a command the script never reaches.
        let mut code = vec![COMMAND_JUMP, 0, 0, word_address(16)];
        code.extend([COMMAND_END, 0, 0, 0]);
        code.extend([0x58, 0, 0, 0]);
        code.extend([0xDEAD_BEEF; 4]);
        // Word 16: label, go to room, a jump back to word 0, a jump out of the image, end.
        code.extend([COMMAND_LABEL, 0, 1, 0]);
        code.extend([0x53, 0, 340, 0]);
        code.extend([COMMAND_JUMP, 0, 0, word_address(0)]);
        code.extend([COMMAND_JUMP, 0, 0, 0x8001_0000]);
        code.extend([COMMAND_END, 0, 0, 0]);
        let file = package(&code, &[]);
        let scene = ScenePackage::parse(&file).unwrap();

        let codes = |commands: Vec<Command>| commands.iter().map(|c| c.code).collect::<Vec<_>>();
        assert_eq!(
            codes(scene.commands_at(word_address(0))),
            [COMMAND_JUMP, COMMAND_END]
        );
        let all = scene.commands_from(word_address(0));
        assert_eq!(all[2].address, word_address(16));
        assert_eq!(
            codes(all),
            [
                COMMAND_JUMP,
                COMMAND_END,
                COMMAND_LABEL,
                0x53,
                COMMAND_JUMP,
                COMMAND_JUMP,
                COMMAND_END
            ]
        );
    }

    #[test]
    fn types_the_remaining_location_commands() {
        let command = |code, a, b, c| Command {
            address: 0,
            code,
            a,
            b,
            c,
        };
        assert_eq!(
            command(0x52, 1, 0x0003_0002, 0).location(),
            Some(LocationCommand::GoToSlot {
                area: 1,
                place: 3,
                slot: 2,
                arrival: None,
            })
        );
        assert_eq!(
            command(0x52, 1, 0, word_address(9)).location(),
            Some(LocationCommand::GoToSlot {
                area: 1,
                place: 0,
                slot: 0,
                arrival: Some(word_address(9)),
            })
        );
        assert_eq!(
            command(0x50, 0, 6, 0).location(),
            Some(LocationCommand::SetStateWord { value: 6 })
        );
        assert_eq!(command(0x4C, 11, 0, 0).location(), None);
    }

    #[test]
    fn types_and_runs_the_commands_of_the_players_things() {
        let keeping = |code, a, b| {
            let command = Command {
                address: 0,
                code,
                a,
                b,
                c: 0,
            };
            command.keeping()
        };
        assert_eq!(
            keeping(0x1F, 0, 30),
            Some(KeepingCommand::WaitForFlag { flag: 30 })
        );
        assert_eq!(keeping(0x2C, 0, 1), None);
        assert_eq!(
            keeping(0x29, 0, 5),
            Some(KeepingCommand::StoryThingUnread { value: 5 })
        );

        let mut has = Keeping::new();
        let mut run = |code, a, b| keeping(code, a, b).unwrap().run(&mut has, [255; 3]);
        // A flag that is clear and a thing that is missing give the choice 1.
        assert_eq!(run(0x1F, 1, 30), Some(1));
        assert_eq!(run(0x20, 0, 30), None);
        assert_eq!(run(0x1F, 1, 30), Some(0));
        assert_eq!(run(0x21, 0, 30), None);
        assert_eq!(run(0x1F, 1, 30), Some(1));
        assert_eq!(run(0x29, 1, 3), Some(1));
        assert_eq!(run(0x2D, 1, 3), None);
        assert_eq!(run(0x29, 1, 3), Some(0));
        assert_eq!(run(0x2D, 0, 3), None);
        assert_eq!(run(0x29, 1, 3), Some(1));
        // Number first, then the count; the waiting forms change nothing.
        for (code, a, b) in [(0x25, 14, 3), (0x26, 14, 1), (0x27, 1, 2), (0x28, 1, 1)] {
            assert_eq!(run(code, a, b), None);
        }
        assert_eq!((run(0x2A, 0, 200), run(0x2B, 0, 12)), (None, None));
        assert_eq!((run(0x1F, 0, 31), run(0x29, 0, 4)), (None, None));
        assert_eq!(has.count(14, Kind::Item), 2);
        assert_eq!(has.count(1, Kind::Weapon), 1);
        assert_eq!((has.money(), has.cocoons()), (200, 12));
        assert!(!has.flag(31) && !has.has_story_thing(4));
    }

    #[test]
    fn a_list_of_lines_is_read_up_to_its_end() {
        // Words 0 and 1: two lines of two and one units. Words 2 to 4: the list. Words 5
        // to 13: a list without an end.
        let mut code = vec![halves(0x4182, 0x4282), 0, halves(0x4382, 0)];
        code.extend([word_address(2), word_address(0), 0]);
        code.extend([word_address(0); 9]);
        let file = package(&code, &[]);
        let scene = ScenePackage::parse(&file).unwrap();
        assert_eq!(
            scene.lines_at(word_address(3)),
            Some(vec![vec![0x4382], vec![0x4182, 0x4282]])
        );
        assert_eq!(scene.lines_at(word_address(5)), Some(Vec::new()));
        assert_eq!(scene.lines_at(word_address(6)), None);
        assert_eq!(scene.lines_at(PACKAGE_BASE - 8), None);
    }

    #[test]
    fn a_notice_command_names_its_things() {
        // Words 0 to 3: a list of two things. Word 4 on: the commands.
        let mut code = vec![halves(5, 3), 1, halves(0, 12), 2];
        code.extend([0x3C, 2, 1, word_address(0)]);
        code.extend([0x3D, 1, 4, 0]);
        code.extend([0x3E, 7, 2, 1]);
        code.extend([0x3E, (-1i32) as u32, 6, 0]);
        code.extend([0x3B, 0, 0, 3]);
        code.extend([0x3C, 9, 1, word_address(0)]);
        code.extend([COMMAND_END, 0, 0, 0]);
        let file = package(&code, &[]);
        let scene = ScenePackage::parse(&file).unwrap();
        let notices: Vec<_> = (scene.commands_at(word_address(4)).iter())
            .map(|command| scene.notice(command))
            .collect();
        let thing = |kind, number, count| NoticeThing {
            kind,
            number,
            count,
        };
        let notice = |way, things| Some(NoticeCommand { way, things });
        let listed = vec![thing(5, 3, 1), thing(0, 12, 2)];
        assert_eq!(notices[0], notice(1, Some(listed)));
        // No list and no number: the things of the thing the task belongs to.
        assert_eq!(notices[1], notice(4, None));
        assert_eq!(notices[2], notice(2, Some(vec![thing(1, 7, 1)])));
        assert_eq!(notices[3], notice(6, None));
        assert_eq!(notices[4], notice(11, Some(Vec::new())));
        // Three things at most, and here the third would lie in the commands: still read,
        // as the game reads it.
        assert_eq!(
            notices[5].as_ref().unwrap().things.as_ref().unwrap().len(),
            3
        );
        assert_eq!(notices[6], None);

        let mut has = Keeping::new();
        for thing in notices[0].as_ref().unwrap().things.as_ref().unwrap() {
            has.notice_ends(1, thing.kind, thing.number, thing.count, [255; 3]);
        }
        assert!(has.has_story_thing(3));
        assert_eq!(has.count(12, Kind::Item), 2);
    }

    #[test]
    fn an_exit_record_cut_off_by_the_image_end_is_dropped() {
        let mut code = sample();
        // Point the exit object at the last word of the image.
        code[12] = word_address(24);
        let file = package(&code, &[0; 64]);
        assert!(ScenePackage::parse(&file).unwrap().exits().is_empty());
    }

    /// A code image with the show routines (starter at word 0, spawner at word 7, table at
    /// word 12), a script that hands the spawner a list, and the list of one record.
    fn show_sample() -> Vec<u32> {
        let mut code = vec![0x27BD_FFE8];
        code.extend(names(word_address(12)));
        code.extend([jump_and_link(ROOM_RESOURCE), 0, 0x03E0_0008, 0]);
        code.push(0x27BD_FFD8);
        code.extend(names(word_address(0)));
        code.extend([0x03E0_0008, 0]);
        code.extend([0, word_address(0), 0, 0, 0]);
        // Words 17 to 28: the script.
        code.extend([COMMAND_ARGUMENT, 0, 0, word_address(29)]);
        code.extend([COMMAND_NATIVE, 0, 0, word_address(7)]);
        code.extend([COMMAND_END, 0, 0, 0]);
        // Words 29 to 39: a show record, then the record that ends the list.
        code.extend([halves(1, 81), 10, (-20i32) as u32, 30]);
        code.extend([halves(1024, 0), halves(-512, 0), halves(120, 30)]);
        code.extend([halves(-1, 2), halves(0, 9), halves(48, 0), word_address(7)]);
        code.push(halves(SHOW_END, 0));
        code.extend([0; 10]);
        code
    }

    #[test]
    fn finds_the_show_routines_and_the_lists_of_the_scripts() {
        let file = package(&show_sample(), &[]);
        let scene = ScenePackage::parse(&file).unwrap();
        assert_eq!(
            scene.show_routines(),
            Some(ShowRoutines {
                starter: word_address(0),
                table: Some(word_address(12)),
                entries: [0, word_address(0), 0, 0, 0],
                spawner: word_address(7),
            })
        );
        // The same routines with a starter that fills its table on the stack: the frame's
        // five words from `sp + 16` are cleared and the sprite routine is stored in the first.
        let mut code = vec![
            0x27BD_FFC8,
            0x27A4_0010,
            0x2406_0014,
            jump_and_link(FILL),
            0,
        ];
        code.extend(names(word_address(8)));
        code.extend([0xAFA4_0010, jump_and_link(ROOM_RESOURCE), 0, 0x03E0_0008, 0]);
        code.push(0x27BD_FFD8);
        code.extend(names(word_address(0)));
        code.extend([0x03E0_0008, 0, 0, 0]);
        let on_stack = package(&code, &[]);
        assert_eq!(
            ScenePackage::parse(&on_stack).unwrap().show_routines(),
            Some(ShowRoutines {
                starter: word_address(0),
                table: None,
                entries: [word_address(8), 0, 0, 0, 0],
                spawner: word_address(12),
            })
        );
        let lists = scene.show_lists();
        assert_eq!(lists.len(), 1);
        assert_eq!(lists[0].address, word_address(29));
        assert_eq!(
            lists[0].shows,
            [Show {
                way: 1,
                number: 81,
                place: [10, -20, 30],
                angles: [1024, 0, -512],
                length: 120,
                wait: 30,
                runs: -1,
                first: 2,
                restart: 0,
                last: 9,
                mode: 48,
                mover: word_address(7),
            }]
        );
        assert_eq!(lists[0].shows[0].shown(), Some(ShowWay::Model));
    }

    #[test]
    fn finds_the_routines_that_read_animations() {
        // Actor 1's model is asked for and handed to the loader of one animation with
        // number 18 (words 0 to 7), then to the loader of a set (words 8 to 14); a routine
        // that asks for no model is none (words 15 to 19).
        let mut code = vec![0x27BD_FFE0, 0x2404_0001, jump_and_link(ACTOR_MODEL), 0];
        code.extend([0x2405_0012, jump_and_link(ANIMATION_LOADER), 0, 0x03E0_0008]);
        code.extend([0x27BD_FFE8, jump_and_link(ACTOR_MODEL), 0x2404_0001]);
        code.extend([0x8E05_0004, jump_and_link(SET_LOADER), 0, 0x03E0_0008]);
        code.extend([0x27BD_FFE8, jump_and_link(SET_LOADER), 0, 0x03E0_0008, 0]);
        let file = package(&code, &[]);
        assert_eq!(
            ScenePackage::parse(&file).unwrap().read_routines(),
            [
                ReadRoutine {
                    address: word_address(0),
                    actor: 1,
                    reads: Reads::Animation(18),
                },
                ReadRoutine {
                    address: word_address(8),
                    actor: 1,
                    reads: Reads::Set,
                },
            ]
        );
    }

    #[test]
    fn reads_the_musics_an_entry_routine_asks_for_by_the_view() {
        // The table is built and copied, the wanted view read, the music asked for and
        // noted; the nine words lie behind the code.
        let table = word_address(12);
        let high = (table >> 16) + ((table & 0xFFFF) >> 15);
        let mut code = vec![
            0x27BD_FFB8,
            0x3C02_0000 | high,
            0x2442_0000 | (table & 0xFFFF),
        ];
        code.extend([VIEW_MUSIC_END, 0, WANTED_VIEW_READ, 0]);
        code.extend([jump_and_link(ASK_MUSIC), 0, jump_and_link(NOTE_MUSIC), 0, 0]);
        code.extend([22, 22, 22, 22, 22, 17, 26, 32, 31]);
        let file = package(&code, &[]);
        let package = ScenePackage::parse(&file).unwrap();
        assert_eq!(
            package.view_musics(word_address(0)),
            Some([22, 22, 22, 22, 22, 17, 26, 32, 31])
        );
        // A routine that asks for no music, and an address outside the package.
        assert_eq!(package.view_musics(word_address(8)), None);
        assert_eq!(package.view_musics(PACKAGE_BASE - 4), None);
        // This one stores no room behind the two calls.
        assert_eq!(package.entry_stored_room(word_address(0)), None);
    }

    #[test]
    fn reads_the_room_an_entry_routine_stores_behind_its_music() {
        let mut code = vec![0x27BD_FFB8, 0];
        code.extend([jump_and_link(ASK_MUSIC), 0, jump_and_link(NOTE_MUSIC), 0]);
        code.extend([jump_and_link(STORE_ROOM), 0x2404_0068, 0]);
        let file = package(&code, &[]);
        let package = ScenePackage::parse(&file).unwrap();
        assert_eq!(package.entry_stored_room(word_address(0)), Some(104));
        assert_eq!(package.entry_stored_room(word_address(4)), None);
        assert_eq!(package.entry_stored_room(PACKAGE_BASE - 4), None);
    }

    #[test]
    fn finds_the_routines_that_fade_the_kept_sound_record() {
        // Nineteen instructions with the telling ones among them, then the same with
        // another call in the place of the fade: only the first is such a routine.
        let routine = |call: u32| {
            let mut code = vec![FADE_STACK, 0x3C02_8009, 0, 0x2450_1CD0, 0, 0x8605_0004];
            code.extend([0, 0, 0x2406_007F, 0, call, 0x2407_0078, 0, 0xA602_0004]);
            code.extend([0, 0, ANSWER_ONE, RETURN, 0x27BD_0018]);
            code
        };
        let mut code = routine(jump_and_link(SEQUENCE_FADE));
        assert_eq!(code.len(), FADE_LENGTH);
        code.extend(routine(jump_and_link(PLAYER_SET_LOADER)));
        code.push(0);
        let file = package(&code, &[]);
        assert_eq!(
            ScenePackage::parse(&file).unwrap().fade_routines(),
            [word_address(0)]
        );
    }

    #[test]
    fn finds_the_routines_that_name_the_kept_set() {
        // An entry routine stores a number in the kept word and calls the loader (words 0
        // to 5): no such routine. Then one that keeps set 11 and one that keeps none.
        let mut code = vec![0x27BD_FFE8, 0x3C03_8009, 0x2402_000A, 0xAC62_1C04];
        code.extend([jump_and_link(PLAYER_SET_LOADER), 0]);
        code.extend([
            0x3C03_8009,
            0x2402_000B,
            0xAC62_1C04,
            0x03E0_0008,
            0x2402_0001,
        ]);
        code.extend([
            0x3C03_8009,
            0x2402_FFFF,
            0xAC62_1C04,
            0x03E0_0008,
            0x2402_0001,
            0,
        ]);
        let file = package(&code, &[]);
        assert_eq!(
            ScenePackage::parse(&file).unwrap().kept_set_routines(),
            [(word_address(6), 11), (word_address(11), -1)]
        );
    }

    #[test]
    fn finds_the_routines_that_put_something_on_the_player() {
        // A routine that calls the equip routine and answers something else (words 0 to
        // 8): no such routine. Then armour 0, and weapon 3.
        let call = jump_and_link(EQUIP);
        let mut code = vec![0x27BD_FFE8, 0x0000_2021, 0xAFBF_0010, call, 0x2405_0001];
        code.extend([0x8FBF_0010, 0x2402_0000, RETURN, 0x27BD_0018]);
        code.extend([0x27BD_FFE8, 0x0000_2021, 0xAFBF_0010, call, 0x2405_0001]);
        code.extend([0x8FBF_0010, ANSWER_ONE, RETURN, 0x27BD_0018]);
        code.extend([0x27BD_FFE8, 0x2404_0003, 0xAFBF_0010, call, 0x2405_0000]);
        code.extend([0x8FBF_0010, ANSWER_ONE, RETURN, 0x27BD_0018, 0]);
        let file = package(&code, &[]);
        let routine = |word, item, kind| EquipRoutine {
            address: word_address(word),
            item,
            kind,
        };
        assert_eq!(
            ScenePackage::parse(&file).unwrap().equip_routines(),
            [routine(9, 0, 1), routine(18, 3, 0)]
        );
    }

    #[test]
    fn finds_the_movie_routine_and_the_movies_of_the_scripts() {
        // A routine that hands the movie player a word of a table (`lw a0,0(v1)`, words 0
        // to 5), the movie routine (words 6 to 11) and a script.
        let mut code = vec![0x27BD_FFE8, 0x8C64_0000, jump_and_link(MOVIE_PLAYER), 0];
        code.extend([0x03E0_0008, 0]);
        // `lw a0,0(s1)` before the call.
        code.extend([0x27BD_FFE0, 0x8E24_0000, jump_and_link(MOVIE_PLAYER), 0]);
        code.extend([0x03E0_0008, 0]);
        code.extend([COMMAND_RECORD, 11, 0, 0]);
        code.extend([COMMAND_NATIVE, 0, 0, word_address(6)]);
        // A record that is handed to another routine is no movie.
        code.extend([COMMAND_RECORD, 5, 0, 0]);
        code.extend([COMMAND_NATIVE, 0, 0, word_address(0)]);
        code.extend([COMMAND_END, 0, 0, 0]);
        let file = package(&code, &[]);
        let scene = ScenePackage::parse(&file).unwrap();
        assert_eq!(scene.movie_routine(), Some(word_address(6)));
        assert_eq!(scene.script_movies(), [11]);
        // The package of the show routines plays none.
        let file = package(&show_sample(), &[]);
        let scene = ScenePackage::parse(&file).unwrap();
        assert_eq!(
            (scene.movie_routine(), scene.script_movies()),
            (None, vec![])
        );
    }

    #[test]
    fn finds_the_routine_that_takes_the_shown_things_away() {
        // A routine that reads the pool and calls another (words 0 to 5), the routine
        // (words 6 to 11) and a script that calls it twice.
        let mut code = vec![POOL_UPPER, POOL_LOAD, jump_and_link(MOVIE_PLAYER), 0];
        code.extend([LIGHT_CLEAR, RETURN]);
        code.extend([POOL_UPPER, POOL_LOAD, 0, POOL_UPPER, LIGHT_CLEAR, RETURN]);
        code.extend([COMMAND_NATIVE, 0, 0, word_address(6)]);
        code.extend([COMMAND_NATIVE, 0, 0, word_address(0)]);
        code.extend([COMMAND_NATIVE, 0, 0, word_address(6)]);
        code.extend([COMMAND_END, 0, 0, 0]);
        let file = package(&code, &[]);
        let scene = ScenePackage::parse(&file).unwrap();
        assert_eq!(scene.clear_routine(), Some(word_address(6)));
        assert_eq!(scene.clear_calls(), 2);
        // The package of the show routines has none.
        let file = package(&show_sample(), &[]);
        let scene = ScenePackage::parse(&file).unwrap();
        assert_eq!((scene.clear_routine(), scene.clear_calls()), (None, 0));
    }

    /// A code image with a spawner at word 0 that builds an address outside the package and
    /// then its two defaults, an actor record at word 9, a creature pair at word 17, two
    /// instances at words 19 and 28 and, named at word 37, the object list at word 39.
    fn actor_sample() -> Vec<u32> {
        let mut code = vec![0x27BD_FED0];
        code.extend(names(0x8007_9064));
        code.extend(names(word_address(60)));
        code.extend(names(word_address(63)));
        code.extend([RETURN, 0]);
        // Words 9 to 16: the record. Model 47, bone scales, fade 4.
        code.extend([word_address(66), 47, 0, 0, word_address(63), 0, 4, 0]);
        // Words 17 and 18: species 52 and its second word.
        code.extend([52, 0x1000_0000]);
        // Words 19 to 27: an instance without a creature and without a set.
        code.extend([word_address(9), 0, (-137i32) as u32, (-12i32) as u32, 175]);
        code.extend([halves(-1024, -1), 10, u32::MAX, 0x0B]);
        // Words 28 to 36: the same record as a creature, with set 5.
        code.extend([word_address(9), word_address(17), 1, 2, 3]);
        code.extend([halves(1672, 0), 31, 5, 2]);
        code.extend(names(word_address(39)));
        // Words 39 to 58: both instances, the first one a second time, the end record.
        code.extend([
            u32::MAX,
            OBJECT_ACTOR,
            word_address(19),
            0x94,
            word_address(0),
        ]);
        code.extend([1, OBJECT_ACTOR, word_address(28), 0x94, word_address(0)]);
        code.extend([2, OBJECT_ACTOR, word_address(19), 0x94, word_address(0)]);
        code.extend([0, OBJECT_END, 0, 0, 0]);
        code.push(0);
        // Words 60 to 65: stand-ins for the default table and the default scale.
        code.extend([0, 0, 0, 4096, 2048, 4096]);
        // Word 66: the name.
        code.extend([0x8260, 0]);
        code
    }

    #[test]
    fn a_fight_record_names_its_things() {
        // Six words of the fight, then the things of the three entries: 5 is none.
        let mut code = actor_sample();
        let at = code.len() as u32;
        code.extend([1, 1, 0, 0, 0, 0, 3, 5, 5, 12, 0, 0]);
        code.extend([0, 1, 0, 0, 0, 0, 1, 2]);
        let file = package(&code, &[]);
        let scene = ScenePackage::parse(&file).unwrap();
        assert_eq!(
            scene.fight_things(word_address(at)),
            Some([Some(3), None, None])
        );
        // A record that the image ends in, and one outside it.
        assert_eq!(scene.fight_things(word_address(at + 12)), None);
        assert_eq!(scene.fight_things(PACKAGE_BASE - 64), None);
    }

    #[test]
    fn reads_the_actors_of_the_object_lists() {
        let file = package(&actor_sample(), &[]);
        let scene = ScenePackage::parse(&file).unwrap();
        let actors = scene.actors();
        assert_eq!(actors.len(), 2, "an instance in two objects is one actor");
        let record = ActorRecord {
            address: word_address(9),
            name: word_address(66),
            model: 47,
            bone_table: 0,
            model_scale: 0,
            bone_scales: word_address(63),
            list: 0,
            fade: 4,
            word: 0,
            // In this sample the instance follows the record after two words, so the 16
            // bits at +0x28 are the low half of the instance's first word.
            turn: word_address(9) as u16 as i16,
            mirror: (word_address(9) >> 16) as u16 as i16,
        };
        assert_eq!(
            actors[0],
            Actor {
                view_mask: u32::MAX,
                instance: word_address(19),
                spawner: word_address(0),
                record,
                creature: None,
                place: [-137, -12, 175],
                angle: -1024,
                action: 10,
                set: None,
                flags: 0x0B,
            }
        );
        assert_eq!(
            actors[1].creature,
            Some(Creature {
                species: 52,
                word: 0x1000_0000,
            })
        );
        // One level, and four more from the top six bits of the word.
        let level = |word| Creature { species: 0, word }.level();
        assert_eq!(actors[1].creature.map(|creature| creature.level()), Some(5));
        assert_eq!(
            (level(0), level(0x03FF_FFFF), level(0x0400_0000)),
            (1, 1, 2)
        );
        assert_eq!((level(0x2800_0000), level(0xFC00_0000)), (11, 0));
        assert_eq!((actors[1].angle, actors[1].set), (1672, Some(5)));
        assert_eq!(scene.image().len(), file.len());
    }

    #[test]
    fn the_list_of_an_actor_record_names_the_kept_mesh_objects() {
        let mut code = actor_sample();
        let at = code.len();
        code.extend([8, 23, u32::MAX, 5]);
        let file = package(&code, &[]);
        let scene = ScenePackage::parse(&file).unwrap();
        let record = scene.actors()[0].record;
        assert!(scene.actor_kept(&record).is_empty());
        let listed = ActorRecord {
            list: word_address(at as u32),
            ..record
        };
        assert_eq!(scene.actor_kept(&listed), [8, 23]);
    }

    #[test]
    fn the_defaults_of_a_spawner_are_its_first_two_addresses_in_the_package() {
        let file = package(&actor_sample(), &[]);
        let scene = ScenePackage::parse(&file).unwrap();
        let defaults = scene.actor_defaults(word_address(0)).unwrap();
        assert_eq!(
            defaults,
            ActorDefaults {
                bone_table: word_address(60),
                model_scale: word_address(63),
            }
        );
        assert_eq!(
            scene.model_scale(defaults.model_scale),
            Some([4096, 2048, 4096])
        );
        // A routine that builds no address of the package has none, nor has no routine.
        assert_eq!(scene.actor_defaults(word_address(7)), None);
        assert_eq!(scene.actor_defaults(0), None);
        assert_eq!(scene.model_scale(PACKAGE_BASE), None);
        // The same words read as bone scales: 16-bit values, four to a record.
        assert_eq!(
            scene.bone_scales(word_address(63), 1),
            Some(vec![BoneScale {
                scale: [4096, 0, 2048],
                used: 0,
            }])
        );
        assert_eq!(
            scene.bone_scales(word_address(63), 2).map(|s| s.len()),
            Some(2)
        );
        assert_eq!(scene.bone_scales(word_address(63), 1000), None);
    }

    #[test]
    fn an_actor_needs_an_actor_object_with_a_record_in_the_package() {
        let file = package(&actor_sample(), &[]);
        let scene = ScenePackage::parse(&file).unwrap();
        let mut object = scene.object_lists()[0].objects[0];
        assert!(scene.actor(&object).is_some());
        object.kind = OBJECT_SCRIPT;
        assert_eq!(scene.actor(&object), None);
        // A record address outside the package.
        let mut code = actor_sample();
        code[19] = 0x8007_9064;
        let file = package(&code, &[]);
        assert_eq!(ScenePackage::parse(&file).unwrap().actors().len(), 1);
    }

    #[test]
    fn a_show_clock_waits_runs_and_ends() {
        let mut show = Show::parse(&[0; SHOW_SIZE]);
        (show.wait, show.length, show.runs, show.restart) = (2, 3, 1, 1);
        (show.first, show.last) = (0, -1);
        let mut clock = ShowClock::default();
        // Six frames to the thing: two game frames of waiting, a run of three, a second run
        // from the restart value, then nothing more.
        let frames: Vec<Option<usize>> = (0..9)
            .map(|_| {
                let frame = clock.frame(&show, 6);
                clock.step(&show);
                frame
            })
            .collect();
        let shown = [None, None, Some(0), Some(2), Some(4), Some(2), Some(4)];
        assert_eq!(frames[..7], shown);
        assert_eq!(frames[7..], [None, None]);
        assert!(clock.ended);

        // Without an end the runs go on, and a length below zero is the number of frames:
        // such a run is over after one game frame.
        (show.wait, show.runs, show.length, show.restart) = (0, -1, -1, 4);
        let mut clock = ShowClock::default();
        assert_eq!(clock.frame(&show, 6), Some(0));
        clock.step(&show);
        assert_eq!(clock.frame(&show, 6), Some(4));
        clock.step(&show);
        assert_eq!((clock.frame(&show, 6), clock.ended), (Some(4), false));
    }

    #[test]
    fn a_list_needs_the_spawner_and_both_commands() {
        // The native call names another routine.
        let mut code = show_sample();
        code[24] = word_address(0);
        let file = package(&code, &[]);
        assert!(ScenePackage::parse(&file).unwrap().show_lists().is_empty());

        // No command that sets the argument in front of the call.
        let mut code = show_sample();
        code[17] = COMMAND_LABEL;
        let file = package(&code, &[]);
        assert!(ScenePackage::parse(&file).unwrap().show_lists().is_empty());

        // A package without a call of the room resource routine has no show routines.
        let mut code = show_sample();
        code[3] = 0;
        let file = package(&code, &[]);
        let scene = ScenePackage::parse(&file).unwrap();
        assert_eq!(scene.show_routines(), None);
        assert!(scene.show_lists().is_empty());

        // The same list handed over twice is one list.
        let mut code = show_sample();
        let script: Vec<u32> = code[17..25].to_vec();
        code.extend(script);
        let file = package(&code, &[]);
        assert_eq!(ScenePackage::parse(&file).unwrap().show_lists().len(), 1);
    }

    #[test]
    fn a_show_list_ends_at_the_end_record_or_the_image_end() {
        let mut code = show_sample();
        // Without the end record the list runs on to what no longer fits in the image.
        code[40] = halves(3, 22);
        let file = package(&code, &[0; 64]);
        let scene = ScenePackage::parse(&file).unwrap();
        let shows = scene.shows_at(word_address(29));
        assert_eq!(shows.len(), 2);
        assert_eq!(shows[1].shown(), Some(ShowWay::Clip));
        assert_eq!(shows[1].number, 22);
        assert!(scene.shows_at(PACKAGE_BASE).is_empty());
    }

    #[test]
    fn the_frame_of_a_show_spreads_its_stretch_over_a_run() {
        let file = package(&show_sample(), &[]);
        let scene = ScenePackage::parse(&file).unwrap();
        let mut show = scene.shows_at(word_address(29))[0];
        // Frames 2 to 9 in 120 game frames.
        assert_eq!(show.frame(0, 40), Some(2));
        assert_eq!(show.frame(60, 40), Some(6));
        assert_eq!(show.frame(119, 40), Some(9));
        // Past the frames the thing has, the count starts again.
        assert_eq!(show.frame(60, 5), Some(1));
        // Below zero the last frame is the last there is.
        show.last = -1;
        show.first = 0;
        assert_eq!(show.frame(60, 40), Some(20));
        assert_eq!(show.frame(119, 40), Some(39));
        assert_eq!(show.frame(0, 0), None);
        show.length = 0;
        assert_eq!(show.frame(0, 40), None);
        show.way = 2;
        assert_eq!(show.shown(), None);
        show.way = 4;
        assert_eq!(show.shown(), Some(ShowWay::Light));
        show.way = 0;
        assert_eq!(show.shown(), Some(ShowWay::Sprite));
    }
}
